//! Symbol tables: exact-byte lookup, aliases, numeric ids, front table, prewarm
//! (REF-045, REF-046, REF-048, REF-049).

#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods
)] // allow-list: test code, not a hot path

use crate::common;

use common::*;
use hft_refdata::SymbolIndex;
use hft_types::{InstId, VenueId};

#[test]
fn ref_045_symbol_get() {
    let rd = ok(&f0());
    let s = |v: u8| rd.symbols(VenueId(v));
    let cases: [(&str, u8, &[u8], Option<u32>); 10] = [
        ("REF-045.1", 1, b"BTCUSDT", Some(1)),
        ("REF-045.2", 1, b"1000PEPEUSDT", Some(5)),
        ("REF-045.3", 1, b"btcusdt", None),
        ("REF-045.4", 1, b"BTCUSD", None),
        ("REF-045.5", 1, b"BTCUSDTX", None),
        ("REF-045.6", 1, b"", None),
        ("REF-045.7", 2, b"BTC_USDT", Some(2)),
        ("REF-045.8", 1, b"BTC_USDT", None),
        ("REF-045.9", 3, b"BTCUSDT", None),
        ("REF-045.10", 1, &[b'A'; 40], None),
    ];
    for (case, v, key, want) in cases {
        assert_eq!(s(v).get(key), want.map(InstId), "{case}");
        assert_eq!(s(v).get_sorted(key), want.map(InstId), "{case} (sorted path)");
    }
}

#[test]
fn ref_046_symbol_get_num() {
    let rd = ok(&f0());
    let s = |v: u8| rd.symbols(VenueId(v));
    assert_eq!(s(5).get_num(10_459), Some(InstId(3)), "REF-046.1");
    assert_eq!(s(6).get_num(20_001), Some(InstId(4)), "REF-046.2");
    assert_eq!(s(6).get_num(10_459), None, "REF-046.3");
    assert_eq!(s(1).get_num(0), None, "REF-046.4");
    assert_eq!(s(5).get_num(u64::MAX), None, "REF-046.5");
}

fn table(n: usize) -> SymbolIndex {
    let keys: Vec<String> = (0..n).map(|k| format!("S{k}")).collect();
    let syms: Vec<(&[u8], InstId)> = keys
        .iter()
        .enumerate()
        .map(|(k, s)| (s.as_bytes(), InstId(k as u32 + 1)))
        .collect();
    SymbolIndex::from_entries(&syms, &[])
}

#[test]
fn ref_048_front_table_capacity() {
    let cases: [(&str, usize, usize); 4] = [
        ("REF-048.1", 1, 2),
        ("REF-048.2", 3, 8),
        ("REF-048.3", 4_096, 8_192),
        ("REF-048.4", 5_000, 16_384),
    ];
    for (case, n, cap) in cases {
        let (a, b) = (table(n), table(n));
        assert_eq!(a.front_table().len(), cap, "{case}");
        assert_eq!(a.front_table(), b.front_table(), "{case}: two builds");
        assert_eq!(a, b, "{case}: two builds");
    }
}

#[test]
fn ref_049_prewarm() {
    let rd1 = ok(&f0());
    let rd2 = ok(&f0());
    count_start();
    let a = rd1.symbols(VenueId(1)).prewarm();
    let b = rd1.symbols(VenueId(1)).prewarm();
    let c = rd2.symbols(VenueId(1)).prewarm();
    let d = rd1.symbols(VenueId(6)).prewarm();
    let (allocs, _) = count_stop();
    assert_eq!(a, b);
    assert_eq!(a, c);
    assert_ne!(a, d);
    assert_eq!(allocs, 0);
}
