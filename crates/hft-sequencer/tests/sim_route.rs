//! Sim accounts (TESTS.md "模拟账户": SEQ-214–SEQ-216).

use hft_event::EventKind;
use hft_sequencer::OutRing;
use hft_strategy_sdk::EventRef;
use hft_types::Side;

use crate::common::*;

fn steady() -> H {
    reset_script();
    let mut h = b0().boot();
    h.steady();
    h
}

#[test]
fn seq_214_sim_orders_out_sim() {
    let mut h = steady();
    let n = h.io_ref().out.len();
    let d = h.open_position(AC9, A1, Side::Buy, "0.001");
    let new: Vec<_> = h.io_ref().out[n..].to_vec();
    assert_eq!(new.len(), 1);
    assert_eq!(new[0].0, OutRing::Sim);
    // The ack and the fill were journaled as Inputs (rep-sim).
    let evs = h
        .recs()
        .into_iter()
        .filter(|r| r.kind == EventKind::ORDER_EVENT)
        .count();
    assert!(evs >= 2);
    assert_eq!(d.account, AC9);
    let m = h.verify();
    assert!(m.is_empty(), "{m:?}");
}

#[test]
fn seq_215_sim_no_periodic_timers() {
    reset_script();
    let mut b = b0();
    b.rd.caps[1].dms = hft_refdata::DeadManCap::CountdownAccount;
    b.rd.caps[1].dms_refresh_ms = 10_000;
    b.rd.caps[1].dms_timeout_ms = 30_000;
    let mut h = b.boot();
    h.step();
    let heap = h.s.engine().heap.sorted();
    let dead: Vec<u32> = heap
        .iter()
        .filter(|e| e.owner == 1 && e.timer_id & 0xFF == 4)
        .map(|e| e.timer_id)
        .collect();
    let acc = |id: u32| (id >> 16) & 0xFF;
    assert!(dead.iter().any(|id| acc(*id) == 1));
    assert!(dead.iter().any(|id| acc(*id) == 2));
    assert!(dead.iter().all(|id| acc(*id) != 9));
    let rot: Vec<u32> = heap
        .iter()
        .filter(|e| e.owner == 1 && e.timer_id & 0xFF == 5)
        .map(|e| (e.timer_id >> 8) & 0xFF)
        .collect();
    assert_eq!(rot, vec![u32::from(A.0), u32::from(B.0)]);
}

#[test]
fn seq_216_live_before_sim() {
    let mut h = steady();
    for s in [S1, S9] {
        add_hook(move |hx, ev, ctx| {
            if let EventRef::Bbo { inst, b, .. } = ev
                && *inst == A1
                && hx.s.strategy == s
                && b.ask_px == px("50009")
            {
                let _ = ctx.place(open_buy(hx, A1));
            }
        });
    }
    clear_log();
    let n = h.io_ref().out.len();
    let m = h.mark();
    h.quote(A1, "50000", "50009");
    h.run();
    let order: Vec<_> = log()
        .iter()
        .filter(|g| g.kind == "Bbo" && !g.model && (g.s == S1 || g.s == S9))
        .map(|g| g.s)
        .collect();
    assert_eq!(order, vec![S1, S9]);
    let ds = decisions(&h.since(m));
    assert_eq!(ds[0].1.account, AC1);
    assert_eq!(ds[1].1.account, AC9);
    let out: Vec<_> = h.io_ref().out[n..].iter().map(|o| o.0).collect();
    assert_eq!(out, vec![OutRing::Venue(A), OutRing::Sim]);
}
