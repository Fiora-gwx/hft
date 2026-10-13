//! Automatic original-receipt selection, using the real engine and independent exchange truth.
use hft_event::{ConnRole, ConnState, ConnStateKind, EventKind, ReconcileBatchView, ReconcileReport};
use hft_testkit::dst::actors::{HkActor, ReconActor};
use hft_testkit::dst::seq::reconcile::ReceiptReader;
use hft_testkit::dst::sut::{BootCtx, CrashPoint, InputRec, Source, StepOut, Sut};
use hft_testkit::dst::venue_behavior::{
    EXTERNAL_HISTORY_CAP, ExternalReceipt, Matcher, MockVenue, MockVenueCfg,
};
use hft_testkit::dst::world::{ACCT_A, ACCT_B, P1, VP, WorldSpec};
use hft_testkit::dst::{SeqSut, SimMatcher};
use hft_testkit::strategies::TestStrategies;
use hft_types::{AccountId, InstId, Qty, Side};
use zerocopy::{IntoBytes, TryFromBytes};

const MS: u64 = 1_000_000;
const LAG: u64 = 2_000 * MS;

struct Session {
    e: SeqSut<TestStrategies>,
    venues: Vec<MockVenue<SimMatcher>>,
    actor: ReconActor,
    now: u64,
}

impl Session {
    fn opening(side: Side) -> Self {
        let world = WorldSpec::w1();
        let e = SeqSut::boot(&BootCtx {
            seed: 1,
            world: &world,
            planted: &[],
            tsc_hz: 1_000_000_000,
        })
        .unwrap();
        let venues = world
            .venues
            .iter()
            .map(|v| {
                let mut cfg = MockVenueCfg::w1(v.id);
                cfg.bbo_ping_ms = 0;
                let mut venue = MockVenue::new(cfg, &world, 1);
                venue.tick(0);
                venue
            })
            .collect();
        let mut s = Self {
            e,
            venues,
            actor: ReconActor::new(30_000 * MS),
            now: 0,
        };
        s.venue()
            .m
            .on_bbo(P1, (600_189_000, 1_000_000), (600_190_000, 1_000_000), 10 * MS);
        let opening = s
            .venue()
            .m
            .external(ACCT_A, P1, side, 200_000, "open", false, 10 * MS);
        s.process(10, InputRec::of(&opening, Source::Venue(VP)));
        s
    }
    fn new(side: Side, qty: i64) -> Self {
        let mut s = Self::opening(side);
        s.venue()
            .m
            .on_bbo(P1, (600_275_000, 1_000_000), (600_276_000, 1_000_000), 20 * MS);
        s.venue()
            .m
            .external(ACCT_A, P1, side.opposite(), qty / 2, "adl-first", false, 20 * MS);
        s.venue()
            .m
            .on_bbo(P1, (600_375_000, 1_000_000), (600_376_000, 1_000_000), 30 * MS);
        s.venue().m.external(
            ACCT_A,
            P1,
            side.opposite(),
            qty - qty / 2,
            "adl-second",
            false,
            30 * MS,
        );
        s.quiet(3_000);
        s
    }
    fn venue(&mut self) -> &mut MockVenue<SimMatcher> {
        self.venues.iter_mut().find(|v| v.cfg.venue == VP).unwrap()
    }
    fn history(&self) -> Vec<ExternalReceipt> {
        self.venues
            .iter()
            .flat_map(|v| v.m.external_history().iter().cloned())
            .collect()
    }
    fn truth(&self) -> Vec<hft_testkit::dst::obs::TruthRow> {
        self.venues.iter().flat_map(|v| v.truth(self.now)).collect()
    }
    fn process(&mut self, ms: u64, input: InputRec) {
        self.process_out(ms, input);
    }
    fn process_out(&mut self, ms: u64, input: InputRec) -> StepOut {
        self.now = ms * MS;
        let mut out = StepOut::default();
        self.e.process(self.now, self.now, &input, &mut out);
        out
    }
    fn acknowledge(&mut self) -> bool {
        let v = self.venues.iter_mut().find(|v| v.cfg.venue == VP).unwrap();
        v.acknowledge_reconciled(&self.e, self.now).unwrap()
    }
    fn quiet(&mut self, ms: u64) {
        // Price changes can enqueue margin reports even when external fills have no push.
        // Deliver those real reports before comparing truth; never clear its skip flags.
        let mut rows = Vec::new();
        for venue in &mut self.venues {
            let mut at = ms * MS;
            for _ in 0..16 {
                rows.extend(venue.tick(at));
                match venue.m.next_due() {
                    Some(due) => at = at.max(due),
                    None => break,
                }
            }
            assert!(venue.m.next_due().is_none(), "reports did not quiesce");
        }
        rows.sort_by_key(|(due, _)| *due);
        for (due, row) in rows {
            assert_ne!(row.kind, EventKind::EXTERNAL_FILL, "gap must stay unreported");
            self.process(ms.max(due.div_ceil(MS)), row);
        }
        let end = ms.max(self.now / MS) + 1;
        self.process(end, HkActor::clock_status(end * MS, 1_000_000_000));
    }
    fn round(&mut self) -> Result<Vec<InputRec>, String> {
        let truth = self.truth();
        let history = self.history();
        self.actor.round_committed(&self.e, &truth, &history, LAG)
    }
    fn deliver(&mut self, rows: Vec<InputRec>) {
        for row in rows {
            self.process(self.now / MS + 1, row);
        }
    }
    fn connection(&mut self, account: AccountId, state: ConnStateKind) {
        let c = ConnState {
            venue: VP,
            conn: 0,
            role: ConnRole::UserStream,
            state,
            account,
            inst: InstId(0),
            reason: 0,
            _pad2: [0; 3],
            rx_tsc: 0,
        };
        self.process(self.now / MS + 1, InputRec::of(&c, Source::Venue(VP)));
    }
    fn exact(&self) {
        let mut obs = hft_testkit::dst::obs::Obs::default();
        self.e.observe(&mut obs);
        let truth = self.truth();
        assert!(
            truth.iter().all(|t| !t.undelivered && t.excluded.is_empty()),
            "truth comparison would be skipped: {truth:?}"
        );
        use hft_testkit::dst::invariants::{EndView, InvId, InvState, InvariantSet, check_end};
        assert!(
            obs.slots.iter().all(|s| s.state.is_terminal()),
            "truth comparison must not skip in-flight orders"
        );
        check_end(
            InvariantSet::only(InvId::I16),
            &mut InvState::default(),
            &EndView {
                now_ns: self.now,
                obs: &obs,
                truth: &truth,
            },
        )
        .unwrap();
        assert_eq!(self.e.rebuilt_hashes(), Ok(Some(self.e.hashes())));
    }
}

fn batch(rows: &[InputRec]) -> Option<ReconcileBatchView<'_>> {
    rows.iter()
        .find(|r| r.kind == EventKind::RECONCILE_BATCH)
        .map(|r| ReconcileBatchView::read(&r.body).unwrap())
}

#[test]
fn tk_134_select_original_receipts_from_committed_history() {
    for side in [Side::Buy, Side::Sell] {
        for qty in [100_000, 200_000] {
            let mut s = Session::new(side, qty);
            let first = s.round().unwrap();
            assert!(
                batch(&first).is_none(),
                "one difference cannot authorize automatic settlement"
            );
            let positions: Vec<_> = first
                .iter()
                .filter_map(|r| r.read::<ReconcileReport>())
                .filter(|r| r.item == 1)
                .collect();
            assert_eq!(positions.len(), 1);
            assert_eq!(positions[0].exch_qty, Qty(side.sign() * (200_000 - qty)));
            assert_eq!(
                first
                    .iter()
                    .filter_map(|r| r.read::<ReconcileReport>())
                    .filter(|r| r.item == 5)
                    .count(),
                4
            );
            s.deliver(first);
            let second = s.round().unwrap();
            let b = batch(&second).expect("second stable round must contain original receipts");
            let history = s.history();
            assert_eq!(b.hdr.n_fills, 2);
            assert_eq!(
                b.fills().map(|f| f.as_bytes().to_vec()).collect::<Vec<_>>(),
                history[1..]
                    .iter()
                    .map(|r| r.fill.as_bytes().to_vec())
                    .collect::<Vec<_>>()
            );
            let reports: Vec<_> = b.reports().collect();
            let mut reader = ReceiptReader::default();
            assert_eq!(
                reader.select(&s.e, &reports, &history).unwrap().body,
                second
                    .iter()
                    .find(|r| r.kind == EventKind::RECONCILE_BATCH)
                    .unwrap()
                    .body
            );
            let mut overlap = history.clone();
            overlap.push(history[1].clone());
            assert_eq!(
                reader.select(&s.e, &reports, &overlap).unwrap().body,
                reader.select(&s.e, &reports, &history).unwrap().body
            );
            // Account, venue, inst and identity are independent namespaces, not trade_id alone.
            let mut other = history[1].clone();
            other.fill.account = ACCT_B;
            overlap.push(other);
            assert!(reader.select(&s.e, &reports, &overlap).is_ok());
            for case in 0..8 {
                let mut rows = reports.clone();
                let mut h = history.clone();
                match case {
                    0 => {
                        h.remove(1);
                    }
                    1 => {
                        let mut duplicate = h[1].clone();
                        duplicate.fill.cash_amount.0 += 1;
                        h.push(duplicate);
                    }
                    2 => {
                        h[1].fill.cash_qty = Qty(0);
                    }
                    3 => {
                        h[1].fill.side = side;
                    }
                    4 => {
                        for r in &mut rows {
                            r.view_seq = s.now;
                        }
                    }
                    5 => {
                        rows[1].engine_qty.0 += 1;
                    }
                    6 => {
                        h[1].fill.trade_id = 0;
                        h[1].fill.exch_order_id = 0;
                    }
                    _ => {
                        h[1].fill.fee = Qty(1);
                        h[1].fill.fee_asset = hft_types::AssetId(250);
                    }
                }
                assert!(reader.select(&s.e, &rows, &h).is_err(), "case {case}");
            }
            assert!(
                reader
                    .select(
                        &s.e,
                        &reports,
                        &vec![history[1].clone(); EXTERNAL_HISTORY_CAP + 1]
                    )
                    .is_err()
            );
            assert!(reader.positions(&s.e, ACCT_A, &[InstId(u32::MAX)]).is_err());
            s.deliver(second);
            s.exact();
            assert_eq!(
                s.e.sequencer()
                    .engine()
                    .ledger
                    .auto_held(ACCT_A, P1, side.opposite()),
                Qty(qty)
            );
            s.e.crash_and_recover(CrashPoint::SeqInput, None).unwrap();
            assert!(
                batch(&s.round().unwrap()).is_none(),
                "restored receipts are already booked"
            );
            for r in history[1..].iter().rev().chain(&history[1..]) {
                s.process(s.now / MS + 1, InputRec::of(&r.fill, Source::Venue(VP)));
                s.exact();
            }
            assert_eq!(
                s.e.sequencer()
                    .engine()
                    .ledger
                    .auto_held(ACCT_A, P1, side.opposite()),
                Qty(0)
            );
        }
    }
}

#[test]
fn tk_135_committed_round_requires_stable_complete_observations() {
    for case in 0..7 {
        let mut s = Session::new(Side::Buy, 200_000);
        let first = s.round().unwrap();
        s.deliver(first);
        match case {
            0 => s.connection(ACCT_A, ConnStateKind::Down),
            1 => {
                s.connection(ACCT_A, ConnStateKind::Down);
                s.connection(ACCT_A, ConnStateKind::Up);
            }
            2 => s.e.crash_and_recover(CrashPoint::SeqInput, None).unwrap(),
            3 => {
                // A duplicate push changes no position but still invalidates a quiet window.
                let opening = s.history()[0].fill;
                s.process(s.now / MS + 1, InputRec::of(&opening, Source::Venue(VP)));
            }
            4 => {
                let mut truth = s.truth();
                truth
                    .iter_mut()
                    .find(|t| t.account == ACCT_A && t.venue == VP)
                    .unwrap()
                    .positions = vec![(P1, 100_000)];
                let history = s.history();
                let rows = s.actor.round_committed(&s.e, &truth, &history, LAG).unwrap();
                assert!(batch(&rows).is_none(), "changed quantity resets stability");
                continue;
            }
            5 => {
                let mut truth = s.truth();
                truth.retain(|t| !(t.account == ACCT_B && t.venue == VP));
                let history = s.history();
                assert!(s.actor.round_committed(&s.e, &truth, &history, LAG).is_err());
                continue;
            }
            _ => s.connection(ACCT_B, ConnStateKind::Down),
        }
        let rows = s.round().unwrap();
        assert_eq!(
            batch(&rows).is_some(),
            case == 6,
            "case {case}: account-local conditions"
        );
        if case == 3 {
            s.deliver(rows);
            for round in 2..=3 {
                let rows = s.round().unwrap();
                assert!(batch(&rows).is_none(), "forced reporting cannot authorize cash");
                let positions = rows
                    .iter()
                    .filter_map(|r| r.read::<ReconcileReport>())
                    .filter(|r| r.item == 1 && r.account == ACCT_A)
                    .count();
                assert_eq!(positions, usize::from(round == 3));
                s.deliver(rows);
            }
        }
    }
    for exchange in [200_001, -1] {
        let mut s = Session::new(Side::Buy, 100_000);
        let mut truth = s.truth();
        truth
            .iter_mut()
            .find(|t| t.account == ACCT_A && t.venue == VP)
            .unwrap()
            .positions = vec![(P1, exchange)];
        let history = s.history();
        for _ in 0..2 {
            let rows = s.actor.round_committed(&s.e, &truth, &history, LAG).unwrap();
            assert!(
                batch(&rows).is_none(),
                "growth and sign reversal never authorize AUTO_EXTERNAL"
            );
            s.deliver(rows);
        }
    }
    let mut s = Session::new(Side::Buy, 100_000);
    let first = s.round().unwrap();
    assert!(batch(&s.round().unwrap()).is_none(), "no new committed boundary");
    s.deliver(first);
    let history = s.history();
    let truth = s.truth();
    assert!(
        s.actor.round_committed(&s.e, &truth, &history[..1], LAG).is_err(),
        "missing originals must fail"
    );
}

#[test]
fn tk_136_receipt_selection_survives_hot_dedup_eviction() {
    let mut s = Session::new(Side::Buy, 100_000);
    let cap = s.e.sequencer().engine().setup.ledger.ext_dedup_cap;
    let opening = s.history()[0].fill;
    let start = s.now / MS + 1;
    for n in 0..=cap {
        let ms = start + u64::from(n);
        let fill = s
            .venue()
            .m
            .external(ACCT_A, P1, Side::Buy, 1, "booked-later", false, ms * MS);
        s.process(ms, InputRec::of(&fill, Source::Venue(VP)));
    }
    let seen = s.e.sequencer().engine().ledger.ext_seen(ACCT_A, VP);
    assert_eq!(seen.len(), cap as usize);
    assert!(
        !seen.iter().any(|k| k.kind == 1 && k.a == opening.trade_id),
        "must actually evict the old identity"
    );
    s.quiet(9_000);
    let first = s.round().unwrap();
    s.deliver(first);
    let second = s.round().unwrap();
    let b = batch(&second).unwrap();
    assert_eq!(b.fills().map(|f| f.trade_id).collect::<Vec<_>>(), vec![2, 3]);
    s.deliver(second);
    s.exact();
}

/// A transport gap with a real exchange execution and two actual cold-actor rounds. This
/// fixture opens directly; LED-081 still requires the strategy's PairOnFill and full seed set.
fn staged_gap(side: Side, full: bool, replay: bool) -> (Session, hft_event::ExternalFill, Vec<InputRec>) {
    use hft_testkit::dst::faults::{ConnKind, Fault};
    let mut s = Session::opening(side);
    s.quiet(3_000);
    s.exact();
    let connection = s.venue().apply_fault(
        &Fault::ConnDown {
            venue: VP,
            conn: ConnKind::UserStream,
            down_ms: 1_000,
            resync: false,
        },
        4_000 * MS,
    );
    for (at, row) in connection.iter().filter(|(at, _)| *at == 4_000 * MS) {
        s.process(at / MS, row.clone());
    }
    s.venue()
        .m
        .on_bbo(P1, (600_375_000, 1_000_000), (600_376_000, 1_000_000), 4_100 * MS);
    let rows = s.venue().apply_fault(
        &Fault::ReconciledAdl {
            account: ACCT_A,
            inst: P1,
            full,
            replay,
        },
        4_500 * MS,
    );
    for (at, row) in rows {
        assert_ne!(row.kind, EventKind::EXTERNAL_FILL);
        s.process(at.div_ceil(MS), row);
    }
    let original = s.venue().m.external_history().last().unwrap().fill;
    assert_eq!(original.qty, Qty(if full { 200_000 } else { 100_000 }));
    assert_eq!(original.cash_qty, original.qty);
    assert_ne!(original.cash_amount.0, 0);
    assert_eq!(s.venue().m.external_history().len(), 2);
    for (at, row) in connection.into_iter().filter(|(at, _)| *at > 4_000 * MS) {
        s.process(at / MS, row);
    }
    s.quiet(8_000);
    assert!(
        !s.acknowledge(),
        "an exchange history row is not proof of engine booking"
    );
    let first = s.round().unwrap();
    assert!(batch(&first).is_none());
    s.deliver(first);
    let second = s.round().unwrap();
    let b = batch(&second).expect("second stable round must produce a receipt batch");
    assert_eq!(b.hdr.n_fills, 1);
    assert_eq!(b.fills().next().unwrap().as_bytes(), original.as_bytes());
    assert!(!s.acknowledge(), "an unsubmitted batch cannot release the gap");
    let now = s.now;
    assert!(
        s.venue()
            .tick(now)
            .iter()
            .all(|(_, i)| i.kind != EventKind::EXTERNAL_FILL)
    );
    let truth = s.truth();
    let t = truth
        .iter()
        .find(|t| t.account == ACCT_A && t.venue == VP)
        .unwrap();
    assert!(t.undelivered);
    assert!(t.excluded.is_empty(), "no inst/asset alias can hide the wallet");
    (s, original, second)
}

fn deliver_checked(s: &mut Session, rows: Vec<InputRec>) -> Vec<hft_testkit::dst::sut::EventStep> {
    let mut points = Vec::new();
    for row in rows {
        let out = s.process_out(s.now / MS + 1, row);
        for event in out.events {
            if event.ledger.is_some() {
                s.e.verify_ledger_event(&event).unwrap();
                points.push(event);
            }
        }
    }
    points
}

fn confirm_gap(s: &mut Session, original: hft_event::ExternalFill) -> hft_testkit::dst::sut::EventStep {
    let at = s.now;
    let before = s.venue().m.state_hash();
    assert!(s.acknowledge());
    assert!(!s.acknowledge(), "a gap is released once");
    assert!(
        s.venue()
            .tick(at)
            .iter()
            .all(|(_, i)| i.kind != EventKind::EXTERNAL_FILL)
    );
    let mut rows = s.venue().tick(at + MS);
    rows.sort_by_key(|r| r.0);
    let mut confirmed = None;
    for (due, row) in rows {
        if row.kind == EventKind::EXTERNAL_FILL {
            let fill = hft_event::ExternalFill::try_read_from_bytes(&row.body).unwrap();
            let mut expected = original;
            expected.cause = hft_event::ExternalCause::Adl;
            assert_eq!(fill.as_bytes(), expected.as_bytes());
            let out = s.process_out(due.div_ceil(MS), row);
            let point = out.events.into_iter().find(|e| e.ledger.is_some()).unwrap();
            s.e.verify_ledger_event(&point).unwrap();
            assert!(confirmed.replace(point).is_none());
        } else {
            s.process(due.div_ceil(MS), row);
        }
    }
    assert_eq!(s.venue().m.state_hash(), before, "delivery must not settle again");
    let confirmed = confirmed.expect("same original ADL must actually be delivered");
    assert_eq!(
        s.e.sequencer()
            .engine()
            .ledger
            .auto_held(ACCT_A, P1, original.side),
        Qty(0)
    );
    let mut duplicate = original;
    duplicate.cause = hft_event::ExternalCause::Adl;
    let points = deliver_checked(s, vec![InputRec::of(&duplicate, Source::Venue(VP))]);
    assert_eq!(points.len(), 1);
    assert_eq!(offset_notes(&points[0].records).len(), 0);
    s.quiet(s.now / MS + 100);
    s.exact();
    confirmed
}

fn offset_notes(rows: &[hft_testkit::dst::JRec]) -> Vec<hft_event::Note> {
    rows.iter()
        .filter_map(|r| r.try_read::<hft_event::Note>())
        .filter(|n| {
            n.source == hft_event::NoteSource::Ledger && n.code == hft_ledger::notes::code::AUTO_HELD_OFFSET
        })
        .collect()
}

#[test]
fn tk_138_reconciled_adl_waits_for_commit_then_restores_full_truth() {
    let mut evidence = Vec::new();
    for side in [Side::Buy, Side::Sell] {
        for full in [false, true] {
            for replay in [false, true] {
                for restart in [false, true] {
                    let (mut s, original, rows) = staged_gap(side, full, replay);
                    let points = deliver_checked(&mut s, rows);
                    assert_eq!(points.len(), 1);
                    assert_eq!(
                        s.e.sequencer()
                            .engine()
                            .ledger
                            .auto_held(ACCT_A, P1, original.side),
                        original.qty
                    );
                    if restart {
                        s.e.clean_restart().unwrap();
                        s.quiet(s.now / MS + 3_000);
                    }
                    if replay {
                        let point = confirm_gap(&mut s, original);
                        assert_eq!(offset_notes(&point.records).len(), 1);
                    } else {
                        assert!(s.acknowledge());
                        s.quiet(s.now / MS + 100);
                        s.exact();
                        assert_eq!(
                            s.e.sequencer()
                                .engine()
                                .ledger
                                .auto_held(ACCT_A, P1, original.side),
                            original.qty
                        );
                    }
                    assert_eq!(s.venue().m.external_history().len(), 2);
                    let proof = s.e.committed_receipts(&[original]).unwrap();
                    assert_eq!(proof, vec![true]);
                    // Independent fixed-point reference from the two scripted execution prices.
                    let expected_cash =
                        i128::from(side.sign()) * i128::from(original.qty.0) * 186_000 / 1_000_000;
                    assert_eq!(i128::from(original.cash_amount.0), expected_cash);
                    let wallet = s.e.sequencer().engine().ledger.margin(ACCT_A, VP).wallet.0;
                    assert_eq!(i128::from(wallet), 50_000_000_000 + expected_cash);
                    let mut other = original;
                    other.account = ACCT_B;
                    assert_eq!(s.e.committed_receipts(&[other]).unwrap(), vec![false]);
                    evidence.push(format!(
                        "{{\"side\":{},\"full\":{full},\"replay\":{replay},\"restart\":{restart},\"qty\":{},\"cash\":{},\"wallet\":{wallet},\"batch_input_seq\":{},\"batch_end_seq\":{},\"offset_notes\":{},\"auto_held_qty\":{},\"truth_pairs\":{},\"truth_unskipped\":true}}",
                        side as u8, original.qty.0, original.cash_amount.0,
                        points[0].records[0].seq, points[0].ledger.as_ref().unwrap().through_seq,
                        offset_notes(s.e.journal()).len(),
                        s.e.sequencer().engine().ledger.auto_held(ACCT_A, P1, original.side).0,
                        s.truth().len()
                    ));
                    other = original;
                    other.trade_id += 1;
                    assert_eq!(s.e.committed_receipts(&[other]).unwrap(), vec![false]);
                }
            }
        }
    }
    // A rejected batch is committed journal input too; its receipt must remain unbooked.
    let (mut s, original, rows) = staged_gap(Side::Sell, false, true);
    let b = batch(&rows).unwrap();
    let reports: Vec<_> = b.reports().collect();
    let mut bad = original;
    bad.cash_qty = Qty(0);
    let input = ReconActor::settled_input(
        &reports,
        &[ExternalReceipt {
            fill: bad,
            client_id: "adl-reconciled".into(),
        }],
    )
    .unwrap();
    deliver_checked(&mut s, vec![input]);
    assert!(!s.acknowledge());
    assert_eq!(s.e.committed_receipts(&[original]).unwrap(), vec![false]);

    // Failed injection prerequisites leave exchange facts unchanged and latch an error.
    use hft_testkit::dst::faults::{ConnKind, Fault};
    for down in [false, true] {
        let mut v = MockVenue::<SimMatcher>::new(MockVenueCfg::w1(VP), &WorldSpec::w1(), 1);
        if down {
            v.apply_fault(
                &Fault::ConnDown {
                    venue: VP,
                    conn: ConnKind::UserStream,
                    down_ms: 1_000,
                    resync: false,
                },
                0,
            );
        }
        let before = v.m.state_hash();
        v.apply_fault(
            &Fault::ReconciledAdl {
                account: ACCT_A,
                inst: P1,
                full: true,
                replay: true,
            },
            MS,
        );
        assert!(
            v.fault_error()
                .unwrap()
                .contains(if down { "nonzero" } else { "stream gap" })
        );
        assert_eq!(v.m.state_hash(), before);
        assert!(v.m.external_history().is_empty());
    }
    assert_eq!(evidence.len(), 16);
    let path = hft_testkit::tmp::per_process_file(
        &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/dst"),
        "reconcile-settlement",
        "json",
    );
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, format!("{{\"seed\":1,\"coverage\":\"direct-opening-transport-matrix\",\"led_081_complete\":false,\"rows\":[{}]}}\n", evidence.join(","))).unwrap();
}

fn log_to(s: &Session, through: u64) -> hft_sequencer::driver::replay::Log {
    let mut log = hft_sequencer::driver::replay::Log::from_segments(&s.e.sequencer().driver().io.j.segments);
    log.recs.truncate(log.recs.partition_point(|r| r.seq <= through));
    log
}

#[test]
fn tk_139_ledger_event_bytes_and_boundaries_cannot_be_skipped() {
    use hft_testkit::dst::seq::ledger::Check;
    let (mut s, _, rows) = staged_gap(Side::Sell, false, false);
    let points = deliver_checked(&mut s, rows);
    let event = &points[0];
    let point = event.ledger.as_ref().unwrap();
    let log = log_to(&s, point.through_seq);
    Check::default().check_event(&log, &point.sections).unwrap();
    for change in 0..3 {
        let mut bad = event.clone();
        match change {
            0 => bad.ledger = None,
            1 => bad.ledger.as_mut().unwrap().through_seq -= 1,
            _ => bad.ledger.as_mut().unwrap().through_seq += 1,
        }
        assert!(s.e.verify_ledger_event(&bad).is_err());
    }
    for change in 0..3 {
        let mut bytes = point.sections.clone();
        match change {
            0 => bytes[24] ^= 1,
            1 => {
                bytes.pop();
            }
            _ => bytes.clear(),
        }
        let mut checker = Check::default();
        let error = checker.check_event(&log, &bytes).unwrap_err();
        assert!(error.contains("sections 1/13/22 differ"), "{error}");
        assert_eq!(checker.check_event(&log, &point.sections).unwrap_err(), error);
    }
}

#[test]
fn led_295_cold_offset_notes_are_generated_and_compared_independently() {
    use hft_testkit::dst::seq::ledger::Check;
    let (mut s, original, rows) = staged_gap(Side::Buy, true, true);
    deliver_checked(&mut s, rows);
    let event = confirm_gap(&mut s, original);
    let point = event.ledger.as_ref().unwrap();
    let log = log_to(&s, point.through_seq);
    let counts = Check::default().check_event(&log, &point.sections).unwrap();
    assert_eq!(counts.auto_held_offsets, 1);
    let index = log
        .recs
        .iter()
        .position(|r| {
            r.kind == EventKind::NOTE
                && r.read::<hft_event::Note>().is_some_and(|n| {
                    n.source == hft_event::NoteSource::Ledger
                        && n.code == hft_ledger::notes::code::AUTO_HELD_OFFSET
                })
        })
        .unwrap();
    for change in 0..4 {
        let mut bad = log.clone();
        match change {
            0 => {
                let r = bad.recs.remove(index);
                if r.flags.contains(hft_event::RecordFlags::EVENT_END) {
                    bad.recs[index - 1].flags.0 |= hft_event::RecordFlags::EVENT_END.0;
                }
            }
            1 => {
                let r = bad.recs[index].clone();
                bad.recs[index].flags.0 &= !hft_event::RecordFlags::EVENT_END.0;
                bad.recs.insert(index + 1, r);
            }
            _ => {
                let mut n = bad.recs[index].read::<hft_event::Note>().unwrap();
                if change == 2 {
                    n.v[4] += 1;
                } else {
                    n.inst.0 += 1;
                }
                bad.recs[index].body = n.as_bytes().to_vec();
            }
        }
        let mut checker = Check::default();
        let error = checker.check(&bad).unwrap_err();
        assert!(error.contains("ledger offset evidence"), "{error}");
        assert_eq!(checker.check(&log).unwrap_err(), error);
    }
}
