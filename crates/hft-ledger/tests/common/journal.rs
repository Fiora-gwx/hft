//! Journal fragments for the cold-reader tests: records are encoded with hft-event's codec and
//! fed to `JournalLedger::apply` as `RecordView`s.

use hft_event::{
    Body, EngineClock, Note, NoteLevel, NoteSource, OrderAction, OrderDecision, OrderEvent, OrderEventKind,
    RecordFlags, decode_record, encode_fixed,
};
use hft_ledger::JournalLedger;
use hft_types::{
    ASSET_NONE, AccountId, AssetId, GROUP_NONE, GroupId, InstId, Px, Qty, Side, SlotId, StrategyId, Tif, Tsc,
    VenueId,
};
use zerocopy::FromZeros;

/// TSC of every fed record; with `CLOCK_HZ_MILLI` = 1 GHz all records map to the clock's base.
pub const TSC: u64 = 1_000;
pub const CLOCK_HZ_MILLI: u64 = 1_000_000_000_000;

/// Encodes `b` as one record and feeds it.
pub fn feed<B: Body>(j: &mut JournalLedger, b: &B, end: bool) {
    let mut buf = vec![0u8; 1024];
    let flags = if end {
        RecordFlags::EVENT_END
    } else {
        RecordFlags(0)
    };
    let n = encode_fixed(&mut buf, 1, TSC, flags, b) as usize;
    let rv = decode_record(&buf[..n]).expect("decode");
    j.apply(&rv).expect("valid journal");
}

/// `EngineClock` placing the following records at engine time `t`.
pub fn clock(j: &mut JournalLedger, t: i64) {
    let mut c = EngineClock::new_zeroed();
    c.base_engine_ns = t;
    c.tsc_hz_milli = CLOCK_HZ_MILLI;
    feed(j, &c, true);
}

pub fn note(source: NoteSource, code: u32, inst: InstId, v: [u64; 8]) -> Note {
    Note {
        code,
        level: NoteLevel::Info,
        source,
        strategy: StrategyId(0),
        _pad: 0,
        group: GROUP_NONE,
        inst,
        v,
    }
}

pub fn epoch_start(epoch: u64) -> Note {
    note(
        NoteSource::Sequencer,
        hft_ledger::notes::code::EPOCH_START,
        InstId(0),
        [epoch, 0, 0, 0, 0, 0, 0, 0],
    )
}

/// `Note{OmsForeignFill}` with hft-oms's `v[]` layout.
#[allow(clippy::too_many_arguments)]
pub fn oms_foreign_fill(
    venue: VenueId,
    account: AccountId,
    inst: InstId,
    side: Side,
    path: u64,
    fee_asset: AssetId,
    qty: Qty,
    px: Px,
    fee: Qty,
    eoid: u64,
    trade_id: u64,
    cum: Qty,
) -> Note {
    let fill_seq = if trade_id == 0 { cum.0 as u64 } else { 0 };
    note(
        NoteSource::Oms,
        hft_ledger::notes::code::OMS_FOREIGN_FILL,
        inst,
        [
            u64::from(venue.0)
                | u64::from(account.0) << 8
                | (side as u64) << 16
                | path << 24
                | u64::from(fee_asset.0) << 32,
            qty.0 as u64,
            px.0 as u64,
            fee.0 as u64,
            eoid,
            trade_id,
            fill_seq,
            cum.0 as u64,
        ],
    )
}

#[allow(clippy::too_many_arguments)]
pub fn decision_new(
    slot: u32,
    r#gen: u32,
    account: AccountId,
    venue: VenueId,
    inst: InstId,
    side: Side,
    group: GroupId,
    qty: Qty,
) -> OrderDecision {
    OrderDecision {
        trigger_seq: 0,
        decided_tsc: Tsc(0),
        px: Px(0),
        qty,
        group,
        inst,
        slot: SlotId(slot),
        r#gen,
        target_slot: SlotId(0),
        reject: 0,
        strategy: StrategyId(1),
        venue,
        account,
        action: OrderAction::New,
        side,
        tif: Tif::Gtc,
        oflags: 0,
        reason: 0,
        intent_no: 0,
        req_px: Px(0),
        link_role: 0,
        _pad: [0; 7],
    }
}

#[allow(clippy::too_many_arguments)]
pub fn order_event(
    what: OrderEventKind,
    epoch: u16,
    slot: u32,
    r#gen: u32,
    account: AccountId,
    venue: VenueId,
    inst: InstId,
    cum: Qty,
    avg_px: Px,
) -> OrderEvent {
    OrderEvent {
        venue,
        account,
        what,
        detail: 0,
        slot: SlotId(slot),
        r#gen,
        inst,
        px: avg_px,
        qty: Qty(0),
        cum_filled: cum,
        fee: Qty(0),
        fee_asset: ASSET_NONE,
        epoch,
        reject_code: 0,
        exch_order_id: 0,
        ex_ts_ns: 0,
        hw_rx_ns: 0,
        rx_tsc: 0,
        avg_px,
        cash_qty: hft_types::Qty(0),
        cash_sum: hft_types::Qty(0),
    }
}
