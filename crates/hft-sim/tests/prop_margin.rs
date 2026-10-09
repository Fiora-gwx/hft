//! SIM-073: a single position is liquidated exactly at the reported liquidation price.

#![allow(clippy::too_many_arguments, clippy::type_complexity)]

use crate::common;

use common::*;
use hft_event::{MarginItem, RepMsg};
use hft_refdata::RefData;
use hft_sim::{MmrSource, SimSetup};
use hft_types::{Px, Qty};
use proptest::prelude::*;
use zerocopy::FromBytes;

struct Env {
    flat_rd: RefData,
    tier_rd: RefData,
}

fn env() -> &'static Env {
    static E: std::sync::OnceLock<Env> = std::sync::OnceLock::new();
    E.get_or_init(|| Env {
        flat_rd: rd_sim(),
        tier_rd: rd_with(|s| s.mmr = btc_p2_tiers()),
    })
}

fn sim(setup: &SimSetup, rd: &RefData, wallet: Qty, qty: Qty, entry: Px) -> Sim {
    let mut s = Sim::with(setup, rd);
    s.seed(
        A9,
        &hft_sim::SimSeedState {
            balances: vec![(P2, USDT, wallet)],
            positions: vec![(BTC_P2, qty, entry)],
            orders: Vec::new(),
            funding: Vec::new(),
        },
    );
    s.mark_ex(0, 0, BTC_P2, entry);
    s.drain(ms(10));
    s
}

fn liquidated(s: &Sim) -> bool {
    s.out.iter().any(|r| matches!(r.msg, RepMsg::ExternalFill(_)))
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(2_000))]
    #[test]
    fn sim_073_liq_px_triggers_exactly(
        wallet_usdt in 1_000i64..=100_000,
        lots in 1i64..=10_000,
        long in any::<bool>(),
        entry_tenth in 100_000i64..=1_000_000,
        tiered in any::<bool>(),
        flat_mmr in 1_000_000i64..=50_000_000,
    ) {
        let e = env();
        let mut setup = setup_sim();
        setup.margin_models[0].mmr = if tiered { MmrSource::RefdataTiers } else { MmrSource::Flat(hft_types::Rate(flat_mmr)) };
        let rd = if tiered { &e.tier_rd } else { &e.flat_rd };
        let wallet = Qty(wallet_usdt * 1_000_000);
        let qty = Qty(if long { lots } else { -lots } * 100_000);
        let entry = Px(entry_tenth * 1_000);
        let s0 = sim(&setup, rd, wallet, qty, entry);
        prop_assume!(!liquidated(&s0));
        let l = s0.view(A9).liq_px(BTC_P2);
        prop_assume!(l.0 > 0);
        // The MarginReport item carries the same price.
        let item = s0.out.iter().rev().find_map(|r| match r.msg {
            RepMsg::MarginReport(_) => r.bulk.chunks_exact(16).find_map(|c| MarginItem::read_from_bytes(c).ok()),
            _ => None,
        });
        prop_assert_eq!(item.map(|i| i.liq_px), Some(l));
        let mut at = sim(&setup, rd, wallet, qty, entry);
        at.mark_ex(ms(20), ms(20) as i64, BTC_P2, l);
        at.drain(ms(30));
        prop_assert!(liquidated(&at), "mark = L must liquidate");
        let near = if long { Px(l.0 + 1) } else { Px(l.0 - 1) };
        let mut off = sim(&setup, rd, wallet, qty, entry);
        off.mark_ex(ms(20), ms(20) as i64, BTC_P2, near);
        off.drain(ms(30));
        prop_assert!(!liquidated(&off), "one price unit short of L must not liquidate");
    }
}
