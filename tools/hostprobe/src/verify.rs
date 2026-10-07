//! Expected profiles (`deploy/tuning/<manifest>.toml`, `benchmarks/reference.toml`) and the
//! comparison of `hostprobe verify`. Format (tools/README.md "hostprobe"):
//!
//! ```toml
//! grub = "../grub/50-hft.cfg"   # optional; the release's GRUB snippet, relative to this file
//! [roles]                        # optional; NIC role -> IMDS device index (default a=0 b=1 c=2 b2=3)
//! eni_b = 1
//! [occupancy]                    # optional
//! irq_cpus = "1"                 # IRQ-dedicated cores (default: where the ENI-B IRQs land)
//! keepawake = ["hft-keepawake-1"]
//! [[expect]]                     # one comparison; exactly one operator key
//! key = "cpu.isolated"           # dotted HostProfile key; `[*]` expands an array
//! eq = "2-15"                    # eq | lt | le (+ optional add) | contains_cpus | disjoint_cpus | empty | present
//! level = "required"             # required | advisory
//! ```
//!
//! Other top-level keys are ignored (the reference profile also records `host_class`).

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use serde_json::{Value, json};

use crate::collect::{Cfg, UNREADABLE};
use crate::cpus;
use crate::json::lookup;

/// Comparison of one expected item.
#[derive(Debug, Clone, PartialEq)]
pub enum Op {
    Eq(Value),
    Lt(f64),
    /// `actual + add <= max`.
    Le {
        max: f64,
        add: f64,
    },
    ContainsCpus(BTreeSet<u32>),
    DisjointCpus(BTreeSet<u32>),
    Empty,
    Present,
}

/// One `[[expect]]` entry.
#[derive(Debug, Clone, PartialEq)]
pub struct Expect {
    pub key: String,
    pub op: Op,
    pub required: bool,
}

/// A parsed expected profile.
#[derive(Debug, Clone)]
pub struct Expected {
    pub cfg: Cfg,
    pub items: Vec<Expect>,
}

/// One item that did not match.
#[derive(Debug, Clone, PartialEq)]
pub struct Mismatch {
    pub key: String,
    pub expected: Value,
    pub actual: Value,
    pub required: bool,
}

fn to_json(v: &toml::Value) -> Value {
    match v {
        toml::Value::String(s) => json!(s),
        toml::Value::Integer(i) => json!(i),
        toml::Value::Float(f) => json!(f),
        toml::Value::Boolean(b) => json!(b),
        toml::Value::Array(a) => Value::Array(a.iter().map(to_json).collect()),
        toml::Value::Table(t) => Value::Object(t.iter().map(|(k, v)| (k.clone(), to_json(v))).collect()),
        toml::Value::Datetime(d) => json!(d.to_string()),
    }
}

fn num(v: &toml::Value) -> Option<f64> {
    v.as_float().or_else(|| v.as_integer().map(|i| i as f64))
}

/// Reads and parses an expected profile file; `Err` = input error (exit 2).
pub fn load(path: &Path) -> Result<Expected, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let dir = path.parent().unwrap_or(Path::new(".")).to_path_buf();
    parse(&text, |rel| {
        let p = dir.join(rel);
        std::fs::read_to_string(&p).map_err(|e| format!("grub {}: {e}", p.display()))
    })
    .map_err(|e| format!("{}: {e}", path.display()))
}

/// Parses expected-profile text; `grub` reads the snippet named by the `grub` key (relative path).
pub fn parse(text: &str, grub: impl FnOnce(&str) -> Result<String, String>) -> Result<Expected, String> {
    let t: toml::Table = text.parse().map_err(|e| format!("{e}"))?;
    let mut cfg = Cfg::default();
    if let Some(r) = t.get("roles").and_then(|v| v.as_table()) {
        cfg.roles = BTreeMap::new();
        for (k, v) in r {
            let i = v
                .as_integer()
                .filter(|i| *i >= 0)
                .ok_or_else(|| format!("roles.{k}: device index must be a non-negative integer"))?;
            cfg.roles.insert(k.clone(), i as u32);
        }
    }
    if let Some(o) = t.get("occupancy").and_then(|v| v.as_table()) {
        if let Some(c) = o.get("irq_cpus") {
            let s = c.as_str().ok_or("occupancy.irq_cpus must be a CPU list string")?;
            cfg.irq_cpus = Some(cpus::parse_list(s).ok_or(format!("occupancy.irq_cpus: bad CPU list {s}"))?);
        }
        if let Some(k) = o.get("keepawake") {
            cfg.keepawake = k
                .as_array()
                .ok_or("occupancy.keepawake must be an array")?
                .iter()
                .map(|x| {
                    x.as_str()
                        .map(str::to_string)
                        .ok_or("occupancy.keepawake: names are strings")
                })
                .collect::<Result<_, _>>()?;
        }
    }
    if let Some(g) = t.get("grub") {
        let rel = g.as_str().ok_or("grub must be a path string")?;
        cfg.grub = Some(grub(rel)?);
    }
    let mut items = Vec::new();
    for (i, e) in t
        .get("expect")
        .and_then(|v| v.as_array())
        .into_iter()
        .flatten()
        .enumerate()
    {
        let e = e.as_table().ok_or(format!("expect[{i}] must be a table"))?;
        let key = e
            .get("key")
            .and_then(|v| v.as_str())
            .ok_or(format!("expect[{i}]: missing key"))?
            .to_string();
        let required = match e.get("level").and_then(|v| v.as_str()) {
            Some("required") => true,
            Some("advisory") => false,
            other => {
                return Err(format!(
                    "{key}: level must be required or advisory, got {other:?}"
                ));
            }
        };
        let cpuset = |v: &toml::Value| -> Result<BTreeSet<u32>, String> {
            v.as_str()
                .and_then(cpus::parse_list)
                .ok_or(format!("{key}: CPU list string expected"))
        };
        let mut ops = Vec::new();
        if let Some(v) = e.get("eq") {
            ops.push(Op::Eq(to_json(v)));
        }
        if let Some(v) = e.get("lt") {
            ops.push(Op::Lt(num(v).ok_or(format!("{key}: lt needs a number"))?));
        }
        if let Some(v) = e.get("le") {
            let add = match e.get("add") {
                Some(a) => num(a).ok_or(format!("{key}: add needs a number"))?,
                None => 0.0,
            };
            ops.push(Op::Le {
                max: num(v).ok_or(format!("{key}: le needs a number"))?,
                add,
            });
        }
        if let Some(v) = e.get("contains_cpus") {
            ops.push(Op::ContainsCpus(cpuset(v)?));
        }
        if let Some(v) = e.get("disjoint_cpus") {
            ops.push(Op::DisjointCpus(cpuset(v)?));
        }
        if e.get("empty").and_then(|v| v.as_bool()) == Some(true) {
            ops.push(Op::Empty);
        }
        if e.get("present").and_then(|v| v.as_bool()) == Some(true) {
            ops.push(Op::Present);
        }
        if ops.len() != 1 {
            return Err(format!(
                "{key}: exactly one operator expected, found {}",
                ops.len()
            ));
        }
        items.push(Expect {
            key,
            op: ops.remove(0),
            required,
        });
    }
    Ok(Expected { cfg, items })
}

fn fnum(v: &Value) -> Option<f64> {
    v.as_f64()
}

fn expected_value(op: &Op) -> Value {
    match op {
        Op::Eq(v) => v.clone(),
        Op::Lt(x) => json!(format!("<{}", show_num(*x))),
        Op::Le { max, .. } => json!(format!("<={}", show_num(*max))),
        Op::ContainsCpus(s) => json!(format!("contains:{}", cpus::format_list(s))),
        Op::DisjointCpus(s) => json!(format!("disjoint:{}", cpus::format_list(s))),
        Op::Empty => json!([]),
        Op::Present => json!("present"),
    }
}

fn show_num(x: f64) -> String {
    if x.fract() == 0.0 && x.abs() < 1e15 {
        format!("{}", x as i64)
    } else {
        format!("{x}")
    }
}

fn whole(x: f64) -> Value {
    if x.fract() == 0.0 && x.abs() < 1e15 {
        json!(x as i64)
    } else {
        json!(x)
    }
}

fn num_eq(a: &Value, b: &Value) -> bool {
    match (fnum(a), fnum(b)) {
        (Some(x), Some(y)) => x == y,
        _ => a == b,
    }
}

/// Compares `profile` with the expected items (without `verify`).
pub fn compare(profile: &Value, items: &[Expect]) -> Vec<Mismatch> {
    let mut out = Vec::new();
    for it in items {
        for f in lookup(profile, &it.key) {
            let exp = expected_value(&it.op);
            let actual = match &f.value {
                None => json!("absent"),
                Some(v) => v.clone(),
            };
            let ok = match (&f.value, &it.op) {
                (None, _) => false,
                (Some(Value::String(s)), _) if s == UNREADABLE => false,
                (Some(v), Op::Eq(e)) => num_eq(v, e),
                (Some(v), Op::Lt(x)) => fnum(v).is_some_and(|a| a < *x),
                (Some(v), Op::Le { max, add }) => fnum(v).is_some_and(|a| a + add <= *max),
                (Some(v), Op::ContainsCpus(want)) => v
                    .as_str()
                    .and_then(cpus::parse_list)
                    .is_some_and(|have| want.is_subset(&have)),
                (Some(v), Op::DisjointCpus(no)) => v
                    .as_str()
                    .and_then(cpus::parse_list)
                    .is_some_and(|have| have.is_disjoint(no)),
                (Some(v), Op::Empty) => v.is_null() || v.as_array().is_some_and(|a| a.is_empty()),
                (Some(v), Op::Present) => !v.is_null(),
            };
            if !ok {
                let actual = match (&it.op, fnum(&actual)) {
                    (Op::Le { add, .. }, Some(a)) => whole(a + add),
                    _ => actual,
                };
                out.push(Mismatch {
                    key: f.path,
                    expected: exp,
                    actual,
                    required: it.required,
                });
            }
        }
    }
    out
}

/// Text of a value in a stderr line (strings unquoted).
pub fn show(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        other => crate::json::compact(other),
    }
}
