//! Schema-order encoding and decoding of parameter bodies and `StrategyCmd` payloads.

use zerocopy::IntoBytes;

use super::body::{ParamOverride, ParamsBodyHdr, body_view};
use super::{GROUP_IX_NONE, ParamError, RawParams, SchemaView, check_value};
use crate::desc::CmdSchema;

/// Number of fields with `since ≤ ver` (a prefix: `since` is non-decreasing);
/// `ver > s.version()` → `None`.
pub fn n_fields_at<S: SchemaView + ?Sized>(s: &S, ver: u16) -> Option<u16> {
    if ver > s.version() {
        return None;
    }
    let mut n = 0u16;
    while let Some(f) = s.field(n) {
        if f.since > ver {
            break;
        }
        n = n.saturating_add(1);
    }
    Some(n)
}

/// Index of the field named exactly `name`.
pub fn field_ix<S: SchemaView + ?Sized>(s: &S, name: &str) -> Option<u16> {
    (0..s.n_fields()).find(|&i| s.field(i).is_some_and(|f| f.name == name))
}

/// Checks one override against the schema (`n_groups` bound, per_group, value).
fn check_override<S: SchemaView + ?Sized>(
    s: &S,
    n_groups: u16,
    n_fields: u16,
    o: &ParamOverride,
) -> Result<(), ParamError> {
    if o.group_ix >= n_groups {
        return Err(ParamError::GroupIxOutOfRange { group_ix: o.group_ix });
    }
    let f = if o.field_ix < n_fields {
        s.field(o.field_ix)
    } else {
        None
    };
    let Some(f) = f.filter(|f| f.per_group) else {
        return Err(ParamError::NotPerGroup { field_ix: o.field_ix });
    };
    check_value(&f, o.value, o.group_ix)
}

/// Encodes all fields at the current `s.version()`. `raw.defaults.len() ≠ n_fields` → `BodyLen`;
/// defaults and overrides are range/enum/per_group checked; overrides are sorted by
/// `(group_ix, field_ix)`, identical entries merged, and the same key with different values →
/// `OverrideOrder{ix}` with `ix` = the input index of the later of the two.
pub fn encode_body<S: SchemaView + ?Sized>(
    s: &S,
    n_groups: u16,
    raw: &RawParams,
) -> Result<Vec<u8>, ParamError> {
    let n_fields = s.n_fields();
    if raw.defaults.len() != usize::from(n_fields) {
        return Err(ParamError::BodyLen);
    }
    for (i, &v) in raw.defaults.iter().enumerate() {
        let f = s.field(i as u16).ok_or(ParamError::BodyLen)?;
        check_value(&f, v, GROUP_IX_NONE)?;
    }
    for o in &raw.overrides {
        check_override(s, n_groups, n_fields, o)?;
    }
    let mut ord: Vec<(usize, ParamOverride)> = raw.overrides.iter().copied().enumerate().collect();
    ord.sort_by_key(|(_, o)| o.key());
    let mut out: Vec<ParamOverride> = Vec::with_capacity(ord.len());
    let mut last: Option<(usize, ParamOverride)> = None;
    for (i, o) in ord {
        match last {
            Some((li, lo)) if lo.key() == o.key() => {
                if lo.value != o.value {
                    return Err(ParamError::OverrideOrder { ix: li.max(i) as u32 });
                }
                last = Some((li.max(i), lo));
            }
            _ => {
                out.push(ParamOverride::new(o.group_ix, o.field_ix, o.value));
                last = Some((i, o));
            }
        }
    }
    let n_overrides = u32::try_from(out.len()).map_err(|_| ParamError::BodyLen)?;
    let hdr = ParamsBodyHdr {
        n_fields,
        _pad: 0,
        n_overrides,
    };
    let mut b = Vec::with_capacity(8 + 8 * raw.defaults.len() + 16 * out.len());
    b.extend_from_slice(hdr.as_bytes());
    b.extend_from_slice(raw.defaults.as_bytes());
    b.extend_from_slice(out.as_bytes());
    Ok(b)
}

/// Decodes a body written at `schema_ver`: container checks (`body_view`; an unaligned body is
/// copied first) → `schema_ver > s.version()` → `SchemaTooNew` → `n_fields ≠
/// n_fields_at(schema_ver)` → `BodyLen` → missing newer fields take their default → range,
/// per_group and enum checks (defaults first, then overrides in body order).
pub fn decode_body<S: SchemaView + ?Sized>(
    s: &S,
    schema_ver: u16,
    n_groups: u16,
    body: &[u8],
) -> Result<RawParams, ParamError> {
    if (body.as_ptr() as usize).is_multiple_of(8) {
        decode_aligned(s, schema_ver, n_groups, body)
    } else {
        let mut copy = vec![0u64; body.len().div_ceil(8)];
        let dst = copy
            .as_mut_bytes()
            .get_mut(..body.len())
            .ok_or(ParamError::BodyLen)?;
        dst.copy_from_slice(body);
        let aligned = copy.as_bytes().get(..body.len()).ok_or(ParamError::BodyLen)?;
        decode_aligned(s, schema_ver, n_groups, aligned)
    }
}

fn decode_aligned<S: SchemaView + ?Sized>(
    s: &S,
    schema_ver: u16,
    n_groups: u16,
    body: &[u8],
) -> Result<RawParams, ParamError> {
    let v = body_view(body, n_groups)?;
    let Some(want) = n_fields_at(s, schema_ver) else {
        return Err(ParamError::SchemaTooNew {
            have: s.version(),
            got: schema_ver,
        });
    };
    if v.hdr.n_fields != want {
        return Err(ParamError::BodyLen);
    }
    let n_fields = s.n_fields();
    let mut defaults = Vec::with_capacity(usize::from(n_fields));
    for ix in 0..n_fields {
        let f = s.field(ix).ok_or(ParamError::BodyLen)?;
        let val = match v.defaults.get(usize::from(ix)) {
            Some(&x) => x,
            None => f.default,
        };
        check_value(&f, val, GROUP_IX_NONE)?;
        defaults.push(val);
    }
    for o in v.overrides {
        check_override(s, n_groups, want, o)?;
    }
    Ok(RawParams {
        defaults,
        overrides: v.overrides.to_vec(),
    })
}

/// `StrategyCmd` payload version.
const CMD_VER: u8 = 1;
/// Most values in one payload.
const CMD_MAX_VALS: usize = 31;

/// Encodes a `StrategyCmd` payload `{ver = 1, op, n, _pad: [u8; 5]} + vals: [i64; n]`.
/// `vals.len() ≠` the command's field count → `BodyLen`; values are enum/range checked
/// (`group_ix = 0xFFFF`).
pub fn encode_cmd<C: CmdSchema + ?Sized>(d: &C, vals: &[i64]) -> Result<Vec<u8>, ParamError> {
    if vals.len() != usize::from(d.n_fields()) || vals.len() > CMD_MAX_VALS {
        return Err(ParamError::BodyLen);
    }
    for (i, &v) in vals.iter().enumerate() {
        let f = d.field(i as u16).ok_or(ParamError::BodyLen)?;
        check_value(&f, v, GROUP_IX_NONE)?;
    }
    let mut b = Vec::with_capacity(8 + 8 * vals.len());
    b.extend_from_slice(&[CMD_VER, d.op(), vals.len() as u8, 0, 0, 0, 0, 0]);
    b.extend_from_slice(vals.as_bytes());
    Ok(b)
}

/// Decodes and checks a `StrategyCmd` payload against `cmds`, in order: length < 8, `ver ≠ 1`,
/// `n > 31` or length ≠ 8 + 8n → `BodyLen`; non-zero padding → `Padding`; `op` not in `cmds`
/// (any op when `cmds` is empty) → `UnknownOp`; `n ≠` that command's field count → `BodyLen`;
/// values → `OutOfRange`/`BadEnum`.
pub fn decode_cmd<C: CmdSchema>(cmds: &[C], payload: &[u8]) -> Result<(u8, Vec<i64>), ParamError> {
    let Some((head, rest)) = payload.split_first_chunk::<8>() else {
        return Err(ParamError::BodyLen);
    };
    let [ver, op, n, pad @ ..] = *head;
    let n = usize::from(n);
    if ver != CMD_VER || n > CMD_MAX_VALS || rest.len() != 8 * n {
        return Err(ParamError::BodyLen);
    }
    if pad != [0; 5] {
        return Err(ParamError::Padding);
    }
    let Some(d) = cmds.iter().find(|c| c.op() == op) else {
        return Err(ParamError::UnknownOp { op });
    };
    if n != usize::from(d.n_fields()) {
        return Err(ParamError::BodyLen);
    }
    let mut vals = Vec::with_capacity(n);
    for (i, c) in rest.chunks_exact(8).enumerate() {
        let mut w = [0u8; 8];
        w.copy_from_slice(c);
        let v = i64::from_le_bytes(w);
        let f = d.field(i as u16).ok_or(ParamError::BodyLen)?;
        check_value(&f, v, GROUP_IX_NONE)?;
        vals.push(v);
    }
    Ok((op, vals))
}
