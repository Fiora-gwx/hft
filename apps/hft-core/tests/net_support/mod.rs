//! Scripted in-process TLS/WS transport shared with the network and venue tests.
#![allow(dead_code)]
#[path = "../../../../crates/hft-venue/tests/support/jrn.rs"]
pub mod jrn;
#[path = "../../../../crates/hft-net/tests/support/mempeer.rs"]
pub mod mempeer;
#[path = "../../../../crates/hft-venue/tests/support/net.rs"]
pub mod net;
#[path = "../../../../crates/hft-net/tests/support/ws_ref.rs"]
pub mod ws_ref;
pub mod rd {
    pub const V7: hft_types::VenueId = hft_types::VenueId::BINANCE_SPOT;
}

use hft_event::{GwCmd, RepMsg};
use hft_net::{BuildReq, RetireMsg};
use hft_rt::{Backing, Consumer, Producer};
use hft_venue::{GwAccountSpec, GwCfg, GwDeps, GwRuntime, Handover};
use hft_venue_proto::{Credential, VenueProtocol};
use std::sync::atomic::AtomicU32;

pub struct Gateway {
    pub runtime: GwRuntime<venue_binance::Spot>,
    pub net: net::Net,
    pub user_conn: Option<hft_net::ConnId>,
    pub builds: usize,
    cmds: Producer<GwCmd>,
    pub reports: Consumer<RepMsg>,
    handoff: Producer<Handover>,
    build: Consumer<BuildReq>,
    retire: Consumer<RetireMsg>,
    _raw: jrn::RawJ,
}

impl Gateway {
    pub fn new(rd: &'static hft_refdata::RefData, clock: hft_rt::TscClock) -> Self {
        let (writer, raw) = jrn::raw_journal("raw-gw", 4);
        let (cmds, rx) = hft_rt::spsc(1024, Backing::Heap).unwrap();
        let (tx, reports) = hft_rt::spsc(1024, Backing::Heap).unwrap();
        let (_, acct) = hft_rt::byte_spsc(1 << 20, Backing::Heap).unwrap();
        let (handoff, hc) = hft_rt::spsc(64, Backing::Heap).unwrap();
        let (rp, retire) = hft_rt::spsc(64, Backing::Heap).unwrap();
        let (bp, build) = hft_rt::spsc(64, Backing::Heap).unwrap();
        let kill = Box::leak(Box::new(hft_rt::CachePadded(AtomicU32::new(0))));
        let cfg = GwCfg {
            stale_guard_ms: 5000,
            ..GwCfg::default()
        };
        let runtime = GwRuntime::new(
            cfg,
            GwDeps {
                rd,
                cmds: rx,
                out: tx,
                acct,
                raw: writer,
                kill,
                kill_mask: hft_types::KillBits::GLOBAL.0
                    | hft_types::KillBits::VENUE.0
                    | hft_types::KillBits::HARD.0,
                tx_tags: None,
                handoff: hc,
                retire: rp,
                build: bp,
                clock: clock.tsc_utc(&clock.base),
            },
            vec![GwAccountSpec {
                account: hft_types::AccountId(1),
                src_ip_idx: 0,
                tpl: venue_binance::Spot::build_templates(
                    &cfg.venue_cfg(),
                    rd,
                    hft_types::AccountId(1),
                    &Credential::default(),
                )
                .unwrap(),
                ctx: venue_binance::GwCtx::default(),
            }],
        )
        .unwrap();
        let mut network = net::net_cfg();
        network.extra_ca = Some(
            concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../../crates/hft-net/tests/support/certs/ca.pem"
            )
            .into(),
        );
        Self {
            runtime,
            net: net::Net::with_config(network),
            user_conn: None,
            builds: 0,
            cmds,
            reports,
            handoff,
            build,
            retire,
            _raw: raw,
        }
    }
    pub fn poll(&mut self, now: u64) {
        self.net.flush(&mut self.runtime, now);
        self.runtime.poll_once(now);
        while let Some(req) = self.build.try_pop() {
            self.builds += 1;
            if req.role == hft_event::ConnRole::UserStream {
                self.user_conn = Some(req.conn);
            }
            let ready = self.net.complete(req);
            self.handoff.try_push(Handover::Ready(ready)).unwrap();
        }
        self.net.reap(&mut self.retire);
    }
    pub fn command(&mut self, c: GwCmd) {
        self.cmds.try_push(c).unwrap();
    }
    pub fn deliver(&mut self, h: &mut crate::common::H) -> Vec<RepMsg> {
        let mut out = Vec::new();
        while let Some(msg) = self.reports.try_pop() {
            hft_core::io::rep_parts(&msg, &[], |kind, parts| {
                h.push_bytes(hft_sequencer::driver::live::RingId::Rep(0), kind, parts.concat());
            });
            out.push(msg);
        }
        h.run();
        out
    }
}
impl Drop for Gateway {
    fn drop(&mut self) {
        self.runtime.retire_all(0);
        self.net.reap(&mut self.retire);
    }
}
