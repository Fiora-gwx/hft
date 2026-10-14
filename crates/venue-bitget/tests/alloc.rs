//! BGT-038: the hot entries (market dispatch, peek and decode; order encode; trade answers and the
//! private channels) make no allocation.

mod support;

use hft_event::{GwCmd, GwOp};
use hft_testkit::CountingAlloc;
use hft_types::{Px, Qty, Side, SlotId, Tif, Tsc};
use hft_venue_proto::{MsgOp, RouteId, TargetRef, VenueProtocol};
use support::{ACCT, F_BTC, FUT, RX, WC, b1v3, cfg, cid, key, rd};
use venue_bitget::{BitgetUsdtFutures as Fut, FeedCtx, GwCtx, Templates};

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc;

struct NoSink(u64);
impl hft_venue_proto::MdSink for NoSink {
    fn bbo(&mut self, e: &hft_event::Bbo) {
        std::hint::black_box(e);
        self.0 += 1;
    }
    fn l2_begin(&mut self, _h: &hft_event::L2BatchHdr) {
        self.0 += 1;
    }
    fn l2_level(&mut self, _s: Side, _p: Px, _q: Qty) {}
    fn l2_end(&mut self) {}
    fn l2_snapshot(&mut self, p: &hft_event::L2SnapshotPart<'_>) {
        std::hint::black_box(p);
        self.0 += 1;
    }
    fn trade(&mut self, _e: &hft_event::Trade) {
        self.0 += 1;
    }
    fn mark(&mut self, _e: &hft_event::MarkPrice) {
        self.0 += 1;
    }
    fn index(&mut self, _e: &hft_event::IndexPrice) {
        self.0 += 1;
    }
    fn funding(&mut self, _e: &hft_event::Funding) {
        self.0 += 1;
    }
    fn liquidation(&mut self, _e: &hft_event::Liquidation) {}
}

/// Report sink that only counts (no allocation).
#[derive(Default)]
struct CountSink {
    n: u64,
}
impl hft_venue_proto::ReportSink for CountSink {
    fn order(&mut self, e: &hft_event::OrderEvent) {
        std::hint::black_box(e);
        self.n += 1;
    }
    fn position(&mut self, _e: &hft_event::PositionReport) {
        self.n += 1;
    }
    fn balance(&mut self, _e: &hft_event::BalanceReport) {
        self.n += 1;
    }
    fn funding_settle(&mut self, _e: &hft_event::FundingSettle) {}
    fn external_fill(&mut self, _e: &hft_event::ExternalFill) {
        self.n += 1;
    }
    fn margin(&mut self, _h: &hft_event::MarginReportHdr, _i: &[hft_event::MarginItem]) {}
    fn account_config(&mut self, _h: &hft_event::AccountConfigReportHdr, _l: &[hft_event::LeverageItem]) {}
    fn exch_id(&mut self, key: &[u8], _o: Option<&hft_types::ClientOrderId>) -> hft_venue_proto::ExchIdHint {
        std::hint::black_box(key);
        hft_venue_proto::ExchIdHint::Recorded
    }
}

const TOPN: &str = r#"{"data":[{"a":[["65000.2","1"],["65000.3","2"]],"b":[["65000.1","1"],["65000","2"]],"seq":777,"ts":"1790000000200"}],"arg":{"instType":"usdt-futures","symbol":"BTCUSDT","topic":"books5"},"action":"snapshot","ts":1790000000202}"#;
const TRADE: &str = r#"{"data":[{"p":"65000.1","S":"buy","T":"1790000000100","v":"0.002","i":"1260903622036942849","L":"1","isRPI":"no"}],"arg":{"instType":"usdt-futures","symbol":"BTCUSDT","topic":"publicTrade"},"action":"update","ts":1790000000102}"#;
const TICKER: &str = r#"{"data":[{"markPrice":"65001.2","indexPrice":"64990.5","fundingRate":"0.0001","nextFundingTime":"1790006400000","lastPrice":"65000"}],"arg":{"instType":"usdt-futures","symbol":"BTCUSDT","topic":"ticker"},"action":"snapshot","ts":1790000000301}"#;
const BOOKS: &str = r#"{"data":[{"a":[["65000.2","3"]],"b":[["65000.1","0"]],"pseq":100,"seq":101,"ts":"1790000000400"}],"arg":{"instType":"usdt-futures","symbol":"BTCUSDT","topic":"books"},"action":"update","ts":1790000000401}"#;

const REPORTS: [&str; 5] = [
    r#"{"event":"trade","id":"p0000002a00000007","category":"usdt-futures","topic":"place-order","args":[{"symbol":"BTCUSDT","orderId":"1234567890123","clientOid":"100030a020002a000000078d","cTime":"1790000000008"}],"code":"0","msg":"success","ts":"1790000000076"}"#,
    r#"{"event":"error","id":"p0000002a00000007","code":"25202","msg":"Insufficient balance"}"#,
    r#"{"action":"snapshot","arg":{"instType":"UTA","topic":"order"},"data":[{"category":"usdt-futures","symbol":"BTCUSDT","orderId":"1234567890123","clientOid":"100030a020002a000000078d","price":"65000.1","qty":"0.004","tradeSide":"buy_single","orderType":"limit","timeInForce":"gtc","side":"buy","cumExecQty":"0.004","cumExecValue":"260","avgPrice":"65000","orderStatus":"filled","cancelReason":"","feeDetail":[{"feeCoin":"USDT","fee":"0.04"}],"createdTime":"1790000000400","updatedTime":"1790000000500"}],"ts":1790000000501}"#,
    r#"{"data":[{"symbol":"BTCUSDT","size":"0.01","posSide":"long","avgPrice":"64000","unrealisedPnl":"10.5","liqPrice":"52000.5","updatedTime":"1790000000700"}],"arg":{"instType":"UTA","topic":"position"},"action":"snapshot","ts":1790000000701}"#,
    r#"{"data":[{"totalEquity":"1","coin":[{"coin":"USDT","available":"8000.5","locked":"100","balance":"8100.5"}]}],"arg":{"instType":"UTA","topic":"account"},"action":"snapshot","ts":1790000000800}"#,
];

fn cmd(op: GwOp) -> GwCmd {
    GwCmd {
        decision_seq: 0,
        decided_tsc: Tsc(0),
        px: Px(6_500_010),
        qty: Qty(400_000),
        inst: F_BTC,
        slot: SlotId(42),
        r#gen: 7,
        target_slot: SlotId(42),
        op,
        account: ACCT,
        side: Side::Buy,
        tif: Tif::PostOnly,
        oflags: 0,
        _pad: [0; 2],
        arg: 0,
        arg2: 0,
    }
}

#[test]
fn bgt_038_hot_path_no_alloc() {
    assert!(CountingAlloc::is_installed());
    let t = Fut::build_templates(&cfg(), rd(), ACCT, &key()).unwrap();
    let mut fctx = FeedCtx::new();
    let mut gctx = GwCtx::new();
    let mut md = NoSink(0);
    let mut rep = CountSink::default();
    // Warm-up (lazy initialization, the payload string).
    round_static(&t, &mut fctx, &mut gctx, &mut md, &mut rep);
    let s = CountingAlloc::scope();
    for _ in 0..1_000 {
        round_static(&t, &mut fctx, &mut gctx, &mut md, &mut rep);
    }
    assert_eq!(s.allocs(), 0);
    assert!(md.0 > 0 && rep.n > 0);
}

/// `round` without building the `books1` payload (the `format!` would allocate).
fn round_static(t: &Templates, fctx: &mut FeedCtx, gctx: &mut GwCtx, md: &mut NoSink, rep: &mut CountSink) {
    static B1: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    let b1 = B1.get_or_init(|| b1v3("123456789", "\"1790000000123\"", "1790000000125"));
    let r = rd();
    for (route, p) in [(0u8, b1.as_str()), (1, TOPN), (2, TRADE), (3, TICKER), (4, BOOKS)] {
        let c = Fut::classify(RouteId(route), MsgOp::Text, &p.as_bytes()[..64]);
        std::hint::black_box(Fut::peek(r, fctx, c, p.as_bytes()).unwrap());
        Fut::decode_md(r, fctx, c, p.as_bytes(), &RX, md).unwrap();
    }
    let tgt = TargetRef {
        id: cid(FUT, 3, 42, 7),
        exch_order_id: 0,
    };
    let mut body = [0u8; 2048];
    for op in [GwOp::Place, GwOp::Cancel, GwOp::Amend] {
        Fut::encode(r, t, &cmd(op), &tgt, &WC, &mut body).unwrap();
    }
    for p in REPORTS {
        let c = Fut::classify_report(MsgOp::Text, &p.as_bytes()[..64]);
        Fut::decode_report(r, gctx, c, p.as_bytes(), &RX, rep).unwrap();
    }
}
