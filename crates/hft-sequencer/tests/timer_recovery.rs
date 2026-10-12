//! Stable SDK timer identities across real restarts (SEQ-408–SEQ-410).

use hft_event::{ControlCmd, SnapshotHdr};
use hft_sequencer::snapshot::SECTION_STRATEGY_TIMERS;
use hft_sequencer::{BootError, SnapshotSel, StepOutcome};
use hft_strategy_sdk::*;
use hft_types::{AccountId, GroupId};
use zerocopy::{FromBytes, IntoBytes};

use crate::common::*;

struct SavedTimers {
    fast: <TestSet as StrategySet>::Inst,
    ids: [u64; 8],
    fired: [u64; 8],
}

/// The IDs live only in the strategy's own snapshot, never in the test script or an Rc.
struct TimerSet;

impl StrategySet for TimerSet {
    type Inst = SavedTimers;
    type ModelInst = <TestSet as StrategySet>::ModelInst;
    type Params = <TestSet as StrategySet>::Params;

    fn kinds() -> &'static [StrategyDesc] {
        TestSet::kinds()
    }
    fn decode_params(k: u16, b: &[u8], v: u16, g: &GroupTable) -> Result<Self::Params, ParamError> {
        TestSet::decode_params(k, b, v, g)
    }
    fn validate_params(p: &Self::Params, g: &GroupTable, r: &RefView<'_>) -> Result<(), ParamError> {
        TestSet::validate_params(p, g, r)
    }
    fn create(
        i: StrategyInstance,
        p: &Self::Params,
        g: &GroupTable,
        r: &RefView<'_>,
    ) -> Result<Self::Inst, ParamError> {
        Ok(SavedTimers {
            fast: TestSet::create(i, p, g, r)?,
            ids: [0; 8],
            fired: [0; 8],
        })
    }
    fn create_model(
        i: StrategyInstance,
        p: &Self::Params,
        g: &GroupTable,
        r: &RefView<'_>,
    ) -> Self::ModelInst {
        TestSet::create_model(i, p, g, r)
    }
    fn on_event<V: Views>(s: &mut Self::Inst, ev: &EventRef<'_>, v: &V, ctx: &mut Ctx<'_>) {
        TestSet::on_event(&mut s.fast, ev, v, ctx);
        if s.fast.i.strategy != S1 {
            return;
        }
        if let EventRef::Timer { tag, .. } = ev {
            if let Some(n) = tag.checked_sub(11).and_then(|i| s.fired.get_mut(i as usize)) {
                *n += 1;
            }
        }
        let EventRef::Bbo { inst, b, .. } = ev else {
            return;
        };
        if *inst != A1 {
            return;
        }
        let now = v.now();
        if b.ask_px == px("50031") {
            // IDs 4/5 have an identical physical heap key. ID order differs from deadlines.
            for (i, (ms, tag)) in [(10, 11), (70, 12), (20, 13), (40, 14), (40, 14), (30, 15)]
                .into_iter()
                .enumerate()
            {
                s.ids[i] = ctx
                    .timer_at(now.tsc_of(now.engine_ns + ms * 1_000_000), tag, G1)
                    .unwrap()
                    .0;
            }
            ctx.cancel_timer(TimerId(s.ids[2]));
        } else if b.ask_px == px("50032") {
            // Both live and stale IDs come from the strategy state restored by the engine.
            for i in [1, 3, 0, 2] {
                ctx.cancel_timer(TimerId(s.ids[i]));
            }
            s.ids[6] = ctx
                .timer_at(now.tsc_of(now.engine_ns + 50_000_000), 16, G1)
                .unwrap()
                .0;
        } else if b.ask_px == px("50033") {
            // By this point all prior timers have fired or been cancelled.
            s.ids[7] = ctx
                .timer_at(now.tsc_of(now.engine_ns + 1_000_000), 17, G1)
                .unwrap()
                .0;
        }
    }
    fn on_params(
        s: &mut Self::Inst,
        p: &Self::Params,
        g: &GroupTable,
        r: &RefView<'_>,
    ) -> Result<(), ParamError> {
        TestSet::on_params(&mut s.fast, p, g, r)
    }
    fn subscriptions(s: &Self::Inst, g: &GroupTable, out: &mut SubsBuilder<'_>) {
        TestSet::subscriptions(&s.fast, g, out);
    }
    fn snapshot(s: &Self::Inst, w: &mut SnapshotWriter) {
        TestSet::snapshot(&s.fast, w);
        for n in s.ids.iter().chain(&s.fired) {
            w.put_u64(*n);
        }
    }
    fn restore(s: &mut Self::Inst, r: &mut SnapshotReader<'_>) -> Result<(), RestoreError> {
        TestSet::restore(&mut s.fast, r)?;
        for n in s.ids.iter_mut().chain(&mut s.fired) {
            *n = r.get_u64()?;
        }
        Ok(())
    }
    fn model_mem_bytes(p: &Self::Params, n: u16) -> u64 {
        TestSet::model_mem_bytes(p, n)
    }
    fn model_snapshot_bytes(p: &Self::Params, n: u16) -> u64 {
        TestSet::model_snapshot_bytes(p, n)
    }
    fn has_model(k: u16) -> bool {
        TestSet::has_model(k)
    }
    fn model_layout(k: u16) -> &'static ModelLayoutDesc {
        TestSet::model_layout(k)
    }
    fn model_layout_version(k: u16) -> u32 {
        TestSet::model_layout_version(k)
    }
    fn model_on_event<V: ModelViews>(
        m: &mut Self::ModelInst,
        e: &EventRef<'_>,
        v: &V,
        o: &mut AdviceOut<'_>,
    ) {
        TestSet::model_on_event(m, e, v, o);
    }
    fn model_on_params(
        m: &mut Self::ModelInst,
        p: &Self::Params,
        g: &GroupTable,
        r: &RefView<'_>,
    ) -> ModelParamEffect {
        TestSet::model_on_params(m, p, g, r)
    }
    fn model_snapshot_chunk(
        m: &Self::ModelInst,
        from: u16,
        w: &mut SnapshotWriter,
        max: u32,
    ) -> SnapshotProgress {
        TestSet::model_snapshot_chunk(m, from, w, max)
    }
    fn model_restore(m: &mut Self::ModelInst, r: &mut SnapshotReader<'_>) -> Result<(), RestoreError> {
        TestSet::model_restore(m, r)
    }
    fn model_path_version(k: u16) -> u32 {
        TestSet::model_path_version(k)
    }
    fn model_snapshot_path(m: &Self::ModelInst, w: &mut SnapshotWriter) {
        TestSet::model_snapshot_path(m, w);
    }
    fn model_restore_path(m: &mut Self::ModelInst, r: &mut SnapshotReader<'_>) -> Result<(), RestoreError> {
        TestSet::model_restore_path(m, r)
    }
    fn model_path_settle_ms(p: &Self::Params) -> u64 {
        TestSet::model_path_settle_ms(p)
    }
    fn reset_preview(p: &Self::Params, g: &GroupTable, q: &Self::Params, h: &GroupTable) -> ResetPreview {
        TestSet::reset_preview(p, g, q, h)
    }
    fn schema_json(k: u16) -> Option<String> {
        TestSet::schema_json(k)
    }
    fn cmds(k: u16) -> &'static [CmdDesc] {
        TestSet::cmds(k)
    }
}

fn state(h: &H<TimerSet>) -> &SavedTimers {
    h.s.instances().get(S1).unwrap().inst.as_ref().unwrap()
}

fn registered(h: &H<TimerSet>) -> Vec<u64> {
    h.s.instances()
        .get(S1)
        .unwrap()
        .timers
        .iter()
        .map(|t| t.id.0)
        .collect()
}

fn rig() -> H<TimerSet> {
    reset_script();
    let mut h = B0::<TimerSet>::new().boot();
    h.steady();
    h.quote(A1, "50000", "50031");
    h.run();
    assert_eq!(state(&h).ids, [1, 2, 3, 4, 5, 6, 0, 0]);
    h.at(h.now() + 10_000_000);
    h.run();
    assert_eq!(registered(&h), [2, 4, 5, 6]);
    assert_eq!(state(&h).fired[0], 1);
    h
}

fn verify(h: &H<TimerSet>, window: u64, runs: usize) {
    #[cfg(feature = "replay-io")]
    {
        use hft_sequencer::driver::replay::Log;
        use hft_sequencer::driver::verify::{VerifyOptions, verify};
        let log = Log::from_segments(&h.j().segments);
        for from_seq in [None, Some(window)] {
            let got = verify::<TimerSet>(
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
            if from_seq.is_none() {
                assert_eq!(got.runs.len(), runs);
            }
        }
    }
    #[cfg(not(feature = "replay-io"))]
    let _ = (h, window, runs);
}

#[test]
fn seq_408_timer_ids_survive_crash_drain_and_repeated_restart() {
    for clean in [false, true] {
        let mut h = rig();
        let window = h.keyframe().seq;
        if clean {
            h.cmd_confirmed(
                AccountId(0),
                &ControlCmd::Drain {
                    cancel_resting: false,
                },
            );
            assert!(matches!(h.run(), StepOutcome::Done { .. }));
        }
        let start = if clean {
            SnapshotSel::DrainTail
        } else {
            SnapshotSel::Latest
        };
        let mut h = h
            .crash_restart(
                plan(start, !clean, 2, 0),
                Rerun {
                    tsc: 500_000,
                    hz: 3 * HZ,
                    ..Default::default()
                },
            )
            .unwrap();
        assert_eq!(registered(&h), [2, 4, 5, 6]);
        assert_eq!(state(&h).ids, [1, 2, 3, 4, 5, 6, 0, 0]);
        h.run();
        h.quote(A1, "50000", "50032");
        h.run();
        assert_eq!(state(&h).ids[6], 7);
        assert_eq!(registered(&h), [5, 6, 7]);
        h.keyframe();
        // Crash before any of the surviving registrations expire, with another clock.
        let mut h = h
            .crash_restart(
                plan(SnapshotSel::Latest, true, 3, 0),
                Rerun {
                    tsc: 900_000,
                    hz: HZ,
                    ..Default::default()
                },
            )
            .unwrap();
        assert_eq!(registered(&h), [5, 6, 7]);
        h.run();
        h.at(h.now() + 60_000_000);
        h.run();
        assert!(registered(&h).is_empty());
        assert_eq!(state(&h).fired, [1, 0, 0, 1, 1, 1, 0, 0]);
        h.keyframe();
        verify(&h, window, 3);
    }
}

#[test]
fn seq_410_empty_heap_retains_consumed_timer_ids() {
    let mut h = rig();
    let window = h.keyframe().seq;
    h.quote(A1, "50000", "50032");
    h.run();
    h.at(h.now() + 60_000_000);
    h.run();
    assert!(registered(&h).is_empty());
    h.keyframe();
    let mut h = h
        .crash_restart(plan(SnapshotSel::Latest, true, 2, 0), Rerun::default())
        .unwrap();
    assert!(registered(&h).is_empty());
    assert_eq!(h.s.instances().get(S1).unwrap().bufs.next_timer_id(), 8);
    h.run();
    h.quote(A1, "50000", "50033");
    h.run();
    assert_eq!(state(&h).ids[7], 8);
    h.at(h.now() + 1_000_000);
    h.run();
    h.keyframe();
    assert_eq!(state(&h).fired[6], 1);
    verify(&h, window, 2);
}

fn frame(body: &[u8], section: u16) -> (usize, usize) {
    let base = size_of::<SnapshotHdr>();
    let mut r = SnapshotReader::new(&body[base..]);
    loop {
        let start = base + r.pos();
        let s = r.next_section().unwrap().unwrap();
        if s.section == section {
            return (start, base + r.pos());
        }
    }
}

fn u32_at(b: &mut [u8], at: usize, value: u32) {
    b[at..at + 4].copy_from_slice(&value.to_le_bytes());
}
fn u64_at(b: &mut [u8], at: usize, value: u64) {
    b[at..at + 8].copy_from_slice(&value.to_le_bytes());
}

#[test]
fn seq_409_invalid_or_missing_timer_identity_history_is_rejected() {
    // Missing history is rejected with both live registrations and a genuinely empty heap.
    for empty in [false, true] {
        let mut h = rig();
        if empty {
            h.at(h.now() + 100_000_000);
            h.run();
        }
        let snap = h.keyframe();
        let (start, end) = frame(&snap.body, SECTION_STRATEGY_TIMERS);
        let hdr = start + 16;
        let row = hdr + 16;
        for case in 0..if empty { 1 } else { 15 } {
            let mut j = h.j().clone();
            let rec = j
                .segments
                .iter_mut()
                .flat_map(|s| &mut s.records)
                .find(|r| r.seq == snap.seq)
                .unwrap();
            let b = &mut rec.body;
            match case {
                0 => {
                    b.drain(start..end);
                    let mut hd = SnapshotHdr::read_from_bytes(&b[..size_of::<SnapshotHdr>()]).unwrap();
                    hd.n_sections -= 1;
                    b[..size_of::<SnapshotHdr>()].copy_from_slice(hd.as_bytes());
                }
                1 => {
                    b[start + 2..start + 4].copy_from_slice(&2u16.to_le_bytes());
                }
                2 => {
                    u64_at(b, hdr + 8, 0);
                }
                3 => {
                    u64_at(b, hdr + 8, 6);
                } // next ID must exceed every live ID.
                4 => {
                    u64_at(b, row, 0);
                }
                5 => {
                    u64_at(b, row + 24, 2);
                } // duplicate ID.
                6 => {
                    b[row + 8] ^= 1;
                } // deadline differs from section 8.
                7 => {
                    u32_at(b, row + 16, 90);
                }
                8 => {
                    u32_at(b, row + 20, GroupId(123).0);
                }
                9 => {
                    b[hdr + 1] = 1;
                } // reserved bytes.
                10 => {
                    b[end - 16] = 10;
                } // unknown bound instance instead of S9.
                11 => {
                    // Drop the last instance's empty counter record, keeping valid framing.
                    b.drain(end - 16..end);
                    u32_at(b, start + 4, 2);
                    u32_at(b, start + 8, (end - start - 32) as u32);
                }
                12 => {
                    // Duplicate a physical strategy timer; multiplicity must also match.
                    let (a, z) = frame(b, 8);
                    let (_, secs) = crate::common::snap(&snap);
                    let t = secs
                        .timers
                        .iter()
                        .find(|t| t.owner >= hft_event::TIMER_OWNER_STRATEGY_BASE)
                        .unwrap();
                    let count = secs.timers.len() as u32;
                    b.splice(z..z, t.as_bytes().iter().copied());
                    u32_at(b, a + 4, count + 1);
                    u32_at(b, a + 8, (z - a - 16 + 24) as u32);
                }
                13 => {
                    let extra = b[start..end].to_vec();
                    b.extend_from_slice(&extra);
                    let mut hd = SnapshotHdr::read_from_bytes(&b[..size_of::<SnapshotHdr>()]).unwrap();
                    hd.n_sections += 1;
                    b[..size_of::<SnapshotHdr>()].copy_from_slice(hd.as_bytes());
                }
                14 => {
                    u32_at(b, hdr + 4, u32::MAX);
                }
                _ => unreachable!(),
            }
            let err = H::<TimerSet>::restart_over(
                j,
                h.setup.clone(),
                plan(SnapshotSel::Latest, false, 2, 0),
                Rerun::default(),
            )
            .err();
            assert_eq!(
                err,
                Some(BootError::SnapshotUndecodable {
                    section: SECTION_STRATEGY_TIMERS
                }),
                "empty={empty} case={case}"
            );
        }
    }
}
