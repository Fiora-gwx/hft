//! Real Drain/restart executions with checkpoint catch-up across a new TSC epoch.
use super::model::{Ring, Store, drain, poll, published};
use super::*;
use hft_sequencer::advisor::{Advisor, AdvisorCfg, CkptHdr, checkpoint, ckpt_checksum};

type Adv<S> = Advisor<S, Store, Ring>;
type Live<S> = Sequencer<LiveDriver<S, Io<S>>, S>;

fn advisor<S: StrategySet>(
    s: &mut Live<S>,
    f: &Fixture,
    setup: &hft_sequencer::EngineSetup,
    store: Store,
) -> Adv<S> {
    let log = published(s, f);
    let mut a = Advisor::start(
        &log,
        setup,
        AdvisorCfg {
            batch: 1,
            checkpoint_ms: 0,
            ..Default::default()
        },
        store,
        Ring::default(),
        Box::leak(Box::new(Heartbeat::new())),
    )
    .unwrap();
    poll(&mut a, &log);
    drain(&mut a, s);
    a
}

fn market<S: StrategySet>(
    s: &mut Live<S>,
    a: &mut Adv<S>,
    f: &Fixture,
    w: &WorldSpec,
    seconds: u64,
    tick: &mut u64,
) {
    let before = s.engine().last_ens;
    for _ in 0..seconds / 10 {
        let t = s.driver().io.rings.tsc + s.driver().io.rings.hz / 100;
        let utc = s.driver().io.utc_of(Tsc(t));
        let io = &mut s.driver_mut().io.rings;
        io.tsc = t;
        io.calib.tsc = Tsc(t);
        io.calib.realtime_ns = utc;
        let status = s.driver().io.clock_status();
        input(s, (1, 0), EventKind::CLOCK_STATUS, status.as_bytes().to_vec());
        *tick += 1;
        for (vi, venue) in w.venues.iter().enumerate() {
            for i in w.insts.iter().filter(|i| i.venue == venue.id) {
                let mut h = MarketHdr::new_zeroed();
                h.venue = i.venue;
                h.inst = i.id;
                h.rx_tsc = t;
                h.ex_ts_ns = utc;
                let shift = (*tick % 41) as i64 * i.tick;
                let b = Bbo {
                    h,
                    bid_px: Px(i.bid + shift),
                    ask_px: Px(i.ask + shift),
                    bid_qty: Qty(i.depth),
                    ask_qty: Qty(i.depth),
                    update_id: *tick,
                };
                input(s, (4, vi as u8), EventKind::BBO, b.as_bytes().to_vec());
            }
        }
        settle(s);
        let log = published(s, f);
        poll(a, &log);
        drain(a, s);
    }
    assert_eq!(s.engine().last_ens - before, seconds as i64 * 1_000_000_000);
}

fn save<S: StrategySet>(s: &mut Live<S>, a: &mut Adv<S>, f: &Fixture) -> (CkptHdr, Vec<u8>) {
    let log = published(s, f);
    poll(a, &log);
    a.begin_checkpoints();
    poll(a, &log);
    a.ckpt.committed.values().next().unwrap().clone()
}

pub(super) fn exercise<S: StrategySet>(
    mut s: Live<S>,
    f: &Fixture,
    w: &WorldSpec,
    setup: &mut hft_sequencer::EngineSetup,
    spec: ModelRestart,
) -> Live<S> {
    assert!((1..=2).contains(&spec.count));
    assert!(spec.reason <= 3);
    let mut a = advisor(&mut s, f, setup, Store::default());
    let mut tick = 0;
    market(&mut s, &mut a, f, w, 300, &mut tick);
    std::fs::create_dir(f.root.join("checkpoints")).unwrap();
    let mut boundaries = Vec::new();
    for turn in 0..spec.count {
        let (h, body) = save(&mut s, &mut a, f);
        let bytes = checkpoint::encode(&h, &body, 1 << 20).unwrap();
        if turn == 0 {
            std::fs::write(f.root.join("checkpoints/tsa.ckpt"), bytes).unwrap();
        }
        market(&mut s, &mut a, f, w, 30, &mut tick);
        command(
            &mut s,
            100 + u64::from(turn),
            &ControlCmd::Drain { cancel_resting: true },
        );
        let mut done = false;
        for _ in 0..1000 {
            match s.step() {
                StepOutcome::Done { .. } => {
                    done = true;
                    break;
                }
                StepOutcome::Busy | StepOutcome::Idle => {}
                other => panic!("Drain failed: {other:?}"),
            }
        }
        assert!(done);
        let stopped = s.engine().last_ens;
        assert_eq!(stopped - h.last_ens, 30_000_000_000);
        let old = published(&mut s, f);
        let prior_last = old.record_end_seq(old.recs.len() - 1).unwrap();
        let mut io = s.into_driver().io;
        let previous_utc = io.rings.utc_of(Tsc(io.rings.tsc));
        io.old = Some(old);
        if spec.recovery_gap {
            // Cold recovery can lose its unpersisted tail yet reuse adjacent seq values.
            io.writer.debug_gap_on_roll(0, true);
        }
        io.rings.tsc = 100;
        io.rings.hz *= 2;
        io.rings.calib.tsc = Tsc(100);
        io.rings.calib.realtime_ns = previous_utc + 600_000_000_000;
        setup.clock.calib_tsc = 100;
        setup.clock.tsc_hz_milli = io.rings.hz;
        setup.clock.calib_realtime_ns = io.rings.calib.realtime_ns;
        let plan = RecoverPlan {
            start: SnapshotSel::DrainTail,
            replay_tail: false,
            epoch: 0,
            flags: RecoveryFlags(0),
            quarantine: Vec::new(),
        };
        s = Sequencer::boot(
            BootSource::Journal(plan),
            setup,
            LiveDriver::new(
                io,
                LiveCfg {
                    prev_last_seq: prior_last,
                    source: 2,
                    ..Default::default()
                },
            ),
            Box::leak(Box::new(Heartbeat::new())),
        )
        .unwrap();
        settle(&mut s);
        assert_eq!(s.engine().last_ens, stopped);
        // Feed a real but unusable path to the real advisor. The raw sample section and
        // its consumed sequence remain untouched; the replayer receives the intact file.
        let mut restored_h = h;
        let mut restored_body = body;
        if spec.reason == 1 {
            // Version zero declares an absent path and cannot prefix nonempty bytes.
            // Use another nonzero version so the file is valid and only its path resets.
            restored_body[..4].copy_from_slice(&3u32.to_le_bytes());
            restored_h.path_version = 3;
        } else if spec.reason == 2 {
            restored_body[8..16].copy_from_slice(&(-1i64).to_le_bytes());
        }
        restored_h.checksum = ckpt_checksum(&restored_body);
        if spec.reason == 3 {
            // Real store bytes no longer match their committed checksum.
            let mut corrupt_file = checkpoint::encode(&restored_h, &restored_body, 1 << 20).unwrap();
            *corrupt_file.last_mut().unwrap() ^= 1;
            assert!(checkpoint::decode(&corrupt_file, 1 << 20).is_err());
            std::fs::write(f.root.join("rejected.ckpt"), corrupt_file).unwrap();
            *restored_body.last_mut().unwrap() ^= 1;
        }
        let mut store = Store::default();
        store.loaded.insert(h.strategy, (restored_h, restored_body));
        drop(a);
        a = advisor(&mut s, f, setup, store);
        let log = published(&mut s, f);
        let note = log.recs.iter().rev().find_map(|r| {
            r.read::<hft_event::Note>().filter(|n| {
                r.kind == EventKind::NOTE
                    && n.code
                        == hft_event::note_code(
                            hft_event::NoteSource::Advisor,
                            hft_sequencer::codes::advisor_note::MODEL_PATH_RESET,
                        )
            })
        });
        if spec.reason == 0 || spec.reason == 3 {
            assert!(note.is_none());
        } else {
            let note = note.unwrap();
            assert_eq!(note.strategy.0, 9);
            assert_eq!(note.v[..3], [spec.reason, h.last_seq, spec.settle_ms]);
        }
        boundaries.push(
            serde_json::json!({ "checkpoint_seq": h.last_seq, "checkpoint_engine_ns": h.last_ens,
            "stop_engine_ns": stopped, "down_ns": 600_000_000_000u64,
            "restart_tsc": 100, "hz_milli": setup.clock.tsc_hz_milli }),
        );
        if spec.recovery_gap {
            market(&mut s, &mut a, f, w, 150, &mut tick);
            let (after_h, after_body) = save(&mut s, &mut a, f);
            std::fs::write(
                f.root.join(format!("after-gap-{turn}.ckpt")),
                checkpoint::encode(&after_h, &after_body, 1 << 20).unwrap(),
            )
            .unwrap();
            market(&mut s, &mut a, f, w, 150, &mut tick);
        } else {
            market(&mut s, &mut a, f, w, 300, &mut tick);
        }
    }
    let (h, body) = save(&mut s, &mut a, f);
    let final_file = checkpoint::encode(&h, &body, 1 << 20).unwrap();
    let d = checkpoint::decode(&final_file, 1 << 20).unwrap();
    let expected_samples = if spec.reason == 3 { 60 } else { tick * 2 };
    assert_eq!(
        u64::from_le_bytes(d.samples.try_into().unwrap()),
        expected_samples
    );
    std::fs::write(f.root.join("final.ckpt"), final_file).unwrap();
    std::fs::write(
        f.root.join("generation.json"),
        serde_json::to_vec_pretty(&serde_json::json!({
            "restarts": boundaries, "path_settle_ms": spec.settle_ms, "reason": spec.reason,
            "recovery_gap": spec.recovery_gap,
            "engine_ns": s.engine().last_ens, "market_ticks": tick, "expected_samples": expected_samples,
        }))
        .unwrap(),
    )
    .unwrap();
    s
}
