//! `hft_py.heartbeat`: read-only view of hft-rt's `HeartbeatBlock` (README "参数语义", 心跳).
//! Mappings come from `HeartbeatBlock::open_ro`, which never unmaps; this module keeps one mapping
//! per (path, device, inode) so repeated reads do not grow the process's mappings.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::Instant;

use hft_rt::{HeartbeatBlock, HkLine, RtErr};
use pyo3::prelude::*;
use pyo3::types::PyDict;

use crate::error::{Error, Res};

/// Default shared-memory root (`/dev/shm/hft/<instance>/heartbeat`).
pub const DEFAULT_ROOT: &str = "/dev/shm/hft";

/// One reading of the block (README field list).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HbRead {
    pub state: u8,
    pub seq_beats: u64,
    pub pid: u32,
    pub instance_id: u8,
    pub boot_id: [u8; 16],
    pub started_utc_ns: i64,
    pub hk_utc_ns: i64,
    pub n_slots: u8,
    /// `CLOCK_MONOTONIC` at the read.
    pub mono_ns: u64,
}

/// `Starting`/`Warmup`/`Running`/`Draining`/`Stopped` (other codes: `"Unknown(<n>)"`).
pub fn state_name(s: u8) -> String {
    match s {
        HkLine::STARTING => "Starting".into(),
        HkLine::WARMUP => "Warmup".into(),
        HkLine::RUNNING => "Running".into(),
        HkLine::DRAINING => "Draining".into(),
        HkLine::STOPPED => "Stopped".into(),
        n => format!("Unknown({n})"),
    }
}

/// `CLOCK_MONOTONIC` in ns, read through `std::time::Instant` (Linux: the `timespec` of the
/// monotonic clock, the same clock as Python's `time.monotonic_ns()`).
pub fn mono_ns() -> u64 {
    let s = format!("{:?}", Instant::now());
    let num = |key: &str| -> Option<u64> {
        let i = s.find(key)? + key.len();
        let rest = s.get(i..)?.trim_start_matches([':', ' ']);
        let end = rest.find(|c: char| !c.is_ascii_digit()).unwrap_or(rest.len());
        rest.get(..end)?.parse().ok()
    };
    match (num("tv_sec"), num("tv_nsec")) {
        (Some(a), Some(b)) => a.saturating_mul(1_000_000_000).saturating_add(b),
        _ => 0,
    }
}

/// Path of an instance's block under `root`.
pub fn block_path(instance: &str, root: &Path) -> Res<PathBuf> {
    if instance.is_empty() || instance.contains(['/', '\\']) || instance == "." || instance == ".." {
        return Err(Error::value(format!("instance `{instance}`: not a plain name")));
    }
    Ok(root.join(instance).join("heartbeat"))
}

#[cfg(unix)]
fn file_id(m: &std::fs::Metadata) -> (u64, u64) {
    use std::os::unix::fs::MetadataExt;
    (m.dev(), m.ino())
}

#[cfg(not(unix))]
fn file_id(_: &std::fs::Metadata) -> (u64, u64) {
    (0, 0)
}

type Cache = Mutex<HashMap<(PathBuf, u64, u64), &'static HeartbeatBlock>>;

fn cache() -> &'static Cache {
    static C: OnceLock<Cache> = OnceLock::new();
    C.get_or_init(|| Mutex::new(HashMap::new()))
}

fn rt_err(e: RtErr, p: &Path) -> Error {
    match e {
        RtErr::BadMagic => Error::Schema(format!("{}: heartbeat magic mismatch", p.display())),
        RtErr::BadVersion(v) => Error::Schema(format!("{}: heartbeat layout_version {v} ≠ 1", p.display())),
        other => Error::Os(format!("{}: {other:?}", p.display())),
    }
}

/// The read-only mapping of the block at `p` (cached per file identity).
pub fn open(p: &Path) -> Res<&'static HeartbeatBlock> {
    let meta = match std::fs::metadata(p) {
        Ok(m) => m,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Err(Error::NotFound(format!("{}: no heartbeat block", p.display())));
        }
        Err(e) => return Err(Error::Os(format!("{}: {e}", p.display()))),
    };
    let (dev, ino) = file_id(&meta);
    let key = (p.to_path_buf(), dev, ino);
    let mut c = cache()
        .lock()
        .map_err(|_| Error::Os("heartbeat cache poisoned".into()))?;
    if let Some(b) = c.get(&key) {
        return Ok(b);
    }
    let b = HeartbeatBlock::open_ro(p).map_err(|e| rt_err(e, p))?;
    c.insert(key, b);
    Ok(b)
}

/// Reads the block (magic and layout version re-checked on every read).
pub fn read_block(b: &HeartbeatBlock, p: &Path) -> Res<HbRead> {
    use std::sync::atomic::Ordering;
    let hk = &b.hk.0;
    if hk.magic != HkLine::MAGIC {
        return Err(rt_err(RtErr::BadMagic, p));
    }
    if hk.layout_version != HkLine::LAYOUT_VERSION {
        return Err(rt_err(RtErr::BadVersion(hk.layout_version), p));
    }
    Ok(HbRead {
        state: hk.state.load(Ordering::Acquire),
        seq_beats: b.beat.read(),
        pid: hk.pid,
        instance_id: hk.instance_id,
        boot_id: hk.boot_id,
        started_utc_ns: hk.started_utc_ns,
        hk_utc_ns: hk.hk_utc_ns.load(Ordering::Acquire),
        n_slots: hk.n_slots.load(Ordering::Acquire),
        mono_ns: mono_ns(),
    })
}

/// `heartbeat.read(instance, root)`.
pub fn read(instance: &str, root: &Path) -> Res<HbRead> {
    let p = block_path(instance, root)?;
    let b = open(&p)?;
    read_block(b, &p)
}

/// Result of `Watch::check`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WatchCheck {
    pub state: u8,
    pub advanced: bool,
    pub age_ms: u64,
}

/// Age of the sequencer heartbeat as seen by this watcher (rule of hft-rt `HeartbeatWatch`, plus a
/// change of `pid` or `boot_id` counting as progress).
#[derive(Debug)]
pub struct Watch {
    path: PathBuf,
    last: Option<(u64, u32, [u8; 16])>,
    changed_at: Option<Instant>,
}

impl Watch {
    pub fn new(instance: &str, root: &Path) -> Res<Watch> {
        Ok(Watch {
            path: block_path(instance, root)?,
            last: None,
            changed_at: None,
        })
    }

    pub fn check(&mut self) -> Res<WatchCheck> {
        let b = open(&self.path)?;
        let r = read_block(b, &self.path)?;
        let now = Instant::now();
        let key = (r.seq_beats, r.pid, r.boot_id);
        let advanced = self.last != Some(key);
        if advanced {
            self.last = Some(key);
            self.changed_at = Some(now);
        }
        let age = self
            .changed_at
            .map_or(0, |t| now.saturating_duration_since(t).as_millis() as u64);
        Ok(WatchCheck {
            state: r.state,
            advanced,
            age_ms: age,
        })
    }
}

/// Lowercase hex of bytes (`boot_id`).
pub fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

fn root_of(root: Option<PathBuf>) -> PathBuf {
    root.unwrap_or_else(|| PathBuf::from(DEFAULT_ROOT))
}

#[pyfunction]
#[pyo3(name = "read", signature = (instance, root=None))]
fn read_py<'py>(py: Python<'py>, instance: &str, root: Option<PathBuf>) -> PyResult<Bound<'py, PyDict>> {
    let r = read(instance, &root_of(root))?;
    let d = PyDict::new(py);
    d.set_item("state", state_name(r.state))?;
    d.set_item("seq_beats", r.seq_beats)?;
    d.set_item("pid", r.pid)?;
    d.set_item("instance_id", r.instance_id)?;
    d.set_item("boot_id", hex(&r.boot_id))?;
    d.set_item("started_utc_ns", r.started_utc_ns)?;
    d.set_item("hk_utc_ns", r.hk_utc_ns)?;
    d.set_item("n_slots", r.n_slots)?;
    d.set_item("mono_ns", r.mono_ns)?;
    Ok(d)
}

/// Heartbeat age watcher of one instance.
#[pyclass(name = "Watch", module = "hft_py.heartbeat")]
#[derive(Debug)]
pub struct PyWatch {
    inner: Watch,
}

#[pymethods]
impl PyWatch {
    #[new]
    #[pyo3(signature = (instance, root=None))]
    fn new(instance: &str, root: Option<PathBuf>) -> PyResult<PyWatch> {
        Ok(PyWatch {
            inner: Watch::new(instance, &root_of(root))?,
        })
    }

    /// `{"state", "advanced", "age_ms"}`.
    fn check<'py>(&mut self, py: Python<'py>) -> PyResult<Bound<'py, PyDict>> {
        let c = self.inner.check()?;
        let d = PyDict::new(py);
        d.set_item("state", state_name(c.state))?;
        d.set_item("advanced", c.advanced)?;
        d.set_item("age_ms", c.age_ms)?;
        Ok(d)
    }
}

/// Builds the `hft_py.heartbeat` submodule.
pub fn module<'py>(py: Python<'py>) -> PyResult<Bound<'py, PyModule>> {
    let m = PyModule::new(py, "heartbeat")?;
    m.add_function(wrap_pyfunction!(read_py, &m)?)?;
    m.add_class::<PyWatch>()?;
    Ok(m)
}
