//! TaskLoop rounds and idle decisions (RT-110 – RT-115, RT-121).
#![cfg(not(loom))]

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};

use hft_rt::{CalibPair, Heartbeat, IdleAction, IdlePolicy, PollTask, Polled, TaskLoop, Tsc, TscClock};

fn c1g() -> TscClock {
    TscClock::from_hz_milli(1_000_000_000_000, CalibPair::default())
}

type Log = Rc<RefCell<Vec<(char, u64)>>>;

struct Fake {
    name: char,
    work: bool,
    log: Log,
    deadline: Option<Tsc>,
}

impl PollTask for Fake {
    fn poll_once(&mut self, now: Tsc) -> Polled {
        self.log.borrow_mut().push((self.name, now.0));
        Polled { did_work: self.work }
    }
    fn next_deadline(&self) -> Option<Tsc> {
        self.deadline
    }
    fn has_pending(&self) -> bool {
        false
    }
}

fn fake(name: char, work: bool, log: &Log) -> Fake {
    Fake {
        name,
        work,
        log: log.clone(),
        deadline: None,
    }
}

#[test]
fn rt_110_round_order_and_fairness() {
    let log: Log = Rc::default();
    let tasks = [
        fake('A', true, &log),
        fake('B', false, &log),
        fake('C', false, &log),
    ];
    let mut tl = TaskLoop::new(tasks, IdlePolicy::Spin, None, &c1g());
    assert!(tl.round(Tsc(100)));
    assert_eq!(*log.borrow(), vec![('A', 100), ('B', 100), ('C', 100)]);
    log.borrow_mut().clear();
    for k in 2..=100u64 {
        tl.round(Tsc(k * 100));
    }
    let count = |n: char| log.borrow().iter().filter(|(x, _)| *x == n).count();
    assert_eq!((count('B'), count('C')), (99, 99));
    let idle = [
        fake('A', false, &log),
        fake('B', false, &log),
        fake('C', false, &log),
    ];
    let mut tl = TaskLoop::new(idle, IdlePolicy::Spin, None, &c1g());
    assert!(!tl.round(Tsc(1)));
}

fn one_task(policy: IdlePolicy) -> TaskLoop<Fake, 1> {
    let log: Log = Rc::default();
    TaskLoop::new([fake('A', false, &log)], policy, None, &c1g())
}

#[test]
fn rt_111_spin_idle() {
    let mut tl = one_task(IdlePolicy::Spin);
    assert_eq!(tl.idle_action(Tsc(0), false), IdleAction::Relax, "RT-111.1");
    assert_eq!(
        tl.idle_action(Tsc(1_000_000_000), false),
        IdleAction::Relax,
        "RT-111.2"
    );
    assert_eq!(
        tl.idle_action(Tsc(1_000_000_001), true),
        IdleAction::Continue,
        "RT-111.3"
    );
}

#[test]
fn rt_112_spin_then_park() {
    let mut tl = one_task(IdlePolicy::SpinThenPark {
        spin_us: 20,
        park_us: 1000,
    });
    let park = IdleAction::Park {
        timeout_ns: 1_000_000,
    };
    let rows = [
        ("RT-112.1", 1_000, false, IdleAction::Relax),
        ("RT-112.2", 20_999, false, IdleAction::Relax),
        ("RT-112.3", 21_000, false, park),
        ("RT-112.4", 21_500, true, IdleAction::Continue),
        ("RT-112.5", 22_000, false, IdleAction::Relax),
        ("RT-112.6", 42_000, false, park),
    ];
    for (row, now, did, want) in rows {
        assert_eq!(tl.idle_action(Tsc(now), did), want, "{row}");
    }
}

#[test]
fn rt_113_park_policy() {
    let mut tl = one_task(IdlePolicy::Park { park_us: 300 });
    assert_eq!(
        tl.idle_action(Tsc(5_000), false),
        IdleAction::Park { timeout_ns: 300_000 },
        "RT-113.1"
    );
    assert_eq!(tl.idle_action(Tsc(6_000), true), IdleAction::Continue, "RT-113.2");
}

#[test]
fn rt_114_park_timeout_deadline() {
    let rows: [(&str, Option<u64>, Option<u64>, IdleAction); 6] = [
        (
            "RT-114.1",
            None,
            None,
            IdleAction::Park {
                timeout_ns: 1_000_000,
            },
        ),
        (
            "RT-114.2",
            Some(60_000),
            None,
            IdleAction::Park { timeout_ns: 50_000 },
        ),
        (
            "RT-114.3",
            Some(60_000),
            Some(30_000),
            IdleAction::Park { timeout_ns: 20_000 },
        ),
        (
            "RT-114.4",
            Some(2_000_000),
            None,
            IdleAction::Park {
                timeout_ns: 1_000_000,
            },
        ),
        ("RT-114.5", Some(10_000), None, IdleAction::Continue),
        ("RT-114.6", Some(9_000), None, IdleAction::Continue),
    ];
    for (row, d1, d2, want) in rows {
        let log: Log = Rc::default();
        let mut t1 = fake('1', false, &log);
        let mut t2 = fake('2', false, &log);
        t1.deadline = d1.map(Tsc);
        t2.deadline = d2.map(Tsc);
        let mut tl = TaskLoop::new([t1, t2], IdlePolicy::Park { park_us: 1000 }, None, &c1g());
        assert_eq!(tl.idle_action(Tsc(10_000), false), want, "{row}");
    }
}

#[test]
fn rt_115_busy_never_parks() {
    let mut tl = one_task(IdlePolicy::SpinThenPark {
        spin_us: 20,
        park_us: 1000,
    });
    let mut now = 0u64;
    for k in 0..10_000u32 {
        now += 1_000;
        let did = k % 100 != 99;
        let a = tl.idle_action(Tsc(now), did);
        assert!(
            matches!(a, IdleAction::Continue | IdleAction::Relax),
            "round {k}: {a:?}"
        );
    }
}

struct Stopper<'a> {
    stop: &'a AtomicBool,
    calls: u32,
}

impl PollTask for Stopper<'_> {
    fn poll_once(&mut self, _: Tsc) -> Polled {
        self.calls += 1;
        if self.calls == 5 {
            self.stop.store(true, Ordering::Release);
        }
        Polled { did_work: false }
    }
    fn has_pending(&self) -> bool {
        false
    }
}

#[test]
fn rt_121_run_beats_and_stops() {
    let stop = AtomicBool::new(false);
    let hb = Heartbeat::new();
    let mut tl = TaskLoop::new(
        [Stopper {
            stop: &stop,
            calls: 0,
        }],
        IdlePolicy::Spin,
        None,
        &c1g(),
    );
    tl.run(&stop, &hb);
    assert_eq!(hb.read(), 5);
    assert_eq!(tl.tasks()[0].calls, 5);
}
