//! Parameter checks against the account/instance binding (`SetParams`), cross-version binding
//! changes and `ref_inst` checks. Cold path; the success paths never allocate.

use hft_types::{AccountId, AssetId, InstId, StrategyInstanceId};

use crate::GroupSpecView;
use crate::RefData;
use crate::blob::sec;
use crate::types::{InstStatus, LoadErr, LoadErrCode as C, LoadReport};

#[inline]
const fn tradable(s: InstStatus) -> bool {
    matches!(s, InstStatus::Trading | InstStatus::ReduceOnly)
}

impl RefData {
    /// `SetParams` binding check: the account is bound to `s` (`AccountBound`) and the
    /// instance's `kind_id` equals `kind_id` (`KindMismatch`).
    pub fn check_binding(&self, a: AccountId, s: StrategyInstanceId, kind_id: u16) -> Result<(), LoadErr> {
        let acc = self.account(a);
        if a.0 == 0 || s.0 == 0 || acc.bound_instance != s {
            return Err(LoadErr::new(C::AccountBound, 0, 0));
        }
        match self.instance(s) {
            Some(i) if i.kind_id == kind_id => Ok(()),
            _ => Err(LoadErr::new(C::KindMismatch, 0, 0)),
        }
    }

    /// Group check (cross-blob rule 2): legs in order (tradable, venue, base asset), then
    /// between legs (same quote, no two legs on one exchange). Returns the quote asset;
    /// `row` of the error = leg index. An observe-only group (`open_allowed = false` and no leg
    /// venue in the account, e.g. a reference market) is exempt from the account-venue rule only.
    pub fn check_group(&self, a: AccountId, g: &GroupSpecView<'_>) -> Result<AssetId, LoadErr> {
        let venues = self.account(a).venues;
        let in_account = |v: hft_types::VenueId| v.0 < 64 && venues & (1u64 << v.0) != 0;
        let observe = !g.open_allowed && g.legs.iter().all(|l| !in_account(l.venue));
        for (k, leg) in g.legs.iter().enumerate() {
            let err = |c| Err(LoadErr::new(c, 0, k as u32));
            let h = self.inst(leg.inst);
            if !tradable(h.status) {
                return err(C::LegNotTradable);
            }
            if leg.venue != h.venue || !(observe || in_account(leg.venue)) {
                return err(C::LegVenueNotInAccount);
            }
            if h.asset != g.asset {
                return err(C::LegAssetMismatch);
            }
        }
        let Some(first) = g.legs.first() else {
            return Err(LoadErr::new(C::LegNotTradable, 0, 0));
        };
        let quote = self.inst(first.inst).quote;
        for (k, leg) in g.legs.iter().enumerate() {
            if self.inst(leg.inst).quote != quote {
                return Err(LoadErr::new(C::QuoteMismatch, 0, k as u32));
            }
        }
        for (k, leg) in g.legs.iter().enumerate() {
            let ex = self.venue(leg.venue).exchange;
            if g.legs.iter().take(k).any(|p| self.venue(p.venue).exchange == ex) {
                return Err(LoadErr::new(C::SameExchangeLegs, 0, k as u32));
            }
        }
        Ok(quote)
    }

    /// Binding changes against `prev`: an account whose `bound_instance` changed while
    /// `busy(a)` → `BindingChangedWithPosition` with `row = AccountId`.
    pub fn check_rebind(&self, prev: &RefData, busy: impl Fn(AccountId) -> bool) -> Result<(), LoadReport> {
        let n = self.accounts.len().max(prev.accounts.len());
        let errors: Vec<LoadErr> = (1..n)
            .filter_map(|i| u8::try_from(i).ok().map(AccountId))
            .filter(|&a| self.account(a).bound_instance != prev.account(a).bound_instance && busy(a))
            .map(|a| LoadErr::new(C::BindingChangedWithPosition, sec::ACCOUNTS, u32::from(a.0)))
            .collect();
        if errors.is_empty() {
            Ok(())
        } else {
            Err(LoadReport { errors })
        }
    }

    /// A `ref_inst` must exist and be `Trading`/`ReduceOnly` (`RefInstNotSubscribed`).
    pub fn check_ref_inst(&self, id: InstId) -> Result<(), LoadErr> {
        if tradable(self.inst(id).status) {
            Ok(())
        } else {
            Err(LoadErr::new(C::RefInstNotSubscribed, 0, 0))
        }
    }
}
