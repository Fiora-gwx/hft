//! venue-pmprice tests (TESTS.md). One binary: unit tables, the login frame, subscriptions, the
//! history frame, the 2026-11-09 capture golden and contract checks.
#![allow(clippy::disallowed_types)] // serde_json::Value: golden comparison in test code, not a hot path

use std::sync::OnceLock;

use hft_refdata::{
    AssetClass, AssetSpec, Exchange, FeePair, FeeSpec, InstKind, InstSpec, InstStatus, RateLimit, RdbBuilder,
    RefData, StaleGuardKind, TsUnit, VenueSpec,
};
use hft_types::{AssetExp, AssetId, ConnId, InstId, Notional, Px, Qty, Rate, VenueId};
use hft_venue_proto::testkit::{MdRec, MdSample, VecSink, check_default_caps, check_md_basics};
use hft_venue_proto::{
    ArrayBuf, ControlHint, Credential, DecodeErr, EndpointSpec, LoginResult, LoginStep, MsgOp, ProtoErr,
    Role, RouteId, RouteKind, RxMeta, SecretBuf, SeqRule, SignAlg, StreamKind, SubFrames, SubSpec, VenueCfg,
    VenueEnv, VenueProtocol,
};
use serde_json::{Value, json};
use venue_pmprice::{FeedCtx, MdClass, PolymarketPrice as Pp, ROUTE_PRICE, api_secret, round_dec};

const USD: AssetId = AssetId(1);
const BTC: AssetId = AssetId(2);
/// `btcusd` on `POLYMARKET_PRICE`.
const BTCUSD: InstId = InstId(1);
/// `ethusd` on `POLYMARKET_PRICE` (a second symbol).
const ETHUSD: InstId = InstId(2);
/// `btcusd` on `COINBASE_SPOT` (same symbol, foreign venue).
const FOREIGN: InstId = InstId(3);

const CFG: VenueCfg = VenueCfg {
    env: VenueEnv::Prod,
    stale_guard_ms: 1500,
};

/// `RD-PP`: USD (Stable) and BTC (px −4, qty −8); `POLYMARKET_PRICE` `btcusd` = InstId 1 and
/// `ethusd` = InstId 2 (asset BTC for brevity); `COINBASE_SPOT` `btcusd` = InstId 3.
fn rd() -> &'static RefData {
    static RD: OnceLock<RefData> = OnceLock::new();
    RD.get_or_init(|| RefData::load(&blob(), None).unwrap_or_else(|e| panic!("RD-PP rejected: {e:?}")))
}

fn blob() -> Vec<u8> {
    let mut b = RdbBuilder::new(1);
    b.created_utc_ns(1_794_220_000_000_000_000);
    for (id, name, class, px, qty) in [
        (USD, "USD", AssetClass::Stable, -6, -6),
        (BTC, "BTC", AssetClass::Crypto, -4, -8),
    ] {
        b.asset(AssetSpec {
            id,
            name: name.into(),
            class,
            exp: AssetExp {
                px_exp: px,
                qty_exp: qty,
            },
            px_max: Px(1_000_000_000_000),
            qty_max: Qty(1_000_000_000_000_000),
            dust: Qty(1),
        });
    }
    for (v, exchange) in [
        (VenueId::COINBASE_SPOT, Exchange::Coinbase),
        (VenueId::POLYMARKET_PRICE, Exchange::Polymarket),
    ] {
        b.venue(VenueSpec {
            id: v,
            exchange,
            product: InstKind::Spot,
            ws_max_streams_per_conn: 16,
            ws_max_in_msgs_per_s: 10,
            ws_max_conn_age_s: 86_400,
            ws_new_conn_per_5min_ip: 300,
            app_ping_interval_s: 5,
            ts_unit: TsUnit::Ms,
            client_id_max_len: 36,
            client_id_prefix: "".into(),
            max_batch: 1,
            max_inflight_amend: 1,
            crossed_tolerance_default: 0,
            stale_guard: StaleGuardKind::None,
        });
        b.fee(FeeSpec {
            venue: v,
            tier: 0,
            fee: FeePair {
                maker: Rate(0),
                taker: Rate(0),
            },
        });
    }
    let mut cb = hft_refdata::VenueCaps::none(VenueId::COINBASE_SPOT);
    cb.market = hft_types::MarketKind::Spot;
    b.caps(cb);
    b.caps(Pp::default_caps(|_| None));
    for r in Pp::rate_rules() {
        b.limit(
            VenueId::POLYMARKET_PRICE,
            RateLimit {
                scope: r.scope,
                bucket: r.bucket,
                limit: r.limit,
                window_ms: r.window_ms,
                default_cost: 1,
            },
        );
    }
    for (id, venue, sym) in [
        (BTCUSD, VenueId::POLYMARKET_PRICE, "btcusd"),
        (ETHUSD, VenueId::POLYMARKET_PRICE, "ethusd"),
        (FOREIGN, VenueId::COINBASE_SPOT, "btcusd"),
    ] {
        b.inst(InstSpec {
            id,
            venue,
            symbol: sym.into(),
            status: InstStatus::Trading,
            kind: InstKind::Spot,
            asset: BTC,
            quote: USD,
            tick: 1,
            lot: 1,
            min_qty: Qty(1),
            max_qty: Qty(100_000_000_000),
            min_notional: Notional(1_000_000),
            book_window: 4096,
            tick_str: "0.0001".into(),
            lot_str: "0.00000001".into(),
            min_qty_str: "0.00000001".into(),
            ..InstSpec::default()
        });
    }
    b.finish()
}

const RX: RxMeta = RxMeta {
    rx_tsc: 0x1234_5678,
    hw_rx_ns: 0,
    sw_rx_ns: 1_794_227_706_561_869_263,
    prev_override: 0,
    conn: ConnId(3),
    account: hft_types::AccountId(0),
    _pad: [0; 2],
    ts_flags: 0,
    rx_empty_gap_ns: 77,
    _pad2: 0,
};

fn class(p: &[u8]) -> MdClass {
    Pp::classify(ROUTE_PRICE, MsgOp::Text, p.get(..64).unwrap_or(p))
}

fn decode(ctx: &mut FeedCtx, p: &[u8]) -> Result<Vec<MdRec>, DecodeErr> {
    let mut sink = VecSink::default();
    Pp::decode_md(rd(), ctx, class(p), p, &RX, &mut sink)?;
    Ok(sink.md)
}

fn render(recs: &[MdRec]) -> Vec<Value> {
    recs.iter()
        .map(|r| match r {
            MdRec::Index(e) => json!({"kind":"Index","inst":e.h.inst.0,"ex_ts_ns":e.h.ex_ts_ns,
                "ex_send_ns":e.h.ex_send_ns,"px":e.px.0.to_string()}),
            MdRec::Mark(e) => json!({"kind":"Mark","inst":e.h.inst.0,"ex_ts_ns":e.h.ex_ts_ns,
                "ex_send_ns":e.h.ex_send_ns,"px":e.px.0.to_string()}),
            other => json!({"kind":format!("{other:?}")}),
        })
        .collect()
}

const PRICE: &str = r#"{"v":1,"channel":"price.crypto","seq":2,"ts":1794227705000,"payload":{"full_accuracy_value":"74748.646845073935","source":"chainlink","symbol":"btcusd","timestamp":1794227705000,"value":74748.64684507393}}"#;
const TWAP: &str = r#"{"v":1,"channel":"price.crypto.twap","seq":2,"ts":1794227705000,"payload":{"full_accuracy_value":"74766.623443357578297344","source":"chainlink","symbol":"btcusd","timestamp":1794227705000,"value":74766.62344335757,"window_seconds":60}}"#;
const HIST: &str = r#"{"v":1,"channel":"price.crypto","seq":1,"ts":1794227704000,"snapshot":true,"payload":{"data":[{"full_accuracy_value":"74818.0414435677","timestamp":1794227702000,"value":74818.04144356771},{"full_accuracy_value":"74818.54781853607","timestamp":1794227703000,"value":74818.54781853607},{"full_accuracy_value":"74800.00005","timestamp":1794227704000,"value":74800.00005}],"source":"chainlink","symbol":"btcusd"}}"#;

fn cred(key: &str, secret: &str, pass: &str) -> Credential {
    Credential {
        api_key: ArrayBuf::from_bytes(key.as_bytes()).unwrap(),
        secret: SecretBuf(ArrayBuf::from_bytes(secret.as_bytes()).unwrap()),
        passphrase: ArrayBuf::from_bytes(pass.as_bytes()).unwrap(),
        alg: SignAlg::HmacSha256,
    }
}

#[test]
fn ppx_001_classify() {
    let rows: [(&str, &[u8], MsgOp, MdClass); 9] = [
        ("PPX-001.1", PRICE.as_bytes(), MsgOp::Text, MdClass::Price),
        ("PPX-001.2", TWAP.as_bytes(), MsgOp::Text, MdClass::Twap),
        ("PPX-001.3", HIST.as_bytes(), MsgOp::Text, MdClass::Price),
        (
            "PPX-001.4",
            br#"{"op":"subscribed","channel":"price.crypto","rid":"s","provider":"chainlink"}"#,
            MsgOp::Text,
            MdClass::Subscribed,
        ),
        (
            "PPX-001.5",
            br#"{"op":"error","channel":"price.bogus","rid":"s2","code":"bad_channel"}"#,
            MsgOp::Text,
            MdClass::Error,
        ),
        (
            "PPX-001.6",
            br#"{"op":"pong","rid":"p"}"#,
            MsgOp::Text,
            MdClass::Pong,
        ),
        (
            "PPX-001.7",
            br#"{"op":"authed","rid":"a1"}"#,
            MsgOp::Text,
            MdClass::Authed,
        ),
        ("PPX-001.8", PRICE.as_bytes(), MsgOp::Binary, MdClass::Unknown),
        (
            "PPX-001.9",
            br#"{"v":1,"channel":"price.other"}"#,
            MsgOp::Text,
            MdClass::Unknown,
        ),
    ];
    for (row, p, op, want) in rows {
        assert_eq!(
            Pp::classify(ROUTE_PRICE, op, p.get(..64).unwrap_or(p)),
            want,
            "{row}"
        );
    }
    assert_eq!(Pp::seq_rule(MdClass::Price), SeqRule::Monotonic, "PPX-001.10");
    assert_eq!(Pp::seq_rule(MdClass::Twap), SeqRule::Monotonic, "PPX-001.10");
    assert_eq!(Pp::seq_rule(MdClass::Pong), SeqRule::None, "PPX-001.10");
}

#[test]
fn ppx_002_login_frame() {
    let mut out = Vec::new();
    // A Polymarket account secret file: only the API secret before the first `|` is sent.
    let c = cred("k-1_A", "c2VjcmV0LXZhbHVl=|0xWALLETKEY|0xDEPOSIT", "pass-phrase!");
    let step = Pp::login(Role::Md, &CFG, &c, 0, 0, &mut out).unwrap();
    assert_eq!(step, LoginStep::Frame, "PPX-002.1");
    assert_eq!(
        std::str::from_utf8(&out).unwrap(),
        r#"{"op":"auth","rid":"a1","auth":{"apiKey":"k-1_A","secret":"c2VjcmV0LXZhbHVl=","passphrase":"pass-phrase!"}}"#,
        "PPX-002.1"
    );
    let text = String::from_utf8(out.clone()).unwrap();
    assert!(!text.contains("WALLETKEY") && !text.contains('|'), "PPX-002.2");
    assert_eq!(api_secret(&c), b"c2VjcmV0LXZhbHVl=", "PPX-002.2");
    assert_eq!(api_secret(&cred("k", "plain", "p")), b"plain", "PPX-002.3");
    // Unsafe characters are refused (written unescaped).
    for (row, c) in [
        ("PPX-002.4", cred("k\"", "s", "p")),
        ("PPX-002.4", cred("k", "s\\x", "p")),
        ("PPX-002.4", cred("k", "|rest", "p")),
    ] {
        assert_eq!(
            Pp::login(Role::Md, &CFG, &c, 0, 0, &mut out),
            Err(ProtoErr::Cfg),
            "{row}"
        );
    }
    assert_eq!(
        Pp::login(Role::Order, &CFG, &c, 0, 0, &mut out),
        Err(ProtoErr::Unsupported),
        "PPX-002.5"
    );
    assert_eq!(
        Pp::login(Role::Md, &CFG, &c, 1, 0, &mut out),
        Err(ProtoErr::Cfg),
        "PPX-002.6"
    );
}

#[test]
fn ppx_003_login_result() {
    assert_eq!(
        Pp::login_result(br#"{"op":"authed","rid":"a1"}"#),
        LoginResult::Ok,
        "PPX-003.1"
    );
    assert_eq!(
        Pp::login_result(br#"{"op":"error","rid":"a1","code":"bad_auth"}"#),
        LoginResult::Rejected(1),
        "PPX-003.2"
    );
    assert_eq!(
        Pp::login_result(PRICE.as_bytes()),
        LoginResult::NotMine,
        "PPX-003.3"
    );
}

#[test]
fn ppx_004_subscribe_frames() {
    let spec = |inst, stream| SubSpec {
        inst,
        stream,
        levels: 0,
        route: ROUTE_PRICE,
    };
    let mut out = SubFrames::with_capacity(4096, 16);
    let subs = [spec(BTCUSD, StreamKind::Index), spec(BTCUSD, StreamKind::Mark)];
    Pp::subscribe(rd(), &subs, false, &CFG, &mut out).unwrap();
    let f = out.get(0).unwrap();
    assert_eq!(f.1, 1, "PPX-004.1 one spec per frame");
    assert_eq!(
        std::str::from_utf8(f.0).unwrap(),
        r#"{"op":"subscribe","rid":"s","subscriptions":[{"channel":"price.crypto","filter":{"symbol":"btcusd"}}]}"#,
        "PPX-004.1"
    );
    let mut out = SubFrames::with_capacity(4096, 16);
    Pp::subscribe(rd(), &subs[1..], false, &CFG, &mut out).unwrap();
    assert_eq!(
        std::str::from_utf8(out.get(0).unwrap().0).unwrap(),
        r#"{"op":"subscribe","rid":"s","subscriptions":[{"channel":"price.crypto.twap","filter":{"symbol":"btcusd","window_seconds":60}}]}"#,
        "PPX-004.2"
    );
    let mut out = SubFrames::with_capacity(4096, 16);
    Pp::subscribe(rd(), &subs[..1], true, &CFG, &mut out).unwrap();
    assert!(
        std::str::from_utf8(out.get(0).unwrap().0)
            .unwrap()
            .starts_with(r#"{"op":"unsubscribe""#),
        "PPX-004.3"
    );
    let mut out = SubFrames::with_capacity(4096, 16);
    assert_eq!(
        Pp::subscribe(rd(), &[spec(FOREIGN, StreamKind::Index)], false, &CFG, &mut out),
        Err(ProtoErr::Cfg),
        "PPX-004.4"
    );
    assert_eq!(
        Pp::subscribe(rd(), &[spec(BTCUSD, StreamKind::Bbo)], false, &CFG, &mut out),
        Err(ProtoErr::Unsupported),
        "PPX-004.5"
    );
}

#[test]
fn ppx_005_control_and_ping() {
    let ctl = |p: &[u8]| Pp::control(class(p), p);
    assert_eq!(
        ctl(br#"{"op":"subscribed","channel":"price.crypto","rid":"s","provider":"chainlink"}"#),
        Some(ControlHint::SubAck { ok: true, code: 0 }),
        "PPX-005.1"
    );
    assert_eq!(
        ctl(br#"{"op":"error","channel":"price.bogus","rid":"s","code":"bad_channel"}"#),
        Some(ControlHint::SubAck { ok: false, code: 1 }),
        "PPX-005.2"
    );
    assert_eq!(
        ctl(br#"{"op":"pong","rid":"p"}"#),
        Some(ControlHint::Pong),
        "PPX-005.3"
    );
    assert_eq!(ctl(PRICE.as_bytes()), None, "PPX-005.4");
    let mut buf = [0u8; 64];
    let n = Pp::app_ping(Role::Md, 0, &mut buf);
    assert_eq!(&buf[..n], br#"{"op":"ping","rid":"p"}"#, "PPX-005.5");
    assert_eq!(Pp::app_ping(Role::Md, 0, &mut [0u8; 4]), 0, "PPX-005.5");
    let hb = Pp::heartbeat(Role::Md);
    assert_eq!(
        (hb.interval_ms, hb.pong_timeout_ms, hb.idle_timeout_ms),
        (5_000, 5_000, 10_000),
        "PPX-005.6"
    );
}

#[test]
fn ppx_006_round_dec() {
    let rows: [(&str, &str, i8, Result<i64, DecodeErr>); 9] = [
        ("PPX-006.1", "74748.646845073935", -4, Ok(747_486_468)),
        ("PPX-006.2", "74748.64684", -4, Ok(747_486_468)),
        ("PPX-006.3", "74748.64685", -4, Ok(747_486_469)),
        ("PPX-006.4", "74748", -4, Ok(747_480_000)),
        ("PPX-006.5", "74748.6", -4, Ok(747_486_000)),
        ("PPX-006.6", "74748.99995", -4, Ok(747_490_000)),
        ("PPX-006.7", "-1.0", -4, Err(DecodeErr::Number)),
        ("PPX-006.8", "1e5", -4, Err(DecodeErr::Number)),
        ("PPX-006.9", "99999999999999999999", -4, Err(DecodeErr::Overflow)),
    ];
    for (row, s, e, want) in rows {
        assert_eq!(round_dec(s.as_bytes(), e), want, "{row}");
    }
    assert_eq!(round_dec(b"1.", -4), Err(DecodeErr::Number), "PPX-006.10");
    assert_eq!(round_dec(b"", -4), Err(DecodeErr::Number), "PPX-006.10");
}

#[test]
fn ppx_007_live_samples() {
    let mut ctx = FeedCtx::default();
    let got = render(&decode(&mut ctx, PRICE.as_bytes()).unwrap());
    assert_eq!(
        got,
        vec![
            json!({"kind":"Index","inst":1,"ex_ts_ns":1_794_227_705_000_000_000_i64,
            "ex_send_ns":1_794_227_705_000_000_000_i64,"px":"747486468"})
        ],
        "PPX-007.1"
    );
    let got = render(&decode(&mut ctx, TWAP.as_bytes()).unwrap());
    assert_eq!(
        got,
        vec![
            json!({"kind":"Mark","inst":1,"ex_ts_ns":1_794_227_705_000_000_000_i64,
            "ex_send_ns":1_794_227_705_000_000_000_i64,"px":"747666234"})
        ],
        "PPX-007.2"
    );
    // The same sample again (e.g. after a reconnect): nothing.
    assert!(
        decode(&mut ctx, PRICE.as_bytes()).unwrap().is_empty(),
        "PPX-007.3"
    );
    // A symbol the refdata does not list: validated, nothing emitted.
    let other = PRICE.replace("btcusd", "dogeusd");
    assert!(
        decode(&mut ctx, other.as_bytes()).unwrap().is_empty(),
        "PPX-007.4"
    );
    // Receive metadata copied (contract 3).
    let recs = decode(&mut FeedCtx::default(), PRICE.as_bytes()).unwrap();
    let MdRec::Index(e) = &recs[0] else {
        panic!("PPX-007.5")
    };
    assert_eq!(
        (e.h.venue, e.h.conn, e.h.rx_tsc, e.h.sw_rx_ns, e.h.rx_empty_gap_ns),
        (
            VenueId::POLYMARKET_PRICE,
            RX.conn,
            RX.rx_tsc,
            RX.sw_rx_ns,
            RX.rx_empty_gap_ns
        ),
        "PPX-007.5"
    );
}

#[test]
fn ppx_008_history_frame() {
    let pk = Pp::peek(rd(), &FeedCtx::default(), MdClass::Price, HIST.as_bytes()).unwrap();
    assert_eq!(
        (pk.inst, pk.stream, pk.ids.last_id),
        (BTCUSD, StreamKind::Index, 1_794_227_704_000),
        "PPX-008.1"
    );
    let mut ctx = FeedCtx::default();
    // One live sample at …703 s already forwarded: the history adds only …704 s.
    assert!(
        ctx.advance(BTCUSD, StreamKind::Index, 1_794_227_703_000),
        "PPX-008.2"
    );
    let got = render(&decode(&mut ctx, HIST.as_bytes()).unwrap());
    assert_eq!(
        got,
        vec![
            json!({"kind":"Index","inst":1,"ex_ts_ns":1_794_227_704_000_000_000_i64,
            "ex_send_ns":1_794_227_704_000_000_000_i64,"px":"748000001"})
        ],
        "PPX-008.2"
    );
    let mut ctx = FeedCtx::default();
    assert_eq!(decode(&mut ctx, HIST.as_bytes()).unwrap().len(), 3, "PPX-008.3");
    assert!(decode(&mut ctx, HIST.as_bytes()).unwrap().is_empty(), "PPX-008.3");
    // The index watermark does not hold back the TWAP stream.
    assert_eq!(decode(&mut ctx, TWAP.as_bytes()).unwrap().len(), 1, "PPX-008.4");
    // A bad sample anywhere rejects the whole frame before any event.
    let bad = HIST.replace(
        r#""full_accuracy_value":"74800.00005""#,
        r#""full_accuracy_value":"74800.0x""#,
    );
    let mut ctx = FeedCtx::default();
    assert_eq!(
        decode(&mut ctx, bad.as_bytes()),
        Err(DecodeErr::Number),
        "PPX-008.5"
    );
    assert!(
        ctx.advance(BTCUSD, StreamKind::Index, 1),
        "PPX-008.5 nothing forwarded"
    );
}

#[test]
fn ppx_009_malformed() {
    let mut ctx = FeedCtx::default();
    for k in 0..PRICE.len() {
        let p = &PRICE.as_bytes()[..k];
        let _ = Pp::peek(rd(), &ctx, MdClass::Price, p);
        let mut sink = VecSink::default();
        let r = Pp::decode_md(rd(), &mut ctx, MdClass::Price, p, &RX, &mut sink);
        assert!(r.is_err() && sink.md.is_empty(), "PPX-009.1 prefix {k}");
    }
    let mut sink = VecSink::default();
    assert_eq!(
        Pp::decode_md(rd(), &mut ctx, MdClass::Pong, b"{}", &RX, &mut sink),
        Err(DecodeErr::UnknownClass),
        "PPX-009.2"
    );
    let snap_without_data =
        r#"{"v":1,"channel":"price.crypto","ts":1,"snapshot":true,"payload":{"symbol":"btcusd"}}"#;
    assert_eq!(
        Pp::decode_md(
            rd(),
            &mut ctx,
            MdClass::Price,
            snap_without_data.as_bytes(),
            &RX,
            &mut sink
        ),
        Err(DecodeErr::Shape),
        "PPX-009.3"
    );
}

#[test]
fn ppx_010_capture_golden() {
    let dir =
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/polymarket-price/capture");
    let input = std::fs::read_to_string(dir.join("btcusd_40s.jsonl")).unwrap();
    let want: Vec<Value> = std::fs::read_to_string(dir.join("btcusd_40s.expected.jsonl"))
        .unwrap()
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    let mut ctx = FeedCtx::default();
    let mut got = Vec::new();
    let (mut acks, mut refused) = (0, 0);
    for line in input.lines() {
        let rec: Value = serde_json::from_str(line).unwrap();
        let raw = rec["raw"].as_str().unwrap().as_bytes();
        let c = class(raw);
        match Pp::control(c, raw) {
            Some(ControlHint::SubAck { ok: true, .. }) => acks += 1,
            Some(ControlHint::SubAck { ok: false, .. }) => refused += 1,
            _ => {}
        }
        if matches!(c, MdClass::Price | MdClass::Twap) {
            let mut sink = VecSink::default();
            Pp::decode_md(rd(), &mut ctx, c, raw, &RX, &mut sink).unwrap();
            got.extend(render(&sink.md));
        }
    }
    assert_eq!((acks, refused), (2, 1), "PPX-010.1");
    assert_eq!(got.len(), want.len(), "PPX-010.2");
    for (i, (g, w)) in got.iter().zip(&want).enumerate() {
        assert_eq!(g, w, "PPX-010.2 event {i}");
    }
}

#[test]
fn ppx_011_contracts() {
    check_default_caps::<Pp>(&Pp::default_caps(|_| None)).unwrap();
    let samples = [
        MdSample {
            name: "price",
            route: ROUTE_PRICE,
            op: MsgOp::Text,
            payload: PRICE.as_bytes(),
            rx: RX,
        },
        MdSample {
            name: "twap",
            route: ROUTE_PRICE,
            op: MsgOp::Text,
            payload: TWAP.as_bytes(),
            rx: RX,
        },
    ];
    check_md_basics::<Pp>(&samples, rd(), &mut FeedCtx::default()).unwrap();
}

#[test]
fn ppx_012_routes_and_endpoint() {
    let r = Pp::routes();
    assert_eq!(r.len(), 1, "PPX-012.1");
    assert_eq!(
        (r[0].id, r[0].kind, r[0].race),
        (ROUTE_PRICE, RouteKind::Md, false),
        "PPX-012.1"
    );
    assert_eq!(
        Pp::stream_route(StreamKind::Index),
        Some(ROUTE_PRICE),
        "PPX-012.2"
    );
    assert_eq!(Pp::stream_route(StreamKind::Mark), Some(ROUTE_PRICE), "PPX-012.2");
    assert_eq!(Pp::stream_route(StreamKind::Bbo), None, "PPX-012.2");
    let mut ep = EndpointSpec::default();
    Pp::endpoint(Role::Md, ROUTE_PRICE, &CFG, None, None, 0, &mut ep).unwrap();
    assert_eq!(ep.path_query.as_slice(), b"/ws", "PPX-012.3");
    assert_eq!(
        Pp::endpoint(Role::Order, ROUTE_PRICE, &CFG, None, None, 0, &mut ep),
        Err(ProtoErr::Unsupported),
        "PPX-012.3"
    );
    assert_eq!(
        Pp::endpoint(Role::Md, RouteId(9), &CFG, None, None, 0, &mut ep),
        Err(ProtoErr::Unsupported),
        "PPX-012.3"
    );
}
