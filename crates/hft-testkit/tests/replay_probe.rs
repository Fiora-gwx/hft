use hft_strategy_sdk::params::RawParams;
use hft_strategy_sdk::testing::{check_layout, ev, layout_json};
use hft_strategy_sdk::{AdviceBufs, BboView, Model, SnapshotReader, SnapshotWriter, StrategyInstance};
use hft_testkit::MockViews;
use hft_testkit::strategies::replay_probe::{ReplayModel, ReplayParams};
use hft_types::{Px, StrategyId};

use crate::sdk_rd::{A1, I1, gt1, gt3_fixture};

fn model<const PATH: u32>(v: &MockViews) -> ReplayModel<PATH> {
    let p = ReplayParams::from_raw(
        &RawParams {
            defaults: vec![0, 1, 7, 5000, 120000],
            overrides: vec![],
        },
        1,
    );
    assert_eq!(ReplayModel::<PATH>::path_settle_ms(&p), 120_000);
    ReplayModel::new(
        StrategyInstance {
            strategy: StrategyId(1),
            account: A1,
            kind_id: 0xFF04,
        },
        &p,
        &gt1(),
        &v.refview(),
    )
}
fn feed<const PATH: u32>(m: &mut ReplayModel<PATH>, v: &mut MockViews, second: i64) -> Vec<u8> {
    v.at_engine_ns(second * 1_000_000_000);
    let e = ev::bbo(
        I1,
        0,
        BboView {
            bid_px: Px(100 + second),
            ..BboView::EMPTY
        },
    );
    let mut buf = Box::new(AdviceBufs::new());
    buf.begin();
    m.on_event(&e.as_ref(), v, &mut buf.out());
    let items: Vec<_> = buf.iter().map(|(_, _, b)| b.to_vec()).collect();
    assert_eq!(items.len(), 1);
    items[0].clone()
}
fn path<const PATH: u32>(m: &ReplayModel<PATH>) -> Vec<u8> {
    let mut w = SnapshotWriter::new(4096);
    m.snapshot_path(&mut w);
    w.status().unwrap();
    w.as_bytes().to_vec()
}

#[test]
fn tk_117_replay_model_path_restore_and_convergence() {
    let mut v = MockViews::new(gt3_fixture());
    let mut live = model::<1>(&v);
    for second in (10..=60).step_by(10) {
        feed(&mut live, &mut v, second);
    }
    check_layout(&live).unwrap();
    let mut w = SnapshotWriter::new(8);
    live.snapshot_chunk(0, &mut w, 8);
    assert_eq!(w.as_bytes(), 6u64.to_le_bytes());
    let saved_path = path(&live);
    let mut exact = model::<1>(&v);
    exact.restore(&mut SnapshotReader::new(w.as_bytes())).unwrap();
    exact.restore_path(&mut SnapshotReader::new(&saved_path)).unwrap();
    let mut approximate = model::<2>(&v);
    approximate
        .restore(&mut SnapshotReader::new(w.as_bytes()))
        .unwrap();
    assert_ne!(
        path(&approximate),
        saved_path,
        "raw restore must not fabricate the lost path"
    );
    for second in (70..=240).step_by(10) {
        let expected = feed(&mut live, &mut v, second);
        assert_eq!(feed(&mut exact, &mut v, second), expected);
        let regenerated = feed(&mut approximate, &mut v, second);
        assert_eq!(
            &regenerated[..16],
            &expected[..16],
            "raw count and engine time continue"
        );
        if second < 180 {
            assert_ne!(regenerated, expected, "premature convergence at {second}");
        } else {
            assert_eq!(regenerated, expected, "120 s bound at {second}");
        }
    }
    let before = path(&exact);
    assert!(
        exact
            .restore_path(&mut SnapshotReader::new(&saved_path[..31]))
            .is_err()
    );
    assert_eq!(path(&exact), before, "failed path restore is transactional");
}

#[test]
fn tk_118_replay_model_layout_and_path_goldens() {
    assert_eq!(ReplayModel::<1>::LAYOUT_VERSION, 1);
    assert_eq!(ReplayModel::<1>::PATH_VERSION, 1);
    let golden = include_str!("../../../fixtures/strategies/sdk-replay/ckpt/layout.json").trim_end();
    assert_eq!(layout_json(ReplayModel::<1>::LAYOUT), golden);
    let mut v = MockViews::new(gt3_fixture());
    let mut m = model::<1>(&v);
    feed(&mut m, &mut v, 1);
    feed(&mut m, &mut v, 2);
    let saved = path(&m);
    assert_eq!(
        saved,
        include_bytes!("../../../fixtures/strategies/sdk-replay/ckpt/path.bin")
    );
    check_layout(&m).unwrap();
    let mut restored = model::<1>(&v);
    restored.restore_path(&mut SnapshotReader::new(&saved)).unwrap();
    assert_eq!(path(&restored), saved);
    // A timestamp stored in the wrong one-second bucket is rejected without modifying state.
    let mut bad = saved.clone();
    bad[16..24].copy_from_slice(&2_000_000_000i64.to_le_bytes());
    assert!(restored.restore_path(&mut SnapshotReader::new(&bad)).is_err());
    assert_eq!(path(&restored), saved);
}

#[test]
fn tk_119_warmup_model_observes_chunks_and_restores_transactionally() {
    use hft_strategy_sdk::EventRef;
    use hft_testkit::strategies::replay_warmup::WarmupModel;
    let v = MockViews::new(gt3_fixture());
    let p = ReplayParams::from_raw(
        &RawParams {
            defaults: vec![0, 1, 7, 5000, 120000],
            overrides: vec![],
        },
        1,
    );
    let fresh = || {
        WarmupModel::new(
            StrategyInstance {
                strategy: StrategyId(1),
                account: A1,
                kind_id: 0xFF05,
            },
            &p,
            &gt1(),
            &v.refview(),
        )
    };
    for (row, (order, expected)) in [
        (vec![0, 1, 2], [3, 0x010203, 81, 4, 0]),
        (vec![0, 2], [2, 0x0103, 58, 4, 0]),
        (vec![0, 1, 1, 2], [4, 0x01020203, 104, 4, 0]),
        (vec![2, 1, 0], [3, 0x030201, 81, 4, 0]),
    ]
    .into_iter()
    .enumerate()
    {
        let id = format!("TK-119.{}", row + 1);
        let mut m = fresh();
        let mut buf = Box::new(AdviceBufs::new());
        for no in order {
            buf.begin();
            m.on_event(
                &EventRef::Warmup {
                    local_kind: 9,
                    chunk_no: no as u32,
                    last: no == 2,
                    payload: &[11u64, 23, 47][no].to_le_bytes(),
                },
                &v,
                &mut buf.out(),
            );
        }
        let raw: Vec<_> = expected.into_iter().flat_map(u64::to_le_bytes).collect();
        let items: Vec<_> = buf.iter().map(|(_, _, b)| b.to_vec()).collect();
        assert_eq!(items, vec![raw.clone()], "{id}: warmup truth table");
        check_layout(&m).unwrap();
        let mut saved = SnapshotWriter::new(40);
        m.snapshot_chunk(0, &mut saved, 40);
        saved.status().unwrap();
        assert_eq!(saved.as_bytes(), raw, "{id}: snapshot words");
        let mut restored = fresh();
        restored.restore(&mut SnapshotReader::new(&raw)).unwrap();
        assert!(
            restored.restore(&mut SnapshotReader::new(&raw[..39])).is_err(),
            "{id}: truncated restore"
        );
        let mut after = SnapshotWriter::new(40);
        restored.snapshot_chunk(0, &mut after, 40);
        assert_eq!(after.as_bytes(), raw, "{id}: failed restore preserves state");
    }
}
