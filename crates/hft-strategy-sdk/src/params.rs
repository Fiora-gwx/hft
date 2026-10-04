//! Parameter schema and the single parameter codec (README "参数 schema 与 ParamSet" and
//! "参数编解码器(`params` 模块,唯一实现)"). Every encoder and decoder of strategy parameter
//! bodies and `StrategyCmd` payloads in the repository calls these functions: `define_params!`,
//! ctl-gateway, hft-cfg and hft-py. Cold path: may allocate; never panics.

mod body;
mod codec;
mod minor;
mod text;

#[cfg(feature = "schema-json")]
mod json;
#[cfg(feature = "schema-json")]
mod owned;
#[cfg(feature = "schema-json")]
mod schema_json;

pub use body::{BodyView, ParamOverride, ParamsBodyHdr, body_view};
pub use codec::{decode_body, decode_cmd, encode_body, encode_cmd, field_ix, n_fields_at};
pub use minor::{MajorChange, minor_change_check};
#[cfg(feature = "schema-json")]
pub use owned::{OwnedCmd, OwnedField, OwnedSchema};
#[cfg(feature = "schema-json")]
pub use schema_json::{schema_digest, schema_json};
pub use text::{format_value, parse_value};

pub use crate::desc::CmdSchema;

/// `group_ix` of errors on a default value or a command field (not group-specific).
pub const GROUP_IX_NONE: u16 = 0xFFFF;

/// A strategy's parameter schema (`define_params!` generates it; fields only appended).
#[derive(Clone, Copy, Debug)]
pub struct ParamSchemaDesc {
    /// = `kind_id`.
    pub id: u32,
    pub version: u16,
    pub fields: &'static [FieldDesc],
}

/// One parameter (or command payload) field. `min`/`max`/`default` are raw fixed-point values.
#[derive(Clone, Copy, Debug)]
pub struct FieldDesc {
    pub name: &'static str,
    pub unit: Unit,
    pub min: i64,
    pub max: i64,
    pub default: i64,
    /// May be overridden per group.
    pub per_group: bool,
    /// Changing the value changes the interpretation of stored Model samples.
    pub structural: bool,
    /// Only set by humans (hft-cfg, manual overrides); pipeline plugins may not own it.
    pub manual_only: bool,
    /// Which direction of change is a relaxation.
    pub relax: Relax,
    /// Gate ⑦ minor-change rule.
    pub minor_change: MinorChange,
    /// Schema version that added the field; non-decreasing over `fields`.
    pub since: u16,
    pub doc: &'static str,
}

impl FieldDesc {
    /// Borrowed form at index `ix` (what the codec reads).
    pub fn as_ref(&self, ix: u16) -> FieldRef<'_> {
        FieldRef {
            ix,
            name: self.name,
            unit: self.unit.kind(),
            enum_names: match self.unit {
                Unit::Enum(n) => EnumNames::Static(n),
                _ => EnumNames::Static(&[]),
            },
            min: self.min,
            max: self.max,
            default: self.default,
            per_group: self.per_group,
            structural: self.structural,
            manual_only: self.manual_only,
            relax: self.relax,
            minor_change: self.minor_change,
            since: self.since,
            doc: self.doc,
        }
    }
}

/// `Up` = a larger value relaxes (limits); `Down` = a smaller value relaxes (thresholds);
/// `None` = no monotone direction (e.g. "0 = off").
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Relax {
    Up,
    Down,
    None,
}

/// Gate ⑦ minor-change rule (semantics in README; checked by `minor_change_check`).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MinorChange {
    /// Every change is major (default).
    None,
    /// `|new − old| ≤ |old| × n / 100` is minor; old = 0 → any change is major.
    RelPct(u16),
    /// `|new − old| ≤ v` (raw) is minor.
    Abs(i64),
    /// Every change is minor.
    Any,
}

/// Unit of a field value.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Unit {
    /// 1e-9 dimensionless ratio: 1 bp = 100_000, 1 % = 10_000_000.
    Rate,
    /// USDT 1e-6.
    Notional,
    /// At the group asset's `qty_exp`; only `per_group = true`.
    Qty,
    /// At the group asset's `px_exp`; only `per_group = true`.
    Px,
    DurationMs,
    Count,
    Bool,
    /// Value = index into the names.
    Enum(&'static [&'static str]),
}

impl Unit {
    /// The unit without its enum names.
    pub const fn kind(self) -> UnitKind {
        match self {
            Unit::Rate => UnitKind::Rate,
            Unit::Notional => UnitKind::Notional,
            Unit::Qty => UnitKind::Qty,
            Unit::Px => UnitKind::Px,
            Unit::DurationMs => UnitKind::DurationMs,
            Unit::Count => UnitKind::Count,
            Unit::Bool => UnitKind::Bool,
            Unit::Enum(_) => UnitKind::Enum,
        }
    }
}

/// `Unit` without the enum names (`FieldRef` carries the names separately).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UnitKind {
    Rate,
    Notional,
    Qty,
    Px,
    DurationMs,
    Count,
    Bool,
    Enum,
}

impl UnitKind {
    /// Name in the canonical schema JSON.
    pub const fn json_name(self) -> &'static str {
        match self {
            UnitKind::Rate => "rate",
            UnitKind::Notional => "notional",
            UnitKind::Qty => "qty",
            UnitKind::Px => "px",
            UnitKind::DurationMs => "duration_ms",
            UnitKind::Count => "count",
            UnitKind::Bool => "bool",
            UnitKind::Enum => "enum",
        }
    }

    /// Inverse of `json_name`.
    pub fn from_json_name(s: &str) -> Option<UnitKind> {
        [
            UnitKind::Rate,
            UnitKind::Notional,
            UnitKind::Qty,
            UnitKind::Px,
            UnitKind::DurationMs,
            UnitKind::Count,
            UnitKind::Bool,
            UnitKind::Enum,
        ]
        .into_iter()
        .find(|u| u.json_name() == s)
    }
}

/// Enum value names of a field: compiled-in or restored from JSON.
#[derive(Clone, Copy, Debug)]
pub enum EnumNames<'a> {
    Static(&'static [&'static str]),
    Owned(&'a [String]),
}

impl<'a> EnumNames<'a> {
    pub fn len(&self) -> usize {
        match self {
            EnumNames::Static(n) => n.len(),
            EnumNames::Owned(n) => n.len(),
        }
    }
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
    pub fn get(&self, i: usize) -> Option<&'a str> {
        match *self {
            EnumNames::Static(n) => n.get(i).copied(),
            EnumNames::Owned(n) => n.get(i).map(String::as_str),
        }
    }
    /// Index of `name` (exact match).
    pub fn position(&self, name: &str) -> Option<usize> {
        (0..self.len()).find(|&i| self.get(i) == Some(name))
    }
}

/// Borrowed form of a `FieldDesc` (`name` as `&str`), plus its index in the schema or command.
#[derive(Clone, Copy, Debug)]
pub struct FieldRef<'a> {
    pub ix: u16,
    pub name: &'a str,
    pub unit: UnitKind,
    /// Empty unless `unit = Enum`.
    pub enum_names: EnumNames<'a>,
    pub min: i64,
    pub max: i64,
    pub default: i64,
    pub per_group: bool,
    pub structural: bool,
    pub manual_only: bool,
    pub relax: Relax,
    pub minor_change: MinorChange,
    pub since: u16,
    pub doc: &'a str,
}

/// What the codec reads from a schema: the compiled-in `&'static ParamSchemaDesc` and the
/// JSON-restored `OwnedSchema` both implement it.
pub trait SchemaView {
    fn id(&self) -> u32;
    fn version(&self) -> u16;
    fn n_fields(&self) -> u16;
    /// `None` when `ix ≥ n_fields()`.
    fn field(&self, ix: u16) -> Option<FieldRef<'_>>;
}

impl SchemaView for ParamSchemaDesc {
    fn id(&self) -> u32 {
        self.id
    }
    fn version(&self) -> u16 {
        self.version
    }
    fn n_fields(&self) -> u16 {
        u16::try_from(self.fields.len()).unwrap_or(u16::MAX)
    }
    fn field(&self, ix: u16) -> Option<FieldRef<'_>> {
        self.fields.get(usize::from(ix)).map(|f| f.as_ref(ix))
    }
}

/// Parameter decoding and validation errors (also used by the command codec).
/// `group_ix = 0xFFFF` (`GROUP_IX_NONE`): the error is on a default value or a command field.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ParamError {
    SchemaMismatch {
        want: u32,
        got: u32,
    },
    SchemaTooNew {
        have: u16,
        got: u16,
    },
    BodyLen,
    OverrideOrder {
        ix: u32,
    },
    Padding,
    /// `body_view` on a body that is not 8-byte aligned (zero-copy view impossible);
    /// `decode_body` copies such bodies and never returns it.
    Misaligned,
    OutOfRange {
        field_ix: u16,
        group_ix: u16,
    },
    BadEnum {
        field_ix: u16,
    },
    NotPerGroup {
        field_ix: u16,
    },
    GroupIxOutOfRange {
        group_ix: u16,
    },
    TooManyGroups,
    GroupShape {
        gix: u16,
        code: u16,
    },
    Invalid {
        field_ix: u16,
        code: u16,
    },
    BadText {
        field_ix: u16,
    },
    UnknownOp {
        op: u8,
    },
}

/// Untyped parameter values: one default per field (schema order) and the override table.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct RawParams {
    pub defaults: Vec<i64>,
    pub overrides: Vec<ParamOverride>,
}

/// Range and enum check of one value of field `f` (`group_ix` for the error).
pub(crate) fn check_value(f: &FieldRef<'_>, v: i64, group_ix: u16) -> Result<(), ParamError> {
    if f.unit == UnitKind::Enum {
        let ok = usize::try_from(v).is_ok_and(|i| i < f.enum_names.len());
        if !ok {
            return Err(ParamError::BadEnum { field_ix: f.ix });
        }
    }
    if v < f.min || v > f.max {
        return Err(ParamError::OutOfRange {
            field_ix: f.ix,
            group_ix,
        });
    }
    Ok(())
}
