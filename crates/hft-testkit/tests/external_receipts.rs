//! Original exchange receipts and transport-only ADL replay.
use hft_event::{ExternalCause, ExternalFill};
use hft_testkit::dst::faults::{ConnKind, Fault};
use hft_testkit::dst::seq::matcher::SimMatcher;
use hft_testkit::dst::sut::{InputRec, Source};
use hft_testkit::dst::venue_behavior::{EXTERNAL_HISTORY_CAP, Matcher, MockVenue, MockVenueCfg};
use hft_testkit::dst::world::{ACCT_A, ACCT_B, P1, USDT, VP, WorldSpec};
use hft_types::{Qty, Side};
use zerocopy::{IntoBytes, TryFromBytes};

const MS: u64 = 1_000_000;

fn venue() -> MockVenue<SimMatcher> {
    let mut cfg = MockVenueCfg::w1(VP);
    cfg.bbo_ping_ms = 0;
    cfg.latency_ns = (MS, MS);
    let mut v = MockVenue::new(cfg, &WorldSpec::w1(), 1);
    v.tick(0);
    v
}

fn seed(v: &mut MockVenue<SimMatcher>, side: Side) -> ExternalFill {
    v.m.on_bbo(P1, (600_189_000, 1_000_000), (600_190_000, 1_000_000), 10 * MS);
    let fill = v.m.external(ACCT_A, P1, side, 200_000, "seed", false, 10 * MS);
    v.m.on_bbo(P1, (600_275_000, 1_000_000), (600_276_000, 1_000_000), 20 * MS);
    fill
}

fn down(v: &mut MockVenue<SimMatcher>, now: u64, down_ms: u32) {
    v.apply_fault(
        &Fault::ConnDown {
            venue: VP,
            conn: ConnKind::UserStream,
            down_ms,
            resync: false,
        },
        now,
    );
}

fn fills(rows: &[(u64, InputRec)]) -> Vec<(u64, ExternalFill)> {
    rows.iter()
        .filter_map(|(at, i)| {
            if i.kind != hft_event::EventKind::EXTERNAL_FILL {
                return None;
            }
            ExternalFill::try_read_from_bytes(&i.body).ok().map(|x| (*at, x))
        })
        .collect()
}

#[test]
fn tk_130_external_history_keeps_original_cash_and_fails_before_overflow() {
    for side in [Side::Buy, Side::Sell] {
        let mut v = venue();
        let open = seed(&mut v, side);
        let close = v.m.external(
            ACCT_A,
            P1,
            side.opposite(),
            100_000,
            "adl-original",
            false,
            20 * MS,
        );
        let cash = if side == Side::Buy { 8_600 } else { -8_600 };
        assert_eq!((close.cash_qty, close.cash_amount), (Qty(100_000), Qty(cash)));
        assert_eq!((open.trade_id, close.trade_id), (1, 2));
        let before = v.m.state_hash();
        for _ in 0..3 {
            let h = v.m.external_history();
            assert_eq!(h.len(), 2);
            assert_eq!(h[0].fill.as_bytes(), open.as_bytes());
            assert_eq!(h[1].fill.as_bytes(), close.as_bytes());
            assert_eq!(h[1].client_id, "adl-original");
            assert_eq!(v.m.state_hash(), before);
        }
        assert!(
            fills(&v.tick(21 * MS)).is_empty(),
            "history reads cannot cause a push"
        );
        v.m.external(ACCT_B, P1, side, 1, "other-account", true, 22 * MS);
        assert_eq!(v.m.external_history()[2].fill.account, ACCT_B);
        assert_eq!(v.m.external_history()[2].fill.trade_id, 3);
        let mut copied = v.m.external_history()[1].clone();
        copied.fill.cash_amount.0 += 1;
        assert_ne!(copied.fill.cash_amount, close.cash_amount);
        assert_eq!(v.m.external_history()[1].fill.as_bytes(), close.as_bytes());
    }
    let mut v = venue();
    for n in 0..EXTERNAL_HISTORY_CAP {
        let x =
            v.m.external(ACCT_A, P1, Side::Buy, 1, "capacity", false, n as u64);
        assert_eq!(x.trade_id, n as u64 + 1);
    }
    let before = v.m.state_hash();
    let last = v.m.external_history().last().unwrap().fill;
    let rejected = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        v.m.external(ACCT_A, P1, Side::Buy, 1, "overflow", true, 10 * MS);
    }));
    assert!(rejected.is_err());
    assert_eq!(v.m.state_hash(), before);
    assert_eq!(v.m.external_history().len(), EXTERNAL_HISTORY_CAP);
    assert_eq!(
        v.m.external_history().last().unwrap().fill.as_bytes(),
        last.as_bytes()
    );
    assert!(fills(&v.tick(11 * MS)).is_empty());
}

#[test]
fn tk_131_adl_replay_waits_for_stream_and_never_settles_twice() {
    for side in [Side::Buy, Side::Sell] {
        for gap in [false, true] {
            for replay in [false, true] {
                for disconnected in [false, true] {
                    let mut v = venue();
                    seed(&mut v, side);
                    if disconnected {
                        down(&mut v, 19 * MS, 81);
                    }
                    let mut rows = v.apply_fault(
                        &Fault::Adl {
                            account: ACCT_A,
                            inst: P1,
                            gap,
                            replay,
                        },
                        20 * MS,
                    );
                    let mut original = v.m.external_history().last().unwrap().fill;
                    original.cause = ExternalCause::Adl;
                    let cash = if side == Side::Buy { 8_600 } else { -8_600 };
                    assert_eq!(original.cash_amount, Qty(cash));
                    let before = v.m.state_hash();
                    let at = if disconnected { 100 * MS } else { 21 * MS };
                    assert!(fills(&v.tick(at - 1)).is_empty());
                    if replay {
                        assert!(
                            v.truth(at - 1)
                                .iter()
                                .find(|t| t.account == ACCT_A)
                                .unwrap()
                                .undelivered
                        );
                        assert_eq!(v.next_wake(at - 1), at);
                    }
                    rows.extend(v.tick(at));
                    let reports = fills(&rows);
                    assert_eq!(reports.len(), usize::from(!gap) + usize::from(replay));
                    for (_, x) in reports {
                        assert_eq!(x.as_bytes(), original.as_bytes());
                    }
                    assert_eq!(
                        v.m.state_hash(),
                        before,
                        "delivery cannot settle another execution"
                    );
                    assert_eq!(v.m.external_history().len(), 2);
                    assert!(fills(&v.tick(at + MS)).is_empty());
                    let truth = v.truth(at + MS);
                    let t = truth.iter().find(|t| t.account == ACCT_A).unwrap();
                    assert!(!t.undelivered);
                    assert_eq!(
                        t.positions,
                        vec![(P1, if side == Side::Buy { 100_000 } else { -100_000 })]
                    );
                    assert_eq!(t.balances, vec![(USDT, 50_000_000_000 + i128::from(cash))]);
                    assert_eq!(t.excluded, if gap && !replay { vec![P1.0] } else { vec![] });
                }
            }
        }
    }
    // A second disconnect can extend the wait. Independent receipts must not collapse to one.
    let mut v = venue();
    seed(&mut v, Side::Sell);
    down(&mut v, 19 * MS, 81);
    for now in [20, 30] {
        v.apply_fault(
            &Fault::Adl {
                account: ACCT_A,
                inst: P1,
                gap: true,
                replay: true,
            },
            now * MS,
        );
    }
    down(&mut v, 40 * MS, 160);
    assert!(fills(&v.tick(100 * MS)).is_empty());
    assert_eq!(v.next_wake(100 * MS), 200 * MS);
    let before = v.m.state_hash();
    let reports = fills(&v.tick(200 * MS));
    assert_eq!(
        reports.iter().map(|(_, e)| e.trade_id).collect::<Vec<_>>(),
        vec![2, 3]
    );
    assert_eq!(
        reports.iter().map(|(_, e)| e.cash_amount.0).collect::<Vec<_>>(),
        vec![-8_600, -4_300]
    );
    assert_eq!(v.m.state_hash(), before);
    assert!(fills(&v.tick(201 * MS)).is_empty());
}

#[test]
fn tk_132_replayed_adl_keeps_real_engine_wallet_after_recovery() {
    use hft_testkit::dst::SeqSut;
    use hft_testkit::dst::sut::{BootCtx, CrashPoint, StepOut, Sut};
    use hft_testkit::strategies::TestStrategies;
    for side in [Side::Buy, Side::Sell] {
        for restart_before_delivery in [false, true] {
            let world = WorldSpec::w1();
            let mut e = SeqSut::<TestStrategies>::boot(&BootCtx {
                seed: 1,
                world: &world,
                planted: &[],
                tsc_hz: 1_000_000_000,
            })
            .unwrap();
            let mut v = venue();
            let opening = seed(&mut v, side);
            let mut out = StepOut::default();
            e.process(
                10 * MS,
                10 * MS,
                &InputRec::of(&opening, Source::Venue(VP)),
                &mut out,
            );
            assert_eq!(e.rebuilt_hashes(), Ok(Some(e.hashes())));
            down(&mut v, 19 * MS, 81);
            v.apply_fault(
                &Fault::Adl {
                    account: ACCT_A,
                    inst: P1,
                    gap: true,
                    replay: true,
                },
                20 * MS,
            );
            if restart_before_delivery {
                e.crash_and_recover(CrashPoint::SeqInput, None).unwrap();
            }
            let reports = fills(&v.tick(100 * MS));
            assert_eq!(reports.len(), 1);
            let original = reports[0].1;
            for (n, now) in [100 * MS, 101 * MS, 102 * MS].into_iter().enumerate() {
                out.clear();
                e.process(now, now, &InputRec::of(&original, Source::Venue(VP)), &mut out);
                let ledger = &e.sequencer().engine().ledger;
                let cash = if side == Side::Buy { 8_600 } else { -8_600 };
                assert_eq!(
                    ledger
                        .venue_book(ACCT_A, VP)
                        .cash
                        .iter()
                        .find(|c| c.asset == USDT)
                        .unwrap()
                        .total
                        .0,
                    50_000_000_000 + cash
                );
                assert_eq!(
                    ledger.pos(ACCT_A, P1).qty.0,
                    if side == Side::Buy { 100_000 } else { -100_000 }
                );
                assert_eq!(e.rebuilt_hashes(), Ok(Some(e.hashes())), "delivery {n}");
                if n == 0 {
                    e.crash_and_recover(CrashPoint::SeqInput, None).unwrap();
                }
            }
        }
    }
}

#[test]
fn tk_133_raw_receipts_reconcile_gap_then_confirm_after_restart() {
    use hft_event::ReconcileReport;
    use hft_testkit::dst::sut::{BootCtx, CrashPoint, StepOut, Sut};
    use hft_testkit::dst::{SeqSut, actors::ReconActor};
    use hft_testkit::strategies::TestStrategies;
    use zerocopy::FromZeros;
    for side in [Side::Buy, Side::Sell] {
        for qty in [100_000, 200_000] {
            let world = WorldSpec::w1();
            let mut e = SeqSut::<TestStrategies>::boot(&BootCtx {
                seed: 1,
                world: &world,
                planted: &[],
                tsc_hz: 1_000_000_000,
            })
            .unwrap();
            let mut v = venue();
            let opening = seed(&mut v, side);
            let mut out = StepOut::default();
            e.process(
                10 * MS,
                10 * MS,
                &InputRec::of(&opening, Source::Venue(VP)),
                &mut out,
            );
            v.m.external(ACCT_A, P1, side.opposite(), qty, "adl-missed", false, 20 * MS);
            let original = v.m.external_history().last().unwrap().clone();
            let expected = v.truth(101 * MS);
            let truth = expected
                .iter()
                .find(|t| t.account == ACCT_A && t.venue == VP)
                .unwrap();
            let truth_wallet = truth.balances.iter().find(|(a, _)| *a == USDT).unwrap().1;
            let truth_position = truth
                .positions
                .iter()
                .find(|(i, _)| *i == P1)
                .map_or(0, |(_, q)| *q);
            // First complete stable view; the following report can request AUTO_EXTERNAL.
            let mut summary = ReconcileReport::new_zeroed();
            summary.report_id = 901;
            summary.venue = VP;
            summary.account = ACCT_A;
            summary.item = 5;
            summary.count = 1;
            summary.view_seq = e.journal().last().unwrap().seq;
            e.process(
                100 * MS,
                100 * MS,
                &InputRec::of(&summary, Source::Recon),
                &mut out,
            );
            assert!(e.sequencer().engine().risk.auto_external_ok(ACCT_A, VP, 0));
            summary.report_id = 902;
            summary.count = 2;
            summary.view_seq = e.journal().last().unwrap().seq;
            let mut position = summary;
            position.idx = 1;
            position.item = 1;
            position.inst = P1;
            position.engine_qty = Qty(side.sign() * 200_000);
            position.exch_qty = Qty(side.sign() * (200_000 - qty));
            position.sflags = ReconcileReport::AUTO_EXTERNAL;
            let input = ReconActor::settled_input(&[summary, position], &[original.clone()]).unwrap();
            e.process(101 * MS, 101 * MS, &input, &mut out);
            let cash = if side == Side::Buy { 8_600 } else { -8_600 } * (qty / 100_000);
            assert_eq!(truth_wallet, i128::from(50_000_000_000i64 + cash));
            assert_eq!(truth_position, i128::from(position.exch_qty.0));
            assert_eq!(
                e.sequencer()
                    .engine()
                    .ledger
                    .auto_held(ACCT_A, P1, side.opposite()),
                Qty(qty)
            );
            assert_eq!(e.rebuilt_hashes(), Ok(Some(e.hashes())));
            e.crash_and_recover(CrashPoint::SeqInput, None).unwrap();
            // The transport classifier may refine Unknown to ADL; money and key stay exact.
            let mut late = original.fill;
            late.cause = ExternalCause::Adl;
            for now in [102 * MS, 103 * MS] {
                e.process(now, now, &InputRec::of(&late, Source::Venue(VP)), &mut out);
                let l = &e.sequencer().engine().ledger;
                assert_eq!(l.auto_held(ACCT_A, P1, side.opposite()), Qty(0));
                assert_eq!(l.pos(ACCT_A, P1).qty, position.exch_qty);
                assert_eq!(
                    i128::from(
                        l.venue_book(ACCT_A, VP)
                            .cash
                            .iter()
                            .find(|c| c.asset == USDT)
                            .unwrap()
                            .total
                            .0
                    ),
                    truth_wallet
                );
                assert_eq!(e.rebuilt_hashes(), Ok(Some(e.hashes())));
            }
            assert_eq!(v.truth(101 * MS), expected);
        }
    }
}
