//! Python classes of the journal API: `open_journal`, `Journal` (`info`, `events`, `tail`,
//! `config_at`), the lazy batch reader with the Arrow PyCapsule stream export, the tail iterator
//! and the batch object (README "Python API").

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use pyo3::exceptions::{PyKeyError, PyOverflowError, PyStopIteration, PyTypeError, PyValueError};
use pyo3::prelude::*;
use pyo3::sync::PyOnceLock;
use pyo3::types::{PyBytes, PyDict, PyList, PyString};

use crate::cols::{Batch, ColType, Column, FieldSpec, SchemaSpec, pa_batch, pa_schema};
use crate::error::{Error, Res};
use crate::journal::{Events, Journal, QueryArgs, Tail, kind_by};

impl std::error::Error for Error {}

/// Python int → i64, values outside the range → `ValueError` (not `OverflowError`).
pub fn int_arg(v: &Bound<'_, PyAny>, what: &str) -> PyResult<i64> {
    if v.is_instance_of::<pyo3::types::PyBool>() {
        return Err(PyTypeError::new_err(format!("{what}: bool is not accepted")));
    }
    match v.extract::<i64>() {
        Ok(x) => Ok(x),
        Err(e) if e.is_instance_of::<PyOverflowError>(v.py()) => {
            Err(PyValueError::new_err(format!("{what}: out of the 64-bit range")))
        }
        Err(e) => Err(e),
    }
}

/// `t0`/`t1`: ns integer, zone-aware `datetime` or zone-qualified ISO string.
pub fn time_arg(v: Option<&Bound<'_, PyAny>>, what: &str) -> PyResult<Option<i64>> {
    let Some(v) = v.filter(|v| !v.is_none()) else {
        return Ok(None);
    };
    if let Ok(s) = v.cast::<PyString>() {
        return Ok(Some(crate::time::parse_iso(&s.to_string())?));
    }
    let py = v.py();
    let dt = py.import("datetime")?;
    if v.is_instance(&dt.getattr("datetime")?)? {
        if v.call_method0("utcoffset")?.is_none() {
            return Err(PyValueError::new_err(format!(
                "{what}: datetime without a time zone"
            )));
        }
        let utc = dt.getattr("timezone")?.getattr("utc")?;
        let epoch = dt.getattr("datetime")?.call1((1970, 1, 1, 0, 0, 0, 0, utc))?;
        let d = v.call_method1("__sub__", (epoch,))?;
        let days: i64 = d.getattr("days")?.extract()?;
        let secs: i64 = d.getattr("seconds")?.extract()?;
        let us: i64 = d.getattr("microseconds")?.extract()?;
        let ns = (days * 86_400 + secs)
            .checked_mul(1_000_000_000)
            .and_then(|x| x.checked_add(us * 1000))
            .ok_or_else(|| PyValueError::new_err(format!("{what}: out of range")))?;
        return Ok(Some(ns));
    }
    if v.is_instance(&dt.getattr("date")?)? {
        return Err(PyValueError::new_err(format!("{what}: a date has no time zone")));
    }
    Ok(Some(int_arg(v, what)?))
}

fn kind_arg(v: &Bound<'_, PyAny>) -> PyResult<&'static hft_event::KindInfo> {
    if let Ok(s) = v.cast::<PyString>() {
        return Ok(kind_by(Some(&s.to_string()), None)?);
    }
    let n = match v.extract::<i64>() {
        Ok(n) => n,
        Err(e) if e.is_instance_of::<PyOverflowError>(v.py()) => -1,
        Err(_) => return Err(PyTypeError::new_err("kind must be a str or an int")),
    };
    Ok(kind_by(None, Some(n))?)
}

fn insts_arg(v: Option<&Bound<'_, PyAny>>) -> PyResult<Option<Vec<u32>>> {
    let Some(v) = v.filter(|v| !v.is_none()) else {
        return Ok(None);
    };
    let one = |x: &Bound<'_, PyAny>| -> PyResult<u32> {
        let n = int_arg(x, "inst")?;
        u32::try_from(n).map_err(|_| PyValueError::new_err(format!("inst {n}: not an InstId")))
    };
    if v.extract::<i64>().is_ok() || v.is_instance_of::<pyo3::types::PyInt>() {
        return Ok(Some(vec![one(v)?]));
    }
    let mut out = Vec::new();
    for x in v.try_iter()? {
        out.push(one(&x?)?);
    }
    Ok(Some(out))
}

/// `open_journal(path, hot_root=None)`.
#[pyfunction]
#[pyo3(signature = (path, hot_root=None))]
pub fn open_journal(path: PathBuf, hot_root: Option<PathBuf>) -> PyResult<PyJournal> {
    let p = path.to_string_lossy().to_string();
    Ok(PyJournal {
        inner: Arc::new(Journal::open(&p, hot_root)?),
    })
}

/// An opened journal (read-only).
#[pyclass(name = "Journal", module = "hft_py", frozen)]
#[derive(Debug)]
pub struct PyJournal {
    pub inner: Arc<Journal>,
}

fn host_profile<'py>(py: Python<'py>, b: Option<&Vec<u8>>) -> PyResult<Bound<'py, PyAny>> {
    let Some(b) = b else {
        return Ok(py.None().into_bound(py));
    };
    let Some(info) = hft_event::kind_info(hft_event::EventKind::HOST_PROFILE) else {
        return Ok(py.None().into_bound(py));
    };
    let map = crate::arrow_map::kind_map(info);
    let d = PyDict::new(py);
    crate::decode::fill_leaves(py, &d, &map, b)?;
    Ok(d.into_any())
}

#[pymethods]
impl PyJournal {
    /// `journal_name`, `instance`, `segments`, `first_seq`, `last_seq`, `schema`, `host_profile`.
    fn info<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyDict>> {
        let j = self.inner.clone();
        let i = py.detach(move || j.info())?;
        let j = &self.inner;
        let d = PyDict::new(py);
        d.set_item("journal_name", &j.spec.name)?;
        d.set_item("instance", j.spec.instance.as_str())?;
        let segs = PyList::empty(py);
        for s in &i.segments {
            let sd = PyDict::new(py);
            sd.set_item("segment_no", s.no)?;
            sd.set_item("first_seq", s.first_seq)?;
            sd.set_item("path", &s.cold)?;
            sd.set_item("hot_path", &s.hot)?;
            sd.set_item("sealed", s.sealed)?;
            segs.append(sd)?;
        }
        d.set_item("segments", segs)?;
        d.set_item("first_seq", i.first_seq)?;
        d.set_item("last_seq", i.last_seq)?;
        let sch = PyList::empty(py);
        for x in &j.diffs {
            let sd = PyDict::new(py);
            sd.set_item("kind", x.name)?;
            sd.set_item("kind_id", x.kind)?;
            sd.set_item("journal_version", x.journal_version)?;
            sd.set_item("journal_size", x.journal_size)?;
            sd.set_item("reader_version", x.reader_version)?;
            sd.set_item("reader_size", x.reader_size)?;
            sd.set_item("segments", x.segments.clone())?;
            sch.append(sd)?;
        }
        d.set_item("schema", sch)?;
        d.set_item(
            "host_profile",
            host_profile(py, j.boot_newest.host_profile.as_ref())?,
        )?;
        Ok(d)
    }

    /// Lazy batches of one kind (README "参数语义").
    #[pyo3(signature = (kind, inst=None, t0=None, t1=None, time="proc", account=None, columns=None, decimals=false, batch_rows=None))]
    #[allow(clippy::too_many_arguments)]
    fn events(
        &self,
        kind: &Bound<'_, PyAny>,
        inst: Option<&Bound<'_, PyAny>>,
        t0: Option<&Bound<'_, PyAny>>,
        t1: Option<&Bound<'_, PyAny>>,
        time: &str,
        account: Option<&Bound<'_, PyAny>>,
        columns: Option<Vec<String>>,
        decimals: bool,
        batch_rows: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<PyEventReader> {
        let info = kind_arg(kind)?;
        let mut a = QueryArgs {
            insts: insts_arg(inst)?,
            t0: time_arg(t0, "t0")?,
            t1: time_arg(t1, "t1")?,
            time: Some(time.to_string()),
            columns,
            decimals,
            ..QueryArgs::default()
        };
        if let Some(acc) = account.filter(|x| !x.is_none()) {
            if let Ok(s) = acc.cast::<PyString>() {
                a.account_label = Some(s.to_string());
            } else {
                a.account_id = Some(int_arg(acc, "account")?);
            }
        }
        if let Some(b) = batch_rows.filter(|x| !x.is_none()) {
            a.batch_rows = Some(int_arg(b, "batch_rows")?);
        }
        let q = self.inner.query(info, &a)?;
        let ev = Events::new(&self.inner, q.clone());
        Ok(PyEventReader {
            schema: ev.schema(),
            inner: Some(ev),
            j: self.inner.clone(),
            q,
        })
    }

    /// New records of `kinds` as runs of same-kind batches (blocks between polls).
    #[pyo3(signature = (kinds, from_seq=None, poll_ms=None))]
    fn tail(
        &self,
        kinds: &Bound<'_, PyAny>,
        from_seq: Option<&Bound<'_, PyAny>>,
        poll_ms: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<PyTail> {
        let mut ks = Vec::new();
        for k in kinds.try_iter()? {
            ks.push(kind_arg(&k?)?);
        }
        let poll = match poll_ms.filter(|x| !x.is_none()) {
            Some(p) => int_arg(p, "poll_ms")?,
            None => 200,
        };
        if !(1..=86_400_000).contains(&poll) {
            return Err(PyValueError::new_err(format!(
                "poll_ms {poll}: expected 1..=86400000"
            )));
        }
        let from = match from_seq.filter(|x| !x.is_none()) {
            Some(f) => {
                let n = int_arg(f, "from_seq")?;
                Some(u64::try_from(n).map_err(|_| PyValueError::new_err(format!("from_seq {n} < 0")))?)
            }
            None => None,
        };
        let j = self.inner.clone();
        let t = Tail::new(&j, &ks, from)?;
        Ok(PyTail {
            inner: t,
            poll: Duration::from_millis(poll as u64),
        })
    }

    /// The configuration in effect at `seq` (README "参数语义").
    fn config_at<'py>(&self, py: Python<'py>, seq: &Bound<'py, PyAny>) -> PyResult<Bound<'py, PyDict>> {
        let n = int_arg(seq, "seq")?;
        let s = u64::try_from(n).map_err(|_| PyValueError::new_err(format!("seq {n} < 0")))?;
        crate::config::config_at_py(py, &self.inner, s)
    }
}

/// Fresh pass over one query, yielding `pyarrow.RecordBatch` objects (source of the C stream).
#[pyclass(name = "_BatchIter", module = "hft_py")]
pub struct PyBatchIter {
    ev: Option<Events>,
}

impl std::fmt::Debug for PyBatchIter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("_BatchIter").finish_non_exhaustive()
    }
}

#[pymethods]
impl PyBatchIter {
    fn __iter__(slf: PyRef<'_, Self>) -> PyRef<'_, Self> {
        slf
    }

    fn __next__<'py>(&mut self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let Some(ev) = self.ev.as_mut() else {
            return Err(PyStopIteration::new_err(()));
        };
        match py.detach(|| ev.next_batch()) {
            Ok(Some(b)) => pa_batch(py, &b),
            Ok(None) => {
                self.ev = None;
                Err(PyStopIteration::new_err(()))
            }
            Err(e) => {
                self.ev = None;
                Err(e.into())
            }
        }
    }
}

/// Batches of `events()`: iterate for `RecordBatch` objects, or hand the stream to
/// Polars/pyarrow/DuckDB through `__arrow_c_stream__` (each call reads the query from the start).
#[pyclass(name = "EventReader", module = "hft_py")]
pub struct PyEventReader {
    inner: Option<Events>,
    schema: Arc<SchemaSpec>,
    j: Arc<Journal>,
    q: crate::journal::Query,
}

impl std::fmt::Debug for PyEventReader {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EventReader").finish_non_exhaustive()
    }
}

#[pymethods]
impl PyEventReader {
    fn __iter__(slf: PyRef<'_, Self>) -> PyRef<'_, Self> {
        slf
    }

    fn __next__(&mut self, py: Python<'_>) -> PyResult<PyBatch> {
        let Some(ev) = self.inner.as_mut() else {
            return Err(PyStopIteration::new_err(()));
        };
        match py.detach(|| ev.next_batch()) {
            Ok(Some(b)) => Ok(PyBatch::new(b)),
            Ok(None) => {
                self.inner = None;
                Err(PyStopIteration::new_err(()))
            }
            Err(e) => {
                self.inner = None;
                Err(e.into())
            }
        }
    }

    /// Arrow PyCapsule stream: a `pyarrow.RecordBatchReader` over a fresh pass of the query.
    #[pyo3(signature = (requested_schema=None))]
    fn __arrow_c_stream__<'py>(
        &self,
        py: Python<'py>,
        requested_schema: Option<&Bound<'py, PyAny>>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let it = Py::new(
            py,
            PyBatchIter {
                ev: Some(Events::new(&self.j, self.q.clone())),
            },
        )?;
        let rdr = py
            .import("pyarrow")?
            .getattr("RecordBatchReader")?
            .call_method1("from_batches", (pa_schema(py, &self.schema)?, it))?;
        rdr.call_method1("__arrow_c_stream__", (requested_schema,))
    }

    /// Arrow PyCapsule schema of the stream.
    fn __arrow_c_schema__<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        pa_schema(py, &self.schema)?.call_method0("__arrow_c_schema__")
    }

    /// Schema metadata (`kind`, `kind_id`).
    #[getter]
    fn metadata<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyDict>> {
        meta_dict(py, &self.schema)
    }

    /// Column names in order.
    #[getter]
    fn column_names(&self) -> Vec<String> {
        self.schema.fields.iter().map(|f| f.name.clone()).collect()
    }
}

/// `tail()` iterator: blocks until a batch is available (the GIL is released while waiting).
#[pyclass(name = "TailIterator", module = "hft_py")]
pub struct PyTail {
    inner: Tail,
    poll: Duration,
}

impl std::fmt::Debug for PyTail {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TailIterator").finish_non_exhaustive()
    }
}

#[pymethods]
impl PyTail {
    fn __iter__(slf: PyRef<'_, Self>) -> PyRef<'_, Self> {
        slf
    }

    fn __next__(&mut self, py: Python<'_>) -> PyResult<PyBatch> {
        loop {
            let t = &mut self.inner;
            let poll = self.poll;
            let r: Res<Option<Batch>> = py.detach(|| match t.poll_once() {
                Ok(None) => {
                    std::thread::sleep(poll);
                    Ok(None)
                }
                other => other,
            });
            if let Some(b) = r? {
                return Ok(PyBatch::new(b));
            }
            py.check_signals()?;
        }
    }
}

fn meta_dict<'py>(py: Python<'py>, s: &SchemaSpec) -> PyResult<Bound<'py, PyDict>> {
    let d = PyDict::new(py);
    for (k, v) in &s.metadata {
        d.set_item(k, v)?;
    }
    Ok(d)
}

/// One Arrow record batch (`__arrow_c_array__`; `pyarrow.record_batch(b)`, `polars.DataFrame(b)`).
/// The `pyarrow.RecordBatch` is built on first export and reused.
#[pyclass(name = "RecordBatch", module = "hft_py", frozen)]
pub struct PyBatch {
    pub b: Batch,
    pa: PyOnceLock<Py<PyAny>>,
}

impl std::fmt::Debug for PyBatch {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RecordBatch")
            .field("rows", &self.b.rows)
            .finish_non_exhaustive()
    }
}

impl PyBatch {
    pub fn new(b: Batch) -> PyBatch {
        PyBatch {
            b,
            pa: PyOnceLock::new(),
        }
    }

    fn pa<'py>(&self, py: Python<'py>) -> PyResult<&Bound<'py, PyAny>> {
        Ok(self
            .pa
            .get_or_try_init(py, || pa_batch(py, &self.b).map(|x| x.unbind()))?
            .bind(py))
    }
}

#[pymethods]
impl PyBatch {
    #[getter]
    fn num_rows(&self) -> usize {
        self.b.rows
    }

    #[getter]
    fn num_columns(&self) -> usize {
        self.b.cols.len()
    }

    #[getter]
    fn column_names(&self) -> Vec<String> {
        self.b.schema.fields.iter().map(|f| f.name.clone()).collect()
    }

    /// Schema metadata (`kind`, `kind_id`).
    #[getter]
    fn metadata<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyDict>> {
        meta_dict(py, &self.b.schema)
    }

    fn __len__(&self) -> usize {
        self.b.rows
    }

    /// Values of one column as a list (ints, floats, bytes, str or None).
    fn __getitem__<'py>(&self, py: Python<'py>, name: &str) -> PyResult<Bound<'py, PyList>> {
        let (f, col) = self
            .b
            .schema
            .fields
            .iter()
            .zip(&self.b.cols)
            .find(|(f, _)| f.name == name)
            .ok_or_else(|| PyKeyError::new_err(name.to_string()))?;
        column_values(py, col, f, self.b.rows)
    }

    #[pyo3(signature = (requested_schema=None))]
    fn __arrow_c_array__<'py>(
        &self,
        py: Python<'py>,
        requested_schema: Option<&Bound<'py, PyAny>>,
    ) -> PyResult<Bound<'py, PyAny>> {
        self.pa(py)?
            .call_method1("__arrow_c_array__", (requested_schema,))
    }

    fn __arrow_c_schema__<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        pa_schema(py, &self.b.schema)?.call_method0("__arrow_c_schema__")
    }

    fn __repr__(&self) -> String {
        let kind = self.b.schema.meta("kind").unwrap_or_default();
        format!("RecordBatch(kind={kind}, rows={})", self.b.rows)
    }
}

/// Python list of one column.
fn column_values<'py>(
    py: Python<'py>,
    c: &Column,
    f: &FieldSpec,
    rows: usize,
) -> PyResult<Bound<'py, PyList>> {
    fn all<'py, T: IntoPyObject<'py> + Copy>(py: Python<'py>, v: &[T]) -> PyResult<Bound<'py, PyList>> {
        PyList::new(py, v.iter().copied())
    }
    Ok(match c {
        Column::U8(v) => all(py, v)?,
        Column::U16(v) => all(py, v)?,
        Column::U32(v) => all(py, v)?,
        Column::U64(v) => all(py, v)?,
        Column::I8(v) => all(py, v)?,
        Column::I16(v) => all(py, v)?,
        Column::I32(v) => all(py, v)?,
        Column::I64(v) => all(py, v)?,
        Column::Dec(v) => all(py, v)?,
        Column::Bool(v) => all(py, v)?,
        Column::F64(v, m) => PyList::new(py, v.iter().zip(m).map(|(x, ok)| ok.then_some(*x)))?,
        Column::Str(v) => PyList::new(py, v.iter().map(Option::as_deref))?,
        Column::Fixed(w, v) => {
            let out = PyList::empty(py);
            for r in 0..rows {
                out.append(PyBytes::new(py, v.get(r * w..(r + 1) * w).unwrap_or(&[])))?;
            }
            out
        }
        Column::Bin(o, d) => {
            let out = PyList::empty(py);
            for w in o.windows(2) {
                let (a, b) = match w {
                    [a, b] => (*a as usize, *b as usize),
                    _ => (0, 0),
                };
                out.append(PyBytes::new(py, d.get(a..b).unwrap_or(&[])))?;
            }
            out
        }
        Column::List(o, children) => {
            let ColType::ListOfStruct(fields) = &f.ty else {
                return Err(PyTypeError::new_err("list column without a struct type"));
            };
            let n = o.last().copied().unwrap_or(0) as usize;
            let cols: Vec<Bound<'py, PyList>> = children
                .iter()
                .zip(fields)
                .map(|(c, f)| column_values(py, c, f, n))
                .collect::<PyResult<_>>()?;
            let out = PyList::empty(py);
            for w in o.windows(2) {
                let (a, b) = match w {
                    [a, b] => (*a as usize, *b as usize),
                    _ => (0, 0),
                };
                let items = PyList::empty(py);
                for r in a..b {
                    let d = PyDict::new(py);
                    for (f, c) in fields.iter().zip(&cols) {
                        d.set_item(&f.name, c.get_item(r)?)?;
                    }
                    items.append(d)?;
                }
                out.append(items)?;
            }
            out
        }
    })
}
