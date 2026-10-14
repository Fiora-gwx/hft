#[test]
fn reg_011_cli_writes_export() {
    let dir = tempfile::tempdir().unwrap();
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_hft-core"))
        .arg("schemas")
        .arg("--out")
        .arg(dir.path())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let expected = hft_strategy_registry::export_schemas();
    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), expected.len());
    for (name, body) in expected {
        assert_eq!(std::fs::read(dir.path().join(name)).unwrap(), body.as_bytes());
    }
}
