use hft_core::{Error, Strategies};
use hft_strategy_sdk::StrategySet;
use std::path::PathBuf;

fn main() -> std::process::ExitCode {
    match run() {
        Ok(code) => std::process::ExitCode::from(code),
        Err(errors) => {
            for e in &errors {
                eprintln!("{e}");
            }
            if errors.iter().any(|e| e.exit == 134) {
                std::process::abort();
            }
            std::process::ExitCode::from(errors.first().map_or(2, |e| e.exit))
        }
    }
}
fn run() -> Result<u8, Vec<Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let usage = || {
        vec![Error::config(
            "Usage",
            "hft-core --config FILE [--check] [--dry-run] [--run-for-ms N]; schemas --out DIR; --version",
        )]
    };
    if args == ["--version"] {
        println!("hft-core {} schema=1", env!("CARGO_PKG_VERSION"));
        for d in Strategies::kinds() {
            println!("kind_id={} {} version={}", d.kind_id, d.name, d.version);
        }
        return Ok(0);
    }
    if args.first().is_some_and(|a| a == "schemas") {
        if args.len() != 3 || args[1] != "--out" {
            return Err(usage());
        }
        std::fs::create_dir_all(&args[2]).map_err(|_| usage())?;
        for (name, body) in hft_strategy_registry::export_schemas() {
            std::fs::write(PathBuf::from(&args[2]).join(name), body).map_err(|_| usage())?;
        }
        return Ok(0);
    }
    let (mut path, mut check, mut dry, mut duration) = (None, false, false, None);
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--config" if path.is_none() => path = Some(PathBuf::from(it.next().ok_or_else(usage)?)),
            "--check" | "check-config" if !check => check = true,
            "--dry-run" if !dry => dry = true,
            "--run-for-ms" if duration.is_none() => {
                let n = it.next().ok_or_else(usage)?.parse::<u64>().map_err(|_| usage())?;
                if n == 0 {
                    return Err(usage());
                }
                duration = Some(n);
            }
            _ => return Err(usage()),
        }
    }
    let path = path.ok_or_else(usage)?;
    let mut startup = hft_core::startup::Serial::default();
    if !check {
        startup.enable_status().map_err(|e| vec![e])?;
    }
    let secrets = if check || dry {
        hft_core::secrets::SecretFiles::default()
    } else {
        let parsed = hft_core::config::parse(&path)?;
        if parsed.instance.name == "prod" {
            hft_core::secrets::SecretFiles::read_and_unlink(&parsed)?
        } else {
            hft_core::secrets::SecretFiles::default()
        }
    };
    let loaded = hft_core::config::load_timed(&path, dry, startup)?;
    if check {
        println!("hft-core: configuration valid");
        return Ok(0);
    }
    hft_core::runtime::run(loaded, duration, secrets).map_err(|e| vec![e])
}
