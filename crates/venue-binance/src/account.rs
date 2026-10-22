//! Signed warm REST account bootstrap. The HTTP status and required fields fail closed.
use crate::json::from;
use hft_event::{AccountConfigReportHdr, BalanceReport, LeverageItem, MarginReportHdr, PositionReport};
use hft_refdata::{RefData, UidPolicy};
use hft_types::*;
use hft_venue_proto::{account::*, *};
use serde::Deserialize;

macro_rules! implementation {
    ($p:ty, $fut:expr) => {
        impl AccountProtocol for $p {
            type Funding = crate::Usdm;
            fn funding_bootstrap_queries(rd: &RefData) -> Vec<Query> {
                if !$fut {
                    return Vec::new();
                }
                let mut queries = vec![Query::FundingInfo];
                queries.extend(
                    rd.insts_of_venue(Self::VENUE)
                        .iter()
                        .map(|i| Query::FundingSchedule {
                            symbol: rd.inst_meta(*i).symbol.to_string(),
                        }),
                );
                queries
            }
            fn seed_funding(ctx: &mut Self::FeedCtx, rows: &[FundingSchedule]) -> Result<(), DecodeErr> {
                for row in rows {
                    ctx.seed_funding(row.inst, row.next_ms, row.interval_s)?;
                }
                Ok(())
            }
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
implementation!(crate::Usdm, true);
implementation!(crate::Spot, false);

pub(crate) fn request(
    fut: bool,
    cfg: &VenueCfg,
    k: &Credential,
    q: &Query,
    now: i64,
    out: &mut RestReqBuf,
) -> Result<(), ProtoErr> {
    *out = RestReqBuf::default();
    let base = if fut { "/fapi" } else { "/api" };
    let path = match q {
        Query::Instruments => format!("{base}/v{}/exchangeInfo", if fut { 1 } else { 3 }),
        Query::Settings | Query::Permissions => {
            if fut {
                "/fapi/v1/accountConfig".into()
            } else {
                "/api/v3/account".into()
            }
        }
        Query::SymbolSettings if fut => "/fapi/v1/symbolConfig".into(),
        Query::SymbolSettings => return Err(ProtoErr::Unsupported),
        Query::Balances => {
            if fut {
                "/fapi/v3/account".into()
            } else {
                "/api/v3/account".into()
            }
        }
        Query::Positions if fut => "/fapi/v3/positionRisk".into(),
        Query::Positions => return Err(ProtoErr::Unsupported),
        Query::FundingInfo if fut => "/fapi/v1/fundingInfo".into(),
        Query::FundingSchedule { symbol } if fut && safe_token(symbol) => {
            format!("/fapi/v1/premiumIndex?symbol={symbol}")
        }
        Query::DeadMan { symbol, timeout_ms } if fut && safe_token(symbol) => {
            out.method = Method::Post;
            format!("/fapi/v1/countdownCancelAll?symbol={symbol}&countdownTime={timeout_ms}")
        }
        Query::OpenOrders { cursor } if cursor.is_empty() => {
            format!("{base}/v{}/openOrders", if fut { 1 } else { 3 })
        }
        Query::Cancel { symbol, order_id } if safe_token(symbol) && safe_token(order_id) => {
            out.method = Method::Delete;
            format!(
                "{base}/v{}/order?symbol={symbol}&orderId={order_id}",
                if fut { 1 } else { 3 }
            )
        }
        Query::ListenKey | Query::Keepalive if fut => {
            out.method = if matches!(q, Query::ListenKey) {
                Method::Post
            } else {
                Method::Put
            };
            "/fapi/v1/listenKey".into()
        }
        _ => return Err(ProtoErr::Cfg),
    };
    out.path_query.push(path.as_bytes())?;
    if matches!(q, Query::Instruments) || q.optional_funding() {
        return Ok(());
    }
    out.headers.push(b"X-MBX-APIKEY: ")?;
    out.headers.push(k.api_key.as_slice())?;
    out.headers.push(b"\r\n")?;
    out.redacted = true;
    if matches!(q, Query::ListenKey | Query::Keepalive) {
        return Ok(());
    }
    sign_query(cfg, k, now, out)
}

pub(crate) fn sign_request(
    cfg: &VenueCfg,
    k: &Credential,
    now: i64,
    out: &mut RestReqBuf,
) -> Result<(), ProtoErr> {
    if k.api_key.is_empty() || k.secret.0.is_empty() || now < 0 || cfg.stale_guard_ms == 0 {
        return Err(ProtoErr::Cfg);
    }
    out.headers.push(b"X-MBX-APIKEY: ")?;
    out.headers.push(k.api_key.as_slice())?;
    out.headers.push(b"\r\n")?;
    out.redacted = true;
    sign_query(cfg, k, now, out)
}

fn sign_query(cfg: &VenueCfg, k: &Credential, now: i64, out: &mut RestReqBuf) -> Result<(), ProtoErr> {
    let path = core::str::from_utf8(out.path_query.as_slice())
        .map_err(|_| ProtoErr::Cfg)?
        .to_owned();
    let args = format!("timestamp={now}&recvWindow={}", cfg.stale_guard_ms);
    let mut query = path
        .split_once('?')
        .map_or(String::new(), |(_, v)| format!("{v}&"));
    query.push_str(&args);
    let sig = match k.alg {
        SignAlg::Ed25519 => {
            let b = crate::sign::ed25519_b64(k.secret.0.as_slice(), query.as_bytes()).ok_or(ProtoErr::Cfg)?;
            b.iter()
                .map(|b| {
                    if b.is_ascii_alphanumeric() {
                        (*b as char).to_string()
                    } else {
                        format!("%{b:02X}")
                    }
                })
                .collect::<String>()
        }
        SignAlg::HmacSha256 => {
            use hmac::{Hmac, Mac};
            let mut mac =
                Hmac::<sha2::Sha256>::new_from_slice(k.secret.0.as_slice()).map_err(|_| ProtoErr::Cfg)?;
            mac.update(query.as_bytes());
            mac.finalize()
                .into_bytes()
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect::<String>()
        }
        SignAlg::Secp256k1 => return Err(ProtoErr::Cfg),
    };
    out.path_query
        .push(if path.contains('?') { b"&" } else { b"?" })?;
    out.path_query.push(args.as_bytes())?;
    out.path_query.push(b"&signature=")?;
    out.path_query.push(sig.as_bytes())
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Filter {
    filter_type: String,
    #[serde(default)]
    tick_size: String,
    #[serde(default)]
    step_size: String,
    #[serde(default)]
    min_qty: String,
    #[serde(default)]
    min_notional: String,
    #[serde(default)]
    notional: String,
}
#[derive(Deserialize)]
struct Symbol {
    symbol: String,
    status: String,
    filters: Vec<Filter>,
}
#[derive(Deserialize)]
struct Instruments {
    symbols: Vec<Symbol>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Settings {
    can_trade: bool,
    #[serde(default)]
    dual_side_position: Option<bool>,
    #[serde(default)]
    multi_assets_margin: Option<bool>,
    #[serde(default)]
    uid: Option<Uid>,
}
#[derive(Deserialize)]
#[serde(untagged)]
enum Uid {
    Number(u64),
    Text(String),
}
impl Uid {
    fn text(self) -> String {
        match self {
            Self::Number(n) => n.to_string(),
            Self::Text(s) => s,
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SymbolSetting {
    symbol: String,
    margin_type: String,
    leverage: u16,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Account {
    total_margin_balance: String,
    total_maint_margin: String,
    available_balance: String,
    assets: Vec<Balance>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Balance {
    asset: String,
    wallet_balance: String,
}
#[derive(Deserialize)]
struct SpotBalance {
    asset: String,
    free: String,
    locked: String,
}
#[derive(Deserialize)]
struct SpotAccount {
    balances: Vec<SpotBalance>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Position {
    symbol: String,
    position_side: String,
    position_amt: String,
    entry_price: String,
    un_realized_profit: String,
    liquidation_price: String,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Order {
    symbol: String,
    order_id: u64,
}

// Each REST page is a transaction. A malformed later row must not replace a validated baseline.
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
    if q.optional_funding() {
        return funding_schedule(v, rd, q, status, b, t, s);
    }
    let mut next = State {
        config: s.config,
        ..State::default()
    };
    decode_page(v, fut, rd, a, q, status, b, t, &mut next)?;
    for (n, row) in next.balances.iter().enumerate() {
        if row.locked.0 < 0
            || (!fut && row.free.0 < 0)
            || next.balances[..n].iter().any(|old| old.asset == row.asset)
        {
            return Err(DecodeErr::Shape);
        }
    }
    if next.positions.iter().any(|p| p.entry_px.0 < 0 || p.liq_px.0 < 0) {
        return Err(DecodeErr::Shape);
    }
    for (n, row) in next.orders.iter().enumerate() {
        if !safe_token(&row.symbol)
            || !safe_token(&row.order_id)
            || !row.order_id.parse::<u64>().is_ok_and(|id| id != 0)
            || next.orders[..n]
                .iter()
                .any(|old| old.symbol == row.symbol && old.order_id == row.order_id)
        {
            return Err(DecodeErr::Shape);
        }
    }
    match q {
        Query::Settings | Query::Permissions => s.config = next.config,
        Query::SymbolSettings => s.leverage = next.leverage,
        Query::Balances => {
            s.balances = next.balances;
            s.margin = next.margin;
        }
        Query::Positions => {
            s.positions = next.positions;
            s.margin_items = next.margin_items;
        }
        Query::OpenOrders { cursor } => {
            if !next.cursor.is_empty() && (next.cursor == *cursor || !safe_token(&next.cursor)) {
                return Err(DecodeErr::Shape);
            }
            s.orders = next.orders;
            s.cursor = next.cursor;
        }
        _ => {}
    }
    Ok(())
}

fn funding_schedule(
    v: VenueId,
    rd: &RefData,
    q: &Query,
    status: u16,
    b: &[u8],
    now: i64,
    s: &mut State,
) -> Result<(), DecodeErr> {
    if status != 200 {
        return Err(DecodeErr::Shape);
    }
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Info {
        symbol: String,
        funding_interval_hours: u32,
    }
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Premium {
        symbol: String,
        next_funding_time: i64,
    }
    match q {
        Query::FundingInfo => {
            let rows: Vec<Info> = from(b)?;
            // fundingInfo lists adjusted contracts only; the documented standard interval is 8h.
            let mut intervals: Vec<_> = rd.insts_of_venue(v).iter().map(|i| (*i, 28_800)).collect();
            let mut seen = Vec::new();
            for row in rows {
                if !(1..=24).contains(&row.funding_interval_hours) {
                    return Err(DecodeErr::Shape);
                }
                if let Ok(inst) = instrument(rd, v, &row.symbol) {
                    if seen.contains(&inst) {
                        return Err(DecodeErr::Shape);
                    }
                    seen.push(inst);
                    let item = intervals
                        .iter_mut()
                        .find(|x| x.0 == inst)
                        .ok_or(DecodeErr::Shape)?;
                    item.1 = row.funding_interval_hours * 3600;
                }
            }
            s.funding_intervals = intervals;
        }
        Query::FundingSchedule { symbol } => {
            let row: Premium = from(b)?;
            if row.symbol != *symbol {
                return Err(DecodeErr::Shape);
            }
            let inst = instrument(rd, v, symbol)?;
            let interval_s = s
                .funding_intervals
                .iter()
                .find(|x| x.0 == inst)
                .ok_or(DecodeErr::Shape)?
                .1;
            let until = row.next_funding_time.checked_sub(now).ok_or(DecodeErr::Number)?;
            if until <= 0 || until > i64::from(interval_s) * 1000 {
                return Err(DecodeErr::Shape);
            }
            s.funding_schedule.push(FundingSchedule {
                inst,
                next_ms: row.next_funding_time,
                interval_s,
            });
        }
        _ => return Err(DecodeErr::Shape),
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn decode_page(
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
        Query::FundingInfo | Query::FundingSchedule { .. } => return Err(DecodeErr::Shape),
        Query::Instruments => {
            let rows: Instruments = from(b)?;
            for i in rd.insts_of_venue(v) {
                let r = rows
                    .symbols
                    .iter()
                    .find(|r| r.symbol == rd.inst_meta(*i).symbol.as_ref())
                    .ok_or(DecodeErr::Shape)?;
                if r.status != "TRADING" {
                    return Err(DecodeErr::Shape);
                }
                let price = r
                    .filters
                    .iter()
                    .find(|f| f.filter_type == "PRICE_FILTER")
                    .ok_or(DecodeErr::Shape)?;
                let lot = r
                    .filters
                    .iter()
                    .find(|f| f.filter_type == "LOT_SIZE")
                    .ok_or(DecodeErr::Shape)?;
                let min = r
                    .filters
                    .iter()
                    .find(|f| f.filter_type == "MIN_NOTIONAL" || f.filter_type == "NOTIONAL")
                    .ok_or(DecodeErr::Shape)?;
                rules(
                    rd,
                    *i,
                    &price.tick_size,
                    &lot.step_size,
                    &lot.min_qty,
                    if min.min_notional.is_empty() {
                        &min.notional
                    } else {
                        &min.min_notional
                    },
                )?;
            }
        }
        Query::Settings | Query::Permissions => {
            let r: Settings = from(b)?;
            if !r.can_trade
                || (fut && (r.dual_side_position != Some(false) || r.multi_assets_margin != Some(false)))
            {
                return Err(DecodeErr::Shape);
            }
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
            match r.uid {
                Some(uid) => {
                    let uid = uid.text();
                    if !safe_token(&uid)
                        || uid.len() > h.venue_uid.len()
                        || (rd.account(a).uid_policy == UidPolicy::Verified
                            && rd.venue_uid(a, v) != Some(uid.as_str()))
                    {
                        return Err(DecodeErr::Shape);
                    }
                    h.venue_uid[..uid.len()].copy_from_slice(uid.as_bytes());
                    h.uid_keys = 3;
                }
                None if rd.account(a).uid_policy == UidPolicy::Unverifiable => {}
                None => return Err(DecodeErr::Shape),
            }
            if matches!(q, Query::Permissions)
                && let Some(previous) = s.config.as_ref()
                && previous.venue_uid != [0; 24]
                && h.venue_uid != [0; 24]
                && previous.venue_uid != h.venue_uid
            {
                return Err(DecodeErr::Shape);
            }
            s.config = Some(h);
        }
        Query::SymbolSettings => {
            let rows: Vec<SymbolSetting> = from(b)?;
            s.leverage.clear();
            for i in rd.insts_of_venue(v) {
                let r = rows
                    .iter()
                    .find(|r| r.symbol == rd.inst_meta(*i).symbol.as_ref())
                    .ok_or(DecodeErr::Shape)?;
                if !r.margin_type.eq_ignore_ascii_case("CROSSED")
                    || r.leverage == 0
                    || r.leverage > rd.inst_meta(*i).max_leverage
                {
                    return Err(DecodeErr::Shape);
                }
                s.leverage.push(LeverageItem {
                    inst: *i,
                    leverage: Rate(i64::from(r.leverage) * 1_000_000_000),
                    ..zerocopy::FromZeros::new_zeroed()
                });
            }
        }
        Query::Balances if fut => {
            let r: Account = from(b)?;
            s.balances.clear();
            for row in r.assets {
                if let Some(asset) = asset(rd, &row.asset) {
                    s.balances.push(BalanceReport {
                        venue: v,
                        account: a,
                        asset,
                        free: Qty(decimal(&row.wallet_balance, rd.asset(asset).exp.qty_exp)?),
                        ex_ts_ns: ns,
                        ..zerocopy::FromZeros::new_zeroed()
                    });
                }
            }
            let equity = notional(&r.total_margin_balance)?;
            let maint = notional(&r.total_maint_margin)?;
            if maint < 0 {
                return Err(DecodeErr::Shape);
            }
            s.margin = Some(MarginReportHdr {
                venue: v,
                account: a,
                source: 2,
                margin_balance: Notional(equity),
                maint_margin: Notional(maint),
                avail_margin: Notional(notional(&r.available_balance)?),
                margin_ratio: Rate(if equity > 0 {
                    (i128::from(maint) * 1_000_000_000 / i128::from(equity)).min(i128::from(i64::MAX)) as i64
                } else {
                    i64::MAX
                }),
                ex_ts_ns: ns,
                ..zerocopy::FromZeros::new_zeroed()
            });
        }
        Query::Balances => {
            let r: SpotAccount = from(b)?;
            s.balances.clear();
            for row in r.balances {
                if let Some(asset) = asset(rd, &row.asset) {
                    s.balances.push(BalanceReport {
                        venue: v,
                        account: a,
                        asset,
                        free: Qty(decimal(&row.free, rd.asset(asset).exp.qty_exp)?),
                        locked: Qty(decimal(&row.locked, rd.asset(asset).exp.qty_exp)?),
                        ex_ts_ns: ns,
                        ..zerocopy::FromZeros::new_zeroed()
                    });
                }
            }
        }
        Query::Positions => {
            let rows: Vec<Position> = from(b)?;
            s.positions.clear();
            for r in rows {
                if r.position_side != "BOTH" {
                    return Err(DecodeErr::Shape);
                }
                let i = match instrument(rd, v, &r.symbol) {
                    Ok(i) => i,
                    Err(_) if decimal(&r.position_amt, -8)? == 0 => continue,
                    Err(e) => return Err(e),
                };
                let h = rd.inst(i);
                if s.positions.iter().any(|p| p.inst == i) {
                    return Err(DecodeErr::Shape);
                }
                s.positions.push(PositionReport {
                    venue: v,
                    account: a,
                    inst: i,
                    qty: Qty(decimal(&r.position_amt, h.qty_wire_exp)?),
                    entry_px: Px(average_price(&r.entry_price, h.px_wire_exp)?),
                    upnl: Notional(notional(&r.un_realized_profit)?),
                    liq_px: Px(average_price(&r.liquidation_price, h.px_wire_exp)?),
                    ex_ts_ns: ns,
                    ..zerocopy::FromZeros::new_zeroed()
                });
            }
            for &i in rd.insts_of_venue(v) {
                if !s.positions.iter().any(|p| p.inst == i) {
                    s.positions.push(PositionReport {
                        venue: v,
                        account: a,
                        inst: i,
                        ex_ts_ns: ns,
                        ..zerocopy::FromZeros::new_zeroed()
                    });
                }
            }
            s.position_margin_items();
        }
        Query::OpenOrders { .. } => {
            let rows: Vec<Order> = from(b)?;
            s.orders = rows
                .into_iter()
                .map(|r| OpenOrder {
                    symbol: r.symbol,
                    order_id: r.order_id.to_string(),
                })
                .collect();
            s.cursor.clear();
        }
        Query::Cancel { symbol, order_id } => {
            let r: Order = from(b)?;
            if r.symbol != *symbol || r.order_id.to_string() != *order_id {
                return Err(DecodeErr::Shape);
            }
        }
        Query::ListenKey | Query::Keepalive => {}
        Query::DeadMan { symbol, timeout_ms } => {
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase")]
            struct Ack {
                symbol: String,
                countdown_time: Countdown,
            }
            #[derive(Deserialize)]
            #[serde(untagged)]
            enum Countdown {
                Number(u32),
                Text(String),
            }
            let r: Ack = from(b)?;
            let count = match r.countdown_time {
                Countdown::Number(n) => n,
                Countdown::Text(s) => s.parse::<u32>().map_err(|_| DecodeErr::Number)?,
            };
            if &r.symbol != symbol || count != *timeout_ms {
                return Err(DecodeErr::Shape);
            }
        }
    }
    if matches!(q, Query::Balances) {
        for &i in rd.insts_of_venue(v) {
            for asset in [rd.inst(i).asset, rd.inst(i).quote] {
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
    }
    Ok(())
}
