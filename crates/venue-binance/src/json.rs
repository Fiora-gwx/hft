//! sonic-rs per-message structs over borrowed strings (README "2. 解码") and the error mapping
//! of contracts 4 and 5.

use core::fmt;

use hft_types::{FxErr, parse_exact};
use hft_venue_proto::DecodeErr;
use serde::Deserialize;
use serde::de::{Deserializer, IgnoredAny, MapAccess, SeqAccess, Visitor};

/// Parses `p` into `T`; the input ending early → `Truncated`, anything else → `Shape`.
pub(crate) fn from<'a, T: Deserialize<'a>>(p: &'a [u8]) -> Result<T, DecodeErr> {
    sonic_rs::from_slice::<T>(p).map_err(|e| {
        if e.is_eof() {
            DecodeErr::Truncated
        } else {
            DecodeErr::Shape
        }
    })
}

/// Decimal string → fixed point at `exp` (contract 4).
pub(crate) fn fixed(s: &str, exp: i8) -> Result<i64, DecodeErr> {
    parse_exact(s.as_bytes(), exp).map_err(fx)
}

pub(crate) fn fx(e: FxErr) -> DecodeErr {
    if e.0 & FxErr::OVERFLOW.0 != 0 {
        DecodeErr::Overflow
    } else {
        DecodeErr::Number
    }
}

/// Exchange time in `unit_ns` units → ns; overflow → `Overflow` (contract 3: never re-guessed).
pub(crate) fn time_ns(v: i64, unit_ns: i64) -> Result<i64, DecodeErr> {
    v.checked_mul(unit_ns).ok_or(DecodeErr::Overflow)
}

/// `{"code":…,"msg":…}`.
#[derive(Deserialize)]
pub(crate) struct ErrBody {
    pub code: i64,
}

/// One `rateLimits[]` entry.
#[derive(Deserialize)]
struct RateItem<'a> {
    #[serde(rename = "rateLimitType", borrow)]
    kind: &'a str,
    #[serde(borrow)]
    interval: &'a str,
    #[serde(rename = "intervalNum")]
    interval_num: u32,
    limit: u32,
    count: u32,
}

/// Bucket of a `rateLimitType` (README "限速默认值").
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub(crate) enum RateKind {
    Orders,
    Weight,
}

/// The entry with the highest `count / limit` (ties: shortest window) — README "限速默认值".
#[derive(Copy, Clone, Eq, PartialEq, Debug, Default)]
pub(crate) struct RateSel {
    pub best: Option<(RateKind, u32, u32, u32)>,
}

impl<'de> Deserialize<'de> for RateSel {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct V;
        impl<'de> Visitor<'de> for V {
            type Value = RateSel;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("rateLimits array")
            }
            fn visit_seq<A: SeqAccess<'de>>(self, mut s: A) -> Result<RateSel, A::Error> {
                let mut best: Option<(RateKind, u32, u32, u32)> = None;
                while let Some(it) = s.next_element::<RateItem<'de>>()? {
                    let kind = match it.kind {
                        "ORDERS" => RateKind::Orders,
                        "REQUEST_WEIGHT" => RateKind::Weight,
                        _ => continue,
                    };
                    let unit: u32 = match it.interval {
                        "SECOND" => 1_000,
                        "MINUTE" => 60_000,
                        "HOUR" => 3_600_000,
                        "DAY" => 86_400_000,
                        _ => continue,
                    };
                    if it.limit == 0 {
                        continue;
                    }
                    let win = unit.saturating_mul(it.interval_num);
                    let cand = (kind, it.count, it.limit, win);
                    best = match best {
                        None => Some(cand),
                        Some(b) => {
                            // count/limit compared exactly: c1 × l2 vs c2 × l1.
                            let lhs = u64::from(it.count) * u64::from(b.2);
                            let rhs = u64::from(b.1) * u64::from(it.limit);
                            if lhs > rhs || (lhs == rhs && win < b.3) {
                                Some(cand)
                            } else {
                                Some(b)
                            }
                        }
                    };
                }
                Ok(RateSel { best })
            }
        }
        d.deserialize_seq(V)
    }
}

/// `result` of a WS API response: an object (order answers) or anything else (an array for
/// `openOrders.cancelAll`), which is skipped.
#[derive(Default)]
pub(crate) struct AnyResult<'a> {
    pub obj: Option<OrderResult<'a>>,
}

impl<'de: 'a, 'a> Deserialize<'de> for AnyResult<'a> {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct V<'a>(core::marker::PhantomData<&'a ()>);
        impl<'de: 'a, 'a> Visitor<'de> for V<'a> {
            type Value = AnyResult<'a>;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("result")
            }
            fn visit_map<A: MapAccess<'de>>(self, m: A) -> Result<AnyResult<'a>, A::Error> {
                let r = OrderResult::deserialize(serde::de::value::MapAccessDeserializer::new(m))?;
                Ok(AnyResult { obj: Some(r) })
            }
            fn visit_seq<A: SeqAccess<'de>>(self, mut s: A) -> Result<AnyResult<'a>, A::Error> {
                while s.next_element::<IgnoredAny>()?.is_some() {}
                Ok(AnyResult { obj: None })
            }
            fn visit_unit<E: serde::de::Error>(self) -> Result<AnyResult<'a>, E> {
                Ok(AnyResult { obj: None })
            }
            fn visit_none<E: serde::de::Error>(self) -> Result<AnyResult<'a>, E> {
                Ok(AnyResult { obj: None })
            }
        }
        d.deserialize_any(V(core::marker::PhantomData))
    }
}

/// The order fields of WS API answers (`order.place` ACK, `order.cancel`, `order.modify`,
/// `order.status`; spot `order.amend.keepPriority` nests the order in `amendedOrder`).
#[derive(Deserialize, Default)]
pub(crate) struct OrderResult<'a> {
    #[serde(rename = "orderId", default)]
    pub order_id: Option<u64>,
    #[serde(default, borrow)]
    pub symbol: Option<&'a str>,
    #[serde(default, borrow)]
    pub status: Option<&'a str>,
    #[serde(rename = "clientOrderId", default, borrow)]
    pub client_order_id: Option<&'a str>,
    #[serde(rename = "origClientOrderId", default, borrow)]
    pub orig_client_order_id: Option<&'a str>,
    #[serde(rename = "executedQty", default, borrow)]
    pub executed_qty: Option<&'a str>,
    #[serde(rename = "avgPrice", default, borrow)]
    pub avg_price: Option<&'a str>,
    #[serde(rename = "cummulativeQuoteQty", default, borrow)]
    pub cum_quote: Option<&'a str>,
    #[serde(rename = "updateTime", default)]
    pub update_time: Option<i64>,
    #[serde(rename = "transactTime", default)]
    pub transact_time: Option<i64>,
    #[serde(rename = "amendedOrder", default, borrow)]
    pub amended: Option<AmendedOrder<'a>>,
}

/// Spot `order.amend.keepPriority` result body.
#[derive(Deserialize, Default)]
pub(crate) struct AmendedOrder<'a> {
    #[serde(rename = "orderId", default)]
    pub order_id: Option<u64>,
    #[serde(default, borrow)]
    pub symbol: Option<&'a str>,
    #[serde(rename = "origClientOrderId", default, borrow)]
    pub orig_client_order_id: Option<&'a str>,
}

/// A WS API response.
#[derive(Deserialize)]
pub(crate) struct WsResp<'a> {
    #[serde(borrow)]
    pub id: &'a str,
    pub status: u16,
    #[serde(default, borrow)]
    pub result: Option<AnyResult<'a>>,
    #[serde(default)]
    pub error: Option<ErrBody>,
    #[serde(rename = "rateLimits", default)]
    pub rate_limits: Option<RateSel>,
}
