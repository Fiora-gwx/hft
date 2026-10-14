//! Target intents and the executor (TESTS.md "目标意图与执行层": SEQ-182–SEQ-187).

use hft_event::{ControlCmd, EventKind};
use hft_strategy_sdk::{EventRef, ExecSpec};
use hft_types::RunMode;

use crate::common::*;

fn steady() -> H {
    reset_script();
    let mut h = b0().boot();
    h.steady();
    h
}

fn exec_decisions(h: &H, m: usize) -> Vec<hft_event::ExecDecision> {
    h.since(m)
        .iter()
        .filter(|r| r.kind == EventKind::EXEC_DECISION)
        .filter_map(|r| hft_sequencer::types::read_body::<hft_event::ExecDecision>(&r.body))
        .collect()
}

#[test]
fn seq_182_target_owner() {
    // .1 own account: accepted, advanced in the same Input.
    let mut h = steady();
    h.trace_on();
    let m = h.mark();
    h.set_target_s2(target(
        AC2,
        G2,
        &[(0, q("0.01")), (1, q("-0.01"))],
        ExecSpec::PairOnFill(pair0()),
    ));
    let tr = h.trace();
    let st = tr
        .iter()
        .position(|x| *x == "exec.set_target")
        .expect("set_target");
    let adv = tr.iter().rposition(|x| *x == "exec.advance").expect("advance");
    assert!(st < adv);
    assert!(!exec_decisions(&h, m).is_empty());
    assert!(h.s.engine().exec.cell(AC2, G2).is_some_and(|c| c.has_target));
    // .2 another instance's account: rejected, no slice.
    clear_log();
    let m = h.mark();
    h.set_target_s2(target(AC1, G1, &[(0, q("0.01"))], ExecSpec::PairOnFill(pair0())));
    let xd = exec_decisions(&h, m);
    assert!(!xd.is_empty());
    assert!(h.s.engine().exec.cell(AC1, G1).is_none_or(|c| !c.has_target));
    assert!(decisions(&h.since(m)).iter().all(|d| d.1.account != AC1));
    // .3 clear_target: positions untouched.
    h.trace_on();
    set_hook(|hx, ev, ctx| {
        if let EventRef::Bbo { inst, .. } = ev
            && *inst == A1
            && hx.s.strategy == S2
        {
            let _ = ctx.clear_target(AC2, G2);
        }
    });
    h.quote(A1, "50000", "50007");
    h.run();
    clear_hook();
    assert!(h.trace().contains(&"exec.clear_target"));
}

#[test]
fn seq_183_on_mode() {
    let mut h = steady();
    clear_log();
    h.trace_on();
    h.cmd(
        AC2,
        &ControlCmd::SetMode {
            scope: scope(S2, AC2, G2),
            mode: RunMode::CloseOnly,
        },
    );
    h.run();
    assert_eq!(h.s.engine().trace.count("exec.on_mode"), 1);
    let ev: Vec<Got> = log()
        .into_iter()
        .filter(|g| g.kind == "Mode" && g.s == S2 && !g.model)
        .collect();
    assert_eq!(ev.len(), 1);
    assert!(
        ev[0].text.contains("old: Live") && ev[0].text.contains("new: CloseOnly"),
        "{}",
        ev[0].text
    );
}

#[test]
fn seq_187_draining_no_increase() {
    let mut h = steady();
    h.set_target_s2(target(
        AC2,
        G2,
        &[(0, q("0.05")), (1, q("-0.05"))],
        ExecSpec::PairOnFill(pair0()),
    ));
    h.cmd_confirmed(
        hft_types::AccountId(0),
        &ControlCmd::Drain {
            cancel_resting: false,
        },
    );
    h.run();
    let m = h.mark();
    for k in 0..10u64 {
        h.adv(1_000_000_000);
        h.quote(A1, "50000", if k % 2 == 0 { "50003" } else { "50004" });
        h.run();
    }
    let ds = decisions(&h.since(m));
    assert!(
        ds.iter()
            .all(|d| d.1.account != AC2 || d.1.reject != 0 || d.1.action != hft_event::OrderAction::New)
    );
}

// ===================================================================================================
// SEQ-185: Kill{Account, flatten} → engine_flatten covering a Broken group (helpers:
// tests/common/risk_kit.rs).
// ===================================================================================================

mod flatten_broken {
    use hft_event::OrderFlags;
    use hft_sequencer::codes::status;
    use hft_types::{CapSource, Side};

    use crate::common::risk_kit::*;
    use crate::common::*;

    #[test]
    fn seq_185_engine_flatten_covers_broken() {
        reset_script();
        let mut h = b0().boot();
        h.steady();
        // g2 Broken: a1 +0.010, b1 −0.005 (naked 0.005 BTC ≈ 250 USDT).
        broken_by_release(
            &mut h,
            S2,
            G2,
            &[(A1, Side::Buy, "0.010"), (B1, Side::Sell, "0.005")],
        );
        h.trace_on();
        let n = h.io_ref().out.len();
        let m = h.mark();
        let v = h.s.engine().risk.mode.ctl_state_version();
        let id = h.cmd_with(
            "cli:trader1",
            TRADER,
            AC2,
            &kill_account(AC2, false, true, true),
            |c| {
                c.confirmed = 1;
                c.expect_state_version = v;
            },
        );
        h.run();
        assert_eq!(h.res(id).0, status::APPLIED, "{:?}", h.res(id));
        assert_eq!(
            h.s.engine().trace.count("exec.engine_flatten"),
            1,
            "{:?}",
            h.trace()
        );
        let ds = decs_since(&h, m);
        let perp: Vec<_> = ds.iter().filter(|d| d.inst == B1 && d.account == AC2).collect();
        assert_eq!(perp.len(), 1, "{ds:?}");
        let p = perp[0];
        assert_eq!((p.side, p.qty, p.reject), (Side::Buy, q("0.005"), 0), "{p:?}");
        assert!(p.oflags & OrderFlags::REDUCE_ONLY.0 != 0, "reduce-only perp leg");
        assert!(
            ds.iter().all(|d| d.inst != A1),
            "no spot leg before the perp leg reaches its target: {ds:?}"
        );
        assert!(
            frames(&h, n).iter().any(|f| f.1.inst == B1 && f.1.account == AC2),
            "perp leg out in the command's Input"
        );
        assert!(cap_on(&h, CapSource::BROKEN, AC2, Some(G2)), "still Broken");
        // The perp leg fills → the spot leg (a1 sell 0.010) follows.
        h.ack(p);
        let m = h.mark();
        h.fill(p, p.qty, p.qty, p.px);
        let t = h.now() + 2 * SEC;
        while h.now() < t {
            h.adv(100 * MS);
            md_alive(&mut h);
            requote(&mut h, A1);
            if decs_since(&h, m).iter().any(|d| d.inst == A1) {
                break;
            }
        }
        let ds = decs_since(&h, m);
        let spot = ds
            .iter()
            .find(|d| d.inst == A1 && d.account == AC2)
            .expect("spot leg");
        assert_eq!(
            (spot.side, spot.qty, spot.reject),
            (Side::Sell, q("0.010"), 0),
            "{spot:?}"
        );
    }

    fn frames(h: &H, n: usize) -> Vec<(hft_sequencer::OutRing, hft_event::GwCmd)> {
        h.io_ref().out[n..].to_vec()
    }
/// g1 Broken with the spot leg a1 +0.004 extra (every second leg expired); `HedgeFix{1, g1}`
/// goes through `exec.hedge_fix_cmd` once, its a1 sell 0.004 through the pipeline to the ring.
#[test]
fn seq_186_hedge_fix_cmd() {
    use crate::scenarios as sc;
    let mut h = sc::steady();
    let m = sc::place_on_a1(&mut h, &[S1], |hx| pair_first(hx, q("0.004")));
    let first = sc::decs(&h, m)[0];
    assert_eq!(first.reject, 0);
    h.ack(&first);
    let m0 = h.mark();
    h.fill(&first, first.qty, first.qty, first.px);
    for _ in 0..8 {
        if sc::has_cap(&h, hft_types::CapSource::BROKEN, AC1, Some(G1)) {
            break;
        }
        let open = sc::decs(&h, m0)
            .into_iter()
            .rev()
            .find(|d| d.inst == B1 && d.reject == 0)
            .expect("hedge leg");
        h.ack(&open);
        sc::expire(&mut h, &open);
        if let Some(due) = sc::hedge_retry_timers(&h).first().map(|t| t.0) {
            sc::run_to(&mut h, due);
        }
    }
    assert!(sc::has_cap(&h, hft_types::CapSource::BROKEN, AC1, Some(G1)));
    h.trace_on();
    let n = h.io_ref().out.len();
    let m = h.mark();
    let id = h.cmd_confirmed(
        AC1,
        &ControlCmd::HedgeFix {
            scope: scope(S1, AC1, G1),
        },
    );
    h.run();
    assert_eq!(h.s.engine().trace.count("exec.hedge_fix_cmd"), 1);
    assert_eq!(
        h.res(id).0,
        hft_sequencer::codes::status::APPLIED,
        "{:?}",
        h.res(id)
    );
    let d = sc::decs(&h, m);
    assert_eq!(d.len(), 1, "{d:?}");
    assert_eq!(
        (d[0].inst, d[0].side, d[0].qty, d[0].reject),
        (A1, hft_types::Side::Sell, q("0.004"), 0)
    );
    let fr = sc::frames(&h, n);
    assert_eq!(fr.len(), 1);
    assert_eq!((fr[0].1.inst, fr[0].1.qty), (A1, q("0.004")));
}
