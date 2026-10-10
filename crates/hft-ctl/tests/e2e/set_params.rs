//! CTL-201.

use hft_strategy_sdk::{GroupTable, StrategySet};

use super::*;
use crate::common::b64;

const A: Option<&str> = Some("acct-a");

/// The `SetParams` command of a blob.
pub fn sp(p: &Spb, base: Option<&str>) -> Value {
    let strategy = match p.strategy {
        x if x == S1 => "s1-main",
        x if x == S2 => "s2-main",
        _ => "s9-sim",
    };
    let mut v = json!({"type":"SetParams","strategy":strategy,"blob_b64":b64(&p.bytes())});
    if let Some(b) = base {
        v["base_version"] = json!(b);
    }
    v
}

fn s1(version: u64) -> Spb {
    Spb::s1(version)
}

/// `s1-main` version 42 with `thr` changed.
fn thr42() -> Spb {
    let mut p = s1(42);
    p.values[tf::THR] = 7;
    p
}

/// `s1-main` version 42 with `win` changed.
fn win42() -> Spb {
    let mut p = s1(42);
    p.values[tf::WIN] = 61;
    p
}

fn decoded(p: &Spb) -> (<Set as StrategySet>::Params, GroupTable) {
    let g = GroupTable::new(&p.groups, 8);
    let parsed = p.parsed(&rd());
    (parsed.params, g)
}

fn drain() -> Value {
    json!({"type":"Drain","cancel_resting":false})
}

#[test]
fn ctl_201_set_params_codes() {
    // .1 thr only, no confirmation.
    let mut r = e2e();
    let x = r.send("cli:tom", A, sp(&thr42(), None));
    want(&x, "Applied", None, "CTL-201.1");
    assert_eq!(r.jres(x.cmd_id).map(|c| c.params_version), Some(42), "CTL-201.1");
    assert_eq!(r.params_version(S1), 42, "CTL-201.1");

    // .2 version 41 (= current).
    let mut r = e2e();
    let mut p = thr42();
    p.version = 41;
    want(
        &r.send("cli:tom", A, sp(&p, None)),
        "StaleVersion",
        None,
        "CTL-201.2",
    );

    // .3 base_version 40.
    let mut r = e2e();
    want(
        &r.send("cli:tom", A, sp(&thr42(), Some("40"))),
        "VersionConflict",
        None,
        "CTL-201.3",
    );

    // .4 structural `win` on a Live instance: needs confirmation; the preview carries the SDK
    // estimate.
    let mut r = e2e();
    let x = r.send("cli:tom", A, sp(&win42(), None));
    want(&x, "NeedsConfirm", None, "CTL-201.4");
    let (op, og) = decoded(&s1(41));
    let (np, ng) = decoded(&win42());
    let est = Set::reset_preview(&op, &og, &np, &ng);
    assert!(est.reset_groups > 0, "CTL-201.4: win is structural");
    let p = r.preview("cli:tom", A, sp(&win42(), None));
    let pv = rflags(&p).expect("CTL-201.4 preview block");
    assert_eq!(
        pv["reset_groups"],
        json!(est.reset_groups.to_string()),
        "CTL-201.4 {pv}"
    );
    assert_eq!(
        pv["no_open_ms"],
        json!(est.no_open_ms.to_string()),
        "CTL-201.4 {pv}"
    );
    assert_eq!(r.params_version(S1), 41, "CTL-201.4");

    // .5 every s1-main cell desired Shadow: not a Live instance; pipeline changes `win`.
    let mut r = e2e();
    let x = r.send(
        "cli:tom",
        A,
        json!({"type":"SetMode","strategy":"s1-main","mode":"Shadow"}),
    );
    want(&x, "Applied", None, "CTL-201.5 shadow");
    want(
        &r.send("pipeline", A, sp(&win42(), None)),
        "Applied",
        None,
        "CTL-201.5",
    );

    // .6 pipeline changes `win` of (s9-sim, sim-a): a Sim account's instance is not Live.
    let mut r = e2e();
    let mut p9 = Spb::s9(4);
    p9.values[tf::WIN] = 61;
    want(
        &r.send("pipeline", Some("sim-a"), sp(&p9, None)),
        "Applied",
        None,
        "CTL-201.6",
    );

    // .7 removing g1 while it holds a position.
    let mut r = e2e();
    r.position_g1();
    assert_ne!(r.leg_net(ACCT_A, G1, 1), 0, "CTL-201.7 position");
    let mut p = s1(42);
    p.groups.clear();
    let x = r.send("cli:tom", A, sp(&p, None));
    want(&x, "PreconditionFailed", Some("GroupHasPosition"), "CTL-201.7");
    assert_eq!(r.params_version(S1), 41, "CTL-201.7");

    // .8 a second non-Shadow group on (acct-a, binance-usdm:BTCUSDT).
    let mut r = e2e();
    let mut p = s1(42);
    p.groups.push(g_target(GroupId((1 << 24) | 102)));
    let x = r.send("pipeline", A, sp(&p, None));
    want(&x, "InvalidArgs", Some("InstOwned"), "CTL-201.8");

    // .10 a group whose trading legs' min_notional (5 USDT) is above acct-a's dust line (4 USDT):
    // g1 replaced by a new group on the same legs.
    let mut r = E2e::with(Opts {
        setup: Some(|s| {
            for a in s.exec.accounts.iter_mut() {
                if a.account == ACCT_A
                    && let Some(p) = a.params.as_mut()
                {
                    p.dust_notional = hft_types::Notional(4_000_000);
                }
            }
        }),
        ..Opts::default()
    });
    let mut p = s1(42);
    p.groups = vec![g_target(GroupId((1 << 24) | 102))];
    let x = r.send("pipeline", A, sp(&p, None));
    want(&x, "InvalidArgs", Some("DustBelowMinNotional"), "CTL-201.10");

    // .11 the sum of model_mem over the instances exceeds 256 MiB (one instance alone does not).
    let mut o = Opts::default();
    o.params[2].values[tf::MODEL_MEM_KIB] = 200 * 1024;
    let mut r = E2e::with(o);
    let mut p = thr42();
    p.values[tf::MODEL_MEM_KIB] = 100 * 1024;
    let x = r.send("cli:tom", A, sp(&p, None));
    want(&x, "InvalidArgs", Some("ParamInvalid"), "CTL-201.11");
    assert_eq!(arg(&x), "ModelMemBudget", "CTL-201.11");
    assert_eq!(r.params_version(S1), 41, "CTL-201.11");

    // .12 s1-main's cells Shadow (the new groups take Shadow: no InstOwned); three new groups on
    // binance-usdm:BTCUSDT make 5 references (> max_subs_per_inst = 4).
    let mut r = e2e();
    r.send(
        "cli:tom",
        A,
        json!({"type":"SetMode","strategy":"s1-main","mode":"Shadow"}),
    );
    let mut p = s1(42);
    for k in 102..105u32 {
        p.groups.push(g_target(GroupId((1 << 24) | k)));
    }
    want(
        &r.send("pipeline", A, sp(&p, None)),
        "CapacityExceeded",
        None,
        "CTL-201.12",
    );

    // .14 Draining: a human adds an `open_allowed = 1` group (upgrade), two-step.
    let mut r = e2e();
    r.hold_drain();
    want(
        &r.send("cli:tom", Some("*"), drain()),
        "Applied",
        None,
        "CTL-201.14 drain",
    );
    let mut p = s1(42);
    p.groups.push(g_target(GroupId((1 << 24) | 102)));
    let (pv, c) = r.two_step("cli:tom", A, sp(&p, None));
    want(&pv, "PreconditionFailed", Some("Draining"), "CTL-201.14 preview");
    want(&c, "PreconditionFailed", Some("Draining"), "CTL-201.14");

    // .15 Draining: 201.1.
    let mut r = e2e();
    r.hold_drain();
    want(
        &r.send("cli:tom", Some("*"), drain()),
        "Applied",
        None,
        "CTL-201.15 drain",
    );
    want(
        &r.send("cli:tom", A, sp(&thr42(), None)),
        "Applied",
        None,
        "CTL-201.15",
    );
}
