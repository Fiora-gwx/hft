//! BIN-020–BIN-026, BIN-035, BIN-036: sequence rules, `peek`, `bookTicker` decoding, control hints.

use crate::support;

use hft_event::Bbo;
use hft_types::{ConnId, InstId, Px, Qty, VenueId};
use hft_venue_proto::testkit::{MdRec, VecSink};
use hft_venue_proto::{ControlHint, DecodeErr, MdOutcome, ResyncMethod, SeqRule, VenueProtocol};
use support::{BT1, DP5, RX, S_BNB, S_BTC, SBT1, SDP5, U_BTC, rd};
use venue_binance::{FeedCtx, Spot, Usdm, spot, usdm};

#[test]
fn bin_037_market_receive_metadata() {
    fn check<P: VenueProtocol>(mut ctx: P::FeedCtx, samples: &[(P::MdClass, &[u8])]) {
        for hw in [0, RX.hw_rx_ns] {
            let rx = hft_venue_proto::RxMeta { hw_rx_ns: hw, ..RX };
            for &(class, body) in samples {
                let mut sink = VecSink::default();
                P::decode_md(rd(), &mut ctx, class, body, &rx, &mut sink).unwrap();
                assert!(!sink.md.is_empty());
                for event in &sink.md {
                    let h = event.market_hdr().unwrap();
                    assert_eq!(
                        (
                            h.venue,
                            h.conn,
                            h.rx_tsc,
                            h.hw_rx_ns,
                            h.sw_rx_ns,
                            h.rx_empty_gap_ns
                        ),
                        (P::VENUE, rx.conn, rx.rx_tsc, hw, rx.sw_rx_ns, rx.rx_empty_gap_ns)
                    );
                }
            }
        }
    }
    check::<Usdm>(FeedCtx::USDM, &[
        (usdm::MdClass::BookTicker, BT1.as_bytes()),
        (usdm::MdClass::DepthTopN, DP5.as_bytes()),
        (usdm::MdClass::AggTrade, br#"{"e":"aggTrade","s":"BTCUSDT","a":9,"p":"65000.1","q":"0.001","T":1000,"E":1001,"m":false}"#),
        (usdm::MdClass::MarkPrice, br#"{"e":"markPriceUpdate","E":1790000000125,"s":"BTCUSDT","p":"65001.23","i":"65000.50","r":"0.00010000","T":1790006400000}"#),
        (usdm::MdClass::ForceOrder, br#"{"e":"forceOrder","E":1001,"o":{"s":"BTCUSDT","ap":"65000.1","z":"0.001","S":"SELL","T":1000}}"#),
    ]);
    check::<Spot>(FeedCtx::SPOT, &[
        (spot::MdClass::JsonBookTicker, SBT1.as_bytes()),
        (spot::MdClass::JsonDepthTopN, SDP5.as_bytes()),
        (spot::MdClass::JsonTrade, br#"{"e":"trade","s":"BTCUSDT","t":10,"p":"65000.1","q":"0.001","T":1000,"E":1001,"m":false}"#),
    ]);
}

#[test]
fn bin_032_markprice_three_events() {
    let mut ctx = FeedCtx::USDM;
    let mut sink = VecSink::default();
    let msg = br#"{"e":"markPriceUpdate","E":1790000000125,"s":"BTCUSDT","p":"65001.23","i":"65000.50","r":"0.00010000","T":1790006400000}"#;
    let result = Usdm::decode_md(rd(), &mut ctx, usdm::MdClass::MarkPrice, msg, &RX, &mut sink).unwrap();
    assert_eq!(result.events, 3);
    assert!(matches!(&sink.md[0], MdRec::Mark(m) if m.px == Px(6_500_123)));
    assert!(matches!(&sink.md[1], MdRec::Index(m) if m.px == Px(6_500_050)));
    assert!(
        matches!(&sink.md[2], MdRec::Funding(f) if f.rate.0 == 100_000 && f.interval_s == 0
        && f.next_funding_ns == 1_790_006_400_000_000_000 && f.h.rx_tsc == RX.rx_tsc)
    );
}

#[test]
fn bin_033_funding_interval() {
    let mut ctx = FeedCtx::USDM;
    for (offset, expected) in [
        (0, 0),
        (0, 0),
        (28_800_000, 28_800),
        (28_800_000, 28_800),
        (43_200_000, 14_400),
    ] {
        let mut sink = VecSink::default();
        let msg = format!(
            r#"{{"E":1790000000125,"s":"BTCUSDT","p":"65001.23","i":"65000.50","r":"0.0001","T":{}}}"#,
            1_790_006_400_000_i64 + offset
        );
        Usdm::decode_md(
            rd(),
            &mut ctx,
            usdm::MdClass::MarkPrice,
            msg.as_bytes(),
            &RX,
            &mut sink,
        )
        .unwrap();
        assert!(matches!(&sink.md[2], MdRec::Funding(f) if f.interval_s == expected));
    }
}

#[test]
fn bin_020_seq_rule_resync() {
    use SeqRule::*;
    let u = [
        ("BIN-020.1", usdm::MdClass::BookTicker, Monotonic),
        ("BIN-020.2", usdm::MdClass::DepthTopN, Snapshot),
        ("BIN-020.3", usdm::MdClass::AggTrade, Monotonic),
        ("BIN-020.4", usdm::MdClass::MarkPrice, ByTime),
        ("BIN-020.5", usdm::MdClass::ForceOrder, SeqRule::None),
    ];
    for (row, c, r) in u {
        assert_eq!(
            (Usdm::seq_rule(c), Usdm::resync_method(c)),
            (r, ResyncMethod::None),
            "{row}"
        );
    }
    let s = [
        ("BIN-020.6", spot::MdClass::JsonBookTicker, Monotonic),
        ("BIN-020.7", spot::MdClass::JsonDepthTopN, Snapshot),
        ("BIN-020.8", spot::MdClass::JsonTrade, Monotonic),
    ];
    for (row, c, r) in s {
        assert_eq!(
            (Spot::seq_rule(c), Spot::resync_method(c)),
            (r, ResyncMethod::None),
            "{row}"
        );
    }
}

#[test]
fn bin_021_peek_ids() {
    let r = rd();
    let u = FeedCtx::USDM;
    let s = FeedCtx::SPOT;
    let p = Usdm::peek(r, &u, usdm::MdClass::BookTicker, BT1.as_bytes()).unwrap();
    assert_eq!(
        (p.inst, p.ids.prev_id, p.ids.last_id),
        (U_BTC, 0, 400_900_217),
        "BIN-021.1"
    );
    let doge = BT1.replace("BTCUSDT", "DOGEUSDT");
    let p = Usdm::peek(r, &u, usdm::MdClass::BookTicker, doge.as_bytes()).unwrap();
    assert_eq!(p.inst, InstId(0), "BIN-021.2");
    let agg = r#"{"e":"aggTrade","E":1790000000125,"s":"BTCUSDT","a":26129,"p":"65000.1","q":"0.001","f":1,"l":1,"T":1790000000123,"m":true}"#;
    let p = Usdm::peek(r, &u, usdm::MdClass::AggTrade, agg.as_bytes()).unwrap();
    assert_eq!((p.ids.prev_id, p.ids.last_id), (0, 26_129), "BIN-021.3");
    let mark = r#"{"e":"markPriceUpdate","E":1790000000125,"s":"BTCUSDT","p":"65000.1","i":"65000.0","P":"65000.2","r":"0.0001","T":1790006400000}"#;
    let p = Usdm::peek(r, &u, usdm::MdClass::MarkPrice, mark.as_bytes()).unwrap();
    assert_eq!(
        (p.ids.prev_id, p.ids.last_id),
        (0, 1_790_000_000_125_000_000),
        "BIN-021.4"
    );
    let force = r#"{"e":"forceOrder","E":1790000000125,"o":{"s":"BTCUSDT","S":"SELL","o":"LIMIT","f":"IOC","q":"0.014","p":"64000.0","ap":"64100.0","X":"FILLED","l":"0.014","z":"0.014","T":1790000000120}}"#;
    let p = Usdm::peek(r, &u, usdm::MdClass::ForceOrder, force.as_bytes()).unwrap();
    assert_eq!((p.inst, p.ids.prev_id, p.ids.last_id), (U_BTC, 0, 0), "BIN-021.5");
    let p = Usdm::peek(r, &u, usdm::MdClass::DepthTopN, DP5.as_bytes()).unwrap();
    assert_eq!(p.ids.last_id, 7_000_000_001, "BIN-021.6");
    let p = Spot::peek(r, &s, spot::MdClass::JsonBookTicker, SBT1.as_bytes()).unwrap();
    assert_eq!((p.inst, p.ids.last_id), (S_BTC, 400_900_217), "BIN-021.7");
    let p = Spot::peek(r, &s, spot::MdClass::JsonDepthTopN, SDP5.as_bytes()).unwrap();
    assert_eq!((p.inst, p.ids.last_id), (S_BNB, 160), "BIN-021.8");
    let doge = SDP5.replace("bnbusdt", "dogeusdt");
    let p = Spot::peek(r, &s, spot::MdClass::JsonDepthTopN, doge.as_bytes()).unwrap();
    assert_eq!(p.inst, InstId(0), "BIN-021.9");
}

#[test]
fn bin_022_peek_skips_px_qty() {
    let r = rd();
    let p = BT1.replace(r#""b":"65000.1""#, r#""b":"abc""#);
    let pk = Usdm::peek(r, &FeedCtx::USDM, usdm::MdClass::BookTicker, p.as_bytes()).unwrap();
    assert_eq!((pk.inst, pk.ids.last_id), (U_BTC, 400_900_217));
    let mut sink = VecSink::default();
    let mut ctx = FeedCtx::USDM;
    let got = Usdm::decode_md(
        r,
        &mut ctx,
        usdm::MdClass::BookTicker,
        p.as_bytes(),
        &RX,
        &mut sink,
    );
    assert_eq!(got, Err(DecodeErr::Number));
    assert!(sink.md.is_empty());
}

fn bbo_of(sink: &VecSink) -> Bbo {
    assert_eq!(sink.md.len(), 1, "{:?}", sink.md);
    match &sink.md[0] {
        MdRec::Bbo(b) => *b,
        o => panic!("not a Bbo: {o:?}"),
    }
}

fn assert_rx(b: &Bbo) {
    assert_eq!(b.h.conn, ConnId(3));
    assert_eq!(b.h.hw_rx_ns, 1_790_000_000_126_000_000);
    assert_eq!(b.h.rx_tsc, 1000);
    assert_eq!(b.h.sw_rx_ns, 1_790_000_000_126_500_000);
    assert_eq!(b.h.rx_empty_gap_ns, 2500);
    assert_eq!(b.h._rsv, 0);
}

#[test]
fn bin_023_usdm_bookticker_to_bbo() {
    let mut sink = VecSink::default();
    let mut ctx = FeedCtx::USDM;
    let o = Usdm::decode_md(
        rd(),
        &mut ctx,
        usdm::MdClass::BookTicker,
        BT1.as_bytes(),
        &RX,
        &mut sink,
    )
    .unwrap();
    assert_eq!(
        o,
        MdOutcome {
            events: 1,
            inst: U_BTC
        }
    );
    let b = bbo_of(&sink);
    assert_eq!(b.h.venue, VenueId::BINANCE_USDM);
    assert_eq!(b.h.inst, U_BTC);
    assert_eq!(b.h.ex_ts_ns, 1_790_000_000_123_000_000);
    assert_eq!(b.h.ex_send_ns, 1_790_000_000_125_000_000);
    assert_rx(&b);
    assert_eq!(
        (b.bid_px, b.bid_qty, b.ask_px, b.ask_qty, b.update_id),
        (
            Px(6_500_010),
            Qty(150_000_000),
            Px(6_500_020),
            Qty(400_000),
            400_900_217
        )
    );
}

#[test]
fn bin_024_no_unit_guessing() {
    let p = BT1.replace("\"T\":1790000000123", "\"T\":1790000000123456");
    let mut sink = VecSink::default();
    let mut ctx = FeedCtx::USDM;
    let got = Usdm::decode_md(
        rd(),
        &mut ctx,
        usdm::MdClass::BookTicker,
        p.as_bytes(),
        &RX,
        &mut sink,
    );
    assert_eq!(got, Err(DecodeErr::Overflow));
    assert!(sink.md.is_empty());
}

#[test]
fn bin_025_spot_bookticker_no_ts() {
    let mut sink = VecSink::default();
    let mut ctx = FeedCtx::SPOT;
    Spot::decode_md(
        rd(),
        &mut ctx,
        spot::MdClass::JsonBookTicker,
        SBT1.as_bytes(),
        &RX,
        &mut sink,
    )
    .unwrap();
    let b = bbo_of(&sink);
    assert_eq!(
        (b.h.venue, b.h.inst, b.h.ex_ts_ns, b.h.ex_send_ns),
        (VenueId::BINANCE_SPOT, S_BTC, 0, 0)
    );
    assert_rx(&b);
    assert_eq!(
        (b.bid_px, b.bid_qty, b.ask_px, b.ask_qty, b.update_id),
        (
            Px(6_500_010),
            Qty(50_000_000),
            Px(6_500_011),
            Qty(25_000_000),
            400_900_217
        )
    );
}

type Want = Result<Option<(Px, Qty, Px, Qty)>, DecodeErr>;

#[test]
fn bin_026_bookticker_errors() {
    let cut = &BT1[..BT1.find(r#""a":"65000.2"#).unwrap() + r#""a":"65000.2"#.len()];
    let ok = |b: Px, bq: Qty, a: Px, aq: Qty| -> Want { Ok(Some((b, bq, a, aq))) };
    let rows: Vec<(&str, String, Want)> = vec![
        (
            "BIN-026.1",
            BT1.replace(r#""b":"65000.1","#, ""),
            Err(DecodeErr::Shape),
        ),
        (
            "BIN-026.2",
            BT1.replace(r#""u":400900217"#, r#""u":"x""#),
            Err(DecodeErr::Shape),
        ),
        ("BIN-026.3", cut.to_string(), Err(DecodeErr::Truncated)),
        (
            "BIN-026.4",
            BT1.replace(r#""b":"65000.1""#, r#""b":"""#),
            Err(DecodeErr::Number),
        ),
        (
            "BIN-026.5",
            BT1.replace(r#""b":"65000.1""#, r#""b":"65000.123""#),
            Err(DecodeErr::Number),
        ),
        (
            "BIN-026.6",
            BT1.replace(r#""B":"1.500""#, r#""B":"100000000000000""#),
            Err(DecodeErr::Overflow),
        ),
        (
            "BIN-026.7",
            BT1.replace(r#""b":"65000.1""#, r#""b":"6.50001E4""#),
            ok(Px(6_500_010), Qty(150_000_000), Px(6_500_020), Qty(400_000)),
        ),
        (
            "BIN-026.8",
            BT1.replace(r#""b":"65000.1""#, r#""b":"65000.1000000""#),
            ok(Px(6_500_010), Qty(150_000_000), Px(6_500_020), Qty(400_000)),
        ),
        (
            "BIN-026.9",
            BT1.replace(r#""A":"0.004""#, r#""A":"92233720.36854775""#),
            ok(
                Px(6_500_010),
                Qty(150_000_000),
                Px(6_500_020),
                Qty(9_223_372_036_854_775),
            ),
        ),
        (
            "BIN-026.10",
            BT1.replace("\"T\":1790000000123", "\"T\":\"1790000000123\""),
            Err(DecodeErr::Shape),
        ),
        (
            "BIN-026.11",
            BT1.replace(
                r#""b":"65000.1","B":"1.500""#,
                r#""b":"0.00000000","B":"0.00000000""#,
            ),
            ok(Px(0), Qty(0), Px(6_500_020), Qty(400_000)),
        ),
    ];
    for (row, p, want) in rows {
        let mut sink = VecSink::default();
        let mut ctx = FeedCtx::USDM;
        let got = Usdm::decode_md(
            rd(),
            &mut ctx,
            usdm::MdClass::BookTicker,
            p.as_bytes(),
            &RX,
            &mut sink,
        );
        match want {
            Err(e) => {
                assert_eq!(got, Err(e), "{row}");
                assert!(sink.md.is_empty(), "{row}");
            }
            Ok(v) => {
                assert!(got.is_ok(), "{row}: {got:?}");
                let b = bbo_of(&sink);
                assert_eq!(Some((b.bid_px, b.bid_qty, b.ask_px, b.ask_qty)), v, "{row}");
            }
        }
    }
}

#[test]
fn bin_035_control_hints() {
    assert_eq!(
        Usdm::control(usdm::MdClass::SubAck, br#"{"result":null,"id":7}"#),
        Some(ControlHint::SubAck { ok: true, code: 0 }),
        "BIN-035.1"
    );
    assert_eq!(
        Usdm::control(
            usdm::MdClass::SubAck,
            br#"{"error":{"code":2,"msg":"Invalid request"},"id":7}"#
        ),
        Some(ControlHint::SubAck { ok: false, code: 2 }),
        "BIN-035.2"
    );
    assert_eq!(
        Usdm::control(
            usdm::MdClass::ServerShutdown,
            br#"{"e":"serverShutdown","E":1790000000000}"#
        ),
        Some(ControlHint::ServerShutdown),
        "BIN-035.3"
    );
    assert_eq!(
        Usdm::control(usdm::MdClass::BookTicker, BT1.as_bytes()),
        None,
        "BIN-035.4"
    );
}

#[test]
fn bin_036_decode_unknown_class() {
    let p = br#"{"result":null,"id":7}"#;
    let mut ctx = FeedCtx::USDM;
    for (row, c) in [
        ("BIN-036.1", usdm::MdClass::SubAck),
        ("BIN-036.2", usdm::MdClass::Unknown),
        ("BIN-036.3", usdm::MdClass::ServerShutdown),
    ] {
        let mut sink = VecSink::default();
        assert_eq!(
            Usdm::decode_md(rd(), &mut ctx, c, p, &RX, &mut sink),
            Err(DecodeErr::UnknownClass),
            "{row}"
        );
        assert!(sink.md.is_empty(), "{row}");
    }
    let mut sink = VecSink::default();
    let mut sctx = FeedCtx::SPOT;
    assert_eq!(
        Spot::decode_md(rd(), &mut sctx, spot::MdClass::Unknown, p, &RX, &mut sink),
        Err(DecodeErr::UnknownClass),
        "BIN-036.4"
    );
}

#[test]
fn bin_027_topn_atomic_decode() {
    let mut sink = VecSink::default();
    Usdm::decode_md(
        rd(),
        &mut { FeedCtx::USDM },
        usdm::MdClass::DepthTopN,
        DP5.as_bytes(),
        &RX,
        &mut sink,
    )
    .unwrap();
    let MdRec::L2Snapshot {
        hdr,
        bids,
        asks,
        parts,
        ..
    } = &sink.md[0]
    else {
        panic!("snapshot")
    };
    assert_eq!(
        (hdr.snap_id, hdr.origin, *parts, bids.len(), asks.len()),
        (7_000_000_001, 3, 1, 5, 5)
    );
    assert_eq!((bids[0].px.0, asks[0].qty.0), (6_500_010, 40_000_000));
    assert_eq!(hdr.h.rx_tsc, RX.rx_tsc);
    sink.md.clear();
    let invalid = DP5.replace("65000.6", "invalid");
    assert!(
        Usdm::decode_md(
            rd(),
            &mut { FeedCtx::USDM },
            usdm::MdClass::DepthTopN,
            invalid.as_bytes(),
            &RX,
            &mut sink
        )
        .is_err()
    );
    assert!(sink.md.is_empty());
}
#[test]
fn bin_028_topn_zero_levels() {
    let mut sink = VecSink::default();
    let msg = DP5.replace("1.000", "0");
    Usdm::decode_md(
        rd(),
        &mut { FeedCtx::USDM },
        usdm::MdClass::DepthTopN,
        msg.as_bytes(),
        &RX,
        &mut sink,
    )
    .unwrap();
    assert!(matches!(&sink.md[0], MdRec::L2Snapshot { bids, .. } if bids.len() == 4));
}
#[test]
fn bin_029_spot_topn_combined() {
    let mut sink = VecSink::default();
    Spot::decode_md(
        rd(),
        &mut { FeedCtx::SPOT },
        spot::MdClass::JsonDepthTopN,
        SDP5.as_bytes(),
        &RX,
        &mut sink,
    )
    .unwrap();
    assert!(
        matches!(&sink.md[0], MdRec::L2Snapshot { hdr, bids, .. } if hdr.h.inst == S_BNB && hdr.h.ex_ts_ns == 0 && bids[0].px.0 == 600100)
    );
}
#[test]
fn bin_030_aggtrade_taker_side() {
    for (maker, side) in [(true, hft_types::Side::Sell), (false, hft_types::Side::Buy)] {
        let msg = format!(
            r#"{{"e":"aggTrade","s":"BTCUSDT","a":9,"p":"65000.1","q":"0.001","T":1000,"E":1001,"m":{maker}}}"#
        );
        let mut sink = VecSink::default();
        Usdm::decode_md(
            rd(),
            &mut { FeedCtx::USDM },
            usdm::MdClass::AggTrade,
            msg.as_bytes(),
            &RX,
            &mut sink,
        )
        .unwrap();
        assert!(
            matches!(&sink.md[0], MdRec::Trade(t) if t.taker_side == side && t.trade_id == 9 && t.qty.0 == 100000 && t.h.ex_ts_ns == 1000000000)
        );
    }
}
#[test]
fn bin_031_spot_trade_microseconds() {
    let mut sink = VecSink::default();
    Spot::decode_md(
        rd(),
        &mut { FeedCtx::SPOT },
        spot::MdClass::JsonTrade,
        br#"{"e":"trade","s":"BTCUSDT","t":10,"p":"65000.1","q":"0.001","T":1000,"E":1001,"m":false}"#,
        &RX,
        &mut sink,
    )
    .unwrap();
    assert!(matches!(&sink.md[0], MdRec::Trade(t) if t.h.ex_ts_ns == 1000000 && t.h.ex_send_ns == 1001000));
}
#[test]
fn bin_034_public_liquidation() {
    let mut sink = VecSink::default();
    Usdm::decode_md(
        rd(),
        &mut { FeedCtx::USDM },
        usdm::MdClass::ForceOrder,
        br#"{"e":"forceOrder","E":1001,"o":{"s":"BTCUSDT","ap":"65000.1","z":"0.001","S":"SELL","T":1000}}"#,
        &RX,
        &mut sink,
    )
    .unwrap();
    assert!(
        matches!(&sink.md[0], MdRec::Liquidation(t) if t.side == hft_types::Side::Sell && t.qty.0 == 100000)
    );
}
