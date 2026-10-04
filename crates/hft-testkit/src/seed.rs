//! Seed derivation: the only way this crate turns a `u64` seed into randomness. Sub-streams are
//! derived by label (`derive`), and `runner` builds a proptest `TestRunner` whose ChaCha RNG is
//! fixed by the seed, so a generator run under it is byte-for-byte reproducible across processes
//! and hosts (no system clock, no OS entropy, no `PROPTEST_*` environment).

use proptest::test_runner::{Config, FileFailurePersistence, RngAlgorithm, TestRng, TestRunner};
use xxhash_rust::xxh3::xxh3_64_with_seed;

/// Child seed of `seed` for the sub-stream named `label` (`xxh3_64(label)` keyed by `seed`).
pub fn derive(seed: u64, label: &str) -> u64 {
    xxh3_64_with_seed(label.as_bytes(), seed)
}

/// The 32-byte ChaCha key of `seed`: four little-endian words `derive(seed, "chacha/<i>")`.
pub fn chacha_key(seed: u64) -> [u8; 32] {
    let mut key = [0u8; 32];
    for (i, w) in key.chunks_exact_mut(8).enumerate() {
        let label = ["chacha/0", "chacha/1", "chacha/2", "chacha/3"][i];
        w.copy_from_slice(&derive(seed, label).to_le_bytes());
    }
    key
}

/// A proptest RNG fixed by `seed`.
pub fn rng(seed: u64) -> TestRng {
    TestRng::from_seed(RngAlgorithm::ChaCha, &chacha_key(seed))
}

/// A runner for `cases` cases whose RNG is fixed by `seed`; no failure persistence files and no
/// fork, so nothing outside the process is read or written.
pub fn runner(seed: u64, cases: u32) -> TestRunner {
    let cfg = Config {
        cases,
        failure_persistence: Some(Box::new(FileFailurePersistence::Off)),
        rng_algorithm: RngAlgorithm::ChaCha,
        ..Config::default()
    };
    TestRunner::new_with_rng(cfg, rng(seed))
}

/// One value of `strategy` drawn deterministically from `seed` (no shrinking).
pub fn sample<S: proptest::strategy::Strategy>(seed: u64, strategy: &S) -> S::Value {
    let mut r = runner(seed, 1);
    match strategy.new_tree(&mut r) {
        Ok(tree) => proptest::strategy::ValueTree::current(&tree),
        Err(e) => panic!("seed::sample: strategy rejected every value: {e}"),
    }
}
