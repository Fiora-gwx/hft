//! The test strategy schema of tools/TESTS.md TOOL-119 (`probe-taker`, kind_id 65280, schema
//! version 2) and its export through the SDK `params` module (`schema_json`, `schema_digest`),
//! the same way `hft-core schemas --out` writes it. The committed copy lives in
//! `tests/fixtures/schemas/`; `schema_dir` re-exports and checks that both agree.
#![allow(dead_code)]

use std::path::{Path, PathBuf};

use hft_strategy_sdk::params::{schema_digest, schema_json};
use hft_strategy_sdk::{FieldDesc, MinorChange, ModelLayoutDesc, ParamSchemaDesc, Relax, StrategyDesc, Unit};

const fn field(name: &'static str, unit: Unit, min: i64, max: i64, default: i64) -> FieldDesc {
    FieldDesc {
        name,
        unit,
        min,
        max,
        default,
        per_group: false,
        structural: false,
        manual_only: false,
        relax: Relax::None,
        minor_change: MinorChange::None,
        since: 1,
        doc: "",
    }
}

pub const FIELDS: [FieldDesc; 7] = [
    FieldDesc {
        per_group: true,
        relax: Relax::Down,
        minor_change: MinorChange::RelPct(10),
        doc: "spread threshold",
        ..field("spread_th", Unit::Rate, 0, 10_000_000, 500_000)
    },
    FieldDesc {
        per_group: true,
        relax: Relax::Up,
        doc: "per-group quantity cap",
        ..field("max_qty", Unit::Qty, 0, 1_000_000_000_000_000, 0)
    },
    FieldDesc {
        relax: Relax::Down,
        minor_change: MinorChange::Abs(100),
        doc: "cooldown",
        ..field("cooldown_ms", Unit::DurationMs, 0, 60_000, 1_000)
    },
    FieldDesc {
        structural: true,
        per_group: true,
        doc: "bins",
        ..field("bins", Unit::Count, 1, 64, 16)
    },
    FieldDesc {
        minor_change: MinorChange::Any,
        doc: "whitelist size",
        ..field("whitelist", Unit::Count, 0, 1_000, 3)
    },
    FieldDesc {
        doc: "limit x",
        ..field("limit_x", Unit::Count, 0, 100, 7)
    },
    FieldDesc {
        since: 2,
        doc: "mode",
        ..field("mode", Unit::Enum(&["a", "b"]), 0, 1, 0)
    },
];

pub const SCHEMA: ParamSchemaDesc = ParamSchemaDesc {
    id: 65280,
    version: 2,
    fields: &FIELDS,
};

pub const DESC: StrategyDesc = StrategyDesc {
    name: "probe-taker",
    kind_id: 65280,
    version: 1,
    non_prod_only: false,
    schema: &SCHEMA,
    notes: &[],
    report: &[],
    cmds: &[],
};

pub const FILE: &str = "65280-probe-taker.schema.json";

/// `(index.json, schema file)` contents as exported by the SDK.
pub fn export() -> (String, String) {
    let file = schema_json(&DESC, &ModelLayoutDesc::EMPTY, SCHEMA.version);
    let index = format!(
        "[{{\"kind_id\":{},\"name\":\"{}\",\"file\":\"{FILE}\",\"schema_id\":{},\"schema_version\":{},\"schema_digest\":\"{:016x}\"}}]",
        DESC.kind_id,
        DESC.name,
        SCHEMA.id,
        SCHEMA.version,
        schema_digest(&DESC, SCHEMA.version)
    );
    (index, file)
}

pub fn committed() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/schemas")
}

/// A writable copy of the schema directory in `dir/schemas` (the committed fixture, checked
/// against a fresh SDK export).
pub fn schema_dir(dir: &Path) -> PathBuf {
    let (index, file) = export();
    let c = committed();
    assert_eq!(
        std::fs::read_to_string(c.join("index.json")).unwrap().trim_end(),
        index,
        "committed index.json is the SDK export"
    );
    assert_eq!(
        std::fs::read_to_string(c.join(FILE)).unwrap().trim_end(),
        file,
        "committed schema file is the SDK export"
    );
    let d = dir.join("schemas");
    std::fs::create_dir_all(&d).unwrap();
    std::fs::write(d.join("index.json"), index).unwrap();
    std::fs::write(d.join(FILE), file).unwrap();
    d
}
