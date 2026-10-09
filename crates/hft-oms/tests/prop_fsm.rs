#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods
)] // allow-list: test code
#![allow(clippy::type_complexity, clippy::too_many_arguments)]
use crate::common;
use crate::model;

use common::*;
use hft_event::{ConnRole, ConnStateKind, GwOp, OrderEventKind, SendStatus};
use hft_oms::*;
use hft_strategy_sdk::{OrderRef, SnapshotWriter};
use hft_types::{GROUP_NONE, Qty, Side, Tif, VenueId};
use model::{Order, Rep};
use proptest::prelude::*;

const QTY: i64 = 1_000_000;

#[derive(Copy, Clone, Debug)]
enum Op {
    Rep(Rep),
    Dup,
    Cancel,
    Amend(i64),
    Send(u8),
    ConnDown,
    ConnUp,
    Sweep,
    UnknownQuery,
}

fn cum() -> impl Strategy<Value = i64> {
    (0i64..=4).prop_map(|k| k * QTY / 4)
}

fn op() -> impl Strategy<Value = Op> {
    prop_oneof![
        Just(Op::Rep(Rep::Ack)),
        Just(Op::Rep(Rep::Reject)),
        cum().prop_map(|c| Op::Rep(Rep::Fill(c))),
        cum().prop_map(|c| Op::Rep(Rep::CancelAck(c))),
        cum().prop_map(|c| Op::Rep(Rep::Expired(c))),
        (any::<bool>(), cum()).prop_map(|(g, c)| Op::Rep(Rep::CancelReject { gone: g, cum: c })),
        (cum(), prop_oneof![Just(0i64), Just(800_000), Just(QTY)])
            .prop_map(|(c, q)| Op::Rep(Rep::AmendAck { cum: c, qty: q })),
        (any::<bool>(), cum()).prop_map(|(g, c)| Op::Rep(Rep::AmendReject { gone: g, cum: c })),
        Just(Op::Rep(Rep::OutcomeUnknown)),
        Just(Op::Rep(Rep::FeeOnly)),
        (1u8..=7, cum(), prop_oneof![Just(0i64), Just(QTY)]).prop_map(|(s, c, q)| Op::Rep(Rep::Query {
            status: s,
            cum: c,
            qty: q
        })),
        Just(Op::Dup),
        Just(Op::Cancel),
        prop_oneof![Just(800_000i64), Just(QTY)].prop_map(Op::Amend),
        (0u8..=2).prop_map(Op::Send),
        Just(Op::ConnDown),
        Just(Op::ConnUp),
    ]
}

fn op_with_maint() -> impl Strategy<Value = Op> {
    prop_oneof![9 => op(), 1 => Just(Op::Sweep), 1 => Just(Op::UnknownQuery)]
}

#[derive(Clone, Debug)]
struct Case {
    orders: Vec<(u8, u8)>,
    ops: Vec<(usize, Op)>,
    quarantine0: bool,
}

fn case(with_maint: bool) -> impl Strategy<Value = Case> {
    (1usize..=64, any::<bool>()).prop_flat_map(move |(n, q0)| {
        let ops = if with_maint {
            prop::collection::vec((0..n, op_with_maint()), 0..=n * 8).boxed()
        } else {
            prop::collection::vec((0..n, op()), 0..=n * 8).boxed()
        };
        (prop::collection::vec((0u8..4, 0u8..3), n), ops).prop_map(move |(orders, ops)| Case {
            orders,
            ops,
            quarantine0: q0,
        })
    })
}

fn venue(i: u8) -> VenueId {
    [VS, VX, VG][usize::from(i)]
}

fn tif(i: u8) -> Tif {
    [Tif::Gtc, Tif::Ioc, Tif::PostOnly, Tif::Fok][usize::from(i)]
}

fn inst(v: VenueId) -> hft_types::InstId {
    if v == VS {
        X
    } else if v == VX {
        Y
    } else {
        Z
    }
}

/// Builds the OrderEvent of a model report.
fn event(h: &H, r: OrderRef, rep: Rep) -> hft_event::OrderEvent {
    let k = |w| h.oe(r, w);
    match rep {
        Rep::Ack => {
            let mut e = k(OrderEventKind::Ack);
            e.exch_order_id = u64::from(r.slot) * 100 + u64::from(r.r#gen);
            e
        }
        Rep::Reject => k(OrderEventKind::Reject),
        Rep::Fill(c) => {
            let mut e = k(if c >= h.slot(r).qty.0 {
                OrderEventKind::Fill
            } else {
                OrderEventKind::PartialFill
            });
            e.cum_filled = Qty(c);
            e.avg_px = p("600.00");
            e.fee_asset = USDT;
            e
        }
        Rep::CancelAck(c) | Rep::Expired(c) => {
            let mut e = k(if matches!(rep, Rep::Expired(_)) {
                OrderEventKind::Expired
            } else {
                OrderEventKind::CancelAck
            });
            e.cum_filled = Qty(c);
            e.avg_px = p("600.00");
            e
        }
        Rep::CancelReject { gone, cum } => {
            let mut e = k(OrderEventKind::CancelReject);
            e.detail = u8::from(gone);
            e.cum_filled = Qty(cum);
            e.avg_px = p("600.00");
            e
        }
        Rep::AmendAck { cum, qty } => {
            let mut e = k(OrderEventKind::AmendAck);
            e.cum_filled = Qty(cum);
            e.avg_px = p("600.00");
            e.qty = Qty(qty);
            e
        }
        Rep::AmendReject { gone, cum } => {
            let mut e = k(OrderEventKind::AmendReject);
            e.detail = u8::from(gone);
            e.cum_filled = Qty(cum);
            e.avg_px = p("600.00");
            e
        }
        Rep::OutcomeUnknown => k(OrderEventKind::OutcomeUnknown),
        Rep::FeeOnly => {
            let mut e = k(OrderEventKind::FeeOnly);
            e.fee = Qty(10);
            e.fee_asset = USDT;
            e
        }
        Rep::Query { status, cum, qty } => {
            let mut e = k(OrderEventKind::QueryResult);
            e.detail = status;
            e.cum_filled = Qty(cum);
            e.avg_px = p("600.00");
            e.qty = Qty(qty);
            e
        }
    }
}

struct Run {
    h: H,
    refs: Vec<OrderRef>,
    model: Vec<Order>,
    last: Vec<Option<Rep>>,
    outs: Vec<String>,
    /// Slots that reported `cum_filled > qty` (`OVERFILL`).
    overfilled: Vec<(VenueId, u32, u32)>,
}

fn start(c: &Case, record: bool) -> Run {
    let mut h = H::with(|cfg| {
        if c.quarantine0 {
            cfg.reuse_quarantine_ms = 0;
        }
    });
    let mut refs = Vec::new();
    for (t, v) in &c.orders {
        let v = venue(*v);
        let it = intent(
            v,
            A1,
            inst(v),
            Side::Buy,
            tif(*t),
            p("600.00"),
            Qty(QTY),
            GROUP_NONE,
        );
        refs.push(h.place_ok(&it, 0));
    }
    let n = refs.len();
    Run {
        h,
        refs,
        model: vec![Order::new(QTY); n],
        last: vec![None; n],
        outs: if record { Vec::new() } else { Vec::with_capacity(0) },
        overfilled: Vec::new(),
    }
}

/// Applies one op to the OMS and the model; `t` ms.
fn step(run: &mut Run, i: usize, op: Op, t: u64, record: bool) {
    let r = run.refs[i];
    let v = r.venue;
    let h = &mut run.h;
    match op {
        Op::Rep(rep) => {
            let e = event(h, r, rep);
            h.ev(&e, t);
            run.model[i].rep(rep);
            run.last[i] = Some(rep);
        }
        Op::Dup => {
            if let Some(rep) = run.last[i] {
                let e = event(h, r, rep);
                h.ev(&e, t);
                run.model[i].rep(rep);
            }
        }
        Op::Cancel => {
            let gone = h.slot(r).state == OrderState::Free;
            let ok = h.cancel(r, t).is_ok();
            let m = run.model[i].cancel();
            assert!(gone || ok == m, "cancel allowed");
        }
        Op::Amend(qn) => {
            let gone = h.slot(r).state == OrderState::Free;
            let px = h.slot(r).px;
            let ok = h.amend(r, px, Qty(qn), t).is_ok();
            let m = run.model[i].amend(qn, v != VS);
            assert!(gone || ok == m, "amend allowed");
        }
        Op::Send(k) => {
            let st = [
                SendStatus::Written,
                SendStatus::WriteError,
                SendStatus::RateLimited,
            ][usize::from(k)];
            h.send(v, A1, GwOp::Place, r.slot, r.r#gen, st, t, t);
            run.model[i].send_place(k);
        }
        Op::ConnDown => {
            h.conn(v, A1, ConnRole::Order, ConnStateKind::Down, t);
            for (j, rr) in run.refs.iter().enumerate() {
                if rr.venue == v {
                    run.model[j].conn_down();
                }
            }
        }
        Op::ConnUp => h.conn(v, A1, ConnRole::Order, ConnStateKind::Up, t),
        Op::Sweep => h.sweep(t),
        Op::UnknownQuery => h.uq(r, t),
    }
    for f in run.h.out.fills.iter() {
        if f.flags & fill_flags::OVERFILL != 0 {
            run.overfilled.push((f.slot.venue, f.slot.slot, f.slot.r#gen));
        }
    }
    if record {
        run.outs.push(format!("{:?}", run.h.out));
    }
}

fn check_model(run: &Run) {
    for (i, r) in run.refs.iter().enumerate() {
        let sl = run.h.slot(*r);
        if sl.r#gen != r.r#gen || sl.state == OrderState::Free {
            continue;
        }
        assert_eq!(sl.state as u8, run.model[i].st.code(), "order {i} state");
        assert_eq!(sl.filled.0, run.model[i].filled, "order {i} filled");
        assert_eq!(sl.qty.0, run.model[i].qty, "order {i} qty");
    }
}

/// README "数据布局" invariants, recomputed from the slots through the public views.
fn check_invariants(h: &H, over: &[(VenueId, u32, u32)]) {
    use std::collections::BTreeMap;
    let mut pos: BTreeMap<(u8, u32), (i64, i64, i64, i64)> = BTreeMap::new();
    for (v, a) in [(VS, A1), (VX, A1), (VG, A1), (VS, A2), (VS, S1)] {
        let tv = h.o.table_view(v, a).unwrap();
        let (mut free, mut unk) = (0u32, 0u32);
        for s in 1..=tv.n_slots {
            let (sl, ex, _) = h.o.raw_slot(v, a, s).unwrap();
            match sl.state {
                OrderState::Free => free += 1,
                OrderState::Unknown => unk += 1,
                _ => {}
            }
            let over_ok = over.contains(&(v, s, sl.r#gen));
            assert!(
                sl.filled.0 <= sl.qty.0 || sl.state == OrderState::Free || over_ok,
                "filled ≤ qty"
            );
            if sl.state != OrderState::Free {
                assert_eq!(ex.net_filled, sl.filled, "net_filled (quote-asset fees)");
            }
            if sl.state.open() {
                let rem = (sl.qty.0 - sl.filled.0).max(0);
                let imm = matches!(sl.tif, Tif::Ioc | Tif::Fok)
                    || sl.sflags.0 & SlotFlags::ACKED == 0
                    || sl.state == OrderState::Unknown;
                let e = pos.entry((a.0, sl.inst.0)).or_default();
                if sl.side == Side::Buy {
                    e.0 += rem;
                    if imm {
                        e.2 += rem;
                    }
                } else {
                    e.1 += rem;
                    if imm {
                        e.3 += rem;
                    }
                }
            }
        }
        assert_eq!(free, tv.free_len, "Free ⇔ in the free list");
        assert_eq!(unk, tv.unknown_count, "unknown_count");
    }
    for a in [A1, A2, S1] {
        for inst in [X, Y, Z] {
            let w = pos.get(&(a.0, inst.0)).copied().unwrap_or_default();
            let ib = h.o.inflight().pos(a, inst);
            assert_eq!(
                (ib.open_buy.0, ib.open_sell.0, ib.imm_buy.0, ib.imm_sell.0),
                w,
                "in-flight of {a:?} {inst:?}"
            );
        }
    }
}

fn run_case(c: &Case, invariants: bool, record: bool) -> Run {
    let mut run = start(c, record);
    for (k, (i, op)) in c.ops.iter().enumerate() {
        step(&mut run, *i, *op, 1 + k as u64, record);
        if !invariants {
            check_model(&run);
        } else {
            check_invariants(&run.h, &run.overfilled);
        }
    }
    // Final round: the exchange's truth by QueryResult (OMS and model alike).
    let t = 1 + c.ops.len() as u64;
    run.h.conn(VS, A1, ConnRole::Order, ConnStateKind::Up, t);
    for i in 0..run.refs.len() {
        let truth = Rep::Query {
            status: 3,
            cum: QTY,
            qty: 0,
        };
        step(&mut run, i, Op::Rep(truth), t, record);
    }
    if !invariants {
        check_model(&run);
    } else {
        check_invariants(&run.h, &run.overfilled);
    }
    run
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(2_000))]

    #[test]
    fn oms_380_fsm_matches_model(c in case(false)) {
        let mut run = run_case(&c, false, false);
        // Previous-generation reports after reuse: booked through the tombstone or StaleGen,
        // never touching the current generation.
        run.h.sweep(1_000_000);
        let olds: Vec<OrderRef> = run.refs.clone();
        let fresh: Vec<OrderRef> = (0..olds.len().min(8))
            .filter_map(|_| run.h.place(&intent(VS, A1, X, Side::Buy, Tif::Gtc, p("600.00"), Qty(QTY), GROUP_NONE), 1_000_001).ok())
            .collect();
        let before: Vec<_> = fresh.iter().map(|r| (run.h.slot(*r).state, run.h.slot(*r).filled)).collect();
        for r in olds.iter().filter(|r| r.venue == VS) {
            let mut e = run.h.oe(*r, OrderEventKind::PartialFill);
            e.r#gen = r.r#gen;
            e.cum_filled = Qty(QTY + 1);
            e.exch_order_id = 77;
            e.detail = 1;
            run.h.ev(&e, 1_000_002);
        }
        let after: Vec<_> = fresh.iter().map(|r| (run.h.slot(*r).state, run.h.slot(*r).filled)).collect();
        prop_assert_eq!(before, after);
    }

    #[test]
    fn oms_381_layout_invariants(c in case(true)) {
        run_case(&c, true, false);
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(500))]

    #[test]
    fn oms_390_deterministic(c in case(true)) {
        let a = run_case(&c, true, true);
        let b = run_case(&c, true, true);
        prop_assert_eq!(&a.outs, &b.outs);
        let mut wa = SnapshotWriter::new(1 << 22);
        a.h.o.snapshot(&mut wa);
        let mut wb = SnapshotWriter::new(1 << 22);
        b.h.o.snapshot(&mut wb);
        prop_assert_eq!(wa.as_bytes(), wb.as_bytes());
    }
}
