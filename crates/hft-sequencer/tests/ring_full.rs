//! Out rings full (TESTS.md "出环满": SEQ-224–SEQ-225).

use hft_event::{EventKind, FeedCmd, Role, SendReport, SendStatus};
use hft_sequencer::OutRing;
use hft_strategy_sdk::EventRef;
use hft_types::{Side, Tif};

use crate::common::*;

fn steady() -> H {
    reset_script();
    let mut h = b0().boot();
    h.steady();
    h
}

#[test]
fn seq_224_ring_full_synth_send_report() {
    let mut h = steady();
    h.io().out_free.insert(OutRing::Venue(B), 0);
    add_hook(|hx, ev, ctx| {
        if let EventRef::Bbo { inst, b, .. } = ev
            && *inst == A1
            && hx.s.strategy == S1
            && b.ask_px == px("50029")
        {
            let bb = hx.v.bbo(B1);
            let mut it = intent(hx, B1, Side::Sell, Tif::Ioc, hft_strategy_sdk::IntentClass::Open, bb.bid_px, q("0.001"));
            it.group = G1;
            let _ = ctx.place(it);
        }
    });
    let m = h.mark();
    h.quote(A1, "50000", "50029");
    h.run();
    let recs = h.since(m);
    let d = recs.iter().position(|r| r.kind == EventKind::ORDER_DECISION).expect("decision");
    let next_input = recs.iter().skip(d + 1).find(|r| r.role() == Role::Input).expect("next input");
    assert_eq!(next_input.kind, EventKind::SEND_REPORT);
    let sr = next_input.read::<SendReport>().unwrap();
    assert_eq!(sr.status, SendStatus::RingFull);
    let v = h.verify();
    assert!(v.is_empty(), "{v:?}");
}

#[test]
fn seq_225_feedctl_staging() {
    reset_script();
    let mut h = b0().boot();
    // Room for 1 FeedCmd: the startup subscriptions are staged and drained round by round.
    h.io().feed_free = Some(1);
    h.step();
    let first = h.io_ref().feed.len();
    assert_eq!(first, 1);
    for _ in 0..20 {
        h.io().feed_free = Some(1);
        h.step();
    }
    let feeds = h.io_ref().feed.clone();
    let mut insts: Vec<_> = feeds
        .iter()
        .filter_map(|(_, c)| match c {
            FeedCmd::Subscribe { inst, .. } => Some(*inst),
            _ => None,
        })
        .collect();
    let n = insts.len();
    insts.sort_by_key(|i| i.0);
    insts.dedup();
    assert_eq!(insts.len(), n, "one Subscribe per inst");
    assert!(insts.contains(&A1) && insts.contains(&B1));
    let v = h.verify();
    assert!(v.is_empty(), "{v:?}");
}
