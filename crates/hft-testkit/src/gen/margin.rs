//! `margin_path` and `one_sided_move` (README "2. proptest 生成器").

use proptest::prelude::*;

/// Thresholds a margin path must cross (Rate raw, 10^9 = 1).
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub struct MarginThresholds {
    pub block_open: i64,
    pub reduce: i64,
    pub force: i64,
    pub hyst: i64,
}

impl MarginThresholds {
    /// 0.30 / 0.20 / 0.10, hysteresis 0.02 (TK-016).
    pub const TK016: MarginThresholds = MarginThresholds {
        block_open: 300_000_000,
        reduce: 200_000_000,
        force: 100_000_000,
        hyst: 20_000_000,
    };
}

/// One step of a margin path.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub struct MarginStep {
    /// Distance to liquidation per the exchange `liq_px` (Rate raw).
    pub liq_dist: i64,
    /// Distance per the ledger estimate (differs in mismatch segments).
    pub ledger_liq_dist: i64,
    /// Maintenance / margin ratio (Rate raw).
    pub ratio: i64,
    /// Data stale (no fresh margin report).
    pub stale: bool,
}

/// Paths of 100–2000 steps: a walk from above `block_open + hyst` down through `force − hyst` and
/// back up, with one mismatch segment, one "ratio ≥ 0.8 while liq_dist is loose" segment and one
/// stale segment at seeded positions.
pub fn margin_path(t: MarginThresholds) -> impl Strategy<Value = Vec<MarginStep>> {
    (100usize..=2_000, any::<u64>()).prop_map(move |(n, r)| {
        let hi = t.block_open + 2 * t.hyst;
        let lo = (t.force - 2 * t.hyst).max(1);
        let half = n / 2;
        let seg = (n / 10).max(3);
        let a = (r % (n as u64 - seg as u64)) as usize;
        let b = ((r >> 16) % (n as u64 - seg as u64)) as usize;
        let c = ((r >> 32) % (n as u64 - seg as u64)) as usize;
        (0..n)
            .map(|i| {
                let k = if i <= half { i } else { n - 1 - i } as i64;
                let liq = hi - (hi - lo) * k / half.max(1) as i64;
                let mismatch = (a..a + seg).contains(&i);
                let ratio_high = (b..b + seg).contains(&i);
                MarginStep {
                    liq_dist: if ratio_high { hi } else { liq },
                    ledger_liq_dist: if mismatch { liq + t.hyst * 3 } else { liq },
                    ratio: if ratio_high { 850_000_000 } else { 200_000_000 },
                    stale: (c..c + seg).contains(&i),
                }
            })
            .collect()
    })
}

/// Shape of a one-sided move.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub enum MoveShape {
    Linear,
    /// One equal jump every `every_ms`.
    Steps {
        every_ms: u64,
    },
    /// Linear for the rest, plus one jump of `start × gap_bp` at `at_ms`.
    Gap {
        at_ms: u64,
        gap_bp: i64,
    },
}

/// Price at `t_ms` of a move of `pct_bp` (basis points of the start) over `over_ms`.
pub fn move_px(start: i64, pct_bp: i64, over_ms: u64, shape: MoveShape, t_ms: u64) -> i64 {
    let t = t_ms.min(over_ms);
    let total = i128::from(start) * i128::from(pct_bp) / 10_000;
    let o = i128::from(over_ms.max(1));
    let d = match shape {
        MoveShape::Linear => total * i128::from(t) / o,
        MoveShape::Steps { every_ms } => {
            let n = i128::from(over_ms / every_ms.max(1)).max(1);
            let k = i128::from(t / every_ms.max(1)).min(n);
            total * k / n
        }
        MoveShape::Gap { at_ms, gap_bp } => {
            let gap = i128::from(start) * i128::from(gap_bp) / 10_000;
            let lin = (total - gap) * i128::from(t) / o;
            lin + if t >= at_ms { gap } else { 0 }
        }
    };
    start + d as i64
}

/// The move as `(t_ms, px)` points every `step_ms` from 0 to `over_ms` (inclusive) for one inst;
/// other insts are untouched by construction.
pub fn one_sided_move(
    start: i64,
    pct_bp: i64,
    over_ms: u64,
    shape: MoveShape,
    step_ms: u64,
) -> Vec<(u64, i64)> {
    let mut v = Vec::new();
    let mut t = 0;
    while t <= over_ms {
        v.push((t, move_px(start, pct_bp, over_ms, shape, t)));
        t += step_ms.max(1);
    }
    v
}
