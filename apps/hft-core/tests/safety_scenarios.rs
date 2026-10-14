//! C6/C15 on the existing B0 real-sequencer fixture. The fixture supplies deterministic
//! strategy intent hooks; the app owns the cross-component acceptance, not engine code.
#[path = "../../../crates/hft-sequencer/tests/common/mod.rs"]
mod common;
use common::{risk_kit::*, *};
use hft_event::{ConnRole, ConnStateKind, ControlCmd, EventKind, LeverageItem};
use hft_sequencer::{SnapshotSel, StepOutcome, driver::live::RingId};
use hft_strategy_sdk::IntentClass;
use hft_types::{AccountId, CapSource, GroupId, InstId, RejectBits, Side, StrategyId};
use zerocopy::IntoBytes;

#[test]
fn c15_funding_period_reaches_stats_in_same_delivery() {
    use hft_strat_lib::{FundingObs, FundingStats, funding_flags};
    reset_script();
    SCRIPT.with(|s| {
        s.borrow_mut().subs.push((
            S1,
            vec![(
                B1,
                0,
                hft_strategy_sdk::EvMask::FUNDING,
                hft_strategy_sdk::Part::Fast,
            )],
        ))
    });
    let mut h = b0().boot_self_contained();
    h.steady();
    let got = cap();
    let captured = got.clone();
    let mut stats = FundingStats::new();
    set_hook(move |hx, ev, _| {
        if matches!(ev, hft_strategy_sdk::EventRef::Funding { inst, .. } if *inst == B1)
            && hx.s.strategy == StrategyId(1)
        {
            let view = hx.v.funding(B1);
            let now = hx.v.now().utc_ns;
            let obs = stats.observe(&view, now);
            captured
                .borrow_mut()
                .push((view, obs, stats.summary(86_400_000_000_000, now, 60_000_000_000)));
        }
    });
    let next = 1_800_000_000_000_000_000;
    h.funding_md(B1, "0.000100000", next, 28_800);
    h.funding_md(B1, "0.000200000", next, 3_600);
    let observed = got.borrow();
    assert_eq!(observed.len(), 2);
    assert_eq!(observed[1].0.prev_interval_s, 28_800);
    assert_eq!(observed[1].0.interval_s, 3_600);
    assert_eq!(
        observed[1].1,
        FundingObs::IntervalChanged {
            old_s: 28_800,
            new_s: 3_600
        }
    );
    assert_ne!(observed[1].2.flags & funding_flags::INTERVAL_CHANGED, 0);
    assert_eq!(observed[1].2.predicted_daily, r("0.004800000"));
    assert_eq!(
        observed[1].2.n_settles, 0,
        "a period change alone is not settlement"
    );
    drop(observed);
    assert!(h.verify_from(None).is_empty());
}

fn account_report(
    h: &mut H,
    venue: hft_types::VenueId,
    position: u8,
    margin: u8,
    leverage: Option<&str>,
    flags: u32,
) {
    let mut hdr = cfg_hdr(AC1, venue, 0);
    hdr.position_mode = position;
    hdr.margin_mode = margin;
    hdr.mismatches = flags;
    let mut items = Vec::new();
    if venue == B {
        if let Some(value) = leverage {
            items.push(LeverageItem {
                inst: B1,
                _pad: 0,
                leverage: r(value),
            });
        }
        items.push(LeverageItem {
            inst: B2,
            _pad: 0,
            leverage: r("3.00"),
        });
    }
    hdr.n_lev = items.len() as u16;
    let mut bytes = hdr.as_bytes().to_vec();
    for item in items {
        bytes.extend_from_slice(item.as_bytes());
    }
    h.push_bytes(RingId::Rep(0), EventKind::ACCOUNT_CONFIG_REPORT, bytes);
    h.run();
}

fn cold_after_drain() -> H {
    reset_script();
    let mut cfg = b0();
    let mut risk = b0_risk(&cfg.rd.load());
    risk.max_perp_leverage_x100 = 300;
    cfg.risk = Some(risk);
    let mut h = cfg.boot_self_contained();
    h.steady();
    h.cmd(AccountId(0), &ControlCmd::Drain { cancel_resting: true });
    assert!(matches!(h.run(), StepOutcome::Done { .. }));
    let mut h = h
        .crash_restart(plan(SnapshotSel::Latest, true, 2, 0), Rerun::default())
        .unwrap();
    h.run();
    let t = h.now();
    for v in [A, B] {
        h.push(
            RingId::Rep(0),
            &conn(v, 0, ConnStateKind::Up, ConnRole::Md, InstId(0), AccountId(0), t),
        );
        for i in [A1, B1] {
            if h.rd.inst(i).venue == v {
                h.push(
                    RingId::Rep(0),
                    &conn(v, 0, ConnStateKind::Up, ConnRole::Md, i, AccountId(0), t),
                );
            }
        }
        for (id, role) in [
            (0, ConnRole::Order),
            (1, ConnRole::Order),
            (0, ConnRole::UserStream),
        ] {
            h.push(
                RingId::Rep(0),
                &conn(v, id, ConnStateKind::Up, role, InstId(0), AC1, t),
            );
        }
    }
    h.run();
    h.push(RingId::Hk, &sys_timer(8, t));
    h.push(RingId::Hk, &sys_timer(16 | (u32::from(AC1.0) << 16), t));
    let view_seq = h.j().records().last().unwrap().seq;
    for (id, v) in [(99901, A), (99902, B)] {
        h.push(RingId::Rep(0), &recon_summary(AC1, v, id, view_seq));
    }
    account_report(&mut h, A, 1, 1, None, 0);
    h.quote(A1, "50000", "50001");
    h.quote(B1, "50010", "50011");
    h.run();
    h
}

#[test]
fn c6_account_cfg_gate_and_cap() {
    let mut h = cold_after_drain();
    let replay_start = h.keyframe().seq;
    account_report(&mut h, B, 3, 2, Some("3.00"), 0);
    assert_eq!(h.gate_bits(AC1) & hft_risk::mode::gate_bits::ACCOUNT_CFG, 0);
    assert!(!h.s.engine().risk.mode.released(AC1));
    assert!(!cap_on(&h, CapSource::ACCOUNT_CFG, AC1, None));
    account_report(&mut h, B, 2, 2, Some("3.00"), 0);
    assert_eq!(h.gate_bits(AC1), hft_risk::mode::gate_bits::ALL);
    assert!(h.s.engine().risk.mode.released(AC1));
    for (margin, leverage, expected) in [
        (3, Some("3.00"), Some(1)),
        (2, Some("2.99"), None),
        (2, Some("3.00"), None),
        (2, Some("3.01"), Some(2)),
        (2, None, Some(2)),
        (2, Some("3.00"), None),
    ] {
        account_report(&mut h, B, 2, margin, leverage, 0);
        let cap =
            h.s.engine()
                .risk
                .mode
                .caps()
                .iter()
                .find(|c| c.source == CapSource::ACCOUNT_CFG && c.scope.account == AC1 && c.scope.venue == B);
        assert_eq!(
            cap.map(|c| c.params[0]),
            expected,
            "margin={margin} leverage={leverage:?}"
        );
    }
    account_report(&mut h, A, 1, 1, None, hft_risk::mode::mismatch::AUTO_BORROW);
    assert!(cap_on(&h, CapSource::ACCOUNT_CFG, AC1, None));
    account_report(&mut h, A, 1, 1, None, 0);
    assert!(!cap_on(&h, CapSource::ACCOUNT_CFG, AC1, None));
    // Independent replay uses the recorded reports and recovered snapshot only.
    let mismatches = h.verify_from(Some(replay_start));
    assert!(mismatches.is_empty(), "{mismatches:?}");

    // C6.1/C6.2: test actual orders against the cap, with real fills providing
    // inventory and a naked spot leg for a legitimate hedge.
    reset_script();
    let mut h = b0().boot_self_contained();
    h.steady();
    let position = h.open_position(AC1, A1, Side::Buy, "0.02");
    assert_eq!(position.reject, 0);
    let initial_hedge = place_in(&mut h, S1, G1, B1, Side::Sell, q("0.02"), IntentClass::Hedge);
    assert_eq!(initial_hedge.reject, 0);
    h.ack(&initial_hedge);
    h.fill(
        &initial_hedge,
        initial_hedge.qty,
        initial_hedge.qty,
        initial_hedge.px,
    );
    account_report(&mut h, B, 2, 3, Some("3.00"), 0);
    assert_eq!(
        place_in(&mut h, S1, G1, B1, Side::Sell, q("0.001"), IntentClass::Open).reject,
        RejectBits::CLOSE_ONLY.0
    );
    let linked = place_with(&mut h, S1, |hx| pair_first(hx, q("0.001")));
    assert!(
        decs_since(&h, linked)
            .iter()
            .any(|d| d.inst == A1 && d.reject == RejectBits::CLOSE_ONLY.0)
    );
    let extra = place_in(&mut h, S1, G1, A1, Side::Buy, q("0.001"), IntentClass::Open);
    assert_eq!(extra.reject, 0);
    h.ack(&extra);
    h.fill(&extra, extra.qty, extra.qty, extra.px);
    let hedge = place_in(&mut h, S1, G1, B1, Side::Sell, q("0.001"), IntentClass::Hedge);
    assert_eq!(hedge.reject, 0);
    h.ack(&hedge);
    h.fill(&hedge, hedge.qty, hedge.qty, hedge.px);
    let reduce = place_in(&mut h, S1, G1, B1, Side::Buy, q("0.005"), IntentClass::Reduce);
    assert_eq!(reduce.reject, 0);
    expire_order(&mut h, &reduce);
    account_report(&mut h, B, 2, 2, Some("3.00"), 0);
    account_report(&mut h, A, 1, 1, None, hft_risk::mode::mismatch::AUTO_BORROW);
    assert_eq!(
        place_in(&mut h, S1, G1, A1, Side::Buy, q("0.001"), IntentClass::Open).reject,
        RejectBits::CLOSE_ONLY.0
    );

    let mut h = cold_after_drain();
    account_report(&mut h, B, 2, 2, Some("3.01"), 0);
    assert_eq!(h.gate_bits(AC1) & hft_risk::mode::gate_bits::ACCOUNT_CFG, 0);
    assert!(h.s.engine().risk.mode.hedge_gate_ok(AC1, B));
}

#[test]
fn c15_delisting_notice_blocks_open_on_inst() {
    reset_script();
    // B0's unused account slots are intentionally unbound and cannot pass a global
    // upgrade gate. This scenario has one real account, as required by W1.
    let mut config = b0();
    config.rd.accounts.truncate(1);
    config.rd.instances.truncate(1);
    config.params.truncate(1);
    let mut risk = b0_risk(&config.rd.load());
    risk.accounts = hft_risk::PreVec::from_vec(
        risk.accounts
            .iter()
            .filter(|row| row.account == AC1)
            .cloned()
            .collect(),
    );
    risk.venues = hft_risk::PreVec::from_vec(
        risk.venues
            .iter()
            .filter(|row| row.account == AC1)
            .cloned()
            .collect(),
    );
    config.risk = Some(risk);
    let mut h = config.boot_self_contained();
    h.steady();
    h.open_position(AC1, A1, Side::Buy, "0.02");
    let hedge = place_in(&mut h, S1, G1, B1, Side::Sell, q("0.02"), IntentClass::Hedge);
    assert_eq!(hedge.reject, 0);
    h.ack(&hedge);
    h.fill(&hedge, hedge.qty, hedge.qty, hedge.px);
    clear_hook();
    clear_log();
    let at = UTC0 + 7 * 86400 * 1_000_000_000;
    let mark = h.mark();
    h.cmd_confirmed(
        AC1,
        &ControlCmd::VenueNotice {
            venue: B,
            kind: 1,
            _pad: 0,
            inst: B1,
            a: at,
            b: 0,
        },
    );
    h.run();
    assert!(
        cap_notes(&h, mark, true)
            .iter()
            .any(|c| c.source == CapSource::DELISTING && c.scope.inst == B1 && c.params[0] == at as u64)
    );
    assert!(log().iter().any(|x| x.s == S1 && x.kind == "Notice" && !x.model));
    let blocked = place_in(&mut h, S1, G1, B1, Side::Sell, q("0.001"), IntentClass::Open);
    assert_eq!(blocked.reject, RejectBits::CLOSE_ONLY.0);
    let linked = place_with(&mut h, S1, |hx| pair_first(hx, q("0.001")));
    assert!(
        decs_since(&h, linked)
            .iter()
            .any(|d| d.inst == A1 && d.reject == RejectBits::CLOSE_ONLY.0)
    );
    let unrelated = place_in(&mut h, S1, G1, A1, Side::Buy, q("0.001"), IntentClass::Open);
    assert_eq!(unrelated.reject, 0);
    h.ack(&unrelated);
    h.fill(&unrelated, unrelated.qty, unrelated.qty, unrelated.px);
    let hedge = place_in(&mut h, S1, G1, B1, Side::Sell, q("0.001"), IntentClass::Hedge);
    assert_eq!(hedge.reject, 0);
    h.ack(&hedge);
    h.fill(&hedge, hedge.qty, hedge.qty, hedge.px);
    let reduce = place_in(&mut h, S1, G1, B1, Side::Buy, q("0.005"), IntentClass::Reduce);
    assert_eq!(reduce.reject, 0);
    expire_order(&mut h, &reduce);
    let mut scope = scope(StrategyId(0), AccountId(0), GroupId(0));
    scope.venue = B;
    scope.inst = B1;
    let id = h.cmd_confirmed(
        AC1,
        &ControlCmd::Resume {
            scope,
            source: CapSource::DELISTING,
        },
    );
    h.run();
    assert_eq!(h.result_of(id).unwrap().status, 0, "{:?}", h.result_of(id));
    assert_eq!(
        place_in(&mut h, S1, G1, B1, Side::Sell, q("0.001"), IntentClass::Open).reject,
        0
    );
}
