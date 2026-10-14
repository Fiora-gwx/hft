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
        let mut found = false;
        loop {
            let at = reader.pos();
            let Some(section) = reader.next_section().unwrap() else {
                break;
            };
            if section.section == 27 {
                assert!(!found);
                found = true;
                let mut rows = section.reader();
                for _ in 0..section.count {
                    let h: hft_sequencer::snapshot::StrategyTimersHdr = rows.get_row().unwrap();
                    assert_eq!(h.n_timers, 0, "audit seeds must not allocate strategy timers");
                    assert_eq!(h.next_id, 1);
                }
                assert!(rows.is_empty());
            } else {
                old.extend_from_slice(&w.as_bytes()[at..reader.pos()]);
            }
        }
        assert!(found);
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
        println!(
            "{seed}\t{full:032x}\t{legacy:032x}\t{}\t{}",
            rec.trail.points.len(),
            rec.snapshot_checks
        );
    }
}
