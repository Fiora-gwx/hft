//! Decode output sinks (README "解码输出 sink"): implemented by the caller (hft-venue ring writers,
//! `testkit::VecSink`, mock-exchange); venue crates only call them.

use hft_event::{
    AccountConfigReportHdr, BalanceReport, Bbo, ExternalFill, Funding, FundingSettle, IndexPrice, L2BatchHdr,
    L2SnapshotPart, LeverageItem, Liquidation, MarginItem, MarginReportHdr, MarkPrice, OrderEvent,
    PositionReport, Trade,
};
use hft_types::{ClientOrderId, Px, Qty, Side, SlotId};

/// Market-data sink.
pub trait MdSink {
    /// Bbo route → `bbo-<venue>` ring.
    fn bbo(&mut self, e: &Bbo);
    /// One incremental depth message = one `L2Batch`: `l2_begin` → N × `l2_level` → `l2_end`.
    fn l2_begin(&mut self, h: &L2BatchHdr);
    /// `qty = 0` deletes the level.
    fn l2_level(&mut self, side: Side, px: Px, qty: Qty);
    fn l2_end(&mut self);
    /// Snapshot part (TopN stream or REST snapshot).
    fn l2_snapshot(&mut self, part: &L2SnapshotPart<'_>);
    fn trade(&mut self, e: &Trade);
    fn mark(&mut self, e: &MarkPrice);
    fn index(&mut self, e: &IndexPrice);
    fn funding(&mut self, e: &Funding);
    /// Public liquidation stream (not this account's liquidation).
    fn liquidation(&mut self, e: &Liquidation);
}

/// Answer of `ReportSink::exch_id`.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum ExchIdHint {
    Resolved { epoch: u16, slot: SlotId, r#gen: u32 },
    Recorded,
    Unknown,
}

/// Report sink (gw thread; tests: `VecSink`).
pub trait ReportSink {
    /// A submitted position-close on this instrument awaits exchange order identity.
    /// Unrecognized fills may be held by exchange id; this is never ownership evidence.
    fn pending_position_close(&self, _inst: hft_types::InstId) -> bool {
        false
    }
    /// `cum_filled`/`avg_px` (contract 9) and `epoch` (contract 13) filled; late fees as `FeeOnly`
    /// (contract 12); `slot` may be `SlotId::UNRESOLVED`.
    fn order(&mut self, e: &OrderEvent);
    /// Checks only.
    fn position(&mut self, e: &PositionReport);
    /// Checks only.
    fn balance(&mut self, e: &BalanceReport);
    /// Per `funding_settle` capability (contract 14), deduplicated (contract 22).
    fn funding_settle(&mut self, e: &FundingSettle);
    /// Contract 18.
    fn external_fill(&mut self, e: &ExternalFill);
    /// Contract 17; the runtime splits the items per the hft-event rules.
    fn margin(&mut self, hdr: &MarginReportHdr, items: &[MarginItem]);
    /// Contract 16; as above.
    fn account_config(&mut self, hdr: &AccountConfigReportHdr, lev: &[LeverageItem]);
    /// Exchange order id ↔ slot (the runtime's `IdTable`): `owner = Some(id)` records (an ack with
    /// both ids), `None` looks up (a report with the exchange id only).
    fn exch_id(&mut self, key: &[u8], owner: Option<&ClientOrderId>) -> ExchIdHint;
}
