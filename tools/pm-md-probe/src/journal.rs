//! `pm-md-probe journal` (README "hft-core 验收"): reads an hft-core seq journal, follows the
//! `BindSlot` commands and rebuilds every slot inst's book from the journalled `L2Snapshot` /
//! `L2Batch` / `Trade` events. Writes the same JSON lines as the live probe, labelled per bound
//! window (`<asset>-<start_s>-<up|dn>`), and a bindings file in the `compare.py` format. Order
//! decisions, order events, external fills (settlement deliveries), `Settle` commands, command
//! results and connection states are written too (README "产出").

use std::io::Write;
use std::path::PathBuf;

use hft_event::{
    CmdResult, ConnState, ControlCmd, EventKind, ExternalFill, L2BatchHdr, L2SnapHdr, OrderDecision,
    OrderEvent, Trade,
};
use hft_journal::{JournalReader, Next};
use hft_refdata::RefData;
use hft_types::{InstId, Side, VenueId};
use venue_polymarket::FeedCtx;
use zerocopy::{FromBytes, TryFromBytes};

use crate::{Book, levels};

struct Args {
    refdata: PathBuf,
    out: PathBuf,
    bindings_out: PathBuf,
    files: Vec<PathBuf>,
}

fn usage() -> String {
    "usage: pm-md-probe journal --refdata RDB1 --out FILE --bindings-out FILE SEG...".into()
}

fn args(raw: &[String]) -> Result<Args, String> {
    let (mut refdata, mut out, mut bindings_out, mut files) = (None, None, None, Vec::new());
    let mut a = raw.iter();
    while let Some(k) = a.next() {
        match k.as_str() {
            "--refdata" => refdata = a.next().map(PathBuf::from),
            "--out" => out = a.next().map(PathBuf::from),
            "--bindings-out" => bindings_out = a.next().map(PathBuf::from),
            f if !f.starts_with("--") => files.push(PathBuf::from(f)),
            _ => return Err(usage()),
        }
    }
    match (refdata, out, bindings_out) {
        (Some(refdata), Some(out), Some(bindings_out)) if !files.is_empty() => Ok(Args {
            refdata,
            out,
            bindings_out,
            files,
        }),
        _ => Err(usage()),
    }
}

/// Lower-case hex of a 16-byte id.
fn hex16(b: &[u8; 16]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

/// `btc-up-0` → (`btc`, `up`).
fn asset_side(symbol: &str) -> (&str, &str) {
    let mut p = symbol.split('-');
    (p.next().unwrap_or("?"), p.next().unwrap_or("?"))
}

pub fn run(raw: &[String]) -> Result<(), String> {
    let a = args(raw)?;
    let blob = std::fs::read(&a.refdata).map_err(|e| format!("{}: {e}", a.refdata.display()))?;
    let rd = RefData::load(&blob, None).map_err(|e| format!("refdata: {e:?}"))?;
    let n = rd.inst_count();
    let mut ctx = FeedCtx::with_capacity(n);
    let mut label: Vec<String> = vec!["?".into(); n + 1];
    let mut books: Vec<Book> = (0..=n).map(|_| Book::default()).collect();
    // After a bind the engine's book resyncs: increments wait for the token's first book.
    let mut awaiting = vec![false; n + 1];
    let mut binds = Vec::new();
    let mut reader = JournalReader::open_files(&a.files, 128 << 20).map_err(|e| format!("journal: {e:?}"))?;
    let mut out = std::io::BufWriter::new(
        std::fs::File::create(&a.out).map_err(|e| format!("{}: {e}", a.out.display()))?,
    );
    let (mut n_bind, mut n_snap, mut n_batch, mut n_trade) = (0u64, 0u64, 0u64, 0u64);
    let (mut n_dec, mut n_oev, mut n_ext, mut n_settle, mut n_conn) = (0u64, 0u64, 0u64, 0u64, 0u64);
    let name = |label: &[String], i: InstId| -> String {
        label
            .get(i.0 as usize)
            .filter(|l| l.as_str() != "?")
            .cloned()
            .unwrap_or_else(|| format!("inst{}", i.0))
    };
    loop {
        let (kind, body) = match reader.poll() {
            Next::Record(r) => (EventKind(r.hdr.kind), r.body),
            Next::Event(e) => (e.kind, e.body),
            Next::End | Next::Pending => break,
            Next::Gap(g) => return Err(format!("journal gap: {g:?}")),
            Next::Err(e) => return Err(format!("journal read: {e:?}")),
        };
        if kind == EventKind::CONTROL_CMD {
            let decoded = ControlCmd::decode(body);
            if let Ok((
                h,
                ControlCmd::Settle {
                    inst, end_s, payout, ..
                },
            )) = decoded
            {
                n_settle += 1;
                let _ = writeln!(
                    out,
                    "{{\"k\":\"settle\",\"slot\":\"{}\",\"end_s\":{end_s},\"payout\":{},\"cmd_id\":\"{}\"}}",
                    name(&label, inst),
                    payout.0,
                    hex16(&h.cmd_id)
                );
                continue;
            }
            let Ok((
                _,
                ControlCmd::BindSlot {
                    venue,
                    inst,
                    start_s,
                    wire_id,
                    ..
                },
            )) = decoded
            else {
                continue;
            };
            if venue != VenueId::POLYMARKET_CLOB || inst.0 as usize > n {
                continue;
            }
            ctx.bind_wire(inst, &wire_id)
                .map_err(|e| format!("bind {}: {e:?}", inst.0))?;
            let (asset, side) = asset_side(&rd.inst_meta(inst).symbol);
            let slot = format!("{asset}-{start_s}-{side}");
            if let Some(token) = ctx.token_of(inst) {
                binds.push(format!(
                    "{{\"slot\":\"{slot}\",\"token\":\"{}\",\"start_ts\":{start_s},\"asset\":\"{asset}\"}}",
                    String::from_utf8_lossy(token)
                ));
            }
            label[inst.0 as usize] = slot;
            if let Some(b) = books.get_mut(inst.0 as usize) {
                *b = Book::default();
            }
            if let Some(w) = awaiting.get_mut(inst.0 as usize) {
                *w = true;
            }
            n_bind += 1;
        } else if kind == EventKind::L2_SNAPSHOT {
            let Ok((h, rest)) = L2SnapHdr::read_from_prefix(body) else {
                continue;
            };
            // Keyframe copies of the engine's books are state, not market data.
            if h.origin != L2SnapHdr::EXCHANGE {
                continue;
            }
            let (nb, na) = (usize::from(h.n_bids), usize::from(h.n_asks));
            let Some(b) = books.get_mut(h.h.inst.0 as usize) else {
                continue;
            };
            b.bids.clear();
            b.asks.clear();
            if let Some(w) = awaiting.get_mut(h.h.inst.0 as usize) {
                *w = false;
            }
            for (k, l) in levels(rest, nb + na).iter().enumerate() {
                let side = if k < nb { &mut b.bids } else { &mut b.asks };
                side.insert(l.px.0, l.qty.0);
            }
            n_snap += 1;
            let (bb, ba) = b.best();
            let _ = writeln!(
                out,
                "{{\"k\":\"snap\",\"slot\":\"{}\",\"ex_ms\":{},\"rx_ns\":{},\"id\":{},\"bb\":{},\"ba\":{}}}",
                label[h.h.inst.0 as usize],
                h.h.ex_ts_ns / 1_000_000,
                h.h.sw_rx_ns,
                h.snap_id,
                bb.unwrap_or(-1),
                ba.unwrap_or(-1)
            );
        } else if kind == EventKind::L2_BATCH {
            let Ok((h, rest)) = L2BatchHdr::read_from_prefix(body) else {
                continue;
            };
            if awaiting.get(h.h.inst.0 as usize).copied().unwrap_or(true) {
                continue;
            }
            let (nb, na) = (usize::from(h.n_bids), usize::from(h.n_asks));
            let Some(b) = books.get_mut(h.h.inst.0 as usize) else {
                continue;
            };
            for (k, l) in levels(rest, nb + na).iter().enumerate() {
                let side = if k < nb { &mut b.bids } else { &mut b.asks };
                if l.qty.0 == 0 {
                    side.remove(&l.px.0);
                } else {
                    side.insert(l.px.0, l.qty.0);
                }
            }
            n_batch += 1;
            let (bb, ba) = b.best();
            let _ = writeln!(
                out,
                "{{\"k\":\"batch\",\"slot\":\"{}\",\"ex_ms\":{},\"rx_ns\":{},\"prev\":{},\"last\":{},\"bb\":{},\"ba\":{}}}",
                label[h.h.inst.0 as usize],
                h.h.ex_ts_ns / 1_000_000,
                h.h.sw_rx_ns,
                h.prev_id,
                h.last_id,
                bb.unwrap_or(-1),
                ba.unwrap_or(-1)
            );
        } else if kind == EventKind::ORDER_DECISION {
            let Ok((d, _)) = OrderDecision::try_read_from_prefix(body) else {
                continue;
            };
            n_dec += 1;
            let _ = writeln!(
                out,
                "{{\"k\":\"dec\",\"slot\":\"{}\",\"action\":{},\"side\":\"{}\",\"tif\":{},\"px\":{},\"qty\":{},\"oslot\":{},\"gen\":{},\"reject\":{}}}",
                name(&label, d.inst),
                d.action as u8,
                if d.side == Side::Buy { "BUY" } else { "SELL" },
                d.tif as u8,
                d.px.0,
                d.qty.0,
                d.slot.0,
                d.r#gen,
                d.reject
            );
        } else if kind == EventKind::ORDER_EVENT {
            let Ok((e, _)) = OrderEvent::try_read_from_prefix(body) else {
                continue;
            };
            n_oev += 1;
            let _ = writeln!(
                out,
                "{{\"k\":\"oev\",\"slot\":\"{}\",\"what\":{},\"px\":{},\"qty\":{},\"cum\":{},\"fee\":{},\"oslot\":{},\"gen\":{},\"ex_ms\":{},\"code\":{}}}",
                name(&label, e.inst),
                e.what as u8,
                e.px.0,
                e.qty.0,
                e.cum_filled.0,
                e.fee.0,
                e.slot.0,
                e.r#gen,
                e.ex_ts_ns / 1_000_000,
                e.reject_code
            );
        } else if kind == EventKind::EXTERNAL_FILL {
            let Ok((e, _)) = ExternalFill::try_read_from_prefix(body) else {
                continue;
            };
            n_ext += 1;
            let _ = writeln!(
                out,
                "{{\"k\":\"ext\",\"slot\":\"{}\",\"cause\":{},\"side\":\"{}\",\"px\":{},\"qty\":{},\"end_s\":{}}}",
                name(&label, e.inst),
                e.cause as u8,
                if e.side == Side::Buy { "BUY" } else { "SELL" },
                e.px.0,
                e.qty.0,
                e.exch_order_id
            );
        } else if kind == EventKind::CMD_RESULT {
            let Ok((r, _)) = CmdResult::try_read_from_prefix(body) else {
                continue;
            };
            let _ = writeln!(
                out,
                "{{\"k\":\"cmdres\",\"cmd_id\":\"{}\",\"status\":{},\"detail\":{}}}",
                hex16(&r.cmd_id),
                r.status,
                r.detail
            );
        } else if kind == EventKind::CONN_STATE {
            let Ok((c, _)) = ConnState::try_read_from_prefix(body) else {
                continue;
            };
            n_conn += 1;
            let _ = writeln!(
                out,
                "{{\"k\":\"conn\",\"venue\":{},\"conn\":{},\"state\":{},\"inst\":{},\"reason\":{},\"role\":{}}}",
                c.venue.0, c.conn, c.state as u8, c.inst.0, c.reason, c.role as u8
            );
        } else if kind == EventKind::TRADE {
            let Ok((t, _)) = Trade::try_read_from_prefix(body) else {
                continue;
            };
            if (t.h.inst.0 as usize) > n || t.h.inst == InstId(0) {
                continue;
            }
            n_trade += 1;
            let _ = writeln!(
                out,
                "{{\"k\":\"trade\",\"slot\":\"{}\",\"ex_ms\":{},\"rx_ns\":{},\"px\":{},\"qty\":{},\"side\":\"{}\"}}",
                label[t.h.inst.0 as usize],
                t.h.ex_ts_ns / 1_000_000,
                t.h.sw_rx_ns,
                t.px.0,
                t.qty.0,
                if t.taker_side == Side::Buy { "BUY" } else { "SELL" }
            );
        }
    }
    let _ = out.flush();
    std::fs::write(&a.bindings_out, format!("[{}]\n", binds.join(",\n")))
        .map_err(|e| format!("{}: {e}", a.bindings_out.display()))?;
    println!(
        "pm-md-probe journal: binds {n_bind}, snapshots {n_snap}, batches {n_batch}, trades {n_trade}, \
         decisions {n_dec}, order events {n_oev}, external fills {n_ext}, settles {n_settle}, conn states {n_conn}"
    );
    Ok(())
}
