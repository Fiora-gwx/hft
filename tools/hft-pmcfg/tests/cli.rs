//! Command-line parsing (TESTS.md PMC-001–PMC-003).
use std::path::PathBuf;

use hft_pmcfg::cli::{Mode, parse};
use hft_pmcfg::{AccountRole, LiveAccount};

fn args(s: &str) -> Vec<String> {
    s.split_whitespace().map(str::to_string).collect()
}

const POS: &str = "out hot 10.0.0.5 ens5";

#[test]
fn pmc_001_valid_command_lines() {
    // .1 no mode flag: the book-observer sim; four positional arguments.
    let c = parse(&args(POS)).unwrap();
    assert!(matches!(c.mode, Mode::Sim), "PMC-001.1");
    assert_eq!(
        (
            c.root.clone(),
            c.hot.clone(),
            c.src_ip.as_str(),
            c.bind_device.as_str(),
            c.user.clone(),
            c.pipeline.clone()
        ),
        (
            PathBuf::from("out"),
            PathBuf::from("hot"),
            "10.0.0.5",
            "ens5",
            None,
            None
        ),
        "PMC-001.1"
    );
    // .2 --maker with USER and PIPELINE_USER, --param anywhere (raw values, in order).
    let c = parse(&args(&format!(
        "--param clip_shares=7 --maker {POS} alice pmpipe --param inv_max_shares=20"
    )))
    .unwrap();
    assert!(matches!(c.mode, Mode::Maker), "PMC-001.2");
    assert_eq!(
        (c.user.as_deref(), c.pipeline.as_deref()),
        (Some("alice"), Some("pmpipe")),
        "PMC-001.2"
    );
    assert_eq!(
        c.params,
        [("clip_shares".to_string(), 7), ("inv_max_shares".to_string(), 20)],
        "PMC-001.2"
    );
    // .3 --live with every live option.
    let c = parse(&args(&format!(
        "--live 12345 k1 --no-presign --daily-loss 20.5 --daily-loss-resumes 3 \
         --day-loss-carry 2026-10-29 -3.5 {POS}"
    )))
    .unwrap();
    let Mode::Live(l) = c.mode else {
        panic!("PMC-001.3")
    };
    assert_eq!(
        (l.uid.as_str(), l.keys_dir),
        ("12345", PathBuf::from("k1")),
        "PMC-001.3"
    );
    assert!(!l.presign, "PMC-001.3");
    assert_eq!(l.daily_loss_abs, Some(20_500_000), "PMC-001.3");
    assert_eq!(l.daily_loss_max_resumes, Some(3), "PMC-001.3");
    // 2026-10-29 = day 20 755 since 1970-01-01.
    assert_eq!(l.day_loss_carry, Some((20_755, -3_500_000)), "PMC-001.3");
    assert!(l.extra_accounts.is_empty(), "PMC-001.3");
    // .4 --live defaults: presign on, no carry, default limits.
    let Mode::Live(l) = parse(&args(&format!("--live 12345 k1 {POS}"))).unwrap().mode else {
        panic!("PMC-001.4")
    };
    assert!(l.presign, "PMC-001.4");
    assert_eq!(
        (l.day_loss_carry, l.daily_loss_abs, l.daily_loss_max_resumes),
        (None, None, None),
        "PMC-001.4"
    );
    // .5 --live-account and --account keep command-line order as accounts 2, 3, 4.
    let Mode::Live(l) = parse(&args(&format!(
        "--account acct-b:888:any k3 --live 12345 k1 --live-account 777 k2 --account acct-c:999:any k4 {POS}"
    )))
    .unwrap()
    .mode
    else {
        panic!("PMC-001.5")
    };
    assert_eq!(
        l.extra_accounts,
        [
            LiveAccount {
                label: Some("acct-b".into()),
                uid: "888".into(),
                keys_dir: "k3".into(),
                role: AccountRole::Any
            },
            LiveAccount::shorthand("777", "k2"),
            LiveAccount {
                label: Some("acct-c".into()),
                uid: "999".into(),
                keys_dir: "k4".into(),
                role: AccountRole::Any
            },
        ],
        "PMC-001.5"
    );
    assert_eq!(
        l.labels(),
        ["pm-live", "acct-b", "pm-live-3", "acct-c"],
        "PMC-001.5"
    );
    // .6 --taker-probe: the live configuration with pm-taker-probe; --param of its schema.
    let c = parse(&args(&format!(
        "--taker-probe --live 12345 k1 --param spend_usdc=1500000 {POS}"
    )))
    .unwrap();
    let Mode::LiveProbe(l) = c.mode else {
        panic!("PMC-001.6")
    };
    assert_eq!((l.uid.as_str(), l.presign), ("12345", true), "PMC-001.6");
    assert_eq!(c.params, [("spend_usdc".to_string(), 1_500_000)], "PMC-001.6");
}

#[test]
fn pmc_002_invalid_command_lines() {
    let live = format!("--live 12345 k1 {POS}");
    let rows: [(&str, String, &str); 24] = [
        ("PMC-002.1", "out hot 10.0.0.5".into(), "expected OUTPUT_DIR"),
        (
            "PMC-002.2",
            format!("{POS} alice pmpipe extra"),
            "got 7 arguments",
        ),
        ("PMC-002.3", format!("--maker --live 1 k1 {POS}"), "exclusive"),
        (
            "PMC-002.4",
            format!("--param clip_shares=7 {POS}"),
            "--param needs --maker or --live",
        ),
        (
            "PMC-002.5",
            format!("--no-presign {POS}"),
            "--no-presign needs --live",
        ),
        ("PMC-002.6", format!("--live-account 7 k2 {POS}"), "needs --live"),
        (
            "PMC-002.7",
            format!("--daily-loss 20 {POS}"),
            "--daily-loss needs --live",
        ),
        ("PMC-002.8", format!("--daily-loss 0 {live}"), "> 0"),
        (
            "PMC-002.9",
            format!("--daily-loss 1.0000001 {live}"),
            "at most 6 places",
        ),
        ("PMC-002.10", format!("--daily-loss-resumes 21 {live}"), "0..=20"),
        (
            "PMC-002.11",
            format!("--day-loss-carry 2026-10-29 0 {live}"),
            "< 0",
        ),
        (
            "PMC-002.12",
            format!("--day-loss-carry 2026-13-01 -1 {live}"),
            "bad date",
        ),
        (
            "PMC-002.13",
            format!("--maker --param clip_shares {POS}"),
            "expected NAME=RAW",
        ),
        (
            "PMC-002.14",
            format!("--maker --param clip_shares=0.5 {POS}"),
            "integer",
        ),
        (
            "PMC-002.15",
            format!("--maker --param no_such_field=1 {POS}"),
            "not a binary-maker parameter",
        ),
        (
            "PMC-002.16",
            format!("--maker --param clip_shares=0 {POS}"),
            "outside",
        ),
        (
            "PMC-002.17",
            format!("--frobnicate {POS}"),
            "unknown option --frobnicate",
        ),
        (
            "PMC-002.18",
            format!("--daily-loss 5 --daily-loss 6 {live}"),
            "given twice",
        ),
        ("PMC-002.19", "--live 12345".into(), "--live: missing KEYS_DIR"),
        (
            "PMC-002.20",
            format!("--day-loss-carry 2026-10-29 -1 {POS}"),
            "--day-loss-carry needs --live",
        ),
        (
            "PMC-002.21",
            format!("--taker-probe {POS}"),
            "--taker-probe needs --live",
        ),
        (
            "PMC-002.22",
            format!("--taker-probe --param clip_shares=5 {live}"),
            "not a pm-taker-probe parameter",
        ),
        (
            "PMC-002.23",
            format!("--taker-probe --param spend_usdc=2000001 {live}"),
            "outside",
        ),
        (
            "PMC-002.24",
            format!("--taker-probe --taker-probe {live}"),
            "given twice",
        ),
    ];
    for (row, line, want) in rows {
        match parse(&args(&line)) {
            Ok(c) => panic!("{row}: accepted {c:?}"),
            Err(e) => assert!(e.contains(want), "{row}: {e}"),
        }
    }
}

#[test]
fn pmc_003_account_entries() {
    let live = |extra: &str| args(&format!("--live 12345 k1 {extra} {POS}"));
    let err = |extra: &str| parse(&live(extra)).unwrap_err();
    // .1 the label is before the first `:`, the role after the last.
    let Mode::Live(l) = parse(&live("--account a.b_c-1:0f5e:8d2a:any k2")).unwrap().mode else {
        panic!("PMC-003.1")
    };
    assert_eq!(
        (
            l.extra_accounts[0].label.as_deref(),
            l.extra_accounts[0].uid.as_str(),
            l.extra_accounts[0].role
        ),
        (Some("a.b_c-1"), "0f5e:8d2a", AccountRole::Any),
        "PMC-003.1"
    );
    // .2 malformed entries.
    for (row, e) in [
        ("PMC-003.2", "--account acct k2"),
        ("PMC-003.2", "--account acct:888 k2"),
        ("PMC-003.2", "--account :888:any k2"),
        ("PMC-003.2", "--account acct::any k2"),
    ] {
        assert!(err(e).contains("expected LABEL:UID:ROLE"), "{row} {e}");
    }
    // .3 KEYS_DIR missing.
    assert!(
        parse(&args("--live 12345 k1 --account acct:888:any")).is_err(),
        "PMC-003.3"
    );
    // .4 roles other than `any`.
    for role in ["maker", "taker", "up", "down", "standby", "ANY", ""] {
        let e = err(&format!("--account acct:888:{role} k2"));
        assert!(e.contains("only `any` is supported"), "PMC-003.4 {role}: {e}");
    }
    // .5 a Sim-style label, a label outside [A-Za-z0-9._-].
    assert!(err("--account sim-x:888:any k2").contains("sim-"), "PMC-003.5");
    assert!(
        err("--account a/b:888:any k2").contains("[A-Za-z0-9._-]"),
        "PMC-003.5"
    );
    // .6 duplicate labels (also the generated `pm-live`, `pm-live-<id>`), duplicate UIDs.
    for (row, e, want) in [
        ("PMC-003.6", "--account pm-live:888:any k2", "used twice"),
        (
            "PMC-003.6",
            "--account pm-live-3:888:any k2 --live-account 777 k3",
            "used twice",
        ),
        (
            "PMC-003.6",
            "--account a:888:any k2 --account a:999:any k3",
            "used twice",
        ),
        ("PMC-003.6", "--account a:12345:any k2", "used by two accounts"),
        (
            "PMC-003.6",
            "--live-account 777 k2 --account a:777:any k3",
            "used by two accounts",
        ),
    ] {
        let got = err(e);
        assert!(got.contains(want), "{row} {e}: {got}");
    }
    // .7 eleven further accounts (12 per venue) are accepted, twelve are refused.
    let n = |k: usize| {
        (0..k)
            .map(|i| {
                if i % 2 == 0 {
                    format!("--live-account u{i} k{i}")
                } else {
                    format!("--account a{i}:u{i}:any k{i}")
                }
            })
            .collect::<Vec<_>>()
            .join(" ")
    };
    let Mode::Live(l) = parse(&live(&n(11))).unwrap().mode else {
        panic!("PMC-003.7")
    };
    assert_eq!(l.extra_accounts.len(), 11, "PMC-003.7");
    assert!(err(&n(12)).contains("at most 12 accounts"), "PMC-003.7");
}
