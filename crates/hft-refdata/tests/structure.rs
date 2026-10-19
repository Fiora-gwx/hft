//! Load step 1: header, directory, row sizes, unknown/retired sections, discriminants
//! (REF-005 – REF-009).

#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods
)] // allow-list: test code, not a hot path

use crate::common;

use common::*;
use hft_refdata::blob::{
    AccountRow, AssetRow, CapsRow, EndpointRow, InstRow, MaintRow, RateLimitRow, RawBlob, RawSection,
    SECTION_OPTIONAL, VenueRow, sec,
};
use hft_refdata::{LoadErrCode::Structure, MaintKind};
use hft_types::InstId;

fn set(b: &mut [u8], at: usize, v: &[u8]) {
    b[at..at + v.len()].copy_from_slice(v);
}

fn patched(f: &[u8], at: usize, v: &[u8]) -> Vec<u8> {
    let mut b = f.to_vec();
    set(&mut b, at, v);
    b
}

#[test]
fn ref_005_header_structure() {
    let f = f0();
    let n = f.len() as u64;
    let s0 = vec![e(Structure, 0, 0)];
    let cases: Vec<(&str, Vec<u8>)> = vec![
        ("REF-005.1", patched(&f, 0, b"HFTRDB02")),
        ("REF-005.2", e0()),
        ("REF-005.3", patched(&f, 8, &0u16.to_le_bytes())),
        ("REF-005.4", patched(&f, 8, &2u16.to_le_bytes())),
        ("REF-005.5", patched(&f, 40, &(n + 1).to_le_bytes())),
        ("REF-005.6", patched(&f, 40, &(n - 1).to_le_bytes())),
        ("REF-005.7", f[..63].to_vec()),
        ("REF-005.8", patched(&f, 24, &1u64.to_le_bytes())),
        ("REF-005.9", patched(&f, 10, &200u16.to_le_bytes())),
        ("REF-005.11", patched(&f, 16, &0u64.to_le_bytes())),
    ];
    for (case, b) in cases {
        assert_eq!(errs(&b), s0, "{case}");
    }
    ok(&f); // REF-005.10
}

/// F0 with the Insts section laid out last.
fn insts_last() -> Vec<u8> {
    let mut r = f0_spec().builder().finish_raw();
    let ix = r.sections.iter().position(|s| s.id == sec::INSTS).expect("insts");
    let s = r.sections.remove(ix);
    r.sections.push(s);
    r.encode()
}

#[test]
fn ref_006_section_bounds() {
    let base = insts_last();
    let ii = dir_index(&base, sec::INSTS);
    let ins = dir_entry(&base, ii);
    assert_eq!(
        ins.offset + u64::from(ins.row_size) * u64::from(ins.count),
        base.len() as u64,
        "REF-006.1"
    );
    ok(&base);

    let mut b = base.clone();
    let mut x = ins;
    x.count += 1;
    set_dir_entry(&mut b, ii, &x);
    assert_eq!(errs(&b), vec![e(Structure, 3, 0)], "REF-006.2");

    let ai = dir_index(&base, sec::ASSETS);
    let vi = dir_index(&base, sec::VENUES);
    let assets = dir_entry(&base, ai);
    let assets_end = assets.offset + u64::from(assets.row_size) * u64::from(assets.count);
    let mut b = base.clone();
    let mut v = dir_entry(&base, vi);
    v.offset = assets_end - 8;
    set_dir_entry(&mut b, vi, &v);
    let es = errs(&b);
    assert!(es.len() == 1 && es[0].code == Structure, "REF-006.3: {es:?}");

    let mut b = base.clone();
    let mut v = dir_entry(&base, vi);
    v.offset = assets_end;
    set_dir_entry(&mut b, vi, &v);
    ok(&b); // REF-006.4

    let mut b = base.clone();
    let mut v = dir_entry(&base, vi);
    v.offset += 4;
    assert_eq!(v.offset % 8, 4);
    set_dir_entry(&mut b, vi, &v);
    assert_eq!(errs(&b), vec![e(Structure, 2, 0)], "REF-006.5");

    let mut r = raw(&base);
    let a = r.section(sec::ASSETS).expect("assets").clone();
    r.sections.insert(1, a);
    let es = errs(&r.encode());
    assert!(es.len() == 1 && es[0].code == Structure, "REF-006.6: {es:?}");
}

#[test]
fn ref_007_row_size_prefix() {
    let base = f0();
    let k = size_of::<InstRow>() as u32;
    let ii = dir_index(&base, sec::INSTS);
    let mut b = base.clone();
    let mut x = dir_entry(&base, ii);
    x.row_size = k - 8;
    set_dir_entry(&mut b, ii, &x);
    assert_eq!(errs(&b), vec![e(Structure, 3, 0)], "REF-007.1");
    assert_eq!(dir_entry(&base, ii).row_size, k);
    let rd0 = ok(&base); // REF-007.2

    let mut r = raw(&base);
    let s = r.section_mut(sec::INSTS).expect("insts");
    let data: Vec<u8> = s
        .data
        .chunks(k as usize)
        .flat_map(|row| row.iter().copied().chain([0xAB; 8]))
        .collect();
    s.data = data;
    s.row_size = k + 8;
    let rd = ok(&r.encode());
    for id in 1..=5 {
        assert_eq!(rd.inst(InstId(id)), rd0.inst(InstId(id)), "REF-007.3 inst {id}");
        assert_eq!(
            rd.inst_meta(InstId(id)),
            rd0.inst_meta(InstId(id)),
            "REF-007.3 inst_meta {id}"
        );
    }
}

#[test]
fn ref_008_unknown_and_retired_sections() {
    let base = f0();
    let rd0 = ok(&base);
    let with = |id: u16, flags: u32| {
        let mut r = raw(&base);
        r.sections.push(RawSection {
            id,
            row_ver: 1,
            row_size: 16,
            count: 1,
            flags,
            data: vec![0; 16],
        });
        r.encode()
    };
    let rd = ok(&with(20, SECTION_OPTIONAL));
    assert_eq!(rd.debug_tables(), rd0.debug_tables(), "REF-008.1");
    let cases: [(&str, u16, u32); 7] = [
        ("REF-008.2", 20, 0),
        ("REF-008.3", 31, 0),
        ("REF-008.4", 32, SECTION_OPTIONAL),
        ("REF-008.5", 32, 0),
        ("REF-008.6", 33, 0),
        ("REF-008.7", 37, SECTION_OPTIONAL),
        ("REF-008.9", 38, 0),
    ];
    for (case, id, flags) in cases {
        assert_eq!(errs(&with(id, flags)), vec![e(Structure, id, 0)], "{case}");
    }
    ok(&with(38, SECTION_OPTIONAL)); // REF-008.8
}

type Edit = fn(&mut RawBlob);

#[test]
fn ref_009_bad_discriminants() {
    let mut spec = f0_spec();
    spec.maint
        .push(window(V6, 0, 1_000, 2_000, MaintKind::VenueMaintenance));
    let base = spec.build();
    ok(&base);
    let cases: [(&str, u16, Edit); 26] = [
        ("REF-009.1", 3, |r| {
            edit_row::<InstRow, _>(r, 3, 0, |x| x.status = 7)
        }),
        ("REF-009.2", 3, |r| {
            edit_row::<InstRow, _>(r, 3, 0, |x| x.kind = 0)
        }),
        ("REF-009.3", 3, |r| {
            edit_row::<InstRow, _>(r, 3, 0, |x| x.kind = 4)
        }),
        ("REF-009.4", 1, |r| {
            edit_row::<AssetRow, _>(r, 1, 0, |x| x.class = 5)
        }),
        ("REF-009.5", 2, |r| {
            edit_row::<VenueRow, _>(r, 2, 0, |x| x.exchange = 9)
        }),
        ("REF-009.6", 2, |r| {
            edit_row::<VenueRow, _>(r, 2, 0, |x| x.stale_guard = 0)
        }),
        ("REF-009.7", 2, |r| {
            edit_row::<VenueRow, _>(r, 2, 0, |x| x.ts_unit = 4)
        }),
        ("REF-009.8", 4, |r| {
            edit_row::<AccountRow, _>(r, 4, 0, |x| x.kind = 3)
        }),
        ("REF-009.9", 4, |r| {
            edit_row::<AccountRow, _>(r, 4, 0, |x| x.uid_policy = 0)
        }),
        ("REF-009.10", 4, |r| {
            edit_row::<AccountRow, _>(r, 4, 0, |x| x.uid_policy = 3)
        }),
        ("REF-009.11", 13, |r| {
            edit_row::<CapsRow, _>(r, 13, 0, |x| x.funding_settle = 5)
        }),
        ("REF-009.12", 13, |r| {
            edit_row::<CapsRow, _>(r, 13, 0, |x| x.margin_data = 4)
        }),
        ("REF-009.13", 13, |r| {
            edit_row::<CapsRow, _>(r, 13, 0, |x| x.position_mode = 4)
        }),
        ("REF-009.14", 13, |r| {
            edit_row::<CapsRow, _>(r, 13, 0, |x| x.margin_mode = 5)
        }),
        ("REF-009.15", 13, |r| {
            edit_row::<CapsRow, _>(r, 13, 0, |x| x.multi_asset = 0)
        }),
        ("REF-009.16", 13, |r| {
            edit_row::<CapsRow, _>(r, 13, 0, |x| x.multi_asset = 4)
        }),
        ("REF-009.17", 13, |r| {
            edit_row::<CapsRow, _>(r, 13, 0, |x| x.spot_borrow = 4)
        }),
        ("REF-009.18", 13, |r| {
            edit_row::<CapsRow, _>(r, 13, 0, |x| x.liq_marker = 5)
        }),
        ("REF-009.19", 13, |r| {
            edit_row::<CapsRow, _>(r, 13, 0, |x| x.dms = 6)
        }),
        ("REF-009.20", 13, |r| {
            edit_row::<CapsRow, _>(r, 13, 0, |x| x.testnet = 4)
        }),
        ("REF-009.21", 13, |r| {
            edit_row::<CapsRow, _>(r, 13, 0, |x| x.fee_rule = 5)
        }),
        ("REF-009.22", 7, |r| {
            edit_row::<RateLimitRow, _>(r, 7, 0, |x| x.scope = 6)
        }),
        ("REF-009.23", 7, |r| {
            edit_row::<RateLimitRow, _>(r, 7, 0, |x| x.bucket = 6)
        }),
        ("REF-009.24", 8, |r| {
            edit_row::<EndpointRow, _>(r, 8, 0, |x| x.role = 7)
        }),
        ("REF-009.25", 8, |r| {
            edit_row::<EndpointRow, _>(r, 8, 0, |x| x.env = 4)
        }),
        ("REF-009.26", 9, |r| {
            edit_row::<MaintRow, _>(r, 9, 0, |x| x.kind = 4)
        }),
    ];
    for (case, section, edit) in cases {
        let mut r = raw(&base);
        edit(&mut r);
        assert_eq!(errs(&r.encode()), vec![e(Structure, section, 0)], "{case}");
    }
}
