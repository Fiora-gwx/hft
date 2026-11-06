//! Shared fixtures of the venue-coinbase tests (TESTS.md "记号"): `RD-CB`, `RX`, fixture reading.
#![allow(dead_code)]

use std::sync::OnceLock;

use hft_refdata::{
    AssetClass, AssetSpec, Exchange, FeePair, FeeSpec, InstKind, InstSpec, InstStatus, RateLimit, RdbBuilder,
    RefData, StaleGuardKind, TsUnit, VenueSpec,
};
use hft_types::{AssetExp, AssetId, ConnId, InstId, Notional, Px, Qty, Rate, VenueId};
use hft_venue_proto::{RxMeta, VenueProtocol};
use venue_coinbase::CoinbaseSpot;

pub const USD: AssetId = AssetId(1);
pub const BTC: AssetId = AssetId(2);
pub const BTC_USD: InstId = InstId(1);
/// An inst of another venue whose symbol is also `BTC-USD`.
pub const FOREIGN: InstId = InstId(2);

/// `RD-CB`: USD (Stable) and BTC (px −4, qty −8); `COINBASE_SPOT` `BTC-USD` = InstId 1 (tick
/// 0.01, lot 1e-8); `BINANCE_SPOT` `BTC-USD` = InstId 2 (a foreign inst with the same symbol).
pub fn rd() -> &'static RefData {
    static RD: OnceLock<RefData> = OnceLock::new();
    RD.get_or_init(|| RefData::load(&blob(), None).unwrap_or_else(|e| panic!("RD-CB rejected: {e:?}")))
}

pub fn blob() -> Vec<u8> {
    let mut b = RdbBuilder::new(1);
    b.created_utc_ns(1_793_980_000_000_000_000);
    for (id, name, class, px, qty) in [
        (USD, "USD", AssetClass::Stable, -6, -6),
        (BTC, "BTC", AssetClass::Crypto, -4, -8),
    ] {
        b.asset(AssetSpec {
            id,
            name: name.into(),
            class,
            exp: AssetExp { px_exp: px, qty_exp: qty },
            px_max: Px(1_000_000_000_000),
            qty_max: Qty(1_000_000_000_000_000),
            dust: Qty(1),
        });
    }
    for (v, exchange, ms, guard) in [
        (VenueId::BINANCE_SPOT, Exchange::Binance, 5u16, StaleGuardKind::RecvWindow),
        (VenueId::COINBASE_SPOT, Exchange::Coinbase, 8, StaleGuardKind::None),
    ] {
        b.venue(VenueSpec {
            id: v,
            exchange,
            product: InstKind::Spot,
            ws_max_streams_per_conn: 64,
            ws_max_in_msgs_per_s: ms,
            ws_max_conn_age_s: 86_400,
            ws_new_conn_per_5min_ip: 300,
            app_ping_interval_s: 0,
            ts_unit: TsUnit::Us,
            client_id_max_len: 36,
            client_id_prefix: "".into(),
            max_batch: 1,
            max_inflight_amend: 1,
            crossed_tolerance_default: 0,
            stale_guard: guard,
        });
        b.fee(FeeSpec {
            venue: v,
            tier: 0,
            fee: FeePair { maker: Rate(0), taker: Rate(0) },
        });
    }
    b.caps(venue_binance_caps());
    b.caps(CoinbaseSpot::default_caps(|_| None));
    for r in CoinbaseSpot::rate_rules() {
        b.limit(
            VenueId::COINBASE_SPOT,
            RateLimit {
                scope: r.scope,
                bucket: r.bucket,
                limit: r.limit,
                window_ms: r.window_ms,
                default_cost: 1,
            },
        );
    }
    for (id, venue) in [(BTC_USD, VenueId::COINBASE_SPOT), (FOREIGN, VenueId::BINANCE_SPOT)] {
        b.inst(InstSpec {
            id,
            venue,
            symbol: "BTC-USD".into(),
            status: InstStatus::Trading,
            kind: InstKind::Spot,
            asset: BTC,
            quote: USD,
            tick: 100,
            lot: 1,
            min_qty: Qty(1),
            max_qty: Qty(100_000_000_000),
            min_notional: Notional(1_000_000),
            book_window: 4096,
            tick_str: "0.01".into(),
            lot_str: "0.00000001".into(),
            min_qty_str: "0.00000001".into(),
            ..InstSpec::default()
        });
    }
    b.finish()
}

/// A minimal spot capability row for the foreign venue (no venue-binance dependency).
fn venue_binance_caps() -> hft_refdata::VenueCaps {
    let mut c = hft_refdata::VenueCaps::none(VenueId::BINANCE_SPOT);
    c.market = hft_types::MarketKind::Spot;
    c
}

/// `RX`: fixed receive metadata copied into every `MarketHdr`.
pub const RX: RxMeta = RxMeta {
    rx_tsc: 0x1234_5678,
    hw_rx_ns: 0,
    sw_rx_ns: 1_793_980_800_000_000_123,
    prev_override: 0,
    conn: ConnId(3),
    account: hft_types::AccountId(0),
    _pad: [0; 2],
    ts_flags: 0,
    rx_empty_gap_ns: 77,
    _pad2: 0,
};

/// `fixtures/coinbase-spot/<msg_type>/<case>`: input bytes (trailing newline stripped) and the
/// expected lines.
pub fn fixture(msg_type: &str, case: &str) -> (Vec<u8>, Vec<serde_json::Value>) {
    let dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/coinbase-spot")
        .join(msg_type);
    let mut input = std::fs::read(dir.join(format!("{case}.in"))).unwrap();
    while input.last() == Some(&b'\n') {
        input.pop();
    }
    let expected = std::fs::read_to_string(dir.join(format!("{case}.expected.jsonl")))
        .unwrap()
        .lines()
        .filter(|l| !l.is_empty())
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    (input, expected)
}

/// Lines of a `.jsonl` file under `fixtures/coinbase-spot/`.
pub fn jsonl(rel: &str) -> Vec<serde_json::Value> {
    let p = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/coinbase-spot")
        .join(rel);
    std::fs::read_to_string(p)
        .unwrap()
        .lines()
        .filter(|l| !l.is_empty())
        .map(|l| serde_json::from_str(l).unwrap())
        .collect()
}
