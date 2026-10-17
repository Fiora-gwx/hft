//! Execution bodies (0x02xx), `OrderFlags`, `TargetOriginWire` and the execution wire enums.
// zerocopy's derive helpers inside `event_body!` expansions carry no Debug impl.
#![allow(missing_debug_implementations)]

use hft_types::{
    AccountId, AssetId, ConnId, GroupId, InstId, Notional, Px, Qty, Rate, Side, SlotId, StrategyId, Tif, Tsc,
    VenueId,
};

use crate::EventKind;
use crate::codes;
use crate::macros::{layout, wire_enum};
use crate::schema::FieldTy;

wire_enum!(
    /// `OrderDecision.action`.
    OrderAction: u8 = "OrderAction" { New = 1, Cancel = 2, Amend = 3, CancelAll = 4, Query = 5 }
);
wire_enum!(
    /// `SendReport.status`.
    SendStatus: u8 = "SendStatus" {
        Written = 1, KillBlocked = 2, RateLimited = 3, WriteError = 4, NoConn = 5, EncodeError = 6, RingFull = 7,
        QueryUnsupported = 8,
    }
);
wire_enum!(
    /// `GwCmd.op` / `SendReport.op`; 1..=5 equal `OrderAction`.
    GwOp: u8 = "GwOp" {
        Place = 1, Cancel = 2, Amend = 3, CancelAll = 4, Query = 5, DeadManRefresh = 6, Rotate = 7, ResetRate = 8,
        Resubscribe = 9, SetEpoch = 10, SessionKeepalive = 11, MirrorRestore = 12, SimReset = 13, SimTransfer = 14,
        SimFault = 15,
    }
);
wire_enum!(
    /// `OrderEvent.what`.
    OrderEventKind: u8 = "OrderEventKind" {
        Ack = 1, Reject = 2, PartialFill = 3, Fill = 4, CancelAck = 5, CancelReject = 6, AmendAck = 7,
        AmendReject = 8, Expired = 9, QueryResult = 10, OutcomeUnknown = 11, FeeOnly = 12,
    }
);
wire_enum!(
    /// `ExternalFill.cause`.
    ExternalCause: u8 = "ExternalCause" { Liquidation = 1, Adl = 2, Manual = 3, Delivery = 4, Unknown = 5 }
);
wire_enum!(
    /// `ExecDecision.origin` wire code; conversion to the SDK view lives in the SDK.
    TargetOriginWire: u8 = "TargetOrigin" {
        Strategy = 1, EngineMarginStress = 2, EngineExternal = 3, EngineCommand = 4, EngineKillFlatten = 5,
        EngineAutoFix = 6,
    }
);

/// `OrderDecision.oflags` / `GwCmd.oflags` bits (only defined here). Bits 8..15 never go on the wire.
#[repr(transparent)]
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, Default)]
pub struct OrderFlags(pub u16);

impl OrderFlags {
    pub const REDUCE_ONLY: Self = OrderFlags(1 << 0);
    pub const POST_ONLY: Self = OrderFlags(1 << 1);
    pub const LINKED: Self = OrderFlags(1 << 2);
    pub const HEDGE_FIX: Self = OrderFlags(1 << 3);
    pub const RISK_REDUCING: Self = OrderFlags(1 << 4);
    pub const SMOKE: Self = OrderFlags(1 << 5);
    /// Engine Kill/flatten tail: venue position-close, with exact remaining quantity for accounting.
    pub const POSITION_CLOSE: Self = OrderFlags(1 << 6);
    /// Bits allowed on the wire (7 reserved for future wire flags).
    pub const WIRE_MASK: Self = OrderFlags(0x00FF);

    #[inline]
    pub const fn contains(self, o: Self) -> bool {
        self.0 & o.0 == o.0
    }
}

impl core::ops::BitOr for OrderFlags {
    type Output = Self;
    #[inline]
    fn bitor(self, o: Self) -> Self {
        OrderFlags(self.0 | o.0)
    }
}

crate::event_body! {
    /// One per intent (0x0201, Output), released, rejected or held by Shadow.
    pub struct OrderDecision : fixed(EventKind::ORDER_DECISION, 1), TryFromBytes {
        pub trigger_seq: u64,
        pub decided_tsc: Tsc => measure,
        pub px: Px,
        pub qty: Qty,
        pub group: GroupId,
        pub inst: InstId,
        pub slot: SlotId,
        pub r#gen: u32,
        pub target_slot: SlotId,
        pub reject: u32,
        pub strategy: StrategyId,
        pub venue: VenueId,
        pub account: AccountId,
        pub action: OrderAction,
        pub side: Side,
        pub tif: Tif,
        pub oflags: u16 = FieldTy::Bits(&codes::ORDER_FLAGS),
        pub reason: u32,
        pub intent_no: u32,
        pub req_px: Px,
        pub link_role: u8 = FieldTy::Enum(&codes::LINK_ROLE),
        pub _pad: [u8; 7],
    }
}
layout!(OrderDecision, 88, 8; trigger_seq @ 0, decided_tsc @ 8, px @ 16, qty @ 24, group @ 32, inst @ 36,
    slot @ 40, r#gen @ 44, target_slot @ 48, reject @ 52, strategy @ 56, venue @ 57, account @ 58, action @ 59,
    side @ 60, tif @ 61, oflags @ 62, reason @ 64, intent_no @ 68, req_px @ 72, link_role @ 80, _pad @ 81);

crate::event_body! {
    /// Send result (0x0202, Input) for each order-class `GwCmd`.
    pub struct SendReport : fixed(EventKind::SEND_REPORT, 1), TryFromBytes {
        pub decision_seq: u64,
        pub sent_tsc: Tsc => measure,
        pub slot: SlotId,
        pub r#gen: u32,
        pub target_slot: SlotId,
        pub target_gen: u32,
        pub venue: VenueId,
        pub account: AccountId,
        pub conn: ConnId,
        pub status: SendStatus,
        pub op: GwOp,
        pub _pad: [u8; 3],
        pub os_errno: i32,
        pub _pad2: u32,
    }
}
layout!(SendReport, 48, 8; decision_seq @ 0, sent_tsc @ 8, slot @ 16, r#gen @ 20, target_slot @ 24,
    target_gen @ 28, venue @ 32, account @ 33, conn @ 34, status @ 35, op @ 36, _pad @ 37, os_errno @ 40, _pad2 @ 44);

crate::event_body! {
    /// Order report (0x0203, Input).
    pub struct OrderEvent : fixed(EventKind::ORDER_EVENT, 4), TryFromBytes {
        pub venue: VenueId,
        pub account: AccountId,
        pub what: OrderEventKind,
        /// By `what`: QueryResult → `codes::ORDER_STATUS`; Cancel/AmendReject → `codes::REJECT_CLASS`; else 0.
        pub detail: u8,
        pub slot: SlotId,
        pub r#gen: u32,
        pub inst: InstId,
        pub px: Px,
        pub qty: Qty,
        pub cum_filled: Qty,
        pub fee: Qty,
        pub fee_asset: AssetId,
        pub epoch: u16,
        pub reject_code: u32,
        pub exch_order_id: u64,
        pub ex_ts_ns: i64,
        pub hw_rx_ns: i64,
        pub rx_tsc: u64,
        pub avg_px: Px,
        /// v2: cumulative quantity covered by `cash_sum`; 0 = settlement facts unavailable.
        pub cash_qty: Qty,
        /// v2: cumulative realized perpetual cash in the quote asset's qty_exp, income positive;
        /// excludes fees and funding. Known zero has `cash_qty = cum_filled > 0`.
        pub cash_sum: Qty,
        /// v3: exchange id of this PartialFill/Fill execution, using ExternalFill's id
        /// normalization; 0 = absent/nonpositive or a non-execution report. This identifies
        /// only this execution, never the whole cumulative quantity or cash prefix.
        pub trade_id: u64,
        /// v4: quantity of this execution covered by `fill_cash_amount`; 0 = unavailable.
        /// Applies only to PartialFill/Fill and must equal `qty`, never `cum_filled` by inference.
        pub fill_cash_qty: Qty,
        /// v4: this execution's realized perpetual cash in quote qty_exp; excludes fees/funding.
        /// Known zero has fill_cash_qty > 0. Mutually exclusive with cumulative cash facts.
        pub fill_cash_amount: Qty,
    }
}
layout!(OrderEvent, 136, 8; venue @ 0, account @ 1, what @ 2, detail @ 3, slot @ 4, r#gen @ 8, inst @ 12,
    px @ 16, qty @ 24, cum_filled @ 32, fee @ 40, fee_asset @ 48, epoch @ 50, reject_code @ 52,
    exch_order_id @ 56, ex_ts_ns @ 64, hw_rx_ns @ 72, rx_tsc @ 80, avg_px @ 88, cash_qty @ 96, cash_sum @ 104, trade_id @ 112,
    fill_cash_qty @ 120, fill_cash_amount @ 128);

crate::event_body! {
    /// Exchange position push (0x0204, Input; check only).
    pub struct PositionReport : fixed(EventKind::POSITION_REPORT, 1), FromBytes {
        pub venue: VenueId,
        pub account: AccountId,
        pub _pad: u16,
        pub inst: InstId,
        pub qty: Qty,
        pub entry_px: Px,
        pub upnl: Notional,
        pub liq_px: Px,
        pub ex_ts_ns: i64,
        pub rx_tsc: Tsc,
    }
}
layout!(PositionReport, 56, 8; venue @ 0, account @ 1, _pad @ 2, inst @ 4, qty @ 8, entry_px @ 16, upnl @ 24,
    liq_px @ 32, ex_ts_ns @ 40, rx_tsc @ 48);

crate::event_body! {
    /// Balance push (0x0205, Input).
    pub struct BalanceReport : fixed(EventKind::BALANCE_REPORT, 1), FromBytes {
        pub venue: VenueId,
        pub account: AccountId,
        pub asset: AssetId,
        pub _pad: u32,
        pub free: Qty,
        pub locked: Qty,
        pub ex_ts_ns: i64,
        pub rx_tsc: Tsc,
    }
}
layout!(BalanceReport, 40, 8; venue @ 0, account @ 1, asset @ 2, _pad @ 4, free @ 8, locked @ 16,
    ex_ts_ns @ 24, rx_tsc @ 32);

crate::event_body! {
    /// Realized funding (0x0206): pushed (Input) or synthesized (`synth = 1`, Output).
    pub struct FundingSettle : fixed(EventKind::FUNDING_SETTLE, 2), FromBytes {
        pub venue: VenueId,
        pub account: AccountId,
        pub _pad: u16,
        /// `INST_NONE` = account-level total only.
        pub inst: InstId,
        pub amount: Notional,
        pub rate: Rate,
        pub pos_qty: Qty,
        pub settle_ts_ns: i64,
        pub rx_tsc: Tsc,
        pub synth: u8,
        pub _pad2: [u8; 7],
    }
}
layout!(FundingSettle, 56, 8; venue @ 0, account @ 1, _pad @ 2, inst @ 4, amount @ 8, rate @ 16, pos_qty @ 24,
    settle_ts_ns @ 32, rx_tsc @ 40, synth @ 48, _pad2 @ 49);

crate::event_body! {
    /// Fill on this account not caused by this system (0x0207, Input).
    pub struct ExternalFill : fixed(EventKind::EXTERNAL_FILL, 3), TryFromBytes {
        pub venue: VenueId,
        pub account: AccountId,
        pub side: Side,
        pub cause: ExternalCause,
        pub inst: InstId,
        pub qty: Qty,
        pub px: Px,
        pub fee: Qty,
        pub fee_asset: AssetId,
        pub _pad: [u8; 6],
        pub exch_order_id: u64,
        pub trade_id: u64,
        pub ex_ts_ns: i64,
        pub rx_tsc: Tsc,
        /// v2: cumulative filled raw after this fill when `trade_id = 0`.
        pub fill_seq: u64,
        /// v3: quantity covered by `cash_amount`; `qty > 0` when known, 0 when unavailable.
        pub cash_qty: Qty,
        /// v3: this execution's realized perpetual cash in the quote asset's qty_exp, income
        /// positive; excludes fees and funding. It is not an account balance difference.
        pub cash_amount: Qty,
    }
}
layout!(ExternalFill, 96, 8; venue @ 0, account @ 1, side @ 2, cause @ 3, inst @ 4, qty @ 8, px @ 16, fee @ 24,
    fee_asset @ 32, _pad @ 34, exch_order_id @ 40, trade_id @ 48, ex_ts_ns @ 56, rx_tsc @ 64, fill_seq @ 72, cash_qty @ 80, cash_amount @ 88);

crate::event_body! {
    /// `MarginReport` (0x0208, Var) prefix; followed by `[MarginItem; n]`.
    pub struct MarginReportHdr : var(EventKind::MARGIN_REPORT, 1, 16_384), FromBytes {
        pub venue: VenueId,
        pub account: AccountId,
        pub source: u8 = FieldTy::Enum(&codes::MARGIN_SOURCE),
        pub mflags: u8 = FieldTy::Bits(&codes::MARGIN_MFLAGS),
        pub n: u16,
        pub _pad: u16,
        pub margin_balance: Notional,
        pub maint_margin: Notional,
        pub avail_margin: Notional,
        pub margin_ratio: Rate,
        pub ex_ts_ns: i64,
        pub rx_tsc: Tsc,
        pub _rsv: u64,
    }
}
layout!(MarginReportHdr, 64, 8; venue @ 0, account @ 1, source @ 2, mflags @ 3, n @ 4, _pad @ 6,
    margin_balance @ 8, maint_margin @ 16, avail_margin @ 24, margin_ratio @ 32, ex_ts_ns @ 40, rx_tsc @ 48, _rsv @ 56);

impl MarginReportHdr {
    pub const MARGIN_CALL: u8 = 1;
    pub const ITEMS_TRUNCATED: u8 = 2;
    /// Upper bound of `n`.
    pub const MAX_ITEMS: u16 = 1024;
}

crate::event_body! {
    /// One perpetual position of a `MarginReport` (16 B).
    pub struct MarginItem : plain, FromBytes {
        pub inst: InstId,
        /// 0..=100 (higher = earlier ADL); 255 = unknown.
        pub adl_quantile: u8,
        pub _pad: [u8; 3],
        /// 0 = not provided.
        pub liq_px: Px,
    }
}
layout!(MarginItem, 16, 8; inst @ 0, adl_quantile @ 4, _pad @ 5, liq_px @ 8);

crate::event_body! {
    /// `AccountConfigReport` (0x0209, Var) prefix; followed by `[LeverageItem; n_lev]`.
    pub struct AccountConfigReportHdr : var(EventKind::ACCOUNT_CONFIG_REPORT, 1, 16_384), FromBytes {
        pub venue: VenueId,
        pub account: AccountId,
        pub position_mode: u8 = FieldTy::Enum(&codes::POSITION_MODE),
        pub margin_mode: u8 = FieldTy::Enum(&codes::MARGIN_MODE),
        pub multi_asset: u8 = FieldTy::Enum(&codes::MULTI_ASSET),
        pub source: u8 = FieldTy::Enum(&codes::ACR_SOURCE),
        pub n_lev: u16,
        pub mismatches: u32 = FieldTy::Bits(&codes::CFG_MISMATCH),
        pub rflags: u16 = FieldTy::Bits(&codes::ACR_RFLAGS),
        pub uid_keys: u8 = FieldTy::Bits(&codes::UID_KEYS),
        pub _pad: u8,
        /// ASCII, right-padded with 0.
        pub venue_uid: [u8; 24],
        pub ex_ts_ns: i64,
        pub rx_tsc: Tsc,
        pub _rsv: u64,
    }
}
layout!(AccountConfigReportHdr, 64, 8; venue @ 0, account @ 1, position_mode @ 2, margin_mode @ 3,
    multi_asset @ 4, source @ 5, n_lev @ 6, mismatches @ 8, rflags @ 12, uid_keys @ 14, _pad @ 15, venue_uid @ 16,
    ex_ts_ns @ 40, rx_tsc @ 48, _rsv @ 56);

impl AccountConfigReportHdr {
    /// Upper bound of `n_lev`.
    pub const MAX_ITEMS: u16 = 1024;
}

crate::event_body! {
    /// Effective leverage of one perpetual inst (16 B); `leverage` 0 = unreadable.
    pub struct LeverageItem : plain, FromBytes {
        pub inst: InstId,
        pub _pad: u32,
        pub leverage: Rate,
    }
}
layout!(LeverageItem, 16, 8; inst @ 0, _pad @ 4, leverage @ 8);

crate::event_body! {
    /// Execution-layer decision (0x020A, Output).
    pub struct ExecDecision : fixed(EventKind::EXEC_DECISION, 1), FromBytes {
        pub trigger_seq: u64,
        pub set_seq: u64,
        pub group: GroupId,
        pub tag: u32,
        pub owner: StrategyId,
        pub account: AccountId,
        pub action: u8 = FieldTy::Enum(&codes::EXEC_ACTION),
        pub algo: u8 = FieldTy::Enum(&codes::EXEC_ALGO),
        pub origin: u8 = FieldTy::Enum(&TargetOriginWire::DESC),
        pub reason: u8 = FieldTy::Enum(&codes::TARGET_REASON),
        pub urgency: u8 = FieldTy::Enum(&codes::URGENCY),
        pub state_from: u8 = FieldTy::Enum(&codes::TARGET_STATE),
        pub state_to: u8 = FieldTy::Enum(&codes::TARGET_STATE),
        /// 0xFF = none.
        pub leg_ix: u8,
        /// 0xFF = none.
        pub first_leg_ix: u8,
        /// `Side` code; 0 = none.
        pub side: u8,
        pub eflags: u8 = FieldTy::Bits(&codes::EXEC_EFLAGS),
        pub _pad: [u8; 3],
        pub detail: u32,
        pub gate: u32,
        pub exec_order_id: u32,
        pub _pad2: u32,
        pub qty: Qty,
        pub px: Px,
        pub target_qty: Qty,
        pub pos_qty: Qty,
        pub inflight_qty: Qty,
        pub depth_qty: Qty,
    }
}
layout!(ExecDecision, 104, 8; trigger_seq @ 0, set_seq @ 8, group @ 16, tag @ 20, owner @ 24, account @ 25,
    action @ 26, algo @ 27, origin @ 28, reason @ 29, urgency @ 30, state_from @ 31, state_to @ 32, leg_ix @ 33,
    first_leg_ix @ 34, side @ 35, eflags @ 36, _pad @ 37, detail @ 40, gate @ 44, exec_order_id @ 48, _pad2 @ 52,
    qty @ 56, px @ 64, target_qty @ 72, pos_qty @ 80, inflight_qty @ 88, depth_qty @ 96);
