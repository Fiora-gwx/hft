//! EXE-010–EXE-017: cell table, `touch_inst`, `view`, `has_pending`, Live-before-Sim.
mod common;
use common::*;
use hft_exec::*;
use hft_strategy_sdk::{ExecUpdateKind, TargetOrigin, TargetReject, TargetState};

#[test]
fn exe_010_cix_order() {
    let g9 = gspec(G9, &[(S, A), (P, B)]);
    let mut fx = Fx::with(c0(), vec![g9, g8(), g7()]);
    assert_eq!(fx.ex.cix(A1, G7), Some(0));
    fx.set(I1, &taker(A1, G7, &[(0, "10")])).0.unwrap();
    fx.set(I1, &taker(A1, G8, &[(0, "-10")])).0.unwrap();
    fx.set(I2, &taker(A2, G9, &[(0, "10")])).0.unwrap();
    let r = fx.adv();
    let ids: Vec<(u32, u16)> = r.orders.iter().map(|o| (o.intent.group.0, o.id.cix)).collect();
    assert_eq!(ids, vec![(G7.0, 0), (G8.0, 1), (G9.0, 2)]);
    let gs: Vec<u32> = r.dec(5).iter().map(|d| d.group.0).collect();
    assert_eq!(gs, vec![G7.0, G8.0, G9.0]);
    // Step 2: ungrouped perp P −3.0 of a1.
    fx.perp(A1, B, P, "-3.0");
    fx.at(1000);
    fx.force(A1, B, "0.2");
    let r = fx.adv();
    let u: Vec<&ExecOrder> = r.orders.iter().filter(|o| o.intent.group == GNONE).collect();
    assert_eq!(u.len(), 1);
    assert_eq!(u[0].id.cix, 3);
    assert!(r.decisions.iter().any(|d| d.group == GNONE && d.action == 11));
}

#[test]
fn exe_011_groups_changed_keeps_state() {
    let mut fx = Fx::new();
    inject(&mut fx, &t0("10", "-10"));
    {
        let c = fx.cm(A1, G7);
        c.view.filled = [q("2"), q("-2"), q("0"), q("0")];
        c.fix.count = 1;
        c.fail_streak = 1;
    }
    let v0 = *fx.ex.view(A1, G7).unwrap();
    let x = fx.snap();
    let r = regroup(&mut fx, vec![gspec(G6, &[(S, A), (P, B)]), g7(), g8()]);
    assert!(r.is_empty());
    let v1 = *fx.ex.view(A1, G7).unwrap();
    assert_eq!((v1.intent, v1.filled, v1.state), (v0.intent, v0.filled, v0.state));
    let y = fx.snap();
    assert_eq!(cell_row(&x, A1, G7), cell_row(&y, A1, G7));
    assert!(cell_row(&y, A1, G7).is_some());
}

#[test]
fn exe_012_leg_change_clears_target() {
    let mut fx = Fx::new();
    inject(&mut fx, &t0("10", "-10"));
    let r = regroup(&mut fx, vec![gspec(G7, &[(S, A), (P3, B)]), g8()]);
    assert!(r.is_empty());
    assert!(fx.ex.has_pending());
    let r = fx.adv();
    assert_eq!(r.dec(3).iter().filter(|d| d.group == G7).count(), 1);
    let v = fx.ex.view(A1, G7).unwrap();
    assert_eq!(v.state, TargetState::Idle);
    assert!(!fx.cell(A1, G7).has_target);
    assert!(r.orders.is_empty());
}

#[test]
fn exe_013_removed_group_not_owner() {
    let mut fx = Fx::new();
    inject(&mut fx, &t1("-10"));
    regroup(&mut fx, vec![g7()]);
    assert!(fx.ex.view(A1, G8).is_none());
    let (res, r) = fx.set(I1, &t1("-10"));
    assert_eq!(res, Err(TargetReject::NotOwner));
    assert!(r.has_dec(4, 1 | 1 << 8));
}

#[test]
fn exe_014_touch_inst_live_only() {
    let mut fx = Fx::new();
    inject(&mut fx, &t1("-10"));
    assert!(!fx.ex.has_pending());
    fx.touch(S);
    assert!(!fx.ex.has_pending());
    fx.touch(P);
    assert!(fx.ex.has_pending());
    let r = fx.adv();
    assert!(!r.decisions.is_empty());
    assert!(r.decisions.iter().all(|d| d.group == G8));
}

#[test]
fn exe_015_view_projection() {
    let mut fx = Fx::new();
    fx.seq = 99;
    let tg = t1("-10.0");
    let (res, _) = fx.set(I1, &tg);
    res.unwrap();
    let r = fx.adv();
    let o = r.orders[0];
    let rf = fx.placed(&o);
    fx.at(10);
    let r = fx.fill(rf, G8, "4.0", true);
    assert!(
        r.upd()
            .iter()
            .any(|u| matches!(u, ExecUpdateKind::Progress { leg_ix: 0, .. }))
    );
    let v = fx.ex.view(A1, G8).unwrap();
    assert_eq!(v.intent, tg);
    assert_eq!(v.origin, TargetOrigin::Strategy);
    assert_eq!(v.state, TargetState::Working);
    assert_eq!(v.set_seq, 100);
    assert_eq!(v.n_slices, 1);
    assert_eq!(v.filled[0], q("-4.0"));
    assert!(fx.ex.view(A1, hft_types::GroupId(0x0100_0099)).is_none());
}

#[test]
fn exe_016_has_pending_cleared() {
    let mut fx = Fx::new();
    fx.set(I1, &t0("10", "-10")).0.unwrap();
    fx.set(I1, &t1("-10")).0.unwrap();
    assert!(fx.ex.has_pending());
    fx.adv();
    assert!(!fx.ex.has_pending());
}

#[test]
fn exe_017_live_before_sim() {
    let mut c = cfg(c0());
    c.accounts[0].sim = true;
    let g9 = gspec(G9, &[(P, B)]);
    let mut fx = Fx::with_cfg(c, vec![g7(), g8(), g9], default_insts());
    fx.set(I1, &t1("-10")).0.unwrap();
    fx.set(I2, &taker(A2, G9, &[(0, "-10")])).0.unwrap();
    let r = fx.adv();
    let accts: Vec<u8> = r.orders.iter().map(|o| o.intent.account.0).collect();
    assert_eq!(accts, vec![2, 1]);
    let first_a1 = r.decisions.iter().position(|d| d.account == A1).unwrap();
    let last_a2 = r.decisions.iter().rposition(|d| d.account == A2).unwrap();
    assert!(last_a2 < first_a1);
}
