//! venue-polymarket order signing benchmarks (README "延迟与资源契约"; PMK-036): the gw's
//! `encode_rest_ctx(Place)` signing at the decision, the same encode with a pre-signed order
//! ready, and one pre-signing step of the gw's idle time.
#![allow(clippy::disallowed_types, clippy::disallowed_methods)] // bench setup and timing only

use std::hint::black_box;
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use criterion::{Criterion, criterion_group, criterion_main};
use hft_event::{GwCmd, GwOp};
use hft_refdata::{
    AssetClass, AssetSpec, Exchange, FeePair, FeeSpec, InstKind, InstSpec, InstStatus, RdbBuilder, RefData,
    StaleGuardKind, TsUnit, VenueSpec,
};
use hft_types::{
    AccountId, AssetExp, AssetId, ClientOrderId, InstId, Notional, Px, Qty, Rate, Side, SlotId, Tif, VenueId,
};
use hft_venue_proto::{
    ArrayBuf, Credential, RestReqBuf, SecretBuf, SignAlg, TargetRef, VenueCfg, VenueEnv, VenueProtocol,
    WallClock,
};
use venue_polymarket::sign::parse_u256;
use venue_polymarket::{GwCtx, PolymarketClob as Pm, Presign};

const V: VenueId = VenueId::POLYMARKET_CLOB;
const UP: InstId = InstId(1);
const T_UP: &str = "93260813640452160844731182437467962294350968265707341153474211213683765578930";
/// Test-only key of the signing golden (never a real key).
const KEY: &str = "0x1111111111111111111111111111111111111111111111111111111111111111";
const WALLET: &str = "0x7cB001deBC0984d5a9e796dd5dc35676d97A8cB2";
const NOW_MS: i64 = 1_792_687_605_366;

fn rd() -> &'static RefData {
    static RD: OnceLock<&'static RefData> = OnceLock::new();
    RD.get_or_init(|| {
        let mut b = RdbBuilder::new(1);
        b.created_utc_ns(1_759_276_800_000_000_000);
        for (id, name, class, px, qty) in [
            (AssetId(1), "USDC", AssetClass::Stable, -6, -6),
            (AssetId(2), "PMBTCUP0", AssetClass::Crypto, -3, -6),
        ] {
            b.asset(AssetSpec {
                id,
                name: name.into(),
                class,
                exp: AssetExp {
                    px_exp: px,
                    qty_exp: qty,
                },
                px_max: Px(1_000_000_000),
                qty_max: Qty(1_000_000_000_000_000),
                dust: Qty(1),
            });
        }
        b.venue(VenueSpec {
            id: V,
            exchange: Exchange::Polymarket,
            product: InstKind::Spot,
            ws_max_streams_per_conn: 500,
            ws_max_in_msgs_per_s: 10,
            ws_max_conn_age_s: 86_400,
            ws_new_conn_per_5min_ip: 300,
            app_ping_interval_s: 10,
            ts_unit: TsUnit::Ms,
            client_id_max_len: 32,
            client_id_prefix: "".into(),
            max_batch: 1,
            max_inflight_amend: 1,
            crossed_tolerance_default: 0,
            stale_guard: StaleGuardKind::ExpireAt,
        });
        b.fee(FeeSpec {
            venue: V,
            tier: 0,
            fee: FeePair {
                maker: Rate(0),
                taker: Rate(0),
            },
        });
        b.caps(Pm::default_caps(|_| None));
        b.inst(InstSpec {
            id: UP,
            venue: V,
            symbol: "btc-up-0".into(),
            status: InstStatus::Trading,
            kind: InstKind::Spot,
            asset: AssetId(2),
            quote: AssetId(1),
            tick: 10,
            lot: 1,
            min_qty: Qty(1),
            max_qty: Qty(1_000_000_000_000),
            min_notional: Notional(1_000_000),
            book_window: 128,
            l2_depth: 100,
            crossed_tolerance: 0,
            tick_str: "0.01".into(),
            lot_str: "0.000001".into(),
            min_qty_str: "0.000001".into(),
            ..InstSpec::default()
        });
        let blob = b.finish();
        Box::leak(Box::new(RefData::load(&blob, None).expect("bench refdata")))
    })
}

fn cred() -> Credential {
    let secret = format!("dGhpc2lzYXRlc3RzZWNyZXRrZXkxMjM0NTY3ODkwYWI=|{KEY}|{WALLET}");
    Credential {
        api_key: ArrayBuf::from_bytes(b"0f5e8d2a-1111-2222-3333-444455556666").unwrap(),
        secret: SecretBuf(ArrayBuf::from_bytes(secret.as_bytes()).unwrap()),
        passphrase: ArrayBuf::from_bytes(b"test-passphrase").unwrap(),
        alg: SignAlg::Secp256k1,
    }
}

fn place() -> GwCmd {
    let mut c = GwCmd::sim_reset(AccountId(1));
    c.op = GwOp::Place;
    c.inst = UP;
    c.side = Side::Buy;
    c.px = Px(420);
    c.qty = Qty(5_000_000);
    c.tif = Tif::PostOnly;
    c.slot = SlotId(7);
    c.r#gen = 3;
    c
}

fn tgt() -> TargetRef {
    TargetRef {
        id: ClientOrderId {
            epoch: 5,
            venue: V,
            account: AccountId(1),
            slot: 7,
            r#gen: 3,
        },
        exch_order_id: 0,
    }
}

fn ctx(presign: Option<Presign>) -> GwCtx {
    let mut c = GwCtx::default();
    Pm::bind_order_wire(&mut c, UP, &parse_u256(T_UP).unwrap()).unwrap();
    if let Some(p) = presign {
        c.set_presign(p);
    }
    c
}

fn clk(ms: i64) -> WallClock {
    WallClock {
        unix_ms: ms,
        unix_us: ms * 1000,
    }
}

fn pmk_036_signing(c: &mut Criterion) {
    let r = rd();
    let cfg = VenueCfg {
        env: VenueEnv::Prod,
        stale_guard_ms: 0,
    };
    let cred = cred();
    let tpl = Pm::build_templates(&cfg, r, AccountId(1), &cred).unwrap();
    let cmd = place();
    let t = tgt();
    // Signing at the decision (pre-signing off).
    let mut g = ctx(None);
    let mut out = RestReqBuf::default();
    let mut ms = NOW_MS;
    c.bench_function("pm/encode_place_signed", |b| {
        b.iter(|| {
            ms += 1;
            Pm::encode_rest_ctx(r, &tpl, &mut g, &cred, black_box(&cmd), &t, &clk(ms), &mut out)
        })
    });
    // A cancel by hash (no order signing: the wallet key is not derived).
    let mut cancel = place();
    cancel.op = GwOp::Cancel;
    c.bench_function("pm/encode_cancel", |b| {
        b.iter(|| {
            ms += 1;
            Pm::encode_rest_ctx(r, &tpl, &mut g, &cred, black_box(&cancel), &t, &clk(ms), &mut out)
        })
    });
    // A pre-signed order ready at the placed price: the encode fills the body only. Each
    // iteration consumes one; the idle step that refills it is outside the timed closure.
    let mut g = ctx(Presign::new(0, 0, 10_000, 240_000));
    let mut ms = NOW_MS;
    // The first place sets the price anchor; the idle step then signs the order for it.
    Pm::encode_rest_ctx(r, &tpl, &mut g, &cred, &cmd, &t, &clk(ms), &mut out).unwrap();
    c.bench_function("pm/encode_place_presigned", |b| {
        b.iter_custom(|iters| {
            let mut spent = Duration::ZERO;
            for _ in 0..iters {
                ms += 1;
                while Pm::gw_idle(r, &tpl, &mut g, &cred, &clk(ms)) {}
                let t0 = Instant::now();
                let _ = black_box(Pm::encode_rest_ctx(
                    r,
                    &tpl,
                    &mut g,
                    &cred,
                    black_box(&cmd),
                    &t,
                    &clk(ms),
                    &mut out,
                ));
                spent += t0.elapsed();
            }
            spent
        })
    });
    // One pre-signing step (one order signed in the gw's idle time).
    let mut g = ctx(Presign::new(0, 0, 10_000, 240_000));
    let mut ms = NOW_MS;
    Pm::encode_rest_ctx(r, &tpl, &mut g, &cred, &cmd, &t, &clk(ms), &mut out).unwrap();
    c.bench_function("pm/presign_step", |b| {
        b.iter_custom(|iters| {
            let mut spent = Duration::ZERO;
            for _ in 0..iters {
                ms += 1;
                // Use up the ready order so the step has one to sign.
                let _ = Pm::encode_rest_ctx(r, &tpl, &mut g, &cred, &cmd, &t, &clk(ms), &mut out);
                let t0 = Instant::now();
                black_box(Pm::gw_idle(r, &tpl, &mut g, &cred, &clk(ms)));
                spent += t0.elapsed();
            }
            spent
        })
    });
}

criterion_group!(benches, pmk_036_signing);
criterion_main!(benches);
