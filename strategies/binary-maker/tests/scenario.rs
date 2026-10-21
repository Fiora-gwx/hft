//! Scenario tests of `binary-maker` on the SDK mock views (TESTS.md BMK-*). The binary counts
//! allocations, so every `StrategyHarness::feed` also checks that the fast part does not allocate.
#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods
)] // allow-list: test code

mod common;

use binary_maker::vol::{Vol, isqrt};
use binary_maker::{BinaryMaker, BinaryMakerParams};
use common::*;
use hft_strategy_sdk::params::{RawParams, SchemaView};
use hft_strategy_sdk::prelude::*;
use hft_strategy_sdk::testing::ev;
use hft_testkit::sdk_mock::StrategyHarness;

#[global_allocator]
static ALLOC: hft_testkit::CountingAlloc = hft_testkit::CountingAlloc;

/// Defaults with `enabled = 1`, `vol_min_samples` = 60, then `edit`.
fn params(edit: impl FnOnce(&mut Vec<i64>)) -> BinaryMakerParams {
    let schema = BinaryMaker::DESC.schema;
    let mut defaults: Vec<i64> = (0..schema.n_fields())
        .filter_map(|i| schema.field(i).map(|f| f.default))
        .collect();
    let ix = |name: &str| {
        (0..schema.n_fields())
            .position(|i| schema.field(i).is_some_and(|f| f.name == name))
            .unwrap()
    };
    defaults[ix("enabled")] = 1;
    defaults[ix("vol_min_samples")] = 60;
    edit(&mut defaults);
    BinaryMakerParams::from_raw(
        &RawParams {
            defaults,
            overrides: Vec::new(),
        },
        7,
    )
}

fn harness(p: &BinaryMakerParams) -> StrategyHarness<BinaryMaker> {
    StrategyHarness::<BinaryMaker>::new(p, &groups(), fixture(), A1).expect("params")
}

/// Reference samples once per second for `secs` seconds (mid alternating ±1 bp around 100 000.00),
/// ending at second `end_s`; then the slot books of pair `pair` (bid 0.450, ask 0.470) and one
/// last reference quote at `end_s` + 0.5 s. Returns the last capture.
fn warm(
    h: &mut StrategyHarness<BinaryMaker>,
    secs: i64,
    end_s: i64,
    pair: usize,
) -> hft_testkit::sdk_mock::CtxCapture {
    for k in 0..secs {
        let t = end_s - secs + 1 + k;
        h.views_mut().at_engine_ns(t * 1_000_000_000);
        ref_quote(h.views_mut(), if k % 2 == 0 { 10_000_000 } else { 10_001_000 });
        let b = h.views_mut().bbo_of(R);
        h.feed(&ev::bbo(R, 6, b).as_ref());
    }
    for i in [SLOTS[pair * 2], SLOTS[pair * 2 + 1]] {
        slot_book(h.views_mut(), i, 450, 470);
    }
    h.views_mut().at_engine_ns(end_s * 1_000_000_000 + 500_000_000);
    ref_quote(h.views_mut(), 10_000_500);
    let b = h.views_mut().bbo_of(R);
    h.feed(&ev::bbo(R, 6, b).as_ref())
}

#[test]
fn bmk_001_group_shape() {
    let p = params(|_| {});
    let rd = fixture();
    let check = |specs: &[GroupSpec]| p.validate(&GroupTable::new(specs, CAPACITY), &rd.view());
    let all = specs();
    assert_eq!(check(&all), Ok(()), "BMK-001.1");
    assert_eq!(
        check(&all[1..]),
        Err(ParamError::GroupShape { gix: 0, code: 1 }),
        "BMK-001.2 even"
    );
    let mut two = all.clone();
    two[0] = spec(gid(0), AssetId(TOKEN0), &[(SLOTS[0], VB), (SLOTS[1], VB)]);
    assert_eq!(
        check(&two),
        Err(ParamError::GroupShape { gix: 0, code: 2 }),
        "BMK-001.3 two legs"
    );
    let mut same = all.clone();
    same[6] = spec(gid(6), AssetId(TOKEN0), &[(SLOTS[0], VB)]);
    assert_eq!(
        check(&same),
        Err(ParamError::GroupShape { gix: 6, code: 3 }),
        "BMK-001.4 reference on the slot venue"
    );
}

#[test]
fn bmk_002_volatility() {
    assert_eq!(
        (isqrt(0), isqrt(1), isqrt(99), isqrt(100), isqrt(1 << 100)),
        (0, 1, 9, 10, 1 << 50)
    );
    let mut v = Vol::new(4);
    // 1 bp up, 1 bp down…: sigma 1 bp = 100 000.
    for (s, m) in [
        (0, 2_000_000),
        (1, 2_000_200),
        (2, 2_000_000),
        (3, 2_000_200),
        (4, 2_000_000),
    ] {
        v.on_mid(m, s);
    }
    assert_eq!(v.samples(), 4, "BMK-002.1");
    let s = v.sigma(4).unwrap();
    assert!((99_000..=100_100).contains(&s), "BMK-002.1 {s}");
    assert_eq!(v.sigma(5), None, "BMK-002.2 below min samples");
    // A quote in the same second adds no sample; a later one does (ring of 4: oldest dropped).
    v.on_mid(2_000_200, 4);
    assert_eq!(v.samples(), 4, "BMK-002.3");
    v.on_mid(2_000_000, 5);
    assert_eq!(v.samples(), 4, "BMK-002.3");
}

#[test]
fn bmk_003_quotes_both_tokens_of_the_current_window() {
    let p = params(|_| {});
    let mut h = harness(&p);
    // 400 s of 1 bp moves end at second 399: window 300..600 is pair 1, 201 s left.
    let out = warm(&mut h, 400, 399, 1);
    let places: Vec<_> = out.places().copied().collect();
    assert_eq!(places.len(), 2, "BMK-003.1 {:?}", out.items);
    for (it, inst) in places.iter().zip([SLOTS[2], SLOTS[3]]) {
        assert_eq!(
            (it.inst, it.side, it.tif, it.px, it.qty, it.venue),
            (inst, Side::Buy, Tif::PostOnly, Px(450), Qty(10_000_000), VB),
            "BMK-003.1"
        );
    }
}

#[test]
fn bmk_004_no_quotes_when_gated() {
    // .1 calm reference: sigma 0.
    let p = params(|_| {});
    let mut h = harness(&p);
    for k in 0..400 {
        h.views_mut().at_engine_ns(k * 1_000_000_000);
        ref_quote(h.views_mut(), 10_000_000);
        let b = h.views_mut().bbo_of(R);
        h.feed(&ev::bbo(R, 6, b).as_ref());
    }
    slot_book(h.views_mut(), SLOTS[2], 450, 470);
    slot_book(h.views_mut(), SLOTS[3], 450, 470);
    let out = h.feed(&ev::book(SLOTS[2], 2).as_ref());
    assert_eq!(out.places().count(), 0, "BMK-004.1");
    // .2 the last 60 s of the window: 400 s ending at second 549 (51 s left).
    let mut h = harness(&p);
    assert_eq!(warm(&mut h, 400, 549, 1).places().count(), 0, "BMK-004.2");
    // .3 disabled.
    let off = params(|d| d[0] = 0);
    let mut h = harness(&off);
    assert_eq!(warm(&mut h, 400, 399, 1).places().count(), 0, "BMK-004.3");
    // .4 bid outside [p_min, p_max]: p_min 0.5.
    let hi = params(|d| d[4] = 500_000_000);
    let mut h = harness(&hi);
    assert_eq!(warm(&mut h, 400, 399, 1).places().count(), 0, "BMK-004.4");
}

/// `warm(400, 399, 1)`, then both quotes of pair 1 live at 0.450 (OMS slots 11 and 12).
fn live_pair(h: &mut StrategyHarness<BinaryMaker>) {
    use binary_maker::fast::QState;
    warm(h, 400, 399, 1);
    for (i, slot) in [(2usize, 11u32), (3, 12)] {
        let q = &mut h.strategy_mut().fast.quotes[i];
        q.state = QState::Live;
        q.r = Some(order_ref(slot));
        q.px = 450;
    }
}

/// Reference quote with mid `mid` at engine time `t` (ns).
fn ref_at(h: &mut StrategyHarness<BinaryMaker>, t: i64, mid: i64) -> hft_testkit::sdk_mock::CtxCapture {
    h.views_mut().at_engine_ns(t);
    ref_quote(h.views_mut(), mid);
    let b = h.views_mut().bbo_of(R);
    h.feed(&ev::bbo(R, 6, b).as_ref())
}

#[test]
fn bmk_005_fair_value_cancel() {
    // Placed at reference mid 100 005.00 with Up mid 0.460, sigma ~1 bp, 200.5 s left: a 0.5 c
    // fair-value move is ~0.2 bp of the reference.
    let p = params(|_| {});
    // .1 a 0.1 bp move: nothing.
    let mut h = harness(&p);
    live_pair(&mut h);
    let out = ref_at(&mut h, 399_600_000_000, 10_000_400);
    assert!(cancels(&out).is_empty(), "BMK-005.1 {:?}", out.items);
    // .2 0.5 bp down: the Up bid is cancelled, the Down bid stays.
    let mut h = harness(&p);
    live_pair(&mut h);
    let up_q = h.strategy().fast.quotes[2];
    let out = ref_at(&mut h, 399_600_000_000, 10_000_000);
    assert!(h.strategy().fast.dfv(&up_q) <= -5_000_000, "BMK-005.2");
    assert_eq!(cancels(&out), vec![11], "BMK-005.2");
    // .3 0.5 bp up: only the Down bid.
    let mut h = harness(&p);
    live_pair(&mut h);
    let out = ref_at(&mut h, 399_600_000_000, 10_001_000);
    assert_eq!(cancels(&out), vec![12], "BMK-005.3");
    // .4 after the cancel completes the Up leg waits fv_cool_ms (2 s): nothing at +1 s, a new
    // bid at +2.1 s.
    let mut h = harness(&p);
    live_pair(&mut h);
    ref_at(&mut h, 399_600_000_000, 10_000_000);
    h.views_mut().at_engine_ns(399_700_000_000);
    let out = h.feed(
        &ev::order(
            2,
            OrderUpdate {
                r: order_ref(11),
                group: gid(2),
                tag: binary_maker::fast::tag_of(2),
                kind: OrderUpdateKind::Cancelled,
            },
        )
        .as_ref(),
    );
    assert_eq!(out.places().count(), 0, "BMK-005.4");
    let out = ref_at(&mut h, 400_600_000_000, 10_000_000);
    assert_eq!(out.places().count(), 0, "BMK-005.4");
    let out = ref_at(&mut h, 401_700_000_000, 10_000_000);
    let insts: Vec<InstId> = out.places().map(|it| it.inst).collect();
    assert_eq!(insts, vec![SLOTS[2]], "BMK-005.4");
}

#[test]
fn bmk_006_snapshot_roundtrip() {
    let p = params(|_| {});
    let mut h = harness(&p);
    warm(&mut h, 120, 399, 1);
    h.snapshot_roundtrip().expect("snapshot → restore → snapshot");
}

fn order_ref(slot: u32) -> OrderRef {
    OrderRef {
        venue: VB,
        account: A1,
        _pad: [0; 2],
        slot,
        r#gen: 1,
    }
}

fn cancels(out: &hft_testkit::sdk_mock::CtxCapture) -> Vec<u32> {
    out.items
        .iter()
        .filter_map(|c| match c {
            hft_testkit::sdk_mock::Captured::Cancel(r) => Some(r.r.slot),
            _ => None,
        })
        .collect()
}

#[test]
fn bmk_007_requote_and_pull() {
    use binary_maker::fast::QState;
    let p = params(|_| {});
    let mut h = harness(&p);
    warm(&mut h, 400, 399, 1);
    // Both quotes of pair 1 live at 0.450 (as after `Placed` outcomes).
    for (i, slot) in [(2usize, 11u32), (3, 12)] {
        let q = &mut h.strategy_mut().fast.quotes[i];
        q.state = QState::Live;
        q.r = Some(order_ref(slot));
        q.px = 450;
    }
    // .1 the Up bid moves to 0.460: its quote is cancelled (the Down quote stays).
    slot_book(h.views_mut(), SLOTS[2], 460, 470);
    h.views_mut().at_engine_ns(399_900_000_000);
    let out = h.feed(&ev::book(SLOTS[2], 2).as_ref());
    assert_eq!(cancels(&out), vec![11], "BMK-007.1");
    assert_eq!(out.places().count(), 0, "BMK-007.1");
    // .2 the window's last 60 s: every live quote is pulled.
    h.views_mut().at_engine_ns(541_000_000_000);
    ref_quote(h.views_mut(), 10_000_500);
    let b = h.views_mut().bbo_of(R);
    let out = h.feed(&ev::bbo(R, 6, b).as_ref());
    assert_eq!(cancels(&out), vec![12], "BMK-007.2");
}

#[test]
fn bmk_008_inventory_cap() {
    let p = params(|_| {});
    let mut h = harness(&p);
    // Long 100 Up shares of pair 1 (inv_max_shares = 100): only the Down token is bid.
    let mut pos = GroupPosView::EMPTY;
    pos.n_legs = 1;
    pos.legs[0].qty = Qty(100_000_000);
    h.views_mut().group_pos(A1, gid(2), pos);
    let out = warm(&mut h, 400, 399, 1);
    let insts: Vec<InstId> = out.places().map(|it| it.inst).collect();
    assert_eq!(insts, vec![SLOTS[3]], "BMK-008.1");
}

#[test]
fn bmk_009_subscriptions_and_trades() {
    let p = params(|_| {});
    let h = harness(&p);
    let rd = refdata();
    let room = vec![16u16; 16];
    let lim = SubsLimits {
        rd: &rd,
        feeds: VenueSet::EMPTY.with(VB).with(VR),
        room: &room,
    };
    let mut sb = SubsBuilder::new(lim, 16);
    h.strategy().subscriptions(&groups(), &mut sb);
    let got: Vec<_> = sb
        .entries()
        .iter()
        .map(|e| (e.inst, e.gix, e.ev, e.part))
        .collect();
    let mut want: Vec<_> = SLOTS
        .iter()
        .enumerate()
        .map(|(k, i)| (*i, k as u16, EvMask(EvMask::BOOK.0 | EvMask::TRADE.0), Part::Fast))
        .collect();
    want.push((R, 6, EvMask::BBO, Part::Fast));
    assert_eq!(got, want, "BMK-009.1");
    let mut h = harness(&p);
    warm(&mut h, 400, 399, 1);
    let t = TradeView {
        px: Px(450),
        qty: Qty(5_000_000),
        side: Side::Sell,
        ex_ts_ns: 1,
    };
    let out = h.feed(&ev::trade(SLOTS[2], 2, t).as_ref());
    assert!(out.items.is_empty(), "BMK-009.2 {:?}", out.items);
}

#[test]
fn bmk_010_stay_when_the_level_clears() {
    let p = params(|_| {});
    let mut h = harness(&p);
    live_pair(&mut h);
    // .1 the Up bid level 0.450 empties (best bid 0.440): the quote stays.
    slot_book(h.views_mut(), SLOTS[2], 440, 470);
    h.views_mut().at_engine_ns(399_600_000_000);
    let out = h.feed(&ev::book(SLOTS[2], 2).as_ref());
    assert!(out.items.is_empty(), "BMK-010.1 {:?}", out.items);
    // .2 a better bid (0.460): cancelled to requote.
    slot_book(h.views_mut(), SLOTS[2], 460, 470);
    h.views_mut().at_engine_ns(399_700_000_000);
    let out = h.feed(&ev::book(SLOTS[2], 2).as_ref());
    assert_eq!(cancels(&out), vec![11], "BMK-010.2");
}

/// Long `qty` shares of slot group `k` at average cost `px` (1/1000).
fn hold(h: &mut StrategyHarness<BinaryMaker>, k: u32, qty: i64, px: i64) {
    let mut pos = GroupPosView::EMPTY;
    pos.n_legs = 1;
    pos.legs[0].qty = Qty(qty * 1_000_000);
    pos.legs[0].entry_px = Px(px);
    h.views_mut().group_pos(A1, gid(k), pos);
}

#[test]
fn bmk_011_pair_cost_limit() {
    let p = params(|_| {});
    // .1 holding 50 Down at 0.560: the Up bid rests at 1 − 0.560 − 0.01 = 0.430 (below the
    // 0.450 best bid); the Down bid at the best bid.
    let mut h = harness(&p);
    hold(&mut h, 3, 50, 560);
    let out = warm(&mut h, 400, 399, 1);
    let got: Vec<(InstId, Px)> = out.places().map(|it| (it.inst, it.px)).collect();
    assert_eq!(got, vec![(SLOTS[2], Px(430)), (SLOTS[3], Px(450))], "BMK-011.1");
    // .2 a live Up bid at 0.450 above the limit is cancelled.
    let mut h = harness(&p);
    live_pair(&mut h);
    hold(&mut h, 3, 50, 560);
    h.views_mut().at_engine_ns(399_600_000_000);
    let out = h.feed(&ev::book(SLOTS[2], 2).as_ref());
    assert_eq!(cancels(&out), vec![11], "BMK-011.2");
    // .3 a limit below p_min (Down at 0.950 → 0.040): no Up bid.
    let mut h = harness(&p);
    hold(&mut h, 3, 50, 950);
    let out = warm(&mut h, 400, 399, 1);
    let insts: Vec<InstId> = out.places().map(|it| it.inst).collect();
    assert_eq!(insts, vec![SLOTS[3]], "BMK-011.3");
}

#[test]
fn bmk_012_normal_cdf() {
    use binary_maker::norm::{cdf, inv_cdf};
    let near = |a: i64, b: i64, tol: i64| (a - b).abs() <= tol;
    // .1 Φ at 0, ±1, 1.96 and beyond the table (1e-9; tolerance 2e-5).
    for (z, want) in [
        (0, 500_000_000),
        (1_000_000_000, 841_344_746),
        (-1_000_000_000, 158_655_254),
        (1_960_000_000, 975_002_105),
        (9_000_000_000, 999_999_713),
    ] {
        assert!(near(cdf(z), want, 20_000), "BMK-012.1 {z} {}", cdf(z));
    }
    // .2 Φ⁻¹ round trip within 1e-3 over [0.01, 0.99]; clamped to [0.001, 0.999].
    for p in (1..100).map(|k| k * 10_000_000) {
        assert!(near(cdf(inv_cdf(p)), p, 1_000_000), "BMK-012.2 {p}");
    }
    assert_eq!(inv_cdf(0), inv_cdf(1_000_000), "BMK-012.2");
    assert!(near(inv_cdf(500_000_000), 0, 1_000_000), "BMK-012.2");
}
