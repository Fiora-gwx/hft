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
use zerocopy::IntoBytes;

const MS: u64 = 1_000_000;
const LAG: u64 = 2_000 * MS;

struct Session {
    e: SeqSut<TestStrategies>,
    venues: Vec<MockVenue<SimMatcher>>,
    actor: ReconActor,
    now: u64,
}

impl Session {
    fn new(side: Side, qty: i64) -> Self {
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
        self.now = ms * MS;
        self.e
            .process(self.now, self.now, &input, &mut StepOut::default());
    }
    fn quiet(&mut self, ms: u64) {
        self.process(ms, HkActor::clock_status(ms * MS, 1_000_000_000));
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
        assert!(truth.iter().all(|t| !t.undelivered && t.excluded.is_empty()));
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
    for n in 0..=cap {
        let ms = 3_001 + u64::from(n);
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
