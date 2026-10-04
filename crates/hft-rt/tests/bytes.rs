//! ByteSpsc capacity, records, size limit, wrap marker, reservation overwrite (RT-030 – RT-034).
#![cfg(not(loom))]

use hft_rt::{Backing, RtErr, byte_spsc};

#[test]
fn rt_030_byte_capacity() {
    let rows: [(&str, usize, Result<(), RtErr>); 5] = [
        ("RT-030.1", 32, Err(RtErr::Capacity)),
        ("RT-030.2", 63, Err(RtErr::Capacity)),
        ("RT-030.3", 64, Ok(())),
        ("RT-030.4", 100, Err(RtErr::Capacity)),
        ("RT-030.5", 262_144, Ok(())),
    ];
    for (row, cap, want) in rows {
        assert_eq!(byte_spsc(cap, Backing::Heap).map(|_| ()), want, "{row}");
    }
}

#[test]
fn rt_031_records_round_trip() {
    let (mut p, mut c) = byte_spsc(256, Backing::Heap).unwrap();
    let r = p.reserve(10).unwrap();
    assert_eq!(r.len(), 10);
    for (i, b) in r.iter_mut().enumerate() {
        *b = i as u8;
    }
    p.commit();
    assert_eq!(p.reserve(0).map(|r| r.len()), Some(0));
    p.commit();
    assert_eq!(c.peek(), Some(&[0u8, 1, 2, 3, 4, 5, 6, 7, 8, 9][..]));
    c.advance();
    assert_eq!(c.peek(), Some(&[][..]));
    c.advance();
    assert_eq!(c.peek(), None);
}

#[test]
fn rt_032_size_limit_and_full() {
    let (mut p, mut c) = byte_spsc(256, Backing::Heap).unwrap();
    assert!(p.reserve(65).is_none());
    assert_eq!(p.reserve(64).map(|r| r.len()), Some(64));
    p.commit();
    let mut ok = 0;
    while p.reserve(56).is_some() {
        p.commit();
        ok += 1;
        assert!(ok < 10);
    }
    assert_eq!(ok, 2);
    c.advance();
    assert!(p.reserve(56).is_some());
}

fn rec(tag: u8) -> [u8; 48] {
    let mut r = [0u8; 48];
    for (i, b) in r.iter_mut().enumerate() {
        *b = tag.wrapping_mul(31).wrapping_add(i as u8);
    }
    r
}

#[test]
fn rt_033_wrap_marker() {
    let (mut p, mut c) = byte_spsc(256, Backing::Heap).unwrap();
    for t in 1..=4u8 {
        p.reserve(48).unwrap().copy_from_slice(&rec(t));
        p.commit();
    }
    for t in 1..=2u8 {
        assert_eq!(c.peek(), Some(&rec(t)[..]));
        c.advance();
    }
    let r5 = p.reserve(48);
    assert!(r5.is_some());
    r5.unwrap().copy_from_slice(&rec(5));
    p.commit();
    for t in [3u8, 4, 5] {
        assert_eq!(c.peek(), Some(&rec(t)[..]));
        c.advance();
    }
    assert_eq!(c.peek(), None);
}

#[test]
fn rt_034_reserve_overwrite() {
    let (mut p, mut c) = byte_spsc(256, Backing::Heap).unwrap();
    p.commit();
    assert_eq!(c.peek(), None);
    p.reserve(8).unwrap().copy_from_slice(&[1; 8]);
    p.reserve(4).unwrap().copy_from_slice(&[2; 4]);
    p.commit();
    assert_eq!(c.peek(), Some(&[2u8, 2, 2, 2][..]));
    c.advance();
    assert_eq!(c.peek(), None);
}
