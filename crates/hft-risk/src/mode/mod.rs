//! `hft-risk::mode`: the only home of the desired-mode table (`ModeTable`), the cap set
//! (including the `MarginStress` tier), the effective mode, the start-up gates,
//! `ctl_state_version` and the kill projection. The sequencer calls `Mode::apply()` and
//! dispatches its `ModeOutcome` (README "mode 模块").

mod caps;
mod gates;
mod recon_gate;
mod table;

use hft_event::{AccountConfigReportHdr, CmdScope, Note};
use hft_refdata::{AccountKind, MarginMode, MultiAsset, PositionMode, RefData, UidPolicy};
use hft_strategy_sdk::RiskEvent;
use hft_types::{AccountId, GROUP_ALL, GroupId, InstId, KillBits, MarketKind, Qty, StrategyId, VenueId};

pub use hft_event::CapRecord;
pub use hft_types::{CapSource, KillLevel, RunMode};

pub use recon_gate::{ReconGateInput, ReconItemKey, recon_flags};

use crate::params::{account_count, venues_of};
use crate::types::{Capacities, GwMap, ModeCfg};
use crate::vec::{FixedVec, PreVec};

pub(crate) use caps::scope_of;
pub(crate) use gates::GateState;
pub use gates::mismatch;
pub(crate) use recon_gate::{PREV_US_DOWN, ReconGateRow, UsRow};
pub(crate) use table::{CellRow, DefaultRow};

/// Derived cap level (never stored or sent; 2 is unused).
#[repr(u8)]
#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug)]
pub enum CapLevel {
    None = 0,
    CloseOnly = 1,
    Hard = 3,
}

/// `Kill` + HARD → `Hard`; everything else → `CloseOnly`.
pub fn level_of(r: &CapRecord) -> CapLevel {
    if r.source == CapSource::KILL && r.cflags & CapRecord::HARD != 0 {
        CapLevel::Hard
    } else {
        CapLevel::CloseOnly
    }
}

/// `Kill` cap `params[0]` bits.
pub mod kill_params {
    pub const CANCEL_RESTING: u64 = 1;
    pub const FLATTEN: u64 = 2;
}

/// `AccountCfg` cap `params[0]` bits.
pub mod cfg_params {
    pub const MISMATCH: u64 = 1;
    pub const LEVERAGE: u64 = 2;
}

/// `gate_bits` values.
pub mod gate_bits {
    pub const FEEDS: u32 = 1;
    pub const ORDER_CONNS: u32 = 2;
    pub const NO_UNKNOWN: u32 = 4;
    pub const RECONCILED: u32 = 8;
    pub const CLOCK: u32 = 16;
    pub const ACCOUNT_CFG: u32 = 32;
    pub const IDENTITY: u32 = 64;
    pub const ALL: u32 = 127;
}

/// `RecoveryFlags` (u8, defined here).
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, Default)]
pub struct RecoveryFlags(pub u8);

impl RecoveryFlags {
    pub const TAIL_LOST: u8 = 1;
    pub const RECOVER_MISMATCH: u8 = 2;
    pub const POISON: u8 = 4;
    pub const FROM_SNAPSHOT: u8 = 8;
    pub const CLEAN_DRAIN: u8 = 16;
}

/// Kill command.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub struct KillCmd {
    pub level: KillLevel,
    pub scope: CmdScope,
    pub cancel_resting: bool,
    pub flatten: bool,
    pub hard: bool,
}

/// Storage degradation input kind (`Both` = `StorageOk`).
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub enum StorageKind {
    PersistLag,
    DiskLow,
    Both,
}

/// `VenueNotice` command payload (kind: 1 Delisting, 2 FundingInterval, 3 Maintenance).
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub struct VenueNoticeCmd {
    pub venue: VenueId,
    pub kind: u8,
    /// 0 = the whole venue.
    pub inst: InstId,
    /// Delisting: `at_ns`.
    pub a: i64,
    pub b: i64,
}

impl VenueNoticeCmd {
    pub const DELISTING: u8 = 1;
    pub const FUNDING_INTERVAL: u8 = 2;
    pub const MAINTENANCE: u8 = 3;
}

/// Disposition of one non-Ignore reconciliation item.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub enum Disposition {
    /// `AdoptExchange` (position or balance item).
    Adopted,
    /// `SetBalanceBaseline`.
    Baselined,
    /// `AUTO_EXTERNAL` position item booked by the sequencer.
    AutoExternal,
}

/// Gate inputs.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum GateEvent<'a> {
    Feeds {
        account: AccountId,
        ok: bool,
    },
    /// `primary_up` = main order connection logged in and user stream Up; `standby_up` = hot standby Up.
    OrderConns {
        account: AccountId,
        venue: VenueId,
        primary_up: bool,
        standby_up: bool,
    },
    Unknown {
        account: AccountId,
        count: u32,
    },
    Clock {
        ok: bool,
    },
    Recon(&'a ReconGateInput),
    Disposed {
        account: AccountId,
        venue: VenueId,
        report_id: u64,
        item: ReconItemKey,
        how: Disposition,
    },
    AccountCfg {
        hdr: &'a AccountConfigReportHdr,
        /// Trading keys of the (account, venue) (SESSION = 1, REST = 2).
        keys_required: u8,
    },
    KillSelfTest {
        account: AccountId,
    },
}

/// A cell of the mode table.
#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug)]
pub struct CellKey {
    pub strategy: StrategyId,
    pub account: AccountId,
    pub group: GroupId,
}

/// Inputs of `Mode::apply`.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum ModeInput<'a> {
    SetMode {
        scope: CmdScope,
        mode: RunMode,
    },
    Kill(KillCmd),
    Resume {
        source: CapSource,
        scope: CmdScope,
    },
    Auto(CapRecord),
    AutoClear {
        source: CapSource,
        scope: CmdScope,
    },
    Clock {
        ok: bool,
    },
    Storage {
        kind: StorageKind,
        level: u8,
    },
    StorageRecheck,
    Notice(&'a VenueNoticeCmd),
    Gate(GateEvent<'a>),
    CellFlat {
        cell: CellKey,
        flat: bool,
    },
    GroupsChanged {
        added: &'a [CellKey],
        removed: &'a [CellKey],
    },
    RecoveryFlags {
        account: AccountId,
        flags: RecoveryFlags,
    },
}

/// Effective mode of a cell.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub enum Effective {
    Shadow,
    CloseOnly { deferred_shadow: bool },
    Live,
}

/// Range of resting orders to cancel.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub enum CancelScope {
    Global,
    Account(AccountId),
    /// Every account on the venue.
    Venue(VenueId),
    VenueAccount(VenueId, AccountId),
    /// Cell (fields 0 / `GROUP_ALL` = any).
    Cell(CellKey),
    Inst(VenueId, InstId),
}

#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub struct ModeCancel {
    pub scope: CancelScope,
    /// Cancel only non risk-reducing orders.
    pub open_only: bool,
}

#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub struct GateChange {
    pub account: AccountId,
    pub old_bits: u32,
    pub new_bits: u32,
    pub released: bool,
}

#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub enum DrainCause {
    StorageCritical,
    IdentityMismatch { account: AccountId, venue: VenueId },
}

/// Rejection of `apply` (first in the documented order; nothing changes).
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub enum ModeErr {
    GateNotReleased,
    Draining,
    NakedNotCleared,
    BrokenRemaining,
    ReconNotClean,
    ResumeLimit,
    NoMatchingCap,
    NotResumable,
    InvalidScope,
    UnknownPending,
}

/// Facts outside the mode state that `apply` reads (the sequencer's adapter; read-only).
pub trait ModeView {
    fn cell_flat(&self, c: CellKey) -> bool;
    fn naked_cleared(&self, a: AccountId, g: GroupId) -> bool;
    fn recon_resumable(&self, a: AccountId, v: VenueId) -> bool;
    fn daily_loss_resumable(&self, a: AccountId) -> bool;
    fn unknown_count(&self, a: AccountId, v: VenueId) -> u32;
    fn lev_bad(&self, a: AccountId, v: VenueId) -> bool;
    fn engine_reduce_pending(&self) -> bool;
}

/// Outputs of one `apply` (capacities are construction bounds; cleared by every `apply`).
#[derive(Clone, Debug)]
pub struct ModeOutcome {
    pub cells: PreVec<(CellKey, Effective, Effective)>,
    pub cancels: PreVec<ModeCancel>,
    pub kill_changed: bool,
    pub resolve_broken: bool,
    pub released: PreVec<AccountId>,
    pub hedge_gate_open: PreVec<(AccountId, VenueId)>,
    pub venue_gate_open: PreVec<(AccountId, VenueId)>,
    pub gate_changes: PreVec<GateChange>,
    pub risk_events: PreVec<(AccountId, RiskEvent)>,
    pub drain: Option<DrainCause>,
    pub daily_loss_resumed: PreVec<AccountId>,
    pub notes: PreVec<Note>,
    pub ctl_state_version: u64,
}

impl ModeOutcome {
    /// N = `max_cells_total`, A = accounts, AV = (account, venue) pairs.
    pub fn with_capacity(n: usize, a: usize, av: usize) -> ModeOutcome {
        ModeOutcome {
            cells: PreVec::with_capacity(n),
            cancels: PreVec::with_capacity(n.max(8)),
            kill_changed: false,
            resolve_broken: false,
            released: PreVec::with_capacity(a),
            hedge_gate_open: PreVec::with_capacity(av),
            venue_gate_open: PreVec::with_capacity(av),
            gate_changes: PreVec::with_capacity(a),
            risk_events: PreVec::with_capacity(n.saturating_add(a.saturating_mul(2)).max(8)),
            drain: None,
            daily_loss_resumed: PreVec::with_capacity(a),
            notes: PreVec::with_capacity(8),
            ctl_state_version: 0,
        }
    }

    pub fn clear(&mut self) {
        self.cells.clear();
        self.cancels.clear();
        self.kill_changed = false;
        self.resolve_broken = false;
        self.released.clear();
        self.hedge_gate_open.clear();
        self.venue_gate_open.clear();
        self.gate_changes.clear();
        self.risk_events.clear();
        self.drain = None;
        self.daily_loss_resumed.clear();
        self.notes.clear();
    }
}

/// One undisposed reconciliation item listed by `PREVIEW`.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub struct PreviewItem {
    pub account: AccountId,
    pub venue: VenueId,
    pub key: ReconItemKey,
    /// Previous-round difference (`ReconRow.last_diff`; filled by `Risk::preview`).
    pub last_diff: Qty,
}

/// `PREVIEW` result (no state change).
#[derive(Clone, Debug, Default)]
pub struct Preview {
    pub active_caps: u32,
    /// Bit n = an active cap of `CapSource(n)`.
    pub cap_sources: u32,
    /// (account, venue, undisposed item count) of each affected venue.
    pub counts: FixedVec<(AccountId, VenueId, u16), 64>,
    pub items: FixedVec<PreviewItem, 64>,
}

/// Refdata facts the mode module keeps (copied once at construction).
#[derive(Clone, Debug)]
pub(crate) struct Facts {
    pub n_accounts: usize,
    pub n_insts: usize,
    /// account → venue bitmap.
    pub venues: Vec<u64>,
    pub sim: Vec<bool>,
    pub unverifiable: Vec<bool>,
    pub bound: Vec<StrategyId>,
    /// Every account is `Sim`.
    pub sim_only: bool,
    /// venue → (required position mode code, margin mode code, multi asset code; 0 = not checked).
    pub req: Vec<(u8, u8, u8)>,
    /// Perp venue bitmap.
    pub perp: u64,
    /// gw → venue.
    pub gw_venue: Vec<VenueId>,
    /// account × 64 + venue → kill-word account index (255 = none).
    pub kix: Vec<u8>,
}

const fn pos_code(p: PositionMode) -> u8 {
    match p {
        PositionMode::NotApplicable => 0,
        PositionMode::OneWay => 2,
        PositionMode::Hedge => 3,
    }
}
const fn margin_code(m: MarginMode) -> u8 {
    match m {
        MarginMode::NotApplicable => 0,
        MarginMode::Cross => 2,
        MarginMode::Isolated => 3,
        MarginMode::Mixed => 4,
    }
}

impl Facts {
    pub fn new(rd: &RefData, gws: &GwMap) -> Facts {
        let n = account_count(rd);
        let mut f = Facts {
            n_accounts: n,
            n_insts: rd.inst_count(),
            venues: vec![0; n + 1],
            sim: vec![false; n + 1],
            unverifiable: vec![false; n + 1],
            bound: vec![StrategyId(0); n + 1],
            sim_only: n > 0,
            req: vec![(0, 0, 0); 64],
            perp: 0,
            gw_venue: gws.venues.clone(),
            kix: vec![u8::MAX; (n + 1) * 64],
        };
        for a in 1..=n {
            let id = AccountId(a as u8);
            let m = rd.account(id);
            if let Some(x) = f.venues.get_mut(a) {
                *x = m.venues;
            }
            let sim = matches!(m.kind, AccountKind::Sim(_));
            if let Some(x) = f.sim.get_mut(a) {
                *x = sim;
            }
            f.sim_only &= sim;
            if let Some(x) = f.unverifiable.get_mut(a) {
                *x = m.uid_policy == UidPolicy::Unverifiable;
            }
            if let Some(x) = f.bound.get_mut(a) {
                *x = m.bound_instance;
            }
            for v in venues_of(rd, id) {
                if let Some(ix) = rd.venue_account_ix(VenueId(v), id)
                    && let Some(x) = f.kix.get_mut(a * 64 + usize::from(v))
                {
                    *x = ix;
                }
            }
        }
        for v in 0u8..64 {
            let c = rd.caps(VenueId(v));
            let ma = match c.required_multi_asset {
                None => 0,
                Some(false) => MultiAsset::Off as u8,
                Some(true) => MultiAsset::On as u8,
            };
            if let Some(x) = f.req.get_mut(usize::from(v)) {
                *x = (
                    pos_code(c.required_position_mode),
                    margin_code(c.required_margin_mode),
                    ma,
                );
            }
            if VenueId(v).market() == Some(MarketKind::LinearPerp) {
                f.perp |= 1u64 << v;
            }
        }
        f
    }
    #[inline]
    pub fn venues(&self, a: AccountId) -> u64 {
        self.venues.get(usize::from(a.0)).copied().unwrap_or(0)
    }
    #[inline]
    pub fn is_sim(&self, a: AccountId) -> bool {
        self.sim.get(usize::from(a.0)).copied().unwrap_or(false)
    }
    pub fn accounts(&self) -> impl Iterator<Item = AccountId> {
        (1..=self.n_accounts).map(|a| AccountId(a as u8))
    }
}

/// The mode state (single writer: the sequencer thread).
#[derive(Clone, Debug)]
pub struct Mode {
    pub(crate) f: Facts,
    pub(crate) cfg: ModeCfg,
    pub(crate) cells: PreVec<CellRow>,
    pub(crate) defaults: PreVec<DefaultRow>,
    pub(crate) caps: PreVec<CapRecord>,
    pub(crate) global_mask: u32,
    pub(crate) acct_mask: Vec<u32>,
    pub(crate) av_mask: Vec<u32>,
    pub(crate) hard_global: bool,
    pub(crate) hard_acct: Vec<bool>,
    pub(crate) flatten_acct: Vec<bool>,
    pub(crate) inst_blocked: Vec<bool>,
    pub(crate) venue_blocked: u64,
    pub(crate) kill_bits: Vec<KillBits>,
    pub(crate) gates: Vec<GateState>,
    pub(crate) recon: Vec<ReconGateRow>,
    pub(crate) us: Vec<UsRow>,
    pub(crate) ctl_state_version: u64,
    pub(crate) storage_drain_sent: bool,
    pub(crate) caps_dirty: bool,
    pub(crate) table_dirty: bool,
    pub(crate) pending_cancels: FixedVec<(ModeCancel, bool), 32>,
    /// Scratch of `apply` (effective modes before the input).
    pub(crate) scratch_eff: PreVec<Effective>,
    pub(crate) scratch_hedge: Vec<u64>,
    pub(crate) scratch_venue: Vec<u64>,
    pub(crate) scratch_bits: Vec<(u32, bool)>,
}

/// Initial `default_mode` of a (strategy, account) without a `SetMode{group = *}` yet.
pub const INITIAL_DEFAULT_MODE: RunMode = RunMode::Shadow;

impl Mode {
    pub(crate) fn new(rd: &RefData, gws: &GwMap, caps: &Capacities, cfg: ModeCfg) -> Mode {
        let f = Facts::new(rd, gws);
        let n = f.n_accounts;
        let cells = caps.max_cells_total as usize;
        let mut m = Mode {
            cfg,
            cells: PreVec::with_capacity(cells),
            defaults: PreVec::with_capacity(n.max(1) * 2),
            caps: PreVec::with_capacity(usize::from(caps.max_caps)),
            global_mask: 0,
            acct_mask: vec![0; n + 1],
            av_mask: vec![0; (n + 1) * 64],
            hard_global: false,
            hard_acct: vec![false; n + 1],
            flatten_acct: vec![false; n + 1],
            inst_blocked: vec![false; f.n_insts + 1],
            venue_blocked: 0,
            kill_bits: vec![KillBits(0); f.gw_venue.len()],
            gates: vec![GateState::default(); n + 1],
            recon: vec![ReconGateRow::default(); (n + 1) * 64],
            us: vec![UsRow::default(); (n + 1) * 64],
            ctl_state_version: 0,
            storage_drain_sent: false,
            caps_dirty: true,
            table_dirty: true,
            pending_cancels: FixedVec::new(),
            scratch_eff: PreVec::with_capacity(cells),
            scratch_hedge: vec![0; n + 1],
            scratch_venue: vec![0; n + 1],
            scratch_bits: vec![(0, false); n + 1],
            f,
        };
        for a in m.f.accounts().collect::<Vec<_>>() {
            m.reset_gate_state(a, 0);
        }
        m
    }

    // ------------------------------------------------------------------------- queries

    /// Cached effective mode, O(log cells); unknown cell → `Live`.
    #[inline]
    pub fn effective(&self, cell: CellKey) -> Effective {
        self.cell_ix(cell)
            .and_then(|i| self.cells.get(i))
            .map_or(Effective::Live, |c| c.eff)
    }

    /// Highest level of the global, account-`a` and (a, v) caps.
    #[inline]
    pub fn venue_level(&self, a: AccountId, v: VenueId) -> CapLevel {
        let m = self.global_mask | self.acct_mask_of(a) | self.av_mask_of(a, v);
        if self.hard_global || self.hard_acct.get(usize::from(a.0)).copied().unwrap_or(false) {
            CapLevel::Hard
        } else if m != 0 {
            CapLevel::CloseOnly
        } else {
            CapLevel::None
        }
    }

    /// `Delisting` on (v, i) or on the whole venue.
    #[inline]
    pub fn inst_blocked(&self, v: VenueId, i: InstId) -> bool {
        self.venue_blocked & (1u64 << (v.0 & 63)) != 0
            || self.inst_blocked.get(i.0 as usize).copied().unwrap_or(false)
    }

    pub fn kill_bits(&self) -> &[KillBits] {
        &self.kill_bits
    }

    pub fn released(&self, a: AccountId) -> bool {
        self.gates.get(usize::from(a.0)).is_some_and(|g| g.released)
    }

    /// Hedge gate subset of (a, v).
    pub fn hedge_gate_ok(&self, a: AccountId, v: VenueId) -> bool {
        self.gates
            .get(usize::from(a.0))
            .is_some_and(|g| g.hedge_bits() & (1u64 << (v.0 & 63)) != 0)
    }

    /// Recovery per-venue release of (a, v).
    pub fn venue_gate_ok(&self, a: AccountId, v: VenueId) -> bool {
        self.gates
            .get(usize::from(a.0))
            .is_some_and(|g| g.venue_bits() & (1u64 << (v.0 & 63)) != 0)
    }

    pub fn ctl_state_version(&self) -> u64 {
        self.ctl_state_version
    }

    /// A Storage level is 2 and there is no `Draining` cap.
    pub fn drain_deferred(&self) -> bool {
        self.storage_level2() && !self.has_source(CapSource::DRAINING) && !self.storage_drain_sent
    }

    /// Confirmation by target value: `SetMode{Live}`, `Resume`, `Kill{Account, flatten}`.
    pub fn is_upgrade(&self, inp: &ModeInput<'_>) -> bool {
        match inp {
            ModeInput::SetMode { mode, .. } => *mode == RunMode::Live,
            ModeInput::Resume { .. } => true,
            ModeInput::Kill(k) => k.level == KillLevel::Account && k.flatten,
            _ => false,
        }
    }

    /// `gate_bits` of an account.
    pub fn gate_bits(&self, a: AccountId) -> u32 {
        self.gates.get(usize::from(a.0)).map_or(0, |g| g.bits)
    }

    /// `reconciled_venues`, `cfg_ok_venues`, `identity_ok_venues`, `conns_ok_venues`,
    /// `standby_ok_venues` of an account. Identity venues count only once the account's kill
    /// self-test has passed (README `IDENTITY` row); the snapshot keeps the per-venue reports.
    pub fn venue_bitmaps(&self, a: AccountId) -> [u64; 5] {
        self.gates.get(usize::from(a.0)).map_or([0; 5], |g| {
            let ident = if g.selftest { g.identity_ok } else { 0 };
            [g.reconciled, g.cfg_ok, ident, g.conns_ok, g.standby_ok]
        })
    }

    /// Active cap records in (source, scope) order.
    pub fn caps(&self) -> &[CapRecord] {
        self.caps.as_slice()
    }

    /// The cap record of exactly (source, scope).
    pub fn cap(&self, source: CapSource, scope: &CmdScope) -> Option<&CapRecord> {
        self.find_cap(source, scope).and_then(|i| self.caps.get(i))
    }

    /// Desired mode of a cell (`None` = no row).
    pub fn desired(&self, cell: CellKey) -> Option<RunMode> {
        self.cell_ix(cell)
            .and_then(|i| self.cells.get(i))
            .map(|c| c.desired)
    }

    /// The (strategy, account) `default_mode`, if written.
    pub fn default_mode(&self, s: StrategyId, a: AccountId) -> Option<RunMode> {
        self.defaults
            .iter()
            .find(|d| d.strategy == s && d.account == a)
            .map(|d| d.mode)
    }

    /// Cells of the table in key order.
    pub fn cells(&self) -> impl Iterator<Item = (CellKey, RunMode, Effective)> + '_ {
        self.cells.iter().map(|c| (c.key, c.desired, c.eff))
    }

    /// Active flatten lock of an account.
    pub fn flatten_lock(&self, a: AccountId) -> bool {
        self.flatten_acct.get(usize::from(a.0)).copied().unwrap_or(false)
    }

    /// HARD covers the account (global or account HARD).
    pub fn hard(&self, a: AccountId) -> bool {
        self.hard_global || self.hard_acct.get(usize::from(a.0)).copied().unwrap_or(false)
    }

    #[inline]
    pub(crate) fn acct_mask_of(&self, a: AccountId) -> u32 {
        self.acct_mask.get(usize::from(a.0)).copied().unwrap_or(0)
    }
    #[inline]
    pub(crate) fn av_mask_of(&self, a: AccountId, v: VenueId) -> u32 {
        self.av_mask
            .get(usize::from(a.0) * 64 + usize::from(v.0 & 63))
            .copied()
            .unwrap_or(0)
    }
    /// Cap-source mask of the cell-level caps covering `cell` plus the global and account masks.
    #[inline]
    pub(crate) fn cell_mask(&self, cell: CellKey) -> (u32, bool) {
        match self.cell_ix(cell).and_then(|i| self.cells.get(i)) {
            Some(c) => (
                c.mask | self.global_mask | self.acct_mask_of(cell.account),
                matches!(c.eff, Effective::CloseOnly { .. }),
            ),
            None => (self.global_mask | self.acct_mask_of(cell.account), false),
        }
    }

    // ------------------------------------------------------------------------- apply

    /// Applies one input (README "mode 模块"); a rejected input changes nothing and leaves
    /// `out` empty with the same `ctl_state_version`.
    pub fn apply(
        &mut self,
        inp: ModeInput<'_>,
        seq: u64,
        v: &impl ModeView,
        out: &mut ModeOutcome,
    ) -> Result<(), ModeErr> {
        out.clear();
        out.ctl_state_version = self.ctl_state_version;
        self.check(&inp, v)?;
        self.snapshot_before();
        let mut changed = false;
        match inp {
            ModeInput::SetMode { scope, mode } => {
                changed |= self.set_mode(&scope, mode, v);
                self.table_dirty = true;
            }
            ModeInput::Kill(k) => changed |= self.kill(&k, seq, out),
            ModeInput::Resume { source, scope } => changed |= self.resume(source, &scope, out),
            ModeInput::Auto(r) => changed |= self.auto(r, seq, out),
            ModeInput::AutoClear { source, scope } => changed |= self.remove_cap(source, &scope, out),
            ModeInput::Clock { ok } => {
                let scope = scope_of(&[]);
                if ok {
                    changed |= self.remove_cap(CapSource::CLOCK_BAD, &scope, out);
                } else {
                    let r = caps::record(CapSource::CLOCK_BAD, 0, CapRecord::CONDITIONAL, scope, [0, 0]);
                    changed |= self.set_cap(r, seq, caps::Origin::Auto, out);
                }
            }
            ModeInput::Storage { kind, level } => changed |= self.storage(kind, level, seq, out),
            ModeInput::StorageRecheck => {}
            ModeInput::Notice(n) => {
                if n.kind == VenueNoticeCmd::DELISTING {
                    let mut s = scope_of(&[]);
                    s.venue = n.venue;
                    s.inst = n.inst;
                    let r = caps::record(CapSource::DELISTING, 0, 0, s, [n.a as u64, 0]);
                    changed |= self.set_cap(r, seq, caps::Origin::Command, out);
                }
            }
            ModeInput::Gate(g) => changed |= self.gate(g, seq, v, out),
            ModeInput::CellFlat { cell, flat } => {
                self.cell_flat(cell, flat);
                self.table_dirty = true;
            }
            ModeInput::GroupsChanged { added, removed } => {
                changed |= self.groups_changed(added, removed, seq, v, out);
                self.table_dirty = true;
            }
            ModeInput::RecoveryFlags { account, flags } => self.reset_gate_state(account, flags.0),
        }
        if changed {
            self.ctl_state_version = self.ctl_state_version.wrapping_add(1);
        }
        self.after(out);
        self.storage_drain(v, out);
        out.ctl_state_version = self.ctl_state_version;
        Ok(())
    }

    /// Validation in the documented order (InvalidScope → NotResumable → Draining →
    /// GateNotReleased → NoMatchingCap → source preconditions).
    pub(crate) fn check(&self, inp: &ModeInput<'_>, v: &impl ModeView) -> Result<(), ModeErr> {
        match inp {
            ModeInput::Kill(k) => {
                let bad = (k.hard && k.level == KillLevel::Venue)
                    || (k.flatten && k.level == KillLevel::Global)
                    || (k.flatten && k.hard)
                    || (k.level == KillLevel::Venue && k.scope.venue.0 == 0)
                    || (k.level == KillLevel::Account && k.scope.account.0 == 0);
                if bad {
                    return Err(ModeErr::InvalidScope);
                }
                Ok(())
            }
            ModeInput::SetMode { scope, mode } => {
                if *mode == RunMode::Live {
                    if self.has_source(CapSource::DRAINING) {
                        return Err(ModeErr::Draining);
                    }
                    if !self.targets_released(scope.account) {
                        return Err(ModeErr::GateNotReleased);
                    }
                }
                Ok(())
            }
            ModeInput::Resume { source, scope } => {
                if !source.resumable() {
                    return Err(ModeErr::NotResumable);
                }
                if self.has_source(CapSource::DRAINING) {
                    return Err(ModeErr::Draining);
                }
                if !self.targets_released(scope.account) {
                    return Err(ModeErr::GateNotReleased);
                }
                let Some(i) = self.find_cap(*source, scope) else {
                    return Err(ModeErr::NoMatchingCap);
                };
                let r = self.caps.get(i).copied().ok_or(ModeErr::NoMatchingCap)?;
                self.resume_precondition(&r, v)
            }
            _ => Ok(()),
        }
    }

    fn targets_released(&self, a: AccountId) -> bool {
        if a.0 == 0 {
            self.f.accounts().all(|x| self.released(x))
        } else {
            self.released(a)
        }
    }

    fn resume_precondition(&self, r: &CapRecord, v: &impl ModeView) -> Result<(), ModeErr> {
        let a = r.scope.account;
        match r.source {
            CapSource::BROKEN | CapSource::REORG => {
                if !v.naked_cleared(a, r.scope.group) {
                    return Err(ModeErr::NakedNotCleared);
                }
            }
            CapSource::HEDGE_FAILURE => {
                if self
                    .caps
                    .iter()
                    .any(|c| c.source == CapSource::BROKEN && c.scope.account == a)
                {
                    return Err(ModeErr::BrokenRemaining);
                }
            }
            CapSource::RECON => {
                if !v.recon_resumable(a, r.scope.venue) {
                    return Err(ModeErr::ReconNotClean);
                }
            }
            CapSource::KILL => {
                if r.cflags & CapRecord::HARD != 0 && !self.hard_release_clean(r) {
                    return Err(ModeErr::ReconNotClean);
                }
            }
            CapSource::DAILY_LOSS => {
                if !v.daily_loss_resumable(a) {
                    return Err(ModeErr::ResumeLimit);
                }
            }
            CapSource::UNKNOWN_TIMEOUT if v.unknown_count(a, r.scope.venue) != 0 => {
                return Err(ModeErr::UnknownPending);
            }
            _ => {}
        }
        Ok(())
    }

    /// Records the state compared after the input (effective modes, gate subsets, gate bits).
    fn snapshot_before(&mut self) {
        self.scratch_eff.clear();
        for c in self.cells.iter() {
            self.scratch_eff.push(c.eff);
        }
        for a in 0..self.gates.len() {
            let (h, v, b) = self.gates.get(a).map_or((0, 0, (0, false)), |g| {
                (g.hedge_bits(), g.venue_bits(), (g.bits, g.released))
            });
            if let Some(x) = self.scratch_hedge.get_mut(a) {
                *x = h;
            }
            if let Some(x) = self.scratch_venue.get_mut(a) {
                *x = v;
            }
            if let Some(x) = self.scratch_bits.get_mut(a) {
                *x = b;
            }
        }
    }

    /// Effective-mode changes, cancels, kill bits, gate changes and the lists derived from them.
    fn after(&mut self, out: &mut ModeOutcome) {
        if self.caps_dirty {
            out.kill_changed = self.recompute_caps();
            self.caps_dirty = false;
            self.table_dirty = true;
        }
        if self.table_dirty {
            self.recompute_effective();
            self.table_dirty = false;
            for (i, c) in self.cells.iter().enumerate() {
                if let Some(old) = self.scratch_eff.get(i).copied()
                    && old != c.eff
                {
                    out.cells.push((c.key, old, c.eff));
                }
            }
        }
        self.resolve_cancels(out);
        for ix in 1..=self.f.n_accounts {
            let a = AccountId(ix as u8);
            let Some(g) = self.gates.get(ix).copied() else {
                continue;
            };
            let (ob, orel) = self.scratch_bits.get(ix).copied().unwrap_or((0, false));
            if ob != g.bits || orel != g.released {
                out.gate_changes.push(GateChange {
                    account: a,
                    old_bits: ob,
                    new_bits: g.bits,
                    released: g.released,
                });
            }
            let oh = self.scratch_hedge.get(ix).copied().unwrap_or(0);
            let ov = self.scratch_venue.get(ix).copied().unwrap_or(0);
            let rise_h = g.hedge_bits() & !oh;
            let rise_v = g.venue_bits() & !ov;
            for vv in 0u8..64 {
                if rise_h & (1u64 << vv) != 0 {
                    out.hedge_gate_open.push((a, VenueId(vv)));
                }
            }
            for vv in 0u8..64 {
                if rise_v & (1u64 << vv) != 0 {
                    out.venue_gate_open.push((a, VenueId(vv)));
                }
            }
        }
    }

    /// Keeps unconditional cancels and the conditional ones covering a cell that turned
    /// CloseOnly; drops ranges covered by a larger one of the same `open_only`.
    fn resolve_cancels(&mut self, out: &mut ModeOutcome) {
        let n = self.pending_cancels.len();
        for i in 0..n {
            let Some((c, conditional)) = self.pending_cancels.get(i).copied() else {
                continue;
            };
            if conditional {
                let hit = out.cells.iter().any(|(k, old, new)| {
                    matches!(new, Effective::CloseOnly { .. })
                        && !matches!(old, Effective::CloseOnly { .. })
                        && cancel_covers(&c.scope, *k)
                });
                if !hit {
                    continue;
                }
            }
            let dup = out.cancels.iter().any(|x| *x == c);
            if !dup {
                out.cancels.push(c);
            }
        }
        self.pending_cancels.clear();
        // Drop ranges subsumed by a larger range with the same open_only.
        let mut i = 0;
        while i < out.cancels.len() {
            let Some(c) = out.cancels.get(i).copied() else {
                break;
            };
            let covered = out
                .cancels
                .iter()
                .enumerate()
                .any(|(j, o)| j != i && o.open_only == c.open_only && subsumes(&o.scope, &c.scope));
            if covered {
                out.cancels.remove(i);
            } else {
                i += 1;
            }
        }
    }

    /// `PREVIEW`: no state change; `Resume{Kill|Recovery}` lists undisposed reconciliation items.
    pub fn preview(&self, inp: &ModeInput<'_>, v: &impl ModeView, out: &mut Preview) {
        let _ = v;
        *out = Preview::default();
        out.active_caps = self.caps.len() as u32;
        for c in self.caps.iter() {
            if c.source.0 < 32 {
                out.cap_sources |= 1u32 << c.source.0;
            }
        }
        if let ModeInput::Resume { source, scope } = inp
            && (*source == CapSource::KILL || *source == CapSource::RECOVERY)
        {
            for a in self
                .f
                .accounts()
                .filter(|a| scope.account.0 == 0 || *a == scope.account)
            {
                if self.f.is_sim(a) {
                    continue;
                }
                for vv in 0u8..64 {
                    if self.f.venues(a) & (1u64 << vv) == 0 {
                        continue;
                    }
                    let row = self.recon_row(a, VenueId(vv));
                    let mut n = 0u16;
                    if let Some(r) = row.filter(|r| r.present) {
                        for (k, key) in r.keys.iter().enumerate().take(usize::from(r.n_items).min(64)) {
                            if r.disposed_mask & (1u64 << k) == 0 {
                                n += 1;
                                out.items.push(PreviewItem {
                                    account: a,
                                    venue: VenueId(vv),
                                    key: *key,
                                    last_diff: Qty(0),
                                });
                            }
                        }
                    }
                    out.counts.push((a, VenueId(vv), n));
                }
            }
        }
    }
}

/// `CmdScope` with the given account (other fields "any").
pub fn account_scope(a: AccountId) -> CmdScope {
    let mut s = scope_of(&[]);
    s.account = a;
    s
}

/// `CmdScope` of (account, venue).
pub fn account_venue_scope(a: AccountId, v: VenueId) -> CmdScope {
    let mut s = account_scope(a);
    s.venue = v;
    s
}

/// `CmdScope` of (account, group).
pub fn account_group_scope(a: AccountId, g: GroupId) -> CmdScope {
    let mut s = account_scope(a);
    s.group = g;
    s
}

/// `CmdScope` of (strategy, account, group).
pub fn cell_scope(s: StrategyId, a: AccountId, g: GroupId) -> CmdScope {
    let mut x = account_group_scope(a, g);
    x.strategy = s;
    x
}

/// `CmdScope` of (venue, inst).
pub fn venue_inst_scope(v: VenueId, i: InstId) -> CmdScope {
    let mut s = scope_of(&[]);
    s.venue = v;
    s.inst = i;
    s
}

const _: () = assert!(GROUP_ALL.0 == u32::MAX);

fn cancel_covers(s: &CancelScope, k: CellKey) -> bool {
    match *s {
        CancelScope::Global => true,
        CancelScope::Account(a) => a == k.account,
        CancelScope::Cell(c) => {
            (c.strategy.0 == 0 || c.strategy == k.strategy)
                && (c.account.0 == 0 || c.account == k.account)
                && (c.group == GROUP_ALL || c.group == k.group)
        }
        CancelScope::Venue(_) | CancelScope::VenueAccount(..) | CancelScope::Inst(..) => false,
    }
}

/// `big` strictly contains `small`.
fn subsumes(big: &CancelScope, small: &CancelScope) -> bool {
    if big == small {
        return false;
    }
    match (*big, *small) {
        (CancelScope::Global, _) => true,
        (CancelScope::Account(a), CancelScope::Cell(c)) => c.account == a,
        (CancelScope::Account(a), CancelScope::VenueAccount(_, b)) => a == b,
        (CancelScope::Venue(v), CancelScope::VenueAccount(w, _)) => v == w,
        (CancelScope::Venue(v), CancelScope::Inst(w, _)) => v == w,
        _ => false,
    }
}
