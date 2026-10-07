//! `cargo xtask gen-schema` (crates/hft-event/TESTS.md EVT-172, EVT-174–EVT-180; tools/TESTS.md
//! TOOL-057, TOOL-058, TOOL-060, TOOL-061, TOOL-179).
//!
//! Source edits of the specs ("改 crates/hft-event 的源码") are applied to the `Model` built from the
//! linked crates (the stand-in allowed by the specs: the binary would link the edited source).

mod common;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;

use common::Fixture;
use xtask::gen_schema::lock::{self, LOCK_PATH};
use xtask::gen_schema::model::{
    Code, Field, KindRow, LayoutRow, Model, NoteConst, NoteReg, Strategies, StrategyRow, Ty,
};
use xtask::gen_schema::{self, Baseline, Options, Report, render};

/// `(id, KINDS_EXT, extra KINDS row, exit code)`.
type ExtRow = (&'static str, Option<Vec<KindRow>>, Option<KindRow>, i32);
/// `(id, edit, output must contain)`.
type EditRow = (&'static str, fn(&mut Model), &'static [&'static str]);
/// `(id, edit, exit code, output must contain)`.
type EditCodeRow = (&'static str, fn(&mut Model), i32, &'static [&'static str]);
/// `(id, edit of the exported strategy rows, exit code, output must contain)`.
type StratRow = (&'static str, fn(&mut Vec<StrategyRow>), i32, &'static str);

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..").join("..")
}

/// The model of the real tree (linked hft-event / hft-refdata), strategies `Exported(strategies)`.
fn real(strategies: Vec<StrategyRow>) -> Model {
    let mut m = gen_schema::model::from_crates(&repo_root());
    m.strategies = Strategies::Exported(strategies);
    m
}

/// The test strategy stand-in of EVT-176 (`kind_id` 0xFF01).
fn test_strategy() -> StrategyRow {
    StrategyRow {
        kind_id: 0xFF01,
        name: "test-probe".into(),
        notes: vec![Code::new(3, "ProbeEdge"), Code::new(5, "ProbeSkip")],
        ops: vec![Code::new(1, "open"), Code::new(2, "close")],
        tags: vec![Code::new(1, "groups"), Code::new(2, "path")],
    }
}

fn run(root: &Path, check: bool, baseline: Option<String>, m: &Model) -> Report {
    gen_schema::run_model(
        root,
        &Options {
            check,
            baseline: Baseline::Text(baseline),
        },
        m,
    )
}

fn base_lock(m: &Model) -> String {
    lock::render(m, None)
}

fn kind<'a>(m: &'a mut Model, name: &str) -> &'a mut KindRow {
    m.kinds.iter_mut().find(|k| k.name == name).unwrap()
}

fn field(name: &str, offset: u16, ty: Ty) -> Field {
    Field {
        name: name.into(),
        offset,
        ty,
        measure: false,
        doc: String::new(),
    }
}

fn set_size(k: &mut KindRow, size: u16) {
    k.layout = match k.layout {
        LayoutRow::Fixed { .. } => LayoutRow::Fixed { size },
        LayoutRow::Var { max, .. } => LayoutRow::Var { prefix: size, max },
        LayoutRow::Multi { .. } => LayoutRow::Multi { prefix: size },
    };
}

/// Appends `f` at the end of the body (size grows by `w`).
fn append(k: &mut KindRow, f: Field, w: u16) {
    k.fields.push(f);
    let s = k.layout.size() + w;
    set_size(k, s);
}

/// Every file under `root` except `target/` and `.git/`: path → bytes.
fn snapshot(root: &Path) -> BTreeMap<String, Vec<u8>> {
    walkdir::WalkDir::new(root)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
        .filter(|e| {
            let r = xtask::rel_path(root, e.path());
            !r.starts_with("target/") && !r.starts_with(".git/")
        })
        .map(|e| (xtask::rel_path(root, e.path()), std::fs::read(e.path()).unwrap()))
        .collect()
}

/// A fresh directory holding the outputs of `m` (no baseline); returns it and the lock text.
fn generated(name: &str, m: &Model) -> (Fixture, String) {
    let f = Fixture::empty(name);
    let r = run(&f.root, false, None, m);
    assert_eq!(r.code, 0, "{}", r.text());
    let l = f.read(LOCK_PATH);
    (f, l)
}

/// Stand-in `NOTE_CODES` tables: every registry name of `sources`, numbered 1.. per source.
fn stand_in_consts(m: &Model, sources: &[&str]) -> Vec<NoteConst> {
    let mut out = Vec::new();
    for src in sources {
        for (i, n) in m.notes.iter().filter(|n| n.source == *src).enumerate() {
            out.push(NoteConst {
                source: n.source.clone(),
                n: i as u32 + 1,
                name: n.name.clone(),
                origin: format!("stand-in/{src}"),
            });
        }
    }
    out
}

fn git(dir: &Path, args: &[&str]) {
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args([
            "-c",
            "user.name=hft-test",
            "-c",
            "user.email=hft-test@localhost",
            "-c",
            "commit.gpgsign=false",
        ])
        .args(args)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

fn git_init_commit(dir: &Path) {
    git(dir, &["init", "-q", "-b", "main"]);
    git(dir, &["add", "-A"]);
    git(dir, &["commit", "-q", "-m", "base"]);
}

fn xtask_bin(root: &Path, args: &[&str]) -> (i32, String) {
    let out = Command::new(env!("CARGO_BIN_EXE_xtask"))
        .env_remove("HFT_CORE_BIN")
        .env_remove("GITHUB_EVENT_NAME")
        .env_remove("GITHUB_BASE_REF")
        .arg("--root")
        .arg(root)
        .args(args)
        .output()
        .unwrap();
    let text = String::from_utf8_lossy(&out.stdout).into_owned() + &String::from_utf8_lossy(&out.stderr);
    (out.status.code().unwrap_or(-1), text)
}

// ---------------------------------------------------------------- hft-event cases

#[test]
fn evt_172_outputs_and_python_dtype() {
    let mut m = real(vec![test_strategy()]);
    m.note_consts = stand_in_consts(&m, &["Oms", "Ledger"]);
    let (f, lock_text) = generated("evt172", &m);
    assert_eq!(hft_event::NOTE_NAMES.len(), 85);
    assert_eq!(m.kinds.len(), 39);

    // event-catalog.md: 39 kind sections, each with number, domain, role, layout, schema_version, field table.
    let cat = f.read(render::CATALOG_MD);
    let kinds_part = cat.split("## 容器结构").next().unwrap();
    let sections: Vec<&str> = kinds_part.split("\n### 0x").skip(1).collect();
    assert_eq!(sections.len(), 39);
    for s in &sections {
        for key in [
            "- 编号:",
            "- 域:",
            "- 角色:",
            "- 布局:",
            "- schema_version:",
            "| 偏移 | 字段 | 类型 |",
        ] {
            assert!(
                s.contains(key),
                "section {} lacks {key}",
                &s[..s.find('\n').unwrap()]
            );
        }
    }
    for n in hft_event::NOTE_NAMES {
        assert!(
            cat.contains(&format!("| `{}` |", n.name)),
            "catalog lacks Note {}",
            n.name
        );
    }

    // Arrow: 39 files; px/qty columns Int64 with hft.type.
    let arrow: Vec<_> = std::fs::read_dir(f.path(render::ARROW_DIR)).unwrap().collect();
    assert_eq!(arrow.len(), 39);
    let mut px_qty = 0;
    for k in &m.kinds {
        let v: serde_json::Value = serde_json::from_str(&f.read(&render::arrow_path(&k.name))).unwrap();
        for flat in lock::flatten(&k.fields, &m.enums) {
            if flat.ty == "px" || flat.ty == "qty" {
                let col = v["fields"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .find(|c| c["name"] == flat.name.as_str())
                    .unwrap();
                assert_eq!(col["type"], "Int64", "{} {}", k.name, flat.name);
                assert_eq!(
                    col["metadata"]["hft.type"],
                    flat.ty.as_str(),
                    "{} {}",
                    k.name,
                    flat.name
                );
                px_qty += 1;
            }
        }
    }
    assert!(px_qty > 39);

    // Python: each kind's dtype matches FieldDesc; SPB1 itemsizes; LoadErrCode.
    let py = xtask::doc_check::find_python().expect("Python 3 on PATH");
    let script = r#"
import json, sys
sys.path.insert(0, sys.argv[1])
import events, refdata
out = {"kinds": {}, "structs": {}, "load_err": {str(k): v for k, v in refdata.LOAD_ERR_CODE.items()}}
for num, (name, ver, layout, dt) in events.KINDS.items():
    out["kinds"][str(num)] = {"itemsize": dt.itemsize, "names": list(dt.names), "offsets": [dt.fields[n][1] for n in dt.names]}
for name, dt in events.STRUCTS.items():
    out["structs"][name] = dt.itemsize
print(json.dumps(out))
"#;
    let out = Command::new(&py)
        .arg("-c")
        .arg(script)
        .arg(f.path("python/_generated"))
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "python: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    for k in &m.kinds {
        let d = &v["kinds"][k.kind.to_string()];
        assert_eq!(d["itemsize"], k.layout.size(), "{}", k.name);
        let names: Vec<&str> = k.fields.iter().map(|x| x.name.as_str()).collect();
        let offsets: Vec<u16> = k.fields.iter().map(|x| x.offset).collect();
        assert_eq!(d["names"], serde_json::json!(names), "{}", k.name);
        assert_eq!(d["offsets"], serde_json::json!(offsets), "{}", k.name);
    }
    assert_eq!(v["structs"]["StrategyParamsHdr"], 48);
    assert_eq!(v["structs"]["GroupSpec"], 80);
    assert_eq!(v["structs"]["LegDef"], 16);
    let le: Vec<(&str, u16)> = hft_refdata::schema::load_err_codes().collect();
    assert_eq!(v["load_err"].as_object().unwrap().len(), le.len());
    for (name, val) in le {
        assert_eq!(v["load_err"][val.to_string()], name);
    }

    // Lock: 39 kinds, 24 CmdKind, every LoadErrCode.
    let l = lock::parse(&lock_text).unwrap();
    assert_eq!(l.kinds.len(), 39);
    assert_eq!(l.cmds.len(), 24);
    assert_eq!(l.load_errs.len(), hft_refdata::schema::load_err_codes().count());

    // notes_catalog.rs: one entry per (source, n), name and default level as NOTE_NAMES.
    let rs = f.read(render::NOTES_RS);
    for c in &m.note_consts {
        let src = gen_schema::model::note_source_value(&c.source).unwrap();
        let reg = hft_event::NOTE_NAMES.iter().find(|r| r.name == c.name).unwrap();
        let level = reg.level.map_or(0, |l| l as u8);
        let line = format!("source: {src}, n: {}, name: \"{}\", level: {level},", c.n, c.name);
        assert_eq!(rs.matches(&line).count(), 1, "{line}");
    }
    assert_eq!(rs.matches("NoteEntry {").count(), m.note_consts.len() + 1);
}

#[test]
fn evt_174_check_detects_drift() {
    let fresh = |name: &str| -> Fixture {
        let f = Fixture::empty(name);
        let m = gen_schema::load(&f.root).unwrap();
        assert!(matches!(m.strategies, Strategies::Skipped(_)));
        let r = run(&f.root, false, None, &m);
        assert_eq!(r.code, 0, "{}", r.text());
        git_init_commit(&f.root);
        f
    };
    // .1 unchanged.
    let f = fresh("evt174-1");
    let (code, out) = xtask_bin(&f.root, &["gen-schema", "--check"]);
    assert_eq!(code, 0, "EVT-174.1: {out}");
    assert!(!out.contains("differs:"), "EVT-174.1: {out}");
    // .2 Bbo offset edited in events.py.
    let f = fresh("evt174-2");
    let py = f.read(render::EVENTS_PY);
    let line = py.lines().find(|l| l.starts_with("BBO = ")).unwrap().to_string();
    f.write(
        render::EVENTS_PY,
        &py.replacen(&line, &line.replacen("56, 64", "56, 65", 1), 1),
    );
    let (code, out) = xtask_bin(&f.root, &["gen-schema", "--check"]);
    assert_eq!(code, 1, "EVT-174.2: {out}");
    assert!(out.contains("python/_generated/events.py"), "EVT-174.2: {out}");
    // .3 schemas/arrow/bbo.json deleted.
    let f = fresh("evt174-3");
    f.remove("schemas/arrow/bbo.json");
    let (code, out) = xtask_bin(&f.root, &["gen-schema", "--check"]);
    assert_eq!(code, 1, "EVT-174.3: {out}");
    assert!(out.contains("schemas/arrow/bbo.json"), "EVT-174.3: {out}");
    // .4 one catalog line edited.
    let f = fresh("evt174-4");
    f.replace(render::CATALOG_MD, "# 事件目录", "# 事件目录(手改)");
    let (code, out) = xtask_bin(&f.root, &["gen-schema", "--check"]);
    assert_eq!(code, 1, "EVT-174.4: {out}");
    // .5 apps/hft-core in the tree but no built binary.
    let f = fresh("evt174-5");
    f.write("apps/hft-core/Cargo.toml", "[package]\nname = \"hft-core\"\n");
    let (code, out) = xtask_bin(&f.root, &["gen-schema", "--check"]);
    assert_eq!(code, 2, "EVT-174.5: {out}");
}

fn row_175(id: &str, edit: impl Fn(&mut Model), want: i32, contains: &[&str]) -> (Fixture, Report) {
    let base = real(vec![]);
    let mut m = base.clone();
    edit(&mut m);
    let f = Fixture::empty(&id.to_lowercase().replace('.', "-"));
    let r = run(&f.root, false, Some(base_lock(&base)), &m);
    assert_eq!(r.code, want, "{id}: {}", r.text());
    for c in contains {
        assert!(r.text().contains(c), "{id}: output lacks '{c}': {}", r.text());
    }
    (f, r)
}

/// The MarketHdr nested field `h` of a market kind.
fn market_hdr(k: &mut KindRow) -> Option<&mut Vec<Field>> {
    k.fields
        .iter_mut()
        .find(|f| f.name == "h")
        .and_then(|f| match &mut f.ty {
            Ty::Nested(inner) => Some(inner),
            _ => None,
        })
}

#[test]
fn evt_175_append_only_rules() {
    // .1 ExternalFill + foo: u64 (88 B), v2 -> v3.
    let (f, _) = row_175(
        "EVT-175.1",
        |m| {
            let k = kind(m, "ExternalFill");
            append(k, field("foo", 80, Ty::U64), 8);
            k.schema_version = 3;
        },
        0,
        &[],
    );
    let lt = f.read(LOCK_PATH);
    assert!(lt.contains("kind 0x0207 ExternalFill v3 fixed 88\n"), "EVT-175.1");
    let base = lock::parse(&base_lock(&real(vec![]))).unwrap();
    let old = base.kinds.iter().find(|k| k.name == "ExternalFill").unwrap();
    assert_eq!(old.fields.len(), 15);
    let new = lock::parse(&lt).unwrap();
    let newk = new.kinds.iter().find(|k| k.name == "ExternalFill").unwrap();
    for (a, b) in old.fields.iter().zip(&newk.fields) {
        assert_eq!(
            (a.offset, &a.name, &a.ty),
            (b.offset, &b.name, &b.ty),
            "EVT-175.1"
        );
    }
    // .2 same without version bump.
    row_175(
        "EVT-175.2",
        |m| append(kind(m, "ExternalFill"), field("foo", 80, Ty::U64), 8),
        1,
        &["ExternalFill", "schema_version"],
    );
    // .3 OrderDecision px <-> qty.
    row_175(
        "EVT-175.3",
        |m| {
            let k = kind(m, "OrderDecision");
            for f in k.fields.iter_mut() {
                match f.name.as_str() {
                    "px" => f.offset = 24,
                    "qty" => f.offset = 16,
                    _ => {}
                }
            }
            k.fields.sort_by_key(|f| f.offset);
        },
        1,
        &["moved"],
    );
    // .4 rename OrderDecision.reason.
    row_175(
        "EVT-175.4",
        |m| {
            kind(m, "OrderDecision")
                .fields
                .iter_mut()
                .find(|f| f.name == "reason")
                .unwrap()
                .name = "reason_code".into()
        },
        1,
        &["reason"],
    );
    // .5 ExternalFill.trade_id u64 -> i64.
    row_175(
        "EVT-175.5",
        |m| {
            kind(m, "ExternalFill")
                .fields
                .iter_mut()
                .find(|f| f.name == "trade_id")
                .unwrap()
                .ty = Ty::I64
        },
        1,
        &["retyped"],
    );
    // .6 delete Trade.trade_id (+1 version).
    row_175(
        "EVT-175.6",
        |m| {
            let k = kind(m, "Trade");
            let i = k.fields.iter().position(|f| f.name == "trade_id").unwrap();
            k.fields.remove(i);
            for f in k.fields.iter_mut().skip(i) {
                f.offset -= 8;
            }
            let s = k.layout.size() - 8;
            set_size(k, s);
            k.schema_version += 1;
        },
        1,
        &["trade_id"],
    );
    // .7 insert foo before Bbo.update_id (+1 version).
    row_175(
        "EVT-175.7",
        |m| {
            let k = kind(m, "Bbo");
            let i = k.fields.iter().position(|f| f.name == "update_id").unwrap();
            k.fields[i].offset += 8;
            k.fields.insert(i, field("foo", 88, Ty::U64));
            set_size(k, 104);
            k.schema_version += 1;
        },
        1,
        &["update_id"],
    );
    // .8 / .9 LatencySample._pad2 byte 1 -> foo: u8 / Side.
    let occupy = |ty: Ty| {
        move |m: &mut Model| {
            let k = kind(m, "LatencySample");
            let i = k.fields.iter().position(|f| f.name == "_pad2").unwrap();
            let off = k.fields[i].offset;
            k.fields[i] = field("_pad2", off + 1, Ty::Pad(5));
            k.fields.insert(i, field("foo", off, ty.clone()));
        }
    };
    row_175("EVT-175.8", occupy(Ty::U8), 0, &[]);
    row_175("EVT-175.9", occupy(Ty::Enum("Side".into())), 1, &["foo", "Side"]);
    // .10 ExternalFill + foo_side: Side (+ _pad), +1 version.
    row_175(
        "EVT-175.10",
        |m| {
            let k = kind(m, "ExternalFill");
            k.fields.push(field("foo_side", 80, Ty::Enum("Side".into())));
            append(k, field("_pad2", 81, Ty::Pad(7)), 8);
            k.schema_version += 1;
        },
        1,
        &["foo_side"],
    );
    // .11 MarketHdr + foo: u64 (56 -> 64), market kinds +1 version.
    row_175(
        "EVT-175.11",
        |m| {
            for k in m.kinds.iter_mut() {
                let Some(h) = market_hdr(k) else { continue };
                h.push(field("foo", 56, Ty::U64));
                for f in k.fields.iter_mut().filter(|f| f.name != "h") {
                    f.offset += 8;
                }
                let s = k.layout.size() + 8;
                set_size(k, s);
                k.schema_version += 1;
            }
        },
        1,
        &["Bbo", "moved"],
    );
    // .12 MarketHdr._rsv -> foo: u32.
    row_175(
        "EVT-175.12",
        |m| {
            for k in m.kinds.iter_mut() {
                if let Some(h) = market_hdr(k) {
                    let r = h.iter_mut().find(|f| f.name == "_rsv").unwrap();
                    r.name = "foo".into();
                    r.ty = Ty::U32;
                }
            }
        },
        0,
        &[],
    );
    // .13 no lock in the baseline.
    let f = Fixture::empty("evt175-13");
    let r = run(&f.root, false, None, &real(vec![]));
    assert_eq!(r.code, 0, "EVT-175.13: {}", r.text());
    assert!(f.path(LOCK_PATH).is_file(), "EVT-175.13");
    // .14 L2Delta deprecated.
    let (f, _) = row_175("EVT-175.14", |m| kind(m, "L2Delta").deprecated = true, 0, &[]);
    assert!(
        f.read(LOCK_PATH)
            .contains("kind 0x0102 L2Delta v1 fixed 96 deprecated\n"),
        "EVT-175.14"
    );
}

fn row_176(id: &str, base: &Model, edit: impl Fn(&mut Model), want: i32, contains: &[&str]) -> Fixture {
    let mut m = base.clone();
    edit(&mut m);
    let f = Fixture::empty(&id.to_lowercase().replace('.', "-"));
    let r = run(&f.root, false, Some(base_lock(base)), &m);
    assert_eq!(r.code, want, "{id}: {}", r.text());
    for c in contains {
        assert!(r.text().contains(c), "{id}: output lacks '{c}': {}", r.text());
    }
    f
}

fn strat(m: &mut Model) -> &mut Vec<StrategyRow> {
    match &mut m.strategies {
        Strategies::Exported(v) => v,
        Strategies::Skipped(_) => panic!("strategies not exported"),
    }
}

#[test]
fn evt_176_numbers_never_vanish_or_reuse() {
    let mut base = real(vec![test_strategy()]);
    base.note_consts = stand_in_consts(&base, &["Oms"]);
    let b = &base;
    row_176(
        "EVT-176.1",
        b,
        |m| m.kinds.retain(|k| k.kind != 0x0108),
        1,
        &["0x0108"],
    );
    row_176(
        "EVT-176.2",
        b,
        |m| kind(m, "Liquidation").name = "Foo".into(),
        1,
        &["0x0108", "reused"],
    );
    row_176(
        "EVT-176.3",
        b,
        |m| m.cmd_kinds.retain(|c| c.value != 0x0016),
        1,
        &["0x0016"],
    );
    row_176(
        "EVT-176.4",
        b,
        |m| m.cmd_kinds.push(Code::new(0x000B, "NewCmd")),
        1,
        &["0x000B"],
    );
    row_176(
        "EVT-176.5",
        b,
        |m| m.cmd_kinds.iter_mut().find(|c| c.value == 0x0017).unwrap().name = "SetBaseline".into(),
        1,
        &["0x0017"],
    );
    row_176(
        "EVT-176.6",
        b,
        |m| m.load_err.retain(|c| c.name != "DanglingRef"),
        1,
        &["DanglingRef"],
    );
    row_176(
        "EVT-176.7",
        b,
        |m| m.load_err.iter_mut().find(|c| c.value == 6).unwrap().name = "NewName".into(),
        1,
        &["LoadErrCode 6"],
    );
    let f = row_176(
        "EVT-176.8",
        b,
        |m| {
            let mut k = m.kinds.iter().find(|k| k.kind == 0x0101).unwrap().clone();
            k.kind = 0x010A;
            k.name = "FooTick".into();
            k.layout = LayoutRow::Fixed { size: 8 };
            k.fields = vec![field("value", 0, Ty::U64)];
            m.kinds.insert(9, k);
        },
        0,
        &[],
    );
    assert!(
        f.read(LOCK_PATH)
            .contains("kind 0x010A FooTick v1 fixed 8\n  0 value u64\n"),
        "EVT-176.8"
    );
    row_176(
        "EVT-176.9",
        b,
        |m| m.cmd_kinds.push(Code::new(0x001D, "NewCmd")),
        0,
        &[],
    );
    row_176(
        "EVT-176.10",
        b,
        |m| {
            let next = m.load_err.iter().map(|c| c.value).max().unwrap() + 1;
            m.load_err.push(Code::new(next, "NewErr"));
        },
        0,
        &[],
    );
    row_176(
        "EVT-176.11",
        b,
        |m| {
            let max = m.note_consts.iter().map(|c| c.n).max().unwrap();
            let c = m.note_consts.iter_mut().find(|c| c.name == "OmsAnomaly").unwrap();
            let old_n = c.n;
            c.n = max + 1;
            m.note_consts.push(NoteConst {
                source: "Oms".into(),
                n: old_n,
                name: "OmsFoo".into(),
                origin: "stand-in".into(),
            });
            m.notes.push(NoteReg {
                name: "OmsFoo".into(),
                source: "Oms".into(),
                level: None,
                v: vec![],
            });
        },
        1,
        &["(Oms,", "OmsAnomaly", "reused"],
    );
    row_176("EVT-176.12", b, |m| strat(m).clear(), 1, &["0xFF01"]);
    row_176(
        "EVT-176.13",
        b,
        |m| strat(m)[0].notes[0].name = "ProbeRenamed".into(),
        1,
        &["(65281, Note 3)"],
    );
    row_176(
        "EVT-176.14",
        b,
        |m| strat(m)[0].ops.retain(|c| c.value != 2),
        1,
        &["(65281, op 2)"],
    );
    row_176(
        "EVT-176.15",
        b,
        |m| strat(m)[0].tags[0].name = "renamed".into(),
        1,
        &["(65281, tag 1)"],
    );
    row_176(
        "EVT-176.16",
        b,
        |m| {
            let s = &mut strat(m)[0];
            s.notes.push(Code::new(9, "ProbeNew"));
            s.ops.push(Code::new(4, "flip"));
            s.tags.push(Code::new(3, "extra"));
        },
        0,
        &[],
    );
    row_176(
        "EVT-176.17",
        b,
        |m| {
            let mut other = test_strategy();
            other.kind_id = 0xFF02;
            other.name = "test-other".into();
            strat(m).push(other);
        },
        0,
        &[],
    );
}

#[test]
fn evt_177_note_const_tables() {
    let mut base = real(vec![test_strategy()]);
    base.note_consts = stand_in_consts(&base, &["Oms", "Ledger", "Exec"]);
    let b = &base;
    let add = |m: &mut Model, src: &str, n: u32, name: &str| {
        m.note_consts.push(NoteConst {
            source: src.into(),
            n,
            name: name.into(),
            origin: "stand-in".into(),
        })
    };
    let max_n = |m: &Model, src: &str| {
        m.note_consts
            .iter()
            .filter(|c| c.source == src)
            .map(|c| c.n)
            .max()
            .unwrap_or(0)
    };
    row_176("EVT-177.1", b, |_| {}, 0, &[]);
    row_176(
        "EVT-177.2",
        b,
        |m| m.note_consts.retain(|c| c.name != "OmsAnomaly"),
        1,
        &["OmsAnomaly"],
    );
    row_176(
        "EVT-177.3",
        b,
        |m| add(m, "Oms", max_n(m, "Oms") + 1, "OmsNewThing"),
        1,
        &["OmsNewThing"],
    );
    row_176(
        "EVT-177.4",
        b,
        |m| {
            m.note_consts
                .iter_mut()
                .find(|c| c.name == "ExternalFill")
                .unwrap()
                .source = "Risk".into()
        },
        1,
        &["ExternalFill", "source"],
    );
    row_176(
        "EVT-177.5",
        b,
        |m| {
            let first = m.note_consts.iter().find(|c| c.source == "Exec").unwrap().n;
            m.note_consts
                .iter_mut()
                .filter(|c| c.source == "Exec")
                .nth(1)
                .unwrap()
                .n = first;
        },
        1,
        &["(Exec,"],
    );
    row_176(
        "EVT-177.6",
        b,
        |m| add(m, "Venue", 1, "ReconnectCount"),
        1,
        &["ReconnectCount", "Venue"],
    );
    row_176("EVT-177.7", b, |m| add(m, "Venue", 1, "VenueReconnects"), 0, &[]);
    row_176(
        "EVT-177.8",
        b,
        |m| add(m, "Housekeeping", 1, "HkDiskSample"),
        0,
        &[],
    );
    row_176(
        "EVT-177.9",
        b,
        |m| {
            let n = m.note_consts.iter().find(|c| c.name == "OmsAnomaly").unwrap().n;
            strat(m)[0].notes.push(Code::new(n + 100, "ProbeLater"));
            strat(m)[0].notes.push(Code::new(n, "ProbeSameAsOms"));
        },
        0,
        &[],
    );
    row_176(
        "EVT-177.10",
        b,
        |m| add(m, "Ledger", 0, "RoundClosed"),
        1,
        &["n = 0"],
    );
    row_176(
        "EVT-177.11",
        b,
        |m| strat(m)[0].notes.push(Code::new(0, "ProbeZero")),
        1,
        &["code 0"],
    );
}

fn ext_kind(num: u16, name: &str) -> KindRow {
    KindRow {
        kind: num,
        name: name.into(),
        domain: "Chain".into(),
        role: "Input".into(),
        layout: LayoutRow::Fixed { size: 8 },
        schema_version: 1,
        journals: "chain".into(),
        crc: "never".into(),
        optional: true,
        deprecated: false,
        fields: vec![field("value", 0, Ty::U64)],
    }
}

#[test]
fn evt_178_ext_proto_merge() {
    let rows: [ExtRow; 5] = [
        (
            "EVT-178.1",
            Some(vec![ext_kind(0x0310, "ExtQuote"), ext_kind(0x0311, "ExtTx")]),
            None,
            0,
        ),
        ("EVT-178.2", Some(vec![ext_kind(0x0301, "ExtHead")]), None, 1),
        ("EVT-178.3", Some(vec![ext_kind(0x0340, "ExtFar")]), None, 1),
        ("EVT-178.4", None, Some(ext_kind(0x0311, "BaseInExt")), 1),
        ("EVT-178.5", None, None, 0),
    ];
    for (id, ext, extra_base, want) in rows {
        let mut m = real(vec![]);
        m.ext_kinds = ext;
        if let Some(k) = extra_base {
            m.kinds.push(k);
        }
        let f = Fixture::empty(&id.to_lowercase().replace('.', "-"));
        let r = run(&f.root, false, None, &m);
        assert_eq!(r.code, want, "{id}: {}", r.text());
        if id == "EVT-178.1" {
            let cat = f.read(render::CATALOG_MD);
            let sec = cat.split("### 0x0310 ExtQuote").nth(1).unwrap();
            assert!(
                sec.split("\n### ").next().unwrap().contains("来源:hft-ext-proto"),
                "{id}"
            );
        }
    }
}

#[test]
fn evt_179_no_write_on_violation() {
    let base = real(vec![]);
    let (f, l) = generated("evt179", &base);
    let before = snapshot(&f.root);
    let mut m = base.clone();
    append(kind(&mut m, "ExternalFill"), field("foo", 80, Ty::U64), 8);
    let r = run(&f.root, false, Some(l), &m);
    assert_eq!(r.code, 1, "{}", r.text());
    assert_eq!(
        snapshot(&f.root),
        before,
        "generated files changed or files added"
    );
}

#[test]
fn evt_180_ci_baseline_merge_base() {
    let base = real(vec![]);
    let (f, _) = generated("evt180", &base);
    git_init_commit(&f.root);
    git(&f.root, &["checkout", "-q", "-b", "pr"]);
    let mut m = base.clone();
    m.kinds.retain(|k| k.kind != 0x0108);
    // The PR commit regenerates everything (the lock loses the 0x0108 block) from the edited model.
    let r = run(&f.root, false, None, &m);
    assert_eq!(r.code, 0, "{}", r.text());
    assert!(!f.read(LOCK_PATH).contains("0x0108"));
    git(&f.root, &["commit", "-q", "-am", "drop Liquidation"]);
    let ci = Options {
        check: true,
        baseline: Baseline::Git {
            pr_base: Some("main".into()),
        },
    };
    let r = gen_schema::run_model(&f.root, &ci, &m);
    assert_eq!(r.code, 1, "step 1: {}", r.text());
    assert!(r.text().contains("0x0108"), "step 1: {}", r.text());
    assert!(r.baseline.contains("merge base"), "step 1: {}", r.baseline);
    let local = Options {
        check: true,
        baseline: Baseline::Git { pr_base: None },
    };
    let r = gen_schema::run_model(&f.root, &local, &m);
    assert_eq!(r.code, 0, "step 2: {}", r.text());
}

// ---------------------------------------------------------------- tools cases (fixture `events`)

fn enum_table(name: &str, width: u8, items: &[(u32, &str)]) -> gen_schema::model::EnumTable {
    gen_schema::model::EnumTable {
        name: name.into(),
        width,
        items: items.iter().map(|(v, n)| Code::new(*v, n)).collect(),
    }
}

/// Fixture `events`: 3 kinds, a two-name Note registry with its `hft-oms` const table, 22 LoadErrCodes.
fn events_fixture() -> Model {
    let mk = |kind: u16, name: &str, layout: LayoutRow, fields: Vec<Field>| KindRow {
        kind,
        name: name.into(),
        domain: "Market".into(),
        role: "Input".into(),
        layout,
        schema_version: 1,
        journals: "seq".into(),
        crc: "never".into(),
        optional: false,
        deprecated: false,
        fields,
    };
    let mut enums = BTreeMap::new();
    for e in [
        enum_table("Side", 1, &[(1, "Buy"), (2, "Sell")]),
        enum_table("NoteSource", 1, &[(3, "Risk"), (4, "Oms")]),
        enum_table("NoteLevel", 1, &[(1, "P0"), (2, "P1"), (3, "P2"), (4, "Info")]),
    ] {
        enums.insert(e.name.clone(), e);
    }
    Model {
        kinds: vec![
            mk(
                0x0101,
                "Alpha",
                LayoutRow::Fixed { size: 16 },
                vec![field("a", 0, Ty::U64), field("b", 8, Ty::Px)],
            ),
            mk(
                0x0201,
                "Beta",
                LayoutRow::Fixed { size: 16 },
                vec![
                    field("side", 0, Ty::Enum("Side".into())),
                    field("_pad", 1, Ty::Pad(7)),
                    field("qty", 8, Ty::Qty),
                ],
            ),
            mk(
                0x0501,
                "Gamma",
                LayoutRow::Var { prefix: 8, max: 64 },
                vec![field("len", 0, Ty::U32), field("_pad", 4, Ty::Pad(4))],
            ),
        ],
        ext_kinds: None,
        cmd_kinds: vec![Code::new(1, "SetMode")],
        load_err: (1..=22).map(|v| Code::new(v, &format!("Err{v}"))).collect(),
        notes: vec![
            NoteReg {
                name: "OmsTransition".into(),
                source: "Oms".into(),
                level: Some("Info".into()),
                v: vec![],
            },
            NoteReg {
                name: "OmsAnomaly".into(),
                source: "Oms".into(),
                level: None,
                v: vec![("slot".into(), 0, 32)],
            },
        ],
        note_consts: vec![
            NoteConst {
                source: "Oms".into(),
                n: 1,
                name: "OmsTransition".into(),
                origin: "crates/hft-oms/src/notes.rs".into(),
            },
            NoteConst {
                source: "Oms".into(),
                n: 2,
                name: "OmsAnomaly".into(),
                origin: "crates/hft-oms/src/notes.rs".into(),
            },
        ],
        conn_reasons: vec![],
        structs: vec![],
        enums,
        rdb_rows: vec![],
        measure_mask: vec![],
        strategies: Strategies::Exported(vec![]),
    }
}

#[test]
fn tool_057_generates_four_outputs() {
    let m = events_fixture();
    let f = Fixture::empty("tool057");
    let r = run(&f.root, false, None, &m);
    assert_eq!(r.code, 0, "{}", r.text());
    for p in [
        "schemas/arrow/alpha.json",
        "schemas/arrow/beta.json",
        "schemas/arrow/gamma.json",
    ] {
        assert!(f.path(p).is_file(), "{p}");
    }
    assert_eq!(std::fs::read_dir(f.path(render::ARROW_DIR)).unwrap().count(), 3);
    for p in [render::EVENTS_PY, render::CATALOG_MD, render::NOTES_RS, LOCK_PATH] {
        assert!(f.path(p).is_file(), "{p}");
    }
    let before = snapshot(&f.root);
    let r = run(&f.root, false, Some(f.read(LOCK_PATH)), &m);
    assert_eq!(r.code, 0, "{}", r.text());
    assert!(r.written.is_empty(), "second run rewrote {:?}", r.written);
    assert_eq!(snapshot(&f.root), before);
}

#[test]
fn tool_058_note_registry_mismatch() {
    let rows: [EditRow; 3] = [
        (
            "TOOL-058.1",
            |m| {
                m.notes.push(NoteReg {
                    name: "OmsFoo".into(),
                    source: "Oms".into(),
                    level: None,
                    v: vec![],
                })
            },
            &["OmsFoo"],
        ),
        (
            "TOOL-058.2",
            |m| {
                m.note_consts.push(NoteConst {
                    source: "Oms".into(),
                    n: 3,
                    name: "OMS_BAR".into(),
                    origin: "crates/hft-oms/src/notes.rs".into(),
                })
            },
            &["OMS_BAR"],
        ),
        (
            "TOOL-058.3",
            |m| {
                m.note_consts
                    .iter_mut()
                    .find(|c| c.name == "OmsAnomaly")
                    .unwrap()
                    .source = "Risk".into()
            },
            &["0x04000002", "0x03000002"],
        ),
    ];
    for (id, edit, contains) in rows {
        let mut m = events_fixture();
        edit(&mut m);
        let f = Fixture::empty(&id.to_lowercase().replace('.', "-"));
        let r = run(&f.root, false, None, &m);
        assert_eq!(r.code, 1, "{id}: {}", r.text());
        for c in contains {
            assert!(r.text().contains(c), "{id}: output lacks {c}: {}", r.text());
        }
    }
}

#[test]
fn tool_060_check_is_read_only() {
    let base = events_fixture();
    let (f, l) = generated("tool060", &base);
    let mut m = base.clone();
    let k = kind(&mut m, "Alpha");
    append(k, field("c", 16, Ty::U64), 8);
    k.schema_version = 2;
    let before = snapshot(&f.root);
    let r = run(&f.root, true, Some(l), &m);
    assert_eq!(r.code, 1, "{}", r.text());
    for p in [render::EVENTS_PY, "schemas/arrow/alpha.json", LOCK_PATH] {
        assert!(r.text().contains(p), "stderr lacks {p}: {}", r.text());
    }
    assert_eq!(snapshot(&f.root), before);
}

#[test]
fn tool_061_append_only_violations() {
    let base = events_fixture();
    let old_lock = base_lock(&base);
    let rows: [EditCodeRow; 3] = [
        (
            "TOOL-061.1",
            |m| {
                let k = kind(m, "Alpha");
                k.fields[0].offset = 8;
                k.fields[1].offset = 0;
                k.fields.sort_by_key(|f| f.offset);
            },
            1,
            &[],
        ),
        ("TOOL-061.2", |m| m.load_err.retain(|c| c.value != 22), 1, &["22"]),
        (
            "TOOL-061.3",
            |m| {
                let k = kind(m, "Gamma");
                k.fields[1] = field("count", 4, Ty::U32);
            },
            0,
            &[],
        ),
    ];
    for (id, edit, want, contains) in rows {
        let mut m = base.clone();
        edit(&mut m);
        // The tree holds the outputs regenerated from the edited source (lock included).
        let (f, _) = generated(&id.to_lowercase().replace('.', "-"), &m);
        let r = run(&f.root, true, Some(old_lock.clone()), &m);
        assert_eq!(r.code, want, "{id}: {}", r.text());
        for c in contains {
            assert!(r.text().contains(c), "{id}: output lacks {c}: {}", r.text());
        }
    }
}

fn probe_a() -> StrategyRow {
    StrategyRow {
        kind_id: 1,
        name: "probe-a".into(),
        notes: vec![Code::new(5, "ProbeEdge")],
        ops: vec![Code::new(1, "set_thr")],
        tags: vec![Code::new(2, "path")],
    }
}

#[test]
fn tool_179_strategy_append_only() {
    let mut base = events_fixture();
    base.strategies = Strategies::Exported(vec![probe_a()]);
    let rows: [StratRow; 9] = [
        ("TOOL-179.1", |_| {}, 0, ""),
        (
            "TOOL-179.2",
            |v| {
                v.push(StrategyRow {
                    kind_id: 2,
                    name: "probe-b".into(),
                    notes: vec![],
                    ops: vec![],
                    tags: vec![],
                })
            },
            0,
            "",
        ),
        ("TOOL-179.3", |v| v[0].name = "probe-x".into(), 1, "kind_id 1"),
        ("TOOL-179.4", |v| v.clear(), 1, "kind_id 1"),
        (
            "TOOL-179.5",
            |v| v[0].notes[0].name = "ProbeOther".into(),
            1,
            "(1, Note 5)",
        ),
        (
            "TOOL-179.6",
            |v| v[0].notes.push(Code::new(6, "ProbeMore")),
            0,
            "",
        ),
        (
            "TOOL-179.7",
            |v| v[0].ops[0].name = "set_win".into(),
            1,
            "(1, op 1)",
        ),
        ("TOOL-179.8", |v| v[0].tags.clear(), 1, "(1, tag 2)"),
        (
            "TOOL-179.9",
            |v| v[0].notes.push(Code::new(0, "ProbeZero")),
            1,
            "code 0",
        ),
    ];
    for (id, edit, want, contains) in rows {
        let (f, l) = generated(&id.to_lowercase().replace('.', "-"), &base);
        let before = snapshot(&f.root);
        let mut m = base.clone();
        let Strategies::Exported(v) = &mut m.strategies else {
            unreachable!()
        };
        edit(v);
        let r = run(&f.root, false, Some(l), &m);
        assert_eq!(r.code, want, "{id}: {}", r.text());
        assert!(
            r.text().contains(contains),
            "{id}: output lacks '{contains}': {}",
            r.text()
        );
        if id == "TOOL-179.2" {
            assert!(f.read(LOCK_PATH).contains("strategy 2 probe-b\n"), "{id}");
        }
        if want == 1 {
            // TOOL-179.10: python/_generated, docs/generated and the lock unchanged.
            assert_eq!(snapshot(&f.root), before, "TOOL-179.10 ({id})");
        }
    }
}
