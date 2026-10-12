//! Shared fixtures (TESTS.md "记号"): `CFG0`, the `SEQ0` harness (sequencer on the heap journal
//! backend with a manual clock) and `B(t, bid, ask)`.
#![allow(dead_code)]

use std::cell::Cell;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::atomic::{AtomicUsize, Ordering};

use hft_event::{
    Bbo, ClockStatus, ConnRole, ConnState, ConnStateKind, EventKind, FeedCmd, FeedMsg, GwCmd, GwOp, HkMsg,
    MarketHdr, OrderEvent, OrderEventKind, RecordHeader, RepMsg, SendReport, SendStatus,
};
use hft_journal::{JournalKind, Next};
use hft_rt::{Backing, CalibPair, Consumer, Producer, TscClock, spsc};
use hft_types::{AccountId, AssetId, ConnId, InstId, Px, Qty, SlotId, Tsc, VenueId};
use latency_slice::config::{self, Config};
use latency_slice::journal::{self, Boot, JSide, jcfg, roll_info};
use latency_slice::rd;
use latency_slice::run::{host_profile, seq_cfg};
use latency_slice::seq::{Clock, Seq, SeqIn, SeqOut};

/// RFC 8032 test 1 private key.
pub const ED1_SEED: &str = "9d61b19deffd5a60ba844af492ec2cc44449c5697b326919703bac031cae7f60";

/// `CFG0`.
pub const CFG0: &str = r#"
[slice]
product = "spot"

[[insts]]
symbol = "BTCUSDT"
base = "BTC"
tick = "0.01"
lot = "0.00001"
min_qty = "0.00001"
min_notional = "5"

[net]
src_ip = "10.0.0.5"
bind_device = "eth1"
"#;

/// A second probed inst (ETHUSDT) appended to a configuration text.
pub const ETH: &str = r#"
[[insts]]
symbol = "ETHUSDT"
base = "ETH"
tick = "0.01"
lot = "0.0001"
min_qty = "0.0001"
min_notional = "5"
"#;

static N: AtomicUsize = AtomicUsize::new(0);

/// A fresh temporary root `<tmp>/slc-<pid>-<n>`.
pub fn tmp(tag: &str) -> PathBuf {
    let n = N.fetch_add(1, Ordering::Relaxed);
    let p = std::env::temp_dir().join(format!("slc-{tag}-{}-{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&p);
    std::fs::create_dir_all(&p).unwrap();
    p
}

/// `CFG0` with `extra` TOML appended (later tables override nothing; extra keys must not repeat).
pub fn cfg_with(extra: &str) -> Config {
    config::parse(&format!("{CFG0}\n{extra}")).unwrap()
}

/// Manual counter clock: `now()` returns the current value and then adds `step`.
#[derive(Clone, Debug, Default)]
pub struct MClock {
    pub cur: Rc<Cell<u64>>,
    pub step: Rc<Cell<u64>>,
}

impl Clock for MClock {
    fn now(&mut self) -> u64 {
        let v = self.cur.get();
        self.cur.set(v + self.step.get());
        v
    }
}

pub const MS: u64 = 1_000_000;

/// The `SEQ0` harness.
pub struct H {
    pub seq: Option<Seq<MClock>>,
    pub clock: MClock,
    pub hk: Producer<HkMsg>,
    pub rep: Producer<RepMsg>,
    pub bbo: Producer<FeedMsg>,
    pub gw: Consumer<GwCmd>,
    pub feed: Consumer<FeedCmd>,
    pub side: JSide,
    pub hot: PathBuf,
    pub cold: PathBuf,
    pub cfg: Config,
}

/// One record read back.
#[derive(Clone, Debug)]
pub struct Rec {
    pub hdr: RecordHeader,
    pub body: Vec<u8>,
}

impl Rec {
    pub fn kind(&self) -> EventKind {
        EventKind(self.hdr.kind)
    }
    pub fn end(&self) -> bool {
        self.hdr.flags & hft_event::RecordFlags::EVENT_END.0 != 0
    }
}

impl H {
    /// `SEQ0` from `cfg`; `up` = the gw order connection is `Up`.
    pub fn new(cfg: Config, up: bool) -> H {
        let root = tmp("seq");
        let hot = root.join("hot");
        hft_journal::testing::mem_root(&hot);
        let cold = root.join("cold");
        std::fs::create_dir_all(&cold).unwrap();
        let built = rd::build(&cfg).unwrap();
        let clock = TscClock::from_hz_milli(1_000_000_000_000, CalibPair::default());
        let utc = clock.tsc_utc(&CalibPair::default());
        let scfg = seq_cfg(&cfg, &built, &clock).unwrap();
        let profile = host_profile(clock.hz_milli);
        let boot = Boot::new(profile, built.blob.clone());
        let (mut w, side) = journal::open(
            jcfg("seq", JournalKind::Seq, VenueId(0), &hot, &cold, cfg.segment_mib),
            profile.profile_sha256,
        )
        .unwrap();
        let configs = [boot.blob_ref];
        boot.roll(
            &mut w,
            hft_journal::RollReason::Startup,
            &roll_info(1, 0, clock.hz_milli, &configs),
        )
        .unwrap();
        let (hk_tx, hk_rx) = spsc::<HkMsg>(256, Backing::Heap).unwrap();
        let (rep_tx, rep_rx) = spsc::<RepMsg>(8192, Backing::Heap).unwrap();
        let (bbo_tx, bbo_rx) = spsc::<FeedMsg>(16384, Backing::Heap).unwrap();
        let (gw_tx, gw_rx) = spsc::<GwCmd>(1024, Backing::Heap).unwrap();
        let (fc_tx, fc_rx) = spsc::<FeedCmd>(256, Backing::Heap).unwrap();
        let mc = MClock::default();
        let seq = Seq::new(
            scfg,
            mc.clone(),
            w,
            boot,
            utc,
            SeqIn {
                hk: hk_rx,
                rep: rep_rx,
                bbo: bbo_rx,
            },
            SeqOut {
                gw: gw_tx,
                feed: fc_tx,
            },
        );
        let mut h = H {
            seq: Some(seq),
            clock: mc,
            hk: hk_tx,
            rep: rep_tx,
            bbo: bbo_tx,
            gw: gw_rx,
            feed: fc_rx,
            side,
            hot,
            cold,
            cfg,
        };
        h.s().start();
        if up {
            h.gw_up(1);
        }
        h
    }

    pub fn s(&mut self) -> &mut Seq<MClock> {
        self.seq.as_mut().unwrap()
    }

    /// Sets the clock to `t` ns (step `step` per read).
    pub fn at(&mut self, t: u64, step: u64) {
        self.clock.cur.set(t);
        self.clock.step.set(step);
    }

    /// Polls until no input is left (no timer sweep: the clock does not move).
    pub fn drain(&mut self) {
        for _ in 0..100_000 {
            if !self.s().poll() {
                return;
            }
        }
    }

    /// The gw order connection `Up` (at `t` ns).
    pub fn gw_up(&mut self, t: u64) {
        let c = ConnState {
            venue: VenueId::BINANCE_SPOT,
            conn: 112,
            state: ConnStateKind::Up,
            role: ConnRole::Order,
            inst: InstId(0),
            reason: 0,
            account: AccountId(1),
            _pad2: [0; 3],
            rx_tsc: t,
        };
        self.rep.try_push(RepMsg::ConnState(c)).unwrap();
        self.at(t, 0);
        self.drain();
    }

    /// `B(t_ns, bid, ask)` of `inst`, delivered with the clock at `t_ns` (step `step`).
    #[allow(clippy::too_many_arguments)]
    pub fn bbo_at(&mut self, inst: u32, t: u64, rx: u64, bid: i64, ask: i64, hw: i64, sw: i64, step: u64) {
        let b = Bbo {
            h: MarketHdr {
                venue: VenueId::BINANCE_SPOT,
                conn: ConnId(0),
                _pad: 0,
                inst: InstId(inst),
                ex_ts_ns: 0,
                ex_send_ns: 0,
                hw_rx_ns: hw,
                rx_tsc: rx,
                sw_rx_ns: sw,
                rx_empty_gap_ns: 1_000,
                _rsv: 0,
            },
            bid_px: Px(bid),
            bid_qty: Qty(100_000),
            ask_px: Px(ask),
            ask_qty: Qty(100_000),
            update_id: t,
        };
        self.bbo.try_push(FeedMsg::Bbo(b)).unwrap();
        self.at(t, step);
        self.drain();
    }

    /// `B(t_ms, bid, ask)` of inst 1 (TESTS.md "记号").
    pub fn b(&mut self, t_ms: u64, bid: i64, ask: i64) {
        let t = t_ms * MS;
        self.bbo_at(1, t, t, bid, ask, t as i64 - 5_000, t as i64 - 3_000, 0);
    }

    pub fn cmds(&mut self) -> Vec<GwCmd> {
        let mut v = Vec::new();
        while let Some(c) = self.gw.try_pop() {
            v.push(c);
        }
        v
    }

    pub fn send(&mut self, c: &GwCmd, status: SendStatus, sent: u64) {
        let r = SendReport {
            decision_seq: c.decision_seq,
            sent_tsc: Tsc(sent),
            slot: c.slot,
            r#gen: c.r#gen,
            target_slot: c.target_slot,
            target_gen: if c.op == GwOp::Place { 0 } else { c.r#gen },
            venue: VenueId::BINANCE_SPOT,
            account: c.account,
            conn: ConnId(112),
            status,
            op: c.op,
            _pad: [0; 3],
            os_errno: 0,
            _pad2: 0,
        };
        self.rep.try_push(RepMsg::SendReport(r)).unwrap();
        self.at(sent, 0);
        self.drain();
    }

    /// An `OrderEvent` of `slot`/`gen` received at `rx` (clock at `rx`, step `step`).
    pub fn ev(&mut self, what: OrderEventKind, slot: u32, r#gen: u32, rx: u64, hw: i64, step: u64) {
        let e = order_event(what, slot, r#gen, rx, hw);
        self.rep.try_push(RepMsg::OrderEvent(e)).unwrap();
        self.at(rx, step);
        self.drain();
    }

    /// Places at `t_ms` (B with 65000.00/65000.10), sends it, and acks it; returns `(place, cancel)`.
    pub fn place_ack(&mut self, t_ms: u64) -> (GwCmd, GwCmd) {
        self.b(t_ms, 6_500_000, 6_500_010);
        let p = self.cmds().pop().expect("place");
        self.send(&p, SendStatus::Written, t_ms * MS + 20_000);
        self.ev(
            OrderEventKind::Ack,
            p.slot.0,
            p.r#gen,
            t_ms * MS + 1_500_000,
            (t_ms * MS + 1_500_000 - 4_000) as i64,
            0,
        );
        let c = self.cmds().pop().expect("cancel");
        (p, c)
    }

    /// Full cycle: place, ack, cancel sent, cancel acked.
    pub fn cycle(&mut self, t_ms: u64) {
        let (p, c) = self.place_ack(t_ms);
        self.send(&c, SendStatus::Written, t_ms * MS + 1_510_000);
        self.ev(
            OrderEventKind::CancelAck,
            p.slot.0,
            p.r#gen,
            t_ms * MS + 3_000_000,
            (t_ms * MS + 3_000_000) as i64,
            0,
        );
    }

    pub fn clock_status(&mut self, t: u64) {
        let c = ClockStatus {
            tsc: Tsc(t),
            realtime_ns: t as i64,
            phc_ns: t as i64,
            tsc_hz_milli: 1_000_000_000_000,
            chrony_offset_ns: 0,
            max_error_ns: 1_000,
            rms_ns: 0,
            calib_err_ns: 100,
            source: 1,
            state: 1,
            _pad: 0,
        };
        self.hk.try_push(HkMsg::ClockStatus(c)).unwrap();
        self.at(t, 0);
        self.drain();
    }

    /// Closes the journal (publish + seal).
    pub fn close(&mut self) {
        if let Some(s) = self.seq.take() {
            let mut w = s.into_writer();
            w.publish();
            w.seal_and_close().unwrap();
        }
    }

    /// Every record of the seq journal so far.
    pub fn records(&mut self) -> Vec<Rec> {
        let mut r = journal::reader(&self.hot, &self.cold, "seq").unwrap();
        let mut v = Vec::new();
        loop {
            match r.poll() {
                Next::Record(x) => v.push(Rec {
                    hdr: x.hdr,
                    body: x.body.to_vec(),
                }),
                Next::Event(e) => v.push(Rec {
                    hdr: RecordHeader {
                        len: 0,
                        kind: e.kind.0,
                        flags: e.flags.0,
                        seq: e.seq,
                        tsc: e.tsc,
                    },
                    body: e.body.to_vec(),
                }),
                Next::Pending | Next::End => break,
                Next::Gap(g) => panic!("gap {g:?}"),
                Next::Err(e) => panic!("read {e:?}"),
            }
        }
        v
    }

    /// Records after the boot block of the first segment (kinds other than boot/meta roll records).
    pub fn input_records(&mut self) -> Vec<Rec> {
        self.records()
            .into_iter()
            .filter(|r| {
                !matches!(
                    r.kind(),
                    EventKind::HOST_PROFILE | EventKind::CONFIG_BLOB | EventKind::SEGMENT_ROLL
                )
            })
            .collect()
    }
}

pub fn order_event(what: OrderEventKind, slot: u32, r#gen: u32, rx: u64, hw: i64) -> OrderEvent {
    OrderEvent {
        venue: VenueId::BINANCE_SPOT,
        account: AccountId(1),
        what,
        detail: 0,
        slot: SlotId(slot),
        r#gen,
        inst: InstId(1),
        px: Px(0),
        qty: Qty(0),
        cum_filled: Qty(0),
        fee: Qty(0),
        fee_asset: AssetId(0),
        epoch: 0,
        reject_code: 0,
        exch_order_id: 77,
        ex_ts_ns: 0,
        hw_rx_ns: hw,
        rx_tsc: rx,
        avg_px: Px(0),
        cash_qty: hft_types::Qty(0),
        cash_sum: hft_types::Qty(0),
    }
}
