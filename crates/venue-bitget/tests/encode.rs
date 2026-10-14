//! BGT-064–BGT-078, BGT-076, BGT-080, BGT-081: order frames, the REST query, signatures, ids.

use crate::support;

use hft_event::{GwCmd, GwOp, OrderFlags};
use hft_types::{AccountId, ClientOrderId, InstId, Px, Qty, Side, SlotId, Tif, Tsc, VenueId};
use hft_venue_proto::{
    EncodeErr, Method, OrderChannel, ReqRef, RestReqBuf, TargetRef, VenueEnv, VenueProtocol,
};
use proptest::prelude::*;
use support::{ACCT, F_BTC, FUT, S_BGB, S_BTC, SPOT, WC, cfg, cid, cid_text, key, rd};
use venue_bitget::{
    BitgetSpot, BitgetUsdtFutures as Fut, Templates, decode_req_id, encode_req_id, rest_sign,
};

fn tpl() -> Templates {
    Fut::build_templates(&cfg(), rd(), ACCT, &key()).unwrap()
}

fn cmd(op: GwOp, inst: InstId, side: Side, tif: Tif, px: i64, qty: i64, oflags: u16) -> GwCmd {
    GwCmd {
        decision_seq: 1,
        decided_tsc: Tsc(0),
        px: Px(px),
        qty: Qty(qty),
        inst,
        slot: SlotId(42),
        r#gen: 7,
        target_slot: SlotId(42),
        op,
        account: ACCT,
        side,
        tif,
        oflags,
        _pad: [0; 2],
        arg: 0,
        arg2: 0,
    }
}

fn place() -> GwCmd {
    cmd(GwOp::Place, F_BTC, Side::Buy, Tif::Gtc, 6_500_010, 400_000, 0)
}

fn tgt(id: ClientOrderId) -> TargetRef {
    TargetRef { id, exch_order_id: 0 }
}

fn enc<P: VenueProtocol<Templates = Templates>>(c: &GwCmd, t: &TargetRef) -> Result<String, EncodeErr> {
    let mut body = [0u8; 2048];
    P::encode(rd(), &tpl(), c, t, &WC, &mut body).map(|n| String::from_utf8(body[..n].to_vec()).unwrap())
}

fn json(s: &str) -> serde_json::Value {
    serde_json::from_str(s).unwrap()
}

#[test]
fn bgt_064_place_frame() {
    let f = enc::<Fut>(&place(), &tgt(cid(FUT, 3, 42, 7))).unwrap();
    assert_eq!(
        f,
        r#"{"op":"trade","id":"p0000002a00000007","category":"usdt-futures","topic":"place-order","args":[{"symbol":"BTCUSDT","side":"buy","orderType":"limit","price":"65000.1","qty":"0.004","timeInForce":"gtc","clientOid":"100030a020002a000000078d"}]}"#
    );
    assert!(!f.contains("apiKey") && !f.contains("sign") && !f.contains("passphrase"));
    assert!(!f.contains("reduceOnly"));
}

#[test]
fn bgt_065_place_variants() {
    let t = tgt(cid(FUT, 3, 42, 7));
    let arg = |f: &str| json(f)["args"][0].clone();
    let mut c = place();
    c.tif = Tif::Ioc;
    c.side = Side::Sell;
    let a = arg(&enc::<Fut>(&c, &t).unwrap());
    assert_eq!(
        (a["side"].as_str(), a["timeInForce"].as_str()),
        (Some("sell"), Some("ioc")),
        "BGT-065.1"
    );
    let mut c = place();
    c.tif = Tif::PostOnly;
    assert_eq!(
        arg(&enc::<Fut>(&c, &t).unwrap())["timeInForce"],
        "post_only",
        "BGT-065.2"
    );
    let mut c = place();
    c.tif = Tif::Fok;
    assert_eq!(
        arg(&enc::<Fut>(&c, &t).unwrap())["timeInForce"],
        "fok",
        "BGT-065.3"
    );
    let mut c = place();
    c.oflags = OrderFlags::REDUCE_ONLY.0;
    assert_eq!(
        arg(&enc::<Fut>(&c, &t).unwrap())["reduceOnly"],
        "YES",
        "BGT-065.4"
    );
    let mut c = place();
    c.oflags = OrderFlags::RISK_REDUCING.0;
    assert!(
        arg(&enc::<Fut>(&c, &t).unwrap()).get("reduceOnly").is_none(),
        "BGT-065.5"
    );
    let mut c = place();
    c.oflags = OrderFlags::SMOKE.0;
    assert_eq!(enc::<Fut>(&c, &t), enc::<Fut>(&place(), &t), "BGT-065.6");
    let mut c = place();
    c.inst = S_BTC;
    assert_eq!(enc::<Fut>(&c, &t), Err(EncodeErr::UnknownInst), "BGT-065.7");
    c.inst = InstId(9999);
    assert_eq!(enc::<Fut>(&c, &t), Err(EncodeErr::UnknownInst), "BGT-065.8");
}

#[test]
fn bgt_066_spot_place() {
    let t = tgt(cid(SPOT, 3, 42, 7));
    let c = cmd(GwOp::Place, S_BGB, Side::Sell, Tif::Ioc, 12_345, 100_000_000, 0);
    let f = enc::<BitgetSpot>(&c, &t).unwrap();
    assert_eq!(
        f,
        r#"{"op":"trade","id":"p0000002a00000007","category":"spot","topic":"place-order","args":[{"symbol":"BGBUSDT","side":"sell","orderType":"limit","price":"1.2345","qty":"1","timeInForce":"ioc","clientOid":"100030b020002a000000079d","autoBorrow":"no"}]}"#,
        "BGT-066.1"
    );
    let mut c2 = c;
    c2.oflags = OrderFlags::REDUCE_ONLY.0;
    assert_eq!(
        enc::<BitgetSpot>(&c2, &t),
        Err(EncodeErr::Unsupported(GwOp::Place)),
        "BGT-066.2"
    );
    c2.op = GwOp::Amend;
    assert_eq!(
        enc::<BitgetSpot>(&c2, &t),
        Err(EncodeErr::Unsupported(GwOp::Amend)),
        "BGT-066.3"
    );
    let mut c3 = c;
    c3.tif = Tif::Fok;
    assert!(
        enc::<BitgetSpot>(&c3, &t)
            .unwrap()
            .contains(r#""timeInForce":"fok""#),
        "BGT-066.4"
    );
}

#[test]
fn bgt_067_shortest_decimal() {
    let t = tgt(cid(FUT, 3, 42, 7));
    let px_of = |inst: InstId, px: i64, qty: i64| -> (String, String, usize) {
        let mut c = place();
        c.inst = inst;
        c.px = Px(px);
        c.qty = Qty(qty);
        let f = if inst == S_BGB {
            enc::<BitgetSpot>(&c, &tgt(cid(SPOT, 3, 42, 7))).unwrap()
        } else {
            enc::<Fut>(&c, &t).unwrap()
        };
        let a = &json(&f)["args"][0];
        (
            a["price"].as_str().unwrap().to_string(),
            a["qty"].as_str().unwrap().to_string(),
            f.len(),
        )
    };
    assert_eq!(px_of(F_BTC, 6_500_010, 1).0, "65000.1", "BGT-067.1");
    assert_eq!(px_of(F_BTC, 6_500_000, 1).0, "65000", "BGT-067.2");
    assert_eq!(px_of(F_BTC, 1, 1).0, "0.01", "BGT-067.3");
    assert_eq!(px_of(F_BTC, 1, 400_000).1, "0.004", "BGT-067.4");
    assert_eq!(px_of(F_BTC, 1, 100_000_000).1, "1", "BGT-067.5");
    assert_eq!(px_of(F_BTC, 1, 1).1, "0.00000001", "BGT-067.6");
    assert_eq!(px_of(S_BGB, 12_345, 1).0, "1.2345", "BGT-067.7");
    assert_eq!(px_of(S_BGB, 10_000, 1).0, "1", "BGT-067.8");
    let (p, _, n) = px_of(F_BTC, i64::MAX, 1);
    assert_eq!(p, "92233720368547758.07", "BGT-067.9");
    assert!(n <= 2048);
}

#[test]
fn bgt_069_cancel_frame() {
    let c = cmd(GwOp::Cancel, F_BTC, Side::Buy, Tif::Gtc, 0, 0, 0);
    let f = enc::<Fut>(&c, &tgt(cid(FUT, 2, 42, 7))).unwrap();
    assert_eq!(
        f,
        r#"{"op":"trade","id":"c0000002a00000007","category":"usdt-futures","topic":"cancel-order","args":[{"clientOid":"100020a020002a000000079d"}]}"#
    );
}

#[test]
fn bgt_070_amend_frame() {
    let c = cmd(GwOp::Amend, F_BTC, Side::Buy, Tif::Gtc, 6_500_020, 300_000, 0);
    let f = enc::<Fut>(&c, &tgt(cid(FUT, 3, 42, 7))).unwrap();
    assert_eq!(
        f,
        r#"{"op":"trade","id":"m0000002a00000007","category":"usdt-futures","topic":"modify-order","args":[{"symbol":"BTCUSDT","clientOid":"100030a020002a000000078d","price":"65000.2","qty":"0.003","autoCancel":"no"}]}"#
    );
}

#[test]
fn bgt_071_cancel_all_per_order() {
    let mut c = cmd(GwOp::CancelAll, F_BTC, Side::Buy, Tif::Gtc, 0, 0, 0);
    c.slot = SlotId(0);
    c.r#gen = 0;
    let mut body = [0u8; 2048];
    assert_eq!(
        Fut::encode(rd(), &tpl(), &c, &tgt(cid(FUT, 3, 42, 7)), &WC, &mut body),
        Err(EncodeErr::Unsupported(GwOp::CancelAll)),
        "BGT-071.1"
    );
    assert!(body.iter().all(|&b| b == 0), "BGT-071.1: nothing written");
    let mut c2 = cmd(GwOp::Cancel, F_BTC, Side::Buy, Tif::Gtc, 0, 0, 0);
    c2.target_slot = SlotId(43);
    let f = enc::<Fut>(&c2, &tgt(cid(FUT, 3, 43, 1))).unwrap();
    assert!(
        f.contains(r#""id":"c0000002b00000001""#) && f.contains("100030a020002b000000018a"),
        "BGT-071.2"
    );
    let c3 = cmd(GwOp::Cancel, F_BTC, Side::Buy, Tif::Gtc, 0, 0, 0);
    assert_eq!(
        enc::<Fut>(&c3, &tgt(cid(FUT, 2, 42, 7))),
        enc::<Fut>(
            &cmd(GwOp::Cancel, F_BTC, Side::Sell, Tif::Ioc, 1, 1, 0),
            &tgt(cid(FUT, 2, 42, 7))
        ),
        "BGT-071.3"
    );
}

#[test]
fn bgt_072_query_rest() {
    let c = cmd(GwOp::Query, F_BTC, Side::Buy, Tif::Gtc, 0, 0, 0);
    assert_eq!(Fut::order_channel(GwOp::Query), OrderChannel::Rest);
    assert_eq!(Fut::order_channel(GwOp::Place), OrderChannel::Ws);
    assert_eq!(
        enc::<Fut>(&c, &tgt(cid(FUT, 3, 42, 7))),
        Err(EncodeErr::Unsupported(GwOp::Query))
    );
    let mut out = RestReqBuf::default();
    Fut::encode_rest_signed(
        rd(),
        &tpl(),
        &crate::support::key(),
        &c,
        &tgt(cid(FUT, 3, 42, 7)),
        &WC,
        &mut out,
    )
    .unwrap();
    assert_eq!((out.endpoint, out.method, out.redacted), (0, Method::Get, true));
    let pq = std::str::from_utf8(out.path_query.as_slice()).unwrap();
    assert_eq!(pq, "/api/v3/trade/order-info?clientOid=100030a020002a000000078d");
    let sig = rest_sign(b"secret", b"1790000000000", b"GET", pq.as_bytes(), b"").unwrap();
    let h = std::str::from_utf8(out.headers.as_slice()).unwrap();
    assert_eq!(
        h,
        format!(
            "ACCESS-KEY: k\r\nACCESS-SIGN: {}\r\nACCESS-TIMESTAMP: 1790000000000\r\nACCESS-PASSPHRASE: p\r\nlocale: en-US\r\n",
            std::str::from_utf8(&sig).unwrap()
        )
    );
    // Demo trading adds `paptrading: 1`.
    let mut demo_cfg = cfg();
    demo_cfg.env = VenueEnv::Testnet;
    let demo = Fut::build_templates(&demo_cfg, rd(), ACCT, &key()).unwrap();
    Fut::encode_rest_signed(
        rd(),
        &demo,
        &crate::support::key(),
        &c,
        &tgt(cid(FUT, 3, 42, 7)),
        &WC,
        &mut out,
    )
    .unwrap();
    assert!(out.headers.as_slice().ends_with(b"paptrading: 1\r\n"));
    // Spot inst on the perpetual line.
    let mut c2 = c;
    c2.inst = S_BTC;
    assert_eq!(
        Fut::encode_rest_signed(
            rd(),
            &tpl(),
            &crate::support::key(),
            &c2,
            &tgt(cid(FUT, 3, 42, 7)),
            &WC,
            &mut out
        ),
        Err(EncodeErr::UnknownInst)
    );
}

#[test]
fn bgt_073_buffer_too_small() {
    let t = tgt(cid(FUT, 3, 42, 7));
    let n0 = enc::<Fut>(&place(), &t).unwrap().len();
    for (row, len, ok) in [
        ("BGT-073.1", n0, true),
        ("BGT-073.2", n0 - 1, false),
        ("BGT-073.3", 0, false),
    ] {
        let mut body = vec![0u8; len];
        let r = Fut::encode(rd(), &tpl(), &place(), &t, &WC, &mut body);
        if ok {
            assert_eq!(r, Ok(n0), "{row}");
        } else {
            assert_eq!(r, Err(EncodeErr::FieldOverflow), "{row}");
        }
    }
}

#[test]
fn bgt_074_defaults() {
    let t = tgt(cid(FUT, 3, 42, 7));
    let mut body = [0u8; 2048];
    assert_eq!(
        Fut::encode_batch(rd(), &tpl(), &[place(), place()], &[t, t], &WC, &mut body),
        Err(EncodeErr::Unsupported(GwOp::Place)),
        "BGT-074.1"
    );
    assert!(!Fut::needs_per_order_signing(), "BGT-074.2");
    let mut c = place();
    c.op = GwOp::SessionKeepalive;
    assert_eq!(
        enc::<Fut>(&c, &t),
        Err(EncodeErr::Unsupported(GwOp::SessionKeepalive)),
        "BGT-074.3"
    );
}

#[test]
fn bgt_076_request_id() {
    assert_eq!(
        &encode_req_id(GwOp::Place, 42, 7).unwrap(),
        b"p0000002a00000007",
        "BGT-076.1"
    );
    let r = |s: &str| decode_req_id(s.as_bytes());
    assert_eq!(
        r("p0000002a00000007"),
        Some(ReqRef {
            slot: 42,
            r#gen: 7,
            op: GwOp::Place
        }),
        "BGT-076.2"
    );
    assert_eq!(
        r("c0000002a00000007").map(|x| x.op),
        Some(GwOp::Cancel),
        "BGT-076.3"
    );
    assert_eq!(
        r("m0000002a00000007").map(|x| x.op),
        Some(GwOp::Amend),
        "BGT-076.3"
    );
    assert_eq!(r("x0000002a00000007"), None, "BGT-076.3");
    assert_eq!(
        r("pffffffffffffffff"),
        Some(ReqRef {
            slot: u32::MAX,
            r#gen: u32::MAX,
            op: GwOp::Place
        }),
        "BGT-076.4"
    );
    for (row, s) in [
        ("BGT-076.5", "q0000002a00000007"),
        ("BGT-076.6", "P0000002a00000007"),
        ("BGT-076.7", "p0000002A00000007"),
        ("BGT-076.8", "p0000002a0000000"),
        ("BGT-076.9", "p0000002a000000070"),
        ("BGT-076.10", ""),
    ] {
        assert_eq!(r(s), None, "{row}");
    }
    assert_eq!(encode_req_id(GwOp::Query, 1, 1), None);
    assert_eq!(encode_req_id(GwOp::CancelAll, 1, 1), None);
}

#[test]
fn bgt_077_rest_sign() {
    let s = |m: &[u8], pq: &str, b: &str| {
        String::from_utf8(
            rest_sign(b"secret", b"1790000000000", m, pq.as_bytes(), b.as_bytes())
                .unwrap()
                .to_vec(),
        )
        .unwrap()
    };
    assert_eq!(
        s(b"GET", "/api/v3/account/settings", ""),
        "DBRPdhO1WymCRYZqSPp0Y7bUgchoZbxoC3NGixZ6GNA=",
        "BGT-077.1"
    );
    assert_eq!(
        s(b"GET", "/api/v3/account/settings?symbol=BTCUSDT", ""),
        "GgDIq9kSphQ6PpVFKQ+XGUeLg+hqA20GYJCbDrYcL98=",
        "BGT-077.2"
    );
    assert_eq!(
        s(b"POST", "/api/v3/trade/place-order", r#"{"symbol":"BTCUSDT"}"#),
        "NCZu4d/3HNx//PS1ST6exwD6MHP14nsbYBZU5eaTmhE=",
        "BGT-077.3"
    );
    assert_eq!(
        s(b"get", "/api/v3/account/settings", ""),
        "xw04pj6Z+llprpM+eKuzvlv1se8i6bSALRbUv7jQaGE=",
        "BGT-077.4"
    );
}

#[test]
fn bgt_080_client_oid() {
    let id = cid(FUT, 3, 42, 7);
    let mut out = [0u8; 48];
    assert_eq!(Fut::render_cid(&id, &mut out), 24, "BGT-080.1");
    assert_eq!(&out[..24], b"100030a020002a000000078d", "BGT-080.1");
    assert_eq!(Fut::parse_cid(b"100030a020002a000000078d"), Some(id), "BGT-080.2");
    for (row, s) in [
        ("BGT-080.3", &b"100030a020002a000000078c"[..]),
        ("BGT-080.4", b"100030A020002A000000078D"),
        ("BGT-080.5", b"100030a020002a000000078"),
        ("BGT-080.6", b"100030a020002a000000078d0"),
        ("BGT-080.7", b""),
        ("BGT-080.8", b"web_100030a020002a000000078d"),
    ] {
        assert_eq!(Fut::parse_cid(s), None, "{row}");
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(5_000))]
    #[test]
    fn bgt_081_prop_cid_roundtrip(epoch in 0u16..(1 << 15), v in prop_oneof![Just(10u8), Just(11u8)],
                                  account in 0u8..=255, slot in 0u32..(1 << 20), r#gen in any::<u32>()) {
        let id = ClientOrderId { epoch, venue: VenueId(v), account: AccountId(account), slot, r#gen };
        let mut out = [0u8; 48];
        let n = Fut::render_cid(&id, &mut out);
        prop_assert_eq!(n, 24);
        prop_assert!(out[..24].iter().all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(c)));
        prop_assert_eq!(Fut::parse_cid(&out[..24]), Some(id));
    }

    #[test]
    fn bgt_075_prop_encode_roundtrip(op in prop_oneof![Just(GwOp::Place), Just(GwOp::Amend), Just(GwOp::Cancel)],
                                     tif in prop_oneof![Just(Tif::Gtc), Just(Tif::Ioc), Just(Tif::PostOnly), Just(Tif::Fok)],
                                     buy in any::<bool>(), pxm in 1i64..1_000_000_000, qm in 1i64..1_000_000_000,
                                     slot in 0u32..(1 << 20), r#gen in any::<u32>(), epoch in 0u16..(1 << 15)) {
        let mut c = cmd(op, F_BTC, if buy { Side::Buy } else { Side::Sell }, tif, pxm * 10, qm * 100_000, 0);
        c.slot = SlotId(slot);
        c.r#gen = r#gen;
        let id = ClientOrderId { epoch, venue: FUT, account: ACCT, slot, r#gen };
        let f = enc::<Fut>(&c, &tgt(id)).unwrap();
        prop_assert!(f.len() <= 2048);
        let v = json(&f);
        let req = decode_req_id(v["id"].as_str().unwrap().as_bytes()).unwrap();
        prop_assert_eq!((req.op, req.slot, req.r#gen), (op, slot, r#gen));
        let a = &v["args"][0];
        prop_assert_eq!(Fut::parse_cid(a["clientOid"].as_str().unwrap().as_bytes()), Some(id));
        prop_assert_eq!(cid_text(&id), a["clientOid"].as_str().unwrap());
        if op != GwOp::Cancel {
            let px = hft_types::parse_exact(a["price"].as_str().unwrap().as_bytes(), -2).unwrap();
            let q = hft_types::parse_exact(a["qty"].as_str().unwrap().as_bytes(), -8).unwrap();
            prop_assert_eq!((px, q), (c.px.0, c.qty.0));
        }
    }
}
