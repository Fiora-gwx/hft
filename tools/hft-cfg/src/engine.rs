//! `build --kind engine`: `params/<file>.toml` + the hft-core instance config → ECF1 blob
//! (hft-refdata `EcfBuilder`). Row layouts of the sections whose formats are drafted by their
//! owners: 3 `capacities` and 5 `sequencer` (hft-sequencer), 4 `rings` (apps/hft-core),
//! 6 `conn` (hft-venue). Sections 1 `oms`, 2 `ledger` and 7 `exec` are added when their owning
//! crates define the row formats (tools/README.md "已知限制").

use std::path::Path;

use hft_refdata::{EcfBuilder, EngineConfig, EngineSection};
use hft_types::HostClass;
use toml::{Table, Value};

use crate::util::*;

/// Scalar type of a row field (little-endian).
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
enum Ty {
    U16,
    U32,
    Bool,
}

/// One field of a fixed row: dotted TOML path under the section table, byte offset, type.
#[derive(Copy, Clone, Debug)]
struct Field {
    path: &'static str,
    off: usize,
    ty: Ty,
}

const fn f(path: &'static str, off: usize, ty: Ty) -> Field {
    Field { path, off, ty }
}

/// ECF1 section 3 `CapacitiesRow` (24 B, row version 1; layout drafted in hft-sequencer).
const CAPACITIES: (usize, &[Field]) = (
    24,
    &[
        f("max_insts", 0, Ty::U32),
        f("max_cells_total", 4, Ty::U32),
        f("max_timers", 8, Ty::U32),
        f("recent_cmd_ids", 12, Ty::U32),
        f("max_subs_per_inst", 16, Ty::U16),
        f("max_groups_per_instance", 18, Ty::U16),
        f("max_instances", 20, Ty::U16),
    ],
);

/// ECF1 section 5 `SeqParamsRow` (72 B, row version 1; hft-sequencer). `rings` is indexed by
/// `RingClass` (Ctl 1, Hk 2, Rep 3, Chain 4, Bbo 5, Advice 6; 0 and 7 reserved, zero).
const SEQUENCER: (usize, &[Field]) = (
    72,
    &[
        f("rings.ctl.per_ring", 4, Ty::U16),
        f("rings.ctl.total", 6, Ty::U16),
        f("rings.hk.per_ring", 8, Ty::U16),
        f("rings.hk.total", 10, Ty::U16),
        f("rings.rep.per_ring", 12, Ty::U16),
        f("rings.rep.total", 14, Ty::U16),
        f("rings.chain.per_ring", 16, Ty::U16),
        f("rings.chain.total", 18, Ty::U16),
        f("rings.bbo.per_ring", 20, Ty::U16),
        f("rings.bbo.total", 22, Ty::U16),
        f("rings.advice.per_ring", 24, Ty::U16),
        f("rings.advice.total", 26, Ty::U16),
        f("md_round_ns", 32, Ty::U32),
        f("ctl_bytes_per_round", 36, Ty::U32),
        f("keyframe_period_ms", 40, Ty::U32),
        f("keyframe_max_events", 44, Ty::U32),
        f(CORE_MEM, 48, Ty::U32),
        f(CORE_CKPT, 52, Ty::U32),
        f("sim_max_per_round", 56, Ty::U16),
        f("conflate_backlog", 58, Ty::U16),
        f("notify_per_continue", 60, Ty::U16),
        f("replay_per_round", 62, Ty::U16),
        f("auto_resume_after_clean_drain", 64, Ty::Bool),
        f(CORE_STANDBY, 65, Ty::Bool),
    ],
);

/// Section 5 fields whose value belongs to the hft-core instance config (`--core-config`).
const CORE_MEM: &str = "advisor_mem_budget_mib";
const CORE_CKPT: &str = "advisor_ckpt_max_mib";
const CORE_STANDBY: &str = "allow_missing_standby";
const CORE_KEYS: [(&str, &str, &str); 3] = [
    (CORE_MEM, "advisor", "mem_budget_mib"),
    (CORE_CKPT, "advisor", "checkpoint_max_mib"),
    (CORE_STANDBY, "startup", "allow_missing_standby"),
];

/// `RingCapKind` codes of ECF1 section 4 (apps/hft-core ring capacity table), code = index + 1.
const RING_KINDS: [&str; 17] = [
    "ctl",
    "hk",
    "feed_ctl",
    "bbo",
    "md",
    "md_bulk",
    "rep",
    "out",
    "rep_bulk",
    "acct_req",
    "acct",
    "rep_sim",
    "out_sim",
    "rep_sim_bulk",
    "advice",
    "snap_req",
    "snap",
];

const HOSTS: [(HostClass, &str); 6] = [
    (HostClass::Dev2, "dev2"),
    (HostClass::C4, "c4"),
    (HostClass::C8, "c8"),
    (HostClass::Metal, "metal"),
    (HostClass::C16, "c16"),
    (HostClass::C32, "c32"),
];

fn host_name(h: HostClass) -> &'static str {
    HOSTS.iter().find(|x| x.0 == h).map_or("?", |x| x.1)
}

fn put(row: &mut [u8], fl: Field, v: i64, ctx: &str) -> Res<()> {
    let bytes: Vec<u8> = match fl.ty {
        Ty::U16 => int_as::<u16>(v, fl.path, ctx)?.to_le_bytes().to_vec(),
        Ty::U32 => int_as::<u32>(v, fl.path, ctx)?.to_le_bytes().to_vec(),
        Ty::Bool => vec![u8::from(v != 0)],
    };
    if let Some(d) = row.get_mut(fl.off..fl.off + bytes.len()) {
        d.copy_from_slice(&bytes);
    }
    Ok(())
}

fn get(row: &[u8], fl: Field) -> Option<i64> {
    let b = |n: usize| row.get(fl.off..fl.off + n);
    Some(match fl.ty {
        Ty::U16 => i64::from(u16::from_le_bytes(b(2)?.try_into().ok()?)),
        Ty::U32 => i64::from(u32::from_le_bytes(b(4)?.try_into().ok()?)),
        Ty::Bool => i64::from(*b(1)?.first()?),
    })
}

/// Leaf paths of a table (`a.b.c`) with their values.
fn flatten<'a>(prefix: &str, t: &'a Table, out: &mut Vec<(String, &'a Value)>) {
    for (k, v) in t {
        let p = if prefix.is_empty() {
            k.clone()
        } else {
            format!("{prefix}.{k}")
        };
        match v {
            Value::Table(sub) => flatten(&p, sub, out),
            other => out.push((p, other)),
        }
    }
}

/// Encodes one fixed row from a section table; `extra` supplies values not read from the table.
fn encode_row(lay: (usize, &[Field]), t: &Table, extra: &[(&str, i64)], ctx: &str) -> Res<Vec<u8>> {
    let mut leaves = Vec::new();
    flatten("", t, &mut leaves);
    for (p, _) in &leaves {
        if !lay.1.iter().any(|f| f.path == p) {
            return rej(format!("{ctx}: unknown key `{p}`"));
        }
    }
    let mut row = vec![0u8; lay.0];
    for fl in lay.1 {
        let v = match extra.iter().find(|e| e.0 == fl.path) {
            Some(e) => e.1,
            None => match leaves.iter().find(|(p, _)| p == fl.path).map(|x| x.1) {
                None => return rej(format!("{ctx}: missing key `{}`", fl.path)),
                Some(Value::Integer(x)) if fl.ty != Ty::Bool => *x,
                Some(Value::Boolean(b)) if fl.ty == Ty::Bool => i64::from(*b),
                Some(_) => return rej(format!("{ctx}.{}: wrong value type", fl.path)),
            },
        };
        put(&mut row, *fl, v, ctx)?;
    }
    Ok(row)
}

/// The three section-5 values read from the hft-core instance config; a missing key → exit 2.
fn core_values(path: &Path) -> Res<[(&'static str, i64); 3]> {
    let t = parse_toml(&read_text(path)?, path)?;
    let mut out = [("", 0i64); 3];
    for (slot, (field, table, key)) in out.iter_mut().zip(CORE_KEYS) {
        let v = t.get(table).and_then(Value::as_table).and_then(|x| x.get(key));
        let x = match v {
            Some(Value::Integer(i)) if field != CORE_STANDBY => *i,
            Some(Value::Boolean(b)) if field == CORE_STANDBY => i64::from(*b),
            Some(_) => return input(format!("{}: `{table}.{key}` has the wrong type", path.display())),
            None => return input(format!("{}: missing `{table}.{key}`", path.display())),
        };
        *slot = (field, x);
    }
    Ok(out)
}

/// `params/engine.toml` + `--core-config` → ECF1 bytes.
pub fn build(params: &Table, core_config: &Path) -> Res<Vec<u8>> {
    // The three core-config values may not appear anywhere in the params file.
    let mut leaves = Vec::new();
    flatten("", params, &mut leaves);
    for (p, _) in &leaves {
        if let Some((field, table, key)) = CORE_KEYS.iter().find(|k| p.rsplit('.').next() == Some(k.0)) {
            return rej(format!(
                "engine params: `{p}` may only come from --core-config (`{table}.{key}` → ECF1 section 5 `{field}`)"
            ));
        }
    }
    let core = core_values(core_config)?;
    check_keys(
        params,
        &[
            "version",
            "host_class",
            "created_utc_ns",
            "capacities",
            "rings",
            "sequencer",
            "conn",
        ],
        "engine params",
    )?;
    let version: u64 = req(params, "version", "engine params")?;
    if version == 0 {
        return rej("engine params.version: must be ≥ 1");
    }
    let host_s = req_str(params, "host_class", "engine params")?;
    let host = HOSTS.iter().find(|h| h.1 == host_s).map_or_else(
        || rej(format!("engine params.host_class: unknown `{host_s}`")),
        |h| Ok(h.0),
    )?;
    let mut b = EcfBuilder::new(version, host);
    b.created_utc_ns(opt::<i64>(params, "created_utc_ns", "engine params")?);

    if let Some(t) = opt_table(params, "capacities", "engine params")? {
        let row = encode_row(CAPACITIES, t, &[], "capacities")?;
        b.section(EngineSection::Capacities, 1, CAPACITIES.0 as u32, 1, row);
    }
    if let Some(t) = opt_table(params, "rings", "engine params")? {
        let mut rows: Vec<(u8, u32)> = Vec::new();
        for (k, v) in t {
            let code = RING_KINDS.iter().position(|n| n == k).map_or_else(
                || rej(format!("rings: unknown ring kind `{k}`")),
                |i| Ok(i as u8 + 1),
            )?;
            let cap = v
                .as_integer()
                .map_or_else(|| rej(format!("rings.{k}: expected an integer")), Ok)?;
            let cap: u32 = int_as(cap, k, "rings")?;
            if cap == 0 {
                return rej(format!("rings.{k}: capacity 0"));
            }
            rows.push((code, cap));
        }
        rows.sort();
        let bytes = rows
            .iter()
            .flat_map(|(k, c)| [vec![*k, 0, 0, 0], c.to_le_bytes().to_vec()].concat())
            .collect();
        b.section(EngineSection::Rings, 1, 8, rows.len() as u32, bytes);
    }
    let seq = opt_table(params, "sequencer", "engine params")?
        .map_or_else(|| rej("engine params: missing table `sequencer`"), Ok)?;
    let row = encode_row(SEQUENCER, seq, &core, "sequencer")?;
    b.section(EngineSection::Sequencer, 1, SEQUENCER.0 as u32, 1, row);
    if let Some(t) = opt_table(params, "conn", "engine params")? {
        check_keys(t, &["rotate_margin_s"], "conn")?;
        let m = opt_table(t, "rotate_margin_s", "conn")?
            .map_or_else(|| rej("conn: missing table `rotate_margin_s`"), Ok)?;
        let mut rows: Vec<(u8, u32)> = Vec::new();
        for (k, v) in m {
            let venue = venue_or_rej(k, "conn.rotate_margin_s")?;
            let x = v.as_integer().map_or_else(
                || rej(format!("conn.rotate_margin_s.{k}: expected an integer")),
                Ok,
            )?;
            rows.push((venue.0, int_as(x, k, "conn.rotate_margin_s")?));
        }
        rows.sort();
        let bytes = rows
            .iter()
            .flat_map(|(v, m)| [vec![*v, 0, 0, 0], m.to_le_bytes().to_vec()].concat())
            .collect();
        b.section(EngineSection::Conn, 1, 8, rows.len() as u32, bytes);
    }
    let blob = b.finish();
    if let Err(r) = EngineConfig::load(&blob, None) {
        return rej(format!(
            "EngineConfig::load rejected the output: {}",
            crate::refdata::report_text(&r)
        ));
    }
    Ok(blob)
}

/// Readable leaves `(path, value)` of a loaded ECF1, in section order.
pub fn leaves(ec: &EngineConfig) -> Vec<(String, String)> {
    let mut out = vec![
        ("version".to_string(), ec.version().to_string()),
        ("host_class".to_string(), host_name(ec.host_class()).to_string()),
    ];
    for s in EngineSection::ALL {
        let Some(v) = ec.section(s) else { continue };
        let name = snake(&format!("{s:?}"));
        let fixed = match s {
            EngineSection::Capacities => Some(CAPACITIES),
            EngineSection::Sequencer => Some(SEQUENCER),
            _ => None,
        };
        match (s, fixed) {
            (_, Some(lay)) if v.row_ver == 1 && v.count == 1 && v.row_size as usize >= lay.0 => {
                for fl in lay.1 {
                    let x = get(v.bytes, *fl).unwrap_or(0);
                    let val = if fl.ty == Ty::Bool {
                        (x != 0).to_string()
                    } else {
                        x.to_string()
                    };
                    out.push((format!("{name}.{}", fl.path), val));
                }
            }
            (EngineSection::Rings | EngineSection::Conn, _) if v.row_ver == 1 && v.row_size == 8 => {
                for r in v.bytes.chunks_exact(8) {
                    let key = r.first().copied().unwrap_or(0);
                    let val = r
                        .get(4..8)
                        .and_then(|x| x.try_into().ok())
                        .map_or(0, u32::from_le_bytes);
                    let path = if s == EngineSection::Rings {
                        let k = RING_KINDS
                            .get(usize::from(key).wrapping_sub(1))
                            .map_or_else(|| format!("kind_{key}"), |n| (*n).to_string());
                        format!("rings.{k}")
                    } else {
                        format!("conn.rotate_margin_s.{}", venue_name(hft_types::VenueId(key)))
                    };
                    out.push((path, val.to_string()));
                }
            }
            _ => out.push((
                format!("{name}.raw"),
                format!("row_ver {} row_size {} count {}", v.row_ver, v.row_size, v.count),
            )),
        }
    }
    out
}

/// True for the capacity fields whose decrease is subject to the shrink rule.
pub fn is_capacity(path: &str) -> bool {
    path.starts_with("capacities.")
}
