//! Complete ECF1 section 7 encoding. Cold path, before constructing execution cells.
use hft_refdata::{AccountKind, EngineConfig, EngineSection, LoadErr, LoadErrCode, RefData};
use hft_strategy_sdk::TakerSliceParams;
use hft_types::{AccountId, Notional, Rate};
use zerocopy::{FromBytes, Immutable, IntoBytes, KnownLayout};

use crate::{ExecAccount, ExecConfig, ExecParams};

pub const EXEC_CONFIG_ROW_VERSION: u16 = 1;

/// Section 7, 176 bytes. Account 0 is the mandatory default; other rows declare each account
/// and whether it overrides that default. Unused parameter words and all padding must be zero.
#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq, FromBytes, IntoBytes, KnownLayout, Immutable)]
pub struct ExecConfigRow {
    pub account: u8,
    pub sim: u8,
    pub has_params: u8,
    pub reserved: [u8; 5],
    pub params: [i64; 21],
}
const _: () = assert!(size_of::<ExecConfigRow>() == 176);

impl ExecParams {
    fn words(&self) -> [i64; 21] {
        [
            self.band_floor_notional.0,
            self.dust_notional.0,
            self.fix_tolerance_default.0,
            i64::from(self.fix_max),
            i64::from(self.fix_cooldown_ms),
            i64::from(self.fix_deadline_ms),
            i64::from(self.broken_auto_fix_ms),
            i64::from(self.external_deadline_ms),
            i64::from(self.gate_naked_alarm_ms),
            i64::from(self.stuck_after_ms),
            i64::from(self.gated_backoff_ms),
            i64::from(self.max_slice_fails),
            i64::from(self.max_nofill),
            i64::from(self.urgent_cooldown_ms),
            i64::from(self.passive_cooldown_mult),
            self.force_slice.depth_frac.0,
            i64::from(self.force_slice.depth_levels),
            self.force_slice.max_cross.0,
            self.force_slice.max_chunk_notional.0,
            self.force_slice.min_chunk_notional.0,
            i64::from(self.force_slice.cooldown_ms),
        ]
    }
    fn from_words(w: [i64; 21]) -> Option<Self> {
        let [
            band,
            dust,
            tolerance,
            fix_max,
            fix_cooldown,
            fix_deadline,
            broken,
            external,
            gate,
            stuck,
            gated,
            fails,
            nofill,
            urgent,
            passive,
            depth_frac,
            depth_levels,
            cross,
            max_chunk,
            min_chunk,
            cooldown,
        ] = w;
        Some(Self {
            band_floor_notional: Notional(band),
            dust_notional: Notional(dust),
            fix_tolerance_default: Rate(tolerance),
            fix_max: fix_max.try_into().ok()?,
            fix_cooldown_ms: fix_cooldown.try_into().ok()?,
            fix_deadline_ms: fix_deadline.try_into().ok()?,
            broken_auto_fix_ms: broken.try_into().ok()?,
            external_deadline_ms: external.try_into().ok()?,
            gate_naked_alarm_ms: gate.try_into().ok()?,
            stuck_after_ms: stuck.try_into().ok()?,
            gated_backoff_ms: gated.try_into().ok()?,
            max_slice_fails: fails.try_into().ok()?,
            max_nofill: nofill.try_into().ok()?,
            urgent_cooldown_ms: urgent.try_into().ok()?,
            passive_cooldown_mult: passive.try_into().ok()?,
            force_slice: TakerSliceParams {
                depth_frac: Rate(depth_frac),
                depth_levels: depth_levels.try_into().ok()?,
                _pad0: [0; 7],
                max_cross: Rate(cross),
                max_chunk_notional: Notional(max_chunk),
                min_chunk_notional: Notional(min_chunk),
                cooldown_ms: cooldown.try_into().ok()?,
                _pad1: 0,
            },
        })
    }
}

impl ExecConfig {
    /// Explicit default plus account declarations, ordered by account id.
    pub fn to_rows(&self) -> Vec<ExecConfigRow> {
        let mut rows = vec![ExecConfigRow {
            account: 0,
            sim: 0,
            has_params: 1,
            reserved: [0; 5],
            params: self.default.words(),
        }];
        rows.extend(self.accounts.iter().map(|a| ExecConfigRow {
            account: a.account.0,
            sim: u8::from(a.sim),
            has_params: u8::from(a.params.is_some()),
            reserved: [0; 5],
            params: a.params.map_or([0; 21], |p| p.words()),
        }));
        rows.sort_by_key(|r| r.account);
        rows
    }

    /// No implicit defaults on load. Missing default, duplicates, invalid flags or widths fail;
    /// `Exec::new` performs the execution-limit checks after assembling the whole configuration.
    pub fn from_rows(rows: &[ExecConfigRow]) -> Option<Self> {
        let d = rows.first()?;
        if d.account != 0 || d.sim != 0 || d.has_params != 1 {
            return None;
        }
        let default = ExecParams::from_words(d.params)?;
        let mut accounts = Vec::new();
        let mut prev = None;
        for r in rows {
            if r.reserved != [0; 5] || r.sim > 1 || r.has_params > 1 || prev.is_some_and(|a| a >= r.account) {
                return None;
            }
            prev = Some(r.account);
            if r.account == 0 {
                continue;
            }
            let params = if r.has_params == 1 {
                Some(ExecParams::from_words(r.params)?)
            } else {
                if r.params != [0; 21] {
                    return None;
                }
                None
            };
            accounts.push(ExecAccount {
                account: AccountId(r.account),
                sim: r.sim == 1,
                params,
            });
        }
        Some(Self { default, accounts })
    }

    pub fn from_engine(ec: &EngineConfig, rd: &RefData) -> Result<Self, LoadErr> {
        let err = LoadErr::new(LoadErrCode::Structure, EngineSection::Exec as u16, 0);
        let s = ec.section(EngineSection::Exec).ok_or(err)?;
        if s.row_ver != EXEC_CONFIG_ROW_VERSION || s.row_size != 176 {
            return Err(err);
        }
        let rows: Vec<_> = s
            .bytes
            .chunks_exact(176)
            .map(ExecConfigRow::read_from_bytes)
            .collect::<Result<_, _>>()
            .map_err(|_| err)?;
        let cfg = Self::from_rows(&rows).ok_or(err)?;
        for id in 1..=u8::MAX {
            let a = rd.account(AccountId(id));
            let entry = cfg.accounts.iter().find(|x| x.account == AccountId(id));
            match (a.id.0 != 0, entry) {
                (true, Some(x)) if x.sim == (a.kind == AccountKind::Sim) => {}
                (false, None) => {}
                _ => return Err(err),
            }
        }
        Ok(cfg)
    }
}
