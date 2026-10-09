//! TK-037–TK-041, TK-094, TK-096: the test strategies.

use hft_strategy_sdk::params::{RawParams, decode_body, encode_body};
use hft_strategy_sdk::testing::ev;
use hft_strategy_sdk::{
    BrokenReason, CapScopeView, ExecSpec, ExecUpdate, ExecUpdateKind, ExternalFillView, Model, RiskEvent,
    StrategySet, TargetOrigin, TargetReason, Views,
};
use hft_testkit::sdk_mock::{Captured, ModelHarness, StrategyHarness};
use hft_testkit::strategies::probe_target::{ProbeModel, ProbeTarget};
use hft_testkit::strategies::{TEST_KIND_IDS, TestStrategies};
use hft_types::{AssetId, CapSource, Px, Qty, Side, Tsc};

use crate::sdk_mock::{quotes, taker, taker_harness, target};
use crate::sdk_rd::{A1, G1, I1, I2, V2, gt1, gt3_fixture};

#[test]
fn tk_037_test_strategy_set() {
    let k = TestStrategies::kinds();
    let names: Vec<&str> = k.iter().map(|d| d.name).collect();
    assert_eq!(names, ["probe-taker", "probe-target", "probe-np"]);
    let mut ids: Vec<u16> = k.iter().map(|d| d.kind_id).collect();
    assert!(ids.iter().all(|i| TEST_KIND_IDS.contains(i)));
    ids.dedup();
    assert_eq!(ids.len(), 3, "distinct kind_ids");
}

fn open_count(a_bid: i64) -> usize {
    let mut h = taker_harness(&taker(1_000_000, 7, 1));
    quotes(h.views_mut(), a_bid, 1_000_000_000);
    let e = ev::bbo(I1, 0, h.views_mut().bbo_of(I1));
    let c = h.feed(&e.as_ref());
    let places: Vec<_> = c.places().copied().collect();
    if let Some(p) = places.first() {
        assert_eq!(p.link.some, 1);
        let l = p.link.l;
        assert_eq!(l.hedge.inst, I2);
        assert_ne!(l.hedge.side, p.side);
    }
    places.len()
}

#[test]
fn tk_038_threshold() {
    // B ask 1 000 000 000 raw; spread = (A bid − B ask) × 10^9 / B ask; 10 bp = 1 000 000 raw.
    assert_eq!(open_count(1_000_999_999), 0, "TK-038.1 (10 bp − 1 raw)");
    assert_eq!(open_count(1_001_000_000), 0, "TK-038.2 (10 bp)");
    assert_eq!(open_count(1_001_000_001), 1, "TK-038.3 (10 bp + 1 raw)");
}

fn held_harness(close_on: i64) -> StrategyHarness<hft_testkit::strategies::ProbeTaker> {
    let mut h = taker_harness(&taker(1_000_000, close_on, 1));
    quotes(h.views_mut(), 1_002_000_000, 1_000_000_000);
    let e = ev::bbo(I1, 0, h.views_mut().bbo_of(I1));
    assert_eq!(h.feed(&e.as_ref()).places().count(), 1);
    h
}

fn events() -> [ev::Ev; 3] {
    let exec = ExecUpdate {
        group: G1,
        tag: 0,
        origin: TargetOrigin::Strategy,
        kind: ExecUpdateKind::Broken(BrokenReason::LinkBroken),
        target: [Qty(0); 4],
        filled: [Qty(0); 4],
        inflight: [Qty(0); 4],
    };
    let ext = ExternalFillView {
        venue: V2,
        inst: I2,
        group: G1,
        leg_ix: 1,
        side: Side::Buy,
        qty: Qty(1_000),
        px: Px(1),
        fee: 0,
        fee_asset: AssetId(0),
        cause: hft_event::ExternalCause::Manual,
        ex_ts_ns: 0,
        deadline_tsc: Tsc(0),
    };
    let risk = RiskEvent::CapSet {
        source: CapSource::KILL,
        scope: CapScopeView {
            strategy: hft_types::StrategyId(0),
            venue: hft_types::VenueId(0),
            account: A1,
            group: G1,
            inst: hft_types::InstId(0),
        },
    };
    [ev::exec(0, exec), ev::external(0, ext), ev::risk(A1, risk)]
}

#[test]
fn tk_039_close_on_events() {
    for (k, e) in events().iter().enumerate() {
        let mut h = held_harness(7);
        let c = h.feed(&e.as_ref());
        let t: Vec<_> = c.targets().copied().collect();
        assert_eq!(t.len(), 1, "TK-039.{}", k + 1);
        assert_eq!(t[0].reason, TargetReason::Exit);
        assert!(t[0].legs().iter().all(|l| l.qty == Qty(0)), "target 0");
    }
    for e in events() {
        let mut h = held_harness(0);
        assert!(h.feed(&e.as_ref()).items.is_empty(), "TK-039.4");
    }
}

#[test]
fn tk_040_schedule() {
    let p = target([1_000, 2_000, 3_000], false, 1_000, 5);
    let mut h = StrategyHarness::<ProbeTarget>::new(&p, &gt1(), gt3_fixture(), A1).expect("params");
    let mut got = Vec::new();
    for k in 0..=40i64 {
        let t = k * 100_000_000;
        h.views_mut().at_engine_ns(t);
        let e = ev::timer(1, G1, 0);
        for c in h.feed(&e.as_ref()).items {
            if let Captured::Target(x) = c {
                got.push((t, x.exec.algo()));
            }
        }
    }
    assert_eq!(
        got,
        vec![(1_000_000_000, 1), (2_000_000_000, 2), (3_000_000_000, 3)]
    );
}

#[test]
fn tk_041_model_advice() {
    // Encode the parameters with the exported schema and decode them back.
    let schema = &<ProbeTarget as hft_strategy_sdk::Strategy>::DESC.schema;
    assert!(TestStrategies::schema_json(0xFF02).is_some(), "schema exported");
    let raw = RawParams {
        defaults: vec![60, 0, 1_000, 0, 0, 0, 0, 5, 2_048, 64],
        overrides: Vec::new(),
    };
    let body = encode_body(*schema, 1, &raw).expect("encode");
    let back = decode_body(*schema, 1, 1, &body).expect("decode");
    assert_eq!(back.defaults, raw.defaults);
    let p = hft_testkit::strategies::TargetParams::from_raw(&back, 1);
    let mut m = ModelHarness::<ProbeTarget>::new(&p, &gt1(), gt3_fixture(), A1);
    let mut n = 0;
    for _ in 0..10 {
        let e = ev::timer(1, G1, 0);
        n += m.feed(&e.as_ref()).len();
    }
    assert!(n >= 1);
    assert!(ProbeModel::mem_bytes(&p, 1) > 0 && ProbeModel::snapshot_bytes(&p, 1) > 0);
}

#[test]
fn tk_094_probe_fixed_capabilities() {
    let k = TestStrategies::kinds();
    let tgt = k.iter().find(|d| d.name == "probe-target").expect("probe-target");
    let field = |n: &str| tgt.schema.fields.iter().find(|f| f.name == n);
    assert!(field("win").is_some_and(|f| f.structural), "TK-094.1");
    assert!(field("thr").is_some_and(|f| !f.structural), "TK-094.2");
    assert!(field("advice_max_age_ms").is_some(), "TK-094.3");
    let ops: Vec<(u8, &str)> = tgt.cmds.iter().map(|c| (c.op, c.name)).collect();
    assert_eq!(ops, vec![(1, "set_thr"), (2, "exit_all")], "TK-094.4");
    let np = k.iter().find(|d| d.name == "probe-np").expect("probe-np");
    assert!(np.non_prod_only, "TK-094.5");
    assert!(TEST_KIND_IDS.contains(&np.kind_id));
    assert!(
        k.iter()
            .filter(|d| d.name != "probe-np")
            .all(|d| d.kind_id != np.kind_id && !d.non_prod_only),
        "TK-094.5/6"
    );
}

fn advice_case(basis_ms: Option<i64>) -> usize {
    let p = target([10_000, 0, 0], true, 1_000, 5);
    let mut h = StrategyHarness::<ProbeTarget>::new(&p, &gt1(), gt3_fixture(), A1).expect("params");
    h.views_mut().at_engine_ns(9_500_000_000);
    if let Some(b) = basis_ms {
        let now = Views::now(h.views_mut());
        let basis_tsc = now.tsc_of(b * 1_000_000);
        let e = ev::advice_raw(0, 1, &[0u8; 16], now.boot_seq, basis_tsc);
        h.feed(&e.as_ref());
    }
    h.views_mut().at_engine_ns(10_000_000_000);
    let e = ev::timer(1, G1, 0);
    h.feed(&e.as_ref()).targets().count()
}

#[test]
fn tk_096_advice_max_age() {
    assert_eq!(advice_case(Some(9_001)), 1, "TK-096.1");
    assert_eq!(advice_case(Some(9_000)), 1, "TK-096.2");
    assert_eq!(advice_case(Some(8_999)), 0, "TK-096.3");
    assert_eq!(advice_case(None), 0, "TK-096.4");
    let _ = ExecSpec::algo;
}
