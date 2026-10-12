//! The timer heap (TESTS.md "定时器": SEQ-234–SEQ-241, SEQ-349–SEQ-351).

use std::cell::RefCell;
use std::collections::BTreeSet;
use std::rc::Rc;

use hft_event::{ControlCmd, EventKind, Role};
use hft_sequencer::timers::{TimerEntry, TimerHeap, shift_tsc};
use hft_sequencer::{BootError, FatalReason, StepOutcome};
use hft_strategy_sdk::{CtxFull, EngineClockAnchor, EventRef, Now, TimerId};
use hft_types::{GroupId, Tsc};
use proptest::prelude::*;

use crate::common::*;

fn steady() -> H {
    reset_script();
    let mut h = b0().boot();
    h.steady();
    h
}

fn verify_timer_windows(h: &H, starts: &[Option<u64>]) {
    #[cfg(feature = "replay-io")]
    {
        use hft_sequencer::driver::replay::Log;
        use hft_sequencer::driver::verify::{VerifyOptions, verify};

        let log = Log::from_segments(&h.j().segments);
        for &from_seq in starts {
            let got = verify::<TestSet>(
                log.clone(),
                VerifyOptions {
                    from_seq,
                    ..Default::default()
                },
                hb(),
            )
            .unwrap();
            assert!(got.inputs > 0 && got.sink.compared > 0);
            assert!(got.sink.mismatches.is_empty(), "{:?}", got.sink.detail);
            assert!(
                got.sink.recovery_diffs.is_empty(),
                "{:?}",
                got.sink.recovery_diffs
            );
        }
    }
    #[cfg(not(feature = "replay-io"))]
    let _ = (h, starts);
}

#[test]
fn seq_234_timer_keyed_by_group() {
    // The third pair is disjoint: adding it must not fail InstOwned validation.
    let mut b = crate::sdk_contract::b_gt3();
    let g2b = GroupId(0x0200_0002);
    let g2c = GroupId(0x0200_03e9);
    let mut p = Spb::s2(1);
    p.groups = vec![gspec(G2), gspec_eth(g2b)];
    b.params = vec![Spb::s1(1), p, Spb::s9(1)];
    let mut h = b.boot();
    h.steady();
    let delivered = cap();
    let got = delivered.clone();
    add_hook(move |hx, ev, ctx| {
        if hx.s.strategy != S2 {
            return;
        }
        if let EventRef::Bbo { inst, b, .. } = ev
            && *inst == A1
            && b.ask_px == px("50041")
        {
            ctx.timer_at(Tsc(hx.v.now().tsc.0 + 100 * MS), 4, g2b).unwrap();
        }
        if let EventRef::Timer { tag, group, gix } = ev {
            got.borrow_mut().push((*tag, *group, *gix));
        }
    });
    h.quote(A1, "50000", "50041");
    h.run();
    let owner = hft_event::TIMER_OWNER_STRATEGY_BASE + u16::from(S2.0);
    let timer = *h.s.engine().heap.find_key(owner, 4, g2b.0).unwrap();
    assert_eq!(h.s.engine().heap.count_if(|t| t.owner == owner), 1);
    let window = h.keyframe().seq;
    let mut p = Spb::s2(2);
    p.groups = vec![crate::sdk_contract::gspec_fill(g2c), gspec(G2), gspec_eth(g2b)];
    let cmd = h.set_params(&p);
    h.run();
    assert_eq!(h.result_of(cmd).map(|r| r.status), Some(0));
    assert_eq!(h.s.engine().heap.find_key(owner, 4, g2b.0), Some(&timer));
    assert_eq!(h.s.engine().heap.count_if(|t| t.owner == owner), 1);
    h.at(timer.deadline - 1);
    h.run();
    assert!(delivered.borrow().is_empty());
    h.at(timer.deadline);
    h.run();
    assert_eq!(&*delivered.borrow(), &[(4, g2b, 2)]);
    assert!(h.s.engine().heap.find_key(owner, 4, g2b.0).is_none());
    assert!(h.s.instances().get(S2).unwrap().timers.is_empty());
    h.keyframe();
    verify_timer_windows(&h, &[None, Some(window)]);
}

#[test]
fn seq_235_removed_group_timers_cancelled() {
    reset_script();
    let mut b = b0();
    let g2b = GroupId(0x0200_0002);
    let mut p = Spb::s2(1);
    p.groups = vec![gspec(G2), gspec_eth(g2b)];
    b.params = vec![Spb::s1(1), p, Spb::s9(1)];
    let mut h = b.boot();
    h.steady();
    let far = h.now() + 1_000_000_000_000;
    add_hook(move |hx, ev, ctx| {
        if let EventRef::Bbo { inst, b, .. } = ev
            && *inst == A1
            && hx.s.strategy == S2
            && b.ask_px == px("50019")
        {
            let _ = ctx.timer_at(Tsc(far), 1, g2b);
            let _ = ctx.timer_at(Tsc(far), 2, g2b);
        }
    });
    h.quote(A1, "50000", "50019");
    h.run();
    assert_eq!(
        h.s.engine()
            .heap
            .sorted()
            .iter()
            .filter(|e| e.group == g2b.0)
            .count(),
        2
    );
    let mut p2 = Spb::s2(2);
    p2.groups = vec![gspec(G2)];
    let m = h.mark();
    let id = h.set_params(&p2);
    h.run();
    assert_eq!(h.result_of(id).map(|r| r.status), Some(0));
    assert_eq!(
        h.s.engine()
            .heap
            .sorted()
            .iter()
            .filter(|e| e.group == g2b.0)
            .count(),
        0
    );
    let n = h
        .notes_since(m)
        .into_iter()
        .find(|n| n.code == hft_sequencer::codes::seq_note(hft_sequencer::codes::note::TIMERS_CANCELLED))
        .expect("note");
    assert_eq!(n.v[0], 2);
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(ProptestConfig::default().cases.max(1_000)))]
    #[test]
    fn seq_236_heap_total_order(entries in proptest::collection::vec((0u64..4, 0u16..40, 0u32..10, 0u32..5), 0..256),
                                cancel in proptest::collection::vec(any::<bool>(), 256)) {
        let mut heap = TimerHeap::new(512);
        let mut set = BTreeSet::new();
        let mut keys = BTreeSet::new();
        for (d, o, id, g) in entries.iter().copied() {
            // Heap entries are keyed by (owner, timer_id, group) (one deadline per key).
            if !keys.insert((o, id, g)) {
                continue;
            }
            let e = TimerEntry { deadline: d * 1_000, owner: o, timer_id: id, group: g };
            set.insert((e.deadline, e.owner, e.timer_id, e.group));
            heap.push(e).unwrap();
        }
        for (k, key) in set.clone().iter().enumerate() {
            if cancel.get(k).copied().unwrap_or(false) {
                heap.remove_key(key.1, key.2, key.3);
                set.remove(key);
            }
        }
        let mut got = Vec::new();
        while let Some(e) = heap.pop() {
            got.push((e.deadline, e.owner, e.timer_id, e.group));
        }
        let want: Vec<_> = set.into_iter().collect();
        prop_assert_eq!(got, want);
    }
}

#[test]
fn seq_237_heap_shift_formula() {
    let stop = Tsc(10_000_000_000);
    let now = Tsc(500_000);
    assert_eq!(shift_tsc(10_300_000_000, now, stop, HZ, HZ), 300_500_000);
    assert_eq!(shift_tsc(9_000_000_000, now, stop, HZ, HZ), 1);
    assert_eq!(shift_tsc(10_000_000_000, now, stop, HZ, HZ), 500_000);
    assert_eq!(shift_tsc(10_300_000_000, now, stop, HZ, 3 * HZ), 900_500_000);
    assert_eq!(shift_tsc(0, now, stop, HZ, HZ), 0);
}

#[test]
fn seq_238_recovery_timers_merge() {
    use hft_exec::{AutoFixPhase, ExecTimerKind};
    use hft_sequencer::SnapshotSel;
    use hft_strategy_sdk::{BrokenReason, TargetState};

    // The prepared keyframe makes the two independent timer sources disagree.
    // No production registration or merge helper is used to calculate expectations.
    for (kind, remaining, expected) in [
        (ExecTimerKind::Wake, 3_000, 4_000),
        (ExecTimerKind::Wake, 5_000, 5_000),
        (ExecTimerKind::AutoFix, 3_000, 4_000),
    ] {
        let mut h = steady();
        let stop = h.now();
        let wake = 1 | (u32::from(AC2.0) << 8);
        let automatic = 5 | (u32::from(AC2.0) << 8);
        {
            let e = h.s.engine_mut();
            let cell = e.exec.cell_mut(AC2, G2).unwrap();
            if kind == ExecTimerKind::Wake {
                cell.wake_at = Tsc(stop + remaining);
            } else {
                cell.view.state = TargetState::Broken(BrokenReason::LinkBroken);
                cell.broken_since = Tsc(stop);
                cell.auto_fix.phase = AutoFixPhase::Primed as u8;
                cell.auto_fix.due_tsc = Tsc(stop + remaining);
            }
            assert_eq!(e.heap.remove_key(5, wake, G2.0), 0);
            e.heap
                .push(TimerEntry {
                    deadline: stop + 4_000,
                    owner: 5,
                    timer_id: wake,
                    group: G2.0,
                })
                .unwrap();
        }
        let window = h.keyframe().seq;
        let request_id = if kind == ExecTimerKind::Wake {
            wake
        } else {
            automatic
        };
        assert!(
            h.s.engine()
                .exec
                .recovery_timers()
                .any(|t| { t.group == G2 && t.timer_id == request_id && t.at == Tsc(stop + remaining) })
        );
        let mut h = h
            .crash_restart(
                plan(SnapshotSel::Latest, true, 2, 0),
                Rerun {
                    tsc: 1_000,
                    ..Default::default()
                },
            )
            .unwrap();
        let entries: Vec<_> =
            h.s.engine()
                .heap
                .sorted()
                .into_iter()
                .filter(|t| t.owner == 5 && t.group == G2.0)
                .map(|t| (t.timer_id, t.deadline))
                .collect();
        if kind == ExecTimerKind::Wake {
            assert_eq!(entries, [(wake, expected)]);
        } else {
            assert_eq!(entries, [(automatic, expected), (wake, 5_000)]);
        }
        h.run();
        h.keyframe();
        verify_timer_windows(&h, &[Some(window)]);
    }
}

#[test]
fn seq_239_periodic_timers_reregistered() {
    use hft_sequencer::SnapshotSel;

    for remaining_ms in [1_500_u64, 400] {
        for scale in [1, 3] {
            reset_script();
            let mut b = b0();
            b.rd.caps[1].dms = hft_refdata::DeadManCap::CountdownAccount;
            b.rd.caps[1].dms_refresh_ms = 10_000;
            b.rd.caps[1].dms_timeout_ms = 30_000;
            let mut h = b.boot();
            h.steady();
            let stop = h.now();
            let dead_man = 4 | (u32::from(B.0) << 8) | (u32::from(AC1.0) << 16);
            // The specification starts from a prepared snapshot. Only the heap fixture is
            // edited; independent verification starts at that actual saved keyframe.
            for (owner, timer_id, ms) in [(3, 2, remaining_ms), (1, dead_man, 2_000)] {
                let heap = &mut h.s.engine_mut().heap;
                assert_eq!(heap.remove_key(owner, timer_id, 0), 1);
                heap.push(TimerEntry {
                    deadline: stop + ms * MS,
                    owner,
                    timer_id,
                    group: 0,
                })
                .unwrap();
            }
            let window = h.keyframe().seq;
            let start = 500_000;
            let mut h = h
                .crash_restart(
                    plan(SnapshotSel::Latest, true, 2, 0),
                    Rerun {
                        tsc: start,
                        hz: scale * HZ,
                        ..Default::default()
                    },
                )
                .unwrap();
            assert_eq!(h.run(), StepOutcome::Idle);
            let ms = scale * MS;
            for (owner, id, delay) in [(3, 2, remaining_ms.min(1_000)), (1, dead_man, 10_000)] {
                let heap = &h.s.engine().heap;
                assert_eq!(
                    heap.count_if(|t| t.owner == owner && t.timer_id == id && t.group == 0),
                    1
                );
                assert_eq!(heap.find_key(owner, id, 0).unwrap().deadline, start + delay * ms);
            }
            let mark = h.io_ref().out.len();
            let refreshes = |h: &H| {
                h.io_ref().out[mark..]
                    .iter()
                    .filter(|(ring, cmd)| {
                        *ring == hft_sequencer::OutRing::Venue(B)
                            && cmd.account == AC1
                            && cmd.op == hft_event::GwOp::DeadManRefresh
                    })
                    .count()
            };
            h.at(start + 9_999 * ms);
            h.run();
            assert_eq!(refreshes(&h), 0, "old DeadMan deadline must be replaced");
            h.at(start + 10_000 * ms);
            h.run();
            assert_eq!(refreshes(&h), 1);
            h.keyframe();
            verify_timer_windows(&h, &[Some(window)]);
        }
    }
}

#[test]
fn seq_240_engine_ns_deadline_ceil() {
    let a = EngineClockAnchor {
        base_engine_ns: 0,
        base_tsc: Tsc(0),
        tsc_hz_milli: 1_050_000_000_000,
        utc_tsc: Tsc(0),
        utc_ns: 0,
    };
    let n = Now::new(Tsc(0), 1, 1, true, a);
    assert_eq!(n.tsc_of(100), Tsc(105));
    assert_eq!(n.tsc_of(101), Tsc(107));
    assert_eq!(n.tsc_of(0), Tsc(0));
}

#[test]
fn seq_241_dead_man_refresh() {
    reset_script();
    let mut b = b0();
    b.rd.caps[1].dms = hft_refdata::DeadManCap::CountdownAccount;
    b.rd.caps[1].dms_refresh_ms = 10_000;
    b.rd.caps[1].dms_timeout_ms = 30_000;
    let mut h = b.boot();
    h.steady();
    let start = h.now();
    let n0 = h.io_ref().out.len();
    for k in 1..=350u64 {
        h.at(start + k * 100_000_000);
        h.step();
    }
    let refresh: Vec<_> = h.io_ref().out[n0..]
        .iter()
        .filter(|o| {
            o.0 == hft_sequencer::OutRing::Venue(B)
                && o.1.op == hft_event::GwOp::DeadManRefresh
                && o.1.account == AC1
        })
        .collect();
    assert_eq!(refresh.len(), 3);
    assert!(refresh.iter().all(|o| o.1.arg == 30_000));
    // No steps: no refresh.
    let n1 = h.io_ref().out.len();
    h.at(start + 65_000_000_000);
    assert_eq!(h.io_ref().out.len(), n1);
}

fn demand(h: &H) -> u32 {
    h.s.engine().engine_timer_demand
}

#[test]
fn seq_349_timer_capacity_at_boot() {
    reset_script();
    let h = b0().boot();
    let d = demand(&h);
    for (cap, ok) in [(d - 1, false), (d, false), (d + 1, true)] {
        reset_script();
        let mut b = b0();
        b.caps.max_timers = cap;
        match b.try_boot() {
            Ok(h) => {
                assert!(ok, "{cap}");
                assert_eq!(h.s.engine().caps.max_timers - demand(&h), 1);
            }
            Err(e) => {
                assert!(!ok);
                assert_eq!(e, BootError::TimerCapacity { demand: d });
            }
        }
    }
}

#[test]
fn seq_350_strategy_timer_quota() {
    use hft_sequencer::driver::live::RingId;
    use hft_strategy_sdk::{EvMask, Part};
    use zerocopy::IntoBytes;

    reset_script();
    let d = demand(&b0().boot());
    // One callback; two competing instances; two callbacks of one instance; queued Timer;
    // direct Advice. Every row uses the actual engine and the same two-slot shared quota.
    for row in 0..5 {
        reset_script();
        let mut b = b0();
        b.caps.max_timers = d + 2;
        if row == 2 {
            b.params[0].groups.push(gspec_eth(GroupId(0x0100_0002)));
            set_subs(
                S1,
                vec![(A1, 0, EvMask::BBO, Part::Fast), (A1, 1, EvMask::BBO, Part::Fast)],
            );
        }
        let mut h = b.boot();
        h.steady();
        let seen = Rc::new(RefCell::new(Vec::new()));
        let out = seen.clone();
        add_hook(move |hx, ev, ctx| {
            let sid = hx.s.strategy;
            if row == 3
                && sid == S1
                && let EventRef::Bbo { inst, b, .. } = ev
                && *inst == A1
                && b.ask_px == px("50010")
            {
                assert_eq!(
                    ctx.timer_at(Tsc(hx.v.now().tsc.0 + 1_000_000), 77, G1),
                    Ok(TimerId(1))
                );
            }
            let trigger = match ev {
                EventRef::Bbo { inst, b, .. } => row <= 2 && *inst == A1 && b.ask_px == px("50011"),
                EventRef::Timer { tag, .. } => row == 3 && *tag == 77,
                EventRef::Advice { local_kind, .. } => row == 4 && *local_kind == 7,
                _ => false,
            };
            if trigger && (sid == S1 || row == 1 && sid == S2) {
                let n = if row == 1 || row == 2 { 2 } else { 3 };
                let group = if sid == S2 { G2 } else { G1 };
                for tag in 1..=n {
                    let r = ctx.timer_at(
                        Tsc(hx.v.now().tsc.0 + 1_000_000_000_000 + u64::from(tag)),
                        tag,
                        group,
                    );
                    out.borrow_mut().push((sid, false, r));
                }
            }
            if sid == S1
                && let EventRef::Bbo { inst, b, gix } = ev
                && *inst == A1
                && *gix == 0
                && b.ask_px == px("50013")
            {
                out.borrow_mut().push((
                    sid,
                    true,
                    ctx.timer_at(Tsc(hx.v.now().tsc.0 + 1_000_000_000_000), 99, G1),
                ));
            }
        });
        let before = h.s.engine().ctx_full;
        match row {
            3 => {
                h.quote(A1, "50000", "50010");
                h.run();
                assert_eq!(h.s.engine().strat_timers, 1);
                h.at(h.now() + 1_000_000);
                h.run();
            }
            4 => {
                let mut hd = <hft_event::AdviceHdr as zerocopy::FromZeros>::new_zeroed();
                hd.strategy = S1;
                hd.account = AC1;
                hd.group = G1;
                hd.local_kind = 7;
                hd.basis_seq = 1_000_000;
                hd.basis_tsc = Tsc(h.now());
                h.push_bytes(RingId::Advice, EventKind::ADVICE, hd.as_bytes().to_vec());
                h.run();
            }
            _ => {
                h.quote(A1, "50000", "50011");
                h.run();
            }
        }
        let expected_calls = if row == 1 || row == 2 { 4 } else { 3 };
        assert_eq!(
            seen.borrow().len(),
            expected_calls,
            "row {row}: delivery not exercised"
        );
        assert!(seen.borrow()[..2].iter().all(|r| r.2.is_ok()), "row {row}");
        assert!(
            seen.borrow()[2..].iter().all(|r| r.2 == Err(CtxFull)),
            "row {row}"
        );
        assert_eq!(
            h.s.engine().ctx_full - before,
            (expected_calls - 2) as u64,
            "row {row}"
        );
        assert_eq!(h.s.engine().strat_timers, 2, "row {row}");
        let owner = hft_event::TIMER_OWNER_STRATEGY_BASE + u16::from(S1.0);
        assert_eq!(h.s.engine().heap.count_if(|t| t.owner == owner), 2);
        assert_eq!(h.s.instances().get(S1).unwrap().timers.len(), 2);
        // A restored full heap must reject at the SDK call too, not only in apply_ctx.
        let window = h.keyframe();
        h.quote(A1, "50000", "50013");
        h.run();
        assert_eq!(seen.borrow().last().unwrap().2, Err(CtxFull));
        assert_eq!(h.s.engine().ctx_full - before, (expected_calls - 1) as u64);
        h.keyframe();
        assert!(h.verify().is_empty(), "row {row}: full replay");
        assert!(
            h.verify_from(Some(window.seq)).is_empty(),
            "row {row}: snapshot window"
        );
        if row == 0 {
            // Strategy reservations leave every remaining physical slot for engine owners.
            let heap = &mut h.s.engine_mut().heap;
            let room = heap.capacity() - heap.len();
            for k in 0..room {
                assert!(
                    heap.push(TimerEntry {
                        deadline: u64::MAX,
                        owner: 3,
                        timer_id: 0x8000_0000 + k as u32,
                        group: 0
                    })
                    .is_ok()
                );
            }
            assert_eq!(heap.len(), heap.capacity());
        }
    }
}

#[test]
fn seq_407_cancel_one_timer_registration_and_reuse_quota() {
    reset_script();
    let d = demand(&b0().boot());
    let mut b = b0();
    b.caps.max_timers = d + 2;
    let mut h = b.boot();
    h.steady();
    let results = Rc::new(RefCell::new(Vec::new()));
    let out = results.clone();
    add_hook(move |hx, ev, ctx| {
        if hx.s.strategy != S1 {
            return;
        }
        if let EventRef::Bbo { inst, b, .. } = ev
            && *inst == A1
        {
            let now = hx.v.now().tsc.0;
            if b.ask_px == px("50021") {
                out.borrow_mut().push(ctx.timer_at(Tsc(now + 10_000_000), 9, G1));
                out.borrow_mut().push(ctx.timer_at(Tsc(now + 20_000_000), 9, G1));
            } else if b.ask_px == px("50022") {
                ctx.cancel_timer(TimerId(2));
                ctx.cancel_timer(TimerId(2));
                ctx.cancel_timer(TimerId(999));
                out.borrow_mut().push(ctx.timer_at(Tsc(now + 30_000_000), 10, G1));
                out.borrow_mut().push(ctx.timer_at(Tsc(now + 40_000_000), 11, G1));
            }
        }
    });
    let start = h.now();
    h.quote(A1, "50000", "50021");
    h.run();
    assert_eq!(&*results.borrow(), &[Ok(TimerId(1)), Ok(TimerId(2))]);
    let window = h.keyframe();
    let before = h.s.engine().ctx_full;
    h.quote(A1, "50000", "50022");
    h.run();
    assert_eq!(&results.borrow()[2..], &[Ok(TimerId(3)), Err(CtxFull)]);
    assert_eq!(h.s.engine().ctx_full, before + 1);
    let live = &h.s.instances().get(S1).unwrap().timers;
    assert_eq!(
        live.iter().map(|t| (t.id, t.tag, t.at.0)).collect::<Vec<_>>(),
        vec![
            (TimerId(1), 9, start + 10_000_000),
            (TimerId(3), 10, start + 30_000_000)
        ]
    );
    assert_eq!(h.s.engine().strat_timers, 2);
    clear_log();
    h.at(start + 10_000_000);
    h.run();
    assert_eq!(log().iter().filter(|g| g.s == S1 && g.kind == "Timer").count(), 1);
    assert_eq!(h.s.engine().strat_timers, 1);
    h.at(start + 20_000_000);
    h.run();
    assert_eq!(
        log().iter().filter(|g| g.s == S1 && g.kind == "Timer").count(),
        1,
        "cancelled deadline fired"
    );
    h.keyframe();
    assert!(h.verify().is_empty(), "full replay");
    assert!(h.verify_from(Some(window.seq)).is_empty(), "snapshot window");
}

#[test]
fn seq_351_timer_overflow_fatal() {
    let mut h = steady();
    let n = h.s.engine().heap.len();
    h.s.engine_mut().heap.set_capacity(n);
    // Any engine registration now overflows: an order whose Ack timeout registers a query.
    set_hook(|hx, ev, ctx| {
        if let EventRef::Bbo { inst, .. } = ev
            && *inst == A1
            && hx.s.strategy == S1
        {
            let _ = ctx.timer_at(Tsc(hx.v.now().tsc.0 + 1), 9, G1);
        }
    });
    let _ = (ControlCmd::SnapshotNow, EventKind::NOTE, Role::Input);
    let n_out = h.io_ref().out.len();
    h.trace_on();
    // Strategy timers stay within the quota check; the engine path: Sweep re-registration.
    let sweep =
        h.s.engine()
            .heap
            .sorted()
            .into_iter()
            .find(|e| e.owner == 3 && e.timer_id == 2)
            .unwrap();
    clear_hook();
    h.s.engine_mut().heap.set_capacity(n - 1);
    h.at(sweep.deadline);
    let o = h.step();
    assert_eq!(o, StepOutcome::Fatal(FatalReason::TimerOverflow));
    assert_eq!(h.io_ref().out.len(), n_out);
}
