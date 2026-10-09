//! Storage degradation (TESTS.md SEQ-276).

use hft_event::{GwOp, OrderDecision};
use hft_sequencer::StepOutcome;
use hft_sequencer::driver::live::RingId;
use hft_strategy_sdk::{EventRef, IntentClass};
use hft_types::{CapSource, Side, Tif};

use crate::common::*;

const PERSIST_LAG_1: u32 = 12 | 1 << 8;
const STORAGE_OK: u32 = 14;
const DISK_LOW_2: u32 = 13 | 2 << 8;

fn cap_code(name: &str) -> u32 {
    hft_risk::NOTE_CODES
        .iter()
        .find(|x| x.2 == name)
        .map(|x| ((x.0 as u32) << 24) | x.1)
        .unwrap_or(0)
}

/// s1 places an Open buy and a Close sell of a1 on the next a1 Bbo; returns (open, close).
fn open_and_close(h: &mut H, ask: &str) -> (OrderDecision, OrderDecision) {
    set_hook(|hx, ev, ctx| {
        if let EventRef::Bbo { inst, .. } = ev
            && *inst == A1
            && hx.s.strategy == S1
        {
            let b = hx.v.bbo(A1);
            let _ = ctx.place(intent(
                hx,
                A1,
                Side::Buy,
                Tif::Ioc,
                IntentClass::Open,
                b.ask_px,
                q("0.001"),
            ));
            let _ = ctx.place(intent(
                hx,
                A1,
                Side::Sell,
                Tif::Ioc,
                IntentClass::Reduce,
                b.bid_px,
                q("0.001"),
            ));
        }
    });
    let m = h.mark();
    h.quote(A1, "50000", ask);
    h.run();
    clear_hook();
    let ds = decisions(&h.since(m));
    let open = ds.iter().find(|d| d.1.side == Side::Buy).expect("open").1;
    let close = ds.iter().find(|d| d.1.side == Side::Sell).expect("close").1;
    (open, close)
}

#[test]
fn seq_276_storage_levels() {
    reset_script();
    let mut h = b0().boot();
    h.steady();
    // A hedged g1 (linked first leg a1 + OnFill hedge b1; no naked leg, nothing reduces it).
    set_hook(|hx, ev, ctx| {
        if let EventRef::Bbo { inst, .. } = ev
            && *inst == A1
            && hx.s.strategy == S1
        {
            let _ = ctx.place(pair_first(hx, q("0.002")));
        }
    });
    let m = h.mark();
    h.quote(A1, "50000", "50001");
    h.run();
    clear_hook();
    let first = decisions(&h.since(m))
        .into_iter()
        .find(|d| d.1.inst == A1)
        .expect("first leg")
        .1;
    h.ack(&first);
    let m = h.mark();
    h.fill(&first, first.qty, first.qty, first.px);
    let hedge = decisions(&h.since(m))
        .into_iter()
        .find(|d| d.1.inst == B1)
        .expect("hedge")
        .1;
    h.ack(&hedge);
    h.fill(&hedge, hedge.qty, hedge.qty, hedge.px);
    assert_eq!(h.s.engine().ledger.pos(AC1, A1).qty, q("0.002"));
    // .1 PersistLag level 1: a Storage cap; Open rejected, Close released.
    let m = h.mark();
    h.sys(PERSIST_LAG_1);
    assert!(h.notes_since(m).iter().any(|n| n.code == cap_code("CapSet")));
    assert!(
        h.s.engine()
            .risk
            .mode
            .caps()
            .iter()
            .any(|c| c.source == CapSource::STORAGE)
    );
    let (o, c) = open_and_close(&mut h, "50002");
    assert_ne!(o.reject, 0, "{o:?}");
    assert_eq!(c.reject, 0, "{c:?}");
    let ev = order_event(&c, hft_event::OrderEventKind::Expired, h.epoch());
    let r = h.rep_ring(AC1);
    h.push(r, &ev);
    h.run();
    // .2 StorageOk: cleared, Open released.
    let m = h.mark();
    h.sys(STORAGE_OK);
    assert!(h.notes_since(m).iter().any(|n| n.code == cap_code("CapCleared")));
    let (o, c) = open_and_close(&mut h, "50003");
    assert_eq!(o.reject, 0, "{o:?}");
    for d in [o, c] {
        let ev = order_event(&d, hft_event::OrderEventKind::Expired, h.epoch());
        let r = h.rep_ring(AC1);
        h.push(r, &ev);
    }
    h.run();
    // .3 DiskLow level 2: an internal Drain with cancel_resting and venue-level CancelAll; ends with
    // Done{restart: false, cause: StorageCritical}.
    let n = h.io_ref().out.len();
    let t = h.now();
    h.push(RingId::Hk, &sys_timer(DISK_LOW_2, t));
    let mut last = StepOutcome::Idle;
    for _ in 0..1_000 {
        last = h.step();
        if matches!(last, StepOutcome::Done { .. } | StepOutcome::Fatal(_)) {
            break;
        }
        if last == StepOutcome::Idle {
            // Resting orders of the drain get their cancel acks; a Timer drives the wait.
            h.adv(1_000_000);
        }
    }
    let d =
        h.s.engine()
            .risk
            .mode
            .caps()
            .iter()
            .find(|c| c.source == CapSource::DRAINING)
            .copied();
    assert!(d.is_some_and(|c| c.params[0] == 1), "{d:?}");
    let out: Vec<_> = h.io_ref().out[n..].to_vec();
    assert!(out.iter().all(|(_, c)| c.op != GwOp::Place), "{out:?}");
    assert_eq!(
        last,
        StepOutcome::Done {
            restart: false,
            cause: hft_sequencer::DoneCause::StorageCritical
        },
        "{last:?}"
    );
}
