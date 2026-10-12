//! `Ledger`: static tables built by `new` from refdata, the dynamic books, lookups and the
//! read-only accessors. Event entries live in the topic modules (`position`, `cash`, `funding`,
//! `attrib`, `adjust`, `margin`, `rates`, `rounds`, `snapshot`).

use hft_event::Note;
use hft_refdata::{FeeAssetRule, FundingSettleKind, InstKind, RefData};
use hft_types::{
    ASSET_NONE, AccountId, AssetId, GROUP_NONE, GroupId, INST_NONE, InstId, Notional, Px, Qty, StrategyId,
    VenueId,
};

use crate::config::{Capacities, LedgerConfig, Valuation, in_names};
use crate::dedup::ExtRing;
use crate::fixed::FixedVec;
use crate::types::*;
use crate::views::*;

pub(crate) const NO_AV: u16 = u16::MAX;
pub(crate) const MAX_VENUES: usize = 64;
/// Pairing table of `BalanceOnlyPush` pushes and syntheses per (account, venue).
pub(crate) const FUND_PENDING_CAP: usize = 16;
/// No-history funding items held per (account, venue).
pub(crate) const NO_HIST_CAP: usize = 64;
/// Notes buffered between two `clear_notes` calls.
pub const NOTE_BUF: usize = 64;

#[derive(Clone, Debug)]
pub(crate) struct VenueStatic {
    pub present: bool,
    pub perp: bool,
    pub perp_cash: bool,
    pub margin_asset: AssetId,
    pub funding: FundingSettleKind,
    pub fee_rule: FeeAssetRule,
    /// Asset set of the venue (spot: bases and quotes; perp: quotes), ascending.
    pub assets: FixedVec<AssetId, MAX_ASSETS_PER_VENUE>,
    /// Bitmap over `assets` positions: quote/margin assets.
    pub quote_mask: u64,
    pub n_insts: u32,
    /// Perpetual insts of the venue, ascending.
    pub perp_insts: Vec<InstId>,
    /// (account, venue) indexes on this venue.
    pub avs: Vec<u16>,
}

#[derive(Copy, Clone, Debug)]
pub(crate) struct InstStatic {
    pub valid: bool,
    pub venue: VenueId,
    pub perp: bool,
    pub perp_cash: bool,
    pub base: AssetId,
    pub quote: AssetId,
    pub px_exp: i8,
    pub qty_exp: i8,
    pub local: u32,
    /// Asset this inst is the reference inst of (`ASSET_NONE` = none).
    pub ref_of: AssetId,
}

#[derive(Copy, Clone, Debug)]
pub(crate) struct AssetStatic {
    pub px_exp: i8,
    pub qty_exp: i8,
    pub dust: Qty,
    pub par: bool,
    pub fallback: bool,
    pub ref_inst: InstId,
}

#[derive(Copy, Clone, Debug)]
pub(crate) struct AvStatic {
    pub account: AccountId,
    pub venue: VenueId,
    pub pos_base: u32,
}

/// One `FundHist` record.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub(crate) struct FundRec {
    pub inst: InstId,
    pub group: GroupId,
    pub hflags: u8,
    pub settle_ts: i64,
    pub booked: Notional,
    pub corrected: Notional,
}

pub(crate) const PUSHED: u8 = 1;

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub(crate) struct FundPend {
    pub settle_ts: i64,
    pub synth_sum: Notional,
    pub pushed: Notional,
    /// 1 SynthOnly, 2 PushOnly.
    pub state: u8,
}

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub(crate) struct NoHist {
    pub inst: InstId,
    pub settle_ts: i64,
    pub exch: Notional,
    pub engine: Notional,
    pub report_id: u64,
}

/// Dynamic per-(account, venue) state besides the public `VenueBook`.
#[derive(Clone, Debug)]
pub(crate) struct AvDyn {
    pub ext: ExtRing,
    /// Sorted by (settle_ts, inst); capacity `funding_hist_cap`.
    pub hist: Vec<FundRec>,
    /// `None` = no `FundMark` (watermark taken at the next `reconcile_funding`).
    pub hist_from: Option<i64>,
    /// FIFO of `(settle_ts, amount)`, capacity `funding_push_seen_cap`.
    pub seen: Vec<(i64, Notional)>,
    pub pending: FixedVec<FundPend, { FUND_PENDING_CAP + 1 }>,
    pub no_hist: FixedVec<NoHist, { NO_HIST_CAP + 1 }>,
    /// Position indexes of the open perpetual positions of this (account, venue).
    pub open_perps: Vec<u32>,
}

#[derive(Copy, Clone, Debug)]
pub(crate) struct PosDyn {
    pub upnl: Notional,
    pub auto_buy: Qty,
    pub auto_sell: Qty,
    /// `|qty| × valuation price` (USDT), contribution to `AccountBook.held_notional`.
    pub notional: Notional,
}

#[derive(Copy, Clone, Debug)]
pub(crate) struct Leg {
    pub inst: InstId,
    pub net: Qty,
    pub cost: i128,
}

pub(crate) const GF_ADOPTED: u8 = 4;
/// A round is open (row flag of the Group snapshot row).
pub(crate) const GF_ROUND_OPEN: u8 = 8;

#[derive(Clone, Debug)]
pub(crate) struct Cell {
    pub account: AccountId,
    pub group: GroupId,
    pub active: bool,
    pub shadow: bool,
    pub n_legs: u8,
    pub legs: [Leg; 4],
    pub leg_upnl: [Notional; 4],
    pub book: GroupBook,
    pub gflags: u8,
    pub last_fill_ens: i64,
}

#[derive(Copy, Clone, Debug)]
pub(crate) struct RateState {
    pub bid: Px,
    pub ask: Px,
    pub ens: i64,
    pub has: bool,
}

#[derive(Copy, Clone, Debug)]
pub(crate) struct InstPx {
    pub bid: Px,
    pub ask: Px,
    pub has_bbo: bool,
    pub mark: Px,
    pub mark_ens: i64,
    pub has_mark: bool,
    pub interval_s: u32,
    pub anchor_s: u32,
    /// Last fill price (valuation fallback before any quote).
    pub last_px: Px,
}

/// Resolved configuration values used on the hot paths.
#[derive(Clone, Debug)]
pub(crate) struct Cfg {
    pub valuation: Valuation,
    pub rate_max_age_ns: i64,
    pub round_dust: Notional,
    pub tol_abs: Notional,
    pub tol_rel: i64,
    pub lag_ns: i64,
    pub window_ns: i64,
    pub hist_cap: usize,
    pub seen_cap: usize,
    pub ext_cap: usize,
}

/// The ledger (README "使用本组件").
#[derive(Clone, Debug)]
pub struct Ledger {
    pub(crate) cfg: Cfg,
    pub(crate) n_acc: usize,
    pub(crate) acc_sim: Vec<bool>,
    pub(crate) acc_instance: Vec<StrategyId>,
    pub(crate) av_of: Vec<u16>,
    pub(crate) avs: Vec<AvStatic>,
    pub(crate) acc_avs: Vec<Vec<u16>>,
    pub(crate) venues: Vec<VenueStatic>,
    /// Insts of each venue, ascending (`insts_of_venue`); position local index → inst.
    pub(crate) venue_insts: Vec<Vec<InstId>>,
    pub(crate) insts: Vec<InstStatic>,
    pub(crate) assets: Vec<AssetStatic>,
    pub(crate) w: usize,
    pub(crate) max_cells: usize,

    pub(crate) books: Vec<VenueBook>,
    pub(crate) avd: Vec<AvDyn>,
    pub(crate) pos: Vec<InstPos>,
    pub(crate) posd: Vec<PosDyn>,
    pub(crate) accounts: Vec<AccountBook>,
    pub(crate) cells: Vec<Cell>,
    /// Sorted `(account, group, cell)`.
    pub(crate) cell_ix: Vec<(u8, u32, u32)>,
    /// `[pos_ix × w + k]` = `(cell + 1) << 2 | leg`; 0 = empty.
    pub(crate) cand: Vec<u32>,
    pub(crate) rates: Vec<RateState>,
    pub(crate) ipx: Vec<InstPx>,
    pub(crate) now: i64,
    pub(crate) run_start: i64,
    pub(crate) next_stale: i64,
    pub(crate) notes: Vec<Note>,
    pub(crate) notes_dropped: u32,
    pub(crate) rounding: u64,
}

pub(crate) static ZERO_POS: InstPos = InstPos::ZERO;
pub(crate) static ZERO_GROUP: GroupBook = GroupBook::ZERO;
pub(crate) static ZERO_ACCOUNT: AccountBook = AccountBook::ZERO;

impl Ledger {
    /// Builds every table once from refdata, the ledger section and the capacities.
    pub fn new(rd: &RefData, cfg: &LedgerConfig, caps: &Capacities) -> Result<Ledger, LedgerConfigError> {
        // Assets.
        let mut assets = vec![
            AssetStatic {
                px_exp: 0,
                qty_exp: 0,
                dust: Qty(0),
                par: false,
                fallback: false,
                ref_inst: INST_NONE,
            };
            1
        ];
        let mut a = 1u16;
        while rd.asset(AssetId(a)).id == AssetId(a) {
            let m = rd.asset(AssetId(a));
            assets.push(AssetStatic {
                px_exp: m.exp.px_exp,
                qty_exp: m.exp.qty_exp,
                dust: m.dust,
                par: in_names(rd, AssetId(a), &cfg.par_assets),
                fallback: in_names(rd, AssetId(a), &cfg.stable_par_fallback),
                ref_inst: INST_NONE,
            });
            a = a.wrapping_add(1);
            if a == 0 {
                break;
            }
        }
        // Insts.
        let n_insts = rd.inst_count();
        let mut insts = Vec::with_capacity(n_insts + 1);
        for i in 0..=n_insts {
            let id = InstId(i as u32);
            let h = rd.inst(id);
            let valid = i != 0 && h.status != hft_refdata::InstStatus::Invalid;
            let local = rd
                .insts_of_venue(h.venue)
                .binary_search(&id)
                .map(|x| x as u32)
                .unwrap_or(0);
            insts.push(InstStatic {
                valid,
                venue: h.venue,
                perp: h.kind == InstKind::Perp,
                perp_cash: rd.caps(h.venue).flags.contains(hft_refdata::CapFlags::PERP_CASH),
                base: h.asset,
                quote: h.quote,
                px_exp: h.px_exp,
                qty_exp: h.qty_exp,
                local,
                ref_of: ASSET_NONE,
            });
        }
        for (ix, s) in assets.iter_mut().enumerate().skip(1) {
            let r = cfg.ref_inst_for(rd, AssetId(ix as u16));
            s.ref_inst = r;
            if let Some(x) = insts.get_mut(r.0 as usize)
                && r != INST_NONE
            {
                x.ref_of = AssetId(ix as u16);
            }
        }
        // Venues.
        let mut venues: Vec<VenueStatic> = (0..MAX_VENUES)
            .map(|_| VenueStatic {
                present: false,
                perp: false,
                perp_cash: false,
                margin_asset: ASSET_NONE,
                funding: FundingSettleKind::NotApplicable,
                fee_rule: FeeAssetRule::Quote,
                assets: FixedVec::new(),
                quote_mask: 0,
                n_insts: 0,
                perp_insts: Vec::new(),
                avs: Vec::new(),
            })
            .collect();
        for &v in rd.venues_in_order() {
            let Some(vs) = venues.get_mut(usize::from(v.0)) else {
                continue;
            };
            let caps_row = rd.caps(v);
            vs.present = true;
            vs.perp = rd.venue(v).product == InstKind::Perp;
            vs.perp_cash = caps_row.flags.contains(hft_refdata::CapFlags::PERP_CASH);
            vs.funding = caps_row.funding_settle;
            vs.fee_rule = caps_row.fee_asset_rule;
            let list = rd.insts_of_venue(v);
            vs.n_insts = list.len() as u32;
            // Collect the asset set (ascending ids, quote flags).
            let mut set: [u8; 65536 / 8] = [0; 65536 / 8];
            let mut quote: [u8; 65536 / 8] = [0; 65536 / 8];
            let mark = |b: &mut [u8; 8192], x: AssetId| {
                if let Some(c) = b.get_mut(usize::from(x.0 / 8)) {
                    *c |= 1 << (x.0 % 8);
                }
            };
            for &i in list {
                let h = rd.inst(i);
                mark(&mut set, h.quote);
                mark(&mut quote, h.quote);
                if h.kind == InstKind::Perp {
                    vs.perp_insts.push(i);
                    if vs.margin_asset == ASSET_NONE {
                        vs.margin_asset = h.quote;
                    }
                } else {
                    mark(&mut set, h.asset);
                }
            }
            let mut n = 0u16;
            for x in 1..=u16::MAX {
                let bit = |b: &[u8; 8192]| b.get(usize::from(x / 8)).is_some_and(|c| c & (1 << (x % 8)) != 0);
                if bit(&set) {
                    n = n.saturating_add(1);
                    if !vs.assets.push(AssetId(x)) {
                        continue;
                    }
                    if bit(&quote) {
                        vs.quote_mask |= 1u64 << (vs.assets.len() - 1);
                    }
                }
            }
            if usize::from(n) > MAX_ASSETS_PER_VENUE {
                return Err(LedgerConfigError::TooManyAssets { venue: v, n });
            }
        }
        // Accounts and (account, venue) pairs.
        let mut n_acc = 0usize;
        while n_acc < 254 && rd.account(AccountId(n_acc as u8 + 1)).id == AccountId(n_acc as u8 + 1) {
            n_acc += 1;
        }
        let mut av_of = vec![NO_AV; (n_acc + 1) * MAX_VENUES];
        let mut avs = Vec::new();
        let mut acc_avs = vec![Vec::new(); n_acc + 1];
        let mut acc_sim = vec![false; n_acc + 1];
        let mut acc_instance = vec![StrategyId(0); n_acc + 1];
        let mut pos_total = 0u32;
        for acc in 1..=n_acc {
            let m = rd.account(AccountId(acc as u8));
            acc_sim[acc] = matches!(m.kind, hft_refdata::AccountKind::Sim(_));
            acc_instance[acc] = m.bound_instance;
            for v in 0..MAX_VENUES {
                if m.venues & (1u64 << v) == 0 {
                    continue;
                }
                let Some(vs) = venues.get_mut(v) else { continue };
                if !vs.present {
                    continue;
                }
                let ix = avs.len() as u16;
                avs.push(AvStatic {
                    account: AccountId(acc as u8),
                    venue: VenueId(v as u8),
                    pos_base: pos_total,
                });
                pos_total = pos_total.saturating_add(vs.n_insts);
                if let Some(s) = av_of.get_mut(acc * MAX_VENUES + v) {
                    *s = ix;
                }
                acc_avs[acc].push(ix);
                vs.avs.push(ix);
            }
        }
        let c = Cfg {
            valuation: cfg.valuation,
            rate_max_age_ns: i64::from(cfg.rate_max_age_ms).saturating_mul(1_000_000),
            round_dust: cfg.round_dust_notional,
            tol_abs: cfg.funding_tol_abs,
            tol_rel: cfg.funding_tol_rel.0,
            lag_ns: i64::from(cfg.funding_recon_lag_ms).saturating_mul(1_000_000),
            window_ns: (cfg.funding_recon_window_ms as i64).saturating_mul(1_000_000),
            hist_cap: cfg.funding_hist_cap.max(1) as usize,
            seen_cap: usize::from(cfg.funding_push_seen_cap.max(1)),
            ext_cap: cfg.ext_dedup_cap.max(1) as usize,
        };
        let mut books = Vec::with_capacity(avs.len());
        let mut avd = Vec::with_capacity(avs.len());
        for s in &avs {
            let vs = &venues[usize::from(s.venue.0)];
            let mut cash = FixedVec::new();
            for &x in vs.assets.iter() {
                cash.push(CashEntry::new(x));
            }
            books.push(VenueBook {
                cash,
                inv: FixedVec::new(),
                margin: MarginState::new(),
                summary: VenueEquityView::empty(s.venue),
                vflags: 0,
            });
            let perp = vs.perp;
            avd.push(AvDyn {
                ext: ExtRing::new(c.ext_cap),
                hist: Vec::with_capacity(if perp { c.hist_cap } else { 0 }),
                hist_from: None,
                seen: Vec::with_capacity(if perp { c.seen_cap } else { 0 }),
                pending: FixedVec::new(),
                no_hist: FixedVec::new(),
                open_perps: Vec::with_capacity(if perp { vs.perp_insts.len() } else { 0 }),
            });
        }
        let w = usize::from(caps.max_subs_per_inst.max(1));
        let max_cells = caps.max_cells_total.max(1) as usize;
        let npos = pos_total as usize;
        let mut l = Ledger {
            cfg: c,
            n_acc,
            acc_sim,
            acc_instance,
            av_of,
            avs,
            acc_avs,
            venues,
            venue_insts: (0..MAX_VENUES)
                .map(|v| rd.insts_of_venue(VenueId(v as u8)).to_vec())
                .collect(),
            insts,
            assets,
            w,
            max_cells,
            books,
            avd,
            pos: vec![InstPos::ZERO; npos],
            posd: vec![
                PosDyn {
                    upnl: Notional(0),
                    auto_buy: Qty(0),
                    auto_sell: Qty(0),
                    notional: Notional(0),
                };
                npos
            ],
            accounts: vec![AccountBook::ZERO; n_acc + 1],
            cells: Vec::with_capacity(max_cells),
            cell_ix: Vec::with_capacity(max_cells),
            cand: vec![0; npos.saturating_mul(w)],
            rates: Vec::new(),
            ipx: Vec::new(),
            now: 0,
            run_start: 0,
            next_stale: i64::MAX,
            notes: Vec::with_capacity(NOTE_BUF),
            notes_dropped: 0,
            rounding: 0,
        };
        l.rates = vec![
            RateState {
                bid: Px(0),
                ask: Px(0),
                ens: 0,
                has: false,
            };
            l.assets.len()
        ];
        l.ipx = vec![
            InstPx {
                bid: Px(0),
                ask: Px(0),
                has_bbo: false,
                mark: Px(0),
                mark_ens: 0,
                has_mark: false,
                interval_s: 0,
                anchor_s: 0,
                last_px: Px(0),
            };
            l.insts.len()
        ];
        for i in 1..l.insts.len() {
            let m = rd.inst_meta(InstId(i as u32));
            if let Some(p) = l.ipx.get_mut(i) {
                p.interval_s = m.funding_interval_s;
                p.anchor_s = m.funding_anchor_s;
            }
        }
        for av in 0..l.avs.len() {
            l.refresh_summary(av);
        }
        Ok(l)
    }

    // ---------------------------------------------------------------- lookups

    pub(crate) fn inst(&self, i: InstId) -> Option<&InstStatic> {
        self.insts.get(i.0 as usize).filter(|x| x.valid)
    }

    pub(crate) fn asset_s(&self, a: AssetId) -> AssetStatic {
        self.assets.get(usize::from(a.0)).copied().unwrap_or(AssetStatic {
            px_exp: 0,
            qty_exp: 0,
            dust: Qty(0),
            par: false,
            fallback: false,
            ref_inst: INST_NONE,
        })
    }

    pub(crate) fn venue_s(&self, v: VenueId) -> Option<&VenueStatic> {
        self.venues.get(usize::from(v.0)).filter(|x| x.present)
    }

    pub(crate) fn av(&self, a: AccountId, v: VenueId) -> Option<usize> {
        let ix = *self.av_of.get(usize::from(a.0) * MAX_VENUES + usize::from(v.0))?;
        (ix != NO_AV && usize::from(v.0) < MAX_VENUES).then_some(usize::from(ix))
    }

    /// Position index of `(a, i)`.
    pub(crate) fn pos_ix(&self, a: AccountId, i: InstId) -> Option<usize> {
        let s = self.inst(i)?;
        let av = self.av(a, s.venue)?;
        Some(self.avs.get(av)?.pos_base as usize + s.local as usize)
    }

    /// Whether `asset` is a quote/margin asset of the venue.
    pub(crate) fn is_quote(&self, v: VenueId, asset: AssetId) -> bool {
        let Some(vs) = self.venue_s(v) else { return false };
        match vs.assets.position(|x| *x == asset) {
            Some(k) => vs.quote_mask & (1u64 << k) != 0,
            None => false,
        }
    }

    pub(crate) fn cell_of(&self, a: AccountId, g: GroupId) -> Option<usize> {
        if g == GROUP_NONE {
            return None;
        }
        let k = (a.0, g.0);
        self.cell_ix
            .binary_search_by(|(x, y, _)| (*x, *y).cmp(&k))
            .ok()
            .and_then(|i| self.cell_ix.get(i))
            .map(|(_, _, c)| *c as usize)
    }

    /// Cell of `(a, g)`, created when missing (capacity permitting).
    pub(crate) fn cell_or_new(&mut self, a: AccountId, g: GroupId) -> Option<usize> {
        if g == GROUP_NONE {
            return None;
        }
        if let Some(c) = self.cell_of(a, g) {
            return Some(c);
        }
        if self.cells.len() >= self.max_cells {
            return None;
        }
        let c = self.cells.len();
        self.cells.push(Cell::new(a, g));
        let k = (a.0, g.0);
        let at = self
            .cell_ix
            .binary_search_by(|(x, y, _)| (*x, *y).cmp(&k))
            .unwrap_or_else(|e| e);
        self.cell_ix.insert(at, (a.0, g.0, c as u32));
        Some(c)
    }

    /// Leg index of `inst` in cell `c` (the given `leg_ix` when it matches or the leg is unset).
    pub(crate) fn leg_of(&mut self, c: usize, inst: InstId, leg_ix: u8) -> Option<usize> {
        let cell = self.cells.get_mut(c)?;
        let l = usize::from(leg_ix);
        if let Some(x) = cell.legs.get(l)
            && (x.inst == inst || (x.inst == INST_NONE && l >= usize::from(cell.n_legs)))
        {
            if x.inst == INST_NONE {
                if let Some(x) = cell.legs.get_mut(l) {
                    x.inst = inst;
                }
                cell.n_legs = cell.n_legs.max(leg_ix + 1);
            }
            return Some(l);
        }
        if let Some(k) = cell.legs.iter().position(|x| x.inst == inst) {
            return Some(k);
        }
        let k = cell.legs.iter().position(|x| x.inst == INST_NONE)?;
        if let Some(x) = cell.legs.get_mut(k) {
            x.inst = inst;
        }
        cell.n_legs = cell.n_legs.max(k as u8 + 1);
        Some(k)
    }

    /// Non-Shadow active candidates of `(a, inst)`: `(count, first)`.
    pub(crate) fn candidates(&self, a: AccountId, inst: InstId) -> (usize, Option<(usize, usize)>) {
        let Some(p) = self.pos_ix(a, inst) else {
            return (0, None);
        };
        let mut n = 0;
        let mut first = None;
        for k in 0..self.w {
            let v = self.cand.get(p * self.w + k).copied().unwrap_or(0);
            if v == 0 {
                break;
            }
            let c = (v >> 2) as usize - 1;
            let l = (v & 3) as usize;
            if let Some(cell) = self.cells.get(c)
                && cell.active
                && !cell.shadow
            {
                n += 1;
                if first.is_none() {
                    first = Some((c, l));
                }
            }
        }
        (n, first)
    }

    pub(crate) fn push_note(&mut self, n: Note) {
        if self.notes.len() < NOTE_BUF {
            self.notes.push(n);
        } else {
            self.notes_dropped = self.notes_dropped.saturating_add(1);
        }
    }

    pub(crate) fn tick(&mut self, t: i64) {
        self.now = t;
        if t > self.next_stale {
            self.refresh_all();
        }
    }

    pub(crate) fn refresh_all(&mut self) {
        self.recompute_next_stale();
        for av in 0..self.avs.len() {
            self.refresh_summary(av);
        }
    }

    // ---------------------------------------------------------------- public: groups

    /// After `SetParams` of instance `s` is applied: rebuilds the `(account, inst) → [(group,
    /// leg_ix)]` candidate table of the account bound to `s` from its `GroupSpec`s.
    pub fn on_groups_changed(&mut self, s: StrategyId, groups: &hft_strategy_sdk::GroupTable) {
        let Some(acc) = (1..=self.n_acc).find(|a| self.acc_instance.get(*a) == Some(&s)) else {
            return;
        };
        let a = AccountId(acc as u8);
        for c in self.cells.iter_mut().filter(|c| c.account == a) {
            c.active = false;
        }
        for (_, g) in groups.iter() {
            let Some(c) = self.cell_or_new(a, g.id) else {
                continue;
            };
            if let Some(cell) = self.cells.get_mut(c) {
                cell.active = true;
                cell.n_legs = g.n_legs.min(4);
                for (k, l) in g.legs.iter().enumerate() {
                    if let Some(x) = cell.legs.get_mut(k) {
                        if k < usize::from(cell.n_legs) {
                            x.inst = l.inst;
                        } else if x.net.0 == 0 {
                            x.inst = INST_NONE;
                        }
                    }
                }
            }
        }
        self.rebuild_candidates(a);
    }

    pub(crate) fn rebuild_candidates(&mut self, a: AccountId) {
        let Some(avs) = self.acc_avs.get(usize::from(a.0)).cloned() else {
            return;
        };
        for av in avs {
            let Some(s) = self.avs.get(usize::from(av)) else {
                continue;
            };
            let n = self.venues.get(usize::from(s.venue.0)).map_or(0, |v| v.n_insts) as usize;
            let base = s.pos_base as usize;
            for x in self.cand.iter_mut().skip(base * self.w).take(n * self.w) {
                *x = 0;
            }
        }
        for c in 0..self.cells.len() {
            let Some(cell) = self.cells.get(c) else { continue };
            if cell.account != a || !cell.active {
                continue;
            }
            let legs = cell.legs;
            let n = usize::from(cell.n_legs);
            for (l, leg) in legs.iter().enumerate().take(n) {
                let Some(p) = self.pos_ix(a, leg.inst) else {
                    continue;
                };
                for k in 0..self.w {
                    if let Some(x) = self.cand.get_mut(p * self.w + k)
                        && *x == 0
                    {
                        *x = ((c as u32 + 1) << 2) | l as u32;
                        break;
                    }
                }
            }
        }
    }

    /// Marks cell `(a, g)` Shadow (external fills are attributed only to non-Shadow cells).
    pub fn set_cell_shadow(&mut self, a: AccountId, g: GroupId, shadow: bool) {
        if let Some(c) = self.cell_of(a, g)
            && let Some(cell) = self.cells.get_mut(c)
        {
            cell.shadow = shadow;
        }
    }

    /// Start of this run (`EngineClock.base_engine_ns`): earlier rates, margin reports and marks
    /// are stale until refreshed.
    pub fn on_run_start(&mut self, run_start_ens: i64) {
        self.run_start = run_start_ens;
        self.now = run_start_ens;
        self.refresh_all();
    }

    /// Resets the day baselines (`day` of every group and account); cumulative books unchanged.
    pub fn on_day_roll(&mut self, t: i64) {
        self.tick(t);
        for a in self.accounts.iter_mut() {
            a.day = PnlBreakdown::zero();
        }
        for c in self.cells.iter_mut() {
            c.book.day = PnlBreakdown::zero();
        }
    }

    // ---------------------------------------------------------------- notes

    /// Notes written since the last `clear_notes` (the sequencer journals them).
    pub fn notes(&self) -> &[Note] {
        &self.notes
    }

    pub fn clear_notes(&mut self) {
        self.notes.clear();
    }

    /// Notes lost because more than `NOTE_BUF` were written between two `clear_notes`.
    pub fn notes_dropped(&self) -> u32 {
        self.notes_dropped
    }

    /// Conversions whose truncation dropped digits (README "数据结构").
    pub fn rounding(&self) -> u64 {
        self.rounding
    }

    // ---------------------------------------------------------------- read-only

    pub fn venue_book(&self, a: AccountId, v: VenueId) -> &VenueBook {
        match self.av(a, v).and_then(|x| self.books.get(x)) {
            Some(b) => b,
            None => empty_book(),
        }
    }

    pub fn pos(&self, a: AccountId, i: InstId) -> &InstPos {
        self.pos_ix(a, i)
            .and_then(|p| self.pos.get(p))
            .unwrap_or(&ZERO_POS)
    }

    /// Spot inventory of `asset` attributed to `g` on `(a, v)`.
    pub fn inventory(&self, a: AccountId, v: VenueId, asset: AssetId, g: GroupId) -> Qty {
        let b = self.venue_book(a, v);
        b.inv
            .iter()
            .find(|e| e.asset == asset && e.group == g)
            .map_or(Qty(0), |e| e.qty)
    }

    pub fn margin(&self, a: AccountId, v: VenueId) -> &MarginState {
        &self.venue_book(a, v).margin
    }

    pub fn group(&self, a: AccountId, g: GroupId) -> &GroupBook {
        self.cell_of(a, g)
            .and_then(|c| self.cells.get(c))
            .map_or(&ZERO_GROUP, |c| &c.book)
    }

    pub fn account(&self, a: AccountId) -> &AccountBook {
        self.accounts.get(usize::from(a.0)).unwrap_or(&ZERO_ACCOUNT)
    }

    /// `(interval_s, anchor_s)` used to normalize funding times; `INST_NONE` = finest grid of
    /// the venue's perpetual insts.
    pub fn funding_grid(&self, v: VenueId, inst: InstId) -> (u32, u32) {
        if inst != INST_NONE {
            return self
                .ipx
                .get(inst.0 as usize)
                .map_or((0, 0), |p| (p.interval_s, p.anchor_s));
        }
        let mut best = (0u32, 0u32);
        if let Some(vs) = self.venue_s(v) {
            for i in &vs.perp_insts {
                if let Some(p) = self.ipx.get(i.0 as usize)
                    && p.interval_s != 0
                    && (best.0 == 0 || p.interval_s < best.0)
                {
                    best = (p.interval_s, p.anchor_s);
                }
            }
        }
        best
    }

    /// Accumulated net quantity of leg `leg_ix` of `(a, g)`.
    pub fn leg_net(&self, a: AccountId, g: GroupId, leg_ix: u8) -> Qty {
        self.cell_of(a, g)
            .and_then(|c| self.cells.get(c))
            .and_then(|c| c.legs.get(usize::from(leg_ix)))
            .map_or(Qty(0), |l| l.net)
    }

    /// Account-level position without a group: `InstPos.qty − Σ` group legs on `inst`.
    pub fn ungrouped_qty(&self, a: AccountId, inst: InstId) -> Qty {
        let q = self.pos(a, inst).qty.0;
        let mut s = 0i64;
        for c in self.cells.iter().filter(|c| c.account == a) {
            for l in c.legs.iter().filter(|l| l.inst == inst) {
                s = s.saturating_add(l.net.0);
            }
        }
        Qty(q.saturating_sub(s))
    }

    /// Pending `auto_held` of `(a, inst, side)`.
    pub fn auto_held(&self, a: AccountId, inst: InstId, side: hft_types::Side) -> Qty {
        self.pos_ix(a, inst)
            .and_then(|p| self.posd.get(p))
            .map_or(Qty(0), |d| match side {
                hft_types::Side::Buy => d.auto_buy,
                hft_types::Side::Sell => d.auto_sell,
            })
    }

    // ---------------------------------------------------------------- views

    pub fn cash_view(&self, a: AccountId, v: VenueId, asset: AssetId) -> CashView {
        let b = self.venue_book(a, v);
        match b.cash.iter().find(|e| e.asset == asset) {
            Some(e) => CashView {
                free: Qty(e.total.0.saturating_sub(e.rep_locked.0).max(0)),
                locked: e.rep_locked,
            },
            None => CashView {
                free: Qty(0),
                locked: Qty(0),
            },
        }
    }

    pub fn inventory_view(&self, a: AccountId, v: VenueId, asset: AssetId) -> InventoryView {
        let b = self.venue_book(a, v);
        let assigned = b
            .inv
            .iter()
            .filter(|e| e.asset == asset)
            .fold(0i64, |s, e| s.saturating_add(e.qty.0));
        match b.cash.iter().find(|e| e.asset == asset) {
            Some(e) => InventoryView {
                total: e.total,
                assigned: Qty(assigned),
                unassigned: e.unassigned,
                baseline: e.baseline,
            },
            None => InventoryView {
                total: Qty(0),
                assigned: Qty(assigned),
                unassigned: Qty(0),
                baseline: Qty(0),
            },
        }
    }

    /// Spot net quantity attributed to `g` on venue `v` (all assets).
    pub fn inventory_of(&self, a: AccountId, v: VenueId, g: GroupId) -> Qty {
        let b = self.venue_book(a, v);
        Qty(b
            .inv
            .iter()
            .filter(|e| e.group == g)
            .fold(0i64, |s, e| s.saturating_add(e.qty.0)))
    }

    pub fn perp_view(&self, a: AccountId, i: InstId) -> PerpPosView {
        let p = self.pos(a, i);
        let upnl = self
            .pos_ix(a, i)
            .and_then(|x| self.posd.get(x))
            .map_or(Notional(0), |d| d.upnl);
        let (mut liq, mut adl) = (Px(0), 0u8);
        if let Some(s) = self.inst(i) {
            let m = &self.venue_book(a, s.venue).margin;
            if let Some(it) = m.items.iter().find(|x| x.inst == i) {
                liq = it.liq_px;
                adl = it.adl_quantile;
            }
        }
        PerpPosView {
            qty: p.qty,
            entry_px: entry_px(p.qty, p.cost),
            upnl,
            liq_px: liq,
            adl_quantile: adl,
        }
    }

    pub fn margin_view(&self, a: AccountId, v: VenueId) -> MarginView {
        let m = self.margin(a, v);
        MarginView {
            margin_balance: m.rep_margin_balance,
            maint_margin: m.rep_maint,
            avail_margin: m.rep_avail,
            margin_ratio: m.rep_ratio,
            src_ens: m.rep_ens,
            stale: !m.has_report || m.rep_ens < self.run_start,
        }
    }

    pub fn group_pos(&self, a: AccountId, g: GroupId) -> GroupPosView {
        let zero_leg = LegPos {
            qty: Qty(0),
            entry_px: Px(0),
            upnl: Notional(0),
        };
        let Some(cell) = self.cell_of(a, g).and_then(|c| self.cells.get(c)) else {
            return GroupPosView {
                legs: [zero_leg; 4],
                n_legs: 0,
                flags: 0,
                held_notional: Notional(0),
                pnl: PnlSummary {
                    price_realized: Notional(0),
                    funding: Notional(0),
                    fees: Notional(0),
                    unrealized: Notional(0),
                },
                round_start_ens: 0,
                last_fill_ens: 0,
            };
        };
        let mut legs = [zero_leg; 4];
        for (k, d) in legs.iter_mut().enumerate() {
            if let (Some(l), Some(u)) = (cell.legs.get(k), cell.leg_upnl.get(k)) {
                *d = LegPos {
                    qty: l.net,
                    entry_px: entry_px(l.net, l.cost),
                    upnl: *u,
                };
            }
        }
        GroupPosView {
            legs,
            n_legs: cell.n_legs,
            flags: if cell.gflags & GF_ADOPTED != 0 {
                GroupPosView::ENTRY_ADOPTED
            } else {
                0
            },
            held_notional: cell.book.held_notional,
            pnl: PnlSummary {
                price_realized: cell.book.pnl.price_realized,
                funding: cell.book.pnl.funding,
                fees: cell.book.pnl.fee_cex,
                unrealized: cell.book.pnl.unrealized,
            },
            round_start_ens: cell.book.round.open_ens,
            last_fill_ens: cell.last_fill_ens,
        }
    }

    /// Venues of account `a` (refdata order).
    pub fn account_venues(&self, a: AccountId) -> impl Iterator<Item = VenueId> + '_ {
        self.acc_avs
            .get(usize::from(a.0))
            .into_iter()
            .flatten()
            .filter_map(|&av| self.avs.get(usize::from(av)).map(|s| s.venue))
    }

    /// Equity of account `a` (Σ venue summaries); `valid` = every venue initialized and no
    /// required rate stale.
    pub fn equity(&self, a: AccountId) -> Equity {
        let mut e = Equity {
            total: Notional(0),
            by_venue: FixedVec::new(),
            valid: true,
        };
        let Some(list) = self.acc_avs.get(usize::from(a.0)) else {
            e.valid = false;
            return e;
        };
        if list.is_empty() {
            e.valid = false;
        }
        for &av in list {
            let Some(b) = self.books.get(usize::from(av)) else {
                continue;
            };
            e.total = Notional(e.total.0.saturating_add(b.summary.equity.0));
            e.by_venue.push((b.summary.venue, b.summary.equity));
            if b.vflags & vflags::INIT == 0 || b.summary.flags & VenueEquityView::RATE_STALE != 0 {
                e.valid = false;
            }
        }
        e
    }

    /// Conversion rate of `asset` and whether it is stale now.
    pub fn rate(&self, asset: AssetId) -> RateView {
        let r = self
            .rates
            .get(usize::from(asset.0))
            .copied()
            .unwrap_or(RateState {
                bid: Px(0),
                ask: Px(0),
                ens: 0,
                has: false,
            });
        RateView {
            bid: r.bid,
            ask: r.ask,
            ens: r.ens,
            stale: self.rate_stale(asset),
        }
    }
}

/// `cost / |qty|` as a price (HalfEven); 0 when flat.
pub(crate) fn entry_px(q: Qty, cost: i128) -> Px {
    if q.0 == 0 {
        return Px(0);
    }
    Px(crate::fixed::sat64(crate::fixed::div_round(
        cost,
        i128::from(q.0).abs(),
        hft_types::RoundMode::HalfEven,
    )))
}

static EMPTY_BOOK: VenueBook = VenueBook {
    cash: FixedVec::new(),
    inv: FixedVec::new(),
    margin: MarginState::new(),
    summary: VenueEquityView::empty(VenueId(0)),
    vflags: 0,
};

fn empty_book() -> &'static VenueBook {
    &EMPTY_BOOK
}

impl Cell {
    pub(crate) fn new(a: AccountId, g: GroupId) -> Cell {
        Cell {
            account: a,
            group: g,
            active: false,
            shadow: false,
            n_legs: 0,
            legs: [Leg {
                inst: INST_NONE,
                net: Qty(0),
                cost: 0,
            }; 4],
            leg_upnl: [Notional(0); 4],
            book: GroupBook::ZERO,
            gflags: 0,
            last_fill_ens: 0,
        }
    }
}
