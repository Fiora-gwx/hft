//! `hft_py.decode.<kind>`: the prefix fields and payload of the six variable records that `events`
//! gives as a `body` binary column (README "Arrow 映射"), and the shared leaf → Python conversion.

use hft_event::{EventKind, KindInfo, kind_info};
use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyDict};

use crate::arrow_map::{KindMap, Leaf, kind_map};
use crate::error::{Error, Res};

/// Python value of one leaf of `b` (ints, or `bytes` for byte arrays).
pub fn leaf_value<'py>(py: Python<'py>, l: &Leaf, b: &[u8]) -> PyResult<Bound<'py, PyAny>> {
    match l.prim {
        Some(_) => Ok(l.read_int(b).into_pyobject(py)?.into_any()),
        None => {
            let s = b.get(l.off..l.off + l.width).unwrap_or(&[]);
            Ok(PyBytes::new(py, s).into_any())
        }
    }
}

/// Every leaf of `map` read from `b` into `d`.
pub fn fill_leaves(py: Python<'_>, d: &Bound<'_, PyDict>, map: &KindMap, b: &[u8]) -> PyResult<()> {
    for l in &map.leaves {
        d.set_item(&l.name, leaf_value(py, l, b)?)?;
    }
    Ok(())
}

fn u_at(b: &[u8], o: usize, w: usize) -> usize {
    let mut x = [0u8; 8];
    if let (Some(s), Some(d)) = (b.get(o..o + w), x.get_mut(..w)) {
        d.copy_from_slice(s);
    }
    u64::from_le_bytes(x) as usize
}

/// Prefix and payload of a body of `kind`; the payload length must match the prefix's length field.
pub fn split(kind: EventKind, b: &[u8]) -> Res<(&'static KindInfo, &[u8])> {
    let info = kind_info(kind).ok_or_else(|| Error::value("unknown kind"))?;
    let map = kind_map(info);
    if b.len() < map.prefix {
        return Err(Error::value(format!(
            "{}: {} bytes is shorter than the {}-byte prefix",
            info.name,
            b.len(),
            map.prefix
        )));
    }
    let rest = b.get(map.prefix..).unwrap_or(&[]);
    // Declared payload length: ConfigBlob `blob.len` (4..8), ControlCmd `payload_len` (76..80),
    // Advice `len` (8..10), RawRx/RawTx `payload_len` (16..20); StateSnapshot: the rest.
    let want = match kind {
        EventKind::CONFIG_BLOB => Some(u_at(b, 4, 4)),
        EventKind::CONTROL_CMD => Some(u_at(b, 76, 4)),
        EventKind::ADVICE => Some(u_at(b, 8, 2)),
        EventKind::RAW_RX | EventKind::RAW_TX => Some(u_at(b, 16, 4)),
        _ => None,
    };
    match want {
        Some(n) if n != rest.len() => Err(Error::value(format!(
            "{}: payload is {} bytes, the prefix declares {n}",
            info.name,
            rest.len()
        ))),
        _ => Ok((info, rest)),
    }
}

fn decode_any<'py>(py: Python<'py>, kind: EventKind, b: &[u8]) -> PyResult<Bound<'py, PyDict>> {
    let (info, payload) = split(kind, b)?;
    let map = kind_map(info);
    let d = PyDict::new(py);
    fill_leaves(py, &d, &map, b)?;
    d.set_item("payload", PyBytes::new(py, payload))?;
    Ok(d)
}

/// `ControlCmd` logical body → `CmdHdr` fields + `payload`.
#[pyfunction]
fn control_cmd<'py>(py: Python<'py>, b: &[u8]) -> PyResult<Bound<'py, PyDict>> {
    decode_any(py, EventKind::CONTROL_CMD, b)
}

/// `ConfigBlob` logical body → `BlobHdr` fields + `payload` (the blob content).
#[pyfunction]
fn config_blob<'py>(py: Python<'py>, b: &[u8]) -> PyResult<Bound<'py, PyDict>> {
    decode_any(py, EventKind::CONFIG_BLOB, b)
}

/// `StateSnapshot` logical body → `SnapshotHdr` fields + `payload` (the section table).
#[pyfunction]
fn state_snapshot<'py>(py: Python<'py>, b: &[u8]) -> PyResult<Bound<'py, PyDict>> {
    decode_any(py, EventKind::STATE_SNAPSHOT, b)
}

/// `Advice` body → `AdviceHdr` fields + `payload` (strategy-defined bytes).
#[pyfunction]
fn advice<'py>(py: Python<'py>, b: &[u8]) -> PyResult<Bound<'py, PyDict>> {
    decode_any(py, EventKind::ADVICE, b)
}

/// `RawRx` logical body → `RawHdr` fields + `payload`.
#[pyfunction]
fn raw_rx<'py>(py: Python<'py>, b: &[u8]) -> PyResult<Bound<'py, PyDict>> {
    decode_any(py, EventKind::RAW_RX, b)
}

/// `RawTx` logical body → `RawHdr` fields + `payload`.
#[pyfunction]
fn raw_tx<'py>(py: Python<'py>, b: &[u8]) -> PyResult<Bound<'py, PyDict>> {
    decode_any(py, EventKind::RAW_TX, b)
}

/// Builds the `hft_py.decode` submodule.
pub fn module<'py>(py: Python<'py>) -> PyResult<Bound<'py, PyModule>> {
    let m = PyModule::new(py, "decode")?;
    m.add_function(wrap_pyfunction!(control_cmd, &m)?)?;
    m.add_function(wrap_pyfunction!(config_blob, &m)?)?;
    m.add_function(wrap_pyfunction!(state_snapshot, &m)?)?;
    m.add_function(wrap_pyfunction!(advice, &m)?)?;
    m.add_function(wrap_pyfunction!(raw_rx, &m)?)?;
    m.add_function(wrap_pyfunction!(raw_tx, &m)?)?;
    Ok(m)
}
