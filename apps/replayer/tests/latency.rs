//! CLI latency evidence over real sealed files, with independently chosen timestamps.
use hft_event::{
    Bbo, BlobHdr, BlobKind, BlobRef, ClockStatus, CmdScope, EventKind, HostProfile, L2SnapHdr, MarketHdr,
    RecordFlags,
};
use hft_journal::{InstanceName, JournalConfig, JournalKind, JournalWriter, PageSize, RollInfo, RollReason};
use hft_rt::CalibPair;
use hft_testkit::dst::{seq::build, world::WorldSpec};
use hft_types::{InstId, Tsc, VenueId};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};
use zerocopy::{FromZeros, IntoBytes};

const SEC: u64 = 1_000_000_000;
const EPOCH: i64 = 1_790_812_800_000_000_000;
static NEXT: AtomicU64 = AtomicU64::new(0);
struct Fixture {
    root: PathBuf,
    writer: Option<JournalWriter>,
    blob: Vec<u8>,
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}
impl Fixture {
    fn new() -> Self {
        // All test writes remain under the worktree's ignored target directory.
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/latency-tests")
            .join(format!(
                "{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
        let cfg = JournalConfig {
            name: "seq".into(),
            kind: JournalKind::Seq,
            venue: VenueId(0),
            instance: InstanceName::Prod,
            hot_dir: root.join("unused-hot"),
            cold_root: root.join("unused-cold"),
            segment_bytes: 8 << 20,
            page: PageSize::Normal,
            size_idle_pct: 75,
            size_valve_pct: 90,
            hot_keep: 1,
        };
        let writer = JournalWriter::open_offline(&cfg, &root.join("input")).unwrap();
        let mut f = Self {
            root,
            writer: Some(writer),
            blob: build::refdata_blob(&WorldSpec::w1(), false).unwrap(),
        };
        f.roll();
        f
    }
    fn roll(&mut self) {
        let r = BlobRef {
            kind: BlobKind::Refdata,
            encoding: 1,
            len: self.blob.len() as u32,
            version: 1,
            sha256: Sha256::digest(&self.blob).into(),
        };
        let hdr = BlobHdr {
            blob: r,
            scope: CmdScope::new_zeroed(),
            boot: 1,
            _pad: [0; 7],
        };
        let info = RollInfo {
            utc_ns: EPOCH,
            calib: CalibPair {
                tsc: Tsc(SEC),
                realtime_ns: EPOCH,
                phc_ns: EPOCH,
                err_ns: 100,
            },
            tsc_hz_milli: SEC * 1000,
            configs: &[r],
        };
        let mut boot = self
            .writer
            .as_mut()
            .unwrap()
            .roll(RollReason::Manual, &info)
            .unwrap();
        let mut profile = HostProfile::new_zeroed();
        profile.profile_sha256 = [1; 32];
        profile.tsc_hz_milli = SEC * 1000;
        boot.append(Tsc(SEC), &profile).unwrap();
        boot.append_var(EventKind::CONFIG_BLOB, Tsc(SEC), &[hdr.as_bytes(), &self.blob])
            .unwrap();
        boot.finish().unwrap();
    }
    fn clock(&mut self, sec: u64, err: u32, ppm: bool) {
        let t = (sec + 1) * SEC;
        let mut c = ClockStatus::new_zeroed();
        c.tsc = Tsc(t);
        c.phc_ns = domain(t, ppm);
        c.realtime_ns = c.phc_ns;
        c.tsc_hz_milli = SEC * 1000;
        c.calib_err_ns = err;
        c.state = 1;
        self.writer.as_mut().unwrap().append_meta(Tsc(t), &c).unwrap();
    }
    fn market(&mut self, t: u64, delay: i64, sw: bool, gap: u32, depth: bool, ppm: bool) {
        let mut h = MarketHdr::new_zeroed();
        h.venue = VenueId::BINANCE_SPOT;
        h.inst = InstId(1);
        h.rx_tsc = t;
        h.hw_rx_ns = domain(t, ppm) - delay;
        h.sw_rx_ns = h.hw_rx_ns + if sw { 0 } else { 1000 };
        h.rx_empty_gap_ns = gap;
        let flags = if sw {
            RecordFlags::SW_TS
        } else {
            RecordFlags::HW_TS
        };
        let w = self.writer.as_mut().unwrap();
        if depth {
            let mut b = L2SnapHdr::new_zeroed();
            b.h = h;
            w.append_var(EventKind::L2_SNAPSHOT, flags, Tsc(t + 1500), &[b.as_bytes()])
                .unwrap();
        } else {
            let mut b = Bbo::new_zeroed();
            b.h = h;
            w.append(flags, Tsc(t + 1500), &b).unwrap();
        }
        w.end_event();
        w.publish();
    }
    fn close(&mut self) {
        self.writer.take().unwrap().seal_and_close().unwrap();
    }
    fn args(&self, out: &str) -> Vec<String> {
        vec![
            "latency".into(),
            "--journal".into(),
            self.root.join("input").display().to_string(),
            "--out".into(),
            self.root.join(out).display().to_string(),
            "--cal-max-err-ns".into(),
            "1000".into(),
            "--burst-gap-ns".into(),
            "50000".into(),
        ]
    }
    fn run(&self, out: &str) -> Value {
        let (code, text, err) = super::run(&self.args(out));
        assert_eq!(code, 0, "{text} {err}");
        serde_json::from_slice(&std::fs::read(self.root.join(out).join("latency.json")).unwrap()).unwrap()
    }
}
fn domain(t: u64, ppm: bool) -> i64 {
    EPOCH + t as i64 + if ppm { (t / 50_000) as i64 } else { 0 }
}
fn group<'a>(v: &'a Value, stage: &str, source: &str) -> &'a Value {
    &v["datasets"][0]["groups"]
        .as_array()
        .unwrap()
        .iter()
        .find(|g| g["key"]["stage"] == stage && g["key"]["ts_src"] == source)
        .unwrap()["q"]
}

#[test]
fn rpl_190_interpolation_within_error() {
    let mut f = Fixture::new();
    for n in 0..1000 {
        f.clock(n, 100, true);
        f.market((n + 1) * SEC + SEC / 2, 3000, false, 1, false, true);
    }
    f.clock(1000, 100, true);
    f.close();
    let v = f.run("out");
    assert_eq!(v["datasets"].as_array().unwrap().len(), 1);
    assert_eq!(v["datasets"][0]["unsynced_cal"], 0);
    assert!((group(&v, "kernel_rx", "Hw")["p50"].as_i64().unwrap() - 3000).abs() <= 100);
    assert_eq!(group(&v, "ingest", "Hw")["p50"], 1500);
    assert_eq!(group(&v, "kernel_rx", "Hw")["count"], 1000);
}

#[test]
fn rpl_191_unsynced_cal_excluded() {
    // Explicit bad calibration bounds isolate exclusion from interpolation assumptions.
    // Missing samples under a fixed slope alone cannot enlarge an interpolation error.
    let mut f = Fixture::new();
    for n in 0..1000 {
        f.clock(n, if (301..=359).contains(&n) { 2000 } else { 100 }, true);
        f.market((n + 1) * SEC + SEC / 2, 3000, false, 1, false, true);
    }
    f.clock(1000, 100, true);
    f.close();
    let v = f.run("out");
    assert_eq!(v["datasets"][0]["unsynced_cal"], 60);
    assert_eq!(group(&v, "kernel_rx", "Hw")["count"], 940);
    assert_eq!(group(&v, "ingest", "Hw")["count"], 1000);
    assert!(
        v["datasets"][0]["cal_err"]
            .as_array()
            .unwrap()
            .iter()
            .any(|g| g[1]["max"].as_u64().unwrap() > 1000)
    );
}

#[test]
fn rpl_192_sw_anchor() {
    let mut f = Fixture::new();
    f.clock(0, 100, false);
    for n in 0..800 {
        f.market(SEC + 1_000_000 + n * 100_000, 3000, n >= 500, 1, false, false);
    }
    f.clock(1, 100, false);
    f.close();
    let v = f.run("out");
    assert_eq!(group(&v, "kernel_rx", "Hw")["count"], 500);
    assert_eq!(group(&v, "kernel_rx", "Sw")["count"], 300);
    assert_eq!(v["datasets"][0]["sw_anchor_share"], 0.375);
}

fn burst() -> Fixture {
    let mut f = Fixture::new();
    for sec in 0..600 {
        f.clock(sec, 100, false);
        for n in 0..10 {
            f.market(
                (sec + 1) * SEC + 1_000_000 + n * 1_000_000,
                2000,
                false,
                1,
                false,
                false,
            );
        }
        if sec == 300 {
            for n in 0..5000 {
                f.market(
                    (sec + 1) * SEC + 100_000_000 + (n / 5) * 10_000,
                    40000,
                    false,
                    1,
                    n % 2 == 0,
                    false,
                );
            }
        }
    }
    f.clock(600, 100, false);
    f.close();
    f
}

#[test]
fn rpl_193_burst_visible() {
    use hdrhistogram::{Histogram, serialization::Deserializer};
    let f = burst();
    let v = f.run("out");
    let d = &v["datasets"][0];
    assert_eq!(d["n"], 11000);
    assert_eq!(
        d["unsynced_cal_by_stratum"][0],
        serde_json::json!(["Burst", 0, 5000])
    );
    assert_eq!(
        d["unsynced_cal_by_stratum"][1],
        serde_json::json!(["Calm", 0, 6000])
    );
    let bytes = std::fs::read(f.root.join("out/latency-0.hdr")).unwrap();
    let mut merged = Histogram::<u64>::new(3).unwrap();
    for s in d["histograms"].as_array().unwrap() {
        if s["upper_bound"] == false
            && s["key"]["kind"] == EventKind::BBO.0
            && s["key"]["stage"] == "kernel_rx"
        {
            let start = s["offset"].as_u64().unwrap() as usize;
            let len = s["bytes"].as_u64().unwrap() as usize;
            let h: Histogram<u64> = Deserializer::new()
                .deserialize(&mut &bytes[start..start + len])
                .unwrap();
            merged.add(&h).unwrap();
        }
    }
    assert_eq!(merged.len(), 8500);
    assert!(merged.value_at_quantile(0.99) >= 38000);
    let mut first_per_second = Histogram::<u64>::new(3).unwrap();
    for _ in 0..600 {
        first_per_second.record(2000).unwrap();
    }
    assert!(first_per_second.value_at_quantile(0.99) <= 2100);
}

#[test]
fn rpl_194_repeatable_bytes() {
    let f = burst();
    assert_eq!(f.run("a"), f.run("b"));
    for name in ["latency.json", "latency-0.hdr"] {
        assert_eq!(
            std::fs::read(f.root.join("a").join(name)).unwrap(),
            std::fs::read(f.root.join("b").join(name)).unwrap()
        );
    }
}

#[test]
fn rpl_197_kernel_rx_upper_bound() {
    for gap in [5000, 0, 1] {
        let mut f = Fixture::new();
        f.clock(0, 100, false);
        for n in 0..100 {
            f.market(SEC + 1_000_000 + n * 1000, 3000, false, gap, false, false);
        }
        f.clock(1, 100, false);
        f.close();
        let v = f.run("out");
        let d = &v["datasets"][0];
        if gap == 0 {
            assert_eq!(d["gap_missing"], 100);
            assert!(d["kernel_rx_hi"].as_array().unwrap().is_empty());
        } else {
            assert_eq!(d["kernel_rx_hi"][0][1]["count"], 100);
            assert_eq!(d["kernel_rx_hi"][0][1]["p50"], gap + 1000);
        }
    }
}

#[test]
fn rpl_199_unsynced_by_stratum() {
    let mut f = Fixture::new();
    f.clock(0, 100, false);
    for n in 0..6000 {
        f.market(SEC + 100_000 + n * 1000, 2000, false, 1, false, false);
    }
    f.clock(1, 100, false);
    f.clock(2, 2000, false);
    for n in 0..5000 {
        f.market(
            3 * SEC + 100_000 + (n / 5) * 1000,
            40000,
            false,
            1,
            n % 2 == 0,
            false,
        );
    }
    f.clock(3, 100, false);
    f.close();
    let v = f.run("out");
    let d = &v["datasets"][0];
    assert_eq!(d["unsynced_cal"], 5000);
    assert_eq!(
        d["unsynced_cal_by_stratum"][0],
        serde_json::json!(["Burst", 5000, 5000])
    );
    assert_eq!(
        d["unsynced_cal_by_stratum"][1],
        serde_json::json!(["Calm", 0, 6000])
    );
}

#[test]
fn rpl_205_missing_segment_exit_3() {
    let mut f = Fixture::new();
    for n in 0..3 {
        if n > 0 {
            f.roll();
        }
        f.clock(n, 100, false);
        f.market((n + 1) * SEC + 100_000, 3000, false, 1, false, false);
        f.clock(n + 1, 100, false);
    }
    f.close();
    let dir = f.root.join("input/prod/seq");
    let mut files: Vec<_> = std::fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|e| e == "jrn"))
        .collect();
    files.sort();
    assert_eq!(files.len(), 3);
    std::fs::remove_file(&files[1]).unwrap();
    let (code, _, err) = super::run(&f.args("out"));
    assert_eq!(code, 3, "{err}");
    assert!(!f.root.join("out/latency.json").exists());
}

#[test]
fn rpl_204_no_sequencer_no_exit_4() {
    // The fixture is produced by a real engine with test-only kinds absent from the release registry.
    let f = super::fixture::create::<hft_testkit::TestStrategies>();
    let a = vec![
        "latency".into(),
        "--journal".into(),
        f.file.display().to_string(),
        "--out".into(),
        f.root.join("latency").display().to_string(),
        "--cal-max-err-ns".into(),
        "1000".into(),
        "--burst-gap-ns".into(),
        "50000".into(),
    ];
    let (code, _, err) = super::run(&a);
    assert_eq!(code, 0, "{err}");
    let (code, _, err) = super::run(&super::args(&f.file));
    assert_eq!(code, 4, "{err}");
}
