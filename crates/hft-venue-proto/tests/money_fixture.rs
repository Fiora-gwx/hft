//! Synthetic wire grammar for testing the contract checker, never exchange-capture evidence.
use super::*;
use hft_event::{
    AccountConfigReportHdr, BalanceReport, ExternalCause, ExternalFill, FundingSettle, LeverageItem,
    MarginItem, MarginReportHdr, OrderEvent, OrderEventKind as K, PositionReport,
};
use hft_types::{Rate, SlotId, Tsc};
use hft_venue_proto::{
    AcctProtocol, AcctQuery, AcctQueryKind, ExchIdHint, RestOrigin, TradeIdRing, TradeKey, dedup_key,
};

#[derive(Default)]
pub struct ReportCtx {
    trades: TradeIdRing<1024>,
    fees: TradeIdRing<1024>,
    funding: TradeIdRing<1024>,
}
pub fn report<P: VenueProtocol>(
    ctx: &mut ReportCtx,
    p: &[u8],
    rx: &RxMeta,
    out: &mut impl ReportSink,
) -> Result<ReportOutcome, DecodeErr> {
    let mut o = ReportOutcome::default();
    let what = str_field(p, "e").ok_or(DecodeErr::Shape)?;
    let value = |k| int_field(p, k).unwrap_or(0);
    let inst = InstId(value("i") as u32);
    if what == b"balance" {
        let mut b = BalanceReport::new_zeroed();
        b.venue = P::VENUE;
        b.account = rx.account;
        b.asset = AssetId(2);
        out.balance(&b);
        o.events = 1;
        return Ok(o);
    }
    if what == b"funding" {
        if value("mode") == 3 {
            return Ok(o);
        }
        let inst = if value("mode") == 2 { InstId(0) } else { inst };
        let key = dedup_key(
            inst,
            TradeKey::Funding {
                settle_ts_ns: value("T"),
                disc: value("disc") as u64,
            },
        );
        if ctx.funding.seen_or_insert(key) {
            return Ok(o);
        }
        let mut f = FundingSettle::new_zeroed();
        f.venue = P::VENUE;
        f.account = rx.account;
        f.inst = inst;
        f.amount = Notional(value("amount"));
        f.settle_ts_ns = value("T");
        out.funding_settle(&f);
        o.events = 1;
        return Ok(o);
    }
    let mut e = blank_report(rx);
    e.inst = inst;
    e.exch_order_id = value("order") as u64;
    e.trade_id = value("id").max(0) as u64;
    e.qty = Qty(value("q"));
    e.cum_filled = Qty(value("cum"));
    e.avg_px = Px(value("avg"));
    e.px = Px(value("px"));
    e.fee = Qty(value("fee"));
    e.fee_asset = AssetId(value("asset") as u16);
    e.ex_ts_ns = value("T");
    let cid = str_field(p, "cid");
    let owner = cid.and_then(P::parse_cid);
    if let Some(id) = owner {
        e.epoch = id.epoch;
        e.slot = SlotId(id.slot);
        e.r#gen = id.r#gen;
    } else {
        e.epoch = 0;
        e.slot = SlotId::UNRESOLVED;
        e.r#gen = 0;
        if let ExchIdHint::Resolved { epoch, slot, r#gen } =
            out.exch_id(str_field(p, "order").unwrap_or(&[]), None)
        {
            e.epoch = epoch;
            e.slot = slot;
            e.r#gen = r#gen;
        }
    }
    e.what = match what {
        b"query" => K::QueryResult,
        b"cancel" => K::CancelReject,
        b"amend" => K::AmendReject,
        b"fee" => K::FeeOnly,
        _ if value("partial") == 1 => K::PartialFill,
        _ => K::Fill,
    };
    e.detail = match e.what {
        K::QueryResult => value("status") as u8,
        K::CancelReject | K::AmendReject => u8::from(value("code") == -2011),
        K::PartialFill | K::Fill => {
            if value("side") == 2 {
                Side::Sell as u8
            } else {
                Side::Buy as u8
            }
        }
        _ => 0,
    };
    if e.cum_filled.0 > 0 && e.avg_px.0 <= 0 {
        return Err(DecodeErr::Shape);
    }
    let execution = matches!(e.what, K::Fill | K::PartialFill | K::FeeOnly);
    if execution {
        let key = dedup_key(
            inst,
            if e.trade_id != 0 {
                TradeKey::Id(e.trade_id)
            } else {
                TradeKey::OrderSeq {
                    exch_order_id: e.exch_order_id,
                    fill_seq: e.cum_filled.0 as u64,
                }
            },
        );
        let ring = if e.what == K::FeeOnly {
            &mut ctx.fees
        } else {
            &mut ctx.trades
        };
        if ring.seen_or_insert(key) {
            return Ok(o);
        }
    }
    let marker = value("marker");
    if what == b"fill" && (marker != 0 || cid.is_some_and(|c| !c.is_empty() && owner.is_none())) {
        let cause = match marker {
            1 => ExternalCause::Liquidation,
            2 => ExternalCause::Adl,
            _ if cid.is_some_and(|c| c.starts_with(b"web_")) => ExternalCause::Manual,
            _ => ExternalCause::Unknown,
        };
        out.external_fill(&ExternalFill {
            venue: P::VENUE,
            account: rx.account,
            side: if e.detail == 2 { Side::Sell } else { Side::Buy },
            cause,
            inst,
            qty: e.qty,
            px: e.px,
            fee: e.fee,
            fee_asset: e.fee_asset,
            _pad: [0; 6],
            exch_order_id: e.exch_order_id,
            trade_id: e.trade_id,
            ex_ts_ns: e.ex_ts_ns,
            rx_tsc: Tsc(rx.rx_tsc),
            fill_seq: e.cum_filled.0 as u64,
            cash_qty: Qty(0),
            cash_amount: Qty(0),
        });
    } else {
        out.order(&e);
    }
    o.events = 1;
    o.fee_rule_deviation = e.fee.0 != 0 && e.fee_asset != AssetId(2);
    Ok(o)
}

pub struct OriginSink<'a, S> {
    pub inner: &'a mut S,
    pub req: RestOrigin,
}
impl<S: ReportSink> ReportSink for OriginSink<'_, S> {
    fn order(&mut self, e: &OrderEvent) {
        let mut e = *e;
        if let Some(r) = self.req.req {
            e.slot = SlotId(r.slot);
            e.r#gen = r.r#gen;
        }
        self.inner.order(&e);
    }
    fn position(&mut self, e: &PositionReport) {
        self.inner.position(e);
    }
    fn balance(&mut self, e: &BalanceReport) {
        self.inner.balance(e);
    }
    fn funding_settle(&mut self, e: &FundingSettle) {
        self.inner.funding_settle(e);
    }
    fn external_fill(&mut self, e: &ExternalFill) {
        self.inner.external_fill(e);
    }
    fn margin(&mut self, h: &MarginReportHdr, t: &[MarginItem]) {
        self.inner.margin(h, t);
    }
    fn account_config(&mut self, h: &AccountConfigReportHdr, t: &[LeverageItem]) {
        self.inner.account_config(h, t);
    }
    fn exch_id(&mut self, k: &[u8], o: Option<&ClientOrderId>) -> ExchIdHint {
        self.inner.exch_id(k, o)
    }
}

#[derive(Default)]
pub struct Scratch {
    pub config: Option<AccountConfigReportHdr>,
    pub leverage: [i64; 2],
    pub balance: i64,
    pub maint: i64,
    pub adl: u8,
    pub margin: bool,
}
impl<const SPOT: bool> AcctProtocol for Fake<SPOT, 4> {
    type AcctCtx = u64;
    type AcctScratch = Scratch;
    fn acct_queries(k: AcctQueryKind) -> &'static [AcctQuery] {
        match k {
            AcctQueryKind::Config => &[AcctQuery {
                kind: AcctQueryKind::Config,
                step: 0,
                every: 1,
                push_fallback: false,
                weight: 1,
            }],
            AcctQueryKind::Margin if SPOT => &[],
            AcctQueryKind::Margin => &[AcctQuery {
                kind: AcctQueryKind::Margin,
                step: 0,
                every: 1,
                push_fallback: true,
                weight: 1,
            }],
        }
    }
    fn acct_request(
        _rd: &RefData,
        _t: &(),
        _a: AccountId,
        q: AcctQuery,
        _c: &WallClock,
        out: &mut RestReqBuf,
    ) -> Result<(), EncodeErr> {
        out.path_query
            .push(if q.kind == AcctQueryKind::Config {
                b"/config?signature=fixture"
            } else {
                b"/margin?signature=fixture"
            })
            .map_err(|_| EncodeErr::FieldOverflow)?;
        out.redacted = true;
        Ok(())
    }
    fn acct_decode(
        _rd: &RefData,
        ctx: &mut u64,
        a: AccountId,
        q: AcctQuery,
        status: u16,
        p: &[u8],
        s: &mut Scratch,
    ) -> Result<(), DecodeErr> {
        if status != 200 {
            return Err(DecodeErr::Shape);
        }
        *ctx += 1;
        if q.kind == AcctQueryKind::Config {
            let mut h = AccountConfigReportHdr::new_zeroed();
            h.venue = Self::VENUE;
            h.account = a;
            h.position_mode = if SPOT { 0 } else { int_field(p, "pos")? as u8 };
            h.margin_mode = if SPOT { 0 } else { int_field(p, "margin")? as u8 };
            h.multi_asset = int_field(p, "multi")? as u8;
            for (k, bit) in [
                ("pos", 1),
                ("margin", 2),
                ("multi", 4),
                ("discount", 8),
                ("type", 16),
                ("borrow", 32),
                ("withdraw", 64),
            ] {
                match int_field(p, k) {
                    Ok(v) if v != 0 => h.mismatches |= bit,
                    Err(_) if k != "borrow" || int_field(p, "cash_only") != Ok(1) => h.mismatches |= 256,
                    _ => {}
                }
            }
            let uid = str_field(p, "uid").unwrap_or(&[]);
            let verified = int_field(p, "verified") == Ok(1);
            if !uid.is_empty() {
                if uid.len() > 24 {
                    return Err(DecodeErr::Shape);
                }
                h.venue_uid[..uid.len()].copy_from_slice(uid);
                h.uid_keys = 2;
                if verified && uid != b"U1" {
                    h.mismatches |= 128;
                }
            } else if verified {
                h.mismatches |= 256;
            }
            s.leverage = [
                int_field(p, "lev1").unwrap_or(0),
                int_field(p, "lev2").unwrap_or(0),
            ];
            h.n_lev = if SPOT { 0 } else { 2 };
            s.config = Some(h);
        } else {
            s.balance = int_field(p, "balance")?;
            s.maint = int_field(p, "maint")?;
            s.adl = match (int_field(p, "adl"), int_field(p, "max_adl")) {
                (Ok(a), Ok(m)) if m > 0 => (a * 100 / m) as u8,
                _ => 255,
            };
            s.margin = true;
        }
        Ok(())
    }
    fn acct_finish(
        _rd: &RefData,
        _ctx: &mut u64,
        _a: AccountId,
        k: AcctQueryKind,
        s: &Scratch,
        out: &mut impl ReportSink,
    ) -> Result<(), DecodeErr> {
        if k == AcctQueryKind::Config {
            let h = s.config.ok_or(DecodeErr::Shape)?;
            let items = [
                LeverageItem {
                    inst: InstId(201),
                    _pad: 0,
                    leverage: Rate(s.leverage[0]),
                },
                LeverageItem {
                    inst: InstId(202),
                    _pad: 0,
                    leverage: Rate(s.leverage[1]),
                },
            ];
            out.account_config(&h, &items[..usize::from(h.n_lev)]);
        } else {
            if SPOT {
                return Ok(());
            }
            if !s.margin {
                return Err(DecodeErr::Shape);
            }
            let mut h = MarginReportHdr::new_zeroed();
            h.venue = Self::VENUE;
            h.account = _a;
            h.n = 1;
            h.margin_balance = Notional(s.balance);
            h.maint_margin = Notional(s.maint);
            h.margin_ratio = if s.balance <= 0 {
                Rate::MAX
            } else {
                Rate(
                    ((i128::from(s.maint) * 1_000_000_000 + i128::from(s.balance) - 1)
                        / i128::from(s.balance)) as i64,
                )
            };
            out.margin(
                &h,
                &[MarginItem {
                    inst: InstId(201),
                    adl_quantile: s.adl,
                    _pad: [0; 3],
                    liq_px: Px(0),
                }],
            );
            let mut p = PositionReport::new_zeroed();
            p.venue = Self::VENUE;
            p.account = _a;
            p.inst = InstId(201);
            out.position(&p);
        }
        Ok(())
    }
}
