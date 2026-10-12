//! Output intervals at snapshot boundaries (TESTS.md SEQ-314).

use hft_event::{ControlCmd, EventKind, RecordFlags, Role};
use hft_sequencer::driver::live::RingId;
use hft_sequencer::driver::replay::{Log, ReplayDriver, ReplayMode};
use hft_sequencer::{BootSource, MemJournal, MemRecord, RecoveryFlags, Sequencer, SnapshotSel, StepOutcome};
use hft_types::{AccountId, InstId, Tsc};
use xxhash_rust::xxh3::Xxh3;
use zerocopy::{FromZeros, IntoBytes};

use crate::common::*;

fn calibration_run(calibration: i64, measure: u64) -> (H, MemRecord, u64) {
    reset_script();
    add_hook(|hx, ev, ctx| {
        if matches!(ev, hft_strategy_sdk::EventRef::Bbo { inst, .. } if *inst == A1)
            && hx.s.strategy == S1
            && hx.v.bbo(A1).ask_px == px("50007")
        {
            let _ = ctx.place(open_buy(hx, A1));
        }
    });
    let mut h = b0().boot_self_contained();
    h.steady();
    let before = h.keyframe().seq;
    let mut c = hft_event::ClockStatus::new_zeroed();
    c.tsc = Tsc(123_456);
    c.realtime_ns = calibration;
    c.phc_ns = calibration + 321;
    c.tsc_hz_milli = 999_999_999_000;
    c.state = 2; // Unsynced Meta still cannot affect clock_ok or the decision clock.
    h.push(RingId::Hk, &c);
    h.run();
    let input_seq = h.s.engine().cur_seq;
    h.io().measure = Some(measure);
    h.quote(A1, "50000", "50007");
    h.run();
    c.tsc = Tsc(c.tsc.0 + 1);
    h.push(RingId::Hk, &c);
    h.run();
    let clock_seq = h.j().records().last().unwrap().seq;
    h.cmd(AC1, &ControlCmd::SnapshotNow);
    h.run();
    h.run();
    let snapshot = h.snapshots().pop().unwrap();
    let cal = snap(&snapshot).1.clock_cal;
    assert_eq!(cal.from_seq, clock_seq);
    assert!(cal.from_seq > input_seq);
    assert_eq!(cal.tsc, c.tsc);
    assert_eq!(cal.realtime_ns, c.realtime_ns);
    assert_eq!(cal.phc_ns, c.phc_ns);
    assert_eq!(cal.tsc_hz_milli, c.tsc_hz_milli);
    assert_eq!(h.mode(S1, AC1, G1), hft_risk::mode::Effective::Live);
    assert_intervals(h.j());
    (h, snapshot, before)
}

fn check_calibration_replay(h: &H, from: Option<u64>) {
    assert!(h.verify_from(from).is_empty());
    #[cfg(feature = "replay-io")]
    {
        use hft_sequencer::driver::verify::{VerifyOptions, verify};
        let result = verify::<TestSet>(
            Log::from_segments(&h.j().segments),
            VerifyOptions {
                from_seq: from,
                ..VerifyOptions::default()
            },
            hb(),
        )
        .unwrap();
        assert!(result.inputs > 0);
        assert!(result.sink.mismatches.is_empty(), "{:?}", result.sink.mismatches);
    }
}

#[test]
fn seq_249_hash_exclusions() {
    let (base, a, before) = calibration_run(17, T0 + 111);
    let (other_cal, b, _) = calibration_run(8_765_432, T0 + 111);
    // .1 Same Inputs, different real hk records and persisted calibration payloads.
    let inputs = |h: &H| {
        h.recs()
            .into_iter()
            .filter(|r| r.role() == Role::Input)
            .collect::<Vec<_>>()
    };
    assert_eq!(inputs(&base), inputs(&other_cal));
    assert_ne!(snap(&a).1.clock_cal, snap(&b).1.clock_cal);
    assert_eq!(snap(&a).0.state_hash, snap(&b).0.state_hash);
    assert_eq!(snap(&a).0.out_hash, snap(&b).0.out_hash);
    for h in [&base, &other_cal] {
        for from in [None, Some(before)] {
            check_calibration_replay(h, from);
        }
    }
    // Observe the actual replay state after both prefix and runtime Meta. A Meta-only,
    // uncommitted tail is excluded by the existing EVENT_END recovery boundary.
    let mut log = Log::from_segments(&base.j().segments);
    let mut last = log
        .recs
        .iter()
        .rev()
        .find(|r| r.kind == EventKind::CLOCK_STATUS)
        .unwrap()
        .clone();
    last.seq = log.recs.last().unwrap().seq + 1;
    log.recs.push(last);
    let drv = ReplayDriver::new(ReplayMode::Verify, log, 0, 1);
    let mut replay = Sequencer::<ReplayDriver, TestSet>::boot(
        BootSource::Journal(plan(SnapshotSel::AtSeq(before), true, 0, 0)),
        &base.setup,
        drv,
        hb(),
    )
    .unwrap();
    for _ in 0..1_000 {
        if replay.driver().finished() {
            break;
        }
        assert!(!matches!(replay.step(), StepOutcome::Fatal(_)));
    }
    assert!(replay.driver().finished());
    let mut w = hft_strategy_sdk::SnapshotWriter::new(1 << 20);
    replay.write_sections(&mut w);
    assert_eq!(
        hft_sequencer::snapshot::read_seq_sections(w.as_bytes())
            .unwrap()
            .clock_cal
            .from_seq,
        snap(&a).1.clock_cal.from_seq
    );
    assert!(replay.driver().mismatches().is_empty());

    // .2 Only the driver's measurement clock changes the actual decision stamp.
    let (measured, c, _) = calibration_run(17, T0 + 999);
    assert_eq!(inputs(&base), inputs(&measured));
    let da = decisions(&base.recs());
    let dc = decisions(&measured.recs());
    assert_eq!(da.len(), 1);
    assert_eq!(dc.len(), 1);
    assert_eq!(da[0].1.reject, 0);
    assert_eq!(dc[0].1.reject, 0);
    assert_eq!(da[0].1.decided_tsc, Tsc(T0 + 111));
    assert_eq!(dc[0].1.decided_tsc, Tsc(T0 + 999));
    assert_ne!(snap(&a).0.snap_tsc, snap(&c).0.snap_tsc);
    assert_eq!(snap(&a).0.state_hash, snap(&c).0.state_hash);
    assert_eq!(snap(&a).0.out_hash, snap(&c).0.out_hash);
    check_calibration_replay(&measured, None);

    // .3 Host header stamps have no decision authority. boot_id is supplied as an offline
    // alternate-host stamp because the in-memory driver does not assign process UUIDs.
    let mut host = base.j().clone();
    for r in host.segments.iter_mut().flat_map(|s| &mut s.records) {
        if r.kind == EventKind::STATE_SNAPSHOT {
            let mut hdr = r.read::<hft_event::SnapshotHdr>().unwrap();
            let original = hdr;
            hdr.snap_tsc = 555;
            hdr.boot_id = [0xA5; 16];
            hdr.snap_utc_ns = 8_000_000;
            r.body[..96].copy_from_slice(hdr.as_bytes());
            assert_eq!(hdr.state_hash, original.state_hash);
            assert_eq!(hdr.out_hash, original.out_hash);
        }
    }
    assert!(verify_journal::<TestSet>(&host, &base.setup, None).is_empty());
    assert!(verify_journal::<TestSet>(&host, &base.setup, Some(before)).is_empty());

    // .4 A changed business field affects the independently calculated output interval,
    // even if the corrupt log also carries its own matching out_hash declaration.
    let mut qty = base.j().clone();
    let mut hash = Xxh3::new();
    let mut changed = 0;
    for r in qty.segments.iter_mut().flat_map(|s| &mut s.records) {
        if r.kind == EventKind::ORDER_DECISION && changed == 0 {
            let mut d = r.read::<hft_event::OrderDecision>().unwrap();
            d.qty.0 += q("0.001").0;
            r.body.copy_from_slice(d.as_bytes());
            changed = r.seq;
        }
        if r.kind == EventKind::STATE_SNAPSHOT {
            r.body[40..56].copy_from_slice(&hash.digest128().to_le_bytes());
            hash.reset();
        } else if r.role() == Role::Output {
            accumulate(&mut hash, r);
        }
    }
    assert_ne!(changed, 0);
    let corrupt = qty.records().find(|r| r.seq == a.seq).unwrap();
    assert_ne!(snap(corrupt).0.out_hash, snap(&a).0.out_hash);
    let failures = verify_journal::<TestSet>(&qty, &base.setup, Some(before));
    assert_eq!(failures.first().unwrap().seq, changed);
    assert!(failures.iter().any(|m| m.seq == a.seq));

    // .5/.6 The exemption covers only the calibration payload, never its structure.
    let mut bad = base.j().clone();
    let mut cal_only = bad.clone();
    let r = bad
        .segments
        .iter_mut()
        .flat_map(|s| &mut s.records)
        .find(|r| r.seq == a.seq)
        .unwrap();
    let mut off = 96;
    loop {
        let section = u16::from_le_bytes(r.body[off..off + 2].try_into().unwrap());
        let len = u32::from_le_bytes(r.body[off + 8..off + 12].try_into().unwrap()) as usize;
        if section == 16 {
            r.body[off + 4..off + 8].copy_from_slice(&0u32.to_le_bytes());
            let payload = cal_only
                .segments
                .iter_mut()
                .flat_map(|s| &mut s.records)
                .find(|r| r.seq == a.seq)
                .unwrap();
            payload.body[off + 16..off + 56].fill(0x5A);
            break;
        }
        off += 16 + len;
    }
    assert!(
        verify_journal::<TestSet>(&bad, &base.setup, Some(before))
            .iter()
            .any(|m| m.seq == a.seq)
    );
    assert!(verify_journal::<TestSet>(&cal_only, &base.setup, Some(before)).is_empty());

    // A real roll uses the source ClockStatus in its boot snapshot, including from_seq.
    let mut rolled = measured;
    rolled.sys(2);
    let s = rolled.snapshots().pop().unwrap();
    let clock = rolled
        .j()
        .records()
        .take_while(|r| r.seq < s.seq)
        .filter(|r| r.kind == EventKind::CLOCK_STATUS)
        .last()
        .unwrap();
    let c = clock.read::<hft_event::ClockStatus>().unwrap();
    assert_eq!(snap(&s).1.clock_cal.from_seq, clock.seq);
    assert_eq!(snap(&s).1.clock_cal.realtime_ns, c.realtime_ns);
    assert_eq!(snap(&s).1.clock_cal.tsc_hz_milli, c.tsc_hz_milli);
    check_calibration_replay(&rolled, None);
    let recovered = H::<TestSet>::restart_over(
        rolled.j().clone(),
        rolled.setup.clone(),
        plan(SnapshotSel::AtSeq(before), true, 1, 0),
        Rerun::default(),
    )
    .unwrap();
    assert!(recovered.s.engine().recovery_mismatches().is_empty());
}

// Independent of OutHash and its reset/stamping helpers. Only the event schema supplies the
// measurement fields and transport flag bits excluded from the byte stream.
pub(crate) fn accumulate(hash: &mut Xxh3, r: &MemRecord) {
    let mut body = r.body.clone();
    for (kind, off, len) in hft_event::MEASURE_MASK {
        if *kind == r.kind {
            body[usize::from(*off)..usize::from(*off + *len)].fill(0);
        }
    }
    let transport = RecordFlags::EVENT_END.0 | RecordFlags::CRC.0 | RecordFlags::PART.0;
    hash.update(&r.kind.0.to_le_bytes());
    hash.update(&(r.flags.0 & !transport).to_le_bytes());
    hash.update(&body);
}

fn assert_intervals(j: &MemJournal) {
    let mut hash = Xxh3::new();
    for r in j.records() {
        if r.kind == EventKind::STATE_SNAPSHOT {
            assert_eq!(
                snap(r).0.out_hash,
                hash.digest128().to_le_bytes(),
                "seq={}",
                r.seq
            );
            hash.reset();
        } else if r.role() == Role::Output {
            accumulate(&mut hash, r);
        }
    }
}

#[test]
fn seq_314_out_hash_and_verify() {
    reset_script();
    let mut b = b0();
    let book_inst = InstId(15);
    b.rd.insts
        .iter_mut()
        .find(|i| i.id == book_inst)
        .unwrap()
        .l2_depth = 5;
    let mut h = b.boot();
    h.steady();
    let first = h.keyframe().seq;
    // Each expired command produces one Output and does not schedule a snapshot.
    for _ in 0..1_000 {
        let expired = Tsc(h.now() - 1);
        let id = h.cmd_with("cli:test", ADMIN, AC1, &ControlCmd::SnapshotNow, |c| {
            c.expires_tsc = expired;
        });
        assert_eq!(h.run(), StepOutcome::Idle);
        assert_eq!(h.res(id).0, hft_sequencer::codes::status::EXPIRED);
    }
    let second = h.keyframe().seq;
    assert_eq!(
        h.j()
            .records()
            .filter(|r| r.seq > first && r.seq < second && r.role() == Role::Output)
            .count(),
        1_000
    );
    h.cmd(AC1, &ControlCmd::SnapshotNow);
    h.run();
    h.run();
    let on_demand = h.snapshots().last().unwrap().seq;
    assert_eq!(snap(h.snapshots().last().unwrap()).0.purpose, 3);
    assert_intervals(h.j());
    for from in [None, Some(first), Some(second), Some(on_demand)] {
        assert!(h.verify_from(from).is_empty(), "from={from:?}");
    }

    // Corrupt an Output and recompute the following logged digest from that corrupt stream.
    // Re-execution must still disagree both at the first bad record and at the interval end.
    let mut bad = h.j().clone();
    let mut changed = 0;
    let mut hash = Xxh3::new();
    for r in bad.segments.iter_mut().flat_map(|s| &mut s.records) {
        if r.seq <= first || r.seq > second {
            continue;
        }
        if r.seq == second {
            r.body[40..56].copy_from_slice(&hash.digest128().to_le_bytes());
        } else if r.role() == Role::Output {
            if changed == 0 {
                assert_eq!(r.kind, EventKind::CMD_RESULT);
                r.body[0] ^= 1;
                changed = r.seq;
            }
            accumulate(&mut hash, r);
        }
    }
    assert_ne!(changed, 0);
    let mismatches = verify_journal::<TestSet>(&bad, &h.setup, Some(first));
    assert_eq!(mismatches.first().unwrap().seq, changed);
    assert!(mismatches.iter().any(|m| m.seq == second));
    let recovered = H::<TestSet>::restart_over(
        bad.clone(),
        h.setup.clone(),
        plan(SnapshotSel::AtSeq(first), true, 1, 0),
        Rerun::default(),
    )
    .unwrap();
    assert!(
        recovered
            .s
            .engine()
            .recovery_mismatches()
            .iter()
            .any(|m| m.seq == second)
    );
    assert!(
        recovered
            .s
            .engine()
            .recovery_flags()
            .iter()
            .any(|(_, f)| f & RecoveryFlags::RECOVER_MISMATCH != 0)
    );
    let drv = ReplayDriver::new(ReplayMode::Mirror, Log::from_segments(&bad.segments), 0, 7);
    let mut mirror: Sequencer<ReplayDriver, TestSet> = Sequencer::boot(
        BootSource::Journal(plan(SnapshotSel::AtSeq(first), true, 0, 0)),
        &h.setup,
        drv,
        hb(),
    )
    .unwrap();
    assert_eq!(
        (0..100_000)
            .map(|_| mirror.step())
            .find(|s| *s != StepOutcome::Busy),
        Some(StepOutcome::Idle)
    );
    assert!(mirror.driver().mismatches().is_empty());
    assert!(mirror.driver().finished());

    // Segment config Outputs precede the boot snapshot. Drain's command result and mode
    // Outputs precede its snapshot within the very same event.
    let mut l2 = hft_event::L2SnapHdr::new_zeroed();
    l2.h.venue = A;
    l2.h.inst = book_inst;
    l2.h.rx_tsc = h.now();
    l2.snap_id = 1;
    l2.origin = hft_event::L2SnapHdr::TOP_N;
    l2.n_bids = 1;
    l2.n_asks = 1;
    let mut body = l2.as_bytes().to_vec();
    for price in ["10", "10.01"] {
        body.extend_from_slice(
            hft_event::Level {
                px: px(price),
                qty: q("1"),
            }
            .as_bytes(),
        );
    }
    h.push_bytes(RingId::Md(0), EventKind::L2_SNAPSHOT, body);
    h.run();
    h.sys(2);
    let rolled = h.snapshots().last().unwrap().seq;
    assert!(
        h.j()
            .records()
            .any(|r| r.seq > rolled && r.kind == EventKind::L2_SNAPSHOT && r.role() == Role::Output)
    );
    h.keyframe();
    h.cmd_with(
        "cli:ops",
        ADMIN,
        AccountId(0),
        &ControlCmd::Drain {
            cancel_resting: false,
        },
        |c| c.confirmed = 1,
    );
    assert!(matches!(h.run(), StepOutcome::Done { .. }));
    assert_intervals(h.j());
    assert!(h.verify_from(Some(first)).is_empty());
    assert!(h.verify_from(Some(rolled)).is_empty());
}
