//! `latency-slice`: the M1.5 latency-probe vertical slice (README "概要"): mainnet BBO feed,
//! simplified sequencer and testnet gw on hft-net/hft-venue/venue-binance, seq and raw journals,
//! the offline report (hft-telemetry), the hft-net host probes and a network-free dry run. Probe
//! only; never linked by another crate and never in a production archive.
//! Specification: `tools/latency-slice/README.md`.

pub mod cli;
pub mod config;
pub mod dry;
pub mod hk;
pub mod journal;
pub mod live;
pub mod probes;
pub mod rd;
pub mod report;
pub mod run;
pub mod seq;
pub mod threads;

use std::path::PathBuf;

use cli::Mode;
use config::{CfgErr, Config};

/// Environment variable reader (`std::env::var`).
pub fn std_env(k: &str) -> Option<String> {
    std::env::var(k).ok()
}

/// Applies the command-line overrides (README "命令行").
pub fn apply_overrides(cfg: &mut Config, a: &cli::Args) {
    if let Some(m) = a.minutes {
        cfg.minutes = m;
    }
    if a.ts_expect_hw {
        cfg.ts_expect_hw = true;
    }
    if let Some(b) = a.ballast {
        cfg.ballast_conns = b;
    }
    if a.no_orders {
        cfg.orders_enabled = false;
    }
    if let Some(o) = &a.out {
        let default_cold = cfg.out_dir.join("journal");
        if cfg.cold_dir == default_cold {
            cfg.cold_dir = o.join("journal");
        }
        cfg.out_dir = o.clone();
    }
}

fn cfg_code(e: &CfgErr) -> i32 {
    match e {
        CfgErr::Io(_) | CfgErr::Invalid(_) | CfgErr::Key(_) | CfgErr::MainnetOrderHost(_) => 2,
    }
}

fn load(a: &cli::Args) -> Result<Config, i32> {
    match config::load(&a.config) {
        Ok(mut c) => {
            apply_overrides(&mut c, a);
            Ok(c)
        }
        Err(e) => {
            eprintln!("latency-slice: {e}");
            Err(cfg_code(&e))
        }
    }
}

/// The program (README "命令行"); returns the exit code.
pub fn main_with(args: &[String]) -> i32 {
    let a = match cli::parse(args, &std_env) {
        Ok(a) => a,
        Err(e) => {
            eprintln!("latency-slice: {e}");
            return 2;
        }
    };
    match a.mode.clone() {
        Mode::TwoPacket { n } => probes::two_packet(n),
        Mode::LoopbackStream { minutes } => probes::loopback_stream(minutes),
        Mode::Report { root } => report_only(&a, &root),
        Mode::CheckConfig => match load(&a) {
            Ok(c) => match config::load_keys(&c, &std_env) {
                Ok(_) => {
                    println!("config ok: product={:?} insts={}", c.product, c.insts.len());
                    0
                }
                Err(e) => {
                    eprintln!("latency-slice: {e}");
                    2
                }
            },
            Err(code) => code,
        },
        Mode::BuildOnly { n, print_napi } => match load(&a) {
            Ok(c) => probes::build_only(&c, n, print_napi),
            Err(code) => code,
        },
        Mode::IdlePolls { n } => match load(&a) {
            Ok(c) => probes::idle_polls(&c, n),
            Err(code) => code,
        },
        Mode::DryRun { seconds, seed } => match load(&a) {
            Ok(mut c) => {
                // A dry run never trades and needs no key.
                c.orders_enabled = true;
                finish(run::execute(&c, run::Kind::Dry { seconds, seed }))
            }
            Err(code) => code,
        },
        Mode::Run => match load(&a) {
            Ok(c) => match config::load_keys(&c, &std_env) {
                Ok(keys) => finish(run::execute(
                    &c,
                    run::Kind::Live {
                        keys,
                        print_conn_stats: a.print_conn_stats,
                    },
                )),
                Err(e) => {
                    eprintln!("latency-slice: {e}");
                    2
                }
            },
            Err(code) => code,
        },
    }
}

fn finish(o: run::Outcome) -> i32 {
    if let Some(r) = &o.report {
        print!("{}", report::text(r));
    }
    if o.code != 0 {
        eprintln!("latency-slice: {}", o.message);
    }
    o.code
}

fn report_only(a: &cli::Args, root: &std::path::Path) -> i32 {
    let (rc, out) = match config::load(&a.config) {
        Ok(c) => (
            report::ReportCfg {
                burst_gap_ns: c.burst_gap_ns,
                cal_max_err_ns: c.cal_max_err_ns,
            },
            a.out.clone().unwrap_or(c.out_dir),
        ),
        Err(_) => (
            report::ReportCfg {
                burst_gap_ns: 20_000,
                cal_max_err_ns: 1_000,
            },
            a.out.clone().unwrap_or_else(|| PathBuf::from("./slice-out")),
        ),
    };
    match report::build(&journal::no_hot(root), root, &rc, None) {
        Ok(v) => {
            if let Err(e) = report::write(&out, &v) {
                eprintln!("latency-slice: {e}");
                return 3;
            }
            print!("{}", report::text(&v));
            0
        }
        Err(e) => {
            eprintln!("latency-slice: report: {e}");
            3
        }
    }
}
