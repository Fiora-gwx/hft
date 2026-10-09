//! Event builders covering every non-chain `EventRef` variant (SDK-014, SDK-179).

use hft_event::ConnStateKind;
use hft_strategy_sdk::testing::ev::{self, Ev};
use hft_strategy_sdk::*;
use hft_types::{AssetId, GroupId, INST_ALL, Px, Qty, Rate, RejectBits, RunMode, Side, Tsc};

use super::{A1, BandAdvice, COOLDOWN_TAG, I1, I2, V1, quote, warm_chunk};

/// Number of non-chain `EventRef` variants `variant` covers.
pub const N_VARIANTS: u8 = 18;

/// Group id of gix `gix` in tables built from `G1, G2, …` (`0x0300_0001 + gix`).
pub fn gid(gix: u16) -> GroupId {
    GroupId(0x0300_0001 + u32::from(gix))
}

/// Event of variant `k % N_VARIANTS` for `gix`, parameterized by `x`; `now` stamps Advice.
pub fn variant(k: u8, gix: u16, x: i64, now: Now) -> Ev {
    let g = gid(gix);
    let side = if x % 2 == 0 { Side::Buy } else { Side::Sell };
    match k % N_VARIANTS {
        0 => ev::bbo(I1, gix, quote(6_500_000 + x % 40_000)),
        1 => ev::book(I1, gix),
        2 => ev::trade(
            I1,
            gix,
            TradeView {
                px: Px(6_500_000 + x % 100),
                qty: Qty(1_000),
                side,
                ex_ts_ns: x,
            },
        ),
        3 => ev::funding(
            I2,
            gix,
            FundingView {
                rate: Rate(x % 1_000_000),
                interval_s: 28_800,
                ..FundingView::EMPTY
            },
        ),
        4 => ev::mark(I2, gix, Px(6_500_000 + x % 1_000)),
        5 => ev::index(I2, gix, Px(6_500_000 - x % 1_000)),
        6 => ev::liquidation(
            I2,
            gix,
            LiquidationView {
                px: Px(6_400_000),
                qty: Qty(500),
                side,
                ex_ts_ns: x,
            },
        ),
        7 => {
            let kind = if x % 3 == 0 {
                OrderUpdateKind::Rejected {
                    bits: RejectBits(1),
                    venue_code: 0,
                }
            } else {
                OrderUpdateKind::Filled {
                    d: Qty(x % 5_000),
                    px: Px(6_500_000),
                    fee: 0,
                    fee_asset: AssetId(3),
                    cum: Qty(x % 5_000),
                    done: x % 2 == 0,
                }
            };
            ev::order(
                gix,
                OrderUpdate {
                    r: OrderRef::new(V1, A1, (x.unsigned_abs() % 64) as u32, 1),
                    group: g,
                    tag: 0,
                    kind,
                },
            )
        }
        8 => ev::exec(
            gix,
            ExecUpdate {
                group: g,
                tag: 0,
                origin: TargetOrigin::Strategy,
                kind: ExecUpdateKind::Progress {
                    leg_ix: 0,
                    d: Qty(x % 1_000),
                },
                target: [Qty(0); 4],
                filled: [Qty(0); 4],
                inflight: [Qty(0); 4],
            },
        ),
        9 => ev::external(
            gix,
            ExternalFillView {
                venue: V1,
                inst: I1,
                group: g,
                leg_ix: 0,
                side,
                qty: Qty(100),
                px: Px(6_500_000),
                fee: 0,
                fee_asset: AssetId(3),
                cause: hft_event::ExternalCause::Manual,
                ex_ts_ns: x,
                deadline_tsc: Tsc(0),
            },
        ),
        10 => ev::risk(A1, RiskEvent::HedgeBroken { group: g }),
        11 => ev::notice(
            I2,
            gix,
            VenueNoticeView {
                venue: V1,
                inst: INST_ALL,
                kind: NoticeKind::FundingInterval { new_s: 14_400 },
            },
        ),
        12 => ev::timer(COOLDOWN_TAG, g, gix),
        13 => {
            let a = BandAdvice {
                flags: (x.unsigned_abs() % 2) as u8,
                band: x,
            };
            ev::advice(gix, &a, now.boot_seq, now.tsc)
        }
        14 => {
            let new = if x % 2 == 0 {
                RunMode::CloseOnly
            } else {
                RunMode::Live
            };
            ev::mode(gix, RunMode::Live, new)
        }
        15 => ev::conn(
            V1,
            if x % 2 == 0 {
                ConnStateKind::Down
            } else {
                ConnStateKind::Up
            },
        ),
        16 => {
            let op = (x.unsigned_abs() % 3) as u8 + 1;
            ev::manual_at(gix, op, &[x.rem_euclid(3)], op == 2)
        }
        _ => {
            let s: Vec<(i64, i64)> = (0..(x.unsigned_abs() % 8) as i64)
                .map(|k| (x * 1_000_000_000 + k, 6_500_000 + k))
                .collect();
            ev::warmup(1, 0, true, &warm_chunk(g, &s))
        }
    }
}
