//! `diff --old <a.blob> --new <b.blob> [--json]` and `show <blob>`: the blob kind comes from the
//! magic (`HFTRDB01` refdata, `HFTECF01` engine, otherwise SPB1 strategy params). Reports are
//! previews: the authoritative upgrade/downgrade decision is made by the sequencer.

use std::collections::BTreeMap;

use hft_refdata::{EngineConfig, RefData};
use hft_strategy_sdk::params::{RawParams, SchemaView, format_value, minor_change_check};
use hft_types::{GroupId, pair_lot};
use serde_json::{Value as J, json};

use crate::engine;
use crate::refdata::{report_text, warnings};
use crate::strategy::{Decoded, LoadedSchema, effective, inst_label};
use crate::util::*;

/// Blob kind by magic.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum BlobKind {
    Refdata,
    Engine,
    Strategy,
}

pub fn kind_of(b: &[u8]) -> BlobKind {
    match b.get(..8) {
        Some(b"HFTRDB01") => BlobKind::Refdata,
        Some(b"HFTECF01") => BlobKind::Engine,
        _ => BlobKind::Strategy,
    }
}

/// The shrink rule printed when an engine capacity decreases.
pub const SHRINK_NOTE: &str =
    "缩容只经 Drain + 重启,启动时当前占用超过新容量 hft-core 会拒绝启动(CapacityShrink)";
/// Printed under every strategy diff.
pub const AUTHORITY_NOTE: &str = "分类只是预览:升级/降级的权威判定在 sequencer";
/// Printed with refdata binding changes.
pub const REBIND_NOTE: &str = "账户有仓位时 hft-core 会拒绝启动(BindingChangedWithPosition)";

/// A rendered report: text lines and the same content as JSON.
#[derive(Clone, Debug)]
pub struct Report {
    pub text: String,
    pub json: J,
}

fn load_engine(b: &[u8], what: &str) -> Res<EngineConfig> {
    EngineConfig::load(b, None)
        .or_else(|r| rej(format!("{what}: not a valid ECF1 blob: {}", report_text(&r))))
}

pub fn load_refdata(b: &[u8], what: &str) -> Res<RefData> {
    RefData::load(b, None).or_else(|r| rej(format!("{what}: not a valid RDB1 blob: {}", report_text(&r))))
}

/// Pairs two ordered leaf lists by path: (path, old, new) for every path whose value differs.
fn pair(old: &[(String, String)], new: &[(String, String)]) -> Vec<(String, String, String)> {
    let mut paths: Vec<&String> = old.iter().map(|x| &x.0).collect();
    for (p, _) in new {
        if !paths.contains(&p) {
            paths.push(p);
        }
    }
    let get = |v: &[(String, String)], p: &str| {
        v.iter()
            .find(|x| x.0 == p)
            .map(|x| x.1.clone())
            .unwrap_or_default()
    };
    paths
        .into_iter()
        .filter_map(|p| {
            let (a, b) = (get(old, p), get(new, p));
            (a != b).then(|| (p.clone(), a, b))
        })
        .collect()
}

/// Engine (ECF1) diff: changed fields; a smaller capacity carries the shrink note.
pub fn engine_diff(old: &[u8], new: &[u8]) -> Res<Report> {
    let (a, b) = (load_engine(old, "--old")?, load_engine(new, "--new")?);
    let changes = pair(&engine::leaves(&a), &engine::leaves(&b));
    let mut text = String::from("engine diff\n");
    let mut items = Vec::new();
    let mut shrink = false;
    for (p, o, n) in &changes {
        let s =
            engine::is_capacity(p) && matches!((o.parse::<i64>(), n.parse::<i64>()), (Ok(x), Ok(y)) if y < x);
        shrink |= s;
        text.push_str(&format!("{p}: {o} → {n}{}\n", if s { " (shrink)" } else { "" }));
        items.push(json!({"path": p, "old": o, "new": n, "shrink": s}));
    }
    let mut notes = Vec::new();
    if shrink {
        text.push_str(&format!("note: {SHRINK_NOTE}\n"));
        notes.push(SHRINK_NOTE);
    }
    Ok(Report {
        text,
        json: json!({"kind": "engine", "changes": items, "notes": notes}),
    })
}

/// `BINANCE_USDM:SYM` (dump key) → `binance-usdm:SYM`.
fn inst_key(k: &str) -> String {
    match k.split_once(':') {
        Some((v, s)) => format!("{}:{s}", parse_venue(v).map_or_else(|| v.to_string(), venue_name)),
        None => k.to_string(),
    }
}

/// Refdata (RDB1) diff: added/removed insts, accounts and instances, binding changes,
/// capability changes field by field, every leaf change, and the new blob's warnings.
pub fn refdata_diff(old: &[u8], new: &[u8]) -> Res<Report> {
    let (a, b) = (load_refdata(old, "--old")?, load_refdata(new, "--new")?);
    Ok(refdata_report(&a, &b))
}

pub fn refdata_report(a: &RefData, b: &RefData) -> Report {
    let items = hft_refdata::diff(a, b);
    let mut added: BTreeMap<&str, Vec<J>> = BTreeMap::new();
    let mut removed: BTreeMap<&str, Vec<J>> = BTreeMap::new();
    let mut bindings = Vec::new();
    let mut caps = Vec::new();
    let mut text = String::from("refdata diff\n");
    for it in &items {
        let parts: Vec<&str> = it.path.splitn(3, '.').collect();
        match parts.as_slice() {
            [table @ ("insts" | "accounts" | "instances"), key, "id"] => {
                let entry = match *table {
                    "insts" => J::from(inst_key(key)),
                    _ => it
                        .old
                        .parse::<i64>()
                        .or_else(|_| it.new.parse::<i64>())
                        .map_or_else(|_| J::from(*key), J::from),
                };
                if it.old.is_empty() {
                    added.entry(table).or_default().push(entry);
                } else if it.new.is_empty() {
                    removed.entry(table).or_default().push(entry);
                }
            }
            ["accounts", label, "bound_instance"] if !it.old.is_empty() && !it.new.is_empty() => {
                let id = (1..=u8::MAX)
                    .map(hft_types::AccountId)
                    .take_while(|x| b.account(*x).id.0 != 0)
                    .find(|x| &*b.account(*x).label == *label)
                    .map_or(0, |x| x.0);
                bindings.push(json!({"account": id, "label": label, "from": it.old, "to": it.new}));
            }
            ["caps", venue, field] => {
                let v = parse_venue(venue).map_or_else(|| venue.to_string(), venue_name);
                let f = if *field == "margin_poll_ms" {
                    "margin_data.interval_ms"
                } else {
                    field
                };
                caps.push(json!([v, f, it.old, it.new]));
            }
            _ => {}
        }
        text.push_str(&format!("{}: {} → {}\n", it.path, it.old, it.new));
    }
    for (what, m) in [("added", &added), ("removed", &removed)] {
        for (t, v) in m {
            text.push_str(&format!("{what} {t}: {}\n", J::from(v.clone())));
        }
    }
    for x in &bindings {
        text.push_str(&format!(
            "binding: account {} {} → {} ({REBIND_NOTE})\n",
            x["account"], x["from"], x["to"]
        ));
    }
    let ws = warnings(b);
    for w in &ws {
        text.push_str(&format!(
            "warning: {} accounts {} uid {}\n",
            w.code,
            w.accounts.join(", "),
            w.uid
        ));
    }
    let changes: Vec<J> = items
        .iter()
        .map(|i| json!({"path": i.path, "old": i.old, "new": i.new}))
        .collect();
    let ws_json: Vec<J> = ws
        .iter()
        .map(|w| json!({"code": w.code, "accounts": w.accounts, "uid": w.uid}))
        .collect();
    let notes: Vec<&str> = if bindings.is_empty() {
        vec![]
    } else {
        vec![REBIND_NOTE]
    };
    Report {
        text,
        json: json!({
            "kind": "refdata",
            "added": added, "removed": removed,
            "binding_changes": bindings, "caps_changes": caps,
            "changes": changes, "warnings": ws_json, "notes": notes,
        }),
    }
}

/// Whether changing only `(field fx, group gix or defaults)` from `old` is a minor change
/// (evaluated by the SDK's `minor_change_check` on a copy that differs in that value only).
fn is_minor(
    view: &dyn SchemaView,
    base: &RawParams,
    ids: &[GroupId],
    fx: u16,
    gix: Option<usize>,
    new: i64,
) -> bool {
    let mut cur = base.clone();
    match gix {
        None => {
            if let Some(d) = cur.defaults.get_mut(usize::from(fx)) {
                *d = new;
            }
        }
        Some(g) => {
            cur.overrides
                .retain(|o| !(usize::from(o.group_ix) == g && o.field_ix == fx));
            cur.overrides
                .push(hft_strategy_sdk::params::ParamOverride::new(g as u16, fx, new));
        }
    }
    minor_change_check(view, (base, ids), (&cur, ids)).is_ok()
}

/// Strategy (SPB1) diff, field by field with the minor-change and structural classification;
/// with refdata, each group's two leg lots and `pair_lot` at a 1/1 hedge ratio.
pub fn strategy_diff(old: &Decoded, new: &Decoded, ls: &LoadedSchema, rd: Option<&RefData>) -> Report {
    let s = &ls.schema;
    let (Some(ro), Some(rn)) = (&old.raw, &new.raw) else {
        return Report {
            text: String::new(),
            json: J::Null,
        };
    };
    let old_ids: Vec<GroupId> = old.groups.iter().map(|g| g.id).collect();
    let mut text = String::from("strategy diff\n");
    let mut changes = Vec::new();
    for fx in 0..s.n_fields() {
        let Some(f) = s.field(fx) else { break };
        let exp_of = |gid: GroupId| {
            rd.and_then(|r| {
                new.groups
                    .iter()
                    .find(|g| g.id == gid)
                    .map(|g| r.asset(g.asset).exp)
            })
        };
        let fmt = |v: i64, gid: Option<GroupId>| format_value(f, v, gid.and_then(exp_of));
        let mut record = |scope: Option<GroupId>, o: i64, n: i64, minor: bool, affected: &[GroupId]| {
            let who = scope.map_or_else(|| "defaults".to_string(), |g| format!("group {:#010x}", g.0));
            let mut line = format!(
                "{} ({who}): {} → {}; {}",
                f.name,
                fmt(o, scope),
                fmt(n, scope),
                if minor { "minor" } else { "not minor" }
            );
            if f.structural {
                let ids: Vec<String> = affected.iter().map(|g| format!("{:#010x}", g.0)).collect();
                line.push_str(&format!("; structural, affected groups [{}]", ids.join(", ")));
            }
            text.push_str(&line);
            text.push('\n');
            changes.push(json!({
                "field": f.name,
                "group": scope.map(|g| format!("{:#010x}", g.0)),
                "old": o, "new": n, "minor": minor, "structural": f.structural,
                "affected_groups": affected.iter().map(|g| format!("{:#010x}", g.0)).collect::<Vec<_>>(),
            }));
        };
        // Groups present in both versions, ascending by id.
        let mut common: Vec<(GroupId, usize, usize)> = old
            .groups
            .iter()
            .enumerate()
            .filter_map(|(oi, g)| {
                new.groups
                    .iter()
                    .position(|x| x.id == g.id)
                    .map(|ni| (g.id, oi, ni))
            })
            .collect();
        common.sort_by_key(|x| x.0.0);
        let od = ro.defaults.get(usize::from(fx)).copied().unwrap_or(f.default);
        let nd = rn.defaults.get(usize::from(fx)).copied().unwrap_or(f.default);
        if od != nd {
            let minor = is_minor(s, ro, &old_ids, fx, None, nd);
            let aff: Vec<GroupId> = common
                .iter()
                .filter(|(_, oi, ni)| effective(ro, *oi, fx).is_none() && effective(rn, *ni, fx).is_none())
                .map(|x| x.0)
                .collect();
            record(None, od, nd, minor, &aff);
        }
        for (gid, oi, ni) in &common {
            let (eo, en) = (effective(ro, *oi, fx), effective(rn, *ni, fx));
            if (eo.is_some() || en.is_some()) && eo.unwrap_or(od) != en.unwrap_or(nd) {
                let (o, n) = (eo.unwrap_or(od), en.unwrap_or(nd));
                let minor = is_minor(s, ro, &old_ids, fx, Some(*oi), n);
                record(Some(*gid), o, n, minor, &[*gid]);
            }
        }
    }
    let mut groups = Vec::new();
    for g in &new.groups {
        let status = if old.groups.iter().any(|o| o.id == g.id) {
            "kept"
        } else {
            "added"
        };
        let mut entry = json!({"group": format!("{:#010x}", g.id.0), "status": status});
        if let Some(r) = rd
            && g.n_legs == 2
        {
            let (l0, l1) = (g.legs[0].inst, g.legs[1].inst);
            let exp = r.asset(g.asset).exp.qty_exp;
            let (a, b) = (r.inst(l0).lot, r.inst(l1).lot);
            let pl = pair_lot(a, b, 1, 1).map(|q| q.0);
            let pls = pl.map_or_else(|| "none".to_string(), |x| fmt_fixed(x, exp));
            text.push_str(&format!(
                "group {:#010x}: legs {} lot {}, {} lot {}; pair_lot {pls}\n",
                g.id.0,
                inst_label(r, l0),
                fmt_fixed(a, exp),
                inst_label(r, l1),
                fmt_fixed(b, exp)
            ));
            entry["lots"] = json!([fmt_fixed(a, exp), fmt_fixed(b, exp)]);
            entry["pair_lot"] = json!(pls);
        }
        groups.push(entry);
    }
    for o in &old.groups {
        if !new.groups.iter().any(|g| g.id == o.id) {
            text.push_str(&format!("group {:#010x}: removed\n", o.id.0));
            groups.push(json!({"group": format!("{:#010x}", o.id.0), "status": "removed"}));
        }
    }
    text.push_str(&format!("note: {AUTHORITY_NOTE}\n"));
    Report {
        text,
        json: json!({"kind": "strategy", "changes": changes, "groups": groups, "notes": [AUTHORITY_NOTE]}),
    }
}
