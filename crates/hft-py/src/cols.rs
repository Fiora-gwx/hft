//! Column batches built in Rust and handed to pyarrow (README "Arrow 映射"): the schema
//! description, the column buffers and their conversion to `pyarrow.RecordBatch` (one copy of each
//! buffer into a Python `bytes` object, wrapped by `pyarrow.py_buffer` without another copy).
//! The arrow-rs crates are not used: their dependency closure carries `num-bigint` (banned by
//! deny.toml, N4) and `tiny-keccak` (CC0-1.0, not an allowed licence).

use std::sync::Arc;

use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyDict, PyList};
use zerocopy::IntoBytes;

/// Arrow type of a column.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ColType {
    U8,
    U16,
    U32,
    U64,
    I8,
    I16,
    I32,
    I64,
    F64,
    Bool,
    Utf8,
    FixedBinary(usize),
    LargeBinary,
    /// `decimal128(38, 0)`.
    Decimal128,
    /// `list<struct<fields>>` (non-null items).
    ListOfStruct(Vec<FieldSpec>),
}

impl ColType {
    /// Bytes per row of the value buffer (primitive and fixed-size types).
    pub fn width(&self) -> Option<usize> {
        Some(match self {
            ColType::U8 | ColType::I8 => 1,
            ColType::U16 | ColType::I16 => 2,
            ColType::U32 | ColType::I32 => 4,
            ColType::U64 | ColType::I64 | ColType::F64 => 8,
            ColType::Decimal128 => 16,
            ColType::FixedBinary(w) => *w,
            _ => return None,
        })
    }
}

/// One field of a schema.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FieldSpec {
    pub name: String,
    pub ty: ColType,
    pub nullable: bool,
    pub metadata: Vec<(String, String)>,
}

impl FieldSpec {
    pub fn new(name: &str, ty: ColType) -> FieldSpec {
        FieldSpec {
            name: name.to_string(),
            ty,
            nullable: false,
            metadata: Vec::new(),
        }
    }

    pub fn nullable(mut self) -> FieldSpec {
        self.nullable = true;
        self
    }
}

/// Schema of a batch stream.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SchemaSpec {
    pub fields: Vec<FieldSpec>,
    pub metadata: Vec<(String, String)>,
}

impl SchemaSpec {
    pub fn meta(&self, key: &str) -> Option<&str> {
        self.metadata
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.as_str())
    }
}

/// Values of one column.
#[derive(Clone, Debug, PartialEq)]
pub enum Column {
    U8(Vec<u8>),
    U16(Vec<u16>),
    U32(Vec<u32>),
    U64(Vec<u64>),
    I8(Vec<i8>),
    I16(Vec<i16>),
    I32(Vec<i32>),
    I64(Vec<i64>),
    /// Values and validity.
    F64(Vec<f64>, Vec<bool>),
    Bool(Vec<bool>),
    Str(Vec<Option<String>>),
    Fixed(usize, Vec<u8>),
    /// `large_binary`: offsets (rows + 1) and data.
    Bin(Vec<i64>, Vec<u8>),
    Dec(Vec<i128>),
    /// `list<struct>`: offsets (rows + 1) and one child column per struct field.
    List(Vec<i32>, Vec<Column>),
}

impl Column {
    /// Bytes held by the column's buffers.
    pub fn bytes(&self) -> usize {
        match self {
            Column::U8(v) => v.len(),
            Column::U16(v) => v.len() * 2,
            Column::U32(v) => v.len() * 4,
            Column::U64(v) => v.len() * 8,
            Column::I8(v) => v.len(),
            Column::I16(v) => v.len() * 2,
            Column::I32(v) => v.len() * 4,
            Column::I64(v) => v.len() * 8,
            Column::F64(v, m) => v.len() * 8 + m.len().div_ceil(8),
            Column::Bool(v) => v.len().div_ceil(8),
            Column::Str(v) => v.iter().map(|s| s.as_ref().map_or(0, String::len) + 4).sum(),
            Column::Fixed(_, v) => v.len(),
            Column::Bin(o, d) => o.len() * 8 + d.len(),
            Column::Dec(v) => v.len() * 16,
            Column::List(o, c) => o.len() * 4 + c.iter().map(Column::bytes).sum::<usize>(),
        }
    }

    /// Integer value at `row` (integer columns), for tests and readers of the Rust core.
    pub fn int(&self, row: usize) -> Option<i128> {
        Some(match self {
            Column::U8(v) => i128::from(*v.get(row)?),
            Column::U16(v) => i128::from(*v.get(row)?),
            Column::U32(v) => i128::from(*v.get(row)?),
            Column::U64(v) => i128::from(*v.get(row)?),
            Column::I8(v) => i128::from(*v.get(row)?),
            Column::I16(v) => i128::from(*v.get(row)?),
            Column::I32(v) => i128::from(*v.get(row)?),
            Column::I64(v) => i128::from(*v.get(row)?),
            Column::Dec(v) => *v.get(row)?,
            _ => return None,
        })
    }

    /// Address of the value buffer (`hft_py._testing.last_buffer_addrs` reports the Python copy).
    fn value_bytes(&self) -> Option<&[u8]> {
        Some(match self {
            Column::U8(v) => v.as_bytes(),
            Column::U16(v) => v.as_bytes(),
            Column::U32(v) => v.as_bytes(),
            Column::U64(v) => v.as_bytes(),
            Column::I8(v) => v.as_bytes(),
            Column::I16(v) => v.as_bytes(),
            Column::I32(v) => v.as_bytes(),
            Column::I64(v) => v.as_bytes(),
            Column::F64(v, _) => v.as_bytes(),
            Column::Fixed(_, v) => v,
            Column::Dec(v) => v.as_bytes(),
            _ => return None,
        })
    }
}

/// One batch: schema, row count and columns in schema order.
#[derive(Clone, Debug)]
pub struct Batch {
    pub schema: Arc<SchemaSpec>,
    pub rows: usize,
    pub cols: Vec<Column>,
}

impl Batch {
    /// The column called `name`.
    pub fn column(&self, name: &str) -> Option<&Column> {
        let i = self.schema.fields.iter().position(|f| f.name == name)?;
        self.cols.get(i)
    }

    /// Bytes held by all columns.
    pub fn bytes(&self) -> usize {
        self.cols.iter().map(Column::bytes).sum()
    }
}

// ------------------------------------------------------------------------------------- pyarrow

fn pa(py: Python<'_>) -> PyResult<Bound<'_, PyModule>> {
    py.import("pyarrow")
}

fn pa_type<'py>(pa: &Bound<'py, PyModule>, t: &ColType) -> PyResult<Bound<'py, PyAny>> {
    Ok(match t {
        ColType::U8 => pa.call_method0("uint8")?,
        ColType::U16 => pa.call_method0("uint16")?,
        ColType::U32 => pa.call_method0("uint32")?,
        ColType::U64 => pa.call_method0("uint64")?,
        ColType::I8 => pa.call_method0("int8")?,
        ColType::I16 => pa.call_method0("int16")?,
        ColType::I32 => pa.call_method0("int32")?,
        ColType::I64 => pa.call_method0("int64")?,
        ColType::F64 => pa.call_method0("float64")?,
        ColType::Bool => pa.call_method0("bool_")?,
        ColType::Utf8 => pa.call_method0("string")?,
        ColType::FixedBinary(w) => pa.call_method1("binary", (*w,))?,
        ColType::LargeBinary => pa.call_method0("large_binary")?,
        ColType::Decimal128 => pa.call_method1("decimal128", (38, 0))?,
        ColType::ListOfStruct(fields) => {
            let st = struct_type(pa, fields)?;
            let item = pa.call_method1("field", ("item", st, false))?;
            pa.call_method1("list_", (item,))?
        }
    })
}

fn struct_type<'py>(pa: &Bound<'py, PyModule>, fields: &[FieldSpec]) -> PyResult<Bound<'py, PyAny>> {
    let fs = PyList::empty(pa.py());
    for f in fields {
        fs.append(pa_field(pa, f)?)?;
    }
    pa.call_method1("struct", (fs,))
}

fn pa_field<'py>(pa: &Bound<'py, PyModule>, f: &FieldSpec) -> PyResult<Bound<'py, PyAny>> {
    let md = PyDict::new(pa.py());
    for (k, v) in &f.metadata {
        md.set_item(k, v)?;
    }
    let kw = PyDict::new(pa.py());
    kw.set_item("nullable", f.nullable)?;
    if !f.metadata.is_empty() {
        kw.set_item("metadata", md)?;
    }
    pa.call_method("field", (f.name.as_str(), pa_type(pa, &f.ty)?), Some(&kw))
}

/// `pyarrow.Schema` of `s`.
pub fn pa_schema<'py>(py: Python<'py>, s: &SchemaSpec) -> PyResult<Bound<'py, PyAny>> {
    let pa = pa(py)?;
    let fs = PyList::empty(py);
    for f in &s.fields {
        fs.append(pa_field(&pa, f)?)?;
    }
    let md = PyDict::new(py);
    for (k, v) in &s.metadata {
        md.set_item(k, v)?;
    }
    pa.call_method1("schema", (fs, md))
}

/// `pyarrow.py_buffer` over a Python copy of `b`; returns the buffer and the copy's address.
fn buffer<'py>(pa: &Bound<'py, PyModule>, b: &[u8]) -> PyResult<(Bound<'py, PyAny>, usize)> {
    let bytes = PyBytes::new(pa.py(), b);
    let addr = bytes.as_bytes().as_ptr() as usize;
    Ok((pa.call_method1("py_buffer", (bytes,))?, addr))
}

fn bitmap(valid: &[bool]) -> Vec<u8> {
    let mut out = vec![0u8; valid.len().div_ceil(8)];
    for (i, v) in valid.iter().enumerate() {
        if *v && let Some(b) = out.get_mut(i / 8) {
            *b |= 1 << (i % 8);
        }
    }
    out
}

fn pa_array<'py>(
    pa: &Bound<'py, PyModule>,
    col: &Column,
    ty: &ColType,
    rows: usize,
    addrs: &mut Vec<usize>,
) -> PyResult<Bound<'py, PyAny>> {
    let py = pa.py();
    let t = pa_type(pa, ty)?;
    let from_buffers = |bufs: Vec<Option<Bound<'py, PyAny>>>, nulls: usize| -> PyResult<Bound<'py, PyAny>> {
        let list = PyList::empty(py);
        for b in bufs {
            match b {
                Some(b) => list.append(b)?,
                None => list.append(py.None())?,
            }
        }
        pa.getattr("Array")?
            .call_method1("from_buffers", (t.clone(), rows, list, nulls))
    };
    match col {
        Column::Bool(v) => pa.call_method1("array", (v.clone(), t)),
        Column::Str(v) => pa.call_method1("array", (v.clone(), t)),
        Column::F64(v, m) => {
            let (vals, a) = buffer(pa, v.as_bytes())?;
            addrs.push(a);
            let nulls = m.iter().filter(|x| !**x).count();
            let mask = if nulls == 0 {
                None
            } else {
                Some(buffer(pa, &bitmap(m))?.0)
            };
            from_buffers(vec![mask, Some(vals)], nulls)
        }
        Column::Bin(o, d) => {
            let (ob, _) = buffer(pa, o.as_bytes())?;
            let (db, a) = buffer(pa, d)?;
            addrs.push(a);
            from_buffers(vec![None, Some(ob), Some(db)], 0)
        }
        Column::List(o, children) => {
            let ColType::ListOfStruct(fields) = ty else {
                return Err(pyo3::exceptions::PyTypeError::new_err(
                    "list column without a struct type",
                ));
            };
            let n = o.last().copied().unwrap_or(0) as usize;
            let arrays = PyList::empty(py);
            let fs = PyList::empty(py);
            for (c, f) in children.iter().zip(fields) {
                arrays.append(pa_array(pa, c, &f.ty, n, &mut Vec::new())?)?;
                fs.append(pa_field(pa, f)?)?;
            }
            let kw = PyDict::new(py);
            kw.set_item("fields", fs)?;
            let st = pa
                .getattr("StructArray")?
                .call_method("from_arrays", (arrays,), Some(&kw))?;
            let offsets = pa_array(
                pa,
                &Column::I32(o.clone()),
                &ColType::I32,
                o.len(),
                &mut Vec::new(),
            )?;
            let kw = PyDict::new(py);
            kw.set_item("type", t)?;
            pa.getattr("ListArray")?
                .call_method("from_arrays", (offsets, st), Some(&kw))
        }
        other => {
            let bytes = other.value_bytes().unwrap_or(&[]);
            let (vals, a) = buffer(pa, bytes)?;
            addrs.push(a);
            from_buffers(vec![None, Some(vals)], 0)
        }
    }
}

/// `pyarrow.RecordBatch` of `b` (`testing` builds record the value-buffer addresses).
pub fn pa_batch<'py>(py: Python<'py>, b: &Batch) -> PyResult<Bound<'py, PyAny>> {
    let pa = pa(py)?;
    let arrays = PyList::empty(py);
    let mut addrs = Vec::new();
    for (c, f) in b.cols.iter().zip(&b.schema.fields) {
        let mut a = Vec::new();
        arrays.append(pa_array(&pa, c, &f.ty, b.rows, &mut a)?)?;
        if let Some(x) = a.first() {
            addrs.push((f.name.clone(), *x));
        }
    }
    let kw = PyDict::new(py);
    kw.set_item("schema", pa_schema(py, &b.schema)?)?;
    let rb = pa
        .getattr("RecordBatch")?
        .call_method("from_arrays", (arrays,), Some(&kw))?;
    crate::testing::record_addrs(&addrs);
    Ok(rb)
}
