//! The command line of `hft-pmcfg` (and of the hft-core example `pm_sim_config`, which calls
//! [`main`]); README "命令行".
use std::path::PathBuf;
use std::process::ExitCode;

use crate::{AccountRole, LiveAccount, MAX_EXTRA_ACCOUNTS, PmLive};

/// The usage line of program `prog`.
pub fn usage(prog: &str) -> String {
    format!(
        "usage: {prog} [--maker | --live UID KEYS_DIR [--day-loss-carry YYYY-MM-DD USDC] [--daily-loss USDC] \
         [--daily-loss-resumes N] [--no-presign] [--live-account UID KEYS_DIR ...] \
         [--account LABEL:UID:ROLE KEYS_DIR ...]] [--param NAME=RAW ...] \
         OUTPUT_DIR TMPFS_DIR SRC_IP BIND_DEVICE [USER] [PIPELINE_USER]"
    )
}

/// What to generate.
#[derive(Clone, Debug)]
pub enum Mode {
    /// No mode flag: the book-observer sim (`write_polymarket_sim`).
    Sim,
    /// `--maker`: the binary-maker sim.
    Maker,
    /// `--live UID KEYS_DIR`: the live maker.
    Live(PmLive),
}

/// One parsed command line.
#[derive(Clone, Debug)]
pub struct Cmd {
    pub mode: Mode,
    /// `--param NAME=RAW`, in order (checked against the binary-maker schema).
    pub params: Vec<(String, i64)>,
    pub root: PathBuf,
    pub hot: PathBuf,
    pub src_ip: String,
    pub bind_device: String,
    pub user: Option<String>,
    pub pipeline: Option<String>,
}

/// `YYYY-MM-DD` → days since 1970-01-01 (proleptic Gregorian).
pub fn utc_day(s: &str) -> Option<u32> {
    let mut it = s.split('-');
    let (y, m, d): (i64, i64, i64) = (
        it.next()?.parse().ok()?,
        it.next()?.parse().ok()?,
        it.next()?.parse().ok()?,
    );
    if it.next().is_some() || !(1..=12).contains(&m) || !(1..=31).contains(&d) {
        return None;
    }
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    u32::try_from(era * 146_097 + doe - 719_468).ok()
}

/// A USDC decimal (at most 6 places) → raw 1e-6.
pub fn usdc(s: &str) -> Option<i64> {
    let (neg, b) = s.strip_prefix('-').map_or((false, s), |r| (true, r));
    let (i, f) = b.split_once('.').unwrap_or((b, ""));
    if f.len() > 6 || i.is_empty() || !f.bytes().all(|c| c.is_ascii_digit()) {
        return None;
    }
    let raw = i
        .parse::<i64>()
        .ok()?
        .checked_mul(1_000_000)?
        .checked_add(format!("{f:0<6}").parse::<i64>().ok()?)?;
    Some(if neg { -raw } else { raw })
}

/// `LABEL:UID:ROLE` of `--account` (label before the first `:`, role after the last).
pub fn parse_account(spec: &str, keys_dir: PathBuf) -> Result<LiveAccount, String> {
    let bad = || format!("--account {spec}: expected LABEL:UID:ROLE");
    let (label, rest) = spec.split_once(':').ok_or_else(bad)?;
    let (uid, role) = rest.rsplit_once(':').ok_or_else(bad)?;
    if label.is_empty() || uid.is_empty() {
        return Err(bad());
    }
    let role = AccountRole::parse(role).map_err(|e| format!("--account {spec}: {e}"))?;
    Ok(LiveAccount {
        label: Some(label.to_string()),
        uid: uid.to_string(),
        keys_dir,
        role,
    })
}

/// Parses the arguments after the program name.
pub fn parse(args: &[String]) -> Result<Cmd, String> {
    let mut maker = false;
    let mut live: Option<(String, PathBuf)> = None;
    let mut carry = None;
    let mut daily_loss = None;
    let mut resumes = None;
    let mut no_presign = false;
    let mut extra = Vec::new();
    let mut params = Vec::new();
    let mut pos: Vec<String> = Vec::new();
    let mut it = args.iter();
    let mut next = |flag: &str, what: &str| -> Result<String, String> {
        it.next()
            .cloned()
            .ok_or_else(|| format!("{flag}: missing {what}"))
    };
    let once = |set: bool, flag: &str| -> Result<(), String> {
        if set {
            Err(format!("{flag} given twice"))
        } else {
            Ok(())
        }
    };
    while let Ok(a) = next("", "") {
        match a.as_str() {
            "--maker" => {
                once(maker, "--maker")?;
                maker = true;
            }
            "--live" => {
                once(live.is_some(), "--live")?;
                let uid = next("--live", "UID")?;
                let dir = next("--live", "KEYS_DIR")?;
                live = Some((uid, PathBuf::from(dir)));
            }
            "--live-account" => {
                let uid = next("--live-account", "UID")?;
                let dir = next("--live-account", "KEYS_DIR")?;
                extra.push(LiveAccount::shorthand(uid, dir));
            }
            "--account" => {
                let spec = next("--account", "LABEL:UID:ROLE")?;
                let dir = next("--account", "KEYS_DIR")?;
                extra.push(parse_account(&spec, PathBuf::from(dir))?);
            }
            "--day-loss-carry" => {
                once(carry.is_some(), "--day-loss-carry")?;
                let day = next("--day-loss-carry", "YYYY-MM-DD")?;
                let pnl = next("--day-loss-carry", "USDC")?;
                let day = utc_day(&day).ok_or_else(|| format!("--day-loss-carry: bad date {day}"))?;
                let pnl = usdc(&pnl)
                    .filter(|p| *p < 0)
                    .ok_or("--day-loss-carry: USDC must be < 0")?;
                carry = Some((day, pnl));
            }
            "--daily-loss" => {
                once(daily_loss.is_some(), "--daily-loss")?;
                let v = next("--daily-loss", "USDC")?;
                daily_loss = Some(
                    usdc(&v)
                        .filter(|p| *p > 0)
                        .ok_or("--daily-loss: USDC must be a decimal > 0 (at most 6 places)")?,
                );
            }
            "--daily-loss-resumes" => {
                once(resumes.is_some(), "--daily-loss-resumes")?;
                let v = next("--daily-loss-resumes", "N")?;
                resumes = Some(
                    v.parse::<u8>()
                        .ok()
                        .filter(|n| *n <= 20)
                        .ok_or("--daily-loss-resumes: N must be 0..=20")?,
                );
            }
            "--no-presign" => {
                once(no_presign, "--no-presign")?;
                no_presign = true;
            }
            "--param" => {
                let p = next("--param", "NAME=RAW")?;
                params.push(crate::parse_param(&p)?);
            }
            s if s.starts_with("--") => return Err(format!("unknown option {s}")),
            _ => pos.push(a),
        }
    }
    crate::check_maker_params(&params)?;
    let needs_live = [
        (no_presign, "--no-presign"),
        (!extra.is_empty(), "--live-account/--account"),
        (carry.is_some(), "--day-loss-carry"),
        (daily_loss.is_some(), "--daily-loss"),
        (resumes.is_some(), "--daily-loss-resumes"),
    ];
    if live.is_none()
        && let Some((_, flag)) = needs_live.iter().find(|(set, _)| *set)
    {
        return Err(format!("{flag} needs --live"));
    }
    if maker && live.is_some() {
        return Err("--maker and --live are exclusive".into());
    }
    if !params.is_empty() && !maker && live.is_none() {
        return Err("--param needs --maker or --live".into());
    }
    if extra.len() > MAX_EXTRA_ACCOUNTS {
        return Err(format!(
            "--live-account/--account: at most {} accounts per venue",
            hft_types::KillBits::MAX_ACCOUNTS_PER_GW
        ));
    }
    if !(4..=6).contains(&pos.len()) {
        return Err(format!(
            "expected OUTPUT_DIR TMPFS_DIR SRC_IP BIND_DEVICE [USER] [PIPELINE_USER], got {} arguments",
            pos.len()
        ));
    }
    let mode = match live {
        Some((uid, keys_dir)) => {
            let l = PmLive {
                uid,
                keys_dir,
                day_loss_carry: carry,
                daily_loss_abs: daily_loss,
                daily_loss_max_resumes: resumes,
                presign: !no_presign,
                extra_accounts: extra,
            };
            l.check()?;
            Mode::Live(l)
        }
        None if maker => Mode::Maker,
        None => Mode::Sim,
    };
    let mut pos = pos.into_iter();
    let mut take = || pos.next().unwrap_or_default();
    Ok(Cmd {
        mode,
        params,
        root: PathBuf::from(take()),
        hot: PathBuf::from(take()),
        src_ip: take(),
        bind_device: take(),
        user: pos.next(),
        pipeline: pos.next(),
    })
}

/// Writes the configuration of `cmd`; returns the path of `core.toml`.
pub fn run(cmd: &Cmd) -> Result<PathBuf, String> {
    let (root, hot) = (cmd.root.as_path(), cmd.hot.as_path());
    let (user, pipeline) = (cmd.user.as_deref(), cmd.pipeline.as_deref());
    let (ip, dev) = (cmd.src_ip.as_str(), cmd.bind_device.as_str());
    match &cmd.mode {
        Mode::Live(l) => {
            crate::write_polymarket_maker_live_with(root, hot, user, pipeline, ip, dev, l, &cmd.params)
        }
        Mode::Maker => {
            crate::write_polymarket_maker_sim_with(root, hot, user, pipeline, ip, dev, &cmd.params)
        }
        Mode::Sim => crate::write_polymarket_sim(root, hot, user, pipeline, ip, dev),
    }
}

/// The program: parse the process arguments, write, print the `core.toml` path. A bad command line
/// or a failed write prints the message and the usage line of `prog` to stderr and exits 2;
/// `-h`/`--help` prints the usage line and exits 0.
pub fn main(prog: &str) -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a == "-h" || a == "--help") {
        println!("{}", usage(prog));
        return ExitCode::SUCCESS;
    }
    match parse(&args).and_then(|c| run(&c)) {
        Ok(path) => {
            println!("{}", path.display());
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("{prog}: {e}\n{}", usage(prog));
            ExitCode::from(2)
        }
    }
}
