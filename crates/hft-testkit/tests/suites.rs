//! TK-066, TK-067, TK-069: suites, suite selection and the default step count.

use hft_testkit::dst::scenario::suite_for_seed;
use hft_testkit::scenarios::{suite_names, suites};
use hft_testkit::seed;
use xxhash_rust::xxh3::xxh3_64;

#[test]
fn tk_066_entry_points_agree() {
    let t = suite_names();
    let entry = include_str!("../../../tests/dst/main.rs");
    let mut tests = Vec::new();
    for (i, l) in entry.lines().enumerate() {
        if l.trim() == "#[test]"
            && let Some(next) = entry.lines().nth(i + 1)
            && let Some(name) = next.trim().strip_prefix("fn dst_suite_")
        {
            tests.push(name.trim_end_matches("() {").to_string());
        }
    }
    let mut want: Vec<String> = t.iter().map(|s| s.to_string()).collect();
    want.sort();
    tests.sort();
    assert_eq!(tests, want, "tests/dst has exactly one #[test] per suite");
    let mut uniq = tests.clone();
    uniq.dedup();
    assert_eq!(uniq.len(), tests.len());
    // hft-dst (M6) and `cargo xtask dst` take the same list: `hft_testkit::scenarios::suites()`.
}

#[test]
fn tk_067_suite_from_seed() {
    let s = suites();
    let names: Vec<&str> = s.iter().map(|x| x.name).collect();
    assert_eq!(names, ["core", "modes", "storage"]);
    for seed in [0u64, 1, 2, 3, 0x9e37_79b9_7f4a_7c15, u64::MAX] {
        let want = names[(xxh3_64(&seed.to_le_bytes()) % 3) as usize];
        // TK-067.1
        assert_eq!(
            suite_for_seed(&s, seed).map(|x| x.name),
            Some(want),
            "TK-067.1 seed {seed}"
        );
        // TK-067.2: both runners call `seed::suite_index`.
        assert_eq!(names[seed::suite_index(seed, 3)], want, "TK-067.2 seed {seed}");
    }
}

#[test]
fn tk_069_default_steps() {
    assert_eq!(seed::DST_DEFAULT_STEPS, 10_000);
    for s in suites() {
        assert_eq!(s.steps, 10_000, "suite {}", s.name);
        assert_eq!(s.config(1).steps, 10_000);
    }
    assert_eq!(hft_testkit::dst::DstConfig::new(1, "core").steps, 10_000);
}
