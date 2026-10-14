//! Simultaneous admission, real-engine fault seeds and replay (SEQ-178 / EXE-502).
use crate::common::*;
use crate::scenarios::{decs, last_link};
use hft_event::{ConnRole, ConnStateKind, EventKind, GwOp, OrderAction, OrderDecision, OrderEventKind};
use hft_sequencer::{OutRing, driver::live::RingId};
use hft_strategy_sdk::{EventRef, HedgeKind, LegLinkOpt, LinkRole, OnFail, PxMode};
use hft_types::{Qty, RejectBits};

fn install(policy: OnFail) {
    set_hook(move |hx, ev, ctx| {
        if let EventRef::Bbo { inst, b, .. } = ev
            && *inst == A1
            && hx.s.strategy == S1
            && b.ask_px == px("50003")
        {
            let mut it = pair_first(hx, q("0.01"));
            let mut link = *it.link.get().unwrap();
            link.hedge.kind = HedgeKind::Simultaneous;
            link.hedge.on_fail = policy;
            link.hedge.px_mode = PxMode::Limit;
            link.hedge.limit_px = px("50010");
            it.link = LegLinkOpt::some(link);
            let _ = ctx.place(it);
        }
    });
}

fn fire(h: &mut H) -> (usize, Vec<OrderDecision>) {
    let m = h.mark();
    h.quote(A1, "50000", "50003");
    h.run();
    (
        m,
        decs(h, m)
            .into_iter()
            .filter(|d| d.action == OrderAction::New)
            .collect(),
    )
}

pub(super) fn check_simultaneous_atomic_risk_and_capacity() {
    reset_script();
    let mut h = b0().boot_self_contained();
    h.steady();
    h.cmd_confirmed(
        AC1,
        &hft_event::ControlCmd::Kill {
            scope: hft_event::CmdScope {
                strategy: hft_types::StrategyId(0),
                venue: B,
                account: AC1,
                _pad: 0,
                group: hft_types::GROUP_ALL,
                inst: hft_types::InstId(0),
                _pad2: 0,
            },
            level: hft_types::KillLevel::Venue,
            flatten: false,
            cancel_resting: true,
            hard: false,
        },
    );
    h.run();
    install(OnFail::Reverse);
    let out = h.io_ref().out.len();
    let (_, ds) = fire(&mut h);
    assert_eq!(ds.len(), 1);
    assert_eq!(ds[0].reject, RejectBits::KILLED.0);
    assert!(last_link(&h, AC1).is_none());
    assert!(h.io_ref().out[out..].iter().all(|(_, c)| c.op != GwOp::Place));
    assert!(h.verify().is_empty());
    for (group_cap, order_cap, bit, shadow) in [
        ("800", "10000", RejectBits::NOTIONAL_GROUP, false),
        ("50000", "500.05", RejectBits::NOTIONAL_ORDER, false),
        ("50000", "500.05", RejectBits::NOTIONAL_ORDER, true),
        ("50000", "10000", RejectBits(0), true),
    ] {
        reset_script();
        let mut b = b0();
        b.setup_edit
            .push(|s| s.exec.default.force_slice.max_chunk_notional = n("100"));
        let mut rp = b0_risk(&b.rd.load());
        let mut ac = rp.accounts.as_slice().to_vec();
        for a in &mut ac {
            a.default_group.max_group_notional = n(group_cap);
            a.max_order_notional = n(order_cap);
        }
        rp.accounts = hft_risk::PreVec::from_vec(ac);
        b.risk = Some(rp);
        let mut h = b.boot_self_contained();
        h.steady();
        if shadow {
            h.cmd(
                AC1,
                &hft_event::ControlCmd::SetMode {
                    scope: scope(S1, AC1, hft_types::GroupId(0)),
                    mode: hft_types::RunMode::Shadow,
                },
            );
            h.run();
        }
        install(OnFail::Reverse);
        let before = h.s.engine().oms.table_view(A, AC1).unwrap().free_len;
        let out = h.io_ref().out.len();
        let (m, ds) = fire(&mut h);
        assert_eq!(ds.len(), 1, "{ds:?}");
        assert_eq!(ds[0].reject, bit.0, "{ds:?}");
        assert_eq!(
            h.since(m)
                .iter()
                .filter(|r| r.flags.0 & hft_event::RecordFlags::SHADOW.0 != 0)
                .count(),
            usize::from(shadow)
        );
        assert_eq!(h.s.engine().oms.table_view(A, AC1).unwrap().free_len, before);
        assert!(last_link(&h, AC1).is_none());
        assert!(h.io_ref().out[out..].iter().all(|(_, c)| c.op != GwOp::Place));
        assert!(h.verify().is_empty());
    }
    reset_script();
    let mut b = b0();
    b.setup_edit.push(|s| s.oms.slots.set(B, AC1, 1));
    let mut h = b.boot_self_contained();
    h.steady();
    install(OnFail::Reverse);
    let (_, ds) = fire(&mut h);
    assert_eq!(ds.len(), 2);
    let before = h.s.engine().oms.table_view(A, AC1).unwrap().free_len;
    let (_, ds) = fire(&mut h);
    assert_eq!(ds.len(), 1);
    assert_eq!(ds[0].reject, RejectBits::CAPACITY.0);
    assert_eq!(h.s.engine().oms.table_view(A, AC1).unwrap().free_len, before);
    assert!(h.verify().is_empty());
}

pub(super) fn check_simultaneous_residual_affordability() {
    reset_script();
    let mut b = b0();
    b.setup_edit
        .push(|s| s.exec.default.force_slice.max_chunk_notional = n("10"));
    let mut rp = b0_risk(&b.rd.load());
    let mut ac = rp.accounts.as_slice().to_vec();
    for a in &mut ac {
        a.max_hedge_notional = n("85");
    }
    rp.accounts = hft_risk::PreVec::from_vec(ac);
    b.risk = Some(rp);
    let mut h = b.boot_self_contained();
    h.steady();
    set_hook(|hx, ev, ctx| {
        if let EventRef::Bbo { inst, b, .. } = ev
            && *inst == A1
            && hx.s.strategy == S1
            && b.ask_px == px("50003")
        {
            let mut it = pair_first(hx, q("0.001"));
            let mut link = *it.link.get().unwrap();
            link.hedge.kind = HedgeKind::Simultaneous;
            link.hedge.on_fail = OnFail::Hold;
            link.hedge.ratio_num = 3;
            link.hedge.ratio_den = 2;
            it.link = LegLinkOpt::some(link);
            let _ = ctx.place(it);
        }
    });
    let (_, ds) = fire(&mut h);
    assert_eq!(ds.len(), 2, "{ds:?}");
    assert!(ds.iter().all(|d| d.reject == 0));
    assert_eq!(ds.iter().find(|d| d.inst == B1).unwrap().qty, q("0.001"));
    // The next 0.0015 ratio estimate plus carried 0.0005 becomes a 0.002 order.
    let before = h.s.engine().oms.table_view(A, AC1).unwrap().free_len;
    let out = h.io_ref().out.len();
    let (_, ds) = fire(&mut h);
    assert_eq!(ds.len(), 1, "{ds:?}");
    assert_eq!(ds[0].reject, RejectBits::HEDGE_LIMIT.0, "{ds:?}");
    assert_eq!(h.s.engine().oms.table_view(A, AC1).unwrap().free_len, before);
    assert!(h.io_ref().out[out..].iter().all(|(_, c)| c.op != GwOp::Place));
    assert!(h.verify().is_empty());
}

pub(super) fn check_simultaneous_ring_full_sends_neither() {
    for ring in [OutRing::Venue(A), OutRing::Venue(B)] {
        reset_script();
        let mut h = b0().boot_self_contained();
        h.steady();
        install(OnFail::Reverse);
        h.io().out_free.insert(ring, 0);
        let out = h.io_ref().out.len();
        let (m, ds) = fire(&mut h);
        assert_eq!(ds.len(), 2);
        assert!(h.io_ref().out[out..].iter().all(|(_, c)| c.op != GwOp::Place));
        let reports = h
            .since(m)
            .iter()
            .filter(|r| r.kind == EventKind::SEND_REPORT)
            .count();
        assert_eq!(reports, 2);
        assert_eq!(last_link(&h, AC1).unwrap().state, hft_oms::LinkState::Done);
        assert!(h.verify().is_empty());
    }
}

pub(super) fn check_simultaneous_second_binding_blocks_both() {
    reset_script();
    let mut h = b0().boot_self_contained();
    h.steady();
    h.cmd(
        AC2,
        &hft_event::ControlCmd::SetMode {
            scope: scope(S2, AC2, hft_types::GroupId(0)),
            mode: hft_types::RunMode::Shadow,
        },
    );
    h.run();
    set_hook(|hx, ev, ctx| {
        if let EventRef::Bbo { inst, b, .. } = ev
            && *inst == A1
            && hx.s.strategy == S1
            && b.ask_px == px("50003")
        {
            let mut it = pair_first(hx, q("0.01"));
            let mut link = *it.link.get().unwrap();
            link.hedge.kind = HedgeKind::Simultaneous;
            link.hedge.on_fail = OnFail::Reverse;
            link.hedge.account = AC2;
            it.link = LegLinkOpt::some(link);
            let _ = ctx.place(it);
        }
    });
    let out = h.io_ref().out.len();
    let (m, ds) = fire(&mut h);
    assert_eq!(ds.len(), 1);
    assert_eq!(ds[0].reject, RejectBits::NOT_BOUND.0);
    assert!(h.since(m).iter().all(|r| r.kind != EventKind::SEND_REPORT));
    assert!(last_link(&h, AC1).is_none());
    assert!(h.io_ref().out[out..].iter().all(|(_, c)| c.op != GwOp::Place));
    assert!(h.verify().is_empty());
}

fn terminal(h: &mut H, d: &OrderDecision, qty: Qty, query: bool) {
    let what = if query {
        OrderEventKind::QueryResult
    } else if qty.0 == 0 {
        OrderEventKind::Reject
    } else if qty == d.qty {
        OrderEventKind::Fill
    } else {
        OrderEventKind::Expired
    };
    let mut ev = order_event(d, what, h.epoch());
    ev.cum_filled = qty;
    ev.qty = qty;
    if query {
        ev.detail = if qty == d.qty { 3 } else { 4 };
    }
    h.push(RingId::Rep(0), &ev);
    h.run();
}

pub(super) fn check_simultaneous_fault_seeds_and_snapshot_replay() {
    for seed in 1..=12u64 {
        for policy in [OnFail::Reverse, OnFail::Hold] {
            reset_script();
            let mut h = b0().boot_self_contained();
            h.steady();
            install(policy);
            let (_, ds) = fire(&mut h);
            assert_eq!(ds.len(), 2, "seed {seed}: {ds:?}");
            assert!(ds.iter().all(|d| d.reject == 0));
            assert_eq!(ds[0].trigger_seq, ds[1].trigger_seq);
            let first = *ds.iter().find(|d| d.inst == A1).unwrap();
            let second = *ds.iter().find(|d| d.inst == B1).unwrap();
            // Snapshot while both original orders are in flight.
            let window = h.keyframe().seq;
            let mut rng = seed;
            let mut next = || {
                rng = rng.wrapping_mul(6364136223846793005).wrapping_add(1);
                rng >> 33
            };
            let qa = if seed % 4 == 0 {
                Qty(0)
            } else {
                Qty((1 + next() % 10) as i64 * q("0.001").0)
            };
            let qb = if seed % 3 == 0 {
                Qty(0)
            } else {
                Qty((next() % 10) as i64 * q("0.001").0)
            };
            let down = seed % 3 == 1;
            if down {
                for c in [0, 1] {
                    h.push(
                        RingId::Rep(0),
                        &conn(
                            B,
                            c,
                            ConnStateKind::Down,
                            ConnRole::Order,
                            hft_types::InstId(0),
                            AC1,
                            h.now(),
                        ),
                    );
                }
                h.run();
            }
            let m = h.mark();
            if seed % 2 == 0 {
                terminal(&mut h, &second, qb, false);
            }
            terminal(&mut h, &first, qa, false);
            if seed % 2 != 0 {
                assert!(decs(&h, m).iter().all(|d| d.link_role != LinkRole::Reverse as u8));
                if down {
                    h.push(
                        RingId::Rep(0),
                        &conn(
                            B,
                            0,
                            ConnStateKind::Up,
                            ConnRole::Order,
                            hft_types::InstId(0),
                            AC1,
                            h.now(),
                        ),
                    );
                    h.run();
                }
                terminal(&mut h, &second, qb, down);
            }
            let reverse: Vec<_> = decs(&h, m)
                .into_iter()
                .filter(|d| d.link_role == LinkRole::Reverse as u8)
                .collect();
            if policy == OnFail::Reverse && qa != qb {
                assert_eq!(reverse.len(), 1, "seed {seed}: {reverse:?}");
                let rv = reverse[0];
                assert_eq!(rv.reject, 0, "seed {seed}: {rv:?}");
                assert_eq!(rv.qty, Qty((qa.0 - qb.0).abs()));
                assert_eq!(rv.inst, if qa > qb { A1 } else { B1 });
                h.fill(&rv, rv.qty, rv.qty, rv.px);
                assert_eq!(h.s.engine().ledger.pos(AC1, A1).qty.0, qa.0.min(qb.0));
                assert_eq!(h.s.engine().ledger.pos(AC1, B1).qty.0, -qa.0.min(qb.0));
            } else {
                assert!(reverse.is_empty());
                assert_eq!(h.s.engine().ledger.pos(AC1, A1).qty, qa);
                assert_eq!(h.s.engine().ledger.pos(AC1, B1).qty, Qty(-qb.0));
            }
            assert_eq!(last_link(&h, AC1).unwrap().state, hft_oms::LinkState::Done);
            assert!(h.verify().is_empty(), "seed {seed}: full replay");
            assert!(
                h.verify_from(Some(window)).is_empty(),
                "seed {seed}: snapshot replay"
            );
        }
    }
}

pub(super) fn check_simultaneous_cold_restart_keeps_inflight_link() {
    reset_script();
    let mut h = b0().boot_self_contained();
    h.steady();
    install(OnFail::Reverse);
    let (_, ds) = fire(&mut h);
    let old_epoch = h.epoch();
    let window = h.keyframe().seq;
    let link = last_link(&h, AC1).unwrap().link_id;
    let mut h = h
        .crash_restart(
            plan(hft_sequencer::SnapshotSel::AtSeq(window), true, 2, 0),
            Rerun::default(),
        )
        .unwrap();
    assert!(h.s.engine().recovery_mismatches().is_empty());
    h.bring_up(false);
    assert_eq!(
        h.s.engine().oms.link_plan(link).unwrap().1.kind,
        HedgeKind::Simultaneous
    );
    let resumed_window = h.keyframe().seq;
    let m = h.mark();
    for d in &ds {
        let mut ev = order_event(d, OrderEventKind::QueryResult, old_epoch);
        ev.detail = 4;
        ev.cum_filled = if d.inst == A1 { q("0.005") } else { q("0.008") };
        h.push(RingId::Rep(0), &ev);
        h.run();
    }
    let rv: Vec<_> = decs(&h, m)
        .into_iter()
        .filter(|d| d.link_role == LinkRole::Reverse as u8)
        .collect();
    assert_eq!(rv.len(), 1, "{rv:?}");
    assert_eq!((rv[0].inst, rv[0].qty, rv[0].reject), (B1, q("0.003"), 0));
    h.fill(&rv[0], rv[0].qty, rv[0].qty, rv[0].px);
    assert_eq!(
        h.s.engine().oms.link(link).unwrap().state,
        hft_oms::LinkState::Done
    );
    assert!(h.verify_from(Some(resumed_window)).is_empty());
}
