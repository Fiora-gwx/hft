//! Strategy descriptors exported with the canonical schema JSON: Note code table, daily-report
//! fields and strategy command payloads (README "策略描述符:Note、报表、策略命令").

use hft_event::NoteLevel;

use crate::params::{FieldDesc, FieldRef};

/// One Note code a strategy writes (`Ctx::note`).
#[derive(Clone, Copy, Debug)]
pub struct NoteDesc {
    /// Unique within the strategy kind, `1..=0xFF_FFFF` (0 reserved).
    pub code: u32,
    pub name: &'static str,
    /// Strategies use `P1 | P2 | Info` only.
    pub level: NoteLevel,
    /// Meaning and unit of `v[0..8]` (≤ 8 entries; unused ones omitted).
    pub v: &'static [&'static str],
    pub doc: &'static str,
}

/// Aggregation of one daily-report field over a UTC day.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReportAgg {
    Count,
    Sum,
    Last,
    Max,
    P99,
}

/// Row key of one daily-report field.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReportScope {
    Instance,
    /// One row per `Note.group`.
    Group,
}

/// One strategy-specific daily-report column (aggregated from a registered Note).
#[derive(Clone, Copy, Debug)]
pub struct ReportFieldDesc {
    pub name: &'static str,
    pub unit: crate::params::Unit,
    pub agg: ReportAgg,
    pub scope: ReportScope,
    /// Source Note code (must be in `notes`).
    pub note: u32,
    /// Sample = `v[v_ix]` of that Note; ignored when `agg = Count`.
    pub v_ix: u8,
    pub doc: &'static str,
}

/// One accepted `StrategyCmd` payload shape.
#[derive(Clone, Copy, Debug)]
pub struct CmdDesc {
    /// Unique within the strategy, `1..=255`.
    pub op: u8,
    /// ops_bot / hft-ctl sub-command name.
    pub name: &'static str,
    /// `StrategyCmd.reduce_only` the tools send with this command.
    pub reduce_only: bool,
    /// Payload fields (`per_group`, `structural` false, `minor_change = None`; ≤ 31).
    pub fields: &'static [FieldDesc],
    pub doc: &'static str,
}

/// What `params::encode_cmd`/`decode_cmd` read from a command description: implemented by the
/// compiled-in `CmdDesc` and by `params::OwnedCmd` (restored from schema JSON).
pub trait CmdSchema {
    fn op(&self) -> u8;
    fn n_fields(&self) -> u16;
    /// `None` when `ix ≥ n_fields()`.
    fn field(&self, ix: u16) -> Option<FieldRef<'_>>;
}

impl CmdSchema for CmdDesc {
    fn op(&self) -> u8 {
        self.op
    }
    fn n_fields(&self) -> u16 {
        u16::try_from(self.fields.len()).unwrap_or(u16::MAX)
    }
    fn field(&self, ix: u16) -> Option<FieldRef<'_>> {
        self.fields.get(usize::from(ix)).map(|f| f.as_ref(ix))
    }
}

/// `CmdView::parse` failure.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum CmdError {
    /// Shorter than the 8 B header.
    TooShort,
    /// `ver ≠ 1`.
    BadVersion,
    /// `n > 31`.
    TooMany,
    /// Length ≠ 8 + 8 × n.
    LenMismatch,
}

/// Borrowed `StrategyCmd` payload: `{ver: u8 = 1, op: u8, n: u8, _pad: [u8; 5]}` + `vals: [i64; n]`
/// (little-endian, n ≤ 31). Fast-part parsing: allocation-free, O(1).
#[derive(Clone, Copy, Debug)]
pub struct CmdView<'a> {
    op: u8,
    n: u8,
    vals: &'a [u8],
}

impl<'a> CmdView<'a> {
    /// Checks in order: length < 8 → `TooShort`; `ver ≠ 1` → `BadVersion`; `n > 31` → `TooMany`;
    /// length ≠ 8 + 8n → `LenMismatch`. The padding and the values are not checked (the gateway's
    /// `params::decode_cmd` already did).
    pub fn parse(payload: &'a [u8]) -> Result<CmdView<'a>, CmdError> {
        let [ver, op, n, _, _, _, _, _, vals @ ..] = payload else {
            return Err(CmdError::TooShort);
        };
        if *ver != 1 {
            return Err(CmdError::BadVersion);
        }
        if usize::from(*n) > crate::check::MAX_CMD_FIELDS {
            return Err(CmdError::TooMany);
        }
        if vals.len() != 8 * usize::from(*n) {
            return Err(CmdError::LenMismatch);
        }
        Ok(CmdView { op: *op, n: *n, vals })
    }
    pub fn op(&self) -> u8 {
        self.op
    }
    /// Number of values.
    pub fn len(&self) -> u8 {
        self.n
    }
    pub fn is_empty(&self) -> bool {
        self.n == 0
    }
    /// Raw fixed-point value `ix`; `ix ≥ n` → `None`.
    pub fn val(&self, ix: u8) -> Option<i64> {
        let s = usize::from(ix).checked_mul(8)?;
        let b: [u8; 8] = self.vals.get(s..s + 8)?.try_into().ok()?;
        Some(i64::from_le_bytes(b))
    }
}
