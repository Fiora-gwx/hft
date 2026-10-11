//! `DstIo`: the live environment of the sequencer in a DST run. The engine runs on the live driver
//! (`LiveDriver<S, DstIo<S>>`: ring order, per-round limits, timers, keyframes are the system
//! under test too); `DstIo` is its `LiveIo` — rings hold the Input the world hands over, the TSC
//! is the world's virtual time ([`DstDriver::stamp`]), out-ring frames go to the driver's
//! collection buffer, the journal is the sequencer's in-memory `MemJournal`. No random source.

use std::collections::{BTreeMap, VecDeque};

use hft_event::{BlobRef, ClockStatus, EventKind, FeedCmd, GwCmd, GwOp, HostProfile, RecordFlags};
use hft_rt::CalibPair;
use hft_sequencer::driver::RollInfo;
use hft_sequencer::driver::live::{LiveIo, RingId};
use hft_sequencer::driver::replay::Log;
use hft_sequencer::types::{InputSlot, OutRing};
use hft_sequencer::{MemJournal, ParsedConfig, SegClock};
use hft_strategy_sdk::StrategySet;
use hft_types::{InstId, KillBits, Tsc, VenueId};

use crate::dst::driver::DstDriver;
use crate::dst::sut::OutRing as DstRing;

/// One queued ring element.
#[derive(Clone, Debug)]
pub struct Elem {
    pub kind: EventKind,
    pub body: Vec<u8>,
}

/// The live environment of one engine instance.
pub struct DstIo<S: StrategySet> {
    pub audit: super::audit::Audit,
    pub events: Vec<crate::dst::sut::EventStep>,
    pending_event: Option<super::events::Pending>,
    world: crate::dst::world::WorldSpec,
    carry: super::observe::Carry,
    pub drv: DstDriver,
    pub j: MemJournal,
    pub rings: BTreeMap<RingKey, VecDeque<Elem>>,
    /// Venues in ring order (`Rep(i)` / `Bbo(i)` = venue `i`).
    pub venues: Vec<VenueId>,
    pub mailbox: Vec<ParsedConfig<S>>,
    pub kills: Vec<KillBits>,
    pub calib: CalibPair,
    pub hz_milli: u64,
    pub utc0: i64,
    pub old: Option<Log>,
    /// Seq of the latest Input record (frame trigger).
    pub last_input: u64,
    /// The gw's epoch per (out ring, account) from `SetEpoch` frames (`epoch0` before any): the
    /// gw encodes it in the client order id of each `Place`, so the reports carry it; here it
    /// rides in `GwCmd.arg` to the matcher.
    pub gw_epoch: BTreeMap<(u8, u8), u16>,
    pub epoch0: u16,
    /// The gw's slot mirror: epoch of each (out ring, account, slot) from its `Place` or
    /// `MirrorRestore` (a `Query` asks with the slot's own client id).
    pub slot_epoch: BTreeMap<(u8, u8, u32), u16>,
}

/// `RingId` as an ordered key.
#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Debug)]
pub struct RingKey(u8, u8);

impl RingKey {
    pub fn of(r: RingId) -> RingKey {
        match r {
            RingId::Ctl => RingKey(0, 0),
            RingId::Hk => RingKey(1, 0),
            RingId::Rep(i) => RingKey(2, i),
            RingId::Chain => RingKey(3, 0),
            RingId::Bbo(i) => RingKey(4, i),
            RingId::Advice => RingKey(5, 0),
            RingId::Md(i) => RingKey(6, i),
            RingId::RepSim => RingKey(7, 0),
        }
    }
}

impl<S: StrategySet> core::fmt::Debug for DstIo<S> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("DstIo")
            .field("now_ns", &self.drv.now_ns())
            .finish_non_exhaustive()
    }
}

impl<S: StrategySet> DstIo<S> {
    pub fn new(
        drv: DstDriver,
        venues: Vec<VenueId>,
        clock: SegClock,
        utc0: i64,
        world: crate::dst::world::WorldSpec,
        carry: super::observe::Carry,
    ) -> DstIo<S> {
        DstIo {
            audit: super::audit::Audit::default(),
            events: Vec::with_capacity(super::events::MAX_EVENTS),
            pending_event: None,
            world,
            carry,
            j: MemJournal::new(clock),
            calib: CalibPair {
                tsc: Tsc(clock.calib_tsc),
                realtime_ns: clock.calib_realtime_ns,
                phc_ns: 0,
                err_ns: 0,
            },
            hz_milli: clock.tsc_hz_milli,
            drv,
            rings: BTreeMap::new(),
            venues,
            mailbox: Vec::new(),
            kills: Vec::new(),
            utc0,
            old: None,
            last_input: 0,
            gw_epoch: BTreeMap::new(),
            epoch0: 1,
            slot_epoch: BTreeMap::new(),
        }
    }
    /// Ring index of a venue.
    pub fn venue_ix(&self, v: VenueId) -> u8 {
        self.venues.iter().position(|x| *x == v).map_or(0, |i| i as u8)
    }
    pub fn push(&mut self, r: RingId, kind: EventKind, body: Vec<u8>) {
        self.rings
            .entry(RingKey::of(r))
            .or_default()
            .push_back(Elem { kind, body });
    }
    pub fn queued(&self) -> usize {
        self.rings.values().map(VecDeque::len).sum()
    }
}

impl<S: StrategySet> LiveIo<S> for DstIo<S> {
    fn observe_input(&mut self, e: &hft_sequencer::Engine, input: &InputSlot, before: bool) {
        if before {
            self.audit.before(e, input);
            self.pending_event = Some(super::events::Pending::begin(
                e,
                &self.world,
                &self.audit.history,
                &self.carry,
                input,
            ));
        } else {
            self.audit.after(e);
            if let Some(p) = self.pending_event.take() {
                if self.events.len() == super::events::MAX_EVENTS {
                    self.audit.history.error = Some("per-step Input observation buffer overflow".into());
                } else {
                    self.events
                        .push(p.finish(e, &self.world, &self.audit.history, &self.carry, &self.j));
                }
            } else {
                self.audit.history.error = Some("missing pre-Input observation".into());
            }
        }
    }
    type J = MemJournal;
    fn journal(&mut self) -> &mut MemJournal {
        &mut self.j
    }
    fn tsc(&mut self) -> Tsc {
        Tsc(self.drv.stamp())
    }
    fn n_rep(&self) -> u8 {
        self.venues.len().max(1) as u8
    }
    fn n_bbo(&self) -> u8 {
        self.venues.len().max(1) as u8
    }
    fn n_md(&self) -> u8 {
        self.venues.len().max(1) as u8
    }
    fn len(&mut self, r: RingId) -> usize {
        self.rings.get(&RingKey::of(r)).map_or(0, VecDeque::len)
    }
    fn peek_len(&mut self, r: RingId) -> Option<usize> {
        self.rings
            .get(&RingKey::of(r))
            .and_then(|q| q.front())
            .map(|e| e.body.len())
    }
    fn peek_inst(&mut self, r: RingId) -> Option<InstId> {
        let e = self.rings.get(&RingKey::of(r))?.front()?;
        (e.kind == EventKind::BBO).then(|| {
            hft_sequencer::types::read_body::<hft_event::Bbo>(&e.body).map_or(InstId(0), |b| b.h.inst)
        })
    }
    fn pop(&mut self, r: RingId, out: &mut InputSlot) -> bool {
        let Some(e) = self.rings.get_mut(&RingKey::of(r)).and_then(VecDeque::pop_front) else {
            return false;
        };
        out.set_bytes(e.kind, RecordFlags(0), Tsc(self.drv.stamp()), &e.body);
        true
    }
    fn push_out(&mut self, ring: OutRing, c: &GwCmd) -> bool {
        let r = match ring {
            OutRing::Venue(v) => DstRing::Venue(v),
            OutRing::Sim => DstRing::Sim,
        };
        let trig = self.last_input;
        let rk = match ring {
            OutRing::Venue(v) => v.0,
            OutRing::Sim => 0xFF,
        };
        let mut c = *c;
        match c.op {
            GwOp::SetEpoch => {
                for a in 0..=255u8 {
                    if c.account.0 == 0 || c.account.0 == a {
                        self.gw_epoch.insert((rk, a), c.epoch());
                    }
                }
            }
            GwOp::Place => {
                let ep = self
                    .gw_epoch
                    .get(&(rk, c.account.0))
                    .copied()
                    .unwrap_or(self.epoch0);
                c.arg = u32::from(ep);
                self.slot_epoch.insert((rk, c.account.0, c.slot.0), ep);
            }
            GwOp::MirrorRestore => {
                self.slot_epoch.insert((rk, c.account.0, c.slot.0), c.epoch());
            }
            GwOp::Query => {
                let gw = self
                    .gw_epoch
                    .get(&(rk, c.account.0))
                    .copied()
                    .unwrap_or(self.epoch0);
                c.arg = u32::from(
                    self.slot_epoch
                        .get(&(rk, c.account.0, c.slot.0))
                        .copied()
                        .unwrap_or(gw),
                );
            }
            _ => {}
        }
        let c = &c;
        self.drv.emit(r, *c, trig);
        if let Some(p) = &mut self.pending_event {
            p.frame(crate::dst::sut::FrameRec {
                ring: r,
                cmd: *c,
                trigger_seq: trig,
            });
        }
        true
    }
    fn push_feed(&mut self, _v: VenueId, _c: &FeedCmd) -> bool {
        true
    }
    fn take_parsed(&mut self, r: &BlobRef) -> Option<ParsedConfig<S>> {
        let ix = self.mailbox.iter().position(|p| p.sha256() == &r.sha256)?;
        Some(self.mailbox.remove(ix))
    }
    fn retire(&mut self, _p: ParsedConfig<S>) {}
    fn write_kill(&mut self, bits: &[KillBits]) {
        self.kills = bits.to_vec();
    }
    fn roll_info(&mut self) -> RollInfo {
        let t = Tsc(self.drv.stamp());
        RollInfo {
            utc_ns: self.utc_of(t),
            calib: self.calib,
            tsc_hz_milli: self.hz_milli,
        }
    }
    fn host_profile(&self) -> HostProfile {
        hft_sequencer::snapshot::host_profile(self.hz_milli)
    }
    fn clock_status(&self) -> ClockStatus {
        hft_sequencer::snapshot::clock_status(self.calib, self.hz_milli)
    }
    fn utc_of(&self, t: Tsc) -> i64 {
        let d = i128::from(t.0) - i128::from(self.calib.tsc.0);
        self.calib.realtime_ns + (d * 1_000_000_000_000 / i128::from(self.hz_milli.max(1))) as i64
    }
    fn after_take(&mut self, _r: Option<RingId>) {
        self.last_input = self.j.next_seq;
    }
    fn measure(&mut self) -> Tsc {
        Tsc(self.drv.stamp())
    }
    fn recovery_log(&mut self) -> Option<Log> {
        self.old.take()
    }
}
