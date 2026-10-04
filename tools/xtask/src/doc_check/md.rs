//! Markdown reading for the native doc-check: frontmatter, headings and anchors, code fences, tables,
//! links and sections. Rules follow `tools/doccheck/check_hft.py` (same anchor and fence handling).

use std::collections::{BTreeMap, HashMap};
use std::sync::LazyLock;

use regex::Regex;

use super::slugs;

static HEAD: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^(#{1,6})\s+(.*?)\s*#*\s*$").unwrap());
static HTML_ANCHOR: LazyLock<Regex> = LazyLock::new(|| Regex::new(r#"<a\s+(?:id|name)="([^"]+)""#).unwrap());
static LINK: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\[[^\]]*\]\(([^)\s]+)\)").unwrap());

/// One heading.
#[derive(Debug, Clone)]
pub struct Head {
    pub level: usize,
    pub text: String,
    /// 0-based line index.
    pub line: usize,
}

/// One Markdown table: header cells and rows `(0-based line, cells)`.
#[derive(Debug, Clone)]
pub struct Table {
    pub line: usize,
    pub header: Vec<String>,
    pub rows: Vec<(usize, Vec<String>)>,
}

/// A parsed Markdown file.
#[derive(Debug, Clone)]
pub struct Doc {
    /// Path relative to the repo root (`/` separators).
    pub rel: String,
    pub text: String,
    pub lines: Vec<String>,
    /// `code[i]`: line i is a fence line or inside a fenced block.
    pub code: Vec<bool>,
    pub fm: Option<BTreeMap<String, String>>,
    pub heads: Vec<Head>,
}

/// Split a table row on unescaped `|`.
pub fn cells(line: &str) -> Vec<String> {
    let t = line.trim();
    let t = t.strip_prefix('|').unwrap_or(t);
    let t = if t.ends_with('|') && !t.ends_with("\\|") {
        &t[..t.len() - 1]
    } else {
        t
    };
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut prev = '\0';
    for ch in t.chars() {
        if ch == '|' && prev != '\\' {
            out.push(cur.trim().to_string());
            cur.clear();
        } else {
            cur.push(ch);
        }
        prev = ch;
    }
    out.push(cur.trim().to_string());
    out
}

fn is_rule_row(line: &str) -> bool {
    let c = cells(line);
    !c.is_empty()
        && c.iter()
            .all(|x| !x.is_empty() && x.chars().all(|ch| matches!(ch, '-' | ':' | ' ')))
}

impl Doc {
    pub fn new(rel: &str, text: String) -> Doc {
        let lines: Vec<String> = text.lines().map(str::to_string).collect();
        let mut code = vec![false; lines.len()];
        let mut in_code = false;
        for (i, l) in lines.iter().enumerate() {
            let st = l.trim_start();
            if st.starts_with("```") || st.starts_with("~~~") {
                code[i] = true;
                in_code = !in_code;
                continue;
            }
            code[i] = in_code;
        }
        // Frontmatter: a leading `---` block.
        let mut fm = None;
        let mut fm_end = 0;
        if lines.first().is_some_and(|l| l.trim() == "---")
            && let Some(end) = lines.iter().skip(1).position(|l| l.trim() == "---")
        {
            let mut m = BTreeMap::new();
            for l in &lines[1..=end] {
                if let Some((k, v)) = l.split_once(':') {
                    let k = k.trim();
                    if !k.is_empty() && !k.starts_with('#') && !l.starts_with(' ') {
                        let v = v.trim().trim_matches('"').trim_matches('\'').to_string();
                        m.insert(k.to_string(), v);
                    }
                }
            }
            fm = Some(m);
            fm_end = end + 2;
            for c in code.iter_mut().take(fm_end) {
                *c = true;
            }
        }
        let mut heads = Vec::new();
        for (i, l) in lines.iter().enumerate() {
            if i < fm_end || code[i] {
                continue;
            }
            if let Some(c) = HEAD.captures(l) {
                heads.push(Head {
                    level: c[1].len(),
                    text: c[2].to_string(),
                    line: i,
                });
            }
        }
        Doc {
            rel: rel.to_string(),
            text,
            lines,
            code,
            fm,
            heads,
        }
    }

    /// Frontmatter value.
    pub fn fm_get(&self, k: &str) -> Option<&str> {
        self.fm.as_ref().and_then(|m| m.get(k)).map(String::as_str)
    }

    /// GitHub anchors of this file (lowercase) plus explicit `<a id/name>` anchors.
    pub fn anchors(&self) -> Vec<String> {
        let mut out = slugs(self.heads.iter().map(|h| h.text.as_str()));
        out.extend(HTML_ANCHOR.captures_iter(&self.text).map(|c| c[1].to_string()));
        out.into_iter().map(|a| a.to_lowercase()).collect()
    }

    /// Line range `(start, end)` of the section headed by `heads[i]` (heading line excluded): up to the
    /// next heading of the same or a higher level.
    pub fn section(&self, i: usize) -> (usize, usize) {
        let h = &self.heads[i];
        let end = self.heads[i + 1..]
            .iter()
            .find(|x| x.level <= h.level)
            .map_or(self.lines.len(), |x| x.line);
        (h.line + 1, end)
    }

    /// Index of the first heading whose text satisfies `f`.
    pub fn find_head(&self, f: impl Fn(&Head) -> bool) -> Option<usize> {
        self.heads.iter().position(f)
    }

    /// Text of lines `[a, b)`.
    pub fn text_of(&self, a: usize, b: usize) -> String {
        self.lines[a..b.min(self.lines.len())].join("\n")
    }

    /// Enclosing heading texts (outermost first) for each line.
    pub fn enclosing(&self) -> Vec<Vec<String>> {
        let mut stack: Vec<(usize, String)> = Vec::new();
        let mut hi = 0;
        let mut out = Vec::with_capacity(self.lines.len());
        for i in 0..self.lines.len() {
            if hi < self.heads.len() && self.heads[hi].line == i {
                let h = &self.heads[hi];
                stack.retain(|(lv, _)| *lv < h.level);
                stack.push((h.level, h.text.clone()));
                hi += 1;
            }
            out.push(stack.iter().map(|(_, t)| t.clone()).collect());
        }
        out
    }

    /// Tables outside code blocks within lines `[a, b)`.
    pub fn tables_in(&self, a: usize, b: usize) -> Vec<Table> {
        let mut out = Vec::new();
        let mut i = a;
        let b = b.min(self.lines.len());
        while i + 1 < b {
            let l = self.lines[i].trim_start();
            if !self.code[i] && l.starts_with('|') && is_rule_row(&self.lines[i + 1]) {
                let header = cells(l);
                let mut rows = Vec::new();
                let mut j = i + 2;
                while j < b && !self.code[j] && self.lines[j].trim_start().starts_with('|') {
                    rows.push((j, cells(&self.lines[j])));
                    j += 1;
                }
                out.push(Table {
                    line: i,
                    header,
                    rows,
                });
                i = j;
            } else {
                i += 1;
            }
        }
        out
    }

    pub fn tables(&self) -> Vec<Table> {
        self.tables_in(0, self.lines.len())
    }

    /// Relative link targets outside code blocks: `(0-based line, target)`.
    pub fn links(&self) -> Vec<(usize, String)> {
        let mut out = Vec::new();
        for (i, l) in self.lines.iter().enumerate() {
            if self.code[i] {
                continue;
            }
            for c in LINK.captures_iter(l) {
                out.push((i, c[1].to_string()));
            }
        }
        out
    }

    /// Fenced code blocks: `(0-based first content line, lines)`.
    pub fn code_blocks(&self) -> Vec<(usize, Vec<String>)> {
        let mut out = Vec::new();
        let mut cur: Option<(usize, Vec<String>)> = None;
        for (i, l) in self.lines.iter().enumerate() {
            let st = l.trim_start();
            if self.code[i] && (st.starts_with("```") || st.starts_with("~~~")) {
                match cur.take() {
                    Some(b) => out.push(b),
                    None => cur = Some((i + 1, Vec::new())),
                }
                continue;
            }
            if let Some((_, v)) = cur.as_mut() {
                v.push(l.clone());
            }
        }
        out
    }
}

/// Resolve `target` (relative to the directory of `from`, both repo-relative) to a normalized repo-relative
/// path; `None` if it climbs above the root.
pub fn resolve(from: &str, target: &str) -> Option<String> {
    let mut parts: Vec<&str> = from.split('/').collect();
    parts.pop();
    for seg in target.split('/') {
        match seg {
            "" | "." => {}
            ".." => {
                parts.pop()?;
            }
            s => parts.push(s),
        }
    }
    Some(parts.join("/"))
}

/// Glob over repo-relative paths: `*` = one segment part, `**/` = any number of directories.
pub fn glob(pat: &str) -> Regex {
    let mut r = String::from("^");
    let mut rest = pat;
    while !rest.is_empty() {
        if let Some(x) = rest.strip_prefix("**/") {
            r += "(?:[^/]+/)*";
            rest = x;
        } else if let Some(x) = rest.strip_prefix('*') {
            r += "[^/]*";
            rest = x;
        } else {
            let ch = rest.chars().next().unwrap_or(' ');
            r += &regex::escape(&ch.to_string());
            rest = &rest[ch.len_utf8()..];
        }
    }
    r += "$";
    Regex::new(&r).unwrap_or_else(|_| Regex::new("^$").unwrap())
}

/// Cache of compiled globs.
#[derive(Default, Debug)]
pub struct Globs(HashMap<String, Regex>);

impl Globs {
    pub fn is_match(&mut self, pat: &str, path: &str) -> bool {
        self.0
            .entry(pat.to_string())
            .or_insert_with(|| glob(pat))
            .is_match(path)
    }
}
