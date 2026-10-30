//! Modes and caps (TESTS.md "模式与 cap 的驱动": SEQ-197–SEQ-213).

use hft_event::ControlCmd;
use hft_risk::mode::Effective;
use hft_sequencer::driver::live::RingId;
use hft_types::{AccountId, GroupId, KillLevel, RunMode, Side, StrategyId};

use crate::common::*;

fn steady() -> H {
    reset_script();
    let mut h = b0().boot();
    h.steady();
    h
}

fn kill(a: AccountId, hard: bool) -> ControlCmd<'static> {
    ControlCmd::Kill {
        scope: scope(StrategyId(0), a, GroupId(0)),
        level: KillLevel::Account,
        flatten: false,
        cancel_resting: true,
        hard,
    }
}

fn trace_args(h: &H, what: &str) -> Vec<(u64, u64)> {
    h.s.engine()
        .trace
        .ev
        .iter()
        .filter(|e| e.what == what)
        .map(|e| (e.a, e.b))
        .collect()
}

#[test]
fn seq_198_cells_changes() {
    let mut h = steady();
    clear_log();
    h.trace_on();
    h.cmd(
        AC1,
        &ControlCmd::SetMode {
            scope: scope(S1, AC1, G1),
            mode: RunMode::Shadow,
        },
    );
    h.run();
    let m: Vec<Got> = log()
        .into_iter()
        .filter(|g| g.kind == "Mode" && g.s == S1 && !g.model)
        .collect();
    assert!(
        m.iter()
            .any(|g| g.text.contains("old: Live") && g.text.contains("new: Shadow")),
        "{m:?}"
    );
    assert_eq!(trace_args(&h, "ledger.set_cell_shadow"), vec![(1, 1)]);
    clear_log();
    h.trace_on();
    h.cmd_confirmed(
        AC1,
        &ControlCmd::SetMode {
            scope: scope(S1, AC1, G1),
            mode: RunMode::Live,
        },
    );
    h.run();
    assert_eq!(trace_args(&h, "ledger.set_cell_shadow"), vec![(1, 0)]);
    clear_log();
    h.trace_on();
    h.cmd(AC1, &kill(AC1, false));
    h.run();
    let m: Vec<Got> = log()
        .into_iter()
        .filter(|g| g.kind == "Mode" && g.s == S1 && !g.model)
        .collect();
    assert!(m.iter().any(|g| g.text.contains("new: CloseOnly")), "{m:?}");
    assert!(trace_args(&h, "ledger.set_cell_shadow").is_empty());
}

#[test]
fn seq_199_shadow_deferred_notes() {
    let mut h = steady();
    h.open_position(AC1, A1, Side::Buy, "0.010");
    let m = h.mark();
    let id = h.cmd(
        AC1,
        &ControlCmd::SetMode {
            scope: scope(S1, AC1, G1),
            mode: RunMode::Shadow,
        },
    );
    h.run();
    let r = h.result_of(id).unwrap();
    assert_eq!((r.status, r.detail), (0, 1));
    assert_eq!(
        h.mode(S1, AC1, G1),
        Effective::CloseOnly {
            deferred_shadow: true
        }
    );
    let n = h
        .notes_since(m)
        .into_iter()
        .find(|n| n.code == hft_sequencer::codes::seq_note(8))
        .expect("ShadowDeferred");
    assert_eq!(n.v[0], 1 | (1 << 8));
    assert_eq!(n.v[1], u64::from(G1.0));
    assert_eq!(n.v[2], 0);
    // 2. the engine's reduce IOC ends unfilled, then the venue delivers the position
    // (settlement: `ExternalFill{Delivery}` takes a1 to 0): the cell lands in Shadow in that Input.
    for d in risk_kit::decs_since(&h, m) {
        risk_kit::expire_order(&mut h, &d);
    }
    assert_eq!(
        h.mode(S1, AC1, G1),
        Effective::CloseOnly {
            deferred_shadow: true
        }
    );
    let m = h.mark();
    h.push(
        RingId::Rep(0),
        &risk_kit::external_fill(
            &h,
            AC1,
            A1,
            Side::Sell,
            "0.010",
            hft_event::ExternalCause::Delivery,
        ),
    );
    h.run();
    assert_eq!(h.mode(S1, AC1, G1), Effective::Shadow, "SEQ-199 step 2");
    let n = h
        .notes_since(m)
        .into_iter()
        .find(|n| n.code == hft_sequencer::codes::seq_note(8))
        .expect("ShadowDeferred on landing");
    assert_eq!((n.v[1], n.v[2]), (u64::from(G1.0), 1), "SEQ-199 step 2");
}

#[test]
fn seq_200_cancels() {
    // .1 SetMode CloseOnly → cancel_open of the cell.
    let mut h = steady();
    h.trace_on();
    h.cmd(
        AC1,
        &ControlCmd::SetMode {
            scope: scope(S1, AC1, G1),
            mode: RunMode::CloseOnly,
        },
    );
    h.run();
    assert_eq!(trace_args(&h, "oms.cancel_open"), vec![(1, u64::from(G1.0))]);
    // .2 Kill (not HARD) → cancel_all open_only = true; .3 HARD → false.
    let mut h = steady();
    h.trace_on();
    h.cmd(AC1, &kill(AC1, false));
    h.run();
    assert!(
        trace_args(&h, "oms.cancel_all").iter().all(|x| x.0 == 1)
            && !trace_args(&h, "oms.cancel_all").is_empty()
    );
    let mut h = steady();
    h.trace_on();
    h.cmd_confirmed(AC1, &kill(AC1, true));
    h.run();
    assert!(trace_args(&h, "oms.cancel_all").iter().any(|x| x.0 == 0));
}

#[test]
fn seq_201_kill_words() {
    let mut h = steady();
    let k0 = h.io_ref().kills.len();
    h.cmd(AC1, &kill(AC1, false));
    h.run();
    let kills = h.io_ref().kills.clone();
    assert!(kills.len() > k0);
    assert_eq!(
        kills.last().unwrap().as_slice(),
        h.s.engine().risk.mode.kill_bits()
    );
    // Sim account: words unchanged.
    let k1 = h.io_ref().kills.clone();
    h.cmd(AC9, &kill(AC9, false));
    h.run();
    assert_eq!(h.io_ref().kills.last(), k1.last());
    // Verify writes no words and replays byte for byte.
    let m = h.verify();
    assert!(m.is_empty(), "{m:?}");
}

#[test]
fn seq_208_risk_events() {
    let mut h = steady();
    clear_log();
    h.trace_on();
    h.cmd(AC2, &kill(AC2, false));
    h.run();
    let r: Vec<Got> = log()
        .into_iter()
        .filter(|g| g.kind == "Risk" && !g.model)
        .collect();
    assert!(r.iter().any(|g| g.s == S2));
    assert!(r.iter().all(|g| g.s != S1));
    assert!(h.s.engine().trace.count("exec.on_risk") >= 1);
    assert!(h.s.engine().trace.count("exec.on_mode") >= 1);
}

#[test]
fn seq_211_venue_notice_delivery() {
    let mut h = steady();
    clear_log();
    h.cmd_confirmed(
        AC1,
        &ControlCmd::VenueNotice {
            venue: B,
            kind: 1,
            _pad: 0,
            inst: B1,
            a: UTC0 + 3_600_000_000_000,
            b: 0,
        },
    );
    h.run();
    let caps = h.s.engine().risk.mode.caps();
    assert!(caps.iter().any(|c| c.source == hft_types::CapSource::DELISTING));
    let notes: Vec<Got> = log()
        .into_iter()
        .filter(|g| g.kind == "Notice" && !g.model)
        .collect();
    assert!(notes.iter().any(|g| g.s == S1));
    let kf = h.keyframe();
    let (_, secs) = snap(&kf);
    assert!(
        secs.notices
            .iter()
            .any(|r| r.venue == B && r.kind == 1 && r.inst == B1)
    );
}

#[test]
fn seq_213_cancel_obligation() {
    // .1 Kill (not HARD, cancel_resting): reissue open_only on ConnState Up.
    let mut h = steady();
    h.cmd(AC1, &kill(AC1, false));
    h.run();
    h.trace_on();
    let t = h.now();
    h.push(
        RingId::Rep(0),
        &conn(
            B,
            0,
            hft_event::ConnStateKind::Up,
            hft_event::ConnRole::Order,
            hft_types::InstId(0),
            AC1,
            t,
        ),
    );
    h.run();
    assert_eq!(trace_args(&h, "oms.reissue_cancels"), vec![(1, 1)]);
    // .5 no cap: nothing.
    let mut h = steady();
    h.trace_on();
    let t = h.now();
    h.push(
        RingId::Rep(0),
        &conn(
            B,
            0,
            hft_event::ConnStateKind::Up,
            hft_event::ConnRole::Order,
            hft_types::InstId(0),
            AC1,
            t,
        ),
    );
    h.run();
    assert!(trace_args(&h, "oms.reissue_cancels").is_empty());
}

// ===================================================================================================
// SEQ-202, SEQ-203 (resolve_broken on HARD release), SEQ-353 (UnknownTimeout cap), SEQ-368
// (exchange rate-limit hit). Shared helpers: tests/common/risk_kit.rs.
// ===================================================================================================

mod resolve {
    use hft_event::{ConnRole, ConnStateKind, OrderDecision};
    use hft_sequencer::codes::{detail, status};
    use hft_sequencer::driver::live::RingId;
    use hft_strategy_sdk::{IntentClass, LinkRole};
    use hft_types::{CapSource, InstId, RejectBits, Side};

    use crate::common::risk_kit::*;
    use crate::common::*;

    fn steady() -> H {
        reset_script();
        let mut h = b0().boot();
        h.steady();
        h
    }

    fn fills_under_hard(h: &mut H, legs: &[(InstId, Side, &str)]) -> Vec<OrderDecision> {
        naked_under_hard(h, S2, G2, legs)
    }

    /// `Resume{Kill, Account(2)}` with the trace on; returns the mark before it.
    fn resume_hard(h: &mut H) -> usize {
        clean_recon(h, AC2);
        h.trace_on();
        let m = h.mark();
        let r = run_cmd(h, AC2, &resume_account(AC2, CapSource::KILL));
        assert_eq!(r.0, status::APPLIED, "{r:?}");
        m
    }

    fn broken_g2(h: &H) -> bool {
        cap_on(h, CapSource::BROKEN, AC2, Some(G2))
    }

    /// No order and no HedgeFix decision since `m` (SEQ-202.6).
    fn no_orders(h: &H, m: usize, row: &str) {
        assert!(decs_since(h, m).is_empty(), "{row}: {:?}", decs_since(h, m));
        assert!(
            exec_decs_since(h, m)
                .iter()
                .all(|x| x.action != hft_exec::decision::action::HEDGE_FIX),
            "{row}: HedgeFix decision"
        );
    }

    #[test]
    fn seq_202_resolve_broken_on_hard_release() {
        // .1 no link, naked a1 0.006 (100 % of the largest leg, ≈ 300 USDT) → Broken.
        let mut h = steady();
        fills_under_hard(&mut h, &[(A1, Side::Buy, "0.006")]);
        assert!(!broken_g2(&h));
        let m = resume_hard(&mut h);
        let t = h.s.engine().trace.clone();
        let rb = t.pos("exec.resolve_broken").expect("resolve_broken called");
        let ev = &t.ev[rb];
        assert_eq!((ev.a, ev.b), (u64::from(AC2.0), u64::from(G2.0)));
        let ap = t.pos("apply.Broken").expect("apply(Auto(Broken))");
        assert!(ap > rb, "{:?}", t.names());
        let legs =
            t.ev.iter()
                .enumerate()
                .skip(ap)
                .find(|(_, e)| e.what == "risk.on_legs" && e.a == u64::from(AC2.0) && e.b == u64::from(G2.0));
        assert!(legs.is_some(), "risk.on_legs(2, g2) after apply: {:?}", t.names());
        assert!(broken_g2(&h), "SEQ-202.1 Broken{{2, g2}}");
        assert!(
            cap_notes(&h, m, true)
                .iter()
                .any(|c| c.source == CapSource::BROKEN && c.scope.group == G2)
        );
        no_orders(&h, m, "SEQ-202.1");

        // .2 within tolerance: a1 +0.1, b1 −0.099 (1 % ≤ 2 %, ≈ 50 USDT).
        let mut h = steady();
        fills_under_hard(&mut h, &[(A1, Side::Buy, "0.1"), (B1, Side::Sell, "0.099")]);
        let m = resume_hard(&mut h);
        assert_eq!(h.s.engine().trace.count("exec.resolve_broken"), 1);
        assert!(!broken_g2(&h), "SEQ-202.2");
        no_orders(&h, m, "SEQ-202.2");

        // .3 at most the dust line: a1 +0.0001 at mid 49 990.5 ≈ 4.999 USDT ≤ 5 USDT.
        let mut h = steady();
        h.quote(A1, "49990", "49991");
        h.run();
        fills_under_hard(&mut h, &[(A1, Side::Buy, "0.0001")]);
        let m = resume_hard(&mut h);
        assert_eq!(h.s.engine().trace.count("exec.resolve_broken"), 1);
        assert!(!broken_g2(&h), "SEQ-202.3");
        no_orders(&h, m, "SEQ-202.3");

        // .4 manual hold: a Manual external sell of 0.001 a1 on g2 (naked 0.005 remains).
        let mut h = steady();
        fills_under_hard(&mut h, &[(A1, Side::Buy, "0.006")]);
        let mh = h.mark();
        h.push(RingId::Rep(0), &manual_fill(&h, AC2, A1, Side::Sell, "0.001"));
        h.run();
        assert_eq!(
            exec_notes(&h, mh, hft_exec::decision::note::EXEC_MANUAL_HOLD).len(),
            1,
            "manual hold set"
        );
        let m = resume_hard(&mut h);
        assert_eq!(h.s.engine().trace.count("exec.resolve_broken"), 1);
        assert!(!broken_g2(&h), "SEQ-202.4");
        no_orders(&h, m, "SEQ-202.4");

        // .5 a Hedging link (second leg gated by HARD): left to the OMS deadline.
        let mut h = steady();
        let mo = place_with(&mut h, S2, |hx| pair_first(hx, q("0.01")));
        let first = decs_since(&h, mo)
            .into_iter()
            .find(|d| d.account == AC2 && d.inst == A1)
            .expect("first leg");
        assert_eq!(first.reject, 0);
        h.ack(&first);
        let r = run_cmd(&mut h, AC2, &kill_account(AC2, true, false, false));
        assert_eq!(r.0, status::APPLIED);
        let mf = h.mark();
        h.fill(&first, first.qty, first.qty, first.px);
        let second = decs_since(&h, mf)
            .into_iter()
            .find(|d| d.inst == B1)
            .expect("second leg decision");
        assert_eq!(second.reject, RejectBits::HARD.0, "{second:?}");
        assert!(h.s.engine().oms.legs(AC2, G2).view.open_links > 0, "Hedging");
        let m = resume_hard(&mut h);
        assert!(!broken_g2(&h), "SEQ-202.5");
        assert!(
            decs_since(&h, m).iter().all(|d| !is_role(d, LinkRole::HedgeFix)),
            "SEQ-202.5/.6"
        );
        assert!(
            exec_decs_since(&h, m)
                .iter()
                .all(|x| x.action != hft_exec::decision::action::HEDGE_FIX)
        );
    }

    #[test]
    fn seq_203_resolve_broken_only_hard() {
        let naked_scans = |h: &H| h.s.engine().trace.count("oms.naked_groups");
        // .1 a non-HARD Kill on account 2 released while g2 is naked.
        let mut h = steady();
        let d = place_in(&mut h, S2, G2, A1, Side::Buy, q("0.006"), IntentClass::Open);
        h.ack(&d);
        let r = run_cmd(&mut h, AC2, &kill_account(AC2, false, false, false));
        assert_eq!(r.0, status::APPLIED);
        h.fill(&d, d.qty, d.qty, d.px);
        h.trace_on();
        let m = h.mark();
        let r = run_cmd(&mut h, AC2, &resume_account(AC2, CapSource::KILL));
        assert_eq!(r.0, status::APPLIED, "{r:?}");
        assert_eq!(naked_scans(&h), 0, "SEQ-203.1");
        assert!(
            cap_notes(&h, m, true)
                .iter()
                .all(|c| c.source != CapSource::BROKEN)
        );

        // .2 Resume{DailyLoss}: account 2's quota 1 USDT, a1 0.006 long marked down 300 USDT/BTC.
        reset_script();
        let mut b = b0();
        let rd = b.rd.load();
        let mut p = b0_risk(&rd);
        for a in p.accounts.as_mut_slice() {
            if a.account == AC2 {
                a.daily_loss_abs = n("1");
            }
        }
        b.risk = Some(p);
        let mut h = b.boot();
        h.steady();
        let d = place_in(&mut h, S2, G2, A1, Side::Buy, q("0.006"), IntentClass::Open);
        h.ack(&d);
        h.fill(&d, d.qty, d.qty, d.px);
        h.quote(A1, "49700", "49701");
        h.run();
        assert!(cap_on(&h, CapSource::DAILY_LOSS, AC2, None), "DailyLoss set");
        h.trace_on();
        let m = h.mark();
        let r = run_cmd(&mut h, AC2, &resume_account(AC2, CapSource::DAILY_LOSS));
        assert_eq!(r.0, status::APPLIED, "{r:?}");
        assert_eq!(naked_scans(&h), 0, "SEQ-203.2");
        assert!(
            cap_notes(&h, m, true)
                .iter()
                .all(|c| c.source != CapSource::BROKEN)
        );

        // .3 Recovery released by the startup gate after a clean-Drain restart (g2 naked from
        // before, the HARD Kill kept).
        let mut h = steady();
        fills_under_hard(&mut h, &[(A1, Side::Buy, "0.006")]);
        h.cmd(
            hft_types::AccountId(0),
            &hft_event::ControlCmd::Drain { cancel_resting: true },
        );
        assert!(
            matches!(h.run(), hft_sequencer::StepOutcome::Done { .. }),
            "Drain done"
        );
        let mut h = H::crash_restart(
            h,
            plan(hft_sequencer::SnapshotSel::Latest, true, 1, 0),
            Rerun::default(),
        )
        .unwrap();
        assert!(cap_on(&h, CapSource::RECOVERY, AC2, None));
        h.trace_on();
        let m = h.mark();
        h.bring_up(false);
        clean_recon(&mut h, AC2);
        assert!(!cap_on(&h, CapSource::RECOVERY, AC2, None), "Recovery released");
        assert_eq!(naked_scans(&h), 0, "SEQ-203.3");
        assert!(
            cap_notes(&h, m, true)
                .iter()
                .all(|c| c.source != CapSource::BROKEN)
        );

        // .4 the HARD record deleted → oms.naked_groups.
        let mut h = steady();
        fills_under_hard(&mut h, &[(A1, Side::Buy, "0.006")]);
        resume_hard(&mut h);
        assert!(naked_scans(&h) >= 1, "SEQ-203.4");
    }

    // ------------------------------------------------------------------------------- SEQ-353

    /// A one-leg b1 sell IOC of s1 is sent (`SendReport{Place, Written}`); at the next periodic
    /// sweep's deadline (B, 1)'s order connections and user stream drop. The slot is `Unknown`
    /// from the returned TSC and the sweeps fall at whole seconds after it.
    fn unknown_on_b(h: &mut H) -> u64 {
        let m = h.mark();
        let d = place_in(h, S1, G1, B1, Side::Sell, q("0.001"), IntentClass::Open);
        assert_eq!(d.reject, 0);
        let seq = decisions(&h.since(m))
            .into_iter()
            .find(|x| x.1.inst == B1)
            .map(|x| x.0)
            .expect("decision seq");
        let t = h.now();
        h.push(RingId::Rep(0), &sent(&d, hft_event::GwOp::Place, seq, t));
        h.run();
        let due = sweep_due(h);
        h.at(due);
        for k in [0u8, 1] {
            h.push(
                RingId::Rep(0),
                &conn(B, k, ConnStateKind::Down, ConnRole::Order, InstId(0), AC1, due),
            );
        }
        h.push(
            RingId::Rep(0),
            &conn(
                B,
                0,
                ConnStateKind::Down,
                ConnRole::UserStream,
                InstId(0),
                AC1,
                due,
            ),
        );
        h.run();
        let r = hft_strategy_sdk::OrderRef::new(d.venue, d.account, d.slot.0, d.r#gen);
        assert_eq!(
            h.s.engine().oms.slot(r).map(|s| s.state),
            Some(hft_oms::OrderState::Unknown)
        );
        due
    }

    fn sweep_due(h: &H) -> u64 {
        h.s.engine()
            .heap
            .sorted()
            .iter()
            .find(|e| e.owner == 3 && e.timer_id == 2)
            .map(|e| e.deadline)
            .expect("OmsSweep in the heap")
    }

    /// Fires every sweep due strictly before `t`, each at its exact deadline.
    fn sweeps_before(h: &mut H, t: u64) {
        loop {
            let d = sweep_due(h);
            if d >= t {
                return;
            }
            h.at(d);
            h.run();
        }
    }

    fn is_p0_unknown(n: &hft_event::Note) -> bool {
        n.code == hft_oms::notes::code_of(hft_oms::notes::UNKNOWN) && n.level == hft_event::NoteLevel::P0
    }

    fn counts(h: &H, m: usize) -> (usize, usize) {
        let p0 = h.notes_since(m).iter().filter(|n| is_p0_unknown(n)).count();
        let caps = cap_notes(h, m, true)
            .iter()
            .filter(|c| {
                c.source == CapSource::UNKNOWN_TIMEOUT && c.scope.account == AC1 && c.scope.venue == B
            })
            .count();
        (p0, caps)
    }

    #[test]
    fn seq_353_unknown_timeout_cap() {
        // .2 the sweep at t + 30 000 ms: not past unknown_alarm_ms.
        let mut h = steady();
        let t = unknown_on_b(&mut h);
        let m = h.mark();
        sweeps_before(&mut h, t + 30_000 * MS);
        assert_eq!(sweep_due(&h), t + 30_000 * MS, "a sweep lands on the boundary");
        h.at(t + 30_000 * MS);
        h.run();
        assert_eq!(counts(&h, m), (0, 0), "SEQ-353.2");

        // .1 the sweep at t + 30 001 ms: P0 OmsUnknown and the cap in the same Input.
        let mut h = steady();
        let t = unknown_on_b(&mut h);
        sweeps_before(&mut h, t + 30_000 * MS);
        let m = h.mark();
        h.at(t + 30_001 * MS);
        h.run();
        assert_eq!(counts(&h, m), (1, 1), "SEQ-353.1");
        let recs = h.since(m);
        let a = note_pos(&recs, is_p0_unknown).and_then(|k| input_of(&recs, k));
        let b = note_pos(&recs, |n| is_cap_note(n, true, CapSource::UNKNOWN_TIMEOUT))
            .and_then(|k| input_of(&recs, k));
        assert!(a.is_some() && a == b, "same Input: {a:?} {b:?}");

        // .3 one more sweep: no second CapSet (the OMS escalates once).
        let m = h.mark();
        let d = sweep_due(&h);
        h.at(d);
        h.run();
        assert_eq!(counts(&h, m), (0, 0), "SEQ-353.3");

        // .4 Resume{UnknownTimeout, 1, B} while the slot is still Unknown.
        let v0 = h.s.engine().risk.mode.ctl_state_version();
        let r = run_cmd(
            &mut h,
            AC1,
            &hft_event::ControlCmd::Resume {
                scope: scope_av(AC1, B),
                source: CapSource::UNKNOWN_TIMEOUT,
            },
        );
        assert_eq!(
            r,
            (status::PRECONDITION_FAILED, u32::from(detail::UNKNOWN_PRESENT)),
            "SEQ-353.4"
        );
        assert_eq!(h.s.engine().risk.mode.ctl_state_version(), v0);
        assert!(cap_on(&h, CapSource::UNKNOWN_TIMEOUT, AC1, None));
    }

    // ------------------------------------------------------------------------------- SEQ-368

    #[test]
    fn seq_368_rate_limit_hit_note() {
        let mut h = steady();
        h.trace_on();
        let m = h.mark();
        let t = h.now();
        let mut cs = conn(A, 0, ConnStateKind::Degraded, ConnRole::Order, InstId(0), AC1, t);
        cs.reason = 11; // RATE_LIMIT
        h.push(RingId::Rep(0), &cs);
        h.run();
        let hits: Vec<_> =
            h.s.engine()
                .trace
                .ev
                .iter()
                .filter(|e| e.what == "risk.on_rate_limit_hit")
                .map(|e| (e.a, e.b))
                .collect();
        assert_eq!(hits, vec![(u64::from(A.0), u64::from(AC1.0))]);
        let is_hit = |n: &hft_event::Note| n.source == hft_event::NoteSource::Risk && n.code & 0xFFFF == 1;
        assert_eq!(
            h.notes_since(m).iter().filter(|n| is_hit(n)).count(),
            1,
            "one RateLimitHit"
        );
        let recs = h.since(m);
        let k = note_pos(&recs, is_hit).expect("note");
        let input = input_of(&recs, k)
            .and_then(|s| recs.iter().find(|r| r.seq == s))
            .map(|r| r.kind);
        assert_eq!(
            input,
            Some(hft_event::EventKind::CONN_STATE),
            "in the ConnState Input"
        );
        // (A, 1) Opens are RATE_BACKOFF afterwards.
        let d = place_in(&mut h, S1, G1, A1, Side::Buy, q("0.001"), IntentClass::Open);
        assert_eq!(d.reject, RejectBits::RATE_BACKOFF.0);
    }
}
