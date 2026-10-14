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
use venue_binance::{Spot, Usdm};
#[test]
fn bin_175_readonly_request_and_atomic_pages() {
    let t = Usdm::build_templates(&cfg(1500), rd(), ACCT, &hm1()).unwrap();
    let mut q = query(RoKind::Fills, U_BTC, 1);
    let mut r = RestReqBuf::default();
    Usdm::ro_request(rd(), &t, ACCT, &q, &WC, &mut r).unwrap();
    let p = std::str::from_utf8(r.path_query.as_slice()).unwrap();
    assert!(p.starts_with("/fapi/v1/userTrades?symbol=BTCUSDT&startTime=1790000000000&limit=1&timestamp="));
    assert!(p.contains("&signature="));
    assert_eq!(r.method, Method::Get);
    assert!(r.redacted);
    let row = serde_json::json!({"symbol":"BTCUSDT","id":21,"orderId":7,"price":"60000","qty":"0.001","commission":"-0.01","commissionAsset":"USDT","time":1790000000001i64,"side":"BUY"});
    let body = serde_json::to_vec(&vec![row.clone()]).unwrap();
    let mut out = Sink::default();
    let page = Usdm::ro_decode(rd(), ACCT, &q, 200, &body, &mut out).unwrap();
    assert_eq!(
        page,
        RoPage {
            rows: 1,
            next_cursor: 22
        }
    );
    assert_eq!(
        (out.fills[0].qty, out.fills[0].fee, out.fills[0].cid),
        (Qty(100_000), Qty(-1_000_000), None)
    );
    q.cursor = 22;
    Usdm::ro_request(rd(), &t, ACCT, &q, &WC, &mut r).unwrap();
    let p = std::str::from_utf8(r.path_query.as_slice()).unwrap();
    assert!(p.contains("fromId=22"));
    assert!(!p.contains("startTime="));
    assert!(Usdm::ro_decode(rd(), ACCT, &q, 200, &body, &mut Sink::default()).is_err());
    q.cursor = 0;
    q.limit = 2;
    let mut bad = row.clone();
    bad["symbol"] = "UNKNOWN".into();
    let mut empty = Sink::default();
    assert!(
        Usdm::ro_decode(
            rd(),
            ACCT,
            &q,
            200,
            &serde_json::to_vec(&vec![row, bad]).unwrap(),
            &mut empty
        )
        .is_err()
    );
    assert!(empty.fills.is_empty());
    q.inst = InstId(0);
    assert!(Usdm::ro_request(rd(), &t, ACCT, &q, &WC, &mut r).is_err());
    let st = Spot::build_templates(&cfg(1500), rd(), ACCT, &hm1()).unwrap();
    q.kind = RoKind::Positions;
    assert!(Spot::ro_request(rd(), &st, ACCT, &q, &WC, &mut r).is_err());
    q.kind = RoKind::Fills;
    q.inst = S_BTC;
    q.limit = 1;
    let body=br#"[{"symbol":"BTCUSDT","id":2,"orderId":9,"price":"60000","qty":"0.0001","commission":"0.00000001","commissionAsset":"BTC","time":1,"isBuyer":false}]"#;
    let mut out = Sink::default();
    Spot::ro_decode(rd(), ACCT, &q, 200, body, &mut out).unwrap();
    assert_eq!(
        (out.fills[0].side, out.fills[0].fee_asset, out.fills[0].fee),
        (Side::Sell, BTC, Qty(1))
    );
}
#[test]
fn bin_175_open_orders_preserve_foreign_identity() {
    let q = query(RoKind::OpenOrders, U_BTC, 0);
    let mut row = serde_json::json!({"symbol":"BTCUSDT","orderId":7,"clientOrderId":"external","status":"PARTIALLY_FILLED","price":"60000","origQty":"0.002","executedQty":"0.001","avgPrice":"60000","updateTime":1});
    let mut id = [0; 24];
    cid(VenueId::BINANCE_USDM, 3, 42, 1).encode(&mut id);
    let mut own = row.clone();
    own["orderId"] = 8.into();
    own["clientOrderId"] = std::str::from_utf8(&id).unwrap().into();
    let mut out = Sink::default();
    Usdm::ro_decode(
        rd(),
        ACCT,
        &q,
        200,
        &serde_json::to_vec(&vec![own, row.clone()]).unwrap(),
        &mut out,
    )
    .unwrap();
    assert_eq!(out.orders[0].slot, SlotId(42));
    assert_eq!(out.orders[1].slot, SlotId::UNRESOLVED);
    assert_eq!(out.orders[0].avg_px, Px(6_000_000));
    row["avgPrice"] = "0".into();
    assert!(
        Usdm::ro_decode(
            rd(),
            ACCT,
            &q,
            200,
            &serde_json::to_vec(&vec![row]).unwrap(),
            &mut Sink::default()
        )
        .is_err()
    );
}
