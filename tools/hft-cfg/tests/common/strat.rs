//! Strategy-test environment: the schema directory, a refdata blob built from `refdata-basic`
//! (optionally edited) and the TOOL-119 strategy TOML.
#![allow(dead_code)]

use std::path::{Path, PathBuf};

use super::schemas::schema_dir;
use super::*;

pub struct Env {
    pub dir: PathBuf,
    pub schemas: PathBuf,
    pub refdata: PathBuf,
}

/// The TOOL-119 strategy TOML text.
pub fn base_toml() -> String {
    std::fs::read_to_string(fixtures().join("strategy/pc-main.pc-live.toml")).unwrap()
}

/// One extra group block (appended to a strategy TOML).
pub fn group(n: u32, asset: &str, legs: [&str; 2]) -> String {
    format!(
        "\n[[groups]]\nn = {n}\nasset = \"{asset}\"\nopen_allowed = true\nlegs = [ {{ inst = \"{}\" }}, {{ inst = \"{}\" }} ]\n",
        legs[0], legs[1]
    )
}

pub const AIN_LEGS: [&str; 2] = ["binance-usdm:AINUSDT", "bitget-uta-spot:AINUSDT"];
pub const BTC_LEGS: [&str; 2] = ["binance-usdm:BTCUSDT", "bitget-uta-spot:BTCUSDT"];

/// Copies `refdata-basic` to `dir/<name>/`, applies `(file, from, to)` edits (each `from` must
/// occur), builds it with the schema directory and returns the blob path.
pub fn refdata_blob(dir: &Path, schemas: &Path, name: &str, edits: &[(&str, &str, &str)]) -> PathBuf {
    let src = fixtures().join("refdata-basic");
    let d = dir.join(name);
    std::fs::create_dir_all(&d).unwrap();
    for e in std::fs::read_dir(&src).unwrap() {
        let e = e.unwrap();
        std::fs::copy(e.path(), d.join(e.file_name())).unwrap();
    }
    for (file, from, to) in edits {
        let p = d.join(file);
        let t = std::fs::read_to_string(&p).unwrap();
        assert!(t.contains(from), "{file}: `{from}` not found");
        std::fs::write(&p, t.replacen(from, to, 1)).unwrap();
    }
    let out = dir.join(format!("{name}.blob"));
    let r = run(&[
        "build",
        "--kind",
        "refdata",
        "--in",
        p(&d),
        "--out",
        p(&out),
        "--schemas",
        p(schemas),
    ]);
    assert_eq!(r.code, 0, "refdata {name}: {}", r.stderr);
    out
}

impl Env {
    pub fn new(name: &str) -> Env {
        Env::with_refdata(name, &[])
    }

    pub fn with_refdata(name: &str, edits: &[(&str, &str, &str)]) -> Env {
        let dir = scratch(name);
        let schemas = schema_dir(&dir);
        let refdata = refdata_blob(&dir, &schemas, "rd", edits);
        Env {
            dir,
            schemas,
            refdata,
        }
    }

    /// `build --kind strategy` of `toml`; returns the result and the output path.
    pub fn build(&self, tag: &str, toml: &str, against: Option<&Path>) -> (Out, PathBuf) {
        let input = self.dir.join(format!("{tag}.toml"));
        std::fs::write(&input, toml).unwrap();
        let out = self.dir.join(format!("{tag}.blob"));
        let _ = std::fs::remove_file(&out);
        let mut args = vec![
            "build".to_string(),
            "--kind".into(),
            "strategy".into(),
            "--schemas".into(),
            p(&self.schemas).into(),
            "--refdata".into(),
            p(&self.refdata).into(),
            "--in".into(),
            p(&input).into(),
            "--out".into(),
            p(&out).into(),
        ];
        if let Some(a) = against {
            args.push("--against".into());
            args.push(p(a).into());
        }
        let refs: Vec<&str> = args.iter().map(String::as_str).collect();
        (run(&refs), out)
    }

    /// Builds and asserts success; returns the blob bytes and path.
    pub fn ok(&self, tag: &str, toml: &str, against: Option<&Path>) -> (Vec<u8>, PathBuf) {
        let (r, out) = self.build(tag, toml, against);
        assert_eq!(r.code, 0, "{tag}: {}", r.stderr);
        (std::fs::read(&out).unwrap(), out)
    }
}

/// `StrategyParamsHdr` fields read from blob bytes.
pub fn hdr_u64(b: &[u8], off: usize) -> u64 {
    u64::from_le_bytes(b[off..off + 8].try_into().unwrap())
}
pub fn hdr_u32(b: &[u8], off: usize) -> u32 {
    u32::from_le_bytes(b[off..off + 4].try_into().unwrap())
}
pub fn hdr_u16(b: &[u8], off: usize) -> u16 {
    u16::from_le_bytes(b[off..off + 2].try_into().unwrap())
}
/// Group ids of the blob's `GroupSpec` table.
pub fn group_ids(b: &[u8]) -> Vec<u32> {
    let n = usize::from(hdr_u16(b, 12));
    (0..n).map(|i| hdr_u32(b, 48 + 80 * i)).collect()
}
pub fn hwm(b: &[u8]) -> u32 {
    hdr_u32(b, 36)
}

/// TOML with `version = v` and the given extra groups appended.
pub fn toml_with(version: u64, extra: &[String]) -> String {
    let mut t = base_toml().replace("version = 1\n", &format!("version = {version}\n"));
    for g in extra {
        t.push_str(g);
    }
    t
}
