//! BIN-074: every encoded frame parses back with the command's fields.
#![allow(clippy::disallowed_types)] // serde_json::Value: reference parser of the frames under test

use crate::support;

use hft_event::{GwCmd, GwOp, OrderFlags};
use hft_refdata::RefData;
use hft_types::{ClientOrderId, InstId, Px, Qty, Side, SlotId, Tif, Tsc, VenueId, write_fixed};
use hft_venue_proto::{TargetRef, VenueProtocol};
use proptest::prelude::*;
use support::{ACCT, WC, cfg, hm1, rd};
use venue_binance::{Spot, Templates, Usdm, decode_req_id};

fn dec(raw: i64, exp: i8) -> String {
    let mut b = [0u8; 40];
    let n = write_fixed(raw, exp, &mut b);
    String::from_utf8(b[..n].to_vec()).unwrap()
}

#[allow(clippy::too_many_arguments)]
fn check<P: VenueProtocol<Templates = Templates>>(
    r: &RefData,
    inst: InstId,
    op: GwOp,
    side: Side,
    tif: Tif,
    px_steps: i64,
    qty_steps: i64,
    slot: u32,
    r#gen: u32,
    reduce: bool,
) -> Result<(), TestCaseError> {
    let row = r.inst(inst);
    let px = Px((px_steps * row.tick).min(1_000_000_000_000_000));
    let qty = Qty((qty_steps * row.lot).min(1_000_000_000_000_000));
    let c = GwCmd {
        decision_seq: 0,
        decided_tsc: Tsc(0),
        px,
        qty,
        inst,
        slot: SlotId(slot),
        r#gen,
        target_slot: SlotId(slot),
        op,
        account: ACCT,
        side,
        tif,
        oflags: if reduce { OrderFlags::REDUCE_ONLY.0 } else { 0 },
        _pad: [0; 2],
        arg: 0,
        arg2: 0,
    };
    let id = ClientOrderId {
        epoch: 3,
        venue: P::VENUE,
        account: ACCT,
        slot,
        r#gen,
    };
    let t = P::build_templates(&cfg(1500), r, ACCT, &hm1()).unwrap();
    let mut body = [0u8; 2048];
    let n = P::encode(r, &t, &c, &TargetRef { id, exch_order_id: 0 }, &WC, &mut body).unwrap();
    prop_assert!(n <= 512);
    let v: serde_json::Value = serde_json::from_slice(&body[..n]).unwrap();
    let p = &v["params"];
    prop_assert_eq!(p["symbol"].as_str().unwrap(), &*r.inst_meta(inst).symbol);
    let req = decode_req_id(v["id"].as_str().unwrap().as_bytes()).unwrap();
    prop_assert_eq!((req.op, req.slot, req.r#gen), (op, slot, r#gen));
    let spot = P::VENUE == VenueId::BINANCE_SPOT;
    match op {
        GwOp::Place => {
            prop_assert_eq!(
                p["side"].as_str().unwrap(),
                if side == Side::Buy { "BUY" } else { "SELL" }
            );
            prop_assert_eq!(p["price"].as_str().unwrap(), dec(px.0, row.px_wire_exp));
            prop_assert_eq!(p["quantity"].as_str().unwrap(), dec(qty.0, row.qty_wire_exp));
            match (spot, tif) {
                (true, Tif::PostOnly) => {
                    prop_assert_eq!(p["type"].as_str().unwrap(), "LIMIT_MAKER");
                    prop_assert!(p.get("timeInForce").is_none());
                }
                (_, t) => {
                    let want = match t {
                        Tif::Gtc => "GTC",
                        Tif::Ioc => "IOC",
                        Tif::Fok => "FOK",
                        Tif::PostOnly => "GTX",
                    };
                    prop_assert_eq!(p["type"].as_str().unwrap(), "LIMIT");
                    prop_assert_eq!(p["timeInForce"].as_str().unwrap(), want);
                }
            }
            prop_assert_eq!(p.get("reduceOnly").is_some(), reduce);
        }
        GwOp::Amend if spot => {
            prop_assert_eq!(p["newQty"].as_str().unwrap(), dec(qty.0, row.qty_wire_exp));
        }
        GwOp::Amend => {
            prop_assert_eq!(p["price"].as_str().unwrap(), dec(px.0, row.px_wire_exp));
            prop_assert_eq!(p["quantity"].as_str().unwrap(), dec(qty.0, row.qty_wire_exp));
        }
        _ => {}
    }
    Ok(())
}

fn tifs() -> impl Strategy<Value = Tif> {
    prop::sample::select(vec![Tif::Gtc, Tif::Ioc, Tif::Fok, Tif::PostOnly])
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(2_000))]
    #[test]
    fn bin_074_encode_parses_back(
        spot in any::<bool>(),
        pick in 0usize..4,
        op in prop::sample::select(vec![GwOp::Place, GwOp::Cancel, GwOp::Amend, GwOp::Query]),
        buy in any::<bool>(),
        tif in tifs(),
        px_steps in 1i64..1_000_000_000,
        qty_steps in 1i64..1_000_000_000,
        slot in 1u32..(1 << 20),
        r#gen in any::<u32>(),
        reduce in any::<bool>(),
    ) {
        let r = rd();
        let side = if buy { Side::Buy } else { Side::Sell };
        if spot {
            let inst = [InstId(2001), InstId(2002)][pick % 2];
            check::<Spot>(r, inst, op, side, tif, px_steps, qty_steps, slot, r#gen, false)?;
        } else {
            let inst = [InstId(1042), InstId(1043), InstId(1), InstId(450)][pick];
            check::<Usdm>(r, inst, op, side, tif, px_steps, qty_steps, slot, r#gen, reduce)?;
        }
    }
}
