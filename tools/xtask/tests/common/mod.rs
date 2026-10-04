//! Fixture repos for xtask integration tests, built in a fresh temporary directory per test
//! (convention: tools/TESTS.md "用例" preamble).

#![allow(dead_code)]

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
        let out = Command::new(env!("CARGO_BIN_EXE_xtask"))
            .arg("--root")
            .arg(&self.root)
            .args(args)
            .output()
            .unwrap();
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

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}
