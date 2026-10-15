//! Measurement-only joins between network frames, normalized inputs and ring handoff.
use hft_event::{EventKind as K, FeedMsg, GwCmd, RepMsg};
use hft_rt::latency_trace::{self as lt, Context, Stage};
use zerocopy::FromBytes;
use zerocopy::IntoBytes;

pub(crate) fn market_bytes(stage: Stage, kind: K, bytes: &[u8]) {
    if !lt::enabled() {
        return;
    }
    if let Ok((h, _)) = hft_event::MarketHdr::read_from_prefix(bytes) {
        lt::event_values(
            stage,
            kind.0,
            bytes,
            [
                h.ex_ts_ns as u64,
                h.ex_send_ns as u64,
                u64::from(h.inst.0),
                h.rx_tsc,
            ],
        );
    }
}

pub(crate) fn feed(stage: Stage, m: &FeedMsg) {
    if !lt::enabled() {
        return;
    }
    match m {
        FeedMsg::Bbo(x) => lt::event_values(
            stage,
            K::BBO.0,
            x.as_bytes(),
            [
                x.h.ex_ts_ns as u64,
                x.h.ex_send_ns as u64,
                u64::from(x.h.inst.0),
                x.h.rx_tsc,
            ],
        ),
        FeedMsg::L2Delta(x) => lt::event_values(
            stage,
            K::L2_DELTA.0,
            x.as_bytes(),
            [
                x.h.ex_ts_ns as u64,
                x.h.ex_send_ns as u64,
                u64::from(x.h.inst.0),
                x.h.rx_tsc,
            ],
        ),
        FeedMsg::Trade(x) => lt::event_values(
            stage,
            K::TRADE.0,
            x.as_bytes(),
            [
                x.h.ex_ts_ns as u64,
                x.h.ex_send_ns as u64,
                u64::from(x.h.inst.0),
                x.h.rx_tsc,
            ],
        ),
        FeedMsg::Funding(x) => lt::event_values(
            stage,
            K::FUNDING.0,
            x.as_bytes(),
            [
                x.h.ex_ts_ns as u64,
                x.h.ex_send_ns as u64,
                u64::from(x.h.inst.0),
                x.h.rx_tsc,
            ],
        ),
        FeedMsg::MarkPrice(x) => lt::event_values(
            stage,
            K::MARK_PRICE.0,
            x.as_bytes(),
            [
                x.h.ex_ts_ns as u64,
                x.h.ex_send_ns as u64,
                u64::from(x.h.inst.0),
                x.h.rx_tsc,
            ],
        ),
        FeedMsg::IndexPrice(x) => lt::event_values(
            stage,
            K::INDEX_PRICE.0,
            x.as_bytes(),
            [
                x.h.ex_ts_ns as u64,
                x.h.ex_send_ns as u64,
                u64::from(x.h.inst.0),
                x.h.rx_tsc,
            ],
        ),
        FeedMsg::Liquidation(x) => lt::event_values(
            stage,
            K::LIQUIDATION.0,
            x.as_bytes(),
            [
                x.h.ex_ts_ns as u64,
                x.h.ex_send_ns as u64,
                u64::from(x.h.inst.0),
                x.h.rx_tsc,
            ],
        ),
        FeedMsg::ConnState(x) => lt::event(stage, K::CONN_STATE.0, x.as_bytes()),
        FeedMsg::Bulk { .. } => {}
    }
}

pub(crate) fn rep(stage: Stage, m: &RepMsg) {
    if !lt::enabled() {
        return;
    }
    match m {
        RepMsg::OrderEvent(x) => lt::event_values(
            stage,
            K::ORDER_EVENT.0,
            x.as_bytes(),
            [x.ex_ts_ns as u64, x.trade_id, x.exch_order_id, x.rx_tsc],
        ),
        RepMsg::SendReport(x) => lt::event(stage, K::SEND_REPORT.0, x.as_bytes()),
        RepMsg::PositionReport(x) => lt::event(stage, K::POSITION_REPORT.0, x.as_bytes()),
        RepMsg::BalanceReport(x) => lt::event(stage, K::BALANCE_REPORT.0, x.as_bytes()),
        RepMsg::FundingSettle(x) => lt::event(stage, K::FUNDING_SETTLE.0, x.as_bytes()),
        RepMsg::ConnState(x) => lt::event(stage, K::CONN_STATE.0, x.as_bytes()),
        RepMsg::ExternalFill(x) => lt::event(stage, K::EXTERNAL_FILL.0, x.as_bytes()),
        RepMsg::Note(x) => lt::event(stage, K::NOTE.0, x.as_bytes()),
        RepMsg::MarginReport(_) | RepMsg::AccountConfigReport(_) => {}
    }
}

pub(crate) fn command(c: &GwCmd, venue: u8) -> Context {
    Context {
        decision_seq: c.decision_seq,
        order: (u64::from(c.r#gen) << 32) | u64::from(c.slot.0),
        venue_conn: u64::from(venue),
        kind_inst: u64::from(c.inst.0) << 32,
        ..Context::default()
    }
}
