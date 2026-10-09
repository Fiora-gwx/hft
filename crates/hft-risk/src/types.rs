//! Shared value types of the crate: intent views, classification, read-only view traits,
//! capacities, gw map, admission, Note codes and the notional definition.

use hft_event::{CapRecord, Note, NoteLevel, NoteSource};
use hft_strategy_sdk::{BboView, IntentClass, LinkRole};
use hft_types::{
    AccountId, AssetExp, AssetId, GroupId, InstId, Notional, Px, Qty, Rate, Side, StrategyId, Tif, VenueId,
};

/// Producer of an intent.
#[repr(u8)]
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub enum Origin {
    /// An order intent of a strategy `Ctx`.
    Strategy = 1,
    /// hft-exec (target slices, auto HedgeFix, Broken auto close, external-fill default
    /// handling, residual trim, default reduction, HedgeFix command, Kill{Account, flatten}).
    Exec = 2,
    /// hft-oms (LegLink second leg, SecondRetry, Reverse).
    Oms = 3,
    /// A command's own order (SmokeOrder only).
    Command = 4,
}

impl Origin {
    pub const fn from_u8(v: u8) -> Option<Origin> {
        Some(match v {
            1 => Origin::Strategy,
            2 => Origin::Exec,
            3 => Origin::Oms,
            4 => Origin::Command,
            _ => return None,
        })
    }
}

/// Classification result (the only definition; OMS, gw, exec and chain intents read it).
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub struct Classification {
    pub class: IntentClass,
    pub risk_reducing: bool,
    /// Exchange-side `reduceOnly`.
    pub reduce_only: bool,
}

/// Input of `Risk::classify` (projected by the sequencer from the intent being placed).
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub struct IntentDraft {
    pub account: AccountId,
    pub inst: InstId,
    pub side: Side,
    pub qty: Qty,
    pub tif: Tif,
    /// Declared class (strategy) or class hint (exec); the table below decides.
    pub class: IntentClass,
    /// `None` = no link.
    pub link_role: Option<LinkRole>,
    /// The second leg of a `Simultaneous` link (sent in the same event as the first leg).
    pub simultaneous: bool,
    /// `HedgePlan.reduce_only` of the link (1 = closing link).
    pub hedge_reduce_only: bool,
    /// `OrderFlags` bits (hft-event).
    pub flags: u16,
}

/// Position facts `classify` reads for the draft's (account, inst).
pub trait PosView {
    /// Perp: position; spot: inventory of the group.
    fn pos(&self) -> Qty;
    /// Resting same-direction reduce quantity (immediate + resting).
    fn pending_reduce(&self, side: Side) -> Qty;
}

/// Plain `PosView` value.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub struct PosFacts {
    pub pos: Qty,
    pub pending_buy: Qty,
    pub pending_sell: Qty,
}

impl Default for PosFacts {
    fn default() -> Self {
        PosFacts {
            pos: Qty(0),
            pending_buy: Qty(0),
            pending_sell: Qty(0),
        }
    }
}

impl PosView for PosFacts {
    fn pos(&self) -> Qty {
        self.pos
    }
    fn pending_reduce(&self, side: Side) -> Qty {
        match side {
            Side::Buy => self.pending_buy,
            Side::Sell => self.pending_sell,
        }
    }
}

/// Risk view of one intent (the sequencer projects it from the intent and its `Classification`).
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub struct IntentView {
    pub venue: VenueId,
    pub account: AccountId,
    pub side: Side,
    pub class: IntentClass,
    pub inst: InstId,
    pub group: GroupId,
    pub strategy: StrategyId,
    pub origin: Origin,
    /// `None` = no link role.
    pub link_role: Option<LinkRole>,
    pub tif: Tif,
    /// `OrderFlags` (REDUCE_ONLY, POST_ONLY, LINKED, HEDGE_FIX, RISK_REDUCING, SMOKE).
    pub flags: u16,
    /// Optional chain `SwapIntent` (#8 uses the chain venue's bucket).
    pub is_swap: bool,
    pub px: Px,
    pub qty: Qty,
    /// `Risk::notional(qty, px, …)` (definition of check #6).
    pub notional: Notional,
    /// Link leg (`VenueId(0)` = none).
    pub link_venue: VenueId,
    pub link_account: AccountId,
    pub link_side: Side,
    pub link_inst: InstId,
    pub link_qty: Qty,
    /// Upper bound of the second leg's notional.
    pub link_notional: Notional,
    /// Hedge-class quantity cap (filled by the producer).
    pub hedge_cap_qty: Qty,
}

/// `IntentView.link_venue` of an intent without a link leg.
pub const VENUE_NONE: VenueId = VenueId(0);

/// Notional aggregation scope of `RiskView::{held_notional, open_notional}`.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub enum Scope {
    Group(GroupId),
    Account,
    Venue(VenueId),
}

/// hft-ledger total equity of an account.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub struct EquityView {
    pub total: Notional,
    /// Balances of every venue initialised and conversion rates fresh.
    pub valid: bool,
}

/// Equity source of `on_day_roll`.
pub trait EquitySource {
    fn equity(&self, a: AccountId) -> EquityView;
}

/// Read-only facts of OMS, ledger and exec seen by `evaluate` (the sequencer's adapter;
/// generic, statically dispatched).
pub trait RiskView {
    fn pos(&self, a: AccountId, i: InstId) -> Qty;
    fn held_notional(&self, a: AccountId, s: Scope) -> Notional;
    /// Directional in-flight: immediate + resting.
    fn open_qty(&self, a: AccountId, i: InstId, side: Side) -> Qty;
    /// Directional in-flight: immediate only.
    fn open_qty_immediate(&self, a: AccountId, i: InstId, side: Side) -> Qty;
    fn open_notional(&self, a: AccountId, s: Scope) -> Notional;
    /// Same-direction reduce quantity already resting: immediate + resting.
    fn pending_reduce(&self, a: AccountId, i: InstId, side: Side) -> Qty;
    /// Same-direction reduce quantity already resting: immediate only.
    fn pending_reduce_immediate(&self, a: AccountId, i: InstId, side: Side) -> Qty;
    fn venue_ready(&self, v: VenueId, a: AccountId) -> bool;
    fn group_has_unknown(&self, a: AccountId, g: GroupId) -> bool;
    fn equity(&self, a: AccountId) -> EquityView;
    fn bbo(&self, i: InstId) -> BboView;
    /// Spot buy leg: free quote asset minus in-flight buy notional; uninitialised → `None`.
    fn avail_quote(&self, a: AccountId, v: VenueId, quote: AssetId) -> Option<Notional>;
    /// Perp leg: available margin minus in-flight opening margin; no/old report → `None`.
    fn avail_margin(&self, a: AccountId, v: VenueId) -> Option<Notional>;
}

/// One row per account, projected by the sequencer from hft-exec `ExecConfig`.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub struct ExecBounds {
    /// `ExecConfig.force_slice.max_cross`.
    pub force_max_cross: Rate,
    /// `ExecConfig.force_slice.max_chunk_notional`.
    pub force_max_chunk: Notional,
    /// `ExecConfig.dust_notional` (the dust line; single definition in hft-exec).
    pub dust_notional: Notional,
}

impl Default for ExecBounds {
    fn default() -> Self {
        ExecBounds {
            force_max_cross: Rate(0),
            force_max_chunk: Notional(0),
            dust_notional: Notional(0),
        }
    }
}

/// The two ECF1 section 5 switches this crate reads.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub struct ModeCfg {
    pub auto_resume_after_clean_drain: bool,
    pub allow_missing_standby: bool,
}

/// Leg summary of one group, projected by the sequencer from the OMS `LegView`.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub struct LegSummary {
    /// Including in-flight (#9).
    pub naked_ratio: Rate,
    /// Including in-flight (#9).
    pub naked_notional: Notional,
    /// Filled only: `|Σ leg.net| × mark`.
    pub naked_filled_notional: Notional,
    /// No Pending*/Unknown slot and no immediate in-flight on any leg.
    pub quiescent: bool,
}

impl Default for LegSummary {
    fn default() -> Self {
        LegSummary {
            naked_ratio: Rate(0),
            naked_notional: Notional(0),
            naked_filled_notional: Notional(0),
            quiescent: false,
        }
    }
}

/// The capacities this crate allocates by (the sequencer projects ECF1 section 3 `Capacities`).
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub struct Capacities {
    /// Insts (`RiskParams.risk_class` rows).
    pub max_insts: u32,
    /// Cells of the whole process (`RiskParams.groups` rows, ModeTable rows).
    pub max_cells_total: u32,
    /// Active cap records (`CapSet`).
    pub max_caps: u16,
}

impl Capacities {
    /// Default cap capacity.
    pub const MAX_CAPS: u16 = 256;
}

/// gw index → venue it serves (`kill_bits()[i]` belongs to `venues[i]`); the account index of
/// each gw is refdata `venue_account_ix`.
#[derive(Clone, Eq, PartialEq, Hash, Debug, Default)]
pub struct GwMap {
    pub venues: Vec<VenueId>,
}

/// `SetRisk` scope.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub enum AccountScope {
    /// `account = *`: every field, including the global ones.
    All,
    /// Only this account's rows.
    Account(AccountId),
}

/// Rejected `RiskParams`.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub enum RiskParamError {
    /// A field violates its validation rule (the field name).
    Invalid(&'static str),
    /// `ExecConfig.force_slice` exceeds the risk limits.
    ExecRiskMismatch,
    /// More rows than allocated at start-up.
    Capacity,
}

/// Token deduction of an admitted intent (`commit` applies it).
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub struct Admission {
    pub venue: VenueId,
    pub account: AccountId,
    pub inst: InstId,
    pub risk_reducing: bool,
}

/// hft-risk Note codes (`n` of `source = Risk`; names registered in hft-event NOTE_NAMES).
pub const NOTE_CODES: &[(NoteSource, u32, &str)] = &[
    (NoteSource::Risk, 1, "RateLimitHit"),
    (NoteSource::Risk, 2, "CapSet"),
    (NoteSource::Risk, 3, "CapCleared"),
    (NoteSource::Risk, 4, "MarginLevel"),
    (NoteSource::Risk, 5, "RecoveryHeldSim"),
    (NoteSource::Risk, 6, "IdentityUnverified"),
];

pub(crate) const N_RATE_LIMIT_HIT: u32 = 1;
pub(crate) const N_CAP_SET: u32 = 2;
pub(crate) const N_CAP_CLEARED: u32 = 3;
pub(crate) const N_MARGIN_LEVEL: u32 = 4;
pub(crate) const N_RECOVERY_HELD_SIM: u32 = 5;
pub(crate) const N_IDENTITY_UNVERIFIED: u32 = 6;

/// A `source = Risk` Note with `v`.
pub(crate) fn note(n: u32, level: NoteLevel, v: [u64; 8]) -> Note {
    Note {
        code: hft_event::note_code(NoteSource::Risk, n),
        level,
        source: NoteSource::Risk,
        strategy: StrategyId(0),
        _pad: 0,
        group: GroupId(0),
        inst: InstId(0),
        v,
    }
}

/// `v[0..6]` = the 48 bytes of `r` (little-endian words).
pub(crate) fn cap_note(n: u32, level: NoteLevel, r: &CapRecord) -> Note {
    use zerocopy::IntoBytes;
    let b = r.as_bytes();
    let mut v = [0u64; 8];
    for (k, w) in v.iter_mut().take(6).enumerate() {
        let mut a = [0u8; 8];
        if let Some(s) = b.get(k * 8..k * 8 + 8) {
            a.copy_from_slice(s);
        }
        *w = u64::from_le_bytes(a);
    }
    note(n, level, v)
}

/// Decodes `v[0..6]` of a `CapSet`/`CapCleared` Note back into its `CapRecord`.
pub fn cap_of_note(n: &Note) -> CapRecord {
    use zerocopy::{FromBytes, FromZeros, IntoBytes};
    let mut b = [0u8; 48];
    for (k, w) in n.v.iter().take(6).enumerate() {
        if let Some(d) = b.get_mut(k * 8..k * 8 + 8) {
            d.copy_from_slice(&w.to_le_bytes());
        }
    }
    CapRecord::read_from_bytes(b.as_bytes()).unwrap_or_else(|_| CapRecord::new_zeroed())
}

const RATE_ONE: i128 = 1_000_000_000;

/// The notional definition (check #6): `qty × px` in the quote asset (asset exponents `e`),
/// `× quote_rate` into USDT, i128 intermediate, rounded up to 1e-6; saturates at `i64::MAX`.
pub fn notional(qty: Qty, px: Px, e: AssetExp, quote_rate: Rate) -> Notional {
    let prod = (qty.0 as i128)
        .wrapping_abs()
        .saturating_mul((px.0 as i128).wrapping_abs())
        .saturating_mul((quote_rate.0 as i128).wrapping_abs());
    // value = prod × 10^(px_exp + qty_exp) × 1e-9; target exponent −6.
    let s = i32::from(e.px_exp) + i32::from(e.qty_exp) + 6 - 9;
    let v = if s >= 0 {
        pow10(s).map_or(i128::MAX, |p| prod.saturating_mul(p))
    } else {
        match pow10(-s) {
            Some(p) => {
                let q = prod / p;
                if prod % p != 0 { q.saturating_add(1) } else { q }
            }
            None => i128::from(prod != 0),
        }
    };
    Notional(i64::try_from(v).unwrap_or(i64::MAX))
}

pub(crate) fn pow10(k: i32) -> Option<i128> {
    if !(0..=38).contains(&k) {
        return None;
    }
    let mut p: i128 = 1;
    for _ in 0..k {
        p = p.checked_mul(10)?;
    }
    Some(p)
}

/// `n × (1 + r)` rounded up (r = 1e-9 units); saturating.
pub(crate) fn with_buffer_ceil(n: Notional, r: Rate) -> i128 {
    let num = (n.0 as i128).saturating_mul(RATE_ONE.saturating_add(r.0 as i128));
    div_ceil(num, RATE_ONE)
}

/// Ceiling division for `d > 0`.
pub(crate) fn div_ceil(n: i128, d: i128) -> i128 {
    if d <= 0 {
        return i128::MAX;
    }
    let q = n.div_euclid(d);
    if n.rem_euclid(d) != 0 {
        q.saturating_add(1)
    } else {
        q
    }
}

pub(crate) const fn rate_one() -> i128 {
    RATE_ONE
}
