//! `InflightBook`: in-flight sums maintained O(1) at every slot change (README "InflightBook").

use hft_types::{AccountId, GroupId, InstId, Notional, Qty, VenueId};

use crate::table::VenueBook;
use crate::types::InflightPos;

/// In-flight book read by hft-risk (position, notional and affordability checks).
#[derive(Clone, Debug)]
pub struct InflightBook {
    pub(crate) n_insts: usize,
    pub(crate) n_accounts: usize,
    /// `[account_ix × (n_insts + 1) + inst]`.
    pub(crate) pos: Box<[InflightPos]>,
    /// Per table index.
    pub(crate) venue: Box<[VenueBook]>,
    /// (account, group) → Open in-flight notional, sorted by key; preallocated.
    pub(crate) group: Vec<(AccountId, GroupId, i64)>,
    pub(crate) venue_ix: [u8; 256],
}

impl InflightBook {
    pub(crate) fn new(
        n_accounts: usize,
        n_insts: usize,
        n_tables: usize,
        max_groups: usize,
        venue_ix: [u8; 256],
    ) -> Self {
        InflightBook {
            n_insts,
            n_accounts,
            pos: vec![InflightPos::ZERO; n_accounts.saturating_mul(n_insts + 1).max(1)].into_boxed_slice(),
            venue: vec![VenueBook::default(); n_tables.max(1)].into_boxed_slice(),
            group: Vec::with_capacity(max_groups),
            venue_ix,
        }
    }

    #[inline]
    fn pos_ix(&self, a: AccountId, inst: InstId) -> Option<usize> {
        let aix = usize::from(a.0).checked_sub(1)?;
        if aix >= self.n_accounts || inst.0 as usize > self.n_insts {
            return None;
        }
        Some(aix * (self.n_insts + 1) + inst.0 as usize)
    }

    #[inline]
    pub(crate) fn pos_mut(&mut self, a: AccountId, inst: InstId) -> Option<&mut InflightPos> {
        let i = self.pos_ix(a, inst)?;
        self.pos.get_mut(i)
    }

    /// In-flight quantities of (account, inst); unknown → zero.
    pub fn pos(&self, a: AccountId, inst: InstId) -> InflightPos {
        self.pos_ix(a, inst)
            .and_then(|i| self.pos.get(i))
            .copied()
            .unwrap_or(InflightPos::ZERO)
    }
    /// Remaining quantity of non-terminal buy orders (both kinds).
    pub fn open_buy(&self, a: AccountId, inst: InstId) -> Qty {
        self.pos(a, inst).open_buy
    }
    pub fn open_sell(&self, a: AccountId, inst: InstId) -> Qty {
        self.pos(a, inst).open_sell
    }
    /// Remaining quantity of non-terminal buy orders, immediate kind only.
    pub fn imm_buy(&self, a: AccountId, inst: InstId) -> Qty {
        self.pos(a, inst).imm_buy
    }
    pub fn imm_sell(&self, a: AccountId, inst: InstId) -> Qty {
        self.pos(a, inst).imm_sell
    }

    fn table_ix(&self, a: AccountId, v: VenueId) -> Option<usize> {
        let vix = *self.venue_ix.get(usize::from(v.0))?;
        if vix == u8::MAX {
            return None;
        }
        let aix = usize::from(a.0).checked_sub(1)?;
        if aix >= self.n_accounts {
            return None;
        }
        Some(usize::from(vix) * self.n_accounts + aix)
    }
    fn vbook(&self, a: AccountId, v: VenueId) -> VenueBook {
        self.table_ix(a, v)
            .and_then(|i| self.venue.get(i))
            .copied()
            .unwrap_or_default()
    }
    /// Open-class in-flight notional of (account, venue).
    pub fn open_notional_venue(&self, a: AccountId, v: VenueId) -> Notional {
        Notional(self.vbook(a, v).open_notional)
    }
    /// Spot buy in-flight notional of (account, venue) (`avail_quote`).
    pub fn buy_notional(&self, a: AccountId, v: VenueId) -> Notional {
        Notional(self.vbook(a, v).buy_notional)
    }
    /// Perp Open in-flight margin of (account, venue) (`avail_margin`).
    pub fn open_margin(&self, a: AccountId, v: VenueId) -> Notional {
        Notional(self.vbook(a, v).open_margin)
    }
    /// Open-class in-flight notional of (account, group).
    pub fn open_notional_group(&self, a: AccountId, g: GroupId) -> Notional {
        match self.group.binary_search_by_key(&(a, g), |(x, y, _)| (*x, *y)) {
            Ok(i) => Notional(self.group.get(i).map_or(0, |(_, _, n)| *n)),
            Err(_) => Notional(0),
        }
    }

    pub(crate) fn add_group(&mut self, a: AccountId, g: GroupId, d: i64) {
        match self.group.binary_search_by_key(&(a, g), |(x, y, _)| (*x, *y)) {
            Ok(i) => {
                let zero = if let Some(e) = self.group.get_mut(i) {
                    e.2 = e.2.saturating_add(d);
                    e.2 == 0
                } else {
                    false
                };
                if zero {
                    self.group.remove(i);
                }
            }
            Err(i) => {
                if d != 0 && self.group.len() < self.group.capacity() {
                    self.group.insert(i, (a, g, d));
                }
            }
        }
    }

    pub(crate) fn clear(&mut self) {
        for p in self.pos.iter_mut() {
            *p = InflightPos::ZERO;
        }
        for v in self.venue.iter_mut() {
            *v = VenueBook::default();
        }
        self.group.clear();
    }
}
