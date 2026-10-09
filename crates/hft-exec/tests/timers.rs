//! EXE-440, EXE-441, EXE-443, EXE-444: timers.
mod common;
use common::*;
use hft_exec::*;
use hft_strategy_sdk::TargetState;
use hft_types::{AccountId, GroupId};

#[test]
fn exe_440_timer_encoding() {
    let rows = [
        ("EXE-440.1", ExecTimerKind::Wake, 0x0101u32),
        ("EXE-440.2", ExecTimerKind::Fix, 0x0102),
        ("EXE-440.3", ExecTimerKind::External, 0x0103),
        ("EXE-440.5", ExecTimerKind::AutoFix, 0x0105),
        ("EXE-440.6", ExecTimerKind::Alarm, 0x0106),
    ];
    for (row, k, id) in rows {
        let x = ExecTimerReq {
            cancel: false,
            timer_id: ExecTimerReq::id(k, A1),
            group: G7,
            at: t(0),
        };
        assert_eq!(x.timer_id, id, "{row}");
        assert_eq!(x.token(), 0x0100_0007_0000_0000 | u64::from(id), "{row}");
        assert_eq!(x.kind(), Some(k), "{row}");
    }
    let c = ExecTimerReq {
        cancel: false,
        timer_id: ExecTimerReq::id(ExecTimerKind::Continue, AccountId(0)),
        group: GNONE,
        at: t(0),
    };
    assert_eq!((c.timer_id, c.token()), (4, 4), "EXE-440.4");
    // .7 the ungrouped cell's Wake uses GROUP_NONE.
    let mut fx = Fx::new();
    fx.risk(A1, |r| r.margin_level = hft_strategy_sdk::MarginLevel::L3);
    fx.perp(A1, B, P4, "-3");
    fx.force(A1, B, "0.5");
    let r = fx.adv();
    let w: Vec<_> = r
        .timer(ExecTimerKind::Wake)
        .into_iter()
        .filter(|x| x.group == GNONE)
        .collect();
    assert!(!w.is_empty(), "EXE-440.7");
}

#[test]
fn exe_441_one_per_kind() {
    let mut fx = Fx::new();
    fx.acct(A1).venue_open = 0;
    {
        let c = fx.cm(A1, G7);
        c.ext_deadline = t(6000);
        c.aux16 |= 1 << ExecTimerKind::External as u16;
    }
    fx.at(10_000);
    let now = fx.now();
    fx.ex.on_venue_gate_open(A1, A, now);
    let r = fx.adv();
    let e = r.timer(ExecTimerKind::External);
    assert_eq!(e.len(), 2);
    assert!(e[0].cancel && e[0].group == G7);
    assert!(!e[1].cancel && e[1].at == t(15_000));
}

#[test]
fn exe_443_early_timer() {
    let mut fx = Fx::new();
    fx.cm(A1, G7).fix.next_tsc = t(2000);
    fx.at(1000);
    let r = fx.fire(ExecTimerKind::Fix, A1, G7);
    assert_eq!(r.timer_set(ExecTimerKind::Fix), vec![t(2000)], "EXE-443.1");
    assert!(fx.adv().orders.is_empty(), "EXE-443.1");
    let mut fx = Fx::new();
    fx.cm(A1, G7).ext_deadline = t(6000);
    fx.at(1000);
    let r = fx.fire(ExecTimerKind::External, A1, G7);
    assert_eq!(r.timer_set(ExecTimerKind::External), vec![t(6000)], "EXE-443.2");
    assert!(fx.adv().dec(10).is_empty(), "EXE-443.2");
    let mut fx = Fx::new();
    fx.legs(A1, G7, &["1.00", "-1.5"]);
    {
        let c = fx.cm(A1, G7);
        c.view.state = TargetState::Broken(hft_strategy_sdk::BrokenReason::LinkBroken);
        c.auto_fix.phase = AutoFixPhase::Primed as u8;
        c.auto_fix.due_tsc = t(120_000);
    }
    fx.at(1000);
    let r = fx.fire(ExecTimerKind::AutoFix, A1, G7);
    assert_eq!(r.timer_set(ExecTimerKind::AutoFix), vec![t(120_000)], "EXE-443.3");
    assert!(fx.adv().dec(14).is_empty(), "EXE-443.3");
}

#[test]
fn exe_444_removed_cell_timer() {
    let mut fx = Fx::new();
    inject(&mut fx, &t1("-10"));
    regroup(&mut fx, vec![g7()]);
    let token = (ExecTimerReq::id(ExecTimerKind::Wake, A1) as u64) | (u64::from(G8.0) << 32);
    let now = fx.now();
    fx.ex.on_timer(token, now, &mut fx.out);
    assert!(fx.out.is_empty());
    assert!(!fx.ex.has_pending());
    let _ = GroupId(0);
}
