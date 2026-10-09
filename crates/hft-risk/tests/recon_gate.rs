//! Reconciliation gate and the strict HARD-release condition (RSK-130, RSK-131).
#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods
)] // allow-list: test code, not a hot path

use crate::common;
use common::*;

use hft_event::ReconcileReport;
use hft_risk::mode::{Disposition, GateEvent, ModeErr, ModeInput, ReconItemKey};
use hft_types::{AssetId, CapSource, InstId, KillLevel, Qty, VenueId};

fn has_b(w: &W) -> bool {
    w.risk.mode.venue_bitmaps(acc(1))[0] & (1u64 << B.0) != 0
}

fn dispose(w: &mut W, v: VenueId, report_id: u64, item: u8, id: u32, how: Disposition) {
    w.ok(ModeInput::Gate(GateEvent::Disposed {
        account: acc(1),
        venue: v,
        report_id,
        item: ReconItemKey::new(item, id),
        how,
    }));
}

/// G0 with every release step except B's reconciliation group.
fn without_b() -> W {
    let mut w = g0();
    let steps: Vec<Step> = w
        .release_steps(1)
        .into_iter()
        .filter(|s| !matches!(s, Step::Recon(v, _) if *v == B))
        .collect();
    for s in &steps {
        w.step(1, s).unwrap();
    }
    w
}

fn group(w: &mut W, v: VenueId, id: u64, vs: u64, sflags: u16, skipped: u16, items: Vec<ReconcileReport>) {
    let mut g = vec![summary(1, v, id, vs, sflags, skipped)];
    g.extend(items);
    w.step(1, &Step::Recon(v, g)).unwrap();
}

fn funding(a: u8, v: VenueId, id: u64, inst: InstId) -> ReconcileReport {
    let mut x = rep(a, v, id, 0, 6);
    x.inst = inst;
    x.exch_qty = q("1");
    x.diff = n("5");
    x
}

#[test]
fn rsk_130_reconciled_strict() {
    use Disposition::*;
    let pos = ReconItemKey::POSITION;
    let vs = |w: &W| w.seq + 1_000;
    // .1
    let mut w = without_b();
    let s = vs(&w);
    group(&mut w, B, 7, s, 0, 0, vec![]);
    assert!(has_b(&w), "RSK-130.1");
    assert_eq!(w.out.released.as_slice(), &[acc(1)], "RSK-130.1 released");
    // .12: a new group with a Warn removes B; disposals of 7 are void.
    let s = vs(&w);
    group(&mut w, B, 8, s, 0, 0, vec![pos_item(1, B, 8, B1, "0.002", "100")]);
    assert!(!has_b(&w), "RSK-130.12");
    dispose(&mut w, B, 7, pos, B1.0, Adopted);
    assert!(!has_b(&w), "RSK-130.12 old disposal");
    // .2
    let mut w = without_b();
    let s = vs(&w);
    group(&mut w, B, 7, s, 0, 0, vec![pos_item(1, B, 7, B1, "0.002", "100")]);
    assert!(!has_b(&w), "RSK-130.2");
    dispose(&mut w, B, 7, pos, B1.0, Adopted);
    assert!(has_b(&w), "RSK-130.2 disposed");
    // .3
    let mut w = without_b();
    let s = vs(&w);
    group(&mut w, B, 7, s, 0, 0, vec![bal_item(1, B, 7, USDT, "1", "1")]);
    assert!(!has_b(&w), "RSK-130.3");
    dispose(&mut w, B, 7, ReconItemKey::BALANCE, u32::from(USDT.0), Baselined);
    assert!(has_b(&w), "RSK-130.3 disposed");
    // .4
    let mut w = without_b();
    let s = vs(&w);
    let mut ae = pos_item(1, B, 7, B1, "0.002", "100");
    ae.sflags = ReconcileReport::AUTO_EXTERNAL;
    group(&mut w, B, 7, s, 0, 0, vec![ae]);
    dispose(&mut w, B, 7, pos, B1.0, AutoExternal);
    assert!(has_b(&w), "RSK-130.4");
    // .5
    let mut w = without_b();
    let s = vs(&w);
    group(
        &mut w,
        B,
        7,
        s,
        0,
        0,
        vec![
            pos_item(1, B, 7, B1, "0.002", "100"),
            pos_item(1, B, 7, B2, "0.02", "50"),
        ],
    );
    dispose(&mut w, B, 7, pos, B1.0, Adopted);
    assert!(!has_b(&w), "RSK-130.5 one");
    dispose(&mut w, B, 7, pos, B2.0, Adopted);
    assert!(has_b(&w), "RSK-130.5 both");
    // .6
    let mut w = without_b();
    let s = vs(&w);
    group(&mut w, B, 7, s, 0, 0, vec![pos_item(1, B, 7, B1, "0.002", "100")]);
    dispose(&mut w, B, 6, pos, B1.0, Adopted);
    assert!(!has_b(&w), "RSK-130.6");
    // .7 – .9
    for (id, sflags, skipped) in [
        ("RSK-130.7", ReconcileReport::TRUNCATED, 0u16),
        ("RSK-130.8", ReconcileReport::USERSTREAM_DOWN, 0),
        ("RSK-130.9", 0, 1),
    ] {
        let mut w = without_b();
        let s = vs(&w);
        group(&mut w, B, 7, s, sflags, skipped, vec![]);
        assert!(!has_b(&w), "{id}");
    }
    // .10
    let mut w = without_b();
    let s = vs(&w);
    let mut orphan = rep(1, B, 7, 0, 3);
    orphan.inst = B1;
    group(&mut w, B, 7, s, 0, 0, vec![orphan]);
    assert!(!has_b(&w), "RSK-130.10");
    let s = vs(&w);
    group(&mut w, B, 8, s, 0, 0, vec![]);
    assert!(has_b(&w), "RSK-130.10 next");
    // .11
    let mut w = without_b();
    let s = vs(&w);
    group(
        &mut w,
        B,
        7,
        s,
        0,
        0,
        vec![funding(1, B, 7, B1), funding(1, B, 7, InstId(0))],
    );
    assert!(has_b(&w), "RSK-130.11");
    // .13 / .14: 64 and 65 Warn items (distinct position keys).
    let warns = |id: u64, k: u32| -> Vec<ReconcileReport> {
        (0..k)
            .map(|j| {
                let mut x = pos_item(1, B, id, InstId(100 + j), "0", "1");
                x.exch_qty = Qty(1);
                x
            })
            .collect()
    };
    let mut w = without_b();
    let s = vs(&w);
    group(&mut w, B, 7, s, 0, 0, warns(7, 64));
    for j in 0..64 {
        dispose(&mut w, B, 7, pos, 100 + j, Adopted);
    }
    assert!(has_b(&w), "RSK-130.13");
    let mut w = without_b();
    let s = vs(&w);
    group(&mut w, B, 7, s, 0, 0, warns(7, 65));
    for j in 0..64 {
        dispose(&mut w, B, 7, pos, 100 + j, Adopted);
    }
    assert!(!has_b(&w), "RSK-130.14");
    let s = vs(&w);
    group(&mut w, B, 8, s, 0, 0, vec![]);
    assert!(has_b(&w), "RSK-130.14 next");
    let _ = AssetId(0);
}

#[test]
fn rsk_131_hard_release_recon() {
    const S: u64 = 10_000;
    let base = || {
        let mut w = r0();
        w.apply_seq(kill_acct(1, true), S).unwrap();
        w
    };
    let res = |w: &mut W| w.apply(resume(CapSource::KILL, sc_acct(1)));
    let mut id = 20_000u64;
    let mut next = || {
        id += 1;
        id
    };
    // .1
    let mut w = base();
    assert_eq!(res(&mut w), Err(ModeErr::ReconNotClean), "RSK-131.1");
    assert!(w.cap(CapSource::KILL, sc_acct(1)).is_some());
    // .2
    let mut w = base();
    for v in [A, B, C] {
        group(&mut w, v, next(), S + 1, 0, 0, vec![]);
    }
    assert_eq!(res(&mut w), Ok(()), "RSK-131.2");
    assert!(w.out.resolve_broken && w.out.kill_changed);
    for b in w.risk.mode.kill_bits() {
        assert_eq!(b.0 & 0x0010_0100, 0, "account 1 bits cleared");
    }
    // .3
    let mut w = base();
    group(&mut w, A, next(), S + 1, 0, 0, vec![]);
    group(&mut w, B, next(), S + 1, 0, 0, vec![]);
    group(&mut w, C, next(), S - 1, 0, 0, vec![]);
    assert_eq!(res(&mut w), Err(ModeErr::ReconNotClean), "RSK-131.3");
    // .4
    let mut w = base();
    group(&mut w, A, next(), S + 1, 0, 0, vec![]);
    group(&mut w, B, next(), S + 1, 0, 0, vec![]);
    group(&mut w, C, next(), S, 0, 0, vec![]);
    assert_eq!(res(&mut w), Err(ModeErr::ReconNotClean), "RSK-131.4 a");
    group(&mut w, C, next(), S + 1, 0, 0, vec![]);
    assert_eq!(res(&mut w), Ok(()), "RSK-131.4 b");
    // .5
    let mut w = base();
    group(&mut w, A, next(), S + 1, 0, 0, vec![]);
    group(&mut w, B, next(), S + 1, 0, 0, vec![]);
    let gid = next();
    group(
        &mut w,
        C,
        gid,
        S + 1,
        0,
        0,
        vec![pos_item(1, C, gid, B1, "0.002", "100")],
    );
    dispose(&mut w, C, gid, ReconItemKey::POSITION, B1.0, Disposition::Adopted);
    assert_eq!(res(&mut w), Ok(()), "RSK-131.5");
    // .6
    let mut w = base();
    group(&mut w, A, next(), S + 1, 0, 0, vec![]);
    group(&mut w, B, next(), S + 1, 0, 0, vec![]);
    let gid = next();
    group(&mut w, C, gid, S + 1, 0, 0, vec![funding(1, C, gid, C1)]);
    assert_eq!(res(&mut w), Ok(()), "RSK-131.6");
    // .7
    let mut w = base();
    group(&mut w, A, next(), S + 1, 0, 0, vec![]);
    group(&mut w, B, next(), S + 1, 0, 0, vec![]);
    group(&mut w, C, next(), S + 1, 0, 0, vec![]);
    let gid = next();
    group(
        &mut w,
        C,
        gid,
        S + 50,
        0,
        0,
        vec![pos_item(1, C, gid, C1, "0.002", "100")],
    );
    assert_eq!(res(&mut w), Err(ModeErr::ReconNotClean), "RSK-131.7");
    // .8
    let mut w = r0();
    w.apply_seq(kill(KillLevel::Global, sc_any(), true, false, true), S)
        .unwrap();
    for (a, vs) in [(1u8, vec![A, B, C]), (2, vec![A, B])] {
        for v in vs {
            let gid = next();
            w.step(a, &Step::Recon(v, vec![summary(a, v, gid, S + 1, 0, 0)]))
                .unwrap();
        }
    }
    assert_eq!(w.apply(resume(CapSource::KILL, sc_any())), Ok(()), "RSK-131.8");
    // .9
    let mut w = r0();
    w.ok(kill_acct(1, false));
    assert_eq!(res(&mut w), Ok(()), "RSK-131.9");
}
