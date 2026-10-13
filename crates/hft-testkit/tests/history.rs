//! TK-175: the five HashTrail migrations of 2026-10-12 whose independent audits ran as temporary
//! CI workflows keep permanent, checkable evidence in `fixtures/dst/history/`: the reviewed
//! golden before each migration, the audit outputs copied byte for byte from the CI artifacts and
//! the audit programs. The evidence chains every golden from the oldest one to
//! `fixtures/dst/recon-before.toml`, which TK-137 reproduces on the current code.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

/// Oldest first.
const MIGRATIONS: [&str; 5] = ["1af231b", "b9d5024", "03c4607", "905fc14", "f1f801d"];
/// 905fc14: seeds whose old-capability digest differs from the previous golden because of the
/// no-client-id / FillBeforeAck fault-model fix (Agent Note 2026-10-12-perp-cash-settlement).
const FAULT_MODEL_SEEDS: [u64; 6] = [5, 9, 11, 14, 18, 43];
/// 1af231b: the seeds whose Drain restart state changed.
const DRAIN_SEEDS: [u64; 3] = [7, 28, 34];

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/dst")
}

fn read(rel: &str) -> Vec<u8> {
    std::fs::read(root().join(rel)).unwrap_or_else(|e| panic!("{rel}: {e}"))
}

fn text(rel: &str) -> String {
    String::from_utf8(read(rel)).unwrap()
}

fn field<'a>(row: &'a str, name: &str) -> &'a str {
    row.lines()
        .find_map(|l| {
            l.split_once(" = ")
                .filter(|(k, _)| k.trim() == name)
                .map(|(_, v)| v.trim().trim_matches('"'))
        })
        .unwrap_or_else(|| panic!("missing {name} in {row}"))
}

/// `[[seed]]` rows of a conformance file: seed → trail; every row core, 2 000 steps.
fn golden(rel: &str) -> BTreeMap<u64, String> {
    let t = text(rel);
    let mut m = BTreeMap::new();
    for row in t.split("[[seed]]").skip(1) {
        assert_eq!(field(row, "suite"), "core", "{rel}");
        assert_eq!(field(row, "steps"), "2000", "{rel}");
        let seed = u64::from_str_radix(field(row, "seed").trim_start_matches("0x"), 16).unwrap();
        assert!(
            m.insert(seed, field(row, "trail").to_string()).is_none(),
            "{rel}: duplicate seed"
        );
    }
    assert_eq!(m.len(), 50, "{rel}");
    m
}

/// Column `c` of a tab-separated evidence file, keyed by its first column (decimal or hex seed).
fn column(rel: &str, c: usize) -> BTreeMap<u64, String> {
    text(rel)
        .lines()
        .map(|l| {
            let f: Vec<&str> = l.split('\t').collect();
            let seed = match f[0].strip_prefix("0x") {
                Some(h) => u64::from_str_radix(h, 16).unwrap(),
                None => f[0].parse().unwrap(),
            };
            (seed, f[c].to_string())
        })
        .collect()
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

#[test]
fn tk_175_historical_hashtrail_migrations_keep_permanent_evidence() {
    // TK-175.1: every evidence file is the byte-identical artifact or git copy the manifest lists.
    let manifest = text("history/MANIFEST.toml");
    let lines: Vec<&str> = manifest.lines().collect();
    let sha_after = |i: usize| -> &str {
        lines[i + 1..]
            .iter()
            .find_map(|l| l.strip_prefix("sha256 = "))
            .unwrap_or_else(|| panic!("TK-175.1: no sha256 after line {i}"))
            .trim_matches('"')
    };
    let check = |rel: &str, want: &str| {
        assert_eq!(hex(&Sha256::digest(read(rel))), want, "TK-175.1 {rel}");
    };
    let (mut current, mut dir, mut checked) = (String::new(), String::from("."), 0);
    for (i, l) in lines.iter().enumerate() {
        let value = |k: &str| l.strip_prefix(k).map(|v| v.trim_matches('"'));
        if let Some(c) = value("commit = ")
            && MIGRATIONS.contains(&c)
        {
            current = c.to_string();
            dir = ".".into();
        } else if let Some(d) = value("dir = ") {
            dir = d.to_string();
        } else if let Some(want) = value("before_sha256 = ") {
            check(&format!("history/{current}/before.toml"), want);
            checked += 1;
        } else if let Some(p) = value("path = ")
            && !p.starts_with(".github/")
        {
            let rel = if dir == "." {
                format!("history/{current}/{p}")
            } else {
                format!("history/{current}/{dir}/{p}")
            };
            check(&rel, sha_after(i));
            checked += 1;
        } else if let Some(f) = value("file = ") {
            check(&format!("history/{current}/{f}"), sha_after(i));
            checked += 1;
        }
    }
    assert!(checked >= 40, "TK-175.1: {checked} files checked");

    // TK-175.2: the goldens chain: each migration's "after" is the next one's "before"; the last
    // one is the reference TK-137 reproduces on the current code.
    let before: Vec<_> = MIGRATIONS
        .iter()
        .map(|m| golden(&format!("history/{m}/before.toml")))
        .collect();
    let mut after: Vec<_> = before[1..].to_vec();
    after.push(golden("recon-before.toml"));
    for (k, m) in MIGRATIONS.iter().enumerate() {
        assert_eq!(
            before[k].keys().collect::<Vec<_>>(),
            after[k].keys().collect::<Vec<_>>(),
            "TK-175.2 {m}: seeds changed"
        );
    }

    // TK-175.3 (1af231b, Drain restart): the baseline build reproduces the old digests and the
    // fixed build the new ones, recomputed from the recorded trail bytes; only seeds 7/28/34 move.
    let (b, a) = (&before[0], &after[0]);
    let moved: Vec<u64> = b.keys().filter(|s| b[s] != a[s]).copied().collect();
    assert_eq!(moved, DRAIN_SEEDS, "TK-175.3");
    for seed in DRAIN_SEEDS {
        for (side, want) in [("baseline", &b[&seed]), ("fixed", &a[&seed])] {
            let trail = read(&format!("history/1af231b/{side}/seed-{seed}.trail"));
            let digest = format!("{:032x}", xxhash_rust::xxh3::xxh3_128(&trail));
            assert_eq!(&digest, want, "TK-175.3 {side} seed {seed}");
            let summary = text(&format!("history/1af231b/{side}/seed-{seed}.txt"));
            assert!(
                summary.contains(&format!("trail={digest}")),
                "TK-175.3 {side} seed {seed}"
            );
            assert!(summary.contains("steps=2000"), "TK-175.3 {side} seed {seed}");
        }
    }

    // TK-175.4 (b9d5024, snapshot section 27; 03c4607, OMS cash rows): with only the new
    // representation removed every old digest is reproduced; the full digest is the new golden.
    for (k, m) in [(1, "b9d5024"), (2, "03c4607")] {
        let full = column(&format!("history/{m}/trails.tsv"), 1);
        let old = column(&format!("history/{m}/trails.tsv"), 2);
        assert_eq!(
            old, before[k],
            "TK-175.4 {m}: projection reproduces the old golden"
        );
        assert_eq!(full, after[k], "TK-175.4 {m}: full digest is the new golden");
    }

    // TK-175.5 (905fc14, strict perpetual cash): the strict digests are the new golden; the old
    // capability reproduces the previous golden except the six fault-model seeds.
    let old_cap = column("history/905fc14/conformance.tsv", 1);
    let strict = column("history/905fc14/conformance.tsv", 2);
    assert_eq!(strict, after[3], "TK-175.5 strict digest is the new golden");
    let differ: BTreeSet<u64> = before[3]
        .keys()
        .filter(|s| old_cap[s] != before[3][s])
        .copied()
        .collect();
    assert_eq!(differ, FAULT_MODEL_SEEDS.into_iter().collect(), "TK-175.5");

    // TK-175.6 (f1f801d, BalanceCheck outputs): removing only the new outputs reproduces every
    // old digest; the current build gives the new golden = recon-before.toml.
    let rollback = column("history/f1f801d/rollback.tsv", 1);
    let current = column("history/f1f801d/current.tsv", 1);
    assert_eq!(rollback, before[4], "TK-175.6 rollback reproduces the old golden");
    assert_eq!(current, after[4], "TK-175.6 current build gives the new golden");
    assert!(
        text("history/f1f801d/rollback.patch").contains("BALANCE_CHECK"),
        "TK-175.6"
    );
}
