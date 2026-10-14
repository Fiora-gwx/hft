//! Process-isolated mock exchange: the production warm scheduler and both pure codecs, no sockets.
#![cfg(feature = "bitget")]
use hft_refdata::*;
use hft_types::*;
use hft_venue::account::{Client, Config, Transport};
use hft_venue_proto::{
    ArrayBuf, Credential, SecretBuf, SignAlg, VenueCfg, VenueEnv, VenueProtocol, account::AccountProtocol,
};
use std::sync::{Arc, Mutex};

fn rd() -> &'static RefData {
    rd_shift(0)
}
fn rd_shift(shift: i8) -> &'static RefData {
    let mut b = RdbBuilder::new(1);
    for (id, name, class, px, qty) in [
        (AssetId(1), "USDT", AssetClass::Stable, -6, -8),
        (AssetId(2), "BTC", AssetClass::Crypto, -2, -8),
    ] {
        b.asset(AssetSpec {
            id,
            name: name.into(),
            class,
            exp: AssetExp {
                px_exp: px,
                qty_exp: qty,
            },
            px_max: Px(1_000_000_000_000),
            qty_max: Qty(1_000_000_000_000),
            dust: Qty(1),
        });
    }
    for (v, exchange) in [
        (VenueId::BINANCE_USDM, Exchange::Binance),
        (VenueId::BITGET_UTA_USDT_FUT, Exchange::Bitget),
    ] {
        b.venue(VenueSpec {
            id: v,
            exchange,
            product: InstKind::Perp,
            ws_max_streams_per_conn: 100,
            ws_max_in_msgs_per_s: 10,
            ws_max_conn_age_s: 86400,
            ws_new_conn_per_5min_ip: 100,
            app_ping_interval_s: 0,
            ts_unit: TsUnit::Ms,
            client_id_max_len: 36,
            client_id_prefix: "".into(),
            max_batch: 1,
            max_inflight_amend: 1,
            crossed_tolerance_default: 0,
            stale_guard: StaleGuardKind::RecvWindow,
        });
        b.caps(if v == VenueId::BINANCE_USDM {
            venue_binance::Usdm::default_caps(|_| None)
        } else {
            venue_bitget::BitgetUsdtFutures::default_caps(|_| None)
        });
        b.fee(FeeSpec {
            venue: v,
            tier: 0,
            fee: FeePair {
                maker: Rate(200000),
                taker: Rate(500000),
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
        b.account_venue(AccountVenueSpec {
            account: AccountId(1),
            venue: v,
            uid: "".into(),
        });
        b.inst(InstSpec {
            id: InstId(if v == VenueId::BINANCE_USDM { 1 } else { 2 }),
            venue: v,
            symbol: "BTCUSDT".into(),
            kind: InstKind::Perp,
            asset: AssetId(2),
            quote: AssetId(1),
            tick: 10,
            lot: 100000,
            min_qty: Qty(100000),
            max_qty: Qty(100_000_000_000),
            min_notional: Notional(5_000_000),
            book_window: 4096,
            px_unit_shift: shift,
            qty_unit_shift: shift,
            tick_str: if shift == 0 { "0.1" } else { "100" }.into(),
            lot_str: if shift == 0 { "0.001" } else { "0.000001" }.into(),
            min_qty_str: if shift == 0 { "0.001" } else { "0.000001" }.into(),
            max_leverage: 5,
            funding_interval_s: 28800,
            ..InstSpec::default()
        });
    }
    b.instance(InstanceSpec {
        id: StrategyId(1),
        kind_id: 5,
        name: "mock".into(),
    });
    b.account(AccountSpec {
        id: AccountId(1),
        bound_instance: StrategyId(1),
        venues: (1 << VenueId::BINANCE_USDM.0) | (1 << VenueId::BITGET_UTA_USDT_FUT.0),
        uid_policy: UidPolicy::Unverifiable,
        label: "mock".into(),
        ..AccountSpec::default()
    });
    Box::leak(Box::new(RefData::load(&b.finish(), None).unwrap()))
}
#[derive(Default)]
struct Server {
    cancelled: bool,
    settings: usize,
    drift: bool,
    hedge: bool,
    funding: bool,
    fail: bool,
    requests: usize,
}
struct Mock {
    state: Arc<Mutex<Server>>,
    bitget: bool,
    pending: Option<Result<hft_net::rest::Response, hft_net::rest::Error>>,
}
impl Transport for Mock {
    fn start(&mut self, _: hft_net::BuildReq, wire: &[u8]) -> Result<(), hft_net::rest::Error> {
        assert!(self.pending.is_none());
        let request = std::str::from_utf8(wire).unwrap();
        let path = request.lines().next().unwrap();
        let mut s = self.state.lock().unwrap();
        s.requests += 1;
        if s.fail {
            self.pending = Some(Err(hft_net::rest::Error::Timeout));
            return Ok(());
        }
        let body = if self.bitget {
            assert!(request.contains("paptrading: 1"));
            let data = if path.contains("/market/instruments") {
                r#"[{"symbol":"BTCUSDT","status":"online","minOrderQty":"0.001","minOrderAmount":"5","pricePrecision":"1","quantityPrecision":"3"}]"#.to_string()
            } else if path.contains("/account/settings") {
                s.settings += 1;
                format!(
                    r#"{{"uid":"123","accountMode":"unified","accountLevel":"basic","holdMode":"{}","symbolConfigList":[{{"symbol":"BTCUSDT","category":"USDT-FUTURES","marginMode":"crossed","leverage":"2"}}]}}"#,
                    if s.hedge { "hedge_mode" } else { "one_way_mode" }
                )
            } else if path.contains("/account/info") {
                r#"{"userId":"123","permType":"read-and-write","permissions":["uta_trade","uta_mgt"]}"#.into()
            } else if path.contains("unfilled-orders") {
                if s.cancelled {
                    r#"{"list":[],"cursor":""}"#
                } else {
                    r#"{"list":[{"symbol":"BTCUSDT","orderId":"7"}],"cursor":""}"#
                }
                .into()
            } else if path.contains("cancel-order") {
                s.cancelled = true;
                r#"{"orderId":"7"}"#.into()
            } else if path.contains("/account/assets") {
                format!(
                    r#"{{"usdtEquity":"100","accountEquity":"200","mmr":"0.1","assets":[{{"coin":"USDT","balance":"{}","available":"90","locked":"0","debt":"0"}}]}}"#,
                    if s.drift { "99" } else { "100" }
                )
            } else if path.contains("current-position") {
                r#"{"list":[{"symbol":"BTCUSDT","holdMode":"one_way_mode","marginMode":"crossed","posSide":"short","total":"0.001","avgPrice":"60000","unrealisedPnl":"0","liquidationPrice":"100000"}]}"#.into()
            } else if path.contains("financial-records") {
                if s.funding {
                    r#"{"list":[{"id":"900","symbol":"BTCUSDT","coin":"USDT","category":"USDT-FUTURES","type":"CONTRACT_MAIN_SETTLE_FEE_USER_OUT","amount":"0.02","ts":"1790006400000"}],"cursor":""}"#.into()
                } else {
                    r#"{"list":[],"cursor":""}"#.into()
                }
            } else {
                panic!("unexpected mock endpoint")
            };
            format!(r#"{{"code":"00000","data":{data}}}"#)
        } else {
            if !path.contains("exchangeInfo") {
                assert!(request.contains("X-MBX-APIKEY: DUMMY"));
            }
            if path.contains("exchangeInfo") {
                r#"{"symbols":[{"symbol":"BTCUSDT","status":"TRADING","filters":[{"filterType":"PRICE_FILTER","tickSize":"0.1"},{"filterType":"LOT_SIZE","stepSize":"0.001","minQty":"0.001"},{"filterType":"MIN_NOTIONAL","notional":"5"}]}]}"#.into()
            } else if path.contains("accountConfig") {
                s.settings += 1;
                format!(
                    r#"{{"canTrade":true,"dualSidePosition":{},"multiAssetsMargin":false}}"#,
                    s.hedge
                )
            } else if path.contains("symbolConfig") {
                r#"[{"symbol":"BTCUSDT","marginType":"CROSSED","leverage":2}]"#.into()
            } else if path.contains("openOrders") {
                if s.cancelled {
                    "[]"
                } else {
                    r#"[{"symbol":"BTCUSDT","orderId":7}]"#
                }
                .into()
            } else if path.starts_with("DELETE ") {
                s.cancelled = true;
                r#"{"symbol":"BTCUSDT","orderId":7}"#.into()
            } else if path.contains("/v3/account") {
                format!(
                    r#"{{"totalMarginBalance":"100","totalMaintMargin":"0.1","availableBalance":"90","assets":[{{"asset":"USDT","walletBalance":"{}"}}]}}"#,
                    if s.drift { "99" } else { "100" }
                )
            } else if path.contains("positionRisk") {
                r#"[{"symbol":"BTCUSDT","positionSide":"BOTH","positionAmt":"0.001","entryPrice":"60000","unRealizedProfit":"0","liquidationPrice":"1000"}]"#.into()
            } else if path.contains("listenKey") {
                r#"{"listenKey":"dummylistenkey"}"#.into()
            } else if path.contains("countdownCancelAll") {
                r#"{"symbol":"BTCUSDT","countdownTime":30000}"#.into()
            } else {
                panic!("unexpected mock endpoint")
            }
        };
        self.pending = Some(Ok(hft_net::rest::Response {
            status: 200,
            body: body.into_bytes(),
        }));
        Ok(())
    }
    fn poll(&mut self, _: i64) -> Option<Result<hft_net::rest::Response, hft_net::rest::Error>> {
        self.pending.take()
    }
}
fn exercise<P: AccountProtocol>(bitget: bool) {
    let rd = rd();
    let server = Arc::new(Mutex::new(Server::default()));
    let key = Credential {
        api_key: ArrayBuf::from_bytes(b"DUMMY").unwrap(),
        secret: SecretBuf(ArrayBuf::from_bytes(b"dummy-secret").unwrap()),
        passphrase: ArrayBuf::from_bytes(b"dummy-pass").unwrap(),
        alg: SignAlg::HmacSha256,
    };
    let cfg = Config {
        timeout_ms: 1000,
        interval_ms: 1000,
        request_gap_ms: 1,
        max_cancel_rounds: 4,
        listen_keepalive_ms: 10000,
        funding_window_ms: 172800000,
        funding_lag_ms: 600000,
    };
    let mut c = Client::<P, Mock>::new(
        Mock {
            state: server.clone(),
            bitget,
            pending: None,
        },
        "mock.invalid".into(),
        cfg,
        VenueCfg {
            env: VenueEnv::Testnet,
            stale_guard_ms: 1500,
        },
        rd,
        AccountId(1),
        key,
        key,
    );
    c.start();
    let mut now = 1_790_000_000_000_000_000i64;
    let baseline = loop {
        now += 2_000_000;
        c.poll(now).unwrap();
        if let Some(s) = c.take() {
            break s;
        }
    };
    assert!(server.lock().unwrap().cancelled);
    assert!(baseline.orders.is_empty());
    assert_eq!(baseline.config.unwrap().position_mode, 2);
    assert_eq!(baseline.leverage[0].leverage.0, 2_000_000_000);
    if bitget {
        let m = baseline.margin.as_ref().unwrap();
        assert_eq!(m.maint_margin.0, 50_000, "USD maintenance converted to USDT");
        assert_eq!(m.margin_ratio.0, 500_000);
    }
    assert_eq!(baseline.balances[0].free.0, 10_000_000_000);
    assert_eq!(baseline.positions[0].qty.0, if bitget { -100000 } else { 100000 });
    let mut config = hft_ledger::LedgerConfig::default();
    config.stable_par_fallback.clear();
    let caps = hft_ledger::Capacities {
        max_insts: 8,
        max_cells_total: 8,
        max_timers: 128,
        recent_cmd_ids: 32,
        max_subs_per_inst: 4,
        max_groups_per_instance: 4,
        max_instances: 2,
    };
    let mut ledger = hft_ledger::Ledger::new(rd, &config, &caps).unwrap();
    for b in &baseline.balances {
        ledger.on_balance(b, true, 0);
    }
    for p in &baseline.positions {
        assert!(ledger.seed_position(p));
    }
    let before = server.lock().unwrap().settings;
    c.reauthenticate().unwrap();
    loop {
        now += 2_000_000;
        c.poll(now).unwrap();
        if c.take().is_some() {
            break;
        }
    }
    assert!(server.lock().unwrap().settings > before);
    if P::USER_STREAM {
        assert!(
            c.listen_body()
                .unwrap()
                .windows(14)
                .any(|s| s == b"dummylistenkey")
        );
    }
    server.lock().unwrap().drift = true;
    now += 2_000_000_000;
    let observed = loop {
        now += 2_000_000;
        c.poll(now).unwrap();
        if let Some(s) = c.take() {
            break s;
        }
    };
    let b = observed.balances.iter().find(|b| b.asset == AssetId(1)).unwrap();
    assert!(matches!(
        ledger.on_balance(b, true, 10_000_000_000),
        hft_ledger::BalanceVerdict::Drift {
            diff: Qty(-100_000_000),
            ..
        }
    ));
    assert_eq!(
        ledger
            .venue_book(AccountId(1), P::VENUE)
            .cash
            .iter()
            .find(|c| c.asset == AssetId(1))
            .unwrap()
            .total
            .0,
        10_000_000_000
    );
    if bitget {
        server.lock().unwrap().funding = true;
        now = 1_790_006_400_000_000_000 + 601_000_000_000;
        let mut seen = false;
        for _ in 0..200 {
            now += 2_000_000;
            c.poll(now).unwrap();
            if let Some(s) = c.take()
                && let Some(f) = s.funding.first()
            {
                assert_eq!(
                    (f.inst.0, f.amount.0, f.settle_ts_ns),
                    (2, -20_000, 1_790_006_400_000_000_000)
                );
                seen = true;
                break;
            }
        }
        assert!(seen, "completed funding window reaches account ingress");
    }
    server.lock().unwrap().fail = true;
    c.reauthenticate().unwrap();
    now += 2_000_000;
    c.poll(now).unwrap();
    assert!(c.poll(now + 2_000_000).is_err());
    c.retry(now);
    server.lock().unwrap().fail = false;
    server.lock().unwrap().hedge = true;
    now += 2_000_000_000;
    let mut failed = false;
    for _ in 0..50 {
        now += 2_000_000;
        if c.poll(now).is_err() {
            failed = true;
            break;
        }
    }
    assert!(failed, "hedge-mode account must fail closed");
}
#[test]
fn core_120_mock_accounts_bootstrap_reauth_and_drift() {
    // Venue units can differ from asset units; rules must share the decoder normalization.
    let shifted = rd_shift(3);
    for inst in [InstId(1), InstId(2)] {
        assert!(hft_venue_proto::account::rules(shifted, inst, "100", "0.000001", "0.000001", "5").is_ok());
        assert!(hft_venue_proto::account::rules(shifted, inst, "0.1", "0.000001", "0.000001", "5").is_err());
    }
    if std::env::var_os("HFT_MOCK_ACCOUNT_CHILD").is_none() {
        let o = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "core_120_mock_accounts_bootstrap_reauth_and_drift",
                "--nocapture",
            ])
            .env("HFT_MOCK_ACCOUNT_CHILD", "1")
            .output()
            .unwrap();
        assert!(
            o.status.success(),
            "{}{}",
            String::from_utf8_lossy(&o.stdout),
            String::from_utf8_lossy(&o.stderr)
        );
        return;
    }
    exercise::<venue_binance::Usdm>(false);
    exercise::<venue_bitget::BitgetUsdtFutures>(true);
}
