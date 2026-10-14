//! Shared fixtures of the venue-binance tests (TESTS.md "记号"): `RD-BIN`, `RX`, `WC`, the test
//! credentials, `CID(e, s, g)`, the sample payloads and a recording `ReportSink`.
#![allow(dead_code)]

use std::sync::OnceLock;

use hft_event::{
    AccountConfigReportHdr, BalanceReport, ExternalFill, FundingSettle, LeverageItem, MarginItem,
    MarginReportHdr, OrderEvent, PositionReport,
};
use hft_refdata::{
    AccountSpec, AccountVenueSpec, AssetClass, AssetSpec, Exchange, FeePair, FeeSpec, InstKind, InstSpec,
    InstStatus, RateLimit, RdbBuilder, RefData, StaleGuardKind, TsUnit, VenueSpec,
};
use hft_types::{
    AccountId, AssetExp, AssetId, ClientOrderId, ConnId, InstId, Notional, Px, Qty, Rate, VenueId,
};
use hft_venue_proto::{
    ArrayBuf, Credential, ExchIdHint, ReportSink, RxMeta, SecretBuf, SignAlg, VenueCfg, VenueEnv,
    VenueProtocol, WallClock,
};
use venue_binance::{Spot, Usdm};

pub const USDT: AssetId = AssetId(1);
pub const BTC: AssetId = AssetId(2);
pub const ETH: AssetId = AssetId(3);
pub const BNB: AssetId = AssetId(4);
pub const U_BTC: InstId = InstId(1042);
pub const U_ETH: InstId = InstId(1043);
pub const S_BTC: InstId = InstId(2001);
pub const S_BNB: InstId = InstId(2002);
pub const ACCT: AccountId = AccountId(2);

/// `RD-BIN` (built in code with `RdbBuilder`; dense inst ids from 1, fillers `F<id>`: 1–2000 on
/// USDT-M except 1042/1043, none on spot besides 2001/2002).
pub fn rd() -> &'static RefData {
    static RD: OnceLock<&'static RefData> = OnceLock::new();
    RD.get_or_init(|| build(&[]))
}

pub fn rd_ignoring(names: &[&str]) -> &'static RefData {
    build(names)
}

fn build(names: &[&str]) -> &'static RefData {
    let mut b = RdbBuilder::new(1);
    b.created_utc_ns(1_759_276_800_000_000_000);
    for (id, name, class, px, qty) in [
        (USDT, "USDT", AssetClass::Stable, -6, -8),
        (BTC, "BTC", AssetClass::Crypto, -2, -8),
        (ETH, "ETH", AssetClass::Crypto, -2, -8),
        (BNB, "BNB", AssetClass::Crypto, -3, -8),
    ] {
        b.asset(AssetSpec {
            id,
            name: name.into(),
            class,
            exp: AssetExp {
                px_exp: px,
                qty_exp: qty,
            },
            px_max: Px(1_000_000_000_000_000),
            qty_max: Qty(1_000_000_000_000_000_000),
            dust: Qty(1_000),
        });
    }
    for (v, product, ms) in [
        (VenueId::BINANCE_USDM, InstKind::Perp, 10u16),
        (VenueId::BINANCE_SPOT, InstKind::Spot, 5),
    ] {
        b.venue(VenueSpec {
            id: v,
            exchange: Exchange::Binance,
            product,
            ws_max_streams_per_conn: 1024,
            ws_max_in_msgs_per_s: ms,
            ws_max_conn_age_s: 86_400,
            ws_new_conn_per_5min_ip: 300,
            app_ping_interval_s: 0,
            ts_unit: TsUnit::Ms,
            client_id_max_len: 36,
            client_id_prefix: "".into(),
            max_batch: 5,
            max_inflight_amend: 1,
            crossed_tolerance_default: 0,
            stale_guard: StaleGuardKind::RecvWindow,
        });
        b.fee(FeeSpec {
            venue: v,
            tier: 0,
            fee: FeePair {
                maker: Rate(200_000),
                taker: Rate(500_000),
            },
        });
    }
    let asset = |n: &str| match n {
        "BNB" => Some(BNB),
        _ => None,
    };
    let mut caps = Usdm::default_caps(asset);
    let mut events = caps.ignorable_user_events.to_vec();
    events.extend(names.iter().map(|s| Box::<str>::from(*s)));
    caps.ignorable_user_events = events.into();
    b.caps(caps);
    b.caps(Spot::default_caps(asset));
    for (v, rules) in [
        (VenueId::BINANCE_USDM, Usdm::rate_rules()),
        (VenueId::BINANCE_SPOT, Spot::rate_rules()),
    ] {
        for r in rules {
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
    }
    for id in 1..=2002u32 {
        let (venue, kind, sym, asset, tick, lot, tick_s, lot_s) = match id {
            1042 => (
                VenueId::BINANCE_USDM,
                InstKind::Perp,
                "BTCUSDT",
                BTC,
                10,
                100_000,
                "0.1",
                "0.001",
            ),
            1043 => (
                VenueId::BINANCE_USDM,
                InstKind::Perp,
                "ETHUSDT",
                ETH,
                1,
                100_000,
                "0.01",
                "0.001",
            ),
            2001 => (
                VenueId::BINANCE_SPOT,
                InstKind::Spot,
                "BTCUSDT",
                BTC,
                1,
                1_000,
                "0.01",
                "0.00001",
            ),
            2002 => (
                VenueId::BINANCE_SPOT,
                InstKind::Spot,
                "BNBUSDT",
                BNB,
                1,
                100_000,
                "0.001",
                "0.001",
            ),
            _ => (
                VenueId::BINANCE_USDM,
                InstKind::Perp,
                "",
                ETH,
                1,
                100_000,
                "0.01",
                "0.001",
            ),
        };
        let symbol = if sym.is_empty() {
            format!("F{id}USDT")
        } else {
            sym.to_string()
        };
        b.inst(InstSpec {
            id: InstId(id),
            venue,
            symbol,
            status: InstStatus::Trading,
            kind,
            asset,
            quote: USDT,
            tick,
            lot,
            min_qty: Qty(lot),
            max_qty: Qty(100_000_000_000_000),
            min_notional: Notional(5_000_000),
            book_window: 64,
            l2_depth: 20,
            crossed_tolerance: 0,
            tick_str: tick_s.into(),
            lot_str: lot_s.into(),
            min_qty_str: lot_s.into(),
            funding_interval_s: if kind == InstKind::Perp { 28_800 } else { 0 },
            max_leverage: if kind == InstKind::Perp { 20 } else { 0 },
            ..InstSpec::default()
        });
    }
    // One account per market of an exchange (refdata `SameExchangeMarkets`): account 1 trades
    // spot, account 2 (`ACCT`) USDT-M; both carry UID `acct-0001` (a warning, not an error).
    for (id, v, label) in [
        (AccountId(1), VenueId::BINANCE_SPOT, "bin-spot"),
        (ACCT, VenueId::BINANCE_USDM, "bin-usdm"),
    ] {
        b.account(AccountSpec {
            id,
            label: label.into(),
            venues: 1 << v.0,
            ..AccountSpec::default()
        });
        b.account_venue(AccountVenueSpec {
            account: id,
            venue: v,
            uid: "acct-0001".into(),
        });
    }
    let blob = b.finish();
    match RefData::load(&blob, None) {
        Ok(r) => Box::leak(Box::new(r)),
        Err(e) => panic!("RD-BIN rejected: {e:?}"),
    }
}

/// `RX`.
pub const RX: RxMeta = RxMeta {
    rx_tsc: 1000,
    hw_rx_ns: 1_790_000_000_126_000_000,
    sw_rx_ns: 1_790_000_000_126_500_000,
    prev_override: 0,
    conn: ConnId(3),
    account: ACCT,
    _pad: [0; 2],
    ts_flags: 0,
    rx_empty_gap_ns: 2500,
    _pad2: 0,
};

/// `WC`.
pub const WC: WallClock = WallClock {
    unix_ms: 1_790_000_000_000,
    unix_us: 1_790_000_000_000_000,
};

pub fn cfg(stale_guard_ms: u32) -> VenueCfg {
    VenueCfg {
        env: VenueEnv::Prod,
        stale_guard_ms,
    }
}

/// `CID(e, s, g)` of a line.
pub fn cid(venue: VenueId, epoch: u16, slot: u32, r#gen: u32) -> ClientOrderId {
    ClientOrderId {
        epoch,
        venue,
        account: ACCT,
        slot,
        r#gen,
    }
}

pub fn cid_text(id: &ClientOrderId) -> String {
    let mut t = [0u8; 24];
    id.encode(&mut t);
    String::from_utf8(t.to_vec()).unwrap()
}

/// RFC 8032 test 1 private key.
pub const ED1_SEED: &str = "9d61b19deffd5a60ba844af492ec2cc44449c5697b326919703bac031cae7f60";
pub const ED1_PUB: &str = "d75a980182b10ab7d54bfed3c964073a0ee172f3daa62325af021a68f707511a";
/// Binance documentation's HMAC example secret.
pub const HM1_SECRET: &str = "NhqPtmdSJYdKjVHjA7PZj4Mge3R5YNiP1e3UZjInClVN65XAbvqqM6A7H5fATj0j";

pub fn hex(s: &str) -> Vec<u8> {
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap())
        .collect()
}

/// `ED1`.
pub fn ed1() -> Credential {
    Credential {
        api_key: ArrayBuf::from_bytes(b"TESTKEY").unwrap(),
        secret: SecretBuf(ArrayBuf::from_bytes(&hex(ED1_SEED)).unwrap()),
        passphrase: ArrayBuf::new(),
        alg: SignAlg::Ed25519,
    }
}

/// `HM1`.
pub fn hm1() -> Credential {
    Credential {
        api_key: ArrayBuf::from_bytes(b"TESTKEY").unwrap(),
        secret: SecretBuf(ArrayBuf::from_bytes(HM1_SECRET.as_bytes()).unwrap()),
        passphrase: ArrayBuf::new(),
        alg: SignAlg::HmacSha256,
    }
}

/// `BT1`.
pub const BT1: &str = r#"{"e":"bookTicker","u":400900217,"s":"BTCUSDT","b":"65000.1","B":"1.500","a":"65000.2","A":"0.004","T":1790000000123,"E":1790000000125}"#;
/// `SBT1`.
pub const SBT1: &str = r#"{"u":400900217,"s":"BTCUSDT","b":"65000.10000000","B":"0.50000000","a":"65000.11000000","A":"0.25000000"}"#;
/// `DP5`.
pub const DP5: &str = r#"{"e":"depthUpdate","E":1790000000301,"T":1790000000300,"s":"BTCUSDT","U":7000000000,"u":7000000001,"pu":6999999999,"b":[["65000.1","1.000"],["65000.0","2.000"],["64999.9","0.500"],["64999.8","0.100"],["64999.7","3.000"]],"a":[["65000.2","0.400"],["65000.3","1.200"],["65000.4","0.050"],["65000.5","2.500"],["65000.6","0.010"]]}"#;
/// `SDP5`.
pub const SDP5: &str = r#"{"stream":"bnbusdt@depth5@100ms","data":{"lastUpdateId":160,"bids":[["600.100","1.000"],["600.000","2.000"],["599.900","0.500"],["599.800","0.100"],["599.700","3.000"]],"asks":[["600.200","0.400"],["600.300","1.200"],["600.400","0.050"],["600.500","2.500"],["600.600","0.010"]]}}"#;

/// Records every `ReportSink` call.
#[derive(Default, Debug)]
pub struct RepSink {
    pub orders: Vec<OrderEvent>,
    pub ext: Vec<ExternalFill>,
    pub balances: Vec<BalanceReport>,
    pub positions: Vec<PositionReport>,
    pub funding: Vec<FundingSettle>,
    pub exch_ids: Vec<(Vec<u8>, Option<ClientOrderId>)>,
    pub other: usize,
}

impl ReportSink for RepSink {
    fn order(&mut self, e: &OrderEvent) {
        self.orders.push(*e);
    }
    fn position(&mut self, e: &PositionReport) {
        self.positions.push(*e);
        self.other += 1;
    }
    fn balance(&mut self, e: &BalanceReport) {
        self.balances.push(*e);
        self.other += 1;
    }
    fn funding_settle(&mut self, e: &FundingSettle) {
        self.funding.push(*e);
        self.other += 1;
    }
    fn external_fill(&mut self, e: &ExternalFill) {
        self.ext.push(*e);
        self.other += 1;
    }
    fn margin(&mut self, _hdr: &MarginReportHdr, _items: &[MarginItem]) {
        self.other += 1;
    }
    fn account_config(&mut self, _hdr: &AccountConfigReportHdr, _lev: &[LeverageItem]) {
        self.other += 1;
    }
    fn exch_id(&mut self, key: &[u8], owner: Option<&ClientOrderId>) -> ExchIdHint {
        self.exch_ids.push((key.to_vec(), owner.copied()));
        ExchIdHint::Recorded
    }
}
