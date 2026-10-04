//! "预算" and "venue README" checks: README budget tables vs `benches/budgets.toml`, crate budgets vs the
//! benchmarks 单 crate 基准上限表, venue required sections, venue budget rows vs the hft-venue-proto 统一门槛表
//! and the `ingest` allocation check.
//!
//! Budget cell parsing (tools/README.md "cargo xtask doc-check"): a comparison is `[pNN] ≤|< <number> <unit>`
//! (`ns`, `µs`/`us`, `ms`, `s`); comparisons right after `+` (relative) or with a per-unit suffix (`/档`,
//! `/条`) are not budgets. A table whose budget header says `p50 / p99` maps the first two comparisons of a
//! cell to p50 and p99. Otherwise a row with percentile comparisons uses only those; a row without them maps
//! its comparisons one-to-one to the row's benchmark names (counts differ → values not compared).

use std::collections::BTreeMap;
use std::sync::LazyLock;

use regex::Regex;

use super::md::{Doc, Globs, Table};
use super::native::{Ctx, Problem, tables_under};

static CMP: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(p\d+(?:\.\d+)?)?\s*[:\x{FF1A}]?\s*(≤|<=|<)\s*(\d+(?:\.\d+)?)\s*(ns|µs|us|ms|s)(?:\s*/\s*(档|条))?",
    )
    .unwrap()
});
static BACKTICK: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"`([^`\n]+)`").unwrap());

/// One comparison in a budget cell.
#[derive(Debug, Clone, PartialEq)]
pub struct Cmp {
    pub pct: Option<String>,
    pub ns: f64,
    pub per: Option<String>,
    pub strict: bool,
}

fn unit_ns(u: &str) -> f64 {
    match u {
        "ns" => 1.0,
        "µs" | "us" => 1e3,
        "ms" => 1e6,
        _ => 1e9,
    }
}

/// Comparisons of a cell (relative `+ ≤ x` ones excluded).
pub fn cmps(cell: &str) -> Vec<Cmp> {
    let mut out = Vec::new();
    for c in CMP.captures_iter(cell) {
        let m = c.get(0).unwrap();
        let after = &cell[m.end()..];
        if after.chars().next().is_some_and(|x| x.is_ascii_alphabetic()) {
            continue;
        }
        let op_start = c.get(2).unwrap().start();
        let before = cell[..c.get(1).map_or(op_start, |p| p.start())].trim_end();
        if before.ends_with('+') {
            continue;
        }
        out.push(Cmp {
            pct: c.get(1).map(|x| x.as_str().to_string()),
            ns: c[3].parse::<f64>().unwrap_or(0.0) * unit_ns(&c[4]),
            per: c.get(5).map(|x| x.as_str().to_string()),
            strict: &c[2] == "<",
        });
    }
    out
}

/// budgets.toml key of a percentile (`p50` → `p50_ns`, `p99.9` → `p999_ns`); `None` → `budget_ns`.
pub fn key(pct: Option<&str>) -> String {
    match pct {
        Some(p) => format!("{}_ns", p.replace('.', "")),
        None => "budget_ns".to_string(),
    }
}

/// One benchmark row of a README budget table.
#[derive(Debug, Clone)]
pub struct BudgetRow {
    pub line: usize,
    pub name: String,
    /// `None` = values not comparable (ambiguous cell).
    pub values: Option<BTreeMap<String, f64>>,
    pub first: String,
}

/// Benchmark names registered in benchmarks/README.md "基准名登记".
pub fn registered(cx: &Ctx) -> Vec<String> {
    let Some(d) = cx.doc("benchmarks/README.md") else {
        return Vec::new();
    };
    tables_under(d, "基准名登记")
        .iter()
        .flat_map(|t| t.rows.iter())
        .filter_map(|(_, c)| {
            BACKTICK
                .captures(c.first().map_or("", |s| s.as_str()))
                .map(|m| m[1].to_string())
        })
        .collect()
}

/// Rows of the budget tables of a README's "延迟与资源契约" (only registered benchmark names).
pub fn budget_rows(d: &Doc, reg: &[String]) -> Vec<BudgetRow> {
    let mut out = Vec::new();
    for t in tables_under(d, "延迟与资源契约") {
        let n = t.header.len();
        if n < 3 {
            continue;
        }
        let slash = t.header[1..n - 1]
            .iter()
            .any(|h| h.replace(' ', "").contains("p50/p99"));
        for (ln, c) in &t.rows {
            let names: Vec<String> = BACKTICK
                .captures_iter(c.last().map_or("", |s| s.as_str()))
                .map(|m| m[1].to_string())
                .filter(|x| reg.contains(x))
                .collect();
            if names.is_empty() {
                continue;
            }
            let all: Vec<Cmp> = c[1..c.len().saturating_sub(1).max(1)]
                .iter()
                .flat_map(|x| cmps(x))
                .filter(|x| x.per.is_none())
                .collect();
            let per_name: Vec<Option<BTreeMap<String, f64>>> = if slash {
                let un: Vec<&Cmp> = all.iter().filter(|x| x.pct.is_none()).collect();
                let mut m = BTreeMap::new();
                for (k, x) in ["p50_ns", "p99_ns"].iter().zip(un.iter()) {
                    m.insert(k.to_string(), x.ns);
                }
                vec![Some(m); names.len()]
            } else if all.iter().any(|x| x.pct.is_some()) {
                let mut m: BTreeMap<String, f64> = BTreeMap::new();
                let mut dup = Vec::new();
                for x in all.iter().filter(|x| x.pct.is_some()) {
                    let k = key(x.pct.as_deref());
                    if m.get(&k).is_some_and(|v| *v != x.ns) {
                        dup.push(k.clone());
                    }
                    m.insert(k, x.ns);
                }
                for k in dup {
                    m.remove(&k);
                }
                vec![Some(m); names.len()]
            } else if all.len() == names.len() {
                all.iter()
                    .map(|x| Some(BTreeMap::from([(key(None), x.ns)])))
                    .collect()
            } else if all.is_empty() {
                vec![Some(BTreeMap::new()); names.len()]
            } else {
                vec![None; names.len()]
            };
            for (name, v) in names.into_iter().zip(per_name) {
                out.push(BudgetRow {
                    line: *ln,
                    name,
                    values: v,
                    first: c[0].clone(),
                });
            }
        }
    }
    out
}

/// Bound cases of a 上限 cell: `(label, comparisons)`.
fn bound_cases(cell: &str) -> Vec<(String, Vec<Cmp>)> {
    let lab = Regex::new(r"^\s*([^:\x{FF1A}]{1,40})[:\x{FF1A}]\s*p\d").unwrap();
    cell.split([';', '\u{FF1B}'])
        .filter_map(|part| {
            let c = cmps(part);
            (!c.is_empty()).then(|| {
                (
                    lab.captures(part)
                        .map(|m| m[1].trim().to_string())
                        .unwrap_or_default(),
                    c,
                )
            })
        })
        .collect()
}

fn bound_table(cx: &Ctx) -> Option<Table> {
    let d = cx.doc("benchmarks/README.md")?;
    d.tables().into_iter().find(|t| {
        t.header.first().is_some_and(|h| h == "原语") && t.header.iter().any(|h| h.contains("上限"))
    })
}

fn num(v: &toml::Value) -> Option<f64> {
    v.as_float().or_else(|| v.as_integer().map(|i| i as f64))
}

/// "预算": every registered benchmark of a hot crate README's 延迟与资源契约 is in `benches/budgets.toml` with the
/// same values; a budgets.toml entry with `bound` is not wider than that row of the 单 crate 基准上限表.
/// Applies to `tier: hot` crates under `crates/` and `strategies/` that have a `Cargo.toml` (implemented).
pub fn check_budgets(cx: &Ctx) -> Vec<Problem> {
    let reg = registered(cx);
    let bounds = bound_table(cx);
    let mut globs = Globs::default();
    let mut out = Vec::new();
    for d in cx.content() {
        if !(globs.is_match("crates/*/README.md", &d.rel) || globs.is_match("strategies/*/README.md", &d.rel))
            || d.fm_get("tier") != Some("hot")
        {
            continue;
        }
        let dir = d.rel.trim_end_matches("README.md");
        if !cx.root.join(dir).join("Cargo.toml").is_file() {
            continue;
        }
        let rows = budget_rows(d, &reg);
        let bpath = format!("{dir}benches/budgets.toml");
        let toml: toml::Table = match std::fs::read_to_string(cx.root.join(&bpath)) {
            Ok(t) => match t.parse() {
                Ok(x) => x,
                Err(e) => {
                    out.push(("BUDGET", format!("{bpath}: {e}")));
                    continue;
                }
            },
            Err(_) => {
                if !rows.is_empty() {
                    out.push((
                        "BUDGET",
                        format!(
                            "{bpath}: missing ({} benchmarks in {} 延迟与资源契约)",
                            rows.len(),
                            d.rel
                        ),
                    ));
                }
                continue;
            }
        };
        for r in &rows {
            let Some(e) = toml.get(&r.name).and_then(|v| v.as_table()) else {
                out.push((
                    "BUDGET",
                    format!(
                        "{}:{}: benchmark `{}` is not in {bpath}",
                        d.rel,
                        r.line + 1,
                        r.name
                    ),
                ));
                continue;
            };
            let Some(vals) = &r.values else { continue };
            for (k, v) in vals {
                match e.get(k).and_then(num) {
                    Some(x) if (x - v).abs() < 1e-6 => {}
                    got => out.push((
                        "BUDGET",
                        format!(
                            "{}:{}: `{}` {k} = {v} ns in the README, {} in {bpath}",
                            d.rel,
                            r.line + 1,
                            r.name,
                            got.map_or("absent".to_string(), |x| format!("{x} ns"))
                        ),
                    )),
                }
            }
            for (k, x) in e {
                if k.ends_with("_ns") && !vals.contains_key(k) {
                    out.push((
                        "BUDGET",
                        format!(
                            "{bpath}: `{}` {k} = {x} has no value in {}:{}",
                            r.name,
                            d.rel,
                            r.line + 1
                        ),
                    ));
                }
            }
        }
        // Entries vs the 单 crate 基准上限表.
        for (name, e) in &toml {
            let Some(e) = e.as_table() else { continue };
            let Some(prim) = e.get("bound").and_then(|v| v.as_str()) else {
                continue;
            };
            let Some(bt) = &bounds else {
                out.push((
                    "BUDGET",
                    "benchmarks/README.md: 单 crate 基准上限表 not found".to_string(),
                ));
                continue;
            };
            let col = bt.header.iter().position(|h| h.contains("上限")).unwrap_or(2);
            let Some((_, row)) = bt
                .rows
                .iter()
                .find(|(_, c)| c.first().is_some_and(|x| x.trim() == prim))
            else {
                out.push((
                    "BUDGET",
                    format!("{bpath}: `{name}` bound `{prim}` is not a row of the 单 crate 基准上限表"),
                ));
                continue;
            };
            let cases = bound_cases(row.get(col).map_or("", |s| s.as_str()));
            let case = e.get("case").and_then(|v| v.as_str());
            let pick: Vec<&(String, Vec<Cmp>)> = match case {
                Some(c) => cases.iter().filter(|(l, _)| l.contains(c)).collect(),
                None => cases.iter().collect(),
            };
            if pick.len() != 1 {
                out.push((
                    "BUDGET",
                    format!(
                        "{bpath}: `{name}` bound `{prim}`: {} matching cases (set `case`)",
                        pick.len()
                    ),
                ));
                continue;
            }
            let levels = e.get("levels").and_then(num);
            for b in &pick[0].1 {
                let keys: Vec<String> = match &b.pct {
                    Some(p) => vec![key(Some(p))],
                    None => e.keys().filter(|k| k.ends_with("_ns")).cloned().collect(),
                };
                for k in keys {
                    let Some(mut v) = e.get(&k).and_then(num) else {
                        continue;
                    };
                    if b.per.is_some() {
                        match levels {
                            Some(l) if l > 0.0 => v /= l,
                            _ => {
                                out.push((
                                    "BUDGET",
                                    format!(
                                        "{bpath}: `{name}`: bound is per {} — set `levels`",
                                        b.per.as_deref().unwrap_or("")
                                    ),
                                ));
                                continue;
                            }
                        }
                    }
                    if v > b.ns || (b.strict && v >= b.ns) {
                        out.push((
                            "BUDGET",
                            format!("{bpath}: `{name}` {k} = {v} ns is wider than the 单 crate 基准上限表 `{prim}` bound {}{} ns", if b.strict { "< " } else { "≤ " }, b.ns),
                        ));
                    }
                }
            }
        }
    }
    out
}

// ------------------------------------------------------------------------------------------ venues

/// Threshold class of a budget row by its entry cell (`None` = no threshold row).
pub fn venue_class(first: &str) -> Option<&'static str> {
    let entry = BACKTICK
        .captures_iter(first)
        .map(|m| m[1].to_string())
        .find(|t| {
            matches!(
                t.as_str(),
                "classify"
                    | "classify_report"
                    | "decode_md"
                    | "encode"
                    | "decode_report"
                    | "decode_snapshot"
                    | "acct_decode"
            )
        })?;
    let has = |rx: &str| Regex::new(rx).unwrap().is_match(first);
    Some(match entry.as_str() {
        "classify" | "classify_report" => "classify",
        "decode_md" if has(r"增量|L2Batch|depthUpdate") => return None,
        "decode_md" if has(r"分档|TopN|快照|books\d|depth\d") => "topn",
        "decode_md" => "bbo",
        "encode" => "encode",
        "decode_report" if has(r"忽略|未知|Ignorable|Unknown") => "ignore",
        "decode_report" if has(r"保证金|MarginReport|account|positions|持仓") => "margin",
        "decode_report" => "trade",
        "decode_snapshot" => "snapshot",
        _ => "acct",
    })
}

/// hft-venue-proto 统一门槛表: class → (p50, p99) in ns.
fn thresholds(cx: &Ctx) -> Option<BTreeMap<&'static str, (f64, f64)>> {
    let d = cx.doc("crates/hft-venue-proto/README.md")?;
    let t = tables_under(d, "延迟与资源契约")
        .into_iter()
        .find(|t| t.header.iter().any(|h| h.contains("p50")))?;
    let p50 = t.header.iter().position(|h| h.contains("p50"))?;
    let p99 = t.header.iter().position(|h| h.contains("p99"))?;
    let mut out = BTreeMap::new();
    for (_, c) in &t.rows {
        let Some(cls) = venue_class(&c[0]) else { continue };
        let a = cmps(c.get(p50).map_or("", |s| s.as_str())).first().map(|x| x.ns);
        let b = cmps(c.get(p99).map_or("", |s| s.as_str())).first().map(|x| x.ns);
        if let (Some(a), Some(b)) = (a, b) {
            out.entry(cls).or_insert((a, b));
        }
    }
    Some(out)
}

/// Required venue README sections: the first column of hft-venue-proto "每个 venue README 的必备节", up to `(`.
fn required_sections(cx: &Ctx) -> Option<Vec<String>> {
    let d = cx.doc("crates/hft-venue-proto/README.md")?;
    let t = tables_under(d, "每个 venue README 的必备节").into_iter().next()?;
    Some(
        t.rows
            .iter()
            .map(|(_, c)| {
                c[0].split(['(', '\u{FF08}'])
                    .next()
                    .unwrap_or("")
                    .trim()
                    .to_string()
            })
            .filter(|s| !s.is_empty())
            .collect(),
    )
}

static BOTH: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"两者|两个产品线|两产品线|两种产品线").unwrap());

/// "venue README": for `scope: core` `crates/venue-*` READMEs — every required section present (heading text
/// starting with the name); a section that names one market (现货 or a `MARKET = Spot` product type; 永续 or a
/// `MARKET = LinearPerp` type) names the other too, unless it says it covers both (两者 / 两个产品线); every row
/// of the budget table has p50 and p99, none wider than the hft-venue-proto threshold of its class; and the
/// `ingest` allocation of hft-venue-proto holds (hft-net `net_rx_300b_mem`/`net_rx_2k_mem` + `classify` +
/// `decode_md` thresholds vs docs/latency-budget.md §3 `ingest` targets: BBO p50 ≤ 1/3, p99 ≤ 1/5; depth
/// p50 ≤ 1/2, p99 ≤ 1/3).
pub fn check_venues(cx: &Ctx) -> Vec<Problem> {
    let mut out = Vec::new();
    let Some(req) = required_sections(cx) else {
        return vec![(
            "VENUE",
            "crates/hft-venue-proto/README.md: 每个 venue README 的必备节 table not found".to_string(),
        )];
    };
    let thr = thresholds(cx).unwrap_or_default();
    let mut globs = Globs::default();
    let spot_rx = Regex::new(r"pub struct (\w+);.*MARKET\s*=\s*Spot\b").unwrap();
    let perp_rx = Regex::new(r"pub struct (\w+);.*MARKET\s*=\s*LinearPerp\b").unwrap();
    for d in cx.content() {
        if !globs.is_match("crates/venue-*/README.md", &d.rel) || d.fm_get("scope") != Some("core") {
            continue;
        }
        let words = |rx: &Regex| -> Vec<Regex> {
            rx.captures_iter(&d.text)
                .map(|c| {
                    Regex::new(&format!(
                        r"(?:^|[^A-Za-z0-9_]){}(?:[^A-Za-z0-9_]|$)",
                        regex::escape(&c[1])
                    ))
                    .unwrap()
                })
                .collect()
        };
        let (spot, perp) = (words(&spot_rx), words(&perp_rx));
        for name in &req {
            let Some(h) = d.find_head(|h| h.level >= 2 && h.text.starts_with(name.as_str())) else {
                out.push((
                    "VENUE",
                    format!(
                        "{}: missing required section `{name}` (hft-venue-proto 每个 venue README 的必备节)",
                        d.rel
                    ),
                ));
                continue;
            };
            let (a, b) = d.section(h);
            let body = d.text_of(d.heads[h].line, b.max(a));
            let s = body.contains("现货") || spot.iter().any(|r| r.is_match(&body));
            let p = body.contains("永续") || perp.iter().any(|r| r.is_match(&body));
            if s != p && !BOTH.is_match(&body) {
                out.push((
                    "VENUE",
                    format!("{}:{}: section `{name}` covers only {} (write the other market too, or say it covers both)", d.rel, d.heads[h].line + 1, if s { "现货" } else { "永续" }),
                ));
            }
        }
        for t in tables_under(d, "延迟与资源契约") {
            let n = t.header.len();
            if n < 3
                || !t.header[1..n - 1]
                    .iter()
                    .any(|h| h.replace(' ', "").contains("p50/p99"))
            {
                continue;
            }
            for (ln, c) in &t.rows {
                let vals: Vec<f64> = c[1..n - 1]
                    .iter()
                    .flat_map(|x| cmps(x))
                    .filter(|x| x.pct.is_none() && x.per.is_none())
                    .map(|x| x.ns)
                    .collect();
                if vals.len() < 2 {
                    out.push((
                        "VENUE",
                        format!(
                            "{}:{}: budget row `{}` lacks p50 or p99 (write `≤ p50 / ≤ p99`)",
                            d.rel,
                            ln + 1,
                            c[0]
                        ),
                    ));
                    continue;
                }
                if let Some((t50, t99)) = venue_class(&c[0]).and_then(|k| thr.get(k))
                    && (vals[0] > *t50 || vals[1] > *t99)
                {
                    out.push((
                            "VENUE",
                            format!("{}:{}: `{}` p50/p99 {}/{} ns is wider than the hft-venue-proto threshold {t50}/{t99} ns", d.rel, ln + 1, c[0], vals[0], vals[1]),
                        ));
                }
            }
        }
    }
    out.extend(check_ingest_allocation(cx, &thr));
    out
}

/// `(hft-net net_rx rows, ingest(bookTicker) targets, ingest(深度) targets)`, each `(p50, p99)` in ns.
type AllocationInputs = (BTreeMap<String, (f64, f64)>, (f64, f64), (f64, f64));

fn ingest_allocation_inputs(cx: &Ctx) -> Option<AllocationInputs> {
    let net = cx.doc("crates/hft-net/README.md")?;
    let mut nets = BTreeMap::new();
    for t in tables_under(net, "延迟与资源契约") {
        for (_, c) in &t.rows {
            let last = c.last().map_or("", |s| s.as_str());
            for name in ["net_rx_300b_mem", "net_rx_2k_mem"] {
                if last.contains(&format!("`{name}`")) {
                    let v: Vec<f64> = c[1..c.len() - 1]
                        .iter()
                        .flat_map(|x| cmps(x))
                        .filter(|x| x.pct.is_none())
                        .map(|x| x.ns)
                        .collect();
                    if v.len() >= 2 {
                        nets.insert(name.to_string(), (v[0], v[1]));
                    }
                }
            }
        }
    }
    let lb = cx.doc("docs/latency-budget.md")?;
    let mut bbo = None;
    let mut depth = None;
    for t in lb.tables() {
        let (Some(p50), Some(p99)) = (
            t.header.iter().position(|h| h == "目标 p50"),
            t.header.iter().position(|h| h == "目标 p99"),
        ) else {
            continue;
        };
        for (_, c) in &t.rows {
            let first = &c[0];
            if !first.starts_with("`ingest`") {
                continue;
            }
            let a = cmps(c.get(p50).map_or("", |s| s.as_str()))
                .first()
                .map(|x| x.ns)?;
            let b = cmps(c.get(p99).map_or("", |s| s.as_str()))
                .first()
                .map(|x| x.ns)?;
            if first.contains("bookTicker") {
                bbo = Some((a, b));
            } else if first.contains("深度") {
                depth = Some((a, b));
            }
        }
    }
    Some((nets, bbo?, depth?))
}

fn check_ingest_allocation(cx: &Ctx, thr: &BTreeMap<&'static str, (f64, f64)>) -> Vec<Problem> {
    let Some((nets, bbo, depth)) = ingest_allocation_inputs(cx) else {
        return vec![("VENUE", "ingest allocation: hft-net net_rx rows, hft-venue-proto thresholds or latency-budget §3 `ingest` rows not found".to_string())];
    };
    let mut out = Vec::new();
    let paths = [
        ("BBO", "net_rx_300b_mem", "bbo", bbo, (3.0, 5.0)),
        ("depth", "net_rx_2k_mem", "topn", depth, (2.0, 3.0)),
    ];
    for (what, net, cls, ingest, (f50, f99)) in paths {
        let (Some(n), Some(c), Some(dm)) = (nets.get(net), thr.get("classify"), thr.get(cls)) else {
            out.push((
                "VENUE",
                format!("ingest allocation ({what}): missing `{net}` or a threshold row"),
            ));
            continue;
        };
        let s50 = n.0 + c.0 + dm.0;
        let s99 = n.1 + c.1 + dm.1;
        if s50 > ingest.0 / f50 + 1e-6 || s99 > ingest.1 / f99 + 1e-6 {
            out.push((
                "VENUE",
                format!("ingest allocation ({what}): `{net}` + classify + decode_md = {s50}/{s99} ns exceeds `ingest` target {}/{} ns × 1/{f50}, 1/{f99}", ingest.0, ingest.1),
            ));
        }
    }
    out
}
