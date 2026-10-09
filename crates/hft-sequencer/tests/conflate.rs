//! BBO conflation (TESTS.md "BBO 合并与 CONFLATED": SEQ-099–SEQ-103).

use hft_event::{ControlCmd, EventKind, RecordFlags, Role};
use hft_sequencer::driver::live::RingId;

use crate::common::*;

fn steady() -> H {
    reset_script();
    let mut h = b0().boot();
    h.steady();
    h
}

fn push_bbo(h: &mut H, i: hft_types::InstId, upd: u64) {
    let v = inst_venue(i);
    h.push(RingId::Bbo(0), &bbo(i, v, 0, "50000", "50002", upd, 1));
}

/// 70 Bbo: the first 32 = a1 ×20, a2 ×11, a3 ×1 (interleaved), then 38 more a1.
fn backlog70(h: &mut H) {
    let mut upd = 100_000;
    for k in 0..32 {
        upd += 1;
        let i = if k == 31 {
            A3
        } else if k % 3 == 1 && k < 33 {
            A2
        } else {
            A1
        };
        push_bbo(h, i, upd);
    }
    for _ in 0..38 {
        upd += 1;
        push_bbo(h, A1, upd);
    }
}

#[test]
fn seq_099_conflate_keeps_last() {
    let mut h = steady();
    backlog70(&mut h);
    let first32: Vec<hft_types::InstId> = h.io().ring(RingId::Bbo(0)).iter().take(32).map(|e| hft_sequencer::types::read_body::<hft_event::Bbo>(&e.body).unwrap().h.inst).collect();
    let n_a1 = first32.iter().filter(|i| **i == A1).count();
    let n_a2 = first32.iter().filter(|i| **i == A2).count();
    assert_eq!(first32.iter().filter(|i| **i == A3).count(), 1);
    let m = h.mark();
    h.step();
    let b: Vec<_> = h.since(m).into_iter().filter(|r| r.kind == EventKind::BBO).collect();
    // The 3 kept records come first; the remaining 38 are handed one by one afterwards.
    let kept: Vec<_> = b.iter().take(3).collect();
    let insts: Vec<hft_types::InstId> = kept.iter().map(|r| r.read::<hft_event::Bbo>().unwrap().h.inst).collect();
    assert!(insts.contains(&A1) && insts.contains(&A2) && insts.contains(&A3));
    for r in kept {
        let i = r.read::<hft_event::Bbo>().unwrap().h.inst;
        let conf = r.flags.0 & RecordFlags::CONFLATED.0 != 0;
        assert_eq!(conf, i != A3, "{i:?}");
    }
    let timers: Vec<hft_event::Timer> = h.since(m).iter().filter(|r| r.kind == EventKind::TIMER).filter_map(|r| r.read::<hft_event::Timer>()).collect();
    assert_eq!(h.s.driver().conflated, (n_a1 - 1 + n_a2 - 1) as u64, "{timers:?} {n_a1} {n_a2}");
    assert_eq!(n_a1 - 1 + n_a2 - 1, 29);
    // ConflateReport one second later carries 29.
    h.run();
    h.adv(1_100_000_000);
    let m = h.mark();
    h.run();
    let n = h.notes_since(m).into_iter().find(|n| n.code == hft_sequencer::codes::seq_note(2)).expect("Conflated");
    assert_eq!(n.v[0], 29);
}

#[test]
fn seq_100_conflate_threshold() {
    for (n, conflate) in [(63u64, false), (64, false), (65, true)] {
        let mut h = steady();
        for k in 0..n {
            push_bbo(&mut h, A1, 200_000 + k);
        }
        let m = h.mark();
        h.step();
        let b: Vec<_> = h.since(m).into_iter().filter(|r| r.kind == EventKind::BBO).collect();
        if conflate {
            assert!(b[0].flags.0 & RecordFlags::CONFLATED.0 != 0);
            assert_eq!(n as usize - h.io().ring(RingId::Bbo(0)).len(), 32);
        } else {
            assert_eq!(b.len(), 32);
            assert!(b.iter().all(|r| r.flags.0 & RecordFlags::CONFLATED.0 == 0));
        }
    }
}

#[test]
fn seq_101_checks_between_conflated() {
    let mut h = steady();
    backlog70(&mut h);
    let base = h.io().takes;
    let (c, _) = h.cmd_elem("cli:test", 4, AC1, &ControlCmd::SnapshotNow, |_| {});
    h.io().arrivals.push(Arrival { after_take: base + 1, ring: RingId::Ctl, elem: c });
    let m = h.mark();
    h.step();
    let k: Vec<EventKind> = h.since(m).iter().filter(|r| r.role() == Role::Input).map(|r| r.kind).take(4).collect();
    assert_eq!(k, vec![EventKind::BBO, EventKind::CONTROL_CMD, EventKind::BBO, EventKind::BBO]);
}

#[test]
fn seq_102_md_never_conflated() {
    let mut h = steady();
    for k in 0..2_000u64 {
        h.push(RingId::Md(0), &trade(A1, k + 1));
    }
    let m = h.mark();
    h.run();
    let md: Vec<_> = h.since(m).into_iter().filter(|r| r.kind == EventKind::TRADE).collect();
    assert_eq!(md.len(), 2_000);
    assert!(md.iter().all(|r| r.flags.0 & RecordFlags::CONFLATED.0 == 0));
    let ids: Vec<u64> = md.iter().map(|r| u64::from_le_bytes(r.body[72..80].try_into().unwrap())).collect();
    assert!(ids.windows(2).all(|w| w[1] == w[0] + 1));
}

#[test]
fn seq_103_dropped_bbo_invisible() {
    let mut h1 = steady();
    backlog70(&mut h1);
    let m1 = h1.mark();
    h1.run();
    let handed: Vec<Elem> = h1
        .since(m1)
        .iter()
        .filter(|r| r.kind == EventKind::BBO)
        .map(|r| Elem { kind: EventKind::BBO, flags: RecordFlags(r.flags.0 & RecordFlags::CONFLATED.0), body: r.body.clone() })
        .collect();
    let mut h2 = steady();
    let m2 = h2.mark();
    for e in handed {
        h2.push_e(RingId::Bbo(0), e);
    }
    h2.run();
    let outs = |h: &H, m: usize| -> Vec<(EventKind, Vec<u8>)> {
        h.since(m)
            .iter()
            .filter(|r| r.role() == Role::Output)
            .map(|r| {
                let mut b = r.body.clone();
                hft_sequencer::journal::mask_measure(r.kind, &mut b);
                (r.kind, b)
            })
            .collect()
    };
    assert_eq!(outs(&h1, m1), outs(&h2, m2));
    let e1 = h1.s.engine();
    let e2 = h2.s.engine();
    assert_eq!(e1.market.bbo_view(&e1.books, A1), e2.market.bbo_view(&e2.books, A1));
}
