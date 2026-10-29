//! Research replay (README): a seq journal through the same `Sequencer` and strategy code with an
//! in-process simulated venue, N parameter variants × M latency seeds, Parquet output.
#![forbid(unsafe_code)]

pub mod calib;
pub mod calib_build;
pub mod driver;
pub mod features;
pub mod parity;
pub mod run;
pub mod source;
pub mod table;
pub mod variants;
pub mod venue;

use std::path::{Path, PathBuf};

use hft_strategy_sdk::StrategySet;
use serde_json::json;

pub use driver::{DriverCfg, ResearchDriver};
pub use features::FeatureCfg;
pub use parity::ParitySummary;
pub use run::{RunCfg, RunOut, run_one};
pub use variants::Variant;
pub use venue::SimParams;

/// A research job (README "命令行").
#[derive(Clone, Debug)]
pub struct ResearchOpts {
    pub files: Vec<PathBuf>,
    pub accounts: Option<Vec<hft_types::AccountId>>,
    /// The baseline (no `set`) is always variant 0.
    pub variants: Vec<Variant>,
    pub seeds: Vec<u64>,
    pub sim: SimParams,
    pub driver: DriverCfg,
    pub reassembly: usize,
    /// Runs in parallel (threads; each run reads the journal itself).
    pub jobs: usize,
    /// Also write `features.parquet` (README "特征导出").
    pub features: Option<FeatureCfg>,
    pub out: PathBuf,
}

/// Content hash of the simulation parameters (README "输出" `calib_sha`).
pub fn sim_sha(p: &SimParams) -> String {
    use sha2::{Digest, Sha256};
    let s = format!("{p:?}");
    variants::hex(&Sha256::digest(s.as_bytes()))
}

/// Runs every (variant, seed) pair and writes `decisions/fills/orders/windows.parquet` and
/// `run.json` into `opts.out`. Returns the `run.json` value.
pub fn research<S: StrategySet>(opts: &ResearchOpts) -> Result<serde_json::Value, String> {
    let mut jobs: Vec<RunCfg> = Vec::new();
    for v in &opts.variants {
        for s in &opts.seeds {
            jobs.push(RunCfg {
                files: opts.files.clone(),
                accounts: opts.accounts.clone(),
                variant: v.clone(),
                sim: SimParams {
                    seed: *s,
                    ..opts.sim.clone()
                },
                driver: opts.driver,
                reassembly: opts.reassembly,
                features: opts.features,
            });
        }
    }
    let started = std::time::Instant::now();
    let results = run_all::<S>(&jobs, opts.jobs.max(1));
    let wall_s = started.elapsed().as_secs_f64();
    std::fs::create_dir_all(&opts.out).map_err(|e| format!("{}: {e}", opts.out.display()))?;
    let mut tables: [table::Table; 5] = Default::default();
    let mut runs = Vec::new();
    for (cfg, r) in jobs.iter().zip(results) {
        let mut r = r.map_err(|e| format!("variant {} seed {}: {e}", cfg.variant.name, cfg.sim.seed))?;
        for (k, t) in [
            &mut r.decisions,
            &mut r.fills,
            &mut r.orders,
            &mut r.windows,
            &mut r.features,
        ]
        .into_iter()
        .enumerate()
        {
            run::tag(t, &r.variant, r.seed);
            tables[k].extend(std::mem::take(t))?;
        }
        runs.push(json!({
            "variant": r.variant,
            "seed": r.seed,
            "set": cfg.variant.set,
            "params_sha": r.params_sha.iter().map(|(a, s)| json!({"account": a.0, "sha256": s})).collect::<Vec<_>>(),
            "accounts": r.mask.iter().map(|a| a.0).collect::<Vec<_>>(),
            "first_seq": r.first_seq,
            "start_snapshot": r.start_snapshot,
            "stats": r.stats,
            "sim_state_hash": format!("{:016x}", r.sim_state_hash),
        }));
    }
    let meta = json!({
        "tool": "replayer research",
        "version": env!("CARGO_PKG_VERSION"),
        "code_sha": option_env!("HFT_GIT_SHA").unwrap_or("unknown"),
        "journal": opts.files.iter().map(|p| p.display().to_string()).collect::<Vec<_>>(),
        "sim": format!("{:?}", opts.sim),
        "calib_sha": sim_sha(&opts.sim),
        "calib_file_sha": opts.sim.calib_file_sha,
        "own": opts.sim.own,
        "engine_timers": opts.driver.engine_timers,
        "features": opts.features.map(|f| json!({"every_ms": f.every_ms, "at_decisions": f.at_decisions})),
        "wall_s": wall_s,
        "runs": runs,
    });
    let kv = [("hft.run", meta.to_string())];
    for (name, t) in ["decisions", "fills", "orders", "windows", "features"]
        .iter()
        .zip(&tables)
    {
        if *name == "features" && opts.features.is_none() {
            continue;
        }
        t.write(&opts.out.join(format!("{name}.parquet")), &kv)?;
    }
    write_json(&opts.out.join("run.json"), &meta)?;
    Ok(meta)
}

/// A parity job (README "parity"): nothing simulated, the journal's own reports and timers.
#[derive(Clone, Debug)]
pub struct ParityOpts {
    pub files: Vec<PathBuf>,
    /// The baseline (no `set`) is always variant 0.
    pub variants: Vec<Variant>,
    pub reassembly: usize,
    pub jobs: usize,
    /// `Some` = the feature export (`features`), not decision parity.
    pub features: Option<FeatureCfg>,
    pub out: PathBuf,
}

/// Runs every variant without simulation and writes `parity.parquet` (one row per decision on
/// either side, `class` 0 both / 1 logged only / 2 regenerated only) and `parity.json`.
pub fn parity<S: StrategySet>(opts: &ParityOpts) -> Result<serde_json::Value, String> {
    let jobs: Vec<RunCfg> = opts
        .variants
        .iter()
        .map(|v| RunCfg {
            files: opts.files.clone(),
            accounts: Some(Vec::new()),
            variant: v.clone(),
            sim: SimParams::default(),
            driver: DriverCfg {
                engine_timers: false,
                ..DriverCfg::default()
            },
            reassembly: opts.reassembly,
            features: opts.features,
        })
        .collect();
    let results = run_all::<S>(&jobs, opts.jobs.max(1));
    std::fs::create_dir_all(&opts.out).map_err(|e| format!("{}: {e}", opts.out.display()))?;
    let mut table = table::Table::default();
    let mut runs = Vec::new();
    for (cfg, r) in jobs.iter().zip(results) {
        let r = r.map_err(|e| format!("variant {}: {e}", cfg.variant.name))?;
        let (mut t, s) = r.parity.ok_or("parity: nothing to compare")?;
        run::tag(&mut t, &r.variant, 0);
        table.extend(t)?;
        runs.push(json!({
            "variant": r.variant,
            "set": cfg.variant.set,
            "params_sha": r.params_sha.iter().map(|(a, s)| json!({"account": a.0, "sha256": s})).collect::<Vec<_>>(),
            "start_snapshot": r.start_snapshot,
            "summary": s,
            "rate": s.rate(),
        }));
    }
    let meta = json!({
        "tool": "replayer parity",
        "version": env!("CARGO_PKG_VERSION"),
        "code_sha": option_env!("HFT_GIT_SHA").unwrap_or("unknown"),
        "journal": opts.files.iter().map(|p| p.display().to_string()).collect::<Vec<_>>(),
        "runs": runs,
    });
    table.write(&opts.out.join("parity.parquet"), &[("hft.run", meta.to_string())])?;
    write_json(&opts.out.join("parity.json"), &meta)?;
    Ok(meta)
}

/// The feature export (README "特征导出"): every variant without simulation (the journal's own
/// reports, so the baseline rows are the values the live strategy read), `features.parquet` and
/// `features.json`.
pub fn features<S: StrategySet>(opts: &ParityOpts) -> Result<serde_json::Value, String> {
    let cfg = opts.features.ok_or("features: no FeatureCfg")?;
    let jobs: Vec<RunCfg> = opts
        .variants
        .iter()
        .map(|v| RunCfg {
            files: opts.files.clone(),
            accounts: Some(Vec::new()),
            variant: v.clone(),
            sim: SimParams::default(),
            driver: DriverCfg {
                engine_timers: false,
                ..DriverCfg::default()
            },
            reassembly: opts.reassembly,
            features: Some(cfg),
        })
        .collect();
    let results = run_all::<S>(&jobs, opts.jobs.max(1));
    std::fs::create_dir_all(&opts.out).map_err(|e| format!("{}: {e}", opts.out.display()))?;
    let mut table = table::Table::default();
    let mut runs = Vec::new();
    for (c, r) in jobs.iter().zip(results) {
        let mut r = r.map_err(|e| format!("variant {}: {e}", c.variant.name))?;
        run::tag(&mut r.features, &r.variant, 0);
        runs.push(json!({
            "variant": r.variant,
            "set": c.variant.set,
            "params_sha": r.params_sha.iter().map(|(a, s)| json!({"account": a.0, "sha256": s})).collect::<Vec<_>>(),
            "start_snapshot": r.start_snapshot,
            "rows": r.features.rows(),
        }));
        table.extend(std::mem::take(&mut r.features))?;
    }
    let cols = features::columns::<S>()?;
    let meta = json!({
        "tool": "replayer features",
        "version": env!("CARGO_PKG_VERSION"),
        "code_sha": option_env!("HFT_GIT_SHA").unwrap_or("unknown"),
        "journal": opts.files.iter().map(|p| p.display().to_string()).collect::<Vec<_>>(),
        "every_ms": cfg.every_ms,
        "at_decisions": cfg.at_decisions,
        "columns": cols.iter().map(|c| json!({"name": c.name, "unit": c.unit.name(), "exp": c.exp})).collect::<Vec<_>>(),
        "runs": runs,
    });
    table.write(
        &opts.out.join("features.parquet"),
        &[("hft.run", meta.to_string())],
    )?;
    write_json(&opts.out.join("features.json"), &meta)?;
    Ok(meta)
}

fn run_all<S: StrategySet>(jobs: &[RunCfg], par: usize) -> Vec<Result<RunOut, String>> {
    let mut out: Vec<Option<Result<RunOut, String>>> = (0..jobs.len()).map(|_| None).collect();
    for chunk in (0..jobs.len()).collect::<Vec<_>>().chunks(par) {
        let done: Vec<(usize, Result<RunOut, String>)> = std::thread::scope(|sc| {
            let hs: Vec<_> = chunk
                .iter()
                .map(|&i| {
                    let cfg = &jobs[i];
                    sc.spawn(move || (i, run_one::<S>(cfg)))
                })
                .collect();
            hs.into_iter()
                .map(|h| {
                    h.join()
                        .unwrap_or_else(|_| (usize::MAX, Err("run panicked".into())))
                })
                .collect()
        });
        for (i, r) in done {
            if let Some(slot) = out.get_mut(i) {
                *slot = Some(r);
            }
        }
    }
    out.into_iter()
        .map(|r| r.unwrap_or_else(|| Err("run panicked".into())))
        .collect()
}

fn write_json(path: &Path, v: &serde_json::Value) -> Result<(), String> {
    let s = serde_json::to_string_pretty(v).map_err(|e| e.to_string())?;
    std::fs::write(path, s).map_err(|e| format!("{}: {e}", path.display()))
}
