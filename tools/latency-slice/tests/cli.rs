//! SLC-001: command line.

use std::path::PathBuf;

use latency_slice::cli::{Mode, parse};

fn p(args: &str, env: Option<&str>) -> Result<latency_slice::cli::Args, latency_slice::cli::ArgErr> {
    let v: Vec<String> = args.split_whitespace().map(String::from).collect();
    let e = env.map(String::from);
    parse(&v, &move |k: &str| {
        if k == "HFT_SLICE_CONFIG" { e.clone() } else { None }
    })
}

#[test]
fn slc_001_cli_parse() {
    let a = p("", None).unwrap();
    assert_eq!(a.mode, Mode::Run, "SLC-001.1");
    assert_eq!(a.config, PathBuf::from("latency-slice.toml"), "SLC-001.1");
    assert_eq!(
        (a.minutes, a.ballast, a.no_orders, a.out.clone()),
        (None, None, false, None),
        "SLC-001.1"
    );

    let a = p("--minutes 60 --ts-expect-hw --print-conn-stats", None).unwrap();
    assert_eq!(
        (a.mode.clone(), a.minutes, a.ts_expect_hw, a.print_conn_stats),
        (Mode::Run, Some(60), true, true),
        "SLC-001.2"
    );

    let a = p("--config a.toml --ballast 0 --no-orders --out d", None).unwrap();
    assert_eq!(
        (a.config.clone(), a.ballast, a.no_orders, a.out.clone()),
        (PathBuf::from("a.toml"), Some(0), true, Some(PathBuf::from("d"))),
        "SLC-001.3"
    );
    assert_eq!(
        p("--dry-run --seconds 3 --seed 7", None).unwrap().mode,
        Mode::DryRun { seconds: 3, seed: 7 },
        "SLC-001.4"
    );
    assert_eq!(
        p("--report r", None).unwrap().mode,
        Mode::Report {
            root: PathBuf::from("r")
        },
        "SLC-001.5"
    );
    assert_eq!(
        p("--build-only 20 --print-napi", None).unwrap().mode,
        Mode::BuildOnly {
            n: 20,
            print_napi: true
        },
        "SLC-001.6"
    );
    assert_eq!(
        p("--idle-polls 1000000", None).unwrap().mode,
        Mode::IdlePolls { n: 1_000_000 },
        "SLC-001.7"
    );
    assert_eq!(
        p("--two-packet-stamp-probe 1000", None).unwrap().mode,
        Mode::TwoPacket { n: 1000 },
        "SLC-001.8"
    );
    assert_eq!(
        p("--loopback-stream-minutes 10", None).unwrap().mode,
        Mode::LoopbackStream { minutes: 10 },
        "SLC-001.9"
    );
    for (row, args) in [
        ("SLC-001.10", "--minutes"),
        ("SLC-001.11", "--minutes x"),
        ("SLC-001.12", "--bogus"),
        ("SLC-001.13", "--dry-run --report r"),
    ] {
        assert!(p(args, None).is_err(), "{row}");
        // The binary maps every argument error to exit code 2.
        let v: Vec<String> = args.split_whitespace().map(String::from).collect();
        assert_eq!(latency_slice::main_with(&v), 2, "{row}");
    }
    assert_eq!(
        p("", Some("e.toml")).unwrap().config,
        PathBuf::from("e.toml"),
        "SLC-001.14"
    );
}
