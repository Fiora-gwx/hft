//! EXE-195–EXE-196: Simultaneous first leg; MakerFirst unsupported.
use crate::common;
use common::*;
use hft_exec::*;
use hft_strategy_sdk::{
    ExecSpec, HedgeKind, MakerParams, OnFail, ResidualPolicy, SimulOnFail, SimulParams, TargetReject,
};
use hft_types::Rate;

#[test]
fn exe_195_simul_first_leg() {
    for (row, of, want) in [
        ("EXE-195.1", SimulOnFail::Reverse, OnFail::Reverse),
        ("EXE-195.2", SimulOnFail::Hold, OnFail::Hold),
    ] {
        let mut fx = Fx::new();
        let sp = SimulParams {
            slice: slice0(),
            on_fail: of,
            _pad0: [0; 7],
            fix_tolerance: rate("0.02"),
            residual: ResidualPolicy::TrimExcess,
            _pad1: [0; 7],
        };
        let o = fx
            .go(&target(
                A1,
                G7,
                &[(0, q("10")), (1, q("-10"))],
                ExecSpec::Simultaneous(sp),
            ))
            .orders[0];
        assert_eq!(
            (o.kind, o.kind.code(), o.intent.qty),
            (ExecOrderKind::SimulFirst, 3, q("9.9")),
            "{row}"
        );
        let h = o.intent.link.get().unwrap().hedge;
        assert_eq!((h.kind, h.on_fail), (HedgeKind::Simultaneous, want), "{row}");
    }
}

#[test]
fn exe_196_maker_first_unsupported() {
    let mut fx = Fx::new();
    let m = MakerParams {
        offset: Rate(0),
        requote: Rate(0),
        max_rest_ms: 0,
        _pad0: 0,
        chunk_notional: usd("100"),
        hedge: pair0(),
    };
    let (res, r) = fx.set(
        I1,
        &target(A1, G7, &[(0, q("1")), (1, q("-1"))], ExecSpec::MakerFirst(m)),
    );
    assert_eq!(res, Err(TargetReject::AlgoUnsupported));
    assert!(r.has_dec(4, pack(1, 4, 0)));
}
