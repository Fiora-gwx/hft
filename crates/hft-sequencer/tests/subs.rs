//! The inst → strategy dense index (TESTS.md SEQ-226–SEQ-233).

use hft_event::{ControlCmd, FeedCmd};
use hft_strategy_sdk::{EvMask, Part};
use hft_types::{GroupId, RunMode};

use crate::common::*;

fn steady() -> H {
    reset_script();
    let mut h = b0().boot();
    h.steady();
    h
}

fn sub_ix(h: &H, i: hft_types::InstId) -> Option<u16> {
    h.s.instances().subs.ix_of(i)
}

#[test]
fn seq_226_sub_ix_not_recycled() {
    let mut h = steady();
    let a1 = sub_ix(&h, A1).expect("a1");
    // s1 moves to a2 (and drops a1), then back.
    set_subs(S1, vec![(A2, 0, EvMask::BBO, Part::Fast)]);
    let mut p = Spb::s1(2);
    p.values[F_THR] = 7;
    h.set_params(&p);
    h.run();
    assert!(sub_ix(&h, A2).is_some());
    assert_eq!(sub_ix(&h, A1), Some(a1));
    set_subs(S1, vec![(A1, 0, EvMask::BBO, Part::Fast)]);
    let mut p = Spb::s1(3);
    p.values[F_THR] = 8;
    h.set_params(&p);
    h.run();
    assert_eq!(sub_ix(&h, A1), Some(a1));
}

#[test]
fn seq_228_subscribe_on_union_change() {
    let mut h = steady();
    // a2 is subscribed by s2's group legs? No: a3 starts unsubscribed.
    let n = h.io_ref().feed.len();
    set_subs(
        S1,
        vec![
            (A1, 0, EvMask::BBO, Part::Fast),
            (A3, 0, EvMask(EvMask::BBO.0 | EvMask::TRADE.0), Part::Fast),
        ],
    );
    let mut p = Spb::s1(2);
    p.values[F_THR] = 7;
    h.set_params(&p);
    h.run();
    let new: Vec<_> = h.io_ref().feed[n..].to_vec();
    assert!(new.iter().any(|(_, c)| matches!(c, FeedCmd::Subscribe { inst, streams } if *inst == A3 && *streams == EvMask::BBO.0 | EvMask::TRADE.0)), "{new:?}");
    // Unchanged union: nothing.
    let n = h.io_ref().feed.len();
    let mut p = Spb::s1(3);
    p.values[F_THR] = 8;
    h.set_params(&p);
    h.run();
    assert!(
        h.io_ref().feed[n..]
            .iter()
            .all(|(_, c)| !matches!(c, FeedCmd::Subscribe { inst, .. } if *inst == A3))
    );
    // Dropped by everyone: Unsubscribe.
    set_subs(S1, vec![(A1, 0, EvMask::BBO, Part::Fast)]);
    let n = h.io_ref().feed.len();
    let mut p = Spb::s1(4);
    p.values[F_THR] = 9;
    h.set_params(&p);
    h.run();
    assert!(
        h.io_ref().feed[n..]
            .iter()
            .any(|(_, c)| matches!(c, FeedCmd::Unsubscribe { inst } if *inst == A3))
    );
}

#[test]
fn seq_229_position_driven_subscription() {
    reset_script();
    set_subs(S1, vec![]);
    let mut b = b0();
    b.setup_edit.push(|s| {
        let _ = s.ledger.ref_inst.push((ETH, A2));
    });
    let mut h = b.boot();
    h.step();
    let u = |h: &H, i: hft_types::InstId| h.s.instances().subs.union[i.0 as usize];
    assert!(u(&h, A1) & EvMask::BBO.0 != 0);
    let perp = EvMask::BBO.0 | EvMask::MARK.0 | EvMask::FUNDING.0;
    assert_eq!(u(&h, B1) & perp, perp);
    assert!(u(&h, A2) & EvMask::BBO.0 != 0);
}

#[test]
fn seq_231_max_insts_exhausted() {
    use hft_sequencer::driver::live::RingId;
    reset_script();
    let mut b = b0();
    b.caps.max_insts = 3;
    b.setup_edit.push(|s| {
        let _ = s.ledger.ref_inst.push((ETH, A2));
    });
    let mut h = b.boot();
    h.steady();
    assert_eq!(h.s.instances().subs.insts.len(), 3);
    let before = h.io_ref().feed.len();
    let x = crate::process::ext(
        AC1,
        B2,
        hft_types::Side::Buy,
        "0.004",
        hft_event::ExternalCause::Manual,
        7,
    );
    h.push(RingId::Rep(1), &x);
    h.step();
    assert_eq!(h.s.engine().ledger.pos(AC1, B2).qty, q("0.004"));
    assert!(sub_ix(&h, B2).is_none());
    assert_eq!(h.s.instances().subs.union[B2.0 as usize], 0);
    assert!(
        !h.io_ref().feed[before..]
            .iter()
            .any(|(_, c)| matches!(c,FeedCmd::Subscribe{inst,..} if *inst==B2))
    );
    assert!(h.verify().is_empty());
}

#[test]
fn seq_230_union_follows_position() {
    use hft_sequencer::driver::live::RingId;
    reset_script();
    // A group-free instrument isolates the same zero-crossing rule after residual write-off.
    set_subs(S1, vec![(B2, 0, EvMask::BBO, Part::Fast)]);
    let mut h = b0().boot();
    h.steady();
    let perp = EvMask::BBO.0 | EvMask::MARK.0 | EvMask::FUNDING.0;
    for (id, side, want) in [
        (1, hft_types::Side::Sell, perp),
        (2, hft_types::Side::Buy, EvMask::BBO.0),
        (3, hft_types::Side::Sell, perp),
    ] {
        let before = h.io_ref().feed.len();
        let x = crate::process::ext(AC1, B2, side, "0.004", hft_event::ExternalCause::Manual, id);
        h.push(RingId::Rep(1), &x);
        h.step();
        assert_eq!(h.s.instances().subs.union[B2.0 as usize], want);
        assert!(
            h.io_ref().feed[before..]
                .iter()
                .any(|(_, c)| matches!(c,FeedCmd::Subscribe{inst,streams} if *inst==B2 && *streams==want))
        );
        assert!(
            !h.io_ref().feed[before..]
                .iter()
                .any(|(_, c)| matches!(c,FeedCmd::Unsubscribe{inst} if *inst==B2))
        );
    }
    assert!(h.verify().is_empty());
}

#[test]
fn seq_232_startup_subscribe_once() {
    reset_script();
    let mut h = b0().boot();
    h.step();
    let subs: Vec<_> = h
        .io_ref()
        .feed
        .iter()
        .filter_map(|(_, c)| match c {
            FeedCmd::Subscribe { inst, .. } => Some(*inst),
            _ => None,
        })
        .collect();
    let mut d = subs.clone();
    d.sort_by_key(|i| i.0);
    d.dedup();
    assert_eq!(d.len(), subs.len());
    assert!(!subs.contains(&A3));
}

#[test]
fn seq_233_group_index_delivery() {
    reset_script();
    let mut b = b0();
    let g2b = GroupId(0x0200_0002);
    let mut p = Spb::s2(1);
    p.groups = vec![gspec(G2), gspec_eth(g2b)];
    b.params = vec![Spb::s1(1), p, Spb::s9(1)];
    let mut h = b.boot();
    h.steady();
    clear_log();
    h.cmd(
        AC2,
        &ControlCmd::SetMode {
            scope: scope(S2, AC2, g2b),
            mode: RunMode::CloseOnly,
        },
    );
    h.run();
    let m: Vec<Got> = log()
        .into_iter()
        .filter(|g| g.kind == "Mode" && !g.model)
        .collect();
    assert_eq!(m.len(), 1);
    assert_eq!(m[0].s, S2);
    assert_eq!(m[0].gix, 1);
}

#[test]
fn seq_433_binding_precedes_first_subscription() {
    let mut h = steady();
    // a3 is not subscribed: the binding is recorded, nothing reaches the feed yet.
    let n = h.io_ref().feed.len();
    let id = h.cmd_confirmed(
        AC1,
        &ControlCmd::BindSlot {
            venue: hft_types::VenueId::BINANCE_SPOT,
            _pad: [0; 3],
            inst: A3,
            start_s: 1_000,
            end_s: 1_300,
            wire_id: [0x33; 32],
        },
    );
    h.run();
    assert_eq!(h.res(id).0, hft_sequencer::codes::status::APPLIED);
    assert!(
        h.io_ref().feed[n..]
            .iter()
            .all(|(_, c)| !matches!(c, FeedCmd::BindWire { inst, .. } if *inst == A3)),
        "SEQ-433.1"
    );
    // The first subscription of a3: its four parts first, then Subscribe.
    let n = h.io_ref().feed.len();
    set_subs(
        S1,
        vec![(A1, 0, EvMask::BBO, Part::Fast), (A3, 0, EvMask::BBO, Part::Fast)],
    );
    let mut p = Spb::s1(2);
    p.values[F_THR] = 7;
    h.set_params(&p);
    h.run();
    let a3: Vec<_> = h.io_ref().feed[n..]
        .iter()
        .filter(|(_, c)| match c {
            FeedCmd::BindWire { inst, .. } | FeedCmd::Subscribe { inst, .. } => *inst == A3,
            _ => false,
        })
        .map(|(_, c)| *c)
        .collect();
    assert_eq!(a3.len(), 5, "SEQ-433.2 {a3:?}");
    for (k, c) in a3.iter().take(4).enumerate() {
        assert!(
            matches!(c, FeedCmd::BindWire { part, chunk, .. } if usize::from(*part) == k && *chunk == [0x33; 8]),
            "SEQ-433.2 {a3:?}"
        );
    }
    assert!(
        matches!(a3.get(4), Some(FeedCmd::Subscribe { .. })),
        "SEQ-433.2 {a3:?}"
    );
}
