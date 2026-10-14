//! Warm, read-only reconciliation queries. Decoders validate a whole page before calling the sink.
use hft_event::{BalanceReport, ExternalCause, FundingSettle, OrderEvent, PositionReport};
use hft_types::{AssetId, ClientOrderId, InstId, Px, Qty, Side};

#[repr(u8)]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum RoKind {
    OpenOrders = 1,
    Fills = 2,
    FundingFlow = 3,
    Balance = 4,
    Positions = 5,
}
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct RoQuery {
    pub kind: RoKind,
    pub inst: InstId,
    pub cursor: u64,
    pub since_ms: i64,
    pub limit: u16,
}
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct RoPage {
    pub rows: u32,
    pub next_cursor: u64,
}
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct FillRow {
    pub inst: InstId,
    pub side: Side,
    pub qty: Qty,
    pub px: Px,
    pub fee: Qty,
    pub fee_asset: AssetId,
    pub exch_order_id: u64,
    pub trade_id: u64,
    pub ex_ts_ns: i64,
    pub cid: Option<ClientOrderId>,
    pub cause: Option<ExternalCause>,
}
pub trait RoSink {
    fn open_order(&mut self, e: &OrderEvent);
    fn fill(&mut self, r: &FillRow);
    fn funding(&mut self, e: &FundingSettle);
    fn balance(&mut self, e: &BalanceReport);
    fn position(&mut self, e: &PositionReport);
}
