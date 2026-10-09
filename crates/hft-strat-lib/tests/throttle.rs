//! `Throttle` (TESTS.md SLB-039–SLB-050, SLB-052–SLB-054, SLB-153).

mod common;

use common::{ThrottleRow, snap_throttle, throttle_row};
use hft_strat_lib::{
    FireKind, Outcome, OutcomeAction, Throttle, ThrottleParams, ThrottleParamsNs, ThrottleReason,
};
use hft_strategy_sdk::SnapshotReader;

use FireKind::{Close, Flatten, Hard, Open};
use ThrottleReason::{Backoff, Cooldown, Halted, HardRearm, InFlight};

/// `TP0` without the `ns()` conversion.
fn tp0_ms() -> ThrottleParams {
    ThrottleParams {
        cooldown_ms: 500,
        max_consec_fail: 3,
        fallback_ms: 60_000,
        hard_rearm_ms: 1_000,
    }
}

fn tp0() -> ThrottleParamsNs {
    tp0_ms().ns()
}

const KINDS: [FireKind; 4] = [Open, Close, Flatten, Hard];
type Fires = [Result<(), ThrottleReason>; 4];
const OK4: Fires = [Ok(()), Ok(()), Ok(()), Ok(())];

fn fires(t: &Throttle, now: i64) -> Fires {
    KINDS.map(|k| t.can_fire(k, now))
}

#[test]
fn slb_039_can_fire_table() {
    let p = tp0();
    type Build = fn(&mut Throttle, &ThrottleParamsNs);
    let rows: &[(&str, Build, i64, Fires)] = &[
        ("SLB-039.1", |_, _| {}, 10_000_000_000, OK4),
        (
            "SLB-039.2",
            |t, p| t.on_fire(Open, 10_000_000_000, p),
            10_600_000_000,
            [Err(InFlight), Err(InFlight), Ok(()), Ok(())],
        ),
        (
            "SLB-039.3",
            |t, p| {
                t.on_fire(Open, 10_000_000_000, p);
                t.on_outcome(Outcome::NoFill, 10_100_000_000, p);
            },
            10_200_000_000,
            [Err(Cooldown), Err(Cooldown), Ok(()), Ok(())],
        ),
        (
            "SLB-039.4",
            |t, _| t.set_halted(true),
            10_000_000_000,
            [Err(Halted), Ok(()), Ok(()), Ok(())],
        ),
        (
            "SLB-039.5",
            |t, _| t.set_backoff(11_000_000_000),
            10_000_000_000,
            [Err(Backoff), Err(Backoff), Ok(()), Ok(())],
        ),
        (
            "SLB-039.6",
            |t, p| t.on_fire(Flatten, 10_000_000_000, p),
            10_500_000_000,
            [Ok(()), Ok(()), Err(HardRearm), Err(HardRearm)],
        ),
    ];
    for &(row, build, at, want) in rows {
        let mut t = Throttle::default();
        build(&mut t, &p);
        assert_eq!(fires(&t, at), want, "{row}");
    }
}

#[test]
fn slb_040_reason_order() {
    let p = tp0();
    let at = 10_200_000_000;
    let base = |t: &mut Throttle| {
        t.on_fire(Open, 10_000_000_000, &p);
        t.set_halted(true);
        t.set_backoff(20_000_000_000);
    };
    let mut t = Throttle::default();
    base(&mut t);
    assert_eq!(
        (t.can_fire(Open, at), t.can_fire(Close, at)),
        (Err(InFlight), Err(InFlight)),
        "SLB-040.1"
    );
    t.on_outcome(Outcome::NoFill, 10_100_000_000, &p);
    assert_eq!(
        (t.can_fire(Open, at), t.can_fire(Close, at)),
        (Err(Cooldown), Err(Cooldown)),
        "SLB-040.2"
    );
    let mut t = Throttle::default();
    t.set_halted(true);
    t.set_backoff(20_000_000_000);
    assert_eq!(
        (t.can_fire(Open, at), t.can_fire(Close, at)),
        (Err(Halted), Err(Backoff)),
        "SLB-040.3"
    );
    let mut t = Throttle::default();
    t.on_fire(Open, 10_000_000_000, &p);
    t.on_fire(Flatten, 10_000_000_000, &p);
    t.set_halted(true);
    t.set_backoff(20_000_000_000);
    assert_eq!(t.can_fire(Flatten, at), Err(HardRearm), "SLB-040.4");
}

#[test]
fn slb_041_cooldown_boundary() {
    let p = tp0();
    let mut t = Throttle::default();
    t.on_fire(Open, 10_000_000_000, &p);
    t.on_outcome(Outcome::NoFill, 10_100_000_000, &p);
    assert_eq!(throttle_row(&t).cooldown_until_ens, 10_500_000_000);
    let rows = [
        ("SLB-041.1", Open, 10_499_999_999, Err(Cooldown)),
        ("SLB-041.2", Open, 10_500_000_000, Ok(())),
        ("SLB-041.3", Open, 10_500_000_001, Ok(())),
        ("SLB-041.4", Close, 10_499_999_999, Err(Cooldown)),
    ];
    for (row, k, at, want) in rows {
        assert_eq!(t.can_fire(k, at), want, "{row}");
    }
}

#[test]
fn slb_042_hard_rearm() {
    let p = tp0();
    let mut t = Throttle::default();
    t.on_fire(Flatten, 10_000_000_000, &p);
    assert_eq!(throttle_row(&t).hard_rearm_until_ens, 11_000_000_000);
    let rows = [
        ("SLB-042.1", Flatten, 10_999_999_999, Err(HardRearm)),
        ("SLB-042.2", Flatten, 11_000_000_000, Ok(())),
        ("SLB-042.3", Hard, 10_500_000_000, Err(HardRearm)),
        ("SLB-042.4", Open, 10_000_000_000, Ok(())),
        ("SLB-042.5", Close, 10_000_000_000, Ok(())),
    ];
    for (row, k, at, want) in rows {
        assert_eq!(t.can_fire(k, at), want, "{row}");
    }
}

#[test]
fn slb_043_backoff() {
    let mut t = Throttle::default();
    t.set_backoff(12_000_000_000);
    let rows = [
        ("SLB-043.1", Open, 11_999_999_999, Err(Backoff)),
        ("SLB-043.2", Open, 12_000_000_000, Ok(())),
        ("SLB-043.3", Close, 11_900_000_000, Err(Backoff)),
        ("SLB-043.4", Flatten, 11_900_000_000, Ok(())),
    ];
    for (row, k, at, want) in rows {
        assert_eq!(t.can_fire(k, at), want, "{row}");
    }
    t.set_backoff(11_000_000_000);
    assert_eq!(t.can_fire(Open, 11_500_000_000), Ok(()), "SLB-043.5");
}

#[test]
fn slb_044_on_fire_effects() {
    let p = tp0();
    let mut x = Throttle::default();
    x.on_fire(Close, 10_000_000_000, &p);
    assert_eq!(
        throttle_row(&x),
        ThrottleRow {
            in_flight: 1,
            cooldown_until_ens: 10_500_000_000,
            ..ThrottleRow::default()
        }
    );
    let mut y = Throttle::default();
    y.on_fire(Hard, 10_000_000_000, &p);
    assert_eq!(
        throttle_row(&y),
        ThrottleRow {
            hard_rearm_until_ens: 11_000_000_000,
            ..ThrottleRow::default()
        }
    );
}

/// Runs `outcomes` from a default throttle, each preceded by `on_fire(Open, t)` (t += 1 s).
fn run_outcomes(p: &ThrottleParamsNs, outcomes: &[Outcome]) -> (Throttle, Vec<OutcomeAction>) {
    let mut t = Throttle::default();
    let mut now = 10_000_000_000i64;
    let mut out = Vec::new();
    for &o in outcomes {
        t.on_fire(Open, now, p);
        out.push(t.on_outcome(o, now, p));
        now += 1_000_000_000;
    }
    (t, out)
}

#[test]
fn slb_045_consec_fail() {
    use Outcome::{Failed as F, Filled, Gated as G, NoFill, Rollback};
    use OutcomeAction::{EnterFallback as EF, None as N, RequestHalt as RH};
    let p = tp0();
    let rows: &[(&str, &[Outcome], &[OutcomeAction])] = &[
        ("SLB-045.1", &[F], &[N]),
        ("SLB-045.2", &[F, F], &[N, N]),
        ("SLB-045.3", &[F, F, F], &[N, N, RH]),
        ("SLB-045.4", &[F, F, F, F], &[N, N, RH, RH]),
        ("SLB-045.5", &[F, F, Filled, F], &[N, N, N, N]),
        ("SLB-045.6", &[F, F, NoFill, F, F], &[N, N, N, N, N]),
        ("SLB-045.7", &[F, F, G, F], &[N, N, N, RH]),
        ("SLB-045.8", &[G; 10], &[N; 10]),
        ("SLB-045.9", &[F, F, Rollback, F], &[N, N, EF, RH]),
    ];
    for &(row, seq, want) in rows {
        assert_eq!(run_outcomes(&p, seq).1, want, "{row}");
    }
}

#[test]
fn slb_046_max_consec_fail_edges() {
    use OutcomeAction::{None as N, RequestHalt as RH};
    let rows: &[(&str, u8, usize)] = &[
        ("SLB-046.1", 0, 300),
        ("SLB-046.2", 1, 1),
        ("SLB-046.3", 2, 2),
        ("SLB-046.4", 255, 254),
        ("SLB-046.5", 255, 300),
    ];
    for &(row, max, n) in rows {
        let p = ThrottleParams {
            max_consec_fail: max,
            ..tp0_ms()
        }
        .ns();
        let (t, out) = run_outcomes(&p, &vec![Outcome::Failed; n]);
        for (i, a) in out.iter().enumerate() {
            let k = i + 1;
            let want = if max > 0 && k >= usize::from(max) { RH } else { N };
            assert_eq!(*a, want, "{row} outcome {k}");
        }
        if row == "SLB-046.1" || row == "SLB-046.5" {
            assert_eq!(throttle_row(&t).consec_fail, 255, "{row}");
        }
    }
}

#[test]
fn slb_047_rollback_fallback() {
    let p = tp0();
    let mut t = Throttle::default();
    t.on_fire(Open, 10_000_000_000, &p);
    assert_eq!(
        t.on_outcome(Outcome::Rollback, 10_200_000_000, &p),
        OutcomeAction::EnterFallback
    );
    assert!(t.in_fallback(70_199_999_999));
    assert!(!t.in_fallback(70_200_000_000));
    assert_eq!(t.can_fire(Open, 10_600_000_000), Ok(()));
}

#[test]
fn slb_048_outcome_clears_inflight() {
    let p = tp0();
    let rows = [
        ("SLB-048.1", Outcome::Filled, OutcomeAction::None),
        ("SLB-048.2", Outcome::NoFill, OutcomeAction::None),
        ("SLB-048.3", Outcome::Failed, OutcomeAction::None),
        ("SLB-048.4", Outcome::Rollback, OutcomeAction::EnterFallback),
        ("SLB-048.5", Outcome::Gated, OutcomeAction::None),
    ];
    for (row, o, want) in rows {
        let mut t = Throttle::default();
        t.on_fire(Open, 10_000_000_000, &p);
        assert_eq!(t.on_outcome(o, 10_100_000_000, &p), want, "{row}");
        assert_eq!(t.can_fire(Open, 10_500_000_000), Ok(()), "{row}");
    }
}

#[test]
fn slb_049_halted_mirror() {
    let mut t = Throttle::default();
    t.set_halted(true);
    assert_eq!(fires(&t, 10_000_000_000), [Err(Halted), Ok(()), Ok(()), Ok(())]);
    t.set_halted(false);
    assert_eq!(t.can_fire(Open, 10_000_000_000), Ok(()));
}

/// SLB-050 preconditions (without `reset_open_state`).
fn x50(p: &ThrottleParamsNs) -> Throttle {
    let mut t = Throttle::default();
    t.on_fire(Open, 10_000_000_000, p);
    t.on_outcome(Outcome::Failed, 10_050_000_000, p);
    t.on_outcome(Outcome::Rollback, 10_100_000_000, p);
    t.on_fire(Open, 10_200_000_000, p);
    t.on_fire(Flatten, 10_200_000_000, p);
    t.set_halted(true);
    t.set_backoff(12_000_000_000);
    t
}

#[test]
fn slb_050_reset_open_state() {
    let p = tp0();
    let mut t = x50(&p);
    t.reset_open_state();
    assert_eq!(
        throttle_row(&t),
        ThrottleRow {
            cooldown_until_ens: 0,
            in_flight: 1,
            consec_fail: 1,
            fallback_until_ens: 0,
            backoff_until_ens: 12_000_000_000,
            halted: 1,
            hard_rearm_until_ens: 11_200_000_000,
        }
    );
    assert_eq!(t.can_fire(Open, 10_300_000_000), Err(InFlight));
    assert!(!t.in_fallback(10_300_000_000));
}

#[test]
fn slb_052_params_ns() {
    assert_eq!(
        tp0(),
        ThrottleParamsNs {
            cooldown_ns: 500_000_000,
            max_consec_fail: 3,
            fallback_ns: 60_000_000_000,
            hard_rearm_ns: 1_000_000_000
        },
        "SLB-052.1"
    );
    let m = ThrottleParams {
        cooldown_ms: u32::MAX,
        max_consec_fail: 3,
        fallback_ms: u32::MAX,
        hard_rearm_ms: u32::MAX,
    }
    .ns();
    assert_eq!(
        (m.cooldown_ns, m.fallback_ns, m.hard_rearm_ns),
        (
            4_294_967_295_000_000,
            4_294_967_295_000_000,
            4_294_967_295_000_000
        ),
        "SLB-052.2"
    );
}

#[test]
fn slb_053_snapshot_layout() {
    let p = tp0();
    let x = x50(&p);
    let b = snap_throttle(&x);
    assert_eq!(b.len(), 35);
    let mut want = Vec::new();
    want.extend_from_slice(&10_700_000_000i64.to_le_bytes());
    want.extend_from_slice(&[1, 1]);
    want.extend_from_slice(&70_100_000_000i64.to_le_bytes());
    want.extend_from_slice(&12_000_000_000i64.to_le_bytes());
    want.push(1);
    want.extend_from_slice(&11_200_000_000i64.to_le_bytes());
    assert_eq!(b, want);
    let mut y = Throttle::default();
    assert_eq!(y.restore(&mut SnapshotReader::new(&b)), Ok(()));
    for at in [10_300_000_000, 10_800_000_000, 12_000_000_000] {
        assert_eq!(fires(&x, at), fires(&y, at), "at {at}");
    }
    assert_eq!(snap_throttle(&y), b);
}

#[test]
fn slb_054_restore_truncated() {
    let b = snap_throttle(&x50(&tp0()));
    let mut t = Throttle::default();
    t.set_halted(true);
    assert!(t.restore(&mut SnapshotReader::new(&b[..34])).is_err());
    assert_eq!(
        throttle_row(&t),
        ThrottleRow {
            halted: 1,
            ..ThrottleRow::default()
        }
    );
}

#[test]
fn slb_153_backoff_mirror_bool() {
    let mirror = |t: &mut Throttle, on: bool| t.set_backoff(if on { i64::MAX } else { 0 });
    type Row = (
        &'static str,
        &'static [bool],
        FireKind,
        i64,
        Result<(), ThrottleReason>,
    );
    let rows: &[Row] = &[
        ("SLB-153.1", &[true], Open, 10_000_000_000, Err(Backoff)),
        (
            "SLB-153.2",
            &[true],
            Close,
            9_000_000_000_000_000_000,
            Err(Backoff),
        ),
        ("SLB-153.3", &[true, false], Open, 10_000_000_000, Ok(())),
        ("SLB-153.4", &[true], Flatten, 10_000_000_000, Ok(())),
    ];
    for &(row, reads, k, at, want) in rows {
        let mut t = Throttle::default();
        for &r in reads {
            mirror(&mut t, r);
        }
        assert_eq!(t.can_fire(k, at), want, "{row}");
    }
}
