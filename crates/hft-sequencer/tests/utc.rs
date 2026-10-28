//! Engine UTC (TESTS.md "引擎 UTC": SEQ-443–SEQ-446).

use hft_event::{EventKind, Role, UtcAnchor};
use hft_rt::{CalibConfig, ClockReads, ClockVerdict, DriftTracker, Tsc, TscClock, sample_pair_with};
use hft_sequencer::advisor::utc_of_record;
use hft_sequencer::clock::{EngineClock, EngineClockRow, UTC_SLEW_PPM, UtcLineRow};
use hft_sequencer::driver::live::RingId;
use hft_sequencer::driver::replay::Log;
use hft_sequencer::{SnapshotSel, StepOutcome};
use hft_strategy_sdk::EventRef;
use proptest::prelude::*;

use crate::common::*;

fn anchor(tsc: u64, realtime_ns: i64, hz_milli: u64) -> UtcAnchor {
    let mut a = <UtcAnchor as zerocopy::FromZeros>::new_zeroed();
    a.tsc = Tsc(tsc);
    a.realtime_ns = realtime_ns;
    a.hz_milli = hz_milli;
    a.err_ns = 40;
    a
}

/// 1 GHz line: one tick = 1 ns.
const G: u64 = 1_000_000_000_000;

#[test]
fn seq_443_anchor_line() {
    let t = Tsc(10_000_000);
    // .1 Engine behind by 2 ms: steps forward onto the target, no lead.
    let l = UtcLineRow::anchored(UTC0, t, &anchor(t.0, UTC0 + 2_000_000, G), G);
    assert_eq!(
        (l.tsc, l.utc_ns, l.hz_milli, l.lead_ns),
        (t.0, UTC0 + 2_000_000, G, 0),
        "SEQ-443.1"
    );
    assert_eq!(l.utc_at(t), UTC0 + 2_000_000, "SEQ-443.1");
    assert_eq!(l.utc_at(Tsc(t.0 + 1_000)), UTC0 + 2_001_000, "SEQ-443.1");
    // .2 Engine ahead by 1 ms: keeps its UTC at t and slews at 500 ppm until it meets the target.
    let target = UTC0 - 1_000_000;
    let l = UtcLineRow::anchored(UTC0, t, &anchor(t.0, target, G), G);
    assert_eq!((l.utc_ns, l.lead_ns), (target, 1_000_000), "SEQ-443.2");
    assert_eq!(l.utc_at(t), UTC0, "SEQ-443.2");
    let sec = 1_000_000_000u64;
    assert_eq!(
        l.utc_at(Tsc(t.0 + sec)),
        target + 1_000_000_000 + 500_000,
        "SEQ-443.2"
    );
    assert_eq!(l.utc_at(Tsc(t.0 + 2 * sec)), target + 2_000_000_000, "SEQ-443.2");
    assert_eq!(l.utc_at(Tsc(t.0 + 3 * sec)), target + 3_000_000_000, "SEQ-443.2");
    assert_eq!(UTC_SLEW_PPM, 500, "SEQ-443.2");
    // .3 The pair is older than the record: the target is extrapolated to the record's tsc.
    let l = UtcLineRow::anchored(UTC0, t, &anchor(t.0 - 50_000, UTC0 + 1_000_000, G), G);
    assert_eq!((l.utc_ns, l.lead_ns), (UTC0 + 1_050_000, 0), "SEQ-443.3");
    // .4 Measured rate (30 ppm slow counter) within tolerance: one real second = 999 970 000 ticks.
    let slow = G - 30_000_000;
    let l = UtcLineRow::anchored(UTC0, t, &anchor(t.0, UTC0, slow), G);
    assert_eq!(l.hz_milli, slow, "SEQ-443.4");
    assert_eq!(
        l.utc_at(Tsc(t.0 + 999_970_000)),
        UTC0 + 1_000_000_000,
        "SEQ-443.4"
    );
    // .5 Rate unknown (0) or beyond 1 000 ppm: EngineClock's frequency.
    for (row, hz) in [("SEQ-443.5", 0u64), ("SEQ-443.5", G + G / 1_000 + 1_000)] {
        let l = UtcLineRow::anchored(UTC0, t, &anchor(t.0, UTC0, hz), G);
        assert_eq!(l.hz_milli, G, "{row}");
    }
    let l = UtcLineRow::anchored(UTC0, t, &anchor(t.0, UTC0, G + G / 1_000), G);
    assert_eq!(l.hz_milli, G + G / 1_000, "SEQ-443.5");
    // .6 Before any anchor the record UTC is the segment pair with EngineClock's frequency.
    let mut c = EngineClock::new(EngineClockRow {
        base_engine_ns: 0,
        base_tsc: T0,
        tsc_hz_milli: G,
        clock_seq: 9,
    })
    .unwrap();
    assert!(!c.utc().is_set(), "SEQ-443.6");
    assert_eq!(
        c.anchor_at(Tsc(T0 + 5_000), Tsc(T0), UTC0)
            .utc_of(Tsc(T0 + 5_000)),
        UTC0 + 5_000,
        "SEQ-443.6"
    );
    c.set_utc(UtcLineRow::anchored(
        UTC0 + 5_000,
        Tsc(T0 + 5_000),
        &anchor(T0 + 5_000, UTC0 + 9_000, G),
        G,
    ))
    .unwrap();
    let a = c.anchor_at(Tsc(T0 + 6_000), Tsc(T0), UTC0);
    assert_eq!(
        (a.utc_tsc, a.utc_ns),
        (Tsc(T0 + 6_000), UTC0 + 10_000),
        "SEQ-443.6"
    );
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(512))]
    #[test]
    fn seq_444_line_monotone(
        steps in proptest::collection::vec((1u64..5_000_000_000, -5_000_000i64..5_000_000, 0u32..3), 1..40),
        hz in prop_oneof![Just(G), Just(1_050_000_000_000u64), 15_625_000_001u64..=8_000_000_000_000u64],
    ) {
        let mut c = EngineClock::new(EngineClockRow { base_engine_ns: 0, base_tsc: 0, tsc_hz_milli: hz, clock_seq: 1 })
            .unwrap();
        let mut t = 1_000u64;
        let mut last = c.anchor_at(Tsc(t), Tsc(0), UTC0).utc_of(Tsc(t));
        for (dt, off, kind) in steps {
            t += dt;
            let cur = c.anchor_at(Tsc(t), Tsc(0), UTC0).utc_of(Tsc(t));
            prop_assert!(cur >= last);
            if c.utc().is_set() {
                // The engine's multiply-shift path equals the floor formula.
                prop_assert_eq!(cur, c.utc().utc_at(Tsc(t)));
            }
            if kind != 0 {
                // An anchor whose pair puts realtime `off` ns away from the engine UTC at `t`.
                let rate = if kind == 1 { hz } else { hz - hz / 100_000 };
                c.set_utc(UtcLineRow::anchored(cur, Tsc(t), &anchor(t, cur + off, rate), hz)).unwrap();
                let after = c.anchor_at(Tsc(t), Tsc(0), UTC0).utc_of(Tsc(t));
                prop_assert_eq!(after, cur.max(cur + off));
            }
            last = c.anchor_at(Tsc(t), Tsc(0), UTC0).utc_of(Tsc(t));
        }
    }
}

/// SEQ-445 host: a counter `ppm_slow` slower than CLOCK_REALTIME; CLOCK_MONOTONIC_RAW scales the
/// counter by its nominal 1 GHz (as the kernel's raw clock does). Every reading costs 10 ticks.
struct SlowHost {
    ticks: u64,
    /// Real ticks per second × 1000 per segment: `(from_tick, hz_milli, realtime at from_tick)`.
    segs: Vec<(u64, u64, i64)>,
}

impl SlowHost {
    fn realtime(&self, t: u64) -> i64 {
        let (t0, hz, r0) = *self.segs.iter().rev().find(|s| s.0 <= t).unwrap();
        r0 + ((u128::from(t - t0) * 1_000_000_000_000) / u128::from(hz)) as i64
    }
    /// From now on the true counter rate is `hz_milli`.
    fn set_rate(&mut self, hz_milli: u64) {
        let r = self.realtime(self.ticks);
        self.segs.push((self.ticks, hz_milli, r));
    }
    fn hz(&self) -> u64 {
        self.segs.last().unwrap().1
    }
    fn advance_ns(&mut self, ns: u64) {
        self.ticks += (u128::from(ns) * u128::from(self.hz()) / 1_000_000_000_000) as u64;
    }
}

impl ClockReads for SlowHost {
    fn tsc(&mut self) -> Tsc {
        self.ticks += 10;
        Tsc(self.ticks)
    }
    fn mono_raw_ns(&mut self) -> i64 {
        self.ticks += 10;
        self.ticks as i64
    }
    fn realtime_ns(&mut self) -> i64 {
        self.ticks += 10;
        self.realtime(self.ticks)
    }
    fn sleep_ms(&mut self, ms: u32) {
        self.advance_ns(u64::from(ms) * 1_000_000);
    }
}

#[test]
fn seq_445_day_bounded() {
    const R0: i64 = 1_791_072_000_000_000_000;
    let slow_30ppm = G - 30_000_000;
    let mut host = SlowHost {
        ticks: 1_000_000,
        segs: vec![(0, slow_30ppm, R0)],
    };
    let cfg = CalibConfig {
        rounds: 5,
        samples: 8,
        phc_samples: 10,
        cpuid_tolerance_ppm: 100,
        step_threshold_ns: 50_000,
    };
    // Boot: frequency against MONOTONIC_RAW (= the nominal 1 GHz) and the boot pair.
    let boot = TscClock::calibrate_with(&mut host, Some(G), &cfg).unwrap();
    assert_eq!(
        boot.hz_milli, G,
        "SEQ-445 boot calibration takes the nominal rate"
    );
    let seg = boot.base;
    let mut clock = EngineClock::new(EngineClockRow {
        base_engine_ns: 0,
        base_tsc: seg.tsc.0,
        tsc_hz_milli: boot.hz_milli,
        clock_seq: 1,
    })
    .unwrap();
    let utc = |c: &EngineClock, t: u64| c.anchor_at(Tsc(t), seg.tsc, seg.realtime_ns).utc_of(Tsc(t));
    let mut tracker = DriftTracker::new(50_000);
    let (mut since, mut anchors, mut last) = (None::<u32>, 0u32, i64::MIN);
    // Worst |engine UTC − realtime| before the first anchor and after it.
    let (mut worst_boot, mut worst) = (0i64, 0i64);
    // 24 h of 1 s calibration ticks (anchor every 10th Synced tick), 10 records per second; at
    // 12 h the counter's true rate moves by 1 ppm (temperature).
    for sec in 0..86_400u32 {
        if sec == 43_200 {
            host.set_rate(slow_30ppm + 1_000_000);
        }
        let pair = sample_pair_with(&mut host, boot.hz_milli, None, 8);
        let verdict = tracker.push(pair);
        if verdict == ClockVerdict::Synced {
            if since.is_none_or(|n| n + 1 >= 10) {
                // The live driver hands the hk element over 50 µs later.
                host.advance_ns(50_000);
                let t = host.ticks;
                let cur = utc(&clock, t);
                let a = anchor(pair.tsc.0, pair.realtime_ns, tracker.hz_milli().unwrap_or(0));
                clock
                    .set_utc(UtcLineRow::anchored(cur, Tsc(t), &a, boot.hz_milli))
                    .unwrap();
                since = Some(0);
                anchors += 1;
            } else {
                since = since.map(|n| n + 1);
            }
        } else {
            since = None;
        }
        for _ in 0..10 {
            host.advance_ns(100_000_000);
            let t = host.ticks;
            let u = utc(&clock, t);
            assert!(u >= last, "SEQ-445 monotone at {sec} s");
            last = u;
            let err = (u - host.realtime(t)).abs();
            if anchors == 0 {
                worst_boot = worst_boot.max(err);
            } else {
                worst = worst.max(err);
            }
        }
    }
    assert!(anchors >= 8_600, "SEQ-445 anchors {anchors}");
    assert!(
        worst_boot <= 1_000_000,
        "SEQ-445 before the first anchor: {worst_boot} ns"
    );
    assert!(
        worst <= 50_000,
        "SEQ-445 worst |engine UTC − realtime| = {worst} ns"
    );
    // Without re-anchoring the boot calibration drifts ≈ 30 ppm: > 2.5 s behind after a day.
    let t = host.ticks;
    let unanchored = EngineClock::new(clock.row).unwrap();
    let lag = host.realtime(t) - utc(&unanchored, t);
    assert!(lag > 2_500_000_000, "SEQ-445 boot-only lag {lag} ns");
}

/// `(tsc, utc_ns)` of every Bbo `S1` sees.
fn capture() -> Cap<(u64, i64)> {
    let c: Cap<(u64, i64)> = cap();
    let c2 = c.clone();
    set_hook(move |hx, ev, _| {
        if let EventRef::Bbo { .. } = ev
            && hx.s.strategy == S1
        {
            let n = hx.v.now();
            c2.borrow_mut().push((n.tsc.0, n.utc_ns));
        }
    });
    c
}

#[test]
fn seq_446_anchor_input_replay() {
    reset_script();
    let mut h = b0().boot();
    h.steady();
    let seen = capture();
    let mut k = 0u32;
    let mut quote = |h: &mut H, d: u64| {
        h.adv(d);
        k += 1;
        h.quote(A1, "50000", if k.is_multiple_of(2) { "50003" } else { "50004" });
        h.run();
    };
    quote(&mut h, MS);
    let e0 = h.s.engine().clock;
    assert!(!e0.utc().is_set(), "SEQ-446.1");
    // .1 Behind by 2 ms with a 30 ppm slow measured rate.
    h.adv(MS);
    let t1 = h.now();
    let before = h.s.engine().utc_at(Tsc(t1));
    let m = h.mark();
    h.push(
        RingId::Hk,
        &anchor(t1 - 20_000, before + 2_000_000, HZ - 30_000_000),
    );
    h.run();
    let recs = h.kinds_since(m, EventKind::UTC_ANCHOR);
    assert_eq!(recs.len(), 1, "SEQ-446.1");
    assert_eq!(recs[0].role(), Role::Input, "SEQ-446.1");
    let l1 = *h.s.engine().clock.utc();
    assert_eq!(
        (l1.tsc, l1.hz_milli, l1.lead_ns),
        (recs[0].tsc, HZ - 30_000_000, 0),
        "SEQ-446.1"
    );
    quote(&mut h, SEC);
    let (tq, uq) = *seen.borrow().last().unwrap();
    assert_eq!(uq, l1.utc_at(Tsc(tq)), "SEQ-446.1");
    assert!(uq > before + 2_000_000 + 999_000_000, "SEQ-446.1");
    // .2 Ahead by 1 ms: the UTC keeps running, slower, and meets the target after 2 s.
    h.adv(MS);
    let t2 = h.now();
    let cur = h.s.engine().utc_at(Tsc(t2));
    h.push(RingId::Hk, &anchor(t2, cur - 1_000_000, HZ - 30_000_000));
    h.run();
    let l2 = *h.s.engine().clock.utc();
    assert_eq!(
        (l2.lead_ns, l2.utc_at(Tsc(l2.tsc))),
        (1_000_000, cur),
        "SEQ-446.2"
    );
    for _ in 0..4 {
        quote(&mut h, SEC);
    }
    let v: Vec<(u64, i64)> = seen.borrow().clone();
    assert!(v.windows(2).all(|w| w[1].1 >= w[0].1), "SEQ-446.2 {v:?}");
    let (tq, uq) = *v.last().unwrap();
    let line = (i128::from(tq - l2.tsc) * 1_000_000_000_000 / i128::from(HZ - 30_000_000)) as i64;
    assert_eq!(uq, l2.utc_ns + line, "SEQ-446.2");
    // .3 Section 32 carries the line; snapshots without an anchor have no section 32.
    let kf = h.keyframe();
    let (_, secs) = snap(&kf);
    assert_eq!(secs.utc_line, l2, "SEQ-446.3");
    let first = h.snapshots().first().cloned().unwrap();
    assert!(!snap(&first).1.utc_line.is_set(), "SEQ-446.3");
    let before3 = *seen.borrow().last().unwrap();
    h.adv(MS);
    let t3 = h.now();
    let cur = h.s.engine().utc_at(Tsc(t3));
    h.push(RingId::Hk, &anchor(t3, cur + 300_000, HZ - 29_000_000));
    h.run();
    for _ in 0..3 {
        quote(&mut h, SEC);
    }
    let kf2 = h.keyframe();
    assert_eq!(snap(&kf2).1.utc_line, *h.s.engine().clock.utc(), "SEQ-446.3");
    // .4 Replay (from the first snapshot and from the keyframe) reproduces every Now.utc_ns and
    // every snapshot byte.
    let live: Vec<(u64, i64)> = seen.borrow().clone();
    let after_kf: Vec<(u64, i64)> = live.iter().copied().filter(|x| x.0 > kf.tsc).collect();
    seen.borrow_mut().clear();
    let mm = h.verify();
    assert!(mm.is_empty(), "SEQ-446.4 {mm:?}");
    // The full replay also delivers the bring-up quotes that preceded the capture.
    assert!(seen.borrow().ends_with(&live), "SEQ-446.4");
    seen.borrow_mut().clear();
    let mm = h.verify_from(Some(kf.seq));
    assert!(mm.is_empty(), "SEQ-446.4 {mm:?}");
    assert_eq!(*seen.borrow(), after_kf, "SEQ-446.4");
    // A cold reader (the advisor's previous AdvisorTick) recomputes every Now.utc_ns from the log,
    // also before the last UtcAnchor, where the final line extrapolated back is off by its step.
    let bbo_at = |log: &Log, t: u64| {
        log.recs
            .iter()
            .position(|r| r.kind == EventKind::BBO && r.role() == Role::Input && r.tsc == t)
            .unwrap()
    };
    let log = Log::from_segments(&h.j().segments);
    for &(tq, uq) in &live {
        assert_eq!(utc_of_record(&log, bbo_at(&log, tq)), Some(uq), "SEQ-446.4 {tq}");
    }
    assert_ne!(h.s.engine().utc_at(Tsc(before3.0)), before3.1, "SEQ-446.4");
    clear_hook();
    // .5 A restart's EngineClock starts a new run from its own segment pair.
    let rr = Rerun::default();
    let snap_seq = kf2.seq;
    let mut h2 = H::crash_restart(h, plan(SnapshotSel::AtSeq(snap_seq), true, 6, 0), rr.clone()).unwrap();
    let o = h2.run();
    assert!(!matches!(o, StepOutcome::Fatal(_)), "SEQ-446.5 {o:?}");
    let e = h2.s.engine();
    assert!(e.clock.is_set() && !e.clock.utc().is_set(), "SEQ-446.5");
    let t = Tsc(e.clock.row.base_tsc + 1_000);
    assert_eq!(
        e.utc_at(t),
        rr.utc + ((t.0 - rr.tsc) * 1_000_000_000_000 / rr.hz) as i64,
        "SEQ-446.5"
    );
    // The previous run's records keep that run's line for a cold reader of the new journal.
    let log = Log::from_segments(&h2.j().segments);
    let (tl, ul) = *live.last().unwrap();
    assert_eq!(utc_of_record(&log, bbo_at(&log, tl)), Some(ul), "SEQ-446.5");
    let _ = e0;
}
