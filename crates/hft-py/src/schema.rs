//! `hft_py.schema_hash()` and `hft_py.schema.catalog()`: the compiled kind table (README "Arrow
//! 映射"), the same rows `cargo xtask gen-schema` writes to `docs/generated/event-catalog.md`.

use hft_event::{CrcPolicy, Domain, JournalMask, KindInfo, Layout, Role};
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList};

use crate::journal::layout_size;

/// FNV-1a 64 over `(kind u16, schema_version u16, size u32)` little-endian of every production
/// kind in number order (`size` = fixed body or Var/Multi prefix). `gen-schema` writes the same
/// value as `SCHEMA_HASH` in `python/_generated/events.py`.
pub fn schema_hash() -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for k in production_kinds() {
        let mut b = [0u8; 8];
        b[..2].copy_from_slice(&k.kind.0.to_le_bytes());
        b[2..4].copy_from_slice(&k.schema_version.to_le_bytes());
        b[4..].copy_from_slice(&layout_size(k).to_le_bytes());
        for x in b {
            h ^= u64::from(x);
            h = h.wrapping_mul(0x0000_0100_0000_01b3);
        }
    }
    h
}

/// Production kinds (test kinds `0x7Fxx` excluded), ascending.
pub fn production_kinds() -> impl Iterator<Item = &'static KindInfo> {
    hft_event::KINDS.iter().filter(|k| k.kind.0 >> 8 != 0x7F)
}

/// `fixed 96` / `var 32 4096` / `multi 72` (event-catalog "布局").
pub fn layout_text(l: Layout) -> String {
    match l {
        Layout::Fixed { size } => format!("fixed {size}"),
        Layout::Var { prefix, max } => format!("var {prefix} {max}"),
        Layout::Multi { prefix } => format!("multi {prefix}"),
    }
}

fn role_text(r: Role) -> &'static str {
    match r {
        Role::Input => "Input",
        Role::Output => "Output",
        Role::Meta => "Meta",
        Role::ByBody => "ByBody",
    }
}

fn domain_text(d: Domain) -> &'static str {
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

fn journals_text(m: JournalMask) -> String {
    let mut v = Vec::new();
    for (bit, n) in [
        (JournalMask::SEQ, "seq"),
        (JournalMask::RAW, "raw"),
        (JournalMask::CHAIN, "chain"),
    ] {
        if m.contains(bit) {
            v.push(n);
        }
    }
    v.join(",")
}

#[pyfunction]
#[pyo3(name = "schema_hash")]
pub fn schema_hash_py() -> u64 {
    schema_hash()
}

/// One dict per kind: `kind`, `name`, `domain`, `role`, `layout`, `schema_version`, `journals`,
/// `crc`, `optional`.
#[pyfunction]
fn catalog(py: Python<'_>) -> PyResult<Bound<'_, PyList>> {
    let out = PyList::empty(py);
    for k in production_kinds() {
        let d = PyDict::new(py);
        d.set_item("kind", k.kind.0)?;
        d.set_item("name", k.name)?;
        d.set_item("domain", domain_text(k.domain))?;
        d.set_item("role", role_text(k.role))?;
        d.set_item("layout", layout_text(k.layout))?;
        d.set_item("schema_version", k.schema_version)?;
        d.set_item("journals", journals_text(k.journals))?;
        d.set_item(
            "crc",
            match k.crc {
                CrcPolicy::Never => "never",
                CrcPolicy::Always => "always",
            },
        )?;
        d.set_item("optional", k.optional)?;
        out.append(d)?;
    }
    Ok(out)
}

/// Builds the `hft_py.schema` submodule.
pub fn module<'py>(py: Python<'py>) -> PyResult<Bound<'py, PyModule>> {
    let m = PyModule::new(py, "schema")?;
    m.add_function(wrap_pyfunction!(catalog, &m)?)?;
    Ok(m)
}
