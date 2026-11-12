//! Advanced Trade market data (README "Advanced Trade：level2 → Bbo"): dispatch from the first
//! 64 B, control hints, and `l2_data` frames applied to the connection's book; a `Bbo` goes out
//! when the best bid or ask changes. The feed has no cross-connection sequence, so the two raced
//! connections each keep their own book and the duplicate of a change is dropped here.

use hft_event::{Bbo, MarketHdr};
use hft_refdata::RefData;
use hft_types::{ConnId, FxErr, InstId, Px, Qty, parse_exact};
use hft_venue_proto::{
    ControlHint, DecodeErr, MdOutcome, MdSink, MsgOp, Peek, RawSeq, RouteId, RxMeta, SeqIds, StreamKind,
};

use crate::adv::VENUE;
use crate::adv_book::Book;
use crate::time::iso8601_ns;

/// Market-data dispatch classes of `coinbase-adv`.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum AdvClass {
    /// `l2_data`: a `snapshot` or `update` event per product → the book, `Bbo` on a new best.
    L2,
    /// `heartbeats`: one per second → `ControlHint::Pong`.
    Heartbeat,
    /// `subscriptions`: answer to every (un)subscribe frame, the keep-alive ones included.
    Subscriptions,
    /// `{"type":"error",…}`: a refused frame.
    Error,
    Unknown,
}

pub(crate) fn classify(_route: RouteId, op: MsgOp, head: &[u8]) -> AdvClass {
    if op != MsgOp::Text {
        return AdvClass::Unknown;
    }
    let name = head
        .strip_prefix(b"{\"channel\":\"")
        .and_then(|r| r.iter().position(|&c| c == b'"').and_then(|e| r.get(..e)));
    match name {
        Some(b"l2_data") => AdvClass::L2,
        Some(b"heartbeats") => AdvClass::Heartbeat,
        Some(b"subscriptions") => AdvClass::Subscriptions,
        _ if head.starts_with(b"{\"type\":\"error\"") => AdvClass::Error,
        _ => AdvClass::Unknown,
    }
}

/// Every `subscriptions` frame acknowledges a frame of ours (README "订阅与保活": the answers to
/// the keep-alive subscription are extra acknowledgements the runtime counts as stray).
pub(crate) fn control(c: AdvClass, _payload: &[u8]) -> Option<ControlHint> {
    match c {
        AdvClass::Subscriptions => Some(ControlHint::SubAck { ok: true, code: 0 }),
        AdvClass::Error => Some(ControlHint::SubAck { ok: false, code: 1 }),
        AdvClass::Heartbeat => Some(ControlHint::Pong),
        _ => None,
    }
}

fn inst_of(rd: &RefData, product: &[u8]) -> InstId {
    rd.symbols(VENUE).get(product).unwrap_or(InstId(0))
}

/// The first `"product_id":"…"` of the frame.
fn first_product(p: &[u8]) -> Option<&[u8]> {
    const KEY: &[u8] = b"\"product_id\":\"";
    let at = p.windows(KEY.len()).position(|w| w == KEY)? + KEY.len();
    let rest = p.get(at..)?;
    let end = rest.iter().position(|&c| c == b'"')?;
    rest.get(..end)
}

pub(crate) fn peek(rd: &RefData, c: AdvClass, p: &[u8]) -> Result<Peek, DecodeErr> {
    match c {
        AdvClass::L2 => Ok(Peek {
            inst: first_product(p).map_or(InstId(0), |s| inst_of(rd, s)),
            stream: StreamKind::Bbo,
            ids: SeqIds {
                prev_id: 0,
                last_id: 0,
            },
            raw: RawSeq::default(),
        }),
        _ => Err(DecodeErr::UnknownClass),
    }
}

fn fixed(s: &[u8], exp: i8) -> Result<i64, DecodeErr> {
    parse_exact(s, exp).map_err(|e| {
        if e.0 & FxErr::OVERFLOW.0 != 0 {
            DecodeErr::Overflow
        } else {
            DecodeErr::Number
        }
    })
}

/// A cursor over the frame's JSON text. The feed writes no escapes in the strings this crate
/// reads; a backslash inside one is a `Shape` error, never guessed.
struct Cur<'a> {
    p: &'a [u8],
    i: usize,
}

impl<'a> Cur<'a> {
    /// The next byte after whitespace; the input ending → `Truncated`.
    fn peek(&mut self) -> Result<u8, DecodeErr> {
        while let Some(&c) = self.p.get(self.i) {
            if !c.is_ascii_whitespace() {
                return Ok(c);
            }
            self.i += 1;
        }
        Err(DecodeErr::Truncated)
    }
    fn eat(&mut self, c: u8) -> Result<(), DecodeErr> {
        if self.peek()? == c {
            self.i += 1;
            Ok(())
        } else {
            Err(DecodeErr::Shape)
        }
    }
    /// `"…"` → its content.
    fn string(&mut self) -> Result<&'a [u8], DecodeErr> {
        self.eat(b'"')?;
        let rest = self.p.get(self.i..).unwrap_or(&[]);
        let end = rest
            .iter()
            .position(|&c| c == b'"' || c == b'\\')
            .ok_or(DecodeErr::Truncated)?;
        if rest.get(end) != Some(&b'"') {
            return Err(DecodeErr::Shape);
        }
        self.i += end + 1;
        rest.get(..end).ok_or(DecodeErr::Shape)
    }
    /// Skips one value of any kind.
    fn skip(&mut self) -> Result<(), DecodeErr> {
        let mut depth = 0usize;
        loop {
            match self.peek()? {
                b'"' => {
                    self.string()?;
                }
                b'{' | b'[' => {
                    depth += 1;
                    self.i += 1;
                }
                b'}' | b']' => {
                    depth = depth.checked_sub(1).ok_or(DecodeErr::Shape)?;
                    self.i += 1;
                }
                b',' | b':' if depth > 0 => self.i += 1,
                _ => {
                    // A number or a literal: up to the next delimiter.
                    let rest = self.p.get(self.i..).unwrap_or(&[]);
                    let n = rest
                        .iter()
                        .position(|c| matches!(c, b',' | b'}' | b']' | b':') || c.is_ascii_whitespace())
                        .ok_or(DecodeErr::Truncated)?;
                    if n == 0 {
                        return Err(DecodeErr::Shape);
                    }
                    self.i += n;
                }
            }
            if depth == 0 {
                return Ok(());
            }
        }
    }
    /// After a member or element: `true` when another follows (`,` consumed), `false` at `close`
    /// (consumed).
    fn more(&mut self, close: u8) -> Result<bool, DecodeErr> {
        let c = self.peek()?;
        self.i += 1;
        if c == b',' {
            Ok(true)
        } else if c == close {
            Ok(false)
        } else {
            Err(DecodeErr::Shape)
        }
    }
    /// `{` then either `}` (false) or the first key (true, not consumed).
    fn open(&mut self, open: u8, close: u8) -> Result<bool, DecodeErr> {
        self.eat(open)?;
        if self.peek()? == close {
            self.i += 1;
            Ok(false)
        } else {
            Ok(true)
        }
    }
}

/// Every receive field of `rx` copied unchanged (contract 3).
fn hdr(inst: InstId, ex_ts_ns: i64, rx: &RxMeta) -> MarketHdr {
    MarketHdr {
        venue: VENUE,
        conn: rx.conn,
        _pad: 0,
        inst,
        ex_ts_ns,
        ex_send_ns: 0,
        hw_rx_ns: rx.hw_rx_ns,
        rx_tsc: rx.rx_tsc,
        sw_rx_ns: rx.sw_rx_ns,
        rx_empty_gap_ns: rx.rx_empty_gap_ns,
        _rsv: 0,
    }
}

/// Books kept: (connection, product) pairs — two raced connections of up to two products.
pub const BOOKS: usize = 4;

/// The best quote last forwarded for one inst.
#[derive(Copy, Clone, Debug)]
struct Last {
    inst: InstId,
    conn: ConnId,
    ex_ts_ns: i64,
    top: (i64, i64, i64, i64),
}

impl Last {
    const NONE: Last = Last {
        inst: InstId(0),
        conn: ConnId(0),
        ex_ts_ns: 0,
        top: (0, 0, 0, 0),
    };
}

/// Feed state of `coinbase-adv`: the level-2 books and the last forwarded best quote per inst.
/// Allocated once at start-up.
#[derive(Clone, Debug)]
pub struct AdvFeedCtx {
    books: Vec<Book>,
    last: [Last; BOOKS],
    stamp: u64,
}

impl Default for AdvFeedCtx {
    fn default() -> AdvFeedCtx {
        AdvFeedCtx {
            books: (0..BOOKS).map(|_| Book::new()).collect(),
            last: [Last::NONE; BOOKS],
            stamp: 0,
        }
    }
}

impl AdvFeedCtx {
    /// The book of `(conn, inst)`; a snapshot (`alloc`) takes a free slot or recycles the one
    /// least recently used.
    fn book(&mut self, conn: ConnId, inst: InstId, alloc: bool) -> Option<&mut Book> {
        self.stamp += 1;
        let stamp = self.stamp;
        let at = self
            .books
            .iter()
            .position(|b| b.inst == inst && b.conn == conn)
            .or_else(|| {
                alloc.then(|| {
                    self.books
                        .iter()
                        .enumerate()
                        .min_by_key(|(_, b)| if b.inst.0 == 0 { 0 } else { b.used })
                        .map_or(0, |(i, _)| i)
                })
            })?;
        let b = self.books.get_mut(at)?;
        b.used = stamp;
        Some(b)
    }

    /// Whether the best quote `top` of `inst`, seen on `conn` with exchange time `ex_ts_ns`, is
    /// new: not the one last forwarded, and from the connection that forwarded last or strictly
    /// later than it (the slower connection's replay of older states is dropped).
    fn fresh(&mut self, inst: InstId, conn: ConnId, ex_ts_ns: i64, top: (i64, i64, i64, i64)) -> bool {
        let at = self
            .last
            .iter()
            .position(|l| l.inst == inst)
            .or_else(|| self.last.iter().position(|l| l.inst.0 == 0))
            .unwrap_or(0);
        let Some(l) = self.last.get_mut(at) else {
            return false;
        };
        let first = l.inst != inst;
        if !first && (l.top == top || (l.conn != conn && ex_ts_ns <= l.ex_ts_ns)) {
            return false;
        }
        *l = Last {
            inst,
            conn,
            ex_ts_ns,
            top,
        };
        true
    }
}

/// One `updates` element: `{"side":…,"event_time":…,"price_level":…,"new_quantity":…}`.
struct Level<'a> {
    bid: bool,
    time: &'a [u8],
    px: &'a [u8],
    qty: &'a [u8],
}

fn level<'a>(c: &mut Cur<'a>) -> Result<Level<'a>, DecodeErr> {
    let (mut side, mut time, mut px, mut qty) = (None, None, None, None);
    let mut any = c.open(b'{', b'}')?;
    while any {
        let key = c.string()?;
        c.eat(b':')?;
        match key {
            b"side" => side = Some(c.string()?),
            b"event_time" => time = Some(c.string()?),
            b"price_level" => px = Some(c.string()?),
            b"new_quantity" => qty = Some(c.string()?),
            _ => c.skip()?,
        }
        any = c.more(b'}')?;
    }
    let bid = match side.ok_or(DecodeErr::Shape)? {
        b"bid" => true,
        b"offer" => false,
        _ => return Err(DecodeErr::Shape),
    };
    Ok(Level {
        bid,
        time: time.ok_or(DecodeErr::Shape)?,
        px: px.ok_or(DecodeErr::Shape)?,
        qty: qty.ok_or(DecodeErr::Shape)?,
    })
}

/// One event `{"type":…,"product_id":…,"updates":[…]}` at the cursor: its updates applied to the
/// connection's book of the product; `Some((inst, exchange time, best))` when the book's best
/// quote changed.
#[allow(clippy::type_complexity)]
fn event(
    rd: &RefData,
    ctx: &mut AdvFeedCtx,
    c: &mut Cur<'_>,
    frame_ts_ns: i64,
    conn: ConnId,
) -> Result<(InstId, Option<(i64, (i64, i64, i64, i64))>), DecodeErr> {
    // First pass: the kind and the product (the updates are skipped), then the updates.
    let start = c.i;
    let (mut kind, mut product, mut updates) = (None, None, None);
    let mut any = c.open(b'{', b'}')?;
    while any {
        let key = c.string()?;
        c.eat(b':')?;
        match key {
            b"type" => kind = Some(c.string()?),
            b"product_id" => product = Some(c.string()?),
            b"updates" => {
                c.peek()?;
                updates = Some(c.i);
                c.skip()?;
            }
            _ => c.skip()?,
        }
        any = c.more(b'}')?;
    }
    let end = c.i;
    let snapshot = match kind.ok_or(DecodeErr::Shape)? {
        b"snapshot" => true,
        b"update" => false,
        _ => return Err(DecodeErr::Shape),
    };
    let inst = inst_of(rd, product.ok_or(DecodeErr::Shape)?);
    let at = updates.ok_or(DecodeErr::Shape)?;
    if inst.0 == 0 || at < start {
        return Ok((inst, None));
    }
    let row = rd.inst(inst);
    let (pe, qe) = (row.px_wire_exp, row.qty_wire_exp);
    let Some(book) = ctx.book(conn, inst, snapshot) else {
        return Ok((inst, None));
    };
    if snapshot {
        book.reset(conn, inst);
        book.live = true;
    } else if !book.live {
        return Ok((inst, None));
    }
    let mut u = Cur { p: c.p, i: at };
    let (mut ts, mut ts_text): (i64, &[u8]) = (0, &[]);
    let mut any = u.open(b'[', b']')?;
    while any {
        let l = level(&mut u)?;
        if l.time != ts_text {
            ts = ts.max(iso8601_ns(l.time)?);
            ts_text = l.time;
        }
        let (px, qty) = (fixed(l.px, pe)?, fixed(l.qty, qe)?);
        if l.bid {
            book.bids.set(px, qty);
        } else {
            book.asks.set(px, qty);
        }
        any = u.more(b']')?;
    }
    if u.i > end {
        return Err(DecodeErr::Shape);
    }
    let Some(top) = book.bbo() else {
        return Ok((inst, None));
    };
    if top == book.top {
        return Ok((inst, None));
    }
    book.top = top;
    Ok((inst, Some((if ts > 0 { ts } else { frame_ts_ns }, top))))
}

pub(crate) fn decode_md(
    rd: &RefData,
    ctx: &mut AdvFeedCtx,
    c: AdvClass,
    p: &[u8],
    rx: &RxMeta,
    out: &mut impl MdSink,
) -> Result<MdOutcome, DecodeErr> {
    if c != AdvClass::L2 {
        return Err(DecodeErr::UnknownClass);
    }
    // First pass over the frame: its time and where the events start.
    let mut cur = Cur { p, i: 0 };
    let (mut frame_ts, mut events) = (0i64, None);
    let mut any = cur.open(b'{', b'}')?;
    while any {
        let key = cur.string()?;
        cur.eat(b':')?;
        match key {
            b"timestamp" => frame_ts = iso8601_ns(cur.string()?)?,
            b"events" => {
                cur.peek()?;
                events = Some(cur.i);
                cur.skip()?;
            }
            _ => cur.skip()?,
        }
        any = cur.more(b'}')?;
    }
    let mut cur = Cur {
        p,
        i: events.ok_or(DecodeErr::Shape)?,
    };
    let mut out_inst = InstId(0);
    let mut n = 0u16;
    let mut any = cur.open(b'[', b']')?;
    while any {
        let (inst, change) = event(rd, ctx, &mut cur, frame_ts, rx.conn)?;
        if inst.0 != 0 {
            out_inst = inst;
        }
        if let Some((ts, top)) = change
            && ctx.fresh(inst, rx.conn, ts, top)
        {
            let e = Bbo {
                h: hdr(inst, ts, rx),
                bid_px: Px(top.0),
                bid_qty: Qty(top.1),
                ask_px: Px(top.2),
                ask_qty: Qty(top.3),
                update_id: u64::try_from(ts / 1_000).unwrap_or(0).max(1),
            };
            out.bbo(&e);
            n = n.saturating_add(1);
        }
        any = cur.more(b']')?;
    }
    Ok(MdOutcome {
        events: n,
        inst: out_inst,
    })
}
