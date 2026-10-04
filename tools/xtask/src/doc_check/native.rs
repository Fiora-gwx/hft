//! Native doc-check checks (tools/README.md "cargo xtask doc-check" table). Each check returns problems
//! `(category, message)`; any problem fails the gate.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use regex::Regex;

use super::config::{Config, Rule};
use super::md::{Doc, Globs, Table, resolve};
use crate::rel_path;

/// One finding.
pub type Problem = (&'static str, String);

/// The scanned tree.
#[derive(Debug)]
pub struct Ctx {
    pub root: PathBuf,
    pub cfg: Config,
    /// Markdown files (repo-relative path → doc).
    pub docs: BTreeMap<String, Doc>,
    /// Every other scanned file (for rules that select non-Markdown files).
    pub others: Vec<String>,
    /// Directories of components whose README says `status: frozen`.
    pub frozen: Vec<String>,
}

fn under(rel: &str, prefix: &str) -> bool {
    let p = prefix.trim_end_matches('/');
    rel == p || rel.starts_with(&format!("{p}/"))
}

impl Ctx {
    pub fn load(root: &Path, cfg: Config) -> Ctx {
        let mut docs = BTreeMap::new();
        let mut others = Vec::new();
        let excl = cfg.exclude.clone();
        let walk = walkdir::WalkDir::new(root)
            .sort_by_file_name()
            .into_iter()
            .filter_entry(|e| {
                let r = rel_path(root, e.path());
                e.depth() == 0 || !excl.iter().any(|x| under(&r, x))
            });
        for e in walk.flatten() {
            if !e.file_type().is_file() {
                continue;
            }
            let rel = rel_path(root, e.path());
            if rel.ends_with(".md") {
                if let Ok(t) = std::fs::read_to_string(e.path()) {
                    docs.insert(rel.clone(), Doc::new(&rel, t));
                }
            } else {
                others.push(rel);
            }
        }
        let frozen = docs
            .values()
            .filter(|d| d.rel.ends_with("README.md") && d.fm_get("status") == Some("frozen"))
            .map(|d| {
                d.rel
                    .rsplit_once('/')
                    .map_or(String::new(), |(a, _)| a.to_string())
            })
            .collect();
        Ctx {
            root: root.to_path_buf(),
            cfg,
            docs,
            others,
            frozen,
        }
    }

    pub fn is_frozen(&self, rel: &str) -> bool {
        self.frozen.iter().any(|d| !d.is_empty() && under(rel, d))
    }

    pub fn is_evidence(&self, rel: &str) -> bool {
        self.cfg.evidence.iter().any(|x| under(rel, x))
    }

    /// Spec files subject to content checks: not evidence, not frozen.
    pub fn content(&self) -> impl Iterator<Item = &Doc> {
        self.docs
            .values()
            .filter(|d| !self.is_evidence(&d.rel) && !self.is_frozen(&d.rel))
    }

    pub fn doc(&self, rel: &str) -> Option<&Doc> {
        self.docs.get(rel)
    }
}

fn loc(d: &Doc, i: usize) -> String {
    format!("{}:{}", d.rel, i + 1)
}

fn snippet(s: &str) -> String {
    let t: String = s.trim().chars().take(90).collect();
    t
}

static BACKTICK: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"`([^`\n]+)`").unwrap());
static SNAKE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^[a-z][a-z0-9_]*$").unwrap());

// ------------------------------------------------------------------------------------------- links

/// "链接": relative link targets exist; anchors exist (GitHub rule).
pub fn check_links(cx: &Ctx) -> Vec<Problem> {
    let mut out = Vec::new();
    let mut anchor_cache: HashMap<String, BTreeSet<String>> = HashMap::new();
    for d in cx.docs.values() {
        for (i, t) in d.links() {
            if t.starts_with("http://") || t.starts_with("https://") || t.starts_with("mailto:") {
                continue;
            }
            let (path, frag) = t.split_once('#').map_or((t.as_str(), ""), |(a, b)| (a, b));
            let target = if path.is_empty() {
                Some(d.rel.clone())
            } else {
                resolve(&d.rel, path)
            };
            let Some(target) = target else {
                out.push(("LINK", format!("{}: link above the repo root: {t}", loc(d, i))));
                continue;
            };
            let fs = cx.root.join(&target);
            if !fs.exists() {
                out.push(("LINK", format!("{}: link target does not exist: {t}", loc(d, i))));
                continue;
            }
            if frag.is_empty() || !target.ends_with(".md") || !fs.is_file() {
                continue;
            }
            let anchors = anchor_cache.entry(target.clone()).or_insert_with(|| {
                let doc = cx
                    .doc(&target)
                    .cloned()
                    .unwrap_or_else(|| Doc::new(&target, std::fs::read_to_string(&fs).unwrap_or_default()));
                doc.anchors().into_iter().collect()
            });
            if !anchors.contains(&frag.to_lowercase()) {
                out.push(("ANCHOR", format!("{}: anchor does not exist: {t}", loc(d, i))));
            }
        }
    }
    out
}

// ------------------------------------------------------------------------------- README structure

/// Template headings (`## …` / `### …` lines of the code block under crates/AGENTS.md "README 模板").
pub fn template_sections(cx: &Ctx) -> Result<Vec<(usize, String)>, String> {
    let d = cx.doc("crates/AGENTS.md").ok_or("crates/AGENTS.md not found")?;
    let h = d
        .find_head(|h| h.text == "README 模板")
        .ok_or("crates/AGENTS.md: no `## README 模板` section")?;
    let (a, b) = d.section(h);
    let block = d
        .code_blocks()
        .into_iter()
        .find(|(s, _)| *s > a && *s < b)
        .ok_or("crates/AGENTS.md: README 模板 has no code block")?;
    let rx = Regex::new(r"^(#{2,3})\s+(\S.*)$").unwrap();
    Ok(block
        .1
        .iter()
        .filter_map(|l| rx.captures(l).map(|c| (c[1].len(), c[2].trim().to_string())))
        .filter(|(_, t)| !t.starts_with('<'))
        .collect())
}

/// "README 结构": frontmatter keys and values; component README section order.
pub fn check_structure(cx: &Ctx) -> Vec<Problem> {
    let mut out = Vec::new();
    let mut globs = Globs::default();
    let sections = match template_sections(cx) {
        Ok(s) => s,
        Err(e) => return vec![("STRUCTURE", e)],
    };
    for d in cx.docs.values() {
        if cx.is_evidence(&d.rel) {
            continue;
        }
        if let Some(fm) = &d.fm {
            if let Some(s) = fm.get("scope")
                && !cx.cfg.fm_scope.contains(s)
            {
                out.push((
                    "FRONTMATTER",
                    format!("{}: scope '{s}' is not one of {:?}", d.rel, cx.cfg.fm_scope),
                ));
            }
            if let Some(s) = fm.get("status")
                && !cx.cfg.fm_status.contains(s)
            {
                out.push((
                    "FRONTMATTER",
                    format!("{}: status '{s}' is not one of {:?}", d.rel, cx.cfg.fm_status),
                ));
            }
        }
        if !cx.cfg.components.iter().any(|g| globs.is_match(g, &d.rel)) {
            continue;
        }
        match &d.fm {
            None => out.push(("FRONTMATTER", format!("{}: no frontmatter", d.rel))),
            Some(fm) => {
                for k in &cx.cfg.fm_keys {
                    if !fm.contains_key(k) {
                        out.push(("FRONTMATTER", format!("{}: frontmatter lacks `{k}`", d.rel)));
                    }
                }
            }
        }
        let mut last: Option<usize> = None;
        for (lv, t) in &sections {
            match d.heads.iter().find(|h| h.level == *lv && h.text == *t) {
                None => out.push((
                    "SECTIONS",
                    format!("{}: missing `{} {t}`", d.rel, "#".repeat(*lv)),
                )),
                Some(h) => {
                    if last.is_some_and(|l| h.line < l) {
                        out.push((
                            "SECTIONS",
                            format!(
                                "{}:{}: `{} {t}` out of template order",
                                d.rel,
                                h.line + 1,
                                "#".repeat(*lv)
                            ),
                        ));
                    } else {
                        last = Some(h.line);
                    }
                }
            }
        }
    }
    out
}

// -------------------------------------------------------------------------------------------- scope

/// "范围": root README component table vs frontmatter `scope`/`tier`; every crate and strategy README listed.
pub fn check_scope_table(cx: &Ctx) -> Vec<Problem> {
    let mut out = Vec::new();
    let Some(readme) = cx.doc("README.md") else {
        return vec![("SCOPE", "README.md not found".to_string())];
    };
    let link = Regex::new(r"\]\(([^)#\s]+)\)").unwrap();
    let mut listed = BTreeSet::new();
    for t in readme.tables() {
        let (Some(tc), Some(sc)) = (
            t.header.iter().position(|h| h == "tier"),
            t.header.iter().position(|h| h == "范围"),
        ) else {
            continue;
        };
        for (ln, cells) in &t.rows {
            let (tier, scope) = (
                cells.get(tc).map_or("", |s| s.as_str()),
                cells.get(sc).map_or("", |s| s.as_str()),
            );
            for c in link.captures_iter(cells.first().map_or("", |s| s.as_str())) {
                let Some(r) = resolve("README.md", &c[1]) else {
                    continue;
                };
                listed.insert(r.clone());
                let Some(fm) = cx.doc(&r).and_then(|d| d.fm.as_ref()) else {
                    continue;
                };
                if let Some(ft) = fm.get("tier")
                    && !matches!(tier, "—" | "-")
                    && ft != tier
                {
                    out.push((
                        "SCOPE",
                        format!(
                            "README.md:{}: {r}: table tier `{tier}`, frontmatter `{ft}`",
                            ln + 1
                        ),
                    ));
                }
                if let Some(fs) = fm.get("scope")
                    && matches!(scope, "core" | "optional")
                    && fs != scope
                {
                    out.push((
                        "SCOPE",
                        format!(
                            "README.md:{}: {r}: table scope `{scope}`, frontmatter `{fs}`",
                            ln + 1
                        ),
                    ));
                }
            }
        }
    }
    let mut globs = Globs::default();
    for d in cx.docs.values() {
        if (globs.is_match("crates/*/README.md", &d.rel) || globs.is_match("strategies/*/README.md", &d.rel))
            && d.fm.is_some()
            && !listed.contains(&d.rel)
        {
            out.push((
                "SCOPE",
                format!("{}: not in the README.md component table", d.rel),
            ));
        }
    }
    out
}

// ------------------------------------------------------------------------------------ pattern rules

fn rule_files<'a>(cx: &'a Ctx, r: &Rule, globs: &mut Globs) -> Vec<(String, Option<&'a Doc>)> {
    let mut out = Vec::new();
    let pick = |rel: &str, globs: &mut Globs| {
        r.files.iter().any(|g| globs.is_match(g, rel))
            && !r.skip.iter().any(|s| under(rel, s))
            && !cx.is_evidence(rel)
            && !cx.is_frozen(rel)
    };
    for d in cx.docs.values() {
        if pick(&d.rel, globs) {
            out.push((d.rel.clone(), Some(d)));
        }
    }
    for o in &cx.others {
        if pick(o, globs) {
            out.push((o.clone(), None));
        }
    }
    out
}

/// "策略族名" / "写当前状态" / "残留" / 排期措辞: the `[[rule]]` patterns of doc-check.toml.
pub fn check_rules(cx: &Ctx) -> Vec<Problem> {
    let mut out = Vec::new();
    let mut globs = Globs::default();
    for r in &cx.cfg.rules {
        let cat: &'static str = match r.check.as_str() {
            "strategy-name" => "STRATNAME",
            "history" => "HISTORY",
            "schedule" => "SCHEDULE",
            _ => "RESIDUE",
        };
        for (rel, doc) in rule_files(cx, r, &mut globs) {
            let owned;
            let d = match doc {
                Some(d) => d,
                None => {
                    owned = Doc::new(
                        &rel,
                        std::fs::read_to_string(cx.root.join(&rel)).unwrap_or_default(),
                    );
                    &owned
                }
            };
            if let Some(sc) = &r.scope
                && d.fm_get("scope") != Some(sc.as_str())
            {
                continue;
            }
            let enclosing = r.skip_sections.as_ref().map(|_| d.enclosing());
            let mut hits = Vec::new();
            for (i, line) in d.lines.iter().enumerate() {
                if r.skip_code && d.code[i] {
                    continue;
                }
                if let (Some(rx), Some(enc)) = (&r.skip_sections, &enclosing)
                    && enc[i].iter().any(|h| rx.is_match(h))
                {
                    continue;
                }
                let mut t = line.clone();
                for s in &r.strip {
                    t = s.replace_all(&t, "").into_owned();
                }
                if r.exempt_line.as_ref().is_some_and(|x| x.is_match(&t)) {
                    continue;
                }
                if r.pattern.is_match(&t) {
                    hits.push(i);
                }
            }
            if let Some(&first) = hits.first() {
                out.push((
                    cat,
                    format!(
                        "{rel}:{}: [{}] {}{}",
                        first + 1,
                        r.id,
                        snippet(&d.lines[first]),
                        if hits.len() > 1 {
                            format!(" (+{} more lines)", hits.len() - 1)
                        } else {
                            String::new()
                        }
                    ),
                ));
            }
        }
    }
    // Deleted components: any mention (links included).
    for name in &cx.cfg.deleted_components {
        let rx = Regex::new(&format!(
            r"(?:^|[^A-Za-z0-9_\-]){}(?:[^A-Za-z0-9_\-]|$)",
            regex::escape(name)
        ))
        .unwrap();
        for d in cx.content() {
            if cx.cfg.deleted_skip.iter().any(|s| under(&d.rel, s)) {
                continue;
            }
            if let Some(i) = d.lines.iter().position(|l| rx.is_match(l)) {
                out.push((
                    "RESIDUE",
                    format!(
                        "{}: deleted component `{name}`: {}",
                        loc(d, i),
                        snippet(&d.lines[i])
                    ),
                ));
            }
        }
    }
    out
}

// ------------------------------------------------------------------------------------- Agent Notes

/// "Agent Notes": the class directory of every note is in the closed set (links: [`check_links`]).
pub fn check_notes(cx: &Ctx) -> Vec<Problem> {
    let mut out = Vec::new();
    let base = cx.root.join(&cx.cfg.notes_root);
    for lc in &cx.cfg.notes_lifecycles {
        let Ok(rd) = std::fs::read_dir(base.join(lc)) else {
            continue;
        };
        let mut ents: Vec<_> = rd.flatten().collect();
        ents.sort_by_key(|e| e.file_name());
        for e in ents {
            let name = e.file_name().to_string_lossy().into_owned();
            let rel = format!("{}/{lc}/{name}", cx.cfg.notes_root);
            if e.path().is_dir() {
                if !cx.cfg.notes_classes.contains(&name) {
                    out.push((
                        "NOTES",
                        format!(
                            "{rel}: class directory `{name}` is not one of {:?}",
                            cx.cfg.notes_classes
                        ),
                    ));
                }
            } else if name.ends_with(".md") {
                out.push(("NOTES", format!("{rel}: note outside a class directory")));
            }
        }
    }
    out
}

// -------------------------------------------------------------------------------------------- length

/// "篇幅": root AGENTS.md ≤ 1,800, subtree AGENTS.md ≤ 900, docs/architecture.md ≤ 3,000 (measure:
/// [`super::doc_length`]).
pub const LENGTH_ROOT: usize = 1800;
pub const LENGTH_SUBTREE: usize = 900;
pub const LENGTH_ARCH: usize = 3000;

pub fn check_length(cx: &Ctx) -> Vec<Problem> {
    let mut out = Vec::new();
    for d in cx.docs.values() {
        let lim = if d.rel == "AGENTS.md" {
            LENGTH_ROOT
        } else if d.rel.ends_with("/AGENTS.md") {
            LENGTH_SUBTREE
        } else if d.rel == "docs/architecture.md" {
            LENGTH_ARCH
        } else {
            continue;
        };
        let n = super::doc_length(&d.text);
        if n > lim {
            out.push(("LENGTH", format!("{}: length {n} > {lim}", d.rel)));
        }
    }
    out
}

// ------------------------------------------------------------------------------------------ runbooks

/// "runbook 结构": `scope: core` pages of docs/runbooks/ have the six level-2 headings of the 写法规则, in order
/// (other headings may sit between them; a heading matches by its text or text followed by `(`/`(`).
pub fn check_runbooks(cx: &Ctx) -> Vec<Problem> {
    let Some(rd) = cx.doc("docs/runbooks/README.md") else {
        return vec![("RUNBOOK", "docs/runbooks/README.md not found".to_string())];
    };
    let rx = Regex::new(r"每页结构[:\x{FF1A}]\s*\*\*([^*]+)\*\*").unwrap();
    let Some(order) = rd
        .lines
        .iter()
        .find_map(|l| rx.captures(l).map(|c| c[1].to_string()))
    else {
        return vec![(
            "RUNBOOK",
            "docs/runbooks/README.md: no `每页结构:**… → …**` line".to_string(),
        )];
    };
    let want: Vec<String> = order
        .split('→')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();
    let mut out = Vec::new();
    for d in cx.docs.values() {
        if !d.rel.starts_with("docs/runbooks/")
            || d.rel.ends_with("/README.md")
            || d.rel.matches('/').count() != 2
        {
            continue;
        }
        if d.fm_get("scope") != Some("core") {
            continue;
        }
        let mut last: Option<usize> = None;
        for w in &want {
            let hit = d.heads.iter().find(|h| {
                h.level == 2
                    && (h.text == *w
                        || h.text.starts_with(&format!("{w}("))
                        || h.text.starts_with(&format!("{w}\u{FF08}")))
            });
            match hit {
                None => out.push(("RUNBOOK", format!("{}: missing `## {w}`", d.rel))),
                Some(h) => {
                    if last.is_some_and(|l| h.line < l) {
                        out.push((
                            "RUNBOOK",
                            format!(
                                "{}:{}: `## {w}` out of order ({})",
                                d.rel,
                                h.line + 1,
                                want.join(" → ")
                            ),
                        ));
                    } else {
                        last = Some(h.line);
                    }
                }
            }
        }
    }
    out
}

// -------------------------------------------------------------------------------------- segment names

/// Names in the first column of latency-budget tables: `(segments, all known names)`.
pub fn segment_names(cx: &Ctx) -> Option<(BTreeSet<String>, BTreeSet<String>)> {
    let d = cx.doc("docs/latency-budget.md")?;
    let mut segs = BTreeSet::new();
    let mut known = BTreeSet::new();
    let sec2 = d.find_head(|h| h.level == 2 && h.text.starts_with("2."))?;
    let (a, b) = d.section(sec2);
    for t in d.tables() {
        let in2 = t.line >= a && t.line < b;
        for (_, c) in &t.rows {
            for m in BACKTICK.captures_iter(c.first().map_or("", |s| s.as_str())) {
                let n = m[1].to_string();
                if !SNAKE.is_match(&n) {
                    continue;
                }
                if in2 && t.header.first().is_some_and(|h| h == "分段") {
                    segs.insert(n.clone());
                }
                known.insert(n);
            }
        }
    }
    Some((segs, known))
}

/// "分段名": a backticked snake_case name that is a known segment name followed directly by more letters or
/// digits (not `_`), and is not itself a name of latency-budget, is a mistyped segment name.
pub fn check_segments(cx: &Ctx) -> Vec<Problem> {
    let Some((segs, known)) = segment_names(cx) else {
        return vec![(
            "SEGMENT",
            "docs/latency-budget.md: §2 segment table not found".to_string(),
        )];
    };
    let name = Regex::new(r"^[a-z][a-z0-9_]*$").unwrap();
    let mut out = Vec::new();
    for d in cx.content() {
        for (i, l) in d.lines.iter().enumerate() {
            if d.code[i] {
                continue;
            }
            for m in BACKTICK.captures_iter(l) {
                let t = &m[1];
                if !name.is_match(t) || known.contains(t) {
                    continue;
                }
                if let Some(s) = segs.iter().find(|s| {
                    t.len() > s.len()
                        && t.starts_with(s.as_str())
                        && t.as_bytes()[s.len()].is_ascii_alphanumeric()
                }) {
                    out.push(("SEGMENT", format!("{}: `{t}` is not a segment name of docs/latency-budget.md §2 (did you mean `{s}`?)", loc(d, i))));
                }
            }
        }
    }
    out
}

// ------------------------------------------------------------------------------------------- events

static VERB: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"产生|产出|写入|写出|写|发出|输出|生成|推入|→").unwrap());

/// "事件" (1): every registered `EventKind` (test kinds excepted) is declared as produced by a component README:
/// a line of a `crates/*`, `apps/*` or `strategies/*` README (not hft-event's own) that names the kind in
/// backticks (optionally with `{…}`) together with a producing verb (产生、产出、写入、写出、写、发出、输出、生成、推入、→).
pub fn check_producers(cx: &Ctx) -> Vec<Problem> {
    let mut globs = Globs::default();
    let files: Vec<&Doc> = cx
        .docs
        .values()
        .filter(|d| {
            (globs.is_match("crates/*/README.md", &d.rel)
                || globs.is_match("apps/*/README.md", &d.rel)
                || globs.is_match("strategies/*/README.md", &d.rel))
                && d.rel != "crates/hft-event/README.md"
        })
        .collect();
    let mut out = Vec::new();
    for k in hft_event::kinds::KINDS {
        if k.domain == hft_event::kinds::Domain::Test {
            continue;
        }
        let rx = Regex::new(&format!(r"`{}(?:\{{[^`]*)?`", regex::escape(k.name))).unwrap();
        let found = files
            .iter()
            .any(|d| d.lines.iter().any(|l| rx.is_match(l) && VERB.is_match(l)));
        if !found {
            out.push((
                "EVENTS",
                format!(
                    "EventKind `{}` (0x{:04X}) is not declared as produced by any component README",
                    k.name, k.kind.0
                ),
            ));
        }
    }
    out
}

/// "事件" (2): the class column of the docs/subsystems/event-model.md kind table equals `KindInfo.role`
/// (`Input`/`Output`/`Meta`; "按体…" = `ByBody`; "raw journal" = `Meta`); every registered kind has a row.
pub fn check_kind_roles(cx: &Ctx) -> Vec<Problem> {
    use hft_event::kinds::{Domain, KINDS, Role};
    let rel = "docs/subsystems/event-model.md";
    let Some(d) = cx.doc(rel) else {
        return vec![("EVENTS", format!("{rel} not found"))];
    };
    let Some(t) = d
        .tables()
        .into_iter()
        .find(|t| t.header.iter().any(|h| h == "kind") && t.header.iter().any(|h| h == "分类"))
    else {
        return vec![(
            "EVENTS",
            format!("{rel}: kind table (| … | kind | … | 分类 | …) not found"),
        )];
    };
    let kc = t.header.iter().position(|h| h == "kind").unwrap_or(0);
    let cc = t.header.iter().position(|h| h == "分类").unwrap_or(0);
    let mut out = Vec::new();
    let mut seen = BTreeSet::new();
    for (ln, c) in &t.rows {
        let Some(name) = BACKTICK
            .captures(c.get(kc).map_or("", |s| s.as_str()))
            .map(|m| m[1].to_string())
        else {
            continue;
        };
        seen.insert(name.clone());
        let class = c.get(cc).map_or("", |s| s.as_str()).replace("**", "");
        let class = class.trim();
        let role = if class.starts_with("按体") {
            Some(Role::ByBody)
        } else if class == "raw journal" || class == "Meta" {
            Some(Role::Meta)
        } else if class == "Input" {
            Some(Role::Input)
        } else if class == "Output" {
            Some(Role::Output)
        } else {
            None
        };
        match KINDS.iter().find(|k| k.name == name) {
            None => out.push((
                "EVENTS",
                format!("{rel}:{}: kind `{name}` is not in the hft-event registry", ln + 1),
            )),
            Some(k) => {
                if role != Some(k.role) {
                    out.push((
                        "EVENTS",
                        format!(
                            "{rel}:{}: `{name}` class `{class}` differs from KindInfo.role {:?}",
                            ln + 1,
                            k.role
                        ),
                    ));
                }
            }
        }
    }
    for k in KINDS {
        if k.domain != Domain::Test && !seen.contains(k.name) {
            out.push((
                "EVENTS",
                format!("{rel}: registered kind `{}` has no row in the kind table", k.name),
            ));
        }
    }
    out
}

static IDENT: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"[A-Z][A-Za-z0-9_]*").unwrap());
static ASSIGN: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"([A-Z][A-Za-z0-9_]*)\s*=\s*\d").unwrap());
static NOTE_USE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(Note|Timer)\{([^{}\n]*)").unwrap());
static KV: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?:code|source|owner)\s*=\s*([A-Za-z_][\w|/]*)").unwrap());
static NAME: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^[A-Z][A-Za-z0-9_]+$").unwrap());

fn ident_ok(prev: Option<char>) -> bool {
    !prev.is_some_and(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// Identifiers of a registry file: every capitalized identifier in its table rows (outside code) plus
/// `Name = <digit>` assignments anywhere.
fn registry_names(d: &Doc) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for (i, l) in d.lines.iter().enumerate() {
        if !d.code[i] && l.trim_start().starts_with('|') {
            let l = l.replace('`', " ");
            for m in IDENT.find_iter(&l) {
                if ident_ok(l[..m.start()].chars().last()) {
                    out.insert(m.as_str().to_string());
                }
            }
        }
    }
    for c in ASSIGN.captures_iter(&d.text) {
        out.insert(c[1].to_string());
    }
    out
}

/// "事件" (3): `Note{X}` / `Timer{X}` names are in the hft-event registry tables or a strategy's
/// `StrategyDesc.notes` (identifiers in the tables of `strategies/*/README.md`, registry excepted).
pub fn check_note_names(cx: &Ctx) -> Vec<Problem> {
    let ev_rel = "crates/hft-event/README.md";
    let Some(ev) = cx.doc(ev_rel) else {
        return vec![("EVENTS", format!("{ev_rel} not found"))];
    };
    let reg = registry_names(ev);
    let head_rx = Regex::new(r"(?m)^\|\s*`(\w+)`\s*\|\s*\d+\s*\|").unwrap();
    let heads: BTreeSet<String> = head_rx
        .captures_iter(&ev.text)
        .map(|c| c[1].to_string())
        .collect();
    let mut globs = Globs::default();
    let mut strat = BTreeSet::new();
    for d in cx.docs.values() {
        if globs.is_match("strategies/*/README.md", &d.rel) && d.rel != "strategies/registry/README.md" {
            strat.extend(registry_names(d));
        }
    }
    let mut out = Vec::new();
    for d in cx.content() {
        if under(&d.rel, ".agents/notes") || under(&d.rel, "docs/postmortem") {
            continue;
        }
        let mut bad: BTreeMap<String, Vec<usize>> = BTreeMap::new();
        for (i, l) in d.lines.iter().enumerate() {
            for c in NOTE_USE.captures_iter(l) {
                let m0 = c.get(0).map_or(0, |m| m.start());
                if !ident_ok(l[..m0].chars().last()) {
                    continue;
                }
                let kind = &c[1];
                let body = c[2].replace("\\|", "|");
                if body.contains(':') {
                    continue;
                }
                let toks: Vec<String> = if body.contains('=') {
                    KV.captures_iter(&body).map(|x| x[1].to_string()).collect()
                } else {
                    let parts: Vec<&str> = body.split(',').map(str::trim).collect();
                    let mut v = vec![parts[0].to_string()];
                    if heads.contains(parts[0]) && parts.len() > 1 {
                        v.push(parts[1].to_string());
                    }
                    v
                };
                for t in toks {
                    for name in t.split(['|', '/']) {
                        let name = name.trim();
                        if !NAME.is_match(name) {
                            continue;
                        }
                        if !reg.contains(name) && !strat.contains(name) {
                            bad.entry(format!("{kind}{{{name}}}")).or_default().push(i);
                        }
                    }
                }
            }
        }
        for (k, ls) in bad {
            out.push((
                "EVENTS",
                format!(
                    "{}: {k} is not in the hft-event registry or a strategy's StrategyDesc.notes",
                    loc(d, ls[0])
                ),
            ));
        }
    }
    out
}

// --------------------------------------------------------------------------------- dependency graph

/// "依赖图": every edge of the crates/AGENTS.md "依赖方向" graph is allowed by `tools/xtask/deps.toml`.
///
/// Graph syntax (one statement per `;`-separated part, parenthesized text ignored, text before `:` ignored):
/// `A ← B ← C` = B depends on A, C on B; `A → X + Y` = A depends on X and Y; `L0`/`L1` = the crates of the
/// line annotated `(L0,…)`/`(L1,…)` (the label followed by a comma or `)`); `venue-<x>` = every `crates/venue-*`; `strategies/<name>` = every strategy;
/// tokens that are not crate names (`按需`, `各`, prose) are ignored.
pub fn check_dep_graph(cx: &Ctx) -> Vec<Problem> {
    let deps = match crate::deps::Config::load(&cx.root) {
        Ok(c) => c,
        Err(e) => return vec![("DEPGRAPH", e.0)],
    };
    let Some(d) = cx.doc("crates/AGENTS.md") else {
        return vec![("DEPGRAPH", "crates/AGENTS.md not found".to_string())];
    };
    let Some(h) = d.find_head(|h| h.text == "依赖方向") else {
        return vec![(
            "DEPGRAPH",
            "crates/AGENTS.md: no `## 依赖方向` section".to_string(),
        )];
    };
    let (a, b) = d.section(h);
    let Some((start, block)) = d.code_blocks().into_iter().find(|(s, _)| *s > a && *s < b) else {
        return vec![(
            "DEPGRAPH",
            "crates/AGENTS.md: 依赖方向 has no code block".to_string(),
        )];
    };
    let paren = Regex::new(r"\([^()]*\)|\x{FF08}[^\x{FF08}\x{FF09}]*\x{FF09}").unwrap();
    let group = Regex::new(r"^[(\x{FF08}]\s*(L\d)\s*[,\x{FF0C})\x{FF09}]").unwrap();
    let token =
        Regex::new(r"strategies/[A-Za-z0-9_<>\-]+|hft-[a-z0-9\-]+|venue-[A-Za-z0-9_<>\-]+|\bL\d\b").unwrap();
    let mut groups: HashMap<String, Vec<String>> = HashMap::new();
    let mut out = Vec::new();
    // Resolve one token to package keys of deps.toml.
    let resolve_tok = |t: &str, groups: &HashMap<String, Vec<String>>| -> Option<Vec<String>> {
        if let Some(g) = groups.get(t) {
            return Some(g.clone());
        }
        if t == "venue-<x>" || t == "venue-<exchange>" {
            return Some(
                deps.pkgs
                    .keys()
                    .filter(|k| k.starts_with("crates/venue-"))
                    .cloned()
                    .collect(),
            );
        }
        if t.starts_with("strategies/<") {
            return Some(vec!["strategies/*".to_string()]);
        }
        if t.starts_with("strategies/") {
            return deps.key_for(t).map(|k| vec![k.to_string()]);
        }
        let p = format!("crates/{t}");
        deps.key_for(&p).map(|k| vec![k.to_string()])
    };
    // First pass: groups (`(L0 …)` annotated lines list their crates).
    for l in &block {
        let ann: Vec<String> = paren.find_iter(l).map(|m| m.as_str().to_string()).collect();
        for a in &ann {
            if let Some(g) = group.captures(a) {
                let bare = paren.replace_all(l, "");
                let names: Vec<String> = token
                    .find_iter(&bare)
                    .filter_map(|m| resolve_tok(m.as_str(), &groups))
                    .flatten()
                    .collect();
                groups.insert(g[1].to_string(), names);
            }
        }
    }
    for (k, l) in block.iter().enumerate() {
        let bare = paren.replace_all(l, " ").into_owned();
        for stmt in bare.split([';', '\u{FF1B}']) {
            let stmt = match stmt.rsplit_once([':', '\u{FF1A}']) {
                Some((_, s)) => s,
                None => stmt,
            };
            let toks = |s: &str| -> Vec<(String, Option<Vec<String>>)> {
                token
                    .find_iter(s)
                    .map(|m| (m.as_str().to_string(), resolve_tok(m.as_str(), &groups)))
                    .collect()
            };
            let mut edges: Vec<(String, String)> = Vec::new();
            let mut unknown = Vec::new();
            if stmt.contains('←') {
                let parts: Vec<_> = stmt.split('←').map(toks).collect();
                for w in parts.windows(2) {
                    for (to, tr) in &w[0] {
                        for (from, fr) in &w[1] {
                            match (fr, tr) {
                                (Some(f), Some(t)) => edges.extend(
                                    f.iter()
                                        .flat_map(|a| t.iter().map(move |b| (a.clone(), b.clone()))),
                                ),
                                _ => unknown.extend([from.clone(), to.clone()]),
                            }
                        }
                    }
                }
            } else if let Some((lhs, rhs)) = stmt.split_once('→') {
                let lt = toks(lhs);
                for part in rhs.split('+') {
                    for (to, tr) in toks(part) {
                        for (from, fr) in &lt {
                            match (fr, &tr) {
                                (Some(f), Some(t)) => edges.extend(
                                    f.iter()
                                        .flat_map(|a| t.iter().map(move |b| (a.clone(), b.clone()))),
                                ),
                                _ => unknown.extend([from.clone(), to.clone()]),
                            }
                        }
                    }
                }
            }
            for u in unknown {
                if resolve_tok(&u, &groups).is_none() && !u.starts_with('L') {
                    out.push((
                        "DEPGRAPH",
                        format!(
                            "crates/AGENTS.md:{}: `{u}` is not a package of {}",
                            start + k + 1,
                            crate::deps::CONFIG
                        ),
                    ));
                }
            }
            for (from, to) in edges {
                if from == to {
                    continue;
                }
                let allowed = deps.pkgs.get(&from).is_some_and(|r| {
                    r.deps
                        .iter()
                        .any(|x| crate::deps::pattern_match(x, &to) || *x == to)
                });
                if !allowed {
                    out.push((
                        "DEPGRAPH",
                        format!(
                            "crates/AGENTS.md:{}: edge `{from} → {to}` is not allowed by {}",
                            start + k + 1,
                            crate::deps::CONFIG
                        ),
                    ));
                }
            }
        }
    }
    out.sort();
    out.dedup();
    out
}

/// Tables helper for other modules.
pub fn tables_under(d: &Doc, heading: &str) -> Vec<Table> {
    match d.find_head(|h| h.text == heading) {
        Some(h) => {
            let (a, b) = d.section(h);
            d.tables_in(a, b)
        }
        None => Vec::new(),
    }
}
