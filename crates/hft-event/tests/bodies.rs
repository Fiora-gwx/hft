//! Every `Body` implementation of the crate (fixed bodies, Var and Multi prefixes), used by
//! EVT-005 (`kinds.rs`). Also built as its own (empty) test target.

#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods
)] // allow-list: test code, not a hot path
#![allow(dead_code)]

use hft_event::*;

/// `(KIND, SCHEMA_VERSION, size_of, form)`; form: 0 fixed, 1 var prefix, 2 multi prefix.
pub type BodyRow = (EventKind, u16, usize, u8);

fn row<B: Body>(form: u8) -> BodyRow {
    (B::KIND, B::SCHEMA_VERSION, size_of::<B>(), form)
}

pub fn bodies() -> Vec<BodyRow> {
    vec![
        row::<Bbo>(0),
        row::<L2Delta>(0),
        row::<Trade>(0),
        row::<Funding>(0),
        row::<MarkPrice>(0),
        row::<IndexPrice>(0),
        row::<Liquidation>(0),
        row::<OrderDecision>(0),
        row::<SendReport>(0),
        row::<OrderEvent>(0),
        row::<PositionReport>(0),
        row::<BalanceReport>(0),
        row::<FundingSettle>(0),
        row::<ExternalFill>(0),
        row::<ExecDecision>(0),
        row::<ChainHead>(0),
        row::<PoolQuote>(0),
        row::<SwapDecision>(0),
        row::<TxLifecycle>(0),
        row::<CmdResult>(0),
        row::<ReconcileReport>(0),
        row::<Timer>(0),
        row::<ConnState>(0),
        row::<LatencySample>(0),
        row::<Note>(0),
        row::<HostProfile>(0),
        row::<ClockStatus>(0),
        row::<SegmentRoll>(0),
        row::<EngineClock>(0),
        row::<L2BatchHdr>(1),
        row::<MarginReportHdr>(1),
        row::<AccountConfigReportHdr>(1),
        row::<AdviceHdr>(1),
        row::<L2SnapHdr>(2),
        row::<CmdHdr>(2),
        row::<BlobHdr>(2),
        row::<SnapshotHdr>(2),
        row::<RawHdr>(2),
    ]
}
