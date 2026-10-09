//! Effective-leverage table from `AccountConfigReport`, the traded perp insts of each account
//! and the leverage cap including unknown leverage (README "永续杠杆上限").

use hft_event::{AccountConfigReportHdr, LeverageItem};
use hft_types::{AccountId, InstId, Rate, VenueId};

use crate::vec::PreVec;

/// `AccountConfigReportHdr.rflags`.
pub mod acr_rflags {
    pub const PARTIAL: u16 = 1;
    pub const LEV_TRUNCATED: u16 = 2;
}

#[derive(Clone, Debug)]
pub(crate) struct Leverage {
    pub n_insts: usize,
    /// account × (n_insts + 1) + inst → leverage (0 = unknown).
    pub lev: Vec<Rate>,
    /// Traded perp insts `(account, venue, inst)`, sorted.
    pub trading: PreVec<(AccountId, VenueId, InstId)>,
    /// account × 64 + venue.
    pub bad: Vec<bool>,
}

impl Leverage {
    pub fn new(n_accounts: usize, n_insts: usize, max_trading: usize) -> Leverage {
        Leverage {
            n_insts,
            lev: vec![Rate(0); (n_accounts + 1) * (n_insts + 1)],
            trading: PreVec::with_capacity(max_trading),
            bad: vec![false; (n_accounts + 1) * 64],
        }
    }

    fn ix(&self, a: AccountId, i: InstId) -> Option<usize> {
        if i.0 as usize > self.n_insts {
            return None;
        }
        Some(usize::from(a.0) * (self.n_insts + 1) + i.0 as usize)
    }

    pub fn get(&self, a: AccountId, i: InstId) -> Option<Rate> {
        let r = self.ix(a, i).and_then(|x| self.lev.get(x)).copied()?;
        (r.0 > 0).then_some(r)
    }

    pub fn set(&mut self, a: AccountId, i: InstId, r: Rate) {
        if let Some(x) = self.ix(a, i)
            && let Some(s) = self.lev.get_mut(x)
        {
            *s = if r.0 > 0 { r } else { Rate(0) };
        }
    }

    pub fn is_bad(&self, a: AccountId, v: VenueId) -> bool {
        self.bad
            .get(usize::from(a.0) * 64 + usize::from(v.0 & 63))
            .copied()
            .unwrap_or(false)
    }

    /// Applies one report (`inst_venue(i)` = the inst's venue).
    pub fn on_report(
        &mut self,
        hdr: &AccountConfigReportHdr,
        items: &[LeverageItem],
        inst_venue: impl Fn(InstId) -> VenueId,
    ) {
        let (a, v) = (hdr.account, hdr.venue);
        if hdr.rflags & acr_rflags::PARTIAL == 0 {
            for i in 1..=self.n_insts {
                let id = InstId(i as u32);
                if inst_venue(id) == v {
                    self.set(a, id, Rate(0));
                }
            }
        }
        for it in items {
            if inst_venue(it.inst) == v {
                self.set(a, it.inst, it.leverage);
            }
        }
    }

    /// Re-judges `lev_bad` of (a, v) against `max_x100`.
    pub fn recheck(&mut self, a: AccountId, v: VenueId, max_x100: u32) {
        let lim = i128::from(max_x100) * 10_000_000;
        let mut bad = false;
        for (ta, tv, ti) in self.trading.iter() {
            if *ta != a || *tv != v {
                continue;
            }
            match self.get(a, *ti) {
                None => bad = true,
                Some(r) if i128::from(r.0) > lim => bad = true,
                Some(_) => {}
            }
        }
        if let Some(x) = self.bad.get_mut(usize::from(a.0) * 64 + usize::from(v.0 & 63)) {
            *x = bad;
        }
    }
}
