//! CTL-100, CTL-103, CTL-104.
#![allow(
    clippy::type_complexity,
    clippy::disallowed_types,
    clippy::disallowed_methods,
    clippy::disallowed_macros
)] // allow-list: test code, not a hot path

mod common;

use common::*;
use hft_event::EventKind;
use serde_json::{Value, json};

fn big_set_params() -> Value {
    json!({"type":"SetParams","strategy":"s1-main","blob_b64":b64(&big_spb().bytes()),"base_version":"41"})
}

#[test]
fn ctl_100_all_or_nothing() {
    // Measure T on a ring with room.
    let mut g = G0::big();
    let cmd = big_set_params();
    let v = g.req("cli:tom", Some("acct-a"), cmd.clone());
    let free0 = g.core.ring().free;
    let _ = g.send_v(&v);
    let t = free0 - g.core.ring().free;
    assert!(t > 0);
    for (row, free, written) in [
        ("CTL-100.1", t, true),
        ("CTL-100.2", t - 1, false),
        ("CTL-100.3", t + 1, true),
    ] {
        let mut g = G0::big();
        g.core.ring_mut().free = free;
        let v = g.req("cli:tom", Some("acct-a"), cmd.clone());
        let r = g.send_v(&v);
        let recs = g.take_ring();
        if written {
            assert!(r.is_none(), "{row}");
            assert!(recs.iter().any(|r| r.is_cmd()), "{row}");
            assert!(recs.iter().any(|r| r.is_note()), "{row}");
            assert!(g.mb_take.try_take().is_some(), "{row}");
        } else {
            assert_eq!(status(&r.unwrap()), "busy", "{row}");
            assert!(recs.is_empty(), "{row}");
            assert!(g.mb_take.try_take().is_none(), "{row}");
        }
    }
}

fn report(n: usize) -> Value {
    let items: Vec<Value> = (0..n)
        .map(|_| json!({"venue":"binance-usdm","account":"acct-a","item":"Position","inst":"binance-usdm:BTCUSDT","engine_qty":"0","exch_qty":"0"}))
        .collect();
    json!({"type":"ReconcileReport","report_id":"7","items":items})
}

#[test]
fn ctl_103_reconcile_report() {
    let mut g = G0::big();
    let v = g.req("reconciler", None, report(3));
    for step in 1..=2 {
        let r = g.send_v(&v).unwrap();
        assert_eq!((status(&r).as_str(), code(&r)), ("ok", "Applied"), "step {step}");
        assert!(r.cmd_hash.is_none());
        let recs = g.take_ring();
        let reps: Vec<_> = recs
            .iter()
            .filter(|r| r.kind == EventKind::RECONCILE_REPORT)
            .collect();
        assert_eq!(reps.len(), 3, "step {step}");
        for (i, x) in reps.iter().enumerate() {
            let rr: hft_event::ReconcileReport = zerocopy::FromBytes::read_from_bytes(&x.body[..]).unwrap();
            assert_eq!((rr.idx, rr.count), (i as u16, 3));
        }
        let a = only_audit(&recs);
        assert_eq!((a.cmd_kind, a.result_code), (0, 0), "step {step}");
    }
}

#[test]
fn ctl_104_reconcile_report_busy() {
    let mut g = G0::new();
    let v = g.req("reconciler", None, report(10));
    let free0 = g.core.ring().free;
    g.send_v(&v);
    let t = free0 - g.core.ring().free;
    let mut g = G0::new();
    g.core.ring_mut().free = t - 1;
    let v = g.req("reconciler", None, report(10));
    assert_eq!(status(&g.send_v(&v).unwrap()), "busy");
    assert!(g.take_ring().is_empty());
}
