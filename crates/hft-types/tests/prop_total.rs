//! Every public function returns for any argument (TYP-157).

#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods
)] // allow-list: test code, not a hot path

use hft_types::*;
use proptest::prelude::*;

fn mode() -> impl Strategy<Value = RoundMode> {
    prop::sample::select(vec![
        RoundMode::Floor,
        RoundMode::Ceil,
        RoundMode::TowardZero,
        RoundMode::HalfEven,
    ])
}

fn side() -> impl Strategy<Value = Side> {
    prop::sample::select(vec![Side::Buy, Side::Sell])
}

/// Any i64, biased towards the interesting edges.
fn int() -> impl Strategy<Value = i64> {
    prop_oneof![
        any::<i64>(),
        prop::sample::select(vec![
            0,
            1,
            -1,
            MAX_RAW,
            -MAX_RAW,
            MAX_RAW + 1,
            -MAX_RAW - 1,
            i64::MAX,
            i64::MIN
        ]),
    ]
}

fn legal(x: i64) -> bool {
    x.unsigned_abs() <= MAX_RAW as u64
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(20_000))]

    #[test]
    fn typ_157_no_panic_on_any_input(
        a in int(),
        b in int(),
        c in int(),
        r in int(),
        e1 in any::<i8>(),
        e2 in any::<i8>(),
        m in mode(),
        s in side(),
        u1 in any::<u32>(),
        u2 in any::<u32>(),
        b8 in any::<u8>(),
        limbs in any::<[u64; 4]>(),
        bytes in proptest::collection::vec(any::<u8>(), 0..48),
    ) {
        let e = AssetExp { px_exp: e1, qty_exp: e2 };
        let _ = Px(a).mul_qty(Qty(b), e, m);
        let _ = Px::from_notional(Notional(a), Qty(b), e, m);
        let p = Px(a).mul_rate(Rate(r), m);
        let _ = Notional(a).mul_rate(Rate(r), m);
        let t = Px(a).round_to_tick(b, m);
        let l = Qty(a).round_to_lot(b, m);
        let (x, y) = Qty(a).split_lot(b);
        let mid = Px::mid_floor(Px(a), Px(b));
        let _ = Px(a).ratio(Px(b));
        let pl = pair_lot(a, b, u1, u2);
        let lim = HedgePxLimits { conn_liveness_ms: u1, rr_max_cross: Rate(r) };
        let cc = cap_cross_px(s, Px(a), Px(b), Rate(c), &lim, b8 as i64 + 1);
        let cc2 = cap_cross_px(s, Px(a), Px(b), Rate(c), &lim, c);
        let _ = funding_settle_norm_ns(a, u1, u2);
        let _ = parse_exact(&bytes, e1);
        let _ = parse_round(&bytes, e1, m);
        let mut out = [0u8; 40];
        let _ = write_fixed(a, e1, &mut out);
        let _ = qty_from_u256(&limbs, b8, e1, m);
        let _ = qty_to_u256(Qty(a), b8, e1);
        let _ = GroupId::compose(StrategyId(b8), u1);
        let _ = ClientOrderId::decode(&bytes);

        // Px/Qty results stay within ±MAX_RAW when the inputs satisfy invariant 1.
        if legal(a) {
            prop_assert!(legal(p.0));
            prop_assert!(legal(t.0));
            prop_assert!(legal(l.0));
            prop_assert!(legal(x.0) && legal(y.0));
            if legal(b) {
                prop_assert!(legal(mid.0));
                prop_assert!(legal(cc.0) && legal(cc2.0));
            }
        }
        if let Some(q) = pl {
            prop_assert!(legal(q.0));
        }
    }
}
