//! Optional chain bodies (0x0301–0x0304). `0x0310–0x033F` belongs to hft-ext-proto.
// zerocopy's derive helpers inside `event_body!` expansions carry no Debug impl.
#![allow(missing_debug_implementations)]

use hft_types::{AccountId, GroupId, InstId, Notional, Px, Qty, Rate, Side, StrategyId, Tsc, VenueId};

use crate::EventKind;
use crate::codes;
use crate::macros::{layout, wire_enum};
use crate::schema::FieldTy;

wire_enum!(
    /// `TxLifecycle.state` (state machine in hft-chain-evm).
    TxState: u8 = "TxState" { Sent = 1, IncludedOk = 2, IncludedReverted = 3, Dropped = 4, Unknown = 5, Finalized = 6 }
);

crate::event_body! {
    /// New node head (0x0301, Input).
    pub struct ChainHead : fixed(EventKind::CHAIN_HEAD, 1), FromBytes {
        pub chain: VenueId,
        pub source: u8 = FieldTy::Enum(&codes::CHAIN_HEAD_SOURCE),
        pub hflags: u8 = FieldTy::Bits(&codes::CHAIN_HFLAGS),
        pub _pad: [u8; 5],
        pub block: u64,
        pub block_ts_ns: i64,
        pub observed_ts_ns: i64,
        pub rx_tsc: Tsc,
        pub hash: [u8; 32],
        pub finalized: u64,
    }
}
layout!(ChainHead, 80, 8; chain @ 0, source @ 1, hflags @ 2, _pad @ 3, block @ 8, block_ts_ns @ 16,
    observed_ts_ns @ 24, rx_tsc @ 32, hash @ 40, finalized @ 72);

crate::event_body! {
    /// Pool quote per (route, size) per block (0x0302, Input).
    pub struct PoolQuote : fixed(EventKind::POOL_QUOTE, 1), FromBytes {
        pub quote_id: u64,
        pub block: u64,
        pub group: GroupId,
        pub route: u32,
        pub chain: VenueId,
        pub level: u8 = FieldTy::Enum(&codes::POOL_LEVEL),
        pub qflags: u8 = FieldTy::Bits(&codes::POOL_QFLAGS),
        pub _pad: [u8; 5],
        pub size: Qty,
        pub buy_px: Px,
        pub sell_px: Px,
        pub depth: Notional,
        pub gas_cost: Notional,
        pub fee: Rate,
        pub computed_ts_ns: i64,
        pub rx_tsc: Tsc,
    }
}
layout!(PoolQuote, 96, 8; quote_id @ 0, block @ 8, group @ 16, route @ 20, chain @ 24, level @ 25, qflags @ 26,
    _pad @ 27, size @ 32, buy_px @ 40, sell_px @ 48, depth @ 56, gas_cost @ 64, fee @ 72, computed_ts_ns @ 80,
    rx_tsc @ 88);

crate::event_body! {
    /// Swap decision (0x0303, Output).
    pub struct SwapDecision : fixed(EventKind::SWAP_DECISION, 1), TryFromBytes {
        pub intent_id: u64,
        pub quote_id: u64,
        pub trigger_seq: u64,
        pub decided_tsc: Tsc => measure,
        pub amount_in: Qty,
        pub min_out: Qty,
        pub amount_out: Qty,
        pub max_in: Qty,
        pub limit_px: Px,
        pub edge: Notional,
        pub max_bribe: Notional,
        pub group: GroupId,
        pub route: u32,
        pub reject: u32,
        /// 0 = no hedge leg.
        pub hedge_inst: InstId,
        pub strategy: StrategyId,
        pub account: AccountId,
        pub chain: VenueId,
        pub side: Side,
        pub valid_blocks: u8,
        pub exact: u8 = FieldTy::Enum(&codes::SWAP_EXACT),
        /// `Buy` when `hedge_inst = 0` (readers ignore it).
        pub hedge_side: Side,
        pub hedge_px_mode: u8 = FieldTy::Enum(&codes::HEDGE_PX_MODE),
        pub hedge_limit: Rate,
        pub sflags: u8 = FieldTy::Bits(&codes::SWAP_SFLAGS),
        pub _pad: [u8; 7],
    }
}
layout!(SwapDecision, 128, 8; intent_id @ 0, quote_id @ 8, trigger_seq @ 16, decided_tsc @ 24, amount_in @ 32,
    min_out @ 40, amount_out @ 48, max_in @ 56, limit_px @ 64, edge @ 72, max_bribe @ 80, group @ 88, route @ 92,
    reject @ 96, hedge_inst @ 100, strategy @ 104, account @ 105, chain @ 106, side @ 107, valid_blocks @ 108,
    exact @ 109, hedge_side @ 110, hedge_px_mode @ 111, hedge_limit @ 112, sflags @ 120, _pad @ 121);

crate::event_body! {
    /// Transaction lifecycle (0x0304, Input).
    pub struct TxLifecycle : fixed(EventKind::TX_LIFECYCLE, 1), TryFromBytes {
        pub intent_id: u64,
        pub state: TxState,
        pub builder: u8,
        pub account: AccountId,
        pub _pad: [u8; 5],
        pub block: u64,
        pub amount_in: Qty,
        pub amount_out: Qty,
        pub gas_used: u64,
        pub gas_cost: Qty,
        pub tx_hash: [u8; 32],
        pub observed_ts_ns: i64,
        pub reason: u16,
        pub tflags: u16,
        pub src_mask: u8,
        pub wallet: u8,
        pub _pad2: [u8; 2],
        pub bribe: Qty,
        pub rx_tsc: u64,
        pub sig_hi: [u8; 32],
    }
}
layout!(TxLifecycle, 152, 8; intent_id @ 0, state @ 8, builder @ 9, account @ 10, _pad @ 11, block @ 16,
    amount_in @ 24, amount_out @ 32, gas_used @ 40, gas_cost @ 48, tx_hash @ 56, observed_ts_ns @ 88, reason @ 96,
    tflags @ 98, src_mask @ 100, wallet @ 101, _pad2 @ 102, bribe @ 104, rx_tsc @ 112, sig_hi @ 120);
