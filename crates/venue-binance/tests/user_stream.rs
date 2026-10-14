use super::support::*;
use hft_event::OrderEventKind;
use hft_venue_proto::{MsgOp, ReportCtl, VenueProtocol};
use venue_binance::{GwCtx, Usdm};
fn otu(x: &str, status: &str) -> String {
    format!(
        r#"{{"e":"ORDER_TRADE_UPDATE","o":{{"s":"BTCUSDT","c":"1000301020002a000000073d","S":"BUY","o":"LIMIT","x":"{x}","X":"{status}","i":325078477,"l":"0.001","z":"0.003","L":"65000.1","ap":"65000.05","n":"0.0130","N":"USDT","rp":"0","T":1790000000010,"t":9001}}}}"#
    )
}
#[test]
fn bin_101_fill_fields() {
    let p = otu("TRADE", "PARTIALLY_FILLED");
    let mut ctx = GwCtx::default();
    let mut sink = RepSink::default();
    let c = Usdm::classify_report(MsgOp::Text, p.as_bytes());
    let o = Usdm::decode_report(rd(), &mut ctx, c, p.as_bytes(), &RX, &mut sink).unwrap();
    assert_eq!(o.events, 1);
    let e = sink.orders[0];
    assert_eq!(e.what, OrderEventKind::PartialFill);
    assert_eq!((e.slot.0, e.r#gen, e.epoch), (42, 7, 3));
    assert_eq!(
        (e.qty.0, e.cum_filled.0, e.px.0, e.avg_px.0, e.fee.0),
        (100000, 300000, 6500010, 6500005, 1300000)
    );
    assert_eq!(e.ex_ts_ns, 1790000000010000000);
    assert_eq!(
        (e.cash_qty.0, e.cash_sum.0),
        (0, 0),
        "per-fill rp is not cumulative cash"
    );
}
#[test]
fn bin_102_trade_dedup() {
    let p = otu("TRADE", "FILLED");
    let mut ctx = GwCtx::default();
    let mut sink = RepSink::default();
    let c = Usdm::classify_report(MsgOp::Text, p.as_bytes());
    for _ in 0..2 {
        Usdm::decode_report(rd(), &mut ctx, c, p.as_bytes(), &RX, &mut sink).unwrap();
    }
    assert_eq!(sink.orders.len(), 1);
}
#[test]
fn bin_100_otu_exec_type() {
    for (x, status, want) in [
        ("NEW", "NEW", OrderEventKind::Ack),
        ("CANCELED", "CANCELED", OrderEventKind::CancelAck),
        ("EXPIRED", "EXPIRED", OrderEventKind::Expired),
        ("AMENDMENT", "NEW", OrderEventKind::AmendAck),
    ] {
        let p = otu(x, status);
        let c = Usdm::classify_report(MsgOp::Text, p.as_bytes());
        let mut sink = RepSink::default();
        Usdm::decode_report(rd(), &mut GwCtx::default(), c, p.as_bytes(), &RX, &mut sink).unwrap();
        assert_eq!(sink.orders[0].what, want);
    }
}
#[test]
fn bin_118_stream_end_events() {
    let p = br#"{"e":"listenKeyExpired","E":1790001800000}"#;
    let c = Usdm::classify_report(MsgOp::Text, p);
    let mut sink = RepSink::default();
    let o = Usdm::decode_report(rd(), &mut GwCtx::default(), c, p, &RX, &mut sink).unwrap();
    assert_eq!(o.ctl, ReportCtl::UserStreamEnded);
    assert_eq!(o.events, 0);
}
