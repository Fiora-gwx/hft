use crate::support::*;
use hft_event::*;
use hft_types::*;
use hft_venue_proto::*;
#[derive(Default)]
struct Sink {
    orders: Vec<OrderEvent>,
    fills: Vec<FillRow>,
    balances: Vec<BalanceReport>,
    positions: Vec<PositionReport>,
}
impl RoSink for Sink {
    fn open_order(&mut self, e: &OrderEvent) {
        self.orders.push(*e);
    }
    fn fill(&mut self, e: &FillRow) {
        self.fills.push(*e);
    }
    fn funding(&mut self, _: &FundingSettle) {
        panic!("unexpected funding");
    }
    fn balance(&mut self, e: &BalanceReport) {
        self.balances.push(*e);
    }
    fn position(&mut self, e: &PositionReport) {
        self.positions.push(*e);
    }
}
fn query(kind: RoKind, inst: InstId, limit: u16) -> RoQuery {
    RoQuery {
        kind,
        inst,
        limit,
        cursor: 0,
        since_ms: 1_790_000_000_000,
    }
}
use venue_bitget::{BitgetSpot, BitgetUsdtFutures as P};
#[test]
fn bgt_175_readonly_pages_are_atomic_and_signed() {
    let t = P::build_templates(
        &VenueCfg {
            env: VenueEnv::Testnet,
            ..cfg()
        },
        rd(),
        ACCT,
        &key(),
    )
    .unwrap();
    let mut q = query(RoKind::Fills, F_BTC, 1);
    let mut r = RestReqBuf::default();
    P::ro_request_signed(rd(), &t, &crate::support::key(), ACCT, &q, &WC, &mut r).unwrap();
    assert_eq!(r.method, Method::Get);
    assert!(r.redacted);
    assert!(
        std::str::from_utf8(r.headers.as_slice())
            .unwrap()
            .contains("paptrading: 1")
    );
    let row = serde_json::json!({"symbol":"BTCUSDT","category":"USDT-FUTURES","execId":"21","orderId":"7","clientOid":"foreign","side":"sell","execPrice":"60000","execQty":"0.001","createdTime":"1790000000001","tradeSide":"open","feeDetail":[{"feeCoin":"USDT","fee":"0.02"}]});
    let body =
        serde_json::to_vec(&serde_json::json!({"code":"00000","data":{"list":[row.clone()],"cursor":"21"}}))
            .unwrap();
    let mut out = Sink::default();
    assert_eq!(
        P::ro_decode(rd(), ACCT, &q, 200, &body, &mut out).unwrap(),
        RoPage {
            rows: 1,
            next_cursor: 21
        }
    );
    assert_eq!(
        (out.fills[0].qty, out.fills[0].fee, out.fills[0].cid),
        (Qty(100_000), Qty(2_000_000), None)
    );
    q.cursor = 21;
    P::ro_request_signed(rd(), &t, &crate::support::key(), ACCT, &q, &WC, &mut r).unwrap();
    assert!(
        std::str::from_utf8(r.path_query.as_slice())
            .unwrap()
            .ends_with("&cursor=21")
    );
    assert!(P::ro_decode(rd(), ACCT, &q, 200, &body, &mut Sink::default()).is_err());
    q.cursor = 0;
    q.limit = 2;
    let mut bad = row.clone();
    bad["execId"] = "22".into();
    bad["feeDetail"][0]["feeCoin"] = "UNKNOWN".into();
    let mut out = Sink::default();
    let b = serde_json::to_vec(&serde_json::json!({"code":"00000","data":{"list":[row,bad],"cursor":""}}))
        .unwrap();
    assert!(P::ro_decode(rd(), ACCT, &q, 200, &b, &mut out).is_err());
    assert!(out.fills.is_empty());
    q.kind = RoKind::Positions;
    q.inst = S_BTC;
    let st = BitgetSpot::build_templates(&cfg(), rd(), ACCT, &key()).unwrap();
    assert!(BitgetSpot::ro_request_signed(rd(), &st, &crate::support::key(), ACCT, &q, &WC, &mut r).is_err());
}
#[test]
fn bgt_175_open_orders_preserve_foreign_identity() {
    let q = query(RoKind::OpenOrders, F_BTC, 0);
    let row = serde_json::json!({"symbol":"BTCUSDT","category":"USDT-FUTURES","orderId":"7","clientOid":"external","orderStatus":"partially_filled","price":"60000","qty":"0.002","cumExecQty":"0.001","avgPrice":"60000","updatedTime":"1"});
    let mut id = [0; 24];
    cid(FUT, 3, 42, 1).encode(&mut id);
    let mut own = row.clone();
    own["orderId"] = "8".into();
    own["clientOid"] = std::str::from_utf8(&id).unwrap().into();
    let b = serde_json::to_vec(&serde_json::json!({"code":"00000","data":{"list":[own,row],"cursor":""}}))
        .unwrap();
    let mut out = Sink::default();
    P::ro_decode(rd(), ACCT, &q, 200, &b, &mut out).unwrap();
    assert_eq!(
        (out.orders[0].slot, out.orders[1].slot),
        (SlotId(42), SlotId::UNRESOLVED)
    );
    assert_eq!(out.orders[0].avg_px, Px(6_000_000));
}

fn captured_page<T: VenueProtocol<FeedCtx = venue_bitget::FeedCtx, GwCtx = venue_bitget::GwCtx>>(
    channel: &str,
    name: &str,
    kind: RoKind,
) {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures")
        .join(T::NAME)
        .join(channel)
        .join(name);
    let input = std::fs::read(path.with_extension("in")).unwrap();
    let expected: Vec<serde_json::Value> = std::fs::read_to_string(path.with_extension("expected.jsonl"))
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    let q = RoQuery {
        kind,
        inst: InstId(0),
        limit: 100,
        cursor: 0,
        since_ms: 1_792_018_800_000,
    };
    let mut sink = Sink::default();
    let page = T::ro_decode(rd_spot_capture(), ACCT, &q, 200, &input, &mut sink).unwrap();
    let mut actual: Vec<_> = sink
        .fills
        .iter()
        .map(|f| {
            serde_json::json!({
                "inst":f.inst.0,"side":if f.side == Side::Buy {"buy"} else {"sell"},
                "qty":f.qty.0.to_string(),"px":f.px.0.to_string(),"fee":f.fee.0.to_string(),
                "fee_asset":f.fee_asset.0,"trade_id":f.trade_id,"exch_order_id":f.exch_order_id
            })
        })
        .collect();
    actual.push(serde_json::json!({"rows":page.rows,"next_cursor":page.next_cursor}));
    assert_eq!(actual, expected);
    if name == "empty" {
        let mut malformed: serde_json::Value = serde_json::from_slice(&input).unwrap();
        malformed["data"].as_object_mut().unwrap().remove("list");
        assert!(
            T::ro_decode(
                rd_spot_capture(),
                ACCT,
                &q,
                200,
                &serde_json::to_vec(&malformed).unwrap(),
                &mut Sink::default()
            )
            .is_err()
        );
        malformed["data"]["list"] = serde_json::Value::Null;
        malformed["data"]["cursor"] = "123".into();
        assert!(
            T::ro_decode(
                rd_spot_capture(),
                ACCT,
                &q,
                200,
                &serde_json::to_vec(&malformed).unwrap(),
                &mut Sink::default()
            )
            .is_err()
        );
    }
}

#[test]
fn bgt_334_ro_open_orders() {
    captured_page::<P>("rest.open_orders", "empty", RoKind::OpenOrders);
    captured_page::<BitgetSpot>("rest.open_orders", "empty", RoKind::OpenOrders);
}

#[test]
fn bgt_335_ro_fills() {
    for name in ["empty", "page"] {
        captured_page::<P>("rest.fills", name, RoKind::Fills);
        captured_page::<BitgetSpot>("rest.fills", name, RoKind::Fills);
    }
}
