//! Exported feature columns (README "特征导出"): the column descriptors a strategy kind declares
//! (`Strategy::FEATURES`) and the row its `export_features` writes. Cold path: only the research
//! export (`replayer features`) reads them; nothing in the engine does.

/// Unit of an exported column; the column value is `raw × 10^exp` of that unit.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub enum Unit {
    /// Dimensionless (returns, imbalances, probabilities, standard deviations).
    Ratio,
    /// Price in the inst's `px_exp` (exp is relative to it).
    Px,
    /// Quantity in the inst's `qty_exp` (exp is relative to it).
    Qty,
    Ns,
    Count,
    /// UTC minute number (minutes since 1970-01-01).
    Minute,
}

impl Unit {
    /// Lower-case name (Parquet column metadata `unit`).
    pub const fn name(self) -> &'static str {
        match self {
            Unit::Ratio => "ratio",
            Unit::Px => "px",
            Unit::Qty => "qty",
            Unit::Ns => "ns",
            Unit::Count => "count",
            Unit::Minute => "minute",
        }
    }
}

/// One exported column: name, unit and decimal exponent. `NA` marks a missing value.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub struct ColumnDesc {
    pub name: &'static str,
    pub unit: Unit,
    pub exp: i8,
}

/// Missing value of an exported column.
pub const NA: i64 = i64::MIN;

/// A row of exported fixed-point columns, written in `COLUMNS` order into caller-owned storage.
#[derive(Debug)]
pub struct FeatureRow<'a> {
    vals: &'a mut [i64],
    n: usize,
}

impl<'a> FeatureRow<'a> {
    pub fn new(vals: &'a mut [i64]) -> FeatureRow<'a> {
        FeatureRow { vals, n: 0 }
    }
    /// Appends one value; values beyond the storage are dropped (counted in `len`).
    pub fn put(&mut self, v: i64) {
        if let Some(x) = self.vals.get_mut(self.n) {
            *x = v;
        }
        self.n += 1;
    }
    /// Appends `v` or `NA`.
    pub fn put_opt(&mut self, v: Option<i64>) {
        self.put(v.unwrap_or(NA));
    }
    /// Values written (may exceed the storage length).
    pub fn len(&self) -> usize {
        self.n
    }
    pub fn is_empty(&self) -> bool {
        self.n == 0
    }
}
