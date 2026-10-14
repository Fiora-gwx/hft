//! Inverse UTA codecs for offline exchange simulation; no socket, clock or credentials.
use hft_venue_proto::{DecodeErr, MsgOp};
use serde::Deserialize;

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum MockVenueKind {
    Futures,
    Spot,
}
impl MockVenueKind {
    fn category(self) -> &'static str {
        match self {
            Self::Futures => "usdt-futures",
            Self::Spot => "spot",
        }
    }
}
#[derive(Clone, Debug)]
pub enum MockMarket<'a> {
    Book {
        symbol: &'a str,
        topic: &'a str,
        bids: &'a [[&'a str; 2]],
        asks: &'a [[&'a str; 2]],
        seq: u64,
        prev: u64,
        ts_ms: u64,
    },
    Trade {
        symbol: &'a str,
        px: &'a str,
        qty: &'a str,
        side: &'a str,
        id: u64,
        ts_ms: u64,
    },
    Mark {
        symbol: &'a str,
        mark: &'a str,
        index: &'a str,
        rate: &'a str,
        next_ms: u64,
        ts_ms: u64,
    },
}
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
pub struct MockOrderReq {
    pub id: String,
    pub topic: String,
    pub args: Vec<MockOrderArg>,
    category: String,
    op: String,
}
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct MockOrderArg {
    pub client_oid: String,
    #[serde(default)]
    pub symbol: String,
    #[serde(default)]
    pub side: String,
    #[serde(default)]
    pub price: String,
    #[serde(default)]
    pub qty: String,
    #[serde(default)]
    pub time_in_force: String,
    #[serde(default)]
    pub reduce_only: String,
    #[serde(default)]
    pub auto_borrow: String,
}
#[derive(Clone, Debug)]
pub enum MockOrderResp<'a> {
    Reply {
        id: &'a str,
        topic: &'a str,
        code: &'a str,
        client_oid: &'a str,
        order_id: u64,
        ts_ms: u64,
    },
    Order {
        symbol: &'a str,
        client_oid: &'a str,
        order_id: u64,
        side: &'a str,
        status: &'a str,
        cum_qty: &'a str,
        cum_value: &'a str,
        avg_px: &'a str,
        ts_ms: u64,
    },
}
#[derive(Clone, Debug)]
pub enum MockRestReq<'a> {
    Book {
        bids: &'a [[&'a str; 2]],
        asks: &'a [[&'a str; 2]],
        ts_ms: u64,
    },
    Order {
        symbol: &'a str,
        client_oid: &'a str,
        order_id: u64,
        status: &'a str,
        cum_qty: &'a str,
        avg_px: &'a str,
        ts_ms: u64,
    },
    Error {
        status: u16,
        code: &'a str,
    },
}
fn quoted(s: &str) -> String {
    sonic_rs::to_string(s).unwrap_or_default()
}
fn levels(s: &[[&str; 2]]) -> String {
    sonic_rs::to_string(s).unwrap_or_default()
}
fn write(out: &mut Vec<u8>, s: String) {
    out.clear();
    out.extend_from_slice(s.as_bytes());
}

pub fn encode_market(v: MockVenueKind, ev: &MockMarket<'_>, out: &mut Vec<u8>) -> MsgOp {
    let (symbol, topic, data, ts, action) = match ev {
        MockMarket::Book {
            symbol,
            topic,
            bids,
            asks,
            seq,
            prev,
            ts_ms,
        } => (
            *symbol,
            *topic,
            format!(
                "{{\"b\":{},\"a\":{},\"seq\":{seq},\"pseq\":{prev},\"ts\":\"{ts_ms}\"}}",
                levels(bids),
                levels(asks)
            ),
            *ts_ms,
            if *prev == 0 { "snapshot" } else { "update" },
        ),
        MockMarket::Trade {
            symbol,
            px,
            qty,
            side,
            id,
            ts_ms,
        } => (
            *symbol,
            "publicTrade",
            format!(
                "{{\"p\":{},\"v\":{},\"S\":{},\"i\":\"{id}\",\"T\":\"{ts_ms}\"}}",
                quoted(px),
                quoted(qty),
                quoted(side)
            ),
            *ts_ms,
            "update",
        ),
        MockMarket::Mark {
            symbol,
            mark,
            index,
            rate,
            next_ms,
            ts_ms,
        } => (
            *symbol,
            "ticker",
            format!(
                "{{\"markPrice\":{},\"indexPrice\":{},\"fundingRate\":{},\"nextFundingTime\":\"{next_ms}\",\"ts\":\"{ts_ms}\"}}",
                quoted(mark),
                quoted(index),
                quoted(rate)
            ),
            *ts_ms,
            "snapshot",
        ),
    };
    write(
        out,
        format!(
            "{{\"arg\":{{\"topic\":{},\"symbol\":{},\"instType\":{}}},\"action\":\"{action}\",\"data\":[{data}],\"ts\":{ts}}}",
            quoted(topic),
            quoted(symbol),
            quoted(v.category())
        ),
    );
    MsgOp::Text
}
pub fn decode_order_frame(v: MockVenueKind, payload: &[u8]) -> Result<MockOrderReq, DecodeErr> {
    let req: MockOrderReq = sonic_rs::from_slice(payload).map_err(|_| DecodeErr::Shape)?;
    if req.op != "trade"
        || req.category != v.category()
        || req.args.len() != 1
        || !matches!(
            req.topic.as_str(),
            "place-order" | "modify-order" | "cancel-order"
        )
        || crate::cid::decode_req_id(req.id.as_bytes()).is_none()
    {
        return Err(DecodeErr::Shape);
    }
    Ok(req)
}
pub fn encode_order_event(v: MockVenueKind, ev: &MockOrderResp<'_>, out: &mut Vec<u8>) -> MsgOp {
    match ev {
        MockOrderResp::Reply {
            id,
            topic,
            code,
            client_oid,
            order_id,
            ts_ms,
        } => write(
            out,
            format!(
                "{{\"event\":\"trade\",\"id\":{},\"topic\":{},\"code\":{},\"args\":[{{\"clientOid\":{},\"orderId\":\"{order_id}\"}}],\"ts\":{ts_ms}}}",
                quoted(id),
                quoted(topic),
                quoted(code),
                quoted(client_oid)
            ),
        ),
        MockOrderResp::Order {
            symbol,
            client_oid,
            order_id,
            side,
            status,
            cum_qty,
            cum_value,
            avg_px,
            ts_ms,
        } => write(
            out,
            format!(
                "{{\"arg\":{{\"topic\":\"order\",\"instType\":\"UTA\"}},\"action\":\"update\",\"data\":[{{\"category\":{},\"symbol\":{},\"clientOid\":{},\"orderId\":\"{order_id}\",\"side\":{},\"orderStatus\":{},\"cumExecQty\":{},\"cumExecValue\":{},\"avgPrice\":{},\"updatedTime\":\"{ts_ms}\"}}],\"ts\":{ts_ms}}}",
                quoted(v.category()),
                quoted(symbol),
                quoted(client_oid),
                quoted(side),
                quoted(status),
                quoted(cum_qty),
                quoted(cum_value),
                quoted(avg_px)
            ),
        ),
    }
    MsgOp::Text
}
pub fn encode_rest_response(_v: MockVenueKind, req: &MockRestReq<'_>, out: &mut Vec<u8>) -> u16 {
    let (status, code, data) = match req {
        MockRestReq::Book { bids, asks, ts_ms } => (
            200,
            "00000",
            format!(
                "{{\"b\":{},\"a\":{},\"ts\":\"{ts_ms}\"}}",
                levels(bids),
                levels(asks)
            ),
        ),
        MockRestReq::Order {
            symbol,
            client_oid,
            order_id,
            status,
            cum_qty,
            avg_px,
            ts_ms,
        } => (
            200,
            "00000",
            format!(
                "{{\"symbol\":{},\"clientOid\":{},\"orderId\":\"{order_id}\",\"orderStatus\":{},\"cumExecQty\":{},\"avgPrice\":{},\"updatedTime\":\"{ts_ms}\"}}",
                quoted(symbol),
                quoted(client_oid),
                quoted(status),
                quoted(cum_qty),
                quoted(avg_px)
            ),
        ),
        MockRestReq::Error { status, code } => (*status, *code, "null".into()),
    };
    write(out, format!("{{\"code\":{},\"data\":{data}}}", quoted(code)));
    status
}
