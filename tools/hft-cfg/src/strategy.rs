//! `build --kind strategy`: schema-generic TOML → SPB1 (`StrategyParamsHdr` + `GroupSpec[]` +
//! body). Field names, units, ranges and `per_group`/`since` come from the exported schema JSON
//! (`--schemas <dir>`: `index.json` + `<kind_id>-<name>.schema.json`); text → raw goes through the
//! SDK `params::parse_value`, the body through `params::encode_body` (the only encoder). Instance,
//! account, assets and legs resolve through the refdata blob. GroupId continuation and
//! `group_ctr_hwm` follow tools/README.md "hft-cfg" (`--against` = the blob in effect).

use std::path::Path;

use hft_event::{GroupSpec, LegDef, ParamSource, SpbView, StrategyParamsHdr, parse_spb1};
use hft_refdata::{GroupSpecView, LegView, RefData};
use hft_strategy_sdk::params::{
    FieldRef, OwnedSchema, ParamError, ParamOverride, RawParams, SchemaView, UnitKind, decode_body,
    encode_body, field_ix, format_value, n_fields_at, parse_value,
};
use hft_types::{AccountId, AssetExp, AssetId, GroupId, InstId, VenueId};
use toml::{Table, Value};
use zerocopy::IntoBytes;

use crate::util::*;

/// One `index.json` entry (format owned by strategies/registry).
#[derive(Clone, Eq, PartialEq, Debug)]
pub struct IndexEntry {
    pub kind_id: u16,
    pub name: String,
    pub file: String,
    pub schema_id: u32,
    pub schema_version: u16,
    pub schema_digest: u64,
}

/// A schema read from the directory and checked against its index entry.
#[derive(Clone, Debug)]
pub struct LoadedSchema {
    pub entry: IndexEntry,
    pub schema: OwnedSchema,
    pub non_prod_only: bool,
}

/// Reads `index.json`; the directory or the file missing → exit 2.
pub fn read_index(dir: &Path) -> Res<Vec<IndexEntry>> {
    if !dir.is_dir() {
        return input(format!("{}: schema directory not found", dir.display()));
    }
    let p = dir.join("index.json");
    let text = read_text(&p)?;
    let v: serde_json::Value =
        serde_json::from_str(&text).or_else(|e| input(format!("{}: invalid JSON: {e}", p.display())))?;
    let arr = v.as_array().map_or_else(
        || input(format!("{}: top level is not an array", p.display())),
        Ok,
    )?;
    arr.iter()
        .map(|e| {
            let s = |k: &str| e.get(k).and_then(serde_json::Value::as_str);
            let u = |k: &str| e.get(k).and_then(serde_json::Value::as_u64);
            let parsed = (|| {
                Some(IndexEntry {
                    kind_id: u16::try_from(u("kind_id")?).ok()?,
                    name: s("name")?.to_string(),
                    file: s("file")?.to_string(),
                    schema_id: u32::try_from(u("schema_id")?).ok()?,
                    schema_version: u16::try_from(u("schema_version")?).ok()?,
                    schema_digest: u64::from_str_radix(s("schema_digest")?, 16).ok()?,
                })
            })();
            parsed.map_or_else(|| input(format!("{}: malformed entry {e}", p.display())), Ok)
        })
        .collect()
}

/// The schema of `kind_id`: index entry → file → `OwnedSchema`, digest checked against the index.
pub fn load_schema(dir: &Path, kind_id: u16) -> Res<LoadedSchema> {
    let index = read_index(dir)?;
    let entry = index.into_iter().find(|e| e.kind_id == kind_id).map_or_else(
        || rej(format!("kind_id {kind_id}: not in the schema index.json")),
        Ok,
    )?;
    let p = dir.join(&entry.file);
    let text = read_text(&p)?;
    let schema = OwnedSchema::from_json(&text)
        .or_else(|_| rej(format!("{}: not a valid schema JSON", p.display())))?;
    if schema.kind_id() != kind_id
        || schema.id() != entry.schema_id
        || schema.version() != entry.schema_version
    {
        return rej(format!(
            "{}: kind_id/schema_id/schema_version differ from index.json",
            p.display()
        ));
    }
    let digest = schema.schema_digest(schema.version());
    if digest != entry.schema_digest {
        return rej(format!(
            "{}: schema digest {digest:016x} differs from index.json {:016x}",
            p.display(),
            entry.schema_digest
        ));
    }
    let non_prod_only = serde_json::from_str::<serde_json::Value>(&text)
        .ok()
        .and_then(|v| v.get("non_prod_only").and_then(serde_json::Value::as_bool))
        .unwrap_or(false);
    Ok(LoadedSchema {
        entry,
        schema,
        non_prod_only,
    })
}

/// The schema as it was at version `ver` (field prefix with `since ≤ ver`).
#[derive(Debug)]
pub struct AtVersion<'a> {
    pub s: &'a OwnedSchema,
    pub ver: u16,
    n: u16,
}

impl<'a> AtVersion<'a> {
    pub fn new(s: &'a OwnedSchema, ver: u16) -> Option<AtVersion<'a>> {
        let n = n_fields_at(s, ver)?;
        Some(AtVersion { s, ver, n })
    }
}

impl SchemaView for AtVersion<'_> {
    fn id(&self) -> u32 {
        self.s.id()
    }
    fn version(&self) -> u16 {
        self.ver
    }
    fn n_fields(&self) -> u16 {
        self.n
    }
    fn field(&self, ix: u16) -> Option<FieldRef<'_>> {
        if ix < self.n { self.s.field(ix) } else { None }
    }
}

/// Every asset of a refdata blob by name.
pub fn asset_by_name(rd: &RefData, name: &str) -> Option<(AssetId, AssetExp)> {
    (1..=u16::MAX)
        .map(AssetId)
        .take_while(|a| rd.asset(*a).id.0 != 0)
        .find(|a| &*rd.asset(*a).name == name)
        .map(|a| (a, rd.asset(a).exp))
}

/// `"<venue>:<symbol>"` → (inst, venue); unknown symbol → inst 0 (refdata reports it).
fn inst_by_name(rd: &RefData, s: &str, ctx: &str) -> Res<(InstId, VenueId)> {
    let Some((v, sym)) = s.split_once(':') else {
        return rej(format!("{ctx}: inst must be `<venue>:<symbol>`"));
    };
    let venue = venue_or_rej(v, ctx)?;
    Ok((rd.symbols(venue).get(sym.as_bytes()).unwrap_or(InstId(0)), venue))
}

/// `"<venue>:<symbol>"` of an inst.
pub fn inst_label(rd: &RefData, i: InstId) -> String {
    let h = rd.inst(i);
    format!("{}:{}", venue_name(h.venue), rd.inst_meta(i).symbol)
}

fn param_err(e: ParamError, s: &dyn SchemaView, groups: &[GroupSpec]) -> String {
    let fname = |ix: u16| {
        s.field(ix)
            .map_or_else(|| format!("#{ix}"), |f| f.name.to_string())
    };
    let gname = |gix: u16| {
        groups
            .get(usize::from(gix))
            .map_or_else(|| "defaults".to_string(), |g| format!("group {:#010x}", g.id.0))
    };
    match e {
        ParamError::OutOfRange { field_ix, group_ix } => {
            format!("OutOfRange: field `{}` ({})", fname(field_ix), gname(group_ix))
        }
        ParamError::BadEnum { field_ix } => format!("BadEnum: field `{}`", fname(field_ix)),
        ParamError::NotPerGroup { field_ix } => format!("NotPerGroup: field `{}`", fname(field_ix)),
        ParamError::BadText { field_ix } => format!("BadText: field `{}` (parse_exact)", fname(field_ix)),
        other => format!("{other:?}"),
    }
}

fn text_of(v: &Value, ctx: &str) -> Res<String> {
    match v {
        Value::String(s) => Ok(s.clone()),
        Value::Integer(i) => Ok(i.to_string()),
        Value::Boolean(b) => Ok(b.to_string()),
        _ => rej(format!("{ctx}: expected a string")),
    }
}

/// The blob currently in effect for (instance, account) (`--against`).
#[derive(Debug)]
pub struct Against<'a> {
    pub view: SpbView<'a>,
}

/// Output of a strategy build.
#[derive(Clone, Debug)]
pub struct StrategyOut {
    pub blob: Vec<u8>,
    pub hdr: StrategyParamsHdr,
}

/// Field index by name in the version-`ver` schema, or the matching rejection.
fn field_at(v: &AtVersion<'_>, name: &str, ctx: &str) -> Res<u16> {
    if let Some(ix) = field_ix(v, name) {
        return Ok(ix);
    }
    match field_ix(v.s, name).and_then(|ix| v.s.field(ix)) {
        Some(f) => rej(format!(
            "{ctx}: field `{name}` does not exist in schema version {} (since {})",
            v.ver, f.since
        )),
        None => rej(format!("{ctx}: unknown field `{name}`")),
    }
}

fn leg_def(inst: InstId, venue: VenueId) -> LegDef {
    LegDef {
        inst,
        venue,
        kind: LegDef::CEX,
        _pad: 0,
        route: 0,
        _pad2: 0,
    }
}

const NO_LEG: LegDef = LegDef {
    inst: InstId(0),
    venue: VenueId(0),
    kind: 0,
    _pad: 0,
    route: 0,
    _pad2: 0,
};

/// What `GroupSpec::fingerprint()` covers: the base asset and the legs.
fn same_fingerprint(a: &GroupSpec, b: &GroupSpec) -> bool {
    a.asset == b.asset && a.n_legs == b.n_legs && a.legs.as_bytes() == b.legs.as_bytes()
}

/// Builds an SPB1 blob from the strategy TOML.
pub fn build(t: &Table, schemas: &Path, rd: &RefData, against: Option<&Against<'_>>) -> Res<StrategyOut> {
    check_keys(t, &["strategy", "defaults", "groups"], "strategy TOML")?;
    let st = opt_table(t, "strategy", "strategy TOML")?
        .map_or_else(|| rej("strategy TOML: missing table `strategy`"), Ok)?;
    check_keys(
        st,
        &[
            "instance",
            "account",
            "schema",
            "version",
            "source",
            "generated_at_ns",
        ],
        "strategy",
    )?;
    let inst_name = req_str(st, "instance", "strategy")?;
    let acct_name = req_str(st, "account", "strategy")?;
    let sch = opt_table(st, "schema", "strategy")?.map_or_else(|| rej("strategy: missing `schema`"), Ok)?;
    check_keys(sch, &["id", "version"], "strategy.schema")?;
    let schema_id: u32 = req(sch, "id", "strategy.schema")?;
    let ver: u16 = req(sch, "version", "strategy.schema")?;
    let version: u64 = req(st, "version", "strategy")?;
    let source = match req_str(st, "source", "strategy")? {
        "manual" => ParamSource::Manual,
        "pipeline" => ParamSource::Pipeline,
        other => {
            return rej(format!(
                "strategy.source: \"{other}\" is not one of \"manual\", \"pipeline\""
            ));
        }
    };
    let generated_at_ns: i64 = opt(st, "generated_at_ns", "strategy")?;

    // Instance, account, binding.
    let inst = rd
        .instances_in_order()
        .iter()
        .find(|i| &*i.name == inst_name)
        .map_or_else(
            || rej(format!("strategy.instance: DanglingRef instance `{inst_name}`")),
            Ok,
        )?;
    let (sid, kind_id) = (inst.id, inst.kind_id);
    let account = (1..=u8::MAX)
        .map(AccountId)
        .take_while(|a| rd.account(*a).id.0 != 0)
        .find(|a| &*rd.account(*a).label == acct_name)
        .map_or_else(
            || rej(format!("strategy.account: DanglingRef account `{acct_name}`")),
            Ok,
        )?;
    if schema_id != u32::from(kind_id) {
        return rej(format!(
            "strategy.schema.id = {schema_id}: KindMismatch (instance `{inst_name}` has kind_id {kind_id})"
        ));
    }
    if let Err(e) = rd.check_binding(account, sid, kind_id) {
        return rej(format!(
            "{}: account `{acct_name}` is not bound to instance `{inst_name}`",
            e.code.name()
        ));
    }

    // Schema.
    let ls = load_schema(schemas, kind_id)?;
    let view = AtVersion::new(&ls.schema, ver).map_or_else(
        || {
            rej(format!(
                "strategy.schema.version = {ver}: the schema directory has version {}",
                ls.schema.version()
            ))
        },
        Ok,
    )?;
    if ver == 0 {
        return rej("strategy.schema.version: must be ≥ 1");
    }
    if ls.non_prod_only && rd.account(account).is_prod {
        return rej(format!(
            "NonProdOnly: kind `{}` is non_prod_only and account `{acct_name}` has is_prod = true",
            ls.entry.name
        ));
    }

    // Defaults.
    let mut defaults: Vec<i64> = (0..view.n_fields())
        .filter_map(|ix| view.field(ix).map(|f| f.default))
        .collect();
    if let Some(d) = opt_table(t, "defaults", "strategy TOML")? {
        for (k, v) in d {
            let ctx = format!("defaults.{k}");
            let ix = field_at(&view, k, &ctx)?;
            let f = view
                .field(ix)
                .map_or_else(|| rej(format!("{ctx}: unknown field")), Ok)?;
            if matches!(f.unit, UnitKind::Qty | UnitKind::Px) {
                return rej(format!(
                    "{ctx}: Qty/Px fields are per group (asset exponent); write them in [groups.overrides]"
                ));
            }
            let raw = parse_value(f, &text_of(v, &ctx)?, None)
                .or_else(|e| rej(format!("{ctx}: {}", param_err(e, &view, &[]))))?;
            if let Some(slot) = defaults.get_mut(usize::from(ix)) {
                *slot = raw;
            }
        }
    }

    // Groups.
    struct G {
        spec: GroupSpec,
        overrides: Vec<(u16, i64)>,
    }
    let mut groups: Vec<G> = Vec::new();
    for (i, g) in opt_array(t, "groups", "strategy TOML")?.iter().enumerate() {
        let ctx = format!("groups[{i}]");
        let g = as_table(g, &ctx)?;
        check_keys(g, &["n", "asset", "open_allowed", "legs", "overrides"], &ctx)?;
        let n = req_int(g, "n", &ctx)?;
        let id = u32::try_from(n)
            .ok()
            .and_then(|n| GroupId::compose(sid, n))
            .map_or_else(
                || rej(format!("{ctx}.n = {n}: must be in 1..={}", GroupId::N_MAX)),
                Ok,
            )?;
        let asset_name = req_str(g, "asset", &ctx)?;
        let (asset, exp) = asset_by_name(rd, asset_name).map_or_else(
            || rej(format!("{ctx}.asset: DanglingRef asset `{asset_name}`")),
            Ok,
        )?;
        let legs_v = opt_array(g, "legs", &ctx)?;
        if legs_v.is_empty() || legs_v.len() > 4 {
            return rej(format!("{ctx}.legs: 1 to 4 legs"));
        }
        let mut legs = [NO_LEG; 4];
        for (k, l) in legs_v.iter().enumerate() {
            let lc = format!("{ctx}.legs[{k}]");
            let l = as_table(l, &lc)?;
            check_keys(l, &["inst"], &lc)?;
            let (iid, v) = inst_by_name(rd, req_str(l, "inst", &lc)?, &lc)?;
            if let Some(slot) = legs.get_mut(k) {
                *slot = leg_def(iid, v);
            }
        }
        let spec = GroupSpec {
            id,
            asset,
            n_legs: legs_v.len() as u8,
            open_allowed: u8::from(req_bool(g, "open_allowed", &ctx)?),
            legs,
            _rsv: 0,
        };
        let mut lv = [LegView {
            inst: InstId(0),
            venue: VenueId(0),
        }; 4];
        let gv: GroupSpecView<'_> = spec.view(&mut lv);
        if let Err(e) = rd.check_group(account, &gv) {
            return rej(format!(
                "{ctx} (GroupId {:#010x}): {} (leg {})",
                id.0,
                e.code.name(),
                e.row
            ));
        }
        let mut overrides = Vec::new();
        if let Some(o) = opt_table(g, "overrides", &ctx)? {
            for (k, v) in o {
                let oc = format!("{ctx}.overrides.{k}");
                let ix = field_at(&view, k, &oc)?;
                let f = view
                    .field(ix)
                    .map_or_else(|| rej(format!("{oc}: unknown field")), Ok)?;
                if !f.per_group {
                    return rej(format!("{oc}: NotPerGroup (field `{k}` is not per_group)"));
                }
                let raw = parse_value(f, &text_of(v, &oc)?, Some(exp))
                    .or_else(|e| rej(format!("{oc}: {}", param_err(e, &view, &[]))))?;
                overrides.push((ix, raw));
            }
        }
        groups.push(G { spec, overrides });
    }
    groups.sort_by_key(|g| g.spec.id.0);
    if let Some(w) = groups
        .windows(2)
        .find(|w| w.first().map(|a| a.spec.id) == w.get(1).map(|b| b.spec.id))
    {
        let id = w.first().map_or(0, |g| g.spec.id.0);
        return rej(format!("duplicate GroupId {id:#010x}"));
    }

    // GroupId continuation and group_ctr_hwm.
    let max_n = groups.iter().map(|g| g.spec.id.counter()).max().unwrap_or(0);
    let hwm = match against {
        None => {
            if version != 1 {
                return input(format!(
                    "strategy.version = {version}: only the first version (1) of ({inst_name}, {acct_name}) may be built without --against"
                ));
            }
            max_n
        }
        Some(a) => {
            let cur = a.view;
            if cur.hdr.strategy != sid || cur.hdr.account != account {
                return rej("--against: the blob is for another (instance, account)");
            }
            if version <= cur.hdr.version {
                return rej(format!(
                    "strategy.version = {version}: must be greater than the version in effect ({})",
                    cur.hdr.version
                ));
            }
            let h = cur
                .groups
                .iter()
                .map(|g| g.id.counter())
                .max()
                .unwrap_or(0)
                .max(cur.hdr.group_ctr_hwm);
            for g in &groups {
                match cur.groups.iter().find(|c| c.id == g.spec.id) {
                    Some(c) => {
                        if !same_fingerprint(c, &g.spec) {
                            return rej(format!(
                                "GroupId {:#010x}: fingerprint (asset and legs) differs from the blob in effect; an id never points to a second fingerprint",
                                g.spec.id.0
                            ));
                        }
                    }
                    None => {
                        let n = g.spec.id.counter();
                        if n <= h {
                            return rej(format!(
                                "new group n = {n} is not greater than H = {h} (= max(group_ctr_hwm {}, max n in effect {})); removed n are never reused",
                                cur.hdr.group_ctr_hwm,
                                cur.groups.iter().map(|g| g.id.counter()).max().unwrap_or(0)
                            ));
                        }
                    }
                }
            }
            h.max(max_n)
        }
    };

    // Body through the SDK codec.
    let specs: Vec<GroupSpec> = groups.iter().map(|g| g.spec).collect();
    let overrides: Vec<ParamOverride> = groups
        .iter()
        .enumerate()
        .flat_map(|(gix, g)| {
            g.overrides
                .iter()
                .map(move |(fx, v)| ParamOverride::new(gix as u16, *fx, *v))
        })
        .collect();
    let n_groups = u16::try_from(specs.len()).or_else(|_| rej("too many groups"))?;
    if rd.account(account).is_prod {
        hft_strategy_sdk::params::check_production_values(
            &view,
            &defaults,
            true,
            hft_strategy_sdk::params::GROUP_IX_NONE,
        )
        .or_else(|e| {
            rej(format!(
                "production account `{acct_name}`: {}",
                param_err(e, &view, &specs)
            ))
        })?;
        for o in &overrides {
            if view
                .field(o.field_ix)
                .and_then(|f| f.prod_min)
                .is_some_and(|min| o.value < min)
            {
                return rej(format!(
                    "production account `{acct_name}`: {}",
                    param_err(
                        ParamError::OutOfRange {
                            field_ix: o.field_ix,
                            group_ix: o.group_ix
                        },
                        &view,
                        &specs,
                    )
                ));
            }
        }
    }
    let body = encode_body(&view, n_groups, &RawParams { defaults, overrides })
        .or_else(|e| rej(param_err(e, &view, &specs)))?;
    let hdr = StrategyParamsHdr {
        strategy: sid,
        account,
        kind_id,
        schema_id: view.id(),
        schema_ver: ver,
        source,
        _pad: 0,
        n_groups,
        _pad2: 0,
        version,
        generated_at_ns,
        body_len: u32::try_from(body.len()).or_else(|_| rej("body too large"))?,
        group_ctr_hwm: hwm,
        schema_digest: ls.schema.schema_digest(ver),
    };
    let mut blob = hdr.as_bytes().to_vec();
    for g in &specs {
        blob.extend_from_slice(g.as_bytes());
    }
    blob.extend_from_slice(&body);
    let al = aligned(&blob);
    if let Err(e) = parse_spb1(aligned_bytes(&al, blob.len())) {
        return rej(format!("parse_spb1 rejected the output: {e:?}"));
    }
    Ok(StrategyOut { blob, hdr })
}

/// Decoded strategy blob for show and diff.
#[derive(Debug)]
pub struct Decoded {
    pub hdr: StrategyParamsHdr,
    pub groups: Vec<GroupSpec>,
    pub raw: Option<RawParams>,
}

/// Parses an SPB1 blob (container) and, with a schema, its body.
pub fn decode(blob: &[u8], schema: Option<&LoadedSchema>) -> Res<Decoded> {
    let al = aligned(blob);
    let v = parse_spb1(aligned_bytes(&al, blob.len()))
        .or_else(|e| rej(format!("not a valid SPB1 blob: {e:?}")))?;
    let raw = match schema {
        Some(ls) => Some(
            decode_body(&ls.schema, v.hdr.schema_ver, v.hdr.n_groups, v.body)
                .or_else(|e| rej(format!("body: {}", param_err(e, &ls.schema, v.groups))))?,
        ),
        None => None,
    };
    Ok(Decoded {
        hdr: *v.hdr,
        groups: v.groups.to_vec(),
        raw,
    })
}

/// Effective value of field `fx` for group `gix`.
pub fn effective(raw: &RawParams, gix: usize, fx: u16) -> Option<i64> {
    raw.overrides
        .iter()
        .find(|o| usize::from(o.group_ix) == gix && o.field_ix == fx)
        .map(|o| o.value)
}

/// Readable rendering (`show`): header, defaults, groups with legs and overrides.
pub fn render(d: &Decoded, schema: Option<&LoadedSchema>, rd: Option<&RefData>) -> String {
    let h = &d.hdr;
    let mut s = String::new();
    let line = |s: &mut String, x: String| {
        s.push_str(&x);
        s.push('\n');
    };
    line(&mut s, format!("strategy = {}", h.strategy.0));
    line(&mut s, format!("account = {}", h.account.0));
    line(&mut s, format!("kind_id = {}", h.kind_id));
    line(&mut s, format!("schema_id = {}", h.schema_id));
    line(&mut s, format!("schema_ver = {}", h.schema_ver));
    line(
        &mut s,
        format!(
            "source = {}",
            if h.source == ParamSource::Manual {
                "manual"
            } else {
                "pipeline"
            }
        ),
    );
    line(&mut s, format!("n_groups = {}", h.n_groups));
    line(&mut s, format!("version = {}", h.version));
    line(&mut s, format!("generated_at_ns = {}", h.generated_at_ns));
    line(&mut s, format!("group_ctr_hwm = {}", h.group_ctr_hwm));
    line(&mut s, format!("schema_digest = {:016x}", h.schema_digest));
    let fields: Vec<FieldRef<'_>> = schema
        .map(|ls| {
            (0..ls.schema.n_fields())
                .filter_map(|i| ls.schema.field(i))
                .collect()
        })
        .unwrap_or_default();
    if let Some(raw) = &d.raw {
        line(&mut s, "[defaults]".into());
        for (ix, v) in raw.defaults.iter().enumerate() {
            match fields.get(ix) {
                Some(f) if !matches!(f.unit, UnitKind::Qty | UnitKind::Px) => {
                    line(&mut s, format!("{} = {}", f.name, format_value(*f, *v, None)));
                }
                Some(f) => line(&mut s, format!("{} = {v} (raw)", f.name)),
                None => line(&mut s, format!("#{ix} = {v}")),
            }
        }
    }
    for (gix, g) in d.groups.iter().enumerate() {
        line(
            &mut s,
            format!("[[groups]] id = {:#010x} n = {}", g.id.0, g.id.counter()),
        );
        let asset = rd.map_or_else(|| g.asset.0.to_string(), |r| r.asset(g.asset).name.to_string());
        line(&mut s, format!("asset = {asset}"));
        line(&mut s, format!("open_allowed = {}", g.open_allowed != 0));
        let legs: Vec<String> = g
            .legs
            .iter()
            .take(usize::from(g.n_legs))
            .map(|l| rd.map_or_else(|| format!("inst {}", l.inst.0), |r| inst_label(r, l.inst)))
            .collect();
        line(&mut s, format!("legs = [{}]", legs.join(", ")));
        let exp = rd.map(|r| r.asset(g.asset).exp);
        if let Some(raw) = &d.raw {
            for o in raw.overrides.iter().filter(|o| usize::from(o.group_ix) == gix) {
                match fields.get(usize::from(o.field_ix)) {
                    Some(f) => line(
                        &mut s,
                        format!("  {} = {}", f.name, format_value(*f, o.value, exp)),
                    ),
                    None => line(&mut s, format!("  #{} = {}", o.field_ix, o.value)),
                }
            }
        }
    }
    s
}
