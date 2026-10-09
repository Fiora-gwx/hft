//! Owned event builders (`testing::ev`): each returns an `Ev` whose `as_ref()` is the `EventRef`
//! the engine would deliver with the same fields.

use hft_event::ConnStateKind;
use hft_types::{AccountId, GroupId, InstId, Px, RunMode, Tsc, VenueId};

use crate::advice::{AdvicePayload, PayloadWriter};
use crate::event::{EventRef, ExecUpdate, GIX_NONE, OrderUpdate};
use crate::view_types::{
    BboView, ExternalFillView, FundingView, LiquidationView, RiskEvent, TradeView, VenueNoticeView,
};

/// An owned event; `as_ref()` borrows it as an `EventRef`.
#[derive(Clone, Debug)]
pub enum Ev {
    Bbo {
        inst: InstId,
        gix: u16,
        b: BboView,
    },
    Book {
        inst: InstId,
        gix: u16,
    },
    Trade {
        inst: InstId,
        gix: u16,
        t: TradeView,
    },
    Funding {
        inst: InstId,
        gix: u16,
        f: FundingView,
    },
    Mark {
        inst: InstId,
        gix: u16,
        px: Px,
    },
    Index {
        inst: InstId,
        gix: u16,
        px: Px,
    },
    Liquidation {
        inst: InstId,
        gix: u16,
        l: LiquidationView,
    },
    Order {
        gix: u16,
        u: OrderUpdate,
    },
    Exec {
        gix: u16,
        u: ExecUpdate,
    },
    External {
        gix: u16,
        f: ExternalFillView,
    },
    Risk {
        a: AccountId,
        e: RiskEvent,
    },
    Notice {
        inst: InstId,
        gix: u16,
        n: VenueNoticeView,
    },
    Timer {
        tag: u32,
        group: GroupId,
        gix: u16,
    },
    Advice {
        gix: u16,
        local_kind: u16,
        payload: Vec<u8>,
        basis_seq: u64,
        basis_tsc: Tsc,
    },
    Mode {
        gix: u16,
        old: RunMode,
        new: RunMode,
    },
    Conn {
        venue: VenueId,
        state: ConnStateKind,
    },
    Manual {
        gix: u16,
        payload: Vec<u8>,
        reduce_only: bool,
    },
    Warmup {
        local_kind: u16,
        chunk_no: u32,
        last: bool,
        payload: Vec<u8>,
    },
}

impl Ev {
    pub fn as_ref(&self) -> EventRef<'_> {
        match self {
            Ev::Bbo { inst, gix, b } => EventRef::Bbo {
                inst: *inst,
                gix: *gix,
                b: *b,
            },
            Ev::Book { inst, gix } => EventRef::Book {
                inst: *inst,
                gix: *gix,
            },
            Ev::Trade { inst, gix, t } => EventRef::Trade {
                inst: *inst,
                gix: *gix,
                t,
            },
            Ev::Funding { inst, gix, f } => EventRef::Funding {
                inst: *inst,
                gix: *gix,
                f,
            },
            Ev::Mark { inst, gix, px } => EventRef::Mark {
                inst: *inst,
                gix: *gix,
                px: *px,
            },
            Ev::Index { inst, gix, px } => EventRef::Index {
                inst: *inst,
                gix: *gix,
                px: *px,
            },
            Ev::Liquidation { inst, gix, l } => EventRef::Liquidation {
                inst: *inst,
                gix: *gix,
                l,
            },
            Ev::Order { gix, u } => EventRef::Order { gix: *gix, u },
            Ev::Exec { gix, u } => EventRef::Exec { gix: *gix, u },
            Ev::External { gix, f } => EventRef::External { gix: *gix, f },
            Ev::Risk { a, e } => EventRef::Risk { a: *a, e },
            Ev::Notice { inst, gix, n } => EventRef::Notice {
                inst: *inst,
                gix: *gix,
                n,
            },
            Ev::Timer { tag, group, gix } => EventRef::Timer {
                tag: *tag,
                group: *group,
                gix: *gix,
            },
            Ev::Advice {
                gix,
                local_kind,
                payload,
                basis_seq,
                basis_tsc,
            } => EventRef::Advice {
                gix: *gix,
                local_kind: *local_kind,
                payload,
                basis_seq: *basis_seq,
                basis_tsc: *basis_tsc,
            },
            Ev::Mode { gix, old, new } => EventRef::Mode {
                gix: *gix,
                old: *old,
                new: *new,
            },
            Ev::Conn { venue, state } => EventRef::Conn {
                venue: *venue,
                state: *state,
            },
            Ev::Manual {
                gix,
                payload,
                reduce_only,
            } => EventRef::Manual {
                gix: *gix,
                payload,
                reduce_only: *reduce_only,
            },
            Ev::Warmup {
                local_kind,
                chunk_no,
                last,
                payload,
            } => EventRef::Warmup {
                local_kind: *local_kind,
                chunk_no: *chunk_no,
                last: *last,
                payload,
            },
        }
    }
}

pub fn bbo(inst: InstId, gix: u16, b: BboView) -> Ev {
    Ev::Bbo { inst, gix, b }
}
pub fn book(inst: InstId, gix: u16) -> Ev {
    Ev::Book { inst, gix }
}
pub fn trade(inst: InstId, gix: u16, t: TradeView) -> Ev {
    Ev::Trade { inst, gix, t }
}
pub fn funding(inst: InstId, gix: u16, f: FundingView) -> Ev {
    Ev::Funding { inst, gix, f }
}
pub fn mark(inst: InstId, gix: u16, px: Px) -> Ev {
    Ev::Mark { inst, gix, px }
}
pub fn index(inst: InstId, gix: u16, px: Px) -> Ev {
    Ev::Index { inst, gix, px }
}
pub fn liquidation(inst: InstId, gix: u16, l: LiquidationView) -> Ev {
    Ev::Liquidation { inst, gix, l }
}
pub fn order(gix: u16, u: OrderUpdate) -> Ev {
    Ev::Order { gix, u }
}
pub fn exec(gix: u16, u: ExecUpdate) -> Ev {
    Ev::Exec { gix, u }
}
pub fn external(gix: u16, f: ExternalFillView) -> Ev {
    Ev::External { gix, f }
}
pub fn risk(a: AccountId, e: RiskEvent) -> Ev {
    Ev::Risk { a, e }
}
pub fn notice(inst: InstId, gix: u16, n: VenueNoticeView) -> Ev {
    Ev::Notice { inst, gix, n }
}
pub fn timer(tag: u32, group: GroupId, gix: u16) -> Ev {
    Ev::Timer { tag, group, gix }
}
pub fn mode(gix: u16, old: RunMode, new: RunMode) -> Ev {
    Ev::Mode { gix, old, new }
}
pub fn conn(venue: VenueId, state: ConnStateKind) -> Ev {
    Ev::Conn { venue, state }
}

/// Advice with `P`'s encoding (an encode failure gives an empty payload).
pub fn advice<P: AdvicePayload>(gix: u16, p: &P, basis_seq: u64, basis_tsc: Tsc) -> Ev {
    let mut buf = vec![0u8; P::MAX_LEN];
    let len = {
        let mut w = PayloadWriter::new(&mut buf, P::MAX_LEN);
        if p.encode(&mut w).is_ok() { w.len() } else { 0 }
    };
    buf.truncate(len);
    advice_raw(gix, P::LOCAL_KIND, &buf, basis_seq, basis_tsc)
}

/// Advice with raw payload bytes.
pub fn advice_raw(gix: u16, local_kind: u16, bytes: &[u8], basis_seq: u64, basis_tsc: Tsc) -> Ev {
    Ev::Advice {
        gix,
        local_kind,
        payload: bytes.to_vec(),
        basis_seq,
        basis_tsc,
    }
}

/// `StrategyCmd` payload `{ver 1, op, n, _pad}` + values (the layout `params::encode_cmd`
/// writes), instance level (`gix = GIX_NONE`).
pub fn manual(op: u8, vals: &[i64], reduce_only: bool) -> Ev {
    manual_at(GIX_NONE, op, vals, reduce_only)
}

/// `manual` for the group at `gix`.
pub fn manual_at(gix: u16, op: u8, vals: &[i64], reduce_only: bool) -> Ev {
    let n = u8::try_from(vals.len()).unwrap_or(u8::MAX);
    let mut payload = vec![1u8, op, n, 0, 0, 0, 0, 0];
    for v in vals {
        payload.extend_from_slice(&v.to_le_bytes());
    }
    Ev::Manual {
        gix,
        payload,
        reduce_only,
    }
}

pub fn warmup(local_kind: u16, chunk_no: u32, last: bool, bytes: &[u8]) -> Ev {
    Ev::Warmup {
        local_kind,
        chunk_no,
        last,
        payload: bytes.to_vec(),
    }
}
