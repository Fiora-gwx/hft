//! Market subscriptions (SDK-033–SDK-036).

use hft_event::{EventKind, FeedCmd, Note, NoteSource};
use hft_refdata::Exchange;
use hft_sequencer::codes::status;
use hft_sequencer::driver::live::RingId;
use hft_strategy_sdk::{EvMask, EventRef, NoteLevel, Part, RefView, SubsErr};
use hft_types::{InstId, VenueId};

use super::*;
use crate::common::*;

/// Venue with a feed in this instance but outside acct-1's venues (`V3`) and one without a feed
/// (`V4`); their BTC spot insts are `I3` and `I4`.
const C: VenueId = VenueId::OKX_SPOT;
const D: VenueId = VenueId::BYBIT_SPOT;
const I3: InstId = InstId(24);
const I4: InstId = InstId(25);

/// `b_gt3` plus venues C (feed) and D (no feed) with one BTC spot inst each.
fn b_ref() -> B0 {
    let mut b = b_gt3();
    b.rd.extra_venues = vec![(C, Exchange::Okx), (D, Exchange::Bybit)];
    b.rd.caps.push(caps_spec(C, false));
    b.rd.caps.push(caps_spec(D, false));
    b.rd.insts.push(inst_spec(
        I3,
        C,
        "BTC-USDT",
        false,
        BTC,
        ("0.01", 1),
        ("0.0001", 10_000),
    ));
    b.rd.insts.push(inst_spec(
        I4,
        D,
        "BTCUSDT",
        false,
        BTC,
        ("0.01", 1),
        ("0.0001", 10_000),
    ));
    b.rd.limits.extend(order_limits(C, 100, 600));
    b.rd.limits.extend(order_limits(D, 100, 600));
    b.setup_edit
        .push(|s| s.feeds = hft_strategy_sdk::VenueSet(vbits(&[A, B, C])));
    b
}

/// A new s1 parameter version (non-structural change) so `subscriptions` runs again.
fn reparam(h: &mut H, version: u64) {
    let mut p = gt3(version);
    p.values[F_THR] = 5 + version as i64;
    let id = h.set_params(&p);
    h.run();
    assert_eq!(h.res(id), (status::APPLIED, 0));
}

fn subs_results() -> Vec<(hft_types::StrategyId, InstId, u16, Result<(), SubsErr>)> {
    SCRIPT.with(|x| x.borrow().subs_results.clone())
}

#[test]
fn sdk_033_multi_group_same_inst_gix_order() {
    let mut h = rig();
    set_subs(
        S1,
        vec![(A1, 2, EvMask::BBO, Part::Fast), (A1, 0, EvMask::BBO, Part::Fast)],
    );
    reparam(&mut h, 2);
    clear_log();
    h.quote(A1, "50000", "50002");
    h.run();
    let got: Vec<Got> = fast(S1).into_iter().filter(|g| g.kind == "Bbo").collect();
    assert_eq!(got.iter().map(|g| g.gix).collect::<Vec<_>>(), vec![0, 2]);
    assert_eq!(got[0].bbo, got[1].bbo);
    assert!(got[0].bbo.is_some());
}

#[test]
fn sdk_034_reference_market_subscription() {
    let mut h = b_ref().boot();
    h.steady();
    let rv = RefView::new(&h.rd, AC1);
    // Every BTC inst in ascending id order, including venues outside acct-1's {A, B}.
    assert_eq!(rv.insts_of(BTC), &[A1, A3, B1, I3, I4]);
    set_subs(
        S1,
        vec![(A1, 0, EvMask::BBO, Part::Fast), (I3, 0, EvMask::BBO, Part::Fast)],
    );
    let n = h.io_ref().feed.len();
    reparam(&mut h, 2);
    assert!(
        subs_results()
            .iter()
            .any(|r| r.0 == S1 && r.1 == I3 && r.3 == Ok(()))
    );
    let new: Vec<(VenueId, FeedCmd)> = h.io_ref().feed[n..].to_vec();
    assert!(
        new.iter().any(|(v, c)| *v == C
            && matches!(c, FeedCmd::Subscribe { inst, streams } if *inst == I3 && streams & EvMask::BBO.0 != 0)),
        "{new:?}"
    );
    clear_log();
    let t = h.now();
    h.push(RingId::Bbo(1), &bbo(I3, C, 0, "50000", "50001", 77, t));
    h.run();
    let got: Vec<Got> = fast(S1).into_iter().filter(|g| g.kind == "Bbo").collect();
    assert_eq!(got.len(), 1);
    assert_eq!((got[0].inst, got[0].gix), (I3, 0));
}

#[test]
fn sdk_035_no_feed_does_not_fail_params() {
    let mut h = b_ref().boot();
    h.steady();
    set_subs(
        S1,
        vec![(A1, 0, EvMask::BBO, Part::Fast), (I4, 0, EvMask::BBO, Part::Fast)],
    );
    let n = h.io_ref().feed.len();
    reparam(&mut h, 2);
    let r: Vec<_> = subs_results()
        .into_iter()
        .filter(|r| r.0 == S1 && r.1 == I4)
        .collect();
    assert_eq!(r.last().map(|x| x.3), Some(Err(SubsErr::NoFeed)));
    assert!(
        h.io_ref().feed[n..]
            .iter()
            .all(|(_, c)| !matches!(c, FeedCmd::Subscribe { inst, .. } if *inst == I4))
    );
    // `subscriptions` has no Ctx: the strategy writes its Note on the next event.
    set_hook(|hx, ev, ctx| {
        if let EventRef::Bbo { .. } = ev
            && hx.s.strategy == S1
            && SCRIPT.with(|x| {
                x.borrow()
                    .subs_results
                    .iter()
                    .any(|r| r.0 == S1 && r.3 == Err(SubsErr::NoFeed))
            })
        {
            ctx.note(3, NoteLevel::P2, GT1, I4, [0; 8]);
        }
    });
    let m = h.mark();
    h.quote(A1, "50000", "50002");
    h.run();
    clear_hook();
    let notes: Vec<Note> = h
        .kinds_since(m, EventKind::NOTE)
        .iter()
        .filter_map(|r| r.read::<Note>())
        .filter(|x| x.source == NoteSource::Strategy)
        .collect();
    assert_eq!(notes.len(), 1, "{notes:?}");
    assert_eq!(notes[0].code, (2 << 24) | 3);
    assert_eq!(notes[0].strategy, S1);
    assert_eq!(notes[0].group, GT1);
    assert_eq!(notes[0].inst, I4);
}

#[test]
fn sdk_036_subs_full_does_not_fail_params() {
    // Two entries per inst. Subscriptions are rebuilt in dispatch order (s1 before s2), so s1's
    // one a1 entry (its GT1 leg) is taken first; s2 (two groups, `S3` of the case) then asks for
    // a1 at gix 0 and gix 1. s9 has no parameters (a1 is referenced by GT1 and G2 only).
    let mut b = b_gt3();
    b.params.retain(|p| p.strategy != S9);
    b.caps.max_subs_per_inst = 2;
    let mut h = b.boot();
    h.steady();
    set_subs(
        S2,
        vec![(A1, 0, EvMask::BBO, Part::Fast), (A1, 1, EvMask::BBO, Part::Fast)],
    );
    SCRIPT.with(|x| x.borrow_mut().subs_results.clear());
    let mut p = Spb::s2(2);
    p.groups = vec![gspec(G2), gspec_eth(hft_types::GroupId(0x0200_0002))];
    let id = h.set_params(&p);
    h.run();
    assert_eq!(h.res(id), (status::APPLIED, 0));
    let r: Vec<_> = subs_results()
        .into_iter()
        .filter(|r| r.0 == S2 && r.1 == A1)
        .map(|r| (r.2, r.3))
        .collect();
    assert_eq!(r, vec![(0, Ok(())), (1, Err(SubsErr::Full))]);
    clear_log();
    h.quote(A1, "50000", "50002");
    h.run();
    let got: Vec<u16> = fast(S2)
        .into_iter()
        .filter(|g| g.kind == "Bbo")
        .map(|g| g.gix)
        .collect();
    assert_eq!(got, vec![0]);
    // s1 keeps its entry.
    assert!(fast(S1).iter().any(|g| g.kind == "Bbo" && g.inst == A1));
}
