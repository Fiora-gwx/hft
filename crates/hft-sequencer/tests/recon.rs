//! Reconciliation reports (TESTS.md SEQ-161–SEQ-168).

use hft_event::{EventKind, ReconcileReport};
use hft_sequencer::SnapshotSel;
use hft_sequencer::driver::live::RingId;
use hft_types::{AccountId, AssetId, InstId, Qty};
use zerocopy::FromZeros;

use crate::common::*;

fn item(a: AccountId, v: hft_types::VenueId, id: u64, item: u8, count: u16) -> ReconcileReport {
    let mut x = ReconcileReport::new_zeroed();
    x.report_id = id;
    x.venue = v;
    x.account = a;
    x.item = item;
    x.count = count;
    x
}

fn steady() -> H {
    reset_script();
    let mut h = b0().boot();
    h.steady();
    h
}

fn pending(h: &H, id: u64) -> Option<(u16, u16, u16)> {
    {
        let e = h.s.engine();
        e.cmd_recon_pending(id)
    }
}

#[test]
fn seq_161_funding_items_immediate() {
    let mut h = steady();
    h.trace_on();
    let mut f = item(AC1, B, 900, 6, 3);
    f.inst = InstId(0);
    f.exch_ts_ns = UTC0;
    h.push(RingId::Rep(0), &f);
    h.run();
    assert_eq!(h.s.engine().trace.count("ledger.reconcile_funding"), 1);
    assert_eq!(pending(&h, 900), Some((0, 3, 1)));
    h.trace_on();
    let mut p = item(AC1, B, 900, 1, 3);
    p.inst = B1;
    h.push(RingId::Rep(0), &p);
    h.run();
    assert_eq!(pending(&h, 900), Some((1, 3, 2)));
    assert_eq!(h.s.engine().trace.count("oms.on_reconcile"), 0);
    // SEQ-162 the last item completes the group.
    h.trace_on();
    let mut b = item(AC1, B, 900, 2, 3);
    b.asset = USDT;
    h.push(RingId::Rep(0), &b);
    h.run();
    let tr = h.trace();
    let o = tr
        .iter()
        .position(|x| *x == "oms.on_reconcile")
        .expect("oms.on_reconcile");
    let r = tr
        .iter()
        .position(|x| *x == "risk.on_reconcile")
        .expect("risk.on_reconcile");
    let w = tr
        .iter()
        .position(|x| *x == "ledger.funding_window_sweep")
        .expect("sweep");
    assert!(o < r && r < w);
    assert_eq!(pending(&h, 900), None);
}

#[test]
fn seq_162_group_completion() {
    // Covered with SEQ-161's continuation; this case starts from the same state independently.
    let mut h = steady();
    for (it, n) in [(6u8, 0usize), (1, 1), (2, 2)] {
        let mut x = item(AC1, B, 901, it, 3);
        if it == 1 {
            x.inst = B1;
        }
        if it == 2 {
            x.asset = USDT;
        }
        h.trace_on();
        h.push(RingId::Rep(0), &x);
        h.run();
        if n == 2 {
            assert_eq!(h.s.engine().trace.count("oms.on_reconcile"), 1);
            assert_eq!(h.s.engine().trace.count("ledger.funding_window_sweep"), 1);
        }
    }
    assert_eq!(pending(&h, 901), None);
}

#[test]
fn seq_163_funding_only_group() {
    reset_script();
    let mut h = b0().boot();
    h.step();
    let g0 = h.gate_bits(AC1);
    let m = h.mark();
    for _ in 0..2 {
        let mut f = item(AC1, B, 902, 6, 2);
        f.exch_ts_ns = UTC0 - 3_600_000_000_000;
        h.push(RingId::Rep(0), &f);
        h.run();
    }
    assert_eq!(h.gate_bits(AC1), g0);
    assert!(
        h.notes_since(m)
            .iter()
            .all(|n| n.code != hft_sequencer::codes::seq_note(7))
    );
}

#[test]
fn seq_164_recon_pending_across_restart() {
    let mut h = steady();
    for it in [1u8, 2] {
        let mut x = item(AC1, B, 903, it, 3);
        if it == 1 {
            x.inst = B1;
        }
        if it == 2 {
            x.asset = USDT;
        }
        h.push(RingId::Rep(0), &x);
        h.run();
    }
    h.cmd(AC1, &hft_event::ControlCmd::SnapshotNow);
    h.run();
    h.run();
    let mut h2 = H::crash_restart(h, plan(SnapshotSel::Latest, false, 1, 0), Rerun::default()).unwrap();
    assert_eq!(pending(&h2, 903), Some((2, 3, 2)));
    h2.step();
    h2.trace_on();
    let mut s = item(AC1, B, 903, 5, 3);
    s.view_seq = 1;
    h2.push(RingId::Rep(0), &s);
    h2.run();
    assert_eq!(h2.s.engine().trace.count("oms.on_reconcile"), 1);
    assert_eq!(pending(&h2, 903), None);
    let _ = (AssetId(0), Qty(0), EventKind::NOTE);
}
