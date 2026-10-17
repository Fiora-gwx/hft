//! Compile-time checks of schemas and strategy descriptors (README "schema JSON 与
//! define_params!" schema 编译期校验, "策略描述符" 编译期校验). Every function is a `const fn`
//! that fails const evaluation (a compile error at the `const _` that calls it) with a message
//! naming the rule; at run time the same functions return normally for valid input.

use hft_event::NoteLevel;

use crate::StrategyDesc;
use crate::desc::{CmdDesc, NoteDesc, ReportFieldDesc};
use crate::params::{FieldDesc, MinorChange, ParamSchemaDesc, Unit};

/// Largest number of schema fields.
pub const MAX_FIELDS: usize = 256;
/// Largest number of command payload fields.
pub const MAX_CMD_FIELDS: usize = 31;
/// Largest strategy Note code (low 24 bits of `Note.code`).
pub const MAX_NOTE_CODE: u32 = 0x00FF_FFFF;

/// Byte-wise `&str` equality usable in `const` context.
pub const fn str_eq(a: &str, b: &str) -> bool {
    let (mut a, mut b) = (a.as_bytes(), b.as_bytes());
    if a.len() != b.len() {
        return false;
    }
    while let ([x, ra @ ..], [y, rb @ ..]) = (a, b) {
        if *x != *y {
            return false;
        }
        a = ra;
        b = rb;
    }
    true
}

/// `s` without one leading space (doc comments arrive as `" text"`).
pub const fn trim_doc(s: &'static str) -> &'static str {
    match s.as_bytes() {
        [b' ', rest @ ..] => match core::str::from_utf8(rest) {
            Ok(t) => t,
            Err(_) => s,
        },
        _ => s,
    }
}

/// The per-field rules shared by schema fields and command fields.
const fn check_field(f: &FieldDesc) {
    assert!(f.min <= f.default, "field: min > default");
    assert!(f.default <= f.max, "field: default > max");
    if let Some(min) = f.prod_min {
        assert!(f.min <= min && min <= f.max, "field: prod_min outside min/max");
    }
    match f.unit {
        Unit::Qty | Unit::Px => assert!(f.per_group, "field: Qty/Px fields must be per_group"),
        Unit::Enum(names) => {
            assert!(f.default >= 0, "field: Enum default index out of range");
            assert!(
                (f.default as u64) < names.len() as u64,
                "field: Enum default index out of range"
            );
        }
        _ => {}
    }
    match f.minor_change {
        MinorChange::None => {}
        MinorChange::Any => {}
        MinorChange::RelPct(n) => {
            assert!(!f.structural, "field: structural fields need minor_change = None");
            assert!(
                !matches!(f.unit, Unit::Bool | Unit::Enum(_)),
                "field: Bool/Enum take minor None or Any"
            );
            assert!(n >= 1 && n <= 100, "field: RelPct(n) needs 1 <= n <= 100");
        }
        MinorChange::Abs(_) => {
            assert!(!f.structural, "field: structural fields need minor_change = None");
            assert!(
                !matches!(f.unit, Unit::Bool | Unit::Enum(_)),
                "field: Bool/Enum take minor None or Any"
            );
        }
    }
    if matches!(f.minor_change, MinorChange::Any) {
        assert!(!f.structural, "field: structural fields need minor_change = None");
    }
}

/// Schema rules: unique names, `min ≤ default ≤ max`, Qty/Px per group, valid Enum default,
/// `since` non-decreasing with maximum <= `version`, ≤ 256 fields, structural ⇒ `minor = None`,
/// Bool/Enum ⇒ `minor ∈ {None, Any}`, `RelPct(n)` with 1 ≤ n ≤ 100.
pub const fn validate_schema(s: &ParamSchemaDesc) {
    assert!(s.fields.len() <= MAX_FIELDS, "schema: more than 256 fields");
    let mut since = 0u16;
    let mut rest = s.fields;
    while let [f, tail @ ..] = rest {
        check_field(f);
        assert!(f.since >= since, "schema: since must be non-decreasing");
        since = f.since;
        let mut others = tail;
        while let [g, more @ ..] = others {
            assert!(!str_eq(f.name, g.name), "schema: duplicate field name");
            others = more;
        }
        rest = tail;
    }
    assert!(since <= s.version, "schema: version must cover the largest since");
}

const fn validate_cmd(c: &CmdDesc) {
    assert!(c.op >= 1, "cmds: op must be 1..=255");
    assert!(c.fields.len() <= MAX_CMD_FIELDS, "cmds: more than 31 fields");
    let mut rest = c.fields;
    while let [f, tail @ ..] = rest {
        check_field(f);
        assert!(!f.per_group, "cmds: command fields cannot be per_group");
        assert!(!f.structural, "cmds: command fields cannot be structural");
        assert!(
            f.prod_min.is_none(),
            "cmds: prod_min is only for strategy parameters"
        );
        assert!(
            matches!(f.minor_change, MinorChange::None),
            "cmds: command fields need minor_change = None"
        );
        let mut others = tail;
        while let [g, more @ ..] = others {
            assert!(!str_eq(f.name, g.name), "cmds: duplicate field name");
            others = more;
        }
        rest = tail;
    }
}

const fn note_has(notes: &[NoteDesc], code: u32) -> bool {
    let mut rest = notes;
    while let [n, tail @ ..] = rest {
        if n.code == code {
            return true;
        }
        rest = tail;
    }
    false
}

/// Descriptor rules: Note codes 1..=0xFF_FFFF and names unique, `v.len() ≤ 8`, level not P0;
/// report names unique, `note` registered, `v_ix < 8`; command ops (1..=255) and names unique,
/// each command's fields valid (≤ 31, not per_group/structural, `minor = None`). Also checks the
/// schema.
pub const fn validate_desc(d: &StrategyDesc) {
    validate_schema(d.schema);
    let mut rest: &[NoteDesc] = d.notes;
    while let [n, tail @ ..] = rest {
        assert!(
            n.code >= 1 && n.code <= MAX_NOTE_CODE,
            "notes: code must be 1..=0xFF_FFFF"
        );
        assert!(n.v.len() <= 8, "notes: at most 8 v[] entries");
        assert!(
            !matches!(n.level, NoteLevel::P0),
            "notes: strategy Notes cannot be P0"
        );
        let mut others = tail;
        while let [m, more @ ..] = others {
            assert!(n.code != m.code, "notes: duplicate code");
            assert!(!str_eq(n.name, m.name), "notes: duplicate name");
            others = more;
        }
        rest = tail;
    }
    let mut rest: &[ReportFieldDesc] = d.report;
    while let [r, tail @ ..] = rest {
        assert!(
            note_has(d.notes, r.note),
            "report: note is not registered in notes"
        );
        assert!(r.v_ix < 8, "report: v_ix must be < 8");
        let mut others = tail;
        while let [q, more @ ..] = others {
            assert!(!str_eq(r.name, q.name), "report: duplicate name");
            others = more;
        }
        rest = tail;
    }
    let mut rest: &[CmdDesc] = d.cmds;
    while let [c, tail @ ..] = rest {
        validate_cmd(c);
        let mut others = tail;
        while let [e, more @ ..] = others {
            assert!(c.op != e.op, "cmds: duplicate op");
            assert!(!str_eq(c.name, e.name), "cmds: duplicate name");
            others = more;
        }
        rest = tail;
    }
}

/// `strategy_set!` rules over the registered kinds: each descriptor valid; `kind_id`, `name` and
/// `schema.id` unique.
pub const fn validate_set(kinds: &[StrategyDesc]) {
    let mut rest = kinds;
    while let [k, tail @ ..] = rest {
        validate_desc(k);
        let mut others = tail;
        while let [o, more @ ..] = others {
            assert!(k.kind_id != o.kind_id, "strategy_set!: duplicate kind_id");
            assert!(!str_eq(k.name, o.name), "strategy_set!: duplicate name");
            assert!(k.schema.id != o.schema.id, "strategy_set!: duplicate schema.id");
            others = more;
        }
        rest = tail;
    }
}
