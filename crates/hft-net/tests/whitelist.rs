//! NET-164: rustls record encryption is called only from `src/tls.rs` — the scan `cargo xtask
//! hot-check` (T6) runs, on the real tree and on a copy with one extra call in `src/conn.rs`.
#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods
)] // allow-list: test code, not a hot path

use std::path::{Path, PathBuf};

fn copy_dir(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for e in std::fs::read_dir(from).unwrap() {
        let p = e.unwrap().path();
        let dst = to.join(p.file_name().unwrap());
        if p.is_dir() {
            copy_dir(&p, &dst);
        } else {
            std::fs::copy(&p, &dst).unwrap();
        }
    }
}

#[test]
fn net_164_encrypt_call_whitelist() {
    let krate = Path::new(env!("CARGO_MANIFEST_DIR"));
    let root = krate.parent().unwrap().parent().unwrap();
    // Step 1: the real tree.
    let v = xtask::hot_check::scan_tls_encrypt(root);
    assert!(v.is_empty(), "{v:?}");
    // Step 2: a copy with one more `WriteTraffic::encrypt` call in src/conn.rs.
    let tmp: PathBuf = std::env::temp_dir().join(format!("hft-net-164-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp);
    let dst = tmp.join("crates").join("hft-net");
    std::fs::create_dir_all(&dst).unwrap();
    std::fs::copy(krate.join("Cargo.toml"), dst.join("Cargo.toml")).unwrap();
    copy_dir(&krate.join("src"), &dst.join("src"));
    let conn_rs = dst.join("src").join("conn.rs");
    let mut s = std::fs::read_to_string(&conn_rs).unwrap();
    let line = s.lines().count() + 2;
    s.push_str("\nfn smuggled(w: &mut rustls::unbuffered::WriteTraffic<'_, ()>, o: &mut [u8]) { let _ = rustls::unbuffered::WriteTraffic::encrypt(w, b\"x\", o); }\n");
    std::fs::write(&conn_rs, s).unwrap();
    let v = xtask::hot_check::scan_tls_encrypt(&tmp);
    let _ = std::fs::remove_dir_all(&tmp);
    assert_eq!(v.len(), 1, "{v:?}");
    assert_eq!(v[0].file, "crates/hft-net/src/conn.rs");
    assert_eq!(v[0].line, line);
    assert_eq!(v[0].rule, "T6");
}
