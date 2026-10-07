//! Reference read-stamp model (README §7), written from the rules, for NET-136:
//! - a frame takes `hw/sw/rx_empty_gap_ns` from the read holding the first byte of its first TLS
//!   record, `rx_tsc` from the read holding the last byte of its last record; when they differ the
//!   gap grows by the difference of the two reads' `rx_tsc`; an unconfirmed first read gives 0;
//! - `MULTI_SKB` when that first read is larger than the MSS or was merged in the 16-entry table;
//! - the table holds reads not yet fully decrypted; pushing a 17th merges the two oldest
//!   (earlier stamps kept); every complete record is decrypted in the round that completes it;
//! - queue-empty confirmations: `EAGAIN` and short reads at the TSC before the call; the gap of a
//!   read is `rx_tsc − confirmation`, saturating at `u32::MAX`; none yet → 0.

use hft_net::{Timestamps, TsFlags};

/// One scripted read: `bytes == 0` is an `EAGAIN`. Every data read here is short (it fits).
#[derive(Copy, Clone, Debug)]
pub struct Read {
    pub bytes: u64,
    pub pre: u64,
    pub post: u64,
    pub hw: i64,
    pub sw: i64,
}

#[derive(Copy, Clone, Debug, Default)]
struct Entry {
    start: u64,
    end: u64,
    bytes: u64,
    merged: bool,
    hw: i64,
    sw: i64,
    gap: u64,
    confirmed: bool,
    first_rx: u64,
    rx: u64,
}

/// Per record: the stamps of its first byte's read and the `rx_tsc` of its last byte's read.
#[derive(Copy, Clone, Debug, Default)]
pub struct RecStamp {
    hw: i64,
    sw: i64,
    gap: u64,
    confirmed: bool,
    multi: bool,
    first_rx: u64,
    done_rx: u64,
}

/// Runs the model over `reads` for records with ciphertext ranges `recs` (`[start, end)`, in
/// stream order) and returns each record's stamps.
pub fn records(reads: &[Read], recs: &[(u64, u64)], mss: u64) -> Vec<RecStamp> {
    let mut out = vec![RecStamp::default(); recs.len()];
    let mut live: Vec<Entry> = Vec::new();
    let mut empty: Option<u64> = None;
    let mut received = 0u64;
    let mut next_rec = 0usize;
    for r in reads {
        if r.bytes == 0 {
            empty = Some(empty.map_or(r.pre, |e| e.max(r.pre)));
            continue;
        }
        let (gap, confirmed) = match empty {
            Some(e) => (r.post.saturating_sub(e).min(u32::MAX as u64), true),
            None => (0, false),
        };
        if live.len() == 16 {
            let (a, b) = (live[0], live[1]);
            live[0] = Entry {
                start: a.start,
                end: b.end,
                bytes: a.bytes + b.bytes,
                merged: true,
                rx: b.rx,
                ..a
            };
            live.remove(1);
        }
        live.push(Entry {
            start: received,
            end: received + r.bytes,
            bytes: r.bytes,
            merged: false,
            hw: r.hw,
            sw: r.sw,
            gap,
            confirmed,
            first_rx: r.post,
            rx: r.post,
        });
        empty = Some(empty.map_or(r.pre, |e| e.max(r.pre)));
        received += r.bytes;
        // Every record complete by now is decrypted this round.
        while next_rec < recs.len() && recs[next_rec].1 <= received {
            let (s, e) = recs[next_rec];
            let f = *live.iter().find(|x| x.start <= s && s < x.end).unwrap();
            let l = *live.iter().find(|x| x.start < e && e <= x.end).unwrap();
            out[next_rec] = RecStamp {
                hw: f.hw,
                sw: f.sw,
                gap: f.gap,
                confirmed: f.confirmed,
                multi: f.merged || f.bytes > mss,
                first_rx: f.first_rx,
                done_rx: l.rx,
            };
            next_rec += 1;
            live.retain(|x| x.end > e);
        }
    }
    out
}

/// Stamps of a frame whose first byte is in record `first` and last byte in record `last`.
pub fn frame(rs: &[RecStamp], first: usize, last: usize) -> Timestamps {
    let f = rs[first];
    let done = rs[last].done_rx;
    let gap = if f.confirmed {
        (f.gap + done.saturating_sub(f.first_rx)).min(u32::MAX as u64) as u32
    } else {
        0
    };
    let mut flags = 0;
    if f.hw != 0 {
        flags |= TsFlags::HW;
    }
    if f.sw != 0 {
        flags |= TsFlags::SW;
    }
    if f.multi {
        flags |= TsFlags::MULTI_SKB;
    }
    Timestamps {
        hw_rx_ns: f.hw,
        sw_rx_ns: f.sw,
        rx_tsc: done,
        flags,
        rx_empty_gap_ns: gap,
    }
}
