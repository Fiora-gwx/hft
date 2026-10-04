//! Linux process helpers for `proc` tests: child roles (this test binary re-executed with one
//! test and a role in the environment), `/dev/shm` scratch directories and strace segments.

use std::collections::BTreeMap;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::atomic::{AtomicU32, Ordering};

pub const ROLE: &str = "HFT_RT_ROLE";
const TIDFILE: &str = "HFT_RT_TIDFILE";

pub fn role() -> Option<String> {
    std::env::var(ROLE).ok()
}

pub fn env(k: &str) -> String {
    std::env::var(k).unwrap_or_else(|_| panic!("missing env {k}"))
}

static SEQ: AtomicU32 = AtomicU32::new(0);

/// A fresh `/dev/shm/hft-test-<pid>-<n>/` directory.
pub fn shm_dir() -> PathBuf {
    let n = SEQ.fetch_add(1, Ordering::Relaxed);
    let d = PathBuf::from(format!("/dev/shm/hft-test-{}-{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

/// A child process running only `test` from this binary with `role` and extra environment.
pub struct Kid {
    pub child: Child,
    stdin: Option<ChildStdin>,
    out: BufReader<ChildStdout>,
}

pub fn spawn(test: &str, role: &str, envs: &[(&str, String)]) -> Kid {
    let exe = std::env::current_exe().unwrap();
    let mut cmd = Command::new(exe);
    cmd.args(["--exact", test, "--nocapture", "--test-threads=1"])
        .env(ROLE, role)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped());
    for (k, v) in envs {
        cmd.env(k, v);
    }
    let mut child = cmd.spawn().unwrap();
    let stdin = child.stdin.take();
    let out = BufReader::new(child.stdout.take().unwrap());
    Kid { child, stdin, out }
}

impl Kid {
    /// Next line the child printed with `say` (libtest chatter is skipped).
    pub fn hear(&mut self) -> String {
        loop {
            let mut l = String::new();
            if self.out.read_line(&mut l).unwrap() == 0 {
                panic!("child closed stdout before saying anything");
            }
            if let Some(m) = l.trim_end().strip_prefix("@@") {
                return m.to_string();
            }
        }
    }

    pub fn tell(&mut self, msg: &str) {
        let s = self.stdin.as_mut().unwrap();
        writeln!(s, "{msg}").unwrap();
        s.flush().unwrap();
    }

    pub fn pid(&self) -> u32 {
        self.child.id()
    }

    /// Exit code (panics on a signal death).
    pub fn code(mut self) -> i32 {
        drop(self.stdin.take());
        let st = self.child.wait().unwrap();
        st.code()
            .unwrap_or_else(|| panic!("child died by signal: {st:?}"))
    }

    pub fn kill(mut self) {
        let _ = self.child.kill(); // SIGKILL
        let _ = self.child.wait();
    }
}

/// Child side: a line to the parent.
pub fn say(msg: &str) {
    let mut o = std::io::stdout().lock();
    writeln!(o, "@@{msg}").unwrap();
    o.flush().unwrap();
}

/// Child side: the next line from the parent.
pub fn listen() -> String {
    let mut l = String::new();
    std::io::stdin().read_line(&mut l).unwrap();
    l.trim().to_string()
}

pub fn exit(code: i32) -> ! {
    std::process::exit(code)
}

/// Segment marker for strace: one `getppid` system call on the measured thread.
pub fn mark() {
    // SAFETY: getppid has no arguments and no side effects.
    unsafe { libc::getppid() };
}

/// Measured-thread side: record this thread's tid for the tracer (before the first marker).
pub fn register_traced_thread() {
    // SAFETY: gettid has no arguments.
    let tid = unsafe { libc::gettid() };
    std::fs::write(env(TIDFILE), tid.to_string()).unwrap();
}

/// Runs `test` in a child under `strace -f -ff` (role `traced`), tracing `syscalls` (`None` =
/// all). Returns, per segment between consecutive `mark()` calls of the registered thread, the
/// count of each traced system call.
pub fn strace_segments(test: &str, syscalls: Option<&str>) -> Vec<BTreeMap<String, u32>> {
    let dir = shm_dir();
    let prefix = dir.join("tr");
    let tidfile = dir.join("tid");
    let exe = std::env::current_exe().unwrap();
    let mut cmd = Command::new("strace");
    cmd.args(["-f", "-ff", "-qq", "-o"]).arg(&prefix);
    if let Some(s) = syscalls {
        cmd.arg("-e").arg(format!("trace={s},getppid"));
    }
    cmd.arg(&exe)
        .args(["--exact", test, "--nocapture", "--test-threads=1"])
        .env(ROLE, "traced")
        .env(TIDFILE, &tidfile)
        .stdout(Stdio::null());
    let st = cmd
        .status()
        .unwrap_or_else(|e| panic!("strace not available ({e}); CI installs it"));
    assert!(st.success(), "traced child failed: {st:?}");
    let tid = std::fs::read_to_string(&tidfile).unwrap();
    let trace = std::fs::read_to_string(trace_file(&prefix, tid.trim())).unwrap();
    let _ = std::fs::remove_dir_all(&dir);
    segments(&trace)
}

fn trace_file(prefix: &Path, tid: &str) -> PathBuf {
    let mut p = prefix.as_os_str().to_owned();
    p.push(format!(".{tid}"));
    PathBuf::from(p)
}

fn segments(trace: &str) -> Vec<BTreeMap<String, u32>> {
    let mut segs: Vec<BTreeMap<String, u32>> = Vec::new();
    let mut cur: Option<BTreeMap<String, u32>> = None;
    for l in trace.lines() {
        let name: String = l
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
            .collect();
        if name.is_empty() || !l[name.len()..].starts_with('(') || l.contains("resumed>") {
            continue;
        }
        if name == "getppid" {
            if let Some(s) = cur.take() {
                segs.push(s);
            }
            cur = Some(BTreeMap::new());
            continue;
        }
        if let Some(s) = cur.as_mut() {
            *s.entry(name).or_default() += 1;
        }
    }
    segs
}

pub fn n(seg: &BTreeMap<String, u32>, name: &str) -> u32 {
    seg.get(name).copied().unwrap_or(0)
}
