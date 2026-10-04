//! `hft-cfg`: compiles version-controlled TOML into refdata (RDB1), `EngineConfig` (ECF1) and
//! `StrategyParams` (SPB1) blobs, the last one generically from the exported strategy schema
//! JSON (no strategy crate is linked); validates accounts, Sim accounts, capability rows and
//! bindings; continues GroupIds and `group_ctr_hwm`; prints diff reports.
//! Specification: tools/README.md "hft-cfg". Exit codes: 0 ok, 1 rejected, 2 input/environment.
#![forbid(unsafe_code)]
#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::float_arithmetic
)]

pub mod diff;
pub mod engine;
pub mod refdata;
pub mod strategy;
pub mod util;

use std::collections::BTreeMap;
use std::io::Write;
use std::path::{Path, PathBuf};

use hft_event::parse_spb1;
use hft_refdata::{EngineConfig, LoadErrCode, RefData};

use crate::diff::{BlobKind, Report, kind_of};
use crate::util::*;

/// Parsed command line: positionals, `--key value` options and the `--json` flag.
#[derive(Debug, Default)]
struct Args {
    pos: Vec<String>,
    opts: BTreeMap<String, String>,
    json: bool,
}

fn parse_args(a: &[String]) -> Res<Args> {
    let mut out = Args::default();
    let mut it = a.iter();
    while let Some(x) = it.next() {
        if x == "--json" {
            out.json = true;
        } else if let Some(k) = x.strip_prefix("--") {
            let v = it
                .next()
                .map_or_else(|| input(format!("--{k}: missing value")), Ok)?;
            out.opts.insert(k.to_string(), v.clone());
        } else {
            out.pos.push(x.clone());
        }
    }
    Ok(out)
}

impl Args {
    fn get(&self, k: &str) -> Option<&Path> {
        self.opts.get(k).map(Path::new)
    }
    fn need(&self, k: &str) -> Res<&Path> {
        self.get(k).map_or_else(|| input(format!("missing --{k}")), Ok)
    }
    fn allow(&self, keys: &[&str]) -> Res<()> {
        for k in self.opts.keys() {
            if !keys.contains(&k.as_str()) {
                return input(format!("unknown option --{k}"));
            }
        }
        Ok(())
    }
}

const USAGE: &str = "usage:
  hft-cfg build --kind refdata  --in <dir|file> --out <blob> [--against <blob>] [--sim-models <core.toml>] [--schemas <dir>] [--admins <file>] [--json]
  hft-cfg build --kind strategy --schemas <dir> --refdata <blob> --in <toml> --out <blob> [--against <blob>] [--json]
  hft-cfg build --kind engine   --in <toml> --core-config <core.toml> --out <blob> [--against <blob>] [--json]
  hft-cfg diff  --old <blob> --new <blob> [--schemas <dir>] [--refdata <blob>] [--json]
  hft-cfg show  <blob> [--schemas <dir>] [--refdata <blob>]
  hft-cfg send  --kind strategy|risk|refdata --blob <blob> --account <name> [--socket <path>]";

/// A blob path that must be a file; a directory (journal) is not supported yet → exit 2.
fn blob_file(p: &Path, what: &str) -> Res<Vec<u8>> {
    if p.is_dir() {
        return input(format!(
            "{what} {}: reading the blob in effect from a journal directory needs hft-journal (not linked yet); pass the blob file",
            p.display()
        ));
    }
    read_bytes(p)
}

fn write_out(p: &Path, b: &[u8]) -> Res<()> {
    std::fs::write(p, b).or_else(|e| input(format!("{}: {e}", p.display())))
}

fn emit(r: &Report, json: bool, out: &mut dyn Write) {
    let s = if json {
        serde_json::to_string_pretty(&r.json).unwrap_or_default() + "\n"
    } else {
        r.text.clone()
    };
    let _ = out.write_all(s.as_bytes());
}

fn build_refdata(a: &Args, out: &mut dyn Write) -> Res<()> {
    a.allow(&["kind", "in", "out", "against", "sim-models", "schemas", "admins"])?;
    let input_path = a.need("in")?;
    let out_path = a.need("out")?;
    if !input_path.exists() {
        return input(format!("{}: not found", input_path.display()));
    }
    let root = load_toml_input(input_path)?;
    let cur = match a.get("against") {
        Some(p) => Some(diff::load_refdata(&blob_file(p, "--against")?, "--against")?),
        None => None,
    };
    let opts = refdata::RefdataOpts {
        schemas: a.get("schemas"),
        sim_models: a.get("sim-models"),
        admins: a.get("admins"),
        against: cur.as_ref(),
    };
    let r = refdata::build(&root, &opts)?;
    write_out(out_path, &r.blob)?;
    let new = diff::load_refdata(&r.blob, "output")?;
    let mut rep = match &cur {
        Some(c) => diff::refdata_report(c, &new),
        None => Report {
            text: String::new(),
            json: serde_json::json!({"kind": "refdata", "warnings": []}),
        },
    };
    if cur.is_none() {
        let ws = refdata::warnings(&new);
        for w in &ws {
            rep.text.push_str(&format!(
                "warning: {} accounts {} uid {}\n",
                w.code,
                w.accounts.join(", "),
                w.uid
            ));
        }
        rep.json["warnings"] = ws
            .iter()
            .map(|w| serde_json::json!({"code": w.code, "accounts": w.accounts, "uid": w.uid}))
            .collect();
    }
    if r.already_active {
        rep.text.push_str(&format!(
            "{}: content equals --against\n",
            LoadErrCode::AlreadyActive.name()
        ));
        rep.json["already_active"] = true.into();
    }
    for (label, who) in &r.signoffs {
        rep.text.push_str(&format!(
            "signoff: account {label} uid_policy = \"unverifiable\" signed off by {who}\n"
        ));
    }
    rep.json["signoffs"] = r
        .signoffs
        .iter()
        .map(|(l, w)| serde_json::json!({"account": l, "signoff": w}))
        .collect();
    emit(&rep, a.json, out);
    Ok(())
}

fn build_engine(a: &Args, out: &mut dyn Write) -> Res<()> {
    a.allow(&["kind", "in", "out", "against", "core-config"])?;
    let input_path = a.need("in")?;
    let core = a.need("core-config")?;
    let out_path = a.need("out")?;
    let t = parse_toml(&read_text(input_path)?, input_path)?;
    let blob = engine::build(&t, core)?;
    if let Some(p) = a.get("against") {
        let cur_b = blob_file(p, "--against")?;
        let cur = EngineConfig::load(&cur_b, None).or_else(|r| {
            rej(format!(
                "--against: not a valid ECF1 blob: {}",
                refdata::report_text(&r)
            ))
        })?;
        match EngineConfig::load(&blob, Some(&cur)) {
            Ok(_) => {}
            Err(r) if r.code() == LoadErrCode::AlreadyActive => {}
            Err(r) => {
                return rej(format!(
                    "EngineConfig::load against the blob in effect: {}",
                    refdata::report_text(&r)
                ));
            }
        }
        write_out(out_path, &blob)?;
        emit(&diff::engine_diff(&cur_b, &blob)?, a.json, out);
        return Ok(());
    }
    write_out(out_path, &blob)
}

fn build_strategy(a: &Args, out: &mut dyn Write) -> Res<()> {
    a.allow(&["kind", "in", "out", "against", "schemas", "refdata"])?;
    let input_path = a.need("in")?;
    let schemas = a.need("schemas")?;
    let rd_path = a.need("refdata")?;
    let out_path = a.need("out")?;
    let t = parse_toml(&read_text(input_path)?, input_path)?;
    let rd = diff::load_refdata(&blob_file(rd_path, "--refdata")?, "--refdata")?;
    let cur_b = a.get("against").map(|p| blob_file(p, "--against")).transpose()?;
    let al = cur_b.as_ref().map(|b| aligned(b));
    let cur_view = match (&cur_b, &al) {
        (Some(b), Some(al)) => Some(
            parse_spb1(aligned_bytes(al, b.len()))
                .or_else(|e| rej(format!("--against: not a valid SPB1 blob: {e:?}")))?,
        ),
        _ => None,
    };
    let against = cur_view.map(|view| strategy::Against { view });
    let r = strategy::build(&t, schemas, &rd, against.as_ref())?;
    write_out(out_path, &r.blob)?;
    if let Some(cb) = &cur_b {
        let ls = strategy::load_schema(schemas, r.hdr.kind_id)?;
        let old = strategy::decode(cb, Some(&ls))?;
        let new = strategy::decode(&r.blob, Some(&ls))?;
        emit(&diff::strategy_diff(&old, &new, &ls, Some(&rd)), a.json, out);
    }
    Ok(())
}

fn optional_refdata(a: &Args) -> Res<Option<RefData>> {
    a.get("refdata")
        .map(|p| diff::load_refdata(&blob_file(p, "--refdata")?, "--refdata"))
        .transpose()
}

fn cmd_diff(a: &Args, out: &mut dyn Write) -> Res<()> {
    a.allow(&["old", "new", "schemas", "refdata"])?;
    let old = blob_file(a.need("old")?, "--old")?;
    let new = blob_file(a.need("new")?, "--new")?;
    let (ko, kn) = (kind_of(&old), kind_of(&new));
    if ko != kn {
        return rej("--old and --new are different blob kinds");
    }
    let rep = match ko {
        BlobKind::Engine => diff::engine_diff(&old, &new)?,
        BlobKind::Refdata => diff::refdata_diff(&old, &new)?,
        BlobKind::Strategy => {
            let rd = optional_refdata(a)?;
            let schemas = a.need("schemas")?;
            let probe = strategy::decode(&old, None)?;
            let ls = strategy::load_schema(schemas, probe.hdr.kind_id)?;
            let o = strategy::decode(&old, Some(&ls))?;
            let n = strategy::decode(&new, Some(&ls))?;
            diff::strategy_diff(&o, &n, &ls, rd.as_ref())
        }
    };
    emit(&rep, a.json, out);
    Ok(())
}

fn cmd_show(a: &Args, out: &mut dyn Write) -> Res<()> {
    a.allow(&["schemas", "refdata"])?;
    let p = a
        .pos
        .get(1)
        .map_or_else(|| input("show: missing <blob>"), |x| Ok(PathBuf::from(x)))?;
    let b = blob_file(&p, "show")?;
    let text = match kind_of(&b) {
        BlobKind::Refdata => hft_refdata::dump(&diff::load_refdata(&b, "show")?),
        BlobKind::Engine => {
            let ec = EngineConfig::load(&b, None).or_else(|r| {
                rej(format!(
                    "show: not a valid ECF1 blob: {}",
                    refdata::report_text(&r)
                ))
            })?;
            engine::leaves(&ec)
                .into_iter()
                .map(|(k, v)| format!("{k} = {v}\n"))
                .collect()
        }
        BlobKind::Strategy => {
            let rd = optional_refdata(a)?;
            let probe = strategy::decode(&b, None)?;
            let ls = a
                .get("schemas")
                .map(|d| strategy::load_schema(d, probe.hdr.kind_id))
                .transpose()?;
            let d = strategy::decode(&b, ls.as_ref())?;
            strategy::render(&d, ls.as_ref(), rd.as_ref())
        }
    };
    let _ = out.write_all(text.as_bytes());
    Ok(())
}

fn cmd_send(a: &Args) -> Res<()> {
    a.allow(&["kind", "blob", "account", "socket"])?;
    match a.opts.get("kind").map(String::as_str) {
        Some("strategy" | "risk" | "refdata") => {}
        Some("engine") => {
            return input("send: engine blobs are not sent (EngineConfig changes need a restart)");
        }
        _ => return input("send: --kind strategy|risk|refdata"),
    }
    read_bytes(a.need("blob")?)?;
    a.need("account")?;
    let sock = a
        .get("socket")
        .map_or_else(|| PathBuf::from("/run/hft/ctl.sock"), Path::to_path_buf);
    if !sock.exists() {
        return input(format!("send: {}: ctl socket not reachable", sock.display()));
    }
    input("send: needs the hft-ctl-proto frames (not available yet)")
}

fn dispatch(args: &[String], out: &mut dyn Write) -> Res<()> {
    let a = parse_args(args)?;
    match (
        a.pos.first().map(String::as_str),
        a.opts.get("kind").map(String::as_str),
    ) {
        (Some("build"), Some("refdata")) => build_refdata(&a, out),
        (Some("build"), Some("engine")) => build_engine(&a, out),
        (Some("build"), Some("strategy")) => build_strategy(&a, out),
        (Some("build"), Some("risk")) => {
            input("build --kind risk: needs the hft-risk RiskParams builder (not available yet)")
        }
        (Some("refdata"), _) if a.pos.get(1).map(String::as_str) == Some("fetch") => {
            input("refdata fetch: not available yet")
        }
        (Some("diff"), _) => cmd_diff(&a, out),
        (Some("show"), _) => cmd_show(&a, out),
        (Some("send"), _) => cmd_send(&a),
        _ => input(USAGE),
    }
}

/// Runs one command (`args` without the program name); returns the exit code. Reports go to
/// `out`, errors to `err`.
pub fn run(args: &[String], out: &mut dyn Write, err: &mut dyn Write) -> i32 {
    match dispatch(args, out) {
        Ok(()) => 0,
        Err(e) => {
            let _ = writeln!(err, "hft-cfg: {e}");
            e.exit as i32
        }
    }
}
