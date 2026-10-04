//! Heartbeat counting and kinds; HeartbeatWatch (RT-092, RT-093).
#![cfg(not(loom))]

use hft_rt::{HbKind, Heartbeat, HeartbeatWatch, Progress};

#[test]
fn rt_092_heartbeat_beat_and_kinds() {
    let hb = Heartbeat::new();
    for _ in 0..3 {
        hb.beat();
    }
    assert_eq!(hb.read(), 3, "RT-092.1");
    let k = |k: HbKind| k as u8;
    assert_eq!(
        [
            k(HbKind::Sequencer),
            k(HbKind::Feed),
            k(HbKind::Gw),
            k(HbKind::Persist),
            k(HbKind::Housekeeping)
        ],
        [1, 2, 3, 4, 5],
        "RT-092.2"
    );
    assert_eq!(
        [
            k(HbKind::Advisor),
            k(HbKind::Ctl),
            k(HbKind::Ext),
            k(HbKind::ChainWorker),
            k(HbKind::Mixed)
        ],
        [6, 7, 8, 9, 10],
        "RT-092.3"
    );
}

#[test]
fn rt_093_heartbeat_watch() {
    let hb = Heartbeat::new();
    let mut w = HeartbeatWatch::new();
    assert_eq!(w.check(&hb), Progress::Advanced, "RT-093.1");
    assert_eq!(w.check(&hb), Progress::Stalled { for_checks: 1 }, "RT-093.2");
    assert_eq!(w.check(&hb), Progress::Stalled { for_checks: 2 }, "RT-093.3");
    assert_eq!(w.check(&hb), Progress::Stalled { for_checks: 3 }, "RT-093.4");
    hb.beat();
    assert_eq!(w.check(&hb), Progress::Advanced, "RT-093.5");
    assert_eq!(w.check(&hb), Progress::Stalled { for_checks: 1 }, "RT-093.6");
}
