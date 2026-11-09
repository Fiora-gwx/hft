//! `binary-maker` (kind_id 7): a maker of binary outcome tokens. It rests a post-only bid at the best
//! bid of the Up and the Down token of the current window while a reference market's realized
//! volatility is high enough, and pulls the quotes near the window end and after a reference jump.
//! Specification: `strategies/binary-maker/README.md`.
#![forbid(unsafe_code)]
#![deny(
    clippy::float_arithmetic,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unreachable,
    clippy::todo,
    clippy::unimplemented
)]

pub mod desc;
pub mod fast;
pub mod fcev;
pub mod features;
pub mod params;

use hft_strategy_sdk::{
    Ctx, EventRef, GroupTable, NoModel, ParamError, RefView, RestoreError, SnapshotReader, SnapshotWriter,
    Strategy, StrategyDesc, StrategyInstance, SubsBuilder, Views,
};

pub use params::{BinaryMakerFields, BinaryMakerParams};

/// The live base parameters (`micro_s2_c5_cap10_mc20`, research reply 2026-10-22): enabled, 5
/// shares per quote, net 10, no-chase 1 s, fair-value cooldown 1 s, streak pause (2 fills within
/// 10 s → 10 s), at most 20 USDC bought per window. hft-pmcfg's live configuration and the
/// replayer's `--param-base live` both start from it.
pub const LIVE_BASE: [(&str, i64); 9] = [
    ("enabled", 1),
    ("clip_shares", 5),
    ("inv_max_shares", 10),
    ("fv_cool_ms", 1_000),
    ("nochase_ms", 1_000),
    ("streak_k", 2),
    ("streak_window_ms", 10_000),
    ("streak_pause_ms", 10_000),
    ("max_window_cost", 20_000_000),
];

/// 1e-8 USD per unit of the Chainlink inst's price (10^(8 + px_exp)); 0 without a Chainlink group
/// or with a price exponent outside −8..=0.
fn cl_mul(f: &fast::Fast, rd: &RefView<'_>) -> i64 {
    if f.clref == hft_strategy_sdk::prelude::INST_NONE {
        return 0;
    }
    let e = rd.try_inst(f.clref).map_or(i8::MIN, |i| i.px_exp);
    if (-8..=0).contains(&e) {
        10i64.pow(u32::from(e.unsigned_abs()).abs_diff(8))
    } else {
        0
    }
}

/// `StrategyDesc.version` (fast-part snapshot layout and decision semantics).
pub const DESC_VERSION: u16 = 7;

/// The strategy (registry row `binary_maker::BinaryMaker`).
#[derive(Debug)]
pub struct BinaryMaker {
    pub inst: StrategyInstance,
    pub fast: fast::Fast,
}

impl Strategy for BinaryMaker {
    const DESC: StrategyDesc = StrategyDesc {
        name: "binary-maker",
        kind_id: 7,
        version: DESC_VERSION,
        non_prod_only: false,
        schema: &BinaryMakerParams::SCHEMA,
        notes: &desc::NOTES,
        report: &desc::REPORT,
        cmds: &desc::CMDS,
    };
    type Params = BinaryMakerParams;
    type Model = NoModel<BinaryMakerParams>;

    fn new(
        inst: StrategyInstance,
        p: &BinaryMakerParams,
        groups: &GroupTable,
        rd: &RefView<'_>,
    ) -> Result<Self, ParamError> {
        let mut fast = fast::Fast::new(inst.account, p, groups);
        fast.cl_mul = cl_mul(&fast, rd);
        Ok(BinaryMaker { inst, fast })
    }

    fn subscriptions(&self, groups: &GroupTable, out: &mut SubsBuilder<'_>) {
        fast::subscribe(groups, self.fast.p.defaults().ref_aux, out);
    }

    fn on_event<V: Views>(&mut self, ev: &EventRef<'_>, v: &V, ctx: &mut Ctx<'_>) {
        self.fast.on_event(ev, v, ctx);
    }

    fn on_params(
        &mut self,
        p: &BinaryMakerParams,
        groups: &GroupTable,
        rd: &RefView<'_>,
    ) -> Result<(), ParamError> {
        self.fast.on_params(p, groups);
        self.fast.cl_mul = cl_mul(&self.fast, rd);
        Ok(())
    }

    fn snapshot(&self, w: &mut SnapshotWriter) {
        self.fast.snapshot(w);
    }

    fn restore(&mut self, r: &mut SnapshotReader<'_>) -> Result<(), RestoreError> {
        self.fast.restore(r)
    }
    const FEATURES: &'static [hft_strategy_sdk::ColumnDesc] = &features::STRATEGY_COLUMNS;
    fn export_features(&self, row: &mut hft_strategy_sdk::FeatureRow<'_>) {
        self.fast.feat.export(row);
        let c = &self.fast.comp;
        if c.ok {
            row.put(c.win);
            row.put(c.z[0]);
            row.put(c.z[1]);
            for x in self
                .fast
                .comp_f
                .iter()
                .flatten()
                .chain(self.fast.comp_book.iter())
            {
                row.put(*x);
            }
        } else {
            for _ in 0..features::COMP_COLUMNS.len() {
                row.put(hft_strategy_sdk::features::NA);
            }
        }
        for x in self.fast.settle.out {
            row.put(x);
        }
    }
}
