//! Strategy test stubs (README "4. 策略测试桩"). `MockViews` is the SDK's (`testing` feature, one
//! implementation); this module adds the single-strategy driver `StrategyHarness` and the Model
//! driver `ModelHarness`. Enabled by this crate's `sdk-mock` feature (dev-dependencies only).
//!
//! `StrategyHarness::feed` checks, for every call: zero allocations (when `CountingAlloc` is the
//! global allocator of the test binary), no clock read (when a `ManualTsc` is bound), no panic
//! (reported with the 0-based event number).

use std::panic::{AssertUnwindSafe, catch_unwind};

use hft_strategy_sdk::ctx::{CancelReq, CtxBufs, NoteReq, Op, TimerReq};
use hft_strategy_sdk::{
    AdviceBufs, EventRef, GroupTable, IntentHandle, Model, OrderIntent, ParamError, ParamSet, RestoreError,
    SnapshotReader, SnapshotWriter, Strategy, StrategyInstance, TargetIntent, TimerId,
};
use hft_types::{AccountId, GroupId, StrategyId};

pub use hft_strategy_sdk::testing::{MockViews, PortfolioFixture, RefFixture};

use crate::alloc::CountingAlloc;
use crate::clock::ManualTsc;

/// One captured `Ctx` operation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Captured {
    Place(OrderIntent),
    Target(TargetIntent),
    ClearTarget(AccountId, GroupId),
    Cancel(CancelReq),
    Timer(TimerReq),
    CancelTimer(TimerId),
    Halt(GroupId, u16),
    /// Notes follow the operations (the SDK keeps them in their own array).
    Note(NoteReq),
}

/// Everything one `feed` produced, in call order.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CtxCapture {
    pub items: Vec<Captured>,
    pub handles: Vec<IntentHandle>,
    /// `CtxFull` count of the call.
    pub full: u32,
}

impl CtxCapture {
    pub fn places(&self) -> impl Iterator<Item = &OrderIntent> {
        self.items.iter().filter_map(|c| match c {
            Captured::Place(p) => Some(p),
            _ => None,
        })
    }
    pub fn targets(&self) -> impl Iterator<Item = &TargetIntent> {
        self.items.iter().filter_map(|c| match c {
            Captured::Target(t) => Some(t),
            _ => None,
        })
    }
}

/// Drives one strategy instance against `MockViews`.
#[derive(Debug)]
pub struct StrategyHarness<S: Strategy> {
    s: S,
    views: MockViews,
    bufs: Box<CtxBufs>,
    groups: GroupTable,
    params: S::Params,
    inst: StrategyInstance,
    events: u64,
    clock: Option<&'static ManualTsc>,
}

impl<S: Strategy> StrategyHarness<S>
where
    S::Params: Clone,
{
    /// Validates the parameters (`ParamSet::validate`) and builds the instance (instance 1 bound to
    /// the fixture's account).
    pub fn new(
        params: &S::Params,
        groups: &GroupTable,
        rd: RefFixture,
        account: AccountId,
    ) -> Result<Self, ParamError> {
        let views = MockViews::new(rd);
        params.validate(groups, &views.refview())?;
        let inst = StrategyInstance {
            strategy: StrategyId(1),
            account,
            kind_id: S::DESC.kind_id,
        };
        let s = S::new(inst, params, groups, &views.refview())?;
        Ok(StrategyHarness {
            s,
            views,
            bufs: Box::new(CtxBufs::new()),
            groups: groups.clone(),
            params: params.clone(),
            inst,
            events: 0,
            clock: None,
        })
    }
    /// Decodes a parameter body with the strategy's schema (range, enum and per-group checks:
    /// `ParamError::OutOfRange` …) and builds the harness.
    pub fn from_body(
        body: &[u8],
        schema_ver: u16,
        groups: &GroupTable,
        rd: RefFixture,
        account: AccountId,
    ) -> Result<Self, ParamError> {
        let p = S::Params::decode(body, schema_ver, groups)?;
        Self::new(&p, groups, rd, account)
    }
    /// Binds a manual clock: any read during `feed` fails the call ("读取了时钟").
    pub fn bind_clock(&mut self, c: &'static ManualTsc) {
        self.clock = Some(c);
    }
    pub fn views_mut(&mut self) -> &mut MockViews {
        &mut self.views
    }
    pub fn strategy(&self) -> &S {
        &self.s
    }
    pub fn strategy_mut(&mut self) -> &mut S {
        &mut self.s
    }
    /// Events fed so far.
    pub fn events(&self) -> u64 {
        self.events
    }

    /// One event; asserts no allocation, no clock read, no panic.
    pub fn feed(&mut self, ev: &EventRef<'_>) -> CtxCapture {
        let n = self.events;
        self.events += 1;
        let reads0 = self.clock.map_or(0, |c| c.reads());
        self.bufs.begin();
        let alloc = CountingAlloc::is_installed().then(CountingAlloc::scope);
        let (s, views, bufs) = (&mut self.s, &self.views, &mut self.bufs);
        let r = catch_unwind(AssertUnwindSafe(|| {
            let mut ctx = bufs.ctx();
            s.on_event(ev, views, &mut ctx);
        }));
        let allocs = alloc.map_or(0, |a| a.allocs());
        if let Err(p) = r {
            let msg = p
                .downcast_ref::<String>()
                .cloned()
                .or_else(|| p.downcast_ref::<&str>().map(|s| (*s).to_string()))
                .unwrap_or_default();
            panic!("StrategyHarness: on_event panicked at event {n}: {msg}");
        }
        assert!(
            allocs == 0,
            "StrategyHarness: on_event 分配次数 {allocs} (event {n})"
        );
        if let Some(c) = self.clock {
            assert!(
                c.reads() == reads0,
                "StrategyHarness: on_event 读取了时钟 (event {n})"
            );
        }
        let mut cap = CtxCapture {
            full: self.bufs.full_count(),
            ..CtxCapture::default()
        };
        for (h, op) in self.bufs.ops() {
            cap.handles.push(h);
            cap.items.push(match op {
                Op::Place(p) => Captured::Place(*p),
                Op::Target(t) => Captured::Target(*t),
                Op::ClearTarget(a, g) => Captured::ClearTarget(a, g),
                Op::Cancel(c) => Captured::Cancel(*c),
                Op::Timer(t) => Captured::Timer(*t),
                Op::CancelTimer(id) => Captured::CancelTimer(id),
                Op::Halt { group, reason } => Captured::Halt(group, reason),
            });
        }
        for nr in self.bufs.notes() {
            cap.items.push(Captured::Note(*nr));
        }
        self.bufs.clear_outcomes();
        cap
    }

    /// Validates and applies new parameters; a validation failure leaves the strategy untouched.
    pub fn on_params(&mut self, p: &S::Params) -> Result<(), ParamError> {
        p.validate(&self.groups, &self.views.refview())?;
        self.s.on_params(p, &self.groups, &self.views.refview())?;
        self.params = p.clone();
        Ok(())
    }

    /// The fast-part snapshot bytes.
    pub fn snapshot(&self) -> Vec<u8> {
        let mut w = SnapshotWriter::new(1 << 20);
        self.s.snapshot(&mut w);
        w.as_bytes().to_vec()
    }

    /// snapshot → restore into a fresh instance → snapshot again; the bytes must be equal (panics
    /// with the first differing offset otherwise).
    pub fn snapshot_roundtrip(&mut self) -> Result<(), RestoreError> {
        let a = self.snapshot();
        let mut fresh = S::new(self.inst, &self.params, &self.groups, &self.views.refview())
            .map_err(|_| RestoreError::VersionUnsupported)?;
        let mut r = SnapshotReader::with_desc_version(&a, S::DESC.version);
        fresh.restore(&mut r)?;
        let mut w = SnapshotWriter::new(1 << 20);
        fresh.snapshot(&mut w);
        let b = w.as_bytes();
        if a.as_slice() != b {
            let off = a
                .iter()
                .zip(b.iter())
                .position(|(x, y)| x != y)
                .unwrap_or(a.len().min(b.len()));
            panic!(
                "snapshot_roundtrip: snapshots differ at byte offset {off} (len {} vs {})",
                a.len(),
                b.len()
            );
        }
        Ok(())
    }
}

/// Drives a strategy's `Model` against `MockViews`.
#[derive(Debug)]
pub struct ModelHarness<S: Strategy> {
    m: S::Model,
    views: MockViews,
    out: Box<AdviceBufs>,
}

impl<S: Strategy> ModelHarness<S> {
    pub fn new(params: &S::Params, groups: &GroupTable, rd: RefFixture, account: AccountId) -> Self {
        let views = MockViews::new(rd);
        let inst = StrategyInstance {
            strategy: StrategyId(1),
            account,
            kind_id: S::DESC.kind_id,
        };
        let m = <S::Model as Model>::new(inst, params, groups, &views.refview());
        ModelHarness {
            m,
            views,
            out: Box::new(AdviceBufs::new()),
        }
    }
    pub fn views_mut(&mut self) -> &mut MockViews {
        &mut self.views
    }
    pub fn model(&self) -> &S::Model {
        &self.m
    }
    /// One event; returns the Advice `(group, local_kind, payload)` written.
    pub fn feed(&mut self, ev: &EventRef<'_>) -> Vec<(GroupId, u16, Vec<u8>)> {
        self.out.begin();
        {
            let mut o = self.out.out();
            self.m.on_event(ev, &self.views, &mut o);
        }
        self.out.iter().map(|(g, k, b)| (g, k, b.to_vec())).collect()
    }
}
