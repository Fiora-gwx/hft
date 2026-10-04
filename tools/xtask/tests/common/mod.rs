//! Fixture repos for xtask integration tests, built in a fresh temporary directory per test
//! (convention: tools/TESTS.md "用例" preamble).

#![allow(dead_code)]

pub mod repos;

use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

static COUNTER: AtomicUsize = AtomicUsize::new(0);

/// A throw-away repo tree; deleted on drop.
#[derive(Debug)]
pub struct Fixture {
    pub root: PathBuf,
}

/// Result of one `xtask` run.
#[derive(Debug)]
pub struct Run {
    pub code: i32,
    pub stdout: String,
    pub stderr: String,
    pub json: Option<serde_json::Value>,
}

impl Fixture {
    pub fn empty(name: &str) -> Fixture {
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!("xtask-{name}-{}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        Fixture { root }
    }

    pub fn path(&self, rel: &str) -> PathBuf {
        self.root.join(rel)
    }

    pub fn write(&self, rel: &str, text: &str) -> &Self {
        let p = self.path(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, text).unwrap();
        self
    }

    pub fn read(&self, rel: &str) -> String {
        std::fs::read_to_string(self.path(rel)).unwrap()
    }

    pub fn remove(&self, rel: &str) -> &Self {
        std::fs::remove_file(self.path(rel)).unwrap();
        self
    }

    /// Replace exactly one occurrence of `from` in `rel`.
    pub fn replace(&self, rel: &str, from: &str, to: &str) -> &Self {
        let s = self.read(rel);
        assert_eq!(s.matches(from).count(), 1, "{rel}: '{from}' must occur once");
        self.write(rel, &s.replacen(from, to, 1))
    }

    pub fn append(&self, rel: &str, text: &str) -> &Self {
        let s = if self.path(rel).exists() {
            self.read(rel)
        } else {
            String::new()
        };
        self.write(rel, &(s + text))
    }

    /// Run `xtask --root <fixture> <args…>`; read `target/xtask/<subcommand>.json` if written.
    pub fn xtask(&self, args: &[&str]) -> Run {
        self.xtask_env(args, &[])
    }

    /// Like [`Fixture::xtask`] with environment changes (`None` removes the variable).
    pub fn xtask_env(&self, args: &[&str], env: &[(&str, Option<&str>)]) -> Run {
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_xtask"));
        cmd.arg("--root").arg(&self.root).args(args);
        for (k, v) in env {
            match v {
                Some(v) => cmd.env(k, v),
                None => cmd.env_remove(k),
            };
        }
        let out = cmd.output().unwrap();
        let json_path = self
            .root
            .join("target")
            .join("xtask")
            .join(format!("{}.json", args[0]));
        let json = std::fs::read_to_string(json_path)
            .ok()
            .map(|s| serde_json::from_str(&s).unwrap());
        Run {
            code: out.status.code().unwrap_or(-1),
            stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
            json,
        }
    }
}

/// Recording executor (tools/TESTS.md "用例" preamble): `(command-line prefix, exit code, stdout, stderr)`
/// rules, first match wins, everything else exits `default_code`.
#[derive(Debug)]
pub struct Script {
    pub script: PathBuf,
    pub log: PathBuf,
}

impl Script {
    pub fn new(f: &Fixture, rules: &[(&str, i32, &str, &str)], default_code: i32) -> Script {
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("xtask-exec-{}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let _ = f;
        let rules: Vec<serde_json::Value> = rules
            .iter()
            .map(|(p, c, o, e)| serde_json::json!({"prefix": p, "code": c, "stdout": o, "stderr": e}))
            .collect();
        let script = dir.join("script.json");
        std::fs::write(
            &script,
            serde_json::json!({"rules": rules, "default_code": default_code}).to_string(),
        )
        .unwrap();
        Script {
            script,
            log: dir.join("log.jsonl"),
        }
    }

    pub fn env(&self) -> Vec<(&'static str, Option<String>)> {
        vec![
            ("XTASK_EXEC_SCRIPT", Some(self.script.display().to_string())),
            ("XTASK_EXEC_LOG", Some(self.log.display().to_string())),
        ]
    }

    /// Recorded command lines (`program arg1 arg2 …`).
    pub fn calls(&self) -> Vec<String> {
        std::fs::read_to_string(&self.log)
            .unwrap_or_default()
            .lines()
            .map(|l| {
                let v: serde_json::Value = serde_json::from_str(l).unwrap();
                let mut s = v["program"].as_str().unwrap().to_string();
                for a in v["args"].as_array().unwrap() {
                    s.push(' ');
                    s.push_str(a.as_str().unwrap());
                }
                s
            })
            .collect()
    }

    /// Recorded program names.
    pub fn programs(&self) -> Vec<String> {
        self.calls()
            .iter()
            .map(|c| c.split(' ').next().unwrap().to_string())
            .collect()
    }
}

impl Drop for Script {
    fn drop(&mut self) {
        if let Some(d) = self.script.parent() {
            let _ = std::fs::remove_dir_all(d);
        }
    }
}

impl Fixture {
    /// Runs with the recording executor `s` plus `env` changes.
    pub fn xtask_script(&self, args: &[&str], s: &Script, env: &[(&str, Option<&str>)]) -> Run {
        let se = s.env();
        let mut all: Vec<(&str, Option<&str>)> = se.iter().map(|(k, v)| (*k, v.as_deref())).collect();
        all.extend_from_slice(env);
        self.xtask_env(args, &all)
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}
