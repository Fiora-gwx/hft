mod support;
use hft_core::budget::{check_space, hot_resident, journals};

#[test]
fn core_100_hot_resident() {
    for (size, keep, expected) in [
        (48 << 20, 1, 144 << 20),
        (4 << 20, 0, 8 << 20),
        (1 << 30, 1, 3 << 30),
        (2 << 30, 2, 8 << 30),
    ] {
        assert_eq!(hot_resident(size, keep).unwrap(), expected);
    }
    assert_eq!(hot_resident(u64::MAX, 1).unwrap_err().exit, 3);
}

#[test]
fn core_101_journal_set_follows_active_venue_tasks() {
    let dir = tempfile::tempdir().unwrap();
    let path = support::write(dir.path(), &dir.path().join("hot"), None);
    let c = hft_core::config::parse(&path).unwrap();
    assert_eq!(journals(&c, true), ["seq"]);
    let expected: Vec<_> = std::iter::once("seq".to_string())
        .chain(c.venues.keys().map(|n| format!("raw-feed-{n}")))
        .collect();
    assert_eq!(journals(&c, false), expected);
}

#[test]
fn core_165_only_self_held_bytes_credit_the_space_check() {
    assert!(check_space(100, 28, 128).is_ok());
    assert_eq!(check_space(100, 27, 128).unwrap_err().code, "ShmSpace");
    assert!(check_space(u64::MAX, 1, u64::MAX).is_ok());
}

#[test]
fn core_167_other_instance_space_is_not_available() {
    assert_eq!(check_space(0, 0, 128).unwrap_err().exit, 3);
}
