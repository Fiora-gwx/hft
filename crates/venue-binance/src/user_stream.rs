//! Private stream normalization. Parse and validate before writing any event.
use crate::{
    GwCtx, Line,
    cid::parse_cid,
    json::{Arr, fixed, from, fx, time_ns},
    md::inst_of,
    reports::{blank, exch_key},
};
use hft_event::{BalanceReport, ExternalCause, ExternalFill, FundingSettle, OrderEventKind, PositionReport};
use hft_refdata::{FeeAssetRule, RefData};
use hft_types::{AssetExp, AssetId, InstId, Notional, Px, Qty, Rate, RoundMode, Side, SlotId, Tsc};
use hft_venue_proto::{AcctHint, DecodeErr, ExchIdHint, ReportCtl, ReportOutcome, ReportSink, RxMeta};
use serde::Deserialize;
use zerocopy::FromZeros;

#[derive(Deserialize)]
struct Head<'a> {
    #[serde(borrow)]
    e: &'a str,
}
#[derive(Deserialize)]
struct Wrapped<'a> {
    #[serde(borrow)]
    event: sonic_rs::LazyValue<'a>,
}
#[derive(Deserialize)]
struct FutOrder<'a> {
    #[serde(borrow)]
    o: Order<'a>,
}
#[derive(Deserialize)]
#[allow(non_snake_case)]
struct Order<'a> {
    #[serde(borrow)]
    s: &'a str,
    #[serde(default, borrow)]
    c: &'a str,
    #[serde(default, borrow)]
    C: &'a str,
    #[serde(borrow)]
    S: &'a str,
    #[serde(borrow)]
    o: &'a str,
    #[serde(borrow)]
    x: &'a str,
    #[serde(borrow)]
    X: &'a str,
    i: u64,
    #[serde(borrow)]
    l: &'a str,
    #[serde(borrow)]
    z: &'a str,
    #[serde(borrow)]
    L: &'a str,
    #[serde(default, borrow)]
    ap: Option<&'a str>,
    #[serde(default, borrow)]
    Z: Option<&'a str>,
    #[serde(default, borrow)]
    n: Option<&'a str>,
    #[serde(default, borrow)]
    N: Option<&'a str>,
    #[serde(default, borrow)]
    rp: Option<&'a str>,
    T: i64,
    #[serde(default)]
    t: i64,
}

fn asset(rd: &RefData, name: &str) -> AssetId {
    hft_venue_proto::account::asset(rd, name).unwrap_or(AssetId(0))
}

pub(crate) fn decode(
    line: Line,
    rd: &RefData,
    ctx: &mut GwCtx,
    p: &[u8],
    rx: &RxMeta,
    out: &mut impl ReportSink,
) -> Result<ReportOutcome, DecodeErr> {
    let wrapper;
    let p = if line == Line::Spot {
        wrapper = from::<Wrapped>(p)?;
        wrapper.event.as_raw_str().as_bytes()
    } else {
        p
    };
    let fast = || {
        use hft_venue_proto::json as wire;
        let [event] = wire::project(p, |key| (key == b"e").then_some(0))?;
        Some(Head {
            e: wire::string(event)?,
        })
    };
    let h: Head = fast().map_or_else(|| from(p), Ok)?;
    match h.e {
        "ORDER_TRADE_UPDATE" => order(line, rd, ctx, &from::<FutOrder>(p)?.o, rx, out),
        "executionReport" => order(line, rd, ctx, &from::<Order>(p)?, rx, out),
        "ACCOUNT_UPDATE" => account(line, rd, ctx, p, rx, out),
        "outboundAccountPosition" => spot_balance(line, rd, p, rx, out),
        "MARGIN_CALL" => Ok(ReportOutcome {
            hint: AcctHint::MarginNow,
            ..ReportOutcome::default()
        }),
        "ACCOUNT_CONFIG_UPDATE" => {
            #[derive(Deserialize)]
            struct Config<'a> {
                #[serde(borrow)]
                ac: Option<sonic_rs::LazyValue<'a>>,
                #[serde(borrow)]
                ai: Option<sonic_rs::LazyValue<'a>>,
            }
            let cfg: Config = from(p)?;
            Ok(ReportOutcome {
                hint: if cfg.ac.is_some() || cfg.ai.is_some() {
                    AcctHint::ConfigRecheck
                } else {
                    AcctHint::None
                },
                ..ReportOutcome::default()
            })
        }
        "listenKeyExpired" | "eventStreamTerminated" => Ok(ReportOutcome {
            ctl: ReportCtl::UserStreamEnded,
            ..ReportOutcome::default()
        }),
        _ => Err(DecodeErr::UnknownClass),
    }
}

fn order(
    line: Line,
    rd: &RefData,
    ctx: &mut GwCtx,
    r: &Order<'_>,
    rx: &RxMeta,
    out: &mut impl ReportSink,
) -> Result<ReportOutcome, DecodeErr> {
    let inst = inst_of(line, rd, r.s.as_bytes());
    if inst.0 == 0 {
        return Err(DecodeErr::Shape);
    }
    let h = rd.inst(inst);
    let side = match r.S {
        "BUY" => Side::Buy,
        "SELL" => Side::Sell,
        _ => return Err(DecodeErr::Shape),
    };
    let cid_text = if matches!(r.x, "CANCELED" | "EXPIRED") && !r.C.is_empty() {
        r.C
    } else {
        r.c
    };
    let cid = parse_cid(cid_text.as_bytes()).filter(|c| c.venue == line.venue() && c.account == rx.account);
    let cause = if r.c.starts_with("adl_autoclose") {
        Some(ExternalCause::Adl)
    } else if r.c.starts_with("settlement_autoclose") {
        Some(ExternalCause::Delivery)
    } else if r.c.starts_with("autoclose-") || r.o == "LIQUIDATION" {
        Some(ExternalCause::Liquidation)
    } else {
        None
    };
    let what = match r.x {
        "NEW" => OrderEventKind::Ack,
        "CANCELED" => OrderEventKind::CancelAck,
        "EXPIRED" | "TRADE_PREVENTION" => OrderEventKind::Expired,
        "AMENDMENT" | "REPLACED" => OrderEventKind::AmendAck,
        "REJECTED" => OrderEventKind::Reject,
        "TRADE" | "CALCULATED" => match r.X {
            "PARTIALLY_FILLED" => OrderEventKind::PartialFill,
            "FILLED" => OrderEventKind::Fill,
            _ => return Err(DecodeErr::Shape),
        },
        _ => return Err(DecodeErr::Shape),
    };
    if r.x == "CALCULATED" && cause.is_none() {
        return Err(DecodeErr::Shape);
    }
    let fill = matches!(what, OrderEventKind::PartialFill | OrderEventKind::Fill);
    let mut e = blank(line, rx.account, rx);
    e.what = what;
    if fill {
        e.detail = side as u8;
    }
    e.inst = inst;
    e.exch_order_id = r.i;
    e.qty = Qty(fixed(r.l, h.qty_wire_exp)?);
    e.cum_filled = Qty(fixed(r.z, h.qty_wire_exp)?);
    e.px = Px(fixed(r.L, h.px_wire_exp)?);
    e.avg_px = if line == Line::Usdm {
        Px(
            hft_types::parse_round(r.ap.unwrap_or("0").as_bytes(), h.px_wire_exp, RoundMode::HalfEven)
                .map_err(fx)?,
        )
    } else {
        let n = hft_types::parse_round(r.Z.ok_or(DecodeErr::Shape)?.as_bytes(), -6, RoundMode::HalfEven)
            .map_err(fx)?;
        if e.cum_filled.0 == 0 {
            Px(0)
        } else {
            Px::from_notional(
                Notional(n),
                e.cum_filled,
                AssetExp {
                    px_exp: h.px_exp,
                    qty_exp: h.qty_exp,
                },
                RoundMode::HalfEven,
            )
            .map_err(fx)?
        }
    };
    if e.cum_filled.0 < 0
        || e.qty.0 < 0
        || e.qty > e.cum_filled
        || (e.cum_filled.0 > 0 && e.avg_px.0 <= 0)
        || (fill && (e.qty.0 <= 0 || e.px.0 <= 0))
    {
        return Err(DecodeErr::Shape);
    }
    e.ex_ts_ns = time_ns(r.T, if line == Line::Spot { 1000 } else { 1_000_000 })?;
    e.trade_id = u64::try_from(r.t).unwrap_or(0);
    if let Some(name) = r.N {
        e.fee_asset = asset(rd, name);
    }
    let mut cash_amount = Qty(0);
    let mut cash_qty = Qty(0);
    if fill {
        let fee_exp = if e.fee_asset.0 == 0 {
            -8
        } else {
            rd.asset(e.fee_asset).exp.qty_exp
        };
        e.fee = Qty(fixed(r.n.ok_or(DecodeErr::Shape)?, fee_exp)?);
        if e.fee.0 != 0 && e.fee_asset.0 == 0 {
            return Err(DecodeErr::Shape);
        }
        if let Some(cash) = r.rp {
            cash_qty = e.qty;
            cash_amount = Qty(fixed(cash, rd.asset(h.quote).exp.qty_exp)?);
        }
    }
    let received = if side == Side::Buy { h.asset } else { h.quote };
    let deviation = fill
        && e.fee.0 != 0
        && match rd.caps(line.venue()).fee_asset_rule {
            FeeAssetRule::Quote => e.fee_asset != h.quote,
            FeeAssetRule::ReceivedAsset => e.fee_asset != received,
            FeeAssetRule::ReceivedAssetUnlessDiscount { token } => {
                e.fee_asset != received && e.fee_asset != token
            }
            FeeAssetRule::Configured(a) => e.fee_asset != a,
        };
    if fill && !ctx.trade(inst.0, e.trade_id, r.i, e.cum_filled.0) {
        return Ok(ReportOutcome::default());
    }
    let mut key = [0; 20];
    let hint = out.exch_id(exch_key(r.i, &mut key), cid.as_ref());
    let identity = cid.map(|c| (c.epoch, SlotId(c.slot), c.r#gen)).or(match hint {
        ExchIdHint::Resolved { epoch, slot, r#gen } => Some((epoch, slot, r#gen)),
        _ => None,
    });
    if fill && (cause.is_some() || identity.is_none()) {
        let cause = cause.unwrap_or_else(|| {
            if ["web_", "ios_", "and_"].iter().any(|p| cid_text.starts_with(p)) {
                ExternalCause::Manual
            } else {
                ExternalCause::Unknown
            }
        });
        out.external_fill(&ExternalFill {
            venue: line.venue(),
            account: rx.account,
            side,
            cause,
            inst,
            qty: e.qty,
            px: e.px,
            fee: e.fee,
            fee_asset: e.fee_asset,
            _pad: [0; 6],
            exch_order_id: r.i,
            trade_id: e.trade_id,
            ex_ts_ns: e.ex_ts_ns,
            rx_tsc: Tsc(rx.rx_tsc),
            fill_seq: if e.trade_id == 0 { e.cum_filled.0 as u64 } else { 0 },
            cash_qty,
            cash_amount,
        });
    } else {
        if let Some((epoch, slot, g)) = identity {
            e.epoch = epoch;
            e.slot = slot;
            e.r#gen = g;
        }
        out.order(&e);
    }
    Ok(ReportOutcome {
        events: 1,
        fee_rule_deviation: deviation,
        ..ReportOutcome::default()
    })
}

#[derive(Deserialize)]
#[allow(non_snake_case)]
struct Account<'a> {
    T: i64,
    #[serde(borrow)]
    a: Update<'a>,
}
#[derive(Deserialize)]
#[allow(non_snake_case)]
struct Update<'a> {
    #[serde(borrow)]
    m: &'a str,
    #[serde(borrow)]
    B: Arr<Balance<'a>, 64>,
    #[serde(borrow)]
    P: Arr<Position<'a>, 64>,
}
#[derive(Deserialize)]
struct Balance<'a> {
    #[serde(borrow)]
    a: &'a str,
    #[serde(borrow)]
    wb: &'a str,
    #[serde(borrow)]
    bc: &'a str,
}
#[derive(Deserialize)]
struct Position<'a> {
    #[serde(borrow)]
    s: &'a str,
    #[serde(borrow)]
    pa: &'a str,
    #[serde(borrow)]
    ep: &'a str,
    #[serde(borrow)]
    up: &'a str,
    #[serde(borrow)]
    ps: &'a str,
    #[serde(borrow)]
    mt: &'a str,
}
fn account(
    line: Line,
    rd: &RefData,
    ctx: &mut GwCtx,
    p: &[u8],
    rx: &RxMeta,
    out: &mut impl ReportSink,
) -> Result<ReportOutcome, DecodeErr> {
    let r: Account = from(p)?;
    if r.a.B.over || r.a.P.over {
        return Err(DecodeErr::Shape);
    }
    let ns = time_ns(r.T, 1_000_000)?;
    if r.a.m == "FUNDING_FEE"
        && rd.caps(line.venue()).funding_settle != hft_refdata::FundingSettleKind::RateOnly
    {
        let Some(b) = r.a.B.iter().find(|b| b.a == "USDT") else {
            return Ok(ReportOutcome::default());
        };
        let amount = Notional(hft_types::parse_round(b.bc.as_bytes(), -6, RoundMode::HalfEven).map_err(fx)?);
        let wallet = fixed(b.wb, rd.asset(asset(rd, "USDT")).exp.qty_exp)?;
        let inst =
            r.a.P
                .iter()
                .find(|p| p.mt == "isolated")
                .map_or(InstId(0), |p| inst_of(line, rd, p.s.as_bytes()));
        if !ctx.funding(ns, wallet) {
            return Ok(ReportOutcome::default());
        }
        out.funding_settle(&FundingSettle {
            venue: line.venue(),
            account: rx.account,
            inst,
            amount,
            rate: Rate(0),
            settle_ts_ns: ns,
            rx_tsc: Tsc(rx.rx_tsc),
            ..FundingSettle::new_zeroed()
        });
        return Ok(ReportOutcome {
            events: 1,
            ..ReportOutcome::default()
        });
    }
    let mut balances = [None; 64];
    let mut positions = [None; 64];
    for (slot, b) in balances.iter_mut().zip(r.a.B.iter()) {
        let a = asset(rd, b.a);
        if a.0 == 0 {
            continue;
        }
        *slot = Some(BalanceReport {
            venue: line.venue(),
            account: rx.account,
            asset: a,
            free: Qty(fixed(b.wb, rd.asset(a).exp.qty_exp)?),
            ex_ts_ns: ns,
            rx_tsc: Tsc(rx.rx_tsc),
            ..BalanceReport::new_zeroed()
        });
    }
    for (slot, p) in positions.iter_mut().zip(r.a.P.iter()) {
        let inst = inst_of(line, rd, p.s.as_bytes());
        if inst.0 == 0 {
            continue;
        }
        if p.ps != "BOTH" {
            return Err(DecodeErr::Shape);
        }
        let h = rd.inst(inst);
        *slot = Some(PositionReport {
            venue: line.venue(),
            account: rx.account,
            inst,
            qty: Qty(fixed(p.pa, h.qty_wire_exp)?),
            entry_px: Px(
                hft_types::parse_round(p.ep.as_bytes(), h.px_wire_exp, RoundMode::HalfEven).map_err(fx)?,
            ),
            upnl: Notional(hft_types::parse_round(p.up.as_bytes(), -6, RoundMode::HalfEven).map_err(fx)?),
            ex_ts_ns: ns,
            rx_tsc: Tsc(rx.rx_tsc),
            ..PositionReport::new_zeroed()
        });
    }
    let mut o = ReportOutcome {
        hint: match r.a.m {
            "MARGIN_TYPE_CHANGE" => AcctHint::ConfigRecheck,
            "ADMIN_DEPOSIT" | "ADMIN_WITHDRAW" | "INSURANCE_CLEAR" => AcctHint::MarginNow,
            _ => AcctHint::None,
        },
        ..ReportOutcome::default()
    };
    for b in balances.iter().flatten() {
        out.balance(b);
        o.events += 1;
    }
    for p in positions.iter().flatten() {
        out.position(p);
        o.events += 1;
    }
    Ok(o)
}
#[derive(Deserialize)]
#[allow(non_snake_case)]
struct SpotBalances<'a> {
    u: i64,
    #[serde(borrow)]
    B: Arr<SpotBalance<'a>, 64>,
}
#[derive(Deserialize)]
struct SpotBalance<'a> {
    #[serde(borrow)]
    a: &'a str,
    #[serde(borrow)]
    f: &'a str,
    #[serde(borrow)]
    l: &'a str,
}
fn spot_balance(
    line: Line,
    rd: &RefData,
    p: &[u8],
    rx: &RxMeta,
    out: &mut impl ReportSink,
) -> Result<ReportOutcome, DecodeErr> {
    let r: SpotBalances = from(p)?;
    if r.B.over {
        return Err(DecodeErr::Shape);
    }
    let ns = time_ns(r.u, 1000)?;
    let mut rows = [None; 64];
    for (slot, b) in rows.iter_mut().zip(r.B.iter()) {
        let a = asset(rd, b.a);
        if a.0 == 0 {
            continue;
        }
        *slot = Some(BalanceReport {
            venue: line.venue(),
            account: rx.account,
            asset: a,
            free: Qty(fixed(b.f, rd.asset(a).exp.qty_exp)?),
            locked: Qty(fixed(b.l, rd.asset(a).exp.qty_exp)?),
            ex_ts_ns: ns,
            rx_tsc: Tsc(rx.rx_tsc),
            ..BalanceReport::new_zeroed()
        });
    }
    let mut o = ReportOutcome::default();
    for b in rows.iter().flatten() {
        out.balance(b);
        o.events += 1;
    }
    Ok(o)
}
