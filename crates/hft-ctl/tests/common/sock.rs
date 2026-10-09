//! Socket harness (Linux): the real `Gateway` task over a temporary Unix socket, an in-memory ctl
//! ring and a fake sequencer feeding `CmdResult`s back through a queue.
#![allow(
    clippy::type_complexity,
    dead_code,
    unused_imports,
    clippy::disallowed_types,
    clippy::disallowed_methods,
    clippy::disallowed_macros
)]

use std::cell::RefCell;
use std::collections::VecDeque;
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::rc::Rc;
use std::time::Instant;

use hft_ctl::gateway::{Gateway, ResultSource};
use hft_ctl::{GatewayConfig, InternalCmd, ParsedConfig, PrincipalTable, UidKind, VecRing};
use hft_ctl_proto::{EngineInfo, RoleSet};
use hft_event::{CmdResult, ControlCmd, EventKind};
use hft_rt::{Backing, MailReceiver, MailSender, Mailbox, PollTask, Producer, spsc};
use hft_types::Tsc;

use super::{CtlTestSet, Outcome, RingRec, clock, g0_active, p0, principals, refdata_check, risk_decode};

/// Queue-backed `CmdResult` source.
#[derive(Clone, Default)]
pub struct ResQ(pub Rc<RefCell<VecDeque<CmdResult>>>);

impl ResultSource for ResQ {
    fn next_result(&mut self) -> Option<CmdResult> {
        self.0.borrow_mut().pop_front()
    }
}

pub fn tmpdir(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("ctl-sock-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

/// The current process uid.
pub fn my_uid() -> u32 {
    // SAFETY: getuid has no preconditions.
    unsafe { libc::getuid() }
}

/// G0 principals plus the test process' own uid as `kind`.
pub fn principals_with_self(kind: UidKind) -> PrincipalTable {
    let base = principals();
    let mut uids: Vec<(u32, UidKind)> = [1100u32, 1101, 1102, 1200, 1201, 1202]
        .into_iter()
        .filter_map(|u| base.uid(u).map(|k| (u, k.clone())))
        .collect();
    uids.retain(|(u, _)| *u != my_uid());
    uids.push((my_uid(), kind));
    let issuers = [
        "tg:1001",
        "tg:1002",
        "tg:1003",
        "tg:1004",
        "tg:1005",
        "cli:alice",
        "cli:tom",
        "cli:vera",
    ]
    .into_iter()
    .map(|i| (i.to_owned(), base.roles(i).unwrap_or(RoleSet::EMPTY)))
    .collect();
    PrincipalTable::from_parts(uids, issuers)
}

fn leak<T: Send + 'static>() -> &'static Mailbox<T> {
    Box::leak(Box::new(Mailbox::new()))
}

pub struct Sock {
    pub gw: Gateway<CtlTestSet, VecRing, ResQ>,
    pub q: ResQ,
    pub internal: Producer<InternalCmd>,
    pub mb_take: MailReceiver<ParsedConfig<CtlTestSet>>,
    pub retired_send: MailSender<ParsedConfig<CtlTestSet>>,
    pub seen: Vec<RingRec>,
    pub unanswered: Vec<([u8; 16], u64)>,
    pub next_seq: u64,
    pub paused: bool,
    /// Per-command override: (predicate on the command, outcome).
    pub script: Vec<(fn(&ControlCmd<'_>) -> bool, Outcome)>,
    pub rflags_preview: u32,
    pub start: Instant,
    pub socket: PathBuf,
}

impl Sock {
    pub fn new(tag: &str, mut cfg_edit: impl FnMut(&mut GatewayConfig)) -> Sock {
        let dir = tmpdir(tag);
        let socket = dir.join("ctl.sock");
        let mut cfg = GatewayConfig::new(socket.clone(), principals(), 256, 1024, 8);
        cfg_edit(&mut cfg);
        let (p_send, p_take) = leak::<ParsedConfig<CtlTestSet>>().split();
        let (r_send, r_take) = leak::<ParsedConfig<CtlTestSet>>().split();
        let (i_prod, i_cons) = spsc::<InternalCmd>(16, Backing::Heap).unwrap();
        let q = ResQ::default();
        let gw = Gateway::new(
            cfg,
            p0::p0(),
            VecRing::with_free(262_144),
            262_144,
            p_send,
            r_take,
            i_cons,
            refdata_check,
            risk_decode,
            g0_active(),
            q.clone(),
            clock(),
            EngineInfo::default(),
        )
        .unwrap_or_else(|e| panic!("{e:?}"));
        Sock {
            gw,
            q,
            internal: i_prod,
            mb_take: p_take,
            retired_send: r_send,
            seen: Vec::new(),
            unanswered: Vec::new(),
            next_seq: 900,
            paused: false,
            script: Vec::new(),
            rflags_preview: 3,
            start: Instant::now(),
            socket,
        }
    }

    /// Real elapsed time as the TSC (1 GHz test clock).
    pub fn now(&self) -> Tsc {
        Tsc(self.start.elapsed().as_nanos() as u64)
    }

    /// A connected client end whose server end the gateway adopts with `uid`.
    pub fn connect_as(&mut self, uid: u32) -> UnixStream {
        let (a, b) = UnixStream::pair().unwrap();
        self.gw.adopt(b, uid);
        a
    }

    /// One gateway round plus one fake-sequencer round.
    pub fn poll(&mut self) -> bool {
        let now = self.now();
        let did = self.gw.poll_once(now).did_work;
        let recs: Vec<RingRec> = self
            .gw
            .core_mut()
            .ring_mut()
            .out
            .drain(..)
            .map(|(kind, flags, body)| RingRec { kind, flags, body })
            .collect();
        // The sequencer consumed them: the space is free again.
        let used: usize = recs
            .iter()
            .map(|r| hft_ctl::ring::rec_len(&(r.kind, r.flags, r.body.clone())))
            .sum();
        self.gw.core_mut().ring_mut().free += used;
        for r in &recs {
            if r.is_cmd() {
                let (h, _) = r.cmd();
                self.unanswered.push((h.cmd_id, self.next_seq));
                self.next_seq += 1;
            }
        }
        self.seen.extend(recs);
        while self.mb_take.try_take().is_some() {}
        if !self.paused {
            self.answer_all();
        }
        did
    }

    fn outcome_of(&self, id: [u8; 16]) -> Outcome {
        let rec = self
            .seen
            .iter()
            .rev()
            .find(|r| r.is_cmd() && r.cmd().0.cmd_id == id);
        let Some(rec) = rec else { return Outcome::APPLIED };
        let (h, c) = rec.cmd();
        for (pred, o) in &self.script {
            if pred(&c) {
                return *o;
            }
        }
        if h.hflags & 1 != 0 {
            return Outcome {
                rflags: self.rflags_preview,
                ctl_state_version: 33,
                ..Outcome::APPLIED
            };
        }
        Outcome::APPLIED
    }

    pub fn answer_all(&mut self) {
        let pend = std::mem::take(&mut self.unanswered);
        for (id, seq) in pend {
            let o = self.outcome_of(id);
            let mut r = <CmdResult as zerocopy::FromZeros>::new_zeroed();
            r.cmd_id = id;
            r.cmd_seq = seq;
            r.status = o.status;
            r.detail = o.detail;
            r.engine_epoch = 17;
            r.ctl_state_version = o.ctl_state_version;
            r.rflags = o.rflags;
            self.q.0.borrow_mut().push_back(r);
        }
    }

    /// Polls until `done()` or `max` rounds.
    pub fn poll_until(&mut self, max: usize, mut done: impl FnMut(&Sock) -> bool) {
        for _ in 0..max {
            if done(self) {
                return;
            }
            self.poll();
        }
    }

    pub fn cmds(&self) -> Vec<&RingRec> {
        self.seen.iter().filter(|r| r.is_cmd()).collect()
    }
}
