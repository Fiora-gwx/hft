//! `cargo xtask doc-check` (tools/TESTS.md TOOL-067, TOOL-068; crates/hft-event/TESTS.md EVT-144).

mod common;

use std::path::{Path, PathBuf};

use xtask::doc_check::{RING_EVENT_MD, RING_SEQ_MD, doc_length, ring_tables, slugs};

#[test]
fn tool_067_anchor_slug() {
    let rows: [(&str, &[&str], &str); 6] = [
        ("TOOL-067.1", &["1. 金样"], "1-金样"),
        ("TOOL-067.2", &["延迟与资源契约"], "延迟与资源契约"),
        ("TOOL-067.3", &["分段(`StageSet`)"], "分段stageset"),
        (
            "TOOL-067.4",
            &["cargo xtask decoupling-gate"],
            "cargo-xtask-decoupling-gate",
        ),
        (
            "TOOL-067.5",
            &["质量门(`quality-gates.yml`)"],
            "质量门quality-gatesyml",
        ),
        ("TOOL-067.6", &["示例", "其他", "示例"], "示例-1"),
    ];
    for (row, headings, want) in rows {
        let got = slugs(headings.iter().copied());
        assert_eq!(got.last().map(String::as_str), Some(want), "{row}: {got:?}");
    }
}

#[test]
fn tool_068_length_metric() {
    let fenced = format!("```\n{}\n```\n是", "汉".repeat(100));
    let rows: [(&str, &str, usize); 5] = [
        ("TOOL-068.1", "单写者规则", 5),
        ("TOOL-068.2", "use hft-oms now", 3),
        ("TOOL-068.3", "[链接](https://example.com/a/b)", 2),
        ("TOOL-068.4", &fenced, 1),
        ("TOOL-068.5", ",。——", 0),
    ];
    for (row, text, want) in rows {
        assert_eq!(doc_length(text), want, "{row}");
    }
}

/// `(id, file, edit, output must contain)`.
type RingEdit = (&'static str, &'static str, fn(&str) -> String, &'static str);

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..").join("..")
}

/// Copies the repo tree (without `target/` and `.git/`) to `dest`.
fn copy_repo(dest: &Path) {
    let root = repo_root();
    for e in walkdir::WalkDir::new(&root)
        .into_iter()
        .filter_entry(|e| {
            let r = xtask::rel_path(&root, e.path());
            r != "target" && r != ".git"
        })
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
    {
        let to = dest.join(e.path().strip_prefix(&root).unwrap());
        std::fs::create_dir_all(to.parent().unwrap()).unwrap();
        std::fs::copy(e.path(), to).unwrap();
    }
}

#[test]
fn evt_144_ring_registry_consistent() {
    let (ev, sq) = ring_tables(&repo_root()).unwrap();
    let rows: [(&str, &[&str], &str, Option<&str>); 10] = [
        ("EVT-144.1", &["ctl"], "CtlRecord", None),
        ("EVT-144.2", &["hk"], "HkMsg", None),
        ("EVT-144.3", &["rep-<venue>"], "RepMsg", Some("rep-<venue>-bulk")),
        ("EVT-144.4", &["rep-sim"], "RepMsg", None),
        ("EVT-144.5", &["bbo-<venue>"], "FeedMsg", None),
        ("EVT-144.6", &["md-<venue>"], "FeedMsg", Some("md-<venue>-bulk")),
        ("EVT-144.7", &["advice"], "AdviceMsg", None),
        ("EVT-144.8", &["out-<venue>"], "GwCmd", None),
        ("EVT-144.9", &["out-sim"], "GwCmd", None),
        (
            "EVT-144.10",
            &["feedctl-<venue>", "feedctl-<venue>-bbo"],
            "FeedCmd",
            None,
        ),
    ];
    for (id, rings, elem, companion) in rows {
        for ring in rings {
            for (side, table) in [(RING_EVENT_MD, &ev), (RING_SEQ_MD, &sq)] {
                let r = table
                    .iter()
                    .find(|r| r.ring == *ring)
                    .unwrap_or_else(|| panic!("{id}: `{ring}` not in {side}"));
                assert_eq!(r.elem, elem, "{id}: `{ring}` in {side}");
                if let Some(c) = companion {
                    assert!(
                        r.companions.iter().any(|x| x == c),
                        "{id}: `{ring}` in {side} lacks {c}"
                    );
                }
            }
        }
    }
    assert!(xtask::doc_check::check_rings(&repo_root()).is_empty());

    // .11 / .12: doc-check on a copy of the tree with one table edited.
    let edits: [RingEdit; 2] = [
        (
            "EVT-144.11",
            RING_EVENT_MD,
            |s| {
                s.replacen(
                    "| `ctl` | ctl-gateway |",
                    "| `foo-<venue>` | 测试 | `FeedMsg` |\n| `ctl` | ctl-gateway |",
                    1,
                )
            },
            "foo-<venue>",
        ),
        (
            "EVT-144.12",
            RING_SEQ_MD,
            |s| {
                s.lines()
                    .filter(|l| !l.starts_with("| `hk` ×1 |"))
                    .collect::<Vec<_>>()
                    .join("\n")
                    + "\n"
            },
            "`hk`",
        ),
    ];
    std::thread::scope(|sc| {
        for (id, file, edit, want) in edits {
            sc.spawn(move || {
                let f = common::Fixture::empty(&id.to_lowercase().replace('.', "-"));
                copy_repo(&f.root);
                let before = f.read(file);
                let after = edit(&before);
                assert_ne!(before, after, "{id}: edit did not apply");
                f.write(file, &after);
                let run = f.xtask(&["doc-check"]);
                assert_eq!(run.code, 1, "{id}: {}", run.stderr);
                assert!(run.stderr.contains(want), "{id}: output lacks {want}");
            });
        }
    });
}
