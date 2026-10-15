//! BGT-020–BGT-037: market-data classes, `peek` and decoding.

use crate::support;

use hft_event::{Bbo, L2SnapHdr, Level, MarketHdr};
use hft_types::{InstId, Px, Qty, Rate, Side};
use hft_venue_proto::testkit::{MdRec, VecSink};
use hft_venue_proto::{
    Align, ControlHint, DecodeErr, MdOutcome, RawSeq, ResyncMethod, RouteId, SeqIds, SeqRule, StreamKind,
    VenueProtocol,
};
use support::{F_BTC, F_ETH, RX, b1v2, b1v3, rd};
use venue_bitget::{BitgetSpot, BitgetUsdtFutures as Fut, FeedCtx, MdClass};

/// A table row: case id, payload, expectation.
type Row<T> = (&'static str, String, T);
/// Bid px/qty, ask px/qty.
type Sides = (Px, Qty, Px, Qty);

const BOOKS1: RouteId = RouteId(0);
const TOPN: RouteId = RouteId(1);
const TRADE: RouteId = RouteId(2);
const TICKER: RouteId = RouteId(3);
const BOOKS: RouteId = RouteId(4);

fn dec(c: MdClass, p: &str) -> (Result<MdOutcome, DecodeErr>, Vec<MdRec>) {
    let mut ctx = FeedCtx::new();
    dec_ctx(&mut ctx, c, p)
}

fn dec_ctx(ctx: &mut FeedCtx, c: MdClass, p: &str) -> (Result<MdOutcome, DecodeErr>, Vec<MdRec>) {
    let mut s = VecSink::default();
    let r = Fut::decode_md(rd(), ctx, c, p.as_bytes(), &RX, &mut s);
    (r, s.md)
}

fn hdr(inst: InstId, ex: i64, send: i64) -> MarketHdr {
    MarketHdr {
        venue: support::FUT,
        conn: RX.conn,
        _pad: 0,
        inst,
        ex_ts_ns: ex,
        ex_send_ns: send,
        hw_rx_ns: RX.hw_rx_ns,
        rx_tsc: RX.rx_tsc,
        sw_rx_ns: RX.sw_rx_ns,
        rx_empty_gap_ns: RX.rx_empty_gap_ns,
        _rsv: 0,
    }
}

fn bbo_024() -> Bbo {
    Bbo {
        h: hdr(F_BTC, 1_790_000_000_123_000_000, 1_790_000_000_125_000_000),
        bid_px: Px(6_500_010),
        bid_qty: Qty(150_000_000),
        ask_px: Px(6_500_020),
        ask_qty: Qty(400_000),
        update_id: 123_456_789,
    }
}

#[test]
fn bgt_020_seq_rule_resync() {
    let rows = [
        (
            "BGT-020.1",
            MdClass::Books1,
            SeqRule::Snapshot,
            ResyncMethod::None,
        ),
        (
            "BGT-020.2",
            MdClass::BooksTopN,
            SeqRule::Snapshot,
            ResyncMethod::None,
        ),
        (
            "BGT-020.3",
            MdClass::Trade,
            SeqRule::Monotonic,
            ResyncMethod::None,
        ),
        ("BGT-020.4", MdClass::Ticker, SeqRule::ByTime, ResyncMethod::None),
        ("BGT-020.5", MdClass::SubAck, SeqRule::None, ResyncMethod::None),
        ("BGT-020.5", MdClass::Pong, SeqRule::None, ResyncMethod::None),
        ("BGT-020.5", MdClass::Unknown, SeqRule::None, ResyncMethod::None),
        (
            "BGT-020.6",
            MdClass::BooksIncr,
            SeqRule::Chain,
            ResyncMethod::Resubscribe,
        ),
    ];
    for (row, c, rule, rs) in rows {
        assert_eq!(Fut::seq_rule(c), rule, "{row}");
        assert_eq!(Fut::resync_method(c), rs, "{row}");
        assert_eq!(BitgetSpot::seq_rule(c), rule, "{row}");
        assert_eq!(BitgetSpot::resync_method(c), rs, "{row}");
    }
    // Incremental books: the snapshot's seq must fall within [pseq, seq] of the first update.
    let raw = |prev: u64, last: u64| RawSeq {
        first: prev,
        last,
        prev,
        has_prev: true,
        _pad: [0; 7],
    };
    assert_eq!(Fut::align_first(100, &raw(99, 101)), Align::Apply, "BGT-020.7");
    assert_eq!(Fut::align_first(100, &raw(100, 105)), Align::Apply, "BGT-020.7");
    assert_eq!(Fut::align_first(100, &raw(90, 100)), Align::Drop, "BGT-020.7");
    assert_eq!(Fut::align_first(100, &raw(101, 105)), Align::Gap, "BGT-020.7");
}

#[test]
fn bgt_021_peek_books1() {
    let ctx = FeedCtx::new();
    for p in [
        b1v2(123_456_789, 1_790_000_000_123),
        b1v3("123456789", "\"1790000000123\"", "1790000000125"),
    ] {
        let k = Fut::peek(rd(), &ctx, MdClass::Books1, p.as_bytes()).unwrap();
        assert_eq!(k.inst, F_BTC);
        assert_eq!(k.stream, StreamKind::Bbo);
        assert_eq!(
            k.ids,
            SeqIds {
                prev_id: 0,
                last_id: 123_456_789
            }
        );
    }
}

#[test]
fn bgt_022_peek_unknown_symbol() {
    let p = b1v2(5, 1_790_000_000_123).replace("BTCUSDT", "DOGEUSDT");
    let k = Fut::peek(rd(), &FeedCtx::new(), MdClass::Books1, p.as_bytes()).unwrap();
    assert_eq!(k.inst, InstId(0));
}

#[test]
fn bgt_023_peek_skips_prices() {
    let p = b1v2(5, 1_790_000_000_123).replace("\"65000.1\"", "\"abc\"");
    let k = Fut::peek(rd(), &FeedCtx::new(), MdClass::Books1, p.as_bytes()).unwrap();
    assert_eq!((k.inst, k.ids.last_id), (F_BTC, 5));
    let (r, md) = dec(MdClass::Books1, &p);
    assert_eq!(r, Err(DecodeErr::Number));
    assert!(md.is_empty());
}

#[test]
fn bgt_024_books1_to_bbo() {
    for p in [
        b1v2(123_456_789, 1_790_000_000_123),
        b1v3("123456789", "\"1790000000123\"", "1790000000125"),
    ] {
        let (r, md) = dec(MdClass::Books1, &p);
        assert_eq!(
            r,
            Ok(MdOutcome {
                events: 1,
                inst: F_BTC
            })
        );
        assert_eq!(md, vec![MdRec::Bbo(bbo_024())]);
    }
}

#[test]
fn bgt_025_number_string_forms() {
    let base = |seq: &str, ts: &str, top: &str| b1v3(seq, ts, top);
    let want = vec![MdRec::Bbo(bbo_024())];
    let rows: Vec<Row<Result<Vec<MdRec>, DecodeErr>>> = vec![
        (
            "BGT-025.1",
            base("\"123456789\"", "\"1790000000123\"", "1790000000125"),
            Ok(want.clone()),
        ),
        (
            "BGT-025.2",
            base("123456789", "1790000000123", "1790000000125"),
            Ok(want.clone()),
        ),
        (
            "BGT-025.3",
            base("123456789", "\"1790000000123\"", "\"1790000000125\""),
            Ok(want.clone()),
        ),
        (
            "BGT-025.4",
            base("\"12a\"", "\"1790000000123\"", "1790000000125"),
            Err(DecodeErr::Number),
        ),
        (
            "BGT-025.5",
            base("-1", "\"1790000000123\"", "1790000000125"),
            Err(DecodeErr::Number),
        ),
        (
            "BGT-025.6",
            base("\"18446744073709551616\"", "\"1790000000123\"", "1790000000125"),
            Err(DecodeErr::Overflow),
        ),
        (
            "BGT-025.7",
            base("123456789", "\"1790000000123.5\"", "1790000000125"),
            Err(DecodeErr::Number),
        ),
    ];
    for (row, p, want) in rows {
        let (r, md) = dec(MdClass::Books1, &p);
        match want {
            Ok(w) => {
                assert!(r.is_ok(), "{row}: {r:?}");
                assert_eq!(md, w, "{row}");
            }
            Err(e) => {
                assert_eq!(r, Err(e), "{row}");
                assert!(md.is_empty(), "{row}");
            }
        }
    }
}

#[test]
fn bgt_027_books1_errors() {
    let p = b1v3("123456789", "\"1790000000123\"", "1790000000125");
    let sub = |from: &str, to: &str| p.replacen(from, to, 1);
    let rows: Vec<Row<Result<Sides, DecodeErr>>> = vec![
        (
            "BGT-027.1",
            sub(r#""b":[["65000.1","1.5"]],"#, ""),
            Err(DecodeErr::Shape),
        ),
        (
            "BGT-027.2",
            sub(r#""b":[["65000.1","1.5"]]"#, r#""b":[]"#),
            Ok((Px(0), Qty(0), Px(6_500_020), Qty(400_000))),
        ),
        (
            "BGT-027.3",
            sub(r#""a":[["65000.2","0.004"]]"#, r#""a":[]"#),
            Ok((Px(6_500_010), Qty(150_000_000), Px(0), Qty(0))),
        ),
        (
            "BGT-027.4",
            p[..p.find("[[\"65000.1").unwrap() + 8].to_string(),
            Err(DecodeErr::Truncated),
        ),
        ("BGT-027.5", sub("\"65000.1\"", "\"\""), Err(DecodeErr::Number)),
        (
            "BGT-027.6",
            sub("\"65000.1\"", "\"65000.123\""),
            Err(DecodeErr::Number),
        ),
        (
            "BGT-027.7",
            sub("\"65000.1\"", "\"65000.10000\""),
            Ok((Px(6_500_010), Qty(150_000_000), Px(6_500_020), Qty(400_000))),
        ),
        (
            "BGT-027.8",
            sub("\"65000.1\"", "\"6.50001E4\""),
            Ok((Px(6_500_010), Qty(150_000_000), Px(6_500_020), Qty(400_000))),
        ),
        (
            "BGT-027.9",
            sub("\"1.5\"", "\"100000000000000\""),
            Err(DecodeErr::Overflow),
        ),
        (
            "BGT-027.10",
            sub(r#"["65000.1","1.5"]"#, r#"["65000.1"]"#),
            Err(DecodeErr::Shape),
        ),
        (
            "BGT-027.11",
            r#"{"data":[],"arg":{"instType":"usdt-futures","symbol":"BTCUSDT","topic":"books1"},"ts":1}"#
                .to_string(),
            Err(DecodeErr::Shape),
        ),
    ];
    for (row, p, want) in rows {
        let (r, md) = dec(MdClass::Books1, &p);
        match want {
            Ok((bp, bq, ap, aq)) => {
                assert!(r.is_ok(), "{row}: {r:?}");
                let MdRec::Bbo(b) = md[0] else { panic!("{row}") };
                assert_eq!(
                    (b.bid_px, b.bid_qty, b.ask_px, b.ask_qty),
                    (bp, bq, ap, aq),
                    "{row}"
                );
            }
            Err(e) => {
                assert_eq!(r, Err(e), "{row}");
                assert!(md.is_empty(), "{row}");
            }
        }
    }
}

fn topn(topic: &str, bids: &str, asks: &str, seq: u64) -> String {
    format!(
        r#"{{"data":[{{"a":{asks},"b":{bids},"seq":{seq},"ts":"1790000000200"}}],"arg":{{"instType":"usdt-futures","symbol":"BTCUSDT","topic":"{topic}"}},"action":"snapshot","ts":1790000000202}}"#
    )
}

const B5: &str = r#"[["65000.1","1"],["65000","2"],["64999.9","3"],["64999.8","4"],["64999.7","5"]]"#;
const A5: &str = r#"[["65000.2","1"],["65000.3","2"],["65000.4","3"],["65000.5","4"],["65000.6","5"]]"#;

#[test]
fn bgt_028_books5_topn() {
    let (r, md) = dec(MdClass::BooksTopN, &topn("books5", B5, A5, 777));
    assert_eq!(
        r,
        Ok(MdOutcome {
            events: 1,
            inst: F_BTC
        })
    );
    assert_eq!(md.len(), 1);
    let MdRec::L2Snapshot {
        hdr: h,
        bids,
        asks,
        part,
        parts,
    } = &md[0]
    else {
        panic!("not a snapshot: {md:?}")
    };
    assert_eq!(
        *h,
        L2SnapHdr {
            h: hdr(F_BTC, 1_790_000_000_200_000_000, 1_790_000_000_202_000_000),
            snap_id: 777,
            origin: L2SnapHdr::TOP_N,
            _pad: [0; 3],
            n_bids: 5,
            n_asks: 5,
        }
    );
    let lv = |px: i64, q: i64| Level {
        px: Px(px),
        qty: Qty(q * 100_000_000),
    };
    assert_eq!(
        bids,
        &vec![
            lv(6_500_010, 1),
            lv(6_500_000, 2),
            lv(6_499_990, 3),
            lv(6_499_980, 4),
            lv(6_499_970, 5)
        ]
    );
    assert_eq!(
        asks,
        &vec![
            lv(6_500_020, 1),
            lv(6_500_030, 2),
            lv(6_500_040, 3),
            lv(6_500_050, 4),
            lv(6_500_060, 5)
        ]
    );
    assert_eq!((*part, *parts), (0, 1));
}

#[test]
fn bgt_029_books50_short_and_zero() {
    let bids = r#"[["65000.1","1"],["65000","0"],["64999.9","3"]]"#;
    let asks: Vec<String> = (0..50).map(|i| format!(r#"["{}.2","1"]"#, 65_000 + i)).collect();
    let asks = format!("[{}]", asks.join(","));
    let (r, md) = dec(MdClass::BooksTopN, &topn("books50", bids, &asks, 900));
    assert!(r.is_ok());
    let MdRec::L2Snapshot { hdr: h, bids, .. } = &md[0] else {
        panic!()
    };
    assert_eq!(
        (h.snap_id, h.origin, h.n_bids, h.n_asks),
        (900, L2SnapHdr::TOP_N, 2, 50)
    );
    assert!(bids.iter().all(|l| l.qty.0 != 0));
}

#[test]
fn bgt_030_topn_same_seq() {
    let p = topn("books5", B5, A5, 777);
    let mut ctx = FeedCtx::new();
    let k1 = Fut::peek(rd(), &ctx, MdClass::BooksTopN, p.as_bytes()).unwrap();
    let k2 = Fut::peek(rd(), &ctx, MdClass::BooksTopN, p.as_bytes()).unwrap();
    assert_eq!(
        (k1.ids.last_id, k2.ids.last_id, k1.stream),
        (777, 777, StreamKind::DepthTopN)
    );
    let (_, a) = dec_ctx(&mut ctx, MdClass::BooksTopN, &p);
    let (_, b) = dec_ctx(&mut ctx, MdClass::BooksTopN, &p);
    assert_eq!(a, b);
}

fn trades(action: &str, rows: &[String]) -> String {
    format!(
        r#"{{"data":[{}],"arg":{{"instType":"usdt-futures","symbol":"BTCUSDT","topic":"publicTrade"}},"action":"{action}","ts":1790000000102}}"#,
        rows.join(",")
    )
}

fn trade_row(id: &str, side: &str) -> String {
    format!(
        r#"{{"p":"65000.1","S":"{side}","T":"1790000000100","v":"0.002","i":"{id}","L":"1","isRPI":"no"}}"#
    )
}

#[test]
fn bgt_031_trade_snapshot_dropped() {
    let hist: Vec<String> = (0..50)
        .map(|i| trade_row(&format!("{}", 900 + i), "buy"))
        .collect();
    let (r, md) = dec(MdClass::Trade, &trades("snapshot", &hist));
    assert_eq!(
        r,
        Ok(MdOutcome {
            events: 0,
            inst: F_BTC
        })
    );
    assert!(md.is_empty());
    let live = [
        trade_row("1000000000000000001", "buy"),
        trade_row("1000000000000000002", "sell"),
    ];
    let (r, md) = dec(MdClass::Trade, &trades("update", &live));
    assert_eq!(r.unwrap().events, 2);
    let ids: Vec<u64> = md
        .iter()
        .map(|m| match m {
            MdRec::Trade(t) => t.trade_id,
            _ => 0,
        })
        .collect();
    assert_eq!(ids, vec![1_000_000_000_000_000_001, 1_000_000_000_000_000_002]);
    // peek: the largest id of the push.
    let k = Fut::peek(
        rd(),
        &FeedCtx::new(),
        MdClass::Trade,
        trades("update", &live).as_bytes(),
    )
    .unwrap();
    assert_eq!(
        (k.inst, k.stream, k.ids.last_id),
        (F_BTC, StreamKind::Trade, 1_000_000_000_000_000_002)
    );
}

#[test]
fn bgt_032_trade_fields() {
    let one = |row: String| dec(MdClass::Trade, &trades("update", &[row]));
    let (r, md) = one(trade_row("1234567890123456789", "buy"));
    assert!(r.is_ok());
    let MdRec::Trade(t) = md[0] else { panic!() };
    assert_eq!(
        t.h,
        hdr(F_BTC, 1_790_000_000_100_000_000, 1_790_000_000_102_000_000),
        "BGT-032.1"
    );
    assert_eq!(
        (t.px, t.qty, t.trade_id, t.taker_side),
        (Px(6_500_010), Qty(200_000), 1_234_567_890_123_456_789, Side::Buy),
        "BGT-032.1"
    );
    let (_, md) = one(trade_row("1", "sell"));
    assert!(
        matches!(md[0], MdRec::Trade(t) if t.taker_side == Side::Sell),
        "BGT-032.2"
    );
    assert_eq!(one(trade_row("1", "hold")).0, Err(DecodeErr::Shape), "BGT-032.3");
    assert_eq!(
        one(trade_row("abc", "buy")).0,
        Err(DecodeErr::Number),
        "BGT-032.4"
    );
    assert_eq!(
        one(trade_row("18446744073709551616", "buy")).0,
        Err(DecodeErr::Overflow),
        "BGT-032.5"
    );
    let no_id = trade_row("1", "buy").replace(r#","i":"1""#, "");
    assert_eq!(one(no_id).0, Err(DecodeErr::Shape), "BGT-032.6");
    // Classic V2 field names decode the same.
    let v2 = r#"{"action":"update","arg":{"instType":"USDT-FUTURES","channel":"trade","instId":"BTCUSDT"},"data":[{"ts":"1790000000100","price":"65000.1","size":"0.002","side":"buy","tradeId":"1234567890123456789"}],"ts":1790000000102}"#;
    let (r, md2) = dec(MdClass::Trade, v2);
    assert!(r.is_ok());
    assert_eq!(
        md2,
        md_first(one(trade_row("1234567890123456789", "buy")).1),
        "BGT-032.7"
    );
}

fn md_first(v: Vec<MdRec>) -> Vec<MdRec> {
    v.into_iter().take(1).collect()
}

fn ticker(sym: &str, next: u64) -> String {
    format!(
        r#"{{"data":[{{"lastPrice":"65000","markPrice":"65001.2","indexPrice":"64990.5","fundingRate":"0.0001","nextFundingTime":"{next}","openInterest":"12.5","bid1Price":"65000","ask1Price":"65000.1"}}],"arg":{{"instType":"usdt-futures","symbol":"{sym}","topic":"ticker"}},"action":"snapshot","ts":1790000000301}}"#
    )
}

#[test]
fn bgt_053_snapshot_isolation() {
    let mut used = FeedCtx::new();
    for n in 0..10_000 {
        dec_ctx(
            &mut used,
            MdClass::Books1,
            &b1v3("123456789", "1790000000123", "1790000000125"),
        )
        .0
        .unwrap();
        dec_ctx(
            &mut used,
            MdClass::Ticker,
            &ticker("BTCUSDT", 1_790_006_400_000 + n * 28_800_000),
        )
        .0
        .unwrap();
    }
    let before = used.clone();
    let mut fresh = FeedCtx::new();
    let body = include_bytes!("../../../fixtures/bitget-usdt-futures/rest.orderbook/public_bbo.in");
    let mut a = VecSink::default();
    let mut b = VecSink::default();
    Fut::decode_bbo_snapshot(rd(), &mut fresh, &[F_BTC], body, &RX, &mut a).unwrap();
    Fut::decode_bbo_snapshot(rd(), &mut used, &[F_BTC], body, &RX, &mut b).unwrap();
    assert_eq!(
        a.md.iter().map(MdRec::bytes).collect::<Vec<_>>(),
        b.md.iter().map(MdRec::bytes).collect::<Vec<_>>()
    );
    assert_eq!(used, before);
    let mut control = before;
    for n in 0..5 {
        let payload = ticker("BTCUSDT", 1_790_006_400_000 + (10_000 + n) * 28_800_000);
        let (_, a) = dec_ctx(&mut control, MdClass::Ticker, &payload);
        let (_, b) = dec_ctx(&mut used, MdClass::Ticker, &payload);
        assert_eq!(
            a.iter().map(MdRec::bytes).collect::<Vec<_>>(),
            b.iter().map(MdRec::bytes).collect::<Vec<_>>()
        );
    }
}

#[test]
fn bgt_033_ticker_events() {
    let p = ticker("BTCUSDT", 1_790_006_400_000);
    let (r, md) = dec(MdClass::Ticker, &p);
    assert_eq!(
        r,
        Ok(MdOutcome {
            events: 3,
            inst: F_BTC
        })
    );
    // UTA ticker data carries no `ts`: the push time stands for both.
    let h = hdr(F_BTC, 1_790_000_000_301_000_000, 1_790_000_000_301_000_000);
    assert_eq!(md[0], MdRec::Mark(hft_event::MarkPrice { h, px: Px(6_500_120) }));
    assert_eq!(
        md[1],
        MdRec::Index(hft_event::IndexPrice { h, px: Px(6_499_050) })
    );
    assert_eq!(
        md[2],
        MdRec::Funding(hft_event::Funding {
            h,
            rate: Rate(100_000),
            next_funding_ns: 1_790_006_400_000_000_000,
            interval_s: 0,
            _pad: 0,
        })
    );
    let k = Fut::peek(rd(), &FeedCtx::new(), MdClass::Ticker, p.as_bytes()).unwrap();
    assert_eq!((k.inst, k.ids.last_id), (F_BTC, 1_790_000_000_301_000_000));
}

#[test]
fn bgt_034_funding_interval() {
    const A: u64 = 1_790_006_400_000;
    let rows = [
        ("BGT-034.1", "BTCUSDT", A, 0),
        ("BGT-034.2", "BTCUSDT", A, 0),
        ("BGT-034.3", "BTCUSDT", A + 28_800_000, 28_800),
        ("BGT-034.4", "BTCUSDT", A + 28_800_000, 28_800),
        ("BGT-034.5", "BTCUSDT", A + 43_200_000, 14_400),
        ("BGT-034.6", "ETHUSDT", A, 0),
        ("BGT-034.7", "BTCUSDT", A, 0),
    ];
    let mut ctx = FeedCtx::new();
    for (row, sym, next, want) in rows {
        let (r, md) = dec_ctx(&mut ctx, MdClass::Ticker, &ticker(sym, next));
        assert!(r.is_ok(), "{row}");
        let MdRec::Funding(f) = md[2] else { panic!("{row}") };
        assert_eq!(f.interval_s, want, "{row}");
        assert_eq!(f.h.inst, if sym == "ETHUSDT" { F_ETH } else { F_BTC }, "{row}");
    }
}

#[test]
fn bgt_035_rxmeta_passthrough() {
    let mut ctx = FeedCtx::new();
    let samples = [
        (MdClass::Books1, b1v3("1", "\"1790000000123\"", "1790000000125")),
        (MdClass::BooksTopN, topn("books5", B5, A5, 1)),
        (MdClass::Trade, trades("update", &[trade_row("1", "buy")])),
        (MdClass::Ticker, ticker("BTCUSDT", 1_790_006_400_000)),
    ];
    for (c, p) in samples {
        let (r, md) = dec_ctx(&mut ctx, c, &p);
        assert!(r.is_ok());
        for m in &md {
            let h = m.market_hdr().unwrap();
            assert_eq!(
                (
                    h.venue,
                    h.conn,
                    h.hw_rx_ns,
                    h.rx_tsc,
                    h.sw_rx_ns,
                    h.rx_empty_gap_ns
                ),
                (
                    support::FUT,
                    RX.conn,
                    RX.hw_rx_ns,
                    RX.rx_tsc,
                    RX.sw_rx_ns,
                    RX.rx_empty_gap_ns
                ),
                "{c:?}"
            );
        }
    }
    // BGT-035.5: spot books1.
    let p = b1v3("1", "\"1790000000123\"", "1790000000125").replace("usdt-futures", "spot");
    let mut s = VecSink::default();
    let r = BitgetSpot::decode_md(rd(), &mut ctx, MdClass::Books1, p.as_bytes(), &RX, &mut s);
    assert_eq!(r.unwrap().inst, support::S_BTC);
    assert_eq!(s.md[0].market_hdr().unwrap().venue, support::SPOT);
}

fn books(action: &str, pseq: u64, seq: u64, bids: &str, asks: &str) -> String {
    format!(
        r#"{{"data":[{{"a":{asks},"b":{bids},"pseq":{pseq},"seq":{seq},"maxDepth":"1000","ts":"1790000000400"}}],"arg":{{"instType":"usdt-futures","symbol":"BTCUSDT","topic":"books"}},"action":"{action}","ts":1790000000401}}"#
    )
}

#[test]
fn bgt_036_unknown_class() {
    let p = b1v3("1", "\"1790000000123\"", "1790000000125");
    for (row, c) in [
        ("BGT-036.1", MdClass::SubAck),
        ("BGT-036.2", MdClass::Pong),
        ("BGT-036.3", MdClass::Unknown),
    ] {
        let (r, md) = dec(c, &p);
        assert_eq!(r, Err(DecodeErr::UnknownClass), "{row}");
        assert!(md.is_empty(), "{row}");
    }
    // BGT-036.4: the incremental `books` stream (optional extension): snapshot, update, reset.
    let snap = books(
        "snapshot",
        0,
        100,
        r#"[["65000.1","1"],["65000","0"]]"#,
        r#"[["65000.2","2"]]"#,
    );
    let (r, md) = dec(MdClass::BooksIncr, &snap);
    assert_eq!(
        r,
        Ok(MdOutcome {
            events: 1,
            inst: F_BTC
        }),
        "BGT-036.4"
    );
    let MdRec::L2Snapshot {
        hdr: h, bids, asks, ..
    } = &md[0]
    else {
        panic!()
    };
    assert_eq!(
        (h.snap_id, h.origin, h.n_bids, h.n_asks),
        (100, L2SnapHdr::EXCHANGE, 1, 1),
        "BGT-036.4"
    );
    assert_eq!((bids.len(), asks.len()), (1, 1));
    let upd = books(
        "update",
        100,
        101,
        r#"[["65000.1","0"]]"#,
        r#"[["65000.2","3"],["65000.3","1"]]"#,
    );
    let k = Fut::peek(rd(), &FeedCtx::new(), MdClass::BooksIncr, upd.as_bytes()).unwrap();
    assert_eq!(
        (k.stream, k.ids),
        (
            StreamKind::DepthIncr,
            SeqIds {
                prev_id: 100,
                last_id: 101
            }
        ),
        "BGT-036.4"
    );
    let (r, md) = dec(MdClass::BooksIncr, &upd);
    assert!(r.is_ok(), "BGT-036.4");
    let MdRec::L2Begin(b) = md[0] else { panic!() };
    assert_eq!(
        (b.prev_id, b.last_id, b.n_bids, b.n_asks),
        (100, 101, 1, 2),
        "BGT-036.4"
    );
    assert_eq!(
        md[1],
        MdRec::L2Level(Side::Buy, Px(6_500_010), Qty(0)),
        "zero size deletes"
    );
    assert_eq!(md[2], MdRec::L2Level(Side::Sell, Px(6_500_020), Qty(300_000_000)));
    assert_eq!(md.last(), Some(&MdRec::L2End));
    // A reset push (`pseq = 0`) is a new snapshot.
    let reset = books("update", 0, 5, r#"[["65000.1","1"]]"#, r#"[["65000.2","1"]]"#);
    let (_, md) = dec(MdClass::BooksIncr, &reset);
    assert!(
        matches!(&md[0], MdRec::L2Snapshot { hdr, .. } if hdr.snap_id == 5),
        "BGT-036.4"
    );
    // BGT-036.5: spot has no ticker.
    let mut s = VecSink::default();
    let r = BitgetSpot::decode_md(
        rd(),
        &mut FeedCtx::new(),
        MdClass::Ticker,
        ticker("BTCUSDT", 1).as_bytes(),
        &RX,
        &mut s,
    );
    assert_eq!(r, Err(DecodeErr::UnknownClass), "BGT-036.5");
}

#[test]
fn bgt_037_control_hints() {
    let ok = Some(ControlHint::SubAck { ok: true, code: 0 });
    let rows = [
        (
            "BGT-037.1",
            MdClass::SubAck,
            r#"{"event":"subscribe","arg":{"instType":"usdt-futures","topic":"books1","symbol":"BTCUSDT"},"connId":"x"}"#,
            ok,
        ),
        (
            "BGT-037.2",
            MdClass::SubAck,
            r#"{"event":"error","code":30001,"msg":"x"}"#,
            Some(ControlHint::SubAck {
                ok: false,
                code: 30001,
            }),
        ),
        (
            "BGT-037.3",
            MdClass::SubAck,
            r#"{"event":"error","code":"30001","msg":"x"}"#,
            Some(ControlHint::SubAck {
                ok: false,
                code: 30001,
            }),
        ),
        (
            "BGT-037.4",
            MdClass::SubAck,
            r#"{"event":"unsubscribe","arg":{"instType":"spot","topic":"books1"}}"#,
            ok,
        ),
        ("BGT-037.5", MdClass::Pong, "pong", Some(ControlHint::Pong)),
        (
            "BGT-037.7",
            MdClass::SubAck,
            r#"{"event":"error"}"#,
            Some(ControlHint::SubAck { ok: false, code: 0 }),
        ),
        (
            "BGT-037.8",
            MdClass::SubAck,
            r#"{"event":"subscribe","arg":{"instType":"spot","topic":"ticker"},"code":"30001","msg":"rejected"}"#,
            Some(ControlHint::SubAck {
                ok: false,
                code: 30001,
            }),
        ),
    ];
    for (row, c, p, want) in rows {
        assert_eq!(Fut::control(c, p.as_bytes()), want, "{row}");
        assert_eq!(BitgetSpot::control(c, p.as_bytes()), want, "{row}");
    }
    let p = b1v2(1, 1);
    assert_eq!(Fut::control(MdClass::Books1, p.as_bytes()), None, "BGT-037.6");
}

#[test]
fn bgt_010_classify_md() {
    let b1 = b1v3("1", "\"1\"", "1");
    let rows: Vec<(&str, RouteId, &str, MdClass)> = vec![
        (
            "BGT-010.1",
            BOOKS1,
            Box::leak(b1v2(1, 1).into_boxed_str()),
            MdClass::Books1,
        ),
        (
            "BGT-010.2",
            BOOKS1,
            Box::leak(b1.into_boxed_str()),
            MdClass::Books1,
        ),
        (
            "BGT-010.3",
            TOPN,
            r#"{"action":"snapshot","arg":{"instType":"USDT-FUTURES","channel":"books15"}}"#,
            MdClass::BooksTopN,
        ),
        (
            "BGT-010.4",
            TOPN,
            r#"{"data":[{"a":[],"b":[],"seq":1}],"arg":{"topic":"books5"}}"#,
            MdClass::BooksTopN,
        ),
        (
            "BGT-010.5",
            TRADE,
            r#"{"data":[{"p":"1"}],"arg":{"topic":"publicTrade"}}"#,
            MdClass::Trade,
        ),
        (
            "BGT-010.6",
            TICKER,
            r#"{"data":[{"markPrice":"1"}],"arg":{"topic":"ticker"}}"#,
            MdClass::Ticker,
        ),
        (
            "BGT-010.7",
            TRADE,
            r#"{"event":"subscribe","arg":{}}"#,
            MdClass::SubAck,
        ),
        (
            "BGT-010.8",
            BOOKS1,
            r#"{"event":"error","code":30001,"msg":"instType:USDT-FUTURES,channel:books1,instId:XXX doesn't exist"}"#,
            MdClass::SubAck,
        ),
        (
            "BGT-010.9",
            TOPN,
            r#"{"event":"unsubscribe","arg":{}}"#,
            MdClass::SubAck,
        ),
        ("BGT-010.10", TICKER, "pong", MdClass::Pong),
        (
            "BGT-010.11",
            BOOKS1,
            r#"{"event":"login","code":0}"#,
            MdClass::Unknown,
        ),
        ("BGT-010.13", BOOKS1, "", MdClass::Unknown),
        ("BGT-010.14", BOOKS1, r#"{"ac"#, MdClass::Unknown),
        ("BGT-010.15", BOOKS1, "[1,2,3]", MdClass::Unknown),
        ("BGT-010.16", BOOKS1, "ping", MdClass::Unknown),
        (
            "BGT-010.18",
            TRADE,
            r#"{"code":30001,"msg":"x","event":"error"}"#,
            MdClass::SubAck,
        ),
        (
            "BGT-010.17",
            BOOKS,
            r#"{"data":[{"a":[],"b":[],"seq":1,"pseq":0}],"arg":{"topic":"books"}}"#,
            MdClass::BooksIncr,
        ),
    ];
    for (row, route, p, want) in rows {
        assert_eq!(
            Fut::classify(route, hft_venue_proto::MsgOp::Text, support::head(p)),
            want,
            "{row}"
        );
    }
    assert_eq!(
        Fut::classify(BOOKS1, hft_venue_proto::MsgOp::Binary, b"{\"data\":"),
        MdClass::Unknown,
        "BGT-010.12"
    );
}

#[test]
fn bgt_011_classify_md_spot() {
    let p = b1v2(1, 1).replace("USDT-FUTURES", "SPOT");
    let t = hft_venue_proto::MsgOp::Text;
    assert_eq!(
        BitgetSpot::classify(BOOKS1, t, support::head(&p)),
        MdClass::Books1,
        "BGT-011.1"
    );
    let p5 = r#"{"action":"snapshot","arg":{"instType":"SPOT","channel":"books5","instId":"BGBUSDT"}}"#;
    assert_eq!(
        BitgetSpot::classify(TOPN, t, support::head(p5)),
        MdClass::BooksTopN,
        "BGT-011.2"
    );
    let tr = r#"{"data":[{"p":"1"}],"arg":{"instType":"spot","topic":"publicTrade"}}"#;
    assert_eq!(
        BitgetSpot::classify(TRADE, t, support::head(tr)),
        MdClass::Trade,
        "BGT-011.3"
    );
    assert_eq!(
        BitgetSpot::classify(TICKER, t, support::head(tr)),
        MdClass::Unknown,
        "BGT-011.4"
    );
    let e = r#"{"event":"error","code":30001,"msg":"x"}"#;
    assert_eq!(
        BitgetSpot::classify(TRADE, t, support::head(e)),
        MdClass::SubAck,
        "BGT-011.5"
    );
}

// The escaped unknown field forces the reference decoder without changing selected fields.
// Cover every byte position, malformed selected values, duplicate keys and nested unknown data.
#[test]
fn bgt_027_projection_matches_checked() {
    fn decode(p: &[u8]) -> (Result<MdOutcome, DecodeErr>, Vec<MdRec>) {
        let mut out = VecSink::default();
        let result = Fut::decode_md(
            support::rd(),
            &mut FeedCtx::new(),
            MdClass::Books1,
            p,
            &support::RX,
            &mut out,
        );
        (result, out.md)
    }
    fn check(p: &[u8]) {
        if p.first() != Some(&b'{') {
            return;
        }
        let mut reference = br#"{"_wire_reference":"\u0061","#.to_vec();
        reference.extend_from_slice(&p[1..]);
        assert_eq!(
            decode(p),
            decode(&reference),
            "payload={}",
            String::from_utf8_lossy(p)
        );
    }
    let seed = support::b1v3("123456789", "\"1790000000123\"", "1790000000125")
        .as_bytes()
        .to_vec();
    check(&seed);
    for at in 1..seed.len() {
        for byte in [b'0', b'-', b'"', b'\\', b'{', b']', b',', b' ', 0, 0xff] {
            let mut p = seed.clone();
            p[at] = byte;
            check(&p);
        }
    }
    for extra in [
        r#", "unused":{"s":"wrong","values":[true,null,1.25,{"u":99}]}"#,
        r#", "u":2"#,
        r#", "arg":{}"#,
        r#", "data":[]"#,
    ] {
        let mut p = seed[..seed.len() - 1].to_vec();
        p.extend_from_slice(extra.as_bytes());
        p.push(b'}');
        check(&p);
    }
}
