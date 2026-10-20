//! `build --kind refdata`: `params/refdata/*.toml` (or one file, e.g. a `hft_refdata::dump`
//! output) → RDB1 through `RdbBuilder`, then `RefData::load` (all load-time checks) plus the
//! checks that only hft-cfg can make: account name prefix, lower-case `uid_policy` with an Admin
//! sign-off, unknown keys, per-inst exponents equal to the asset's, Sim model indexes against
//! `--sim-models`, and the capability override rules.
//!
//! Builder calls are made in a canonical order (assets, venues, caps, insts, instances, accounts
//! by id; account UIDs by (account, venue); limits and endpoints by venue then environment), so
//! the same content always gives the same bytes and `dump → build` reproduces the blob.

use std::collections::BTreeMap;
use std::path::Path;

use hft_refdata::{
    AccountSpec, AccountVenueSpec, AliasSpec, AssetClass, AssetSpec, CapFlags, ChainToken, DeadManCap,
    Endpoint, EndpointEnv, EndpointRole, Exchange, FeeAssetRule, FeeOverrideSpec, FeePair, FeeSpec,
    FundingSettleKind, InstKind, InstSpec, InstStatus, InstanceSpec, LimitBucket, LimitScope, LiqMarker,
    LoadErrCode, LoadReport, MaintKind, MaintWindow, MarginData, MarginMode, MmrTier, MmrTierSpec,
    PositionMode, PriceBand, RateLimit, RdbBuilder, RefData, SimBalanceSpec, SimSpec, StaleGuardKind,
    TestnetKind, TsUnit, UidPolicy, VenueCaps, VenueSpec,
};
use hft_types::{
    AccountId, AssetExp, AssetId, InstId, MarketKind, Notional, Px, Qty, Rate, StrategyId, VenueId,
};
use toml::{Table, Value};

use crate::util::*;

/// Options of `build --kind refdata` besides the input.
#[derive(Clone, Debug, Default)]
pub struct RefdataOpts<'a> {
    /// Schema directory: resolves `kind = "<name>"` of instances through `index.json`.
    pub schemas: Option<&'a Path>,
    /// hft-core config: pre-check Sim `latency_model` / `margin_model` indexes.
    pub sim_models: Option<&'a Path>,
    /// Admin list for `uid_policy_signoff` (default `.github/hft-admins.txt`).
    pub admins: Option<&'a Path>,
    /// The blob currently in effect (`--against`): immutables and version rule.
    pub against: Option<&'a RefData>,
}

/// Result of a successful build.
#[derive(Clone, Debug)]
pub struct RefdataOut {
    pub blob: Vec<u8>,
    /// `(account label, Admin login)` of every `uid_policy = "unverifiable"` sign-off.
    pub signoffs: Vec<(String, String)>,
    /// `RefData::warnings()` rendered with the accounts and UID involved.
    pub warnings: Vec<Warning>,
    /// Same SHA-256 as `--against` (`AlreadyActive`, idempotent).
    pub already_active: bool,
}

/// One load warning (only `DuplicateVenueUid` between two non-prod accounts).
#[derive(Clone, Eq, PartialEq, Debug)]
pub struct Warning {
    pub code: &'static str,
    pub accounts: Vec<String>,
    pub uid: String,
}

/// `Code(section s, row r), …` of a load report.
pub fn report_text(r: &LoadReport) -> String {
    r.errors
        .iter()
        .map(|e| format!("{}(section {}, row {})", e.code.name(), e.section, e.row))
        .collect::<Vec<_>>()
        .join(", ")
}

fn ev<T: Copy>(all: &[T], name: fn(T) -> &'static str, t: &Table, key: &str, ctx: &str) -> Res<T> {
    parse_enum(all, name, req_str(t, key, ctx)?, &format!("{ctx}.{key}"))
}

fn rate(t: &Table, key: &str, ctx: &str) -> Res<Rate> {
    Ok(Rate(raw_or_int_decimal(t, key, -9, ctx)?))
}

/// Integer raw value, or a decimal string at `exp`.
fn raw_or_int_decimal(t: &Table, key: &str, exp: i8, ctx: &str) -> Res<i64> {
    match t.get(key) {
        Some(Value::Integer(x)) => Ok(*x),
        Some(Value::String(s)) => decimal(s, exp, &format!("{ctx}.{key}")),
        Some(_) => rej(format!("{ctx}.{key}: expected an integer or a decimal string")),
        None => rej(format!("{ctx}: missing key `{key}`")),
    }
}

fn opt_bool_text(t: &Table, key: &str, ctx: &str) -> Res<Option<bool>> {
    match t.get(key) {
        Some(Value::Boolean(b)) => Ok(Some(*b)),
        Some(Value::String(s)) if s == "none" => Ok(None),
        Some(Value::String(s)) if s == "false" => Ok(Some(false)),
        Some(Value::String(s)) if s == "true" => Ok(Some(true)),
        None => Ok(None),
        Some(_) => rej(format!("{ctx}.{key}: expected true, false or \"none\"")),
    }
}

/// Everything parsed, in builder call order.
#[derive(Default)]
struct Model {
    version: u64,
    created: i64,
    assets: Vec<AssetSpec>,
    venues: Vec<VenueSpec>,
    caps: Vec<VenueCaps>,
    insts: Vec<InstSpec>,
    instances: Vec<InstanceSpec>,
    accounts: Vec<AccountSpec>,
    account_venues: Vec<AccountVenueSpec>,
    fees: Vec<FeeSpec>,
    fee_overrides: Vec<FeeOverrideSpec>,
    sims: Vec<SimSpec>,
    sim_balances: Vec<SimBalanceSpec>,
    limits: Vec<(VenueId, RateLimit)>,
    endpoints: Vec<(VenueId, Endpoint)>,
    maint: Vec<MaintWindow>,
    tokens: Vec<ChainToken>,
    aliases: Vec<AliasSpec>,
    mmr: Vec<MmrTierSpec>,
    signoffs: Vec<(String, String)>,
}

impl Model {
    fn encode(&self) -> Vec<u8> {
        let mut b = RdbBuilder::new(self.version);
        b.created_utc_ns(self.created);
        self.assets.iter().for_each(|x| {
            b.asset(x.clone());
        });
        self.venues.iter().for_each(|x| {
            b.venue(x.clone());
        });
        self.caps.iter().for_each(|x| b.caps(x.clone()));
        self.insts.iter().for_each(|x| {
            b.inst(x.clone());
        });
        self.instances.iter().for_each(|x| {
            b.instance(x.clone());
        });
        self.accounts.iter().for_each(|x| {
            b.account(x.clone());
        });
        self.account_venues
            .iter()
            .for_each(|x| b.account_venue(x.clone()));
        self.fees.iter().for_each(|x| b.fee(*x));
        self.fee_overrides.iter().for_each(|x| b.fee_override(*x));
        self.sims.iter().for_each(|x| b.sim(*x));
        self.sim_balances.iter().for_each(|x| b.sim_balance(*x));
        self.limits.iter().for_each(|(v, x)| b.limit(*v, *x));
        self.endpoints.iter().for_each(|(v, x)| b.endpoint(*v, x.clone()));
        self.maint.iter().for_each(|x| b.maint(*x));
        self.tokens.iter().for_each(|x| b.token(*x));
        self.aliases.iter().for_each(|x| b.alias(x.clone()));
        self.mmr.iter().for_each(|x| b.mmr_tier(*x));
        b.finish()
    }
}

/// Name lookups built while parsing.
#[derive(Default)]
struct Names {
    assets: BTreeMap<String, (AssetId, AssetExp)>,
    venues: BTreeMap<u8, InstKind>,
    insts: BTreeMap<(u8, String), InstId>,
    instances: BTreeMap<String, StrategyId>,
    accounts: BTreeMap<String, AccountId>,
}

impl Names {
    fn asset(&self, name: &str, ctx: &str) -> Res<(AssetId, AssetExp)> {
        self.assets
            .get(name)
            .copied()
            .map_or_else(|| rej(format!("{ctx}: DanglingRef asset `{name}`")), Ok)
    }
    /// Inst by symbol on a venue; `""` = every inst (0).
    fn inst(&self, v: VenueId, sym: &str, ctx: &str) -> Res<InstId> {
        if sym.is_empty() {
            return Ok(InstId(0));
        }
        self.insts.get(&(v.0, sym.to_string())).copied().map_or_else(
            || rej(format!("{ctx}: DanglingRef inst `{}:{sym}`", venue_name(v))),
            Ok,
        )
    }
}

/// Instances or accounts given either keyed (`[instances.<name>]`) or as an array
/// (`[[instance]]` with `name`, `[[account]]` with `label`). Array entries are renamed to the
/// keyed spelling: account `venue_uids` → `uids`, `sim.initial_balances` → `sim.balances`.
fn entries(root: &Table, keyed: &str, array: &str, name_key: &str) -> Res<Vec<(String, Table)>> {
    let mut out = Vec::new();
    if let Some(t) = opt_table(root, keyed, "refdata")? {
        for (k, v) in t {
            out.push((k.clone(), as_table(v, &format!("{keyed}.{k}"))?.clone()));
        }
    }
    for (i, v) in opt_array(root, array, "refdata")?.iter().enumerate() {
        let ctx = format!("{array}[{i}]");
        let mut t = as_table(v, &ctx)?.clone();
        let name = match t.remove(name_key) {
            Some(Value::String(s)) => s,
            _ => return rej(format!("{ctx}: missing key `{name_key}`")),
        };
        if let Some(u) = t.remove("venue_uids") {
            t.insert("uids".into(), u);
        }
        if let Some(Value::Table(sim)) = t.get_mut("sim")
            && let Some(b) = sim.remove("initial_balances")
        {
            sim.insert("balances".into(), b);
        }
        out.push((name, t));
    }
    Ok(out)
}

/// Builds and validates a refdata blob from the merged TOML.
pub fn build(root: &Table, o: &RefdataOpts<'_>) -> Res<RefdataOut> {
    check_keys(
        root,
        &[
            "version",
            "created_utc_ns",
            "assets",
            "venues",
            "caps",
            "venue_caps",
            "insts",
            "instances",
            "instance",
            "accounts",
            "account",
            "fees",
            "fee_overrides",
            "aliases",
            "maint",
            "tokens",
        ],
        "refdata",
    )?;
    let mut m = Model {
        version: req(root, "version", "refdata")?,
        created: opt(root, "created_utc_ns", "refdata")?,
        ..Model::default()
    };
    let mut n = Names::default();
    parse_assets(root, &mut m, &mut n)?;
    parse_venues(root, &mut m, &mut n)?;
    parse_caps(root, &mut m, &n)?;
    parse_insts(root, &mut m, &mut n)?;
    parse_instances(root, &mut m, &mut n, o)?;
    parse_accounts(root, &mut m, &mut n, o)?;
    parse_tables(root, &mut m, &n)?;

    let blob = m.encode();
    let mut already_active = false;
    let rd = match RefData::load(&blob, o.against) {
        Ok(rd) => rd,
        Err(r) if r.code() == LoadErrCode::AlreadyActive && r.errors.len() == 1 => {
            already_active = true;
            RefData::load(&blob, None).or_else(|r| rej(format!("RefData::load: {}", report_text(&r))))?
        }
        Err(r) => return rej(format!("RefData::load: {}", report_text(&r))),
    };
    let warnings = warnings(&rd);
    for v in rd.venues_in_order() {
        let caps = rd.caps(*v);
        if !m.accounts.iter().any(|a| !a.sim && a.venues & (1u64 << v.0) != 0) {
            continue;
        }
        if let Some(protocol) = default_caps(*v, &m.assets)
            && protocol.market == MarketKind::LinearPerp
            && (!caps.reduce_only
                || !caps.flags.contains(CapFlags::IOC | CapFlags::POSITION_CLOSE)
                || caps.flags.contains(CapFlags::POSITION_CLOSE_ALL)
                    != protocol.flags.contains(CapFlags::POSITION_CLOSE_ALL))
        {
            return rej(format!(
                "KillCapsMissing: {} requires protocol position-close mode, reduce-only and IOC; omit obsolete complete caps rows to use protocol defaults",
                venue_name(*v)
            ));
        }
    }
    Ok(RefdataOut {
        blob,
        signoffs: m.signoffs,
        warnings,
        already_active,
    })
}

/// `RefData::warnings()` with the two accounts and the UID (row = the later AccountVenues row).
pub fn warnings(rd: &RefData) -> Vec<Warning> {
    let avs = rd.account_venues();
    rd.warnings()
        .iter()
        .filter_map(|w| {
            let (a, v, uid) = avs.get(w.row as usize)?;
            let ex = rd.venue(*v).exchange;
            let mut accounts: Vec<String> = avs
                .iter()
                .filter(|(b, w2, u)| u == uid && rd.venue(*w2).exchange == ex && (b != a || w2 != v))
                .map(|(b, ..)| rd.account(*b).label.to_string())
                .collect();
            accounts.push(rd.account(*a).label.to_string());
            accounts.sort();
            accounts.dedup();
            Some(Warning {
                code: w.code.name(),
                accounts,
                uid: uid.to_string(),
            })
        })
        .collect()
}

fn parse_assets(root: &Table, m: &mut Model, n: &mut Names) -> Res<()> {
    let Some(t) = opt_table(root, "assets", "refdata")? else {
        return Ok(());
    };
    for (name, v) in t {
        let ctx = format!("assets.{name}");
        let a = as_table(v, &ctx)?;
        check_keys(
            a,
            &[
                "id",
                "class",
                "px_exp",
                "qty_exp",
                "px_max",
                "px_max_raw",
                "qty_max",
                "qty_max_raw",
                "dust",
                "dust_raw",
            ],
            &ctx,
        )?;
        let exp = AssetExp {
            px_exp: req(a, "px_exp", &ctx)?,
            qty_exp: req(a, "qty_exp", &ctx)?,
        };
        let spec = AssetSpec {
            id: AssetId(req(a, "id", &ctx)?),
            name: name.as_str().into(),
            class: ev(AssetClass::ALL, AssetClass::name, a, "class", &ctx)?,
            exp,
            px_max: Px(raw_or_decimal(a, "px_max", exp.px_exp, &ctx, true)?.unwrap_or(0)),
            qty_max: Qty(raw_or_decimal(a, "qty_max", exp.qty_exp, &ctx, true)?.unwrap_or(0)),
            dust: Qty(raw_or_decimal(a, "dust", exp.qty_exp, &ctx, true)?.unwrap_or(0)),
        };
        n.assets.insert(name.clone(), (spec.id, exp));
        m.assets.push(spec);
    }
    m.assets.sort_by_key(|a| a.id.0);
    Ok(())
}

fn parse_venues(root: &Table, m: &mut Model, n: &mut Names) -> Res<()> {
    let Some(t) = opt_table(root, "venues", "refdata")? else {
        return Ok(());
    };
    for (name, v) in t {
        let ctx = format!("venues.{name}");
        let a = as_table(v, &ctx)?;
        check_keys(
            a,
            &[
                "id",
                "exchange",
                "product",
                "ws_max_streams_per_conn",
                "ws_max_in_msgs_per_s",
                "ws_max_conn_age_s",
                "ws_new_conn_per_5min_ip",
                "app_ping_interval_s",
                "ts_unit",
                "client_id_max_len",
                "client_id_prefix",
                "max_batch",
                "max_inflight_amend",
                "crossed_tolerance_default",
                "stale_guard",
                "rate_limits",
                "endpoints",
            ],
            &ctx,
        )?;
        let id = venue_or_rej(name, &ctx)?;
        if let Some(x) = opt_int(a, "id", &ctx)?
            && x != i64::from(id.0)
        {
            return rej(format!("{ctx}.id: {x} is not the id of `{name}` ({})", id.0));
        }
        let spec = VenueSpec {
            id,
            exchange: ev(Exchange::ALL, Exchange::name, a, "exchange", &ctx)?,
            product: ev(InstKind::ALL, InstKind::name, a, "product", &ctx)?,
            ws_max_streams_per_conn: req(a, "ws_max_streams_per_conn", &ctx)?,
            ws_max_in_msgs_per_s: req(a, "ws_max_in_msgs_per_s", &ctx)?,
            ws_max_conn_age_s: req(a, "ws_max_conn_age_s", &ctx)?,
            ws_new_conn_per_5min_ip: req(a, "ws_new_conn_per_5min_ip", &ctx)?,
            app_ping_interval_s: req(a, "app_ping_interval_s", &ctx)?,
            ts_unit: ev(TsUnit::ALL, TsUnit::name, a, "ts_unit", &ctx)?,
            client_id_max_len: req(a, "client_id_max_len", &ctx)?,
            client_id_prefix: opt_str(a, "client_id_prefix", &ctx)?.unwrap_or("").into(),
            max_batch: req(a, "max_batch", &ctx)?,
            max_inflight_amend: req(a, "max_inflight_amend", &ctx)?,
            crossed_tolerance_default: req(a, "crossed_tolerance_default", &ctx)?,
            stale_guard: ev(StaleGuardKind::ALL, StaleGuardKind::name, a, "stale_guard", &ctx)?,
        };
        n.venues.insert(id.0, spec.product);
        for (i, l) in opt_array(a, "rate_limits", &ctx)?.iter().enumerate() {
            let c = format!("{ctx}.rate_limits[{i}]");
            let l = as_table(l, &c)?;
            check_keys(l, &["scope", "bucket", "limit", "window_ms", "default_cost"], &c)?;
            m.limits.push((
                id,
                RateLimit {
                    scope: ev(LimitScope::ALL, LimitScope::name, l, "scope", &c)?,
                    bucket: ev(LimitBucket::ALL, LimitBucket::name, l, "bucket", &c)?,
                    limit: req(l, "limit", &c)?,
                    window_ms: req(l, "window_ms", &c)?,
                    default_cost: req(l, "default_cost", &c)?,
                },
            ));
        }
        for (i, e) in opt_array(a, "endpoints", &ctx)?.iter().enumerate() {
            let c = format!("{ctx}.endpoints[{i}]");
            let e = as_table(e, &c)?;
            check_keys(e, &["role", "env", "url", "conn_group"], &c)?;
            m.endpoints.push((
                id,
                Endpoint {
                    role: ev(EndpointRole::ALL, EndpointRole::name, e, "role", &c)?,
                    env: ev(EndpointEnv::ALL, EndpointEnv::name, e, "env", &c)?,
                    url: req_str(e, "url", &c)?.into(),
                    conn_group: opt(e, "conn_group", &c)?,
                },
            ));
        }
        m.venues.push(spec);
    }
    m.venues.sort_by_key(|v| v.id.0);
    // Stable: the file order is kept within (venue) and (venue, env).
    m.limits.sort_by_key(|(v, _)| v.0);
    m.endpoints.sort_by_key(|(v, e)| (v.0, e.env as u8));
    Ok(())
}

fn fee_rule(t: &Table, asset_key: Option<&str>, n: &Names, ctx: &str) -> Res<FeeAssetRule> {
    let (kind, asset) = match t.get("fee_asset_rule") {
        Some(Value::String(s)) => (
            s.clone(),
            asset_key.map(|k| opt_str(t, k, ctx)).transpose()?.flatten(),
        ),
        Some(Value::Table(x)) => {
            let c = format!("{ctx}.fee_asset_rule");
            check_keys(x, &["kind", "asset"], &c)?;
            (req_str(x, "kind", &c)?.to_string(), opt_str(x, "asset", &c)?)
        }
        Some(_) => return rej(format!("{ctx}.fee_asset_rule: expected a string or a table")),
        None => return rej(format!("{ctx}: missing key `fee_asset_rule`")),
    };
    let need = |a: Option<&str>| -> Res<AssetId> {
        n.asset(
            a.map_or_else(
                || rej(format!("{ctx}: fee_asset_rule `{kind}` needs an asset")),
                Ok,
            )?,
            ctx,
        )
        .map(|x| x.0)
    };
    match kind.replace('-', "_").as_str() {
        "quote" => Ok(FeeAssetRule::Quote),
        "received_asset" => Ok(FeeAssetRule::ReceivedAsset),
        "received_asset_unless_discount" => {
            Ok(FeeAssetRule::ReceivedAssetUnlessDiscount { token: need(asset)? })
        }
        "configured" => Ok(FeeAssetRule::Configured(need(asset)?)),
        _ => rej(format!("{ctx}.fee_asset_rule: unknown `{kind}`")),
    }
}

fn margin_data(t: &Table, poll_key: Option<&str>, ctx: &str) -> Res<MarginData> {
    let (kind, interval) = match t.get("margin_data") {
        Some(Value::String(s)) => (
            s.clone(),
            poll_key.map(|k| opt_int(t, k, ctx)).transpose()?.flatten(),
        ),
        Some(Value::Table(x)) => {
            let c = format!("{ctx}.margin_data");
            check_keys(x, &["kind", "interval_ms"], &c)?;
            (
                req_str(x, "kind", &c)?.to_string(),
                opt_int(x, "interval_ms", &c)?,
            )
        }
        Some(_) => return rej(format!("{ctx}.margin_data: expected a string or a table")),
        None => return rej(format!("{ctx}: missing key `margin_data`")),
    };
    match kind.replace('-', "_").as_str() {
        "push" => Ok(MarginData::Push),
        "not_applicable" => Ok(MarginData::NotApplicable),
        "poll" => Ok(MarginData::Poll {
            interval_ms: int_as(interval.unwrap_or(0), "interval_ms", ctx)?,
        }),
        _ => rej(format!("{ctx}.margin_data: unknown `{kind}`")),
    }
}

const CAPS_KEYS: &[&str] = &[
    "market",
    "fee_asset_rule",
    "fee_asset",
    "reduce_only",
    "required_position_mode",
    "required_margin_mode",
    "required_multi_asset",
    "required_spot_borrow",
    "funding_settle",
    "margin_data",
    "margin_poll_ms",
    "own_liquidation_marker",
    "liq_marker_arg",
    "ignorable_user_events",
    "flags",
    "amend_qty_down_only",
    "dms",
    "dms_min_ms",
    "dms_max_ms",
    "dms_timeout_ms",
    "dms_refresh_ms",
    "testnet",
];

fn strings(t: &Table, key: &str, ctx: &str) -> Res<Vec<String>> {
    opt_array(t, key, ctx)?
        .iter()
        .map(|v| {
            v.as_str()
                .map(str::to_string)
                .map_or_else(|| rej(format!("{ctx}.{key}: expected strings")), Ok)
        })
        .collect()
}

/// A complete capability row written in TOML (`[caps.<venue>]`, the dump form).
fn caps_full(t: &Table, venue: VenueId, n: &Names, ctx: &str) -> Res<VenueCaps> {
    check_keys(t, CAPS_KEYS, ctx)?;
    let market = match req_str(t, "market", ctx)?.replace('-', "_").as_str() {
        "spot" => MarketKind::Spot,
        "linear_perp" => MarketKind::LinearPerp,
        other => return rej(format!("{ctx}.market: unknown `{other}`")),
    };
    Ok(VenueCaps {
        venue,
        market,
        fee_asset_rule: fee_rule(t, Some("fee_asset"), n, ctx)?,
        reduce_only: req_bool(t, "reduce_only", ctx)?,
        required_position_mode: ev(
            PositionMode::ALL,
            PositionMode::name,
            t,
            "required_position_mode",
            ctx,
        )?,
        required_margin_mode: ev(MarginMode::ALL, MarginMode::name, t, "required_margin_mode", ctx)?,
        required_multi_asset: opt_bool_text(t, "required_multi_asset", ctx)?,
        required_spot_borrow: opt_bool_text(t, "required_spot_borrow", ctx)?,
        funding_settle: ev(
            FundingSettleKind::ALL,
            FundingSettleKind::name,
            t,
            "funding_settle",
            ctx,
        )?,
        margin_data: margin_data(t, Some("margin_poll_ms"), ctx)?,
        own_liquidation_marker: ev(LiqMarker::ALL, LiqMarker::name, t, "own_liquidation_marker", ctx)?,
        liq_marker_arg: opt_str(t, "liq_marker_arg", ctx)?.unwrap_or("").into(),
        ignorable_user_events: strings(t, "ignorable_user_events", ctx)?
            .into_iter()
            .map(Into::into)
            .collect::<Vec<Box<str>>>()
            .into(),
        flags: CapFlags(req(t, "flags", ctx)?),
        amend_qty_down_only: opt_bool(t, "amend_qty_down_only", ctx)?.unwrap_or(false),
        dms: ev(DeadManCap::ALL, DeadManCap::name, t, "dms", ctx)?,
        dms_min_ms: opt(t, "dms_min_ms", ctx)?,
        dms_max_ms: opt(t, "dms_max_ms", ctx)?,
        dms_timeout_ms: opt(t, "dms_timeout_ms", ctx)?,
        dms_refresh_ms: opt(t, "dms_refresh_ms", ctx)?,
        testnet: ev(TestnetKind::ALL, TestnetKind::name, t, "testnet", ctx)?,
        _pad: 0,
    })
}

/// Applies a `[venue_caps.<venue>]` override to the base row under the override rules:
/// capability bits may only be cleared, `amend_qty_down_only` only set, `ignorable_user_events`
/// only extended; `required_*`, `fee_asset_rule`, `margin_data` and the dead-man timeout and
/// refresh may change and then need `reason`.
fn apply_override(base: &VenueCaps, t: &Table, n: &Names, ctx: &str) -> Res<VenueCaps> {
    check_keys(
        t,
        &[
            "fee_asset_rule",
            "required_position_mode",
            "required_margin_mode",
            "required_multi_asset",
            "required_spot_borrow",
            "margin_data",
            "flags",
            "ignorable_user_events",
            "amend_qty_down_only",
            "dms_timeout_ms",
            "dms_refresh_ms",
            "reason",
        ],
        ctx,
    )?;
    let mut c = base.clone();
    let mut needs_reason = false;
    if t.contains_key("fee_asset_rule") {
        c.fee_asset_rule = fee_rule(t, None, n, ctx)?;
        needs_reason = true;
    }
    if t.contains_key("required_position_mode") {
        c.required_position_mode = ev(
            PositionMode::ALL,
            PositionMode::name,
            t,
            "required_position_mode",
            ctx,
        )?;
        needs_reason = true;
    }
    if t.contains_key("required_margin_mode") {
        c.required_margin_mode = ev(MarginMode::ALL, MarginMode::name, t, "required_margin_mode", ctx)?;
        needs_reason = true;
    }
    if t.contains_key("required_multi_asset") {
        c.required_multi_asset = opt_bool_text(t, "required_multi_asset", ctx)?;
        needs_reason = true;
    }
    if t.contains_key("required_spot_borrow") {
        c.required_spot_borrow = opt_bool_text(t, "required_spot_borrow", ctx)?;
        needs_reason = true;
    }
    if t.contains_key("margin_data") {
        c.margin_data = margin_data(t, None, ctx)?;
        needs_reason = true;
    }
    if let Some(x) = opt_int(t, "flags", ctx)? {
        let f: u32 = int_as(x, "flags", ctx)?;
        if f & !base.flags.0 != 0 {
            return rej(format!(
                "{ctx}.flags: capability bits {:#x} are not supported by the default row (bits may only be cleared)",
                f & !base.flags.0
            ));
        }
        c.flags = CapFlags(f);
    }
    if t.contains_key("ignorable_user_events") {
        let ev = strings(t, "ignorable_user_events", ctx)?;
        if let Some(missing) = base
            .ignorable_user_events
            .iter()
            .find(|e| !ev.iter().any(|x| x == &***e))
        {
            return rej(format!(
                "{ctx}.ignorable_user_events: may only append to the default list (missing `{missing}`)"
            ));
        }
        let mut all: Vec<Box<str>> = base.ignorable_user_events.to_vec();
        all.extend(
            ev.into_iter()
                .filter(|e| !base.ignorable_user_events.iter().any(|b| &**b == e))
                .map(Into::into),
        );
        c.ignorable_user_events = all.into();
    }
    if let Some(b) = opt_bool(t, "amend_qty_down_only", ctx)? {
        if base.amend_qty_down_only && !b {
            return rej(format!(
                "{ctx}.amend_qty_down_only: may only change from false to true"
            ));
        }
        c.amend_qty_down_only = b;
    }
    if let Some(x) = opt_int(t, "dms_timeout_ms", ctx)? {
        c.dms_timeout_ms = int_as(x, "dms_timeout_ms", ctx)?;
        needs_reason = true;
    }
    if let Some(x) = opt_int(t, "dms_refresh_ms", ctx)? {
        c.dms_refresh_ms = int_as(x, "dms_refresh_ms", ctx)?;
        needs_reason = true;
    }
    if needs_reason && opt_str(t, "reason", ctx)?.is_none_or(str::is_empty) {
        return rej(format!(
            "{ctx}: overriding required_*, fee_asset_rule, margin_data or the dead man needs `reason`"
        ));
    }
    Ok(c)
}

/// Linked protocol defaults; asset IDs come from this input, not a hard-coded token table.
fn default_caps(v: VenueId, assets: &[AssetSpec]) -> Option<VenueCaps> {
    use hft_venue_proto::VenueProtocol;
    let asset = |name: &str| assets.iter().find(|a| a.name.as_ref() == name).map(|a| a.id);
    Some(match v {
        VenueId::BINANCE_USDM => venue_binance::Usdm::default_caps(asset),
        VenueId::BINANCE_SPOT => venue_binance::Spot::default_caps(asset),
        VenueId::BITGET_UTA_USDT_FUT => venue_bitget::BitgetUsdtFutures::default_caps(asset),
        VenueId::BITGET_UTA_SPOT => venue_bitget::BitgetSpot::default_caps(asset),
        VenueId::POLYMARKET_CLOB => venue_polymarket::PolymarketClob::default_caps(asset),
        _ => return None,
    })
}

fn parse_caps(root: &Table, m: &mut Model, n: &Names) -> Res<()> {
    let full = opt_table(root, "caps", "refdata")?;
    let over = opt_table(root, "venue_caps", "refdata")?;
    let mut venues: Vec<VenueId> = m.venues.iter().map(|v| v.id).collect();
    for (section, table) in [("caps", full), ("venue_caps", over)] {
        let mut seen = Vec::new();
        for (k, value) in table.iter().flat_map(|t| t.iter()) {
            let ctx = format!("{section}.{k}");
            let v = venue_or_rej(k, &ctx)?;
            as_table(value, &ctx)?;
            if seen.contains(&v) {
                return rej(format!("{ctx}: duplicate venue alias"));
            }
            seen.push(v);
            if !venues.contains(&v) {
                venues.push(v);
            }
        }
    }
    venues.sort_by_key(|v| v.0);
    let find = |t: Option<&Table>, v: VenueId| -> Option<(String, Table)> {
        t?.iter()
            .find(|(k, _)| parse_venue(k) == Some(v))
            .and_then(|(k, x)| Some((k.clone(), x.as_table()?.clone())))
    };
    for v in venues {
        // Complete rows are the stable dump representation. Keep them byte-exact on a
        // roundtrip; omitted rows use the current linked protocol defaults.
        let base = match (default_caps(v, &m.assets), find(full, v)) {
            (_, Some((k, t))) => caps_full(&t, v, n, &format!("caps.{k}"))?,
            (Some(d), None) => d,
            (None, None) => {
                return rej(format!(
                    "venue_caps.{}: no protocol defaults or complete caps row",
                    venue_name(v)
                ));
            }
        };
        let row = match find(over, v) {
            Some((k, t)) => apply_override(&base, &t, n, &format!("venue_caps.{k}"))?,
            None => base,
        };
        m.caps.push(row);
    }
    Ok(())
}

fn parse_insts(root: &Table, m: &mut Model, n: &mut Names) -> Res<()> {
    let Some(t) = opt_table(root, "insts", "refdata")? else {
        return Ok(());
    };
    for (key, v) in t {
        let ctx = format!("insts.\"{key}\"");
        let a = as_table(v, &ctx)?;
        check_keys(
            a,
            &[
                "id",
                "status",
                "kind",
                "flags",
                "asset",
                "quote",
                "px_exp",
                "qty_exp",
                "px_unit_shift",
                "qty_unit_shift",
                "tick",
                "lot",
                "min_qty",
                "tick_raw",
                "lot_raw",
                "min_qty_raw",
                "max_qty",
                "max_qty_raw",
                "min_notional",
                "min_notional_raw",
                "book_window",
                "l2_depth",
                "crossed_tolerance",
                "venue_num_id",
                "funding_interval_s",
                "funding_anchor_s",
                "max_leverage",
                "listed_utc_ns",
                "delivery_utc_ns",
                "price_band_up",
                "price_band_down",
                "mmr_tiers",
            ],
            &ctx,
        )?;
        let Some((vname, symbol)) = key.split_once(':') else {
            return rej(format!("{ctx}: key must be `<venue>:<symbol>`"));
        };
        let venue = venue_or_rej(vname, &ctx)?;
        let asset_name = req_str(a, "asset", &ctx)?;
        let (asset, exp) = n.asset(asset_name, &ctx)?;
        for (k, have) in [("px_exp", exp.px_exp), ("qty_exp", exp.qty_exp)] {
            if let Some(x) = opt_int(a, k, &ctx)?
                && x != i64::from(have)
            {
                return rej(format!(
                    "{ctx}.{k} = {x}: asset {asset_name} has {k} = {have} (one exponent per asset on every venue)"
                ));
            }
        }
        let (quote, _) = n.asset(req_str(a, "quote", &ctx)?, &ctx)?;
        let px_unit_shift: i8 = opt(a, "px_unit_shift", &ctx)?;
        let qty_unit_shift: i8 = opt(a, "qty_unit_shift", &ctx)?;
        let px_wire = exp.px_exp.saturating_add(px_unit_shift);
        let qty_wire = exp.qty_exp.saturating_sub(qty_unit_shift);
        let tick_str = req_str(a, "tick", &ctx)?;
        let lot_str = req_str(a, "lot", &ctx)?;
        let min_qty_str = req_str(a, "min_qty", &ctx)?;
        // Steps given only as text are parsed at the wire exponent; a text that does not parse
        // there is left as 0 so that RefData::load reports it in its step order (an exponent
        // error of step 6 before the TickMismatch / AssetExpTooCoarse of step 7).
        let step = |k: &str, s: &str, e: i8| -> Res<i64> {
            Ok(opt_int(a, &format!("{k}_raw"), &ctx)?
                .unwrap_or_else(|| hft_types::parse_exact(s.as_bytes(), e).unwrap_or(0)))
        };
        let venue_num_id = match a.get("venue_num_id") {
            None => 0,
            Some(Value::Integer(x)) => int_as(*x, "venue_num_id", &ctx)?,
            Some(Value::String(s)) => s
                .parse::<u64>()
                .or_else(|_| rej(format!("{ctx}.venue_num_id: not an integer")))?,
            Some(_) => return rej(format!("{ctx}.venue_num_id: expected an integer")),
        };
        let id = InstId(req(a, "id", &ctx)?);
        let mut tiers = Vec::new();
        for (i, x) in opt_array(a, "mmr_tiers", &ctx)?.iter().enumerate() {
            let pair = x.as_array().filter(|p| p.len() == 2);
            let (Some(nm), Some(r)) = (
                pair.and_then(|p| p.first()).and_then(Value::as_integer),
                pair.and_then(|p| p.get(1)).and_then(Value::as_integer),
            ) else {
                return rej(format!(
                    "{ctx}.mmr_tiers[{i}]: expected [notional_max_raw, mmr_raw]"
                ));
            };
            tiers.push(MmrTierSpec {
                inst: id,
                tier: MmrTier {
                    notional_max: Notional(nm),
                    mmr: Rate(r),
                },
            });
        }
        let spec = InstSpec {
            id,
            venue,
            symbol: symbol.to_string(),
            status: ev(InstStatus::ALL, InstStatus::name, a, "status", &ctx)?,
            kind: ev(InstKind::ALL, InstKind::name, a, "kind", &ctx)?,
            flags: opt(a, "flags", &ctx)?,
            asset,
            quote,
            px_unit_shift,
            qty_unit_shift,
            tick: step("tick", tick_str, px_wire)?,
            lot: step("lot", lot_str, qty_wire)?,
            min_qty: Qty(step("min_qty", min_qty_str, qty_wire)?),
            max_qty: Qty(raw_or_decimal(a, "max_qty", exp.qty_exp, &ctx, true)?.unwrap_or(0)),
            min_notional: Notional(raw_or_decimal(a, "min_notional", -6, &ctx, true)?.unwrap_or(0)),
            book_window: req(a, "book_window", &ctx)?,
            l2_depth: req(a, "l2_depth", &ctx)?,
            crossed_tolerance: opt(a, "crossed_tolerance", &ctx)?,
            venue_num_id,
            tick_str: tick_str.to_string(),
            lot_str: lot_str.to_string(),
            min_qty_str: min_qty_str.to_string(),
            funding_interval_s: opt(a, "funding_interval_s", &ctx)?,
            funding_anchor_s: opt(a, "funding_anchor_s", &ctx)?,
            max_leverage: opt(a, "max_leverage", &ctx)?,
            listed_utc_ns: opt(a, "listed_utc_ns", &ctx)?,
            delivery_utc_ns: opt(a, "delivery_utc_ns", &ctx)?,
            price_band: PriceBand {
                up: Rate(opt(a, "price_band_up", &ctx)?),
                down: Rate(opt(a, "price_band_down", &ctx)?),
            },
        };
        n.insts.insert((venue.0, symbol.to_string()), id);
        m.mmr.extend(tiers);
        m.insts.push(spec);
    }
    m.insts.sort_by_key(|i| i.id.0);
    m.mmr.sort_by_key(|x| x.inst.0);
    Ok(())
}

/// `index.json` of a schema directory: `(kind_id, name)` of every entry.
pub fn schema_kinds(dir: &Path) -> Res<Vec<(u16, String)>> {
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
            let k = e.get("kind_id").and_then(serde_json::Value::as_u64);
            let n = e.get("name").and_then(serde_json::Value::as_str);
            match (k.and_then(|k| u16::try_from(k).ok()), n) {
                (Some(k), Some(n)) => Ok((k, n.to_string())),
                _ => input(format!("{}: entry without kind_id/name", p.display())),
            }
        })
        .collect()
}

fn parse_instances(root: &Table, m: &mut Model, n: &mut Names, o: &RefdataOpts<'_>) -> Res<()> {
    let mut kinds: Option<Vec<(u16, String)>> = None;
    for (name, a) in entries(root, "instances", "instance", "name")? {
        let ctx = format!("instances.{name}");
        check_keys(&a, &["id", "kind", "kind_id"], &ctx)?;
        let kind_id: u16 = match (opt_int(&a, "kind_id", &ctx)?, opt_str(&a, "kind", &ctx)?) {
            (Some(k), None) => int_as(k, "kind_id", &ctx)?,
            (None, Some(kname)) => {
                if kinds.is_none() {
                    let dir = o.schemas.map_or_else(
                        || {
                            input(format!(
                                "{ctx}: `kind = \"{kname}\"` needs --schemas <dir> (or write kind_id)"
                            ))
                        },
                        Ok,
                    )?;
                    kinds = Some(schema_kinds(dir)?);
                }
                kinds.iter().flatten().find(|(_, nm)| nm == kname).map_or_else(
                    || rej(format!("{ctx}.kind: `{kname}` is not in the schema index.json")),
                    |x| Ok(x.0),
                )?
            }
            _ => return rej(format!("{ctx}: exactly one of `kind` and `kind_id`")),
        };
        let id = StrategyId(req(&a, "id", &ctx)?);
        n.instances.insert(name.clone(), id);
        m.instances.push(InstanceSpec {
            id,
            kind_id,
            name: name.as_str().into(),
        });
    }
    m.instances.sort_by_key(|i| i.id.0);
    Ok(())
}

fn admins(o: &RefdataOpts<'_>) -> Res<Vec<String>> {
    let p = o.admins.unwrap_or(Path::new(".github/hft-admins.txt"));
    Ok(read_text(p)?
        .lines()
        .map(|l| l.split('#').next().unwrap_or("").trim().to_string())
        .filter(|l| !l.is_empty())
        .collect())
}

/// `[[sim.latency_models]]` and `[[sim.margin_models]]` counts of an hft-core config.
fn sim_model_counts(p: &Path) -> Res<(usize, usize)> {
    let t = parse_toml(&read_text(p)?, p)?;
    let sim = t.get("sim").and_then(Value::as_table);
    let count = |k: &str| {
        sim.and_then(|s| s.get(k))
            .and_then(Value::as_array)
            .map_or(0, Vec::len)
    };
    Ok((count("latency_models"), count("margin_models")))
}

fn parse_accounts(root: &Table, m: &mut Model, n: &mut Names, o: &RefdataOpts<'_>) -> Res<()> {
    let models = o.sim_models.map(sim_model_counts).transpose()?;
    let mut admin_list: Option<Vec<String>> = None;
    for (label, a) in entries(root, "accounts", "account", "label")? {
        let ctx = format!("accounts.{label}");
        check_keys(
            &a,
            &[
                "id",
                "kind",
                "is_prod",
                "uid_policy",
                "uid_policy_signoff",
                "bound_instance",
                "venues",
                "uids",
                "sim",
            ],
            &ctx,
        )?;
        let id = AccountId(req(&a, "id", &ctx)?);
        let sim = match req_str(&a, "kind", &ctx)? {
            "live" => false,
            "sim" => true,
            other => return rej(format!("{ctx}.kind: \"{other}\" is not one of \"live\", \"sim\"")),
        };
        // AccountNamePrefix (apps/hft-core; pre-checked here).
        if sim != label.starts_with("sim-") {
            return rej(format!(
                "{ctx}: AccountNamePrefix: {} account labels {} start with \"sim-\"",
                if sim { "Sim" } else { "Live" },
                if sim { "must" } else { "must not" }
            ));
        }
        let uid_policy = match opt_str(&a, "uid_policy", &ctx)? {
            None => UidPolicy::Verified,
            Some("verified") => UidPolicy::Verified,
            Some("unverifiable") => UidPolicy::Unverifiable,
            Some(other) => {
                return rej(format!(
                    "{ctx}.uid_policy: \"{other}\" is not one of \"verified\", \"unverifiable\" (lower case)"
                ));
            }
        };
        let is_prod = opt_bool(&a, "is_prod", &ctx)?.unwrap_or(false);
        if uid_policy == UidPolicy::Unverifiable && !sim && !is_prod {
            let who = opt_str(&a, "uid_policy_signoff", &ctx)?.map_or_else(
                || {
                    rej(format!(
                        "{ctx}: uid_policy = \"unverifiable\" needs uid_policy_signoff (an Admin login)"
                    ))
                },
                Ok,
            )?;
            if admin_list.is_none() {
                admin_list = Some(admins(o)?);
            }
            if !admin_list.iter().flatten().any(|x| x == who) {
                return rej(format!(
                    "{ctx}.uid_policy_signoff: `{who}` is not in the Admin list (.github/hft-admins.txt)"
                ));
            }
            m.signoffs.push((label.clone(), who.to_string()));
        }
        let bound_instance = match opt_str(&a, "bound_instance", &ctx)? {
            None | Some("") => StrategyId(0),
            Some(name) => n.instances.get(name).copied().map_or_else(
                || rej(format!("{ctx}.bound_instance: DanglingRef instance `{name}`")),
                Ok,
            )?,
        };
        let mut venues = 0u64;
        for v in strings(&a, "venues", &ctx)? {
            let id = venue_or_rej(&v, &format!("{ctx}.venues"))?;
            venues |= 1u64 << (id.0 & 63);
        }
        if let Some(u) = opt_table(&a, "uids", &ctx)? {
            let mut rows = Vec::new();
            for (k, v) in u {
                let venue = venue_or_rej(k, &format!("{ctx}.uids"))?;
                let uid = v
                    .as_str()
                    .map_or_else(|| rej(format!("{ctx}.uids.{k}: expected a string")), Ok)?;
                rows.push(AccountVenueSpec {
                    account: id,
                    venue,
                    uid: uid.to_string(),
                });
            }
            rows.sort_by_key(|r| r.venue.0);
            m.account_venues.extend(rows);
        }
        if let Some(s) = opt_table(&a, "sim", &ctx)? {
            let c = format!("{ctx}.sim");
            check_keys(s, &["fee_tier", "latency_model", "margin_model", "balances"], &c)?;
            let spec = SimSpec {
                account: id,
                fee_tier: req(s, "fee_tier", &c)?,
                latency_model: req(s, "latency_model", &c)?,
                margin_model: req(s, "margin_model", &c)?,
            };
            if let Some((lat, mar)) = models {
                if usize::from(spec.latency_model) >= lat {
                    return rej(format!(
                        "{c}.latency_model = {}: --sim-models has {lat} latency models",
                        spec.latency_model
                    ));
                }
                if usize::from(spec.margin_model) >= mar {
                    return rej(format!(
                        "{c}.margin_model = {}: --sim-models has {mar} margin models",
                        spec.margin_model
                    ));
                }
            }
            m.sims.push(spec);
            for (i, b) in opt_array(s, "balances", &c)?.iter().enumerate() {
                let bc = format!("{c}.balances[{i}]");
                let b = as_table(b, &bc)?;
                check_keys(b, &["venue", "asset", "qty", "qty_raw"], &bc)?;
                let (asset, exp) = n.asset(req_str(b, "asset", &bc)?, &bc)?;
                m.sim_balances.push(SimBalanceSpec {
                    account: id,
                    venue: venue_or_rej(req_str(b, "venue", &bc)?, &bc)?,
                    asset,
                    qty: Qty(raw_or_decimal(b, "qty", exp.qty_exp, &bc, true)?.unwrap_or(0)),
                });
            }
        }
        n.accounts.insert(label.clone(), id);
        m.accounts.push(AccountSpec {
            id,
            sim,
            bound_instance,
            is_prod,
            venues,
            label,
            uid_policy,
        });
    }
    m.accounts.sort_by_key(|a| a.id.0);
    m.account_venues.sort_by_key(|r| r.account.0);
    m.sims.sort_by_key(|r| r.account.0);
    m.sim_balances.sort_by_key(|r| r.account.0);
    Ok(())
}

fn parse_tables(root: &Table, m: &mut Model, n: &Names) -> Res<()> {
    for (i, f) in opt_array(root, "fees", "refdata")?.iter().enumerate() {
        let c = format!("fees[{i}]");
        let f = as_table(f, &c)?;
        check_keys(f, &["venue", "tier", "maker", "taker"], &c)?;
        m.fees.push(FeeSpec {
            venue: venue_or_rej(req_str(f, "venue", &c)?, &c)?,
            tier: req(f, "tier", &c)?,
            fee: FeePair {
                maker: rate(f, "maker", &c)?,
                taker: rate(f, "taker", &c)?,
            },
        });
    }
    for (i, f) in opt_array(root, "fee_overrides", "refdata")?.iter().enumerate() {
        let c = format!("fee_overrides[{i}]");
        let f = as_table(f, &c)?;
        check_keys(f, &["account", "venue", "inst", "maker", "taker"], &c)?;
        let label = req_str(f, "account", &c)?;
        let account = n
            .accounts
            .get(label)
            .copied()
            .map_or_else(|| rej(format!("{c}: DanglingRef account `{label}`")), Ok)?;
        let venue = venue_or_rej(req_str(f, "venue", &c)?, &c)?;
        m.fee_overrides.push(FeeOverrideSpec {
            account,
            venue,
            inst: n.inst(venue, opt_str(f, "inst", &c)?.unwrap_or(""), &c)?,
            fee: FeePair {
                maker: rate(f, "maker", &c)?,
                taker: rate(f, "taker", &c)?,
            },
        });
    }
    for (i, a) in opt_array(root, "aliases", "refdata")?.iter().enumerate() {
        let c = format!("aliases[{i}]");
        let a = as_table(a, &c)?;
        check_keys(a, &["venue", "alias", "inst"], &c)?;
        let venue = venue_or_rej(req_str(a, "venue", &c)?, &c)?;
        m.aliases.push(AliasSpec {
            venue,
            alias: req_str(a, "alias", &c)?.to_string(),
            inst: n.inst(venue, req_str(a, "inst", &c)?, &c)?,
        });
    }
    for (i, w) in opt_array(root, "maint", "refdata")?.iter().enumerate() {
        let c = format!("maint[{i}]");
        let w = as_table(w, &c)?;
        check_keys(w, &["venue", "inst", "start_ns", "end_ns", "kind"], &c)?;
        let venue = venue_or_rej(req_str(w, "venue", &c)?, &c)?;
        m.maint.push(MaintWindow {
            venue,
            inst: n.inst(venue, opt_str(w, "inst", &c)?.unwrap_or(""), &c)?,
            start_ns: req(w, "start_ns", &c)?,
            end_ns: req(w, "end_ns", &c)?,
            kind: ev(MaintKind::ALL, MaintKind::name, w, "kind", &c)?,
        });
    }
    for (i, t) in opt_array(root, "tokens", "refdata")?.iter().enumerate() {
        let c = format!("tokens[{i}]");
        let t = as_table(t, &c)?;
        check_keys(t, &["chain", "asset", "address", "decimals"], &c)?;
        let hex = req_str(t, "address", &c)?;
        let mut address = [0u8; 32];
        if hex.len() != 64 || !hex.is_ascii() {
            return rej(format!("{c}.address: expected 64 hex digits"));
        }
        for (k, b) in address.iter_mut().enumerate() {
            *b = u8::from_str_radix(hex.get(2 * k..2 * k + 2).unwrap_or("zz"), 16)
                .or_else(|_| rej(format!("{c}.address: not hex")))?;
        }
        m.tokens.push(ChainToken {
            chain: venue_or_rej(req_str(t, "chain", &c)?, &c)?,
            asset: n.asset(req_str(t, "asset", &c)?, &c)?.0,
            address,
            decimals: req(t, "decimals", &c)?,
        });
    }
    Ok(())
}
