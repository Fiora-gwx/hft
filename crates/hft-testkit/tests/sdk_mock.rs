//! TK-024–TK-035: MockViews, StrategyHarness and ModelHarness.

use std::any::TypeId;

use hft_strategy_sdk::params::{ParamOverride, RawParams};
use hft_strategy_sdk::testing::ev;
use hft_strategy_sdk::{
    Ctx, EventRef, FundingView, GroupTable, IntentClass, LegLinkOpt, LegView, NoModel, OrderIntent,
    ParamError, ParamSet, RefView, RestoreError, RiskStateView, SnapshotReader, SnapshotWriter, Strategy,
    StrategyDesc, StrategyInstance, SubsBuilder, Views,
};
use hft_testkit::sdk_mock::{Captured, ModelHarness, StrategyHarness};
use hft_testkit::strategies::probe_taker::{ProbeTaker, TakerParams};
use hft_testkit::strategies::probe_target::{ProbeTarget, TargetParams};
use hft_testkit::{ManualTsc, MockViews};
use hft_types::{AccountId, GroupId, InstId, Px, Qty, Rate, RunMode, Side, Tif, VenueId};

use crate::sdk_rd::{A1, G1, I1, I2, V1, gt1, gt3_fixture};

/// TakerParams with the given defaults (spread_th, qty_lots, close_on, hold_ms) for `n` groups.
pub fn taker(spread_th: i64, close_on: i64, n: u16) -> TakerParams {
    TakerParams::from_raw(
        &RawParams {
            defaults: vec![spread_th, 1, close_on, 5_000],
            overrides: Vec::new(),
        },
        n,
    )
}

/// TargetParams with schedule points and advice settings.
pub fn target(points: [i64; 3], need_advice: bool, max_age_ms: i64, advice_every: i64) -> TargetParams {
    TargetParams::from_raw(
        &RawParams {
            defaults: vec![
                60,
                0,
                max_age_ms,
                points[0],
                points[1],
                points[2],
                i64::from(need_advice),
                advice_every,
                2_048,
                64,
            ],
            overrides: Vec::new(),
        },
        1,
    )
}

/// A taker harness on GT1 with quotes making leg A (I1) bid exceed leg B (I2) ask by `bp_raw`.
pub fn taker_harness(p: &TakerParams) -> StrategyHarness<ProbeTaker> {
    let mut h = StrategyHarness::<ProbeTaker>::new(p, &gt1(), gt3_fixture(), A1).expect("params");
    h.views_mut().at_engine_ns(1_000_000_000);
    h
}

/// Sets the quotes: A bid = `a_bid`, B ask = `b_ask` (raw prices).
pub fn quotes(v: &mut MockViews, a_bid: i64, b_ask: i64) {
    MockViews::bbo(
        v,
        I1,
        (Px(a_bid), Qty(1_000_000)),
        (Px(a_bid + 10), Qty(1_000_000)),
    );
    MockViews::bbo(
        v,
        I2,
        (Px(b_ask - 10), Qty(1_000_000)),
        (Px(b_ask), Qty(1_000_000)),
    );
}

fn bbo_ev(v: &MockViews, i: InstId) -> hft_strategy_sdk::testing::ev::Ev {
    ev::bbo(i, 0, v.bbo_of(i))
}

#[test]
fn tk_024_no_clock_double_needed() {
    let clk: &'static ManualTsc = Box::leak(Box::new(ManualTsc::new(1_000_000_000, 0)));
    let mut h = taker_harness(&taker(1_000_000, 7, 1));
    h.bind_clock(clk);
    for k in 0..100i64 {
        h.views_mut().at_engine_ns(1_000_000_000 + k * 1_000_000);
        quotes(h.views_mut(), 1_000_000_000 + k, 1_000_000_000);
        let e = bbo_ev(h.views_mut(), I1);
        h.feed(&e.as_ref());
        let now = Views::now(h.views_mut());
        assert_eq!(
            now.engine_ns,
            1_000_000_000 + k * 1_000_000,
            "Now comes from the event's time"
        );
    }
    assert_eq!(clk.reads(), 0, "the manual clock was never read");
}

#[test]
fn tk_025_setters_visible() {
    let mut v = MockViews::new(gt3_fixture());
    // TK-025.1
    v.at_engine_ns(5_000);
    assert_eq!(Views::now(&v).engine_ns, 5_000, "TK-025.1");
    // TK-025.2
    MockViews::bbo(&mut v, I1, (Px(1_000_000), Qty(10)), (Px(1_000_100), Qty(7)));
    let b = Views::bbo(&v, I1);
    assert_eq!(
        (b.bid_px, b.bid_qty, b.ask_px, b.ask_qty),
        (Px(1_000_000), Qty(10), Px(1_000_100), Qty(7)),
        "TK-025.2"
    );
    // TK-025.3
    let lv = |px, qty| hft_event::Level {
        px: Px(px),
        qty: Qty(qty),
    };
    MockViews::book(&mut v, I1, &[lv(999_900, 10)], &[lv(1_000_200, 7)]);
    let bk = Views::book(&v, I1);
    assert_eq!(
        hft_strategy_sdk::BookRead::best(&bk, Side::Buy).map(|l| (l.px, l.qty)),
        Some((Px(999_900), Qty(10))),
        "TK-025.3"
    );
    assert_eq!(
        hft_strategy_sdk::BookRead::best(&bk, Side::Sell).map(|l| (l.px, l.qty)),
        Some((Px(1_000_200), Qty(7))),
        "TK-025.3"
    );
    // TK-025.4
    let mut f = *Views::funding(&v, I2);
    f.rate = Rate(100_000);
    MockViews::funding(&mut v, I2, f);
    assert_eq!(Views::funding(&v, I2).rate, Rate(100_000), "TK-025.4");
    // TK-025.5
    MockViews::portfolio(&mut v, A1, |p| {
        p.cash(V1, crate::sdk_rd::USDT, 5_000, 1);
    });
    let _ = Views::portfolio(&v, A1);
    // TK-025.6
    let mut l = *Views::legs(&v, A1, G1);
    l.naked_qty = Qty(3);
    MockViews::legs(&mut v, A1, G1, l);
    assert_eq!(Views::legs(&v, A1, G1).naked_qty, Qty(3), "TK-025.6");
    // TK-025.8
    MockViews::mode(&mut v, A1, G1, RunMode::Shadow);
    assert_eq!(Views::mode(&v, A1, G1), RunMode::Shadow, "TK-025.8");
    // TK-025.9
    let mut r: RiskStateView = Views::risk(&v, A1);
    r.conn_liveness_ms = 1_234;
    MockViews::risk(&mut v, A1, r);
    assert_eq!(Views::risk(&v, A1).conn_liveness_ms, 1_234, "TK-025.9");
    let _: Option<&LegView> = None;
    let _: Option<FundingView> = None;
}

#[test]
fn tk_026_captures_in_order() {
    let mut h = taker_harness(&taker(1_000_000, 7, 1));
    quotes(h.views_mut(), 1_002_000_000, 1_000_000_000);
    let e = bbo_ev(h.views_mut(), I1);
    let c = h.feed(&e.as_ref());
    assert_eq!(c.items.len(), 3, "{:?}", c.items);
    assert!(matches!(c.items[0], Captured::Place(p) if p.link.some == 1 && p.inst == I1));
    assert!(matches!(c.items[1], Captured::Timer(_)));
    assert!(matches!(c.items[2], Captured::Note(_)));
}

/// Places `CAP_PLACE + 1` intents in one event.
#[derive(Debug)]
struct Burst;

impl Strategy for Burst {
    const DESC: StrategyDesc = StrategyDesc {
        name: "burst",
        kind_id: 0xFFF0,
        ..<ProbeTaker as Strategy>::DESC
    };
    type Params = TakerParams;
    type Model = NoModel<TakerParams>;
    fn new(
        _: StrategyInstance,
        _: &TakerParams,
        _: &GroupTable,
        _: &RefView<'_>,
    ) -> Result<Self, ParamError> {
        Ok(Burst)
    }
    fn subscriptions(&self, _: &GroupTable, _: &mut SubsBuilder<'_>) {}
    fn on_event<V: Views>(&mut self, _: &EventRef<'_>, _: &V, ctx: &mut Ctx<'_>) {
        for k in 0..=hft_strategy_sdk::ctx::CAP_PLACE {
            let _ = ctx.place(intent(k as u32));
        }
    }
    fn on_params(&mut self, _: &TakerParams, _: &GroupTable, _: &RefView<'_>) -> Result<(), ParamError> {
        Ok(())
    }
    fn snapshot(&self, _: &mut SnapshotWriter) {}
    fn restore(&mut self, _: &mut SnapshotReader<'_>) -> Result<(), RestoreError> {
        Ok(())
    }
}

fn intent(tag: u32) -> OrderIntent {
    OrderIntent {
        venue: V1,
        account: A1,
        side: Side::Buy,
        tif: Tif::Ioc,
        class: IntentClass::Open,
        flags: 0,
        _pad: [0; 2],
        inst: I1,
        group: G1,
        px: Px(1),
        qty: Qty(1_000),
        link: LegLinkOpt::NONE,
        tag,
        max_rest_ms: 0,
    }
}

#[test]
fn tk_027_ctx_full_recorded() {
    let mut h = StrategyHarness::<Burst>::new(&taker(0, 0, 1), &gt1(), gt3_fixture(), A1).expect("params");
    let e = ev::timer(1, G1, 0);
    let c = h.feed(&e.as_ref());
    assert_eq!(c.places().count(), hft_strategy_sdk::ctx::CAP_PLACE);
    assert_eq!(c.full, 1, "the (capacity + 1)-th intent is recorded as CtxFull");
}

/// Misbehaving test strategies: `ALLOC` allocates, `CLOCK` reads the bound clock, `PANIC` panics at
/// the third event.
#[derive(Debug)]
struct Bad<const K: u8> {
    n: u32,
    v: Vec<u64>,
}

static BAD_CLOCK: std::sync::OnceLock<&'static ManualTsc> = std::sync::OnceLock::new();

impl<const K: u8> Strategy for Bad<K> {
    const DESC: StrategyDesc = StrategyDesc {
        name: "bad",
        kind_id: 0xFFF1,
        ..<ProbeTaker as Strategy>::DESC
    };
    type Params = TakerParams;
    type Model = NoModel<TakerParams>;
    fn new(
        _: StrategyInstance,
        _: &TakerParams,
        _: &GroupTable,
        _: &RefView<'_>,
    ) -> Result<Self, ParamError> {
        Ok(Bad {
            n: 0,
            v: Vec::with_capacity(1),
        })
    }
    fn subscriptions(&self, _: &GroupTable, _: &mut SubsBuilder<'_>) {}
    fn on_event<V: Views>(&mut self, _: &EventRef<'_>, _: &V, _: &mut Ctx<'_>) {
        self.n += 1;
        match K {
            0 => {
                self.v.push(1);
                self.v.push(2);
            }
            1 => {
                if let Some(c) = BAD_CLOCK.get() {
                    let mut r: &ManualTsc = c;
                    let _ = hft_rt::ClockReads::tsc(&mut r);
                }
            }
            _ => assert!(self.n < 3, "planted panic"),
        }
    }
    fn on_params(&mut self, _: &TakerParams, _: &GroupTable, _: &RefView<'_>) -> Result<(), ParamError> {
        Ok(())
    }
    fn snapshot(&self, _: &mut SnapshotWriter) {}
    fn restore(&mut self, _: &mut SnapshotReader<'_>) -> Result<(), RestoreError> {
        Ok(())
    }
}

#[test]
#[should_panic(expected = "分配次数 1")]
fn tk_028_alloc_fails_assertion() {
    assert!(hft_testkit::alloc::CountingAlloc::is_installed());
    let mut h = StrategyHarness::<Bad<0>>::new(&taker(0, 0, 1), &gt1(), gt3_fixture(), A1).expect("params");
    let e = ev::timer(1, G1, 0);
    h.feed(&e.as_ref());
}

#[test]
#[should_panic(expected = "读取了时钟")]
fn tk_029_clock_read_fails_assertion() {
    let clk: &'static ManualTsc = Box::leak(Box::new(ManualTsc::new(1_000_000_000, 0)));
    let _ = BAD_CLOCK.set(clk);
    let mut h = StrategyHarness::<Bad<1>>::new(&taker(0, 0, 1), &gt1(), gt3_fixture(), A1).expect("params");
    h.bind_clock(BAD_CLOCK.get().copied().unwrap_or(clk));
    let e = ev::timer(1, G1, 0);
    h.feed(&e.as_ref());
}

#[test]
fn tk_030_panic_reported() {
    let mut h = StrategyHarness::<Bad<2>>::new(&taker(0, 0, 1), &gt1(), gt3_fixture(), A1).expect("params");
    let e = ev::timer(1, G1, 0);
    assert!(h.feed(&e.as_ref()).items.is_empty());
    assert!(h.feed(&e.as_ref()).items.is_empty());
    let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        h.feed(&e.as_ref());
    }));
    let msg = r
        .err()
        .and_then(|p| p.downcast_ref::<String>().cloned())
        .unwrap_or_default();
    assert!(msg.contains("event 2") && msg.contains("planted panic"), "{msg}");
}

#[test]
fn tk_031_on_params_failure_keeps_state() {
    let mut h = taker_harness(&taker(1_000_000, 7, 1));
    for k in 0..10 {
        quotes(h.views_mut(), 1_002_000_000 + k, 1_000_000_000);
        let e = bbo_ev(h.views_mut(), I1);
        h.feed(&e.as_ref());
    }
    let a = h.snapshot();
    // hold_ms in (0, 10) fails `check`.
    let bad = TakerParams::from_raw(
        &RawParams {
            defaults: vec![1_000_000, 1, 7, 5],
            overrides: Vec::new(),
        },
        1,
    );
    assert!(matches!(h.on_params(&bad), Err(ParamError::Invalid { .. })));
    assert_eq!(a, h.snapshot());
}

#[test]
fn tk_032_snapshot_roundtrip() {
    let mut h = taker_harness(&taker(1_000_000, 7, 1));
    for k in 0..10 {
        quotes(h.views_mut(), 1_002_000_000 + k, 1_000_000_000);
        let e = bbo_ev(h.views_mut(), I1);
        h.feed(&e.as_ref());
    }
    // TK-032.1
    assert_eq!(h.snapshot_roundtrip(), Ok(()), "TK-032.1");
    // TK-032.2: a restore that skips a field.
    let mut d = StrategyHarness::<Lossy>::new(&taker(0, 0, 1), &gt1(), gt3_fixture(), A1).expect("params");
    let e = ev::timer(1, G1, 0);
    d.feed(&e.as_ref());
    let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| d.snapshot_roundtrip()));
    let msg = r
        .err()
        .and_then(|p| p.downcast_ref::<String>().cloned())
        .unwrap_or_default();
    assert!(msg.contains("byte offset 8"), "TK-032.2: {msg}");
}

/// Restores `a` but drops `b` (planted).
#[derive(Debug)]
struct Lossy {
    a: u64,
    b: u64,
}

impl Strategy for Lossy {
    const DESC: StrategyDesc = StrategyDesc {
        name: "lossy",
        kind_id: 0xFFF2,
        ..<ProbeTaker as Strategy>::DESC
    };
    type Params = TakerParams;
    type Model = NoModel<TakerParams>;
    fn new(
        _: StrategyInstance,
        _: &TakerParams,
        _: &GroupTable,
        _: &RefView<'_>,
    ) -> Result<Self, ParamError> {
        Ok(Lossy { a: 0, b: 0 })
    }
    fn subscriptions(&self, _: &GroupTable, _: &mut SubsBuilder<'_>) {}
    fn on_event<V: Views>(&mut self, _: &EventRef<'_>, _: &V, _: &mut Ctx<'_>) {
        self.a += 1;
        self.b += 7;
    }
    fn on_params(&mut self, _: &TakerParams, _: &GroupTable, _: &RefView<'_>) -> Result<(), ParamError> {
        Ok(())
    }
    fn snapshot(&self, w: &mut SnapshotWriter) {
        w.put_u64(self.a);
        w.put_u64(self.b);
    }
    fn restore(&mut self, r: &mut SnapshotReader<'_>) -> Result<(), RestoreError> {
        self.a = r.get_u64()?;
        Ok(())
    }
}

#[test]
fn tk_033_model_harness_advice() {
    let p = target([0, 0, 0], false, 1_000, 5);
    let mut m = ModelHarness::<ProbeTarget>::new(&p, &gt1(), gt3_fixture(), A1);
    let mut all = Vec::new();
    for k in 0..10 {
        m.views_mut().at_engine_ns(1_000_000 + k);
        let e = ev::timer(1, G1, 0);
        all.extend(m.feed(&e.as_ref()));
    }
    assert_eq!(all.len(), 2);
    for (_, _, b) in &all {
        assert!(b.len() <= 4_096);
    }
    assert_eq!(all[0].2[..8], 5u64.to_le_bytes());
    assert_eq!(all[1].2[..8], 10u64.to_le_bytes());
}

#[test]
fn tk_034_bad_params_rejected() {
    // spread_th (field 0) is per_group; group 0 gets max + 1.
    let raw = RawParams {
        defaults: vec![1_000_000, 1, 7, 5_000],
        overrides: Vec::new(),
    };
    let mut body = hft_strategy_sdk::params::encode_body(&TakerParams::SCHEMA, 1, &raw).expect("encode");
    body[4] = 1; // n_overrides = 1
    let o = ParamOverride {
        group_ix: 0,
        field_ix: 0,
        _pad: 0,
        value: 1_000_000_001,
    };
    body.extend_from_slice(zerocopy::IntoBytes::as_bytes(&o));
    let r = TakerParams::decode(&body, 1, &gt1());
    assert!(matches!(r, Err(ParamError::OutOfRange { .. })), "{r:?}");
    let h = StrategyHarness::<ProbeTaker>::from_body(&body, 1, &gt1(), gt3_fixture(), A1);
    assert!(matches!(h, Err(ParamError::OutOfRange { .. })));
}

#[test]
fn tk_035_single_mockviews() {
    assert_eq!(
        TypeId::of::<hft_testkit::MockViews>(),
        TypeId::of::<hft_strategy_sdk::testing::MockViews>()
    );
    let src = include_str!("../src/sdk_mock.rs");
    assert_eq!(
        src.matches("struct MockViews").count(),
        0,
        "re-exported, not redefined"
    );
    let _ = (AccountId(0), GroupId(0), VenueId(0), I2, Rate(0));
    let _ = target([0, 0, 0], false, 0, 1);
}
