//! UTA V3 warm account REST. Required risk fields and unknown nonzero positions fail closed.
use crate::json::from;
use hft_event::{AccountConfigReportHdr, BalanceReport, LeverageItem, MarginReportHdr, PositionReport};
use hft_refdata::{RefData, UidPolicy};
use hft_types::*;
use hft_venue_proto::{account::*, *};
use serde::{Deserialize, de::DeserializeOwned};

macro_rules! implementation {
    ($p:ty, $fut:expr) => {
        impl AccountProtocol for $p {
            type Funding = crate::BitgetUsdtFutures;
            fn account_request(
                c: &VenueCfg,
                k: &Credential,
                q: &Query,
                t: i64,
                out: &mut RestReqBuf,
            ) -> Result<(), ProtoErr> {
                request($fut, c, k, q, t, out)
            }
            fn account_decode(
                rd: &RefData,
                a: AccountId,
                q: &Query,
                status: u16,
                b: &[u8],
                t: i64,
                s: &mut State,
            ) -> Result<(), DecodeErr> {
                decode(
                    <Self as VenueProtocol>::VENUE,
                    $fut,
                    rd,
                    a,
                    q,
                    status,
                    b,
                    t,
                    s,
                )
            }
        }
    };
}
implementation!(crate::BitgetUsdtFutures, true);
implementation!(crate::BitgetSpot, false);

fn request(
    fut: bool,
    cfg: &VenueCfg,
    key: &Credential,
    q: &Query,
    now: i64,
    out: &mut RestReqBuf,
) -> Result<(), ProtoErr> {
    *out = RestReqBuf::default();
    let cat = if fut { "USDT-FUTURES" } else { "SPOT" };
    let path = match q {
        Query::Instruments => format!("/api/v3/market/instruments?category={cat}"),
        Query::Settings | Query::SymbolSettings => "/api/v3/account/settings".into(),
        Query::Permissions => "/api/v3/account/info".into(),
        Query::Balances => "/api/v3/account/assets".into(),
        Query::Positions if fut => format!("/api/v3/position/current-position?category={cat}"),
        Query::OpenOrders { cursor } => {
            if !cursor.is_empty() && !safe_token(cursor) {
                return Err(ProtoErr::Cfg);
            }
            format!(
                "/api/v3/trade/unfilled-orders?category={cat}&limit=100{}",
                if cursor.is_empty() {
                    String::new()
                } else {
                    format!("&cursor={cursor}")
                }
            )
        }
        Query::Cancel { symbol, order_id } if safe_token(symbol) && safe_token(order_id) => {
            out.method = Method::Post;
            out.body.push(
                format!("{{\"category\":\"{cat}\",\"symbol\":\"{symbol}\",\"orderId\":\"{order_id}\"}}")
                    .as_bytes(),
            )?;
            "/api/v3/trade/cancel-order".into()
        }
        _ => return Err(ProtoErr::Unsupported),
    };
    out.path_query.push(path.as_bytes())?;
    // Demo header also accompanies public instrument queries.
    let tpl = crate::orders::build_templates(cfg, AccountId(0), key)?;
    crate::orders::sign_headers(
        &tpl,
        key,
        &WallClock {
            unix_ms: now,
            ..Default::default()
        },
        out,
    )
}
#[derive(Deserialize)]
struct Envelope<T> {
    code: String,
    data: T,
}
fn data<T: DeserializeOwned>(b: &[u8]) -> Result<T, DecodeErr> {
    let e: Envelope<T> = from(b)?;
    if e.code != "00000" {
        return Err(DecodeErr::Shape);
    }
    Ok(e.data)
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Instrument {
    symbol: String,
    status: String,
    min_order_qty: String,
    min_order_amount: String,
    price_precision: String,
    quantity_precision: String,
    #[serde(default)]
    price_multiplier: Option<String>,
    #[serde(default)]
    quantity_multiplier: Option<String>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Settings {
    #[serde(default)]
    uid: String,
    account_mode: String,
    account_level: String,
    hold_mode: String,
    symbol_config_list: Vec<SymbolSettings>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SymbolSettings {
    symbol: String,
    category: String,
    margin_mode: String,
    leverage: String,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Permissions {
    #[serde(default)]
    user_id: String,
    perm_type: String,
    permissions: Vec<String>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Assets {
    usdt_equity: String,
    account_equity: String,
    mmr: String,
    assets: Vec<Asset>,
}
#[derive(Deserialize)]
struct Asset {
    coin: String,
    balance: String,
    available: String,
    locked: String,
    debt: String,
}
#[derive(Deserialize)]
struct List<T> {
    list: Vec<T>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Position {
    symbol: String,
    hold_mode: String,
    margin_mode: String,
    pos_side: String,
    total: String,
    avg_price: String,
    unrealised_pnl: String,
    liquidation_price: String,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Order {
    symbol: String,
    order_id: String,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Orders {
    list: Vec<Order>,
    #[serde(default)]
    cursor: Option<String>,
}

fn precision(s: &str) -> Result<String, DecodeErr> {
    let n = s.parse::<usize>().map_err(|_| DecodeErr::Number)?;
    if n > 18 {
        return Err(DecodeErr::Number);
    }
    Ok(if n == 0 {
        "1".into()
    } else {
        format!("0.{}1", "0".repeat(n - 1))
    })
}
fn identity(
    rd: &RefData,
    a: AccountId,
    v: VenueId,
    uid: &str,
    h: &mut AccountConfigReportHdr,
) -> Result<(), DecodeErr> {
    if uid.is_empty() && rd.account(a).uid_policy == UidPolicy::Unverifiable {
        h.venue_uid.fill(0);
        h.uid_keys = 0;
        return Ok(());
    }
    if rd.account(a).uid_policy == UidPolicy::Verified && rd.venue_uid(a, v) != Some(uid) {
        return Err(DecodeErr::Shape);
    }
    if uid.len() > 24 || !safe_token(uid) {
        return Err(DecodeErr::Shape);
    }
    h.venue_uid[..uid.len()].copy_from_slice(uid.as_bytes());
    h.uid_keys = 3;
    Ok(())
}
#[allow(clippy::too_many_arguments)]
fn decode(
    v: VenueId,
    fut: bool,
    rd: &RefData,
    a: AccountId,
    q: &Query,
    status: u16,
    b: &[u8],
    t: i64,
    s: &mut State,
) -> Result<(), DecodeErr> {
    if !(200..300).contains(&status) {
        return Err(DecodeErr::Shape);
    }
    let ns = t.checked_mul(1_000_000).ok_or(DecodeErr::Overflow)?;
    match q {
        Query::Instruments => {
            let rows: Vec<Instrument> = data(b)?;
            for i in rd.insts_of_venue(v) {
                let row = rows
                    .iter()
                    .find(|r| r.symbol == rd.inst_meta(*i).symbol.as_ref())
                    .ok_or(DecodeErr::Shape)?;
                if row.status != "online" {
                    return Err(DecodeErr::Shape);
                }
                let tick = match &row.price_multiplier {
                    Some(s) => s.clone(),
                    None => precision(&row.price_precision)?,
                };
                let lot = match &row.quantity_multiplier {
                    Some(s) => s.clone(),
                    None => precision(&row.quantity_precision)?,
                };
                rules(rd, *i, &tick, &lot, &row.min_order_qty, &row.min_order_amount)?;
            }
        }
        Query::Settings | Query::SymbolSettings => {
            let r: Settings = data(b)?;
            if r.account_mode != "unified"
                || r.account_level != "basic"
                || (fut && r.hold_mode != "one_way_mode")
            {
                return Err(DecodeErr::Shape);
            }
            // Basic account + zero debt (assets query); every spot order disables auto-borrow.
            let mut h = AccountConfigReportHdr {
                venue: v,
                account: a,
                position_mode: if fut { 2 } else { 1 },
                margin_mode: if fut { 2 } else { 1 },
                multi_asset: if fut { 2 } else { 1 },
                source: 1,
                ex_ts_ns: ns,
                ..zerocopy::FromZeros::new_zeroed()
            };
            identity(rd, a, v, &r.uid, &mut h)?;
            s.leverage.clear();
            for i in rd.insts_of_venue(v) {
                if !fut {
                    continue;
                }
                let r = r
                    .symbol_config_list
                    .iter()
                    .find(|r| r.category == "USDT-FUTURES" && r.symbol == rd.inst_meta(*i).symbol.as_ref())
                    .ok_or(DecodeErr::Shape)?;
                let lev = r.leverage.parse::<u16>().map_err(|_| DecodeErr::Number)?;
                if r.margin_mode != "crossed" || lev == 0 || lev > rd.inst_meta(*i).max_leverage {
                    return Err(DecodeErr::Shape);
                }
                s.leverage.push(LeverageItem {
                    inst: *i,
                    leverage: Rate(i64::from(lev) * 1_000_000_000),
                    ..zerocopy::FromZeros::new_zeroed()
                });
            }
            s.config = Some(h);
        }
        Query::Permissions => {
            let r: Permissions = data(b)?;
            if r.perm_type != "read-and-write"
                || !r.permissions.iter().any(|x| x == "uta_trade")
                || r.permissions.iter().any(|x| x == "withdraw")
            {
                return Err(DecodeErr::Shape);
            }
            let h = s.config.as_mut().ok_or(DecodeErr::Shape)?;
            let old = h.venue_uid;
            h.venue_uid.fill(0);
            identity(rd, a, v, &r.user_id, h)?;
            let absent = old == [0; 24] || h.venue_uid == [0; 24];
            if absent && rd.account(a).uid_policy == UidPolicy::Unverifiable {
                h.venue_uid.fill(0);
                h.uid_keys = 0;
            } else if old != h.venue_uid {
                return Err(DecodeErr::Shape);
            }
        }
        Query::Balances => {
            let r: Assets = data(b)?;
            s.balances.clear();
            let mut available = 0;
            for row in &r.assets {
                if decimal(&row.debt, -8)? != 0 {
                    return Err(DecodeErr::Shape);
                }
                if let Some(asset) = asset(rd, &row.coin) {
                    let exp = rd.asset(asset).exp.qty_exp;
                    // A futures wallet total excludes unrealized PnL; available is a margin fact.
                    let total = decimal(&row.balance, exp)?;
                    let locked = if fut { 0 } else { decimal(&row.locked, exp)? };
                    s.balances.push(BalanceReport {
                        venue: v,
                        account: a,
                        asset,
                        free: Qty(total.checked_sub(locked).ok_or(DecodeErr::Overflow)?),
                        locked: Qty(locked),
                        ex_ts_ns: ns,
                        ..zerocopy::FromZeros::new_zeroed()
                    });
                }
                if row.coin == "USDT" {
                    available = notional(&row.available)?;
                }
            }
            // Omitted assets are explicitly zero, including the disappearance of a previous balance.
            for i in rd.insts_of_venue(v) {
                for asset in [rd.inst(*i).asset, rd.inst(*i).quote] {
                    if !s.balances.iter().any(|r| r.asset == asset) {
                        s.balances.push(BalanceReport {
                            venue: v,
                            account: a,
                            asset,
                            ex_ts_ns: ns,
                            ..zerocopy::FromZeros::new_zeroed()
                        });
                    }
                }
            }
            if fut {
                let equity = notional(&r.usdt_equity)?;
                let usd_equity = notional(&r.account_equity)?;
                let usd_maint = notional(&r.mmr)?;
                if equity < 0
                    || usd_equity < 0
                    || usd_maint < 0
                    || (usd_equity == 0 && (equity != 0 || usd_maint != 0))
                {
                    return Err(DecodeErr::Shape);
                }
                // mmr and accountEquity are USD; the event carries USDT. Round maintenance up.
                let maint = if usd_equity == 0 {
                    0
                } else {
                    i64::try_from(
                        (i128::from(usd_maint) * i128::from(equity) + i128::from(usd_equity) - 1)
                            / i128::from(usd_equity),
                    )
                    .map_err(|_| DecodeErr::Overflow)?
                };
                s.margin = Some(MarginReportHdr {
                    venue: v,
                    account: a,
                    source: 2,
                    margin_balance: Notional(equity),
                    maint_margin: Notional(maint),
                    avail_margin: Notional(available),
                    margin_ratio: Rate(if equity > 0 {
                        (i128::from(maint) * 1_000_000_000 / i128::from(equity)).min(i128::from(i64::MAX))
                            as i64
                    } else {
                        i64::MAX
                    }),
                    ex_ts_ns: ns,
                    ..zerocopy::FromZeros::new_zeroed()
                });
            }
        }
        Query::Positions => {
            let rows: List<Position> = data(b)?;
            s.positions.clear();
            for r in rows.list {
                if r.hold_mode != "one_way_mode" || r.margin_mode != "crossed" {
                    return Err(DecodeErr::Shape);
                }
                let i = instrument(rd, v, &r.symbol)?;
                let h = rd.inst(i);
                let qty = decimal(&r.total, h.qty_wire_exp)?;
                if qty < 0 {
                    return Err(DecodeErr::Shape);
                }
                let qty = match r.pos_side.as_str() {
                    "long" => qty,
                    "short" => qty.checked_neg().ok_or(DecodeErr::Overflow)?,
                    _ => return Err(DecodeErr::Shape),
                };
                if s.positions.iter().any(|p| p.inst == i) {
                    return Err(DecodeErr::Shape);
                }
                s.positions.push(PositionReport {
                    venue: v,
                    account: a,
                    inst: i,
                    qty: Qty(qty),
                    entry_px: Px(average_price(&r.avg_price, h.px_wire_exp)?),
                    upnl: Notional(notional(&r.unrealised_pnl)?),
                    liq_px: Px(if r.liquidation_price.is_empty() {
                        0
                    } else {
                        average_price(&r.liquidation_price, h.px_wire_exp)?
                    }),
                    ex_ts_ns: ns,
                    ..zerocopy::FromZeros::new_zeroed()
                });
            }
            for i in rd.insts_of_venue(v) {
                if !s.positions.iter().any(|p| p.inst == *i) {
                    s.positions.push(PositionReport {
                        venue: v,
                        account: a,
                        inst: *i,
                        ex_ts_ns: ns,
                        ..zerocopy::FromZeros::new_zeroed()
                    });
                }
            }
            s.position_margin_items();
        }
        Query::OpenOrders { .. } => {
            let r: Orders = data(b)?;
            s.orders = r
                .list
                .into_iter()
                .map(|r| OpenOrder {
                    symbol: r.symbol,
                    order_id: r.order_id,
                })
                .collect();
            s.cursor = r.cursor.unwrap_or_default();
        }
        Query::Cancel { .. } => {
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase")]
            struct Cancel {
                order_id: String,
            }
            let r: Cancel = data(b)?;
            if r.order_id.is_empty() {
                return Err(DecodeErr::Shape);
            }
        }
        _ => return Err(DecodeErr::UnknownClass),
    }
    Ok(())
}
