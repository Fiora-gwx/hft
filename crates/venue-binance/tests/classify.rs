//! BIN-010, BIN-011, BIN-013: dispatch from the first 64 bytes.

use hft_venue_proto::{MsgOp, RouteId, VenueProtocol};
use proptest::prelude::*;
use venue_binance::{Spot, Usdm, spot, usdm};

proptest! {
    #![proptest_config(ProptestConfig::with_cases(2000))]
    #[test]
    fn bin_012_classify_reads_prefix_only(prefix in any::<[u8;64]>(), a in prop::collection::vec(any::<u8>(),0..4097), b in prop::collection::vec(any::<u8>(),0..4097), event in 0..10usize) {
        let names=["bookTicker","depthUpdate","aggTrade","markPriceUpdate","forceOrder","ORDER_TRADE_UPDATE","ACCOUNT_UPDATE","executionReport","outboundAccountPosition",""];
        let mut p=prefix;
        if event<9 {let s=format!("{{\"e\":\"{}\",",names[event]);p[..s.len()].copy_from_slice(s.as_bytes());}
        let x=[&p[..],&a].concat();let y=[&p[..],&b].concat();
        for op in [MsgOp::Text,MsgOp::Binary] {
            for r in Usdm::routes() {prop_assert_eq!(Usdm::classify(r.id,op,&x),Usdm::classify(r.id,op,&p));prop_assert_eq!(Usdm::classify(r.id,op,&x),Usdm::classify(r.id,op,&y));}
            for r in Spot::routes() {prop_assert_eq!(Spot::classify(r.id,op,&x),Spot::classify(r.id,op,&p));prop_assert_eq!(Spot::classify(r.id,op,&x),Spot::classify(r.id,op,&y));}
            prop_assert_eq!(Usdm::classify_report(op,&x),Usdm::classify_report(op,&y));
            prop_assert_eq!(Spot::classify_report(op,&x),Spot::classify_report(op,&y));
        }
    }
}

fn route<P: VenueProtocol>(name: &str) -> RouteId {
    P::routes().iter().find(|r| r.name == name).unwrap().id
}

#[test]
fn bin_010_usdm_classify() {
    use usdm::MdClass as M;
    let bbo = route::<Usdm>("public-bbo");
    let topn = route::<Usdm>("public-topn");
    let depth = route::<Usdm>("public-depth");
    let market = route::<Usdm>("market");
    let spaced = format!("{}{}", " ".repeat(70), r#"{"e":"bookTicker","u":1}"#);
    let rows: Vec<(&str, RouteId, MsgOp, &[u8], M)> = vec![
        (
            "BIN-010.1",
            bbo,
            MsgOp::Text,
            br#"{"e":"bookTicker","u":4009"#,
            M::BookTicker,
        ),
        (
            "BIN-010.2",
            topn,
            MsgOp::Text,
            br#"{"e":"depthUpdate","E":17"#,
            M::DepthTopN,
        ),
        (
            "BIN-010.3",
            depth,
            MsgOp::Text,
            br#"{"e":"depthUpdate","E":17"#,
            M::DepthUpdate,
        ),
        (
            "BIN-010.4",
            market,
            MsgOp::Text,
            br#"{"e":"depthUpdate","E":17"#,
            M::Unknown,
        ),
        (
            "BIN-010.5",
            market,
            MsgOp::Text,
            br#"{"e":"aggTrade","E":17"#,
            M::AggTrade,
        ),
        (
            "BIN-010.6",
            market,
            MsgOp::Text,
            br#"{"e":"markPriceUpdate","E":17"#,
            M::MarkPrice,
        ),
        (
            "BIN-010.7",
            market,
            MsgOp::Text,
            br#"{"e":"forceOrder","E":17"#,
            M::ForceOrder,
        ),
        (
            "BIN-010.8",
            bbo,
            MsgOp::Text,
            br#"{"result":null,"id":7}"#,
            M::SubAck,
        ),
        (
            "BIN-010.9",
            market,
            MsgOp::Text,
            br#"{"error":{"code":2,"msg":"Invalid request"},"id":7}"#,
            M::SubAck,
        ),
        (
            "BIN-010.10",
            bbo,
            MsgOp::Text,
            br#"{"e":"serverShutdown","E":17"#,
            M::ServerShutdown,
        ),
        (
            "BIN-010.11",
            market,
            MsgOp::Text,
            br#"{"e":"kline","E":17"#,
            M::Unknown,
        ),
        (
            "BIN-010.12",
            bbo,
            MsgOp::Binary,
            &[1, 2, 3, 4, 5, 6, 7, 8],
            M::Unknown,
        ),
        ("BIN-010.13", bbo, MsgOp::Text, b"", M::Unknown),
        ("BIN-010.14", bbo, MsgOp::Text, br#"{"e"#, M::Unknown),
        ("BIN-010.15", bbo, MsgOp::Text, spaced.as_bytes(), M::Unknown),
    ];
    for (row, r, op, p, want) in rows {
        let head = &p[..p.len().min(64)];
        assert_eq!(Usdm::classify(r, op, head), want, "{row}");
    }
}

#[test]
fn bin_011_spot_classify() {
    use spot::MdClass as M;
    let bbo = route::<Spot>("spot-bbo");
    let topn = route::<Spot>("spot-topn");
    let md = route::<Spot>("spot-md");
    let rows: Vec<(&str, RouteId, MsgOp, &[u8], M)> = vec![
        (
            "BIN-011.1",
            bbo,
            MsgOp::Text,
            br#"{"u":400900217,"s":"BTCUSDT","#,
            M::JsonBookTicker,
        ),
        (
            "BIN-011.2",
            md,
            MsgOp::Text,
            br#"{"e":"trade","E":1790"#,
            M::JsonTrade,
        ),
        (
            "BIN-011.3",
            md,
            MsgOp::Text,
            br#"{"e":"depthUpdate","E":1790"#,
            M::JsonDepth,
        ),
        (
            "BIN-011.4",
            topn,
            MsgOp::Text,
            br#"{"stream":"btcusdt@depth5@100ms","data":{"lastUpdateId":1"#,
            M::JsonDepthTopN,
        ),
        (
            "BIN-011.5",
            topn,
            MsgOp::Text,
            br#"{"stream":"btcusdt@trade","data":{"e":1"#,
            M::Unknown,
        ),
        (
            "BIN-011.6",
            bbo,
            MsgOp::Text,
            br#"{"result":null,"id":3}"#,
            M::SubAck,
        ),
        (
            "BIN-011.7",
            bbo,
            MsgOp::Binary,
            &[9, 8, 7, 6, 5, 4, 3, 2],
            M::Unknown,
        ),
        (
            "BIN-011.8",
            md,
            MsgOp::Text,
            br#"{"e":"24hrTicker","E":1"#,
            M::Unknown,
        ),
    ];
    for (row, r, op, p, want) in rows {
        let head = &p[..p.len().min(64)];
        assert_eq!(Spot::classify(r, op, head), want, "{row}");
    }
}

#[test]
fn bin_013_classify_report() {
    use spot::ReportClass as S;
    use usdm::ReportClass as U;
    let u: Vec<(&str, &[u8], U)> = vec![
        (
            "BIN-013.1",
            br#"{"id":"p0000002a00000007","status":200,"result":{}}"#,
            U::WsResp,
        ),
        (
            "BIN-013.2",
            br#"{"id":"q0000002a00000007","status":400,"error":{}}"#,
            U::WsResp,
        ),
        (
            "BIN-013.3",
            br#"{"id":"l0000000000000000","status":200,"result":{}}"#,
            U::Control,
        ),
        (
            "BIN-013.4",
            br#"{"id":"z0000002a00000007","status":200}"#,
            U::Unknown,
        ),
        (
            "BIN-013.5",
            br#"{"id":"P0000002A00000007","status":200}"#,
            U::Unknown,
        ),
        (
            "BIN-013.6",
            br#"{"e":"ORDER_TRADE_UPDATE","T":1}"#,
            U::OrderTradeUpdate,
        ),
        ("BIN-013.7", br#"{"e":"ACCOUNT_UPDATE","T":1}"#, U::AccountUpdate),
        ("BIN-013.8", br#"{"e":"MARGIN_CALL","E":1}"#, U::MarginCall),
        (
            "BIN-013.9",
            br#"{"e":"ACCOUNT_CONFIG_UPDATE","E":1}"#,
            U::AccountConfigUpdate,
        ),
        (
            "BIN-013.10",
            br#"{"e":"listenKeyExpired","E":1}"#,
            U::ListenKeyExpired,
        ),
        ("BIN-013.11", br#"{"e":"TRADE_LITE","E":1}"#, U::Ignorable),
        ("BIN-013.12", br#"{"e":"GRID_UPDATE","E":1}"#, U::Ignorable),
        ("BIN-013.13", br#"{"e":"FOO_EVENT","E":1}"#, U::Unknown),
    ];
    for (row, p, want) in u {
        assert_eq!(
            Usdm::classify_report(hft_venue_proto::MsgOp::Text, &p[..p.len().min(64)]),
            want,
            "{row}"
        );
    }
    let s: Vec<(&str, &[u8], S)> = vec![
        (
            "BIN-013.14",
            br#"{"subscriptionId":0,"event":{"e":"executionReport","E":1}}"#,
            S::ExecutionReport,
        ),
        (
            "BIN-013.15",
            br#"{"subscriptionId":0,"event":{"e":"outboundAccountPosition","E":1}}"#,
            S::OutboundAccountPosition,
        ),
        (
            "BIN-013.16",
            br#"{"subscriptionId":0,"event":{"e":"eventStreamTerminated","E":1}}"#,
            S::EventStreamTerminated,
        ),
        (
            "BIN-013.17",
            br#"{"subscriptionId":0,"event":{"e":"balanceUpdate","E":1}}"#,
            S::Ignorable,
        ),
        (
            "BIN-013.18",
            br#"{"subscriptionId":0,"event":{"e":"listStatus","E":1}}"#,
            S::Ignorable,
        ),
        (
            "BIN-013.19",
            br#"{"id":"u0000000000000000","status":200,"result":{"subscriptionId":0}}"#,
            S::Control,
        ),
        (
            "BIN-013.20",
            br#"{"subscriptionId":0,"event":{"e":"newEvt","E":1}}"#,
            S::Unknown,
        ),
    ];
    for (row, p, want) in s {
        assert_eq!(
            Spot::classify_report(hft_venue_proto::MsgOp::Text, &p[..p.len().min(64)]),
            want,
            "{row}"
        );
    }
}
