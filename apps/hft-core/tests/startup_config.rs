mod support;

fn fixture() -> (tempfile::TempDir, std::path::PathBuf, String) {
    let d = tempfile::tempdir().unwrap();
    let path = support::write(d.path(), &d.path().join("hot"), None);
    let text = std::fs::read_to_string(&path).unwrap();
    (d, path, text)
}

#[test]
fn core_001_exit_codes_preserve_drain_cause() {
    use hft_sequencer::DoneCause;
    for (cause, code) in [
        (DoneCause::Drain, 0),
        (DoneCause::ReplaceRefdata, 75),
        (DoneCause::StorageCritical, 3),
        (DoneCause::IdentityMismatch, 2),
    ] {
        assert_eq!(hft_core::exit_code(cause), code);
    }
}

#[test]
fn core_005_invalid_cli_is_usage_exit_two() {
    for args in [
        vec![],
        vec!["--unknown"],
        vec!["--config"],
        vec!["--run-for-ms", "0"],
        vec!["--config", "missing", "--check", "--check"],
        vec!["schemas", "--out"],
    ] {
        let out = std::process::Command::new(env!("CARGO_BIN_EXE_hft-core"))
            .args(args)
            .output()
            .unwrap();
        assert_eq!(out.status.code(), Some(2));
        let stderr = String::from_utf8(out.stderr).unwrap();
        assert_eq!(stderr.lines().count(), 1);
        assert!(stderr.contains("error[Usage]"));
    }
}

#[test]
fn core_008_unknown_field_is_one_redacted_parse_error() {
    let (_d, path, text) = fixture();
    std::fs::write(
        &path,
        text.replace("schema = 1", "schema = 1\ninline_secret = 'SENTINEL_SECRET'"),
    )
    .unwrap();
    let errors = hft_core::config::load(&path, true).unwrap_err();
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].code, "ConfigParse");
    assert!(!format!("{errors:?}").contains("SENTINEL_SECRET"));
}

#[test]
fn core_011_check_has_no_journal_or_socket_side_effects() {
    let (d, path, _) = fixture();
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_hft-core"))
        .args(["--config", path.to_str().unwrap(), "--dry-run", "--check"])
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    assert!(out.stderr.is_empty());
    for p in ["hot", "cold", "core.sock"] {
        assert!(!d.path().join(p).exists());
    }
}

#[test]
fn core_312_shutdown_cancel_has_no_default() {
    let (_d, path, text) = fixture();
    std::fs::write(&path, text.replace("cancel_open_orders = true\n", "")).unwrap();
    let errors = hft_core::config::load(&path, true).unwrap_err();
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].code, "MissingField");
    assert_eq!(errors[0].detail, "shutdown.cancel_open_orders");
}

#[test]
fn core_360_missing_fields_collected_before_files_are_read() {
    let (_d, path, text) = fixture();
    let text = text
        .replace("cancel_open_orders = true\n", "")
        .replace("drain_timeout_ms = 3000\n", "")
        .replace("interval_ms = 1000\n", "");
    std::fs::write(&path, text).unwrap();
    let errors = hft_core::config::load(&path, true).unwrap_err();
    let mut fields: Vec<_> = errors
        .iter()
        .map(|e| {
            assert_eq!(e.code, "MissingField");
            e.detail.as_str()
        })
        .collect();
    fields.sort();
    assert_eq!(
        fields,
        [
            "clock.interval_ms",
            "shutdown.cancel_open_orders",
            "shutdown.drain_timeout_ms"
        ]
    );
}

#[test]
fn core_060_thread_names_fit_linux_comm_and_are_unique() {
    let (_d, path, text) = fixture();
    // The kernel truncates comm at 15 bytes, making heartbeat attribution ambiguous.
    let mut cfg: toml::Value = toml::from_str(&text).unwrap();
    cfg["threads"][0]["name"] = "abcdefghijklmnop".into();
    std::fs::write(&path, toml::to_string(&cfg).unwrap()).unwrap();
    assert!(
        hft_core::config::load(&path, true)
            .unwrap_err()
            .iter()
            .any(|e| e.code == "ThreadPlan")
    );
}

#[test]
fn core_067_park_interval_is_strictly_below_heartbeat_bound() {
    let (_d, path, text) = fixture();
    let mut cfg: toml::Value = toml::from_str(&text).unwrap();
    cfg["watchdog"]["hb_stale_ms"] = 4.into();
    cfg["threads"][0]["park_us"] = 1000.into();
    std::fs::write(&path, toml::to_string(&cfg).unwrap()).unwrap();
    assert!(
        hft_core::config::load(&path, true)
            .unwrap_err()
            .iter()
            .any(|e| e.code == "ThreadPlan")
    );
    for row in cfg["threads"].as_array_mut().unwrap() {
        row["park_us"] = 999.into();
    }
    std::fs::write(&path, toml::to_string(&cfg).unwrap()).unwrap();
    hft_core::config::load(&path, true).unwrap();
}

#[test]
fn core_114_subscription_capacity_is_checked_before_startup() {
    let (_d, path, text) = fixture();
    let mut cfg: toml::Value = toml::from_str(&text).unwrap();
    cfg["venues"]["public"]["max_sub_insts"] = 1024.into();
    std::fs::write(&path, toml::to_string(&cfg).unwrap()).unwrap();
    let errors = hft_core::config::load(&path, true).unwrap_err();
    for code in ["SubCapacity", "FeedCtlCapacity"] {
        assert!(errors.iter().any(|e| e.code == code), "{errors:?}");
    }
}

#[test]
fn core_031_duplicate_instance_parameters_fail_binding() {
    let (_d, path, text) = fixture();
    let mut cfg: toml::Value = toml::from_str(&text).unwrap();
    let rows = cfg["blobs"]["strategy_params"].as_array_mut().unwrap();
    rows.push(rows[0].clone());
    std::fs::write(&path, toml::to_string(&cfg).unwrap()).unwrap();
    assert!(
        hft_core::config::load(&path, true)
            .unwrap_err()
            .iter()
            .any(|e| e.code == "BindingMismatch")
    );
}

#[test]
fn core_030_unknown_strategy_reports_kind() {
    let (d, path, text) = fixture();
    let mut cfg: toml::Value = toml::from_str(&text).unwrap();
    let blob_path = d
        .path()
        .join(cfg["blobs"]["strategy_params"][0]["path"].as_str().unwrap());
    let mut bytes = std::fs::read(&blob_path).unwrap();
    bytes[2..4].copy_from_slice(&999u16.to_le_bytes());
    std::fs::write(&blob_path, &bytes).unwrap();
    cfg["blobs"]["strategy_params"][0]["sha256"] =
        hft_core::config::hex(&hft_core::config::digest(&bytes)).into();
    std::fs::write(&path, toml::to_string(&cfg).unwrap()).unwrap();
    let errors = hft_core::config::load(&path, true).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|e| e.code == "UnknownStrategyKind" && e.detail.contains("999")),
        "{errors:?}"
    );
}

fn replace_engine(path: &std::path::Path, text: &str, setup: &hft_sequencer::EngineSetup) {
    let mut cfg: toml::Value = toml::from_str(text).unwrap();
    let bytes = setup.encode_blob(1, hft_types::HostClass::Dev2, 0);
    std::fs::write(
        path.parent()
            .unwrap()
            .join(cfg["blobs"]["engine"]["path"].as_str().unwrap()),
        &bytes,
    )
    .unwrap();
    cfg["blobs"]["engine"]["sha256"] = hft_core::config::hex(&hft_core::config::digest(&bytes)).into();
    std::fs::write(path, toml::to_string(&cfg).unwrap()).unwrap();
}

#[test]
fn core_115_exec_risk_mismatch() {
    let (_d, path, text) = fixture();
    let base = hft_core::config::load(&path, true).unwrap().setup;
    for (cross, chunk, accepted) in [
        (10_000_000, 100_000_000, true),
        (10_000_001, 100_000_000, false),
        (10_000_000, 100_000_001, false),
    ] {
        let mut setup = base.clone();
        setup.exec.default.force_slice.max_cross = hft_types::Rate(cross);
        setup.exec.default.force_slice.max_chunk_notional = hft_types::Notional(chunk);
        replace_engine(&path, &text, &setup);
        let result = hft_core::config::load(&path, true);
        if accepted {
            result.unwrap();
        } else {
            let errors = result.unwrap_err();
            assert_eq!(errors.len(), 1);
            assert_eq!(errors[0].code, "ExecRiskMismatch");
        }
    }
}

#[test]
fn core_116_dust_vs_min_notional() {
    let (_d, path, text) = fixture();
    let base = hft_core::config::load(&path, true).unwrap().setup;
    for (dust, accepted) in [(4_999_999, false), (5_000_000, true), (5_000_001, true)] {
        let mut setup = base.clone();
        setup.exec.default.dust_notional = hft_types::Notional(dust);
        replace_engine(&path, &text, &setup);
        let result = hft_core::config::load(&path, true);
        if accepted {
            result.unwrap();
        } else {
            let errors = result.unwrap_err();
            assert_eq!(errors.len(), 1);
            assert_eq!(errors[0].code, "DustBelowMinNotional");
        }
    }
}

#[test]
fn core_014_missing_rest_key_reference_is_a_credential_error() {
    let (_d, path, text) = fixture();
    let mut cfg: toml::Value = toml::from_str(&text).unwrap();
    let accounts = toml::from_str::<toml::Value>(
        "[[rows]]\naccount = 1\napi_key_path = 'session-key'\nsession_seed_path = 'seed'\nrest_secret_path = 'rest-secret'"
    ).unwrap()["rows"].clone();
    cfg["venues"]["public"]
        .as_table_mut()
        .unwrap()
        .insert("accounts".into(), accounts);
    std::fs::write(&path, toml::to_string(&cfg).unwrap()).unwrap();
    let errors = hft_core::config::load(&path, false).unwrap_err();
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].code, "MissingCredential");
    assert!(errors[0].detail.ends_with("rest_api_key_path"));
}
