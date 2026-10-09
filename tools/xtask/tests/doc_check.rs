//! `cargo xtask doc-check` (tools/TESTS.md TOOL-067, TOOL-068; crates/hft-event/TESTS.md EVT-144).

use crate::common;

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

// ------------------------------------------------------------------------------------------ TOOL-066

/// The `docs-clean` fixture (tools/xtask/tests/fixtures/docs-clean), without `schedule.md` (the text of
/// TOOL-066.21, kept out of the scanned tree).
fn docs_clean(tag: &str) -> common::Fixture {
    let src = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("docs-clean");
    let f = common::Fixture::empty(&format!("docs-{tag}"));
    for e in walkdir::WalkDir::new(&src)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
    {
        let rel = e.path().strip_prefix(&src).unwrap();
        if rel == Path::new("schedule.md") {
            continue;
        }
        let to = f.root.join(rel);
        std::fs::create_dir_all(to.parent().unwrap()).unwrap();
        std::fs::copy(e.path(), to).unwrap();
    }
    f
}

fn schedule_text() -> String {
    std::fs::read_to_string(
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/docs-clean/schedule.md"),
    )
    .unwrap()
}

/// Pad `rel` with CJK characters to exactly `n` by the 篇幅 measure.
fn pad_to(f: &common::Fixture, rel: &str, n: usize) {
    let s = f.read(rel);
    let have = doc_length(&s);
    assert!(have <= n, "{rel}: already {have} > {n}");
    f.write(rel, &format!("{s}\n{}\n", "汉".repeat(n - have)));
    assert_eq!(doc_length(&f.read(rel)), n);
}

const OMS: &str = "crates/hft-oms/README.md";
const SEQ: &str = "crates/hft-sequencer/README.md";
const OMS_BUDGETS: &str = "crates/hft-oms/benches/budgets.toml";
const NOTE: &str = ".agents/notes/proposed/architecture/2026-10-01-sample.md";

/// `(row, edit, expected exit, output must contain)`.
type DocRow = (&'static str, fn(&common::Fixture), i32, &'static str);

fn r01(f: &common::Fixture) {
    f.append(OMS, "\n见 [不存在](../nosuch.md)。\n");
}
fn r02(f: &common::Fixture) {
    f.append(SEQ, "\n见 [OMS](../hft-oms/README.md#no-such-anchor)。\n");
}
fn r03(f: &common::Fixture) {
    f.append(SEQ, "\n见 [OMS](../hft-oms/README.md#延迟与资源契约)。\n");
}
fn r04(f: &common::Fixture) {
    f.replace(OMS, "status: spec\n", "");
}
fn r05(f: &common::Fixture) {
    f.replace(OMS, "scope: core\n", "scope: experimental\n");
}
fn r06(f: &common::Fixture) {
    f.replace(OMS, "status: spec\n", "status: done\n");
}
fn r07(f: &common::Fixture) {
    f.replace(
        OMS,
        "## 测试与验收\n\nhft-oms 的测试与验收。\n\n## 依赖\n\nhft-oms 的依赖。\n",
        "## 依赖\n\nhft-oms 的依赖。\n\n## 测试与验收\n\nhft-oms 的测试与验收。\n",
    );
}
fn r08(f: &common::Fixture) {
    f.replace(
        OMS,
        "| `oms_place` |\n",
        "| `oms_place` |\n| `alloc` | p50 ≤ 50 ns | `oms/alloc` |\n",
    );
}
fn r09(f: &common::Fixture) {
    f.replace(OMS_BUDGETS, "p99_ns = 1000", "p99_ns = 900");
}
fn r10(f: &common::Fixture) {
    f.replace(OMS, "p99 ≤ 1 µs |", "p99 ≤ 2 µs |");
    f.replace(OMS_BUDGETS, "p99_ns = 1000", "p99_ns = 2000");
}
fn r11(f: &common::Fixture) {
    f.append(OMS, "\n分段 `kernel_rxx` 的直方图。\n");
}
fn r12(f: &common::Fixture) {
    f.replace(SEQ, "`L2Delta`、", "");
}
fn r13(f: &common::Fixture) {
    f.replace(
        "docs/subsystems/event-model.md",
        "| `Bbo` | 定长 96 | Input |",
        "| `Bbo` | 定长 96 | Output |",
    );
}
fn r14(f: &common::Fixture) {
    f.append(OMS, "\n写一条 Note{OmsNoSuch}。\n");
}
fn r15(f: &common::Fixture) {
    f.append(OMS, "\n写一条 Note{ProbeEdge}。\n");
}
fn r16(f: &common::Fixture) {
    f.append(OMS, "\n到期产出 Timer{NoSuch}。\n");
}
fn r17(f: &common::Fixture) {
    f.replace(OMS, "// 1 = First、2 = Second", "// 腿的种类");
}
fn r18(f: &common::Fixture) {
    f.replace(OMS, "// 40 B", "// 48 B");
}
fn r19(f: &common::Fixture) {
    f.replace(
        "crates/AGENTS.md",
        "strategies/<name> → hft-strategy-sdk\n",
        "strategies/<name> → hft-strategy-sdk\nhft-oms → hft-sequencer\n",
    );
}
fn r20(f: &common::Fixture) {
    f.replace("crates/hft-net/README.md", "scope: core\n", "scope: optional\n");
}
fn r21(f: &common::Fixture) {
    f.append(OMS, &format!("\n{}", schedule_text()));
}
fn r22(f: &common::Fixture) {
    f.append(
        "crates/venue-old/README.md",
        "\n见 [HypoDriver](../hft-oms/README.md)。\n",
    );
}
fn r23(f: &common::Fixture) {
    f.append(OMS, "\n见 [HypoDriver](../hft-oms/README.md)。\n");
}
fn r24(f: &common::Fixture) {
    f.append(OMS, "\nfooarb 的参数在这里。\n");
}
fn r25(f: &common::Fixture) {
    f.append(OMS, "\n与 cdex 共用。\n");
}
fn r26(f: &common::Fixture) {
    f.append("python/pipeline/hft_pipeline/core.py", "# fooarb\n");
}
fn r27(f: &common::Fixture) {
    f.append("strategies/fooarb/README.md", "\nfooarb 的参数说明。\n");
}
fn r28(f: &common::Fixture) {
    f.append(OMS, "\n以前的做法是轮询。\n");
}
fn r29(f: &common::Fixture) {
    f.replace(
        OMS,
        "- [hft-sequencer](../hft-sequencer/README.md)\n",
        "- [hft-sequencer](../hft-sequencer/README.md)\n- 以前的讨论见设计记录。\n",
    );
}
fn r30(f: &common::Fixture) {
    f.write(
        ".agents/notes/proposed/architecture/2026-10-02-old.md",
        "# Old\n\n以前用轮询。\n",
    );
    f.write("docs/postmortem/2026-10-01-x.md", "# x\n\n以前用轮询。\n");
}
fn r31(f: &common::Fixture) {
    f.append(OMS, "\n见 [旧组件](../old-comp/README.md)。\n");
}
fn r32(f: &common::Fixture) {
    f.append(NOTE, "\n见 [x](nosuch.md)。\n");
}
fn r33(f: &common::Fixture) {
    f.write(".agents/notes/proposed/misc/2026-10-01-x.md", "# x\n");
}
fn r34(f: &common::Fixture) {
    f.write("AGENTS.md", &"汉".repeat(1800));
}
fn r35(f: &common::Fixture) {
    f.write("AGENTS.md", &"汉".repeat(1801));
}
fn r36(f: &common::Fixture) {
    pad_to(f, "crates/AGENTS.md", 901);
}
fn r37(f: &common::Fixture) {
    pad_to(f, "docs/architecture.md", 3001);
}
fn r38(f: &common::Fixture) {
    f.replace(SEQ, "`HkMsg`:`ClockStatus`", "`TimerMsg`:`ClockStatus`");
}
fn r39(f: &common::Fixture) {
    f.replace(
        SEQ,
        "| `out-<venue>` ×M |",
        "| `foo` ×1 | x → sequencer | `FooMsg` | 7 |\n| `out-<venue>` ×M |",
    );
}
fn r40(f: &common::Fixture) {
    f.replace(
        "crates/hft-event/README.md",
        "| `out-<venue>` | sequencer |",
        "| `chain-in` | ext | `ChainMsg` |\n| `out-<venue>` | sequencer |",
    );
    f.replace(
        SEQ,
        "| `out-<venue>` ×M |",
        "| `chain-in` ×0–1 | ext → sequencer | `PoolQuote` | 2 |\n| `out-<venue>` ×M |",
    );
}
fn r41(f: &common::Fixture) {
    f.replace(
        "crates/venue-foo/README.md",
        "### 账户配置核验\n\n`FooPerp`(永续)与 `FooSpot`(现货)的账户配置核验。\n\n",
        "",
    );
}
fn r42(f: &common::Fixture) {
    f.replace(
        "crates/venue-foo/README.md",
        "| ≤ 20 ns / ≤ 50 ns |",
        "| ≤ 20 ns |",
    );
}
fn r43(f: &common::Fixture) {
    f.replace("docs/runbooks/x.md", "## 回退\n\n无。\n\n", "");
}
fn r44(f: &common::Fixture) {
    f.replace(
        "docs/runbooks/x.md",
        "## 步骤\n\n1. 运行。\n\n## 验收\n\n通过。\n",
        "## 验收\n\n通过。\n\n## 步骤\n\n1. 运行。\n",
    );
}
fn r45(_: &common::Fixture) {}

const DOC_ROWS: &[DocRow] = &[
    ("TOOL-066.1", r01, 1, "crates/hft-oms/README.md:"),
    ("TOOL-066.2", r02, 1, "anchor does not exist"),
    ("TOOL-066.3", r03, 0, ""),
    ("TOOL-066.4", r04, 1, "lacks `status`"),
    ("TOOL-066.5", r05, 1, "scope 'experimental'"),
    ("TOOL-066.6", r06, 1, "status 'done'"),
    ("TOOL-066.7", r07, 1, "out of template order"),
    (
        "TOOL-066.8",
        r08,
        1,
        "`oms/alloc` is not in crates/hft-oms/benches/budgets.toml",
    ),
    ("TOOL-066.9", r09, 1, "p99_ns = 1000 ns in the README"),
    ("TOOL-066.10", r10, 1, "wider than the 单 crate 基准上限表"),
    ("TOOL-066.11", r11, 1, "`kernel_rxx`"),
    ("TOOL-066.12", r12, 1, "EventKind `L2Delta`"),
    ("TOOL-066.13", r13, 1, "`Bbo` class `Output`"),
    ("TOOL-066.14", r14, 1, "Note{OmsNoSuch}"),
    ("TOOL-066.15", r15, 0, ""),
    ("TOOL-066.16", r16, 1, "Timer{NoSuch}"),
    ("TOOL-066.17", r17, 1, "LegRow.leg_kind"),
    ("TOOL-066.18", r18, 1, "LegRow declared 48 B, fields sum 40 B"),
    (
        "TOOL-066.19",
        r19,
        1,
        "edge `crates/hft-oms → crates/hft-sequencer`",
    ),
    (
        "TOOL-066.20",
        r20,
        1,
        "table scope `core`, frontmatter `optional`",
    ),
    ("TOOL-066.21", r21, 1, "[SCHEDULE]"),
    ("TOOL-066.22", r22, 0, ""),
    ("TOOL-066.23", r23, 1, "[OLDTERM]"),
    ("TOOL-066.24", r24, 1, "[STRATNAME]"),
    ("TOOL-066.25", r25, 1, "[CDEXCORE]"),
    ("TOOL-066.26", r26, 1, "python/pipeline/hft_pipeline/core.py"),
    ("TOOL-066.27", r27, 0, ""),
    ("TOOL-066.28", r28, 1, "[HISTORY]"),
    ("TOOL-066.29", r29, 0, ""),
    ("TOOL-066.30", r30, 0, ""),
    ("TOOL-066.31", r31, 1, "deleted component `old-comp`"),
    (
        "TOOL-066.32",
        r32,
        1,
        ".agents/notes/proposed/architecture/2026-10-01-sample.md:",
    ),
    ("TOOL-066.33", r33, 1, "class directory `misc`"),
    ("TOOL-066.34", r34, 0, ""),
    ("TOOL-066.35", r35, 1, "AGENTS.md: length 1801 > 1800"),
    ("TOOL-066.36", r36, 1, "crates/AGENTS.md: length 901 > 900"),
    ("TOOL-066.37", r37, 1, "docs/architecture.md: length 3001 > 3000"),
    ("TOOL-066.38", r38, 1, "ring `hk`"),
    ("TOOL-066.39", r39, 1, "ring `foo`"),
    ("TOOL-066.40", r40, 0, ""),
    ("TOOL-066.41", r41, 1, "missing required section `账户配置核验`"),
    ("TOOL-066.42", r42, 1, "lacks p50 or p99"),
    ("TOOL-066.43", r43, 1, "docs/runbooks/x.md: missing `## 回退`"),
    ("TOOL-066.44", r44, 1, "out of order"),
    ("TOOL-066.45", r45, 0, ""),
];

#[test]
fn tool_066_check_counterexamples() {
    assert_eq!(DOC_ROWS.len(), 45);
    std::thread::scope(|sc| {
        let handles: Vec<_> = DOC_ROWS
            .chunks(6)
            .map(|chunk| {
                sc.spawn(move || {
                    let mut fails = Vec::new();
                    for (row, edit, want, needle) in chunk {
                        let f = docs_clean(&row.to_lowercase().replace('.', "-"));
                        edit(&f);
                        let run = f.xtask(&["doc-check"]);
                        if run.code != *want {
                            fails.push(format!("{row}: exit {} (want {want})\n{}", run.code, run.stderr));
                        } else if !needle.is_empty() && !run.stderr.contains(needle) {
                            fails.push(format!("{row}: output lacks '{needle}'\n{}", run.stderr));
                        }
                    }
                    fails
                })
            })
            .collect();
        let fails: Vec<String> = handles.into_iter().flat_map(|h| h.join().unwrap()).collect();
        assert!(fails.is_empty(), "{}", fails.join("\n"));
    });
}
