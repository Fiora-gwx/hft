//! Field and code-table descriptions (`FieldDesc`, `FieldTy`, `EnumDesc`) and the library side
//! of `cargo xtask gen-schema` (`catalog()`). Offsets come from `offset_of!` inside
//! `event_body!`, so the export cannot drift from the structs.

use hft_types::{
    AccountId, AssetId, CapSource, ConnId, GroupId, InstId, KillLevel, Notional, Px, Qty, Rate, RunMode,
    Side, SlotId, StrategyId, Tif, Tsc, VenueId,
};

use crate::cmd::CmdKind;
use crate::kinds::{EventKind, KindInfo, MEASURE_MASK};
use crate::system::{NOTE_NAMES, NoteName};

/// One field of a body, prefix or container struct.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub struct FieldDesc {
    pub name: &'static str,
    pub offset: u16,
    pub ty: FieldTy,
    /// Measurement field: masked by the verifying replay on Output records.
    pub measure: bool,
    pub doc: &'static str,
}

/// Wire type of a field.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum FieldTy {
    U8,
    U16,
    U32,
    U64,
    I8,
    I16,
    I32,
    I64,
    Px,
    Qty,
    Notional,
    Rate,
    Tsc,
    UtcNs,
    Id(IdKind),
    /// Code field; the table lists every legal value.
    Enum(&'static EnumDesc),
    /// Bit-set field; the table lists the named bits.
    Bits(&'static EnumDesc),
    Bytes(u16),
    Pad(u16),
    Nested(&'static [FieldDesc]),
}

/// Which ID newtype an `Id` field carries.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub enum IdKind {
    Inst,
    Asset,
    Venue,
    Account,
    Strategy,
    Group,
    Slot,
    Conn,
    Kind,
}

/// A code table (`Enum`) or a named bit set (`Bits`).
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub struct EnumDesc {
    pub name: &'static str,
    /// Field width in bytes.
    pub width: u8,
    pub items: &'static [EnumItem],
}

/// One code or bit.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub struct EnumItem {
    pub name: &'static str,
    pub value: u32,
}

impl EnumDesc {
    /// Value of the item called `name`.
    pub fn value(&self, name: &str) -> Option<u32> {
        self.items.iter().find(|i| i.name == name).map(|i| i.value)
    }

    /// Whether `v` is a listed value.
    pub const fn contains(&self, v: u32) -> bool {
        let mut rest = self.items;
        while let [it, tail @ ..] = rest {
            if it.value == v {
                return true;
            }
            rest = tail;
        }
        false
    }
}

impl IdKind {
    pub const fn width(self) -> u16 {
        match self {
            IdKind::Venue | IdKind::Account | IdKind::Strategy | IdKind::Conn => 1,
            IdKind::Asset | IdKind::Kind => 2,
            IdKind::Inst | IdKind::Group | IdKind::Slot => 4,
        }
    }
}

impl FieldTy {
    /// Width in bytes.
    pub const fn width(&self) -> u16 {
        match self {
            FieldTy::U8 | FieldTy::I8 => 1,
            FieldTy::U16 | FieldTy::I16 => 2,
            FieldTy::U32 | FieldTy::I32 => 4,
            FieldTy::U64
            | FieldTy::I64
            | FieldTy::Px
            | FieldTy::Qty
            | FieldTy::Notional
            | FieldTy::Rate
            | FieldTy::Tsc
            | FieldTy::UtcNs => 8,
            FieldTy::Id(k) => k.width(),
            FieldTy::Enum(d) | FieldTy::Bits(d) => d.width as u16,
            FieldTy::Bytes(n) | FieldTy::Pad(n) => *n,
            FieldTy::Nested(f) => fields_size(f),
        }
    }
}

/// End of the last field (= struct size for the padding-free structs of this crate).
pub const fn fields_size(f: &[FieldDesc]) -> u16 {
    match f.last() {
        Some(l) => l.offset + l.ty.width(),
        None => 0,
    }
}

/// Wire type of a Rust field type (implemented for every type that appears in a body).
#[doc(hidden)]
pub trait FieldType {
    const TY: FieldTy;
}

macro_rules! field_type {
    ($($t:ty => $ty:expr),* $(,)?) => { $(impl FieldType for $t { const TY: FieldTy = $ty; })* };
}
field_type!(
    u8 => FieldTy::U8, u16 => FieldTy::U16, u32 => FieldTy::U32, u64 => FieldTy::U64,
    i8 => FieldTy::I8, i16 => FieldTy::I16, i32 => FieldTy::I32, i64 => FieldTy::I64,
    Px => FieldTy::Px, Qty => FieldTy::Qty, Notional => FieldTy::Notional, Rate => FieldTy::Rate, Tsc => FieldTy::Tsc,
    InstId => FieldTy::Id(IdKind::Inst), AssetId => FieldTy::Id(IdKind::Asset), VenueId => FieldTy::Id(IdKind::Venue),
    AccountId => FieldTy::Id(IdKind::Account), StrategyId => FieldTy::Id(IdKind::Strategy),
    GroupId => FieldTy::Id(IdKind::Group), SlotId => FieldTy::Id(IdKind::Slot), ConnId => FieldTy::Id(IdKind::Conn),
    EventKind => FieldTy::Id(IdKind::Kind), CmdKind => FieldTy::Enum(&crate::codes::CMD_KIND),
    Side => FieldTy::Enum(&crate::codes::SIDE), Tif => FieldTy::Enum(&crate::codes::TIF),
    RunMode => FieldTy::Enum(&crate::codes::RUN_MODE), KillLevel => FieldTy::Enum(&crate::codes::KILL_LEVEL),
    CapSource => FieldTy::Enum(&crate::codes::CAP_SOURCE),
);

impl<T: FieldType, const N: usize> FieldType for [T; N] {
    const TY: FieldTy = FieldTy::Bytes((N * size_of::<T>()) as u16);
}

const fn starts_with(s: &str, p: &str) -> bool {
    let (mut s, mut p) = (s.as_bytes(), p.as_bytes());
    while let [c, pt @ ..] = p {
        let [d, st @ ..] = s else { return false };
        if *c != *d {
            return false;
        }
        p = pt;
        s = st;
    }
    true
}

/// Field name as written (`r#gen` → `gen`).
#[doc(hidden)]
pub const fn field_name(raw: &'static str) -> &'static str {
    if starts_with(raw, "r#") {
        raw.split_at(2).1
    } else {
        raw
    }
}

/// Final `FieldTy` of a field: `_pad*`/`_rsv*` → `Pad(size)`, `*_tsc`/`tsc` `U64` → `Tsc`.
#[doc(hidden)]
pub const fn field_ty(name: &str, ty: FieldTy, size: usize) -> FieldTy {
    if starts_with(name, "_pad") || starts_with(name, "_rsv") {
        return FieldTy::Pad(size as u16);
    }
    if let FieldTy::U64 = ty {
        let b = name.as_bytes();
        if (b.len() >= 4 && starts_with(name.split_at(b.len() - 4).1, "_tsc")) || starts_with(name, "tsc") {
            return FieldTy::Tsc;
        }
    }
    ty
}

/// A container struct exported by `gen-schema` that is not a kind (SPB1 headers, `PartHdr`…).
#[derive(Copy, Clone, Debug)]
pub struct StructDesc {
    pub name: &'static str,
    pub size: u16,
    pub fields: &'static [FieldDesc],
}

/// Everything `gen-schema` exports from this crate (test kinds `0x7Fxx` excluded).
#[derive(Copy, Clone, Debug)]
pub struct Catalog {
    pub kinds: &'static [KindInfo],
    /// Every assigned `CmdKind` `(number, name)`.
    pub cmd_kinds: &'static [EnumItem],
    pub notes: &'static [NoteName],
    /// SPB1 container and framing structs.
    pub structs: &'static [StructDesc],
    /// Every code table and bit set registered in this crate.
    pub enums: &'static [&'static EnumDesc],
    pub measure_mask: &'static [(EventKind, u16, u16)],
}

/// The export consumed by `cargo xtask gen-schema`.
pub fn catalog() -> Catalog {
    Catalog {
        kinds: crate::kinds::KINDS_BASE,
        cmd_kinds: crate::codes::CMD_KIND.items,
        notes: NOTE_NAMES,
        structs: STRUCTS,
        enums: crate::codes::ALL,
        measure_mask: MEASURE_MASK,
    }
}

macro_rules! structs {
    ($($t:ty => $n:literal),* $(,)?) => {
        &[$(StructDesc { name: $n, size: size_of::<$t>() as u16, fields: <$t>::FIELDS }),*]
    };
}

static STRUCTS: &[StructDesc] = structs!(
    crate::strategy::StrategyParamsHdr => "StrategyParamsHdr",
    crate::strategy::GroupSpec => "GroupSpec",
    crate::strategy::LegDef => "LegDef",
    crate::header::RecordHeader => "RecordHeader",
    crate::var::PartHdr => "PartHdr",
    crate::var::SectionHdr => "SectionHdr",
    crate::var::Level => "Level",
    crate::exec::MarginItem => "MarginItem",
    crate::exec::LeverageItem => "LeverageItem",
    crate::control::CapRecord => "CapRecord",
    crate::ring::CtlRecordHdr => "CtlRecordHdr",
);
