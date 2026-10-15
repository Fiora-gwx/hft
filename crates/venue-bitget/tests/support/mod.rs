//! Shared fixtures of the venue-bitget tests (TESTS.md "记号"): `RD-BGT`, `RX`, `WC`, `KEY`,
//! `CID(e, s, g)`, sample payloads and a recording `ReportSink`.
#![allow(dead_code)]

use std::sync::OnceLock;

use hft_event::{
    AccountConfigReportHdr, BalanceReport, ExternalFill, FundingSettle, LeverageItem, MarginItem,
    MarginReportHdr, OrderEvent, PositionReport,
};
use hft_refdata::{
    AccountSpec, AccountVenueSpec, AssetClass, AssetSpec, Exchange, FeePair, FeeSpec, InstKind, InstSpec,
    InstStatus, RateLimit, RdbBuilder, RefData, StaleGuardKind, TsUnit, VenueCaps, VenueSpec,
};
use hft_types::{
    AccountId, AssetExp, AssetId, ClientOrderId, ConnId, InstId, Notional, Px, Qty, Rate, VenueId,
};
use hft_venue_proto::{
    ArrayBuf, Credential, ExchIdHint, ReportSink, RxMeta, SecretBuf, SignAlg, VenueCfg, VenueEnv,
    VenueProtocol, WallClock,
};
use venue_bitget::{BitgetSpot, BitgetUsdtFutures};

pub const USDT: AssetId = AssetId(1);
pub const BTC: AssetId = AssetId(2);
pub const ETH: AssetId = AssetId(3);
pub const BGB: AssetId = AssetId(6);
pub const F_BTC: InstId = InstId(1801);
pub const F_ETH: InstId = InstId(1802);
pub const S_BTC: InstId = InstId(1901);
pub const S_BGB: InstId = InstId(1902);
pub const ACCT: AccountId = AccountId(2);
pub const FUT: VenueId = VenueId::BITGET_UTA_USDT_FUT;
pub const SPOT: VenueId = VenueId::BITGET_UTA_SPOT;

/// `RD-BGT` with the effective caps = `default_caps()`.
pub fn rd() -> &'static RefData {
    static RD: OnceLock<&'static RefData> = OnceLock::new();
    RD.get_or_init(|| build(&[], false, false, false))
}

pub fn rd_spot_capture() -> &'static RefData {
    static RD: OnceLock<&'static RefData> = OnceLock::new();
    RD.get_or_init(|| build(&[], false, false, true))
}

/// `RD-BGT` with extra `ignorable_user_events` on both lines (BGT-115.6).
pub fn rd_ignoring(names: &[&str]) -> &'static RefData {
    build(names, false, false, false)
}

pub fn account_rd(unverifiable: bool) -> &'static RefData {
    build(&[], true, unverifiable, false)
}

fn build(ignorable: &[&str], compact: bool, unverifiable: bool, spot_eth: bool) -> &'static RefData {
    let mut b = RdbBuilder::new(1);
    b.created_utc_ns(1_759_276_800_000_000_000);
    for (id, name, class, px, qty) in [
        (USDT, "USDT", AssetClass::Stable, -6, -8),
        (BTC, "BTC", AssetClass::Crypto, -2, -8),
        (ETH, "ETH", AssetClass::Crypto, -2, -8),
        (AssetId(4), "A4", AssetClass::Crypto, -2, -8),
        (AssetId(5), "A5", AssetClass::Crypto, -2, -8),
        (BGB, "BGB", AssetClass::Crypto, -4, -8),
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
    for (v, product) in [(FUT, InstKind::Perp), (SPOT, InstKind::Spot)] {
        b.venue(VenueSpec {
            id: v,
            exchange: Exchange::Bitget,
            product,
            ws_max_streams_per_conn: 1000,
            ws_max_in_msgs_per_s: 10,
            ws_max_conn_age_s: 86_400,
            ws_new_conn_per_5min_ip: 300,
            app_ping_interval_s: 25,
            ts_unit: TsUnit::Ms,
            client_id_max_len: 32,
            client_id_prefix: "".into(),
            max_batch: 1,
            max_inflight_amend: 1,
            crossed_tolerance_default: 0,
            stale_guard: StaleGuardKind::None,
        });
        b.fee(FeeSpec {
            venue: v,
            tier: 0,
            fee: FeePair {
                maker: Rate(200_000),
                taker: Rate(600_000),
            },
        });
    }
    let asset = |n: &str| match n {
        "BGB" => Some(BGB),
        _ => None,
    };
    for mut c in [
        BitgetUsdtFutures::default_caps(asset),
        BitgetSpot::default_caps(asset),
    ] {
        if !ignorable.is_empty() {
            c.ignorable_user_events = ignorable.iter().map(|s| (*s).into()).collect();
        }
        b.caps(c);
    }
    for v in [FUT, SPOT] {
        for r in BitgetUsdtFutures::rate_rules() {
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
    for id in 1..=if compact {
        4
    } else if spot_eth {
        1903u32
    } else {
        1902u32
    } {
        let wire_id = if compact {
            [1801, 1802, 1901, 1902][id as usize - 1]
        } else {
            id
        };
        let (venue, kind, sym, asset, tick, lot, tick_s, lot_s) = match wire_id {
            1801 => (FUT, InstKind::Perp, "BTCUSDT", BTC, 10, 100_000, "0.1", "0.001"),
            1802 => (FUT, InstKind::Perp, "ETHUSDT", ETH, 1, 1_000_000, "0.01", "0.01"),
            1901 => (SPOT, InstKind::Spot, "BTCUSDT", BTC, 1, 100, "0.01", "0.000001"),
            1903 => (SPOT, InstKind::Spot, "ETHUSDT", ETH, 1, 10_000, "0.01", "0.0001"),
            1902 => (
                SPOT,
                InstKind::Spot,
                "BGBUSDT",
                BGB,
                1,
                1_000_000,
                "0.0001",
                "0.01",
            ),
            _ => (FUT, InstKind::Perp, "", ETH, 1, 1_000_000, "0.01", "0.01"),
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
            l2_depth: 50,
            crossed_tolerance: 0,
            tick_str: tick_s.into(),
            lot_str: lot_s.into(),
            min_qty_str: lot_s.into(),
            funding_interval_s: if kind == InstKind::Perp { 28_800 } else { 0 },
            max_leverage: if kind == InstKind::Perp { 20 } else { 0 },
            ..InstSpec::default()
        });
    }
    // One account per market of an exchange: account 1 trades spot, 2 (`ACCT`) the perpetual.
    for (id, v, label) in [(AccountId(1), SPOT, "bg-spot"), (ACCT, FUT, "bg-fut")] {
        b.account(AccountSpec {
            id,
            label: label.into(),
            uid_policy: if unverifiable {
                hft_refdata::UidPolicy::Unverifiable
            } else {
                hft_refdata::UidPolicy::Verified
            },
            venues: 1 << v.0,
            ..AccountSpec::default()
        });
        b.account_venue(AccountVenueSpec {
            account: id,
            venue: v,
            uid: if unverifiable { "" } else { "acct-0001" }.into(),
        });
    }
    let blob = b.finish();
    match RefData::load(&blob, None) {
        Ok(r) => Box::leak(Box::new(r)),
        Err(e) => panic!("RD-BGT rejected: {e:?}"),
    }
}

/// The effective caps of a line in `RD-BGT`.
pub fn caps(v: VenueId) -> &'static VenueCaps {
    rd().caps(v)
}

/// `RX` (private connection: `account = 2`).
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

pub fn cfg() -> VenueCfg {
    VenueCfg {
        env: VenueEnv::Prod,
        stale_guard_ms: 5_000,
    }
}

/// `KEY` (synthetic).
pub fn key() -> Credential {
    Credential {
        api_key: ArrayBuf::from_bytes(b"k").unwrap(),
        secret: SecretBuf(ArrayBuf::from_bytes(b"secret").unwrap()),
        passphrase: ArrayBuf::from_bytes(b"p").unwrap(),
        alg: SignAlg::HmacSha256,
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

/// Head of a payload (first 64 bytes).
pub fn head(p: &str) -> &[u8] {
    &p.as_bytes()[..p.len().min(64)]
}

/// `B1v3(seq, ts)`: UTA `books1` push for BTCUSDT on the perpetual line.
pub fn b1v3(seq: &str, ts: &str, top_ts: &str) -> String {
    format!(
        r#"{{"data":[{{"a":[["65000.2","0.004"]],"b":[["65000.1","1.5"]],"seq":{seq},"ts":{ts}}}],"arg":{{"instType":"usdt-futures","symbol":"BTCUSDT","topic":"books1"}},"action":"snapshot","ts":{top_ts}}}"#
    )
}

/// `B1v2(seq, ts)` (Classic V2 shape; TESTS.md "记号").
pub fn b1v2(seq: u64, ts: u64) -> String {
    format!(
        r#"{{"action":"snapshot","arg":{{"instType":"USDT-FUTURES","channel":"books1","instId":"BTCUSDT"}},"data":[{{"asks":[["65000.2","0.004"]],"bids":[["65000.1","1.5"]],"checksum":0,"seq":{seq},"ts":"{ts}"}}],"ts":1790000000125}}"#
    )
}

/// Records every `ReportSink` call; `exch_id` lookups answer `hint`.
#[derive(Debug)]
pub struct RepSink {
    pub orders: Vec<OrderEvent>,
    pub ext: Vec<ExternalFill>,
    pub pos: Vec<PositionReport>,
    pub bal: Vec<BalanceReport>,
    pub funding: usize,
    pub margin: usize,
    pub exch_ids: Vec<(Vec<u8>, Option<ClientOrderId>)>,
    pub hint: ExchIdHint,
}

impl Default for RepSink {
    fn default() -> Self {
        RepSink {
            orders: Vec::new(),
            ext: Vec::new(),
            pos: Vec::new(),
            bal: Vec::new(),
            funding: 0,
            margin: 0,
            exch_ids: Vec::new(),
            hint: ExchIdHint::Unknown,
        }
    }
}

impl RepSink {
    pub fn events(&self) -> usize {
        self.orders.len() + self.ext.len() + self.pos.len() + self.bal.len() + self.funding + self.margin
    }
}

impl ReportSink for RepSink {
    fn order(&mut self, e: &OrderEvent) {
        self.orders.push(*e);
    }
    fn position(&mut self, e: &PositionReport) {
        self.pos.push(*e);
    }
    fn balance(&mut self, e: &BalanceReport) {
        self.bal.push(*e);
    }
    fn funding_settle(&mut self, _e: &FundingSettle) {
        self.funding += 1;
    }
    fn external_fill(&mut self, e: &ExternalFill) {
        self.ext.push(*e);
    }
    fn margin(&mut self, _hdr: &MarginReportHdr, _items: &[MarginItem]) {
        self.margin += 1;
    }
    fn account_config(&mut self, _hdr: &AccountConfigReportHdr, _lev: &[LeverageItem]) {
        self.margin += 1;
    }
    fn exch_id(&mut self, key: &[u8], owner: Option<&ClientOrderId>) -> ExchIdHint {
        self.exch_ids.push((key.to_vec(), owner.copied()));
        if owner.is_some() {
            ExchIdHint::Recorded
        } else {
            self.hint
        }
    }
}
