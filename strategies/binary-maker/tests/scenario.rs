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
    assert_eq!(schema.version(), 10, "BMK-024.1");
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
    use binary_maker::fast::QState;
    let mut h = harness(p);
    let mut rng = TraceRng(seed);
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
    assert_eq!(
        <binary_maker::BinaryMaker as hft_strategy_sdk::Strategy>::FEATURES,
        COLUMNS,
        "BMK-035.3"
    );
    let mut via = [7i64; 9];
    let mut row = FeatureRow::new(&mut via);
    hft_strategy_sdk::Strategy::export_features(h.strategy(), &mut row);
    assert_eq!(via, buf, "BMK-035.3");
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
