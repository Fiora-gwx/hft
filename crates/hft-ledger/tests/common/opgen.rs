//! Random operation sequences for the property tests: account `A1`, groups `G1` (BTC/S1 +
//! BTC/P2), `G3` (ETH/P2), `G4` (BNB/S1); one `fee_asset_rule` and one `funding_settle` per case.

use std::sync::OnceLock;

use hft_event::ExternalCause;
use hft_ledger::{
    AdjustDelta, AdjustKind, FixedVec, FundingReconItem, Ledger, LedgerConfig, TransferDelta, TransferKind,
};
use hft_refdata::{FeeAssetRule, FundingSettleKind, RefData};
use hft_types::{ASSET_NONE, AssetId, GROUP_NONE, INST_NONE, Notional, Px, Qty, Rate};
use proptest::prelude::*;

use super::*;

pub const KINDS: [FundingSettleKind; 3] = [
    FundingSettleKind::PerSymbolPush,
    FundingSettleKind::BalanceOnlyPush,
    FundingSettleKind::RateOnly,
];

pub fn fee_rule(ix: usize) -> FeeAssetRule {
    match ix {
        0 => FeeAssetRule::Quote,
        1 => FeeAssetRule::ReceivedAsset,
        2 => FeeAssetRule::ReceivedAssetUnlessDiscount { token: BNB },
        _ => FeeAssetRule::Configured(BNB),
    }
}

/// `rd_lf` with `S1` fee rule `fee_ix` and `P2` funding `KINDS[fund_ix]` (built once each).
pub fn rd_combo(fee_ix: usize, fund_ix: usize) -> &'static RefData {
    static ALL: OnceLock<Vec<&'static RefData>> = OnceLock::new();
    let all = ALL.get_or_init(|| {
        let mut v = Vec::with_capacity(12);
        for f in 0..4 {
            for k in KINDS {
                v.push(rd_with(&RdOpts {
                    s1_fee: fee_rule(f),
                    p2_funding: k,
                    ..RdOpts::default()
                }));
            }
        }
        v
    });
    all[fee_ix * 3 + fund_ix]
}

#[derive(Clone, Debug)]
pub enum Op {
    /// `fee`: 0 none, 1 quote, 2 base asset, 3 BNB; `late`: the fee arrives as a `FeeDelta`.
    Fill {
        perp: bool,
        buy: bool,
        lots: i64,
        px: i64,
        fee: u8,
        late: bool,
        g4: bool,
    },
    Ext {
        cause: u8,
        buy: bool,
        perp: bool,
        lots: i64,
        px: i64,
        tid: u64,
    },
    /// Settle slot `k` (`D + k × 8 h`); `push` picks push vs synthesis where both exist.
    Fund {
        k: i64,
        amount: i64,
        push: bool,
    },
    Recon {
        k: i64,
        exch: i64,
        late: bool,
    },
    Transfer {
        to_perp: bool,
        amount: i64,
    },
    WriteOff {
        perp: bool,
        pct: i64,
    },
    AdoptPos {
        lots: i64,
        px: i64,
    },
    AdoptBal {
        group: bool,
        lots: i64,
        px: i64,
    },
    Baseline,
    RefPx {
        px: i64,
    },
    Mark {
        px: i64,
    },
    Report,
}

fn px_raw() -> impl Strategy<Value = i64> {
    (3_000i64..=9_000).prop_map(|k| k * 100_000)
}

pub fn op(prices: bool) -> BoxedStrategy<Op> {
    // Without price moves (LED-240) no late fees either: a late base-asset fee leaves a residual
    // against the average cost that the cumulative identity does not model.
    let fill = (
        any::<bool>(),
        any::<bool>(),
        1i64..=1_000,
        px_raw(),
        0u8..4,
        any::<bool>(),
        prop::bool::weighted(0.1),
    )
        .prop_map(move |(perp, buy, lots, px, fee, late, g4)| Op::Fill {
            perp,
            buy,
            lots,
            px,
            fee,
            late: late && prices,
            g4,
        });
    let ext = (
        0u8..5,
        any::<bool>(),
        any::<bool>(),
        1i64..=200,
        px_raw(),
        1u64..=40,
    )
        .prop_map(|(cause, buy, perp, lots, px, tid)| Op::Ext {
            cause,
            buy,
            perp,
            lots,
            px,
            tid,
        });
    let fund = (1i64..=20, -20_000_000i64..=20_000_000, any::<bool>())
        .prop_map(|(k, amount, push)| Op::Fund { k, amount, push });
    let recon = (1i64..=20, -20_000_000i64..=20_000_000, any::<bool>())
        .prop_map(|(k, exch, late)| Op::Recon { k, exch, late });
    let transfer = (any::<bool>(), 1i64..=1_000).prop_map(|(to_perp, a)| Op::Transfer {
        to_perp,
        amount: a * 1_000_000,
    });
    let wo = (any::<bool>(), 0i64..=100).prop_map(|(perp, pct)| Op::WriteOff { perp, pct });
    let ap = (-500i64..=500, px_raw()).prop_map(|(lots, px)| Op::AdoptPos { lots, px });
    let ab =
        (any::<bool>(), 1i64..=500, px_raw()).prop_map(|(group, lots, px)| Op::AdoptBal { group, lots, px });
    let base = prop_oneof![
        10 => fill,
        3 => ext,
        2 => fund,
        2 => recon,
        1 => transfer,
        1 => wo,
        1 => ap,
        1 => ab,
        1 => Just(Op::Baseline),
        1 => Just(Op::Report),
    ];
    if prices {
        prop_oneof![
            20 => base,
            2 => px_raw().prop_map(|px| Op::RefPx { px }),
            2 => px_raw().prop_map(|px| Op::Mark { px }),
        ]
        .boxed()
    } else {
        base.boxed()
    }
}

/// One property case: fee rule, funding kind and the operations.
pub fn case(prices: bool, max: usize) -> impl Strategy<Value = (usize, usize, Vec<Op>)> {
    (0usize..4, 0usize..3, prop::collection::vec(op(prices), 0..=max))
}

/// What a step called (LED-243's change sources).
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum Call {
    Fill,
    Fee,
    ExtSome,
    ExtNone,
    Funding,
    Recon,
    Transfer,
    WriteOff,
    AdoptPos,
    AdoptBal,
    Baseline,
    Price,
    Report,
}

#[derive(Clone)]
pub struct Run {
    pub l: Ledger,
    pub kind: FundingSettleKind,
    pub t: i64,
    pub mark: Px,
    /// Conversion rate of BTC (px raw; the last `RefPx`).
    pub btc_px: i64,
    /// Σ fee × (rate − fill px) of immediate base-asset fees (USDT raw): the fee is charged at the
    /// conversion rate while the traded quantity moves at the fill price.
    pub residual: i128,
    /// The last step had a late base-asset fee (residual against the average cost, not tracked).
    pub approx: bool,
    pub calls: Vec<Call>,
}

pub fn cfg_prop() -> LedgerConfig {
    let mut c = cfg_lf();
    c.valuation.spot = hft_ledger::SpotVal::Mid;
    c.valuation.include_baseline = true;
    c.valuation.include_unassigned = true;
    c
}

impl Run {
    pub fn new(fee_ix: usize, fund_ix: usize, cfg: &LedgerConfig) -> Run {
        let mut l = Ledger::new(rd_combo(fee_ix, fund_ix), cfg, &caps_lf()).expect("new");
        set_groups(&mut l, &[g4()]);
        boot_a1(&mut l, 0);
        l.on_ref_px(BTC_S1, px("60000"), px("60000"), 0);
        l.on_ref_px(BNB_S1, px("600"), px("600"), 0);
        l.on_ref_px(USDC_S1, px("1"), px("1"), 0);
        let mut touched = FixedVec::new();
        l.on_mark(BTC_P2, px("60000"), 0, &mut touched);
        l.on_ref_px(BTC_P2, px("60000"), px("60000"), 0);
        l.clear_notes();
        Run {
            l,
            kind: KINDS[fund_ix],
            t: s(1),
            mark: px("60000"),
            btc_px: 600_000_000,
            residual: 0,
            approx: false,
            calls: Vec::new(),
        }
    }

    fn fill_delta(
        &self,
        perp: bool,
        buy: bool,
        lots: i64,
        p: i64,
        fee: u8,
        g4: bool,
    ) -> hft_ledger::FillDelta {
        let side = if buy { Buy } else { Sell };
        let (inst, group, leg) = match (perp, g4) {
            (true, _) => (BTC_P2, G1, 1),
            (false, true) => (BNB_S1, G4, 0),
            (false, false) => (BTC_S1, G1, 0),
        };
        let qty = if perp { lots * 100_000 } else { lots * 1_000 };
        // qty (1e-8) × px (1e-4) = 1e-12 → USDT raw (1e-6).
        let notional_usdt = i128::from(qty) * i128::from(p) / 1_000_000;
        let (fee_raw, fee_asset) = match fee {
            1 => ((notional_usdt / 1_000).max(1) as i64, USDT),
            2 if !perp => ((qty / 1_000).max(1), base_of(inst)),
            3 if !perp => (lots * 100, BNB),
            0 => (0, ASSET_NONE),
            _ => ((notional_usdt / 1_000).max(1) as i64, USDT),
        };
        let q = Qty(qty);
        hft_ledger::FillDelta {
            account: A1,
            venue: venue_of(inst),
            side,
            class: hft_ledger::FillClass::Open as u8,
            flags: 0,
            leg_ix: leg,
            _pad: [0; 2],
            inst,
            group,
            qty: q,
            net_qty: net_qty(side, inst, q, fee_raw, fee_asset),
            px: Px(p),
            fee: fee_raw,
            fee_asset,
            _pad2: 0,
            slot: 0,
            r#gen: 0,
            _pad3: 0,
            seq: 0,
        }
    }

    /// Applies `op` one second after the previous step; returns the calls it made.
    pub fn step(&mut self, op: &Op) -> Vec<Call> {
        let t = self.t + s(1);
        self.step_at(op, t)
    }

    /// Applies `op` at engine time `t` (not before the previous step).
    pub fn step_at(&mut self, op: &Op, t: i64) -> Vec<Call> {
        self.t = t;
        self.approx = false;
        let mut calls = Vec::new();
        match *op {
            Op::Fill {
                perp,
                buy,
                lots,
                px,
                fee,
                late,
                g4,
            } => {
                let mut f = self.fill_delta(perp, buy, lots, px, fee, g4);
                let (fee_raw, fee_asset) = (f.fee, f.fee_asset);
                let base = base_of(f.inst);
                if fee_asset == base && !f.venue.eq(&P2) {
                    if late {
                        self.approx = true;
                    } else {
                        let rate = if base == BNB { 6_000_000 } else { self.btc_px };
                        self.residual += i128::from(fee_raw) * i128::from(rate - f.px.0) / 1_000_000;
                    }
                }
                if late && fee_asset != ASSET_NONE {
                    f.fee = 0;
                    f.fee_asset = ASSET_NONE;
                    f.net_qty = f.qty;
                }
                self.l.apply_fill(&f, t);
                calls.push(Call::Fill);
                if late && fee_asset != ASSET_NONE {
                    let d = hft_ledger::FeeDelta {
                        account: A1,
                        venue: f.venue,
                        inst: f.inst,
                        group: f.group,
                        side: f.side,
                        fee: fee_raw,
                        fee_asset,
                        seq: 0,
                    };
                    self.l.apply_fee(&d, t);
                    calls.push(Call::Fee);
                }
            }
            Op::Ext {
                cause,
                buy,
                perp,
                lots,
                px,
                tid,
            } => {
                let causes = [
                    ExternalCause::Liquidation,
                    ExternalCause::Adl,
                    ExternalCause::Manual,
                    ExternalCause::Delivery,
                    ExternalCause::Unknown,
                ];
                let inst = if perp { BTC_P2 } else { BTC_S1 };
                let qty = if perp { lots * 100_000 } else { lots * 1_000 };
                let mut e = ext(
                    causes[usize::from(cause) % 5],
                    if buy { Buy } else { Sell },
                    inst,
                    "0",
                    "0",
                );
                e.qty = Qty(qty);
                e.px = Px(px);
                e.trade_id = tid;
                let r = self.l.apply_external(&e, false, t);
                calls.push(if r.is_some() { Call::ExtSome } else { Call::ExtNone });
            }
            Op::Fund {
                k,
                amount,
                push: is_push,
            } => {
                let ts = D + k * 8 * H;
                match self.kind {
                    FundingSettleKind::PerSymbolPush => {
                        let mut x = fs(BTC_P2, "0", ts, 0);
                        x.amount = Notional(amount);
                        self.l.apply_funding(&x, t);
                    }
                    FundingSettleKind::BalanceOnlyPush if is_push => {
                        let mut x = fs(INST_NONE, "0", ts, 0);
                        x.amount = Notional(amount);
                        self.l.apply_funding_push(&x, t);
                    }
                    _ => {
                        let r = Rate(amount / 100);
                        if let Some(x) = self.l.synth_funding(A1, BTC_P2, self.mark, r, ts) {
                            self.l.apply_funding(&x, t);
                        }
                    }
                }
                calls.push(Call::Funding);
            }
            Op::Recon { k, exch, late } => {
                let ts = D + k * 8 * H;
                let inst = if self.kind == FundingSettleKind::BalanceOnlyPush {
                    INST_NONE
                } else {
                    BTC_P2
                };
                let it = FundingReconItem {
                    account: A1,
                    venue: P2,
                    inst,
                    settle_ts_ns: ts,
                    exch_amount: Notional(exch),
                    engine_amount: Notional(0),
                    report_id: k as u64,
                };
                let now = if late { ts + H } else { ts + 60_000_000_000 };
                self.l.reconcile_funding(&it, now, t);
                calls.push(Call::Recon);
            }
            Op::Transfer { to_perp, amount } => {
                let (from, to) = if to_perp { (S1, P2) } else { (P2, S1) };
                self.l.apply_transfer(
                    &TransferDelta {
                        account: A1,
                        from_venue: from,
                        to_venue: to,
                        asset: USDT,
                        amount: Qty(amount),
                        kind: TransferKind::Record,
                    },
                    t,
                );
                calls.push(Call::Transfer);
            }
            Op::WriteOff { perp, pct } => {
                let (inst, v, asset, leg) = if perp {
                    (BTC_P2, P2, ASSET_NONE, 1)
                } else {
                    (BTC_S1, S1, BTC, 0)
                };
                let net = self.l.leg_net(A1, G1, leg).0;
                let q = net * pct / 100;
                self.l.apply_adjust(
                    &adjust(v, AdjustKind::WriteOff, inst, asset, G1, Qty(q), Px(0)),
                    t,
                );
                calls.push(Call::WriteOff);
            }
            Op::AdoptPos { lots, px } => {
                let a = adjust(
                    P2,
                    AdjustKind::Position,
                    BTC_P2,
                    AssetId(0),
                    G1,
                    Qty(lots * 100_000),
                    Px(px),
                );
                self.l.apply_adjust(&a, t);
                calls.push(Call::AdoptPos);
            }
            Op::AdoptBal { group, lots, px } => {
                let g = if group { G1 } else { GROUP_NONE };
                let q = lots * 1_000;
                let a = adjust(S1, AdjustKind::Balance, BTC_S1, BTC, g, Qty(q), Px(px));
                self.l.apply_adjust(&a, t);
                calls.push(Call::AdoptBal);
            }
            Op::Baseline => {
                let mut out = FixedVec::new();
                self.l.set_balance_baseline(A1, S1, 1, &mut out);
                calls.push(Call::Baseline);
            }
            Op::RefPx { px } => {
                self.l.on_ref_px(BTC_S1, Px(px), Px(px), t);
                self.btc_px = px;
                calls.push(Call::Price);
            }
            Op::Mark { px } => {
                let mut touched = FixedVec::new();
                self.l.on_mark(BTC_P2, Px(px), t, &mut touched);
                self.l.on_ref_px(BTC_P2, Px(px), Px(px), t);
                self.mark = Px(px);
                calls.push(Call::Price);
            }
            Op::Report => {
                let c = cash(&self.l, A1, S1, BTC);
                self.l
                    .on_balance(&bal(A1, S1, BTC, Qty(c.total.0.max(0)), Qty(0), D + t), true, t);
                calls.push(Call::Report);
            }
        }
        self.l.clear_notes();
        self.calls.extend_from_slice(&calls);
        calls
    }
}

pub fn adjust(
    v: hft_types::VenueId,
    kind: AdjustKind,
    inst: hft_types::InstId,
    asset: AssetId,
    g: hft_types::GroupId,
    qty: Qty,
    p: Px,
) -> AdjustDelta {
    AdjustDelta {
        account: A1,
        venue: v,
        kind: kind as u8,
        px_src: if kind == AdjustKind::WriteOff { 0 } else { 1 },
        inst,
        asset,
        _pad: 0,
        group: g,
        qty,
        px: p,
        report_id: 1,
        cmd_seq: 1,
        settle_ts_ns: 0,
    }
}

/// Property-test case count: `PROPTEST_CASES` (nightly) or the PR default.
pub fn cases(pr: u32) -> u32 {
    std::env::var("PROPTEST_CASES")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(pr)
}

/// Base-asset invariant `total = baseline + unassigned + Σ inv` on every base asset of `v`.
pub fn base_invariant(l: &Ledger, v: hft_types::VenueId) -> Result<(), String> {
    for e in l.venue_book(A1, v).cash.iter() {
        // USDT is the quote and margin asset; every other cash asset of rd_lf is a base asset.
        if e.asset == USDT {
            continue;
        }
        let inv: i64 = l
            .venue_book(A1, v)
            .inv
            .iter()
            .filter(|x| x.asset == e.asset)
            .map(|x| x.qty.0)
            .sum();
        if e.total.0 != e.baseline.0 + e.unassigned.0 + inv {
            return Err(format!(
                "{:?}: total {} != {} + {} + {}",
                e.asset, e.total.0, e.baseline.0, e.unassigned.0, inv
            ));
        }
    }
    Ok(())
}
