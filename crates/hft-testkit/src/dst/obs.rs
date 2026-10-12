//! Read-only observations of the system under test (README "6. DST 框架", docs/tests/invariants.md
//! "检查时机与数据源"). An engine adapter copies its components' debug views into these plain rows
//! after every step (`Sut::observe`) and describes each step's internal outputs in [`StepInfo`];
//! the invariant checkers (`invariants.rs`) only read them. Rows carry raw fixed-point values
//! (`i128` = raw × 1 of the asset's exponent) so that checkers never convert units.
//!
//! Every list is sorted by its key so two observations compare and hash deterministically.

use hft_types::{AccountId, AssetId, GroupId, InstId, RunMode, Side, StrategyId, VenueId};

/// Slot state as the OMS debug view reports it (hft-oms "状态机").
#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug)]
pub enum SlotState {
    Free,
    PendingNew,
    Live,
    PendingCancel,
    PendingAmend,
    Unknown,
    Filled,
    Cancelled,
    Rejected,
    Expired,
}

impl SlotState {
    /// `Filled`, `Cancelled`, `Rejected`, `Expired`.
    pub const fn is_terminal(self) -> bool {
        matches!(
            self,
            SlotState::Filled | SlotState::Cancelled | SlotState::Rejected | SlotState::Expired
        )
    }
    /// `PendingNew`, `Live`, `PendingCancel`, `PendingAmend` (the states an INV-1 transport input
    /// may only move to `Unknown`).
    pub const fn is_inflight(self) -> bool {
        matches!(
            self,
            SlotState::PendingNew | SlotState::Live | SlotState::PendingCancel | SlotState::PendingAmend
        )
    }
}

/// One non-`Free` OMS slot.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub struct SlotRow {
    pub venue: VenueId,
    pub account: AccountId,
    pub slot: u32,
    pub r#gen: u32,
    pub epoch: u16,
    pub state: SlotState,
    pub group: GroupId,
    pub inst: InstId,
    pub side: Side,
    /// Order quantity (raw).
    pub qty: i128,
    /// Cumulative filled (raw).
    pub filled: i128,
    /// IOC/FOK.
    pub immediate: bool,
    pub risk_reducing: bool,
    /// Engine time of entering a terminal state; 0 while not terminal.
    pub terminal_ens: i64,
}

/// One (venue, account) slot table.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub struct SlotTableRow {
    pub venue: VenueId,
    pub account: AccountId,
    pub capacity: u32,
    /// Slots in the free list.
    pub free: u32,
    /// The table's `unknown_count` counter.
    pub unknown_count: u32,
    /// Every `Free` slot is in the free list and every listed slot is `Free`.
    pub free_list_consistent: bool,
}

/// A tombstone (hft-oms "墓碑与迟到成交").
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub struct TombRow {
    pub venue: VenueId,
    pub account: AccountId,
    pub slot: u32,
    pub r#gen: u32,
    pub epoch: u16,
    pub inst: InstId,
    pub group: GroupId,
    pub filled: i128,
}

/// Link state (hft-oms `LegLink`).
#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug)]
pub enum LinkState {
    Open,
    Hedging,
    Done,
    Broken,
    Expired,
}

/// Hedge plan kind of a link.
#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug)]
pub enum PlanKind {
    OnFill,
    Simultaneous,
    Other,
}

/// Link instance, including the epoch and event that created its first order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct LinkKey {
    pub id: u32,
    pub venue: u8,
    pub account: u8,
    pub epoch: u16,
    pub slot: u32,
    pub generation: u32,
    pub created_seq: u64,
}

/// One OMS link with its hedge accounting (INV-8, INV-14).
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub struct LinkRow {
    pub id: u32,
    /// Complete first-order identity; mandatory for the real engine adapter.
    pub key: Option<LinkKey>,
    pub account: AccountId,
    pub group: GroupId,
    pub plan: PlanKind,
    pub state: LinkState,
    /// Hedge leg index within the group.
    pub hedge_leg: u8,
    pub hedge_inst: InstId,
    pub ratio_num: i64,
    pub ratio_den: i64,
    /// Net first-leg fill counted for hedging (overfill excluded).
    pub first_net: i128,
    /// Σ hedge quantity requested.
    pub hedge_req: i128,
    pub hedge_filled: i128,
    pub reversed: bool,
    pub retries: u32,
    /// A slot of this link is `Unknown`.
    pub has_unknown: bool,
}

/// Completed OnFill links aggregated by hedge leg and ratio, outside the active link table.
#[derive(Clone, Eq, PartialEq, Hash, Debug)]
pub struct HedgeHistoryRow {
    pub account: AccountId,
    pub group: GroupId,
    pub inst: InstId,
    pub ratio_num: i64,
    pub ratio_den: i64,
    pub first_net: i128,
    pub hedge_req: i128,
    pub capped: i128,
    pub links: u64,
}

/// LegTracker row of one (account, group).
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub struct LegRow {
    pub account: AccountId,
    pub group: GroupId,
    /// `LegState.unknown_slots`.
    pub unknown_slots: u32,
    /// `LegView.naked_qty` (≥ 0).
    pub naked_qty: i128,
    /// Naked quantity the hedge would buy / sell (`naked_dir`), ≥ 0 each.
    pub naked_buy: i128,
    pub naked_sell: i128,
    /// max over the group's legs of `max_pos_qty`.
    pub max_pos_qty: i128,
    /// Residual carried for the hedge leg (`residual[g][h]`).
    pub residual: i128,
    /// Σ fee true-ups recorded in the residual (`net_adjust`).
    pub fee_adjust: i128,
    /// Σ quantities cut by the naked cap (`OmsHedgeNakedCapped` v[1] − v[2]).
    pub naked_capped: i128,
    /// Immediate (IOC/FOK) in-flight quantity of each leg (index = leg).
    pub inflight_immediate: [i128; 4],
}

/// Per (account, group, leg) net attributed by the LegTracker (INV-1 group attribution).
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub struct LegNetRow {
    pub account: AccountId,
    pub group: GroupId,
    pub leg: u8,
    pub net: i128,
}

/// Ledger position of (account, venue, inst): signed perp position or spot inventory of the inst's
/// base asset held through that inst.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub struct PosRow {
    pub account: AccountId,
    pub venue: VenueId,
    pub inst: InstId,
    /// Base asset (INV-3 sums per asset).
    pub asset: AssetId,
    pub qty: i128,
}

/// Ledger spot inventory or balance of (account, venue, asset).
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub struct AssetRow {
    pub account: AccountId,
    pub venue: VenueId,
    pub asset: AssetId,
    pub qty: i128,
}

/// INV-3 limit `L(a, X)`.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub struct LimitRow {
    pub account: AccountId,
    pub asset: AssetId,
    pub max_pos_qty: i128,
}

/// In-flight quantity of (account, inst) in one direction (INV-3 third clause, INV-19).
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub struct InflightRow {
    pub account: AccountId,
    pub inst: InstId,
    pub side: Side,
    pub immediate: i128,
    pub resting: i128,
}

/// Cap level order of hft-risk `level_of` (only the order matters to the checkers).
#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug)]
pub enum CapLevel {
    None,
    NoOpen,
    CloseOnly,
    Hard,
}

/// Scope of a cap.
#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug)]
pub enum CapScope {
    Global,
    Account(AccountId),
    AccountVenue(AccountId, VenueId),
    Venue(VenueId),
    Cell(AccountId, StrategyId, GroupId),
}

/// One effective cap (hft-event `CapRecord` projected).
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub struct CapRow {
    pub source: u8,
    pub scope: CapScope,
    pub level: CapLevel,
    /// Venue-level caps (`Kill{Venue}`, `Recon`, `UnknownTimeout`, `AccountCfg`, `Delisting`) do not
    /// take part in the cell computation.
    pub venue_level: bool,
    /// A `Kill` cap (kill bits are recomputed from these).
    pub kill: bool,
    pub hard: bool,
    pub set_seq: u64,
}

/// Mode of one cell as hft-risk `mode` reports it (INV-11).
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub struct CellModeRow {
    pub account: AccountId,
    pub strategy: StrategyId,
    pub group: GroupId,
    pub desired: RunMode,
    /// The cached effective mode (`mode.effective(c)`).
    pub effective: RunMode,
    /// `CloseOnly{deferred_shadow = true}`.
    pub deferred_shadow: bool,
    pub flat: bool,
}

/// Kill word of one gw (venue) as `mode.kill_bits()` reports it, decoded (bit layout owned by hft-risk and
/// hft-venue; the adapter decodes it so the checkers stay independent of the layout).
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub struct KillRow {
    pub venue: VenueId,
    pub global: bool,
    pub venue_bit: bool,
    pub hard: bool,
    /// Bit `a` = `account(ix)` of account id `a`.
    pub accounts: u64,
    /// Bit `a` = `account_hard(ix)` of account id `a`.
    pub accounts_hard: u64,
}

impl KillRow {
    /// No bit set.
    pub const fn clear(venue: VenueId) -> KillRow {
        KillRow {
            venue,
            global: false,
            venue_bit: false,
            hard: false,
            accounts: 0,
            accounts_hard: 0,
        }
    }
}

/// Auto-fix phase of an execution-layer cell (hft-exec "Broken 处置").
#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug)]
pub enum AutoFixPhase {
    Idle,
    Primed,
    WaitHard,
    WaitHold,
    Running,
    Cancelled,
    Done,
    Failed,
}

/// Execution-layer cell (INV-17, INV-18, INV-20).
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub struct ExecCellRow {
    pub account: AccountId,
    pub group: GroupId,
    pub broken: bool,
    pub serial_fallback: bool,
    pub manual_hold: bool,
    pub phase: AutoFixPhase,
    /// Auto-fix due time (engine ns) as the cell stores it.
    pub due_ens: i64,
    /// The same due time recomputed from the timing rules by the adapter's reference.
    pub due_ens_recomputed: i64,
    pub budget: i128,
    pub autofix_filled: i128,
    pub fail_streak: u32,
    pub fix_count: u32,
    /// An unreached `MarginStress`/`Hard` engine target exists.
    pub engine_target_pending: bool,
}

/// Per-account margin stress level (INV-17, INV-20).
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub struct MarginRow {
    pub account: AccountId,
    /// 3 = L3 (`MarginStress` forced deleverage).
    pub level: u8,
    pub engine_flatten: bool,
    pub hard: bool,
}

/// Risk, execution and OMS parameters the checkers compare against (INV-7, INV-15, INV-17).
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, Default)]
pub struct RiskRow {
    pub max_hedge_notional: i128,
    pub max_reduce_notional: i128,
    /// `force_step` in basis points.
    pub force_step_bp: i64,
    pub broken_auto_fix_ms: i64,
    pub reuse_quarantine_ms: i64,
    pub sweep_batch: u32,
}

/// Lot of an inst (INV-15, INV-17).
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub struct InstRow {
    pub inst: InstId,
    pub venue: VenueId,
    pub asset: AssetId,
    pub perp: bool,
    pub lot: i128,
    pub min_qty: i128,
}

/// Drain status (INV-18).
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, Default)]
pub struct DrainRow {
    /// The step ended a Drain with `StepOutcome::Done`.
    pub done: bool,
    /// The Drain wrote `Note{DrainTimeout}`.
    pub timed_out: bool,
    pub exec_drain_ready: bool,
    pub hedge_retry_timers: u32,
    /// Links of the `purpose = Drain` snapshot that are `Open`/`Hedging`.
    pub snapshot_open_links: u32,
}

/// Everything a checker reads from the engine at one instant.
#[derive(Clone, Eq, PartialEq, Hash, Debug, Default)]
pub struct ObservationRequirements {
    pub accounts: Vec<AccountId>,
    pub cells: Vec<(AccountId, StrategyId, GroupId)>,
    pub instruments: Vec<(AccountId, InstId)>,
    pub hedge_legs: Vec<(AccountId, GroupId, InstId)>,
    pub venues: Vec<VenueId>,
    pub venue_accounts: Vec<(VenueId, AccountId)>,
}

/// Everything a checker reads from the engine at one instant.
#[derive(Clone, Eq, PartialEq, Hash, Debug, Default)]
pub struct Obs {
    pub engine_ns: Option<i64>,
    pub required: Option<ObservationRequirements>,
    pub positions: Vec<PosRow>,
    pub inventory: Vec<AssetRow>,
    pub balances: Vec<AssetRow>,
    pub leg_net: Vec<LegNetRow>,
    pub slots: Vec<SlotRow>,
    pub tables: Vec<SlotTableRow>,
    pub tombs: Vec<TombRow>,
    pub links: Vec<LinkRow>,
    pub hedge_history: Vec<HedgeHistoryRow>,
    /// Residual accounting by hedge instrument, independent of other group-level checks.
    pub hedge_balances: Vec<HedgeBalanceRow>,
    /// A diagnostic loss is a failed observation, never an empty successful check.
    pub observation_error: Option<String>,
    pub legs: Vec<LegRow>,
    /// `OmsStats` dup and stale-gen counters.
    pub oms_dup: u64,
    pub oms_stale_gen: u64,
    pub limits: Vec<LimitRow>,
    pub inflight: Vec<InflightRow>,
    pub caps: Vec<CapRow>,
    pub cells: Vec<CellModeRow>,
    pub kill: Vec<KillRow>,
    pub exec: Vec<ExecCellRow>,
    pub margin: Vec<MarginRow>,
    pub risk: RiskRow,
    pub insts: Vec<InstRow>,
    pub drain: DrainRow,
    /// Ledger external-fill dedup keys (`ExtSeen`).
    pub ext_seen: Vec<(AccountId, u64)>,
}

#[derive(Clone, Eq, PartialEq, Hash, Debug)]
pub struct HedgeBalanceRow {
    pub account: AccountId,
    pub group: GroupId,
    pub inst: InstId,
    pub residual: i128,
    pub fee_adjust: i128,
    pub naked_capped: i128,
}

impl Obs {
    pub fn pos(&self, a: AccountId, v: VenueId, i: InstId) -> i128 {
        self.positions
            .iter()
            .find(|p| p.account == a && p.venue == v && p.inst == i)
            .map_or(0, |p| p.qty)
    }
    pub fn slot(&self, v: VenueId, a: AccountId, slot: u32) -> Option<&SlotRow> {
        self.slots
            .iter()
            .find(|s| s.venue == v && s.account == a && s.slot == slot)
    }
    pub fn leg(&self, a: AccountId, g: GroupId) -> Option<&LegRow> {
        self.legs.iter().find(|l| l.account == a && l.group == g)
    }
    pub fn inst(&self, i: InstId) -> Option<&InstRow> {
        self.insts.iter().find(|r| r.inst == i)
    }
    pub fn exec_cell(&self, a: AccountId, g: GroupId) -> Option<&ExecCellRow> {
        self.exec.iter().find(|c| c.account == a && c.group == g)
    }
    pub fn margin_of(&self, a: AccountId) -> Option<&MarginRow> {
        self.margin.iter().find(|m| m.account == a)
    }
    pub fn inflight_of(&self, a: AccountId, i: InstId, s: Side) -> Option<&InflightRow> {
        self.inflight
            .iter()
            .find(|r| r.account == a && r.inst == i && r.side == s)
    }
    /// Sorts every list by its key (adapters call this once after filling).
    pub fn normalize(&mut self) {
        self.positions.sort_by_key(|p| (p.account.0, p.venue.0, p.inst.0));
        self.inventory
            .sort_by_key(|p| (p.account.0, p.venue.0, p.asset.0));
        self.balances.sort_by_key(|p| (p.account.0, p.venue.0, p.asset.0));
        self.leg_net.sort_by_key(|p| (p.account.0, p.group.0, p.leg));
        self.slots.sort_by_key(|s| (s.venue.0, s.account.0, s.slot));
        self.tables.sort_by_key(|t| (t.venue.0, t.account.0));
        self.tombs
            .sort_by_key(|t| (t.venue.0, t.account.0, t.slot, t.r#gen));
        self.links.sort_by_key(|l| l.id);
        self.hedge_balances
            .sort_by_key(|l| (l.account.0, l.group.0, l.inst.0));
        self.legs.sort_by_key(|l| (l.account.0, l.group.0));
        self.limits.sort_by_key(|l| (l.account.0, l.asset.0));
        self.inflight
            .sort_by_key(|r| (r.account.0, r.inst.0, r.side as u8));
        self.cells.sort_by_key(|c| (c.account.0, c.strategy.0, c.group.0));
        self.kill.sort_by_key(|k| k.venue.0);
        self.exec.sort_by_key(|c| (c.account.0, c.group.0));
        self.margin.sort_by_key(|m| m.account.0);
        self.insts.sort_by_key(|r| r.inst.0);
        self.ext_seen.sort_by_key(|k| (k.0.0, k.1));
    }
}

/// Intent class of an order decision (the OMS slot's class; not on the wire).
#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug)]
pub enum DecClass {
    Open,
    Close,
    Hedge,
    Flatten,
}

/// Who produced the order.
#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug)]
pub enum DecOrigin {
    Strategy,
    Exec,
    Oms,
    Command,
}

/// Execution-layer / OMS hedge order kind (INV-15, INV-19).
#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug)]
pub enum HedgeKind {
    Second,
    SecondRetry,
    Reverse,
    HedgeFix,
    ResidualTrim,
    ExternalDefault,
    CmdHedgeFix,
    AutoFix,
}

/// Debug-view facts about one `OrderDecision` of the step, taken at decision time (before the
/// order's own in-flight is counted).
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub struct DecisionInfo {
    /// Journal seq of the `OrderDecision` record.
    pub seq: u64,
    pub class: DecClass,
    pub origin: DecOrigin,
    pub hedge: Option<HedgeKind>,
    /// `reason ∈ {MarginStress, Hard}` engine target slice (INV-4 exception, INV-17).
    pub margin_stress: bool,
    /// The basis inputs of INV-15, copied raw from the views: `Second` = `d_net × ratio + residual`
    /// (the fill delta after base fee); `SecondRetry` = `owed`; `Reverse` = `first_net −
    /// hedge_filled × den / num`; execution-layer kinds = `|Σ leg[i].net|`.
    pub basis: i128,
    /// LegTracker `naked_dir` in the order's direction (INV-19).
    pub naked_dir: i128,
    /// Signed position of the order's leg (inst) in the account.
    pub pos: i128,
    /// Same-direction in-flight of the leg: immediate + resting for `Open`, immediate otherwise.
    pub same_dir_inflight: i128,
    /// `max_pos_qty` that applies to the order (INV-3).
    pub max_pos_qty: i128,
    /// Notional `⌈qty × px × quote_rate⌉` (raw notional units).
    pub notional: i128,
    /// Link id (0 = none) and whether that link has an `Unknown` slot (INV-4).
    pub link: u32,
    pub link_unknown: bool,
}

/// One Note record of the step, named through the owning crate's code table.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub struct NoteInfo {
    pub seq: u64,
    pub name: &'static str,
    pub account: AccountId,
    pub group: GroupId,
    pub inst: InstId,
    pub v: [u64; 8],
}

/// One unit of evidence that may change a ledger position or inventory (INV-1 ①–⑤).
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub enum EvidenceKind {
    /// OMS `FillOut` (`FillDelta`).
    Fill,
    /// `ExternalFill` (input, defensive path, StaleGen synthesis, AUTO_RECON synthesis).
    External,
    /// `AdjustDelta` from `AdoptExchange`.
    Adjust,
    /// `NetAdjust` of a base-asset fee.
    FeeNet,
}

/// One evidence item of a step.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub struct Evidence {
    pub kind: EvidenceKind,
    pub account: AccountId,
    pub venue: VenueId,
    pub inst: InstId,
    /// Signed position delta (raw).
    pub delta: i128,
    /// External-fill dedup key (契约 22), 0 = none.
    pub ext_key: u64,
}

/// What the engine did inside one step besides its journal records.
#[derive(Clone, Eq, PartialEq, Hash, Debug, Default)]
pub struct StepInfo {
    pub decisions: Vec<DecisionInfo>,
    /// Failed OMS hedge places, including attempts which emitted no OrderDecision.
    pub place_failures: Vec<HedgePlaceFailure>,
    /// Raw producer count; even a failure that leaves retries unchanged must be observed.
    pub required_place_failures: u32,
    /// All legs' raw immediate in-flight, before the decision's own order is admitted.
    pub decision_inflight: Vec<(u64, [i128; 4])>,
    /// Account risk caps captured at decision time (hedge, reduce notionals).
    pub decision_limits: Vec<(u64, i128, i128)>,
    /// A perp still needs reducing toward its engine target at the time of this decision.
    pub decision_perp_pending: Vec<(u64, bool)>,
    pub notes: Vec<NoteInfo>,
    /// `ExecDecision{AutoFix, detail = AutoFix/Start}` of this step.
    pub autofix_starts: Vec<(AccountId, GroupId)>,
    /// Cells actually advanced with phase Primed and no documented start gate.
    pub autofix_ready: Vec<(AccountId, GroupId)>,
    pub evidence: Vec<Evidence>,
    /// `WriteOff` amounts per (account, group, leg) (INV-1 group attribution).
    pub writeoffs: Vec<LegNetRow>,
    /// Engine targets set by `ForceDeleverage` this step: (account, group, leg inst, base, target).
    pub force_targets: Vec<ForceTarget>,
    /// Original force fraction (Rate raw), keyed like `force_targets` and in the same order.
    pub force_fractions: Vec<i64>,
    /// Applicable force-target keys declared from the raw operation facts, before projection.
    pub required_force: Vec<(AccountId, GroupId, InstId)>,
}

#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub enum PlaceFailureKind {
    NoTable,
    NoFreeSlot,
    LinkTableFull,
}

/// Raw pre-place facts, tied to one Input and one link instance.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub struct HedgePlaceFailure {
    pub input_seq: u64,
    pub index: u32,
    pub key: LinkKey,
    pub group: GroupId,
    pub venue: VenueId,
    pub inst: InstId,
    pub free_slots: Option<u32>,
    pub error: PlaceFailureKind,
    pub retries_before: u32,
}

/// One leg target of a `ForceDeleverage` step (INV-17).
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub struct ForceTarget {
    pub account: AccountId,
    pub group: GroupId,
    pub inst: InstId,
    pub base: i128,
    pub target: i128,
    pub strategy_target: Option<i128>,
}

/// Exchange truth of one (account, venue) from `MockVenue`/`SimActor` (INV-16).
#[derive(Clone, Eq, PartialEq, Hash, Debug)]
pub struct TruthRow {
    pub account: AccountId,
    pub venue: VenueId,
    pub positions: Vec<(InstId, i128)>,
    pub inventory: Vec<(AssetId, i128)>,
    pub balances: Vec<(AssetId, i128)>,
    /// Undelivered reports, `FeeOnly` or `ExternalFill` exist for the account.
    pub undelivered: bool,
    /// Keys (inst or asset raw id) excluded until synthesized / adopted (INV-16 例外).
    pub excluded: Vec<u32>,
}
