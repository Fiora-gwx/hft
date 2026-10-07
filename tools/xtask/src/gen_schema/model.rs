//! The intermediate representation `gen-schema` renders and checks (one `Model` per run).
//!
//! The real run builds it from the linked `hft-event`/`hft-refdata` exports, the source crates'
//! `NOTE_CODES` tables (text scan) and the strategy schemas exported by `hft-core schemas`; tests build
//! or mutate it directly (the stand-ins of crates/hft-event/TESTS.md EVT-172–EVT-180).

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::sync::LazyLock;

use hft_event::schema::{EnumDesc, FieldDesc, FieldTy, IdKind};
use hft_event::{CrcPolicy, Domain, Layout, NoteLevel, NoteSource, Role};
use regex::Regex;
use walkdir::WalkDir;

use crate::{EnvError, rel_path};

/// Everything one `gen-schema` run exports and checks.
#[derive(Clone, Debug)]
pub struct Model {
    /// Production kinds (`KINDS` without test kinds), in number order.
    pub kinds: Vec<KindRow>,
    /// hft-ext-proto `KINDS_EXT`; `None` = the optional extension is not enabled.
    pub ext_kinds: Option<Vec<KindRow>>,
    pub cmd_kinds: Vec<Code>,
    /// hft-refdata `LoadErrCode`.
    pub load_err: Vec<Code>,
    /// Engine Note name registry (`NOTE_NAMES`).
    pub notes: Vec<NoteReg>,
    /// `(source, n, name)` rows of the source crates' `NOTE_CODES` tables.
    pub note_consts: Vec<NoteConst>,
    /// `ConnState.reason` codes (hft-venue `CONN_REASONS`); empty when that crate is absent.
    pub conn_reasons: Vec<ConnReason>,
    /// Exported container structs (SPB1, framing).
    pub structs: Vec<StructRow>,
    /// Every code table and bit set, by name.
    pub enums: BTreeMap<String, EnumTable>,
    /// RDB1 row formats (hft-refdata).
    pub rdb_rows: Vec<RdbRow>,
    /// `(kind, body offset, length)` masked by the verifying replay.
    pub measure_mask: Vec<(u16, u16, u16)>,
    pub strategies: Strategies,
}

/// One `ConnState.reason` code: `(code, name, ConnStateKind name)`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ConnReason {
    pub code: u32,
    pub name: String,
    pub state: String,
}

/// Strategy descriptors read from the `hft-core schemas` export.
#[derive(Clone, Debug)]
pub enum Strategies {
    /// Not read (reason); the lock keeps the baseline's strategy rows unchanged.
    Skipped(String),
    Exported(Vec<StrategyRow>),
}

/// One `(number, name)` code.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Code {
    pub value: u32,
    pub name: String,
}

impl Code {
    pub fn new(value: u32, name: &str) -> Code {
        Code {
            value,
            name: name.to_string(),
        }
    }
}

/// One kind of the registry.
#[derive(Clone, Debug)]
pub struct KindRow {
    pub kind: u16,
    pub name: String,
    pub domain: String,
    pub role: String,
    pub layout: LayoutRow,
    pub schema_version: u16,
    pub journals: String,
    pub crc: String,
    pub optional: bool,
    pub deprecated: bool,
    /// Fixed: body fields; Var/Multi: prefix fields.
    pub fields: Vec<Field>,
}

/// Body layout class of a kind.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum LayoutRow {
    Fixed { size: u16 },
    Var { prefix: u16, max: u32 },
    Multi { prefix: u16 },
}

impl LayoutRow {
    /// Bytes described by the field table (body size or prefix size).
    pub fn size(&self) -> u16 {
        match *self {
            LayoutRow::Fixed { size } => size,
            LayoutRow::Var { prefix, .. } | LayoutRow::Multi { prefix } => prefix,
        }
    }

    pub fn class(&self) -> &'static str {
        match self {
            LayoutRow::Fixed { .. } => "fixed",
            LayoutRow::Var { .. } => "var",
            LayoutRow::Multi { .. } => "multi",
        }
    }

    /// `fixed 96` / `var 32 4096` / `multi 72`.
    pub fn text(&self) -> String {
        match *self {
            LayoutRow::Fixed { size } => format!("fixed {size}"),
            LayoutRow::Var { prefix, max } => format!("var {prefix} {max}"),
            LayoutRow::Multi { prefix } => format!("multi {prefix}"),
        }
    }
}

/// One field of a body, prefix or struct.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Field {
    pub name: String,
    pub offset: u16,
    pub ty: Ty,
    pub measure: bool,
    pub doc: String,
}

/// Wire type of a field.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Ty {
    U8,
    U16,
    U32,
    U64,
    I8,
    I16,
    I32,
    I64,
    Px,
    Qty,
    Notional,
    Rate,
    Tsc,
    UtcNs,
    /// ID newtype (`inst`, `venue`, …) and its width.
    Id(String, u16),
    /// Code field: table name.
    Enum(String),
    /// Bit-set field: table name.
    Bits(String),
    Bytes(u16),
    Pad(u16),
    Nested(Vec<Field>),
}

impl Ty {
    /// Width in bytes (`enums` resolves the width of code fields).
    pub fn width(&self, enums: &BTreeMap<String, EnumTable>) -> u16 {
        match self {
            Ty::U8 | Ty::I8 => 1,
            Ty::U16 | Ty::I16 => 2,
            Ty::U32 | Ty::I32 => 4,
            Ty::U64 | Ty::I64 | Ty::Px | Ty::Qty | Ty::Notional | Ty::Rate | Ty::Tsc | Ty::UtcNs => 8,
            Ty::Id(_, w) => *w,
            Ty::Enum(n) | Ty::Bits(n) => enums.get(n).map_or(0, |e| e.width as u16),
            Ty::Bytes(n) | Ty::Pad(n) => *n,
            Ty::Nested(f) => fields_end(f, enums),
        }
    }

    /// Type text of the lock file and the catalog (`u8`, `px`, `id:inst`, `enum:Side`, `pad:4` …).
    pub fn text(&self) -> String {
        match self {
            Ty::U8 => "u8".into(),
            Ty::U16 => "u16".into(),
            Ty::U32 => "u32".into(),
            Ty::U64 => "u64".into(),
            Ty::I8 => "i8".into(),
            Ty::I16 => "i16".into(),
            Ty::I32 => "i32".into(),
            Ty::I64 => "i64".into(),
            Ty::Px => "px".into(),
            Ty::Qty => "qty".into(),
            Ty::Notional => "notional".into(),
            Ty::Rate => "rate".into(),
            Ty::Tsc => "tsc".into(),
            Ty::UtcNs => "utc_ns".into(),
            Ty::Id(k, _) => format!("id:{k}"),
            Ty::Enum(n) => format!("enum:{n}"),
            Ty::Bits(n) => format!("bits:{n}"),
            Ty::Bytes(n) => format!("bytes:{n}"),
            Ty::Pad(n) => format!("pad:{n}"),
            Ty::Nested(_) => "struct".into(),
        }
    }
}

/// End offset of the last field.
pub fn fields_end(f: &[Field], enums: &BTreeMap<String, EnumTable>) -> u16 {
    f.iter().map(|x| x.offset + x.ty.width(enums)).max().unwrap_or(0)
}

/// A code table or a bit set.
#[derive(Clone, Debug)]
pub struct EnumTable {
    pub name: String,
    pub width: u8,
    pub items: Vec<Code>,
}

impl EnumTable {
    /// A field of this code table cannot hold 0 (no item has value 0).
    pub fn zero_illegal(&self) -> bool {
        !self.items.iter().any(|i| i.value == 0)
    }
}

/// One `NOTE_NAMES` row.
#[derive(Clone, Debug)]
pub struct NoteReg {
    pub name: String,
    /// `NoteSource` name (`Oms`, `Ledger`, …).
    pub source: String,
    /// Default `NoteLevel` name; `None` = by condition.
    pub level: Option<String>,
    /// `(name, bit, bits)` of the listed `v[]` fields.
    pub v: Vec<(String, u16, u16)>,
}

/// One row of a source crate's `NOTE_CODES` table.
#[derive(Clone, Debug)]
pub struct NoteConst {
    pub source: String,
    pub n: u32,
    pub name: String,
    /// Where it was read (`crates/hft-oms/src/notes.rs`).
    pub origin: String,
}

/// An exported container struct.
#[derive(Clone, Debug)]
pub struct StructRow {
    pub name: String,
    pub size: u16,
    pub fields: Vec<Field>,
}

/// One RDB1 row format.
#[derive(Clone, Debug)]
pub struct RdbRow {
    pub section: u16,
    pub name: String,
    pub row_ver: u16,
    pub size: u16,
    /// `(name, offset, size, Rust type)`.
    pub fields: Vec<(String, u16, u16, String)>,
}

/// The append-only facts of one strategy kind (from its exported schema JSON).
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StrategyRow {
    pub kind_id: u32,
    pub name: String,
    /// `NoteDesc.code → name`.
    pub notes: Vec<Code>,
    /// `CmdDesc.op → name`.
    pub ops: Vec<Code>,
    /// Checkpoint `SectionDesc.tag → name`.
    pub tags: Vec<Code>,
}

/// `NoteSource` names in value order.
pub fn note_sources() -> Vec<Code> {
    NoteSource::DESC
        .items
        .iter()
        .map(|i| Code::new(i.value, i.name))
        .collect()
}

/// Value of a `NoteSource` name.
pub fn note_source_value(name: &str) -> Option<u32> {
    NoteSource::DESC.value(name)
}

/// Value of a `NoteLevel` name.
pub fn note_level_value(name: &str) -> Option<u32> {
    NoteLevel::DESC.value(name)
}

/// Sources whose const tables only need the name prefix (not in `NOTE_NAMES`).
pub const PREFIX_SOURCES: &[(&str, &str)] =
    &[("Venue", "Venue"), ("Measure", "Measure"), ("Housekeeping", "Hk")];

/// Container structs recorded in the lock file (SPB1).
pub const LOCK_STRUCTS: &[&str] = &["StrategyParamsHdr", "GroupSpec", "LegDef"];

fn id_name(k: IdKind) -> &'static str {
    match k {
        IdKind::Inst => "inst",
        IdKind::Asset => "asset",
        IdKind::Venue => "venue",
        IdKind::Account => "account",
        IdKind::Strategy => "strategy",
        IdKind::Group => "group",
        IdKind::Slot => "slot",
        IdKind::Conn => "conn",
        IdKind::Kind => "kind",
    }
}

fn add_enum(enums: &mut BTreeMap<String, EnumTable>, d: &EnumDesc) {
    enums.entry(d.name.to_string()).or_insert_with(|| EnumTable {
        name: d.name.to_string(),
        width: d.width,
        items: d.items.iter().map(|i| Code::new(i.value, i.name)).collect(),
    });
}

fn conv_ty(t: &FieldTy, enums: &mut BTreeMap<String, EnumTable>) -> Ty {
    match t {
        FieldTy::U8 => Ty::U8,
        FieldTy::U16 => Ty::U16,
        FieldTy::U32 => Ty::U32,
        FieldTy::U64 => Ty::U64,
        FieldTy::I8 => Ty::I8,
        FieldTy::I16 => Ty::I16,
        FieldTy::I32 => Ty::I32,
        FieldTy::I64 => Ty::I64,
        FieldTy::Px => Ty::Px,
        FieldTy::Qty => Ty::Qty,
        FieldTy::Notional => Ty::Notional,
        FieldTy::Rate => Ty::Rate,
        FieldTy::Tsc => Ty::Tsc,
        FieldTy::UtcNs => Ty::UtcNs,
        FieldTy::Id(k) => Ty::Id(id_name(*k).to_string(), k.width()),
        FieldTy::Enum(d) => {
            add_enum(enums, d);
            Ty::Enum(d.name.to_string())
        }
        FieldTy::Bits(d) => {
            add_enum(enums, d);
            Ty::Bits(d.name.to_string())
        }
        FieldTy::Bytes(n) => Ty::Bytes(*n),
        FieldTy::Pad(n) => Ty::Pad(*n),
        FieldTy::Nested(f) => Ty::Nested(conv_fields(f, enums)),
    }
}

fn conv_fields(f: &[FieldDesc], enums: &mut BTreeMap<String, EnumTable>) -> Vec<Field> {
    f.iter()
        .map(|d| Field {
            name: d.name.to_string(),
            offset: d.offset,
            ty: conv_ty(&d.ty, enums),
            measure: d.measure,
            doc: d.doc.trim().to_string(),
        })
        .collect()
}

fn domain_name(d: Domain) -> &'static str {
    match d {
        Domain::Market => "Market",
        Domain::Exec => "Exec",
        Domain::Chain => "Chain",
        Domain::Control => "Control",
        Domain::System => "System",
        Domain::Raw => "Raw",
        Domain::Test => "Test",
    }
}

fn role_name(r: Role) -> &'static str {
    match r {
        Role::Input => "Input",
        Role::Output => "Output",
        Role::Meta => "Meta",
        Role::ByBody => "ByBody",
    }
}

fn journals_name(m: hft_event::JournalMask) -> String {
    let mut v = Vec::new();
    for (bit, n) in [
        (hft_event::JournalMask::SEQ, "seq"),
        (hft_event::JournalMask::RAW, "raw"),
        (hft_event::JournalMask::CHAIN, "chain"),
    ] {
        if m.contains(bit) {
            v.push(n);
        }
    }
    v.join("+")
}

static DEPRECATED: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"#\[deprecated[^\]]*\]\s*(?:///[^\n]*\n\s*)*pub const \w+\s*:\s*Self\s*=\s*EventKind\(\s*(0x[0-9A-Fa-f]+)\s*\)")
        .unwrap()
});
static NOTE_CODES_BLOCK: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?s)NOTE_CODES\s*:\s*&\s*\[\s*\(\s*NoteSource\s*,\s*u32\s*,\s*&\s*(?:'static\s+)?str\s*\)\s*\]\s*=\s*&\s*\[(.*?)\]\s*;")
        .unwrap()
});
static NOTE_CODE_ROW: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"\(\s*NoteSource::(\w+)\s*,\s*(0x[0-9A-Fa-f_]+|[0-9_]+)\s*,\s*"([^"]*)"\s*,?\s*\)"#).unwrap()
});
static CONN_REASONS_BLOCK: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?s)CONN_REASONS\s*:\s*&\s*\[\s*\(\s*u32\s*,\s*&\s*(?:'static\s+)?str\s*,\s*ConnStateKind\s*\)\s*\]\s*=\s*&\s*\[(.*?)\]\s*;")
        .unwrap()
});
static CONN_REASON_ROW: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"\(\s*([0-9_]+)\s*,\s*"([^"]*)"\s*,\s*ConnStateKind::(\w+)\s*,?\s*\)"#).unwrap()
});
static LINE_COMMENT: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"//[^\n]*").unwrap());

/// Kind numbers marked `#[deprecated]` in `crates/hft-event/src/kinds.rs` under `root`.
pub fn scan_deprecated(root: &Path) -> BTreeSet<u16> {
    let p = root.join("crates/hft-event/src/kinds.rs");
    let Ok(text) = std::fs::read_to_string(p) else {
        return BTreeSet::new();
    };
    DEPRECATED
        .captures_iter(&text)
        .filter_map(|c| u16::from_str_radix(c[1].trim_start_matches("0x"), 16).ok())
        .collect()
}

fn parse_int(s: &str) -> Option<u32> {
    let s = s.replace('_', "");
    match s.strip_prefix("0x") {
        Some(h) => u32::from_str_radix(h, 16).ok(),
        None => s.parse().ok(),
    }
}

/// Rows of every `NOTE_CODES: &[(NoteSource, u32, &str)]` table under `root/{crates,apps}/*/src`.
pub fn scan_note_consts(root: &Path) -> Vec<NoteConst> {
    let mut out = Vec::new();
    for top in ["crates", "apps"] {
        let dir = root.join(top);
        let Ok(rd) = std::fs::read_dir(&dir) else { continue };
        let mut crates: Vec<_> = rd.filter_map(|e| e.ok()).map(|e| e.path()).collect();
        crates.sort();
        for c in crates {
            let src = c.join("src");
            if !src.is_dir() {
                continue;
            }
            let mut files: Vec<_> = WalkDir::new(&src)
                .into_iter()
                .filter_map(|e| e.ok())
                .filter(|e| e.file_type().is_file() && e.path().extension().is_some_and(|x| x == "rs"))
                .map(|e| e.into_path())
                .collect();
            files.sort();
            for f in files {
                let Ok(text) = std::fs::read_to_string(&f) else {
                    continue;
                };
                if !text.contains("NOTE_CODES") {
                    continue;
                }
                for b in NOTE_CODES_BLOCK.captures_iter(&text) {
                    let body = LINE_COMMENT.replace_all(&b[1], "");
                    for r in NOTE_CODE_ROW.captures_iter(&body) {
                        if let Some(n) = parse_int(&r[2]) {
                            out.push(NoteConst {
                                source: r[1].to_string(),
                                n,
                                name: r[3].to_string(),
                                origin: rel_path(root, &f),
                            });
                        }
                    }
                }
            }
        }
    }
    out
}

/// Rows of hft-venue's `CONN_REASONS: &[(u32, &str, ConnStateKind)]` (text scan of
/// `crates/hft-venue/src/codes.rs` under `root`; absent → empty).
pub fn scan_conn_reasons(root: &Path) -> Vec<ConnReason> {
    let Ok(text) = std::fs::read_to_string(root.join("crates/hft-venue/src/codes.rs")) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for b in CONN_REASONS_BLOCK.captures_iter(&text) {
        let body = LINE_COMMENT.replace_all(&b[1], "");
        for r in CONN_REASON_ROW.captures_iter(&body) {
            if let Some(code) = parse_int(&r[1]) {
                out.push(ConnReason {
                    code,
                    name: r[2].to_string(),
                    state: r[3].to_string(),
                });
            }
        }
    }
    out
}

fn kind_row(
    k: &hft_event::KindInfo,
    deprecated: &BTreeSet<u16>,
    enums: &mut BTreeMap<String, EnumTable>,
) -> KindRow {
    KindRow {
        kind: k.kind.0,
        name: k.name.to_string(),
        domain: domain_name(k.domain).to_string(),
        role: role_name(k.role).to_string(),
        layout: match k.layout {
            Layout::Fixed { size } => LayoutRow::Fixed { size },
            Layout::Var { prefix, max } => LayoutRow::Var { prefix, max },
            Layout::Multi { prefix } => LayoutRow::Multi { prefix },
        },
        schema_version: k.schema_version,
        journals: journals_name(k.journals),
        crc: match k.crc {
            CrcPolicy::Never => "never".into(),
            CrcPolicy::Always => "always".into(),
        },
        optional: k.optional,
        deprecated: deprecated.contains(&k.kind.0),
        fields: conv_fields(k.fields, enums),
    }
}

/// The library part of the model: the linked hft-event and hft-refdata exports, with the
/// `#[deprecated]` marks and `NOTE_CODES` tables read from the tree at `root`. Strategies: `Skipped`.
pub fn from_crates(root: &Path) -> Model {
    let cat = hft_event::schema::catalog();
    let deprecated = scan_deprecated(root);
    let mut enums = BTreeMap::new();
    for e in cat.enums {
        add_enum(&mut enums, e);
    }
    add_enum(&mut enums, &NoteSource::DESC);
    add_enum(&mut enums, &NoteLevel::DESC);
    let kinds = cat
        .kinds
        .iter()
        .map(|k| kind_row(k, &deprecated, &mut enums))
        .collect();
    let structs = cat
        .structs
        .iter()
        .map(|s| StructRow {
            name: s.name.to_string(),
            size: s.size,
            fields: conv_fields(s.fields, &mut enums),
        })
        .collect();
    let notes = cat
        .notes
        .iter()
        .map(|n| NoteReg {
            name: n.name.to_string(),
            source: NoteSource::DESC
                .items
                .iter()
                .find(|i| i.value == n.source as u32)
                .map_or_else(|| format!("{}", n.source as u8), |i| i.name.to_string()),
            level: n.level.and_then(|l| {
                NoteLevel::DESC
                    .items
                    .iter()
                    .find(|i| i.value == l as u32)
                    .map(|i| i.name.to_string())
            }),
            v: n.v.iter().map(|f| (f.name.to_string(), f.bit, f.bits)).collect(),
        })
        .collect();
    let rdb_rows = hft_refdata::schema::RDB1_ROWS
        .iter()
        .map(|r| RdbRow {
            section: r.section,
            name: r.name.to_string(),
            row_ver: r.row_ver,
            size: r.size as u16,
            fields: r
                .fields
                .iter()
                .map(|f| {
                    let short = f.name.rsplit('.').next().unwrap_or(f.name);
                    (
                        short.to_string(),
                        f.offset as u16,
                        f.size as u16,
                        f.ty.to_string(),
                    )
                })
                .collect(),
        })
        .collect();
    Model {
        kinds,
        ext_kinds: None,
        cmd_kinds: cat.cmd_kinds.iter().map(|i| Code::new(i.value, i.name)).collect(),
        load_err: hft_refdata::schema::load_err_codes()
            .map(|(n, v)| Code::new(v as u32, n))
            .collect(),
        notes,
        note_consts: scan_note_consts(root),
        conn_reasons: scan_conn_reasons(root),
        structs,
        enums,
        rdb_rows,
        measure_mask: cat.measure_mask.iter().map(|(k, o, l)| (k.0, *o, *l)).collect(),
        strategies: Strategies::Skipped("not read".into()),
    }
}

/// Reads the strategy rows of an `hft-core schemas --out <dir>` export (`index.json` + files).
pub fn read_strategy_export(dir: &Path) -> Result<Vec<StrategyRow>, EnvError> {
    let idx_path = dir.join("index.json");
    let idx_text = std::fs::read_to_string(&idx_path)
        .map_err(|e| EnvError(format!("read {}: {e}", idx_path.display())))?;
    let idx: serde_json::Value = serde_json::from_str(&idx_text)
        .map_err(|e| EnvError(format!("parse {}: {e}", idx_path.display())))?;
    let arr = idx
        .as_array()
        .ok_or_else(|| EnvError(format!("{}: top level is not an array", idx_path.display())))?;
    let mut out = Vec::new();
    for item in arr {
        let kind_id = item["kind_id"]
            .as_u64()
            .ok_or_else(|| EnvError("index.json: kind_id missing".into()))? as u32;
        let name = item["name"].as_str().unwrap_or_default().to_string();
        let file = item["file"]
            .as_str()
            .ok_or_else(|| EnvError("index.json: file missing".into()))?;
        let p = dir.join(file);
        let text = std::fs::read_to_string(&p).map_err(|e| EnvError(format!("read {}: {e}", p.display())))?;
        let v: serde_json::Value =
            serde_json::from_str(&text).map_err(|e| EnvError(format!("parse {}: {e}", p.display())))?;
        let codes = |key: &str, num: &str| -> Vec<Code> {
            v[key]
                .as_array()
                .map(|a| {
                    a.iter()
                        .filter_map(|x| Some(Code::new(x[num].as_u64()? as u32, x["name"].as_str()?)))
                        .collect()
                })
                .unwrap_or_default()
        };
        out.push(StrategyRow {
            kind_id,
            name,
            notes: codes("notes", "code"),
            ops: codes("cmds", "op"),
            tags: codes("sections", "tag"),
        });
    }
    out.sort_by_key(|s| s.kind_id);
    Ok(out)
}
