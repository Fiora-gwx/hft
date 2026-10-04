//! Local retention of cold copies (JNL-091).

mod common;

use common::harness::*;
use hft_journal::*;

const DAY: i64 = 86_400_000_000_000;

/// Segment 1 sealed at `UTC0` (alone in its day directory); segment 2 on the next day.
fn sealed_one() -> H {
    let mut h = H::open0(linux_c0());
    h.events(3);
    h.utc = UTC0 + DAY;
    h.roll(RollReason::Hourly);
    h.utc = UTC0;
    h.persist_all();
    h
}

#[test]
fn jnl_091_cold_retire() {
    for (row, uploaded, require, age, deleted) in [
        ("JNL-091.1", true, true, 7 * DAY + 1, true),
        ("JNL-091.2", true, true, 7 * DAY, false),
        ("JNL-091.3", false, true, 30 * DAY, false),
        ("JNL-091.4", false, false, 7 * DAY + 1, true),
        ("JNL-091.5", true, true, 30 * DAY, false),
    ] {
        let h = sealed_one();
        let stem = h.cold_stem(1);
        let (jrn, seal, up) = (
            stem.with_extension("jrn"),
            stem.with_extension("seal"),
            stem.with_extension("uploaded"),
        );
        assert!(seal.exists(), "{row}");
        if uploaded {
            std::fs::write(&up, b"").unwrap();
        }
        if row == "JNL-091.5" {
            std::fs::remove_file(&seal).unwrap();
        }
        let r = ColdRetention {
            keep_days: 7,
            require_uploaded: require,
        };
        let rep = cold_retire(&h.cold_jdir(), &r, UTC0 + age).unwrap();
        assert_eq!(rep.segments, u32::from(deleted), "{row}");
        assert_eq!(jrn.exists(), !deleted, "{row}");
        if deleted {
            assert!(!seal.exists() && !up.exists(), "{row}");
            assert!(
                !stem.parent().unwrap().exists(),
                "{row}: empty day directory removed"
            );
        }
    }
}
