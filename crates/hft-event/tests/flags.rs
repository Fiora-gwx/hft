//! `OrderFlags` (EVT-051 – EVT-053).

mod common;

use common::*;
use hft_event::*;

#[test]
fn evt_051_order_flags() {
    let six = [
        ("EVT-051.1", OrderFlags::REDUCE_ONLY, 0x0001),
        ("EVT-051.2", OrderFlags::POST_ONLY, 0x0002),
        ("EVT-051.3", OrderFlags::LINKED, 0x0004),
        ("EVT-051.4", OrderFlags::HEDGE_FIX, 0x0008),
        ("EVT-051.5", OrderFlags::RISK_REDUCING, 0x0010),
        ("EVT-051.6", OrderFlags::SMOKE, 0x0020),
    ];
    for (row, f, v) in six {
        assert_eq!(f.0, v, "{row}");
        assert_eq!(f.0 & !OrderFlags::WIRE_MASK.0, 0, "EVT-051.8 {row}");
    }
    assert_eq!(OrderFlags::WIRE_MASK.0, 0x00FF, "EVT-051.7");
    for (i, a) in six.iter().enumerate() {
        for b in &six[i + 1..] {
            assert_eq!(a.1.0 & b.1.0, 0, "EVT-051.8");
        }
    }
}

fn encode_oflags(oflags: u16) -> Vec<u8> {
    let mut od = order_decision();
    od.oflags = oflags;
    let mut out = vec![0u8; 112];
    let n = encode_fixed(&mut out, 1, 1, RecordFlags(0), &od);
    out.truncate(n as usize);
    out
}

#[test]
fn evt_052_oflags_reserved_bits_assert() {
    if cfg!(debug_assertions) {
        assert!(
            std::panic::catch_unwind(|| encode_oflags(0x0100)).is_err(),
            "EVT-052.1"
        );
        assert!(
            std::panic::catch_unwind(|| encode_oflags(0x8000)).is_err(),
            "EVT-052.2"
        );
    }
    let r = encode_oflags(0x00FF);
    assert_eq!(r.len(), 112, "EVT-052.3");
    assert_eq!(&r[24 + 62..24 + 64], &[0xFF, 0x00], "EVT-052.3");
    let r = encode_oflags(0x0030);
    let od = decode_record(&r).unwrap().read::<OrderDecision>().unwrap();
    assert_eq!(od.oflags, 0x0030, "EVT-052.4");
}

#[cfg(not(debug_assertions))]
#[test]
fn evt_053_oflags_release_no_check() {
    let r = encode_oflags(0x0100);
    let od = decode_record(&r).unwrap().read::<OrderDecision>().unwrap();
    assert_eq!(od.oflags, 0x0100);
}
