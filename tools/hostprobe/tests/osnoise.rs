//! `hostprobe osnoise` wrapper (tools/TESTS.md TOOL-111): parses the `rtla osnoise hist` substitute.

mod common;

use common::Fx;
use hostprobe::host::FixtureHost;

/// Three CPUs, 10,000 samples each; CPU 2: max 18 µs, p99.9 4 µs.
fn hist() -> String {
    let mut rows = vec![
        (0u64, [9980u64, 9990, 9995]),
        (1, [0, 9, 5]),
        (3, [9, 1, 0]),
        (4, [10, 0, 0]),
        (18, [1, 0, 0]),
    ];
    rows.sort();
    let mut s = String::from(
        "# RTLA osnoise histogram\n# Time unit is microseconds (us)\n# Duration:   0 00:00:10\nIndex   CPU-002   CPU-003   CPU-004\n",
    );
    for (i, c) in rows {
        s += &format!("{i:<5} {:>9} {:>9} {:>9}\n", c[0], c[1], c[2]);
    }
    s += "over:         0         0         0\ncount:    10000     10000     10000\nmin:          0         0         0\navg:          0         0         0\nmax:         18         3         1\n";
    s
}

#[test]
fn tool_111_wrapper_output() {
    let f = Fx::new("111");
    f.write(
        "root/.hostprobe/exec/rtla osnoise hist -c 2-4 -d 10s.out",
        &hist(),
    );
    let r = f.run(&["osnoise", "--cpus", "2-4", "--duration", "10s"]);
    assert_eq!(r.code, 0, "{}", r.stderr);
    let v: serde_json::Value = serde_json::from_str(&r.stdout).unwrap();
    assert_eq!(v["per_cpu"]["2"]["max_ns"], 18000);
    assert_eq!(v["per_cpu"]["2"]["p999_ns"], 4000);
    assert_eq!(v["per_cpu"]["2"]["samples"], 10000);
    assert_eq!(v["per_cpu"]["3"]["max_ns"], 3000);
    assert_eq!(v["per_cpu"]["4"]["max_ns"], 1000);
    // The rtla call carries `-c 2-4`.
    let mut h = FixtureHost::new(&f.root());
    hostprobe::osnoise::run(&mut h, "2-4", "10s").unwrap();
    assert_eq!(h.trace.execs.len(), 1);
    assert!(h.trace.execs[0].contains("-c 2-4"), "{:?}", h.trace.execs);
}
