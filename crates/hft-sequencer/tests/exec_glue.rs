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
