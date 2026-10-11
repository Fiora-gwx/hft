//! `hft-dst` (README "8. hft-dst 运行器"): the DST suites as a standalone program.
//!
//! ```text
//! hft-dst run   --seeds <from>..<to> [--suite <name>|all] [--steps 10000] [--deadline-s N] [--checkpoint-seeds 1000] --out <result.json> --crash-dir <dir>
//! hft-dst repro --seed 0x… --suite <name> --steps N [--trace] --crash-dir <dir>
//! hft-dst list
//! hft-dst hashtrail --seed 0x… --suite <name> --steps N   # one line: trail <xxh3-128 hex> points=<n>
//! ```
//!
//! Exit codes: 0 no violation, 1 violations, 2 argument or environment error. The engine is
//! `scenarios::DstEngine`.

use std::io::Write as _;
use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Instant;

use hft_testkit::dst::report::{repro_line, write_artifacts};
use hft_testkit::dst::scenario::{DstSuite, suite_for_seed};
use hft_testkit::dst::{DstOutcome, run};
use hft_testkit::scenarios::{DstEngine, suites};
use hft_testkit::seed::DST_DEFAULT_STEPS;

struct Args {
    cmd: String,
    seeds: Option<(u64, u64)>,
    seed: Option<u64>,
    suite: Option<String>,
    steps: u64,
    deadline_s: Option<u64>,
    checkpoint: u64,
    out: Option<PathBuf>,
    crash_dir: Option<PathBuf>,
    trace: bool,
}

fn parse_u64(s: &str) -> Option<u64> {
    match s.strip_prefix("0x") {
        Some(h) => u64::from_str_radix(h, 16).ok(),
        None => s.replace('_', "").parse().ok(),
    }
}

fn parse() -> Result<Args, String> {
    let mut it = std::env::args().skip(1);
    let cmd = it.next().ok_or("missing command (run | repro | list)")?;
    let mut a = Args {
        cmd,
        seeds: None,
        seed: None,
        suite: None,
        steps: DST_DEFAULT_STEPS,
        deadline_s: None,
        checkpoint: 1_000,
        out: None,
        crash_dir: None,
        trace: false,
    };
    while let Some(k) = it.next() {
        let mut val = || it.next().ok_or(format!("{k} needs a value"));
        match k.as_str() {
            "--seeds" => {
                let v = val()?;
                let (f, t) = v.split_once("..").ok_or("--seeds <from>..<to>")?;
                let (f, t) = (parse_u64(f).ok_or("bad seed")?, parse_u64(t).ok_or("bad seed")?);
                if t < f {
                    return Err("empty seed range".into());
                }
                a.seeds = Some((f, t));
            }
            "--seed" => a.seed = Some(parse_u64(&val()?).ok_or("bad --seed")?),
            "--suite" => a.suite = Some(val()?),
            "--steps" => a.steps = parse_u64(&val()?).ok_or("bad --steps")?,
            "--deadline-s" => a.deadline_s = Some(parse_u64(&val()?).ok_or("bad --deadline-s")?),
            "--checkpoint-seeds" => a.checkpoint = parse_u64(&val()?).ok_or("bad --checkpoint-seeds")?.max(1),
            "--out" => a.out = Some(PathBuf::from(val()?)),
            "--crash-dir" => a.crash_dir = Some(PathBuf::from(val()?)),
            "--trace" => a.trace = true,
            _ => return Err(format!("unknown argument {k}")),
        }
    }
    Ok(a)
}

fn pick<'a>(all: &'a [DstSuite], name: Option<&str>, seed: u64) -> Result<&'a DstSuite, String> {
    match name {
        None | Some("all") => suite_for_seed(all, seed).ok_or_else(|| "no suites".to_string()),
        Some(n) => all
            .iter()
            .find(|s| s.name == n)
            .ok_or_else(|| format!("unknown suite {n}")),
    }
}

fn atomic_write(path: &PathBuf, body: &str) -> std::io::Result<()> {
    let tmp = path.with_extension("new");
    {
        let mut f = std::fs::File::create(&tmp)?;
        f.write_all(body.as_bytes())?;
        f.sync_all()?;
    }
    std::fs::rename(tmp, path)
}

struct Violation {
    seed: u64,
    suite: &'static str,
    step: u64,
    invariant: String,
}

fn result_json(
    a: &Args,
    from: u64,
    done: Option<u64>,
    n: u64,
    v: &[Violation],
    elapsed: u64,
    names: &[&str],
) -> String {
    let suites: Vec<String> = names.iter().map(|s| format!("\"{s}\"")).collect();
    let viol: Vec<String> = v
        .iter()
        .map(|x| {
            format!(
                "{{\"seed\":\"{:#018x}\",\"suite\":\"{}\",\"step\":{},\"invariant\":\"{}\"}}",
                x.seed, x.suite, x.step, x.invariant
            )
        })
        .collect();
    format!(
        "{{\"bin_version\":\"{}\",\"git_sha\":\"{}\",\"profile\":\"{}\",\"suites\":[{}],\"seed_from\":{from},\"seed_to_done\":{},\"seeds_run\":{n},\"steps\":{},\"violations\":[{}],\"elapsed_s\":{elapsed}}}\n",
        env!("CARGO_PKG_VERSION"),
        option_env!("HFT_GIT_SHA").unwrap_or("unknown"),
        if cfg!(debug_assertions) { "ci" } else { "release" },
        suites.join(","),
        done.map_or("null".to_string(), |d| d.to_string()),
        a.steps,
        viol.join(",")
    )
}

fn cmd_list() -> ExitCode {
    for s in suites() {
        let rates: Vec<String> = hft_testkit::dst::faults::FaultCategory::ALL
            .iter()
            .filter(|c| s.faults.get(**c).on)
            .map(|c| format!("{}:{}", c.name(), s.faults.get(*c).per_mille))
            .collect();
        println!(
            "suite {} must_pass={} steps={} slot_ms={} faults={}",
            s.name,
            s.must_pass.join(","),
            s.steps,
            s.faults.slot_ms,
            rates.join(",")
        );
    }
    ExitCode::SUCCESS
}

fn cmd_run(a: &Args) -> Result<ExitCode, String> {
    let (from, to) = a.seeds.ok_or("run needs --seeds")?;
    let out = a.out.clone().ok_or("run needs --out")?;
    let crash = a.crash_dir.clone().ok_or("run needs --crash-dir")?;
    std::fs::create_dir_all(&crash).map_err(|e| format!("crash dir: {e}"))?;
    let all = suites();
    let names: Vec<&str> = match a.suite.as_deref() {
        None | Some("all") => all.iter().map(|s| s.name).collect(),
        Some(n) => vec![pick(&all, Some(n), 0)?.name],
    };
    let start = Instant::now();
    let mut v = Vec::new();
    let mut done = None;
    let mut n = 0u64;
    let inflight = crash.join(".inflight.json");
    for seed in from..=to {
        if a.deadline_s.is_some_and(|d| start.elapsed().as_secs() >= d) {
            break;
        }
        let s = pick(&all, a.suite.as_deref(), seed)?;
        let mut cfg = s.config(seed);
        cfg.steps = a.steps;
        let marker = format!(
            "{{\"seed\":\"{seed:#018x}\",\"suite\":\"{}\",\"steps\":{}}}\n",
            s.name, a.steps
        );
        atomic_write(&inflight, &marker).map_err(|e| format!("inflight marker: {e}"))?;
        let outcome = std::panic::catch_unwind(|| run::<DstEngine>(&cfg));
        match outcome {
            Ok(DstOutcome::Fail(f)) => {
                eprintln!("{}", repro_line(&f));
                let _ = write_artifacts(&crash, &f);
                v.push(Violation {
                    seed,
                    suite: s.name,
                    step: f.step,
                    invariant: f.invariant.label(),
                });
            }
            Err(_) => v.push(Violation {
                seed,
                suite: s.name,
                step: 0,
                invariant: "panic".into(),
            }),
            Ok(DstOutcome::Pass { .. }) => {}
        }
        n += 1;
        done = Some(seed);
        if n.is_multiple_of(a.checkpoint) {
            let j = result_json(a, from, done, n, &v, start.elapsed().as_secs(), &names);
            atomic_write(&out, &j).map_err(|e| format!("out: {e}"))?;
        }
    }
    let j = result_json(a, from, done, n, &v, start.elapsed().as_secs(), &names);
    atomic_write(&out, &j).map_err(|e| format!("out: {e}"))?;
    let _ = std::fs::remove_file(&inflight);
    Ok(if v.is_empty() {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    })
}

fn cmd_repro(a: &Args) -> Result<ExitCode, String> {
    let seed = a.seed.ok_or("repro needs --seed")?;
    let all = suites();
    let s = pick(&all, a.suite.as_deref(), seed)?;
    let mut cfg = s.config(seed);
    cfg.steps = a.steps;
    cfg.trace = a.trace;
    let rec = hft_testkit::dst::run_record::<DstEngine>(&cfg);
    if a.trace {
        for t in &rec.trace {
            println!(
                "{} {} {:?} {:#06x} seq={}",
                t.step, t.at_ns, t.src, t.kind.0, t.seq
            );
        }
    }
    match rec.outcome {
        DstOutcome::Fail(f) => {
            eprintln!("{}", repro_line(&f));
            if let Some(d) = &a.crash_dir {
                let _ = write_artifacts(d, &f);
            }
            Ok(ExitCode::from(1))
        }
        DstOutcome::Pass { .. } => Ok(ExitCode::SUCCESS),
    }
}

fn cmd_hashtrail(a: &Args) -> Result<ExitCode, String> {
    let seed = a.seed.ok_or("hashtrail needs --seed")?;
    let all = suites();
    let s = pick(&all, a.suite.as_deref(), seed)?;
    let mut cfg = s.config(seed);
    cfg.steps = a.steps;
    cfg.rerun = hft_testkit::dst::RerunPolicy::Never;
    let rec = hft_testkit::dst::run_record::<DstEngine>(&cfg);
    if let DstOutcome::Fail(f) = &rec.outcome {
        eprintln!("{}", repro_line(f));
        eprintln!("{}", f.message);
        return Ok(ExitCode::from(1));
    }
    let digest = xxhash_rust::xxh3::xxh3_128(&rec.trail.to_bytes());
    println!("trail {digest:032x} points={}", rec.trail.points.len());
    Ok(ExitCode::SUCCESS)
}

fn main() -> ExitCode {
    let a = match parse() {
        Ok(a) => a,
        Err(e) => {
            eprintln!("hft-dst: {e}");
            return ExitCode::from(2);
        }
    };
    let r = match a.cmd.as_str() {
        "list" => Ok(cmd_list()),
        "run" => cmd_run(&a),
        "repro" => cmd_repro(&a),
        "hashtrail" => cmd_hashtrail(&a),
        other => Err(format!("unknown command {other}")),
    };
    match r {
        Ok(c) => c,
        Err(e) => {
            eprintln!("hft-dst: {e}");
            ExitCode::from(2)
        }
    }
}
