//! Linux process helpers for `proc` tests: child roles (this test binary re-executed with one test
//! and a role in the environment), line protocol over stdin/stdout, signals.
#![allow(dead_code)]

use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::time::{Duration, Instant};

pub const ROLE: &str = "HFT_JNL_ROLE";

pub fn role() -> Option<String> {
    std::env::var(ROLE).ok()
}

pub fn var(k: &str) -> String {
    std::env::var(k).unwrap_or_else(|_| panic!("missing env {k}"))
}

/// Longest a parent waits for one line from a child or for the child to exit; past it the child is
/// killed and the test fails instead of hanging the job.
pub const CHILD_DEADLINE: Duration = Duration::from_secs(60);

/// A child running only `test` of this binary with `role`. Dropping it kills and reaps the child.
pub struct Kid {
    child: Option<Child>,
    pid: i32,
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
    // Protocol lines go to the channel; libtest chatter is dropped. The thread ends at EOF.
    std::thread::spawn(move || forward(out, &tx));
    Kid {
        pid: child.id() as i32,
        child: Some(child),
        stdin,
        lines,
    }
}

fn forward(mut out: BufReader<ChildStdout>, tx: &Sender<String>) {
    loop {
        let mut l = String::new();
        if out.read_line(&mut l).unwrap_or(0) == 0 {
            return;
        }
        // Not necessarily at the line start: with one test thread libtest prints "test <name> ... "
        // before the test runs, and the child's first line follows it on the same line.
        if let Some(i) = l.find("@@")
            && tx.send(l[i + 2..].trim_end().to_string()).is_err()
        {
            return;
        }
    }
}

impl Kid {
    /// Next line the child said (`None` at EOF); libtest chatter is skipped. Panics (after killing
    /// the child) when nothing arrives within `CHILD_DEADLINE`.
    pub fn try_hear(&mut self) -> Option<String> {
        match self.lines.recv_timeout(CHILD_DEADLINE) {
            Ok(l) => Some(l),
            Err(RecvTimeoutError::Disconnected) => None,
            Err(RecvTimeoutError::Timeout) => {
                self.reap();
                panic!("child {} said nothing for {CHILD_DEADLINE:?}", self.pid);
            }
        }
    }

    pub fn hear(&mut self) -> String {
        self.try_hear().expect("child closed stdout")
    }

    pub fn tell(&mut self, msg: &str) {
        let s = self.stdin.as_mut().unwrap();
        writeln!(s, "{msg}").unwrap();
        s.flush().unwrap();
    }

    pub fn pid(&self) -> i32 {
        self.pid
    }

    pub fn signal(&self, sig: i32) {
        // SAFETY: sending a signal to our own child process.
        unsafe { libc::kill(self.pid, sig) };
    }

    /// SIGKILL and reap.
    pub fn kill(mut self) {
        self.reap();
    }

    /// Waits (closing its stdin first); `Err(signal)` when killed by a signal. Panics (after
    /// killing the child) when it has not exited within `CHILD_DEADLINE`.
    pub fn wait(mut self) -> Result<i32, i32> {
        use std::os::unix::process::ExitStatusExt;
        drop(self.stdin.take());
        let mut child = self.child.take().unwrap();
        let t0 = Instant::now();
        let st = loop {
            if let Some(st) = child.try_wait().unwrap() {
                break st;
            }
            if t0.elapsed() > CHILD_DEADLINE {
                let _ = child.kill();
                let _ = child.wait();
                panic!("child {} did not exit within {CHILD_DEADLINE:?}", self.pid);
            }
            std::thread::sleep(Duration::from_millis(5));
        };
        match st.code() {
            Some(c) => Ok(c),
            None => Err(st.signal().unwrap_or(0)),
        }
    }

    fn reap(&mut self) {
        if let Some(mut c) = self.child.take() {
            let _ = c.kill();
            let _ = c.wait();
        }
    }
}

impl Drop for Kid {
    fn drop(&mut self) {
        self.reap();
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

/// `ru_minflt` of this process.
pub fn minflt() -> i64 {
    // SAFETY: zeroed rusage is a valid out-buffer.
    let mut u: libc::rusage = unsafe { std::mem::zeroed() };
    // SAFETY: RUSAGE_SELF with a writable buffer.
    unsafe { libc::getrusage(libc::RUSAGE_SELF, &mut u) };
    u.ru_minflt
}

/// Calling thread's own accounting: CPU time and the `getrusage(RUSAGE_THREAD)` counters. Unlike
/// wall time it does not grow while the thread is preempted (another runnable thread on a busy
/// host); a call that blocks shows up as a voluntary context switch.
#[derive(Clone, Copy, Debug, Default)]
pub struct ThreadUsage {
    pub cpu: Duration,
    /// Voluntary context switches: the thread gave up the CPU (it blocked).
    pub nvcsw: i64,
    /// Involuntary context switches: the scheduler preempted the thread.
    pub nivcsw: i64,
    pub minflt: i64,
    pub majflt: i64,
}

impl ThreadUsage {
    pub fn now() -> ThreadUsage {
        // SAFETY: zeroed timespec/rusage are valid out-buffers.
        let mut ts: libc::timespec = unsafe { std::mem::zeroed() };
        // SAFETY: valid clock id and writable buffer.
        unsafe { libc::clock_gettime(libc::CLOCK_THREAD_CPUTIME_ID, &mut ts) };
        // SAFETY: as above.
        let mut u: libc::rusage = unsafe { std::mem::zeroed() };
        // SAFETY: RUSAGE_THREAD with a writable buffer.
        unsafe { libc::getrusage(libc::RUSAGE_THREAD, &mut u) };
        ThreadUsage {
            cpu: Duration::new(ts.tv_sec as u64, ts.tv_nsec as u32),
            nvcsw: u.ru_nvcsw,
            nivcsw: u.ru_nivcsw,
            minflt: u.ru_minflt,
            majflt: u.ru_majflt,
        }
    }

    /// Counters accumulated from `self` to `later`.
    pub fn until(self, later: ThreadUsage) -> ThreadUsage {
        ThreadUsage {
            cpu: later.cpu.saturating_sub(self.cpu),
            nvcsw: later.nvcsw - self.nvcsw,
            nivcsw: later.nivcsw - self.nivcsw,
            minflt: later.minflt - self.minflt,
            majflt: later.majflt - self.majflt,
        }
    }
}

/// A prerequisite from the CI job (mount points, sudo); `None` → the test reports and returns.
pub fn prereq(var: &str, what: &str) -> Option<String> {
    match std::env::var(var) {
        Ok(v) if !v.is_empty() => Some(v),
        _ => {
            eprintln!("prerequisite missing: {var} ({what})");
            None
        }
    }
}
