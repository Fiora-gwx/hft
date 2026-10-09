//! Start-up gates: bit conditions, bitmaps, identity, AccountCfg, hedge and venue gates,
//! per-run reset (RSK-100–RSK-109).
#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods
)] // allow-list: test code, not a hot path

use crate::common;
use common::*;

use hft_event::NoteLevel;
use hft_refdata::UidPolicy;
use hft_risk::mode::{
    Disposition, DrainCause, GateChange, GateEvent, ModeInput, ModeOutcome, ReconItemKey, RecoveryFlags,
    cfg_params, gate_bits, mismatch,
};
use hft_risk::snapshot::{RiskUsRow, row_kind};
use hft_risk::{ModeCfg, Risk};
use hft_types::{AccountId, CapSource, GroupId, Side, Tif, VenueId};
use zerocopy::FromBytes;

const SESSION: u8 = 1;

fn gc(a: u8, old: u32, new: u32, released: bool) -> GateChange {
    GateChange {
        account: acc(a),
        old_bits: old,
        new_bits: new,
        released,
    }
}
fn changes(o: &ModeOutcome) -> Vec<GateChange> {
    o.gate_changes.iter().copied().collect()
}
fn hg(o: &ModeOutcome) -> Vec<(AccountId, VenueId)> {
    o.hedge_gate_open.iter().copied().collect()
}
fn vg(o: &ModeOutcome) -> Vec<(AccountId, VenueId)> {
    o.venue_gate_open.iter().copied().collect()
}
fn bits(w: &W, a: u8) -> u32 {
    w.risk.mode.gate_bits(acc(a))
}
fn has(w: &W, a: u8, bit: u32) -> bool {
    bits(w, a) & bit != 0
}
fn vbit(v: VenueId) -> u64 {
    1u64 << v.0
}
/// `identity_ok_venues` of an account.
fn ident(w: &W, a: u8) -> u64 {
    w.risk.mode.venue_bitmaps(acc(a))[2]
}

/// Replaces the step matching `pred` with `new` (or appends `new` when none matches).
fn replace(steps: &mut Vec<Step>, pred: impl Fn(&Step) -> bool, new: Vec<Step>) {
    if let Some(i) = steps.iter().position(pred) {
        steps.splice(i..=i, new);
    } else {
        steps.extend(new);
    }
}
fn is_conns(v: VenueId) -> impl Fn(&Step) -> bool {
    move |s| matches!(s, Step::Conns(x, _, _) if *x == v)
}
fn is_cfg(v: VenueId) -> impl Fn(&Step) -> bool {
    move |s| matches!(s, Step::Cfg(h, _, _) if h.venue == v)
}
fn is_recon(v: VenueId) -> impl Fn(&Step) -> bool {
    move |s| matches!(s, Step::Recon(x, _) if *x == v)
}
fn cfg_step(w: &W, a: u8, v: VenueId, mism: u32, keys: u8) -> Step {
    Step::Cfg(hdr(a, v, mism, keys), w.levs_for(v), KEYS)
}

#[test]
fn rsk_100_release_sequence() {
    let mut w = g0();
    let v = w.risk.mode.ctl_state_version();
    let steps = w.release_steps(1);
    let mut got = Vec::new();
    for s in &steps {
        w.step(1, s).unwrap();
        got.push(w.out.clone());
        assert_eq!(w.risk.mode.ctl_state_version(), v, "version at {s:?}");
    }
    // Feeds, Conns A B C, Unknown, Recon A B C, Clock, Cfg A B C, SelfTest.
    assert_eq!(steps.len(), 13);
    assert_eq!(changes(&got[0]), vec![gc(1, 0, 1, false)], "step 1");
    assert!(
        changes(&got[1]).is_empty() && changes(&got[2]).is_empty(),
        "step 2 A/B"
    );
    assert_eq!(changes(&got[3]), vec![gc(1, 1, 3, false)], "step 2 C");
    assert_eq!(changes(&got[4]), vec![gc(1, 3, 7, false)], "step 3");
    assert!(
        changes(&got[5]).is_empty() && changes(&got[6]).is_empty(),
        "step 4 A/B"
    );
    assert_eq!(changes(&got[7]), vec![gc(1, 7, 15, false)], "step 4 C");
    assert_eq!(changes(&got[8]), vec![gc(1, 15, 31, false)], "step 5");
    assert!(
        changes(&got[9]).is_empty() && changes(&got[10]).is_empty(),
        "step 6 A/B"
    );
    assert_eq!(changes(&got[11]), vec![gc(1, 31, 63, false)], "step 6 C");
    for o in &got[..12] {
        assert!(hg(o).is_empty() && vg(o).is_empty(), "gates before self-test");
    }
    let last = &got[12];
    assert_eq!(changes(last), vec![gc(1, 63, 127, true)], "step 7");
    assert_eq!(last.released.as_slice(), &[acc(1)]);
    let all = vec![(acc(1), A), (acc(1), B), (acc(1), C)];
    assert_eq!(hg(last), all);
    assert_eq!(vg(last), all);
    assert!(w.cap(CapSource::RECOVERY, sc_acct(1)).is_some());
}

/// G0 + the release steps edited by `edit`; returns the world.
fn g0_run(mc: ModeCfg, edit: &dyn Fn(&W, &mut Vec<Step>)) -> W {
    let mut w = g0_flags(0, mc);
    let mut steps = w.release_steps(1);
    edit(&w, &mut steps);
    for s in &steps {
        w.step(1, s).unwrap();
    }
    w
}

#[test]
fn rsk_101_gate_bit_conditions() {
    let not_set = |id: &str, w: &W, bit: u32| {
        assert!(!has(w, 1, bit), "{id}: bits {:#x}", bits(w, 1));
        assert!(!w.risk.mode.released(acc(1)), "{id} released");
    };
    let w = g0_run(MC, &|_, s| {
        replace(s, |x| matches!(x, Step::Feeds(_)), vec![Step::Feeds(false)])
    });
    not_set("RSK-101.1", &w, gate_bits::FEEDS);
    let w = g0_run(MC, &|_, s| {
        replace(s, is_conns(C), vec![Step::Conns(C, false, true)])
    });
    not_set("RSK-101.2", &w, gate_bits::ORDER_CONNS);
    let w = g0_run(MC, &|_, s| {
        replace(s, is_conns(C), vec![Step::Conns(C, true, false)])
    });
    not_set("RSK-101.3", &w, gate_bits::ORDER_CONNS);
    let allow = ModeCfg {
        allow_missing_standby: true,
        ..MC
    };
    let w = g0_run(allow, &|_, s| {
        replace(s, is_conns(C), vec![Step::Conns(C, true, false)])
    });
    assert!(
        has(&w, 1, gate_bits::ORDER_CONNS) && w.risk.mode.released(acc(1)),
        "RSK-101.4"
    );
    let mut w = g0_run(MC, &|_, s| {
        replace(s, |x| matches!(x, Step::Unknown(_)), vec![Step::Unknown(1)])
    });
    not_set("RSK-101.5 a", &w, gate_bits::NO_UNKNOWN);
    w.step(1, &Step::Unknown(0)).unwrap();
    assert!(has(&w, 1, gate_bits::NO_UNKNOWN), "RSK-101.5 b");
    let w = g0_run(MC, &|w, s| {
        let id = 9_001;
        let g = vec![
            summary(1, C, id, w.seq + 300, 0, 0),
            pos_item(1, C, id, C1, "0.002", "100"),
        ];
        replace(s, is_recon(C), vec![Step::Recon(C, g)]);
    });
    not_set("RSK-101.6", &w, gate_bits::RECONCILED);
    let w = g0_run(MC, &|_, s| {
        replace(s, |x| matches!(x, Step::Clock(_)), vec![Step::Clock(false)])
    });
    not_set("RSK-101.7", &w, gate_bits::CLOCK);
    for m in [mismatch::POS_MODE, mismatch::MARGIN_MODE, mismatch::MULTI_ASSET] {
        let w = g0_run(MC, &|w, s| {
            replace(s, is_cfg(B), vec![cfg_step(w, 1, B, m, KEYS)])
        });
        not_set(&format!("RSK-101.8 {m}"), &w, gate_bits::ACCOUNT_CFG);
    }
    for m in [
        mismatch::AUTO_BORROW,
        mismatch::FEE_RULE,
        mismatch::ACCT_TYPE,
        mismatch::KEY_PERM,
        mismatch::UNREADABLE,
    ] {
        let w = g0_run(MC, &|w, s| {
            replace(s, is_cfg(A), vec![cfg_step(w, 1, A, m, KEYS)])
        });
        not_set(&format!("RSK-101.9 {m}"), &w, gate_bits::ACCOUNT_CFG);
    }
    let w = g0_run(MC, &|w, s| {
        let mut h = hdr(1, B, 0, KEYS);
        h.position_mode = 3; // Hedge; VenueCaps requires OneWay
        replace(s, is_cfg(B), vec![Step::Cfg(h, w.levs_for(B), KEYS)]);
    });
    not_set("RSK-101.10", &w, gate_bits::ACCOUNT_CFG);
    // .11 / .12: account 1 also trades c1 (g4 = a1 + c1).
    let with_g4 = |w: &mut W| {
        let mut specs = w.specs(1);
        specs.push(gspec(GroupId(0x0100_0003), BTC, &[(A1, A), (C1, C)]));
        w.risk.on_group_specs(acc(1), &specs);
    };
    {
        let mut w = g0_flags(0, MC);
        with_g4(&mut w);
        let steps = w.release_steps(1);
        for s in &steps {
            if is_cfg(C)(s) {
                w.risk.on_account_cfg(&hdr(1, C, 0, KEYS), &[lev(C1, "3.01")]);
                let h = hdr(1, C, 0, KEYS);
                w.apply(ModeInput::Gate(GateEvent::AccountCfg {
                    hdr: &h,
                    keys_required: KEYS,
                }))
                .unwrap();
            } else {
                w.step(1, s).unwrap();
            }
        }
        not_set("RSK-101.11", &w, gate_bits::ACCOUNT_CFG);
        assert_ne!(
            w.risk.mode.venue_bitmaps(acc(1))[1] & vbit(C),
            0,
            "RSK-101.11 cfg_ok has C"
        );
    }
    {
        let mut w = g0_flags(0, MC);
        with_g4(&mut w);
        let mut steps = w.release_steps(1);
        replace(
            &mut steps,
            is_cfg(C),
            vec![Step::Cfg(hdr(1, C, 0, KEYS), Vec::new(), KEYS)],
        );
        for s in &steps {
            w.step(1, s).unwrap();
        }
        not_set("RSK-101.12", &w, gate_bits::ACCOUNT_CFG);
    }
    for (id, mism, keys) in [
        ("RSK-101.13", 0, SESSION),
        ("RSK-101.14", mismatch::UID | mismatch::UNREADABLE, KEYS),
    ] {
        let mut w = g0_flags(0, MC);
        let mut steps = w.release_steps(1);
        replace(&mut steps, is_cfg(C), vec![cfg_step(&w, 1, C, mism, keys)]);
        let mut drained = false;
        for s in &steps {
            w.step(1, s).unwrap();
            drained |= w.out.drain.is_some();
        }
        not_set(id, &w, gate_bits::IDENTITY);
        assert!(!drained, "{id} drain");
        assert!(w.cap(CapSource::KILL, sc_acct(1)).is_none(), "{id} kill");
    }
    let w = g0_run(MC, &|_, s| s.retain(|x| !matches!(x, Step::SelfTest)));
    not_set("RSK-101.15", &w, gate_bits::IDENTITY);
    {
        let mut w = r0();
        w.ok(auto(CapSource::RECOVERY, sc_acct(3)));
        w.ok(ModeInput::RecoveryFlags {
            account: acc(3),
            flags: RecoveryFlags(0),
        });
        assert_eq!(changes(&w.out), vec![gc(3, 127, 104, false)], "RSK-101.16 reset");
        let steps = [
            Step::Feeds(true),
            Step::Conns(A, true, true),
            Step::Conns(B, true, true),
            Step::Unknown(0),
            Step::Clock(true),
        ];
        for s in &steps {
            w.step(3, s).unwrap();
        }
        assert_eq!(w.out.released.as_slice(), &[acc(3)], "RSK-101.16 released");
    }
    {
        let mut w = r0();
        w.ok(auto(CapSource::RECOVERY, sc_acct(3)));
        w.ok(ModeInput::RecoveryFlags {
            account: acc(3),
            flags: RecoveryFlags(0),
        });
        w.step(3, &Step::Conns(A, true, true)).unwrap();
        assert_eq!(hg(&w.out), vec![(acc(3), A)], "RSK-101.17 hedge");
        assert_eq!(vg(&w.out), vec![(acc(3), A)], "RSK-101.17 venue");
    }
}

#[test]
fn rsk_102_bitmaps_track_latest() {
    let mut w = g0();
    let mut steps = w.release_steps(1);
    replace(
        &mut steps,
        is_conns(C),
        vec![Step::Conns(C, true, true), Step::Conns(C, false, true)],
    );
    for s in &steps {
        w.step(1, s).unwrap();
    }
    assert!(!has(&w, 1, gate_bits::ORDER_CONNS));
    assert!(!w.risk.mode.released(acc(1)));
    w.step(1, &Step::Conns(C, true, true)).unwrap();
    assert_eq!(w.out.released.as_slice(), &[acc(1)], "step 2");
    w.step(1, &Step::Conns(C, false, true)).unwrap();
    assert_eq!(changes(&w.out), vec![gc(1, 127, 125, true)], "step 3");
    assert!(w.risk.mode.released(acc(1)));
    assert!(!w.risk.mode.hedge_gate_ok(acc(1), C));
}

fn unverifiable_g0() -> W {
    let mut sp = r0_spec();
    for a in sp.accounts.iter_mut().filter(|a| a.id == acc(1)) {
        a.uid_policy = UidPolicy::Unverifiable;
    }
    let rd = load(&sp);
    let mut w = world(rd, p0_for(rd, 3), MC, CAPS);
    w.ok(auto(CapSource::RECOVERY, sc_acct(1)));
    w.ok(ModeInput::RecoveryFlags {
        account: acc(1),
        flags: RecoveryFlags(0),
    });
    w
}

fn unverified(o: &ModeOutcome) -> Vec<u64> {
    o.notes
        .iter()
        .filter(|x| note_n(x) == N_IDENTITY_UNVERIFIED)
        .map(|x| {
            assert_eq!(x.level, NoteLevel::P1);
            x.v[0]
        })
        .collect()
}

#[test]
fn rsk_103_uid_unverifiable() {
    // Self-test first, then the reports.
    let mut w = unverifiable_g0();
    let steps: Vec<Step> = w
        .release_steps(1)
        .into_iter()
        .filter(|s| !matches!(s, Step::Cfg(..)))
        .collect();
    for s in &steps {
        w.step(1, s).unwrap();
    }
    let rep_a = cfg_step(&w, 1, A, mismatch::UNREADABLE, 0);
    w.step(1, &rep_a).unwrap();
    assert_ne!(ident(&w, 1) & vbit(A), 0, "RSK-103.1");
    assert_eq!(unverified(&w.out), vec![258], "RSK-103.1 note");
    w.step(1, &rep_a).unwrap();
    assert!(unverified(&w.out).is_empty(), "RSK-103.2");
    assert_eq!(ident(&w, 1) & vbit(C), 0, "RSK-103.3");
    assert!(!has(&w, 1, gate_bits::IDENTITY), "RSK-103.3 bit");
    w.step(1, &cfg_step(&w, 1, C, 0, KEYS)).unwrap();
    assert_ne!(ident(&w, 1) & vbit(C), 0, "RSK-103.4");
    assert_eq!(unverified(&w.out), vec![261], "RSK-103.4 note");
    w.step(1, &cfg_step(&w, 1, B, mismatch::UID, KEYS)).unwrap();
    let k = w.cap(CapSource::KILL, sc_acct(1)).expect("RSK-103.5 kill");
    assert_ne!(k.cflags & hft_event::CapRecord::HARD, 0);
    assert_eq!(
        w.out.drain,
        Some(DrainCause::IdentityMismatch {
            account: acc(1),
            venue: B
        }),
        "RSK-103.5"
    );
    assert_eq!(ident(&w, 1) & vbit(B), 0, "RSK-103.5 B");
    // .6: every report before the self-test.
    let mut w = unverifiable_g0();
    let steps: Vec<Step> = w
        .release_steps(1)
        .into_iter()
        .filter(|s| !matches!(s, Step::SelfTest))
        .collect();
    let mut notes = 0;
    for s in &steps {
        if let Step::Cfg(h, l, _) = s {
            let x = Step::Cfg(*h, l.clone(), 0);
            let _ = x;
        }
        w.step(1, s).unwrap();
        notes += unverified(&w.out).len();
    }
    assert_eq!((ident(&w, 1), notes), (0, 0), "RSK-103.6 before");
    w.step(1, &Step::SelfTest).unwrap();
    assert_eq!(ident(&w, 1), vbit(A) | vbit(B) | vbit(C), "RSK-103.6 after");
    assert_eq!(unverified(&w.out).len(), 3, "RSK-103.6 notes");
}

#[test]
fn rsk_104_uid_mismatch_hard_and_drain() {
    let check = |id: &str, w: &W| {
        let k = w.cap(CapSource::KILL, sc_acct(1)).expect(id);
        assert_ne!(k.cflags & hft_event::CapRecord::HARD, 0, "{id}");
        let sets: Vec<_> = w
            .out
            .notes
            .iter()
            .filter(|x| note_n(x) == N_CAP_SET)
            .map(|x| x.level)
            .collect();
        assert_eq!(sets, vec![NoteLevel::P1], "{id} note");
        assert!(w.out.kill_changed, "{id}");
        for b in w.risk.mode.kill_bits() {
            assert_eq!(b.0 & 0x0010_0100, 0x0010_0100, "{id} kill bits {:#x}", b.0);
        }
        let c: Vec<_> = w.out.cancels.iter().map(|c| (c.scope, c.open_only)).collect();
        assert_eq!(
            c,
            vec![(hft_risk::mode::CancelScope::Account(acc(1)), false)],
            "{id}"
        );
        assert_eq!(
            w.out.drain,
            Some(DrainCause::IdentityMismatch {
                account: acc(1),
                venue: C
            }),
            "{id}"
        );
        assert_eq!(ident(w, 1) & vbit(C), 0, "{id}");
    };
    let mut w = g0();
    let mut steps = w.release_steps(1);
    let at = steps.iter().position(is_cfg(C)).unwrap();
    steps[at] = cfg_step(&w, 1, C, mismatch::UID, KEYS);
    for s in &steps[..=at] {
        w.step(1, s).unwrap();
    }
    check("RSK-104.1", &w);
    let mut w = r0();
    w.step(1, &cfg_step(&w, 1, C, mismatch::UID, KEYS)).unwrap();
    check("RSK-104.2", &w);
    assert_eq!(changes(&w.out), vec![gc(1, 127, 63, true)], "RSK-104.2 gate");
    let mut w = r0();
    w.step(1, &cfg_step(&w, 1, C, mismatch::UID | mismatch::UNREADABLE, KEYS))
        .unwrap();
    assert!(w.cap(CapSource::KILL, sc_acct(1)).is_none(), "RSK-104.3");
    assert_eq!(w.out.drain, None, "RSK-104.3");
    assert_eq!(ident(&w, 1) & vbit(C), 0, "RSK-104.3");
}

#[test]
fn rsk_105_account_cfg_after_release() {
    let mut w = r0();
    w.step(1, &cfg_step(&w, 1, B, mismatch::MARGIN_MODE, KEYS))
        .unwrap();
    let c = w.cap(CapSource::ACCOUNT_CFG, sc_av(1, B)).expect("AccountCfg");
    assert_eq!(c.params[0], cfg_params::MISMATCH);
    assert_eq!(
        w.out
            .notes
            .iter()
            .filter(|x| note_n(x) == N_CAP_SET)
            .map(|x| x.level)
            .collect::<Vec<_>>(),
        vec![NoteLevel::P1]
    );
    assert_eq!(changes(&w.out), vec![gc(1, 127, 95, true)]);
    let ob = open(1, G1, B1, Side::Buy, "0.01", "50000.0", Tif::Gtc);
    assert_eq!(w.eval(&ob).map(|_| ()), Err(hft_types::RejectBits::CLOSE_ONLY));
    assert!(
        w.eval(&open(1, G1, A1, Side::Buy, "0.01", "50000.00", Tif::Gtc))
            .is_ok()
    );
    w.step(1, &cfg_step(&w, 1, B, 0, KEYS)).unwrap();
    assert!(w.cap(CapSource::ACCOUNT_CFG, sc_av(1, B)).is_none(), "step 2");
    assert_eq!(
        w.out
            .notes
            .iter()
            .filter(|x| note_n(x) == N_CAP_CLEARED)
            .map(|x| x.level)
            .collect::<Vec<_>>(),
        vec![NoteLevel::Info]
    );
    w.step(1, &cfg_step(&w, 1, A, mismatch::AUTO_BORROW, KEYS))
        .unwrap();
    assert_eq!(
        w.cap(CapSource::ACCOUNT_CFG, sc_av(1, A)).expect("step 3").params[0],
        cfg_params::MISMATCH
    );
}

#[test]
fn rsk_106_hedge_gate_subset() {
    // Runs steps; returns every apply's hedge_gate_open.
    let run = |w: &mut W, steps: &[Step]| -> Vec<Vec<(AccountId, VenueId)>> {
        let mut all = Vec::new();
        for s in steps {
            w.step(1, s).unwrap();
            all.push(hg(&w.out));
        }
        all
    };
    let a_ok = |w: &W| {
        vec![
            Step::SelfTest,
            cfg_step(w, 1, A, 0, KEYS),
            Step::Conns(A, true, true),
        ]
    };
    let mut w = g0();
    let s = a_ok(&w);
    let got = run(&mut w, &s);
    assert_eq!(got.last().unwrap(), &vec![(acc(1), A)], "RSK-106.1");
    assert!(w.out.released.is_empty(), "RSK-106.1 released");
    let mut w = g0();
    let s = vec![
        Step::SelfTest,
        cfg_step(&w, 1, A, 0, KEYS),
        Step::Conns(A, true, false),
    ];
    let got = run(&mut w, &s);
    assert_eq!(got.last().unwrap(), &vec![(acc(1), A)], "RSK-106.2");
    assert!(!has(&w, 1, gate_bits::ORDER_CONNS));
    let mut w = g0();
    w.step(1, &Step::SelfTest).unwrap();
    w.risk
        .on_account_cfg(&hdr(1, B, 0, KEYS), &[lev(B1, "3.01"), lev(B2, "3.00")]);
    let h = hdr(1, B, 0, KEYS);
    w.apply(ModeInput::Gate(GateEvent::AccountCfg {
        hdr: &h,
        keys_required: KEYS,
    }))
    .unwrap();
    w.step(1, &Step::Conns(B, true, true)).unwrap();
    assert_eq!(hg(&w.out), vec![(acc(1), B)], "RSK-106.3");
    assert!(!has(&w, 1, gate_bits::ACCOUNT_CFG));
    let mut w = g0();
    let mut s = a_ok(&w);
    s.push(Step::Conns(C, false, true));
    let got = run(&mut w, &s);
    assert!(!w.risk.mode.hedge_gate_ok(acc(1), C), "RSK-106.4");
    assert!(
        got.iter().flatten().all(|(_, v)| *v != C),
        "RSK-106.4 never listed"
    );
    let mut w = g0();
    let s = a_ok(&w);
    run(&mut w, &s);
    w.step(1, &Step::Conns(A, false, true)).unwrap();
    assert!(
        hg(&w.out).is_empty() && !w.risk.mode.hedge_gate_ok(acc(1), A),
        "RSK-106.5 down"
    );
    w.step(1, &Step::Conns(A, true, true)).unwrap();
    assert_eq!(hg(&w.out), vec![(acc(1), A)], "RSK-106.5 up");
    let mut w = g0();
    let s = vec![cfg_step(&w, 1, A, 0, KEYS), Step::Conns(A, true, true)];
    let got = run(&mut w, &s);
    assert!(
        got.iter().all(|x| x.is_empty()) && !w.risk.mode.hedge_gate_ok(acc(1), A),
        "RSK-106.6"
    );
    for (id, m) in [
        ("RSK-106.7", mismatch::POS_MODE),
        ("RSK-106.8", mismatch::UID | mismatch::UNREADABLE),
    ] {
        let mut w = g0();
        let s = vec![
            Step::SelfTest,
            cfg_step(&w, 1, A, m, KEYS),
            Step::Conns(A, true, true),
        ];
        let got = run(&mut w, &s);
        assert!(got.iter().all(|x| x.is_empty()), "{id}");
    }
}

/// G0 with self-test, consistent reports and connections of A, B, C (no Feeds).
fn hedge_ready() -> W {
    let mut w = g0();
    let s = vec![
        Step::SelfTest,
        cfg_step(&w, 1, A, 0, KEYS),
        cfg_step(&w, 1, B, 0, KEYS),
        cfg_step(&w, 1, C, 0, KEYS),
        Step::Conns(A, true, true),
        Step::Conns(B, true, true),
        Step::Conns(C, true, true),
    ];
    for x in &s {
        w.step(1, x).unwrap();
    }
    w
}

fn ignore_group(w: &W, v: VenueId, id: u64) -> Step {
    Step::Recon(v, vec![summary(1, v, id, w.seq + 500, 0, 0)])
}

#[test]
fn rsk_107_venue_gate() {
    let w = hedge_ready();
    for v in [A, B, C] {
        assert!(w.risk.mode.hedge_gate_ok(acc(1), v));
    }
    assert!(!w.risk.mode.venue_gate_ok(acc(1), B), "RSK-107.1");
    let mut w = hedge_ready();
    w.step(1, &ignore_group(&w, B, 7_001)).unwrap();
    assert_eq!(vg(&w.out), vec![(acc(1), B)], "RSK-107.2");
    assert!(w.out.released.is_empty());
    let mut w3 = w.clone();
    w3.step(1, &Step::Conns(C, false, true)).unwrap();
    assert!(
        w3.risk.mode.venue_gate_ok(acc(1), B) && vg(&w3.out).is_empty(),
        "RSK-107.3"
    );
    w.step(1, &Step::Conns(B, false, true)).unwrap();
    assert!(!w.risk.mode.venue_gate_ok(acc(1), B), "RSK-107.4 down");
    w.step(1, &Step::Conns(B, true, true)).unwrap();
    assert_eq!(vg(&w.out), vec![(acc(1), B)], "RSK-107.4 up");
    let mut w = hedge_ready();
    let g = vec![
        summary(1, B, 7_002, w.seq + 500, 0, 0),
        pos_item(1, B, 7_002, B1, "0.002", "100"),
    ];
    w.step(1, &Step::Recon(B, g)).unwrap();
    assert!(vg(&w.out).is_empty(), "RSK-107.5 undisposed");
    w.apply(ModeInput::Gate(GateEvent::Disposed {
        account: acc(1),
        venue: B,
        report_id: 7_002,
        item: ReconItemKey::new(ReconItemKey::POSITION, B1.0),
        how: Disposition::Adopted,
    }))
    .unwrap();
    assert_eq!(vg(&w.out), vec![(acc(1), B)], "RSK-107.5 disposed");
}

#[test]
fn rsk_108_clock_releases_many() {
    let mut w = r0();
    for a in [1u8, 2] {
        w.ok(auto(CapSource::RECOVERY, sc_acct(a)));
        w.ok(ModeInput::RecoveryFlags {
            account: acc(a),
            flags: RecoveryFlags(0),
        });
        let steps: Vec<Step> = w
            .release_steps(a)
            .into_iter()
            .filter(|s| !matches!(s, Step::Clock(_)))
            .collect();
        for s in &steps {
            w.step(a, s).unwrap();
        }
    }
    w.ok(ModeInput::Gate(GateEvent::Clock { ok: true }));
    let mut rel: Vec<u8> = w.out.released.iter().map(|a| a.0).collect();
    rel.sort_unstable();
    assert_eq!(rel, vec![1, 2]);
    let mut c = changes(&w.out);
    c.sort_by_key(|g| g.account);
    assert_eq!(c, vec![gc(1, 111, 127, true), gc(2, 111, 127, true)]);
}

#[test]
fn rsk_109_per_run_reset() {
    let mut w = r0();
    let g = vec![summary(1, B, 5, w.seq + 20, 0, 0)];
    w.step(1, &Step::Recon(B, g)).unwrap();
    w.step(1, &Step::Conns(B, false, true)).unwrap();
    let s = 40.max(w.seq);
    w.apply_seq(
        ModeInput::Gate(GateEvent::OrderConns {
            account: acc(1),
            venue: B,
            primary_up: true,
            standby_up: true,
        }),
        s,
    )
    .unwrap();
    let us_rows = |bytes: &[u8], a: u8| -> Vec<(u8, RiskUsRow)> {
        section17(bytes)
            .into_iter()
            .filter(|r| r.h.kind == row_kind::USER_STREAM && r.h.account == a)
            .map(|r| (r.h.venue, RiskUsRow::read_from_bytes(&r.body).unwrap()))
            .collect()
    };
    let before = w.snapshot();
    assert!(
        us_rows(&before, 1)
            .iter()
            .any(|(v, r)| *v == B.0 && r.us_up_seq == s)
    );
    let rows2: Vec<_> = section17(&before)
        .into_iter()
        .filter(|r| r.h.account == 2)
        .map(|r| (r.h, r.body))
        .collect();
    let bits2 = bits(&w, 2);
    w.ok(ModeInput::RecoveryFlags {
        account: acc(1),
        flags: RecoveryFlags(0),
    });
    assert_eq!(changes(&w.out), vec![gc(1, 127, 0, false)], "step 1");
    for v in [A, B, C] {
        assert!(
            !w.risk.mode.hedge_gate_ok(acc(1), v) && !w.risk.mode.venue_gate_ok(acc(1), v),
            "{v:?}"
        );
    }
    let after = w.snapshot();
    assert!(
        !section17(&after)
            .iter()
            .any(|r| r.h.kind == row_kind::RECON_GATE && r.h.account == 1),
        "no ReconGate row of account 1"
    );
    assert!(
        us_rows(&after, 1)
            .iter()
            .all(|(v, r)| *v != B.0 || r.us_up_seq == 0),
        "us_up_seq reset"
    );
    let rows2b: Vec<_> = section17(&after)
        .into_iter()
        .filter(|r| r.h.account == 2)
        .map(|r| (r.h, r.body))
        .collect();
    assert_eq!(rows2, rows2b, "account 2 rows");
    assert_eq!(bits(&w, 2), bits2);
    w.ok(ModeInput::Gate(GateEvent::Disposed {
        account: acc(1),
        venue: B,
        report_id: 5,
        item: ReconItemKey::new(ReconItemKey::POSITION, B1.0),
        how: Disposition::Adopted,
    }));
    assert!(!has(&w, 1, gate_bits::RECONCILED), "step 2");
    let _ = Risk::notional;
}
