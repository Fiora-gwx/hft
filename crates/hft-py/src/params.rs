//! `hft_py.params`: strategy parameter blobs (SPB1) and `StrategyCmd` payloads through the SDK's
//! pure `params` module — the only codec (README "参数编码(`params`)"). This module checks names,
//! converts the caller's values to raw fixed-point values (text through SDK `parse_value`) and
//! assembles the hft-event SPB1 container around the body that SDK `encode_body` returns.

use std::path::Path;
use std::sync::Arc;

use hft_event::{GroupSpec, LegDef, ParamSource, ParamsErr, StrategyParamsHdr, parse_spb1};
use hft_strategy_sdk::params::{
    FieldRef, GROUP_IX_NONE, OwnedCmd, OwnedSchema, ParamError, ParamOverride, RawParams, SchemaView,
    UnitKind, decode_body, encode_body, encode_cmd as sdk_encode_cmd, field_ix,
    format_value as sdk_format_value, parse_value as sdk_parse_value,
};
use hft_types::{AccountId, AssetExp, AssetId, GroupId, InstId, StrategyId, VenueId};
use pyo3::exceptions::PyTypeError;
use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyDict, PyList, PyTuple};
use serde_json::Value as Json;
use zerocopy::IntoBytes;

use crate::error::{Error, Res};

// ------------------------------------------------------------------------------------------------
// Core
// ------------------------------------------------------------------------------------------------

/// A strategy schema loaded from `hft-core schemas --out` output.
#[derive(Debug)]
pub struct Schema {
    pub owned: OwnedSchema,
    pub kind_id: u16,
    pub name: String,
    pub schema_id: u32,
    pub version: u16,
    pub digest: u64,
    /// The parsed JSON (descriptors the SDK's `OwnedSchema` does not keep: notes, report, docs).
    pub json: Json,
}

/// A value given by the caller: decimal text (SDK `parse_value`) or a raw fixed-point value.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PVal {
    Text(String),
    Raw(i64),
}

/// One entry of the error list of `encode` (README: `variant`, `field`, `group_ix`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParamErr {
    pub variant: String,
    pub field: Option<String>,
    pub group_ix: Option<u16>,
}

/// `header` of `encode` (the schema fills `kind_id`, `schema_id`, `schema_ver`, `schema_digest`).
#[derive(Clone, Copy, Debug)]
pub struct HeaderIn {
    pub strategy: u8,
    pub account: u8,
    pub version: u64,
    pub source: ParamSource,
    pub generated_at_ns: i64,
    /// `None` → the largest group counter in `groups`.
    pub group_ctr_hwm: Option<u32>,
}

/// One `groups` item of `encode`: the `GroupSpec` and the optional asset exponents.
#[derive(Clone, Copy, Debug)]
pub struct GroupIn {
    pub spec: GroupSpec,
    pub qty_exp: Option<i8>,
    pub px_exp: Option<i8>,
}

/// Decoded SPB1 blob.
#[derive(Clone, Debug)]
pub struct Decoded {
    pub hdr: StrategyParamsHdr,
    pub groups: Vec<GroupSpec>,
    pub raw: RawParams,
}

fn read_text(p: &Path) -> Res<String> {
    std::fs::read_to_string(p).map_err(|e| Error::params("Schema", format!("{}: {e}", p.display())))
}

/// Parses one schema JSON text.
pub fn schema_from_json(text: &str, what: &str) -> Res<Schema> {
    let json: Json = serde_json::from_str(text)
        .map_err(|e| Error::params("Schema", format!("{what}: invalid JSON: {e}")))?;
    let owned = OwnedSchema::from_json(text)
        .map_err(|_| Error::params("Schema", format!("{what}: not a valid strategy schema")))?;
    let digest = owned.schema_digest(owned.version());
    Ok(Schema {
        kind_id: owned.kind_id(),
        name: owned.name().to_string(),
        schema_id: owned.id(),
        version: owned.version(),
        digest,
        owned,
        json,
    })
}

/// `load_schema(path)`.
pub fn load_schema(path: &Path) -> Res<Schema> {
    let text = read_text(path)?;
    schema_from_json(&text, &path.display().to_string())
}

/// `load_schemas(dir)`: `index.json` + the files it names, ascending `kind_id`.
pub fn load_schemas(dir: &Path) -> Res<Vec<Schema>> {
    let ip = dir.join("index.json");
    let text = read_text(&ip)?;
    let idx: Json = serde_json::from_str(&text)
        .map_err(|e| Error::params("Index", format!("{}: invalid JSON: {e}", ip.display())))?;
    let arr = idx
        .as_array()
        .ok_or_else(|| Error::params("Index", format!("{}: top level is not an array", ip.display())))?;
    let mut out: Vec<Schema> = Vec::with_capacity(arr.len());
    for e in arr {
        let bad = || Error::params("Index", format!("{}: malformed entry {e}", ip.display()));
        let u = |k: &str| e.get(k).and_then(Json::as_u64);
        let kind_id = u("kind_id").and_then(|v| u16::try_from(v).ok()).ok_or_else(bad)?;
        let name = e.get("name").and_then(Json::as_str).ok_or_else(bad)?;
        let file = e.get("file").and_then(Json::as_str).ok_or_else(bad)?;
        let schema_id = u("schema_id")
            .and_then(|v| u32::try_from(v).ok())
            .ok_or_else(bad)?;
        let ver = u("schema_version")
            .and_then(|v| u16::try_from(v).ok())
            .ok_or_else(bad)?;
        let digest = e
            .get("schema_digest")
            .and_then(Json::as_str)
            .and_then(|s| u64::from_str_radix(s, 16).ok())
            .ok_or_else(bad)?;
        if file.contains('/') || file.contains('\\') || file.contains("..") {
            return Err(bad());
        }
        let s = load_schema(&dir.join(file))?;
        if s.kind_id != kind_id || s.name != name || s.schema_id != schema_id || s.version != ver {
            return Err(Error::params(
                "Index",
                format!("{file}: kind_id/name/schema_id/schema_version differ from index.json"),
            ));
        }
        if s.digest != digest {
            return Err(Error::params(
                "Index",
                format!(
                    "{file}: schema digest {:016x} differs from index.json {digest:016x}",
                    s.digest
                ),
            ));
        }
        if out.iter().any(|o| o.kind_id >= kind_id) {
            return Err(Error::params(
                "Index",
                "index.json: kind_id not strictly ascending",
            ));
        }
        out.push(s);
    }
    Ok(out)
}

/// SDK error variant name.
pub fn variant_name(e: &ParamError) -> &'static str {
    match e {
        ParamError::SchemaMismatch { .. } => "SchemaMismatch",
        ParamError::SchemaTooNew { .. } => "SchemaTooNew",
        ParamError::BodyLen => "BodyLen",
        ParamError::OverrideOrder { .. } => "OverrideOrder",
        ParamError::Padding => "Padding",
        ParamError::Misaligned => "Misaligned",
        ParamError::OutOfRange { .. } => "OutOfRange",
        ParamError::BadEnum { .. } => "BadEnum",
        ParamError::NotPerGroup { .. } => "NotPerGroup",
        ParamError::GroupIxOutOfRange { .. } => "GroupIxOutOfRange",
        ParamError::TooManyGroups => "TooManyGroups",
        ParamError::GroupShape { .. } => "GroupShape",
        ParamError::Invalid { .. } => "Invalid",
        ParamError::BadText { .. } => "BadText",
        ParamError::UnknownOp { .. } => "UnknownOp",
    }
}

fn container_variant(e: ParamsErr) -> &'static str {
    match e {
        ParamsErr::Truncated => "Truncated",
        ParamsErr::BadLen => "BadLen",
        ParamsErr::BadGroup => "BadGroup",
        ParamsErr::BadLeg => "BadLeg",
        ParamsErr::Padding => "Padding",
        ParamsErr::DupGroup => "DupGroup",
        ParamsErr::BadHdr => "BadHdr",
        ParamsErr::Misaligned => "Misaligned",
    }
}

fn field_name(s: &dyn SchemaView, ix: u16) -> Option<String> {
    s.field(ix).map(|f| f.name.to_string())
}

fn gix(g: u16) -> Option<u16> {
    (g != GROUP_IX_NONE).then_some(g)
}

/// Exponent argument of SDK `parse_value` for a field (`Qty`/`Px` take the matching exponent).
fn exp_for(f: &FieldRef<'_>, qty_exp: Option<i8>, px_exp: Option<i8>) -> Option<AssetExp> {
    match f.unit {
        UnitKind::Qty => qty_exp.map(|q| AssetExp {
            px_exp: 0,
            qty_exp: q,
        }),
        UnitKind::Px => px_exp.map(|p| AssetExp {
            px_exp: p,
            qty_exp: 0,
        }),
        _ => None,
    }
}

/// Raw value of `v` for field `f` (text through SDK `parse_value`; no range check).
fn raw_of(f: &FieldRef<'_>, v: &PVal, qty_exp: Option<i8>, px_exp: Option<i8>) -> Result<i64, ParamError> {
    match v {
        PVal::Raw(x) => Ok(*x),
        PVal::Text(t) => sdk_parse_value(*f, t, exp_for(f, qty_exp, px_exp)),
    }
}

/// `encode`: `(blob, [])` or `(None, errors)`.
pub fn encode(
    s: &Schema,
    h: &HeaderIn,
    groups: &[GroupIn],
    defaults: &[(String, PVal)],
    overrides: &[(u16, String, PVal)],
) -> (Option<Vec<u8>>, Vec<ParamErr>) {
    let sv: &dyn SchemaView = &s.owned;
    let mut errs: Vec<ParamErr> = Vec::new();
    fn add(errs: &mut Vec<ParamErr>, variant: &str, field: Option<String>, group_ix: Option<u16>) {
        errs.push(ParamErr {
            variant: variant.to_string(),
            field,
            group_ix,
        });
    }
    let mut dvals: Vec<i64> = (0..sv.n_fields())
        .map(|i| sv.field(i).map_or(0, |f| f.default))
        .collect();
    for (name, v) in defaults {
        let Some(ix) = field_ix(&s.owned, name) else {
            add(&mut errs, "UnknownField", Some(name.clone()), None);
            continue;
        };
        let Some(f) = sv.field(ix) else { continue };
        match raw_of(&f, v, None, None) {
            Ok(x) => {
                if let Some(d) = dvals.get_mut(usize::from(ix)) {
                    *d = x;
                }
            }
            Err(e) => add(&mut errs, variant_name(&e), Some(name.clone()), None),
        }
    }
    let mut ovs: Vec<ParamOverride> = Vec::with_capacity(overrides.len());
    for (g, name, v) in overrides {
        let Some(ix) = field_ix(&s.owned, name) else {
            add(&mut errs, "UnknownField", Some(name.clone()), Some(*g));
            continue;
        };
        let Some(f) = sv.field(ix) else { continue };
        let (qe, pe) = groups
            .get(usize::from(*g))
            .map_or((None, None), |gi| (gi.qty_exp, gi.px_exp));
        match raw_of(&f, v, qe, pe) {
            Ok(x) => {
                ovs.push(ParamOverride::new(*g, ix, x));
            }
            Err(e) => add(&mut errs, variant_name(&e), Some(name.clone()), Some(*g)),
        }
    }
    let specs: Vec<GroupSpec> = groups.iter().map(|g| g.spec).collect();
    let n_groups = match u16::try_from(specs.len()) {
        Ok(n) => n,
        Err(_) => {
            add(&mut errs, "TooManyGroups", None, None);
            return (None, errs);
        }
    };
    let raw = RawParams {
        defaults: dvals,
        overrides: ovs.clone(),
    };
    let body = match encode_body(&s.owned, n_groups, &raw) {
        Ok(b) => Some(b),
        Err(e) => {
            let (field, group) = match e {
                ParamError::OutOfRange { field_ix, group_ix } => (field_name(sv, field_ix), gix(group_ix)),
                ParamError::BadEnum { field_ix } | ParamError::BadText { field_ix } => {
                    (field_name(sv, field_ix), None)
                }
                ParamError::NotPerGroup { field_ix } => {
                    let g = ovs.iter().find(|o| o.field_ix == field_ix).map(|o| o.group_ix);
                    (field_name(sv, field_ix), g)
                }
                ParamError::GroupIxOutOfRange { group_ix } => {
                    let f = ovs
                        .iter()
                        .find(|o| o.group_ix == group_ix)
                        .and_then(|o| field_name(sv, o.field_ix));
                    (f, Some(group_ix))
                }
                ParamError::OverrideOrder { ix } => {
                    let o = ovs.get(ix as usize);
                    (o.and_then(|o| field_name(sv, o.field_ix)), o.map(|o| o.group_ix))
                }
                _ => (None, None),
            };
            add(&mut errs, variant_name(&e), field, group);
            None
        }
    };
    let Some(body) = body.filter(|_| errs.is_empty()) else {
        return (None, errs);
    };
    let hwm = h
        .group_ctr_hwm
        .unwrap_or_else(|| specs.iter().map(|g| g.id.counter()).max().unwrap_or(0));
    let Ok(body_len) = u32::try_from(body.len()) else {
        add(&mut errs, "BodyLen", None, None);
        return (None, errs);
    };
    let hdr = StrategyParamsHdr {
        strategy: StrategyId(h.strategy),
        account: AccountId(h.account),
        kind_id: s.kind_id,
        schema_id: s.schema_id,
        schema_ver: s.version,
        source: h.source,
        _pad: 0,
        n_groups,
        _pad2: 0,
        version: h.version,
        generated_at_ns: h.generated_at_ns,
        body_len,
        group_ctr_hwm: hwm,
        schema_digest: s.digest,
    };
    let mut blob = hdr.as_bytes().to_vec();
    for g in &specs {
        blob.extend_from_slice(g.as_bytes());
    }
    blob.extend_from_slice(&body);
    if let Err(e) = spb1_view(&blob, |_| ()) {
        add(&mut errs, container_variant(e), None, None);
        return (None, errs);
    }
    (Some(blob), errs)
}

/// Runs `f` on the SPB1 view of an 8-byte aligned copy of `blob`.
fn spb1_view<T>(blob: &[u8], f: impl FnOnce(hft_event::SpbView<'_>) -> T) -> Result<T, ParamsErr> {
    let mut al = vec![0u64; blob.len().div_ceil(8)];
    if let Some(d) = al.as_mut_bytes().get_mut(..blob.len()) {
        d.copy_from_slice(blob);
    }
    let b = al.as_bytes().get(..blob.len()).unwrap_or(&[]);
    parse_spb1(b).map(f)
}

/// Container fields of an SPB1 blob (no schema needed).
pub fn parse_container(blob: &[u8]) -> Res<(StrategyParamsHdr, Vec<GroupSpec>, Vec<u8>)> {
    spb1_view(blob, |v| (*v.hdr, v.groups.to_vec(), v.body.to_vec()))
        .map_err(|e| Error::params(container_variant(e), "not a valid SPB1 blob"))
}

/// `decode`: container, schema selection (already done by the caller), body.
pub fn decode(s: &Schema, blob: &[u8]) -> Res<Decoded> {
    let (hdr, groups, body) = parse_container(blob)?;
    if hdr.kind_id != s.kind_id || hdr.schema_id != s.schema_id {
        return Err(Error::params(
            "SchemaMismatch",
            format!(
                "blob kind_id {} schema_id {} ≠ schema kind_id {} schema_id {}",
                hdr.kind_id, hdr.schema_id, s.kind_id, s.schema_id
            ),
        ));
    }
    let raw = decode_body(&s.owned, hdr.schema_ver, hdr.n_groups, &body).map_err(|e| Error::Params {
        msg: format!("body: {e:?}"),
        variant: variant_name(&e).to_string(),
        field: None,
        group_ix: None,
    })?;
    Ok(Decoded { hdr, groups, raw })
}

fn cmd_by_name<'a>(s: &'a Schema, op: &str) -> Res<&'a OwnedCmd> {
    s.owned
        .cmds()
        .iter()
        .find(|c| c.name == op)
        .ok_or_else(|| Error::params("UnknownOp", format!("command `{op}` is not in schema {}", s.name)))
}

fn perr(e: ParamError, field: Option<String>) -> Error {
    Error::Params {
        msg: format!("{e:?}"),
        variant: variant_name(&e).to_string(),
        field,
        group_ix: None,
    }
}

/// `encode_cmd`: the `StrategyCmd` payload of command `op`.
pub fn encode_cmd(s: &Schema, op: &str, values: &[(String, PVal)]) -> Res<Vec<u8>> {
    let c = cmd_by_name(s, op)?;
    for (k, _) in values {
        if !c.fields.iter().any(|f| &f.name == k) {
            return Err(Error::Params {
                msg: format!("command `{op}` has no field `{k}`"),
                variant: "UnknownField".into(),
                field: Some(k.clone()),
                group_ix: None,
            });
        }
    }
    let mut vals = Vec::with_capacity(c.fields.len());
    for (i, f) in c.fields.iter().enumerate() {
        let Some((_, v)) = values.iter().find(|(k, _)| k == &f.name) else {
            return Err(Error::Params {
                msg: format!("command `{op}`: missing field `{}`", f.name),
                variant: "BodyLen".into(),
                field: Some(f.name.clone()),
                group_ix: None,
            });
        };
        let fr = f.as_ref(i as u16);
        vals.push(raw_of(&fr, v, None, None).map_err(|e| perr(e, Some(f.name.clone())))?);
    }
    sdk_encode_cmd(c, &vals).map_err(|e| {
        let field = match e {
            ParamError::OutOfRange { field_ix, .. } | ParamError::BadEnum { field_ix } => {
                c.fields.get(usize::from(field_ix)).map(|f| f.name.clone())
            }
            _ => None,
        };
        perr(e, field)
    })
}

fn schema_field<'a>(s: &'a Schema, field: &str) -> Res<FieldRef<'a>> {
    field_ix(&s.owned, field)
        .and_then(|ix| s.owned.field(ix))
        .ok_or_else(|| Error::Params {
            msg: format!("field `{field}` is not in schema {}", s.name),
            variant: "UnknownField".into(),
            field: Some(field.to_string()),
            group_ix: None,
        })
}

fn exp_arg(exp: Option<i8>) -> Option<AssetExp> {
    exp.map(|e| AssetExp {
        px_exp: e,
        qty_exp: e,
    })
}

/// `parse_value`: text → raw value of a schema field, then `[min, max]`.
pub fn parse_value(s: &Schema, field: &str, text: &str, exp: Option<i8>) -> Res<i64> {
    let f = schema_field(s, field)?;
    let v = sdk_parse_value(f, text, exp_arg(exp)).map_err(|e| perr(e, Some(field.to_string())))?;
    if v < f.min || v > f.max {
        return Err(perr(
            ParamError::OutOfRange {
                field_ix: f.ix,
                group_ix: GROUP_IX_NONE,
            },
            Some(field.to_string()),
        ));
    }
    Ok(v)
}

/// `format_value`: raw value → display text (SDK `format_value`).
pub fn format_value(s: &Schema, field: &str, raw: i64, exp: Option<i8>) -> Res<String> {
    let f = schema_field(s, field)?;
    Ok(sdk_format_value(f, raw, exp_arg(exp)))
}

// ------------------------------------------------------------------------------------------------
// Python bindings
// ------------------------------------------------------------------------------------------------

/// A strategy parameter schema (`load_schema` / `load_schemas`); read-only.
#[pyclass(name = "Schema", module = "hft_py.params", frozen)]
#[derive(Debug)]
pub struct PySchema {
    pub inner: Arc<Schema>,
}

/// One error of `encode` (`variant`, `field`, `group_ix`).
#[pyclass(name = "Error", module = "hft_py.params", frozen, eq, skip_from_py_object)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PyParamErr {
    #[pyo3(get)]
    pub variant: String,
    #[pyo3(get)]
    pub field: Option<String>,
    #[pyo3(get)]
    pub group_ix: Option<u16>,
}

#[pymethods]
impl PyParamErr {
    fn __repr__(&self) -> String {
        format!(
            "Error(variant={:?}, field={:?}, group_ix={:?})",
            self.variant, self.field, self.group_ix
        )
    }
}

/// `types.MappingProxyType(d)`: a read-only view.
pub fn ro<'py>(py: Python<'py>, d: Bound<'py, PyDict>) -> PyResult<Bound<'py, PyAny>> {
    py.import("types")?.getattr("MappingProxyType")?.call1((d,))
}

/// JSON value → Python object (arrays become tuples, objects read-only mappings).
fn json_ro<'py>(py: Python<'py>, v: &Json) -> PyResult<Bound<'py, PyAny>> {
    Ok(match v {
        Json::Null => py.None().into_bound(py),
        Json::Bool(b) => b.into_pyobject(py)?.to_owned().into_any(),
        Json::Number(n) => match (n.as_i64(), n.as_u64()) {
            (Some(i), _) => i.into_pyobject(py)?.into_any(),
            (None, Some(u)) => u.into_pyobject(py)?.into_any(),
            _ => n.to_string().into_pyobject(py)?.into_any(),
        },
        Json::String(s) => s.into_pyobject(py)?.into_any(),
        Json::Array(a) => {
            let items = a.iter().map(|x| json_ro(py, x)).collect::<PyResult<Vec<_>>>()?;
            PyTuple::new(py, items)?.into_any()
        }
        Json::Object(o) => {
            let d = PyDict::new(py);
            for (k, x) in o {
                d.set_item(k, json_ro(py, x)?)?;
            }
            ro(py, d)?
        }
    })
}

#[pymethods]
impl PySchema {
    #[getter]
    fn kind_id(&self) -> u16 {
        self.inner.kind_id
    }
    #[getter]
    fn name(&self) -> String {
        self.inner.name.clone()
    }
    #[getter]
    fn schema_id(&self) -> u32 {
        self.inner.schema_id
    }
    #[getter]
    fn schema_version(&self) -> u16 {
        self.inner.version
    }
    /// 16 lowercase hex digits (as in `index.json`).
    #[getter]
    fn schema_digest(&self) -> String {
        format!("{:016x}", self.inner.digest)
    }
    /// Parameter fields in schema order (read-only mappings).
    #[getter]
    fn fields<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        self.desc(py, "fields")
    }
    /// Command descriptors.
    #[getter]
    fn cmds<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        self.desc(py, "cmds")
    }
    /// Note descriptors.
    #[getter]
    fn notes<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        self.desc(py, "notes")
    }
    /// Report descriptors.
    #[getter]
    fn report<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        self.desc(py, "report")
    }
    fn __repr__(&self) -> String {
        format!(
            "Schema(kind_id={}, name={:?}, schema_version={})",
            self.inner.kind_id, self.inner.name, self.inner.version
        )
    }
}

impl PySchema {
    fn desc<'py>(&self, py: Python<'py>, key: &str) -> PyResult<Bound<'py, PyAny>> {
        match self.inner.json.get(key) {
            Some(v @ Json::Array(_)) => json_ro(py, v),
            _ => Ok(PyTuple::empty(py).into_any()),
        }
    }
}

fn wrap(s: Schema) -> PySchema {
    PySchema { inner: Arc::new(s) }
}

/// Value of a parameter or command field: `str`, `int` or `decimal.Decimal`; `float` → `TypeError`.
pub fn pval(v: &Bound<'_, PyAny>) -> PyResult<PVal> {
    if let Ok(s) = v.extract::<String>() {
        return Ok(PVal::Text(s));
    }
    let tname = v.get_type().name()?.to_string();
    if tname == "float" {
        return Err(PyTypeError::new_err(
            "float values are rejected: use str, int or decimal.Decimal",
        ));
    }
    if tname == "Decimal" {
        let s: String = v
            .py()
            .import("builtins")?
            .getattr("format")?
            .call1((v, "f"))?
            .extract()?;
        return Ok(PVal::Text(s));
    }
    if let Ok(i) = v.extract::<i64>() {
        return Ok(PVal::Raw(i));
    }
    Err(PyTypeError::new_err(format!(
        "unsupported value type {tname}: use str, int or decimal.Decimal"
    )))
}

fn get_item<'py>(d: &Bound<'py, PyAny>, k: &str) -> PyResult<Option<Bound<'py, PyAny>>> {
    match d.get_item(k) {
        Ok(v) => Ok(Some(v)),
        Err(e) if e.is_instance_of::<pyo3::exceptions::PyKeyError>(d.py()) => Ok(None),
        Err(e) => Err(e),
    }
}

fn req<'py>(d: &Bound<'py, PyAny>, k: &str, what: &str) -> PyResult<Bound<'py, PyAny>> {
    get_item(d, k)?.ok_or_else(|| PyTypeError::new_err(format!("{what}: missing key `{k}`")))
}

fn source_of(v: &Bound<'_, PyAny>) -> PyResult<ParamSource> {
    let s: String = v.extract()?;
    match s.as_str() {
        "Pipeline" => Ok(ParamSource::Pipeline),
        "Manual" => Ok(ParamSource::Manual),
        _ => Err(Error::value(format!("source `{s}`: expected \"Pipeline\" or \"Manual\"")).into()),
    }
}

fn header_in(h: &Bound<'_, PyAny>) -> PyResult<HeaderIn> {
    Ok(HeaderIn {
        strategy: req(h, "strategy", "header")?.extract()?,
        account: req(h, "account", "header")?.extract()?,
        version: req(h, "version", "header")?.extract()?,
        source: source_of(&req(h, "source", "header")?)?,
        generated_at_ns: req(h, "generated_at_ns", "header")?.extract()?,
        group_ctr_hwm: match get_item(h, "group_ctr_hwm")? {
            Some(v) if !v.is_none() => Some(v.extract()?),
            _ => None,
        },
    })
}

fn leg_kind(v: &Bound<'_, PyAny>) -> PyResult<u8> {
    if let Ok(s) = v.extract::<String>() {
        return match s.as_str() {
            "Cex" => Ok(LegDef::CEX),
            "Chain" => Ok(LegDef::CHAIN),
            _ => Err(Error::value(format!("leg kind `{s}`: expected \"Cex\" or \"Chain\"")).into()),
        };
    }
    v.extract()
}

fn group_in(g: &Bound<'_, PyAny>) -> PyResult<GroupIn> {
    let legs_obj = req(g, "legs", "group")?;
    let legs_list: Vec<Bound<'_, PyAny>> = legs_obj.try_iter()?.collect::<PyResult<_>>()?;
    if legs_list.len() > 4 {
        return Err(Error::value("a group has at most 4 legs").into());
    }
    let mut legs = [LegDef {
        inst: InstId(0),
        venue: VenueId(0),
        kind: 0,
        _pad: 0,
        route: 0,
        _pad2: 0,
    }; 4];
    for (d, l) in legs.iter_mut().zip(&legs_list) {
        d.inst = InstId(req(l, "inst", "leg")?.extract()?);
        d.venue = VenueId(req(l, "venue", "leg")?.extract()?);
        d.kind = match get_item(l, "kind")? {
            Some(k) => leg_kind(&k)?,
            None => LegDef::CEX,
        };
        d.route = match get_item(l, "route")? {
            Some(r) => r.extract()?,
            None => 0,
        };
    }
    let open: bool = req(g, "open_allowed", "group")?.is_truthy()?;
    let opt_i8 = |k: &str| -> PyResult<Option<i8>> {
        match get_item(g, k)? {
            Some(v) if !v.is_none() => Ok(Some(v.extract()?)),
            _ => Ok(None),
        }
    };
    Ok(GroupIn {
        spec: GroupSpec {
            id: GroupId(req(g, "id", "group")?.extract()?),
            asset: AssetId(req(g, "asset", "group")?.extract()?),
            n_legs: legs_list.len() as u8,
            open_allowed: u8::from(open),
            legs,
            _rsv: 0,
        },
        qty_exp: opt_i8("qty_exp")?,
        px_exp: opt_i8("px_exp")?,
    })
}

fn source_name(s: ParamSource) -> &'static str {
    match s {
        ParamSource::Pipeline => "Pipeline",
        ParamSource::Manual => "Manual",
    }
}

/// `StrategyParamsHdr` as a dict.
pub fn hdr_dict<'py>(py: Python<'py>, h: &StrategyParamsHdr) -> PyResult<Bound<'py, PyDict>> {
    let d = PyDict::new(py);
    d.set_item("strategy", h.strategy.0)?;
    d.set_item("account", h.account.0)?;
    d.set_item("kind_id", h.kind_id)?;
    d.set_item("schema_id", h.schema_id)?;
    d.set_item("schema_ver", h.schema_ver)?;
    d.set_item("source", source_name(h.source))?;
    d.set_item("n_groups", h.n_groups)?;
    d.set_item("version", h.version)?;
    d.set_item("generated_at_ns", h.generated_at_ns)?;
    d.set_item("body_len", h.body_len)?;
    d.set_item("group_ctr_hwm", h.group_ctr_hwm)?;
    d.set_item("schema_digest", format!("{:016x}", h.schema_digest))?;
    Ok(d)
}

/// `GroupSpec` list as dicts.
pub fn groups_list<'py>(py: Python<'py>, gs: &[GroupSpec]) -> PyResult<Bound<'py, PyList>> {
    let out = PyList::empty(py);
    for g in gs {
        let d = PyDict::new(py);
        d.set_item("id", g.id.0)?;
        d.set_item("asset", g.asset.0)?;
        d.set_item("open_allowed", g.open_allowed != 0)?;
        let legs = PyList::empty(py);
        for l in g.legs.iter().take(usize::from(g.n_legs)) {
            let ld = PyDict::new(py);
            ld.set_item("inst", l.inst.0)?;
            ld.set_item("venue", l.venue.0)?;
            ld.set_item(
                "kind",
                match l.kind {
                    LegDef::CEX => "Cex",
                    LegDef::CHAIN => "Chain",
                    _ => "Unknown",
                },
            )?;
            if l.route != 0 {
                ld.set_item("route", l.route)?;
            }
            legs.append(ld)?;
        }
        d.set_item("legs", legs)?;
        out.append(d)?;
    }
    Ok(out)
}

fn named_values(d: Option<&Bound<'_, PyAny>>) -> PyResult<Vec<(String, PVal)>> {
    let Some(d) = d.filter(|d| !d.is_none()) else {
        return Ok(Vec::new());
    };
    let d = d.cast::<PyDict>()?;
    let mut out = Vec::with_capacity(d.len());
    for (k, v) in d.iter() {
        out.push((k.extract::<String>()?, pval(&v)?));
    }
    Ok(out)
}

#[pyfunction]
#[pyo3(name = "load_schema")]
fn load_schema_py(path: std::path::PathBuf) -> PyResult<PySchema> {
    Ok(wrap(load_schema(&path)?))
}

#[pyfunction]
#[pyo3(name = "load_schemas")]
fn load_schemas_py<'py>(py: Python<'py>, dir: std::path::PathBuf) -> PyResult<Bound<'py, PyDict>> {
    let d = PyDict::new(py);
    for s in load_schemas(&dir)? {
        d.set_item(s.kind_id, wrap(s))?;
    }
    Ok(d)
}

#[pyfunction]
#[pyo3(name = "encode", signature = (schema, header, groups, defaults=None, overrides=None))]
fn encode_py<'py>(
    py: Python<'py>,
    schema: PyRef<'_, PySchema>,
    header: &Bound<'py, PyAny>,
    groups: &Bound<'py, PyAny>,
    defaults: Option<&Bound<'py, PyAny>>,
    overrides: Option<&Bound<'py, PyAny>>,
) -> PyResult<(Option<Bound<'py, PyBytes>>, Vec<PyParamErr>)> {
    let h = header_in(header)?;
    let gs = groups
        .try_iter()?
        .map(|g| group_in(&g?))
        .collect::<PyResult<Vec<_>>>()?;
    let defs = named_values(defaults)?;
    let mut ovs = Vec::new();
    if let Some(o) = overrides.filter(|o| !o.is_none()) {
        for item in o.try_iter()? {
            let item = item?;
            let g: u16 = req(&item, "group_ix", "override")?.extract()?;
            let f: String = req(&item, "field", "override")?.extract()?;
            let v = pval(&req(&item, "value", "override")?)?;
            ovs.push((g, f, v));
        }
    }
    let (blob, errs) = encode(&schema.inner, &h, &gs, &defs, &ovs);
    let errs = errs
        .into_iter()
        .map(|e| PyParamErr {
            variant: e.variant,
            field: e.field,
            group_ix: e.group_ix,
        })
        .collect();
    Ok((blob.map(|b| PyBytes::new(py, &b)), errs))
}

/// The schema `decode` uses: a mapping by `kind_id` or a single schema.
fn pick_schema(schemas: &Bound<'_, PyAny>, blob: &[u8]) -> PyResult<Arc<Schema>> {
    let (hdr, _, _) = parse_container(blob)?;
    if let Ok(s) = schemas.cast::<PySchema>() {
        let s = s.get().inner.clone();
        if s.kind_id != hdr.kind_id || s.schema_id != hdr.schema_id {
            return Err(Error::params(
                "SchemaMismatch",
                format!(
                    "blob kind_id {} schema_id {} ≠ schema kind_id {} schema_id {}",
                    hdr.kind_id, hdr.schema_id, s.kind_id, s.schema_id
                ),
            )
            .into());
        }
        return Ok(s);
    }
    let found = match schemas.get_item(hdr.kind_id) {
        Ok(v) => Some(v),
        Err(e) if e.is_instance_of::<pyo3::exceptions::PyKeyError>(schemas.py()) => None,
        Err(e) => return Err(e),
    };
    match found {
        Some(v) => Ok(v.cast::<PySchema>()?.get().inner.clone()),
        None => Err(Error::params("Schema", format!("no schema for kind_id {}", hdr.kind_id)).into()),
    }
}

#[pyfunction]
#[pyo3(name = "decode")]
fn decode_py<'py>(py: Python<'py>, schemas: &Bound<'py, PyAny>, blob: &[u8]) -> PyResult<Bound<'py, PyDict>> {
    let s = pick_schema(schemas, blob)?;
    let d = decode(&s, blob)?;
    let out = PyDict::new(py);
    out.set_item("header", hdr_dict(py, &d.hdr)?)?;
    out.set_item("groups", groups_list(py, &d.groups)?)?;
    let defs = PyDict::new(py);
    for (i, v) in d.raw.defaults.iter().enumerate() {
        if let Some(f) = s.owned.field(i as u16) {
            defs.set_item(f.name, *v)?;
        }
    }
    out.set_item("defaults", defs)?;
    let ovs = PyList::empty(py);
    for o in &d.raw.overrides {
        let od = PyDict::new(py);
        od.set_item("group_ix", o.group_ix)?;
        od.set_item("field", s.owned.field(o.field_ix).map(|f| f.name.to_string()))?;
        od.set_item("value", o.value)?;
        ovs.append(od)?;
    }
    out.set_item("overrides", ovs)?;
    Ok(out)
}

#[pyfunction]
#[pyo3(name = "encode_cmd")]
fn encode_cmd_py<'py>(
    py: Python<'py>,
    schema: PyRef<'_, PySchema>,
    op: &str,
    values: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyBytes>> {
    let vals = named_values(Some(values))?;
    let b = encode_cmd(&schema.inner, op, &vals)?;
    Ok(PyBytes::new(py, &b))
}

#[pyfunction]
#[pyo3(name = "parse_value", signature = (schema, field, text, exp=None))]
fn parse_value_py(schema: PyRef<'_, PySchema>, field: &str, text: &str, exp: Option<i8>) -> PyResult<i64> {
    Ok(parse_value(&schema.inner, field, text, exp)?)
}

#[pyfunction]
#[pyo3(name = "format_value", signature = (schema, field, raw, exp=None))]
fn format_value_py(schema: PyRef<'_, PySchema>, field: &str, raw: i64, exp: Option<i8>) -> PyResult<String> {
    Ok(format_value(&schema.inner, field, raw, exp)?)
}

/// Builds the `hft_py.params` submodule.
pub fn module<'py>(py: Python<'py>) -> PyResult<Bound<'py, PyModule>> {
    let m = PyModule::new(py, "params")?;
    m.add_function(wrap_pyfunction!(load_schemas_py, &m)?)?;
    m.add_function(wrap_pyfunction!(load_schema_py, &m)?)?;
    m.add_function(wrap_pyfunction!(encode_py, &m)?)?;
    m.add_function(wrap_pyfunction!(decode_py, &m)?)?;
    m.add_function(wrap_pyfunction!(encode_cmd_py, &m)?)?;
    m.add_function(wrap_pyfunction!(parse_value_py, &m)?)?;
    m.add_function(wrap_pyfunction!(format_value_py, &m)?)?;
    m.add_class::<PyParamErr>()?;
    Ok(m)
}
