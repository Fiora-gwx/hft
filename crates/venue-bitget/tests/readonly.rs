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
