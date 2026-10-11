//! `EngineConfig` (ECF1): engine assembly parameters in the RDB1 container. This crate does
//! the container checks; each section's row format and semantic check belong to the owning
//! crate (`EngineValidator`).

use hft_types::HostClass;
use sha2::{Digest, Sha256};

use crate::RefData;
use crate::blob::{Kind, parse_container};
use crate::types::{LoadErr, LoadErrCode, LoadReport};

/// ECF1 section ids (append-only, never reused).
#[repr(u16)]
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub enum EngineSection {
    Oms = 1,
    Ledger = 2,
    Capacities = 3,
    Rings = 4,
    Sequencer = 5,
    Conn = 6,
    Exec = 7,
    Assembly = 8,
}

impl EngineSection {
    pub const ALL: [EngineSection; 8] = [
        EngineSection::Oms,
        EngineSection::Ledger,
        EngineSection::Capacities,
        EngineSection::Rings,
        EngineSection::Sequencer,
        EngineSection::Conn,
        EngineSection::Exec,
        EngineSection::Assembly,
    ];
}

impl TryFrom<u16> for EngineSection {
    type Error = u16;
    fn try_from(v: u16) -> Result<Self, u16> {
        let ix = usize::from(v).checked_sub(1).ok_or(v)?;
        EngineSection::ALL.get(ix).copied().ok_or(v)
    }
}

/// Raw view of one section (rows are parsed by the owning crate).
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub struct SectionView<'a> {
    pub row_ver: u16,
    pub row_size: u32,
    pub count: u32,
    pub bytes: &'a [u8],
}

/// Semantic validator of one section, provided by the owning crate. Cold path.
#[derive(Copy, Clone, Debug)]
pub struct EngineValidator {
    pub section: EngineSection,
    pub check: fn(&EngineConfig, &RefData) -> Result<(), LoadErr>,
}

#[derive(Clone, Debug)]
struct Section {
    id: EngineSection,
    row_ver: u16,
    row_size: u32,
    count: u32,
    start: usize,
    len: usize,
}

/// Immutable engine configuration.
#[derive(Clone, Debug)]
pub struct EngineConfig {
    version: u64,
    sha256: [u8; 32],
    host: HostClass,
    blob: Box<[u8]>,
    /// Ascending by id.
    sections: Box<[Section]>,
}

const fn host_class(code: u8) -> Option<HostClass> {
    match code {
        1 => Some(HostClass::Dev2),
        2 => Some(HostClass::C4),
        3 => Some(HostClass::C8),
        4 => Some(HostClass::Metal),
        5 => Some(HostClass::C16),
        6 => Some(HostClass::C32),
        _ => None,
    }
}

impl EngineConfig {
    /// Container-level load (what cold readers use): structure as RDB1 step 1 with magic
    /// `HFTECF01` and a legal `host_class`, then the version rule of step 10.
    pub fn load(blob: &[u8], current: Option<&EngineConfig>) -> Result<EngineConfig, LoadReport> {
        let sha256: [u8; 32] = Sha256::digest(blob).into();
        if let Some(c) = current
            && c.sha256 == sha256
        {
            return Err(LoadReport {
                errors: vec![LoadErr::new(LoadErrCode::AlreadyActive, 0, 0)],
            });
        }
        let mut errors = Vec::new();
        let Some((hdr, sects)) = parse_container(blob, Kind::Ecf, &mut errors) else {
            return Err(LoadReport { errors });
        };
        if !errors.is_empty() {
            return Err(LoadReport { errors });
        }
        let Some(host) = host_class(hdr._rsv[0]) else {
            return Err(LoadReport {
                errors: vec![LoadErr::new(LoadErrCode::Structure, 0, 0)],
            });
        };
        if let Some(c) = current
            && hdr.version <= c.version
        {
            return Err(LoadReport {
                errors: vec![LoadErr::new(LoadErrCode::VersionNotIncreasing, 0, 0)],
            });
        }
        let sections = sects
            .iter()
            .filter_map(|s| {
                Some(Section {
                    id: EngineSection::try_from(s.id).ok()?,
                    row_ver: s.row_ver,
                    row_size: s.row_size,
                    count: s.count,
                    start: s.offset,
                    len: s.data.len(),
                })
            })
            .collect();
        Ok(EngineConfig {
            version: hdr.version,
            sha256,
            host,
            blob: blob.into(),
            sections,
        })
    }

    /// Calls, in ascending section id, the validator of every present section; a present
    /// section without a validator → `NoValidator`. Validator errors are collected as returned.
    pub fn validate(&self, rd: &RefData, validators: &[EngineValidator]) -> Result<(), LoadReport> {
        let mut errors = Vec::new();
        for s in self.sections.iter() {
            match validators.iter().find(|v| v.section == s.id) {
                Some(v) => {
                    if let Err(e) = (v.check)(self, rd) {
                        errors.push(e);
                    }
                }
                None => errors.push(LoadErr::new(LoadErrCode::NoValidator, s.id as u16, 0)),
            }
        }
        if errors.is_empty() {
            Ok(())
        } else {
            Err(LoadReport { errors })
        }
    }

    /// Raw view of a section; absent → `None`.
    pub fn section(&self, s: EngineSection) -> Option<SectionView<'_>> {
        let x = self.sections.iter().find(|x| x.id == s)?;
        Some(SectionView {
            row_ver: x.row_ver,
            row_size: x.row_size,
            count: x.count,
            bytes: self.blob.get(x.start..x.start.checked_add(x.len)?)?,
        })
    }

    pub fn host_class(&self) -> HostClass {
        self.host
    }
    pub fn version(&self) -> u64 {
        self.version
    }
    pub fn sha256(&self) -> &[u8; 32] {
        &self.sha256
    }
}
