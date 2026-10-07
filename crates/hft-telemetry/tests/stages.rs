//! Segments (TEL-002–TEL-009, TEL-080).

mod common;

use std::collections::BTreeSet;

use common::*;
use hft_event::{EventKind, LatencySample, OrderEventKind, RecordFlags};
use hft_telemetry::{CHAIN_STAGES, CounterName, HistKey, Stage, Telemetry, WindowReport};
use hft_types::Tsc;

fn count(w: &WindowReport, f: impl Fn(&HistKey) -> bool) -> u64 {
    w.hist.iter().filter(|(k, _)| f(k)).map(|(_, q)| q.count).sum()
}

fn counter(w: &WindowReport, name: CounterName, stage: Option<Stage>) -> u64 {
    w.counters
        .iter()
        .filter(|(k, _)| k.name == name && k.stage == stage)
        .map(|(_, n)| n)
        .sum()
}

/// One window of `f`'s records.
fn window(f: impl FnOnce(&mut Feed)) -> WindowReport {
    let mut feed = Feed::new(&cfg());
    feed.tel.window(0);
    f(&mut feed);
    feed.tel.window(10_000_000_000).unwrap()
}

#[test]
fn tel_002_formula_values() {
    let st = Telemetry::stages(&std_sample(), &std_cal());
    let rows: &[(&str, Stage, i64)] = &[
        ("TEL-002.1", Stage::ExQueue, 1_000_000),
        ("TEL-002.2", Stage::WireIn, 1_400_000),
        ("TEL-002.3", Stage::FeedOneway, 2_400_000),
        ("TEL-002.4", Stage::KernelRx, 12_000),
        ("TEL-002.5", Stage::NicToKernel, 5_000),
        ("TEL-002.6", Stage::KernelToUser, 7_000),
        ("TEL-002.7", Stage::Ingest, 3_000),
        ("TEL-002.8", Stage::FeedHost, 15_000),
        ("TEL-002.9", Stage::Decide, 5_000),
        ("TEL-002.10", Stage::Egress, 8_000),
        ("TEL-002.11", Stage::TickToTrade, 28_000),
        ("TEL-002.12", Stage::OrderRtt, 1_000_000),
        ("TEL-002.13", Stage::TickToAck, 1_028_000),
    ];
    for (row, stage, want) in rows {
        assert_eq!(st.get(*stage), Some(*want), "{row} {}", stage.name());
    }
    assert_eq!(
        st.get(Stage::TickToTrade),
        Some(st.get(Stage::KernelRx).unwrap() + 16_000),
        "TEL-002.11: kernel_rx + (sent_tsc − rx_tsc)"
    );
    assert_eq!((st.err_bound_ns, st.cal_ok), (100, true), "TEL-002.14");
}

fn with_trigger(kind: EventKind, link_role: u8) -> LatencySample {
    let mut s = std_sample();
    s.trigger_kind = kind.0;
    s.link_role = link_role;
    s
}

#[test]
fn tel_003_trigger_classification() {
    let cal = std_cal();
    let hw_stages = [
        Stage::WireIn,
        Stage::FeedOneway,
        Stage::KernelRx,
        Stage::NicToKernel,
        Stage::FeedHost,
        Stage::TickToTrade,
        Stage::RepToOrder,
        Stage::HedgeHost,
        Stage::TickToAck,
    ];
    // TEL-003.1
    let st = Telemetry::stages(&with_trigger(EventKind::BBO, 0), &cal);
    assert!(st.get(Stage::TickToTrade).is_some(), "TEL-003.1");
    assert_eq!(
        (st.get(Stage::RepToOrder), st.get(Stage::HedgeHost)),
        (None, None),
        "TEL-003.1"
    );
    // TEL-003.2
    let st = Telemetry::stages(&with_trigger(EventKind::ORDER_EVENT, 0), &cal);
    assert_eq!(st.get(Stage::RepToOrder), Some(28_000), "TEL-003.2");
    assert_eq!(
        (st.get(Stage::TickToTrade), st.get(Stage::HedgeHost)),
        (None, None),
        "TEL-003.2"
    );
    // TEL-003.3
    let st = Telemetry::stages(&with_trigger(EventKind::ORDER_EVENT, 2), &cal);
    assert_eq!(
        (st.get(Stage::RepToOrder), st.get(Stage::HedgeHost)),
        (Some(28_000), Some(28_000)),
        "TEL-003.3"
    );
    // TEL-003.4
    let st = Telemetry::stages(&with_trigger(EventKind::ORDER_EVENT, 3), &cal);
    assert!(st.get(Stage::RepToOrder).is_some(), "TEL-003.4");
    assert_eq!(st.get(Stage::HedgeHost), None, "TEL-003.4");
    // TEL-003.5
    let st = Telemetry::stages(&with_trigger(EventKind::TIMER, 0), &cal);
    let produced: BTreeSet<Stage> = st.iter().map(|(s, _)| s).collect();
    assert_eq!(
        produced,
        BTreeSet::from([Stage::Decide, Stage::Egress, Stage::OrderRtt]),
        "TEL-003.5"
    );
    for s in hw_stages {
        assert_eq!(st.get(s), None, "TEL-003.5 {}", s.name());
    }

    // TEL-003.6 / TEL-003.7: v2 records (trigger_kind = 0) resolved through the trigger record.
    let run = |kind: EventKind, v2: bool| {
        window(|f| {
            f.clock(&std_a());
            let trig = if kind == EventKind::BBO {
                f.rec(
                    T0 + 415_000,
                    RecordFlags::HW_TS.0,
                    &bbo(mhdr(P0 + 400_000, P0 + 405_000, T0 + 412_000, 0)),
                )
            } else {
                f.rec(T0 + 415_000, 0, &order_event(OrderEventKind::Ack))
            };
            let mut s = with_trigger(kind, 0);
            s.trigger_seq = trig;
            if v2 {
                s.trigger_kind = 0;
            }
            f.sample(&s);
            f.clock(&std_b());
        })
    };
    assert_eq!(run(EventKind::BBO, true), run(EventKind::BBO, false), "TEL-003.6");
    assert_eq!(
        run(EventKind::ORDER_EVENT, true),
        run(EventKind::ORDER_EVENT, false),
        "TEL-003.7"
    );
    assert!(
        count(&run(EventKind::ORDER_EVENT, true), |k| k.stage
            == Stage::RepToOrder)
            == 1,
        "TEL-003.7"
    );
}

#[test]
fn tel_004_action_sampling() {
    let cal = std_cal();
    for (row, action, rtt) in [
        ("TEL-004.1", 0u8, true),
        ("TEL-004.2", 1, false),
        ("TEL-004.3", 2, false),
    ] {
        let mut s = with_trigger(EventKind::ORDER_EVENT, 0);
        s.action = action;
        let st = Telemetry::stages(&s, &cal);
        assert_eq!(st.get(Stage::OrderRtt).is_some(), rtt, "{row}");
        assert!(st.get(Stage::RepToOrder).is_some(), "{row}");
    }
    // TEL-004.4
    let w = window(|f| {
        f.clock(&std_a());
        let trig = f.rec(T0 + 415_000, 0, &order_event(OrderEventKind::Ack));
        for action in 0..3u8 {
            let mut s = with_trigger(EventKind::ORDER_EVENT, 0);
            s.trigger_seq = trig;
            s.action = action;
            f.sample(&s);
        }
        f.clock(&std_b());
    });
    assert_eq!(count(&w, |k| k.stage == Stage::RepToOrder), 3, "TEL-004.4");
    assert_eq!(count(&w, |k| k.stage == Stage::OrderRtt), 1, "TEL-004.4");
    let actions: BTreeSet<u8> = w
        .hist
        .iter()
        .filter(|(k, _)| k.stage == Stage::RepToOrder)
        .map(|(k, _)| k.action)
        .collect();
    assert_eq!(actions.len(), 3, "TEL-004.4");
}

#[test]
fn tel_005_rep_to_order_by_report_kind() {
    let w = window(|f| {
        f.clock(&std_a());
        for what in [
            OrderEventKind::Ack,
            OrderEventKind::Fill,
            OrderEventKind::CancelAck,
        ] {
            let trig = f.rec(T0 + 415_000, 0, &order_event(what));
            let mut s = with_trigger(EventKind::ORDER_EVENT, 0);
            s.trigger_seq = trig;
            f.sample(&s);
        }
        f.clock(&std_b());
    });
    let keys: Vec<(&HistKey, u64)> = w
        .hist
        .iter()
        .filter(|(k, _)| k.stage == Stage::RepToOrder)
        .map(|(k, q)| (k, q.count))
        .collect();
    assert_eq!(keys.len(), 3, "{keys:?}");
    assert!(keys.iter().all(|(_, n)| *n == 1));
    let subs: BTreeSet<u8> = keys.iter().map(|(k, _)| k.trigger_sub).collect();
    assert_eq!(subs, BTreeSet::from([1, 4, 5]));
}

#[test]
fn tel_006_sw_rx_split() {
    // TEL-006.1
    let st = Telemetry::stages(&std_sample(), &std_cal());
    let (n2k, k2u) = (
        st.get(Stage::NicToKernel).unwrap(),
        st.get(Stage::KernelToUser).unwrap(),
    );
    assert_eq!((n2k, k2u), (5_000, 7_000), "TEL-006.1");
    assert_eq!(n2k + k2u, st.get(Stage::KernelRx).unwrap(), "TEL-006.1");
    // TEL-006.2
    let mut s = std_sample();
    s.sw_rx_ns = 0;
    let st = Telemetry::stages(&s, &std_cal());
    assert_eq!(
        (st.get(Stage::NicToKernel), st.get(Stage::KernelToUser)),
        (None, None),
        "TEL-006.2"
    );
    let w = window(|f| f.std_with(s));
    assert_eq!(
        counter(&w, CounterName::Missing, Some(Stage::NicToKernel)),
        1,
        "TEL-006.2"
    );
    assert_eq!(
        counter(&w, CounterName::Missing, Some(Stage::KernelToUser)),
        1,
        "TEL-006.2"
    );
    assert_eq!(
        count(&w, |k| matches!(
            k.stage,
            Stage::NicToKernel | Stage::KernelToUser
        )),
        0,
        "TEL-006.2: not filled with 0"
    );
}

#[test]
fn tel_007_kernel_tx_off() {
    let st = Telemetry::stages(&std_sample(), &std_cal());
    assert_eq!(st.get(Stage::KernelTx), None);
    assert!(!st.is_missing(Stage::KernelTx));
    let w = window(|f| f.std_with(std_sample()));
    assert_eq!(counter(&w, CounterName::Missing, Some(Stage::KernelTx)), 0);
    assert_eq!(count(&w, |k| k.stage == Stage::KernelTx), 0);
}

#[test]
fn tel_008_leg_gap_pairing() {
    let w = window(|f| {
        f.clock(&std_a());
        let fill = f.rec(T0 + 500_000, 0, &order_event(OrderEventKind::Fill));
        let mut s = with_trigger(EventKind::ORDER_EVENT, 2);
        s.trigger_seq = fill;
        s.sent_tsc = Tsc(T0 + 530_000);
        f.sample(&s);
        f.clock(&std_b());
    });
    let lg: Vec<_> = w.hist.iter().filter(|(k, _)| k.stage == Stage::LegGap).collect();
    assert_eq!(lg.len(), 1);
    assert_eq!((lg[0].1.count, lg[0].1.max), (1, 30_000));
}

#[test]
fn tel_009_missing_negative_zero() {
    // TEL-009.1: hw_rx source missing (value 0).
    let mut s = std_sample();
    set_src(&mut s, 2, 0);
    s.hw_rx_ns = 0;
    let st = Telemetry::stages(&s, &std_cal());
    for stage in [Stage::KernelRx, Stage::FeedHost, Stage::TickToTrade] {
        assert_eq!(st.get(stage), None, "TEL-009.1 {}", stage.name());
    }
    let w = window(|f| f.std_with(s));
    for stage in [Stage::KernelRx, Stage::FeedHost, Stage::TickToTrade] {
        assert_eq!(
            counter(&w, CounterName::Missing, Some(stage)),
            1,
            "TEL-009.1 {}",
            stage.name()
        );
    }
    // TEL-009.2: ingest = −1.
    let mut s = std_sample();
    s.proc_tsc = Tsc(T0 + 411_999);
    assert_eq!(
        Telemetry::stages(&s, &std_cal()).get(Stage::Ingest),
        Some(-1),
        "TEL-009.2"
    );
    let w = window(|f| f.std_with(s));
    assert_eq!(count(&w, |k| k.stage == Stage::Ingest), 0, "TEL-009.2");
    assert_eq!(
        counter(&w, CounterName::Negative, Some(Stage::Ingest)),
        1,
        "TEL-009.2"
    );
    // TEL-009.3: ingest = 0 is recorded.
    let mut s = std_sample();
    s.proc_tsc = Tsc(T0 + 412_000);
    assert_eq!(
        Telemetry::stages(&s, &std_cal()).get(Stage::Ingest),
        Some(0),
        "TEL-009.3"
    );
    let w = window(|f| f.std_with(s));
    assert_eq!(count(&w, |k| k.stage == Stage::Ingest), 1, "TEL-009.3");
}

/// First-column backticked names of the first table after `heading` in `md`.
fn table_names(md: &str, heading: &str) -> BTreeSet<String> {
    let start = md.find(heading).unwrap();
    let mut out = BTreeSet::new();
    let mut in_table = false;
    for line in md[start..].lines().skip(1) {
        if line.starts_with('|') {
            in_table = true;
            let cell = line.trim_start_matches('|').split('|').next().unwrap().trim();
            if let Some(name) = cell.strip_prefix('`').and_then(|c| c.split('`').next()) {
                out.insert(name.to_string());
            }
        } else if in_table {
            break;
        }
    }
    out
}

#[test]
fn tel_080_names_match_latency_budget() {
    let md = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../docs/latency-budget.md"
    ))
    .unwrap();
    let mut doc = table_names(&md, "## 2. 分段名");
    assert!(doc.remove("ws_rtt"), "ws_rtt is in the §2 table");
    let ours: BTreeSet<String> = Stage::ALL.iter().map(|s| s.name().to_string()).collect();
    assert_eq!(ours, doc, "production and diagnostic segments");
    assert!(!ours.contains("ws_rtt"));
    // On-chain segments (optional extension), listed separately.
    let md22 = &md[md.find("### 2.2").unwrap()..];
    let chain_tbl = &md22[md22.find("| 分段 |").unwrap()..];
    let chain = table_names(chain_tbl, "| 分段 |");
    let ours: BTreeSet<String> = CHAIN_STAGES.iter().map(|s| s.to_string()).collect();
    assert_eq!(ours, chain, "on-chain segments");
}
