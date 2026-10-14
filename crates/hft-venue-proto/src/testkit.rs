//! Owned protocol observations, independent golden comparisons and contract checks.

use hft_event::{
    Bbo, Funding, IndexPrice, L2BatchHdr, L2SnapHdr, L2SnapshotPart, Level, Liquidation, MarkPrice,
    MarketHdr, Trade,
};
use hft_refdata::RefData;
use hft_types::{Px, Qty, Side};

use crate::{MdSink, MsgOp, RouteId, RxMeta, VenueProtocol};
use zerocopy::IntoBytes;
mod checks;
pub use checks::*;

/// One `MdSink` call, recorded with owned copies of its inputs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MdRec {
    Bbo(Bbo),
    L2Begin(L2BatchHdr),
    L2Level(Side, Px, Qty),
    L2End,
    L2Snapshot {
        hdr: L2SnapHdr,
        bids: Vec<Level>,
        asks: Vec<Level>,
        part: u16,
        parts: u16,
    },
    Trade(Trade),
    Mark(MarkPrice),
    Index(IndexPrice),
    Funding(Funding),
    Liquidation(Liquidation),
}

impl MdRec {
    /// `MarketHdr` of the recorded event (`None` for `L2Level`/`L2End`).
    pub fn market_hdr(&self) -> Option<&MarketHdr> {
        match self {
            MdRec::Bbo(e) => Some(&e.h),
            MdRec::L2Begin(e) => Some(&e.h),
            MdRec::L2Snapshot { hdr, .. } => Some(&hdr.h),
            MdRec::Trade(e) => Some(&e.h),
            MdRec::Mark(e) => Some(&e.h),
            MdRec::Index(e) => Some(&e.h),
            MdRec::Funding(e) => Some(&e.h),
            MdRec::Liquidation(e) => Some(&e.h),
            MdRec::L2Level(..) | MdRec::L2End => None,
        }
    }
}

/// Records every sink call in order.
#[derive(Clone, Debug, Default)]
pub struct VecSink {
    pub md: Vec<MdRec>,
    pub reports: Vec<ReportRec>,
    pub ids: Vec<(Vec<u8>, hft_types::ClientOrderId)>,
    pub id_calls: Vec<(Vec<u8>, Option<hft_types::ClientOrderId>)>,
}

/// Owned, ordered report observations; no deduplication is hidden in the test sink.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ReportRec {
    Order(hft_event::OrderEvent),
    Position(hft_event::PositionReport),
    Balance(hft_event::BalanceReport),
    Funding(hft_event::FundingSettle),
    External(hft_event::ExternalFill),
    Margin(hft_event::MarginReportHdr, Vec<hft_event::MarginItem>),
    Config(hft_event::AccountConfigReportHdr, Vec<hft_event::LeverageItem>),
}
impl crate::ReportSink for VecSink {
    fn order(&mut self, e: &hft_event::OrderEvent) {
        self.reports.push(ReportRec::Order(*e));
    }
    fn position(&mut self, e: &hft_event::PositionReport) {
        self.reports.push(ReportRec::Position(*e));
    }
    fn balance(&mut self, e: &hft_event::BalanceReport) {
        self.reports.push(ReportRec::Balance(*e));
    }
    fn funding_settle(&mut self, e: &hft_event::FundingSettle) {
        self.reports.push(ReportRec::Funding(*e));
    }
    fn external_fill(&mut self, e: &hft_event::ExternalFill) {
        self.reports.push(ReportRec::External(*e));
    }
    fn margin(&mut self, h: &hft_event::MarginReportHdr, items: &[hft_event::MarginItem]) {
        self.reports.push(ReportRec::Margin(*h, items.to_vec()));
    }
    fn account_config(&mut self, h: &hft_event::AccountConfigReportHdr, items: &[hft_event::LeverageItem]) {
        self.reports.push(ReportRec::Config(*h, items.to_vec()));
    }
    fn exch_id(&mut self, key: &[u8], owner: Option<&hft_types::ClientOrderId>) -> crate::ExchIdHint {
        self.id_calls.push((key.to_vec(), owner.copied()));
        if let Some(id) = owner {
            if let Some(row) = self.ids.iter_mut().find(|(k, _)| k == key) {
                row.1 = *id;
            } else {
                self.ids.push((key.to_vec(), *id));
            }
            crate::ExchIdHint::Recorded
        } else if let Some((_, id)) = self.ids.iter().find(|(k, _)| k == key) {
            crate::ExchIdHint::Resolved {
                epoch: id.epoch,
                slot: hft_types::SlotId(id.slot),
                r#gen: id.r#gen,
            }
        } else {
            crate::ExchIdHint::Unknown
        }
    }
}

impl MdSink for VecSink {
    fn bbo(&mut self, e: &Bbo) {
        self.md.push(MdRec::Bbo(*e));
    }
    fn l2_begin(&mut self, h: &L2BatchHdr) {
        self.md.push(MdRec::L2Begin(*h));
    }
    fn l2_level(&mut self, side: Side, px: Px, qty: Qty) {
        self.md.push(MdRec::L2Level(side, px, qty));
    }
    fn l2_end(&mut self) {
        self.md.push(MdRec::L2End);
    }
    fn l2_snapshot(&mut self, part: &L2SnapshotPart<'_>) {
        self.md.push(MdRec::L2Snapshot {
            hdr: part.hdr,
            bids: part.bids.to_vec(),
            asks: part.asks.to_vec(),
            part: part.part,
            parts: part.parts,
        });
    }
    fn trade(&mut self, e: &Trade) {
        self.md.push(MdRec::Trade(*e));
    }
    fn mark(&mut self, e: &MarkPrice) {
        self.md.push(MdRec::Mark(*e));
    }
    fn index(&mut self, e: &IndexPrice) {
        self.md.push(MdRec::Index(*e));
    }
    fn funding(&mut self, e: &Funding) {
        self.md.push(MdRec::Funding(*e));
    }
    fn liquidation(&mut self, e: &Liquidation) {
        self.md.push(MdRec::Liquidation(*e));
    }
}

/// A contract check failure: `contract` = contract number (README "实现方必须遵守的契约").
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ContractFail {
    pub contract: u8,
    pub sample: String,
    pub detail: String,
}

/// A golden mismatch identifies the first differing byte of the first differing record.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GoldenFail {
    pub sample: String,
    pub record: usize,
    pub offset: usize,
}

#[derive(Debug)]
pub struct GoldenDriver<P>(core::marker::PhantomData<P>);

impl MdRec {
    pub fn bytes(&self) -> Vec<u8> {
        match self {
            Self::Bbo(e) => e.as_bytes().to_vec(),
            Self::Trade(e) => e.as_bytes().to_vec(),
            Self::Mark(e) => e.as_bytes().to_vec(),
            Self::Index(e) => e.as_bytes().to_vec(),
            Self::Funding(e) => e.as_bytes().to_vec(),
            Self::Liquidation(e) => e.as_bytes().to_vec(),
            Self::L2Begin(e) => e.as_bytes().to_vec(),
            Self::L2Level(side, px, qty) => {
                let mut b = vec![*side as u8];
                b.extend(px.as_bytes());
                b.extend(qty.as_bytes());
                b
            }
            Self::L2End => Vec::new(),
            Self::L2Snapshot {
                hdr,
                bids,
                asks,
                part,
                parts,
            } => {
                let mut b = hdr.as_bytes().to_vec();
                b.extend(part.to_le_bytes());
                b.extend(parts.to_le_bytes());
                b.extend(bids.as_slice().as_bytes());
                b.extend(asks.as_slice().as_bytes());
                b
            }
        }
    }
}
impl<P: VenueProtocol> GoldenDriver<P> {
    pub fn compare(sample: &str, actual: &[Vec<u8>], expected: &[Vec<u8>]) -> Result<(), GoldenFail> {
        for i in 0..actual.len().max(expected.len()) {
            let (Some(a), Some(b)) = (actual.get(i), expected.get(i)) else {
                return Err(GoldenFail {
                    sample: sample.into(),
                    record: i,
                    offset: 0,
                });
            };
            if let Some(offset) = a
                .iter()
                .zip(b)
                .position(|(x, y)| x != y)
                .or_else(|| (a.len() != b.len()).then_some(a.len().min(b.len())))
            {
                return Err(GoldenFail {
                    sample: sample.into(),
                    record: i,
                    offset,
                });
            }
        }
        Ok(())
    }
    /// Expectations must come from an independently reviewed fixture, never from this decoder.
    pub fn run_md(
        samples: &[MdSample<'_>],
        rd: &RefData,
        ctx: &mut P::FeedCtx,
        expected: &[Vec<Vec<u8>>],
    ) -> Result<(), GoldenFail> {
        if samples.len() != expected.len() {
            return Err(GoldenFail {
                sample: "sample count".into(),
                record: samples.len().min(expected.len()),
                offset: 0,
            });
        }
        for (sample, want) in samples.iter().zip(expected) {
            let recs =
                decode_samples::<P>(core::slice::from_ref(sample), rd, ctx).map_err(|_| GoldenFail {
                    sample: sample.name.into(),
                    record: 0,
                    offset: 0,
                })?;
            let got: Vec<_> = recs[0].iter().map(MdRec::bytes).collect();
            Self::compare(sample.name, &got, want)?;
        }
        Ok(())
    }
}

/// A report fixture includes independently specified records and outcome metadata.
#[derive(Clone, Debug)]
pub struct ReportSample<'a> {
    pub name: &'a str,
    pub op: MsgOp,
    pub payload: &'a [u8],
    pub rx: RxMeta,
    pub expected: &'a [ReportRec],
    pub outcome: crate::ReportOutcome,
}
pub fn check_reports<P: VenueProtocol>(
    contract: u8,
    samples: &[ReportSample<'_>],
    rd: &RefData,
    ctx: &mut P::GwCtx,
) -> Result<Vec<ReportRec>, ContractFail> {
    let mut sink = VecSink::default();
    let mut all = Vec::new();
    for s in samples {
        sink.reports.clear();
        let class = P::classify_report(s.op, s.payload.get(..64).unwrap_or(s.payload));
        let got =
            P::decode_report(rd, ctx, class, s.payload, &s.rx, &mut sink).map_err(|e| ContractFail {
                contract,
                sample: s.name.into(),
                detail: format!("decode_report: {e:?}"),
            })?;
        if got != s.outcome || sink.reports != s.expected {
            return Err(ContractFail {
                contract,
                sample: s.name.into(),
                detail: "records or outcome differ from independent expectation".into(),
            });
        }
        if usize::from(got.events) != sink.reports.len() {
            return Err(ContractFail {
                contract,
                sample: s.name.into(),
                detail: "event count differs from sink".into(),
            });
        }
        all.extend(sink.reports.iter().cloned());
    }
    Ok(all)
}

/// Gate the current request only on explicit rejection evidence; transport failures remain unknown.
pub fn check_unknown_codes<P: VenueProtocol>(
    samples: &[ReportSample<'_>],
    rd: &RefData,
    ctx: &mut P::GwCtx,
) -> Result<(), ContractFail> {
    check_reports::<P>(8, samples, rd, ctx)?;
    for s in samples {
        if s.outcome.outcome_unknown.is_some()
            && s.expected
                .iter()
                .any(|r| matches!(r,ReportRec::Order(e) if e.what==hft_event::OrderEventKind::Reject))
        {
            return Err(ContractFail {
                contract: 6,
                sample: s.name.into(),
                detail: "unknown response emitted Reject".into(),
            });
        }
    }
    Ok(())
}

pub fn check_cum<P: VenueProtocol>(
    samples: &[ReportSample<'_>],
    rd: &RefData,
    ctx: &mut P::GwCtx,
) -> Result<(), ContractFail> {
    let reports = check_reports::<P>(9, samples, rd, ctx)?;
    let mut watermarks = Vec::new();
    for r in reports {
        if let ReportRec::Order(e) = r
            && matches!(
                e.what,
                hft_event::OrderEventKind::Fill
                    | hft_event::OrderEventKind::PartialFill
                    | hft_event::OrderEventKind::QueryResult
            )
        {
            if e.cum_filled.0 < 0 || (e.cum_filled.0 > 0 && e.avg_px.0 <= 0) {
                return Err(ContractFail {
                    contract: 9,
                    sample: "cumulative".into(),
                    detail: "negative cumulative quantity or missing average price".into(),
                });
            }
            let key = (e.account, e.epoch, e.slot, e.r#gen, e.inst, e.exch_order_id);
            if let Some((_, previous)) = watermarks.iter_mut().find(|(k, _)| *k == key) {
                if e.cum_filled.0 < *previous {
                    return Err(ContractFail {
                        contract: 9,
                        sample: "cumulative".into(),
                        detail: "cumulative quantity decreased".into(),
                    });
                }
                *previous = e.cum_filled.0;
            } else {
                watermarks.push((key, e.cum_filled.0));
            }
        }
    }
    Ok(())
}

/// One market-data sample supplied by the venue's `tests/contract.rs`.
#[derive(Clone, Debug)]
pub struct MdSample<'a> {
    pub name: &'a str,
    pub route: RouteId,
    pub op: MsgOp,
    pub payload: &'a [u8],
    pub rx: RxMeta,
}

/// Decodes each sample with a fresh `VecSink` and returns the recorded calls (decode errors are
/// contract failures of contract 3: every sample must decode).
pub fn decode_samples<P: VenueProtocol>(
    samples: &[MdSample<'_>],
    rd: &RefData,
    ctx: &mut P::FeedCtx,
) -> Result<Vec<Vec<MdRec>>, ContractFail> {
    let mut out = Vec::with_capacity(samples.len());
    for s in samples {
        let c = P::classify(s.route, s.op, s.payload.get(..64).unwrap_or(s.payload));
        let mut sink = VecSink::default();
        P::decode_md(rd, ctx, c, s.payload, &s.rx, &mut sink).map_err(|e| ContractFail {
            contract: 3,
            sample: s.name.to_string(),
            detail: format!("decode_md failed: {e:?}"),
        })?;
        out.push(sink.md);
    }
    Ok(out)
}

/// `check_md_basics`, contract-3 part: every event of every sample carries `RxMeta`'s `rx_tsc`,
/// `hw_rx_ns`, `sw_rx_ns`, `rx_empty_gap_ns` and `conn` unchanged in its `MarketHdr` (in particular
/// `hw_rx_ns = 0` stays 0: no `sw_rx_ns` substitution), and every sample yields at least one event.
pub fn check_md_basics<P: VenueProtocol>(
    samples: &[MdSample<'_>],
    rd: &RefData,
    ctx: &mut P::FeedCtx,
) -> Result<(), ContractFail> {
    let recs = decode_samples::<P>(samples, rd, ctx)?;
    for (s, recs) in samples.iter().zip(&recs) {
        let fail = |detail: String| ContractFail {
            contract: 3,
            sample: s.name.to_string(),
            detail,
        };
        let hdrs: Vec<&MarketHdr> = recs.iter().filter_map(MdRec::market_hdr).collect();
        if hdrs.is_empty() {
            return Err(fail("no market event".to_string()));
        }
        for h in hdrs {
            let got = (h.rx_tsc, h.hw_rx_ns, h.sw_rx_ns, h.rx_empty_gap_ns, h.conn);
            let want = (
                s.rx.rx_tsc,
                s.rx.hw_rx_ns,
                s.rx.sw_rx_ns,
                s.rx.rx_empty_gap_ns,
                s.rx.conn,
            );
            if got != want {
                return Err(fail(format!(
                    "MarketHdr (rx_tsc, hw_rx_ns, sw_rx_ns, rx_empty_gap_ns, conn) = {got:?}, RxMeta has {want:?}"
                )));
            }
        }
    }
    Ok(())
}
