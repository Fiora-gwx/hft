//! Event-weighted offline statistics (TEL-025–TEL-035, TEL-037).

mod common;

use std::collections::{BTreeMap, BTreeSet};

use common::*;
use hft_event::{
    EventKind, Funding, IndexPrice, L2SnapHdr, Level, MarkPrice, MarketHdr, OrderEventKind, RecordFlags,
    Trade,
};
use hft_journal::testing;
use hft_telemetry::{
    EventWeightedReport, EwSample, Stage, Stratum, TsSrc, event_weighted, event_weighted_with,
    stratified_probe, stratum,
};
use hft_types::{Px, Qty, Rate, Side};
use sha2::{Digest, Sha256};
use zerocopy::{FromZeros, IntoBytes};

const HW: u16 = RecordFlags::HW_TS.0;

fn run(j: &Jw) -> (Vec<EventWeightedReport>, Vec<EwSample>) {
    let mut v = Vec::new();
    let r = event_weighted_with(&mut j.reader(), &rd(), &ew_cfg(), |s| v.push(*s)).unwrap();
    (r, v)
}

fn values(v: &[EwSample], stage: Stage, hi: bool) -> Vec<i64> {
    v.iter()
        .filter(|s| s.key.stage == stage && s.hi == hi)
        .map(|s| s.value)
        .collect()
}

#[test]
fn tel_025_one_read_three_samples() {
    let mut j = Jw::new([1; 32], 1);
    j.clock(&std_a());
    for d in [415_000, 416_000, 417_000] {
        j.input(
            T0 + d,
            HW,
            &bbo(mhdr(P0 + 400_000, P0 + 405_000, T0 + 412_000, 20_000)),
        );
    }
    j.clock(&std_b());
    let (r, v) = run(&j);
    assert_eq!(r.len(), 1);
    assert_eq!(r[0].n, 3);
    let kr: Vec<_> = r[0]
        .hist
        .iter()
        .filter(|(k, _)| k.stage == Stage::KernelRx)
        .collect();
    assert_eq!(kr.len(), 1);
    assert_eq!((kr[0].0.stratum, kr[0].1.count), (Stratum::Burst, 3));
    assert_eq!(values(&v, Stage::KernelRx, false), vec![12_000; 3]);
    assert_eq!(values(&v, Stage::Ingest, false), vec![3_000, 4_000, 5_000]);
    assert!(r[0].hist.iter().all(|(k, _)| k.stratum != Stratum::Calm));
    assert!(v.iter().all(|s| s.key.stratum == Stratum::Burst));
}

#[test]
fn tel_026_stratum_boundaries() {
    for (row, frames, gap, want) in [
        ("TEL-026.1", 1u16, 50_000u32, Stratum::Calm),
        ("TEL-026.2", 1, 49_999, Stratum::Calm),
        ("TEL-026.3", 1, 50_001, Stratum::Burst),
        ("TEL-026.4", 2, 0, Stratum::Burst),
        ("TEL-026.5", 1, 0, Stratum::Calm),
        ("TEL-026.6", 3, 10, Stratum::Burst),
    ] {
        assert_eq!(stratum(frames, gap, 50_000), want, "{row}");
    }
}

/// One `Bbo` (hw P0+400 000, sw P0+405 000, rx T0+412 000) with `gap` and record `flags`.
fn single(gap: u32, flags: u16) -> (Vec<EventWeightedReport>, Vec<EwSample>) {
    let mut j = Jw::new([1; 32], 1);
    j.clock(&std_a());
    j.input(
        T0 + 415_000,
        flags,
        &bbo(mhdr(P0 + 400_000, P0 + 405_000, T0 + 412_000, gap)),
    );
    j.clock(&std_b());
    run(&j)
}

#[test]
fn tel_027_kernel_rx_hi() {
    // TEL-027.1
    let (r, v) = single(30_000, HW);
    assert_eq!(values(&v, Stage::KernelRx, true), vec![35_000], "TEL-027.1");
    assert!(35_000 >= values(&v, Stage::KernelRx, false)[0], "TEL-027.1");
    assert_eq!(values(&v, Stage::KernelRx, false), vec![12_000], "TEL-027.1");
    assert!(
        r[0].kernel_rx_hi.iter().all(|(k, _)| k.stage == Stage::KernelRx),
        "TEL-027.4"
    );
    // TEL-027.2
    let (r, v) = single(7_000, HW);
    assert_eq!(values(&v, Stage::KernelRx, true), vec![12_000], "TEL-027.2");
    assert!(
        r[0].kernel_rx_hi.iter().all(|(k, _)| k.stage == Stage::KernelRx),
        "TEL-027.4"
    );
    // TEL-027.3
    let (r, v) = single(30_000, RecordFlags::SW_TS.0);
    assert_eq!(values(&v, Stage::KernelRx, true), vec![30_000], "TEL-027.3");
    assert_eq!(r[0].kernel_rx_hi.len(), 1, "TEL-027.3");
    assert_eq!(r[0].kernel_rx_hi[0].0.ts_src, TsSrc::Sw, "TEL-027.3");
    assert!(
        r[0].kernel_rx_hi_sw_gap_only,
        "TEL-027.3: the report notes the missing NIC → softirq part"
    );
    assert!(
        r[0].kernel_rx_hi.iter().all(|(k, _)| k.stage == Stage::KernelRx),
        "TEL-027.4"
    );
}

#[test]
fn tel_028_gap_missing() {
    let (r, v) = single(0, HW);
    assert_eq!(r[0].gap_missing, 1);
    assert!(r[0].kernel_rx_hi.is_empty());
    assert!(values(&v, Stage::KernelRx, true).is_empty());
    assert_eq!(values(&v, Stage::KernelRx, false), vec![12_000]);
}

fn mh(i: i64) -> MarketHdr {
    // Record i: hw P0 + 400 000 + i·1 000, sw + 4 000 + i·100, rx_tsc T0 + 410 000 + i·2 000.
    mhdr(
        P0 + 400_000 + i * 1_000,
        P0 + 404_000 + i * 1_100,
        T0 + 410_000 + i as u64 * 2_000,
        20_000,
    )
}

/// TEL-029 journal: market records at seq 10..=14 (2 Bbo, Trade, Mark, Funding), proc_tsc =
/// rx_tsc + 3 000 + i·500.
fn manual_journal() -> Jw {
    let mut j = Jw::new([1; 32], 1);
    j.clock(&std_a());
    j.pad_to(10);
    let proc = |i: i64| mh(i).rx_tsc + 3_000 + i as u64 * 500;
    assert_eq!(j.input(proc(0), HW, &bbo(mh(0))), 10);
    j.input(proc(1), HW, &bbo(mh(1)));
    let t = Trade {
        h: mh(2),
        px: Px(100),
        qty: Qty(1),
        trade_id: 9,
        taker_side: Side::Buy,
        _pad: [0; 7],
    };
    j.input(proc(2), HW, &t);
    j.input(
        proc(3),
        HW,
        &MarkPrice {
            h: mh(3),
            px: Px(100),
        },
    );
    let f = Funding {
        h: mh(4),
        rate: Rate(1),
        next_funding_ns: 0,
        interval_s: 28_800,
        _pad: 0,
    };
    assert_eq!(j.input(proc(4), HW, &f), 14);
    j.clock(&std_b());
    j
}

#[test]
fn tel_029_matches_manual() {
    let j = manual_journal();
    let (r, v) = run(&j);
    assert_eq!(r[0].n, 5);
    let kinds = [
        EventKind::BBO,
        EventKind::BBO,
        EventKind::TRADE,
        EventKind::MARK_PRICE,
        EventKind::FUNDING,
    ];
    let mut want: BTreeMap<(u16, &str), Vec<i64>> = BTreeMap::new();
    for (i, k) in kinds.iter().enumerate() {
        let h = mh(i as i64);
        let proc = h.rx_tsc + 3_000 + i as u64 * 500;
        // latency-budget §2 under the standard calibration (1 tick = 1 ns, PHC = P0 + tsc − T0).
        let kernel_rx = P0 + (h.rx_tsc - T0) as i64 - h.hw_rx_ns;
        let ingest = (proc - h.rx_tsc) as i64;
        want.entry((k.0, "kernel_rx")).or_default().push(kernel_rx);
        want.entry((k.0, "ingest")).or_default().push(ingest);
        want.entry((k.0, "feed_host"))
            .or_default()
            .push(kernel_rx + ingest);
    }
    let mut got: BTreeMap<(u16, &str), Vec<i64>> = BTreeMap::new();
    for s in v.iter().filter(|s| !s.hi) {
        if matches!(s.key.stage, Stage::KernelRx | Stage::Ingest | Stage::FeedHost) {
            got.entry((s.key.kind.0, s.key.stage.name()))
                .or_default()
                .push(s.value);
        }
    }
    assert_eq!(got, want);
}

#[test]
fn tel_030_hostprofile_split() {
    let (h1, h2) = ([0x11; 32], [0x22; 32]);
    let mut j = Jw::new(h1, 1);
    j.clock(&std_a());
    for i in 0..10u64 {
        j.input(
            T0 + 415_000 + i * 1_000,
            HW,
            &bbo(mhdr(P0 + 400_000, 0, T0 + 412_000 + i * 1_000, 10)),
        );
    }
    j.clock(&std_b());
    j.roll(h2, "v0.2.0");
    let (a2, b2) = (row(3 * T0, P0 + 2_000_000_000), row(4 * T0, P0 + 3_000_000_000));
    j.clock(&a2);
    for i in 0..7u64 {
        j.input(
            3 * T0 + 415_000 + i * 1_000,
            HW,
            &bbo(mhdr(P0 + 2_000_400_000, 0, 3 * T0 + 412_000 + i * 1_000, 10)),
        );
    }
    j.clock(&b2);
    let (r, _) = run(&j);
    assert_eq!(r.len(), 2);
    assert_eq!((r[0].host_profile, r[0].n), (h1, 10));
    assert_eq!((r[1].host_profile, r[1].n), (h2, 7));
    assert_eq!(r[1].release, "v0.2.0");
    let c0: u64 = r[0]
        .hist
        .iter()
        .filter(|(k, _)| k.stage == Stage::Ingest)
        .map(|(_, q)| q.count)
        .sum();
    let c1: u64 = r[1]
        .hist
        .iter()
        .filter(|(k, _)| k.stage == Stage::Ingest)
        .map(|(_, q)| q.count)
        .sum();
    assert_eq!((c0, c1), (10, 7), "no merged histogram");
}

#[test]
fn tel_031_deterministic() {
    let j = manual_journal();
    let a = serde_json::to_vec(&event_weighted(&mut j.reader(), &rd(), &ew_cfg()).unwrap()).unwrap();
    let b = serde_json::to_vec(&event_weighted(&mut j.reader(), &rd(), &ew_cfg()).unwrap()).unwrap();
    assert_eq!(a, b);
}

fn digests(j: &Jw) -> Vec<[u8; 32]> {
    j.files()
        .iter()
        .map(|p| Sha256::digest(testing::read(p).unwrap()).into())
        .collect()
}

#[test]
fn tel_032_read_only() {
    let j = manual_journal();
    let before_files = j.files();
    let before = digests(&j);
    let cold_before: Vec<_> = walk(&j.cfg.cold_root);
    testing::oplog_start();
    let r = event_weighted(&mut j.reader(), &rd(), &ew_cfg());
    let ops = testing::oplog_take();
    assert!(r.is_ok());
    // Only mappings: no preparation, copy, sync, retirement, unlink or append.
    assert!(
        ops.iter()
            .all(|o| matches!(o, testing::Op::Map | testing::Op::Unmap)),
        "{ops:?}"
    );
    assert_eq!(j.files(), before_files);
    assert_eq!(digests(&j), before, "journal SHA-256 unchanged");
    assert_eq!(walk(&j.cfg.cold_root), cold_before, "no file written");
}

fn walk(p: &std::path::Path) -> Vec<std::path::PathBuf> {
    let mut out = Vec::new();
    if let Ok(rd) = std::fs::read_dir(p) {
        for e in rd.flatten() {
            let path = e.path();
            if path.is_dir() {
                out.extend(walk(&path));
            }
            out.push(path);
        }
    }
    out.sort();
    out
}

#[test]
fn tel_033_record_kinds() {
    let mut j = Jw::new([1; 32], 1);
    j.clock(&std_a());
    let h = mhdr(P0 + 400_000, 0, T0 + 412_000, 10);
    let p = T0 + 415_000;
    // TEL-033.1
    j.input(p, HW, &bbo(h));
    j.input(
        p,
        HW,
        &Trade {
            h,
            px: Px(1),
            qty: Qty(1),
            trade_id: 1,
            taker_side: Side::Sell,
            _pad: [0; 7],
        },
    );
    j.input(p, HW, &MarkPrice { h, px: Px(1) });
    j.input(p, HW, &IndexPrice { h, px: Px(1) });
    j.input(
        p,
        HW,
        &Funding {
            h,
            rate: Rate(1),
            next_funding_ns: 0,
            interval_s: 0,
            _pad: 0,
        },
    );
    let mut sh = L2SnapHdr::new_zeroed();
    sh.h = h;
    sh.origin = L2SnapHdr::TOP_N;
    sh.n_bids = 1;
    sh.n_asks = 1;
    let lv = [
        Level {
            px: Px(1),
            qty: Qty(1),
        },
        Level {
            px: Px(2),
            qty: Qty(1),
        },
    ];
    j.var(EventKind::L2_SNAPSHOT, p, HW, &[sh.as_bytes(), lv.as_bytes()]);
    // TEL-033.2
    j.input(p, 0, &order_event(OrderEventKind::Ack));
    j.input(p, 0, &timer());
    let mut s = std_sample();
    s.trigger_seq = 5;
    j.meta(p + 20_000, &s);
    j.clock(&std_b());
    let (r, _) = run(&j);
    assert_eq!(r[0].n, 6, "TEL-033.1 / TEL-033.2");
    let kinds: BTreeSet<u16> = r[0].hist.iter().map(|(k, _)| k.kind.0).collect();
    let want: BTreeSet<u16> = [
        EventKind::BBO,
        EventKind::TRADE,
        EventKind::MARK_PRICE,
        EventKind::INDEX_PRICE,
        EventKind::FUNDING,
        EventKind::L2_SNAPSHOT,
    ]
    .iter()
    .map(|k| k.0)
    .collect();
    assert_eq!(kinds, want, "TEL-033.1");
}

#[test]
fn tel_034_unsynced_by_stratum() {
    let c0 = row(T0, P0);
    let c1 = row(2 * T0, P0 + 1_000_000_000);
    let mut c2 = row(3 * T0, P0 + 2_000_000_000);
    c2.calib_err_ns = 2_000;
    let c3 = row(4 * T0, P0 + 3_000_000_000);
    let mut j = Jw::new([1; 32], 1);
    let rec = |j: &mut Jw, base: u64, i: u64, burst: bool| {
        let rx = base + 100_000 + i * 100_000;
        let hw = P0 + (rx - T0) as i64 - 12_000;
        j.input(
            rx + 3_000,
            HW,
            &bbo(mhdr(hw, 0, rx, if burst { 60_000 } else { 10 })),
        );
    };
    j.clock(&c0);
    // Within [c0, c1] (bound 100 ns): Burst 2, Calm 5.
    for i in 0..7 {
        rec(&mut j, T0, i, i < 2);
    }
    j.clock(&c1);
    // Within [c1, c2] (bound 2 000 ns > 1 000): Burst 2, Calm 1.
    for i in 0..3 {
        rec(&mut j, 2 * T0, i, i < 2);
    }
    j.clock(&c2);
    j.clock(&c3);
    let (r, _) = run(&j);
    assert_eq!(r[0].n, 10);
    assert_eq!(r[0].unsynced_cal, 3);
    assert_eq!(
        r[0].unsynced_cal_by_stratum,
        [(Stratum::Burst, 2, 4), (Stratum::Calm, 1, 6)]
    );
    let n_err: u64 = r[0].cal_err.iter().map(|(_, q)| q.count).sum();
    assert_eq!(n_err, 10);
}

#[test]
fn tel_035_seq_range() {
    let j = manual_journal();
    let cfg = hft_telemetry::EventWeightedCfg {
        from_seq: 11,
        to_seq: 13,
        ..ew_cfg()
    };
    let r = event_weighted(&mut j.reader(), &rd(), &cfg).unwrap();
    assert_eq!(r[0].n, 3);
}

#[test]
fn tel_037_insufficient_stratum() {
    for (row, burst, some) in [("TEL-037.1", 1_000usize, true), ("TEL-037.2", 999, false)] {
        let j = probe_journal(1_000, burst, 0);
        let ew = event_weighted(&mut j.reader(), &rd(), &ew_cfg()).unwrap();
        let r = stratified_probe(&mut j.reader(), &rd(), &ew_cfg(), &ew[0]).unwrap();
        let g = &r.groups[0];
        assert_eq!(g.n, [burst as u64, 1_000], "{row}");
        assert_eq!(g.composite_p50_p99.is_some(), some, "{row}");
        let (b, c) = (g.by_stratum[0].unwrap(), g.by_stratum[1].unwrap());
        assert_eq!(b.p99.is_some(), some, "{row}: Burst p99");
        assert!(c.p99.is_some(), "{row}: Calm p99");
    }
}
