//! Readable TOML dump of a loaded blob and a two-version diff, for review (feature `tools`).
//! Enum values are written as lower-case snake_case names, like the hft-cfg TOML.

use toml::{Table, Value};

use crate::RefData;
use crate::caps::{FeeAssetRule, MarginData};
use crate::types::AccountKind;
use crate::validate::raw_tables;
use hft_types::VenueId;

/// One difference: dotted path, old value, new value (`""` when absent on that side).
#[derive(Clone, Eq, PartialEq, Debug)]
pub struct DiffItem {
    pub path: String,
    pub old: String,
    pub new: String,
}

fn snake(name: &str) -> String {
    let mut out = String::with_capacity(name.len() + 4);
    for (i, ch) in name.chars().enumerate() {
        if ch.is_ascii_uppercase() {
            if i > 0 {
                out.push('_');
            }
            out.push(ch.to_ascii_lowercase());
        } else {
            out.push(ch);
        }
    }
    out
}

fn venue_name(v: VenueId) -> String {
    v.name().map_or_else(|| format!("venue_{}", v.0), str::to_owned)
}

fn int(x: impl Into<i64>) -> Value {
    Value::Integer(x.into())
}

fn s(x: &str) -> Value {
    Value::String(x.to_owned())
}

fn to_table(rd: &RefData) -> Table {
    let mut root = Table::new();
    root.insert("version".into(), int(rd.version() as i64));
    root.insert("created_utc_ns".into(), int(rd.created_utc_ns()));

    let mut assets = Table::new();
    for a in rd.assets.iter().skip(1) {
        let mut t = Table::new();
        t.insert("id".into(), int(a.id.0));
        t.insert("class".into(), s(&snake(a.class.name())));
        t.insert("px_exp".into(), int(a.exp.px_exp));
        t.insert("qty_exp".into(), int(a.exp.qty_exp));
        t.insert("px_max_raw".into(), int(a.px_max.0));
        t.insert("qty_max_raw".into(), int(a.qty_max.0));
        t.insert("dust_raw".into(), int(a.dust.0));
        assets.insert(a.name.to_string(), Value::Table(t));
    }
    root.insert("assets".into(), Value::Table(assets));

    let mut venues = Table::new();
    let mut caps = Table::new();
    for &v in rd.venues_in_order() {
        let m = rd.venue(v);
        let mut t = Table::new();
        t.insert("id".into(), int(v.0));
        t.insert("exchange".into(), s(&snake(m.exchange.name())));
        t.insert("product".into(), s(&snake(m.product.name())));
        t.insert("ws_max_streams_per_conn".into(), int(m.ws_max_streams_per_conn));
        t.insert("ws_max_in_msgs_per_s".into(), int(m.ws_max_in_msgs_per_s));
        t.insert("ws_max_conn_age_s".into(), int(m.ws_max_conn_age_s));
        t.insert("ws_new_conn_per_5min_ip".into(), int(m.ws_new_conn_per_5min_ip));
        t.insert("app_ping_interval_s".into(), int(m.app_ping_interval_s));
        t.insert("ts_unit".into(), s(&snake(m.ts_unit.name())));
        t.insert("client_id_max_len".into(), int(m.client_id_max_len));
        t.insert("client_id_prefix".into(), s(&m.client_id_prefix));
        t.insert("max_batch".into(), int(m.max_batch));
        t.insert("max_inflight_amend".into(), int(m.max_inflight_amend));
        t.insert(
            "crossed_tolerance_default".into(),
            int(m.crossed_tolerance_default),
        );
        t.insert("stale_guard".into(), s(&snake(m.stale_guard.name())));
        let mut limits = Vec::new();
        for l in rd.rate_limits(v) {
            let mut lt = Table::new();
            lt.insert("scope".into(), s(&snake(l.scope.name())));
            lt.insert("bucket".into(), s(&snake(l.bucket.name())));
            lt.insert("limit".into(), int(l.limit));
            lt.insert("window_ms".into(), int(l.window_ms));
            lt.insert("default_cost".into(), int(l.default_cost));
            limits.push(Value::Table(lt));
        }
        t.insert("rate_limits".into(), Value::Array(limits));
        let mut eps = Vec::new();
        for env in crate::types::EndpointEnv::ALL {
            for e in rd.endpoints(v, *env) {
                let mut et = Table::new();
                et.insert("role".into(), s(&snake(e.role.name())));
                et.insert("env".into(), s(&snake(e.env.name())));
                et.insert("url".into(), s(&e.url));
                et.insert("conn_group".into(), int(e.conn_group));
                eps.push(Value::Table(et));
            }
        }
        t.insert("endpoints".into(), Value::Array(eps));
        venues.insert(venue_name(v), Value::Table(t));
        if rd.caps_row(v).is_some() {
            let c = rd.caps(v);
            let mut ct = Table::new();
            ct.insert("market".into(), s(&snake(&format!("{:?}", c.market))));
            let (rule, asset) = match c.fee_asset_rule {
                FeeAssetRule::Quote => ("quote", 0),
                FeeAssetRule::ReceivedAsset => ("received_asset", 0),
                FeeAssetRule::ReceivedAssetUnlessDiscount { token } => {
                    ("received_asset_unless_discount", token.0)
                }
                FeeAssetRule::Configured(a) => ("configured", a.0),
            };
            ct.insert("fee_asset_rule".into(), s(rule));
            if asset != 0 {
                ct.insert("fee_asset".into(), s(&rd.asset(hft_types::AssetId(asset)).name));
            }
            ct.insert("reduce_only".into(), Value::Boolean(c.reduce_only));
            ct.insert(
                "required_position_mode".into(),
                s(&snake(c.required_position_mode.name())),
            );
            ct.insert(
                "required_margin_mode".into(),
                s(&snake(c.required_margin_mode.name())),
            );
            let ob = |x: Option<bool>| {
                s(match x {
                    None => "none",
                    Some(false) => "false",
                    Some(true) => "true",
                })
            };
            ct.insert("required_multi_asset".into(), ob(c.required_multi_asset));
            ct.insert("required_spot_borrow".into(), ob(c.required_spot_borrow));
            ct.insert("funding_settle".into(), s(&snake(c.funding_settle.name())));
            let (md, poll) = match c.margin_data {
                MarginData::Push => ("push", 0),
                MarginData::Poll { interval_ms } => ("poll", interval_ms),
                MarginData::NotApplicable => ("not_applicable", 0),
            };
            ct.insert("margin_data".into(), s(md));
            if poll != 0 {
                ct.insert("margin_poll_ms".into(), int(poll));
            }
            ct.insert(
                "own_liquidation_marker".into(),
                s(&snake(c.own_liquidation_marker.name())),
            );
            ct.insert("liq_marker_arg".into(), s(&c.liq_marker_arg));
            ct.insert(
                "ignorable_user_events".into(),
                Value::Array(c.ignorable_user_events.iter().map(|e| s(e)).collect()),
            );
            ct.insert("flags".into(), int(c.flags.0));
            ct.insert(
                "amend_qty_down_only".into(),
                Value::Boolean(c.amend_qty_down_only),
            );
            ct.insert("dms".into(), s(&snake(c.dms.name())));
            ct.insert("dms_min_ms".into(), int(c.dms_min_ms));
            ct.insert("dms_max_ms".into(), int(c.dms_max_ms));
            ct.insert("dms_timeout_ms".into(), int(c.dms_timeout_ms));
            ct.insert("dms_refresh_ms".into(), int(c.dms_refresh_ms));
            ct.insert("testnet".into(), s(&snake(c.testnet.name())));
            caps.insert(venue_name(v), Value::Table(ct));
        }
    }
    root.insert("venues".into(), Value::Table(venues));
    root.insert("caps".into(), Value::Table(caps));

    let mut insts = Table::new();
    for (h, m) in rd.insts.iter().zip(rd.inst_meta.iter()).skip(1) {
        let mut t = Table::new();
        t.insert("id".into(), int(m.id.0));
        t.insert("status".into(), s(&snake(h.status.name())));
        t.insert("kind".into(), s(&snake(h.kind.name())));
        t.insert("flags".into(), int(h.flags));
        t.insert("asset".into(), s(&rd.asset(h.asset).name));
        t.insert("quote".into(), s(&rd.asset(h.quote).name));
        t.insert("px_unit_shift".into(), int(m.px_unit_shift));
        t.insert("qty_unit_shift".into(), int(m.qty_unit_shift));
        t.insert("tick".into(), s(&m.tick_str));
        t.insert("lot".into(), s(&m.lot_str));
        t.insert("min_qty".into(), s(&m.min_qty_str));
        t.insert("tick_raw".into(), int(h.tick));
        t.insert("lot_raw".into(), int(h.lot));
        t.insert("min_qty_raw".into(), int(h.min_qty.0));
        t.insert("max_qty_raw".into(), int(h.max_qty.0));
        t.insert("min_notional_raw".into(), int(h.min_notional.0));
        t.insert("book_window".into(), int(h.book_window));
        t.insert("l2_depth".into(), int(h.l2_depth));
        t.insert("crossed_tolerance".into(), int(h.crossed_tolerance));
        t.insert("venue_num_id".into(), s(&m.venue_num_id.to_string()));
        t.insert("funding_interval_s".into(), int(m.funding_interval_s));
        t.insert("funding_anchor_s".into(), int(m.funding_anchor_s));
        t.insert("max_leverage".into(), int(m.max_leverage));
        t.insert("listed_utc_ns".into(), int(m.listed_utc_ns));
        t.insert("delivery_utc_ns".into(), int(m.delivery_utc_ns));
        t.insert("price_band_up".into(), int(m.price_band.up.0));
        t.insert("price_band_down".into(), int(m.price_band.down.0));
        let tiers = m
            .mmr_tiers
            .iter()
            .map(|x| Value::Array(vec![int(x.notional_max.0), int(x.mmr.0)]))
            .collect();
        t.insert("mmr_tiers".into(), Value::Array(tiers));
        insts.insert(format!("{}:{}", venue_name(h.venue), m.symbol), Value::Table(t));
    }
    root.insert("insts".into(), Value::Table(insts));

    let mut instances = Table::new();
    for i in rd.instances_in_order() {
        let mut t = Table::new();
        t.insert("id".into(), int(i.id.0));
        t.insert("kind_id".into(), int(i.kind_id));
        instances.insert(i.name.to_string(), Value::Table(t));
    }
    root.insert("instances".into(), Value::Table(instances));

    let mut accounts = Table::new();
    for a in rd.accounts.iter().skip(1) {
        let mut t = Table::new();
        t.insert("id".into(), int(a.id.0));
        t.insert(
            "kind".into(),
            s(if matches!(a.kind, AccountKind::Live) {
                "live"
            } else {
                "sim"
            }),
        );
        t.insert("is_prod".into(), Value::Boolean(a.is_prod));
        t.insert("uid_policy".into(), s(&snake(a.uid_policy.name())));
        let bound = rd
            .instance(a.bound_instance)
            .map_or_else(String::new, |i| i.name.to_string());
        t.insert("bound_instance".into(), s(&bound));
        let venues = (0..64u8)
            .filter(|v| a.venues & (1u64 << v) != 0)
            .map(|v| s(&venue_name(VenueId(v))))
            .collect();
        t.insert("venues".into(), Value::Array(venues));
        let mut uids = Table::new();
        for (acc, v, uid) in rd.account_venues() {
            if *acc == a.id {
                uids.insert(venue_name(*v), s(uid));
            }
        }
        t.insert("uids".into(), Value::Table(uids));
        if let AccountKind::Sim(cfg) = &a.kind {
            let mut st = Table::new();
            st.insert("fee_tier".into(), int(cfg.fee_tier));
            st.insert("latency_model".into(), int(cfg.latency_model));
            st.insert("margin_model".into(), int(cfg.margin_model));
            let bal = cfg
                .initial_balances
                .iter()
                .map(|b| {
                    let mut bt = Table::new();
                    bt.insert("venue".into(), s(&venue_name(b.venue)));
                    bt.insert("asset".into(), s(&rd.asset(b.asset).name));
                    bt.insert("qty_raw".into(), int(b.qty.0));
                    Value::Table(bt)
                })
                .collect();
            st.insert("balances".into(), Value::Array(bal));
            t.insert("sim".into(), Value::Table(st));
        }
        accounts.insert(a.label.to_string(), Value::Table(t));
    }
    root.insert("accounts".into(), Value::Table(accounts));

    if let Some(raw) = raw_tables(rd.blob()) {
        let fees = raw
            .fees
            .iter()
            .map(|f| {
                let mut t = Table::new();
                t.insert("venue".into(), s(&venue_name(VenueId(f.venue))));
                t.insert("tier".into(), int(f.tier));
                t.insert("maker".into(), int(f.maker));
                t.insert("taker".into(), int(f.taker));
                Value::Table(t)
            })
            .collect();
        root.insert("fees".into(), Value::Array(fees));
        let ovs = raw
            .fee_overrides
            .iter()
            .map(|f| {
                let mut t = Table::new();
                let label = rd.account(hft_types::AccountId(f.account)).label.to_string();
                t.insert("account".into(), s(&label));
                t.insert("venue".into(), s(&venue_name(VenueId(f.venue))));
                t.insert("inst".into(), s(&rd.inst_meta(hft_types::InstId(f.inst)).symbol));
                t.insert("maker".into(), int(f.maker));
                t.insert("taker".into(), int(f.taker));
                Value::Table(t)
            })
            .collect();
        root.insert("fee_overrides".into(), Value::Array(ovs));
        let aliases = raw
            .aliases
            .iter()
            .map(|a| {
                let mut t = Table::new();
                t.insert("venue".into(), s(&venue_name(VenueId(a.venue))));
                t.insert("alias".into(), s(raw.text(a.alias)));
                t.insert("inst".into(), s(&rd.inst_meta(hft_types::InstId(a.inst)).symbol));
                Value::Table(t)
            })
            .collect();
        root.insert("aliases".into(), Value::Array(aliases));
    }
    let maint = rd
        .maint
        .iter()
        .flat_map(|ws| ws.iter())
        .map(|w| {
            let mut t = Table::new();
            t.insert("venue".into(), s(&venue_name(w.venue)));
            t.insert("inst".into(), s(&rd.inst_meta(w.inst).symbol));
            t.insert("start_ns".into(), int(w.start_ns));
            t.insert("end_ns".into(), int(w.end_ns));
            t.insert("kind".into(), s(&snake(w.kind.name())));
            Value::Table(t)
        })
        .collect();
    root.insert("maint".into(), Value::Array(maint));
    let tokens = rd
        .tokens
        .iter()
        .map(|tk| {
            let mut t = Table::new();
            t.insert("chain".into(), s(&venue_name(tk.chain)));
            t.insert("asset".into(), s(&rd.asset(tk.asset).name));
            let hex: String = tk.address.iter().map(|b| format!("{b:02x}")).collect();
            t.insert("address".into(), s(&hex));
            t.insert("decimals".into(), int(tk.decimals));
            Value::Table(t)
        })
        .collect();
    root.insert("tokens".into(), Value::Array(tokens));
    root
}

/// Readable TOML of a loaded blob; deterministic (tables are key-ordered).
pub fn dump(rd: &RefData) -> String {
    toml::to_string(&to_table(rd)).unwrap_or_default()
}

fn flatten(prefix: &str, t: &Table, out: &mut Vec<(String, String)>) {
    for (k, v) in t {
        let path = if prefix.is_empty() {
            k.clone()
        } else {
            format!("{prefix}.{k}")
        };
        match v {
            Value::Table(sub) => flatten(&path, sub, out),
            Value::String(x) => out.push((path, x.clone())),
            other => out.push((path, other.to_string())),
        }
    }
}

/// Leaf-level differences between two loaded blobs, ordered by path.
pub fn diff(a: &RefData, b: &RefData) -> Vec<DiffItem> {
    let (mut fa, mut fb) = (Vec::new(), Vec::new());
    flatten("", &to_table(a), &mut fa);
    flatten("", &to_table(b), &mut fb);
    fa.sort();
    fb.sort();
    let (mut i, mut j) = (0, 0);
    let mut out = Vec::new();
    loop {
        match (fa.get(i), fb.get(j)) {
            (None, None) => break,
            (Some((pa, va)), Some((pb, vb))) if pa == pb => {
                if va != vb {
                    out.push(DiffItem {
                        path: pa.clone(),
                        old: va.clone(),
                        new: vb.clone(),
                    });
                }
                i += 1;
                j += 1;
            }
            (Some((pa, va)), Some((pb, _))) if pa < pb => {
                out.push(DiffItem {
                    path: pa.clone(),
                    old: va.clone(),
                    new: String::new(),
                });
                i += 1;
            }
            (Some((pa, va)), None) => {
                out.push(DiffItem {
                    path: pa.clone(),
                    old: va.clone(),
                    new: String::new(),
                });
                i += 1;
            }
            (_, Some((pb, vb))) => {
                out.push(DiffItem {
                    path: pb.clone(),
                    old: String::new(),
                    new: vb.clone(),
                });
                j += 1;
            }
        }
    }
    out
}
