//! `Expiring<T>` (TESTS.md SLB-062–SLB-067).

use crate::common;

use common::{now, snap_expiring, tsc_at};
use hft_strat_lib::{Expiring, Fresh};
use hft_strategy_sdk::{RestoreError, SnapshotReader};
use hft_types::Tsc;

fn get_i64(r: &mut SnapshotReader<'_>) -> Result<i64, RestoreError> {
    r.get_i64()
}

#[test]
fn slb_062_get() {
    let e0 = Expiring::new(0i64);
    assert_eq!(e0.get(&now(999_999_999_999, 40)), Fresh::Missing, "SLB-062.1");
    let mut e = Expiring::new(0i64);
    e.put(7, 1_000_000_000_000, 50);
    let rows = [
        ("SLB-062.2", 999_999_999_999, 40, Fresh::Fresh(&7)),
        ("SLB-062.3", 1_000_000_000_000, 40, Fresh::Fresh(&7)),
        ("SLB-062.4", 1_000_000_000_001, 40, Fresh::Stale(&7)),
        ("SLB-062.5", 999_999_999_999, 51, Fresh::Stale(&7)),
        ("SLB-062.6", 999_999_999_999, 50, Fresh::Fresh(&7)),
    ];
    for (row, ens, boot, want) in rows {
        assert_eq!(e.get(&now(ens, boot)), want, "{row}");
    }
}

#[test]
fn slb_063_put_advice_basis_time() {
    let n = now(15_002_000_000, 80);
    assert_eq!(n.ens_of(Tsc(30_000_000_000)), 15_000_000_000);
    let mut e = Expiring::new(0i64);
    e.put_advice(9, 90, Tsc(30_000_000_000), 5, &n);
    assert_eq!(e.get(&now(15_005_000_000, 80)), Fresh::Fresh(&9));
    assert_eq!(e.get(&now(15_005_000_001, 80)), Fresh::Stale(&9));
}

#[test]
fn slb_064_arrival_time_irrelevant() {
    let mut e1 = Expiring::new(0i64);
    let mut e2 = Expiring::new(0i64);
    e1.put_advice(9, 90, Tsc(30_000_000_000), 5, &now(15_001_000_000, 80));
    e2.put_advice(9, 90, Tsc(30_000_000_000), 5, &now(15_004_000_000, 80));
    let (a, b) = (snap_expiring(&e1), snap_expiring(&e2));
    assert_eq!(a, b);
    assert_eq!(i64::from_le_bytes(a[1..9].try_into().unwrap()), 15_005_000_000);
    let _ = tsc_at;
}

fn e1_65() -> Expiring<i64> {
    let mut e = Expiring::new(0i64);
    e.put(7, 1_000_000_000_000, 50);
    e
}

#[test]
fn slb_065_snapshot_layout() {
    let (a, b) = (snap_expiring(&e1_65()), snap_expiring(&Expiring::new(0)));
    let mut want = vec![1u8];
    want.extend_from_slice(&1_000_000_000_000i64.to_le_bytes());
    want.extend_from_slice(&50u64.to_le_bytes());
    want.extend_from_slice(&7i64.to_le_bytes());
    assert_eq!(a, want);
    assert_eq!(b, vec![0u8; 17]);
    let n = now(999_999_999_999, 40);
    let mut x = Expiring::new(0i64);
    x.restore(&mut SnapshotReader::new(&a), get_i64).unwrap();
    assert_eq!(x.get(&n), Fresh::Fresh(&7));
    let mut y = Expiring::new(0i64);
    y.restore(&mut SnapshotReader::new(&b), get_i64).unwrap();
    assert_eq!(y.get(&n), Fresh::Missing);
}

#[test]
fn slb_066_prior_boot_stale() {
    let mut e = Expiring::new(0i64);
    e.put(7, i64::MAX, 50);
    let b = snap_expiring(&e);
    let mut r = Expiring::new(0i64);
    r.restore(&mut SnapshotReader::new(&b), get_i64).unwrap();
    assert_eq!(r.get(&now(2_000_000_000_000, 100)), Fresh::Stale(&7));
    r.put(8, i64::MAX, 120);
    assert_eq!(r.get(&now(2_000_000_000_001, 100)), Fresh::Fresh(&8));
}

#[test]
fn slb_067_restore_error_keeps_state() {
    let a = snap_expiring(&e1_65());
    let mut t = Expiring::new(0i64);
    t.put(3, 500, 1);
    assert!(t.restore(&mut SnapshotReader::new(&a[..24]), get_i64).is_err());
    assert_eq!(t.get(&now(400, 1)), Fresh::Fresh(&3));
}
