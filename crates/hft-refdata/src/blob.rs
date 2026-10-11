//! Segmented container shared by RDB1 and ECF1: header, section directory, RDB1 row formats
//! (zero-copy views) and the structural checks of load step 1.
//!
//! Little endian; every section starts 8-byte aligned; rows are `#[repr(C)]` + zerocopy and are
//! read as a prefix of `row_size` bytes (rows may grow at the end).

use zerocopy::{FromBytes, Immutable, IntoBytes, KnownLayout};

use crate::types::{LoadErr, LoadErrCode};

/// RDB1 magic.
pub const RDB_MAGIC: [u8; 8] = *b"HFTRDB01";
/// ECF1 magic.
pub const ECF_MAGIC: [u8; 8] = *b"HFTECF01";
/// The only container format version.
pub const FORMAT: u16 = 1;
/// `SectionEntry.flags` bit: an unknown section with this bit is skipped.
pub const SECTION_OPTIONAL: u32 = 1;
/// Header length in bytes.
pub const HEADER_LEN: usize = 64;
/// Directory entry length in bytes.
pub const ENTRY_LEN: usize = 24;

/// RDB1 section ids (append-only; 32–37 retired).
pub mod sec {
    pub const ASSETS: u16 = 1;
    pub const VENUES: u16 = 2;
    pub const INSTS: u16 = 3;
    pub const ACCOUNTS: u16 = 4;
    pub const FEES: u16 = 5;
    pub const FEE_OVERRIDES: u16 = 6;
    pub const RATE_LIMITS: u16 = 7;
    pub const ENDPOINTS: u16 = 8;
    pub const MAINT: u16 = 9;
    pub const TOKENS: u16 = 10;
    pub const ALIASES: u16 = 11;
    pub const STRINGS: u16 = 12;
    pub const VENUE_CAPS: u16 = 13;
    pub const ACCOUNT_VENUES: u16 = 14;
    pub const INSTANCES: u16 = 15;
    pub const SIM_ACCOUNTS: u16 = 16;
    pub const SIM_BALANCES: u16 = 17;
    pub const MMR_TIERS: u16 = 18;
    pub const STR_REFS: u16 = 19;
    /// Retired ids: rejected whatever the OPTIONAL bit says.
    pub const fn is_retired(id: u16) -> bool {
        id >= 32 && id <= 37
    }
}

/// Container header (RDB1 and ECF1); ECF1 keeps `host_class` in `_rsv[0]`.
#[repr(C)]
#[derive(Copy, Clone, Eq, PartialEq, Debug, FromBytes, IntoBytes, KnownLayout, Immutable)]
pub struct RdbHeader {
    pub magic: [u8; 8],
    pub format: u16,
    pub n_sections: u16,
    pub _pad: u32,
    pub version: u64,
    /// 0 = full blob (the only supported kind).
    pub base_version: u64,
    pub created_utc_ns: i64,
    pub total_len: u64,
    pub _rsv: [u8; 16],
}
const _: () = assert!(size_of::<RdbHeader>() == 64);
const _: () = assert!(align_of::<RdbHeader>() == 8);
const _: () = assert!(core::mem::offset_of!(RdbHeader, magic) == 0);
const _: () = assert!(core::mem::offset_of!(RdbHeader, format) == 8);
const _: () = assert!(core::mem::offset_of!(RdbHeader, n_sections) == 10);
const _: () = assert!(core::mem::offset_of!(RdbHeader, _pad) == 12);
const _: () = assert!(core::mem::offset_of!(RdbHeader, version) == 16);
const _: () = assert!(core::mem::offset_of!(RdbHeader, base_version) == 24);
const _: () = assert!(core::mem::offset_of!(RdbHeader, created_utc_ns) == 32);
const _: () = assert!(core::mem::offset_of!(RdbHeader, total_len) == 40);
const _: () = assert!(core::mem::offset_of!(RdbHeader, _rsv) == 48);

/// One directory entry; ids are unique within a directory.
#[repr(C)]
#[derive(Copy, Clone, Eq, PartialEq, Debug, FromBytes, IntoBytes, KnownLayout, Immutable)]
pub struct SectionEntry {
    pub id: u16,
    pub row_ver: u16,
    pub row_size: u32,
    pub offset: u64,
    pub count: u32,
    /// `SECTION_OPTIONAL` = 1.
    pub flags: u32,
}
const _: () = assert!(size_of::<SectionEntry>() == 24);
const _: () = assert!(core::mem::offset_of!(SectionEntry, id) == 0);
const _: () = assert!(core::mem::offset_of!(SectionEntry, row_ver) == 2);
const _: () = assert!(core::mem::offset_of!(SectionEntry, row_size) == 4);
const _: () = assert!(core::mem::offset_of!(SectionEntry, offset) == 8);
const _: () = assert!(core::mem::offset_of!(SectionEntry, count) == 16);
const _: () = assert!(core::mem::offset_of!(SectionEntry, flags) == 20);

/// Reference into the Strings section (UTF-8, not NUL-terminated).
#[repr(C)]
#[derive(Copy, Clone, Eq, PartialEq, Debug, Default, FromBytes, IntoBytes, KnownLayout, Immutable)]
pub struct StrRef {
    pub off: u32,
    pub len: u32,
}
const _: () = assert!(size_of::<StrRef>() == 8);

/// Assets (1).
#[repr(C)]
#[derive(Copy, Clone, Eq, PartialEq, Debug, Default, FromBytes, IntoBytes, Immutable)]
pub struct AssetRow {
    pub id: u16,
    pub class: u8,
    pub px_exp: i8,
    pub qty_exp: i8,
    pub _pad: [u8; 3],
    pub name: StrRef,
    pub px_max: i64,
    pub qty_max: i64,
    pub dust: i64,
}
const _: () = assert!(size_of::<AssetRow>() == 40);
const _: () = assert!(align_of::<AssetRow>() <= 8);

/// Venues (2), keyed and sorted by `VenueId`.
#[repr(C)]
#[derive(Copy, Clone, Eq, PartialEq, Debug, Default, FromBytes, IntoBytes, Immutable)]
pub struct VenueRow {
    pub id: u8,
    pub exchange: u8,
    pub product: u8,
    pub ts_unit: u8,
    pub stale_guard: u8,
    pub client_id_max_len: u8,
    pub max_batch: u8,
    pub max_inflight_amend: u8,
    pub ws_max_streams_per_conn: u16,
    pub ws_max_in_msgs_per_s: u16,
    pub ws_new_conn_per_5min_ip: u16,
    pub app_ping_interval_s: u16,
    pub ws_max_conn_age_s: u32,
    pub crossed_tolerance_default: u16,
    pub _pad: u16,
    pub client_id_prefix: StrRef,
}
const _: () = assert!(size_of::<VenueRow>() == 32);
const _: () = assert!(align_of::<VenueRow>() <= 8);

/// Insts (3), dense by `InstId`.
#[repr(C)]
#[derive(Copy, Clone, Eq, PartialEq, Debug, Default, FromBytes, IntoBytes, Immutable)]
pub struct InstRow {
    pub id: u32,
    pub venue: u8,
    pub status: u8,
    pub kind: u8,
    pub flags: u8,
    pub asset: u16,
    pub quote: u16,
    pub book_window: u32,
    pub px_unit_shift: i8,
    pub qty_unit_shift: i8,
    pub l2_depth: u16,
    pub crossed_tolerance: u16,
    pub max_leverage: u16,
    pub tick: i64,
    pub lot: i64,
    pub min_qty: i64,
    pub max_qty: i64,
    pub min_notional: i64,
    pub venue_num_id: u64,
    pub symbol: StrRef,
    pub tick_str: StrRef,
    pub lot_str: StrRef,
    pub min_qty_str: StrRef,
    pub funding_interval_s: u32,
    pub funding_anchor_s: u32,
    pub listed_utc_ns: i64,
    pub delivery_utc_ns: i64,
    pub band_up: i64,
    pub band_down: i64,
}
const _: () = assert!(size_of::<InstRow>() == 144);
const _: () = assert!(align_of::<InstRow>() <= 8);

/// Accounts (4), dense by `AccountId`; `kind` Live 1 / Sim 2, `is_prod` 0/1.
#[repr(C)]
#[derive(Copy, Clone, Eq, PartialEq, Debug, Default, FromBytes, IntoBytes, Immutable)]
pub struct AccountRow {
    pub id: u8,
    pub kind: u8,
    pub bound_instance: u8,
    pub is_prod: u8,
    pub uid_policy: u8,
    pub _pad: [u8; 3],
    pub venues: u64,
    pub label: StrRef,
}
const _: () = assert!(size_of::<AccountRow>() == 24);
const _: () = assert!(align_of::<AccountRow>() <= 8);

/// Fees (5): per (venue, tier).
#[repr(C)]
#[derive(Copy, Clone, Eq, PartialEq, Debug, Default, FromBytes, IntoBytes, Immutable)]
pub struct FeeRow {
    pub venue: u8,
    pub tier: u8,
    pub _pad: [u8; 6],
    pub maker: i64,
    pub taker: i64,
}
const _: () = assert!(size_of::<FeeRow>() == 24);
const _: () = assert!(align_of::<FeeRow>() <= 8);

/// FeeOverrides (6): per (account, venue[, inst]); `inst = 0` = every inst of the venue.
#[repr(C)]
#[derive(Copy, Clone, Eq, PartialEq, Debug, Default, FromBytes, IntoBytes, Immutable)]
pub struct FeeOverrideRow {
    pub account: u8,
    pub venue: u8,
    pub _pad: [u8; 2],
    pub inst: u32,
    pub maker: i64,
    pub taker: i64,
}
const _: () = assert!(size_of::<FeeOverrideRow>() == 24);
const _: () = assert!(align_of::<FeeOverrideRow>() <= 8);

/// RateLimits (7).
#[repr(C)]
#[derive(Copy, Clone, Eq, PartialEq, Debug, Default, FromBytes, IntoBytes, Immutable)]
pub struct RateLimitRow {
    pub venue: u8,
    pub scope: u8,
    pub bucket: u8,
    pub _pad: u8,
    pub limit: u32,
    pub window_ms: u32,
    pub default_cost: u16,
    pub _pad2: u16,
}
const _: () = assert!(size_of::<RateLimitRow>() == 16);
const _: () = assert!(align_of::<RateLimitRow>() <= 8);

/// Endpoints (8).
#[repr(C)]
#[derive(Copy, Clone, Eq, PartialEq, Debug, Default, FromBytes, IntoBytes, Immutable)]
pub struct EndpointRow {
    pub venue: u8,
    pub role: u8,
    pub env: u8,
    pub conn_group: u8,
    pub _pad: u32,
    pub url: StrRef,
}
const _: () = assert!(size_of::<EndpointRow>() == 16);
const _: () = assert!(align_of::<EndpointRow>() <= 8);

/// Maint (9); `inst = 0` = whole venue.
#[repr(C)]
#[derive(Copy, Clone, Eq, PartialEq, Debug, Default, FromBytes, IntoBytes, Immutable)]
pub struct MaintRow {
    pub venue: u8,
    pub kind: u8,
    pub _pad: [u8; 2],
    pub inst: u32,
    pub start_ns: i64,
    pub end_ns: i64,
}
const _: () = assert!(size_of::<MaintRow>() == 24);
const _: () = assert!(align_of::<MaintRow>() <= 8);

/// Tokens (10, optional chain venues).
#[repr(C)]
#[derive(Copy, Clone, Eq, PartialEq, Debug, Default, FromBytes, IntoBytes, Immutable)]
pub struct TokenRow {
    pub chain: u8,
    pub decimals: u8,
    pub asset: u16,
    pub _pad: u32,
    pub address: [u8; 32],
}
const _: () = assert!(size_of::<TokenRow>() == 40);
const _: () = assert!(align_of::<TokenRow>() <= 8);

/// Aliases (11): alternative spelling → inst of the same venue.
#[repr(C)]
#[derive(Copy, Clone, Eq, PartialEq, Debug, Default, FromBytes, IntoBytes, Immutable)]
pub struct AliasRow {
    pub venue: u8,
    pub _pad: [u8; 3],
    pub inst: u32,
    pub alias: StrRef,
}
const _: () = assert!(size_of::<AliasRow>() == 16);
const _: () = assert!(align_of::<AliasRow>() <= 8);

/// VenueCaps (13), keyed and sorted by `VenueId`. `fee_rule` / `margin_data` carry the
/// discriminant, `fee_asset` / `margin_poll_ms` the payload; `multi_asset`, `spot_borrow`
/// use the `Option<bool>` code; `events_off/count` index the StrRefs section (19).
#[repr(C)]
#[derive(Copy, Clone, Eq, PartialEq, Debug, Default, FromBytes, IntoBytes, Immutable)]
pub struct CapsRow {
    pub venue: u8,
    pub market: u8,
    pub fee_rule: u8,
    pub reduce_only: u8,
    pub position_mode: u8,
    pub margin_mode: u8,
    pub multi_asset: u8,
    pub spot_borrow: u8,
    pub funding_settle: u8,
    pub margin_data: u8,
    pub liq_marker: u8,
    pub dms: u8,
    pub testnet: u8,
    pub amend_qty_down_only: u8,
    pub fee_asset: u16,
    pub margin_poll_ms: u32,
    pub flags: u32,
    pub dms_min_ms: u32,
    pub dms_max_ms: u32,
    pub dms_timeout_ms: u32,
    pub dms_refresh_ms: u32,
    pub liq_marker_arg: StrRef,
    pub events_off: u32,
    pub events_count: u32,
}
const _: () = assert!(size_of::<CapsRow>() == 56);
const _: () = assert!(align_of::<CapsRow>() <= 8);

/// AccountVenues (14): exchange UID of a Live account on a venue.
#[repr(C)]
#[derive(Copy, Clone, Eq, PartialEq, Debug, Default, FromBytes, IntoBytes, Immutable)]
pub struct AccountVenueRow {
    pub account: u8,
    pub venue: u8,
    pub _pad: [u8; 6],
    pub uid: StrRef,
}
const _: () = assert!(size_of::<AccountVenueRow>() == 16);
const _: () = assert!(align_of::<AccountVenueRow>() <= 8);

/// Instances (15), dense by `StrategyInstanceId`.
#[repr(C)]
#[derive(Copy, Clone, Eq, PartialEq, Debug, Default, FromBytes, IntoBytes, Immutable)]
pub struct InstanceRow {
    pub id: u8,
    pub _pad: u8,
    pub kind_id: u16,
    pub _pad2: u32,
    pub name: StrRef,
}
const _: () = assert!(size_of::<InstanceRow>() == 16);
const _: () = assert!(align_of::<InstanceRow>() <= 8);

/// SimAccounts (16): fixed part of `SimAccountCfg`.
#[repr(C)]
#[derive(Copy, Clone, Eq, PartialEq, Debug, Default, FromBytes, IntoBytes, Immutable)]
pub struct SimAccountRow {
    pub account: u8,
    pub fee_tier: u8,
    pub latency_model: u16,
    pub margin_model: u16,
    pub _pad: u16,
}
const _: () = assert!(size_of::<SimAccountRow>() == 8);
const _: () = assert!(align_of::<SimAccountRow>() <= 8);

/// SimBalances (17).
#[repr(C)]
#[derive(Copy, Clone, Eq, PartialEq, Debug, Default, FromBytes, IntoBytes, Immutable)]
pub struct SimBalanceRow {
    pub account: u8,
    pub venue: u8,
    pub asset: u16,
    pub _pad: u32,
    pub qty: i64,
}
const _: () = assert!(size_of::<SimBalanceRow>() == 16);
const _: () = assert!(align_of::<SimBalanceRow>() <= 8);

/// MmrTiers (18): per (inst, tier), ascending `notional_max` within an inst.
#[repr(C)]
#[derive(Copy, Clone, Eq, PartialEq, Debug, Default, FromBytes, IntoBytes, Immutable)]
pub struct MmrTierRow {
    pub inst: u32,
    pub _pad: u32,
    pub notional_max: i64,
    pub mmr: i64,
}
const _: () = assert!(size_of::<MmrTierRow>() == 24);
const _: () = assert!(align_of::<MmrTierRow>() <= 8);

/// Known row size of an RDB1 section (`None` = unknown id). Strings rows are bytes.
pub const fn rdb_row_size(id: u16) -> Option<usize> {
    Some(match id {
        sec::ASSETS => size_of::<AssetRow>(),
        sec::VENUES => size_of::<VenueRow>(),
        sec::INSTS => size_of::<InstRow>(),
        sec::ACCOUNTS => size_of::<AccountRow>(),
        sec::FEES => size_of::<FeeRow>(),
        sec::FEE_OVERRIDES => size_of::<FeeOverrideRow>(),
        sec::RATE_LIMITS => size_of::<RateLimitRow>(),
        sec::ENDPOINTS => size_of::<EndpointRow>(),
        sec::MAINT => size_of::<MaintRow>(),
        sec::TOKENS => size_of::<TokenRow>(),
        sec::ALIASES => size_of::<AliasRow>(),
        sec::STRINGS => 1,
        sec::VENUE_CAPS => size_of::<CapsRow>(),
        sec::ACCOUNT_VENUES => size_of::<AccountVenueRow>(),
        sec::INSTANCES => size_of::<InstanceRow>(),
        sec::SIM_ACCOUNTS => size_of::<SimAccountRow>(),
        sec::SIM_BALANCES => size_of::<SimBalanceRow>(),
        sec::MMR_TIERS => size_of::<MmrTierRow>(),
        sec::STR_REFS => size_of::<StrRef>(),
        _ => return None,
    })
}

/// Which container is being parsed.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub(crate) enum Kind {
    Rdb,
    Ecf,
}

/// A section that passed the container checks.
#[derive(Copy, Clone, Debug)]
pub(crate) struct Sect<'a> {
    pub id: u16,
    pub row_ver: u16,
    pub row_size: u32,
    pub count: u32,
    /// Byte offset of `data` in the blob.
    pub offset: usize,
    pub data: &'a [u8],
}

impl<'a> Sect<'a> {
    /// Rows read as `row_size`-byte prefixes.
    pub fn rows<T: FromBytes + Copy>(&self) -> Vec<T> {
        let stride = self.row_size as usize;
        if stride == 0 {
            return Vec::new();
        }
        self.data
            .chunks_exact(stride)
            .filter_map(|c| T::read_from_prefix(c).ok().map(|(t, _)| t))
            .collect()
    }
}

/// Load step 1, container part: header, directory, bounds, overlap, alignment, row sizes,
/// unknown and retired sections. Header problems end parsing with one `{Structure, 0, 0}`;
/// section problems add one `{Structure, id, 0}` per section and drop that section.
pub(crate) fn parse_container<'a>(
    blob: &'a [u8],
    kind: Kind,
    errs: &mut Vec<LoadErr>,
) -> Option<(RdbHeader, Vec<Sect<'a>>)> {
    let Ok((h, _)) = RdbHeader::read_from_prefix(blob) else {
        errs.push(LoadErr::new(LoadErrCode::Structure, 0, 0));
        return None;
    };
    let magic = match kind {
        Kind::Rdb => RDB_MAGIC,
        Kind::Ecf => ECF_MAGIC,
    };
    let host_ok = match kind {
        Kind::Rdb => true,
        Kind::Ecf => (1..=6).contains(&h._rsv[0]),
    };
    let n = usize::from(h.n_sections);
    let dir_end = HEADER_LEN + ENTRY_LEN * n;
    if h.magic != magic
        || h.format != FORMAT
        || h.version == 0
        || h.total_len != blob.len() as u64
        || h.base_version != 0
        || !host_ok
        || dir_end > blob.len()
    {
        errs.push(LoadErr::new(LoadErrCode::Structure, 0, 0));
        return None;
    }
    let entries: Vec<SectionEntry> = blob
        .get(HEADER_LEN..dir_end)
        .unwrap_or(&[])
        .chunks_exact(ENTRY_LEN)
        .filter_map(|c| SectionEntry::read_from_bytes(c).ok())
        .collect();
    // First occurrence of each id (sorted copy; O(n log n) for hostile directories).
    let mut by_id: Vec<(u16, usize)> = entries.iter().enumerate().map(|(i, e)| (e.id, i)).collect();
    by_id.sort_unstable();
    let mut dup = vec![false; entries.len()];
    for w in by_id.windows(2) {
        if let [a, b] = w
            && a.0 == b.0
            && let Some(d) = dup.get_mut(b.1)
        {
            *d = true;
        }
    }
    let mut local: Vec<LoadErr> = Vec::new();
    let mut ok: Vec<(u64, u64, Sect<'a>)> = Vec::new();
    for (i, e) in entries.iter().enumerate() {
        let bad = |local: &mut Vec<LoadErr>| local.push(LoadErr::new(LoadErrCode::Structure, e.id, 0));
        if dup.get(i).copied().unwrap_or(true) {
            bad(&mut local);
            continue;
        }
        let known = match kind {
            Kind::Rdb => {
                if sec::is_retired(e.id) {
                    bad(&mut local);
                    continue;
                }
                rdb_row_size(e.id)
            }
            Kind::Ecf => crate::EngineSection::try_from(e.id).ok().map(|_| 0),
        };
        let Some(known) = known else {
            if e.flags & SECTION_OPTIONAL == 0 {
                bad(&mut local);
            }
            continue;
        };
        let len = u128::from(e.row_size) * u128::from(e.count);
        let end = u128::from(e.offset) + len;
        if e.offset % 8 != 0
            || e.offset < dir_end as u64
            || end > blob.len() as u128
            || (e.row_size as usize) < known
        {
            bad(&mut local);
            continue;
        }
        let (start, stop) = (e.offset as usize, end as usize);
        let Some(data) = blob.get(start..stop) else {
            bad(&mut local);
            continue;
        };
        let s = Sect {
            id: e.id,
            row_ver: e.row_ver,
            row_size: e.row_size,
            count: e.count,
            offset: start,
            data,
        };
        ok.push((e.offset, end as u64, s));
    }
    // Overlap among the surviving non-empty sections: the later one (by offset) is reported.
    ok.sort_by_key(|(start, end, s)| (*start, *end, s.id));
    let mut prev_end = 0u64;
    let mut sects = Vec::with_capacity(ok.len());
    for (start, end, s) in ok {
        if end > start {
            if start < prev_end {
                local.push(LoadErr::new(LoadErrCode::Structure, s.id, 0));
                continue;
            }
            prev_end = end;
        }
        sects.push(s);
    }
    local.sort_by_key(|e| (e.section, e.row));
    errs.extend(local);
    sects.sort_by_key(|s| s.id);
    Some((h, sects))
}

/// One section of a `RawBlob` (directory fields + row bytes). Tool and test hook.
#[derive(Clone, Eq, PartialEq, Debug)]
pub struct RawSection {
    pub id: u16,
    pub row_ver: u16,
    pub row_size: u32,
    pub count: u32,
    pub flags: u32,
    pub data: Vec<u8>,
}

impl RawSection {
    /// Byte range of row `i`.
    pub fn row_range(&self, i: usize) -> Option<core::ops::Range<usize>> {
        let rs = self.row_size as usize;
        let start = i.checked_mul(rs)?;
        let end = start.checked_add(rs)?;
        (end <= self.data.len()).then_some(start..end)
    }
    /// Row `i` read as `T` (prefix).
    pub fn read_row<T: FromBytes>(&self, i: usize) -> Option<T> {
        let r = self.row_range(i)?;
        T::read_from_prefix(self.data.get(r)?).ok().map(|(t, _)| t)
    }
    /// Overwrites the prefix of row `i` with `v`; false if out of range.
    pub fn write_row<T: IntoBytes + Immutable>(&mut self, i: usize, v: &T) -> bool {
        let Some(r) = self.row_range(i) else { return false };
        let b = v.as_bytes();
        match self.data.get_mut(r.start..r.start.saturating_add(b.len())) {
            Some(dst) if b.len() <= r.len() => {
                dst.copy_from_slice(b);
                true
            }
            _ => false,
        }
    }
}

/// Editable form of a container: header fields + sections in layout order. `encode` lays the
/// sections out in vector order (8-byte aligned) after the directory. Tool and test hook.
#[derive(Clone, Eq, PartialEq, Debug)]
pub struct RawBlob {
    pub magic: [u8; 8],
    pub format: u16,
    pub version: u64,
    pub base_version: u64,
    pub created_utc_ns: i64,
    pub rsv: [u8; 16],
    pub sections: Vec<RawSection>,
}

const fn align8(x: usize) -> usize {
    (x + 7) & !7
}

impl RawBlob {
    pub fn section(&self, id: u16) -> Option<&RawSection> {
        self.sections.iter().find(|s| s.id == id)
    }
    pub fn section_mut(&mut self, id: u16) -> Option<&mut RawSection> {
        self.sections.iter_mut().find(|s| s.id == id)
    }

    /// Serialized bytes with a consistent directory and `total_len`.
    pub fn encode(&self) -> Vec<u8> {
        let n = self.sections.len();
        let mut pos = align8(HEADER_LEN + ENTRY_LEN * n);
        let mut dir = Vec::with_capacity(n);
        for s in &self.sections {
            dir.push(SectionEntry {
                id: s.id,
                row_ver: s.row_ver,
                row_size: s.row_size,
                offset: pos as u64,
                count: s.count,
                flags: s.flags,
            });
            pos = align8(pos + s.data.len());
        }
        let mut out = vec![0u8; pos];
        let h = RdbHeader {
            magic: self.magic,
            format: self.format,
            n_sections: n as u16,
            _pad: 0,
            version: self.version,
            base_version: self.base_version,
            created_utc_ns: self.created_utc_ns,
            total_len: pos as u64,
            _rsv: self.rsv,
        };
        write_at(&mut out, 0, h.as_bytes());
        for (i, (e, s)) in dir.iter().zip(&self.sections).enumerate() {
            write_at(&mut out, HEADER_LEN + ENTRY_LEN * i, e.as_bytes());
            write_at(&mut out, e.offset as usize, &s.data);
        }
        out
    }

    /// Parses a container without semantic checks (sections in directory order); `None` when
    /// the header or a section lies outside the bytes.
    pub fn parse(b: &[u8]) -> Option<RawBlob> {
        let (h, _) = RdbHeader::read_from_prefix(b).ok()?;
        let mut sections = Vec::new();
        for i in 0..usize::from(h.n_sections) {
            let at = HEADER_LEN + ENTRY_LEN * i;
            let (e, _) = SectionEntry::read_from_prefix(b.get(at..)?).ok()?;
            let len = (e.row_size as usize).checked_mul(e.count as usize)?;
            let start = e.offset as usize;
            let data = b.get(start..start.checked_add(len)?)?.to_vec();
            sections.push(RawSection {
                id: e.id,
                row_ver: e.row_ver,
                row_size: e.row_size,
                count: e.count,
                flags: e.flags,
                data,
            });
        }
        Some(RawBlob {
            magic: h.magic,
            format: h.format,
            version: h.version,
            base_version: h.base_version,
            created_utc_ns: h.created_utc_ns,
            rsv: h._rsv,
            sections,
        })
    }
}

fn write_at(out: &mut [u8], at: usize, b: &[u8]) {
    if let Some(dst) = out.get_mut(at..at.saturating_add(b.len())) {
        dst.copy_from_slice(b);
    }
}
