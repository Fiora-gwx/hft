//! `pm-md-probe journal` (README "hft-core 验收"): reads an hft-core seq journal, follows the
//! `BindSlot` commands and rebuilds every slot inst's book from the journalled `L2Snapshot` /
//! `L2Batch` / `Trade` events. Writes the same JSON lines as the live probe, labelled per bound
//! window (`<asset>-<start_s>-<up|dn>`), and a bindings file in the `compare.py` format. Order
//! decisions, order events, external fills (settlement deliveries), `Settle` commands, command
//! results and connection states are written too (README "产出"). `--orders-only` skips the book
//! rebuild and the market lines and writes one `bind` line per binding instead, so a reader can
//! decode only the newest segments and keep the inst → window labels itself.

use std::io::Write;
use std::path::PathBuf;

use hft_event::{
    Bbo, CmdResult, ConnState, ControlCmd, EngineClock, EventKind, ExternalFill, L2BatchHdr, L2SnapHdr,
    OrderDecision, OrderEvent, Trade, UtcAnchor,
};
use hft_journal::{JournalReader, Next};
use hft_refdata::RefData;
use hft_sequencer::clock::UtcLineRow;
use hft_strategy_sdk::EngineClockAnchor;
use hft_types::{InstId, Side, Tsc, VenueId};
use venue_polymarket::FeedCtx;
use zerocopy::{FromBytes, TryFromBytes};

use crate::{Book, levels};

struct Args {
    refdata: PathBuf,
    out: PathBuf,
    bindings_out: PathBuf,
    orders_only: bool,
    files: Vec<PathBuf>,
}

fn usage() -> String {
    "usage: pm-md-probe journal --refdata RDB1 --out FILE --bindings-out FILE [--orders-only] SEG...".into()
}

fn args(raw: &[String]) -> Result<Args, String> {
    let (mut refdata, mut out, mut bindings_out, mut files) = (None, None, None, Vec::new());
    let mut orders_only = false;
    let mut a = raw.iter();
    while let Some(k) = a.next() {
        match k.as_str() {
            "--refdata" => refdata = a.next().map(PathBuf::from),
            "--out" => out = a.next().map(PathBuf::from),
            "--bindings-out" => bindings_out = a.next().map(PathBuf::from),
            "--orders-only" => orders_only = true,
            f if !f.starts_with("--") => files.push(PathBuf::from(f)),
            _ => return Err(usage()),
        }
    }
    match (refdata, out, bindings_out) {
        (Some(refdata), Some(out), Some(bindings_out)) if !files.is_empty() => Ok(Args {
            refdata,
            out,
            bindings_out,
            orders_only,
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

/// The segment pair converted at rate `hz` (SDK `EngineClockAnchor::utc_of`, the formula the
/// sequencer uses before the run's first `UtcAnchor`).
fn seg_utc(t: u64, calib_tsc: u64, calib_realtime_ns: i64, hz: u64) -> i64 {
    EngineClockAnchor {
        base_engine_ns: 0,
        base_tsc: Tsc(0),
        tsc_hz_milli: hz,
        utc_tsc: Tsc(calib_tsc),
        utc_ns: calib_realtime_ns,
    }
    .utc_of(Tsc(t))
}

/// Strategy-visible engine UTC (SDK `Now.utc_ns`; hft-sequencer README "引擎 UTC"):
/// `Engine::utc_at(t)` = `EngineClock::anchor_at(t, seg pair).utc_of(t)`. The line itself is
/// hft-sequencer's `UtcLineRow` (one implementation of the anchoring and slew rules).
#[derive(Default)]
struct EngineUtc {
    /// `EngineClock.tsc_hz_milli` of the current run; 0 = no `EngineClock` read yet (the segment
    /// header's `tsc_hz_milli` stands in: hft-core writes the same value into both).
    hz: u64,
    line: Option<UtcLineRow>,
}

impl EngineUtc {
    fn hz_or(&self, seg_hz: u64) -> u64 {
        if self.hz != 0 { self.hz } else { seg_hz }
    }
    /// Engine UTC of record tsc `t`; `seg` = (calib_tsc, calib_realtime_ns, tsc_hz_milli) of the
    /// segment header.
    fn at(&self, t: u64, seg: Option<(u64, i64, u64)>) -> i64 {
        match (&self.line, seg) {
            (Some(l), _) => l.utc_at(Tsc(t)),
            (None, Some((ct, cr, shz))) => seg_utc(t, ct, cr, self.hz_or(shz)),
            (None, None) => 0,
        }
    }
    /// `EngineClock` Input: new run, UTC line cleared.
    fn engine_clock(&mut self, hz: u64) {
        self.hz = hz;
        self.line = None;
    }
    /// `UtcAnchor` Input at record tsc `t` (`UtcLineRow::anchored` with `cur` = engine UTC at `t`
    /// before the anchor). Returns (cur, new line).
    fn anchor(&mut self, t: u64, a: &UtcAnchor, seg: Option<(u64, i64, u64)>) -> (i64, UtcLineRow) {
        let cur = self.at(t, seg);
        let l = UtcLineRow::anchored(cur, Tsc(t), a, self.hz_or(seg.map_or(0, |s| s.2)));
        // A zero-rate line would be ClockRange in the engine; keep the previous state.
        if l.is_set() {
            self.line = Some(l);
        }
        (cur, l)
    }
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
    // Engine receive time of the last market event (decisions and order
    // events carry it as t_ns), and the level deltas of every book record.
    let mut last_rx: i64 = 0;
    let lv = |ls: &[hft_event::Level], nb: usize| -> String {
        let mut s = String::from("[");
        for (k, l) in ls.iter().enumerate() {
            if k > 0 {
                s.push(',');
            }
            s.push_str(&format!(
                "[{},{},{}]",
                if k < nb { 1 } else { 2 },
                l.px.0,
                l.qty.0
            ));
        }
        s.push(']');
        s
    };
    let (mut n_dec, mut n_oev, mut n_ext, mut n_settle, mut n_conn) = (0u64, 0u64, 0u64, 0u64, 0u64);
    let name = |label: &[String], i: InstId| -> String {
        label
            .get(i.0 as usize)
            .filter(|l| l.as_str() != "?")
            .cloned()
            .unwrap_or_else(|| format!("inst{}", i.0))
    };
    // Strategy-visible engine UTC (`eu`), rebuilt from EngineClock / UtcAnchor inputs.
    let mut eutc = EngineUtc::default();
    let (mut n_eclk, mut n_anchor, mut n_anchor_noclk) = (0u64, 0u64, 0u64);
    loop {
        // UTC calibration pair of the segment being read (the first record of a new segment still
        // uses the previous one).
        let calib = reader
            .segment_header()
            .map(|h| (h.calib_tsc, h.calib_realtime_ns, h.tsc_hz_milli));
        let (kind, body, cur_tsc, cur_seq) = match reader.poll() {
            Next::Record(r) => (EventKind(r.hdr.kind), r.body, r.hdr.tsc, r.hdr.seq),
            Next::Event(e) => (e.kind, e.body, e.tsc, e.seq),
            Next::End | Next::Pending => break,
            Next::Gap(g) => return Err(format!("journal gap: {g:?}")),
            Next::Err(e) => return Err(format!("journal read: {e:?}")),
        };
        // Engine-time scale of the record tsc (tsc × 10^12 / tsc_hz_milli; differences are engine_ns differences).
        let cur_e = calib.map_or(0, |(_, _, hz)| {
            (i128::from(cur_tsc) * 1_000_000_000_000 / i128::from(hz.max(1))) as i64
        });
        let cur_u = calib.map_or(0, |(ct, cr, hz)| {
            let d = i128::from(cur_tsc) - i128::from(ct);
            cr + (d * 1_000_000_000_000 / i128::from(hz.max(1))) as i64
        });
        // Engine clock inputs act before the record's own Now (process.rs: EngineClock sets the
        // base first; UtcAnchor recomputes Now on the new line).
        if kind == EventKind::ENGINE_CLOCK {
            if let Ok((c, _)) = EngineClock::read_from_prefix(body) {
                eutc.engine_clock(c.tsc_hz_milli);
                n_eclk += 1;
                let _ = writeln!(
                    out,
                    "{{\"k\":\"eclk\",\"u\":{cur_u},\"eu\":{},\"sq\":{cur_seq},\"e\":{cur_e},\"base\":{},\"hz\":{},\"prev\":{},\"src\":{}}}",
                    eutc.at(cur_tsc, calib),
                    c.base_engine_ns,
                    c.tsc_hz_milli,
                    c.prev_last_seq,
                    c.source
                );
            }
            continue;
        }
        if kind == EventKind::UTC_ANCHOR {
            if let Ok((a, _)) = UtcAnchor::read_from_prefix(body) {
                if eutc.hz == 0 && n_anchor_noclk == 0 {
                    eprintln!(
                        "pm-md-probe journal: UtcAnchor at seq {cur_seq} before any EngineClock: eu assumes no earlier lead (exact when the engine was behind)"
                    );
                }
                if eutc.hz == 0 {
                    n_anchor_noclk += 1;
                }
                let (cur, l) = eutc.anchor(cur_tsc, &a, calib);
                n_anchor += 1;
                let _ = writeln!(
                    out,
                    "{{\"k\":\"anchor\",\"u\":{cur_u},\"eu\":{},\"sq\":{cur_seq},\"e\":{cur_e},\"tsc\":{},\"rt\":{},\"hz\":{},\"err\":{},\"cur\":{cur},\"tgt\":{},\"lhz\":{},\"lead\":{}}}",
                    eutc.at(cur_tsc, calib),
                    a.tsc.0,
                    a.realtime_ns,
                    a.hz_milli,
                    a.err_ns,
                    l.utc_ns,
                    l.hz_milli,
                    l.lead_ns
                );
            }
            continue;
        }
        let cur_eu = eutc.at(cur_tsc, calib);
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
                    "{{\"k\":\"settle\",\"u\":{cur_u},\"eu\":{cur_eu},\"sq\":{cur_seq},\"e\":{cur_e},\"slot\":\"{}\",\"inst\":{},\"end_s\":{end_s},\"payout\":{},\"cmd_id\":\"{}\"}}",
                    name(&label, inst),
                    inst.0,
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
            if a.orders_only {
                let _ = writeln!(
                    out,
                    "{{\"k\":\"bind\",\"u\":{cur_u},\"eu\":{cur_eu},\"sq\":{cur_seq},\"e\":{cur_e},\"inst\":{},\"slot\":\"{slot}\",\"start_s\":{start_s}}}",
                    inst.0
                );
            }
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
        } else if a.orders_only
            && matches!(
                kind,
                EventKind::L2_SNAPSHOT | EventKind::L2_BATCH | EventKind::TRADE
            )
        {
            continue;
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
            let ls = levels(rest, nb + na);
            for (k, l) in ls.iter().enumerate() {
                let side = if k < nb { &mut b.bids } else { &mut b.asks };
                side.insert(l.px.0, l.qty.0);
            }
            n_snap += 1;
            last_rx = h.h.sw_rx_ns;
            let (bb, ba) = b.best();
            let _ = writeln!(
                out,
                "{{\"k\":\"snap\",\"u\":{cur_u},\"eu\":{cur_eu},\"sq\":{cur_seq},\"e\":{cur_e},\"slot\":\"{}\",\"ex_ms\":{},\"rx_ns\":{},\"id\":{},\"org\":{},\"bb\":{},\"ba\":{},\"lv\":{}}}",
                label[h.h.inst.0 as usize],
                h.h.ex_ts_ns / 1_000_000,
                h.h.sw_rx_ns,
                h.snap_id,
                h.origin,
                bb.unwrap_or(-1),
                ba.unwrap_or(-1),
                lv(&ls, nb)
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
            let ls = levels(rest, nb + na);
            last_rx = h.h.sw_rx_ns;
            for (k, l) in ls.iter().enumerate() {
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
                "{{\"k\":\"batch\",\"u\":{cur_u},\"eu\":{cur_eu},\"sq\":{cur_seq},\"e\":{cur_e},\"slot\":\"{}\",\"ex_ms\":{},\"rx_ns\":{},\"prev\":{},\"last\":{},\"bb\":{},\"ba\":{},\"lv\":{}}}",
                label[h.h.inst.0 as usize],
                h.h.ex_ts_ns / 1_000_000,
                h.h.sw_rx_ns,
                h.prev_id,
                h.last_id,
                bb.unwrap_or(-1),
                ba.unwrap_or(-1),
                lv(&ls, nb)
            );
        } else if kind == EventKind::ORDER_DECISION {
            let Ok((d, _)) = OrderDecision::try_read_from_prefix(body) else {
                continue;
            };
            n_dec += 1;
            let _ = writeln!(
                out,
                "{{\"k\":\"dec\",\"u\":{cur_u},\"eu\":{cur_eu},\"sq\":{cur_seq},\"e\":{cur_e},\"t_ns\":{last_rx},\"slot\":\"{}\",\"inst\":{},\"action\":{},\"side\":\"{}\",\"tif\":{},\"px\":{},\"qty\":{},\"oslot\":{},\"gen\":{},\"reject\":{},\"trig\":{},\"strat\":{},\"reason\":{},\"ino\":{},\"oflags\":{}}}",
                name(&label, d.inst),
                d.inst.0,
                d.action as u8,
                if d.side == Side::Buy { "BUY" } else { "SELL" },
                d.tif as u8,
                d.px.0,
                d.qty.0,
                d.slot.0,
                d.r#gen,
                d.reject,
                d.trigger_seq,
                d.strategy.0,
                d.reason,
                d.intent_no,
                d.oflags
            );
        } else if kind == EventKind::ORDER_EVENT {
            let Ok((e, _)) = OrderEvent::try_read_from_prefix(body) else {
                continue;
            };
            n_oev += 1;
            let _ = writeln!(
                out,
                "{{\"k\":\"oev\",\"u\":{cur_u},\"eu\":{cur_eu},\"sq\":{cur_seq},\"e\":{cur_e},\"t_ns\":{last_rx},\"slot\":\"{}\",\"inst\":{},\"what\":{},\"px\":{},\"qty\":{},\"cum\":{},\"fee\":{},\"oslot\":{},\"gen\":{},\"ex_ms\":{},\"code\":{}}}",
                name(&label, e.inst),
                e.inst.0,
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
                "{{\"k\":\"ext\",\"u\":{cur_u},\"eu\":{cur_eu},\"sq\":{cur_seq},\"e\":{cur_e},\"slot\":\"{}\",\"inst\":{},\"cause\":{},\"side\":\"{}\",\"px\":{},\"qty\":{},\"end_s\":{}}}",
                name(&label, e.inst),
                e.inst.0,
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
                "{{\"k\":\"cmdres\",\"u\":{cur_u},\"eu\":{cur_eu},\"sq\":{cur_seq},\"e\":{cur_e},\"cmd_id\":\"{}\",\"status\":{},\"detail\":{}}}",
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
                "{{\"k\":\"conn\",\"u\":{cur_u},\"eu\":{cur_eu},\"sq\":{cur_seq},\"e\":{cur_e},\"venue\":{},\"conn\":{},\"state\":{},\"inst\":{},\"slot\":\"{}\",\"reason\":{},\"role\":{}}}",
                c.venue.0,
                c.conn,
                c.state as u8,
                c.inst.0,
                if c.inst.0 == 0 {
                    String::new()
                } else {
                    name(&label, c.inst)
                },
                c.reason,
                c.role as u8
            );
        } else if kind == EventKind::BBO {
            // Reference quotes: the reference price as the engine saw it.
            let Ok((b, _)) = Bbo::read_from_prefix(body) else {
                continue;
            };
            let _ = writeln!(
                out,
                "{{\"k\":\"bbo\",\"u\":{cur_u},\"eu\":{cur_eu},\"sq\":{cur_seq},\"e\":{cur_e},\"inst\":{},\"rx_ns\":{},\"ex_ms\":{},\"bid\":{},\"bq\":{},\"ask\":{},\"aq\":{}}}",
                b.h.inst.0,
                b.h.sw_rx_ns,
                b.h.ex_ts_ns / 1_000_000,
                b.bid_px.0,
                b.bid_qty.0,
                b.ask_px.0,
                b.ask_qty.0
            );
        } else if kind == EventKind::TRADE {
            let Ok((t, _)) = Trade::try_read_from_prefix(body) else {
                continue;
            };
            if (t.h.inst.0 as usize) > n || t.h.inst == InstId(0) {
                continue;
            }
            n_trade += 1;
            last_rx = t.h.sw_rx_ns;
            let _ = writeln!(
                out,
                "{{\"k\":\"trade\",\"u\":{cur_u},\"eu\":{cur_eu},\"sq\":{cur_seq},\"e\":{cur_e},\"slot\":\"{}\",\"ex_ms\":{},\"rx_ns\":{},\"px\":{},\"qty\":{},\"side\":\"{}\"}}",
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
         decisions {n_dec}, order events {n_oev}, external fills {n_ext}, settles {n_settle}, conn states {n_conn}, \
         engine clocks {n_eclk}, utc anchors {n_anchor} ({n_anchor_noclk} before an EngineClock)"
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::args;

    fn v(a: &[&str]) -> Vec<String> {
        a.iter().map(|s| (*s).to_string()).collect()
    }

    #[test]
    fn orders_only_flag_and_required_arguments() {
        let a = args(&v(&[
            "--refdata",
            "r",
            "--out",
            "o",
            "--bindings-out",
            "b",
            "--orders-only",
            "s1",
            "s2",
        ]));
        let a = a.unwrap();
        assert!(a.orders_only);
        assert_eq!(a.files.len(), 2);
        let a = args(&v(&["--refdata", "r", "--out", "o", "--bindings-out", "b", "s1"])).unwrap();
        assert!(!a.orders_only);
    }

    #[test]
    fn pmp_005_engine_utc_steps_forward_and_slews_back() {
        use super::{EngineUtc, UtcAnchor};
        use hft_types::Tsc;
        // 1 GHz counter: 1 tick = 1 ns; segment pair (0, 1_000_000_000_000).
        let hz = 1_000_000_000_000u64;
        let seg = Some((0u64, 1_000_000_000_000i64, hz));
        let mut u = EngineUtc::default();
        u.engine_clock(hz);
        assert_eq!(u.at(5_000, seg), 1_000_000_005_000);
        // Behind by 2 ms: steps forward onto the target (lead 0).
        let mk = |tsc: u64, rt: i64, h: u64| UtcAnchor {
            tsc: Tsc(tsc),
            realtime_ns: rt,
            hz_milli: h,
            err_ns: 0,
            _pad: 0,
        };
        let (cur, l) = u.anchor(10_000_000, &mk(10_000_000, 1_000_012_000_000, hz), seg);
        assert_eq!(cur, 1_000_010_000_000);
        assert_eq!((l.utc_ns, l.lead_ns), (1_000_012_000_000, 0));
        assert_eq!(u.at(10_000_000, seg), 1_000_012_000_000);
        // Ahead by 1 ms: keeps the value, lead removed at 500 ppm (1 ms over 2 s of line time).
        let (cur, l) = u.anchor(20_000_000, &mk(20_000_000, 1_000_021_000_000, hz), seg);
        assert_eq!(cur, 1_000_022_000_000);
        assert_eq!(l.lead_ns, 1_000_000);
        assert_eq!(u.at(20_000_000, seg), cur);
        assert_eq!(
            u.at(20_000_000 + 1_000_000_000, seg),
            1_000_021_000_000 + 1_000_000_000 + 500_000
        );
        assert_eq!(
            u.at(20_000_000 + 3_000_000_000, seg),
            1_000_021_000_000 + 3_000_000_000
        );
        // Rate outside 1000 ppm of EngineClock's is ignored; within is used.
        let (_, l) = u.anchor(30_000_000, &mk(30_000_000, 0, hz + hz / 500), seg);
        assert_eq!(l.hz_milli, hz);
        let (_, l) = u.anchor(30_000_000, &mk(30_000_000, 0, hz + hz / 10_000), seg);
        assert_eq!(l.hz_milli, hz + hz / 10_000);
        // A new run clears the line.
        u.engine_clock(hz);
        assert_eq!(u.at(5_000, seg), 1_000_000_005_000);
    }

    #[test]
    fn utc_anchor_golden_body_decodes() {
        use zerocopy::FromBytes;
        // EVT-173 golden record: 24 B header + body tsc 1, realtime_ns 2, hz_milli 3, err_ns 4.
        let rec = include_bytes!("../../../fixtures/hft-event/EVT-173/UtcAnchor.bin");
        let (a, _) = super::UtcAnchor::read_from_prefix(&rec[24..]).unwrap();
        assert_eq!((a.tsc.0, a.realtime_ns, a.hz_milli, a.err_ns), (1, 2, 3, 4));
    }

    #[test]
    fn missing_segments_or_unknown_flags_are_refused() {
        assert!(args(&v(&["--refdata", "r", "--out", "o", "--bindings-out", "b"])).is_err());
        assert!(
            args(&v(&[
                "--refdata",
                "r",
                "--out",
                "o",
                "--bindings-out",
                "b",
                "--bogus",
                "s1"
            ]))
            .is_err()
        );
    }
}
