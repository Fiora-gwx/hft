//! Warm account REST (README "账户配置核验"): the startup baseline (server reachable, API key
//! of this account, not in closed-only mode, no open order, pUSD balance) and the periodic
//! balance poll. Fails closed on any unexpected answer.

use hft_event::{AccountConfigReportHdr, BalanceReport};
use hft_refdata::{RefData, UidPolicy};
use hft_types::{AccountId, Qty, VenueId, parse_exact};
use hft_venue_proto::account::{OpenOrder, Query, State};
use hft_venue_proto::{Credential, DecodeErr, Method, ProtoErr, RestReqBuf, WallClock};
use serde::Deserialize;

use crate::orders::{keys, sign_headers};

/// Cursor of the first and past the last page of `/data/orders`.
const FIRST: &str = "MA==";
const END: &str = "LTE=";

/// A cursor or order hash safe to put into a path or a JSON string.
fn safe(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 128
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"=+/_-x".contains(&b))
}

pub(crate) fn request(
    cred: &Credential,
    q: &Query,
    now_ms: i64,
    out: &mut RestReqBuf,
) -> Result<(), ProtoErr> {
    *out = RestReqBuf::default();
    let path: String = match q {
        // Public: the server is reachable (slot insts are bound at run time, nothing to list).
        Query::Instruments => {
            out.path_query.push(b"/time")?;
            return Ok(());
        }
        Query::Settings => "/auth/api-keys".into(),
        Query::Permissions => "/auth/ban-status/closed-only".into(),
        Query::Balances => "/balance-allowance?asset_type=COLLATERAL&signature_type=3".into(),
        Query::OpenOrders { cursor } => {
            let c = if cursor.is_empty() { FIRST } else { cursor.as_str() };
            if !safe(c) {
                return Err(ProtoErr::Cfg);
            }
            format!("/data/orders?next_cursor={c}")
        }
        Query::Cancel { order_id, .. } if safe(order_id) => {
            out.method = Method::Delete;
            out.body
                .push(format!("{{\"orderID\":\"{order_id}\"}}").as_bytes())?;
            "/order".into()
        }
        _ => return Err(ProtoErr::Unsupported),
    };
    out.path_query.push(path.as_bytes())?;
    let k = keys(cred)?;
    sign_headers(
        &k.wallet.address(),
        k.api_secret,
        cred,
        &WallClock {
            unix_ms: now_ms,
            unix_us: now_ms.saturating_mul(1000),
        },
        // hft-venue's account client writes its own Content-Type.
        false,
        out,
    )
}

#[derive(Deserialize)]
struct ApiKeys {
    #[serde(rename = "apiKeys")]
    api_keys: Vec<String>,
}

#[derive(Deserialize)]
struct ClosedOnly {
    closed_only: bool,
}

#[derive(Deserialize)]
struct Orders {
    data: Vec<Order>,
    next_cursor: String,
}

#[derive(Deserialize)]
struct Order {
    id: String,
    #[serde(default)]
    asset_id: String,
}

#[derive(Deserialize)]
struct Balance {
    balance: String,
}

fn from<'a, T: Deserialize<'a>>(b: &'a [u8]) -> Result<T, DecodeErr> {
    sonic_rs::from_slice(b).map_err(|_| DecodeErr::Shape)
}

/// The venue UID of a Polymarket account is the first 24 characters of its CLOB API key
/// (README "账户配置核验"): `/auth/api-keys` lists the keys of the wallet that signed the
/// request, so the configured UID must prefix one of them.
fn identity(
    rd: &RefData,
    a: AccountId,
    v: VenueId,
    keys: &[String],
    h: &mut AccountConfigReportHdr,
) -> Result<(), DecodeErr> {
    let want = rd.venue_uid(a, v).unwrap_or("");
    let hit = !want.is_empty() && want.len() <= 24 && keys.iter().any(|k| k.starts_with(want));
    if !hit {
        if rd.account(a).uid_policy == UidPolicy::Unverifiable {
            h.venue_uid.fill(0);
            h.uid_keys = 0;
            return Ok(());
        }
        return Err(DecodeErr::Shape);
    }
    h.venue_uid.fill(0);
    h.venue_uid[..want.len()].copy_from_slice(want.as_bytes());
    h.uid_keys = 3;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn decode(
    v: VenueId,
    rd: &RefData,
    a: AccountId,
    q: &Query,
    status: u16,
    b: &[u8],
    now_ms: i64,
    s: &mut State,
) -> Result<(), DecodeErr> {
    if !(200..300).contains(&status) {
        return Err(DecodeErr::Shape);
    }
    let ns = now_ms.checked_mul(1_000_000).ok_or(DecodeErr::Overflow)?;
    match q {
        Query::Instruments => {
            let t: i64 = core::str::from_utf8(b)
                .map_err(|_| DecodeErr::Shape)?
                .trim()
                .parse()
                .map_err(|_| DecodeErr::Number)?;
            if t <= 0 {
                return Err(DecodeErr::Shape);
            }
        }
        Query::Settings => {
            let r: ApiKeys = from(b)?;
            // Spot-like outcome tokens: one-way, cash, single asset (no margin modes).
            let mut h = AccountConfigReportHdr {
                venue: v,
                account: a,
                position_mode: 1,
                margin_mode: 1,
                multi_asset: 1,
                source: 1,
                ex_ts_ns: ns,
                ..zerocopy::FromZeros::new_zeroed()
            };
            identity(rd, a, v, &r.api_keys, &mut h)?;
            s.leverage.clear();
            s.config = Some(h);
        }
        Query::Permissions => {
            let r: ClosedOnly = from(b)?;
            if r.closed_only || s.config.is_none() {
                return Err(DecodeErr::Shape);
            }
        }
        Query::OpenOrders { cursor } => {
            let r: Orders = from(b)?;
            let mut orders = Vec::with_capacity(r.data.len());
            for o in r.data {
                if !safe(&o.id) || orders.iter().any(|x: &OpenOrder| x.order_id == o.id) {
                    return Err(DecodeErr::Shape);
                }
                orders.push(OpenOrder {
                    symbol: o.asset_id,
                    order_id: o.id,
                });
            }
            let next = if r.next_cursor == END {
                String::new()
            } else {
                r.next_cursor
            };
            if !next.is_empty() && (next == *cursor || !safe(&next)) {
                return Err(DecodeErr::Shape);
            }
            s.orders = orders;
            s.cursor = next;
        }
        Query::Balances => {
            let r: Balance = from(b)?;
            s.balances.clear();
            // pUSD (6 decimals) is the quote of every slot inst; outcome-token balances are
            // per window and not part of this poll (README "账户配置核验").
            let mut seen = Vec::new();
            for i in rd.insts_of_venue(v) {
                let asset = rd.inst(*i).quote;
                if seen.contains(&asset) {
                    continue;
                }
                seen.push(asset);
                let exp = rd.asset(asset).exp.qty_exp;
                let q = parse_exact(r.balance.as_bytes(), 0).map_err(|_| DecodeErr::Number)?;
                // The answer is in 1e-6 units.
                let shift = i32::from(exp) + 6;
                let q = if shift <= 0 {
                    q.checked_mul(
                        10i64
                            .checked_pow(shift.unsigned_abs())
                            .ok_or(DecodeErr::Overflow)?,
                    )
                    .ok_or(DecodeErr::Overflow)?
                } else {
                    let p = 10i64
                        .checked_pow(shift.unsigned_abs())
                        .ok_or(DecodeErr::Overflow)?;
                    if q % p != 0 {
                        return Err(DecodeErr::Number);
                    }
                    q / p
                };
                if q < 0 {
                    return Err(DecodeErr::Shape);
                }
                s.balances.push(BalanceReport {
                    venue: v,
                    account: a,
                    asset,
                    free: Qty(q),
                    locked: Qty(0),
                    ex_ts_ns: ns,
                    ..zerocopy::FromZeros::new_zeroed()
                });
            }
        }
        Query::Cancel { order_id, .. } => {
            #[derive(Deserialize)]
            struct Cancelled {
                canceled: Vec<String>,
            }
            let r: Cancelled = from(b)?;
            if !r.canceled.iter().any(|x| x == order_id) {
                return Err(DecodeErr::Shape);
            }
        }
        _ => return Err(DecodeErr::UnknownClass),
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::safe;

    #[test]
    fn cursor_and_hash_shapes() {
        assert!(safe("MA=="));
        assert!(safe("LTE="));
        assert!(safe(
            "0x766cf144ea53ad8d1726f1a230dc5e5535d8eb296d114aee313cd83520685f6e"
        ));
        assert!(!safe("a\"b"));
        assert!(!safe(""));
    }
}
