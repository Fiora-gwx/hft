//! `BboTop` (README "节点库"): 2 × mid and level-1 quantity imbalance of the last two-sided quote.

use crate::fixed::ONE;
use crate::{ColumnDesc, Events, Feature, FeatureRow, Unit};

/// One quote: bid price, bid quantity, ask price, ask quantity (raw).
pub type TopInput = (i64, i64, i64, i64);

#[derive(Copy, Clone, Debug, Default, Eq, PartialEq)]
pub struct BboTop {
    /// bid + ask of the last two-sided quote (raw price units); 0 = none yet.
    pub mid_x2: i64,
    /// (bid − ask quantity) / (bid + ask) of the last quote with non-negative quantities and a
    /// positive sum (`Rate`); kept from the previous quote otherwise.
    pub imb: i64,
}

impl BboTop {
    /// Handler: a quote; returns `true` when it was two-sided (bid > 0 and ask > 0), i.e. when
    /// `mid_x2` was updated.
    pub fn on_quote(&mut self, bid_px: i64, bid_qty: i64, ask_px: i64, ask_qty: i64) -> bool {
        if bid_px <= 0 || ask_px <= 0 {
            return false;
        }
        self.mid_x2 = bid_px.saturating_add(ask_px);
        let (bq, aq) = (i128::from(bid_qty), i128::from(ask_qty));
        if bq >= 0 && aq >= 0 && bq + aq > 0 {
            self.imb = ((bq - aq) * i128::from(ONE) / (bq + aq)) as i64;
        }
        true
    }
}

impl Feature for BboTop {
    type Input<'a> = TopInput;
    const EVENTS: Events = Events::BBO;
    const COLUMNS: &'static [ColumnDesc] = &[
        ColumnDesc {
            name: "mid_x2",
            unit: Unit::Px,
            exp: 0,
        },
        ColumnDesc {
            name: "imb",
            unit: Unit::Ratio,
            exp: -9,
        },
    ];
    fn on_event(&mut self, x: TopInput) {
        self.on_quote(x.0, x.1, x.2, x.3);
    }
    fn export(&self, row: &mut FeatureRow<'_>) {
        row.put(self.mid_x2);
        row.put(self.imb);
    }
}
