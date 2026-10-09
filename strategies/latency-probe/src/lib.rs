//! `latency-probe`: a measurement-only strategy (README): quote probes (PostOnly far from the touch,
//! cancelled on Ack), pair probes (IOC + LegLink second leg) and target probes (TargetIntent sliced by
//! hft-exec) on non-production accounts. `Model = NoModel`. Specification:
//! `strategies/latency-probe/README.md`.
#![forbid(unsafe_code)]
#![deny(
    clippy::float_arithmetic,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unreachable,
    clippy::todo,
    clippy::unimplemented
)]

pub mod desc;
pub mod fast;
pub mod params;

use hft_strategy_sdk::{
    Ctx, EvMask, EventRef, GroupTable, NoModel, ParamError, Part, RefView, RestoreError, SnapshotReader,
    SnapshotWriter, Strategy, StrategyDesc, StrategyInstance, SubsBuilder, Views,
};

pub use fast::{Phase, ProbeGroup};
pub use params::{ProbeFields, ProbeParams};

/// `StrategyDesc.version` (fast-part snapshot layout).
pub const DESC_VERSION: u16 = 1;

/// The strategy (registry row `latency_probe::LatencyProbe`, kind_id 5).
#[derive(Debug)]
pub struct LatencyProbe {
    pub inst: StrategyInstance,
    pub fast: fast::Fast,
}

impl LatencyProbe {
    /// State of the group at `gix` (tests and diagnostics).
    pub fn group(&self, gix: u16) -> Option<&ProbeGroup> {
        (gix < self.fast.n)
            .then(|| self.fast.groups.get(usize::from(gix)))
            .flatten()
    }
}

impl Strategy for LatencyProbe {
    const DESC: StrategyDesc = StrategyDesc {
        name: "latency-probe",
        kind_id: 5,
        version: DESC_VERSION,
        non_prod_only: true,
        schema: &ProbeParams::SCHEMA,
        notes: &desc::NOTES,
        report: &desc::REPORT,
        cmds: &[],
    };
    type Params = ProbeParams;
    type Model = NoModel<ProbeParams>;

    fn new(
        inst: StrategyInstance,
        p: &ProbeParams,
        groups: &GroupTable,
        rd: &RefView<'_>,
    ) -> Result<Self, ParamError> {
        Ok(LatencyProbe {
            inst,
            fast: fast::Fast::new(inst.account, p, groups, rd),
        })
    }

    fn subscriptions(&self, groups: &GroupTable, out: &mut SubsBuilder<'_>) {
        for (gix, g) in groups.iter() {
            for l in g.legs.iter().take(usize::from(g.n_legs).min(fast::MAX_LEGS)) {
                let _ = out.market(l.inst, gix, EvMask::BBO, Part::Fast);
            }
        }
    }

    fn on_event<V: Views>(&mut self, ev: &EventRef<'_>, v: &V, ctx: &mut Ctx<'_>) {
        self.fast.on_event(ev, v, ctx);
    }

    fn on_params(
        &mut self,
        p: &ProbeParams,
        groups: &GroupTable,
        rd: &RefView<'_>,
    ) -> Result<(), ParamError> {
        self.fast.on_params(p, groups, rd);
        Ok(())
    }

    fn snapshot(&self, w: &mut SnapshotWriter) {
        self.fast.snapshot(w);
    }

    fn restore(&mut self, r: &mut SnapshotReader<'_>) -> Result<(), RestoreError> {
        self.fast.restore(r)
    }
}
