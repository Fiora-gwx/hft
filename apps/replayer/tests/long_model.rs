use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;

use hft_event::{
    AdviceHdr, EngineClock, EventKind, OrderAction, OrderDecision, OrderEvent, OrderEventKind, SnapshotHdr,
};
use hft_sequencer::advisor::checkpoint;
use hft_sequencer::driver::replay::Log;
use hft_strategy_sdk::SnapshotReader;
use serde_json::Value;

use crate::fixture;

pub(super) fn three_hours() -> &'static fixture::Fixture {
    static F: OnceLock<fixture::Fixture> = OnceLock::new();
    F.get_or_init(|| fixture::path_model(3))
}
fn files(f: &fixture::Fixture) -> Vec<PathBuf> {
    let mut paths: Vec<_> = std::fs::read_dir(f.file.parent().unwrap())
        .unwrap()
        .map(|e| e.unwrap().path())
        .collect();
    paths.sort();
    paths
}
pub(super) fn options(checkpoint: &Path, from: Option<u64>) -> Vec<String> {
    let mut a = vec!["--model-checkpoint".into(), checkpoint.display().to_string()];
    if let Some(seq) = from {
        a.extend(["--from-seq".into(), seq.to_string()]);
    }
    a
}
pub(super) fn execute(f: &fixture::Fixture, path_v2: bool, extra: &[String]) -> (i32, Value, String) {
    execute_input(f, f.file.parent().unwrap(), path_v2, extra)
}
pub(super) fn execute_input(
    f: &fixture::Fixture,
    journal: &Path,
    path_v2: bool,
    extra: &[String],
) -> (i32, Value, String) {
    // CI builds --tests --examples before executing tests; no test invokes a compiler.
    let exe = std::env::current_exe()
        .unwrap()
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("examples")
        .join(format!("replay_probe{}", std::env::consts::EXE_SUFFIX));
    assert!(exe.is_file(), "CI prerequisite missing: {}", exe.display());
    let mut a = Vec::new();
    if path_v2 {
        a.push("--fixture-path-v2".into());
    }
    a.extend(crate::args(journal));
    a.extend(["--advice".into(), "recompute".into()]);
    a.extend_from_slice(extra);
    let out = Command::new(exe).args(&a).output().unwrap();
    if let Some(root) = std::env::var_os("HFT_REPLAY_EVIDENCE_DIR") {
        let dest = PathBuf::from(root).join(f.root.file_name().unwrap());
        let window = extra
            .iter()
            .position(|s| s == "--from-seq")
            .and_then(|i| extra.get(i + 1))
            .map_or("all", String::as_str);
        let mut label = format!(
            "replay-path{}-ck{}-{window}",
            if path_v2 { 2 } else { 1 },
            u8::from(extra.iter().any(|s| s == "--model-checkpoint"))
        );
        if let Some(at) = extra.iter().position(|s| s == "--report") {
            label.push('-');
            label.push_str(Path::new(&extra[at + 1]).file_stem().unwrap().to_str().unwrap());
        }
        std::fs::write(dest.join(format!("{label}.json")), &out.stdout).unwrap();
        std::fs::write(dest.join(format!("{label}.stderr.txt")), &out.stderr).unwrap();
    }
    (
        out.status.code().unwrap(),
        serde_json::from_slice(&out.stdout).unwrap_or(Value::Null),
        String::from_utf8(out.stderr).unwrap(),
    )
}
pub(super) fn passed(r: &(i32, Value, String)) {
    assert_eq!(r.0, 0, "{r:?}");
    assert_eq!(r.1["summary"]["verify"], "pass", "{r:?}");
    assert_eq!(r.1["summary"]["recompute"], "pass", "{r:?}");
    assert!(r.1["model_divergences"].as_array().unwrap().is_empty());
    assert!(r.1["model_no_start"].as_array().unwrap().is_empty());
}
fn advice_count(log: &Log, after: u64, min_basis: u64) -> usize {
    log.recs
        .iter()
        .filter(|r| {
            r.seq > after
                && r.kind == EventKind::ADVICE
                && r.read::<AdviceHdr>().is_some_and(|a| a.basis_seq >= min_basis)
        })
        .count()
}
pub(super) fn h2(log: &Log) -> u64 {
    log.recs
        .iter()
        .find(|r| {
            r.kind == EventKind::STATE_SNAPSHOT
                && r.read::<SnapshotHdr>()
                    .is_some_and(|h| h.engine_ns > 7_200_000_000_000)
        })
        .unwrap()
        .seq
}

#[test]
fn rpl_270_full_duration_path_model_files_and_checkpoint_window() {
    let one = fixture::path_model(1);
    for (hours, f) in [(1, &one), (3, three_hours())] {
        let before: Vec<_> = files(f)
            .into_iter()
            .map(|p| {
                let b = std::fs::read(&p).unwrap();
                (p, b)
            })
            .collect();
        let log = fixture::read(f);
        assert_eq!(log.seg_start.len(), hours + 1, "actual hourly rolls");
        let snap_times: Vec<_> = log
            .recs
            .iter()
            .filter(|r| r.kind == EventKind::STATE_SNAPSHOT)
            .map(|r| r.read::<SnapshotHdr>().unwrap().engine_ns)
            .collect();
        assert!(snap_times.len() >= hours * 12, "keyframes: {snap_times:?}");
        assert!(
            snap_times.windows(2).all(|x| x[1] - x[0] <= 310_000_000_000),
            "5 min schedule with 10 s input granularity"
        );
        assert!(log.recs.iter().any(|r| r.kind == EventKind::ORDER_DECISION));
        assert!(
            log.recs.iter().any(|r| r.kind == EventKind::ORDER_EVENT
                && r.read::<OrderEvent>()
                    .is_some_and(|e| e.what == OrderEventKind::Fill)),
            "no actual SimExchange fill"
        );
        let start = log
            .recs
            .iter()
            .find(|r| r.kind == EventKind::STATE_SNAPSHOT)
            .unwrap();
        let snapshot = SnapshotReader::new(&start.body[96..]);
        let epoch_section = snapshot.find_section(3).unwrap().unwrap();
        assert_eq!(epoch_section.version, 1);
        // Empty boot has a committed epoch in its snapshot without a recovery EpochStart note.
        let epoch = u16::try_from(epoch_section.reader().get_u32().unwrap() & 0x7fff).unwrap();
        assert_ne!(epoch, 0);
        let mut perpetual_fills = 0;
        for r in log.recs.iter().filter(|r| r.kind == EventKind::ORDER_EVENT) {
            let e = r.read::<OrderEvent>().unwrap();
            if !matches!(e.what, OrderEventKind::PartialFill | OrderEventKind::Fill) {
                continue;
            }
            assert_eq!(e.epoch, epoch, "fill seq {} lost gateway epoch", r.seq);
            assert!(
                log.recs.iter().any(|d| d.seq < r.seq
                    && d.kind == EventKind::ORDER_DECISION
                    && d.read::<OrderDecision>()
                        .is_some_and(|d| d.action == OrderAction::New
                            && d.reject == 0
                            && (d.account, d.venue, d.inst, d.slot, d.r#gen)
                                == (e.account, e.venue, e.inst, e.slot, e.r#gen))),
                "unmapped fixture fill at {}",
                r.seq
            );
            if e.venue.market() == Some(hft_types::MarketKind::LinearPerp) {
                assert!(e.cum_filled.0 > 0);
                assert_eq!(e.cash_qty, e.cum_filled);
                perpetual_fills += 1;
            }
        }
        assert!(
            perpetual_fills > 0,
            "strict cash fixture must produce perpetual fills"
        );
        let meta: Value =
            serde_json::from_slice(&std::fs::read(f.root.join("generation.json")).unwrap()).unwrap();
        assert_eq!(meta["engine_ns"], (hours as u64 * 3600 * 1_000_000_000));
        assert_eq!(meta["keyframe_period_ms"], 300000);
        assert_eq!(meta["keyframe_max_events"], 1500000);
        assert_eq!(meta["checkpoint_ms"], 600000);
        let commits = meta["checkpoints"].as_array().unwrap();
        assert!(commits.windows(2).all(|x| {
            let d = x[1][1].as_i64().unwrap() - x[0][1].as_i64().unwrap();
            (600_000_000_000..=610_000_000_000).contains(&d)
        }));
        let full = execute(f, false, &[]);
        passed(&full);
        assert_eq!(full.1["summary"]["coalesced"], 0, "{full:?}");
        let n = advice_count(&log, 0, 0);
        assert!(n >= hours * 720, "real BBO callbacks and Advice missing: {n}");
        assert_eq!(full.1["summary"]["model_advice_compared"], n);
        if hours == 3 {
            let path = f.root.join("ckpt-1h/tsa.ckpt");
            let bytes = std::fs::read(&path).unwrap();
            let c = checkpoint::decode(&bytes, 1 << 20).unwrap();
            assert_eq!(c.key.layout_version, 1);
            assert_eq!(c.path_version, 1);
            let from = h2(&log);
            assert!(c.key.last_seq < from);
            let r = execute(f, false, &options(&path, Some(from)));
            passed(&r);
            assert_eq!(
                r.1["summary"]["model_advice_compared"],
                advice_count(&log, from, 0)
            );
            assert_eq!(r.1["summary"]["checkpoint_key"][0]["strategy_instance"], "tsa");
            assert_eq!(r.1["summary"]["checkpoint_key"][0]["last_seq"], c.key.last_seq);
            assert_eq!(std::fs::read(&path).unwrap(), bytes);
        }
        for (p, bytes) in before {
            assert_eq!(std::fs::read(p).unwrap(), bytes);
        }
    }
}

/// Compute the comparison point from recorded clock anchors, independently of recompute's
/// timeline and SDK time helpers. All values are journal facts, never report outputs.
fn rebuild_point(log: &Log, last: u64, settle_ns: i128) -> u64 {
    let mut anchor = (0i64, 0u64, 1_000_000_000_000u64);
    let mut boundary = None;
    for r in &log.recs {
        if r.kind == EventKind::ENGINE_CLOCK {
            let c = r.read::<EngineClock>().unwrap();
            anchor = (c.base_engine_ns, r.tsc, c.tsc_hz_milli);
        }
        let ns = i128::from(anchor.0)
            + (i128::from(r.tsc) - i128::from(anchor.1)) * 1_000_000_000_000 / i128::from(anchor.2);
        if r.seq == last {
            boundary = Some(ns + settle_ns);
        }
        if r.seq > last && boundary.is_some_and(|b| ns >= b) {
            return r.seq;
        }
    }
    panic!("checkpoint or rebuild interval missing");
}

#[test]
fn rpl_271_path_version_rebuild_uses_actual_engine_time() {
    let f = three_hours();
    let log = fixture::read(f);
    let path = f.root.join("ckpt-1h/tsa.ckpt");
    let bytes = std::fs::read(&path).unwrap();
    let c = checkpoint::decode(&bytes, 1 << 20).unwrap();
    let from = rebuild_point(&log, c.key.last_seq, 120_000_000_000);
    let same = execute(f, false, &options(&path, None));
    passed(&same);
    assert_eq!(same.1["summary"]["recompute_from_seq"], Value::Null);
    let changed = execute(f, true, &options(&path, None));
    passed(&changed);
    assert_eq!(changed.1["summary"]["recompute_from_seq"], from);
    let count = advice_count(&log, c.key.last_seq, from);
    assert!(count > 0);
    assert_eq!(changed.1["summary"]["model_advice_compared"], count);
    assert!(count < same.1["summary"]["model_advice_compared"].as_u64().unwrap() as usize);
    assert_eq!(std::fs::read(&path).unwrap(), bytes);
}
