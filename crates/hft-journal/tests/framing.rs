//! Record framing (JNL-009).

mod common;

use common::harness::*;
use hft_event::record_len;
use hft_types::Tsc;

#[test]
fn jnl_009_record_len() {
    assert_eq!(record_len(0, false), 24, "JNL-009.1");
    assert_eq!(record_len(1, false), 32, "JNL-009.2");
    assert_eq!(record_len(80, false), 104, "JNL-009.3");
    assert_eq!(record_len(40, true), 72, "JNL-009.4");
    assert_eq!(record_len(65_512, false), 65_536, "JNL-009.5");
    let mut h = H::open0(mem_c0());
    let mut starts = Vec::new();
    starts.push(h.w().offset());
    h.w().append(F0, Tsc(1), &tin(1)).unwrap();
    starts.push(h.w().offset());
    h.w().append(F0, Tsc(1), &tcrc(2)).unwrap();
    starts.push(h.w().offset());
    h.w().append(F0, Tsc(1), &tmeta(3)).unwrap();
    assert_eq!(starts, vec![4240, 4344, 4416], "JNL-009.6");
    assert!(starts.iter().all(|s| s % 8 == 0), "JNL-009.6");
}
