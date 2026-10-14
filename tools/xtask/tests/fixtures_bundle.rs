//! Real filesystem/codec transport tests and closed replay-dispatch contract tests.

use std::fs;

use serde_json::json;
use sha2::{Digest, Sha256};
use xtask::exec::{Cmd, Exec, Output};
use xtask::{EnvError, fixtures, replay_verify};

use crate::common::{Fixture, Script};
use crate::fixtures_fetch::{Store, cache_names, compressed};

const POINTER: &str = "fixtures/replay/bundle.pointer.toml";
const MEMBERS: &[(&str, &[u8])] = &[
    ("prod/seq/0000000001.jrn", b"first\x00\xff"),
    ("prod/seq/0000000001.seal", b"first seal"),
    ("prod/seq/0000000002.jrn", b"second\x00\xff"),
    ("prod/seq/0000000002.seal", b"second seal"),
    ("ckpt/tsa.ckpt", b"model checkpoint"),
];

fn uri(path: &str) -> String {
    format!("s3://test-bucket/hft-fixtures/bundle/{path}.zst")
}
fn manifest() -> String {
    let mut text =
        "format = \"files-v1\"\nstrategy_set = \"testkit-replay-v1\"\njournal = \"prod/seq\"\n".to_owned();
    for (path, data) in MEMBERS {
        text.push_str(&format!(
            "\n[[files]]\npath = {path:?}\nuri = {:?}\nsha256 = \"{:x}\"\nbytes = {}\n",
            uri(path),
            Sha256::digest(data),
            data.len()
        ));
    }
    text.push_str("\n[[checks]]\nname = \"verify\"\nadvice = \"journal\"\n\n[[checks]]\nname = \"model\"\nadvice = \"recompute\"\n\n[[checks]]\nname = \"window\"\nadvice = \"recompute\"\ncheckpoint = \"ckpt/tsa.ckpt\"\nfrom_seq = 20\nto_seq = 30\n");
    text
}
fn store(f: &Fixture) -> Store {
    let mut s = Store::new(f);
    for (path, data) in MEMBERS {
        s.objects.insert(uri(path), compressed(f, data));
    }
    s
}
fn cached(f: &Fixture) {
    f.write(POINTER, &manifest());
    for (path, data) in MEMBERS {
        let p = f.path(&format!("target/fixtures-cache/bundle/{path}"));
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, data).unwrap();
    }
}
fn fetch(f: &Fixture, s: &mut Store) -> i32 {
    fixtures::fetch::run_with(&f.root, s)
}
fn replay(f: &Fixture, exec: &mut dyn Exec) -> i32 {
    replay_verify::run_with(&f.root, &[POINTER.into()], exec)
}

#[test]
fn tool_198_bundle_fetch_is_complete_and_exact_cache_is_reused() {
    let f = Fixture::empty("bundle-fetch");
    f.write(POINTER, &manifest());
    let mut s = store(&f);
    assert_eq!(fetch(&f, &mut s), 0);
    assert_eq!(
        s.calls.iter().filter(|c| c.program == "aws").count(),
        MEMBERS.len()
    );
    assert_eq!(cache_names(&f), ["bundle"]);
    for (path, data) in MEMBERS {
        assert_eq!(
            fs::read(f.path(&format!("target/fixtures-cache/bundle/{path}"))).unwrap(),
            *data
        );
    }
    s.calls.clear();
    s.objects.clear();
    assert_eq!(fetch(&f, &mut s), 0);
    assert!(s.calls.is_empty());
}

#[test]
fn tool_199_bundle_failure_never_publishes_partial_set_and_refreshes_corruption() {
    for old in [false, true] {
        let f = Fixture::empty("bundle-failure");
        f.write(POINTER, &manifest());
        let corrupt = "target/fixtures-cache/bundle/prod/seq/0000000001.jrn";
        if old {
            cached(&f);
            fs::write(f.path(corrupt), b"corrupt").unwrap();
        }
        let mut s = store(&f);
        // Last member in sorted acquisition order; most of the new tree has already been staged.
        let last = uri("prod/seq/0000000002.seal");
        let bytes = s.objects.remove(&last).unwrap();
        assert_eq!(fetch(&f, &mut s), 2);
        assert_eq!(
            s.calls.iter().filter(|c| c.program == "aws").count(),
            MEMBERS.len()
        );
        if old {
            assert_eq!(fs::read(f.path(corrupt)).unwrap(), b"corrupt");
        } else {
            assert!(cache_names(&f).is_empty());
        }
        let mut commands = Replies::new(0);
        assert_eq!(replay(&f, &mut commands), 2);
        assert!(commands.calls.is_empty());
        s.objects.insert(last, bytes);
        assert_eq!(fetch(&f, &mut s), 0);
        assert_eq!(cache_names(&f), ["bundle"]);
        assert_eq!(replay(&f, &mut commands), 0);
        assert_eq!(commands.calls.len(), 3);
    }
}

#[test]
fn tool_200_bundle_manifests_preflight_before_any_side_effects() {
    let good = manifest();
    let mut bad: Vec<String> = [
        ("files-v1", "files-v9"),
        ("testkit-replay-v1", "arbitrary-executable"),
        ("prod/seq\"", "absent\""),
        ("path = \"ckpt/tsa.ckpt\"", "path = \"../tsa.ckpt\""),
        ("path = \"ckpt/tsa.ckpt\"", "path = \"/tsa.ckpt\""),
        ("path = \"ckpt/tsa.ckpt\"", "path = \"C:/tsa.ckpt\""),
        ("path = \"ckpt/tsa.ckpt\"", "path = \"ckpt//tsa.ckpt\""),
        ("path = \"ckpt/tsa.ckpt\"", "path = \"ckpt/./tsa.ckpt\""),
        ("path = \"ckpt/tsa.ckpt\"", "path = \"ckpt/NUL.ckpt\""),
        ("path = \"ckpt/tsa.ckpt\"", "path = \"ckpt/tsa.ckpt.\""),
        ("path = \"ckpt/tsa.ckpt\"", "path = \"prod/seq\""),
        ("path = \"ckpt/tsa.ckpt\"", "path = \"prod/SEQ/tsa.ckpt\""),
        ("path = \"ckpt/tsa.ckpt\"", "path = \"prod/seq/0000000001.jrn\""),
        (
            "path = \"prod/seq/0000000001.seal\"",
            "path = \"ckpt/unpaired.seal\"",
        ),
        ("advice = \"journal\"", "advice = \"other\""),
        ("name = \"window\"", "name = \"VERIFY\""),
        ("checkpoint = \"ckpt/tsa.ckpt\"", "checkpoint = \"absent.ckpt\""),
        ("checkpoint = \"ckpt/tsa.ckpt\"", "checkpoint = 3"),
        ("from_seq = 20", "from_seq = 0"),
        ("from_seq = 20", "from_seq = 31"),
        ("from_seq = 20", "from_seq = \"20\""),
        ("to_seq = 30", "to_seq = -1"),
        ("bytes = 7", "bytes = 0"),
        ("s3://test-bucket", "https://test-bucket"),
        ("format = \"files-v1\"", "format = 1"),
    ]
    .iter()
    .map(|(a, b)| {
        assert!(good.contains(a), "{a}");
        good.replacen(a, b, 1)
    })
    .collect();
    bad.push(format!("command = \"ignored-command\"\n{good}"));
    bad.push(good.replace("advice = \"recompute\"", "advice = \"journal\""));
    bad.push(good.replace("[[checks]]", "[[checks]]\ncommand = \"extra\""));
    bad.push(good.replace("[[files]]", "[[files]]\nextra = true"));
    let value: toml::Table = good.parse().unwrap();
    for key in ["files", "checks"] {
        let mut t = value.clone();
        t.insert(key.into(), toml::Value::Array(Vec::new()));
        bad.push(toml::to_string(&t).unwrap());
    }
    for (row, text) in bad.iter().enumerate() {
        let f = Fixture::empty("bundle-invalid");
        f.write("fixtures/replay/a.pointer.toml", &good);
        f.write(POINTER, text);
        let mut s = Store::new(&f);
        assert_eq!(fetch(&f, &mut s), 2, "row {row}");
        assert!(s.calls.is_empty(), "row {row}");
        assert!(!f.path("target").exists(), "row {row}");
        assert_eq!(replay(&f, &mut s), 2, "row {row}");
    }
}

#[test]
fn tool_201_bundle_replay_rejects_changed_missing_and_extra_members() {
    for row in 0..5 {
        let f = Fixture::empty("bundle-cache");
        cached(&f);
        let path = f.path("target/fixtures-cache/bundle/prod/seq/0000000001.jrn");
        match row {
            0 => {
                fs::write(path, b"bad\x00\xff!!").unwrap();
            }
            1 => {
                fs::remove_file(path).unwrap();
            }
            2 => {
                f.write("target/fixtures-cache/bundle/extra", "unexpected");
            }
            3 => {
                fs::create_dir(f.path("target/fixtures-cache/bundle/extra")).unwrap();
            }
            _ => {
                fs::remove_dir_all(f.path("target/fixtures-cache/bundle/ckpt")).unwrap();
            }
        }
        let mut s = Replies::new(0);
        assert_eq!(replay(&f, &mut s), 2, "row {row}");
        assert!(s.calls.is_empty());
    }
}

struct Replies {
    code: i32,
    calls: Vec<Cmd>,
    stdout: String,
}
impl Replies {
    fn new(code: i32) -> Self {
        Self {
            code,
            calls: Vec::new(),
            stdout: json!({"summary": {"verify":"pass", "recompute":"pass", "mismatches":0,
            "outputs_compared":12,"snapshots_checked":2,"model_advice_compared":3}})
            .to_string(),
        }
    }
}
impl Exec for Replies {
    fn run(&mut self, c: &Cmd) -> Result<Output, EnvError> {
        self.calls.push(c.clone());
        Ok(Output {
            code: if self.calls.len() == 1 { self.code } else { 0 },
            stdout: self.stdout.clone(),
            stderr: "diagnostic".into(),
        })
    }
}

#[test]
fn tool_202_bundle_replay_dispatches_closed_strategy_and_all_declared_checks() {
    let f = Fixture::empty("bundle-dispatch");
    cached(&f);
    for strategy in ["testkit-replay-v1", "registry"] {
        f.write(POINTER, &manifest().replace("testkit-replay-v1", strategy));
        for (code, want, count) in [(0, 0, 3), (1, 1, 3), (3, 1, 3), (4, 1, 3), (2, 2, 1), (-1, 2, 1)] {
            let mut exec = Replies::new(code);
            assert_eq!(replay(&f, &mut exec), want);
            assert_eq!(exec.calls.len(), count);
            for (i, cmd) in exec.calls.iter().enumerate() {
                assert_eq!(cmd.program, "cargo");
                assert!(cmd.capture);
                let mut expected = ["run", "--locked", "--profile", "ci", "-p", "replayer"]
                    .map(str::to_owned)
                    .to_vec();
                if strategy == "testkit-replay-v1" {
                    expected.extend(["--example".into(), "replay_probe".into()]);
                }
                expected.extend([
                    "--".into(),
                    "verify".into(),
                    "--journal".into(),
                    f.path("target")
                        .join("fixtures-cache")
                        .join("bundle")
                        .join("prod/seq")
                        .display()
                        .to_string(),
                    "--advice".into(),
                    if i == 0 { "journal" } else { "recompute" }.into(),
                ]);
                if i == 2 {
                    expected.extend([
                        "--model-checkpoint".into(),
                        f.path("target")
                            .join("fixtures-cache")
                            .join("bundle")
                            .join("ckpt/tsa.ckpt")
                            .display()
                            .to_string(),
                        "--from-seq".into(),
                        "20".into(),
                        "--to-seq".into(),
                        "30".into(),
                    ]);
                }
                assert_eq!(cmd.args, expected);
            }
        }
    }
    let mut exec = Replies::new(0);
    assert_eq!(
        replay_verify::run_with(
            &f.root,
            &[POINTER.into(), "--from-seq".into(), "20".into()],
            &mut exec
        ),
        2
    );
    assert!(exec.calls.is_empty());
    // Also exercise the actual xtask CLI and its single aggregate JSON, without spawning cargo.
    for (code, want, count) in [(0, 0, 3), (1, 1, 3), (2, 2, 1), (3, 1, 3), (4, 1, 3)] {
        let report = Replies::new(code).stdout;
        let script = Script::new(&f, &[("cargo", code, &report, "diagnostic")], 99);
        let out = f.xtask_script(&["replay-verify", POINTER], &script, &[]);
        assert_eq!(out.code, want, "{}", out.stderr);
        let json: serde_json::Value = serde_json::from_str(&out.stdout).unwrap();
        assert_eq!(json["bundle"], "bundle");
        assert_eq!(json["exit_code"], want);
        assert_eq!(json["checks_planned"], 3);
        assert_eq!(json["checks"].as_array().unwrap().len(), count);
        assert_eq!(script.calls().len(), count);
        for row in json["checks"].as_array().unwrap() {
            assert_eq!(row["exit_code"], code);
            assert_eq!(row["status"], want);
            assert_eq!(row["stderr"], "diagnostic");
            assert!(row["report"].is_object());
        }
    }
}

#[test]
fn tool_203_bundle_success_requires_nonempty_consistent_report() {
    let f = Fixture::empty("bundle-report");
    cached(&f);
    for bad in ["", "garbage", "null", "{}", "{\"summary\":{\"verify\":\"pass\"}}"] {
        let mut exec = Replies::new(0);
        exec.stdout = bad.into();
        assert_eq!(replay(&f, &mut exec), 2);
        assert_eq!(exec.calls.len(), 1);
    }
    for (key, value, count) in [
        ("verify", json!("fail"), 1),
        ("mismatches", json!(1), 1),
        ("outputs_compared", json!(0), 1),
        ("snapshots_checked", json!(0), 1),
        ("recompute", json!("skipped"), 2),
        ("model_advice_compared", json!(0), 2),
    ] {
        let mut exec = Replies::new(0);
        let mut value0: serde_json::Value = serde_json::from_str(&exec.stdout).unwrap();
        value0["summary"][key] = value;
        exec.stdout = value0.to_string();
        assert_eq!(replay(&f, &mut exec), 2);
        assert_eq!(exec.calls.len(), count);
    }
    let mut exec = Replies::new(1);
    exec.stdout.clear();
    assert_eq!(replay(&f, &mut exec), 2);
}

#[test]
fn tool_204_bundle_lock_and_links_never_touch_external_targets() {
    let f = Fixture::empty("bundle-lock");
    cached(&f);
    f.write("target/fixtures-cache/.bundle-bundle.lock", "held");
    let mut s = Store::new(&f);
    assert_eq!(fetch(&f, &mut s), 2);
    assert!(s.calls.is_empty());
    assert_eq!(
        fs::read_to_string(f.path("target/fixtures-cache/.bundle-bundle.lock")).unwrap(),
        "held"
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::symlink;
        for member in ["ckpt/tsa.ckpt", "ckpt", ""] {
            let f = Fixture::empty("bundle-links");
            cached(&f);
            f.write("outside/sentinel", "unchanged");
            // symlink() needs the entry itself, not a trailing-slash directory dereference.
            let p = if member.is_empty() {
                f.path("target/fixtures-cache/bundle")
            } else {
                f.path("target/fixtures-cache/bundle").join(member)
            };
            let dir = p.is_dir();
            if dir {
                fs::remove_dir_all(&p).unwrap();
            } else {
                fs::remove_file(&p).unwrap();
            }
            symlink(f.path(if dir { "outside" } else { "outside/sentinel" }), &p).unwrap();
            let mut s = Store::new(&f);
            assert_eq!(fetch(&f, &mut s), 2);
            assert_eq!(replay(&f, &mut s), 2);
            assert!(s.calls.is_empty());
            assert_eq!(
                fs::read_to_string(f.path("outside/sentinel")).unwrap(),
                "unchanged"
            );
            assert!(fs::symlink_metadata(p).unwrap().file_type().is_symlink());
        }
    }
}
