//! ECF1 section 1: complete OMS parameters, with canonical default and override rows.
use hft_refdata::{EngineConfig, EngineSection, LoadErr, LoadErrCode, RefData};
use hft_types::{AccountId, Rate, VenueId};
use zerocopy::{FromBytes, Immutable, IntoBytes, KnownLayout};

use crate::OmsConfig;

pub const OMS_CONFIG_ROW_VERSION: u16 = 1;

/// Section 1, 16 bytes. `field` 1..=25 follows the README table. (0, 0) is the default;
/// venue / table overrides carry their nonzero key. `value = -1` encodes an optional None.
#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq, FromBytes, IntoBytes, KnownLayout, Immutable)]
pub struct OmsConfigRow {
    pub field: u16,
    pub venue: u8,
    pub account: u8,
    pub reserved: u32,
    pub value: i64,
}
const _: () = assert!(size_of::<OmsConfigRow>() == 16);

fn put(out: &mut Vec<OmsConfigRow>, field: u16, venue: u8, account: u8, value: i64) {
    if let Some(r) = out
        .iter_mut()
        .find(|r| (r.field, r.venue, r.account) == (field, venue, account))
    {
        r.value = value;
    } else {
        out.push(OmsConfigRow {
            field,
            venue,
            account,
            reserved: 0,
            value,
        });
    }
}

fn optional<T: TryFrom<i64>>(v: i64) -> Option<Option<T>> {
    if v == -1 {
        Some(None)
    } else {
        T::try_from(v).ok().map(Some)
    }
}

impl OmsConfig {
    /// Cold-path encoding. All defaults are written explicitly. Repeated override keys use the
    /// same last-value rule as `PerVenue::get` / `PerTable::get`, then rows are sorted by key.
    pub fn to_rows(&self) -> Vec<OmsConfigRow> {
        let mut out = Vec::new();
        macro_rules! plain {
            ($id:expr, $f:ident) => {
                put(&mut out, $id, 0, 0, i64::from(self.$f));
            };
        }
        macro_rules! venue {
            ($id:expr, $f:ident, $cv:expr) => {{
                let cv = $cv;
                put(&mut out, $id, 0, 0, cv(self.$f.default));
                for (v, x) in &self.$f.overrides {
                    put(&mut out, $id, v.0, 0, cv(*x));
                }
            }};
        }
        macro_rules! table {
            ($id:expr, $f:ident, $cv:expr) => {{
                let cv = $cv;
                put(&mut out, $id, 0, 0, cv(self.$f.default));
                for (v, a, x) in &self.$f.overrides {
                    put(&mut out, $id, v.0, a.0, cv(*x));
                }
            }};
        }
        table!(1, slots, i64::from);
        plain!(2, links_per_account);
        plain!(3, max_groups);
        venue!(4, ack_timeout_ms, i64::from);
        venue!(5, cancel_timeout_ms, i64::from);
        venue!(6, ioc_timeout_ms, i64::from);
        plain!(7, unknown_query_interval_ms);
        plain!(8, unknown_max_queries);
        plain!(9, unknown_alarm_ms);
        table!(10, out_quota, |v: Option<u16>| v.map_or(-1, i64::from));
        venue!(11, not_found_grace_ms, |v: Option<u32>| v.map_or(-1, i64::from));
        venue!(12, expire_window_ms, |v: Option<u32>| v.map_or(-1, i64::from));
        put(&mut out, 13, 0, 0, self.smoke_far_bp.0);
        plain!(14, reuse_quarantine_ms);
        plain!(15, tombstone_gens);
        plain!(16, tombstone_keep_ms);
        plain!(17, sweep_batch);
        plain!(18, max_out_per_event);
        plain!(19, hedge_retry_max);
        plain!(20, hedge_retry_min_ms);
        plain!(21, hedge_deadline_ms);
        plain!(22, resume_hold_max_ms);
        table!(23, foreign_fill_track, i64::from);
        table!(24, ext_held_track, i64::from);
        plain!(25, decode_fail_down_n);
        out.sort_by_key(|r| (r.field, r.venue, r.account));
        out
    }

    /// Decode the complete section. Epoch and frequency are run facts, supplied by the journal
    /// snapshot and clock. Missing defaults, duplicates, invalid keys/widths/padding fail.
    pub fn from_rows(rows: &[OmsConfigRow], epoch: u32, hz_milli: u64) -> Option<Self> {
        let mut c = Self::new(epoch, hz_milli);
        let mut mask = 0u32;
        let mut prev = None;
        for r in rows {
            let key = (r.field, r.venue, r.account);
            if r.reserved != 0 || prev.is_some_and(|p| p >= key) || !(1..=25).contains(&r.field) {
                return None;
            }
            prev = Some(key);
            let is_default = r.venue == 0 && r.account == 0;
            let table = matches!(r.field, 1 | 10 | 23 | 24);
            let venue = matches!(r.field, 4..=6 | 11 | 12);
            if !is_default
                && (!(1..64).contains(&r.venue)
                    || (!table && !venue)
                    || (table && r.account == 0)
                    || (venue && r.account != 0))
            {
                return None;
            }
            if is_default {
                mask |= 1 << (r.field - 1);
            }
            macro_rules! plain {
                ($f:ident) => {
                    c.$f = r.value.try_into().ok()?
                };
            }
            macro_rules! venue {
                ($f:ident, $value:expr) => {{
                    let value = $value;
                    if is_default {
                        c.$f.default = value;
                    } else {
                        c.$f.set(VenueId(r.venue), value);
                    }
                }};
            }
            macro_rules! table {
                ($f:ident, $value:expr) => {{
                    let value = $value;
                    if is_default {
                        c.$f.default = value;
                    } else {
                        c.$f.set(VenueId(r.venue), AccountId(r.account), value);
                    }
                }};
            }
            match r.field {
                1 => table!(slots, u32::try_from(r.value).ok()?),
                2 => plain!(links_per_account),
                3 => plain!(max_groups),
                4 => venue!(ack_timeout_ms, u32::try_from(r.value).ok()?),
                5 => venue!(cancel_timeout_ms, u32::try_from(r.value).ok()?),
                6 => venue!(ioc_timeout_ms, u32::try_from(r.value).ok()?),
                7 => plain!(unknown_query_interval_ms),
                8 => plain!(unknown_max_queries),
                9 => plain!(unknown_alarm_ms),
                10 => table!(out_quota, optional::<u16>(r.value)?),
                11 => venue!(not_found_grace_ms, optional::<u32>(r.value)?),
                12 => venue!(expire_window_ms, optional::<u32>(r.value)?),
                13 => c.smoke_far_bp = Rate(r.value),
                14 => plain!(reuse_quarantine_ms),
                15 => plain!(tombstone_gens),
                16 => plain!(tombstone_keep_ms),
                17 => plain!(sweep_batch),
                18 => plain!(max_out_per_event),
                19 => plain!(hedge_retry_max),
                20 => plain!(hedge_retry_min_ms),
                21 => plain!(hedge_deadline_ms),
                22 => plain!(resume_hold_max_ms),
                23 => table!(foreign_fill_track, u16::try_from(r.value).ok()?),
                24 => table!(ext_held_track, u16::try_from(r.value).ok()?),
                25 => plain!(decode_fail_down_n),
                _ => return None,
            }
        }
        (mask == (1 << 25) - 1).then_some(c)
    }

    pub fn from_engine(ec: &EngineConfig, rd: &RefData, epoch: u32, hz_milli: u64) -> Result<Self, LoadErr> {
        let err = LoadErr::new(LoadErrCode::Structure, EngineSection::Oms as u16, 0);
        let s = ec.section(EngineSection::Oms).ok_or(err)?;
        if s.row_ver != OMS_CONFIG_ROW_VERSION || s.row_size != 16 {
            return Err(err);
        }
        let rows: Vec<_> = s
            .bytes
            .chunks_exact(16)
            .map(OmsConfigRow::read_from_bytes)
            .collect::<Result<_, _>>()
            .map_err(|_| err)?;
        for r in &rows {
            if r.venue != 0 && rd.venue(VenueId(r.venue)).id != VenueId(r.venue) {
                return Err(err);
            }
            if r.account != 0
                && (r.venue >= 64 || rd.account(AccountId(r.account)).venues & (1u64 << r.venue) == 0)
            {
                return Err(err);
            }
        }
        Self::from_rows(&rows, epoch, hz_milli).ok_or(err)
    }
}
