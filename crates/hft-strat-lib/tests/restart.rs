//! Snapshots carry no TSC (TESTS.md SLB-056).

mod common;

use common::{anchor_with, now_tsc, snap_confirm, snap_expiring, snap_throttle, snap_trigger, tsc_at};
use hft_strat_lib::{
    ConfirmParams, ConfirmWindow, Expiring, FireKind, Outcome, Throttle, ThrottleParams, TriggerPx,
};
use hft_strategy_sdk::{EngineClockAnchor, Now};
use hft_types::{Px, Rate, Tsc};

const ENS0: i64 = 20_000_000_000;

/// The four components after the same 20 steps at the engine times of `clk`.
fn run(clk: EngineClockAnchor) -> [Vec<u8>; 4] {
    let tp = ThrottleParams {
        cooldown_ms: 300,
        max_consec_fail: 2,
        fallback_ms: 5_000,
        hard_rearm_ms: 700,
    }
    .ns();
    let cp = ConfirmParams {
        min_ms: 250,
        min_updates: 3,
        min_slow_updates: 1,
        min_slow_span: 2,
    }
    .ns();
    let (mut th, mut cw, mut tr, mut ex) = (
        Throttle::default(),
        ConfirmWindow::default(),
        TriggerPx::default(),
        Expiring::new(0i64),
    );
    for k in 0..20i64 {
        let ens = ENS0 + (k + 1) * 137_000_000;
        let n: Now = now_tsc(clk, tsc_at(clk, ens), 100 + k as u64, 7);
        assert_eq!(n.engine_ns, ens);
        let e = n.engine_ns;
        match k % 4 {
            0 => th.on_fire(FireKind::Open, e, &tp),
            1 => {
                th.on_outcome(
                    if k % 3 == 0 {
                        Outcome::Failed
                    } else {
                        Outcome::Rollback
                    },
                    e,
                    &tp,
                );
            }
            2 => th.on_fire(FireKind::Flatten, e, &tp),
            _ => th.set_backoff(e + 1_000),
        }
        cw.observe(k % 5 != 4, k % 2 == 0, 1_000 + k as u64, e, &cp);
        tr.update(
            Px(6_000_000 + k),
            Px(6_000_100),
            Rate(500_000),
            Rate(-100_000),
            Px(10),
            e,
        );
        // basis = the TSC of the previous step's engine time in this run's clock.
        let basis: Tsc = n.tsc_of(e - 137_000_000);
        ex.put_advice(k, n.seq, basis, 400, &n);
    }
    [
        snap_throttle(&th),
        snap_confirm(&cw),
        snap_trigger(&tr),
        snap_expiring(&ex),
    ]
}

#[test]
fn slb_056_snapshot_has_no_tsc() {
    let n1 = anchor_with(0, 3_000_000, ENS0);
    let n2 = anchor_with(1_000_000_000_000_000, 1_000_000, ENS0);
    assert_eq!(run(n1), run(n2));
}
