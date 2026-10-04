//! Two-thread stress: SPSC sequence, byte records, kill word (RT-009, RT-036, RT-046).
//!
//! Scale: `HFT_RT_STRESS_SCALE` multiplies the PR counts (the nightly arm job sets 10).
#![cfg(not(loom))]

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::time::{Duration, Instant};

use hft_rt::{Backing, CachePadded, KillWord, byte_spsc, cpu_relax, spsc};

fn scale() -> u64 {
    std::env::var("HFT_RT_STRESS_SCALE")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(1)
}

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        // xorshift64*
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
}

#[test]
fn rt_009_two_thread_stress() {
    let n: u64 = 100_000_000 * scale();
    for cap in [2usize, 16, 1024, 65536] {
        let t0 = Instant::now();
        let (mut p, mut c) = spsc::<u64>(cap, Backing::Heap).unwrap();
        let prod = std::thread::spawn(move || {
            let mut rng = Rng(0x9E37_79B9_7F4A_7C15 ^ cap as u64);
            let mut next = 0u64;
            while next < n {
                let k = (rng.next() % 64 + 1).min(n - next) as usize;
                let mut b = p.write_batch(k);
                while next < n && b.push(next) {
                    next += 1;
                }
                b.commit();
                if next < n {
                    cpu_relax();
                }
            }
        });
        let mut rng = Rng(0xD1B5_4A32_D192_ED03 ^ cap as u64);
        let mut want = 0u64;
        while want < n {
            let k = (rng.next() % 64 + 1) as usize;
            let b = c.read_batch(k);
            let (x, y) = b.slices();
            for v in x.iter().chain(y.iter()) {
                assert_eq!(*v, want, "capacity {cap}");
                want += 1;
            }
            let empty = x.is_empty();
            b.release();
            if empty {
                cpu_relax();
            }
        }
        prod.join().unwrap();
        assert_eq!(c.try_pop(), None);
        assert_eq!(want, n);
        assert!(
            t0.elapsed() <= Duration::from_secs(120) * scale() as u32,
            "capacity {cap} too slow"
        );
    }
}

fn fill(seq: u64, buf: &mut [u8]) {
    let mut r = Rng(seq.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1);
    for ch in buf.chunks_mut(8) {
        let w = r.next().to_le_bytes();
        ch.copy_from_slice(&w[..ch.len()]);
    }
}

fn len_of(seq: u64) -> u32 {
    (Rng(seq.wrapping_add(0x1234_5678) | 1).next() % 65_537) as u32
}

#[test]
fn rt_036_bytes_stress() {
    let n: u64 = 100_000 * scale();
    let (mut p, mut c) = byte_spsc(262_144, Backing::Heap).unwrap();
    let prod = std::thread::spawn(move || {
        for seq in 0..n {
            let len = len_of(seq);
            loop {
                if let Some(buf) = p.reserve(len) {
                    fill(seq, buf);
                    break;
                }
                cpu_relax();
            }
            p.commit();
        }
    });
    let mut want = vec![0u8; 65_536];
    let mut seq = 0u64;
    while seq < n {
        match c.peek() {
            Some(r) => {
                let len = len_of(seq) as usize;
                assert_eq!(r.len(), len, "record {seq}");
                fill(seq, &mut want[..len]);
                assert!(r == &want[..len], "record {seq} content");
                c.advance();
                seq += 1;
            }
            None => cpu_relax(),
        }
    }
    prod.join().unwrap();
    assert_eq!(c.peek(), None);
}

#[test]
fn rt_046_killword_threads() {
    const SET: [u32; 4] = [0, 0x0000_0100, 0x0010_0000, 0xFFFF_FFFF];
    let n: u64 = 10_000_000 * scale();
    let kw: &'static KillWord = Box::leak(Box::new(CachePadded(AtomicU32::new(0))));
    let go: &'static AtomicBool = Box::leak(Box::new(AtomicBool::new(false)));
    let w = std::thread::spawn(move || {
        while !go.load(Ordering::Acquire) {
            cpu_relax();
        }
        for i in 0..n {
            kw.store(SET[(i % 4) as usize], Ordering::Release);
        }
    });
    go.store(true, Ordering::Release);
    let mut seen = [0u64; 4];
    for _ in 0..n {
        let v = kw.load(Ordering::Acquire);
        let i = SET
            .iter()
            .position(|s| *s == v)
            .unwrap_or_else(|| panic!("unwritten value {v:#x}"));
        seen[i] += 1;
    }
    w.join().unwrap();
    assert_eq!(seen.iter().sum::<u64>(), n);
}
