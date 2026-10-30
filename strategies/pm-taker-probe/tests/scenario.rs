//! Scenario tests of `pm-taker-probe` on the SDK mock views (TESTS.md). The binary counts
//! allocations, so every driver call also checks that the fast part does not allocate.
#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods
)] // allow-list: test code

mod common;

use common::{CAPACITY, D, G1, U, VB, X, filled, fixture, g1, params, position, r_of, spec};
use hft_strategy_sdk::params::SchemaView;
use hft_strategy_sdk::{
    EvMask, GroupTable, IntentClass, IntentOutcome, OrderIntent, OrderUpdateKind, ParamError, ParamSet, Part,
    Strategy, SubsBuilder,
};
use hft_types::{Notional, Px, Qty, RejectBits, Side, Tif};
use pm_taker_probe::desc::{self, end, skip};
use pm_taker_probe::fast::{Phase, TAG_BUY, TAG_SELL};
use pm_taker_probe::{PmTakerProbe, PmTakerProbeParams};

#[global_allocator]
static ALLOC: hft_testkit::CountingAlloc = hft_testkit::CountingAlloc;

/// Arms a fresh driver (`enabled` 0 → 1) with `set`.
fn armed(set: &[(usize, i64)]) -> D {
    let mut d = D::new(set);
    d.set(&[(0, 1)]);
    d
}

/// The expected buy of 1 USDC at 0.110: 9.09 shares.
fn buy_of(px: i64, qty: i64) -> OrderIntent {
    OrderIntent::spend_buy(VB, common::A1, G1, U, Tif::Ioc, Px(px), Qty(qty), TAG_BUY)
}

#[test]
fn ptp_001_subscribes_the_leg_book() {
    use hft_strategy_sdk::{SubsLimits, VenueSet};
    let d = D::new(&[]);
    let rd = common::refdata();
    let room = vec![8u16; 16];
    let lim = SubsLimits {
        rd: &rd,
        feeds: VenueSet::EMPTY.with(VB),
        room: &room,
    };
    let mut sb = SubsBuilder::new(lim, 16);
    d.s.subscriptions(&g1(), &mut sb);
    let got: Vec<_> = sb
        .entries()
        .iter()
        .map(|e| (e.inst, e.gix, e.ev, e.part))
        .collect();
    assert_eq!(got, vec![(U, 0, EvMask::BOOK, Part::Fast)], "PTP-001.1");
}

#[test]
fn ptp_002_nothing_until_enabled_goes_0_to_1() {
    // .1 built with enabled = 1: not armed.
    let mut d = D::new(&[(0, 1)]);
    assert_eq!(d.s.fast.phase, Phase::Idle, "PTP-002.1");
    assert!(d.book().places.is_empty(), "PTP-002.1");
    // .2 SetParams with enabled still 1: nothing.
    d.set(&[(0, 1), (1, 1_500_000)]);
    assert!(d.book().places.is_empty(), "PTP-002.2");
    // .3 enabled 0, then 1: armed; the next book update sends the buy.
    d.set(&[(0, 0)]);
    d.set(&[(0, 1)]);
    assert_eq!(d.s.fast.phase, Phase::Armed, "PTP-002.3");
    let o = d.book();
    assert_eq!(
        o.places,
        vec![buy_of(110, 13_630_000)],
        "PTP-002.3 1.5 USDC at 0.110"
    );
    // .4 armed then disarmed before a quote: nothing.
    let mut d = D::new(&[]);
    d.set(&[(0, 1)]);
    d.set(&[(0, 0)]);
    assert_eq!(d.s.fast.phase, Phase::Idle, "PTP-002.4");
    assert!(d.book().places.is_empty(), "PTP-002.4");
}

#[test]
fn ptp_003_one_round_buy_then_sell_back() {
    let mut d = armed(&[]);
    // .1 the spend buy at the best ask: 1 USDC / 0.110 = 9.09 shares (lot 0.01).
    let o = d.book();
    assert_eq!(o.places, vec![buy_of(110, 9_090_000)], "PTP-003.1");
    let b = o.places[0];
    assert!(b.spend() && b.side == Side::Buy && b.tif == Tif::Ioc, "PTP-003.1");
    assert_eq!(b.class, IntentClass::Open, "PTP-003.1");
    assert_eq!(o.codes(), vec![desc::BUY_SENT], "PTP-003.1");
    assert_eq!(o.notes[0].v[..3], [110, 9_090_000, 1_000_000], "PTP-003.1");
    let r1 = o.refs[0];
    // .2 while the buy is open: more quotes send nothing; a partial fill past qty is not an end.
    assert!(d.book().places.is_empty(), "PTP-003.2");
    assert_eq!(d.s.fast.phase, Phase::BuyLive, "PTP-003.2");
    let o = d.order(r1, filled(9_090_910, false));
    assert!(o.places.is_empty() && o.notes.is_empty(), "PTP-003.2");
    // .3 the buy ends (done, no increment): BuyEnd, then the sell-back of the position (9.09091
    // shares → 9.09) at the best bid.
    position(&mut d.v, 9_090_910);
    let o = d.order(r1, filled(9_090_910, true));
    assert_eq!(o.codes(), vec![desc::BUY_END, desc::SELL_SENT], "PTP-003.3");
    assert_eq!(o.notes[0].v[..3], [end::FILLED, 9_090_910, 0], "PTP-003.3");
    assert_eq!(o.places.len(), 1, "PTP-003.3");
    let s = o.places[0];
    assert_eq!(
        (s.side, s.tif, s.class, s.flags, s.px, s.qty, s.tag, s.inst),
        (
            Side::Sell,
            Tif::Ioc,
            IntentClass::Reduce,
            0,
            Px(100),
            Qty(9_090_000),
            TAG_SELL,
            U
        ),
        "PTP-003.3"
    );
    assert_eq!(o.notes[1].v[..3], [100, 9_090_000, 9_090_910], "PTP-003.3");
    let r2 = o.refs[0];
    // .4 the sell ends: SellEnd; the round is over; no new order while enabled stays 1.
    position(&mut d.v, 910);
    let o = d.order(r2, filled(9_090_000, true));
    assert_eq!(o.codes(), vec![desc::SELL_END], "PTP-003.4");
    assert_eq!(o.notes[0].v[..3], [end::FILLED, 9_090_000, 0], "PTP-003.4");
    assert_eq!(d.s.fast.phase, Phase::Done, "PTP-003.4");
    for _ in 0..3 {
        assert!(d.book().places.is_empty(), "PTP-003.4");
    }
    assert_eq!(d.s.fast.rounds, 1, "PTP-003.4");
}

#[test]
fn ptp_004_rearm_needs_enabled_0_then_1() {
    let mut d = armed(&[(2, 0)]);
    let r = d.book().refs[0];
    let o = d.order(r, OrderUpdateKind::Cancelled);
    assert_eq!(o.codes(), vec![desc::BUY_END], "PTP-004.1 sell_back 0: no sell");
    assert_eq!(o.notes[0].v[0], end::CANCELLED, "PTP-004.1");
    assert_eq!(d.s.fast.phase, Phase::Done, "PTP-004.1");
    // .2 enabled 1 again (unchanged): nothing.
    d.set(&[(0, 1)]);
    assert!(d.book().places.is_empty(), "PTP-004.2");
    // .3 0 then 1: a second round.
    d.set(&[(0, 0)]);
    assert_eq!(d.s.fast.phase, Phase::Idle, "PTP-004.3");
    d.set(&[(0, 1)]);
    let o = d.book();
    assert_eq!(o.places, vec![buy_of(110, 9_090_000)], "PTP-004.3");
    assert_eq!(d.s.fast.rounds, 2, "PTP-004.3");
    // .4 disabled while the buy is open: the round finishes, then Idle (no Done wait).
    d.set(&[(0, 0)]);
    let r = o.refs[0];
    let o = d.order(r, OrderUpdateKind::Cancelled);
    assert_eq!(o.codes(), vec![desc::BUY_END], "PTP-004.4");
    assert_eq!(d.s.fast.phase, Phase::Idle, "PTP-004.4");
}

#[test]
fn ptp_005_spend_cap_and_parameter_schema() {
    let schema = PmTakerProbe::DESC.schema;
    let f = |i: u16| schema.field(i).unwrap();
    assert_eq!(
        (f(0).name, f(0).default, f(0).manual_only),
        ("enabled", 0, true),
        "PTP-005.1"
    );
    assert_eq!(
        (f(1).name, f(1).min, f(1).max, f(1).default),
        ("spend_usdc", 1, 2_000_000, 1_000_000),
        "PTP-005.2"
    );
    assert_eq!((f(2).name, f(2).default), ("sell_back", 1), "PTP-005.3");
    // .4 at the cap, the buy spends at most 2 USDC: 2 / 0.110 = 18.18 shares.
    let mut d = armed(&[(1, 2_000_000)]);
    let o = d.book();
    let b = o.places[0];
    assert_eq!(b.qty, Qty(18_180_000), "PTP-005.4");
    let cost = i128::from(b.px.0) * i128::from(b.qty.0);
    assert!(cost <= 2_000_000 * 1_000, "PTP-005.4 px × qty ≤ spend");
    // .5 a spend that buys less than one lot: Skipped, round over, no order.
    let mut d = armed(&[(1, 1_000)]);
    let o = d.book();
    assert!(o.places.is_empty(), "PTP-005.5");
    assert_eq!(o.codes(), vec![desc::SKIPPED], "PTP-005.5");
    assert_eq!(o.notes[0].v[0], skip::BUY_QTY_ZERO, "PTP-005.5");
    assert_eq!(d.s.fast.phase, Phase::Done, "PTP-005.5");
    assert!(d.book().places.is_empty(), "PTP-005.5");
}

#[test]
fn ptp_006_sell_back_sizing_and_skips() {
    let round = |d: &mut D, pos: i64| {
        let r = d.book().refs[0];
        position(&mut d.v, pos);
        d.order(r, filled(pos, true))
    };
    // .1 position below one lot: nothing to sell.
    let mut d = armed(&[]);
    let o = round(&mut d, 9_999);
    assert!(o.places.is_empty(), "PTP-006.1");
    assert_eq!(o.codes(), vec![desc::BUY_END, desc::SKIPPED], "PTP-006.1");
    assert_eq!(o.notes[1].v[..2], [skip::NO_POSITION, 9_999], "PTP-006.1");
    assert_eq!(d.s.fast.phase, Phase::Done, "PTP-006.1");
    // .2 no bid: skipped.
    let mut d = armed(&[]);
    let r = d.book().refs[0];
    d.quote(0, 110);
    position(&mut d.v, 9_090_910);
    let o = d.order(r, filled(9_090_910, true));
    assert!(o.places.is_empty(), "PTP-006.2");
    assert_eq!(o.notes[1].v[..2], [skip::NO_BID, 9_090_000], "PTP-006.2");
    // .3 the whole position (also what the account held before), rounded down to the lot.
    let mut d = armed(&[]);
    let o = round(&mut d, 12_345_678);
    assert_eq!(o.places[0].qty, Qty(12_340_000), "PTP-006.3");
    // .4 a buy that ended Rejected by the venue still sells what is held, never buys again.
    let mut d = armed(&[]);
    let r = d.book().refs[0];
    position(&mut d.v, 2_000_000);
    let o = d.order(
        r,
        OrderUpdateKind::Rejected {
            bits: RejectBits(0),
            venue_code: 77,
        },
    );
    assert_eq!(o.notes[0].v[..3], [end::REJECTED, 0, 77], "PTP-006.4");
    assert_eq!(o.places.len(), 1, "PTP-006.4");
    assert_eq!(o.places[0].side, Side::Sell, "PTP-006.4");
}

#[test]
fn ptp_007_gates_quotes_and_outcomes() {
    // .1 armed without an ask or with an unhealthy quote: waits; the first healthy quote sends.
    let mut d = armed(&[]);
    d.quote(100, 0);
    assert!(d.book().places.is_empty(), "PTP-007.1");
    d.quote(100, 110);
    d.v.conn(U, hft_types::Tsc(1), false, false);
    assert!(d.book().places.is_empty(), "PTP-007.1");
    d.quote(100, 120);
    assert_eq!(d.book().places, vec![buy_of(120, 8_330_000)], "PTP-007.1");
    // .2 other insts' books do not trigger.
    let mut d = armed(&[]);
    let o = d.feed(&hft_strategy_sdk::EventRef::Book { inst: X, gix: 0 });
    assert!(o.places.is_empty(), "PTP-007.2");
    // .3 Shadowed buy: BuyEnd{shadowed}; nothing held → Skipped; round over.
    let mut d = armed(&[]);
    d.next = Some(IntentOutcome::Shadowed);
    assert_eq!(d.book().places.len(), 1, "PTP-007.3");
    let o = d.book();
    assert!(o.places.is_empty(), "PTP-007.3");
    assert_eq!(o.codes(), vec![desc::BUY_END, desc::SKIPPED], "PTP-007.3");
    assert_eq!(o.notes[0].v[0], end::SHADOWED, "PTP-007.3");
    assert_eq!(d.s.fast.phase, Phase::Done, "PTP-007.3");
    // .4 a gated buy (RejectBits): BuyEnd{gated, bits}; no second buy.
    let mut d = armed(&[(2, 0)]);
    d.next = Some(IntentOutcome::Rejected(RejectBits::RATE));
    d.book();
    let o = d.book();
    assert_eq!(
        o.notes[0].v[..3],
        [end::GATED, 0, u64::from(RejectBits::RATE.0)],
        "PTP-007.4"
    );
    assert!(o.places.is_empty(), "PTP-007.4");
    assert!(d.book().places.is_empty(), "PTP-007.4");
    // .5 a gated sell ends the round.
    let mut d = armed(&[]);
    let r = d.book().refs[0];
    position(&mut d.v, 9_090_000);
    d.next = Some(IntentOutcome::Rejected(RejectBits::RATE));
    assert_eq!(d.order(r, filled(9_090_000, true)).places.len(), 1, "PTP-007.5");
    let o = d.book();
    assert_eq!(o.codes(), vec![desc::SELL_END], "PTP-007.5");
    assert_eq!(d.s.fast.phase, Phase::Done, "PTP-007.5");
    // .6 updates of other orders are ignored.
    let mut d = armed(&[]);
    d.book();
    let o = d.order(r_of(99), OrderUpdateKind::Cancelled);
    assert!(o.places.is_empty() && o.notes.is_empty(), "PTP-007.6");
    assert_eq!(d.s.fast.phase, Phase::BuyLive, "PTP-007.6");
}

#[test]
fn ptp_008_snapshot_and_restart() {
    use hft_strategy_sdk::{SnapshotReader, SnapshotWriter};
    let restored = |d: &D| {
        let mut w = SnapshotWriter::new(1 << 12);
        d.s.snapshot(&mut w);
        let mut fresh = PmTakerProbe::new(d.s.inst, &d.p, &d.groups, &d.v.refview()).unwrap();
        let mut r = SnapshotReader::with_desc_version(w.as_bytes(), PmTakerProbe::DESC.version);
        fresh.restore(&mut r).unwrap();
        let mut w2 = SnapshotWriter::new(1 << 12);
        fresh.snapshot(&mut w2);
        (fresh, w.as_bytes().to_vec(), w2.as_bytes().to_vec())
    };
    // .1 an open buy is followed after the restart (same bytes).
    let mut d = armed(&[]);
    d.book();
    d.book();
    assert_eq!(d.s.fast.phase, Phase::BuyLive, "PTP-008.1");
    let (f, a, b) = restored(&d);
    assert_eq!(a, b, "PTP-008.1");
    assert_eq!(
        (f.fast.phase, f.fast.r),
        (Phase::BuyLive, Some(r_of(1))),
        "PTP-008.1"
    );
    // .2 an armed instance is disarmed by a restart.
    let d = armed(&[]);
    let (f, _, _) = restored(&d);
    assert_eq!(f.fast.phase, Phase::Idle, "PTP-008.2");
    // .3 a place whose outcome was not read yet: the round is over (Done while enabled).
    let mut d = armed(&[]);
    d.book();
    let (f, _, _) = restored(&d);
    assert_eq!((f.fast.phase, f.fast.r), (Phase::Done, None), "PTP-008.3");
}

#[test]
fn ptp_009_group_shape() {
    let p: PmTakerProbeParams = params(&[]);
    let rd = fixture();
    assert_eq!(p.validate(&g1(), &rd.view()), Ok(()), "PTP-009.1");
    let two_legs = GroupTable::new(&[spec(G1, &[U, X])], CAPACITY);
    assert_eq!(
        p.validate(&two_legs, &rd.view()),
        Err(ParamError::GroupShape { gix: 0, code: 1 }),
        "PTP-009.2"
    );
    let two_groups = GroupTable::new(
        &[spec(G1, &[U]), spec(hft_types::GroupId(G1.0 + 1), &[X])],
        CAPACITY,
    );
    assert_eq!(
        p.validate(&two_groups, &rd.view()),
        Err(ParamError::GroupShape { gix: 1, code: 2 }),
        "PTP-009.3"
    );
    let _ = Notional(0);
}
