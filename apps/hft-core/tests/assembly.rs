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
    // SDK-005 supplies only a kind id: preserve its original BTC spot/spot reference data.
    use hft_strategy_sdk::StrategySet;
    let kind = hft_core::Strategies::kinds()
        .iter()
        .find(|d| d.non_prod_only)
        .unwrap()
        .kind_id;
    let d = tempfile::tempdir().unwrap();
    support::write_for_kind(d.path(), &d.path().join("hot"), None, Some(kind));
    let bytes = std::fs::read(d.path().join("refdata.bin")).unwrap();
    let rd = hft_refdata::RefData::load(&bytes, None).unwrap();
    assert_eq!(rd.asset(hft_types::AssetId(1)).name.as_ref(), "BTC");
    assert_eq!(rd.inst(hft_types::InstId(1)).venue, VenueId::BINANCE_SPOT);
    assert_eq!(rd.inst(hft_types::InstId(2)).venue, VenueId::BITGET_UTA_SPOT);
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

#[test]
fn core_033_testnet_variants_check_production_bounds() {
    use hft_strategy_sdk::{StrategySet, params::parse_value};
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../deploy/testnet");
    let mut files: Vec<_> = std::fs::read_dir(dir)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| {
            path.file_name()
                .unwrap()
                .to_str()
                .unwrap()
                .starts_with("strategy")
                && path.extension().is_some_and(|ext| ext == "toml")
        })
        .collect();
    files.sort();
    assert!(!files.is_empty());
    for file in files {
        let params: toml::Value = toml::from_str(&std::fs::read_to_string(&file).unwrap()).unwrap();
        let kind = params["strategy"]["schema"]["id"].as_integer().unwrap();
        let Some(desc) = hft_core::Strategies::kinds()
            .iter()
            .find(|d| i64::from(d.kind_id) == kind)
        else {
            continue;
        };
        let d = tempfile::tempdir().unwrap();
        let path = support::write_strategy_testnet(d.path(), &params, false);
        let output = std::process::Command::new(env!("CARGO_BIN_EXE_hft-core"))
            .args(["--config", path.to_str().unwrap(), "--check"])
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}: {}",
            file.display(),
            String::from_utf8_lossy(&output.stderr)
        );
        let loaded = hft_core::config::load(&path, false).unwrap();
        assert!(!loaded.rd.account(AccountId(1)).is_prod);
        assert_eq!(
            loaded.rd.asset(hft_types::AssetId(1)).name.as_ref(),
            params["groups"][0]["asset"].as_str().unwrap()
        );
        assert_eq!(loaded.rd.inst(hft_types::InstId(1)).venue, VenueId::BINANCE_USDM);
        assert_eq!(
            loaded.rd.inst(hft_types::InstId(2)).venue,
            VenueId::BITGET_UTA_SPOT
        );
        let violates_prod_bound = desc.schema.fields.iter().enumerate().any(|(ix, field)| {
            field.prod_min.is_some_and(|min| {
                let value = params["defaults"].get(field.name).map_or(field.default, |v| {
                    let text = v.as_str().map(str::to_owned).unwrap_or_else(|| v.to_string());
                    parse_value(field.as_ref(ix as u16), &text, None).unwrap()
                });
                value < min
            })
        });
        if violates_prod_bound {
            let p = support::write_strategy_testnet(d.path(), &params, true);
            let errors = hft_core::config::load(&p, false).unwrap_err();
            assert!(errors.iter().any(|e| e.code == "StrategyParams"), "{errors:?}");
        }
    }
}
