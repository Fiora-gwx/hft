//! The `TP` test schema, the SDK-092 auxiliary schema, `GT3` group ids and the `EchoProbe`
//! descriptors. M1: `TP` is a hand-written `ParamSchemaDesc` (the same schema `define_params!`
//! generates from M2 on).

use hft_strategy_sdk::params::RawParams;
use hft_strategy_sdk::*;
use hft_types::{AssetExp, GroupId};

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

pub const TP_FIELDS: [FieldDesc; 6] = [
    FieldDesc {
        per_group: true,
        relax: Relax::Down,
        minor_change: MinorChange::RelPct(10),
        doc: "开仓阈值",
        ..field("open_th", Unit::Rate, 0, 100_000_000, 5_000_000)
    },
    FieldDesc {
        relax: Relax::Down,
        minor_change: MinorChange::Abs(1_000),
        doc: "同一 group 两次开仓的最小间隔",
        ..field("cooldown_ms", Unit::DurationMs, 0, 600_000, 20_000)
    },
    FieldDesc {
        per_group: true,
        structural: true,
        doc: "分位箱宽",
        ..field("pct_bin", Unit::Rate, 10_000, 1_000_000, 100_000)
    },
    FieldDesc {
        per_group: true,
        relax: Relax::Up,
        minor_change: MinorChange::Any,
        doc: "单 group 数量上限",
        ..field("max_qty", Unit::Qty, 0, 1_000_000_000_000, 0)
    },
    FieldDesc {
        manual_only: true,
        minor_change: MinorChange::Any,
        doc: "总开关",
        ..field("enabled", Unit::Bool, 0, 1, 1)
    },
    FieldDesc {
        manual_only: true,
        since: 2,
        doc: "模式",
        ..field("mode", Unit::Enum(&["a", "b", "c"]), 0, 2, 0)
    },
];

/// `TP` v2.
pub static TP: ParamSchemaDesc = ParamSchemaDesc {
    id: 0xFF01,
    version: 2,
    fields: &TP_FIELDS,
};

/// `TP` as it was at v1 (the first 5 fields).
pub static TP_V1: ParamSchemaDesc = ParamSchemaDesc {
    id: 0xFF01,
    version: 1,
    fields: TP_FIELDS.split_at(5).0,
};

/// SDK-092 auxiliary schema: a Count field `n` and a Notional field `cap`.
pub static AUX: ParamSchemaDesc = ParamSchemaDesc {
    id: 0xFF0A,
    version: 1,
    fields: &[
        field("n", Unit::Count, 0, 10_000, 0),
        field("cap", Unit::Notional, 0, 1_000_000_000_000, 0),
    ],
};

/// BTC (asset 1).
pub const BTC: AssetExp = AssetExp {
    px_exp: -2,
    qty_exp: -8,
};

pub const G1: GroupId = GroupId(0x0300_0001);
pub const G2: GroupId = GroupId(0x0300_0002);
pub const G3: GroupId = GroupId(0x0300_0003);

/// `TP` defaults in field order.
pub fn tp_defaults() -> Vec<i64> {
    TP_FIELDS.iter().map(|f| f.default).collect()
}

pub fn raw(defaults: Vec<i64>, overrides: &[(u16, u16, i64)]) -> RawParams {
    RawParams {
        defaults,
        overrides: overrides
            .iter()
            .map(|&(g, f, v)| params::ParamOverride::new(g, f, v))
            .collect(),
    }
}

pub const ECHO_NOTES: [NoteDesc; 3] = [
    NoteDesc {
        code: 1,
        name: "EchoFire",
        level: NoteLevel::Info,
        v: &["mid_px", "open_th"],
        doc: "发出一张 Open IOC",
    },
    NoteDesc {
        code: 2,
        name: "EchoOverflow",
        level: NoteLevel::P2,
        v: &[],
        doc: "定点运算溢出,放弃本事件该 group 的动作",
    },
    NoteDesc {
        code: 3,
        name: "EchoNoFeed",
        level: NoteLevel::P2,
        v: &["inst"],
        doc: "参考 inst 无 feed,未订阅",
    },
];

pub const ECHO_REPORT: [ReportFieldDesc; 1] = [ReportFieldDesc {
    name: "fires",
    unit: Unit::Count,
    agg: ReportAgg::Count,
    scope: ReportScope::Group,
    note: 1,
    v_ix: 0,
    doc: "开仓次数",
}];

pub const ECHO_CMDS: [CmdDesc; 3] = [
    CmdDesc {
        op: 1,
        name: "open",
        reduce_only: false,
        fields: &[FieldDesc {
            doc: "开仓名义",
            ..field("notional", Unit::Notional, 0, 10_000_000_000, 0)
        }],
        doc: "按名义开仓",
    },
    CmdDesc {
        op: 2,
        name: "close",
        reduce_only: true,
        fields: &[],
        doc: "平掉该 group",
    },
    CmdDesc {
        op: 3,
        name: "set_mode",
        reduce_only: false,
        fields: &[FieldDesc {
            doc: "模式",
            ..field("m", Unit::Enum(&["x", "y"]), 0, 1, 0)
        }],
        doc: "切换模式",
    },
];

/// The SDK test strategy `EchoProbe` (M1: descriptors only; the `Strategy` impl arrives in M2).
pub struct EchoProbe;

impl EchoProbe {
    pub const DESC: StrategyDesc = StrategyDesc {
        name: "echo_probe",
        kind_id: 0xFF01,
        version: 3,
        non_prod_only: false,
        schema: &TP,
        notes: &ECHO_NOTES,
        report: &ECHO_REPORT,
        cmds: &ECHO_CMDS,
    };
}

/// `ProbeModel::LAYOUT` (TESTS.md "共用夹具"): instance section tag 1, per-group section tag 2.
pub static PROBE_LAYOUT: ModelLayoutDesc = ModelLayoutDesc {
    sections: &[
        SectionDesc {
            tag: 1,
            name: "ticks",
            scope: SectionScope::Instance,
            elems: &[ElemDesc {
                name: "n_ticks",
                ty: ElemTy::U64,
                len: LenRule::One,
            }],
        },
        SectionDesc {
            tag: 2,
            name: "samples",
            scope: SectionScope::Group,
            elems: &[
                ElemDesc {
                    name: "utc_ns",
                    ty: ElemTy::I64,
                    len: LenRule::Prefixed,
                },
                ElemDesc {
                    name: "mid",
                    ty: ElemTy::I64,
                    len: LenRule::Prefixed,
                },
            ],
        },
    ],
};

/// 8-byte aligned copy of `b` (a body as it sits in an SPB1 blob).
pub fn aligned(b: &[u8]) -> Vec<u64> {
    let mut v = vec![0u64; b.len().div_ceil(8)];
    zerocopy::IntoBytes::as_mut_bytes(v.as_mut_slice())[..b.len()].copy_from_slice(b);
    v
}

/// The first `len` bytes of an aligned buffer.
pub fn bytes_of(v: &[u64], len: usize) -> &[u8] {
    &zerocopy::IntoBytes::as_bytes(v)[..len]
}
