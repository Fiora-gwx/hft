//! Cross-component Drain acceptance on the real sequencer/OMS/exec fixture.
#[path = "../../../crates/hft-sequencer/tests/common/mod.rs"]
mod common;
use common::{risk_kit::*, *};
use hft_event::{ControlCmd, EventKind, OrderEventKind, SysTimer, TimerCode};
use hft_sequencer::{StepOutcome, driver::live::RingId};
use hft_types::{AccountId, Tif};

fn ready() -> H {
    reset_script();
    let mut config = b0();
    config.setup_edit.push(|e| e.drain_timeout_ms = 8000);
    let mut h = config.boot_self_contained();
    h.steady();
    h
}
fn drain(h: &mut H) -> StepOutcome {
    h.cmd_with(
        "cli:systemd",
        ADMIN,
        AccountId(0),
        &ControlCmd::Drain { cancel_resting: true },
        |hdr| hdr.confirmed = 1,
    );
    h.run()
}
fn drain_snap(h: &H) -> Option<u64> {
    h.j()
        .records()
        .filter(|r| r.kind == EventKind::STATE_SNAPSHOT)
        .find(|r| snap(r).0.purpose == hft_sequencer::snapshot::PURPOSE_DRAIN)
        .map(|r| r.seq)
}

#[test]
fn core_303_drain_waits_leg_link() {
    let mut h = ready();
    let mark = place_with(&mut h, S1, |hx| pair_first(hx, q("0.001")));
    let first = decs_since(&h, mark).into_iter().find(|d| d.inst == A1).unwrap();
    assert_eq!(first.reject, 0);
    h.ack(&first);
    let before_fill = h.keyframe().seq;
    h.adv(100 * MS);
    assert!(!matches!(drain(&mut h), StepOutcome::Done { .. }));
    assert!(drain_snap(&h).is_none());
    h.adv(400 * MS);
    h.fill(&first, first.qty, first.qty, first.px);
    let second = decs_since(&h, mark)
        .into_iter()
        .find(|d| d.inst == B1 && d.reject == 0)
        .unwrap();
    assert!(drain_snap(&h).is_none());
    h.ack(&second);
    h.fill(&second, second.qty, second.qty, second.px);
    assert!(matches!(
        h.run(),
        StepOutcome::Done {
            cause: hft_sequencer::DoneCause::Drain,
            ..
        }
    ));
    let snapshot = drain_snap(&h).unwrap();
    assert!(
        h.j()
            .records()
            .any(|r| r.kind == EventKind::ORDER_DECISION && r.seq > before_fill && r.seq < snapshot)
    );
    assert_eq!(
        h.s.engine().oms.quiescent(AC1) & hft_oms::quiesce::LINKS_ACTIVE,
        0
    );
    let mismatches = h.verify_from(Some(before_fill));
    assert!(mismatches.is_empty(), "{mismatches:?}");
}

#[test]
fn core_304_drain_timeout() {
    for (storage, ack_ms, exit) in [(false, None, 0), (true, None, 3), (false, Some(7000), 0)] {
        let mut h = ready();
        let mark = place_with(&mut h, S1, |hx| {
            let mut order = pair_first(hx, q("0.001"));
            order.tif = Tif::Gtc;
            order.px = px("49000");
            order
        });
        let order = decs_since(&h, mark)[0];
        assert_eq!(order.reject, 0);
        h.ack(&order);
        let replay = h.keyframe().seq;
        let started = h.now();
        let output = if storage {
            let (_, timer, _) = TimerCode::Sys(SysTimer::DiskLow { level: 2 }).encode().unwrap();
            h.push(RingId::Hk, &sys_timer(timer, h.now()));
            h.run()
        } else {
            drain(&mut h)
        };
        assert!(!matches!(output, StepOutcome::Done { .. }));
        h.adv(ack_ms.unwrap_or(7999) * MS);
        if ack_ms.is_some() {
            let event = order_event(&order, OrderEventKind::CancelAck, h.epoch());
            h.push(h.rep_ring(AC1), &event);
        } else {
            h.push(RingId::Hk, &sys_timer(8, h.now()));
            assert!(!matches!(h.run(), StepOutcome::Done { .. }));
            h.adv(MS);
            h.push(RingId::Hk, &sys_timer(8, h.now()));
        }
        let StepOutcome::Done { cause, .. } = h.run() else {
            panic!("Drain did not finish")
        };
        assert_eq!(hft_core::exit_code(cause), exit);
        let timeouts: Vec<_> = h
            .notes_since(mark)
            .into_iter()
            .filter(|n| {
                n.source == hft_event::NoteSource::Sequencer
                    && n.code & 0xffff == hft_sequencer::codes::note::DRAIN_TIMEOUT
            })
            .collect();
        assert_eq!(timeouts.len(), usize::from(ack_ms.is_none()));
        if ack_ms.is_none() {
            assert!(h.now() - started >= 8000 * MS);
        }
        assert!(drain_snap(&h).is_some());
        let mismatches = h.verify_from(Some(replay));
        assert!(mismatches.is_empty(), "{mismatches:?}");
    }
}
