#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods
)] // allow-list: test code
#![allow(clippy::type_complexity, clippy::too_many_arguments)]
mod common;

use common::*;
use hft_event::{ExternalCause, GwOp, OrderAction, OrderEventKind, SendStatus};
use hft_oms::*;
use hft_strategy_sdk::{HedgeKind, HedgePlan, LinkRole, OnFail, OrderRef};
use hft_types::{ASSET_NONE, AssetId, Qty, RejectBits, Tif, Tsc};
use proptest::prelude::*;

const LOT_Y: i64 = 100_000;

fn ratio_plan(k: u8) -> HedgePlan {
    let mut p = plan_l1();
    let (n, d) = [(1, 1), (2, 1), (1, 3)][usize::from(k)];
    p.ratio_num = n;
    p.ratio_den = d;
    p
}

/// Big GTC first leg (10 BTC) so that increments never fill it.
fn big_first(h: &mut H, plan: HedgePlan) -> (OrderRef, u32) {
    let (f, l) = place_first(h, q("10"), Tif::Gtc, plan, 0);
    h.ack(f, u64::from(f.slot), 0);
    (f, l)
}

fn fee_asset(k: u8) -> AssetId {
    [BTC, USDT, ASSET_NONE][usize::from(k)]
}

/// Delivers a cumulative fill with a fee; returns the hedge requests it produced.
fn fill_fee(h: &mut H, f: OrderRef, cum: i64, fee: i64, fa: AssetId, t: u64) -> Vec<HedgeRequest> {
    let mut e = h.oe(f, OrderEventKind::PartialFill);
    e.cum_filled = Qty(cum);
    e.avg_px = p("60000.00");
    e.fee = Qty(if fa == BTC { fee } else { 0 });
    e.fee_asset = fa;
    h.ev(&e, t);
    h.out.hedges.to_vec()
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(2_000))]

    #[test]
    fn oms_382_hedge_sum_conserved(
        rk in 0u8..3,
        incs in prop::collection::vec((1i64..=10_000_000, 0u8..3), 1..=20),
        late in 0usize..=5,
    ) {
        let plan = ratio_plan(rk);
        let mut h = H::new();
        let (f, _l) = big_first(&mut h, plan);
        let (num, den) = (i128::from(plan.ratio_num), i128::from(plan.ratio_den));
        let mut cum = 0i64;
        let mut net_total: i128 = 0;
        let mut sent: i128 = 0;
        let mut adjusts = 0i128;
        let mut late_fees: Vec<i64> = Vec::new();
        for (k, (inc, fk)) in incs.iter().enumerate() {
            cum += inc;
            let fee = inc / 1000;
            let fa = fee_asset(*fk);
            for r in fill_fee(&mut h, f, cum, fee, fa, 1 + k as u64) {
                prop_assert!(r.qty.0 % LOT_Y == 0);
                prop_assert!(r.qty.0 <= r.basis.0);
                sent += i128::from(r.qty.0);
            }
            net_total += i128::from(if fa == BTC { inc - fee } else { *inc });
            if fa == ASSET_NONE && late_fees.len() < late {
                late_fees.push(fee);
            }
        }
        for fee in late_fees {
            let mut e = h.oe(f, OrderEventKind::FeeOnly);
            e.fee = Qty(fee);
            e.fee_asset = BTC;
            h.ev(&e, 100);
            h.o.net_adjust(A1, g1(), X, Qty(-fee));
            net_total -= i128::from(fee);
            adjusts += 1;
        }
        let res = i128::from(h.legs(A1, g1()).view.leg[1].residual.0);
        let want = net_total * num / den;
        prop_assert!((sent + res - want).abs() <= adjusts, "sent {} res {} want {}", sent, res, want);
    }
}

/// Independent `naked_dir` from the legs (hedge side Sell, ratio 1/1, X → Y).
fn naked_ref(h: &H) -> i64 {
    let v = h.legs(A1, g1()).view;
    let (f, hh) = (v.leg[0], v.leg[1]);
    let e = i128::from(f.net.0) + i128::from(hh.net.0);
    let x = e + i128::from(f.inflight_immediate.0).min(0) + i128::from(hh.inflight_immediate.0).min(0);
    x.max(0) as i64
}

#[derive(Copy, Clone, Debug)]
enum L3 {
    Fill(i64, u8),
    External(i64, u8, bool),
    PlaceReject,
    Retry,
    Late(i64),
}

fn l3() -> impl Strategy<Value = L3> {
    prop_oneof![
        3 => (1i64..=5_000_000, 0u8..3).prop_map(|(a, b)| L3::Fill(a, b)),
        1 => (-3_000_000i64..=3_000_000, 0u8..4, any::<bool>()).prop_map(|(a, b, c)| L3::External(a, b, c)),
        1 => Just(L3::PlaceReject),
        1 => Just(L3::Retry),
        1 => (1i64..=1000).prop_map(L3::Late),
    ]
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(2_000))]

    #[test]
    fn oms_383_hedge_le_naked(ops in prop::collection::vec(l3(), 1..=30)) {
        let mut h = H::with(|c| c.hedge_deadline_ms = 1_000_000);
        let (f, l) = big_first(&mut h, plan_l1());
        let mut cum = 0i64;
        let mut pending: Option<HedgeRequest> = None;
        let mut t = 1u64;
        for op in ops {
            t += 10;
            match op {
                L3::Fill(inc, fk) => {
                    cum += inc;
                    fill_fee(&mut h, f, cum, inc / 1000, fee_asset(fk), t);
                }
                L3::External(net, ck, held) => {
                    let cause = [ExternalCause::Liquidation, ExternalCause::Adl, ExternalCause::Manual, ExternalCause::Unknown][usize::from(ck)];
                    h.out.clear();
                    h.o.external_adjust(A1, VX, g1(), Y, Qty(net), if held { 4242 } else { 0 }, cause, &mut h.out);
                }
                L3::PlaceReject => {
                    if let Some(r) = pending.take()
                        && let Ok(s) = place_hedge(&mut h, &r, t)
                    {
                        let e = h.oe(s, OrderEventKind::Reject);
                        h.ev(&e, t + 1);
                    }
                }
                L3::Retry => h.retry(l, t),
                L3::Late(fee) => {
                    let mut e = h.oe(f, OrderEventKind::FeeOnly);
                    e.fee = Qty(fee);
                    e.fee_asset = BTC;
                    h.ev(&e, t);
                    h.o.net_adjust(A1, g1(), X, Qty(-fee));
                }
            }
            for r in h.out.hedges.iter() {
                prop_assert!(matches!(r.role, LinkRole::Second | LinkRole::SecondRetry | LinkRole::Reverse));
                let lot = if r.inst == Y { LOT_Y } else { 1000 };
                prop_assert_eq!(r.hedge_cap_qty.0, r.basis.0 + lot);
                if r.role != LinkRole::Reverse {
                    let n = naked_ref(&h);
                    prop_assert!(r.qty.0 <= n - n % LOT_Y, "qty {} naked {}", r.qty.0, n);
                }
                pending = Some(*r);
            }
        }
    }

    #[test]
    fn oms_384_residual_across_links(
        links in prop::collection::vec(prop::collection::vec(1i64..=3_000_000, 1..=5), 1..=10),
        rk in 0u8..3,
    ) {
        let plan = ratio_plan(rk);
        let (num, den) = (i128::from(plan.ratio_num), i128::from(plan.ratio_den));
        let mut h = H::new();
        let mut sent: i128 = 0;
        let mut total: i128 = 0;
        let mut t = 1;
        for incs in &links {
            let (f, _) = big_first(&mut h, plan);
            let mut cum = 0;
            for inc in incs {
                cum += inc;
                t += 1;
                for r in fill_fee(&mut h, f, cum, 0, USDT, t) {
                    sent += i128::from(r.qty.0);
                    // Settle the second leg so that the next fills are not capped.
                    let s = place_hedge(&mut h, &r, t).unwrap();
                    h.fill(s, r.qty, p("59990.0"), t);
                }
                let res = i128::from(h.legs(A1, g1()).view.leg[1].residual.0);
                prop_assert!(res.abs() < i128::from(LOT_Y) + 1);
            }
            total += i128::from(cum);
            h.what(f, OrderEventKind::CancelAck, t);
        }
        let res = i128::from(h.legs(A1, g1()).view.leg[1].residual.0);
        let want = total * num / den;
        prop_assert!((sent + res - want).abs() <= links.len() as i128);
    }
}

#[derive(Copy, Clone, Debug)]
enum Fail {
    Reject,
    IocPartial,
    IocZero,
    Send(SendStatus),
    Px(HedgePxErr),
    RiskKilled,
}

fn fail() -> impl Strategy<Value = Fail> {
    prop_oneof![
        Just(Fail::Reject),
        Just(Fail::IocPartial),
        Just(Fail::IocZero),
        Just(Fail::Send(SendStatus::EncodeError)),
        Just(Fail::Send(SendStatus::KillBlocked)),
        Just(Fail::Send(SendStatus::RateLimited)),
        Just(Fail::Send(SendStatus::NoConn)),
        Just(Fail::Send(SendStatus::RingFull)),
        Just(Fail::Px(HedgePxErr::StaleQuote)),
        Just(Fail::Px(HedgePxErr::BeyondCross)),
        Just(Fail::RiskKilled),
    ]
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(2_000))]

    #[test]
    fn oms_385_retry_counting(max in 0u8..=5, fails in prop::collection::vec(fail(), 1..=12), ro in 0u8..2) {
        let mut h = H::with(|c| {
            c.hedge_retry_max = max;
            c.hedge_deadline_ms = 10_000_000;
        });
        let mut plan = plan_l1();
        plan.reduce_only = ro;
        let (f, l) = place_first(&mut h, q("0.1"), Tif::Ioc, plan, 0);
        h.fill(f, q("0.1"), p("60000.00"), 0);
        let mut req = Some(one_hedge(&h));
        let mut counted = 0u8;
        // SecondRetry requests that follow an execution failure (gates do not count).
        let mut n_retry = 0;
        let mut after_counted = false;
        let mut t = 10;
        for fl in fails {
            let Some(r) = req.take() else { break };
            if r.role == LinkRole::SecondRetry {
                n_retry += usize::from(after_counted);
                prop_assert_eq!(r.reduce_only, ro != 0);
            }
            let c0 = counted;
            match fl {
                Fail::Px(e) => {
                    h.out.clear();
                    h.o.hedge_gated(l, HedgeGate::Px(e), Tsc(0), ms(t), 1, &mut h.out);
                }
                Fail::RiskKilled => {
                    h.out.clear();
                    h.o.hedge_gated(l, HedgeGate::Risk(RejectBits::KILLED), Tsc(0), ms(t), 1, &mut h.out);
                }
                _ => {
                    let s = place_hedge(&mut h, &r, t).unwrap();
                    match fl {
                        Fail::Reject => {
                            let e = h.oe(s, OrderEventKind::Reject);
                            h.ev(&e, t);
                            counted += 1;
                        }
                        Fail::IocPartial => {
                            h.fill(s, Qty(LOT_Y), p("59990.0"), t);
                            h.what(s, OrderEventKind::Expired, t);
                            counted += 1;
                        }
                        Fail::IocZero => {
                            h.what(s, OrderEventKind::Expired, t);
                            counted += 1;
                        }
                        Fail::Send(st) => {
                            h.send(VX, A1, GwOp::Place, s.slot, s.r#gen, st, t, t);
                            counted += u8::from(st == SendStatus::EncodeError);
                        }
                        _ => {}
                    }
                }
            }
            after_counted = counted > c0;
            let e = h.link(l);
            if e.state == LinkState::Broken {
                break;
            }
            prop_assert_eq!(e.retries, counted);
            let tm = h.timers(OmsTimerKind::HedgeRetry);
            prop_assert_eq!(tm.len(), 1);
            let at = tm[0].deadline_tsc.0 / MS;
            h.retry(l, at);
            t = at + 1;
            req = h.out.hedges.first().copied();
        }
        prop_assert!(n_retry <= usize::from(max));
    }

    #[test]
    fn oms_386_simul_reverse_qty(a in 0i64..=1_200_000, b in 0i64..=1_200_000) {
        let mut plan = plan_l1();
        plan.kind = HedgeKind::Simultaneous;
        plan.on_fail = OnFail::Reverse;
        let mut h = H::new();
        let (f, _l) = place_first(&mut h, q("0.012"), Tif::Ioc, plan, 0);
        let r = one_hedge(&h);
        let s = place_hedge(&mut h, &r, 0).unwrap();
        if a > 0 {
            h.fill(f, Qty(a), p("60000.00"), 1);
        }
        h.what(f, OrderEventKind::Expired, 1);
        let mut out = Vec::new();
        if b > 0 {
            h.fill(s, Qty(b), p("59990.0"), 2);
            out.extend(h.out.hedges.iter().copied());
        }
        h.what(s, OrderEventKind::Expired, 2);
        out.extend(h.out.hedges.iter().copied());
        let rev: Vec<_> = out.iter().filter(|x| x.role == LinkRole::Reverse).collect();
        let (want, on_first) = if a > b {
            (a - b - (a - b) % 1000, true)
        } else {
            (b - a - (b - a) % LOT_Y, false)
        };
        if want == 0 {
            prop_assert!(rev.is_empty());
        } else {
            prop_assert_eq!(rev.len(), 1);
            prop_assert_eq!(rev[0].qty.0, want);
            prop_assert_eq!(rev[0].inst, if on_first { X } else { Y });
        }
    }

    #[test]
    fn oms_387_vetoed_no_orders(ops in prop::collection::vec(l3(), 1..=30), at in 0usize..30) {
        let mut h = H::with(|c| c.hedge_deadline_ms = 1_000_000);
        let (f, l) = big_first(&mut h, plan_l1());
        let mut cum = 0i64;
        let mut vetoed = false;
        let mut t = 1;
        for (k, op) in ops.into_iter().enumerate() {
            t += 10;
            if k == at {
                h.out.clear();
                h.o.hedge_vetoed(l, ms(t), 1, &mut h.out);
                vetoed = true;
            }
            match op {
                L3::Fill(inc, _) => {
                    cum += inc;
                    fill_fee(&mut h, f, cum, 0, USDT, t);
                }
                L3::Retry | L3::PlaceReject => h.retry(l, t),
                L3::External(net, _, _) => {
                    h.out.clear();
                    h.o.external_adjust(A1, VX, g1(), Y, Qty(net), 0, ExternalCause::Adl, &mut h.out);
                }
                L3::Late(_) => h.sweep(t),
            }
            if vetoed {
                prop_assert!(!h.out.hedges.iter().any(|r| r.link_id == l));
                prop_assert!(h.decisions(OrderAction::New).is_empty());
            }
        }
    }
}
