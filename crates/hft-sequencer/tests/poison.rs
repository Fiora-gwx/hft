//! Poison inputs (TESTS.md "毒丸输入隔离": SEQ-270, SEQ-271).

use hft_event::{EventKind, ReconcileReport};
use hft_sequencer::driver::live::RingId;
use hft_sequencer::journal::JournalSink;
use hft_sequencer::{RecoveryFlags, SnapshotSel};
use hft_types::Tsc;
use zerocopy::{FromZeros, IntoBytes};

use crate::common::*;

fn report(id: u64) -> ReconcileReport {
    let mut r = ReconcileReport::new_zeroed();
    r.report_id = id;
    r.venue = B;
    r.account = AC1;
    r.item = 2;
    r.count = 2;
    r.asset = USDT;
    r
}

fn input_hash(kind: EventKind, body: &[u8]) -> u64 {
    let mut s = hft_sequencer::types::InputSlot::with_capacity(body.len());
    s.set_bytes(kind, hft_event::RecordFlags(0), Tsc(0), body);
    hft_sequencer::driver::live::input_hash(&s)
}

#[test]
fn seq_270_runtime_poison_quarantine() {
    reset_script();
    let mut h = b0().boot();
    h.steady();
    h.keyframe();
    // The process died while processing R: R is the last record, without EVENT_END.
    let r = report(77);
    let mut j = h.j().clone();
    j.append_var(EventKind::RECONCILE_REPORT, hft_event::RecordFlags(0), Tsc(h.now()), &[r.as_bytes()]).unwrap();
    let hash = input_hash(EventKind::RECONCILE_REPORT, r.as_bytes());
    let setup = h.setup.clone();
    let mut p = plan(SnapshotSel::Latest, true, 1, RecoveryFlags::POISON);
    p.quarantine.push(hash);
    let mut h2 = H::<TestSet>::restart_over(j, setup, p, Rerun::default()).unwrap();
    assert_eq!(h2.s.engine().truncated_input_hash(), Some(hash));
    h2.step();
    let m = h2.mark();
    h2.push(RingId::Rep(0), &r);
    h2.push(RingId::Rep(0), &r);
    h2.push(RingId::Rep(0), &report(78));
    h2.run();
    let recs = h2.since(m);
    let quarantined: Vec<_> = recs
        .iter()
        .filter(|x| x.kind == EventKind::NOTE)
        .filter_map(|x| x.read::<hft_event::Note>())
        .filter(|n| n.code == hft_event::note_code(hft_event::NoteSource::Boot, hft_sequencer::codes::boot_note::INPUT_QUARANTINED))
        .collect();
    assert_eq!(quarantined.len(), 2);
    assert!(quarantined.iter().all(|n| n.v[0] == hash && n.level == hft_event::NoteLevel::P0));
    let reps: Vec<_> = recs.iter().filter(|x| x.kind == EventKind::RECONCILE_REPORT).collect();
    assert_eq!(reps.len(), 1);
    assert_eq!(reps[0].read::<ReconcileReport>().unwrap().report_id, 78);
}

#[test]
fn seq_271_quarantine_process_lifetime() {
    reset_script();
    let mut h = b0().boot();
    h.steady();
    h.keyframe();
    let r = report(77);
    let setup = h.setup.clone();
    let j = h.j().clone();
    let mut h2 = H::<TestSet>::restart_over(j, setup, plan(SnapshotSel::Latest, true, 1, 0), Rerun::default()).unwrap();
    h2.step();
    let m = h2.mark();
    h2.push(RingId::Rep(0), &r);
    h2.run();
    assert!(h2.since(m).iter().any(|x| x.kind == EventKind::RECONCILE_REPORT));
}
