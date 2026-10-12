//! TK-137: old actor reproduces its goldens; new actor's first difference is a recon Input.
//! The new actor is then compared byte-for-byte with every checker disabled.
use hft_event::{EventKind, ReconcileReport};
use hft_testkit::dst::actors::ReconActor;
use hft_testkit::dst::obs::{Obs, TruthRow};
use hft_testkit::dst::sut::{
    BootCtx, BootFail, CrashPoint, InputRec, JRec, SnapshotPoint, Source, StepOut, Sut,
};
use hft_testkit::dst::venue_behavior::ExternalReceipt;
use hft_testkit::dst::{InvariantSet, RerunPolicy, run_record};
use hft_testkit::hash::ComponentHashes;
use hft_testkit::scenarios::DstEngine;
use hft_types::AccountId;
use std::fmt::Write;

struct PriorActor(DstEngine, std::cell::Cell<u64>);

impl Sut for PriorActor {
    type Matcher = <DstEngine as Sut>::Matcher;
    fn boot(c: &BootCtx<'_>) -> Result<Self, BootFail> {
        DstEngine::boot(c).map(|engine| Self(engine, std::cell::Cell::new(0)))
    }
    fn process(&mut self, n: u64, t: u64, i: &InputRec, o: &mut StepOut) {
        self.0.process(n, t, i, o);
    }
    fn drain_observations(&mut self, o: &mut StepOut) {
        self.0.drain_observations(o);
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
        self.0.hashes()
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
    fn reconcile_inputs(
        &self,
        actor: &mut ReconActor,
        truth: &[TruthRow],
        _: &[ExternalReceipt],
    ) -> Result<Option<Vec<InputRec>>, String> {
        let mut obs = Obs::default();
        self.0.observe(&mut obs);
        // The world schedules this actor at period, 2 * period, ... . Its event may
        // precede the engine's next clock Input, so the driver clock can still lag.
        let now = self.1.get().checked_add(actor.period_ns).unwrap();
        self.1.set(now);
        // The pre-change adapter submitted Source::Recon through Rep(0), not Ctl.
        let venue = self
            .0
            .world()
            .venues
            .iter()
            .map(|v| v.id)
            .min_by_key(|v| v.0)
            .unwrap();
        Ok(Some(
            actor
                .round(now, truth, &|a, v, i| obs.pos(a, v, i))
                .iter()
                .map(|r| InputRec::of(r, Source::Venue(venue)))
                .collect(),
        ))
    }
}

fn value<'a>(row: &'a str, name: &str) -> &'a str {
    row.lines()
        .find_map(|l| {
            l.split_once(" = ")
                .filter(|(k, _)| *k == name)
                .map(|(_, v)| v.trim_matches('"'))
        })
        .unwrap()
}

#[test]
fn tk_137_recon_migration_preserves_prefix_and_checker_transparency() {
    let old = include_str!("../../../fixtures/dst/recon-before.toml");
    let suites = hft_testkit::scenarios::suites();
    let mut rows = Vec::new();
    for row in old.split("[[seed]]").skip(1) {
        let seed = u64::from_str_radix(value(row, "seed").trim_start_matches("0x"), 16).unwrap();
        let suite = suites.iter().find(|s| s.name == value(row, "suite")).unwrap();
        let steps = value(row, "steps").parse::<u64>().unwrap();
        let expected = value(row, "trail");
        let mut cfg = suite.config(seed);
        cfg.steps = steps;
        cfg.rerun = RerunPolicy::Never;
        let before = run_record::<PriorActor>(&cfg);
        let after = run_record::<DstEngine>(&cfg);
        for r in [&before, &after] {
            assert!(
                r.outcome.is_pass(),
                "seed {seed} failure {:?}",
                r.outcome.failure()
            );
            assert_eq!(r.steps_run, steps);
        }
        let digest = |r: &hft_testkit::dst::RunRecord| {
            format!("{:032x}", xxhash_rust::xxh3::xxh3_128(&r.trail.to_bytes()))
        };
        assert_eq!(
            digest(&before),
            expected,
            "seed {seed}: old actor no longer reproduces the original golden"
        );
        let first = before
            .journal
            .iter()
            .zip(&after.journal)
            .position(|(a, b)| a != b)
            .unwrap();
        let (a, b) = (&before.journal[first], &after.journal[first]);
        assert_eq!(
            (a.kind, b.kind),
            (EventKind::RECONCILE_REPORT, EventKind::RECONCILE_REPORT),
            "seed {seed}: unexpected first difference"
        );
        assert_eq!(
            a.tsc, 30_000_000_000,
            "seed {seed}: changed before first recon round"
        );
        assert_eq!(a.tsc, b.tsc);
        let report = b.read::<ReconcileReport>().unwrap();
        assert_eq!((report.item, report.idx), (5, 0));
        assert!(report.account.0 != 0 && report.view_seq > 0 && report.view_seq < b.seq);
        assert_eq!(&before.journal[..first], &after.journal[..first]);
        cfg.invariants = InvariantSet::none();
        let off = run_record::<DstEngine>(&cfg);
        assert!(off.outcome.is_pass(), "seed {seed}: checker-off run failed");
        assert_eq!(off.steps_run, steps);
        assert_eq!(after.journal.len(), off.journal.len());
        for (index, (a, b)) in after.journal.iter().zip(&off.journal).enumerate() {
            assert!(a == b, "seed {seed}: checker changes journal record {index}");
        }
        assert_eq!(
            after.frames, off.frames,
            "seed {seed}: checker changes emitted intents"
        );
        assert_eq!(
            after.trail.to_bytes(),
            off.trail.to_bytes(),
            "seed {seed}: checker changes full HashTrail"
        );
        assert_eq!(
            after.truth, off.truth,
            "seed {seed}: checker changes exchange truth"
        );
        let mut line = String::new();
        write!(&mut line,
            "{{\"seed\":\"{seed:#018x}\",\"suite\":\"{}\",\"steps\":{steps},\"before\":\"{}\",\"after\":\"{}\",\"first_seq\":{},\"first_tsc\":{},\"before_records\":{},\"after_records\":{},\"prefix_equal\":true,\"checker_equal\":true}}",
            suite.name, digest(&before), digest(&after), b.seq, b.tsc, before.journal.len(), after.journal.len()).unwrap();
        rows.push(line);
    }
    assert_eq!(rows.len(), 50);
    let path =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/dst/reconcile-migration.json");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let run_id = std::env::var("GITHUB_RUN_ID")
        .ok()
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(0);
    std::fs::write(
        path,
        format!(
            "{{\"baseline\":\"af9edb9\",\"run_id\":{run_id},\"seeds\":[{}]}}\n",
            rows.join(",")
        ),
    )
    .unwrap();
}
