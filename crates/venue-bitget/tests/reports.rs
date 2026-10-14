//! BGT-013, BGT-014, BGT-090–BGT-117: report dispatch, trade answers, the REST query, the private
//! `order` / `position` / `account` channels and external fills.

use crate::support;

use hft_event::{ExternalCause, GwOp, OrderEvent, OrderEventKind};
use hft_refdata::LimitBucket;
use hft_types::{AssetId, Px, Qty, Side, SlotId};
use hft_venue_proto::{
    DecodeErr, ExchIdHint, MsgOp, ReportCtl, ReportOutcome, ReqRef, RestOrigin, RestWhat, Skip, VenueProtocol,
};
use support::{ACCT, BGB, BTC, F_BTC, F_ETH, FUT, RX, RepSink, S_BGB, S_BTC, SPOT, USDT, cid, cid_text, rd};
use venue_bitget::codes::DEFINITE_REJECT;
use venue_bitget::{BitgetSpot, BitgetUsdtFutures as Fut, GwCtx, ReportClass};

fn dec_with(
    rd: &'static hft_refdata::RefData,
    ctx: &mut GwCtx,
    p: &str,
) -> (Result<ReportOutcome, DecodeErr>, RepSink) {
    let mut s = RepSink::default();
    let c = Fut::classify_report(MsgOp::Text, support::head(p));
    let r = Fut::decode_report(rd, ctx, c, p.as_bytes(), &RX, &mut s);
    (r, s)
}

fn dec(ctx: &mut GwCtx, p: &str) -> (Result<ReportOutcome, DecodeErr>, RepSink) {
    dec_with(rd(), ctx, p)
}

fn dec_spot(ctx: &mut GwCtx, p: &str) -> (Result<ReportOutcome, DecodeErr>, RepSink) {
    let mut s = RepSink::default();
    let c = BitgetSpot::classify_report(MsgOp::Text, support::head(p));
    let r = BitgetSpot::decode_report(rd(), ctx, c, p.as_bytes(), &RX, &mut s);
    (r, s)
}

fn one(s: &RepSink) -> OrderEvent {
    assert_eq!(s.orders.len(), 1, "{:?}", s.orders);
    s.orders[0]
}

/// Expected event kind and cumulative quantity (`None` = decode error).
type Expect = Option<(OrderEventKind, i64)>;

const CID7: &str = "100030a020002a000000078d";

#[test]
fn bgt_013_classify_report() {
    let rows = [
        (
            "BGT-013.1",
            r#"{"event":"trade","id":"p0000002a00000007","category":"usdt-futures""#,
            ReportClass::WsResp,
        ),
        (
            "BGT-013.2",
            r#"{"id":"c0000002a00000007","event":"trade"}"#,
            ReportClass::WsResp,
        ),
        (
            "BGT-013.3",
            r#"{"event":"error","id":"m0000002a00000007","code":"25204"}"#,
            ReportClass::WsResp,
        ),
        (
            "BGT-013.4",
            r#"{"id":"x0000002a00000007","event":"trade"}"#,
            ReportClass::Unknown,
        ),
        (
            "BGT-013.5",
            r#"{"id":"z0000002a00000007","event":"trade"}"#,
            ReportClass::Unknown,
        ),
        (
            "BGT-013.6",
            r#"{"action":"snapshot","arg":{"instType":"UTA","topic":"order"},"data":[]}"#,
            ReportClass::Orders,
        ),
        (
            "BGT-013.7",
            r#"{"arg":{"instType":"UTA","topic":"position"},"data":[]}"#,
            ReportClass::Positions,
        ),
        (
            "BGT-013.8",
            r#"{"arg":{"instType":"UTA","topic":"account"},"data":[]}"#,
            ReportClass::Account,
        ),
        (
            "BGT-013.9",
            r#"{"data":[{"unrealisedPnL":"0","totalEquity":"1"}],"arg":{"instType":"UTA","topic":"account"}}"#,
            ReportClass::Push,
        ),
        (
            "BGT-013.10",
            r#"{"event":"login","code":0}"#,
            ReportClass::Control,
        ),
        (
            "BGT-013.11",
            r#"{"event":"error","code":30001,"msg":"x"}"#,
            ReportClass::Control,
        ),
        ("BGT-013.12", "pong", ReportClass::Pong),
        (
            "BGT-013.13",
            r#"{"arg":{"instType":"UTA","topic":"strategy-x"},"data":[]}"#,
            ReportClass::Unknown,
        ),
        ("BGT-013.14", "", ReportClass::Unknown),
        (
            "BGT-013.16",
            r#"{"event":"subscribe","arg":{"instType":"UTA","topic":"order"}}"#,
            ReportClass::Control,
        ),
    ];
    for (row, p, want) in rows {
        assert_eq!(Fut::classify_report(MsgOp::Text, support::head(p)), want, "{row}");
    }
    assert_eq!(
        Fut::classify_report(MsgOp::Binary, b"{\"event\":\"trade\""),
        ReportClass::Unknown,
        "BGT-013.15"
    );
    // BGT-013.17: an error answer with `code` first and the request id beyond the head.
    let late = r#"{"code":"25204","msg":"Order does not exist","id":"c0000002a00000007","event":"error"}"#;
    assert_eq!(
        Fut::classify_report(MsgOp::Text, support::head(late)),
        ReportClass::Control,
        "BGT-013.17"
    );
    let (r, s) = dec(&mut GwCtx::new(), late);
    assert_eq!(r.unwrap().req.map(|x| x.op), Some(GwOp::Cancel), "BGT-013.17");
    assert_eq!(
        (one(&s).what, one(&s).detail),
        (OrderEventKind::CancelReject, 1),
        "BGT-013.17"
    );
}

// ---------------------------------------------------------------- trade answers

fn ack(id: &str, topic: &str, args: &str) -> String {
    format!(
        r#"{{"event":"trade","id":"{id}","category":"usdt-futures","topic":"{topic}","args":[{args}],"code":"0","msg":"success","connId":"x","ts":"1790000000076"}}"#
    )
}

fn err(id: &str, code: &str) -> String {
    format!(r#"{{"event":"error","id":"{id}","code":{code},"msg":"x"}}"#)
}

#[test]
fn bgt_090_place_ack() {
    let p = ack(
        "p0000002a00000007",
        "place-order",
        &format!(
            r#"{{"symbol":"BTCUSDT","orderId":"1234567890123","clientOid":"{CID7}","cTime":"1790000000008"}}"#
        ),
    );
    let (r, s) = dec(&mut GwCtx::new(), &p);
    let e = one(&s);
    assert_eq!(
        (
            e.venue,
            e.account,
            e.what,
            e.slot,
            e.r#gen,
            e.epoch,
            e.inst,
            e.exch_order_id
        ),
        (
            FUT,
            ACCT,
            OrderEventKind::Ack,
            SlotId(42),
            7,
            3,
            F_BTC,
            1_234_567_890_123
        )
    );
    assert_eq!(
        (e.cum_filled, e.fee, e.reject_code, e.ex_ts_ns),
        (Qty(0), Qty(0), 0, 1_790_000_000_008_000_000)
    );
    assert_eq!(
        s.exch_ids,
        vec![(b"1234567890123".to_vec(), Some(cid(FUT, 3, 42, 7)))]
    );
    assert_eq!(
        r.unwrap(),
        ReportOutcome {
            req: Some(ReqRef {
                slot: 42,
                r#gen: 7,
                op: GwOp::Place
            }),
            events: 1,
            ..ReportOutcome::default()
        }
    );
}

#[test]
fn bgt_091_code_table() {
    assert!(
        !DEFINITE_REJECT.is_empty(),
        "BGT-091.1: registered from the official code page"
    );
    let req = |op| {
        Some(ReqRef {
            slot: 42,
            r#gen: 7,
            op,
        })
    };
    // Unregistered and outcome-unknown codes.
    for (row, id, code, op, raw) in [
        (
            "BGT-091.2",
            "p0000002a00000007",
            "\"40001\"",
            GwOp::Place,
            40001u32,
        ),
        ("BGT-091.5", "p0000002a00000007", "\"40010\"", GwOp::Place, 40010),
        ("BGT-091.6", "p0000002a00000007", "40001", GwOp::Place, 40001),
        ("BGT-091.8", "c0000002a00000007", "\"43001\"", GwOp::Cancel, 43001),
        ("BGT-091.9", "m0000002a00000007", "\"25000\"", GwOp::Amend, 25000),
        ("BGT-091.13", "p0000002a00000007", "\"25212\"", GwOp::Place, 25212),
    ] {
        let (r, s) = dec(&mut GwCtx::new(), &err(id, code));
        let o = r.unwrap();
        assert!(s.orders.is_empty(), "{row}");
        assert_eq!(
            (o.outcome_unknown, o.raw_code, o.req),
            (req(op), raw, req(op)),
            "{row}"
        );
    }
    let (r, _) = dec(&mut GwCtx::new(), &err("p0000002a00000007", "\"abc\""));
    let o = r.unwrap();
    assert!(
        o.outcome_unknown.is_some() && (o.raw_code & 0xFF00_0000) == 0x4000_0000,
        "BGT-091.7"
    );
    // Registered definite rejections.
    for (row, id, code, what) in [
        (
            "BGT-091.3",
            "p0000002a00000007",
            "\"25202\"",
            OrderEventKind::Reject,
        ),
        (
            "BGT-091.4",
            "p0000002a00000007",
            "\"40017\"",
            OrderEventKind::Reject,
        ),
        (
            "BGT-091.14",
            "c0000002a00000007",
            "\"25200\"",
            OrderEventKind::CancelReject,
        ),
    ] {
        let (r, s) = dec(&mut GwCtx::new(), &err(id, code));
        let o = r.unwrap();
        let e = one(&s);
        assert_eq!(
            (e.what, e.slot, e.r#gen, e.detail),
            (what, SlotId(42), 7, 0),
            "{row}"
        );
        assert_eq!((o.outcome_unknown, e.reject_code), (None, o.raw_code), "{row}");
    }
    // Rate limited and clock skew.
    let (r, s) = dec(&mut GwCtx::new(), &err("p0000002a00000007", "\"25004\""));
    let o = r.unwrap();
    assert_eq!(
        (o.ctl, one(&s).what),
        (ReportCtl::RateLimited, OrderEventKind::Reject),
        "BGT-091.15"
    );
    let (r, _) = dec(&mut GwCtx::new(), &err("p0000002a00000007", "\"40008\""));
    assert_eq!(r.unwrap().ctl, ReportCtl::ClockSkew, "BGT-091.16");
    // A non-zero code on an `event = trade` answer is an error too.
    let p = ack("p0000002a00000007", "place-order", "{}").replace(r#""code":"0""#, r#""code":"25202""#);
    let (_, s) = dec(&mut GwCtx::new(), &p);
    assert_eq!(one(&s).what, OrderEventKind::Reject, "BGT-091.17");
    let _ = LimitBucket::Orders;
}

#[test]
fn bgt_092_amend_cancel_ack() {
    let args = format!(r#"{{"orderId":"1234567890123","clientOid":"{CID7}"}}"#);
    let (r, s) = dec(
        &mut GwCtx::new(),
        &ack("m0000002a00000007", "modify-order", &args),
    );
    let o = r.unwrap();
    assert_eq!(
        (o.events, o.req.map(|x| x.op)),
        (0, Some(GwOp::Amend)),
        "BGT-092.1: receipt only"
    );
    assert!(s.orders.is_empty());
    let (r, s) = dec(
        &mut GwCtx::new(),
        &ack("c0000002a00000007", "cancel-order", &args),
    );
    let o = r.unwrap();
    assert_eq!(
        (o.events, o.req.map(|x| x.op)),
        (0, Some(GwOp::Cancel)),
        "BGT-092.2: receipt only"
    );
    assert!(s.orders.is_empty() && o.outcome_unknown.is_none());
    let (r, _) = dec(
        &mut GwCtx::new(),
        &ack("c0000002b00000001", "cancel-order", &args),
    );
    assert_eq!(
        r.unwrap().req,
        Some(ReqRef {
            slot: 43,
            r#gen: 1,
            op: GwOp::Cancel
        }),
        "BGT-092.3"
    );
    let (r, s) = dec(
        &mut GwCtx::new(),
        &ack(
            "p0000002a00000007",
            "place-order",
            &format!(r#"{{"clientOid":"{CID7}"}}"#),
        ),
    );
    assert_eq!((r, s.orders.len()), (Err(DecodeErr::Shape), 0), "BGT-092.4");
    // GONE: cancel / modify of an order that no longer exists.
    for (id, what) in [
        ("c0000002a00000007", OrderEventKind::CancelReject),
        ("m0000002a00000007", OrderEventKind::AmendReject),
    ] {
        let (r, s) = dec(&mut GwCtx::new(), &err(id, "\"25204\""));
        let e = one(&s);
        assert_eq!((e.what, e.detail, e.reject_code), (what, 1, 25204), "BGT-092.5");
        assert!(r.unwrap().outcome_unknown.is_none());
    }
}

fn query(status: &str, cum: &str, avg: &str) -> String {
    format!(
        r#"{{"code":"00000","msg":"success","requestTime":1,"data":{{"orderId":"1234567890123","clientOid":"{CID7}","category":"USDT-FUTURES","symbol":"BTCUSDT","orderType":"limit","side":"buy","price":"65000.1","qty":"0.004","amount":"","cumExecQty":"{cum}","cumExecValue":"0","avgPrice":"{avg}","timeInForce":"gtc","orderStatus":"{status}","reduceOnly":"NO","feeDetail":[],"cancelReason":"","execType":"","createdTime":"1790000000000","updatedTime":"1790000000050"}}}}"#
    )
}

fn dec_rest(status: u16, body: &str) -> (Result<ReportOutcome, DecodeErr>, RepSink) {
    let mut s = RepSink::default();
    let origin = RestOrigin {
        what: RestWhat::Order,
        req: Some(ReqRef {
            slot: 42,
            r#gen: 7,
            op: GwOp::Query,
        }),
        inst: F_BTC,
    };
    let r = Fut::decode_rest(
        rd(),
        &mut GwCtx::new(),
        &origin,
        status,
        body.as_bytes(),
        &RX,
        &mut s,
    );
    (r, s)
}

#[test]
fn bgt_093_query_result() {
    let rows = [
        ("BGT-093.1", "filled", "0.004", "65000.1", 3u8, 400_000, 6_500_010),
        ("BGT-093.2", "live", "0", "", 1, 0, 0),
        (
            "BGT-093.3",
            "partially_filled",
            "0.002",
            "65000",
            2,
            200_000,
            6_500_000,
        ),
        (
            "BGT-093.4",
            "cancelled",
            "0.001",
            "65000.1",
            4,
            100_000,
            6_500_010,
        ),
    ];
    for (row, st, cum, avg, detail, cumq, avgp) in rows {
        let (r, s) = dec_rest(200, &query(st, cum, avg));
        assert_eq!(r.unwrap().events, 1, "{row}");
        let e = one(&s);
        assert_eq!(
            (
                e.what,
                e.slot,
                e.r#gen,
                e.epoch,
                e.detail,
                e.cum_filled,
                e.avg_px,
                e.exch_order_id
            ),
            (
                OrderEventKind::QueryResult,
                SlotId(42),
                7,
                3,
                detail,
                Qty(cumq),
                Px(avgp),
                1_234_567_890_123
            ),
            "{row}"
        );
    }
    assert_eq!(
        dec_rest(200, &query("filled", "0.004", "0")).0,
        Err(DecodeErr::Shape),
        "BGT-093.5"
    );
    let (r, s) = dec_rest(
        400,
        r#"{"code":"25204","msg":"Order does not exist","requestTime":1,"data":null}"#,
    );
    let e = one(&s);
    assert_eq!(
        (e.what, e.detail, e.slot, e.r#gen, e.cum_filled),
        (OrderEventKind::QueryResult, 7, SlotId(42), 7, Qty(0)),
        "BGT-093.6"
    );
    assert!(r.unwrap().outcome_unknown.is_none(), "BGT-093.6");
    // BGT-091.10–12: transport-level answers.
    let (r, s) = dec_rest(503, "<html>");
    let o = r.unwrap();
    assert!(s.orders.is_empty());
    assert_eq!(
        (o.outcome_unknown.map(|x| x.op), o.raw_code),
        (Some(GwOp::Query), 0x2000_01F7),
        "BGT-091.10"
    );
    let (r, _) = dec_rest(429, "");
    let o = r.unwrap();
    assert_eq!(
        (o.ctl, o.raw_code, o.outcome_unknown.is_some()),
        (ReportCtl::RateLimited, 0x2000_01AD, true),
        "BGT-091.11"
    );
    let (r, _) = dec_rest(200, &query("filled", "0.004", "65000.1"));
    assert!(r.unwrap().outcome_unknown.is_none(), "BGT-091.12");
}

// ---------------------------------------------------------------- order channel

/// UTA `order` push with one order.
fn ord(status: &str, cum: &str, value: &str, avg: &str, fee: &str, extra: &str) -> String {
    ord_on(
        "usdt-futures",
        "BTCUSDT",
        "1234567890123",
        CID7,
        status,
        cum,
        value,
        avg,
        fee,
        extra,
    )
}

#[allow(clippy::too_many_arguments)]
fn ord_on(
    cat: &str,
    sym: &str,
    oid: &str,
    coid: &str,
    status: &str,
    cum: &str,
    value: &str,
    avg: &str,
    fee: &str,
    extra: &str,
) -> String {
    format!(
        r#"{{"action":"snapshot","arg":{{"instType":"UTA","topic":"order"}},"data":[{{"category":"{cat}","symbol":"{sym}","orderId":"{oid}","clientOid":"{coid}","price":"65000.1","qty":"0.004","amount":"","holdMode":"one_way_mode","holdSide":"long","delegateType":"normal","tradeSide":"buy_single","orderType":"limit","timeInForce":"gtc","side":"buy","marginMode":"crossed","marginCoin":"USDT","reduceOnly":"no","cumExecQty":"{cum}","cumExecValue":"{value}","avgPrice":"{avg}","totalProfit":"0","orderStatus":"{status}","cancelReason":"","leverage":"20","feeDetail":[{{"feeCoin":"USDT","fee":"{fee}"}}],"createdTime":"1790000000400","updatedTime":"1790000000500","stpMode":"none"{extra}}}],"ts":1790000000501}}"#
    )
}

#[test]
fn bgt_100_order_status_map() {
    let rows: Vec<(&str, String, Expect)> = vec![
        (
            "BGT-100.1",
            ord("live", "0", "0", "0", "0", ""),
            Some((OrderEventKind::Ack, 0)),
        ),
        (
            "BGT-100.1",
            ord("new", "0", "0", "0", "0", ""),
            Some((OrderEventKind::Ack, 0)),
        ),
        (
            "BGT-100.2",
            ord("partially_filled", "0.001", "65.0001", "65000.1", "0.01", ""),
            Some((OrderEventKind::PartialFill, 100_000)),
        ),
        (
            "BGT-100.3",
            ord("filled", "0.004", "260.0004", "65000.1", "0.04", ""),
            Some((OrderEventKind::Fill, 400_000)),
        ),
        (
            "BGT-100.4",
            ord("cancelled", "0", "0", "0", "0", ""),
            Some((OrderEventKind::CancelAck, 0)),
        ),
        (
            "BGT-100.5",
            ord("cancelled", "0.001", "65.0001", "65000.1", "0.01", "")
                .replace(r#""timeInForce":"gtc""#, r#""timeInForce":"ioc""#),
            Some((OrderEventKind::Expired, 100_000)),
        ),
        (
            "BGT-100.8",
            ord("cancelled", "0", "0", "0", "0", "").replace(
                r#""cancelReason":"""#,
                r#""cancelReason":"post_only_fill_cancel""#,
            ),
            Some((OrderEventKind::Expired, 0)),
        ),
        ("BGT-100.6", ord("foo", "0", "0", "0", "0", ""), None),
        (
            "BGT-100.7",
            ord("filled", "0", "0", "0", "0", "").replace(r#""cumExecQty":"0","#, ""),
            None,
        ),
    ];
    for (row, p, want) in rows {
        let (r, s) = dec(&mut GwCtx::new(), &p);
        match want {
            Some((what, cum)) => {
                let e = one(&s);
                assert_eq!(
                    (e.what, e.slot, e.r#gen, e.epoch, e.exch_order_id, e.cum_filled),
                    (what, SlotId(42), 7, 3, 1_234_567_890_123, Qty(cum)),
                    "{row}"
                );
                assert_eq!(r.unwrap().events, 1, "{row}");
            }
            None => {
                assert_eq!(r, Err(DecodeErr::Shape), "{row}");
                assert_eq!(s.events(), 0, "{row}");
            }
        }
    }
}

#[test]
fn bgt_101_partial_fill_fields() {
    // Two fills of one order: 0.002 @ 65000 then 0.001 @ 65000.1.
    let mut ctx = GwCtx::new();
    let (_, s) = dec(
        &mut ctx,
        &ord("partially_filled", "0.002", "130", "65000", "0.026", ""),
    );
    assert_eq!(one(&s).qty, Qty(200_000));
    let (r, s) = dec(
        &mut ctx,
        &ord("partially_filled", "0.003", "195.0001", "65000.03", "0.039", ""),
    );
    let e = one(&s);
    assert_eq!(
        e,
        OrderEvent {
            venue: FUT,
            account: ACCT,
            what: OrderEventKind::PartialFill,
            detail: 0,
            slot: SlotId(42),
            r#gen: 7,
            inst: F_BTC,
            px: Px(6_500_010),
            qty: Qty(100_000),
            cum_filled: Qty(300_000),
            fee: Qty(1_300_000),
            fee_asset: USDT,
            epoch: 3,
            reject_code: 0,
            exch_order_id: 1_234_567_890_123,
            ex_ts_ns: 1_790_000_000_500_000_000,
            hw_rx_ns: RX.hw_rx_ns,
            rx_tsc: RX.rx_tsc,
            avg_px: Px(6_500_003),
            cash_qty: Qty(0),
            cash_sum: Qty(0),
            trade_id: 0,
        }
    );
    let o = r.unwrap();
    assert_eq!((o.events, o.skip, o.fee_rule_deviation), (1, Skip::No, false));
    // Classic V2 per-fill fields (`baseVolume`, `fillPrice`, `fillFee`, `tradeId`, `fillTime`) win.
    let v2 = r#"{"action":"snapshot","arg":{"instType":"USDT-FUTURES","channel":"orders","instId":"default"},"data":[{"instId":"BTCUSDT","orderId":"1234567890123","clientOid":"100030a020002a000000078d","force":"gtc","tradeSide":"open","side":"buy","status":"partially_filled","baseVolume":"0.001","fillPrice":"65000.1","accBaseVolume":"0.003","priceAvg":"65000.05","fillFee":"-0.039","fillFeeCoin":"USDT","tradeId":"9003","fillTime":"1790000000500"}],"ts":1}"#;
    let (r, s) = dec(&mut GwCtx::new(), v2);
    assert_eq!(r.unwrap().events, 1, "BGT-014");
    let e = one(&s);
    assert_eq!(
        (
            e.what,
            e.px,
            e.qty,
            e.cum_filled,
            e.avg_px,
            e.fee,
            e.fee_asset,
            e.trade_id,
            e.ex_ts_ns
        ),
        (
            OrderEventKind::PartialFill,
            Px(6_500_010),
            Qty(100_000),
            Qty(300_000),
            Px(6_500_005),
            Qty(3_900_000),
            USDT,
            9003,
            1_790_000_000_500_000_000
        ),
        "BGT-101 (V2)"
    );
}

#[test]
fn bgt_014_push_class_resolves() {
    // The same order as an arg-first push (→ Orders) and a data-first push (→ Push).
    let a = ord("partially_filled", "0.001", "65.0001", "65000.1", "0.01", "");
    let b = format!(
        r#"{{"data":{},"ts":1790000000501,"action":"snapshot","arg":{{"instType":"UTA","topic":"order"}}}}"#,
        &a[a.find(r#"[{"category""#).unwrap()..a.rfind(r#","ts":"#).unwrap()]
    );
    assert_eq!(
        Fut::classify_report(MsgOp::Text, support::head(&a)),
        ReportClass::Orders
    );
    assert_eq!(
        Fut::classify_report(MsgOp::Text, support::head(&b)),
        ReportClass::Push
    );
    let (ra, sa) = dec(&mut GwCtx::new(), &a);
    let (rb, sb) = dec(&mut GwCtx::new(), &b);
    assert_eq!(sa.orders, sb.orders);
    assert_eq!(ra.unwrap(), rb.unwrap());
}

#[test]
fn bgt_102_fill_dedup() {
    let mut ctx = GwCtx::new();
    let p = ord("partially_filled", "0.001", "65.0001", "65000.1", "0.01", "");
    let (_, s) = dec(&mut ctx, &p);
    assert_eq!(one(&s).what, OrderEventKind::PartialFill);
    let (r, s) = dec(&mut ctx, &p);
    assert_eq!((r.unwrap().events, s.events()), (0, 0));
}

#[test]
fn bgt_103_same_trade_id_two_insts() {
    let mut ctx = GwCtx::new();
    let a = ord_on(
        "usdt-futures",
        "BTCUSDT",
        "1",
        CID7,
        "filled",
        "0.001",
        "65",
        "65000",
        "0",
        "",
    );
    let b = ord_on(
        "usdt-futures",
        "ETHUSDT",
        "2",
        "100030a020002b000000018a",
        "filled",
        "0.01",
        "30",
        "3000",
        "0",
        "",
    );
    let (_, sa) = dec(&mut ctx, &a);
    let (_, sb) = dec(&mut ctx, &b);
    assert_eq!((one(&sa).inst, one(&sb).inst), (F_BTC, F_ETH));
}

#[test]
fn bgt_104_dedup_without_trade_id() {
    let mut ctx = GwCtx::new();
    let rows = [
        ("BGT-104.1", "0.001", Some(100_000)),
        ("BGT-104.2", "0.001", None),
        ("BGT-104.3", "0.002", Some(200_000)),
        ("BGT-104.4", "0.002", None),
        ("BGT-104.5", "0.003", Some(300_000)),
    ];
    for (row, cum, want) in rows {
        let (_, s) = dec(&mut ctx, &ord("partially_filled", cum, "", "65000", "0", ""));
        match want {
            Some(c) => assert_eq!(one(&s).cum_filled, Qty(c), "{row}"),
            None => assert_eq!(s.events(), 0, "{row}"),
        }
    }
}

#[test]
fn bgt_106_cumulative_fee_diff() {
    let mut ctx = GwCtx::new();
    let (_, a) = dec(
        &mut ctx,
        &ord("partially_filled", "0.001", "65", "65000", "0.01", ""),
    );
    let (_, b) = dec(&mut ctx, &ord("filled", "0.004", "260", "65000", "0.03", ""));
    assert_eq!(
        (one(&a).fee, one(&b).fee, one(&b).fee_asset),
        (Qty(1_000_000), Qty(2_000_000), USDT)
    );
    // V2 reports fees negative: the event carries the absolute amount.
    let (_, c) = dec(
        &mut GwCtx::new(),
        &ord("filled", "0.004", "260", "65000", "-0.03", ""),
    );
    assert_eq!(one(&c).fee, Qty(3_000_000));
}

#[test]
fn bgt_108_fee_rule_deviation() {
    let fee_push = |sym: &str, side: &str, coin: &str, spot: bool| {
        let cat = if spot { "spot" } else { "usdt-futures" };
        ord_on(cat, sym, "1", CID7, "filled", "1", "65000", "65000", "0.5", "")
            .replace(r#""feeCoin":"USDT""#, &format!(r#""feeCoin":"{coin}""#))
            .replace(r#""side":"buy""#, &format!(r#""side":"{side}""#))
    };
    let (r, s) = dec(&mut GwCtx::new(), &fee_push("BTCUSDT", "buy", "USDT", false));
    assert!(!r.unwrap().fee_rule_deviation, "BGT-108.1");
    assert_eq!(one(&s).fee_asset, USDT);
    let (r, s) = dec(&mut GwCtx::new(), &fee_push("BTCUSDT", "buy", "BGB", false));
    assert!(r.unwrap().fee_rule_deviation, "BGT-108.2");
    assert_eq!(one(&s).fee_asset, BGB, "BGT-108.2");
    let spot_cid = cid_text(&cid(SPOT, 3, 42, 7));
    let sp = |sym: &str, side: &str, coin: &str| fee_push(sym, side, coin, true).replace(CID7, &spot_cid);
    for (row, sym, side, coin, dev, asset) in [
        ("BGT-108.3", "BTCUSDT", "buy", "BTC", false, BTC),
        ("BGT-108.4", "BTCUSDT", "buy", "BGB", false, BGB),
        ("BGT-108.5", "BTCUSDT", "buy", "USDT", true, USDT),
        ("BGT-108.6", "BGBUSDT", "sell", "USDT", false, USDT),
    ] {
        let (r, s) = dec_spot(&mut GwCtx::new(), &sp(sym, side, coin));
        assert_eq!(r.unwrap().fee_rule_deviation, dev, "{row}");
        let e = one(&s);
        assert_eq!((e.fee_asset, e.venue), (asset, SPOT), "{row}");
        assert_eq!(e.inst, if sym == "BTCUSDT" { S_BTC } else { S_BGB }, "{row}");
    }
}

#[test]
fn bgt_109_external_order() {
    let push = |trade_side: &str, coid: &str| {
        ord_on(
            "usdt-futures",
            "BTCUSDT",
            "777000",
            coid,
            "filled",
            "0.004",
            "208.002",
            "52000.5",
            "0.6",
            "",
        )
        .replace(
            r#""tradeSide":"buy_single""#,
            &format!(r#""tradeSide":"{trade_side}""#),
        )
    };
    let bad = "100030a020002a000000078c";
    let rows = [
        (
            "BGT-109.1",
            "burst_close_long",
            CID7,
            Some(ExternalCause::Liquidation),
        ),
        (
            "BGT-109.2",
            "burst_close_short",
            "",
            Some(ExternalCause::Liquidation),
        ),
        (
            "BGT-109.3",
            "dte_sys_adl_close_long",
            "",
            Some(ExternalCause::Adl),
        ),
        (
            "BGT-109.4",
            "dte_sys_adl_close_short",
            "abc",
            Some(ExternalCause::Adl),
        ),
        (
            "BGT-109.5",
            "delivery_close_long",
            "",
            Some(ExternalCause::Delivery),
        ),
        (
            "BGT-109.6",
            "delivery_close_short",
            "",
            Some(ExternalCause::Delivery),
        ),
        ("BGT-109.7", "foo_bar", "abc", Some(ExternalCause::Unknown)),
        ("BGT-109.8", "foo_bar", CID7, None),
        ("BGT-109.9", "open", CID7, None),
        ("BGT-109.10", "open", "web_123", Some(ExternalCause::Unknown)),
        ("BGT-109.11", "open", bad, Some(ExternalCause::Unknown)),
        (
            "BGT-109.14",
            "burst_sell_single",
            CID7,
            Some(ExternalCause::Liquidation),
        ),
        (
            "BGT-109.15",
            "dte_sys_adl_buy_in_single_side_mode",
            "",
            Some(ExternalCause::Adl),
        ),
        (
            "BGT-109.16",
            "delivery_buy_single",
            "",
            Some(ExternalCause::Delivery),
        ),
        (
            "BGT-109.17",
            "reduce_sell_single",
            CID7,
            Some(ExternalCause::Liquidation),
        ),
    ];
    for (row, ts, coid, want) in rows {
        let (_, s) = dec(&mut GwCtx::new(), &push(ts, coid));
        match want {
            Some(cause) => {
                assert_eq!(s.ext.len(), 1, "{row}");
                assert_eq!(s.ext[0].cause, cause, "{row}");
                assert!(s.orders.is_empty(), "{row}");
            }
            None => {
                let e = one(&s);
                assert_eq!(
                    (e.what, e.slot, e.r#gen),
                    (OrderEventKind::Fill, SlotId(42), 7),
                    "{row}"
                );
                assert!(s.ext.is_empty(), "{row}");
            }
        }
    }
    // Spot: no liquidation marker; a foreign client id is Unknown, ours is an OrderEvent.
    let spot_cid = cid_text(&cid(SPOT, 3, 42, 7));
    let sp = |coid: &str| {
        ord_on(
            "spot", "BTCUSDT", "5", coid, "filled", "1", "65000", "65000", "0", "",
        )
        .replace(r#""tradeSide":"buy_single""#, r#""tradeSide":"burst_close_long""#)
    };
    let (_, s) = dec_spot(&mut GwCtx::new(), &sp("abc"));
    assert_eq!(s.ext[0].cause, ExternalCause::Unknown, "BGT-109.12");
    let (_, s) = dec_spot(&mut GwCtx::new(), &sp(&spot_cid));
    assert_eq!(one(&s).what, OrderEventKind::Fill, "BGT-109.13");
}

#[test]
fn bgt_110_external_fields() {
    let p = ord_on(
        "usdt-futures",
        "BTCUSDT",
        "777000",
        "",
        "filled",
        "0.004",
        "208.002",
        "52000.5",
        "0.6",
        "",
    )
    .replace(r#""tradeSide":"buy_single""#, r#""tradeSide":"burst_close_long""#)
    .replace(r#""side":"buy""#, r#""side":"sell""#)
    .replace(
        r#""updatedTime":"1790000000500""#,
        r#""updatedTime":"1790000000600""#,
    );
    let (_, s) = dec(&mut GwCtx::new(), &p);
    let x = s.ext[0];
    assert_eq!(
        (
            x.venue,
            x.account,
            x.side,
            x.cause,
            x.inst,
            x.qty,
            x.px,
            x.fee,
            x.fee_asset
        ),
        (
            FUT,
            ACCT,
            Side::Sell,
            ExternalCause::Liquidation,
            F_BTC,
            Qty(400_000),
            Px(5_200_050),
            Qty(60_000_000),
            USDT
        ),
        "BGT-110.2"
    );
    assert_eq!(
        (x.exch_order_id, x.trade_id, x.ex_ts_ns, x.rx_tsc.0, x.fill_seq),
        (777_000, 0, 1_790_000_000_600_000_000, 1000, 400_000),
        "BGT-110.2"
    );
    // V2 per-fill trade id.
    let v2 = p.replace(r#""stpMode":"none""#, r#""stpMode":"none","tradeId":"9301""#);
    let (_, s) = dec(&mut GwCtx::new(), &v2);
    assert_eq!((s.ext[0].trade_id, s.ext[0].fill_seq), (9301, 0), "BGT-110.1");
}

#[test]
fn bgt_111_missing_client_oid() {
    let p = ord_on(
        "usdt-futures",
        "BTCUSDT",
        "1234567890123",
        "",
        "filled",
        "0.004",
        "260",
        "65000",
        "0",
        "",
    );
    let (_, s) = dec(&mut GwCtx::new(), &p);
    let e = one(&s);
    assert_eq!(
        (e.what, e.slot, e.epoch, e.exch_order_id),
        (OrderEventKind::Fill, SlotId::UNRESOLVED, 0, 1_234_567_890_123)
    );
    assert!(s.ext.is_empty());
    assert_eq!(s.exch_ids, vec![(b"1234567890123".to_vec(), None)]);
    // A resolved IdTable answer is used.
    let mut s = RepSink {
        hint: ExchIdHint::Resolved {
            epoch: 3,
            slot: SlotId(42),
            r#gen: 7,
        },
        ..RepSink::default()
    };
    let c = Fut::classify_report(MsgOp::Text, support::head(&p));
    Fut::decode_report(rd(), &mut GwCtx::new(), c, p.as_bytes(), &RX, &mut s).unwrap();
    assert_eq!((one(&s).slot, one(&s).epoch), (SlotId(42), 3));
    // Non-fill events of an order without a client id are dropped.
    let live = ord_on("usdt-futures", "BTCUSDT", "1", "", "live", "0", "0", "0", "0", "");
    let (r, s) = dec(&mut GwCtx::new(), &live);
    assert_eq!((r.unwrap().events, s.events()), (0, 0));
}

#[test]
fn bgt_112_external_dedup() {
    let mut ctx = GwCtx::new();
    let p = |cum: &str| {
        ord_on(
            "usdt-futures",
            "BTCUSDT",
            "777000",
            "",
            "partially_filled",
            cum,
            "",
            "52000.5",
            "0",
            "",
        )
        .replace(r#""tradeSide":"buy_single""#, r#""tradeSide":"burst_close_long""#)
    };
    let (_, s) = dec(&mut ctx, &p("0.004"));
    assert_eq!(s.ext.len(), 1, "BGT-112.1");
    let (_, s) = dec(&mut ctx, &p("0.004"));
    assert_eq!(s.events(), 0, "BGT-112.2");
    let (_, s) = dec(&mut ctx, &p("0.006"));
    assert_eq!(
        (s.ext[0].qty, s.ext[0].fill_seq),
        (Qty(200_000), 600_000),
        "BGT-112.3"
    );
}

// ---------------------------------------------------------------- position / account channels

fn pos(rows: &[(&str, &str, &str, &str)]) -> String {
    let rows: Vec<String> = rows
        .iter()
        .map(|(sym, side, size, liq)| {
            format!(
                r#"{{"symbol":"{sym}","leverage":"20","available":"{size}","liqPrice":"{liq}","marginMode":"crossed","unrealisedPnl":"10.5","markPrice":"64010","avgPrice":"64000","marginCoin":"USDT","frozen":"0","size":"{size}","positionStatus":"opening","posSide":"{side}","holdMode":"one_way_mode","updatedTime":"1790000000700"}}"#
            )
        })
        .collect();
    format!(
        r#"{{"data":[{}],"arg":{{"instType":"UTA","topic":"position"}},"action":"snapshot","ts":1790000000701}}"#,
        rows.join(",")
    )
}

#[test]
fn bgt_113_position_report() {
    let (_, s) = dec(&mut GwCtx::new(), &pos(&[("BTCUSDT", "long", "0.01", "52000.5")]));
    let e = s.pos[0];
    assert_eq!(
        (
            e.venue, e.account, e.inst, e.qty, e.entry_px, e.upnl.0, e.liq_px, e.ex_ts_ns
        ),
        (
            FUT,
            ACCT,
            F_BTC,
            Qty(1_000_000),
            Px(6_400_000),
            10_500_000,
            Px(5_200_050),
            1_790_000_000_700_000_000
        ),
        "BGT-113.1"
    );
    let (_, s) = dec(&mut GwCtx::new(), &pos(&[("BTCUSDT", "short", "0.01", "")]));
    assert_eq!(
        (s.pos[0].qty, s.pos[0].liq_px),
        (Qty(-1_000_000), Px(0)),
        "BGT-113.2/3"
    );
    let (_, s) = dec(&mut GwCtx::new(), &pos(&[("BTCUSDT", "long", "0.01", "0")]));
    assert_eq!(s.pos[0].liq_px, Px(0), "BGT-113.3");
    let (r, s) = dec(&mut GwCtx::new(), &pos(&[("DOGEUSDT", "long", "1", "")]));
    assert_eq!((r.unwrap().events, s.events()), (0, 0), "BGT-113.4");
    let (_, s) = dec(
        &mut GwCtx::new(),
        &pos(&[("BTCUSDT", "long", "0.01", ""), ("ETHUSDT", "long", "0.1", "")]),
    );
    assert_eq!((s.pos.len(), s.margin), (2, 0), "BGT-113.5");
}

fn acct(coins: &[(&str, &str, &str)]) -> String {
    let rows: Vec<String> = coins
        .iter()
        .map(|(c, a, l)| {
            format!(r#"{{"debts":"0","balance":"{a}","available":"{a}","borrow":"0","locked":"{l}","equity":"{a}","coin":"{c}","usdValue":"1"}}"#)
        })
        .collect();
    format!(
        r#"{{"data":[{{"unrealisedPnL":"0","totalEquity":"1","positionMgnRatio":"0","mmr":"0","effEquity":"1","imr":"0","mgnRatio":"0","coin":[{}]}}],"arg":{{"instType":"UTA","topic":"account"}},"action":"snapshot","ts":1790000000800}}"#,
        rows.join(",")
    )
}

#[test]
fn bgt_114_account_push() {
    let (_, s) = dec(&mut GwCtx::new(), &acct(&[("USDT", "8000.5", "100")]));
    let b = s.bal[0];
    assert_eq!(
        (b.venue, b.account, b.asset, b.free, b.locked, b.ex_ts_ns),
        (
            FUT,
            ACCT,
            USDT,
            Qty(800_050_000_000),
            Qty(10_000_000_000),
            1_790_000_000_800_000_000
        ),
        "BGT-114.1/2"
    );
    assert_eq!(s.margin, 0, "BGT-114.1");
    let (_, s) = dec_spot(
        &mut GwCtx::new(),
        &acct(&[("BTC", "0.5", "0"), ("USDT", "1000", "0")]),
    );
    assert_eq!(
        s.bal.iter().map(|b| (b.venue, b.asset)).collect::<Vec<_>>(),
        vec![(SPOT, BTC), (SPOT, USDT)],
        "BGT-114.3"
    );
    let (_, s) = dec(&mut GwCtx::new(), &acct(&[("XYZ", "1", "0"), ("USDT", "1", "0")]));
    assert_eq!(
        s.bal.iter().map(|b| b.asset).collect::<Vec<_>>(),
        vec![USDT],
        "BGT-114.4"
    );
}

#[test]
fn bgt_115_skip_events() {
    let ok0 = |r: Result<ReportOutcome, DecodeErr>, skip: Skip, row: &str| {
        let o = r.unwrap();
        assert_eq!((o.events, o.skip), (0, skip), "{row}");
    };
    for (row, p) in [
        ("BGT-115.1", r#"{"event":"login","code":0}"#),
        (
            "BGT-115.2",
            r#"{"event":"subscribe","arg":{"instType":"UTA","topic":"order"}}"#,
        ),
        ("BGT-115.3", r#"{"event":"error","code":30001,"msg":"x"}"#),
        ("BGT-115.4", "pong"),
    ] {
        ok0(dec(&mut GwCtx::new(), p).0, Skip::No, row);
    }
    let unk = r#"{"arg":{"instType":"UTA","topic":"strategy-x"},"data":[]}"#;
    ok0(dec(&mut GwCtx::new(), unk).0, Skip::Unknown, "BGT-115.5");
    let rd2 = support::rd_ignoring(&["strategy-x"]);
    ok0(dec_with(rd2, &mut GwCtx::new(), unk).0, Skip::Listed, "BGT-115.6");
    let late =
        r#"{"data":[{"x":1}],"action":"snapshot","ts":1,"arg":{"instType":"UTA","topic":"strategy-x"}}"#;
    ok0(dec(&mut GwCtx::new(), late).0, Skip::Unknown, "BGT-115.7");
    assert!(
        support::caps(FUT).ignorable_user_events.is_empty()
            && support::caps(SPOT).ignorable_user_events.is_empty(),
        "BGT-115.8"
    );
}

#[test]
fn bgt_116_decode_errors() {
    let base = ord("partially_filled", "0.003", "195", "65000", "0.039", "");
    let rows: Vec<(&str, String, Result<u16, DecodeErr>)> = vec![
        (
            "BGT-116.1",
            base.replace(r#""cumExecQty":"0.003""#, r#""cumExecQty":"abc""#),
            Err(DecodeErr::Number),
        ),
        (
            "BGT-116.2",
            base[..base.find("\"orderStatus\"").unwrap()].to_string(),
            Err(DecodeErr::Truncated),
        ),
        (
            "BGT-116.3",
            r#"{"arg":{"instType":"UTA","topic":"order"},"data":{"a":1}}"#.to_string(),
            Err(DecodeErr::Shape),
        ),
        (
            "BGT-116.4",
            base.replace(r#""cumExecQty":"0.003""#, r#""cumExecQty":"1e30""#),
            Err(DecodeErr::Overflow),
        ),
        (
            "BGT-116.5",
            base.replace(r#""symbol":"BTCUSDT""#, r#""symbol":"DOGEUSDT""#),
            Ok(0),
        ),
        (
            "BGT-116.6",
            base.replace(r#""avgPrice":"65000""#, r#""avgPrice":"""#),
            Err(DecodeErr::Shape),
        ),
        ("BGT-116.7", base.replace("usdt-futures", "spot"), Ok(0)),
    ];
    for (row, p, want) in rows {
        let (r, s) = dec(&mut GwCtx::new(), &p);
        assert_eq!(r.map(|o| o.events), want, "{row}");
        if want.is_err() {
            assert_eq!(s.events(), 0, "{row}");
        }
    }
}

#[test]
fn bgt_117_no_funding_settle() {
    let mut ctx = GwCtx::new();
    let mut s = RepSink::default();
    for p in [
        ord("filled", "0.004", "260", "65000", "0.04", ""),
        pos(&[("BTCUSDT", "long", "0.01", "")]),
        acct(&[("USDT", "8000.5", "100")]),
    ] {
        let c = Fut::classify_report(MsgOp::Text, support::head(&p));
        Fut::decode_report(rd(), &mut ctx, c, p.as_bytes(), &RX, &mut s).unwrap();
    }
    assert_eq!(s.funding, 0);
    let _ = AssetId(0);
}
