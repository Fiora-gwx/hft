//! Model checkpoint raw-sample section layout description (README "Model 检查点").
//! The checkpoint key and the layout JSON helpers arrive with the M2 Model API; the canonical
//! schema JSON already exports each section's `tag` and `name` (`sections`).

/// Layout of a Model's raw-sample section (`Model::LAYOUT`).
#[derive(Clone, Copy, Debug)]
pub struct ModelLayoutDesc {
    pub sections: &'static [SectionDesc],
}

impl ModelLayoutDesc {
    /// No sections (models without checkpoint state, `NoModel`).
    pub const EMPTY: ModelLayoutDesc = ModelLayoutDesc { sections: &[] };
}

/// One section of the raw-sample section.
#[derive(Clone, Copy, Debug)]
pub struct SectionDesc {
    /// `SnapshotWriter::section(tag)`; unique within the Model, append-only, never reused.
    pub tag: u16,
    pub name: &'static str,
    pub scope: SectionScope,
    /// In write order.
    pub elems: &'static [ElemDesc],
}

/// `Instance`: once per instance (head of the `from_gix = 0` chunk); `Group`: once per group,
/// starting with the `GroupId`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SectionScope {
    Instance,
    Group,
}

/// One element of a section.
#[derive(Clone, Copy, Debug)]
pub struct ElemDesc {
    pub name: &'static str,
    pub ty: ElemTy,
    pub len: LenRule,
}

/// Element type (little-endian; `F64Bits` = IEEE bits as `u64`).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ElemTy {
    U8,
    U16,
    U32,
    U64,
    I32,
    I64,
    I128,
    F64Bits,
    Bytes,
}

/// Element count rule.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LenRule {
    One,
    Fixed(u32),
    /// Preceded by a `u32` element count.
    Prefixed,
}
