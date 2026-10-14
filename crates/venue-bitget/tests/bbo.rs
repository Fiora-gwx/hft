use crate::support::*;
use hft_venue_proto::{
    RestReqBuf, VenueProtocol,
    testkit::{MdRec, VecSink},
};
use venue_bitget::{BitgetSpot as Spot, BitgetUsdtFutures as Fut, FeedCtx};
#[test]
fn bgt_050_bbo_public_request() {
    let mut r = RestReqBuf::default();
    assert!(Fut::bbo_snapshot_request(rd(), &FeedCtx::new(), &[F_BTC], &mut r).unwrap());
    assert_eq!(
        r.path_query.as_slice(),
        b"/api/v3/market/orderbook?category=USDT-FUTURES&symbol=BTCUSDT&limit=1"
    );
    assert!(r.headers.as_slice().is_empty());
    Spot::bbo_snapshot_request(rd(), &FeedCtx::new(), &[S_BTC], &mut r).unwrap();
    assert!(
        r.path_query
            .as_slice()
            .starts_with(b"/api/v3/market/orderbook?category=SPOT&")
    );
}
#[test]
fn bgt_051_bbo_invalid_inst_count() {
    for insts in [vec![], vec![F_BTC, F_ETH], vec![S_BTC]] {
        assert!(
            Fut::bbo_snapshot_request(rd(), &FeedCtx::new(), &insts, &mut RestReqBuf::default()).is_err()
        );
    }
}
#[test]
fn bgt_052_bbo_raw_decimal_numbers() {
    let mut s = VecSink::default();
    Fut::decode_bbo_snapshot(
        rd(),
        &mut FeedCtx::new(),
        &[F_BTC],
        br#"{"code":"00000","data":{"b":[[65000.1,0.001]],"a":[["65000.2","0.002"]],"ts":"1000"}}"#,
        &RX,
        &mut s,
    )
    .unwrap();
    assert!(
        matches!(&s.md[0], MdRec::Bbo(e) if e.bid_px.0 == 6500010 && e.bid_qty.0 == 100000 && e.update_id == 0 && e.h.ex_ts_ns == 1000000000)
    );
    s.md.clear();
    for body in [
        br#"{"code":"25000","data":{"b":[],"a":[],"ts":"1000"}}"#.as_slice(),
        br#"{"code":"00000","data":{"b":[[65000.1,0.001]],"a":[[1,"bad"]],"ts":"1000"}}"#,
    ] {
        assert!(Fut::decode_bbo_snapshot(rd(), &mut FeedCtx::new(), &[F_BTC], body, &RX, &mut s).is_err());
        assert!(s.md.is_empty());
    }
}

fn golden<P: VenueProtocol>(mut ctx: P::FeedCtx, inst: hft_types::InstId, input: &[u8], expected: &str) {
    let mut sink = VecSink::default();
    P::decode_bbo_snapshot(rd(), &mut ctx, &[inst], input, &RX, &mut sink).unwrap();
    assert_eq!(sink.md.len(), 1);
    let MdRec::Bbo(e) = sink.md[0] else {
        panic!("expected BBO")
    };
    let value = serde_json::json!({"kind":"Bbo", "venue":P::NAME,"inst":inst.0,"ex_ts_ns":e.h.ex_ts_ns,"ex_send_ns":e.h.ex_send_ns,"bid_px":e.bid_px.0.to_string(),"bid_qty":e.bid_qty.0.to_string(),"ask_px":e.ask_px.0.to_string(),"ask_qty":e.ask_qty.0.to_string(),"update_id":e.update_id,"flags":[]});
    assert_eq!(
        value,
        serde_json::from_str::<serde_json::Value>(expected).unwrap()
    );
}

#[test]
fn bgt_052_public_bbo_golden_0() {
    golden::<venue_bitget::BitgetUsdtFutures>(
        FeedCtx::new(),
        F_BTC,
        include_bytes!("../../../fixtures/bitget-usdt-futures/rest.orderbook/public_bbo.in"),
        include_str!("../../../fixtures/bitget-usdt-futures/rest.orderbook/public_bbo.expected.jsonl"),
    );
}

#[test]
fn bgt_052_public_bbo_golden_1() {
    golden::<venue_bitget::BitgetSpot>(
        FeedCtx::new(),
        S_BTC,
        include_bytes!("../../../fixtures/bitget-spot/rest.orderbook/public_bbo.in"),
        include_str!("../../../fixtures/bitget-spot/rest.orderbook/public_bbo.expected.jsonl"),
    );
}
