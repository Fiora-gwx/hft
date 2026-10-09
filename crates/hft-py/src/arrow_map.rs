//! Kind → Arrow column map and the row → column transposition (README "Arrow 映射").
//!
//! The column table of every kind is derived at run time from hft-event's `schema` catalog (the
//! same descriptors `cargo xtask gen-schema` exports to `python/_generated/`), so it cannot drift
//! from the Rust structs. Every table starts with the record-header columns `seq`, `tsc`,
//! `proc_ns`, `flags`; nested structs are flattened to their leaf names; padding is dropped.

use std::collections::BTreeMap;
use std::sync::Arc;

use hft_event::schema::StructDesc;
use hft_event::{EventKind, FieldDesc, FieldTy, IdKind, KindInfo, Layout};

pub use crate::cols::{Batch, ColType, Column, FieldSpec, SchemaSpec};

/// Record-header columns present in every table.
pub const HEADER_COLS: [&str; 4] = ["seq", "tsc", "proc_ns", "flags"];

/// Integer width and signedness of a leaf.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Prim {
    U8,
    U16,
    U32,
    U64,
    I8,
    I16,
    I32,
    I64,
}

impl Prim {
    fn col_type(self) -> ColType {
        match self {
            Prim::U8 => ColType::U8,
            Prim::U16 => ColType::U16,
            Prim::U32 => ColType::U32,
            Prim::U64 => ColType::U64,
            Prim::I8 => ColType::I8,
            Prim::I16 => ColType::I16,
            Prim::I32 => ColType::I32,
            Prim::I64 => ColType::I64,
        }
    }
}

/// Scaling of a fixed-point leaf (`*_f` columns and field metadata).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Scale {
    None,
    /// `Px` at the inst's `px_exp`.
    InstPx,
    /// `Qty` at the inst's `qty_exp`.
    InstQty,
    /// `Qty` at the `qty_exp` of the asset in leaf `asset_leaf`.
    AssetQty {
        asset_leaf: usize,
    },
    /// Fixed exponent (`Notional` 1e-6 USDT, `Rate` 1e-9).
    Fixed(i8),
}

/// One flattened leaf field.
#[derive(Clone, Debug)]
pub struct Leaf {
    /// Column name.
    pub name: String,
    /// Dotted path in the struct (`h.inst`).
    pub path: String,
    pub off: usize,
    pub width: usize,
    /// `None` = fixed-size bytes (`width` bytes).
    pub prim: Option<Prim>,
    pub ty: FieldTy,
    pub scale: Scale,
}

impl Leaf {
    pub fn col_type(&self) -> ColType {
        match self.prim {
            Some(p) => p.col_type(),
            None => ColType::FixedBinary(self.width),
        }
    }

    /// Little-endian integer at the leaf (zero beyond `b`), as i128 for any signedness.
    pub fn read_int(&self, b: &[u8]) -> i128 {
        let Some(p) = self.prim else { return 0 };
        let mut w = [0u8; 8];
        if let Some(s) = b.get(self.off..self.off + self.width) {
            if let Some(d) = w.get_mut(..s.len()) {
                d.copy_from_slice(s);
            }
        } else {
            return 0;
        }
        let u = u64::from_le_bytes(w);
        match p {
            Prim::U8 | Prim::U16 | Prim::U32 | Prim::U64 => i128::from(u),
            Prim::I8 => i128::from(u as u8 as i8),
            Prim::I16 => i128::from(u as u16 as i16),
            Prim::I32 => i128::from(u as u32 as i32),
            Prim::I64 => i128::from(u as i64),
        }
    }
}

fn prim_of(ty: &FieldTy) -> Option<Prim> {
    Some(match ty {
        FieldTy::U8 => Prim::U8,
        FieldTy::U16 => Prim::U16,
        FieldTy::U32 => Prim::U32,
        FieldTy::U64 | FieldTy::Tsc => Prim::U64,
        FieldTy::I8 => Prim::I8,
        FieldTy::I16 => Prim::I16,
        FieldTy::I32 => Prim::I32,
        FieldTy::I64 | FieldTy::Px | FieldTy::Qty | FieldTy::Notional | FieldTy::Rate | FieldTy::UtcNs => {
            Prim::I64
        }
        FieldTy::Id(k) => match k.width() {
            1 => Prim::U8,
            2 => Prim::U16,
            _ => Prim::U32,
        },
        FieldTy::Enum(d) | FieldTy::Bits(d) => match d.width {
            1 => Prim::U8,
            2 => Prim::U16,
            4 => Prim::U32,
            _ => Prim::U64,
        },
        FieldTy::Bytes(_) | FieldTy::Pad(_) | FieldTy::Nested(_) => return None,
    })
}

/// Flattens `fields` (offset `base`, path prefix `prefix`) into leaves (padding dropped).
fn flatten(fields: &[FieldDesc], base: usize, prefix: &str, out: &mut Vec<Leaf>) {
    for f in fields {
        let path = if prefix.is_empty() {
            f.name.to_string()
        } else {
            format!("{prefix}.{}", f.name)
        };
        let off = base + usize::from(f.offset);
        match f.ty {
            FieldTy::Pad(_) => {}
            FieldTy::Nested(inner) => flatten(inner, off, &path, out),
            ty => out.push(Leaf {
                name: f.name.to_string(),
                path,
                off,
                width: usize::from(ty.width()),
                prim: prim_of(&ty),
                ty,
                scale: Scale::None,
            }),
        }
    }
}

/// Unique column names: a leaf name that repeats or hits a reserved name uses its path with `_`,
/// and `body_` in front if that still collides.
fn uniquify(leaves: &mut [Leaf], reserved: &[&str]) {
    let names: Vec<String> = leaves.iter().map(|l| l.name.clone()).collect();
    let dup = |n: &str| names.iter().filter(|x| x.as_str() == n).count() > 1 || reserved.contains(&n);
    let mut used: Vec<String> = Vec::new();
    for l in leaves.iter_mut() {
        if dup(&l.name) {
            let p = l.path.replace('.', "_");
            l.name = if reserved.contains(&p.as_str()) || used.contains(&p) {
                format!("body_{p}")
            } else {
                p
            };
        }
        used.push(l.name.clone());
    }
}

/// Assigns fixed-point scaling (README "Arrow 映射": `Px`/`Qty` by inst; a fee in another asset by
/// its asset column; a kind without inst but with an asset field: by that asset).
fn assign_scale(leaves: &mut [Leaf]) {
    let inst = leaves.iter().position(|l| l.ty == FieldTy::Id(IdKind::Inst));
    let asset = leaves.iter().position(|l| l.ty == FieldTy::Id(IdKind::Asset));
    let fee_asset = leaves.iter().position(|l| l.path.ends_with("fee_asset"));
    for i in 0..leaves.len() {
        let (ty, name) = match leaves.get(i) {
            Some(l) => (l.ty, l.name.clone()),
            None => continue,
        };
        let s = match ty {
            FieldTy::Px if inst.is_some() => Scale::InstPx,
            FieldTy::Qty => {
                if name == "fee" && fee_asset.is_some() {
                    Scale::AssetQty {
                        asset_leaf: fee_asset.unwrap_or(0),
                    }
                } else if inst.is_some() {
                    Scale::InstQty
                } else if let Some(a) = asset {
                    Scale::AssetQty { asset_leaf: a }
                } else {
                    Scale::None
                }
            }
            FieldTy::Notional => Scale::Fixed(-6),
            FieldTy::Rate => Scale::Fixed(-9),
            _ => Scale::None,
        };
        if let Some(l) = leaves.get_mut(i) {
            l.scale = s;
        }
    }
}

/// Repeated tail entries of a Var/Multi kind (`list<struct>`).
#[derive(Clone, Debug)]
pub struct Entries {
    pub leaves: Vec<Leaf>,
    pub size: usize,
    /// `(offset, width)` of the count fields in the prefix; the count is their sum.
    pub counts: &'static [(usize, usize)],
}

impl Entries {
    pub fn count(&self, prefix: &[u8]) -> usize {
        self.counts
            .iter()
            .map(|&(o, w)| {
                let mut b = [0u8; 8];
                if let (Some(s), Some(d)) = (prefix.get(o..o + w), b.get_mut(..w)) {
                    d.copy_from_slice(s);
                }
                u64::from_le_bytes(b) as usize
            })
            .sum()
    }
}

/// Column map of one kind.
#[derive(Clone, Debug)]
pub struct KindMap {
    pub info: &'static KindInfo,
    /// Body size (Fixed) or prefix size (Var/Multi) of this build.
    pub prefix: usize,
    pub leaves: Vec<Leaf>,
    pub entries: Option<Entries>,
    /// `body` column: the whole logical body (variable records given as raw bytes).
    pub body_col: bool,
    /// `payload` column: the bytes after the prefix (`Advice`).
    pub payload_col: bool,
    pub inst_leaf: Option<usize>,
    pub account_leaf: Option<usize>,
    pub ex_leaf: Option<usize>,
    pub hw_leaf: Option<usize>,
}

fn struct_fields(name: &str) -> Option<&'static StructDesc> {
    hft_event::schema::catalog()
        .structs
        .iter()
        .find(|s| s.name == name)
}

/// The column map of `info`.
pub fn kind_map(info: &'static KindInfo) -> KindMap {
    let prefix = match info.layout {
        Layout::Fixed { size } => usize::from(size),
        Layout::Var { prefix, .. } | Layout::Multi { prefix } => usize::from(prefix),
    };
    let mut leaves = Vec::new();
    flatten(info.fields, 0, "", &mut leaves);
    let entries_of = |sname: &str, counts: &'static [(usize, usize)]| {
        struct_fields(sname).map(|s| {
            let mut l = Vec::new();
            flatten(s.fields, 0, "", &mut l);
            uniquify(&mut l, &[]);
            Entries {
                leaves: l,
                size: usize::from(s.size),
                counts,
            }
        })
    };
    let k = info.kind;
    let entries = if k == EventKind::L2_BATCH {
        entries_of("Level", &[(72, 2), (74, 2)])
    } else if k == EventKind::L2_SNAPSHOT {
        entries_of("Level", &[(68, 2), (70, 2)])
    } else if k == EventKind::MARGIN_REPORT {
        entries_of("MarginItem", &[(4, 2)])
    } else if k == EventKind::ACCOUNT_CONFIG_REPORT {
        entries_of("LeverageItem", &[(6, 2)])
    } else {
        None
    };
    let body_col = [
        EventKind::CONTROL_CMD,
        EventKind::CONFIG_BLOB,
        EventKind::STATE_SNAPSHOT,
        EventKind::ADVICE,
        EventKind::RAW_RX,
        EventKind::RAW_TX,
    ]
    .contains(&k)
        || (entries.is_none() && !matches!(info.layout, Layout::Fixed { .. }));
    let payload_col = k == EventKind::ADVICE;
    let mut reserved: Vec<&str> = HEADER_COLS.to_vec();
    reserved.extend(["entries", "body", "payload", "account_kind", "_missing"]);
    uniquify(&mut leaves, &reserved);
    assign_scale(&mut leaves);
    let find = |pred: &dyn Fn(&Leaf) -> bool| leaves.iter().position(pred);
    let inst_leaf = find(&|l| l.ty == FieldTy::Id(IdKind::Inst) && (l.path == "inst" || l.path == "h.inst"));
    let account_leaf = find(&|l| l.ty == FieldTy::Id(IdKind::Account) && l.path == "account");
    let ex_leaf = find(&|l| l.path == "ex_ts_ns" || l.path == "h.ex_ts_ns");
    let hw_leaf = find(&|l| l.path == "hw_rx_ns" || l.path == "h.hw_rx_ns");
    KindMap {
        info,
        prefix,
        leaves,
        entries,
        body_col,
        payload_col,
        inst_leaf,
        account_leaf,
        ex_leaf,
        hw_leaf,
    }
}

/// Exponents known for this query (field metadata and `*_f` columns).
#[derive(Clone, Debug, Default)]
pub struct Exps {
    /// inst → (px_exp, qty_exp), for the insts the query can return.
    pub inst: BTreeMap<u32, (i8, i8)>,
    /// asset → qty_exp.
    pub asset: BTreeMap<u16, i8>,
    /// account → `"Live"` / `"Sim"`.
    pub account_kind: BTreeMap<u8, &'static str>,
}

/// What one column holds.
#[derive(Clone, Debug)]
pub enum ColSrc {
    Seq,
    Tsc,
    ProcNs,
    Flags,
    Leaf(usize),
    Entries,
    Payload,
    Body,
    AccountKind,
    Missing,
    Float(usize),
}

/// Output columns of one query, in output order.
#[derive(Clone, Debug)]
pub struct Plan {
    pub map: Arc<KindMap>,
    pub cols: Vec<(String, ColSrc)>,
    pub schema: Arc<SchemaSpec>,
    pub exps: Arc<Exps>,
    /// Leaves whose values are read (all leaves, also for filters and `*_f`).
    pub missing_bits: usize,
}

/// Every available column of `map` (names in order) for the given options.
pub fn all_columns(map: &KindMap, decimals: bool, missing: bool) -> Vec<(String, ColSrc)> {
    let mut v: Vec<(String, ColSrc)> = vec![
        ("seq".into(), ColSrc::Seq),
        ("tsc".into(), ColSrc::Tsc),
        ("proc_ns".into(), ColSrc::ProcNs),
        ("flags".into(), ColSrc::Flags),
    ];
    for (i, l) in map.leaves.iter().enumerate() {
        v.push((l.name.clone(), ColSrc::Leaf(i)));
    }
    if map.entries.is_some() {
        v.push(("entries".into(), ColSrc::Entries));
    }
    if map.payload_col {
        v.push(("payload".into(), ColSrc::Payload));
    }
    if map.body_col {
        v.push(("body".into(), ColSrc::Body));
    }
    if map.account_leaf.is_some() {
        v.push(("account_kind".into(), ColSrc::AccountKind));
    }
    if missing {
        v.push(("_missing".into(), ColSrc::Missing));
    }
    if decimals {
        for (i, l) in map.leaves.iter().enumerate() {
            if l.scale != Scale::None {
                v.push((format!("{}_f", l.name), ColSrc::Float(i)));
            }
        }
    }
    v
}

fn exp_json<K: std::fmt::Display, V: std::fmt::Display>(m: impl Iterator<Item = (K, V)>) -> String {
    let parts: Vec<String> = m.map(|(k, v)| format!("\"{k}\": {v}")).collect();
    format!("{{{}}}", parts.join(", "))
}

fn leaf_field(map: &KindMap, l: &Leaf, exps: &Exps) -> FieldSpec {
    let mut md = vec![("hft.path".to_string(), l.path.clone())];
    match &l.scale {
        Scale::InstPx => {
            md.push(("px_exp".into(), exp_json(exps.inst.iter().map(|(k, v)| (k, v.0)))));
        }
        Scale::InstQty => {
            md.push((
                "qty_exp".into(),
                exp_json(exps.inst.iter().map(|(k, v)| (k, v.1))),
            ));
        }
        Scale::AssetQty { asset_leaf } => {
            if let Some(a) = map.leaves.get(*asset_leaf) {
                md.push(("exp_asset_col".into(), a.name.clone()));
            }
            md.push(("qty_exp_by_asset".into(), exp_json(exps.asset.iter())));
        }
        Scale::Fixed(e) => {
            md.push(("exp".into(), e.to_string()));
        }
        Scale::None => {}
    }
    let mut f = FieldSpec::new(&l.name, l.col_type());
    f.metadata = md;
    f
}

fn entries_type(e: &Entries) -> ColType {
    ColType::ListOfStruct(
        e.leaves
            .iter()
            .map(|l| FieldSpec::new(&l.name, l.col_type()))
            .collect(),
    )
}

/// Type of the `_missing` column (one bit per leaf).
pub fn missing_type(n: usize) -> ColType {
    if n <= 64 {
        ColType::U64
    } else {
        ColType::FixedBinary(n.div_ceil(8))
    }
}

/// Builds the output plan; `columns` (when given) selects and orders the columns.
pub fn plan(
    map: Arc<KindMap>,
    decimals: bool,
    missing: bool,
    columns: Option<&[String]>,
    exps: Arc<Exps>,
) -> Result<Plan, String> {
    let all = all_columns(&map, decimals, missing);
    let cols = match columns {
        None => all,
        Some(sel) => {
            let mut out = Vec::with_capacity(sel.len());
            for (i, name) in sel.iter().enumerate() {
                if sel.iter().take(i).any(|p| p == name) {
                    return Err(format!("column `{name}` given twice"));
                }
                match all.iter().find(|(n, _)| n == name) {
                    Some(c) => out.push(c.clone()),
                    None => return Err(format!("unknown column `{name}` for {}", map.info.name)),
                }
            }
            out
        }
    };
    let fields: Vec<FieldSpec> = cols
        .iter()
        .map(|(name, src)| match src {
            ColSrc::Seq | ColSrc::Tsc => FieldSpec::new(name, ColType::U64),
            ColSrc::ProcNs => FieldSpec::new(name, ColType::I64),
            ColSrc::Flags => FieldSpec::new(name, ColType::U16),
            ColSrc::Leaf(i) => match map.leaves.get(*i) {
                Some(l) => leaf_field(&map, l, &exps),
                None => FieldSpec::new(name, ColType::U8),
            },
            ColSrc::Entries => match &map.entries {
                Some(e) => FieldSpec::new(name, entries_type(e)),
                None => FieldSpec::new(name, ColType::ListOfStruct(Vec::new())),
            },
            ColSrc::Payload | ColSrc::Body => FieldSpec::new(name, ColType::LargeBinary),
            ColSrc::AccountKind => FieldSpec::new(name, ColType::Utf8).nullable(),
            ColSrc::Missing => FieldSpec::new(name, missing_type(map.leaves.len())),
            ColSrc::Float(_) => FieldSpec::new(name, ColType::F64).nullable(),
        })
        .collect();
    let schema = Arc::new(SchemaSpec {
        fields,
        metadata: vec![
            ("kind".to_string(), map.info.name.to_string()),
            ("kind_id".to_string(), map.info.kind.0.to_string()),
        ],
    });
    Ok(Plan {
        missing_bits: map.leaves.len(),
        map,
        cols,
        schema,
        exps,
    })
}

/// One record to append.
#[derive(Copy, Clone, Debug)]
pub struct Row<'a> {
    pub seq: u64,
    pub tsc: u64,
    pub proc_ns: i64,
    pub flags: u16,
    /// Logical body (multi-part records reassembled).
    pub body: &'a [u8],
    /// Prefix size of the writer that wrote the record (≤ or ≥ this build's).
    pub prefix: usize,
}

fn buf_for(dt_prim: Option<Prim>, width: usize, cap: usize) -> Column {
    match dt_prim {
        Some(Prim::U8) => Column::U8(Vec::with_capacity(cap)),
        Some(Prim::U16) => Column::U16(Vec::with_capacity(cap)),
        Some(Prim::U32) => Column::U32(Vec::with_capacity(cap)),
        Some(Prim::U64) => Column::U64(Vec::with_capacity(cap)),
        Some(Prim::I8) => Column::I8(Vec::with_capacity(cap)),
        Some(Prim::I16) => Column::I16(Vec::with_capacity(cap)),
        Some(Prim::I32) => Column::I32(Vec::with_capacity(cap)),
        Some(Prim::I64) => Column::I64(Vec::with_capacity(cap)),
        None => Column::Fixed(width, Vec::with_capacity(cap * width)),
    }
}

trait Fill {
    fn reserve_to(&mut self, rows: usize);
    fn push_leaf(&mut self, l: &Leaf, b: &[u8]);
}

fn missing_col(n: usize, cap: usize) -> Column {
    if n <= 64 {
        Column::U64(Vec::with_capacity(cap))
    } else {
        Column::Fixed(n.div_ceil(8), Vec::new())
    }
}

impl Fill for Column {
    fn reserve_to(&mut self, rows: usize) {
        fn r<T>(v: &mut Vec<T>, n: usize) {
            if v.capacity() < n {
                v.reserve_exact(n - v.len());
            }
        }
        match self {
            Column::U8(v) => r(v, rows),
            Column::U16(v) => r(v, rows),
            Column::U32(v) => r(v, rows),
            Column::U64(v) => r(v, rows),
            Column::I8(v) => r(v, rows),
            Column::I16(v) => r(v, rows),
            Column::I32(v) => r(v, rows),
            Column::I64(v) => r(v, rows),
            Column::Fixed(w, v) => r(v, rows * *w),
            Column::Bin(o, _) => r(o, rows + 1),
            Column::Str(v) => r(v, rows),
            Column::Bool(v) => r(v, rows),
            Column::F64(v, m) => {
                r(v, rows);
                r(m, rows);
            }
            Column::Dec(v) => r(v, rows),
            Column::List(o, _) => r(o, rows + 1),
        }
    }

    fn push_leaf(&mut self, l: &Leaf, b: &[u8]) {
        let x = l.read_int(b);
        match self {
            Column::U8(v) => v.push(x as u8),
            Column::U16(v) => v.push(x as u16),
            Column::U32(v) => v.push(x as u32),
            Column::U64(v) => v.push(x as u64),
            Column::I8(v) => v.push(x as i8),
            Column::I16(v) => v.push(x as i16),
            Column::I32(v) => v.push(x as i32),
            Column::I64(v) => v.push(x as i64),
            Column::Fixed(w, v) => {
                let start = v.len();
                v.resize(start + *w, 0);
                if let (Some(src), Some(dst)) = (b.get(l.off..), v.get_mut(start..start + *w)) {
                    let n = src.len().min(*w);
                    if let (Some(s), Some(d)) = (src.get(..n), dst.get_mut(..n)) {
                        d.copy_from_slice(s);
                    }
                }
            }
            _ => {}
        }
    }
}

/// Column builder of one batch.
#[derive(Debug)]
pub struct Builder {
    plan: Plan,
    bufs: Vec<Column>,
    rows: usize,
    limit: usize,
    cap: usize,
}

const START_CAP: usize = 4096;

/// `raw × 10^e` with one correctly rounded division or multiplication (10^n exact for n ≤ 22).
fn scaled(raw: i64, e: i8) -> f64 {
    let mut p = 1.0f64;
    for _ in 0..e.unsigned_abs() {
        p *= 10.0;
    }
    if e >= 0 { raw as f64 * p } else { raw as f64 / p }
}

impl Builder {
    /// A builder for at most `limit` rows.
    pub fn new(plan: Plan, limit: usize) -> Builder {
        let cap = limit.clamp(1, START_CAP);
        let bufs = plan
            .cols
            .iter()
            .map(|(_, src)| match src {
                ColSrc::Seq | ColSrc::Tsc => Column::U64(Vec::with_capacity(cap)),
                ColSrc::ProcNs => Column::I64(Vec::with_capacity(cap)),
                ColSrc::Flags => Column::U16(Vec::with_capacity(cap)),
                ColSrc::Leaf(i) => {
                    let l = plan.map.leaves.get(*i);
                    buf_for(l.and_then(|l| l.prim), l.map_or(0, |l| l.width), cap)
                }
                ColSrc::Entries => {
                    let children = plan
                        .map
                        .entries
                        .as_ref()
                        .map(|e| e.leaves.iter().map(|l| buf_for(l.prim, l.width, 0)).collect())
                        .unwrap_or_default();
                    Column::List(vec![0], children)
                }
                ColSrc::Payload | ColSrc::Body => Column::Bin(vec![0], Vec::new()),
                ColSrc::AccountKind => Column::Str(Vec::with_capacity(cap)),
                ColSrc::Missing => missing_col(plan.map.leaves.len(), cap),
                ColSrc::Float(_) => Column::F64(Vec::with_capacity(cap), Vec::with_capacity(cap)),
            })
            .collect();
        Builder {
            plan,
            bufs,
            rows: 0,
            limit: limit.max(1),
            cap,
        }
    }

    pub fn len(&self) -> usize {
        self.rows
    }

    pub fn is_empty(&self) -> bool {
        self.rows == 0
    }

    pub fn is_full(&self) -> bool {
        self.rows >= self.limit
    }

    pub fn plan(&self) -> &Plan {
        &self.plan
    }

    fn grow(&mut self) {
        if self.rows < self.cap {
            return;
        }
        let next = (self.cap * 2).min(self.limit).max(self.rows + 1);
        for b in &mut self.bufs {
            b.reserve_to(next);
        }
        self.cap = next;
    }

    /// Appends one record.
    pub fn push(&mut self, r: &Row<'_>) {
        self.grow();
        let map = self.plan.map.clone();
        let body = r.body;
        // Prefix as written: shorter (older writer) → missing leaves read as 0.
        let have = if matches!(map.info.layout, Layout::Fixed { .. }) {
            body.len()
        } else {
            r.prefix.min(body.len())
        };
        let pre = body.get(..have.min(map.prefix)).unwrap_or(&[]);
        let tail = body.get(r.prefix.min(body.len())..).unwrap_or(&[]);
        for ((_, src), buf) in self.plan.cols.iter().zip(self.bufs.iter_mut()) {
            match (src, buf) {
                (ColSrc::Seq, Column::U64(v)) => v.push(r.seq),
                (ColSrc::Tsc, Column::U64(v)) => v.push(r.tsc),
                (ColSrc::ProcNs, Column::I64(v)) => v.push(r.proc_ns),
                (ColSrc::Flags, Column::U16(v)) => v.push(r.flags),
                (ColSrc::Leaf(i), b) => {
                    if let Some(l) = map.leaves.get(*i) {
                        b.push_leaf(l, pre);
                    }
                }
                (ColSrc::Entries, Column::List(o, children)) => {
                    if let Some(e) = &map.entries {
                        let n = e.count(pre);
                        let mut k = 0;
                        while k < n {
                            let Some(item) = tail.get(k * e.size..(k + 1) * e.size) else {
                                break;
                            };
                            for (l, c) in e.leaves.iter().zip(children.iter_mut()) {
                                c.push_leaf(l, item);
                            }
                            k += 1;
                        }
                        let last = o.last().copied().unwrap_or(0);
                        o.push(last + k as i32);
                    }
                }
                (ColSrc::Payload, Column::Bin(o, d)) => {
                    let len = map
                        .leaves
                        .iter()
                        .find(|l| l.path == "len")
                        .map_or(tail.len(), |l| (l.read_int(pre) as usize).min(tail.len()));
                    d.extend_from_slice(tail.get(..len).unwrap_or(&[]));
                    o.push(d.len() as i64);
                }
                (ColSrc::Body, Column::Bin(o, d)) => {
                    d.extend_from_slice(body);
                    o.push(d.len() as i64);
                }
                (ColSrc::AccountKind, Column::Str(v)) => {
                    let a = map
                        .account_leaf
                        .and_then(|i| map.leaves.get(i))
                        .map_or(0, |l| l.read_int(pre) as u8);
                    v.push(self.plan.exps.account_kind.get(&a).map(|s| (*s).to_string()));
                }
                (ColSrc::Missing, col) => {
                    let n = map.leaves.len();
                    if let Column::U64(v) = col {
                        let mut bits = 0u64;
                        for (i, l) in map.leaves.iter().enumerate() {
                            if l.off + l.width > have {
                                bits |= 1 << i;
                            }
                        }
                        v.push(bits);
                    } else {
                        let mut bytes = vec![0u8; n.div_ceil(8)];
                        for (i, l) in map.leaves.iter().enumerate() {
                            if l.off + l.width > have
                                && let Some(b) = bytes.get_mut(i / 8)
                            {
                                *b |= 1 << (i % 8);
                            }
                        }
                        if let Column::Fixed(_, fixed) = col {
                            fixed.extend_from_slice(&bytes);
                        }
                    }
                }
                (ColSrc::Float(i), Column::F64(v, m)) => {
                    let (x, ok) = match map.leaves.get(*i) {
                        Some(l) => {
                            let raw = l.read_int(pre);
                            let exps = &self.plan.exps;
                            let exp = match &l.scale {
                                Scale::InstPx | Scale::InstQty => {
                                    let inst = map
                                        .inst_leaf
                                        .and_then(|j| map.leaves.get(j))
                                        .map_or(0, |il| il.read_int(pre) as u32);
                                    exps.inst
                                        .get(&inst)
                                        .map(|e| if l.scale == Scale::InstPx { e.0 } else { e.1 })
                                }
                                Scale::AssetQty { asset_leaf } => {
                                    let a = map
                                        .leaves
                                        .get(*asset_leaf)
                                        .map_or(0, |al| al.read_int(pre) as u16);
                                    exps.asset.get(&a).copied()
                                }
                                Scale::Fixed(e) => Some(*e),
                                Scale::None => None,
                            };
                            match exp {
                                Some(e) => (scaled(raw as i64, e), true),
                                None => (0.0, false),
                            }
                        }
                        None => (0.0, false),
                    };
                    v.push(x);
                    m.push(ok);
                }
                _ => {}
            }
        }
        self.rows += 1;
    }

    /// The finished batch; the builder is reset (same plan).
    pub fn finish(&mut self) -> Result<Batch, String> {
        let fresh = Builder::new(self.plan.clone(), self.limit);
        let old = std::mem::replace(self, fresh);
        Ok(Batch {
            schema: old.plan.schema.clone(),
            rows: old.rows,
            cols: old.bufs,
        })
    }
}
