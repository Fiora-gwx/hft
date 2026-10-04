//! "码值与布局": Rust code blocks in Markdown. A `#[repr(C)]` struct's integer fields named `*kind`/`*level`/
//! `*purpose`/`*role`/`*mode`/`*status`/`*reason`/`*code` carry a code value (`=`) or a code-table pointer in
//! their comment; a struct whose header comment (or a `size_of` assertion) declares a size in bytes has exactly
//! that `#[repr(C)]` layout size. Type sizes are resolved across all Markdown files (primitives, arrays, ID
//! newtypes, aliases, `#[repr(uN)]` enums, nested structs). Port of `tools/doccheck/check_hft.py` ④/⑤.

use std::collections::{BTreeSet, HashMap};
use std::sync::LazyLock;

use regex::Regex;

use super::md::Doc;
use super::native::{Ctx, Problem};

const TYPE: &str = r"(?:\[(?:[^\[\]]|\[[^\[\]]*\])*\]|[A-Za-z_][\w:]*(?:<(?:[^<>]|<[^<>]*>)*>)?)";

static FIELD: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(&format!(
        r"^\s*(?:\x00\d+\x00\s*)*(?:#\[[^\]]*\]\s*)*(?:pub(?:\([^)]*\))?\s+)?([A-Za-z_]\w*)\s*:\s*({TYPE})\s*((?:\x00\d+\x00\s*)*)$"
    ))
    .unwrap()
});
static STRUCT: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\bstruct\s+([A-Z]\w*)\s*(<[^>{]*>)?\s*\{").unwrap());
static NEWTYPE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(&format!(
        r"\bstruct\s+([A-Z]\w*)\s*\(\s*(?:pub(?:\([^)]*\))?\s+)?({TYPE})\s*\)\s*;"
    ))
    .unwrap()
});
static ALIAS: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(&format!(r"\btype\s+([A-Z]\w*)\s*=\s*({TYPE})\s*;")).unwrap());
static ENUM_DATA: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"#\[repr\(\s*((?:C\s*,\s*)?(?:u8|u16|u32|u64|i8|i16|i32|i64)(?:\s*,\s*C)?)\s*\)\]\s*(?:#\[[^\]]*\]\s*)*(?:pub(?:\([^)]*\))?\s+)?enum\s+([A-Z]\w*)\s*\{((?:[^{}]|\{[^{}]*\})*)\}",
    )
    .unwrap()
});
static PAYLOAD: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\b[A-Z]\w*\s*\(([^()]*)\)").unwrap());
static ENUM: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"#\[repr\((u8|u16|u32|u64|i8|i16|i32|i64)\)\]\s*(?:#\[[^\]]*\]\s*)*(?:pub(?:\([^)]*\))?\s+)?enum\s+([A-Z]\w*)").unwrap()
});
static ENUM_PLAIN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(^|[^\]])\s(?:pub(?:\([^)]*\))?\s+)?enum\s+([A-Z]\w*)\s*\{([^{}]*)\}").unwrap()
});
static DISCR: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"=\s*(\d+)").unwrap());
static CONST: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\bconst\s+([A-Z][A-Z0-9_]*)\s*:\s*\w+\s*=\s*([\d_]+)\s*;").unwrap());
static SIZEOF: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"size_of::<\s*([A-Z]\w*)\s*>\(\)\s*==\s*(\d[\d_]*)").unwrap());
static DECL: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(\d+)\s*B").unwrap());
static REPR: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"#\[repr\(((?:[^()]|\([^()]*\))*)\)\]").unwrap());
static TABLE_NEWTYPE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?m)^\|\s*`([A-Z]\w*)`\s*\|\s*`?(u8|u16|u32|u64|u128|i8|i16|i32|i64|i128)`?\s*\|").unwrap()
});
static CODE_FIELD: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(kind|level|purpose|role|mode|status|reason|code)$").unwrap());
/// What counts as a code value or code-table pointer in a field comment (tools/README.md "码值与布局").
static CODE_PTR: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"=|#|§|\]\(|码表|码值|登记|见|自定义|\d+\s*[A-Za-z\x{4E00}-\x{9FFF}]\w*\s*/\s*\d+|同\s*`?[A-Z]\w*",
    )
    .unwrap()
});
static BOUND_BEFORE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"[≤<>≥]\s*$").unwrap());
static INT_TAG: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"[ui]\d+").unwrap());
static PLACEHOLDER: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\x00(\d+)\x00").unwrap());
static ALIGN: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"align\((\d+)\)").unwrap());
static REPR_SPLIT: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"[,\s]+").unwrap());

fn prim(t: &str) -> Option<(u64, u64)> {
    Some(match t {
        "u8" | "i8" | "bool" | "AtomicU8" | "AtomicBool" | "NonZeroU8" => (1, 1),
        "u16" | "i16" | "AtomicU16" | "NonZeroU16" => (2, 2),
        "u32" | "i32" | "f32" | "char" | "AtomicU32" | "AtomicI32" | "NonZeroU32" => (4, 4),
        "u64" | "i64" | "f64" | "usize" | "isize" | "AtomicU64" | "AtomicI64" | "NonZeroU64" => (8, 8),
        "u128" | "i128" => (16, 16),
        _ => return None,
    })
}

const INT_TYPES: &[&str] = &[
    "u8", "u16", "u32", "u64", "i8", "i16", "i32", "i64", "u128", "i128",
];

/// One comment-scanned code line.
struct ScLine {
    /// Code with block comments replaced by `\0<n>\0` placeholders, line comment removed.
    code: String,
    blocks: Vec<String>,
    line_comment: String,
    /// Code with every comment blanked (spaces), same byte length as the line.
    blank: String,
}

fn scan_comments(lines: &[String]) -> Vec<ScLine> {
    let mut res = Vec::new();
    let mut in_block = false;
    for line in lines {
        let (mut code, mut blank, mut blocks, mut lc) =
            (String::new(), String::new(), Vec::new(), String::new());
        let mut cur: Vec<String> = Vec::new();
        let n = line.len();
        let mut j = 0;
        while j < n {
            let rest = &line[j..];
            if in_block {
                match rest.find("*/") {
                    None => {
                        cur.push(rest.to_string());
                        blank += &" ".repeat(n - j);
                        j = n;
                    }
                    Some(k) => {
                        cur.push(rest[..k].to_string());
                        blank += &" ".repeat(k + 2);
                        blocks.push(cur.join(" "));
                        code += &format!("\0{}\0", blocks.len() - 1);
                        cur.clear();
                        in_block = false;
                        j += k + 2;
                    }
                }
                continue;
            }
            if let Some(c) = rest.strip_prefix("//") {
                lc = c.to_string();
                blank += &" ".repeat(n - j);
                break;
            }
            if rest.starts_with("/*") {
                in_block = true;
                blank += "  ";
                j += 2;
                continue;
            }
            let ch = rest.chars().next().unwrap_or(' ');
            code.push(ch);
            blank.push(ch);
            j += ch.len_utf8();
        }
        if in_block && !cur.is_empty() {
            blocks.push(cur.join(" "));
        }
        res.push(ScLine {
            code,
            blocks,
            line_comment: lc,
            blank,
        });
    }
    res
}

fn split_top(s: &str, sep: char) -> Vec<String> {
    let (mut out, mut depth, mut cur) = (Vec::new(), 0i32, String::new());
    for ch in s.chars() {
        if "[(<{".contains(ch) {
            depth += 1;
        } else if "])>}".contains(ch) {
            depth -= 1;
        }
        if ch == sep && depth == 0 {
            out.push(std::mem::take(&mut cur));
        } else {
            cur.push(ch);
        }
    }
    out.push(cur);
    out
}

/// Declared sizes in a comment (`N B`, not a bound `≤ N B`, not a variable body `N B + …`).
fn decl_sizes(c: &str) -> BTreeSet<u64> {
    let mut out = BTreeSet::new();
    for m in DECL.find_iter(c) {
        let prev = c[..m.start()].chars().last();
        if prev.is_some_and(|p| p.is_alphanumeric() || p == '_' || p == '.') {
            continue;
        }
        let after = &c[m.end()..];
        if after.chars().next().is_some_and(|x| x.is_ascii_alphabetic())
            || after.trim_start().starts_with('+')
        {
            continue;
        }
        let before: String = c[..m.start()]
            .chars()
            .rev()
            .take(3)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect();
        if BOUND_BEFORE.is_match(&before) {
            continue;
        }
        if let Some(v) = DECL.captures(m.as_str()).and_then(|x| x[1].parse().ok()) {
            out.insert(v);
        }
    }
    out
}

#[derive(Default)]
struct Defs {
    local: HashMap<String, HashMap<String, (u64, u64)>>,
    glob: HashMap<String, BTreeSet<(u64, u64)>>,
    consts_local: HashMap<String, HashMap<String, u64>>,
    consts_glob: HashMap<String, BTreeSet<u64>>,
    enums: BTreeSet<String>,
}

impl Defs {
    fn put(&mut self, f: &str, name: &str, sa: Option<(u64, u64)>) -> bool {
        let Some(sa) = sa else { return false };
        let l = self.local.entry(f.to_string()).or_default();
        if l.get(name) == Some(&sa) {
            return false;
        }
        l.insert(name.to_string(), sa);
        self.glob.entry(name.to_string()).or_default().insert(sa);
        true
    }
    fn get(&self, f: &str, name: &str) -> Option<(u64, u64)> {
        if let Some(p) = prim(name) {
            return Some(p);
        }
        if let Some(x) = self.local.get(f).and_then(|m| m.get(name)) {
            return Some(*x);
        }
        let g = self.glob.get(name)?;
        (g.len() == 1).then(|| *g.iter().next().unwrap())
    }
    fn konst(&self, f: &str, name: &str) -> Option<u64> {
        if let Some(x) = self.consts_local.get(f).and_then(|m| m.get(name)) {
            return Some(*x);
        }
        let g = self.consts_glob.get(name)?;
        (g.len() == 1).then(|| *g.iter().next().unwrap())
    }
    fn eval_len(&self, e: &str, f: &str) -> Option<u64> {
        let e = e.trim();
        if !e.is_empty() && e.chars().all(|c| c.is_ascii_digit() || c == '_') {
            return e.replace('_', "").parse().ok();
        }
        if Regex::new(r"^[A-Z][A-Z0-9_]*$").unwrap().is_match(e) {
            return self.konst(f, e);
        }
        let c = Regex::new(r"^(\w+)\s*([*+])\s*(\w+)$").unwrap().captures(e)?;
        let (a, b) = (self.eval_len(&c[1], f)?, self.eval_len(&c[3], f)?);
        Some(if &c[2] == "*" { a * b } else { a + b })
    }
    fn type_size(&self, t: &str, f: &str) -> Option<(u64, u64)> {
        let t = t.trim();
        if t.starts_with('[') && t.ends_with(']') {
            let parts = split_top(&t[1..t.len() - 1], ';');
            if parts.len() != 2 {
                return None;
            }
            let inner = self.type_size(&parts[0], f)?;
            let n = self.eval_len(&parts[1], f)?;
            return Some((inner.0 * n, inner.1));
        }
        if t.contains('<') || t.contains('&') || t.contains('*') {
            return None;
        }
        self.get(f, t.rsplit("::").next().unwrap_or(t))
    }
}

struct Field {
    name: String,
    ty: String,
    line: usize,
    comment: String,
}

enum Def {
    Struct {
        file: String,
        line: usize,
        name: String,
        fields: Vec<Field>,
        repr: String,
        decl: BTreeSet<u64>,
        block_repr: bool,
    },
    NewType {
        file: String,
        name: String,
        ty: String,
    },
    Alias {
        file: String,
        name: String,
        ty: String,
    },
    EnumData {
        file: String,
        name: String,
        tag: String,
        payloads: Vec<Vec<String>>,
    },
}

fn layout(d: &Defs, file: &str, fields: &[Field], repr: &str) -> Option<(u64, u64, u64)> {
    if fields.is_empty() {
        return None;
    }
    let packed = repr.contains("packed");
    let (mut off, mut mx, mut plain) = (0u64, 1u64, 0u64);
    for fd in fields {
        let (sz, mut al) = d.type_size(&fd.ty, file)?;
        if packed {
            al = 1;
        }
        off = off.div_ceil(al) * al + sz;
        mx = mx.max(al);
        plain += sz;
    }
    if let Some(c) = ALIGN.captures(repr) {
        mx = mx.max(c[1].parse().unwrap_or(1));
    }
    Some((off.div_ceil(mx) * mx, mx, plain))
}

fn enum_layout(d: &Defs, file: &str, tag: &str, payloads: &[Vec<String>]) -> Option<(u64, u64)> {
    let (ts, ta) = prim(tag)?;
    let (mut umax, mut ual) = (0u64, 1u64);
    for pl in payloads {
        let (mut off, mut al) = (0u64, 1u64);
        for t in pl {
            let (s, a) = d.type_size(t, file)?;
            off = off.div_ceil(a) * a + s;
            al = al.max(a);
        }
        umax = umax.max(off.div_ceil(al) * al);
        ual = ual.max(al);
    }
    let al = ta.max(ual);
    Some(((ts.div_ceil(ual) * ual + umax).div_ceil(al) * al, al))
}

fn parse_doc(doc: &Doc, d: &mut Defs, defs: &mut Vec<Def>) {
    let p = doc.rel.as_str();
    for c in CONST.captures_iter(&doc.text) {
        let v: u64 = c[2].replace('_', "").parse().unwrap_or(0);
        d.consts_local
            .entry(p.to_string())
            .or_default()
            .insert(c[1].to_string(), v);
        d.consts_glob.entry(c[1].to_string()).or_default().insert(v);
    }
    let mut asserts: HashMap<String, BTreeSet<u64>> = HashMap::new();
    for c in SIZEOF.captures_iter(&doc.text) {
        asserts
            .entry(c[1].to_string())
            .or_default()
            .insert(c[2].replace('_', "").parse().unwrap_or(0));
    }
    for c in TABLE_NEWTYPE.captures_iter(&doc.text) {
        d.put(p, &c[1], prim(&c[2]));
    }
    for (start, lines) in doc.code_blocks() {
        let sc = scan_comments(&lines);
        let blank = sc.iter().map(|x| x.blank.as_str()).collect::<Vec<_>>().join("\n");
        let mut offs = vec![0usize];
        for x in &sc {
            offs.push(offs.last().unwrap() + x.blank.len() + 1);
        }
        let line_of = |pos: usize| -> usize { offs.partition_point(|&o| o <= pos).saturating_sub(1) };
        let block_repr = blank.contains("#[repr(C");
        let mut data_enums = BTreeSet::new();
        for m in ENUM_DATA.captures_iter(&blank) {
            let payloads: Vec<Vec<String>> = PAYLOAD
                .captures_iter(&m[3])
                .map(|pl| {
                    pl[1]
                        .split(',')
                        .map(|x| x.trim().to_string())
                        .filter(|x| !x.is_empty())
                        .collect()
                })
                .collect();
            if !payloads.is_empty() {
                let tag = INT_TAG.find(&m[1]).map_or("u8", |x| x.as_str()).to_string();
                data_enums.insert(m[2].to_string());
                d.enums.insert(m[2].to_string());
                defs.push(Def::EnumData {
                    file: p.to_string(),
                    name: m[2].to_string(),
                    tag,
                    payloads,
                });
            }
        }
        for m in ENUM.captures_iter(&blank) {
            if data_enums.contains(&m[2]) {
                continue;
            }
            d.enums.insert(m[2].to_string());
            d.put(p, &m[2], prim(&m[1]));
        }
        let nl = format!("\n{blank}");
        for m in ENUM_PLAIN.captures_iter(&nl) {
            let vals: Vec<u64> = DISCR
                .captures_iter(&m[3])
                .filter_map(|x| x[1].parse().ok())
                .collect();
            let name = m[2].to_string();
            let known_local = d.local.get(p).is_some_and(|l| l.contains_key(&name));
            if !vals.is_empty()
                && vals.iter().max().is_some_and(|&x| x < 256)
                && !known_local
                && !data_enums.contains(&name)
            {
                d.enums.insert(name.clone());
                d.put(p, &name, Some((1, 1)));
            }
        }
        for m in STRUCT.captures_iter(&blank) {
            if m.get(2).is_some() {
                continue;
            }
            let name = m[1].to_string();
            let whole = m.get(0).unwrap();
            let ob = whole.end() - 1;
            let bb = blank.as_bytes();
            let (mut depth, mut cb) = (0i32, None);
            for (k, &ch) in bb.iter().enumerate().skip(ob) {
                if ch == b'{' {
                    depth += 1;
                } else if ch == b'}' {
                    depth -= 1;
                    if depth == 0 {
                        cb = Some(k);
                        break;
                    }
                }
            }
            let Some(cb) = cb else { continue };
            let pre = &blank[..whole.start()];
            let prev_end = pre
                .rfind(';')
                .into_iter()
                .chain(pre.rfind('}'))
                .max()
                .map_or(0, |x| x + 1);
            let repr = REPR
                .captures_iter(&blank[prev_end..whole.start()])
                .map(|c| c[1].to_string())
                .collect::<Vec<_>>()
                .join(" ");
            let (l0, l1) = (line_of(ob), line_of(cb));
            let mut hdr_c = sc[l0].line_comment.clone();
            if l0 != l1 {
                hdr_c = format!("{hdr_c} {}", sc[l0].blocks.join(" "));
            }
            let close_c = &sc[l1].line_comment;
            let mut decl = decl_sizes(&hdr_c);
            decl.extend(decl_sizes(close_c));
            if let Some(a) = asserts.get(&name) {
                decl.extend(a);
            }
            let head_rx = Regex::new(&format!(r"\bstruct\s+{}\b[^{{]*\{{", regex::escape(&name))).unwrap();
            let mut fields = Vec::new();
            let mut pending: Vec<String> = Vec::new();
            #[allow(clippy::needless_range_loop)]
            for li in l0..=l1 {
                let ScLine {
                    code,
                    blocks,
                    line_comment: lc,
                    ..
                } = &sc[li];
                let mut a = 0;
                if li == l0 {
                    a = head_rx
                        .find(code)
                        .map_or_else(|| code.find('{').map_or(0, |x| x + 1), |m| m.end());
                }
                let mut b = code.len();
                if li == l1 {
                    let mut depth = 1;
                    for (k, ch) in code.char_indices().filter(|(k, _)| *k >= a) {
                        if ch == '{' {
                            depth += 1;
                        } else if ch == '}' {
                            depth -= 1;
                            if depth == 0 {
                                b = k;
                                break;
                            }
                        }
                    }
                }
                let seg = if a <= b && b <= code.len() {
                    &code[a..b]
                } else {
                    ""
                };
                let interior = l0 < li && li < l1;
                let line_comment = if interior { lc.as_str() } else { "" };
                let stripped: String = seg.replace('\0', "");
                if stripped
                    .trim_matches(|c: char| " \t,0123456789".contains(c))
                    .is_empty()
                {
                    if !lc.trim().is_empty() && interior {
                        pending.push(lc.clone());
                    }
                    continue;
                }
                let mut got = false;
                for piece in split_top(seg, ',') {
                    let Some(fm) = FIELD.captures(&piece) else {
                        continue;
                    };
                    got = true;
                    let bc = PLACEHOLDER
                        .captures_iter(&piece)
                        .filter_map(|x| x[1].parse::<usize>().ok())
                        .filter_map(|x| blocks.get(x).cloned())
                        .collect::<Vec<_>>()
                        .join(" ");
                    let mut parts = vec![bc, line_comment.to_string()];
                    parts.extend(pending.iter().cloned());
                    fields.push(Field {
                        name: fm[1].to_string(),
                        ty: fm[2].trim().to_string(),
                        line: start + li,
                        comment: parts.join(" ").trim().to_string(),
                    });
                }
                if got {
                    pending.clear();
                }
            }
            defs.push(Def::Struct {
                file: p.to_string(),
                line: start + l0,
                name,
                fields,
                repr,
                decl,
                block_repr,
            });
        }
        for m in NEWTYPE.captures_iter(&blank) {
            defs.push(Def::NewType {
                file: p.to_string(),
                name: m[1].to_string(),
                ty: m[2].to_string(),
            });
        }
        for m in ALIAS.captures_iter(&blank) {
            defs.push(Def::Alias {
                file: p.to_string(),
                name: m[1].to_string(),
                ty: m[2].to_string(),
            });
        }
    }
}

/// The "码值与布局" check over every Markdown file of the tree (types from all files; findings only in
/// content files).
pub fn check_layout(cx: &Ctx) -> Vec<Problem> {
    let mut d = Defs::default();
    let mut defs = Vec::new();
    for doc in cx.docs.values() {
        parse_doc(doc, &mut d, &mut defs);
    }
    for _ in 0..10 {
        let mut changed = false;
        for x in &defs {
            changed |= match x {
                Def::NewType { file, name, ty } | Def::Alias { file, name, ty } => {
                    let s = d.type_size(ty, file);
                    d.put(file, name, s)
                }
                Def::EnumData {
                    file,
                    name,
                    tag,
                    payloads,
                } => {
                    let s = enum_layout(&d, file, tag, payloads);
                    d.put(file, name, s)
                }
                Def::Struct {
                    file,
                    name,
                    fields,
                    repr,
                    ..
                } => {
                    let s = layout(&d, file, fields, repr).map(|(a, b, _)| (a, b));
                    d.put(file, name, s)
                }
            };
        }
        if !changed {
            break;
        }
    }
    let mut out = Vec::new();
    let content: BTreeSet<&str> = cx
        .content()
        .map(|x| x.rel.as_str())
        .filter(|f| !f.starts_with(".agents/notes/") && !f.starts_with("docs/postmortem/"))
        .collect();
    for x in &defs {
        let Def::Struct {
            file,
            line,
            name,
            fields,
            repr,
            decl,
            block_repr,
        } = x
        else {
            continue;
        };
        if !content.contains(file.as_str()) {
            continue;
        }
        let own_c = REPR_SPLIT.split(repr).any(|t| t == "C");
        if own_c || *block_repr || !decl.is_empty() {
            for f in fields {
                if f.name.starts_with('_')
                    || !INT_TYPES.contains(&f.ty.as_str())
                    || !CODE_FIELD.is_match(&f.name)
                {
                    continue;
                }
                if CODE_PTR.is_match(&f.comment) || d.enums.iter().any(|e| f.comment.contains(e.as_str())) {
                    continue;
                }
                out.push((
                    "CODEVAL",
                    format!("{file}:{}: {name}.{}: {} — comment has no code value (`=`) or code-table pointer: {:?}", f.line + 1, f.name, f.ty, f.comment.chars().take(60).collect::<String>()),
                ));
            }
        }
        if decl.is_empty() {
            continue;
        }
        if let Some((size, al, plain)) = layout(&d, file, fields, repr)
            && !decl.contains(&size)
        {
            out.push((
                    "LAYOUT",
                    format!(
                        "{file}:{}: {name} declared {} B, fields sum {plain} B, repr(C) layout {size} B (align {al})",
                        line + 1,
                        decl.iter().map(u64::to_string).collect::<Vec<_>>().join("/")
                    ),
                ));
        }
    }
    out
}
