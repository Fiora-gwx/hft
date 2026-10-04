//! Test writer (README "cargo feature"): writes a `seq` journal for reader-only components' fixtures.
//! Not part of any release artefact; run as a subprocess by tests.
//!
//! testwriter --hot <dir> --cold <dir> --instance prod|sim
//!            (--from <journal dir> | --synth-bbo <n> --inst <id> --bid <raw> --ask <raw> --qty <raw>
//!             --t0-ns <utc> --step-ns <ns>)
//!            [--refdata <blob>] [--publish-every <n>] [--segment-bytes <b>] [--sync-interval-ms <ms>] [--seal]
//!
//! Exit codes: 0 success, 1 argument or source error, 2 writer error.

#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods
)] // allow-list: test code, not a hot path

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::exit;

use hft_event::{Bbo, BlobHdr, BlobKind, BlobRef, CmdScope, EventKind, MarketHdr, RecordFlags};
use hft_journal::*;
use hft_rt::{Backing, CalibPair, spsc};
use hft_types::{InstId, Px, Qty, Tsc, VenueId};
use sha2::{Digest, Sha256};
use zerocopy::{FromZeros, IntoBytes};

fn usage(msg: &str) -> ! {
    eprintln!("testwriter: {msg}");
    exit(1)
}

fn writer_err(msg: &str) -> ! {
    eprintln!("testwriter: writer error: {msg}");
    exit(2)
}

struct Args {
    m: HashMap<String, String>,
    flags: Vec<String>,
}

impl Args {
    fn parse() -> Args {
        let mut m = HashMap::new();
        let mut flags = Vec::new();
        let mut it = std::env::args().skip(1);
        while let Some(a) = it.next() {
            let Some(k) = a.strip_prefix("--") else {
                usage(&format!("unexpected argument {a}"))
            };
            if k == "seal" {
                flags.push(k.to_string());
                continue;
            }
            let Some(v) = it.next() else {
                usage(&format!("--{k} needs a value"))
            };
            m.insert(k.to_string(), v);
        }
        Args { m, flags }
    }
    fn get(&self, k: &str) -> Option<&str> {
        self.m.get(k).map(String::as_str)
    }
    fn req(&self, k: &str) -> &str {
        self.get(k).unwrap_or_else(|| usage(&format!("missing --{k}")))
    }
    fn num<T: std::str::FromStr>(&self, k: &str, def: Option<T>) -> T {
        match self.get(k) {
            Some(v) => v.parse().unwrap_or_else(|_| usage(&format!("bad --{k}"))),
            None => def.unwrap_or_else(|| usage(&format!("missing --{k}"))),
        }
    }
}

/// One source event: kind + logical body.
enum Ev {
    Bbo(Bbo),
    Raw(u16, u64, Vec<u8>),
}

fn synth(a: &Args) -> Vec<Ev> {
    let n: u64 = a.num("synth-bbo", None);
    let inst: u32 = a.num("inst", None);
    let bid: i64 = a.num("bid", None);
    let ask: i64 = a.num("ask", None);
    let qty: i64 = a.num("qty", None);
    let t0: i64 = a.num("t0-ns", None);
    let step: i64 = a.num("step-ns", None);
    (0..n)
        .map(|i| {
            let t = t0 + i as i64 * step;
            let mut h = MarketHdr::new_zeroed();
            h.inst = InstId(inst);
            h.ex_ts_ns = t - 2_000_000;
            h.hw_rx_ns = if i % 2 == 0 { t - 1_000_000 } else { 0 };
            Ev::Bbo(Bbo {
                h,
                bid_px: Px(bid),
                bid_qty: Qty(qty),
                ask_px: Px(ask),
                ask_qty: Qty(qty),
                update_id: 0,
            })
        })
        .collect()
}

/// Non-Meta records after each boot block of `<root>/<instance>/<journal>` (`--from`).
fn from_journal(dir: &Path) -> Vec<Ev> {
    let name = dir
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or_else(|| usage("bad --from"));
    let inst_dir = dir.parent().unwrap_or_else(|| usage("bad --from"));
    let inst: InstanceName = inst_dir
        .file_name()
        .and_then(|s| s.to_str())
        .and_then(|s| s.parse().ok())
        .unwrap_or_else(|| usage("--from must be <root>/<prod|sim>/<journal>"));
    let root = inst_dir.parent().unwrap_or_else(|| usage("bad --from"));
    let mut rc = ReaderConfig::new(root.to_path_buf(), root.join("__no_cold__"), inst, name.into());
    rc.end = EndAt::LastCompleteEvent;
    let mut r = JournalReader::open(&rc).unwrap_or_else(|e| usage(&format!("--from: {e:?}")));
    let mut out = Vec::new();
    loop {
        let (k, tsc, body) = match r.poll() {
            Next::Record(v) => (v.hdr.kind, v.hdr.tsc, v.body.to_vec()),
            Next::Event(e) => (e.kind.0, e.tsc, e.body.to_vec()),
            Next::End | Next::Pending => break,
            Next::Gap(_) | Next::Err(_) => usage("--from: unreadable source"),
        };
        let meta = hft_event::kind_info(EventKind(k)).is_some_and(|i| i.role == hft_event::Role::Meta);
        if !meta && !r.in_boot_block() {
            out.push(Ev::Raw(k, tsc, body));
        }
    }
    out
}

fn main() {
    let a = Args::parse();
    let hot = PathBuf::from(a.req("hot"));
    let cold = PathBuf::from(a.req("cold"));
    let instance: InstanceName = a
        .req("instance")
        .parse()
        .unwrap_or_else(|_| usage("--instance prod|sim"));
    let publish_every: u64 = a.num("publish-every", Some(1)).max(1);
    let segment_bytes: u64 = a.num("segment-bytes", Some(8 << 20));
    let sync_ms: u32 = a.num("sync-interval-ms", Some(50));
    let seal = a.flags.iter().any(|f| f == "seal");
    let events = match (a.get("from"), a.get("synth-bbo")) {
        (Some(f), None) => from_journal(Path::new(f)),
        (None, Some(_)) => synth(&a),
        _ => usage("exactly one of --from / --synth-bbo"),
    };
    let refdata = a
        .get("refdata")
        .map(|p| std::fs::read(p).unwrap_or_else(|_| usage("cannot read --refdata")));

    let cfg = JournalConfig {
        name: "seq".into(),
        kind: JournalKind::Seq,
        venue: VenueId(0),
        instance,
        hot_dir: hot,
        cold_root: cold,
        segment_bytes,
        page: PageSize::Normal,
        size_idle_pct: DEFAULT_SIZE_IDLE_PCT,
        size_valve_pct: DEFAULT_SIZE_VALVE_PCT,
        hot_keep: 1,
    };
    let (ptx, prx) = spsc(4, Backing::Heap).unwrap_or_else(|_| writer_err("ring"));
    let (rtx, rrx) = spsc(1024, Backing::Heap).unwrap_or_else(|_| writer_err("ring"));
    let env = WriterEnv {
        writer_epoch: 1,
        build_git: [0; 20],
        host_profile_sha256: [0; 32],
        prepared: prx,
        retired: rtx,
    };
    let (mut w, rep) = JournalWriter::open(&cfg, env).unwrap_or_else(|e| writer_err(&format!("{e:?}")));
    let mut prep = SegmentPreparer::new(
        &cfg,
        PrepEnv {
            writer_epoch: 1,
            build_git: [0; 20],
            host_profile_sha256: [0; 32],
            to_writer: ptx,
        },
    );
    let mut pers = Persister::new(PersistConfig {
        sync_interval_ms: sync_ms,
        ..PersistConfig::default()
    });
    pers.add(&cfg);
    let mut ret = HotRetirer::new(&cfg, rrx);
    let blob = refdata.as_ref().map(|b| {
        let r = BlobRef {
            kind: BlobKind::Refdata,
            encoding: BlobRef::ENC_RDB1,
            len: b.len() as u32,
            version: 0,
            sha256: Sha256::digest(b).into(),
        };
        let h = BlobHdr {
            blob: r,
            scope: CmdScope::new_zeroed(),
            boot: 1,
            _pad: [0; 7],
        };
        (r, h)
    });
    let configs: Vec<BlobRef> = blob.iter().map(|(r, _)| *r).collect();
    let utc0 = 0i64;
    let mut mono = 0u64;
    let do_roll = |w: &mut JournalWriter, prep: &mut SegmentPreparer, reason: RollReason| {
        if prep.poll() == PrepState::Failed {
            writer_err("prepare failed");
        }
        let info = RollInfo {
            utc_ns: utc0,
            calib: CalibPair::default(),
            tsc_hz_milli: 0,
            configs: &configs,
        };
        let mut b = w
            .roll(reason, &info)
            .unwrap_or_else(|e| writer_err(&format!("{e:?}")));
        if let (Some((_, h)), Some(content)) = (&blob, &refdata) {
            b.append_var(EventKind::CONFIG_BLOB, Tsc(0), &[h.as_bytes(), content])
                .unwrap_or_else(|e| writer_err(&format!("{e:?}")));
        }
        b.finish().unwrap_or_else(|e| writer_err(&format!("{e:?}")));
    };
    do_roll(&mut w, &mut prep, rep.boot_reason);
    for (i, ev) in events.iter().enumerate() {
        if let Some((reason, _)) = w.needs_roll(Tsc(0)) {
            w.publish();
            do_roll(&mut w, &mut prep, reason);
            pers.poll(mono, utc0);
            ret.poll(utc0);
        }
        let r = match ev {
            Ev::Bbo(b) => w.append(RecordFlags(0), Tsc(i as u64), b),
            Ev::Raw(k, tsc, body) => w.append_var(EventKind(*k), RecordFlags(0), Tsc(*tsc), &[body]),
        };
        if let Err(e) = r {
            writer_err(&format!("{e:?}"));
        }
        w.end_event();
        if (i as u64 + 1).is_multiple_of(publish_every) {
            w.publish();
        }
        mono += 1_000_000;
        let _ = prep.poll();
        pers.poll(mono, utc0);
    }
    w.publish();
    if seal {
        w.seal_and_close()
            .unwrap_or_else(|e| writer_err(&format!("{e:?}")));
    } else {
        // Keep the last segment Open (a running instance); leak the writer's mappings.
        std::mem::forget(w);
    }
    // Flush persist up to committed_len (and `.seal` for sealed segments).
    for _ in 0..10_000 {
        mono += u64::from(sync_ms) * 1_000_000;
        pers.poll(mono, utc0);
        if pers.lag().iter().all(|l| l.bytes == 0) {
            let p = pers.poll(mono, utc0);
            if p.bytes_copied == 0 && p.sealed == 0 {
                break;
            }
        }
    }
    exit(0)
}
