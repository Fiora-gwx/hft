//! Scenario tests of `binary-maker` on the SDK mock views (TESTS.md BMK-*). The binary counts
//! allocations, so every `StrategyHarness::feed` also checks that the fast part does not allocate.
#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods
)] // allow-list: test code

mod common;

use binary_maker::{BinaryMaker, BinaryMakerParams};
use common::*;
use hft_feature::{RealizedVol as Vol, isqrt};
use hft_strategy_sdk::params::{RawParams, SchemaView};
use hft_strategy_sdk::prelude::*;
use hft_strategy_sdk::testing::{MockViews, ev};
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
        q.win = 300;
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
        q.win = 300;
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
    // Slot 11's cancel (399.9 s) has had no terminal update for more than `stuck_reset_ms`: it is
    // sent once more and forgotten.
    assert_eq!(cancels(&out), vec![11, 12], "BMK-007.2");
}

#[test]
fn bmk_008_inventory_cap() {
    let p = params(|_| {});
    // Long Up shares of pair 1 (inv_max_shares = 100, clip_shares = 10): the Up token is bid only
    // while the net after a whole clip stays within the cap.
    for (row, qty, want) in [
        ("BMK-008.1", 100_000_000, vec![SLOTS[3]]),
        ("BMK-008.2", 90_000_000, vec![SLOTS[2], SLOTS[3]]),
        ("BMK-008.3", 90_010_000, vec![SLOTS[3]]),
    ] {
        let mut h = harness(&p);
        let mut pos = GroupPosView::EMPTY;
        pos.n_legs = 1;
        pos.legs[0].qty = Qty(qty);
        h.views_mut().group_pos(A1, gid(2), pos);
        let out = warm(&mut h, 400, 399, 1);
        let insts: Vec<InstId> = out.places().map(|it| it.inst).collect();
        assert_eq!(insts, want, "{row}");
    }
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
    // .4/.5 a negative margin lets the pair lock at a loss: limit 1 − cost + 0.05.
    let ix = (0..BinaryMaker::DESC.schema.n_fields())
        .position(|i| {
            BinaryMaker::DESC
                .schema
                .field(i)
                .is_some_and(|f| f.name == "pair_margin")
        })
        .unwrap();
    let neg = params(|d| d[ix] = -50_000_000);
    for (row, cost, up) in [("BMK-011.4", 600, 450), ("BMK-011.5", 640, 410)] {
        let mut h = harness(&neg);
        hold(&mut h, 3, 50, cost);
        let out = warm(&mut h, 400, 399, 1);
        let got: Vec<(InstId, Px)> = out.places().map(|it| (it.inst, it.px)).collect();
        assert_eq!(got, vec![(SLOTS[2], Px(up)), (SLOTS[3], Px(450))], "{row}");
    }
    // .6 the schema range of pair_margin.
    let f = BinaryMaker::DESC
        .schema
        .field(u16::try_from(ix).unwrap())
        .unwrap();
    assert_eq!((f.min, f.max), (-100_000_000, 1_000_000_000), "BMK-011.6");
}

#[test]
fn bmk_012_normal_cdf() {
    use hft_feature::{cdf, inv_cdf};
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

/// Defaults (as `params`) with the named fields set.
fn params_with(fields: &[(&str, i64)]) -> BinaryMakerParams {
    let schema = BinaryMaker::DESC.schema;
    let ix = |name: &str| {
        (0..schema.n_fields())
            .position(|i| schema.field(i).is_some_and(|f| f.name == name))
            .unwrap()
    };
    let idx: Vec<(usize, i64)> = fields.iter().map(|(n, v)| (ix(n), *v)).collect();
    params(|d| {
        for (i, v) in idx {
            d[i] = v;
        }
    })
}

fn book_at(h: &mut StrategyHarness<BinaryMaker>, t: i64, bid: i64) -> hft_testkit::sdk_mock::CtxCapture {
    slot_book(h.views_mut(), SLOTS[2], bid, 470);
    h.views_mut().at_engine_ns(t);
    h.feed(&ev::book(SLOTS[2], 2).as_ref())
}

fn order_at(
    h: &mut StrategyHarness<BinaryMaker>,
    t: i64,
    kind: OrderUpdateKind,
) -> hft_testkit::sdk_mock::CtxCapture {
    h.views_mut().at_engine_ns(t);
    h.feed(
        &ev::order(
            2,
            OrderUpdate {
                r: order_ref(11),
                group: gid(2),
                tag: binary_maker::fast::tag_of(2),
                kind,
            },
        )
        .as_ref(),
    )
}

fn up_places(out: &hft_testkit::sdk_mock::CtxCapture) -> Vec<Px> {
    out.places()
        .filter(|it| it.inst == SLOTS[2])
        .map(|it| it.px)
        .collect()
}

#[test]
fn bmk_013_nochase_after_undercut() {
    // .1 default (nochase off): undercut at 399.6 s, cancelled at 399.7 s, rejoins at 399.9 s.
    let mut h = harness(&params(|_| {}));
    live_pair(&mut h);
    assert_eq!(
        cancels(&book_at(&mut h, 399_600_000_000, 460)),
        vec![11],
        "BMK-013.1"
    );
    order_at(&mut h, 399_700_000_000, OrderUpdateKind::Cancelled);
    assert_eq!(
        up_places(&book_at(&mut h, 399_900_000_000, 460)),
        vec![Px(460)],
        "BMK-013.1"
    );
    // .2 nochase_ms 1000: nothing before 400.6 s, then the new best bid.
    let mut h = harness(&params_with(&[("nochase_ms", 1000)]));
    live_pair(&mut h);
    assert_eq!(
        cancels(&book_at(&mut h, 399_600_000_000, 460)),
        vec![11],
        "BMK-013.2"
    );
    order_at(&mut h, 399_700_000_000, OrderUpdateKind::Cancelled);
    assert!(
        up_places(&book_at(&mut h, 399_900_000_000, 460)).is_empty(),
        "BMK-013.2"
    );
    assert!(
        up_places(&book_at(&mut h, 400_500_000_000, 460)).is_empty(),
        "BMK-013.2"
    );
    assert_eq!(
        up_places(&book_at(&mut h, 400_700_000_000, 460)),
        vec![Px(460)],
        "BMK-013.2"
    );
}

fn filled(d: i64, cum: i64, done: bool) -> OrderUpdateKind {
    OrderUpdateKind::Filled {
        d: Qty(d),
        px: Px(450),
        fee: 0,
        fee_asset: USDC,
        cum: Qty(cum),
        done,
    }
}

#[test]
fn bmk_014_streak_pause() {
    // Two fills of the Up leg 0.2 s apart (the second completes the order).
    let run = |p: &BinaryMakerParams| {
        let mut h = harness(p);
        live_pair(&mut h);
        order_at(&mut h, 399_600_000_000, filled(5_000_000, 5_000_000, false));
        order_at(&mut h, 399_800_000_000, filled(5_000_000, 10_000_000, true));
        let mut placed = Vec::new();
        for t in [400_100_000_000i64, 409_500_000_000, 409_700_000_000] {
            placed.push(!up_places(&book_at(&mut h, t, 450)).is_empty());
        }
        placed
    };
    // .1 off: the leg rejoins at once.
    assert!(run(&params(|_| {}))[0], "BMK-014.1");
    // .2 streak_k 2 / 10 s / 10 s: paused while the first of the two fills is within 10 s and the
    // last one less than 10 s ago, i.e. until 409.6 s (as the August replay).
    let p = params_with(&[
        ("streak_k", 2),
        ("streak_window_ms", 10_000),
        ("streak_pause_ms", 10_000),
    ]);
    assert_eq!(run(&p), vec![false, false, true], "BMK-014.2");
    // .3 fills 12 s apart are not a streak.
    let mut h = harness(&p);
    live_pair(&mut h);
    order_at(&mut h, 387_000_000_000, filled(5_000_000, 5_000_000, false));
    order_at(&mut h, 399_800_000_000, filled(5_000_000, 10_000_000, true));
    assert!(
        !up_places(&book_at(&mut h, 400_100_000_000, 450)).is_empty(),
        "BMK-014.3"
    );
}

#[test]
fn bmk_015_window_cost_cap() {
    let p = params_with(&[("max_window_cost", 20_000_000)]);
    // .1 50 Up held at 0.45 = 22.50 USDC ≥ 20: no new quote on either leg.
    let mut h = harness(&p);
    hold(&mut h, 2, 50, 450);
    assert_eq!(warm(&mut h, 400, 399, 1).places().count(), 0, "BMK-015.1");
    // .2 30 Up at 0.45 = 13.50 USDC: both legs quote.
    let mut h = harness(&p);
    hold(&mut h, 2, 30, 450);
    assert_eq!(warm(&mut h, 400, 399, 1).places().count(), 2, "BMK-015.2");
    // .3 a live quote is not cancelled by the cap.
    let mut h = harness(&p);
    live_pair(&mut h);
    hold(&mut h, 2, 50, 450);
    assert!(
        cancels(&book_at(&mut h, 399_600_000_000, 450)).is_empty(),
        "BMK-015.3"
    );
}

/// Order update `kind` of the order in slot `slot` of slot group `gix` at engine time `t` (ns).
fn update_at(
    h: &mut StrategyHarness<BinaryMaker>,
    t: i64,
    gix: u16,
    slot: u32,
    kind: OrderUpdateKind,
) -> hft_testkit::sdk_mock::CtxCapture {
    h.views_mut().at_engine_ns(t);
    h.feed(
        &ev::order(
            gix,
            OrderUpdate {
                r: order_ref(slot),
                group: gid(u32::from(gix)),
                tag: binary_maker::fast::tag_of(gix),
                kind,
            },
        )
        .as_ref(),
    )
}

/// Sets slot group `i`'s quote to `state` with order `slot`, price 0.450, window start `win`.
fn set_quote(
    h: &mut StrategyHarness<BinaryMaker>,
    i: usize,
    state: binary_maker::fast::QState,
    slot: u32,
    win: i64,
    last_ens: i64,
) {
    let q = &mut h.strategy_mut().fast.quotes[i];
    q.state = state;
    q.r = Some(order_ref(slot));
    q.px = 450;
    q.win = win;
    q.last_ens = last_ens;
}

#[test]
fn bmk_016_stuck_cancel_reset() {
    use binary_maker::fast::QState;
    use hft_strategy_sdk::params::n_fields_at;
    // .1 undercut at 399.6 s, no terminal update: nothing at 404.5 s, at 404.7 s (≥ 5 s) the
    // cancel is sent again and the leg quotes the new best bid.
    let mut h = harness(&params(|_| {}));
    live_pair(&mut h);
    assert_eq!(
        cancels(&book_at(&mut h, 399_600_000_000, 460)),
        vec![11],
        "BMK-016.1"
    );
    let out = book_at(&mut h, 404_500_000_000, 460);
    assert!(out.items.is_empty(), "BMK-016.1 {:?}", out.items);
    let out = book_at(&mut h, 404_700_000_000, 460);
    assert_eq!(cancels(&out), vec![11], "BMK-016.1");
    assert_eq!(up_places(&out), vec![Px(460)], "BMK-016.1");
    // .2 the old order's late Cancelled does not touch the new quote (slot 21).
    set_quote(&mut h, 2, QState::Live, 21, 300, 404_700_000_000);
    h.strategy_mut().fast.quotes[2].px = 460;
    let out = update_at(&mut h, 404_800_000_000, 2, 11, OrderUpdateKind::Cancelled);
    assert!(out.items.is_empty(), "BMK-016.2 {:?}", out.items);
    let q = h.strategy().fast.quotes[2];
    assert_eq!(
        (q.state, q.r.map(|r| r.slot)),
        (QState::Live, Some(21)),
        "BMK-016.2"
    );
    // .3 stuck_reset_ms 10 000: still waiting at 404.7 s.
    let mut h = harness(&params_with(&[("stuck_reset_ms", 10_000)]));
    live_pair(&mut h);
    book_at(&mut h, 399_600_000_000, 460);
    let out = book_at(&mut h, 404_700_000_000, 460);
    assert!(out.items.is_empty(), "BMK-016.3 {:?}", out.items);
    // .4 schema: field 22, since 4, default 5 000 ms; a version-3 body has the first 22 fields, a
    // version-4 body 23.
    let schema = BinaryMaker::DESC.schema;
    let f = schema.field(22).unwrap();
    assert_eq!(
        (f.name, f.since, f.default),
        ("stuck_reset_ms", 4, 5_000),
        "BMK-016.4"
    );
    assert_eq!(n_fields_at(schema, 3), Some(22), "BMK-016.4");
    assert_eq!(n_fields_at(schema, 4), Some(23), "BMK-016.4");
}

#[test]
fn bmk_017_window_change_clears_stale_quotes() {
    use binary_maker::fast::QState;
    // Pair 1 is current again in the window 1200–1500 (4 mod 3 = 1). Its quotes still carry the
    // window 300 state: the Up leg Live (slot 11), the Down leg Cancelling since 1 s (slot 12).
    let mut h = harness(&params(|_| {}));
    warm(&mut h, 400, 1199, 0);
    set_quote(&mut h, 2, QState::Live, 11, 300, 399_500_000_000);
    set_quote(&mut h, 3, QState::Cancelling, 12, 300, 1_298_600_000_000);
    for i in [SLOTS[2], SLOTS[3]] {
        slot_book(h.views_mut(), i, 450, 470);
    }
    let out = ref_at(&mut h, 1_299_600_000_000, 10_000_500);
    // .1 both old orders are cancelled once more and both legs quote the new window.
    let mut c = cancels(&out);
    c.sort_unstable();
    assert_eq!(c, vec![11, 12], "BMK-017.1");
    let insts: Vec<InstId> = out.places().map(|it| it.inst).collect();
    assert_eq!(insts, vec![SLOTS[2], SLOTS[3]], "BMK-017.1");
    // .2 a late Filled{done} of the old Up order does not touch the new quote (slot 21).
    set_quote(&mut h, 2, QState::Live, 21, 1200, 1_299_600_000_000);
    let out = update_at(
        &mut h,
        1_299_700_000_000,
        2,
        11,
        filled(10_000_000, 10_000_000, true),
    );
    assert!(
        cancels(&out).is_empty() && up_places(&out).is_empty(),
        "BMK-017.2"
    );
    let q = h.strategy().fast.quotes[2];
    assert_eq!(
        (q.state, q.r.map(|r| r.slot)),
        (QState::Live, Some(21)),
        "BMK-017.2"
    );
}

#[test]
fn bmk_018_locked_book_is_not_an_undercut() {
    let mut h = harness(&params(|_| {}));
    live_pair(&mut h);
    // .1 locked (bid 0.460 = ask 0.460) and .2 crossed (bid 0.470 > ask 0.460): the quote stays.
    for (row, t, bid, ask) in [(1, 399_600_000_000i64, 460, 460), (2, 399_700_000_000, 470, 460)] {
        slot_book(h.views_mut(), SLOTS[2], bid, ask);
        h.views_mut().at_engine_ns(t);
        let out = h.feed(&ev::book(SLOTS[2], 2).as_ref());
        assert!(out.items.is_empty(), "BMK-018.{row} {:?}", out.items);
    }
    // .3 the book uncrosses with bid 0.460 < ask 0.470: undercut, cancelled.
    assert_eq!(
        cancels(&book_at(&mut h, 399_800_000_000, 460)),
        vec![11],
        "BMK-018.3"
    );
}

#[test]
fn bmk_019_aged_quote_is_cancelled() {
    use binary_maker::fast::QState;
    let mut h = harness(&params(|_| {}));
    live_pair(&mut h);
    let out = update_at(&mut h, 399_600_000_000, 2, 11, OrderUpdateKind::Aged);
    assert_eq!(cancels(&out), vec![11], "BMK-019");
    assert!(up_places(&out).is_empty(), "BMK-019");
    assert_eq!(h.strategy().fast.quotes[2].state, QState::Cancelling, "BMK-019");
}

/// Fair-value limit on (README "规则" 9) with no inventory skew and no forecast drift, then `extra`.
fn fv_params(extra: &[(&str, i64)]) -> BinaryMakerParams {
    let mut f: Vec<(&str, i64)> = vec![("fv_limit", 1), ("inv_skew", 0), ("pred_k", 0)];
    f.extend_from_slice(extra);
    params_with(&f)
}

fn placed(out: &hft_testkit::sdk_mock::CtxCapture) -> Vec<(InstId, Px)> {
    out.places().map(|it| (it.inst, it.px)).collect()
}

#[test]
fn bmk_020_fair_value_limit() {
    // .1 anchored to the Up mid 0.460: Up limit 0.46, Down 0.54; both bid the best bid 0.450.
    let mut h = harness(&fv_params(&[]));
    let out = warm(&mut h, 400, 399, 1);
    assert_eq!(
        placed(&out),
        vec![(SLOTS[2], Px(450)), (SLOTS[3], Px(450))],
        "BMK-020.1"
    );
    // .2 long 50 Up (net 50 of 100) with inv_skew 0.195: Up limit 0.46 − 0.0975 = 0.3625 → 0.360;
    // the Down bid stays at the best bid (its limit rises, its pair-cost limit is 0.540).
    let mut h = harness(&fv_params(&[("inv_skew", 195_000_000)]));
    hold(&mut h, 2, 50, 450);
    let out = warm(&mut h, 400, 399, 1);
    assert_eq!(
        placed(&out),
        vec![(SLOTS[2], Px(360)), (SLOTS[3], Px(450))],
        "BMK-020.2"
    );
    // .3 both quotes live at 0.450 (fair-value cancel of rule 5 off); the reference jumps 5 bp up
    // 0.1 s later: the anchor absorbs 1/11, the Up probability rises to ~0.56 and the Down limit
    // falls to ~0.44 < 0.450 − 0.005: only the Down bid is cancelled.
    let p = fv_params(&[("fv_cancel", 0)]);
    let mut h = harness(&p);
    live_pair(&mut h);
    let out = ref_at(&mut h, 399_600_000_000, 10_005_500);
    assert_eq!(cancels(&out), vec![12], "BMK-020.3");
    assert!(out.places().next().is_none(), "BMK-020.3");
    // .4 the Down cancel completes 0.3 s later: with the default anchor half-life (1 s) the gap is
    // ~70 % left, Down limit ~0.461 → the Down bid returns to the best bid 0.450.
    let out = update_at(&mut h, 399_900_000_000, 3, 12, OrderUpdateKind::Cancelled);
    assert_eq!(placed(&out), vec![(SLOTS[3], Px(450))], "BMK-020.4");
    // .5 the same with fv_anchor_ms 600 000 (anchor practically fixed): Down limit ~0.427 → 0.420.
    let mut h = harness(&fv_params(&[("fv_cancel", 0), ("fv_anchor_ms", 600_000)]));
    live_pair(&mut h);
    assert_eq!(
        cancels(&ref_at(&mut h, 399_600_000_000, 10_005_500)),
        vec![12],
        "BMK-020.5"
    );
    let out = update_at(&mut h, 399_900_000_000, 3, 12, OrderUpdateKind::Cancelled);
    assert_eq!(placed(&out), vec![(SLOTS[3], Px(420))], "BMK-020.5");
    // .6 fv_limit 0 (v5): the same jump cancels nothing with rule 5 off.
    let mut h = harness(&params_with(&[("fv_cancel", 0)]));
    live_pair(&mut h);
    assert!(
        cancels(&ref_at(&mut h, 399_600_000_000, 10_005_500)).is_empty(),
        "BMK-020.6"
    );
}

#[test]
fn bmk_021_forecast() {
    let near = |a: i64, b: i64, tol: i64| (a - b).abs() <= tol;
    // .1 imbalance only: bid 3, ask 1 → imbalance 0.5; forecast = 1.37249 × 0.5.
    let mut h = harness(&params_with(&[("pred_w_ret", 0), ("pred_w_ser", 0)]));
    warm(&mut h, 400, 399, 1);
    h.views_mut().at_engine_ns(399_600_000_000);
    h.views_mut().bbo(
        R,
        (Px(10_000_499), Qty(3_000_000)),
        (Px(10_000_501), Qty(1_000_000)),
    );
    let b = h.views_mut().bbo_of(R);
    h.feed(&ev::bbo(R, 6, b).as_ref());
    let f = &h.strategy().fast;
    assert_eq!(f.feat.top.imb, 500_000_000, "BMK-021.1");
    let sigma = f.feat.vol.sigma(60).unwrap();
    assert_eq!(f.forecast(sigma), 686_245_000, "BMK-021.1");
    // .2 returns only: the last five one-second returns +1, −1, +1, −1, +1 bp sum to ~1 bp;
    // / (σ √5) ≈ 0.447.
    let mut h = harness(&params_with(&[
        ("pred_w_imb", 0),
        ("pred_w_ret", 1_000_000_000),
        ("pred_w_ser", 0),
    ]));
    warm(&mut h, 400, 399, 1);
    let f = &h.strategy().fast;
    let sigma = f.feat.vol.sigma(60).unwrap();
    assert_eq!(f.feat.vol.recent(5), (100_020, 499_980), "BMK-021.2");
    assert!(
        near(f.forecast(sigma), 447_300_000, 1_000_000),
        "BMK-021.2 {}",
        f.forecast(sigma)
    );
    // .3 15 s signed efficiency only: (8 − 7) bp over 15 bp of path ≈ 0.0667.
    let mut h = harness(&params_with(&[
        ("pred_w_imb", 0),
        ("pred_w_ret", 0),
        ("pred_w_ser", 1_000_000_000),
    ]));
    warm(&mut h, 400, 399, 1);
    let f = &h.strategy().fast;
    let sigma = f.feat.vol.sigma(60).unwrap();
    assert!(
        near(f.forecast(sigma), 66_717_000, 100_000),
        "BMK-021.3 {}",
        f.forecast(sigma)
    );
    // .4 clamped to ±6: weight 10 on an imbalance of 1.
    let mut h = harness(&params_with(&[
        ("pred_w_imb", 10_000_000_000),
        ("pred_w_ret", 0),
        ("pred_w_ser", 0),
    ]));
    warm(&mut h, 400, 399, 1);
    h.strategy_mut().fast.feat.top.imb = 1_000_000_000;
    let f = &h.strategy().fast;
    assert_eq!(
        f.forecast(f.feat.vol.sigma(60).unwrap()),
        6_000_000_000,
        "BMK-021.4"
    );
}

#[test]
fn bmk_022_schema_v5() {
    use hft_strategy_sdk::params::n_fields_at;
    let schema = BinaryMaker::DESC.schema;
    assert!(schema.version() >= 5, "BMK-022.1");
    assert_eq!(n_fields_at(schema, 5), Some(32), "BMK-022.1");
    let want: [(&str, i64); 9] = [
        ("fv_limit", 0),
        ("fv_anchor_ms", 1_000),
        ("fv_edge", 0),
        ("fv_hyst", 5_000_000),
        ("inv_skew", 30_000_000),
        ("pred_k", 200_000_000),
        ("pred_w_imb", 1_372_490_000),
        ("pred_w_ret", 121_290_000),
        ("pred_w_ser", 187_200_000),
    ];
    for (k, (name, default)) in want.iter().enumerate() {
        let f = schema.field(23 + k as u16).unwrap();
        assert_eq!((f.name, f.since, f.default), (*name, 5, *default), "BMK-022.2");
    }
}

#[test]
fn bmk_023_forecast_drift() {
    // As `warm(400, 399, 1)` but the last reference quote (when the books are first quoted) has
    // bid 9 / ask 1 (imbalance 0.8): with pred_k 2 the fair Up probability rises to ~0.73, so the
    // Down bid rests at its limit (~0.27) while the Up bid stays at the best bid; pred_k 0: both
    // at 0.450.
    let run = |k: i64| {
        let mut h = harness(&fv_params(&[("pred_k", k)]));
        for t in 0..400i64 {
            h.views_mut().at_engine_ns(t * 1_000_000_000);
            ref_quote(h.views_mut(), if t % 2 == 0 { 10_000_000 } else { 10_001_000 });
            let b = h.views_mut().bbo_of(R);
            h.feed(&ev::bbo(R, 6, b).as_ref());
        }
        for i in [SLOTS[2], SLOTS[3]] {
            slot_book(h.views_mut(), i, 450, 470);
        }
        h.views_mut().at_engine_ns(399_500_000_000);
        h.views_mut().bbo(
            R,
            (Px(10_000_499), Qty(9_000_000)),
            (Px(10_000_501), Qty(1_000_000)),
        );
        let b = h.views_mut().bbo_of(R);
        placed(&h.feed(&ev::bbo(R, 6, b).as_ref()))
    };
    let drift = run(2_000_000_000);
    assert!(
        matches!(drift.as_slice(), [(u, Px(450)), (d, Px(px))] if *u == SLOTS[2] && *d == SLOTS[3] && (200..=300).contains(px)),
        "BMK-023.1 {drift:?}"
    );
    assert_eq!(
        run(0),
        vec![(SLOTS[2], Px(450)), (SLOTS[3], Px(450))],
        "BMK-023.2"
    );
}

// ------------------------------------------------------------------------------------------- v7

/// Schema v6 fields (README "参数"), in order.
const V6_FIELDS: [(&str, i64); 24] = [
    ("comp_skip", 0),
    ("comp_dir", 0),
    ("dir_cap_shares", 20),
    ("comp_every_ms", 250),
    ("comp_flow_min_shares", 50),
    ("comp_excl_own", 1),
    ("comp_imb1_mu", 6_035),
    ("comp_imb1_sd", 674_103_869),
    ("comp_imb5_mu", 3_815),
    ("comp_imb5_sd", 382_699_882),
    ("comp_mom30_mu", -1_799),
    ("comp_mom30_sd", 165_941_588),
    ("comp_mom60_mu", -5_917),
    ("comp_mom60_sd", 224_952_723),
    ("comp_bret15_mu", -28_103),
    ("comp_bret15_sd", 919_829_667),
    ("comp_bret60_mu", -24_869),
    ("comp_bret60_sd", 968_755_915),
    ("comp_flow30_mu", -49_610),
    ("comp_flow30_sd", 6_260_786_978),
    ("comp_bdrop1_mu", 1_550_071),
    ("comp_bdrop1_sd", 3_031_529_770),
    ("load_gate_ns", 0),
    ("mdslow_gate_ns", 0),
];

#[test]
fn bmk_024_schema_v6() {
    use hft_strategy_sdk::params::n_fields_at;
    let schema = BinaryMaker::DESC.schema;
    assert_eq!(schema.version(), 45, "BMK-024.1");
    assert_eq!(binary_maker::DESC_VERSION, 7, "BMK-024.1");
    assert_eq!(n_fields_at(schema, 5), Some(32), "BMK-024.1");
    assert_eq!(n_fields_at(schema, 6), Some(56), "BMK-024.1");
    for (k, (name, default)) in V6_FIELDS.iter().enumerate() {
        let f = schema.field(32 + k as u16).unwrap();
        assert_eq!((f.name, f.since, f.default), (*name, 6, *default), "BMK-024.2");
    }
    // .3 schema v7 appends dir_pair_free (default 0).
    assert_eq!(n_fields_at(schema, 7), Some(57), "BMK-024.3");
    let f = schema.field(56).unwrap();
    assert_eq!((f.name, f.since, f.default), ("dir_pair_free", 7, 0), "BMK-024.3");
    // .4 schema v8 appends the tilt and the last-minute stop-loss (all 0 = off).
    assert_eq!(n_fields_at(schema, 8), Some(61), "BMK-024.4");
    for (k, name) in ["tilt_z", "tilt_clip_shares", "late_hedge_ms", "late_hedge_z"]
        .iter()
        .enumerate()
    {
        let f = schema.field(57 + k as u16).unwrap();
        assert_eq!((f.name, f.since, f.default), (*name, 8, 0), "BMK-024.4");
    }
    // .5 schema v9 appends the opening rules (all 0 = off).
    assert_eq!(n_fields_at(schema, 9), Some(67), "BMK-024.5");
    let v9 = [
        "mm_min_path",
        "mm_max_bq",
        "early_ms",
        "early_cap_shares",
        "open_min_px",
        "open_skip_z",
    ];
    for (k, name) in v9.iter().enumerate() {
        let f = schema.field(61 + k as u16).unwrap();
        assert_eq!((f.name, f.since, f.default), (*name, 9, 0), "BMK-024.5");
    }
    // .6 schema v10 appends absorb_shares (0 = off).
    assert_eq!(n_fields_at(schema, 10), Some(68), "BMK-024.6");
    let f = schema.field(67).unwrap();
    assert_eq!(
        (f.name, f.since, f.default),
        ("absorb_shares", 10, 0),
        "BMK-024.6"
    );
    // .7 schema v11 appends open_max_px and pair_stop_ms (0 = off).
    assert_eq!(n_fields_at(schema, 11), Some(70), "BMK-024.7");
    for (k, name) in ["open_max_px", "pair_stop_ms"].iter().enumerate() {
        let f = schema.field(68 + k as u16).unwrap();
        assert_eq!((f.name, f.since, f.default), (*name, 11, 0), "BMK-024.7");
    }
    // .8 schema v12 appends the pairing phases (0 = off).
    assert_eq!(n_fields_at(schema, 12), Some(73), "BMK-024.8");
    for (k, name) in ["pair_prio_age_ms", "pair_late_ms", "pair_late_margin"]
        .iter()
        .enumerate()
    {
        let f = schema.field(70 + k as u16).unwrap();
        assert_eq!((f.name, f.since, f.default), (*name, 12, 0), "BMK-024.8");
    }
    // .9 schema v13 appends pair_late_all (0 = off).
    assert_eq!(n_fields_at(schema, 13), Some(74), "BMK-024.9");
    let f = schema.field(73).unwrap();
    assert_eq!(
        (f.name, f.since, f.default),
        ("pair_late_all", 13, 0),
        "BMK-024.9"
    ); // .10 schema v14 appends the early fast phase (0 = off).
    assert_eq!(n_fields_at(schema, 14), Some(77), "BMK-024.10");
    for (k, name) in ["fast_until_ms", "fast_requote_ms", "fast_free"]
        .iter()
        .enumerate()
    {
        let f = schema.field(74 + k as u16).unwrap();
        assert_eq!((f.name, f.since, f.default), (*name, 14, 0), "BMK-024.10");
    }
    // .11 schema v15 appends fast_pair_only (0 = off).
    assert_eq!(n_fields_at(schema, 15), Some(78), "BMK-024.11");
    let f = schema.field(77).unwrap();
    assert_eq!(
        (f.name, f.since, f.default),
        ("fast_pair_only", 15, 0),
        "BMK-024.11"
    );
    // .12 schema v16 appends fast_dir_from_ms (0 = off).
    assert_eq!(n_fields_at(schema, 16), Some(79), "BMK-024.12");
    let f = schema.field(78).unwrap();
    assert_eq!(
        (f.name, f.since, f.default),
        ("fast_dir_from_ms", 16, 0),
        "BMK-024.12"
    );
    // .13 schema v17 appends bid_offset_c (0 = off).
    assert_eq!(n_fields_at(schema, 17), Some(80), "BMK-024.13");
    let f = schema.field(79).unwrap();
    assert_eq!(
        (f.name, f.since, f.default),
        ("bid_offset_c", 17, 0),
        "BMK-024.13"
    );
    // .14 schema v18 appends open_min_imb (0 = off).
    assert_eq!(n_fields_at(schema, 18), Some(81), "BMK-024.14");
    let f = schema.field(80).unwrap();
    assert_eq!(
        (f.name, f.since, f.default),
        ("open_min_imb", 18, 0),
        "BMK-024.14"
    );
    // .15 schema v19 appends bid_improve_c, bid_improve_short (0 = off).
    assert_eq!(n_fields_at(schema, 19), Some(83), "BMK-024.15");
    for (k, name) in ["bid_improve_c", "bid_improve_short"].iter().enumerate() {
        let f = schema.field(81 + k as u16).unwrap();
        assert_eq!((f.name, f.since, f.default), (*name, 19, 0), "BMK-024.15");
    }
    // .16 schema v20 appends dir_lock, fav_max_px, fav_pair_cap (0 = off).
    assert_eq!(n_fields_at(schema, 20), Some(86), "BMK-024.16");
    for (k, name) in ["dir_lock", "fav_max_px", "fav_pair_cap"].iter().enumerate() {
        let f = schema.field(83 + k as u16).unwrap();
        assert_eq!((f.name, f.since, f.default), (*name, 20, 0), "BMK-024.16");
    }
    // .17 schema v21 appends dir_phase_ms (0 = off).
    assert_eq!(n_fields_at(schema, 21), Some(87), "BMK-024.17");
    let f = schema.field(86).unwrap();
    assert_eq!(
        (f.name, f.since, f.default),
        ("dir_phase_ms", 21, 0),
        "BMK-024.17"
    );
    // .18 schema v22 appends comp_always (0 = off).
    assert_eq!(n_fields_at(schema, 22), Some(88), "BMK-024.18");
    let f = schema.field(87).unwrap();
    assert_eq!((f.name, f.since, f.default), ("comp_always", 22, 0), "BMK-024.18");
    // .19 schema v23 appends comp_dir_max, dir_fresh_ms (0 = off).
    assert_eq!(n_fields_at(schema, 23), Some(90), "BMK-024.19");
    for (k, name) in ["comp_dir_max", "dir_fresh_ms"].iter().enumerate() {
        let f = schema.field(88 + k as u16).unwrap();
        assert_eq!((f.name, f.since, f.default), (*name, 23, 0), "BMK-024.19");
    }
    // .20 schema v24 appends the mature-level / swept / complement-up fields (0 = off).
    let v24 = [
        "open_min_bq",
        "open_min_level_ms",
        "pair_min_bq",
        "pair_min_level_ms",
        "leave_bq",
        "fav_obey_open",
        "hold_thin_undercut",
        "undercut_min_bq",
        "undercut_min_level_ms",
        "hold_below_max_ms",
        "swept_pause_ms",
        "swept_pair_cap",
        "oth_up_cancel",
        "oth_up_cool_ms",
    ];
    assert_eq!(n_fields_at(schema, 24), Some(90 + v24.len() as u16), "BMK-024.20");
    for (k, name) in v24.iter().enumerate() {
        let f = schema.field(90 + k as u16).unwrap();
        assert_eq!((f.name, f.since, f.default), (*name, 24, 0), "BMK-024.20");
    }
    // .21 schema v25 appends the rule 12 fields (settle_ms 0 = off).
    let v25 = [
        ("settle_ms", 0),
        ("settle_p_min", 950_000_000),
        ("settle_bid_lo", 900_000_000),
        ("settle_bid_hi", 970_000_000),
        ("settle_clip_shares", 20),
        ("settle_max_shares", 60),
        ("settle_vol_mult", 1_300_000_000),
        ("settle_cancel_p", 900_000_000),
        ("settle_min_lock_s", 5),
        ("settle_mu_min", 50_000),
        ("settle_requote_ms", 200),
        ("settle_improve", 0),
    ];
    let n24 = 90 + v24.len() as u16;
    assert_eq!(
        n_fields_at(schema, 25),
        Some(n24 + v25.len() as u16),
        "BMK-024.21"
    );
    for (k, (name, default)) in v25.iter().enumerate() {
        let f = schema.field(n24 + k as u16).unwrap();
        assert_eq!((f.name, f.since, f.default), (*name, 25, *default), "BMK-024.21");
    }
    // .22 schema v26 appends settle_stop_ms (0 = to the end).
    let n25 = n24 + v25.len() as u16;
    assert_eq!(n_fields_at(schema, 26), Some(n25 + 1), "BMK-024.22");
    let f = schema.field(n25).unwrap();
    assert_eq!(
        (f.name, f.since, f.default),
        ("settle_stop_ms", 26, 0),
        "BMK-024.22"
    );
    // .23 schema v27 appends dir_from_ms, pair_eager_z, pair_lazy_c (0 = off).
    assert_eq!(n_fields_at(schema, 27), Some(n25 + 4), "BMK-024.23");
    for (k, name) in ["dir_from_ms", "pair_eager_z", "pair_lazy_c"].iter().enumerate() {
        let f = schema.field(n25 + 1 + k as u16).unwrap();
        assert_eq!((f.name, f.since, f.default), (*name, 27, 0), "BMK-024.23");
    }
    // .24 schema v28 appends the first-leg rules (0 = off).
    let v28 = [
        "mkt_drop_min_c",
        "mkt_drop_pause_ms",
        "mkt_drop_need_trade",
        "pair_window_ms",
        "pair_risk_usd",
        "open_from_ms",
    ];
    assert_eq!(
        n_fields_at(schema, 28),
        Some(n25 + 4 + v28.len() as u16),
        "BMK-024.24"
    );
    for (k, name) in v28.iter().enumerate() {
        let f = schema.field(n25 + 4 + k as u16).unwrap();
        assert_eq!((f.name, f.since, f.default), (*name, 28, 0), "BMK-024.24");
    }
    // .25 schema v29 appends the dip-bid fields (dip_mode 0 = off).
    let v29 = [
        ("dip_mode", 0),
        ("dip_mom_min", 0),
        ("dip_z_max", 0),
        ("dip_ttl_ms", 0),
        ("dip_depth_k", 0),
        ("dip_range_s", 30),
        ("dip_depth_min_c", 1),
        ("dip_depth_max_c", 5),
    ];
    let n28 = n25 + 4 + v28.len() as u16;
    assert_eq!(
        n_fields_at(schema, 29),
        Some(n28 + v29.len() as u16),
        "BMK-024.25"
    );
    for (k, (name, default)) in v29.iter().enumerate() {
        let f = schema.field(n28 + k as u16).unwrap();
        assert_eq!((f.name, f.since, f.default), (*name, 29, *default), "BMK-024.25");
    }
    // .26 schema v30 appends dip_falling, dip_z_min (0 = off).
    let n29 = n28 + v29.len() as u16;
    assert_eq!(n_fields_at(schema, 30), Some(n29 + 2), "BMK-024.26");
    for (k, name) in ["dip_falling", "dip_z_min"].iter().enumerate() {
        let f = schema.field(n29 + k as u16).unwrap();
        assert_eq!((f.name, f.since, f.default), (*name, 30, 0), "BMK-024.26");
    }
    // .27 schema v31 appends chop_k, chop_er_max, dip_max_shares (0 = off).
    assert_eq!(n_fields_at(schema, 31), Some(n29 + 5), "BMK-024.27");
    for (k, name) in ["chop_k", "chop_er_max", "dip_max_shares"].iter().enumerate() {
        let f = schema.field(n29 + 2 + k as u16).unwrap();
        assert_eq!((f.name, f.since, f.default), (*name, 31, 0), "BMK-024.27");
    }
    // .28 schema v32 appends the stop-loss pairing and the rule 12 Chainlink fields (0 = off).
    let v32 = [
        "rescue_ms",
        "rescue_z",
        "rescue_px",
        "rescue_cap",
        "settle_src",
        "settle_lead_usd",
        "settle_edge",
    ];
    assert_eq!(
        n_fields_at(schema, 32),
        Some(n29 + 5 + v32.len() as u16),
        "BMK-024.28"
    );
    for (k, name) in v32.iter().enumerate() {
        let f = schema.field(n29 + 5 + k as u16).unwrap();
        assert_eq!((f.name, f.since, f.default), (*name, 32, 0), "BMK-024.28");
    }
    // .29 schema v33 appends the pressure block (0 = off).
    let n32 = n29 + 5 + v32.len() as u16;
    assert_eq!(n_fields_at(schema, 33), Some(n32 + 2), "BMK-024.29");
    for (k, name) in ["ofi_win_ms", "ofi_obi_max"].iter().enumerate() {
        let f = schema.field(n32 + k as u16).unwrap();
        assert_eq!((f.name, f.since, f.default), (*name, 33, 0), "BMK-024.29");
    }
    // .30 schema v34 appends the chop scope / trend gate and the low-volatility band (0 = off).
    let n33 = n32 + 2;
    let v34 = [
        "chop_all",
        "chop_er_min",
        "sigma_lo",
        "lo_er_max",
        "lo_obi_on",
        "lo_obi_min",
    ];
    assert_eq!(
        n_fields_at(schema, 34),
        Some(n33 + v34.len() as u16),
        "BMK-024.30"
    );
    for (k, name) in v34.iter().enumerate() {
        let f = schema.field(n33 + k as u16).unwrap();
        assert_eq!((f.name, f.since, f.default), (*name, 34, 0), "BMK-024.30");
    }
    // .31 schema v35 appends the trade-activity gate; .32 v36 the A/B sizing (0 = off).
    let n34 = n33 + v34.len() as u16;
    assert_eq!(n_fields_at(schema, 35), Some(n34 + 2), "BMK-024.31");
    for (k, name) in ["trd_win_ms", "trd_min"].iter().enumerate() {
        let f = schema.field(n34 + k as u16).unwrap();
        assert_eq!((f.name, f.since, f.default), (*name, 35, 0), "BMK-024.31");
    }
    assert_eq!(n_fields_at(schema, 36), Some(n34 + 3), "BMK-024.32");
    let f = schema.field(n34 + 2).unwrap();
    assert_eq!((f.name, f.since, f.default), ("ab_scale", 36, 0), "BMK-024.32");
    // .33 v37 appends the Coinbase guard (0 = off).
    assert_eq!(n_fields_at(schema, 37), Some(n34 + 7), "BMK-024.33");
    for (k, name) in ["cb_win_ms", "cb_shock", "cb_hold_ms", "cb_pair"]
        .iter()
        .enumerate()
    {
        let f = schema.field(n34 + 3 + k as u16).unwrap();
        assert_eq!((f.name, f.since, f.default), (*name, 37, 0), "BMK-024.33");
    }
    // .34 v38 appends the reference fusion weight (0 = off).
    assert_eq!(n_fields_at(schema, 38), Some(n34 + 8), "BMK-024.34");
    let f = schema.field(n34 + 7).unwrap();
    assert_eq!((f.name, f.since, f.default), ("ref_cb_w", 38, 0), "BMK-024.34");
    // .35 v41 (v39/v40 belong to another line): the auxiliary groups, then the fill-conditional
    // EV model (all 0 = off).
    assert_eq!(n_fields_at(schema, 40), Some(n34 + 8), "BMK-024.35");
    let v41: Vec<_> = (n34 + 8..schema.n_fields())
        .map(|i| {
            let f = schema.field(i).unwrap();
            (f.name, f.since, f.default)
        })
        .filter(|f| f.1 == 41)
        .collect();
    let names: Vec<&str> = v41.iter().map(|f| f.0).collect();
    assert_eq!(
        names,
        [
            "ref_aux",
            "fc_on",
            "fc_s",
            "fc_lam",
            "fc_c0",
            "fc_c1",
            "fc_w",
            "fc_m_post",
            "fc_m_cancel",
            "fc_m_pair",
            "fc_iv_n",
            "fc_ver"
        ],
        "BMK-024.35"
    );
    assert!(v41.iter().all(|f| f.1 == 41), "BMK-024.35 {v41:?}");
    // Off by default: no auxiliary group, rule 14 off, no pairing.
    assert_eq!(
        (v41[0].2, v41[1].2, v41[9].2),
        (0, 0, -1_000_000_000),
        "BMK-024.35"
    );
    // .36 v42 appends rule 15 (lead quote), off by default (lead_w_ms 0, lead_pc_w_ms 0, lead_only 0).
    let v42: Vec<_> = (0..schema.n_fields())
        .filter_map(|i| schema.field(i))
        .filter(|f| f.since == 42)
        .map(|f| (f.name, f.default))
        .collect();
    assert_eq!(
        v42,
        [
            ("lead_w_ms", 0),
            ("lead_x_bp", 0),
            ("lead_life_ms", 3_000),
            ("lead_cool_ms", 2_000),
            ("lead_shares", 5),
            ("lead_pc_w_ms", 0),
            ("lead_pc_x_bp", 0),
            ("lead_pc_block_ms", 1_000),
            ("lead_pc_cool_ms", 1_000),
            ("lead_only", 0)
        ],
        "BMK-024.36"
    );
    let n43 = (0..schema.n_fields())
        .filter_map(|i| schema.field(i))
        .filter(|f| f.since >= 43)
        .count() as u16;
    assert_eq!(
        n_fields_at(schema, 42),
        Some(schema.n_fields() - n43),
        "BMK-024.36"
    );
    // .37 v43 appends rule 15's trigger sources and model price; the defaults keep v42 (perpetual
    // only, best bid + 1 cent).
    let v43: Vec<_> = (0..schema.n_fields())
        .filter_map(|i| schema.field(i))
        .filter(|f| f.since == 43)
        .map(|f| (f.name, f.default))
        .collect();
    assert_eq!(
        v43,
        [
            ("lead_src", 1),
            ("lead_price", 0),
            ("lead_m", 500_000_000),
            ("lead_maxtick", 3),
            ("lead_k", 1_000_000_000)
        ],
        "BMK-024.37"
    );
    // .38 v44 appends rule 15's minimum price (0 = off keeps v43).
    let v44: Vec<_> = (0..schema.n_fields())
        .filter_map(|i| schema.field(i))
        .filter(|f| f.since == 44)
        .map(|f| (f.name, f.default))
        .collect();
    assert_eq!(v44, [("lead_p_min", 0)], "BMK-024.38");
    // .39 v45 appends rule 16, the short-horizon head (head_mode 0 = off keeps v44).
    let v45: Vec<_> = (0..schema.n_fields())
        .filter_map(|i| schema.field(i))
        .filter(|f| f.since == 45)
        .map(|f| (f.name, f.default))
        .collect();
    assert_eq!(
        v45,
        [
            ("head_mode", 0),
            ("head_on", 2_000_000_000),
            ("head_gate", 500_000_000),
            ("head_off", 0),
            ("head_life_ms", 1_000),
            ("head_cool_ms", 1_000),
            ("head_eval_ms", 25)
        ],
        "BMK-024.39"
    );
    assert_eq!(n_fields_at(schema, 44), Some(schema.n_fields() - 7), "BMK-024.39");
    assert_eq!(n_fields_at(schema, 43), Some(schema.n_fields() - 8), "BMK-024.38");
}

#[test]
fn bmk_025_integer_logarithm() {
    use hft_feature::{atanh2, ln_rate, ln_ratio};
    // Exact values (the README's arithmetic; the offline port reproduces them).
    assert_eq!(ln_rate(1_000_000_000), 0, "BMK-025.1");
    assert_eq!(ln_rate(2_000_000_000), 693_147_181, "BMK-025.1");
    assert_eq!(ln_rate(10_000_000_000), 2_302_585_091, "BMK-025.1");
    assert_eq!(ln_rate(31_000_000_000), 3_433_987_198, "BMK-025.1");
    assert_eq!(ln_rate(999_999_999), 0, "BMK-025.1 below 1");
    assert_eq!(ln_ratio(10_001, 10_000), 99_994, "BMK-025.2");
    assert_eq!(ln_ratio(10_000, 10_001), -99_994, "BMK-025.2");
    assert_eq!(ln_ratio(0, 10_000), 0, "BMK-025.2");
    assert_eq!(atanh2(-300_000_000), -619_039_204, "BMK-025.3");
    // Within 2e-8 of the true logarithm over a range of shares.
    for shares in [1i128, 7, 50, 999, 123_456, 10_000_000] {
        let x = 1_000_000_000 + shares * 1_000_000_000;
        let want = ((1 + shares) as f64).ln() * 1e9;
        let got = ln_rate(x) as f64;
        assert!((got - want).abs() <= 20.0, "BMK-025.4 {shares}: {got} vs {want}");
    }
}

fn lv(px: i64, shares: i64) -> Level {
    Level {
        px: Px(px),
        qty: Qty(shares * 1_000_000),
    }
}

#[test]
fn bmk_026_composite_score() {
    use binary_maker::features::{self as comp, BidSide, ScoreIn, SecHist, SlotHist, Trades};
    let mut v = MockViews::new(fixture());
    let (x, o) = (SLOTS[2], SLOTS[3]);
    MockViews::book(
        &mut v,
        x,
        &[lv(453, 30), lv(450, 120), lv(447, 40), lv(440, 60), lv(420, 200)],
        &[lv(470, 100)],
    );
    MockViews::book(
        &mut v,
        o,
        &[lv(540, 80), lv(530, 50), lv(500, 100)],
        &[lv(560, 100)],
    );
    // Our 30 shares at 0.453 are the whole level: the best bid as the score sees it is 0.450.
    let bx = BidSide::read(&Views::book(&v, x), Side::Buy, Some((453, 30_000_000)));
    let bo = BidSide::read(&Views::book(&v, o), Side::Buy, None);
    assert_eq!(
        (bx.best(), bx.top_size(), bx.depth(10, 5)),
        (Some(450), 120_000_000, 380_000_000),
        "BMK-026.1"
    );
    assert_eq!(
        (bo.best(), bo.top_size(), bo.depth(10, 5)),
        (Some(540), 80_000_000, 230_000_000),
        "BMK-026.1"
    );
    let mut hx = SlotHist::new(x);
    hx.mid_x2.put(1000, 920);
    hx.mid_x2.put(969, 900);
    hx.mid_x2.put(938, 880);
    hx.bid.put(1000, 453);
    hx.bid.put(998, 460);
    let mut r = SecHist::default();
    r.put(1000, 20_010_000);
    r.put(985, 20_000_000);
    r.put(940, 19_990_000);
    let mut t = Trades::default();
    let s = 1_000_000_000i64;
    t.push(969 * s, x, 500_000_000);
    t.push(975 * s, x, 100_000_000);
    t.push(990 * s, o, 60_000_000);
    t.push(995 * s, x, -70_000_000);
    t.push(996 * s, SLOTS[0], 1_000_000_000);
    let input = |hx: &SlotHist, r: &SecHist| {
        comp::features(&ScoreIn {
            x: &bx,
            o: &bo,
            hx,
            ref_x2: r,
            trades: &t,
            x_inst: x,
            o_inst: o,
            is_up: true,
            utc_ns: 1_000_400_000_000,
            implied: Some(139_000),
            unit_px: 1_000,
            unit_qty: 1_000_000,
        })
    };
    let f = input(&hx, &r);
    assert_eq!(
        f,
        [
            Some(200_000_000),
            Some(245_901_639),
            Some(10_000_000),
            Some(20_000_000),
            Some(928_539_016),
            Some(928_772_947),
            Some(-3_433_987_198),
            Some(-700_000_000),
        ],
        "BMK-026.2"
    );
    let p = params(|_| {});
    let h = harness(&p);
    let fast = &h.strategy().fast;
    assert_eq!(
        comp::score(&f, &fast.comp_mu(), &fast.comp_sd()),
        1_192_797_279,
        "BMK-026.3"
    );
    // .4 no history: the momentum and reference returns are missing (add 0), the best-bid change
    // is 0.
    let f = input(&SlotHist::new(x), &SecHist::default());
    assert_eq!(
        f,
        [
            Some(200_000_000),
            Some(245_901_639),
            None,
            None,
            None,
            None,
            Some(-3_433_987_198),
            Some(0),
        ],
        "BMK-026.4"
    );
    assert_eq!(
        comp::score(&f, &fast.comp_mu(), &fast.comp_sd()),
        525_800_236,
        "BMK-026.4"
    );
    // .5 the prints kept: at least comp_flow_min_shares (50) of a slot inst, signed by the taker
    // side, at the event's UTC time; only while the score is on.
    let trade = |h: &mut StrategyHarness<BinaryMaker>, t: i64, inst: InstId, sh: i64, side: Side| {
        h.views_mut().at_engine_ns(t);
        let tv = TradeView {
            px: Px(450),
            qty: Qty(sh * 1_000_000),
            side,
            ex_ts_ns: t,
        };
        h.feed(&ev::trade(inst, 2, tv).as_ref());
    };
    for (row, p) in [("on", comp_params(&[])), ("off", params(|_| {}))] {
        let mut h = harness(&p);
        trade(&mut h, 10_000_000_000, SLOTS[2], 49, Side::Buy);
        trade(&mut h, 11_000_000_000, SLOTS[2], 50, Side::Buy);
        trade(&mut h, 12_000_000_000, SLOTS[3], 80, Side::Buy);
        trade(&mut h, 13_000_000_000, SLOTS[2], 60, Side::Sell);
        let net = h.strategy().fast.feat.trades.net(SLOTS[2], SLOTS[3], 0);
        let want = if row == "on" { -90_000_000 } else { 0 };
        assert_eq!(net, want, "BMK-026.5 {row}");
        assert_eq!(
            h.strategy()
                .fast
                .feat
                .trades
                .net(SLOTS[2], SLOTS[3], 12_000_000_001),
            if row == "on" { -60_000_000 } else { 0 },
            "BMK-026.5 {row} window"
        );
    }
}

/// v7 rules on, the best-bid imbalance decisive (`comp_imb1_sd` 0.1), then `extra`.
fn comp_params(extra: &[(&str, i64)]) -> BinaryMakerParams {
    let mut f: Vec<(&str, i64)> = vec![
        ("comp_skip", 500_000_000),
        ("comp_dir", 500_000_000),
        ("comp_imb1_sd", 100_000_000),
    ];
    f.extend_from_slice(extra);
    params_with(&f)
}

/// Pair 1's books with bid 0.450 of `up` and `dn` shares (ask 0.470).
fn pair_books(h: &mut StrategyHarness<BinaryMaker>, up: i64, dn: i64) {
    h.views_mut().book(SLOTS[2], &[lv(450, up)], &[lv(470, 100)]);
    h.views_mut().book(SLOTS[3], &[lv(450, dn)], &[lv(470, 100)]);
}

/// `warm(400, 399, 1)` with pair 1's bid sizes `up`/`dn`.
fn warm_sized(h: &mut StrategyHarness<BinaryMaker>, up: i64, dn: i64) -> hft_testkit::sdk_mock::CtxCapture {
    for k in 0..400i64 {
        h.views_mut().at_engine_ns(k * 1_000_000_000);
        ref_quote(h.views_mut(), if k % 2 == 0 { 10_000_000 } else { 10_001_000 });
        let b = h.views_mut().bbo_of(R);
        h.feed(&ev::bbo(R, 6, b).as_ref());
    }
    pair_books(h, up, dn);
    h.views_mut().at_engine_ns(399_500_000_000);
    ref_quote(h.views_mut(), 10_000_500);
    let b = h.views_mut().bbo_of(R);
    h.feed(&ev::bbo(R, 6, b).as_ref())
}

#[test]
fn bmk_027_score_skips_the_token_it_is_against() {
    // .1 Up bid 10 shares against Down 190: Up skipped, Down quoted.
    let mut h = harness(&comp_params(&[]));
    let out = warm_sized(&mut h, 10, 190);
    let c = h.strategy().fast.comp;
    assert!(
        c.ok && c.win == 300 && c.z[0] < -500_000_000 && c.z[1] > 500_000_000,
        "BMK-027.1 {c:?}"
    );
    assert_eq!(placed(&out), vec![(SLOTS[3], Px(450))], "BMK-027.1");
    // .2 balanced books: both quoted; .3 the Up side thins out at 399.6 s: the score is still the
    // cached one (no cancel); .4 at 399.8 s it is recomputed and the Up bid is cancelled.
    let mut h = harness(&comp_params(&[]));
    let out = warm_sized(&mut h, 100, 100);
    assert_eq!(
        placed(&out),
        vec![(SLOTS[2], Px(450)), (SLOTS[3], Px(450))],
        "BMK-027.2"
    );
    for (i, slot) in [(2usize, 11u32), (3, 12)] {
        set_quote(
            &mut h,
            i,
            binary_maker::fast::QState::Live,
            slot,
            300,
            399_500_000_000,
        );
    }
    pair_books(&mut h, 10, 190);
    h.views_mut().at_engine_ns(399_600_000_000);
    let out = h.feed(&ev::book(SLOTS[2], 2).as_ref());
    assert!(
        cancels(&out).is_empty() && out.places().count() == 0,
        "BMK-027.3 {:?}",
        out.items
    );
    assert_eq!(h.strategy().fast.comp.utc_ns, 399_500_000_000, "BMK-027.3 cached");
    h.views_mut().at_engine_ns(399_800_000_000);
    let out = h.feed(&ev::book(SLOTS[2], 2).as_ref());
    assert_eq!(cancels(&out), vec![11], "BMK-027.4");
    assert_eq!(out.places().count(), 0, "BMK-027.4");
    assert_eq!(
        h.strategy().fast.comp.utc_ns,
        399_800_000_000,
        "BMK-027.4 recomputed"
    );
    // .5 comp_skip 0: the same books quote both tokens.
    let mut h = harness(&comp_params(&[("comp_skip", 0)]));
    let out = warm_sized(&mut h, 10, 190);
    assert_eq!(
        placed(&out),
        vec![(SLOTS[2], Px(450)), (SLOTS[3], Px(450))],
        "BMK-027.5"
    );
    // .6 comp_every_ms 0: recomputed on every decision (399.6 s cancels at once).
    let mut h = harness(&comp_params(&[("comp_every_ms", 0)]));
    warm_sized(&mut h, 100, 100);
    for (i, slot) in [(2usize, 11u32), (3, 12)] {
        set_quote(
            &mut h,
            i,
            binary_maker::fast::QState::Live,
            slot,
            300,
            399_500_000_000,
        );
    }
    pair_books(&mut h, 10, 190);
    h.views_mut().at_engine_ns(399_600_000_000);
    let out = h.feed(&ev::book(SLOTS[2], 2).as_ref());
    assert_eq!(cancels(&out), vec![11], "BMK-027.6");
    // .7 comp_skip −0.5 (directional maker): balanced books, both scores within ±0.5: nothing.
    let mut h = harness(&comp_params(&[("comp_skip", -500_000_000)]));
    let out = warm_sized(&mut h, 100, 100);
    let c = h.strategy().fast.comp;
    assert!(
        c.ok && c.z[0].abs() <= 500_000_000 && c.z[1].abs() <= 500_000_000,
        "BMK-027.7 {c:?}"
    );
    assert_eq!(placed(&out), vec![], "BMK-027.7");
    // .8 comp_skip −0.5, 10 / 190 shares: only the favoured Down quoted.
    let mut h = harness(&comp_params(&[("comp_skip", -500_000_000)]));
    let out = warm_sized(&mut h, 10, 190);
    assert_eq!(placed(&out), vec![(SLOTS[3], Px(450))], "BMK-027.8");
    // .9 continuing .8 with live quotes on both legs (slots 11, 12): at 399.8 s the score is
    // recomputed; the Up bid (score not above 0.5) is cancelled, the Down bid stays.
    for (i, slot) in [(2usize, 11u32), (3, 12)] {
        set_quote(
            &mut h,
            i,
            binary_maker::fast::QState::Live,
            slot,
            300,
            399_500_000_000,
        );
    }
    h.views_mut().at_engine_ns(399_800_000_000);
    let out = h.feed(&ev::book(SLOTS[2], 2).as_ref());
    assert_eq!(cancels(&out), vec![11], "BMK-027.9");
    assert_eq!(out.places().count(), 0, "BMK-027.9");
}

#[test]
fn bmk_028_direction_cap() {
    // clip 5, cap 10, dir_cap 20; long 10 Down. The score favours Down (Up 10 shares vs Down 190).
    let base = [("clip_shares", 5), ("inv_max_shares", 10)];
    let run = |extra: &[(&str, i64)], down: i64| {
        let mut f = base.to_vec();
        f.extend_from_slice(extra);
        let mut h = harness(&comp_params(&f));
        hold(&mut h, 3, down, 450);
        let out = warm_sized(&mut h, 10, 190);
        placed(&out)
    };
    // .1 10 + 5 ≤ 20: Down quoted (Up skipped).
    assert_eq!(run(&[], 10), vec![(SLOTS[3], Px(450))], "BMK-028.1");
    // .2 comp_dir 0: the cap is inv_max_shares (10 + 5 > 10): nothing.
    assert_eq!(run(&[("comp_dir", 0)], 10), vec![], "BMK-028.2");
    // .3 16 + 5 > 20: nothing.
    assert_eq!(run(&[], 16), vec![], "BMK-028.3");
    // .4 15 + 5 = 20: Down quoted.
    assert_eq!(run(&[], 15), vec![(SLOTS[3], Px(450))], "BMK-028.4");
}

#[test]
fn bmk_029_own_bid_left_out_of_the_imbalance() {
    use binary_maker::fast::QState;
    // Live Up quote (slot 11) of 10 shares at 0.450 acknowledged; the Up book's 0.450 holds 20
    // shares, the Down book's 20.
    let up_order = |filled: i64, state| OrderView {
        r: order_ref(11),
        group: gid(2),
        inst: SLOTS[2],
        side: Side::Buy,
        tif: Tif::PostOnly,
        class: IntentClass::Open,
        px: Px(450),
        qty: Qty(10_000_000),
        filled: Qty(filled),
        state,
        tag: binary_maker::fast::tag_of(2),
    };
    let z = |excl: i64, o: Option<OrderView>| {
        let mut h = harness(&comp_params(&[("comp_excl_own", excl)]));
        warm_sized(&mut h, 20, 20);
        set_quote(&mut h, 2, QState::Live, 11, 300, 399_500_000_000);
        if let Some(o) = o {
            h.views_mut().set_order(o);
        }
        // Recomputed at 399.8 s (after comp_every_ms).
        h.views_mut().at_engine_ns(399_800_000_000);
        h.feed(&ev::book(SLOTS[2], 2).as_ref());
        let c = h.strategy().fast.comp;
        assert_eq!(c.utc_ns, 399_800_000_000, "BMK-029 recomputed");
        c.z[0]
    };
    let even = z(1, None);
    // .1 our 10 shares out: Up 10 vs Down 20 (imbalance −1/3).
    let mine = z(1, Some(up_order(0, OrderStateView::Live)));
    assert!(mine < even - 1_000_000_000, "BMK-029.1 {mine} vs {even}");
    // .2 4 shares filled: 6 left out (Up 14 vs 20) — between the two.
    let part = z(1, Some(up_order(4_000_000, OrderStateView::Live)));
    assert!(mine < part && part < even, "BMK-029.2 {part}");
    // .3 not acknowledged yet, or comp_excl_own 0: nothing left out.
    assert_eq!(
        z(1, Some(up_order(0, OrderStateView::PendingNew))),
        even,
        "BMK-029.3"
    );
    assert_eq!(z(0, Some(up_order(0, OrderStateView::Live))), even, "BMK-029.3");
}

#[test]
fn bmk_030_lag_quantiles() {
    use hft_feature::{LagQuantiles as LagGate, quantile_x10};
    assert_eq!(
        quantile_x10(&[1, 2, 3, 4, 5, 6, 7, 8, 9, 10], 9),
        Some(91),
        "BMK-030.1"
    );
    assert_eq!(quantile_x10(&[1, 2, 3, 4], 5), Some(25), "BMK-030.1");
    assert_eq!(quantile_x10(&[7], 9), Some(70), "BMK-030.1");
    assert_eq!(quantile_x10(&[], 5), None, "BMK-030.1");
    let s = 1_000_000_000i64;
    let mut g = LagGate::new();
    // Minute 1 (60..119 s): second 60 lags 10..=19 ms (p90 18.1, median 14.5), second 61 one 30 ms
    // sample (p90 = median = 30), second 62 lags 1, 3 ms (p90 2.8, median 2).
    for k in 0..10 {
        g.push(60 * s + k * 1_000_000, (10 + k) * 1_000_000);
    }
    g.push(61 * s, 30_000_000);
    g.push(62 * s + 5, 3_000_000);
    g.push(62 * s + 9, 1_000_000);
    // A sample of an already closed second is dropped.
    g.push(61 * s + 1, 99_000_000);
    assert_eq!(g.late, 1, "BMK-030.2");
    // UTC enters minute 2: the minute closes. Medians ×100 ns: p90s {18.1, 30, 2.8} → 18.1 ms,
    // medians {14.5, 30, 2} → 14.5 ms.
    g.tick(120 * s + 1);
    assert_eq!(
        (g.done_min, g.done_p90, g.done_p50),
        (1, 1_810_000_000, 1_450_000_000),
        "BMK-030.3"
    );
    assert_eq!(
        g.gates(130 * s, 18_100_000, 0),
        (true, false),
        "BMK-030.4 at the threshold"
    );
    assert_eq!(g.gates(130 * s, 18_100_001, 0), (false, false), "BMK-030.4");
    assert_eq!(g.gates(130 * s, 0, 14_499_999), (false, true), "BMK-030.4");
    assert_eq!(
        g.gates(130 * s, 0, 14_500_000),
        (false, false),
        "BMK-030.4 not above"
    );
    // .5 only the previous minute counts: in minute 3 the gates are open.
    assert_eq!(g.gates(180 * s, 1, 1), (false, false), "BMK-030.5");
}

/// `warm(400, 399, 1)` with `n` market-data stamps of `U1` per second in seconds `from..=to`,
/// each with lag `lag_ms(second)` ms; params `p`.
fn warm_lag(
    p: &BinaryMakerParams,
    from: i64,
    to: i64,
    lag_ms: impl Fn(i64, i64) -> i64,
) -> Vec<(InstId, Px)> {
    let mut h = harness(p);
    for k in 0..400i64 {
        if (from..=to).contains(&k) {
            for j in 0..3 {
                let rx = k * 1_000_000_000 + j * 200_000_000;
                h.views_mut().md_push(
                    SLOTS[2],
                    MdStampView {
                        kind: MdStampView::MD_BATCH,
                        ex_ts_ns: rx - lag_ms(k, j) * 1_000_000,
                        sw_rx_ns: rx,
                        rx_tsc: Tsc(0),
                    },
                );
            }
        }
        h.views_mut().at_engine_ns(k * 1_000_000_000 + 900_000_000);
        ref_quote(h.views_mut(), if k % 2 == 0 { 10_000_000 } else { 10_001_000 });
        let b = h.views_mut().bbo_of(R);
        h.feed(&ev::bbo(R, 6, b).as_ref());
    }
    for i in [SLOTS[2], SLOTS[3]] {
        slot_book(h.views_mut(), i, 450, 470);
    }
    h.views_mut().at_engine_ns(399_950_000_000);
    ref_quote(h.views_mut(), 10_000_500);
    let b = h.views_mut().bbo_of(R);
    placed(&h.feed(&ev::bbo(R, 6, b).as_ref()))
}

#[test]
fn bmk_031_venue_load_gates() {
    let both = vec![(SLOTS[2], Px(450)), (SLOTS[3], Px(450))];
    let load = params_with(&[("load_gate_ns", 12_688_500)]);
    // The decision at 399.95 s (minute 6) reads minute 5 (300–359 s).
    // .1 13 ms lags in minute 5: closed. .2 12 ms: open.
    assert_eq!(warm_lag(&load, 300, 359, |_, _| 13), vec![], "BMK-031.1");
    assert_eq!(warm_lag(&load, 300, 359, |_, _| 12), both, "BMK-031.2");
    // .3 per-second lags 5, 5, 20 ms: p90 17 ms ≥ 12.6885 although the median is 5.
    assert_eq!(
        warm_lag(&load, 300, 359, |_, j| if j == 2 { 20 } else { 5 }),
        vec![],
        "BMK-031.3"
    );
    // .4 13 ms only in minute 4 (240–299 s), nothing in minute 5: open.
    assert_eq!(warm_lag(&load, 240, 299, |_, _| 13), both, "BMK-031.4");
    // .5 the slow feed: medians 41 ms closed, 40 ms (not above) open; load gate off.
    let slow = params_with(&[("mdslow_gate_ns", 40_000_000)]);
    assert_eq!(warm_lag(&slow, 300, 359, |_, _| 41), vec![], "BMK-031.5");
    assert_eq!(warm_lag(&slow, 300, 359, |_, _| 40), both, "BMK-031.5");
    // .6 both gates off: 100 ms lags change nothing.
    assert_eq!(warm_lag(&params(|_| {}), 300, 359, |_, _| 100), both, "BMK-031.6");
}

#[test]
fn bmk_032_gate_cancels_resting_quotes() {
    use binary_maker::fast::QState;
    // Quotes live at 0.450 placed with the gate open (12 ms in minute 5); minute 6 sees 13 ms;
    // at 420.5 s (minute 7) both are pulled.
    let p = params_with(&[("load_gate_ns", 12_688_500)]);
    let mut h = harness(&p);
    let push = |h: &mut StrategyHarness<BinaryMaker>, k: i64, ms: i64| {
        for j in 0..3 {
            let rx = k * 1_000_000_000 + j * 200_000_000;
            h.views_mut().md_push(
                SLOTS[2],
                MdStampView {
                    kind: MdStampView::MD_TRADE,
                    ex_ts_ns: rx - ms * 1_000_000,
                    sw_rx_ns: rx,
                    rx_tsc: Tsc(0),
                },
            );
        }
    };
    for k in 0..421i64 {
        push(&mut h, k, if k >= 360 { 13 } else { 12 });
        h.views_mut().at_engine_ns(k * 1_000_000_000 + 900_000_000);
        // Constant from 399 s: no fair-value cancel of the quotes placed at 400.9 s.
        let mid = if k >= 399 {
            10_000_500
        } else if k % 2 == 0 {
            10_000_000
        } else {
            10_001_000
        };
        ref_quote(h.views_mut(), mid);
        let b = h.views_mut().bbo_of(R);
        let out = h.feed(&ev::bbo(R, 6, b).as_ref());
        if k == 399 {
            for i in [SLOTS[2], SLOTS[3]] {
                slot_book(h.views_mut(), i, 450, 470);
            }
        }
        if k == 400 {
            assert_eq!(out.places().count(), 2, "BMK-032 placed at 400.9 s");
            for (i, slot) in [(2usize, 11u32), (3, 12)] {
                set_quote(&mut h, i, QState::Live, slot, 300, 400_900_000_000);
            }
        }
        if (401..420).contains(&k) {
            assert!(cancels(&out).is_empty(), "BMK-032 kept at {k}");
        }
        if k == 420 {
            assert_eq!(cancels(&out), vec![11, 12], "BMK-032.1");
        }
    }
}

#[test]
fn bmk_033_v7_off_is_v6() {
    // The v7 fields other than the switches (comp_skip, comp_dir, load_gate_ns, mdslow_gate_ns)
    // changed: every decision equals the defaults' through warm-up, fills, cancels, a reference
    // move, book changes and large prints.
    let script = |p: &BinaryMakerParams| {
        let mut h = harness(p);
        let mut all = vec![warm_sized(&mut h, 10, 190)];
        let mut t = 399_500_000_000i64;
        for step in 0..40i64 {
            t += 150_000_000;
            h.views_mut().at_engine_ns(t);
            let out = match step % 5 {
                0 => {
                    pair_books(&mut h, 10 + step, 190 - step);
                    h.feed(&ev::book(SLOTS[2], 2).as_ref())
                }
                1 => {
                    ref_quote(h.views_mut(), 10_000_500 + step * 200);
                    let b = h.views_mut().bbo_of(R);
                    h.feed(&ev::bbo(R, 6, b).as_ref())
                }
                2 => {
                    let tv = TradeView {
                        px: Px(450),
                        qty: Qty(80_000_000),
                        side: Side::Sell,
                        ex_ts_ns: t,
                    };
                    h.feed(&ev::trade(SLOTS[2], 2, tv).as_ref())
                }
                3 => {
                    h.views_mut().md_push(
                        SLOTS[3],
                        MdStampView {
                            kind: MdStampView::MD_BATCH,
                            ex_ts_ns: t - 90_000_000,
                            sw_rx_ns: t,
                            rx_tsc: Tsc(0),
                        },
                    );
                    h.feed(&ev::book(SLOTS[3], 3).as_ref())
                }
                _ => update_at(&mut h, t, 3, 12, OrderUpdateKind::Cancelled),
            };
            all.push(out);
        }
        all
    };
    let v6 = script(&params(|_| {}));
    let off = script(&params_with(&[
        ("dir_cap_shares", 1),
        ("comp_every_ms", 0),
        ("comp_flow_min_shares", 0),
        ("comp_excl_own", 0),
        ("comp_imb1_mu", 900_000_000),
        ("comp_imb1_sd", 1),
        ("comp_bdrop1_sd", 1),
    ]));
    assert!(
        v6.iter().map(|c| c.places().count()).sum::<usize>() > 0,
        "BMK-033 script places"
    );
    assert_eq!(v6, off, "BMK-033.1");
}

/// xorshift64* stream of the decision trace (BMK-034).
struct TraceRng(u64);

impl TraceRng {
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
    fn below(&mut self, n: u64) -> i64 {
        (self.next() % n.max(1)) as i64
    }
}

fn fnv(h: &mut u64, bytes: &[u8]) {
    for b in bytes {
        *h ^= u64::from(*b);
        *h = h.wrapping_mul(0x0100_0000_01b3);
    }
}

/// Decision trace of a generated stream over seven windows (reference quotes, slot books including
/// locked and crossed ones, large and small prints, market-data stamps with a load episode, fills,
/// cancels and ageing of the live quotes, a parameter change at 1 100 s): FNV-1a over every
/// captured operation, the quote states, the score cache and the fair-value anchor after every
/// event and the final snapshot; and (places, cancels, events with a valid score, fills).
fn decision_trace(p: &BinaryMakerParams, p2: &BinaryMakerParams, seed: u64) -> (u64, [u32; 4]) {
    decision_trace_with(p, p2, seed, false)
}

/// One auxiliary-group event (perpetual BBO, book or trade; Chainlink price or TWAP) at `t`;
/// it must produce no output.
fn feed_aux(h: &mut StrategyHarness<BinaryMaker>, rng: &mut TraceRng, t: i64) {
    let out = match rng.below(6) {
        0 => {
            let m = 10_000_000 + (rng.below(200) - 100) * 10;
            h.views_mut()
                .bbo(RP, (Px(m - 5), Qty(1_000_000)), (Px(m + 5), Qty(2_000_000)));
            let b = h.views_mut().bbo_of(RP);
            h.feed(&ev::bbo(RP, 8, b).as_ref())
        }
        1 => {
            h.views_mut().book(RP, &[lv(9_999_990, 5)], &[lv(10_000_010, 7)]);
            h.feed(&ev::book(RP, 8).as_ref())
        }
        2 => {
            let tv = TradeView {
                px: Px(10_000_000),
                qty: Qty(100_000),
                side: Side::Buy,
                ex_ts_ns: t,
            };
            h.feed(&ev::trade(RP, 8, tv).as_ref())
        }
        3 => h.feed(&ev::index_at(RC, 9, Px(10_000_000 + rng.below(1_000)), t - 1_200_000_000).as_ref()),
        4 => h.feed(&ev::mark_at(RC, 9, Px(10_000_000), t - 1_200_000_000).as_ref()),
        _ => return,
    };
    assert!(out.items.is_empty(), "auxiliary event produced {:?}", out.items);
}

/// `decision_trace`; `aux`: on the table with the v41 auxiliary groups (`ref_aux` = 3 in both
/// parameter sets) and with auxiliary events interleaved (their own random stream).
fn decision_trace_with(
    p: &BinaryMakerParams,
    p2: &BinaryMakerParams,
    seed: u64,
    aux: bool,
) -> (u64, [u32; 4]) {
    use binary_maker::fast::QState;
    let mut h = if aux {
        StrategyHarness::<BinaryMaker>::new(p, &GroupTable::new(&specs3(), CAPACITY), fixture(), A1)
            .expect("params")
    } else {
        harness(p)
    };
    let mut rng = TraceRng(seed);
    let mut arng = TraceRng(seed ^ 0x5eed_5eed);
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    let mut n = [0u32; 4];
    let mut mid: i64 = 10_000_000;
    let mut tok_mid: i64 = 500;
    let mut next_slot = 100u32;
    let mut pos = [(0i64, 0i64); 6];
    let mut win = 0i64;
    let mut t: i64 = 1_000_000_000;
    let mut switched = false;
    let s = 1_000_000_000i64;
    while t < 2_400 * s {
        t += 20_000_000 + rng.below(280) * 1_000_000;
        h.views_mut().at_engine_ns(t);
        if aux {
            feed_aux(&mut h, &mut arng, t);
        }
        if !switched && t >= 1_100 * s {
            h.on_params(p2).unwrap();
            switched = true;
        }
        let w = t.div_euclid(300 * s) * 300;
        if w != win {
            win = w;
            pos = [(0, 0); 6];
            for k in 0..6u32 {
                h.views_mut().group_pos(A1, gid(k), GroupPosView::EMPTY);
            }
        }
        let cur = (w / 300).rem_euclid(3) as usize;
        let r = rng.below(100);
        let out = if r < 30 {
            mid += (rng.below(5) - 2) * 600;
            let (bq, aq) = (1 + rng.below(5_000_000), 1 + rng.below(5_000_000));
            h.views_mut()
                .bbo(R, (Px(mid - 1), Qty(bq)), (Px(mid + 1), Qty(aq)));
            let b = h.views_mut().bbo_of(R);
            h.feed(&ev::bbo(R, 6, b).as_ref())
        } else if r < 72 {
            let k = if rng.below(4) == 0 {
                rng.below(6) as usize
            } else {
                cur * 2 + rng.below(2) as usize
            };
            tok_mid = (tok_mid + rng.below(11) - 5).clamp(60, 940);
            let m = if k % 2 == 0 { tok_mid } else { 1_000 - tok_mid };
            let half = 1 + rng.below(8);
            let (mut bid, mut ask) = (m - half, m + half);
            if rng.below(20) == 0 {
                ask = bid - rng.below(2);
            }
            let mut bids = Vec::new();
            let mut asks = Vec::new();
            for _ in 0..1 + rng.below(5) {
                bids.push(lv(bid, 1 + rng.below(300)));
                asks.push(lv(ask, 1 + rng.below(300)));
                bid -= 1 + rng.below(10);
                ask += 1 + rng.below(10);
            }
            h.views_mut().book(SLOTS[k], &bids, &asks);
            if r < 40 {
                // Market-data stamps of this inst: normal lag, or a load episode (300 to 900 s).
                let slow = (300 * s..900 * s).contains(&t);
                for j in 0..1 + rng.below(4) {
                    let lag = if slow {
                        100 + rng.below(300)
                    } else {
                        1 + rng.below(80)
                    };
                    let rx = t - 30_000_000 + j * 1_000_000;
                    h.views_mut().md_push(
                        SLOTS[k],
                        MdStampView {
                            kind: MdStampView::MD_BATCH,
                            ex_ts_ns: rx - lag * 1_000_000,
                            sw_rx_ns: rx,
                            rx_tsc: Tsc(0),
                        },
                    );
                }
            }
            h.feed(&ev::book(SLOTS[k], k as u16).as_ref())
        } else if r < 84 {
            let k = cur * 2 + rng.below(2) as usize;
            let tv = TradeView {
                px: Px(tok_mid),
                qty: Qty((1 + rng.below(150)) * 1_000_000),
                side: if rng.below(2) == 0 { Side::Buy } else { Side::Sell },
                ex_ts_ns: t,
            };
            h.feed(&ev::trade(SLOTS[k], k as u16, tv).as_ref())
        } else if r < 97 {
            let live: Vec<usize> = (0..6)
                .filter(|i| {
                    let q = h.strategy().fast.quotes[*i];
                    q.r.is_some() && matches!(q.state, QState::Live | QState::Cancelling)
                })
                .collect();
            if live.is_empty() {
                h.feed(&ev::timer(1, gid(0), 0).as_ref())
            } else {
                let i = live[rng.below(live.len() as u64) as usize];
                let q = h.strategy().fast.quotes[i];
                let o = q.r.unwrap();
                let kind = match rng.below(10) {
                    0..=4 => {
                        let d = (1 + rng.below(10)) * 1_000_000;
                        let (qty, cost) = pos[i];
                        pos[i] = (qty + d, cost + d * q.px);
                        let mut gp = GroupPosView::EMPTY;
                        gp.n_legs = 1;
                        gp.legs[0].qty = Qty(pos[i].0);
                        gp.legs[0].entry_px = Px(pos[i].1 / pos[i].0.max(1));
                        h.views_mut().group_pos(A1, gid(i as u32), gp);
                        n[3] += 1;
                        OrderUpdateKind::Filled {
                            d: Qty(d),
                            px: Px(q.px),
                            fee: 0,
                            fee_asset: USDC,
                            cum: Qty(d),
                            done: rng.below(3) == 0,
                        }
                    }
                    5..=7 => OrderUpdateKind::Cancelled,
                    8 => OrderUpdateKind::Aged,
                    _ => OrderUpdateKind::Acked,
                };
                h.feed(
                    &ev::order(
                        i as u16,
                        OrderUpdate {
                            r: o,
                            group: gid(i as u32),
                            tag: binary_maker::fast::tag_of(i as u16),
                            kind,
                        },
                    )
                    .as_ref(),
                )
            }
        } else {
            h.feed(&ev::timer(1, gid(0), 0).as_ref())
        };
        for c in &out.items {
            match c {
                hft_testkit::sdk_mock::Captured::Place(_) => n[0] += 1,
                hft_testkit::sdk_mock::Captured::Cancel(_) => n[1] += 1,
                _ => {}
            }
            fnv(&mut hash, format!("{c:?}").as_bytes());
        }
        // Most places are acknowledged: the quote becomes Live with a resting order.
        for (i, &inst) in SLOTS.iter().enumerate() {
            let q = h.strategy().fast.quotes[i];
            if q.state == QState::Placing && rng.below(5) != 0 {
                next_slot += 1;
                let r = OrderRef {
                    venue: VB,
                    account: A1,
                    _pad: [0; 2],
                    slot: next_slot,
                    r#gen: 1,
                };
                let m = &mut h.strategy_mut().fast.quotes[i];
                m.state = QState::Live;
                m.r = Some(r);
                h.views_mut().set_order(OrderView {
                    r,
                    group: gid(i as u32),
                    inst,
                    side: Side::Buy,
                    tif: Tif::PostOnly,
                    class: IntentClass::Open,
                    px: Px(q.px),
                    qty: Qty(10_000_000),
                    filled: Qty(0),
                    state: OrderStateView::Live,
                    tag: binary_maker::fast::tag_of(i as u16),
                });
            }
        }
        let f = &h.strategy().fast;
        if f.comp.ok {
            n[2] += 1;
        }
        fnv(
            &mut hash,
            format!("{:?}{:?}{:?}", f.quotes, f.comp, f.fv).as_bytes(),
        );
    }
    fnv(&mut hash, &h.snapshot());
    (hash, n)
}

#[test]
fn bmk_034_decision_trace_is_stable() {
    let base = params(|_| {});
    let fv = params_with(&[
        ("fv_limit", 1),
        ("nochase_ms", 500),
        ("streak_k", 2),
        ("max_window_cost", 30_000_000),
    ]);
    let comp = comp_params(&[("fv_limit", 1), ("comp_imb1_sd", 674_103_869)]);
    let lag = comp_params(&[
        ("load_gate_ns", 150_000_000),
        ("mdslow_gate_ns", 90_000_000),
        ("comp_imb1_sd", 674_103_869),
    ]);
    let got = [
        decision_trace(&base, &fv, 1),
        decision_trace(&fv, &comp, 2),
        decision_trace(&comp, &lag, 3),
        decision_trace(&lag, &base, 4),
    ];
    // Recorded with the v7 inline implementation (before the features moved to hft-feature).
    let want: [(u64, [u32; 4]); 4] = [
        (0x154a_b946_f140_8d49, [626, 533, 0, 417]),
        (0xeaeb_f288_1178_f3ae, [631, 552, 6253, 382]),
        (0x8f25_fd82_3dd8_f9a4, [1119, 977, 11589, 646]),
        (0xa5fc_cf6a_5c2b_8311, [797, 704, 2335, 536]),
    ];
    assert_eq!(got, want, "BMK-034");
}

fn with_aux(p: &BinaryMakerParams) -> BinaryMakerParams {
    let schema = BinaryMaker::DESC.schema;
    let mut raw: Vec<i64> = p.defaults().raw().to_vec();
    let k = (0..schema.n_fields())
        .position(|i| schema.field(i).is_some_and(|f| f.name == "ref_aux"))
        .unwrap();
    raw[k] = 3;
    BinaryMakerParams::from_raw(
        &RawParams {
            defaults: raw,
            overrides: Vec::new(),
        },
        7,
    )
}

#[test]
fn bmk_051_auxiliary_groups() {
    use binary_maker::params::shape;
    let rd = refdata();
    let rv = fixture();
    let aux = with_aux(&params(|_| {}));
    let g3 = GroupTable::new(&specs3(), CAPACITY);
    // .1 group shape: the auxiliary groups need ref_aux; the perpetual one must be a perpetual.
    assert_eq!(aux.check(&g3, &rv.view()), Ok(()), "BMK-051.1");
    let code = |p: &BinaryMakerParams, g: &GroupTable| match p.check(g, &rv.view()) {
        Err(ParamError::GroupShape { code, .. }) => code,
        r => panic!("{r:?}"),
    };
    assert_eq!(code(&params(|_| {}), &g3), shape::COUNT, "BMK-051.1 ref_aux 0");
    let mut swapped = specs3();
    let n = swapped.len();
    swapped.swap(n - 1, n - 2);
    let gs = GroupTable::new(&swapped, CAPACITY);
    assert_eq!(code(&aux, &gs), shape::AUX, "BMK-051.1 order");
    // .2 subscriptions: slots book + trades, the two references BBO, the perpetual BBO + book +
    // trades, Chainlink index + mark.
    let h = StrategyHarness::<BinaryMaker>::new(&aux, &g3, fixture(), A1).expect("params");
    let room = vec![16u16; 32];
    let lim = SubsLimits {
        rd: &rd,
        feeds: VenueSet::EMPTY.with(VB).with(VR).with(VR2).with(VP).with(VC),
        room: &room,
    };
    let mut sb = SubsBuilder::new(lim, 16);
    h.strategy().subscriptions(&g3, &mut sb);
    let got: Vec<_> = sb.entries().iter().map(|e| (e.inst, e.gix, e.ev)).collect();
    let mut want: Vec<_> = SLOTS
        .iter()
        .enumerate()
        .map(|(k, i)| (*i, k as u16, EvMask(EvMask::BOOK.0 | EvMask::TRADE.0)))
        .collect();
    want.push((R, 6, EvMask::BBO));
    want.push((R2, 7, EvMask::BBO));
    want.push((RP, 8, EvMask(EvMask::BBO.0 | EvMask::BOOK.0 | EvMask::TRADE.0)));
    want.push((RC, 9, EvMask(EvMask::INDEX.0 | EvMask::MARK.0)));
    assert_eq!(got, want, "BMK-051.2");
    let f = &h.strategy().fast;
    assert_eq!(
        (f.reference, f.reference2, f.perp, f.clref),
        (R, R2, RP, RC),
        "BMK-051.2"
    );
    assert_eq!(f.cl_mul, 1_000_000, "BMK-051.2 px_exp −2");
    // .3 decisions with the auxiliary groups and their events equal the BMK-034 traces.
    let base = params(|_| {});
    let fv = params_with(&[
        ("fv_limit", 1),
        ("nochase_ms", 500),
        ("streak_k", 2),
        ("max_window_cost", 30_000_000),
    ]);
    let lag = comp_params(&[
        ("load_gate_ns", 150_000_000),
        ("mdslow_gate_ns", 90_000_000),
        ("comp_imb1_sd", 674_103_869),
    ]);
    for (k, (a, b, seed)) in [(&base, &fv, 1u64), (&lag, &base, 4)].into_iter().enumerate() {
        let plain = decision_trace(a, b, seed);
        let with = decision_trace_with(&with_aux(a), &with_aux(b), seed, true);
        assert_eq!(with, plain, "BMK-051.3 pair {k}");
    }
    // .4 native Chainlink samples equal the sidecar's: same price per second and latest sample;
    // the sidecar's samples are ignored once the native group exists; the TWAP is stored.
    let mut side = harness(&base);
    let mut nat = StrategyHarness::<BinaryMaker>::new(&aux, &g3, fixture(), A1).expect("params");
    for (k, px_e8) in [
        (0i64, 7_480_012_000_000i64),
        (1, 7_480_112_000_000),
        (3, 7_479_900_000_000),
    ] {
        let ts_ms = 1_794_227_700_000 + k * 1_000;
        let mut pl = vec![1u8];
        pl.extend_from_slice(&ts_ms.to_le_bytes());
        pl.extend_from_slice(&px_e8.to_le_bytes());
        side.feed(&ev::warmup(1, k as u32, true, &pl).as_ref());
        nat.feed(&ev::index_at(RC, 9, Px(px_e8 / 1_000_000), ts_ms * 1_000_000).as_ref());
        // A sidecar sample on the native table: ignored.
        let mut junk = vec![1u8];
        junk.extend_from_slice(&(ts_ms + 500).to_le_bytes());
        junk.extend_from_slice(&1i64.to_le_bytes());
        nat.feed(&ev::warmup(1, 9, true, &junk).as_ref());
    }
    let (a, b) = (&side.strategy().fast.cl, &nat.strategy().fast.cl);
    assert_eq!(
        (a.last_s, a.last_px, a.n),
        (b.last_s, b.last_px, b.n),
        "BMK-051.4"
    );
    for s in 1_794_227_700..1_794_227_704 {
        assert_eq!(a.sec.exact(s), b.sec.exact(s), "BMK-051.4 second {s}");
    }
    nat.feed(&ev::mark_at(RC, 9, Px(7_480_050), 1_794_227_700_000_000_000).as_ref());
    let c = &nat.strategy().fast.cl;
    assert_eq!(
        c.twap_at(1_794_227_702),
        Some(7_480_050_000_000),
        "BMK-051.4 TWAP"
    );
    assert_eq!(c.twap_at(1_794_227_699), None, "BMK-051.4 TWAP before");
}

#[test]
fn bmk_035_feature_export_row() {
    use binary_maker::features::COLUMNS;
    use hft_feature::{FeatureRow, NA};
    let mut h = harness(&params(|_| {}));
    warm(&mut h, 400, 399, 1);
    let f = &h.strategy().fast.feat;
    let names: Vec<&str> = COLUMNS.iter().map(|c| c.name).collect();
    assert_eq!(
        names,
        [
            "ref_mid_x2",
            "ref_imb",
            "ref_sigma",
            "ref_samples",
            "lag_minute",
            "lag_p90",
            "lag_p50"
        ],
        "BMK-035"
    );
    let sigma = f.vol.sigma(1).unwrap();
    let mut buf = [7i64; 9];
    let mut row = FeatureRow::new(&mut buf);
    f.export(&mut row);
    assert_eq!(row.len(), 7, "BMK-035");
    assert_eq!(buf, [20_001_000, 0, sigma, 399, NA, NA, NA, 7, 7], "BMK-035");
    // .3 the Strategy hooks (research export) are the same columns and values.
    // (the strategy appends the composite-score research columns, NA without a cached score)
    let feats = <binary_maker::BinaryMaker as hft_strategy_sdk::Strategy>::FEATURES;
    assert_eq!((&feats[..7], feats.len()), (COLUMNS, 33), "BMK-035.3");
    let mut via = [7i64; 9];
    let mut row = FeatureRow::new(&mut via);
    hft_strategy_sdk::Strategy::export_features(h.strategy(), &mut row);
    let n = row.len();
    assert_eq!((&via[..7], n), (&buf[..7], 33), "BMK-035.3");
    let mut short = [0i64; 3];
    let mut row = FeatureRow::new(&mut short);
    f.export(&mut row);
    assert_eq!((row.len(), short), (7, [20_001_000, 0, sigma]), "BMK-035 short");
}

#[test]
fn bmk_036_favoured_side_pairs_without_the_limit() {
    // Long 10 Down at 0.600; the score favours Up (Up bid 190 shares vs Down 10): Down skipped.
    let run = |extra: &[(&str, i64)]| {
        let mut f = vec![("clip_shares", 5), ("inv_max_shares", 10)];
        f.extend_from_slice(extra);
        let mut h = harness(&comp_params(&f));
        hold(&mut h, 3, 10, 600);
        let out = warm_sized(&mut h, 190, 10);
        placed(&out)
    };
    // .1 dir_pair_free 0: the Up bid rests at the pair limit 1 − 0.600 − 0.01 = 0.390.
    assert_eq!(run(&[]), vec![(SLOTS[2], Px(390))], "BMK-036.1");
    // .2 dir_pair_free 1: at the best bid 0.450.
    assert_eq!(
        run(&[("dir_pair_free", 1)]),
        vec![(SLOTS[2], Px(450))],
        "BMK-036.2"
    );
    // .3 comp_dir 0 (no favoured token): the limit applies.
    assert_eq!(
        run(&[("dir_pair_free", 1), ("comp_dir", 0)]),
        vec![(SLOTS[2], Px(390))],
        "BMK-036.3"
    );
}

#[test]
fn bmk_037_tilt_quotes_the_leaning_side_larger() {
    // Balanced books but Up bid 140 vs Down 60 shares: Up leans (0 < z_up), both quoted (|z| < 0.5
    // would still quote both with comp_skip 0.5); with tilt_z 0.1 / tilt_clip 8 the Up order is 8 shares.
    let run = |extra: &[(&str, i64)]| {
        let mut f = vec![
            ("clip_shares", 5),
            ("inv_max_shares", 20),
            ("comp_skip", 5_000_000_000),
            ("comp_dir", 5_000_000_000),
        ];
        f.extend_from_slice(extra);
        let mut h = harness(&comp_params(&f));
        let out = warm_sized(&mut h, 140, 60);
        let mut got: Vec<(InstId, i64)> = out.places().map(|it| (it.inst, it.qty.0 / 1_000_000)).collect();
        got.sort();
        got
    };
    // .1 off: both legs 5 shares.
    assert_eq!(run(&[]), vec![(SLOTS[2], 5), (SLOTS[3], 5)], "BMK-037.1");
    // .2 tilt_z 0.1, tilt_clip_shares 8: the leaning Up leg 8 shares, Down 5.
    assert_eq!(
        run(&[("tilt_z", 100_000_000), ("tilt_clip_shares", 8)]),
        vec![(SLOTS[2], 8), (SLOTS[3], 5)],
        "BMK-037.2"
    );
    // .3 tilt_clip_shares 0: off.
    assert_eq!(
        run(&[("tilt_z", 100_000_000)]),
        vec![(SLOTS[2], 5), (SLOTS[3], 5)],
        "BMK-037.3"
    );
}

#[test]
fn bmk_038_last_minute_stop_loss_pairs_on_the_signal() {
    // Long 10 Down at 0.600; the score favours Up (Up bid 190 shares vs Down 10). stop_before_ms
    // 250 000 puts 399.5 s (200.5 s left) inside the last stretch: no regular quoting.
    let run = |extra: &[(&str, i64)]| {
        let mut f = vec![
            ("clip_shares", 5),
            ("inv_max_shares", 10),
            ("stop_before_ms", 250_000),
        ];
        f.extend_from_slice(extra);
        let mut h = harness(&comp_params(&f));
        hold(&mut h, 3, 10, 600);
        let out = warm_sized(&mut h, 190, 10);
        placed(&out)
    };
    // .1 late_hedge_ms 0: nothing (quoting has stopped).
    assert_eq!(run(&[]), vec![], "BMK-038.1");
    // .2 late_hedge_ms 10 000, late_hedge_z 0.5: Up pairs at the best bid 0.450 (no pair-cost limit 0.390).
    assert_eq!(
        run(&[("late_hedge_ms", 10_000), ("late_hedge_z", 500_000_000)]),
        vec![(SLOTS[2], Px(450))],
        "BMK-038.2"
    );
    // .3 late_hedge_z above the Up score: nothing.
    assert_eq!(
        run(&[("late_hedge_ms", 10_000), ("late_hedge_z", 9_000_000_000)]),
        vec![],
        "BMK-038.3"
    );
    // .4 inside the last late_hedge_ms (left 200.5 s <= 250 000 ms): nothing.
    assert_eq!(
        run(&[("late_hedge_ms", 250_000), ("late_hedge_z", 500_000_000)]),
        vec![],
        "BMK-038.4"
    );
}

#[test]
fn bmk_039_opening_rules() {
    // comp_skip / comp_dir 5 (no skip, no favoured token), clip 5, cap 10; books 0.450 / 0.470.
    let run = |extra: &[(&str, i64)], up: i64, dn: i64, long_dn: i64| {
        let mut f = vec![
            ("clip_shares", 5),
            ("inv_max_shares", 10),
            ("comp_skip", 5_000_000_000),
            ("comp_dir", 5_000_000_000),
        ];
        f.extend_from_slice(extra);
        let mut h = harness(&comp_params(&f));
        if long_dn > 0 {
            hold(&mut h, 3, long_dn, 400);
        }
        let out = warm_sized(&mut h, up, dn);
        let mut got: Vec<InstId> = out.places().map(|it| it.inst).collect();
        got.sort();
        got
    };
    let both = vec![SLOTS[2], SLOTS[3]];
    // .1 rules off: both legs.
    assert_eq!(run(&[], 100, 100, 0), both, "BMK-039.1");
    // .2 open_min_px 0.50 (bids 0.450): neither leg opens; long 5 Down: Up pairs, Down does not open.
    assert_eq!(
        run(&[("open_min_px", 500_000_000)], 100, 100, 0),
        vec![],
        "BMK-039.2"
    );
    assert_eq!(
        run(&[("open_min_px", 500_000_000)], 100, 100, 5),
        vec![SLOTS[2]],
        "BMK-039.2"
    );
    // .3 mm_max_bq 50 with 100-share best bids: neither opens; 150: both open.
    assert_eq!(run(&[("mm_max_bq", 50)], 100, 100, 0), vec![], "BMK-039.3");
    assert_eq!(run(&[("mm_max_bq", 150)], 100, 100, 0), both, "BMK-039.3");
    // .4 early_ms 120 000 (399.5 s is 99.5 s into the window), early_cap 5: flat, both open; long
    // 5 Down: Down may not open (5 + 5 > 5), Up pairs.
    let e = [("early_ms", 120_000), ("early_cap_shares", 5)];
    assert_eq!(run(&e, 100, 100, 0), both, "BMK-039.4");
    assert_eq!(run(&e, 100, 100, 5), vec![SLOTS[2]], "BMK-039.4");
    // .5 open_skip_z 0.5, Up 10 vs Down 190 shares (Up score < -0.5): flat, only Down opens; long 5
    // Down: Up still pairs (comp_skip would not quote it), Down opens up to the cap.
    assert_eq!(
        run(&[("open_skip_z", 500_000_000)], 10, 190, 0),
        vec![SLOTS[3]],
        "BMK-039.5"
    );
    assert_eq!(
        run(&[("open_skip_z", 500_000_000)], 10, 190, 5),
        both,
        "BMK-039.5"
    );
}

#[test]
fn bmk_040_absorption_pairs_the_excess() {
    // The score favours Down (Up bid 10 vs Down 190 shares; comp_skip = comp_dir = 0.5): Up is
    // skipped. Long 15 Down at 0.400, cap 10, dir_cap 20, clip 5.
    let run = |extra: &[(&str, i64)]| {
        let mut f = vec![("clip_shares", 5), ("inv_max_shares", 10), ("dir_cap_shares", 20)];
        f.extend_from_slice(extra);
        let mut h = harness(&comp_params(&f));
        hold(&mut h, 3, 15, 400);
        let out = warm_sized(&mut h, 10, 190);
        placed(&out)
    };
    // .1 absorb off: Up skipped, Down bids (15 + 5 <= 20).
    assert_eq!(run(&[]), vec![(SLOTS[3], Px(450))], "BMK-040.1");
    // .2 absorb 10: 15 - 10 = 5 Down shares are the maker's: Up bids to pair them, at the pair
    // limit min(0.450, 1 - 0.400 - 0.01 = 0.590) = 0.450; Down still bids.
    assert_eq!(
        run(&[("absorb_shares", 10)]),
        vec![(SLOTS[2], Px(450)), (SLOTS[3], Px(450))],
        "BMK-040.2"
    );
    // .3 absorb 15: no excess, Up skipped.
    assert_eq!(
        run(&[("absorb_shares", 15)]),
        vec![(SLOTS[3], Px(450))],
        "BMK-040.3"
    );
}

#[test]
fn bmk_041_expensive_side_and_pairing_priority() {
    let run = |extra: &[(&str, i64)], long_dn: i64| {
        let mut f = vec![
            ("clip_shares", 5),
            ("inv_max_shares", 10),
            ("comp_skip", 5_000_000_000),
            ("comp_dir", 5_000_000_000),
        ];
        f.extend_from_slice(extra);
        let mut h = harness(&comp_params(&f));
        if long_dn > 0 {
            hold(&mut h, 3, long_dn, 400);
        }
        let out = warm_sized(&mut h, 100, 100);
        let mut got: Vec<InstId> = out.places().map(|it| it.inst).collect();
        got.sort();
        got
    };
    // .1 open_max_px 0.45 (bids 0.450): neither opens; long 5 Down: Up pairs.
    assert_eq!(run(&[("open_max_px", 450_000_000)], 0), vec![], "BMK-041.1");
    assert_eq!(
        run(&[("open_max_px", 450_000_000)], 5),
        vec![SLOTS[2]],
        "BMK-041.1"
    );
    // .2 stop_before_ms 250 000 puts 399.5 s inside the last stretch: no quoting; long 5 Down with
    // pair_stop_ms 10 000: Up pairs (Down does not open).
    assert_eq!(run(&[("stop_before_ms", 250_000)], 5), vec![], "BMK-041.2");
    assert_eq!(
        run(&[("stop_before_ms", 250_000), ("pair_stop_ms", 10_000)], 5),
        vec![SLOTS[2]],
        "BMK-041.2"
    );
    // .3 the volatility gate closed (sigma_min huge): pairing still bids; flat: nothing.
    let g = [("sigma_min", 1_000_000_000), ("pair_stop_ms", 10_000)];
    assert_eq!(run(&g, 5), vec![SLOTS[2]], "BMK-041.3");
    assert_eq!(run(&g, 0), vec![], "BMK-041.3");
}

#[test]
fn bmk_042_pairing_phases() {
    // 399.5 s = 99.5 s into the window; long Down at 0.400; comp_skip / comp_dir 5 (no direction).
    let run = |extra: &[(&str, i64)], long_dn: i64| {
        let mut f = vec![
            ("clip_shares", 5),
            ("inv_max_shares", 10),
            ("comp_skip", 5_000_000_000),
            ("comp_dir", 5_000_000_000),
        ];
        f.extend_from_slice(extra);
        let mut h = harness(&comp_params(&f));
        hold(&mut h, 3, long_dn, 400);
        let out = warm_sized(&mut h, 100, 100);
        placed(&out)
    };
    // .1 pairing priority inside stop_before (250 000 ms): only while younger than pair_prio_age_ms.
    let p = [("stop_before_ms", 250_000), ("pair_stop_ms", 10_000)];
    assert_eq!(
        run(&[p[0], p[1], ("pair_prio_age_ms", 120_000)], 5),
        vec![(SLOTS[2], Px(450))],
        "BMK-042.1"
    );
    assert_eq!(
        run(&[p[0], p[1], ("pair_prio_age_ms", 60_000)], 5),
        vec![],
        "BMK-042.1"
    );
    // .2 long 10 Down: the Up pairing bid at min(0.450, 1 - 0.400 - 0) = 0.450; from pair_late_ms
    // 60 000 with pair_late_margin 0.2 at 1 - 0.4 - 0.2 = 0.400; margin 1: no pairing bid.
    assert_eq!(run(&[], 10), vec![(SLOTS[2], Px(450))], "BMK-042.2");
    assert_eq!(
        run(&[("pair_late_ms", 60_000), ("pair_late_margin", 200_000_000)], 10),
        vec![(SLOTS[2], Px(400))],
        "BMK-042.2"
    );
    assert_eq!(
        run(
            &[("pair_late_ms", 60_000), ("pair_late_margin", 1_000_000_000)],
            10
        ),
        vec![],
        "BMK-042.2"
    );
    // .3 before pair_late_ms (120 000): the normal limit.
    assert_eq!(
        run(
            &[("pair_late_ms", 120_000), ("pair_late_margin", 1_000_000_000)],
            10
        ),
        vec![(SLOTS[2], Px(450))],
        "BMK-042.3"
    );
}

#[test]
fn bmk_043_late_favoured_pairing() {
    // Long 10 Down at 0.600; the score favours Up (Up bid 190 vs Down 10), dir_pair_free: Up pairs at
    // the best bid 0.450 without the limit 0.390 (BMK-036). 399.5 s is 99.5 s into the window.
    let run = |extra: &[(&str, i64)]| {
        let mut f = vec![("clip_shares", 5), ("inv_max_shares", 10), ("dir_pair_free", 1)];
        f.extend_from_slice(extra);
        let mut h = harness(&comp_params(&f));
        hold(&mut h, 3, 10, 600);
        let out = warm_sized(&mut h, 190, 10);
        placed(&out)
    };
    let late = [("pair_late_ms", 60_000), ("pair_late_margin", 1_000_000_000)];
    // .1 pair_late without pair_late_all: the favoured Up still pairs at 0.450.
    assert_eq!(run(&late), vec![(SLOTS[2], Px(450))], "BMK-043.1");
    // .2 pair_late_all: no late pairing at all.
    assert_eq!(
        run(&[late[0], late[1], ("pair_late_all", 1)]),
        vec![],
        "BMK-043.2"
    );
}

/// Reference mid (raw 0.01) of second `s` in the rule 12 fixture: alternating 100 000.00 /
/// 100 010.00 (σ ≈ 1 bp) before 540 s, 100 100.00 from 540 s (the last minute of window 300).
fn settle_mid(s: i64) -> i64 {
    if s >= 540 {
        10_010_000
    } else if s % 2 == 0 {
        10_000_000
    } else {
        10_001_000
    }
}

/// Reference quote `mid` at second `s` + 0.1 s.
fn settle_tick(h: &mut StrategyHarness<BinaryMaker>, s: i64, mid: i64) -> hft_testkit::sdk_mock::CtxCapture {
    ref_at(h, s * 1_000_000_000 + 100_000_000, mid)
}

/// Places of one capture: (second, inst, px, tag).
type SettlePlaces = Vec<(i64, InstId, i64, u32)>;

/// Rule 12 harness (main rules off by `sigma_min`): settle_ms 60 s and `extra`; Up book of pair 1
/// `ub` / `ua`, Down the complement; reference ticks from 100 s to `to_s` (exclusive). Returns the
/// harness and every place.
fn settle_h(
    extra: &[(&str, i64)],
    ub: i64,
    ua: i64,
    to_s: i64,
) -> (StrategyHarness<BinaryMaker>, SettlePlaces) {
    let mut f = vec![("settle_ms", 60_000), ("sigma_min", 1_000_000_000)];
    f.extend_from_slice(extra);
    let mut h = harness(&params_with(&f));
    slot_book(h.views_mut(), SLOTS[2], ub, ua);
    slot_book(h.views_mut(), SLOTS[3], 1_000 - ua, 1_000 - ub);
    let mut got = Vec::new();
    for s in 100..to_s {
        let out = settle_tick(&mut h, s, settle_mid(s));
        for it in out.places() {
            got.push((s, it.inst, it.px.0, it.tag));
        }
    }
    (h, got)
}

/// Φ(z) by Abramowitz–Stegun 7.1.26 (error below 1.5e-7).
fn phi_f64(z: f64) -> f64 {
    let x = z.abs() / std::f64::consts::SQRT_2;
    let t = 1.0 / (1.0 + 0.327_591_1 * x);
    let poly = t
        * (0.254_829_592
            + t * (-0.284_496_736 + t * (1.421_413_741 + t * (-1.453_152_027 + t * 1.061_405_429))));
    let erf = 1.0 - poly * (-x * x).exp();
    if z >= 0.0 {
        0.5 * (1.0 + erf)
    } else {
        0.5 * (1.0 - erf)
    }
}

#[test]
fn bmk_044_settlement_window_liquidity() {
    use binary_maker::fast::{QState, settle_tag_of};
    use hft_feature::{FeatureRow, NA};
    let tag = settle_tag_of(2);
    // .1 p_up against the float formula of oct_lock.p_up_bn (same σ) at 555.1 s: n_lock 15, n_fut
    // 45, K = mean(R[240..300)), locked = mean(R[540..555)), R_now = R(555).
    let (h, _) = settle_h(&[("settle_p_min", 1_000_000_000)], 950, 960, 556);
    let f = &h.strategy().fast;
    let [p, mu, n] = f.settle.out;
    let r = |s: i64| 2.0 * settle_mid(s) as f64;
    let k = (240..300).map(r).sum::<f64>() / 60.0;
    let lock = (540..555).map(r).sum::<f64>() / 15.0;
    let mu_f = (15.0 * lock + 45.0 * r(555)) / 60.0 / k - 1.0;
    let var = (1..=45i64)
        .flat_map(|i| (1..=45i64).map(move |j| i.min(j) as f64))
        .sum::<f64>()
        / (45.0 * 45.0)
        * (45.0f64 / 60.0).powi(2);
    let sig = f.feat.vol.sigma(60).unwrap() as f64 / 1e9;
    let p_f = phi_f64(mu_f / (sig * 1.3 * var.sqrt()));
    assert_eq!(n, 15, "BMK-044.1");
    assert!((mu as f64 / 1e9 - mu_f).abs() < 2e-9, "BMK-044.1 mu {mu} {mu_f}");
    // The Φ table interpolates linearly in steps of 1/64 (error below 1e-5).
    assert!((p as f64 / 1e9 - p_f).abs() < 1e-5, "BMK-044.1 p {p} {p_f}");
    assert!(p_f > 0.95, "BMK-044.1 {p_f}");
    // The export carries the same values after the existing columns.
    let mut buf = [0i64; 33];
    let mut row = FeatureRow::new(&mut buf);
    hft_strategy_sdk::Strategy::export_features(h.strategy(), &mut row);
    assert_eq!(&buf[30..], &[p, mu, n], "BMK-044.1 export");
    // Before the last minute nothing is computed.
    let (h, _) = settle_h(&[], 950, 960, 530);
    assert_eq!(h.strategy().fast.settle.out, [NA; 3], "BMK-044.1 NA");
    // .2 in band (bid 0.950): the first bid at 545 s (5 s locked), Up at the best bid, 20 shares,
    // the rule 12 tag; nothing before the last minute or before settle_min_lock_s.
    let (_, got) = settle_h(&[], 950, 960, 546);
    assert_eq!(got, vec![(545, SLOTS[2], 950, tag)], "BMK-044.2");
    // Out of band (0.980 above settle_bid_hi, 0.850 below settle_bid_lo): no bid.
    assert_eq!(settle_h(&[], 980, 990, 560).1, vec![], "BMK-044.2 high");
    assert_eq!(settle_h(&[], 850, 860, 560).1, vec![], "BMK-044.2 low");
    // settle_improve with a 3 c spread: one cent above the best bid.
    let (_, got) = settle_h(&[("settle_improve", 1)], 940, 970, 546);
    assert_eq!(got, vec![(545, SLOTS[2], 950, tag)], "BMK-044.2 improve");
    // settle_ms 0: off.
    assert_eq!(
        settle_h(&[("settle_ms", 0)], 950, 960, 560).1,
        vec![],
        "BMK-044.2 off"
    );
    // .3 live bid (OMS slot 21); the reference falls back to 100 000.00 at 550 s: p_Up < 0.90 →
    // cancel.
    let live = |h: &mut StrategyHarness<BinaryMaker>| {
        let q = &mut h.strategy_mut().fast.quotes[2];
        q.state = QState::Live;
        q.r = Some(order_ref(21));
    };
    let (mut h, _) = settle_h(&[], 950, 960, 546);
    live(&mut h);
    let out = settle_tick(&mut h, 546, settle_mid(546));
    assert!(out.items.is_empty(), "BMK-044.3 stays {:?}", out.items);
    let out = settle_tick(&mut h, 550, 10_000_000);
    assert!(h.strategy().fast.settle.out[0] < 900_000_000, "BMK-044.3");
    assert_eq!(cancels(&out), vec![21], "BMK-044.3");
    // .4 undercut (best bid 0.955 above ours): cancel; after the cancel, a new bid at 0.955 only
    // settle_requote_ms after the cancel. A cleared level (bid 0.940) keeps the bid.
    let (mut h, _) = settle_h(&[], 950, 960, 546);
    live(&mut h);
    slot_book(h.views_mut(), SLOTS[2], 940, 960);
    h.views_mut().at_engine_ns(546_200_000_000);
    let out = h.feed(&ev::book(SLOTS[2], 2).as_ref());
    assert!(out.items.is_empty(), "BMK-044.4 cleared {:?}", out.items);
    slot_book(h.views_mut(), SLOTS[2], 955, 960);
    h.views_mut().at_engine_ns(546_300_000_000);
    let out = h.feed(&ev::book(SLOTS[2], 2).as_ref());
    assert_eq!(cancels(&out), vec![21], "BMK-044.4");
    let out = update_at(&mut h, 546_310_000_000, 2, 21, OrderUpdateKind::Cancelled);
    assert!(out.places().next().is_none(), "BMK-044.4 requote_ms");
    let out = ref_at(&mut h, 546_450_000_000, settle_mid(546));
    assert!(out.places().next().is_none(), "BMK-044.4 requote_ms");
    let out = ref_at(&mut h, 546_550_000_000, settle_mid(546));
    let got: Vec<(InstId, Px, u32)> = out.places().map(|it| (it.inst, it.px, it.tag)).collect();
    assert_eq!(got, vec![(SLOTS[2], Px(955), tag)], "BMK-044.4");
    // .5 settle_max_shares: 20 shares filled; with the cap 20 no further bid, with 60 one more.
    for (max, want) in [(20, 0usize), (60, 1)] {
        let (mut h, _) = settle_h(&[("settle_max_shares", max)], 950, 960, 546);
        live(&mut h);
        update_at(
            &mut h,
            546_200_000_000,
            2,
            21,
            filled(20_000_000, 20_000_000, true),
        );
        assert_eq!(h.strategy().fast.settle.bought, 20_000_000, "BMK-044.5");
        let out = settle_tick(&mut h, 547, settle_mid(547));
        assert_eq!(out.places().count(), want, "BMK-044.5 max {max}");
    }
    // .6 the main rules quoting (default sigma_min, stop_before_ms 30 s; bids outside [p_min, p_max]
    // so they have no target): the module waits for the last 30 s.
    let (_, got) = settle_h(
        &[("sigma_min", 50_000), ("stop_before_ms", 30_000)],
        950,
        960,
        571,
    );
    assert_eq!(got, vec![(570, SLOTS[2], 950, tag)], "BMK-044.6");
}

#[test]
fn bmk_045_stop_loss_pairing() {
    // Long 10 Down at 0.600, both books bid 0.450 / ask 0.470, the score favours Up (Up bid 190
    // shares vs Down 10); sigma_min closes the main rules. 399.5 s = age 99.5 s of window 300.
    let run = |extra: &[(&str, i64)]| {
        let mut f = vec![
            ("clip_shares", 5),
            ("inv_max_shares", 10),
            ("sigma_min", 1_000_000_000),
        ];
        f.extend_from_slice(extra);
        let mut h = harness(&comp_params(&f));
        hold(&mut h, 3, 10, 600);
        let out = warm_sized(&mut h, 190, 10);
        (placed(&out), h.strategy().fast.rescued)
    };
    let on = [
        ("rescue_ms", 90_000),
        ("rescue_z", 500_000_000),
        ("rescue_px", 500_000_000),
    ];
    // .1 rescue_ms 0: nothing.
    assert_eq!(run(&[]), (vec![], i64::MIN), "BMK-045.1");
    // .2 on: Up pairs at the best bid 0.450 without the pair-cost limit (0.390); the window is marked.
    assert_eq!(run(&on), (vec![(SLOTS[2], Px(450))], 300), "BMK-045.2");
    // .3 the held Down bid 0.450 above rescue_px 0.40, the age below rescue_ms, the score below
    // rescue_z: nothing.
    for (row, k, v) in [
        ("BMK-045.3 px", 2, 400_000_000),
        ("BMK-045.3 age", 0, 120_000),
        ("BMK-045.3 z", 1, 9_000_000_000),
    ] {
        let mut f = on.to_vec();
        f[k].1 = v;
        assert_eq!(run(&f).0, vec![], "{row}");
    }
    // .4 rescue_cap 0.02: at most 1 - 0.600 + 0.02 = 0.420.
    let mut f = on.to_vec();
    f.push(("rescue_cap", 20_000_000));
    assert_eq!(run(&f).0, vec![(SLOTS[2], Px(420))], "BMK-045.4");
}

/// Chainlink sample payload v1 for UTC second `s` + 0.5 s, price `px` (1e-8 USD).
fn cl_payload(s: i64, px: i64) -> Vec<u8> {
    let mut b = vec![1u8];
    b.extend_from_slice(&(s * 1_000 + 500).to_le_bytes());
    b.extend_from_slice(&px.to_le_bytes());
    b
}

/// `settle_h` with Chainlink samples `cl(s)` (1e-8 USD; `None` = no sample) before each second's
/// reference tick.
fn settle_hc(
    extra: &[(&str, i64)],
    ub: i64,
    ua: i64,
    to_s: i64,
    cl: impl Fn(i64) -> Option<i64>,
) -> (StrategyHarness<BinaryMaker>, SettlePlaces) {
    let mut f = vec![("settle_ms", 60_000), ("sigma_min", 1_000_000_000)];
    f.extend_from_slice(extra);
    let mut h = harness(&params_with(&f));
    slot_book(h.views_mut(), SLOTS[2], ub, ua);
    slot_book(h.views_mut(), SLOTS[3], 1_000 - ua, 1_000 - ub);
    let mut got = Vec::new();
    for s in 100..to_s {
        if let Some(px) = cl(s) {
            let b = cl_payload(s, px);
            h.views_mut().at_engine_ns(s * 1_000_000_000 + 50_000_000);
            h.feed(&EventRef::Warmup {
                local_kind: 1,
                chunk_no: 0,
                last: true,
                payload: &b,
            });
        }
        let out = settle_tick(&mut h, s, settle_mid(s));
        for it in out.places() {
            got.push((s, it.inst, it.px.0, it.tag));
        }
    }
    (h, got)
}

#[test]
fn bmk_046_settlement_on_chainlink() {
    use binary_maker::fast::{ClFeed, settle_tag_of};
    let tag = settle_tag_of(2);
    // Chainlink = the reference mid (raw 0.01 USD) in 1e-8 USD.
    let same = |s: i64| Some(settle_mid(s) * 1_000_000);
    // .1 settle_src 1 on the same path: the same p, μ and the same bid as the reference.
    let (hb, gb) = settle_h(&[], 950, 960, 546);
    let (hc, gc) = settle_hc(&[("settle_src", 1)], 950, 960, 546, same);
    assert_eq!(
        hc.strategy().fast.settle.out,
        hb.strategy().fast.settle.out,
        "BMK-046.1"
    );
    assert_eq!(gc, gb, "BMK-046.1");
    assert_eq!(gc, vec![(545, SLOTS[2], 950, tag)], "BMK-046.1");
    // .2 no Chainlink samples, or none after 541 s (stale at 545 s): no bid; the reference source
    // ignores the samples.
    assert_eq!(
        settle_hc(&[("settle_src", 1)], 950, 960, 560, |_| None).1,
        vec![],
        "BMK-046.2"
    );
    let stale = |s: i64| (s <= 541).then(|| settle_mid(s) * 1_000_000);
    assert_eq!(
        settle_hc(&[("settle_src", 1)], 950, 960, 546, stale).1,
        vec![],
        "BMK-046.2 stale"
    );
    assert_eq!(
        settle_hc(&[], 950, 960, 546, |_| None).1,
        gb,
        "BMK-046.2 reference"
    );
    // .3 Chainlink stays at the strike while the reference moves: μ ~ 0, no bid.
    let flat = |s: i64| {
        Some(if s >= 540 {
            10_000_500 * 1_000_000
        } else {
            settle_mid(s) * 1_000_000
        })
    };
    assert_eq!(
        settle_hc(&[("settle_src", 1)], 950, 960, 560, flat).1,
        vec![],
        "BMK-046.3"
    );
    // .4 settle_lead_usd: the projected average 100 100.00 leads the strike 100 005.00 by 95 USD.
    for (lead, want) in [(90, 1usize), (100, 0)] {
        let got = settle_hc(
            &[("settle_src", 1), ("settle_lead_usd", lead)],
            950,
            960,
            546,
            same,
        )
        .1;
        assert_eq!(got.len(), want, "BMK-046.4 lead {lead}");
        let got = settle_h(&[("settle_lead_usd", lead)], 950, 960, 546).1;
        assert_eq!(got.len(), want, "BMK-046.4 reference lead {lead}");
    }
    // .5 settle_edge: a bid only at most at p - edge.
    let p = hb.strategy().fast.settle.out[0];
    for (edge, want) in [
        (p - 950_000_000 - 1_000_000, 1usize),
        (p - 950_000_000 + 1_000_000, 0),
    ] {
        assert_eq!(
            settle_h(&[("settle_edge", edge)], 950, 960, 546).1.len(),
            want,
            "BMK-046.5 {edge}"
        );
    }
    // .6 payload decoding: version, length, price sign; an older sample fills only its second.
    let mut c = ClFeed {
        last_s: i64::MIN,
        ..ClFeed::default()
    };
    c.on_payload(&cl_payload(100, 7));
    c.on_payload(&cl_payload(98, 9));
    let mut bad = cl_payload(101, 5);
    bad[0] = 2;
    c.on_payload(&bad);
    c.on_payload(&cl_payload(102, 5)[..16]);
    c.on_payload(&cl_payload(103, -5));
    assert_eq!((c.n, c.last_s, c.last_px), (2, 100, 7), "BMK-046.6");
    assert_eq!(
        (c.sec.exact(98), c.sec.exact(100), c.sec.exact(101)),
        (Some(9), Some(7), None),
        "BMK-046.6"
    );
    assert_eq!((c.now(103), c.now(104)), (Some(7), None), "BMK-046.6 fresh");
}

/// Chop seed payload v1 of (window start, efficiency ratio) pairs.
fn chop_payload(e: &[(i64, i64)]) -> Vec<u8> {
    let mut b = vec![1u8, e.len() as u8];
    for (w, er) in e {
        b.extend_from_slice(&w.to_le_bytes());
        b.extend_from_slice(&er.to_le_bytes());
    }
    b
}

#[test]
fn bmk_047_chop_seed() {
    use binary_maker::fast::Chop;
    let e: Vec<(i64, i64)> = (0..12)
        .map(|k| (1_000 + 300 * k, 100_000_000 + k * 10_000_000))
        .collect();
    // .1 twelve windows into an empty ring: mean of the last 12 = 0.155, of the last 6 = 0.185.
    let mut c = Chop::default();
    c.seed(&chop_payload(&e));
    assert_eq!((c.len, c.pos), (12, 12), "BMK-047.1");
    assert_eq!(
        (c.mean(12), c.mean(6)),
        (Some(155_000_000), Some(185_000_000)),
        "BMK-047.1"
    );
    // .2 a ring with entries ignores a seed; a bad version is ignored.
    let mut c2 = c;
    c2.seed(&chop_payload(&[(9_000, 0)]));
    assert_eq!(c2.ring, c.ring, "BMK-047.2");
    let mut bad = chop_payload(&e);
    bad[0] = 2;
    let mut c3 = Chop::default();
    c3.seed(&bad);
    assert_eq!(c3.len, 0, "BMK-047.2 version");
    // .3 skipped: out of range, out of order, at or after the window being accumulated.
    let mut c4 = Chop {
        win: 2_200,
        ..Chop::default()
    };
    c4.seed(&chop_payload(&[
        (1_000, 2_000_000_000),
        (1_300, 300_000_000),
        (1_200, 100_000_000),
        (1_600, 200_000_000),
        (2_200, 400_000_000),
    ]));
    assert_eq!(
        (c4.len, c4.ring[0], c4.ring[1]),
        (2, 300_000_000, 200_000_000),
        "BMK-047.3"
    );
    // .4 thirty windows: the last 24 kept, the newest last.
    let e30: Vec<(i64, i64)> = (0..30).map(|k| (300 * k, k * 1_000_000)).collect();
    let mut c5 = Chop::default();
    c5.seed(&chop_payload(&e30));
    assert_eq!(
        (c5.len, c5.pos, c5.ring[0], c5.ring[23]),
        (24, 0, 6_000_000, 29_000_000),
        "BMK-047.4"
    );
    assert_eq!(c5.mean(1), Some(29_000_000), "BMK-047.4");
    // .5 through the strategy: InjectWarmup local kind 2 fills the chop ring.
    let mut h = harness(&params_with(&[("chop_k", 12)]));
    let b = chop_payload(&e);
    h.views_mut().at_engine_ns(5_000_000_000);
    h.feed(&EventRef::Warmup {
        local_kind: 2,
        chunk_no: 0,
        last: true,
        payload: &b,
    });
    assert_eq!(h.strategy().fast.chop.mean(12), Some(155_000_000), "BMK-047.5");
}

/// Reference warm-up (392 s, empty slot books), then pair 1's books once a second at 392..=398 s with the Up bid
/// sizes `up_bids` (Up ask 0.470 × 100, Down 0.450 × 100 / 0.470 × 100), each followed by a `Book`
/// event of Up, and a last reference quote at 399.5 s; the places from 398 s on (the order-flow
/// ring reaches 5 s back).
fn ofi_run(extra: &[(&str, i64)], up_bids: &[i64]) -> Vec<(InstId, Px)> {
    let mut h = harness(&params_with(extra));
    let mut got = Vec::new();
    // No slot books during the warm-up.
    h.views_mut().book(SLOTS[2], &[], &[]);
    h.views_mut().book(SLOTS[3], &[], &[]);
    for k in 0..392i64 {
        h.views_mut().at_engine_ns(k * 1_000_000_000);
        ref_quote(h.views_mut(), if k % 2 == 0 { 10_000_000 } else { 10_001_000 });
        let b = h.views_mut().bbo_of(R);
        h.feed(&ev::bbo(R, 6, b).as_ref());
    }
    for (k, q) in up_bids.iter().enumerate() {
        h.views_mut().at_engine_ns((392 + k as i64) * 1_000_000_000);
        h.views_mut().book(SLOTS[2], &[lv(450, *q)], &[lv(470, 100)]);
        h.views_mut().book(SLOTS[3], &[lv(450, 100)], &[lv(470, 100)]);
        let out = h.feed(&ev::book(SLOTS[2], 2).as_ref());
        // From 398 s the order-flow ring reaches 5 s back.
        if k >= 6 {
            got.extend(placed(&out));
        }
    }
    h.views_mut().at_engine_ns(399_500_000_000);
    ref_quote(h.views_mut(), 10_000_500);
    let b = h.views_mut().bbo_of(R);
    got.extend(placed(&h.feed(&ev::bbo(R, 6, b).as_ref())));
    got
}

#[test]
fn bmk_048_pressure_block() {
    use binary_maker::fast::OfiRing;
    let s = 1_000_000_000i64;
    // .1 Cont-Kukanov-Stoikov increments: bid 100 → 150 at the same price +50; ask 0.470 × 100 →
    // 0.460 × 80 −80.
    let mut o = OfiRing::default();
    o.on_book(SLOTS[2], Some((450, 100)), Some((470, 100)), 0);
    o.on_book(SLOTS[2], Some((450, 150)), Some((470, 100)), 50_000_000);
    assert_eq!(o.cum, 50, "BMK-048.1");
    o.on_book(SLOTS[2], Some((450, 150)), Some((460, 80)), 1_200_000_000);
    assert_eq!(o.cum, -30, "BMK-048.1");
    // .2 over the last 1 s / 0.5 s at 1.3 s: since the end of bucket 3 / 8 (value 50) = −80; 5 s
    // reaches before the first bucket; nothing since the last bucket = 0.
    assert_eq!(o.over(1_300_000_000, 1_000), Some(-80), "BMK-048.2");
    assert_eq!(o.over(1_300_000_000, 500), Some(-80), "BMK-048.2");
    assert_eq!(o.over(1_300_000_000, 5_000), None, "BMK-048.2");
    assert_eq!(o.over(1_250_000_000, 0), Some(0), "BMK-048.2");
    // .3 a new instrument restarts the ring.
    o.on_book(SLOTS[3], Some((450, 100)), Some((470, 100)), 2 * s);
    assert_eq!((o.inst, o.cum, o.first), (SLOTS[3], 0, 20), "BMK-048.3");
    // .4 Up bid 10 against ask 100 (imbalance −0.82) with flat order flow: Up opens nothing,
    // Down (imbalance 0) quotes; off (ofi_win_ms 0) both quote.
    let on = [("ofi_win_ms", 5_000), ("ofi_obi_max", -300_000_000)];
    let flat = [10, 10, 10, 10, 10, 10, 10];
    let has = |v: &[(InstId, Px)], i| v.iter().any(|(x, _)| *x == i);
    let got = ofi_run(&on, &flat);
    assert!(!has(&got, SLOTS[2]) && has(&got, SLOTS[3]), "BMK-048.4 {got:?}");
    let got = ofi_run(&[], &flat);
    assert!(
        has(&got, SLOTS[2]) && has(&got, SLOTS[3]),
        "BMK-048.4 off {got:?}"
    );
    // .5 the Up bid grows 10 → 30 at 397 s (order flow +20 within 5 s, imbalance still −0.54):
    // Up quotes.
    let got = ofi_run(&on, &[10, 10, 10, 10, 10, 30, 30]);
    assert!(has(&got, SLOTS[2]), "BMK-048.5 {got:?}");
    // .6 the imbalance above ofi_obi_max (bid 100 against ask 100) with flat flow: Up quotes.
    let got = ofi_run(&on, &[100, 100, 100, 100, 100, 100, 100]);
    assert!(has(&got, SLOTS[2]), "BMK-048.6 {got:?}");
}

#[test]
fn bmk_049_second_reference_group() {
    let p = params(|_| {});
    let rd = fixture();
    let check = |specs: &[GroupSpec]| p.validate(&GroupTable::new(specs, CAPACITY), &rd.view());
    // .1 six slots + two references validate; .2 two references and an odd slot count do not.
    let all = specs2();
    assert_eq!(check(&all), Ok(()), "BMK-049.1");
    assert_eq!(
        check(&all[1..]),
        Err(ParamError::GroupShape { gix: 0, code: 1 }),
        "BMK-049.2 odd slots"
    );
    let mut three = all.clone();
    three.push(spec(gid(8), BTC, &[(R, VR)]));
    three.remove(0);
    assert_eq!(
        check(&three[1..]),
        Err(ParamError::GroupShape { gix: 0, code: 1 }),
        "BMK-049.2 three references"
    );
    // .3 the first trailing reference is the fair-value reference, the second one feeds the guard.
    let g = GroupTable::new(&all, CAPACITY);
    let mut h = StrategyHarness::<BinaryMaker>::new(&p, &g, fixture(), A1).expect("params");
    assert_eq!(
        (h.strategy().fast.reference, h.strategy().fast.reference2),
        (R, R2),
        "BMK-049.3"
    );
    // .4 a native BBO on R2 is one guard sample (mid × 2 in R2's price units).
    h.views_mut().at_engine_ns(1_000_000_000);
    h.views_mut().bbo(
        R2,
        (Px(9_999_999), Qty(1_000_000)),
        (Px(10_000_001), Qty(1_000_000)),
    );
    let b = h.views_mut().bbo_of(R2);
    h.feed(&ev::bbo(R2, 7, b).as_ref());
    let cb = &h.strategy().fast.cb;
    assert_eq!((cb.n, cb.at(1_000_000_000)), (1, Some(20_000_000)), "BMK-049.4");
    // .5 with a native reference the sidecar's samples (1e-8 USD) are ignored.
    let mut payload = vec![1u8, 1, 0, 1];
    // One kind-1 sample: receive ns, exchange ns, bid, ask (1e-8 USD), unused.
    for x in [1_000_000_000i64, 0, 10_000_000_000_000, 10_000_100_000_000, 0] {
        payload.extend_from_slice(&x.to_le_bytes());
    }
    h.feed(&ev::warmup(3, 0, true, &payload).as_ref());
    assert_eq!(h.strategy().fast.cb.n, 1, "BMK-049.5");
    // .6 the one-reference table keeps reference2 unset and takes the sidecar's samples.
    let mut h1 = harness(&p);
    assert_eq!(h1.strategy().fast.reference2, InstId(0), "BMK-049.6");
    h1.views_mut().at_engine_ns(1_000_000_000);
    h1.feed(&ev::warmup(3, 0, true, &payload).as_ref());
    assert_eq!(h1.strategy().fast.cb.n, 1, "BMK-049.6");
}

#[test]
fn bmk_050_reference_fusion() {
    use binary_maker::fast::RefFuse;
    // .1 off (w = 0) or no fresh second reference: not ready (0), the first reference alone.
    let mut f = RefFuse::default();
    assert_eq!(f.on_first(20_000_000, 1_000_000_000, 500_000_000), 0, "BMK-050.1");
    f.on_second(19_980_000, 1_000_000_000, 20_000_000, 0);
    assert_eq!(f.on_first(20_000_000, 1_100_000_000, 0), 0, "BMK-050.1 w = 0");
    // .2 w = 0.5: the first quote sets the ratio (1.001…), the fused mid equals the first's.
    let fused = f.on_first(20_000_000, 1_200_000_000, 500_000_000);
    assert!((fused - 20_000_000).abs() <= 1, "BMK-050.2 {fused}");
    // .3 the second reference moves +10 bp: the fused mid moves +5 bp before the first moves.
    f.on_second(19_999_980, 1_300_000_000, 20_000_000, 500_000_000);
    assert!((f.mid_x2 - 20_010_000).abs() <= 2, "BMK-050.3 {}", f.mid_x2);
    // .4 the second reference stale (> 5 s): not ready again.
    assert_eq!(f.on_first(20_000_000, 7_000_000_000, 500_000_000), 0, "BMK-050.4");
    // .5 in the strategy: ref_cb_w on the two-reference table, the Coinbase BBO moves the reference
    // mid the fair value reads; off, it stays the Binance mid.
    let ix = |name: &str| {
        let s = BinaryMaker::DESC.schema;
        (0..s.n_fields())
            .position(|i| s.field(i).is_some_and(|x| x.name == name))
            .unwrap()
    };
    for (w, want_moved) in [(500_000_000i64, true), (0, false)] {
        let p = params(|d| d[ix("ref_cb_w")] = w);
        let g = GroupTable::new(&specs2(), CAPACITY);
        let mut h = StrategyHarness::<BinaryMaker>::new(&p, &g, fixture(), A1).expect("params");
        let cb = |h: &mut StrategyHarness<BinaryMaker>, t: i64, mid: i64| {
            h.views_mut().at_engine_ns(t);
            h.views_mut()
                .bbo(R2, (Px(mid - 1), Qty(1_000_000)), (Px(mid + 1), Qty(1_000_000)));
            let b = h.views_mut().bbo_of(R2);
            h.feed(&ev::bbo(R2, 7, b).as_ref());
        };
        cb(&mut h, 1_000_000_000, 9_990_000);
        h.views_mut().at_engine_ns(1_100_000_000);
        ref_quote(h.views_mut(), 10_000_000);
        let b = h.views_mut().bbo_of(R);
        h.feed(&ev::bbo(R, 6, b).as_ref());
        cb(&mut h, 1_200_000_000, 9_999_990);
        let m = h.strategy().fast.ref_mid_x2();
        assert_eq!(m != 20_000_000, want_moved, "BMK-050.5 w {w}: {m}");
    }
}

/// A numeric field of a flat JSON object line (fixture rows; no JSON crate in this test binary).
fn json_num(line: &str, key: &str) -> f64 {
    let pat = format!("\"{key}\": ");
    let at = line.find(&pat).unwrap_or_else(|| panic!("no {key}")) + pat.len();
    let rest = &line[at..];
    let end = rest.find([',', '}']).unwrap();
    rest[..end].trim().parse().unwrap()
}

#[test]
fn bmk_052_fcev_parity_with_the_python_reference() {
    use binary_maker::fcev::{self, FcState, Model};
    let e9 = |x: f64| (x * 1e9).round() as i64;
    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/binary-maker-fcev/parity_20261109.jsonl");
    let text = std::fs::read_to_string(path).unwrap();
    let m = Model {
        s: 1_250_000_000,
        lam: 350_000_000,
        c0: 1_300_000_000,
        c1: 500_000_000,
        w: 500_000_000,
    };
    let mut st = FcState::default();
    let (mut rows, mut iv_rows, mut iv_len_diff) = (0usize, 0usize, 0usize);
    let (mut max_sq, mut max_dist, mut max_sig_rel, mut max_score) = (0i64, 0i64, 0f64, 0i64);
    for line in text.lines() {
        let f = |k: &str| json_num(line, k);
        let (start, sec, left_ms) = (f("start") as i64, f("t_s") as i64, f("left_ms") as i64);
        let n_real = f("n_real") as i64;
        // .1 time factors.
        let (sq, remf) = fcev::time_factors(left_ms, n_real);
        max_sq = max_sq
            .max((sq - e9(f("sq"))).abs())
            .max((remf - e9(f("remf"))).abs());
        // .2 distance of the final average from the strike.
        let real_sum = f("real_sum_e8") as i128;
        let dist = fcev::dist_up(f("K_e8") as i64, f("s_now_e8") as i64, left_ms, real_sum, n_real).unwrap();
        max_dist = max_dist.max((dist - e9(f("dist_up"))).abs());
        // .3 implied σ: one sample per second, median of the window's samples.
        st.window(start);
        st.push_iv(sec, fcev::implied_sigma(e9(f("mid_up")), dist, sq), 60);
        if st.iv_len() != f("iv_n") as usize {
            iv_len_diff += 1;
        }
        let py_sig = f("sig");
        let sig = if f("iv_n") >= 5.0 {
            iv_rows += 1;
            let s = st.iv_median().unwrap();
            max_sig_rel = max_sig_rel.max(((s as f64) / 1e9 - py_sig).abs() / py_sig);
            s
        } else {
            // The realized-σ fallback differs by design (README "规则" 14): compare the score with
            // the reference's σ.
            e9(py_sig)
        };
        // .4 scores of both tokens at their best bid.
        for (tok, sg) in [("up", 1i64), ("dn", -1)] {
            let s = fcev::score(
                &m,
                sg * dist,
                sq,
                remf,
                sig,
                e9(f(&format!("{tok}_mid"))),
                e9(f(&format!("{tok}_px"))),
            );
            max_score = max_score.max((s - e9(f(&format!("{tok}_score")))).abs());
        }
        rows += 1;
    }
    assert_eq!(rows, 1195, "BMK-052 rows");
    assert!(iv_rows > 1000, "BMK-052 implied-σ rows {iv_rows}");
    assert!(max_sq <= 1_000, "BMK-052.1 sq/remf {max_sq}");
    assert!(max_dist <= 200_000, "BMK-052.2 dist {max_dist} (1e-9 bp)");
    assert!(
        iv_len_diff * 100 <= rows,
        "BMK-052.3 sample count differs on {iv_len_diff} rows"
    );
    assert!(max_sig_rel <= 5e-4, "BMK-052.3 sigma relative {max_sig_rel}");
    assert!(max_score <= 2_000_000, "BMK-052.4 score {max_score} (1e-9 c)");
}

#[test]
fn bmk_053_rule14_decisions() {
    use binary_maker::fast::QState;
    let s = 1_000_000_000i64;
    let fc = |extra: &[(&str, i64)]| {
        let mut f = vec![("ref_aux", 3), ("fc_on", 1)];
        f.extend_from_slice(extra);
        params_with(&f)
    };
    let g3 = GroupTable::new(&specs3(), CAPACITY);
    // Window 600 s uses pair 2 (slots 4, 5): Up 0.450 / 0.470, Down 0.530 / 0.550; reference and
    // Chainlink at 100 100.00, the strike TWAP 100 000.00 (+10 bp: Up favoured).
    let setup = |p: &BinaryMakerParams, twap: bool| {
        let mut h = StrategyHarness::<BinaryMaker>::new(p, &g3, fixture(), A1).expect("params");
        if twap {
            h.feed(&ev::mark_at(RC, 9, Px(10_000_000), 600 * s).as_ref());
        }
        h.views_mut().at_engine_ns(639 * s + s / 2);
        h.views_mut().bbo(
            R,
            (Px(10_009_999), Qty(1_000_000)),
            (Px(10_010_001), Qty(1_000_000)),
        );
        let b = h.views_mut().bbo_of(R);
        h.feed(&ev::bbo(R, 6, b).as_ref());
        h.feed(&ev::index_at(RC, 9, Px(10_010_000), 640 * s).as_ref());
        slot_book(h.views_mut(), SLOTS[4], 450, 470);
        slot_book(h.views_mut(), SLOTS[5], 530, 550);
        h
    };
    let book_ev = |h: &mut StrategyHarness<BinaryMaker>, t: i64| {
        h.views_mut().at_engine_ns(t);
        h.feed(&ev::book(SLOTS[4], 4).as_ref())
    };
    // .1 no strike TWAP: no bid.
    let mut h = setup(&fc(&[]), false);
    assert!(placed(&book_ev(&mut h, 641 * s)).is_empty(), "BMK-053.1");
    // .2 the favoured token is bid at its best bid; the other is not.
    let mut h = setup(&fc(&[]), true);
    let out = book_ev(&mut h, 641 * s);
    assert_eq!(placed(&out), vec![(SLOTS[4], Px(450))], "BMK-053.2");
    // .3 a post margin above the score: nothing.
    let mut h = setup(&fc(&[("fc_m_post", 40_000_000_000)]), true);
    assert!(placed(&book_ev(&mut h, 641 * s)).is_empty(), "BMK-053.3");
    // .4 resting at 0.450: Chainlink falls to −10 bp → the score is below fc_m_cancel → cancel; a
    // better bid than ours (0.460) also cancels.
    for (row, cl, up_bid) in [
        ("BMK-053.4 score", 9_990_000i64, 450i64),
        ("BMK-053.4 undercut", 10_010_000, 460),
    ] {
        let mut h = setup(&fc(&[]), true);
        {
            let q = &mut h.strategy_mut().fast.quotes[4];
            q.state = QState::Live;
            q.r = Some(order_ref(21));
            q.px = 450;
            q.win = 600;
        }
        h.feed(&ev::index_at(RC, 9, Px(cl), 641 * s).as_ref());
        slot_book(h.views_mut(), SLOTS[4], up_bid, 470);
        assert_eq!(cancels(&book_ev(&mut h, 642 * s)), vec![21], "{row}");
    }
    // .5 pairing (fc_m_pair 0.5 c): 10 Up shares at 0.450 → the Down leg is bid only by the pair
    // bid, min(⌊100 − 45 − 0.5⌋ = 54 c, best bid 53 c) for the 10-share excess.
    let mut h = setup(&fc(&[("fc_m_pair", 500_000_000)]), true);
    let mut gp = GroupPosView::EMPTY;
    gp.n_legs = 1;
    gp.legs[0].qty = Qty(10_000_000);
    gp.legs[0].entry_px = Px(450);
    h.views_mut().group_pos(A1, gid(4), gp);
    let out = book_ev(&mut h, 641 * s);
    let places: Vec<_> = out.places().map(|it| (it.inst, it.px, it.qty)).collect();
    assert!(
        places.contains(&(SLOTS[5], Px(530), Qty(10_000_000))),
        "BMK-053.5 {places:?}"
    );
    // .6 off (fc_on 0): rules 1–13 decide (no σ yet: nothing), the model never runs.
    let mut h = setup(&params_with(&[("ref_aux", 3)]), true);
    assert!(placed(&book_ev(&mut h, 641 * s)).is_empty(), "BMK-053.6");
    assert_eq!(h.strategy().fast.fc.iv_sec, i64::MIN, "BMK-053.6");
}

#[test]
fn bmk_054_rule15_lead_quote() {
    use binary_maker::fast::QState;
    use binary_maker::lead::LeadLeg;
    let s = 1_000_000_000i64;
    let ms = 1_000_000i64;
    let g3 = GroupTable::new(&specs3(), CAPACITY);
    let lead = |extra: &[(&str, i64)]| {
        let mut f = vec![
            ("ref_aux", 3),
            ("lead_w_ms", 300),
            ("lead_x_bp", 100_000),
            ("lead_only", 1),
            ("inv_max_shares", 20),
        ];
        f.extend_from_slice(extra);
        params_with(&f)
    };
    // Window 600 s (pair 2: slots 4 Up, 5 Down): Up 0.450 / 0.470, Down 0.530 / 0.550.
    let setup = |p: &BinaryMakerParams| {
        let mut h = StrategyHarness::<BinaryMaker>::new(p, &g3, fixture(), A1).expect("params");
        slot_book(h.views_mut(), SLOTS[4], 450, 470);
        slot_book(h.views_mut(), SLOTS[5], 530, 550);
        h
    };
    // Perpetual BBO with mid `mid` (px_exp −2: 100 000.00 = 10 000 000; 1 bp = 1 000).
    let perp = |h: &mut StrategyHarness<BinaryMaker>, t: i64, mid: i64| {
        h.views_mut().at_engine_ns(t);
        h.views_mut()
            .bbo(RP, (Px(mid - 5), Qty(1_000_000)), (Px(mid + 5), Qty(1_000_000)));
        let b = h.views_mut().bbo_of(RP);
        h.feed(&ev::bbo(RP, 8, b).as_ref())
    };
    let places = |out: &hft_testkit::sdk_mock::CtxCapture| -> Vec<(InstId, Px, Qty)> {
        out.places().map(|it| (it.inst, it.px, it.qty)).collect()
    };
    let up = 640 * s;
    // .1 +1.5 bp over 400 ms (≥ 1 bp over 300 ms): 5 shares one cent inside the Up spread.
    let mut h = setup(&lead(&[]));
    assert!(
        places(&perp(&mut h, up, 10_000_000)).is_empty(),
        "BMK-054.1 first update"
    );
    let out = perp(&mut h, up + 400 * ms, 10_001_500);
    assert_eq!(
        places(&out),
        vec![(SLOTS[4], Px(460), Qty(5_000_000))],
        "BMK-054.1"
    );
    // .2 a fall favours Down (0.530 → 0.540).
    let mut h = setup(&lead(&[]));
    perp(&mut h, up, 10_000_000);
    let out = perp(&mut h, up + 400 * ms, 9_998_500);
    assert_eq!(
        places(&out),
        vec![(SLOTS[5], Px(540), Qty(5_000_000))],
        "BMK-054.2"
    );
    // .3 below the threshold, a one-cent spread, the cap (20 Up shares held) or outside the
    // window's [5 s, end − 10 s]: nothing.
    let mut h = setup(&lead(&[]));
    perp(&mut h, up, 10_000_000);
    assert!(
        places(&perp(&mut h, up + 400 * ms, 10_000_900)).is_empty(),
        "BMK-054.3 0.9 bp"
    );
    let mut h = setup(&lead(&[]));
    slot_book(h.views_mut(), SLOTS[4], 450, 460);
    perp(&mut h, up, 10_000_000);
    assert!(
        places(&perp(&mut h, up + 400 * ms, 10_001_500)).is_empty(),
        "BMK-054.3 spread"
    );
    let mut h = setup(&lead(&[]));
    let mut gp = GroupPosView::EMPTY;
    gp.n_legs = 1;
    gp.legs[0].qty = Qty(20_000_000);
    gp.legs[0].entry_px = Px(450);
    h.views_mut().group_pos(A1, gid(4), gp);
    perp(&mut h, up, 10_000_000);
    assert!(
        places(&perp(&mut h, up + 400 * ms, 10_001_500)).is_empty(),
        "BMK-054.3 cap"
    );
    for t in [602 * s, 891 * s] {
        let mut h = setup(&lead(&[]));
        perp(&mut h, t - 400 * ms, 10_000_000);
        assert!(
            places(&perp(&mut h, t, 10_001_500)).is_empty(),
            "BMK-054.3 at {t}"
        );
    }
    // .4 cool-down: a second up trigger 1 s later places nothing (even with the leg free), one
    // 2.1 s after the first places again.
    let mut h = setup(&lead(&[]));
    perp(&mut h, up, 10_000_000);
    assert_eq!(
        places(&perp(&mut h, up + 400 * ms, 10_001_500)).len(),
        1,
        "BMK-054.4"
    );
    let free = |h: &mut StrategyHarness<BinaryMaker>| {
        let q = &mut h.strategy_mut().fast.quotes[4];
        q.state = QState::None;
        q.r = None;
    };
    free(&mut h);
    perp(&mut h, up + 1_100 * ms, 10_001_500);
    assert!(
        places(&perp(&mut h, up + 1_400 * ms, 10_003_000)).is_empty(),
        "BMK-054.4 cool-down"
    );
    free(&mut h);
    perp(&mut h, up + 2_200 * ms, 10_003_000);
    assert_eq!(
        places(&perp(&mut h, up + 2_500 * ms, 10_004_500)).len(),
        1,
        "BMK-054.4 after"
    );
    // .5 lifetime: a live lead bid is cancelled 3 s after its place; a non-lead bid is cancelled
    // at once under lead_only.
    let live_lead = |h: &mut StrategyHarness<BinaryMaker>, slot: u32, placed: i64| {
        let q = &mut h.strategy_mut().fast.quotes[4];
        q.state = QState::Live;
        q.r = Some(order_ref(slot));
        q.px = 460;
        q.win = 600;
        h.strategy_mut().fast.lead.legs[4] = LeadLeg {
            handle: None,
            r: Some(order_ref(slot)),
            placed,
            life: 0,
        };
    };
    let mut h = setup(&lead(&[]));
    live_lead(&mut h, 31, up);
    assert!(
        cancels(&perp(&mut h, up + 2_900 * ms, 10_000_000)).is_empty(),
        "BMK-054.5 alive"
    );
    assert_eq!(
        cancels(&perp(&mut h, up + 3_000 * ms, 10_000_000)),
        vec![31],
        "BMK-054.5 life"
    );
    let mut h = setup(&lead(&[]));
    {
        let q = &mut h.strategy_mut().fast.quotes[5];
        q.state = QState::Live;
        q.r = Some(order_ref(32));
        q.px = 530;
        q.win = 600;
    }
    assert_eq!(
        cancels(&perp(&mut h, up, 10_000_000)),
        vec![32],
        "BMK-054.5 lead_only"
    );
    // .6 perp cancel: −0.6 bp over 1 s (≥ 0.5 bp) is against Up: its lead bid is cancelled.
    let mut h = setup(&lead(&[("lead_pc_w_ms", 1_000), ("lead_pc_x_bp", 50_000)]));
    live_lead(&mut h, 33, up);
    perp(&mut h, up, 10_000_000);
    assert_eq!(
        cancels(&perp(&mut h, up + 1_000 * ms, 9_999_400)),
        vec![33],
        "BMK-054.6"
    );
    // .7 lead_only: other events place nothing; rule 15 off (defaults on the same table): a
    // perpetual move places nothing.
    let mut h = setup(&lead(&[]));
    h.views_mut().at_engine_ns(up);
    assert!(
        placed(&h.feed(&ev::book(SLOTS[4], 4).as_ref())).is_empty(),
        "BMK-054.7"
    );
    let mut h = setup(&params_with(&[("ref_aux", 3)]));
    perp(&mut h, up, 10_000_000);
    assert!(
        places(&perp(&mut h, up + 400 * ms, 10_005_000)).is_empty(),
        "BMK-054.7 off"
    );
}

#[test]
fn bmk_055_rule15_sources_and_model_price() {
    use binary_maker::fcev;
    use binary_maker::lead::model_px;
    let s = 1_000_000_000i64;
    let ms = 1_000_000i64;
    // .1 the price formula (raw px, cent = 10): max(bid + 1, min(bid + maxtick, ⌊mid + E − m⌋, ask − 1)).
    // Δ 2 c/bp × 1.5 bp × k 1 = E 3 c; Up 0.450 / 0.500 (mid 47.5): ⌊47.5 + 3 − 0.5⌋ = 50 → min(48, 50, 49) = 48.
    let c = 1_000_000_000i64;
    assert_eq!(
        model_px(450, 500, 10, 2 * c, 150_000, c, c / 2, 3),
        480,
        "BMK-055.1 maxtick"
    );
    // E 0.3 c: ⌊47.5 + 0.3 − 0.5⌋ = 47 → 47 (inside the cap).
    assert_eq!(
        model_px(450, 500, 10, 200_000_000, 150_000, c, c / 2, 3),
        470,
        "BMK-055.1 model"
    );
    // E 0: ⌊47.5 − 0.5⌋ = 47 above bid + 1; a tight spread caps at ask − 1; never below bid + 1.
    assert_eq!(
        model_px(450, 470, 10, 0, 0, c, c / 2, 3),
        460,
        "BMK-055.1 ask − 1"
    );
    assert_eq!(
        model_px(450, 500, 10, 0, 0, c, 10 * c, 3),
        460,
        "BMK-055.1 floor bid + 1"
    );
    // .2 parity with sim2.py: the 116 lead bids PS1_mod05_pc_life1 placed on 2026-11-08 (29
    // windows) from the features sim2 computed.
    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/binary-maker-fcev/lead_px_20261108.jsonl");
    let text = std::fs::read_to_string(path).unwrap();
    let e9 = |x: f64| (x * 1e9).round() as i64;
    let (mut rows, mut same) = (0, 0);
    for line in text.lines() {
        let f = |k: &str| json_num(line, k);
        let up = line.contains("\"tok\": \"up\"");
        let dist = if up { f("dist_up") } else { -f("dist_up") };
        let delta = fcev::delta(e9(dist), e9(f("sq")), e9(f("remf")), e9(f("sig")));
        let raw = |cents: f64| (cents * 10.0).round() as i64;
        let move_abs = (f("ret_bp").abs() * 100_000.0).round() as i64;
        let px = model_px(raw(f("bid")), raw(f("ask")), 10, delta, move_abs, c, c / 2, 3);
        rows += 1;
        if px == raw(f("px")) {
            same += 1;
        }
    }
    assert_eq!(rows, 116, "BMK-055.2 rows");
    assert_eq!(same, rows, "BMK-055.2 identical prices");
    // .3 the spot source: lead_src 2, reference (spot) quotes +1.5 bp over 400 ms place the Up
    // lead bid; the perpetual alone (src 1 only) does not see spot moves; each source has its
    // own cool-down (a perpetual trigger, then a spot trigger in the same direction 0.5 s later
    // still fires).
    let g3 = GroupTable::new(&specs3(), CAPACITY);
    let lead = |extra: &[(&str, i64)]| {
        let mut f = vec![
            ("ref_aux", 3),
            ("lead_w_ms", 300),
            ("lead_x_bp", 100_000),
            ("lead_only", 1),
            ("inv_max_shares", 20),
        ];
        f.extend_from_slice(extra);
        params_with(&f)
    };
    let setup = |p: &BinaryMakerParams| {
        let mut h = StrategyHarness::<BinaryMaker>::new(p, &g3, fixture(), A1).expect("params");
        slot_book(h.views_mut(), SLOTS[4], 450, 470);
        slot_book(h.views_mut(), SLOTS[5], 530, 550);
        h
    };
    let spot = |h: &mut StrategyHarness<BinaryMaker>, t: i64, mid: i64| {
        h.views_mut().at_engine_ns(t);
        ref_quote(h.views_mut(), mid);
        let b = h.views_mut().bbo_of(R);
        h.feed(&ev::bbo(R, 6, b).as_ref())
    };
    let perp = |h: &mut StrategyHarness<BinaryMaker>, t: i64, mid: i64| {
        h.views_mut().at_engine_ns(t);
        h.views_mut()
            .bbo(RP, (Px(mid - 5), Qty(1_000_000)), (Px(mid + 5), Qty(1_000_000)));
        let b = h.views_mut().bbo_of(RP);
        h.feed(&ev::bbo(RP, 8, b).as_ref())
    };
    let t0 = 640 * s;
    let mut h = setup(&lead(&[("lead_src", 2)]));
    spot(&mut h, t0, 10_000_000);
    assert_eq!(
        placed(&spot(&mut h, t0 + 400 * ms, 10_001_500)),
        vec![(SLOTS[4], Px(460))],
        "BMK-055.3 spot"
    );
    let mut h = setup(&lead(&[]));
    spot(&mut h, t0, 10_000_000);
    assert!(
        placed(&spot(&mut h, t0 + 400 * ms, 10_001_500)).is_empty(),
        "BMK-055.3 src 1"
    );
    let mut h = setup(&lead(&[("lead_src", 3)]));
    perp(&mut h, t0, 10_000_000);
    spot(&mut h, t0, 10_000_000);
    assert_eq!(
        placed(&perp(&mut h, t0 + 400 * ms, 10_001_500)).len(),
        1,
        "BMK-055.3 perp first"
    );
    {
        let q = &mut h.strategy_mut().fast.quotes[4];
        q.state = binary_maker::fast::QState::None;
        q.r = None;
    }
    assert_eq!(
        placed(&spot(&mut h, t0 + 900 * ms, 10_001_500)).len(),
        1,
        "BMK-055.3 own cool-down"
    );
    // .4 lead_price 1 without Chainlink features (no strike TWAP): one cent above the bid.
    let mut h = setup(&lead(&[("lead_price", 1)]));
    perp(&mut h, t0, 10_000_000);
    assert_eq!(
        placed(&perp(&mut h, t0 + 400 * ms, 10_001_500)),
        vec![(SLOTS[4], Px(460))],
        "BMK-055.4"
    );
    // .5 lead_p_min: no lead bid below the minimum price (the bid above would be 0.46).
    let mut h = setup(&lead(&[("lead_p_min", 500_000_000)]));
    perp(&mut h, t0, 10_000_000);
    assert!(
        placed(&perp(&mut h, t0 + 400 * ms, 10_001_500)).is_empty(),
        "BMK-055.5 below"
    );
    let mut h = setup(&lead(&[("lead_p_min", 460_000_000)]));
    perp(&mut h, t0, 10_000_000);
    assert_eq!(
        placed(&perp(&mut h, t0 + 400 * ms, 10_001_500)),
        vec![(SLOTS[4], Px(460))],
        "BMK-055.5 at"
    );
}

/// The numbers of a JSON array field `"key": [..]` of one line.
fn json_arr(line: &str, key: &str) -> Vec<f64> {
    let pat = format!("\"{key}\": [");
    let at = line.find(&pat).unwrap_or_else(|| panic!("no {key}")) + pat.len();
    let rest = &line[at..];
    let end = rest.find(']').unwrap();
    rest[..end]
        .split(',')
        .map(|x| x.trim().parse().unwrap())
        .collect()
}

#[test]
fn bmk_056_rule16_head() {
    use binary_maker::fast::{HEAD_CANCEL, HEAD_ENTRY, HEAD_GATE, QState};
    use binary_maker::head::{self, HeadState, NF, Ring, SCALE, Top, inside_px, ix, predict};
    use binary_maker::lead::LeadLeg;
    let s = 1_000_000_000i64;
    let ms = 1_000_000i64;
    let dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/binary-maker-head");
    // .1 the ring: latest value at or before a time; (max, min) from a time on with the entry in
    // effect at that time; a thinned ring keeps one value per gap.
    let mut r = Ring::new(8, 0);
    for (t, v) in [(10, 100), (20, 90), (30, 120), (40, 110)] {
        r.push(t, v);
    }
    assert_eq!(
        (
            r.at_or_before(9),
            r.at_or_before(10),
            r.at_or_before(35),
            r.last()
        ),
        (None, Some(100), Some(120), Some(110)),
        "BMK-056.1 look-up"
    );
    assert_eq!(
        r.range_from(25),
        Some((120, 90)),
        "BMK-056.1 range incl. the entry in effect"
    );
    assert_eq!(r.range_from(5), Some((120, 90)), "BMK-056.1 range, all");
    assert_eq!(r.range_from(41), Some((110, 110)), "BMK-056.1 range, last only");
    let mut g = Ring::new(8, 10);
    for t in [0, 4, 9, 10, 15, 21] {
        g.push(t, t);
    }
    assert_eq!(
        (g.len(), g.at_or_before(20), g.last()),
        (3, Some(10), Some(21)),
        "BMK-056.1 thinned"
    );
    // .2 the bid price inside the spread (raw px, cent = 10): 0.450 / 0.500, expected rise 3 c,
    // m 0.5 c → min(48, ⌊47.5 + 3 − 0.5⌋ = 50, 49) = 48; rise 0 → 47; never below bid + 1.
    let c = 1_000_000_000i64;
    assert_eq!(inside_px(450, 500, 10, 30, c / 2, 3), 480, "BMK-056.2 maxtick");
    assert_eq!(inside_px(450, 500, 10, 0, c / 2, 3), 470, "BMK-056.2 no rise");
    assert_eq!(inside_px(450, 470, 10, 30, c / 2, 3), 460, "BMK-056.2 ask - 1");
    assert_eq!(
        inside_px(450, 500, 10, 0, 10 * c, 3),
        460,
        "BMK-056.2 floor bid + 1"
    );
    // .3 the generated model against LightGBM: 1 500 feature vectors (1e-6 fixed point).
    let text = std::fs::read_to_string(dir.join("pred.jsonl")).unwrap();
    let (mut n, mut worst) = (0, 0.0f64);
    for line in text.lines() {
        let x: Vec<i64> = json_arr(line, "x").iter().map(|v| *v as i64).collect();
        let mut a = [0i64; NF];
        a.copy_from_slice(&x);
        let d = (predict(&a) as f64 / SCALE as f64 - json_num(line, "y")).abs();
        worst = worst.max(d);
        n += 1;
    }
    assert_eq!(n, 1_500, "BMK-056.3 rows");
    assert!(worst < 1e-3, "BMK-056.3 worst difference {worst} c");
    // .4 the features against the Python reference on one recorder window (211 evaluations): the
    // Up mid changes and the reference quotes the look-backs need are replayed in time order.
    let text = std::fs::read_to_string(dir.join("feat.jsonl")).unwrap();
    let mut st = HeadState::default();
    let (mut q, mut no_bid, mut head_bad, mut head_sum) = (0, 0, 0, 0.0f64);
    let mut feat_bad = Vec::new();
    for line in text.lines() {
        let t = json_num(line, "t") as i64;
        // Times are 19 digits: parse the integer exactly.
        let t = {
            let at = line.find("\"t\": ").unwrap() + 5;
            let rest = &line[at..];
            let end = rest.find([',', '}']).unwrap();
            rest[..end].trim().parse::<i64>().unwrap_or(t)
        };
        if line.contains("\"k\": \"m\"") {
            st.on_mid(t, json_num(line, "mid_x2") as i64);
        } else if line.contains("\"k\": \"r\"") {
            st.on_ref(
                line.contains("\"s\": \"perp\""),
                t,
                json_num(line, "mid_x2") as i64,
            );
        } else {
            let fc = json_arr(line, "fc");
            let top = Top {
                bid: json_num(line, "bb") as i64,
                bid_q: (json_num(line, "bq") * 1e6).round() as i64,
                ask: json_num(line, "ba") as i64,
                ask_q: (json_num(line, "aq") * 1e6).round() as i64,
                unit_px: 1_000,
                unit_qty: 1_000_000,
            };
            let got = st.features(
                t,
                json_num(line, "age_ms") as i64,
                top,
                (fc[0] as i64, fc[1] as i64, fc[2] as i64, fc[3] as i64),
            );
            q += 1;
            // No bid on the Up token (the window's Up collapsed to 0): no head.
            if top.bid <= 0 {
                assert!(got.is_none(), "BMK-056.4 no bid at {t}");
                no_bid += 1;
                continue;
            }
            let x = got.unwrap_or_else(|| panic!("BMK-056.4 no features at {t}"));
            let want = json_arr(line, "x");
            for (k, (got, w)) in x.iter().zip(&want).enumerate() {
                let w6 = w * 1e6;
                let tol = match k {
                    ix::AGE => 1_000.0,
                    ix::SIG => 10.0,
                    ix::D_TOK => 200.0 + 2e-4 * w6.abs(),
                    ix::DELTA | ix::E300 | ix::E1000 => 3_000.0 + 3e-3 * w6.abs(),
                    k if k == ix::PERP + 4 || k == ix::SPOT + 4 => 60_000.0,
                    k if (ix::PERP..ix::E300).contains(&k) => 50.0,
                    _ => 10.0,
                };
                if (*got as f64 - w6).abs() > tol {
                    feat_bad.push((q, k, *got, w6));
                }
            }
            let d = (predict(&x) as f64 / 1e6 - json_num(line, "head")).abs();
            head_sum += d;
            if d > 0.15 {
                head_bad += 1;
            }
        }
    }
    assert_eq!(q, 211, "BMK-056.4 evaluations");
    assert!(
        feat_bad.is_empty(),
        "BMK-056.4 features (query, index, got, want): {:?}",
        &feat_bad[..feat_bad.len().min(8)]
    );
    let n = (q - no_bid) as f64;
    assert!(no_bid < q / 2, "BMK-056.4 {no_bid} of {q} without a bid");
    assert!(
        head_bad <= 4 && head_sum / n < 0.03,
        "BMK-056.4 head: {head_bad} above 0.15 c, mean {}",
        head_sum / n
    );
    // .5–.8 the strategy. Window 600 s (slots 4 Up, 5 Down): Up 0.450 / 0.480, Down 0.520 / 0.550;
    // Chainlink strike and price as in BMK-053; perpetual and spot quotes from 605 s on.
    let g3 = GroupTable::new(&specs3(), CAPACITY);
    let hp = |extra: &[(&str, i64)]| {
        let mut f = vec![("ref_aux", 3), ("lead_only", 1), ("inv_max_shares", 20)];
        f.extend_from_slice(extra);
        params_with(&f)
    };
    let perp = |h: &mut StrategyHarness<BinaryMaker>, t: i64, mid: i64| {
        h.views_mut().at_engine_ns(t);
        h.views_mut()
            .bbo(RP, (Px(mid - 5), Qty(1_000_000)), (Px(mid + 5), Qty(1_000_000)));
        let b = h.views_mut().bbo_of(RP);
        h.feed(&ev::bbo(RP, 8, b).as_ref())
    };
    let spot = |h: &mut StrategyHarness<BinaryMaker>, t: i64, mid: i64| {
        h.views_mut().at_engine_ns(t);
        ref_quote(h.views_mut(), mid);
        let b = h.views_mut().bbo_of(R);
        h.feed(&ev::bbo(R, 6, b).as_ref())
    };
    let setup = |p: &BinaryMakerParams, twap: bool| {
        let mut h = StrategyHarness::<BinaryMaker>::new(p, &g3, fixture(), A1).expect("params");
        if twap {
            h.feed(&ev::mark_at(RC, 9, Px(10_000_000), 600 * s).as_ref());
        }
        slot_book(h.views_mut(), SLOTS[4], 450, 480);
        slot_book(h.views_mut(), SLOTS[5], 520, 550);
        perp(&mut h, 605 * s, 10_010_000);
        spot(&mut h, 605 * s, 10_010_000);
        spot(&mut h, 639 * s + s / 2, 10_010_000);
        h.feed(&ev::index_at(RC, 9, Px(10_010_000), 640 * s).as_ref());
        h
    };
    let t0 = 641 * s;
    // .5 off (head_mode 0): no evaluation, nothing placed; on without the Chainlink strike: no
    // head, nothing placed.
    let mut h = setup(&hp(&[]), true);
    assert!(placed(&perp(&mut h, t0, 10_010_000)).is_empty(), "BMK-056.5 off");
    assert_eq!(h.strategy().fast.head.last, None, "BMK-056.5 off: not evaluated");
    let mut h = setup(
        &hp(&[("head_mode", i64::from(HEAD_ENTRY)), ("head_on", 0)]),
        false,
    );
    assert!(
        placed(&perp(&mut h, t0, 10_010_000)).is_empty(),
        "BMK-056.5 no features"
    );
    // .6 entry (threshold 0): the token the head favours gets a 5-share bid inside its spread at
    // max(bid + 1, min(bid + 3, ⌊mid + head − 0.5⌋, ask − 1)); a second quote inside head_cool_ms
    // (the leg freed by hand) places nothing; after it, again.
    let mut h = setup(&hp(&[("head_mode", i64::from(HEAD_ENTRY)), ("head_on", 0)]), true);
    let out = perp(&mut h, t0, 10_010_000);
    let hv = h.strategy().fast.head.last.expect("BMK-056.6 head");
    let (leg, bid, ask, ht) = if hv >= 0 {
        (4, 450, 480, hv)
    } else {
        (5, 520, 550, -hv)
    };
    let px = inside_px(bid, ask, 10, ht * 10 / SCALE, c / 2, 3);
    let got: Vec<_> = out.places().map(|it| (it.inst, it.px, it.qty)).collect();
    assert_eq!(
        got,
        vec![(SLOTS[leg], Px(px), Qty(5_000_000))],
        "BMK-056.6 head {hv}"
    );
    assert!(
        h.strategy().fast.lead.legs[leg].life == 1_000 * ms,
        "BMK-056.6 life"
    );
    let free = |h: &mut StrategyHarness<BinaryMaker>| {
        for k in [4, 5] {
            let q = &mut h.strategy_mut().fast.quotes[k];
            q.state = QState::None;
            q.r = None;
        }
    };
    free(&mut h);
    assert!(
        placed(&perp(&mut h, t0 + 500 * ms, 10_010_000)).is_empty(),
        "BMK-056.6 cool-down"
    );
    free(&mut h);
    h.feed(&ev::index_at(RC, 9, Px(10_010_000), 642 * s).as_ref());
    assert_eq!(
        placed(&perp(&mut h, t0 + 1_100 * ms, 10_010_000)).len(),
        1,
        "BMK-056.6 after the cool-down"
    );
    // .7 cancel (bit 4): a live lead bid whose token's head is below head_off is cancelled (here
    // head_off = 50 c: always); without the bit it stays.
    for (mode, want) in [(i64::from(HEAD_CANCEL), vec![31u32]), (0, vec![])] {
        let mut h = setup(&hp(&[("head_mode", mode), ("head_off", 50 * c)]), true);
        {
            let q = &mut h.strategy_mut().fast.quotes[4];
            q.state = QState::Live;
            q.r = Some(order_ref(31));
            q.px = 460;
            q.win = 600;
        }
        h.strategy_mut().fast.lead.legs[4] = LeadLeg {
            handle: None,
            r: Some(order_ref(31)),
            placed: t0 - 100 * ms,
            life: 0,
        };
        assert_eq!(
            cancels(&perp(&mut h, t0, 10_010_000)),
            want,
            "BMK-056.7 mode {mode}"
        );
    }
    // .8 gate (bit 2): a rule-15 trigger (+1.5 bp over 400 ms) places only when the head for the
    // token is at least head_gate: −50 c passes, +50 c blocks.
    for (gate, n) in [(-50 * c, 1usize), (50 * c, 0)] {
        let p = hp(&[
            ("head_mode", i64::from(HEAD_GATE)),
            ("head_gate", gate),
            ("lead_w_ms", 300),
            ("lead_x_bp", 100_000),
        ]);
        let mut h = setup(&p, true);
        perp(&mut h, t0, 10_010_000);
        assert_eq!(
            placed(&perp(&mut h, t0 + 400 * ms, 10_011_500)).len(),
            n,
            "BMK-056.8 gate {gate}"
        );
    }
    let _ = head::RET_MS;
}
