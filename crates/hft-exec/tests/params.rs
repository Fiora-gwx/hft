//! EXE-040–EXE-041: tag propagation, PairParams into the first leg's LegLink.
use crate::common;
use common::*;
use hft_exec::*;
use hft_strategy_sdk::{ExecSpec, ExecUpdateKind, HedgeKind, LinkRole, OnFail, PxMode, ResidualPolicy};
use hft_types::Side;

#[test]
fn exe_040_tag_propagates() {
    let mut fx = Fx::new();
    let mut tg = t1("-10.0");
    tg.tag = 0xDEAD_BEEF;
    let r = fx.go(&tg);
    assert_eq!(r.dec(1)[0].tag, 0xDEAD_BEEF);
    assert_eq!(r.dec(5)[0].tag, 0xDEAD_BEEF);
    assert_eq!(r.orders[0].intent.tag, 0xDEAD_BEEF);
    let rf = fx.placed(&r.orders[0]);
    let r = fx.fill(rf, G8, "4.0", true);
    let n = r
        .notify
        .iter()
        .find(|n| matches!(n.u.kind, ExecUpdateKind::Progress { .. }))
        .unwrap();
    assert_eq!(
        n.u.kind,
        ExecUpdateKind::Progress {
            leg_ix: 0,
            d: q("-4.0")
        }
    );
    assert_eq!(n.u.tag, 0xDEAD_BEEF);
}

#[test]
fn exe_041_pair_params_into_leglink() {
    let mut fx = Fx::new();
    let mut p = pair0();
    p.ratio_num = 2;
    p.fix_tolerance = rate("0.03");
    p.residual = ResidualPolicy::Defer;
    let tg = target(
        A1,
        G7,
        &[(0, q("10.00")), (1, q("-20.0"))],
        ExecSpec::PairOnFill(p),
    );
    let r = fx.go(&tg);
    let o = r.orders[0];
    assert_eq!(o.kind, ExecOrderKind::PairFirst);
    assert_eq!(o.kind.code(), 2);
    let l = o.intent.link.get().unwrap();
    assert_eq!(
        (l.group, l.role, l.tolerance),
        (G7, LinkRole::First, rate("0.03"))
    );
    let h = l.hedge;
    assert_eq!(
        (h.kind, h.venue, h.account, h.inst, h.side),
        (HedgeKind::OnFill, B, A1, P, Side::Sell)
    );
    assert_eq!(
        (h.px_mode, h.px_param, h.max_slip, h.reduce_only),
        (PxMode::Cross, rate("0.003"), rate("0.004"), 0)
    );
    assert_eq!(
        (h.ratio_num, h.ratio_den, h.residual, h.on_fail, h.tif),
        (2, 1, ResidualPolicy::Defer, OnFail::Retry, hft_types::Tif::Ioc)
    );
}
