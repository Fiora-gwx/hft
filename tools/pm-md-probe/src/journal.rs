//! `pm-md-probe journal` (README "hft-core 验收"): reads an hft-core seq journal, follows the
//! `BindSlot` commands and rebuilds every slot inst's book from the journalled `L2Snapshot` /
//! `L2Batch` / `Trade` events. Writes the same JSON lines as the live probe, labelled per bound
//! window (`<asset>-<start_s>-<up|dn>`), and a bindings file in the `compare.py` format.

use std::io::Write;
use std::path::PathBuf;

use hft_event::{ControlCmd, EventKind, L2BatchHdr, L2SnapHdr, Trade};
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
    let mut binds = Vec::new();
    let mut reader = JournalReader::open_files(&a.files, 128 << 20).map_err(|e| format!("journal: {e:?}"))?;
    let mut out = std::io::BufWriter::new(
        std::fs::File::create(&a.out).map_err(|e| format!("{}: {e}", a.out.display()))?,
    );
    let (mut n_bind, mut n_snap, mut n_batch, mut n_trade) = (0u64, 0u64, 0u64, 0u64);
    loop {
        let (kind, body) = match reader.poll() {
            Next::Record(r) => (EventKind(r.hdr.kind), r.body),
            Next::Event(e) => (e.kind, e.body),
            Next::End | Next::Pending => break,
            Next::Gap(g) => return Err(format!("journal gap: {g:?}")),
            Next::Err(e) => return Err(format!("journal read: {e:?}")),
        };
        if kind == EventKind::CONTROL_CMD {
            let Ok((
                _,
                ControlCmd::BindSlot {
                    venue,
                    inst,
                    start_s,
                    wire_id,
                    ..
                },
            )) = ControlCmd::decode(body)
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
            n_bind += 1;
        } else if kind == EventKind::L2_SNAPSHOT {
            let Ok((h, rest)) = L2SnapHdr::read_from_prefix(body) else {
                continue;
            };
            let (nb, na) = (usize::from(h.n_bids), usize::from(h.n_asks));
            let Some(b) = books.get_mut(h.h.inst.0 as usize) else {
                continue;
            };
            b.bids.clear();
            b.asks.clear();
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
    println!("pm-md-probe journal: binds {n_bind}, snapshots {n_snap}, batches {n_batch}, trades {n_trade}");
    Ok(())
}
