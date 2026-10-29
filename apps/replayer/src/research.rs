//! `replayer research` (README "research"): option parsing and the hft-replay call.

use std::io::Write;
use std::path::PathBuf;

use hft_replay::{DriverCfg, ResearchOpts, SimParams, Variant};
use hft_strategy_sdk::StrategySet;

use crate::Failure;

const USAGE: &str = "usage: replayer research --journal <file|dir> --out <dir> [--account N]... \
[--variants file.toml] [--set NAME=RAW[,NAME=RAW...]] [--seeds S[,S...]] [--latency ENTRY_US:RESP_US] \
[--maker-fill queue|through] [--reseed-s N] [--log-timers] [--jobs N] [--reassembly-bytes N]";

pub(crate) fn parse(args: &[String]) -> Result<ResearchOpts, String> {
    let mut journal = None;
    let mut out = None;
    let mut accounts = Vec::new();
    let mut variants = vec![Variant {
        name: "base".into(),
        set: Default::default(),
    }];
    let mut seeds = vec![1u64];
    let mut sim = SimParams::default();
    let mut driver = DriverCfg::default();
    let mut jobs = 1usize;
    let mut reassembly = 128usize << 20;
    let mut i = 1;
    while i < args.len() {
        let key = args[i].as_str();
        i += 1;
        if key == "--log-timers" {
            driver.engine_timers = false;
            continue;
        }
        let value = args
            .get(i)
            .filter(|s| !s.starts_with("--"))
            .ok_or_else(|| format!("{key} needs a value; {USAGE}"))?
            .clone();
        i += 1;
        let num = |v: &str| v.parse::<u64>().map_err(|_| format!("{key} {v}: not a number"));
        match key {
            "--journal" => journal = Some(PathBuf::from(value)),
            "--out" => out = Some(PathBuf::from(value)),
            "--account" => accounts.push(hft_types::AccountId(
                u8::try_from(num(&value)?).map_err(|_| format!("--account {value}: 1..=255"))?,
            )),
            "--variants" => {
                let text = std::fs::read_to_string(&value).map_err(|e| format!("{value}: {e}"))?;
                variants.extend(hft_replay::variants::parse_file(&text)?);
            }
            "--set" => variants.push(Variant {
                name: format!("set{}", variants.len()),
                set: hft_replay::variants::parse_sets(&value)?,
            }),
            "--seeds" => {
                seeds = value
                    .split(',')
                    .map(|s| num(s.trim()))
                    .collect::<Result<_, _>>()?;
                if seeds.is_empty() {
                    return Err("--seeds needs at least one seed".into());
                }
            }
            "--latency" => {
                let (e, r) = value
                    .split_once(':')
                    .ok_or_else(|| format!("--latency {value}: ENTRY_US:RESP_US"))?;
                sim.latency = hft_sim::LatencyModel::Fixed {
                    entry_us: u32::try_from(num(e)?).map_err(|_| "--latency: entry too large")?,
                    resp_us: u32::try_from(num(r)?).map_err(|_| "--latency: resp too large")?,
                };
            }
            "--maker-fill" => {
                sim.maker_fill = match value.as_str() {
                    "queue" => hft_sim::MakerFill::Queue,
                    "through" => hft_sim::MakerFill::TradeThrough,
                    _ => return Err(format!("--maker-fill {value}: queue|through")),
                }
            }
            "--reseed-s" => sim.reseed_s = u32::try_from(num(&value)?).map_err(|_| "--reseed-s too large")?,
            "--jobs" => {
                jobs = usize::try_from(num(&value)?)
                    .map_err(|_| "--jobs too large")?
                    .max(1)
            }
            "--reassembly-bytes" => reassembly = usize::try_from(num(&value)?).map_err(|_| "too large")?,
            _ => return Err(format!("unsupported option {key}; {USAGE}")),
        }
    }
    let journal = journal.ok_or_else(|| format!("--journal is required; {USAGE}"))?;
    let out = out.ok_or_else(|| format!("--out is required; {USAGE}"))?;
    let mut names = std::collections::BTreeSet::new();
    for v in &variants {
        if !names.insert(v.name.clone()) {
            return Err(format!("duplicate variant name {}", v.name));
        }
    }
    let mut files = Vec::new();
    crate::discover(&journal, &mut files).map_err(|(_, m)| m)?;
    files.sort();
    Ok(ResearchOpts {
        files,
        accounts: (!accounts.is_empty()).then_some(accounts),
        variants,
        seeds,
        sim,
        driver,
        reassembly,
        jobs,
        out,
    })
}

pub(crate) fn execute<S: StrategySet>(args: &[String], stdout: &mut impl Write) -> Result<i32, Failure> {
    let opts = parse(args).map_err(|e| (2, e))?;
    if opts.files.is_empty() {
        return Err((3, "no journal segment found".into()));
    }
    let meta = hft_replay::research::<S>(&opts).map_err(|e| (3, e))?;
    writeln!(stdout, "{meta}").map_err(|e| (2, e.to_string()))?;
    Ok(0)
}

const PARITY_USAGE: &str = "usage: replayer parity --journal <file|dir> --out <dir> [--variants file.toml] [--set NAME=RAW[,NAME=RAW...]] [--jobs N] [--reassembly-bytes N]";

/// `replayer parity`: the research options that apply without simulation.
pub(crate) fn parse_parity(args: &[String]) -> Result<hft_replay::ParityOpts, String> {
    for k in args.iter().skip(1).filter(|a| a.starts_with("--")) {
        if !matches!(
            k.as_str(),
            "--journal" | "--out" | "--variants" | "--set" | "--jobs" | "--reassembly-bytes"
        ) {
            return Err(format!("unsupported option {k}; {PARITY_USAGE}"));
        }
    }
    let o = parse(args).map_err(|e| e.replace(USAGE, PARITY_USAGE))?;
    Ok(hft_replay::ParityOpts {
        files: o.files,
        variants: o.variants,
        reassembly: o.reassembly,
        jobs: o.jobs,
        out: o.out,
    })
}

/// Exit 1 when the baseline (the journal's own parameters) differs from the logged decisions.
pub(crate) fn execute_parity<S: StrategySet>(
    args: &[String],
    stdout: &mut impl Write,
) -> Result<i32, Failure> {
    let opts = parse_parity(args).map_err(|e| (2, e))?;
    if opts.files.is_empty() {
        return Err((3, "no journal segment found".into()));
    }
    let meta = hft_replay::parity::<S>(&opts).map_err(|e| (3, e))?;
    writeln!(stdout, "{meta}").map_err(|e| (2, e.to_string()))?;
    let base = &meta["runs"][0]["summary"];
    let differs = base["logged_only"].as_u64().unwrap_or(1) + base["regen_only"].as_u64().unwrap_or(1) > 0;
    Ok(i32::from(differs))
}
