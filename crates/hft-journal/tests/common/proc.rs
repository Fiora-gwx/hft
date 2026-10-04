//! Linux process helpers for `proc` tests: child roles (this test binary re-executed with one test
//! and a role in the environment), line protocol over stdin/stdout, signals.
#![allow(dead_code)]

use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

pub const ROLE: &str = "HFT_JNL_ROLE";

pub fn role() -> Option<String> {
    std::env::var(ROLE).ok()
}

pub fn var(k: &str) -> String {
    std::env::var(k).unwrap_or_else(|_| panic!("missing env {k}"))
}

/// A child running only `test` of this binary with `role`.
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
    /// Next line the child said (`None` at EOF); libtest chatter is skipped.
    pub fn try_hear(&mut self) -> Option<String> {
        loop {
            let mut l = String::new();
            if self.out.read_line(&mut l).unwrap_or(0) == 0 {
                return None;
            }
            if let Some(m) = l.trim_end().strip_prefix("@@") {
                return Some(m.to_string());
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
        self.child.id() as i32
    }

    pub fn signal(&self, sig: i32) {
        // SAFETY: sending a signal to our own child process.
        unsafe { libc::kill(self.pid(), sig) };
    }

    /// SIGKILL and reap.
    pub fn kill(mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }

    /// Waits; `Err(signal)` when killed by a signal.
    pub fn wait(mut self) -> Result<i32, i32> {
        use std::os::unix::process::ExitStatusExt;
        drop(self.stdin.take());
        let st = self.child.wait().unwrap();
        match st.code() {
            Some(c) => Ok(c),
            None => Err(st.signal().unwrap_or(0)),
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

/// `ru_minflt` of this process.
pub fn minflt() -> i64 {
    // SAFETY: zeroed rusage is a valid out-buffer.
    let mut u: libc::rusage = unsafe { std::mem::zeroed() };
    // SAFETY: RUSAGE_SELF with a writable buffer.
    unsafe { libc::getrusage(libc::RUSAGE_SELF, &mut u) };
    u.ru_minflt
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
