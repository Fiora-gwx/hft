//! SLC-040: the dry run drives every thread and the histogram pipeline.

mod support;

use latency_slice::config;
use latency_slice::run::{Kind, execute};
use serde_json::Value;
use support::CFG0;

fn n_samples(v: &Value, stage: &str) -> u64 {
    v["samples"]["hist"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|e| e["key"]["stage"] == stage)
        .map(|e| e["q"]["count"].as_u64().unwrap())
        .sum()
}

fn run(text: &str) {
    let root = support::tmp("dry");
    let hot = root.join("hot");
    hft_journal::testing::mem_root(&hot);
    let out = root.join("out");
    // Generous calibration error bound: shared CI runners and VMs have noisy clocks.
    let cfg = config::parse(&format!(
        "{text}\n[journal]\nhot_dir = {:?}\n[report]\ncal_max_err_ns = 1000000\n",
        hot.display().to_string()
    ))
    .unwrap();
    let mut cfg = cfg;
    cfg.out_dir = out.clone();
    cfg.cold_dir = out.join("journal");
    let o = execute(&cfg, Kind::Dry { seconds: 3, seed: 1 });
    assert_eq!(o.code, 0, "{}", o.message);
    let v = o.report.expect("report");
    assert!(v["run"]["halted"].is_null());
    for stage in [
        "ingest",
        "decide",
        "egress",
        "order_rtt",
        "rep_to_order",
        "tick_to_trade",
    ] {
        assert!(n_samples(&v, stage) > 0, "{stage}: {}", report_text(&v));
    }
    let ew: u64 = v["event_weighted"][0]["hist"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|e| e[0]["stage"] == "kernel_rx")
        .map(|e| e[1]["count"].as_u64().unwrap())
        .sum();
    assert!(ew > 0, "event_weighted kernel_rx");
    let j = &v["journal"];
    let new = j["decisions_new"].as_u64().unwrap();
    assert!(new > 0);
    assert_eq!(j["acks"].as_u64(), Some(new));
    assert_eq!(j["cancel_acks"].as_u64(), Some(new));
    assert_eq!(v["run"]["open_orders_at_end"].as_u64(), Some(0));
    assert!(out.join("report.json").is_file() && out.join("report.txt").is_file());
}

fn report_text(v: &Value) -> String {
    latency_slice::report::text(v)
}

#[test]
fn slc_040_dry_run_pipeline() {
    run(CFG0);
    let usdm = CFG0
        .replace("product = \"spot\"", "product = \"usdm\"")
        .replace("tick = \"0.01\"", "tick = \"0.1\"")
        .replace("lot = \"0.00001\"", "lot = \"0.001\"")
        .replace("min_qty = \"0.00001\"", "min_qty = \"0.001\"")
        .replace("min_notional = \"5\"", "min_notional = \"100\"")
        .replace(
            "[net]",
            "[orders]\nhost = \"testnet.binancefuture.com\"\nmax_order_notional = \"500\"\n[net]",
        );
    run(&usdm);
}
