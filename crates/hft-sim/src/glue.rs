//! Journal glue shared by the hft-core sim-gw and the research replay (README "journal 衔接"):
//! a seq-journal record → `on_record`, the task-layer `SendReport{Written}` of an order command,
//! and a `SimReply` → the Input records it becomes.

use hft_event::{
    Bbo, EventKind, Funding, GwCmd, GwOp, L2BatchHdr, L2BatchView, L2SnapHdr, L2SnapshotPart, Level,
    MarkPrice, RecordFlags, RecordHeader, RepMsg, SendReport, SendStatus, Trade,
};
use hft_refdata::RefData;
use hft_types::{ConnId, SlotId, Tsc, VenueId};
use zerocopy::FromBytes;

use crate::{MarketEventRef, SimExchange, SimInput};

/// What `feed_record` did with a record.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub enum Fed {
    /// A market record (Bbo, Trade, MarkPrice, Funding, exchange or TopN `L2Snapshot`, `L2Batch`).
    Market,
    /// Any other record: its tsc only advances time.
    Clock,
    /// An engine keyframe `L2Snapshot` (book state, not market data) or an undecodable market body:
    /// nothing fed.
    Skipped,
}

/// One seq-journal record (logical body, multi-part records reassembled) to the simulated
/// exchange, in journal order. `levels` is caller-owned scratch (reused, never shrunk).
pub fn feed_record(
    ex: &mut SimExchange,
    hdr: &RecordHeader,
    kind: EventKind,
    body: &[u8],
    levels: &mut Vec<Level>,
) -> Fed {
    let market = |ex: &mut SimExchange, m: MarketEventRef<'_>| {
        ex.on_record(hdr, SimInput::Market(m));
        Fed::Market
    };
    match kind {
        EventKind::BBO => match body_of::<Bbo>(body) {
            Some(b) => market(ex, MarketEventRef::Bbo(&b)),
            None => Fed::Skipped,
        },
        EventKind::TRADE => match body_of::<Trade>(body) {
            Some(t) => market(ex, MarketEventRef::Trade(&t)),
            None => Fed::Skipped,
        },
        EventKind::MARK_PRICE => match body_of::<MarkPrice>(body) {
            Some(m) => market(ex, MarketEventRef::Mark(&m)),
            None => Fed::Skipped,
        },
        EventKind::FUNDING => match body_of::<Funding>(body) {
            Some(f) => market(ex, MarketEventRef::Funding(&f)),
            None => Fed::Skipped,
        },
        EventKind::L2_SNAPSHOT => {
            let Ok((h, rest)) = L2SnapHdr::read_from_prefix(body) else {
                return Fed::Skipped;
            };
            if h.origin == L2SnapHdr::KEYFRAME {
                return Fed::Skipped;
            }
            let nb = usize::from(h.n_bids);
            read_levels(levels, rest, nb + usize::from(h.n_asks));
            let split = nb.min(levels.len());
            let (bids, asks) = levels.split_at(split);
            market(
                ex,
                MarketEventRef::L2Snapshot(L2SnapshotPart {
                    hdr: h,
                    bids,
                    asks,
                    part: 0,
                    parts: 1,
                }),
            )
        }
        EventKind::L2_BATCH => {
            let Ok((h, rest)) = L2BatchHdr::read_from_prefix(body) else {
                return Fed::Skipped;
            };
            read_levels(levels, rest, usize::from(h.n_bids) + usize::from(h.n_asks));
            market(ex, MarketEventRef::L2Batch(L2BatchView { hdr: h, levels }))
        }
        _ => {
            ex.on_record(hdr, SimInput::Clock);
            Fed::Clock
        }
    }
}

/// A fixed body from a logical body written by an older schema version (appended fields read 0).
pub(crate) fn body_of<B: hft_event::Body>(body: &[u8]) -> Option<B> {
    let size = size_of::<B>();
    if let Some(b) = body.get(..size) {
        return B::try_read_from_bytes(b).ok();
    }
    let mut buf = [0u8; 512];
    let dst = buf.get_mut(..size)?;
    dst.get_mut(..body.len())?.copy_from_slice(body);
    B::try_read_from_bytes(dst).ok()
}

fn read_levels(levels: &mut Vec<Level>, rest: &[u8], n: usize) {
    levels.clear();
    for c in rest.chunks_exact(size_of::<Level>()).take(n) {
        if let Ok(l) = Level::read_from_bytes(c) {
            levels.push(l);
        }
    }
}

/// The task layer's transport acknowledgement of an order-class command (`Written` at `now`);
/// matching and exchange evidence come later from `SimExchange`. Other ops → `None`.
pub fn written(cmd: &GwCmd, venue: VenueId, now: Tsc) -> Option<SendReport> {
    let (slot, r#gen, target_slot, target_gen) = match cmd.op {
        GwOp::Place => (cmd.slot, cmd.r#gen, SlotId(0), 0),
        GwOp::Cancel | GwOp::Amend | GwOp::Query => (cmd.target_slot, cmd.r#gen, cmd.target_slot, cmd.r#gen),
        GwOp::CancelAll => (SlotId(0), 0, SlotId(0), 0),
        _ => return None,
    };
    Some(SendReport {
        decision_seq: cmd.decision_seq,
        sent_tsc: now,
        slot,
        r#gen,
        target_slot,
        target_gen,
        venue,
        account: cmd.account,
        conn: ConnId(0),
        status: SendStatus::Written,
        op: cmd.op,
        _pad: [0; 3],
        os_errno: 0,
        _pad2: 0,
    })
}

/// The Input records of one reply: an account-wide `ConnState` (venue 0) is expanded to every venue
/// of the account; an `OrderEvent` gets the engine `epoch` (the pure simulator has none, OMS
/// matches `(slot, gen, epoch)`).
pub fn reply_parts(
    rd: &RefData,
    epoch: u16,
    msg: &RepMsg,
    bulk: &[u8],
    mut emit: impl FnMut(EventKind, &[&[u8]]),
) {
    if let RepMsg::ConnState(c) = msg
        && c.venue.0 == 0
        && c.account.0 != 0
    {
        let venues = rd.account(c.account).venues;
        for v in 1..64 {
            if venues & (1u64 << v) != 0 {
                let mut state = *c;
                state.venue = VenueId(v);
                RepMsg::ConnState(state).with_parts(&[], &mut emit);
            }
        }
    } else if let RepMsg::OrderEvent(e, _) = msg {
        let mut event = *e;
        event.epoch = epoch;
        RepMsg::OrderEvent(event, RecordFlags(0)).with_parts(bulk, emit);
    } else {
        msg.with_parts(bulk, emit);
    }
}
