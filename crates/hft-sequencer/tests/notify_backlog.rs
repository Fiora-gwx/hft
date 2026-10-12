//! Notification continuation with real cells and recorded control inputs (SEQ-220–SEQ-223).

use std::cell::RefCell;
use std::collections::{BTreeMap, VecDeque};
use std::rc::Rc;

use hft_event::{CmdHdr, ConnRole, ConnStateKind, ControlCmd, EventKind, Timer};
use hft_journal::{RollAt, RollReason};
use hft_risk::mode::Effective;
use hft_sequencer::codes::{detail, status};
use hft_sequencer::driver::live::{LiveDriver, RingId};
use hft_sequencer::{BootSource, Sequencer, StepOutcome};
use hft_strategy_sdk::EventRef;
use hft_types::{AccountId, CapSource, GroupId, HostClass, InstId, KillLevel, Qty, RunMode, StrategyId, Tsc};
use proptest::prelude::*;
use zerocopy::IntoBytes;

use crate::common::*;

// The Sim instance deliberately has the lowest ID. Generation/delivery must still put Live first.
const ORDER: [StrategyId; 4] = [StrategyId(2), StrategyId(3), StrategyId(4), StrategyId(1)];

fn all_scope() -> hft_event::CmdScope {
    scope(StrategyId(0), AccountId(0), GroupId(0))
}

fn kill() -> ControlCmd<'static> {
    ControlCmd::Kill {
        scope: all_scope(),
        level: KillLevel::Global,
        flatten: false,
        cancel_resting: false,
        hard: false,
    }
}

fn balance(h: &mut H, account: AccountId) {
    h.push(
        RingId::Rep(0),
        &hft_event::BalanceReport {
            venue: A,
            account,
            asset: USDT,
            _pad: 0,
            free: q("100000"),
            locked: Qty(0),
            ex_ts_ns: 0,
            rx_tsc: Tsc(h.now()),
        },
    );
}

/// Four bound accounts, up to 64 disjoint spot groups each, all made Live by normal Inputs.
/// No queue, mode, cell or gate is fabricated. ECF1/RKP1/SPB1 are self-contained for replay.
fn ready(counts: [u16; 4], period_ms: u32) -> H {
    reset_script();
    let mut b = b0();
    b.caps.max_cells_total = 256;
    b.caps.max_groups_per_instance = 64;
    b.caps.max_timers = 4_096;
    b.seq.keyframe_period_ms = period_ms;
    b.rd.insts = (1..=64)
        .map(|i| {
            inst_spec(
                InstId(i),
                A,
                &format!("N{i}USDT"),
                false,
                FILL,
                ("0.01", 1),
                ("0.001", 100_000),
            )
        })
        .collect();
    let mut sim = b.rd.accounts[8].clone();
    sim.id = AccountId(4);
    b.rd.accounts.truncate(3);
    b.rd.accounts.push(sim);
    for (a, s) in b.rd.accounts.iter_mut().zip(ORDER) {
        a.bound_instance = s;
        a.venues = vbits(&[A]);
    }
    b.rd.instances.truncate(4);
    for s in &mut b.rd.instances {
        s.kind_id = TAKER;
    }
    b.params.clear();
    let rd = b.rd.load();
    let mut risk = b0_risk(&rd);
    risk.accounts = hft_risk::PreVec::from_vec((1..=4).map(acct_limits).collect());
    b.risk = Some(risk);
    let mut setup = b.setup(&rd);
    setup.risk_decode = Some(hft_sequencer::config_blob::decode_risk_blob);
    setup.keys_required = (1..=3).map(|a| (AccountId(a), A, 3)).collect();
    setup.exec.accounts.retain(|a| a.account.0 <= 4);
    for a in &mut setup.exec.accounts {
        a.sim = a.account == AccountId(4);
    }
    let mut configs = b.configs(&rd);
    configs.engine_blob = setup.encode_blob(1, HostClass::Dev2, UTC0);
    configs.risk_blob = configs.risk.encode_blob().unwrap();
    configs.risk_sha256 = sha(&configs.risk_blob);
    for (ix, (s, n)) in ORDER.into_iter().zip(counts).enumerate() {
        let groups = (0..n)
            .map(|i| {
                group(
                    GroupId::compose(s, u32::from(i) + 1).unwrap(),
                    FILL,
                    &[leg(InstId(u32::from(i) + 1), A)],
                )
            })
            .collect();
        let spb = Spb::new(s, AccountId(ix as u8 + 1), TAKER, 1, groups).bytes();
        let params = hft_sequencer::parsed::parse_params::<TestSet>(&spb, sha(&spb), &rd, 64, None).unwrap();
        configs.params.push((spb, params));
    }
    let heartbeat = hb();
    let s = Sequencer::boot(
        BootSource::Empty { configs },
        &setup,
        LiveDriver::new(TestIo::new(setup.clock), b.live),
        heartbeat,
    )
    .unwrap();
    let mut h = H {
        s,
        rd,
        setup,
        cmd_no: 0,
        hb: heartbeat,
        upd: 1,
    };
    h.step();
    let t = h.now();
    h.push(
        RingId::Rep(0),
        &conn(A, 0, ConnStateKind::Up, ConnRole::Md, InstId(0), AccountId(0), t),
    );
    for i in 1..=64 {
        h.push(
            RingId::Rep(0),
            &conn(A, 0, ConnStateKind::Up, ConnRole::Md, InstId(i), AccountId(0), t),
        );
    }
    for a in 1..=4 {
        let ring = if a == 4 { RingId::RepSim } else { RingId::Rep(0) };
        for (slot, role) in [
            (0, ConnRole::Order),
            (1, ConnRole::Order),
            (0, ConnRole::UserStream),
        ] {
            h.push(
                ring,
                &conn(A, slot, ConnStateKind::Up, role, InstId(0), AccountId(a), t),
            );
        }
    }
    h.run();
    h.push(RingId::Hk, &sys_timer(8, t));
    for a in 1..=3 {
        h.push_bytes(
            RingId::Rep(0),
            EventKind::ACCOUNT_CONFIG_REPORT,
            cfg_hdr(AccountId(a), A, 0).as_bytes().to_vec(),
        );
        h.push(
            RingId::Rep(0),
            &recon_summary(AccountId(a), A, 1_000 + u64::from(a), 0),
        );
        h.push(RingId::Hk, &sys_timer(16 | (u32::from(a) << 16), t));
        balance(&mut h, AccountId(a));
    }
    for i in 1..=64 {
        h.quote(InstId(i), "10", "10.01");
    }
    h.run();
    for a in 1..=4 {
        assert!(
            h.s.engine().risk.mode.released(AccountId(a)),
            "account {a}: gate {}",
            h.gate_bits(AccountId(a))
        );
    }
    let id = h.cmd_confirmed(
        AccountId(0),
        &ControlCmd::SetMode {
            scope: all_scope(),
            mode: RunMode::Live,
        },
    );
    h.run();
    assert_eq!(h.res(id), (status::APPLIED, 0));
    let cells: Vec<_> = h.s.instances().cells().collect();
    assert_eq!(cells.len(), counts.into_iter().map(usize::from).sum::<usize>());
    for c in cells {
        assert_eq!(h.s.engine().risk.mode.effective(c), Effective::Live);
    }
    assert!(!h.s.view().wants_continue());
    assert_eq!(h.s.engine().notify.backlog_cap(), 1_536);
    clear_log();
    h
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Change {
    strategy: StrategyId,
    gix: u16,
    old: RunMode,
    new: RunMode,
}

type Seen = Rc<RefCell<Vec<(u64, Change)>>>;

fn observe() -> Seen {
    let seen = Rc::new(RefCell::new(Vec::new()));
    let out = seen.clone();
    set_hook(move |hx, ev, _| {
        if let EventRef::Mode { gix, old, new } = ev {
            out.borrow_mut().push((
                hx.v.now().seq,
                Change {
                    strategy: hx.s.strategy,
                    gix: *gix,
                    old: *old,
                    new: *new,
                },
            ));
        }
    });
    seen
}

fn expected(counts: [u16; 4], old: RunMode, new: RunMode) -> Vec<Change> {
    ORDER
        .into_iter()
        .zip(counts)
        .flat_map(|(strategy, n)| {
            (0..n).map(move |gix| Change {
                strategy,
                gix,
                old,
                new,
            })
        })
        .collect()
}

fn continue_seqs(h: &H, mark: usize) -> Vec<u64> {
    h.since(mark)
        .iter()
        .filter_map(|r| {
            (r.kind == EventKind::TIMER)
                .then(|| r.read::<Timer>())
                .flatten()
                .filter(|t| t.owner == 1 && t.timer_id == 17)
                .map(|_| r.seq)
        })
        .collect()
}

fn bound(h: &H) {
    let n = &h.s.engine().notify;
    assert!(n.backlog_len() <= 1_536);
    assert!(n.backlog_peak <= 1_536, "peak {}", n.backlog_peak);
    assert_eq!(h.s.view().notify_headroom(), 1_536 - n.backlog_len());
}

fn check_replay(h: &mut H, start: u64) {
    clear_hook();
    h.keyframe();
    assert!(h.verify().is_empty());
    assert!(h.verify_from(Some(start)).is_empty());
}

#[test]
fn seq_220_mass_mode_change() {
    let mut h = ready([64; 4], 300_000);
    let start = h.keyframe().seq;
    let seen = observe();
    let mark = h.mark();
    let id = h.cmd_confirmed(AccountId(0), &kill());
    assert_eq!(h.step(), StepOutcome::Busy);
    assert_eq!(h.res(id), (status::APPLIED, 0));
    assert_eq!(seen.borrow().len(), 64);
    assert_eq!(h.s.engine().notify.backlog_len(), 192);
    for left in [128, 64, 0] {
        let before = seen.borrow().len();
        assert_eq!(h.step(), StepOutcome::Busy);
        assert_eq!(seen.borrow().len() - before, 64);
        assert_eq!(h.s.engine().notify.backlog_len(), left);
        bound(&h);
    }
    assert!(!h.s.view().wants_continue());
    let seqs = continue_seqs(&h, mark);
    assert_eq!(seqs.len(), 3);
    let rows = seen.borrow();
    assert_eq!(
        rows.iter().map(|x| x.1).collect::<Vec<_>>(),
        expected([64; 4], RunMode::Live, RunMode::CloseOnly)
    );
    for (chunk, seq) in rows[64..].chunks(64).zip(seqs) {
        assert!(chunk.iter().all(|x| x.0 == seq));
    }
    drop(rows);
    check_replay(&mut h, start);
}

#[test]
fn seq_221_notify_per_continue() {
    for n in [63, 64, 65] {
        let counts = [64, n - 2, 1, 1];
        let mut h = ready(counts, 300_000);
        let start = h.keyframe().seq;
        let seen = observe();
        h.cmd_confirmed(AccountId(0), &kill());
        h.step();
        assert_eq!(h.s.engine().notify.backlog_len(), usize::from(n));
        assert_eq!(seen.borrow().len(), 64);
        let mark = h.mark();
        let mut batches = Vec::new();
        for _ in 0..2 {
            if !h.s.view().wants_continue() {
                break;
            }
            let before = seen.borrow().len();
            h.step();
            batches.push(seen.borrow().len() - before);
            bound(&h);
        }
        assert_eq!(
            batches,
            if n == 65 {
                vec![64, 1]
            } else {
                vec![usize::from(n)]
            }
        );
        assert_eq!(continue_seqs(&h, mark).len(), batches.len());
        assert!(!h.s.view().wants_continue());
        assert_eq!(
            seen.borrow().iter().map(|x| x.1).collect::<Vec<_>>(),
            expected(counts, RunMode::Live, RunMode::CloseOnly)
        );
        check_replay(&mut h, start);
    }
}

#[test]
fn seq_222_continue_blocks_snapshot_roll_drain() {
    for drain in [false, true] {
        let mut h = ready([64; 4], 1_000);
        let start = h.keyframe().seq;
        let seen = observe();
        // Due at this timestamp, but the control ring prevents an idle keyframe/roll first.
        h.adv(1_000 * MS);
        h.io().j.faults.needs_roll = Some((RollReason::Size, RollAt::IdleRound));
        h.cmd_confirmed(AccountId(0), &kill());
        let mark = h.mark();
        assert_eq!(h.step(), StepOutcome::Busy);
        assert_eq!(h.s.engine().notify.backlog_len(), 192);
        if drain {
            h.cmd_confirmed(
                AccountId(0),
                &ControlCmd::Drain {
                    cancel_resting: false,
                },
            );
        }
        let mut last = StepOutcome::Busy;
        for _ in 0..3 {
            assert!(h.s.view().wants_continue());
            last = h.step();
            bound(&h);
            if h.s.view().wants_continue() {
                assert!(!matches!(last, StepOutcome::Done { .. }));
                assert!(
                    h.since(mark)
                        .iter()
                        .all(|r| r.kind != EventKind::STATE_SNAPSHOT && r.kind != EventKind::SEGMENT_ROLL)
                );
            }
        }
        assert!(!h.s.view().wants_continue());
        assert_eq!(seen.borrow().len(), 256);
        let final_delivery = seen.borrow().last().unwrap().0;
        if drain {
            assert!(matches!(last, StepOutcome::Done { .. }));
            let snap = h.snapshots().pop().unwrap();
            assert!(snap.seq > final_delivery);
            assert_eq!(
                snap.read::<hft_event::SnapshotHdr>().unwrap().purpose,
                hft_sequencer::snapshot::PURPOSE_DRAIN
            );
            assert_eq!(h.j().committed, h.j().len());
            clear_hook();
            assert!(h.verify().is_empty());
            assert!(h.verify_from(Some(start)).is_empty());
        } else {
            assert!(
                h.since(mark)
                    .iter()
                    .all(|r| r.kind != EventKind::STATE_SNAPSHOT && r.kind != EventKind::SEGMENT_ROLL)
            );
            for _ in 0..8 {
                h.step();
                if h.since(mark).iter().any(|r| r.kind == EventKind::SEGMENT_ROLL) {
                    h.io().j.faults.needs_roll = None;
                    break;
                }
            }
            let records = h.since(mark);
            let sys: Vec<_> = records
                .iter()
                .filter_map(|r| {
                    (r.kind == EventKind::TIMER)
                        .then(|| r.read::<Timer>())
                        .flatten()
                        .filter(|t| t.owner == 1 && [1, 2].contains(&t.timer_id))
                        .map(|t| (r.seq, t.timer_id))
                })
                .collect();
            assert!(sys.iter().any(|x| x.1 == 1), "{sys:?}");
            assert!(sys.iter().any(|x| x.1 == 2), "{sys:?}");
            assert!(sys.iter().all(|x| x.0 > final_delivery));
            check_replay(&mut h, start);
        }
    }
}

/// Unlimited per-instance reference queues, driven only by the submitted command and its
/// recorded Input boundary. No engine mode/queue output is used to generate expected changes.
struct Reference {
    desired: RunMode,
    killed: bool,
    commands: BTreeMap<[u8; 16], u8>,
    queues: [VecDeque<Change>; 4],
    produced: usize,
    delivered: usize,
}

impl Reference {
    fn new() -> Self {
        Self {
            desired: RunMode::Live,
            killed: false,
            commands: BTreeMap::new(),
            queues: std::array::from_fn(|_| VecDeque::new()),
            produced: 0,
            delivered: 0,
        }
    }
    fn mode(&self) -> RunMode {
        if self.killed && self.desired != RunMode::Shadow {
            RunMode::CloseOnly
        } else {
            self.desired
        }
    }
    fn command(&mut self, h: &mut H, action: u8) {
        let c = match action {
            0 => kill(),
            1 => ControlCmd::Resume {
                scope: all_scope(),
                source: CapSource::KILL,
            },
            2..=4 => ControlCmd::SetMode {
                scope: all_scope(),
                mode: [RunMode::Live, RunMode::CloseOnly, RunMode::Shadow][usize::from(action - 2)],
            },
            _ => unreachable!(),
        };
        let id = h.cmd_confirmed(AccountId(0), &c);
        assert!(self.commands.insert(id, action).is_none());
    }
    fn step(&mut self, h: &mut H, seen: &Seen) {
        let mark = h.mark();
        assert!(matches!(h.step(), StepOutcome::Busy | StepOutcome::Idle));
        bound(h);
        for rec in h.since(mark).iter().filter(|r| r.kind == EventKind::CONTROL_CMD) {
            let hdr = rec.read::<CmdHdr>().unwrap();
            let action = self.commands.remove(&hdr.cmd_id).expect("submitted command");
            let old = self.mode();
            let absent = action == 1 && !self.killed;
            match action {
                0 => self.killed = true,
                1 => self.killed = false,
                2..=4 => {
                    self.desired =
                        [RunMode::Live, RunMode::CloseOnly, RunMode::Shadow][usize::from(action - 2)]
                }
                _ => unreachable!(),
            }
            let result = h.res(hdr.cmd_id);
            if absent {
                assert_eq!(
                    result,
                    (status::PRECONDITION_FAILED, u32::from(detail::NO_MATCHING_CAP))
                );
            } else {
                assert!([status::APPLIED, status::NO_OP].contains(&result.0), "{result:?}");
            }
            let new = self.mode();
            if old != new {
                for x in expected([64; 4], old, new) {
                    self.queues[usize::from(x.strategy.0 - 1)].push_back(x);
                    self.produced += 1;
                }
            }
        }
        let deliveries = std::mem::take(&mut *seen.borrow_mut());
        let mut per_event = BTreeMap::<u64, usize>::new();
        for (seq, x) in deliveries {
            assert_eq!(
                self.queues[usize::from(x.strategy.0 - 1)].pop_front(),
                Some(x),
                "notification at {seq}"
            );
            self.delivered += 1;
            *per_event.entry(seq).or_default() += 1;
        }
        for seq in continue_seqs(h, mark) {
            assert!(per_event.get(&seq).copied().unwrap_or(0) <= 64);
        }
        let pending: usize = self.queues.iter().map(VecDeque::len).sum();
        assert_eq!(pending, h.s.engine().notify.backlog_len());
        assert_eq!(self.produced - self.delivered, pending);
        clear_log();
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(ProptestConfig::default().cases.max(300)))]
    #[test]
    fn seq_223_backlog_never_overflows(actions in proptest::collection::vec(0u8..7, 1..=500)) {
        let mut h = ready([64; 4], 300_000);
        let seen = observe();
        let mut reference = Reference::new();
        // Every case reaches backpressure; the random suffix exercises no-ops and mixed input.
        for action in [0, 1, 0, 1, 0, 1].into_iter().chain(actions) {
            // A confirmed command is queued only after its predecessor has been processed.
            // Notifications may still be pending; their state version cannot stale this command.
            for _ in 0..32 {
                if reference.commands.is_empty() { break; }
                reference.step(&mut h, &seen);
            }
            assert!(reference.commands.is_empty());
            match action {
                0..=4 => reference.command(&mut h, action),
                5 => balance(&mut h, AccountId(1)),
                _ => h.quote(InstId(1), "10", "10.02"),
            }
            reference.step(&mut h, &seen);
        }
        for _ in 0..64 {
            if reference.commands.is_empty() && !h.s.view().wants_continue() { break; }
            reference.step(&mut h, &seen);
        }
        assert!(reference.commands.is_empty());
        assert!(!h.s.view().wants_continue());
        assert_eq!(reference.produced, reference.delivered);
        assert!(reference.queues.iter().all(VecDeque::is_empty));
        assert!(h.s.engine().notify.backlog_peak > 768, "backpressure was never reached");
        clear_hook();
    }
}
