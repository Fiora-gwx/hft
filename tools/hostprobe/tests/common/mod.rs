//! Per-test copy of the `c32-root` fixture root and the `upgrade-c32.toml` expected profile
//! (tools/TESTS.md TOOL-100 preconditions), plus runners for the binary and the library.

#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

use serde_json::Value;

static N: AtomicUsize = AtomicUsize::new(0);

fn copy_dir(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for e in std::fs::read_dir(from).unwrap() {
        let e = e.unwrap();
        let t = to.join(e.file_name());
        if e.file_type().unwrap().is_dir() {
            copy_dir(&e.path(), &t);
        } else {
            std::fs::copy(e.path(), t).unwrap();
        }
    }
}

/// Result of one binary run.
#[derive(Debug)]
pub struct Run {
    pub code: i32,
    pub stdout: String,
    pub stderr: String,
}

/// A temporary directory with `root/` (the fixture root), `upgrade-c32.toml` and `grub/`.
#[derive(Debug)]
pub struct Fx {
    pub dir: PathBuf,
}

impl Fx {
    pub fn new(tag: &str) -> Fx {
        let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
        let n = N.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("hostprobe-{tag}-{}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        copy_dir(&src.join("c32-root"), &dir.join("root"));
        copy_dir(&src.join("grub"), &dir.join("grub"));
        std::fs::copy(src.join("upgrade-c32.toml"), dir.join("upgrade-c32.toml")).unwrap();
        Fx { dir }
    }

    pub fn root(&self) -> PathBuf {
        self.dir.join("root")
    }

    pub fn path(&self, rel: &str) -> PathBuf {
        self.dir.join(rel)
    }

    /// Writes `<dir>/<rel>` (`root/proc/cmdline`, `upgrade-c32.toml`, …).
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

    /// Replaces exactly one occurrence of `from` in `rel`.
    pub fn replace(&self, rel: &str, from: &str, to: &str) -> &Self {
        let s = self.read(rel);
        assert_eq!(s.matches(from).count(), 1, "{rel}: '{from}' must occur once");
        self.write(rel, &s.replacen(from, to, 1))
    }

    /// A runnable task (`/proc/<pid>/task/<pid>/stat`, state R) on `cpu`.
    pub fn task(&self, pid: u32, comm: &str, cpu: u32) -> &Self {
        let zeros = vec!["0"; 29].join(" ");
        let stat = format!("{pid} ({comm}) R 1 1 1 0 -1 4194560 {zeros} {cpu} 0 0 0 0 0\n");
        self.write(&format!("root/proc/{pid}/stat"), &stat);
        self.write(&format!("root/proc/{pid}/task/{pid}/stat"), &stat)
    }

    pub fn non_root(&self) -> &Self {
        self.write("root/.hostprobe/euid", "1000\n")
    }

    /// `hostprobe --root <root> <args…>`; relative file arguments are under the temp dir.
    pub fn run(&self, args: &[&str]) -> Run {
        let out = Command::new(env!("CARGO_BIN_EXE_hostprobe"))
            .arg("--root")
            .arg(self.root())
            .args(args)
            .current_dir(&self.dir)
            .output()
            .unwrap();
        Run {
            code: out.status.code().unwrap_or(-1),
            stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
        }
    }

    /// `verify --expected upgrade-c32.toml --out v.json`.
    pub fn verify(&self) -> Run {
        self.run(&["verify", "--expected", "upgrade-c32.toml", "--out", "v.json"])
    }

    pub fn json(&self, rel: &str) -> Value {
        serde_json::from_str(&self.read(rel)).unwrap()
    }
}

impl Drop for Fx {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

/// Value at a dotted key (exactly one match).
pub fn at(v: &Value, key: &str) -> Value {
    let f = hostprobe::json::lookup(v, key);
    assert_eq!(f.len(), 1, "{key}");
    f[0].value.clone().unwrap_or_else(|| panic!("{key} missing"))
}
