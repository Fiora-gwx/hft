use crate::support::*;
use hft_event::{GwCmd, GwOp};
use hft_types::{Px, Qty, Side, SlotId, Tif, Tsc};
use hft_venue_proto::{
    TargetRef, VenueProtocol,
    testkit::{MdRec, VecSink},
};
use venue_bitget::{BitgetUsdtFutures as Fut, FeedCtx, GwCtx, mock::*};

#[test]
fn bgt_075_mock_order_roundtrip() {
    let t = Fut::build_templates(&cfg(), rd(), ACCT, &key()).unwrap();
    let c = GwCmd {
        decision_seq: 0,
        decided_tsc: Tsc(0),
        px: Px(6500010),
        qty: Qty(400000),
        inst: F_BTC,
        slot: SlotId(42),
        r#gen: 7,
        target_slot: SlotId(42),
        op: GwOp::Place,
        account: ACCT,
        side: Side::Buy,
        tif: Tif::Ioc,
        oflags: 0,
        _pad: [0; 2],
        arg: 0,
        arg2: 0,
    };
    let mut bytes = [0u8; 2048];
    let n = Fut::encode(
        rd(),
        &t,
        &c,
        &TargetRef {
            id: cid(FUT, 3, 42, 7),
            exch_order_id: 0,
        },
        &WC,
        &mut bytes,
    )
    .unwrap();
    let r = decode_order_frame(MockVenueKind::Futures, &bytes[..n]).unwrap();
    assert_eq!(
        (&*r.id, &*r.args[0].price, &*r.args[0].qty),
        ("p0000002a00000007", "65000.1", "0.004")
    );
    assert!(decode_order_frame(MockVenueKind::Spot, &bytes[..n]).is_err());
    let mut reply = Vec::new();
    encode_order_event(
        MockVenueKind::Futures,
        &MockOrderResp::Reply {
            id: &r.id,
            topic: &r.topic,
            code: "0",
            client_oid: &r.args[0].client_oid,
            order_id: 123,
            ts_ms: 1000,
        },
        &mut reply,
    );
    let mut sink = RepSink::default();
    let class = Fut::classify_report(hft_venue_proto::MsgOp::Text, &reply[..64]);
    Fut::decode_report(rd(), &mut GwCtx::new(), class, &reply, &RX, &mut sink).unwrap();
    assert_eq!(sink.orders[0].slot, c.slot);
    assert_eq!(sink.orders[0].exch_order_id, 123);
}

#[test]
fn bgt_052_mock_bbo_ws_and_rest() {
    let bids = [["65000.1", "0.001"]];
    let asks = [["65000.2", "0.002"]];
    let mut bytes = Vec::new();
    encode_market(
        MockVenueKind::Futures,
        &MockMarket::Book {
            symbol: "BTCUSDT",
            topic: "books1",
            bids: &bids,
            asks: &asks,
            seq: 7,
            prev: 0,
            ts_ms: 1000,
        },
        &mut bytes,
    );
    let mut sink = VecSink::default();
    let mut ctx = FeedCtx::new();
    let class = Fut::classify(
        hft_venue_proto::RouteId(0),
        hft_venue_proto::MsgOp::Text,
        &bytes[..64],
    );
    Fut::decode_md(rd(), &mut ctx, class, &bytes, &RX, &mut sink).unwrap();
    assert!(matches!(sink.md[0], MdRec::Bbo(e) if e.bid_px.0 == 6500010 && e.update_id == 7));
    sink.md.clear();
    assert_eq!(
        encode_rest_response(
            MockVenueKind::Futures,
            &MockRestReq::Book {
                bids: &bids,
                asks: &asks,
                ts_ms: 1000
            },
            &mut bytes
        ),
        200
    );
    Fut::decode_bbo_snapshot(rd(), &mut ctx, &[F_BTC], &bytes, &RX, &mut sink).unwrap();
    assert!(matches!(sink.md[0], MdRec::Bbo(e) if e.bid_px.0 == 6500010 && e.update_id == 0));
}
