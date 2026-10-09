//! EXE-052: the ungrouped cell's in-flight comes from its own `own_infl` (Unknown included).
mod common;
use common::*;
use hft_exec::*;
use hft_strategy_sdk::{MarginLevel, OrderUpdateKind};
use hft_types::Side;

#[test]
fn exe_052_ungrouped_own_infl() {
    let mut fx = Fx::new();
    fx.perp(A1, B, P, "-3.0");
    fx.risk(A1, |r| r.margin_level = MarginLevel::L3);
    fx.force(A1, B, "0.5");
    let r = fx.adv();
    let u: Vec<&ExecOrder> = r.orders.iter().filter(|o| o.intent.group == GNONE).collect();
    assert_eq!(u.len(), 1);
    let o = *u[0];
    assert_eq!(
        (o.kind, o.intent.side, o.intent.inst, o.intent.qty),
        (ExecOrderKind::ForceStep, Side::Buy, P, q("1.5"))
    );
    assert!(o.intent.reduce_only());
    let rf = fx.placed(&o);
    fx.upd(rf, GNONE, OrderUpdateKind::Unknown);
    let snap = fx.snap();
    let row = cell_row(&snap, A1, GNONE).unwrap();
    assert_eq!(row.own_infl[0], q("1.5"));
    assert_eq!(row.filled, [q("0"); 4]);
    fx.at(200);
    let r = fx.adv();
    assert!(r.orders.is_empty());
    assert!(fx.log.notify.iter().all(|n| n.u.group != GNONE));
}
