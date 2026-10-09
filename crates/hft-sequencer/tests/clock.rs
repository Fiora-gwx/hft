//! Engine time (TESTS.md "引擎时间": SEQ-104–SEQ-112).

use hft_event::{EventKind, Role};
use hft_sequencer::clock::{EngineClock, EngineClockRow, multiplier, scaled};
use hft_sequencer::driver::live::RingId;
use hft_sequencer::{FatalReason, SnapshotSel, StepOutcome};
use hft_strategy_sdk::EventRef;
use hft_types::Tsc;
use proptest::prelude::*;

use crate::common::*;

fn row(base: i64, hz: u64) -> EngineClockRow {
    EngineClockRow { base_engine_ns: base, base_tsc: 0, tsc_hz_milli: hz, clock_seq: 0 }
}

#[test]
fn seq_104_engine_clock_processing() {
    reset_script();
    let mut h = b0().boot();
    h.steady();
    // Membership and a preview from the previous run.
    let t = h.now();
    h.push(RingId::Rep(0), &conn(A, 1, hft_event::ConnStateKind::Up, hft_event::ConnRole::Md, A1, hft_types::AccountId(0), t));
    h.run();
    h.cmd(AC1, &hft_event::ControlCmd::SnapshotNow);
    h.run();
    h.run();
    let snap_seq = h.snapshots().last().unwrap().seq;
    let p = plan(SnapshotSel::AtSeq(snap_seq), false, 6, 0);
    let mut h2 = H::crash_restart(h, p, Rerun::default()).unwrap();
    h2.trace_on();
    let m = h2.mark();
    h2.step();
    let recs = h2.since(m);
    let ec = recs.iter().find(|r| r.kind == EventKind::ENGINE_CLOCK).unwrap();
    let note = recs.iter().find(|r| r.kind == EventKind::NOTE).unwrap();
    assert_eq!(note.seq, ec.seq + 1);
    let n = note.read::<hft_event::Note>().unwrap();
    assert_eq!(n.v[0], u64::from(h2.s.view().engine_epoch()));
    assert_eq!(n.v[2], ec.seq);
    assert!(note.flags.0 & hft_event::RecordFlags::EVENT_END.0 != 0);
    let e = h2.s.engine();
    assert_eq!(e.clock.row.base_tsc, ec.tsc);
    assert_eq!(e.clock.row.clock_seq, ec.seq);
    assert_eq!(e.boot_seq, note.seq);
    assert!(e.market.conns.iter().all(|s| s.iter().all(|c| !c.used)));
    assert_eq!(e.market.row(A1).unwrap().bbo_member_mask, 0);
    let tr = h2.trace();
    assert_eq!(tr.iter().filter(|x| **x == "ledger.on_run_start").count(), 1);
    assert_eq!(tr.iter().filter(|x| **x == "risk.set_run_start").count(), 1);
}

#[test]
fn seq_105_engine_ns_values() {
    let cases: [(u64, u64, i64); 8] = [
        (1_000_000_000_000, 0, 1_000),
        (1_000_000_000_000, 1, 1_001),
        (3_000_000_000_000, 10, 1_003),
        (3_000_000_000_000, 3, 1_001),
        (1_050_000_000_000, 105, 1_100),
        (1_050_000_000_000, 104, 1_099),
        (8_000_000_000_000, (1 << 58) - 1, 1_000 + 36_028_797_018_963_967),
        (15_625_000_001, 1, 1_063),
    ];
    for (hz, d, want) in cases {
        let c = EngineClock::new(row(1_000, hz)).unwrap();
        assert_eq!(c.ens_of(Tsc(d)).unwrap(), want, "H {hz} Δ {d}");
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(2_000))]
    #[test]
    fn seq_106_mult_shift_exact(d in prop_oneof![Just(0u64), Just(1u64), Just((1u64 << 58) - 1), 0u64..(1u64 << 58)],
                                hz in prop_oneof![Just(15_625_000_001u64), Just(15_625_000_002u64), Just(1_000_000_000_000u64),
                                                  Just(1_050_000_000_000u64), Just(8_000_000_000_000u64),
                                                  15_625_000_001u64..=8_000_000_000_000u64]) {
        let m = multiplier(hz).unwrap();
        let want = (u128::from(d) * 1_000_000_000_000 / u128::from(hz)) as u64;
        prop_assert_eq!(scaled(d, hz, m), want);
    }
}

#[test]
fn seq_107_clock_range() {
    assert!(multiplier(15_625_000_000).is_err());
    assert!(multiplier(15_625_000_001).is_ok());
    assert!(multiplier(15_624_999_999).is_err());
    for (hz, want) in [(15_625_000_000u64, false), (15_625_000_001, true), (15_624_999_999, false)] {
        reset_script();
        let mut h = b0().boot();
        h.io().hz = hz;
        let o = h.step();
        if want {
            assert_eq!(o, StepOutcome::Busy, "{hz}");
        } else {
            assert_eq!(o, StepOutcome::Fatal(FatalReason::ClockRange), "{hz}");
        }
    }
    // .4/.5 Δ = 2^58 − 1 / 2^58 after a 1 GHz base; .6 tsc before base.
    for (dt, ok) in [(((1u64 << 58) - 1) as i64, true), (1i64 << 58, false), (-1, false)] {
        reset_script();
        let mut h = b0().boot();
        h.step();
        let base = h.s.engine().clock.row.base_tsc;
        h.at((base as i64 + dt) as u64);
        h.push(RingId::Bbo(0), &bbo(A1, A, 0, "50000", "50002", 77, 1));
        let o = h.step();
        if ok {
            assert_eq!(o, StepOutcome::Busy);
        } else {
            assert_eq!(o, StepOutcome::Fatal(FatalReason::ClockRange), "dt {dt}");
        }
    }
}

#[test]
fn seq_108_stop_engine_ns() {
    reset_script();
    let h = b0().boot();
    assert_eq!(h.s.view().stop_engine_ns(), 0);
    // .2 replay_tail = true: the last applied record.
    reset_script();
    let mut h = b0().boot();
    h.steady();
    h.adv(123_456_789);
    h.quote(A1, "50000", "50004");
    h.run();
    let last = h.s.engine().last_ens;
    let h2 = H::crash_restart(h, plan(SnapshotSel::Latest, true, 1, 0), Rerun::default()).unwrap();
    assert_eq!(h2.s.view().stop_engine_ns(), last);
    // .3 replay_tail = false: the snapshot's engine_ns.
    reset_script();
    let mut h = b0().boot();
    h.steady();
    h.adv(100_000);
    h.cmd(AC1, &hft_event::ControlCmd::SnapshotNow);
    h.run();
    h.run();
    let sr = h.snapshots().last().cloned().unwrap();
    let ens = snap(&sr).0.engine_ns;
    h.adv(5_000_000);
    h.quote(A1, "50000", "50004");
    h.run();
    let h2 = H::crash_restart(h, plan(SnapshotSel::Latest, false, 1, 1), Rerun::default()).unwrap();
    assert_eq!(h2.s.view().stop_engine_ns(), ens);
}

#[test]
fn seq_109_snapshot_engine_ns() {
    reset_script();
    let mut h = b0().boot();
    h.steady();
    let base = h.s.engine().clock.row.base_tsc;
    h.cmd(AC1, &hft_event::ControlCmd::SnapshotNow);
    h.run();
    h.at(base + 5_000_000);
    h.step();
    h.step();
    let sr = h.snapshots().last().cloned().unwrap();
    let t = h.recs().into_iter().filter(|r| r.kind == EventKind::TIMER && r.seq < sr.seq).last().unwrap();
    let (hd, _) = snap(&sr);
    assert_eq!(hd.at_seq, t.seq);
    assert_eq!(hd.engine_ns, (t.tsc - base) as i64);
}

#[test]
fn seq_110_engine_ns_continuous() {
    for (tsc, hz, adv) in [(0u64, HZ, 1_000_000_000u64), (5_000, HZ, 1_000_000_000), (0, 3 * HZ, 3_000_000_000)] {
        reset_script();
        let mut h = b0().boot();
        h.steady();
        h.adv(1_000_000);
        h.quote(A1, "50000", "50004");
        h.run();
        let x = h.s.engine().last_ens;
        let now = h.now();
        let rr = Rerun { tsc: if tsc == 0 { now + 600_000_000_000 } else { tsc }, hz, ..Rerun::default() };
        let mut h2 = H::crash_restart(h, plan(SnapshotSel::Latest, true, 1, 0), rr.clone()).unwrap();
        h2.io().hz = hz;
        h2.step();
        let c: Cap<i64> = cap();
        let c2 = c.clone();
        set_hook(move |hx, ev, _| {
            if let EventRef::Bbo { .. } = ev
                && hx.s.strategy == S1
            {
                c2.borrow_mut().push(hx.v.now().engine_ns);
            }
        });
        let base = h2.s.engine().clock.row.base_tsc;
        h2.at(base + adv);
        let t = h2.now();
        for v in [A, B] {
            h2.push(RingId::Rep(0), &conn(v, 0, hft_event::ConnStateKind::Up, hft_event::ConnRole::Md, hft_types::InstId(0), hft_types::AccountId(0), t));
        }
        h2.push(RingId::Rep(0), &conn(A, 0, hft_event::ConnStateKind::Up, hft_event::ConnRole::Md, A1, hft_types::AccountId(0), t));
        h2.quote(A1, "50000", "50006");
        h2.run();
        clear_hook();
        let got = c.borrow().last().copied().expect("delivered");
        assert_eq!(got, x + 1_000_000_000, "tsc {tsc} hz {hz}");
    }
}

#[test]
fn seq_111_start_from_snapshot_uses_section25() {
    reset_script();
    let mut h = b0().boot();
    h.steady();
    let kf = h.keyframe();
    let (_, secs) = snap(&kf);
    assert_eq!(secs.engine_clock, h.s.engine().clock.row);
    for k in 0..20u64 {
        h.adv(1_000_000);
        h.quote(A1, "50000", if k % 2 == 0 { "50003" } else { "50004" });
        h.run();
    }
    let m = h.verify_from(Some(kf.seq));
    assert!(m.is_empty(), "{m:?}");
}

#[test]
fn seq_112_roll_keeps_base() {
    reset_script();
    let mut h = b0().boot();
    h.steady();
    let e0 = h.s.engine().clock.row;
    h.sys(2);
    let sr = h.snapshots().last().cloned().unwrap();
    let (hd, secs) = snap(&sr);
    assert_eq!(hd.purpose, 1);
    assert_eq!(secs.engine_clock, e0);
    let seg = h.j().segments.last().unwrap();
    assert!(!seg.records.iter().any(|r| r.kind == EventKind::ENGINE_CLOCK));
    let c: Cap<(i64, u64)> = cap();
    let c2 = c.clone();
    set_hook(move |hx, ev, _| {
        if let EventRef::Bbo { .. } = ev
            && hx.s.strategy == S1
        {
            let n = hx.v.now();
            c2.borrow_mut().push((n.engine_ns, n.tsc.0));
        }
    });
    h.adv(1_000);
    h.quote(A1, "50000", "50004");
    h.run();
    clear_hook();
    let (ens, tsc) = *c.borrow().last().unwrap();
    assert_eq!(ens, (tsc - e0.base_tsc) as i64);
    let _ = Role::Input;
}
