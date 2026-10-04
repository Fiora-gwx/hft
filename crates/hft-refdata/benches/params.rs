//! Four-leg `check_group` (REF-086): F0 plus Bitget futures and Gate spot, account 1 on
//! Binance USD-M, OKX spot, Bitget futures and Gate spot.

#[path = "../tests/common/f0.rs"]
mod f0;

use std::hint::black_box;

use criterion::{Criterion, criterion_group, criterion_main};
use hft_refdata::{
    AccountVenueSpec, Exchange, FeeSpec, GroupSpecView, InstKind, LegView, RefData, StaleGuardKind,
};
use hft_types::{AccountId, InstId, VenueId};

fn four_exchanges() -> RefData {
    let mut s = f0::f0_spec();
    let (bg, gt) = (VenueId::BITGET_UTA_USDT_FUT, VenueId::GATE_SPOT);
    s.add_venue(f0::venue(
        gt,
        Exchange::Gate,
        InstKind::Spot,
        StaleGuardKind::None,
        28,
    ));
    s.add_venue(f0::venue(
        bg,
        Exchange::Bitget,
        InstKind::Perp,
        StaleGuardKind::None,
        40,
    ));
    s.add_caps(f0::spot_caps(gt, None));
    s.add_caps(f0::perp_caps(bg));
    s.fees.push(FeeSpec {
        venue: gt,
        tier: 0,
        fee: f0::fee(200_000, 500_000),
    });
    s.fees.push(FeeSpec {
        venue: bg,
        tier: 0,
        fee: f0::fee(200_000, 500_000),
    });
    s.fees.push(FeeSpec {
        venue: gt,
        tier: 1,
        fee: f0::fee(200_000, 500_000),
    });
    s.fees.push(FeeSpec {
        venue: bg,
        tier: 1,
        fee: f0::fee(200_000, 500_000),
    });
    s.insts.push(f0::inst(
        6,
        "BTCUSDT",
        bg,
        InstKind::Perp,
        f0::BTC,
        ("0.1", 10_000_000),
        ("0.001", 100_000),
        ("0.001", 100_000),
    ));
    s.insts.push(f0::inst(
        7,
        "BTC_USDT",
        gt,
        InstKind::Spot,
        f0::BTC,
        ("0.1", 10_000_000),
        ("0.00001", 1_000),
        ("0.00001", 1_000),
    ));
    s.account_mut(1).venues = f0::vbits(&[1, 4, 6, 10]);
    s.account_venues.push(AccountVenueSpec {
        account: AccountId(1),
        venue: gt,
        uid: "3001".into(),
    });
    s.account_venues.push(AccountVenueSpec {
        account: AccountId(1),
        venue: bg,
        uid: "4001".into(),
    });
    RefData::load(&s.build(), None).unwrap_or_else(|r| panic!("{:?}", r.errors))
}

fn ref_086_refdata_check_group(c: &mut Criterion) {
    let rd = four_exchanges();
    let legs = [
        LegView {
            inst: InstId(1),
            venue: VenueId::BINANCE_USDM,
        },
        LegView {
            inst: InstId(4),
            venue: VenueId::OKX_SPOT,
        },
        LegView {
            inst: InstId(6),
            venue: VenueId::BITGET_UTA_USDT_FUT,
        },
        LegView {
            inst: InstId(7),
            venue: VenueId::GATE_SPOT,
        },
    ];
    let g = GroupSpecView {
        asset: f0::BTC,
        legs: &legs,
    };
    assert!(rd.check_group(AccountId(1), &g).is_ok());
    c.bench_function("refdata_check_group", |b| {
        b.iter(|| rd.check_group(black_box(AccountId(1)), black_box(&g)))
    });
}

criterion_group!(benches, ref_086_refdata_check_group);
criterion_main!(benches);
