//! tmpfs small-segment mode equals the heap backend byte for byte (JNL-105). Linux; elsewhere both
//! sides run on the heap backend.

#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods
)] // allow-list: test code, not a hot path

use crate::common;

use common::harness::*;
use hft_event::EventKind;
use hft_journal::*;
use hft_types::Tsc;

fn masked(b: &[u8]) -> Vec<u8> {
    let mut v = b.to_vec();
    for x in v.iter_mut().take(3200).skip(3072) {
        *x = 0;
    }
    v
}

/// 500 events with `TMulti` and `TCrc`, two rolls.
fn script(h: &mut H) {
    for i in 0..500u32 {
        let w = h.w();
        w.append(F0, Tsc(u64::from(i)), &tin(i as u8)).unwrap();
        if i % 7 == 0 {
            w.append_var(
                EventKind::T_MULTI,
                F0,
                Tsc(u64::from(i)),
                &[&multi_body(70_000 + i as usize)],
            )
            .unwrap();
        }
        w.append(F0, Tsc(u64::from(i)), &tcrc(i as u8)).unwrap();
        w.end_event();
        w.publish();
        if i == 200 || i == 400 {
            h.roll(RollReason::Hourly);
        }
    }
}

#[cfg(target_os = "linux")]
fn minflt() -> i64 {
    // SAFETY: zeroed rusage is a valid out-buffer.
    let mut u: libc::rusage = unsafe { std::mem::zeroed() };
    // SAFETY: RUSAGE_SELF with a writable buffer.
    unsafe { libc::getrusage(libc::RUSAGE_SELF, &mut u) };
    u.ru_minflt
}

#[test]
fn jnl_105_tmpfs_equals_mem() {
    let seg = 8u64 << 20;
    let mut a = c0(&mem_hot(), &cold_dir());
    a.segment_bytes = seg;
    let mut b = c0(&linux_hot(), &cold_dir());
    b.segment_bytes = seg;
    let mut ha = H::open0(a);
    let mut hb = H::open0(b);
    script(&mut ha);
    script(&mut hb);
    for n in 1..=3 {
        assert_eq!(masked(&ha.seg(n)), masked(&hb.seg(n)), "segment {n}");
    }
    let w = hb.w();
    let body = tin(1);
    let out = tout(2);
    #[cfg(target_os = "linux")]
    let before = minflt();
    for i in 0..10_000u64 {
        w.append(F0, Tsc(i), &body).unwrap();
        w.append(F0, Tsc(i), &out).unwrap();
        w.end_event();
        w.publish();
    }
    #[cfg(target_os = "linux")]
    assert_eq!(minflt() - before, 0, "page faults while appending");
}
