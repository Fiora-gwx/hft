//! Daily loss latch (RSK-190–RSK-193).
#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods
)] // allow-list: test code, not a hot path

use crate::common;
use common::*;

use std::collections::HashMap;

use hft_risk::mode::{ModeErr, ModeInput};
use hft_risk::{EquitySource, EquityView};
use hft_types::{AccountId, CapSource, Notional};

struct Eqs(HashMap<u8, EquityView>);
impl EquitySource for Eqs {
    fn equity(&self, a: AccountId) -> EquityView {
        *self.0.get(&a.0).unwrap_or(&EquityView {
            total: Notional(0),
            valid: false,
        })
    }
}
fn eq(total: &str, valid: bool) -> EquityView {
    EquityView {
        total: n(total),
        valid,
    }
}
fn roll(w: &mut W, day: u32, eqs: &[(u8, EquityView)]) {
    w.risk.on_day_roll(day, &Eqs(eqs.iter().copied().collect()));
}
/// `params[0]` of a returned `Auto(DailyLoss)` (as signed raw).
fn trig(i: Option<ModeInput<'static>>) -> Option<i64> {
    i.map(|x| match x {
        ModeInput::Auto(r) if r.source == CapSource::DAILY_LOSS => r.params[0] as i64,
        other => panic!("unexpected {other:?}"),
    })
}

#[test]
fn rsk_190_daily_loss_quota() {
    let run = |id: &str, sod: EquityView, seq: &[(&str, EquityView, Option<&str>)]| {
        let mut w = r0();
        roll(&mut w, 100, &[(1, sod)]);
        for (k, (pnl, e, want)) in seq.iter().enumerate() {
            let got = trig(w.risk.on_pnl(acc(1), n(pnl), *e));
            assert_eq!(got, want.map(|x| n(x).0), "{id} step {k}");
        }
    };
    let v200 = eq("200000", true);
    run(
        "RSK-190.1",
        v200,
        &[("-5000", v200, None), ("-5000.000001", v200, Some("-5000"))],
    );
    let v80 = eq("80000", true);
    run(
        "RSK-190.2",
        v80,
        &[("-4000", v80, None), ("-4000.000001", v80, Some("-4000"))],
    );
    let bad = eq("0", false);
    run("RSK-190.3", bad, &[("-5000.000001", bad, Some("-5000"))]);
    run(
        "RSK-190.4",
        bad,
        &[
            ("-3000", v80, None),
            ("-4000", v80, None),
            ("-4000.000001", v80, Some("-4000")),
        ],
    );
    run(
        "RSK-190.5",
        v200,
        &[
            ("-5000.000001", v200, Some("-5000")),
            ("-6000", v200, None),
            ("-7000", v200, None),
        ],
    );
    run("RSK-190.6", v200, &[("1000", v200, None)]);
}

#[test]
fn rsk_191_day_roll_keeps_latch() {
    let mut w = r0();
    w.mm.daily = None;
    let v200 = eq("200000", true);
    roll(&mut w, 100, &[(1, v200), (2, v200)]);
    let i = w.risk.on_pnl(acc(1), n("-5000.000001"), v200).unwrap();
    w.ok(i);
    let i = w.risk.on_pnl(acc(2), n("-5000.000001"), v200).unwrap();
    w.ok(i);
    w.ok(resume(CapSource::DAILY_LOSS, sc_acct(2)));
    w.risk.on_daily_loss_resumed(acc(2), n("-2000"));
    roll(&mut w, 101, &[(1, v200), (2, eq("60000", true))]);
    assert!(w.cap(CapSource::DAILY_LOSS, sc_acct(1)).is_some(), "step 1");
    assert_eq!(w.risk.on_pnl(acc(1), n("-1"), v200), None, "step 2 account 1");
    let e60 = eq("60000", true);
    assert_eq!(w.risk.on_pnl(acc(2), n("-3000"), e60), None, "step 2 account 2");
    let got = w.risk.on_pnl(acc(2), n("-3000.000001"), e60).unwrap();
    match got {
        ModeInput::Auto(r) => assert_eq!(r.params[0] as i64, n("-3000").0),
        _ => panic!(),
    }
    assert!(
        w.risk.daily_loss_resumable(acc(1)) && w.risk.daily_loss_resumable(acc(2)),
        "step 3"
    );
}

#[test]
fn rsk_192_resume_daily_loss() {
    let mut w = r0();
    w.mm.daily = None;
    let v200 = eq("200000", true);
    roll(&mut w, 100, &[(1, v200)]);
    let i = w.risk.on_pnl(acc(1), n("-5500"), v200).unwrap();
    w.ok(i);
    assert_eq!(
        w.apply(resume(CapSource::DAILY_LOSS, sc_acct(1))),
        Ok(()),
        "step 1"
    );
    assert_eq!(w.out.daily_loss_resumed.as_slice(), &[acc(1)]);
    assert!(w.cap(CapSource::DAILY_LOSS, sc_acct(1)).is_none());
    w.risk.on_daily_loss_resumed(acc(1), n("-5500"));
    assert_eq!(w.risk.on_pnl(acc(1), n("-10500"), v200), None, "step 3 a");
    let i = w.risk.on_pnl(acc(1), n("-10500.000001"), v200);
    assert_eq!(trig(i), Some(n("-10500").0), "step 3 b");
    w.ok(i.unwrap());
    assert_eq!(
        w.apply(resume(CapSource::DAILY_LOSS, sc_acct(1))),
        Err(ModeErr::ResumeLimit),
        "step 4"
    );
    assert!(w.cap(CapSource::DAILY_LOSS, sc_acct(1)).is_some());
    roll(&mut w, 101, &[(1, v200)]);
    assert_eq!(
        w.apply(resume(CapSource::DAILY_LOSS, sc_acct(1))),
        Ok(()),
        "step 5"
    );
}

#[test]
fn rsk_193_resumable_boundary() {
    for (id, max, done, want) in [
        ("RSK-193.1", 1u8, 0u8, true),
        ("RSK-193.2", 1, 1, false),
        ("RSK-193.3", 2, 1, true),
        ("RSK-193.4", 2, 2, false),
        ("RSK-193.5", 0, 0, false),
    ] {
        let mut w = r0_with(|p| with_acct(&p, 1, |a| a.daily_loss_max_resumes = max));
        for _ in 0..done {
            w.risk.on_daily_loss_resumed(acc(1), n("0"));
        }
        assert_eq!(w.risk.daily_loss_resumable(acc(1)), want, "{id}");
    }
}
