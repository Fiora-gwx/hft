//! Decision parity (README "parity"): the decisions regenerated from the journal's own reports
//! against the journal's logged decisions, matched per triggering Input.

use std::collections::BTreeMap;

use hft_event::{EventKind, OrderDecision};
use hft_sequencer::read_body;

use crate::driver::{Captured, ResearchSink};
use crate::table::{Col, Table};

/// Counts of one parity run.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize)]
pub struct ParitySummary {
    pub logged: u64,
    pub regenerated: u64,
    /// Decisions present on both sides (same trigger and same key).
    pub both: u64,
    pub logged_only: u64,
    pub regen_only: u64,
    /// Logged seq of the first trigger with a difference (0 = none).
    pub first_diff_trigger: u64,
}

impl ParitySummary {
    /// `both / max(logged, regenerated)` (1 when both are empty).
    pub fn rate(&self) -> f64 {
        let d = self.logged.max(self.regenerated);
        if d == 0 {
            return 1.0;
        }
        #[allow(clippy::cast_precision_loss)]
        let r = self.both as f64 / d as f64;
        r
    }
}

/// The compared fields of a decision (slot, gen and intent number are excluded: they shift once
/// the two sides diverge).
type Key = (u8, u8, u32, u8, u8, u8, i64, i64, u32, u32);

fn key(d: &OrderDecision) -> Key {
    (
        d.strategy.0,
        d.account.0,
        d.inst.0,
        d.action as u8,
        d.side as u8,
        d.tif as u8,
        d.px.0,
        d.qty.0,
        d.reject,
        d.reason,
    )
}

/// Classes of `parity.parquet` rows.
pub const BOTH: u8 = 0;
pub const LOGGED_ONLY: u8 = 1;
pub const REGEN_ONLY: u8 = 2;

/// Matches regenerated decisions (`regen`, keyed by the logged seq of their trigger through
/// `sink`) with `logged` (keyed by their `trigger_seq`) as multisets per trigger.
pub fn compare(sink: &ResearchSink, regen: &[Captured], logged: &[Captured]) -> (Table, ParitySummary) {
    let mut by_trigger: BTreeMap<u64, (Vec<Key>, Vec<Key>)> = BTreeMap::new();
    let mut s = ParitySummary::default();
    for c in logged.iter().filter(|c| c.kind == EventKind::ORDER_DECISION) {
        if let Some(d) = read_body::<OrderDecision>(&c.body) {
            by_trigger.entry(d.trigger_seq).or_default().0.push(key(&d));
            s.logged += 1;
        }
    }
    for c in regen.iter().filter(|c| c.kind == EventKind::ORDER_DECISION) {
        if let Some(d) = read_body::<OrderDecision>(&c.body) {
            by_trigger
                .entry(sink.log_seq(d.trigger_seq))
                .or_default()
                .1
                .push(key(&d));
            s.regenerated += 1;
        }
    }
    let mut cols: [Vec<i64>; 10] = Default::default();
    let (mut class, mut trig) = (Vec::new(), Vec::new());
    for (t, (mut lg, mut rg)) in by_trigger {
        lg.sort_unstable();
        rg.sort_unstable();
        let (mut i, mut j) = (0, 0);
        let mut row = |c: u8, k: &Key| {
            class.push(c);
            trig.push(t);
            for (n, v) in [
                i64::from(k.0),
                i64::from(k.1),
                i64::from(k.2),
                i64::from(k.3),
                i64::from(k.4),
                i64::from(k.5),
                k.6,
                k.7,
                i64::from(k.8),
                i64::from(k.9),
            ]
            .into_iter()
            .enumerate()
            {
                cols[n].push(v);
            }
        };
        let mut diff = false;
        while i < lg.len() || j < rg.len() {
            match (lg.get(i), rg.get(j)) {
                (Some(a), Some(b)) if a == b => {
                    row(BOTH, a);
                    s.both += 1;
                    i += 1;
                    j += 1;
                }
                (Some(a), Some(b)) if a < b => {
                    row(LOGGED_ONLY, a);
                    s.logged_only += 1;
                    diff = true;
                    i += 1;
                }
                (Some(a), None) => {
                    row(LOGGED_ONLY, a);
                    s.logged_only += 1;
                    diff = true;
                    i += 1;
                }
                (_, Some(b)) => {
                    row(REGEN_ONLY, b);
                    s.regen_only += 1;
                    diff = true;
                    j += 1;
                }
                (None, None) => break,
            }
        }
        if diff && s.first_diff_trigger == 0 {
            s.first_diff_trigger = t;
        }
    }
    let [
        strategy,
        account,
        inst,
        action,
        side,
        tif,
        px,
        qty,
        reject,
        reason,
    ] = cols;
    let t = Table::new(vec![
        ("class", Col::U8(class)),
        ("trigger_log_seq", Col::U64(trig)),
        ("strategy", Col::I64(strategy)),
        ("account", Col::I64(account)),
        ("inst", Col::I64(inst)),
        ("action", Col::I64(action)),
        ("side", Col::I64(side)),
        ("tif", Col::I64(tif)),
        ("px", Col::I64(px)),
        ("qty", Col::I64(qty)),
        ("reject", Col::I64(reject)),
        ("reason", Col::I64(reason)),
    ]);
    (t, s)
}
