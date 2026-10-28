use hft_event::{GwCmd, GwOp, OrderEvent, OrderEventKind, OrderFlags};
use hft_types::{AccountId, AssetId, ClientOrderId, InstId, Px, Qty, Side, SlotId, Tif, Tsc};
use hft_venue_proto::{testkit::*, *};
#[path = "fake_venue.rs"]
mod fake_venue;
use fake_venue::{FAKE_VENUE as V, Fake, FakePerp, FakeSpot, fake_rd};

fn place() -> (GwCmd, TargetRef) {
    (
        GwCmd {
            decision_seq: 1,
            decided_tsc: Tsc(1),
            px: Px(100),
            qty: Qty(2),
            inst: InstId(201),
            slot: SlotId(42),
            r#gen: 7,
            target_slot: SlotId(42),
            op: GwOp::Place,
            account: AccountId(1),
            side: Side::Buy,
            tif: Tif::Gtc,
            oflags: 0,
            _pad: [0; 2],
            arg: 0,
            arg2: 0,
        },
        TargetRef {
            id: ClientOrderId {
                epoch: 4,
                venue: V,
                account: AccountId(1),
                slot: 42,
                r#gen: 7,
            },
            exch_order_id: 100,
        },
    )
}
fn protection(b: &[u8]) -> bool {
    b.windows(9).any(|s| s == b"\"ro\":true")
}

#[test]
fn vpr_016_default_caps_asset_injection() {
    let yes = FakeSpot::default_caps(|s| {
        assert_eq!(s, "FKT");
        Some(AssetId(9))
    });
    let mut missing = FakeSpot::default_caps(|_| None);
    assert_eq!(
        missing.fee_asset_rule,
        hft_refdata::FeeAssetRule::ReceivedAssetUnlessDiscount { token: AssetId(0) }
    );
    missing.fee_asset_rule = yes.fee_asset_rule;
    assert_eq!(format!("{missing:?}"), format!("{yes:?}"));
    assert_eq!(
        FakePerp::default_caps(|_| panic!("unexpected lookup")).fee_asset_rule,
        hft_refdata::FeeAssetRule::Quote
    );
}

#[test]
fn vpr_087_capability_and_encoded_protection_contracts() {
    let rd = fake_rd();
    let (c, t) = place();
    check_default_caps::<FakeSpot>(&FakeSpot::default_caps(|_| None)).unwrap();
    check_default_caps::<FakePerp>(&FakePerp::default_caps(|_| None)).unwrap();
    check_reduce_only::<FakeSpot>(&rd, &(), &c, &t, &WallClock::default(), protection).unwrap();
    check_reduce_only::<FakePerp>(&rd, &(), &c, &t, &WallClock::default(), protection).unwrap();
    let mut dm = DeadManOut {
        frame: ArrayBuf::default(),
        reqs: Vec::new(),
    };
    assert_eq!(
        FakeSpot::dead_man_refresh(&rd, &(), 10000, &[], &WallClock::default(), &mut dm),
        Err(EncodeErr::Unsupported(GwOp::DeadManRefresh))
    );
}
#[test]
fn vpr_091_spot_dropped_reduce_only_is_rejected() {
    let rd = fake_rd();
    let (c, t) = place();
    assert_eq!(
        check_reduce_only::<Fake<true, 1>>(&rd, &(), &c, &t, &WallClock::default(), protection)
            .unwrap_err()
            .contract,
        11
    );
}
#[test]
fn vpr_092_perp_reduce_only_field() {
    let rd = fake_rd();
    let (mut c, t) = place();
    let mut buf = [0; 2048];
    for (flags, expected) in [
        (OrderFlags::REDUCE_ONLY.0, true),
        (0, false),
        (OrderFlags::RISK_REDUCING.0, false),
    ] {
        c.oflags = flags;
        let n = FakePerp::encode(&rd, &(), &c, &t, &WallClock::default(), &mut buf).unwrap();
        assert_eq!(protection(&buf[..n]), expected);
    }
}
#[test]
fn vpr_093_zero_rate_window_or_limit_is_rejected() {
    let caps = FakePerp::default_caps(|_| None);
    for err in [
        check_default_caps::<Fake<false, 2>>(&caps).unwrap_err(),
        check_default_caps::<Fake<false, 3>>(&caps).unwrap_err(),
    ] {
        assert_eq!(err.contract, 15);
        assert!(err.detail.contains("rate_rules"));
    }
}
fn reject(code: u32) -> OrderEvent {
    OrderEvent {
        venue: V,
        account: AccountId(1),
        what: OrderEventKind::Reject,
        detail: 0,
        slot: SlotId(42),
        r#gen: 7,
        inst: InstId(201),
        px: Px(0),
        qty: Qty(0),
        cum_filled: Qty(0),
        fee: Qty(0),
        fee_asset: AssetId(0),
        epoch: 4,
        reject_code: code,
        exch_order_id: 0,
        ex_ts_ns: 0,
        hw_rx_ns: 0,
        rx_tsc: 0,
        avg_px: Px(0),
        cash_qty: Qty(0),
        cash_sum: Qty(0),
        trade_id: 0,
        fill_cash_qty: hft_types::Qty(0),
        fill_cash_amount: hft_types::Qty(0),
    }
}
#[test]
fn vpr_076_unknown_codes_do_not_become_business_rejects() {
    let rd = fake_rd();
    let r = ReqRef {
        slot: 42,
        r#gen: 7,
        op: GwOp::Place,
    };
    for (code, raw, known) in [
        ("-2010", 0xFFFF_F826, true),
        ("51008", 51008, true),
        ("-1007", 0xFFFF_FC11, false),
        ("50004", 50004, false),
        ("-9999", 0xFFFF_D8F1, false),
        ("ERR_X", raw_code_str(b"ERR_X"), false),
        ("HTTP503", 0x2000_01F7, false),
    ] {
        let body = format!("{{\"e\":\"response\",\"code\":\"{code}\"}}");
        let expected = if known {
            vec![ReportRec::Order(reject(raw))]
        } else {
            vec![]
        };
        let samples = [ReportSample {
            name: code,
            op: MsgOp::Text,
            payload: body.as_bytes(),
            rx: RxMeta {
                account: AccountId(1),
                ..Default::default()
            },
            expected: &expected,
            outcome: ReportOutcome {
                req: Some(r),
                outcome_unknown: (!known).then_some(r),
                raw_code: raw,
                events: u16::from(known),
                ..Default::default()
            },
        }];
        check_unknown_codes::<FakePerp>(&samples, &rd, &mut Default::default()).unwrap();
    }
}
#[test]
fn vpr_108_ignored_and_unknown_are_successful_empty_decodes() {
    let rd = fake_rd();
    for (body, skip) in [
        (b"{\"e\":\"listenKeyExtended\"}".as_slice(), Skip::Listed),
        (b"{\"e\":\"new-event\"}", Skip::Unknown),
    ] {
        let s = ReportSample {
            name: "skip",
            op: MsgOp::Text,
            payload: body,
            rx: RxMeta::default(),
            expected: &[],
            outcome: ReportOutcome {
                skip,
                ..Default::default()
            },
        };
        check_reports::<FakePerp>(20, &[s], &rd, &mut Default::default()).unwrap();
    }
}
#[test]
fn vpr_112_uid_requires_successful_signed_session_evidence() {
    fn account<P: AcctProtocol>(rd: &hft_refdata::RefData, keys: u8, uid: [u8; 24]) {
        let mut ctx = P::AcctCtx::default();
        let mut scratch = P::AcctScratch::default();
        for q in P::acct_queries(AcctQueryKind::Config) {
            P::acct_decode(
                rd,
                &mut ctx,
                AccountId(1),
                *q,
                200,
                &1i64.to_le_bytes(),
                &mut scratch,
            )
            .unwrap();
        }
        let mut sink = VecSink::default();
        P::acct_finish(
            rd,
            &mut ctx,
            AccountId(1),
            AcctQueryKind::Config,
            &scratch,
            &mut sink,
        )
        .unwrap();
        let ReportRec::Config(h, _) = &sink.reports[0] else {
            panic!("config required")
        };
        assert_eq!((h.venue_uid, h.uid_keys), (uid, keys));
    }
    let cfg = VenueCfg {
        env: VenueEnv::Testnet,
        stale_guard_ms: 1000,
    };
    let mut uid = [0; 24];
    uid[..2].copy_from_slice(b"U1");
    check_identity::<FakePerp>(&cfg, &Credential::default(), b"U1", uid).unwrap();
    check_identity::<FakeSpot>(&cfg, &Credential::default(), b"U1", uid).unwrap();
    account::<FakeSpot>(&fake_rd(), 3, uid);
    account::<FakePerp>(&fake_rd(), 2, uid);
    for b in [b"".as_slice(), b"U\x001", b"U 1", b"\xff", &[b'U'; 25]] {
        assert!(FakePerp::decode_identity(200, b, &mut uid).is_err());
    }
    assert!(!valid_uid(&[0; 24]));
    let mut malformed = [0; 24];
    malformed[0] = b'U';
    malformed[2] = b'1';
    assert!(!valid_uid(&malformed));
}

#[test]
fn vpr_152_gw_rest_pool_defaults() {
    assert_eq!(
        (FakeSpot::GW_REST_CONNS, FakePerp::GW_REST_CONNS),
        (2, 2),
        "VPR-152.1"
    );
    assert_eq!(
        (FakeSpot::GW_REST_CANCEL_RESERVE, FakePerp::GW_REST_CANCEL_RESERVE),
        (0, 0),
        "VPR-152.2"
    );
}
