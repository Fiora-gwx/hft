//! Decode output sinks (README "解码输出 sink"): implemented by the caller (hft-venue ring writers,
//! `testkit::VecSink`, mock-exchange); venue crates only call them. M1.5 part: `MdSink`.

use hft_event::{Bbo, Funding, IndexPrice, L2BatchHdr, L2SnapshotPart, Liquidation, MarkPrice, Trade};
use hft_types::{Px, Qty, Side};

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
