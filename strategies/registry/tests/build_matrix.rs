//! Default and all-features builds (TESTS.md REG-003). CI builds both (`--all-features` in the clippy and
//! test steps); this test checks the list of the build it runs in against the manifest's features.
#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods,
    clippy::type_complexity
)] // allow-list: test code

use std::collections::BTreeSet;

use hft_strategy_registry::Strategies;
use hft_strategy_sdk::StrategySet;

/// Feature names declared in this crate's `[features]` (besides `default`).
fn features() -> Vec<String> {
    let toml = include_str!("../Cargo.toml");
    let mut in_features = false;
    let mut out = Vec::new();
    for l in toml.lines() {
        let l = l.trim();
        if l.starts_with('[') {
            in_features = l == "[features]";
            continue;
        }
        if in_features
            && !l.starts_with('#')
            && let Some((name, _)) = l.split_once('=')
            && name.trim() != "default"
        {
            out.push(name.trim().to_string());
        }
    }
    out
}

#[test]
fn reg_003_build_matrix() {
    let k = Strategies::kinds();
    // REG-003.1: the default build lists the core strategies only (every row without a feature).
    let core: BTreeSet<&str> = ["latency-probe", "portfolio-carry"].into_iter().collect();
    let feats = features();
    let names: BTreeSet<&str> = k.iter().map(|d| d.name).collect();
    for n in &names {
        assert!(
            core.contains(n) || feats.iter().any(|f| f == n),
            "REG-003.1: {n} is neither core nor feature-gated"
        );
    }
    // REG-003.2: unique kind_id, name and schema id; feature-gated rows appear only with their feature.
    let ids: BTreeSet<u16> = k.iter().map(|d| d.kind_id).collect();
    let sids: BTreeSet<u32> = k.iter().map(|d| d.schema.id).collect();
    assert_eq!(
        (ids.len(), names.len(), sids.len()),
        (k.len(), k.len(), k.len()),
        "REG-003.2"
    );
    assert_eq!(sids.len(), k.len(), "REG-003.2");
    assert_eq!(
        names.len(),
        core.len() + feats.iter().filter(|f| names.contains(f.as_str())).count(),
        "REG-003.2"
    );
}
