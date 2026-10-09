//! EXE-021–EXE-027: output capacity, multi-cell entries, Continue, cancel_resting dedup, ordering.
mod common;
use common::*;
use hft_exec::*;
use hft_strategy_sdk::{ExecUpdateKind, TargetState};
use hft_types::{CapSource, Tsc};

fn setup64() -> Fx {
    let mut fx = fx_many(64);
    for k in 0..64 {
        inject(
            &mut fx,
            &target(
                A1,
                many(k),
                &[(0, q("10")), (1, q("-10"))],
                hft_strategy_sdk::ExecSpec::PairOnFill(pair0()),
            ),
        );
        fx.legs(A1, many(k), &["2", "-2"]);
    }
    for k in 0..2 {
        fx.cm(A1, many(k)).view.state = TargetState::Broken(hft_strategy_sdk::BrokenReason::LinkBroken);
    }
    fx
}

#[test]
fn exe_021_multi_cell_entries_write_nothing() {
    type Call = fn(&mut Fx);
    let rows: [(&str, Call); 7] = [
        ("EXE-021.1", |fx| fx.flatten(A1)),
        ("EXE-021.2", |fx| fx.force(A1, B, "0.2")),
        ("EXE-021.3", |fx| {
            let now = fx.now();
            fx.ex.clear_account(A1, now, &mut fx.out)
        }),
        ("EXE-021.4", |fx| {
            fx.risk_ev(A1, cap_set(CapSource::KILL, A1, GNONE))
        }),
        ("EXE-021.5", |fx| {
            let now = fx.now();
            fx.ex.on_gate_open(A1, now)
        }),
        ("EXE-021.6", |fx| {
            let now = fx.now();
            fx.ex.on_venue_gate_open(A1, B, now)
        }),
        ("EXE-021.7", |fx| {
            let mut g = vec![];
            for k in 0..64 {
                let legs: &[(hft_types::InstId, hft_types::VenueId)] = if k < 10 {
                    &[(S, A), (P3, B)]
                } else {
                    &[(S, A), (P, B)]
                };
                g.push(gspec(many(k), legs));
            }
            let r = regroup(fx, g);
            assert!(r.is_empty());
        }),
    ];
    for (row, call) in rows {
        let mut fx = setup64();
        if row == "EXE-021.5" || row == "EXE-021.6" {
            let now = fx.now();
            fx.ex
                .recovery_end(now.tsc, now.tsc, 1_000_000_000_000, 1_000_000_000_000);
        }
        fx.out.clear();
        call(&mut fx);
        assert!(fx.out.is_empty(), "{row}: no output");
        assert!(fx.ex.has_pending(), "{row}: pending");
        let r = fx.adv_all();
        if row == "EXE-021.1" {
            assert_eq!(
                r.decisions
                    .iter()
                    .filter(|d| d.action == 11 && d.detail == pack(5, 1, 0))
                    .count(),
                64,
                "EXE-021.8"
            );
        }
        if row == "EXE-021.7" {
            assert_eq!(r.dec(3).len(), 10, "EXE-021.8");
        }
    }
}

fn flatten64(caps: OutCaps) -> (Fx, Vec<Rec>) {
    let mut fx = fx_many(64);
    for k in 0..64 {
        fx.legs(A1, many(k), &["5.00", "-5.0"]);
        fx.spot(A1, A, many(k), "5", "5");
    }
    fx.out = ExecOut::with_caps(caps);
    fx.flatten(A1);
    let mut batches = vec![fx.adv()];
    for _ in 0..200 {
        if !fx.ex.has_pending() {
            break;
        }
        let r = fx.fire(ExecTimerKind::Continue, hft_types::AccountId(0), GNONE);
        assert!(r.is_empty());
        batches.push(fx.adv());
    }
    (fx, batches)
}

#[test]
fn exe_022_advance_continue() {
    let (fx, batches) = flatten64(OutCaps::README);
    assert!(batches.len() > 1);
    let b0 = &batches[0];
    let cont: Vec<_> = b0
        .timers
        .iter()
        .filter(|t| t.kind() == Some(ExecTimerKind::Continue))
        .collect();
    assert_eq!(cont.len(), 1);
    assert_eq!((cont[0].timer_id, cont[0].group), (4, GNONE));
    let p = OutCaps::PER_CELL;
    for b in &batches[..batches.len() - 1] {
        assert!(b.orders.len() <= 8 && b.decisions.len() <= 16 && b.notes.len() <= 8 && b.timers.len() <= 8);
        assert!(
            8 - b.orders.len() < p.orders
                || 8 - b.cancel_resting.len() < p.cancel_resting
                || 16 - b.decisions.len() < p.decisions
                || 8 - b.notify.len() < p.notify
                || 8 - b.timers.len() <= p.timers
                || 8 - b.notes.len() < p.notes
        );
    }
    assert!(!fx.ex.has_pending());
    let mut all = Vec::new();
    for b in &batches {
        all.extend(b.decisions.iter().copied());
    }
    let (_, reference) = flatten64(OutCaps::all(100_000));
    assert_eq!(norm(&all), norm(&reference[0].decisions));
    assert_eq!(
        all.iter()
            .filter(|d| d.action == 11 && d.detail == pack(5, 1, 0))
            .count(),
        64
    );
    let cix: Vec<u16> = b0.orders.iter().map(|o| o.id.cix).collect();
    assert!(cix.windows(2).all(|w| w[0] < w[1]));
}

#[test]
fn exe_023_cancel_resting_dedup() {
    let mut fx = Fx::with(c0(), vec![g7(), g8(), gspec(G10, &[(S, A), (P, B)])]);
    fx.legs(A1, G7, &["5", "-5.0"]);
    fx.legs(A1, G10, &["5", "-5.0"]);
    fx.risk(A1, |r| r.margin_level = hft_strategy_sdk::MarginLevel::L3);
    fx.force(A1, B, "0.2");
    let r = fx.adv();
    assert_eq!(
        r.cancel_resting,
        vec![
            CancelResting { account: A1, inst: S },
            CancelResting { account: A1, inst: P }
        ]
    );
    for g in [G7, G10] {
        assert_eq!(
            r.decisions
                .iter()
                .filter(|d| d.group == g && d.action == 11 && d.detail == pack(5, 5, 0b11))
                .count(),
            1
        );
    }
}

/// g7 Broken with a Running auto-fix slice (sell S) in flight.
pub fn running_autofix(fx: &mut Fx, budget: &str, done: &str, n: u8) -> hft_strategy_sdk::OrderRef {
    let r0 = hft_strategy_sdk::OrderRef::new(A, A1, 77, 1);
    let c = fx.cm(A1, G7);
    c.view.state = TargetState::Broken(hft_strategy_sdk::BrokenReason::LinkBroken);
    c.cap_block = CAP_BROKEN;
    c.broken_since = Tsc(1);
    c.auto_fix.phase = AutoFixPhase::Running as u8;
    c.auto_fix.leg_ix = 0;
    c.auto_fix.start_sign = 1;
    c.auto_fix.budget = q(budget);
    c.auto_fix.done_qty = q(done);
    c.auto_fix.n_slices = n;
    c.auto_fix.run_mark_tsc = Tsc(1);
    c.active.kind = ACT_AUTOFIX;
    c.active.ext = [10, 1, 1, 0];
    c.active.first = OrderRefRow::of(r0);
    r0
}

#[test]
fn exe_025_last_autofix_slice_before_capcleared() {
    let mut fx = Fx::new();
    fx.spot(A1, A, G7, "1.10", "1.10");
    let r0 = running_autofix(&mut fx, "0.50", "0.40", 4);
    fx.legs(A1, G7, &["1.00", "-1.00"]);
    let r1 = fx.fill(r0, G7, "0.10", true);
    assert!(r1.has_dec(14, pack(4, 3, 0)));
    let n = r1.notes_n(hft_exec::decision::note::EXEC_AUTO_FIX_DONE);
    assert_eq!(n.len(), 1);
    assert_eq!(n[0].level, hft_event::NoteLevel::P1);
    assert_eq!(
        &n[0].v[..6],
        &[0x01_0000_0701, 0, q("0.50").0 as u64, q("0.50").0 as u64, 0, 5]
    );
    fx.risk_ev(A1, cap_clr(CapSource::BROKEN, A1, G7));
    assert_eq!(fx.cell(A1, G7).auto_fix.phase, 0);
    let r3 = fx.adv();
    assert!(r3.states().contains(&(4, 1, 0)));
    assert!(r3.upd().contains(&ExecUpdateKind::Done {
        cleared_by_engine: true
    }));
    assert_eq!(
        fx.log.notes_n(hft_exec::decision::note::EXEC_AUTO_FIX_DONE).len(),
        1
    );
}

#[test]
fn exe_026_set_target_registers_wake() {
    let mut fx = Fx::new();
    fx.at(1000);
    let (res, r) = fx.set(I1, &t1("-10.0"));
    assert_eq!(res, Ok(()));
    assert!(r.orders.is_empty());
    let w = r.timer(ExecTimerKind::Wake);
    assert_eq!(w.len(), 1);
    assert_eq!(
        (w[0].timer_id, w[0].group, w[0].at, w[0].cancel),
        (1 | 1 << 8, G8, t(1000), false)
    );
    assert_eq!(r.dec(1).len(), 1);
    assert!(fx.ex.has_pending());
}

#[test]
fn exe_027_early_wake_reregisters() {
    let mut fx = Fx::new();
    fx.at(1000);
    let r = fx.go(&t1("-10.0"));
    let o = r.orders[0];
    let rf = fx.placed(&o);
    fx.fill(rf, G8, "4.0", true);
    fx.adv();
    assert!(!fx.ex.has_pending());
    fx.at(1001);
    let r1 = fx.fire(ExecTimerKind::Wake, A1, G8);
    assert_eq!(r1.timers.len(), 1);
    assert_eq!(
        (r1.timers[0].kind(), r1.timers[0].at, r1.timers[0].cancel),
        (Some(ExecTimerKind::Wake), t(1300), false)
    );
    assert!(r1.orders.is_empty() && r1.decisions.is_empty());
    assert!(!fx.ex.has_pending());
    fx.at(1300);
    fx.fire(ExecTimerKind::Wake, A1, G8);
    assert!(fx.ex.has_pending());
    let r2 = fx.adv();
    assert_eq!(r2.orders.len(), 1);
}
