//! TK-176, TK-177: per-invariant non-vacuity counters of the real-engine DST.

use hft_testkit::dst::invariants::{CHECKERS, InvId};
use hft_testkit::dst::{RerunPolicy, run_record};
use hft_testkit::scenarios::{SeqEngine, suites};

/// The cargo-test smoke set of `tests/dst/main.rs`: every suite, seeds 1 and 2, 2 000 steps.
const SMOKE_SEEDS: u64 = 2;
const SMOKE_STEPS: u64 = 2_000;

/// Invariants every standard smoke run must actually exercise. The others are only reached by
/// their directed cases (TK-179, TK-053.21–.23, LED-080/081 …) and are reported, not required.
const CORE: &[InvId] = &[
    InvId::I1,
    InvId::I2,
    InvId::I3,
    InvId::I4,
    InvId::I5,
    InvId::I6,
    InvId::I7,
    InvId::I8,
    InvId::I11,
    InvId::I12,
    InvId::I14,
    InvId::I15,
    InvId::I16,
    InvId::I19,
];

#[test]
fn tk_177_standard_smoke_compares_every_core_invariant() {
    let mut total = [0u64; 32];
    let mut lines = Vec::new();
    for s in suites() {
        for seed in 1..=SMOKE_SEEDS {
            let mut c = s.config(seed);
            c.steps = SMOKE_STEPS;
            c.rerun = RerunPolicy::Always;
            let r = run_record::<SeqEngine>(&c);
            assert!(
                r.outcome.is_pass(),
                "{} seed {seed}: {:?}",
                s.name,
                r.outcome.failure().map(|f| (f.invariant.label(), &f.message))
            );
            let row: Vec<String> = CHECKERS
                .iter()
                .map(|k| format!("{}={}", k.id.code(), r.compared_of(k.id)))
                .collect();
            lines.push(format!("{} seed {seed}: {}", s.name, row.join(" ")));
            for (t, n) in total.iter_mut().zip(r.compared) {
                *t += n;
            }
        }
    }
    let sum: Vec<String> = CHECKERS
        .iter()
        .map(|k| format!("{}={}", k.id.code(), total[usize::from(k.id.0)]))
        .collect();
    eprintln!("{}\nTK-177 compared: {}", lines.join("\n"), sum.join(" "));
    let vacuous: Vec<String> = CORE
        .iter()
        .filter(|i| total[usize::from(i.0)] == 0)
        .map(|i| i.code())
        .collect();
    assert!(
        vacuous.is_empty(),
        "TK-177: core invariants compared nothing: {vacuous:?}"
    );
}

/// TK-176.1/.2: a checker that is off compares nothing; an enabled checker's count does not depend
/// on which other checkers run.
#[test]
fn tk_176_compared_counts_follow_enabled_checkers() {
    use hft_testkit::dst::InvariantSet;
    let suite = suites().into_iter().find(|s| s.name == "core").unwrap();
    let mut c = suite.config(1);
    c.steps = SMOKE_STEPS;
    c.rerun = RerunPolicy::Never;
    let all = run_record::<SeqEngine>(&c);
    assert!(all.outcome.is_pass());
    c.invariants = InvariantSet::none();
    let none = run_record::<SeqEngine>(&c);
    assert_eq!(none.compared, [0; 32], "TK-176.1: checkers off compare nothing");
    for k in CHECKERS {
        c.invariants = InvariantSet::only(k.id);
        let one = run_record::<SeqEngine>(&c);
        assert!(one.outcome.is_pass(), "{}", k.id.code());
        for other in CHECKERS.iter().filter(|o| o.id != k.id) {
            assert_eq!(
                one.compared_of(other.id),
                0,
                "TK-176.1: {} counted under {}",
                other.id.code(),
                k.id.code()
            );
        }
        assert_eq!(
            one.compared_of(k.id),
            all.compared_of(k.id),
            "TK-176.2: {} depends on the other checkers",
            k.id.code()
        );
    }
}

/// The opt-in trading world (`WorldSpec::w1_trading`, hft-testkit README "DST 的世界模型") on the
/// standard smoke set: every run passes, and the hedge/naked-leg checkers compare far more than
/// in W1 (INV-15/19: 69 vs 12 when introduced).
#[test]
fn trading_world_smoke_exercises_hedges() {
    let mut total = [0u64; 32];
    let mut w1 = [0u64; 32];
    for s in suites() {
        for seed in 1..=SMOKE_SEEDS {
            for (trading, sum) in [(true, &mut total), (false, &mut w1)] {
                let mut c = s.config(seed);
                if trading {
                    c.world = hft_testkit::dst::world::WorldSpec::w1_trading(false);
                }
                c.steps = SMOKE_STEPS;
                c.rerun = RerunPolicy::Never;
                let r = run_record::<SeqEngine>(&c);
                assert!(
                    r.outcome.is_pass(),
                    "{} seed {seed} trading={trading}: {:?}",
                    s.name,
                    r.outcome.failure().map(|f| (f.invariant.label(), &f.message))
                );
                for (t, n) in sum.iter_mut().zip(r.compared) {
                    *t += n;
                }
            }
        }
    }
    for i in [InvId::I15, InvId::I19] {
        let (t, b) = (total[usize::from(i.0)], w1[usize::from(i.0)]);
        eprintln!("{}: trading {t}, W1 {b}", i.code());
        assert!(t > 2 * b, "{}: trading world compared {t}, W1 {b}", i.code());
    }
}
