//! TK-057–TK-063, TK-097: the venue behaviour layer and the actors.

use hft_event::{
    ConnRole, ConnStateKind, EventKind, GwCmd, GwOp, OrderEvent, OrderEventKind, ReconcileReport, Timer,
};
use hft_testkit::dst::actors::{AdvisorActor, HkActor, ReconActor, advice_basis};
use hft_testkit::dst::faults::{AdvisorLagFault, Fault, FaultCategory};
use hft_testkit::dst::obs::TruthRow;
use hft_testkit::dst::sut::{InputRec, Source};
use hft_testkit::dst::venue_behavior::{Matcher, MockVenue, MockVenueCfg, ToyMatcher};
use hft_testkit::dst::world::{ACCT_A, AccountKind, S1, VP, VS, WorldSpec};
use hft_testkit::dst::{MarketSource, ToyEngine, run_record};
use hft_types::{AccountId, InstId, Px, Qty, Side, SlotId, Tif, Tsc, VenueId};

use crate::support::cfg;

fn place(slot: u32, qty: i64, px: i64) -> GwCmd {
    GwCmd {
        decision_seq: u64::from(slot),
        decided_tsc: Tsc(0),
        px: Px(px),
        qty: Qty(qty),
        inst: S1,
        slot: SlotId(slot),
        r#gen: slot,
        target_slot: SlotId(0),
        op: GwOp::Place,
        account: ACCT_A,
        side: Side::Buy,
        tif: Tif::Gtc,
        oflags: 0,
        _pad: [0; 2],
        arg: 0,
        arg2: 0,
    }
}

#[test]
#[cfg(feature = "seq")]
fn tk_126_external_reports_carry_exchange_cash_facts() {
    use hft_event::RepMsg;
    use hft_testkit::dst::seq::matcher::SimMatcher;
    use hft_testkit::dst::world::{BTC, P1, USDT};

    let world = WorldSpec::w1();
    let facts = [
        (Side::Sell, 100_000, 600_189_000, 0),
        (Side::Sell, 100_000, 600_244_000, 0),
        (Side::Buy, 200_000, 600_275_000, -11_700),
        (Side::Sell, 100_000, 600_274_000, 0),
        (Side::Buy, 100_000, 600_275_000, -100),
    ];
    for mirror in [false, true] {
        let mut matchers = [
            (true, SimMatcher::new(&world, VP, 1)),
            (false, SimMatcher::new(&world, VP, 1)),
        ];
        for (_, m) in &mut matchers {
            assert!(m.is_ready());
            m.drain_due(0, &mut Vec::new());
        }
        let mut wallet = 50_000_000_000_i128;
        let mut position = 0;
        for (i, (side, qty, price, cash)) in facts.into_iter().enumerate() {
            let side = if mirror { side.opposite() } else { side };
            let cash = if mirror { -cash } else { cash };
            wallet += i128::from(cash);
            position += if side == Side::Buy { qty } else { -qty };
            let now = (i as u64 + 1) * 10_000_000;
            for (push, m) in &mut matchers {
                m.on_bbo(P1, (price, 500_000_000), (price + 1_000, 500_000_000), now);
                m.external(ACCT_A, P1, side, qty, "foreign", *push, now);
                let mut out = Vec::new();
                m.drain_due(now + 999_999, &mut out);
                assert!(!out.iter().any(|r| matches!(r.rep, RepMsg::ExternalFill(_))));
                out.clear();
                m.drain_due(now + 1_000_000, &mut out);
                let fills: Vec<_> = out
                    .iter()
                    .filter_map(|r| match r.rep {
                        RepMsg::ExternalFill(e) => Some((r, e)),
                        _ => None,
                    })
                    .collect();
                assert_eq!(fills.len(), usize::from(*push));
                if *push {
                    let (r, e) = fills[0];
                    assert_eq!(r.client_id, "foreign");
                    assert_eq!((e.account, e.venue, e.inst), (ACCT_A, VP, P1));
                    assert_eq!((e.side, e.qty, e.px), (side, Qty(qty), Px(price)));
                    assert_eq!((e.cash_qty, e.cash_amount), (Qty(qty), Qty(cash)));
                    assert_eq!(e.fee, Qty(0));
                    assert_eq!(e.trade_id, i as u64 + 1);
                }
                let truth = m.truth(ACCT_A);
                assert_eq!(truth.balances, vec![(USDT, wallet)]);
                let expected = if position == 0 {
                    vec![]
                } else {
                    vec![(P1, i128::from(position))]
                };
                assert_eq!(truth.positions, expected);
            }
            assert_eq!(matchers[0].1.state_hash(), matchers[1].1.state_hash());
        }
    }
    let mut m = SimMatcher::new(&world, VS, 1);
    m.drain_due(0, &mut Vec::new());
    m.on_bbo(
        S1,
        (600_000_000, 500_000_000),
        (600_000_100, 500_000_000),
        10_000_000,
    );
    m.external(ACCT_A, S1, Side::Buy, 100_000, "spot", true, 10_000_000);
    let mut out = Vec::new();
    m.drain_due(11_000_000, &mut out);
    let fills: Vec<_> = out
        .iter()
        .filter_map(|r| match r.rep {
            RepMsg::ExternalFill(e) => Some(e),
            _ => None,
        })
        .collect();
    assert_eq!(fills.len(), 1);
    assert_eq!((fills[0].cash_qty, fills[0].cash_amount), (Qty(0), Qty(0)));
    let truth = m.truth(ACCT_A);
    assert_eq!(truth.inventory, vec![(BTC, 100_000)]);
    assert_eq!(truth.balances, vec![(USDT, 99_940_000_000)]);

    for (account, inst, qty) in [
        (AccountId(0), P1, 100_000),
        (ACCT_A, S1, 100_000),
        (ACCT_A, P1, 0),
    ] {
        let mut m = SimMatcher::new(&world, VP, 1);
        m.drain_due(0, &mut Vec::new());
        let before = m.state_hash();
        let rejected = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            m.external(account, inst, Side::Buy, qty, "invalid", true, 0);
        }));
        assert!(rejected.is_err(), "invalid external fact must fail the DST run");
        assert_eq!(m.state_hash(), before);
        let mut out = Vec::new();
        m.drain_due(1_000_000, &mut out);
        assert!(!out.iter().any(|r| matches!(r.rep, RepMsg::ExternalFill(_))));
    }
}

#[test]
#[cfg(feature = "seq")]
fn tk_128_no_client_id_uses_the_accounts_ack_identity_and_keeps_cash() {
    use hft_testkit::dst::seq::matcher::SimMatcher;
    use hft_testkit::dst::world::{ACCT_B, P1};
    for (within_hold, delayed_ack, hold_ms) in [
        (false, false, 5_000),
        (true, false, 5_000),
        (false, true, 5_000),
        (true, true, 5_000),
        (false, true, 0),
        (true, true, 0),
    ] {
        let mut world = WorldSpec::w1();
        world.venues.iter_mut().find(|v| v.id == VP).unwrap().perp_cash = true;
        let tick = world.insts.iter().find(|i| i.id == P1).unwrap().tick;
        let bid = 600_010_000;
        let ask = bid + tick;
        assert_eq!(bid % tick, 0);
        let mut cfg = MockVenueCfg::w1(VP);
        cfg.latency_ns = (1_000_000, 1_000_000);
        cfg.unmatched_hold_ms = hold_ms;
        let mut plain: MockVenue<SimMatcher> = MockVenue::new(cfg.clone(), &world, 1);
        let mut faulted: MockVenue<SimMatcher> = MockVenue::new(cfg, &world, 1);
        for v in [&mut plain, &mut faulted] {
            v.tick(0);
            for a in [ACCT_A, ACCT_B] {
                v.m.external(a, P1, Side::Sell, 100_000, "manual", false, 10_000_000);
            }
        }
        let mut exchange_ids = Vec::new();
        for (ix, account) in [ACCT_A, ACCT_B].into_iter().enumerate() {
            let mut cmd = place(10 + ix as u32, 100_000, ask);
            cmd.account = account;
            cmd.inst = P1;
            cmd.arg = 7 + ix as u32;
            let now = 40_000_000 + ix as u64 * 50_000_000;
            for v in [&mut plain, &mut faulted] {
                v.on_market(P1, (bid, 10_000_000), (ask, 10_000_000), now);
            }
            faulted.apply_fault(
                &Fault::NoClientIdFill {
                    venue: VP,
                    within_hold,
                },
                now,
            );
            if delayed_ack && ix == 1 {
                for v in [&mut plain, &mut faulted] {
                    v.apply_fault(&Fault::FillBeforeAck { venue: VP }, now);
                }
            }
            let mut expected = plain.on_frame(&cmd, now);
            let mut actual = faulted.on_frame(&cmd, now);
            expected.extend(plain.tick(now + 20_000_000));
            actual.extend(faulted.tick(now + 20_000_000));
            let events = |rows: &[(u64, InputRec)]| {
                rows.iter()
                    .filter_map(|(t, i)| i.read_try::<OrderEvent>().map(|e| (*t, e)))
                    .collect::<Vec<_>>()
            };
            let expected = events(&expected);
            let actual = events(&actual);
            let ack = expected
                .iter()
                .find(|(_, e)| e.what == OrderEventKind::Ack)
                .unwrap_or_else(|| panic!("missing Ack: {expected:?}"));
            let fill = expected
                .iter()
                .find(|(_, e)| e.what == OrderEventKind::Fill)
                .unwrap_or_else(|| panic!("missing Fill: {expected:?}"));
            assert_ne!(fill.1.cash_sum.0, 0);
            assert_eq!(fill.1.cash_qty, fill.1.cum_filled);
            let copies: Vec<_> = actual
                .iter()
                .filter(|(_, e)| e.what == OrderEventKind::Fill)
                .collect();
            if delayed_ack && ix == 1 {
                assert!(ack.0 > fill.0);
                if hold_ms == 0 {
                    let mut unresolved = *fill;
                    unresolved.1.slot = SlotId::UNRESOLVED;
                    unresolved.1.r#gen = 0;
                    unresolved.1.epoch = 0;
                    assert_eq!(copies, [&unresolved, fill]);
                } else {
                    let held = (ack.0, fill.1);
                    assert_eq!(copies, [&held, fill]);
                    assert!(
                        faulted
                            .truth(ack.0 - 1)
                            .iter()
                            .find(|t| t.account == account)
                            .unwrap()
                            .undelivered
                    );
                }
            } else {
                assert_eq!(ack.0, fill.0);
                assert_eq!(expected[0].1.what, OrderEventKind::Ack);
                assert_eq!(copies, [fill, fill]);
            }
            assert_eq!(actual.len(), expected.len() + 1);
            assert_eq!(
                (fill.1.account, fill.1.epoch, fill.1.slot, fill.1.r#gen),
                (account, cmd.arg as u16, cmd.slot, cmd.r#gen)
            );
            exchange_ids.push(fill.1.exch_order_id);
            assert_eq!(plain.m.truth(account), faulted.m.truth(account));
            assert_eq!(plain.m.state_hash(), faulted.m.state_hash());
        }
        // The same exchange order id in two accounts must never borrow the other's identity.
        assert_eq!(exchange_ids[0], exchange_ids[1]);
    }
}

fn venue(dead_man_ms: u32) -> MockVenue<ToyMatcher> {
    let mut c = MockVenueCfg::w1(VS);
    c.rate_per_s = 10;
    c.dead_man_ms = dead_man_ms;
    c.bbo_ping_ms = 1_000;
    MockVenue::new(c, &WorldSpec::w1(), 1)
}

fn rejects(ins: &[(u64, InputRec)]) -> Vec<u32> {
    ins.iter()
        .filter_map(|(_, i)| i.read_try::<OrderEvent>())
        .filter(|e| e.what == OrderEventKind::Reject)
        .map(|e| e.reject_code)
        .collect()
}

#[test]
fn tk_103_late_fee_delivery() {
    use hft_testkit::dst::world::{BTC, USDT};
    for (base, dup, clean) in [
        (false, false, false),
        (true, false, false),
        (false, true, false),
        (false, false, true),
    ] {
        let mut w = WorldSpec::w1();
        w.venues.iter_mut().find(|v| v.id == VS).unwrap().fee_in_base = base;
        let mut v: MockVenue<ToyMatcher> = MockVenue::new(MockVenueCfg::w1(VS), &w, 1);
        v.clean = clean;
        let before = v.m.truth(ACCT_A);
        v.apply_fault(
            &Fault::LateFeeOnly {
                venue: VS,
                delay_ms: 6_000,
            },
            0,
        );
        if dup {
            v.apply_fault(
                &Fault::ReportDup {
                    venue: VS,
                    dur_ms: 10_000,
                },
                0,
            );
        }
        // The resting order's Ack must not consume the queued fee fault.
        let ack = v.on_frame(&place(1, 1_000, 500_000_000), 0);
        assert!(ack.iter().all(|(_, i)| {
            i.read_try::<OrderEvent>()
                .is_none_or(|e| e.what != OrderEventKind::FeeOnly)
        }));
        let out = v.on_frame(&place(2, 100_000, 600_000_000), 10_000_000);
        let fills: Vec<_> = out
            .iter()
            .filter_map(|(t, i)| {
                i.read_try::<OrderEvent>()
                    .filter(|e| e.what == OrderEventKind::Fill)
                    .map(|e| (*t, e))
            })
            .collect();
        let fees: Vec<_> = out
            .iter()
            .filter_map(|(t, i)| {
                i.read_try::<OrderEvent>()
                    .filter(|e| e.what == OrderEventKind::FeeOnly)
                    .map(|e| (*t, e))
            })
            .collect();
        assert_eq!(fills.len(), if dup { 2 } else { 1 });
        if clean {
            assert!(fees.is_empty());
            assert!(fills[0].1.fee.0 > 0);
            continue;
        }
        assert_eq!(fees.len(), 1);
        let (due, fee) = fees[0];
        let first = fills.iter().map(|(t, _)| *t).min().unwrap();
        assert_eq!(due - first, 6_000_000_000);
        assert!(fills.iter().all(|(_, e)| e.fee.0 == 0 && e.fee_asset.0 == 0));
        assert_eq!(
            (fee.slot, fee.r#gen, fee.epoch, fee.cum_filled),
            (
                fills[0].1.slot,
                fills[0].1.r#gen,
                fills[0].1.epoch,
                fills[0].1.cum_filled
            )
        );
        assert_eq!(fee.qty, Qty(0));
        assert!(fee.fee.0 > 0);
        assert_eq!(fee.fee_asset, if base { BTC } else { USDT });
        assert!(
            v.truth(due - 1)
                .iter()
                .find(|t| t.account == ACCT_A)
                .unwrap()
                .undelivered
        );
        assert!(
            !v.truth(due + 1)
                .iter()
                .find(|t| t.account == ACCT_A)
                .unwrap()
                .undelivered
        );
        let truth = v.m.truth(ACCT_A);
        let amount =
            |xs: &[(hft_types::AssetId, i128)], a| xs.iter().find(|(x, _)| *x == a).map_or(0, |(_, q)| *q);
        if base {
            assert_eq!(
                amount(&truth.inventory, BTC) - amount(&before.inventory, BTC),
                100_000 - i128::from(fee.fee.0)
            );
        } else {
            assert_eq!(
                amount(&before.balances, USDT) - amount(&truth.balances, USDT),
                100_000 * 600_000_000i128 / 1_000_000 + i128::from(fee.fee.0)
            );
        }
        // The one-shot fault is consumed; the next fill carries its fee normally.
        let next = v.on_frame(&place(3, 100_000, 600_000_000), due + 1);
        assert!(!next.iter().any(|(_, i)| {
            i.read_try::<OrderEvent>()
                .is_some_and(|e| e.what == OrderEventKind::FeeOnly)
        }));
        assert!(next.iter().any(|(_, i)| {
            i.read_try::<OrderEvent>()
                .is_some_and(|e| e.what == OrderEventKind::Fill && e.fee.0 > 0)
        }));
    }
}

trait ReadTry {
    fn read_try<B: hft_event::Body>(&self) -> Option<B>;
}
impl ReadTry for InputRec {
    fn read_try<B: hft_event::Body>(&self) -> Option<B> {
        if self.kind != B::KIND {
            return None;
        }
        B::try_read_from_bytes(self.body.get(..size_of::<B>())?).ok()
    }
}

#[test]
fn tk_057_behavior_layer() {
    // TK-057.1: the 11th new order within one second is rejected 429-shaped; the first 10 pass.
    let mut v = venue(0);
    let mut codes = Vec::new();
    for k in 0..11u32 {
        codes.extend(rejects(
            &v.on_frame(&place(k + 1, 1_000, 500_000_000), 1_000_000 * u64::from(k)),
        ));
    }
    assert_eq!(codes, vec![429], "TK-057.1");
    // TK-057.2: not enough margin → -2019.
    let mut v = venue(0);
    let r = rejects(&v.on_frame(&place(1, 100_000_000_000, 600_000_000), 0));
    assert_eq!(r, vec![(-2019i32) as u32], "TK-057.2");
    // TK-057.3 / .4: dead-man 30 000 ms.
    for (row, refresh_at, want_cancel) in [("TK-057.3", None, true), ("TK-057.4", Some(29_999u64), false)] {
        let mut v = venue(30_000);
        let _ = v.on_frame(&place(1, 1_000, 500_000_000), 0); // resting (below the ask)
        if let Some(t) = refresh_at {
            let mut r = place(0, 0, 0);
            r.op = GwOp::DeadManRefresh;
            let _ = v.on_frame(&r, t * 1_000_000);
        }
        let out = v.tick(30_000 * 1_000_000);
        let cancels = out
            .iter()
            .filter_map(|(_, i)| i.read_try::<OrderEvent>())
            .filter(|e| e.what == OrderEventKind::CancelAck)
            .count();
        assert_eq!(cancels > 0, want_cancel, "{row}");
        assert_eq!(v.m.open_orders(ACCT_A).is_empty(), want_cancel, "{row}");
    }
    // TK-057.5: idle md connection for 3 500 ms → 3 pongs, 1 000 ms apart.
    let mut v = venue(0);
    let pings: Vec<u64> = v
        .tick(3_500 * 1_000_000)
        .iter()
        .filter(|(_, i)| {
            i.read_try::<hft_event::ConnState>()
                .is_some_and(|c| c.role == ConnRole::Md && c.state == ConnStateKind::Alive)
        })
        .map(|(t, _)| *t)
        .collect();
    assert_eq!(
        pings,
        vec![1_000_000_000, 2_000_000_000, 3_000_000_000],
        "TK-057.5"
    );
    // TK-057.6: a liquidation from the truth carries the venue's own marker.
    let mut w = WorldSpec::w1();
    w.accounts[0].venues = vec![VP];
    let mut c = MockVenueCfg::w1(VP);
    c.liquidation_prefix = "autoclose-";
    let mut v: MockVenue<ToyMatcher> = MockVenue::new(c, &w, 1);
    v.m.external(ACCT_A, InstId(2), Side::Sell, 100_000, "seed", false, 0);
    let out = v.apply_fault(
        &Fault::SelfLiquidation {
            account: ACCT_A,
            inst: InstId(2),
        },
        1_000,
    );
    let x = out
        .iter()
        .find_map(|(_, i)| i.read_try::<hft_event::ExternalFill>())
        .expect("liquidation pushed");
    assert_eq!(
        x.cause,
        hft_event::ExternalCause::Liquidation,
        "TK-057.6: client id starts with autoclose-"
    );
}

#[test]
fn tk_058_sim_account_unfaulted() {
    let make = |faults: bool| {
        let mut c = cfg(
            3,
            2_000,
            if faults { &[FaultCategory::Report] } else { &[] },
            1_000,
            &[],
        );
        c.world.accounts[1].kind = AccountKind::Sim;
        c.rerun = hft_testkit::dst::RerunPolicy::Never;
        run_record::<ToyEngine>(&c)
    };
    // Exchange reports of the Sim account (engine-assigned seqs, as in `SendReport.decision_seq`,
    // naturally differ between the runs and are not compared).
    let sim_reports = |r: &hft_testkit::dst::RunRecord| -> Vec<(OrderEventKind, u32, u32, i64, i64)> {
        r.deliveries
            .iter()
            .filter(|(_, i)| i.src == Source::Sim)
            .filter_map(|(_, i)| i.read_try::<OrderEvent>())
            .map(|e| (e.what, e.slot.0, e.r#gen, e.qty.0, e.cum_filled.0))
            .collect()
    };
    let (faulted, clean) = (make(true), make(false));
    // Live reports were perturbed (duplicates at least).
    let live: Vec<Vec<u8>> = faulted
        .deliveries
        .iter()
        .filter(|(_, i)| matches!(i.src, Source::Venue(_)) && i.kind == EventKind::ORDER_EVENT)
        .map(|(_, i)| i.body.clone())
        .collect();
    let mut uniq = live.clone();
    uniq.sort();
    uniq.dedup();
    assert!(
        uniq.len() < live.len(),
        "Live reports duplicated under report faults"
    );
    let (a, b) = (sim_reports(&faulted), sim_reports(&clean));
    let n = a.len().min(b.len());
    assert!(n > 10, "sim reports seen ({n})");
    assert_eq!(
        a[..n],
        b[..n],
        "Sim reports identical with and without report faults"
    );
}

#[test]
fn tk_059_advisor_lag() {
    let mut c = cfg(5, 20_000, &[], 0, &[]);
    c.horizon_ns = 20_000_000_000;
    c.scripted.push((
        0,
        Fault::AdvisorLag(AdvisorLagFault {
            from_ens: 10_000_000_000,
            to_ens: 15_000_000_000,
            lag_ms: 5_000,
        }),
    ));
    let r = run_record::<ToyEngine>(&c);
    let adv: Vec<(u64, u64, u64)> = r
        .deliveries
        .iter()
        .filter(|(_, i)| i.kind == EventKind::ADVICE)
        .filter_map(|(t, i)| advice_basis(i).map(|(seq, b)| (*t, seq, b)))
        .collect();
    assert!(
        adv.iter()
            .all(|(t, _, _)| !(10_000_000_000..15_000_000_000).contains(t)),
        "nothing delivered in [10, 15) s"
    );
    let batch: Vec<_> = adv
        .iter()
        .filter(|(t, _, b)| *t >= 15_000_000_000 && (10_000_000_000..15_000_000_000).contains(b))
        .collect();
    assert!(
        batch.len() >= 40,
        "the lagged Advice arrives from 15 s on ({})",
        batch.len()
    );
    // Their basis seqs are below the engine's seq at delivery.
    let seq_at = |t: u64| r.journal.iter().rfind(|j| j.tsc <= t).map_or(0, |j| j.seq);
    assert!(batch.iter().all(|(t, seq, _)| *seq < seq_at(*t)));
}

#[test]
fn tk_060_ctl_principals() {
    let c = cfg(9, 5_000, &[FaultCategory::Control], 1_000, &[]);
    let r = run_record::<ToyEngine>(&c);
    let cmds: Vec<hft_event::CmdHdr> = r
        .journal
        .iter()
        .filter(|j| j.kind == EventKind::CONTROL_CMD)
        .filter_map(|j| hft_event::ControlCmd::decode(&j.body).ok().map(|(h, _)| h))
        .collect();
    assert!(!cmds.is_empty());
    assert!(
        cmds.iter().all(|h| h.role != 0 && h.issuer[0] != 0),
        "principal and role on every command"
    );
    assert!(
        cmds.iter().any(|h| h.role == hft_event::CmdHdr::ROLE_VIEWER),
        "a Viewer command"
    );
}

fn truth_one_more(a: AccountId) -> Vec<TruthRow> {
    vec![TruthRow {
        account: a,
        venue: VP,
        positions: vec![(InstId(2), 100_000)],
        inventory: Vec::new(),
        balances: Vec::new(),
        undelivered: false,
        excluded: Vec::new(),
    }]
}

#[test]
fn tk_061_recon_actor_flags() {
    let p = 30_000_000_000;
    let engine = |_: AccountId, _: VenueId, _: InstId| 0i128;
    let first_auto = |r: &[ReconcileReport]| r.iter().any(|x| x.sflags & ReconcileReport::AUTO_EXTERNAL != 0);
    // TK-061.1: two reports without USERSTREAM_DOWN, stream stable for a period.
    let mut a = ReconActor::new(p);
    a.round(p, &truth_one_more(ACCT_A), &engine);
    a.round(2 * p, &truth_one_more(ACCT_A), &engine);
    assert!(
        first_auto(&a.round(3 * p, &truth_one_more(ACCT_A), &engine)),
        "TK-061.1"
    );
    // TK-061.2: the previous report had USERSTREAM_DOWN.
    let mut a = ReconActor::new(p);
    a.userstream_down_until = p + 1;
    let prev = a.round(p, &truth_one_more(ACCT_A), &engine);
    assert!(
        prev.iter()
            .all(|x| x.sflags & ReconcileReport::USERSTREAM_DOWN != 0)
    );
    let next = a.round(2 * p, &truth_one_more(ACCT_A), &engine);
    assert!(!first_auto(&next), "TK-061.2");
    // TK-061.3: the stream recovered after the previous view point.
    let mut a = ReconActor::new(p);
    a.round(p, &truth_one_more(ACCT_A), &engine);
    a.userstream_down_until = p + p / 2;
    assert!(
        !first_auto(&a.round(2 * p, &truth_one_more(ACCT_A), &engine)),
        "TK-061.3"
    );
    // TK-061.4: injected funding differences produce per-item funding entries.
    let mut a = ReconActor::new(p);
    a.funding_diffs.push((ACCT_A, VP, 1_234));
    let r = a.round(p, &truth_one_more(ACCT_A), &engine);
    assert!(r.iter().any(|x| x.item == 5 && x.diff.0 == 1_234), "TK-061.4");
}

#[test]
fn tk_062_hk_injection() {
    let mut c = cfg(11, 100_000, &[], 0, &[]);
    c.horizon_ns = 10_000_000_000;
    c.scripted
        .push((5_000_000_000, Fault::PersistLag { lag_ms: 2_000 }));
    c.scripted.push((8_000_000_000, Fault::DiskLow));
    let r = run_record::<ToyEngine>(&c);
    let at = |k: EventKind| -> Vec<u64> {
        r.deliveries
            .iter()
            .filter(|(_, i)| i.kind == k)
            .map(|(t, _)| *t)
            .collect()
    };
    let clock = at(EventKind::CLOCK_STATUS);
    assert_eq!(clock.len(), 10, "{clock:?}");
    let timers: Vec<(u64, hft_event::TimerCode)> = r
        .deliveries
        .iter()
        .filter(|(_, i)| i.src == Source::Hk && i.kind == EventKind::TIMER)
        .filter_map(|(t, i)| {
            let x = i.read::<Timer>()?;
            hft_event::TimerCode::decode(x.owner, x.timer_id, x.group)
                .ok()
                .map(|c| (*t, c))
        })
        .collect();
    use hft_event::{SysTimer, TimerCode};
    assert!(
        timers
            .iter()
            .any(|(t, c)| *t >= 5_000_000_000 && matches!(c, TimerCode::Sys(SysTimer::PersistLag { .. })))
    );
    assert!(
        timers
            .iter()
            .any(|(t, c)| *t >= 8_000_000_000 && matches!(c, TimerCode::Sys(SysTimer::DiskLow { .. })))
    );
    let _ = HkActor::clock_status(0, 1);
}

#[test]
fn tk_063_recorded_market() {
    // A 500-record fragment from a generated run.
    let src = run_record::<ToyEngine>(&cfg(13, 10_000, &[], 0, &[]));
    let frag: Vec<(u64, InputRec)> = src
        .deliveries
        .iter()
        .filter(|(_, i)| i.kind == EventKind::BBO)
        .take(500)
        .cloned()
        .collect();
    assert_eq!(frag.len(), 500);
    let mut c = cfg(13, 100_000, &[], 0, &[]);
    c.market = MarketSource::Recorded(frag.clone());
    c.horizon_ns = frag.last().map_or(0, |f| f.0) + 1;
    let r = run_record::<ToyEngine>(&c);
    assert!(
        r.outcome.is_pass(),
        "{:?}",
        r.outcome.failure().map(|f| &f.message)
    );
    let got: Vec<&Vec<u8>> = r
        .deliveries
        .iter()
        .filter(|(_, i)| i.kind == EventKind::BBO)
        .map(|(_, i)| &i.body)
        .collect();
    let want: Vec<&Vec<u8>> = frag.iter().map(|(_, i)| &i.body).collect();
    assert_eq!(got, want, "bodies and order equal");
}

#[test]
fn tk_097_advisor_lag_by_engine_time() {
    let a = AdvisorActor {
        period_ns: 100_000_000,
        strategy: hft_types::StrategyId(1),
        lags: vec![AdvisorLagFault {
            from_ens: 10_000_000_000,
            to_ens: 13_000_000_000,
            lag_ms: 3_000,
        }],
    };
    for basis in (0..200u64).map(|k| k * 100_000_000) {
        let want = if (10_000_000_000..13_000_000_000).contains(&basis) {
            basis + 3_000_000_000
        } else {
            basis
        };
        assert_eq!(a.delivery(basis), want, "basis {basis}");
    }
    // Same seed twice: identical delivery sequence in a run.
    let run = || {
        let mut c = cfg(21, 30_000, &[], 0, &[]);
        c.horizon_ns = 20_000_000_000;
        c.scripted.push((
            0,
            Fault::AdvisorLag(AdvisorLagFault {
                from_ens: 10_000_000_000,
                to_ens: 13_000_000_000,
                lag_ms: 3_000,
            }),
        ));
        run_record::<ToyEngine>(&c)
            .deliveries
            .into_iter()
            .filter(|(_, i)| i.kind == EventKind::ADVICE)
            .map(|(t, i)| (t, advice_basis(&i)))
            .collect::<Vec<_>>()
    };
    let (x, y) = (run(), run());
    assert_eq!(x, y);
    for (t, b) in &x {
        let (_, basis) = b.expect("advice");
        let lagged = (10_000_000_000..13_000_000_000).contains(&basis);
        assert_eq!(
            *t,
            if lagged { basis + 3_000_000_000 } else { basis },
            "delivery of basis {basis}"
        );
    }
}
