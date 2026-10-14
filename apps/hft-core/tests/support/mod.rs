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

pub fn write(root: &Path, hot: &Path, user: Option<&str>) -> PathBuf {
    std::fs::create_dir_all(root).unwrap();
    let v = VenueId::BINANCE_SPOT;
    let a = AccountId(1);
    let btc = AssetId(1);
    let usdt = AssetId(2);
    let desc = Strategies::kinds()
        .iter()
        .find(|d| d.non_prod_only && d.schema.fields.iter().any(|f| f.name == "probe_period_ms"))
        .unwrap();
    let mut b = RdbBuilder::new(1);
    for (id, name, class, exp) in [
        (
            btc,
            "BTC",
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
    b.venue(VenueSpec {
        id: v,
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
    use hft_venue_proto::VenueProtocol;
    b.caps(venue_binance::Spot::default_caps(|s| match s {
        "USDT" => Some(usdt),
        "BNB" => Some(AssetId(3)),
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
        id: InstId(1),
        venue: v,
        symbol: "BTCUSDT".into(),
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
    b.instance(InstanceSpec {
        id: StrategyId(1),
        kind_id: desc.kind_id,
        name: "paper-probe".into(),
    });
    b.account(AccountSpec {
        id: a,
        sim: true,
        bound_instance: StrategyId(1),
        is_prod: false,
        venues: 1 << v.0,
        label: "paper".into(),
        uid_policy: UidPolicy::Verified,
    });
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
        n_legs: 1,
        open_allowed: 1,
        legs: [
            LegDef {
                inst: InstId(1),
                venue: v,
                kind: LegDef::CEX,
                ..none
            },
            none,
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
            max_order_notional: Notional(100_000_000),
            max_account_notional: Notional(1_000_000_000),
            account_notional_from_equity: Rate(1_000_000_000),
            default_group: hft_risk::GroupLimits {
                max_group_notional: Notional(500_000_000),
                max_pos_qty: Qty(10_000_000),
                naked_gate: Rate(500_000_000),
                naked_gate_min: Notional(100_000_000),
            },
            max_hedge_notional: Some(Notional(100_000_000)),
            max_reduce_notional: Some(Notional(100_000_000)),
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
        venues: vec![v],
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
    engine.feeds = VenueSet(1 << v.0);
    engine.seq.rings[7] = hft_sequencer::RingLimit::default();
    engine.exec.default.force_slice.max_chunk_notional = Notional(100_000_000);
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
    for task in [
        "sequencer",
        "persist",
        "housekeeping",
        "ctl-gateway",
        "sim-gw",
        "paper-feed",
    ] {
        text += &format!("\n[[threads]]\nname = '{task}'\ntask = '{task}'\npark_us = 100\n");
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
    std::fs::write(&path, text).unwrap();
    path
}
