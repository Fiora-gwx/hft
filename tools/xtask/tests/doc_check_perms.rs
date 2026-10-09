//! crates/hft-ctl-proto/TESTS.md CTP-093: the README permission table equals the rows rendered from
//! `perms.rs`.

use std::fs;
use std::path::{Path, PathBuf};

use xtask::doc_check::perms::{PERM_CODE, PERM_README, check, code_rows};

fn repo() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn copy_tree(dst: &Path) {
    for f in [PERM_README, PERM_CODE] {
        let to = dst.join(f);
        fs::create_dir_all(to.parent().unwrap()).unwrap();
        fs::copy(repo().join(f), to).unwrap();
    }
}

fn tmp(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("xtask-ctp093-{tag}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&d);
    fs::create_dir_all(&d).unwrap();
    d
}

fn edit_row(root: &Path, starts: &str, f: impl Fn(&str) -> Option<String>) {
    let p = root.join(PERM_README);
    let text = fs::read_to_string(&p).unwrap();
    let mut out = Vec::new();
    let mut hit = false;
    for l in text.lines() {
        if l.starts_with(starts) {
            hit = true;
            if let Some(n) = f(l) {
                out.push(n);
            }
        } else {
            out.push(l.to_string());
        }
    }
    assert!(hit, "row {starts} not found");
    fs::write(&p, out.join("\n") + "\n").unwrap();
}

#[test]
fn ctp_093_perm_table_matches_code() {
    // Step 1: the untouched copy passes; one rendered row per PermRow (+ optional rows).
    let d = tmp("ok");
    copy_tree(&d);
    assert_eq!(check(&d), Vec::<String>::new());
    let src = fs::read_to_string(d.join(PERM_CODE)).unwrap();
    let rows = code_rows(&src).unwrap();
    let n_core = src.matches("    PermRow {").count();
    let n_opt = rows.iter().filter(|r| r.contains("| optional |")).count();
    assert_eq!(rows.len(), n_core + n_opt);
    // Step 2: SmokeOrder's Trader column changed to ✓.
    let d2 = tmp("smoke");
    copy_tree(&d2);
    edit_row(&d2, "| `SmokeOrder`", |l| {
        let mut cells: Vec<String> = l.split(" | ").map(str::to_owned).collect();
        cells[4] = "✓".to_owned();
        Some(cells.join(" | "))
    });
    let p = check(&d2);
    assert!(
        !p.is_empty() && p.iter().all(|m| m.contains("SmokeOrder")),
        "{p:?}"
    );
    // Step 3: the HedgeFix row removed.
    let d3 = tmp("hedge");
    copy_tree(&d3);
    edit_row(&d3, "| `HedgeFix`", |_| None);
    let p = check(&d3);
    assert!(!p.is_empty() && p.iter().all(|m| m.contains("HedgeFix")), "{p:?}");
    for x in [d, d2, d3] {
        let _ = fs::remove_dir_all(x);
    }
}
