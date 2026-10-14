//! The in-memory live environment of E2E: the ctl ring shared by the gateway (`RigRing`, the
//! `CtlRing` it writes) and the sequencer's `LiveIo` (`RigIo`, which hands every record over as an
//! Input after joining the `PartHdr` segments of Multi records), the `parsed`/`retired` Mailboxes,
//! the other input rings, the out rings and a manual TSC shared by both sides.

use std::cell::{Cell, RefCell};
use std::collections::{HashMap, VecDeque};
use std::rc::Rc;

use hft_ctl::ring::{CtlRing, Rec, rec_len};
use hft_event::{BlobRef, ClockStatus, EventKind, FeedCmd, GwCmd, HostProfile, PartHdr, RecordFlags};
use hft_rt::{CalibPair, MailReceiver, MailSender};
use hft_sequencer::driver::RollInfo;
use hft_sequencer::driver::live::{LiveIo, RingId};
use hft_sequencer::types::{InputSlot, OutRing};
use hft_sequencer::{MemJournal, ParsedConfig, SegClock};
use hft_types::{InstId, KillBits, Tsc, VenueId};
use zerocopy::FromBytes;

use super::world::Set;

/// Capacity of the ctl ring (G0: 262 144 B).
pub const CTL_RING_BYTES: usize = 262_144;

/// The ctl ring between the gateway and the sequencer.
#[derive(Debug, Default)]
pub struct CtlQ {
    pub recs: VecDeque<Rec>,
    /// Encoded bytes in the ring.
    pub used: usize,
    /// Records ever written (for the CTL-154 write TSC).
    pub written: u64,
    /// TSC of the last write.
    pub last_write_tsc: u64,
}

/// The gateway's writer end.
#[derive(Clone, Debug)]
pub struct RigRing {
    pub q: Rc<RefCell<CtlQ>>,
    pub tsc: Rc<Cell<u64>>,
}

impl CtlRing for RigRing {
    fn fits(&mut self, lens: &[usize]) -> bool {
        self.q.borrow().used + lens.iter().sum::<usize>() <= CTL_RING_BYTES
    }
    fn write_all(&mut self, recs: &[Rec]) -> bool {
        let lens: Vec<usize> = recs.iter().map(rec_len).collect();
        if !self.fits(&lens) {
            return false;
        }
        let mut q = self.q.borrow_mut();
        q.used += lens.iter().sum::<usize>();
        q.recs.extend(recs.iter().cloned());
        q.written += recs.len() as u64;
        q.last_write_tsc = self.tsc.get();
        true
    }
}

/// One element of a non-ctl ring.
#[derive(Clone, Debug)]
pub struct Elem {
    pub kind: EventKind,
    pub body: Vec<u8>,
}

fn is_multi(k: EventKind) -> bool {
    k == EventKind::CONTROL_CMD || k == EventKind::CONFIG_BLOB
}

/// The sequencer's live environment.
pub struct RigIo {
    pub j: MemJournal,
    pub tsc: Rc<Cell<u64>>,
    pub ctl: Rc<RefCell<CtlQ>>,
    pub rings: HashMap<RingId, VecDeque<Elem>>,
    pub out: Vec<(OutRing, GwCmd)>,
    pub feed: Vec<(VenueId, FeedCmd)>,
    pub parsed: MailReceiver<ParsedConfig<Set>>,
    /// Objects taken from the Mailbox and not yet claimed by their `BlobRef`.
    pub held: Vec<ParsedConfig<Set>>,
    pub retired: MailSender<ParsedConfig<Set>>,
    /// Retired objects the reverse Mailbox had no room for.
    pub retired_q: VecDeque<ParsedConfig<Set>>,
    pub kills: Vec<Vec<KillBits>>,
    pub calib: CalibPair,
    pub hz: u64,
    /// Ctl records handed over: (TSC of the hand-over, kind).
    pub ctl_taken: Vec<(u64, EventKind)>,
}

impl RigIo {
    pub fn new(
        c: SegClock,
        tsc: Rc<Cell<u64>>,
        ctl: Rc<RefCell<CtlQ>>,
        parsed: MailReceiver<ParsedConfig<Set>>,
        retired: MailSender<ParsedConfig<Set>>,
    ) -> RigIo {
        RigIo {
            j: MemJournal::new(c),
            tsc,
            ctl,
            rings: HashMap::new(),
            out: Vec::new(),
            feed: Vec::new(),
            parsed,
            held: Vec::new(),
            retired,
            retired_q: VecDeque::new(),
            kills: Vec::new(),
            calib: CalibPair {
                tsc: Tsc(c.calib_tsc),
                realtime_ns: c.calib_realtime_ns,
                phc_ns: 0,
                err_ns: 0,
            },
            hz: c.tsc_hz_milli,
            ctl_taken: Vec::new(),
        }
    }

    pub fn ring(&mut self, r: RingId) -> &mut VecDeque<Elem> {
        self.rings.entry(r).or_default()
    }

    /// Logical length of the next ctl record (the `PartHdr.total_len` of a Multi record).
    fn ctl_peek(&self) -> Option<usize> {
        let q = self.ctl.borrow();
        let (k, _, b) = q.recs.front()?;
        if is_multi(*k) {
            PartHdr::read_from_prefix(b)
                .ok()
                .map(|(h, _)| h.total_len as usize)
        } else {
            Some(b.len())
        }
    }

    /// Pops the next logical ctl record (joining the segments of a Multi record).
    fn ctl_pop(&mut self) -> Option<(EventKind, Vec<u8>)> {
        let mut q = self.ctl.borrow_mut();
        let first = q.recs.pop_front()?;
        q.used = q.used.saturating_sub(rec_len(&first));
        let (k, _, b) = first;
        if !is_multi(k) {
            return Some((k, b));
        }
        let Ok((h, rest)) = PartHdr::read_from_prefix(&b) else {
            return Some((k, b));
        };
        let mut body = rest[..(h.chunk_len as usize).min(rest.len())].to_vec();
        let mut part = h.part;
        while part + 1 < h.parts {
            let Some(next) = q.recs.pop_front() else { break };
            q.used = q.used.saturating_sub(rec_len(&next));
            let Ok((nh, nrest)) = PartHdr::read_from_prefix(&next.2) else {
                break;
            };
            body.extend_from_slice(&nrest[..(nh.chunk_len as usize).min(nrest.len())]);
            part = nh.part;
        }
        Some((k, body))
    }

    /// Returns retired objects to the gateway (as many as the reverse Mailbox takes).
    pub fn flush_retired(&mut self) {
        while let Some(p) = self.retired_q.pop_front() {
            if let Err(p) = self.retired.try_send(p) {
                self.retired_q.push_front(p);
                break;
            }
        }
    }
}

impl LiveIo<Set> for RigIo {
    type J = MemJournal;
    fn journal(&mut self) -> &mut MemJournal {
        &mut self.j
    }
    fn tsc(&mut self) -> Tsc {
        Tsc(self.tsc.get())
    }
    fn n_rep(&self) -> u8 {
        1
    }
    fn n_bbo(&self) -> u8 {
        1
    }
    fn n_md(&self) -> u8 {
        1
    }
    fn len(&mut self, r: RingId) -> usize {
        if r == RingId::Ctl {
            let q = self.ctl.borrow();
            return q
                .recs
                .iter()
                .filter(|(k, _, b)| {
                    !is_multi(*k) || PartHdr::read_from_prefix(b).is_ok_and(|(h, _)| h.part == 0)
                })
                .count();
        }
        self.rings.get(&r).map_or(0, VecDeque::len)
    }
    fn peek_len(&mut self, r: RingId) -> Option<usize> {
        if r == RingId::Ctl {
            return self.ctl_peek();
        }
        self.rings.get(&r).and_then(|q| q.front()).map(|e| e.body.len())
    }
    fn peek_inst(&mut self, r: RingId) -> Option<InstId> {
        let e = self.rings.get(&r)?.front()?;
        (e.kind == EventKind::BBO).then(|| {
            hft_sequencer::types::read_body::<hft_event::Bbo>(&e.body).map_or(InstId(0), |b| b.h.inst)
        })
    }
    fn pop(&mut self, r: RingId, out: &mut InputSlot) -> bool {
        let t = Tsc(self.tsc.get());
        if r == RingId::Ctl {
            let Some((k, body)) = self.ctl_pop() else {
                return false;
            };
            self.ctl_taken.push((t.0, k));
            out.set_bytes(k, RecordFlags(0), t, &body);
            return true;
        }
        let Some(e) = self.rings.get_mut(&r).and_then(VecDeque::pop_front) else {
            return false;
        };
        out.set_bytes(e.kind, RecordFlags(0), t, &e.body);
        true
    }
    fn out_free(&mut self, ring: OutRing) -> usize {
        let _ = ring;
        usize::MAX
    }
    fn push_out(&mut self, ring: OutRing, c: &GwCmd) -> bool {
        self.out.push((ring, *c));
        true
    }
    fn push_feed(&mut self, v: VenueId, c: &FeedCmd) -> bool {
        self.feed.push((v, *c));
        true
    }
    fn take_parsed(&mut self, r: &BlobRef) -> Option<ParsedConfig<Set>> {
        while let Some(p) = self.parsed.try_take() {
            self.held.push(p);
        }
        let ix = self.held.iter().position(|p| p.sha256() == &r.sha256)?;
        Some(self.held.remove(ix))
    }
    fn retire(&mut self, p: ParsedConfig<Set>) {
        self.retired_q.push_back(p);
        self.flush_retired();
    }
    fn write_kill(&mut self, bits: &[KillBits]) {
        self.kills.push(bits.to_vec());
    }
    fn roll_info(&mut self) -> RollInfo {
        RollInfo {
            utc_ns: self.utc_of(Tsc(self.tsc.get())),
            calib: self.calib,
            tsc_hz_milli: self.hz,
        }
    }
    fn host_profile(&self) -> HostProfile {
        hft_sequencer::snapshot::host_profile(self.hz)
    }
    fn clock_status(&self) -> ClockStatus {
        hft_sequencer::snapshot::clock_status(self.calib, self.hz)
    }
    fn utc_of(&self, t: Tsc) -> i64 {
        let d = i128::from(t.0) - i128::from(self.calib.tsc.0);
        self.calib.realtime_ns + (d * 1_000_000_000_000 / i128::from(self.hz.max(1))) as i64
    }
}
