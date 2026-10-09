//! EXE-051: TargetCell size, AutoFixState layout and phase codes.
use core::mem::{offset_of, size_of};
use hft_exec::*;

#[test]
fn exe_051_cell_layout() {
    assert!(size_of::<TargetCell>() <= 640, "EXE-051.1");
    assert_eq!(size_of::<AutoFixState>(), 56, "EXE-051.2");
    assert_eq!(offset_of!(AutoFixState, exec_ms), 8, "EXE-051.3");
    assert_eq!(offset_of!(AutoFixState, due_tsc), 16, "EXE-051.4");
    assert_eq!(offset_of!(AutoFixState, started_tsc), 24, "EXE-051.5");
    assert_eq!(offset_of!(AutoFixState, run_mark_tsc), 32, "EXE-051.6");
    assert_eq!(offset_of!(AutoFixState, budget), 40, "EXE-051.7");
    assert_eq!(offset_of!(AutoFixState, done_qty), 48, "EXE-051.8");
    let codes = [
        (AutoFixPhase::None, 0),
        (AutoFixPhase::Primed, 1),
        (AutoFixPhase::WaitHard, 2),
        (AutoFixPhase::Running, 3),
        (AutoFixPhase::Done, 4),
        (AutoFixPhase::Failed, 5),
        (AutoFixPhase::Cancelled, 6),
        (AutoFixPhase::WaitHold, 7),
    ];
    for (p, c) in codes {
        assert_eq!(p as u8, c, "EXE-051.9");
        assert_eq!(AutoFixPhase::from_code(c), Some(p), "EXE-051.9");
    }
}
