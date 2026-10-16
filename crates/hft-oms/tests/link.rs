#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods
)] // allow-list: test code
#![allow(clippy::type_complexity, clippy::too_many_arguments)]
use crate::common;

use common::*;
use hft_event::{ExternalCause, GwOp, NoteLevel, OrderAction, OrderEventKind, SendStatus};
use hft_oms::notes;
use hft_oms::*;
use hft_strategy_sdk::{HedgeKind, HedgePlan, LegLink, LinkRole, OnFail, OrderRef};
use hft_types::{Qty, RejectBits, Tif, Tsc};

/// L1 first leg (IOC, `qty`) filled at t = 0; returns (h, first, link) with the Second request in `h.out`.
fn filled(plan: HedgePlan, qty: &str) -> (H, OrderRef, u32) {
    let mut h = H::new();
    let (f, l) = place_first(&mut h, q(qty), Tif::Ioc, plan, 0);
    h.fill(f, q(qty), p("60000.00"), 0);
    (h, f, l)
}

fn reject(h: &mut H, r: OrderRef, t: u64) {
    let mut e = h.oe(r, OrderEventKind::Reject);
    e.reject_code = 1;
    h.ev(&e, t);
}

fn retry_timers(h: &H) -> Vec<OmsTimerReq> {
    h.timers(OmsTimerKind::HedgeRetry)
}

#[test]
fn oms_043_link_table_full() {
    let mut h = H::with(|c| c.links_per_account = 1);
    let _ = place_first(&mut h, q("0.012"), Tif::Ioc, plan_l1(), 0);
    let free = h.o.table_view(VS, A1).unwrap().free_len;
    assert_eq!(
        h.place(&first_leg(q("0.012"), Tif::Ioc, plan_l1()), 0),
        Err(PlaceError::LinkTableFull)
    );
    assert_eq!(h.o.table_view(VS, A1).unwrap().free_len, free);
    assert!(h.out.is_empty());
    assert!(h.place(&buy_x(), 0).is_ok());
}

#[test]
fn oms_044_hedge_place_err_counts() {
    let mut h = H::with(|c| c.slots.set(VX, A1, 1));
    let mut b = sell_y();
    b.intent.group = hft_types::GROUP_NONE;
    let _blocker = h.place_ok(&b, 0);
    let (f, l) = place_first(&mut h, q("0.012"), Tif::Ioc, plan_l1(), 0);
    h.fill(f, q("0.012"), p("60000.00"), 0);
    let req = one_hedge(&h);
    assert_eq!(h.link(l).owed(), q("0.012"));
    let plan = h.o.link_plan(l).unwrap().1;
    let it = ApprovedIntent::hedge(
        &req,
        &plan,
        HedgePx::default(),
        hft_strategy_sdk::IntentClass::Hedge,
    );
    h.out.clear();
    let r = h.o.place(&it, ms(1000), 1, &mut h.out);
    assert_eq!(r, Err(PlaceError::NoFreeSlot));
    assert!(h.out.decisions.is_empty());
    assert_eq!(h.link(l).retries, 1);
    let t = retry_timers(&h);
    assert_eq!(
        (t.len(), t[0].id, t[0].deadline_tsc, t[0].cancel),
        (1, l, ms(1100), false)
    );
}

/// Places the request in `h.out` and returns its slot.
fn place_req(h: &mut H, t: u64) -> OrderRef {
    let req = one_hedge(h);
    place_hedge(h, &req, t).unwrap()
}

#[test]
fn oms_076_encode_error_exhausts() {
    let (mut h, _f, l) = filled(plan_l1(), "0.012");
    let mut roles = vec![one_hedge(&h).role];
    let hs = place_req(&mut h, 0);
    h.send(
        VX,
        A1,
        GwOp::Place,
        hs.slot,
        hs.r#gen,
        SendStatus::EncodeError,
        10,
        10,
    );
    for t in [120u64, 230, 340] {
        h.retry(l, t - 10);
        roles.push(one_hedge(&h).role);
        let r = place_req(&mut h, t - 10);
        h.send(
            VX,
            A1,
            GwOp::Place,
            r.slot,
            r.r#gen,
            SendStatus::EncodeError,
            t,
            t,
        );
    }
    assert_eq!(
        roles,
        vec![
            LinkRole::Second,
            LinkRole::SecondRetry,
            LinkRole::SecondRetry,
            LinkRole::SecondRetry
        ]
    );
    let e = h.link(l);
    assert_eq!(e.state, LinkState::Broken);
    let u: Vec<_> = h
        .out
        .links
        .iter()
        .filter(|u| u.state == LinkState::Broken)
        .collect();
    assert_eq!((u.len(), u[0].broken_reason as u8), (1, 1));
    assert_eq!(h.notes(notes::HEDGE_BROKEN)[0].level, NoteLevel::P0);
    h.retry(l, 500);
    assert!(h.out.is_empty());
}

#[test]
fn oms_077_kill_blocked_deadline() {
    let (mut h, _f, l) = filled(plan_l1(), "0.012");
    let mut hs = place_req(&mut h, 0);
    let mut t = 1;
    loop {
        h.send(
            VX,
            A1,
            GwOp::Place,
            hs.slot,
            hs.r#gen,
            SendStatus::KillBlocked,
            t,
            t,
        );
        if h.link(l).state == LinkState::Broken {
            assert!(t > 3000);
            break;
        }
        assert!(t <= 3000, "t = {t}");
        assert_eq!((h.link(l).retries, h.link(l).state), (0, LinkState::Hedging));
        assert_eq!(retry_timers(&h).len(), 1);
        let at = retry_timers(&h)[0].deadline_tsc.0 / MS;
        h.retry(l, at);
        if h.link(l).state == LinkState::Broken {
            assert!(at > 3000);
            break;
        }
        hs = place_req(&mut h, at);
        t = at + 1;
    }
    let lu = h.out.links.iter().find(|u| u.state == LinkState::Broken).unwrap();
    assert_eq!(lu.broken_reason, LinkBroken::Deadline);
    assert_eq!(h.n_notes(notes::HEDGE_BROKEN), 1);
}

#[test]
fn oms_151_unknown_leg_blocks_retry() {
    for on_fail in [OnFail::Retry, OnFail::Reverse] {
        let mut plan = plan_l1();
        plan.on_fail = on_fail;
        let (mut h, _f, l) = filled(plan, "0.012");
        let hs = place_req(&mut h, 0);
        h.what(hs, OrderEventKind::OutcomeUnknown, 1);
        h.retry(l, 100);
        assert!(
            h.out.hedges.is_empty() && h.decisions(OrderAction::New).is_empty(),
            "{on_fail:?}"
        );
    }
}

#[test]
fn oms_180_link_created() {
    let mut h = H::new();
    let (f, l) = place_first(&mut h, q("0.012"), Tif::Ioc, plan_l1(), 0);
    let e = h.link(l);
    assert_eq!(
        (
            e.state,
            e.first,
            e.ref_px,
            e.first_net,
            e.hedge_req,
            e.hedge_filled,
            e.hedge_open
        ),
        (LinkState::Open, f, p("60010.0"), Qty(0), Qty(0), Qty(0), Qty(0))
    );
    assert_eq!((e.first_fill_tsc, e.retries, e.flags), (Tsc(0), 0, 0));
    let u = h.out.links[0];
    assert_eq!(
        (
            h.out.links.len(),
            u.state,
            u.first,
            u.tolerance,
            u.residual_policy,
            u.flags
        ),
        (
            1,
            LinkState::Open,
            f,
            r("0.02"),
            hft_strategy_sdk::ResidualPolicy::Defer,
            0
        )
    );
    assert!(h.slot(f).oflags & hft_event::OrderFlags::LINKED.0 != 0);
    assert_eq!((h.ext(f).role, h.slot(f).link_id), (1, l));
    assert_eq!(h.decisions(OrderAction::New)[0].link_role, 1);
    assert_eq!(h.legs(A1, g1()).view.open_links, 1);
}

#[test]
fn oms_181_onfill_basic() {
    let mut h = H::new();
    let (f, l) = place_first(&mut h, q("0.012"), Tif::Ioc, plan_l1(), 0);
    h.seq = 999;
    h.fill(f, q("0.012"), p("60000.01"), 100);
    assert_eq!(h.legs(A1, g1()).view.leg[0].net, q("0.012"));
    let e = h.link(l);
    assert_eq!(
        (e.first_net, e.first_fill_tsc, e.hedge_req, e.state),
        (q("0.012"), ms(100), q("0.012"), LinkState::Hedging)
    );
    let r = one_hedge(&h);
    assert_eq!(
        (
            r.link_id,
            r.role,
            r.venue,
            r.account,
            r.side,
            r.reduce_only,
            r.inst,
            r.group,
            r.strategy.0
        ),
        (
            l,
            LinkRole::Second,
            VX,
            A1,
            hft_types::Side::Sell,
            false,
            Y,
            g1(),
            1
        )
    );
    assert_eq!(
        (
            r.qty,
            r.basis,
            r.hedge_cap_qty,
            r.ref_px,
            r.peer_venue,
            r.peer_account,
            r.trigger_seq
        ),
        (q("0.012"), q("0.012"), q("0.013"), p("60010.0"), VS, A1, 1000)
    );
    let u: Vec<_> = h
        .out
        .links
        .iter()
        .filter(|u| u.state == LinkState::Hedging)
        .collect();
    assert_eq!((u.len(), u[0].first_net), (1, q("0.012")));
}

#[test]
fn oms_182_residual_carry() {
    let mut h = H::new();
    let (f, _l) = place_first(&mut h, q("0.02"), Tif::Gtc, plan_l1(), 0);
    h.ack(f, 1, 0);
    h.fill(f, q("0.0123"), p("60000.00"), 1);
    let r = one_hedge(&h);
    assert_eq!(
        (r.qty, r.basis, r.hedge_cap_qty),
        (q("0.012"), q("0.0123"), q("0.0133"))
    );
    assert_eq!(h.legs(A1, g1()).view.leg[1].residual, q("0.0003"));
    h.fill(f, q("0.0131"), p("60000.00"), 2);
    let r = one_hedge(&h);
    assert_eq!((r.qty, r.basis), (q("0.001"), q("0.0011")));
    assert_eq!(h.legs(A1, g1()).view.leg[1].residual, q("0.0001"));
}

#[test]
fn oms_183_min_qty_notional() {
    for (row, inc, mq, mn, sent, res) in [
        ("OMS-183.1", "0.0009", "0.001", 5_000_000i64, None, "0.0009"),
        ("OMS-183.2", "0.001", "0.001", 5_000_000, Some("0.001"), "0"),
        ("OMS-183.3", "0.012", "0.001", usdt("720.12"), Some("0.012"), "0"),
        ("OMS-183.4", "0.012", "0.001", usdt("720.13"), None, "0.012"),
        ("OMS-183.5", "0.001", "0.002", 5_000_000, None, "0.001"),
    ] {
        let rd = build_rd(&RdOpts {
            y_min_qty: q(mq),
            y_min_notional: mn,
            ..RdOpts::default()
        });
        let mut h = H::from_cfg(&p0(), rd);
        let (f, _l) = place_first(&mut h, q("0.02"), Tif::Gtc, plan_l1(), 0);
        h.ack(f, 1, 0);
        h.fill(f, q(inc), p("60000.00"), 1);
        match sent {
            Some(x) => assert_eq!(one_hedge(&h).qty, q(x), "{row}"),
            None => assert!(h.out.hedges.is_empty(), "{row}"),
        }
        assert_eq!(h.legs(A1, g1()).view.leg[1].residual, q(res), "{row}");
    }
}

#[test]
fn oms_184_base_fee_net_hedge() {
    let mut h = H::new();
    let (f, l) = place_first(&mut h, q("0.0123"), Tif::Ioc, plan_l1(), 0);
    let mut e = h.oe(f, OrderEventKind::Fill);
    e.cum_filled = q("0.0123");
    e.avg_px = p("60000.00");
    e.fee = q("0.0000123");
    e.fee_asset = BTC;
    h.ev(&e, 1);
    let r = one_hedge(&h);
    assert_eq!((r.qty, r.basis), (q("0.012"), q("0.0122877")));
    assert_eq!(h.legs(A1, g1()).view.leg[1].residual, q("0.0002877"));
    assert_eq!(h.link(l).first_net, q("0.0122877"));
}

#[test]
fn oms_185_ratio() {
    for (row, num, den, inc, qty, basis, res) in [
        ("OMS-185.1", 2u32, 1u32, "0.005", "0.010", Some("0.010"), None),
        ("OMS-185.2", 1, 3, "0.006", "0.002", None, None),
        (
            "OMS-185.3",
            1,
            3,
            "0.0061",
            "0.002",
            Some("0.00203333"),
            Some("0.00003333"),
        ),
    ] {
        let mut plan = plan_l1();
        plan.ratio_num = num;
        plan.ratio_den = den;
        let mut h = H::new();
        let (f, _) = place_first(&mut h, q("0.02"), Tif::Gtc, plan, 0);
        h.ack(f, 1, 0);
        h.fill(f, q(inc), p("60000.00"), 1);
        let r = one_hedge(&h);
        assert_eq!(r.qty, q(qty), "{row}");
        if let Some(b) = basis {
            assert_eq!(r.basis, q(b), "{row}");
        }
        if let Some(x) = res {
            assert_eq!(h.legs(A1, g1()).view.leg[1].residual, q(x), "{row}");
        }
    }
}

#[test]
fn oms_186_naked_cap() {
    let mut h = H::new();
    h.o.test_set_net(A1, g1(), Y, q("-0.005"));
    let (f, l) = place_first(&mut h, q("0.0123"), Tif::Ioc, plan_l1(), 0);
    h.fill(f, q("0.0123"), p("60000.00"), 1);
    let r = one_hedge(&h);
    assert_eq!((r.qty, r.basis), (q("0.007"), q("0.0123")));
    let n = h.notes(notes::HEDGE_NAKED_CAPPED);
    assert_eq!((n.len(), n[0].level), (1, NoteLevel::P1));
    assert_eq!(&n[0].v[..4], &[u64::from(l), 1_200_000, 700_000, 730_000]);
    assert_eq!(h.link(l).hedge_req, q("0.007"));
    assert_eq!(h.legs(A1, g1()).view.leg[1].residual, q("0.0003"));
}

#[test]
fn oms_187_naked_cap_below_min() {
    let mut h = H::new();
    h.o.test_set_net(A1, g1(), Y, q("-0.0115"));
    let (f, l) = place_first(&mut h, q("0.012"), Tif::Ioc, plan_l1(), 0);
    h.fill(f, q("0.012"), p("60000.00"), 1);
    assert!(h.out.hedges.is_empty());
    assert_eq!(h.n_notes(notes::HEDGE_NAKED_CAPPED), 1);
    assert_eq!(h.link(l).hedge_req, Qty(0));
    assert_eq!(h.legs(A1, g1()).view.leg[1].residual, Qty(0));
}

#[test]
fn oms_188_second_retry() {
    let (mut h, _f, l) = filled(plan_l1(), "0.012");
    let hs = place_req(&mut h, 100);
    assert_eq!(h.link(l).hedge_open, q("0.012"));
    reject(&mut h, hs, 200);
    let e = h.link(l);
    assert_eq!((e.retries, e.owed(), e.hedge_open), (1, q("0.012"), Qty(0)));
    let t = retry_timers(&h);
    assert_eq!((t.len(), t[0].id, t[0].deadline_tsc), (1, l, ms(300)));
    h.retry(l, 300);
    let r = one_hedge(&h);
    assert_eq!(
        (r.role, r.qty, r.basis, r.hedge_cap_qty, r.reduce_only),
        (LinkRole::SecondRetry, q("0.012"), q("0.012"), q("0.013"), false)
    );
    assert_eq!(h.link(l).hedge_req, q("0.012"));
}

#[test]
fn oms_189_retry_interval() {
    let setup = |min: u32| {
        let mut h = H::with(|c| c.hedge_retry_min_ms = min);
        let (f, l) = place_first(&mut h, q("0.012"), Tif::Ioc, plan_l1(), 0);
        h.fill(f, q("0.012"), p("60000.00"), 0);
        (h, l)
    };
    let (mut h, _) = setup(100);
    let hs = place_req(&mut h, 100);
    reject(&mut h, hs, 200);
    assert_eq!(retry_timers(&h)[0].deadline_tsc, ms(300), "OMS-189.1");
    for (row, why, at, want) in [
        (
            "OMS-189.2",
            HedgeGate::Risk(RejectBits::RATE_BACKOFF),
            450u64,
            450u64,
        ),
        ("OMS-189.3", HedgeGate::Risk(RejectBits::RATE_BACKOFF), 250, 250),
        ("OMS-189.3", HedgeGate::Risk(RejectBits::RATE), 201, 201),
        ("OMS-189.5", HedgeGate::Px(HedgePxErr::StaleQuote), 0, 300),
    ] {
        let (mut h, l) = setup(100);
        h.out.clear();
        h.o.hedge_gated(l, why, ms(at), ms(200), 1, &mut h.out);
        assert_eq!(retry_timers(&h)[0].deadline_tsc, ms(want), "{row}");
    }
    let (mut h, _) = setup(1);
    let hs = place_req(&mut h, 100);
    reject(&mut h, hs, 200);
    assert_eq!(retry_timers(&h)[0].deadline_tsc, ms(201), "OMS-189.4");
}

#[test]
fn oms_190_retry_naked_writeoff() {
    let (mut h, _f, l) = filled(plan_l1(), "0.012");
    let hs = place_req(&mut h, 100);
    reject(&mut h, hs, 200);
    h.o.test_set_net(A1, g1(), Y, q("-0.007"));
    h.retry(l, 300);
    let r = one_hedge(&h);
    assert_eq!(
        (r.role, r.qty, r.basis),
        (LinkRole::SecondRetry, q("0.005"), q("0.012"))
    );
    assert_eq!(h.link(l).hedge_req, q("0.005"));
    let n = h.notes(notes::HEDGE_NAKED_CAPPED);
    assert_eq!(&n[0].v[..4], &[u64::from(l), 1_200_000, 500_000, 500_000]);
}

#[test]
fn oms_191_retry_capped_to_zero_done() {
    let (mut h, _f, l) = filled(plan_l1(), "0.012");
    let hs = place_req(&mut h, 100);
    reject(&mut h, hs, 200);
    h.o.test_set_net(A1, g1(), Y, q("-0.012"));
    h.retry(l, 300);
    assert!(h.out.hedges.is_empty());
    assert_eq!((h.link(l).hedge_req, h.link(l).state), (Qty(0), LinkState::Done));
    assert_eq!(
        h.out.links.iter().filter(|u| u.state == LinkState::Done).count(),
        1
    );
    assert_eq!(h.n_notes(notes::HEDGE_NAKED_CAPPED), 1);
    assert_eq!(h.legs(A1, g1()).view.open_links, 0);
}

#[test]
fn oms_191_retry_after_first_tomb_expires_still_caps_naked() {
    let mut h = H::with(|c| {
        c.reuse_quarantine_ms = 1;
        c.tombstone_keep_ms = 1;
    });
    let (f, l) = place_first(&mut h, q("0.012"), Tif::Ioc, plan_l1(), 0);
    h.fill(f, q("0.012"), p("60000.00"), 0);
    let hs = place_req(&mut h, 100);
    reject(&mut h, hs, 200);
    h.o.test_set_net(A1, g1(), Y, q("-0.012"));
    h.sweep(201);
    h.sweep(203);
    assert!(h.o.tomb(f.venue, f.account, f.slot, f.r#gen).is_none());
    h.retry(l, 300);
    assert!(h.out.hedges.is_empty());
    assert_eq!(h.link(l).state, LinkState::Done);
    assert_eq!(h.n_notes(notes::HEDGE_NAKED_CAPPED), 1);
}

#[test]
fn oms_192_reduce_only_inherit() {
    for (row, ro, on_fail, retry, want) in [
        ("OMS-192.1", 0u8, OnFail::Retry, false, false),
        ("OMS-192.2", 0, OnFail::Retry, true, false),
        ("OMS-192.3", 1, OnFail::Retry, false, true),
        ("OMS-192.4", 1, OnFail::Retry, true, true),
        ("OMS-192.5", 0, OnFail::Reverse, true, true),
    ] {
        let mut plan = plan_l1();
        plan.reduce_only = ro;
        plan.on_fail = on_fail;
        let (mut h, _f, l) = filled(plan, "0.012");
        if !retry {
            assert_eq!(one_hedge(&h).reduce_only, want, "{row}");
            continue;
        }
        let hs = place_req(&mut h, 100);
        reject(&mut h, hs, 200);
        if on_fail == OnFail::Retry {
            h.retry(l, 300);
        }
        let r = one_hedge(&h);
        assert_eq!(r.reduce_only, want, "{row}");
        if on_fail == OnFail::Reverse {
            assert_eq!(r.role, LinkRole::Reverse, "{row}");
        }
    }
}

#[test]
fn oms_193_gated_not_counted() {
    for (row, why, note) in [
        (
            "OMS-193.1",
            HedgeGate::Px(HedgePxErr::StaleQuote),
            Some(notes::HEDGE_PX_STALE),
        ),
        (
            "OMS-193.2",
            HedgeGate::Px(HedgePxErr::BeyondCross),
            Some(notes::HEDGE_PX_GATED),
        ),
        ("OMS-193.3", HedgeGate::Risk(RejectBits::KILLED), None),
        ("OMS-193.4", HedgeGate::Risk(RejectBits::STALE_MID), None),
    ] {
        let (mut h, _f, l) = filled(plan_l1(), "0.012");
        h.out.clear();
        h.o.hedge_gated(l, why, Tsc(0), ms(500), 1, &mut h.out);
        assert_eq!(h.link(l).retries, 0, "{row}");
        assert_eq!(retry_timers(&h)[0].deadline_tsc, ms(600), "{row}");
        match note {
            Some(c) => assert_eq!(h.notes(c)[0].level, NoteLevel::P1, "{row}"),
            None => assert!(
                h.n_notes(notes::HEDGE_PX_STALE) + h.n_notes(notes::HEDGE_PX_GATED) == 0,
                "{row}"
            ),
        }
    }
}

#[test]
fn oms_194_deadline_boundary() {
    for (row, dl, now, broken) in [
        ("OMS-194.1", 3000u32, 2999u64, false),
        ("OMS-194.2", 3000, 3000, false),
        ("OMS-194.3", 3000, 3001, true),
        ("OMS-194.4", 500, 501, true),
    ] {
        let mut h = H::with(|c| c.hedge_deadline_ms = dl);
        let (f, l) = place_first(&mut h, q("0.012"), Tif::Ioc, plan_l1(), 0);
        h.fill(f, q("0.012"), p("60000.00"), 0);
        let hs = place_req(&mut h, 0);
        reject(&mut h, hs, now);
        if broken {
            assert_eq!(h.link(l).state, LinkState::Broken, "{row}");
            assert!(retry_timers(&h).is_empty(), "{row}");
            assert_eq!(
                h.out
                    .links
                    .iter()
                    .find(|u| u.state == LinkState::Broken)
                    .unwrap()
                    .broken_reason,
                LinkBroken::Deadline
            );
        } else {
            assert_eq!(h.link(l).state, LinkState::Hedging, "{row}");
            assert_eq!(retry_timers(&h).len(), 1, "{row}");
        }
    }
}

#[test]
fn oms_195_retry_cap() {
    for (row, max, retries) in [
        ("OMS-195.1", 3u8, 3usize),
        ("OMS-195.2", 1, 1),
        ("OMS-195.3", 0, 0),
    ] {
        let mut h = H::with(|c| {
            c.hedge_retry_max = max;
            c.hedge_deadline_ms = 60_000;
        });
        let (f, l) = place_first(&mut h, q("0.012"), Tif::Ioc, plan_l1(), 0);
        h.fill(f, q("0.012"), p("60000.00"), 0);
        let mut n_retry = 0;
        let mut hs = place_req(&mut h, 0);
        let mut t = 10;
        loop {
            reject(&mut h, hs, t);
            if h.link(l).state == LinkState::Broken {
                break;
            }
            h.retry(l, t + 100);
            assert_eq!(one_hedge(&h).role, LinkRole::SecondRetry);
            n_retry += 1;
            hs = place_req(&mut h, t + 100);
            t += 200;
        }
        assert_eq!(n_retry, retries, "{row}");
        assert_eq!(
            h.out
                .links
                .iter()
                .find(|u| u.state == LinkState::Broken)
                .unwrap()
                .broken_reason,
            LinkBroken::RetryExhausted,
            "{row}"
        );
    }
}

#[test]
fn oms_196_ioc_partial_retry() {
    let (mut h, _f, l) = filled(plan_l1(), "0.012");
    let hs = place_req(&mut h, 100);
    h.fill(hs, q("0.005"), p("59990.0"), 150);
    h.what(hs, OrderEventKind::Expired, 200);
    let e = h.link(l);
    assert_eq!(
        (e.hedge_filled, e.hedge_open, e.owed(), e.retries),
        (q("0.005"), Qty(0), q("0.007"), 1)
    );
    h.retry(l, 300);
    let r = one_hedge(&h);
    assert_eq!(
        (r.role, r.qty, r.basis, r.hedge_cap_qty),
        (LinkRole::SecondRetry, q("0.007"), q("0.007"), q("0.008"))
    );
}

#[test]
fn oms_197_on_fail_hold() {
    let mut plan = plan_l1();
    plan.on_fail = OnFail::Hold;
    let (mut h, _f, l) = filled(plan, "0.012");
    let hs = place_req(&mut h, 100);
    reject(&mut h, hs, 200);
    assert!(h.out.hedges.is_empty() && retry_timers(&h).is_empty());
    assert_eq!(h.link(l).state, LinkState::Done);
    let u: Vec<_> = h
        .out
        .links
        .iter()
        .filter(|u| u.state == LinkState::Done)
        .collect();
    assert_eq!((u.len(), u[0].owed), (1, q("0.012")));
    assert_eq!(h.legs(A1, g1()).view.leg[0].net, q("0.012"));
}

#[test]
fn oms_198_reverse() {
    let mut plan = plan_l1();
    plan.on_fail = OnFail::Reverse;
    let (mut h, f, l) = filled(plan, "0.012");
    let hs = place_req(&mut h, 100);
    reject(&mut h, hs, 200);
    let r = one_hedge(&h);
    assert_eq!(
        (r.role, r.venue, r.account, r.inst, r.side, r.reduce_only),
        (LinkRole::Reverse, VS, A1, X, hft_types::Side::Sell, true)
    );
    assert_eq!(
        (r.qty, r.basis, r.hedge_cap_qty, r.ref_px),
        (q("0.012"), q("0.012"), q("0.01201"), hft_types::Px(0))
    );
    let e = h.link(l);
    assert_eq!(
        (e.hedge_req, e.owed(), e.state),
        (Qty(0), Qty(0), LinkState::Hedging)
    );
    let rv = place_hedge(&mut h, &r, 210).unwrap();
    h.fill(rv, q("0.012"), p("60000.00"), 220);
    assert_eq!(h.link(l).first_net, Qty(0));
    assert_eq!(h.link(l).state, LinkState::Done);
    assert_eq!(
        h.out.links.iter().filter(|u| u.state == LinkState::Done).count(),
        1
    );
    let _ = f;
}

#[test]
fn oms_199_reverse_retry_exhausted() {
    let mut h = H::with(|c| c.hedge_deadline_ms = 60_000);
    let mut plan = plan_l1();
    plan.on_fail = OnFail::Reverse;
    let (f, l) = place_first(&mut h, q("0.012"), Tif::Ioc, plan, 0);
    h.fill(f, q("0.012"), p("60000.00"), 0);
    let hs = place_req(&mut h, 100);
    reject(&mut h, hs, 200);
    assert_eq!(h.link(l).retries, 1);
    let mut n_rev = 1;
    let rv = place_req(&mut h, 210);
    h.fill(rv, q("0.005"), p("60000.00"), 220);
    h.what(rv, OrderEventKind::Expired, 230);
    assert_eq!((h.link(l).retries, h.link(l).first_net), (2, q("0.007")));
    h.retry(l, 400);
    let r = one_hedge(&h);
    assert_eq!((r.role, r.qty), (LinkRole::Reverse, q("0.007")));
    n_rev += 1;
    let rv = place_req(&mut h, 400);
    reject(&mut h, rv, 410);
    assert_eq!(h.link(l).retries, 3);
    h.retry(l, 600);
    n_rev += usize::from(!h.out.hedges.is_empty());
    let rv = place_req(&mut h, 600);
    reject(&mut h, rv, 610);
    assert_eq!(h.link(l).retries, 4);
    assert_eq!(h.link(l).state, LinkState::Broken);
    assert_eq!(
        h.out
            .links
            .iter()
            .find(|u| u.state == LinkState::Broken)
            .unwrap()
            .broken_reason,
        LinkBroken::ReverseFailed
    );
    assert_eq!(h.n_notes(notes::HEDGE_BROKEN), 1);
    assert_eq!(n_rev, 3);
}

#[test]
fn oms_200_hedge_vetoed() {
    let mut h = H::new();
    let (f, l) = place_first(&mut h, q("0.02"), Tif::Gtc, plan_l1(), 0);
    h.ack(f, 1, 0);
    h.fill(f, q("0.012"), p("60000.00"), 1);
    h.out.clear();
    h.o.hedge_vetoed(l, ms(2), 1, &mut h.out);
    assert_eq!(h.link(l).state, LinkState::Broken);
    assert_eq!(h.out.links[0].broken_reason, LinkBroken::HedgeLimit);
    assert_eq!(h.notes(notes::HEDGE_BROKEN)[0].level, NoteLevel::P0);
    assert!(retry_timers(&h).is_empty());
    h.fill(f, q("0.015"), p("60000.00"), 3);
    let mut any = !h.out.hedges.is_empty() || !h.decisions(OrderAction::New).is_empty();
    h.retry(l, 4);
    any |= !h.out.hedges.is_empty();
    h.sweep(5);
    any |= !h.out.hedges.is_empty() || !h.decisions(OrderAction::New).is_empty();
    assert!(!any);
    assert_eq!(h.legs(A1, g1()).view.leg[0].net, q("0.015"));
}

#[test]
fn oms_201_done_conditions() {
    // .1
    let (mut h, _f, l) = filled(plan_l1(), "0.012");
    let hs = place_req(&mut h, 1);
    h.fill(hs, q("0.012"), p("59990.0"), 2);
    assert_eq!(h.link(l).state, LinkState::Done, "OMS-201.1");
    assert_eq!(
        h.out.links.iter().filter(|u| u.state == LinkState::Done).count(),
        1
    );
    // .2
    let mut h = H::new();
    let (f, l) = place_first(&mut h, q("0.012"), Tif::Ioc, plan_l1(), 0);
    h.what(f, OrderEventKind::CancelAck, 1);
    assert_eq!(h.link(l).state, LinkState::Done, "OMS-201.2");
    // .3
    let mut h = H::new();
    let (f, l) = place_first(&mut h, q("0.012"), Tif::Ioc, plan_l1(), 0);
    h.fill(f, q("0.004"), p("60000.00"), 1);
    let hs = place_req(&mut h, 1);
    h.what(f, OrderEventKind::CancelAck, 2);
    h.fill(hs, q("0.004"), p("59990.0"), 3);
    assert_eq!(h.link(l).state, LinkState::Done, "OMS-201.3");
    // .4
    let (mut h, _f, l) = filled(plan_l1(), "0.012");
    let hs = place_req(&mut h, 1);
    reject(&mut h, hs, 2);
    assert_eq!(h.link(l).state, LinkState::Hedging, "OMS-201.4");
    // .5
    let mut h = H::new();
    let (f, l) = place_first(&mut h, q("0.012"), Tif::Gtc, plan_l1(), 0);
    h.ack(f, 1, 0);
    h.fill(f, q("0.004"), p("60000.00"), 1);
    let hs = place_req(&mut h, 1);
    h.fill(hs, q("0.004"), p("59990.0"), 2);
    assert_eq!(h.link(l).state, LinkState::Hedging, "OMS-201.5");
}

#[test]
fn oms_202_broken_link_update() {
    let (mut h, f, l) = filled(plan_l1(), "0.012");
    let hs = place_req(&mut h, 0);
    reject(&mut h, hs, 3001);
    let u: Vec<_> = h
        .out
        .links
        .iter()
        .filter(|u| u.state == LinkState::Broken)
        .copied()
        .collect();
    assert_eq!(u.len(), 1);
    let u = u[0];
    assert_eq!(
        (
            u.link_id,
            u.account,
            u.group,
            u.first,
            u.broken_reason as u8,
            u.flags
        ),
        (l, A1, g1(), f, 2, 0)
    );
    assert_eq!(
        (
            u.first_net,
            u.hedge_filled,
            u.owed,
            u.tolerance,
            u.residual_policy
        ),
        (
            q("0.012"),
            Qty(0),
            q("0.012"),
            r("0.02"),
            hft_strategy_sdk::ResidualPolicy::Defer
        )
    );
    let n = h.notes(notes::HEDGE_BROKEN);
    assert_eq!((n[0].level, n[0].group), (NoteLevel::P0, g1()));
}

#[test]
fn oms_203_residual_across_links() {
    // L1's first leg 0.0123 leaves the residual 0.0003 after its 0.012 hedge.
    let (mut h, _f, l) = filled(plan_l1(), "0.0123");
    let hs = place_req(&mut h, 1);
    h.fill(hs, q("0.012"), p("59990.0"), 2);
    assert_eq!(h.link(l).state, LinkState::Done);
    assert_eq!(h.legs(A1, g1()).view.leg[1].residual, q("0.0003"));
    let (f2, l2) = place_first(&mut h, q("0.0007"), Tif::Ioc, plan_l1(), 3);
    h.fill(f2, q("0.0007"), p("60000.00"), 4);
    let r = one_hedge(&h);
    assert_eq!((r.link_id, r.qty), (l2, q("0.001")));
    assert_eq!(h.legs(A1, g1()).view.leg[1].residual, Qty(0));
}

#[test]
fn oms_205_no_auto_fix() {
    let mut h = H::with(|c| c.hedge_retry_max = 0);
    let (f, l) = place_first(&mut h, q("0.012"), Tif::Ioc, plan_l1(), 0);
    h.fill(f, q("0.012"), p("60000.00"), 0);
    let hs = place_req(&mut h, 1);
    reject(&mut h, hs, 2);
    assert_eq!(h.link(l).state, LinkState::Broken);
    let mut outs = Vec::new();
    h.sweep(10);
    outs.push(h.out.clone());
    h.out.clear();
    h.o.external_adjust(A1, VX, g1(), Y, q("-0.004"), 0, ExternalCause::Adl, &mut h.out);
    outs.push(h.out.clone());
    h.o.net_adjust(A1, g1(), X, q("-0.00001"));
    h.retry(l, 20);
    outs.push(h.out.clone());
    for o in outs {
        assert!(o.hedges.is_empty());
        assert!(!o.decisions.iter().any(|d| d.action == OrderAction::New));
        assert!(o.hedges.iter().all(|r| r.role != LinkRole::HedgeFix));
    }
}

#[test]
fn oms_206_stale_retry_timer() {
    let (mut h, _f, l) = filled(plan_l1(), "0.012");
    let hs = place_req(&mut h, 1);
    h.fill(hs, q("0.012"), p("59990.0"), 2);
    assert_eq!(h.link(l).state, LinkState::Done);
    h.retry(l, 3);
    assert!(h.out.is_empty(), "OMS-206.1");
    let (mut h, _f, l) = filled(plan_l1(), "0.012");
    h.out.clear();
    h.o.hedge_vetoed(l, ms(1), 1, &mut h.out);
    h.retry(l, 3);
    assert!(h.out.is_empty(), "OMS-206.2");
    let (mut h, _f, l) = filled(plan_l1(), "0.012");
    let _hs = place_req(&mut h, 1);
    assert_eq!((h.link(l).owed(), h.link(l).hedge_open), (Qty(0), q("0.012")));
    h.retry(l, 3);
    assert!(h.out.is_empty(), "OMS-206.3");
    h.retry(999, 3);
    assert!(h.out.is_empty(), "OMS-206.4");
}

#[test]
fn oms_235_pair_lot_full_fill() {
    let (h, _f, _l) = filled(plan_l1(), "0.012");
    assert_eq!(hft_types::pair_lot(1000, 100_000, 1, 1), Some(q("0.001")));
    assert_eq!(one_hedge(&h).qty, q("0.012"));
    assert_eq!(h.legs(A1, g1()).view.leg[1].residual, Qty(0));
}

#[test]
fn oms_236_unaligned_first_leg_accepted() {
    let mut h = H::new();
    let (f, _l) = place_first(&mut h, q("0.01234"), Tif::Ioc, plan_l1(), 0);
    assert_eq!(h.decisions(OrderAction::New)[0].qty, q("0.01234"));
    h.fill(f, q("0.01234"), p("60000.00"), 1);
    assert_eq!(one_hedge(&h).qty, q("0.012"));
    assert_eq!(h.legs(A1, g1()).view.leg[1].residual, q("0.00034"));
}

#[test]
fn oms_240_link_peer_and_ready() {
    let mut h = H::new();
    let (_f, l) = place_first(&mut h, q("0.012"), Tif::Ioc, plan_l1(), 0);
    let t = h.place_ok(&sell_y(), 0);
    h.what(t, OrderEventKind::OutcomeUnknown, 1);
    assert_eq!(h.o.link_peer(l), Some((VX, A1)));
    assert_eq!(h.o.link_peer(999), None);
    assert!(h.o.venue_ready(VS, A1));
    assert!(!h.o.venue_ready(VX, A1));
}

#[test]
fn oms_457_link_valid() {
    let base = link_l1(plan_l1());
    let with = |f: &dyn Fn(&mut LegLink)| {
        let mut l = base;
        f(&mut l);
        link_valid(&l)
    };
    assert!(with(&|_| {}), "OMS-457.1");
    assert!(!with(&|l| l.role = LinkRole::Second), "OMS-457.2");
    for r in [LinkRole::SecondRetry, LinkRole::Reverse, LinkRole::HedgeFix] {
        assert!(!with(&|l| l.role = r), "OMS-457.3");
    }
    assert!(!with(&|l| l.hedge.kind = HedgeKind::None), "OMS-457.4");
    assert!(
        with(&|l| l.hedge.on_fail = OnFail::Reverse) && with(&|l| l.hedge.on_fail = OnFail::Hold),
        "OMS-457.5"
    );
    assert!(!with(&|l| l.hedge.tif = Tif::Fok), "OMS-457.6");
    assert!(!with(&|l| l.hedge.tif = Tif::Gtc), "OMS-457.7");
    assert!(!with(&|l| l.hedge.kind = HedgeKind::Simultaneous), "OMS-457.8");
    assert!(
        with(&|l| {
            l.hedge.kind = HedgeKind::Simultaneous;
            l.hedge.on_fail = OnFail::Reverse;
            l.hedge.tif = Tif::Fok;
        }),
        "OMS-457.9"
    );
    assert!(
        with(&|l| {
            l.hedge.kind = HedgeKind::Simultaneous;
            l.hedge.on_fail = OnFail::Hold;
        }),
        "OMS-457.10"
    );
    assert!(!with(&|l| l.hedge.ratio_num = 0), "OMS-457.11");
    assert!(!with(&|l| l.hedge.ratio_den = 0), "OMS-457.12");
}

#[test]
fn oms_458_hard_second_leg_gated() {
    let (mut h, _f, l) = filled(plan_l1(), "0.012");
    let mut t = 1u64;
    loop {
        h.out.clear();
        h.o.hedge_gated(l, HedgeGate::Risk(RejectBits::HARD), Tsc(0), ms(t), 1, &mut h.out);
        assert!(h.out.decisions.is_empty());
        if t == 1 {
            assert_eq!(retry_timers(&h)[0].deadline_tsc, ms(101));
        }
        assert_eq!(h.link(l).retries, 0);
        if h.link(l).state == LinkState::Broken {
            assert_eq!(t, 3001);
            break;
        }
        assert!(t < 3001);
        let at = retry_timers(&h)[0].deadline_tsc.0 / MS;
        h.retry(l, at);
        if h.link(l).state == LinkState::Broken {
            // The judgement at t = 3001 happens in the retry itself (before a new request).
            assert_eq!(at, 3001);
            break;
        }
        t = at;
    }
    assert_eq!(
        h.out
            .links
            .iter()
            .find(|u| u.state == LinkState::Broken)
            .unwrap()
            .broken_reason,
        LinkBroken::Deadline
    );
    assert_eq!(h.notes(notes::HEDGE_BROKEN)[0].level, NoteLevel::P0);
}

#[test]
fn oms_459_deadline_from_first_fill() {
    let setup = || {
        let mut h = H::with(|c| c.hedge_retry_max = 10);
        let (f, l) = place_first(&mut h, q("0.02"), Tif::Gtc, plan_l1(), 0);
        h.ack(f, 1, 0);
        (h, f, l)
    };
    // .1 / .2 / .4: hedged at t = 100, new fill at t = 5000.
    let base = || {
        let (mut h, f, l) = setup();
        h.fill(f, q("0.005"), p("60000.00"), 0);
        let hs = place_req(&mut h, 0);
        h.fill(hs, q("0.005"), p("59990.0"), 100);
        assert_eq!(h.link(l).first_fill_tsc, Tsc(0));
        h.fill(f, q("0.010"), p("60000.00"), 5000);
        assert_eq!(h.link(l).first_fill_tsc, ms(5000));
        let hs2 = place_req(&mut h, 5000);
        (h, l, hs2)
    };
    let (mut h, l, hs2) = base();
    h.fill(hs2, q("0.005"), p("59990.0"), 5100);
    assert_eq!(h.link(l).state, LinkState::Hedging, "OMS-459.1");
    let (mut h, l, hs2) = base();
    reject(&mut h, hs2, 5100);
    assert_eq!(
        (h.link(l).state, h.link(l).retries, retry_timers(&h).len()),
        (LinkState::Hedging, 1, 1),
        "OMS-459.2"
    );
    let (mut h, l, hs2) = base();
    reject(&mut h, hs2, 5100);
    for (t, broken) in [(7999u64, false), (8000, false), (8001, true)] {
        h.retry(l, t - 1);
        let r = place_req(&mut h, t - 1);
        reject(&mut h, r, t);
        assert_eq!(h.link(l).state == LinkState::Broken, broken, "OMS-459.4 t={t}");
    }
    assert_eq!(h.n_notes(notes::HEDGE_BROKEN), 1);
    // .5: second leg rejected at 100, another first-leg fill at 2000 does not move the start.
    let (mut h, f, l) = setup();
    h.fill(f, q("0.005"), p("60000.00"), 0);
    let hs = place_req(&mut h, 0);
    reject(&mut h, hs, 100);
    h.fill(f, q("0.010"), p("60000.00"), 2000);
    assert!(h.link(l).first_fill_tsc.0 < MS, "OMS-459.5");
    let mut hs = place_req(&mut h, 2000);
    reject(&mut h, hs, 2100);
    loop {
        let at = retry_timers(&h).first().map(|t| t.deadline_tsc.0 / MS);
        let Some(at) = at else { break };
        if at > 3001 {
            break;
        }
        h.retry(l, at);
        hs = place_req(&mut h, at);
        reject(&mut h, hs, at);
    }
    h.retry(l, 3000);
    if !h.out.hedges.is_empty() {
        hs = place_req(&mut h, 3000);
    }
    reject(&mut h, hs, 3001);
    assert_eq!(h.link(l).state, LinkState::Broken, "OMS-459.5");
    // .6: the retry fills 0.010 at 2500: unhedged back to 0.
    let (mut h, f, l) = setup();
    h.fill(f, q("0.005"), p("60000.00"), 0);
    let hs = place_req(&mut h, 0);
    reject(&mut h, hs, 100);
    h.fill(f, q("0.010"), p("60000.00"), 2000);
    let hs2 = place_req(&mut h, 2000);
    h.retry(l, 200);
    let r = place_req(&mut h, 200);
    h.fill(r, q("0.005"), p("59990.0"), 2500);
    h.fill(hs2, q("0.005"), p("59990.0"), 2500);
    assert_eq!(h.link(l).first_fill_tsc, Tsc(0), "OMS-459.6");
    h.sweep(3001);
    assert_ne!(h.link(l).state, LinkState::Broken, "OMS-459.6");
    // .3: nothing owed, nothing open.
    let (mut h, f, l) = setup();
    h.fill(f, q("0.005"), p("60000.00"), 0);
    let hs = place_req(&mut h, 0);
    h.fill(hs, q("0.005"), p("59990.0"), 100);
    h.sweep(10_000);
    assert_ne!(h.link(l).state, LinkState::Broken, "OMS-459.3");
}
