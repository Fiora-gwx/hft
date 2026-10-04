//! loom: every interleaving of push / batch push against pop / batch read delivers 1, 2, 3 exactly
//! once and in order; slot accesses are race-checked by loom (RT-008).
//! Run: `RUSTFLAGS="--cfg loom" cargo test -p hft-rt --test loom_spsc --release`.

#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods
)] // allow-list: test code, not a hot path
#![cfg(loom)]

use hft_rt::{Backing, spsc};

#[test]
fn rt_008_loom_spsc() {
    loom::model(|| {
        let (mut p, mut c) = spsc::<u64>(2, Backing::Heap).unwrap();
        let prod = loom::thread::spawn(move || {
            while p.try_push(1).is_err() {
                loom::thread::yield_now();
            }
            let mut next = 2u64;
            while next <= 3 {
                let mut b = p.write_batch(2);
                while next <= 3 && b.push(next) {
                    next += 1;
                }
                b.commit();
                if next <= 3 {
                    loom::thread::yield_now();
                }
            }
        });
        let mut got = Vec::new();
        let mut use_batch = false;
        while got.len() < 3 {
            if use_batch {
                let b = c.read_batch(2);
                let (x, y) = b.slices();
                got.extend(x.iter().chain(y.iter()).copied());
                let empty = x.is_empty();
                b.release();
                if empty {
                    loom::thread::yield_now();
                }
            } else {
                match c.try_pop() {
                    Some(v) => got.push(v),
                    None => loom::thread::yield_now(),
                }
            }
            use_batch = !use_batch;
        }
        prod.join().unwrap();
        assert_eq!(got, vec![1, 2, 3]);
        assert_eq!(c.try_pop(), None);
    });
}
