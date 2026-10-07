//! Occupancy of the IRQ-dedicated core and the allowed keep-awake forms (tools/TESTS.md TOOL-105).

mod common;

use common::Fx;
use serde_json::json;

#[test]
fn tool_105_keepawake_whitelist() {
    // Expected profile: IRQ core 1, keepawake thread `hft-keepawake-1`.
    type Setup = fn(&Fx);
    let rows: Vec<(&str, Setup, serde_json::Value, serde_json::Value, i32)> = vec![
        (
            "TOOL-105.1",
            |f| {
                f.task(9001, "hft-keepawake-1", 1);
            },
            json!([]),
            json!([]),
            0,
        ),
        (
            "TOOL-105.2",
            |f| {
                f.task(9002, "spin-loop", 1);
            },
            json!([{"cpu": 1, "comm": "spin-loop", "tid": 9002}]),
            json!([]),
            1,
        ),
        (
            "TOOL-105.3",
            |f| {
                f.task(9001, "hft-keepawake-1", 1)
                    .task(9003, "hft-keepawake-1b", 1);
            },
            json!([{"cpu": 1, "comm": "hft-keepawake-1b", "tid": 9003}]),
            json!([]),
            1,
        ),
        (
            "TOOL-105.4",
            |f| {
                f.replace(
                    "root/proc/cmdline",
                    " transparent_hugepage",
                    " cpuidle.off=1 transparent_hugepage",
                );
            },
            json!([]),
            json!(["cpuidle.off"]),
            1,
        ),
        (
            "TOOL-105.5",
            |f| {
                // Process x holds a 0 µs constraint: the device reads 0 instead of the default.
                std::fs::write(f.path("root/dev/cpu_dma_latency"), 0i32.to_le_bytes()).unwrap();
            },
            json!([]),
            json!(["cpu_dma_latency=0"]),
            1,
        ),
        (
            "TOOL-105.6",
            |f| {
                f.replace(
                    "root/proc/cmdline",
                    " transparent_hugepage",
                    " nohlt transparent_hugepage",
                );
            },
            json!([]),
            json!([]),
            0,
        ),
    ];
    for (row, setup, tasks, bad, code) in rows {
        let f = Fx::new("105");
        setup(&f);
        let r = f.verify();
        assert_eq!(r.code, code, "{row}: {}", r.stderr);
        let v = f.json("v.json");
        assert_eq!(v["occupancy"]["irq_cores_tasks"], tasks, "{row}");
        assert_eq!(v["occupancy"]["keepawake_violations"], bad, "{row}");
    }
}
