#![allow(clippy::disallowed_types)] // independent FIFO reference model
use hft_types::{AccountId, ClientOrderId, InstId, SlotId, VenueId};
use hft_venue_proto::testkit::{ReportRec, VecSink};
use hft_venue_proto::*;
use zerocopy::FromZeros;

#[test]
fn vpr_040_vec_sink_md() {
    use hft_event::*;
    use hft_types::{Px, Qty, Side};
    use hft_venue_proto::testkit::MdRec;
    let mut s = VecSink::default();
    let b = Bbo::new_zeroed();
    let h = MarketHdr::new_zeroed();
    let batch = L2BatchHdr::new_zeroed();
    let snap = L2SnapHdr::new_zeroed();
    let lv = [Level {
        px: Px(100),
        qty: Qty(2),
    }];
    let trade = Trade {
        h,
        px: Px(100),
        qty: Qty(2),
        trade_id: 7,
        taker_side: Side::Buy,
        _pad: [0; 7],
    };
    let liq = Liquidation {
        h,
        px: Px(100),
        qty: Qty(2),
        side: Side::Sell,
        _pad: [0; 7],
    };
    let m = MarkPrice::new_zeroed();
    let i = IndexPrice::new_zeroed();
    let f = Funding::new_zeroed();
    s.bbo(&b);
    s.l2_begin(&batch);
    s.l2_level(Side::Buy, Px(100), Qty(2));
    s.l2_end();
    s.l2_snapshot(&L2SnapshotPart {
        hdr: snap,
        bids: &lv,
        asks: &[],
        part: 0,
        parts: 1,
    });
    s.trade(&trade);
    s.mark(&m);
    s.index(&i);
    s.funding(&f);
    s.liquidation(&liq);
    assert_eq!(
        s.md,
        [
            MdRec::Bbo(b),
            MdRec::L2Begin(batch),
            MdRec::L2Level(Side::Buy, Px(100), Qty(2)),
            MdRec::L2End,
            MdRec::L2Snapshot {
                hdr: snap,
                bids: lv.to_vec(),
                asks: vec![],
                part: 0,
                parts: 1
            },
            MdRec::Trade(trade),
            MdRec::Mark(m),
            MdRec::Index(i),
            MdRec::Funding(f),
            MdRec::Liquidation(liq)
        ]
    );
}
#[path = "fake_venue.rs"]
mod fake_venue;

#[test]
fn vpr_046_golden_first_difference() {
    use hft_venue_proto::testkit::GoldenDriver;
    type G = GoldenDriver<fake_venue::FakePerp>;
    let want = vec![vec![1, 2, 3], vec![4, 5]];
    let mark = testkit::MdRec::Mark(hft_event::MarkPrice::new_zeroed()).bytes();
    let index = testkit::MdRec::Index(hft_event::IndexPrice::new_zeroed()).bytes();
    assert_eq!(G::compare("event kind", &[mark], &[index]).unwrap_err().offset, 0);
    assert!(G::compare("fixture", &want, &want).is_ok());
    for (got, record, offset) in [
        (vec![vec![1, 9, 3], vec![4, 5]], 0, 1),
        (vec![vec![1, 2, 3], vec![4]], 1, 1),
        (vec![vec![1, 2, 3]], 1, 0),
    ] {
        let err = G::compare("fixture", &got, &want).unwrap_err();
        assert_eq!(
            (err.sample.as_str(), err.record, err.offset),
            ("fixture", record, offset)
        );
    }
}
#[test]
fn vpr_089_dead_man_boundary() {
    use hft_refdata::{DeadManCap, VenueCaps};
    for timeout in [1999, 2000, 2001] {
        let mut caps = VenueCaps::none(VenueId(1));
        caps.dms = DeadManCap::CountdownPerInst;
        caps.dms_min_ms = 500;
        caps.dms_timeout_ms = timeout;
        caps.dms_refresh_ms = 1000;
        let result = testkit::check_dead_man(
            DeadManSpec::CountdownPerInst {
                min_ms: 500,
                timeout_ms: timeout,
                refresh_ms: 1000,
                via: Via::Rest,
            },
            &caps,
        );
        assert_eq!(result.is_ok(), timeout >= 2000);
    }
    let mut caps = VenueCaps::none(VenueId(1));
    caps.dms = DeadManCap::CountdownPerInst;
    caps.dms_timeout_ms = u32::MAX;
    caps.dms_refresh_ms = u32::MAX;
    assert!(
        testkit::check_dead_man(
            DeadManSpec::CountdownPerInst {
                min_ms: 0,
                timeout_ms: u32::MAX,
                refresh_ms: u32::MAX,
                via: Via::Ws
            },
            &caps
        )
        .is_err()
    );
}
#[test]
fn vpr_090_dead_man_caps() {
    use hft_refdata::{DeadManCap as D, VenueCaps};
    for (spec, kind, min, max, timeout, refresh) in [
        (DeadManSpec::None, D::None, 0, 0, 0, 0),
        (
            DeadManSpec::CountdownPerInst {
                min_ms: 5,
                timeout_ms: 20,
                refresh_ms: 10,
                via: Via::Rest,
            },
            D::CountdownPerInst,
            5,
            0,
            20,
            10,
        ),
        (
            DeadManSpec::CountdownAccount {
                min_ms: 5,
                max_ms: 30,
                timeout_ms: 20,
                refresh_ms: 10,
                via: Via::Ws,
            },
            D::CountdownAccount,
            5,
            30,
            20,
            10,
        ),
        (
            DeadManSpec::OnDisconnect { window_ms: 5 },
            D::OnDisconnect,
            0,
            0,
            5,
            0,
        ),
        (
            DeadManSpec::Schedule {
                min_ahead_ms: 5,
                max_triggers_per_day: 10,
                timeout_ms: 20,
                refresh_ms: 10,
            },
            D::Schedule,
            5,
            0,
            20,
            10,
        ),
    ] {
        let mut caps = VenueCaps::none(VenueId(1));
        caps.dms = kind;
        caps.dms_min_ms = min;
        caps.dms_max_ms = max;
        caps.dms_timeout_ms = timeout;
        caps.dms_refresh_ms = refresh;
        assert!(testkit::check_dead_man(spec, &caps).is_ok());
        caps.dms_timeout_ms += 1;
        assert!(testkit::check_dead_man(spec, &caps).is_err());
    }
}

#[test]
fn vpr_020_raw_code_encoding() {
    assert_eq!(raw_code_num(-2010), 0xffff_f826);
    assert_eq!(raw_code_num(-1007), 0xffff_fc11);
    for (s, n) in [
        (b"51008".as_slice(), 51008),
        (b"-1007", 0xffff_fc11),
        (b"007", 7),
        (b"2147483647", 0x7fff_ffff),
        (b"-2147483648", 0x8000_0000),
    ] {
        assert_eq!(raw_code_str(s), n);
    }
    for s in [b"2147483648".as_slice(), b"abc", b"+5", b"", b"-"] {
        assert_eq!(
            raw_code_str(s),
            0x4000_0000 | (xxhash_rust::xxh3::xxh3_64(s) as u32 & 0x00ff_ffff)
        );
    }
    assert_eq!(raw_code_http(503), 0x2000_01f7);
    assert_eq!(RAW_CODE_NONE, 0);
}
#[test]
fn vpr_030_dedup_key_distinct() {
    let inputs = [
        (201, TradeKey::Id(5)),
        (202, TradeKey::Id(5)),
        (
            201,
            TradeKey::OrderSeq {
                exch_order_id: 5,
                fill_seq: 0,
            },
        ),
        (
            201,
            TradeKey::Funding {
                settle_ts_ns: 5,
                disc: 0,
            },
        ),
        (
            201,
            TradeKey::OrderSeq {
                exch_order_id: 7,
                fill_seq: 100,
            },
        ),
        (
            201,
            TradeKey::OrderSeq {
                exch_order_id: 7,
                fill_seq: 200,
            },
        ),
        (
            0,
            TradeKey::Funding {
                settle_ts_ns: 1_700_000_000_000_000_000,
                disc: 1,
            },
        ),
        (
            0,
            TradeKey::Funding {
                settle_ts_ns: 1_700_000_000_000_000_000,
                disc: 2,
            },
        ),
    ];
    let mut keys = Vec::new();
    for (inst, key) in inputs {
        let a = dedup_key(InstId(inst), key);
        assert_ne!(a, 0);
        assert_eq!(a, dedup_key(InstId(inst), key));
        assert!(!keys.contains(&a));
        keys.push(a);
    }
}
#[test]
fn vpr_032_ring_seen() {
    let mut r = TradeIdRing::<1024>::new();
    assert!(!r.seen_or_insert(1));
    assert!(r.seen_or_insert(1));
    assert!(!r.seen_or_insert(2));
}
#[test]
fn vpr_033_ring_eviction_boundary() {
    for n in [1023, 1024, 1025] {
        let mut r = TradeIdRing::<1024>::new();
        for key in 1..=n {
            assert!(!r.seen_or_insert(key));
        }
        assert_eq!(r.seen_or_insert(1), n <= 1024);
    }
}
#[test]
fn vpr_035_ring_matches_model() {
    use std::collections::{HashSet, VecDeque};
    for seed in 1..=10 {
        let mut r = TradeIdRing::<1024>::new();
        let mut set = HashSet::new();
        let mut fifo = VecDeque::new();
        let mut x = seed;
        for _ in 0..1_000_000 {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            let key = x % 4096 + 1;
            let seen = set.contains(&key);
            assert_eq!(r.seen_or_insert(key), seen);
            if !seen {
                if fifo.len() == 1024 {
                    set.remove(&fifo.pop_front().unwrap());
                }
                set.insert(key);
                fifo.push_back(key);
            }
        }
    }
}
#[test]
fn vpr_041_vec_sink_report() {
    use hft_event::*;
    use hft_types::{AssetId, Px, Qty, Side, Tsc};
    let mut s = VecSink::default();
    let o = OrderEvent {
        venue: VenueId(1),
        account: AccountId(1),
        what: OrderEventKind::Ack,
        detail: 0,
        slot: SlotId(1),
        r#gen: 1,
        inst: InstId(201),
        px: Px(10),
        qty: Qty(7),
        cum_filled: Qty(0),
        fee: Qty(0),
        fee_asset: AssetId(0),
        epoch: 4,
        reject_code: 0,
        exch_order_id: 1,
        ex_ts_ns: 0,
        hw_rx_ns: 0,
        rx_tsc: 0,
        avg_px: Px(0),
        cash_qty: Qty(0),
        cash_sum: Qty(0),
        trade_id: 0,
    };
    let p = PositionReport::new_zeroed();
    let b = BalanceReport::new_zeroed();
    let f = FundingSettle::new_zeroed();
    let e = ExternalFill {
        venue: VenueId(1),
        account: AccountId(1),
        side: Side::Buy,
        cause: ExternalCause::Unknown,
        inst: InstId(201),
        qty: Qty(7),
        px: Px(10),
        fee: Qty(0),
        fee_asset: AssetId(0),
        _pad: [0; 6],
        exch_order_id: 1,
        trade_id: 0,
        ex_ts_ns: 0,
        rx_tsc: Tsc(0),
        fill_seq: 7,
        cash_qty: Qty(0),
        cash_amount: Qty(0),
    };
    let m = MarginReportHdr::new_zeroed();
    let items = [MarginItem::new_zeroed(); 2];
    let c = AccountConfigReportHdr::new_zeroed();
    s.order(&o);
    s.position(&p);
    s.balance(&b);
    s.funding_settle(&f);
    s.external_fill(&e);
    s.margin(&m, &items);
    s.account_config(&c, &[]);
    assert_eq!(
        s.reports,
        [
            ReportRec::Order(o),
            ReportRec::Position(p),
            ReportRec::Balance(b),
            ReportRec::Funding(f),
            ReportRec::External(e),
            ReportRec::Margin(m, items.to_vec()),
            ReportRec::Config(c, vec![])
        ]
    );
}
#[test]
fn vpr_042_vec_sink_exch_id() {
    let mut s = VecSink::default();
    let id = ClientOrderId {
        epoch: 4,
        venue: VenueId(1),
        account: AccountId(3),
        slot: 77,
        r#gen: 9,
    };
    assert_eq!(s.exch_id(b"X1", None), ExchIdHint::Unknown);
    assert_eq!(s.exch_id(b"X1", Some(&id)), ExchIdHint::Recorded);
    assert_eq!(
        s.exch_id(b"X1", None),
        ExchIdHint::Resolved {
            epoch: 4,
            slot: SlotId(77),
            r#gen: 9
        }
    );
    assert_eq!(s.exch_id(b"X2", None), ExchIdHint::Unknown);
    assert_eq!(s.id_calls.len(), 4);
}

#[test]
fn vpr_047_golden_acct() {
    use fake_venue::{FAKE_VENUE as V, FakePerp as P};
    use hft_event::{AccountConfigReportHdr, LeverageItem, MarginItem, MarginReportHdr, PositionReport};
    use hft_types::{Notional, Px, Qty, Rate};
    use hft_venue_proto::testkit::{AcctSample, GoldenDriver};
    let rd = fake_venue::fake_rd();
    let account = AccountId(1);
    let mut config = AccountConfigReportHdr::new_zeroed();
    config.venue = V;
    config.account = account;
    config.n_lev = 1;
    config.uid_keys = 2;
    config.venue_uid[..2].copy_from_slice(b"U1");
    let lev = LeverageItem {
        inst: InstId(201),
        _pad: 0,
        leverage: Rate(5_000_000_000),
    };
    let mut margin = MarginReportHdr::new_zeroed();
    margin.venue = V;
    margin.account = account;
    margin.margin_balance = Notional(11);
    margin.n = 1;
    let item = MarginItem {
        inst: InstId(201),
        adl_quantile: 255,
        _pad: [0; 3],
        liq_px: Px(0),
    };
    let mut pos = PositionReport::new_zeroed();
    pos.venue = V;
    pos.account = account;
    pos.inst = InstId(201);
    pos.qty = Qty(12);
    let mut pos2 = pos;
    pos2.inst = InstId(202);
    pos2.qty = Qty(-12);
    for (kind, want) in [
        (AcctQueryKind::Config, vec![ReportRec::Config(config, vec![lev])]),
        (
            AcctQueryKind::Margin,
            vec![
                ReportRec::Margin(margin, vec![item]),
                ReportRec::Position(pos),
                ReportRec::Position(pos2),
            ],
        ),
    ] {
        let queries = P::acct_queries(kind);
        let wires: Vec<_> = queries
            .iter()
            .map(|q| {
                let mut r = RestReqBuf {
                    redacted: true,
                    ..Default::default()
                };
                r.path_query
                    .push(b"/account?signature=test-signed-value")
                    .unwrap();
                r.endpoint = if kind == AcctQueryKind::Config {
                    q.step % 2
                } else {
                    0
                };
                r
            })
            .collect();
        let bodies: Vec<_> = (11i64..).take(queries.len()).map(i64::to_le_bytes).collect();
        let samples: Vec<_> = queries
            .iter()
            .enumerate()
            .map(|(i, q)| AcctSample {
                name: "account sequence",
                query: *q,
                status: 200,
                body: &bodies[i],
                request: &wires[i],
            })
            .collect();
        GoldenDriver::<P>::run_acct(
            kind,
            &samples,
            &rd,
            account,
            &(),
            &WallClock::default(),
            &mut 0,
            &want,
        )
        .unwrap();
        let mut bad = want.clone();
        if let ReportRec::Config(h, _) = &mut bad[0] {
            h.mismatches = 1;
        }
        if let ReportRec::Margin(h, _) = &mut bad[0] {
            h.margin_balance = Notional(12);
        }
        assert!(
            GoldenDriver::<P>::run_acct(
                kind,
                &samples,
                &rd,
                account,
                &(),
                &WallClock::default(),
                &mut 0,
                &bad
            )
            .is_err()
        );
    }
}
