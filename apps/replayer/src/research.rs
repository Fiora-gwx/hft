//! `replayer research` (README "research"): option parsing and the hft-replay call.

use std::io::Write;
use std::path::PathBuf;

use hft_replay::{DriverCfg, ResearchOpts, SimParams, Variant};
use hft_strategy_sdk::StrategySet;

use crate::Failure;

const USAGE: &str = "usage: replayer research --journal <file|dir> --out <dir> [--account N]... \
[--variants file.toml] [--set NAME=RAW[,NAME=RAW...]] [--seeds S[,S...]] [--latency ENTRY_US:RESP_US] \
[--maker-fill queue|queue-cap|through] [--reseed-s N] [--log-timers] [--jobs N] [--reassembly-bytes N] \
[--calib calib.json] [--own] [--tombstone] [--cancel-latency ENTRY_US:RESP_US] \
[--taker-delay BASE_US[:SLOW_US:SLOW_PPM]] [--taker-resp-us N] [--fee-binary K] [--gtd-ms N] [--fill-resp-us N]";

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
        if key == "--own" {
            sim.own = true;
            continue;
        }
        if key == "--tombstone" {
            sim.venue.tombstone = true;
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
                    "queue-cap" => hft_sim::MakerFill::QueueCap,
                    "through" => hft_sim::MakerFill::TradeThrough,
                    _ => return Err(format!("--maker-fill {value}: queue|queue-cap|through")),
                }
            }
            "--calib" => {
                let text = std::fs::read(&value).map_err(|e| format!("{value}: {e}"))?;
                sim.calib_file_sha = hft_replay::calib::apply(&text, &mut sim)?;
            }
            "--cancel-latency" => {
                let (e, r) = value
                    .split_once(':')
                    .ok_or_else(|| format!("--cancel-latency {value}: ENTRY_US:RESP_US"))?;
                let us = |v: &str| u32::try_from(num(v)?).map_err(|_| format!("{key}: {v} too large"));
                sim.venue.cancel_us = Some((us(e)?, us(r)?));
            }
            "--taker-delay" => {
                let us = |v: &str| u32::try_from(num(v)?).map_err(|_| format!("{key}: {v} too large"));
                let p: Vec<&str> = value.split(':').collect();
                let (base, slow, ppm) = match p.as_slice() {
                    [b] => (us(b)?, 0, 0),
                    [b, s, q] => (us(b)?, us(s)?, us(q)?),
                    _ => return Err(format!("--taker-delay {value}: BASE_US[:SLOW_US:SLOW_PPM]")),
                };
                let resp = sim.venue.taker.map_or(0, |t| t.resp_us);
                sim.venue.taker = Some(hft_sim::TakerDelay {
                    base_us: base,
                    slow_us: slow,
                    slow_ppm: ppm,
                    resp_us: resp,
                });
            }
            "--taker-resp-us" => {
                let r = u32::try_from(num(&value)?).map_err(|_| "--taker-resp-us too large")?;
                let t = sim.venue.taker.get_or_insert(hft_sim::TakerDelay {
                    base_us: 0,
                    slow_us: 0,
                    slow_ppm: 0,
                    resp_us: 0,
                });
                t.resp_us = r;
            }
            "--fee-binary" => {
                let k: f64 = value
                    .parse()
                    .map_err(|_| format!("--fee-binary {value}: a fraction"))?;
                if !(0.0..=1.0).contains(&k) {
                    return Err(format!("--fee-binary {value}: 0..=1"));
                }
                #[allow(clippy::cast_possible_truncation)]
                let k_e9 = (k * 1e9).round() as i64;
                sim.venue.fee = hft_sim::FeeModel::BinaryTaker { k_e9 };
            }
            "--gtd-ms" => sim.venue.gtd_ms = u32::try_from(num(&value)?).map_err(|_| "--gtd-ms too large")?,
            "--fill-resp-us" => {
                sim.venue.fill_resp_us =
                    Some(u32::try_from(num(&value)?).map_err(|_| "--fill-resp-us too large")?)
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

const CALIB_USAGE: &str =
    "usage: replayer calib --journal <file|dir> --account N --out calib.json [--window-s N] [--phase-s N]";

/// `replayer calib`: the conditional latency pools of one account from a live journal.
pub(crate) fn execute_calib(args: &[String], stdout: &mut impl Write) -> Result<i32, Failure> {
    let mut journal = None;
    let mut out = None;
    let mut account = None;
    let (mut window_s, mut phase_s) = (300u32, 5u32);
    let mut i = 1;
    while i < args.len() {
        let key = args[i].as_str();
        let value = args
            .get(i + 1)
            .filter(|s| !s.starts_with("--"))
            .ok_or_else(|| (2, format!("{key} needs a value; {CALIB_USAGE}")))?
            .clone();
        i += 2;
        let n = |v: &str| {
            v.parse::<u32>()
                .map_err(|_| (2, format!("{key} {v}: not a number")))
        };
        match key {
            "--journal" => journal = Some(PathBuf::from(value)),
            "--out" => out = Some(PathBuf::from(value)),
            "--account" => {
                account = Some(hft_types::AccountId(
                    u8::try_from(n(&value)?).map_err(|_| (2, format!("--account {value}: 1..=255")))?,
                ))
            }
            "--window-s" => window_s = n(&value)?,
            "--phase-s" => phase_s = n(&value)?,
            _ => return Err((2, format!("unsupported option {key}; {CALIB_USAGE}"))),
        }
    }
    let (Some(journal), Some(out), Some(account)) = (journal, out, account) else {
        return Err((2, CALIB_USAGE.into()));
    };
    let mut files = Vec::new();
    crate::discover(&journal, &mut files)?;
    files.sort();
    if files.is_empty() {
        return Err((3, "no journal segment found".into()));
    }
    let v = hft_replay::calib_build::build(&hft_replay::calib_build::CalibOpts {
        files,
        account,
        reassembly: 128 << 20,
        window_s,
        phase_s,
    })
    .map_err(|e| (3, e))?;
    let text = serde_json::to_string_pretty(&v).map_err(|e| (2, e.to_string()))?;
    std::fs::write(&out, text).map_err(|e| (2, format!("{}: {e}", out.display())))?;
    writeln!(stdout, "{}", v["meta"]).map_err(|e| (2, e.to_string()))?;
    Ok(0)
}
