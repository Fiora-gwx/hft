//! `coinbase-adv` tests (TESTS.md CBX-019 – CBX-025): dispatch, frames, control hints, the
//! level-2 book → `Bbo`, the two-connection golden capture, contracts and the truncation smoke.
use hft_event::Bbo;
use hft_types::{AccountId, ConnId, InstId, VenueId};
use hft_venue_proto::account::{AccountProtocol, Query, State};
use hft_venue_proto::testkit::{MdRec, MdSample, VecSink, check_default_caps, check_md_basics};
use hft_venue_proto::{
    ControlHint, Credential, DecodeErr, EndpointSpec, LoginStep, MsgOp, PingKind, ProtoErr, RestReqBuf, Role,
    RouteKind, RxMeta, SeqRule, StaleGuard, StreamKind, SubFrames, SubSpec, VenueCfg, VenueEnv,
    VenueProtocol,
};

use crate::support::{BIP, BTC_USD, RX, jsonl_adv, rd};
use venue_coinbase::{ADV_BOOKS, ADV_LEVELS, AdvClass, AdvFeedCtx, CoinbaseAdv as Adv, ROUTE_L2};

const CFG: VenueCfg = VenueCfg {
    env: VenueEnv::Prod,
    stale_guard_ms: 1500,
};

/// A forwarded best quote: `(conn, ex_ts_ns, bid px, bid qty, ask px, ask qty)`.
type Quote = (u8, i64, i64, i64, i64, i64);

fn class(p: &[u8]) -> AdvClass {
    Adv::classify(ROUTE_L2, MsgOp::Text, p.get(..64).unwrap_or(p))
}

fn rx(conn: u8) -> RxMeta {
    RxMeta {
        conn: ConnId(conn),
        ..RX
    }
}

/// Decodes one frame of connection `conn`; the `Bbo`s as `(conn, ex_ts_ns, bid px, bid qty, ask px, ask qty)`.
fn feed(ctx: &mut AdvFeedCtx, conn: u8, p: &str) -> Result<Vec<Quote>, DecodeErr> {
    let mut sink = VecSink::default();
    Adv::decode_md(rd(), ctx, class(p.as_bytes()), p.as_bytes(), &rx(conn), &mut sink)?;
    Ok(sink
        .md
        .iter()
        .map(|r| match r {
            MdRec::Bbo(b) => bbo(b),
            other => panic!("unexpected {other:?}"),
        })
        .collect())
}

fn bbo(b: &Bbo) -> Quote {
    (
        b.h.conn.0,
        b.h.ex_ts_ns,
        b.bid_px.0,
        b.bid_qty.0,
        b.ask_px.0,
        b.ask_qty.0,
    )
}

/// An `l2_data` frame of one event; `levels` = `(side, second.micro of 2026-11-11T06:00:SS, price, quantity)`.
fn l2(kind: &str, product: &str, levels: &[(&str, &str, &str, &str)]) -> String {
    let ups: Vec<String> = levels
        .iter()
        .map(|(side, t, px, q)| {
            format!(
                r#"{{"side":"{side}","event_time":"2026-11-11T06:00:{t}Z","price_level":"{px}","new_quantity":"{q}"}}"#
            )
        })
        .collect();
    format!(
        r#"{{"channel":"l2_data","timestamp":"2026-11-11T06:00:59.5Z","sequence_num":7,"events":[{{"type":"{kind}","product_id":"{product}","updates":[{}]}}]}}"#,
        ups.join(",")
    )
}

/// 2026-11-11T06:00:00Z in ns.
const T0: i64 = 1_794_376_800_000_000_000;
/// Engine units of RD-CB: price × 10⁴; one contract = 0.01 BTC = 10⁶ quantity units.
const PX: i64 = 10_000;
const Q: i64 = 1_000_000;

const SUBS: &str = r#"{"channel":"subscriptions","timestamp":"2026-11-11T06:22:28.357101678Z","sequence_num":1,"events":[{"subscriptions":{"level2":["BIP-20DEC30-CDE"]}}]}"#;
const SUBS_KA: &str = r#"{"channel":"subscriptions","timestamp":"2026-11-11T06:22:29.85803522Z","sequence_num":12,"events":[{"subscriptions":{"heartbeats":["heartbeats"],"level2":["BIP-20DEC30-CDE"]}}]}"#;
const HEARTBEAT: &str = r#"{"channel":"heartbeats","timestamp":"2026-11-11T06:22:29.974052246Z","sequence_num":14,"events":[{"current_time":"2026-11-11 06:22:29.970232672 +0000 UTC m=+107375.254866854","heartbeat_counter":107375}]}"#;
const ERROR: &str = r#"{"type":"error","message":"authentication failure"}"#;
const UPDATE: &str = r#"{"channel":"l2_data","timestamp":"2026-11-11T06:14:55.134598716Z","sequence_num":4,"events":[{"type":"update","product_id":"BIP-20DEC30-CDE","updates":[{"side":"offer","event_time":"2026-11-11T06:14:55.08152Z","price_level":"73770","new_quantity":"432"}]}]}"#;

#[test]
fn cbx_019_adv_classify() {
    let rows: [(&str, &[u8], MsgOp, AdvClass); 8] = [
        ("CBX-019.1", UPDATE.as_bytes(), MsgOp::Text, AdvClass::L2),
        (
            "CBX-019.2",
            HEARTBEAT.as_bytes(),
            MsgOp::Text,
            AdvClass::Heartbeat,
        ),
        ("CBX-019.3", SUBS.as_bytes(), MsgOp::Text, AdvClass::Subscriptions),
        ("CBX-019.4", ERROR.as_bytes(), MsgOp::Text, AdvClass::Error),
        (
            "CBX-019.5",
            br#"{"channel":"ticker","timestamp":"x"}"#,
            MsgOp::Text,
            AdvClass::Unknown,
        ),
        ("CBX-019.6", UPDATE.as_bytes(), MsgOp::Binary, AdvClass::Unknown),
        (
            "CBX-019.7",
            br#"{"timestamp":"x","channel":"l2_data"}"#,
            MsgOp::Text,
            AdvClass::Unknown,
        ),
        ("CBX-019.8", b"", MsgOp::Text, AdvClass::Unknown),
    ];
    for (id, p, op, want) in rows {
        assert_eq!(
            Adv::classify(ROUTE_L2, op, p.get(..64).unwrap_or(p)),
            want,
            "{id}"
        );
    }
}

fn sub(inst: InstId, stream: StreamKind) -> SubSpec {
    SubSpec {
        inst,
        stream,
        levels: 0,
        route: ROUTE_L2,
    }
}

fn frames(subs: &[SubSpec], unsub: bool) -> Result<Vec<(String, u16)>, ProtoErr> {
    let mut out = SubFrames::with_capacity(1024, 4);
    Adv::subscribe(rd(), subs, unsub, &CFG, &mut out)?;
    Ok((0..out.len())
        .map(|i| {
            let (t, n) = out.get(i).unwrap();
            (String::from_utf8(t.to_vec()).unwrap(), n)
        })
        .collect())
}

#[test]
fn cbx_020_adv_route_frames_and_keepalive() {
    // .1 one raced best-quote route; nothing but Bbo is routed; no sequence rule.
    let r = Adv::routes();
    assert_eq!(
        (r.len(), r[0].id, r[0].kind, r[0].race, r[0].name),
        (1, ROUTE_L2, RouteKind::Bbo, true, "level2"),
        "CBX-020.1"
    );
    assert_eq!(Adv::stream_route(StreamKind::Bbo), Some(ROUTE_L2), "CBX-020.1");
    for s in [StreamKind::Trade, StreamKind::DepthTopN, StreamKind::Mark] {
        assert_eq!(Adv::stream_route(s), None, "CBX-020.1 {s:?}");
    }
    for c in [AdvClass::L2, AdvClass::Heartbeat, AdvClass::Subscriptions] {
        assert_eq!(Adv::seq_rule(c), SeqRule::None, "CBX-020.1");
    }
    // .2 endpoint `/` for the Md role only; no login.
    let mut e = EndpointSpec::default();
    assert_eq!(
        Adv::endpoint(Role::Md, ROUTE_L2, &CFG, None, None, 0, &mut e),
        Ok(()),
        "CBX-020.2"
    );
    assert_eq!(e.path_query.as_slice(), b"/", "CBX-020.2");
    assert_eq!(
        Adv::endpoint(Role::Order, ROUTE_L2, &CFG, None, None, 0, &mut e),
        Err(ProtoErr::Unsupported),
        "CBX-020.2"
    );
    assert_eq!(
        Adv::login(Role::Md, &CFG, &Credential::default(), 0, 0, &mut Vec::new()),
        Ok(LoginStep::NotNeeded),
        "CBX-020.2"
    );
    // .3 subscribe / unsubscribe frames; other streams and foreign insts are refused.
    assert_eq!(
        frames(&[sub(BIP, StreamKind::Bbo)], false).unwrap(),
        vec![(
            r#"{"type":"subscribe","product_ids":["BIP-20DEC30-CDE"],"channel":"level2"}"#.to_string(),
            1
        )],
        "CBX-020.3"
    );
    assert_eq!(
        frames(&[sub(BIP, StreamKind::Bbo)], true).unwrap()[0].0,
        r#"{"type":"unsubscribe","product_ids":["BIP-20DEC30-CDE"],"channel":"level2"}"#,
        "CBX-020.3"
    );
    assert_eq!(
        frames(&[sub(BIP, StreamKind::Trade)], false),
        Err(ProtoErr::Unsupported),
        "CBX-020.3"
    );
    assert_eq!(
        frames(&[sub(BTC_USD, StreamKind::Bbo)], false),
        Err(ProtoErr::Cfg),
        "CBX-020.3 an inst of another venue"
    );
    assert_eq!(frames(&[], false), Err(ProtoErr::Cfg), "CBX-020.3");
    // .4 the keep-alive: the BBO probe is an application frame subscribing `heartbeats`.
    assert_eq!(Adv::bbo_probe(), PingKind::AppJson, "CBX-020.4");
    let mut buf = [0u8; 256];
    let n = Adv::app_ping(Role::Md, 0, &mut buf);
    assert_eq!(
        &buf[..n],
        br#"{"type":"subscribe","channel":"heartbeats"}"#,
        "CBX-020.4"
    );
    assert_eq!(Adv::app_ping(Role::Md, 0, &mut [0u8; 8]), 0, "CBX-020.4 no room");
}

#[test]
fn cbx_021_adv_control_hints() {
    let hint = |p: &str| Adv::control(class(p.as_bytes()), p.as_bytes());
    assert_eq!(
        hint(SUBS),
        Some(ControlHint::SubAck { ok: true, code: 0 }),
        "CBX-021.1"
    );
    assert_eq!(
        hint(SUBS_KA),
        Some(ControlHint::SubAck { ok: true, code: 0 }),
        "CBX-021.2 the keep-alive answer"
    );
    assert_eq!(hint(HEARTBEAT), Some(ControlHint::Pong), "CBX-021.3");
    assert_eq!(
        hint(ERROR),
        Some(ControlHint::SubAck { ok: false, code: 1 }),
        "CBX-021.4"
    );
    assert_eq!(hint(UPDATE), None, "CBX-021.5");
    // .6 peek: the product's inst on the Bbo stream; control classes have none.
    let pk = Adv::peek(rd(), &AdvFeedCtx::default(), AdvClass::L2, UPDATE.as_bytes()).unwrap();
    assert_eq!(
        (pk.inst, pk.stream, pk.ids.last_id),
        (BIP, StreamKind::Bbo, 0),
        "CBX-021.6"
    );
    let other = l2("update", "BIT-28NOV26-CDE", &[("bid", "01.0", "1", "1")]);
    assert_eq!(
        Adv::peek(rd(), &AdvFeedCtx::default(), AdvClass::L2, other.as_bytes())
            .unwrap()
            .inst,
        InstId(0),
        "CBX-021.6 unknown product"
    );
    assert_eq!(
        Adv::peek(
            rd(),
            &AdvFeedCtx::default(),
            AdvClass::Heartbeat,
            HEARTBEAT.as_bytes()
        ),
        Err(DecodeErr::UnknownClass),
        "CBX-021.6"
    );
}

const P: &str = "BIP-20DEC30-CDE";

#[test]
fn cbx_022_adv_book_to_bbo() {
    let mut c = AdvFeedCtx::default();
    // .1 an update before any snapshot of the connection is ignored.
    assert_eq!(
        feed(&mut c, 1, &l2("update", P, &[("bid", "01.0", "73750", "5")])).unwrap(),
        vec![],
        "CBX-022.1"
    );
    // .2 the snapshot: one Bbo with the best bid and offer, exchange time = the levels' time.
    let snap = l2(
        "snapshot",
        P,
        &[
            ("bid", "02.000001", "73755", "6"),
            ("bid", "02.000001", "73750", "609"),
            ("bid", "02.000001", "5", "1"),
            ("offer", "02.000001", "73765", "12"),
            ("offer", "02.000001", "73770", "40"),
            ("offer", "02.000001", "100000000", "5"),
        ],
    );
    let t = |us: i64| T0 + us * 1_000;
    assert_eq!(
        feed(&mut c, 1, &snap).unwrap(),
        vec![(1, t(2_000_001), 73_755 * PX, 6 * Q, 73_765 * PX, 12 * Q)],
        "CBX-022.2"
    );
    // .3 a change behind the best quote: no event; a new quantity at the best: one.
    assert_eq!(
        feed(&mut c, 1, &l2("update", P, &[("bid", "03.0", "73750", "700")])).unwrap(),
        vec![],
        "CBX-022.3"
    );
    assert_eq!(
        feed(&mut c, 1, &l2("update", P, &[("offer", "03.5", "73765", "9")])).unwrap(),
        vec![(1, t(3_500_000), 73_755 * PX, 6 * Q, 73_765 * PX, 9 * Q)],
        "CBX-022.3"
    );
    // .4 the best bid deleted: the next level; a better offer inserted; both in one frame → one
    // event with the frame's latest level time.
    assert_eq!(
        feed(
            &mut c,
            1,
            &l2(
                "update",
                P,
                &[("bid", "04.1", "73755", "0"), ("offer", "04.2", "73760", "3")]
            )
        )
        .unwrap(),
        vec![(1, t(4_200_000), 73_750 * PX, 700 * Q, 73_760 * PX, 3 * Q)],
        "CBX-022.4"
    );
    // .5 the second connection: its snapshot shows the state already forwarded → nothing; its
    // replay of an older change → nothing; a change later than the last forwarded one → an event.
    let snap2 = l2(
        "snapshot",
        P,
        &[("bid", "04.2", "73750", "700"), ("offer", "04.2", "73760", "3")],
    );
    assert_eq!(feed(&mut c, 2, &snap2).unwrap(), vec![], "CBX-022.5 same state");
    assert_eq!(
        feed(&mut c, 2, &l2("update", P, &[("offer", "04.2", "73760", "4")])).unwrap(),
        vec![],
        "CBX-022.5 not later than the last forwarded"
    );
    assert_eq!(
        feed(&mut c, 2, &l2("update", P, &[("offer", "04.3", "73760", "5")])).unwrap(),
        vec![(2, t(4_300_000), 73_750 * PX, 700 * Q, 73_760 * PX, 5 * Q)],
        "CBX-022.5 later"
    );
    // .6 the first connection catches up: the same state → nothing; then it leads again.
    assert_eq!(
        feed(&mut c, 1, &l2("update", P, &[("offer", "04.3", "73760", "5")])).unwrap(),
        vec![],
        "CBX-022.6"
    );
    assert_eq!(
        feed(&mut c, 1, &l2("update", P, &[("bid", "04.4", "73755", "2")])).unwrap(),
        vec![(1, t(4_400_000), 73_755 * PX, 2 * Q, 73_760 * PX, 5 * Q)],
        "CBX-022.6"
    );
    // .7 a side emptied or a crossed book: no event until the quote is two-sided again.
    let mut d = AdvFeedCtx::default();
    assert_eq!(
        feed(&mut d, 1, &l2("snapshot", P, &[("bid", "05.0", "100", "1")])).unwrap(),
        vec![],
        "CBX-022.7 one side"
    );
    assert_eq!(
        feed(&mut d, 1, &l2("update", P, &[("offer", "05.1", "100", "1")])).unwrap(),
        vec![],
        "CBX-022.7 crossed"
    );
    assert_eq!(
        feed(
            &mut d,
            1,
            &l2(
                "update",
                P,
                &[("offer", "05.2", "100", "0"), ("offer", "05.2", "105", "2")]
            )
        )
        .unwrap(),
        vec![(1, t(5_200_000), 100 * PX, Q, 105 * PX, 2 * Q)],
        "CBX-022.7"
    );
    // .8 a product without an inst is skipped; a new snapshot replaces the connection's book.
    assert_eq!(
        feed(
            &mut d,
            1,
            &l2("snapshot", "BIT-28NOV26-CDE", &[("bid", "06.0", "1", "1")])
        )
        .unwrap(),
        vec![],
        "CBX-022.8"
    );
    assert_eq!(
        feed(
            &mut d,
            1,
            &l2(
                "snapshot",
                P,
                &[("bid", "06.5", "90", "1"), ("offer", "06.5", "95", "1")]
            )
        )
        .unwrap(),
        vec![(1, t(6_500_000), 90 * PX, Q, 95 * PX, Q)],
        "CBX-022.8"
    );
    // .9 more levels than a side keeps: the best stay exact; when the kept ones are all deleted
    // the side's best is unknown → no event.
    let mut e = AdvFeedCtx::default();
    let mut levels = vec![("bid".to_string(), "50".to_string())];
    for k in 0..(ADV_LEVELS + 5) {
        levels.push(("offer".to_string(), (1_000 + 5 * k).to_string()));
    }
    let rows: Vec<(&str, &str, &str, &str)> = levels
        .iter()
        .map(|(s, p)| (s.as_str(), "07.0", p.as_str(), "1"))
        .collect();
    assert_eq!(
        feed(&mut e, 1, &l2("snapshot", P, &rows)).unwrap(),
        vec![(1, t(7_000_000), 50 * PX, Q, 1_000 * PX, Q)],
        "CBX-022.9"
    );
    let dels: Vec<(&str, &str, &str, &str)> = levels[1..=ADV_LEVELS]
        .iter()
        .map(|(s, p)| (s.as_str(), "07.5", p.as_str(), "0"))
        .collect();
    assert_eq!(
        feed(&mut e, 1, &l2("update", P, &dels)).unwrap(),
        vec![],
        "CBX-022.9 best unknown"
    );
    // .10 more (connection, product) pairs than books: the least recently used book is recycled.
    let mut f = AdvFeedCtx::default();
    let two = [("bid", "08.0", "10", "1"), ("offer", "08.0", "20", "1")];
    for conn in 1..=(ADV_BOOKS as u8 + 1) {
        let _ = feed(&mut f, conn, &l2("snapshot", P, &two)).unwrap();
    }
    assert_eq!(
        feed(&mut f, 1, &l2("update", P, &[("bid", "08.9", "15", "1")])).unwrap(),
        vec![],
        "CBX-022.10 connection 1's book was recycled: its update is ignored"
    );
    assert_eq!(
        feed(
            &mut f,
            ADV_BOOKS as u8 + 1,
            &l2("update", P, &[("bid", "09.0", "15", "1")])
        )
        .unwrap()
        .len(),
        1,
        "CBX-022.10"
    );
}

#[test]
fn cbx_023_adv_decode_errors() {
    let dec = |p: &str| feed(&mut AdvFeedCtx::default(), 1, p).map(|v| v.len());
    let snap = l2(
        "snapshot",
        P,
        &[("bid", "01.0", "10", "1"), ("offer", "01.0", "20", "1")],
    );
    assert_eq!(dec(&snap), Ok(1), "CBX-023.1 reference");
    let rows: [(&str, String, DecodeErr); 9] = [
        (
            "CBX-023.2",
            snap[..snap.len() - 9].to_string(),
            DecodeErr::Truncated,
        ),
        ("CBX-023.3", snap.replace("\"bid\"", "\"buy\""), DecodeErr::Shape),
        ("CBX-023.4", snap.replace("snapshot", "resync"), DecodeErr::Shape),
        (
            "CBX-023.5",
            snap.replace("\"price_level\":\"10\"", "\"price_level\":\"1x\""),
            DecodeErr::Number,
        ),
        (
            "CBX-023.6",
            snap.replace("\"price_level\":\"10\"", "\"price_level\":\"10.00001\""),
            DecodeErr::Number,
        ),
        (
            "CBX-023.7",
            snap.replace("06:00:01.0Z", "06:00:01.0"),
            DecodeErr::Number,
        ),
        (
            "CBX-023.8",
            snap.replace("\"new_quantity\":\"1\"}", "\"new_quantity\":\"1\\u0031\"}"),
            DecodeErr::Shape,
        ),
        (
            "CBX-023.9",
            snap.replace(",\"new_quantity\":\"1\"}]", "}]"),
            DecodeErr::Shape,
        ),
        (
            "CBX-023.10",
            snap.replace("\"events\":[", "\"event\":["),
            DecodeErr::Shape,
        ),
    ];
    for (id, p, want) in rows {
        assert_eq!(dec(&p), Err(want), "{id} {p}");
    }
    // .11 unknown members and another member order are accepted; a control class is not decoded.
    let odd = r#"{"sequence_num":9,"events":[{"updates":[{"new_quantity":"2","x":[1,{"y":"z"}],"price_level":"10","side":"bid","event_time":"2026-11-11T06:00:01Z"},{"side":"offer","event_time":"2026-11-11T06:00:01Z","price_level":"20","new_quantity":"3"}],"product_id":"BIP-20DEC30-CDE","type":"snapshot","n":1.5}],"channel":"l2_data","timestamp":"2026-11-11T06:00:59Z"}"#;
    let mut c = AdvFeedCtx::default();
    let mut sink = VecSink::default();
    Adv::decode_md(rd(), &mut c, AdvClass::L2, odd.as_bytes(), &rx(1), &mut sink).unwrap();
    assert_eq!(sink.md.len(), 1, "CBX-023.11");
    assert_eq!(
        Adv::decode_md(
            rd(),
            &mut c,
            AdvClass::Heartbeat,
            HEARTBEAT.as_bytes(),
            &RX,
            &mut sink
        ),
        Err(DecodeErr::UnknownClass),
        "CBX-023.11"
    );
}

/// The 2026-11-11 capture of two connections in receive order, with the keep-alive frames.
#[test]
fn cbx_024_adv_golden_two_connections() {
    let frames = jsonl_adv("race/level2_two_conns.jsonl");
    let want: Vec<Quote> = jsonl_adv("race/level2_two_conns.expected.jsonl")
        .iter()
        .map(|v| {
            let n = |k: &str| v[k].as_str().unwrap().parse::<i64>().unwrap();
            (
                v["conn"].as_u64().unwrap() as u8 + 1,
                v["ex_ts_ns"].as_i64().unwrap(),
                n("bid_px"),
                n("bid_qty"),
                n("ask_px"),
                n("ask_qty"),
            )
        })
        .collect();
    let mut ctx = AdvFeedCtx::default();
    let (mut got, mut acks, mut pongs, mut ids) = (Vec::new(), 0, 0, Vec::new());
    for f in &frames {
        let p = f["raw"].as_str().unwrap().as_bytes();
        let conn = f["conn"].as_u64().unwrap() as u8 + 1;
        let c = class(p);
        match Adv::control(c, p) {
            Some(ControlHint::SubAck { ok: true, .. }) => {
                acks += 1;
                continue;
            }
            Some(ControlHint::Pong) => {
                pongs += 1;
                continue;
            }
            Some(other) => panic!("CBX-024: {other:?}"),
            None => {}
        }
        assert_eq!(Adv::peek(rd(), &ctx, c, p).unwrap().inst, BIP, "CBX-024");
        let mut sink = VecSink::default();
        Adv::decode_md(rd(), &mut ctx, c, p, &rx(conn), &mut sink).unwrap();
        for r in &sink.md {
            let MdRec::Bbo(b) = r else { panic!("CBX-024") };
            assert_eq!(b.h.venue, VenueId::COINBASE_ADV, "CBX-024");
            ids.push(b.update_id);
            got.push(bbo(b));
        }
    }
    assert_eq!(got.len(), want.len(), "CBX-024.1 event count");
    assert_eq!(
        got, want,
        "CBX-024.1 the events computed independently from the wire"
    );
    assert!(
        ids.windows(2).all(|w| w[0] < w[1]),
        "CBX-024.2 exchange time strictly increases"
    );
    assert!(
        got.iter().all(|e| e.2 < e.4 && e.3 > 0 && e.5 > 0),
        "CBX-024.2 never crossed, never empty"
    );
    let by = |c: u8| got.iter().filter(|e| e.0 == c).count();
    assert!(
        by(1) > 20 && by(2) > 20,
        "CBX-024.3 both connections forward: {} / {}",
        by(1),
        by(2)
    );
    assert!(
        acks >= 40 && pongs >= 40,
        "CBX-024.4 {acks} acks, {pongs} heartbeats"
    );
}

#[test]
fn cbx_025_adv_contracts_defaults_and_truncation() {
    // .1 capability defaults and the md contract checks.
    let caps = Adv::default_caps(|_| None);
    check_default_caps::<Adv>(&caps).unwrap();
    assert_eq!(
        (caps.flags.0, caps.market),
        (0, hft_types::MarketKind::LinearPerp),
        "CBX-025.1"
    );
    assert_eq!(Adv::stale_guard(), StaleGuard::None, "CBX-025.1");
    assert_eq!(
        (Adv::NAME, Adv::VENUE, VenueId::COINBASE_ADV.0),
        ("coinbase-adv", VenueId::COINBASE_ADV, 18),
        "CBX-025.1"
    );
    let snap = l2(
        "snapshot",
        P,
        &[("bid", "01.0", "10", "1"), ("offer", "01.0", "20", "1")],
    );
    let samples = [MdSample {
        name: "l2 snapshot",
        route: ROUTE_L2,
        op: MsgOp::Text,
        payload: snap.as_bytes(),
        rx: RX,
    }];
    check_md_basics::<Adv>(&samples, rd(), &mut AdvFeedCtx::default()).unwrap();
    // .2 read-only: no templates, private stream, identity or account.
    let cred = Credential::default();
    assert_eq!(
        Adv::build_templates(&CFG, rd(), AccountId(1), &cred),
        Err(ProtoErr::Unsupported),
        "CBX-025.2"
    );
    let mut out = SubFrames::with_capacity(256, 2);
    assert_eq!(
        Adv::private_subscribe(&CFG, AccountId(1), false, &mut out),
        Err(ProtoErr::Unsupported),
        "CBX-025.2"
    );
    let mut req = RestReqBuf::default();
    assert_eq!(
        Adv::session_identity_request(&CFG, &cred, 0, &mut req),
        Err(ProtoErr::Unsupported),
        "CBX-025.2"
    );
    assert_eq!(
        Adv::account_request(&CFG, &cred, &Query::Balances, 0, &mut req),
        Err(ProtoErr::Unsupported),
        "CBX-025.2"
    );
    assert_eq!(
        Adv::account_decode(
            rd(),
            AccountId(1),
            &Query::Balances,
            200,
            b"{}",
            0,
            &mut State::default()
        ),
        Err(DecodeErr::UnknownClass),
        "CBX-025.2"
    );
    assert_eq!(Adv::dead_man(), hft_venue_proto::DeadManSpec::None, "CBX-025.2");
    // .3 every prefix of a real update frame and of a snapshot: an error or a result, no panic,
    // and never an event from a cut frame.
    for full in [UPDATE, snap.as_str()] {
        for cut in 0..full.len() {
            let mut ctx = AdvFeedCtx::default();
            let _ = feed(
                &mut ctx,
                1,
                &l2(
                    "snapshot",
                    P,
                    &[("bid", "00.5", "73700", "1"), ("offer", "00.5", "73800", "1")],
                ),
            );
            let p = &full.as_bytes()[..cut];
            let mut sink = VecSink::default();
            let r = Adv::decode_md(rd(), &mut ctx, AdvClass::L2, p, &RX, &mut sink);
            assert!(
                matches!(
                    r,
                    Err(DecodeErr::Truncated | DecodeErr::Shape | DecodeErr::Number)
                ),
                "CBX-025.3 cut {cut}: {r:?}"
            );
            assert!(sink.md.is_empty(), "CBX-025.3 cut {cut}");
        }
    }
}
