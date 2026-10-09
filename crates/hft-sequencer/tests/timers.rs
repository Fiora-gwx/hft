//! The timer heap (TESTS.md "定时器": SEQ-234–SEQ-241, SEQ-349–SEQ-351).

use std::collections::BTreeSet;

use hft_event::{ControlCmd, EventKind, Role};
use hft_sequencer::timers::{TimerEntry, TimerHeap, shift_tsc};
use hft_sequencer::{BootError, FatalReason, StepOutcome};
use hft_strategy_sdk::{EngineClockAnchor, EventRef, Now};
use hft_types::{GroupId, Tsc};
use proptest::prelude::*;

use crate::common::*;

fn steady() -> H {
    reset_script();
    let mut h = b0().boot();
    h.steady();
    h
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
    #![proptest_config(ProptestConfig::with_cases(300))]
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
