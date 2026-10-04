//! Cold copy damaged by power loss: recovery keeps exactly the intact complete events (JNL-073).

mod common;

use common::harness::*;
use hft_journal::testing;
use hft_journal::*;
use hft_types::Tsc;
use proptest::prelude::*;

fn masked(b: &[u8]) -> Vec<u8> {
    let mut v = b.to_vec();
    for x in v.iter_mut().take(3200.min(b.len())).skip(3072) {
        *x = 0;
    }
    v
}

#[derive(Clone, Debug)]
enum Damage {
    Truncate(usize),
    ZeroPage(usize),
    RandomPage(usize, u64),
}

fn run(events: &[u8], damage_at: f64, kind: u8, seed: u64) {
    let mut cfg = c0(&mem_hot(), &cold_dir());
    cfg.segment_bytes = 8 << 20;
    let mut h = H::open0(cfg);
    // Every record carries a CRC, so any damaged byte is detectable.
    for (i, n) in events.iter().enumerate() {
        for k in 0..*n {
            h.w()
                .append(F0, Tsc(i as u64), &tcrc(k.wrapping_add(i as u8)))
                .unwrap();
        }
        h.w().end_event();
        h.w().publish();
    }
    h.persist_all();
    let c = h.committed(1) as usize;
    assert_eq!(h.synced(1) as usize, c);
    let hot = h.seg(1);
    let stem = h.cold_stem(1);
    let jrn = stem.with_extension("jrn");
    // Damage starts after the boot block of segment 1 (4 240), anywhere in the written data.
    let span = c - 4240;
    let off = (4240 + ((span as f64 * damage_at) as usize).min(span - 1)) & !7;
    let damage = match kind % 3 {
        0 => Damage::Truncate(off),
        1 => Damage::ZeroPage(off),
        _ => Damage::RandomPage(off, seed),
    };
    let mut cold = std::fs::read(&jrn).unwrap();
    let start = match damage {
        Damage::Truncate(o) => {
            cold.truncate(o);
            o
        }
        Damage::ZeroPage(p) => {
            cold[p..p + 4096].fill(0);
            p
        }
        Damage::RandomPage(p, s) => {
            let mut x = s | 1;
            for b in &mut cold[p..p + 4096] {
                x ^= x << 13;
                x ^= x >> 7;
                x ^= x << 17;
                *b = x as u8;
            }
            p
        }
    };
    std::fs::write(&jrn, &cold).unwrap();
    for p in testing::hot_files(&h.hot_jdir()) {
        testing::unlink(&p).unwrap();
    }
    // Expected cut: after the last EVENT_END record lying wholly before the damage.
    let mut cut = 4096;
    for (o, r) in records(&hot, c) {
        if o + r.len as usize <= start && r.flags & EVENT_END != 0 {
            cut = o + r.len as usize;
        }
    }
    h.crash();
    let rep = h.open().unwrap();
    let after = std::fs::read(&jrn).unwrap();
    assert_eq!(after.len(), cut + 72, "{damage:?}");
    assert_eq!(masked(&after[..cut]), masked(&hot[..cut]), "{damage:?}");
    assert_eq!(roll_body(&after, cut).reason, 4);
    assert!(rep.gap_possible);
    h.roll(rep.boot_reason);
    assert_eq!(h.hdr(2).flags, RECOVERY_GAP);
    h.events(1);
    h.persist_all();
    let v = verify(&h.cold_jdir(), &VerifyOpts::default());
    assert!(v.issues.is_empty(), "{:?}", v.issues);
    assert_eq!(v.recovery_gaps, vec![2]);
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(128))]
    #[test]
    fn jnl_073_cold_corruption(
        events in proptest::collection::vec(1u8..=3, 1..=5000),
        at in 0.0f64..1.0,
        kind in 0u8..3,
        seed in any::<u64>(),
    ) {
        run(&events, at, kind, seed);
    }
}
