//! TK-106 fixture: repeated spot-first OnFill orders in both directions.
//! The ordinary DST suites retain their own strategies, quantities and market processes.
use hft_strategy_sdk::prelude::*;
use hft_strategy_sdk::{
    Ctx, EvMask, EventRef, GroupTable, HedgeKind, HedgePlan, IntentClass, LegLink, LegLinkOpt, LinkRole,
    NoModel, OnFail, OrderIntent, ParamError, Part, PxMode, RefView, ResidualPolicy, RestoreError,
    SnapshotReader, SnapshotWriter, Strategy, StrategyDesc, StrategyInstance, SubsBuilder, Views,
};
use hft_testkit::strategies::TakerParams;

#[derive(Debug)]
pub struct FeeCycle {
    inst: StrategyInstance,
    group: GroupId,
    first: (VenueId, InstId),
    hedge: (VenueId, InstId),
    qty: i64,
    high: i64,
    interval_ns: i64,
    next_ns: i64,
    perp_first: bool,
}

impl Strategy for FeeCycle {
    const DESC: StrategyDesc = StrategyDesc {
        name: "probe-fee-cycle",
        kind_id: 0xFF04,
        version: 1,
        non_prod_only: true,
        schema: &TakerParams::SCHEMA,
        notes: &[],
        report: &[],
        cmds: &[],
    };
    type Params = TakerParams;
    type Model = NoModel<TakerParams>;
    fn new(
        inst: StrategyInstance,
        p: &TakerParams,
        groups: &GroupTable,
        rd: &RefView<'_>,
    ) -> Result<Self, ParamError> {
        let (_, g) = groups.iter().next().unwrap();
        let a = g.legs[0];
        let b = g.legs[1];
        let qty = rd.inst(a.inst).lot.max(rd.inst(b.inst).lot) * 2;
        Ok(Self {
            inst,
            group: g.id,
            first: (a.venue, a.inst),
            hedge: (b.venue, b.inst),
            qty,
            high: qty * i64::from(p.at(0).qty_lots),
            interval_ns: i64::from(p.at(0).hold_ms) * 1_000_000,
            next_ns: 0,
            perp_first: rd.inst(a.inst).kind == hft_refdata::InstKind::Perp,
        })
    }
    fn subscriptions(&self, groups: &GroupTable, out: &mut SubsBuilder<'_>) {
        for (gix, g) in groups.iter() {
            for l in g.legs.iter().take(2) {
                let _ = out.market(l.inst, gix, EvMask::BBO, Part::Fast);
            }
        }
    }
    fn on_event<V: Views>(&mut self, ev: &EventRef<'_>, v: &V, ctx: &mut Ctx<'_>) {
        if !matches!(ev, EventRef::Bbo { .. }) || v.now().engine_ns < self.next_ns {
            return;
        }
        let legs = v.legs(self.inst.account, self.group);
        if legs.has_pending || legs.has_unknown {
            return;
        }
        let net = if self.perp_first {
            -legs.leg[0].net.0
        } else {
            legs.leg[0].net.0
        };
        let side = if net >= self.high { Side::Sell } else { Side::Buy };
        let side = if self.perp_first { side.opposite() } else { side };
        let hedge_side = if side == Side::Buy { Side::Sell } else { Side::Buy };
        let bbo = v.bbo(self.first.1);
        let link = LegLinkOpt::some(LegLink {
            group: self.group,
            role: LinkRole::First,
            _pad: [0; 3],
            tolerance: Rate(0),
            hedge: HedgePlan {
                kind: HedgeKind::OnFill,
                px_mode: PxMode::Cross,
                on_fail: OnFail::Retry,
                tif: Tif::Ioc,
                venue: self.hedge.0,
                account: self.inst.account,
                side: hedge_side,
                reduce_only: 0,
                inst: self.hedge.1,
                ratio_num: 1,
                ratio_den: 1,
                residual: ResidualPolicy::TrimExcess,
                _pad: [0; 3],
                px_param: Rate(5_000_000),
                max_slip: Rate(5_000_000),
                limit_px: Px(0),
            },
        });
        let _ = ctx.place(OrderIntent {
            venue: self.first.0,
            account: self.inst.account,
            side,
            tif: Tif::Ioc,
            class: IntentClass::Open,
            flags: 0,
            _pad: [0; 2],
            inst: self.first.1,
            group: self.group,
            px: if side == Side::Buy { bbo.ask_px } else { bbo.bid_px },
            qty: Qty(self.qty),
            link,
            tag: 0,
            max_rest_ms: 0,
        });
        self.next_ns = v.now().engine_ns + self.interval_ns;
    }
    fn on_params(&mut self, p: &TakerParams, _: &GroupTable, _: &RefView<'_>) -> Result<(), ParamError> {
        self.high = self.qty * i64::from(p.at(0).qty_lots);
        self.interval_ns = i64::from(p.at(0).hold_ms) * 1_000_000;
        Ok(())
    }
    fn snapshot(&self, w: &mut SnapshotWriter) {
        w.put_i64(self.next_ns);
    }
    fn restore(&mut self, r: &mut SnapshotReader<'_>) -> Result<(), RestoreError> {
        self.next_ns = r.get_i64()?;
        Ok(())
    }
}

hft_strategy_sdk::strategy_set! { pub struct FeeStrategies { FeeCycle } }

#[test]
fn tk_106_perp_first_fee_residual_reclaims_links_seed_1017c4() {
    use hft_event::{Note, OrderAction, OrderDecision};
    use hft_testkit::dst::{RerunPolicy, SeqSut, run_record};
    let mut c = config(0);
    c.seed = 0x1017c4;
    c.steps = 20_000;
    c.horizon_ns = 30_000_000_000;
    c.scripted.clear();
    c.world.accounts.truncate(1);
    c.world.groups.truncate(1);
    c.world.strategies.truncate(1);
    for group in &mut c.world.groups {
        core::mem::swap(&mut group.first, &mut group.second);
    }
    c.rerun = RerunPolicy::Always;
    let r = run_record::<SeqSut<FeeStrategies>>(&c);
    assert!(r.outcome.is_pass(), "{}", crate::seq_sut::show(&r));
    let orders: Vec<_> = r
        .journal
        .iter()
        .filter_map(|j| j.try_read::<OrderDecision>())
        .filter(|d| d.action == OrderAction::New)
        .collect();
    assert!(
        orders
            .iter()
            .filter(|d| d.link_role == 1 && d.reject == 0)
            .count()
            > 32,
        "orders={orders:?}"
    );
    assert!(
        orders
            .iter()
            .all(|d| d.reject & hft_types::RejectBits::CAPACITY.0 == 0)
    );
    assert!(
        r.journal
            .iter()
            .filter_map(|j| j.try_read::<Note>())
            .any(|n| n.code == hft_oms::notes::code_of(hft_oms::notes::HEDGE_RESIDUAL))
    );
    assert!(r.snapshot_checks > 0);
}

pub fn config(delay_ms: u32) -> hft_testkit::dst::DstConfig {
    use hft_testkit::dst::{MarketGen, MarketSource, faults::Fault, seq::build, world::VS};
    let mut c = crate::support::cfg(1, 10_000, &[], 0, &[]);
    c.market = MarketSource::Generated(MarketGen {
        tick_ms: 50,
        walk_centi_bp: 0,
    });
    for s in &mut c.world.strategies {
        s.kind_id = 0xFF04;
        s.params = hft_strategy_sdk::params::encode_body(
            &TakerParams::SCHEMA,
            1,
            &hft_strategy_sdk::params::RawParams {
                defaults: vec![build::TAKER_SPREAD_TH, 20, 0, 100],
                overrides: Vec::new(),
            },
        )
        .unwrap();
    }
    c.world
        .venues
        .iter_mut()
        .find(|v| v.id == VS)
        .unwrap()
        .fee_in_base = true;
    // A fee fixture over a still market: no marks or restart margin snapshots (TK-106/TK-110
    // pin the late-fee path of this exact delivery order).
    c.world.margin_feed = false;
    c.scripted.push((0, Fault::LateFeeOnly { venue: VS, delay_ms }));
    c
}
