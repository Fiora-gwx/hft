//! Long, continuous executions: a real advisor and a real SimExchange behind each live venue.
//! Virtual time visits reply deadlines as well as the ten-second market schedule. Every recorded
//! input, output, checkpoint and automatic keyframe is produced by its owning component.

use super::model::{Ring, Store, drain, poll, published};
use super::*;
use hft_event::{GwOp, SendReport, SendStatus};
use hft_sequencer::advisor::{Advisor, AdvisorCfg, checkpoint};
use hft_testkit::dst::Source;
use hft_testkit::dst::seq::SimMatcher;
use hft_testkit::dst::venue_behavior::{Matcher, rep_input};
use hft_types::ConnId;

fn time<S: StrategySet>(s: &mut Sequencer<LiveDriver<S, Io<S>>, S>, t: u64) {
    assert!(t >= s.driver().io.rings.tsc);
    let utc = s.driver().io.utc_of(Tsc(t));
    let io = &mut s.driver_mut().io.rings;
    io.tsc = t;
    io.calib.tsc = Tsc(t);
    io.calib.realtime_ns = utc;
}

/// Deliver same-time gw sends and due exchange reports, and process any resulting commands.
fn pump<S: StrategySet>(
    s: &mut Sequencer<LiveDriver<S, Io<S>>, S>,
    world: &WorldSpec,
    venues: &mut [SimMatcher],
) {
    for _ in 0..1000 {
        let t = s.driver().io.rings.tsc;
        let cmds = core::mem::take(&mut s.driver_mut().io.rings.out);
        let mut active = !cmds.is_empty();
        for (ring, cmd) in cmds {
            let OutRing::Venue(venue) = ring else {
                panic!("fixture must contain only Live accounts")
            };
            let vi = world.venues.iter().position(|v| v.id == venue).unwrap();
            if matches!(
                cmd.op,
                GwOp::Place | GwOp::Cancel | GwOp::Amend | GwOp::CancelAll | GwOp::Query
            ) {
                let sr = SendReport {
                    decision_seq: cmd.decision_seq,
                    sent_tsc: Tsc(t),
                    slot: cmd.slot,
                    r#gen: cmd.r#gen,
                    target_slot: cmd.target_slot,
                    target_gen: 0,
                    venue,
                    account: cmd.account,
                    conn: ConnId(0),
                    status: SendStatus::Written,
                    op: cmd.op,
                    _pad: [0; 3],
                    os_errno: 0,
                    _pad2: 0,
                };
                input(s, (2, vi as u8), EventKind::SEND_REPORT, sr.as_bytes().to_vec());
            }
            venues[vi].on_command(&cmd, t);
        }
        for (vi, venue) in venues.iter_mut().enumerate() {
            let mut reports = Vec::new();
            venue.drain_due(t, &mut reports);
            active |= !reports.is_empty();
            for r in reports {
                assert!(r.due_ns <= t);
                let rec = rep_input(&r.rep, Source::Venue(world.venues[vi].id));
                input(s, (2, vi as u8), rec.kind, rec.body);
            }
        }
        if !active {
            return;
        }
        settle(s);
    }
    panic!("exchange reply loop did not settle");
}

fn advance<S: StrategySet>(
    s: &mut Sequencer<LiveDriver<S, Io<S>>, S>,
    w: &WorldSpec,
    venues: &mut [SimMatcher],
    until: u64,
) {
    for _ in 0..10000 {
        pump(s, w, venues);
        let now = s.driver().io.rings.tsc;
        if now == until {
            return;
        }
        let next = venues
            .iter()
            .filter_map(Matcher::next_due)
            .filter(|t| *t > now)
            .min()
            .unwrap_or(until)
            .min(until);
        time(s, next);
        settle(s);
    }
    panic!("exchange clock did not advance to {until}");
}

pub(super) fn exercise<S: StrategySet>(
    s: &mut Sequencer<LiveDriver<S, Io<S>>, S>,
    f: &Fixture,
    world: &WorldSpec,
    setup: &hft_sequencer::EngineSetup,
    hours: u64,
) {
    let log = published(s, f);
    let mut a = Advisor::start(
        &log,
        setup,
        AdvisorCfg {
            batch: 1,
            checkpoint_ms: 600_000,
            ..Default::default()
        },
        Store::default(),
        Ring::default(),
        Box::leak(Box::new(Heartbeat::new())),
    )
    .unwrap();
    poll(&mut a, &log);
    drain(&mut a, s);
    prepare_smoke(s, world);
    let mut venues: Vec<_> = world
        .venues
        .iter()
        .map(|v| SimMatcher::new(world, v.id, 1))
        .collect();
    pump(s, world, &mut venues);
    for g in &world.groups {
        command(
            s,
            u64::from(g.group.0),
            &ControlCmd::SetMode {
                scope: hft_event::CmdScope {
                    strategy: g.instance,
                    venue: VenueId(0),
                    account: g.account,
                    _pad: 0,
                    group: g.group,
                    inst: InstId(0),
                    _pad2: 0,
                },
                mode: hft_types::RunMode::Live,
            },
        );
        settle(s);
        assert_eq!(
            s.driver().io.writer.last_result.as_ref().unwrap().status,
            hft_sequencer::codes::status::APPLIED
        );
    }
    let mut commits = Vec::new();
    let mut last_ckpt = 0;
    let mut saved_hour = false;
    std::fs::create_dir(f.root.join("checkpoints")).unwrap();
    std::fs::create_dir(f.root.join("ckpt-1h")).unwrap();
    for second in (10..=hours * 3600).step_by(10) {
        let t = second * 1_000_000_000;
        advance(s, world, &mut venues, t);
        let status = s.driver().io.clock_status();
        input(s, (1, 0), EventKind::CLOCK_STATUS, status.as_bytes().to_vec());
        for (vi, venue) in world.venues.iter().enumerate() {
            // The one live account's connection evidence has a 30 s bound in recorded refdata.
            for (role, conn) in [
                (ConnRole::Order, 0),
                (ConnRole::Order, 1),
                (ConnRole::UserStream, 0),
            ] {
                let e = ConnState {
                    venue: venue.id,
                    conn,
                    state: ConnStateKind::Alive,
                    role,
                    inst: InstId(0),
                    reason: 0,
                    account: world.accounts[0].id,
                    _pad2: [0; 3],
                    rx_tsc: t,
                };
                input(s, (2, vi as u8), EventKind::CONN_STATE, e.as_bytes().to_vec());
            }
            for i in world.insts.iter().filter(|i| i.venue == venue.id) {
                let shift = (second / 10 % 41) as i64 * i.tick;
                let mut h = MarketHdr::new_zeroed();
                h.venue = venue.id;
                h.inst = i.id;
                h.rx_tsc = t;
                h.ex_ts_ns = s.driver().io.utc_of(Tsc(t));
                let b = Bbo {
                    h,
                    bid_px: Px(i.bid + shift),
                    bid_qty: Qty(i.depth),
                    ask_px: Px(i.ask + shift),
                    ask_qty: Qty(i.depth),
                    update_id: second / 10,
                };
                venues[vi].on_bbo(i.id, (b.bid_px.0, i.depth), (b.ask_px.0, i.depth), t);
                input(s, (4, vi as u8), EventKind::BBO, b.as_bytes().to_vec());
            }
        }
        settle(s);
        pump(s, world, &mut venues);
        let log = published(s, f);
        poll(&mut a, &log);
        drain(&mut a, s);
        // Drain each callback's Advice (batch=1) so no model output is hidden by an outbox merge.
        for (h, body) in a.ckpt.committed.values() {
            if h.last_seq <= last_ckpt {
                continue;
            }
            assert_eq!(h.path_version, 1);
            assert_eq!(h.layout_version, 1);
            let bytes = checkpoint::encode(h, body, 1 << 20).unwrap();
            assert_eq!(checkpoint::decode(&bytes, 1 << 20).unwrap().path.len(), 120 * 16);
            let name = s.engine().rd.instance(h.strategy).unwrap().name.as_ref();
            assert_eq!(name, "tsa");
            std::fs::write(f.root.join("checkpoints").join(format!("{name}.ckpt")), &bytes).unwrap();
            if !saved_hour && h.last_ens > 3_600_000_000_000 {
                std::fs::write(f.root.join("ckpt-1h").join(format!("{name}.ckpt")), &bytes).unwrap();
                saved_hour = true;
            }
            commits.push((h.last_seq, h.last_ens));
            last_ckpt = h.last_seq;
        }
    }
    // Catch up over the final recorded Advice before ending the actual file run.
    let log = published(s, f);
    poll(&mut a, &log);
    drain(&mut a, s);
    assert_eq!(s.engine().last_ens, (hours * 3600 * 1_000_000_000) as i64);
    assert!(
        commits.len() >= (hours * 6 - 1) as usize,
        "checkpoints: {commits:?}"
    );
    assert_eq!(saved_hour, hours > 1);
    std::fs::write(f.root.join("generation.json"), serde_json::to_vec_pretty(&serde_json::json!({
        "hours": hours, "engine_ns": s.engine().last_ens, "checkpoint_ms": 600000,
        "keyframe_period_ms": setup.seq.keyframe_period_ms, "keyframe_max_events": setup.seq.keyframe_max_events,
        "checkpoints": commits, "instance": "prod", "account": "acct-l", "strategy": "tsa",
        "layout_version": 1, "path_version": 1, "path_settle_ms": 120000,
        "venue_aliases": {"v-spot": world.venues[0].id.0, "v-perp": world.venues[1].id.0}
    })).unwrap()).unwrap();
}
