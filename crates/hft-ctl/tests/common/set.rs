//! Test strategy kinds of the G0 baseline (`probe-taker` 0xFF01, `probe-target` 0xFF02, and the
//! `non_prod_only` kind `probe-np` 0xFF03 of CTL-081), implemented by hand on the SDK trait. They
//! stand in for hft-testkit's `TestStrategies` until that crate carries them.
#![allow(
    dead_code,
    clippy::disallowed_types,
    clippy::disallowed_methods,
    clippy::disallowed_macros
)]

use hft_strategy_sdk::params::{RawParams, decode_body, encode_body, schema_digest, schema_json};
use hft_strategy_sdk::{
    AdviceOut, CmdDesc, Ctx, EventRef, FieldDesc, GroupTable, MinorChange, ModelLayoutDesc, ModelParamEffect,
    ModelViews, ParamError, ParamSchemaDesc, RefView, Relax, ResetPreview, RestoreError, SnapshotProgress,
    SnapshotReader, SnapshotWriter, StrategyDesc, StrategyInstance, StrategySet, SubsBuilder, Unit, Views,
};

const fn field(name: &'static str, max: i64, structural: bool) -> FieldDesc {
    FieldDesc {
        name,
        unit: Unit::Count,
        min: 0,
        max,
        default: 0,
        per_group: false,
        structural,
        manual_only: false,
        relax: Relax::None,
        minor_change: MinorChange::None,
        since: 1,
        doc: "test field",
    }
}

/// Field indices: win 0 (structural), thr 1, bad 2 (`validate` rejects ≠ 0), mem 3, ckpt 4.
pub const FIELDS: &[FieldDesc] = &[
    field("win", 1_000_000, true),
    field("thr", 1_000_000, false),
    field("bad", 1, false),
    field("mem", i64::MAX / 4, false),
    field("ckpt", i64::MAX / 4, false),
];
pub const F_WIN: usize = 0;
pub const F_THR: usize = 1;
pub const F_BAD: usize = 2;
pub const F_MEM: usize = 3;
pub const F_CKPT: usize = 4;

const QTY: &[FieldDesc] = &[FieldDesc {
    name: "qty",
    unit: Unit::Count,
    min: 1,
    max: 100,
    default: 1,
    per_group: false,
    structural: false,
    manual_only: false,
    relax: Relax::None,
    minor_change: MinorChange::None,
    since: 1,
    doc: "lots",
}];

pub const CMDS: &[CmdDesc] = &[
    CmdDesc {
        op: 1,
        name: "open",
        reduce_only: false,
        fields: QTY,
        doc: "open one group",
    },
    CmdDesc {
        op: 2,
        name: "close",
        reduce_only: true,
        fields: &[],
        doc: "close",
    },
];

pub static SCHEMA_TAKER: ParamSchemaDesc = ParamSchemaDesc {
    id: 0xFF01,
    version: 1,
    fields: FIELDS,
};
pub static SCHEMA_TARGET: ParamSchemaDesc = ParamSchemaDesc {
    id: 0xFF02,
    version: 1,
    fields: FIELDS,
};
pub static SCHEMA_NP: ParamSchemaDesc = ParamSchemaDesc {
    id: 0xFF03,
    version: 1,
    fields: FIELDS,
};

const fn desc(
    name: &'static str,
    kind_id: u16,
    non_prod_only: bool,
    schema: &'static ParamSchemaDesc,
) -> StrategyDesc {
    StrategyDesc {
        name,
        kind_id,
        version: 1,
        non_prod_only,
        schema,
        notes: &[],
        report: &[],
        cmds: CMDS,
    }
}

pub static KINDS_FULL: [StrategyDesc; 3] = [
    desc("probe-taker", 0xFF01, false, &SCHEMA_TAKER),
    desc("probe-target", 0xFF02, false, &SCHEMA_TARGET),
    desc("probe-np", 0xFF03, true, &SCHEMA_NP),
];
pub static KINDS_MIN: [StrategyDesc; 1] = [desc("probe-taker", 0xFF01, false, &SCHEMA_TAKER)];

/// Decoded parameters of a test kind.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct P {
    pub kind_id: u16,
    pub raw: RawParams,
}

impl P {
    pub fn get(&self, f: usize) -> i64 {
        self.raw.defaults.get(f).copied().unwrap_or(0)
    }
}

/// Body bytes of `values` (one default per field, no overrides).
pub fn body(values: [i64; 5], n_groups: u16) -> Vec<u8> {
    encode_body(
        &SCHEMA_TAKER,
        n_groups,
        &RawParams {
            defaults: values.to_vec(),
            overrides: Vec::new(),
        },
    )
    .unwrap()
}

/// Schema digest of a kind (as the SPB1 header carries it).
pub fn digest(kind_id: u16) -> u64 {
    let d = KINDS_FULL.iter().find(|d| d.kind_id == kind_id).unwrap();
    schema_digest(d, 1)
}

macro_rules! test_set {
    ($name:ident, $kinds:ident) => {
        /// A hand-written `StrategySet` over the test kinds.
        #[derive(Debug)]
        pub struct $name;

        impl StrategySet for $name {
            type Inst = ();
            type ModelInst = ();
            type Params = P;
            fn kinds() -> &'static [StrategyDesc] {
                &$kinds
            }
            fn decode_params(
                kind_id: u16,
                body: &[u8],
                schema_ver: u16,
                g: &GroupTable,
            ) -> Result<P, ParamError> {
                let d = $kinds
                    .iter()
                    .find(|d| d.kind_id == kind_id)
                    .ok_or(ParamError::SchemaMismatch {
                        want: 0,
                        got: u32::from(kind_id),
                    })?;
                if g.len() > g.capacity() {
                    return Err(ParamError::TooManyGroups);
                }
                let raw = decode_body(d.schema, schema_ver, g.len(), body)?;
                Ok(P { kind_id, raw })
            }
            fn validate_params(p: &P, _g: &GroupTable, _rd: &RefView<'_>) -> Result<(), ParamError> {
                if p.get(F_BAD) != 0 {
                    return Err(ParamError::Invalid {
                        field_ix: F_BAD as u16,
                        code: 1,
                    });
                }
                Ok(())
            }
            fn create(_: StrategyInstance, _: &P, _: &GroupTable, _: &RefView<'_>) -> Result<(), ParamError> {
                Ok(())
            }
            fn create_model(_: StrategyInstance, _: &P, _: &GroupTable, _: &RefView<'_>) {}
            fn on_event<V: Views>(_: &mut (), _: &EventRef<'_>, _: &V, _: &mut Ctx<'_>) {}
            fn on_params(_: &mut (), _: &P, _: &GroupTable, _: &RefView<'_>) -> Result<(), ParamError> {
                Ok(())
            }
            fn subscriptions(_: &(), _: &GroupTable, _: &mut SubsBuilder<'_>) {}
            fn snapshot(_: &(), _: &mut SnapshotWriter) {}
            fn restore(_: &mut (), _: &mut SnapshotReader<'_>) -> Result<(), RestoreError> {
                Ok(())
            }
            fn model_mem_bytes(p: &P, _n_groups: u16) -> u64 {
                p.get(F_MEM) as u64
            }
            fn model_snapshot_bytes(p: &P, _n_groups: u16) -> u64 {
                p.get(F_CKPT) as u64
            }
            fn model_layout(_: u16) -> &'static ModelLayoutDesc {
                &ModelLayoutDesc::EMPTY
            }
            fn model_layout_version(_: u16) -> u32 {
                0
            }
            fn model_on_event<V: ModelViews>(_: &mut (), _: &EventRef<'_>, _: &V, _: &mut AdviceOut<'_>) {}
            fn model_on_params(_: &mut (), _: &P, _: &GroupTable, _: &RefView<'_>) -> ModelParamEffect {
                ModelParamEffect::None
            }
            fn model_snapshot_chunk(_: &(), _: u16, _: &mut SnapshotWriter, _: u32) -> SnapshotProgress {
                SnapshotProgress::Done
            }
            fn model_restore(_: &mut (), _: &mut SnapshotReader<'_>) -> Result<(), RestoreError> {
                Ok(())
            }
            fn model_path_version(_: u16) -> u32 {
                0
            }
            fn model_snapshot_path(_: &(), _: &mut SnapshotWriter) {}
            fn model_restore_path(_: &mut (), _: &mut SnapshotReader<'_>) -> Result<(), RestoreError> {
                Ok(())
            }
            fn model_path_settle_ms(_: &P) -> u64 {
                0
            }
            /// Groups of the new table that are new or whose structural `win` changed.
            fn reset_preview(old: &P, old_g: &GroupTable, new: &P, new_g: &GroupTable) -> ResetPreview {
                let changed = old.get(F_WIN) != new.get(F_WIN);
                let n = new_g
                    .specs()
                    .iter()
                    .filter(|s| changed || old_g.gix_of(s.id).is_none())
                    .count() as u32;
                ResetPreview {
                    reset_groups: n,
                    no_open_ms: n.saturating_mul(1_000),
                }
            }
            fn schema_json(kind_id: u16) -> Option<String> {
                $kinds
                    .iter()
                    .find(|d| d.kind_id == kind_id)
                    .map(|d| schema_json(d, &ModelLayoutDesc::EMPTY, d.schema.version))
            }
            fn cmds(kind_id: u16) -> &'static [CmdDesc] {
                $kinds
                    .iter()
                    .find(|d| d.kind_id == kind_id)
                    .map_or(&[], |d| d.cmds)
            }
        }
    };
}

test_set!(CtlTestSet, KINDS_FULL);
test_set!(CtlTestSetMin, KINDS_MIN);
