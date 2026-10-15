use hft_core::{config::Config, secrets::SecretFiles};
use std::path::PathBuf;

fn fixture() -> (tempfile::TempDir, Config, Vec<PathBuf>) {
    let d = tempfile::tempdir().unwrap();
    let mut c: Config = toml::from_str(include_str!("../config/binance-testnet.toml")).unwrap();
    let paths: Vec<_> = ["api", "seed", "passphrase"]
        .iter()
        .map(|n| d.path().join(n))
        .collect();
    for (p, bytes) in paths
        .iter()
        .zip([b"test-api".as_slice(), &[0xab; 32], b"test-pass"])
    {
        std::fs::write(p, bytes).unwrap();
    }
    let keys = &mut c.venues.values_mut().next().unwrap().accounts[0];
    keys.api_key_path = paths[0].clone();
    keys.rest_api_key_path = paths[0].clone();
    keys.session_seed_path = paths[1].clone();
    keys.rest_secret_path = paths[1].clone();
    keys.passphrase_path = Some(paths[2].clone());
    (d, c, paths)
}

#[test]
fn core_222_unlink_after_read() {
    let (_d, c, paths) = fixture();
    let secrets = SecretFiles::read_and_unlink(&c).unwrap();
    assert!(paths.iter().all(|p| !p.exists()));
    assert_eq!(secrets.get(&paths[1]).unwrap(), &[0xab; 32]);
    let session = hft_core::venues::credential(
        &paths[0],
        &paths[1],
        Some(&paths[2]),
        hft_venue_proto::SignAlg::Ed25519,
        &secrets,
    )
    .unwrap();
    assert_eq!(session.secret.0.as_slice(), &[0xab; 32]);
    assert!(!format!("{secrets:?} {session:?}").contains("test-api"));
}

fn cli_fixture() -> (tempfile::TempDir, PathBuf, Vec<PathBuf>) {
    let (d, c, paths) = fixture();
    let mut config: toml::Value = toml::from_str(include_str!("../config/binance-testnet.toml")).unwrap();
    let (name, venue) = c.venues.iter().next().unwrap();
    let target = &mut config["venues"][name]["accounts"][0];
    let keys = &venue.accounts[0];
    for (key, path) in [
        ("api_key_path", &keys.api_key_path),
        ("session_seed_path", &keys.session_seed_path),
        ("rest_api_key_path", &keys.rest_api_key_path),
        ("rest_secret_path", &keys.rest_secret_path),
    ] {
        target[key] = path.to_str().unwrap().into();
    }
    // Do not supply the unrelated passphrase to this protocol.
    let path = d.path().join("core.toml");
    std::fs::write(&path, toml::to_string(&config).unwrap()).unwrap();
    (d, path, paths[..2].to_vec())
}

#[test]
fn core_223_unlink_before_step2_exit() {
    let (_d, path, paths) = cli_fixture();
    // Blob validation fails, after the startup-only secret transfer.
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_hft-core"))
        .args(["--config", path.to_str().unwrap()])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2));
    assert!(paths.iter().all(|p| !p.exists()));
    assert!(!String::from_utf8_lossy(&out.stderr).contains("test-api"));
}

#[test]
fn core_013_check_keeps_secret_files() {
    let (_d, path, paths) = cli_fixture();
    let before: Vec<_> = paths.iter().map(|p| std::fs::read(p).unwrap()).collect();
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_hft-core"))
        .args(["--config", path.to_str().unwrap(), "--check"])
        .output()
        .unwrap();
    // Even a failed --check must never consume credentials.
    assert_eq!(out.status.code(), Some(2));
    for (path, bytes) in paths.iter().zip(before) {
        assert_eq!(std::fs::read(path).unwrap(), bytes);
    }
}

#[test]
fn core_222_local_demo_mode_rejects_production_before_secret_read() {
    let (_d, path, paths) = cli_fixture();
    let original = std::fs::read_to_string(&path).unwrap();
    let mut config: toml::Value = toml::from_str(&original).unwrap();
    config["instance"]
        .as_table_mut()
        .unwrap()
        .insert("secrets_mode".into(), "local_demo".into());
    std::fs::write(&path, toml::to_string(&config).unwrap()).unwrap();
    assert_eq!(
        hft_core::config::parse(&path).unwrap().instance.secrets_mode,
        hft_core::config::SecretsMode::LocalDemo
    );
    for env in ["prod", "live", ""] {
        config["instance"]["order_env"] = env.into();
        std::fs::write(&path, toml::to_string(&config).unwrap()).unwrap();
        let out = std::process::Command::new(env!("CARGO_BIN_EXE_hft-core"))
            .args(["--config", path.to_str().unwrap()])
            .output()
            .unwrap();
        assert_eq!(out.status.code(), Some(2));
        assert!(String::from_utf8_lossy(&out.stderr).contains("LocalSecretEnvironment"));
        assert!(paths.iter().all(|p| p.exists()));
    }
}

#[test]
fn core_222_local_demo_rejects_production_host_under_testnet_label() {
    use hft_core::venues::local_demo_endpoint as allowed;
    use hft_refdata::EndpointRole as R;
    for (protocol, role, good, bad) in [
        (
            "binance-usdm",
            R::Rest,
            "https://demo-fapi.binance.com",
            "https://fapi.binance.com",
        ),
        (
            "binance-usdm",
            R::OrderWs,
            "wss://testnet.binancefuture.com/ws-fapi/v1",
            "wss://ws-fapi.binance.com/ws-fapi/v1",
        ),
        (
            "binance-usdm",
            R::MdPublic,
            "wss://demo-fstream.binance.com/public/ws",
            "wss://fstream.binance.com/public/ws",
        ),
        (
            "bitget-usdt-futures",
            R::Rest,
            "https://api.bitget.com",
            "https://api.bitget.com.evil.invalid",
        ),
        (
            "bitget-usdt-futures",
            R::OrderWs,
            "wss://wspap.bitget.com/v3/ws/private",
            "wss://ws.bitget.com/v3/ws/private",
        ),
    ] {
        assert!(allowed(protocol, role, good));
        assert!(!allowed(protocol, role, bad));
        assert!(!allowed(protocol, role, &good.replace("://", "://user@")));
        assert!(!allowed(
            protocol,
            role,
            &good.replace("://", "://evil.invalid?host=")
        ));
    }
}

#[test]
fn test_malformed_secret_removes_files_and_redacts_errors() {
    let (_d, c, paths) = fixture();
    std::fs::write(&paths[1], [0xab; 4097]).unwrap();
    let errors = SecretFiles::read_and_unlink(&c).unwrap_err();
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].code, "CredentialRead");
    assert!(paths.iter().all(|p| !p.exists()));
    assert!(!format!("{errors:?}").contains("test-api"));
}
