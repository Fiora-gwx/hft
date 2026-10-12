//! Fixture generator of the hft-py pytest suite (crates/hft-py/TESTS.md "共用夹具"). It does not
//! write journals: it produces the inputs of hft-journal's test writer (`testwriter --script`,
//! README "测试写者") — refdata/engine blobs, boot-block lines and `bulk` record files — and the
//! Rust reference readings (`RUST_REF`) the tests compare against.
//!
//! pyfixture refdata <prod|sim> <out.blob>
//! pyfixture md <prod|sim> <n_per_inst> <dir>          J_MD / J_SIM: <dir>/script.txt, md.bulk
//! pyfixture all <dir>                                  J_ALL (2 segments)
//! pyfixture cfg <dir>                                  J_CFG: <dir>/script.txt, <dir>/run2/script.txt
//! pyfixture ref <cold_root> <prod|sim> <out_dir>       RUST_REF dump: <out_dir>/<Kind>.jsonl
//! pyfixture led <seed> <dir>                           J_LED20 seed: <dir>/script.txt, led<n>.bulk
//! pyfixture ledger-ref <cold_root> <prod|sim> <from_seq> <out.json>   ledger RUST_REF
//! pyfixture seq <ext | ff|sample|fills <row>> <dir>      hot-process fixtures (pyfixture/seq.rs)
//! pyfixture seq-check <cold_root> <prod|sim> <dir>      the written journal = the rig's journal
//!
//! Exit codes: 0 success, 1 usage or I/O error, 2 reference check failed.

#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods
)]

#[path = "pyfixture/seq.rs"]
mod seq;

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::exit;

use hft_event::{
    Bbo, BlobHdr, BlobKind, BlobRef, ClockStatus, CmdScope, EventKind, ExternalFill, FieldDesc, FieldTy,
    IdKind, KindInfo, Layout, MarginReportHdr, MarketHdr, OrderAction, OrderDecision, OrderEventKind, Trade,
    kind_info,
};
use hft_journal::{EndAt, InstanceName, JournalReader, Next, ReaderConfig, StartAt};
use hft_refdata::{
    AccountSpec, AccountVenueSpec, AssetClass, AssetSpec, Exchange, FeeAssetRule, FeePair, FeeSpec,
    FundingSettleKind, InstKind, InstSpec, InstStatus, InstanceSpec, MarginData, MarginMode, PositionMode,
    RdbBuilder, SimBalanceSpec, SimSpec, StaleGuardKind, TsUnit, VenueCaps, VenueSpec,
};
use hft_types::{
    AccountId, AssetExp, AssetId, GroupId, InstId, MarketKind, Notional, Px, Qty, Rate, Side, StrategyId,
    Tif, Tsc, VenueId,
};
use serde_json::{Map, Value, json};
use sha2::{Digest, Sha256};
use zerocopy::{FromZeros, IntoBytes};

/// `D` = 2026-10-01T00:00:00Z.
pub const D: i64 = 1_790_812_800_000_000_000;
const S: i64 = 1_000_000_000;
const MS: i64 = 1_000_000;

pub const BTC: AssetId = AssetId(1);
pub const ETH: AssetId = AssetId(2);
pub const USDT: AssetId = AssetId(3);
/// Spot venue `S1` (exchange binance).
pub const S1: VenueId = VenueId::BINANCE_SPOT;
/// USDT perpetual venue `P2` (exchange okx).
pub const P2: VenueId = VenueId::OKX_SWAP;
pub const I101: InstId = InstId(101);
pub const I102: InstId = InstId(102);
pub const I202: InstId = InstId(202);
/// `A1` (prod) and `A9` (sim) are both AccountId 1 of their instance's refdata (ids are dense).
pub const ACC: AccountId = AccountId(1);
/// Strategy instance `alpha`.
pub const ALPHA: StrategyId = StrategyId(1);

fn die(msg: &str) -> ! {
    eprintln!("pyfixture: {msg}");
    exit(1)
}

/// Record TSC of UTC `t` (3 GHz, calibration pair 0 ↔ D).
pub fn tsc(t: i64) -> u64 {
    (3 * (t - D)) as u64
}

// ------------------------------------------------------------------------------------- refdata

fn venue_spec(id: VenueId, exchange: Exchange, product: InstKind) -> VenueSpec {
    VenueSpec {
        id,
        exchange,
        product,
        ws_max_streams_per_conn: 50,
        ws_max_in_msgs_per_s: 10,
        ws_max_conn_age_s: 86_400,
        ws_new_conn_per_5min_ip: 300,
        app_ping_interval_s: 0,
        ts_unit: TsUnit::Ms,
        client_id_max_len: 36,
        client_id_prefix: "py-".into(),
        max_batch: 20,
        max_inflight_amend: 1,
        crossed_tolerance_default: 0,
        stale_guard: StaleGuardKind::RecvWindow,
    }
}

fn raw_str(raw: i64, exp: i32) -> String {
    if exp >= 0 {
        return raw.to_string();
    }
    let d = (-exp) as usize;
    let s = format!("{raw:0>w$}", w = d + 1);
    let (i, f) = s.split_at(s.len() - d);
    let f = f.trim_end_matches('0');
    if f.is_empty() {
        i.to_string()
    } else {
        format!("{i}.{f}")
    }
}

/// `RD_PY`: assets BTC/ETH/USDT, venues S1 (spot) and P2 (USDT perpetual), insts 101 BTC/S1,
/// 102 BTC/P2, 202 ETH/P2 (ids dense from 1: the others are delisted fillers), strategy instance
/// `alpha` (1); prod: account 1 `bn-1` (Live), sim: account 1 `sim-a` (Sim).
pub fn refdata(sim: bool, version: u64) -> Vec<u8> {
    refdata_fs(sim, version, FundingSettleKind::PerSymbolPush, 20)
}

/// `RD_PY` with the funding settlement kind of `P2` and the L2 depth of the insts given.
pub fn refdata_fs(sim: bool, version: u64, p2_funding: FundingSettleKind, l2_depth: u16) -> Vec<u8> {
    let mut b = RdbBuilder::new(version);
    b.created_utc_ns(D);
    for (id, name, class, px, qty, dust) in [
        (BTC, "BTC", AssetClass::Crypto, -2i8, -8i8, 1_000i64),
        (ETH, "ETH", AssetClass::Crypto, -2, -6, 100),
        (USDT, "USDT", AssetClass::Stable, -4, -6, 10_000),
    ] {
        b.asset(AssetSpec {
            id,
            name: name.into(),
            class,
            exp: AssetExp {
                px_exp: px,
                qty_exp: qty,
            },
            px_max: Px(1_000_000_000_000_000),
            qty_max: Qty(1_000_000_000_000_000),
            dust: Qty(dust),
        });
    }
    b.venue(venue_spec(S1, Exchange::Binance, InstKind::Spot));
    b.venue(venue_spec(P2, Exchange::Okx, InstKind::Perp));
    let mut sc = VenueCaps::none(S1);
    sc.fee_asset_rule = FeeAssetRule::Quote;
    b.caps(sc);
    let mut pc = VenueCaps::none(P2);
    pc.market = MarketKind::LinearPerp;
    pc.fee_asset_rule = FeeAssetRule::Quote;
    pc.required_position_mode = PositionMode::OneWay;
    pc.required_margin_mode = MarginMode::Cross;
    pc.funding_settle = p2_funding;
    pc.margin_data = MarginData::Poll { interval_ms: 2_000 };
    pc.reduce_only = true;
    b.caps(pc);
    for id in 1..=202u32 {
        let (venue, symbol, kind, asset, status, interval) = match id {
            101 => (
                S1,
                "BTCUSDT".to_string(),
                InstKind::Spot,
                BTC,
                InstStatus::Trading,
                0,
            ),
            102 => (
                P2,
                "BTC-USDT-SWAP".to_string(),
                InstKind::Perp,
                BTC,
                InstStatus::Trading,
                28_800,
            ),
            202 => (
                P2,
                "ETH-USDT-SWAP".to_string(),
                InstKind::Perp,
                ETH,
                InstStatus::Trading,
                28_800,
            ),
            _ => (S1, format!("F{id}"), InstKind::Spot, BTC, InstStatus::Delisted, 0),
        };
        let (px_exp, qty_exp) = if asset == ETH { (-2, -6) } else { (-2, -8) };
        let lot = 1_000;
        b.inst(InstSpec {
            id: InstId(id),
            venue,
            symbol,
            status,
            kind,
            asset,
            quote: USDT,
            tick: 1,
            lot,
            min_qty: Qty(lot),
            max_qty: Qty(1_000_000_000_000),
            min_notional: Notional(5_000_000),
            book_window: 64,
            l2_depth,
            crossed_tolerance: 0,
            tick_str: raw_str(1, px_exp),
            lot_str: raw_str(lot, qty_exp),
            min_qty_str: raw_str(lot, qty_exp),
            funding_interval_s: interval,
            funding_anchor_s: 0,
            max_leverage: if kind == InstKind::Perp { 20 } else { 0 },
            ..InstSpec::default()
        });
    }
    for v in [S1, P2] {
        b.fee(FeeSpec {
            venue: v,
            tier: 0,
            fee: FeePair {
                maker: Rate(200_000),
                taker: Rate(500_000),
            },
        });
    }
    b.instance(InstanceSpec {
        id: ALPHA,
        kind_id: 0xFF01,
        name: "alpha".into(),
    });
    let venues = (1u64 << S1.0) | (1 << P2.0);
    b.account(AccountSpec {
        id: ACC,
        sim,
        bound_instance: ALPHA,
        is_prod: !sim,
        label: if sim { "sim-a".into() } else { "bn-1".into() },
        venues,
        ..AccountSpec::default()
    });
    if sim {
        b.sim(SimSpec {
            account: ACC,
            fee_tier: 0,
            latency_model: 0,
            margin_model: 0,
        });
        for v in [S1, P2] {
            b.sim_balance(SimBalanceSpec {
                account: ACC,
                venue: v,
                asset: USDT,
                qty: Qty(10_000_000_000),
            });
        }
    } else {
        for v in [S1, P2] {
            b.account_venue(AccountVenueSpec {
                account: ACC,
                venue: v,
                uid: format!("1{}", v.0),
            });
        }
    }
    b.finish()
}

// ------------------------------------------------------------------------------------- output

/// Boot-block lines and the bulk record stream of one journal.
#[derive(Default, Debug)]
pub struct Out {
    pub boot: Vec<(u16, Vec<u8>)>,
    pub bulk: Vec<u8>,
}

impl Out {
    pub fn rec(&mut self, kind: EventKind, flags: u16, tsc: u64, body: &[u8]) {
        self.bulk.push(1);
        self.bulk.extend_from_slice(&kind.0.to_le_bytes());
        self.bulk.extend_from_slice(&flags.to_le_bytes());
        self.bulk.extend_from_slice(&(body.len() as u32).to_le_bytes());
        self.bulk.extend_from_slice(&tsc.to_le_bytes());
        self.bulk.extend_from_slice(body);
    }
    pub fn end(&mut self) {
        self.bulk.push(2);
    }
    pub fn roll(&mut self) {
        self.bulk.push(4);
    }
    /// One event of one record.
    pub fn event(&mut self, kind: EventKind, t: i64, body: &[u8]) {
        self.rec(kind, 0, tsc(t), body);
        self.end();
    }

    /// Writes `<dir>/script.txt` (boot lines, `bulk`, `seal` when asked) and `<dir>/<name>.bulk`.
    pub fn write(&self, dir: &Path, name: &str, seal: bool) {
        std::fs::create_dir_all(dir).unwrap_or_else(|e| die(&format!("{}: {e}", dir.display())));
        let bulk = dir.join(format!("{name}.bulk"));
        std::fs::write(&bulk, &self.bulk).unwrap_or_else(|e| die(&format!("{}: {e}", bulk.display())));
        let mut s = String::new();
        for (k, b) in &self.boot {
            s.push_str(&format!("boot {k:#06x} {}\n", hex(b)));
        }
        s.push_str(&format!("bulk {}\n", bulk.display()));
        if seal {
            s.push_str("seal\n");
        }
        let p = dir.join("script.txt");
        std::fs::write(&p, s).unwrap_or_else(|e| die(&format!("{}: {e}", p.display())));
    }
}

pub fn hex(b: &[u8]) -> String {
    if b.is_empty() {
        return "-".into();
    }
    b.iter().map(|x| format!("{x:02x}")).collect()
}

/// `BlobHdr` + content of a config blob.
pub fn config_blob(
    kind: BlobKind,
    version: u64,
    content: &[u8],
    boot: bool,
    strategy: u8,
    account: u8,
) -> Vec<u8> {
    let mut scope = CmdScope::new_zeroed();
    scope.strategy = StrategyId(strategy);
    scope.account = AccountId(account);
    let h = BlobHdr {
        blob: BlobRef {
            kind,
            encoding: 1,
            len: content.len() as u32,
            version,
            sha256: Sha256::digest(content).into(),
        },
        scope,
        boot: u8::from(boot),
        _pad: [0; 7],
    };
    let mut v = h.as_bytes().to_vec();
    v.extend_from_slice(content);
    v
}

fn clock_status(t: i64) -> ClockStatus {
    let mut c = ClockStatus::new_zeroed();
    c.tsc = Tsc(tsc(t));
    c.realtime_ns = t;
    c.tsc_hz_milli = 3_000_000_000_000;
    c.source = 1;
    c.state = 1;
    c
}

fn market_hdr(venue: VenueId, inst: InstId, t: i64, k: u64) -> MarketHdr {
    let mut h = MarketHdr::new_zeroed();
    h.venue = venue;
    h.inst = inst;
    h.ex_ts_ns = t - 2 * MS;
    h.hw_rx_ns = if k.is_multiple_of(2) { t - MS } else { 0 };
    h
}

fn bbo(venue: VenueId, inst: InstId, t: i64, k: u64) -> Bbo {
    Bbo {
        h: market_hdr(venue, inst, t, k),
        bid_px: Px(6_000_000),
        bid_qty: Qty(50_000_000),
        ask_px: Px(6_000_001),
        ask_qty: Qty(50_000_000),
        update_id: k + 1,
    }
}

fn host_profile() -> Vec<u8> {
    let mut b = vec![0u8; 176];
    for (i, x) in b.iter_mut().enumerate().take(120) {
        *x = (i as u8).wrapping_mul(7).wrapping_add(1);
    }
    b[36..40].fill(0);
    b[120..128].copy_from_slice(&3_000_000_000_000u64.to_le_bytes());
    b
}

/// `OrderDecision{New}` body (88 B): Buy 0.01 BTC @ 60 000.00 on 101/S1, slot `i`, gen 1.
pub fn order_decision(i: u64, t: i64) -> Vec<u8> {
    let mut b = vec![0u8; 88];
    put(&mut b, 0, i, 8); // trigger_seq
    put(&mut b, 8, tsc(t), 8); // decided_tsc
    put(&mut b, 16, 6_000_000, 8); // px
    put(&mut b, 24, 1_000_000, 8); // qty
    put(&mut b, 32, 0x0100_0001, 4); // group
    put(&mut b, 36, u64::from(I101.0), 4); // inst
    put(&mut b, 40, i, 4); // slot
    put(&mut b, 44, 1, 4); // gen
    put(&mut b, 56, u64::from(ALPHA.0), 1); // strategy
    put(&mut b, 57, u64::from(S1.0), 1); // venue
    put(&mut b, 58, u64::from(ACC.0), 1); // account
    put(&mut b, 59, OrderAction::New as u64, 1);
    put(&mut b, 60, Side::Buy as u64, 1);
    put(&mut b, 61, Tif::Gtc as u64, 1);
    put(&mut b, 72, 6_000_000, 8); // req_px
    b
}

/// `OrderEvent` body (96 B) of slot `i`, gen 1, on 101/S1, fee asset USDT.
pub fn order_event(i: u64, t: i64, what: OrderEventKind) -> Vec<u8> {
    let mut b = vec![0u8; 96];
    put(&mut b, 0, u64::from(S1.0), 1);
    put(&mut b, 1, u64::from(ACC.0), 1);
    put(&mut b, 2, what as u64, 1);
    put(&mut b, 4, i, 4); // slot
    put(&mut b, 8, 1, 4); // gen
    put(&mut b, 12, u64::from(I101.0), 4);
    put(&mut b, 16, 6_000_000, 8); // px
    put(&mut b, 24, 1_000_000, 8); // qty
    put(&mut b, 48, u64::from(USDT.0), 2); // fee_asset
    put(&mut b, 50, 1, 2); // epoch
    put(&mut b, 56, 1_000 + i, 8); // exch_order_id
    put(&mut b, 64, (t - 2 * MS) as u64, 8); // ex_ts_ns
    b
}

/// J_MD / J_SIM: `n` Bbo per inst (101 at `t_k`, 102 at `t_k + 5 ms`), Trade on 101 every 1 s,
/// 100 OrderEvent + OrderDecision pairs from D + 20 s, ClockStatus every second.
fn md(sim: bool, n: u64, dir: &Path) {
    let mut o = Out::default();
    o.boot.push((
        EventKind::CONFIG_BLOB.0,
        config_blob(BlobKind::Refdata, 1, &refdata(sim, 1), true, 0, 0),
    ));
    o.boot.push((EventKind::HOST_PROFILE.0, host_profile()));
    // (t, order, kind, body)
    let mut ev: Vec<(i64, u8, EventKind, Vec<u8>)> = Vec::new();
    let t0 = D + 10 * S;
    for k in 0..n {
        let t = t0 + k as i64 * 10 * MS;
        ev.push((t, 1, EventKind::BBO, bbo(S1, I101, t, k).as_bytes().to_vec()));
        ev.push((
            t + 5 * MS,
            1,
            EventKind::BBO,
            bbo(P2, I102, t + 5 * MS, k).as_bytes().to_vec(),
        ));
    }
    let span = n as i64 * 10 * MS;
    let mut j = 0i64;
    while j * S < span && j < 600 {
        let t = t0 + j * S;
        let tr = Trade {
            h: market_hdr(S1, I101, t, 0),
            px: Px(6_000_000),
            qty: Qty(1_000_000),
            trade_id: j as u64,
            taker_side: Side::Buy,
            _pad: [0; 7],
        };
        ev.push((t, 2, EventKind::TRADE, tr.as_bytes().to_vec()));
        j += 1;
    }
    for i in 0..100u64 {
        let t = D + 20 * S + i as i64 * S;
        let mut body = order_event(i, t, OrderEventKind::Ack);
        body.extend_from_slice(&order_decision(i, t));
        ev.push((t, 3, EventKind::ORDER_EVENT, body));
    }
    let end = ev.iter().map(|e| e.0).max().unwrap_or(t0);
    let mut s = (D / S + 1) * S;
    while s <= end + S {
        ev.push((s, 0, EventKind::CLOCK_STATUS, clock_status(s).as_bytes().to_vec()));
        s += S;
    }
    ev.sort_by_key(|e| (e.0, e.1));
    for (t, _, kind, body) in ev {
        if kind == EventKind::ORDER_EVENT {
            let (a, b) = body.split_at(96);
            o.rec(EventKind::ORDER_EVENT, 0, tsc(t), a);
            o.rec(EventKind::ORDER_DECISION, 0, tsc(t), b);
            o.end();
        } else if kind == EventKind::CLOCK_STATUS {
            o.rec(kind, 0, tsc(t), &body);
        } else {
            o.event(kind, t, &body);
        }
    }
    o.write(dir, "md", true);
}

// ------------------------------------------------------------------------------------- J_ALL

/// Whether `info` may appear in a seq journal and is written here (not optional, not a test kind,
/// not the writer's own `SegmentRoll`).
pub fn seq_kind(info: &KindInfo) -> bool {
    info.journals.contains(hft_event::JournalMask::SEQ)
        && !info.optional
        && info.kind.0 >> 8 != 0x7F
        && info.kind != EventKind::SEGMENT_ROLL
}

fn put(b: &mut [u8], off: usize, v: u64, w: usize) {
    if let Some(d) = b.get_mut(off..off + w) {
        d.copy_from_slice(&v.to_le_bytes()[..w]);
    }
}

/// Deterministic legal values for every leaf of `fields` (enums: a listed value; ids: fixture ids).
pub fn fill(fields: &[FieldDesc], base: usize, b: &mut [u8], seed: u64) {
    for (i, f) in fields.iter().enumerate() {
        let off = base + usize::from(f.offset);
        let s = seed.wrapping_mul(31).wrapping_add(i as u64 * 7 + 1);
        match f.ty {
            FieldTy::Pad(_) => {}
            FieldTy::Nested(inner) => fill(inner, off, b, s),
            FieldTy::Enum(d) => {
                let v = d
                    .items
                    .get((s as usize) % d.items.len().max(1))
                    .map_or(0, |x| x.value);
                put(b, off, u64::from(v), usize::from(d.width));
            }
            FieldTy::Bits(d) => put(b, off, 0, usize::from(d.width)),
            FieldTy::Id(k) => {
                let v: u64 = match k {
                    IdKind::Inst => [101u64, 102, 202][(s % 3) as usize],
                    IdKind::Asset => 3,
                    IdKind::Venue => [S1.0, P2.0][(s % 2) as usize] as u64,
                    IdKind::Account => 1,
                    IdKind::Strategy => 1,
                    IdKind::Group => 0x0100_0001,
                    IdKind::Slot => s % 1000,
                    IdKind::Conn => s % 4,
                    IdKind::Kind => 0x0101,
                };
                put(b, off, v, usize::from(k.width()));
            }
            FieldTy::Bytes(n) => {
                for (j, x) in b.iter_mut().skip(off).take(usize::from(n)).enumerate() {
                    *x = (s as u8).wrapping_add(j as u8) | 1;
                }
            }
            ty => {
                let w = usize::from(ty.width());
                let v = match ty {
                    FieldTy::I8 | FieldTy::I16 | FieldTy::I32 | FieldTy::I64 => (s % 1000) as i64 - 500,
                    _ => (s % 1_000_000) as i64 + 1,
                } as u64;
                put(b, off, v, w);
            }
        }
    }
}

fn struct_desc(name: &str) -> &'static hft_event::schema::StructDesc {
    hft_event::schema::catalog()
        .structs
        .iter()
        .find(|s| s.name == name)
        .unwrap_or_else(|| die(&format!("no struct {name}")))
}

/// Body of `info` number `n` (Var kinds: `n` tail entries; n = 0 → empty tail).
pub fn body_of(info: &KindInfo, n: u64, seed: u64) -> Vec<u8> {
    let k = info.kind;
    let prefix = match info.layout {
        Layout::Fixed { size } => usize::from(size),
        Layout::Var { prefix, .. } | Layout::Multi { prefix } => usize::from(prefix),
    };
    let mut b = vec![0u8; prefix];
    fill(info.fields, 0, &mut b, seed);
    let entries = |name: &str, n: u64, seed: u64| -> Vec<u8> {
        let s = struct_desc(name);
        let mut t = vec![0u8; usize::from(s.size) * n as usize];
        for j in 0..n as usize {
            let mut e = vec![0u8; usize::from(s.size)];
            fill(s.fields, 0, &mut e, seed + j as u64);
            t[j * usize::from(s.size)..(j + 1) * usize::from(s.size)].copy_from_slice(&e);
        }
        t
    };
    match k {
        EventKind::L2_BATCH => {
            put(&mut b, 72, n, 2);
            put(&mut b, 74, n, 2);
            b.extend(entries("Level", 2 * n, seed));
        }
        EventKind::L2_SNAPSHOT => {
            put(&mut b, 68, n, 2);
            put(&mut b, 70, n, 2);
            b.extend(entries("Level", 2 * n, seed));
        }
        EventKind::MARGIN_REPORT => {
            put(&mut b, 4, n, 2);
            b.extend(entries("MarginItem", n, seed));
        }
        EventKind::ACCOUNT_CONFIG_REPORT => {
            put(&mut b, 6, n, 2);
            b.extend(entries("LeverageItem", n, seed));
        }
        EventKind::ADVICE => {
            put(&mut b, 8, 8 * n, 2);
            b.extend((0..8 * n).map(|x| x as u8));
        }
        EventKind::CONFIG_BLOB => {
            let content: Vec<u8> = (0..32 * n).map(|x| (x * 3) as u8).collect();
            let kinds = [
                BlobKind::StrategyParams,
                BlobKind::HostManifest,
                BlobKind::SimConfig,
            ];
            b = config_blob(kinds[(seed % 3) as usize], seed, &content, false, 1, 1);
        }
        EventKind::CONTROL_CMD => {
            let plen = if n == 0 { 0 } else { 16 * n as usize };
            put(&mut b, 76, plen as u64, 4);
            b.extend((0..plen).map(|x| x as u8));
        }
        EventKind::RAW_RX | EventKind::RAW_TX => {
            put(&mut b, 16, 8 * n, 4);
            b.extend((0..8 * n).map(|x| x as u8));
        }
        EventKind::STATE_SNAPSHOT => {
            put(&mut b, 0, 3, 1);
            put(&mut b, 4, 0, 4);
            b.extend((0..16 * n).map(|x| x as u8));
        }
        EventKind::NOTE => {
            // Output source (Sequencer) so the writer files it as an ordinary record.
            put(&mut b, 5, 1, 1);
        }
        EventKind::FUNDING_SETTLE => put(&mut b, 48, 0, 1),
        _ => {}
    }
    b
}

/// J_ALL: every seq kind ≥ 3 records (Var kinds with a non-empty tail), a 3-part ControlCmd, Bbo
/// on 101/102/202, OrderEvent of account 1 with fee_asset USDT; two segments.
fn all(dir: &Path) {
    let mut o = Out::default();
    o.boot.push((
        EventKind::CONFIG_BLOB.0,
        config_blob(BlobKind::Refdata, 1, &refdata(false, 1), true, 0, 0),
    ));
    o.boot.push((EventKind::HOST_PROFILE.0, host_profile()));
    let kinds: Vec<&'static KindInfo> = hft_event::KINDS.iter().filter(|k| seq_kind(k)).collect();
    let mut t = D + 10 * S;
    for round in 0..4u64 {
        if round == 2 {
            o.roll();
        }
        o.rec(EventKind::CLOCK_STATUS, 0, tsc(t), clock_status(t).as_bytes());
        for (i, info) in kinds.iter().enumerate() {
            t += MS;
            let seed = round * 1000 + i as u64;
            let mut body = body_of(info, round % 3 + u64::from(round == 0), seed);
            if info.kind == EventKind::BBO {
                body[4..8].copy_from_slice(&[101u32, 102, 202, 101][round as usize].to_le_bytes());
            }
            if info.kind == EventKind::ORDER_EVENT {
                body[1] = 1;
                body[48..50].copy_from_slice(&USDT.0.to_le_bytes());
            }
            if info.kind == EventKind::CLOCK_STATUS {
                body = clock_status(t).as_bytes().to_vec();
            }
            if info.kind == EventKind::CONTROL_CMD && round == 1 {
                // 3 parts: logical body > 2 × PART_MAX_CHUNK.
                let plen = 140_000usize;
                body.truncate(128);
                body[76..80].copy_from_slice(&(plen as u32).to_le_bytes());
                body.extend((0..plen).map(|x| (x % 251) as u8));
            }
            let role = hft_event::classify(info.kind, &body).unwrap_or(hft_event::Role::Input);
            o.rec(info.kind, 0, tsc(t), &body);
            if role != hft_event::Role::Meta {
                o.end();
            }
        }
    }
    // PY-045: one Advice with known header fields and a 16-byte payload.
    let mut adv = vec![0u8; 32];
    put(&mut adv, 0, 3, 1); // strategy
    put(&mut adv, 2, 1, 2); // local_kind
    put(&mut adv, 4, 0x0300_0001, 4); // group
    put(&mut adv, 8, 16, 2); // len
    put(&mut adv, 16, 1000, 8); // basis_seq
    put(&mut adv, 24, 3_000_000_000, 8); // basis_tsc
    adv.extend_from_slice(&[1, 0, 0, 0, 0, 0, 0, 0]);
    adv.extend_from_slice(&12_345i64.to_le_bytes());
    o.event(EventKind::ADVICE, t + MS, &adv);
    o.write(dir, "all", true);
}

// ------------------------------------------------------------------------------------- RUST_REF

fn leaf_values(fields: &[FieldDesc], base: usize, prefix: &str, b: &[u8], out: &mut Map<String, Value>) {
    for f in fields {
        let path = if prefix.is_empty() {
            f.name.to_string()
        } else {
            format!("{prefix}.{}", f.name)
        };
        let off = base + usize::from(f.offset);
        match f.ty {
            FieldTy::Pad(_) => {}
            FieldTy::Nested(inner) => leaf_values(inner, off, &path, b, out),
            FieldTy::Bytes(n) => {
                let s = b.get(off..off + usize::from(n)).unwrap_or(&[]);
                out.insert(path, json!(hex(s)));
            }
            ty => {
                let w = usize::from(ty.width());
                let mut x = [0u8; 8];
                if let Some(s) = b.get(off..off + w) {
                    x[..w].copy_from_slice(s);
                }
                let u = u64::from_le_bytes(x);
                let signed = matches!(
                    ty,
                    FieldTy::I8
                        | FieldTy::I16
                        | FieldTy::I32
                        | FieldTy::I64
                        | FieldTy::Px
                        | FieldTy::Qty
                        | FieldTy::Notional
                        | FieldTy::Rate
                        | FieldTy::UtcNs
                );
                let v = if signed {
                    let sh = 64 - 8 * w as u32;
                    json!(((u << sh) as i64) >> sh)
                } else {
                    json!(u)
                };
                out.insert(path, v);
            }
        }
    }
}

/// Typed decoding of the five hand-checked kinds must agree with the generic field dump.
fn typed_check(kind: EventKind, body: &[u8], fields: &Map<String, Value>) -> bool {
    use hft_event::RecordView;
    use zerocopy::TryFromBytes;
    if ![
        EventKind::BBO,
        EventKind::ORDER_DECISION,
        EventKind::EXTERNAL_FILL,
        EventKind::MARGIN_REPORT,
        EventKind::CONFIG_BLOB,
    ]
    .contains(&kind)
    {
        return true;
    }
    let mut rec = vec![0u8; 24 + body.len() + 8];
    let n = 24 + body.len().div_ceil(8) * 8;
    rec[..4].copy_from_slice(&(n as u32).to_le_bytes());
    rec[4..6].copy_from_slice(&kind.0.to_le_bytes());
    rec[24..24 + body.len()].copy_from_slice(body);
    let Ok(v) = hft_event::decode_record(&rec[..n]) else {
        return false;
    };
    let v: RecordView<'_> = v;
    let f = |k: &str| fields.get(k).cloned().unwrap_or(Value::Null);
    match kind {
        EventKind::BBO => v.read::<Bbo>().is_ok_and(|b| {
            f("bid_px") == json!(b.bid_px.0)
                && f("h.inst") == json!(b.h.inst.0)
                && f("update_id") == json!(b.update_id)
        }),
        EventKind::ORDER_DECISION => v.read::<OrderDecision>().is_ok_and(|d| {
            f("qty") == json!(d.qty.0)
                && f("account") == json!(d.account.0)
                && f("action") == json!(d.action as u8)
        }),
        EventKind::EXTERNAL_FILL => v
            .read::<ExternalFill>()
            .is_ok_and(|e| f("fee") == json!(e.fee.0) && f("fee_asset") == json!(e.fee_asset.0)),
        EventKind::MARGIN_REPORT => v
            .var::<MarginReportHdr>()
            .is_ok_and(|(h, _)| f("n") == json!(h.n) && f("margin_balance") == json!(h.margin_balance.0)),
        EventKind::CONFIG_BLOB => {
            let h = body
                .get(..72)
                .and_then(|x| hft_event::BlobHdr::try_read_from_bytes(x).ok());
            h.is_some_and(|h| f("blob.len") == json!(h.blob.len) && f("boot") == json!(h.boot))
        }
        _ => true,
    }
}

fn reference(cold_root: &Path, instance: &str, out: &Path) {
    let inst: InstanceName = instance.parse().unwrap_or_else(|_| die("instance"));
    let mut rc = ReaderConfig::new(
        cold_root.join(".nohot"),
        cold_root.to_path_buf(),
        inst,
        "seq".into(),
    );
    rc.start = StartAt::Oldest;
    rc.end = EndAt::LastCompleteEvent;
    let mut r = JournalReader::open(&rc).unwrap_or_else(|e| die(&format!("open: {e:?}")));
    let mut files: std::collections::BTreeMap<String, std::fs::File> = std::collections::BTreeMap::new();
    std::fs::create_dir_all(out).unwrap_or_else(|e| die(&e.to_string()));
    let mut bad = false;
    loop {
        let (seq, kind, tsc, flags, body): (u64, u16, u64, u16, Vec<u8>) = match r.poll() {
            Next::Record(v) => (v.hdr.seq, v.hdr.kind, v.hdr.tsc, v.hdr.flags, v.body.to_vec()),
            Next::Event(e) => (e.seq, e.kind.0, e.tsc, e.flags.0, e.body.to_vec()),
            Next::End | Next::Pending => break,
            Next::Gap(g) => die(&format!("gap {g:?}")),
            Next::Err(e) => die(&format!("read error {e:?}")),
        };
        let Some(info) = kind_info(EventKind(kind)) else {
            continue;
        };
        let mut fields = Map::new();
        leaf_values(info.fields, 0, "", &body, &mut fields);
        if !typed_check(EventKind(kind), &body, &fields) {
            eprintln!(
                "pyfixture: typed decode of {} seq {seq} disagrees with the field dump",
                info.name
            );
            bad = true;
        }
        let mut line = json!({"seq": seq, "tsc": tsc, "flags": flags, "fields": fields});
        if !matches!(info.layout, Layout::Fixed { .. }) {
            line["body_hex"] = json!(hex(&body));
        }
        let f = files.entry(info.name.to_string()).or_insert_with(|| {
            std::fs::File::create(out.join(format!("{}.jsonl", info.name)))
                .unwrap_or_else(|e| die(&e.to_string()))
        });
        let _ = writeln!(f, "{line}");
    }
    if bad {
        exit(2);
    }
}

// ------------------------------------------------------------------------------------- J_CFG

/// SPB1 blob of `alpha` on account 1 (`EchoProbe` schema, `version` as given, one group).
pub fn sp_blob(version: u64) -> Vec<u8> {
    use hft_event::{GroupSpec, LegDef, ParamSource, StrategyParamsHdr};
    use hft_strategy_sdk::params::{OwnedSchema, RawParams, SchemaView, encode_body};
    let text = std::fs::read_to_string(repo().join("fixtures/params/sdk-tp/schema.golden.json"))
        .unwrap_or_else(|e| die(&format!("schema golden: {e}")));
    let s = OwnedSchema::from_json(&text).unwrap_or_else(|_| die("schema golden"));
    let defaults: Vec<i64> = s.fields().iter().map(|f| f.default).collect();
    let body = encode_body(
        &s,
        1,
        &RawParams {
            defaults,
            overrides: Vec::new(),
        },
    )
    .unwrap_or_else(|_| die("body"));
    let mut legs = [LegDef {
        inst: InstId(0),
        venue: VenueId(0),
        kind: 0,
        _pad: 0,
        route: 0,
        _pad2: 0,
    }; 4];
    legs[0] = LegDef {
        inst: I101,
        venue: S1,
        kind: LegDef::CEX,
        _pad: 0,
        route: 0,
        _pad2: 0,
    };
    legs[1] = LegDef {
        inst: I102,
        venue: P2,
        kind: LegDef::CEX,
        _pad: 0,
        route: 0,
        _pad2: 0,
    };
    let g = GroupSpec {
        id: GroupId(0x0100_0001),
        asset: BTC,
        n_legs: 2,
        open_allowed: 1,
        legs,
        _rsv: 0,
    };
    let hdr = StrategyParamsHdr {
        strategy: ALPHA,
        account: ACC,
        kind_id: s.kind_id(),
        schema_id: s.id(),
        schema_ver: s.version(),
        source: ParamSource::Manual,
        _pad: 0,
        n_groups: 1,
        _pad2: 0,
        version,
        generated_at_ns: D,
        body_len: body.len() as u32,
        group_ctr_hwm: 1,
        schema_digest: s.schema_digest(s.version()),
    };
    let mut v = hdr.as_bytes().to_vec();
    v.extend_from_slice(g.as_bytes());
    v.extend_from_slice(&body);
    v
}

/// ECF1 with ledger defaults and an assembly row for the cold-reader projection test.
pub fn engine_blob(version: u64) -> Vec<u8> {
    use hft_refdata::{EcfBuilder, EngineSection};
    let row = hft_ledger::LedgerConfig::default().to_row();
    let mut b = EcfBuilder::new(version, hft_types::HostClass::Dev2);
    b.created_utc_ns(D);
    b.section(
        EngineSection::Ledger,
        hft_ledger::LEDGER_ROW_VERSION,
        512,
        1,
        row.as_bytes().to_vec(),
    );
    let mut assembly = [0u8; 16];
    assembly[0] = 1; // Assembly field 1: role_prod = false; hft-py exposes raw section bytes.
    b.section(EngineSection::Assembly, 1, 16, 1, assembly.to_vec());
    b.finish()
}

fn repo() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn blob_ref(kind: BlobKind, version: u64, content: &[u8]) -> BlobRef {
    BlobRef {
        kind,
        encoding: 1,
        len: content.len() as u32,
        version,
        sha256: Sha256::digest(content).into(),
    }
}

fn control_cmd(id: u8, cmd: &hft_event::ControlCmd<'_>, t: i64) -> Vec<u8> {
    let mut h = hft_event::CmdHdr::new_zeroed();
    h.cmd_id = [id; 16];
    h.issued_at_ns = t;
    h.role = hft_event::CmdHdr::ROLE_ADMIN;
    h.account = ACC;
    let mut out = vec![0u8; 4096];
    let n = cmd
        .encode(&h, &mut out)
        .unwrap_or_else(|e| die(&format!("encode cmd: {e:?}")));
    out.truncate(n);
    out
}

fn cmd_result(id: u8, cmd: hft_event::CmdKind, status: u8) -> Vec<u8> {
    let mut r = hft_event::CmdResult::new_zeroed();
    r.cmd_id = [id; 16];
    r.cmd = cmd;
    r.status = status;
    r.as_bytes().to_vec()
}

/// J_CFG (TESTS.md `config_at` fixture): boot block of segment 1, steps ①–④, Drain; segment 2 is a
/// new run whose boot block restates refdata v2, risk v2 and strategy params v8.
fn cfg(dir: &Path) {
    use hft_event::{CmdKind, ControlCmd};
    let rd1 = refdata(false, 1);
    let rd2 = refdata(false, 2);
    let risk1 = b"risk-params-v1".to_vec();
    let risk2 = b"risk-params-v2".to_vec();
    let manifest = b"host-manifest".to_vec();
    let eng = engine_blob(1);
    let mut o = Out::default();
    let boot1 = vec![
        (EventKind::HOST_PROFILE.0, host_profile()),
        (
            EventKind::CONFIG_BLOB.0,
            config_blob(BlobKind::Refdata, 1, &rd1, true, 0, 0),
        ),
        (
            EventKind::CONFIG_BLOB.0,
            config_blob(BlobKind::RiskParams, 1, &risk1, true, 0, 0),
        ),
        (
            EventKind::CONFIG_BLOB.0,
            config_blob(BlobKind::Engine, 1, &eng, true, 0, 0),
        ),
        (
            EventKind::CONFIG_BLOB.0,
            config_blob(BlobKind::StrategyParams, 7, &sp_blob(7), true, ALPHA.0, ACC.0),
        ),
        (
            EventKind::CONFIG_BLOB.0,
            config_blob(BlobKind::HostManifest, 1, &manifest, true, 0, 0),
        ),
    ];
    o.boot = boot1;
    let mut t = D + 10 * S;
    let mut step = |o: &mut Out,
                    id: u8,
                    kind: BlobKind,
                    version: u64,
                    content: &[u8],
                    cmd: ControlCmd<'_>,
                    ck: CmdKind,
                    status: u8| {
        t += S;
        o.event(
            EventKind::CONFIG_BLOB,
            t,
            &config_blob(kind, version, content, false, ALPHA.0, ACC.0),
        );
        t += S;
        o.rec(EventKind::CONTROL_CMD, 0, tsc(t), &control_cmd(id, &cmd, t));
        o.rec(EventKind::CMD_RESULT, 0, tsc(t), &cmd_result(id, ck, status));
        o.end();
        let _ = blob_ref(kind, version, content);
    };
    let sp8 = sp_blob(8);
    let sp9 = sp_blob(9);
    let mut scope = CmdScope::new_zeroed();
    scope.strategy = ALPHA;
    scope.account = ACC;
    step(
        &mut o,
        1,
        BlobKind::StrategyParams,
        8,
        &sp8,
        ControlCmd::SetParams {
            scope,
            blob: blob_ref(BlobKind::StrategyParams, 8, &sp8),
        },
        CmdKind::SET_PARAMS,
        0,
    );
    step(
        &mut o,
        2,
        BlobKind::StrategyParams,
        9,
        &sp9,
        ControlCmd::SetParams {
            scope,
            blob: blob_ref(BlobKind::StrategyParams, 9, &sp9),
        },
        CmdKind::SET_PARAMS,
        20,
    );
    step(
        &mut o,
        3,
        BlobKind::RiskParams,
        2,
        &risk2,
        ControlCmd::SetRisk {
            blob: blob_ref(BlobKind::RiskParams, 2, &risk2),
        },
        CmdKind::SET_RISK,
        0,
    );
    step(
        &mut o,
        4,
        BlobKind::Refdata,
        2,
        &rd2,
        ControlCmd::ReplaceRefdata {
            blob: blob_ref(BlobKind::Refdata, 2, &rd2),
        },
        CmdKind::REPLACE_REFDATA,
        0,
    );
    let drain_t = D + 30 * S;
    o.rec(
        EventKind::CONTROL_CMD,
        0,
        tsc(drain_t),
        &control_cmd(
            5,
            &ControlCmd::Drain {
                cancel_resting: false,
            },
            drain_t,
        ),
    );
    o.rec(
        EventKind::CMD_RESULT,
        0,
        tsc(drain_t),
        &cmd_result(5, CmdKind::DRAIN, 0),
    );
    o.end();
    o.write(dir, "cfg1", true);
    // Segment 2: written by a second TW run (a restart) with this boot block.
    let mut o2 = Out {
        boot: Vec::new(),
        bulk: Vec::new(),
    };
    o2.boot = vec![
        (EventKind::HOST_PROFILE.0, host_profile()),
        (
            EventKind::CONFIG_BLOB.0,
            config_blob(BlobKind::Refdata, 2, &rd2, true, 0, 0),
        ),
        (
            EventKind::CONFIG_BLOB.0,
            config_blob(BlobKind::RiskParams, 2, &risk2, true, 0, 0),
        ),
        (
            EventKind::CONFIG_BLOB.0,
            config_blob(BlobKind::Engine, 1, &eng, true, 0, 0),
        ),
        (
            EventKind::CONFIG_BLOB.0,
            config_blob(BlobKind::StrategyParams, 8, &sp8, true, ALPHA.0, ACC.0),
        ),
        (
            EventKind::CONFIG_BLOB.0,
            config_blob(BlobKind::HostManifest, 1, &manifest, true, 0, 0),
        ),
    ];
    let t2 = D + 40 * S;
    o2.event(EventKind::BBO, t2, bbo(S1, I101, t2, 0).as_bytes());
    std::fs::create_dir_all(dir.join("run2")).unwrap_or_else(|e| die(&e.to_string()));
    o2.write(&dir.join("run2"), "cfg2", true);
}

// ------------------------------------------------------------------------------------- J_LED20

/// The capacities hft-py's cold ledger uses (`hft_py::ledger::capacities`); kept equal here so
/// snapshots written by this generator restore there.
fn led_caps(rd: &hft_refdata::RefData) -> hft_ledger::Capacities {
    hft_ledger::Capacities {
        max_insts: (rd.inst_count() as u32 + 1).max(4096),
        max_cells_total: 8192,
        max_timers: 256,
        recent_cmd_ids: 64,
        max_subs_per_inst: 8,
        max_groups_per_instance: 1024,
        max_instances: 254,
    }
}

const G1: u32 = 0x0100_0001;
const G2: u32 = 0x0100_0002;

fn led_groups() -> Vec<hft_event::GroupSpec> {
    use hft_event::{GroupSpec, LegDef};
    let leg = |inst: InstId, venue: VenueId| LegDef {
        inst,
        venue,
        kind: LegDef::CEX,
        _pad: 0,
        route: 0,
        _pad2: 0,
    };
    let none = LegDef {
        inst: InstId(0),
        venue: VenueId(0),
        kind: 0,
        _pad: 0,
        route: 0,
        _pad2: 0,
    };
    vec![
        GroupSpec {
            id: GroupId(G1),
            asset: BTC,
            n_legs: 2,
            open_allowed: 1,
            legs: [leg(I101, S1), leg(I102, P2), none, none],
            _rsv: 0,
        },
        GroupSpec {
            id: GroupId(G2),
            asset: ETH,
            n_legs: 1,
            open_allowed: 1,
            legs: [leg(I202, P2), none, none, none],
            _rsv: 0,
        },
    ]
}

/// SPB1 of `alpha` with the ledger fixture groups.
fn led_sp_blob(version: u64) -> Vec<u8> {
    use hft_event::{ParamSource, StrategyParamsHdr};
    use hft_strategy_sdk::params::{OwnedSchema, RawParams, SchemaView, encode_body};
    let text = std::fs::read_to_string(repo().join("fixtures/params/sdk-tp/schema.golden.json"))
        .unwrap_or_else(|e| die(&format!("schema golden: {e}")));
    let s = OwnedSchema::from_json(&text).unwrap_or_else(|_| die("schema golden"));
    let groups = led_groups();
    let defaults: Vec<i64> = s.fields().iter().map(|f| f.default).collect();
    let body = encode_body(
        &s,
        groups.len() as u16,
        &RawParams {
            defaults,
            overrides: Vec::new(),
        },
    )
    .unwrap_or_else(|_| die("body"));
    let hdr = StrategyParamsHdr {
        strategy: ALPHA,
        account: ACC,
        kind_id: s.kind_id(),
        schema_id: s.id(),
        schema_ver: s.version(),
        source: ParamSource::Manual,
        _pad: 0,
        n_groups: groups.len() as u16,
        _pad2: 0,
        version,
        generated_at_ns: D,
        body_len: body.len() as u32,
        group_ctr_hwm: 2,
        schema_digest: s.schema_digest(s.version()),
    };
    let mut v = hdr.as_bytes().to_vec();
    for g in &groups {
        v.extend_from_slice(g.as_bytes());
    }
    v.extend_from_slice(&body);
    v
}

/// xorshift64*.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n.max(1)
    }
}

/// `StateSnapshot` logical body: `SnapshotHdr` + the ledger's sections + hft-oms section 2 rows of
/// the open orders (the slot map `JournalLedger` restores; row offsets as `from_snapshot` reads).
fn snapshot_body(
    l: &hft_ledger::Ledger,
    purpose: u8,
    at_seq: u64,
    engine_ns: i64,
    orders: &[Ord],
) -> Vec<u8> {
    let mut w = hft_strategy_sdk::SnapshotWriter::new(64 << 20);
    l.snapshot(&mut w);
    w.begin_section(hft_ledger::OMS_SECTION_SLOTS, 2);
    for o in orders {
        let mut row = vec![0u8; hft_ledger::OMS_SLOT_ROW_LEN];
        put(&mut row, 1, if o.buy { 1 } else { 2 }, 1);
        put(
            &mut row,
            6,
            u64::from(if o.inst == I101 { S1.0 } else { P2.0 }),
            1,
        );
        put(&mut row, 7, u64::from(ACC.0), 1);
        put(&mut row, 8, 1, 4); // gen
        put(&mut row, 12, u64::from(o.slot), 4);
        put(&mut row, 16, u64::from(o.inst.0), 4);
        put(&mut row, 22, 1, 2); // epoch
        put(&mut row, 24, u64::from(if o.inst == I202 { G2 } else { G1 }), 4);
        put(&mut row, 40, o.cum as u64, 8);
        let n = i128::from(o.avg) * i128::from(o.cum);
        put(&mut row, 64, n as u64, 8);
        put(&mut row, 72, (n >> 64) as u64, 8);
        put(&mut row, 160, o.cum as u64, 8); // SlotExt.fee_cum
        w.put_bytes(&row);
        w.add_rows(1);
    }
    w.end_section();
    let mut b = vec![0u8; 96];
    put(&mut b, 0, u64::from(purpose), 1);
    put(&mut b, 4, 4, 4);
    put(&mut b, 8, at_seq, 8);
    put(&mut b, 88, engine_ns as u64, 8);
    b.extend_from_slice(w.as_bytes());
    b
}

/// Records of one segment's boot block for the ledger fixtures.
fn led_boot(rd: &[u8], eng: &[u8], sp: &[u8], snap: &[u8]) -> Vec<(u16, Vec<u8>)> {
    vec![
        (EventKind::HOST_PROFILE.0, host_profile()),
        (
            EventKind::CONFIG_BLOB.0,
            config_blob(BlobKind::Refdata, 1, rd, true, 0, 0),
        ),
        (
            EventKind::CONFIG_BLOB.0,
            config_blob(BlobKind::Engine, 1, eng, true, 0, 0),
        ),
        (
            EventKind::CONFIG_BLOB.0,
            config_blob(BlobKind::StrategyParams, 1, sp, true, ALPHA.0, ACC.0),
        ),
        (EventKind::STATE_SNAPSHOT.0, snap.to_vec()),
    ]
}

/// Generator state: the bulk being written and a mirror `JournalLedger` fed the same records.
struct Led {
    out: Vec<u8>,
    jl: hft_ledger::JournalLedger,
    pending: Vec<(EventKind, u64, Vec<u8>)>,
    seq: u64,
}

impl Led {
    fn rec(&mut self, kind: EventKind, t: i64, body: Vec<u8>) {
        self.pending.push((kind, tsc(t), body));
    }

    /// Ends the event: bulk records + `end`; the mirror ledger sees the same records (the last one
    /// with `EVENT_END`).
    fn end(&mut self) {
        let n = self.pending.len();
        for (i, (k, ts, b)) in std::mem::take(&mut self.pending).into_iter().enumerate() {
            self.out.push(1);
            self.out.extend_from_slice(&k.0.to_le_bytes());
            self.out.extend_from_slice(&0u16.to_le_bytes());
            self.out.extend_from_slice(&(b.len() as u32).to_le_bytes());
            self.out.extend_from_slice(&ts.to_le_bytes());
            self.out.extend_from_slice(&b);
            let flags = if i + 1 == n {
                hft_event::RecordFlags::EVENT_END.0
            } else {
                0
            };
            self.seq += 1;
            let hdr = hft_event::RecordHeader {
                len: (24 + b.len()) as u32,
                kind: k.0,
                flags,
                seq: self.seq,
                tsc: ts,
            };
            self.jl
                .apply(&hft_event::RecordView { hdr, body: &b })
                .expect("valid fixture journal");
        }
        self.out.push(2);
    }
}

fn bbo_px(inst: InstId, t: i64, k: u64, mid: i64) -> Vec<u8> {
    let venue = if inst == I101 { S1 } else { P2 };
    let mut b = bbo(venue, inst, t, k);
    b.bid_px = Px(mid - 1);
    b.ask_px = Px(mid + 1);
    b.as_bytes().to_vec()
}

fn mark_px(inst: InstId, t: i64, px: i64) -> Vec<u8> {
    let mut b = market_hdr(P2, inst, t, 0).as_bytes().to_vec();
    b.extend_from_slice(&px.to_le_bytes());
    b
}

fn balance(venue: VenueId, asset: AssetId, free: i64, t: i64) -> Vec<u8> {
    let mut b = vec![0u8; 40];
    put(&mut b, 0, u64::from(venue.0), 1);
    put(&mut b, 1, u64::from(ACC.0), 1);
    put(&mut b, 2, u64::from(asset.0), 2);
    put(&mut b, 8, free as u64, 8);
    put(&mut b, 24, t as u64, 8);
    b
}

fn led_note(code: u32, v0: u64) -> Vec<u8> {
    let mut b = vec![0u8; 80];
    put(&mut b, 0, u64::from(code), 4);
    put(&mut b, 4, 1, 1); // level Info
    put(&mut b, 5, 1, 1); // source Sequencer
    put(&mut b, 16, v0, 8);
    b
}

/// One open order: (slot, inst, side buy, qty, cum, px).
struct Ord {
    slot: u32,
    inst: InstId,
    buy: bool,
    qty: i64,
    cum: i64,
    px: i64,
    /// Last reported average fill price.
    avg: i64,
}

/// J_LED20 seed `seed`: account 1 with groups G1 (BTC 101/S1 + 102/P2) and G2 (ETH 202/P2);
/// orders and partial fills, external fills on the perpetuals, funding pushes, balances, BBO and
/// marks over about 30 h (crossing a UTC day); keyframe snapshots; three segments, each boot
/// block restating the configuration and the current ledger state.
fn led(seed: u64, dir: &Path) {
    let rd_blob = refdata(false, 1);
    let rd = hft_refdata::RefData::load(&rd_blob, None).unwrap_or_else(|e| die(&format!("refdata: {e:?}")));
    let eng = engine_blob(1);
    let sp = led_sp_blob(1);
    let cfg = hft_ledger::LedgerConfig::default();
    let caps = led_caps(&rd);
    let mut base =
        hft_ledger::Ledger::new(&rd, &cfg, &caps).unwrap_or_else(|e| die(&format!("ledger: {e:?}")));
    base.on_groups_changed(ALPHA, &hft_strategy_sdk::GroupTable::new(&led_groups(), 1024));
    let snap0 = snapshot_body(&base, 1, 0, 0, &[]);
    let mut r = hft_strategy_sdk::SnapshotReader::new(snap0.get(96..).unwrap_or(&[]));
    let jl = hft_ledger::JournalLedger::from_snapshot(&mut r, &rd, &cfg, &caps)
        .unwrap_or_else(|e| die(&format!("{e:?}")));
    let mut g = Led {
        out: Vec::new(),
        jl,
        pending: Vec::new(),
        seq: 0,
    };
    let mut rng = Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1);
    let mut script = String::new();
    for (k, b) in led_boot(&rd_blob, &eng, &sp, &snap0) {
        script.push_str(&format!("boot {k:#06x} {}\n", hex(&b)));
    }
    let mut part = 0;
    let flush_part = |g: &mut Led, script: &mut String, part: &mut usize| {
        let p = dir.join(format!("led{part}.bulk"));
        std::fs::write(&p, std::mem::take(&mut g.out)).unwrap_or_else(|e| die(&e.to_string()));
        script.push_str(&format!("bulk {}\n", p.display()));
        *part += 1;
    };
    std::fs::create_dir_all(dir).unwrap_or_else(|e| die(&e.to_string()));
    // Boot block of segment 1 occupies seqs 1..=7 (Begin, 5 boot records, BootDone).
    g.seq = 7;
    let mut t = D + 3_600 * S;
    // Run start: EngineClock + EpochStart.
    let mut ec = vec![0u8; 32];
    put(&mut ec, 0, t as u64, 8);
    put(&mut ec, 8, 3_000_000_000_000, 8);
    g.rec(EventKind::ENGINE_CLOCK, t, ec);
    g.rec(
        EventKind::NOTE,
        t,
        led_note(hft_ledger::notes::code::EPOCH_START, 1),
    );
    g.end();
    for (v, a, q) in [
        (S1, USDT, 100_000_000_000i64),
        (S1, BTC, 100_000_000),
        (P2, USDT, 50_000_000_000),
    ] {
        t += S;
        g.rec(EventKind::BALANCE_REPORT, t, balance(v, a, q, t));
        g.end();
    }
    let mut mids = [(I101, 6_000_000i64), (I102, 6_000_100), (I202, 300_000)];
    let mut orders: Vec<Ord> = Vec::new();
    let mut slot = 0u32;
    let mut trade_id = 1_000u64;
    let n_ops = 150 + rng.below(50);
    let rolls = [n_ops / 3, 2 * n_ops / 3];
    for op in 0..n_ops {
        t += (60 + rng.below(1_200) as i64) * S;
        for (inst, mid) in mids.iter_mut() {
            *mid += rng.below(2_001) as i64 - 1_000;
            if *mid < 1_000 {
                *mid = 1_000;
            }
            let body = bbo_px(*inst, t, op, *mid);
            g.rec(EventKind::BBO, t, body);
            g.end();
            if *inst != I101 {
                g.rec(EventKind::MARK_PRICE, t, mark_px(*inst, t, *mid));
                g.end();
            }
        }
        match rng.below(10) {
            0..=3 => {
                // New order: Input Bbo + Output OrderDecision{New}.
                let ix = rng.below(3) as usize;
                let (inst, mid) = mids[ix];
                let buy = rng.below(2) == 0;
                let qty = if inst == I202 {
                    10_000 * (1 + rng.below(20) as i64)
                } else {
                    100_000 * (1 + rng.below(50) as i64)
                };
                slot += 1;
                let mut d = order_decision(u64::from(slot), t);
                put(&mut d, 16, mid as u64, 8);
                put(&mut d, 24, qty as u64, 8);
                put(&mut d, 32, u64::from(if inst == I202 { G2 } else { G1 }), 4);
                put(&mut d, 36, u64::from(inst.0), 4);
                put(&mut d, 57, u64::from(if inst == I101 { S1.0 } else { P2.0 }), 1);
                put(&mut d, 60, if buy { 1 } else { 2 }, 1);
                g.rec(EventKind::BBO, t, bbo_px(inst, t, op, mid));
                g.rec(EventKind::ORDER_DECISION, t, d);
                g.end();
                orders.push(Ord {
                    slot,
                    inst,
                    buy,
                    qty,
                    cum: 0,
                    px: mid,
                    avg: 0,
                });
            }
            4..=6 if !orders.is_empty() => {
                // Fill or partial fill of an open order.
                let i = rng.below(orders.len() as u64) as usize;
                let o = &mut orders[i];
                let step = (o.qty - o.cum).min(o.qty / 2 + 1 + rng.below(o.qty as u64) as i64);
                o.cum += step;
                let full = o.cum >= o.qty;
                let venue = if o.inst == I101 { S1 } else { P2 };
                let mut e = order_event(
                    u64::from(o.slot),
                    t,
                    if full {
                        OrderEventKind::Fill
                    } else {
                        OrderEventKind::PartialFill
                    },
                );
                put(&mut e, 0, u64::from(venue.0), 1);
                put(&mut e, 12, u64::from(o.inst.0), 4);
                put(&mut e, 16, o.px as u64, 8);
                put(&mut e, 24, step as u64, 8);
                put(&mut e, 32, o.cum as u64, 8);
                put(&mut e, 40, rng.below(50_000) + 1, 8); // fee (USDT 1e-6)
                o.avg = o.px + rng.below(5) as i64;
                put(&mut e, 88, o.avg as u64, 8); // avg_px
                g.rec(EventKind::ORDER_EVENT, t, e);
                g.end();
                if full {
                    orders.remove(i);
                }
            }
            7 => {
                // External fill on a perpetual (liquidation / ADL), attributed by the ledger.
                let (inst, mid) = mids[1 + rng.below(2) as usize];
                let mut e = vec![0u8; 80];
                put(&mut e, 0, u64::from(P2.0), 1);
                put(&mut e, 1, u64::from(ACC.0), 1);
                put(&mut e, 2, 1 + rng.below(2), 1); // side
                put(&mut e, 3, 1 + rng.below(2), 1); // cause
                put(&mut e, 4, u64::from(inst.0), 4);
                put(
                    &mut e,
                    8,
                    (if inst == I202 { 10_000 } else { 100_000 }) * (1 + rng.below(5)),
                    8,
                );
                put(&mut e, 16, mid as u64, 8);
                put(&mut e, 24, rng.below(10_000), 8);
                put(&mut e, 32, u64::from(USDT.0), 2);
                trade_id += 1;
                put(&mut e, 48, trade_id, 8);
                put(&mut e, 56, t as u64, 8);
                g.rec(EventKind::EXTERNAL_FILL, t, e);
                g.end();
            }
            8 => {
                // Funding push for one perpetual at the last 8 h boundary.
                let inst = if rng.below(2) == 0 { I102 } else { I202 };
                let settle = (t / (8 * 3_600 * S)) * (8 * 3_600 * S);
                let mut f = vec![0u8; 56];
                put(&mut f, 0, u64::from(P2.0), 1);
                put(&mut f, 1, u64::from(ACC.0), 1);
                put(&mut f, 4, u64::from(inst.0), 4);
                put(&mut f, 8, (rng.below(2_000_000) as i64 - 1_000_000) as u64, 8); // amount
                put(&mut f, 16, 100_000, 8); // rate
                put(&mut f, 32, settle as u64, 8);
                g.rec(EventKind::FUNDING_SETTLE, t, f);
                g.end();
            }
            _ => {}
        }
        if rolls.iter().any(|r| op == r + 5) {
            // One keyframe snapshot (Output) in segments 2 and 3.
            t += S;
            let body = snapshot_body(g.jl.ledger(), 2, g.seq, t, &orders);
            g.rec(EventKind::STATE_SNAPSHOT, t, body);
            g.end();
        }
        if rolls.contains(&op) {
            // A new segment whose boot block restates the configuration and the current state.
            flush_part(&mut g, &mut script, &mut part);
            let snap = snapshot_body(g.jl.ledger(), 1, g.seq, t, &orders);
            script.push_str("boot-clear\n");
            for (k, b) in led_boot(&rd_blob, &eng, &sp, &snap) {
                script.push_str(&format!("boot {k:#06x} {}\n", hex(&b)));
            }
            script.push_str("roll\n");
            // End (1) + Begin, 5 boot records, BootDone (7).
            g.seq += 8;
        }
    }
    flush_part(&mut g, &mut script, &mut part);
    script.push_str("seal\n");
    std::fs::write(dir.join("script.txt"), script).unwrap_or_else(|e| die(&e.to_string()));
}

// ------------------------------------------------------------------------------------- ledger RUST_REF

/// Reference reading of the cold ledger (`RUST_REF`): hft-ledger's `JournalLedger` over the
/// journal from the first ledger snapshot at or after `from_seq` (or the first boot snapshot),
/// per-event PnL deltas of account 1's cells and the final views, as JSON.
fn ledger_ref(cold_root: &Path, instance: &str, from_seq: u64, out: &Path) {
    use hft_ledger::{JournalLedger, LedgerConfig, PnlBreakdown};
    let inst: InstanceName = instance.parse().unwrap_or_else(|_| die("instance"));
    let mut rc = ReaderConfig::new(
        cold_root.join(".nohot"),
        cold_root.to_path_buf(),
        inst,
        "seq".into(),
    );
    rc.start = StartAt::Oldest;
    rc.end = EndAt::LastCompleteEvent;
    let mut r = JournalReader::open(&rc).unwrap_or_else(|e| die(&format!("open: {e:?}")));
    let mut rd_blob: Option<Vec<u8>> = None;
    let mut eng: Option<Vec<u8>> = None;
    let mut sps: Vec<Vec<u8>> = Vec::new();
    let mut jl: Option<JournalLedger> = None;
    let mut rd: Option<hft_refdata::RefData> = None;
    let mut events: Vec<Value> = Vec::new();
    let mut first: Option<(u64, u64)> = None;
    let mut before: Vec<(u32, PnlBreakdown)> = Vec::new();
    let groups = [hft_types::GROUP_NONE.0, G1, G2];
    let pnl_json = |p: &PnlBreakdown| {
        json!([
            p.price_realized.0,
            p.funding.0,
            p.fee_cex.0,
            p.gas.0,
            p.bribe.0,
            p.unrealized.0,
            p.adjust.0,
            p.external.0
        ])
    };
    loop {
        let (seq, kind, tsc, flags, body): (u64, u16, u64, u16, Vec<u8>) = match r.poll() {
            Next::Record(v) => (v.hdr.seq, v.hdr.kind, v.hdr.tsc, v.hdr.flags, v.body.to_vec()),
            Next::Event(e) => (e.seq, e.kind.0, e.tsc, e.flags.0, e.body.to_vec()),
            Next::End | Next::Pending => break,
            Next::Gap(gp) => die(&format!("gap {gp:?}")),
            Next::Err(e) => die(&format!("read error {e:?}")),
        };
        let k = EventKind(kind);
        if jl.is_none() {
            if k == EventKind::CONFIG_BLOB && body.len() >= 72 {
                let bk = u16::from_le_bytes([body[0], body[1]]);
                let content = body[72..].to_vec();
                match bk {
                    1 => rd_blob = Some(content),
                    5 => eng = Some(content),
                    2 => sps.push(content),
                    _ => {}
                }
            }
            let ledger_snap = k == EventKind::STATE_SNAPSHOT && seq >= from_seq && body.len() > 96 && {
                let rr = hft_strategy_sdk::SnapshotReader::new(&body[96..]);
                matches!(rr.find_section(1), Ok(Some(_)))
            };
            let at_boot_done = from_seq == 0
                && k == EventKind::SEGMENT_ROLL
                && body.get(32) == Some(&3)
                && rd_blob.is_some();
            if ledger_snap || at_boot_done {
                let rdx = hft_refdata::RefData::load(rd_blob.as_deref().unwrap_or(&[]), None)
                    .unwrap_or_else(|e| die(&format!("refdata: {e:?}")));
                let cfg = eng
                    .as_deref()
                    .and_then(|b| hft_refdata::EngineConfig::load(b, None).ok())
                    .and_then(|ec| LedgerConfig::from_engine(&ec).ok())
                    .unwrap_or_default();
                let caps = led_caps(&rdx);
                let mut l = if ledger_snap {
                    let mut rr = hft_strategy_sdk::SnapshotReader::new(&body[96..]);
                    JournalLedger::from_snapshot(&mut rr, &rdx, &cfg, &caps)
                        .unwrap_or_else(|e| die(&format!("{e:?}")))
                } else {
                    let base =
                        hft_ledger::Ledger::new(&rdx, &cfg, &caps).unwrap_or_else(|e| die(&format!("{e:?}")));
                    let mut w = hft_strategy_sdk::SnapshotWriter::new(64 << 20);
                    base.snapshot(&mut w);
                    let bytes = w.as_bytes().to_vec();
                    let mut rr = hft_strategy_sdk::SnapshotReader::new(&bytes);
                    JournalLedger::from_snapshot(&mut rr, &rdx, &cfg, &caps)
                        .unwrap_or_else(|e| die(&format!("{e:?}")))
                };
                for spb in &sps {
                    let mut al = vec![0u64; spb.len().div_ceil(8)];
                    zerocopy::IntoBytes::as_mut_bytes(al.as_mut_slice())[..spb.len()].copy_from_slice(spb);
                    let b = &zerocopy::IntoBytes::as_bytes(al.as_slice())[..spb.len()];
                    if let Ok(v) = hft_event::parse_spb1(b) {
                        l.ledger_mut().on_groups_changed(
                            v.hdr.strategy,
                            &hft_strategy_sdk::GroupTable::new(v.groups, 1024),
                        );
                    }
                }
                rd = Some(rdx);
                jl = Some(l);
            }
            continue;
        }
        let Some(l) = jl.as_mut() else { continue };
        let proc_ns = D + (tsc / 3) as i64;
        if first.is_none() {
            first = Some((seq, proc_ns as u64));
            before = groups
                .iter()
                .map(|gg| (*gg, l.ledger().group(ACC, GroupId(*gg)).pnl))
                .collect();
        }
        let hdr = hft_event::RecordHeader {
            len: (24 + body.len()) as u32,
            kind,
            flags,
            seq,
            tsc,
        };
        l.apply(&hft_event::RecordView { hdr, body: &body })
            .expect("valid reference journal");
        if flags & hft_event::RecordFlags::EVENT_END.0 != 0 {
            let (s0, at) = first.take().unwrap_or((seq, 0));
            let mut cells = Vec::new();
            for (gg, p) in &before {
                let now = l.ledger().group(ACC, GroupId(*gg)).pnl;
                if now != *p {
                    let mut d = now;
                    d.price_realized.0 -= p.price_realized.0;
                    d.funding.0 -= p.funding.0;
                    d.fee_cex.0 -= p.fee_cex.0;
                    d.gas.0 -= p.gas.0;
                    d.bribe.0 -= p.bribe.0;
                    d.unrealized.0 -= p.unrealized.0;
                    d.adjust.0 -= p.adjust.0;
                    d.external.0 -= p.external.0;
                    cells.push(json!({"group": gg, "round": l.ledger().group(ACC, GroupId(*gg)).round.round_id, "d": pnl_json(&d)}));
                }
            }
            if !cells.is_empty() {
                events.push(json!({"seq": s0, "proc_ns": at, "cells": cells}));
            }
        }
    }
    let (Some(l), Some(rdx)) = (jl, rd) else {
        die("no ledger start point")
    };
    let led = l.ledger();
    let (pos, inv, port) = views_json(led, &rdx);
    let mut w = hft_strategy_sdk::SnapshotWriter::new(64 << 20);
    led.snapshot(&mut w);
    let doc = json!({"events": events, "positions": pos, "inventory": inv, "portfolio": port,
        "snapshot_hex": hex(w.as_bytes())});
    std::fs::write(out, doc.to_string()).unwrap_or_else(|e| die(&e.to_string()));
}

/// `positions()`, `inventory()` and `portfolio()` of account 1 as the hft-py tests compare them
/// (`RUST_REF` and the hot ledger of the `seq` fixtures).
pub fn views_json(
    led: &hft_ledger::Ledger,
    rdx: &hft_refdata::RefData,
) -> (Vec<Value>, Vec<Value>, Vec<Value>) {
    let mut pos = Vec::new();
    for v in [S1, P2] {
        for i in rdx.insts_of_venue(v) {
            let p = led.pos(ACC, *i);
            if p.qty.0 != 0 || p.cost != 0 || p.realized_quote != 0 || p.fees_quote != 0 {
                pos.push(json!({"venue": v.0, "inst": i.0, "qty": p.qty.0, "cost": p.cost.to_string(),
                    "realized_quote": p.realized_quote.to_string(), "fees_quote": p.fees_quote, "last_fill_ens": p.last_fill_ens}));
            }
        }
    }
    let mut inv = Vec::new();
    let mut port = Vec::new();
    for v in [S1, P2] {
        let b = led.venue_book(ACC, v);
        for e in b.inv.iter().filter(|e| e.qty.0 != 0) {
            inv.push(json!({"venue": v.0, "asset": e.asset.0, "group": e.group.0, "unassigned": false, "qty": e.qty.0}));
        }
        for e in b.cash.iter().filter(|e| e.unassigned.0 != 0) {
            inv.push(json!({"venue": v.0, "asset": e.asset.0, "group": hft_types::GROUP_NONE.0, "unassigned": true, "qty": e.unassigned.0}));
        }
        let s = b.summary;
        port.push(json!({"venue": s.venue.0, "flags": s.flags, "equity": s.equity.0, "quote_cash": s.quote_cash.0,
            "spot_value": s.spot_value.0, "perp_upnl": s.perp_upnl.0, "margin_balance": s.margin_balance.0, "as_of_ens": s.as_of_ens}));
    }
    (pos, inv, port)
}

fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let arg = |i: usize| {
        a.get(i)
            .map(String::as_str)
            .unwrap_or_else(|| die("missing argument"))
    };
    match arg(0) {
        "refdata" => {
            let b = refdata(arg(1) == "sim", 1);
            std::fs::write(arg(2), b).unwrap_or_else(|e| die(&e.to_string()));
        }
        "md" => md(
            arg(1) == "sim",
            arg(2).parse().unwrap_or_else(|_| die("n")),
            &PathBuf::from(arg(3)),
        ),
        "all" => all(&PathBuf::from(arg(1))),
        "cfg" => cfg(&PathBuf::from(arg(1))),
        "ref" => reference(&PathBuf::from(arg(1)), arg(2), &PathBuf::from(arg(3))),
        "led" => led(
            arg(1).parse().unwrap_or_else(|_| die("seed")),
            &PathBuf::from(arg(2)),
        ),
        "ledger-ref" => ledger_ref(
            &PathBuf::from(arg(1)),
            arg(2),
            arg(3).parse().unwrap_or_else(|_| die("from_seq")),
            &PathBuf::from(arg(4)),
        ),
        "seq" => seq::main(&a[1..]),
        "seq-check" => seq::check(&PathBuf::from(arg(1)), arg(2), &PathBuf::from(arg(3))),
        other => die(&format!("unknown command {other}")),
    }
}
