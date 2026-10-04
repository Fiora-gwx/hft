//! Forbidden APIs do not compile (TYP-005); one source file per table row in `tests/ui/`.

#[test]
fn typ_005_forbidden_api_does_not_compile() {
    let t = trybuild::TestCases::new();
    for row in 1..=8 {
        let pattern = format!("tests/ui/typ_005_{row}_*.rs");
        t.compile_fail(pattern);
    }
    t.pass("tests/ui/typ_005_9_control.rs");
}
