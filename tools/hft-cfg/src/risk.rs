//! Offline RiskParams TOML mapping. RKP1 encoding and reference checks belong to hft-risk.
//! Integers are exact wire units; show emits a complete, rebuildable TOML document.

use std::collections::BTreeMap;

use hft_refdata::RefData;
use hft_risk::{
    AcctLimits, AcctSpec, BandSpec, GroupLimits, GroupLimitsRow, InstClassRow, MarginLimits, ParamsBuilder,
    PxBand, RiskParams, VenueLimits, VenueSpec,
};
use hft_types::{AccountId, GroupId, InstId, Notional, Qty, Rate, VenueId};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::diff::{AUTHORITY_NOTE, Report};
use crate::util::{Res, rej};

trait DocValue {
    type Raw;
    fn from_raw(value: Self::Raw) -> Self;
    fn into_raw(self) -> Self::Raw;
}

macro_rules! scalar {
    ($($wire:ident : $raw:ty),+ $(,)?) => {$(
        impl DocValue for $wire {
            type Raw = $raw;
            fn from_raw(value: $raw) -> Self { Self(value) }
            fn into_raw(self) -> $raw { self.0 }
        }
    )+};
}
scalar!(AccountId: u8, VenueId: u8, InstId: u32, GroupId: u32, Notional: i64, Qty: i64, Rate: i64);

macro_rules! integer {
    ($($ty:ty),+ $(,)?) => {$(
        impl DocValue for $ty {
            type Raw = Self;
            fn from_raw(value: Self) -> Self { value }
            fn into_raw(self) -> Self { self }
        }
    )+};
}
integer!(u8, u32, bool);

macro_rules! row {
    ($doc:ident => $wire:ident { $($field:ident : $ty:ty),+ $(,)? }) => {
        #[derive(Clone, Debug, Serialize, Deserialize)]
        #[serde(deny_unknown_fields)]
        struct $doc { $($field: <$ty as DocValue>::Raw,)+ }
        impl DocValue for $wire {
            type Raw = $doc;
            fn from_raw(value: $doc) -> Self {
                Self { $($field: <$ty as DocValue>::from_raw(value.$field),)+ }
            }
            fn into_raw(self) -> $doc {
                $doc { $($field: self.$field.into_raw(),)+ }
            }
        }
    };
}
row!(BandDoc => PxBand { max_px_dev: Rate, ioc_max_cross: Rate, rr_max_cross: Rate });
row!(InstDoc => InstClassRow { inst: InstId, class: u8 });
row!(GroupDoc => GroupLimits {
    max_group_notional: Notional, max_pos_qty: Qty, naked_gate: Rate, naked_gate_min: Notional
});
row!(GroupRowDoc => GroupLimitsRow { account: AccountId, group: GroupId, l: GroupLimits });
row!(MarginDoc => MarginLimits {
    liq_dist_block_open: Rate, liq_dist_reduce: Rate, liq_dist_force: Rate, liq_dist_hyst: Rate,
    margin_ratio_crosscheck: Rate, margin_ratio_hyst: Rate, deleverage_deadline_ms: u32,
    force_step: Rate, force_step_interval_ms: u32, margin_stale_ms: u32, mark_stale_ms: u32
});
row!(AccountDoc => AcctLimits {
    account: AccountId, max_order_notional: Notional, default_group: GroupLimits,
    max_account_notional: Notional, account_notional_from_equity: Rate, max_hedge_notional: Notional,
    max_reduce_notional: Notional, daily_loss_kill: Rate, daily_loss_abs: Notional,
    daily_loss_max_resumes: u8, recon_closeonly_notional: Notional, unknown_blocks_open: bool,
    funds_buffer: Rate, margin: MarginLimits
});
row!(VenueDoc => VenueLimits {
    venue: VenueId, account: AccountId, max_venue_notional: Notional, inst_bucket_per_s: u32,
    backoff_base_ms: u32, backoff_max_ms: u32, rr_reserve: Rate
});

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct RiskDoc {
    version: u64,
    generated_at_ns: i64,
    author: [u8; 16],
    rate_headroom: i64,
    conn_liveness_ms: u32,
    max_perp_leverage_x100: u32,
    l3_max_cross: i64,
    classes: Vec<BandDoc>,
    risk_class: Vec<InstDoc>,
    accounts: Vec<AccountDoc>,
    venues: Vec<VenueDoc>,
    groups: Vec<GroupRowDoc>,
}

impl RiskDoc {
    fn from_params(p: RiskParams) -> Self {
        Self {
            version: p.version,
            generated_at_ns: p.generated_at_ns,
            author: p.author,
            rate_headroom: p.rate_headroom.0,
            conn_liveness_ms: p.conn_liveness_ms,
            max_perp_leverage_x100: p.max_perp_leverage_x100,
            l3_max_cross: p.l3_max_cross.0,
            classes: p.classes.iter().copied().map(DocValue::into_raw).collect(),
            risk_class: p.risk_class.iter().copied().map(DocValue::into_raw).collect(),
            accounts: p.accounts.iter().copied().map(DocValue::into_raw).collect(),
            venues: p.venues.iter().copied().map(DocValue::into_raw).collect(),
            groups: p.groups.iter().copied().map(DocValue::into_raw).collect(),
        }
    }
}

fn inspect(blob: &[u8]) -> Res<RiskParams> {
    RiskParams::inspect_blob(blob).or_else(|e| rej(format!("invalid RKP1 blob: {e:?}")))
}

/// Full, strict TOML input. Explicit row lists prevent the builder's defaults from hiding typos
/// or missing accounts/venues. Runtime validation still checks execution bounds and capacities.
pub fn build(table: &toml::Table, rd: &RefData) -> Res<Vec<u8>> {
    let d: RiskDoc = table
        .clone()
        .try_into()
        .or_else(|e| rej(format!("risk TOML: {e}")))?;
    if d.classes.is_empty() || d.classes.len() > 8 {
        return rej("risk.classes: expected 1..=8 price bands");
    }
    let mut b = ParamsBuilder::new(rd);
    b.version = d.version;
    b.rate_headroom = Rate(d.rate_headroom);
    b.conn_liveness_ms = Some(d.conn_liveness_ms);
    b.max_perp_leverage_x100 = Some(d.max_perp_leverage_x100);
    b.l3_max_cross = Some(Rate(d.l3_max_cross));
    b.classes = d
        .classes
        .into_iter()
        .map(|x| BandSpec {
            max_px_dev: Rate(x.max_px_dev),
            ioc_max_cross: Rate(x.ioc_max_cross),
            rr_max_cross: Some(Rate(x.rr_max_cross)),
        })
        .collect();
    b.risk_class = d.risk_class.into_iter().map(InstClassRow::from_raw).collect();
    b.groups = d.groups.into_iter().map(GroupLimitsRow::from_raw).collect();
    b.accounts = d
        .accounts
        .into_iter()
        .map(|x| {
            let a = AcctLimits::from_raw(x);
            (
                a.account,
                AcctSpec {
                    max_order_notional: a.max_order_notional,
                    default_group: a.default_group,
                    max_account_notional: a.max_account_notional,
                    account_notional_from_equity: a.account_notional_from_equity,
                    max_hedge_notional: Some(a.max_hedge_notional),
                    max_reduce_notional: Some(a.max_reduce_notional),
                    daily_loss_kill: a.daily_loss_kill,
                    daily_loss_abs: a.daily_loss_abs,
                    daily_loss_max_resumes: Some(a.daily_loss_max_resumes),
                    recon_closeonly_notional: a.recon_closeonly_notional,
                    unknown_blocks_open: Some(a.unknown_blocks_open),
                    funds_buffer: Some(a.funds_buffer),
                    margin: Some(a.margin),
                },
            )
        })
        .collect();
    let venues: Vec<_> = d.venues.into_iter().map(VenueLimits::from_raw).collect();
    b.venues = venues
        .iter()
        .map(|v| {
            (
                v.venue,
                v.account,
                VenueSpec {
                    max_venue_notional: v.max_venue_notional,
                    inst_bucket_per_s: v.inst_bucket_per_s,
                    backoff_base_ms: v.backoff_base_ms,
                    backoff_max_ms: v.backoff_max_ms,
                    rr_reserve: Some(v.rr_reserve),
                },
            )
        })
        .collect();
    let mut p = b.build();
    // The builder fills missing venue rows and ignores unknown ones. Neither is acceptable
    // for a complete persisted configuration; compare its result with the explicit input.
    if p.venues.as_slice() != venues.as_slice() {
        return rej("risk.venues: missing, duplicate, out-of-order or unknown (account, venue)");
    }
    p.generated_at_ns = d.generated_at_ns;
    p.author = d.author;
    let blob = p.encode_blob().or_else(|e| rej(format!("risk: {e:?}")))?;
    RiskParams::decode_blob(&blob, rd).or_else(|e| rej(format!("risk: {e:?}")))?;
    Ok(blob)
}

pub fn show(blob: &[u8]) -> Res<String> {
    toml::to_string_pretty(&RiskDoc::from_params(inspect(blob)?)).or_else(|e| rej(format!("risk TOML: {e}")))
}

fn flatten(path: &str, v: &Value, out: &mut BTreeMap<String, Value>) {
    match v {
        Value::Object(fields) => {
            for (key, value) in fields {
                flatten(&format!("{path}.{key}"), value, out);
            }
        }
        Value::Array(rows) => {
            for (ix, value) in rows.iter().enumerate() {
                let key = match path {
                    "risk.accounts" => format!("{}", value["account"]),
                    "risk.venues" => format!("{},{}", value["account"], value["venue"]),
                    "risk.groups" => format!("{},{}", value["account"], value["group"]),
                    "risk.risk_class" => format!("{}", value["inst"]),
                    _ => ix.to_string(),
                };
                flatten(&format!("{path}[{key}]"), value, out);
            }
        }
        _ => {
            out.insert(path.to_owned(), v.clone());
        }
    }
}

fn leaves(blob: &[u8]) -> Res<BTreeMap<String, Value>> {
    let doc = serde_json::to_value(RiskDoc::from_params(inspect(blob)?))
        .or_else(|e| rej(format!("risk JSON: {e}")))?;
    let mut out = BTreeMap::new();
    flatten("risk", &doc, &mut out);
    Ok(out)
}

fn direction(path: &str, old: &Value, new: &Value) -> &'static str {
    let field = path.rsplit('.').next().unwrap_or(path);
    if field == "unknown_blocks_open" {
        return match (old.as_bool(), new.as_bool()) {
            (Some(true), Some(false)) => "loosen",
            (Some(false), Some(true)) => "tighten",
            _ => "changed",
        };
    }
    let decreasing = matches!(
        field,
        "rate_headroom"
            | "rr_reserve"
            | "funds_buffer"
            | "backoff_base_ms"
            | "backoff_max_ms"
            | "liq_dist_block_open"
            | "liq_dist_reduce"
            | "liq_dist_force"
            | "liq_dist_hyst"
            | "margin_ratio_hyst"
            | "force_step"
    );
    let increasing = field.starts_with("max_")
        || matches!(
            field,
            "account_notional_from_equity"
                | "daily_loss_abs"
                | "daily_loss_kill"
                | "daily_loss_max_resumes"
                | "recon_closeonly_notional"
                | "naked_gate"
                | "naked_gate_min"
                | "inst_bucket_per_s"
                | "ioc_max_cross"
                | "rr_max_cross"
                | "l3_max_cross"
                | "conn_liveness_ms"
                | "margin_ratio_crosscheck"
                | "deleverage_deadline_ms"
                | "force_step_interval_ms"
                | "margin_stale_ms"
                | "mark_stale_ms"
        );
    match (old.as_i64(), new.as_i64()) {
        (Some(a), Some(b)) if increasing || decreasing => {
            if (b > a) == increasing {
                "loosen"
            } else {
                "tighten"
            }
        }
        _ => "changed",
    }
}

/// Field-level preview, keyed by account/group/venue identity rather than array position.
pub fn diff(old: &[u8], new: &[u8]) -> Res<Report> {
    let (old, new) = (leaves(old)?, leaves(new)?);
    let mut all = old.clone();
    all.extend(new.clone());
    let mut text = format!("risk diff\n{AUTHORITY_NOTE}\n");
    let mut changes = Vec::new();
    for path in all.keys() {
        let (a, b) = (
            old.get(path).unwrap_or(&Value::Null),
            new.get(path).unwrap_or(&Value::Null),
        );
        if a == b {
            continue;
        }
        let class = direction(path, a, b);
        let label = match class {
            "loosen" => "放宽",
            "tighten" => "收紧",
            _ => "变化",
        };
        text.push_str(&format!("{path}: {a} -> {b} ({label})\n"));
        changes.push(json!({"path": path, "old": a, "new": b, "class": class}));
    }
    Ok(Report {
        text,
        json: json!({"kind": "risk", "changes": changes, "note": AUTHORITY_NOTE}),
    })
}
