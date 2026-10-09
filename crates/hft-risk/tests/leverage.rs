//! Perp leverage cap (RSK-180, RSK-181).
#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods
)] // allow-list: test code, not a hot path

use crate::common;
use common::*;

use hft_event::{LeverageItem, NoteLevel};
use hft_risk::acr_rflags;
use hft_risk::mode::{ModeInput, cfg_params, mismatch};
use hft_types::{CapSource, GroupId, RejectBits, Side, Tif, VenueId};

/// `on_account_cfg` + `Gate(AccountCfg)` of account 1 on `v`.
fn report(w: &mut W, v: VenueId, rflags: u16, mism: u32, levs: &[LeverageItem]) {
    let mut h = hdr(1, v, mism, KEYS);
    h.rflags = rflags;
    w.step(1, &Step::Cfg(h, levs.to_vec(), KEYS)).unwrap();
}

fn cap_params(w: &W, v: VenueId) -> Option<u64> {
    w.cap(CapSource::ACCOUNT_CFG, sc_av(1, v)).map(|c| c.params[0])
}

fn capset_levels(w: &W) -> Vec<NoteLevel> {
    w.out
        .notes
        .iter()
        .filter(|x| note_n(x) == N_CAP_SET)
        .map(|x| x.level)
        .collect()
}

#[test]
fn rsk_180_leverage_cap() {
    let bad_b = |id: &str, w: &mut W| {
        assert!(w.risk.lev_bad(acc(1), B), "{id} lev_bad");
        assert_eq!(cap_params(w, B), Some(cfg_params::LEVERAGE), "{id} cap");
        assert_eq!(capset_levels(w), vec![NoteLevel::P1], "{id} note");
        let ob1 = open(1, G1, B1, Side::Buy, "0.01", "50000.0", Tif::Gtc);
        let ob2 = open(1, G3, B2, Side::Buy, "0.1", "2500.0", Tif::Gtc);
        let oa1 = open(1, G1, A1, Side::Buy, "0.01", "50000.00", Tif::Gtc);
        assert_eq!(w.eval(&ob1).map(|_| ()), Err(RejectBits::CLOSE_ONLY), "{id} b1");
        assert_eq!(w.eval(&ob2).map(|_| ()), Err(RejectBits::CLOSE_ONLY), "{id} b2");
        assert!(w.eval(&oa1).is_ok(), "{id} a1");
    };
    let mut w = r0();
    report(&mut w, B, 0, 0, &[lev(B1, "3.00"), lev(B2, "3.00")]);
    assert!(
        !w.risk.lev_bad(acc(1), B) && cap_params(&w, B).is_none(),
        "RSK-180.1"
    );
    let mut w = r0();
    report(&mut w, B, 0, 0, &[lev(B1, "3.01"), lev(B2, "3.00")]);
    bad_b("RSK-180.2", &mut w);
    {
        // .8 / .9 continue from .2.
        let mut w8 = w.clone();
        report(&mut w8, B, 0, 0, &[lev(B1, "3.00"), lev(B2, "3.00")]);
        assert!(cap_params(&w8, B).is_none(), "RSK-180.8");
        assert_eq!(
            w8.out.notes.iter().filter(|x| note_n(x) == N_CAP_CLEARED).count(),
            1,
            "RSK-180.8 note"
        );
        let mut w9 = w.clone();
        report(&mut w9, B, acr_rflags::PARTIAL, 0, &[lev(B1, "3.00")]);
        assert!(cap_params(&w9, B).is_none(), "RSK-180.9");
    }
    let mut w = r0();
    report(&mut w, B, 0, 0, &[lev(B1, "0"), lev(B2, "3.00")]);
    bad_b("RSK-180.3", &mut w);
    let mut w = r0();
    report(&mut w, B, 0, 0, &[lev(B2, "3.00")]);
    bad_b("RSK-180.4", &mut w);
    let mut w = r0();
    report(&mut w, B, acr_rflags::LEV_TRUNCATED, 0, &[lev(B2, "3.00")]);
    bad_b("RSK-180.5", &mut w);
    let mut w = r0();
    report(&mut w, C, 0, 0, &[lev(C1, "20.00")]);
    assert!(
        !w.risk.lev_bad(acc(1), C) && cap_params(&w, C).is_none(),
        "RSK-180.6"
    );
    let mut w = r0();
    report(&mut w, C, 0, 0, &[]);
    assert!(cap_params(&w, C).is_none(), "RSK-180.7");
    let mut w = r0();
    report(&mut w, B, acr_rflags::PARTIAL, 0, &[lev(B2, "5.00")]);
    assert!(w.risk.lev_bad(acc(1), B), "RSK-180.10");
    assert_eq!(cap_params(&w, B), Some(cfg_params::LEVERAGE), "RSK-180.10 cap");
    let mut w = r0();
    report(
        &mut w,
        B,
        0,
        mismatch::MARGIN_MODE,
        &[lev(B1, "3.01"), lev(B2, "3.00")],
    );
    assert_eq!(cap_params(&w, B), Some(3), "RSK-180.11");
    report(
        &mut w,
        B,
        0,
        mismatch::MARGIN_MODE,
        &[lev(B1, "3.00"), lev(B2, "3.00")],
    );
    assert!(cap_params(&w, B).is_some(), "RSK-180.11 leverage fixed only");
    report(&mut w, B, 0, 0, &[lev(B1, "3.00"), lev(B2, "3.00")]);
    assert!(cap_params(&w, B).is_none(), "RSK-180.11 both fixed");
    let mut w = r0();
    report(&mut w, B, 0, 0, &[lev(B1, "3.00"), lev(B2, "3.00")]);
    assert_eq!(w.risk.leverage(acc(1), B, B1), Some(r("3.00")), "RSK-180.12");
    assert_eq!(w.risk.leverage(acc(1), B, C1), None, "RSK-180.12 c1");
}

#[test]
fn rsk_181_groups_changed_rechecks() {
    let mut w = r0();
    report(&mut w, C, 0, 0, &[lev(C1, "20.00")]);
    let g5 = GroupId(0x0100_0003);
    let mut specs = w.specs(1);
    specs.push(gspec(g5, BTC, &[(A1, A), (C1, C)]));
    w.risk.on_group_specs(acc(1), &specs);
    assert!(w.risk.lev_bad(acc(1), C), "step 1");
    w.ok(ModeInput::GroupsChanged {
        added: &[cell(1, 1, g5)],
        removed: &[],
    });
    assert_eq!(cap_params(&w, C), Some(cfg_params::LEVERAGE), "step 2");
    assert_eq!(
        w.out.notes.iter().filter(|x| note_n(x) == N_CAP_SET).count(),
        1,
        "step 2 note"
    );
    let specs = w.specs(1);
    w.risk.on_group_specs(acc(1), &specs);
    w.ok(ModeInput::GroupsChanged {
        added: &[],
        removed: &[cell(1, 1, g5)],
    });
    assert!(!w.risk.lev_bad(acc(1), C), "step 3");
    assert!(cap_params(&w, C).is_none(), "step 3 cap");
}
