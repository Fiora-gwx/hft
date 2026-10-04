//! Human text ↔ raw value per unit (`parse_value`, `format_value`). Decimal parsing goes through
//! hft-types `parse_exact` (byte-wise, never via floating point).
//!
//! | unit | accepted text | `format_value` |
//! |---|---|---|
//! | Rate | `"0.5%"` (× 1e7), `"50bp"` (× 1e5), `"0.005"` (× 1e9) | `"<x>%"` |
//! | Notional | decimal USDT (× 1e6) | decimal USDT |
//! | Qty / Px | decimal at the asset exponent (`exp` required) | decimal; without `exp` the raw integer |
//! | DurationMs | `"1500"`/`"1500ms"`, `"30s"`, `"5m"`, `"2h"`, `"1d"` | largest of `d`/`h`/`m`/`s` that divides exactly, else ms |
//! | Count | integer | integer |
//! | Bool | `"true"`/`"false"`/`"1"`/`"0"` | `"true"`/`"false"` |
//! | Enum | a name (exact) | the name |

use hft_types::{AssetExp, parse_exact, write_fixed};

use super::{FieldRef, ParamError, UnitKind};

const RATE_EXP: i8 = -9;
const NOTIONAL_EXP: i8 = -6;

fn dec(text: &str, exp: i8, ix: u16) -> Result<i64, ParamError> {
    parse_exact(text.as_bytes(), exp).map_err(|_| ParamError::BadText { field_ix: ix })
}

/// Text → raw value of field `f` (no range check; enum names are checked: unknown → `BadEnum`).
/// `Qty`/`Px` need the group asset's exponents (`None` → `BadText`).
pub fn parse_value(f: FieldRef<'_>, text: &str, exp: Option<AssetExp>) -> Result<i64, ParamError> {
    let ix = f.ix;
    let bad = ParamError::BadText { field_ix: ix };
    let t = text.trim();
    match f.unit {
        UnitKind::Rate => {
            if let Some(n) = t.strip_suffix('%') {
                dec(n, RATE_EXP + 2, ix)
            } else if let Some(n) = t.strip_suffix("bp") {
                dec(n, RATE_EXP + 4, ix)
            } else {
                dec(t, RATE_EXP, ix)
            }
        }
        UnitKind::Notional => dec(t, NOTIONAL_EXP, ix),
        UnitKind::Qty => dec(t, exp.ok_or(bad)?.qty_exp, ix),
        UnitKind::Px => dec(t, exp.ok_or(bad)?.px_exp, ix),
        UnitKind::DurationMs => {
            let (num, mul): (&str, i64) = if let Some(n) = t.strip_suffix("ms") {
                (n, 0)
            } else if let Some(n) = t.strip_suffix('s') {
                (n, 1)
            } else if let Some(n) = t.strip_suffix('m') {
                (n, 60)
            } else if let Some(n) = t.strip_suffix('h') {
                (n, 3_600)
            } else if let Some(n) = t.strip_suffix('d') {
                (n, 86_400)
            } else {
                (t, 0)
            };
            if mul == 0 {
                dec(num, 0, ix)
            } else {
                // Thousandths of the unit × seconds per unit = milliseconds.
                dec(num, -3, ix)?.checked_mul(mul).ok_or(bad)
            }
        }
        UnitKind::Count => dec(t, 0, ix),
        UnitKind::Bool => match t {
            "true" | "1" => Ok(1),
            "false" | "0" => Ok(0),
            _ => Err(bad),
        },
        UnitKind::Enum => f
            .enum_names
            .position(t)
            .map(|i| i as i64)
            .ok_or(ParamError::BadEnum { field_ix: ix }),
    }
}

fn fixed(raw: i64, exp: i8) -> String {
    let mut buf = [0u8; 40];
    let n = write_fixed(raw, exp, &mut buf);
    match buf.get(..n).and_then(|b| core::str::from_utf8(b).ok()) {
        Some(s) if n > 0 => s.to_string(),
        _ => raw.to_string(),
    }
}

/// Raw value → display text of field `f` (also the schema JSON `display` values);
/// `parse_value(f, &format_value(f, x, exp), exp) == Ok(x)` for every in-range `x`.
pub fn format_value(f: FieldRef<'_>, raw: i64, exp: Option<AssetExp>) -> String {
    match f.unit {
        UnitKind::Rate => {
            let mut s = fixed(raw, RATE_EXP + 2);
            s.push('%');
            s
        }
        UnitKind::Notional => fixed(raw, NOTIONAL_EXP),
        UnitKind::Qty => match exp {
            Some(e) => fixed(raw, e.qty_exp),
            None => raw.to_string(),
        },
        UnitKind::Px => match exp {
            Some(e) => fixed(raw, e.px_exp),
            None => raw.to_string(),
        },
        UnitKind::DurationMs => {
            const UNITS: [(i64, &str); 4] =
                [(86_400_000, "d"), (3_600_000, "h"), (60_000, "m"), (1_000, "s")];
            if raw != 0 {
                for (ms, suffix) in UNITS {
                    if raw % ms == 0 {
                        return format!("{}{suffix}", raw / ms);
                    }
                }
            }
            raw.to_string()
        }
        UnitKind::Count => raw.to_string(),
        UnitKind::Bool => match raw {
            0 => "false".to_string(),
            1 => "true".to_string(),
            _ => raw.to_string(),
        },
        UnitKind::Enum => match usize::try_from(raw).ok().and_then(|i| f.enum_names.get(i)) {
            Some(n) => n.to_string(),
            None => raw.to_string(),
        },
    }
}
