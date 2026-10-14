//! Temporary, read-only schema audit. The real runner still compares the complete new state.
use hft_strategy_sdk::{SnapshotReader, SnapshotWriter};
use hft_testkit::dst::obs::Obs;
use hft_testkit::dst::sut::*;
use hft_testkit::dst::{RerunPolicy, run_record};
use hft_testkit::hash::{ComponentHashes, component_hashes_of_sections};
use hft_testkit::scenarios::{DstEngine, suites};
use hft_types::AccountId;
use std::cell::RefCell;

thread_local! {
    static PAIRS: RefCell<Vec<(ComponentHashes, ComponentHashes)>> = const { RefCell::new(Vec::new()) };
    static CASH_STATS: RefCell<(u64, u64, u64)> = const { RefCell::new((0, 0, 0)) };
}
struct Audit(DstEngine);
impl Sut for Audit {
    type Matcher = <DstEngine as Sut>::Matcher;
    fn boot(c: &BootCtx<'_>) -> Result<Self, BootFail> {
        DstEngine::boot(c).map(Self)
    }
    fn drain_observations(&mut self, o: &mut StepOut) {
        self.0.drain_observations(o);
    }
    fn process(&mut self, n: u64, t: u64, i: &InputRec, o: &mut StepOut) {
        self.0.process(n, t, i, o);
    }
    fn next_deadline(&self) -> Option<u64> {
        self.0.next_deadline()
    }
    fn fire_timers(&mut self, n: u64, t: u64, o: &mut StepOut) -> bool {
        self.0.fire_timers(n, t, o)
    }
    fn observe(&self, o: &mut Obs) {
        self.0.observe(o);
    }
    fn hashes(&self) -> ComponentHashes {
        let full = self.0.hashes();
        let mut w = SnapshotWriter::new(64 << 20);
        self.0.sequencer().write_sections(&mut w);
        w.status().unwrap();
        assert_eq!(full, component_hashes_of_sections(w.as_bytes()));
        let mut reader = SnapshotReader::new(w.as_bytes());
        let mut old = Vec::new();
        let mut found = 0;
        let mut slots = 0;
        let mut tombs = 0;
        loop {
            let at = reader.pos();
            let Some(section) = reader.next_section().unwrap() else {
                break;
            };
            let original = &w.as_bytes()[at..reader.pos()];
            let mut header = original[..16].to_vec();
            let mut body = section.body.to_vec();
            match section.section {
                2 => {
                    assert_eq!(section.version, 3);
                    assert_eq!(body.len(), section.count as usize * 192);
                    for row in body.chunks_exact(192) {
                        assert_eq!(&row[168..184], &[0; 16], "legacy worlds have no cash cursor");
                        slots += 1;
                    }
                    header[2..4].copy_from_slice(&2u16.to_le_bytes());
                    found += 1;
                }
                12 => {
                    assert_eq!(section.version, 2);
                    let mut p = 0;
                    let mut projected = Vec::new();
                    for _ in 0..section.count {
                        let kind = body[p];
                        let len = u32::from_le_bytes(body[p + 4..p + 8].try_into().unwrap()) as usize;
                        let row = &body[p + 8..p + 8 + len];
                        let mut row_header = body[p..p + 8].to_vec();
                        if kind == 4 {
                            assert_eq!(len, 80);
                            assert_eq!(&row[64..80], &[0; 16], "legacy tombs have no cash cursor");
                            row_header[4..8].copy_from_slice(&64u32.to_le_bytes());
                            projected.extend_from_slice(&row_header);
                            projected.extend_from_slice(&row[..64]);
                            tombs += 1;
                        } else {
                            projected.extend_from_slice(&row_header);
                            projected.extend_from_slice(row);
                        }
                        p += 8 + len;
                    }
                    assert_eq!(p, body.len());
                    body = projected;
                    header[2..4].copy_from_slice(&1u16.to_le_bytes());
                    header[8..12].copy_from_slice(&(body.len() as u32).to_le_bytes());
                    found += 1;
                }
                _ => {}
            }
            old.extend_from_slice(&header);
            old.extend_from_slice(&body);
        }
        assert_eq!(found, 2);
        CASH_STATS.with(|stats| {
            let mut stats = stats.borrow_mut();
            stats.0 += 1;
            stats.1 += slots;
            stats.2 += tombs;
        });
        let legacy = component_hashes_of_sections(&old);
        PAIRS.with(|p| p.borrow_mut().push((full, legacy)));
        full
    }
    fn rebuilt_hashes(&self) -> Result<Option<ComponentHashes>, String> {
        self.0.rebuilt_hashes()
    }
    fn rebuilt_snapshot(&self, p: &SnapshotPoint) -> Result<ComponentHashes, String> {
        self.0.rebuilt_snapshot(p)
    }
    fn seq(&self) -> u64 {
        self.0.seq()
    }
    fn journal(&self) -> &[JRec] {
        self.0.journal()
    }
    fn crash_and_recover(&mut self, p: CrashPoint, i: Option<&InputRec>) -> Result<(), BootFail> {
        self.0.crash_and_recover(p, i)
    }
    fn clean_restart(&mut self) -> Result<(), BootFail> {
        self.0.clean_restart()
    }
    fn is_sim(&self, a: AccountId) -> bool {
        self.0.is_sim(a)
    }
}
fn main() {
    let suite = suites().into_iter().find(|s| s.name == "core").unwrap();
    for seed in 1..=50 {
        PAIRS.with(|p| p.borrow_mut().clear());
        CASH_STATS.with(|stats| *stats.borrow_mut() = (0, 0, 0));
        let mut cfg = suite.config(seed);
        cfg.steps = 2_000;
        cfg.rerun = RerunPolicy::Never;
        let rec = run_record::<Audit>(&cfg);
        assert!(rec.outcome.is_pass(), "seed {seed}: {:?}", rec.outcome);
        assert!(rec.snapshot_checks > 0);
        let mut old = rec.trail.clone();
        PAIRS.with(|p| {
            let pairs = p.borrow();
            for (_, hash) in &mut old.points {
                let mut matches = pairs
                    .iter()
                    .filter(|(full, _)| full == hash)
                    .map(|(_, legacy)| *legacy);
                let first = matches.next().expect("every real trail point was observed");
                assert!(matches.all(|h| h == first));
                *hash = first;
            }
        });
        let full = xxhash_rust::xxh3::xxh3_128(&rec.trail.to_bytes());
        let legacy = xxhash_rust::xxh3::xxh3_128(&old.to_bytes());
        let (snapshots, slots, tombs) = CASH_STATS.with(|stats| *stats.borrow());
        println!(
            "{seed}\t{full:032x}\t{legacy:032x}\t{}\t{}\t{snapshots}\t{slots}\t{tombs}",
            rec.trail.points.len(),
            rec.snapshot_checks
        );
    }
}
