#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods
)] // allow-list: test code
#![allow(clippy::type_complexity, clippy::too_many_arguments)]
mod common;

use common::*;
use hft_event::{ConnRole, ConnStateKind, ExternalCause, Note, OrderAction, OrderDecision, OrderEventKind};
use hft_oms::notes;
use hft_oms::*;
use hft_strategy_sdk::{OrderRef, OrderUpdate, SnapshotReader, SnapshotWriter};
use hft_types::{AccountId, GROUP_NONE, Side, StrategyId, Tif};

#[derive(Default)]
struct Streams {
    notes: Vec<Note>,
    updates: Vec<OrderUpdate>,
    timers: Vec<OmsTimerReq>,
    decisions: Vec<OrderDecision>,
    hedges: Vec<HedgeRequest>,
    links: Vec<LinkUpdate>,
}

impl Streams {
    fn take(&mut self, o: &OmsOut) {
        assert!(o.notes.len() <= o.notes.capacity());
        assert!(o.updates.len() <= o.updates.capacity());
        assert!(o.timers.len() <= o.timers.capacity());
        assert!(o.decisions.len() <= o.decisions.capacity());
        self.notes.extend(o.notes.iter().copied());
        self.updates.extend(o.updates.iter().copied());
        self.timers.extend(o.timers.iter().copied());
        self.decisions.extend(o.decisions.iter().copied());
        self.hedges.extend(o.hedges.iter().copied());
        self.links.extend(o.links.iter().copied());
    }
}

fn drain(h: &mut H, s: &mut Streams) -> usize {
    let mut rounds = 0;
    while h.o.wants_continue() {
        h.cont();
        s.take(&h.out);
        rounds += 1;
        assert!(rounds < 100_000);
    }
    rounds
}

fn mass_pending(unbounded: bool) -> (H, Vec<OrderRef>) {
    let mut h = H::with(|c| c.slots.set(VX, A1, 4096));
    if unbounded {
        h.out = OmsOut::unbounded();
    }
    let mut it = sell_y();
    it.intent.group = GROUP_NONE;
    let rs = (0..4096).map(|_| h.place_ok(&it, 0)).collect();
    (h, rs)
}

#[test]
fn oms_330_mass_unknown_fanout() {
    let (mut h, rs) = mass_pending(false);
    h.seq = 4999;
    h.conn(VX, A1, ConnRole::Order, ConnStateKind::Down, 1);
    assert!(rs.iter().all(|r| h.state(*r) == OrderState::Unknown));
    assert_eq!(h.o.table_view(VX, A1).unwrap().unknown_count, 4096);
    assert!(!h.o.venue_ready(VX, A1));
    assert_eq!(h.o.inflight().imm_sell(A1, Y), q("40.96"));
    assert!(h.o.wants_continue());
    let mut b = Streams::default();
    b.take(&h.out);
    drain(&mut h, &mut b);
    let (mut r, _) = mass_pending(true);
    r.seq = 4999;
    r.conn(VX, A1, ConnRole::Order, ConnStateKind::Down, 1);
    let mut u = Streams::default();
    u.take(&r.out);
    assert_eq!(b.notes, u.notes);
    assert_eq!(b.updates, u.updates);
    assert_eq!(b.timers, u.timers);
    let tc = notes::code_of(notes::TRANSITION);
    let uc = notes::code_of(notes::UNKNOWN);
    assert_eq!(b.notes.iter().filter(|n| n.code == tc).count(), 4096);
    assert_eq!(b.notes.iter().filter(|n| n.code == uc).count(), 4096);
    assert_eq!(b.updates.len(), 4096);
    assert!(b.notes.iter().filter(|n| n.code == tc).all(|n| n.v[3] == 5000));
}

fn mass_live(v: hft_types::VenueId) -> (H, Vec<OrderRef>) {
    let mut h = H::with(|c| c.slots.set(v, A1, 4096));
    let it = intent(
        v,
        A1,
        if v == VG { Z } else { Y },
        Side::Sell,
        Tif::Gtc,
        p("60100.0"),
        q("0.001"),
        GROUP_NONE,
    );
    let rs: Vec<_> = (0..4096u64)
        .map(|i| {
            let r = h.place_ok(&it, 0);
            h.ack(r, i + 1, 0);
            r
        })
        .collect();
    h.out.clear();
    h.o.cancel_all(CancelScope::Account(A1), false, ms(1), 7, &mut h.out);
    (h, rs)
}

#[test]
fn oms_331_mass_cancel() {
    let (mut h, rs) = mass_live(VG);
    assert!(
        rs.iter().all(|r| h.state(*r) == OrderState::PendingCancel),
        "OMS-331.1"
    );
    assert_eq!(h.decisions(OrderAction::Cancel).len(), 16);
    assert_eq!(
        rs.iter()
            .filter(|r| h.flags(**r) & SlotFlags::QUEUED != 0)
            .count(),
        4080
    );
    let mut s = Streams::default();
    s.take(&h.out);
    let rounds = drain(&mut h, &mut s);
    assert_eq!(rounds, 255);
    let slots: Vec<u32> = s.decisions.iter().map(|d| d.target_slot.0).collect();
    assert_eq!(slots, rs.iter().map(|r| r.slot).collect::<Vec<_>>());
    assert!(rs.iter().all(|r| h.flags(*r) & SlotFlags::QUEUED == 0));
    let (h, rs) = mass_live(VX);
    let ca = h.decisions(OrderAction::CancelAll);
    assert_eq!(
        (h.out.decisions.len(), ca.len(), ca[0].venue, ca[0].account),
        (1, 1, VX, A1),
        "OMS-331.2"
    );
    assert!(rs.iter().all(|r| h.flags(*r) & SlotFlags::CANCEL_SENT != 0));
}

/// 64 RESUME_HOLD links of a1 (second legs on VX), each owing 0.001.
fn resume64() -> (H, Vec<u32>) {
    let mut h = H::with(|c| c.links_per_account = 128);
    let mut ls = Vec::new();
    for _ in 0..64 {
        let (f, l) = place_first(&mut h, q("0.001"), Tif::Ioc, plan_l1(), 498_800);
        h.fill(f, q("0.001"), p("60000.00"), 498_800);
        assert_eq!(h.out.hedges.len(), 1);
        ls.push(l);
    }
    h.out.clear();
    h.o.recovery_end(ms(10_000), ms(500_000), HZ, HZ, 1, &mut h.out);
    (h, ls)
}

#[test]
fn oms_332_mass_resume() {
    let (mut h, ls) = resume64();
    h.out.clear();
    h.o.on_hedge_gate_open(A1, VX, ms(30_000), 1, &mut h.out);
    assert!(
        ls.iter().all(|l| h.link(*l).flags & link_flags::RESUME_HOLD == 0),
        "OMS-332.1"
    );
    assert_eq!(h.out.hedges.len(), 8, "OMS-332.1");
    let mut s = Streams::default();
    s.take(&h.out);
    drain(&mut h, &mut s);
    assert_eq!(s.hedges.len(), 64, "OMS-332.1");
    for l in &ls {
        assert_eq!(s.hedges.iter().filter(|r| r.link_id == *l).count(), 1);
        assert_eq!(
            s.links.iter().filter(|u| u.link_id == *l && u.flags == 0).count(),
            1
        );
    }
    let (mut h, ls) = resume64();
    h.out.clear();
    h.o.on_hedge_gate_open(A1, VX, ms(30_000), 1, &mut h.out);
    let mut s = Streams::default();
    s.take(&h.out);
    h.out.clear();
    h.o.external_adjust(A1, VX, g1(), Y, q("-0.064"), 0, ExternalCause::Adl, &mut h.out);
    drain(&mut h, &mut s);
    let l9 = ls[8];
    assert_eq!(
        s.hedges.iter().filter(|r| r.link_id == l9).count(),
        0,
        "OMS-332.2"
    );
    assert_eq!(h.link(l9).hedge_req, hft_types::Qty(0), "OMS-332.2");
    let (mut h, ls) = resume64();
    h.sweep(40_000);
    assert!(
        ls.iter().all(|l| h.link(*l).state == LinkState::Broken),
        "OMS-332.3"
    );
    let mut s = Streams::default();
    s.take(&h.out);
    drain(&mut h, &mut s);
    let broken: Vec<_> = s.links.iter().filter(|u| u.state == LinkState::Broken).collect();
    assert_eq!(broken.len(), 64, "OMS-332.3");
    assert!(
        broken
            .iter()
            .all(|u| u.broken_reason == LinkBroken::ResumeTimeout)
    );
}

#[test]
fn oms_333_pending_out_snapshot() {
    let (mut h, _) = mass_live(VG);
    let mut w = SnapshotWriter::new(1 << 23);
    h.o.snapshot(&mut w);
    let bytes = w.as_bytes().to_vec();
    let rd_ = SnapshotReader::new(&bytes);
    let s12 = rd_.find_section(SECTION_OMS_LINKS).unwrap().unwrap();
    let mut outs = 0;
    let mut b = s12.body;
    while b.len() >= 8 && b[0] != 0 {
        let len = u32::from_le_bytes([b[4], b[5], b[6], b[7]]) as usize;
        outs += usize::from(b[0] == 5 && b[8] == out_kind::DECISION);
        b = &b[8 + len..];
    }
    assert_eq!(outs, 4080);
    let o = Oms::restore(
        &mut SnapshotReader::new(&bytes),
        &{
            let mut c = p0();
            c.slots.set(VG, A1, 4096);
            c
        },
        rd(),
    )
    .unwrap();
    let mut r = H {
        o,
        out: OmsOut::new(),
        seq: h.seq,
    };
    let mut a = Streams::default();
    drain(&mut h, &mut a);
    let mut b2 = Streams::default();
    drain(&mut r, &mut b2);
    assert_eq!(a.decisions, b2.decisions);
    assert_eq!(a.decisions.len(), 4080);
}

#[test]
fn oms_334_quota_round_robin() {
    for (row, quota, a1n, a2n) in [
        ("OMS-334.1", 64u16, 13usize, 3usize),
        ("OMS-334.2", 8, 8, 3),
        ("OMS-334.3", 1, 1, 1),
    ] {
        let mut h = H::with(|c| {
            c.out_quota = PerTable::new(Some(quota));
            c.slots.set(VS, A1, 200);
        });
        let mut a1 = Vec::new();
        for i in 0..102 {
            let mut it = buy_x();
            it.intent.group = GROUP_NONE;
            it.risk_reducing = i >= 100;
            a1.push(h.place_ok(&it, 0));
        }
        let mut a2 = Vec::new();
        for _ in 0..3 {
            let it = intent(
                VS,
                A2,
                X,
                Side::Buy,
                Tif::Gtc,
                p("60000.00"),
                q("0.01"),
                GROUP_NONE,
            );
            a2.push(h.place_ok(&it, 0));
        }
        for r in a1.iter().chain(&a2) {
            h.o.test_enqueue_decision(*r, OrderAction::New, 1);
        }
        h.cont();
        let d = h.out.decisions.as_slice();
        let n1 = d.iter().filter(|x| x.account == A1).count();
        let n2 = d.iter().filter(|x| x.account == A2).count();
        assert_eq!((n1, n2), (a1n, a2n), "{row}");
        let first_a1: Vec<_> = d.iter().filter(|x| x.account == A1).collect();
        assert!(
            first_a1[0].oflags & hft_event::OrderFlags::RISK_REDUCING.0 != 0,
            "{row}"
        );
        if quota == 64 {
            assert!(
                first_a1[1].oflags & hft_event::OrderFlags::RISK_REDUCING.0 != 0,
                "{row}"
            );
            let accts: Vec<AccountId> = d.iter().take(6).map(|x| x.account).collect();
            assert_eq!(accts, vec![A1, A2, A1, A2, A1, A2], "{row}");
        }
    }
}

#[test]
fn oms_335_drop_stale_queued_cancel() {
    let (mut h, rs) = mass_live(VG);
    let r100 = rs[99];
    assert!(h.flags(r100) & SlotFlags::QUEUED != 0);
    h.fill(r100, q("0.001"), p("60100.0"), 2);
    assert_eq!(h.state(r100), OrderState::Filled);
    let mut s = Streams::default();
    s.take(&h.out);
    drain(&mut h, &mut s);
    assert!(!s.decisions.iter().any(|d| d.target_slot.0 == r100.slot));
    assert_eq!(s.decisions.len(), 4079);
}

/// a1@`v`: s1 Live Open, s2 PendingNew, s3 PendingAmend, s4 PendingCancel, s5 Unknown, s6 Live RR.
fn six(v: hft_types::VenueId, with_s6: bool) -> (H, Vec<OrderRef>) {
    let mut h = H::new();
    let inst = if v == VG { Z } else { Y };
    let mk = |h: &mut H, rr: bool| {
        let mut it = intent(
            v,
            A1,
            inst,
            Side::Sell,
            Tif::Gtc,
            p("60100.0"),
            q("0.001"),
            GROUP_NONE,
        );
        it.risk_reducing = rr;
        h.place_ok(&it, 0)
    };
    let s1 = mk(&mut h, false);
    h.ack(s1, 1, 0);
    let s2 = mk(&mut h, false);
    let s3 = mk(&mut h, false);
    h.ack(s3, 3, 0);
    h.amend(s3, p("60100.0"), q("0.001"), 0).unwrap();
    let s4 = mk(&mut h, false);
    h.ack(s4, 4, 0);
    h.cancel(s4, 0).unwrap();
    let s5 = mk(&mut h, false);
    h.what(s5, OrderEventKind::OutcomeUnknown, 0);
    let mut v6 = vec![s1, s2, s3, s4, s5];
    if with_s6 {
        let s6 = mk(&mut h, true);
        h.ack(s6, 6, 0);
        v6.push(s6);
    }
    h.out.clear();
    (h, v6)
}

#[test]
fn oms_340_non_hard_kill_scope() {
    let (mut h, s) = six(VX, true);
    h.o.cancel_all(CancelScope::Account(A1), true, ms(1), 1, &mut h.out);
    let c: Vec<u32> = h
        .decisions(OrderAction::Cancel)
        .iter()
        .map(|d| d.target_slot.0)
        .collect();
    assert_eq!(c, vec![s[0].slot, s[1].slot, s[2].slot], "OMS-340.1");
    assert!(h.decisions(OrderAction::CancelAll).is_empty(), "OMS-340.1");
    let (mut h, s) = six(VX, false);
    h.o.cancel_all(CancelScope::Account(A1), true, ms(1), 1, &mut h.out);
    assert_eq!(
        (
            h.decisions(OrderAction::CancelAll).len(),
            h.decisions(OrderAction::Cancel).len()
        ),
        (1, 0),
        "OMS-340.2"
    );
    for r in &s[..3] {
        assert_eq!(h.state(*r), OrderState::PendingCancel, "OMS-340.2");
        assert!(h.flags(*r) & SlotFlags::CANCEL_SENT != 0, "OMS-340.2");
    }
    let (mut h, s) = six(VG, false);
    h.o.cancel_all(CancelScope::Account(A1), true, ms(1), 1, &mut h.out);
    let c: Vec<u32> = h
        .decisions(OrderAction::Cancel)
        .iter()
        .map(|d| d.target_slot.0)
        .collect();
    assert_eq!(c, vec![s[0].slot, s[1].slot, s[2].slot], "OMS-340.3");
}

#[test]
fn oms_341_full_cancel_scope() {
    let (mut h, s) = six(VX, true);
    h.o.cancel_all(CancelScope::Account(A1), false, ms(1), 1, &mut h.out);
    assert_eq!(h.decisions(OrderAction::CancelAll).len(), 1, "OMS-341.1");
    for i in [0, 1, 2, 5] {
        assert!(h.flags(s[i]) & SlotFlags::CANCEL_SENT != 0, "OMS-341.1");
    }
    assert_eq!(
        (h.state(s[3]), h.state(s[4])),
        (OrderState::PendingCancel, OrderState::Unknown),
        "OMS-341.1"
    );
    let (mut h, s) = six(VG, true);
    h.o.cancel_all(CancelScope::Account(A1), false, ms(1), 1, &mut h.out);
    let c: Vec<u32> = h
        .decisions(OrderAction::Cancel)
        .iter()
        .map(|d| d.target_slot.0)
        .collect();
    assert_eq!(c, vec![s[0].slot, s[1].slot, s[2].slot, s[5].slot], "OMS-341.2");
}

#[test]
fn oms_342_reissue_cancels() {
    let z = |rr: bool| {
        let mut it = intent(
            VG,
            A1,
            Z,
            Side::Sell,
            Tif::Gtc,
            p("60100.0"),
            q("0.001"),
            GROUP_NONE,
        );
        it.risk_reducing = rr;
        it
    };
    let reissue = |h: &mut H, open_only: bool| {
        h.out.clear();
        h.o.reissue_cancels(CancelScope::Account(A1), open_only, ms(5), 1, &mut h.out);
        h.decisions(OrderAction::Cancel).len()
    };
    let mut h = H::new();
    for i in 0..2 {
        let r = h.place_ok(&z(false), 0);
        h.ack(r, i + 1, 0);
    }
    assert_eq!(reissue(&mut h, true), 2, "OMS-342.1");
    let mut h = H::new();
    let r = h.place_ok(&z(true), 0);
    h.ack(r, 1, 0);
    assert_eq!(reissue(&mut h, true), 0, "OMS-342.2");
    assert_eq!(reissue(&mut h, false), 1, "OMS-342.3");
    let mut h = H::new();
    let r = h.place_ok(&z(false), 0);
    h.what(r, OrderEventKind::OutcomeUnknown, 0);
    h.query(r, 1, hft_types::Qty(0), hft_types::Px(0), 1);
    assert_eq!(h.state(r), OrderState::Live);
    assert_eq!(reissue(&mut h, false), 1, "OMS-342.4");
    let mut h = H::new();
    let r = h.place_ok(&z(false), 0);
    h.ack(r, 1, 0);
    h.cancel(r, 1).unwrap();
    assert_eq!(reissue(&mut h, false), 0, "OMS-342.5");
    let mut h = H::new();
    let r = h.place_ok(&z(false), 0);
    h.ack(r, 1, 0);
    h.cancel(r, 1).unwrap();
    h.send(
        VG,
        A1,
        hft_event::GwOp::Cancel,
        r.slot,
        r.r#gen,
        hft_event::SendStatus::NoConn,
        2,
        2,
    );
    assert!(h.flags(r) & SlotFlags::CANCEL_SENT == 0);
    assert_eq!(reissue(&mut h, false), 0, "OMS-342.6");
}

#[test]
fn oms_343_cancel_resting_on() {
    let mut h = H::new();
    let mk = |h: &mut H, v, a, inst, tif, rr: bool, exec: bool, smoke: bool, ack: bool| {
        let mut it = intent(v, a, inst, Side::Sell, tif, p("60100.0"), q("0.001"), GROUP_NONE);
        it.risk_reducing = rr;
        if exec {
            it.origin = Origin::Exec;
        }
        if smoke {
            it.origin = Origin::Command;
            it.smoke_ttl_ms = 60_000;
        }
        let r = h.place_ok(&it, 0);
        if ack {
            h.ack(r, u64::from(r.slot) + 100, 0);
        }
        r
    };
    let rows = [
        (
            "OMS-343.1",
            mk(&mut h, VX, A1, Y, Tif::Gtc, false, false, false, true),
            true,
        ),
        (
            "OMS-343.2",
            mk(&mut h, VX, A1, Y, Tif::Gtc, true, false, false, true),
            true,
        ),
        (
            "OMS-343.3",
            mk(&mut h, VX, A1, Y, Tif::PostOnly, false, false, true, true),
            true,
        ),
        (
            "OMS-343.4",
            mk(&mut h, VX, A1, Y, Tif::Gtc, false, false, false, false),
            true,
        ),
        (
            "OMS-343.5",
            mk(&mut h, VX, A1, Y, Tif::Gtc, false, true, false, true),
            false,
        ),
        (
            "OMS-343.6",
            mk(&mut h, VX, A1, Y, Tif::Ioc, false, false, false, true),
            false,
        ),
        (
            "OMS-343.7",
            mk(&mut h, VG, A1, Z, Tif::Gtc, false, false, false, true),
            false,
        ),
        (
            "OMS-343.8",
            mk(&mut h, VS, A2, X, Tif::Gtc, false, false, false, true),
            false,
        ),
    ];
    let stamps: Vec<_> = rows.iter().map(|(_, r, _)| h.stamps(*r).op).collect();
    h.out.clear();
    h.o.cancel_resting_on(A1, Y, ms(1), 1, &mut h.out);
    let c: Vec<u32> = h
        .decisions(OrderAction::Cancel)
        .iter()
        .map(|d| d.target_slot.0)
        .collect();
    for (row, r, want) in &rows {
        let hit = c.contains(&r.slot) && r.venue == VX;
        assert_eq!(hit, *want, "{row}");
    }
    assert!(h.decisions(OrderAction::CancelAll).is_empty(), "OMS-343.9");
    assert_eq!(
        transitions(&h).iter().filter(|t| t.2 == 2).count(),
        4,
        "OMS-343.9"
    );
    for ((_, r, _), op) in rows.iter().zip(stamps) {
        assert_eq!(h.stamps(*r).op, op, "OMS-343.9");
    }
}

#[test]
fn oms_344_cancel_open() {
    let mut h = H::new();
    let mk = |h: &mut H, class, tif, rr, g| {
        let mut it = intent(VS, A1, X, Side::Buy, tif, p("60000.00"), q("0.01"), g);
        it.class = class;
        it.risk_reducing = rr;
        h.place_ok(&it, 0)
    };
    use hft_strategy_sdk::IntentClass as C;
    let a = mk(&mut h, C::Open, Tif::Gtc, false, g1());
    h.ack(a, 1, 0);
    let b = mk(&mut h, C::Open, Tif::PostOnly, false, g1());
    let c = mk(&mut h, C::Reduce, Tif::Gtc, true, g1());
    h.ack(c, 3, 0);
    let d = mk(&mut h, C::Open, Tif::Ioc, false, g1());
    h.ack(d, 4, 0);
    let e = mk(&mut h, C::Open, Tif::Gtc, false, g2());
    h.ack(e, 5, 0);
    h.out.clear();
    h.o.cancel_open(
        CellKey {
            strategy: StrategyId(1),
            account: A1,
            group: g1(),
        },
        ms(1),
        1,
        &mut h.out,
    );
    let got: Vec<u32> = h
        .decisions(OrderAction::Cancel)
        .iter()
        .map(|x| x.target_slot.0)
        .collect();
    assert_eq!(got, vec![a.slot, b.slot]);
    for (row, r, want) in [
        ("OMS-344.1", a, true),
        ("OMS-344.2", b, true),
        ("OMS-344.3", c, false),
        ("OMS-344.4", d, false),
        ("OMS-344.5", e, false),
    ] {
        assert_eq!(h.state(r) == OrderState::PendingCancel, want, "{row}");
    }
}
