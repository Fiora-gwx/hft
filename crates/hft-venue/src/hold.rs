//! Bounded unmatched executions, owned by a single gateway account.
use hft_event::{ExternalCause, ExternalFill, OrderEvent, OrderEventKind, RepMsg};
use hft_types::{Qty, Side, Tsc};

#[derive(Clone, Copy, Debug)]
pub(crate) struct IdKey {
    bytes: [u8; 36],
    len: usize,
}
impl Default for IdKey {
    fn default() -> Self {
        Self {
            bytes: [0; 36],
            len: 0,
        }
    }
}
impl IdKey {
    pub fn new(key: &[u8]) -> Self {
        let mut out = Self::default();
        if key.len() <= out.bytes.len() {
            out.bytes[..key.len()].copy_from_slice(key);
            out.len = key.len();
        }
        out
    }
    pub fn bytes(&self) -> &[u8] {
        &self.bytes[..self.len]
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct Held {
    pub event: OrderEvent,
    pub flags: hft_event::RecordFlags,
    pub key: IdKey,
    pub since: u64,
}

/// Timeout changes attribution, never creates a synthetic fill or guesses realized cash.
pub(crate) fn expired(e: OrderEvent, flags: hft_event::RecordFlags) -> RepMsg {
    if e.what == OrderEventKind::FeeOnly || ![Side::Buy as u8, Side::Sell as u8].contains(&e.detail) {
        return RepMsg::OrderEvent(e, flags);
    }
    RepMsg::ExternalFill(ExternalFill {
        venue: e.venue,
        account: e.account,
        side: if e.detail == Side::Sell as u8 {
            Side::Sell
        } else {
            Side::Buy
        },
        cause: ExternalCause::Unknown,
        inst: e.inst,
        qty: e.qty,
        px: e.px,
        fee: e.fee,
        fee_asset: e.fee_asset,
        _pad: [0; 6],
        exch_order_id: e.exch_order_id,
        trade_id: e.trade_id,
        ex_ts_ns: e.ex_ts_ns,
        rx_tsc: Tsc(e.rx_tsc),
        fill_seq: e.cum_filled.0.max(0) as u64,
        cash_qty: Qty(0),
        cash_amount: Qty(0),
    })
}
