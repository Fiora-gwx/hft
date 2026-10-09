//! `define_params!` (README "schema JSON 与 define_params!", "define_params! 的展开"): one
//! declaration generates the typed field struct, the `ParamSchemaDesc` constant with its
//! compile-time checks, `ParamSet` (decode through `params::decode_body`, per-gix override table,
//! `validate` → the strategy's `check`, `structural_changed`).

use hft_types::{Notional, Px, Qty, Rate};

use crate::group::GroupTable;
use crate::params::ParamError;
use crate::views::RefView;

/// Typed member of a generated field struct ↔ raw fixed-point value.
pub trait FieldValue: Copy {
    /// From a raw value already range-checked by the codec (out-of-type values saturate).
    fn from_raw(v: i64) -> Self;
    fn to_raw(self) -> i64;
}

macro_rules! newtype_value {
    ($($t:ident),*) => {$(
        impl FieldValue for $t {
            #[inline]
            fn from_raw(v: i64) -> Self {
                $t(v)
            }
            #[inline]
            fn to_raw(self) -> i64 {
                self.0
            }
        }
    )*};
}
newtype_value!(Rate, Notional, Qty, Px);

impl FieldValue for u32 {
    #[inline]
    fn from_raw(v: i64) -> Self {
        u32::try_from(v.max(0)).unwrap_or(u32::MAX)
    }
    #[inline]
    fn to_raw(self) -> i64 {
        i64::from(self)
    }
}

impl FieldValue for u8 {
    #[inline]
    fn from_raw(v: i64) -> Self {
        u8::try_from(v.max(0)).unwrap_or(u8::MAX)
    }
    #[inline]
    fn to_raw(self) -> i64 {
        i64::from(self)
    }
}

impl FieldValue for bool {
    #[inline]
    fn from_raw(v: i64) -> Self {
        v != 0
    }
    #[inline]
    fn to_raw(self) -> i64 {
        i64::from(self)
    }
}

/// Cross-field and group-shape check hooked into the generated `ParamSet::validate`. The default
/// accepts everything; a strategy writes an inherent `fn check(&self, groups, rd)` on its
/// parameter type, which takes precedence over this trait method.
pub trait ParamCheck {
    fn check(&self, groups: &GroupTable, rd: &RefView<'_>) -> Result<(), ParamError> {
        let _ = (groups, rd);
        Ok(())
    }
}

/// Raw-value helpers for `define_params!` bounds and defaults.
pub mod rate {
    /// Basis points → `Rate` raw (1 bp = 100 000).
    pub const fn bp(x: i64) -> i64 {
        x.saturating_mul(100_000)
    }
    /// Percent → `Rate` raw (1 % = 10 000 000).
    pub const fn pct(x: i64) -> i64 {
        x.saturating_mul(10_000_000)
    }
}

#[doc(hidden)]
#[macro_export]
macro_rules! __param_ty {
    (Rate) => {
        $crate::prelude::Rate
    };
    (Notional) => {
        $crate::prelude::Notional
    };
    (Qty) => {
        $crate::prelude::Qty
    };
    (Px) => {
        $crate::prelude::Px
    };
    (DurationMs) => {
        u32
    };
    (Count) => {
        u32
    };
    (Bool) => {
        bool
    };
    (Enum) => {
        u8
    };
}

#[doc(hidden)]
#[macro_export]
macro_rules! __param_base {
    ($name:ident, Enum [$($en:literal),*], $doc:expr) => {
        $crate::params::FieldDesc {
            name: stringify!($name),
            unit: $crate::params::Unit::Enum(&[$($en),*]),
            min: 0,
            max: [$($en),*].len() as i64 - 1,
            default: 0,
            per_group: false,
            structural: false,
            manual_only: false,
            relax: $crate::params::Relax::None,
            minor_change: $crate::params::MinorChange::None,
            since: 1,
            doc: $doc,
        }
    };
    ($name:ident, Bool [], $doc:expr) => {
        $crate::params::FieldDesc {
            unit: $crate::params::Unit::Bool,
            max: 1,
            ..$crate::__param_base!($name, Count [], $doc)
        }
    };
    ($name:ident, $unit:ident [], $doc:expr) => {
        $crate::params::FieldDesc {
            name: stringify!($name),
            unit: $crate::params::Unit::$unit,
            min: 0,
            max: 0,
            default: 0,
            per_group: false,
            structural: false,
            manual_only: false,
            relax: $crate::params::Relax::None,
            minor_change: $crate::params::MinorChange::None,
            since: 1,
            doc: $doc,
        }
    };
}

#[doc(hidden)]
#[macro_export]
macro_rules! __param_attrs {
    // Each attribute is one assignment to the `FieldDesc` being built (a `let mut` inside a const
    // block). Nested struct-update expressions (`FieldDesc { a, ..FieldDesc { b, ..base } }`) made
    // rustc's const evaluation grow exponentially with the attribute count (6 attributes on one
    // field took > 6 GB per rustc), so attributes are applied as sequential statements.
    ($fd:ident;) => {};
    ($fd:ident; min = $v:expr $(, $($r:tt)*)?) => {
        $fd.min = $v;
        $crate::__param_attrs!($fd; $($($r)*)?);
    };
    ($fd:ident; max = $v:expr $(, $($r:tt)*)?) => {
        $fd.max = $v;
        $crate::__param_attrs!($fd; $($($r)*)?);
    };
    ($fd:ident; default = $v:expr $(, $($r:tt)*)?) => {
        $fd.default = $v;
        $crate::__param_attrs!($fd; $($($r)*)?);
    };
    ($fd:ident; since = $v:expr $(, $($r:tt)*)?) => {
        $fd.since = $v;
        $crate::__param_attrs!($fd; $($($r)*)?);
    };
    ($fd:ident; per_group $(, $($r:tt)*)?) => {
        $fd.per_group = true;
        $crate::__param_attrs!($fd; $($($r)*)?);
    };
    ($fd:ident; structural $(, $($r:tt)*)?) => {
        $fd.structural = true;
        $crate::__param_attrs!($fd; $($($r)*)?);
    };
    ($fd:ident; manual_only $(, $($r:tt)*)?) => {
        $fd.manual_only = true;
        $crate::__param_attrs!($fd; $($($r)*)?);
    };
    ($fd:ident; relax = $v:ident $(, $($r:tt)*)?) => {
        $fd.relax = $crate::params::Relax::$v;
        $crate::__param_attrs!($fd; $($($r)*)?);
    };
    ($fd:ident; minor = $v:ident $(($a:expr))? $(, $($r:tt)*)?) => {
        $fd.minor_change = $crate::params::MinorChange::$v $(($a))?;
        $crate::__param_attrs!($fd; $($($r)*)?);
    };
}

/// Declares a strategy's parameters (README "schema JSON 与 define_params!").
///
/// ```ignore
/// hft_strategy_sdk::define_params! {
///     pub struct FooParams(schema_id = 7, version = 2, fields = FooFields) {
///         /// open threshold
///         open_th: Rate [min = 0, max = rate::pct(10), default = rate::bp(50), per_group, relax = Down, minor = RelPct(10)],
///         cooldown_ms: DurationMs [min = 0, max = 600_000, default = 20_000, relax = Down],
///         mode: Enum(["a", "b"]) [default = 0, since = 2, manual_only],
///     }
/// }
/// ```
///
/// Generates `FooFields` (one typed member per field: `Rate`, `Notional`, `Qty`, `Px`, `u32` for
/// `DurationMs`/`Count`, `bool`, `u8` enum index), `FooParams { defaults, per-gix table }` with
/// `FooParams::SCHEMA`/`FIELDS`, `defaults()`, `at(gix)` (O(1)), `n_groups()`, and
/// `impl ParamSet` whose `validate` calls `self.check(groups, rd)` — the strategy's inherent
/// `check` when written, otherwise `ParamCheck`'s accept-all default. The schema is checked at
/// compile time (`check::validate_schema`).
#[macro_export]
macro_rules! define_params {
    (
        $(#[$sm:meta])*
        $vis:vis struct $name:ident(schema_id = $sid:expr, version = $ver:expr, fields = $fname:ident $(,)?) {
            $(
                $(#[doc = $doc:literal])*
                $f:ident : $unit:ident $(([$($en:literal),* $(,)?]))? [$($attr:tt)*]
            ),* $(,)?
        }
    ) => {
        /// Typed parameter values of one group (or the defaults).
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        $vis struct $fname {
            $(
                $(#[doc = $doc])*
                pub $f: $crate::__param_ty!($unit),
            )*
        }

        impl $fname {
            /// Number of fields.
            pub const N: usize = [$(stringify!($f)),*].len();

            fn __from_raw(raw: &[i64]) -> Self {
                let mut it = raw.iter().copied();
                $fname {
                    $($f: $crate::params::FieldValue::from_raw(it.next().unwrap_or(0)),)*
                }
            }

            #[allow(unused_assignments)]
            fn __set_raw(&mut self, ix: u16, v: i64) {
                let mut i = 0u16;
                $(
                    if ix == i {
                        self.$f = $crate::params::FieldValue::from_raw(v);
                    }
                    i += 1;
                )*
            }

            /// Raw values in field order.
            pub fn raw(&self) -> [i64; $fname::N] {
                [$($crate::params::FieldValue::to_raw(self.$f)),*]
            }
        }

        $(#[$sm])*
        #[derive(Clone, Debug, PartialEq, Eq)]
        $vis struct $name {
            d: $fname,
            g: ::std::boxed::Box<[$fname]>,
        }

        impl $name {
            /// Field descriptors in schema order.
            pub const FIELDS: &'static [$crate::params::FieldDesc] = &[
                $(
                    {
                        #[allow(unused_mut)]
                        let mut __fd = $crate::__param_base!(
                            $f,
                            $unit [$($($en),*)?],
                            $crate::check::trim_doc(concat!("" $(, $doc)*))
                        );
                        $crate::__param_attrs!(__fd; $($attr)*);
                        __fd
                    },
                )*
            ];
            /// The schema (`id` = the strategy's `kind_id`).
            pub const SCHEMA: $crate::params::ParamSchemaDesc =
                $crate::params::ParamSchemaDesc { id: $sid, version: $ver, fields: $name::FIELDS };

            /// Defaults (no override applied).
            pub fn defaults(&self) -> &$fname {
                &self.d
            }
            /// Effective values of group `gix` (overrides applied); out of range → defaults. O(1).
            #[inline]
            pub fn at(&self, gix: u16) -> &$fname {
                self.g.get(usize::from(gix)).unwrap_or(&self.d)
            }
            /// Number of groups the table was decoded for.
            pub fn n_groups(&self) -> u16 {
                u16::try_from(self.g.len()).unwrap_or(u16::MAX)
            }
            /// Builds the typed table from raw values (`params::decode_body` output).
            pub fn from_raw(raw: &$crate::params::RawParams, n_groups: u16) -> $name {
                let d = $fname::__from_raw(&raw.defaults);
                let mut g = ::std::vec![d; usize::from(n_groups)];
                for o in &raw.overrides {
                    if let Some(x) = g.get_mut(usize::from(o.group_ix)) {
                        x.__set_raw(o.field_ix, o.value);
                    }
                }
                $name { d, g: g.into_boxed_slice() }
            }
        }

        const _: () = $crate::check::validate_schema(&$name::SCHEMA);

        impl $crate::params::ParamCheck for $name {}

        impl $crate::ParamSet for $name {
            fn decode(
                body: &[u8],
                schema_ver: u16,
                groups: &$crate::GroupTable,
            ) -> ::core::result::Result<Self, $crate::ParamError> {
                if groups.len() > groups.capacity() {
                    return ::core::result::Result::Err($crate::ParamError::TooManyGroups);
                }
                let raw = $crate::params::decode_body(&$name::SCHEMA, schema_ver, groups.len(), body)?;
                ::core::result::Result::Ok($name::from_raw(&raw, groups.len()))
            }
            fn validate(
                &self,
                groups: &$crate::GroupTable,
                rd: &$crate::RefView<'_>,
            ) -> ::core::result::Result<(), $crate::ParamError> {
                #[allow(unused_imports)]
                use $crate::params::ParamCheck as _;
                self.check(groups, rd)
            }
            fn structural_changed(&self, old: &Self, old_gix: u16, gix: u16) -> bool {
                let (a, b) = (old.at(old_gix).raw(), self.at(gix).raw());
                $name::FIELDS
                    .iter()
                    .zip(a.iter().zip(b.iter()))
                    .any(|(f, (x, y))| f.structural && x != y)
            }
        }
    };
}
