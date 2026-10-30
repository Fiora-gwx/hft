//! The strategy set (TESTS.md "策略集": SEQ-015–SEQ-017).

use hft_event::ControlCmd;
use hft_sequencer::driver::live::RingId;
use hft_types::{AccountId, GroupId, StrategyId};

use crate::common::*;

#[test]
fn seq_015_instance_created_on_first_params() {
    reset_script();
    let mut b = b0();
    b.params.retain(|p| p.strategy != S2);
    let mut h = b.boot();
    h.steady();
    clear_log();
    h.quote(A1, "50000", "50002");
    h.run();
    assert!(h.s.instances().get(S2).is_some_and(|s| s.inst.is_none()));
    assert!(!log().iter().any(|g| g.s == S2));
    let n0 = calls().iter().filter(|c| c.0 == S2 && c.1 == "create").count();
    let id = h.set_params(&Spb::s2(1));
    h.run();
    assert_eq!(h.result_of(id).map(|r| r.status), Some(0));
    assert_eq!(
        calls().iter().filter(|c| c.0 == S2 && c.1 == "create").count(),
        n0 + 1
    );
    let mut p = Spb::s2(2);
    p.values[F_THR] = 7;
    let id = h.set_params(&p);
    h.run();
    assert_eq!(h.result_of(id).map(|r| r.status), Some(0));
    assert_eq!(
        calls().iter().filter(|c| c.0 == S2 && c.1 == "create").count(),
        n0 + 1
    );
    assert_eq!(
        calls().iter().filter(|c| c.0 == S2 && c.1 == "on_params").count(),
        1
    );
}

#[test]
fn seq_016_dispatch_order_live_first() {
    reset_script();
    let mut b = b0();
    // s3 on Live account 3, s1 on Sim account 9, s2 on Live account 2.
    let mut a3 = b.rd.accounts[2].clone();
    a3.bound_instance = StrategyId(3);
    a3.venues = vbits(&[A, B]);
    b.rd.accounts[2] = a3;
    b.rd.accounts[8].bound_instance = StrategyId(1);
    b.rd.accounts[0].bound_instance = StrategyId(0);
    b.rd.accounts[0].venues = 0;
    b.rd.instances[8].kind_id = TAKER;
    b.params = vec![
        Spb::new(
            StrategyId(3),
            AccountId(3),
            TAKER,
            1,
            vec![gspec(GroupId(0x0300_0001))],
        ),
        Spb::new(S1, AC9, TAKER, 1, vec![gspec(G1)]),
        Spb::s2(1),
    ];
    let mut h = b.boot();
    let ord: Vec<StrategyId> =
        h.s.instances()
            .dispatch_order
            .iter()
            .map(|ix| h.s.instances().slots[usize::from(*ix)].id)
            .collect();
    assert_eq!(&ord[..3], &[S2, StrategyId(3), S1]);
    h.step();
    clear_log();
    h.quote(A1, "50000", "50002");
    h.run();
    let got: Vec<StrategyId> = log()
        .iter()
        .filter(|g| g.kind == "Bbo" && !g.model)
        .map(|g| g.s)
        .collect();
    assert_eq!(got, vec![S2, StrategyId(3), S1]);
}

#[test]
fn seq_017_desc_version_in_section5() {
    reset_script();
    let mut h = b0().boot();
    h.steady();
    let n = h.snapshots().len();
    h.cmd(AC1, &ControlCmd::SnapshotNow);
    h.run();
    h.run();
    let snaps = h.snapshots();
    assert!(snaps.len() > n);
    let (hdr, secs) = snap(snaps.last().unwrap());
    assert_eq!(hdr.purpose, 3);
    let (r, g, _) = secs
        .strategies
        .iter()
        .find(|x| x.0.strategy == S1)
        .expect("s1 row");
    assert_eq!(r.account, AC1);
    assert_eq!(r.kind_id, TAKER);
    assert_eq!(r.desc_version, 3);
    assert_eq!(r.params_version, 1);
    assert_eq!(r.n_groups, 1);
    assert_eq!(g[0].group, G1);
    let _ = RingId::Ctl;
}

/// SEQ-451: an instance bound to two accounts (account set) has cells on both, may place on
/// either (not on an account of another instance), and accepts commands scoped to either.
#[test]
fn seq_451_instance_account_set() {
    use hft_types::RejectBits;
    reset_script();
    let mut b = b0();
    let mut a3 = b.rd.accounts[2].clone();
    a3.bound_instance = S1;
    a3.venues = vbits(&[A, B]);
    a3.label = "acct-3".into();
    b.rd.accounts[2] = a3;
    // One more Live account: more engine timers per account.
    b.caps.max_timers += 64;
    let mut h = b.boot();
    h.steady();
    // .1 the slot lists both accounts (primary = the lower id); one cell per account and group.
    let s1 = h.s.instances().get(S1).unwrap();
    assert_eq!(
        (s1.account, s1.accounts()),
        (AC1, &[AC1, AccountId(3)][..]),
        "SEQ-451.1"
    );
    let cells: Vec<_> =
        h.s.instances()
            .cells()
            .filter(|c| c.strategy == S1)
            .map(|c| c.account)
            .collect();
    assert_eq!(cells, vec![AC1, AccountId(3)], "SEQ-451.1");
    assert_eq!(
        h.s.instances().ix_of_account(AccountId(3)),
        h.s.instances().ix(S1),
        "SEQ-451.1"
    );
    let rd = &h.s.engine().rd;
    let rv = |a| hft_strategy_sdk::RefView::new(rd, a).accounts().to_vec();
    assert_eq!(
        (rv(AC1), rv(AccountId(3)), rv(AC2)),
        (vec![AC1, AccountId(3)], vec![AC1, AccountId(3)], vec![AC2]),
        "SEQ-451.1 RefView"
    );
    // .2 S1 places on account 3: not refused as unbound; on account 2 (bound to S2): NOT_BOUND.
    let m = h.mark();
    set_hook(|hx, ev, ctx| {
        if let hft_strategy_sdk::EventRef::Bbo { inst, .. } = ev
            && *inst == A1
            && hx.s.strategy == S1
        {
            for a in [AccountId(3), AC2] {
                let mut it = intent(
                    hx,
                    A1,
                    hft_types::Side::Buy,
                    hft_types::Tif::Gtc,
                    hft_strategy_sdk::IntentClass::Open,
                    px("49000"),
                    q("0.001"),
                );
                it.account = a;
                let _ = ctx.place(it);
            }
        }
    });
    h.quote(A1, "50000", "50002");
    h.run();
    clear_hook();
    let ds = decisions(&h.since(m));
    let on = |a: AccountId| ds.iter().find(|(_, d)| d.account == a).map(|(_, d)| d.reject);
    assert_eq!(
        on(AccountId(3)).map(|r| r & RejectBits::NOT_BOUND.0),
        Some(0),
        "SEQ-451.2 {ds:?}"
    );
    assert_eq!(
        on(AC2).map(|r| r & RejectBits::NOT_BOUND.0),
        Some(RejectBits::NOT_BOUND.0),
        "SEQ-451.2 {ds:?}"
    );
    // .3 a SetMode scoped to account 3 of S1: applied.
    let id = h.cmd_confirmed(
        AccountId(3),
        &ControlCmd::SetMode {
            scope: scope(S1, AccountId(3), GroupId(0)),
            mode: hft_types::RunMode::CloseOnly,
        },
    );
    h.run();
    assert_eq!(h.result_of(id).map(|r| r.status), Some(0), "SEQ-451.3");
}

/// SEQ-452: self-trade prevention between the accounts of one instance — a place that would
/// trade against another account's open order is refused with SELF_TRADE; a non-crossing one is
/// not; a single-account instance is never checked.
#[test]
fn seq_452_self_trade_between_instance_accounts() {
    use hft_types::{RejectBits, Side};
    reset_script();
    let mut b = b0();
    let mut a3 = b.rd.accounts[2].clone();
    a3.bound_instance = S1;
    a3.venues = vbits(&[A, B]);
    a3.label = "acct-3".into();
    b.rd.accounts[2] = a3;
    b.caps.max_timers += 64;
    let mut h = b.boot();
    h.steady();
    let place = |h: &mut H, orders: Vec<(AccountId, Side, &'static str)>| {
        let m = h.mark();
        set_hook(move |hx, ev, ctx| {
            if let hft_strategy_sdk::EventRef::Bbo { inst, .. } = ev
                && *inst == A1
                && hx.s.strategy == S1
            {
                for (a, side, p) in orders.iter() {
                    let mut it = intent(
                        hx,
                        A1,
                        *side,
                        hft_types::Tif::Gtc,
                        hft_strategy_sdk::IntentClass::Open,
                        px(p),
                        q("0.001"),
                    );
                    it.account = *a;
                    let _ = ctx.place(it);
                }
            }
        });
        h.quote(A1, "50000", "50002");
        h.run();
        clear_hook();
        decisions(&h.since(m))
    };
    // .1 account 1 rests a sell at 50 100 (acked).
    let ds = place(&mut h, vec![(AC1, Side::Sell, "50100")]);
    assert_eq!(ds.len(), 1, "SEQ-452.1 {ds:?}");
    assert_eq!(ds[0].1.reject & RejectBits::SELF_TRADE.0, 0, "SEQ-452.1");
    let rest = ds[0].1;
    h.ack(&rest);
    // .2 account 3 buys at 50 100 (would cross): SELF_TRADE; at 50 050 (below): not.
    let ds = place(
        &mut h,
        vec![
            (AccountId(3), Side::Buy, "50100"),
            (AccountId(3), Side::Buy, "50050"),
        ],
    );
    assert_eq!(ds.len(), 2, "SEQ-452.2 {ds:?}");
    assert_eq!(ds[0].1.reject, RejectBits::SELF_TRADE.0, "SEQ-452.2 {ds:?}");
    assert_eq!(ds[1].1.reject & RejectBits::SELF_TRADE.0, 0, "SEQ-452.2 {ds:?}");
    // .3 the same account crossing its own resting sell: not a self-trade check (single
    // account of the order book side).
    let ds = place(&mut h, vec![(AC1, Side::Buy, "50100")]);
    assert_eq!(ds[0].1.reject & RejectBits::SELF_TRADE.0, 0, "SEQ-452.3 {ds:?}");
}
