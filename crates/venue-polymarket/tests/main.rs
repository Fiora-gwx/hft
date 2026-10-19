//! venue-polymarket tests (TESTS.md). Frames are copied from the market channel as recorded on
//! 2026-10-19 (pmrec, eu-west-1); token ids shortened only where the case says so.

use std::sync::OnceLock;

use hft_event::{L2BatchHdr, L2SnapHdr, Level, MarketHdr, Trade};
use hft_refdata::{
    AssetClass, AssetSpec, Exchange, FeePair, FeeSpec, InstKind, InstSpec, InstStatus, RateLimit, RdbBuilder,
    RefData, StaleGuardKind, TsUnit, VenueSpec,
};
use hft_types::{AccountId, AssetExp, AssetId, ConnId, InstId, Notional, Px, Qty, Rate, Side, VenueId};
use hft_venue_proto::testkit::{MdRec, VecSink};
use hft_venue_proto::{
    DecodeErr, MsgOp, ProtoErr, RouteId, RxMeta, SeqRule, StreamKind, SubFrames, SubSpec, VenueCfg, VenueEnv,
    VenueProtocol,
};
use venue_polymarket::{BindErr, FeedCtx, MdClass, PolymarketClob as Pm};

const V: VenueId = VenueId::POLYMARKET_CLOB;
const UP: InstId = InstId(1);
const DN: InstId = InstId(2);
/// Up / Down tokens of `btc-updown-5m-1792424100` (recorded).
const T_UP: &str = "93260813640452160844731182437467962294350968265707341153474211213683765578930";
const T_DN: &str = "58004665500158711684886517185121669615527317995671495798460764652022651132693";
const MARKET: RouteId = RouteId(0);

/// `RD-PM`: USDC (AssetId 1, Stable), two outcome-token assets (px_exp −3, qty_exp −6), insts
/// `btc-up-0` = 1 and `btc-dn-0` = 2 on `POLYMARKET_CLOB` (tick 0.01, lot 0.000001).
fn rd() -> &'static RefData {
    static RD: OnceLock<&'static RefData> = OnceLock::new();
    RD.get_or_init(|| {
        let mut b = RdbBuilder::new(1);
        b.created_utc_ns(1_759_276_800_000_000_000);
        for (id, name, class, px, qty) in [
            (AssetId(1), "USDC", AssetClass::Stable, -6, -6),
            (AssetId(2), "PMBTCUP0", AssetClass::Crypto, -3, -6),
            (AssetId(3), "PMBTCDN0", AssetClass::Crypto, -3, -6),
        ] {
            b.asset(AssetSpec {
                id,
                name: name.into(),
                class,
                exp: AssetExp {
                    px_exp: px,
                    qty_exp: qty,
                },
                px_max: Px(1_000_000_000),
                qty_max: Qty(1_000_000_000_000_000),
                dust: Qty(1),
            });
        }
        b.venue(VenueSpec {
            id: V,
            exchange: Exchange::Polymarket,
            product: InstKind::Spot,
            ws_max_streams_per_conn: 500,
            ws_max_in_msgs_per_s: 10,
            ws_max_conn_age_s: 86_400,
            ws_new_conn_per_5min_ip: 300,
            app_ping_interval_s: 10,
            ts_unit: TsUnit::Ms,
            client_id_max_len: 32,
            client_id_prefix: "".into(),
            max_batch: 1,
            max_inflight_amend: 1,
            crossed_tolerance_default: 0,
            stale_guard: StaleGuardKind::None,
        });
        b.fee(FeeSpec {
            venue: V,
            tier: 0,
            fee: FeePair {
                maker: Rate(0),
                taker: Rate(0),
            },
        });
        b.caps(Pm::default_caps(|_| None));
        for r in Pm::rate_rules() {
            b.limit(
                V,
                RateLimit {
                    scope: r.scope,
                    bucket: r.bucket,
                    limit: r.limit,
                    window_ms: r.window_ms,
                    default_cost: 1,
                },
            );
        }
        for (id, sym, asset) in [(UP, "btc-up-0", AssetId(2)), (DN, "btc-dn-0", AssetId(3))] {
            b.inst(InstSpec {
                id,
                venue: V,
                symbol: sym.into(),
                status: InstStatus::Trading,
                kind: InstKind::Spot,
                asset,
                quote: AssetId(1),
                tick: 10,
                lot: 1,
                min_qty: Qty(1),
                max_qty: Qty(1_000_000_000_000),
                min_notional: Notional(1_000_000),
                book_window: 128,
                l2_depth: 100,
                crossed_tolerance: 0,
                tick_str: "0.01".into(),
                lot_str: "0.000001".into(),
                min_qty_str: "0.000001".into(),
                ..InstSpec::default()
            });
        }
        let blob = b.finish();
        match RefData::load(&blob, None) {
            Ok(r) => Box::leak(Box::new(r)),
            Err(e) => panic!("RD-PM rejected: {e:?}"),
        }
    })
}

const RX: RxMeta = RxMeta {
    rx_tsc: 1000,
    hw_rx_ns: 1_792_424_114_100_000_000,
    sw_rx_ns: 1_792_424_114_100_500_000,
    prev_override: 0,
    conn: ConnId(3),
    account: AccountId(0),
    _pad: [0; 2],
    ts_flags: 0,
    rx_empty_gap_ns: 2500,
    _pad2: 0,
};

fn bound() -> FeedCtx {
    let mut c = FeedCtx::with_capacity(16);
    assert_eq!(c.bind(UP, T_UP.as_bytes()), Ok(()));
    assert_eq!(c.bind(DN, T_DN.as_bytes()), Ok(()));
    c
}

fn hdr(inst: InstId, ex_ms: i64) -> MarketHdr {
    MarketHdr {
        venue: V,
        conn: RX.conn,
        _pad: 0,
        inst,
        ex_ts_ns: ex_ms * 1_000_000,
        ex_send_ns: 0,
        hw_rx_ns: RX.hw_rx_ns,
        rx_tsc: RX.rx_tsc,
        sw_rx_ns: RX.sw_rx_ns,
        rx_empty_gap_ns: RX.rx_empty_gap_ns,
        _rsv: 0,
    }
}

fn dec(ctx: &mut FeedCtx, p: &str) -> (Result<hft_venue_proto::MdOutcome, DecodeErr>, Vec<MdRec>) {
    let mut s = VecSink::default();
    let c = Pm::classify(
        MARKET,
        MsgOp::Text,
        p.as_bytes().get(..64).unwrap_or(p.as_bytes()),
    );
    let r = Pm::decode_md(rd(), ctx, c, p.as_bytes(), &RX, &mut s);
    (r, s.md)
}

fn lv(px: i64, qty: i64) -> Level {
    Level {
        px: Px(px),
        qty: Qty(qty),
    }
}

fn book_up() -> String {
    format!(
        "{{\"market\":\"0xf515e6967f410f73fa60dab5f4469b3c89c7ff49548e22114e07f0d7c8d92a1e\", \"asset_id\":\"{T_UP}\", \
         \"bids\":[{{\"price\":\"0.01\", \"size\":\"9468.04\"}}, {{\"price\":\"0.49\", \"size\":\"0\"}}, {{\"price\":\"0.5\", \"size\":\"242.09\"}}], \
         \"asks\":[{{\"price\":\"0.99\", \"size\":\"1200\"}}, {{\"price\":\"0.51\", \"size\":\"1.960785\"}}], \
         \"hash\":\"40d4258e2ed98c21699147377fa43ffe35665bd7\", \"timestamp\":\"1792424113368\", \"event_type\":\"book\"}}"
    )
}

fn price_change() -> String {
    format!(
        "{{\"market\":\"0xf515e6967f410f73fa60dab5f4469b3c89c7ff49548e22114e07f0d7c8d92a1e\", \"price_changes\":[\
         {{\"asset_id\":\"{T_UP}\", \"price\":\"0.42\", \"size\":\"242.09\", \"side\":\"BUY\", \"hash\":\"d96398aa668ede9a7247a2cc71296c96c924c4ee\", \"best_bid\":\"0.5\", \"best_ask\":\"0.51\"}}, \
         {{\"asset_id\":\"{T_DN}\", \"price\":\"0.58\", \"size\":\"242.09\", \"side\":\"SELL\", \"hash\":\"2be57c4c2cf2b973372339baf87379ce320322e6\", \"best_bid\":\"0.49\", \"best_ask\":\"0.5\"}}], \
         \"timestamp\":\"1792424114058\", \"event_type\":\"price_change\"}}"
    )
}

fn last_trade() -> String {
    format!(
        "{{\"market\":\"0xf515e6967f410f73fa60dab5f4469b3c89c7ff49548e22114e07f0d7c8d92a1e\", \"asset_id\":\"{T_UP}\", \
         \"price\":\"0.51\", \"size\":\"1.960785\", \"fee_rate_bps\":\"0\", \"side\":\"BUY\", \"timestamp\":\"1792424441146\", \
         \"event_type\":\"last_trade_price\", \"transaction_hash\":\"0xfef1f6c5dd0f86a783f8056e1e7249d063715fe18bc379040a5236a59490a21a\"}}"
    )
}

#[test]
fn pmk_001_classify_push_pong_unknown() {
    assert_eq!(
        Pm::classify(MARKET, MsgOp::Text, b"{\"market\":\"0x"),
        MdClass::Push
    );
    assert_eq!(
        Pm::classify(MARKET, MsgOp::Text, b"[{\"market\":\"0x"),
        MdClass::Push
    );
    assert_eq!(Pm::classify(MARKET, MsgOp::Text, b"PONG"), MdClass::Pong);
    assert_eq!(Pm::classify(MARKET, MsgOp::Binary, b"{}"), MdClass::Unknown);
    assert_eq!(
        Pm::classify(MARKET, MsgOp::Text, b"INVALID OPERATION"),
        MdClass::Unknown
    );
    assert_eq!(
        Pm::control(MdClass::Pong, b"PONG"),
        Some(hft_venue_proto::ControlHint::Pong)
    );
    assert_eq!(Pm::control(MdClass::Push, b"{}"), None);
    assert_eq!(Pm::seq_rule(MdClass::Push), SeqRule::None);
}

#[test]
fn pmk_002_peek_stream_and_bound_inst() {
    let ctx = bound();
    let p = |s: &str| Pm::peek(rd(), &ctx, MdClass::Push, s.as_bytes());
    let b = p(&book_up()).ok();
    assert_eq!(b.map(|x| (x.inst, x.stream)), Some((UP, StreamKind::DepthTopN)));
    let c = p(&price_change()).ok();
    assert_eq!(c.map(|x| (x.inst, x.stream)), Some((UP, StreamKind::DepthTopN)));
    let t = p(&last_trade()).ok();
    assert_eq!(t.map(|x| (x.inst, x.stream)), Some((UP, StreamKind::Trade)));
    let tick = format!(
        "{{\"event_type\":\"tick_size_change\",\"asset_id\":\"{T_UP}\",\"market\":\"0x1\",\"old_tick_size\":\"0.01\",\"new_tick_size\":\"0.001\",\"timestamp\":\"1\"}}"
    );
    assert_eq!(p(&tick).map(|x| x.inst), Err(DecodeErr::UnknownClass));
    let other = book_up().replace(T_UP, "12345");
    assert_eq!(p(&other).map(|x| x.inst), Ok(InstId(0)));
}

#[test]
fn pmk_003_book_is_a_snapshot_without_zero_levels() {
    let mut ctx = bound();
    let (r, ev) = dec(&mut ctx, &book_up());
    assert_eq!(r.map(|o| (o.events, o.inst)), Ok((1, UP)));
    let want = MdRec::L2Snapshot {
        hdr: L2SnapHdr {
            h: hdr(UP, 1_792_424_113_368),
            snap_id: 1,
            origin: L2SnapHdr::EXCHANGE,
            _pad: [0; 3],
            n_bids: 2,
            n_asks: 2,
        },
        bids: vec![lv(10, 9_468_040_000), lv(500, 242_090_000)],
        asks: vec![lv(990, 1_200_000_000), lv(510, 1_960_785)],
        part: 0,
        parts: 1,
    };
    assert_eq!(ev, vec![want]);
    // A second book of the same token gets the next id.
    let (_, ev2) = dec(&mut ctx, &book_up());
    assert!(matches!(ev2.first(), Some(MdRec::L2Snapshot { hdr, .. }) if hdr.snap_id == 2));
}

#[test]
fn pmk_004_price_change_one_chained_batch_per_token() {
    let mut ctx = bound();
    let _ = dec(&mut ctx, &book_up());
    let (r, ev) = dec(&mut ctx, &price_change());
    assert_eq!(r.map(|o| (o.events, o.inst)), Ok((2, UP)));
    let batch = |inst, prev, last, nb, na| {
        MdRec::L2Begin(L2BatchHdr {
            h: hdr(inst, 1_792_424_114_058),
            prev_id: prev,
            last_id: last,
            n_bids: nb,
            n_asks: na,
            bflags: 0,
            _pad: 0,
        })
    };
    assert_eq!(
        ev,
        vec![
            batch(UP, 1, 2, 1, 0),
            MdRec::L2Level(Side::Buy, Px(420), Qty(242_090_000)),
            MdRec::L2End,
            // The Down token had no book yet: its chain starts at 0.
            batch(DN, 0, 1, 0, 1),
            MdRec::L2Level(Side::Sell, Px(580), Qty(242_090_000)),
            MdRec::L2End,
        ]
    );
}

#[test]
fn pmk_005_last_trade_price_is_a_trade_with_a_synthetic_id() {
    let mut ctx = bound();
    let (r, ev) = dec(&mut ctx, &last_trade());
    assert_eq!(r.map(|o| o.events), Ok(1));
    assert_eq!(
        ev,
        vec![MdRec::Trade(Trade {
            h: hdr(UP, 1_792_424_441_146),
            px: Px(510),
            qty: Qty(1_960_785),
            trade_id: 1,
            taker_side: Side::Buy,
            _pad: [0; 7],
        })]
    );
    let (_, ev2) = dec(&mut ctx, &last_trade());
    assert!(matches!(ev2.first(), Some(MdRec::Trade(t)) if t.trade_id == 2));
}

#[test]
fn pmk_006_list_frame_of_two_books() {
    let mut ctx = bound();
    let dn = book_up().replace(T_UP, T_DN);
    let frame = format!("[{},{}]", book_up(), dn);
    let (r, ev) = dec(&mut ctx, &frame);
    assert_eq!(r.map(|o| (o.events, o.inst)), Ok((2, UP)));
    let insts: Vec<InstId> = ev
        .iter()
        .filter_map(|e| match e {
            MdRec::L2Snapshot { hdr, .. } => Some(hdr.h.inst),
            _ => None,
        })
        .collect();
    assert_eq!(insts, vec![UP, DN]);
}

#[test]
fn pmk_007_bad_number_writes_nothing() {
    let mut ctx = bound();
    let bad = price_change().replace("\"0.58\"", "\"0.5x\"");
    let (r, ev) = dec(&mut ctx, &bad);
    assert_eq!(r, Err(DecodeErr::Number));
    assert!(ev.is_empty());
    // Finer than the price exponent is a number error too.
    let fine = price_change().replace("\"0.42\"", "\"0.4215\"");
    assert_eq!(dec(&mut ctx, &fine).0, Err(DecodeErr::Number));
    // Unbound tokens are skipped without events.
    let mut empty = FeedCtx::with_capacity(4);
    let (r, ev) = dec(&mut empty, &price_change());
    assert_eq!(r.map(|o| o.events), Ok(0));
    assert!(ev.is_empty());
}

#[test]
fn pmk_008_subscribe_frames_from_bindings() {
    let ctx = bound();
    let cfg = VenueCfg {
        env: VenueEnv::Prod,
        stale_guard_ms: 0,
    };
    let spec = |inst, stream| SubSpec {
        inst,
        stream,
        levels: 0,
        route: MARKET,
    };
    let subs = [
        spec(UP, StreamKind::DepthTopN),
        spec(UP, StreamKind::Trade),
        spec(DN, StreamKind::DepthTopN),
    ];
    let mut out = SubFrames::with_capacity(1 << 16, 8);
    assert_eq!(
        Pm::subscribe_ctx(rd(), &ctx, &subs, false, &cfg, &mut out),
        Ok(())
    );
    let want =
        format!("{{\"type\":\"market\",\"operation\":\"subscribe\",\"assets_ids\":[\"{T_UP}\",\"{T_DN}\"]}}");
    assert_eq!(
        out.get(0).map(|(t, n)| (t.to_vec(), n)),
        Some((want.into_bytes(), 3))
    );
    out.clear();
    assert_eq!(
        Pm::subscribe_ctx(rd(), &ctx, &subs[2..], true, &cfg, &mut out),
        Ok(())
    );
    let want = format!("{{\"type\":\"market\",\"operation\":\"unsubscribe\",\"assets_ids\":[\"{T_DN}\"]}}");
    assert_eq!(out.get(0).map(|(t, _)| t.to_vec()), Some(want.into_bytes()));
    // Unbound inst or an unsupported stream refuses the call.
    out.clear();
    let unbound = FeedCtx::with_capacity(4);
    assert_eq!(
        Pm::subscribe_ctx(rd(), &unbound, &subs, false, &cfg, &mut out),
        Err(ProtoErr::Cfg)
    );
    assert_eq!(
        Pm::subscribe_ctx(rd(), &ctx, &[spec(UP, StreamKind::Bbo)], false, &cfg, &mut out),
        Err(ProtoErr::Unsupported)
    );
    const { assert!(!Pm::SUB_ACKS) };
}

#[test]
fn pmk_009_binding_table_rules() {
    let mut c = FeedCtx::with_capacity(2);
    assert_eq!(c.bind(UP, b""), Err(BindErr::Token));
    assert_eq!(c.bind(UP, b"12a"), Err(BindErr::Token));
    assert_eq!(c.bind(UP, &[b'1'; 81]), Err(BindErr::Token));
    assert_eq!(c.bind(InstId(0), b"1"), Err(BindErr::Inst));
    assert_eq!(c.bind(UP, b"111"), Ok(()));
    assert_eq!(c.bind(DN, b"111"), Err(BindErr::TokenInUse));
    assert_eq!(c.bind(DN, b"222"), Ok(()));
    assert_eq!(c.bind(InstId(9), b"333"), Err(BindErr::Full));
    assert_eq!(c.inst_of(b"222"), DN);
    // Rebinding moves the token; the old one no longer resolves.
    assert_eq!(c.bind(DN, b"444"), Ok(()));
    assert_eq!(c.inst_of(b"222"), InstId(0));
    assert_eq!(c.token_of(DN), Some(&b"444"[..]));
    c.unbind(UP);
    assert_eq!(c.inst_of(b"111"), InstId(0));
    assert_eq!(c.token_of(UP), None);
}

#[test]
fn pmk_010_rebinding_keeps_the_chain_monotonic() {
    let mut ctx = bound();
    let _ = dec(&mut ctx, &book_up());
    let _ = dec(&mut ctx, &price_change());
    // The slot moves to the next window's token: ids continue, so the book never sees a reset id.
    let next = "11111111111111111111111111111111111111111111111111111111111111111111111111111";
    assert_eq!(ctx.bind(UP, next.as_bytes()), Ok(()));
    let (_, ev) = dec(&mut ctx, &book_up().replace(T_UP, next));
    assert!(matches!(ev.first(), Some(MdRec::L2Snapshot { hdr, .. }) if hdr.snap_id == 3));
}

/// Applies decoded events to per-inst price → size maps (bids, asks).
type Book = (
    std::collections::BTreeMap<i64, i64>,
    std::collections::BTreeMap<i64, i64>,
);

fn apply(books: &mut std::collections::HashMap<InstId, Book>, ev: &[MdRec], cur: &mut Option<(InstId, u64)>) {
    for e in ev {
        match e {
            MdRec::L2Snapshot { hdr, bids, asks, .. } => {
                let b = books.entry(hdr.h.inst).or_default();
                b.0 = bids.iter().map(|l| (l.px.0, l.qty.0)).collect();
                b.1 = asks.iter().map(|l| (l.px.0, l.qty.0)).collect();
                *cur = None;
            }
            MdRec::L2Begin(h) => *cur = Some((h.h.inst, h.prev_id)),
            MdRec::L2Level(side, px, qty) => {
                let Some((inst, _)) = *cur else { continue };
                let b = books.entry(inst).or_default();
                let m = if *side == Side::Buy { &mut b.0 } else { &mut b.1 };
                if qty.0 == 0 {
                    m.remove(&px.0);
                } else {
                    m.insert(px.0, qty.0);
                }
            }
            _ => {}
        }
    }
}

#[test]
fn pmk_011_recorded_window_matches_exchange_best_prices() {
    // 400 consecutive frames of btc-updown-5m (pmrec 2026-10-19, company eu-west-1 box).
    let text = include_str!("../../../fixtures/polymarket/market-btc-400.jsonl");
    let mut ctx = bound();
    let mut books = std::collections::HashMap::new();
    let (mut checked, mut frames) = (0usize, 0usize);
    let mut last_id = std::collections::HashMap::new();
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        frames += 1;
        let (r, ev) = dec(&mut ctx, line);
        assert!(r.is_ok(), "frame {frames}: {r:?}");
        // Every batch continues the inst's chain.
        for e in &ev {
            match e {
                MdRec::L2Snapshot { hdr, .. } => {
                    last_id.insert(hdr.h.inst, hdr.snap_id);
                }
                MdRec::L2Begin(h) => {
                    assert_eq!(
                        last_id.get(&h.h.inst).copied().unwrap_or(0),
                        h.prev_id,
                        "frame {frames}"
                    );
                    last_id.insert(h.h.inst, h.last_id);
                }
                _ => {}
            }
        }
        let mut cur = None;
        apply(&mut books, &ev, &mut cur);
        let v: serde_json::Value = serde_json::from_str(line).unwrap_or(serde_json::Value::Null);
        if v["event_type"] != "price_change" {
            continue;
        }
        for ch in v["price_changes"].as_array().into_iter().flatten() {
            let inst = if ch["asset_id"] == T_UP {
                UP
            } else if ch["asset_id"] == T_DN {
                DN
            } else {
                continue;
            };
            let Some(b) = books.get(&inst) else { continue };
            let px = |s: &serde_json::Value| {
                s.as_str()
                    .and_then(|x| hft_types::parse_exact(x.as_bytes(), -3).ok())
            };
            // Only once the token's book exists (the window may start mid-stream).
            if b.0.is_empty() && b.1.is_empty() {
                continue;
            }
            assert_eq!(
                b.0.keys().next_back().copied(),
                px(&ch["best_bid"]),
                "frame {frames} bid"
            );
            assert_eq!(
                b.1.keys().next().copied(),
                px(&ch["best_ask"]),
                "frame {frames} ask"
            );
            checked += 1;
        }
    }
    assert!(
        frames >= 390 && checked > 100,
        "frames {frames} checked {checked}"
    );
}
