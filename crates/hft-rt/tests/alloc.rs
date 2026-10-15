//! Hot entries allocate nothing (RT-166).
//!
//! A per-thread counting allocator stands in for `hft-testkit::CountingAlloc` until that crate
//! exists; only allocations made on the measuring thread while counting is enabled are counted.

#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods
)] // allow-list: test code, not a hot path
#![cfg(not(loom))]

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::hint::black_box;
use std::sync::atomic::{AtomicU32, Ordering};

use hft_rt::*;

struct CountingAlloc;

thread_local! {
    static ENABLED: Cell<bool> = const { Cell::new(false) };
    static COUNT: Cell<u64> = const { Cell::new(0) };
}

fn note() {
    let _ = ENABLED.try_with(|e| {
        if e.get() {
            let _ = COUNT.try_with(|c| c.set(c.get() + 1));
        }
    });
}

// SAFETY: every method forwards to `System` unchanged; the extra work touches const-initialized
// thread-locals, which never allocate.
unsafe impl GlobalAlloc for CountingAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        note();
        // SAFETY: same contract as the caller's.
        unsafe { System.alloc(layout) }
    }
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        note();
        // SAFETY: same contract as the caller's.
        unsafe { System.alloc_zeroed(layout) }
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        note();
        // SAFETY: same contract as the caller's.
        unsafe { System.realloc(ptr, layout, new_size) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: same contract as the caller's.
        unsafe { System.dealloc(ptr, layout) }
    }
}

#[global_allocator]
static GLOBAL: CountingAlloc = CountingAlloc;

fn count() -> u64 {
    COUNT.with(|c| c.get())
}

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
fn rt_166_zero_alloc_hot_entries() {
    let (mut p, mut c) = spsc::<u64>(1024, Backing::Heap).unwrap();
    let (mut bp, mut bc) = byte_spsc(4096, Backing::Heap).unwrap();
    let mb: &'static Mailbox<Box<u64>> = Box::leak(Box::new(Mailbox::new()));
    let (mut tx, mut rx) = mb.split();
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

    let (trace_writer, mut trace_reader) = latency_trace::channel(256).unwrap();
    latency_trace::install(trace_writer).unwrap();
    ENABLED.with(|e| e.set(true));
    let c0 = count();
    for i in 0..100_000u64 {
        latency_trace::stamp(latency_trace::Stage::Read, [i; 4]);
        black_box(trace_reader.rx.try_pop());
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
            assert!(tx.try_send(v).is_ok());
        }
        boxed = rx.try_take();
        black_box(cell.load());
        black_box(kw.load(Ordering::Acquire));
        let t = tsc_now();
        black_box(clk.delta_ns(Tsc(0), t));
        hb.beat();
        black_box(tl.idle_action(t, i % 3 == 0));
    }
    let c1 = count();
    drop(black_box(Box::new(1u64)));
    let c2 = count();
    ENABLED.with(|e| e.set(false));
    assert_eq!(c1 - c0, 0);
    assert_eq!(c2 - c1, 1, "the counter works");
}
