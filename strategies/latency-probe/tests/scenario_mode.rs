//! Mode changes (TESTS.md LPR-049).
#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods,
    clippy::type_complexity
)] // allow-list: test code

mod common;

use common::*;
use hft_types::RunMode;
use latency_probe::Phase;

fn mode_ev(h: &mut H, g: hft_types::GroupId, old: RunMode, new: RunMode) -> Rec {
    h.mode(g, new);
    let gix = h.gix_of(g);
    h.ev(&ev::mode(gix, old, new))
}

#[test]
fn lpr_049_mode_events() {
    // .1 GPS Due + Live → CloseOnly.
    let mut h = H::new(&[], vec![gps()]);
    h.bbo(I2);
    h.timer(GPS, 1);
    let rec = mode_ev(&mut h, GPS, RunMode::Live, RunMode::CloseOnly);
    assert_eq!((h.phase(GPS), rec.intents()), (Phase::Idle, 0), "LPR-049.1");

    // .2 GTS Opening + Live → Shadow at t.
    let mut h = H::new(&[], vec![gts()]);
    h.bbo(I2);
    h.timer(GTS, 1);
    assert_eq!(h.bbo(I1).targets.len(), 1, "LPR-049.2");
    h.at(300);
    let rec = mode_ev(&mut h, GTS, RunMode::Live, RunMode::Shadow);
    assert_eq!(rec.clears, vec![(T1, GTS)], "LPR-049.2");
    assert_eq!(h.phase(GTS), Phase::Holding, "LPR-049.2");
    let flat: Vec<_> = rec.timers.iter().filter(|t| t.tag == 2).collect();
    assert_eq!(
        (flat.len(), flat[0].at),
        (1, h.tsc_of(ms(300) + 5_000_000_000)),
        "LPR-049.2"
    );

    // .3 GQ probed at +0 and cancelled; CloseOnly → Live at +300; Bbo at +301.
    let mut h = H::new(&[], vec![gq()]);
    let r = h.bbo(I2).refs[0];
    h.at(50);
    h.order(GQ, r, probe_tag(0), acked());
    h.at(80);
    h.order(GQ, r, probe_tag(0), cancelled());
    h.mode(GQ, RunMode::CloseOnly);
    h.at(300);
    mode_ev(&mut h, GQ, RunMode::CloseOnly, RunMode::Live);
    h.at(301);
    assert_eq!(h.bbo(I2).places.len(), 1, "LPR-049.3");

    // .4 GPS Idle with next_pair_ens = +10 000 ms; Shadow → Live at +400 registers PAIR_TICK now.
    let mut h = H::new(&[], vec![gps()]);
    h.bbo(I2);
    h.timer(GPS, 1);
    let r = h.bbo(I1).refs[0];
    h.order(GPS, r, OPEN_TAG, cancelled());
    assert_eq!(
        (h.phase(GPS), h.group(GPS).next_pair_ens),
        (Phase::Idle, ms(10_000)),
        "LPR-049.4"
    );
    h.mode(GPS, RunMode::Shadow);
    h.at(400);
    let rec = mode_ev(&mut h, GPS, RunMode::Shadow, RunMode::Live);
    let ticks: Vec<_> = rec.timers.iter().filter(|t| t.tag == 1).collect();
    assert_eq!(
        (ticks.len(), ticks[0].at, ticks[0].group),
        (1, h.tsc_of(ms(400)), GPS),
        "LPR-049.4"
    );

    // .5 GPS Holding + Live → CloseOnly: nothing, flattening still happens at FLAT.
    let mut h = H::new(&[], vec![gps()]);
    h.bbo(I2);
    h.timer(GPS, 1);
    let r = h.bbo(I1).refs[0];
    h.order(GPS, r, OPEN_TAG, filled(100_000, 6_030_001, true));
    let rec = mode_ev(&mut h, GPS, RunMode::Live, RunMode::CloseOnly);
    assert_eq!((h.phase(GPS), rec.intents()), (Phase::Holding, 0), "LPR-049.5");
    h.net(GPS, 0, 100_000).net(GPS, 1, -100_000);
    assert_eq!(h.timer(GPS, 2).places.len(), 1, "LPR-049.5");
}
