//! Row layouts of this crate's snapshot sections (SEQ-251; crates/hft-journal/TESTS.md JNL-007).

use core::mem::{offset_of, size_of};

use hft_sequencer::clock::{EngineClockRow, SeqDayRow};

#[test]
fn jnl_007_seqday_engineclock_row_layout() {
    assert_eq!(size_of::<SeqDayRow>(), 8);
    assert_eq!(offset_of!(SeqDayRow, last_day_rolled), 0);
    assert_eq!(offset_of!(SeqDayRow, _pad), 4);
    assert_eq!(size_of::<EngineClockRow>(), 32);
    assert_eq!(offset_of!(EngineClockRow, base_engine_ns), 0);
    assert_eq!(offset_of!(EngineClockRow, base_tsc), 8);
    assert_eq!(offset_of!(EngineClockRow, tsc_hz_milli), 16);
    assert_eq!(offset_of!(EngineClockRow, clock_seq), 24);
}
