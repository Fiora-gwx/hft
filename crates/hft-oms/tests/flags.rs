#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods
)] // allow-list: test code
#![allow(clippy::type_complexity, clippy::too_many_arguments)]
use crate::common;

use common::*;
use hft_event::{OrderAction, OrderFlags};
use hft_oms::*;
use hft_strategy_sdk::{IntentClass, LegLink, LegLinkOpt, LinkRole};
use hft_types::{Side, Tif};

#[test]
fn oms_021_slot_flags_never_on_wire() {
    let mut h = H::new();
    let mut it = buy_x();
    it.intent.tif = Tif::PostOnly;
    let s = h.place_ok(&it, 0);
    h.ack(s, 777, 1);
    let t = h.place_ok(&sell_y(), 1);
    h.ack(t, 778, 2);
    let all = SlotFlags::ALL & !SlotFlags::CANCEL_SENT & !SlotFlags::AMEND_SENT & !SlotFlags::CANCEL_HELD;
    for r in [s, t] {
        h.o.test_slot_mut(r.venue, r.account, r.slot, |sl, _| sl.sflags.0 = all);
    }
    let mut ds = Vec::new();
    h.cancel(s, 3).unwrap();
    ds.extend(
        h.decisions(OrderAction::Cancel)
            .into_iter()
            .map(|d| (d, h.slot(s).oflags)),
    );
    h.amend(t, p("60200.00"), q("0.008"), 3).unwrap();
    ds.extend(
        h.decisions(OrderAction::Amend)
            .into_iter()
            .map(|d| (d, h.slot(t).oflags)),
    );
    let n = h.place_ok(&buy_x(), 3);
    ds.extend(
        h.decisions(OrderAction::New)
            .into_iter()
            .map(|d| (d, h.slot(n).oflags)),
    );
    assert_eq!(ds.len(), 3);
    for (d, slot_flags) in ds {
        assert_eq!(d.oflags & 0xFF00, 0);
        assert_eq!(d.oflags, slot_flags);
    }
    let bits = [
        SlotFlags::CANCEL_SENT,
        SlotFlags::AMEND_SENT,
        SlotFlags::FILL_BEFORE_ACK,
        SlotFlags::VERIFY,
        SlotFlags::QUEUED,
        SlotFlags::EXEC,
        SlotFlags::SMOKE_DUE,
        SlotFlags::ACKED,
        SlotFlags::HIST_WAIT,
        SlotFlags::ALARMED,
        SlotFlags::AGED,
        SlotFlags::REST_QUERY,
        SlotFlags::CANCEL_HELD,
    ];
    assert_eq!(bits, [1, 2, 4, 8, 16, 32, 64, 128, 256, 512, 1024, 2048, 4096]);
    assert_eq!(bits.iter().fold(0u16, |a, b| a | b), SlotFlags::ALL);
}

#[test]
fn oms_022_oflags_from_hft_event() {
    let mut h = H::new();
    let mut it = intent(
        VG,
        A1,
        Z,
        Side::Sell,
        Tif::PostOnly,
        p("60000.0"),
        q("0.001"),
        g2(),
    );
    it.risk_reducing = true;
    it.reduce_only = true;
    let s = h.place_ok(&it, 0);
    let want = (OrderFlags::REDUCE_ONLY | OrderFlags::POST_ONLY | OrderFlags::RISK_REDUCING).0;
    assert_eq!(want, 0x0013);
    assert_eq!(h.slot(s).oflags, want);
    assert_eq!(h.decisions(OrderAction::New)[0].oflags, want);
}

fn hedge_req(link_id: u32, role: LinkRole, reduce_only: bool) -> HedgeRequest {
    HedgeRequest {
        link_id,
        role,
        venue: VX,
        account: A1,
        side: Side::Sell,
        reduce_only,
        inst: Y,
        group: g1(),
        strategy: hft_types::StrategyId(1),
        qty: q("0.001"),
        basis: q("0.001"),
        hedge_cap_qty: q("0.002"),
        ref_px: p("60010.0"),
        peer_venue: VS,
        peer_account: A1,
        trigger_seq: 1,
    }
}

#[test]
fn oms_245_oflags_from_classify() {
    let mut h = H::new();
    let row = |h: &mut H, it: &ApprovedIntent| -> (u16, u16) {
        let r = h.place_ok(it, 0);
        (h.slot(r).oflags, h.slot(r).sflags.0)
    };
    assert_eq!(row(&mut h, &buy_x()).0, 0, "OMS-245.1");
    let mut po = buy_x();
    po.intent.tif = Tif::PostOnly;
    assert_eq!(row(&mut h, &po).0, 0x02, "OMS-245.2");
    let (_, l1) = place_first(&mut h, q("0.012"), Tif::Ioc, plan_l1(), 0);
    assert_eq!(h.slot(h.o.link(l1).unwrap().first).oflags, 0x04, "OMS-245.3");
    let plan = plan_l1();
    let a = ApprovedIntent::hedge(
        &hedge_req(l1, LinkRole::Second, false),
        &plan,
        HedgePx::default(),
        IntentClass::Hedge,
    );
    assert_eq!(row(&mut h, &a).0, 0x14, "OMS-245.4");
    let a = ApprovedIntent::hedge(
        &hedge_req(l1, LinkRole::SecondRetry, true),
        &plan,
        HedgePx::default(),
        IntentClass::Hedge,
    );
    assert_eq!(row(&mut h, &a).0, 0x15, "OMS-245.5");
    let a = ApprovedIntent::hedge(
        &hedge_req(l1, LinkRole::Reverse, true),
        &plan,
        HedgePx::default(),
        IntentClass::Hedge,
    );
    assert_eq!(row(&mut h, &a).0, 0x15, "OMS-245.6");
    let mut fx = intent(VX, A1, Y, Side::Sell, Tif::Ioc, p("60000.0"), q("0.001"), g1());
    fx.origin = Origin::Exec;
    fx.risk_reducing = true;
    fx.intent.link = LegLinkOpt::some(LegLink {
        role: LinkRole::HedgeFix,
        ..link_l1(plan)
    });
    let (of, sf) = row(&mut h, &fx);
    assert_eq!(of, 0x18, "OMS-245.7");
    assert!(sf & SlotFlags::EXEC != 0, "OMS-245.7");
    let mut sm = buy_x();
    sm.intent.tif = Tif::PostOnly;
    sm.intent.group = hft_types::GROUP_NONE;
    sm.origin = Origin::Command;
    sm.smoke_ttl_ms = 60_000;
    assert_eq!(row(&mut h, &sm).0, 0x22, "OMS-245.8");
}
