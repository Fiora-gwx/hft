//! VEN-070–VEN-072: a full output ring.
#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods
)]

use crate::support;

use hft_event::{FeedCmd, FeedMsg};
use hft_net::ConnId;
use hft_rt::{CalibPair, IdleAction, IdlePolicy, PollTask, Polled, TaskLoop, Tsc, TscClock};
use hft_venue::{FeedCfg, FeedRoutes, FeedRuntime, streams};
use support::feedh::*;
use support::net::Ev;
use support::rd::*;
use support::stub::{self, StubProto};

/// `routes = Bbo`, bbo ring of `cap`; connection 0 read u = 17–23 in one `recvmsg`: 17–20 filled the
/// ring (cap 4), 21–23 stay in its TLS buffer.
fn full(cap: usize) -> (Fh, u64) {
    let cfg = FeedCfg {
        routes: FeedRoutes::Bbo,
        ring_full_park_us: 200,
        ..f0()
    };
    let mut h = feed_with::<StubProto>(
        cfg,
        Caps {
            bbo: cap,
            ..Caps::default()
        },
    );
    let mut now = 100;
    h.up(&[S1], streams::BBO, &mut now);
    h.clear();
    for u in 17..=23 {
        h.net.at(ConnId(0), 0, Ev::Text(b("S1", u)));
    }
    h.poll(now + 1);
    (h, now)
}

#[test]
fn ven_070_bbo_ring_full() {
    let (mut h, _) = full(4);
    let ls = h.poll(1_000_000);
    assert_eq!(
        (ls.did_work, ls.ring_full, ls.next_deadline),
        (false, 1, Some(1_200_000))
    );
    let first: Vec<u64> = bbos(&h.drain_bbo()).iter().map(|x| x.update_id).collect();
    assert_eq!(first, vec![17, 18, 19, 20]);
    h.poll(1_200_000);
    let rest: Vec<u64> = bbos(&h.drain_bbo()).iter().map(|x| x.update_id).collect();
    assert_eq!(rest, vec![21, 22, 23]);
    // The same input with a 1024-slot ring gives the same sequence.
    let (mut big, _) = full(1024);
    big.poll(1_000_000);
    let all: Vec<u64> = bbos(&big.drain_bbo()).iter().map(|x| x.update_id).collect();
    assert_eq!(all, (17..=23).collect::<Vec<_>>());
}

#[test]
fn ven_071_has_pending_ring_full() {
    let (mut h, _) = full(4);
    h.poll(1_000_000);
    assert!(!PollTask::has_pending(&h.rt), "step 1");
    h.cmd
        .try_push(FeedCmd::Subscribe {
            inst: S2,
            streams: streams::BBO,
        })
        .unwrap();
    assert!(PollTask::has_pending(&h.rt), "step 2");
    h.poll(1_000_001);
    for _ in 0..4 {
        h.bbo.as_mut().unwrap().try_pop().unwrap();
    }
    assert!(PollTask::has_pending(&h.rt), "step 3");
}

enum Task<'a> {
    Feed(&'a mut FeedRuntime<StubProto>),
    Tick(u64),
}

impl PollTask for Task<'_> {
    fn poll_once(&mut self, now: Tsc) -> Polled {
        match self {
            Task::Feed(f) => PollTask::poll_once(&mut **f, now),
            Task::Tick(n) => {
                *n += 1;
                Polled { did_work: false }
            }
        }
    }
    fn next_deadline(&self) -> Option<Tsc> {
        match self {
            Task::Feed(f) => PollTask::next_deadline(&**f),
            Task::Tick(_) => None,
        }
    }
    fn has_pending(&self) -> bool {
        match self {
            Task::Feed(f) => PollTask::has_pending(&**f),
            Task::Tick(_) => false,
        }
    }
}

#[test]
fn ven_072_ring_full_parks() {
    let (mut h, _) = full(4);
    let clock = TscClock::from_hz_milli(1_000_000_000_000, CalibPair::default());
    let mut lp = TaskLoop::new(
        [Task::Feed(&mut h.rt), Task::Tick(0)],
        IdlePolicy::SpinThenPark {
            spin_us: 10,
            park_us: 1000,
        },
        None,
        &clock,
    );
    let mut now = 2_000_000u64;
    for _ in 0..100 {
        now += 1000;
        let did = lp.round(Tsc(now));
        assert!(!did, "the feed task is idle while its ring is full");
        let _ = lp.idle_action(Tsc(now), did);
    }
    let Task::Tick(n) = lp.tasks()[1] else { panic!() };
    assert_eq!(n, 100);
    let a = lp.idle_action(Tsc(now), false);
    assert_eq!(a, IdleAction::Park { timeout_ns: 200_000 });
    let _ = (
        stub::cfg(),
        FeedMsg::Bulk {
            kind: hft_event::EventKind::BBO,
            len: 0,
        },
    );
}
