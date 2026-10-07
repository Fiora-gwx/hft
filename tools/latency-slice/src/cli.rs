//! Command line (README "命令行").

use std::path::PathBuf;

/// What to run.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Mode {
    Run,
    DryRun { seconds: u32, seed: u64 },
    Report { root: PathBuf },
    CheckConfig,
    BuildOnly { n: u16, print_napi: bool },
    IdlePolls { n: u64 },
    TwoPacket { n: u32 },
    LoopbackStream { minutes: u32 },
}

/// Parsed arguments.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Args {
    pub mode: Mode,
    pub config: PathBuf,
    pub minutes: Option<u32>,
    pub ts_expect_hw: bool,
    pub print_conn_stats: bool,
    pub ballast: Option<u8>,
    pub no_orders: bool,
    pub out: Option<PathBuf>,
}

/// A command-line error (exit code 2).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ArgErr(pub String);

impl std::fmt::Display for ArgErr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

fn num<T: std::str::FromStr>(flag: &str, v: Option<String>) -> Result<T, ArgErr> {
    let v = v.ok_or_else(|| ArgErr(format!("{flag} needs a value")))?;
    v.parse()
        .map_err(|_| ArgErr(format!("{flag}: not a number: {v}")))
}

fn val(flag: &str, v: Option<String>) -> Result<String, ArgErr> {
    v.ok_or_else(|| ArgErr(format!("{flag} needs a value")))
}

/// Parses `args` (without the program name); `env` reads environment variables
/// (`HFT_SLICE_CONFIG`).
pub fn parse(args: &[String], env: &dyn Fn(&str) -> Option<String>) -> Result<Args, ArgErr> {
    let mut a = Args {
        mode: Mode::Run,
        config: PathBuf::new(),
        minutes: None,
        ts_expect_hw: false,
        print_conn_stats: false,
        ballast: None,
        no_orders: false,
        out: None,
    };
    let mut config: Option<PathBuf> = None;
    let mut mode: Option<Mode> = None;
    let mut seconds: u32 = 5;
    let mut seed: u64 = 1;
    let mut print_napi = false;
    let mut it = args.iter().cloned();
    let set_mode = |m: Mode, mode: &mut Option<Mode>| -> Result<(), ArgErr> {
        if mode.is_some() {
            return Err(ArgErr("only one mode flag may be given".into()));
        }
        *mode = Some(m);
        Ok(())
    };
    while let Some(f) = it.next() {
        match f.as_str() {
            "--config" => config = Some(PathBuf::from(val(&f, it.next())?)),
            "--minutes" => a.minutes = Some(num(&f, it.next())?),
            "--ts-expect-hw" => a.ts_expect_hw = true,
            "--print-conn-stats" => a.print_conn_stats = true,
            "--ballast" => a.ballast = Some(num(&f, it.next())?),
            "--no-orders" => a.no_orders = true,
            "--out" => a.out = Some(PathBuf::from(val(&f, it.next())?)),
            "--seconds" => seconds = num(&f, it.next())?,
            "--seed" => seed = num(&f, it.next())?,
            "--print-napi" => print_napi = true,
            "--dry-run" => set_mode(Mode::DryRun { seconds: 0, seed: 0 }, &mut mode)?,
            "--report" => set_mode(
                Mode::Report {
                    root: PathBuf::from(val(&f, it.next())?),
                },
                &mut mode,
            )?,
            "--check-config" => set_mode(Mode::CheckConfig, &mut mode)?,
            "--build-only" => set_mode(
                Mode::BuildOnly {
                    n: num(&f, it.next())?,
                    print_napi: false,
                },
                &mut mode,
            )?,
            "--idle-polls" => set_mode(
                Mode::IdlePolls {
                    n: num(&f, it.next())?,
                },
                &mut mode,
            )?,
            "--two-packet-stamp-probe" => set_mode(
                Mode::TwoPacket {
                    n: num(&f, it.next())?,
                },
                &mut mode,
            )?,
            "--loopback-stream-minutes" => set_mode(
                Mode::LoopbackStream {
                    minutes: num(&f, it.next())?,
                },
                &mut mode,
            )?,
            other => return Err(ArgErr(format!("unknown argument {other}"))),
        }
    }
    a.mode = match mode.unwrap_or(Mode::Run) {
        Mode::DryRun { .. } => Mode::DryRun { seconds, seed },
        Mode::BuildOnly { n, .. } => Mode::BuildOnly { n, print_napi },
        m => m,
    };
    a.config = config
        .or_else(|| env("HFT_SLICE_CONFIG").map(PathBuf::from))
        .unwrap_or_else(|| PathBuf::from("latency-slice.toml"));
    Ok(a)
}
