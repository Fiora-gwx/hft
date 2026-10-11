//! Real runtime inputs for delayed Model creation, chunked warmup and a full Advice ring.
use super::model::{Ring, Store, drain, poll, published};
use super::*;
use hft_event::{AdviceHdr, BlobHdr, CmdScope};
use hft_sequencer::advisor::{AdviceRing, Advisor, AdvisorCfg, checkpoint};

type Live<S> = Sequencer<LiveDriver<S, Io<S>>, S>;
type Adv<S> = Advisor<S, Store, Ring>;

fn time<S: StrategySet>(s: &mut Live<S>, t: u64) {
    let utc = s.driver().io.utc_of(Tsc(t));
    let io = &mut s.driver_mut().io.rings;
    assert!(t >= io.tsc);
    io.tsc = t;
    io.calib.tsc = Tsc(t);
    io.calib.realtime_ns = utc;
    let status = s.driver().io.clock_status();
    input(s, (1, 0), EventKind::CLOCK_STATUS, status.as_bytes().to_vec());
}

fn market<S: StrategySet>(s: &mut Live<S>, w: &WorldSpec, t: u64, both: bool) {
    time(s, t);
    for (vi, venue) in w.venues.iter().enumerate() {
        for i in w.insts.iter().filter(|i| i.venue == venue.id) {
            if !both && i.id != w.insts[0].id {
                continue;
            }
            let mut h = MarketHdr::new_zeroed();
            h.venue = i.venue;
            h.inst = i.id;
            h.rx_tsc = t;
            h.ex_ts_ns = s.driver().io.utc_of(Tsc(t));
            let b = Bbo {
                h,
                bid_px: Px(i.bid + (t % 41) as i64 * i.tick),
                ask_px: Px(i.ask + (t % 41) as i64 * i.tick),
                bid_qty: Qty(i.depth),
                ask_qty: Qty(i.depth),
                update_id: t,
            };
            input(s, (4, vi as u8), EventKind::BBO, b.as_bytes().to_vec());
        }
    }
    settle(s);
}

fn catchup<S: StrategySet>(s: &mut Live<S>, a: &mut Adv<S>, f: &Fixture) {
    let log = published(s, f);
    poll(a, &log);
    drain(a, s);
}

fn save<S: StrategySet>(s: &mut Live<S>, a: &mut Adv<S>, f: &Fixture, path: &Path) {
    let log = published(s, f);
    poll(a, &log);
    a.begin_checkpoints();
    poll(a, &log);
    let (h, body) = a.ckpt.committed.values().next().unwrap();
    std::fs::write(path, checkpoint::encode(h, body, 1 << 20).unwrap()).unwrap();
}

pub(super) fn exercise<S: StrategySet>(
    s: &mut Live<S>,
    f: &Fixture,
    w: &WorldSpec,
    setup: &hft_sequencer::EngineSetup,
    mode: ModelBoundary,
    pending: Option<(Vec<u8>, hft_sequencer::ParsedParams<S>)>,
) {
    let log = published(s, f);
    let mut a = Advisor::start(
        &log,
        setup,
        AdvisorCfg {
            batch: 1,
            checkpoint_ms: 0,
            max_lag_ms: 600_000,
            ..Default::default()
        },
        Store::default(),
        Ring::default(),
        Box::leak(Box::new(Heartbeat::new())),
    )
    .unwrap();
    poll(&mut a, &log);
    drain(&mut a, s);
    match mode {
        ModelBoundary::Created => {
            assert!(s.instances().get(w.strategies[0].instance).unwrap().cur.is_none());
            for second in (10..=1200).step_by(10) {
                market(s, w, second * 1_000_000_000, true);
                catchup(s, &mut a, f);
            }
            assert!(!published(s, f).recs.iter().any(|r| r.kind == EventKind::ADVICE));
            let (bytes, p) = pending.unwrap();
            let blob = build::spb_ref(&bytes, p.version);
            let scope = CmdScope {
                strategy: p.strategy,
                account: p.account,
                venue: VenueId(0),
                group: GroupId(0),
                inst: InstId(0),
                _pad: 0,
                _pad2: 0,
            };
            s.driver_mut()
                .io
                .rings
                .mailbox
                .push(ParsedConfig::StrategyParams(Box::new(p)));
            let h = BlobHdr {
                blob,
                scope,
                boot: 0,
                _pad: [0; 7],
            };
            let mut body = h.as_bytes().to_vec();
            body.extend_from_slice(&bytes);
            input(s, (0, 0), EventKind::CONFIG_BLOB, body);
            command(s, 201, &ControlCmd::SetParams { scope, blob });
            settle(s);
            assert_eq!(
                s.driver().io.writer.last_result.as_ref().unwrap().status,
                hft_sequencer::codes::status::APPLIED
            );
            catchup(s, &mut a, f);
            for second in (1210..=3600).step_by(10) {
                market(s, w, second * 1_000_000_000, true);
                catchup(s, &mut a, f);
            }
            assert_eq!(s.engine().last_ens, 3_600_000_000_000);
        }
        ModelBoundary::Warmup => {
            market(s, w, 1_000_000_000, false);
            catchup(s, &mut a, f);
            for (no, value) in [11u64, 23, 47].into_iter().enumerate() {
                time(s, (no as u64 + 2) * 1_000_000_000);
                command(
                    s,
                    300 + no as u64,
                    &ControlCmd::InjectWarmup {
                        instance: w.strategies[0].instance,
                        local_kind: 9,
                        chunk_no: no as u32,
                        last: u8::from(no == 2),
                        len: 8,
                        _pad: [0; 6],
                        payload: &value.to_le_bytes(),
                    },
                );
                settle(s);
                assert_eq!(
                    s.driver().io.writer.last_result.as_ref().unwrap().status,
                    hft_sequencer::codes::status::APPLIED
                );
                catchup(s, &mut a, f);
                if no == 1 {
                    std::fs::create_dir(f.root.join("checkpoints")).unwrap();
                    save(s, &mut a, f, &f.root.join("checkpoints/tsa.ckpt"));
                }
            }
            market(s, w, 5_000_000_000, true);
            catchup(s, &mut a, f);
        }
        ModelBoundary::Coalesced => {
            a.ring.1 = size_of::<AdviceHdr>() + 32;
            for no in 0..39u64 {
                market(s, w, 1 + no * 10_000_000_000 / 38, false);
                let log = published(s, f);
                poll(&mut a, &log);
                assert_eq!(a.ring.free(), 0, "ring was not full during the stall");
                assert_eq!(a.ring.0.len(), 1);
            }
            assert_eq!(a.advice_coalesced, 37, "real outbox replacements");
            a.ring.1 = 1 << 24;
            drain(&mut a, s);
            catchup(s, &mut a, f);
        }
    }
    command(s, 900, &ControlCmd::SnapshotNow);
    settle(s);
    catchup(s, &mut a, f);
    save(s, &mut a, f, &f.root.join("final.ckpt"));
    std::fs::write(
        f.root.join("generation.json"),
        serde_json::to_vec_pretty(&serde_json::json!({
            "mode": format!("{mode:?}"), "engine_ns": s.engine().last_ens,
            "advice_coalesced": a.advice_coalesced,
        }))
        .unwrap(),
    )
    .unwrap();
}
