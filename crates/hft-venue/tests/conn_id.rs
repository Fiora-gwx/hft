//! VEN-030–VEN-033: connection numbers and BBO slots (README §1).
#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods
)]

use crate::support;

use std::collections::BTreeSet;

use hft_event::{ConnStateKind, FeedCmd, FeedMsg};
use hft_net::{ConnId, ErrBits};
use hft_types::{InstId, MAX_BBO_CONN_SLOTS};
use hft_venue::{FeedCfg, FeedRoutes, streams};
use proptest::prelude::*;
use support::feedh::*;
use support::net::Ev;
use support::rd::*;

#[test]
fn ven_030_bbo_slot_smallest_free() {
    let mut h = feed(FeedCfg {
        routes: FeedRoutes::Bbo,
        max_sub_insts: 100,
        ..f0()
    });
    for n in 1..=51 {
        h.sub(p(n), streams::BBO);
    }
    for t in 0..20 {
        h.poll(t);
    }
    assert_eq!(
        h.all_builds.iter().map(|b| b.conn.0).collect::<Vec<_>>(),
        vec![0, 1, 2, 3]
    );
    h.complete_builds();
    let mut subs: Vec<BTreeSet<u32>> = vec![BTreeSet::new(); 4];
    let mut t = 100;
    for _ in 0..200 {
        t += 10_000_000;
        h.poll(t);
        for c in 0..4u8 {
            for f in h.net.sent_text(&mut h.rt, ConnId(c)) {
                if let Some(rest) = f.strip_prefix("SUB|") {
                    for x in rest.split(',') {
                        subs[c as usize].insert(x.trim_end_matches('b').parse().unwrap());
                    }
                }
            }
        }
    }
    let g1: BTreeSet<u32> = (1..=50).map(|n| p(n).0).collect();
    let g2: BTreeSet<u32> = [p(51).0].into_iter().collect();
    assert_eq!(subs, vec![g1.clone(), g1, g2.clone(), g2]);
}

#[test]
fn ven_031_slot_reuse_after_down() {
    let (mut h, now) = bbo_up(|_| {}, &[S1]);
    h.builds.clear();
    h.net.at(ConnId(0), 0, Ev::Rst);
    h.poll(now + 10);
    let v1 = h.drain_bbo();
    assert!(
        has_state(&v1, 0, ConnStateKind::Down, InstId(0), ErrBits::RESET.bits()),
        "{v1:?}"
    );
    assert_eq!(h.retired, vec![ConnId(0)], "RetireMsg");
    assert_eq!(
        h.builds.iter().map(|b| b.conn).collect::<Vec<_>>(),
        vec![ConnId(0)],
        "new BuildReq{{conn: 0}}"
    );
    h.complete_builds();
    h.poll(now + 20);
    h.ack(ConnId(0));
    h.net.push(ConnId(0), &b("S1", 9));
    h.poll(now + 30);
    let v2 = h.drain_bbo();
    let first_new = v2
        .iter()
        .position(|m| matches!(m, FeedMsg::ConnState(s) if s.conn == 0 && s.state == ConnStateKind::Up && s.inst == InstId(0)));
    assert_eq!(
        first_new,
        Some(0),
        "the new connection's Up comes first, after the Down of round 1"
    );
}

#[test]
fn ven_032_build_fail_releases_id() {
    let mut h = feed(FeedCfg {
        routes: FeedRoutes::Bbo,
        ..f0()
    });
    h.sub(S1, streams::BBO);
    h.poll(100);
    h.complete(ConnId(0));
    h.poll(200);
    assert_eq!(
        h.builds.iter().map(|b| b.conn).collect::<Vec<_>>(),
        vec![ConnId(1)]
    );
    h.fail(ConnId(1));
    h.poll(300);
    assert_eq!(
        h.builds.iter().map(|b| b.conn).collect::<Vec<_>>(),
        vec![ConnId(1)],
        "BuildReq{{conn: 1}} again"
    );
    assert!(!states(&h.drain_bbo()).iter().any(|s| s.conn == 1));
}

#[derive(Clone, Debug)]
enum Step {
    Eof(u8),
    Rst(u8),
    BuildFail,
    BuildOk,
    Bye(u8),
    Advance(u32),
    Rotate(u8),
}

fn step() -> impl Strategy<Value = Step> {
    prop_oneof![
        (0u8..8).prop_map(Step::Eof),
        (0u8..8).prop_map(Step::Rst),
        Just(Step::BuildFail),
        Just(Step::BuildOk),
        (0u8..8).prop_map(Step::Bye),
        (0u32..=4000).prop_map(Step::Advance),
        (0u8..8).prop_map(Step::Rotate),
    ]
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 256, .. ProptestConfig::default() })]
    #[test]
    fn ven_033_bbo_slots_bounded(bbo_conns in 1u8..=3, n_inst in 1u32..=150, steps in prop::collection::vec(step(), 1..1_000)) {
        let groups = n_inst.div_ceil(50);
        prop_assume!(u32::from(bbo_conns) * groups < u32::from(MAX_BBO_CONN_SLOTS));
        let mut h = feed(FeedCfg { routes: FeedRoutes::Bbo, bbo_conns, max_sub_insts: n_inst, ..f0() });
        for n in 1..=n_inst {
            h.sub(p(n), streams::BBO);
        }
        let mut now = 100u64;
        let mut up = [false; 8];
        for s in steps {
            match s {
                Step::Eof(c) => h.net.at(ConnId(c), 0, Ev::Eof),
                Step::Rst(c) => h.net.at(ConnId(c), 0, Ev::Rst),
                Step::BuildFail => {
                    if let Some(b) = h.builds.first().copied() {
                        h.fail(b.conn);
                    }
                }
                Step::BuildOk => {
                    if let Some(b) = h.builds.first().copied() {
                        h.complete(b.conn);
                    }
                }
                Step::Bye(c) => h.net.at(ConnId(c), 0, Ev::Text(b"BYE".to_vec())),
                Step::Advance(ms) => now += u64::from(ms) * 1_000_000,
                Step::Rotate(c) => {
                    let _ = h.cmd.try_push(FeedCmd::Rotate { conn: ConnId(c) });
                }
            }
            now += 1;
            h.poll(now);
            for c in h.rt.conn_ids().collect::<Vec<_>>() {
                h.ack(c);
            }
            // Bound: open builds + live connections + retired-not-reclaimed ≤ 8.
            let live = h.rt.conn_ids().count();
            prop_assert!(h.builds.len() + live <= usize::from(MAX_BBO_CONN_SLOTS), "{} + {}", h.builds.len(), live);
            for m in h.drain_bbo() {
                if let FeedMsg::ConnState(s) = m {
                    prop_assert!(s.conn < MAX_BBO_CONN_SLOTS);
                    if s.inst == InstId(0) {
                        match s.state {
                            ConnStateKind::Up => {
                                prop_assert!(!up[s.conn as usize], "two Up on slot {} without Down", s.conn);
                                up[s.conn as usize] = true;
                            }
                            ConnStateKind::Down => {
                                prop_assert!(up[s.conn as usize], "Down on slot {} without Up", s.conn);
                                up[s.conn as usize] = false;
                            }
                            _ => {}
                        }
                    }
                }
            }
        }
    }
}
