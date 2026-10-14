//! SPSC adapters. Each endpoint moves to exactly one task.
use crate::Strategies;
use hft_event::*;
use hft_rt::{ByteConsumer, ByteProducer, Consumer, KillWord, MailReceiver, MailSender, Producer, TscClock};
use hft_sequencer::driver::{LiveIo, RingId, RollInfo};
use hft_sequencer::{InputSlot, JournalSink, OutRing, ParsedConfig};
use hft_types::{InstId, KillBits, Tsc, VenueId};
use zerocopy::{FromBytes, FromZeros, IntoBytes};

/// A complete ctl request (audit + blob chunks + command) occupies one byte-ring reservation.
#[derive(Debug)]
pub struct AtomicCtl(pub ByteProducer);
impl hft_ctl::CtlRing for AtomicCtl {
    fn fits(&mut self, lens: &[usize]) -> bool {
        lens.iter()
            .try_fold(0u32, |n, l| n.checked_add(u32::try_from(*l).ok()?))
            .is_some_and(|n| n != 0 && self.0.reserve(n).is_some())
    }
    fn write_all(&mut self, recs: &[hft_ctl::ring::Rec]) -> bool {
        let Some(n) = recs.iter().try_fold(0u32, |n, r| {
            n.checked_add(u32::try_from(hft_ctl::ring::rec_len(r)).ok()?)
        }) else {
            return false;
        };
        if n == 0 {
            return false;
        }
        let Some(buf) = self.0.reserve(n) else {
            return false;
        };
        let mut at = 0;
        for (kind, flags, body) in recs {
            let r = CtlRecord {
                kind: *kind,
                flags: *flags,
                body,
            };
            let end = at + r.encoded_len();
            if r.encode_into(&mut buf[at..end]).is_err() {
                return false;
            }
            at = end;
        }
        self.0.commit();
        true
    }
}
#[derive(Debug)]
pub struct Inputs {
    pub ring: ByteConsumer,
    offset: usize,
    assembled: Vec<u8>,
}
impl Inputs {
    pub fn new(ring: ByteConsumer) -> Self {
        // Maximum atomic publication is one quarter of the configured 4 MiB byte ring.
        Self {
            ring,
            offset: 0,
            assembled: Vec::with_capacity(1 << 20),
        }
    }
    pub fn pending(&self) -> bool {
        self.ring.has_data()
    }
    pub fn peek_len(&mut self) -> Option<usize> {
        let r = frame(self.ring.peek()?.get(self.offset..)?)?;
        if matches!(r.kind, EventKind::CONTROL_CMD | EventKind::CONFIG_BLOB) {
            Some(PartHdr::read_from_prefix(r.body).ok()?.0.total_len as usize)
        } else {
            Some(r.body.len())
        }
    }
    pub fn pop(&mut self, now: Tsc, out: &mut InputSlot) -> bool {
        self.assembled.clear();
        let mut next_part = 0;
        loop {
            let Some(buf) = self.ring.peek() else { return false };
            let Some(r) = frame(&buf[self.offset..]) else {
                return false;
            };
            let mut done = true;
            if matches!(r.kind, EventKind::CONTROL_CMD | EventKind::CONFIG_BLOB) {
                // Gateway records carry physical PartHdrs; LiveDriver accepts logical inputs.
                let Ok((h, rest)) = PartHdr::read_from_prefix(r.body) else {
                    return false;
                };
                let Some(chunk) = rest.get(..h.chunk_len as usize) else {
                    return false;
                };
                if h.part != next_part
                    || h.parts == 0
                    || h.part >= h.parts
                    || h.total_len as usize > self.assembled.capacity()
                    || self.assembled.len() + chunk.len() > h.total_len as usize
                {
                    return false;
                }
                self.assembled.extend_from_slice(chunk);
                next_part += 1;
                done = next_part == h.parts;
                if done {
                    if self.assembled.len() != h.total_len as usize {
                        return false;
                    }
                    out.set_bytes(
                        r.kind,
                        RecordFlags(r.flags.0 & !RecordFlags::PART.0),
                        now,
                        &self.assembled,
                    );
                }
            } else {
                if next_part != 0 {
                    return false;
                }
                out.set_bytes(r.kind, r.flags, now, r.body);
            }
            self.offset += r.encoded_len();
            if self.offset == buf.len() {
                self.ring.advance();
                self.offset = 0;
            }
            if done {
                return true;
            }
        }
    }
}
// Both ctl batches and simulated replies use this framing. CtlRecord::read intentionally
// accepts only ctl input kinds, so it cannot decode OrderEvent/SendReport from sim-gw.
fn frame(bytes: &[u8]) -> Option<CtlRecord<'_>> {
    let (h, rest) = CtlRecordHdr::read_from_prefix(bytes).ok()?;
    let body = rest.get(..h.len as usize)?;
    let r = CtlRecord {
        kind: h.kind,
        flags: RecordFlags(h.flags),
        body,
    };
    (bytes.len() >= r.encoded_len()).then_some(r)
}
pub fn write_record(tx: &mut ByteProducer, kind: EventKind, pieces: &[&[u8]]) -> bool {
    let len: usize = pieces.iter().map(|p| p.len()).sum();
    let total = (8 + len).div_ceil(8) * 8;
    let Some(b) = tx.reserve(total as u32) else {
        return false;
    };
    b.fill(0);
    let h = CtlRecordHdr {
        kind,
        flags: 0,
        len: len as u32,
    };
    b[..8].copy_from_slice(h.as_bytes());
    let mut at = 8;
    for p in pieces {
        b[at..at + p.len()].copy_from_slice(p);
        at += p.len();
    }
    tx.commit();
    true
}
#[derive(Debug)]
pub struct FeedInput {
    pub venue: VenueId,
    pub bbo: Consumer<FeedMsg>,
    pub md: Consumer<FeedMsg>,
    pub bulk: ByteConsumer,
    pub commands: Producer<FeedCmd>,
}
#[derive(Debug)]
pub struct GatewayInput {
    pub venue: VenueId,
    pub reports: Consumer<RepMsg>,
    pub rep_bulk: ByteConsumer,
    pub pending_report: Option<RepMsg>,
    pub orders: Producer<GwCmd>,
    pub kill: &'static KillWord,
    pub account_reports: Inputs,
}
pub struct Io<J: JournalSink> {
    pub journal: J,
    pub clock: TscClock,
    pub ctl: Inputs,
    pub hk: Consumer<HkMsg>,
    pub sim_reports: Inputs,
    pub sim_orders: Producer<GwCmd>,
    pub feeds: Vec<FeedInput>,
    pub gws: Vec<GatewayInput>,
    pub parsed: MailReceiver<ParsedConfig<Strategies>>,
    pub retired: MailSender<ParsedConfig<Strategies>>,
    pub retired_pending: Option<ParsedConfig<Strategies>>,
    pub clock_status: ClockStatus,
    pub profile: HostProfile,
    pub orders: u64,
    pub cancels: u64,
    pub reports: u64,
}
impl<J: JournalSink> std::fmt::Debug for Io<J> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Io").finish_non_exhaustive()
    }
}
pub fn feed(msg: FeedMsg, now: Tsc, out: &mut InputSlot, bulk: &mut ByteConsumer) -> bool {
    let flags = RecordFlags(0);
    match msg {
        FeedMsg::Bbo(b) => out.set(flags, now, &b),
        FeedMsg::Trade(b) => out.set(flags, now, &b),
        FeedMsg::Funding(b) => out.set(flags, now, &b),
        FeedMsg::MarkPrice(b) => out.set(flags, now, &b),
        FeedMsg::IndexPrice(b) => out.set(flags, now, &b),
        FeedMsg::Liquidation(b) => out.set(flags, now, &b),
        FeedMsg::ConnState(b) => out.set(flags, now, &b),
        FeedMsg::L2Delta(b) => out.set(flags, now, &b),
        FeedMsg::Bulk { kind, len } => {
            let Some(b) = bulk.peek() else { return false };
            if b.len() != len as usize {
                return false;
            }
            out.set_bytes(kind, flags, now, b);
            bulk.advance();
        }
    }
    true
}
pub fn rep_parts<R>(msg: &RepMsg, bulk: &[u8], mut f: impl FnMut(EventKind, &[&[u8]]) -> R) -> R {
    match msg {
        RepMsg::OrderEvent(b) => f(EventKind::ORDER_EVENT, &[b.as_bytes()]),
        RepMsg::SendReport(b) => f(EventKind::SEND_REPORT, &[b.as_bytes()]),
        RepMsg::PositionReport(b) => f(EventKind::POSITION_REPORT, &[b.as_bytes()]),
        RepMsg::BalanceReport(b) => f(EventKind::BALANCE_REPORT, &[b.as_bytes()]),
        RepMsg::FundingSettle(b) => f(EventKind::FUNDING_SETTLE, &[b.as_bytes()]),
        RepMsg::ExternalFill(b) => f(EventKind::EXTERNAL_FILL, &[b.as_bytes()]),
        RepMsg::ConnState(b) => f(EventKind::CONN_STATE, &[b.as_bytes()]),
        RepMsg::AccountConfigReport(b) => f(EventKind::ACCOUNT_CONFIG_REPORT, &[b.as_bytes(), bulk]),
        RepMsg::MarginReport(b) => f(EventKind::MARGIN_REPORT, &[b.as_bytes(), bulk]),
        RepMsg::Note(b) => f(EventKind::NOTE, &[b.as_bytes()]),
    }
}
impl<J: JournalSink> LiveIo<Strategies> for Io<J> {
    type J = J;
    fn journal(&mut self) -> &mut J {
        &mut self.journal
    }
    fn tsc(&mut self) -> Tsc {
        hft_rt::tsc_now()
    }
    fn n_rep(&self) -> u8 {
        self.gws.len() as u8
    }
    fn n_bbo(&self) -> u8 {
        self.feeds.len() as u8
    }
    fn n_md(&self) -> u8 {
        self.feeds.len() as u8
    }
    fn len(&mut self, r: RingId) -> usize {
        match r {
            RingId::Ctl => usize::from(self.retired_pending.is_none() && self.ctl.pending()),
            RingId::Hk => self.hk.len_hint(),
            RingId::RepSim => usize::from(self.sim_reports.pending()),
            RingId::Rep(n) => self.gws.get(n as usize).map_or(0, |g| {
                g.reports.len_hint()
                    + usize::from(g.pending_report.is_some())
                    + usize::from(g.account_reports.pending())
            }),
            RingId::Bbo(n) => self.feeds.get(n as usize).map_or(0, |f| f.bbo.len_hint()),
            RingId::Md(n) => self.feeds.get(n as usize).map_or(0, |f| f.md.len_hint()),
            _ => 0,
        }
    }
    fn peek_len(&mut self, r: RingId) -> Option<usize> {
        if let RingId::Rep(n) = r
            && let Some(g) = self.gws.get_mut(n as usize)
        {
            if g.account_reports.pending() {
                return g.account_reports.peek_len();
            }
            if g.pending_report.is_none() {
                g.pending_report = g.reports.try_pop();
            }
            return g.pending_report.map(|m| match m {
                RepMsg::MarginReport(h) => 64 + usize::from(h.n) * 16,
                RepMsg::AccountConfigReport(h) => 64 + usize::from(h.n_lev) * 16,
                _ => 128,
            });
        }
        if r == RingId::Ctl {
            self.ctl.peek_len()
        } else {
            (self.len(r) > 0).then_some(128)
        }
    }
    fn peek_inst(&mut self, r: RingId) -> Option<InstId> {
        if let RingId::Bbo(n) = r {
            let batch = self.feeds.get_mut(n as usize)?.bbo.read_batch(1);
            match batch.slices().0.first() {
                Some(FeedMsg::Bbo(b)) => Some(b.h.inst),
                _ => None,
            }
        } else {
            None
        }
    }
    fn pop(&mut self, r: RingId, out: &mut InputSlot) -> bool {
        let now = hft_rt::tsc_now();
        match r {
            RingId::Ctl => self.ctl.pop(now, out),
            RingId::RepSim => {
                let ok = self.sim_reports.pop(now, out);
                self.reports += u64::from(ok && out.kind == EventKind::ORDER_EVENT);
                ok
            }
            RingId::Hk => {
                let Some(m) = self.hk.try_pop() else { return false };
                match m {
                    HkMsg::ClockStatus(b) => {
                        self.clock_status = b;
                        out.set(RecordFlags(0), now, &b);
                    }
                    HkMsg::Note(b) => out.set(RecordFlags(0), now, &b),
                    HkMsg::Sys { timer_id } => {
                        let mut b = Timer::new_zeroed();
                        b.timer_id = timer_id;
                        b.owner = 1;
                        b.deadline_tsc = now.0;
                        out.set(RecordFlags(0), now, &b);
                    }
                }
                true
            }
            RingId::Bbo(n) | RingId::Md(n) => {
                let Some(f) = self.feeds.get_mut(n as usize) else {
                    return false;
                };
                let m = if matches!(r, RingId::Bbo(_)) {
                    f.bbo.try_pop()
                } else {
                    f.md.try_pop()
                };
                m.is_some_and(|m| feed(m, now, out, &mut f.bulk))
            }
            RingId::Rep(n) => {
                if let Some(g) = self.gws.get_mut(n as usize)
                    && g.account_reports.pending()
                {
                    return g.account_reports.pop(now, out);
                }
                let Some(g) = self.gws.get_mut(n as usize) else {
                    return false;
                };
                let Some(m) = g.pending_report.take().or_else(|| g.reports.try_pop()) else {
                    return false;
                };
                self.reports += u64::from(matches!(m, RepMsg::OrderEvent(_)));
                let variable = matches!(m, RepMsg::MarginReport(_) | RepMsg::AccountConfigReport(_));
                let body = if variable {
                    let expected = match m {
                        RepMsg::MarginReport(h) => usize::from(h.n) * 16,
                        RepMsg::AccountConfigReport(h) => usize::from(h.n_lev) * 16,
                        _ => 0,
                    };
                    let Some(body) = g.rep_bulk.peek().filter(|b| b.len() == expected) else {
                        g.pending_report = Some(m);
                        return false;
                    };
                    body
                } else {
                    &[]
                };
                rep_parts(&m, body, |kind, pieces| {
                    out.set_pieces(kind, RecordFlags(0), now, pieces)
                });
                if variable {
                    g.rep_bulk.advance();
                }
                true
            }
            _ => false,
        }
    }
    fn push_out(&mut self, ring: OutRing, c: &GwCmd) -> bool {
        let ok = match ring {
            OutRing::Sim => self.sim_orders.try_push(*c).is_ok(),
            OutRing::Venue(v) => self
                .gws
                .iter_mut()
                .find(|g| g.venue == v)
                .is_some_and(|g| g.orders.try_push(*c).is_ok()),
        };
        if ok {
            self.orders += u64::from(c.op == GwOp::Place);
            self.cancels += u64::from(matches!(c.op, GwOp::Cancel | GwOp::CancelAll));
        }
        ok
    }
    fn out_free(&mut self, ring: OutRing) -> usize {
        match ring {
            OutRing::Sim => self.sim_orders.free_hint(),
            OutRing::Venue(v) => self
                .gws
                .iter()
                .find(|g| g.venue == v)
                .map_or(0, |g| g.orders.free_hint()),
        }
    }
    fn push_feed(&mut self, v: VenueId, c: &FeedCmd) -> bool {
        self.feeds
            .iter_mut()
            .find(|f| f.venue == v)
            .is_some_and(|f| f.commands.try_push(*c).is_ok())
    }
    fn take_parsed(&mut self, r: &BlobRef) -> Option<ParsedConfig<Strategies>> {
        self.parsed.try_take().filter(|p| p.sha256() == &r.sha256)
    }
    fn retire(&mut self, p: ParsedConfig<Strategies>) {
        // Gateway serializes configuration replacements through a one-slot mailbox.
        self.retired_pending = self.retired.try_send(p).err();
    }
    fn write_kill(&mut self, bits: &[KillBits]) {
        for (g, b) in self.gws.iter().zip(bits) {
            g.kill.store(b.0, std::sync::atomic::Ordering::Release);
        }
    }
    fn roll_info(&mut self) -> RollInfo {
        RollInfo {
            utc_ns: self.clock.to_utc_ns(hft_rt::tsc_now()),
            calib: self.clock.base,
            tsc_hz_milli: self.clock.hz_milli,
        }
    }
    fn host_profile(&self) -> HostProfile {
        self.profile
    }
    fn clock_status(&self) -> ClockStatus {
        self.clock_status
    }
    fn utc_of(&self, t: Tsc) -> i64 {
        self.clock.to_utc_ns(t)
    }
}
