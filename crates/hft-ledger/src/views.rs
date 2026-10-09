//! Portfolio view rows (types owned by hft-strategy-sdk "视图类型", field semantics by this
//! crate's README "权益数据") and `Portfolio`, the `PortfolioRead` implementor over a `Ledger`.

use hft_strategy_sdk::PortfolioRead;
pub use hft_strategy_sdk::{
    CashView, GroupPosView, InventoryView, LegPos, MarginView, PerpPosView, PnlSummary, VenueEquityView,
    VenueSet,
};
use hft_types::{AccountId, AssetId, GroupId, InstId, Notional, Px, Qty, VenueId};

use crate::ledger::Ledger;

/// Equity view of one account, borrowing the ledger (`Views::portfolio` of the sequencer adapter).
#[derive(Copy, Clone, Debug)]
pub struct Portfolio<'a> {
    ledger: &'a Ledger,
    account: AccountId,
}

impl Ledger {
    /// `PortfolioRead` view of account `a`.
    pub fn portfolio(&self, a: AccountId) -> Portfolio<'_> {
        Portfolio {
            ledger: self,
            account: a,
        }
    }
}

impl PortfolioRead for Portfolio<'_> {
    fn account(&self) -> AccountId {
        self.account
    }

    fn venues(&self) -> VenueSet {
        self.ledger
            .account_venues(self.account)
            .fold(VenueSet::EMPTY, |s, v| s.with(v))
    }

    fn venue(&self, v: VenueId) -> &VenueEquityView {
        &self.ledger.venue_book(self.account, v).summary
    }

    fn cash(&self, v: VenueId, asset: AssetId) -> CashView {
        self.ledger.cash_view(self.account, v, asset)
    }

    fn inventory(&self, v: VenueId, asset: AssetId) -> InventoryView {
        self.ledger.inventory_view(self.account, v, asset)
    }

    fn inventory_of(&self, v: VenueId, g: GroupId) -> Qty {
        self.ledger.inventory_of(self.account, v, g)
    }

    fn perp(&self, v: VenueId, i: InstId) -> PerpPosView {
        if self.ledger.inst(i).is_some_and(|s| s.venue == v) {
            self.ledger.perp_view(self.account, i)
        } else {
            PerpPosView {
                qty: Qty(0),
                entry_px: Px(0),
                upnl: Notional(0),
                liq_px: Px(0),
                adl_quantile: 0,
            }
        }
    }

    fn margin(&self, v: VenueId) -> MarginView {
        self.ledger.margin_view(self.account, v)
    }

    fn equity_total(&self) -> Notional {
        self.ledger.equity(self.account).total
    }
}
