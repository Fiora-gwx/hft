//! Canonical schema JSON and `schema_digest` (README "schema JSON 与 define_params!").

use core::fmt::Write as _;

use hft_event::NoteLevel;
use sha2::{Digest, Sha256};

use super::json::push_str;
use super::text::format_value;
use super::{FieldRef, MinorChange, Relax, SchemaView, UnitKind};
use crate::StrategyDesc;
use crate::desc::{ReportAgg, ReportScope};
use crate::layout::ModelLayoutDesc;

fn relax_name(r: Relax) -> &'static str {
    match r {
        Relax::Up => "up",
        Relax::Down => "down",
        Relax::None => "none",
    }
}

pub(crate) fn minor_text(m: MinorChange) -> String {
    match m {
        MinorChange::None => "none".to_string(),
        MinorChange::Any => "any".to_string(),
        MinorChange::RelPct(n) => format!("rel_pct:{n}"),
        MinorChange::Abs(v) => format!("abs:{v}"),
    }
}

fn level_name(l: NoteLevel) -> &'static str {
    match l {
        NoteLevel::P0 => "p0",
        NoteLevel::P1 => "p1",
        NoteLevel::P2 => "p2",
        NoteLevel::Info => "info",
    }
}

fn agg_name(a: ReportAgg) -> &'static str {
    match a {
        ReportAgg::Count => "count",
        ReportAgg::Sum => "sum",
        ReportAgg::Last => "last",
        ReportAgg::Max => "max",
        ReportAgg::P99 => "p99",
    }
}

fn scope_name(s: ReportScope) -> &'static str {
    match s {
        ReportScope::Instance => "instance",
        ReportScope::Group => "group",
    }
}

fn push_key(out: &mut String, first: &mut bool, key: &str) {
    if !*first {
        out.push(',');
    }
    *first = false;
    push_str(out, key);
    out.push(':');
}

fn push_names(out: &mut String, f: &FieldRef<'_>) {
    out.push('[');
    for i in 0..f.enum_names.len() {
        if i > 0 {
            out.push(',');
        }
        push_str(out, f.enum_names.get(i).unwrap_or(""));
    }
    out.push(']');
}

/// One element of `fields` (parameter or command field). `full = false` writes only the
/// encoding-relevant keys (digest input).
fn push_field(out: &mut String, f: &FieldRef<'_>, full: bool) {
    let mut first = true;
    out.push('{');
    push_key(out, &mut first, "ix");
    let _ = write!(out, "{}", f.ix);
    push_key(out, &mut first, "name");
    push_str(out, f.name);
    push_key(out, &mut first, "unit");
    push_str(out, f.unit.json_name());
    if f.unit == UnitKind::Enum {
        push_key(out, &mut first, "enum");
        push_names(out, f);
    }
    push_key(out, &mut first, "min");
    let _ = write!(out, "{}", f.min);
    push_key(out, &mut first, "max");
    let _ = write!(out, "{}", f.max);
    if let Some(min) = f.prod_min {
        push_key(out, &mut first, "prod_min");
        let _ = write!(out, "{min}");
    }
    push_key(out, &mut first, "default");
    let _ = write!(out, "{}", f.default);
    push_key(out, &mut first, "per_group");
    let _ = write!(out, "{}", f.per_group);
    if full {
        push_key(out, &mut first, "structural");
        let _ = write!(out, "{}", f.structural);
        push_key(out, &mut first, "manual_only");
        let _ = write!(out, "{}", f.manual_only);
        push_key(out, &mut first, "relax");
        push_str(out, relax_name(f.relax));
        push_key(out, &mut first, "minor_change");
        push_str(out, &minor_text(f.minor_change));
    }
    push_key(out, &mut first, "since");
    let _ = write!(out, "{}", f.since);
    if full {
        push_key(out, &mut first, "doc");
        push_str(out, f.doc);
        push_key(out, &mut first, "display");
        let mut df = true;
        out.push('{');
        for (k, v) in [("min", f.min), ("max", f.max), ("default", f.default)] {
            push_key(out, &mut df, k);
            push_str(out, &format_value(*f, v, None));
        }
        out.push('}');
    }
    out.push('}');
}

/// Fields of `s` with `since ≤ ver` (a prefix).
fn fields_at<S: SchemaView + ?Sized>(s: &S, ver: u16) -> impl Iterator<Item = FieldRef<'_>> {
    (0..s.n_fields())
        .map_while(|i| s.field(i))
        .take_while(move |f| f.since <= ver)
}

/// Strategy-level head values (`non_prod_only = None` in the digest input).
struct Head<'a> {
    name: &'a str,
    kind_id: u16,
    non_prod_only: Option<bool>,
}

/// Head keys shared by the full JSON and the digest input.
fn push_head<S: SchemaView + ?Sized>(
    out: &mut String,
    first: &mut bool,
    h: &Head<'_>,
    s: &S,
    ver: u16,
    full: bool,
) {
    let Head {
        name,
        kind_id,
        non_prod_only,
    } = *h;
    push_key(out, first, "name");
    push_str(out, name);
    push_key(out, first, "kind_id");
    let _ = write!(out, "{kind_id}");
    if let Some(np) = non_prod_only {
        push_key(out, first, "non_prod_only");
        let _ = write!(out, "{np}");
    }
    push_key(out, first, "schema_id");
    let _ = write!(out, "{}", s.id());
    push_key(out, first, "schema_version");
    let _ = write!(out, "{ver}");
    push_key(out, first, "fields");
    out.push('[');
    for (i, f) in fields_at(s, ver).enumerate() {
        if i > 0 {
            out.push(',');
        }
        push_field(out, &f, full);
    }
    out.push(']');
}

/// Digest input JSON and its SHA-256 prefix (shared with `OwnedSchema::schema_digest`).
pub(crate) fn digest_of<S: SchemaView + ?Sized>(name: &str, kind_id: u16, s: &S, ver: u16) -> u64 {
    let mut out = String::new();
    let mut first = true;
    out.push('{');
    push_head(
        &mut out,
        &mut first,
        &Head {
            name,
            kind_id,
            non_prod_only: None,
        },
        s,
        ver,
        false,
    );
    out.push('}');
    let h = Sha256::digest(out.as_bytes());
    let mut b = [0u8; 8];
    for (d, s) in b.iter_mut().zip(h.iter()) {
        *d = *s;
    }
    u64::from_le_bytes(b)
}

/// Canonical schema JSON of strategy `d` at schema version `ver` (fields with `since ≤ ver`);
/// `layout` = `Model::LAYOUT` (`ModelLayoutDesc::EMPTY` without a Model) gives `sections`.
/// Keys in README order, no whitespace, integers in decimal raw units; each field element ends
/// with `"display": {"min", "max", "default"}` (`format_value`; `Qty`/`Px` raw without exponent).
pub fn schema_json(d: &StrategyDesc, layout: &ModelLayoutDesc, ver: u16) -> String {
    let mut out = String::new();
    let mut first = true;
    out.push('{');
    let head = Head {
        name: d.name,
        kind_id: d.kind_id,
        non_prod_only: Some(d.non_prod_only),
    };
    push_head(&mut out, &mut first, &head, d.schema, ver, true);

    push_key(&mut out, &mut first, "notes");
    out.push('[');
    for (i, n) in d.notes.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        let mut f = true;
        out.push('{');
        push_key(&mut out, &mut f, "code");
        let _ = write!(out, "{}", n.code);
        push_key(&mut out, &mut f, "name");
        push_str(&mut out, n.name);
        push_key(&mut out, &mut f, "level");
        push_str(&mut out, level_name(n.level));
        push_key(&mut out, &mut f, "v");
        out.push('[');
        for (j, v) in n.v.iter().enumerate() {
            if j > 0 {
                out.push(',');
            }
            push_str(&mut out, v);
        }
        out.push(']');
        push_key(&mut out, &mut f, "doc");
        push_str(&mut out, n.doc);
        out.push('}');
    }
    out.push(']');

    push_key(&mut out, &mut first, "report");
    out.push('[');
    for (i, r) in d.report.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        let mut f = true;
        out.push('{');
        push_key(&mut out, &mut f, "name");
        push_str(&mut out, r.name);
        push_key(&mut out, &mut f, "unit");
        push_str(&mut out, r.unit.kind().json_name());
        push_key(&mut out, &mut f, "agg");
        push_str(&mut out, agg_name(r.agg));
        push_key(&mut out, &mut f, "scope");
        push_str(&mut out, scope_name(r.scope));
        push_key(&mut out, &mut f, "note");
        let _ = write!(out, "{}", r.note);
        push_key(&mut out, &mut f, "v_ix");
        let _ = write!(out, "{}", r.v_ix);
        push_key(&mut out, &mut f, "doc");
        push_str(&mut out, r.doc);
        out.push('}');
    }
    out.push(']');

    push_key(&mut out, &mut first, "cmds");
    out.push('[');
    for (i, c) in d.cmds.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        let mut f = true;
        out.push('{');
        push_key(&mut out, &mut f, "op");
        let _ = write!(out, "{}", c.op);
        push_key(&mut out, &mut f, "name");
        push_str(&mut out, c.name);
        push_key(&mut out, &mut f, "reduce_only");
        let _ = write!(out, "{}", c.reduce_only);
        push_key(&mut out, &mut f, "fields");
        out.push('[');
        for (j, fd) in c.fields.iter().enumerate() {
            if j > 0 {
                out.push(',');
            }
            push_field(&mut out, &fd.as_ref(j as u16), true);
        }
        out.push(']');
        push_key(&mut out, &mut f, "doc");
        push_str(&mut out, c.doc);
        out.push('}');
    }
    out.push(']');

    push_key(&mut out, &mut first, "sections");
    out.push('[');
    for (i, s) in layout.sections.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        let mut f = true;
        out.push('{');
        push_key(&mut out, &mut f, "tag");
        let _ = write!(out, "{}", s.tag);
        push_key(&mut out, &mut f, "name");
        push_str(&mut out, s.name);
        out.push('}');
    }
    out.push(']');
    out.push('}');
    out
}

/// SHA-256 of the digest input JSON (`name`, `kind_id`, `schema_id`, `schema_version`, and
/// `fields` cut at `ver` with only `ix/name/unit/enum/min/max/prod_min/default/per_group/since`), first
/// 8 bytes read little-endian. Docs, non-validation attributes, `non_prod_only`, descriptors and sections
/// are not covered: changing them does not affect encoded parameters.
pub fn schema_digest(d: &StrategyDesc, ver: u16) -> u64 {
    digest_of(d.name, d.kind_id, d.schema, ver)
}
