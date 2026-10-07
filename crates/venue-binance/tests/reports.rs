//! BIN-090, BIN-091, BIN-094, BIN-096: WS API answers.

mod support;

use hft_event::{OrderEvent, OrderEventKind};
use hft_refdata::LimitBucket;
use hft_types::{Px, Qty, SlotId, VenueId};
use hft_venue_proto::{DecodeErr, MsgOp, RateObs, ReportOutcome, ReqRef, Skip, VenueProtocol};
use support::{RX, RepSink, S_BTC, U_BTC, cid, rd};
use venue_binance::{GwCtx, Spot, Usdm};

const RL3: &str = r#"[{"rateLimitType":"ORDERS","interval":"SECOND","intervalNum":10,"limit":300,"count":12},{"rateLimitType":"ORDERS","interval":"MINUTE","intervalNum":1,"limit":1200,"count":40},{"rateLimitType":"REQUEST_WEIGHT","interval":"MINUTE","intervalNum":1,"limit":2400,"count":5}]"#;

fn ack(rl: Option<&str>) -> String {
    let base = r#"{"id":"p0000002a00000007","status":200,"result":{"orderId":325078477,"symbol":"BTCUSDT","status":"NEW","clientOrderId":"1000301020002a000000073d","updateTime":1790000000010}"#;
    match rl {
        Some(r) => format!(r#"{base},"rateLimits":{r}}}"#),
        None => format!("{base}}}"),
    }
}

fn dec_u(p: &str) -> (Result<ReportOutcome, DecodeErr>, RepSink) {
    let mut s = RepSink::default();
    let c = Usdm::classify_report(MsgOp::Text, &p.as_bytes()[..p.len().min(64)]);
    let r = Usdm::decode_report(rd(), &mut GwCtx, c, p.as_bytes(), &RX, &mut s);
    (r, s)
}

fn dec_s(p: &str) -> (Result<ReportOutcome, DecodeErr>, RepSink) {
    let mut s = RepSink::default();
    let c = Spot::classify_report(MsgOp::Text, &p.as_bytes()[..p.len().min(64)]);
    let r = Spot::decode_report(rd(), &mut GwCtx, c, p.as_bytes(), &RX, &mut s);
    (r, s)
}

fn one(s: &RepSink) -> OrderEvent {
    assert_eq!(s.orders.len(), 1, "{:?}", s.orders);
    s.orders[0]
}

#[test]
fn bin_090_usdm_place_ack() {
    let (r, s) = dec_u(&ack(Some(RL3)));
    let o = r.unwrap();
    let e = one(&s);
    assert_eq!(
        (
            e.what,
            e.venue,
            e.account,
            e.epoch,
            e.slot,
            e.r#gen,
            e.inst,
            e.exch_order_id
        ),
        (
            OrderEventKind::Ack,
            VenueId::BINANCE_USDM,
            support::ACCT,
            3,
            SlotId(42),
            7,
            U_BTC,
            325_078_477
        )
    );
    assert_eq!((e.cum_filled, e.avg_px, e.reject_code), (Qty(0), Px(0), 0));
    assert_eq!(s.exch_ids.len(), 1);
    assert_eq!(s.exch_ids[0].0, b"325078477".to_vec());
    assert_eq!(s.exch_ids[0].1, Some(cid(VenueId::BINANCE_USDM, 3, 42, 7)));
    assert_eq!(
        o,
        ReportOutcome {
            req: Some(ReqRef {
                slot: 42,
                r#gen: 7,
                op: hft_event::GwOp::Place
            }),
            rate: Some(RateObs {
                bucket: LimitBucket::Orders,
                used: 12,
                limit: 300,
                window_ms: 10_000
            }),
            events: 1,
            outcome_unknown: None,
            raw_code: 0,
            skip: Skip::No,
            ..ReportOutcome::default()
        }
    );
}

#[test]
fn bin_091_rate_limits_pick() {
    let item = |t: &str, iv: &str, n: u32, l: u32, c: u32| {
        format!(r#"{{"rateLimitType":"{t}","interval":"{iv}","intervalNum":{n},"limit":{l},"count":{c}}}"#)
    };
    let obs = |bucket, used, limit, window_ms| {
        Some(RateObs {
            bucket,
            used,
            limit,
            window_ms,
        })
    };
    let rows: Vec<(&str, Option<String>, Option<RateObs>)> = vec![
        (
            "BIN-091.1",
            Some(RL3.to_string()),
            obs(LimitBucket::Orders, 12, 300, 10_000),
        ),
        (
            "BIN-091.2",
            Some(format!(
                "[{},{}]",
                item("ORDERS", "SECOND", 10, 300, 12),
                item("REQUEST_WEIGHT", "MINUTE", 1, 2400, 2000)
            )),
            obs(LimitBucket::Weight, 2000, 2400, 60_000),
        ),
        (
            "BIN-091.3",
            Some(format!(
                "[{},{}]",
                item("ORDERS", "SECOND", 10, 300, 30),
                item("ORDERS", "MINUTE", 1, 1200, 120)
            )),
            obs(LimitBucket::Orders, 30, 300, 10_000),
        ),
        (
            "BIN-091.4",
            Some(format!("[{}]", item("RAW_REQUESTS", "MINUTE", 1, 61000, 3))),
            None,
        ),
        ("BIN-091.5", None, None),
        (
            "BIN-091.6",
            Some(format!("[{}]", item("ORDERS", "DAY", 1, 200_000, 100))),
            obs(LimitBucket::Orders, 100, 200_000, 86_400_000),
        ),
    ];
    for (row, rl, want) in rows {
        let (r, s) = dec_u(&ack(rl.as_deref()));
        assert_eq!(r.unwrap().rate, want, "{row}");
        assert_eq!(s.orders.len(), 1, "{row}");
    }
}

fn query(status: &str, z: &str, ap_field: &str, ap: &str) -> String {
    format!(
        r#"{{"id":"q0000002a00000007","status":200,"result":{{"orderId":325078477,"symbol":"BTCUSDT","status":"{status}","clientOrderId":"1000301020002a000000073d","price":"65000.1","origQty":"0.004","executedQty":"{z}","{ap_field}":"{ap}","type":"LIMIT","side":"BUY"}}}}"#
    )
}

#[test]
fn bin_094_query_result() {
    let rows = [
        ("BIN-094.1", "FILLED", "0.004", "65000.15", 3, 400_000, 6_500_015),
        (
            "BIN-094.2",
            "PARTIALLY_FILLED",
            "0.002",
            "65000.10",
            2,
            200_000,
            6_500_010,
        ),
        ("BIN-094.3", "NEW", "0", "0.00", 1, 0, 0),
        ("BIN-094.4", "CANCELED", "0.001", "65000.1", 4, 100_000, 6_500_010),
        ("BIN-094.5", "EXPIRED", "0", "0", 6, 0, 0),
        ("BIN-094.6", "EXPIRED_IN_MATCH", "0", "0", 6, 0, 0),
    ];
    for (row, st, z, ap, detail, cum, avg) in rows {
        let (r, s) = dec_u(&query(st, z, "avgPrice", ap));
        r.unwrap();
        let e = one(&s);
        assert_eq!(
            (e.what, e.slot, e.r#gen, e.epoch, e.detail, e.cum_filled, e.avg_px),
            (
                OrderEventKind::QueryResult,
                SlotId(42),
                7,
                3,
                detail,
                Qty(cum),
                Px(avg)
            ),
            "{row}"
        );
    }
    let (r, s) = dec_u(&query("FILLED", "0.004", "avgPrice", "0"));
    assert_eq!(r, Err(DecodeErr::Shape), "BIN-094.7");
    assert!(s.orders.is_empty(), "BIN-094.7");
    let spot = |st: &str, z: &str, q: &str| {
        query(st, z, "cummulativeQuoteQty", q).replace("1000301020002a000000073d", "1000302020002a000000070d")
    };
    for (row, st, z, q, detail, cum, avg) in [
        ("BIN-094.8", "FILLED", "0.003", "195.00", 3, 300_000, 6_500_000),
        (
            "BIN-094.9",
            "PARTIALLY_FILLED",
            "0.003",
            "100.00",
            2,
            300_000,
            3_333_333,
        ),
        ("BIN-094.10", "NEW", "0", "0", 1, 0, 0),
    ] {
        let (r, s) = dec_s(&spot(st, z, q));
        r.unwrap();
        let e = one(&s);
        assert_eq!(
            (e.what, e.inst, e.detail, e.cum_filled, e.avg_px),
            (OrderEventKind::QueryResult, S_BTC, detail, Qty(cum), Px(avg)),
            "{row}"
        );
    }
}

#[test]
fn bin_096_cancel_amend_acks() {
    // BIN-096.1: USDT-M order.cancel answer → CancelAck of the target (client id `clientOrderId`).
    let p = r#"{"id":"c0000002a00000007","status":200,"result":{"clientOrderId":"1000301020002a000000073d","cumQty":"0","cumQuote":"0","executedQty":"0","orderId":325078477,"origQty":"0.004","price":"65000.1","reduceOnly":false,"side":"BUY","positionSide":"BOTH","status":"CANCELED","symbol":"BTCUSDT","timeInForce":"GTX","type":"LIMIT","updateTime":1790000000020}}"#;
    let (r, s) = dec_u(p);
    let o = r.unwrap();
    let e = one(&s);
    assert_eq!(
        (
            e.what,
            e.epoch,
            e.slot,
            e.r#gen,
            e.inst,
            e.exch_order_id,
            e.ex_ts_ns
        ),
        (
            OrderEventKind::CancelAck,
            3,
            SlotId(42),
            7,
            U_BTC,
            325_078_477,
            1_790_000_000_020_000_000
        ),
        "BIN-096.1"
    );
    assert_eq!(o.req.map(|q| q.op), Some(hft_event::GwOp::Cancel), "BIN-096.1");
    assert!(s.exch_ids.is_empty(), "BIN-096.1");
    // BIN-096.2: spot order.cancel → the cancelled order's id is `origClientOrderId`.
    let p = r#"{"id":"c0000002a00000007","status":200,"result":{"symbol":"BTCUSDT","origClientOrderId":"1000302020002a000000070d","orderId":12569099453,"orderListId":-1,"clientOrderId":"91fe37ce9e69c90d6358c0","transactTime":1790000000020000,"price":"65000.10000000","origQty":"0.00100000","executedQty":"0.00000000","cummulativeQuoteQty":"0.00000000","status":"CANCELED","timeInForce":"GTC","type":"LIMIT_MAKER","side":"BUY","selfTradePreventionMode":"EXPIRE_MAKER"}}"#;
    let (r, s) = dec_s(p);
    r.unwrap();
    let e = one(&s);
    assert_eq!(
        (
            e.what,
            e.epoch,
            e.slot,
            e.r#gen,
            e.inst,
            e.exch_order_id,
            e.ex_ts_ns
        ),
        (
            OrderEventKind::CancelAck,
            3,
            SlotId(42),
            7,
            S_BTC,
            12_569_099_453,
            1_790_000_000_020_000_000
        ),
        "BIN-096.2"
    );
    // BIN-096.3: USDT-M order.modify → AmendAck.
    let p = r#"{"id":"m0000002a00000007","status":200,"result":{"orderId":325078477,"symbol":"BTCUSDT","status":"NEW","clientOrderId":"1000301020002a000000073d","price":"65000.2","origQty":"0.003","updateTime":1790000000030}}"#;
    let (r, s) = dec_u(p);
    r.unwrap();
    let e = one(&s);
    assert_eq!(
        (e.what, e.slot, e.r#gen, e.epoch),
        (OrderEventKind::AmendAck, SlotId(42), 7, 3),
        "BIN-096.3"
    );
    // BIN-096.4: spot openOrders.cancelAll answer (array result) → no event.
    let p = r#"{"id":"x0000000000000000","status":200,"result":[{"symbol":"BTCUSDT","origClientOrderId":"1000302020002a000000070d","orderId":1,"status":"CANCELED"}]}"#;
    let (r, s) = dec_s(p);
    assert_eq!(r.unwrap().events, 0, "BIN-096.4");
    assert!(s.orders.is_empty(), "BIN-096.4");
}
