//! `Journal.config_at(seq)` (README "参数语义"): the run's boot-block `ConfigBlob{boot = 1}`, plus
//! the blobs of `SetParams`/`SetRisk` commands that got `CmdResult{Applied}` up to `seq`;
//! `ReplaceRefdata` takes effect at the next boot block.

use std::collections::BTreeMap;

use hft_event::{CmdHdr, ControlCmd, EventKind};
use hft_journal::StartAt;
use hft_refdata::{AccountKind, EngineConfig, EngineSection, RefData};
use hft_types::{AccountId, AssetId, StrategyId};
use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyDict, PyList};

use crate::error::{Error, Res};
use crate::journal::{Journal, Rec, Step};

/// `BlobKind` codes (hft-event).
pub mod blob_kind {
    pub const REFDATA: u16 = 1;
    pub const STRATEGY_PARAMS: u16 = 2;
    pub const RISK_PARAMS: u16 = 3;
    pub const HOST_MANIFEST: u16 = 4;
    pub const ENGINE: u16 = 5;
    pub const SIM_CONFIG: u16 = 6;
}

const BLOB_HDR: usize = 72;
/// `CmdResult.status` of an applied command (control-plane result code `Applied`).
const APPLIED: u8 = 0;

/// One config blob in effect.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Blob {
    pub seq: u64,
    pub kind: u16,
    pub encoding: u16,
    pub version: u64,
    pub sha256: [u8; 32],
    pub strategy: u8,
    pub account: u8,
    pub content: Vec<u8>,
}

impl Blob {
    fn parse(seq: u64, body: &[u8]) -> Option<(Blob, u8)> {
        let h = body.get(..BLOB_HDR)?;
        let u = |o: usize, w: usize| {
            let mut x = [0u8; 8];
            x.get_mut(..w)?.copy_from_slice(h.get(o..o + w)?);
            Some(u64::from_le_bytes(x))
        };
        let len = u(4, 4)? as usize;
        let mut sha = [0u8; 32];
        sha.copy_from_slice(h.get(16..48)?);
        Some((
            Blob {
                seq,
                kind: u(0, 2)? as u16,
                encoding: u(2, 2)? as u16,
                version: u(8, 8)?,
                sha256: sha,
                strategy: *h.get(48)?,
                account: *h.get(50)?,
                content: body.get(BLOB_HDR..BLOB_HDR + len)?.to_vec(),
            },
            *h.get(64)?,
        ))
    }

    /// Key in the result: the kind, or `(strategy, account)` for strategy params.
    fn key(&self) -> (u16, u8, u8) {
        if self.kind == blob_kind::STRATEGY_PARAMS {
            (self.kind, self.strategy, self.account)
        } else {
            (self.kind, 0, 0)
        }
    }
}

/// The configuration in effect at a seq.
#[derive(Clone, Debug, Default)]
pub struct Config {
    pub blobs: BTreeMap<(u16, u8, u8), Blob>,
}

/// Configuration state machine over journal records (boot blocks, staged blobs, applied commands).
#[derive(Debug, Default)]
pub struct Tracker {
    /// Config of the last completed boot block plus applied commands.
    pub state: Option<Config>,
    /// Boot-block blobs seen since the last `Begin`.
    pub boot: Vec<Blob>,
    staged: BTreeMap<[u8; 32], Blob>,
    pending: BTreeMap<[u8; 16], [u8; 32]>,
}

/// What one record changed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Change {
    None,
    /// A boot block completed (`BootDone`): the whole state was replaced.
    Boot,
    /// An applied command replaced this blob.
    Applied(Blob),
}

impl Tracker {
    /// Feeds one record (logical body).
    pub fn feed(&mut self, rec: &Rec<'_>) -> Change {
        let k = EventKind(rec.kind);
        if k == EventKind::SEGMENT_ROLL {
            match rec.roll_mark() {
                2 => self.boot.clear(),
                3 => {
                    let mut c = Config::default();
                    for b in self.boot.drain(..) {
                        c.blobs.insert(b.key(), b);
                    }
                    self.state = Some(c);
                    return Change::Boot;
                }
                _ => {}
            }
        } else if k == EventKind::CONFIG_BLOB {
            if let Some((b, flag)) = Blob::parse(rec.seq, rec.body) {
                if rec.in_boot && flag == 1 {
                    self.boot.push(b);
                } else {
                    self.staged.insert(b.sha256, b);
                }
            }
        } else if k == EventKind::CONTROL_CMD {
            if let Ok((hdr, cmd)) = ControlCmd::decode(rec.body) {
                let hdr: CmdHdr = hdr;
                if let ControlCmd::SetParams { blob, .. } | ControlCmd::SetRisk { blob } = cmd {
                    self.pending.insert(hdr.cmd_id, blob.sha256);
                }
            }
        } else if k == EventKind::CMD_RESULT {
            let mut id = [0u8; 16];
            if let Some(s) = rec.body.get(..16) {
                id.copy_from_slice(s);
            }
            let status = rec.body.get(26).copied().unwrap_or(u8::MAX);
            if let Some(sha) = self.pending.remove(&id)
                && status == APPLIED
                && let (Some(st), Some(b)) = (self.state.as_mut(), self.staged.get(&sha))
            {
                let mut b = b.clone();
                b.seq = rec.seq;
                st.blobs.insert(b.key(), b.clone());
                return Change::Applied(b);
            }
        }
        Change::None
    }

    /// The configuration in effect, or (inside the first boot block) the blobs seen so far.
    pub fn current(&self) -> Config {
        match &self.state {
            Some(c) if self.boot.is_empty() => c.clone(),
            _ => {
                let mut c = self.state.clone().unwrap_or_default();
                for b in &self.boot {
                    c.blobs.insert(b.key(), b.clone());
                }
                c
            }
        }
    }
}

fn scan_from(j: &Journal, start: StartAt, target: u64) -> Res<Option<Config>> {
    let mut sc = j.scan(start)?;
    let mut t = Tracker::default();
    while let Step::Rec(rec) = sc.step()? {
        if rec.seq > target {
            break;
        }
        t.feed(&rec);
    }
    Ok(t.state)
}

/// `config_at(seq)`.
pub fn config_at(j: &Journal, seq: u64) -> Res<Config> {
    let candidates: Vec<u64> = j
        .segs
        .values()
        .filter(|s| s.first_seq != 0 && s.first_seq <= seq)
        .map(|s| s.no)
        .rev()
        .collect();
    for (i, no) in candidates.iter().enumerate() {
        if let Some(c) = scan_from(j, StartAt::SegmentBoot(*no), seq)? {
            return Ok(c);
        }
        if i >= 1 {
            break;
        }
    }
    Err(Error::value(format!(
        "seq {seq} is before the first complete boot block of the journal"
    )))
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

fn base<'py>(py: Python<'py>, b: &Blob) -> PyResult<Bound<'py, PyDict>> {
    let d = PyDict::new(py);
    d.set_item("seq", b.seq)?;
    d.set_item("sha256", hex(&b.sha256))?;
    d.set_item("version", b.version)?;
    d.set_item("encoding", b.encoding)?;
    Ok(d)
}

fn account_kind(k: &AccountKind) -> &'static str {
    match k {
        AccountKind::Live => "Live",
        AccountKind::Sim(_) => "Sim",
    }
}

/// Refdata blob → dict (assets, insts, accounts, strategy instances).
pub fn refdata_dict<'py>(py: Python<'py>, d: &Bound<'py, PyDict>, rd: &RefData) -> PyResult<()> {
    d.set_item("refdata_version", rd.version())?;
    let assets = PyList::empty(py);
    for a in 1..=u16::MAX {
        let m = rd.asset(AssetId(a));
        if m.id.0 == 0 {
            if a > 4096 {
                break;
            }
            continue;
        }
        let x = PyDict::new(py);
        x.set_item("id", a)?;
        x.set_item("name", &*m.name)?;
        x.set_item("px_exp", m.exp.px_exp)?;
        x.set_item("qty_exp", m.exp.qty_exp)?;
        assets.append(x)?;
    }
    d.set_item("assets", assets)?;
    let insts = PyList::empty(py);
    for v in rd.venues_in_order() {
        for i in rd.insts_of_venue(*v) {
            let h = rd.inst(*i);
            let x = PyDict::new(py);
            x.set_item("id", i.0)?;
            x.set_item("venue", h.venue.0)?;
            x.set_item("symbol", &*rd.inst_meta(*i).symbol)?;
            x.set_item("asset", h.asset.0)?;
            x.set_item("quote", h.quote.0)?;
            x.set_item("px_exp", h.px_exp)?;
            x.set_item("qty_exp", h.qty_exp)?;
            insts.append(x)?;
        }
    }
    d.set_item("insts", insts)?;
    let accounts = PyList::empty(py);
    for a in 1..=254u8 {
        let m = rd.account(AccountId(a));
        if m.id != AccountId(a) {
            continue;
        }
        let x = PyDict::new(py);
        x.set_item("id", a)?;
        x.set_item("label", &*m.label)?;
        x.set_item("kind", account_kind(&m.kind))?;
        x.set_item("bound_instance", m.bound_instance.0)?;
        accounts.append(x)?;
    }
    d.set_item("accounts", accounts)?;
    let inst_list = PyList::empty(py);
    for i in rd.instances_in_order() {
        let x = PyDict::new(py);
        x.set_item("id", i.id.0)?;
        x.set_item("kind_id", i.kind_id)?;
        x.set_item("name", &*i.name)?;
        inst_list.append(x)?;
    }
    d.set_item("instances", inst_list)?;
    Ok(())
}

fn section_name(s: EngineSection) -> &'static str {
    match s {
        EngineSection::Oms => "oms",
        EngineSection::Ledger => "ledger",
        EngineSection::Capacities => "capacities",
        EngineSection::Rings => "rings",
        EngineSection::Sequencer => "sequencer",
        EngineSection::Conn => "conn",
        EngineSection::Exec => "exec",
        EngineSection::Assembly => "assembly",
    }
}

/// `config_at` as a Python dict.
pub fn config_at_py<'py>(py: Python<'py>, j: &Journal, seq: u64) -> PyResult<Bound<'py, PyDict>> {
    let c = py.detach(|| config_at(j, seq))?;
    let out = PyDict::new(py);
    let refdata = c
        .blobs
        .get(&(blob_kind::REFDATA, 0, 0))
        .and_then(|b| RefData::load(&b.content, None).ok());
    let sp = PyDict::new(py);
    for b in c.blobs.values() {
        let d = base(py, b)?;
        match b.kind {
            blob_kind::REFDATA => {
                if let Some(rd) = &refdata {
                    refdata_dict(py, &d, rd)?;
                }
                out.set_item("refdata", d)?;
            }
            blob_kind::ENGINE => {
                if let Ok(ec) = EngineConfig::load(&b.content, None) {
                    d.set_item("host_class", format!("{:?}", ec.host_class()))?;
                    let secs = PyDict::new(py);
                    for s in EngineSection::ALL {
                        if let Some(v) = ec.section(s) {
                            secs.set_item(section_name(s), PyBytes::new(py, v.bytes))?;
                        }
                    }
                    d.set_item("sections", secs)?;
                }
                out.set_item("engine", d)?;
            }
            blob_kind::STRATEGY_PARAMS => {
                if let Ok((hdr, groups, body)) = crate::params::parse_container(&b.content) {
                    d.set_item("hdr", crate::params::hdr_dict(py, &hdr)?)?;
                    d.set_item("groups", crate::params::groups_list(py, &groups)?)?;
                    d.set_item("body", PyBytes::new(py, &body))?;
                    d.set_item("version", hdr.version)?;
                }
                let key = match &refdata {
                    Some(rd) => {
                        let inst = rd
                            .instance(StrategyId(b.strategy))
                            .map_or_else(|| b.strategy.to_string(), |m| m.name.to_string());
                        let acc = rd.account(AccountId(b.account));
                        let acc = if acc.id == AccountId(b.account) {
                            acc.label.to_string()
                        } else {
                            b.account.to_string()
                        };
                        format!("{inst}/{acc}")
                    }
                    None => format!("{}/{}", b.strategy, b.account),
                };
                sp.set_item(key, d)?;
            }
            blob_kind::RISK_PARAMS => {
                d.set_item("bytes", PyBytes::new(py, &b.content))?;
                out.set_item("risk", d)?;
            }
            blob_kind::HOST_MANIFEST => {
                d.set_item("bytes", PyBytes::new(py, &b.content))?;
                out.set_item("host_manifest", d)?;
            }
            blob_kind::SIM_CONFIG => {
                d.set_item("bytes", PyBytes::new(py, &b.content))?;
                out.set_item("sim_config", d)?;
            }
            _ => {}
        }
    }
    out.set_item("strategy_params", sp)?;
    Ok(out)
}
