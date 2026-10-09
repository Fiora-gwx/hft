//! Test writer (README "cargo feature"): writes a `seq` journal for reader-only components' fixtures.
//! Not part of any release artefact; run as a subprocess by tests.
//!
//! testwriter --hot <dir> --cold <dir> --instance prod|sim
//!            (--from <journal dir> | --synth-bbo <n> --inst <id> --bid <raw> --ask <raw> --qty <raw>
//!             --t0-ns <utc> --step-ns <ns>)
//!            [--refdata <blob>] [--publish-every <n>] [--segment-bytes <b>] [--sync-interval-ms <ms>] [--seal]
//! testwriter --hot <dir> --cold <dir> --instance prod|sim --script <file|->
//!            [--segment-bytes <b>] [--sync-interval-ms <ms>] [--publish-every <n>]
//!            [--tsc-hz-milli <hz x 1000>] [--calib-utc-ns <utc>] [--clock-every-ns <ns>]
//!
//! Script mode (README "测试写者"): one command per line, `#` starts a comment; numbers decimal or
//! `0x` hex, bodies hex (`-` = empty). `boot <kind> <hex>` (record of every later boot block),
//! `boot-clear`, `rec <kind> <flags> <tsc> <hex>`, `bulk <file>`, `synth <n> <inst> <bid> <ask>
//! <qty> <t0_ns> <step_ns> [<per_s>]`, `end`, `publish`, `roll`, `persist on|off`, `flush`,
//! `sleep <ms>`, `mark <tag>` (prints `<tag> <CLOCK_MONOTONIC ns>`), `seal`, `abort`, `exit`.
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
    if let Some(script) = a.get("script") {
        script::run(&a, script);
    }
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

/// `--script` mode: drives the writer from a command stream (README "测试写者").
mod script {
    use std::io::{BufRead, Write};
    use std::path::PathBuf;
    use std::process::exit;
    use std::time::{Duration, Instant};

    use hft_event::{Bbo, ClockStatus, EventKind, MarketHdr, RecordFlags};
    use hft_journal::*;
    use hft_rt::{Backing, CalibPair, spsc};
    use hft_types::{InstId, Px, Qty, Tsc, VenueId};
    use zerocopy::FromZeros;

    use super::{Args, usage, writer_err};

    fn num<T: TryFrom<u128>>(s: &str) -> T {
        let v = match s.strip_prefix("0x") {
            Some(h) => u128::from_str_radix(h, 16).ok(),
            None => s.parse::<u128>().ok(),
        };
        v.and_then(|v| T::try_from(v).ok())
            .unwrap_or_else(|| usage(&format!("bad number {s}")))
    }

    fn inum(s: &str) -> i64 {
        s.parse::<i64>()
            .unwrap_or_else(|_| usage(&format!("bad number {s}")))
    }

    fn hex(s: &str) -> Vec<u8> {
        if s == "-" {
            return Vec::new();
        }
        let b = s.as_bytes();
        if !b.len().is_multiple_of(2) {
            usage("odd hex length");
        }
        b.chunks(2)
            .map(|c| {
                std::str::from_utf8(c)
                    .ok()
                    .and_then(|x| u8::from_str_radix(x, 16).ok())
                    .unwrap_or_else(|| usage("bad hex"))
            })
            .collect()
    }

    /// `CLOCK_MONOTONIC` in ns (same clock as Python's `time.monotonic_ns()` on Linux).
    fn mono_ns() -> u64 {
        let mut ts = libc::timespec {
            tv_sec: 0,
            tv_nsec: 0,
        };
        // SAFETY: valid out-pointer for clock_gettime.
        unsafe { libc::clock_gettime(libc::CLOCK_MONOTONIC, &mut ts) };
        ts.tv_sec as u64 * 1_000_000_000 + ts.tv_nsec as u64
    }

    struct St {
        w: Option<JournalWriter>,
        prep: SegmentPreparer,
        pers: Persister,
        ret: HotRetirer,
        persist: bool,
        mono: u64,
        sync_ms: u32,
        hz_milli: u64,
        calib_utc: i64,
        clock_every: i64,
        next_clock: i64,
        boot: Vec<(u16, Vec<u8>)>,
        events: u64,
        publish_every: u64,
        polls: u64,
        /// TSC of the last appended record (rolls happen there).
        last_tsc: u64,
    }

    impl St {
        fn w(&mut self) -> &mut JournalWriter {
            self.w.as_mut().unwrap_or_else(|| writer_err("writer closed"))
        }

        fn tsc_of(&self, t: i64) -> u64 {
            let d = (t - self.calib_utc).max(0) as u128;
            (d * u128::from(self.hz_milli) / 1_000_000_000_000) as u64
        }

        /// Rolls at the TSC of the last appended record: the new header's calibration pair, the
        /// roll time and the boot-block records all use it.
        fn roll(&mut self, reason: RollReason) {
            if self.prep.poll() == PrepState::Failed {
                writer_err("prepare failed");
            }
            let at = self.last_tsc;
            let utc = if self.hz_milli == 0 {
                self.calib_utc
            } else {
                self.calib_utc + (u128::from(at) * 1_000_000_000_000 / u128::from(self.hz_milli)) as i64
            };
            let calib = CalibPair {
                tsc: Tsc(at),
                realtime_ns: utc,
                phc_ns: 0,
                err_ns: 0,
            };
            let info = RollInfo {
                utc_ns: utc,
                calib,
                tsc_hz_milli: self.hz_milli,
                configs: &[],
            };
            let boot = self.boot.clone();
            let w = self.w();
            let mut b = w
                .roll(reason, &info)
                .unwrap_or_else(|e| writer_err(&format!("{e:?}")));
            for (k, body) in &boot {
                b.append_var(EventKind(*k), Tsc(at), &[body])
                    .unwrap_or_else(|e| writer_err(&format!("{e:?}")));
            }
            b.finish().unwrap_or_else(|e| writer_err(&format!("{e:?}")));
            let _ = self.prep.poll();
            if self.persist {
                self.pers.poll(self.mono, self.calib_utc);
            }
        }

        /// Advances the synthetic monotonic clock by 1 ms per publish; the persist round (and the
        /// next-segment preparation) runs every 256 publishes (`flush` and rolls run it at once).
        fn poll_persist(&mut self) {
            self.mono += 1_000_000;
            self.polls += 1;
            if !self.polls.is_multiple_of(256) {
                return;
            }
            let _ = self.prep.poll();
            if self.persist {
                self.pers.poll(self.mono, self.calib_utc);
            }
        }

        fn flush(&mut self) {
            let _ = self.prep.poll();
            for _ in 0..10_000 {
                self.mono += u64::from(self.sync_ms) * 1_000_000;
                self.pers.poll(self.mono, self.calib_utc);
                if self.pers.lag().iter().all(|l| l.bytes == 0) {
                    let p = self.pers.poll(self.mono, self.calib_utc);
                    if p.bytes_copied == 0 && p.sealed == 0 {
                        break;
                    }
                }
            }
        }

        fn maybe_roll(&mut self) {
            if let Some((reason, _)) = self.w().needs_roll(Tsc(0)) {
                self.w().publish();
                self.roll(reason);
                self.ret.poll(self.calib_utc);
            }
        }

        fn rec(&mut self, kind: u16, flags: u16, tsc: u64, body: &[u8]) {
            self.maybe_roll();
            self.last_tsc = tsc;
            self.w()
                .append_var(EventKind(kind), RecordFlags(flags), Tsc(tsc), &[body])
                .unwrap_or_else(|e| writer_err(&format!("{e:?}")));
        }

        fn end(&mut self) {
            self.w().end_event();
            self.events += 1;
            if self.events.is_multiple_of(self.publish_every) {
                self.w().publish();
                self.poll_persist();
            }
        }

        /// `ClockStatus` (Meta) at every `--clock-every-ns` boundary up to `t`.
        fn clock_status(&mut self, t: i64) {
            if self.clock_every <= 0 || t < self.next_clock {
                return;
            }
            while self.next_clock <= t {
                let at = self.next_clock;
                let mut c = ClockStatus::new_zeroed();
                c.tsc = Tsc(self.tsc_of(at));
                c.realtime_ns = at;
                c.tsc_hz_milli = self.hz_milli;
                c.source = 1;
                c.state = 1;
                self.maybe_roll();
                let tsc = c.tsc;
                self.w()
                    .append_meta(tsc, &c)
                    .unwrap_or_else(|e| writer_err(&format!("{e:?}")));
                self.next_clock += self.clock_every;
            }
        }

        #[allow(clippy::too_many_arguments)]
        fn synth(&mut self, n: u64, inst: u32, bid: i64, ask: i64, qty: i64, t0: i64, step: i64, per_s: u64) {
            let start = Instant::now();
            for i in 0..n {
                if let Some(ns) = (i * 1_000_000_000).checked_div(per_s)
                    && let Some(d) = Duration::from_nanos(ns).checked_sub(start.elapsed())
                {
                    std::thread::sleep(d);
                }
                let t = t0 + i as i64 * step;
                self.clock_status(t);
                let mut h = MarketHdr::new_zeroed();
                h.inst = InstId(inst);
                h.venue = VenueId(1);
                h.ex_ts_ns = t - 2_000_000;
                h.hw_rx_ns = if i % 2 == 0 { t - 1_000_000 } else { 0 };
                let b = Bbo {
                    h,
                    bid_px: Px(bid),
                    bid_qty: Qty(qty),
                    ask_px: Px(ask),
                    ask_qty: Qty(qty),
                    update_id: i,
                };
                self.maybe_roll();
                let tsc = Tsc(self.tsc_of(t));
                self.last_tsc = tsc.0;
                self.w()
                    .append(RecordFlags(0), tsc, &b)
                    .unwrap_or_else(|e| writer_err(&format!("{e:?}")));
                self.end();
            }
        }

        /// Binary command file: op 1 record (`kind u16, flags u16, len u32, tsc u64, body`), 2
        /// `end`, 3 `publish`, 4 `roll`, 5 `ClockStatus` boundaries up to `t i64`.
        fn bulk(&mut self, path: &str) {
            let b = std::fs::read(path).unwrap_or_else(|_| usage("cannot read bulk file"));
            let rd = |o: usize, n: usize| -> &[u8] {
                b.get(o..o + n).unwrap_or_else(|| usage("truncated bulk file"))
            };
            let u64_at = |o: usize| -> u64 {
                let mut t = [0u8; 8];
                t.copy_from_slice(rd(o, 8));
                u64::from_le_bytes(t)
            };
            let mut o = 0usize;
            while o < b.len() {
                let op = rd(o, 1).first().copied().unwrap_or(0);
                o += 1;
                match op {
                    1 => {
                        let h = rd(o, 8);
                        let kind = u16::from_le_bytes([h[0], h[1]]);
                        let flags = u16::from_le_bytes([h[2], h[3]]);
                        let len = u32::from_le_bytes([h[4], h[5], h[6], h[7]]) as usize;
                        let tsc = u64_at(o + 8);
                        o += 16;
                        let body = rd(o, len).to_vec();
                        o += len;
                        self.rec(kind, flags, tsc, &body);
                    }
                    2 => self.end(),
                    3 => {
                        self.w().publish();
                        self.poll_persist();
                    }
                    4 => {
                        self.w().publish();
                        self.roll(RollReason::Manual);
                    }
                    5 => {
                        let t = u64_at(o) as i64;
                        o += 8;
                        self.clock_status(t);
                    }
                    _ => usage("bad bulk op"),
                }
            }
        }
    }

    pub(super) fn run(a: &Args, script: &str) -> ! {
        let hot = PathBuf::from(a.req("hot"));
        let cold = PathBuf::from(a.req("cold"));
        let instance: InstanceName = a
            .req("instance")
            .parse()
            .unwrap_or_else(|_| usage("--instance prod|sim"));
        let cfg = JournalConfig {
            name: "seq".into(),
            kind: JournalKind::Seq,
            venue: VenueId(0),
            instance,
            hot_dir: hot,
            cold_root: cold,
            segment_bytes: a.num("segment-bytes", Some(8 << 20)),
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
        let (w, rep) = JournalWriter::open(&cfg, env).unwrap_or_else(|e| writer_err(&format!("{e:?}")));
        let prep = SegmentPreparer::new(
            &cfg,
            PrepEnv {
                writer_epoch: 1,
                build_git: [0; 20],
                host_profile_sha256: [0; 32],
                to_writer: ptx,
            },
        );
        let sync_ms: u32 = a.num("sync-interval-ms", Some(50));
        let mut pers = Persister::new(PersistConfig {
            sync_interval_ms: sync_ms,
            ..PersistConfig::default()
        });
        pers.add(&cfg);
        let calib_utc: i64 = a.num("calib-utc-ns", Some(0));
        let mut st = St {
            w: Some(w),
            prep,
            pers,
            ret: HotRetirer::new(&cfg, rrx),
            persist: true,
            mono: 0,
            sync_ms,
            hz_milli: a.num("tsc-hz-milli", Some(0)),
            calib_utc,
            clock_every: a.num("clock-every-ns", Some(0)),
            next_clock: calib_utc,
            boot: Vec::new(),
            events: 0,
            publish_every: a.num("publish-every", Some(1u64)).max(1),
            polls: 0,
            last_tsc: 0,
        };
        let input: Box<dyn BufRead> = if script == "-" {
            Box::new(std::io::BufReader::new(std::io::stdin()))
        } else {
            Box::new(std::io::BufReader::new(
                std::fs::File::open(script).unwrap_or_else(|_| usage("cannot open --script")),
            ))
        };
        let mut rolled = false;
        let mut sealed = false;
        let out = std::io::stdout();
        for line in input.lines() {
            let line = line.unwrap_or_else(|_| usage("script read error"));
            let line = line.split('#').next().unwrap_or("").trim().to_string();
            let t: Vec<&str> = line.split_whitespace().collect();
            let Some((&cmd, args)) = t.split_first() else {
                continue;
            };
            let arg = |i: usize| -> &str {
                args.get(i)
                    .copied()
                    .unwrap_or_else(|| usage(&format!("{cmd}: missing argument")))
            };
            if !rolled && !matches!(cmd, "boot" | "boot-clear" | "mark" | "sleep") {
                st.roll(rep.boot_reason);
                rolled = true;
            }
            match cmd {
                "boot" => st.boot.push((num(arg(0)), hex(arg(1)))),
                "boot-clear" => st.boot.clear(),
                "rec" => {
                    let body = hex(arg(3));
                    st.rec(num(arg(0)), num(arg(1)), num(arg(2)), &body);
                }
                "bulk" => st.bulk(arg(0)),
                "synth" => {
                    let per_s = args.get(7).map_or(0, |s| num(s));
                    st.synth(
                        num(arg(0)),
                        num(arg(1)),
                        inum(arg(2)),
                        inum(arg(3)),
                        inum(arg(4)),
                        inum(arg(5)),
                        inum(arg(6)),
                        per_s,
                    );
                }
                "end" => st.end(),
                "publish" => {
                    st.w().publish();
                    st.poll_persist();
                }
                "roll" => {
                    st.w().publish();
                    st.roll(RollReason::Manual);
                }
                "persist" => st.persist = arg(0) == "on",
                "flush" => st.flush(),
                "sleep" => std::thread::sleep(Duration::from_millis(num(arg(0)))),
                "mark" => {
                    let mut o = out.lock();
                    let _ = writeln!(o, "{} {}", arg(0), mono_ns());
                    let _ = o.flush();
                }
                "seal" => {
                    if let Some(w) = st.w.take() {
                        w.seal_and_close()
                            .unwrap_or_else(|e| writer_err(&format!("{e:?}")));
                    }
                    sealed = true;
                    if st.persist {
                        st.flush();
                    }
                }
                "abort" => std::process::abort(),
                "exit" => break,
                _ => usage(&format!("unknown script command {cmd}")),
            }
        }
        if !rolled {
            st.roll(rep.boot_reason);
        }
        if !sealed {
            st.w().publish();
            if st.persist {
                st.flush();
            }
            // Keep the last segment Open (a running instance); leak the writer's mappings.
            std::mem::forget(st.w.take());
        }
        exit(0)
    }
}
