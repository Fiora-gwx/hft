//! SLC-030, SLC-031: the report from a seq journal.

mod support;

use hft_event::{OrderEventKind, SendStatus};
use latency_slice::report::{self, ReportCfg};
use serde_json::Value;
use support::{H, MS, cfg_with};

const N: u64 = 1_200;

/// SLC-030's journal: N cycles, `decide` = 2 µs, `egress` = 3 µs; `sw_every` > 0 makes every
/// `sw_every`-th Bbo a SW-stamped one (`hw_rx_ns = sw_rx_ns`).
fn journal(sw_every: u64) -> H {
    let mut h = H::new(cfg_with(""), true);
    h.clock_status(500 * MS);
    for k in 1..=N {
        let t = k * 1_000 * MS;
        let (hw, sw) = if sw_every > 0 && k % sw_every == 0 {
            (t as i64 - 5_000, t as i64 - 5_000)
        } else {
            (t as i64 - 5_000, t as i64 - 3_000)
        };
        h.bbo_at(1, t, t, 6_500_000, 6_500_010, hw, sw, 2_000);
        let p = h.cmds().pop().expect("place");
        let sent = p.decided_tsc.0 + 3_000;
        h.send(&p, SendStatus::Written, sent);
        let rx = sent + 1_500_000;
        h.ev(
            OrderEventKind::Ack,
            p.slot.0,
            p.r#gen,
            rx,
            rx as i64 - 4_000,
            2_000,
        );
        let c = h.cmds().pop().expect("cancel");
        let csent = c.decided_tsc.0 + 3_000;
        h.send(&c, SendStatus::Written, csent);
        let crx = csent + 1_000_000;
        h.ev(OrderEventKind::CancelAck, p.slot.0, p.r#gen, crx, crx as i64, 0);
        h.clock_status(t + 500 * MS);
    }
    h.close();
    h
}

fn rc() -> ReportCfg {
    ReportCfg {
        burst_gap_ns: 20_000,
        cal_max_err_ns: 1_000,
    }
}

/// `(count, p50, p99)` of every `samples.hist` entry with `stage` (and `ts_src`, `action` when given).
fn q(v: &Value, stage: &str, ts_src: Option<&str>, action: Option<u64>) -> Vec<(u64, i64, i64)> {
    v["samples"]["hist"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|e| e["key"]["stage"] == stage)
        .filter(|e| ts_src.is_none_or(|t| e["key"]["ts_src"] == t))
        .filter(|e| action.is_none_or(|a| e["key"]["action"] == a))
        .map(|e| {
            (
                e["q"]["count"].as_u64().unwrap(),
                e["q"]["p50"].as_i64().unwrap(),
                e["q"]["p99"].as_i64().unwrap_or(-1),
            )
        })
        .collect()
}

fn ew(v: &Value, stage: &str, sec: &str) -> Vec<(String, u64)> {
    v["event_weighted"][0][sec]
        .as_array()
        .unwrap()
        .iter()
        .filter(|e| e[0]["stage"] == stage)
        .map(|e| {
            (
                e[0]["ts_src"].as_str().unwrap().to_string(),
                e[1]["count"].as_u64().unwrap(),
            )
        })
        .collect()
}

fn within(v: i64, want: i64) -> bool {
    // hdrhistogram with 3 significant digits.
    (v - want).abs() <= want / 500 + 1
}

#[test]
fn slc_030_report_from_journal() {
    let h = journal(0);
    let v = report::build(&h.hot, &h.cold, &rc(), None).unwrap();
    for (stage, want) in [("decide", 2_000), ("egress", 3_000)] {
        let rows = q(&v, stage, None, None);
        assert!(!rows.is_empty(), "{stage}");
        for (n, p50, p99) in rows {
            assert!(n >= 1_000, "{stage} n = {n}");
            assert!(within(p50, want) && within(p99, want), "{stage}: {p50} {p99}");
        }
    }
    for stage in ["tick_to_trade", "rep_to_order", "order_rtt"] {
        let n: u64 = q(&v, stage, None, None).iter().map(|r| r.0).sum();
        assert!(n >= 1_000, "{stage} n = {n}");
    }
    let bbo = v["journal"]["bbo"].as_u64().unwrap();
    for stage in ["kernel_rx", "ingest"] {
        let n: u64 = ew(&v, stage, "hist").iter().map(|r| r.1).sum();
        assert_eq!(n, bbo, "{stage}");
    }
    let hi: u64 = ew(&v, "kernel_rx", "kernel_rx_hi").iter().map(|r| r.1).sum();
    assert_eq!(hi, bbo, "kernel_rx_hi");
    let groups = v["stratified"]["groups"].as_array().unwrap();
    assert!(
        groups.iter().any(|g| g["key"]["stage"] == "tick_to_trade"),
        "stratified"
    );
    let j = &v["journal"];
    assert_eq!(
        (
            j["decisions_new"].as_u64(),
            j["acks"].as_u64(),
            j["cancel_acks"].as_u64()
        ),
        (Some(N), Some(N), Some(N))
    );
    // The text form renders.
    assert!(report::text(&v).contains("decide"));
}

#[test]
fn slc_031_hw_sw_separate() {
    let h = journal(2);
    let v = report::build(&h.hot, &h.cold, &rc(), None).unwrap();
    let hw: u64 = q(&v, "kernel_rx", Some("Hw"), Some(0)).iter().map(|r| r.0).sum();
    let sw: u64 = q(&v, "kernel_rx", Some("Sw"), Some(0)).iter().map(|r| r.0).sum();
    assert_eq!((hw, sw), (N / 2, N / 2), "samples");
    let e = ew(&v, "kernel_rx", "hist");
    let ehw: u64 = e.iter().filter(|r| r.0 == "Hw").map(|r| r.1).sum();
    let esw: u64 = e.iter().filter(|r| r.0 == "Sw").map(|r| r.1).sum();
    assert_eq!((ehw, esw), (N / 2, N / 2), "event_weighted");
}
