use super::support::*;
use hft_event::ExternalCause;
use hft_event::OrderEventKind;
use hft_venue_proto::{AcctHint, DecodeErr, ReportOutcome};
use hft_venue_proto::{MsgOp, ReportCtl, VenueProtocol};
use venue_binance::{GwCtx, Usdm};
fn decode<P: VenueProtocol<GwCtx = GwCtx>>(
    ctx: &mut GwCtx,
    p: &str,
) -> (Result<ReportOutcome, DecodeErr>, RepSink) {
    let mut sink = RepSink::default();
    let class = P::classify_report(MsgOp::Text, p.as_bytes());
    let result = P::decode_report(rd(), ctx, class, p.as_bytes(), &RX, &mut sink);
    (result, sink)
}

#[test]
fn bin_103_same_trade_id_two_insts() {
    let mut ctx = GwCtx::default();
    let a = otu("TRADE", "FILLED");
    let b = a.replace("BTCUSDT", "ETHUSDT");
    for (p, inst) in [(&a, U_BTC), (&b, U_ETH)] {
        let (o, s) = decode::<Usdm>(&mut ctx, p);
        assert_eq!(o.unwrap().events, 1);
        assert_eq!(s.orders[0].inst, inst);
    }
}

#[test]
fn bin_104_order_seq_dedup() {
    let mut ctx = GwCtx::default();
    for (id, cum, count) in [
        (Some(0), "0.002", 1),
        (Some(0), "0.002", 0),
        (Some(0), "0.004", 1),
        (Some(-1), "0.004", 0),
        (None, "0.006", 1),
    ] {
        let mut p: serde_json::Value = serde_json::from_str(&otu("CALCULATED", "FILLED")).unwrap();
        p["o"]["c"] = "autoclose-1790000000000".into();
        p["o"]["z"] = cum.into();
        if let Some(id) = id {
            p["o"]["t"] = id.into();
        } else {
            p["o"].as_object_mut().unwrap().remove("t");
        }
        let (o, s) = decode::<Usdm>(&mut ctx, &p.to_string());
        assert_eq!(o.unwrap().events, count);
        if count == 1 {
            assert_eq!(s.ext[0].trade_id, 0);
            assert_eq!(
                s.ext[0].fill_seq,
                hft_types::parse_exact(cum.as_bytes(), -8).unwrap() as u64
            );
        }
    }
}

#[test]
fn bin_107_negative_fee_rebate() {
    let p = otu("TRADE", "PARTIALLY_FILLED").replace("0.0130", "-0.0013");
    let (o, s) = decode::<Usdm>(&mut GwCtx::default(), &p);
    assert_eq!(o.unwrap().events, 1);
    assert_eq!((s.orders[0].fee.0, s.orders[0].fee_asset), (-130_000, USDT));
}

#[test]
fn bin_108_external_fill_order() {
    for (cid, typ, cause) in [
        (
            "autoclose-1790000000000",
            "LIMIT",
            Some(ExternalCause::Liquidation),
        ),
        ("adl_autoclose", "LIMIT", Some(ExternalCause::Adl)),
        (
            "settlement_autoclose-1790",
            "LIMIT",
            Some(ExternalCause::Delivery),
        ),
        (
            "1000301020002a000000073d",
            "LIQUIDATION",
            Some(ExternalCause::Liquidation),
        ),
        ("adl_autoclose", "LIQUIDATION", Some(ExternalCause::Adl)),
        ("1000301020002a000000073d", "LIMIT", None),
        ("web_8fa3b1", "LIMIT", Some(ExternalCause::Manual)),
        ("ios_12", "LIMIT", Some(ExternalCause::Manual)),
        ("and_12", "LIMIT", Some(ExternalCause::Manual)),
        ("x-abc123", "LIMIT", Some(ExternalCause::Unknown)),
        ("", "LIMIT", Some(ExternalCause::Unknown)),
        ("1000301020002a000000073c", "LIMIT", Some(ExternalCause::Unknown)),
    ] {
        let p = otu("TRADE", "FILLED")
            .replace("1000301020002a000000073d", cid)
            .replace("LIMIT", typ);
        let (o, s) = decode::<Usdm>(&mut GwCtx::default(), &p);
        assert_eq!(o.unwrap().events, 1);
        if let Some(cause) = cause {
            assert_eq!(s.ext[0].cause, cause);
            assert!(s.orders.is_empty());
        } else {
            assert_eq!(s.orders.len(), 1);
            assert_eq!(
                (s.orders[0].what, s.orders[0].slot),
                (OrderEventKind::Fill, hft_types::SlotId(42))
            );
            assert!(s.ext.is_empty());
        }
    }
    let missing = otu("TRADE", "FILLED").replace("\"c\":\"1000301020002a000000073d\",", "");
    let (out, sink) = decode::<Usdm>(&mut GwCtx::default(), &missing);
    assert_eq!(out.unwrap().events, 1);
    assert_eq!(sink.ext[0].cause, ExternalCause::Unknown);
    assert!(sink.orders.is_empty());
    let spot = spot_report(
        "TRADE",
        "FILLED",
        "web_8fa3b1",
        "",
        "0.003",
        "0.003",
        "195.0003",
        9114,
    );
    let (out, sink) = decode::<venue_binance::Spot>(&mut GwCtx::default(), &spot);
    assert_eq!(out.unwrap().events, 1);
    assert_eq!(
        (sink.ext[0].cause, sink.ext[0].venue),
        (ExternalCause::Manual, hft_types::VenueId::BINANCE_SPOT)
    );
    assert!(sink.orders.is_empty());
}

#[test]
fn bin_110_insurance_clear_balance_only() {
    let p = r#"{"e":"ACCOUNT_UPDATE","T":1790000000599,"a":{"m":"INSURANCE_CLEAR","B":[{"a":"USDT","wb":"990","bc":"-10"}],"P":[]}}"#;
    let (o, s) = decode::<Usdm>(&mut GwCtx::default(), p);
    let o = o.unwrap();
    assert_eq!((o.events, s.balances[0].free.0), (1, 99_000_000_000));
    assert_eq!(o.hint, AcctHint::MarginNow);
    assert_eq!(s.balances[0].ex_ts_ns, 1790000000599000000);
    assert!(s.orders.is_empty() && s.ext.is_empty());
}

#[allow(clippy::too_many_arguments)]
fn spot_report(
    x: &str,
    status: &str,
    cid: &str,
    original: &str,
    qty: &str,
    cum: &str,
    value: &str,
    trade: i64,
) -> String {
    format!(
        r#"{{"subscriptionId":0,"event":{{"e":"executionReport","E":1790000000700000,"s":"BTCUSDT","c":"{cid}","S":"BUY","o":"LIMIT","f":"GTC","q":"0.00300","p":"65000.10","C":"{original}","x":"{x}","X":"{status}","i":88001,"l":"{qty}","z":"{cum}","L":"65000.10","n":"0.00000100","N":"BTC","T":1790000000699000,"t":{trade},"Z":"{value}"}}}}"#
    )
}

#[test]
fn bin_106_fee_rule_deviation() {
    for (spot, side, asset, aid, want) in [
        (false, "BUY", "USDT", USDT, false),
        (false, "BUY", "BNB", BNB, true),
        (true, "BUY", "BTC", BTC, false),
        (true, "BUY", "BNB", BNB, false),
        (true, "BUY", "USDT", USDT, true),
        (true, "SELL", "USDT", USDT, false),
        (true, "SELL", "BTC", BTC, true),
    ] {
        let (o, s) = if spot {
            let p = spot_report(
                "TRADE",
                "FILLED",
                "1000302020002a00000007b1",
                "",
                "0.003",
                "0.003",
                "195",
                501,
            )
            .replace("BUY", side)
            .replace("\"N\":\"BTC\"", &format!("\"N\":\"{asset}\""));
            decode::<venue_binance::Spot>(&mut GwCtx::default(), &p)
        } else {
            decode::<Usdm>(
                &mut GwCtx::default(),
                &otu("TRADE", "FILLED").replace("\"N\":\"USDT\"", &format!("\"N\":\"{asset}\"")),
            )
        };
        assert_eq!(o.unwrap().fee_rule_deviation, want, "{spot} {side} {asset}");
        let got = s
            .orders
            .first()
            .map(|e| e.fee_asset)
            .or_else(|| s.ext.first().map(|e| e.fee_asset));
        assert_eq!(got, Some(aid));
    }
}

#[test]
fn bin_109_external_fill_fields() {
    for tid in [55001, 0, -3] {
        let p = format!(
            r#"{{"e":"ORDER_TRADE_UPDATE","T":1790000000500,"o":{{"s":"BTCUSDT","c":"autoclose-1790000000000","S":"SELL","o":"LIQUIDATION","x":"CALCULATED","X":"FILLED","i":777,"l":"0.010","z":"0.010","L":"60000.5","ap":"60000.5","n":"0.30001","N":"USDT","T":1790000000500,"t":{tid}}}}}"#
        );
        let (o, s) = decode::<Usdm>(&mut GwCtx::default(), &p);
        assert_eq!(o.unwrap().events, 1);
        assert!(s.orders.is_empty());
        let e = s.ext[0];
        assert_eq!(
            (e.venue, e.account, e.inst),
            (hft_types::VenueId::BINANCE_USDM, ACCT, U_BTC)
        );
        assert_eq!(
            (e.side, e.cause),
            (hft_types::Side::Sell, ExternalCause::Liquidation)
        );
        assert_eq!(
            (e.qty.0, e.px.0, e.fee.0, e.fee_asset),
            (1_000_000, 6_000_050, 30_001_000, USDT)
        );
        assert_eq!((e.exch_order_id, e.ex_ts_ns), (777, 1790000000500000000));
        assert_eq!(
            (e.trade_id, e.fill_seq),
            if tid > 0 { (tid as u64, 0) } else { (0, 1_000_000) }
        );
    }
}

#[test]
fn bin_111_spot_execution_report() {
    use hft_types::{SlotId, VenueId};
    let id = cid(VenueId::BINANCE_SPOT, 3, 42, 7);
    let mut buf = [0u8; 48];
    let n = venue_binance::Spot::render_cid(&id, &mut buf);
    let cid = std::str::from_utf8(&buf[..n]).unwrap();
    for (x, status, c, original, qty, cum, value, tid, kind) in [
        ("NEW", "NEW", cid, "", "0", "0", "0", -1, OrderEventKind::Ack),
        (
            "TRADE",
            "PARTIALLY_FILLED",
            cid,
            "",
            "0.001",
            "0.001",
            "65.0001",
            501,
            OrderEventKind::PartialFill,
        ),
        (
            "TRADE",
            "FILLED",
            cid,
            "",
            "0.002",
            "0.003",
            "195.00",
            502,
            OrderEventKind::Fill,
        ),
        (
            "CANCELED",
            "CANCELED",
            "x-cxl-01",
            cid,
            "0",
            "0.001",
            "65.0001",
            -1,
            OrderEventKind::CancelAck,
        ),
        (
            "EXPIRED",
            "EXPIRED",
            cid,
            "",
            "0",
            "0",
            "0",
            -1,
            OrderEventKind::Expired,
        ),
        (
            "REPLACED",
            "NEW",
            cid,
            "",
            "0",
            "0",
            "0",
            -1,
            OrderEventKind::AmendAck,
        ),
    ] {
        let p = spot_report(x, status, c, original, qty, cum, value, tid);
        let (o, s) = decode::<venue_binance::Spot>(&mut GwCtx::default(), &p);
        assert_eq!(o.unwrap().events, 1, "{p}");
        let e = s.orders[0];
        assert_eq!(
            (e.what, e.slot, e.r#gen, e.exch_order_id),
            (kind, SlotId(42), 7, 88001)
        );
        if x == "TRADE" {
            assert_eq!(e.avg_px.0, if tid == 501 { 6500010 } else { 6500000 });
            assert_eq!(
                (e.fee.0, e.fee_asset, e.ex_ts_ns),
                (100, BTC, 1790000000699000000)
            );
        }
    }
    let (o, s) = decode::<venue_binance::Spot>(
        &mut GwCtx::default(),
        &spot_report("TRADE", "FILLED", "web_77", "", "0.003", "0.003", "195", 503),
    );
    assert_eq!(o.unwrap().events, 1);
    assert!(s.orders.is_empty());
    assert_eq!(
        (s.ext[0].cause, s.ext[0].qty.0, s.ext[0].trade_id),
        (ExternalCause::Manual, 300000, 503)
    );
    let (o, _) = decode::<venue_binance::Spot>(
        &mut GwCtx::default(),
        &spot_report("TRADE", "FILLED", cid, "", "0.003", "0.003", "abc", 504),
    );
    assert_eq!(o, Err(DecodeErr::Number));
}

#[test]
fn bin_112_account_update() {
    for (reason, hint) in [
        ("ORDER", AcctHint::None),
        ("MARGIN_TYPE_CHANGE", AcctHint::ConfigRecheck),
        ("ADMIN_DEPOSIT", AcctHint::MarginNow),
        ("ADMIN_WITHDRAW", AcctHint::MarginNow),
        ("DEPOSIT", AcctHint::None),
    ] {
        let p = format!(
            r#"{{"e":"ACCOUNT_UPDATE","E":1790000000800,"T":1790000000799,"a":{{"m":"{reason}","B":[{{"a":"USDT","wb":"1000.5","cw":"1000.5","bc":"0"}}],"P":[{{"s":"BTCUSDT","pa":"-0.004","ep":"65000.1","up":"-0.40","mt":"cross","ps":"BOTH"}},{{"s":"DOGEUSDT","pa":"100","ep":"0.1","up":"0","mt":"cross","ps":"BOTH"}}]}}}}"#
        );
        let (o, s) = decode::<Usdm>(&mut GwCtx::default(), &p);
        let o = o.unwrap();
        assert_eq!((o.events, o.hint), (2, hint));
        assert_eq!(s.positions.len(), 1);
        let e = s.positions[0];
        assert_eq!(
            (e.inst, e.qty.0, e.entry_px.0, e.upnl.0, e.liq_px.0, e.ex_ts_ns),
            (U_BTC, -400000, 6500010, -400000, 0, 1790000000799000000)
        );
        assert_eq!(
            (s.balances[0].asset, s.balances[0].free.0, s.balances[0].locked.0),
            (USDT, 100050000000, 0)
        );
        assert_eq!(
            decode::<Usdm>(
                &mut GwCtx::default(),
                &p.replace(&format!("\"m\":\"{reason}\","), "")
            )
            .0,
            Err(DecodeErr::Shape)
        );
    }
}

#[test]
fn bin_112_rate_only_push_does_not_double_book_history() {
    let p = r#"{"e":"ACCOUNT_UPDATE","T":1790000000599,"a":{"m":"FUNDING_FEE","B":[{"a":"USDT","wb":"990","bc":"-10"}],"P":[]}}"#;
    let (o, s) = decode::<Usdm>(&mut GwCtx::default(), p);
    assert_eq!(o.unwrap().events, 1);
    assert!(s.funding.is_empty());
    assert_eq!(s.balances[0].free.0, 99_000_000_000);
}

#[test]
fn bin_117_hint_only_events() {
    for (p, hint) in [
        (r#"{"e":"MARGIN_CALL","p":[]}"#, AcctHint::MarginNow),
        (
            r#"{"e":"ACCOUNT_CONFIG_UPDATE","ac":{"s":"BTCUSDT","l":25}}"#,
            AcctHint::ConfigRecheck,
        ),
        (
            r#"{"e":"ACCOUNT_CONFIG_UPDATE","ai":{"j":true}}"#,
            AcctHint::ConfigRecheck,
        ),
        (r#"{"e":"ACCOUNT_CONFIG_UPDATE"}"#, AcctHint::None),
    ] {
        let (o, _) = decode::<Usdm>(&mut GwCtx::default(), p);
        let o = o.unwrap();
        assert_eq!((o.events, o.hint), (0, hint));
    }
}

#[test]
fn bin_120_known_class_decode_error() {
    let original = otu("TRADE", "FILLED");
    for (p, error) in [
        (original.replace("0.003", "abc"), DecodeErr::Number),
        (r#"{"e":"ORDER_TRADE_UPDATE","o":{"#.into(), DecodeErr::Truncated),
        (original.replace("\"i\":325078477,", ""), DecodeErr::Shape),
        (original.replace("BTCUSDT", "DOGEUSDT"), DecodeErr::Shape),
    ] {
        let (o, s) = decode::<Usdm>(&mut GwCtx::default(), &p);
        assert_eq!(o, Err(error));
        assert!(s.orders.is_empty() && s.ext.is_empty());
    }
}

#[test]
fn bin_121_spot_balance() {
    let p = r#"{"subscriptionId":0,"event":{"e":"outboundAccountPosition","u":1790000000999000,"B":[{"a":"BTC","f":"0.01000000","l":"0.00300000"},{"a":"USDT","f":"1000.00","l":"0"},{"a":"XYZ","f":"5","l":"0"}]}}"#;
    let (o, s) = decode::<venue_binance::Spot>(&mut GwCtx::default(), p);
    assert_eq!(o.unwrap().events, 2);
    assert_eq!(
        (s.balances[0].asset, s.balances[0].free.0, s.balances[0].locked.0),
        (BTC, 1_000_000, 300_000)
    );
    assert_eq!(s.balances[0].ex_ts_ns, 1790000000999000000);
    assert_eq!(
        (s.balances[1].asset, s.balances[1].free.0),
        (USDT, 100_000_000_000)
    );
}
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

#[test]
fn bin_119_ignorable_unknown() {
    use hft_venue_proto::Skip;
    let rd = rd_ignoring(&["BAR_EVENT"]);
    fn check<P: VenueProtocol<GwCtx = GwCtx>>(
        rd: &hft_refdata::RefData,
        name: &str,
        skip: Skip,
        truncated: bool,
        wrapped: bool,
    ) {
        let payload = if truncated {
            format!("{{\"e\":\"{name}\",\"unfinished\":")
        } else {
            format!("{{\"e\":\"{name}\"}}")
        };
        let payload = if wrapped {
            format!("{{\"subscriptionId\":1,\"event\":{payload}}}")
        } else {
            payload
        };
        let mut sink = RepSink::default();
        let class = P::classify_report(MsgOp::Text, payload.as_bytes());
        let out = P::decode_report(
            rd,
            &mut GwCtx::default(),
            class,
            payload.as_bytes(),
            &RX,
            &mut sink,
        )
        .unwrap();
        assert_eq!((out.events, out.skip), (0, skip), "{name}");
        assert!(sink.orders.is_empty() && sink.ext.is_empty() && sink.other == 0);
    }
    for name in [
        "TRADE_LITE",
        "STRATEGY_UPDATE",
        "GRID_UPDATE",
        "CONDITIONAL_ORDER_TRIGGER_REJECT",
        "BAR_EVENT",
    ] {
        check::<Usdm>(rd, name, Skip::Listed, false, false);
    }
    check::<Usdm>(rd, "FOO_EVENT", Skip::Unknown, false, false);
    for name in ["balanceUpdate", "listStatus", "externalLockUpdate"] {
        check::<venue_binance::Spot>(rd, name, Skip::Listed, false, true);
    }
    check::<venue_binance::Spot>(rd, "newEvt", Skip::Unknown, false, true);
    check::<Usdm>(rd, "TRADE_LITE", Skip::Listed, true, false);
}

#[test]
fn bin_107_zero_commission_without_asset_and_invalid_last_fill() {
    let base = otu("TRADE", "PARTIALLY_FILLED");
    let mut v: serde_json::Value = serde_json::from_str(&base).unwrap();
    v["o"]["N"] = serde_json::Value::Null;
    v["o"]["n"] = "0".into();
    let (out, sink) = decode::<Usdm>(&mut GwCtx::default(), &v.to_string());
    assert_eq!(out.unwrap().events, 1);
    assert_eq!(sink.orders[0].fee.0, 0);
    v["o"]["n"] = "0.01".into();
    let (out, sink) = decode::<Usdm>(&mut GwCtx::default(), &v.to_string());
    assert!(out.is_err());
    assert!(sink.orders.is_empty());
    let mut v: serde_json::Value = serde_json::from_str(&base).unwrap();
    v["o"]["l"] = "1".into();
    let (out, sink) = decode::<Usdm>(&mut GwCtx::default(), &v.to_string());
    assert!(out.is_err());
    assert!(sink.orders.is_empty());
}
