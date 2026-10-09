//! `ImpactModel::vwap` (TESTS.md SLB-058–SLB-060).

use crate::book;

use book::{TestBook, bk1};
use hft_strat_lib::{ImpactModel, Vwap, VwapParams};
use hft_strategy_sdk::BookStatus;
use hft_types::{Px, Qty, Rate, Side};

fn vp(book_frac: i64, max_levels: u8) -> VwapParams {
    VwapParams {
        book_frac: Rate(book_frac),
        max_levels,
    }
}

fn walked(vwap: i64, levels: u8, impact: i64) -> Vwap {
    Vwap::Walked {
        vwap: Px(vwap),
        levels,
        impact: Rate(impact),
    }
}

fn insufficient(avail: i64) -> Vwap {
    Vwap::Insufficient { avail: Qty(avail) }
}

#[test]
fn slb_058_vwap_bk1() {
    let b = bk1();
    let h = 500_000_000;
    let rows: &[(&str, Side, i64, VwapParams, Vwap)] = &[
        (
            "SLB-058.1",
            Side::Buy,
            20_000_000,
            vp(h, 20),
            Vwap::Top(Px(6_001_000)),
        ),
        (
            "SLB-058.2",
            Side::Buy,
            25_000_000,
            vp(h, 20),
            Vwap::Top(Px(6_001_000)),
        ),
        (
            "SLB-058.3",
            Side::Buy,
            25_000_001,
            vp(h, 20),
            walked(6_001_000, 1, 0),
        ),
        (
            "SLB-058.4",
            Side::Buy,
            100_000_000,
            vp(h, 20),
            walked(6_001_050, 2, 8_332),
        ),
        (
            "SLB-058.5",
            Side::Buy,
            70_000_000,
            vp(h, 20),
            walked(6_001_029, 2, 4_833),
        ),
        (
            "SLB-058.6",
            Side::Buy,
            350_000_000,
            vp(h, 20),
            walked(6_001_315, 3, 52_491),
        ),
        (
            "SLB-058.7",
            Side::Buy,
            350_000_001,
            vp(h, 20),
            insufficient(350_000_000),
        ),
        (
            "SLB-058.8",
            Side::Buy,
            200_000_000,
            vp(h, 2),
            insufficient(150_000_000),
        ),
        (
            "SLB-058.9",
            Side::Buy,
            150_000_000,
            vp(h, 2),
            walked(6_001_067, 2, 11_165),
        ),
        (
            "SLB-058.10",
            Side::Buy,
            20_000_000,
            vp(h, 0),
            Vwap::Top(Px(6_001_000)),
        ),
        ("SLB-058.11", Side::Buy, 30_000_000, vp(h, 0), insufficient(0)),
        (
            "SLB-058.12",
            Side::Buy,
            350_000_000,
            vp(h, 200),
            walked(6_001_315, 3, 52_491),
        ),
        (
            "SLB-058.13",
            Side::Buy,
            20_000_000,
            vp(0, 20),
            walked(6_001_000, 1, 0),
        ),
        (
            "SLB-058.14",
            Side::Buy,
            20_000_000,
            vp(-1, 20),
            walked(6_001_000, 1, 0),
        ),
        (
            "SLB-058.15",
            Side::Sell,
            70_000_000,
            vp(h, 20),
            walked(5_999_971, 2, 4_833),
        ),
        (
            "SLB-058.16",
            Side::Sell,
            20_000_000,
            vp(h, 20),
            Vwap::Top(Px(6_000_000)),
        ),
        (
            "SLB-058.17",
            Side::Buy,
            50_000_000,
            vp(1_000_000_000, 20),
            Vwap::Top(Px(6_001_000)),
        ),
        (
            "SLB-058.18",
            Side::Buy,
            50_000_001,
            vp(1_000_000_000, 20),
            walked(6_001_001, 2, 167),
        ),
        ("SLB-058.19", Side::Buy, 0, vp(h, 20), Vwap::Top(Px(6_001_000))),
    ];
    for &(row, side, q, p, want) in rows {
        assert_eq!(ImpactModel::vwap(&b, side, Qty(q), &p), want, "{row}");
    }
}

#[test]
fn slb_059_vwap_unusable_book() {
    let p = vp(500_000_000, 20);
    let with_status = |s| {
        let mut b = bk1();
        b.status = s;
        b
    };
    let mut bids_only = bk1();
    bids_only.asks.clear();
    let five = TestBook::live(
        &[(6_000_000, 10_000_000)],
        &(0..5)
            .map(|k| (6_001_000 + k * 100, 10_000_000))
            .collect::<Vec<_>>(),
    );
    let rows: &[(&str, TestBook, Side, i64, Vwap)] = &[
        (
            "SLB-059.1",
            with_status(BookStatus::Uninit),
            Side::Buy,
            20_000_000,
            insufficient(0),
        ),
        (
            "SLB-059.2",
            with_status(BookStatus::Stale),
            Side::Buy,
            20_000_000,
            insufficient(0),
        ),
        (
            "SLB-059.3",
            with_status(BookStatus::Resyncing),
            Side::Sell,
            20_000_000,
            insufficient(0),
        ),
        (
            "SLB-059.4",
            bids_only.clone(),
            Side::Buy,
            20_000_000,
            insufficient(0),
        ),
        (
            "SLB-059.5",
            bids_only,
            Side::Sell,
            20_000_000,
            Vwap::Top(Px(6_000_000)),
        ),
        ("SLB-059.6", five, Side::Buy, 60_000_000, insufficient(50_000_000)),
    ];
    for (row, b, side, q, want) in rows {
        assert_eq!(ImpactModel::vwap(b, *side, Qty(*q), &p), *want, "{row}");
    }
}

#[test]
fn slb_060_vwap_reads() {
    let rows: &[(&str, i64, u8, usize, Option<usize>)] = &[
        ("SLB-060.1", 20_000_000, 20, 0, None),
        ("SLB-060.2", 70_000_000, 20, 1, Some(20)),
        ("SLB-060.3", 70_000_000, 200, 1, Some(64)),
    ];
    for &(row, q, max, calls, len) in rows {
        let b = bk1();
        ImpactModel::vwap(&b, Side::Buy, Qty(q), &vp(500_000_000, max));
        assert_eq!(b.levels_calls.get(), calls, "{row}");
        if let Some(n) = len {
            assert_eq!(b.last_out_len.get(), n, "{row}");
        }
    }
}
