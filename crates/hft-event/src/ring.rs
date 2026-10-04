//! In-process ring element types (`FeedMsg`, `RepMsg`, `HkMsg`, `OutMsg` = `GwCmd`, `FeedCmd`) and
//! the byte-ring frames of the ctl and advice rings (`CtlRecord`, `AdviceMsg`).
// zerocopy's derive helpers inside `event_body!` expansions carry no Debug impl.
#![allow(missing_debug_implementations)]

use hft_types::{AccountId, AssetId, ConnId, InstId, Px, Qty, Side, SlotId, Tif, Tsc, VenueId};
use zerocopy::{FromBytes, IntoBytes, TryFromBytes};

use crate::bytes::{put, round8};
use crate::exec::{
    AccountConfigReportHdr, BalanceReport, ExternalFill, FundingSettle, GwOp, MarginReportHdr, OrderEvent,
    PositionReport, SendReport,
};
use crate::header::RecordFlags;
use crate::macros::layout;
use crate::market::{Bbo, Funding, IndexPrice, L2Delta, Liquidation, MarkPrice, Trade};
use crate::strategy::AdviceRecord;
use crate::system::{ClockStatus, ConnState, Note, NoteSource};
use crate::{DecodeErr, EncodeErr, EventKind};

/// `bbo-<venue>` / `md-<venue>` ring element.
#[derive(Copy, Clone, Debug)]
pub enum FeedMsg {
    Bbo(Bbo),
    L2Delta(L2Delta),
    Trade(Trade),
    Funding(Funding),
    MarkPrice(MarkPrice),
    IndexPrice(IndexPrice),
    Liquidation(Liquidation),
    ConnState(ConnState),
    /// Next variable logical body (L2Batch, L2Snapshot) is in the paired `md-<venue>-bulk` byte ring.
    Bulk {
        kind: EventKind,
        len: u32,
    },
}
const _: () = assert!(size_of::<FeedMsg>() <= 128);

/// `rep-<venue>` / `rep-sim` ring element.
#[derive(Copy, Clone, Debug)]
pub enum RepMsg {
    OrderEvent(OrderEvent),
    SendReport(SendReport),
    PositionReport(PositionReport),
    BalanceReport(BalanceReport),
    FundingSettle(FundingSettle),
    ConnState(ConnState),
    ExternalFill(ExternalFill),
    /// Followed by `n_lev` `LeverageItem`s in `rep-<venue>-bulk`.
    AccountConfigReport(AccountConfigReportHdr),
    /// Followed by `n` `MarginItem`s in `rep-<venue>-bulk`.
    MarginReport(MarginReportHdr),
}
const _: () = assert!(size_of::<RepMsg>() <= 128);

/// `hk` ring element (housekeeping → sequencer).
#[derive(Copy, Clone, Debug)]
pub enum HkMsg {
    ClockStatus(ClockStatus),
    /// `source ∈ {Housekeeping, Boot}`.
    Note(Note),
    /// Environment observation as a packed Sys `timer_id`; the live driver writes `Timer{Sys}`.
    Sys {
        timer_id: u32,
    },
}
const _: () = assert!(size_of::<HkMsg>() <= 128);

/// sequencer → feed control (`feedctl-<venue>`).
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum FeedCmd {
    Resync {
        inst: InstId,
    },
    Rotate {
        conn: ConnId,
    },
    /// `streams`: union of the SDK `EvMask` bits for this inst.
    Subscribe {
        inst: InstId,
        streams: u8,
    },
    Unsubscribe {
        inst: InstId,
    },
}
const _: () = assert!(size_of::<FeedCmd>() <= 16);

crate::event_body! {
    /// `out-<venue>` / `out-sim` ring element (64 B). Maintenance and sim ops carry
    /// placeholder `side = Buy`, `tif = Gtc` (not read).
    pub struct GwCmd : plain, TryFromBytes {
        pub decision_seq: u64,
        pub decided_tsc: Tsc,
        pub px: Px,
        pub qty: Qty,
        pub inst: InstId,
        pub slot: SlotId,
        pub r#gen: u32,
        pub target_slot: SlotId,
        pub op: GwOp,
        pub account: AccountId,
        pub side: Side,
        pub tif: Tif,
        pub oflags: u16,
        pub _pad: [u8; 2],
        pub arg: u32,
        pub arg2: u32,
    }
}
layout!(GwCmd, 64, 8; decision_seq @ 0, decided_tsc @ 8, px @ 16, qty @ 24, inst @ 32, slot @ 36, r#gen @ 40,
    target_slot @ 44, op @ 48, account @ 49, side @ 50, tif @ 51, oflags @ 52, _pad @ 54, arg @ 56, arg2 @ 60);

/// The only out-ring element type.
pub type OutMsg = GwCmd;

impl GwCmd {
    fn base(op: GwOp, account: AccountId) -> GwCmd {
        GwCmd {
            decision_seq: 0,
            decided_tsc: Tsc(0),
            px: Px(0),
            qty: Qty(0),
            inst: InstId(0),
            slot: SlotId(0),
            r#gen: 0,
            target_slot: SlotId(0),
            op,
            account,
            side: Side::Buy,
            tif: Tif::Gtc,
            oflags: 0,
            _pad: [0; 2],
            arg: 0,
            arg2: 0,
        }
    }

    /// Rebuild a gw slot mirror entry; the exchange id rides in `px`'s 64 bits, the epoch in `arg`.
    pub fn mirror_restore(
        account: AccountId,
        inst: InstId,
        slot: SlotId,
        r#gen: u32,
        epoch: u16,
        exch_order_id: u64,
    ) -> GwCmd {
        GwCmd {
            px: Px(exch_order_id as i64),
            inst,
            slot,
            r#gen,
            arg: u32::from(epoch),
            ..Self::base(GwOp::MirrorRestore, account)
        }
    }

    /// MirrorRestore only.
    pub fn exch_order_id(&self) -> u64 {
        self.px.0 as u64
    }

    /// SetEpoch / MirrorRestore only: low 16 bits of `arg`.
    pub fn epoch(&self) -> u16 {
        self.arg as u16
    }

    /// `op = SetEpoch`; `arg2 = accept_prev | prev_epoch << 16`.
    pub fn set_epoch(epoch: u16, accept_prev: bool, prev_epoch: u16) -> GwCmd {
        GwCmd {
            arg: u32::from(epoch),
            arg2: u32::from(accept_prev) | u32::from(prev_epoch) << 16,
            ..Self::base(GwOp::SetEpoch, AccountId(0))
        }
    }

    /// SetEpoch only: `arg2` bit 0.
    pub fn accept_prev(&self) -> bool {
        self.arg2 & 1 == 1
    }

    /// SetEpoch only: high 16 bits of `arg2`.
    pub fn prev_epoch(&self) -> u16 {
        (self.arg2 >> 16) as u16
    }

    pub fn sim_reset(account: AccountId) -> GwCmd {
        Self::base(GwOp::SimReset, account)
    }

    /// `qty = amount`, `arg = asset | from << 16 | to << 24`.
    pub fn sim_transfer(
        account: AccountId,
        from: VenueId,
        to: VenueId,
        asset: AssetId,
        amount: Qty,
    ) -> GwCmd {
        GwCmd {
            qty: amount,
            arg: u32::from(asset.0) | u32::from(from.0) << 16 | u32::from(to.0) << 24,
            ..Self::base(GwOp::SimTransfer, account)
        }
    }

    /// `arg = venue | fault << 8`, `arg2 = delay_ms`.
    pub fn sim_fault(account: AccountId, venue: VenueId, fault: u8, delay_ms: u32) -> GwCmd {
        GwCmd {
            arg: u32::from(venue.0) | u32::from(fault) << 8,
            arg2: delay_ms,
            ..Self::base(GwOp::SimFault, account)
        }
    }
}

crate::event_body! {
    /// Frame header of the ctl and advice byte rings (8 B), followed by `len` body bytes.
    pub struct CtlRecordHdr : plain, FromBytes {
        pub kind: EventKind,
        /// `RecordFlags` bits (PART …).
        pub flags: u16,
        pub len: u32,
    }
}
layout!(CtlRecordHdr, 8, 4; kind @ 0, flags @ 2, len @ 4);

/// One ctl-ring frame.
#[derive(Copy, Clone, Debug)]
pub struct CtlRecord<'a> {
    pub kind: EventKind,
    pub flags: RecordFlags,
    pub body: &'a [u8],
}

fn frame_hdr(b: &[u8]) -> Result<(CtlRecordHdr, &[u8]), DecodeErr> {
    let (h, rest) = CtlRecordHdr::read_from_prefix(b).map_err(|_| DecodeErr::Short)?;
    let body = rest.get(..h.len as usize).ok_or(DecodeErr::Short)?;
    Ok((h, body))
}

fn write_frame(
    out: &mut [u8],
    kind: EventKind,
    flags: u16,
    body_len: usize,
    n: usize,
) -> Result<&mut [u8], EncodeErr> {
    let len = u32::try_from(body_len).map_err(|_| EncodeErr::TooLong)?;
    let dst = out.get_mut(..n).ok_or(EncodeErr::BufTooSmall)?;
    put(dst, 0, CtlRecordHdr { kind, flags, len }.as_bytes());
    if let Some(rest) = dst.get_mut(8..) {
        rest.fill(0);
    }
    Ok(dst)
}

impl<'a> CtlRecord<'a> {
    /// `8 + body.len()` rounded up to 8.
    pub fn encoded_len(&self) -> usize {
        round8(8 + self.body.len())
    }

    /// Writes header, body and zero padding.
    pub fn encode_into(&self, out: &mut [u8]) -> Result<usize, EncodeErr> {
        let n = self.encoded_len();
        let dst = write_frame(out, self.kind, self.flags.0, self.body.len(), n)?;
        put(dst, 8, self.body);
        Ok(n)
    }

    /// Kind must be ControlCmd, ConfigBlob, ReconcileReport or Note.
    pub fn read(b: &'a [u8]) -> Result<Self, DecodeErr> {
        let (h, body) = frame_hdr(b)?;
        if !matches!(
            h.kind,
            EventKind::CONTROL_CMD | EventKind::CONFIG_BLOB | EventKind::RECONCILE_REPORT | EventKind::NOTE
        ) {
            return Err(DecodeErr::KindMismatch);
        }
        Ok(CtlRecord {
            kind: h.kind,
            flags: RecordFlags(h.flags),
            body,
        })
    }
}

/// advice-ring element.
#[derive(Copy, Clone, Debug)]
pub enum AdviceMsg<'a> {
    Advice(AdviceRecord<'a>),
    /// `source = Advisor` only.
    Note(Note),
}

impl<'a> AdviceMsg<'a> {
    /// `8 +` body length (Advice: `AdviceRecord::encoded_len`; Note: 80).
    pub fn encoded_len(&self) -> usize {
        8 + match self {
            AdviceMsg::Advice(a) => a.encoded_len(),
            AdviceMsg::Note(_) => size_of::<Note>(),
        }
    }

    pub fn encode_into(&self, out: &mut [u8]) -> Result<usize, EncodeErr> {
        let n = self.encoded_len();
        match self {
            AdviceMsg::Advice(a) => {
                // Validate before touching `out`.
                if a.payload.len() > crate::ADVICE_MAX_PAYLOAD {
                    return Err(EncodeErr::TooLong);
                }
                if usize::from(a.hdr.len) != a.payload.len() {
                    return Err(EncodeErr::Count);
                }
                let dst = write_frame(out, EventKind::ADVICE, 0, n - 8, n)?;
                let body = dst.get_mut(8..).ok_or(EncodeErr::BufTooSmall)?;
                a.encode_into(body)?;
            }
            AdviceMsg::Note(note) => {
                if note.source != NoteSource::Advisor {
                    return Err(EncodeErr::Count);
                }
                let dst = write_frame(out, EventKind::NOTE, 0, size_of::<Note>(), n)?;
                put(dst, 8, note.as_bytes());
            }
        }
        Ok(n)
    }

    pub fn read(b: &'a [u8]) -> Result<Self, DecodeErr> {
        let (h, body) = frame_hdr(b)?;
        match h.kind {
            EventKind::ADVICE => AdviceRecord::read(body).map(AdviceMsg::Advice),
            EventKind::NOTE => {
                if body.len() < size_of::<Note>() {
                    return Err(DecodeErr::Short);
                }
                if body.len() > size_of::<Note>() {
                    return Err(DecodeErr::TooLong);
                }
                let note = Note::try_read_from_bytes(body).map_err(|_| DecodeErr::InvalidEnum)?;
                if note.source != NoteSource::Advisor {
                    return Err(DecodeErr::InvalidEnum);
                }
                Ok(AdviceMsg::Note(note))
            }
            _ => Err(DecodeErr::KindMismatch),
        }
    }
}
