//! Code values of enums and bit sets (TYP-026, TYP-086, TYP-110 – TYP-114).

#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods
)] // allow-list: test code, not a hot path

use hft_types::*;
use zerocopy::{IntoBytes, TryFromBytes};

#[test]
fn typ_026_round_mode_values() {
    assert_eq!(RoundMode::Floor as u8, 1, "TYP-026.1");
    assert_eq!(RoundMode::Ceil as u8, 2, "TYP-026.2");
    assert_eq!(RoundMode::TowardZero as u8, 3, "TYP-026.3");
    assert_eq!(RoundMode::HalfEven as u8, 4, "TYP-026.4");
}

/// Decode `b`, compare with `want`, and check the variant re-encodes to `[b]`.
fn check<T>(row: &str, b: u8, want: Option<T>)
where
    T: TryFromBytes + IntoBytes + zerocopy::KnownLayout + zerocopy::Immutable + PartialEq + std::fmt::Debug,
{
    let got = T::try_read_from_bytes(&[b]).ok();
    assert_eq!(got, want, "{row}");
    if let Some(v) = got {
        assert_eq!(v.as_bytes(), [b], "{row} as_bytes");
    }
}

#[test]
fn typ_086_wire_enum_codes() {
    assert_eq!(size_of::<Side>(), 1);
    assert_eq!(size_of::<Tif>(), 1);
    assert_eq!(size_of::<MarketKind>(), 1);
    assert_eq!(size_of::<HostClass>(), 1);
    check::<Side>("TYP-086.1", 0, None);
    check("TYP-086.2", 1, Some(Side::Buy));
    check("TYP-086.3", 2, Some(Side::Sell));
    check::<Side>("TYP-086.4", 3, None);
    check::<Side>("TYP-086.5", 255, None);
    check::<Tif>("TYP-086.6", 0, None);
    check("TYP-086.7", 1, Some(Tif::Gtc));
    check("TYP-086.8", 2, Some(Tif::Ioc));
    check("TYP-086.9", 3, Some(Tif::Fok));
    check("TYP-086.10", 4, Some(Tif::PostOnly));
    check::<Tif>("TYP-086.11", 5, None);
    check::<MarketKind>("TYP-086.12", 0, None);
    check("TYP-086.13", 1, Some(MarketKind::Spot));
    check("TYP-086.14", 2, Some(MarketKind::LinearPerp));
    check::<MarketKind>("TYP-086.15", 3, None);
    check::<HostClass>("TYP-086.16", 0, None);
    check("TYP-086.17", 1, Some(HostClass::Dev2));
    check("TYP-086.18", 2, Some(HostClass::C4));
    check("TYP-086.19", 3, Some(HostClass::C8));
    check("TYP-086.20", 4, Some(HostClass::Metal));
    check("TYP-086.21", 5, Some(HostClass::C16));
    check("TYP-086.22", 6, Some(HostClass::C32));
    check::<HostClass>("TYP-086.23", 7, None);
}

#[test]
fn typ_110_run_mode_codes() {
    check::<RunMode>("TYP-110.1", 0, None);
    check("TYP-110.2", 1, Some(RunMode::Shadow));
    check::<RunMode>("TYP-110.3", 2, None);
    check("TYP-110.4", 3, Some(RunMode::CloseOnly));
    check("TYP-110.5", 4, Some(RunMode::Live));
    check::<RunMode>("TYP-110.6", 5, None);
    check::<RunMode>("TYP-110.7", 255, None);
}

#[test]
fn typ_111_kill_level_codes() {
    check::<KillLevel>("TYP-111.1", 0, None);
    check::<KillLevel>("TYP-111.2", 1, None);
    check("TYP-111.3", 2, Some(KillLevel::Venue));
    check("TYP-111.4", 3, Some(KillLevel::Account));
    check("TYP-111.5", 4, Some(KillLevel::Global));
    check::<KillLevel>("TYP-111.6", 5, None);
}

#[test]
fn typ_112_cap_source() {
    let named: [(&str, u8, Option<CapSource>, bool); 18] = [
        ("TYP-112.1", 0, None, false),
        ("TYP-112.2", 1, Some(CapSource::KILL), true),
        ("TYP-112.3", 2, Some(CapSource::DAILY_LOSS), true),
        ("TYP-112.4", 3, Some(CapSource::RECON), true),
        ("TYP-112.5", 4, Some(CapSource::UNKNOWN_TIMEOUT), true),
        ("TYP-112.6", 5, Some(CapSource::BROKEN), true),
        ("TYP-112.7", 6, Some(CapSource::REORG), true),
        ("TYP-112.8", 7, Some(CapSource::RECOVERY), true),
        ("TYP-112.9", 8, Some(CapSource::HALTED), true),
        ("TYP-112.10", 9, Some(CapSource::DRAINING), false),
        ("TYP-112.11", 10, Some(CapSource::CLOCK_BAD), false),
        ("TYP-112.12", 11, Some(CapSource::HEDGE_FAILURE), true),
        ("TYP-112.13", 12, Some(CapSource::MARGIN_STRESS), false),
        ("TYP-112.14", 13, Some(CapSource::STORAGE), false),
        ("TYP-112.15", 14, Some(CapSource::DELISTING), true),
        ("TYP-112.16", 15, Some(CapSource::ACCOUNT_CFG), false),
        ("TYP-112.17", 16, None, false),
        ("TYP-112.18", 255, None, false),
    ];
    for (row, v, constant, resumable) in named {
        if let Some(c) = constant {
            assert_eq!(c, CapSource(v), "{row} value");
        }
        assert_eq!(CapSource(v).resumable(), resumable, "{row} resumable");
    }
}

fn reject_table() -> [(&'static str, RejectBits, u32); 30] {
    [
        ("TYP-113.1", RejectBits::INVALID, 0),
        ("TYP-113.2", RejectBits::NOT_TRADABLE, 1),
        ("TYP-113.3", RejectBits::VENUE_NOT_READY, 2),
        ("TYP-113.4", RejectBits::KILLED, 3),
        ("TYP-113.5", RejectBits::CLOSE_ONLY, 4),
        ("TYP-113.6", RejectBits::HARD, 5),
        ("TYP-113.7", RejectBits::UNKNOWN_PENDING, 6),
        ("TYP-113.8", RejectBits::NO_MID, 7),
        ("TYP-113.9", RejectBits::STALE_MID, 8),
        ("TYP-113.10", RejectBits::PX_BAND, 9),
        ("TYP-113.11", RejectBits::NOTIONAL_ORDER, 10),
        ("TYP-113.12", RejectBits::NOTIONAL_GROUP, 11),
        ("TYP-113.13", RejectBits::NOTIONAL_ACCOUNT, 12),
        ("TYP-113.14", RejectBits::NOTIONAL_VENUE, 13),
        ("TYP-113.15", RejectBits::POSITION, 14),
        ("TYP-113.16", RejectBits::REDUCE_INVALID, 15),
        ("TYP-113.17", RejectBits::RATE, 16),
        ("TYP-113.18", RejectBits::RATE_BACKOFF, 17),
        ("TYP-113.19", RejectBits::DAILY_LOSS, 18),
        ("TYP-113.20", RejectBits::EXPOSURE_GATE, 19),
        ("TYP-113.21", RejectBits::RECON_DEVIATION, 20),
        ("TYP-113.22", RejectBits::CLOCK, 22),
        ("TYP-113.23", RejectBits::DRAINING, 23),
        ("TYP-113.24", RejectBits::MODE_SHADOW, 24),
        ("TYP-113.25", RejectBits::LOCAL_SEND_FAIL, 25),
        ("TYP-113.26", RejectBits::HEDGE_LIMIT, 26),
        ("TYP-113.27", RejectBits::NOT_BOUND, 27),
        ("TYP-113.28", RejectBits::LINK_STALE, 28),
        ("TYP-113.29", RejectBits::LINK_FUNDS, 29),
        ("TYP-113 capacity", RejectBits::CAPACITY, 30),
    ]
}

#[test]
fn typ_113_reject_bits_values() {
    for (row, c, bit) in reject_table() {
        assert_eq!(c, RejectBits(1 << bit), "{row}");
    }
}

#[test]
fn typ_114_reject_bits_disjoint() {
    let t = reject_table();
    for (i, (ra, a, _)) in t.iter().enumerate() {
        for (rb, b, _) in &t[i + 1..] {
            assert_eq!(a.0 & b.0, 0, "{ra} & {rb}");
        }
    }
    let all = t.iter().fold(0u32, |acc, (_, c, _)| acc | c.0);
    assert_eq!(all, 0x7FDF_FFFF);
    assert_eq!(size_of::<RejectBits>(), 4);
}
