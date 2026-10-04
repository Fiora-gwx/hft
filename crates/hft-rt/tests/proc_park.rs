//! Parking and waking with real futex / epoll (RT-117 – RT-120, RT-122). Linux.

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

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Duration, Instant};

use common::proc::*;
use hft_rt::*;

fn c1g() -> TscClock {
    TscClock::from_hz_milli(1_000_000_000_000, CalibPair::default())
}

fn base() -> &'static Instant {
    static B: std::sync::OnceLock<Instant> = std::sync::OnceLock::new();
    B.get_or_init(Instant::now)
}

fn now_ns() -> u64 {
    base().elapsed().as_nanos() as u64
}

/// Pops one element per poll, spending `cost` per element; records when the first one was seen.
struct RingTask {
    rx: Consumer<u64>,
    cost: Duration,
    seen_at: Arc<AtomicU64>,
    count: Arc<AtomicU64>,
}

impl PollTask for RingTask {
    fn poll_once(&mut self, _: Tsc) -> Polled {
        match self.rx.try_pop() {
            Some(_) => {
                let t = Instant::now();
                while t.elapsed() < self.cost {
                    cpu_relax();
                }
                self.seen_at.store(now_ns(), Ordering::Release);
                self.count.fetch_add(1, Ordering::AcqRel);
                Polled { did_work: true }
            }
            None => Polled { did_work: false },
        }
    }
    fn has_pending(&self) -> bool {
        self.rx.len_hint() > 0
    }
}

#[test]
fn rt_117_futex_wake() {
    if role().as_deref() == Some("traced") {
        let parker = Parker::new(ParkBackend::Futex).unwrap();
        let (mut p, rx) = spsc::<u64>(16_384, Backing::Heap).unwrap();
        p.set_wake(Some(parker.unparker()));
        let seen = Arc::new(AtomicU64::new(0));
        let count = Arc::new(AtomicU64::new(0));
        let stop = Arc::new(AtomicBool::new(false));
        let task = RingTask {
            rx,
            cost: Duration::from_micros(20),
            seen_at: seen.clone(),
            count: count.clone(),
        };
        let st = stop.clone();
        let h = std::thread::spawn(move || {
            let hb = Heartbeat::new();
            let mut tl = TaskLoop::new(
                [task],
                IdlePolicy::Park { park_us: 1_000_000 },
                Some(parker),
                &c1g(),
            );
            tl.run(&st, &hb);
        });
        register_traced_thread();
        std::thread::sleep(Duration::from_millis(100));
        mark();
        let t0 = now_ns();
        p.try_push(1).unwrap();
        mark();
        while count.load(Ordering::Acquire) < 1 {
            cpu_relax();
        }
        let lat = seen.load(Ordering::Acquire) - t0;
        assert!(lat < 10_000_000, "step 1 latency {lat} ns");
        // Keep the consumer busy, then commit 10 000 more while it is running.
        for i in 0..200 {
            p.try_push(i).unwrap();
        }
        while count.load(Ordering::Acquire) < 2 {
            cpu_relax();
        }
        mark();
        for i in 0..10_000 {
            p.try_push(i).unwrap();
        }
        mark();
        while count.load(Ordering::Acquire) < 10_201 {
            std::thread::sleep(Duration::from_millis(5));
        }
        stop.store(true, Ordering::Release);
        let _ = p.try_push(0);
        h.join().unwrap();
        return;
    }
    let segs = strace_segments("rt_117_futex_wake", Some("futex"));
    assert_eq!(segs.len(), 3, "{segs:?}");
    assert_eq!(n(&segs[0], "futex"), 1, "step 1: exactly one wake");
    assert_eq!(n(&segs[2], "futex"), 0, "step 2: consumer busy, no wake");
}

struct SockTask {
    fd: i32,
    got: Arc<AtomicU64>,
}

impl PollTask for SockTask {
    fn poll_once(&mut self, _: Tsc) -> Polled {
        let mut b = [0u8; 16];
        // SAFETY: non-blocking recv into a live buffer.
        let n = unsafe { libc::recv(self.fd, b.as_mut_ptr().cast(), b.len(), libc::MSG_DONTWAIT) };
        if n > 0 {
            self.got.store(now_ns(), Ordering::Release);
            Polled { did_work: true }
        } else {
            Polled { did_work: false }
        }
    }
    fn wait_fds(&self, out: &mut WaitFds) {
        out.push(self.fd);
    }
    fn has_pending(&self) -> bool {
        false
    }
}

#[test]
fn rt_118_epoll_socket_wake() {
    let mut fds = [0i32; 2];
    // SAFETY: valid out array for two descriptors.
    assert_eq!(
        unsafe { libc::socketpair(libc::AF_UNIX, libc::SOCK_STREAM, 0, fds.as_mut_ptr()) },
        0
    );
    let got = Arc::new(AtomicU64::new(0));
    let stop = Arc::new(AtomicBool::new(false));
    let parker = Parker::new(ParkBackend::Epoll).unwrap();
    let (g, st, fd) = (got.clone(), stop.clone(), fds[0]);
    let h = std::thread::spawn(move || {
        let hb = Heartbeat::new();
        let mut tl = TaskLoop::new(
            [SockTask { fd, got: g }],
            IdlePolicy::Park { park_us: 1_000_000 },
            Some(parker),
            &c1g(),
        );
        tl.run(&st, &hb);
    });
    std::thread::sleep(Duration::from_millis(100));
    let t0 = now_ns();
    // SAFETY: writes one byte from a live buffer to the other socket end.
    assert_eq!(unsafe { libc::write(fds[1], b"x".as_ptr().cast(), 1) }, 1);
    let deadline = Instant::now() + Duration::from_secs(2);
    while got.load(Ordering::Acquire) == 0 && Instant::now() < deadline {
        std::thread::sleep(Duration::from_micros(100));
    }
    let at = got.load(Ordering::Acquire);
    assert!(
        at != 0 && at - t0 < 10_000_000,
        "read after {} ns",
        at.saturating_sub(t0)
    );
    stop.store(true, Ordering::Release);
    // SAFETY: wake the loop once more so it sees `stop`.
    unsafe { libc::write(fds[1], b"y".as_ptr().cast(), 1) };
    h.join().unwrap();
}

struct IdleCount {
    polls: Arc<AtomicU64>,
}

impl PollTask for IdleCount {
    fn poll_once(&mut self, _: Tsc) -> Polled {
        self.polls.fetch_add(1, Ordering::AcqRel);
        Polled { did_work: false }
    }
    fn has_pending(&self) -> bool {
        false
    }
}

#[test]
fn rt_119_park_timeout_bound() {
    let polls = Arc::new(AtomicU64::new(0));
    let stop = Arc::new(AtomicBool::new(false));
    let hb: &'static Heartbeat = Box::leak(Box::new(Heartbeat::new()));
    let parker = Parker::new(ParkBackend::Futex).unwrap();
    let (pc, st) = (polls.clone(), stop.clone());
    let h = std::thread::spawn(move || {
        let mut tl = TaskLoop::new(
            [IdleCount { polls: pc }],
            IdlePolicy::Park { park_us: 5_000 },
            Some(parker),
            &c1g(),
        );
        tl.run(&st, hb);
    });
    std::thread::sleep(Duration::from_secs(1));
    stop.store(true, Ordering::Release);
    h.join().unwrap();
    let rounds = polls.load(Ordering::Acquire);
    assert!(rounds <= 201, "rounds {rounds}");
    assert!(rounds >= 50, "rounds {rounds}");
    assert_eq!(hb.read(), rounds);
}

#[test]
fn rt_120_spin_producer_no_wake() {
    if role().as_deref() == Some("traced") {
        let parker = Parker::new(ParkBackend::Futex).unwrap();
        let (mut p, rx) = spsc::<u64>(64, Backing::Heap).unwrap();
        p.set_wake(None);
        let lat = Arc::new(AtomicU64::new(0));
        let count = Arc::new(AtomicU64::new(0));
        let stop = Arc::new(AtomicBool::new(false));
        struct Lat {
            rx: Consumer<u64>,
            worst: Arc<AtomicU64>,
            count: Arc<AtomicU64>,
        }
        impl PollTask for Lat {
            fn poll_once(&mut self, _: Tsc) -> Polled {
                match self.rx.try_pop() {
                    Some(sent) => {
                        let d = now_ns() - sent;
                        self.worst.fetch_max(d, Ordering::AcqRel);
                        self.count.fetch_add(1, Ordering::AcqRel);
                        Polled { did_work: true }
                    }
                    None => Polled { did_work: false },
                }
            }
            fn has_pending(&self) -> bool {
                self.rx.len_hint() > 0
            }
        }
        let task = Lat {
            rx,
            worst: lat.clone(),
            count: count.clone(),
        };
        let st = stop.clone();
        let h = std::thread::spawn(move || {
            let hb = Heartbeat::new();
            let mut tl = TaskLoop::new([task], IdlePolicy::Park { park_us: 2_000 }, Some(parker), &c1g());
            tl.run(&st, &hb);
        });
        register_traced_thread();
        mark();
        for _ in 0..20 {
            std::thread::sleep(Duration::from_millis(50));
            p.try_push(now_ns()).unwrap();
        }
        mark();
        while count.load(Ordering::Acquire) < 20 {
            std::thread::sleep(Duration::from_millis(5));
        }
        stop.store(true, Ordering::Release);
        h.join().unwrap();
        let worst = lat.load(Ordering::Acquire);
        assert!(worst <= 7_000_000, "worst latency {worst} ns");
        return;
    }
    let segs = strace_segments("rt_120_spin_producer_no_wake", Some("futex,write"));
    assert_eq!(segs.len(), 1, "{segs:?}");
    assert_eq!((n(&segs[0], "futex"), n(&segs[0], "write")), (0, 0), "{segs:?}");
}

struct Pending {
    polls: Vec<Instant>,
    pending_calls: std::cell::Cell<u32>,
    stop: Arc<AtomicBool>,
}

impl PollTask for Pending {
    fn poll_once(&mut self, _: Tsc) -> Polled {
        self.polls.push(Instant::now());
        if self.polls.len() == 6 {
            self.stop.store(true, Ordering::Release);
        }
        Polled { did_work: false }
    }
    fn has_pending(&self) -> bool {
        let c = self.pending_calls.get() + 1;
        self.pending_calls.set(c);
        c <= 3
    }
}

#[test]
fn rt_122_pending_cancels_park() {
    let stop = Arc::new(AtomicBool::new(false));
    let parker = Parker::new(ParkBackend::Futex).unwrap();
    let task = Pending {
        polls: Vec::new(),
        pending_calls: std::cell::Cell::new(0),
        stop: stop.clone(),
    };
    let mut tl = TaskLoop::new(
        [task],
        IdlePolicy::Park { park_us: 100_000 },
        Some(parker),
        &c1g(),
    );
    let hb = Heartbeat::new();
    let t0 = Instant::now();
    tl.run(&stop, &hb);
    assert!(t0.elapsed() < Duration::from_secs(1));
    let p = &tl.tasks()[0].polls;
    assert_eq!(p.len(), 6);
    for k in 0..3 {
        assert!(
            p[k + 1] - p[k] < Duration::from_millis(10),
            "interval {}→{}",
            k + 1,
            k + 2
        );
    }
    assert!(p[4] - p[3] >= Duration::from_millis(100), "interval 4→5");
}
