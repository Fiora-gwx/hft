//! LED-005: public entries take engine time (`i64`), never `Tsc`.

#[test]
fn led_005_entries_take_engine_ns_not_tsc() {
    let t = trybuild::TestCases::new();
    t.compile_fail("tests/ui/led_005_*.rs");
    // No public signature mentions `Tsc` (the crate's public items are re-exported from lib.rs).
    let src = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/src/lib.rs")).expect("lib.rs");
    assert!(!src.contains("Tsc"), "lib.rs re-exports a Tsc item");
    for f in std::fs::read_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/src")).expect("src") {
        let p = f.expect("entry").path();
        let s = std::fs::read_to_string(&p).expect("read");
        for line in s
            .lines()
            .filter(|l| l.trim_start().starts_with("pub fn") || l.trim_start().starts_with("pub "))
        {
            assert!(
                !line.contains("Tsc"),
                "{}: public item uses Tsc: {line}",
                p.display()
            );
        }
    }
}
