//! The command path (README "命令路径"), `ConfigBlob` intake, `ReconcileReport` grouping
//! (README "process" ReconcileReport row), the `Sys` timers, funding synthesis and the Drain
//! state machine (README "Drain").

use std::collections::VecDeque;

use hft_event::{
    BlobRef, CmdHdr, CmdPreview, CmdResult, CmdScope, ControlCmd, EventKind, NoteLevel, ReconcileReport,
    RecordFlags,
};
use hft_ledger::{AdjustDelta, AdjustKind, FundingRecon, FundingReconItem, TransferDelta, TransferKind};
use hft_refdata::{AccountKind, RefData};
use hft_risk::mode::{GateEvent, KillCmd, ModeErr, ModeInput, ReconItemKey, StorageKind, VenueNoticeCmd};
use hft_risk::{AccountScope, ReconcileGroup};
use hft_strategy_sdk::StrategySet;
use hft_types::{
    AccountId, CapSource, GROUP_ALL, GROUP_NONE, GroupId, InstId, KillLevel, Qty, RunMode, StrategyId, Tsc,
    VenueId,
};
use zerocopy::{FromBytes, Immutable, IntoBytes, KnownLayout};

use crate::Driver;
use crate::codes::{detail, note, status};
use crate::engine::{Emit, Engine};
use crate::process::Pending;
use crate::strategies::{Direct, Instances};
use crate::types::{DoneCause, FatalReason, InputSlot, OutRing, StepOutcome};

type R<T> = Result<T, FatalReason>;

pub const SECTION_ACTIVE_CONFIG: u16 = 9;
pub const ACTIVE_CONFIG_VERSION: u16 = 1;
pub const SECTION_CMD_DEDUP: u16 = 15;
pub const CMD_DEDUP_VERSION: u16 = 2;
pub const SECTION_RECON_PENDING: u16 = 19;
pub const RECON_PENDING_VERSION: u16 = 2;
pub const SECTION_VENUE_NOTICES: u16 = 24;
pub const VENUE_NOTICES_VERSION: u16 = 1;
pub const SECTION_SLOT_BINDINGS: u16 = 31;
pub const SLOT_BINDINGS_VERSION: u16 = 1;
/// Viewer emergency-stop confirmation TTL.
pub const VIEWER_CONFIRM_TTL_MS: i64 = 60_000;

/// Section 9 row (64 B).
#[repr(C)]
#[derive(Copy, Clone, Eq, PartialEq, Debug, IntoBytes, Immutable, KnownLayout, zerocopy::TryFromBytes)]
pub struct ActiveConfigRow {
    pub blob: BlobRef,
    pub scope: CmdScope,
}
const _: () = assert!(core::mem::size_of::<ActiveConfigRow>() == 64);

/// Section 15 header.
#[repr(C)]
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, Default, FromBytes, IntoBytes, Immutable, KnownLayout)]
pub struct CmdDedupHdr {
    pub n_dedup: u32,
    pub n_preview: u32,
}
const _: () = assert!(core::mem::size_of::<CmdDedupHdr>() == 8);

/// Section 15 dedup row (32 B).
#[repr(C)]
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, Default, FromBytes, IntoBytes, Immutable, KnownLayout)]
pub struct CmdDedupRow {
    pub cmd_id: [u8; 16],
    pub cmd_seq: u64,
    pub status: u8,
    pub _pad: [u8; 7],
}
const _: () = assert!(core::mem::size_of::<CmdDedupRow>() == 32);
const _: () = assert!(core::mem::offset_of!(CmdDedupRow, cmd_seq) == 16);
const _: () = assert!(core::mem::offset_of!(CmdDedupRow, status) == 24);

/// Section 15 Viewer preview row (64 B).
#[repr(C)]
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, Default, FromBytes, IntoBytes, Immutable, KnownLayout)]
pub struct ViewerPreviewRow {
    pub cmd_hash: [u8; 32],
    pub issuer: [u8; 24],
    pub preview_engine_ns: i64,
}
const _: () = assert!(core::mem::size_of::<ViewerPreviewRow>() == 64);
const _: () = assert!(core::mem::offset_of!(ViewerPreviewRow, issuer) == 32);
const _: () = assert!(core::mem::offset_of!(ViewerPreviewRow, preview_engine_ns) == 56);

/// Section 19 header (16 B), followed by `n_stored` ReconcileReport bodies.
#[repr(C)]
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, FromBytes, IntoBytes, Immutable, KnownLayout)]
pub struct ReconPendingHdr {
    pub report_id: u64,
    pub account: AccountId,
    pub venue: VenueId,
    pub n_stored: u16,
    pub count: u16,
    pub received: u16,
}
const _: () = assert!(core::mem::size_of::<ReconPendingHdr>() == 16);
const _: () = assert!(core::mem::offset_of!(ReconPendingHdr, n_stored) == 10);

/// Section 24 row (32 B; key (venue, inst, kind)).
#[repr(C)]
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, FromBytes, IntoBytes, Immutable, KnownLayout)]
pub struct VenueNoticeRow {
    pub venue: VenueId,
    /// Delisting=1 FundingInterval=2 Maintenance=3.
    pub kind: u8,
    pub _pad: u16,
    pub inst: InstId,
    pub a: i64,
    pub b: i64,
    pub cmd_seq: u64,
}
const _: () = assert!(core::mem::size_of::<VenueNoticeRow>() == 32);
const _: () = assert!(core::mem::offset_of!(VenueNoticeRow, inst) == 4);
const _: () = assert!(core::mem::offset_of!(VenueNoticeRow, cmd_seq) == 24);

/// Section 31 row (64 B; key inst): the wire id a `BindSlot` bound to a slot inst.
#[repr(C)]
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, FromBytes, IntoBytes, Immutable, KnownLayout)]
pub struct SlotBindingRow {
    pub venue: VenueId,
    pub _pad: [u8; 3],
    pub inst: InstId,
    pub start_s: i64,
    pub end_s: i64,
    pub wire_id: [u8; 32],
    pub cmd_seq: u64,
}
const _: () = assert!(core::mem::size_of::<SlotBindingRow>() == 64);
const _: () = assert!(core::mem::offset_of!(SlotBindingRow, wire_id) == 24);

/// The `BindWire` parts carrying `row`'s wire id to the venue's feed.
pub(crate) fn emit_binding(e: &mut Engine, row: &SlotBindingRow) {
    for (part, c) in row.wire_id.chunks_exact(8).enumerate() {
        let mut chunk = [0u8; 8];
        chunk.copy_from_slice(c);
        e.out.emits.push(Emit::Feed {
            venue: row.venue,
            cmd: hft_event::FeedCmd::BindWire {
                inst: row.inst,
                part: part as u8,
                chunk,
            },
        });
    }
}

/// `BindWire` (2 parts) of one binding to the venue's gw when the venue has a Live account: the
/// gw's protocol needs the wire id to encode orders (Polymarket token ids). All zero = unbind.
pub(crate) fn emit_order_binding(e: &mut Engine, venue: VenueId, inst: InstId, wire_id: &[u8; 32]) {
    if !crate::engine::live_gw_venues(&e.rd)
        .iter()
        .any(|(v, _)| *v == venue)
    {
        return;
    }
    for (part, c) in wire_id.chunks_exact(16).enumerate() {
        let mut chunk = [0u8; 16];
        chunk.copy_from_slice(c);
        e.out.emits.push(Emit::Gw {
            ring: OutRing::Venue(venue),
            cmd: hft_event::GwCmd::bind_wire(inst, part as u8, &chunk),
            rec: None,
        });
    }
}

/// After `SetEpoch`: every recorded binding again to the (fresh) gws.
pub(crate) fn emit_order_bindings_all(e: &mut Engine) {
    let rows: Vec<_> = e
        .cmd
        .bindings
        .iter()
        .map(|r| (r.venue, r.inst, r.wire_id))
        .collect();
    for (v, inst, id) in rows {
        emit_order_binding(e, v, inst, &id);
    }
}

/// The active configuration (section 9) plus the pending refdata reference.
#[derive(Copy, Clone, Debug)]
pub struct ActiveConfig {
    pub refdata: BlobRef,
    pub engine: BlobRef,
    pub risk: BlobRef,
    /// `ReplaceRefdata` accepted, effective at the restart.
    pub pending_refdata: Option<BlobRef>,
}

/// Logged config blobs by sha256 (boot blocks and the replay drivers' `take_parsed`).
#[derive(Clone, Debug, Default)]
pub struct BlobStore {
    pub blobs: Vec<(BlobRef, Vec<u8>)>,
}

impl BlobStore {
    pub fn put(&mut self, r: BlobRef, bytes: &[u8]) {
        if self.get(&r.sha256).is_none() {
            self.blobs.push((r, bytes.to_vec()));
        }
    }
    pub fn get(&self, sha: &[u8; 32]) -> Option<&[u8]> {
        self.blobs
            .iter()
            .find(|(r, _)| r.sha256 == *sha)
            .map(|(_, b)| b.as_slice())
    }
}

/// One incomplete reconciliation group (section 19).
#[derive(Clone, Debug)]
pub struct ReconGroupState {
    pub report_id: u64,
    pub account: AccountId,
    pub venue: VenueId,
    pub count: u16,
    pub received: u16,
    pub stored: Vec<ReconcileReport>,
}

/// gw connection states per (account, venue) (OrderConns gate).
#[derive(Copy, Clone, Debug, Default)]
pub struct ConnRow {
    pub primary: bool,
    pub standby: bool,
    pub us_seen: bool,
    pub us_up: bool,
    pub orders: [u64; 4],
    pub users: [u64; 4],
}

pub const SECTION_ORDER_CONNS: u16 = 30;

#[repr(C)]
#[derive(Copy, Clone, Debug, FromBytes, IntoBytes, KnownLayout, Immutable)]
pub struct OrderConnsRow {
    pub account: AccountId,
    pub venue: VenueId,
    pub flags: u8,
    pub _pad: [u8; 5],
    pub orders: [u64; 4],
    pub users: [u64; 4],
}
const _: () = assert!(core::mem::size_of::<OrderConnsRow>() == 72);

#[derive(Clone, Debug)]
pub struct ConnTable {
    rows: Vec<((AccountId, VenueId), ConnRow)>,
    /// Absent section 30 retains legacy replay semantics until recovery ends.
    pub physical: bool,
}

impl ConnTable {
    pub fn new(rd: &RefData) -> Self {
        let mut rows = Vec::new();
        for a in 1..=255 {
            let account = AccountId(a);
            for v in 0..64 {
                if rd.account(account).venues & (1u64 << v) != 0 {
                    rows.push(((account, VenueId(v)), ConnRow::default()));
                }
            }
        }
        Self { rows, physical: true }
    }
    pub fn clear_run(&mut self) {
        for (_, row) in &mut self.rows {
            *row = ConnRow::default();
        }
    }
    pub fn snapshot_rows(&self) -> impl Iterator<Item = OrderConnsRow> + '_ {
        self.rows.iter().map(|((account, venue), r)| OrderConnsRow {
            account: *account,
            venue: *venue,
            flags: u8::from(r.primary)
                | (u8::from(r.standby) << 1)
                | (u8::from(r.us_seen) << 2)
                | (u8::from(r.us_up) << 3),
            _pad: [0; 5],
            orders: r.orders,
            users: r.users,
        })
    }
    pub fn get(&self, a: AccountId, v: VenueId) -> ConnRow {
        self.rows
            .iter()
            .find(|(k, _)| *k == (a, v))
            .map_or(ConnRow::default(), |x| x.1)
    }
    pub fn set(&mut self, a: AccountId, v: VenueId, r: ConnRow) {
        // Only configured account/venue pairs can own a gate.
        if let Some(x) = self.rows.iter_mut().find(|(k, _)| *k == (a, v)) {
            x.1 = r;
        }
    }
}

/// Command-path state.
#[derive(Clone, Debug)]
pub struct CmdState {
    pub dedup: VecDeque<CmdDedupRow>,
    pub dedup_cap: usize,
    pub previews: Vec<ViewerPreviewRow>,
    pub notices: Vec<VenueNoticeRow>,
    /// Slot bindings (section 31), sorted by inst.
    pub bindings: Vec<SlotBindingRow>,
    pub active: ActiveConfig,
    pub blobs: BlobStore,
    pub recon: Vec<ReconGroupState>,
    pub legs_touched: Vec<GroupId>,
    pub legs_spare: Vec<GroupId>,
    pub conns: ConnTable,
    pub unknown_last: Vec<u32>,
    pub feeds_last: Vec<bool>,
    /// Retained reconciliation views and position diffs read by `AdoptExchange`
    /// (snapshot section 29).
    pub recon_views: ReconViews,
    /// `(account, inst) → seq of the last fill` (snapshot section 28).
    pub fill_seq: FillSeqTable,
}

/// `(account, inst) → seq of the latest fill` read by the `AdoptExchange` `StaleReport` guard
/// and the `AUTO_EXTERNAL` precondition. Dense and sized from refdata at boot, so the fill path
/// never allocates (M1); persisted in snapshot section 28 so recovery equals continuous replay.
#[derive(Clone, Debug, Default)]
pub struct FillSeqTable {
    /// Row stride: `inst_count + 1` (inst 0 is never a fill inst).
    stride: usize,
    seqs: Vec<u64>,
}

impl FillSeqTable {
    pub fn new(n_accounts: usize, n_insts: usize) -> FillSeqTable {
        let stride = n_insts + 1;
        FillSeqTable {
            stride,
            seqs: vec![0; (n_accounts + 1) * stride],
        }
    }
    fn ix(&self, a: AccountId, i: InstId) -> Option<usize> {
        let i = i.0 as usize;
        (i < self.stride).then(|| usize::from(a.0) * self.stride + i)
    }
    /// 0 = no fill seen (or outside refdata).
    pub fn get(&self, a: AccountId, i: InstId) -> u64 {
        self.ix(a, i).and_then(|x| self.seqs.get(x)).copied().unwrap_or(0)
    }
    /// `false` = the key is outside the boot dimensions (nothing stored).
    pub fn set(&mut self, a: AccountId, i: InstId, seq: u64) -> bool {
        match self.ix(a, i).and_then(|x| self.seqs.get_mut(x)) {
            Some(x) => {
                *x = seq;
                true
            }
            None => false,
        }
    }
    /// Nonzero entries in `(account, inst)` order (the section 28 row order).
    pub fn rows(&self) -> impl Iterator<Item = FillSeqRow> + '_ {
        let stride = self.stride.max(1);
        self.seqs
            .iter()
            .enumerate()
            .filter(|(_, s)| **s != 0)
            .map(move |(k, s)| FillSeqRow {
                account: AccountId((k / stride) as u8),
                _pad: [0; 3],
                inst: InstId((k % stride) as u32),
                seq: *s,
            })
    }
}

/// Reports whose view and position diffs `AdoptExchange` can still use (oldest evicted first).
pub const RECON_VIEW_REPORTS: usize = 16;
/// Position-diff rows retained across those reports.
pub const RECON_VIEW_DIFFS: usize = 4096;

/// One retained reconciliation view (`AdoptExchange` precondition).
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct ReconView {
    pub report_id: u64,
    pub view_seq: u64,
    /// Some position diff of this report did not fit: the report cannot be adopted.
    pub overflow: bool,
}

/// One retained position diff `exch − engine`.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct ReconDiff {
    pub report_id: u64,
    pub account: AccountId,
    pub inst: InstId,
    pub diff: Qty,
}

/// Fixed-capacity table of the latest reconciliation views and their position diffs. Both
/// containers are allocated at boot and never grow (FIFO eviction of whole reports). A report
/// that is not retained (evicted, overflowed, or older than the snapshot it was restored from
/// without section 29) cannot be adopted: `AdoptExchange` answers `StaleReport`.
#[derive(Clone, Debug)]
pub struct ReconViews {
    views: VecDeque<ReconView>,
    diffs: Vec<ReconDiff>,
}

impl Default for ReconViews {
    fn default() -> ReconViews {
        ReconViews {
            views: VecDeque::with_capacity(RECON_VIEW_REPORTS),
            diffs: Vec::with_capacity(RECON_VIEW_DIFFS),
        }
    }
}

impl ReconViews {
    /// The retained view of `report_id`.
    pub fn view(&self, report_id: u64) -> Option<ReconView> {
        self.views.iter().find(|v| v.report_id == report_id).copied()
    }
    /// The retained diff (0 when the report has no position row for the key).
    pub fn diff(&self, report_id: u64, a: AccountId, i: InstId) -> Qty {
        self.diffs
            .iter()
            .find(|d| d.report_id == report_id && d.account == a && d.inst == i)
            .map_or(Qty(0), |d| d.diff)
    }
    /// Views (oldest first) and diffs (insertion order): the section 29 order.
    pub fn views(&self) -> impl Iterator<Item = &ReconView> + '_ {
        self.views.iter()
    }
    pub fn diffs_of(&self, report_id: u64) -> impl Iterator<Item = &ReconDiff> + '_ {
        self.diffs.iter().filter(move |d| d.report_id == report_id)
    }
    pub fn is_empty(&self) -> bool {
        self.views.is_empty()
    }
    fn evict_oldest(&mut self, keep: u64) -> bool {
        let Some(ix) = self.views.iter().position(|v| v.report_id != keep) else {
            return false;
        };
        if let Some(v) = self.views.remove(ix) {
            self.diffs.retain(|d| d.report_id != v.report_id);
        }
        true
    }
    /// Records one report row. Position rows set the view (last one wins) and their diff;
    /// other rows only open the view. Returns `true` the first time this report overflows.
    pub fn record(&mut self, r: &ReconcileReport, position: bool) -> bool {
        if self.view(r.report_id).is_none() {
            if self.views.len() >= RECON_VIEW_REPORTS {
                self.evict_oldest(r.report_id);
            }
            self.views.push_back(ReconView {
                report_id: r.report_id,
                view_seq: r.view_seq,
                overflow: false,
            });
        }
        if !position {
            return false;
        }
        let diff = Qty(r.exch_qty.0.saturating_sub(r.engine_qty.0));
        let Some(v) = self.views.iter_mut().find(|v| v.report_id == r.report_id) else {
            return false;
        };
        v.view_seq = r.view_seq;
        if v.overflow {
            return false;
        }
        if let Some(d) = self
            .diffs
            .iter_mut()
            .find(|d| d.report_id == r.report_id && d.account == r.account && d.inst == r.inst)
        {
            d.diff = diff;
            return false;
        }
        while self.diffs.len() >= RECON_VIEW_DIFFS {
            if !self.evict_oldest(r.report_id) {
                if let Some(v) = self.views.iter_mut().find(|v| v.report_id == r.report_id) {
                    v.overflow = true;
                }
                self.diffs.retain(|d| d.report_id != r.report_id);
                return true;
            }
        }
        self.diffs.push(ReconDiff {
            report_id: r.report_id,
            account: r.account,
            inst: r.inst,
            diff,
        });
        false
    }
    /// Restores one section 29 report (rows already validated by the reader).
    pub(crate) fn restore(&mut self, v: ReconView, diffs: &[ReconDiff]) -> bool {
        if self.views.len() >= RECON_VIEW_REPORTS
            || self.view(v.report_id).is_some()
            || self.diffs.len() + diffs.len() > RECON_VIEW_DIFFS
        {
            return false;
        }
        self.views.push_back(v);
        self.diffs.extend_from_slice(diffs);
        true
    }
}

pub const SECTION_RECON_VIEW: u16 = 29;
pub const RECON_VIEW_VERSION: u16 = 1;

/// Section 29 report row (24 B), followed by `n_diffs` [`ReconDiffRow`]s; reports oldest first.
#[repr(C)]
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, FromBytes, IntoBytes, Immutable, KnownLayout)]
pub struct ReconViewRow {
    pub report_id: u64,
    pub view_seq: u64,
    pub n_diffs: u32,
    /// OVERFLOW = 1.
    pub flags: u32,
}
const _: () = assert!(core::mem::size_of::<ReconViewRow>() == 24);
const _: () = assert!(core::mem::offset_of!(ReconViewRow, n_diffs) == 16);

/// Section 29 position-diff row (16 B), in recording order.
#[repr(C)]
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, FromBytes, IntoBytes, Immutable, KnownLayout)]
pub struct ReconDiffRow {
    pub account: AccountId,
    pub _pad: [u8; 3],
    pub inst: InstId,
    pub diff: Qty,
}
const _: () = assert!(core::mem::size_of::<ReconDiffRow>() == 16);
const _: () = assert!(core::mem::offset_of!(ReconDiffRow, diff) == 8);

pub const SECTION_FILL_SEQ: u16 = 28;
pub const FILL_SEQ_VERSION: u16 = 1;

/// Section 28 row (16 B): the seq of the latest fill of `(account, inst)`; only nonzero rows,
/// ascending `(account, inst)`. The section is written only when it has a row.
#[repr(C)]
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, FromBytes, IntoBytes, Immutable, KnownLayout)]
pub struct FillSeqRow {
    pub account: AccountId,
    pub _pad: [u8; 3],
    pub inst: InstId,
    pub seq: u64,
}
const _: () = assert!(core::mem::size_of::<FillSeqRow>() == 16);
const _: () = assert!(core::mem::offset_of!(FillSeqRow, inst) == 4);
const _: () = assert!(core::mem::offset_of!(FillSeqRow, seq) == 8);

impl CmdState {
    pub fn new(recent: usize, cells: usize, rd: &RefData) -> CmdState {
        let z = crate::strategies::zero_blob();
        CmdState {
            dedup: VecDeque::with_capacity(recent),
            dedup_cap: recent,
            previews: Vec::with_capacity(16),
            notices: Vec::with_capacity(64),
            bindings: Vec::with_capacity(256),
            active: ActiveConfig {
                refdata: z,
                engine: z,
                risk: z,
                pending_refdata: None,
            },
            blobs: BlobStore::default(),
            recon: Vec::with_capacity(8),
            legs_touched: Vec::with_capacity(cells),
            legs_spare: Vec::with_capacity(cells),
            conns: ConnTable::new(rd),
            unknown_last: vec![u32::MAX; 256],
            feeds_last: vec![false; 256],
            recon_views: ReconViews::default(),
            fill_seq: FillSeqTable::new(crate::engine::account_count(rd), rd.inst_count()),
        }
    }
    fn dedup_find(&self, id: &[u8; 16]) -> Option<&CmdDedupRow> {
        self.dedup.iter().find(|r| r.cmd_id == *id)
    }
    fn dedup_push(&mut self, r: CmdDedupRow) {
        if self.dedup.len() >= self.dedup_cap {
            self.dedup.pop_front();
        }
        self.dedup.push_back(r);
    }
}

// ------------------------------------------------------------------------------- ConfigBlob

/// A `ConfigBlob` Input: stored by sha for `take_parsed` and the boot block.
pub(crate) fn config_blob(e: &mut Engine, slot: &InputSlot) -> R<()> {
    let body = slot.body();
    let Some(h) = crate::types::read_body::<hft_event::BlobHdr>(body) else {
        return Ok(());
    };
    let content = body.get(72..72 + h.blob.len as usize).unwrap_or(&[]);
    e.cmd.blobs.put(h.blob, content);
    Ok(())
}

// ------------------------------------------------------------------------------- results

struct Res {
    status: u8,
    detail: u32,
    preview: bool,
    effect: u32,
    /// `rflags` besides `PREVIEW`/`NEEDS_CONFIRM` of the status (preview: `NEEDS_CONFIRM`,
    /// `PREVIEW_TTL`).
    rflags: u32,
    /// The preview block (only written with `preview`).
    pv: CmdPreview,
}

fn ok() -> Res {
    res_d(status::APPLIED, 0)
}
fn res(status: u8, code: u16) -> Res {
    res_d(status, u32::from(code))
}
fn res_d(status: u8, d: u32) -> Res {
    Res {
        status,
        detail: d,
        preview: false,
        effect: 0,
        rflags: 0,
        pv: <CmdPreview as zerocopy::FromZeros>::new_zeroed(),
    }
}

fn write_result(e: &mut Engine, inst_ver: u64, hdr: &CmdHdr, r: &Res) {
    let mut c = <CmdResult as zerocopy::FromZeros>::new_zeroed();
    c.cmd_id = hdr.cmd_id;
    c.cmd_seq = e.cur_seq;
    c.cmd = hdr.cmd;
    c.status = r.status;
    c.detail = r.detail;
    c.engine_epoch = e.epoch;
    c.effect_count = r.effect;
    c.risk_version = e.risk.params().version;
    c.params_version = inst_ver;
    c.ctl_state_version = e.risk.mode.ctl_state_version();
    c.rflags = if r.preview { CmdResult::PREVIEW } else { 0 }
        | if r.status == status::NEEDS_CONFIRM {
            CmdResult::NEEDS_CONFIRM
        } else {
            0
        }
        | r.rflags;
    c.preview = if r.preview {
        r.pv
    } else {
        <CmdPreview as zerocopy::FromZeros>::new_zeroed()
    };
    e.trace
        .rec("cmd_result", u64::from(r.status), u64::from(r.detail));
    e.out.push(RecordFlags(0), &c);
}

fn mode_err(m: ModeErr) -> Res {
    match m {
        ModeErr::GateNotReleased => res(status::PRECONDITION_FAILED, detail::GATE_NOT_RELEASED),
        ModeErr::Draining => res(status::PRECONDITION_FAILED, detail::DRAINING),
        ModeErr::NakedNotCleared => res(status::PRECONDITION_FAILED, detail::NAKED_NOT_CLEARED),
        ModeErr::ReconNotClean => res(status::PRECONDITION_FAILED, detail::RECON_NOT_CLEAN),
        ModeErr::ResumeLimit => res(status::PRECONDITION_FAILED, detail::RESUME_LIMIT),
        ModeErr::NoMatchingCap => res(status::PRECONDITION_FAILED, detail::NO_MATCHING_CAP),
        ModeErr::BrokenRemaining => res(status::REJECTED, detail::BROKEN_REMAINING),
        ModeErr::NotResumable => res(status::INVALID_ARGS, detail::NOT_RESUMABLE),
        ModeErr::InvalidScope => res(status::INVALID_ARGS, detail::BAD_COMBINATION),
        ModeErr::UnknownPending => res(status::PRECONDITION_FAILED, detail::UNKNOWN_PRESENT),
    }
}

fn issuer_str(h: &CmdHdr) -> &str {
    let n = h.issuer.iter().position(|b| *b == 0).unwrap_or(h.issuer.len());
    core::str::from_utf8(h.issuer.get(..n).unwrap_or(&[])).unwrap_or("")
}

const ROLE_TRADER: u8 = CmdHdr::ROLE_TRADER;
const ROLE_ADMIN: u8 = CmdHdr::ROLE_ADMIN;

/// The role recheck (control-plane permission table; process principals by issuer).
fn role_ok(h: &CmdHdr, c: &ControlCmd<'_>) -> bool {
    let issuer = issuer_str(h);
    if h.role == 0 {
        return match issuer {
            "cli:systemd" => match c {
                ControlCmd::Kill { flatten, hard, .. } => !*flatten && !*hard,
                ControlCmd::CancelAll { .. } | ControlCmd::Drain { .. } => true,
                _ => false,
            },
            "pipeline" => matches!(
                c,
                ControlCmd::SetParams { .. }
                    | ControlCmd::InjectWarmup { .. }
                    | ControlCmd::BindSlot { .. }
                    | ControlCmd::Settle { .. }
                    | ControlCmd::SetMode {
                        mode: RunMode::CloseOnly | RunMode::Shadow,
                        ..
                    }
            ),
            // hft-ctl-proto principal table: default-parameter `Kill{Global | Account}` (the
            // re-sent `/stop`) and `Annotate{OpsNote}`.
            "ops_bot" => match c {
                ControlCmd::Kill {
                    level, flatten, hard, ..
                } => matches!(level, KillLevel::Global | KillLevel::Account) && !*flatten && !*hard,
                ControlCmd::Annotate { .. } => true,
                _ => false,
            },
            _ => false,
        };
    }
    let viewer_only = h.role & (ROLE_TRADER | ROLE_ADMIN) == 0;
    if viewer_only {
        return match c {
            ControlCmd::Kill {
                level,
                flatten,
                hard,
                cancel_resting,
                ..
            } => {
                matches!(level, KillLevel::Global | KillLevel::Account)
                    && !*flatten
                    && !*hard
                    && *cancel_resting
            }
            ControlCmd::Annotate { .. } | ControlCmd::SnapshotNow => true,
            _ => false,
        };
    }
    let admin = h.role & ROLE_ADMIN != 0;
    match c {
        ControlCmd::SetRisk { .. }
        | ControlCmd::ReplaceRefdata { .. }
        | ControlCmd::WriteOffResidual { .. }
        | ControlCmd::SmokeOrder { .. }
        | ControlCmd::SimFault { .. } => admin,
        _ => true,
    }
}

/// The account a command is about (scope or payload).
fn cmd_account(c: &ControlCmd<'_>) -> Option<AccountId> {
    Some(match c {
        ControlCmd::SetMode { scope, .. }
        | ControlCmd::SetParams { scope, .. }
        | ControlCmd::Kill { scope, .. }
        | ControlCmd::Resume { scope, .. }
        | ControlCmd::HedgeFix { scope }
        | ControlCmd::CancelAll { scope } => scope.account,
        ControlCmd::AdoptExchange { account, .. }
        | ControlCmd::SimReset { account }
        | ControlCmd::SimTransfer { account, .. }
        | ControlCmd::SetBalanceBaseline { account, .. }
        | ControlCmd::SmokeOrder { account, .. }
        | ControlCmd::WriteOffResidual { account, .. }
        | ControlCmd::SimFault { account, .. }
        | ControlCmd::RecordTransfer { account, .. } => *account,
        _ => return None,
    })
}

/// Whether the command is an upgrade that needs confirmation in the current state.
fn needs_confirm<S: StrategySet>(
    e: &Engine,
    inst: &Instances<S>,
    h: &CmdHdr,
    c: &ControlCmd<'_>,
    d: &mut dyn_free::Drv<'_>,
) -> bool {
    match c {
        ControlCmd::SetMode { scope, mode } => e.risk.mode.is_upgrade(&ModeInput::SetMode {
            scope: *scope,
            mode: *mode,
        }),
        ControlCmd::Resume { .. } => true,
        ControlCmd::Kill { flatten, level, .. } => *flatten && *level == KillLevel::Account,
        // control-plane "类别": the upgrades, plus `ReplaceRefdata` and `SimReset` (neutral but
        // confirmed); `HedgeFix` is neutral.
        ControlCmd::WriteOffResidual { .. }
        | ControlCmd::SmokeOrder { .. }
        | ControlCmd::ReplaceRefdata { .. }
        | ControlCmd::SimReset { .. }
        | ControlCmd::AdoptExchange { .. }
        | ControlCmd::SetBalanceBaseline { .. } => true,
        ControlCmd::StrategyCmd { reduce_only, .. } => *reduce_only == 0,
        ControlCmd::SetRisk { .. } => d.risk_loosens,
        ControlCmd::SetParams { scope, .. } => {
            let _ = inst;
            let human = issuer_str(h) != "pipeline" && h.role != 0;
            let live_inst = d.params_live_instance;
            (live_inst && d.params_structural)
                || (human && d.params_opens)
                || (human && false && scope.account.0 == 0)
        }
        _ => false,
    }
}

/// Facts about the parsed object that the confirmation rule needs (computed before deciding).
mod dyn_free {
    #[derive(Copy, Clone, Debug, Default)]
    pub struct Drv<'a> {
        pub risk_loosens: bool,
        pub params_live_instance: bool,
        pub params_structural: bool,
        pub params_opens: bool,
        pub _p: core::marker::PhantomData<&'a ()>,
    }
}

fn is_upgrade_class(c: &ControlCmd<'_>, e: &Engine) -> bool {
    match c {
        ControlCmd::SetMode { scope, mode } => e.risk.mode.is_upgrade(&ModeInput::SetMode {
            scope: *scope,
            mode: *mode,
        }),
        ControlCmd::Resume { .. }
        | ControlCmd::WriteOffResidual { .. }
        | ControlCmd::SmokeOrder { .. }
        | ControlCmd::AdoptExchange { .. }
        | ControlCmd::SetBalanceBaseline { .. } => true,
        ControlCmd::StrategyCmd { reduce_only, .. } => *reduce_only == 0,
        _ => false,
    }
}

/// `ControlCmd` Input (README "命令路径").
pub(crate) fn control<'a, D: Driver<S>, S: StrategySet>(
    e: &mut Engine,
    inst: &mut Instances<S>,
    drv: &mut D,
    slot: &'a InputSlot,
    p: &mut Pending<'a>,
) -> R<()> {
    let body = slot.body();
    let Ok((h, c)) = ControlCmd::decode(body) else {
        let mut hz = CmdHdr::zeroed();
        if let Some(x) = body.get(..16) {
            hz.cmd_id.copy_from_slice(x);
        }
        write_result(
            e,
            0,
            &hz,
            &res(status::INVALID_ARGS, detail::UNKNOWN_DISCRIMINANT),
        );
        return Ok(());
    };
    // 2. expiry.
    if h.expires_tsc.0 != 0 && slot.tsc.0 > h.expires_tsc.0 {
        write_result(e, 0, &h, &res(status::EXPIRED, 0));
        return Ok(());
    }
    // de-duplication.
    if e.cmd.dedup_find(&h.cmd_id).is_some() {
        write_result(e, 0, &h, &res(status::DUPLICATE, 0));
        return Ok(());
    }
    let r = command(e, inst, drv, &h, &c, body, p)?;
    if !r.preview && r.status != status::EXPIRED {
        e.cmd.dedup_push(CmdDedupRow {
            cmd_id: h.cmd_id,
            cmd_seq: e.cur_seq,
            status: r.status,
            _pad: [0; 7],
        });
    }
    let pv = cmd_account(&c)
        .and_then(|a| inst.ix_of_account(a))
        .and_then(|ix| inst.slots.get(ix))
        .map_or(0, |s| s.params_version());
    write_result(e, pv, &h, &r);
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn command<'a, D: Driver<S>, S: StrategySet>(
    e: &mut Engine,
    inst: &mut Instances<S>,
    drv: &mut D,
    h: &CmdHdr,
    c: &ControlCmd<'a>,
    body: &'a [u8],
    p: &mut Pending<'a>,
) -> R<Res> {
    let preview = h.hflags & CmdHdr::PREVIEW != 0;
    // role.
    if !role_ok(h, c) {
        return Ok(res(status::ROLE_INSUFFICIENT, 0));
    }
    // account semantics.
    if let Some(a) = cmd_account(c)
        && h.account.0 != 0
        && a.0 != 0
        && a != h.account
    {
        return Ok(res(status::INVALID_ARGS, detail::ACCOUNT_MISMATCH));
    }
    // control-plane: `AdoptExchange` and `SetBalanceBaseline` only apply to Live accounts.
    if let ControlCmd::AdoptExchange { account, .. } | ControlCmd::SetBalanceBaseline { account, .. } = c
        && !matches!(e.rd.account(*account).kind, AccountKind::Live)
    {
        return Ok(res(status::INVALID_ARGS, detail::NOT_LIVE));
    }
    // Facts for SetParams / SetRisk confirmation.
    let mut facts = dyn_free::Drv::default();
    let mut parsed: Option<crate::parsed::ParsedConfig<S>> = None;
    if let ControlCmd::SetParams { blob, .. }
    | ControlCmd::SetRisk { blob }
    | ControlCmd::ReplaceRefdata { blob } = c
    {
        let base_ix = match c {
            ControlCmd::SetParams { scope, .. } => inst.ix(scope.strategy),
            _ => None,
        };
        let base = base_ix
            .and_then(|ix| inst.slots.get(ix))
            .and_then(|s| s.cur.as_ref())
            .map(|p| crate::parsed::ParamBase::<S> {
                params: &p.params,
                groups: &p.groups,
            });
        let cx = crate::driver::ParseCx {
            rd: &e.rd,
            max_groups: e.caps.max_groups_per_instance,
            base,
            risk_account: h.account,
            risk_decode: e.setup.risk_decode,
        };
        match drv.take_parsed(blob, &cx) {
            Ok(pc) => parsed = Some(pc),
            Err(crate::types::ConfigErr::Params(d)) => {
                return Ok(res_d(status::INVALID_ARGS, d));
            }
            Err(_) => return Ok(res(status::INVALID_ARGS, detail::BAD_VALUE)),
        }
        let _ = blob;
        match (c, parsed.as_ref()) {
            (ControlCmd::SetRisk { .. }, Some(crate::parsed::ParsedConfig::Risk { params, .. })) => {
                let sc = if h.account.0 == 0 {
                    AccountScope::All
                } else {
                    AccountScope::Account(h.account)
                };
                facts.risk_loosens = e.risk.loosens(params, sc);
            }
            (ControlCmd::SetParams { scope, .. }, Some(crate::parsed::ParsedConfig::StrategyParams(pp))) => {
                facts.params_structural = pp.structural_changed;
                facts.params_live_instance = live_instance(e, inst, scope.strategy);
                let old = inst.get(scope.strategy).and_then(|s| s.groups());
                facts.params_opens = pp.groups.iter().any(|(_, g)| {
                    g.open_allowed != 0
                        && old
                            .and_then(|o| o.gix_of(g.id).map(|gi| o.get(gi).open_allowed))
                            .is_none_or(|prev| prev == 0)
                });
            }
            _ => {}
        }
    }
    // Draining blocks upgrades (Kill{Account, flatten} excepted).
    let draining = e
        .risk
        .mode
        .cap(CapSource::DRAINING, &hft_risk::scope_any())
        .is_some()
        || e.drain.active;
    let flatten_kill = matches!(
        c,
        ControlCmd::Kill {
            flatten: true,
            level: KillLevel::Account,
            ..
        }
    );
    // confirmation.
    let viewer = h.role != 0 && h.role & (ROLE_TRADER | ROLE_ADMIN) == 0;
    if viewer && let ControlCmd::Kill { .. } = c {
        if preview {
            if e.cmd.previews.len() >= 16 {
                e.cmd.previews.remove(0);
            }
            let mut issuer = [0u8; 24];
            issuer.copy_from_slice(&h.issuer);
            e.cmd.previews.push(ViewerPreviewRow {
                cmd_hash: h.cmd_hash,
                issuer,
                preview_engine_ns: e.now.engine_ns,
            });
            let mut r = preview_result(e, inst, c);
            r.preview = true;
            r.rflags = CmdResult::NEEDS_CONFIRM | CmdResult::PREVIEW_TTL;
            r.pv = preview_block(e, inst, h, c, parsed.as_ref());
            return Ok(r);
        }
        if h.confirmed == 0 {
            return Ok(res(status::NEEDS_CONFIRM, 0));
        }
        let ttl = VIEWER_CONFIRM_TTL_MS * 1_000_000;
        let now = e.now.engine_ns;
        let pos = e.cmd.previews.iter().position(|x| {
            x.cmd_hash == h.cmd_hash && x.issuer == h.issuer && now - x.preview_engine_ns <= ttl
        });
        match pos {
            Some(i) => {
                e.cmd.previews.remove(i);
            }
            None => return Ok(res(status::CONFIRM_INVALID, detail::NO_VALID_PREVIEW)),
        }
    }
    let need = !viewer && needs_confirm(e, inst, h, c, &mut facts);
    if !viewer && !preview && need {
        if h.confirmed == 0 {
            return Ok(res(status::NEEDS_CONFIRM, 0));
        }
        let is_kill = matches!(c, ControlCmd::Kill { .. });
        if !is_kill && h.expect_state_version != e.risk.mode.ctl_state_version() {
            return Ok(res(status::CONFIRM_INVALID, detail::STATE_CHANGED));
        }
    }
    // `Resume{Draining}` is answered by the cap rules (not resumable), not by the drain gate.
    let resume_draining = matches!(c, ControlCmd::Resume { source, .. } if *source == CapSource::DRAINING);
    // `SetParams` and `SetRisk` are upgrades exactly when they need confirmation.
    let upgrade = is_upgrade_class(c, e)
        || (need && matches!(c, ControlCmd::SetParams { .. } | ControlCmd::SetRisk { .. }));
    if draining && upgrade && !flatten_kill && !resume_draining {
        return Ok(res(status::PRECONDITION_FAILED, detail::DRAINING));
    }
    // control-plane "类别": an upgrade while the target account's start gate is not released
    // (`SetMode`/`Resume` get it from the mode checks, in their order; `SetParams`/`SetRisk` are
    // not gated here: an account's first parameters precede its gate).
    let target = cmd_account(c).unwrap_or(h.account);
    if is_upgrade_class(c, e)
        && !matches!(c, ControlCmd::SetMode { .. } | ControlCmd::Resume { .. })
        && target.0 != 0
        && !e.risk.mode.released(target)
    {
        return Ok(res(status::PRECONDITION_FAILED, detail::GATE_NOT_RELEASED));
    }
    if preview {
        let mut r = preview_result(e, inst, c);
        r.preview = true;
        if need {
            r.rflags |= CmdResult::NEEDS_CONFIRM;
        }
        r.pv = preview_block(e, inst, h, c, parsed.as_ref());
        return Ok(r);
    }
    apply_cmd(e, inst, drv, h, c, parsed, body, p)
}

fn live_instance<S: StrategySet>(e: &Engine, inst: &Instances<S>, s: StrategyId) -> bool {
    let Some(sl) = inst.get(s) else { return false };
    if !matches!(e.rd.account(sl.account).kind, AccountKind::Live) {
        return false;
    }
    // A Live instance: at least one cell desired Live (control-plane definition).
    inst.cells()
        .filter(|c| c.strategy == s)
        .any(|c| e.risk.mode.desired(c) == Some(RunMode::Live))
}

fn preview_result<S: StrategySet>(e: &Engine, _inst: &Instances<S>, c: &ControlCmd<'_>) -> Res {
    let ad = crate::views::ModeAdapter {
        oms: &e.oms,
        ledger: &e.ledger,
        exec: &e.exec,
    };
    // The mode checks `apply` would make (control-plane: the preview's status is the code the
    // confirmed command would get now).
    let mode_inp = match c {
        ControlCmd::SetMode { scope, mode } => Some(ModeInput::SetMode {
            scope: *scope,
            mode: *mode,
        }),
        ControlCmd::Kill {
            scope,
            level,
            flatten,
            cancel_resting,
            hard,
        } => Some(ModeInput::Kill(KillCmd {
            level: *level,
            scope: *scope,
            cancel_resting: *cancel_resting,
            flatten: *flatten,
            hard: *hard,
        })),
        ControlCmd::Resume { scope, source } => Some(ModeInput::Resume {
            source: *source,
            scope: *scope,
        }),
        _ => None,
    };
    if let Some(inp) = mode_inp
        && let Err(m) = e.risk.check_mode(&inp, &ad)
    {
        return mode_err(m);
    }
    match c {
        ControlCmd::Resume { scope, source } => {
            let mut pv = hft_risk::mode::Preview::default();
            let inp = ModeInput::Resume {
                source: *source,
                scope: *scope,
            };
            let ad = crate::views::ModeAdapter {
                oms: &e.oms,
                ledger: &e.ledger,
                exec: &e.exec,
            };
            e.risk.preview(&inp, &ad, &mut pv);
            if *source == CapSource::DAILY_LOSS && !e.risk.daily_loss_resumable(scope.account) {
                return res(status::PRECONDITION_FAILED, detail::RESUME_LIMIT);
            }
            ok()
        }
        ControlCmd::SimFault { account, .. } => {
            if !e.is_sim(*account) {
                res(status::INVALID_ARGS, detail::NOT_SIM)
            } else if e.setup.role_prod {
                res(status::REJECTED, 0)
            } else {
                ok()
            }
        }
        _ => ok(),
    }
}

/// The `CmdPreview` block: target account and its kind, the cells and caps in scope, and for
/// `SetParams` the SDK `reset_preview` estimate the gateway computed.
fn preview_block<S: StrategySet>(
    e: &Engine,
    inst: &Instances<S>,
    h: &CmdHdr,
    c: &ControlCmd<'_>,
    parsed: Option<&crate::parsed::ParsedConfig<S>>,
) -> CmdPreview {
    let mut p = <CmdPreview as zerocopy::FromZeros>::new_zeroed();
    let a = match c {
        ControlCmd::Kill { scope, .. } | ControlCmd::Resume { scope, .. } => scope.account,
        _ => cmd_account(c).unwrap_or(h.account),
    };
    let a = if a.0 == 0 { h.account } else { a };
    p.target_account = a;
    if a.0 != 0 {
        p.target_kind = match e.rd.account(a).kind {
            AccountKind::Live => 1,
            AccountKind::Sim(_) => 2,
        };
    }
    p.affected_cells = inst.cells().filter(|x| a.0 == 0 || x.account == a).count() as u32;
    for cap in e.risk.mode.caps() {
        if a.0 == 0 || cap.scope.account.0 == 0 || cap.scope.account == a {
            p.active_caps += 1;
            if cap.source.0 < 32 {
                p.cap_sources |= 1u32 << cap.source.0;
            }
        }
    }
    if let Some(crate::parsed::ParsedConfig::StrategyParams(pp)) = parsed {
        p.reset_groups = pp.reset_preview.reset_groups;
        p.no_open_ms = pp.reset_preview.no_open_ms;
    }
    p
}

#[allow(clippy::too_many_arguments)]
fn apply_cmd<'a, D: Driver<S>, S: StrategySet>(
    e: &mut Engine,
    inst: &mut Instances<S>,
    drv: &mut D,
    h: &CmdHdr,
    c: &ControlCmd<'a>,
    parsed: Option<crate::parsed::ParsedConfig<S>>,
    body: &'a [u8],
    p: &mut Pending<'a>,
) -> R<Res> {
    let now = e.now;
    match c {
        ControlCmd::SetMode { scope, mode } => {
            let before = e.risk.mode.ctl_state_version();
            let r = crate::mode_glue::apply_mode_r(
                e,
                inst,
                drv,
                ModeInput::SetMode {
                    scope: *scope,
                    mode: *mode,
                },
            )?;
            Ok(match r {
                Ok(()) => {
                    let deferred = *mode == RunMode::Shadow && shadow_deferred(e, scope);
                    if deferred {
                        res(status::APPLIED, detail::SHADOW_DEFERRED)
                    } else if e.risk.mode.ctl_state_version() == before {
                        // Already in the target state (control-plane "幂等").
                        res(status::NO_OP, 0)
                    } else {
                        ok()
                    }
                }
                Err(m) => mode_err(m),
            })
        }
        ControlCmd::Kill {
            scope,
            level,
            flatten,
            cancel_resting,
            hard,
        } => {
            if *hard && *flatten {
                return Ok(res(status::INVALID_ARGS, detail::BAD_COMBINATION));
            }
            let k = KillCmd {
                level: *level,
                scope: *scope,
                cancel_resting: *cancel_resting,
                flatten: *flatten,
                hard: *hard,
            };
            let r = crate::mode_glue::apply_mode_r(e, inst, drv, ModeInput::Kill(k))?;
            match r {
                Ok(()) => {
                    if *flatten && *level == KillLevel::Account {
                        crate::exec_glue::flatten(e, inst, drv, scope.account)?;
                    }
                    Ok(ok())
                }
                Err(m) => Ok(mode_err(m)),
            }
        }
        ControlCmd::Resume { scope, source } => {
            let r = crate::mode_glue::apply_mode_r(
                e,
                inst,
                drv,
                ModeInput::Resume {
                    source: *source,
                    scope: *scope,
                },
            )?;
            Ok(match r {
                Ok(()) => ok(),
                Err(m) => mode_err(m),
            })
        }
        ControlCmd::SetParams { scope, blob } => {
            let Some(crate::parsed::ParsedConfig::StrategyParams(pp)) = parsed else {
                return Ok(res(status::INVALID_ARGS, detail::BAD_VALUE));
            };
            set_params(e, inst, drv, h, scope, blob, pp)
        }
        ControlCmd::SetRisk { .. } => {
            let Some(crate::parsed::ParsedConfig::Risk { params, sha256 }) = parsed else {
                return Ok(res(status::INVALID_ARGS, detail::BAD_VALUE));
            };
            // control-plane "版本与确认": a version at or below the current one is stale.
            if params.version <= e.risk.params().version {
                drv.retire(crate::parsed::ParsedConfig::Risk { sha256, params });
                return Ok(res(status::STALE_VERSION, 0));
            }
            let sc = if h.account.0 == 0 {
                AccountScope::All
            } else {
                AccountScope::Account(h.account)
            };
            let r = e.risk.set_params(&params, sc);
            e.trace.rec("risk.set_params", u64::from(h.account.0), 0);
            let res = match r {
                Ok(()) => {
                    e.cmd.active.risk.sha256 = sha256;
                    e.cmd.active.risk.version = params.version;
                    ok()
                }
                Err(hft_risk::RiskParamError::Invalid(_)) => res(status::INVALID_ARGS, detail::PARAM_INVALID),
                Err(hft_risk::RiskParamError::ExecRiskMismatch) => {
                    res(status::INVALID_ARGS, detail::EXEC_RISK_MISMATCH)
                }
                Err(hft_risk::RiskParamError::Capacity) => res(status::CAPACITY_EXCEEDED, 0),
            };
            drv.retire(crate::parsed::ParsedConfig::Risk { sha256, params });
            Ok(res)
        }
        ControlCmd::SnapshotNow => {
            e.snapshot_requested = true;
            Ok(ok())
        }
        ControlCmd::Drain { cancel_resting } => {
            start_drain(e, inst, drv, *cancel_resting, DoneCause::Drain)?;
            Ok(ok())
        }
        ControlCmd::CancelAll { scope } => {
            let sc = cancel_scope(scope);
            let mut o = e.take_oms();
            e.trace.rec("oms.cancel_all", 0, 0);
            e.oms.cancel_all(sc, false, now.tsc, e.cur_seq, &mut o);
            crate::intents::drain_oms(e, inst, drv, &mut o)?;
            e.put_oms(o);
            Ok(ok())
        }
        ControlCmd::Annotate { .. } => Ok(ok()),
        ControlCmd::HedgeFix { scope } => {
            let r = crate::exec_glue::hedge_fix(e, inst, drv, scope.account, scope.group)?;
            Ok(match r {
                Ok(()) => ok(),
                Err(x) => exec_err(x),
            })
        }
        ControlCmd::RecordTransfer {
            account,
            from,
            to,
            asset,
            amount,
            ..
        } => {
            e.ledger.apply_transfer(
                &TransferDelta {
                    account: *account,
                    from_venue: *from,
                    to_venue: *to,
                    asset: *asset,
                    amount: *amount,
                    kind: TransferKind::Record,
                },
                now.engine_ns,
            );
            e.trace.rec("ledger.apply_transfer", u64::from(account.0), 0);
            Ok(ok())
        }
        ControlCmd::AdoptExchange {
            account,
            venue,
            inst: inst_id,
            report_id,
            item,
            asset,
            group,
            entry_px,
            ..
        } => adopt(
            e, inst, drv, *account, *venue, *inst_id, *report_id, *item, *asset, *group, *entry_px,
        ),
        ControlCmd::SetBalanceBaseline { account, venue } => {
            let mut outv = hft_ledger::FixedVec::<hft_types::AssetId, 64>::new();
            let r = e
                .ledger
                .set_balance_baseline(*account, *venue, e.cur_seq, &mut outv);
            e.trace
                .rec("ledger.set_balance_baseline", u64::from(account.0), 0);
            for a in outv.iter().copied().collect::<Vec<_>>() {
                crate::mode_glue::apply_mode(
                    e,
                    inst,
                    drv,
                    ModeInput::Gate(GateEvent::Disposed {
                        account: *account,
                        venue: *venue,
                        report_id: 0,
                        item: ReconItemKey::new(ReconItemKey::BALANCE, u32::from(a.0) | hft_risk::ASSET_KEY),
                        how: hft_risk::mode::Disposition::Baselined,
                    }),
                )?;
            }
            Ok(match r {
                hft_ledger::BaselineResult::Set { .. } => ok(),
                hft_ledger::BaselineResult::NoBalanceView => {
                    res(status::PRECONDITION_FAILED, detail::NO_BALANCE_VIEW)
                }
            })
        }
        ControlCmd::VenueNotice {
            venue,
            kind,
            inst: inst_id,
            a,
            b,
            ..
        } => {
            let row = VenueNoticeRow {
                venue: *venue,
                kind: *kind,
                _pad: 0,
                inst: *inst_id,
                a: *a,
                b: *b,
                cmd_seq: e.cur_seq,
            };
            e.cmd
                .notices
                .retain(|x| !(x.venue == *venue && x.inst == *inst_id && x.kind == *kind));
            e.cmd.notices.push(row);
            e.cmd.notices.sort_by_key(|x| (x.venue, x.inst, x.kind));
            let n = VenueNoticeCmd {
                venue: *venue,
                kind: *kind,
                inst: *inst_id,
                a: *a,
                b: *b,
            };
            crate::mode_glue::apply_mode(e, inst, drv, ModeInput::Notice(&n))?;
            notice_delivery(e, inst, &row);
            Ok(ok())
        }
        ControlCmd::BindSlot {
            venue,
            inst: slot,
            start_s,
            end_s,
            wire_id,
            ..
        } => Ok(bind_slot(e, *venue, *slot, *start_s, *end_s, wire_id)),
        ControlCmd::Settle {
            venue,
            inst: slot,
            end_s,
            payout,
            ..
        } => Ok(settle(e, *venue, *slot, *end_s, *payout)),
        ControlCmd::SimReset { account } => sim_reset(e, inst, drv, *account),
        ControlCmd::SimTransfer {
            account,
            from,
            to,
            asset,
            amount,
        } => {
            if !e.is_sim(*account) {
                return Ok(res(status::INVALID_ARGS, detail::NOT_SIM));
            }
            let cv = e.ledger.cash_view(*account, *from, *asset);
            if cv.free.0 < amount.0 {
                return Ok(res(status::PRECONDITION_FAILED, detail::INSUFFICIENT_BALANCE));
            }
            e.ledger.apply_transfer(
                &TransferDelta {
                    account: *account,
                    from_venue: *from,
                    to_venue: *to,
                    asset: *asset,
                    amount: *amount,
                    kind: TransferKind::Sim,
                },
                now.engine_ns,
            );
            e.out.emits.push(Emit::Gw {
                ring: OutRing::Sim,
                cmd: hft_event::GwCmd::sim_transfer(*account, *from, *to, *asset, *amount),
                rec: None,
            });
            Ok(ok())
        }
        ControlCmd::SimFault {
            account,
            venue,
            fault,
            delay_ms,
            ..
        } => {
            if !e.is_sim(*account) {
                return Ok(res(status::INVALID_ARGS, detail::NOT_SIM));
            }
            if e.setup.role_prod {
                return Ok(res(status::REJECTED, 0));
            }
            e.out.emits.push(Emit::Gw {
                ring: OutRing::Sim,
                cmd: hft_event::GwCmd::sim_fault(*account, *venue, *fault, *delay_ms),
                rec: None,
            });
            Ok(ok())
        }
        ControlCmd::StrategyCmd {
            instance,
            reduce_only,
            len,
            group,
            payload,
        } => {
            let Some(ix) = inst
                .ix(*instance)
                .filter(|ix| inst.slots.get(*ix).is_some_and(|s| s.inst.is_some()))
            else {
                return Ok(res(status::UNKNOWN_TARGET, 0));
            };
            let acc = inst.slots.get(ix).map_or(AccountId(0), |s| s.account);
            if h.account.0 != 0 && h.account != acc {
                return Ok(res(status::INVALID_ARGS, detail::ACCOUNT_BOUND));
            }
            let gix = if *group == GROUP_NONE {
                hft_strategy_sdk::GIX_NONE
            } else {
                match inst.slots.get(ix).and_then(|s| s.gix_of(*group)) {
                    Some(g) => g,
                    None => return Ok(res(status::INVALID_ARGS, detail::UNKNOWN_GROUP)),
                }
            };
            let ro = *reduce_only != 0;
            let n = usize::from(*len).min(hft_event::STRATEGY_CMD_MAX_PAYLOAD);
            // The payload in the logged body: CmdHdr (128 B) + instance, reduce_only, len, group.
            let pl: &'a [u8] = body.get(136..136 + n).unwrap_or(&[]);
            let _ = payload;
            p.direct = Some((
                ix,
                Direct::Manual {
                    gix,
                    payload: pl,
                    reduce_only: ro,
                },
                ro,
            ));
            Ok(ok())
        }
        ControlCmd::InjectWarmup {
            instance,
            last,
            local_kind,
            chunk_no,
            payload,
            ..
        } => {
            let Some(ix) = inst
                .ix(*instance)
                .filter(|ix| inst.slots.get(*ix).is_some_and(|s| s.inst.is_some()))
            else {
                return Ok(res(status::UNKNOWN_TARGET, 0));
            };
            if payload.len() > hft_event::WARMUP_MAX_PAYLOAD {
                return Ok(res(status::INVALID_ARGS, detail::BAD_VALUE));
            }
            let both = inst
                .slots
                .get(ix)
                .is_some_and(|s| s.warmup == hft_strategy_sdk::Part::Both);
            if inst.models.on
                && let ControlCmd::InjectWarmup {
                    local_kind,
                    chunk_no,
                    last,
                    payload,
                    ..
                } = c
            {
                let d = Direct::Warmup {
                    local_kind: *local_kind,
                    chunk_no: *chunk_no,
                    last: *last != 0,
                    payload,
                };
                crate::advisor::warmup(e, inst, ix, &d);
            }
            if both {
                p.direct = Some((
                    ix,
                    Direct::Warmup {
                        local_kind: *local_kind,
                        chunk_no: *chunk_no,
                        last: *last != 0,
                        payload,
                    },
                    false,
                ));
            }
            Ok(ok())
        }
        ControlCmd::SmokeOrder {
            account,
            venue,
            inst: inst_id,
            side,
            ttl_ms,
            ..
        } => smoke(e, inst, drv, *account, *venue, *inst_id, *side, *ttl_ms),
        ControlCmd::WriteOffResidual { account, group, .. } => write_off(e, inst, drv, *account, *group),
        ControlCmd::ReplaceRefdata { blob } => {
            let Some(crate::parsed::ParsedConfig::Refdata { result, sha256 }) = parsed else {
                return Ok(res(status::INVALID_ARGS, detail::BAD_VALUE));
            };
            if blob.version <= e.rd.version() {
                drv.retire(crate::parsed::ParsedConfig::Refdata { sha256, result });
                return Ok(res(status::STALE_VERSION, 0));
            }
            let arg = refdata_recheck(e, inst, &result);
            drv.retire(crate::parsed::ParsedConfig::Refdata { sha256, result });
            match arg {
                Some(a) => Ok(res_d(status::REJECTED, detail::pack(detail::REFDATA_RECHECK, a))),
                None => {
                    e.cmd.active.pending_refdata = Some(*blob);
                    start_drain(e, inst, drv, false, DoneCause::ReplaceRefdata)?;
                    Ok(ok())
                }
            }
        }
        ControlCmd::RotateConn { .. } | ControlCmd::Resubscribe { .. } => Ok(ok()),
    }
}

/// `BindSlot`: the inst must be a slot of `venue` with no position and no open order in any
/// account; the binding is recorded (section 31) and, when the inst's feed subscription is live,
/// sent to the feed now (else just before its next subscription, `subs::send_union_changes`);
/// a venue with a Live account also gets it on its gw (`emit_order_binding`).
fn bind_slot(
    e: &mut Engine,
    venue: VenueId,
    slot: InstId,
    start_s: i64,
    end_s: i64,
    wire_id: &[u8; 32],
) -> Res {
    if slot.0 == 0 || e.rd.inst(slot).venue != venue || venue.0 == 0 || start_s >= end_s {
        return res(status::INVALID_ARGS, detail::BAD_VALUE);
    }
    let accounts = crate::engine::account_count(&e.rd);
    if (1..=accounts).any(|a| e.ledger.pos(AccountId(a as u8), slot).qty.0 != 0) {
        return res(status::PRECONDITION_FAILED, detail::GROUP_HAS_POSITION);
    }
    let infl = e.oms.inflight();
    if (1..=accounts).any(|a| {
        let a = AccountId(a as u8);
        infl.open_buy(a, slot).0 != 0 || infl.open_sell(a, slot).0 != 0
    }) {
        return res(status::PRECONDITION_FAILED, detail::NOT_QUIESCENT);
    }
    let row = SlotBindingRow {
        venue,
        _pad: [0; 3],
        inst: slot,
        start_s,
        end_s,
        wire_id: *wire_id,
        cmd_seq: e.cur_seq,
    };
    let same = e
        .cmd
        .bindings
        .iter()
        .any(|x| x.inst == slot && x.wire_id == *wire_id && x.start_s == start_s && x.end_s == end_s);
    if same {
        return res(status::NO_OP, 0);
    }
    e.cmd.bindings.retain(|x| x.inst != slot);
    if *wire_id != [0; 32] {
        e.cmd.bindings.push(row);
        e.cmd.bindings.sort_by_key(|x| x.inst.0);
    }
    e.trace.rec("cmd.bind_slot", u64::from(slot.0), 0);
    if e.sent_union.get(slot.0 as usize).is_some_and(|u| *u != 0) {
        emit_binding(e, &row);
    }
    emit_order_binding(e, venue, slot, wire_id);
    ok()
}

/// `Settle`: the inst must be a slot of `venue` whose binding ends at `end_s`, the window must have
/// ended (engine UTC ≥ `end_s`) and `0 ≤ payout ≤` one unit of the inst's price. Every Sim account
/// with inventory or an open order on the inst gets a `SimSettle` on `out-sim` (sim-gw cancels the
/// orders and delivers the inventory at `payout`, hft-sim). A Live account with an open order on
/// the inst → `NotQuiescent` (nothing sent); every Live account with inventory gets a `Deliver` on
/// `out-<venue>` (its gw reports the delivery, hft-venue; the exchange redeems on chain). None →
/// `NoOp`.
fn settle(e: &mut Engine, venue: VenueId, slot: InstId, end_s: i64, payout: hft_types::Px) -> Res {
    if slot.0 == 0 || venue.0 == 0 || e.rd.inst(slot).venue != venue {
        return res(status::INVALID_ARGS, detail::BAD_VALUE);
    }
    let one = 10i64.checked_pow(u32::from(e.rd.inst(slot).px_exp.unsigned_abs()));
    let bound = e.cmd.bindings.iter().any(|x| x.inst == slot && x.end_s == end_s);
    let Some(one) = one else {
        return res(status::INVALID_ARGS, detail::BAD_VALUE);
    };
    if !bound || payout.0 < 0 || payout.0 > one || e.rd.inst(slot).px_exp > 0 {
        return res(status::INVALID_ARGS, detail::BAD_VALUE);
    }
    if e.now.utc_ns < end_s.saturating_mul(1_000_000_000) {
        return res(status::PRECONDITION_FAILED, detail::NOT_ENDED);
    }
    let accounts = crate::engine::account_count(&e.rd);
    let infl = e.oms.inflight();
    let mut targets = Vec::new();
    for k in 1..=accounts {
        let a = AccountId(k as u8);
        let pos = e.ledger.pos(a, slot).qty;
        let open = infl.open_buy(a, slot).0 != 0 || infl.open_sell(a, slot).0 != 0;
        if pos.0 == 0 && !open {
            continue;
        }
        if !e.is_sim(a) {
            if open {
                return res(status::PRECONDITION_FAILED, detail::NOT_QUIESCENT);
            }
            if pos.0 < 0 {
                continue;
            }
        }
        targets.push((a, pos));
    }
    if targets.is_empty() {
        return res(status::NO_OP, 0);
    }
    let end = end_s.clamp(0, i64::from(u32::MAX)) as u32;
    for (a, pos) in targets {
        let (ring, cmd) = if e.is_sim(a) {
            (
                OutRing::Sim,
                hft_event::GwCmd::sim_settle(a, venue, slot, payout, end),
            )
        } else {
            (
                OutRing::Venue(venue),
                hft_event::GwCmd::deliver(a, venue, slot, payout, end, pos),
            )
        };
        e.out.emits.push(Emit::Gw { ring, cmd, rec: None });
    }
    e.trace.rec("cmd.settle", u64::from(slot.0), payout.0 as u64);
    ok()
}

fn exec_err(x: hft_exec::ExecCmdErr) -> Res {
    use hft_exec::ExecCmdErr as X;
    match x {
        X::UnknownGroup => res(status::INVALID_ARGS, detail::UNKNOWN_GROUP),
        X::Hard => res(status::PRECONDITION_FAILED, detail::HARD),
        X::NotQuiescent => res(status::PRECONDITION_FAILED, detail::NOT_QUIESCENT),
        X::NothingToFix => res(status::PRECONDITION_FAILED, detail::NOTHING_TO_FIX),
        X::QuoteUnavailable => res(status::PRECONDITION_FAILED, detail::QUOTE_UNAVAILABLE),
        X::ChainLeg => res(status::PRECONDITION_FAILED, detail::CHAIN_LEG),
    }
}

fn shadow_deferred(e: &Engine, scope: &CmdScope) -> bool {
    if scope.group == GROUP_ALL || scope.group.0 == 0 {
        return false;
    }
    let cell = hft_risk::mode::CellKey {
        strategy: scope.strategy,
        account: scope.account,
        group: scope.group,
    };
    matches!(
        e.risk.mode.effective(cell),
        hft_risk::mode::Effective::CloseOnly {
            deferred_shadow: true
        }
    )
}

pub(crate) fn cancel_scope(s: &CmdScope) -> hft_oms::CancelScope {
    match (s.account.0, s.venue.0, s.inst.0) {
        (0, 0, _) => hft_oms::CancelScope::Global,
        (a, 0, _) => hft_oms::CancelScope::Account(AccountId(a)),
        (0, v, 0) => hft_oms::CancelScope::Venue(VenueId(v)),
        (0, v, i) => hft_oms::CancelScope::Inst(VenueId(v), InstId(i)),
        (a, v, _) => hft_oms::CancelScope::VenueAccount(VenueId(v), AccountId(a)),
    }
}

fn notice_delivery<S: StrategySet>(e: &mut Engine, inst: &Instances<S>, row: &VenueNoticeRow) {
    use hft_strategy_sdk::{NoticeKind, VenueNoticeView};
    let kind = match row.kind {
        1 => NoticeKind::Delisting { at_ns: row.a },
        2 => NoticeKind::FundingInterval { new_s: row.a as u32 },
        _ => NoticeKind::Maintenance {
            from_ns: row.a,
            to_ns: row.b,
        },
    };
    let n = VenueNoticeView {
        venue: row.venue,
        inst: row.inst,
        kind,
    };
    // One delivery per (instance, subscribed group) in gix order, whatever the entry's `Part`
    // (SDK "EventRef": both parts receive `Notice`).
    for (ix, s) in inst.slots.iter().enumerate() {
        let mut last = None;
        loop {
            // Subscription order need not be gix order. Pick the next distinct group without
            // allocating a temporary collection; ties keep the first matching subscription.
            let next = s
                .subs
                .iter()
                .filter(|se| {
                    last.is_none_or(|gix| se.gix > gix)
                        && if row.inst.0 == 0 {
                            e.rd.inst(se.inst).venue == row.venue
                        } else {
                            se.inst == row.inst
                        }
                })
                .min_by_key(|se| se.gix);
            let Some(se) = next else { break };
            last = Some(se.gix);
            e.notify.push(
                ix as u16,
                crate::notify::Notif::Notice {
                    inst: se.inst,
                    gix: se.gix,
                    n,
                },
            );
        }
    }
}

// ------------------------------------------------------------------------------- SetParams

fn set_params<D: Driver<S>, S: StrategySet>(
    e: &mut Engine,
    inst: &mut Instances<S>,
    drv: &mut D,
    h: &CmdHdr,
    scope: &CmdScope,
    blob: &BlobRef,
    pp: Box<crate::parsed::ParsedParams<S>>,
) -> R<Res> {
    let strategy = scope.strategy;
    let account = if scope.account.0 != 0 {
        scope.account
    } else {
        pp.account
    };
    let Some(ix) = inst.ix(strategy) else {
        drv.retire(crate::parsed::ParsedConfig::StrategyParams(pp));
        return Ok(res(status::INVALID_ARGS, detail::ACCOUNT_BOUND));
    };
    let fail = |drv: &mut D, pp: Box<crate::parsed::ParsedParams<S>>, r: Res| -> R<Res> {
        drv.retire(crate::parsed::ParsedConfig::StrategyParams(pp));
        Ok(r)
    };
    // ① binding, kind, non_prod_only, legs.
    if let Err(x) = e.rd.check_binding(account, strategy, pp.kind_id) {
        let d = if x.code == hft_refdata::LoadErrCode::KindMismatch {
            detail::KIND_MISMATCH
        } else {
            detail::ACCOUNT_BOUND
        };
        return fail(drv, pp, res(status::INVALID_ARGS, d));
    }
    let desc = S::kinds().iter().find(|d| d.kind_id == pp.kind_id);
    if desc.is_some_and(|d| d.non_prod_only) && e.rd.account(account).is_prod {
        return fail(
            drv,
            pp,
            res_d(
                status::INVALID_ARGS,
                detail::pack(
                    detail::PARAM_INVALID,
                    detail::param_arg(detail::PARAM_KIND_NON_PROD_ONLY, 0),
                ),
            ),
        );
    }
    if let Err(d) = crate::parsed::check_groups(&e.rd, account, &pp.groups) {
        return fail(drv, pp, res_d(status::INVALID_ARGS, d));
    }
    // ② version.
    let cur_ver = inst.slots.get(ix).map_or(0, |s| s.params_version());
    if pp.version <= cur_ver && inst.slots.get(ix).is_some_and(|s| s.cur.is_some()) {
        return fail(drv, pp, res(status::STALE_VERSION, 0));
    }
    if h.base_version != 0 && h.base_version != cur_ver {
        return fail(drv, pp, res(status::VERSION_CONFLICT, 0));
    }
    // ③ GroupId.
    let mut bad: Option<u16> = None;
    let mut seen: Vec<GroupId> = Vec::new();
    for (_, g) in pp.groups.iter() {
        if g.id == GROUP_NONE || g.id == GROUP_ALL || g.id.strategy() != strategy {
            bad = Some(detail::BAD_GROUP);
            break;
        }
        if seen.contains(&g.id) {
            bad = Some(detail::GROUP_ID_DUP);
            break;
        }
        seen.push(g.id);
    }
    if let Some(d) = bad {
        return fail(drv, pp, res(status::INVALID_ARGS, d));
    }
    // ④ InstOwned: one non-Shadow cell per (account, inst) within the instance.
    let mut owned: Vec<InstId> = Vec::new();
    let mut inst_owned = false;
    for (_, g) in pp.groups.iter() {
        let cell = hft_risk::mode::CellKey {
            strategy,
            account,
            group: g.id,
        };
        let desired = e
            .risk
            .mode
            .desired(cell)
            .or_else(|| e.risk.mode.default_mode(strategy, account));
        if desired == Some(RunMode::Shadow) {
            continue;
        }
        for l in g.legs.iter().take(usize::from(g.n_legs)) {
            if owned.contains(&l.inst) {
                inst_owned = true;
            }
            owned.push(l.inst);
        }
    }
    if inst_owned {
        return fail(drv, pp, res(status::INVALID_ARGS, detail::INST_OWNED));
    }
    // ⑤ removed / fingerprint-changed groups must be empty.
    let old = inst
        .slots
        .get(ix)
        .and_then(|s| s.cur.as_ref())
        .map(|c| c.groups.clone());
    if let Some(old) = old.as_ref() {
        use hft_strategy_sdk::GroupSpecExt;
        let mut busy = false;
        for (_, g) in old.iter() {
            let gone = match pp.groups.gix_of(g.id) {
                None => true,
                Some(ng) => pp.groups.get(ng).fingerprint() != g.fingerprint(),
            };
            if gone && group_busy(e, account, g.id) {
                busy = true;
            }
        }
        if busy {
            return fail(
                drv,
                pp,
                res(status::PRECONDITION_FAILED, detail::GROUP_HAS_POSITION),
            );
        }
    }
    // ⑥ capacity.
    let other_cells = inst.n_cells()
        - inst
            .slots
            .get(ix)
            .and_then(|s| s.groups())
            .map_or(0, |g| usize::from(g.len()));
    if other_cells + usize::from(pp.groups.len()) > e.caps.max_cells_total as usize {
        return fail(drv, pp, res(status::CAPACITY_EXCEEDED, 0));
    }
    {
        let mut refs: Vec<(InstId, u16)> = Vec::new();
        for s in inst.slots.iter() {
            if s.id == strategy {
                continue;
            }
            if let Some(g) = s.groups() {
                for (_, sp) in g.iter() {
                    for l in sp.legs.iter().take(usize::from(sp.n_legs)) {
                        bump(&mut refs, l.inst);
                    }
                }
            }
        }
        for (_, sp) in pp.groups.iter() {
            for l in sp.legs.iter().take(usize::from(sp.n_legs)) {
                bump(&mut refs, l.inst);
            }
        }
        let over_refs = refs.iter().any(|(_, n)| *n > e.caps.max_subs_per_inst);
        let new_insts = refs.iter().filter(|(i, _)| inst.subs.ix_of(*i).is_none()).count();
        if over_refs || inst.subs.insts.len() + new_insts > e.caps.max_insts as usize {
            return fail(drv, pp, res(status::CAPACITY_EXCEEDED, 0));
        }
    }
    // ⑦ lot alignment and dust line (new and fingerprint-changed groups only).
    {
        use hft_strategy_sdk::GroupSpecExt;
        let p = e
            .setup
            .exec
            .accounts
            .iter()
            .find(|x| x.account == account)
            .and_then(|x| x.params)
            .unwrap_or(e.setup.exec.default);
        let mut viol: Option<u16> = None;
        for (_, g) in pp.groups.iter() {
            if viol.is_some() {
                break;
            }
            let is_new = old.as_ref().and_then(|o| {
                o.gix_of(g.id)
                    .map(|gi| o.get(gi).fingerprint() == g.fingerprint())
            }) != Some(true);
            if !is_new {
                continue;
            }
            let legs: Vec<InstId> = g
                .legs
                .iter()
                .take(usize::from(g.n_legs))
                .map(|l| l.inst)
                .collect();
            let mut max_min = 0i64;
            for i in legs.iter() {
                max_min = max_min.max(e.rd.inst(*i).min_notional.0);
            }
            if max_min > p.dust_notional.0 {
                viol = Some(detail::DUST_BELOW_MIN_NOTIONAL);
                break;
            }
            for x in 0..legs.len() {
                for y in x + 1..legs.len() {
                    let (ia, ib) = (legs[x], legs[y]);
                    let (la, lb) = (e.rd.inst(ia).lot, e.rd.inst(ib).lot);
                    if la <= 0 || lb <= 0 {
                        continue;
                    }
                    let divides = la % lb == 0 || lb % la == 0;
                    if divides {
                        continue;
                    }
                    let px = ref_px(e, ia).or_else(|| ref_px(e, ib));
                    let ok = match (hft_types::pair_lot(la, lb, 1, 1), px) {
                        (Some(l), Some(px)) => {
                            let exp = e.rd.asset(e.rd.inst(ia).asset).exp;
                            let n = hft_risk::Risk::notional(
                                l,
                                px,
                                exp,
                                crate::views::quote_rate(&e.rd, &e.ledger, ia),
                            );
                            n.0 <= p.force_slice.max_chunk_notional.0
                        }
                        _ => false,
                    };
                    if !ok {
                        viol = Some(detail::LOT_MISMATCH);
                    }
                }
            }
        }
        if let Some(d) = viol {
            return fail(drv, pp, res(status::INVALID_ARGS, d));
        }
    }
    // ⑧ Model memory and checkpoint budget.
    {
        let mib = |m: u32| u64::from(m) << 20;
        let mut mem = pp.model_mem;
        let mut ckpt = pp.model_ckpt;
        for s in inst.slots.iter() {
            if s.id != strategy {
                mem += s.model_mem;
                ckpt += s.model_ckpt;
            }
        }
        if mem > mib(e.sp.advisor_mem_budget_mib) || ckpt.saturating_mul(2) > mib(e.sp.advisor_ckpt_max_mib) {
            return fail(
                drv,
                pp,
                res_d(
                    status::INVALID_ARGS,
                    detail::pack(
                        detail::PARAM_INVALID,
                        detail::param_arg(detail::PARAM_KIND_MODEL_MEM_BUDGET, 0),
                    ),
                ),
            );
        }
    }
    // Apply.
    let content = e.cmd.blobs.get(&blob.sha256).map(|b| b.to_vec());
    if let Some(c) = content {
        e.cmd.blobs.put(*blob, &c);
    }
    match crate::strategies::apply_params(e, inst, drv, ix, pp, *blob, e.cur_seq)? {
        Ok(old) => {
            if let Some(o) = old {
                e.trace.rec("drv.retire", 0, 0);
                drv.retire(crate::parsed::ParsedConfig::StrategyParams(o));
            }
            crate::subs::rebuild_all(e, inst);
            Ok(ok())
        }
        Err(d) => Ok(res_d(status::INVALID_ARGS, d)),
    }
}

fn bump(v: &mut Vec<(InstId, u16)>, i: InstId) {
    match v.iter_mut().find(|x| x.0 == i) {
        Some(x) => x.1 += 1,
        None => v.push((i, 1)),
    }
}

fn ref_px(e: &Engine, i: InstId) -> Option<hft_types::Px> {
    let m = e.market.row(i).map_or(0, |r| r.mark_px.0);
    if m > 0 {
        return Some(hft_types::Px(m));
    }
    let t = e.books.top(i);
    (t.bid_px.0 > 0 && t.ask_px.0 > 0).then(|| hft_types::Px::mid_floor(t.bid_px, t.ask_px))
}

/// Position, inventory, in-flight or an active target in the group.
fn group_busy(e: &Engine, a: AccountId, g: GroupId) -> bool {
    let gp = e.ledger.group_pos(a, g);
    let legs = &e.oms.legs(a, g).view;
    gp.legs.iter().any(|l| l.qty.0 != 0)
        || legs
            .leg
            .iter()
            .any(|l| l.net.0 != 0 || l.inflight_immediate.0 != 0 || l.inflight_resting.0 != 0)
        || legs.has_pending
        || e.exec.cell(a, g).is_some_and(|c| c.has_target)
}

// ------------------------------------------------------------------------------- others

#[allow(clippy::too_many_arguments)]
fn adopt<D: Driver<S>, S: StrategySet>(
    e: &mut Engine,
    inst: &mut Instances<S>,
    drv: &mut D,
    account: AccountId,
    venue: VenueId,
    inst_id: InstId,
    report_id: u64,
    item: u8,
    asset: hft_types::AssetId,
    group: GroupId,
    entry_px: hft_types::Px,
) -> R<Res> {
    // Stale: the report is not retained (evicted, overflowed or not restored), or a fill on the
    // inst came after the report's view seq.
    let Some(view) = e.cmd.recon_views.view(report_id).filter(|v| !v.overflow) else {
        return Ok(res(status::PRECONDITION_FAILED, detail::STALE_REPORT));
    };
    let last_fill = e.cmd.fill_seq.get(account, inst_id);
    if last_fill > view.view_seq && view.view_seq != 0 {
        return Ok(res(status::PRECONDITION_FAILED, detail::STALE_REPORT));
    }
    let diff = e.cmd.recon_views.diff(report_id, account, inst_id);
    // `AdoptExchange.item` (hft-event `codes::ADOPT_ITEM`): Position = 0, Balance = 1.
    let position = item == 0;
    let kind = if position {
        AdjustKind::Position
    } else {
        AdjustKind::Balance
    };
    let d = AdjustDelta {
        account,
        venue,
        kind: kind as u8,
        px_src: if entry_px.0 != 0 {
            hft_ledger::PxSrc::Cmd as u8
        } else {
            hft_ledger::PxSrc::Mark as u8
        },
        inst: inst_id,
        asset,
        _pad: 0,
        group,
        qty: diff,
        px: if entry_px.0 != 0 {
            entry_px
        } else {
            e.market.row(inst_id).map_or(hft_types::Px(0), |r| r.mark_px)
        },
        report_id,
        cmd_seq: e.cur_seq,
        settle_ts_ns: 0,
    };
    let pnl = e.ledger.apply_adjust(&d, e.now.engine_ns);
    e.trace.rec("ledger.apply_adjust", u64::from(account.0), 0);
    crate::process::ledger_notes(e);
    crate::process::pnl(e, inst, drv, pnl)?;
    if group != GROUP_NONE && position {
        e.trace
            .rec("oms.adopt_adjust", u64::from(account.0), u64::from(group.0));
        e.oms.adopt_adjust(account, group, inst_id, diff);
    }
    let key = if position {
        ReconItemKey::new(ReconItemKey::POSITION, inst_id.0)
    } else {
        ReconItemKey::new(ReconItemKey::BALANCE, u32::from(asset.0) | hft_risk::ASSET_KEY)
    };
    crate::mode_glue::apply_mode(
        e,
        inst,
        drv,
        ModeInput::Gate(GateEvent::Disposed {
            account,
            venue,
            report_id,
            item: key,
            how: hft_risk::mode::Disposition::Adopted,
        }),
    )?;
    Ok(ok())
}

fn sim_reset<D: Driver<S>, S: StrategySet>(
    e: &mut Engine,
    inst: &mut Instances<S>,
    drv: &mut D,
    a: AccountId,
) -> R<Res> {
    let AccountKind::Sim(cfg) = e.rd.account(a).kind.clone() else {
        return Ok(res(status::INVALID_ARGS, detail::NOT_SIM));
    };
    // Preconditions: no Live cell, no target, no Unknown, no non-terminal IOC.
    let live = inst
        .cells()
        .filter(|c| c.account == a)
        .any(|c| matches!(e.risk.mode.effective(c), hft_risk::mode::Effective::Live));
    let target = inst
        .cells()
        .filter(|c| c.account == a)
        .any(|c| e.exec.cell(a, c.group).is_some_and(|x| x.has_target));
    let q = e.oms.quiescent(a);
    let unknown = q & hft_oms::quiesce::UNKNOWN != 0;
    let ioc = q & hft_oms::quiesce::IOC_NONTERMINAL != 0;
    if live || target || unknown || ioc {
        return Ok(res(status::PRECONDITION_FAILED, detail::SIM_NOT_IDLE));
    }
    e.out.emits.push(Emit::Gw {
        ring: OutRing::Sim,
        cmd: hft_event::GwCmd::sim_reset(a),
        rec: None,
    });
    e.trace.rec("ledger.sim_reset", u64::from(a.0), 0);
    e.ledger.sim_reset(a, &cfg.initial_balances, e.now.engine_ns);
    e.trace.rec("oms.sim_reset", u64::from(a.0), 0);
    let _ = e.oms.sim_reset(a);
    e.trace.rec("exec.clear_account", u64::from(a.0), 0);
    let mut xo = e.take_exec();
    e.exec.clear_account(a, e.now, &mut xo);
    crate::exec_glue::drain_out(e, inst, drv, &mut xo)?;
    e.put_exec(xo);
    let groups: Vec<GroupId> = inst.cells().filter(|c| c.account == a).map(|c| c.group).collect();
    for g in groups {
        crate::mode_glue::on_legs(e, inst, drv, a, g)?;
    }
    Ok(ok())
}

#[allow(clippy::too_many_arguments)]
fn smoke<D: Driver<S>, S: StrategySet>(
    e: &mut Engine,
    inst: &mut Instances<S>,
    drv: &mut D,
    account: AccountId,
    venue: VenueId,
    inst_id: InstId,
    side: hft_types::Side,
    ttl_ms: u32,
) -> R<Res> {
    if !matches!(e.rd.account(account).kind, AccountKind::Live) {
        return Ok(res(status::INVALID_ARGS, detail::NOT_LIVE));
    }
    let req = hft_oms::SmokeReq {
        account,
        venue,
        inst: inst_id,
        side,
        ttl_ms,
    };
    let bbo = e.market.bbo_view(&e.books, inst_id);
    let liveness = e.risk.view(account, &e.now).conn_liveness_ms;
    let r = e
        .oms
        .smoke_intent(&req, &bbo, e.rd.inst(inst_id), liveness, e.now.tsc);
    let it = match r {
        Ok(it) => it,
        Err(hft_oms::SmokeErr::Busy) => return Ok(res(status::PRECONDITION_FAILED, detail::SMOKE_ACTIVE)),
        Err(hft_oms::SmokeErr::NoQuote) => {
            return Ok(res(status::PRECONDITION_FAILED, detail::SMOKE_NO_PRICE));
        }
        Err(hft_oms::SmokeErr::BadInst) => return Ok(res(status::INVALID_ARGS, detail::BAD_VALUE)),
    };
    let strategy = e.rd.account(account).bound_instance;
    let out = crate::intents::pipeline(
        e,
        inst,
        drv,
        strategy,
        &it,
        crate::intents::Src::Smoke { ttl_ms },
        0,
    )?;
    Ok(match out {
        crate::intents::PipeOut::Placed(_) => ok(),
        // control-plane: arg = the lowest set `RejectBits` bit.
        crate::intents::PipeOut::Rejected(b) | crate::intents::PipeOut::RateLimited(b, _) if b.0 != 0 => {
            res_d(
                status::REJECTED,
                detail::pack(detail::SMOKE_RISK_REJECT, b.0.trailing_zeros() as u16),
            )
        }
        _ => res(status::REJECTED, detail::SMOKE_RISK_REJECT),
    })
}

fn write_off<D: Driver<S>, S: StrategySet>(
    e: &mut Engine,
    inst: &mut Instances<S>,
    drv: &mut D,
    a: AccountId,
    g: GroupId,
) -> R<Res> {
    let bound = e.rd.account(a).bound_instance;
    let in_params = inst.get(bound).and_then(|s| s.gix_of(g)).is_some();
    if !in_params {
        return Ok(res(status::INVALID_ARGS, detail::UNKNOWN_GROUP));
    }
    let w = match e.oms.write_off_residual(a, g) {
        Ok(w) => w,
        Err(_) => return Ok(res(status::PRECONDITION_FAILED, detail::NOT_QUIESCENT)),
    };
    e.trace
        .rec("oms.write_off_residual", u64::from(a.0), w.qty.0 as u64);
    let d = AdjustDelta {
        account: a,
        venue: w.venue,
        kind: AdjustKind::WriteOff as u8,
        px_src: hft_ledger::PxSrc::Mark as u8,
        inst: w.inst,
        asset: e.rd.inst(w.inst).asset,
        _pad: 0,
        group: g,
        qty: w.qty,
        px: e.market.row(w.inst).map_or(hft_types::Px(0), |r| r.mark_px),
        report_id: 0,
        cmd_seq: e.cur_seq,
        settle_ts_ns: 0,
    };
    let pnl = e.ledger.apply_adjust(&d, e.now.engine_ns);
    e.trace.rec("ledger.apply_adjust", u64::from(a.0), 4);
    // `Note{ResidualWrittenOff}` belongs to this Input.
    crate::process::ledger_notes(e);
    crate::process::pnl(e, inst, drv, pnl)?;
    e.exec.on_leg_adjust(a, g);
    e.trace.rec("exec.on_leg_adjust", u64::from(a.0), u64::from(g.0));
    crate::mode_glue::on_legs(e, inst, drv, a, g)?;
    Ok(ok())
}

/// `ReplaceRefdata` recheck: `None` = pass, else the arg (LoadErrCode, binding code, 0).
fn refdata_recheck<S: StrategySet>(
    e: &Engine,
    inst: &Instances<S>,
    r: &Result<RefData, hft_refdata::LoadReport>,
) -> Option<u16> {
    let new = match r {
        Ok(n) => n,
        Err(rep) => return Some(rep.code() as u16),
    };
    // Binding change with position, orders or Unknown.
    for a in 1..=crate::engine::account_count(&e.rd) {
        let a = AccountId(a as u8);
        if new.account(a).bound_instance != e.rd.account(a).bound_instance {
            let busy = (1..=e.rd.inst_count()).any(|i| e.ledger.pos(a, InstId(i as u32)).qty.0 != 0)
                || e.oms.quiescent(a) != 0
                || e.oms.inflight().open_notional_venue(a, VenueId(0)).0 != 0;
            if busy {
                return Some(hft_refdata::LoadErrCode::BindingChangedWithPosition as u16);
            }
        }
    }
    let _ = inst;
    if new.inst_count() as u32 > e.caps.max_insts.max(new.inst_count() as u32) {
        return Some(0);
    }
    None
}

/// `keep` of the smoke TTL cancel: Draining with `cancel_resting = false`.
pub(crate) fn draining_keep(e: &Engine) -> bool {
    e.risk
        .mode
        .caps()
        .iter()
        .any(|c| c.source == CapSource::DRAINING && c.params[0] == 0)
}

// ------------------------------------------------------------------------------- Drain

/// Starts a Drain (command, SIGTERM, ReplaceRefdata, storage critical, identity mismatch).
pub(crate) fn start_drain<D: Driver<S>, S: StrategySet>(
    e: &mut Engine,
    inst: &mut Instances<S>,
    drv: &mut D,
    cancel_resting: bool,
    cause: DoneCause,
) -> R<()> {
    if e.drain.active {
        return Ok(());
    }
    e.drain = crate::engine::DrainState {
        active: true,
        cancel_resting,
        cause: Some(cause),
        started_ens: e.now.engine_ns,
        ..Default::default()
    };
    e.trace.rec("drain.start", u64::from(cancel_resting), 0);
    let rec = crate::mode_glue::auto_rec(
        CapSource::DRAINING,
        hft_risk::scope_any(),
        e.cur_seq,
        u64::from(cancel_resting),
    );
    crate::mode_glue::apply_mode(e, inst, drv, ModeInput::Auto(rec))?;
    let mut o = e.take_oms();
    if cancel_resting {
        e.oms
            .cancel_all(hft_oms::CancelScope::Global, false, e.now.tsc, e.cur_seq, &mut o);
    } else {
        // Only the resting first legs that carry a LegLink (their fill would leave a naked leg);
        // every other resting order stays.
        let trig = e.trig;
        let first = hft_strategy_sdk::LinkRole::First as u8;
        for a in 1..=crate::engine::account_count(&e.rd) {
            let acc = AccountId(a as u8);
            for v in 0..64u8 {
                if e.rd.account(acc).venues & (1u64 << v) == 0 {
                    continue;
                }
                let venue = VenueId(v);
                let n = e.oms.table_view(venue, acc).map_or(0, |t| t.n_slots);
                for s in 1..=n {
                    let Some((sl, ext, _)) = e.oms.raw_slot(venue, acc, s) else {
                        continue;
                    };
                    let open = matches!(
                        sl.state,
                        hft_oms::OrderState::Live
                            | hft_oms::OrderState::PendingNew
                            | hft_oms::OrderState::PendingAmend
                    );
                    if !open || ext.role != first {
                        continue;
                    }
                    let r = hft_strategy_sdk::OrderRef {
                        venue,
                        account: acc,
                        _pad: [0; 2],
                        slot: s,
                        r#gen: sl.r#gen,
                    };
                    e.trace.rec("oms.cancel", u64::from(s), 2);
                    if e.oms.cancel(r, &trig, e.now.tsc, e.cur_seq, &mut o).is_ok() {
                        e.out.stamped.push(r);
                    }
                }
            }
        }
    }
    crate::intents::drain_oms(e, inst, drv, &mut o)?;
    e.put_oms(o);
    Ok(())
}

/// Drain step 3–6 after every Input.
pub(crate) fn drain_check<D: Driver<S>, S: StrategySet>(
    e: &mut Engine,
    inst: &mut Instances<S>,
    drv: &mut D,
) -> R<()> {
    if !e.drain.active || e.finished.is_some() {
        return Ok(());
    }
    let timeout = e.setup.drain_timeout_ms != 0
        && e.now.engine_ns - e.drain.started_ens >= i64::from(e.setup.drain_timeout_ms) * 1_000_000;
    let ready = drain_ready(e, inst);
    if !ready && !timeout {
        return Ok(());
    }
    if timeout && !ready && !e.drain.timed_out {
        e.drain.timed_out = true;
        drain_timeout_note(e, inst);
    }
    if e.drain.cause == Some(DoneCause::ReplaceRefdata) && !e.drain.recheck_done {
        e.drain.recheck_due = true;
        return Ok(());
    }
    if e.notify.backlog_len() > 0 || e.naked_scan.is_some() || e.oms.wants_continue() {
        return Ok(());
    }
    finish_drain(e, inst, drv)
}

fn drain_ready<S: StrategySet>(e: &Engine, _inst: &Instances<S>) -> bool {
    for a in 1..=crate::engine::account_count(&e.rd) {
        let a = AccountId(a as u8);
        let q = e.oms.quiescent(a);
        let blocking = hft_oms::quiesce::LINKS_ACTIVE
            | hft_oms::quiesce::IOC_NONTERMINAL
            | hft_oms::quiesce::HEDGE_RETRY_PENDING;
        if q & blocking != 0 {
            return false;
        }
        for v in 0..64u8 {
            if e.rd.account(a).venues & (1u64 << v) != 0
                && let Some(t) = e.oms.table_view(VenueId(v), a)
                && pending_slots(e, VenueId(v), a, t.n_slots)
            {
                return false;
            }
        }
    }
    e.exec.drain_ready() && e.notify.backlog_len() == 0 && e.naked_scan.is_none()
}

fn pending_slots(e: &Engine, v: VenueId, a: AccountId, n: u32) -> bool {
    for s in 0..n {
        if let Some((sl, _, _)) = e.oms.raw_slot(v, a, s)
            && matches!(
                sl.state,
                hft_oms::OrderState::PendingNew
                    | hft_oms::OrderState::PendingCancel
                    | hft_oms::OrderState::PendingAmend
            )
        {
            return true;
        }
    }
    false
}

fn drain_timeout_note<S: StrategySet>(e: &mut Engine, _inst: &Instances<S>) {
    let mut bitmap = [0u64; 4];
    let mut links = 0u64;
    for a in 1..=crate::engine::account_count(&e.rd) {
        let acc = AccountId(a as u8);
        let q = e.oms.quiescent(acc);
        if (q
            & (hft_oms::quiesce::LINKS_ACTIVE
                | hft_oms::quiesce::IOC_NONTERMINAL
                | hft_oms::quiesce::HEDGE_RETRY_PENDING)
            != 0
            || !e.exec.drain_ready_acct(acc))
            && let Some(w) = bitmap.get_mut(a / 64)
        {
            *w |= 1u64 << (a % 64);
        }
        if q & hft_oms::quiesce::LINKS_ACTIVE != 0 {
            links += 1;
        }
    }
    let engine_targets = u64::from(e.exec.engine_reduce_pending());
    let level = if links > 0 || engine_targets > 0 {
        NoteLevel::P0
    } else {
        NoteLevel::P1
    };
    e.seq_note(
        note::DRAIN_TIMEOUT,
        level,
        GroupId(0),
        InstId(0),
        [
            bitmap[0],
            bitmap[1],
            bitmap[2],
            bitmap[3],
            links,
            0,
            engine_targets,
            0,
        ],
    );
}

fn finish_drain<D: Driver<S>, S: StrategySet>(e: &mut Engine, inst: &mut Instances<S>, drv: &mut D) -> R<()> {
    crate::snapshot::write_snapshot(e, inst, drv, crate::snapshot::PURPOSE_DRAIN)?;
    let cause = e.drain.cause.unwrap_or(DoneCause::Drain);
    let (restart, cause) = match cause {
        DoneCause::ReplaceRefdata if e.drain.recheck_ok => (true, DoneCause::ReplaceRefdata),
        DoneCause::ReplaceRefdata => (false, DoneCause::Drain),
        c => (false, c),
    };
    e.finished = Some(StepOutcome::Done { restart, cause });
    Ok(())
}

// ------------------------------------------------------------------------------- Sys timers

pub(crate) fn sys_timer<'a, D: Driver<S>, S: StrategySet>(
    e: &mut Engine,
    inst: &mut Instances<S>,
    drv: &mut D,
    id: u32,
    slot: &'a InputSlot,
    _p: &mut Pending<'a>,
) -> R<()> {
    let k = id & 0xFF;
    let arg = id >> 8;
    match k {
        1 => crate::snapshot::write_snapshot(e, inst, drv, crate::snapshot::PURPOSE_KEYFRAME)?,
        2 => crate::snapshot::roll(e, inst, drv, slot.tsc)?,
        3 => {
            let day = arg;
            if day > e.last_day_rolled {
                e.trace.rec("risk.on_day_roll", u64::from(day), 0);
                let eq = crate::process::LedgerEquity(&e.ledger);
                e.risk.on_day_roll(day, &eq);
                e.trace.rec("ledger.on_day_roll", 0, 0);
                e.ledger.on_day_roll(e.now.engine_ns);
                e.last_day_rolled = day;
            }
        }
        4 => {
            let v = VenueId((arg & 0xFF) as u8);
            let a = AccountId(((arg >> 8) & 0xFF) as u8);
            let caps = e.rd.caps(v);
            let refresh = caps.dms_refresh_ms.max(1);
            let timeout = caps.dms_timeout_ms.max(2 * refresh);
            let mut c = hft_event::GwCmd::sim_reset(a);
            c.op = hft_event::GwOp::DeadManRefresh;
            c.arg = timeout;
            e.out.emits.push(Emit::Gw {
                ring: OutRing::Venue(v),
                cmd: c,
                rec: None,
            });
            let ms = e.now.tsc_per_ms.max(1);
            e.heap
                .push(crate::timers::TimerEntry {
                    deadline: e.now.tsc.0 + u64::from(refresh) * ms,
                    owner: 1,
                    timer_id: id,
                    group: 0,
                })
                .map_err(|_| FatalReason::TimerOverflow)?;
        }
        5 => {
            let v = VenueId((arg & 0xFF) as u8);
            let pending = crate::engine::live_gw_venues(&e.rd)
                .iter()
                .find(|x| x.0 == v)
                .is_some_and(|x| {
                    x.1.iter().any(|a| {
                        e.oms
                            .table_view(v, *a)
                            .is_some_and(|t| pending_slots(e, v, *a, t.n_slots))
                    })
                });
            if !pending {
                let mut c = hft_event::GwCmd::sim_reset(AccountId(0));
                c.op = hft_event::GwOp::Rotate;
                e.out.emits.push(Emit::Gw {
                    ring: OutRing::Venue(v),
                    cmd: c,
                    rec: None,
                });
            }
        }
        6 => crate::advisor::model_tick(e, inst),
        7 => {
            crate::snapshot::write_snapshot(e, inst, drv, crate::snapshot::PURPOSE_ON_DEMAND)?;
            e.snapshot_requested = false;
        }
        8 | 9 => {
            e.clock_ok = k == 8;
            e.now = e.now_of(slot.tsc, e.cur_seq);
            e.trace.rec("apply.Clock", u64::from(e.clock_ok), 0);
            crate::mode_glue::apply_mode(e, inst, drv, ModeInput::Clock { ok: e.clock_ok })?;
            crate::mode_glue::apply_mode(e, inst, drv, ModeInput::Gate(GateEvent::Clock { ok: e.clock_ok }))?;
        }
        10 => {
            // The count of Bbo dropped by conflation rides in `deadline_tsc` (the live driver's
            // decision, journaled with the Timer).
            let n = slot.read::<hft_event::Timer>().map_or(0, |t| t.deadline_tsc);
            e.conflated = 0;
            e.seq_note(
                note::CONFLATED,
                NoteLevel::Info,
                GroupId(0),
                InstId(0),
                [n, 0, 0, 0, 0, 0, 0, 0],
            );
        }
        11 => {
            let v = VenueId((arg & 0xFF) as u8);
            let a = AccountId(((arg >> 8) & 0xFF) as u8);
            let mut c = hft_event::GwCmd::sim_reset(a);
            c.op = hft_event::GwOp::SessionKeepalive;
            e.out.emits.push(Emit::Gw {
                ring: OutRing::Venue(v),
                cmd: c,
                rec: None,
            });
        }
        12 | 13 => {
            let kind = if k == 12 {
                StorageKind::PersistLag
            } else {
                StorageKind::DiskLow
            };
            let level = (arg & 0xFF) as u8;
            e.trace.rec("apply.Storage", k.into(), u64::from(level));
            crate::mode_glue::apply_mode(e, inst, drv, ModeInput::Storage { kind, level })?;
        }
        14 => {
            e.trace.rec("apply.Storage", 14, 0);
            crate::mode_glue::apply_mode(
                e,
                inst,
                drv,
                ModeInput::Storage {
                    kind: StorageKind::Both,
                    level: 0,
                },
            )?;
        }
        15 => {
            let v = VenueId((arg & 0xFF) as u8);
            // Only a settlement that is due by this record's UTC (the driver writes the Timer
            // when UTC reaches it; a replayed or early Timer finds nothing due).
            let at = crate::driver::next_funding_settle_of(e, v);
            if let Some(t) = at
                && e.now.utc_ns >= t
            {
                synth_funding(e, inst, drv, v, t)?;
            }
        }
        16 => {
            let a = AccountId(((arg >> 8) & 0xFF) as u8);
            e.trace.rec("apply.KillSelfTest", u64::from(a.0), 0);
            crate::mode_glue::apply_mode(
                e,
                inst,
                drv,
                ModeInput::Gate(GateEvent::KillSelfTest { account: a }),
            )?;
        }
        17 => {
            let n = usize::from(e.sp.notify_per_continue);
            let mut v: Vec<crate::notify::Queued> = Vec::new();
            e.notify.pop_backlog(n, &mut v);
            e.trace.rec("notify_continue", v.len() as u64, 0);
            // Delivered in this Input's first round (queued ahead of new notifications).
            for q in v {
                e.notify.push_front_queue(q);
            }
            crate::process::naked_scan(e, inst, drv)?;
        }
        18 => {
            refdata_recheck_timer(e, inst, drv, (arg & 0xFF) as u16)?;
        }
        _ => {}
    }
    Ok(())
}

fn refdata_recheck_timer<D: Driver<S>, S: StrategySet>(
    e: &mut Engine,
    inst: &mut Instances<S>,
    drv: &mut D,
    assembly: u16,
) -> R<()> {
    e.drain.recheck_due = false;
    e.drain.recheck_done = true;
    let mut stage_arg: Option<(u64, u64)> = None;
    if assembly != 0 {
        stage_arg = Some((1, u64::from(assembly)));
    } else if let Some(r) = e.cmd.active.pending_refdata {
        let content = e.cmd.blobs.get(&r.sha256).map(|b| b.to_vec()).unwrap_or_default();
        let res = RefData::load(&content, Some(&e.rd));
        match &res {
            Err(rep) => stage_arg = Some((2, rep.code() as u64)),
            Ok(_) => {
                if let Some(a) = refdata_recheck(e, inst, &res) {
                    stage_arg = Some(if a == 0 { (4, 0) } else { (3, u64::from(a)) });
                }
            }
        }
    }
    match stage_arg {
        None => e.drain.recheck_ok = true,
        Some((st, arg)) => {
            e.drain.recheck_ok = false;
            e.cmd.active.pending_refdata = None;
            e.seq_note(
                note::REFDATA_RECHECK_FAILED,
                NoteLevel::P1,
                GroupId(0),
                InstId(0),
                [st, arg, 0, 0, 0, 0, 0, 0],
            );
        }
    }
    drain_check(e, inst, drv)
}

// ------------------------------------------------------------------------------- funding

/// Synthesis of every unsettled (account, inst) of venue `v` up to settle time `at`.
pub(crate) fn synth_funding<D: Driver<S>, S: StrategySet>(
    e: &mut Engine,
    inst: &mut Instances<S>,
    drv: &mut D,
    v: VenueId,
    at: i64,
) -> R<()> {
    let insts: Vec<InstId> = e.rd.insts_of_venue(v).to_vec();
    let n_acc = crate::engine::account_count(&e.rd);
    for a in 1..=n_acc {
        let a = AccountId(a as u8);
        for i in insts.iter().copied() {
            let Some(r) = e.market.row(i).copied() else {
                continue;
            };
            if r.next_funding_ns == 0 || r.next_funding_ns > at || r.settled_cut_ns >= r.next_funding_ns {
                continue;
            }
            if e.ledger.pos(a, i).qty.0 == 0 {
                continue;
            }
            e.trace
                .rec("ledger.synth_funding", u64::from(a.0), u64::from(i.0));
            if let Some(mut f) = e
                .ledger
                .synth_funding(a, i, r.mark_px, r.funding_rate, r.next_funding_ns)
            {
                f.rx_tsc = Tsc(0);
                f.synth = 1;
                e.out.push(RecordFlags(0), &f);
                if let Some(d) = e.ledger.apply_funding(&f, e.now.engine_ns) {
                    crate::process::pnl(e, inst, drv, d)?;
                }
            }
        }
    }
    for i in insts {
        if let Some(r) = e.market.row_mut(i)
            && r.next_funding_ns != 0
            && r.next_funding_ns <= at
        {
            r.settled_cut_ns = r.next_funding_ns;
        }
    }
    Ok(())
}

// ------------------------------------------------------------------------------- recon

/// `ReconcileRejected.v[2]` reasons (hft-event README "变长记录前缀").
pub mod recon_reject {
    pub const UNDECODABLE: u64 = 1;
    pub const IN_FLIGHT: u64 = 2;
    pub const LEDGER_PREFLIGHT: u64 = 3;
    pub const OMS_PREFLIGHT: u64 = 4;
    /// The report's position diffs exceed the retained-view capacity; it cannot be adopted.
    pub const VIEW_OVERFLOW: u64 = 5;
}

fn recon_rejected(
    e: &mut Engine,
    report_id: u64,
    venue: VenueId,
    account: AccountId,
    reason: u64,
    detail: u64,
) {
    e.seq_note(
        note::RECONCILE_REJECTED,
        NoteLevel::P0,
        GROUP_NONE,
        hft_types::INST_NONE,
        [
            report_id,
            u64::from(venue.0) | u64::from(account.0) << 8,
            reason,
            detail,
            0,
            0,
            0,
            0,
        ],
    );
}

/// One `ReconcileBatch` Input (README "原始凭据补账"). Untrusted warm-plane evidence never stops
/// the engine: an undecodable body, a report_id whose group is still in flight, or a group the
/// ledger/OMS preflight refuses is not applied (`ReconcileRejected` P0, then `ReconcileCash`
/// with applied = 0 when the header is readable). Fatal stays for broken in-engine invariants.
pub(crate) fn reconcile_batch<D: Driver<S>, S: StrategySet>(
    e: &mut Engine,
    inst: &mut Instances<S>,
    drv: &mut D,
    slot: &InputSlot,
) -> R<()> {
    let Ok(batch) = hft_event::ReconcileBatchView::read(slot.body()) else {
        let h = crate::types::read_body_plain::<hft_event::ReconcileBatchHdr>(slot.body());
        let (id, v, a) = h.map_or((0, VenueId(0), AccountId(0)), |h| {
            (h.report_id, h.venue, h.account)
        });
        recon_rejected(e, id, v, a, recon_reject::UNDECODABLE, 0);
        return Ok(());
    };
    let (report_id, venue, account) = (batch.hdr.report_id, batch.hdr.venue, batch.hdr.account);
    let in_flight = e
        .cmd
        .recon
        .iter()
        .any(|g| g.report_id == report_id && g.account == account && g.venue == venue);
    let flags = batch
        .reports()
        .find(|r| r.item == ITEM_SUMMARY)
        .map_or(0, |r| r.sflags);
    let mut applicable = !in_flight
        && flags
            & (ReconcileReport::USERSTREAM_DOWN
                | ReconcileReport::TRUNCATED
                | ReconcileReport::HISTORY_UNCHECKED)
            == 0
        && batch
            .reports()
            .filter(|r| r.item == ITEM_POSITION && r.sflags & ReconcileReport::AUTO_EXTERNAL != 0)
            .all(|r| auto_external_check(e, &r, flags));
    if in_flight {
        recon_rejected(e, report_id, venue, account, recon_reject::IN_FLIGHT, 0);
    } else if applicable {
        if let Err(x) = e.ledger.validate_reconcile_batch(&batch) {
            recon_rejected(
                e,
                report_id,
                venue,
                account,
                recon_reject::LEDGER_PREFLIGHT,
                x as u64,
            );
            applicable = false;
        } else if let Err(x) = e.oms.reconcile_cash_guard(&batch) {
            recon_rejected(
                e,
                report_id,
                venue,
                account,
                recon_reject::OMS_PREFLIGHT,
                x as u64,
            );
            applicable = false;
        }
    }
    e.seq_note(
        note::RECONCILE_CASH,
        NoteLevel::Info,
        GROUP_NONE,
        hft_types::INST_NONE,
        [
            report_id,
            u64::from(venue.0) | u64::from(account.0) << 8,
            u64::from(batch.hdr.n_reports),
            u64::from(batch.hdr.n_fills),
            u64::from(applicable),
            0,
            0,
            0,
        ],
    );
    // A duplicate of a group still being collected must not be merged into its counts.
    if in_flight {
        return Ok(());
    }
    for r in batch.reports() {
        reconcile_row(e, inst, drv, r, Some((&batch, applicable)))?;
    }
    Ok(())
}

pub(crate) fn reconcile<D: Driver<S>, S: StrategySet>(
    e: &mut Engine,
    inst: &mut Instances<S>,
    drv: &mut D,
    slot: &InputSlot,
) -> R<()> {
    let Some(r) = slot.read::<ReconcileReport>() else {
        return Ok(());
    };
    reconcile_row(e, inst, drv, r, None)
}

fn reconcile_row<D: Driver<S>, S: StrategySet>(
    e: &mut Engine,
    inst: &mut Instances<S>,
    drv: &mut D,
    r: ReconcileReport,
    batch: Option<(&hft_event::ReconcileBatchView<'_>, bool)>,
) -> R<()> {
    let key = (r.report_id, r.account, r.venue);
    let gi = match e
        .cmd
        .recon
        .iter()
        .position(|g| (g.report_id, g.account, g.venue) == key)
    {
        Some(i) => i,
        None => {
            e.cmd.recon.push(ReconGroupState {
                report_id: r.report_id,
                account: r.account,
                venue: r.venue,
                count: r.count,
                received: 0,
                stored: Vec::with_capacity(usize::from(r.count)),
            });
            e.cmd.recon.len() - 1
        }
    };
    if let Some(g) = e.cmd.recon.get_mut(gi) {
        g.received += 1;
    }
    if r.item == ITEM_FUNDING {
        let it = FundingReconItem {
            account: r.account,
            venue: r.venue,
            inst: r.inst,
            settle_ts_ns: r.exch_ts_ns,
            exch_amount: r.exch_amount,
            engine_amount: r.engine_amount,
            report_id: r.report_id,
        };
        e.trace.rec("ledger.reconcile_funding", u64::from(r.account.0), 0);
        let fr = e.ledger.reconcile_funding(&it, e.now.utc_ns, e.now.engine_ns);
        // `Note{FundingCorrected}` belongs to this Input (cold readers book the correction from it).
        crate::process::ledger_notes(e);
        if let FundingRecon::Corrected { pnl, .. } = fr {
            crate::process::pnl(e, inst, drv, pnl)?;
        }
    } else if let Some(g) = e.cmd.recon.get_mut(gi) {
        g.stored.push(r);
    }
    if e.cmd.recon_views.record(&r, r.item == ITEM_POSITION) {
        recon_rejected(
            e,
            r.report_id,
            r.venue,
            r.account,
            recon_reject::VIEW_OVERFLOW,
            RECON_VIEW_DIFFS as u64,
        );
    }
    let done = e.cmd.recon.get(gi).is_some_and(|g| g.received >= g.count);
    if !done {
        return Ok(());
    }
    let g = e.cmd.recon.remove(gi);
    // ① AUTO_EXTERNAL.
    let summary_flags = g
        .stored
        .iter()
        .find(|x| x.item == ITEM_SUMMARY)
        .map_or(0, |x| x.sflags);
    for it in g
        .stored
        .iter()
        .filter(|x| x.item == ITEM_POSITION && x.sflags & ReconcileReport::AUTO_EXTERNAL != 0)
    {
        if batch.map_or_else(
            || auto_external_check(e, it, summary_flags),
            |(_, applies)| applies,
        ) {
            if let Some((batch, _)) = batch {
                for fill in batch.fills().filter(|f| f.inst == it.inst) {
                    crate::process::external_fill(e, inst, drv, &fill, true, false)?;
                }
            } else {
                let mark = e.market.row(it.inst).map_or(hft_types::Px(0), |r| r.mark_px);
                let x = hft_ledger::Ledger::auto_external_fill(
                    it.account,
                    it.venue,
                    it.inst,
                    Qty(it.engine_qty.0 - it.exch_qty.0),
                    mark,
                    it.report_id,
                );
                crate::process::external_fill(e, inst, drv, &x, true, false)?;
            }
            crate::mode_glue::apply_mode(
                e,
                inst,
                drv,
                ModeInput::Gate(GateEvent::Disposed {
                    account: it.account,
                    venue: it.venue,
                    report_id: it.report_id,
                    item: ReconItemKey::new(ReconItemKey::POSITION, it.inst.0),
                    how: hft_risk::mode::Disposition::AutoExternal,
                }),
            )?;
        }
    }
    // ② OMS and risk grading.
    let h = hft_event::RecordHeader {
        len: 0,
        kind: EventKind::RECONCILE_REPORT.0,
        flags: 0,
        seq: e.cur_seq,
        tsc: e.now.tsc.0,
    };
    // A group of Funding items only touches neither the gate nor the Recon cap (SEQ-163).
    let graded = !g.stored.is_empty();
    if graded {
        for it in g.stored.iter() {
            let mut o = e.take_oms();
            e.oms.on_reconcile(it, &h, &mut o);
            crate::intents::drain_oms(e, inst, drv, &mut o)?;
            e.put_oms(o);
        }
        e.trace.rec("oms.on_reconcile", g.stored.len() as u64, 0);
        let v = e.risk.on_reconcile(&ReconcileGroup { items: &g.stored });
        e.trace.rec("risk.on_reconcile", u64::from(v.gate.is_some()), 0);
        if let Some(m) = v.recon {
            crate::mode_glue::apply_mode(e, inst, drv, m)?;
        }
        if let Some(gi) = v.gate {
            e.trace.rec("apply.Gate.Recon", 0, 0);
            crate::mode_glue::apply_mode(e, inst, drv, ModeInput::Gate(GateEvent::Recon(&gi)))?;
        }
    }
    // ③ funding window sweep.
    let mut outv = hft_ledger::FixedVec::<FundingRecon, 64>::new();
    e.trace
        .rec("ledger.funding_window_sweep", u64::from(g.account.0), 0);
    e.ledger
        .funding_window_sweep(g.account, g.venue, e.now.utc_ns, e.now.engine_ns, &mut outv);
    crate::process::ledger_notes(e);
    for fr in outv.iter().copied().collect::<Vec<_>>() {
        if let FundingRecon::Corrected { pnl, .. } = fr {
            crate::process::pnl(e, inst, drv, pnl)?;
        }
    }
    if graded {
        crate::process::unknown_gate(e, inst, drv, g.account)?;
    }
    Ok(())
}

const ITEM_POSITION: u8 = 1;
const ITEM_SUMMARY: u8 = 5;
const ITEM_FUNDING: u8 = 6;

fn auto_external_check(e: &Engine, it: &ReconcileReport, summary_flags: u16) -> bool {
    let (a, v, i) = (it.account, it.venue, it.inst);
    if !e.risk.auto_external_ok(a, v, summary_flags) {
        return false;
    }
    if e.oms.table_view(v, a).is_some_and(|t| t.unknown_count > 0) {
        return false;
    }
    let fl = e.oms.inflight().pos(a, i);
    if fl.open_buy.0 != 0 || fl.open_sell.0 != 0 {
        return false;
    }
    let last_fill = e.cmd.fill_seq.get(a, i);
    if last_fill > it.view_seq {
        return false;
    }
    let eng = e.ledger.pos(a, i).qty.0;
    if eng != it.engine_qty.0 {
        return false;
    }
    let ex = it.exch_qty.0;
    let same_sign = ex == 0 || (ex > 0) == (eng > 0);
    same_sign && ex.abs() < eng.abs()
}

// Fill seq tracking for StaleReport / AUTO_EXTERNAL (kept in CmdState).

impl Default for ReconPendingHdr {
    fn default() -> ReconPendingHdr {
        <ReconPendingHdr as zerocopy::FromZeros>::new_zeroed()
    }
}

impl Default for VenueNoticeRow {
    fn default() -> VenueNoticeRow {
        <VenueNoticeRow as zerocopy::FromZeros>::new_zeroed()
    }
}
