//! Backoff spins, then yields, never sleeps (RT-099). Linux + strace.

#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods
)] // allow-list: test code, not a hot path
#![cfg(all(target_os = "linux", not(loom)))]

#[allow(dead_code)]
mod common {
    pub mod proc;
}

use common::proc::*;
use hft_rt::Backoff;

#[test]
fn rt_099_backoff_never_sleeps() {
    if role().as_deref() == Some("traced") {
        let mut b = Backoff::new(4);
        register_traced_thread();
        mark();
        for _ in 0..1000 {
            b.snooze();
        }
        mark();
        b.reset();
        for _ in 0..4 {
            b.snooze();
        }
        mark();
        return;
    }
    let segs = strace_segments(
        "rt_099_backoff_never_sleeps",
        Some("sched_yield,nanosleep,clock_nanosleep,futex"),
    );
    assert_eq!(segs.len(), 2, "{segs:?}");
    let s = &segs[0];
    assert_eq!(n(s, "sched_yield"), 996, "{s:?}");
    assert_eq!(
        (n(s, "nanosleep"), n(s, "clock_nanosleep"), n(s, "futex")),
        (0, 0, 0),
        "{s:?}"
    );
    assert_eq!(n(&segs[1], "sched_yield"), 0, "{:?}", segs[1]);
}
