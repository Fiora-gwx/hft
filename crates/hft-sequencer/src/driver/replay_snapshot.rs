//! Offline snapshot self-check before any replay input is delivered. The state is rebuilt by
//! the production restore path, using only this log's configuration, then serialized again.

use hft_event::{EventKind, SnapshotHdr};
use hft_strategy_sdk::{SnapshotReader, SnapshotWriter, StrategySet};

use super::replay::Log;
use crate::{BootError, EngineSetup, recover, snapshot};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SnapshotCheck {
    pub seq: u64,
    pub state_hash: [u8; 16],
    pub sections: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SnapshotCheckError {
    Missing,
    Header,
    /// Invalid section framing, duplicate section id, or header count mismatch.
    Sections,
    DeclaredHash {
        declared: [u8; 16],
        computed: [u8; 16],
    },
    Restore(BootError),
    /// Restored state needs more space than the original section bytes.
    Size,
    RoundTrip {
        section: u16,
        logged_hash: [u8; 16],
        restored_hash: [u8; 16],
    },
}

fn frames(bytes: &[u8]) -> Result<Vec<(u16, &[u8])>, SnapshotCheckError> {
    let mut r = SnapshotReader::new(bytes);
    let mut out = Vec::new();
    loop {
        let start = r.pos();
        let Some(s) = r.next_section().map_err(|_| SnapshotCheckError::Sections)? else {
            return Ok(out);
        };
        if out.iter().any(|(id, _)| *id == s.section) {
            return Err(SnapshotCheckError::Sections);
        }
        let frame = bytes.get(start..r.pos()).ok_or(SnapshotCheckError::Sections)?;
        out.push((s.section, frame));
    }
}

/// Check the exact selected snapshot; never fall back to a different snapshot. Both zero and
/// nonzero declared hashes require a restored-state round trip. Section 16 is the only hash
/// exclusion, as defined by `snapshot::state_hash`. Source bytes and running engines are untouched.
pub fn check_snapshot<S: StrategySet>(log: &Log, seq: u64) -> Result<SnapshotCheck, SnapshotCheckError> {
    let ix = log
        .recs
        .iter()
        .position(|r| r.seq == seq && r.kind == EventKind::STATE_SNAPSHOT)
        .ok_or(SnapshotCheckError::Missing)?;
    let rec = &log.recs[ix];
    let hdr = rec.read::<SnapshotHdr>().ok_or(SnapshotCheckError::Header)?;
    if !(snapshot::PURPOSE_BOOT..=snapshot::PURPOSE_DRAIN).contains(&hdr.purpose)
        || hdr._pad != [0; 3]
        || hdr._pad2 != 0
        || hdr.at_seq > seq
    {
        return Err(SnapshotCheckError::Header);
    }
    let bytes = rec
        .body
        .get(size_of::<SnapshotHdr>()..)
        .ok_or(SnapshotCheckError::Header)?;
    let logged = frames(bytes)?;
    if logged.len() != hdr.n_sections as usize || logged.is_empty() {
        return Err(SnapshotCheckError::Sections);
    }
    let computed = snapshot::state_hash(bytes);
    if hdr.state_hash != [0; 16] && hdr.state_hash != computed {
        return Err(SnapshotCheckError::DeclaredHash {
            declared: hdr.state_hash,
            computed,
        });
    }
    let setup = EngineSetup::from_snapshot(log, seq).map_err(SnapshotCheckError::Restore)?;
    let (e, inst) = recover::restore_at::<S>(log, ix, &setup, None).map_err(SnapshotCheckError::Restore)?;
    let mut w = SnapshotWriter::new(bytes.len());
    snapshot::write_sections(&e, &inst, &mut w);
    w.status().map_err(|_| SnapshotCheckError::Size)?;
    let restored_hash = snapshot::state_hash(w.as_bytes());
    if computed != restored_hash {
        let restored = frames(w.as_bytes())?;
        let mut a = logged.iter().filter(|(id, _)| *id != snapshot::SECTION_CLOCK_CAL);
        let mut b = restored
            .iter()
            .filter(|(id, _)| *id != snapshot::SECTION_CLOCK_CAL);
        let section = loop {
            match (a.next(), b.next()) {
                (Some(x), Some(y)) if x == y => {}
                (Some(x), _) => break x.0,
                (None, Some(y)) => break y.0,
                (None, None) => break 0,
            }
        };
        return Err(SnapshotCheckError::RoundTrip {
            section,
            logged_hash: computed,
            restored_hash,
        });
    }
    Ok(SnapshotCheck {
        seq,
        state_hash: computed,
        sections: hdr.n_sections,
    })
}
