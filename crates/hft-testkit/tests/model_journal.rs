//! Actual Advisor output is independently recomputed from the initial journal origin.
use hft_event::{AdviceHdr, Bbo, EventKind, MarketHdr, RecordFlags, Timer};
use hft_rt::Heartbeat;
use hft_sequencer::driver::recompute::{RecomputeLimits, recompute};
use hft_sequencer::driver::replay::Log;
use hft_sequencer::driver::verify::VerifyOptions;
use hft_testkit::dst::faults::{CrashFault, Fault};
use hft_testkit::dst::world::WorldSpec;
use hft_testkit::dst::{
    BootCtx, CrashPoint, InputRec, InvariantSet, SeqSut, Source, StepOut, Sut, run_record,
};
use hft_testkit::strategies::TestStrategies;
use hft_types::{GroupId, Px, Qty};
use zerocopy::{FromZeros, IntoBytes};

type E = SeqSut<TestStrategies>;
static HB: Heartbeat = Heartbeat::new();

fn check(log: &Log) -> u64 {
    let r = recompute::<TestStrategies>(
        log,
        VerifyOptions::default(),
        &[],
        RecomputeLimits {
            records: 200_000,
            bytes: 64 << 20,
        },
        &HB,
    )
    .unwrap();
    assert!(r.model_no_start.is_empty(), "{r:?}");
    assert!(r.differences.is_empty(), "{r:?}");
    assert!(r.checkpoint_keys.is_empty());
    assert_eq!(r.started_instances.len(), 1);
    let n = log.recs.iter().filter(|r| r.kind == EventKind::ADVICE).count() as u64;
    assert!(n > 0);
    assert_eq!(r.compared, n);
    n
}

fn deliver_advisor(e: &mut E, now: u64) {
    for input in e.advisor_inputs().unwrap().unwrap() {
        if input.kind == EventKind::ADVICE {
            let h = hft_sequencer::types::read_body::<AdviceHdr>(&input.body).unwrap();
            assert_eq!(
                h.len, 16,
                "ProbeModel emits count/time, synthetic actor emits 8 B"
            );
            assert!(h.basis_seq <= e.seq());
        }
        e.process(now, now, &input, &mut StepOut::default());
    }
}

#[test]
fn tk_152_real_model_replays_across_restarts_and_rejects_changed_advice() {
    let mut w = WorldSpec::w1();
    w.physical_journal = true;
    w.model_advice = true;
    w.strategies[1].kind_id = 0xFF02;
    let mut e = E::boot(&BootCtx {
        seed: 1,
        world: &w,
        planted: &[],
        tsc_hz: 1_000_000_000,
    })
    .unwrap();
    let mut now = 0;
    for phase in 0..4 {
        for update_id in 0..16 {
            now += 50_000_000;
            let inst = &w.insts[0];
            let mut h = MarketHdr::new_zeroed();
            h.venue = inst.venue;
            h.inst = inst.id;
            let b = Bbo {
                h,
                bid_px: Px(inst.bid),
                ask_px: Px(inst.ask),
                bid_qty: Qty(inst.depth),
                ask_qty: Qty(inst.depth),
                update_id,
            };
            e.process(
                now,
                now,
                &InputRec::of(&b, Source::Market),
                &mut StepOut::default(),
            );
            deliver_advisor(&mut e, now);
        }
        let snapshot = Timer {
            owner: 1,
            timer_id: 1,
            group: GroupId(0),
            ..Timer::new_zeroed()
        };
        e.process(
            now,
            now,
            &InputRec::of(&snapshot, Source::Engine),
            &mut StepOut::default(),
        );
        deliver_advisor(&mut e, now);
        let log = e.physical_journal().unwrap().unwrap();
        assert!(log.recs.last().unwrap().flags.contains(RecordFlags::EVENT_END));
        check(&log);
        if phase == 0 {
            e.clean_restart().unwrap();
        } else if phase < 3 {
            e.crash_and_recover(CrashPoint::SeqInput, None).unwrap();
        }
    }
    let log = e.physical_journal().unwrap().unwrap();
    assert_eq!(log.segs.len(), 4);
    for field in ["payload", "basis"] {
        let mut bad = log.clone();
        let r = bad.recs.iter_mut().find(|r| r.kind == EventKind::ADVICE).unwrap();
        let at = r.seq;
        if field == "payload" {
            r.body[size_of::<AdviceHdr>()] ^= 1;
        } else {
            let mut h = r.read::<AdviceHdr>().unwrap();
            h.basis_seq += 1;
            r.body[..size_of::<AdviceHdr>()].copy_from_slice(h.as_bytes());
        }
        let result = recompute::<TestStrategies>(
            &bad,
            VerifyOptions::default(),
            &[],
            RecomputeLimits {
                records: 200_000,
                bytes: 64 << 20,
            },
            &HB,
        );
        assert!(
            result.is_err() || result.is_ok_and(|r| !r.differences.is_empty()),
            "{field} at {at}"
        );
    }
}

#[test]
fn tk_153_model_world_keeps_checker_independence_and_requires_adapters() {
    let suites = hft_testkit::scenarios::suites();
    let mut cfg = hft_testkit::scenarios::replay_config(&suites[0], 1);
    cfg.steps = 600;
    cfg.faults = hft_testkit::dst::FaultRates::off();
    cfg.rerun = hft_testkit::dst::RerunPolicy::Never;
    cfg.hash_every = 1;
    cfg.scripted = vec![(
        0,
        Fault::Crash(CrashFault {
            at_step: 120,
            repeat: 1,
            point: CrashPoint::SeqInput,
        }),
    )];
    let a = run_record::<E>(&cfg);
    assert!(a.outcome.is_pass(), "{}", crate::seq_sut::show(&a));
    check(a.physical_journal.as_ref().unwrap());
    cfg.invariants = InvariantSet::none();
    let b = run_record::<E>(&cfg);
    assert!(b.outcome.is_pass(), "{}", crate::seq_sut::show(&b));
    assert_eq!(a.journal, b.journal);
    assert_eq!(a.frames, b.frames);
    assert_eq!(a.deliveries, b.deliveries);
    assert_eq!(a.trail.to_bytes(), b.trail.to_bytes());
    for model in [true, false] {
        let mut cfg = hft_testkit::dst::DstConfig::new(1, "core");
        cfg.steps = 64;
        cfg.invariants = InvariantSet::none();
        cfg.world.model_advice = model;
        cfg.world.physical_journal = !model;
        let r = run_record::<hft_testkit::dst::ToyEngine>(&cfg);
        let hft_testkit::dst::DstOutcome::Fail(f) = r.outcome else {
            panic!("missing adapter passed")
        };
        assert!(f.message.contains("unavailable"), "{}", f.message);
    }
}
