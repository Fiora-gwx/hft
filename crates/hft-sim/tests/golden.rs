//! SIM-123, SIM-125: golden replays of `fixtures/sim/<case>/` (fixtures/README.md "模拟撮合金样").
//! `SIM_GOLDEN_BLESS=1` rewrites the expected files (they are then reviewed by hand against
//! the hft-sim rules before being committed).

#![allow(clippy::too_many_arguments, clippy::type_complexity)]

mod common;

use std::path::{Path, PathBuf};

use common::*;
use hft_event::{
    Bbo, Funding, GwCmd, GwOp, L2SnapHdr, L2SnapshotPart, Level, MarginItem, MarkPrice, RepMsg, Trade,
};
use hft_refdata::{FeeAssetRule, FundingSettleKind};
use hft_sim::{Dist, LatencyModel, MarketEventRef, MmrSource, SimExchange, SimInput, SimSeedState};
use hft_types::{AccountId, AssetId, InstId, Px, Qty, Side, SlotId, Tif, Tsc};
use serde_json::Value;
use zerocopy::FromBytes;

fn dir(case: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/sim")
        .join(case)
}

fn read(p: &Path) -> String {
    std::fs::read_to_string(p).unwrap_or_else(|e| panic!("{}: {e}", p.display()))
}

fn inst_of(s: &str) -> InstId {
    match s {
        "BTC/S1" => BTC_S1,
        "BNB/S1" => BNB_S1,
        "BTC/P2" => BTC_P2,
        "ETH/P2" => ETH_P2,
        _ => panic!("unknown inst {s}"),
    }
}

fn inst_name(i: InstId) -> &'static str {
    match i.0 {
        11 => "BTC/S1",
        12 => "BNB/S1",
        21 => "BTC/P2",
        22 => "ETH/P2",
        0 => "-",
        _ => "?",
    }
}

fn asset_name(a: AssetId) -> &'static str {
    match a.0 {
        1 => "USDT",
        2 => "BTC",
        3 => "ETH",
        4 => "BNB",
        _ => "-",
    }
}

fn asset_exp(a: AssetId) -> i8 {
    if a == USDT { -6 } else { -8 }
}

fn d(raw: i64, exp: i8) -> String {
    let mut b = [0u8; 40];
    let n = hft_types::write_fixed(raw, exp, &mut b);
    String::from_utf8_lossy(&b[..n]).into_owned()
}

fn s<'a>(v: &'a Value, k: &str) -> &'a str {
    v.get(k)
        .and_then(Value::as_str)
        .unwrap_or_else(|| panic!("missing string {k} in {v}"))
}

fn u(v: &Value, k: &str) -> u64 {
    v.get(k)
        .and_then(Value::as_u64)
        .unwrap_or_else(|| panic!("missing integer {k} in {v}"))
}

fn levels(v: &Value, k: &str) -> Vec<Level> {
    v.get(k)
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .map(|l| Level {
                    px: px(l[0].as_str().unwrap_or("0")),
                    qty: q(l[1].as_str().unwrap_or("0")),
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Canonical one-line rendering of a reply (expected.jsonl).
fn render(acct: AccountId, due: u64, m: &RepMsg, bulk: &[u8]) -> String {
    let head = format!("{{\"due\":{due},\"acct\":{}", acct.0);
    let body = match m {
        RepMsg::OrderEvent(e) => {
            let (pe, qe) = (-4, -8);
            format!(
                "\"ev\":\"OrderEvent\",\"what\":\"{:?}\",\"slot\":{},\"inst\":\"{}\",\"px\":\"{}\",\"qty\":\"{}\",\"cum\":\"{}\",\"avg\":\"{}\",\"fee\":\"{}\",\"fee_asset\":\"{}\",\"code\":{},\"detail\":{},\"ex_ts\":{}",
                e.what,
                e.slot.0,
                inst_name(e.inst),
                d(e.px.0, pe),
                d(e.qty.0, qe),
                d(e.cum_filled.0, qe),
                d(e.avg_px.0, pe),
                d(e.fee.0, asset_exp(e.fee_asset)),
                asset_name(e.fee_asset),
                e.reject_code,
                e.detail,
                e.ex_ts_ns
            )
        }
        RepMsg::SendReport(x) => format!(
            "\"ev\":\"SendReport\",\"status\":\"{:?}\",\"slot\":{}",
            x.status, x.slot.0
        ),
        RepMsg::PositionReport(p) => format!(
            "\"ev\":\"PositionReport\",\"inst\":\"{}\",\"qty\":\"{}\",\"entry\":\"{}\",\"liq\":\"{}\"",
            inst_name(p.inst),
            d(p.qty.0, -8),
            d(p.entry_px.0, -4),
            d(p.liq_px.0, -4)
        ),
        RepMsg::BalanceReport(b) => format!(
            "\"ev\":\"BalanceReport\",\"venue\":{},\"asset\":\"{}\",\"free\":\"{}\",\"locked\":\"{}\"",
            b.venue.0,
            asset_name(b.asset),
            d(b.free.0, asset_exp(b.asset)),
            d(b.locked.0, asset_exp(b.asset))
        ),
        RepMsg::FundingSettle(f) => format!(
            "\"ev\":\"FundingSettle\",\"inst\":\"{}\",\"amount\":\"{}\",\"rate\":\"{}\",\"pos\":\"{}\",\"settle_ts\":{}",
            inst_name(f.inst),
            d(f.amount.0, -6),
            d(f.rate.0, -9),
            d(f.pos_qty.0, -8),
            f.settle_ts_ns
        ),
        RepMsg::ConnState(c) => format!(
            "\"ev\":\"ConnState\",\"venue\":{},\"state\":\"{:?}\"",
            c.venue.0, c.state
        ),
        RepMsg::ExternalFill(f) => format!(
            "\"ev\":\"ExternalFill\",\"cause\":\"{:?}\",\"side\":\"{:?}\",\"inst\":\"{}\",\"qty\":\"{}\",\"px\":\"{}\",\"fee\":\"{}\",\"fee_asset\":\"{}\"",
            f.cause,
            f.side,
            inst_name(f.inst),
            d(f.qty.0, -8),
            d(f.px.0, -4),
            d(f.fee.0, asset_exp(f.fee_asset)),
            asset_name(f.fee_asset)
        ),
        RepMsg::AccountConfigReport(h) => {
            format!(
                "\"ev\":\"AccountConfigReport\",\"venue\":{},\"n_lev\":{},\"mismatches\":{}",
                h.venue.0, h.n_lev, h.mismatches
            )
        }
        RepMsg::MarginReport(h) => {
            let items: Vec<String> = bulk
                .chunks_exact(16)
                .filter_map(|c| MarginItem::read_from_bytes(c).ok())
                .map(|i| format!("[\"{}\",\"{}\"]", inst_name(i.inst), d(i.liq_px.0, -4)))
                .collect();
            format!(
                "\"ev\":\"MarginReport\",\"mb\":\"{}\",\"maint\":\"{}\",\"avail\":\"{}\",\"ratio\":\"{}\",\"items\":[{}]",
                d(h.margin_balance.0, -6),
                d(h.maint_margin.0, -6),
                d(h.avail_margin.0, -6),
                d(h.margin_ratio.0, -9),
                items.join(",")
            )
        }
    };
    format!("{head},{body}}}")
}

struct Out {
    lines: Vec<String>,
    bin: Vec<u8>,
    hashes: Vec<String>,
}

/// Runs `fixtures/sim/<case>/`.
fn run_case(case: &str) -> Out {
    let p = dir(case);
    let setup_t: toml::Table = read(&p.join("setup.toml"))
        .parse()
        .unwrap_or_else(|e| panic!("{case}: {e}"));
    let get = |k: &str| setup_t.get(k).and_then(|v| v.as_str()).unwrap_or("");
    let rd = rd_with(|s| {
        s.caps_mut(S1).fee_asset_rule = match get("s1_fee_rule") {
            "ReceivedAsset" => FeeAssetRule::ReceivedAsset,
            "DiscountBNB" => FeeAssetRule::ReceivedAssetUnlessDiscount { token: BNB },
            "ConfiguredBNB" => FeeAssetRule::Configured(BNB),
            _ => FeeAssetRule::Quote,
        };
        s.caps_mut(P2).funding_settle = match get("p2_funding_settle") {
            "BalanceOnlyPush" => FundingSettleKind::BalanceOnlyPush,
            "RateOnly" => FundingSettleKind::RateOnly,
            _ => FundingSettleKind::PerSymbolPush,
        };
        if get("mmr") == "tiers" {
            s.mmr = btc_p2_tiers();
        }
    });
    let mut setup = setup_sim();
    if get("latency") == "empirical" {
        let dd = Dist::from_points(&[(0, 800), (5_000, 2_000), (9_500, 6_000), (10_000, 15_000)]);
        setup.latency_models = vec![LatencyModel::Empirical { entry: dd, resp: dd }];
    }
    if get("mmr") == "tiers" {
        setup.margin_models[0].mmr = MmrSource::RefdataTiers;
    }
    let end = setup_t
        .get("end_ms")
        .and_then(|v| v.as_integer())
        .unwrap_or(10_000) as u64;
    let mut ex = SimExchange::new(&setup, &rd).unwrap_or_else(|e| panic!("{case}: {e:?}"));
    if let Some(seed) = setup_t.get("seed").and_then(|v| v.as_table()) {
        let wallet = seed.get("wallet").and_then(|v| v.as_str()).unwrap_or("0");
        let positions = seed
            .get("positions")
            .and_then(|v| v.as_array())
            .map(|a| {
                a.iter()
                    .filter_map(|r| {
                        let r = r.as_array()?;
                        Some((inst_of(r[0].as_str()?), q(r[1].as_str()?), px(r[2].as_str()?)))
                    })
                    .collect()
            })
            .unwrap_or_default();
        let st = SimSeedState {
            balances: vec![(S1, USDT, usdt("100000")), (P2, USDT, usdt(wallet))],
            positions,
            orders: vec![],
            funding: vec![],
        };
        ex.seed_account_from(A9, &st).unwrap_or_else(|e| panic!("{e:?}"));
    }
    let recs: Vec<Value> = read(&p.join("records.jsonl"))
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).unwrap_or_else(|e| panic!("{case} record {l}: {e}")))
        .collect();
    let cmds: Vec<Value> = read(&p.join("commands.jsonl"))
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).unwrap_or_else(|e| panic!("{case} command {l}: {e}")))
        .collect();
    let mut out = Out {
        lines: Vec::new(),
        bin: Vec::new(),
        hashes: Vec::new(),
    };
    let mut ci = 0usize;
    let mut slot = 0u32;
    let mut snap_id = 0u64;
    let feed_cmd = |ex: &mut SimExchange, c: &Value, slot: &mut u32| {
        let t = u(c, "decided_tsc");
        let a = AccountId(u(c, "account") as u8);
        let op = s(c, "op");
        let cmd = match op {
            "SimTransfer" => GwCmd::sim_transfer(a, S1, P2, USDT, usdt(s(c, "amount"))),
            "SimReset" => GwCmd::sim_reset(a),
            _ => {
                let op = match op {
                    "Place" => GwOp::Place,
                    "Cancel" => GwOp::Cancel,
                    "Query" => GwOp::Query,
                    _ => panic!("op {op}"),
                };
                let target = c.get("target").and_then(Value::as_u64).unwrap_or(0) as u32;
                if op == GwOp::Place {
                    *slot += 1;
                }
                GwCmd {
                    decision_seq: 0,
                    decided_tsc: Tsc(t),
                    px: c.get("px").and_then(Value::as_str).map_or(Px(0), px),
                    qty: c.get("qty").and_then(Value::as_str).map_or(Qty(0), q),
                    inst: c.get("inst").and_then(Value::as_str).map_or(InstId(0), inst_of),
                    slot: SlotId(if op == GwOp::Place { *slot } else { target }),
                    r#gen: 1,
                    target_slot: SlotId(target),
                    op,
                    account: a,
                    side: if c.get("side").and_then(Value::as_str) == Some("Sell") {
                        Side::Sell
                    } else {
                        Side::Buy
                    },
                    tif: match c.get("tif").and_then(Value::as_str) {
                        Some("Gtc") => Tif::Gtc,
                        Some("Fok") => Tif::Fok,
                        Some("PostOnly") => Tif::PostOnly,
                        _ => Tif::Ioc,
                    },
                    oflags: 0,
                    _pad: [0; 2],
                    arg: 0,
                    arg2: 0,
                }
            }
        };
        ex.on_command(&cmd, Tsc(t));
    };
    let collect = |ex: &mut SimExchange, now: u64, out: &mut Out| {
        ex.drain_due(Tsc(now), |r| {
            out.lines.push(render(r.account, r.due_tsc.0, &r.msg, r.bulk));
            out.bin
                .extend_from_slice(&common::scen::reply_bytes(r.account, r.due_tsc.0, &r.msg, r.bulk));
        });
        out.hashes.push(format!("{:016x}", ex.state_hash()));
    };
    collect(&mut ex, 0, &mut out);
    for r in &recs {
        let t = u(r, "tsc");
        while ci < cmds.len() && u(&cmds[ci], "decided_tsc") < t {
            feed_cmd(&mut ex, &cmds[ci], &mut slot);
            ci += 1;
            collect(&mut ex, 0, &mut out);
        }
        let i = inst_of(s(r, "inst"));
        let ex_ts = r.get("ex_ts_ns").and_then(Value::as_i64).unwrap_or(t as i64);
        let h = mhdr(venue_of(i), i, ex_ts);
        let hdr = rhdr(t);
        match s(r, "kind") {
            "Bbo" => {
                let b = levels(r, "bid");
                let a = levels(r, "ask");
                let x = Bbo {
                    h,
                    bid_px: b[0].px,
                    bid_qty: b[0].qty,
                    ask_px: a[0].px,
                    ask_qty: a[0].qty,
                    update_id: 0,
                };
                ex.on_record(&hdr, SimInput::Market(MarketEventRef::Bbo(&x)));
            }
            "TopN" => {
                let bids = levels(r, "bids");
                let asks = levels(r, "asks");
                snap_id += 1;
                let sh = L2SnapHdr {
                    h,
                    snap_id,
                    origin: L2SnapHdr::TOP_N,
                    _pad: [0; 3],
                    n_bids: bids.len() as u16,
                    n_asks: asks.len() as u16,
                };
                let part = L2SnapshotPart {
                    hdr: sh,
                    bids: &bids,
                    asks: &asks,
                    part: 0,
                    parts: 1,
                };
                ex.on_record(&hdr, SimInput::Market(MarketEventRef::L2Snapshot(part)));
            }
            "Trade" => {
                let x = Trade {
                    h,
                    px: px(s(r, "px")),
                    qty: q(s(r, "qty")),
                    trade_id: 1,
                    taker_side: Side::Buy,
                    _pad: [0; 7],
                };
                ex.on_record(&hdr, SimInput::Market(MarketEventRef::Trade(&x)));
            }
            "Funding" => {
                let x = Funding {
                    h,
                    rate: rate(s(r, "rate")),
                    next_funding_ns: u(r, "next_funding_ns") as i64,
                    interval_s: 0,
                    _pad: 0,
                };
                ex.on_record(&hdr, SimInput::Market(MarketEventRef::Funding(&x)));
            }
            "Mark" => {
                let x = MarkPrice {
                    h,
                    px: px(s(r, "px")),
                };
                ex.on_record(&hdr, SimInput::Market(MarketEventRef::Mark(&x)));
            }
            k => panic!("record kind {k}"),
        }
        collect(&mut ex, 0, &mut out);
    }
    while ci < cmds.len() {
        feed_cmd(&mut ex, &cmds[ci], &mut slot);
        ci += 1;
        collect(&mut ex, 0, &mut out);
    }
    ex.advance(Tsc(ms(end)));
    collect(&mut ex, u64::MAX, &mut out);
    out
}

fn bless() -> bool {
    std::env::var("SIM_GOLDEN_BLESS").is_ok_and(|v| v == "1")
}

const CASES_125: [&str; 5] = [
    "sim-125-taker-walk",
    "sim-125-fee-rules",
    "sim-125-funding-push-forms",
    "sim-125-liquidation",
    "sim-125-transfer-reset",
];

#[test]
fn sim_125_replay_fixtures() {
    for case in CASES_125 {
        let out = run_case(case);
        let text = out.lines.join("\n") + "\n";
        let path = dir(case).join("expected.jsonl");
        if bless() {
            std::fs::write(&path, &text).unwrap_or_else(|e| panic!("{e}"));
            continue;
        }
        let want = read(&path).replace("\r\n", "\n");
        assert_eq!(text, want, "{case}");
    }
}

#[test]
fn sim_123_cross_arch_golden() {
    let case = "sim-123-mixed";
    let out = run_case(case);
    let hashes = out.hashes.join("\n") + "\n";
    let p = dir(case);
    if bless() {
        std::fs::write(p.join("expected-replies.bin"), &out.bin).unwrap_or_else(|e| panic!("{e}"));
        std::fs::write(p.join("expected-state-hash.txt"), &hashes).unwrap_or_else(|e| panic!("{e}"));
        return;
    }
    let want = std::fs::read(p.join("expected-replies.bin")).unwrap_or_else(|e| panic!("{e}"));
    assert!(out.bin == want, "{case}: reply bytes differ");
    assert_eq!(
        hashes,
        read(&p.join("expected-state-hash.txt")).replace("\r\n", "\n"),
        "{case}"
    );
}
