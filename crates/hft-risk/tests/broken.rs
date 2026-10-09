//! Broken, HedgeFailure, auto clear, resolve_broken and engine orders (RSK-050–RSK-056).
#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic
)] // allow-list: test code, not a hot path

mod common;
use common::*;

use hft_event::NoteLevel;
use hft_risk::mode::{CancelScope, Effective, ModeErr, ModeInput, RecoveryFlags};
use hft_risk::{ExecBounds, Origin};
use hft_strategy_sdk::RiskEvent;
use hft_types::{AccountId, CapSource, GROUP_ALL, InstId, RejectBits, Side, Tif};

fn open_a1() -> hft_risk::IntentView {
    open(1, G1, A1, Side::Buy, "0.01", "50000.00", Tif::Gtc)
}

fn auto_clear_broken(a: u8, g: hft_types::GroupId) -> ModeInput<'static> {
    ModeInput::AutoClear {
        source: CapSource::BROKEN,
        scope: sc_ag(a, g),
    }
}

#[test]
fn rsk_050_broken_sets_hedge_failure() {
    let mut w = r0();
    let v = w.v0;
    assert_eq!(w.apply_seq(broken(1, G1), 1_000), Ok(()));
    let b = w.cap(CapSource::BROKEN, sc_ag(1, G1)).expect("Broken");
    let h = w.cap(CapSource::HEDGE_FAILURE, sc_acct(1)).expect("HedgeFailure");
    assert_eq!((b.set_seq, h.set_seq), (1_000, 1_000));
    assert_eq!(h.scope.group, GROUP_ALL);
    let mut cells: Vec<_> = w.out.cells.iter().copied().collect();
    cells.sort_by_key(|c| c.0);
    let close = Effective::CloseOnly {
        deferred_shadow: false,
    };
    assert_eq!(
        cells,
        vec![
            (cell(1, 1, G1), Effective::Live, close),
            (cell(1, 1, G3), Effective::Live, close)
        ]
    );
    assert_eq!(w.out.cancels.len(), 1);
    let c = w.out.cancels.as_slice()[0];
    assert!(c.open_only);
    let acct_wide = matches!(c.scope, CancelScope::Account(AccountId(1)))
        || matches!(c.scope, CancelScope::Cell(k) if k.account == acc(1) && k.group == GROUP_ALL && k.strategy.0 == 0);
    assert!(acct_wide, "cancel scope {:?}", c.scope);
    let ev: Vec<_> = w.out.risk_events.iter().copied().collect();
    assert_eq!(ev.len(), 3);
    assert!(ev.iter().all(|(a, _)| *a == acc(1)));
    assert!(
        ev.iter()
            .any(|(_, e)| matches!(e, RiskEvent::HedgeBroken { group } if *group == G1))
    );
    assert!(
        ev.iter()
            .any(|(_, e)| matches!(e, RiskEvent::CapSet { source, .. } if *source == CapSource::BROKEN))
    );
    assert!(
        ev.iter().any(
            |(_, e)| matches!(e, RiskEvent::CapSet { source, .. } if *source == CapSource::HEDGE_FAILURE)
        )
    );
    assert_eq!(w.out.notes.len(), 2);
    assert!(
        w.out
            .notes
            .iter()
            .all(|x| note_n(x) == N_CAP_SET && x.level == NoteLevel::P1)
    );
    assert_eq!(w.risk.mode.ctl_state_version(), v + 1);
    assert!(!w.out.kill_changed && !w.out.resolve_broken && w.out.drain.is_none());
    assert!(w.out.released.is_empty() && w.out.hedge_gate_open.is_empty());
    assert!(w.out.venue_gate_open.is_empty() && w.out.gate_changes.is_empty());
}

#[test]
fn rsk_051_on_legs_auto_clear() {
    let want = Some(auto_clear_broken(1, G1));
    let fresh = || {
        let mut w = r0();
        w.ok(broken(1, G1));
        w
    };
    let mut w = fresh();
    assert_eq!(w.legs_full(1, G1, true, "10"), want, "RSK-051.1");
    assert_eq!(w.legs_full(1, G1, true, "10.000001"), None, "RSK-051.2");
    assert_eq!(w.legs_full(1, G1, false, "0"), None, "RSK-051.3");
    assert_eq!(w.legs_full(1, G1, false, "0"), None, "RSK-051.4");
    assert_eq!(w.legs_full(1, G1, true, "0"), want, "RSK-051.5");
    assert_eq!(w.legs_full(1, G3, true, "0"), None, "RSK-051.6");
    let mut w = fresh();
    assert_eq!(w.legs_full(1, G1, false, "0"), None);
    assert_eq!(w.legs_full(1, G1, true, "12"), None, "RSK-051.7");
    let mut w = fresh();
    assert_eq!(w.legs_full(1, G1, true, "12"), None, "RSK-051.8 a");
    assert_eq!(w.legs_full(1, G1, true, "0"), want, "RSK-051.8 b");
    {
        let rd = r0_refdata();
        let mut ex = exec_bounds(3);
        for b in ex.iter_mut() {
            b.dust_notional = hft_types::Notional(0);
        }
        let mut w = world_ex(rd, p0_for(rd, 3), MC, CAPS, &ex);
        w.ok(broken(1, G1));
        assert_eq!(w.legs_full(1, G1, true, "0"), want, "RSK-051.9 a");
        assert_eq!(w.legs_full(1, G1, true, "0.000001"), None, "RSK-051.9 b");
        let _: Option<ExecBounds> = None;
    }
}

#[test]
fn rsk_052_auto_clear_keeps_hedge_failure() {
    let mut w = r0();
    w.ok(broken(1, G1));
    let v = w.risk.mode.ctl_state_version();
    let inp = w.legs_full(1, G1, true, "5").expect("AutoClear");
    w.ok(inp);
    assert!(w.cap(CapSource::BROKEN, sc_ag(1, G1)).is_none());
    assert!(w.cap(CapSource::HEDGE_FAILURE, sc_acct(1)).is_some());
    assert_eq!(w.out.notes.len(), 1);
    assert_eq!(
        (
            note_n(&w.out.notes.as_slice()[0]),
            w.out.notes.as_slice()[0].level
        ),
        (N_CAP_CLEARED, NoteLevel::Info)
    );
    let ev: Vec<_> = w.out.risk_events.iter().copied().collect();
    assert_eq!(ev.len(), 1);
    assert!(
        matches!(ev[0], (a, RiskEvent::CapCleared { source, .. }) if a == acc(1) && source == CapSource::BROKEN)
    );
    assert!(w.out.cells.is_empty());
    assert_eq!(w.risk.mode.ctl_state_version(), v + 1);
    assert!(!w.out.resolve_broken);
    assert_eq!(w.eval(&open_a1()).map(|_| ()), Err(RejectBits::CLOSE_ONLY));
}

#[test]
fn rsk_053_resume_broken_requires_cleared() {
    let mut w = r0();
    w.ok(broken(1, G1));
    w.mm.naked = None;
    w.mm.naked_cell.insert((1, G1.0), false);
    let v = w.risk.mode.ctl_state_version();
    let caps0 = w.risk.mode.caps().to_vec();
    w.out.clear();
    assert_eq!(
        w.apply(resume(CapSource::BROKEN, sc_ag(1, G1))),
        Err(ModeErr::NakedNotCleared)
    );
    assert_eq!(w.risk.mode.caps(), caps0.as_slice());
    assert_eq!(w.risk.mode.ctl_state_version(), v);
    assert!(
        w.out.cells.is_empty()
            && w.out.cancels.is_empty()
            && w.out.risk_events.is_empty()
            && w.out.notes.is_empty()
    );
    w.mm.naked_cell.insert((1, G1.0), true);
    assert_eq!(w.apply(resume(CapSource::BROKEN, sc_ag(1, G1))), Ok(()));
    assert!(w.cap(CapSource::BROKEN, sc_ag(1, G1)).is_none());
    assert!(w.cap(CapSource::HEDGE_FAILURE, sc_acct(1)).is_some());
    assert!(!w.out.resolve_broken);
}

#[test]
fn rsk_054_resume_hedge_failure_after_all_broken() {
    let mut w = r0();
    w.ok(broken(1, G1));
    w.ok(broken(1, G3));
    let hf = || resume(CapSource::HEDGE_FAILURE, sc_acct(1));
    assert_eq!(w.apply(hf()), Err(ModeErr::BrokenRemaining), "step 1");
    w.ok(auto_clear_broken(1, G1));
    assert_eq!(w.apply(hf()), Err(ModeErr::BrokenRemaining), "step 2");
    w.ok(auto_clear_broken(1, G3));
    assert_eq!(w.apply(hf()), Ok(()), "step 3");
    let mut cells: Vec<_> = w.out.cells.iter().copied().collect();
    cells.sort_by_key(|c| c.0);
    let close = Effective::CloseOnly {
        deferred_shadow: false,
    };
    assert_eq!(
        cells,
        vec![
            (cell(1, 1, G1), close, Effective::Live),
            (cell(1, 1, G3), close, Effective::Live)
        ]
    );
    assert_eq!(
        w.out.notes.iter().filter(|x| note_n(x) == N_CAP_CLEARED).count(),
        1
    );
    assert!(w.eval(&open_a1()).is_ok(), "step 4");
}

#[test]
fn rsk_055_resolve_broken_only_on_hard_release() {
    // .1–.3: HARD kills released after a fresh reconciliation group of every Live venue.
    let hard_case = |id: &str, set: &[ModeInput<'static>], release: ModeInput<'static>| {
        let mut w = r0();
        for s in set {
            w.ok(*s);
        }
        w.recon_all();
        assert_eq!(w.apply(release), Ok(()), "{id}");
        assert!(w.out.resolve_broken, "{id}");
    };
    hard_case(
        "RSK-055.1",
        &[kill_global(true)],
        resume(CapSource::KILL, sc_any()),
    );
    hard_case(
        "RSK-055.2",
        &[kill_acct(1, true)],
        resume(CapSource::KILL, sc_acct(1)),
    );
    hard_case(
        "RSK-055.3",
        &[kill_acct(1, false), kill_acct(1, true)],
        resume(CapSource::KILL, sc_acct(1)),
    );
    let soft = |id: &str, set: ModeInput<'static>, release: ModeInput<'static>| {
        let mut w = r0();
        w.ok(set);
        assert_eq!(w.apply(release), Ok(()), "{id}");
        assert!(!w.out.resolve_broken, "{id}");
    };
    soft(
        "RSK-055.4",
        kill_acct(1, false),
        resume(CapSource::KILL, sc_acct(1)),
    );
    soft("RSK-055.5", daily(1), resume(CapSource::DAILY_LOSS, sc_acct(1)));
    {
        let mut w = g0_flags(RecoveryFlags::CLEAN_DRAIN, MC);
        let steps = w.release_steps(1);
        for s in &steps {
            w.step(1, s).unwrap();
            assert!(!w.out.resolve_broken, "RSK-055.6 step {s:?}");
        }
        assert!(
            w.cap(CapSource::RECOVERY, sc_acct(1)).is_none(),
            "RSK-055.6 recovery removed"
        );
    }
    soft(
        "RSK-055.7",
        auto(CapSource::RECOVERY, sc_acct(1)),
        resume(CapSource::RECOVERY, sc_acct(1)),
    );
    soft("RSK-055.8", broken(1, G1), auto_clear_broken(1, G1));
}

#[test]
fn rsk_056_broken_engine_orders() {
    let fresh = || {
        let mut w = r0();
        w.ok(broken(1, G1));
        w.mv.set_pos(1, B1, "-0.5");
        w
    };
    let hf = |qty: &str, p: &str, cap: &str| hedge_fix(1, G1, B1, Side::Buy, qty, p, cap);
    let mut w = fresh();
    assert!(w.eval(&hf("0.01", "50005.0", "0.01")).is_ok(), "RSK-056.1");
    assert_eq!(
        w.eval(&hf("0.011", "50005.0", "0.01")).map(|_| ()),
        Err(RejectBits::HEDGE_LIMIT),
        "RSK-056.2"
    );
    let mut it = hf("0.4", "50000.0", "0.5");
    it.notional = n("20000.000001");
    assert_eq!(w.eval(&it).map(|_| ()), Err(RejectBits::HEDGE_LIMIT), "RSK-056.3");
    let mut w = fresh();
    w.ok(kill_acct(1, true));
    assert_eq!(
        w.eval(&hf("0.01", "50005.0", "0.01")).map(|_| ()),
        Err(RejectBits::HARD),
        "RSK-056.4"
    );
    let mut w = fresh();
    w.ok(margin_auto(1, 3, 1 << B.0));
    w.ok(kill_flatten(1));
    let it = w.flatten(1, G1, B1, Side::Buy, "0.2", "50000.0", Origin::Exec);
    assert!(w.eval(&it).is_ok(), "RSK-056.5: {:?}", w.eval(&it));
    let mut w = fresh();
    w.ok(kill_flatten(1));
    let mut it = w.flatten(1, G1, B1, Side::Buy, "0.2", "50000.0", Origin::Exec);
    it.notional = n("10000.000001");
    assert_eq!(
        w.eval(&it).map(|_| ()),
        Err(RejectBits::NOTIONAL_ORDER),
        "RSK-056.6"
    );
    let _ = InstId(0);
}
