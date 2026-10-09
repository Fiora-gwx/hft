//! Error type of the Rust core and its mapping to the Python exceptions of README "错误".
//! No error crosses the FFI as a panic: every failure is one of these variants.

use pyo3::exceptions::{PyException, PyFileNotFoundError, PyOSError, PyTypeError, PyValueError};
use pyo3::prelude::*;

pyo3::create_exception!(
    hft_py,
    JournalError,
    PyException,
    "Damaged segment, CRC failure, broken segment chain, missing journal (attributes `path`, `seq`)."
);
pyo3::create_exception!(
    hft_py,
    SchemaError,
    PyException,
    "Incompatible event schema or heartbeat layout."
);
pyo3::create_exception!(
    hft_py,
    ParamsError,
    PyException,
    "Unknown or mismatched parameter schema, inconsistent index, undecodable blob (attributes `variant`, `field`, `group_ix`)."
);

/// Failure of a core operation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Error {
    /// Argument value out of its domain → `ValueError`.
    Value(String),
    /// Argument of the wrong type → `TypeError`.
    Type(String),
    /// → `JournalError` with `path` and `seq` (either may be unknown).
    Journal {
        msg: String,
        path: Option<String>,
        seq: Option<u64>,
    },
    /// → `SchemaError`.
    Schema(String),
    /// → `ParamsError`; `variant` is the SDK `ParamError`/`ParamsErr` variant name or one of
    /// `UnknownField`, `UnknownOp`, `Schema`, `Index`.
    Params {
        msg: String,
        variant: String,
        field: Option<String>,
        group_ix: Option<u16>,
    },
    /// → `FileNotFoundError`.
    NotFound(String),
    /// → `OSError`.
    Os(String),
}

impl Error {
    pub fn journal(msg: impl Into<String>, path: Option<String>, seq: Option<u64>) -> Error {
        Error::Journal {
            msg: msg.into(),
            path,
            seq,
        }
    }

    pub fn params(variant: &str, msg: impl Into<String>) -> Error {
        Error::Params {
            msg: msg.into(),
            variant: variant.to_string(),
            field: None,
            group_ix: None,
        }
    }

    pub fn value(msg: impl Into<String>) -> Error {
        Error::Value(msg.into())
    }
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Value(m) | Error::Type(m) | Error::Schema(m) | Error::NotFound(m) | Error::Os(m) => {
                f.write_str(m)
            }
            Error::Journal { msg, path, seq } => {
                write!(f, "{msg}")?;
                if let Some(p) = path {
                    write!(f, " (path {p})")?;
                }
                if let Some(s) = seq {
                    write!(f, " (seq {s})")?;
                }
                Ok(())
            }
            Error::Params { msg, variant, .. } => write!(f, "{variant}: {msg}"),
        }
    }
}

impl From<Error> for PyErr {
    fn from(e: Error) -> PyErr {
        let text = e.to_string();
        match e {
            Error::Value(_) => PyValueError::new_err(text),
            Error::Type(_) => PyTypeError::new_err(text),
            Error::NotFound(_) => PyFileNotFoundError::new_err(text),
            Error::Os(_) => PyOSError::new_err(text),
            Error::Schema(_) => SchemaError::new_err(text),
            Error::Journal { path, seq, .. } => Python::attach(|py| {
                let err = JournalError::new_err(text);
                let v = err.value(py);
                let _ = v.setattr("path", path);
                let _ = v.setattr("seq", seq);
                err
            }),
            Error::Params {
                variant,
                field,
                group_ix,
                ..
            } => Python::attach(|py| {
                let err = ParamsError::new_err(text);
                let v = err.value(py);
                let _ = v.setattr("variant", variant);
                let _ = v.setattr("field", field);
                let _ = v.setattr("group_ix", group_ix);
                err
            }),
        }
    }
}

pub type Res<T> = Result<T, Error>;
