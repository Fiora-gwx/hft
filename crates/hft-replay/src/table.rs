//! Column tables written as Parquet (README "输出"): typed columns, one row group per file,
//! zstd compression, key-value metadata from `run.json`.

use std::path::Path;
use std::sync::Arc;

use arrow_array::{ArrayRef, Float64Array, Int64Array, RecordBatch, StringArray, UInt8Array, UInt64Array};
use arrow_schema::{DataType, Field, Schema};
use parquet::arrow::ArrowWriter;
use parquet::basic::{Compression, ZstdLevel};
use parquet::file::metadata::KeyValue;
use parquet::file::properties::WriterProperties;

/// One column's values.
#[derive(Clone, Debug, PartialEq)]
pub enum Col {
    I64(Vec<i64>),
    U64(Vec<u64>),
    U8(Vec<u8>),
    F64(Vec<f64>),
    Str(Vec<String>),
}

impl Col {
    fn len(&self) -> usize {
        match self {
            Col::I64(v) => v.len(),
            Col::U64(v) => v.len(),
            Col::U8(v) => v.len(),
            Col::F64(v) => v.len(),
            Col::Str(v) => v.len(),
        }
    }
    fn array(&self) -> (DataType, ArrayRef) {
        match self {
            Col::I64(v) => (DataType::Int64, Arc::new(Int64Array::from(v.clone()))),
            Col::U64(v) => (DataType::UInt64, Arc::new(UInt64Array::from(v.clone()))),
            Col::U8(v) => (DataType::UInt8, Arc::new(UInt8Array::from(v.clone()))),
            Col::F64(v) => (DataType::Float64, Arc::new(Float64Array::from(v.clone()))),
            Col::Str(v) => (DataType::Utf8, Arc::new(StringArray::from(v.clone()))),
        }
    }
}

/// A table under construction: named columns of equal length.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Table {
    pub cols: Vec<(&'static str, Col)>,
}

impl Table {
    pub fn new(cols: Vec<(&'static str, Col)>) -> Table {
        Table { cols }
    }
    pub fn rows(&self) -> usize {
        self.cols.first().map_or(0, |c| c.1.len())
    }
    /// Appends `other` (same columns in the same order).
    pub fn extend(&mut self, other: Table) -> Result<(), String> {
        if self.cols.is_empty() {
            *self = other;
            return Ok(());
        }
        if self.cols.len() != other.cols.len() {
            return Err("table columns differ".into());
        }
        for ((na, a), (nb, b)) in self.cols.iter_mut().zip(other.cols) {
            if *na != nb {
                return Err(format!("table column {na} vs {nb}"));
            }
            match (a, b) {
                (Col::I64(x), Col::I64(y)) => x.extend(y),
                (Col::U64(x), Col::U64(y)) => x.extend(y),
                (Col::U8(x), Col::U8(y)) => x.extend(y),
                (Col::F64(x), Col::F64(y)) => x.extend(y),
                (Col::Str(x), Col::Str(y)) => x.extend(y),
                _ => return Err(format!("table column {na}: type differs")),
            }
        }
        Ok(())
    }
    /// Writes the table to `path` (zstd level 3), with `meta` as file key-value metadata.
    pub fn write(&self, path: &Path, meta: &[(&str, String)]) -> Result<(), String> {
        let n = self.rows();
        let mut fields = Vec::with_capacity(self.cols.len());
        let mut arrays = Vec::with_capacity(self.cols.len());
        for (name, c) in &self.cols {
            if c.len() != n {
                return Err(format!("column {name}: {} rows, expected {n}", c.len()));
            }
            let (ty, a) = c.array();
            fields.push(Field::new(*name, ty, false));
            arrays.push(a);
        }
        let schema = Arc::new(Schema::new(fields));
        let batch = RecordBatch::try_new(schema.clone(), arrays).map_err(|e| e.to_string())?;
        let props = WriterProperties::builder()
            .set_compression(Compression::ZSTD(
                ZstdLevel::try_new(3).map_err(|e| e.to_string())?,
            ))
            .set_key_value_metadata(Some(
                meta.iter()
                    .map(|(k, v)| KeyValue::new((*k).to_string(), v.clone()))
                    .collect(),
            ))
            .build();
        let f = std::fs::File::create(path).map_err(|e| format!("{}: {e}", path.display()))?;
        let mut w = ArrowWriter::try_new(f, schema, Some(props)).map_err(|e| e.to_string())?;
        w.write(&batch).map_err(|e| e.to_string())?;
        w.close().map_err(|e| e.to_string())?;
        Ok(())
    }
}
