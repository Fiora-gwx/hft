//! DurableMeta (JNL-094 – JNL-097). Needs no writer feature.

use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};

use hft_journal::{DurableMeta, InstanceName};

static N: AtomicU32 = AtomicU32::new(0);

fn root() -> PathBuf {
    let n = N.fetch_add(1, Ordering::Relaxed);
    let d = std::env::temp_dir().join(format!("hftj-meta-{}-{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn content(v: &str) -> String {
    format!("{v}\ncrc32c:{:08x}\n", crc32c::crc32c(v.as_bytes()))
}

#[test]
fn jnl_094_durable_meta_rw() {
    let r = root();
    let m = DurableMeta::open(&r, &InstanceName::Prod).unwrap();
    let dir = r.join("prod").join("meta");
    assert_eq!(m.read("engine_epoch").unwrap(), None, "JNL-094.1");
    m.write_sync("engine_epoch", 0).unwrap();
    assert_eq!(m.read("engine_epoch").unwrap(), Some(0), "JNL-094.2");
    assert_eq!(
        std::fs::read_to_string(dir.join("engine_epoch")).unwrap(),
        content("0"),
        "JNL-094.2"
    );
    m.write_sync("engine_epoch", u64::MAX).unwrap();
    assert_eq!(
        m.read("engine_epoch").unwrap(),
        Some(18_446_744_073_709_551_615),
        "JNL-094.3"
    );
    m.write_sync("engine_epoch", 5).unwrap();
    m.write_sync("engine_epoch", 6).unwrap();
    assert_eq!(m.read("engine_epoch").unwrap(), Some(6), "JNL-094.4");
    let tmps = std::fs::read_dir(&dir)
        .unwrap()
        .filter(|e| {
            e.as_ref()
                .unwrap()
                .file_name()
                .to_string_lossy()
                .ends_with(".tmp")
        })
        .count();
    assert_eq!(tmps, 0, "JNL-094.4");
    let s = DurableMeta::open(&r, &InstanceName::Sim).unwrap();
    assert_eq!(s.read("engine_epoch").unwrap(), None, "JNL-094.5");
    assert!(dir.is_dir(), "JNL-094.6");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(&dir).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o770, "JNL-094.6");
    }
}

#[test]
fn jnl_095_durable_meta_crash() {
    for (row, tmp, key, want) in [
        ("JNL-095.1", Some(content("6")), Some(content("5")), Some(5)),
        (
            "JNL-095.2",
            Some("6\ncrc".to_string()),
            Some(content("5")),
            Some(5),
        ),
        ("JNL-095.3", None, Some(content("6")), Some(6)),
        ("JNL-095.4", Some(content("6")), None, None),
    ] {
        let r = root();
        let m = DurableMeta::open(&r, &InstanceName::Prod).unwrap();
        let dir = r.join("prod").join("meta");
        if let Some(t) = tmp {
            std::fs::write(dir.join("engine_epoch.tmp"), t).unwrap();
        }
        if let Some(k) = key {
            std::fs::write(dir.join("engine_epoch"), k).unwrap();
        }
        assert_eq!(m.read("engine_epoch").unwrap(), want, "{row}");
    }
}

#[test]
fn jnl_096_durable_meta_corrupt() {
    for (row, c) in [
        ("JNL-096.1", "42\ncrc32c:00000000\n".to_string()),
        ("JNL-096.2", content("4x")),
        ("JNL-096.3", content("042")),
        ("JNL-096.4", "42\n".to_string()),
        ("JNL-096.5", content("18446744073709551616")),
        ("JNL-096.6", String::new()),
    ] {
        let r = root();
        let m = DurableMeta::open(&r, &InstanceName::Prod).unwrap();
        std::fs::write(r.join("prod").join("meta").join("engine_epoch"), c).unwrap();
        assert!(m.read("engine_epoch").is_err(), "{row}");
    }
}

#[test]
fn jnl_097_durable_meta_keys() {
    let r = root();
    let m = DurableMeta::open(&r, &InstanceName::Prod).unwrap();
    assert!(m.write_sync("engine_epoch", 1).is_ok(), "JNL-097.1");
    assert!(m.write_sync(&"a".repeat(64), 1).is_ok(), "JNL-097.2");
    for (row, k) in [
        ("JNL-097.3", "a".repeat(65)),
        ("JNL-097.4", String::new()),
        ("JNL-097.5", "Engine".to_string()),
        ("JNL-097.6", "a/b".to_string()),
        ("JNL-097.6", "a.tmp".to_string()),
    ] {
        let e = m.write_sync(&k, 1).unwrap_err();
        assert_eq!(e.kind(), std::io::ErrorKind::InvalidInput, "{row}");
    }
}
