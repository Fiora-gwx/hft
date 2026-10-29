//! Order entry, REST answers, the user channel and the account baseline (PMK-023 – PMK-032), the
//! rate defaults (PMK-037).
//! REST answers and user-channel frames are copied from a capture of 2026-10-22 (post-only bids
//! at 0.02, never fillable; `owner` API keys replaced by the test key).

use hft_event::{GwCmd, GwOp, OrderEventKind};
use hft_types::{AccountId, ClientOrderId, InstId, Px, Qty, Side, SlotId, Tif};
use hft_venue_proto::account::{AccountProtocol, Query, State};
use hft_venue_proto::testkit::{ReportRec, VecSink};
use hft_venue_proto::{
    ArrayBuf, Credential, EncodeErr, ExchIdHint, LoginStep, Method, MsgOp, ReportSink, ReqRef, RestOrigin,
    RestReqBuf, RestWhat, Role, SecretBuf, SignAlg, Skip, TargetRef, VenueCfg, VenueEnv, VenueProtocol,
    WallClock, raw_code_http, raw_code_str,
};
use venue_polymarket::sign::{
    CHAIN_ID, EXCHANGE_V2, OrderV2, Wallet, exchange_domain, hex_bytes, l2_hmac, order_digest, parse_u256,
    sign_order_1271,
};
use venue_polymarket::{GwCtx, PolymarketClob as Pm, ReportClass};

use super::{DN, RX, T_DN, T_UP, UP, V, rd};

/// Test-only key of the signing golden (never a real key).
const KEY: &str = "0x1111111111111111111111111111111111111111111111111111111111111111";
const EOA: &str = "0x19E7E376E7C213B7E7e7e46cc70A5dD086DAff2A";
const WALLET: &str = "0x7cB001deBC0984d5a9e796dd5dc35676d97A8cB2";
const API_KEY: &str = "0f5e8d2a-1111-2222-3333-444455556666";
const SECRET: &str = "dGhpc2lzYXRlc3RzZWNyZXRrZXkxMjM0NTY3ODkwYWI=";
const PASS: &str = "test-passphrase";
const NOW_MS: i64 = 1_792_687_605_366;
/// Order hashes of the capture.
const H1: &str = "0x766cf144ea53ad8d1726f1a230dc5e5535d8eb296d114aee313cd83520685f6e";
const H2: &str = "0xd4098b206afb2471eaca3a1b58dc23dbf71f7187eef4d54194046e53e1a04615";

fn cred() -> Credential {
    Credential {
        api_key: ArrayBuf::from_bytes(API_KEY.as_bytes()).unwrap(),
        secret: SecretBuf(ArrayBuf::from_bytes(format!("{SECRET}|{KEY}|{WALLET}").as_bytes()).unwrap()),
        passphrase: ArrayBuf::from_bytes(PASS.as_bytes()).unwrap(),
        alg: SignAlg::Secp256k1,
    }
}

fn cfg() -> VenueCfg {
    VenueCfg {
        env: VenueEnv::Prod,
        stale_guard_ms: 0,
    }
}

fn clk() -> WallClock {
    WallClock {
        unix_ms: NOW_MS,
        unix_us: NOW_MS * 1000,
    }
}

fn ctx() -> GwCtx {
    let mut c = GwCtx::default();
    Pm::bind_order_wire(&mut c, UP, &parse_u256(T_UP).unwrap()).unwrap();
    Pm::bind_order_wire(&mut c, DN, &parse_u256(T_DN).unwrap()).unwrap();
    c
}

fn cmd(op: GwOp, inst: InstId, side: Side, px: i64, qty: i64, tif: Tif) -> GwCmd {
    let mut c = GwCmd::sim_reset(AccountId(1));
    c.op = op;
    c.inst = inst;
    c.side = side;
    c.px = Px(px);
    c.qty = Qty(qty);
    c.tif = tif;
    c.slot = SlotId(7);
    c.r#gen = 3;
    c
}

fn tgt(exch: u64) -> TargetRef {
    TargetRef {
        id: ClientOrderId {
            epoch: 5,
            venue: V,
            account: AccountId(1),
            slot: 7,
            r#gen: 3,
        },
        exch_order_id: exch,
    }
}

/// Encodes on a copy of `ctx` (what a `Place` remembers is dropped; see `encode_mut`).
fn encode(ctx: &GwCtx, c: &GwCmd, t: &TargetRef) -> Result<RestReqBuf, EncodeErr> {
    encode_mut(&mut ctx.clone(), c, t)
}

fn encode_mut(ctx: &mut GwCtx, c: &GwCmd, t: &TargetRef) -> Result<RestReqBuf, EncodeErr> {
    let tpl = Pm::build_templates(&cfg(), rd(), AccountId(1), &cred()).unwrap();
    let mut out = RestReqBuf::default();
    Pm::encode_rest_ctx(rd(), &tpl, ctx, &cred(), c, t, &clk(), &mut out).map(|()| out)
}

fn text(b: &[u8]) -> &str {
    core::str::from_utf8(b).unwrap()
}

fn header<'a>(r: &'a RestReqBuf, name: &str) -> Option<&'a str> {
    text(r.headers.as_slice())
        .split("\r\n")
        .find_map(|l| l.strip_prefix(&format!("{name}: ")[..]))
}

fn key_of(h: &str) -> u64 {
    u64::from_str_radix(&h[2..18], 16).unwrap()
}

fn orders(s: &VecSink) -> Vec<hft_event::OrderEvent> {
    s.reports
        .iter()
        .filter_map(|r| match r {
            ReportRec::Order(e) => Some(*e),
            _ => None,
        })
        .collect()
}

fn origin(op: GwOp, inst: InstId) -> RestOrigin {
    RestOrigin {
        what: RestWhat::Order,
        req: Some(ReqRef {
            slot: 7,
            r#gen: 3,
            op,
        }),
        inst,
    }
}

/// The gw's binding of a REST Place ack (`REST_PLACE_BINDS_ID`): exchange id → slot 7, gen 3.
fn bind(s: &mut VecSink, hash: &str) {
    let _ = s.exch_id(key_of(hash).to_string().as_bytes(), Some(&tgt(0).id));
}

#[test]
fn pmk_023_place_is_a_signed_post_only_gtd_order() {
    let c = cmd(GwOp::Place, UP, Side::Buy, 450, 5_000_000, Tif::PostOnly);
    let r = encode(&ctx(), &c, &tgt(0)).unwrap();
    assert_eq!(
        (r.method, text(r.path_query.as_slice())),
        (Method::Post, "/order"),
        "PMK-023.1"
    );
    assert!(r.redacted, "PMK-023.1");
    let b: serde_json::Value = serde_json::from_slice(r.body.as_slice()).unwrap();
    let o = &b["order"];
    assert_eq!(o["maker"], WALLET, "PMK-023.2");
    assert_eq!(o["signer"], WALLET, "PMK-023.2");
    assert_eq!(o["tokenId"], T_UP, "PMK-023.2");
    assert_eq!(
        (o["makerAmount"].as_str(), o["takerAmount"].as_str()),
        (Some("2250000"), Some("5000000")),
        "PMK-023.2"
    );
    assert_eq!(
        (o["side"].as_str(), o["signatureType"].as_u64()),
        (Some("BUY"), Some(3)),
        "PMK-023.2"
    );
    assert_eq!(
        o["timestamp"].as_str(),
        Some(NOW_MS.to_string().as_str()),
        "PMK-023.2"
    );
    assert_eq!(
        o["expiration"].as_str(),
        Some((NOW_MS / 1000 + 300).to_string().as_str()),
        "PMK-023.3"
    );
    assert_eq!(
        (b["orderType"].as_str(), b["postOnly"].as_bool()),
        (Some("GTD"), Some(true)),
        "PMK-023.3"
    );
    assert_eq!(
        (b["owner"].as_str(), b["deferExec"].as_bool()),
        (Some(API_KEY), Some(false)),
        "PMK-023.3"
    );
    let salt = o["salt"].as_u64().unwrap();
    assert!(salt < 1 << 53 && salt & 0xF_FFFF == 7, "PMK-023.4 {salt}");
    // .5 the signature is the Deposit Wallet signature of exactly these fields.
    let w = Wallet::from_hex(KEY).unwrap();
    let order = OrderV2 {
        salt,
        maker: venue_wallet(),
        signer: venue_wallet(),
        token_id: parse_u256(T_UP).unwrap(),
        maker_amount: 2_250_000,
        taker_amount: 5_000_000,
        side: 0,
        signature_type: 3,
        timestamp_ms: NOW_MS as u64,
        metadata: [0; 32],
        builder: [0; 32],
    };
    let mut sig = [0u8; 512];
    let n = sign_order_1271(
        &w,
        &exchange_domain("2", CHAIN_ID, &EXCHANGE_V2),
        CHAIN_ID,
        &order,
        &mut sig,
    );
    let mut hex = [0u8; 1026];
    assert_eq!(
        o["signature"].as_str(),
        hex_bytes(&sig[..n], &mut hex),
        "PMK-023.5"
    );
    // .6 L2 headers: the wallet, key, passphrase, seconds and the HMAC over this body.
    assert_eq!(header(&r, "POLY_ADDRESS"), Some(EOA), "PMK-023.6");
    assert_eq!(header(&r, "POLY_API_KEY"), Some(API_KEY), "PMK-023.6");
    assert_eq!(header(&r, "POLY_PASSPHRASE"), Some(PASS), "PMK-023.6");
    let ts = (NOW_MS / 1000).to_string();
    assert_eq!(header(&r, "POLY_TIMESTAMP"), Some(ts.as_str()), "PMK-023.6");
    let mut mac = [0u8; 44];
    let want = l2_hmac(SECRET, &ts, "POST", "/order", text(r.body.as_slice()), &mut mac).unwrap();
    assert_eq!(header(&r, "POLY_SIGNATURE"), Some(want), "PMK-023.6");
    assert!(text(r.headers.as_slice()).ends_with("\r\n"), "PMK-023.6");
    assert_eq!(header(&r, "Content-Type"), Some("application/json"), "PMK-023.6");
    // .7 a sell of the Down token swaps the amounts; a GTC order is not post-only.
    let c = cmd(GwOp::Place, DN, Side::Sell, 550, 5_000_000, Tif::Gtc);
    let b: serde_json::Value =
        serde_json::from_slice(encode(&ctx(), &c, &tgt(0)).unwrap().body.as_slice()).unwrap();
    assert_eq!(
        (
            b["order"]["makerAmount"].as_str(),
            b["order"]["takerAmount"].as_str(),
            b["order"]["side"].as_str()
        ),
        (Some("5000000"), Some("2750000"), Some("SELL")),
        "PMK-023.7"
    );
    assert_eq!(b["postOnly"].as_bool(), Some(false), "PMK-023.7");
}

fn venue_wallet() -> [u8; 20] {
    let h = &WALLET[2..];
    core::array::from_fn(|i| u8::from_str_radix(&h[2 * i..2 * i + 2], 16).unwrap())
}

#[test]
fn pmk_024_place_refusals() {
    let ok = cmd(GwOp::Place, UP, Side::Buy, 450, 5_000_000, Tif::PostOnly);
    // .1 an unbound inst (no token id yet).
    assert_eq!(
        encode(&GwCtx::default(), &ok, &tgt(0)).unwrap_err(),
        EncodeErr::UnknownInst,
        "PMK-024.1"
    );
    // .2 IOC / FOK; reduce-only.
    for tif in [Tif::Ioc, Tif::Fok] {
        let c = cmd(GwOp::Place, UP, Side::Buy, 450, 5_000_000, tif);
        assert_eq!(
            encode(&ctx(), &c, &tgt(0)).unwrap_err(),
            EncodeErr::Unsupported(GwOp::Place),
            "PMK-024.2"
        );
    }
    let mut c = ok;
    c.oflags = hft_event::OrderFlags::REDUCE_ONLY.0;
    assert_eq!(
        encode(&ctx(), &c, &tgt(0)).unwrap_err(),
        EncodeErr::Unsupported(GwOp::Place),
        "PMK-024.2"
    );
    // .3 a USDC amount below 1e-6 (0.001 × 0.000001 shares).
    let c = cmd(GwOp::Place, UP, Side::Buy, 1, 1, Tif::PostOnly);
    assert_eq!(
        encode(&ctx(), &c, &tgt(0)).unwrap_err(),
        EncodeErr::FieldOverflow,
        "PMK-024.3"
    );
    // .4 a credential whose secret lacks a part, or whose deposit wallet is not an address.
    let tpl = Pm::build_templates(&cfg(), rd(), AccountId(1), &cred()).unwrap();
    for bad in [format!("{SECRET}|{KEY}"), format!("{SECRET}|{KEY}|0x12")] {
        let mut k = cred();
        k.secret = SecretBuf(ArrayBuf::from_bytes(bad.as_bytes()).unwrap());
        let mut out = RestReqBuf::default();
        assert!(
            Pm::build_templates(&cfg(), rd(), AccountId(1), &k).is_err(),
            "PMK-024.4"
        );
        assert!(
            Pm::encode_rest_ctx(rd(), &tpl, &mut ctx(), &k, &ok, &tgt(0), &clk(), &mut out).is_err(),
            "PMK-024.4"
        );
    }
    // .5 the context-free entry never encodes (token ids live in GwCtx).
    let mut out = RestReqBuf::default();
    assert!(
        Pm::encode_rest_signed(rd(), &tpl, &cred(), &ok, &tgt(0), &clk(), &mut out).is_err(),
        "PMK-024.5"
    );
}

#[test]
fn pmk_025_ack_binds_the_hash_and_cancel_uses_it() {
    let mut c = ctx();
    let mut s = VecSink::default();
    // .1 the capture's POST /order answer: an Ack of slot 7 / gen 3 with the hash's key.
    let body = format!(
        "{{\"errorMsg\":\"\",\"orderID\":\"{H1}\",\"takingAmount\":\"\",\"makingAmount\":\"\",\"status\":\"live\",\"success\":true}}"
    );
    let o = Pm::decode_rest(
        rd(),
        &mut c,
        &origin(GwOp::Place, UP),
        200,
        body.as_bytes(),
        &RX,
        &mut s,
    )
    .unwrap();
    let e = orders(&s);
    assert_eq!((o.events, e.len()), (1, 1), "PMK-025.1");
    assert_eq!(
        (
            e[0].what,
            e[0].slot,
            e[0].r#gen,
            e[0].inst,
            e[0].exch_order_id,
            e[0].epoch
        ),
        (OrderEventKind::Ack, SlotId(7), 3, UP, key_of(H1), 0),
        "PMK-025.1"
    );
    assert_eq!(
        s.id_calls,
        vec![(key_of(H1).to_string().into_bytes(), None)],
        "PMK-025.1"
    );
    // .2 a cancel of that order names its full hash.
    let r = encode(
        &c,
        &cmd(GwOp::Cancel, UP, Side::Buy, 0, 0, Tif::Gtc),
        &tgt(key_of(H1)),
    )
    .unwrap();
    assert_eq!(
        (r.method, text(r.path_query.as_slice())),
        (Method::Delete, "/order"),
        "PMK-025.2"
    );
    assert_eq!(
        text(r.body.as_slice()),
        format!("{{\"orderID\":\"{H1}\"}}"),
        "PMK-025.2"
    );
    assert!(header(&r, "POLY_SIGNATURE").is_some(), "PMK-025.2");
    // .3 an order not acked yet: every order of the account on its token.
    let r = encode(&c, &cmd(GwOp::Cancel, UP, Side::Buy, 0, 0, Tif::Gtc), &tgt(0)).unwrap();
    assert_eq!(
        text(r.path_query.as_slice()),
        "/cancel-market-orders",
        "PMK-025.3"
    );
    assert_eq!(
        text(r.body.as_slice()),
        format!("{{\"asset_id\":\"{T_UP}\"}}"),
        "PMK-025.3"
    );
    // .4 CancelAll of one inst; of the account.
    let r = encode(&c, &cmd(GwOp::CancelAll, DN, Side::Buy, 0, 0, Tif::Gtc), &tgt(0)).unwrap();
    assert_eq!(
        text(r.body.as_slice()),
        format!("{{\"asset_id\":\"{T_DN}\"}}"),
        "PMK-025.4"
    );
    let r = encode(
        &c,
        &cmd(GwOp::CancelAll, InstId(0), Side::Buy, 0, 0, Tif::Gtc),
        &tgt(0),
    )
    .unwrap();
    assert_eq!(
        (text(r.path_query.as_slice()), r.body.is_empty()),
        ("/cancel-all", true),
        "PMK-025.4"
    );
    assert_eq!(header(&r, "Content-Type"), None, "PMK-025.4");
    // .5 Query by hash; unknown hash → UnknownTarget.
    let r = encode(
        &c,
        &cmd(GwOp::Query, UP, Side::Buy, 0, 0, Tif::Gtc),
        &tgt(key_of(H1)),
    )
    .unwrap();
    assert_eq!(
        (r.method, text(r.path_query.as_slice())),
        (Method::Get, format!("/data/order/{H1}").as_str()),
        "PMK-025.5"
    );
    let e = encode(&c, &cmd(GwOp::Query, UP, Side::Buy, 0, 0, Tif::Gtc), &tgt(42)).unwrap_err();
    assert_eq!(e, EncodeErr::UnknownTarget, "PMK-025.5");
}

#[test]
fn pmk_026_rest_refusals_and_unknown_outcomes() {
    let mut c = ctx();
    // .1 a post-only order that would cross: Reject with the message's code.
    let msg = "invalid post-only order: order crosses book";
    let body = format!("{{\"error\":\"{msg}\"}}");
    let mut s = VecSink::default();
    let o = Pm::decode_rest(
        rd(),
        &mut c,
        &origin(GwOp::Place, UP),
        400,
        body.as_bytes(),
        &RX,
        &mut s,
    )
    .unwrap();
    let e = orders(&s);
    assert_eq!(
        (e[0].what, e[0].slot, e[0].reject_code),
        (OrderEventKind::Reject, SlotId(7), raw_code_str(msg.as_bytes())),
        "PMK-026.1"
    );
    assert_eq!(
        (o.raw_code, o.outcome_unknown),
        (raw_code_str(msg.as_bytes()), None),
        "PMK-026.1"
    );
    // .2 `success: false` with a message.
    let mut s = VecSink::default();
    let body = br#"{"errorMsg":"not enough balance / allowance","orderID":"","status":"","success":false}"#;
    Pm::decode_rest(rd(), &mut c, &origin(GwOp::Place, UP), 200, body, &RX, &mut s).unwrap();
    assert_eq!(orders(&s)[0].what, OrderEventKind::Reject, "PMK-026.2");
    // .3 rate limited (429) or the engine restarting (425): Reject + RateLimited.
    for st in [429u16, 425] {
        let mut s = VecSink::default();
        let o = Pm::decode_rest(rd(), &mut c, &origin(GwOp::Place, UP), st, b"", &RX, &mut s).unwrap();
        assert_eq!(orders(&s)[0].reject_code, raw_code_http(st), "PMK-026.3");
        assert_eq!(o.ctl, hft_venue_proto::ReportCtl::RateLimited, "PMK-026.3");
    }
    // .4 5xx and no answer: outcome unknown, no event.
    for st in [500u16, 502, 0] {
        let mut s = VecSink::default();
        let o = Pm::decode_rest(rd(), &mut c, &origin(GwOp::Place, UP), st, b"", &RX, &mut s).unwrap();
        assert!(
            s.reports.is_empty() && o.outcome_unknown.is_some(),
            "PMK-026.4 {st}"
        );
    }
    // .5 a cancel the exchange refused: CancelReject GONE.
    let mut s = VecSink::default();
    let body = format!(
        "{{\"not_canceled\":{{\"{H1}\":\"order can't be found - already canceled or matched\"}},\"canceled\":[]}}"
    );
    Pm::decode_rest(
        rd(),
        &mut c,
        &origin(GwOp::Cancel, UP),
        200,
        body.as_bytes(),
        &RX,
        &mut s,
    )
    .unwrap();
    let e = orders(&s);
    assert_eq!(
        (e[0].what, e[0].detail, e[0].slot),
        (OrderEventKind::CancelReject, 1, SlotId(7)),
        "PMK-026.5"
    );
    // .6 a malformed success body is a decode error.
    let mut s = VecSink::default();
    assert!(
        Pm::decode_rest(
            rd(),
            &mut c,
            &origin(GwOp::Place, UP),
            200,
            b"{\"success\":",
            &RX,
            &mut s
        )
        .is_err(),
        "PMK-026.6"
    );
}

fn order_frame(id: &str, kind: &str, status: &str, matched: &str) -> String {
    format!(
        "{{\"id\":\"{id}\", \"owner\":\"{API_KEY}\", \"market\":\"0xf5fb7c18bcbb23bb2afb83e5fe7f6e78eb2127c12bcd9bb74e3899cce30bbf23\", \"asset_id\":\"{T_UP}\", \"side\":\"BUY\", \"order_owner\":\"{API_KEY}\", \"original_size\":\"5\", \"size_matched\":\"{matched}\", \"price\":\"0.02\", \"associate_trades\":[], \"outcome\":\"Up\", \"type\":\"{kind}\", \"created_at\":\"1792687605\", \"expiration\":\"0\", \"order_type\":\"GTC\", \"status\":\"{status}\", \"maker_address\":\"{WALLET}\", \"timestamp\":\"1792687605408\", \"event_type\":\"order\"}}"
    )
}

fn report(c: &mut GwCtx, s: &mut VecSink, frame: &str) -> hft_venue_proto::ReportOutcome {
    let class = Pm::classify_report(MsgOp::Text, &frame.as_bytes()[..64.min(frame.len())]);
    Pm::decode_report(rd(), c, class, frame.as_bytes(), &RX, s).unwrap()
}

#[test]
fn pmk_027_user_channel_fills_and_cancels() {
    let mut c = ctx();
    let mut s = VecSink::default();
    bind(&mut s, H1);
    // .1 PLACEMENT: nothing (the REST answer is the Ack).
    let o = report(&mut c, &mut s, &order_frame(H1, "PLACEMENT", "LIVE", "0"));
    assert_eq!((orders(&s).len(), o.skip), (0, Skip::Listed), "PMK-027.1");
    // .2 UPDATE 2 of 5 → PartialFill 2; UPDATE 5 → Fill 3 (cum 5), at the order price.
    report(&mut c, &mut s, &order_frame(H1, "UPDATE", "LIVE", "2"));
    report(&mut c, &mut s, &order_frame(H1, "UPDATE", "MATCHED", "5"));
    let e = orders(&s);
    let got: Vec<_> = e
        .iter()
        .map(|e| (e.what, e.qty, e.cum_filled, e.px, e.avg_px, e.detail, e.slot))
        .collect();
    assert_eq!(
        got,
        vec![
            (
                OrderEventKind::PartialFill,
                Qty(2_000_000),
                Qty(2_000_000),
                Px(20),
                Px(20),
                Side::Buy as u8,
                SlotId(7)
            ),
            (
                OrderEventKind::Fill,
                Qty(3_000_000),
                Qty(5_000_000),
                Px(20),
                Px(20),
                Side::Buy as u8,
                SlotId(7)
            ),
        ],
        "PMK-027.2"
    );
    assert!(
        e.iter().all(|e| e.exch_order_id == key_of(H1)
            && e.inst == UP
            && e.epoch == 5
            && e.ex_ts_ns == 1_792_687_605_408_000_000),
        "PMK-027.2"
    );
    // .3 a repeated UPDATE (second connection, replay) adds nothing.
    let n = s.reports.len();
    report(&mut c, &mut s, &order_frame(H1, "UPDATE", "MATCHED", "5"));
    assert_eq!(s.reports.len(), n, "PMK-027.3");
    // .4 CANCELLATION of a live order → one CancelAck; the REST answer naming it adds none.
    bind(&mut s, H2);
    report(&mut c, &mut s, &order_frame(H2, "CANCELLATION", "CANCELED", "0"));
    let body = format!("{{\"not_canceled\":{{}},\"canceled\":[\"{H2}\"]}}");
    Pm::decode_rest(
        rd(),
        &mut c,
        &origin(GwOp::Cancel, UP),
        200,
        body.as_bytes(),
        &RX,
        &mut s,
    )
    .unwrap();
    let cancels: Vec<_> = orders(&s)
        .into_iter()
        .filter(|e| e.what == OrderEventKind::CancelAck)
        .collect();
    assert_eq!(cancels.len(), 1, "PMK-027.4");
    assert_eq!(
        (cancels[0].slot, cancels[0].exch_order_id, cancels[0].inst),
        (SlotId(7), key_of(H2), UP),
        "PMK-027.4"
    );
    // .5 a fill of an order the runtime has not bound yet: UNRESOLVED (held by the gw).
    let h3 = "0x189601a886d3182e6abc6afee63361744721fa72ef31ac48a96b88d5c88f214d";
    let mut s2 = VecSink::default();
    report(&mut c, &mut s2, &order_frame(h3, "UPDATE", "LIVE", "1"));
    let e = orders(&s2);
    assert_eq!(
        (e[0].what, e[0].slot, e[0].qty),
        (OrderEventKind::PartialFill, SlotId::UNRESOLVED, Qty(1_000_000)),
        "PMK-027.5"
    );
    // .6 a token no slot is bound to, `trade` events, PONG: no events.
    let mut s3 = VecSink::default();
    let other = order_frame(H1, "UPDATE", "LIVE", "1").replace(T_UP, "123");
    report(&mut c, &mut s3, &other);
    let o = report(
        &mut c,
        &mut s3,
        r#"{"event_type":"trade","type":"TRADE","id":"t1","status":"MATCHED"}"#,
    );
    assert_eq!(o.skip, Skip::Listed, "PMK-027.6");
    assert_eq!(
        Pm::classify_report(MsgOp::Text, b"PONG"),
        ReportClass::Ignorable,
        "PMK-027.6"
    );
    assert!(s3.reports.is_empty(), "PMK-027.6");
    // .7 an unknown event type: skip Unknown, its name for the note.
    let f = r#"{"event_type":"market_resolved","x":1}"#;
    assert_eq!(report(&mut c, &mut s3, f).skip, Skip::Unknown, "PMK-027.7");
    assert_eq!(
        Pm::report_event_name(f.as_bytes()),
        b"market_resolved",
        "PMK-027.7"
    );
    // .8 an array frame of two events; a malformed number is a decode error.
    let mut s4 = VecSink::default();
    let h4 = "0xdd8473af003833b1b6fc568247e146ae153586a3e06e16bd0f730dce7a748c19";
    bind(&mut s4, h4);
    let arr = format!(
        "[{},{}]",
        order_frame(h4, "UPDATE", "LIVE", "1"),
        order_frame(h4, "UPDATE", "LIVE", "4")
    );
    report(&mut c, &mut s4, &arr);
    let q: Vec<_> = orders(&s4).iter().map(|e| e.qty.0).collect();
    assert_eq!(q, vec![1_000_000, 3_000_000], "PMK-027.8");
    let bad = order_frame(h4, "UPDATE", "LIVE", "x");
    let class = Pm::classify_report(MsgOp::Text, &bad.as_bytes()[..64]);
    assert!(
        Pm::decode_report(rd(), &mut c, class, bad.as_bytes(), &RX, &mut s4).is_err(),
        "PMK-027.8"
    );
    assert!(Pm::is_private_report(ReportClass::Push), "PMK-027.8");
}

#[test]
fn pmk_028_query_result() {
    let mut c = ctx();
    let body = format!(
        "{{\"id\":\"{H1}\",\"status\":\"LIVE\",\"asset_id\":\"{T_UP}\",\"original_size\":\"5\",\"size_matched\":\"2\",\"price\":\"0.45\",\"side\":\"BUY\"}}"
    );
    let mut s = VecSink::default();
    Pm::decode_rest(
        rd(),
        &mut c,
        &origin(GwOp::Query, UP),
        200,
        body.as_bytes(),
        &RX,
        &mut s,
    )
    .unwrap();
    let e = orders(&s);
    assert_eq!(
        (
            e[0].what,
            e[0].detail,
            e[0].cum_filled,
            e[0].avg_px,
            e[0].exch_order_id
        ),
        (
            OrderEventKind::QueryResult,
            2,
            Qty(2_000_000),
            Px(450),
            key_of(H1)
        ),
        "PMK-028.1"
    );
    for (st, want) in [("MATCHED", 3u8), ("CANCELED", 4)] {
        let mut s = VecSink::default();
        let b = body.replace("LIVE", st);
        Pm::decode_rest(
            rd(),
            &mut c,
            &origin(GwOp::Query, UP),
            200,
            b.as_bytes(),
            &RX,
            &mut s,
        )
        .unwrap();
        assert_eq!(orders(&s)[0].detail, want, "PMK-028.2 {st}");
    }
    for (st, b) in [(200u16, &b"null"[..]), (404, b"{\"error\":\"not found\"}")] {
        let mut s = VecSink::default();
        Pm::decode_rest(rd(), &mut c, &origin(GwOp::Query, UP), st, b, &RX, &mut s).unwrap();
        assert_eq!(orders(&s)[0].detail, 7, "PMK-028.3 {st}");
    }
}

#[test]
fn pmk_029_user_channel_session() {
    // .1 the order connection is the user channel, authenticated by its (unanswered) login frame.
    let mut spec = hft_venue_proto::EndpointSpec::default();
    Pm::endpoint(
        Role::Order,
        hft_venue_proto::RouteId(0),
        &cfg(),
        Some(&cred()),
        None,
        0,
        &mut spec,
    )
    .unwrap();
    assert_eq!(text(spec.path_query.as_slice()), "/ws/user", "PMK-029.1");
    let mut out = Vec::new();
    assert_eq!(
        Pm::login(Role::Order, &cfg(), &cred(), 0, 0, &mut out),
        Ok(LoginStep::Sent),
        "PMK-029.1"
    );
    let f: serde_json::Value = serde_json::from_slice(&out).unwrap();
    assert_eq!(
        f,
        serde_json::json!({"auth": {"apiKey": API_KEY, "secret": SECRET, "passphrase": PASS}, "type": "user"}),
        "PMK-029.1"
    );
    assert_eq!(
        Pm::login(Role::Md, &cfg(), &cred(), 0, 0, &mut out),
        Ok(LoginStep::NotNeeded),
        "PMK-029.2"
    );
    let mut frames = hft_venue_proto::SubFrames::with_capacity(1024, 4);
    Pm::private_subscribe(&cfg(), AccountId(1), false, &mut frames).unwrap();
    assert_eq!(frames.len(), 0, "PMK-029.2");
    // .4 user-channel liveness: no pong matching, 30 s without any frame.
    let hb = Pm::heartbeat(Role::Order);
    assert_eq!(
        (hb.interval_ms, hb.pong_timeout_ms, hb.idle_timeout_ms),
        (10_000, 0, 30_000),
        "PMK-029.4"
    );
    // .3 a passphrase with a quote cannot be put into the frame.
    let mut k = cred();
    k.passphrase = ArrayBuf::from_bytes(b"a\"b").unwrap();
    assert!(
        Pm::login(Role::Order, &cfg(), &k, 0, 0, &mut out).is_err(),
        "PMK-029.3"
    );
}

fn acct(q: &Query, status: u16, body: &str, s: &mut State) -> Result<(), hft_venue_proto::DecodeErr> {
    Pm::account_decode(rd(), AccountId(1), q, status, body.as_bytes(), NOW_MS, s)
}

#[test]
fn pmk_030_account_baseline() {
    // .1 requests: paths and the L2 headers (Instruments is public).
    let mut r = RestReqBuf::default();
    for (q, path) in [
        (Query::Instruments, "/time"),
        (Query::Settings, "/auth/api-keys"),
        (Query::Permissions, "/auth/ban-status/closed-only"),
        (
            Query::Balances,
            "/balance-allowance?asset_type=COLLATERAL&signature_type=3",
        ),
        (
            Query::OpenOrders {
                cursor: String::new(),
            },
            "/data/orders?next_cursor=MA==",
        ),
    ] {
        Pm::account_request(&cfg(), &cred(), &q, NOW_MS, &mut r).unwrap();
        assert_eq!(text(r.path_query.as_slice()), path, "PMK-030.1");
        assert_eq!(
            header(&r, "POLY_API_KEY").is_some(),
            q != Query::Instruments,
            "PMK-030.1 {path}"
        );
        // The account client writes Host, Content-Type and the blank line itself.
        let h = text(r.headers.as_slice());
        assert!(
            h.is_empty()
                || h.ends_with(
                    "
"
                ),
            "PMK-030.1 {path}"
        );
        assert_eq!(header(&r, "Content-Type"), None, "PMK-030.1 {path}");
    }
    let q = Query::Cancel {
        symbol: T_UP.into(),
        order_id: H1.into(),
    };
    Pm::account_request(&cfg(), &cred(), &q, NOW_MS, &mut r).unwrap();
    assert_eq!(
        (r.method, text(r.body.as_slice())),
        (Method::Delete, format!("{{\"orderID\":\"{H1}\"}}").as_str()),
        "PMK-030.1"
    );
    // .2 the configured UID (24 characters) prefixes one of the wallet's API keys.
    let mut s = State::default();
    acct(&Query::Instruments, 200, "1792689496", &mut s).unwrap();
    let keys = format!("{{\"apiKeys\":[\"{API_KEY}\"]}}");
    acct(&Query::Settings, 200, &keys, &mut s).unwrap();
    let h = s.config.unwrap();
    assert_eq!(
        (&h.venue_uid[..24], h.uid_keys),
        (&API_KEY.as_bytes()[..24], 3),
        "PMK-030.2"
    );
    let other = "{\"apiKeys\":[\"aaaaaaaa-1111-2222-3333-444455556666\"]}";
    assert!(
        acct(&Query::Settings, 200, other, &mut State::default()).is_err(),
        "PMK-030.2"
    );
    // .3 closed-only mode refuses the account.
    acct(&Query::Permissions, 200, "{\"closed_only\":false}", &mut s).unwrap();
    assert!(
        acct(&Query::Permissions, 200, "{\"closed_only\":true}", &mut s).is_err(),
        "PMK-030.3"
    );
    // .4 open orders: one page, then the end cursor.
    let q = Query::OpenOrders {
        cursor: String::new(),
    };
    let page = format!(
        "{{\"data\":[{{\"id\":\"{H1}\",\"asset_id\":\"{T_UP}\"}}],\"next_cursor\":\"MTAw\",\"limit\":100,\"count\":1}}"
    );
    acct(&q, 200, &page, &mut s).unwrap();
    assert_eq!((s.orders.len(), s.cursor.as_str()), (1, "MTAw"), "PMK-030.4");
    acct(
        &q,
        200,
        "{\"data\":[],\"next_cursor\":\"LTE=\",\"limit\":5000,\"count\":0}",
        &mut s,
    )
    .unwrap();
    assert_eq!((s.orders.len(), s.cursor.as_str()), (0, ""), "PMK-030.4");
    // .5 the pUSD balance (1e-6 units) as the quote asset's balance.
    acct(
        &Query::Balances,
        200,
        "{\"balance\":\"82254260\",\"allowances\":{}}",
        &mut s,
    )
    .unwrap();
    let b: Vec<_> = s.balances.iter().map(|b| (b.asset, b.free, b.locked)).collect();
    assert_eq!(
        b,
        vec![(hft_types::AssetId(1), Qty(82_254_260), Qty(0))],
        "PMK-030.5"
    );
    // .6 any non-2xx answer fails the baseline.
    assert!(acct(&Query::Balances, 401, "{}", &mut s).is_err(), "PMK-030.6");
    let _ = ExchIdHint::Unknown;
}

/// Events of `s` as (kind, slot, gen, epoch, qty, cum, avg_px, detail, exch_order_id).
#[allow(clippy::type_complexity)]
fn rows(s: &VecSink) -> Vec<(OrderEventKind, SlotId, u32, u16, i64, i64, i64, u8, u64)> {
    orders(s)
        .iter()
        .map(|e| {
            (
                e.what,
                e.slot,
                e.r#gen,
                e.epoch,
                e.qty.0,
                e.cum_filled.0,
                e.avg_px.0,
                e.detail,
                e.exch_order_id,
            )
        })
        .collect()
}

const FILL: OrderEventKind = OrderEventKind::Fill;
const PARTIAL: OrderEventKind = OrderEventKind::PartialFill;
const QUERY: OrderEventKind = OrderEventKind::QueryResult;
const ACK: OrderEventKind = OrderEventKind::Ack;
const BUY: u8 = Side::Buy as u8;

fn query_body(id: &str, token: &str, status: &str, matched: &str) -> String {
    format!(
        "{{\"id\":\"{id}\",\"status\":\"{status}\",\"asset_id\":\"{token}\",\"original_size\":\"5\",\"size_matched\":\"{matched}\",\"price\":\"0.42\",\"side\":\"BUY\"}}"
    )
}

fn query(c: &mut GwCtx, s: &mut VecSink, inst: InstId, body: &str) -> hft_venue_proto::ReportOutcome {
    Pm::decode_rest(rd(), c, &origin(GwOp::Query, inst), 200, body.as_bytes(), &RX, s).unwrap()
}

#[test]
fn pmk_031_complete_below_size_ends_the_order() {
    let k1 = key_of(H1);
    let s7 = SlotId(7);
    // .1 MATCHED at 4.99737 of 5 (live journal 2026-10-23): Fill, then QueryResult{Filled}.
    let mut c = ctx();
    let mut s = VecSink::default();
    bind(&mut s, H1);
    let o = report(&mut c, &mut s, &order_frame(H1, "UPDATE", "MATCHED", "4.99737"));
    assert_eq!(o.events, 2, "PMK-031.1");
    assert_eq!(
        rows(&s),
        vec![
            (FILL, s7, 3, 5, 4_997_370, 4_997_370, 20, BUY, k1),
            (QUERY, s7, 3, 5, 0, 4_997_370, 20, 3, k1),
        ],
        "PMK-031.1"
    );
    // .2 the same frame again (second connection, replay): nothing.
    let n = s.reports.len();
    report(&mut c, &mut s, &order_frame(H1, "UPDATE", "MATCHED", "4.99737"));
    assert_eq!(s.reports.len(), n, "PMK-031.2");
    // .3 btc-dn-0 (lot 0.01): LIVE at 4.99116 leaves 0.00884, less than one lot: complete.
    let dn = |h: &str, st: &str, m: &str| order_frame(h, "UPDATE", st, m).replace(T_UP, T_DN);
    let h4 = "0xdd8473af003833b1b6fc568247e146ae153586a3e06e16bd0f730dce7a748c19";
    let mut s3 = VecSink::default();
    bind(&mut s3, h4);
    report(&mut c, &mut s3, &dn(h4, "LIVE", "4.99116"));
    assert_eq!(
        rows(&s3),
        vec![
            (FILL, s7, 3, 5, 4_991_160, 4_991_160, 20, BUY, key_of(h4)),
            (QUERY, s7, 3, 5, 0, 4_991_160, 20, 3, key_of(h4)),
        ],
        "PMK-031.3"
    );
    // .4 LIVE at 4.99 leaves exactly one lot: a partial fill only.
    let h5 = "0x189601a886d3182e6abc6afee63361744721fa72ef31ac48a96b88d5c88f214d";
    let mut s4 = VecSink::default();
    bind(&mut s4, h5);
    report(&mut c, &mut s4, &dn(h5, "LIVE", "4.99"));
    assert_eq!(
        rows(&s4),
        vec![(PARTIAL, s7, 3, 5, 4_990_000, 4_990_000, 20, BUY, key_of(h5))],
        "PMK-031.4"
    );
    // .5 completed while unresolved (no Ack yet, not signed here): Fill UNRESOLVED only; the
    // REST Place answer then carries Ack + QueryResult{Filled} of the requesting slot.
    let mut c = ctx();
    let mut s5 = VecSink::default();
    report(&mut c, &mut s5, &order_frame(H2, "UPDATE", "MATCHED", "4.99737"));
    let k2 = key_of(H2);
    assert_eq!(
        rows(&s5),
        vec![(FILL, SlotId::UNRESOLVED, 0, 0, 4_997_370, 4_997_370, 20, BUY, k2)],
        "PMK-031.5"
    );
    let mut s5 = VecSink::default();
    let body = format!("{{\"errorMsg\":\"\",\"orderID\":\"{H2}\",\"status\":\"matched\",\"success\":true}}");
    let o = Pm::decode_rest(
        rd(),
        &mut c,
        &origin(GwOp::Place, UP),
        200,
        body.as_bytes(),
        &RX,
        &mut s5,
    )
    .unwrap();
    assert_eq!(o.events, 2, "PMK-031.5");
    assert_eq!(
        rows(&s5),
        vec![
            (ACK, s7, 3, 0, 0, 0, 0, 0, k2),
            (QUERY, s7, 3, 0, 0, 4_997_370, 20, 3, k2),
        ],
        "PMK-031.5"
    );
    // .6 queries: MATCHED 4.99737 (btc-up-0) and LIVE 4.99116 (btc-dn-0) → Filled; LIVE 4.99
    // (btc-dn-0) → PartiallyFilled.
    for (inst, token, st, m, want, row) in [
        (UP, T_UP, "MATCHED", "4.99737", 3u8, "PMK-031.6 matched"),
        (DN, T_DN, "LIVE", "4.99116", 3, "PMK-031.6 dust"),
        (DN, T_DN, "LIVE", "4.99", 2, "PMK-031.6 lot"),
    ] {
        let mut s = VecSink::default();
        query(&mut c, &mut s, inst, &query_body(H1, token, st, m));
        let e = orders(&s);
        assert_eq!((e.len(), e[0].what, e[0].detail), (1, QUERY, want), "{row}");
    }
}

#[test]
fn pmk_032_unacked_place_is_resolved_by_its_signed_hash() {
    // .1 Place Up 0.42 × 5 (slot 7, gen 3, epoch 5), no REST answer: Query by the signed hash.
    let mut c = ctx();
    let place = cmd(GwOp::Place, UP, Side::Buy, 420, 5_000_000, Tif::PostOnly);
    let r = encode_mut(&mut c, &place, &tgt(0)).unwrap();
    let b: serde_json::Value = serde_json::from_slice(r.body.as_slice()).unwrap();
    let order = OrderV2 {
        salt: b["order"]["salt"].as_u64().unwrap(),
        maker: venue_wallet(),
        signer: venue_wallet(),
        token_id: parse_u256(T_UP).unwrap(),
        maker_amount: 2_100_000,
        taker_amount: 5_000_000,
        side: 0,
        signature_type: 3,
        timestamp_ms: NOW_MS as u64,
        metadata: [0; 32],
        builder: [0; 32],
    };
    let hash = order_digest(&exchange_domain("2", CHAIN_ID, &EXCHANGE_V2), &order);
    let mut hex = [0u8; 66];
    let h = hex_bytes(&hash, &mut hex).unwrap().to_string();
    let k = key_of(&h);
    let q = cmd(GwOp::Query, UP, Side::Buy, 0, 0, Tif::Gtc);
    let r = encode(&c, &q, &tgt(0)).unwrap();
    assert_eq!(
        (r.method, text(r.path_query.as_slice())),
        (Method::Get, format!("/data/order/{h}").as_str()),
        "PMK-032.1"
    );
    let mut other = tgt(0);
    other.id.r#gen = 4;
    assert_eq!(
        encode(&c, &q, &other).unwrap_err(),
        EncodeErr::UnknownTarget,
        "PMK-032.1"
    );
    other = tgt(0);
    other.id.epoch = 6;
    assert_eq!(
        encode(&c, &q, &other).unwrap_err(),
        EncodeErr::UnknownTarget,
        "PMK-032.1"
    );
    // .2 the user channel reports it MATCHED at 4.99737 before any Ack: bound to the signed
    // client id, Fill + QueryResult{Filled} of slot 7.
    let placed = c.clone();
    let mut s = VecSink::default();
    report(&mut c, &mut s, &order_frame(&h, "UPDATE", "MATCHED", "4.99737"));
    assert_eq!(
        rows(&s),
        vec![
            (FILL, SlotId(7), 3, 5, 4_997_370, 4_997_370, 20, BUY, k),
            (QUERY, SlotId(7), 3, 5, 0, 4_997_370, 20, 3, k),
        ],
        "PMK-032.2"
    );
    assert!(
        s.id_calls
            .contains(&(k.to_string().into_bytes(), Some(tgt(0).id))),
        "PMK-032.2"
    );
    // .3 the query answer of the unacked order: Ack (exch id) + QueryResult{Filled, 4.99737}.
    let mut c = placed;
    let mut s = VecSink::default();
    let body = query_body(&h, T_UP, "MATCHED", "4.99737");
    let o = query(&mut c, &mut s, UP, &body);
    assert_eq!(o.events, 2, "PMK-032.3");
    assert_eq!(
        rows(&s),
        vec![
            (ACK, SlotId(7), 3, 0, 0, 0, 0, 0, k),
            (QUERY, SlotId(7), 3, 0, 0, 4_997_370, 420, 3, k),
        ],
        "PMK-032.3"
    );
    assert!(
        s.id_calls
            .contains(&(k.to_string().into_bytes(), Some(tgt(0).id))),
        "PMK-032.3"
    );
    // .4 queried again: the QueryResult only.
    let mut s = VecSink::default();
    query(&mut c, &mut s, UP, &body);
    let e = orders(&s);
    assert_eq!((e.len(), e[0].what), (1, QUERY), "PMK-032.4");
}

#[test]
fn pmk_033_stale_guard_is_the_gtd_lifetime() {
    use hft_venue_proto::StaleGuard;
    let StaleGuard::ExpireAt { default_ms } = Pm::stale_guard() else {
        panic!("PMK-033.1 {:?}", Pm::stale_guard());
    };
    assert_eq!(
        (default_ms, venue_polymarket::GTD_S),
        (venue_polymarket::GTD_S * 1000, 300),
        "PMK-033.1"
    );
    let c = cmd(GwOp::Place, UP, Side::Buy, 450, 5_000_000, Tif::PostOnly);
    let r = encode(&ctx(), &c, &tgt(0)).unwrap();
    let b: serde_json::Value = serde_json::from_slice(r.body.as_slice()).unwrap();
    let exp: i64 = b["order"]["expiration"].as_str().unwrap().parse().unwrap();
    assert_eq!(exp - NOW_MS / 1000, i64::from(default_ms / 1000), "PMK-033.2");
}

/// A `GwCtx` that signed `Place{UP, Buy, 0.42, 5 shares, PostOnly}` for slot 7 / gen 3 / epoch 5
/// and, before any answer, encoded the cancel of that slot; returns both requests too.
fn cancel_first() -> (GwCtx, RestReqBuf, RestReqBuf) {
    let mut c = ctx();
    let place = cmd(GwOp::Place, UP, Side::Buy, 420, 5_000_000, Tif::PostOnly);
    let p = encode_mut(&mut c, &place, &tgt(0)).unwrap();
    let r = encode_mut(&mut c, &cmd(GwOp::Cancel, UP, Side::Buy, 0, 0, Tif::Gtc), &tgt(0)).unwrap();
    (c, p, r)
}

/// The order hash a `POST /order` body signs (the exchange's `orderID`).
fn signed_hash(place: &RestReqBuf) -> String {
    let b: serde_json::Value = serde_json::from_slice(place.body.as_slice()).unwrap();
    let o = &b["order"];
    let n = |k: &str| o[k].as_str().unwrap().parse::<u64>().unwrap();
    let order = OrderV2 {
        salt: o["salt"].as_u64().unwrap(),
        maker: venue_wallet(),
        signer: venue_wallet(),
        token_id: parse_u256(o["tokenId"].as_str().unwrap()).unwrap(),
        maker_amount: n("makerAmount"),
        taker_amount: n("takerAmount"),
        side: 0,
        signature_type: 3,
        timestamp_ms: n("timestamp"),
        metadata: [0; 32],
        builder: [0; 32],
    };
    let hash = order_digest(&exchange_domain("2", CHAIN_ID, &EXCHANGE_V2), &order);
    let mut hex = [0u8; 66];
    hex_bytes(&hash, &mut hex).unwrap().to_string()
}

fn rest(c: &mut GwCtx, s: &mut VecSink, op: GwOp, status: u16, body: &str) -> hft_venue_proto::ReportOutcome {
    Pm::decode_rest(rd(), c, &origin(op, UP), status, body.as_bytes(), &RX, s).unwrap()
}

/// Answers measured 2026-10-28 (a cancel sent before its order was posted).
fn not_found(h: &str) -> String {
    format!(
        "{{\"not_canceled\":{{\"{h}\":\"order can't be found - already canceled or matched\"}},\"canceled\":[]}}"
    )
}
fn already_cancelled(h: &str) -> String {
    format!("{{\"error\":\"order {h} is invalid. Already canceled.\"}}")
}
fn placed_answer(h: &str, status: &str) -> String {
    format!(
        "{{\"errorMsg\":\"\",\"orderID\":\"{h}\",\"takingAmount\":\"\",\"makingAmount\":\"\",\"status\":\"{status}\",\"success\":true}}"
    )
}

#[test]
fn pmk_034_cancel_before_the_place_answer() {
    const CACK: OrderEventKind = OrderEventKind::CancelAck;
    const CREJ: OrderEventKind = OrderEventKind::CancelReject;
    const GONE: u8 = 1;
    const EARLY: u8 = 2;
    let s7 = SlotId(7);
    let kinds = |s: &VecSink| orders(s).iter().map(|e| (e.what, e.detail)).collect::<Vec<_>>();
    // .1 the cancel names the hash signed for the target's client id, before any answer; the
    // venue does not declare CANCEL_NEEDS_ACK.
    let (c, p, r) = cancel_first();
    let h = signed_hash(&p);
    let k = key_of(&h);
    assert_eq!(
        (r.method, text(r.path_query.as_slice()), text(r.body.as_slice())),
        (
            Method::Delete,
            "/order",
            format!("{{\"orderID\":\"{h}\"}}").as_str()
        ),
        "PMK-034.1"
    );
    assert!(header(&r, "POLY_SIGNATURE").is_some(), "PMK-034.1");
    assert!(
        !Pm::default_caps(|_| None)
            .flags
            .contains(hft_refdata::CapFlags::CANCEL_NEEDS_ACK),
        "PMK-034.1"
    );
    // .2 the cancel overtook the order: its "not found" is held back, the place's "Already
    // canceled" refusal ends the slot as cancelled.
    let mut c2 = c.clone();
    let mut s = VecSink::default();
    let o = rest(&mut c2, &mut s, GwOp::Cancel, 200, &not_found(&h));
    assert_eq!((o.events, orders(&s).len()), (0, 0), "PMK-034.2");
    assert_eq!(
        o.raw_code,
        raw_code_str(b"order can't be found - already canceled or matched"),
        "PMK-034.2"
    );
    let o = rest(&mut c2, &mut s, GwOp::Place, 400, &already_cancelled(&h));
    assert_eq!(o.events, 1, "PMK-034.2");
    assert!(o.outcome_unknown.is_none(), "PMK-034.2");
    assert_eq!(rows(&s), vec![(CACK, s7, 3, 0, 0, 0, 0, 0, k)], "PMK-034.2");
    // .3 the refusal first, then "not found": CancelAck, then CancelReject{GONE} (a duplicate
    // for the OMS, whose slot is already Cancelled).
    let mut c3 = c.clone();
    let mut s = VecSink::default();
    rest(&mut c3, &mut s, GwOp::Place, 400, &already_cancelled(&h));
    rest(&mut c3, &mut s, GwOp::Cancel, 200, &not_found(&h));
    assert_eq!(kinds(&s), vec![(CACK, 0), (CREJ, GONE)], "PMK-034.3");
    // .4 the order rested and the cancel removed it before the place answer: CancelAck of the
    // signed client id (bound now); the late place answer is an Ack; the user channel's
    // CANCELLATION adds nothing.
    let mut c4 = c.clone();
    let mut s = VecSink::default();
    let body = format!("{{\"not_canceled\":{{}},\"canceled\":[\"{h}\"]}}");
    let o = rest(&mut c4, &mut s, GwOp::Cancel, 200, &body);
    assert_eq!(o.events, 1, "PMK-034.4");
    assert_eq!(rows(&s), vec![(CACK, s7, 3, 5, 0, 0, 0, 0, k)], "PMK-034.4");
    assert!(
        s.id_calls
            .contains(&(k.to_string().into_bytes(), Some(tgt(0).id))),
        "PMK-034.4"
    );
    let mut s = VecSink::default();
    rest(&mut c4, &mut s, GwOp::Place, 200, &placed_answer(&h, "live"));
    assert_eq!(kinds(&s), vec![(ACK, 0)], "PMK-034.4");
    let mut s = VecSink::default();
    bind(&mut s, &h);
    let o = report(&mut c4, &mut s, &order_frame(&h, "CANCELLATION", "CANCELED", "0"));
    assert_eq!((o.events, orders(&s).len()), (0, 0), "PMK-034.4");
    // .5 "not found", yet the place answer says live: CancelReject{EARLY} before the Ack; the
    // second cancel names the hash again and its "not found" is a plain GONE.
    let mut c5 = c.clone();
    let mut s = VecSink::default();
    rest(&mut c5, &mut s, GwOp::Cancel, 200, &not_found(&h));
    let o = rest(&mut c5, &mut s, GwOp::Place, 200, &placed_answer(&h, "live"));
    assert_eq!(o.events, 2, "PMK-034.5");
    assert_eq!(
        rows(&s),
        vec![
            (CREJ, s7, 3, 0, 0, 0, 0, EARLY, k),
            (ACK, s7, 3, 0, 0, 0, 0, 0, k)
        ],
        "PMK-034.5"
    );
    let r = encode_mut(
        &mut c5,
        &cmd(GwOp::Cancel, UP, Side::Buy, 0, 0, Tif::Gtc),
        &tgt(k),
    )
    .unwrap();
    assert_eq!(
        text(r.body.as_slice()),
        format!("{{\"orderID\":\"{h}\"}}"),
        "PMK-034.5"
    );
    let mut s = VecSink::default();
    rest(&mut c5, &mut s, GwOp::Cancel, 200, &not_found(&h));
    assert_eq!(kinds(&s), vec![(CREJ, GONE)], "PMK-034.5");
    // .6 the place answer (live) first, then "not found" for the cancel sent before it: EARLY.
    let mut c6 = c.clone();
    let mut s = VecSink::default();
    rest(&mut c6, &mut s, GwOp::Place, 200, &placed_answer(&h, "live"));
    rest(&mut c6, &mut s, GwOp::Cancel, 200, &not_found(&h));
    assert_eq!(kinds(&s), vec![(ACK, 0), (CREJ, EARLY)], "PMK-034.6");
    // .7 "not found" and the place answer is lost (500): a query that finds the order LIVE gives
    // EARLY, then the Ack and QueryResult{New}.
    let mut c7 = c.clone();
    let mut s = VecSink::default();
    rest(&mut c7, &mut s, GwOp::Cancel, 200, &not_found(&h));
    let o = rest(&mut c7, &mut s, GwOp::Place, 500, "");
    assert!(o.outcome_unknown.is_some() && orders(&s).is_empty(), "PMK-034.7");
    query(&mut c7, &mut s, UP, &query_body(&h, T_UP, "LIVE", "0"));
    assert_eq!(
        orders(&s)
            .iter()
            .map(|e| (e.what, e.detail, e.slot))
            .collect::<Vec<_>>(),
        vec![(CREJ, EARLY, s7), (ACK, 0, s7), (QUERY, 1, s7)],
        "PMK-034.7"
    );
    // .8 "not found" and the place answer says matched: the Ack only (fills come from the user
    // channel).
    let mut c8 = c.clone();
    let mut s = VecSink::default();
    rest(&mut c8, &mut s, GwOp::Cancel, 200, &not_found(&h));
    rest(&mut c8, &mut s, GwOp::Place, 200, &placed_answer(&h, "matched"));
    assert_eq!(kinds(&s), vec![(ACK, 0)], "PMK-034.8");
    // .9 a refusal of another kind stays a Reject.
    let mut c9 = c;
    let mut s = VecSink::default();
    rest(&mut c9, &mut s, GwOp::Cancel, 200, &not_found(&h));
    let crosses = "{\"error\":\"invalid post-only order: order crosses book\"}";
    rest(&mut c9, &mut s, GwOp::Place, 400, crosses);
    assert_eq!(kinds(&s), vec![(OrderEventKind::Reject, 0)], "PMK-034.9");
}

fn encode_at(ctx: &mut GwCtx, c: &GwCmd, t: &TargetRef, ms: i64) -> RestReqBuf {
    let tpl = Pm::build_templates(&cfg(), rd(), AccountId(1), &cred()).unwrap();
    let mut out = RestReqBuf::default();
    let clk = WallClock {
        unix_ms: ms,
        unix_us: ms * 1000,
    };
    Pm::encode_rest_ctx(rd(), &tpl, ctx, &cred(), c, t, &clk, &mut out).unwrap();
    out
}

fn idle_at(ctx: &mut GwCtx, ms: i64) -> bool {
    let tpl = Pm::build_templates(&cfg(), rd(), AccountId(1), &cred()).unwrap();
    let clk = WallClock {
        unix_ms: ms,
        unix_us: ms * 1000,
    };
    Pm::gw_idle(rd(), &tpl, ctx, &cred(), &clk)
}

/// Idle steps until there is nothing left to do; returns how many signed an order.
fn idle_all(ctx: &mut GwCtx, ms: i64) -> usize {
    (0..64).take_while(|_| idle_at(ctx, ms)).count()
}

/// (timestamp, salt, signature) of a `POST /order` body, and whether the signature is the
/// wallet's over the body's own fields.
fn body_order(r: &RestReqBuf) -> (u64, u64, bool) {
    let b: serde_json::Value = serde_json::from_slice(r.body.as_slice()).unwrap();
    let o = &b["order"];
    let n = |k: &str| o[k].as_str().unwrap().parse::<u64>().unwrap();
    let order = OrderV2 {
        salt: o["salt"].as_u64().unwrap(),
        maker: venue_wallet(),
        signer: venue_wallet(),
        token_id: parse_u256(o["tokenId"].as_str().unwrap()).unwrap(),
        maker_amount: n("makerAmount"),
        taker_amount: n("takerAmount"),
        side: if o["side"] == "BUY" { 0 } else { 1 },
        signature_type: 3,
        timestamp_ms: n("timestamp"),
        metadata: [0; 32],
        builder: [0; 32],
    };
    let mut sig = [0u8; 512];
    let len = sign_order_1271(
        &Wallet::from_hex(KEY).unwrap(),
        &exchange_domain("2", CHAIN_ID, &EXCHANGE_V2),
        CHAIN_ID,
        &order,
        &mut sig,
    );
    let mut hex = [0u8; 1026];
    let want = hex_bytes(&sig[..len], &mut hex).unwrap();
    (
        order.timestamp_ms,
        order.salt,
        o["signature"].as_str().unwrap() == want,
    )
}

#[test]
fn pmk_035_presigned_orders() {
    use venue_polymarket::Presign;
    let on = Presign::new(1, 1, 10_000, 30_000).unwrap();
    let buy = |px| cmd(GwOp::Place, UP, Side::Buy, px, 5_000_000, Tif::PostOnly);
    // .1 configuration bounds.
    assert!(Presign::new(3, 4, 10_000, 30_000).is_some(), "PMK-035.1");
    assert!(Presign::new(4, 4, 10_000, 30_000).is_none(), "PMK-035.1");
    assert!(Presign::new(1, 1, 0, 30_000).is_none(), "PMK-035.1");
    assert!(Presign::new(1, 1, 10_000, 999).is_none(), "PMK-035.1");
    assert!(Presign::new(1, 1, 10_000, 240_001).is_none(), "PMK-035.1");
    // .2 off (default): no idle work, every place signed at the decision.
    let mut c = ctx();
    encode_at(&mut c, &buy(420), &tgt(0), NOW_MS);
    assert!(!idle_at(&mut c, NOW_MS), "PMK-035.2");
    assert_eq!(c.presigned(), 0, "PMK-035.2");
    // .3 on: the place at 0.42 anchors 0.41, 0.42, 0.43 (side, size of the place); three idle
    // steps sign them, then there is nothing to do.
    let mut c = ctx();
    c.set_presign(on);
    let mut t6 = tgt(0);
    t6.id.slot = 6;
    let mut p6 = buy(420);
    p6.slot = SlotId(6);
    let r = encode_at(&mut c, &p6, &t6, NOW_MS);
    let (ts0, _, ok) = body_order(&r);
    assert!(ok && ts0 == NOW_MS as u64, "PMK-035.3");
    assert_eq!(idle_all(&mut c, NOW_MS + 10), 3, "PMK-035.3");
    assert_eq!(c.presigned(), 3, "PMK-035.3");
    // .4 a place at 0.43 uses its pre-signed order (signed at NOW + 10 ms, unique timestamps
    // after the first place's); a second one at the same clock is signed afresh.
    let pre = encode_at(&mut c, &buy(430), &tgt(0), NOW_MS + 20);
    let (ts1, salt1, ok) = body_order(&pre);
    assert!(ok, "PMK-035.4");
    assert!(ts1 > ts0 && ts1 <= NOW_MS as u64 + 13, "PMK-035.4 {ts1}");
    assert_eq!(salt1 & 0xF_FFFF, 0, "PMK-035.4: no slot in a pre-signed salt");
    assert_eq!(c.presigned(), 2, "PMK-035.4: used once");
    let mut t8 = tgt(0);
    t8.id.slot = 8;
    let mut p8 = buy(430);
    p8.slot = SlotId(8);
    let r = encode_at(&mut c, &p8, &t8, NOW_MS + 20);
    let (ts2, salt2, ok) = body_order(&r);
    assert!(ok && ts2 == NOW_MS as u64 + 20, "PMK-035.4");
    assert_eq!(salt2 & 0xF_FFFF, 8, "PMK-035.4");
    // The order hash kept for the slot is the pre-signed one: its cancel names it.
    let r = encode_mut(&mut c, &cmd(GwOp::Cancel, UP, Side::Buy, 0, 0, Tif::Gtc), &tgt(0)).unwrap();
    assert_eq!(
        text(r.body.as_slice()),
        format!("{{\"orderID\":\"{}\"}}", signed_hash(&pre)),
        "PMK-035.4"
    );
    // .5 timestamps stay unique when the clock repeats: two places at one millisecond.
    let mut c2 = ctx();
    let a = body_order(&encode_at(&mut c2, &buy(420), &tgt(0), NOW_MS)).0;
    let b2 = body_order(&encode_at(&mut c2, &p8, &t8, NOW_MS)).0;
    assert_eq!((a, b2), (NOW_MS as u64, NOW_MS as u64 + 1), "PMK-035.5");
    // .6 ageing: from 3/4 of `max_age_ms` the step re-signs; past `max_age_ms` an order is not
    // used (the place signs afresh).
    let mut c = ctx();
    c.set_presign(on);
    encode_at(&mut c, &buy(420), &tgt(0), NOW_MS);
    assert_eq!(idle_all(&mut c, NOW_MS), 3, "PMK-035.6");
    assert_eq!(idle_all(&mut c, NOW_MS + 22_499), 0, "PMK-035.6");
    assert_eq!(idle_all(&mut c, NOW_MS + 22_504), 3, "PMK-035.6");
    let late = NOW_MS + 22_504 + 30_001;
    let (ts, _, ok) = body_order(&encode_at(&mut c, &buy(410), &tgt(0), late));
    assert!(ok && ts == late as u64, "PMK-035.6");
    // .7 rebinding the inst to another token drops its orders and anchor.
    let mut c = ctx();
    c.set_presign(on);
    encode_at(&mut c, &buy(420), &tgt(0), NOW_MS);
    idle_all(&mut c, NOW_MS);
    Pm::bind_order_wire(&mut c, UP, &parse_u256("12345678901234567890").unwrap()).unwrap();
    assert_eq!(c.presigned(), 0, "PMK-035.7");
    assert!(!idle_at(&mut c, NOW_MS + 1), "PMK-035.7");
    // .8 prices stay inside (0, 1): an anchor at 0.99 signs 0.98 and 0.99 only.
    let mut c = ctx();
    c.set_presign(on);
    encode_at(&mut c, &buy(990), &tgt(0), NOW_MS);
    assert_eq!(idle_all(&mut c, NOW_MS), 2, "PMK-035.8");
}

#[test]
fn pmk_037_cancels_have_their_own_rate_rule() {
    use hft_refdata::{LimitBucket, LimitScope};
    use hft_venue_proto::VenueProtocol;
    let rules: Vec<_> = Pm::rate_rules()
        .iter()
        .map(|r| (r.scope, r.bucket, r.window_ms, r.limit))
        .collect();
    assert_eq!(
        rules,
        [
            (LimitScope::Ip, LimitBucket::NewConns, 300_000, 300),
            (LimitScope::Conn, LimitBucket::WsInMsgs, 1_000, 10),
            (LimitScope::Account, LimitBucket::Orders, 1_000, 20),
            (LimitScope::Account, LimitBucket::Cancels, 1_000, 40),
        ],
        "PMK-037"
    );
}
