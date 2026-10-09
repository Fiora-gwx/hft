//! Command executor for external programs (tools/TESTS.md "用例" preamble). Subcommands that run
//! `cargo`, `git`, `gh`, `rustup`, … or other xtask subcommands go through [`Exec`], so tests can
//! substitute the recording executor:
//!
//! - `XTASK_EXEC_SCRIPT=<file.json>`: do not spawn anything; answer each command with the first rule
//!   whose `prefix` is a prefix of the command line (`program arg1 arg2 …`):
//!   `{"rules": [{"prefix": "xtask deps", "code": 1, "stdout": "", "stderr": ""}], "default_code": 0}`.
//!   A rule may also carry `"files": {"<path>": "<text>", …}`: written (parents created) each time
//!   the rule answers, paths relative to the repo root, so a scripted program can leave outputs
//!   (`hft-core schemas --out …`).
//! - `XTASK_EXEC_LOG=<file>`: append one JSON line `{"program": …, "args": […]}` per command (both
//!   executors), plus `"env": {…}` when the command sets environment variables.
//!
//! Program `xtask` means this binary (`<current exe> --root <root> …`); program `cargo` honours `$CARGO`.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use serde_json::{Value, json};

use crate::EnvError;

/// One external command.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cmd {
    pub program: String,
    pub args: Vec<String>,
    /// Capture stdout/stderr (else they are inherited).
    pub capture: bool,
    /// Extra environment variables.
    pub env: Vec<(String, String)>,
}

impl Cmd {
    /// A command whose output is inherited (build and test steps).
    pub fn new(program: &str, args: &[&str]) -> Cmd {
        Cmd {
            program: program.to_string(),
            args: args.iter().map(|s| s.to_string()).collect(),
            capture: false,
            env: Vec::new(),
        }
    }

    /// A command whose stdout/stderr are captured.
    pub fn capture(program: &str, args: &[&str]) -> Cmd {
        Cmd {
            capture: true,
            ..Cmd::new(program, args)
        }
    }

    pub fn arg(mut self, a: impl Into<String>) -> Cmd {
        self.args.push(a.into());
        self
    }

    /// `program arg1 arg2 …`.
    pub fn line(&self) -> String {
        let mut s = self.program.clone();
        for a in &self.args {
            s.push(' ');
            s.push_str(a);
        }
        s
    }
}

/// Result of one command.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Output {
    pub code: i32,
    pub stdout: String,
    pub stderr: String,
}

impl Output {
    pub fn ok(&self) -> bool {
        self.code == 0
    }
}

/// Runs external commands. `Err` = the program could not be started (maps to exit 2).
pub trait Exec {
    fn run(&mut self, cmd: &Cmd) -> Result<Output, EnvError>;
}

/// Spawns real processes.
#[derive(Debug)]
pub struct SysExec {
    root: PathBuf,
    log: Option<PathBuf>,
}

impl SysExec {
    pub fn new(root: &Path) -> SysExec {
        SysExec {
            root: root.to_path_buf(),
            log: None,
        }
    }
}

fn append_log(log: &Option<PathBuf>, cmd: &Cmd) {
    if let Some(p) = log
        && let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(p)
    {
        let mut v = json!({"program": cmd.program, "args": cmd.args});
        if !cmd.env.is_empty() {
            let env: serde_json::Map<String, Value> = cmd
                .env
                .iter()
                .map(|(k, x)| (k.clone(), Value::from(x.clone())))
                .collect();
            v["env"] = Value::Object(env);
        }
        let _ = writeln!(f, "{v}");
    }
}

impl Exec for SysExec {
    fn run(&mut self, cmd: &Cmd) -> Result<Output, EnvError> {
        append_log(&self.log, cmd);
        let (program, pre): (PathBuf, Vec<String>) = match cmd.program.as_str() {
            "xtask" => (
                std::env::current_exe().map_err(|e| EnvError(format!("current exe: {e}")))?,
                vec!["--root".into(), self.root.display().to_string()],
            ),
            "cargo" => (
                PathBuf::from(std::env::var("CARGO").unwrap_or_else(|_| "cargo".into())),
                Vec::new(),
            ),
            p => (PathBuf::from(p), Vec::new()),
        };
        let mut c = Command::new(&program);
        c.args(&pre).args(&cmd.args).current_dir(&self.root);
        for (k, v) in &cmd.env {
            c.env(k, v);
        }
        if cmd.capture {
            let out = c
                .stdin(Stdio::null())
                .output()
                .map_err(|e| EnvError(format!("run {}: {e}", cmd.program)))?;
            Ok(Output {
                code: out.status.code().unwrap_or(-1),
                stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
                stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
            })
        } else {
            let st = c
                .stdin(Stdio::null())
                .status()
                .map_err(|e| EnvError(format!("run {}: {e}", cmd.program)))?;
            Ok(Output {
                code: st.code().unwrap_or(-1),
                ..Output::default()
            })
        }
    }
}

/// One scripted answer.
#[derive(Debug, Clone, Default)]
pub struct Rule {
    pub prefix: String,
    pub out: Output,
    /// `(path, text)` written when the rule answers (relative paths against the executor root).
    pub files: Vec<(String, String)>,
}

/// Recording executor: answers from a script, never spawns.
#[derive(Debug, Default)]
pub struct ScriptExec {
    pub rules: Vec<Rule>,
    pub default_code: i32,
    /// Every command run, in order.
    pub calls: Vec<Cmd>,
    /// Base of relative rule `files` paths.
    pub root: PathBuf,
    log: Option<PathBuf>,
}

impl ScriptExec {
    pub fn new(rules: Vec<Rule>) -> ScriptExec {
        ScriptExec {
            rules,
            ..ScriptExec::default()
        }
    }

    /// Parses the JSON script format of the module docs.
    pub fn parse(text: &str) -> Result<ScriptExec, EnvError> {
        let v: Value = serde_json::from_str(text).map_err(|e| EnvError(format!("exec script: {e}")))?;
        let s = |r: &Value, k: &str| r.get(k).and_then(Value::as_str).unwrap_or("").to_string();
        let rules = v
            .get("rules")
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .map(|r| Rule {
                        files: r
                            .get("files")
                            .and_then(Value::as_object)
                            .map(|m| {
                                m.iter()
                                    .map(|(k, v)| (k.clone(), v.as_str().unwrap_or("").to_string()))
                                    .collect()
                            })
                            .unwrap_or_default(),
                        prefix: s(r, "prefix"),
                        out: Output {
                            code: r.get("code").and_then(Value::as_i64).unwrap_or(0) as i32,
                            stdout: s(r, "stdout"),
                            stderr: s(r, "stderr"),
                        },
                    })
                    .collect()
            })
            .unwrap_or_default();
        Ok(ScriptExec {
            rules,
            default_code: v.get("default_code").and_then(Value::as_i64).unwrap_or(0) as i32,
            ..ScriptExec::default()
        })
    }
}

impl Exec for ScriptExec {
    fn run(&mut self, cmd: &Cmd) -> Result<Output, EnvError> {
        append_log(&self.log, cmd);
        self.calls.push(cmd.clone());
        let line = cmd.line();
        let rule = self
            .rules
            .iter()
            .find(|r| line == r.prefix || line.starts_with(&format!("{} ", r.prefix)) || r.prefix.is_empty());
        if let Some(r) = rule {
            for (path, text) in &r.files {
                let p = self.root.join(path);
                if let Some(d) = p.parent() {
                    std::fs::create_dir_all(d)
                        .map_err(|e| EnvError(format!("create {}: {e}", d.display())))?;
                }
                std::fs::write(&p, text).map_err(|e| EnvError(format!("write {}: {e}", p.display())))?;
            }
        }
        let out = rule.map(|r| r.out.clone()).unwrap_or(Output {
            code: self.default_code,
            ..Output::default()
        });
        if !cmd.capture {
            // Inherited output goes to our stderr (stdout stays machine-only).
            eprint!("{}{}", out.stdout, out.stderr);
        }
        Ok(out)
    }
}

/// The executor selected by the environment (module docs).
pub fn from_env(root: &Path) -> Result<Box<dyn Exec>, EnvError> {
    let log = std::env::var_os("XTASK_EXEC_LOG").map(PathBuf::from);
    match std::env::var_os("XTASK_EXEC_SCRIPT") {
        Some(p) => {
            let text = std::fs::read_to_string(&p)
                .map_err(|e| EnvError(format!("read {}: {e}", Path::new(&p).display())))?;
            let mut s = ScriptExec::parse(&text)?;
            s.log = log;
            s.root = root.to_path_buf();
            Ok(Box::new(s))
        }
        None => Ok(Box::new(SysExec {
            root: root.to_path_buf(),
            log,
        })),
    }
}
