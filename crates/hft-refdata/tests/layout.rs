//! Container and hot-row layouts (REF-001, REF-002, REF-074).

#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods
)] // allow-list: test code, not a hot path

mod common;

use core::mem::{align_of, offset_of, size_of};

use common::*;
use hft_refdata::blob::{RdbHeader, SectionEntry, StrRef};
use hft_refdata::{AccountMeta, InstHot, SymEntry, VenueCaps};
use hft_types::InstId;

#[test]
fn ref_001_rdb_header_layout() {
    assert_eq!(
        (size_of::<RdbHeader>(), align_of::<RdbHeader>()),
        (64, 8),
        "REF-001.1"
    );
    let o = [
        offset_of!(RdbHeader, magic),
        offset_of!(RdbHeader, format),
        offset_of!(RdbHeader, n_sections),
        offset_of!(RdbHeader, _pad),
    ];
    assert_eq!(o, [0, 8, 10, 12], "REF-001.2");
    let o = [
        offset_of!(RdbHeader, version),
        offset_of!(RdbHeader, base_version),
        offset_of!(RdbHeader, created_utc_ns),
        offset_of!(RdbHeader, total_len),
    ];
    assert_eq!(o, [16, 24, 32, 40], "REF-001.3");
    assert_eq!(offset_of!(RdbHeader, _rsv), 48, "REF-001.4");
    assert_eq!(size_of::<[u8; 16]>(), 16, "REF-001.4");
}

#[test]
fn ref_002_section_entry_layout() {
    assert_eq!(size_of::<SectionEntry>(), 24, "REF-002.1");
    let o = [
        offset_of!(SectionEntry, id),
        offset_of!(SectionEntry, row_ver),
        offset_of!(SectionEntry, row_size),
        offset_of!(SectionEntry, offset),
        offset_of!(SectionEntry, count),
        offset_of!(SectionEntry, flags),
    ];
    assert_eq!(o, [0, 2, 4, 8, 16, 20], "REF-002.2");
    assert_eq!(size_of::<StrRef>(), 8, "REF-002.3");
}

#[test]
fn ref_074_hot_layouts() {
    assert_eq!(
        (size_of::<InstHot>(), align_of::<InstHot>()),
        (64, 64),
        "REF-074.1"
    );
    let o = [
        offset_of!(InstHot, venue),
        offset_of!(InstHot, status),
        offset_of!(InstHot, kind),
        offset_of!(InstHot, flags),
    ];
    assert_eq!(o, [0, 1, 2, 3], "REF-074.2");
    let o = [
        offset_of!(InstHot, px_exp),
        offset_of!(InstHot, qty_exp),
        offset_of!(InstHot, px_wire_exp),
        offset_of!(InstHot, qty_wire_exp),
    ];
    assert_eq!(o, [4, 5, 6, 7], "REF-074.3");
    let o = [
        offset_of!(InstHot, asset),
        offset_of!(InstHot, quote),
        offset_of!(InstHot, book_window),
    ];
    assert_eq!(o, [8, 10, 12], "REF-074.4");
    let o = [
        offset_of!(InstHot, tick),
        offset_of!(InstHot, lot),
        offset_of!(InstHot, min_qty),
        offset_of!(InstHot, max_qty),
        offset_of!(InstHot, min_notional),
    ];
    assert_eq!(o, [16, 24, 32, 40, 48], "REF-074.5");
    let o = [
        offset_of!(InstHot, l2_depth),
        offset_of!(InstHot, crossed_tolerance),
        offset_of!(InstHot, _pad),
    ];
    assert_eq!(o, [56, 58, 60], "REF-074.6");
    let o = [
        size_of::<SymEntry>(),
        offset_of!(SymEntry, key),
        offset_of!(SymEntry, len),
        offset_of!(SymEntry, inst),
    ];
    assert_eq!(o, [32, 0, 27, 28], "REF-074.7");
    assert!(size_of::<VenueCaps>() <= 128, "REF-074.8");
    assert!(
        offset_of!(VenueCaps, flags) < 64 && offset_of!(VenueCaps, dms) < 64,
        "REF-074.8"
    );
    // The kind field (and hence its discriminant) lies entirely inside the first 64 bytes.
    assert!(
        offset_of!(AccountMeta, kind) + size_of::<hft_refdata::AccountKind>() <= 64,
        "REF-074.9"
    );
    assert!(
        offset_of!(AccountMeta, bound_instance) < 64 && offset_of!(AccountMeta, venues) < 64,
        "REF-074.9"
    );
    let rd = ok(&f0());
    let base = rd.inst(InstId(0)) as *const InstHot as usize;
    assert_eq!(base % 64, 0, "REF-074.10");
    assert_eq!(
        (rd.inst(InstId(1)) as *const InstHot as usize) % 64,
        0,
        "REF-074.10"
    );
}
