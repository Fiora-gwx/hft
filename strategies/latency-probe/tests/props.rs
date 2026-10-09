//! Property tests (TESTS.md LPR-058–LPR-062).
#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods,
    clippy::type_complexity
)] // allow-list: test code

mod common;

use std::collections::BTreeMap;

use common::*;
use hft_strategy_sdk::{
    ExecUpdateKind, IntentClass, IntentOutcome, OrderRef, OrderStateView, OrderUpdateKind, OrderView,
    StuckReason, TargetOrigin, TargetReason, TargetState, TargetView,
};
use hft_types::{GroupId, Px, Qty, Rate, RejectBits, RunMode, Side, Tif, Tsc};
use latency_probe::fast::quote::probe_px;
use proptest::prelude::*;

/// One random step of the quote-group sequences.
#[derive(Clone, Copy, Debug)]
enum Step {
    Bbo {
        leg: u8,
        dt: u16,
        same_rx: bool,
        gate: u8,
    },
    Report {
        pick: u8,
        kind: u8,
        dt: u16,
    },
    Mode(bool),
    Restart,
}

fn steps(n: usize) -> impl Strategy<Value = Vec<Step>> {
    let s = prop_oneof![
        5 => (0u8..4, 0u16..=500, prop::bool::weighted(0.3), 0u8..10)
            .prop_map(|(leg, dt, same_rx, gate)| Step::Bbo { leg, dt, same_rx, gate }),
        5 => (any::<u8>(), 0u8..9, 0u16..=200).prop_map(|(pick, kind, dt)| Step::Report { pick, kind, dt }),
        1 => any::<bool>().prop_map(Step::Mode),
        1 => Just(Step::Restart),
    ];
    prop::collection::vec(s, 0..n)
}

fn report(kind: u8) -> OrderUpdateKind {
    match kind {
        0 => acked(),
        1 => cancelled(),
        2 => OrderUpdateKind::Aged,
        3 => rejected(0, 1),
        4 => rejected(RejectBits::RATE.0, 0),
        5 => OrderUpdateKind::Unknown,
        6 => filled(1_000, 5_820_000, false),
        7 => filled(100_000, 5_820_000, true),
        _ => acked(),
    }
}

const fn terminal(kind: u8) -> bool {
    matches!(kind, 1..=4 | 7)
}

/// Quote group of `n` legs (`xgw` for the cross-gw variant).
fn quote_group(n: usize, xgw: bool) -> G {
    let insts = [I2, I3, I1, I5];
    let mut grp = g(GroupId(0x0500_0061), &insts[..n], QUOTE);
    if xgw && n >= 2 {
        grp = grp.with(f::QUOTE_XGW, 1);
    }
    grp
}

/// Runs the steps; calls `on_place(t_ens, leg)` for every probe placed. Asserts at most one
/// probe in flight per leg (LPR-058).
fn run_quote(
    h: &mut H,
    n: usize,
    steps: &[Step],
    mut on_place: impl FnMut(i64, usize),
) -> Result<(), TestCaseError> {
    let gid = h.groups[0].spec.id;
    let insts: Vec<_> = (0..n).map(|k| h.groups[0].spec.legs[k].inst).collect();
    // ref → leg; place times (the OMS ages a resting probe after quote_rest_ms = 2 000 ms, so a
    // probe without any report for 4 000 ms counts as finished).
    let mut out: BTreeMap<OrderRef, usize> = BTreeMap::new();
    let mut placed_at: BTreeMap<OrderRef, i64> = BTreeMap::new();
    let mut last: Option<(OrderRef, u8, usize)> = None;
    let mut t = 0i64;
    let mut rx = 1u64;
    for s in steps {
        match *s {
            Step::Bbo {
                leg,
                dt,
                same_rx,
                gate,
            } => {
                t += i64::from(dt);
                h.at(t);
                let now = h.now().engine_ns;
                out.retain(|r, _| placed_at.get(r).is_none_or(|p| now < p + 4_000_000_000));
                let k = usize::from(leg) % n;
                if !same_rx {
                    rx += 1;
                }
                let gated = gate == 0;
                if gated {
                    h.next_outcomes = Some(vec![IntentOutcome::Rejected(RejectBits::RATE)]);
                }
                let rec = h.bbo_rx(insts[k], rx * 1_000);
                for (p, r) in rec.places.iter().zip(&rec.refs) {
                    let pl = (p.tag & 0xFF) as usize;
                    if p.tag >> 8 == 1 {
                        prop_assert!(
                            !out.values().any(|&l| l == pl),
                            "second probe on leg {} at +{} ms",
                            pl,
                            t
                        );
                        on_place(h.now().engine_ns, pl);
                        if !gated {
                            out.insert(*r, pl);
                            placed_at.insert(*r, h.now().engine_ns);
                        }
                    }
                }
                if gated {
                    h.next_outcomes = None;
                }
            }
            Step::Report { pick, kind, dt } => {
                t += i64::from(dt);
                h.at(t);
                let target = if out.is_empty() {
                    last
                } else {
                    let (r, l) = out
                        .iter()
                        .nth(usize::from(pick) % out.len())
                        .map(|(r, l)| (*r, *l))
                        .unwrap_or((r_of(0, V2), 0));
                    Some((r, kind, l))
                };
                let Some((r, kind, l)) = target else { continue };
                h.order(gid, r, probe_tag(l as u32), report(kind));
                if terminal(kind) {
                    out.remove(&r);
                }
                last = Some((r, kind, l));
            }
            Step::Mode(live) => {
                let (old, new) = if live {
                    (RunMode::CloseOnly, RunMode::Live)
                } else {
                    (RunMode::Live, RunMode::CloseOnly)
                };
                h.mode(gid, new);
                h.ev(&ev::mode(0, old, new));
            }
            Step::Restart => {
                // The engine still has the outstanding probes resting.
                h.v.orders = out
                    .iter()
                    .map(|(r, l)| OrderView {
                        r: *r,
                        group: gid,
                        inst: insts[*l],
                        side: Side::Buy,
                        tif: Tif::PostOnly,
                        class: IntentClass::Open,
                        px: Px(1),
                        qty: Qty(1),
                        filled: Qty(0),
                        state: OrderStateView::Live,
                        tag: probe_tag(*l as u32),
                    })
                    .collect();
                h.restart(7 + t as u64, 2 + t as u64);
            }
        }
    }
    Ok(())
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(2_000))]

    #[test]
    fn lpr_058_one_inflight_per_leg(n in 1usize..=4, xgw in any::<bool>(), steps in steps(2_000)) {
        let mut h = H::new(&[], vec![quote_group(n, xgw)]);
        h.all.clear();
        run_quote(&mut h, n, &steps, |_, _| {})?;
    }

    #[test]
    fn lpr_059_rate_bound(
        period in 100i64..=2_000,
        burst_sel in 0u8..3,
        burst_v in 1i64..=2_000,
        steps in steps(2_000),
    ) {
        let burst = if burst_sel == 0 { 0 } else { burst_v.min(period) };
        let grp = quote_group(1, false).with(f::PROBE_PERIOD_MS, period).with(f::BURST_PERIOD_MS, burst);
        let mut h = H::new(&[], vec![grp]);
        // Mode re-entry restarts the probe cadence (README "事件处理" Mode row); not part of this bound.
        let steps: Vec<Step> = steps.into_iter().filter(|s| !matches!(s, Step::Mode(_))).collect();
        let p = if burst == 0 { period } else { period.min(burst) } * 1_000_000;
        let mut times: Vec<i64> = Vec::new();
        run_quote(&mut h, 1, &steps, |t, _| times.push(t))?;
        for w in times.windows(2) {
            prop_assert!(w[1] - w[0] >= p, "probes {} ns apart, P = {} ns", w[1] - w[0], p);
        }
    }

    #[test]
    fn lpr_060_never_crosses(
        bid in 1i64..=10_000_000_000,
        spread in 0i64..=100_000,
        tick in prop_oneof![Just(1i64), Just(10), Just(100)],
        far in 5_000_000i64..=100_000_000,
    ) {
        let ask = bid + spread.min(bid / 100);
        for side in [Side::Buy, Side::Sell] {
            let px = probe_px(side, Px(bid), Px(ask), Rate(far), tick);
            if px.0 <= 0 {
                continue;
            }
            prop_assert_eq!(px.0 % tick, 0);
            match side {
                Side::Buy => prop_assert!(px.0 < ask, "buy {} ≥ ask {}", px.0, ask),
                Side::Sell => prop_assert!(px.0 > bid, "sell {} ≤ bid {}", px.0, bid),
            }
        }
    }
}

// --------------------------------------------------------------------------- LPR-061, LPR-062

#[derive(Clone, Copy, Debug)]
enum PStep {
    BboA { g: u8, dt: u16 },
    Timer { g: u8, tag: u8, dt: u16 },
    Fill { g: u8, done: bool },
    NoFill { g: u8 },
    Exec { g: u8, kind: u8 },
    Nets { g: u8, a: i8, b: i8 },
    EngineClear { g: u8 },
    Disable,
    Restart,
}

fn psteps(n: usize) -> impl Strategy<Value = Vec<PStep>> {
    let s = prop_oneof![
        4 => (any::<u8>(), 0u16..=3_000).prop_map(|(g, dt)| PStep::BboA { g, dt }),
        4 => (any::<u8>(), 1u8..=2, 0u16..=6_000).prop_map(|(g, tag, dt)| PStep::Timer { g, tag, dt }),
        2 => (any::<u8>(), any::<bool>()).prop_map(|(g, done)| PStep::Fill { g, done }),
        1 => any::<u8>().prop_map(|g| PStep::NoFill { g }),
        2 => (any::<u8>(), 0u8..6).prop_map(|(g, kind)| PStep::Exec { g, kind }),
        2 => (any::<u8>(), -1i8..=1, -1i8..=1).prop_map(|(g, a, b)| PStep::Nets { g, a, b }),
        1 => any::<u8>().prop_map(|g| PStep::EngineClear { g }),
        1 => Just(PStep::Disable),
        1 => Just(PStep::Restart),
    ];
    prop::collection::vec(s, 0..n)
}

fn exec_kind(k: u8) -> ExecUpdateKind {
    match k {
        0 => ExecUpdateKind::Done {
            cleared_by_engine: false,
        },
        1 => ExecUpdateKind::Done {
            cleared_by_engine: true,
        },
        2 => ExecUpdateKind::Stuck(StuckReason::NoLiquidity),
        3 => ExecUpdateKind::Stuck(StuckReason::Shadow),
        4 => ExecUpdateKind::Broken(hft_strategy_sdk::BrokenReason::LinkBroken),
        _ => ExecUpdateKind::Progress { leg_ix: 0, d: Qty(1) },
    }
}

/// Mirrors the engine's target table in the views: set on `target`, removed on `clear_target`.
fn mirror_targets(h: &mut H, rec: &Rec) {
    for t in &rec.targets {
        let tv = TargetView {
            intent: *t,
            origin: TargetOrigin::Strategy,
            state: TargetState::Working,
            set_seq: 1,
            set_tsc: Tsc(1),
            last_slice_tsc: Tsc(0),
            n_slices: 0,
            filled: [Qty(0); 4],
        };
        hft_strategy_sdk::testing::MockViews::target(&mut h.v.m, T1, t.group, Some(tv));
    }
    for (_, g) in &rec.clears {
        hft_strategy_sdk::testing::MockViews::target(&mut h.v.m, T1, *g, None);
    }
}

/// Runs pair/target steps over `groups`; `check(h, rec, disabled, targets_before)` per event.
fn run_pairs(
    groups: Vec<G>,
    steps: &[PStep],
    mut check: impl FnMut(&H, &Rec, bool, &[Option<TargetView>]) -> Result<(), TestCaseError>,
) -> Result<(), TestCaseError> {
    let mut h = H::new(&[], groups);
    let ids: Vec<GroupId> = h.groups.iter().map(|g| g.spec.id).collect();
    let n = ids.len();
    let mut t = 0i64;
    let mut disabled = false;
    let mut first_leg: BTreeMap<GroupId, OrderRef> = BTreeMap::new();
    for s in steps {
        let before: Vec<Option<TargetView>> = ids
            .iter()
            .map(|g| hft_strategy_sdk::Views::target(&h.v, T1, *g).copied())
            .collect();
        let rec = match *s {
            PStep::BboA { g, dt } => {
                t += i64::from(dt);
                h.at(t);
                let gid = ids[usize::from(g) % n];
                let a = h.table.get(h.gix_of(gid)).legs[0].inst;
                h.bbo(a)
            }
            PStep::Timer { g, tag, dt } => {
                t += i64::from(dt);
                h.at(t);
                h.timer(ids[usize::from(g) % n], u32::from(tag))
            }
            PStep::Fill { g, done } => {
                let gid = ids[usize::from(g) % n];
                let Some(r) = first_leg.get(&gid).copied() else {
                    continue;
                };
                h.order(gid, r, OPEN_TAG, filled(100_000, 6_000_000, done))
            }
            PStep::NoFill { g } => {
                let gid = ids[usize::from(g) % n];
                let Some(r) = first_leg.get(&gid).copied() else {
                    continue;
                };
                h.order(gid, r, OPEN_TAG, cancelled())
            }
            PStep::Exec { g, kind } => h.exec(ids[usize::from(g) % n], exec_kind(kind)),
            PStep::Nets { g, a, b } => {
                let gid = ids[usize::from(g) % n];
                h.net(gid, 0, i64::from(a) * 100_000)
                    .net(gid, 1, i64::from(b) * 100_000);
                continue;
            }
            PStep::EngineClear { g } => {
                hft_strategy_sdk::testing::MockViews::target(&mut h.v.m, T1, ids[usize::from(g) % n], None);
                continue;
            }
            PStep::Disable => {
                if !disabled {
                    disabled = true;
                    let groups = h.groups.clone();
                    h.params(&[(f::ENABLED, 0)], groups);
                }
                continue;
            }
            PStep::Restart => {
                h.restart(7 + t as u64, 2 + t as u64);
                continue;
            }
        };
        for (p, r) in rec.places.iter().zip(&rec.refs) {
            if p.tag == OPEN_TAG {
                first_leg.insert(p.group, *r);
            }
        }
        mirror_targets(&mut h, &rec);
        check(&h, &rec, disabled, &before)?;
    }
    Ok(())
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(2_000))]

    #[test]
    fn lpr_061_disabled_no_open(
        nq in 0usize..=1, np in 0usize..=1, nt in 0usize..=1,
        steps in psteps(1_000),
    ) {
        let mut groups = Vec::new();
        if nq == 1 {
            groups.push(g(GroupId(0x0500_0070), &[I1], QUOTE));
        }
        if np == 1 {
            groups.push(gpp());
        }
        if nt == 1 || groups.is_empty() {
            groups.push(g(GroupId(0x0500_0090), &[I4, I5], TARGET));
        }
        run_pairs(groups, &steps, |h, rec, disabled, _| {
            if disabled {
                for p in &rec.places {
                    prop_assert!(matches!(p.class, IntentClass::Reduce | IntentClass::Flatten), "{:?}", p);
                }
                for t in &rec.targets {
                    let flat = t.legs().iter().enumerate().all(|(k, l)| {
                        let net = hft_strategy_sdk::Views::legs(&h.v, T1, t.group).leg[k].net.0;
                        l.qty.0.abs() <= net.abs()
                    });
                    prop_assert!(t.reason == TargetReason::Exit || flat, "{:?}", t);
                }
            }
            Ok(())
        })?;
    }

    #[test]
    fn lpr_062_single_active_target(n in 1usize..=2, steps in psteps(1_000)) {
        let mut groups = vec![gts()];
        if n == 2 {
            groups.push(g(GroupId(0x0500_0090), &[I4, I5], TARGET));
        }
        run_pairs(groups.clone(), &steps, |h, rec, _, before| {
            for t in &rec.targets {
                prop_assert_eq!(rec.targets.iter().filter(|x| x.group == t.group).count(), 1);
                if t.reason == TargetReason::Rebalance {
                    let gix = h.gix_of(t.group);
                    prop_assert!(before[usize::from(gix)].is_none(), "Rebalance target over an active one");
                }
            }
            Ok(())
        })?;
    }
}
