//! SPB1 body layout (README "SPB1 body 布局"): `ParamsBodyHdr` | `defaults: [i64; n_fields]` |
//! `overrides: [ParamOverride; n_overrides]`, little-endian, 8 B aligned.

use zerocopy::{FromBytes, Immutable, IntoBytes, KnownLayout};

use super::ParamError;

/// Body header (8 B).
#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq, FromBytes, IntoBytes, KnownLayout, Immutable)]
pub struct ParamsBodyHdr {
    /// Number of defaults; `decode_body` requires `= n_fields_at(schema_ver)`.
    pub n_fields: u16,
    /// Must be 0.
    pub _pad: u16,
    pub n_overrides: u32,
}
const _: () = assert!(size_of::<ParamsBodyHdr>() == 8);
const _: () = assert!(align_of::<ParamsBodyHdr>() == 4);
const _: () = assert!(core::mem::offset_of!(ParamsBodyHdr, n_fields) == 0);
const _: () = assert!(core::mem::offset_of!(ParamsBodyHdr, _pad) == 2);
const _: () = assert!(core::mem::offset_of!(ParamsBodyHdr, n_overrides) == 4);

/// One per-group override (16 B).
#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq, FromBytes, IntoBytes, KnownLayout, Immutable)]
pub struct ParamOverride {
    /// Index of the `GroupSpec` in the same blob (= gix).
    pub group_ix: u16,
    /// Schema field index; the field must be `per_group`.
    pub field_ix: u16,
    /// Must be 0.
    pub _pad: u32,
    /// Raw fixed-point value.
    pub value: i64,
}
const _: () = assert!(size_of::<ParamOverride>() == 16);
const _: () = assert!(align_of::<ParamOverride>() == 8);
const _: () = assert!(core::mem::offset_of!(ParamOverride, group_ix) == 0);
const _: () = assert!(core::mem::offset_of!(ParamOverride, field_ix) == 2);
const _: () = assert!(core::mem::offset_of!(ParamOverride, _pad) == 4);
const _: () = assert!(core::mem::offset_of!(ParamOverride, value) == 8);

impl ParamOverride {
    pub const fn new(group_ix: u16, field_ix: u16, value: i64) -> ParamOverride {
        ParamOverride {
            group_ix,
            field_ix,
            _pad: 0,
            value,
        }
    }

    #[inline]
    pub(crate) const fn key(&self) -> (u16, u16) {
        (self.group_ix, self.field_ix)
    }
}

/// Zero-copy view of a container-checked body.
#[derive(Clone, Copy, Debug)]
pub struct BodyView<'a> {
    pub hdr: &'a ParamsBodyHdr,
    pub defaults: &'a [i64],
    pub overrides: &'a [ParamOverride],
}

/// Container-level checks (no schema), in order: ① `body.len() = 8 + 8 × n_fields +
/// 16 × n_overrides` (`BodyLen`; a body shorter than 8 B too); the body must be 8-byte aligned
/// (`Misaligned`); ② padding is zero (`Padding`); ③ overrides strictly ascending by
/// `(group_ix, field_ix)` (`OverrideOrder{ix}`); ④ `group_ix < n_groups` (`GroupIxOutOfRange`).
/// Zero-copy, no allocation.
pub fn body_view(body: &[u8], n_groups: u16) -> Result<BodyView<'_>, ParamError> {
    if body.len() < size_of::<ParamsBodyHdr>() {
        return Err(ParamError::BodyLen);
    }
    if !(body.as_ptr() as usize).is_multiple_of(8) {
        return Err(ParamError::Misaligned);
    }
    let (hdr, rest) = ParamsBodyHdr::ref_from_prefix(body).map_err(|_| ParamError::BodyLen)?;
    let want = 8u64 + 8 * u64::from(hdr.n_fields) + 16 * u64::from(hdr.n_overrides);
    if want != body.len() as u64 {
        return Err(ParamError::BodyLen);
    }
    let (defaults, rest) = <[i64]>::ref_from_prefix_with_elems(rest, usize::from(hdr.n_fields))
        .map_err(|_| ParamError::BodyLen)?;
    let overrides = <[ParamOverride]>::ref_from_bytes(rest).map_err(|_| ParamError::BodyLen)?;
    if hdr._pad != 0 || overrides.iter().any(|o| o._pad != 0) {
        return Err(ParamError::Padding);
    }
    let mut prev: Option<(u16, u16)> = None;
    for (i, o) in overrides.iter().enumerate() {
        if prev.is_some_and(|p| p >= o.key()) {
            return Err(ParamError::OverrideOrder { ix: i as u32 });
        }
        prev = Some(o.key());
    }
    if let Some(o) = overrides.iter().find(|o| o.group_ix >= n_groups) {
        return Err(ParamError::GroupIxOutOfRange { group_ix: o.group_ix });
    }
    Ok(BodyView {
        hdr,
        defaults,
        overrides,
    })
}
