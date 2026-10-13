use hft_event::{EventKind, GwCmd, GwOp, OrderEvent, OrderEventKind};
use hft_testkit::dst::seq::matcher::SimMatcher;
use hft_testkit::dst::venue_behavior::{EXTERNAL_HISTORY_CAP, MockVenue, MockVenueCfg};
use hft_testkit::dst::world::{ACCT_A, BTC, P1, S1, VP, VS, WorldSpec};
use hft_testkit::dst::{ConnKind, DstConfig, Fault, InvariantSet, run_record};
use hft_testkit::scenarios::{DstEngine, cancel_filled};
use hft_types::{InstId, Px, Qty, Side, SlotId, Tif, Tsc, VenueId};
use zerocopy::{IntoBytes, TryFromBytes};

const MS: u64 = 1_000_000;

fn order(r: &hft_testkit::dst::sut::InputRec) -> Option<OrderEvent> {
    if r.kind != EventKind::ORDER_EVENT {
        return None;
    }
    OrderEvent::try_read_from_bytes(&r.body).ok()
}

pub(super) fn without_cancel(cfg: &DstConfig) -> DstConfig {
    let mut c = cfg.clone();
    let mut plan = c.fault_plan();
    for (_, f) in &mut plan.items {
        if let Fault::CancelRejectFilled { count, .. } = f {
            *count = 0;
        }
    }
    c.plan = Some(plan);
    c
}

fn held(venue: VenueId, inst: InstId, side: Side, base: bool) -> (MockVenue<SimMatcher>, GwCmd) {
    let mut w = WorldSpec::w1();
    w.accounts[0].balances.push((VS, BTC, 100_000_000));
    w.venues.iter_mut().find(|v| v.id == VS).unwrap().fee_in_base = base;
    let mut cfg = MockVenueCfg::w1(venue);
    cfg.rate_per_s = 0;
    cfg.bbo_ping_ms = 0;
    cfg.latency_ns = (2 * MS, 2 * MS);
    let mut v = MockVenue::new(cfg, &w, 0);
    v.apply_fault(
        &Fault::CancelRejectFilled {
            venue,
            count: 1,
            hold_ms: 5_000,
        },
        0,
    );
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
    let first = v.on_frame(&cmd, MS);
    assert!(
        !first
            .iter()
            .any(|(_, r)| { order(r).is_some_and(|e| e.what == OrderEventKind::Fill) })
    );
    assert_eq!(v.cancel_filled_report().cases.len(), 1);
    assert!(
        v.truth(10 * MS)
            .iter()
            .find(|t| t.account == ACCT_A)
            .unwrap()
            .undelivered
    );
    (
        v,
        GwCmd {
            op: GwOp::Cancel,
            decision_seq: 2,
            target_slot: cmd.slot,
            arg: 0,
            ..cmd
        },
    )
}

fn balances(v: &MockVenue<SimMatcher>, now: u64) -> Vec<hft_testkit::dst::obs::TruthRow> {
    v.truth(now)
        .into_iter()
        .map(|mut t| {
            t.undelivered = false;
            t
        })
        .collect()
}

#[test]
fn tk_170_cancel_race_uses_real_matcher_reject_and_original_cash() {
    for (venue, inst) in [(VS, S1), (VP, P1)] {
        for side in [Side::Buy, Side::Sell] {
            for base in [false, true] {
                let (mut v, cmd) = held(venue, inst, side, base);
                let before = balances(&v, 10 * MS);
                v.apply_fault(&Fault::LateFeeOnly { venue, delay_ms: 500 }, 10 * MS);
                v.apply_fault(&Fault::ReportDup { venue, dur_ms: 3_000 }, 10 * MS);
                v.apply_fault(
                    &Fault::ReportReorder {
                        venue,
                        window: 3,
                        dur_ms: 3_000,
                    },
                    10 * MS,
                );
                let rejects = v.on_frame(&cmd, 20 * MS);
                let x = v.cancel_filled_report().cases.remove(0);
                let reject = x.rejection.expect("actual matching cancel rejection");
                assert_eq!(reject.what, OrderEventKind::CancelReject);
                assert_eq!(reject.detail, 1);
                assert_eq!(reject.cum_filled, x.original.cum_filled);
                assert_eq!(
                    (reject.cash_qty, reject.cash_sum),
                    (x.original.cash_qty, x.original.cash_sum)
                );
                assert!(rejects.iter().any(|(_, r)| r.body == reject.as_bytes()));
                assert!(x.original.fee.0 > 0);
                let rows = v.tick(100 * MS);
                let x = v.cancel_filled_report().cases.remove(0);
                let fill = x.report.unwrap();
                let mut expected = x.original;
                expected.fee = Qty(0);
                expected.fee_asset = hft_types::AssetId(0);
                assert_eq!(fill.as_bytes(), expected.as_bytes());
                let copies: Vec<_> = rows.iter().filter(|(_, r)| r.body == fill.as_bytes()).collect();
                assert_eq!(copies.len(), 2);
                assert!(copies.iter().all(|(t, _)| *t > x.rejection_due_ns.unwrap()));
                let fees: Vec<_> = rows
                    .iter()
                    .filter_map(|(_, r)| order(r))
                    .filter(|e| e.what == OrderEventKind::FeeOnly)
                    .collect();
                assert_eq!(fees.len(), 1);
                assert_eq!(fees[0].fee, x.original.fee);
                assert_eq!(balances(&v, 100 * MS), before);
                assert_eq!(v.cancel_filled_report().awaiting_release, 0);
                assert!(
                    !v.tick(1_000 * MS)
                        .iter()
                        .any(|(_, r)| { order(r).is_some_and(|e| e.what == OrderEventKind::Fill) })
                );
                assert!(v.truth(1_000 * MS).iter().all(|t| !t.undelivered));
            }
        }
    }
}

#[test]
fn tk_171_cancel_race_requires_matching_command_and_bounded_wait() {
    for damage in 0..4 {
        let (mut v, mut cmd) = held(VS, S1, Side::Buy, false);
        match damage {
            0 => cmd.r#gen += 1,
            1 => cmd.target_slot = SlotId(63),
            2 => cmd.inst = P1,
            _ => cmd.op = GwOp::Query,
        }
        v.on_frame(&cmd, 20 * MS);
        assert!(v.cancel_filled_report().cases[0].rejection.is_none());
        let deadline = v.cancel_filled_report().cases[0].deadline_ns;
        v.tick(deadline - 1);
        assert_eq!(v.cancel_filled_report().awaiting_release, 1);
        v.apply_fault(
            &Fault::ConnDown {
                venue: VS,
                conn: ConnKind::UserStream,
                down_ms: 1_000,
                resync: false,
            },
            deadline,
        );
        v.tick(deadline);
        assert_eq!(v.cancel_filled_report().awaiting_release, 1);
        let rows = v.tick(deadline + 1_000 * MS);
        let x = v.cancel_filled_report().cases.remove(0);
        assert_eq!(x.release_ns, Some(deadline + 1_000 * MS));
        assert!(x.rejection.is_none());
        assert!(rows.iter().any(|(_, r)| r.body == x.original.as_bytes()));
        assert_eq!(v.cancel_filled_report().awaiting_release, 0);
    }
    for (count, hold_ms, error) in [
        (0, 0, false),
        (1, 0, true),
        (EXTERNAL_HISTORY_CAP as u32 + 1, 1, true),
    ] {
        let mut v = MockVenue::<SimMatcher>::new(MockVenueCfg::w1(VS), &WorldSpec::w1(), 0);
        v.apply_fault(
            &Fault::CancelRejectFilled {
                venue: VS,
                count,
                hold_ms,
            },
            0,
        );
        assert_eq!(v.fault_error().is_some(), error);
        assert_eq!(v.cancel_filled_report().requested, 0);
    }
    let mut v = MockVenue::<SimMatcher>::new(MockVenueCfg::w1(VS), &WorldSpec::w1(), 0);
    v.clean = true;
    v.apply_fault(
        &Fault::CancelRejectFilled {
            venue: VS,
            count: 1,
            hold_ms: 1,
        },
        0,
    );
    assert_eq!(v.cancel_filled_report().requested, 0);
}

#[test]
fn tk_172_real_cancel_races_keep_wallets_recovery_and_checkers_equal() {
    for restart in [false, true] {
        let mut cfg = cancel_filled::config(0, restart);
        let on = run_record::<DstEngine>(&cfg);
        assert!(
            on.outcome.is_pass(),
            "{}\n{:?}",
            crate::seq_sut::show(&on),
            on.cancel_filled
        );
        assert_eq!(
            on.steps_run, cfg.steps,
            "directed race ended before its step budget"
        );
        let races = cancel_filled::validate(&on).unwrap_or_else(|e| panic!("{e}\n{:?}", on.cancel_filled));
        assert_eq!(races, 2);
        assert!(
            on.cancel_filled
                .iter()
                .flat_map(|v| &v.cases)
                .any(|x| x.command_ns == Some(x.held_ns)),
            "no original fill first captured in its cancel call"
        );
        assert!(on.truth.iter().all(|t| !t.undelivered && t.excluded.is_empty()));
        assert!(on.journal_verification.is_some());
        for damage in 0..5 {
            let mut bad = on.clone();
            let x = &mut bad
                .cancel_filled
                .iter_mut()
                .find(|v| !v.cases.is_empty())
                .unwrap()
                .cases[0];
            match damage {
                0 => x.rejection.as_mut().unwrap().epoch ^= 1,
                1 => x.command.as_mut().unwrap().decision_seq = u64::MAX,
                2 => x.original.cash_sum.0 += 1,
                3 => bad.ledger_checks.clear(),
                _ => {
                    let bytes = x.report.unwrap().as_bytes().to_vec();
                    bad.journal.retain(|j| j.body != bytes);
                }
            }
            assert!(cancel_filled::validate(&bad).is_err(), "damage {damage}");
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
