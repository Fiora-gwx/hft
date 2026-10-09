//! Shared items of the trybuild cases (not a case itself).
#![allow(dead_code)]

pub use hft_strategy_sdk::prelude::*;

hft_strategy_sdk::define_params! {
    pub struct P1(schema_id = 0xFF01, version = 1, fields = P1Fields) {
        open_th: Rate [min = 0, max = 100_000_000, default = 5_000_000, per_group],
    }
}

hft_strategy_sdk::define_params! {
    pub struct P2(schema_id = 0xFF02, version = 1, fields = P2Fields) {
        open_th: Rate [min = 0, max = 100_000_000, default = 5_000_000, per_group],
    }
}

pub static S1: ParamSchemaDesc = P1::SCHEMA;
pub static S2: ParamSchemaDesc = P2::SCHEMA;

/// A strategy kind with no behavior: `ui_strategy!(Name, Params, DESC)`.
#[macro_export]
macro_rules! ui_strategy {
    ($name:ident, $p:ty, $desc:expr) => {
        pub struct $name;
        impl Strategy for $name {
            const DESC: StrategyDesc = $desc;
            type Params = $p;
            type Model = NoModel<$p>;
            fn new(_: StrategyInstance, _: &$p, _: &GroupTable, _: &RefView<'_>) -> Result<Self, ParamError> {
                Ok($name)
            }
            fn subscriptions(&self, _: &GroupTable, _: &mut SubsBuilder<'_>) {}
            fn on_event<V: Views>(&mut self, _: &EventRef<'_>, _: &V, _: &mut Ctx<'_>) {}
            fn on_params(&mut self, _: &$p, _: &GroupTable, _: &RefView<'_>) -> Result<(), ParamError> {
                Ok(())
            }
            fn snapshot(&self, _: &mut SnapshotWriter) {}
            fn restore(&mut self, _: &mut SnapshotReader<'_>) -> Result<(), RestoreError> {
                Ok(())
            }
        }
    };
}

/// A descriptor with the given kind, name and schema (no notes, report or commands).
pub const fn desc(kind_id: u16, name: &'static str, schema: &'static ParamSchemaDesc) -> StrategyDesc {
    StrategyDesc {
        name,
        kind_id,
        version: 1,
        non_prod_only: false,
        schema,
        notes: &[],
        report: &[],
        cmds: &[],
    }
}
