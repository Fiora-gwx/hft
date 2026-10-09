//! cargo-test entry of the DST suites (crates/hft-testkit README "6. DST 框架"): one `#[test]` per
//! suite of `hft_testkit::scenarios::suites()`, each running that suite's scenarios on a short smoke
//! seed range. Full seed counts run through `cargo xtask dst` and `hft-dst`.

use hft_testkit::scenarios::{framework::smoke, scenarios_of};

const SMOKE_SEEDS: u64 = 2;
const SMOKE_STEPS: u64 = 2_000;

fn suite(name: &'static str) {
    for s in scenarios_of(name) {
        if let hft_testkit::dst::SuiteSel::Named(n) = s.suite
            && n != name
        {
            continue;
        }
        let mut sc = s;
        sc.suite = hft_testkit::dst::SuiteSel::Named(name);
        if let Err(e) = smoke(&sc, 1, SMOKE_SEEDS, SMOKE_STEPS) {
            panic!("suite {name}, scenario {}: {e}", s.id);
        }
    }
}

#[test]
fn dst_suite_core() {
    suite("core");
}

#[test]
fn dst_suite_modes() {
    suite("modes");
}

#[test]
fn dst_suite_storage() {
    suite("storage");
}
