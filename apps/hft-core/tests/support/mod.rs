//! Configuration fixture shared with the offline configuration example.
use hft_core::{
    Strategies,
    config::{digest, hex},
};
use hft_event::{GroupSpec, LegDef, ParamSource, StrategyParamsHdr};
use hft_refdata::*;
use hft_strategy_sdk::{
    StrategySet, VenueSet,
    params::{RawParams, SchemaView, encode_body},
};
use hft_testkit::seq_rig::{RigConfig, engine_setup};
use hft_types::*;
use std::path::{Path, PathBuf};
use zerocopy::IntoBytes;

#[allow(dead_code)]
pub fn write(root: &Path, hot: &Path, user: Option<&str>) -> PathBuf {
    write_for_kind(root, hot, user, None)
}

#[allow(dead_code)]
pub fn write_for_kind(root: &Path, hot: &Path, user: Option<&str>, kind: Option<u16>) -> PathBuf {
    let params =
        kind.map(|id| toml::from_str::<toml::Value>(&format!("[strategy.schema]\nid={id}\n")).unwrap());
    write_mode(root, hot, user, params.as_ref(), false, false, &[])
}

#[allow(dead_code)]
pub fn write_live(root: &Path, hot: &Path, is_prod: bool) -> PathBuf {
    write_mode(root, hot, None, None, true, is_prod, &[])
}

#[allow(dead_code)]
pub fn write_live_endpoints(root: &Path, rows: &[(EndpointEnv, EndpointRole, &str)]) -> PathBuf {
    write_mode(root, &root.join("hot"), None, None, true, false, rows)
}

fn write_mode(
    root: &Path,
    hot: &Path,
    user: Option<&str>,
    params: Option<&toml::Value>,
    live: bool,
    is_prod: bool,
    endpoints: &[(EndpointEnv, EndpointRole, &str)],
) -> PathBuf {
    std::fs::create_dir_all(root).unwrap();
    let second = params.is_some();
    let perp_spot = params.is_some_and(|p| p.get("groups").is_some());
    let base = params
        .and_then(|p| p.get("groups"))
        .and_then(toml::Value::as_array)
        .and_then(|groups| groups.first())
        .and_then(|group| group.get("asset"))
        .and_then(toml::Value::as_str)
        .unwrap_or("BTC");
    let symbol = format!("{base}USDT");
    let v = if perp_spot {
        VenueId::BINANCE_USDM
    } else {
        VenueId::BINANCE_SPOT
    };
    let a = AccountId(1);
    let btc = AssetId(1);
    let usdt = AssetId(2);
    let desc = Strategies::kinds()
        .iter()
        .find(|d| {
            params.map_or_else(
                || d.non_prod_only && d.schema.fields.iter().any(|f| f.name == "probe_period_ms"),
                |p| i64::from(d.kind_id) == p["strategy"]["schema"]["id"].as_integer().unwrap(),
            )
        })
        .unwrap();
    let mut b = RdbBuilder::new(1);
    for (id, name, class, exp) in [
        (
            btc,
            base,
            AssetClass::Crypto,
            AssetExp {
                px_exp: -4,
                qty_exp: -8,
            },
        ),
        (
            usdt,
            "USDT",
            AssetClass::Stable,
            AssetExp {
                px_exp: -6,
                qty_exp: -6,
            },
        ),
        (
            AssetId(3),
            "BNB",
            AssetClass::Crypto,
            AssetExp {
                px_exp: -4,
                qty_exp: -8,
            },
        ),
    ] {
        b.asset(AssetSpec {
            id,
            name: name.into(),
            class,
            exp,
            px_max: Px(1_000_000_000_000),
            qty_max: Qty(1_000_000_000_000_000),
            dust: Qty(1),
        });
    }
    if second {
        b.asset(AssetSpec {
            id: AssetId(4),
            name: "BGB".into(),
            class: AssetClass::Crypto,
            exp: AssetExp {
                px_exp: -4,
                qty_exp: -8,
            },
            px_max: Px(1_000_000_000_000),
            qty_max: Qty(1_000_000_000_000_000),
            dust: Qty(1),
        });
    }
    b.venue(VenueSpec {
        id: v,
        exchange: Exchange::Binance,
        product: if perp_spot { InstKind::Perp } else { InstKind::Spot },
        ws_max_streams_per_conn: 1024,
        ws_max_in_msgs_per_s: 5,
        ws_max_conn_age_s: 86400,
        ws_new_conn_per_5min_ip: 300,
        app_ping_interval_s: 0,
        ts_unit: TsUnit::Us,
        client_id_max_len: 36,
        client_id_prefix: "".into(),
        max_batch: 5,
        max_inflight_amend: 1,
        crossed_tolerance_default: 0,
        stale_guard: StaleGuardKind::RecvWindow,
    });
    for (env, role, url) in endpoints {
        b.endpoint(
            v,
            Endpoint {
                env: *env,
                role: *role,
                url: (*url).into(),
                conn_group: 0,
            },
        );
    }
    use hft_venue_proto::VenueProtocol;
    let asset = |s: &str| match s {
        "USDT" => Some(usdt),
        "BNB" => Some(AssetId(3)),
        _ => None,
    };
    b.caps(if perp_spot {
        venue_binance::Usdm::default_caps(asset)
    } else {
        venue_binance::Spot::default_caps(asset)
    });
    b.fee(FeeSpec {
        venue: v,
        tier: 0,
        fee: FeePair {
            maker: Rate(1_000_000),
            taker: Rate(1_000_000),
        },
    });
    b.limit(
        v,
        RateLimit {
            scope: LimitScope::Account,
            bucket: LimitBucket::Orders,
            limit: 100,
            window_ms: 1000,
            default_cost: 1,
        },
    );
    b.inst(InstSpec {
        id: InstId(1),
        venue: v,
        symbol: symbol.clone(),
        kind: if perp_spot { InstKind::Perp } else { InstKind::Spot },
        asset: btc,
        quote: usdt,
        tick: 100,
        lot: 1000,
        min_qty: Qty(1000),
        max_qty: Qty(100_000_000_000),
        min_notional: Notional(5_000_000),
        book_window: 4096,
        tick_str: "0.01".into(),
        lot_str: "0.00001".into(),
        min_qty_str: "0.00001".into(),
        ..InstSpec::default()
    });
    let other = VenueId::BITGET_UTA_SPOT;
    #[cfg(feature = "bitget")]
    if second {
        let v = other;
        b.venue(VenueSpec {
            id: v,
            exchange: Exchange::Bitget,
            product: InstKind::Spot,
            ws_max_streams_per_conn: 1024,
            ws_max_in_msgs_per_s: 5,
            ws_max_conn_age_s: 86400,
            ws_new_conn_per_5min_ip: 300,
            app_ping_interval_s: 0,
            ts_unit: TsUnit::Us,
            client_id_max_len: 36,
            client_id_prefix: "".into(),
            max_batch: 5,
            max_inflight_amend: 1,
            crossed_tolerance_default: 0,
            stale_guard: StaleGuardKind::RecvWindow,
        });
        use hft_venue_proto::VenueProtocol;
        b.caps(venue_bitget::BitgetSpot::default_caps(|s| match s {
            "USDT" => Some(usdt),
            "BGB" => Some(AssetId(4)),
            _ => None,
        }));
        b.fee(FeeSpec {
            venue: v,
            tier: 0,
            fee: FeePair {
                maker: Rate(1_000_000),
                taker: Rate(1_000_000),
            },
        });
        b.limit(
            v,
            RateLimit {
                scope: LimitScope::Account,
                bucket: LimitBucket::Orders,
                limit: 100,
                window_ms: 1000,
                default_cost: 1,
            },
        );
        b.inst(InstSpec {
            id: InstId(2),
            venue: v,
            symbol,
            asset: btc,
            quote: usdt,
            tick: 100,
            lot: 1000,
            min_qty: Qty(1000),
            max_qty: Qty(100_000_000_000),
            min_notional: Notional(5_000_000),
            book_window: 4096,
            tick_str: "0.01".into(),
            lot_str: "0.00001".into(),
            min_qty_str: "0.00001".into(),
            ..InstSpec::default()
        });
    }
    b.instance(InstanceSpec {
        id: StrategyId(1),
        kind_id: desc.kind_id,
        name: params
            .and_then(|p| p["strategy"].get("instance"))
            .and_then(toml::Value::as_str)
            .unwrap_or("paper-probe")
            .into(),
    });
    b.account(AccountSpec {
        id: a,
        sim: !live,
        bound_instance: StrategyId(1),
        is_prod,
        venues: (1 << v.0) | if second { 1 << other.0 } else { 0 },
        label: params
            .and_then(|p| p["strategy"].get("account"))
            .and_then(toml::Value::as_str)
            .unwrap_or("paper")
            .into(),
        uid_policy: UidPolicy::Verified,
    });
    if live {
        b.account_venue(AccountVenueSpec {
            account: a,
            venue: v,
            uid: "123".into(),
        });
        if perp_spot {
            b.account_venue(AccountVenueSpec {
                account: a,
                venue: other,
                uid: "456".into(),
            });
        }
    } else {
        b.sim(SimSpec {
            account: a,
            fee_tier: 0,
            latency_model: 0,
            margin_model: 0,
        });
        b.sim_balance(SimBalanceSpec {
            account: a,
            venue: v,
            asset: usdt,
            qty: Qty(100_000_000_000),
        });
        if second {
            b.sim_balance(SimBalanceSpec {
                account: a,
                venue: other,
                asset: usdt,
                qty: Qty(100_000_000_000),
            });
        }
    }
    let rdb = b.finish();
    let rd = RefData::load(&rdb, None).unwrap();
    let schema = desc.schema;
    let mut defaults: Vec<_> = (0..schema.n_fields())
        .map(|i| schema.field(i).unwrap().default)
        .collect();
    for i in 0..schema.n_fields() {
        match schema.field(i).unwrap().name {
            "enabled" => defaults[i as usize] = 1,
            "far_bp" => defaults[i as usize] = 10_000_000,
            "probe_period_ms" => defaults[i as usize] = 100,
            _ => {}
        }
    }
    if let Some(table) = params
        .and_then(|p| p.get("defaults"))
        .and_then(toml::Value::as_table)
    {
        for (name, value) in table {
            let ix = hft_strategy_sdk::params::field_ix(schema, name).unwrap();
            let text = match value {
                toml::Value::String(s) => s.clone(),
                v => v.to_string(),
            };
            defaults[usize::from(ix)] =
                hft_strategy_sdk::params::parse_value(schema.field(ix).unwrap(), &text, None).unwrap();
        }
    }
    let body = encode_body(
        schema,
        1,
        &RawParams {
            defaults,
            overrides: vec![],
        },
    )
    .unwrap();
    let h = StrategyParamsHdr {
        strategy: StrategyId(1),
        account: a,
        kind_id: desc.kind_id,
        schema_id: schema.id,
        schema_ver: schema.version,
        source: ParamSource::Manual,
        _pad: 0,
        n_groups: 1,
        _pad2: 0,
        version: 1,
        generated_at_ns: 0,
        body_len: body.len() as u32,
        group_ctr_hwm: 1,
        schema_digest: hft_strategy_sdk::params::schema_digest(desc, schema.version),
    };
    let none = LegDef {
        inst: InstId(0),
        venue: VenueId(0),
        kind: 0,
        _pad: 0,
        route: 0,
        _pad2: 0,
    };
    let group = GroupSpec {
        id: GroupId::compose(StrategyId(1), 1).unwrap(),
        asset: btc,
        n_legs: if second { 2 } else { 1 },
        open_allowed: 1,
        legs: [
            LegDef {
                inst: InstId(1),
                venue: v,
                kind: LegDef::CEX,
                ..none
            },
            if second {
                LegDef {
                    inst: InstId(2),
                    venue: other,
                    kind: LegDef::CEX,
                    ..none
                }
            } else {
                none
            },
            none,
            none,
        ],
        _rsv: 0,
    };
    let mut params = h.as_bytes().to_vec();
    params.extend_from_slice(group.as_bytes());
    params.extend(body);
    let mut risk = hft_risk::ParamsBuilder::new(&rd);
    risk.rate_headroom = Rate(100_000_000);
    risk.classes.push(hft_risk::BandSpec {
        max_px_dev: Rate(30_000_000),
        ioc_max_cross: Rate(5_000_000),
        rr_max_cross: Some(Rate(10_000_000)),
    });
    risk.accounts.push((
        a,
        hft_risk::AcctSpec {
            max_order_notional: Notional(if perp_spot { 25_000_000 } else { 100_000_000 }),
            max_account_notional: Notional(1_000_000_000),
            account_notional_from_equity: Rate(1_000_000_000),
            default_group: hft_risk::GroupLimits {
                max_group_notional: Notional(500_000_000),
                max_pos_qty: Qty(10_000_000),
                naked_gate: Rate(500_000_000),
                naked_gate_min: Notional(100_000_000),
            },
            max_hedge_notional: Some(Notional(if perp_spot { 25_000_000 } else { 100_000_000 })),
            max_reduce_notional: Some(Notional(if perp_spot { 25_000_000 } else { 100_000_000 })),
            daily_loss_kill: Rate(100_000_000),
            daily_loss_abs: Notional(100_000_000),
            recon_closeonly_notional: Notional(5_000_000),
            ..hft_risk::AcctSpec::default()
        },
    ));
    risk.venues.push((
        v,
        a,
        hft_risk::VenueSpec {
            max_venue_notional: Notional(1_000_000_000),
            inst_bucket_per_s: 10,
            backoff_base_ms: 500,
            backoff_max_ms: 8000,
            rr_reserve: None,
        },
    ));
    let risk = risk.build();
    let rpb = risk.encode_blob().unwrap();
    let mut ledger = hft_ledger::LedgerConfig::default();
    ledger.stable_par_fallback.clear();
    let rig = RigConfig {
        refdata_blob: rdb.clone(),
        params: vec![params.clone()],
        risk,
        risk_blob: rpb.clone(),
        caps: hft_ledger::Capacities {
            max_insts: 16,
            max_cells_total: 16,
            max_timers: 1024,
            recent_cmd_ids: 256,
            max_subs_per_inst: 4,
            max_groups_per_instance: 4,
            max_instances: 4,
        },
        ledger,
        venues: if second { vec![v, other] } else { vec![v] },
        keys: vec![],
        epoch: 1,
        tsc_hz_milli: 1_000_000_000_000,
        calib_tsc: 0,
        calib_utc_ns: 0,
        boot_tsc: 0,
        max_groups: 4,
    };
    let mut engine = engine_setup(&rig, &rd);
    engine.drain_timeout_ms = 3000;
    engine.gws.venues.clear();
    engine.feeds = VenueSet((1 << v.0) | if second { 1 << other.0 } else { 0 });
    engine.seq.rings[7] = hft_sequencer::RingLimit::default();
    engine.seq.allow_missing_standby = 1;
    if live {
        engine.role_prod = true;
        engine.gws.venues = vec![v];
        engine.keys_required = vec![(a, v, 3)];
        if perp_spot {
            engine.gws.venues.push(other);
            engine.keys_required.push((a, other, 3));
        }
    }
    engine.exec.default.force_slice.max_chunk_notional =
        Notional(if perp_spot { 25_000_000 } else { 100_000_000 });
    let ecf = engine.encode_blob(1, HostClass::Dev2, 0);
    let principals = user.map_or_else(String::new, |u| {
        format!("[uids]\n{u:?} = \"cli\"\n[issuers]\n\"cli:{u}\" = [\"Admin\"]\n")
    });
    std::fs::write(root.join("principals.toml"), principals).unwrap();
    let mut text = format!(
        r#"schema = 1
[instance]
name = "sim"
config_source = "file"
[paths]
journal_ram = '{}'
journal_cold = 'cold'
ctl_socket = 'core.sock'
ctl_principals = 'principals.toml'
[shutdown]
cancel_open_orders = true
drain_timeout_ms = 3000
[journal]
segment_bytes = 8388608
hot_keep = 2
[clock]
step_threshold_ns = 50000
interval_ms = 1000
[watchdog]
hb_stale_ms = 1000
[rings]
elements = 1024
bytes = 1048576
[venues.public]
protocol = 'binance-spot'
id = {}
max_sub_insts = 1
stale_guard_ms = 1500
[sim]
seed = 123
entry_us = 100
resp_us = 100
leverage = 1
mmr_raw = 5000000
liq_fee_raw = 1000000
liq_slippage_bp = 10
quote_interval_ms = 20
[[sim.quotes]]
inst = 1
bid_raw = 600000000
ask_raw = 600001000
qty_raw = 200000000
"#,
        hot.display(),
        v.0
    );
    if second {
        text += &format!(
            "\n[venues.second]\nprotocol = 'bitget-spot'\nid = {}\nmax_sub_insts = 1\nstale_guard_ms = 1500\n\n[[sim.quotes]]\ninst = 2\nbid_raw = 600000000\nask_raw = 600001000\nqty_raw = 200000000\n",
            other.0
        );
    }
    for task in [
        "sequencer",
        "persist",
        "housekeeping",
        "ctl-gateway",
        "sim-gw",
        "paper-feed",
    ] {
        let nice = if matches!(task, "persist" | "ctl-gateway") {
            10
        } else {
            0
        };
        text += &format!("\n[[threads]]\nname = '{task}'\ntask = '{task}'\npark_us = 100\nnice = {nice}\n");
    }
    for (name, bytes) in [
        ("refdata", rdb),
        ("engine", ecf),
        ("risk_params", rpb),
        ("strategy_params", params),
    ] {
        let file = format!("{name}.bin");
        std::fs::write(root.join(&file), &bytes).unwrap();
        let table = if name == "strategy_params" {
            format!("[[blobs.{name}]]")
        } else {
            format!("[blobs.{name}]")
        };
        text += &format!(
            "\n{table}\npath = '{file}'\nsha256 = '{}'\n",
            hex(&digest(&bytes))
        );
    }
    let path = root.join("core.toml");
    if live {
        let mut config: toml::Value = toml::from_str(&text).unwrap();
        config.as_table_mut().unwrap().remove("sim");
        config["instance"]["name"] = "prod".into();
        config["instance"]
            .as_table_mut()
            .unwrap()
            .insert("order_env".into(), "testnet".into());
        let venue = config["venues"]["public"].as_table_mut().unwrap();
        if perp_spot {
            venue.insert("protocol".into(), "binance-usdm".into());
        }
        venue.insert("src_ip".into(), "127.0.0.1".into());
        venue.insert("bind_device".into(), "lo".into());
        let keys: toml::Value = toml::from_str("account=1\napi_key_path='api'\nsession_seed_path='session'\nrest_api_key_path='rest-api'\nrest_secret_path='rest'").unwrap();
        venue.insert("accounts".into(), toml::Value::Array(vec![keys]));
        venue.insert("account_io".into(), toml::from_str("timeout_ms=5000\ninterval_ms=1000\nrequest_gap_ms=100\nmax_cancel_rounds=3\nlisten_keepalive_ms=600000").unwrap());
        let threads = config["threads"].as_array_mut().unwrap();
        threads.retain(|t| !matches!(t["task"].as_str(), Some("sim-gw" | "paper-feed")));
        for (name, task) in [
            ("feed", "feed:public"),
            ("gw", "gw:public"),
            ("conn", "conn-builder:public"),
        ] {
            threads
                .push(toml::from_str(&format!("name='{name}'\ntask='{task}'\npark_us=100\nnice=0")).unwrap());
        }
        if perp_spot {
            let credentials = config["venues"]["public"]["accounts"].clone();
            let io = config["venues"]["public"]["account_io"].clone();
            let venue = config["venues"]["second"].as_table_mut().unwrap();
            venue.insert("src_ip".into(), "127.0.0.1".into());
            venue.insert("bind_device".into(), "lo".into());
            venue.insert("accounts".into(), credentials);
            venue.insert("account_io".into(), io);
            venue.get_mut("accounts").unwrap()[0]
                .as_table_mut()
                .unwrap()
                .insert("passphrase_path".into(), "passphrase".into());
            for (name, task) in [
                ("feed-second", "feed:second"),
                ("gw-second", "gw:second"),
                ("conn-second", "conn-builder:second"),
            ] {
                config["threads"].as_array_mut().unwrap().push(
                    toml::from_str(&format!("name='{name}'\ntask='{task}'\npark_us=100\nnice=0")).unwrap(),
                );
            }
        }
        text = toml::to_string(&config).unwrap();
    }
    std::fs::write(&path, text).unwrap();
    path
}

#[allow(dead_code)]
pub fn write_strategy_testnet(root: &Path, params: &toml::Value, is_prod: bool) -> PathBuf {
    write_mode(root, &root.join("hot"), None, Some(params), true, is_prod, &[])
}

/// Crossed tolerance of the Polymarket venue and slot insts: the book stays Live through at most this
/// many consecutive messages leaving bid ≥ ask, the next one makes it `Stale(Crossed)` until a snapshot.
/// A taker sweep briefly locks the published book (the new level before the match removes it): on
/// the 2026-10-26 live journal 91.5 % of such episodes clear within 1 message, 99.7 % within 3, all
/// by an incremental update; with 0 every sweep made the book Stale and the maker cancel its quotes.
#[allow(dead_code)]
pub const PM_CROSSED_TOLERANCE: u16 = 3;

/// Coins of `write_polymarket_sim`; slot `<coin>-<up|dn>-<k>` for windows k = 0, 1.
#[allow(dead_code)]
pub const PM_COINS: [&str; 4] = ["btc", "eth", "sol", "xrp"];

/// `--dry-run` configuration whose only venue is `polymarket-clob` with `live_feed = true`: 16 slot
/// insts (InstId 1–16 in the order coin, window, up/dn; tick 0.001), Sim account `sim-pm` bound to
/// a book-observer instance with one single-leg group per slot. `user` is the Admin cli principal;
/// `pipeline` (the discovery process's system user) is the pipeline principal that sends `BindSlot`.
#[cfg(feature = "polymarket")]
#[allow(dead_code)]
pub fn write_polymarket_sim(
    root: &Path,
    hot: &Path,
    user: Option<&str>,
    pipeline: Option<&str>,
    src_ip: &str,
    bind_device: &str,
) -> PathBuf {
    write_pm(
        root,
        hot,
        user,
        pipeline,
        src_ip,
        bind_device,
        &PM_COINS,
        2,
        false,
        None,
        &[],
    )
}

/// The maker simulation (`--dry-run`, `live_feed` venues): `polymarket-clob` with three BTC slot
/// pairs (InstId 1–6: up-0, dn-0, up-1, dn-1, up-2, dn-2) and `binance-spot` BTCUSDT (InstId 7) as
/// the reference; Sim account `sim-pm` (10 000 USDC) bound to a `binary-maker` instance (enabled,
/// other parameters at their defaults) with one single-leg group per slot plus the reference group;
/// `[sim] maker_fill = "queue"`. Discovery: `bind_slots.py --assets btc --slots 3 --settle`.
#[cfg(feature = "polymarket")]
#[allow(dead_code)]
pub fn write_polymarket_maker_sim(
    root: &Path,
    hot: &Path,
    user: Option<&str>,
    pipeline: Option<&str>,
    src_ip: &str,
    bind_device: &str,
) -> PathBuf {
    write_polymarket_maker_sim_with(root, hot, user, pipeline, src_ip, bind_device, &[])
}

/// `write_polymarket_maker_sim` with binary-maker parameter values `params` (field name, raw value;
/// `pm_sim_config --param`, checked by `check_maker_params`) set after the defaults.
#[cfg(feature = "polymarket")]
#[allow(dead_code)]
pub fn write_polymarket_maker_sim_with(
    root: &Path,
    hot: &Path,
    user: Option<&str>,
    pipeline: Option<&str>,
    src_ip: &str,
    bind_device: &str,
    params: &[(String, i64)],
) -> PathBuf {
    write_pm(
        root,
        hot,
        user,
        pipeline,
        src_ip,
        bind_device,
        &["btc"],
        3,
        true,
        None,
        params,
    )
}

/// `NAME=RAW` of `pm_sim_config --param` → (name, raw i64).
#[cfg(feature = "polymarket")]
#[allow(dead_code)]
pub fn parse_param(s: &str) -> Result<(String, i64), String> {
    let (k, v) = s
        .split_once('=')
        .ok_or_else(|| format!("--param {s}: expected NAME=RAW"))?;
    let v = v
        .trim()
        .parse::<i64>()
        .map_err(|_| format!("--param {s}: RAW must be an integer (schema raw units)"))?;
    Ok((k.trim().to_string(), v))
}

/// Every `(name, raw)` names a field of the maker's parameter schema and lies within its
/// `[min, max]`.
#[cfg(feature = "polymarket")]
#[allow(dead_code)]
pub fn check_maker_params(params: &[(String, i64)]) -> Result<(), String> {
    use hft_strategy_sdk::StrategySet;
    use hft_strategy_sdk::params::SchemaView;
    let desc = Strategies::kinds()
        .iter()
        .find(|d| d.name == "binary-maker")
        .ok_or("binary-maker is not registered")?;
    for (name, v) in params {
        let f = hft_strategy_sdk::params::field_ix(desc.schema, name)
            .and_then(|i| desc.schema.field(i))
            .ok_or_else(|| format!("--param {name}: not a binary-maker parameter"))?;
        if *v < f.min || *v > f.max {
            return Err(format!("--param {name}={v}: outside [{}, {}]", f.min, f.max));
        }
    }
    Ok(())
}

/// The live account of `write_polymarket_maker_live`.
#[cfg(feature = "polymarket")]
#[allow(dead_code)]
#[derive(Clone, Debug)]
pub struct PmLive {
    /// Venue UID: the first 24 characters of the CLOB API key (venue-polymarket README "账户配置核验").
    pub uid: String,
    /// Directory of the credential files `api_key`, `secret` (`<api secret>|<EOA key>|<deposit
    /// wallet>`) and `passphrase`; hft-core reads and unlinks them at startup.
    pub keys_dir: PathBuf,
    /// Day loss of earlier runs of this UTC day: `(UTC day number, USDC raw 1e-6, < 0)`, booked at
    /// the first day roll of the fresh journal (hft-sequencer ECF1 section 8 fields 7/8).
    pub day_loss_carry: Option<(u32, i64)>,
    /// `daily_loss_abs` of `pm-live` (USDC raw 1e-6, > 0); `None` = 15 USDC.
    pub daily_loss_abs: Option<i64>,
    /// `daily_loss_max_resumes` of `pm-live`; `None` = the hft-risk default (1).
    pub daily_loss_max_resumes: Option<u8>,
    /// `venues.pm.presign` = `PM_LIVE_PRESIGN`; false (`--no-presign`) = no table (off).
    pub presign: bool,
    /// Further Live accounts `(UID, KEYS_DIR)` (`--live-account`, repeatable): accounts 2, 3, …
    /// labelled `pm-live-2`, …, bound to the same binary-maker instance with `pm-live`'s risk
    /// limits; the strategy parameters stay one blob under `pm-live`.
    pub extra_accounts: Vec<(String, PathBuf)>,
}

/// `venues.pm.presign` of the live configuration: two prices below and two above the last place
/// in 0.01 USDC steps (the Polymarket tick), re-signed within one minute (venue-polymarket "预签名").
#[allow(dead_code)]
pub const PM_LIVE_PRESIGN: &str = "below=2
above=2
step=\"0.01\"
max_age_ms=60000";

/// Live `daily_loss_abs` without `PmLive.daily_loss_abs` (15 USDC).
#[allow(dead_code)]
pub const PM_LIVE_DAILY_LOSS: i64 = 15_000_000;

/// Binary-maker parameters of the first live run (`micro_s2_c5_cap10_mc20`, research reply
/// 2026-10-22): 5 shares per quote, net 10, no-chase 1 s, fair-value cooldown 1 s, streak pause
/// (2 fills within 10 s → 10 s), at most 20 USDC bought per window.
#[cfg(feature = "polymarket")]
#[allow(dead_code)]
pub const PM_LIVE_PARAMS: [(&str, i64); 9] = [
    ("enabled", 1),
    ("clip_shares", 5),
    ("inv_max_shares", 10),
    ("fv_cool_ms", 1_000),
    ("nochase_ms", 1_000),
    ("streak_k", 2),
    ("streak_window_ms", 10_000),
    ("streak_pause_ms", 10_000),
    ("max_window_cost", 20_000_000),
];

/// Live Polymarket: OMS `ack_timeout_ms` and the REST request timeout (equal: the answer is read
/// as long as the OMS waits for it), and the REST connections' maximum idle age.
#[allow(dead_code)]
pub const PM_REST_TIMEOUT_MS: u32 = 5000;
#[allow(dead_code)]
pub const PM_REST_MAX_IDLE_MS: u32 = 25_000;

/// The live maker (`order_env = "prod"`, no `--dry-run`): the maker simulation's venues, slots and
/// strategy with a Live account `pm-live` (`is_prod`, UID `live.uid`) on Polymarket, its keys in
/// `live.keys_dir`, `PM_LIVE_PARAMS`, the micro risk limits (order 10 USDC, account and venue
/// 55, slot group 25, daily loss `live.daily_loss_abs` or 15, resumes `live.daily_loss_max_resumes`
/// or the default), OMS `ack_timeout_ms[polymarket] = 5000` and the REST pools'
/// `venues.pm.net` (`rest_timeout_ms = 5000`, `rest_max_idle_ms = 25000`); every Polymarket
/// config sets OMS `expire_window_ms[polymarket]` to the GTD lifetime (`stale_guard()`).
#[cfg(feature = "polymarket")]
#[allow(dead_code)]
pub fn write_polymarket_maker_live(
    root: &Path,
    hot: &Path,
    user: Option<&str>,
    pipeline: Option<&str>,
    src_ip: &str,
    bind_device: &str,
    live: &PmLive,
) -> PathBuf {
    write_polymarket_maker_live_with(root, hot, user, pipeline, src_ip, bind_device, live, &[])
}

/// `write_polymarket_maker_live` with binary-maker parameter values `params` set after
/// `PM_LIVE_PARAMS` (`pm_sim_config --live … --param`).
#[cfg(feature = "polymarket")]
#[allow(dead_code, clippy::too_many_arguments)]
pub fn write_polymarket_maker_live_with(
    root: &Path,
    hot: &Path,
    user: Option<&str>,
    pipeline: Option<&str>,
    src_ip: &str,
    bind_device: &str,
    live: &PmLive,
    params: &[(String, i64)],
) -> PathBuf {
    write_pm(
        root,
        hot,
        user,
        pipeline,
        src_ip,
        bind_device,
        &["btc"],
        3,
        true,
        Some(live),
        params,
    )
}

#[cfg(feature = "polymarket")]
#[allow(clippy::too_many_arguments)]
fn write_pm(
    root: &Path,
    hot: &Path,
    user: Option<&str>,
    pipeline: Option<&str>,
    src_ip: &str,
    bind_device: &str,
    coins: &[&str],
    pairs: usize,
    maker: bool,
    live: Option<&PmLive>,
    params: &[(String, i64)],
) -> PathBuf {
    use hft_venue_proto::VenueProtocol;
    std::fs::create_dir_all(root).unwrap();
    let v = VenueId::POLYMARKET_CLOB;
    let bn = VenueId::BINANCE_SPOT;
    let a = AccountId(1);
    let usdc = AssetId(1);
    let desc = Strategies::kinds()
        .iter()
        .find(|d| d.name == if maker { "binary-maker" } else { "book-observer" })
        .unwrap();
    let mut slots = Vec::new();
    for coin in coins {
        for k in 0..pairs {
            for side in ["up", "dn"] {
                slots.push(format!("{coin}-{side}-{k}"));
            }
        }
    }
    // Reference market of the maker: BTC and USDT assets after the token assets.
    let btc = AssetId(slots.len() as u16 + 2);
    let usdt = AssetId(slots.len() as u16 + 3);
    // Binance spot's fee rule names BNB (fee discount asset).
    let bnb = AssetId(slots.len() as u16 + 4);
    let reference = InstId(slots.len() as u32 + 1);
    let token = |i: usize| AssetId(i as u16 + 2);
    let mut b = RdbBuilder::new(1);
    b.asset(AssetSpec {
        id: usdc,
        name: "USDC".into(),
        class: AssetClass::Stable,
        exp: AssetExp {
            px_exp: -6,
            qty_exp: -6,
        },
        px_max: Px(1_000_000_000_000),
        qty_max: Qty(1_000_000_000_000_000),
        dust: Qty(1),
    });
    for (i, s) in slots.iter().enumerate() {
        b.asset(AssetSpec {
            id: token(i),
            name: format!("PM{}", s.replace('-', "").to_ascii_uppercase()).into(),
            class: AssetClass::Crypto,
            exp: AssetExp {
                px_exp: -3,
                qty_exp: -6,
            },
            px_max: Px(1_000),
            qty_max: Qty(1_000_000_000_000_000),
            dust: Qty(1),
        });
    }
    if maker {
        for (id, name, class, px_exp, qty_exp) in [
            (btc, "BTC", AssetClass::Crypto, -4, -8),
            (usdt, "USDT", AssetClass::Stable, -6, -6),
            (bnb, "BNB", AssetClass::Crypto, -4, -8),
        ] {
            b.asset(AssetSpec {
                id,
                name: name.into(),
                class,
                exp: AssetExp { px_exp, qty_exp },
                px_max: Px(1_000_000_000_000),
                qty_max: Qty(1_000_000_000_000_000),
                dust: Qty(1),
            });
        }
        b.venue(VenueSpec {
            id: bn,
            exchange: Exchange::Binance,
            product: InstKind::Spot,
            ws_max_streams_per_conn: 1024,
            ws_max_in_msgs_per_s: 5,
            ws_max_conn_age_s: 86400,
            ws_new_conn_per_5min_ip: 300,
            app_ping_interval_s: 0,
            ts_unit: TsUnit::Us,
            client_id_max_len: 36,
            client_id_prefix: "".into(),
            max_batch: 5,
            max_inflight_amend: 1,
            crossed_tolerance_default: 0,
            stale_guard: StaleGuardKind::RecvWindow,
        });
        for (role, url) in [
            (EndpointRole::MdPublic, "wss://stream.binance.com/ws"),
            (EndpointRole::MdMarket, "wss://stream.binance.com/ws"),
            (EndpointRole::Rest, "https://api.binance.com"),
        ] {
            b.endpoint(
                bn,
                Endpoint {
                    env: EndpointEnv::Prod,
                    role,
                    url: url.into(),
                    conn_group: 0,
                },
            );
        }
        b.caps(venue_binance::Spot::default_caps(|s| match s {
            "USDT" => Some(usdt),
            "BNB" => Some(bnb),
            _ => None,
        }));
        b.fee(FeeSpec {
            venue: bn,
            tier: 0,
            fee: FeePair {
                maker: Rate(1_000_000),
                taker: Rate(1_000_000),
            },
        });
        b.limit(
            bn,
            RateLimit {
                scope: LimitScope::Account,
                bucket: LimitBucket::Orders,
                limit: 100,
                window_ms: 1000,
                default_cost: 1,
            },
        );
    }
    b.venue(VenueSpec {
        id: v,
        exchange: Exchange::Polymarket,
        product: InstKind::Spot,
        ws_max_streams_per_conn: 2,
        ws_max_in_msgs_per_s: 10,
        ws_max_conn_age_s: 86_400,
        ws_new_conn_per_5min_ip: 300,
        app_ping_interval_s: 10,
        ts_unit: TsUnit::Ms,
        client_id_max_len: 32,
        client_id_prefix: "".into(),
        max_batch: 1,
        max_inflight_amend: 1,
        crossed_tolerance_default: PM_CROSSED_TOLERANCE,
        stale_guard: StaleGuardKind::ExpireAt,
    });
    let market = "wss://ws-subscriptions-clob.polymarket.com/ws/market";
    let mut pm_endpoints = vec![
        (EndpointRole::MdPublic, market),
        (EndpointRole::MdMarket, market),
        (EndpointRole::Rest, "https://clob.polymarket.com"),
    ];
    if live.is_some() {
        pm_endpoints.push((
            EndpointRole::OrderWs,
            "wss://ws-subscriptions-clob.polymarket.com/ws/user",
        ));
    }
    for (role, url) in pm_endpoints {
        b.endpoint(
            v,
            Endpoint {
                env: EndpointEnv::Prod,
                role,
                url: url.into(),
                conn_group: 0,
            },
        );
    }
    b.caps(venue_polymarket::PolymarketClob::default_caps(|s| {
        (s == "USDC").then_some(usdc)
    }));
    b.fee(FeeSpec {
        venue: v,
        tier: 0,
        fee: FeePair {
            maker: Rate(0),
            taker: Rate(0),
        },
    });
    for r in venue_polymarket::PolymarketClob::rate_rules() {
        b.limit(
            v,
            RateLimit {
                scope: r.scope,
                bucket: r.bucket,
                limit: r.limit,
                window_ms: r.window_ms,
                default_cost: 1,
            },
        );
    }
    for (i, s) in slots.iter().enumerate() {
        b.inst(InstSpec {
            id: InstId(i as u32 + 1),
            venue: v,
            symbol: s.as_str().into(),
            status: InstStatus::Trading,
            kind: InstKind::Spot,
            asset: token(i),
            quote: usdc,
            tick: 1,
            // Live: the exchange takes sizes in 0.01 shares (taker amount of a buy at most 2
            // decimals) and at least 5 shares; the sim keeps one unit.
            lot: if live.is_some() { 10_000 } else { 1 },
            min_qty: Qty(if live.is_some() { 5_000_000 } else { 1 }),
            max_qty: Qty(1_000_000_000_000),
            min_notional: Notional(1_000_000),
            book_window: 128,
            l2_depth: 100,
            crossed_tolerance: PM_CROSSED_TOLERANCE,
            tick_str: "0.001".into(),
            lot_str: if live.is_some() { "0.01" } else { "0.000001" }.into(),
            min_qty_str: if live.is_some() { "5" } else { "0.000001" }.into(),
            ..InstSpec::default()
        });
    }
    if maker {
        b.inst(InstSpec {
            id: reference,
            venue: bn,
            symbol: "BTCUSDT".into(),
            kind: InstKind::Spot,
            asset: btc,
            quote: usdt,
            tick: 100,
            lot: 1000,
            min_qty: Qty(1000),
            max_qty: Qty(100_000_000_000),
            min_notional: Notional(5_000_000),
            book_window: 4096,
            tick_str: "0.01".into(),
            lot_str: "0.00001".into(),
            min_qty_str: "0.00001".into(),
            ..InstSpec::default()
        });
    }
    b.instance(InstanceSpec {
        id: StrategyId(1),
        kind_id: desc.kind_id,
        name: if maker { "pm-maker" } else { "pm-observer" }.into(),
    });
    b.account(AccountSpec {
        id: a,
        sim: live.is_none(),
        bound_instance: StrategyId(1),
        is_prod: live.is_some(),
        venues: 1 << v.0,
        label: if live.is_some() { "pm-live" } else { "sim-pm" }.into(),
        uid_policy: UidPolicy::Verified,
    });
    if let Some(l) = live {
        b.account_venue(AccountVenueSpec {
            account: a,
            venue: v,
            uid: l.uid.as_str().into(),
        });
        for (i, (uid, _)) in l.extra_accounts.iter().enumerate() {
            let id = AccountId(a.0 + 1 + i as u8);
            b.account(AccountSpec {
                id,
                sim: false,
                bound_instance: StrategyId(1),
                is_prod: true,
                venues: 1 << v.0,
                label: format!("pm-live-{}", id.0).as_str().into(),
                uid_policy: UidPolicy::Verified,
            });
            b.account_venue(AccountVenueSpec {
                account: id,
                venue: v,
                uid: uid.as_str().into(),
            });
        }
    } else {
        b.sim(SimSpec {
            account: a,
            fee_tier: 0,
            latency_model: 0,
            margin_model: 0,
        });
        b.sim_balance(SimBalanceSpec {
            account: a,
            venue: v,
            asset: usdc,
            qty: Qty(if maker { 10_000_000_000 } else { 1_000_000_000 }),
        });
    }
    let rdb = b.finish();
    let rd = RefData::load(&rdb, None).unwrap();
    let schema = desc.schema;
    let n = slots.len() as u16 + u16::from(maker);
    let mut defaults: Vec<_> = (0..schema.n_fields())
        .map(|i| schema.field(i).unwrap().default)
        .collect();
    if maker {
        let enabled = (0..schema.n_fields())
            .position(|i| schema.field(i).unwrap().name == "enabled")
            .unwrap();
        defaults[enabled] = 1;
    }
    if live.is_some() {
        for (name, value) in PM_LIVE_PARAMS {
            let k = (0..schema.n_fields())
                .position(|i| schema.field(i).unwrap().name == name)
                .unwrap();
            defaults[k] = value;
        }
    }
    assert!(maker || params.is_empty(), "parameter values need the maker");
    if let Err(e) = check_maker_params(params) {
        panic!("{e}");
    }
    for (name, value) in params {
        if let Some(k) = (0..schema.n_fields()).position(|i| schema.field(i).unwrap().name == *name) {
            defaults[k] = *value;
        }
    }
    let body = encode_body(
        schema,
        n,
        &RawParams {
            defaults,
            overrides: vec![],
        },
    )
    .unwrap();
    let h = StrategyParamsHdr {
        strategy: StrategyId(1),
        account: a,
        kind_id: desc.kind_id,
        schema_id: schema.id,
        schema_ver: schema.version,
        source: ParamSource::Manual,
        _pad: 0,
        n_groups: n,
        _pad2: 0,
        version: 1,
        generated_at_ns: 0,
        body_len: body.len() as u32,
        group_ctr_hwm: u32::from(n),
        schema_digest: hft_strategy_sdk::params::schema_digest(desc, schema.version),
    };
    let none = LegDef {
        inst: InstId(0),
        venue: VenueId(0),
        kind: 0,
        _pad: 0,
        route: 0,
        _pad2: 0,
    };
    let mut params = h.as_bytes().to_vec();
    for i in 0..usize::from(n) {
        let (asset, venue) = if i < slots.len() { (token(i), v) } else { (btc, bn) };
        let g = GroupSpec {
            id: GroupId::compose(StrategyId(1), i as u32 + 1).unwrap(),
            asset,
            n_legs: 1,
            open_allowed: u8::from(maker && i < slots.len()),
            legs: [
                LegDef {
                    inst: InstId(i as u32 + 1),
                    venue,
                    kind: LegDef::CEX,
                    ..none
                },
                none,
                none,
                none,
            ],
            _rsv: 0,
        };
        params.extend_from_slice(g.as_bytes());
    }
    params.extend(body);
    let mut risk = hft_risk::ParamsBuilder::new(&rd);
    risk.rate_headroom = Rate(100_000_000);
    risk.classes.push(hft_risk::BandSpec {
        // Outcome-token spreads are wide relative to their price (0.01 at 0.10 is 10 %).
        max_px_dev: Rate(if maker { 500_000_000 } else { 30_000_000 }),
        ioc_max_cross: Rate(5_000_000),
        rr_max_cross: Some(Rate(10_000_000)),
    });
    // Live micro limits (USDC, 1e-6): account and venue 55 (two overlapping windows plus resting
    // quotes, below the 82 deposited), one slot group 25, daily loss 15 unless chosen.
    let (acct_cap, group_cap, loss_cap) = match (live, maker) {
        (Some(l), _) => (
            55_000_000,
            25_000_000,
            l.daily_loss_abs.unwrap_or(PM_LIVE_DAILY_LOSS),
        ),
        (None, true) => (10_000_000_000, 1_000_000_000, 1_000_000_000),
        (None, false) => (100_000_000, 10_000_000, 10_000_000),
    };
    risk.accounts.push((
        a,
        hft_risk::AcctSpec {
            max_order_notional: Notional(10_000_000),
            max_account_notional: Notional(acct_cap),
            account_notional_from_equity: Rate(1_000_000_000),
            // A slot group is one leg held to settlement: its whole position is "naked" (ratio 1),
            // so the exposure gate is off for the maker (naked_gate = 1).
            default_group: if maker {
                hft_risk::GroupLimits {
                    max_group_notional: Notional(group_cap),
                    max_pos_qty: Qty(1_000_000_000),
                    naked_gate: Rate(1_000_000_000),
                    naked_gate_min: Notional(1_000_000_000),
                }
            } else {
                hft_risk::GroupLimits {
                    max_group_notional: Notional(10_000_000),
                    max_pos_qty: Qty(100_000_000),
                    naked_gate: Rate(500_000_000),
                    naked_gate_min: Notional(10_000_000),
                }
            },
            max_hedge_notional: Some(Notional(10_000_000)),
            max_reduce_notional: Some(Notional(10_000_000)),
            // The absolute line binds: the quota is min(daily_loss_abs, equity × daily_loss_kill).
            daily_loss_kill: Rate(if live.is_some() {
                1_000_000_000
            } else {
                100_000_000
            }),
            daily_loss_abs: Notional(loss_cap),
            daily_loss_max_resumes: live.and_then(|l| l.daily_loss_max_resumes),
            recon_closeonly_notional: Notional(5_000_000),
            ..hft_risk::AcctSpec::default()
        },
    ));
    risk.venues.push((
        v,
        a,
        hft_risk::VenueSpec {
            max_venue_notional: Notional(acct_cap),
            inst_bucket_per_s: 10,
            backoff_base_ms: 500,
            backoff_max_ms: 8000,
            rr_reserve: None,
        },
    ));
    // Further live accounts: `pm-live`'s account and venue limits each.
    let extra: Vec<AccountId> = live.map_or(Vec::new(), |l| {
        (0..l.extra_accounts.len())
            .map(|i| AccountId(a.0 + 1 + i as u8))
            .collect()
    });
    for id in &extra {
        let (acct, venue) = (risk.accounts[0].1, risk.venues[0].2);
        risk.accounts.push((*id, acct));
        risk.venues.push((v, *id, venue));
    }
    let risk = risk.build();
    let rpb = risk.encode_blob().unwrap();
    // Polymarket settles in USDC (pUSD, 1:1) and lists no USDC/USDT book: USDC is valued at par,
    // otherwise every USDC amount (P&L, held notional, equity) converts to 0 USDT and the daily
    // loss never fires (hft-ledger refuses such a config: NoUsdtRate).
    let mut ledger = hft_ledger::LedgerConfig::default();
    ledger.stable_par_fallback.clear();
    ledger.par_assets.push(hft_ledger::AssetName::new("USDC"));
    let rig = RigConfig {
        refdata_blob: rdb.clone(),
        params: vec![params.clone()],
        risk,
        risk_blob: rpb.clone(),
        caps: hft_ledger::Capacities {
            max_insts: 16,
            max_cells_total: 32,
            max_timers: 1024,
            recent_cmd_ids: 256,
            max_subs_per_inst: 4,
            max_groups_per_instance: 16,
            max_instances: 4,
        },
        ledger,
        venues: if maker { vec![v, bn] } else { vec![v] },
        keys: vec![],
        epoch: 1,
        tsc_hz_milli: 1_000_000_000_000,
        calib_tsc: 0,
        calib_utc_ns: 0,
        boot_tsc: 0,
        max_groups: 16,
    };
    let mut engine = engine_setup(&rig, &rd);
    engine.drain_timeout_ms = 3000;
    engine.gws.venues.clear();
    engine.feeds = VenueSet((1 << v.0) | if maker { 1 << bn.0 } else { 0 });
    engine.seq.rings[7] = hft_sequencer::RingLimit::default();
    engine.seq.allow_missing_standby = 1;
    // Every place is GTD: its lifetime is the expiry window, a NotFound past it + 500 ms ends the
    // slot (hft-oms "Unknown 收敛流程" 4; venue-polymarket "VenueCaps 默认值").
    let hft_venue_proto::StaleGuard::ExpireAt { default_ms } =
        venue_polymarket::PolymarketClob::stale_guard()
    else {
        panic!("Polymarket declares ExpireAt");
    };
    engine.oms.expire_window_ms.set(v, Some(default_ms));
    if let Some(l) = live {
        engine.role_prod = true;
        engine.gws.venues = vec![v];
        engine.keys_required = vec![(a, v, 3)];
        engine.keys_required.extend(extra.iter().map(|id| (*id, v, 3)));
        // A SmokeOrder bids half the ask (an outcome token at 0.50 → 0.25): it must never fill.
        engine.oms.smoke_far_bp = Rate(500_000_000);
        // A REST place answer can take over 2 s (2026-10-23, round trips otherwise up to 0.76 s);
        // a slot that goes Unknown on time-out only resolves by query. The REST pool waits as long
        // (`venues.pm.net.rest_timeout_ms`, at most this value).
        engine.oms.ack_timeout_ms.set(v, PM_REST_TIMEOUT_MS);
        if let Some((day, pnl)) = l.day_loss_carry {
            engine.day_pnl_carry = vec![(a, day, Notional(pnl))];
        }
    }
    engine.exec.default.force_slice.max_chunk_notional = Notional(10_000_000);
    let ecf = engine.encode_blob(1, HostClass::Dev2, 0);
    let mut uids = String::new();
    let mut issuers = String::new();
    if let Some(u) = user {
        uids += &format!("{u:?} = \"cli\"\n");
        issuers += &format!("\"cli:{u}\" = [\"Admin\"]\n");
    }
    if let Some(p) = pipeline {
        uids += &format!("{p:?} = \"pipeline\"\n");
    }
    std::fs::write(
        root.join("principals.toml"),
        format!("[uids]\n{uids}[issuers]\n{issuers}"),
    )
    .unwrap();
    let mut text = format!(
        r#"schema = 1
[instance]
name = "sim"
feed_env = "prod"
config_source = "file"
[paths]
journal_ram = '{}'
journal_cold = 'cold'
ctl_socket = 'core.sock'
ctl_principals = 'principals.toml'
[shutdown]
cancel_open_orders = true
drain_timeout_ms = 3000
[journal]
segment_bytes = 8388608
hot_keep = 2
[clock]
step_threshold_ns = 50000
interval_ms = 1000
[watchdog]
hb_stale_ms = 1000
[rings]
elements = 4096
bytes = 4194304
[venues.pm]
protocol = 'polymarket-clob'
id = {}
max_sub_insts = {n_slots}
stale_guard_ms = 1500
src_ip = '{src_ip}'
bind_device = '{bind_device}'
live_feed = true
[venues.pm.feed]
depth_conns = 2
{bn_text}[sim]
{maker_fill}seed = 123
entry_us = 100
resp_us = 100
leverage = 1
mmr_raw = 5000000
liq_fee_raw = 1000000
liq_slippage_bp = 10
quote_interval_ms = 1000
"#,
        hot.display(),
        v.0,
        n_slots = slots.len(),
        bn_text = if maker {
            format!(
                "[venues.bn]\nprotocol = 'binance-spot'\nid = {}\nmax_sub_insts = 1\nstale_guard_ms = 1500\n\
                 src_ip = '{src_ip}'\nbind_device = '{bind_device}'\nlive_feed = true\n",
                bn.0
            )
        } else {
            String::new()
        },
        maker_fill = if maker { "maker_fill = 'queue'\n" } else { "" },
    );
    for (name, task, nice) in [
        ("sequencer", "sequencer", 0),
        ("persist", "persist", 10),
        ("housekeeping", "housekeeping", 0),
        ("ctl-gateway", "ctl-gateway", 10),
        ("sim-gw", "sim-gw", 0),
        ("paper-feed", "paper-feed", 0),
        ("feed-pm", "feed:pm", 0),
        ("build-pm", "conn-builder:pm", 10),
    ] {
        text += &format!("\n[[threads]]\nname = '{name}'\ntask = '{task}'\npark_us = 100\nnice = {nice}\n");
    }
    if maker {
        for (name, task, nice) in [("feed-bn", "feed:bn", 0), ("build-bn", "conn-builder:bn", 10)] {
            text +=
                &format!("\n[[threads]]\nname = '{name}'\ntask = '{task}'\npark_us = 100\nnice = {nice}\n");
        }
    }
    for (name, bytes) in [
        ("refdata", rdb),
        ("engine", ecf),
        ("risk_params", rpb),
        ("strategy_params", params),
    ] {
        let file = format!("{name}.bin");
        std::fs::write(root.join(&file), &bytes).unwrap();
        let table = if name == "strategy_params" {
            format!("[[blobs.{name}]]")
        } else {
            format!("[blobs.{name}]")
        };
        text += &format!(
            "\n{table}\npath = '{file}'\nsha256 = '{}'\n",
            hex(&digest(&bytes))
        );
    }
    let path = root.join("core.toml");
    if let Some(l) = live {
        let mut config: toml::Value = toml::from_str(&text).unwrap();
        config.as_table_mut().unwrap().remove("sim");
        let instance = config["instance"].as_table_mut().unwrap();
        instance.insert("name".into(), "prod".into());
        instance.insert("order_env".into(), "prod".into());
        for name in ["pm", "bn"] {
            if let Some(v) = config["venues"].get_mut(name).and_then(toml::Value::as_table_mut) {
                v.remove("live_feed");
            }
        }
        let keys = |account: u8, dir: &Path| -> toml::Value {
            let k = |f: &str| dir.join(f).display().to_string();
            toml::from_str(&format!(
                "account={account}\napi_key_path={:?}\nsession_seed_path={:?}\nrest_api_key_path={:?}\nrest_secret_path={:?}\npassphrase_path={:?}",
                k("api_key"),
                k("secret"),
                k("api_key"),
                k("secret"),
                k("passphrase")
            ))
            .unwrap()
        };
        let mut accounts = vec![keys(a.0, &l.keys_dir)];
        for (id, (_, dir)) in extra.iter().zip(&l.extra_accounts) {
            accounts.push(keys(id.0, dir));
        }
        let pm = config["venues"]["pm"].as_table_mut().unwrap();
        pm.insert("accounts".into(), toml::Value::Array(accounts));
        pm.insert(
            "account_io".into(),
            toml::from_str(
                "timeout_ms=5000\ninterval_ms=60000\nrequest_gap_ms=200\nmax_cancel_rounds=3\nlisten_keepalive_ms=600000",
            )
            .unwrap(),
        );
        // A place answer is read up to the OMS ack timeout (tail 0.8 s, multi-second at window
        // starts); an idle REST connection is rebuilt after 25 s, before a middlebox can drop it
        // silently (hft-net "9. REST 客户端").
        pm.insert(
            "net".into(),
            toml::from_str(&format!(
                "rest_timeout_ms={PM_REST_TIMEOUT_MS}\nrest_max_idle_ms={PM_REST_MAX_IDLE_MS}"
            ))
            .unwrap(),
        );
        if l.presign {
            pm.insert("presign".into(), toml::from_str(PM_LIVE_PRESIGN).unwrap());
        }
        let threads = config["threads"].as_array_mut().unwrap();
        threads.retain(|t| !matches!(t["task"].as_str(), Some("sim-gw" | "paper-feed")));
        threads.push(toml::from_str("name='gw-pm'\ntask='gw:pm'\npark_us=100\nnice=0").unwrap());
        text = toml::to_string(&config).unwrap();
    }
    std::fs::write(&path, text).unwrap();
    path
}
