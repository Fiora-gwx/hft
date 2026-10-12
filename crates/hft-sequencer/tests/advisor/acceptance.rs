//! Existing M2 advisor acceptance cases, driven through the real mirror and checkpoint code.

use hft_event::{Bbo, RecordFlags, Role, Timer};
use hft_sequencer::{EngineSetup, SnapshotSel, StepOutcome};
use hft_types::Tsc;
use zerocopy::{FromZeros, IntoBytes};

use super::*;

fn on_log(log: &Log, setup: &EngineSetup, store: MemCkpt, batch: u16, chunk: u32) -> Adv {
    let mut config = cfg();
    config.batch = batch;
    config.chunk_bytes = chunk;
    config.max_lag_ms = u32::MAX;
    Advisor::start(log, setup, config, store, Ring::big(), hb()).unwrap()
}

fn run_to(a: &mut Adv, log: &Log) {
    for _ in 0..=log.recs.len() {
        let busy = a.poll_once(log);
        assert!(a.failure.is_none(), "{:?}", a.failure);
        clear_log();
        if !busy && !a.checkpointing() {
            assert!(a.mirror.driver().finished(), "mirror must reach the whole input");
            assert!(a.outbox.is_empty());
            return;
        }
    }
    panic!("advisor did not finish");
}

fn prefix(log: &Log, len: usize) -> Log {
    assert!(log.recs[len - 1].flags.contains(RecordFlags::EVENT_END));
    let mut out = log.clone();
    out.recs.truncate(len);
    let segments = out.seg_start.partition_point(|&first| first < len);
    out.seg_start.truncate(segments);
    out.segs.truncate(segments);
    out
}

fn at_seq(log: &Log, seq: u64) -> Log {
    let ix = log.recs.iter().position(|r| r.seq == seq).unwrap();
    assert_eq!(log.recs[ix].kind, EventKind::BBO);
    prefix(log, ix + 1)
}

fn count(a: &Adv) -> u64 {
    a.mirror
        .instances()
        .models
        .slots
        .iter()
        .filter_map(|s| s.model.as_ref())
        .find(|m| m.i.strategy == S2)
        .unwrap()
        .n_events
}

fn counters() -> Vec<(u64, [u8; 256])> {
    SCRIPT.with(|x| core::mem::take(&mut x.borrow_mut().counter_outputs))
}

/// A cross-run mirror reconstructs assembly from the journal. B0's compact RSK1/ECF fixture
/// intentionally needs injected decoders and is only suitable for single-run unit tests.
fn complete_model_boot() -> H {
    use hft_sequencer::driver::live::LiveDriver;
    use hft_sequencer::{BootSource, Sequencer};
    use hft_types::HostClass;

    reset_script();
    set_subs(S2, vec![(A1, 0, EvMask::BBO, Part::Model)]);
    let mut b = b0();
    for p in &mut b.params {
        p.values[F_CKPT] = 64;
    }
    let rd = b.rd.load();
    let mut setup = b.setup(&rd);
    setup.risk_decode = Some(hft_sequencer::config_blob::decode_risk_blob);
    let mut configs = b.configs(&rd);
    configs.engine_blob = setup.encode_blob(1, HostClass::Dev2, UTC0);
    configs.risk_blob = configs.risk.encode_blob().unwrap();
    configs.risk_sha256 = sha(&configs.risk_blob);
    let heartbeat = hb();
    let s = Sequencer::boot(
        BootSource::Empty { configs },
        &setup,
        LiveDriver::new(TestIo::new(setup.clock), b.live),
        heartbeat,
    )
    .unwrap();
    let mut h = H {
        s,
        rd,
        setup,
        cmd_no: 0,
        hb: heartbeat,
        upd: 1,
    };
    h.steady();
    h
}

/// The payload's counter comes from restored Model state. The oracle below independently
/// counts the subscribed journal inputs; it does not read that state or emitted Advice.
fn long_source(inputs: usize) -> H {
    let mut h = b0_model();
    SCRIPT.with(|x| x.borrow_mut().counter_advice = Some(G2));
    let existing = h.j().records().filter(|r| r.role() == Role::Input).count();
    for k in existing..inputs {
        h.quote(A1, "50000", if k % 2 == 0 { "50002" } else { "50003" });
        assert_eq!(h.run(), StepOutcome::Idle);
        if k % 512 == 0 {
            clear_log();
        }
    }
    clear_log();
    assert_eq!(
        h.j().records().filter(|r| r.role() == Role::Input).count(),
        inputs
    );
    assert!(h.verify().is_empty());
    clear_log();
    h
}

fn expected_advice(log: &Log) -> Vec<(u64, Vec<u8>)> {
    log.recs
        .iter()
        .filter(|r| r.kind == EventKind::BBO && r.read::<Bbo>().is_some_and(|b| b.h.inst == A1))
        .enumerate()
        .map(|(i, r)| {
            let mut hdr = AdviceHdr::new_zeroed();
            hdr.strategy = S2;
            hdr.account = AC2;
            hdr.group = G2;
            hdr.local_kind = 7;
            hdr.len = 256;
            hdr.basis_seq = r.seq;
            hdr.basis_tsc = Tsc(r.tsc);
            let mut payload = [0xA5; 256];
            payload[..8].copy_from_slice(&(i as u64 + 1).to_le_bytes());
            payload[8..16].copy_from_slice(&r.seq.to_le_bytes());
            let mut bytes = hdr.as_bytes().to_vec();
            bytes.extend_from_slice(&payload);
            (r.seq, bytes)
        })
        .collect()
}

#[test]
fn seq_036_model_tick_from_journal() {
    reset_script();
    set_subs(S2, vec![(A1, 0, EvMask::BBO, Part::Model)]);
    SCRIPT.with(|x| x.borrow_mut().model_tick.push((S2, 1_000)));
    let mut h = b0().boot();
    h.steady();
    h.sys(6); // UTC origin for crossing; the first AdvisorTick deliberately emits no tick.
    h.keyframe();
    for second in [1, 2] {
        h.at(T0 + second * SEC);
        h.run();
    }
    h.at(T0 + 2 * SEC + 500 * MS);
    for _ in 0..1_000 {
        h.quote(A1, "50000", "50003");
        h.run();
    }
    h.at(T0 + 3 * SEC);
    h.run();
    let log = committed(h.j());
    let ticks: Vec<_> = log
        .recs
        .iter()
        .filter(|r| r.tsc > T0)
        .filter(|r| {
            r.kind == EventKind::TIMER && r.read::<Timer>().is_some_and(|t| t.owner == 1 && t.timer_id == 6)
        })
        .map(|r| (r.seq, (r.tsc - T0) as i64))
        .collect();
    assert_eq!(
        ticks.iter().map(|x| x.1).collect::<Vec<_>>(),
        vec![SEC as i64, 2 * SEC as i64, 3 * SEC as i64]
    );
    for batch in [1, 137] {
        let mut a = on_log(&log, &h.setup, MemCkpt::default(), batch, 1 << 20);
        clear_log();
        while a.poll_once(&log) {
            assert!(a.failure.is_none());
        }
        let got = model_log(S2);
        assert_eq!(got.iter().filter(|g| g.kind == "Bbo").count(), 1_000);
        assert_eq!(
            got.iter()
                .filter(|g| g.kind == "Timer")
                .map(|g| (g.now.seq, g.now.engine_ns))
                .collect::<Vec<_>>(),
            ticks
        );
        let before = got.len();
        for _ in 0..100 {
            assert!(!a.poll_once(&log));
        }
        assert_eq!(model_log(S2).len(), before, "idle polls cannot invent ticks");
    }
    assert!(h.verify().is_empty());
}

#[test]
fn seq_039_reset_note_dedup_key_on_catchup() {
    reset_script();
    let mut b = b0();
    let other = GroupId(0x0200_0002);
    let mut p = Spb::s2(1);
    p.groups = vec![gspec(G2), gspec_eth(other)];
    b.params = vec![Spb::s1(1), p.clone(), Spb::s9(1)];
    let mut h = b.boot();
    h.steady();
    let mut resets = GroupResets::new(8);
    resets.set(0, ResetCause::Structural);
    resets.set(1, ResetCause::Structural);
    SCRIPT.with(|x| {
        x.borrow_mut()
            .reset_effect
            .push((5, ModelParamEffect::ResetGroups(resets)))
    });
    let mut a = start(&h, MemCkpt::default(), Ring::big(), cfg());
    catch_up(&mut a, &h);
    let store = checkpoint_now(&mut a);
    let last = store.gens[&S2].0.last_seq;
    p.version = 5;
    p.values[F_THR] = 5;
    let command = h.set_params(&p);
    h.run();
    assert_eq!(h.result_of(command).map(|r| r.status), Some(0));
    assert!(
        h.recs()
            .iter()
            .any(|r| r.kind == EventKind::CONTROL_CMD && r.seq > last)
    );
    catch_up(&mut a, &h);
    let reset_notes = |a: &Adv| {
        a.ring
            .notes()
            .into_iter()
            .filter(|n| n.code == hft_event::note_code(NoteSource::Advisor, 4))
            .collect::<Vec<_>>()
    };
    let first = reset_notes(&a);
    assert_eq!(
        first
            .iter()
            .map(|n| (n.strategy, n.group, n.v[0], n.v[1]))
            .collect::<Vec<_>>(),
        vec![(S2, G2, 1, 5), (S2, other, 1, 5)]
    );
    drop(a);
    let mut resumed = start(&h, store, Ring::big(), cfg());
    catch_up(&mut resumed, &h);
    let second = reset_notes(&resumed);
    assert_eq!(second.len(), 2);
    for (a, b) in first.iter().zip(&second) {
        assert_eq!(a.as_bytes(), b.as_bytes());
    }
    assert!(h.verify().is_empty());
}

#[test]
fn seq_041_advisor_lag_rate_limited() {
    let mut h = b0_model();
    let mut a = start(&h, MemCkpt::default(), Ring::big(), cfg());
    catch_up(&mut a, &h);
    let mut ends = vec![h.j().committed];
    for sec in 1..=61 {
        h.at(T0 + sec * SEC);
        h.quote(A1, "50000", "50003");
        h.run();
        ends.push(h.j().committed);
    }
    let log = committed(h.j());
    let mut observed = Vec::new();
    let code = hft_event::note_code(NoteSource::Advisor, 3);
    for elapsed in 0..55 {
        let end_second = elapsed + 6;
        let consumed_second = if elapsed < 25 { elapsed } else { end_second - 1 };
        let published = prefix(&log, ends[end_second]);
        a.feed(&published);
        // Model bounded processing with a deterministic reader frontier, while lag observes
        // the actual committed frontier. No engine state, clock, or lag counter is rewritten.
        a.mirror.driver_mut().end = ends[consumed_second];
        for _ in 0..10 {
            if !a.poll_once(&published) {
                break;
            }
        }
        assert_eq!(a.mirror.engine().last_ens, consumed_second as i64 * SEC as i64);
        for n in a
            .ring
            .notes()
            .into_iter()
            .filter(|n| n.code == code)
            .skip(observed.len())
        {
            observed.push((elapsed, n));
        }
    }
    assert_eq!(observed.iter().map(|x| x.0).collect::<Vec<_>>(), vec![0, 10, 20]);
    assert!(
        observed
            .iter()
            .all(|(_, n)| n.strategy == S2 && n.v[0] == 2 && n.v[1] == 6_000)
    );
    assert!(a.failure.is_none());
    assert!(h.verify().is_empty());
}

#[test]
fn seq_042_lag_continuous_across_restart() {
    let mut h = complete_model_boot();
    h.at(T0 + 98 * SEC);
    h.run();
    let mut config = cfg();
    config.shard = Some(vec![S1, S2]);
    let mut a = start(&h, MemCkpt::default(), Ring::big(), config.clone());
    catch_up(&mut a, &h);
    let earlier = checkpoint_now(&mut a).gens.remove(&S1).unwrap();
    h.at(T0 + 99 * SEC);
    h.quote(A1, "50000", "50003");
    h.run();
    catch_up(&mut a, &h);
    let mut store = checkpoint_now(&mut a);
    store.gens.insert(S1, earlier);
    let first_checkpoint_seq = store.gens[&S1].0.last_seq;
    let checkpoint_seq = store.gens[&S2].0.last_seq;
    assert_eq!(store.gens[&S2].0.last_ens, 99 * SEC as i64);
    // This runtime hint is not a checkpoint-file field and must never be the lag authority.
    store.gens.get_mut(&S2).unwrap().0.last_ens = -123;
    h.at(T0 + 100 * SEC);
    h.run();
    let mut h = H::crash_restart(
        h,
        plan(SnapshotSel::Latest, true, 1, 0),
        Rerun {
            tsc: 11 * MS,
            utc: UTC0 + 700 * SEC as i64,
            ..Rerun::default()
        },
    )
    .unwrap();
    h.run();
    h.adv(6 * SEC);
    h.quote(A1, "50000", "50004");
    h.run();
    assert_eq!(h.s.engine().last_ens, 106 * SEC as i64);
    let log = committed(h.j());
    config.batch = 1;
    let mut resumed: Adv = Advisor::start(&log, &h.setup, config, store, Ring::big(), hb()).unwrap();
    assert!(resumed.poll_once(&log));
    assert!(
        resumed.applied_seq() < checkpoint_seq,
        "mirror is still reconstructing the prefix"
    );
    let lag: Vec<_> = resumed
        .ring
        .notes()
        .into_iter()
        .filter(|n| n.code == hft_event::note_code(NoteSource::Advisor, 3))
        .collect();
    assert_eq!(lag.len(), 2);
    assert_eq!(
        (lag[0].v[0], lag[0].v[1], lag[0].v[3]),
        (1, 8_000, first_checkpoint_seq)
    );
    assert_eq!(
        (lag[1].v[0], lag[1].v[1], lag[1].v[3]),
        (2, 7_000, checkpoint_seq)
    );
    assert_eq!(lag[0].v[2], log.recs.last().unwrap().seq - first_checkpoint_seq);
    assert_eq!(lag[1].v[2], log.recs.last().unwrap().seq - checkpoint_seq);
    run_to(&mut resumed, &log);
    assert_eq!(resumed.mirror.engine().last_ens, 106 * SEC as i64);
    assert_eq!(
        resumed
            .ring
            .notes()
            .iter()
            .filter(|n| n.code == lag[0].code)
            .count(),
        2
    );
}

/// Independent one-key outbox + byte-capacity model. It consumes journal-derived expected
/// emissions, never the advisor's outbox, dropped-count, or captured payloads.
#[derive(Default)]
struct ExpectedRing {
    pending: Option<Vec<u8>>,
    used: usize,
    sent: Vec<Vec<u8>>,
    coalesced: u64,
}

impl ExpectedRing {
    fn flush(&mut self, capacity: usize) {
        if self
            .pending
            .as_ref()
            .is_some_and(|b| b.len() <= capacity - self.used)
        {
            let b = self.pending.take().unwrap();
            self.used += b.len();
            self.sent.push(b);
        }
    }
    fn emit(&mut self, b: &[u8]) {
        if self.pending.replace(b.to_vec()).is_some() {
            self.coalesced += 1;
        }
    }
    fn compare(&self, a: &Adv, from: usize) {
        assert_eq!(a.ring.recs.len(), self.sent.len());
        for ((kind, got), want) in a.ring.recs.iter().zip(&self.sent).skip(from) {
            assert_eq!(*kind, EventKind::ADVICE);
            assert_eq!(got, want);
        }
        assert_eq!(a.advice_coalesced, self.coalesced);
    }
}

#[test]
fn seq_043_model_deterministic() {
    let h = long_source(50_000);
    let log = committed(h.j());
    let expected = expected_advice(&log);
    assert!(expected.len() > 49_000);
    let mut totals = Vec::new();
    for capacity in [64 * 1024, 4 * 1024] {
        let mut a = on_log(&log, &h.setup, MemCkpt::default(), 64, 1 << 20);
        a.flush(); // Drain the startup Missing-checkpoint Meta note before this schedule.
        a.ring.recs.clear();
        a.ring.used = 0;
        a.ring.cap = capacity;
        let mut oracle = ExpectedRing::default();
        let mut consumed = 0;
        let mut complete = false;
        for poll in 0..=log.recs.len() {
            if poll % 64 == 0 {
                a.ring.used = 0;
                oracle.used = 0;
            }
            let checked = oracle.sent.len();
            oracle.flush(capacity);
            let busy = a.poll_once(&log);
            while consumed < expected.len() && expected[consumed].0 <= a.applied_seq() {
                oracle.emit(&expected[consumed].1);
                consumed += 1;
            }
            oracle.flush(capacity);
            oracle.compare(&a, checked);
            clear_log();
            if !busy {
                a.ring.used = 0;
                oracle.used = 0;
                let checked = oracle.sent.len();
                a.flush();
                oracle.flush(capacity);
                oracle.compare(&a, checked);
                complete = true;
                break;
            }
        }
        assert!(complete && a.failure.is_none() && a.mirror.driver().finished());
        assert_eq!(consumed, expected.len());
        assert_eq!(count(&a) as usize, expected.len());
        assert!(oracle.pending.is_none() && a.outbox.is_empty());
        assert_eq!(oracle.sent.last(), expected.last().map(|r| &r.1));
        assert_eq!(oracle.sent.len() as u64 + oracle.coalesced, expected.len() as u64);
        let raw = counters();
        assert_eq!(raw.len(), expected.len());
        for ((seq, payload), (want_seq, bytes)) in raw.iter().zip(&expected) {
            assert_eq!(seq, want_seq);
            assert_eq!(payload.as_slice(), &bytes[32..]);
        }
        totals.push(oracle.coalesced);
    }
    assert!(
        totals[1] > totals[0],
        "small ring must actually cause additional overwrites"
    );
}

#[test]
fn seq_046_checkpoint_resume_equals_continuous() {
    let h = long_source(100_000);
    let log = committed(h.j());
    let head = at_seq(&log, 40_000);
    let stopped = at_seq(&log, 60_000);
    let expected = expected_advice(&log);
    let mut continuous = on_log(&log, &h.setup, MemCkpt::default(), 256, 1 << 20);
    run_to(&mut continuous, &log);
    let whole = counters();
    assert_eq!(whole.len(), expected.len());
    for ((seq, payload), (want_seq, bytes)) in whole.iter().zip(&expected) {
        assert_eq!(seq, want_seq);
        assert_eq!(payload.as_slice(), &bytes[32..]);
    }
    let suffix: Vec<_> = whole.iter().filter(|r| r.0 > 60_000).copied().collect();
    assert!(suffix.len() >= 40_000);
    let final_store = checkpoint_now(&mut continuous);
    for kib in [256, 1_024, 4_096] {
        let mut original = on_log(&head, &h.setup, MemCkpt::default(), 256, kib * 1024);
        run_to(&mut original, &head);
        let store = checkpoint_now(&mut original);
        assert_eq!(store.gens[&S2].0.last_seq, 40_000);
        run_to(&mut original, &stopped);
        assert_eq!(original.applied_seq(), 60_000);
        assert!(count(&original) > u64::from_le_bytes(store.gens[&S2].1[8..16].try_into().unwrap()));
        drop(original);
        counters();
        let mut resumed = on_log(&stopped, &h.setup, store, 256, kib * 1024);
        run_to(&mut resumed, &log);
        let raw = counters();
        assert_eq!(raw.first().map(|r| r.0), Some(40_001));
        assert_eq!(
            raw.into_iter().filter(|r| r.0 > 60_000).collect::<Vec<_>>(),
            suffix,
            "every generated payload before outbox coalescing, chunk {kib} KiB"
        );
        assert_eq!(count(&resumed) as usize, expected.len());
        assert_eq!(checkpoint_now(&mut resumed).gens[&S2], final_store.gens[&S2]);
    }
}
