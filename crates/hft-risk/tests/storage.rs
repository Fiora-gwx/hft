//! Storage degradation (RSK-220–RSK-222).
#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods
)] // allow-list: test code, not a hot path

mod common;
use common::*;

use hft_risk::mode::{DrainCause, GateEvent, ModeInput, StorageKind};
use hft_types::{CapSource, RejectBits, Side, Tif};

fn st(kind: StorageKind, level: u8) -> ModeInput<'static> {
    ModeInput::Storage { kind, level }
}
fn params(w: &W) -> Option<[u64; 2]> {
    w.cap(CapSource::STORAGE, sc_any()).map(|c| c.params)
}
const CRIT: Option<DrainCause> = Some(DrainCause::StorageCritical);

#[test]
fn rsk_220_storage_levels() {
    use StorageKind::*;
    let mut w = r0();
    w.ok(st(PersistLag, 1));
    assert_eq!(params(&w), Some([1, 0]), "RSK-220.1");
    assert_eq!(w.out.drain, None);
    let o = open(1, G1, A1, Side::Buy, "0.01", "50000.00", Tif::Gtc);
    assert_eq!(w.eval(&o).map(|_| ()), Err(RejectBits::CLOSE_ONLY));
    assert!(
        w.eval(&hedge(1, G1, B1, Side::Sell, "0.01", "49995.0", "0.01"))
            .is_ok()
    );
    let mut w = r0();
    w.ok(st(PersistLag, 1));
    w.ok(st(DiskLow, 1));
    assert_eq!(params(&w), Some([1, 1]), "RSK-220.2");
    assert_eq!(w.out.notes.iter().filter(|x| note_n(x) == N_CAP_SET).count(), 1);
    let mut w = r0();
    w.ok(st(DiskLow, 2));
    assert_eq!(params(&w), Some([0, 2]), "RSK-220.3");
    assert_eq!(w.out.drain, CRIT);
    let mut w = r0();
    w.ok(st(PersistLag, 1));
    w.ok(st(Both, 0));
    assert_eq!(params(&w), None, "RSK-220.4");
    assert_eq!(
        w.out.notes.iter().filter(|x| note_n(x) == N_CAP_CLEARED).count(),
        1
    );
    let mut w = r0();
    w.ok(draining());
    w.ok(st(PersistLag, 2));
    assert_eq!(w.out.drain, None, "RSK-220.5");
}

#[test]
fn rsk_221_deferred_until_targets_reached() {
    let mut w = r0();
    w.ok(kill_flatten(1));
    w.mm.pending = true;
    w.ok(st(StorageKind::PersistLag, 2));
    assert_eq!(w.out.drain, None, "step 1");
    assert!(w.risk.mode.drain_deferred());
    assert!(params(&w).is_some());
    let v = w.risk.mode.ctl_state_version();
    let caps = w.risk.mode.caps().to_vec();
    w.ok(ModeInput::StorageRecheck);
    assert_eq!(w.out.drain, None, "step 2");
    assert_eq!(w.risk.mode.caps(), caps.as_slice());
    assert_eq!(w.risk.mode.ctl_state_version(), v);
    w.mm.pending = false;
    w.ok(ModeInput::StorageRecheck);
    assert_eq!(w.out.drain, CRIT, "step 3");
    w.ok(draining());
    w.ok(ModeInput::StorageRecheck);
    assert!(!w.risk.mode.drain_deferred(), "step 4");
    assert_eq!(w.out.drain, None, "step 4 once");
}

#[test]
fn rsk_222_defer_condition_is_targets() {
    let mut w = r0();
    w.ok(kill_flatten(1));
    w.ok(st(StorageKind::DiskLow, 2));
    assert_eq!(w.out.drain, CRIT, "RSK-222.1");
    let mut w = r0();
    w.ok(margin_auto(1, 3, 1 << B.0));
    w.ok(st(StorageKind::DiskLow, 2));
    assert_eq!(w.out.drain, CRIT, "RSK-222.2");
    let mut w = r0();
    w.mm.pending = true;
    w.ok(st(StorageKind::DiskLow, 2));
    assert_eq!(w.out.drain, None, "RSK-222.3");
    w.mm.pending = false;
    w.ok(ModeInput::Gate(GateEvent::OrderConns {
        account: acc(2),
        venue: A,
        primary_up: true,
        standby_up: true,
    }));
    assert_eq!(w.out.drain, CRIT, "RSK-222.4");
}
