//! CTL-211.

use hft_types::CapSource;

use super::*;
use crate::common::{ASSEMBLY_FAIL, b64};

const A: Option<&str> = Some("acct-a");
const ALL: Option<&str> = Some("*");

/// acct-a's two resting orders: a smoke order on each of its venues.
fn resting_a(r: &mut E2e) {
    r.smoke_direct(ACCT_A, I_BG_SPOT);
    r.smoke_direct(ACCT_A, I_BN_USDM);
    assert_eq!(
        r.resting.iter().filter(|d| d.account == ACCT_A).count(),
        2,
        "resting orders"
    );
}

fn cancel_all() -> Value {
    json!({"type":"CancelAll"})
}

fn refdata(version: u64, f: impl FnOnce(&mut crate::common::p0::P0)) -> Value {
    let mut s = rd_spec();
    s.version = version;
    f(&mut s);
    json!({"type":"ReplaceRefdata","blob_b64":b64(&s.build()),"version":version.to_string()})
}

fn smoke_slots(r: &E2e, a: AccountId) -> Vec<u32> {
    [V_BG_SPOT, V_BN_USDM]
        .iter()
        .filter_map(|v| r.s.engine().oms.table_view(*v, a))
        .map(|t| t.smoke_slot)
        .collect()
}

#[test]
fn ctl_211_cancel_drain_refdata() {
    // .1 CancelAll of acct-a: cancels on both venues, nothing rests afterwards.
    let mut r = e2e();
    resting_a(&mut r);
    let m = r.mark();
    want(&r.send("cli:tom", A, cancel_all()), "Applied", None, "CTL-211.1");
    let venues: Vec<VenueId> = r
        .decisions_since(m)
        .iter()
        .filter(|d| matches!(d.action, OrderAction::Cancel | OrderAction::CancelAll))
        .map(|d| d.venue)
        .collect();
    assert!(
        venues.contains(&V_BG_SPOT) && venues.contains(&V_BN_USDM),
        "CTL-211.1 {venues:?}"
    );
    assert!(r.resting.iter().all(|d| d.account != ACCT_A), "CTL-211.1 resting");
    assert_eq!(smoke_slots(&r, ACCT_A), vec![0, 0], "CTL-211.1 slots");

    // .2 HARD Kill{Global}: CancelAll for every account.
    let mut r = e2e();
    want(
        &r.send(
            "cli:alice",
            ALL,
            json!({"type":"Kill","level":"Global","hard":true}),
        ),
        "Applied",
        None,
        "CTL-211.2 kill",
    );
    want(
        &r.send("cli:tom", ALL, cancel_all()),
        "Applied",
        None,
        "CTL-211.2",
    );

    // .3 / .4 / .5 Drain{cancel_resting}: the venue answers nothing, so the Drain stays open.
    let mut r = e2e();
    resting_a(&mut r);
    r.fills = Fills::Silent;
    let m = r.mark();
    let x = r.send("cli:tom", ALL, json!({"type":"Drain","cancel_resting":true}));
    want(&x, "Applied", None, "CTL-211.3");
    assert!(r.caps_of(CapSource::DRAINING) == 1, "CTL-211.3 cap");
    let cancels: Vec<VenueId> = r
        .decisions_since(m)
        .iter()
        .filter(|d| d.account == ACCT_A && matches!(d.action, OrderAction::Cancel | OrderAction::CancelAll))
        .map(|d| d.venue)
        .collect();
    assert!(!cancels.is_empty(), "CTL-211.3 cancels");
    let (p, c) = r.two_step(
        "cli:tom",
        A,
        json!({"type":"SetMode","strategy":"s1-main","group":"16777317","mode":"Live"}),
    );
    want(&p, "PreconditionFailed", Some("Draining"), "CTL-211.4 preview");
    want(&c, "PreconditionFailed", Some("Draining"), "CTL-211.4");
    want(&r.send("cli:tom", A, cancel_all()), "Applied", None, "CTL-211.5");

    // .6 the gateway's assembly check fails: nothing reaches the sequencer.
    let mut r = e2e();
    ASSEMBLY_FAIL.with(|c| c.set(3));
    let m = r.mark();
    // The preview is refused by the gateway (no `ctl_state_version`): confirm with the current.
    let p = r.preview("cli:alice", ALL, refdata(2, |_| {}));
    let v = r.version();
    let c = r.confirm("cli:alice", ALL, refdata(2, |_| {}), Some(v));
    ASSEMBLY_FAIL.with(|c| c.set(0));
    want(&p, "Rejected", Some("RefdataAssembly"), "CTL-211.6 preview");
    want(&c, "Rejected", Some("RefdataAssembly"), "CTL-211.6");
    assert!(
        r.kinds_since(m, EventKind::CONTROL_CMD).is_empty(),
        "CTL-211.6 ControlCmd"
    );
    assert!(
        r.kinds_since(m, EventKind::CMD_RESULT).is_empty(),
        "CTL-211.6 CmdResult"
    );

    // .8 version 1 (= current).
    let mut r = e2e();
    let (_, c) = r.two_step("cli:alice", ALL, refdata(1, |_| {}));
    want(&c, "StaleVersion", None, "CTL-211.8");

    // .9 version 2 whose load against the current refdata fails with AssetChanged (BTC renamed).
    let mut r = e2e();
    let (_, c) = r.two_step(
        "cli:alice",
        ALL,
        refdata(2, |s| {
            for a in s.assets.iter_mut() {
                if a.id == BTC {
                    a.name = "XBT".into();
                }
            }
        }),
    );
    want(&c, "Rejected", Some("RefdataRecheck"), "CTL-211.9");
    assert_eq!(arg(&c), "AssetChanged", "CTL-211.9 arg");
    assert_eq!(r.caps_of(CapSource::DRAINING), 0, "CTL-211.9 no Drain");
    assert_eq!(r.s.engine().rd.version(), 1, "CTL-211.9 refdata");

    // .10 / .11 version 2.
    let mut r = e2e();
    want(
        &r.send("cli:alice", ALL, refdata(2, |_| {})),
        "NeedsConfirm",
        None,
        "CTL-211.10",
    );
    r.hold_drain();
    let (_, c) = r.two_step("cli:alice", ALL, refdata(2, |_| {}));
    want(&c, "Applied", None, "CTL-211.11");
    let d =
        r.s.engine()
            .risk
            .mode
            .caps()
            .iter()
            .find(|c| c.source == CapSource::DRAINING)
            .copied()
            .expect("CTL-211.11 Draining");
    assert_eq!(d.params[0], 0, "CTL-211.11 cancel_resting = false");
}
