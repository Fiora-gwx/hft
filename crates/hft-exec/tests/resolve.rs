//! EXE-230–EXE-232: `resolve_broken` at HARD release.
mod common;
use common::*;
use hft_exec::*;
use hft_strategy_sdk::TargetState;
use hft_types::{CapSource, Side};

#[test]
fn exe_230_resolve_broken_table() {
    type Edit = fn(&mut Fx);
    let rows: [(&str, &str, &str, Edit, bool); 8] = [
        ("EXE-230.1", "1.00", "-1.5", |_| {}, true),
        (
            "EXE-230.2",
            "1.00",
            "-1.5",
            |fx| fx.lv(A1, G7, |l| l.open_links = 1),
            false,
        ),
        (
            "EXE-230.3",
            "1.00",
            "-1.5",
            |fx| fx.cm(A1, G7).hold = MANUAL_HOLD,
            false,
        ),
        ("EXE-230.4", "10.00", "-9.8", |_| {}, false),
        ("EXE-230.5", "10.00", "-9.79999999", |_| {}, true),
        ("EXE-230.6", "1.05", "-1.0", |_| {}, false),
        ("EXE-230.7", "1.05000001", "-1.0", |_| {}, true),
        (
            "EXE-230.8",
            "1.00",
            "-1.5",
            |fx| fx.lv(A1, G7, |l| l.has_unknown = true),
            true,
        ),
    ];
    for (row, s, p, edit, want) in rows {
        let mut fx = Fx::new();
        fx.legs(A1, G7, &[s, p]);
        fx.cm(A1, G7).tolerance = rate("0.02");
        edit(&mut fx);
        assert_eq!(fx.ex.resolve_broken(A1, G7, &fx.v), want, "{row}");
    }
}

#[test]
fn exe_231_read_only() {
    let mut fx = Fx::new();
    fx.legs(A1, G7, &["1.00", "-1.5"]);
    let x = fx.snap();
    let pend = fx.ex.has_pending();
    for _ in 0..1000 {
        assert!(fx.ex.resolve_broken(A1, G7, &fx.v));
    }
    assert_eq!(fx.snap(), x);
    assert_eq!(fx.ex.has_pending(), pend);
}

#[test]
fn exe_232_soft_kill_release_no_broken() {
    let mut fx = Fx::new();
    fx.risk(A1, |r| r.open_blocked = true);
    fx.legs(A1, G7, &["1.00", "-1.5"]);
    fx.leg_adjust(A1, G7);
    let r = fx.adv();
    let o = r.ord(ExecOrderKind::HedgeFix);
    assert_eq!((o[0].intent.side, o[0].intent.qty), (Side::Buy, q("0.5")));
    fx.risk(A1, |r| r.open_blocked = false);
    fx.risk_ev(A1, cap_clr(CapSource::KILL, A1, GNONE));
    let r = fx.adv();
    assert!(r.states().iter().all(|s| s.1 != 4));
    assert!(!matches!(fx.cell(A1, G7).view.state, TargetState::Broken(_)));
}
