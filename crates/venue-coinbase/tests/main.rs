//! venue-coinbase tests (TESTS.md). One binary: unit tables, goldens from the 2026-11-06 capture,
//! the two-connection race replay, contract checks and the fuzz truncation smoke.
#![allow(clippy::disallowed_types)] // serde_json::Value: golden comparison in test code, not a hot path
mod adv;
mod support;

#[path = "../fuzz/exercise.rs"]
mod fuzz_exercise;

use hft_event::Bbo;
use hft_types::{InstId, Side};
use hft_venue_proto::account::{AccountProtocol, Query, State};
use hft_venue_proto::testkit::{MdRec, MdSample, VecSink, check_default_caps, check_md_basics};
use hft_venue_proto::{
    ControlHint, Credential, DecodeErr, EndpointSpec, LoginResult, LoginStep, MsgOp, ProtoErr, RestReqBuf,
    Role, RouteId, RouteKind, SeqRule, StaleGuard, StreamKind, SubFrames, SubSpec, VenueCfg, VenueEnv,
    VenueProtocol,
};
use serde_json::{Value, json};
use support::{BTC_USD, FOREIGN, RX, fixture, jsonl, rd};
use venue_coinbase::{CoinbaseSpot as Cb, FeedCtx, MdClass, ROUTE_BBO, ROUTE_MD, iso8601_ns};

const CFG: VenueCfg = VenueCfg {
    env: VenueEnv::Prod,
    stale_guard_ms: 1500,
};

fn class(p: &[u8]) -> MdClass {
    Cb::classify(ROUTE_BBO, MsgOp::Text, p.get(..64).unwrap_or(p))
}

fn decode(p: &[u8]) -> Result<Vec<MdRec>, DecodeErr> {
    let mut sink = VecSink::default();
    Cb::decode_md(rd(), &mut FeedCtx, class(p), p, &RX, &mut sink)?;
    Ok(sink.md)
}

/// Canonical rendering of the decoded events (fixtures/README "金样文件格式", fields used here).
fn render(recs: &[MdRec]) -> Vec<Value> {
    recs.iter()
        .map(|r| match r {
            MdRec::Bbo(b) => json!({"kind":"Bbo","venue":b.h.venue.0,"inst":b.h.inst.0,
                "ex_ts_ns":b.h.ex_ts_ns,"ex_send_ns":b.h.ex_send_ns,
                "bid_px":b.bid_px.0.to_string(),"bid_qty":b.bid_qty.0.to_string(),
                "ask_px":b.ask_px.0.to_string(),"ask_qty":b.ask_qty.0.to_string(),"update_id":b.update_id}),
            MdRec::Trade(t) => json!({"kind":"Trade","venue":t.h.venue.0,"inst":t.h.inst.0,
                "ex_ts_ns":t.h.ex_ts_ns,"px":t.px.0.to_string(),"qty":t.qty.0.to_string(),
                "trade_id":t.trade_id,"taker_side":if t.taker_side == Side::Buy {"Buy"} else {"Sell"}}),
            other => json!({"kind":format!("{other:?}")}),
        })
        .collect()
}

fn control_json(p: &[u8]) -> Vec<Value> {
    match Cb::control(class(p), p) {
        Some(ControlHint::SubAck { ok, .. }) => vec![json!({"control":"SubAck","ok":ok})],
        Some(ControlHint::Pong) => vec![json!({"control":"Pong"})],
        Some(other) => vec![json!({"control":format!("{other:?}")})],
        None => vec![],
    }
}

const TICKER: &str = r#"{"type":"ticker","sequence":138933168161,"product_id":"BTC-USD","price":"75750.28","open_24h":"75922.02","volume_24h":"13353.57379738","low_24h":"74722","high_24h":"76917.79","volume_30d":"179372.35799304","best_bid":"75750.28","best_bid_size":"0.06203374","best_ask":"75750.29","best_ask_size":"0.23767411","side":"sell","time":"2026-11-06T15:59:00.164231Z","trade_id":1116405816,"last_size":"0.04446873"}"#;
const MATCH: &str = r#"{"type":"match","trade_id":1116405767,"maker_order_id":"31f4cf76-7486-41c7-ae3b-5a1a5f6ff466","taker_order_id":"cb1f121f-c5b8-43fc-828b-a32da921e10d","side":"sell","size":"0.03128785","price":"75732.02","product_id":"BTC-USD","sequence":138933157653,"time":"2026-11-06T15:58:52.630047Z"}"#;

#[test]
fn cbx_001_classify() {
    let rows: [(&str, &[u8], MsgOp, MdClass); 9] = [
        ("CBX-001.1", TICKER.as_bytes(), MsgOp::Text, MdClass::Ticker),
        ("CBX-001.2", MATCH.as_bytes(), MsgOp::Text, MdClass::Match),
        (
            "CBX-001.3",
            b"{\"type\":\"last_match\",\"trade_id\":1",
            MsgOp::Text,
            MdClass::LastMatch,
        ),
        (
            "CBX-001.4",
            b"{\"type\":\"heartbeat\",\"last_trade_id\":1",
            MsgOp::Text,
            MdClass::Heartbeat,
        ),
        (
            "CBX-001.5",
            b"{\"type\":\"subscriptions\",\"channels\":[]}",
            MsgOp::Text,
            MdClass::Subscriptions,
        ),
        (
            "CBX-001.6",
            b"{\"type\":\"error\",\"message\":\"x\"}",
            MsgOp::Text,
            MdClass::Error,
        ),
        (
            "CBX-001.7",
            b"{\"type\":\"l2update\",\"product_id\":\"BTC-USD\"}",
            MsgOp::Text,
            MdClass::Unknown,
        ),
        ("CBX-001.8", TICKER.as_bytes(), MsgOp::Binary, MdClass::Unknown),
        (
            "CBX-001.9",
            b" {\"type\":\"ticker\"}",
            MsgOp::Text,
            MdClass::Unknown,
        ),
    ];
    for (row, p, op, want) in rows {
        for route in [ROUTE_BBO, ROUTE_MD] {
            assert_eq!(Cb::classify(route, op, p.get(..64).unwrap_or(p)), want, "{row}");
        }
    }
}

#[test]
fn cbx_002_routes_and_seq_rules() {
    let r = Cb::routes();
    assert_eq!(r.len(), 2, "CBX-002.1");
    assert_eq!(
        (r[0].id, r[0].kind, r[0].race),
        (ROUTE_BBO, RouteKind::Bbo, true),
        "CBX-002.1"
    );
    assert_eq!(
        (r[1].id, r[1].kind, r[1].race),
        (ROUTE_MD, RouteKind::Md, false),
        "CBX-002.1"
    );
    assert!(r.iter().all(|x| x.topn_levels.is_empty()), "CBX-002.1");
    assert_eq!(Cb::stream_route(StreamKind::Bbo), Some(ROUTE_BBO), "CBX-002.2");
    assert_eq!(Cb::stream_route(StreamKind::Trade), Some(ROUTE_MD), "CBX-002.2");
    for s in [
        StreamKind::DepthTopN,
        StreamKind::DepthIncr,
        StreamKind::Mark,
        StreamKind::Index,
        StreamKind::Funding,
        StreamKind::Liquidation,
    ] {
        assert_eq!(Cb::stream_route(s), None, "CBX-002.2 {s:?}");
    }
    for (c, want) in [
        (MdClass::Ticker, SeqRule::Monotonic),
        (MdClass::Match, SeqRule::Monotonic),
        (MdClass::LastMatch, SeqRule::Monotonic),
        (MdClass::Heartbeat, SeqRule::None),
        (MdClass::Subscriptions, SeqRule::None),
        (MdClass::Error, SeqRule::None),
        (MdClass::Unknown, SeqRule::None),
    ] {
        assert_eq!(Cb::seq_rule(c), want, "CBX-002.3 {c:?}");
    }
    let mut out = RestReqBuf::default();
    assert_eq!(
        Cb::bbo_snapshot_request(rd(), &FeedCtx, &[BTC_USD], &mut out),
        Ok(false),
        "CBX-002.4"
    );
}

#[test]
fn cbx_003_endpoint_and_login() {
    for route in [ROUTE_BBO, ROUTE_MD] {
        let mut e = EndpointSpec::default();
        Cb::endpoint(Role::Md, route, &CFG, None, None, 0, &mut e).unwrap();
        assert_eq!(
            (e.path_query.as_slice(), e.headers.as_slice()),
            (&b"/"[..], &b""[..]),
            "CBX-003.1"
        );
    }
    let mut e = EndpointSpec::default();
    assert_eq!(
        Cb::endpoint(Role::Md, RouteId(3), &CFG, None, None, 0, &mut e),
        Err(ProtoErr::Unsupported),
        "CBX-003.2"
    );
    for role in [Role::Order, Role::UserStream, Role::Rest] {
        assert_eq!(
            Cb::endpoint(role, ROUTE_BBO, &CFG, None, None, 0, &mut e),
            Err(ProtoErr::Unsupported),
            "CBX-003.2"
        );
    }
    let cred = Credential::default();
    let mut v = Vec::new();
    assert_eq!(
        Cb::login(Role::Md, &CFG, &cred, 0, 0, &mut v),
        Ok(LoginStep::NotNeeded),
        "CBX-003.3"
    );
    assert_eq!(
        Cb::login(Role::Order, &CFG, &cred, 0, 0, &mut v),
        Err(ProtoErr::Unsupported),
        "CBX-003.3"
    );
    assert_eq!(
        Cb::login_result(TICKER.as_bytes()),
        LoginResult::NotMine,
        "CBX-003.3"
    );
    assert!(Cb::bootstrap(Role::Md, &CFG, None).is_none(), "CBX-003.3");
}

fn sub(inst: InstId, stream: StreamKind) -> SubSpec {
    SubSpec {
        inst,
        stream,
        levels: 0,
        route: Cb::stream_route(stream).unwrap_or(ROUTE_MD),
    }
}

fn frames(subs: &[SubSpec], unsub: bool) -> Result<Vec<(String, u16)>, ProtoErr> {
    let mut out = SubFrames::with_capacity(4096, 8);
    Cb::subscribe(rd(), subs, unsub, &CFG, &mut out)?;
    Ok((0..out.len())
        .map(|i| {
            let (t, n) = out.get(i).unwrap();
            (String::from_utf8(t.to_vec()).unwrap(), n)
        })
        .collect())
}

#[test]
fn cbx_004_subscribe_frames() {
    assert_eq!(
        frames(&[sub(BTC_USD, StreamKind::Bbo)], false).unwrap(),
        vec![(
            r#"{"type":"subscribe","product_ids":["BTC-USD"],"channels":["ticker","heartbeat"]}"#.to_string(),
            1
        )],
        "CBX-004.1"
    );
    assert_eq!(
        frames(&[sub(BTC_USD, StreamKind::Trade)], false).unwrap(),
        vec![(
            r#"{"type":"subscribe","product_ids":["BTC-USD"],"channels":["matches","heartbeat"]}"#
                .to_string(),
            1
        )],
        "CBX-004.2"
    );
    assert_eq!(
        frames(&[sub(BTC_USD, StreamKind::Bbo)], true).unwrap(),
        vec![(
            r#"{"type":"unsubscribe","product_ids":["BTC-USD"],"channels":["ticker"]}"#.to_string(),
            1
        )],
        "CBX-004.3"
    );
    // A batch stops at the first spec of another stream.
    assert_eq!(
        frames(
            &[sub(BTC_USD, StreamKind::Bbo), sub(BTC_USD, StreamKind::Trade)],
            false
        )
        .unwrap()[0]
            .1,
        1,
        "CBX-004.4"
    );
    assert_eq!(
        frames(&[sub(BTC_USD, StreamKind::Mark)], false),
        Err(ProtoErr::Unsupported),
        "CBX-004.5"
    );
    assert_eq!(
        frames(&[sub(FOREIGN, StreamKind::Bbo)], false),
        Err(ProtoErr::Cfg),
        "CBX-004.6"
    );
    assert_eq!(
        frames(&[sub(InstId(0), StreamKind::Bbo)], false),
        Err(ProtoErr::Cfg),
        "CBX-004.6"
    );
    assert_eq!(frames(&[], false), Err(ProtoErr::Cfg), "CBX-004.7");
    const { assert!(Cb::SUB_ACKS, "CBX-004.8") };
}

#[test]
fn cbx_005_control_hints() {
    let ack = |p: &str| Cb::control(class(p.as_bytes()), p.as_bytes());
    assert_eq!(
        ack(
            r#"{"type":"subscriptions","channels":[{"name":"heartbeat","product_ids":["BTC-USD"],"account_ids":null}]}"#
        ),
        Some(ControlHint::SubAck { ok: true, code: 0 }),
        "CBX-005.1"
    );
    assert_eq!(
        ack(r#"{"type":"subscriptions","channels":[]}"#),
        None,
        "CBX-005.2"
    );
    assert_eq!(
        ack(r#"{"type":"subscriptions","channels": [ ] }"#),
        None,
        "CBX-005.2"
    );
    assert_eq!(
        ack(r#"{"type":"error","message":"Failed to subscribe","reason":"x"}"#),
        Some(ControlHint::SubAck { ok: false, code: 1 }),
        "CBX-005.3"
    );
    assert_eq!(
        ack(
            r#"{"type":"heartbeat","last_trade_id":1,"product_id":"BTC-USD","sequence":2,"time":"2026-11-06T15:58:53.000000Z"}"#
        ),
        Some(ControlHint::Pong),
        "CBX-005.4"
    );
    assert_eq!(ack(TICKER), None, "CBX-005.5");
    assert_eq!(ack(MATCH), None, "CBX-005.5");
}

#[test]
fn cbx_006_peek() {
    let p = |s: &str| Cb::peek(rd(), &FeedCtx, class(s.as_bytes()), s.as_bytes());
    let t = p(TICKER).unwrap();
    assert_eq!(
        (t.inst, t.stream, t.ids.prev_id, t.ids.last_id),
        (BTC_USD, StreamKind::Bbo, 0, 138_933_168_161),
        "CBX-006.1"
    );
    let m = p(MATCH).unwrap();
    assert_eq!(
        (m.inst, m.stream, m.ids.last_id),
        (BTC_USD, StreamKind::Trade, 1_116_405_767),
        "CBX-006.2"
    );
    let lm = MATCH.replace("\"type\":\"match\"", "\"type\":\"last_match\"");
    assert_eq!(p(&lm).unwrap().ids.last_id, 1_116_405_767, "CBX-006.3");
    assert_eq!(
        p(&TICKER.replace("BTC-USD", "ETH-USD")).unwrap().inst,
        InstId(0),
        "CBX-006.4"
    );
    // `peek` does not parse prices.
    assert_eq!(
        p(&TICKER.replace("\"75750.28\"", "\"x\"")).unwrap(),
        t,
        "CBX-006.5"
    );
    // Field order changed: the checked decoder gives the same ids.
    let reordered = r#"{"product_id":"BTC-USD","type":"ticker","sequence":138933168161}"#;
    assert_eq!(
        Cb::peek(rd(), &FeedCtx, MdClass::Ticker, reordered.as_bytes()).unwrap(),
        t,
        "CBX-006.6"
    );
    for c in [
        MdClass::Heartbeat,
        MdClass::Subscriptions,
        MdClass::Error,
        MdClass::Unknown,
    ] {
        assert_eq!(
            Cb::peek(rd(), &FeedCtx, c, TICKER.as_bytes()),
            Err(DecodeErr::UnknownClass),
            "CBX-006.7"
        );
    }
}

#[test]
fn cbx_007_time() {
    let base = 1_793_980_740_000_000_000i64; // 2026-11-06T15:59:00Z
    let ok: [(&str, i64); 6] = [
        ("2026-11-06T15:59:00.164231Z", base + 164_231_000),
        ("2026-11-06T15:59:00Z", base),
        ("2026-11-06T15:59:00.1Z", base + 100_000_000),
        ("2026-11-06T15:59:00.075329692Z", base + 75_329_692),
        ("1970-01-01T00:00:00.000000Z", 0),
        ("2024-02-29T00:00:00Z", 1_709_164_800_000_000_000),
    ];
    for (s, want) in ok {
        assert_eq!(iso8601_ns(s.as_bytes()), Ok(want), "CBX-007.1 {s}");
    }
    for s in [
        "2026-11-06T15:59:00.164231",
        "2026-11-06 15:59:00Z",
        "2026-13-06T15:59:00Z",
        "2023-02-29T00:00:00Z",
        "2026-11-06T24:00:00Z",
        "2026-11-06T15:59:00.Z",
        "2026-11-06T15:59:00.1234567890Z",
        "2026-11-06T15:59:00ZZ",
        "1969-12-31T23:59:59Z",
        "",
    ] {
        assert_eq!(
            iso8601_ns(s.as_bytes()),
            Err(DecodeErr::Number),
            "CBX-007.2 {s:?}"
        );
    }
}

#[test]
fn cbx_008_decode_errors() {
    let rows: [(&str, String, DecodeErr); 6] = [
        (
            "CBX-008.1",
            MATCH.replace("\"side\":\"sell\"", "\"side\":\"x\""),
            DecodeErr::Shape,
        ),
        (
            "CBX-008.2",
            TICKER.replace("\"best_ask\":\"75750.29\"", "\"best_ask\":\"7a\""),
            DecodeErr::Number,
        ),
        (
            "CBX-008.3",
            TICKER.replace(
                "\"best_bid_size\":\"0.06203374\"",
                "\"best_bid_size\":\"0.062033741\"",
            ),
            DecodeErr::Number,
        ),
        (
            "CBX-008.4",
            MATCH.replace("15:58:52.630047Z", "15:58:52"),
            DecodeErr::Number,
        ),
        (
            "CBX-008.5",
            TICKER[..TICKER.len() - 20].to_string(),
            DecodeErr::Truncated,
        ),
        (
            "CBX-008.6",
            TICKER.replace("\"sequence\":138933168161", "\"sequence\":\"1\""),
            DecodeErr::Shape,
        ),
    ];
    for (row, p, want) in rows {
        let mut sink = VecSink::default();
        let r = Cb::decode_md(
            rd(),
            &mut FeedCtx,
            class(p.as_bytes()),
            p.as_bytes(),
            &RX,
            &mut sink,
        );
        assert_eq!(r.map(|o| o.events), Err(want), "{row}");
        assert!(sink.md.is_empty(), "{row}: partial events");
    }
    // Unknown product: no event, no error.
    let other = TICKER.replace("BTC-USD", "ETH-USD");
    assert_eq!(decode(other.as_bytes()), Ok(vec![]), "CBX-008.7");
    for c in [
        MdClass::Heartbeat,
        MdClass::Subscriptions,
        MdClass::Error,
        MdClass::Unknown,
    ] {
        let mut sink = VecSink::default();
        assert_eq!(
            Cb::decode_md(rd(), &mut FeedCtx, c, TICKER.as_bytes(), &RX, &mut sink),
            Err(DecodeErr::UnknownClass),
            "CBX-008.8"
        );
    }
}

#[test]
fn cbx_009_contracts_and_defaults() {
    let caps = Cb::default_caps(|_| None);
    check_default_caps::<Cb>(&caps).unwrap();
    assert_eq!(caps.flags.0, 0, "CBX-009.1");
    assert_eq!(Cb::stale_guard(), StaleGuard::None, "CBX-009.1");
    let samples = [
        MdSample {
            name: "ticker",
            route: ROUTE_BBO,
            op: MsgOp::Text,
            payload: TICKER.as_bytes(),
            rx: RX,
        },
        MdSample {
            name: "match",
            route: ROUTE_MD,
            op: MsgOp::Text,
            payload: MATCH.as_bytes(),
            rx: RX,
        },
    ];
    check_md_basics::<Cb>(&samples, rd(), &mut FeedCtx).unwrap();
    let hb = Cb::heartbeat(Role::Md);
    assert_eq!(
        (
            hb.interval_ms,
            hb.unconditional,
            hb.pong_timeout_ms,
            hb.idle_timeout_ms
        ),
        (5_000, false, 5_000, 0),
        "CBX-009.2"
    );
    assert_eq!(Cb::bbo_probe(), hft_venue_proto::PingKind::WsPing, "CBX-009.2");
    let rules: Vec<_> = Cb::rate_rules()
        .iter()
        .map(|r| (r.scope, r.bucket, r.window_ms, r.limit))
        .collect();
    use hft_refdata::{LimitBucket as B, LimitScope as S};
    assert_eq!(
        rules,
        vec![
            (S::Ip, B::WsInMsgs, 1000, 8),
            (S::Conn, B::WsInMsgs, 1000, 10),
            (S::Ip, B::NewConns, 1000, 8)
        ],
        "CBX-009.3"
    );
}

#[test]
fn cbx_010_golden_ticker() {
    for case in ["normal", "initial_on_subscribe"] {
        let (input, want) = fixture("ticker", case);
        let recs = decode(&input).unwrap();
        assert_eq!(render(&recs), want, "CBX-010 {case}");
        let MdRec::Bbo(Bbo { h, .. }) = &recs[0] else {
            panic!("CBX-010")
        };
        assert_eq!(
            (h.conn, h.rx_tsc, h.sw_rx_ns, h.rx_empty_gap_ns),
            (RX.conn, RX.rx_tsc, RX.sw_rx_ns, 77),
            "CBX-010 {case}"
        );
    }
}

#[test]
fn cbx_011_golden_match() {
    for case in ["taker_buy", "taker_sell"] {
        let (input, want) = fixture("match", case);
        assert_eq!(render(&decode(&input).unwrap()), want, "CBX-011 {case}");
    }
}

#[test]
fn cbx_012_golden_last_match() {
    let (input, want) = fixture("last_match", "on_subscribe");
    assert_eq!(class(&input), MdClass::LastMatch, "CBX-012");
    let mut sink = VecSink::default();
    let o = Cb::decode_md(rd(), &mut FeedCtx, MdClass::LastMatch, &input, &RX, &mut sink).unwrap();
    assert_eq!((o.events, o.inst), (0, BTC_USD), "CBX-012");
    assert_eq!(render(&sink.md), want, "CBX-012");
}

#[test]
fn cbx_013_golden_control() {
    for (mt, case) in [
        ("heartbeat", "normal"),
        ("subscriptions", "ticker_heartbeat"),
        ("subscriptions", "empty_after_error"),
        ("error", "level2_needs_auth"),
        ("error", "unknown_channel"),
    ] {
        let (input, want) = fixture(mt, case);
        assert_eq!(control_json(&input), want, "CBX-013 {mt}/{case}");
        assert_eq!(
            Cb::peek(rd(), &FeedCtx, class(&input), &input),
            Err(DecodeErr::UnknownClass),
            "CBX-013 {mt}/{case}"
        );
    }
}

/// The runtime's `Monotonic` rule (hft-venue §3) over both connections' frames in receive order.
#[test]
fn cbx_014_race_two_connections() {
    let frames = jsonl("race/ticker_two_conns.jsonl");
    let want: Vec<u64> = jsonl("race/ticker_two_conns.expected.jsonl")
        .iter()
        .map(|v| v["update_id"].as_u64().unwrap())
        .collect();
    let mut last_fwd = 0u64;
    let mut out = Vec::new();
    let (mut dups, mut acks, mut pongs) = (0, 0, 0);
    let mut first_by_conn = [0usize; 2];
    for f in &frames {
        let p = f["raw"].as_str().unwrap().as_bytes();
        let c = Cb::classify(ROUTE_BBO, MsgOp::Text, p.get(..64).unwrap_or(p));
        match Cb::control(c, p) {
            Some(ControlHint::SubAck { ok: true, .. }) => {
                acks += 1;
                continue;
            }
            Some(ControlHint::Pong) => {
                pongs += 1;
                continue;
            }
            Some(other) => panic!("CBX-014: {other:?}"),
            None => {}
        }
        let pk = Cb::peek(rd(), &FeedCtx, c, p).unwrap();
        assert_eq!(
            (pk.inst, Cb::seq_rule(c)),
            (BTC_USD, SeqRule::Monotonic),
            "CBX-014"
        );
        if pk.ids.last_id <= last_fwd {
            dups += 1;
            continue;
        }
        last_fwd = pk.ids.last_id;
        first_by_conn[f["conn"].as_u64().unwrap() as usize] += 1;
        let recs = decode(p).unwrap();
        let [MdRec::Bbo(b)] = recs.as_slice() else {
            panic!("CBX-014")
        };
        out.push(b.update_id);
    }
    assert_eq!(out, want, "CBX-014.1 no duplicate, no gap against the union");
    assert_eq!(acks, 2, "CBX-014.2");
    assert!(pongs > 0 && dups > 0, "CBX-014.2");
    assert!(
        first_by_conn.iter().all(|n| *n > 0),
        "CBX-014.3 both connections won some frames: {first_by_conn:?}"
    );
}

#[test]
fn cbx_015_read_only_forms() {
    let cred = Credential::default();
    assert_eq!(
        Cb::build_templates(&CFG, rd(), hft_types::AccountId(1), &cred),
        Err(ProtoErr::Unsupported),
        "CBX-015.1"
    );
    let mut out = SubFrames::with_capacity(256, 2);
    assert_eq!(
        Cb::private_subscribe(&CFG, hft_types::AccountId(1), false, &mut out),
        Err(ProtoErr::Unsupported),
        "CBX-015.2"
    );
    let mut req = RestReqBuf::default();
    assert_eq!(
        Cb::session_identity_request(&CFG, &cred, 0, &mut req),
        Err(ProtoErr::Unsupported),
        "CBX-015.3"
    );
    assert_eq!(
        Cb::account_request(&CFG, &cred, &Query::Balances, 0, &mut req),
        Err(ProtoErr::Unsupported),
        "CBX-015.4"
    );
    assert_eq!(
        Cb::account_decode(
            rd(),
            hft_types::AccountId(1),
            &Query::Balances,
            200,
            b"{}",
            0,
            &mut State::default()
        ),
        Err(DecodeErr::UnknownClass),
        "CBX-015.4"
    );
    assert_eq!(Cb::dead_man(), hft_venue_proto::DeadManSpec::None, "CBX-015.5");
    let mut cid = [0u8; 48];
    assert_eq!(
        Cb::render_cid(
            &hft_types::ClientOrderId {
                epoch: 1,
                venue: hft_types::VenueId::COINBASE_SPOT,
                account: hft_types::AccountId(1),
                slot: 1,
                r#gen: 1
            },
            &mut cid
        ),
        0,
        "CBX-015.6"
    );
    assert_eq!(Cb::parse_cid(b"000000000000000000000000"), None, "CBX-015.6");
    assert_eq!(
        Cb::classify_report(MsgOp::Text, TICKER.as_bytes()),
        venue_coinbase::ReportClass::Unknown,
        "CBX-015.7"
    );
    let mut rs = fuzz_exercise::NullReports;
    assert_eq!(
        Cb::decode_report(
            rd(),
            &mut (),
            venue_coinbase::ReportClass::Unknown,
            TICKER.as_bytes(),
            &RX,
            &mut rs
        ),
        Err(DecodeErr::UnknownClass),
        "CBX-015.7"
    );
}

#[test]
fn cbx_017_matches_run() {
    let mut last = 0u64;
    let mut trades = 0;
    for f in jsonl("race/matches_one_conn.jsonl") {
        let p = f["raw"].as_str().unwrap().as_bytes();
        let c = class(p);
        if Cb::control(c, p).is_some() {
            continue;
        }
        let pk = Cb::peek(rd(), &FeedCtx, c, p).unwrap();
        assert!(pk.ids.last_id > last, "CBX-017.1 trade ids increase");
        last = pk.ids.last_id;
        let recs = decode(p).unwrap();
        if c == MdClass::Match {
            let [MdRec::Trade(t)] = recs.as_slice() else {
                panic!("CBX-017.2")
            };
            assert_eq!(t.trade_id, pk.ids.last_id, "CBX-017.2");
            trades += 1;
        } else {
            assert_eq!((c, recs.len()), (MdClass::LastMatch, 0), "CBX-017.3");
        }
    }
    assert!(trades > 100, "CBX-017.2 {trades}");
}
