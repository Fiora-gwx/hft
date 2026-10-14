//! Real receipt copies must wait for a committed matching tomb, including after recovery.
use hft_event::{EventKind, GwCmd, GwOp, OrderEvent, OrderEventKind, RecordFlags};
use hft_testkit::dst::obs::{Obs, SlotRow, SlotState, TombRow};
use hft_testkit::dst::seq::matcher::SimMatcher;
use hft_testkit::dst::sut::{InputRec, JRec};
use hft_testkit::dst::venue_behavior::{EXTERNAL_HISTORY_CAP, Matcher, MockVenue, MockVenueCfg};
use hft_testkit::dst::world::{ACCT_A, BTC, P1, S1, VP, VS, WorldSpec};
use hft_testkit::dst::{ConnKind, Fault, InvariantSet, RerunPolicy, RunRecord, run_record};
use hft_testkit::scenarios::{DstEngine, foreign_ledger, suites};
use hft_types::{GroupId, InstId, Px, Qty, Side, SlotId, Tif, Tsc, VenueId};
use zerocopy::{IntoBytes, TryFromBytes};

const MS: u64 = 1_000_000;
const SECOND: u64 = 1_000 * MS;

fn orders(rows: &[(u64, InputRec)]) -> Vec<(u64, OrderEvent)> {
    rows.iter()
        .filter(|(_, i)| i.kind == EventKind::ORDER_EVENT)
        .map(|(at, i)| (*at, OrderEvent::try_read_from_bytes(&i.body).unwrap()))
        .collect()
}

fn filled(
    venue: VenueId,
    inst: InstId,
    side: Side,
    base_fee: bool,
) -> (MockVenue<SimMatcher>, OrderEvent, Obs, Vec<JRec>) {
    let mut world = WorldSpec::w1();
    world.accounts[0].balances.push((VS, BTC, 100_000_000));
    world.venues.iter_mut().find(|v| v.id == VS).unwrap().fee_in_base = base_fee;
    let mut cfg = MockVenueCfg::w1(venue);
    cfg.latency_ns = (2 * MS, 2 * MS);
    cfg.bbo_ping_ms = 0;
    cfg.rate_per_s = 0;
    let mut v = MockVenue::new(cfg, &world, 0);
    v.apply_fault(&Fault::LateFillAfterRecycle { venue, count: 1 }, 0);
    // The saved matcher original includes this fee; the transport copy must retain the
    // fee-free first delivery and must never split/replay the FeeOnly delta again.
    v.apply_fault(&Fault::LateFeeOnly { venue, delay_ms: 500 }, 0);
    let cmd = GwCmd {
        decision_seq: 1,
        decided_tsc: Tsc(0),
        px: Px(if side == Side::Buy {
            700_000_000
        } else {
            500_000_000
        }),
        qty: Qty(100_000),
        inst,
        slot: SlotId(1),
        r#gen: 3,
        target_slot: SlotId(0),
        op: GwOp::Place,
        account: ACCT_A,
        side,
        tif: Tif::Ioc,
        oflags: 0,
        _pad: [0; 2],
        arg: 7,
        arg2: 0,
    };
    let rows = v.on_frame(&cmd, MS);
    let e = orders(&rows)
        .into_iter()
        .find(|(_, e)| e.what == OrderEventKind::Fill)
        .unwrap()
        .1;
    let mut obs = Obs::default();
    obs.tombs.push(TombRow {
        venue,
        account: ACCT_A,
        slot: e.slot.0,
        r#gen: e.r#gen,
        epoch: e.epoch,
        inst,
        group: GroupId(1),
        filled: i128::from(e.cum_filled.0),
    });
    // A different generation now occupies the same slot.
    obs.slots.push(SlotRow {
        venue,
        account: ACCT_A,
        slot: e.slot.0,
        r#gen: e.r#gen + 1,
        epoch: e.epoch,
        state: SlotState::Live,
        group: GroupId(1),
        inst,
        side,
        qty: 100_000,
        filled: 0,
        immediate: false,
        risk_reducing: false,
        terminal_ens: 0,
    });
    let journal = vec![
        JRec {
            seq: 10,
            end_seq: 10,
            tsc: 4 * MS,
            kind: EventKind::ORDER_EVENT,
            flags: RecordFlags::EVENT_END,
            body: e.as_bytes().to_vec(),
        },
        JRec {
            seq: 20,
            end_seq: 20,
            tsc: SECOND,
            kind: EventKind::NOTE,
            flags: RecordFlags::EVENT_END,
            body: vec![],
        },
    ];
    (v, e, obs, journal)
}

#[test]
fn tk_166_recycled_fill_copies_keep_original_facts_and_transport_faults() {
    for (venue, inst) in [(VS, S1), (VP, P1)] {
        for side in [Side::Buy, Side::Sell] {
            for base in [false, true] {
                let (mut v, e, obs, journal) = filled(venue, inst, side, base);
                v.tick(SECOND);
                let before = v.m.truth(ACCT_A);
                for fault in [
                    Fault::GrayConn {
                        venue,
                        dur_ms: 1000,
                        factor: 100,
                    },
                    Fault::ReportDup { venue, dur_ms: 1000 },
                    Fault::NoClientIdFill {
                        venue,
                        within_hold: true,
                    },
                ] {
                    v.apply_fault(&fault, SECOND);
                }
                assert!(v.observe_recycled(&obs, &journal, SECOND));
                assert!(!v.observe_recycled(&obs, &journal, SECOND));
                let rows = orders(&v.tick(SECOND));
                assert_eq!(rows.len(), 3);
                assert!(
                    rows.iter()
                        .all(|(at, copy)| *at > SECOND && copy.as_bytes() == e.as_bytes())
                );
                let report = v.late_recycle_report();
                assert_eq!(
                    (report.requested, report.awaiting_fill, report.awaiting_recycle),
                    (1, 0, 0)
                );
                assert_eq!(report.copies.len(), 1);
                let copy = &report.copies[0];
                assert_eq!((copy.original_seq, copy.recycle_seq), (10, 20));
                assert_ne!(copy.original.fee.0, 0);
                assert_eq!(copy.report.fee.0, 0);
                assert_eq!(copy.delivered_ns, rows.iter().map(|(at, _)| *at).max().unwrap());
                v.tick(copy.delivered_ns + MS);
                let after = v.m.truth(ACCT_A);
                assert_eq!(before.positions, after.positions);
                assert_eq!(before.inventory, after.inventory);
                assert_eq!(before.balances, after.balances);
                assert!(v.m.external_history().is_empty());
            }
        }
    }
}

#[test]
fn tk_167_recycled_copies_require_full_committed_identity_and_bounded_requests() {
    let (mut v, _, obs, journal) = filled(VS, S1, Side::Buy, false);
    for damage in 0..10 {
        let mut bad = obs.clone();
        let mut log = journal.clone();
        match damage {
            0 => bad.tombs[0].epoch ^= 1,
            1 => bad.tombs[0].r#gen ^= 1,
            2 => bad.tombs[0].account.0 += 1,
            3 => bad.tombs[0].venue.0 += 1,
            4 => bad.tombs[0].slot += 1,
            5 => bad.tombs[0].inst.0 += 1,
            6 => bad.tombs[0].filled -= 1,
            7 => {
                log.clear();
            }
            8 => {
                for r in &mut log {
                    r.flags = RecordFlags(0);
                }
            }
            _ => bad.slots[0].r#gen -= 1,
        }
        assert!(!v.observe_recycled(&bad, &log, SECOND), "damage {damage}");
    }
    v.apply_fault(
        &Fault::ConnDown {
            venue: VS,
            conn: ConnKind::UserStream,
            down_ms: 1000,
            resync: false,
        },
        SECOND,
    );
    assert!(!v.observe_recycled(&obs, &journal, SECOND));
    assert!(v.observe_recycled(&obs, &journal, 2 * SECOND));
    let (mut overflow, _, obs, log) = filled(VS, S1, Side::Buy, false);
    assert!(!overflow.observe_recycled(&obs, &log, u64::MAX));
    assert!(overflow.fault_error().unwrap().contains("overflow"));
    let (mut cap, _, _, _) = filled(VS, S1, Side::Buy, false);
    cap.apply_fault(
        &Fault::LateFillAfterRecycle {
            venue: VS,
            count: EXTERNAL_HISTORY_CAP as u32,
        },
        SECOND,
    );
    assert!(cap.fault_error().unwrap().contains("capacity"));
    for clean in [false, true] {
        let mut v = MockVenue::<SimMatcher>::new(MockVenueCfg::w1(VS), &WorldSpec::w1(), 0);
        v.clean = clean;
        v.apply_fault(
            &Fault::LateFillAfterRecycle {
                venue: VS,
                count: if clean { 1 } else { 0 },
            },
            0,
        );
        assert_eq!(v.late_recycle_report().requested, 0);
    }
}

pub(super) fn validate(rec: &RunRecord) -> Result<usize, String> {
    let mut count = 0;
    for v in &rec.late_recycle {
        for copy in &v.copies {
            let e = copy.report;
            if (
                copy.tomb.venue,
                copy.tomb.account,
                copy.tomb.epoch,
                copy.tomb.slot,
                copy.tomb.r#gen,
                copy.tomb.inst,
            ) != (v.venue, e.account, e.epoch, e.slot.0, e.r#gen, e.inst)
                || copy.tomb.filled < i128::from(e.cum_filled.0)
            {
                return Err("tomb identity".into());
            }
            if !rec
                .journal
                .iter()
                .any(|j| j.seq == copy.original_seq && j.body == e.as_bytes())
            {
                return Err("missing original delivery".into());
            }
            let Some(late) = rec.journal.iter().find(|j| {
                j.seq > copy.recycle_seq && j.kind == EventKind::ORDER_EVENT && j.body == e.as_bytes()
            }) else {
                return Err("missing late delivery".into());
            };
            if copy.original_seq >= copy.recycle_seq || late.tsc <= copy.recycle_ns {
                return Err("recycle boundary".into());
            }
            if !rec.ledger_checks.contains(&late.seq) {
                return Err("missing hot/cold comparison".into());
            }
            count += 1;
        }
    }
    if count == 0 {
        return Err("no actual late copies".into());
    }
    Ok(count)
}

#[test]
fn tk_168_real_recycled_copies_keep_wallets_recovery_and_checkers_equal() {
    for restart in [false, true] {
        let mut cfg = foreign_ledger::led_080_foreign_order_cold_hot_equal()
            .config(&suites(), 0)
            .unwrap();
        cfg.rerun = RerunPolicy::Never;
        cfg.scripted = vec![
            (0, Fault::LateFillAfterRecycle { venue: VS, count: 1 }),
            (0, Fault::LateFillAfterRecycle { venue: VP, count: 1 }),
        ];
        if restart {
            cfg.scripted.push((
                0,
                Fault::Crash(hft_testkit::dst::CrashFault {
                    at_step: 150,
                    repeat: 2,
                    point: hft_testkit::dst::CrashPoint::SeqInput,
                }),
            ));
        }
        let on = run_record::<DstEngine>(&cfg);
        assert!(on.outcome.is_pass(), "{}", crate::seq_sut::show(&on));
        assert_eq!(validate(&on).unwrap(), 2);
        assert!(on.journal_verification.is_some());
        assert!(on.truth.iter().all(|t| !t.undelivered && t.excluded.is_empty()));
        for damage in 0..4 {
            let mut bad = on.clone();
            let v = bad
                .late_recycle
                .iter_mut()
                .find(|v| !v.copies.is_empty())
                .unwrap();
            match damage {
                0 => v.copies[0].tomb.epoch ^= 1,
                1 => v.copies[0].original_seq = u64::MAX,
                2 => v.copies[0].recycle_seq = u64::MAX,
                _ => bad.ledger_checks.clear(),
            }
            assert!(validate(&bad).is_err(), "damage {damage}");
        }
        cfg.invariants = InvariantSet::none();
        let off = run_record::<DstEngine>(&cfg);
        assert!(off.outcome.is_pass());
        assert!(on.journal == off.journal);
        assert_eq!(on.frames, off.frames);
        assert_eq!(on.deliveries, off.deliveries);
        assert_eq!(on.trail.to_bytes(), off.trail.to_bytes());
        assert_eq!(on.truth, off.truth);
    }
}
