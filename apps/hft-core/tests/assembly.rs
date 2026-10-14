mod support;
use hft_types::{AccountId, VenueId};
fn live(prod: bool) -> (tempfile::TempDir, std::path::PathBuf) {
    let d = tempfile::tempdir().unwrap();
    let path = support::write_live(d.path(), &d.path().join("hot"), prod);
    (d, path)
}
#[test]
fn core_032_probe_on_prod_account() {
    let (_d, path) = live(true);
    let errors = hft_core::config::load(&path, false).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|e| e.code == "ProbeOnProdAccount" && e.exit == 2 && e.detail.contains('1')),
        "{errors:?}"
    );
}
#[test]
fn core_033_probe_on_testnet_ok() {
    let (_d, path) = live(false);
    let loaded = hft_core::config::load(&path, false).unwrap();
    assert!(loaded.setup.role_prod);
    assert!(!loaded.rd.account(AccountId(1)).is_prod);
    assert_eq!(loaded.configs.params.len(), 1);
}
#[test]
fn core_043_credential_account_matches_refdata() {
    let (_d, path) = live(false);
    let base: toml::Value = toml::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    for id in [0, 9] {
        let mut cfg = base.clone();
        cfg["venues"]["public"]["accounts"][0]["account"] = id.into();
        std::fs::write(&path, toml::to_string(&cfg).unwrap()).unwrap();
        let errors = hft_core::config::load(&path, false).unwrap_err();
        assert!(errors.iter().any(|e| e.code == "AccountMismatch"), "{errors:?}");
    }
}
#[test]
fn core_044_both_keys_required_and_sim_rejects_credentials() {
    let (_d, path) = live(false);
    let base: toml::Value = toml::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    for field in [
        "api_key_path",
        "session_seed_path",
        "rest_api_key_path",
        "rest_secret_path",
    ] {
        let mut cfg = base.clone();
        cfg["venues"]["public"]["accounts"][0][field] = "".into();
        std::fs::write(&path, toml::to_string(&cfg).unwrap()).unwrap();
        let errors = hft_core::config::load(&path, false).unwrap_err();
        assert!(
            errors.iter().any(|e| e.code == "MissingCredential"),
            "{field}: {errors:?}"
        );
    }
    let d = tempfile::tempdir().unwrap();
    let path = support::write(d.path(), &d.path().join("hot"), None);
    let mut cfg: toml::Value = toml::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    cfg["venues"]["public"]
        .as_table_mut()
        .unwrap()
        .insert("accounts".into(), base["venues"]["public"]["accounts"].clone());
    std::fs::write(&path, toml::to_string(&cfg).unwrap()).unwrap();
    assert!(
        hft_core::config::load(&path, true)
            .unwrap_err()
            .iter()
            .any(|e| e.code == "SecretsInSim")
    );
}
#[test]
fn core_420_sequencer_handoff() {
    let (_d, path) = live(false);
    let loaded = hft_core::config::load(&path, false).unwrap();
    assert!(loaded.setup.role_prod);
    assert_eq!(
        loaded.setup.keys_required,
        [(AccountId(1), VenueId::BINANCE_SPOT, 3)]
    );
    for standby in [0, 1] {
        let mut setup = loaded.setup.clone();
        setup.seq.allow_missing_standby = standby;
        let bytes = setup.encode_blob(1, hft_types::HostClass::Dev2, 0);
        let mut cfg: toml::Value = toml::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        std::fs::write(path.parent().unwrap().join("engine.bin"), &bytes).unwrap();
        cfg["blobs"]["engine"]["sha256"] = hft_core::config::hex(&hft_core::config::digest(&bytes)).into();
        std::fs::write(&path, toml::to_string(&cfg).unwrap()).unwrap();
        assert_eq!(
            hft_core::config::load(&path, false)
                .unwrap()
                .setup
                .seq
                .allow_missing_standby,
            standby
        );
    }
    let d = tempfile::tempdir().unwrap();
    let path = support::write(d.path(), &d.path().join("hot"), None);
    let loaded = hft_core::config::load(&path, true).unwrap();
    assert!(!loaded.setup.role_prod);
    assert!(loaded.setup.keys_required.is_empty());
}
