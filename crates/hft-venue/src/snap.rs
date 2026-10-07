//! snap-fetch (README §3b): `SnapReq` and the task shell. In this build the task checks its
//! configuration and owns its rings; executing BBO refresh requests needs the hft-net `RestPool`
//! (M3), so `poll_once` does no work yet and feed tasks do not send `SnapReq`s.

use hft_refdata::RefData;
use hft_rt::{ByteProducer, Consumer, PollTask, Polled, TscUtc};
use hft_types::{InstId, Tsc};
use hft_venue_proto::VenueProtocol;

use crate::config::{InitErr, SnapCfg};
use crate::{Handover, LoopStat};

/// snapreq ring element (16 B, in-process, not journaled).
#[repr(C)]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct SnapReq {
    /// `L2 = 1`, `Bbo = 2`.
    pub kind: u8,
    pub _pad: u8,
    /// Attempt number (the feed task's backoff count).
    pub attempt: u16,
    pub inst: InstId,
    /// Monotonic within the feed task; echoed in the snap record.
    pub req_id: u32,
    pub _pad2: u32,
}
const _: () = assert!(core::mem::size_of::<SnapReq>() == 16);

impl SnapReq {
    pub const L2: u8 = 1;
    pub const BBO: u8 = 2;
}

/// snap-fetch rings (README "运行时入口").
#[derive(Debug)]
pub struct SnapDeps {
    pub rd: &'static RefData,
    /// `snapreq-<venue>`, `snapreq-<venue>-bbo`.
    pub reqs: [Option<Consumer<SnapReq>>; 2],
    /// `snap-<venue>`, `snap-<venue>-bbo`.
    pub out: [Option<ByteProducer>; 2],
    pub handoff: Consumer<Handover>,
    pub retire: hft_rt::Producer<hft_net::RetireMsg>,
    pub build: hft_rt::Producer<hft_net::BuildReq>,
    pub clock: TscUtc,
}

/// The warm task `snap-fetch` of one venue.
pub struct SnapFetchTask<P: VenueProtocol> {
    cfg: SnapCfg,
    deps: SnapDeps,
    ctx: P::FeedCtx,
}

impl<P: VenueProtocol> core::fmt::Debug for SnapFetchTask<P> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("SnapFetchTask").field("cfg", &self.cfg).finish()
    }
}

impl<P: VenueProtocol> SnapFetchTask<P> {
    /// `ctx` = this task's own `FeedCtx` instance (contract 25).
    pub fn new(cfg: SnapCfg, deps: SnapDeps, ctx: P::FeedCtx) -> Result<Self, InitErr> {
        if cfg.parallel == 0 {
            return Err(InitErr::Cfg("snap.parallel"));
        }
        let vm = deps.rd.venue(P::VENUE);
        if vm.ws_max_conn_age_s != 0 && cfg.rotate_margin_s >= vm.ws_max_conn_age_s {
            return Err(InitErr::RotateMargin);
        }
        Ok(SnapFetchTask { cfg, deps, ctx })
    }

    /// No work in this build (see the module docs).
    pub fn poll_once(&mut self, _now_tsc: u64) -> LoopStat {
        let _ = (&self.deps, &self.ctx);
        LoopStat::default()
    }
}

impl<P: VenueProtocol> PollTask for SnapFetchTask<P> {
    fn poll_once(&mut self, now: Tsc) -> Polled {
        Polled {
            did_work: SnapFetchTask::poll_once(self, now.0).did_work,
        }
    }

    fn has_pending(&self) -> bool {
        false
    }
}
