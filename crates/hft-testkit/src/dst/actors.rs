//! The world's non-venue actors (README "DST 的世界模型" 2): `MarketActor` (generated walk or a
//! recorded fragment), `CtlActor` (control commands with principal and role), `ReconActor`
//! (reconciliation reports from the exchange truth with the reconciler's flag conditions),
//! `HkActor` (`ClockStatus`, persist and disk timers) and `AdvisorActor` (Advice with its basis,
//! optionally lagging). Every actor draws only from its own stream (`actor_rng`).

use hft_event::{
    AdviceHdr, ClockStatus, CmdHdr, CmdScope, ControlCmd, ReconcileReport, SysTimer, Timer, TimerCode,
};
use hft_types::{AccountId, GroupId, InstId, Notional, Qty, RunMode, StrategyId, Tsc, VenueId};
use rand_chacha::ChaCha8Rng;

use crate::dst::faults::{AdvisorLagFault, Fault};
use crate::dst::obs::TruthRow;
use crate::dst::sut::{InputRec, Source};
use crate::dst::world::{ActorId, RngExt, WorldSpec, actor_rng};
use crate::dst::{MarketGen, MarketSource};

/// Market data source of a run.
#[derive(Debug)]
pub struct MarketActor {
    rng: ChaCha8Rng,
    mgen: MarketGen,
    /// (inst, venue, bid, ask, depth, tick).
    state: Vec<(InstId, VenueId, i64, i64, i64, i64)>,
    recorded: Vec<(u64, InputRec)>,
    next_rec: usize,
    /// Drift per inst (OneSidedMove): (inst, start_ns, over_ns, pct_bp, start_mid).
    moves: Vec<(InstId, u64, u64, i32, i64)>,
}

/// One market update for a venue.
#[derive(Clone, Debug)]
pub enum MarketOut {
    /// Generated BBO through the venue's md layer.
    Bbo {
        venue: VenueId,
        inst: InstId,
        bid: (i64, i64),
        ask: (i64, i64),
    },
    /// A recorded Input, delivered as is.
    Recorded(InputRec),
}

impl MarketActor {
    pub fn new(seed: u64, world: &WorldSpec, src: &MarketSource) -> MarketActor {
        let (g, recorded) = match src {
            MarketSource::Generated(g) => (*g, Vec::new()),
            MarketSource::Recorded(r) => (MarketGen::default(), r.clone()),
        };
        MarketActor {
            rng: actor_rng(seed, ActorId::Market),
            mgen: g,
            state: world
                .insts
                .iter()
                .map(|i| (i.id, i.venue, i.bid, i.ask, i.depth, i.tick))
                .collect(),
            recorded,
            next_rec: 0,
            moves: Vec::new(),
        }
    }
    pub fn is_recorded(&self) -> bool {
        !self.recorded.is_empty()
    }
    /// Time of the first tick.
    pub fn first(&mut self) -> u64 {
        if self.is_recorded() {
            return self.recorded.first().map_or(u64::MAX, |r| r.0);
        }
        self.mgen.tick_ms * 1_000_000 / 2
    }
    /// One tick at `now`: the updates and the next tick time.
    pub fn tick(&mut self, now: u64) -> (Vec<MarketOut>, u64) {
        if self.is_recorded() {
            let mut out = Vec::new();
            while let Some((t, r)) = self.recorded.get(self.next_rec) {
                if *t > now {
                    break;
                }
                out.push(MarketOut::Recorded(r.clone()));
                self.next_rec += 1;
            }
            let next = self.recorded.get(self.next_rec).map_or(u64::MAX, |r| r.0);
            return (out, next);
        }
        let n = self.state.len() as u64;
        let ix = self.rng.below(n) as usize;
        let (inst, venue, bid, ask, depth, tick) = self.state[ix];
        let mid = (bid + ask) / 2;
        let step = (mid as i128 * i128::from(self.mgen.walk_centi_bp) / 1_000_000) as i64;
        let mut d = self.rng.range(0, 2 * step.max(0) as u64) as i64 - step.max(0);
        for (i, start, over, pct, m0) in &self.moves {
            if *i == inst && now >= *start {
                let el = (now - start).min(*over);
                let target = *m0
                    + (*m0 as i128 * i128::from(*pct) * i128::from(el)
                        / (10_000 * i128::from((*over).max(1)))) as i64;
                d += target - mid;
            }
        }
        let tick = tick.max(1);
        let nb = ((bid + d) / tick * tick).max(tick);
        let na = (nb + (ask - bid).max(tick)).max(nb + tick);
        self.state[ix].2 = nb;
        self.state[ix].3 = na;
        let jitter = self.mgen.tick_ms * 1_000_000 / n.max(1);
        let next = now + jitter / 2 + self.rng.below(jitter.max(1));
        (
            vec![MarketOut::Bbo {
                venue,
                inst,
                bid: (nb, depth),
                ask: (na, depth),
            }],
            next,
        )
    }
    pub fn apply_fault(&mut self, f: &Fault, now: u64) {
        if let Fault::OneSidedMove {
            inst,
            pct_bp,
            over_ms,
        } = *f
            && let Some(s) = self.state.iter().find(|s| s.0 == inst)
        {
            self.moves
                .push((inst, now, u64::from(over_ms) * 1_000_000, pct_bp, (s.2 + s.3) / 2));
        }
    }
}

/// Control principals (role codes of `CmdHdr.role`).
pub const PRINCIPALS: [(&str, u8); 3] = [
    ("admin", CmdHdr::ROLE_ADMIN),
    ("trader", CmdHdr::ROLE_TRADER),
    ("viewer", CmdHdr::ROLE_VIEWER),
];

/// Encodes a command as the `ControlCmd` logical body with the principal's envelope.
pub fn encode_cmd(
    cmd: &ControlCmd<'_>,
    issuer: &str,
    role: u8,
    account: AccountId,
    now_ns: u64,
    id: u64,
) -> InputRec {
    let mut h = CmdHdr::zeroed();
    h.cmd_id[..8].copy_from_slice(&id.to_le_bytes());
    let n = issuer.len().min(24);
    h.issuer[..n].copy_from_slice(&issuer.as_bytes()[..n]);
    h.issued_at_ns = now_ns as i64;
    h.expires_at_ns = now_ns as i64 + 60_000_000_000;
    h.role = role;
    h.account = account;
    let mut buf = vec![0u8; 8_192];
    let len = cmd.encode(&h, &mut buf).unwrap_or(0);
    buf.truncate(len);
    InputRec {
        kind: hft_event::EventKind::CONTROL_CMD,
        body: buf,
        src: Source::Ctl,
    }
}

/// Injects control commands from the control faults of the plan.
#[derive(Debug)]
pub struct CtlActor {
    rng: ChaCha8Rng,
    next_id: u64,
}

impl CtlActor {
    pub fn new(seed: u64) -> CtlActor {
        CtlActor {
            rng: actor_rng(seed, ActorId::Ctl),
            next_id: 1,
        }
    }
    /// Commands for a control fault (with a randomly chosen principal; `ViewerEmergency` is a
    /// Viewer `Kill`).
    pub fn apply_fault(&mut self, f: &Fault, now: u64) -> Vec<(u64, InputRec)> {
        let scope = |a: AccountId| CmdScope {
            strategy: StrategyId(0),
            venue: VenueId(0),
            account: a,
            _pad: 0,
            group: hft_types::GROUP_ALL,
            inst: InstId(0),
            _pad2: 0,
        };
        let (who, role) = PRINCIPALS[self.rng.below(3) as usize];
        let mut out = Vec::new();
        let mut push = |me: &mut Self, c: ControlCmd<'_>, who: &str, role: u8, a: AccountId, at: u64| {
            let id = me.next_id;
            me.next_id += 1;
            out.push((at, encode_cmd(&c, who, role, a, at, id)));
        };
        match *f {
            Fault::KillHard { account } => push(
                self,
                ControlCmd::Kill {
                    scope: scope(account),
                    level: hft_types::KillLevel::Account,
                    flatten: false,
                    cancel_resting: true,
                    hard: true,
                },
                "admin",
                CmdHdr::ROLE_ADMIN,
                account,
                now,
            ),
            Fault::KillFlatten { account, .. } => push(
                self,
                ControlCmd::Kill {
                    scope: scope(account),
                    level: hft_types::KillLevel::Account,
                    flatten: true,
                    cancel_resting: true,
                    hard: false,
                },
                who,
                role,
                account,
                now,
            ),
            Fault::ViewerEmergency { .. } => push(
                self,
                ControlCmd::Kill {
                    scope: scope(AccountId(0)),
                    level: hft_types::KillLevel::Global,
                    flatten: false,
                    cancel_resting: true,
                    hard: false,
                },
                "viewer",
                CmdHdr::ROLE_VIEWER,
                AccountId(0),
                now,
            ),
            Fault::ResumeKill { .. } => push(
                self,
                ControlCmd::Resume {
                    scope: scope(AccountId(0)),
                    source: hft_types::CapSource::KILL,
                },
                "trader",
                CmdHdr::ROLE_TRADER,
                AccountId(0),
                now,
            ),
            Fault::FlattenLockTarget { .. } => push(
                self,
                ControlCmd::Drain { cancel_resting: true },
                "admin",
                CmdHdr::ROLE_ADMIN,
                AccountId(0),
                now,
            ),
            Fault::SimReset { account } => {
                push(self, ControlCmd::SimReset { account }, who, role, account, now)
            }
            Fault::CmdDup | Fault::CmdReorder | Fault::CmdExpired | Fault::SetParamsKillInterleave { .. } => {
                let mode = if self.rng.chance(1, 2) {
                    RunMode::Live
                } else {
                    RunMode::CloseOnly
                };
                let c = ControlCmd::SetMode {
                    scope: scope(AccountId(0)),
                    mode,
                };
                push(self, c, who, role, AccountId(0), now);
                if matches!(f, Fault::CmdDup) {
                    let mut again = out.last().cloned();
                    if let Some(a) = again.as_mut() {
                        a.0 += 1_000_000;
                    }
                    out.extend(again);
                }
            }
            _ => {}
        }
        out
    }
}

/// Builds reconciliation reports from the exchange truth (reconciler `ReconcileReport` flags).
#[derive(Debug)]
pub struct ReconActor {
    pub period_ns: u64,
    next_id: u64,
    /// Previous report had `USERSTREAM_DOWN`.
    prev_down: bool,
    /// User stream down until (virtual ns) and the last view point.
    pub userstream_down_until: u64,
    last_view: u64,
    /// Funding differences to report (`funding_flows` shape).
    pub funding_diffs: Vec<(AccountId, VenueId, i64)>,
}

impl ReconActor {
    /// Package one complete account/venue report group with original exchange receipts.
    /// The caller selects the missing executions from the exchange's retained history.
    pub fn settled_input(
        reports: &[ReconcileReport],
        receipts: &[super::venue_behavior::ExternalReceipt],
    ) -> Result<InputRec, &'static str> {
        use zerocopy::IntoBytes;
        let first = reports.first().ok_or("empty reconciliation")?;
        let hdr = hft_event::ReconcileBatchHdr {
            report_id: first.report_id,
            venue: first.venue,
            account: first.account,
            n_reports: u16::try_from(reports.len()).map_err(|_| "too many reports")?,
            n_fills: u16::try_from(receipts.len()).map_err(|_| "too many receipts")?,
            _pad: 0,
        };
        let mut body = hdr.as_bytes().to_vec();
        for r in reports {
            body.extend_from_slice(r.as_bytes());
        }
        for r in receipts {
            body.extend_from_slice(r.fill.as_bytes());
        }
        hft_event::ReconcileBatchView::read(&body).map_err(|_| "invalid reconciliation group")?;
        Ok(InputRec {
            kind: hft_event::EventKind::RECONCILE_BATCH,
            body,
            src: Source::Recon,
        })
    }

    pub fn new(period_ns: u64) -> ReconActor {
        ReconActor {
            period_ns,
            next_id: 1,
            prev_down: false,
            userstream_down_until: 0,
            last_view: 0,
            funding_diffs: Vec::new(),
        }
    }
    /// One reconciliation round at `now` against the ledger quantities `engine` (per account,
    /// venue, inst) and the truth.
    pub fn round(
        &mut self,
        now: u64,
        truth: &[TruthRow],
        engine: &dyn Fn(AccountId, VenueId, InstId) -> i128,
    ) -> Vec<ReconcileReport> {
        let down = now < self.userstream_down_until;
        // AUTO_EXTERNAL: two consecutive reports without USERSTREAM_DOWN and the stream stable for
        // a full period before the previous view point.
        let stable =
            !down && !self.prev_down && self.userstream_down_until + self.period_ns <= self.last_view;
        let mut out = Vec::new();
        let id = self.next_id;
        self.next_id += 1;
        let mut sflags = 0u16;
        if down {
            sflags |= ReconcileReport::USERSTREAM_DOWN;
        }
        for t in truth {
            for (inst, q) in &t.positions {
                let e = engine(t.account, t.venue, *inst);
                if e != *q {
                    let mut r: ReconcileReport = zerocopy::FromZeros::new_zeroed();
                    r.report_id = id;
                    r.venue = t.venue;
                    r.account = t.account;
                    r.item = 1;
                    r.inst = *inst;
                    r.engine_qty = Qty(e as i64);
                    r.exch_qty = Qty(*q as i64);
                    r.view_seq = now;
                    r.sflags = sflags | if stable { ReconcileReport::AUTO_EXTERNAL } else { 0 };
                    out.push(r);
                }
            }
        }
        for (a, v, d) in &self.funding_diffs {
            let mut r: ReconcileReport = zerocopy::FromZeros::new_zeroed();
            r.report_id = id;
            r.venue = *v;
            r.account = *a;
            r.item = 5;
            r.diff = Notional(*d);
            r.sflags = sflags;
            out.push(r);
        }
        self.funding_diffs.clear();
        let n = out.len() as u16;
        for (i, r) in out.iter_mut().enumerate() {
            r.idx = i as u16;
            r.count = n;
        }
        if out.is_empty() {
            let mut r: ReconcileReport = zerocopy::FromZeros::new_zeroed();
            r.report_id = id;
            r.count = 1;
            r.sflags = sflags;
            out.push(r);
        }
        self.prev_down = down;
        self.last_view = now;
        out
    }
}

/// Housekeeping: `ClockStatus` every `clock_ms`, persist/disk timers from storage faults.
#[derive(Debug)]
pub struct HkActor {
    pub clock_ms: u64,
}

impl HkActor {
    pub fn clock_status(now: u64, tsc_hz: u64) -> InputRec {
        let c = ClockStatus {
            tsc: Tsc(crate::dst::driver::tsc_at(0, tsc_hz, now)),
            realtime_ns: crate::dst::world::UTC0 + now as i64,
            phc_ns: crate::dst::world::UTC0 + now as i64,
            tsc_hz_milli: tsc_hz * 1_000,
            chrony_offset_ns: 0,
            max_error_ns: 1_000,
            rms_ns: 100,
            calib_err_ns: 10,
            source: 1,
            state: 1,
            _pad: 0,
        };
        InputRec::of(&c, Source::Hk)
    }
    /// `Timer{Sys, PersistLag|DiskLow}` of a storage fault.
    pub fn apply_fault(f: &Fault) -> Option<InputRec> {
        let code = match f {
            Fault::PersistLag { .. } => TimerCode::Sys(SysTimer::PersistLag { level: 1 }),
            Fault::DiskLow => TimerCode::Sys(SysTimer::DiskLow { level: 1 }),
            _ => return None,
        };
        let (owner, timer_id, group) = code.encode().ok()?;
        let t = Timer {
            timer_id,
            owner,
            _pad: 0,
            group,
            _pad2: 0,
            deadline_tsc: 0,
        };
        Some(InputRec::of(&t, Source::Hk))
    }
}

/// Drives the Model side: Advice every `period_ns` of engine time with the current seq as basis;
/// lag faults delay delivery (README "DST 的世界模型" AdvisorActor).
#[derive(Debug)]
pub struct AdvisorActor {
    pub period_ns: u64,
    pub strategy: StrategyId,
    pub lags: Vec<AdvisorLagFault>,
}

impl AdvisorActor {
    /// Delivery time of an Advice produced at engine time `basis_ns`.
    pub fn delivery(&self, basis_ns: u64) -> u64 {
        let e = basis_ns as i64;
        self.lags
            .iter()
            .find(|l| l.from_ens <= e && e < l.to_ens)
            .map_or(basis_ns, |l| basis_ns + u64::from(l.lag_ms) * 1_000_000)
    }
    /// The Advice Input of `basis_seq` at `basis_ns` (opaque payload = basis time).
    pub fn advice(&self, basis_seq: u64, basis_ns: u64, tsc_hz: u64) -> InputRec {
        let payload = basis_ns.to_le_bytes();
        let h = AdviceHdr {
            strategy: self.strategy,
            account: AccountId(0),
            local_kind: 1,
            group: GroupId(0),
            len: payload.len() as u16,
            _pad: [0; 6],
            basis_seq,
            basis_tsc: Tsc(crate::dst::driver::tsc_at(0, tsc_hz, basis_ns)),
        };
        let mut body = zerocopy::IntoBytes::as_bytes(&h).to_vec();
        body.extend_from_slice(&payload);
        InputRec {
            kind: hft_event::EventKind::ADVICE,
            body,
            src: Source::Advisor,
        }
    }
}

/// Reads the basis of an Advice Input.
pub fn advice_basis(r: &InputRec) -> Option<(u64, u64)> {
    let (h, rest) = <AdviceHdr as zerocopy::FromBytes>::read_from_prefix(&r.body).ok()?;
    let b = rest.get(..8)?;
    Some((h.basis_seq, u64::from_le_bytes(b.try_into().ok()?)))
}
