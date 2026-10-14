//! BIN-039: the M1.5 hot subset (bookTicker dispatch, peek and decode; place/cancel encode; WS API
//! answers) makes no allocation.

mod support;

use hft_event::{GwCmd, GwOp};
use hft_testkit::CountingAlloc;
use hft_types::{Px, Qty, Side, SlotId, Tif, Tsc, VenueId};
use hft_venue_proto::{MsgOp, TargetRef, VenueProtocol};
use support::{ACCT, BT1, RX, S_BTC, SBT1, U_BTC, WC, cfg, cid, hm1, rd};
use venue_binance::{FeedCtx, GwCtx, Spot, Templates, Usdm};

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc;

struct NoSink;
impl hft_venue_proto::MdSink for NoSink {
    fn bbo(&mut self, e: &hft_event::Bbo) {
        std::hint::black_box(e);
    }
    fn l2_begin(&mut self, _h: &hft_event::L2BatchHdr) {}
    fn l2_level(&mut self, _s: Side, _p: Px, _q: Qty) {}
    fn l2_end(&mut self) {}
    fn l2_snapshot(&mut self, _p: &hft_event::L2SnapshotPart<'_>) {}
    fn trade(&mut self, _e: &hft_event::Trade) {}
    fn mark(&mut self, _e: &hft_event::MarkPrice) {}
    fn index(&mut self, _e: &hft_event::IndexPrice) {}
    fn funding(&mut self, _e: &hft_event::Funding) {}
    fn liquidation(&mut self, _e: &hft_event::Liquidation) {}
}

/// Report sink that only counts (no allocation).
#[derive(Default)]
struct CountSink {
    orders: u64,
    ids: u64,
}
impl hft_venue_proto::ReportSink for CountSink {
    fn order(&mut self, e: &hft_event::OrderEvent) {
        std::hint::black_box(e);
        self.orders += 1;
    }
    fn position(&mut self, _e: &hft_event::PositionReport) {}
    fn balance(&mut self, _e: &hft_event::BalanceReport) {}
    fn funding_settle(&mut self, _e: &hft_event::FundingSettle) {}
    fn external_fill(&mut self, _e: &hft_event::ExternalFill) {}
    fn margin(&mut self, _h: &hft_event::MarginReportHdr, _i: &[hft_event::MarginItem]) {}
    fn account_config(&mut self, _h: &hft_event::AccountConfigReportHdr, _l: &[hft_event::LeverageItem]) {}
    fn exch_id(&mut self, key: &[u8], _o: Option<&hft_types::ClientOrderId>) -> hft_venue_proto::ExchIdHint {
        std::hint::black_box(key);
        self.ids += 1;
        hft_venue_proto::ExchIdHint::Recorded
    }
}

const ANSWERS: [&str; 4] = [
    r#"{"id":"p0000002a00000007","status":200,"result":{"orderId":325078477,"symbol":"BTCUSDT","status":"NEW","clientOrderId":"1000301020002a000000073d","updateTime":1790000000010},"rateLimits":[{"rateLimitType":"ORDERS","interval":"SECOND","intervalNum":10,"limit":300,"count":12},{"rateLimitType":"REQUEST_WEIGHT","interval":"MINUTE","intervalNum":1,"limit":2400,"count":5}]}"#,
    r#"{"id":"c0000002a00000007","status":200,"result":{"clientOrderId":"1000301020002a000000073d","orderId":325078477,"status":"CANCELED","symbol":"BTCUSDT","updateTime":1790000000020}}"#,
    r#"{"id":"p0000002a00000007","status":400,"error":{"code":-5022,"msg":"Due to the order could not be executed as maker, the Post Only order will be rejected."}}"#,
    r#"{"id":"q0000002a00000007","status":200,"result":{"orderId":325078477,"symbol":"BTCUSDT","status":"FILLED","clientOrderId":"1000301020002a000000073d","executedQty":"0.004","avgPrice":"65000.15"}}"#,
];

fn cmd(op: GwOp, inst: hft_types::InstId) -> GwCmd {
    GwCmd {
        decision_seq: 0,
        decided_tsc: Tsc(0),
        px: Px(6_500_010),
        qty: Qty(400_000),
        inst,
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

fn round(tu: &Templates, ts: &Templates, sink: &mut CountSink) {
    let r = rd();
    let mut body = [0u8; 2048];
    let mut md = NoSink;
    let route = Usdm::routes()[0].id;
    let c = Usdm::classify(route, MsgOp::Text, &BT1.as_bytes()[..64]);
    let mut ctx = FeedCtx::USDM;
    std::hint::black_box(Usdm::peek(r, &ctx, c, BT1.as_bytes()).unwrap());
    Usdm::decode_md(r, &mut ctx, c, BT1.as_bytes(), &RX, &mut md).unwrap();
    let c = Spot::classify(Spot::routes()[0].id, MsgOp::Text, &SBT1.as_bytes()[..64]);
    let mut sctx = FeedCtx::SPOT;
    std::hint::black_box(Spot::peek(r, &sctx, c, SBT1.as_bytes()).unwrap());
    Spot::decode_md(r, &mut sctx, c, SBT1.as_bytes(), &RX, &mut md).unwrap();
    Usdm::decode_md(
        r,
        &mut ctx,
        venue_binance::usdm::MdClass::DepthTopN,
        support::DP5.as_bytes(),
        &RX,
        &mut md,
    )
    .unwrap();
    Spot::decode_md(
        r,
        &mut sctx,
        venue_binance::spot::MdClass::JsonDepthTopN,
        support::SDP5.as_bytes(),
        &RX,
        &mut md,
    )
    .unwrap();
    Usdm::decode_md(
        r,
        &mut ctx,
        venue_binance::usdm::MdClass::AggTrade,
        br#"{"s":"BTCUSDT","p":"65000.1","q":"0.001","a":7,"T":1000,"E":1001,"m":true}"#,
        &RX,
        &mut md,
    )
    .unwrap();
    let ut = TargetRef {
        id: cid(VenueId::BINANCE_USDM, 3, 42, 7),
        exch_order_id: 0,
    };
    let st = TargetRef {
        id: cid(VenueId::BINANCE_SPOT, 3, 42, 7),
        exch_order_id: 0,
    };
    for op in [GwOp::Place, GwOp::Cancel] {
        Usdm::encode(r, tu, &cmd(op, U_BTC), &ut, &WC, &mut body).unwrap();
        Spot::encode(r, ts, &cmd(op, S_BTC), &st, &WC, &mut body).unwrap();
    }
    for a in ANSWERS {
        let c = Usdm::classify_report(MsgOp::Text, &a.as_bytes()[..64]);
        Usdm::decode_report(r, &mut GwCtx::default(), c, a.as_bytes(), &RX, sink).unwrap();
    }
}

#[test]
fn bin_039_hot_subset_zero_alloc() {
    assert!(CountingAlloc::is_installed());
    let tu = Usdm::build_templates(&cfg(1500), rd(), ACCT, &hm1()).unwrap();
    let ts = Spot::build_templates(&cfg(1500), rd(), ACCT, &hm1()).unwrap();
    let mut sink = CountSink::default();
    // Warm-up (one-time lazy initialization, if any).
    round(&tu, &ts, &mut sink);
    let s = CountingAlloc::scope();
    for _ in 0..1_000 {
        round(&tu, &ts, &mut sink);
    }
    assert_eq!(s.allocs(), 0);
    assert_eq!((sink.orders, sink.ids), (4 * 1_001, 1_001));
}
