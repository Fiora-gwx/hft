//! Hot entries make no system call (RT-167). Linux + strace (all system calls traced).

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

use std::hint::black_box;
use std::sync::atomic::{AtomicU32, Ordering};

use common::proc::*;
use hft_rt::*;

struct Idle;
impl PollTask for Idle {
    fn poll_once(&mut self, _: Tsc) -> Polled {
        Polled { did_work: false }
    }
    fn has_pending(&self) -> bool {
        false
    }
}

#[test]
fn rt_167_zero_syscalls_hot_entries() {
    if role().as_deref() == Some("traced") {
        let (mut p, mut c) = spsc::<u64>(1024, Backing::Heap).unwrap();
        p.set_wake(None);
        let (mut bp, mut bc) = byte_spsc(4096, Backing::Heap).unwrap();
        bp.set_wake(None);
        let mb: &'static Mailbox<Box<u64>> = Box::leak(Box::new(Mailbox::new()));
        let (mut tx, mut rx) = mb.split();
        tx.set_wake(None);
        let mut boxed = Some(Box::new(7u64));
        let clk = TscClock::from_hz_milli(2_500_000_000_000, CalibPair::default());
        let cell = SeqCell::new(clk.tsc_utc(&CalibPair::default()));
        let kw: KillWord = CachePadded(AtomicU32::new(0));
        let hb = Heartbeat::new();
        let mut tl = TaskLoop::new(
            [Idle],
            IdlePolicy::SpinThenPark {
                spin_us: 20,
                park_us: 1000,
            },
            None,
            &clk,
        );
        register_traced_thread();
        mark();
        for i in 0..100_000u64 {
            let _ = p.try_push(i);
            black_box(c.try_pop());
            let mut b = p.write_batch(4);
            for k in 0..4 {
                b.push(k);
            }
            b.commit();
            let r = c.read_batch(4);
            black_box(r.slices());
            r.release();
            if let Some(buf) = bp.reserve(24) {
                buf[0] = i as u8;
            }
            bp.commit();
            black_box(bc.peek());
            bc.advance();
            if let Some(v) = boxed.take() {
                let _ = tx.try_send(v);
            }
            boxed = rx.try_take();
            black_box(cell.load());
            black_box(kw.load(Ordering::Acquire));
            let t = tsc_now();
            black_box(clk.delta_ns(Tsc(0), t));
            hb.beat();
            black_box(tl.idle_action(t, i % 3 == 0));
        }
        mark();
        return;
    }
    let segs = strace_segments("rt_167_zero_syscalls_hot_entries", None);
    assert_eq!(segs.len(), 1);
    assert_eq!(segs[0].values().sum::<u32>(), 0, "{:?}", segs[0]);
}
