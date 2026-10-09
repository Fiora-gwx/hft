//! Test hooks (cargo feature `testing`, README "打包与发布"): the data-buffer addresses of the last
//! batch and the number of `tail` journal checks, exported as `hft_py._testing`. Without the
//! feature the hooks compile to nothing and the submodule does not exist.

#[cfg(feature = "testing")]
mod imp {
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicU64, Ordering};

    use pyo3::prelude::*;
    use pyo3::types::PyDict;

    static LAST: Mutex<Vec<(String, usize)>> = Mutex::new(Vec::new());
    static POLLS: AtomicU64 = AtomicU64::new(0);

    pub fn record_addrs(a: &[(String, usize)]) {
        if let Ok(mut l) = LAST.lock() {
            *l = a.to_vec();
        }
    }

    pub fn tail_poll() {
        POLLS.fetch_add(1, Ordering::Relaxed);
    }

    #[pyfunction]
    fn last_buffer_addrs(py: Python<'_>) -> PyResult<Bound<'_, PyDict>> {
        let d = PyDict::new(py);
        if let Ok(l) = LAST.lock() {
            for (k, v) in l.iter() {
                d.set_item(k, *v)?;
            }
        }
        Ok(d)
    }

    #[pyfunction]
    fn tail_polls() -> u64 {
        POLLS.load(Ordering::Relaxed)
    }

    /// Ledger sections 1, 13 and 22 of a `ledger(...)` result's final state.
    #[pyfunction]
    fn ledger_snapshot<'py>(
        py: Python<'py>,
        l: PyRef<'_, crate::py_ledger::PyLedger>,
    ) -> Bound<'py, pyo3::types::PyBytes> {
        pyo3::types::PyBytes::new(py, &l.snapshot())
    }

    pub fn module<'py>(py: Python<'py>) -> PyResult<Option<Bound<'py, PyModule>>> {
        let m = PyModule::new(py, "_testing")?;
        m.add_function(wrap_pyfunction!(last_buffer_addrs, &m)?)?;
        m.add_function(wrap_pyfunction!(tail_polls, &m)?)?;
        m.add_function(wrap_pyfunction!(ledger_snapshot, &m)?)?;
        Ok(Some(m))
    }
}

/// Records the value-buffer addresses of the last batch handed to pyarrow (`testing` builds only).
pub fn record_addrs(a: &[(String, usize)]) {
    #[cfg(feature = "testing")]
    imp::record_addrs(a);
    #[cfg(not(feature = "testing"))]
    let _ = a;
}

/// Counts one `tail` journal check (`testing` builds only).
pub fn tail_poll() {
    #[cfg(feature = "testing")]
    imp::tail_poll();
}

/// The `_testing` submodule (`testing` builds only).
pub fn module(py: pyo3::Python<'_>) -> pyo3::PyResult<Option<pyo3::Bound<'_, pyo3::types::PyModule>>> {
    #[cfg(feature = "testing")]
    return imp::module(py);
    #[cfg(not(feature = "testing"))]
    {
        let _ = py;
        Ok(None)
    }
}
