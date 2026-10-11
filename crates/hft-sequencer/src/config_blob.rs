//! Reconstruct assembly inputs from the journal's complete ECF1 blob. No filesystem, environment,
//! default config or process-local callbacks are consulted by the decoding path.
use hft_event::{BlobHdr, BlobKind, BlobRef, EventKind, SnapshotHdr};
use hft_refdata::{AccountKind, EcfBuilder, EngineConfig, EngineSection, LoadErrCode, RefData};
use hft_strategy_sdk::{SnapshotReader, VenueSet};
use hft_types::{AccountId, HostClass, VenueId};
use sha2::{Digest, Sha256};
use zerocopy::{FromBytes, Immutable, IntoBytes, KnownLayout};

use crate::driver::replay::Log;
use crate::{BootError, ConfigErr, EngineSetup, SegClock};

pub const ASSEMBLY_ROW_VERSION: u16 = 1;

/// ECF1 section 8, 16 bytes. Tags: 1 role, 2 advisor lag, 3 drain timeout, 4 ordered gw,
/// 5 feed venue, 6 required key-slot bitmap. No credentials or key references are stored.
#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq, FromBytes, IntoBytes, KnownLayout, Immutable)]
pub struct AssemblyRow {
    pub field: u16,
    pub index: u16,
    pub account: u8,
    pub venue: u8,
    pub reserved: [u8; 2],
    pub value: u64,
}
const _: () = assert!(size_of::<AssemblyRow>() == 16);

fn err() -> ConfigErr {
    ConfigErr::Blob {
        kind: BlobKind::Engine as u16,
        code: LoadErrCode::Structure as u16,
    }
}

/// Canonical risk decoder used by replay and production assembly; caller still validates scope
/// and risk/exec consistency through the usual `Risk::new` / `set_params` path.
pub fn decode_risk_blob(b: &[u8], _a: AccountId, rd: &RefData) -> Result<hft_risk::RiskParams, u16> {
    hft_risk::RiskParams::decode_blob(b, rd).map_err(|_| crate::codes::detail::BAD_VALUE)
}

fn content_before<'a>(log: &'a Log, ix: usize, wanted: &BlobRef) -> Result<&'a [u8], ConfigErr> {
    for r in log.recs.iter().take(ix).rev() {
        if r.kind != EventKind::CONFIG_BLOB {
            continue;
        }
        let Some(h) = r.read::<BlobHdr>() else { continue };
        if h.blob != *wanted {
            continue;
        }
        let bytes = r
            .body
            .get(72..72 + wanted.len as usize)
            .ok_or(ConfigErr::Missing)?;
        let sha: [u8; 32] = Sha256::digest(bytes).into();
        if sha != wanted.sha256 {
            return Err(ConfigErr::Blob {
                kind: wanted.kind as u16,
                code: LoadErrCode::Structure as u16,
            });
        }
        return Ok(bytes);
    }
    Err(ConfigErr::Missing)
}

impl EngineSetup {
    /// Decode the active configuration at an exact snapshot record. Only preceding, fully
    /// matching and SHA-256-checked ConfigBlob records may satisfy its references. Run facts
    /// come from that snapshot and its segment; another world's assembly cannot fill any gap.
    pub fn from_snapshot(log: &Log, seq: u64) -> Result<Self, BootError> {
        let bad = |section| BootError::SnapshotUndecodable { section };
        let ix = log
            .recs
            .iter()
            .position(|r| r.seq == seq && r.kind == EventKind::STATE_SNAPSHOT)
            .ok_or(BootError::Journal(crate::JournalErr::NoSnapshot))?;
        let body = log
            .recs
            .get(ix)
            .and_then(|r| r.body.get(size_of::<SnapshotHdr>()..))
            .ok_or_else(|| bad(0))?;
        let sections = crate::snapshot::read_seq_sections(body).map_err(bad)?;
        for (blob, _) in &sections.active {
            content_before(log, ix, blob).map_err(BootError::Config)?;
        }
        let active = |kind| {
            let mut rows = sections.active.iter().filter(|(b, _)| b.kind == kind);
            let (b, _) = rows.next().ok_or_else(|| bad(9))?;
            if rows.next().is_some() || b.encoding != 1 {
                return Err(bad(9));
            }
            Ok(*b)
        };
        let rd_ref = active(BlobKind::Refdata)?;
        let engine_ref = active(BlobKind::Engine)?;
        let rd_bytes = content_before(log, ix, &rd_ref).map_err(BootError::Config)?;
        let engine_bytes = content_before(log, ix, &engine_ref).map_err(BootError::Config)?;
        let rd = RefData::load(rd_bytes, None).map_err(|r| {
            BootError::Config(ConfigErr::Blob {
                kind: BlobKind::Refdata as u16,
                code: r.code() as u16,
            })
        })?;
        let r = SnapshotReader::new(body);
        let counters = r
            .find_section(hft_oms::SECTION_OMS_COUNTERS)
            .map_err(|_| bad(3))?
            .ok_or_else(|| bad(3))?;
        if counters.version != hft_oms::ROW_VERSION_COUNTERS {
            return Err(bad(3));
        }
        let epoch: hft_oms::OmsCountersHdr = counters.reader().get_row().map_err(|_| bad(3))?;
        let clock = *log.segs.get(log.seg_of(ix)).ok_or_else(|| bad(16))?;
        if sections.engine_clock.tsc_hz_milli == 0 || sections.engine_clock.tsc_hz_milli != clock.tsc_hz_milli
        {
            return Err(bad(25));
        }
        Self::decode_blob(
            engine_bytes,
            &rd,
            clock,
            epoch.engine_epoch,
            sections.engine_clock.base_engine_ns,
        )
        .map_err(BootError::Config)
    }

    /// Complete engine configuration for a boot block. The run's epoch and clock are supplied
    /// separately by the snapshot / segment header, never encoded as mutable configuration.
    pub fn encode_blob(&self, version: u64, host: HostClass, created_utc_ns: i64) -> Vec<u8> {
        let mut b = EcfBuilder::new(version, host);
        b.created_utc_ns(created_utc_ns);
        let oms = self.oms.to_rows();
        b.section(
            EngineSection::Oms,
            hft_oms::OMS_CONFIG_ROW_VERSION,
            16,
            oms.len() as u32,
            oms.as_slice().as_bytes().to_vec(),
        );
        b.section(
            EngineSection::Ledger,
            hft_ledger::LEDGER_ROW_VERSION,
            512,
            1,
            self.ledger.to_row().as_bytes().to_vec(),
        );
        b.section(
            EngineSection::Capacities,
            crate::params::CAPACITIES_ROW_VERSION,
            24,
            1,
            self.caps.as_bytes().to_vec(),
        );
        b.section(
            EngineSection::Sequencer,
            crate::params::SEQ_PARAMS_ROW_VERSION,
            72,
            1,
            self.seq.as_bytes().to_vec(),
        );
        let exec = self.exec.to_rows();
        b.section(
            EngineSection::Exec,
            hft_exec::EXEC_CONFIG_ROW_VERSION,
            176,
            exec.len() as u32,
            exec.as_slice().as_bytes().to_vec(),
        );
        let mut rows = Vec::new();
        let mut row = |field, index, account, venue, value| {
            rows.push(AssemblyRow {
                field,
                index,
                account,
                venue,
                reserved: [0; 2],
                value,
            })
        };
        row(1, 0, 0, 0, u64::from(self.role_prod));
        row(2, 0, 0, 0, u64::from(self.advisor_max_lag_ms));
        row(3, 0, 0, 0, u64::from(self.drain_timeout_ms));
        for (i, v) in self.gws.venues.iter().enumerate() {
            row(4, i as u16, 0, v.0, 0);
        }
        for v in 1..64u8 {
            if self.feeds.0 & (1u64 << v) != 0 {
                row(5, 0, 0, v, 0);
            }
        }
        for (a, v, bits) in &self.keys_required {
            row(6, 0, a.0, v.0, u64::from(*bits));
        }
        rows.sort_by_key(|r| (r.field, r.index, r.account, r.venue));
        b.section(
            EngineSection::Assembly,
            ASSEMBLY_ROW_VERSION,
            16,
            rows.len() as u32,
            rows.as_slice().as_bytes().to_vec(),
        );
        b.finish()
    }

    /// Assemble the configuration recorded in the journal. Missing sections or unresolved
    /// mappings fail before constructing the engine; no `OmsConfig` / `ExecConfig` defaults fill
    /// holes. `clock`, `epoch` and `base_engine_ns` are restored from that same journal.
    pub fn decode_blob(
        blob: &[u8],
        rd: &RefData,
        clock: SegClock,
        epoch: u32,
        base_engine_ns: i64,
    ) -> Result<Self, ConfigErr> {
        let ec = EngineConfig::load(blob, None).map_err(|_| err())?;
        let caps = crate::params::capacities(&ec).map_err(|_| err())?;
        caps.check(hft_refdata::DEFAULT_MAX_UNIVERSE_INSTS)
            .map_err(|_| err())?;
        let seq = crate::params::seq_params(&ec).map_err(|_| err())?;
        seq.check().map_err(|_| err())?;
        for (section, ver) in [
            (EngineSection::Capacities, crate::params::CAPACITIES_ROW_VERSION),
            (EngineSection::Sequencer, crate::params::SEQ_PARAMS_ROW_VERSION),
        ] {
            if ec.section(section).is_none_or(|s| s.row_ver != ver) {
                return Err(err());
            }
        }
        let oms = hft_oms::OmsConfig::from_engine(&ec, rd, epoch, clock.tsc_hz_milli).map_err(|_| err())?;
        let ledger = hft_ledger::LedgerConfig::from_engine(&ec).map_err(|_| err())?;
        let exec = hft_exec::ExecConfig::from_engine(&ec, rd).map_err(|_| err())?;
        let s = ec.section(EngineSection::Assembly).ok_or_else(err)?;
        if s.row_ver != ASSEMBLY_ROW_VERSION || s.row_size != 16 || clock.tsc_hz_milli == 0 {
            return Err(err());
        }
        let mut role_prod = None;
        let mut advisor_max_lag_ms = None;
        let mut drain_timeout_ms = None;
        let mut gws = hft_risk::GwMap { venues: Vec::new() };
        let mut feeds = VenueSet(0);
        let mut keys_required = Vec::new();
        let mut prev = None;
        for b in s.bytes.chunks_exact(16) {
            let r = AssemblyRow::read_from_bytes(b).map_err(|_| err())?;
            let key = (r.field, r.index, r.account, r.venue);
            if r.reserved != [0; 2] || prev.is_some_and(|p| p >= key) {
                return Err(err());
            }
            prev = Some(key);
            if r.field <= 3 && (r.index != 0 || r.account != 0 || r.venue != 0) {
                return Err(err());
            }
            if r.field >= 4
                && (!(1..64).contains(&r.venue) || rd.venue(VenueId(r.venue)).id != VenueId(r.venue))
            {
                return Err(err());
            }
            match r.field {
                1 if r.value <= 1 => role_prod = Some(r.value == 1),
                2 => advisor_max_lag_ms = Some(u32::try_from(r.value).map_err(|_| err())?),
                3 => drain_timeout_ms = Some(u32::try_from(r.value).map_err(|_| err())?),
                4 if usize::from(r.index) == gws.venues.len() && r.account == 0 && r.value == 0 => {
                    let v = VenueId(r.venue);
                    if gws.venues.contains(&v) {
                        return Err(err());
                    }
                    gws.venues.push(v);
                }
                5 if r.index == 0 && r.account == 0 && r.value == 0 => feeds.0 |= 1u64 << r.venue,
                6 if r.index == 0 && r.account != 0 && r.value <= 3 => {
                    let a = rd.account(AccountId(r.account));
                    if a.id.0 == 0
                        || matches!(a.kind, AccountKind::Sim(_))
                        || a.venues & (1u64 << r.venue) == 0
                    {
                        return Err(err());
                    }
                    keys_required.push((a.id, VenueId(r.venue), r.value as u8));
                }
                _ => return Err(err()),
            }
        }
        // Live account key requirements are facts, including an explicit empty bitmap.
        for a in 1..=u8::MAX {
            let meta = rd.account(AccountId(a));
            if meta.id.0 == 0 || metmatches!(a.kind, AccountKind::Sim(_)) {
                continue;
            }
            for v in 1..64u8 {
                if meta.venues & (1u64 << v) != 0
                    && !keys_required.iter().any(|r| r.0 == meta.id && r.1 == VenueId(v))
                {
                    return Err(err());
                }
            }
        }
        Ok(Self {
            caps,
            seq,
            oms,
            ledger,
            exec,
            gws,
            feeds,
            keys_required,
            role_prod: role_prod.ok_or_else(err)?,
            advisor_max_lag_ms: advisor_max_lag_ms.ok_or_else(err)?,
            drain_timeout_ms: drain_timeout_ms.ok_or_else(err)?,
            risk_decode: Some(decode_risk_blob),
            clock,
            base_engine_ns,
        })
    }
}
