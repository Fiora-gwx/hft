//! Drain (TESTS.md SEQ-305).

use hft_event::{ControlCmd, GwOp};
use hft_sequencer::codes::{detail, status};
use hft_strategy_sdk::EventRef;
use hft_types::{AccountId, CapSource, GroupId, Side, StrategyId, Tif};

use crate::common::*;

/// B0 with batch cancel on both venues (a venue-level `CancelAll` needs it).
fn steady() -> H {
    reset_script();
    let mut b = b0();
    for c in b.rd.caps.iter_mut() {
        c.flags = c.flags | hft_refdata::CapFlags::BATCH_CANCEL;
    }
    let mut h = b.boot();
    h.steady();
    h
}

/// Account 1: two plain GTC buys on A, one linked GTC first leg on A, one plain GTC buy on B;
/// all acked. Returns the linked one's decision.
fn resting(h: &mut H) -> hft_event::OrderDecision {
    set_hook(|hx, ev, ctx| {
        if let EventRef::Bbo { inst, .. } = ev
            && *inst == A1
            && hx.s.strategy == S1
        {
            let b = hx.v.bbo(A1);
            let p = hft_types::Px(b.bid_px.0 - 500);
            for _ in 0..2 {
                let _ = ctx.place(intent(
                    hx,
                    A1,
                    Side::Buy,
                    Tif::Gtc,
                    hft_strategy_sdk::IntentClass::Open,
                    p,
                    q("0.001"),
                ));
            }
            let mut l = pair_first(hx, q("0.001"));
            l.tif = Tif::Gtc;
            l.px = p;
            let _ = ctx.place(l);
            let bb = hx.v.bbo(B1);
            let _ = ctx.place(intent(
                hx,
                B1,
                Side::Buy,
                Tif::Gtc,
                hft_strategy_sdk::IntentClass::Open,
                hft_types::Px(bb.bid_px.0 - 500),
                q("0.001"),
            ));
        }
    });
    let m = h.mark();
    h.quote(A1, "50000", "50001");
    h.run();
    clear_hook();
    let ds = decisions(&h.since(m));
    assert_eq!(ds.len(), 4, "{ds:?}");
    for d in &ds {
        assert_eq!(d.1.reject, 0, "{:?}", d.1);
        h.ack(&d.1);
    }
    ds.iter()
        .find(|d| d.1.link_role != 0)
        .expect("linked first leg")
        .1
}

fn drain(h: &mut H, issuer: &str, cancel_resting: bool) -> [u8; 16] {
    let id = h.cmd_with(
        issuer,
        ADMIN,
        AccountId(0),
        &ControlCmd::Drain { cancel_resting },
        |x| x.confirmed = 1,
    );
    h.run();
    id
}

fn draining_param(h: &H) -> Option<u64> {
    h.s.engine()
        .risk
        .mode
        .caps()
        .iter()
        .find(|c| c.source == CapSource::DRAINING)
        .map(|c| c.params[0])
}

#[test]
fn seq_305_drain_cancel() {
    // .1 cancel_resting: a venue-level CancelAll on A and on B.
    let mut h = steady();
    resting(&mut h);
    let n = h.io_ref().out.len();
    let id = drain(&mut h, "cli:ops", true);
    assert_eq!(h.res(id).0, status::APPLIED);
    assert_eq!(draining_param(&h), Some(1));
    let out: Vec<_> = h.io_ref().out[n..].to_vec();
    for v in [A, B] {
        assert!(
            out.iter()
                .any(|(r, c)| *r == hft_sequencer::types::OutRing::Venue(v)
                    && c.op == GwOp::CancelAll
                    && c.account == AC1),
            "CancelAll on {v:?}: {out:?}"
        );
    }
    // .3 Draining is not resumable.
    let id = h.cmd_confirmed(
        AccountId(0),
        &ControlCmd::Resume {
            scope: scope(StrategyId(0), AccountId(0), GroupId(0)),
            source: CapSource::DRAINING,
        },
    );
    h.run();
    assert_eq!(
        h.res(id),
        (status::INVALID_ARGS, u32::from(detail::NOT_RESUMABLE))
    );
    // .2 / .4 without cancel_resting (also from cli:systemd): only the linked first leg.
    for issuer in ["cli:ops", "cli:systemd"] {
        let mut h = steady();
        let l = resting(&mut h);
        let n = h.io_ref().out.len();
        let id = drain(&mut h, issuer, false);
        assert_eq!(h.res(id).0, status::APPLIED, "{issuer}");
        assert_eq!(draining_param(&h), Some(0));
        let out: Vec<_> = h.io_ref().out[n..].to_vec();
        let cancels: Vec<_> = out
            .iter()
            .filter(|(_, c)| matches!(c.op, GwOp::Cancel | GwOp::CancelAll))
            .collect();
        assert_eq!(cancels.len(), 1, "{issuer}: {out:?}");
        assert_eq!(cancels[0].1.op, GwOp::Cancel);
        assert_eq!(cancels[0].1.target_slot, l.slot);
    }
}

#[test]
fn seq_370_done_publishes_the_last_event() {
    // Drain without cancel_resting and with nothing in flight: Done in the command's event, which
    // emits nothing to a gw.
    let mut h = steady();
    let p0 = h.j().publishes;
    let id = h.cmd_with(
        "cli:ops",
        ADMIN,
        AccountId(0),
        &ControlCmd::Drain {
            cancel_resting: false,
        },
        |x| x.confirmed = 1,
    );
    let o = h.run();
    assert!(matches!(o, hft_sequencer::StepOutcome::Done { .. }), "{o:?}");
    assert_eq!(h.res(id).0, status::APPLIED);
    assert_eq!(h.j().committed, h.j().len(), "SEQ-370: everything published");
    assert!(h.j().publishes > p0);
    let last = h
        .j()
        .published()
        .filter(|r| r.kind == hft_event::EventKind::STATE_SNAPSHOT)
        .last()
        .cloned()
        .expect("a published snapshot");
    assert_eq!(
        snap(&last).0.purpose,
        hft_sequencer::snapshot::PURPOSE_DRAIN,
        "SEQ-370: the Drain snapshot is published"
    );
}
