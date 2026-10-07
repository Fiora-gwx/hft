//! Linux process helpers for `proc` tests: child roles (this test binary re-executed with one
//! test and a role in the environment), `/dev/shm` scratch directories and strace segments.

use std::collections::BTreeMap;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command, ExitStatus, Stdio};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::time::{Duration, Instant};

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

/// Longest a parent waits for one line from a child, for a child to exit, or for a traced run;
/// past it the child is killed and the test fails instead of hanging the job.
pub const CHILD_DEADLINE: Duration = Duration::from_secs(60);

/// A child process running only `test` from this binary with `role` and extra environment.
/// Dropping it kills and reaps the child.
pub struct Kid {
    pub child: Child,
    stdin: Option<ChildStdin>,
    lines: Receiver<String>,
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
    let (tx, lines) = mpsc::channel();
    std::thread::spawn(move || forward(out, &tx));
    Kid { child, stdin, lines }
}

/// Sends every `say` line of the child to the channel until EOF. The marker is not necessarily at
/// the line start: with one test thread libtest prints "test <name> ... " before the test runs.
fn forward(mut out: BufReader<ChildStdout>, tx: &Sender<String>) {
    loop {
        let mut l = String::new();
        if out.read_line(&mut l).unwrap_or(0) == 0 {
            return;
        }
        if let Some(i) = l.find("@@")
            && tx.send(l[i + 2..].trim_end().to_string()).is_err()
        {
            return;
        }
    }
}

/// Waits for `child` up to `CHILD_DEADLINE`; kills it and panics past that.
pub fn wait_deadline(child: &mut Child) -> ExitStatus {
    let t0 = Instant::now();
    loop {
        if let Some(st) = child.try_wait().unwrap() {
            return st;
        }
        if t0.elapsed() > CHILD_DEADLINE {
            let _ = child.kill();
            let _ = child.wait();
            panic!("child {} did not exit within {CHILD_DEADLINE:?}", child.id());
        }
        std::thread::sleep(Duration::from_millis(5));
    }
}

impl Kid {
    /// Next line the child printed with `say` (libtest chatter is skipped).
    pub fn hear(&mut self) -> String {
        match self.lines.recv_timeout(CHILD_DEADLINE) {
            Ok(l) => l,
            Err(RecvTimeoutError::Disconnected) => {
                panic!("child closed stdout before saying anything")
            }
            Err(RecvTimeoutError::Timeout) => {
                let _ = self.child.kill();
                let _ = self.child.wait();
                panic!("child said nothing for {CHILD_DEADLINE:?}");
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

    /// Exit code (panics on a signal death or after `CHILD_DEADLINE`).
    pub fn code(mut self) -> i32 {
        drop(self.stdin.take());
        let st = wait_deadline(&mut self.child);
        st.code()
            .unwrap_or_else(|| panic!("child died by signal: {st:?}"))
    }

    /// Waits up to `CHILD_DEADLINE` (stdin closed first).
    pub fn wait(&mut self) -> ExitStatus {
        drop(self.stdin.take());
        wait_deadline(&mut self.child)
    }

    pub fn kill(mut self) {
        let _ = self.child.kill(); // SIGKILL
        let _ = self.child.wait();
    }
}

impl Drop for Kid {
    fn drop(&mut self) {
        // No-op for a child already reaped (its pid is not signalled again).
        if matches!(self.child.try_wait(), Ok(None)) {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
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
    let mut ch = cmd
        .spawn()
        .unwrap_or_else(|e| panic!("strace not available ({e}); CI installs it"));
    let st = wait_deadline(&mut ch);
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
