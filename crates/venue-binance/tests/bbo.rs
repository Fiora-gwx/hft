use crate::support::*;
use hft_venue_proto::{
    RestReqBuf, VenueProtocol,
    testkit::{MdRec, VecSink},
};
use venue_binance::{FeedCtx, Spot, Usdm};

#[test]
fn bin_050_bbo_request() {
    let mut req = RestReqBuf::default();
    assert!(Usdm::bbo_snapshot_request(rd(), &FeedCtx::USDM, &[U_BTC], &mut req).unwrap());
    assert_eq!(
        req.path_query.as_slice(),
        b"/fapi/v1/ticker/bookTicker?symbol=BTCUSDT"
    );
    assert!(req.headers.as_slice().is_empty());
}
#[test]
fn bin_051_bbo_batch_rejected() {
    for insts in [vec![], vec![U_BTC, U_ETH], vec![S_BTC]] {
        assert!(
            Usdm::bbo_snapshot_request(rd(), &FeedCtx::USDM, &insts, &mut RestReqBuf::default()).is_err()
        );
    }
}
#[test]
fn bin_052_spot_bbo_symbols() {
    let mut req = RestReqBuf::default();
    Spot::bbo_snapshot_request(rd(), &FeedCtx::SPOT, &[S_BTC, S_BNB], &mut req).unwrap();
    assert_eq!(
        req.path_query.as_slice(),
        b"/api/v3/ticker/bookTicker?symbols=%5B%22BTCUSDT%22%2C%22BNBUSDT%22%5D"
    );
}
const BBO: &[u8] = br#"{"symbol":"BTCUSDT","bidPrice":"65000.1","bidQty":"0.001","askPrice":"65000.2","askQty":"0.002","time":1000}"#;
#[test]
fn bin_053_bbo_rest_decode() {
    let mut s = VecSink::default();
    assert_eq!(
        Usdm::decode_bbo_snapshot(rd(), &mut { FeedCtx::USDM }, &[U_BTC], BBO, &RX, &mut s).unwrap(),
        1
    );
    assert!(
        matches!(&s.md[0], MdRec::Bbo(e) if e.bid_px.0 == 6500010 && e.bid_qty.0 == 100000 && e.update_id == 0 && e.h.ex_ts_ns == 1000000000 && e.h.rx_tsc == RX.rx_tsc)
    );
}
#[test]
fn bin_054_spot_bbo_rest_decode() {
    let mut s = VecSink::default();
    let body = [b"[".as_slice(), BBO, b"]"].concat();
    Spot::decode_bbo_snapshot(rd(), &mut { FeedCtx::SPOT }, &[S_BTC], &body, &RX, &mut s).unwrap();
    assert!(matches!(&s.md[0], MdRec::Bbo(e) if e.h.inst == S_BTC && e.h.ex_ts_ns == 0 && e.update_id == 0));
}
#[test]
fn bin_055_bbo_error_atomic() {
    let valid = core::str::from_utf8(BBO).unwrap();
    let body = format!("[{valid},{}]", valid.replace("65000.2", "bad"));
    let mut s = VecSink::default();
    assert!(
        Spot::decode_bbo_snapshot(
            rd(),
            &mut { FeedCtx::SPOT },
            &[S_BTC],
            body.as_bytes(),
            &RX,
            &mut s
        )
        .is_err()
    );
    assert!(s.md.is_empty());
    assert!(
        Usdm::decode_bbo_snapshot(
            rd(),
            &mut { FeedCtx::USDM },
            &[U_BTC],
            br#"{"code":-1}"#,
            &RX,
            &mut s
        )
        .is_err()
    );
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
fn bin_054_public_bbo_golden_0() {
    golden::<venue_binance::Usdm>(
        FeedCtx::USDM,
        U_BTC,
        include_bytes!("../../../fixtures/binance-usdm/rest.bookTicker/public_bbo.in"),
        include_str!("../../../fixtures/binance-usdm/rest.bookTicker/public_bbo.expected.jsonl"),
    );
}

#[test]
fn bin_054_public_bbo_golden_1() {
    golden::<venue_binance::Spot>(
        FeedCtx::SPOT,
        S_BTC,
        include_bytes!("../../../fixtures/binance-spot/rest.bookTicker/public_bbo.in"),
        include_str!("../../../fixtures/binance-spot/rest.bookTicker/public_bbo.expected.jsonl"),
    );
}

#[test]
fn bin_056_snapshot_isolation() {
    fn run<P: VenueProtocol<FeedCtx = FeedCtx>>(
        mut used: FeedCtx,
        class: P::MdClass,
        payload: &[u8],
        inst: hft_types::InstId,
        body: &[u8],
    ) {
        let mut sink = VecSink::default();
        for _ in 0..10_000 {
            sink.md.clear();
            P::decode_md(rd(), &mut used, class, payload, &RX, &mut sink).unwrap();
        }
        let before = used;
        let mut a = VecSink::default();
        let mut b = VecSink::default();
        let mut fresh = if P::VENUE == hft_types::VenueId::BINANCE_USDM {
            FeedCtx::USDM
        } else {
            FeedCtx::SPOT
        };
        P::decode_bbo_snapshot(rd(), &mut fresh, &[inst], body, &RX, &mut a).unwrap();
        P::decode_bbo_snapshot(rd(), &mut used, &[inst], body, &RX, &mut b).unwrap();
        assert_eq!(
            a.md.iter().map(MdRec::bytes).collect::<Vec<_>>(),
            b.md.iter().map(MdRec::bytes).collect::<Vec<_>>()
        );
        assert_eq!(used, before);
    }
    run::<Usdm>(FeedCtx::USDM, venue_binance::usdm::MdClass::MarkPrice,
        br#"{"e":"markPriceUpdate","E":1790000000125,"s":"BTCUSDT","p":"65001.23","i":"65000.50","r":"0.00010000","T":1790006400000}"#,
        U_BTC, BBO);
    run::<Spot>(
        FeedCtx::SPOT,
        venue_binance::spot::MdClass::JsonBookTicker,
        SBT1.as_bytes(),
        S_BTC,
        &[b"[".as_slice(), BBO, b"]"].concat(),
    );
}
