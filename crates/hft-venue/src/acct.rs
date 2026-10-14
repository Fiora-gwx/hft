//! Gateway-owned account-query scheduling. All times are injected counter ticks.
use hft_refdata::MarginData;
use hft_types::{AccountId, MarketKind};
use hft_venue_proto::{AcctHint, AcctQuery, AcctQueryKind};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(C)]
pub struct AcctReq {
    pub kind: AcctQueryKind,
    pub account: AccountId,
    /// Scheduled=1, MarginNow=2, ConfigRecheck=3, PushStale=4, Startup=5.
    pub why: u8,
    /// 0: same credential; 1: verified session UID; 2: session query failed.
    pub session: u8,
    pub seq_no: u32,
    pub poll_no: u32,
    pub _pad: u32,
    pub session_uid: [u8; 24],
}
const _: () = assert!(size_of::<AcctReq>() == 40);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AcctState {
    Idle,
    Running(AcctReq),
    Backoff { until: u64 },
}

#[derive(Debug)]
pub struct AcctSched {
    account: AccountId,
    margin: MarginData,
    margin_interval: u64,
    config_interval: u64,
    second: u64,
    next_margin: u64,
    next_config: u64,
    poll_no: u32,
    seq_no: u32,
    hints: u8,
    startup: bool,
    session: u8,
    uid: [u8; 24],
    fails: u8,
    degraded: bool,
    state: AcctState,
    retry: Option<AcctReq>,
}

impl AcctSched {
    pub fn new(
        account: AccountId,
        market: MarketKind,
        margin: MarginData,
        cfg: &crate::GwCfg,
        ms: impl Fn(u64) -> u64,
    ) -> Self {
        let margin = if market == MarketKind::Spot {
            MarginData::NotApplicable
        } else {
            margin
        };
        let period = match margin {
            MarginData::Poll { interval_ms } => interval_ms.max(cfg.margin_poll_min_ms),
            _ => cfg.margin_push_stale_ms,
        };
        Self {
            account,
            margin,
            margin_interval: ms(u64::from(period)),
            config_interval: ms(u64::from(cfg.acct_cfg_recheck_s) * 1000),
            second: ms(1000),
            next_margin: 0,
            next_config: 0,
            poll_no: 0,
            seq_no: 0,
            hints: 0,
            startup: true,
            session: 0,
            uid: [0; 24],
            fails: 0,
            degraded: false,
            state: AcctState::Idle,
            retry: None,
        }
    }
    pub fn state(&self) -> AcctState {
        self.state
    }
    pub fn hint(&mut self, hint: AcctHint) {
        self.hints |= match hint {
            AcctHint::None => 0,
            AcctHint::MarginNow => 1,
            AcctHint::ConfigRecheck => 2,
        };
    }
    pub fn session(&mut self, status: u8, uid: [u8; 24]) {
        self.session = status;
        self.uid = uid;
        self.hint(AcctHint::ConfigRecheck);
    }
    pub fn pushed(&mut self, now: u64) {
        if self.margin == MarginData::Push {
            self.next_margin = now.saturating_add(self.margin_interval);
        }
    }
    /// Does not consume hints, sequence numbers or quota until the caller has reserved its ring.
    pub fn due(
        &self,
        now: u64,
        ready: bool,
        config: &[AcctQuery],
        margin: &[AcctQuery],
    ) -> Option<(AcctReq, u32)> {
        if !ready
            || matches!(self.state, AcctState::Running(_))
            || matches!(self.state, AcctState::Backoff { until } if now < until)
        {
            return None;
        }
        let has_margin = self.margin != MarginData::NotApplicable && !margin.is_empty();
        let (kind, why) = if has_margin && self.hints & 1 != 0 {
            (AcctQueryKind::Margin, 2)
        } else if self.hints & 2 != 0 && !config.is_empty() {
            (AcctQueryKind::Config, 3)
        } else if let Some(r) = self.retry {
            (r.kind, r.why)
        } else if self.startup && !config.is_empty() {
            (AcctQueryKind::Config, 5)
        } else if has_margin && now >= self.next_margin {
            (
                AcctQueryKind::Margin,
                if self.margin == MarginData::Push { 4 } else { 1 },
            )
        } else if now >= self.next_config && !config.is_empty() {
            (AcctQueryKind::Config, 1)
        } else {
            return None;
        };
        let queries = if kind == AcctQueryKind::Config {
            config
        } else {
            margin
        };
        let fallback = kind == AcctQueryKind::Margin && self.margin == MarginData::Push;
        let weight = queries
            .iter()
            .filter(|q| q.selected(self.poll_no, fallback))
            .map(|q| u32::from(q.weight))
            .sum();
        Some((
            AcctReq {
                kind,
                account: self.account,
                why,
                session: self.session,
                seq_no: self.seq_no.wrapping_add(1),
                poll_no: self.poll_no,
                _pad: 0,
                session_uid: self.uid,
            },
            weight,
        ))
    }
    pub fn started(&mut self, req: AcctReq) {
        self.retry = None;
        self.seq_no = req.seq_no;
        self.hints &= if req.kind == AcctQueryKind::Margin { !1 } else { !2 };
        self.state = AcctState::Running(req);
    }
    /// Returns whether a previously degraded REST lane has recovered. Stale records are ignored.
    pub fn done(&mut self, seq: u32, now: u64) -> Option<bool> {
        let AcctState::Running(r) = self.state else {
            return None;
        };
        if r.seq_no != seq {
            return None;
        }
        match r.kind {
            AcctQueryKind::Config => {
                self.startup = false;
                self.next_config = now.saturating_add(self.config_interval);
            }
            AcctQueryKind::Margin => {
                self.poll_no = self.poll_no.wrapping_add(1);
                self.next_margin = self.next_margin.max(now.saturating_add(self.margin_interval));
            }
        }
        let recovered = self.degraded;
        self.fails = 0;
        self.degraded = false;
        self.state = AcctState::Idle;
        Some(recovered)
    }
    /// Returns true exactly at the first degraded transition of a failure streak.
    pub fn failed(&mut self, seq: u32, now: u64, threshold: u8) -> bool {
        let AcctState::Running(r) = self.state else {
            return false;
        };
        if r.seq_no != seq {
            return false;
        }
        self.retry = Some(r);
        self.fails = self.fails.saturating_add(1);
        let delay = (1u64 << self.fails.saturating_sub(1).min(5)).min(30);
        self.state = AcctState::Backoff {
            until: now.saturating_add(self.second.saturating_mul(delay)),
        };
        let changed = !self.degraded && self.fails >= threshold;
        self.degraded |= changed;
        changed
    }
}
