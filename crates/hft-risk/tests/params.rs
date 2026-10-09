//! RiskParams defaults, validation, capacity, scope, `loosens` and `ExecRiskMismatch`
//! (RSK-001–RSK-007).
#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods
)] // allow-list: test code, not a hot path

mod common;
use common::*;

use hft_risk::{
    AccountScope, AcctSpec, BandSpec, Capacities, ExecBounds, GroupLimitsRow, InstClassRow, ParamsBuilder,
    PreVec, Risk, RiskParamError, RiskParams,
};
use hft_strategy_sdk::MarginLevel;
use hft_testkit::alloc::CountingAlloc;
use hft_types::{AccountId, GroupId, InstId, Notional, Qty, Rate, RejectBits, Side, StrategyId, Tif};

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc;

fn new_risk(
    rd: &'static hft_refdata::RefData,
    p: &RiskParams,
    exec: &[ExecBounds],
) -> Result<Risk, RiskParamError> {
    Risk::new(p, rd, &gws_for(rd), &CAPS, exec, MC)
}

fn is_invalid(r: &Result<Risk, RiskParamError>) -> bool {
    matches!(r, Err(RiskParamError::Invalid(_)))
}

#[test]
fn rsk_001_risk_params_defaults() {
    let mut sp = r0_spec();
    sp.accounts.push(account_spec(4, false, 4, &[C]));
    sp.instances.push(hft_refdata::InstanceSpec {
        id: StrategyId(4),
        kind_id: 0xFF00,
        name: "inst-4".into(),
    });
    let rd = load(&sp);
    let mut b = ParamsBuilder::new(rd);
    b.classes.push(BandSpec {
        max_px_dev: r("0.03"),
        ioc_max_cross: r("0.005"),
        rr_max_cross: None,
    });
    for a in [1u8, 2, 4] {
        b.accounts.push((
            acc(a),
            AcctSpec {
                max_order_notional: n("10000"),
                ..AcctSpec::default()
            },
        ));
    }
    let p = b.build();
    let ac = |a: u8| *p.accounts.iter().find(|x| x.account == acc(a)).unwrap();
    let a1 = ac(1);
    assert_eq!(p.conn_liveness_ms, 2_000, "RSK-001.1");
    assert_eq!(p.max_perp_leverage_x100, 300, "RSK-001.2");
    assert_eq!(p.l3_max_cross, Rate(0), "RSK-001.3");
    assert!(p.venues.iter().all(|v| v.rr_reserve == r("0.20")), "RSK-001.4");
    assert_eq!(p.classes.get(0).unwrap().rr_max_cross, r("0.01"), "RSK-001.5");
    assert_eq!(a1.max_hedge_notional, n("20000"), "RSK-001.6");
    assert_eq!(a1.max_reduce_notional, n("10000"), "RSK-001.7");
    assert_eq!(a1.daily_loss_max_resumes, 1, "RSK-001.8");
    assert_eq!(a1.funds_buffer, r("0.05"), "RSK-001.9");
    let m = a1.margin;
    assert_eq!(
        (m.liq_dist_block_open, m.liq_dist_reduce, m.liq_dist_force),
        (r("0.25"), r("0.15"), r("0.10")),
        "RSK-001.10"
    );
    assert_eq!(
        (m.liq_dist_hyst, m.margin_ratio_crosscheck, m.margin_ratio_hyst),
        (r("0.02"), r("0.80"), r("0.05")),
        "RSK-001.11"
    );
    assert_eq!(
        (m.deleverage_deadline_ms, m.force_step, m.force_step_interval_ms),
        (30_000, r("0.20"), 5_000),
        "RSK-001.12"
    );
    assert_eq!(m.mark_stale_ms, 10_000, "RSK-001.13");
    assert_eq!(ac(2).margin.margin_stale_ms, 15_000, "RSK-001.14");
    assert_eq!(ac(4).margin.margin_stale_ms, 6_000, "RSK-001.15");
    assert_eq!(a1.margin.margin_stale_ms, 15_000, "RSK-001.16");
    assert!(a1.unknown_blocks_open, "RSK-001.17");
}

#[test]
fn rsk_002_param_validation() {
    let rd = r0_refdata();
    let p0 = p0_for(rd, 3);
    let ex = exec_bounds(3);
    type Edit = fn(&mut hft_risk::AcctLimits);
    let rows: [(&str, Edit, bool); 11] = [
        ("RSK-002.1", |_| {}, true),
        ("RSK-002.2", |a| a.margin.liq_dist_block_open = r("0.15"), false),
        (
            "RSK-002.3",
            |a| a.margin.liq_dist_block_open = r("0.150000001"),
            true,
        ),
        ("RSK-002.4", |a| a.margin.liq_dist_reduce = r("0.10"), false),
        ("RSK-002.5", |a| a.margin.liq_dist_force = Rate(0), false),
        ("RSK-002.6", |a| a.margin.liq_dist_force = r("0.000000001"), true),
        ("RSK-002.7", |a| a.margin.liq_dist_hyst = r("0.05"), false),
        ("RSK-002.8", |a| a.margin.liq_dist_hyst = r("0.049999999"), true),
        ("RSK-002.9", |a| a.daily_loss_abs = Notional(0), false),
        ("RSK-002.10", |a| a.daily_loss_abs = n("0.000001"), true),
        ("RSK-002.11", |a| a.daily_loss_abs = n("-1"), false),
    ];
    for (id, f, ok) in rows {
        let p = with_acct(&p0, 1, f);
        let res = new_risk(rd, &p, &ex);
        if ok {
            assert!(res.is_ok(), "{id}: {:?}", res.err());
        } else {
            assert!(is_invalid(&res), "{id}: {:?}", res.err());
        }
    }
}

#[test]
fn rsk_003_invalid_setrisk_keeps_old() {
    let mut w = r0();
    let p = with_acct(&w.p, 1, |a| {
        a.margin.liq_dist_block_open = r("0.15");
        a.max_order_notional = n("20000");
    });
    let res = w.risk.set_params(&p, AccountScope::Account(acc(1)));
    assert!(matches!(res, Err(RiskParamError::Invalid(_))), "{res:?}");
    let s = msample("0.25", "0.10", T0);
    w.margin(1, B, &s, T0);
    assert_eq!(w.risk.view(acc(1), &w.nowv()).margin_level, MarginLevel::L1);
    let it = open(1, G1, A1, Side::Buy, "0.3", "50000.00", Tif::Gtc);
    assert_eq!(res_of(w.eval(&it)), err(RejectBits::NOTIONAL_ORDER));
}

fn res_of(r: Result<hft_risk::Admission, RejectBits>) -> Result<(), RejectBits> {
    r.map(|_| ())
}

/// 20 Live accounts, each only on A.
fn twenty() -> &'static hft_refdata::RefData {
    let mut sp = r0_spec();
    sp.accounts = (1..=20u8)
        .map(|a| account_spec(a, false, a, if a <= 10 { &[A] } else { &[B] }))
        .collect();
    sp.instances = (1..=20u8)
        .map(|i| hft_refdata::InstanceSpec {
            id: StrategyId(i),
            kind_id: 0xFF00,
            name: format!("inst-{i}").into(),
        })
        .collect();
    sp.sims.clear();
    load(&sp)
}

#[test]
fn rsk_004_capacity_rows() {
    let rd = twenty();
    let caps = Capacities {
        max_insts: 64,
        max_cells_total: 32,
        max_caps: 256,
    };
    let p0 = p0_for(rd, 20);
    let ex = exec_bounds(20);
    let mut risk = Risk::new(&p0, rd, &gws_for(rd), &caps, &ex, MC).unwrap();
    let now = now_at(T0);

    // .1: 20 account rows, no allocation.
    let p20 = with_acct(&p0, 1, |a| a.max_order_notional = n("12000"));
    {
        let sc = CountingAlloc::scope();
        assert_eq!(risk.set_params(&p20, AccountScope::All), Ok(()), "RSK-004.1");
        assert_eq!(sc.allocs(), 0, "RSK-004.1 allocations");
    }
    // .2: 21 account rows.
    let mut p21 = p20.clone();
    let mut v: Vec<_> = p21.accounts.iter().copied().collect();
    let mut extra = v[0];
    extra.max_order_notional = n("1");
    v.push(extra);
    p21.accounts = PreVec::from_vec(v);
    assert_eq!(
        risk.set_params(&p21, AccountScope::All),
        Err(RiskParamError::Capacity),
        "RSK-004.2"
    );
    assert_eq!(
        risk.view(acc(1), &now).max_order_notional,
        n("12000"),
        "RSK-004.2 old value"
    );
    // .3 / .4: groups.
    let groups = |k: usize| {
        let rows: Vec<GroupLimitsRow> = (0..k)
            .map(|i| GroupLimitsRow {
                account: acc((i % 20) as u8 + 1),
                group: GroupId((((i % 20) as u32 + 1) << 24) | (i as u32 / 20 + 1)),
                l: group_limits("1"),
            })
            .collect();
        let mut p = p20.clone();
        p.groups = PreVec::from_vec(rows);
        p
    };
    assert_eq!(
        risk.set_params(&groups(32), AccountScope::All),
        Ok(()),
        "RSK-004.3"
    );
    assert_eq!(
        risk.set_params(&groups(33), AccountScope::All),
        Err(RiskParamError::Capacity),
        "RSK-004.4"
    );
    // .5 / .6: risk_class.
    let classes = |k: u32| {
        let mut p = p20.clone();
        p.risk_class = PreVec::from_vec(
            (1..=k)
                .map(|i| InstClassRow {
                    inst: InstId(i),
                    class: 0,
                })
                .collect(),
        );
        p
    };
    assert_eq!(
        risk.set_params(&classes(64), AccountScope::All),
        Ok(()),
        "RSK-004.5"
    );
    assert_eq!(
        risk.set_params(&classes(65), AccountScope::All),
        Err(RiskParamError::Capacity),
        "RSK-004.6"
    );
    // .7: 21 venue rows ((account, venue) count is 20).
    let mut pv = p20.clone();
    let mut rows: Vec<_> = pv.venues.iter().copied().collect();
    assert_eq!(rows.len(), 20);
    rows.push(rows[0]);
    pv.venues = PreVec::from_vec(rows);
    assert_eq!(
        risk.set_params(&pv, AccountScope::All),
        Err(RiskParamError::Capacity),
        "RSK-004.7"
    );
}

#[test]
fn rsk_005_account_scope_replace() {
    let mut w = r0();
    let mut p1 = with_acct(&w.p, 1, |a| a.max_order_notional = n("15000"));
    p1 = with_acct(&p1, 2, |a| a.max_order_notional = n("15000"));
    p1 = with_class(&p1, 0, |b| b.rr_max_cross = r("0.02"));
    p1.l3_max_cross = r("0.03");
    assert_eq!(w.risk.set_params(&p1, AccountScope::Account(acc(1))), Ok(()));
    let now = w.nowv();
    assert_eq!(w.risk.view(acc(1), &now).max_order_notional, n("15000"));
    assert_eq!(w.risk.view(acc(2), &now).max_order_notional, n("10000"));
    let vf = w.risk.view_for(acc(1), A1, &now);
    assert_eq!(vf.rr_max_cross, r("0.01"));
    assert_eq!(vf.l3_max_cross, Rate(0));
}

fn up_n(x: &mut Notional) {
    x.0 += 1;
}

#[test]
fn rsk_006_loosens_by_field() {
    let w = r0();
    let p0 = w.p.clone();
    let s1 = AccountScope::Account(acc(1));
    let lo = |p: &RiskParams, s: AccountScope| w.risk.loosens(p, s);
    let acct = |f: &dyn Fn(&mut hft_risk::AcctLimits)| with_acct(&p0, 1, f);

    assert!(!lo(&p0, s1), "RSK-006.1");
    assert!(
        lo(&acct(&|a| a.max_order_notional = n("10000.000001")), s1),
        "RSK-006.2"
    );
    assert!(
        !lo(&acct(&|a| a.max_order_notional = n("9999.999999")), s1),
        "RSK-006.3"
    );
    // .4: six sub-cases.
    assert!(
        lo(&acct(&|a| up_n(&mut a.max_hedge_notional)), s1),
        "RSK-006.4 hedge"
    );
    assert!(
        lo(&acct(&|a| up_n(&mut a.max_reduce_notional)), s1),
        "RSK-006.4 reduce"
    );
    assert!(
        lo(&acct(&|a| up_n(&mut a.max_account_notional)), s1),
        "RSK-006.4 account"
    );
    assert!(
        lo(&with_group(&p0, 1, G1, |g| up_n(&mut g.max_group_notional)), s1),
        "RSK-006.4 group"
    );
    assert!(
        lo(&with_venue(&p0, A, 1, |v| up_n(&mut v.max_venue_notional)), s1),
        "RSK-006.4 venue"
    );
    assert!(
        lo(
            &with_group(&p0, 1, G1, |g| g.max_pos_qty = Qty(g.max_pos_qty.0 + 1)),
            s1
        ),
        "RSK-006.4 pos"
    );
    assert!(
        lo(&acct(&|a| a.account_notional_from_equity = r("1.000000001")), s1),
        "RSK-006.5"
    );
    assert!(lo(&acct(&|a| up_n(&mut a.daily_loss_abs)), s1), "RSK-006.6 abs");
    assert!(
        lo(&acct(&|a| a.daily_loss_kill = Rate(a.daily_loss_kill.0 + 1)), s1),
        "RSK-006.6 kill"
    );
    assert!(
        lo(&acct(&|a| a.daily_loss_max_resumes = 2), s1),
        "RSK-006.6 resumes"
    );
    assert!(
        lo(&acct(&|a| up_n(&mut a.recon_closeonly_notional)), s1),
        "RSK-006.7 recon"
    );
    assert!(
        lo(
            &with_group(&p0, 1, G1, |g| g.naked_gate = Rate(g.naked_gate.0 + 1)),
            s1
        ),
        "RSK-006.7 gate"
    );
    assert!(
        lo(&with_group(&p0, 1, G1, |g| up_n(&mut g.naked_gate_min)), s1),
        "RSK-006.7 gate_min"
    );
    assert!(
        lo(&with_venue(&p0, A, 1, |v| v.inst_bucket_per_s += 1), s1),
        "RSK-006.7 bucket"
    );
    assert!(
        lo(&acct(&|a| a.margin.liq_dist_block_open = r("0.249999999")), s1),
        "RSK-006.8"
    );
    assert!(
        !lo(&acct(&|a| a.margin.liq_dist_block_open = r("0.250000001")), s1),
        "RSK-006.9"
    );
    let dn = |x: &mut Rate| x.0 -= 1;
    assert!(
        lo(&acct(&|a| dn(&mut a.margin.liq_dist_reduce)), s1),
        "RSK-006.10 reduce"
    );
    assert!(
        lo(&acct(&|a| dn(&mut a.margin.liq_dist_force)), s1),
        "RSK-006.10 force"
    );
    assert!(
        lo(&acct(&|a| dn(&mut a.margin.liq_dist_hyst)), s1),
        "RSK-006.10 hyst"
    );
    assert!(
        lo(&acct(&|a| dn(&mut a.margin.margin_ratio_hyst)), s1),
        "RSK-006.10 ratio_hyst"
    );
    assert!(
        lo(&acct(&|a| dn(&mut a.margin.force_step)), s1),
        "RSK-006.10 force_step"
    );
    assert!(
        lo(&acct(&|a| a.margin.margin_ratio_crosscheck.0 += 1), s1),
        "RSK-006.11 crosscheck"
    );
    assert!(
        lo(&acct(&|a| a.margin.deleverage_deadline_ms += 1), s1),
        "RSK-006.11 deadline"
    );
    assert!(
        lo(&acct(&|a| a.margin.force_step_interval_ms += 1), s1),
        "RSK-006.11 interval"
    );
    assert!(
        lo(&acct(&|a| a.margin.margin_stale_ms += 1), s1),
        "RSK-006.11 margin_stale"
    );
    assert!(
        lo(&acct(&|a| a.margin.mark_stale_ms += 1), s1),
        "RSK-006.11 mark_stale"
    );
    assert!(
        lo(&acct(&|a| a.funds_buffer = r("0.049999999")), s1),
        "RSK-006.12"
    );
    assert!(
        !lo(&acct(&|a| a.funds_buffer = r("0.050000001")), s1),
        "RSK-006.13"
    );
    assert!(lo(&acct(&|a| a.unknown_blocks_open = false), s1), "RSK-006.14");
    {
        let w2 = r0_with(|p| with_acct(&p, 1, |a| a.unknown_blocks_open = false));
        let back = with_acct(&w2.p, 1, |a| a.unknown_blocks_open = true);
        assert!(!w2.risk.loosens(&back, s1), "RSK-006.15");
    }
    assert!(
        lo(&with_venue(&p0, A, 1, |v| v.rr_reserve = r("0.19")), s1),
        "RSK-006.16 reserve"
    );
    assert!(
        lo(&with_venue(&p0, A, 1, |v| v.backoff_base_ms = 999), s1),
        "RSK-006.16 base"
    );
    assert!(
        lo(&with_venue(&p0, A, 1, |v| v.backoff_max_ms = 7_999), s1),
        "RSK-006.16 max"
    );
    let all = AccountScope::All;
    let glob = |f: &dyn Fn(&mut RiskParams)| {
        let mut p = p0.clone();
        f(&mut p);
        p
    };
    assert!(!lo(&glob(&|p| p.rate_headroom = r("0.1")), all), "RSK-006.17");
    {
        let w2 = r0_with(|mut p| {
            p.rate_headroom = r("0.1");
            p
        });
        let mut back = w2.p.clone();
        back.rate_headroom = Rate(0);
        assert!(w2.risk.loosens(&back, all), "RSK-006.18");
    }
    assert!(lo(&glob(&|p| p.conn_liveness_ms = 2_001), all), "RSK-006.19");
    assert!(lo(&glob(&|p| p.max_perp_leverage_x100 = 301), all), "RSK-006.20");
    assert!(lo(&glob(&|p| p.l3_max_cross = Rate(1)), all), "RSK-006.21");
    {
        let w2 = r0_with(|mut p| {
            p.l3_max_cross = r("0.02");
            p
        });
        let set = |x: &str| {
            let mut p = w2.p.clone();
            p.l3_max_cross = r(x);
            p
        };
        assert!(w2.risk.loosens(&set("0.03"), all), "RSK-006.22");
        assert!(!w2.risk.loosens(&set("0.01"), all), "RSK-006.23 0.01");
        assert!(!w2.risk.loosens(&set("0"), all), "RSK-006.23 0");
    }
    assert!(
        lo(&with_class(&p0, 0, |b| b.max_px_dev.0 += 1), all),
        "RSK-006.24 dev"
    );
    assert!(
        lo(&with_class(&p0, 0, |b| b.ioc_max_cross.0 += 1), all),
        "RSK-006.24 ioc"
    );
    assert!(
        lo(&with_class(&p0, 0, |b| b.rr_max_cross.0 += 1), all),
        "RSK-006.24 rr"
    );
    let reclass = |inst: InstId, class: u8| {
        let mut p = p0.clone();
        let rows: Vec<InstClassRow> = p
            .risk_class
            .iter()
            .map(|r| {
                if r.inst == inst {
                    InstClassRow { inst, class }
                } else {
                    *r
                }
            })
            .collect();
        p.risk_class = PreVec::from_vec(rows);
        p
    };
    assert!(lo(&reclass(A1, 1), all), "RSK-006.25");
    assert!(!lo(&reclass(A2, 0), all), "RSK-006.26");
    assert!(
        !lo(&with_acct(&p0, 2, |a| a.max_order_notional = n("20000")), s1),
        "RSK-006.27"
    );
}

#[test]
fn rsk_007_exec_risk_mismatch() {
    let rd = r0_refdata();
    let p0 = p0_for(rd, 3);
    let ex_with = |a: usize, f: &dyn Fn(&mut ExecBounds)| {
        let mut e = exec_bounds(3);
        f(&mut e[a]);
        e
    };
    let mm = |r: Result<Risk, RiskParamError>| matches!(r, Err(RiskParamError::ExecRiskMismatch));
    assert!(
        new_risk(rd, &p0, &ex_with(1, &|b| b.force_max_cross = r("0.01"))).is_ok(),
        "RSK-007.1"
    );
    assert!(
        mm(new_risk(
            rd,
            &p0,
            &ex_with(1, &|b| b.force_max_cross = r("0.010000001"))
        )),
        "RSK-007.2"
    );
    assert!(
        new_risk(rd, &p0, &ex_with(1, &|b| b.force_max_cross = r("0.009999999"))).is_ok(),
        "RSK-007.3"
    );
    assert!(
        new_risk(rd, &p0, &ex_with(1, &|b| b.force_max_chunk = n("10000"))).is_ok(),
        "RSK-007.4"
    );
    assert!(
        mm(new_risk(
            rd,
            &p0,
            &ex_with(1, &|b| b.force_max_chunk = n("10000.000001"))
        )),
        "RSK-007.5"
    );
    {
        let mut w = r0();
        let p = with_class(&w.p, 0, |b| b.rr_max_cross = r("0.009999999"));
        assert_eq!(
            w.risk.set_params(&p, AccountScope::All),
            Err(RiskParamError::ExecRiskMismatch),
            "RSK-007.6"
        );
        assert_eq!(
            w.risk.view_for(acc(1), A1, &w.nowv()).rr_max_cross,
            r("0.01"),
            "RSK-007.6 old"
        );
        let p = with_acct(&w.p, 1, |a| a.max_order_notional = n("9999.999999"));
        assert_eq!(
            w.risk.set_params(&p, AccountScope::Account(acc(1))),
            Err(RiskParamError::ExecRiskMismatch),
            "RSK-007.7"
        );
        assert_eq!(
            w.risk.view(acc(1), &w.nowv()).max_order_notional,
            n("10000"),
            "RSK-007.7 old"
        );
    }
    assert!(
        mm(new_risk(
            rd,
            &p0,
            &ex_with(3, &|b| b.force_max_cross = r("0.010000001"))
        )),
        "RSK-007.8"
    );
    {
        // Account 2 trades a2 (class 1); account 1's bound is kept within the new minimum so
        // only account 2 fails.
        let p = with_class(&p0, 1, |b| b.rr_max_cross = r("0.005"));
        let mut e = exec_bounds(3);
        e[1].force_max_cross = r("0.005");
        e[3].force_max_cross = r("0.005");
        assert!(mm(new_risk(rd, &p, &e)), "RSK-007.9");
        e[2].force_max_cross = r("0.005");
        assert!(new_risk(rd, &p, &e).is_ok(), "RSK-007.9 control");
    }
    let _ = AccountId(0);
}
