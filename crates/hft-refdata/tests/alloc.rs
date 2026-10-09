//! Allocation rules: queries never allocate; memory of a 5 000-inst snapshot (REF-072, REF-073).

#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods
)] // allow-list: test code, not a hot path

use crate::common;

use std::hint::black_box;

use common::*;
use hft_refdata::{EndpointEnv, GroupSpecView, InstHot, LegView};
use hft_types::{AccountId, AssetId, InstId, StrategyId, VenueId};

#[test]
fn ref_072_queries_do_not_allocate() {
    let rd = ok(&f0());
    let legs = [
        LegView {
            inst: InstId(1),
            venue: VenueId(1),
        },
        LegView {
            inst: InstId(4),
            venue: VenueId(6),
        },
    ];
    let g = GroupSpecView {
        asset: BTC,
        legs: &legs,
    };
    count_start();
    for k in 0..10_000u32 {
        let id = InstId(k % 7);
        black_box(rd.inst(id));
        black_box(rd.caps(VenueId((k % 8) as u8)));
        black_box(rd.account(AccountId((k % 4) as u8)));
        black_box(rd.fees(AccountId(1), id));
        black_box(rd.symbols(VenueId(1)).get(b"BTCUSDT"));
        black_box(rd.symbols(VenueId(6)).get_num(20_001));
        black_box(rd.inst_meta(id));
        black_box(rd.asset(AssetId((k % 6) as u16)));
        black_box(rd.venue(VenueId(6)));
        black_box(rd.venue_uid(AccountId(1), VenueId(6)));
        black_box(rd.instance(StrategyId(1)));
        black_box(rd.account_of(StrategyId(1)));
        black_box(rd.insts_of_asset(BTC));
        black_box(rd.insts_of_venue(VenueId(1)));
        black_box(rd.rate_limits(VenueId(1)));
        black_box(rd.endpoints(VenueId(1), EndpointEnv::Prod));
        black_box(rd.maintenance(InstId(4), i64::from(k)));
        black_box(rd.venue_account_ix(VenueId(6), AccountId(1)));
        black_box(rd.warnings());
    }
    let (step1, _) = count_stop();
    count_start();
    for _ in 0..10_000 {
        black_box(rd.check_binding(AccountId(1), StrategyId(1), 0xFF00)).ok();
        black_box(rd.check_group(AccountId(1), &g)).ok();
        black_box(rd.check_ref_inst(InstId(2))).ok();
    }
    let (step2, _) = count_stop();
    assert_eq!((step1, step2), (0, 0));
}

#[test]
fn ref_073_memory_5000() {
    let b = universe6(5000, 20).build();
    count_start();
    let rd = ok(&b);
    let (_, live) = count_stop();
    let hot = (rd.inst_count() + 1) * size_of::<InstHot>();
    assert_eq!(hot, 320_064);
    let rest = live - b.len() as i64 - hot as i64;
    assert!(
        rest < 4_000_000,
        "allocated outside blob copy and InstHot: {rest} B"
    );
    drop(rd);
}
