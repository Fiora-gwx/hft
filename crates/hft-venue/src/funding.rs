//! Warm funding-history poll driver. HTTP is driven by the caller, like `SessionDriver`.
//! Completed windows cross acct → gw → rep → journal; no direct ledger mutation.
use core::marker::PhantomData;

use hft_event::FundingSettle;
use hft_refdata::RefData;
use hft_rt::ByteProducer;
use hft_types::{AccountId, Qty, Rate, Tsc, funding_settle_norm_ns};
use hft_venue_proto::{
    ArrayBuf, Credential, FundingBill, FundingHistoryProtocol, FundingQuery, ProtoErr, RestReqBuf, VenueCfg,
    WallClock,
};
use zerocopy::{FromBytes, IntoBytes};

/// Deployment-controlled warm-path bounds. No timers or wall-clock reads inside the driver.
#[derive(Clone, Copy, Debug)]
pub struct FundingPollCfg {
    pub poll_ms: i64,
    pub lookback_ms: i64,
    pub settle_lag_ms: i64,
    /// Reject bills outside the registered settlement grid; requires a refdata refresh.
    pub max_settlement_skew_ms: i64,
    pub max_pages: u32,
    pub max_bills: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FundingPollErr {
    Config,
    Busy,
    Decode,
    Capacity,
    ConflictingBill,
    Cursor,
}

/// One account, one in-flight page, bounded staging. Restart from the ledger history watermark;
/// overlaps are safe because actual per-symbol totals replace previous estimates/totals.
#[derive(Debug)]
pub struct FundingPoll<P: FundingHistoryProtocol> {
    account: AccountId,
    cfg: FundingPollCfg,
    from_ms: i64,
    due_ms: i64,
    query: Option<FundingQuery>,
    in_flight: bool,
    bills: Vec<FundingBill>,
    ready: Vec<FundingSettle>,
    sent: usize,
    _protocol: PhantomData<P>,
}

impl<P: FundingHistoryProtocol> FundingPoll<P> {
    pub fn new(account: AccountId, from_ms: i64, cfg: FundingPollCfg) -> Result<Self, FundingPollErr> {
        if account.0 == 0
            || from_ms < 0
            || cfg.poll_ms <= 0
            || cfg.lookback_ms <= 0
            || cfg.lookback_ms >= 30 * 86_400_000
            || cfg.max_settlement_skew_ms < 0
            || cfg.settle_lag_ms < cfg.max_settlement_skew_ms
            || cfg.max_pages == 0
            || cfg.max_bills == 0
        {
            return Err(FundingPollErr::Config);
        }
        Ok(Self {
            account,
            cfg,
            from_ms,
            due_ms: from_ms,
            query: None,
            in_flight: false,
            bills: Vec::new(),
            ready: Vec::new(),
            sent: 0,
            _protocol: PhantomData,
        })
    }

    /// Return one signed GET when due. The transport must call `response` or `retry` exactly once.
    pub fn request(
        &mut self,
        cfg: &VenueCfg,
        cred: &Credential,
        clock: &WallClock,
    ) -> Result<Option<RestReqBuf>, ProtoErr> {
        if self.in_flight || !self.ready.is_empty() || clock.unix_ms < self.due_ms {
            return Ok(None);
        }
        if self.query.is_none() {
            let end_ms = clock
                .unix_ms
                .saturating_sub(self.cfg.settle_lag_ms)
                .min(self.from_ms.saturating_add(30 * 86_400_000));
            if end_ms < self.from_ms {
                return Ok(None);
            }
            self.query = Some(FundingQuery {
                start_ms: self.from_ms,
                end_ms,
                page: 1,
                cursor: ArrayBuf::default(),
            });
        }
        let Some(q) = &self.query else { return Ok(None) };
        let mut out = RestReqBuf::default();
        P::funding_request(cfg, cred, q, clock, &mut out)?;
        self.in_flight = true;
        Ok(Some(out))
    }

    /// Timeout/403/429/5xx/decode failure: retain the entire fixed window, retry the same page.
    pub fn retry(&mut self, now_ms: i64) {
        self.in_flight = false;
        self.due_ms = now_ms.saturating_add(self.cfg.poll_ms);
    }

    pub fn response(
        &mut self,
        rd: &RefData,
        status: u16,
        body: &[u8],
        now_ms: i64,
        rx: Tsc,
    ) -> Result<(), FundingPollErr> {
        if !self.in_flight {
            return Err(FundingPollErr::Busy);
        }
        self.retry(now_ms);
        let q = self.query.as_ref().ok_or(FundingPollErr::Busy)?;
        let page = P::funding_decode(rd, q, status, body).map_err(|_| FundingPollErr::Decode)?;
        if page.next.as_ref().is_some_and(|n| {
            n.start_ms != q.start_ms
                || n.end_ms != q.end_ms
                || n.page <= q.page
                || n.page > self.cfg.max_pages
        }) {
            return Err(FundingPollErr::Cursor);
        }
        // Validate the whole page before touching staging (retrying a failed page is atomic).
        let mut staged = self.bills.clone();
        for bill in page.bills {
            if rd.inst(bill.inst).venue != P::VENUE {
                return Err(FundingPollErr::Decode);
            }
            if let Some(old) = staged.iter().find(|old| old.id == bill.id) {
                if old != &bill {
                    return Err(FundingPollErr::ConflictingBill);
                }
            } else {
                if staged.len() == self.cfg.max_bills {
                    return Err(FundingPollErr::Capacity);
                }
                staged.push(bill);
            }
        }
        if let Some(next) = page.next {
            self.bills = staged;
            self.query = Some(next);
            return Ok(());
        }
        let mut totals: Vec<FundingSettle> = Vec::new();
        for bill in &staged {
            let meta = rd.inst_meta(bill.inst);
            let ts = funding_settle_norm_ns(bill.time_ns, meta.funding_interval_s, meta.funding_anchor_s);
            if meta.funding_interval_s == 0
                || i128::from(bill.time_ns).abs_diff(i128::from(ts))
                    > self.cfg.max_settlement_skew_ms as u128 * 1_000_000
            {
                return Err(FundingPollErr::Config);
            }
            // A window beginning inside an older settlement cannot prove its complete total.
            if ts / 1_000_000 < q.start_ms {
                continue;
            }
            if let Some(e) = totals
                .iter_mut()
                .find(|e| e.inst == bill.inst && e.settle_ts_ns == ts)
            {
                e.amount.0 = e
                    .amount
                    .0
                    .checked_add(bill.amount.0)
                    .ok_or(FundingPollErr::Capacity)?;
            } else {
                totals.push(FundingSettle {
                    venue: P::VENUE,
                    account: self.account,
                    _pad: 0,
                    inst: bill.inst,
                    amount: bill.amount,
                    rate: Rate(0),
                    pos_qty: Qty(0),
                    settle_ts_ns: ts,
                    rx_tsc: rx,
                    synth: 0,
                    _pad2: [0; 7],
                });
            }
        }
        totals.sort_by_key(|e| (e.settle_ts_ns, e.inst.0));
        self.from_ms = self.from_ms.max(q.end_ms.saturating_sub(self.cfg.lookback_ms));
        self.ready = totals;
        self.sent = 0;
        self.bills.clear();
        self.query = None;
        Ok(())
    }

    pub fn pending(&self) -> &[FundingSettle] {
        self.ready.get(self.sent..).unwrap_or(&[])
    }

    /// At most one acct record per warm task round. Full SPSC keeps the record for the next round.
    pub fn publish_one(&mut self, acct: &mut ByteProducer) -> bool {
        let Some(event) = self.ready.get(self.sent) else {
            return false;
        };
        if !write_funding_record(acct, event) {
            return false;
        }
        self.sent += 1;
        if self.sent == self.ready.len() {
            self.ready.clear();
            self.sent = 0;
        }
        true
    }
}

/// Versioned, process-local acct record; contains no credentials or raw signed request.
pub fn write_funding_record(out: &mut ByteProducer, event: &FundingSettle) -> bool {
    let Some(buf) = out.reserve(64) else { return false };
    let Some((tag, body)) = buf.split_at_mut_checked(8) else {
        return false;
    };
    tag.copy_from_slice(b"FUNDv001");
    body.copy_from_slice(event.as_bytes());
    out.commit();
    true
}

pub(crate) fn read_funding_record(bytes: &[u8]) -> Option<FundingSettle> {
    let (tag, body) = bytes.split_at_checked(8)?;
    if tag != b"FUNDv001" {
        return None;
    }
    FundingSettle::read_from_bytes(body).ok()
}
