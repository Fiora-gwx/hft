//! `cargo xtask hot-check` (tools/TESTS.md TOOL-032). Fixture `clean`: common::repos::hot_clean.

mod common;

use common::Fixture;
use common::repos::{OMS_A, OMS_LIB, RT_A, X_LIB, hot_clean};

/// `(row, edit, expected exit, stderr must contain)`.
type Row = (&'static str, fn(&Fixture), i32, &'static str);

const ALLOW: &str = "tools/xtask/allow-list.toml";

fn r01(f: &Fixture) {
    let s = f.read("crates/hft-oms/clippy.toml");
    let mut lines: Vec<&str> = s.lines().collect();
    let i = lines
        .iter()
        .position(|l| l.contains("std::thread::sleep"))
        .unwrap();
    lines.remove(i);
    f.write("crates/hft-oms/clippy.toml", &(lines.join("\n") + "\n"));
}
fn r02(_: &Fixture) {}
fn r03(f: &Fixture) {
    f.replace(OMS_LIB, "    clippy::unwrap_used,\n", "");
}
fn r04(f: &Fixture) {
    f.replace(OMS_LIB, "    clippy::panic,\n", "");
    f.replace(
        OMS_LIB,
        "#![forbid(unsafe_code)]\n",
        "#![forbid(unsafe_code)]\n#![warn(clippy::panic)]\n",
    );
}
fn r05(f: &Fixture) {
    f.append(OMS_A, "\n#[allow(clippy::disallowed_methods)]\npub fn t() {}\n");
}
fn r06(f: &Fixture) {
    r05(f);
    f.append(
        ALLOW,
        "\n[[allow]]\nfile = \"crates/hft-oms/src/a.rs\"\nlints = [\"clippy::disallowed_methods\"]\nreason = \"startup only\"\n",
    );
}
fn r07(f: &Fixture) {
    f.append(OMS_A, "\n#[allow(clippy::unwrap_used)]\npub fn u() {}\n");
}
fn r08(f: &Fixture) {
    f.append(OMS_A, "\npub struct Cb {\n    pub f: Box<dyn Fn(u64)>,\n}\n");
}
fn r09(f: &Fixture) {
    f.append(
        "apps/watchtower/src/main.rs",
        "\npub struct Cb {\n    pub f: Box<dyn Fn(u64)>,\n}\n",
    );
}
fn r10(f: &Fixture) {
    f.append(OMS_A, "\npub fn px(x: f64) -> i64 {\n    x as i64\n}\n");
}
fn r11(f: &Fixture) {
    f.append(
        "strategies/x/src/model/mod.rs",
        "\npub struct W {\n    pub w: f64,\n}\n",
    );
}
fn r12(f: &Fixture) {
    f.append(X_LIB, "\npub struct Y {\n    pub w: f64,\n}\n");
}
fn r13(f: &Fixture) {
    f.append(
        X_LIB,
        "\npub trait Strategy {}\npub fn on(s: &dyn Strategy) {\n    let _ = s;\n}\n",
    );
}
fn r14(f: &Fixture) {
    f.append(
        X_LIB,
        "\npub fn g(x: i64) -> i64 {\n    hft_strategy_sdk::model_math::ln(x as _) as i64\n}\n",
    );
}
fn r15(f: &Fixture) {
    f.replace(
        "strategies/x/src/model/mod.rs",
        "pub struct M",
        "mod fit;\n\npub struct M",
    );
    f.write(
        "strategies/x/src/model/fit.rs",
        "pub fn g(x: f64) -> f64 {\n    hft_strategy_sdk::model_math::ln(x)\n}\n",
    );
}
fn r16(f: &Fixture) {
    f.append(OMS_A, "\npub fn z() {\n    unsafe {}\n}\n");
}
fn r17(f: &Fixture) {
    f.append(RT_A, "\npub fn z(p: *const u8) -> u8 {\n    unsafe { *p }\n}\n");
}
fn r18(f: &Fixture) {
    f.append(
        RT_A,
        "\npub fn z(p: *const u8) -> u8 {\n    // SAFETY: the caller passes a valid pointer.\n    unsafe { *p }\n}\n",
    );
}
fn r19(f: &Fixture) {
    f.append(OMS_A, "\n#[repr(C)]\npub struct R {\n    pub a: u64,\n}\n");
}
fn r20(f: &Fixture) {
    f.append(
        OMS_A,
        "\n#[repr(C)]\npub struct P {\n    pub a: u8,\n    pub b: u64,\n}\nconst _: () = assert!(size_of::<P>() == 16);\n",
    );
}
fn r21(f: &Fixture) {
    f.append(
        OMS_A,
        "\n#[repr(C)]\npub struct P {\n    pub a: u8,\n    pub _pad: [u8; 7],\n    pub b: u64,\n}\nconst _: () = assert!(size_of::<P>() == 16);\n",
    );
}
fn r22(f: &Fixture) {
    f.append(OMS_A, "\npub struct SlotRow {\n    pub deadline_tsc: u64,\n}\n");
}
fn r23(f: &Fixture) {
    r22(f);
    f.append(
        ALLOW,
        "\n[[tsc]]\nrow = \"SlotRow\"\nfield = \"deadline_tsc\"\nclass = 1\nreason = \"shifted by recovery_end\"\n",
    );
}
fn r24(f: &Fixture) {
    f.append(OMS_A, "\npub fn sp() {\n    std::thread::spawn(|| {});\n}\n");
}
fn r25(f: &Fixture) {
    f.append(OMS_A, "\npub struct Seg {\n    pub m: memmap2::MmapMut,\n}\n");
}
fn r26(f: &Fixture) {
    f.append(
        OMS_A,
        "\npub struct Ctr {\n    pub n: std::sync::atomic::AtomicU64,\n}\n",
    );
}
fn r27(f: &Fixture) {
    f.replace(
        "Cargo.toml",
        "codegen-units = 16\npanic = \"abort\"",
        "codegen-units = 16\npanic = \"unwind\"",
    );
}
fn r28(f: &Fixture) {
    f.write(
        ".cargo/config.toml",
        "[target.aarch64-unknown-linux-gnu]\nrustflags = []\n",
    );
}
const ENCRYPT: &str = "\nuse rustls::unbuffered::WriteTraffic;\npub fn send(w: &mut WriteTraffic<'_, ()>, b: &[u8], o: &mut [u8]) {\n    let _ = w.encrypt(b, o);\n}\n";
fn r29(f: &Fixture) {
    f.append(OMS_A, ENCRYPT);
}
fn r31(f: &Fixture) {
    f.append(
        "apps/watchtower/src/main.rs",
        "\n#[global_allocator]\nstatic A: mimalloc::MiMalloc = mimalloc::MiMalloc;\n",
    );
}
fn r32(f: &Fixture) {
    r31(f);
    f.append(
        "apps/watchtower/src/main.rs",
        "\nfn init_alloc() {\n    // SAFETY: called once before any allocation.\n    unsafe { libmimalloc_sys::mi_option_set(libmimalloc_sys::mi_option_purge_delay, -1) };\n}\n",
    );
}

const ROWS: &[Row] = &[
    ("TOOL-032.1", r01, 1, "crates/hft-oms/clippy.toml"),
    ("TOOL-032.2", r02, 0, ""),
    ("TOOL-032.3", r03, 1, "[C1] crates/hft-oms/src/lib.rs"),
    ("TOOL-032.4", r04, 1, "`clippy::panic` weakened"),
    (
        "TOOL-032.5",
        r05,
        1,
        "allow(clippy::disallowed_methods)] is not registered",
    ),
    ("TOOL-032.6", r06, 0, ""),
    (
        "TOOL-032.7",
        r07,
        1,
        "allow(clippy::unwrap_used)] is not registered",
    ),
    ("TOOL-032.8", r08, 1, "[N6] crates/hft-oms/src/a.rs"),
    ("TOOL-032.9", r09, 0, ""),
    ("TOOL-032.10", r10, 1, "[N3] crates/hft-oms/src/a.rs"),
    ("TOOL-032.11", r11, 0, ""),
    ("TOOL-032.12", r12, 1, "[S1] strategies/x/src/lib.rs"),
    ("TOOL-032.13", r13, 1, "[S1] strategies/x/src/lib.rs"),
    ("TOOL-032.14", r14, 1, "[N3] strategies/x/src/lib.rs"),
    ("TOOL-032.15", r15, 0, ""),
    ("TOOL-032.16", r16, 1, "[B5] crates/hft-oms/src/a.rs"),
    ("TOOL-032.17", r17, 1, "without a `// SAFETY:` comment"),
    ("TOOL-032.18", r18, 0, ""),
    ("TOOL-032.19", r19, 1, "[M4] crates/hft-oms/src/a.rs"),
    ("TOOL-032.20", r20, 1, "implicit padding (7 B between fields"),
    ("TOOL-032.21", r21, 0, ""),
    ("TOOL-032.22", r22, 1, "[D6] crates/hft-oms/src/a.rs"),
    ("TOOL-032.23", r23, 0, ""),
    ("TOOL-032.24", r24, 1, "[T1] crates/hft-oms/src/a.rs"),
    ("TOOL-032.25", r25, 1, "[M3] crates/hft-oms/src/a.rs"),
    ("TOOL-032.26", r26, 1, "[T3] crates/hft-oms/src/a.rs"),
    ("TOOL-032.27", r27, 1, "[B1] Cargo.toml"),
    ("TOOL-032.28", r28, 1, "[B2] .cargo/config.toml"),
    ("TOOL-032.29", r29, 1, "[T6] crates/hft-oms/src/a.rs"),
    ("TOOL-032.31", r31, 1, "[M1] apps/watchtower/src/main.rs"),
    ("TOOL-032.32", r32, 0, ""),
];

#[test]
fn tool_032_check_counterexamples() {
    for (row, edit, code, needle) in ROWS {
        let f = hot_clean(&format!("hot-{}", row.replace('.', "-")));
        edit(&f);
        let r = f.xtask(&["hot-check"]);
        assert_eq!(r.code, *code, "{row}: stderr:\n{}", r.stderr);
        assert!(
            r.stderr.contains(needle),
            "{row}: expected {needle:?} in:\n{}",
            r.stderr
        );
        let n = r.json.as_ref().unwrap()["violations"].as_array().unwrap().len();
        assert_eq!(n == 0, *code == 0, "{row}: JSON violations {n}");
    }
    // TOOL-032.30: the exported T6 scan accepts the call site in crates/hft-net/src/tls.rs.
    let f = hot_clean("hot-tool-032-30");
    f.write(
        "crates/hft-net/Cargo.toml",
        "[package]\nname = \"hft-net\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    );
    f.write("crates/hft-net/src/tls.rs", ENCRYPT);
    assert!(
        xtask::hot_check::scan_tls_encrypt(&f.root).is_empty(),
        "TOOL-032.30"
    );
    r29(&f);
    let vs = xtask::hot_check::scan_tls_encrypt(&f.root);
    assert_eq!(vs.len(), 1, "TOOL-032.30: {vs:?}");
    assert_eq!(vs[0].file, OMS_A, "TOOL-032.30");
}
