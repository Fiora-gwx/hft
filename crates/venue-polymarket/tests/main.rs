//! venue-polymarket tests (TESTS.md). Frames are copied from the market channel as recorded on
//! 2026-10-19 (pmrec, eu-west-1); token ids shortened only where the case says so.
#![allow(clippy::disallowed_types)] // allow-list: test code, not a hot path

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
/// `btc-up-0` = 1 and `btc-dn-0` = 2 on `POLYMARKET_CLOB` (tick 0.01; lot 0.000001, `btc-dn-0` 0.01
/// as live).
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
        for (id, sym, asset, lot, lot_str) in [
            (UP, "btc-up-0", AssetId(2), 1, "0.000001"),
            (DN, "btc-dn-0", AssetId(3), 10_000, "0.01"),
        ] {
            b.inst(InstSpec {
                id,
                venue: V,
                symbol: sym.into(),
                status: InstStatus::Trading,
                kind: InstKind::Spot,
                asset,
                quote: AssetId(1),
                tick: 10,
                lot,
                min_qty: Qty(lot),
                max_qty: Qty(1_000_000_000_000),
                min_notional: Notional(1_000_000),
                book_window: 128,
                l2_depth: 100,
                crossed_tolerance: 0,
                tick_str: "0.01".into(),
                lot_str: lot_str.into(),
                min_qty_str: lot_str.into(),
                ..InstSpec::default()
            });
        }
        // Live account 1 with its venue UID = the first 24 characters of the test API key
        // (README "账户配置核验"; `orders.rs`).
        b.instance(hft_refdata::InstanceSpec {
            id: hft_types::StrategyId(1),
            kind_id: 7,
            name: "pm-test".into(),
        });
        b.account(hft_refdata::AccountSpec {
            id: AccountId(1),
            sim: false,
            bound_instance: hft_types::StrategyId(1),
            is_prod: true,
            venues: 1 << V.0,
            label: "pm-live".into(),
            uid_policy: hft_refdata::UidPolicy::Verified,
        });
        b.account_venue(hft_refdata::AccountVenueSpec {
            account: AccountId(1),
            venue: V,
            uid: "0f5e8d2a-1111-2222-3333-".into(),
        });
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

/// `dec` with the frame received on connection `conn`.
fn dec_on(ctx: &mut FeedCtx, p: &str, conn: u8) -> u16 {
    let mut s = VecSink::default();
    let c = Pm::classify(
        MARKET,
        MsgOp::Text,
        p.as_bytes().get(..64).unwrap_or(p.as_bytes()),
    );
    let rx = RxMeta {
        conn: ConnId(conn),
        ..RX
    };
    match Pm::decode_md(rd(), ctx, c, p.as_bytes(), &rx, &mut s) {
        Ok(o) => o.events,
        Err(e) => panic!("decode: {e:?}"),
    }
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
    assert_eq!(p("[]").map(|x| x.inst), Err(DecodeErr::UnknownClass));
    assert_eq!(p("[ ]\n").map(|x| x.inst), Err(DecodeErr::UnknownClass));
    assert_eq!(p("[[").map(|x| x.inst), Err(DecodeErr::Shape));
    assert_eq!(p("[").map(|x| x.inst), Err(DecodeErr::Shape));
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

#[test]
fn pmk_012_changes_older_than_the_book_are_dropped() {
    let mut ctx = bound();
    // book at 1792424113368; a change stamped 1 ms earlier arrives after it (resubscription).
    let _ = dec(&mut ctx, &book_up());
    let older = price_change()
        .replace("1792424114058", "1792424113367")
        .replace(T_DN, "99");
    let (r, ev) = dec(&mut ctx, &older);
    assert_eq!(r.map(|o| o.events), Ok(0));
    assert!(ev.is_empty());
    // Same timestamp as the book: applied.
    let same = price_change()
        .replace("1792424114058", "1792424113368")
        .replace(T_DN, "99");
    let (r, ev) = dec(&mut ctx, &same);
    assert_eq!(r.map(|o| o.events), Ok(1));
    assert!(matches!(ev.first(), Some(MdRec::L2Begin(h)) if h.prev_id == 1 && h.last_id == 2));
    // Rebinding to a new token forgets the book time.
    let next = "22222222222222222222222222222222222222222222222222222222222222222222222222222";
    assert_eq!(ctx.bind(UP, next.as_bytes()), Ok(()));
    let (r, _) = dec(&mut ctx, &older.replace(T_UP, next));
    assert_eq!(r.map(|o| o.events), Ok(1));
}

#[test]
fn pmk_013_wire_id_binds_the_decimal_token() {
    // T_UP as uint256, big-endian.
    const ID: [u8; 32] = [
        0xce, 0x2f, 0xbf, 0xfb, 0xbb, 0x97, 0xcf, 0xd6, 0xd8, 0x7a, 0x19, 0xd4, 0x4d, 0xea, 0xe1, 0x2f, 0x8f,
        0xd4, 0xad, 0xc3, 0xaa, 0xf7, 0x03, 0x9a, 0xe6, 0x5b, 0xa2, 0x88, 0x5a, 0x54, 0x8c, 0xb2,
    ];
    let mut ctx = FeedCtx::with_capacity(4);
    assert_eq!(Pm::bind_wire(&mut ctx, UP, &ID), Ok(()));
    assert_eq!(ctx.token_of(UP), Some(T_UP.as_bytes()));
    assert_eq!(ctx.inst_of(T_UP.as_bytes()), UP);
    let mut one = [0u8; 32];
    one[31] = 7;
    assert_eq!(Pm::bind_wire(&mut ctx, DN, &one), Ok(()));
    assert_eq!(ctx.token_of(DN), Some(&b"7"[..]));
    // All zero unbinds; inst 0 is refused.
    assert_eq!(Pm::bind_wire(&mut ctx, DN, &[0; 32]), Ok(()));
    assert_eq!(ctx.token_of(DN), None);
    assert_eq!(Pm::bind_wire(&mut ctx, InstId(0), &ID), Err(ProtoErr::Cfg));
}

#[test]
fn pmk_014_rebinding_unsubscribes_the_previous_token() {
    let cfg = VenueCfg {
        env: VenueEnv::Prod,
        stale_guard_ms: 0,
    };
    let spec = |inst| SubSpec {
        inst,
        stream: StreamKind::DepthTopN,
        levels: 0,
        route: MARKET,
    };
    let mut ctx = bound();
    let next = "33333333333333333333333333333333333333333333333333333333333333333333333333333";
    assert_eq!(ctx.bind(UP, next.as_bytes()), Ok(()));
    let mut out = SubFrames::with_capacity(1 << 16, 8);
    assert_eq!(
        Pm::subscribe_ctx(rd(), &ctx, &[spec(UP)], true, &cfg, &mut out),
        Ok(())
    );
    let want = format!("{{\"type\":\"market\",\"operation\":\"unsubscribe\",\"assets_ids\":[\"{T_UP}\"]}}");
    assert_eq!(out.get(0).map(|(t, _)| t.to_vec()), Some(want.into_bytes()));
    out.clear();
    assert_eq!(
        Pm::subscribe_ctx(rd(), &ctx, &[spec(UP)], false, &cfg, &mut out),
        Ok(())
    );
    let want = format!("{{\"type\":\"market\",\"operation\":\"subscribe\",\"assets_ids\":[\"{next}\"]}}");
    assert_eq!(out.get(0).map(|(t, _)| t.to_vec()), Some(want.into_bytes()));
    // An unbound inst is skipped when another one is bound.
    ctx.unbind(DN);
    out.clear();
    assert_eq!(
        Pm::subscribe_ctx(rd(), &ctx, &[spec(DN), spec(UP)], false, &cfg, &mut out),
        Ok(())
    );
    assert_eq!(out.get(0).map(|(_, n)| n), Some(2));
}

#[test]
fn pmk_015_peek_routes_by_the_first_bound_token() {
    // Only the Down token bound (the Up slot not yet / no longer bound).
    let mut ctx = FeedCtx::with_capacity(16);
    assert_eq!(ctx.bind(DN, T_DN.as_bytes()), Ok(()));
    let p =
        |ctx: &FeedCtx, s: &str| Pm::peek(rd(), ctx, MdClass::Push, s.as_bytes()).map(|x| (x.inst, x.stream));
    // .1 a price_change whose first change is the unbound token.
    assert_eq!(
        p(&ctx, &price_change()),
        Ok((DN, StreamKind::DepthTopN)),
        "PMK-015.1"
    );
    // .2 a list frame whose first book is the unbound token.
    let list = format!("[{},{}]", book_up(), book_up().replace(T_UP, T_DN));
    assert_eq!(p(&ctx, &list), Ok((DN, StreamKind::DepthTopN)), "PMK-015.2");
    // .3 nothing bound: the first token's (unbound) inst, as before.
    let none = FeedCtx::with_capacity(16);
    assert_eq!(
        p(&none, &price_change()),
        Ok((InstId(0), StreamKind::DepthTopN)),
        "PMK-015.3"
    );
    // .4 decoding the .1 message still writes only the bound token's batch.
    let (r, ev) = dec(&mut ctx, &price_change());
    assert_eq!(r.map(|o| o.events), Ok(1), "PMK-015.4");
    assert!(
        matches!(ev.first(), Some(MdRec::L2Begin(h)) if h.h.inst == DN),
        "PMK-015.4"
    );
}

#[test]
fn pmk_016_copies_from_another_connection_are_dropped() {
    let mut ctx = bound();
    // .1 the same book on connections 3 and 4.
    assert_eq!(dec_on(&mut ctx, &book_up(), 3), 1);
    assert_eq!(dec_on(&mut ctx, &book_up(), 4), 0);
    assert_eq!(ctx.dups(), 1);
    // .2 the same price_change, connection 4 first.
    assert_eq!(dec_on(&mut ctx, &price_change(), 4), 2);
    assert_eq!(dec_on(&mut ctx, &price_change(), 3), 0);
    // .3 two identical fills on connection 3 are two trades; connection 4's copies are not,
    // until it delivers the message a third time.
    let t = last_trade();
    assert_eq!(dec_on(&mut ctx, &t, 3), 1);
    assert_eq!(dec_on(&mut ctx, &t, 3), 1);
    assert_eq!(dec_on(&mut ctx, &t, 4), 0);
    assert_eq!(dec_on(&mut ctx, &t, 4), 0);
    assert_eq!(dec_on(&mut ctx, &t, 4), 1);
    assert_eq!(ctx.dups(), 4);
    // .5 same timestamp and hashes, different content (seen in recordings): not a copy.
    let mut two = bound();
    assert_eq!(dec_on(&mut two, &price_change(), 3), 2);
    let other = price_change().replace("\"0.42\"", "\"0.41\"");
    assert_eq!(dec_on(&mut two, &other, 4), 2);
    assert_eq!(two.dups(), 0);
    // .4 one connection alone: a repeated message is applied each time.
    let mut one = bound();
    assert_eq!(dec_on(&mut one, &price_change(), 3), 2);
    assert_eq!(dec_on(&mut one, &price_change(), 3), 2);
    assert_eq!(one.dups(), 0);
}

#[test]
fn pmk_017_late_books_and_messages_are_dropped() {
    let mut ctx = bound();
    let at = |ms: &str, hash: &str| {
        price_change()
            .replace("1792424114058", ms)
            .replace("d96398aa668ede9a7247a2cc71296c96c924c4ee", hash)
            .replace(T_DN, "99")
    };
    assert_eq!(dec_on(&mut ctx, &book_up(), 3), 1);
    assert_eq!(dec_on(&mut ctx, &at("1792424118000", "a1"), 3), 1);
    // .1 a book older than data already applied (another connection resubscribed).
    let old_book = book_up()
        .replace("1792424113368", "1792424117999")
        .replace("40d4258e2ed98c21699147377fa43ffe35665bd7", "b1");
    assert_eq!(dec_on(&mut ctx, &old_book, 4), 0);
    assert_eq!(ctx.lates(), 1);
    // .2 a change more than 2000 ms before the latest applied time is late; within it, applied.
    assert_eq!(dec_on(&mut ctx, &at("1792424115500", "a2"), 4), 0);
    assert_eq!(ctx.lates(), 2);
    assert_eq!(dec_on(&mut ctx, &at("1792424116500", "a3"), 4), 1);
    // .3 a trade more than 2000 ms late.
    let late_trade = last_trade().replace("1792424441146", "1792424115000").replace(
        "0xfef1f6c5dd0f86a783f8056e1e7249d063715fe18bc379040a5236a59490a21a",
        "0x1",
    );
    assert_eq!(dec_on(&mut ctx, &late_trade, 3), 0);
    assert_eq!(ctx.lates(), 3);
    // .4 rebinding forgets the applied time: the old book of the new token applies.
    let next = "44444444444444444444444444444444444444444444444444444444444444444444444444444";
    assert_eq!(ctx.bind(UP, next.as_bytes()), Ok(()));
    assert_eq!(dec_on(&mut ctx, &old_book.replace(T_UP, next), 4), 1);
}

mod orders;

mod sign_v2 {
    use venue_polymarket::sign::{
        EXCHANGE_V2, OrderV2, Wallet, auth_digest, checksum_address, exchange_domain, hex_signature, l2_hmac,
        order_digest, parse_u256,
    };

    fn golden() -> serde_json::Value {
        serde_json::from_str(include_str!("sign_v2_golden.json")).unwrap()
    }

    fn wallet(g: &serde_json::Value) -> Wallet {
        Wallet::from_hex(g["key"].as_str().unwrap()).unwrap()
    }

    #[test]
    fn pmk_018_order_signature_matches_the_official_client() {
        let g = golden();
        let w = wallet(&g);
        let mut a = [0u8; 42];
        assert_eq!(
            checksum_address(&w.address(), &mut a),
            g["address"].as_str().unwrap(),
            "PMK-018.1"
        );
        let domain = exchange_domain("2", 137, &EXCHANGE_V2);
        for (k, o) in g["orders"].as_array().unwrap().iter().enumerate() {
            let order = OrderV2 {
                salt: o["salt"].as_str().unwrap().parse().unwrap(),
                maker: w.address(),
                signer: w.address(),
                token_id: parse_u256(o["tokenId"].as_str().unwrap()).unwrap(),
                maker_amount: o["makerAmount"].as_str().unwrap().parse().unwrap(),
                taker_amount: o["takerAmount"].as_str().unwrap().parse().unwrap(),
                side: o["side"].as_u64().unwrap() as u8,
                signature_type: 0,
                timestamp_ms: o["timestamp"].as_str().unwrap().parse().unwrap(),
                metadata: [0; 32],
                builder: [0; 32],
            };
            let sig = w.sign(&order_digest(&domain, &order));
            let mut buf = [0u8; 132];
            assert_eq!(
                hex_signature(&sig, &mut buf),
                o["signature"].as_str().unwrap(),
                "PMK-018.2 order {k}"
            );
        }
    }

    #[test]
    fn pmk_019_l1_auth_signature_matches_the_official_client() {
        let g = golden();
        let w = wallet(&g);
        for a in g["auth"].as_array().unwrap() {
            let d = auth_digest(
                137,
                &w.address(),
                a["timestamp"].as_u64().unwrap(),
                a["nonce"].as_u64().unwrap(),
            );
            let mut buf = [0u8; 132];
            assert_eq!(
                hex_signature(&w.sign(&d), &mut buf),
                a["signature"].as_str().unwrap(),
                "PMK-019"
            );
        }
    }

    #[test]
    fn pmk_020_l2_hmac_matches_the_official_client() {
        let g = golden();
        for h in g["hmac"].as_array().unwrap() {
            let mut out = [0u8; 44];
            let s = l2_hmac(
                h["secret"].as_str().unwrap(),
                h["timestamp"].as_str().unwrap(),
                h["method"].as_str().unwrap(),
                h["path"].as_str().unwrap(),
                h["body"].as_str().unwrap_or(""),
                &mut out,
            )
            .unwrap();
            assert_eq!(s, h["signature"].as_str().unwrap(), "PMK-020");
        }
    }

    #[test]
    fn pmk_021_inputs_are_validated() {
        // .1 keys: wrong length, non-hex, zero scalar.
        assert!(Wallet::from_hex("0x1234").is_err(), "PMK-021.1");
        assert!(
            Wallet::from_hex(&format!("0x{}", "zz".repeat(32))).is_err(),
            "PMK-021.1"
        );
        assert!(
            Wallet::from_hex(&format!("0x{}", "00".repeat(32))).is_err(),
            "PMK-021.1"
        );
        // .2 uint256 token ids: decimal only, at most 2^256 - 1.
        assert_eq!(parse_u256("255").map(|b| b[31]), Some(255), "PMK-021.2");
        assert_eq!(
            parse_u256("256").map(|b| (b[30], b[31])),
            Some((1, 0)),
            "PMK-021.2"
        );
        let max = "115792089237316195423570985008687907853269984665640564039457584007913129639935";
        assert_eq!(parse_u256(max), Some([0xff; 32]), "PMK-021.2");
        assert!(
            parse_u256("115792089237316195423570985008687907853269984665640564039457584007913129639936")
                .is_none(),
            "PMK-021.2"
        );
        assert!(
            parse_u256("12a").is_none() && parse_u256("").is_none(),
            "PMK-021.2"
        );
        // .3 a secret that is not URL-safe base64.
        let mut out = [0u8; 44];
        assert!(
            l2_hmac("not base64!", "1", "GET", "/", "", &mut out).is_err(),
            "PMK-021.3"
        );
    }
}

mod sign_v2_1271 {
    use venue_polymarket::sign::{
        EXCHANGE_V2, OrderV2, Wallet, exchange_domain, hex_bytes, parse_u256, sign_order_1271,
    };

    fn addr(s: &str) -> [u8; 20] {
        let mut a = [0u8; 20];
        for (i, c) in a.iter_mut().enumerate() {
            *c = u8::from_str_radix(&s[2 + 2 * i..4 + 2 * i], 16).unwrap();
        }
        a
    }

    #[test]
    fn pmk_022_deposit_wallet_signature_matches_the_official_client() {
        let g: serde_json::Value = serde_json::from_str(include_str!("sign_v2_golden.json")).unwrap();
        let w = Wallet::from_hex(g["key"].as_str().unwrap()).unwrap();
        let domain = exchange_domain("2", 137, &EXCHANGE_V2);
        for (k, o) in g["orders_1271"].as_array().unwrap().iter().enumerate() {
            let order = OrderV2 {
                salt: o["salt"].as_str().unwrap().parse().unwrap(),
                maker: addr(o["maker"].as_str().unwrap()),
                signer: addr(o["signer"].as_str().unwrap()),
                token_id: parse_u256(o["tokenId"].as_str().unwrap()).unwrap(),
                maker_amount: o["makerAmount"].as_str().unwrap().parse().unwrap(),
                taker_amount: o["takerAmount"].as_str().unwrap().parse().unwrap(),
                side: o["side"].as_u64().unwrap() as u8,
                signature_type: 3,
                timestamp_ms: o["timestamp"].as_str().unwrap().parse().unwrap(),
                metadata: [0; 32],
                builder: [0; 32],
            };
            let mut sig = [0u8; 512];
            let n = sign_order_1271(&w, &domain, 137, &order, &mut sig);
            let mut hex = [0u8; 1026];
            assert_eq!(
                hex_bytes(&sig[..n], &mut hex).unwrap(),
                o["signature"].as_str().unwrap(),
                "PMK-022 order {k}"
            );
        }
    }
}
