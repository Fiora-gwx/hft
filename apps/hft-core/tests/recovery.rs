use hft_core::recovery::{InstanceLease, next_epoch};
use hft_journal::{DurableMeta, InstanceName};

#[test]
fn core_241_next_epoch() {
    for (snapshot, durable, want) in [
        (Some(5), Some(5), 6),
        (Some(5), Some(9), 10),
        (Some(9), Some(5), 10),
        (None, None, 1),
        (Some(3), Some(7), 8),
        (Some(u32::MAX - 1), None, u32::MAX),
    ] {
        assert_eq!(next_epoch(snapshot, durable).unwrap(), want);
    }
    for (snapshot, durable) in [
        (Some(u32::MAX), None),
        (None, Some(u64::MAX)),
        (None, Some(u64::from(u32::MAX))),
    ] {
        let error = next_epoch(snapshot, durable).unwrap_err();
        assert_eq!((error.code, error.exit), ("Epoch", 4));
    }
}

#[test]
fn core_242_reservation_survives_owner_exit_and_never_reuses_epoch() {
    let root = tempfile::tempdir().unwrap();
    let meta = DurableMeta::open(root.path(), &InstanceName::Prod).unwrap();
    meta.write_sync("engine_epoch", 4).unwrap();
    let lease = InstanceLease::acquire(root.path(), InstanceName::Prod).unwrap();
    assert!(InstanceLease::acquire(root.path(), InstanceName::Prod).is_err());
    assert_eq!(lease.reserve_epoch(&meta, None).unwrap(), 5);
    // The order-capable caller cannot obtain the epoch until durable storage reads it back.
    assert_eq!(
        DurableMeta::open(root.path(), &InstanceName::Prod)
            .unwrap()
            .read("engine_epoch")
            .unwrap(),
        Some(5)
    );
    drop(lease);
    assert!(root.path().join("prod/core.lock").exists());
    let lease = InstanceLease::acquire(root.path(), InstanceName::Prod).unwrap();
    assert_eq!(lease.reserve_epoch(&meta, Some(3)).unwrap(), 6);
    let sim = InstanceLease::acquire(root.path(), InstanceName::Sim).unwrap();
    let sim_meta = DurableMeta::open(root.path(), &InstanceName::Sim).unwrap();
    assert_eq!(sim.reserve_epoch(&sim_meta, None).unwrap(), 1);
    std::fs::write(root.path().join("prod/meta/engine_epoch"), b"corrupt").unwrap();
    assert_eq!(lease.reserve_epoch(&meta, None).unwrap_err().exit, 4);
    assert_eq!(
        std::fs::read(root.path().join("prod/meta/engine_epoch")).unwrap(),
        b"corrupt"
    );
}
