//! Warm account bootstrap wire contract. No I/O; all decimal conversion is exact.
use crate::{Credential, DecodeErr, ProtoErr, RestReqBuf, VenueCfg, VenueProtocol};
use hft_event::{
    AccountConfigReportHdr, BalanceReport, LeverageItem, MarginItem, MarginReportHdr, PositionReport,
};
use hft_refdata::RefData;
use hft_types::{AccountId, InstId};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Query {
    Instruments,
    Settings,
    Permissions,
    SymbolSettings,
    Balances,
    Positions,
    OpenOrders { cursor: String },
    Cancel { symbol: String, order_id: String },
    ListenKey,
    Keepalive,
    DeadMan { symbol: String, timeout_ms: u32 },
    FundingInfo,
    FundingSchedule { symbol: String },
}
impl Query {
    pub fn optional_funding(&self) -> bool {
        matches!(self, Self::FundingInfo | Self::FundingSchedule { .. })
    }
}
#[derive(Clone, Copy, Debug)]
pub struct FundingSchedule {
    pub inst: InstId,
    pub next_ms: i64,
    pub interval_s: u32,
}
#[derive(Clone, Debug)]
pub struct OpenOrder {
    pub symbol: String,
    pub order_id: String,
}
#[derive(Default, Debug)]
pub struct State {
    pub config: Option<AccountConfigReportHdr>,
    pub leverage: Vec<LeverageItem>,
    pub balances: Vec<BalanceReport>,
    pub positions: Vec<PositionReport>,
    pub margin: Option<MarginReportHdr>,
    pub margin_items: Vec<MarginItem>,
    pub orders: Vec<OpenOrder>,
    pub cursor: String,
    pub funding: Vec<hft_event::FundingSettle>,
    /// Public REST observations used only to seed the feed decoder before its first WS frame.
    pub funding_intervals: Vec<(InstId, u32)>,
    pub funding_schedule: Vec<FundingSchedule>,
}

impl State {
    /// Only actual nonzero positions contribute margin items. No ADL observation means unknown.
    pub fn position_margin_items(&mut self) {
        self.margin_items = self
            .positions
            .iter()
            .filter(|p| p.qty.0 != 0)
            .map(|p| MarginItem {
                inst: p.inst,
                adl_quantile: 255,
                _pad: [0; 3],
                liq_px: p.liq_px,
            })
            .collect();
    }
}

/// Separate warm extension: implementations may allocate; no hot decoder depends on it.
pub trait AccountProtocol: VenueProtocol {
    type Funding: crate::FundingHistoryProtocol + Send;
    fn funding_bootstrap_queries(_rd: &RefData) -> Vec<Query> {
        Vec::new()
    }
    fn seed_funding(_ctx: &mut Self::FeedCtx, schedules: &[FundingSchedule]) -> Result<(), DecodeErr> {
        if schedules.is_empty() {
            Ok(())
        } else {
            Err(DecodeErr::Shape)
        }
    }
    fn account_request(
        cfg: &VenueCfg,
        cred: &Credential,
        query: &Query,
        now_ms: i64,
        out: &mut RestReqBuf,
    ) -> Result<(), ProtoErr>;
    fn account_decode(
        rd: &RefData,
        account: AccountId,
        query: &Query,
        status: u16,
        body: &[u8],
        now_ms: i64,
        state: &mut State,
    ) -> Result<(), DecodeErr>;
    /// Environment-aware warm validation. The legacy entry remains production-strict.
    #[allow(clippy::too_many_arguments)]
    fn account_decode_env(
        _cfg: &VenueCfg,
        rd: &RefData,
        account: AccountId,
        query: &Query,
        status: u16,
        body: &[u8],
        now_ms: i64,
        state: &mut State,
    ) -> Result<(), DecodeErr> {
        Self::account_decode(rd, account, query, status, body, now_ms, state)
    }
}

pub fn instrument(rd: &RefData, venue: hft_types::VenueId, symbol: &str) -> Result<InstId, DecodeErr> {
    rd.insts_of_venue(venue)
        .iter()
        .copied()
        .find(|i| rd.inst_meta(*i).symbol.as_ref() == symbol)
        .ok_or(DecodeErr::Shape)
}
pub fn decimal(s: &str, exp: i8) -> Result<i64, DecodeErr> {
    hft_types::parse_exact(s.as_bytes(), exp).map_err(number_error)
}
fn number_error(e: hft_types::FxErr) -> DecodeErr {
    if e == hft_types::FxErr::OVERFLOW {
        DecodeErr::Overflow
    } else {
        DecodeErr::Number
    }
}
/// Exchange amounts carry more decimals than the Notional ABI; round once at that boundary.
pub fn notional(s: &str) -> Result<i64, DecodeErr> {
    hft_types::parse_round(s.as_bytes(), -6, hft_types::RoundMode::HalfEven).map_err(number_error)
}
/// Average/estimated prices need not fall on the order tick or wire decimal grid.
pub fn average_price(s: &str, exp: i8) -> Result<i64, DecodeErr> {
    hft_types::parse_round(s.as_bytes(), exp, hft_types::RoundMode::HalfEven).map_err(number_error)
}
pub fn safe_token(s: &str) -> bool {
    !s.is_empty()
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}

/// Require an exact instrument rule match; a configuration mismatch cannot be silently rounded.
pub fn rules(
    rd: &RefData,
    inst: InstId,
    tick: &str,
    lot: &str,
    min_qty: &str,
    min_notional: &str,
) -> Result<(), DecodeErr> {
    let h = rd.inst(inst);
    if decimal(tick, h.px_wire_exp)? != h.tick
        || decimal(lot, h.qty_wire_exp)? != h.lot
        || decimal(min_qty, h.qty_wire_exp)? != h.min_qty.0
        || decimal(min_notional, -6)? != h.min_notional.0
    {
        return Err(DecodeErr::Shape);
    }
    Ok(())
}

pub fn asset(rd: &RefData, name: &str) -> Option<hft_types::AssetId> {
    // RDB1 asset IDs are dense; NONE terminates the table.
    (1..=u16::MAX)
        .map(hft_types::AssetId)
        .take_while(|a| rd.asset(*a).id.0 != 0)
        .find(|a| rd.asset(*a).name.as_ref() == name)
}
