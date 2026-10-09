//! Property tests of the fast-part components (TESTS.md SLB-011, SLB-016, SLB-017, SLB-029,
//! SLB-036, SLB-037, SLB-051, SLB-055, SLB-057, SLB-061).

mod book;
mod common;
mod rf;

use std::panic::{AssertUnwindSafe, catch_unwind};

use common::{anchor_with, now_tsc, throttle_row, tsc_at};
use hft_strat_lib::{
    Confirm, ConfirmParams, ConfirmWindow, CostItem, CostLine, DirtyFilter, DirtyParams, DirtyReason,
    Expiring, FireKind, Fresh, Freshness, ImpactModel, LegFresh, Outcome, OutcomeAction, SpreadCalc,
    Throttle, ThrottleParams, ThrottleParamsNs, ThrottleReason, TriggerPx, Vwap, VwapParams, fee_round_trip,
};
use hft_strat_lib::{FeeLeg, Liquidity};
use hft_strategy_sdk::{
    BboView, BookStatus, EngineClockAnchor, Now, RefView, SnapshotReader, SnapshotWriter,
};
use hft_types::{InstId, Px, Qty, Rate, Side, Tsc};
use proptest::prelude::*;

// ------------------------------------------------------------------------------------ SLB-011

fn edge_i64() -> impl Strategy<Value = i64> {
    prop_oneof![
        Just(i64::MIN + 1),
        Just(-1i64),
        Just(0i64),
        Just(1i64),
        Just(1_000_000i64),
        Just(i64::MAX),
        any::<i64>(),
    ]
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(10_000))]

    #[test]
    fn slb_011_fee_sum_saturates(
        legs in prop::collection::vec((edge_i64(), edge_i64(), any::<bool>()), 1..=4),
    ) {
        let insts = [rf::I1, rf::I2, InstId(4), InstId(5)];
        let extra: Vec<(InstId, i64, i64)> =
            legs.iter().zip(insts).map(|(&(m, t, _), i)| (i, m, t)).collect();
        let rd = rf::build(hft_refdata::FeeAssetRule::ReceivedAsset, &extra);
        let v = RefView::new(&rd, rf::A1);
        let fl: Vec<FeeLeg> = legs
            .iter()
            .zip(insts)
            .map(|(&(_, _, taker), inst)| FeeLeg {
                inst,
                account: rf::A1,
                liq: if taker { Liquidity::Taker } else { Liquidity::Maker },
            })
            .collect();
        let sum: i128 = legs.iter().map(|&(m, t, tk)| i128::from(if tk { t } else { m })).sum();
        let lim = i128::from(i64::MAX);
        let want = (2 * sum).clamp(-lim, lim) as i64;
        prop_assert_eq!(fee_round_trip(&v, &fl), Ok(Rate(want)));
    }
}

// ------------------------------------------------------------------------- SLB-016, SLB-017

fn tick_s() -> impl Strategy<Value = i64> {
    prop_oneof![Just(1i64), Just(10), Just(100), Just(1_000)]
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(10_000))]

    #[test]
    fn slb_016_hit_buy_vs_spread(
        (tick, slow) in tick_s().prop_flat_map(|t| (Just(t), 1..=1_000_000_000i64 * t)),
        theta in -100_000_000i64..=100_000_000,
        k in -3i64..=3,
    ) {
        let mut t = TriggerPx::default();
        t.update(Px(slow), Px(slow), Rate(theta), Rate(0), Px(tick), 1);
        let b = Px(t.buy_bid_fast.0 + k * tick);
        let s = SpreadCalc::buy(b, Px(slow));
        if t.hit_buy(b) {
            prop_assert!(s.0 >= theta, "hit at {b:?} but spread {s:?} < {theta}");
        } else if s.0 >= theta {
            prop_assert_eq!(b.0, t.buy_bid_fast.0 - tick);
        }
    }

    #[test]
    fn slb_017_hit_sell_vs_spread(
        (tick, slow) in tick_s().prop_flat_map(|t| (Just(t), 1..=1_000_000_000i64 * t)),
        theta in -100_000_000i64..=100_000_000,
        k in -3i64..=3,
    ) {
        let mut t = TriggerPx::default();
        t.update(Px(slow), Px(slow), Rate(0), Rate(theta), Px(tick), 1);
        let a = Px(t.sell_ask_fast.0 + k * tick);
        let s = SpreadCalc::sell(a, Px(slow));
        if t.hit_sell(a) {
            prop_assert!(s.0 <= theta, "hit at {a:?} but spread {s:?} > {theta}");
        } else if s.0 <= theta {
            prop_assert_eq!(a.0, t.sell_ask_fast.0 + tick);
        }
    }
}

// ------------------------------------------------------------------------------------ SLB-029

#[derive(Clone, Copy, Debug)]
enum CwStep {
    Obs {
        cond: bool,
        from_slow: bool,
        seq: u64,
        dt: i64,
    },
    Reset,
}

fn cw_steps() -> impl Strategy<Value = Vec<CwStep>> {
    let obs = (
        prop::bool::weighted(0.9),
        any::<bool>(),
        0u64..=1_000,
        0i64..=50_000_000,
    )
        .prop_map(|(cond, from_slow, seq, dt)| CwStep::Obs {
            cond,
            from_slow,
            seq,
            dt,
        });
    prop::collection::vec(prop_oneof![99 => obs, 1 => Just(CwStep::Reset)], 1..=200)
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(10_000))]

    #[test]
    fn slb_029_confirm_no_false_inside(
        steps in cw_steps(),
        min_ms in 0u32..=200, min_updates in 0u16..=10, min_slow in 0u16..=10, min_span in 0u8..=10,
    ) {
        let p = ConfirmParams { min_ms, min_updates, min_slow_updates: min_slow, min_slow_span: min_span }.ns();
        let mut w = ConfirmWindow::default();
        // Naive reference: every observation of the current true-run since the last false/reset.
        let mut run: Vec<(bool, u64, i64)> = Vec::new();
        let mut now = 1_000_000_000i64;
        for (i, s) in steps.iter().enumerate() {
            match *s {
                CwStep::Reset => {
                    w.reset();
                    run.clear();
                }
                CwStep::Obs { cond, from_slow, seq, dt } => {
                    now += dt;
                    let got = w.observe(cond, from_slow, seq, now, &p);
                    if !cond {
                        run.clear();
                        prop_assert_eq!(got, Confirm::Idle);
                        continue;
                    }
                    run.push((from_slow, seq, now));
                    let (_, first_seq, t0) = run[0];
                    let n = run.len() as u64;
                    let n_slow = run.iter().filter(|r| r.0).count() as u64;
                    let max_seq = run.iter().map(|r| r.1).max().unwrap_or(first_seq);
                    let ok = now - t0 >= p.min_ns
                        && n >= u64::from(min_updates)
                        && n_slow >= u64::from(min_slow)
                        && max_seq.saturating_sub(first_seq) + 1 >= u64::from(min_span);
                    let want = if ok { Confirm::Confirmed } else { Confirm::Pending };
                    prop_assert_eq!(got, want, "step {}", i);
                }
            }
        }
    }
}

// ------------------------------------------------------------------------- SLB-036, SLB-037

fn healthy_slow(now: Tsc) -> LegFresh {
    LegFresh::from_quote(now, true, false)
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(10_000))]

    #[test]
    fn slb_036_dirty_matches_quote_live(
        stale_ms in 1u32..=10_000,
        tpm in 1_000_000u64..=4_000_000,
        live_sel in 0u8..=3,
        live_any in 0u64..=1_000_001_000_000,
        conn_up in any::<bool>(),
        resync in any::<bool>(),
        rx in any::<u64>(),
    ) {
        let now = 1_000_000_000_000u64;
        let lim = u64::from(stale_ms) * tpm;
        let live = match live_sel {
            0 => now - lim - 1,
            1 => now - lim,
            2 => now - lim + 1,
            _ => live_any,
        };
        let b = BboView {
            conn_live_tsc: Tsc(live),
            conn_up,
            resync,
            rx_tsc: Tsc(rx),
            ..BboView::EMPTY
        };
        let p = DirtyParams { stale_fast_ms: stale_ms, stale_slow_ms: 0, max_abs_spread: Rate(0), junk_after: 0 }
            .tsc(tpm);
        let fr = Freshness { fast: LegFresh::from_bbo(&b), slow: healthy_slow(Tsc(now)), no_quote_streak: 0 };
        let ok = DirtyFilter::check(Rate(0), &fr, Tsc(now), &p).is_ok();
        prop_assert_eq!(ok, b.quote_live(Tsc(now), lim));
    }

    #[test]
    fn slb_037_quiet_book_live(
        stale_ms in 1_500u32..=5_000,
        stop in prop::option::of(0i64..=10_000),
        disc in prop::option::of(0i64..=10_000),
        rsy in prop::option::of(0i64..=10_000),
    ) {
        let tpm = 3_000_000u64;
        let base = 50_000_000_000u64;
        let p = DirtyParams { stale_fast_ms: stale_ms, stale_slow_ms: 1_000, max_abs_spread: Rate(0), junk_after: 0 }
            .tsc(tpm);
        let lim = u64::from(stale_ms) * tpm;
        for ms in (0..=10_000i64).step_by(100) {
            let now = Tsc(base + ms as u64 * tpm);
            // Liveness evidence every second until `stop`; the quote itself never changes.
            let last_beat = ms.min(stop.unwrap_or(i64::MAX)) / 1_000 * 1_000;
            let up = disc.is_none_or(|d| ms < d);
            let rs = rsy.is_some_and(|r| ms >= r);
            let b = BboView {
                bid_px: Px(6_000_000),
                ask_px: Px(6_000_010),
                rx_tsc: Tsc(base),
                conn_live_tsc: Tsc(base + last_beat as u64 * tpm),
                conn_up: up,
                resync: rs,
                ..BboView::EMPTY
            };
            let fr = Freshness { fast: LegFresh::from_bbo(&b), slow: healthy_slow(now), no_quote_streak: 0 };
            let got = DirtyFilter::check(Rate(0), &fr, now, &p);
            let silent = now.0 - b.conn_live_tsc.0 > lim;
            let want = if !up || silent {
                Err(DirtyReason::Stale)
            } else if rs {
                Err(DirtyReason::Resync)
            } else {
                Ok(())
            };
            prop_assert_eq!(got, want, "t = {} ms", ms);
            if stop.is_none() && disc.is_none() && rsy.is_none() {
                prop_assert_eq!(got, Ok(()));
            }
        }
    }
}

// ------------------------------------------------------------------------------------ SLB-051

/// Naive reference of the README release table and outcome rules.
#[derive(Clone, Copy, Debug, Default)]
struct RefThrottle {
    cool: i64,
    inflight: bool,
    fails: u32,
    fallback: i64,
    backoff: i64,
    halted: bool,
    rearm: i64,
}

impl RefThrottle {
    fn can(&self, k: FireKind, t: i64) -> Result<(), ThrottleReason> {
        let open_close = matches!(k, FireKind::Open | FireKind::Close);
        let checks = [
            (open_close && self.inflight, ThrottleReason::InFlight),
            (open_close && t < self.cool, ThrottleReason::Cooldown),
            (k == FireKind::Open && self.halted, ThrottleReason::Halted),
            (open_close && t < self.backoff, ThrottleReason::Backoff),
            (!open_close && t < self.rearm, ThrottleReason::HardRearm),
        ];
        match checks.iter().find(|c| c.0) {
            Some(c) => Err(c.1),
            None => Ok(()),
        }
    }
    fn fire(&mut self, k: FireKind, t: i64, p: &ThrottleParamsNs) {
        if matches!(k, FireKind::Open | FireKind::Close) {
            self.inflight = true;
            self.cool = t.saturating_add(p.cooldown_ns);
        } else {
            self.rearm = t.saturating_add(p.hard_rearm_ns);
        }
    }
    fn outcome(&mut self, o: Outcome, t: i64, p: &ThrottleParamsNs) -> OutcomeAction {
        self.inflight = false;
        match o {
            Outcome::Failed => {
                self.fails = (self.fails + 1).min(255);
                if p.max_consec_fail > 0 && self.fails >= u32::from(p.max_consec_fail) {
                    OutcomeAction::RequestHalt
                } else {
                    OutcomeAction::None
                }
            }
            Outcome::Filled | Outcome::NoFill => {
                self.fails = 0;
                OutcomeAction::None
            }
            Outcome::Gated => OutcomeAction::None,
            Outcome::Rollback => {
                self.fallback = t.saturating_add(p.fallback_ns);
                OutcomeAction::EnterFallback
            }
        }
    }
}

#[derive(Clone, Copy, Debug)]
enum ThOp {
    Fire(u8),
    Outcome(u8),
    Halted(bool),
    Backoff(i64),
    ResetOpen,
}

fn kind(k: u8) -> FireKind {
    [FireKind::Open, FireKind::Close, FireKind::Flatten, FireKind::Hard][usize::from(k % 4)]
}
fn outcome(o: u8) -> Outcome {
    [
        Outcome::Filled,
        Outcome::NoFill,
        Outcome::Failed,
        Outcome::Rollback,
        Outcome::Gated,
    ][usize::from(o % 5)]
}

fn th_ops() -> impl Strategy<Value = Vec<(ThOp, i64)>> {
    let op = prop_oneof![
        (0u8..4).prop_map(ThOp::Fire),
        (0u8..5).prop_map(ThOp::Outcome),
        any::<bool>().prop_map(ThOp::Halted),
        prop_oneof![Just(0i64), Just(i64::MAX), 0i64..=1_000_000_000_000].prop_map(ThOp::Backoff),
        Just(ThOp::ResetOpen),
    ];
    prop::collection::vec((op, 0i64..=2_000_000_000), 1..=300)
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(10_000))]

    #[test]
    fn slb_051_throttle_invariants(
        ops in th_ops(),
        cool in 0u32..=10_000, fallback in 0u32..=10_000, rearm in 0u32..=10_000, max_fail in 0u8..=10,
    ) {
        let p = ThrottleParams { cooldown_ms: cool, max_consec_fail: max_fail, fallback_ms: fallback, hard_rearm_ms: rearm }.ns();
        let (mut t, mut r) = (Throttle::default(), RefThrottle::default());
        let mut now = 1_000_000_000i64;
        for (i, &(op, dt)) in ops.iter().enumerate() {
            now += dt;
            match op {
                ThOp::Fire(k) => {
                    t.on_fire(kind(k), now, &p);
                    r.fire(kind(k), now, &p);
                }
                ThOp::Outcome(o) => {
                    let before = throttle_row(&t).consec_fail;
                    let a = t.on_outcome(outcome(o), now, &p);
                    prop_assert_eq!(a, r.outcome(outcome(o), now, &p), "op {}", i);
                    if outcome(o) == Outcome::Gated {
                        prop_assert_eq!(throttle_row(&t).consec_fail, before);
                    }
                }
                ThOp::Halted(h) => {
                    t.set_halted(h);
                    r.halted = h;
                }
                ThOp::Backoff(b) => {
                    t.set_backoff(b);
                    r.backoff = b;
                }
                ThOp::ResetOpen => {
                    t.reset_open_state();
                    r.cool = 0;
                    r.fallback = 0;
                }
            }
            for k in 0..4 {
                let got = t.can_fire(kind(k), now);
                prop_assert_eq!(got, r.can(kind(k), now), "op {} kind {}", i, k);
                if matches!(kind(k), FireKind::Flatten | FireKind::Hard) {
                    prop_assert!(got.is_ok() || got == Err(ThrottleReason::HardRearm));
                }
                if kind(k) == FireKind::Close {
                    prop_assert_ne!(got, Err(ThrottleReason::Halted));
                }
            }
            if r.halted {
                prop_assert!(t.can_fire(FireKind::Open, now).is_err());
            }
            prop_assert_eq!(t.in_fallback(now), now < r.fallback);
        }
    }
}

// ------------------------------------------------------------------------------------ SLB-055

#[derive(Clone, Copy, Debug)]
enum RsOp {
    Th(ThOp),
    Cw { cond: bool, from_slow: bool, seq: u64 },
    TrUpd { buy: i64, sell: i64 },
    TrInv,
    TrHit(i64),
    ExPutAdvice { v: i64, valid_ms: u32 },
    ExPut { v: i64, valid_ns: i64 },
    ExGet,
}

fn rs_ops() -> impl Strategy<Value = Vec<(RsOp, i64)>> {
    let th = prop_oneof![
        (0u8..4).prop_map(ThOp::Fire),
        (0u8..5).prop_map(ThOp::Outcome),
        any::<bool>().prop_map(ThOp::Halted),
        Just(ThOp::ResetOpen),
    ];
    let op = prop_oneof![
        th.prop_map(RsOp::Th),
        (any::<bool>(), any::<bool>(), 0u64..100).prop_map(|(cond, from_slow, seq)| RsOp::Cw {
            cond,
            from_slow,
            seq
        }),
        (5_990_000i64..6_010_000, 5_990_000i64..6_010_000).prop_map(|(buy, sell)| RsOp::TrUpd { buy, sell }),
        Just(RsOp::TrInv),
        (5_990_000i64..6_010_000).prop_map(RsOp::TrHit),
        (any::<i64>(), 0u32..5_000).prop_map(|(v, valid_ms)| RsOp::ExPutAdvice { v, valid_ms }),
        (any::<i64>(), 0i64..5_000_000_000).prop_map(|(v, valid_ns)| RsOp::ExPut { v, valid_ns }),
        Just(RsOp::ExGet),
    ];
    prop::collection::vec((op, 0i64..=400_000_000), 1..=200)
}

/// The four components of one run.
#[derive(Clone, Copy, Debug)]
struct Comps {
    th: Throttle,
    cw: ConfirmWindow,
    tr: TriggerPx,
    ex: Expiring<i64>,
}

impl Comps {
    fn new() -> Comps {
        Comps {
            th: Throttle::default(),
            cw: ConfirmWindow::default(),
            tr: TriggerPx::default(),
            ex: Expiring::new(0),
        }
    }
    fn snapshot(&self) -> Vec<u8> {
        let mut w = SnapshotWriter::new(256);
        self.th.snapshot(&mut w);
        self.cw.snapshot(&mut w);
        self.tr.snapshot(&mut w);
        self.ex.snapshot(&mut w, |v, w| w.put_i64(*v));
        w.as_bytes().to_vec()
    }
    fn restore(b: &[u8]) -> Comps {
        let mut c = Comps::new();
        let mut r = SnapshotReader::new(b);
        c.th.restore(&mut r).unwrap();
        c.cw.restore(&mut r).unwrap();
        c.tr.restore(&mut r).unwrap();
        c.ex.restore(&mut r, |r| r.get_i64()).unwrap();
        c
    }
    /// One step; returns the observable result as a string-free value.
    fn step(&mut self, op: RsOp, n: &Now, p: &ThrottleParamsNs) -> i128 {
        let e = n.engine_ns;
        let cp = ConfirmParams {
            min_ms: 300,
            min_updates: 3,
            min_slow_updates: 1,
            min_slow_span: 2,
        }
        .ns();
        let code = |r: Result<(), ThrottleReason>| match r {
            Ok(()) => 0,
            Err(x) => 1 + x as i128,
        };
        match op {
            RsOp::Th(ThOp::Fire(k)) => {
                let c = code(self.th.can_fire(kind(k), e));
                self.th.on_fire(kind(k), e, p);
                c
            }
            RsOp::Th(ThOp::Outcome(o)) => self.th.on_outcome(outcome(o), e, p) as i128,
            RsOp::Th(ThOp::Halted(h)) => {
                self.th.set_halted(h);
                code(self.th.can_fire(FireKind::Open, e))
            }
            RsOp::Th(_) => {
                self.th.reset_open_state();
                code(self.th.can_fire(FireKind::Close, e))
            }
            RsOp::Cw { cond, from_slow, seq } => self.cw.observe(cond, from_slow, seq, e, &cp) as i128,
            RsOp::TrUpd { buy, sell } => {
                self.tr
                    .update(Px(buy), Px(sell), Rate(300_000), Rate(-300_000), Px(10), e);
                0
            }
            RsOp::TrInv => {
                self.tr.invalidate();
                0
            }
            RsOp::TrHit(px) => i128::from(self.tr.hit_buy(Px(px))) * 2 + i128::from(self.tr.hit_sell(Px(px))),
            RsOp::ExPutAdvice { v, valid_ms } => {
                self.ex.put_advice(v, n.seq, n.tsc, valid_ms, n);
                0
            }
            RsOp::ExPut { v, valid_ns } => {
                self.ex.put(v, e + valid_ns, n.seq);
                0
            }
            RsOp::ExGet => match self.ex.get(n) {
                Fresh::Missing => -1,
                Fresh::Stale(v) => -2 - i128::from(*v).abs(),
                Fresh::Fresh(v) => i128::from(*v),
            },
        }
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(5_000))]

    #[test]
    fn slb_055_restart_equivalence(
        ops in rs_ops(),
        at in any::<prop::sample::Index>(),
        tsc0 in 0u64..=1_000_000_000_000_000,
        tpm in prop_oneof![Just(1_000_000u64), Just(3_000_000u64)],
    ) {
        let p = ThrottleParams { cooldown_ms: 500, max_consec_fail: 3, fallback_ms: 2_000, hard_rearm_ms: 700 }.ns();
        let i = at.index(ops.len());
        let clk0: EngineClockAnchor = anchor_with(0, 3_000_000, 10_000_000_000);
        let (mut r, mut r0) = (Comps::new(), Comps::new());
        let (mut clk_r, mut boot_r) = (clk0, 500u64);
        let mut boot0 = 500u64;
        let mut ens = 10_000_000_000i64;
        for (k, &(op, dt)) in ops.iter().enumerate() {
            ens += dt;
            let seq = 1_000 + k as u64;
            if k == i {
                // Restart of R: snapshot, new TSC origin and rate, engine_ns continuous.
                let b = r.snapshot();
                clk_r = anchor_with(tsc0, tpm, ens);
                boot_r = seq;
                r = Comps::restore(&b);
                // Reference: same boot_seq advance, TriggerPx invalidated, nothing else.
                boot0 = seq;
                r0.tr.invalidate();
            }
            let n_r = now_tsc(clk_r, tsc_at(clk_r, ens), seq, boot_r);
            let n_0 = now_tsc(clk0, tsc_at(clk0, ens), seq, boot0);
            prop_assert_eq!(n_r.engine_ns, n_0.engine_ns);
            prop_assert_eq!(r.step(op, &n_r, &p), r0.step(op, &n_0, &p), "step {}", k);
        }
    }
}

// ------------------------------------------------------------------------------------ SLB-057

fn wild() -> impl Strategy<Value = i64> {
    prop_oneof![
        Just(i64::MIN),
        Just(-1i64),
        Just(0i64),
        Just(1i64),
        Just(i64::MAX),
        any::<i64>()
    ]
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(10_000))]

    #[test]
    fn slb_057_no_panic(
        a in wild(), b in wild(), c in wild(), d in wild(), e in wild(),
        u in any::<u64>(), w in any::<u32>(), x in any::<u16>(), y in any::<u8>(), z in any::<u8>(),
        flags in any::<u8>(), bools in any::<(bool, bool, bool)>(),
    ) {
        let r = catch_unwind(AssertUnwindSafe(|| {
            let _ = (SpreadCalc::buy(Px(a), Px(b)), SpreadCalc::sell(Px(c), Px(d)), SpreadCalc::net(Rate(a), Rate(e)));
            let mut cl = CostLine::default();
            let _ = cl.set(&[CostItem::new(Rate(a), x, flags), CostItem::new(Rate(b), 0, flags & 7)], Rate(c));
            let _ = (cl.round_trip(), cl.open_floor(), cl.out_of_quote());
            let mut t = TriggerPx::default();
            t.update(Px(a), Px(b), Rate(c), Rate(d), Px(e), a);
            let _ = (t.hit_buy(Px(b)), t.hit_sell(Px(c)));
            let cp = ConfirmParams { min_ms: w, min_updates: x, min_slow_updates: x, min_slow_span: y }.ns();
            let mut cw = ConfirmWindow::default();
            let _ = cw.observe(bools.0, bools.1, u, a, &cp);
            let _ = cw.observe(true, bools.2, u.wrapping_add(1), b, &cp);
            let dp = DirtyParams { stale_fast_ms: w, stale_slow_ms: w / 2, max_abs_spread: Rate(e), junk_after: x }.tsc(u);
            let fr = Freshness {
                fast: LegFresh::from_quote(Tsc(u), bools.0, bools.1),
                slow: LegFresh::from_quote(Tsc(u.rotate_left(7)), bools.2, false),
                no_quote_streak: x,
            };
            let _ = DirtyFilter::check(Rate(a), &fr, Tsc(u.rotate_right(3)), &dp);
            let tp = ThrottleParams { cooldown_ms: w, max_consec_fail: y, fallback_ms: w, hard_rearm_ms: w }.ns();
            let mut th = Throttle::default();
            for k in 0..4 {
                let _ = th.can_fire(kind(k), a);
                th.on_fire(kind(k), b, &tp);
                let _ = th.on_outcome(outcome(z.wrapping_add(k)), c, &tp);
            }
            th.set_halted(bools.0);
            th.set_backoff(d);
            let _ = th.in_fallback(e);
            th.reset_open_state();
            let book = book::TestBook::live(&[(a, b), (c, d)], &[(b, c), (d, e)]);
            let vp = VwapParams { book_frac: Rate(e), max_levels: y };
            let _ = ImpactModel::vwap(&book, Side::Buy, Qty(a), &vp);
            let _ = ImpactModel::vwap(&book, Side::Sell, Qty(b), &vp);
            let clk = anchor_with(u, 3_000_000, a);
            let n = now_tsc(clk, u.wrapping_add(u64::from(w)), u, u.rotate_left(1));
            let mut ex = Expiring::new(0i64);
            let _ = ex.get(&n);
            ex.put_advice(a, u, Tsc(u.rotate_left(9)), w, &n);
            let _ = ex.get(&n);
            ex.put(b, c, u);
            let _ = ex.get(&n);
        }));
        prop_assert!(r.is_ok());
        let mut th = Throttle::default();
        th.on_fire(FireKind::Open, i64::MAX, &ThrottleParams { cooldown_ms: 500, ..ThrottleParams::default() }.ns());
        prop_assert_eq!(throttle_row(&th).cooldown_until_ens, i64::MAX);
    }
}

// ------------------------------------------------------------------------------------ SLB-061

/// Book levels of one side, best first: monotone prices, quantities 1–10^10.
fn side_levels(buy_side: bool) -> impl Strategy<Value = Vec<(i64, i64)>> {
    (1usize..=80, 1_000_000i64..=100_000_000).prop_flat_map(move |(n, start)| {
        prop::collection::vec((1i64..=1_000, 1i64..=10_000_000_000), n).prop_map(move |steps| {
            let mut px = start;
            steps
                .iter()
                .map(|&(d, q)| {
                    let p = px;
                    px = if buy_side { px - d } else { px + d };
                    (p, q)
                })
                .collect()
        })
    })
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(10_000))]

    #[test]
    fn slb_061_vwap_bounds(
        buy in any::<bool>(),
        lv in side_levels(false),
        lv_b in side_levels(true),
        qty in 0i64..=100_000_000_000,
        frac in -1i64..=2_000_000_000,
        max_levels in 0u8..=255,
    ) {
        let (side, levels) = if buy { (Side::Buy, &lv) } else { (Side::Sell, &lv_b) };
        let book = if buy { book::TestBook::live(&[], levels) } else { book::TestBook::live(levels, &[]) };
        let p = VwapParams { book_frac: Rate(frac), max_levels };
        let top = levels[0];
        let got = ImpactModel::vwap(&book, side, Qty(qty), &p);
        let walk = usize::from(max_levels).min(64).min(levels.len());
        match got {
            Vwap::Top(px) => {
                prop_assert_eq!(px.0, top.0);
                prop_assert!(i128::from(top.1) * i128::from(frac) >= i128::from(qty) * 1_000_000_000);
            }
            Vwap::Walked { vwap, levels: n, impact } => {
                let n = usize::from(n);
                prop_assert!(n >= 1 && (qty == 0 || n <= walk));
                let last = levels[n - 1].0;
                let (lo, hi) = (top.0.min(last), top.0.max(last));
                prop_assert!(vwap.0 >= lo && vwap.0 <= hi, "vwap {} outside [{}, {}]", vwap.0, lo, hi);
                // Quantity conservation: the first n − 1 levels are taken whole and do not cover qty.
                let before: i128 = levels[..n - 1].iter().map(|l| i128::from(l.1)).sum();
                let through: i128 = before + i128::from(levels[n - 1].1);
                prop_assert!(qty == 0 || (before < i128::from(qty) && through >= i128::from(qty)));
                prop_assert_eq!(impact.0, vwap.ratio(Px(top.0)).0.abs());
                // Reference VWAP (i128), rounded against the taker.
                if qty > 0 {
                    let mut rem = i128::from(qty);
                    let mut sum = 0i128;
                    for l in &levels[..n] {
                        let t = i128::from(l.1).min(rem);
                        sum += i128::from(l.0) * t;
                        rem -= t;
                    }
                    let q = i128::from(qty);
                    let want = if buy { sum.div_euclid(q) + i128::from(sum.rem_euclid(q) != 0) } else { sum.div_euclid(q) };
                    prop_assert_eq!(i128::from(vwap.0), want);
                }
            }
            Vwap::Insufficient { avail } => {
                let sum: i128 = levels[..walk].iter().map(|l| i128::from(l.1)).sum();
                prop_assert_eq!(i128::from(avail.0), sum);
                prop_assert!(sum < i128::from(qty));
            }
        }
        let _ = BookStatus::Live;
    }
}
