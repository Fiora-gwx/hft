#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods
)] // allow-list: test code
#![allow(clippy::type_complexity, clippy::too_many_arguments)]
mod common;

use common::*;
use hft_event::{ExternalCause, OrderAction};
use hft_oms::*;
use hft_strategy_sdk::{GroupTable, OrderRef, SnapshotReader, SnapshotWriter};
use hft_types::{AccountId, Side, Tif};

fn snap(o: &Oms) -> Vec<u8> {
    let mut w = SnapshotWriter::new(1 << 22);
    o.snapshot(&mut w);
    w.as_bytes().to_vec()
}

fn restore(b: &[u8]) -> Result<Oms, hft_strategy_sdk::RestoreError> {
    Oms::restore(&mut SnapshotReader::new(b), &p0(), rd())
}

/// (section id, version offset, body start, body end, count) of each section.
fn sections(b: &[u8]) -> Vec<(u16, usize, usize, usize, u32)> {
    let mut v = Vec::new();
    let mut i = 0;
    while i + 16 <= b.len() {
        let id = u16::from_le_bytes([b[i], b[i + 1]]);
        let count = u32::from_le_bytes([b[i + 4], b[i + 5], b[i + 6], b[i + 7]]);
        let len = u32::from_le_bytes([b[i + 8], b[i + 9], b[i + 10], b[i + 11]]) as usize;
        v.push((id, i + 2, i + 16, i + 16 + len, count));
        i += 16 + len;
    }
    v
}

fn sec(b: &[u8], id: u16) -> (usize, usize, usize, u32) {
    let s = sections(b).into_iter().find(|s| s.0 == id).unwrap();
    (s.1, s.2, s.3, s.4)
}

/// Rows of section 12: (kind, body offset in `b`, len).
fn rows12(b: &[u8]) -> Vec<(u8, usize, usize)> {
    let (_, mut i, end, _) = sec(b, SECTION_OMS_LINKS);
    let mut v = Vec::new();
    while i + 8 <= end && b[i] != 0 {
        let len = u32::from_le_bytes([b[i + 4], b[i + 5], b[i + 6], b[i + 7]]) as usize;
        v.push((b[i], i + 8, len));
        i += 8 + len;
    }
    v
}

#[test]
fn oms_360_snapshot_roundtrip() {
    let (h, slots, links) = mixed();
    let w1 = snap(&h.o);
    let r = restore(&w1).unwrap();
    let w2 = snap(&r);
    assert_eq!(w1, w2);
    for s in &slots {
        assert_eq!(format!("{:?}", h.o.slot(*s)), format!("{:?}", r.slot(*s)));
    }
    for a in [A1, A2, S1] {
        for inst in [X, X2, Y, Z] {
            assert_eq!(h.o.inflight().pos(a, inst), r.inflight().pos(a, inst));
        }
        assert_eq!(h.o.quiescent(a), r.quiescent(a));
    }
    for (a, g) in [(A1, g1()), (A1, g2()), (S1, g3())] {
        assert_eq!(h.o.legs(a, g).view, r.legs(a, g).view);
    }
    for (v, a) in [(VS, A1), (VX, A1), (VG, A1), (VS, A2), (VS, S1)] {
        assert_eq!(h.o.venue_ready(v, a), r.venue_ready(v, a));
    }
    for l in links {
        assert_eq!(h.o.link_peer(l), r.link_peer(l));
    }
}

#[test]
fn oms_361_snapshot_rows() {
    let (h, _, _) = mixed();
    let b = snap(&h.o);
    let (_, s, e, count) = sec(&b, SECTION_ORDER_SLOTS);
    assert_eq!(count, 40, "OMS-361.1");
    assert_eq!(e - s, 40 * 160, "OMS-361.1");
    let keys: Vec<(u8, u8, u32)> = (0..40)
        .map(|i| {
            let r = &b[s + i * 160..];
            (r[6], r[7], u32::from_le_bytes([r[12], r[13], r[14], r[15]]))
        })
        .map(|(v, a, sl)| (vix(v), a, sl))
        .collect();
    let mut sorted = keys.clone();
    sorted.sort_unstable();
    assert_eq!(keys, sorted, "OMS-361.1");
    let (_, s3, _, c3) = sec(&b, SECTION_OMS_COUNTERS);
    let epoch = u32::from_le_bytes([b[s3], b[s3 + 1], b[s3 + 2], b[s3 + 3]]);
    let n_tables = u32::from_le_bytes([b[s3 + 4], b[s3 + 5], b[s3 + 6], b[s3 + 7]]);
    let rhu = u64::from_le_bytes(b[s3 + 8..s3 + 16].try_into().unwrap());
    assert_eq!((epoch, n_tables, rhu, c3), (7, 5, ms(40_000).0, 6), "OMS-361.2");
    let rows = rows12(&b);
    let kinds: Vec<u8> = rows.iter().map(|r| r.0).collect();
    let mut k2 = kinds.clone();
    k2.sort_unstable();
    assert_eq!(kinds, k2, "OMS-361.3");
    assert_eq!(
        kinds.iter().filter(|k| **k == 1).count(),
        5,
        "OMS-361.3 (Done not written)"
    );
    assert_eq!(kinds.iter().filter(|k| **k == 2).count(), 5, "OMS-361.3");
    assert_eq!(
        kinds.iter().filter(|k| **k == 3).count(),
        3,
        "OMS-361.3 (zero LegState not written)"
    );
    assert_eq!(
        kinds.iter().filter(|k| **k == 4).count(),
        6,
        "OMS-361.3 (expired tomb not written)"
    );
    assert!(kinds.iter().filter(|k| **k == 5).count() >= 20, "OMS-361.3");
    for (k, _, len) in rows {
        let want = [0, 80, 64, 200, 64, 32][k as usize];
        assert_eq!(len, want, "OMS-361.4");
    }
}

fn vix(v: u8) -> u8 {
    match v {
        2 => 0,
        5 => 1,
        _ => 2,
    }
}

#[test]
fn oms_362_row_versions() {
    let (h, _, _) = mixed();
    let b = snap(&h.o);
    for id in [SECTION_ORDER_SLOTS, SECTION_OMS_COUNTERS, SECTION_OMS_LINKS] {
        let (vo, _, _, _) = sec(&b, id);
        assert_eq!(u16::from_le_bytes([b[vo], b[vo + 1]]), 1, "OMS-362.1");
    }
    assert!(restore(&b).is_ok(), "OMS-362.1");
    for (row, id) in [
        ("OMS-362.2", SECTION_ORDER_SLOTS),
        ("OMS-362.3", SECTION_OMS_COUNTERS),
        ("OMS-362.4", SECTION_OMS_LINKS),
    ] {
        let mut c = b.clone();
        let (vo, _, _, _) = sec(&c, id);
        c[vo] = 2;
        assert!(restore(&c).is_err(), "{row}");
    }
}

#[test]
fn oms_363_bad_discriminants() {
    let (h, _, _) = mixed();
    let b = snap(&h.o);
    let (_, s2, _, _) = sec(&b, SECTION_ORDER_SLOTS);
    let rows = rows12(&b);
    let off = |k: u8| rows.iter().find(|r| r.0 == k).unwrap().1;
    let cases: [(&str, usize, u8); 7] = [
        ("OMS-363.1", s2, 9),
        ("OMS-363.2", s2 + 1, 0),
        ("OMS-363.3", s2 + 128 + 13, 6),
        ("OMS-363.4", off(1) + 9, 9),
        ("OMS-363.5", off(4) + 8 + 16, 3),
        ("OMS-363.6", off(2) + 16 + 1, 4),
        ("OMS-363.7", off(1) - 8, 6),
    ];
    for (row, at, val) in cases {
        let mut c = b.clone();
        c[at] = val;
        assert!(restore(&c).is_err(), "{row}");
    }
}

#[test]
fn oms_364_inflight_recomputed() {
    let (h, _, _) = mixed();
    let b = snap(&h.o);
    let r = restore(&b).unwrap();
    for inst in [X, Y, Z] {
        assert_eq!(
            h.o.inflight().pos(A1, inst),
            r.inflight().pos(A1, inst),
            "OMS-364.1"
        );
    }
    for v in [VS, VX, VG] {
        assert_eq!(
            h.o.inflight().open_notional_venue(A1, v),
            r.inflight().open_notional_venue(A1, v),
            "OMS-364.1"
        );
    }
    let rows = rows12(&b);
    let leg = rows.iter().find(|r| r.0 == 3).unwrap().1;
    let mut c = b.clone();
    let at = leg + 8 + 8;
    let v = i64::from_le_bytes(c[at..at + 8].try_into().unwrap()) + 1;
    c[at..at + 8].copy_from_slice(&v.to_le_bytes());
    assert!(restore(&c).is_err(), "OMS-364.2");
}

#[test]
fn oms_365_group_map_rebuilt() {
    let mut h = H::new();
    h.o.test_set_net(A1, g2(), Z, q("0.003"));
    let before = h.legs(A1, g2()).view;
    let b = snap(&h.o);
    let mut o = restore(&b).unwrap();
    assert_eq!(o.legs(A1, g2()).view, before);
    o.on_groups_changed(&groups_s1(true));
    let mut r = H {
        o,
        out: OmsOut::new(),
        seq: 500,
    };
    r.conn(
        VG,
        A1,
        hft_event::ConnRole::Order,
        hft_event::ConnStateKind::Up,
        0,
    );
    let z = r.place_ok(
        &intent(VG, A1, Z, Side::Buy, Tif::Gtc, p("60000.0"), q("0.001"), g2()),
        0,
    );
    assert_eq!(r.ext(z).leg_ix, 1);
    let _ = GroupTable::new(&[], 1);
    let _ = AccountId(0);
}

#[test]
fn oms_366_snapshot_golden() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/oms/OMS-366");
    let script = std::fs::read_to_string(dir.join("script.txt")).unwrap();
    let mut h = H::new();
    let mut refs: Vec<OrderRef> = Vec::new();
    for line in script
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
    {
        let w: Vec<&str> = line.split_whitespace().collect();
        let t: u64 = w[w.len() - 1].parse().unwrap();
        match w[0] {
            "place_buy_x" => refs.push(h.place_ok(&buy_x(), t)),
            "place_sell_y" => refs.push(h.place_ok(&sell_y(), t)),
            "place_first" => refs.push(place_first(&mut h, q(w[1]), Tif::Gtc, plan_l1(), t).0),
            "ack" => {
                let r = refs[w[1].parse::<usize>().unwrap()];
                h.ack(r, w[2].parse().unwrap(), t);
            }
            "fill" => {
                let r = refs[w[1].parse::<usize>().unwrap()];
                h.fill(r, q(w[2]), p(w[3]), t);
            }
            "external" => {
                h.out.clear();
                h.o.external_adjust(
                    A1,
                    VX,
                    g1(),
                    Y,
                    q(w[1]),
                    w[2].parse().unwrap(),
                    ExternalCause::Unknown,
                    &mut h.out,
                );
            }
            "cancel" => {
                let r = refs[w[1].parse::<usize>().unwrap()];
                h.cancel(r, t).unwrap();
            }
            "enqueue_query" => {
                let r = refs[w[1].parse::<usize>().unwrap()];
                h.o.test_enqueue_decision(r, OrderAction::Query, t);
            }
            other => panic!("unknown script command {other}"),
        }
    }
    let b = snap(&h.o);
    let txt = dump(&b);
    if std::env::var_os("OMS_GOLDEN_UPDATE").is_some() {
        std::fs::write(dir.join("sections.bin"), &b).unwrap();
        std::fs::write(dir.join("sections.txt"), &txt).unwrap();
    }
    let want = std::fs::read(dir.join("sections.bin")).unwrap();
    assert_eq!(b, want, "sections.bin");
    let want_txt = std::fs::read_to_string(dir.join("sections.txt"))
        .unwrap()
        .replace("\r\n", "\n");
    assert_eq!(txt, want_txt, "sections.txt");
}

/// Readable dump: per section, then per row, 8-byte little-endian words in decimal.
fn dump(b: &[u8]) -> String {
    use std::fmt::Write;
    let mut s = String::new();
    for (id, _, start, end, count) in sections(b) {
        let ver = u16::from_le_bytes([b[start - 14], b[start - 13]]);
        writeln!(s, "section {id} version {ver} count {count} len {}", end - start).unwrap();
        for (i, ch) in b[start..end].chunks(8).enumerate() {
            let mut w = [0u8; 8];
            w[..ch.len()].copy_from_slice(ch);
            writeln!(s, "  +{:04} {}", i * 8, u64::from_le_bytes(w)).unwrap();
        }
    }
    s
}
