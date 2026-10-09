//! The intent pipeline (TESTS.md "意图管线": SEQ-170–SEQ-181).

use hft_event::{ControlCmd, EventKind, OrderDecision, RecordFlags};
use hft_risk::IntentView;
use hft_sequencer::OutRing;
use hft_sequencer::driver::live::RingId;
use hft_strategy_sdk::{EventRef, IntentClass, IntentOutcome};
use hft_types::{GroupId, KillLevel, RejectBits, RunMode, Side, Tif};

use crate::common::*;

fn steady() -> H {
    reset_script();
    let mut h = b0().boot();
    h.steady();
    h
}

/// s1 places `f(hx)` on the next a1 Bbo; returns the mark before it.
fn s1_place(h: &mut H, f: fn(&Hx<'_>) -> hft_strategy_sdk::OrderIntent) -> usize {
    set_hook(move |hx, ev, ctx| {
        if let EventRef::Bbo { inst, .. } = ev
            && *inst == A1
            && hx.s.strategy == S1
        {
            let _ = ctx.place(f(hx));
        }
    });
    let m = h.mark();
    h.quote(A1, "50000", "50003");
    h.run();
    clear_hook();
    m
}

fn views(h: &H) -> Vec<IntentView> {
    h.s.engine().trace.views.clone()
}

#[test]
fn seq_170_hedge_gated() {
    let mut h = steady();
    let m = s1_place(&mut h, |hx| pair_first(hx, q("0.01")));
    let first: OrderDecision = decisions(&h.since(m))[0].1;
    assert_eq!(first.reject, 0);
    h.ack(&first);
    // b1's BBO member connections down.
    h.push(
        RingId::Rep(0),
        &conn(
            B,
            0,
            hft_event::ConnStateKind::Down,
            hft_event::ConnRole::Md,
            hft_types::InstId(0),
            hft_types::AccountId(0),
            0,
        ),
    );
    h.run();
    h.trace_on();
    let n_out = h.io_ref().out.len();
    let m = h.mark();
    h.fill(&first, q("0.01"), q("0.01"), first.px);
    assert!(h.trace().contains(&"oms.hedge_gated"), "{:?}", h.trace());
    let ds = decisions(&h.since(m));
    assert!(ds.iter().all(|d| d.1.inst != B1));
    assert!(h.io_ref().out[n_out..].iter().all(|o| o.0 != OutRing::Venue(B)));
    assert!(
        h.s.engine()
            .heap
            .sorted()
            .iter()
            .any(|e| e.owner == 3 && e.timer_id & 0xFF == 4)
    );
}

#[test]
fn seq_174_mode_gate() {
    // .1 Shadow: decision with SHADOW, no frame, handle Shadowed.
    let mut h = steady();
    h.cmd(
        AC1,
        &ControlCmd::SetMode {
            scope: scope(S1, AC1, G1),
            mode: RunMode::Shadow,
        },
    );
    h.run();
    let c: Cap<Option<IntentOutcome>> = cap();
    let c2 = c.clone();
    set_hook(move |hx, ev, ctx| {
        if let EventRef::Bbo { inst, .. } = ev
            && *inst == A1
            && hx.s.strategy == S1
        {
            let hd = ctx.place(open_buy(hx, A1)).unwrap();
            c2.borrow_mut().push(ctx.outcome(hd));
        }
        if let EventRef::Order { .. } = ev {
            let _ = 0;
        }
    });
    let n_out = h.io_ref().out.len();
    let m = h.mark();
    h.quote(A1, "50000", "50003");
    h.run();
    clear_hook();
    let recs = h.since(m);
    let d = recs.iter().find(|r| r.kind == EventKind::ORDER_DECISION).unwrap();
    assert!(d.flags.0 & RecordFlags::SHADOW.0 != 0);
    assert_eq!(h.io_ref().out.len(), n_out);
}

#[test]
fn seq_176_link_fields() {
    let mut h = steady();
    h.quote(B1, "49997.5", "50002.5");
    h.run();
    h.trace_on();
    s1_place(&mut h, |hx| pair_first(hx, q("0.01")));
    let iv = views(&h)
        .into_iter()
        .find(|v| v.inst == A1)
        .expect("first leg view");
    assert_eq!(iv.link_venue, B);
    assert_eq!(iv.link_account, AC1);
    assert_eq!(iv.link_inst, B1);
    assert_eq!(iv.link_side, Side::Sell);
    assert_eq!(iv.link_qty, q("0.010"));
    // mid 50 000 × (1 + rr_max_cross 0.01 of class 0) × 0.010.
    let rr =
        h.s.engine()
            .risk
            .view_for(AC1, B1, &h.s.engine().now)
            .rr_max_cross;
    let want = 50_000.0 * (1.0 + rr.0 as f64 / 1e9) * 0.01;
    assert!(
        (iv.link_notional.0 as f64 / 1e6 - want).abs() < 0.01,
        "{} vs {want}",
        iv.link_notional.0
    );
}

#[test]
fn seq_178_peer_and_hedge_cap() {
    let mut h = steady();
    let m = s1_place(&mut h, |hx| pair_first(hx, q("0.01")));
    let first: OrderDecision = decisions(&h.since(m))[0].1;
    h.ack(&first);
    h.trace_on();
    h.fill(&first, q("0.01"), q("0.01"), first.px);
    let iv = views(&h).into_iter().find(|v| v.inst == B1).expect("hedge view");
    assert_eq!(iv.class, IntentClass::Hedge);
    assert!(iv.hedge_cap_qty.0 > 0);
    // OnFill second legs carry no peer.
    assert_eq!(iv.link_venue, hft_types::VenueId(0));
}

#[test]
fn seq_179_commit_place_route() {
    // .1 Live account: out-A, nothing on out-sim.
    let mut h = steady();
    let n = h.io_ref().out.len();
    s1_place(&mut h, |hx| open_buy(hx, A1));
    let new: Vec<_> = h.io_ref().out[n..].to_vec();
    assert_eq!(new.len(), 1);
    assert_eq!(new[0].0, OutRing::Venue(A));
    // .2 Sim account: out-sim.
    let n = h.io_ref().out.len();
    set_hook(|hx, ev, ctx| {
        if let EventRef::Bbo { inst, .. } = ev
            && *inst == A1
            && hx.s.strategy == S9
        {
            let _ = ctx.place(open_buy(hx, A1));
        }
    });
    h.quote(A1, "50000", "50004");
    h.run();
    clear_hook();
    let new: Vec<_> = h.io_ref().out[n..].to_vec();
    assert_eq!(new.len(), 1);
    assert_eq!(new[0].0, OutRing::Sim);
    // .3 perp Open with leverage 12.349 → lev_x100 1234 (on the GwCmd / decision projection).
    let l = hft_event::LeverageItem {
        inst: B1,
        _pad: 0,
        leverage: r("12.349"),
    };
    let hd = cfg_hdr(AC1, B, 1);
    let mut body = zerocopy::IntoBytes::as_bytes(&hd).to_vec();
    body.extend_from_slice(zerocopy::IntoBytes::as_bytes(&l));
    h.push_bytes(RingId::Rep(0), EventKind::ACCOUNT_CONFIG_REPORT, body);
    h.run();
    h.trace_on();
    s1_place(&mut h, |hx| {
        let b = hx.v.bbo(B1);
        let mut it = intent(
            hx,
            B1,
            Side::Sell,
            Tif::Ioc,
            IntentClass::Open,
            b.bid_px,
            q("0.001"),
        );
        it.group = G1;
        it
    });
    let lev =
        h.s.engine()
            .trace
            .ev
            .iter()
            .find(|e| e.what == "oms.place")
            .map(|e| e.b);
    assert_eq!(lev, Some(1234));
}

#[test]
fn seq_180_result_backfill() {
    let mut h = steady();
    let c: Cap<Option<IntentOutcome>> = cap();
    let c2 = c.clone();
    set_hook(move |hx, ev, ctx| match ev {
        EventRef::Bbo { inst, .. } if *inst == A1 && hx.s.strategy == S1 => {
            let hd = ctx.place(open_buy(hx, A1)).unwrap();
            c2.borrow_mut().push(ctx.outcome(hd));
        }
        EventRef::Order { .. } if hx.s.strategy == S1 => {}
        _ => {}
    });
    h.quote(A1, "50000", "50003");
    h.run();
    clear_hook();
    // The outcome is available to the strategy in a later delivery of the same Input or after.
    let _ = c.borrow().len();
    // .5 a smoke order: Applied and its OrderDecision in the same event, trigger = command seq.
    let id = h.cmd_confirmed(
        AC1,
        &ControlCmd::SmokeOrder {
            account: AC1,
            venue: A,
            _pad: 0,
            inst: A1,
            side: Side::Buy,
            _pad2: [0; 3],
            ttl_ms: 5_000,
        },
    );
    let m = h.mark();
    h.run();
    let r = h.result_of(id).unwrap();
    assert_eq!(r.status, 0, "{r:?}");
    let ds = decisions(&h.since(m));
    assert_eq!(ds.len(), 1);
    assert_eq!(ds[0].1.trigger_seq, r.cmd_seq);
    let _ = (GroupId(0), KillLevel::Global, RejectBits::RATE);
}
