//! Criterion benchmarks for hft-rt (RT-150 – RT-165); ids equal the `基准名` fields in TESTS.md
//! and the budget names in README "延迟与资源契约" / "线程计划的执行".

#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods
)] // allow-list: test code, not a hot path

use std::hint::black_box;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{Duration, Instant};

use criterion::{Criterion, Throughput, criterion_group, criterion_main};
use hft_rt::*;

type Elem = [u64; 8]; // 64 B element

fn c1g() -> TscClock {
    TscClock::from_hz_milli(1_000_000_000_000, CalibPair::default())
}

/// Ping-pong over two rings; returns the total time of `iters` round trips divided by 2 (one hop).
fn ping_pong<P, C>(iters: u64, mut tx: P, mut rx: C, echo: impl FnOnce() + Send + 'static) -> Duration
where
    P: FnMut(u64) -> bool,
    C: FnMut() -> Option<u64>,
{
    let h = std::thread::spawn(echo);
    let t0 = Instant::now();
    for i in 0..iters {
        while !tx(i) {
            cpu_relax();
        }
        loop {
            if rx().is_some() {
                break;
            }
            cpu_relax();
        }
    }
    let el = t0.elapsed();
    while !tx(u64::MAX) {
        cpu_relax();
    }
    let _ = h.join();
    el / 2
}

fn rt_150_spsc_hop(c: &mut Criterion) {
    c.bench_function("rt_spsc_hop", |b| {
        b.iter_custom(|iters| {
            let (mut p1, mut c1) = spsc::<Elem>(1024, Backing::Heap).unwrap();
            let (mut p2, mut c2) = spsc::<Elem>(1024, Backing::Heap).unwrap();
            let echo = move || {
                loop {
                    if let Some(v) = c1.try_pop() {
                        if v[0] == u64::MAX {
                            return;
                        }
                        while p2.try_push(v).is_err() {
                            cpu_relax();
                        }
                    } else {
                        cpu_relax();
                    }
                }
            };
            ping_pong(
                iters,
                move |i| p1.try_push([i; 8]).is_ok(),
                move || c2.try_pop().map(|v| v[0]),
                echo,
            )
        })
    });
    // Comparison: rtrb with the same element and capacity.
    c.bench_function("rt_spsc_hop_rtrb", |b| {
        b.iter_custom(|iters| {
            let (mut p1, mut c1) = rtrb::RingBuffer::<Elem>::new(1024);
            let (mut p2, mut c2) = rtrb::RingBuffer::<Elem>::new(1024);
            let echo = move || {
                loop {
                    if let Ok(v) = c1.pop() {
                        if v[0] == u64::MAX {
                            return;
                        }
                        while p2.push(v).is_err() {
                            cpu_relax();
                        }
                    } else {
                        cpu_relax();
                    }
                }
            };
            ping_pong(
                iters,
                move |i| p1.push([i; 8]).is_ok(),
                move || c2.pop().ok().map(|v| v[0]),
                echo,
            )
        })
    });
}

fn rt_151_spsc_push_pop(c: &mut Criterion) {
    let (mut p, mut q) = spsc::<Elem>(1024, Backing::Heap).unwrap();
    c.bench_function("rt_spsc_push_pop", |b| {
        b.iter(|| {
            let _ = p.try_push(black_box([1; 8]));
            black_box(q.try_pop())
        })
    });
}

fn throughput<T: Copy + Send + 'static>(iters: u64, v: T) -> Duration {
    let (mut p, mut q) = spsc::<T>(4096, Backing::Heap).unwrap();
    let h = std::thread::spawn(move || {
        let mut got = 0u64;
        while got < iters {
            let b = q.read_batch(256);
            let (x, y) = b.slices();
            let n = (x.len() + y.len()) as u64;
            black_box((x, y));
            b.release();
            got += n;
            if n == 0 {
                cpu_relax();
            }
        }
    });
    let t0 = Instant::now();
    let mut sent = 0u64;
    while sent < iters {
        let mut b = p.write_batch(256.min((iters - sent) as usize));
        while b.push(v) {
            sent += 1;
        }
        b.commit();
    }
    let _ = h.join();
    t0.elapsed()
}

fn rt_152_spsc_throughput(c: &mut Criterion) {
    let mut g = c.benchmark_group("rt_spsc_throughput");
    g.throughput(Throughput::Elements(1));
    g.bench_function("8B", |b| b.iter_custom(|iters| throughput::<u64>(iters, 7)));
    g.bench_function("128B", |b| {
        b.iter_custom(|iters| throughput::<[u64; 16]>(iters, [7; 16]))
    });
    g.finish();
}

fn rt_153_shm_spsc_hop(c: &mut Criterion) {
    #[cfg(target_os = "linux")]
    {
        let dir = std::path::PathBuf::from(format!("/dev/shm/hft-bench-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        c.bench_function("rt_shm_spsc_hop", |b| {
            b.iter_custom(|iters| {
                let (a, z) = (dir.join("a"), dir.join("b"));
                let mk = |p: &std::path::Path, side| {
                    ShmSpsc::create::<Elem>(p, 1024, PageSize::Normal, side, 1).unwrap()
                };
                let mut p1 = mk(&a, RingSide::Producer).into_producer().unwrap();
                let mut c1 = ShmSpsc::attach::<Elem>(&a, RingSide::Consumer, 1)
                    .unwrap()
                    .into_consumer()
                    .unwrap();
                let mut c2 = mk(&z, RingSide::Consumer).into_consumer().unwrap();
                let mut p2 = ShmSpsc::attach::<Elem>(&z, RingSide::Producer, 1)
                    .unwrap()
                    .into_producer()
                    .unwrap();
                let echo = move || {
                    loop {
                        if let Some(v) = c1.try_pop() {
                            if v[0] == u64::MAX {
                                return;
                            }
                            while p2.try_push(v).is_err() {
                                cpu_relax();
                            }
                        } else {
                            cpu_relax();
                        }
                    }
                };
                ping_pong(
                    iters,
                    move |i| p1.try_push([i; 8]).is_ok(),
                    move || c2.try_pop().map(|v| v[0]),
                    echo,
                )
            })
        });
        let _ = std::fs::remove_dir_all(&dir);
    }
    #[cfg(not(target_os = "linux"))]
    let _ = c;
}

fn rt_154_bytes_hop_1k(c: &mut Criterion) {
    c.bench_function("rt_bytes_hop_1k", |b| {
        b.iter_custom(|iters| {
            let (mut p1, mut c1) = byte_spsc(1 << 18, Backing::Heap).unwrap();
            let (mut p2, mut c2) = byte_spsc(1 << 18, Backing::Heap).unwrap();
            let echo = move || {
                loop {
                    let stop = match c1.peek() {
                        Some(r) => {
                            let stop = r[0] == 0xFF;
                            if !stop {
                                let buf = loop {
                                    if let Some(buf) = p2.reserve(1024) {
                                        break buf;
                                    }
                                    cpu_relax();
                                };
                                buf.copy_from_slice(r);
                                p2.commit();
                            }
                            stop
                        }
                        None => {
                            cpu_relax();
                            continue;
                        }
                    };
                    c1.advance();
                    if stop {
                        return;
                    }
                }
            };
            let tx = move |i: u64| match p1.reserve(1024) {
                Some(buf) => {
                    buf.fill(if i == u64::MAX { 0xFF } else { 1 });
                    p1.commit();
                    true
                }
                None => false,
            };
            let rx = move || {
                let got = c2.peek().map(|r| r.len() as u64);
                if got.is_some() {
                    c2.advance();
                }
                got
            };
            ping_pong(iters, tx, rx, echo)
        })
    });
}

fn rt_155_tsc_now(c: &mut Criterion) {
    c.bench_function("rt_tsc_now", |b| b.iter(tsc_now));
    c.bench_function("rt_tsc_ordered", |b| b.iter(tsc_ordered));
}

fn rt_156_killword_load(c: &mut Criterion) {
    let kw: KillWord = CachePadded(AtomicU32::new(0x0010_0107));
    c.bench_function("rt_killword_load", |b| {
        b.iter(|| black_box(&kw).load(Ordering::Acquire))
    });
}

fn rt_157_tsc_delta(c: &mut Criterion) {
    let clk = TscClock::from_hz_milli(2_500_000_000_000, CalibPair::default());
    c.bench_function("rt_tsc_delta", |b| {
        b.iter(|| clk.delta_ns(black_box(Tsc(1_000)), black_box(Tsc(123_456_789))))
    });
}

fn rt_158_mailbox(c: &mut Criterion) {
    let mb: &'static Mailbox<Box<u64>> = Box::leak(Box::new(Mailbox::new()));
    let (_tx, mut rx) = mb.split();
    c.bench_function("rt_mailbox", |b| b.iter(|| black_box(rx.try_take())));
}

fn rt_159_seqcell(c: &mut Criterion) {
    let clk = TscClock::from_hz_milli(2_500_000_000_000, CalibPair::default());
    let cell = SeqCell::new(clk.tsc_utc(&CalibPair::default()));
    c.bench_function("rt_seqcell", |b| b.iter(|| black_box(&cell).load()));
}

fn rt_160_heartbeat(c: &mut Criterion) {
    let hb = Heartbeat::new();
    c.bench_function("rt_heartbeat", |b| b.iter(|| black_box(&hb).beat()));
}

fn rt_161_pause_cost(c: &mut Criterion) {
    c.bench_function("rt_pause_cost", |b| b.iter(cpu_relax));
}

fn rt_162_sample_pair(c: &mut Criterion) {
    let clk = TscClock::from_hz_milli(2_500_000_000_000, CalibPair::default());
    c.bench_function("rt_sample_pair", |b| {
        b.iter(|| sample_pair(black_box(&clk), None))
    });
}

struct RingTask {
    rx: Consumer<u64>,
}

impl PollTask for RingTask {
    fn poll_once(&mut self, _: Tsc) -> Polled {
        Polled {
            did_work: self.rx.try_pop().is_some(),
        }
    }
    fn has_pending(&self) -> bool {
        self.rx.len_hint() > 0
    }
}

fn rt_163_taskloop_idle_round(c: &mut Criterion) {
    let mut keep = Vec::new();
    let tasks: [RingTask; 4] = core::array::from_fn(|_| {
        let (p, rx) = spsc::<u64>(64, Backing::Heap).unwrap();
        keep.push(p);
        RingTask { rx }
    });
    let mut tl = TaskLoop::new(tasks, IdlePolicy::Spin, None, &c1g());
    c.bench_function("rt_taskloop_idle_round", |b| {
        b.iter(|| {
            let now = tsc_now();
            let did = tl.round(now);
            tl.idle_action(now, did)
        })
    });
}

fn rt_164_wake_check(c: &mut Criterion) {
    let st = ParkState::new();
    c.bench_function("rt_wake_check", |b| b.iter(|| black_box(&st).notify()));
}

fn rt_165_unpark_latency(c: &mut Criterion) {
    #[cfg(target_os = "linux")]
    {
        let mut g = c.benchmark_group("rt_unpark_latency");
        g.sample_size(10);
        for (name, backend) in [("futex", ParkBackend::Futex), ("epoll", ParkBackend::Epoll)] {
            g.bench_function(name, |b| b.iter_custom(|iters| unpark_run(iters, backend)));
        }
        g.finish();
    }
    #[cfg(not(target_os = "linux"))]
    let _ = c;
}

#[cfg(target_os = "linux")]
fn unpark_run(iters: u64, backend: ParkBackend) -> Duration {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, AtomicU64};
    struct Echo {
        rx: Consumer<u64>,
        seen: Arc<AtomicU64>,
    }
    impl PollTask for Echo {
        fn poll_once(&mut self, _: Tsc) -> Polled {
            match self.rx.try_pop() {
                Some(_) => {
                    self.seen.store(tsc_now().0, Ordering::Release);
                    Polled { did_work: true }
                }
                None => Polled { did_work: false },
            }
        }
        fn has_pending(&self) -> bool {
            self.rx.len_hint() > 0
        }
    }
    let parker = Parker::new(backend).unwrap();
    let (mut p, rx) = spsc::<u64>(64, Backing::Heap).unwrap();
    p.set_wake(Some(parker.unparker()));
    let seen = Arc::new(AtomicU64::new(0));
    let stop = Arc::new(AtomicBool::new(false));
    let (s2, st2) = (seen.clone(), stop.clone());
    let h = std::thread::spawn(move || {
        let hb = Heartbeat::new();
        let mut tl = TaskLoop::new(
            [Echo { rx, seen: s2 }],
            IdlePolicy::Park { park_us: 100_000 },
            Some(parker),
            &c1g(),
        );
        tl.run(&st2, &hb);
    });
    let mut total_ns = 0u64;
    for i in 0..iters {
        std::thread::sleep(Duration::from_micros(500)); // let the consumer park
        seen.store(0, Ordering::Release);
        let t0 = tsc_now();
        let _ = p.try_push(i);
        let t1 = loop {
            let s = seen.load(Ordering::Acquire);
            if s != 0 {
                break Tsc(s);
            }
            cpu_relax();
        };
        total_ns += ticks_to_ns(t0, t1);
    }
    stop.store(true, Ordering::Release);
    let _ = p.try_push(u64::MAX);
    let _ = h.join();
    Duration::from_nanos(total_ns)
}

#[cfg(target_os = "linux")]
fn ticks_to_ns(a: Tsc, b: Tsc) -> u64 {
    let hz = tsc_hz_nominal().unwrap_or(1_000_000_000);
    let clk = TscClock::from_hz_milli(hz.saturating_mul(1000), CalibPair::default());
    clk.delta_ns(a, b)
}

criterion_group!(
    benches,
    rt_150_spsc_hop,
    rt_151_spsc_push_pop,
    rt_152_spsc_throughput,
    rt_153_shm_spsc_hop,
    rt_154_bytes_hop_1k,
    rt_155_tsc_now,
    rt_156_killword_load,
    rt_157_tsc_delta,
    rt_158_mailbox,
    rt_159_seqcell,
    rt_160_heartbeat,
    rt_161_pause_cost,
    rt_162_sample_pair,
    rt_163_taskloop_idle_round,
    rt_164_wake_check,
    rt_165_unpark_latency,
);
criterion_main!(benches);
