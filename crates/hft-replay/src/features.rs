//! Feature export (README "特征导出"): rows of the strategies' own exported feature columns
//! (`Strategy::FEATURES` / `export_features`, the values the strategy's rules read) sampled after
//! the deciding Input and on an engine-UTC grid.

use hft_event::{EventKind, OrderDecision};
use hft_sequencer::{Sequencer, read_body};
use hft_strategy_sdk::features::NA;
use hft_strategy_sdk::{ColumnDesc, FeatureRow, StrategySet};

use crate::driver::ResearchDriver;
use crate::table::{Col, Table};

/// When rows are taken.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub struct FeatureCfg {
    /// One row per instance every this many ms of engine UTC (0 = none).
    pub every_ms: u32,
    /// One row per instance that decided, after the deciding Input.
    pub at_decisions: bool,
}

/// `cause` of a row.
pub const DECISION: u8 = 0;
pub const GRID: u8 = 1;

/// The union of every registered kind's columns, by name; one name with two descriptors is an
/// error (the Parquet column would have two meanings).
pub fn columns<S: StrategySet>() -> Result<Vec<ColumnDesc>, String> {
    let mut out: Vec<ColumnDesc> = Vec::new();
    for k in S::kinds() {
        for c in S::feature_columns(k.kind_id) {
            match out.iter().find(|o| o.name == c.name) {
                Some(o) if o != c => {
                    return Err(format!(
                        "feature column {}: kinds disagree ({o:?} vs {c:?})",
                        c.name
                    ));
                }
                Some(_) => {}
                None => out.push(*c),
            }
        }
    }
    Ok(out)
}

/// Rows under construction.
#[derive(Debug)]
pub struct Sampler {
    cfg: FeatureCfg,
    cols: Vec<ColumnDesc>,
    /// Union position of each column of a kind: `(kind_id, positions)`.
    pos: Vec<(u16, Vec<usize>)>,
    next_grid: i64,
    scanned: usize,
    buf: Vec<i64>,
    utc_ns: Vec<i64>,
    engine_ns: Vec<i64>,
    grid_utc_ns: Vec<i64>,
    seq: Vec<u64>,
    log_seq: Vec<u64>,
    cause: Vec<u8>,
    strategy: Vec<u8>,
    account: Vec<u8>,
    kind: Vec<u64>,
    vals: Vec<Vec<Option<i64>>>,
}

impl Sampler {
    pub fn new<S: StrategySet>(cfg: FeatureCfg) -> Result<Sampler, String> {
        let cols = columns::<S>()?;
        let pos = S::kinds()
            .iter()
            .map(|k| {
                let p = S::feature_columns(k.kind_id)
                    .iter()
                    .filter_map(|c| cols.iter().position(|u| u.name == c.name))
                    .collect();
                (k.kind_id, p)
            })
            .collect();
        let width = S::kinds()
            .iter()
            .map(|k| S::feature_columns(k.kind_id).len())
            .max()
            .unwrap_or(0);
        Ok(Sampler {
            cfg,
            vals: vec![Vec::new(); cols.len()],
            cols,
            pos,
            next_grid: 0,
            scanned: 0,
            buf: vec![NA; width],
            utc_ns: Vec::new(),
            engine_ns: Vec::new(),
            grid_utc_ns: Vec::new(),
            seq: Vec::new(),
            log_seq: Vec::new(),
            cause: Vec::new(),
            strategy: Vec::new(),
            account: Vec::new(),
            kind: Vec::new(),
        })
    }

    /// After one Input (the run uses one Input per round while exporting).
    pub fn after_input<S: StrategySet>(&mut self, seq: &Sequencer<ResearchDriver, S>) {
        let e = seq.engine();
        let now = e.now;
        let drv = seq.driver();
        if self.cfg.at_decisions {
            let mut decided: Vec<u8> = Vec::new();
            for c in drv.sink.captured.iter().skip(self.scanned) {
                if c.kind == EventKind::ORDER_DECISION
                    && let Some(d) = read_body::<OrderDecision>(&c.body)
                    && !decided.contains(&d.strategy.0)
                {
                    decided.push(d.strategy.0);
                }
            }
            for s in decided {
                self.row(seq, s, DECISION, 0);
            }
        }
        self.scanned = drv.sink.captured.len();
        let every = i64::from(self.cfg.every_ms) * 1_000_000;
        if every > 0 && now.utc_ns > 0 {
            if self.next_grid == 0 {
                self.next_grid = (now.utc_ns / every + 1) * every;
            } else if now.utc_ns >= self.next_grid {
                let grid = now.utc_ns / every * every;
                let ids: Vec<u8> = seq.instances().slots.iter().map(|s| s.id.0).collect();
                for s in ids {
                    self.row(seq, s, GRID, grid);
                }
                self.next_grid = grid + every;
            }
        }
    }

    fn row<S: StrategySet>(
        &mut self,
        seq: &Sequencer<ResearchDriver, S>,
        strategy: u8,
        cause: u8,
        grid: i64,
    ) {
        let Some(slot) = seq.instances().slots.iter().find(|s| s.id.0 == strategy) else {
            return;
        };
        let Some(inst) = &slot.inst else {
            return;
        };
        let Some((_, pos)) = self.pos.iter().find(|p| p.0 == slot.kind_id) else {
            return;
        };
        if pos.is_empty() {
            return;
        }
        self.buf.fill(NA);
        let mut row = FeatureRow::new(&mut self.buf);
        S::export_features(inst, &mut row);
        let mut put = vec![None; self.cols.len()];
        for (i, p) in pos.iter().enumerate() {
            if let (Some(v), Some(slot)) = (self.buf.get(i), put.get_mut(*p)) {
                *slot = (*v != NA).then_some(*v);
            }
        }
        for (col, v) in self.vals.iter_mut().zip(put) {
            col.push(v);
        }
        let e = seq.engine();
        self.utc_ns.push(e.now.utc_ns);
        self.engine_ns.push(e.now.engine_ns);
        self.grid_utc_ns.push(grid);
        self.seq.push(e.now.seq);
        self.log_seq.push(seq.driver().sink.log_seq(e.now.seq));
        self.cause.push(cause);
        self.strategy.push(strategy);
        self.account.push(slot.account.0);
        self.kind.push(u64::from(slot.kind_id));
    }

    /// `features.parquet` columns; every feature column carries `unit` and `exp` field metadata.
    pub fn table(self) -> Table {
        let mut cols = vec![
            ("utc_ns", Col::I64(self.utc_ns)),
            ("engine_ns", Col::I64(self.engine_ns)),
            ("grid_utc_ns", Col::I64(self.grid_utc_ns)),
            ("seq", Col::U64(self.seq)),
            ("log_seq", Col::U64(self.log_seq)),
            ("cause", Col::U8(self.cause)),
            ("strategy", Col::U8(self.strategy)),
            ("account", Col::U8(self.account)),
            ("kind_id", Col::U64(self.kind)),
        ];
        let mut meta = Vec::new();
        for (c, v) in self.cols.iter().zip(self.vals) {
            cols.push((c.name, Col::OptI64(v)));
            meta.push((c.name, "unit".to_string(), c.unit.name().to_string()));
            meta.push((c.name, "exp".to_string(), c.exp.to_string()));
        }
        let mut t = Table::new(cols);
        t.field_meta = meta;
        t
    }
}
