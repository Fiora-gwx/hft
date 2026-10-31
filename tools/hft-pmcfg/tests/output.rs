//! Generated configurations (TESTS.md PMC-004–PMC-006).
use std::path::{Path, PathBuf};
use std::process::Command;

use hft_pmcfg::{AccountRole, LiveAccount, PmLive};
use hft_refdata::RefData;
use hft_types::{AccountId, StrategyId, VenueId};

const FILES: [&str; 6] = [
    "core.toml",
    "principals.toml",
    "refdata.bin",
    "engine.bin",
    "risk_params.bin",
    "strategy_params.bin",
];

fn live(keys: &Path, extra: Vec<LiveAccount>) -> PmLive {
    PmLive {
        uid: "12345".into(),
        keys_dir: keys.join("k1"),
        day_loss_carry: None,
        daily_loss_abs: None,
        daily_loss_max_resumes: None,
        account_cap: None,
        group_cap: None,
        presign: true,
        extra_accounts: extra,
    }
}

fn labelled(label: &str, uid: &str, dir: PathBuf) -> LiveAccount {
    LiveAccount {
        label: Some(label.into()),
        uid: uid.into(),
        keys_dir: dir,
        role: AccountRole::Any,
    }
}

fn rd_of(dir: &Path) -> RefData {
    RefData::load(&std::fs::read(dir.join("refdata.bin")).unwrap(), None).unwrap()
}

#[test]
fn pmc_004_multi_account_live_output() {
    let d = tempfile::tempdir().unwrap();
    let k = d.path();
    let l = live(
        k,
        vec![
            LiveAccount::shorthand("777", k.join("k2")),
            labelled("acct-b", "888", k.join("k3")),
            labelled("acct-c", "999", k.join("k4")),
        ],
    );
    let out = d.path().join("out");
    let path = hft_pmcfg::write_polymarket_maker_live(
        &out,
        &d.path().join("hot"),
        None,
        None,
        "10.0.0.5",
        "ens5",
        &l,
    )
    .unwrap();
    assert_eq!(path, out.join("core.toml"), "PMC-004");
    let v = VenueId::POLYMARKET_CLOB;
    let ids = [1u8, 2, 3, 4].map(AccountId);
    // Refdata: accounts 1..4 in order, labels, Live and prod, bound to instance 1, one UID each.
    let rd = rd_of(&out);
    assert_eq!(rd.accounts_of(StrategyId(1)), &ids, "PMC-004.1");
    for (a, (label, uid)) in ids.iter().zip([
        ("pm-live", "12345"),
        ("pm-live-2", "777"),
        ("acct-b", "888"),
        ("acct-c", "999"),
    ]) {
        let m = rd.account(*a);
        assert_eq!(&*m.label, label, "PMC-004.1");
        assert!(
            m.is_prod && m.bound_instance == StrategyId(1),
            "PMC-004.1 {label}"
        );
        assert!(
            matches!(m.kind, hft_refdata::AccountKind::Live),
            "PMC-004.1 {label}"
        );
        assert_eq!(rd.venue_uid(*a, v), Some(uid), "PMC-004.1");
    }
    // core.toml: one key entry per account with that account's directory.
    let cfg: toml::Value = toml::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    let keys = cfg["venues"]["pm"]["accounts"].as_array().unwrap();
    assert_eq!(keys.len(), 4, "PMC-004.2");
    for (i, (key, dir)) in keys.iter().zip(["k1", "k2", "k3", "k4"]).enumerate() {
        assert_eq!(key["account"].as_integer(), Some(i as i64 + 1), "PMC-004.2");
        for f in ["api_key_path", "rest_secret_path", "passphrase_path"] {
            let p = PathBuf::from(key[f].as_str().unwrap());
            assert!(p.starts_with(k.join(dir)), "PMC-004.2 {dir} {f}: {}", p.display());
        }
    }
    // Engine: keys required for every account; risk: account and venue limits for every account.
    let setup = hft_sequencer::EngineSetup::decode_blob(
        &std::fs::read(out.join("engine.bin")).unwrap(),
        &rd,
        hft_sequencer::SegClock {
            tsc_hz_milli: 1_000_000_000_000,
            calib_tsc: 0,
            calib_realtime_ns: 0,
        },
        1,
        0,
    )
    .unwrap();
    assert_eq!(setup.keys_required, ids.map(|a| (a, v, 3)), "PMC-004.3");
    let risk =
        hft_risk::RiskParams::decode_blob(&std::fs::read(out.join("risk_params.bin")).unwrap(), &rd).unwrap();
    assert_eq!(risk.accounts.as_slice().len(), 4, "PMC-004.3");
    assert_eq!(risk.venues.as_slice().len(), 4, "PMC-004.3");
    // .4 the same accounts as `--live-account` only: the labelled ones differ in label alone.
    let short = d.path().join("short");
    let l2 = live(
        k,
        vec![
            LiveAccount::shorthand("777", k.join("k2")),
            LiveAccount::shorthand("888", k.join("k3")),
            LiveAccount::shorthand("999", k.join("k4")),
        ],
    );
    hft_pmcfg::write_polymarket_maker_live(
        &short,
        &d.path().join("hot"),
        None,
        None,
        "10.0.0.5",
        "ens5",
        &l2,
    )
    .unwrap();
    let rd2 = rd_of(&short);
    assert_eq!(&*rd2.account(AccountId(3)).label, "pm-live-3", "PMC-004.4");
    assert_eq!(&*rd2.account(AccountId(4)).label, "pm-live-4", "PMC-004.4");
    for f in ["engine.bin", "risk_params.bin", "strategy_params.bin"] {
        assert_eq!(
            std::fs::read(out.join(f)).unwrap(),
            std::fs::read(short.join(f)).unwrap(),
            "PMC-004.4 {f}"
        );
    }
}

/// Runs the tool; (exit code, stdout, stderr).
fn tool(args: &[&str]) -> (Option<i32>, String, String) {
    let o = Command::new(env!("CARGO_BIN_EXE_hft-pmcfg"))
        .args(args)
        .output()
        .unwrap();
    (
        o.status.code(),
        String::from_utf8_lossy(&o.stdout).into_owned(),
        String::from_utf8_lossy(&o.stderr).into_owned(),
    )
}

#[test]
fn pmc_005_tool_output_equals_library() {
    let d = tempfile::tempdir().unwrap();
    let p = |s: &str| d.path().join(s);
    let s = |p: &Path| p.display().to_string();
    let hot = p("hot");
    let carry = PmLive {
        presign: false,
        daily_loss_abs: Some(20_000_000),
        daily_loss_max_resumes: Some(3),
        account_cap: None,
        group_cap: None,
        day_loss_carry: Some((20_755, -3_500_000)),
        ..live(d.path(), vec![])
    };
    let multi = live(
        d.path(),
        vec![
            LiveAccount::shorthand("777", p("k2")),
            labelled("acct-b", "888", p("k3")),
        ],
    );
    let params = vec![("clip_shares".to_string(), 7)];
    let probe_params = vec![("spend_usdc".to_string(), 1_500_000)];
    let probe = live(d.path(), vec![]);
    type Lib<'a> = Box<dyn Fn(&Path) -> Result<PathBuf, String> + 'a>;
    let rows: Vec<(&str, Vec<String>, Lib<'_>)> = vec![
        (
            "PMC-005.1",
            vec![],
            Box::new(|o: &Path| hft_pmcfg::write_polymarket_sim(o, &hot, None, None, "10.0.0.5", "ens5")),
        ),
        (
            "PMC-005.2",
            ["--maker", "--param", "clip_shares=7"].map(String::from).to_vec(),
            Box::new(|o: &Path| {
                hft_pmcfg::write_polymarket_maker_sim_with(o, &hot, None, None, "10.0.0.5", "ens5", &params)
            }),
        ),
        (
            "PMC-005.3",
            vec![
                "--live".into(),
                "12345".into(),
                s(&p("k1")),
                "--no-presign".into(),
                "--daily-loss".into(),
                "20".into(),
                "--daily-loss-resumes".into(),
                "3".into(),
                "--day-loss-carry".into(),
                "2026-10-29".into(),
                "-3.5".into(),
            ],
            Box::new(|o: &Path| {
                hft_pmcfg::write_polymarket_maker_live(o, &hot, None, None, "10.0.0.5", "ens5", &carry)
            }),
        ),
        (
            "PMC-005.4",
            vec![
                "--live".into(),
                "12345".into(),
                s(&p("k1")),
                "--live-account".into(),
                "777".into(),
                s(&p("k2")),
                "--account".into(),
                "acct-b:888:any".into(),
                s(&p("k3")),
                "--param".into(),
                "clip_shares=7".into(),
            ],
            Box::new(|o: &Path| {
                hft_pmcfg::write_polymarket_maker_live_with(
                    o, &hot, None, None, "10.0.0.5", "ens5", &multi, &params,
                )
            }),
        ),
        (
            "PMC-005.6",
            vec![
                "--live".into(),
                "12345".into(),
                s(&p("k1")),
                "--taker-probe".into(),
                "--param".into(),
                "spend_usdc=1500000".into(),
            ],
            Box::new(|o: &Path| {
                hft_pmcfg::write_polymarket_taker_probe_live(
                    o,
                    &hot,
                    None,
                    None,
                    "10.0.0.5",
                    "ens5",
                    &probe,
                    &probe_params,
                )
            }),
        ),
    ];
    for (i, (row, flags, lib)) in rows.iter().enumerate() {
        let (t, l) = (p(&format!("tool{i}")), p(&format!("lib{i}")));
        let mut a: Vec<String> = flags.clone();
        a.extend([s(&t), s(&hot), "10.0.0.5".into(), "ens5".into()]);
        let (code, out, err) = tool(&a.iter().map(String::as_str).collect::<Vec<_>>());
        assert_eq!(code, Some(0), "{row}: {err}");
        assert_eq!(out.trim_end(), s(&t.join("core.toml")), "{row}");
        lib(&l).unwrap();
        for f in FILES {
            assert_eq!(
                std::fs::read(t.join(f)).unwrap(),
                std::fs::read(l.join(f)).unwrap(),
                "{row} {f}"
            );
        }
    }
    // .5 a refused command line: exit 2, the message and the usage line on stderr, nothing written.
    let bad = p("bad");
    let (code, out, err) = tool(&["--no-presign", &s(&bad), &s(&hot), "10.0.0.5", "ens5"]);
    assert_eq!(code, Some(2), "PMC-005.5");
    assert!(out.is_empty(), "PMC-005.5");
    assert!(
        err.contains("--no-presign needs --live") && err.contains("usage: hft-pmcfg"),
        "PMC-005.5: {err}"
    );
    assert!(!bad.exists(), "PMC-005.5");
}

#[test]
fn pmc_006_engine_base_equals_seq_rig_setup() {
    use hft_testkit::seq_rig::{RigConfig, engine_setup};
    let d = tempfile::tempdir().unwrap();
    let hot = d.path().join("hot");
    let sim = d.path().join("sim");
    hft_pmcfg::write_polymarket_sim(&sim, &hot, None, None, "10.0.0.5", "ens5").unwrap();
    let multi = d.path().join("multi");
    let l = live(
        d.path(),
        vec![
            LiveAccount::shorthand("777", d.path().join("k2")),
            labelled("acct-b", "888", d.path().join("k3")),
        ],
    );
    hft_pmcfg::write_polymarket_maker_live(&multi, &hot, None, None, "10.0.0.5", "ens5", &l).unwrap();
    let caps = hft_ledger::Capacities {
        max_insts: 16,
        max_cells_total: 32,
        max_timers: 1024,
        recent_cmd_ids: 256,
        max_subs_per_inst: 4,
        max_groups_per_instance: 16,
        max_instances: 4,
    };
    let (pm, bn) = (VenueId::POLYMARKET_CLOB, VenueId::BINANCE_SPOT);
    for (row, dir, venues) in [("PMC-006.1", &sim, vec![pm]), ("PMC-006.2", &multi, vec![pm, bn])] {
        let rdb = std::fs::read(dir.join("refdata.bin")).unwrap();
        let rd = RefData::load(&rdb, None).unwrap();
        let mut ledger = hft_ledger::LedgerConfig::default();
        ledger.par_assets.push(hft_ledger::AssetName::new("USDC"));
        let risk = hft_risk::ParamsBuilder::new(&rd).build();
        let rig = RigConfig {
            refdata_blob: rdb,
            params: vec![],
            risk,
            risk_blob: vec![],
            caps,
            ledger: ledger.clone(),
            venues: venues.clone(),
            keys: vec![],
            epoch: 1,
            tsc_hz_milli: 1_000_000_000_000,
            calib_tsc: 0,
            calib_utc_ns: 0,
            boot_tsc: 0,
            max_groups: 16,
        };
        let want = engine_setup(&rig, &rd).encode_blob(1, hft_types::HostClass::Dev2, 0);
        let got =
            hft_pmcfg::engine_base(&rd, &caps, ledger, venues).encode_blob(1, hft_types::HostClass::Dev2, 0);
        assert_eq!(got, want, "{row}");
    }
}

#[test]
fn pmc_007_taker_probe_live_output() {
    use hft_strategy_sdk::StrategySet;
    let d = tempfile::tempdir().unwrap();
    let hot = d.path().join("hot");
    let l = live(d.path(), vec![LiveAccount::shorthand("777", d.path().join("k2"))]);
    let (probe, maker) = (d.path().join("probe"), d.path().join("maker"));
    let params = vec![("spend_usdc".to_string(), 1_500_000)];
    hft_pmcfg::write_polymarket_taker_probe_live(&probe, &hot, None, None, "10.0.0.5", "ens5", &l, &params)
        .unwrap();
    hft_pmcfg::write_polymarket_maker_live(&maker, &hot, None, None, "10.0.0.5", "ens5", &l).unwrap();
    // .1 the same core.toml but the blob digests, the same engine; same accounts and slots.
    let strip = |dir: &Path| -> Vec<String> {
        std::fs::read_to_string(dir.join("core.toml"))
            .unwrap()
            .lines()
            .filter(|x| !x.starts_with("sha256"))
            .map(str::to_string)
            .collect()
    };
    assert_eq!(strip(&probe), strip(&maker), "PMC-007.1");
    assert_eq!(
        std::fs::read(probe.join("engine.bin")).unwrap(),
        std::fs::read(maker.join("engine.bin")).unwrap(),
        "PMC-007.1"
    );
    let (rp, rm) = (rd_of(&probe), rd_of(&maker));
    assert_eq!(rp.inst_count(), rm.inst_count(), "PMC-007.1");
    assert_eq!(
        rp.accounts_of(StrategyId(1)),
        rm.accounts_of(StrategyId(1)),
        "PMC-007.1"
    );
    // .2 the instance is pm-taker-probe (kind 8).
    let i = rp.instances_in_order();
    assert_eq!(
        (i.len(), i[0].kind_id, &*i[0].name),
        (1, 8, "pm-taker-probe"),
        "PMC-007.2"
    );
    // .3 the parameter blob: kind 8, one group (slot InstId 1, open allowed), spend 1.5, enabled 0.
    let blob = std::fs::read(probe.join("strategy_params.bin")).unwrap();
    let spb = hft_event::parse_spb1(&blob).unwrap();
    let (h, body) = (spb.hdr, spb.body);
    assert_eq!(
        (h.kind_id, h.n_groups, h.account),
        (8, 1, AccountId(1)),
        "PMC-007.3"
    );
    assert_eq!(spb.groups.len(), 1, "PMC-007.3");
    let g = spb.groups[0];
    assert_eq!(
        (g.n_legs, g.open_allowed, g.legs[0].inst, g.legs[0].venue),
        (1, 1, hft_types::InstId(1), VenueId::POLYMARKET_CLOB),
        "PMC-007.3"
    );
    let desc = hft_strategy_registry::Strategies::kinds()
        .iter()
        .find(|k| k.kind_id == 8)
        .unwrap();
    let raw = hft_strategy_sdk::params::decode_body(desc.schema, h.schema_ver, 1, body).unwrap();
    assert_eq!(raw.defaults, vec![0, 1_500_000, 1, 0, 0], "PMC-007.3");
    // .4 risk: the maker's limits with rr_max_cross 0.5 in the price band.
    let risk = |dir: &Path, rd: &RefData| {
        hft_risk::RiskParams::decode_blob(&std::fs::read(dir.join("risk_params.bin")).unwrap(), rd).unwrap()
    };
    let (kp, km) = (risk(&probe, &rp), risk(&maker, &rm));
    let rr = |k: &hft_risk::RiskParams| k.classes.iter().map(|c| c.rr_max_cross.0).collect::<Vec<_>>();
    assert!(
        rr(&kp).contains(&hft_pmcfg::PM_PROBE_RR_MAX_CROSS),
        "PMC-007.4 {:?}",
        rr(&kp)
    );
    assert!(!rr(&km).contains(&hft_pmcfg::PM_PROBE_RR_MAX_CROSS), "PMC-007.4");
    assert_eq!(kp.accounts.as_slice(), km.accounts.as_slice(), "PMC-007.4");
    assert_eq!(kp.venues.as_slice(), km.venues.as_slice(), "PMC-007.4");
    // .5 a parameter of another strategy, or without a live account, is refused.
    let bad = vec![("clip_shares".to_string(), 5)];
    let e = hft_pmcfg::write_polymarket_taker_probe_live(
        &d.path().join("x"),
        &hot,
        None,
        None,
        "1",
        "lo",
        &l,
        &bad,
    )
    .unwrap_err();
    assert!(e.contains("not a pm-taker-probe parameter"), "PMC-007.5 {e}");
}
