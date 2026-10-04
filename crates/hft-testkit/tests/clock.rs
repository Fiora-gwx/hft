//! TK-022, TK-023: manual clocks.

use hft_rt::ClockReads;
use hft_testkit::{ManualTsc, ManualUtc};
use hft_types::Tsc;

#[test]
fn tk_022_manual_tsc() {
    let c = ManualTsc::new(3_000_000_000, 1000);
    assert_eq!(c.now(), Tsc(1000), "TK-022.1");
    c.advance_ns(1000);
    assert_eq!(c.now(), Tsc(4000), "TK-022.2");
    c.advance_ns(1);
    assert_eq!(c.now(), Tsc(4003), "TK-022.3");
    c.set(10);
    assert_eq!(c.now(), Tsc(10), "TK-022.4");
    // The hft-rt reading trait sees the same counter.
    let mut r = &c;
    assert_eq!(r.tsc(), Tsc(10), "TK-022.4 via ClockReads");
}

#[test]
fn tk_023_manual_utc() {
    let u = ManualUtc::new(1_000_000_000, 1_700_000_000_000_000_000, 1_000_000_000);
    assert_eq!(u.utc_ns(1_000_000_500), 1_700_000_000_000_000_500);
    // hft-rt's distributed conversion built from the same pair agrees.
    assert_eq!(u.tsc_utc().utc_ns(Tsc(1_000_000_500)), 1_700_000_000_000_000_500);
}
