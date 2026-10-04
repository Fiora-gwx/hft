//! Control bodies (0x0402 `CmdResult`, 0x0404 `ReconcileReport`) and `CapRecord`.
// zerocopy's derive helpers inside `event_body!` expansions carry no Debug impl.
#![allow(missing_debug_implementations)]

use hft_types::{AccountId, AssetId, CapSource, InstId, Notional, Qty, SlotId, VenueId};

use crate::EventKind;
use crate::cmd::{CmdKind, CmdScope};
use crate::codes;
use crate::macros::layout;
use crate::schema::FieldTy;

crate::event_body! {
    /// Preview block of a `CmdResult` (64 B); all zero = not provided.
    pub struct CmdPreview : plain, FromBytes {
        pub target_account: AccountId,
        pub other_account: AccountId,
        pub target_mode: u8,
        pub other_mode: u8,
        pub target_kind: u8 = FieldTy::Enum(&codes::ACCOUNT_KIND),
        pub _pad: [u8; 3],
        pub affected_cells: u32,
        pub open_orders: u32,
        pub active_caps: u32,
        pub cap_sources: u32,
        /// v3.
        pub reset_groups: u32,
        /// v3.
        pub no_open_ms: u32,
        pub wo_venue: VenueId,
        pub wo_leg_ix: u8,
        pub _pad2: u16,
        pub wo_inst: InstId,
        pub wo_qty: Qty,
        pub _rsv: [u64; 2],
    }
}
layout!(CmdPreview, 64, 8; target_account @ 0, other_account @ 1, target_mode @ 2, other_mode @ 3,
    target_kind @ 4, _pad @ 5, affected_cells @ 8, open_orders @ 12, active_caps @ 16, cap_sources @ 20,
    reset_groups @ 24, no_open_ms @ 28, wo_venue @ 32, wo_leg_ix @ 33, _pad2 @ 34, wo_inst @ 36, wo_qty @ 40, _rsv @ 48);

crate::event_body! {
    /// Result of a command that reached the sequencer (0x0402, Output).
    pub struct CmdResult : fixed(EventKind::CMD_RESULT, 3), FromBytes {
        pub cmd_id: [u8; 16],
        pub cmd_seq: u64,
        pub cmd: CmdKind,
        /// control-plane result code.
        pub status: u8,
        pub _pad: u8,
        pub detail: u32,
        pub engine_epoch: u32,
        pub effect_count: u32,
        pub refdata_version: u64,
        pub risk_version: u64,
        pub params_version: u64,
        pub ctl_state_version: u64,
        pub rflags: u32 = FieldTy::Bits(&codes::CMD_RFLAGS),
        pub _pad2: u32,
        pub preview: CmdPreview,
    }
}
layout!(CmdResult, 144, 8; cmd_id @ 0, cmd_seq @ 16, cmd @ 24, status @ 26, _pad @ 27, detail @ 28,
    engine_epoch @ 32, effect_count @ 36, refdata_version @ 40, risk_version @ 48, params_version @ 56,
    ctl_state_version @ 64, rflags @ 72, _pad2 @ 76, preview @ 80);

impl CmdResult {
    pub const PREVIEW: u32 = 1;
    pub const NEEDS_CONFIRM: u32 = 2;
    pub const PREVIEW_TTL: u32 = 4;
}

crate::event_body! {
    /// One reconciliation difference (0x0404, Input).
    pub struct ReconcileReport : fixed(EventKind::RECONCILE_REPORT, 3), FromBytes {
        pub report_id: u64,
        pub venue: VenueId,
        pub account: AccountId,
        pub item: u8 = FieldTy::Enum(&codes::RECON_ITEM),
        pub severity: u8 = FieldTy::Enum(&codes::RECON_SEVERITY),
        pub inst: InstId,
        pub asset: AssetId,
        pub idx: u16,
        pub count: u16,
        pub _pad: u16,
        pub engine_qty: Qty,
        pub exch_qty: Qty,
        pub diff: Notional,
        pub exch_ts_ns: i64,
        pub slot: SlotId,
        pub order_status: u8 = FieldTy::Enum(&codes::RECON_ORDER_STATUS),
        pub history_checked: u8,
        pub _pad2: [u8; 2],
        pub view_seq: u64,
        pub r#gen: u32,
        pub epoch: u16,
        pub _pad3: u16,
        /// v2.
        pub exch_amount: Notional,
        /// v2.
        pub engine_amount: Notional,
        /// v3.
        pub sflags: u16 = FieldTy::Bits(&codes::RECON_SFLAGS),
        /// v3.
        pub skipped_inflight: u16,
        pub _pad4: u32,
    }
}
layout!(ReconcileReport, 104, 8; report_id @ 0, venue @ 8, account @ 9, item @ 10, severity @ 11, inst @ 12,
    asset @ 16, idx @ 18, count @ 20, _pad @ 22, engine_qty @ 24, exch_qty @ 32, diff @ 40, exch_ts_ns @ 48,
    slot @ 56, order_status @ 60, history_checked @ 61, _pad2 @ 62, view_seq @ 64, r#gen @ 72, epoch @ 76,
    _pad3 @ 78, exch_amount @ 80, engine_amount @ 88, sflags @ 96, skipped_inflight @ 98, _pad4 @ 100);

impl ReconcileReport {
    pub const USERSTREAM_DOWN: u16 = 1;
    pub const TRUNCATED: u16 = 2;
    pub const HISTORY_UNCHECKED: u16 = 4;
    pub const AUTO_EXTERNAL: u16 = 8;
}

crate::event_body! {
    /// Mode cap (48 B): snapshot section 4 rows and `Note{CapSet/CapCleared}.v[0..6]`.
    pub struct CapRecord : plain, FromBytes {
        pub source: CapSource,
        /// Kill: `KillLevel`; MarginStress: tier 1..3; else 0.
        pub level: u8,
        pub cflags: u8 = FieldTy::Bits(&codes::CAP_CFLAGS),
        pub _pad: [u8; 5],
        pub scope: CmdScope,
        pub set_seq: u64,
        pub params: [u64; 2],
    }
}
layout!(CapRecord, 48, 8; source @ 0, level @ 1, cflags @ 2, _pad @ 3, scope @ 8, set_seq @ 24, params @ 32);

impl CapRecord {
    pub const HARD: u8 = 1;
    pub const CONDITIONAL: u8 = 2;
    pub const NON_CLEARABLE: u8 = 4;
}
