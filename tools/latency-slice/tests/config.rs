//! SLC-002–SLC-006: configuration, the order-host rule and the key sources.

mod support;

use std::path::PathBuf;

use base64::Engine;
use latency_slice::config::{self, CfgErr, Idle, Product, QuoteSide, RawOpt, load_keys};
use support::{CFG0, ED1_SEED, cfg_with};

#[test]
fn slc_002_config_defaults() {
    let c = cfg_with("");
    assert_eq!(
        (c.minutes, c.out_dir.clone(), c.epoch),
        (60, PathBuf::from("./slice-out"), 0),
        "SLC-002.1"
    );
    assert_eq!(
        (c.feed_host.as_str(), c.feed_port, c.bbo_conns),
        ("stream.binance.com", 9443, 2),
        "SLC-002.2"
    );
    assert_eq!(
        (c.orders_enabled, c.orders_host.as_str(), c.orders_port),
        (true, "demo-ws-api.binance.com", 443),
        "SLC-002.3"
    );
    assert_eq!(
        (c.api_key_env.as_str(), c.secret_env.as_deref()),
        ("HFT_SLICE_API_KEY", Some("HFT_SLICE_ED25519")),
        "SLC-002.4"
    );
    assert_eq!(
        (c.max_open_orders, c.max_order_notional.raw(-6), c.max_orders),
        (2, Some(50_000_000), 20_000),
        "SLC-002.5"
    );
    assert_eq!(
        (c.probe_period_ms, c.burst_period_ms, c.far_bp, c.quote_side),
        (1000, 0, 300, QuoteSide::Buy),
        "SLC-002.6"
    );
    assert_eq!(
        (
            c.max_spread_bp,
            c.size_margin_pct,
            c.quote_rest_ms,
            c.max_consec_fail
        ),
        (100, 10, 2000, 5),
        "SLC-002.7"
    );
    assert_eq!(
        (c.ballast_conns, c.hot_dir.clone(), c.segment_mib, c.raw),
        (0, PathBuf::from("/dev/shm/hft-slice"), 64, RawOpt::On),
        "SLC-002.8"
    );
    assert_eq!(
        (c.ts_expect_hw, c.burst_gap_ns, c.cal_max_err_ns),
        (false, 20_000, 1_000),
        "SLC-002.9"
    );
    let t = c.threads;
    let cores: Vec<Option<u16>> = [t.hk, t.persist, t.warm, t.seq, t.feed_bbo, t.feed_md, t.gw]
        .iter()
        .map(|x| x.core)
        .collect();
    assert_eq!(
        cores,
        [Some(0), Some(2), Some(3), Some(4), Some(5), Some(7), Some(8)],
        "SLC-002.10"
    );
    assert_eq!((t.seq.idle, t.hk.idle), (Idle::Spin, Idle::Park), "SLC-002.10");
    let u = config::parse(
        &CFG0
            .replace("product = \"spot\"", "product = \"usdm\"")
            .replace("[net]", "[orders]\nhost = \"testnet.binancefuture.com\"\n[net]"),
    )
    .unwrap();
    assert_eq!(
        (u.product, u.feed_host.as_str(), u.feed_port),
        (Product::Usdm, "fstream.binance.com", 443),
        "SLC-002.11"
    );
}

fn err(text: &str) -> CfgErr {
    config::parse(text).unwrap_err()
}

#[test]
fn slc_003_config_errors() {
    let e = err(&CFG0.replace("[slice]", "[slice]\ncolour = 1"));
    assert!(
        matches!(&e, CfgErr::Invalid(m) if m.contains("colour")),
        "SLC-003.1: {e:?}"
    );
    assert!(
        matches!(err(&CFG0.replace("product = \"spot\"", "")), CfgErr::Invalid(_)),
        "SLC-003.2"
    );
    assert!(
        matches!(
            err(&CFG0.replace("product = \"spot\"", "product = \"okx\"")),
            CfgErr::Invalid(_)
        ),
        "SLC-003.3"
    );
    let no_insts = CFG0.split("[[insts]]").next().unwrap().to_string() + "[net]\nsrc_ip = \"10.0.0.5\"\n";
    assert!(matches!(err(&no_insts), CfgErr::Invalid(_)), "SLC-003.4");
    assert!(
        matches!(
            err(&format!("{CFG0}\n[probe]\nburst_period_ms = 1001\n")),
            CfgErr::Invalid(_)
        ),
        "SLC-003.5"
    );
    assert!(
        config::parse(&format!("{CFG0}\n[probe]\nburst_period_ms = 1000\n")).is_ok(),
        "SLC-003.6"
    );
    assert!(
        matches!(
            err(&format!("{CFG0}\n[probe]\nquote_side = \"sell\"\n")),
            CfgErr::Invalid(_)
        ),
        "SLC-003.7"
    );
    for (v, ok) in [(49, false), (50, true), (1000, true), (1001, false)] {
        let r = config::parse(&format!("{CFG0}\n[probe]\nfar_bp = {v}\n"));
        assert_eq!(r.is_ok(), ok, "SLC-003.8 far_bp = {v}");
    }
    assert!(
        matches!(
            err(&CFG0.replace("tick = \"0.01\"", "tick = \"abc\"")),
            CfgErr::Invalid(_)
        ),
        "SLC-003.9"
    );
    let usdm = CFG0.replace("product = \"spot\"", "product = \"usdm\"");
    assert!(matches!(err(&usdm), CfgErr::Invalid(_)), "SLC-003.10");
    assert!(
        config::parse(&format!("{usdm}\n[orders]\nenabled = false\n")).is_ok(),
        "SLC-003.11"
    );
}

#[test]
fn slc_004_mainnet_order_host_refused() {
    let with = |h: &str| config::parse(&format!("{CFG0}\n[orders]\nhost = \"{h}\"\n"));
    for (row, h) in [
        ("SLC-004.1", "ws-api.binance.com"),
        ("SLC-004.2", "ws-fapi.binance.com"),
        ("SLC-004.3", "WS-API.Binance.com"),
        ("SLC-004.4", "api3.binance.com"),
        ("SLC-004.5", "fstream.binance.com"),
        ("SLC-004.6", "example.com"),
    ] {
        assert!(matches!(with(h), Err(CfgErr::MainnetOrderHost(_))), "{row}");
    }
    assert!(with("demo-ws-api.binance.com").is_ok(), "SLC-004.7");
    assert!(with("testnet.binancefuture.com").is_ok(), "SLC-004.8");
    let off = config::parse(&format!(
        "{CFG0}\n[orders]\nhost = \"ws-api.binance.com\"\nenabled = false\n"
    ));
    assert!(off.is_ok(), "SLC-004.9");
}

/// RFC 8032 test 1 private key as a PKCS#8 PEM.
pub fn ed1_pem() -> String {
    let mut der = vec![
        0x30, 0x2e, 0x02, 0x01, 0x00, 0x30, 0x05, 0x06, 0x03, 0x2b, 0x65, 0x70, 0x04, 0x22, 0x04, 0x20,
    ];
    der.extend(hex(ED1_SEED));
    let b = base64::engine::general_purpose::STANDARD.encode(&der);
    format!("-----BEGIN PRIVATE KEY-----\n{b}\n-----END PRIVATE KEY-----\n")
}

fn hex(s: &str) -> Vec<u8> {
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap())
        .collect()
}

fn env_of(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> + use<> {
    let v: Vec<(String, String)> = pairs
        .iter()
        .map(|(a, b)| (a.to_string(), b.to_string()))
        .collect();
    move |k: &str| v.iter().find(|(a, _)| a == k).map(|(_, b)| b.clone())
}

#[test]
fn slc_005_key_sources() {
    let seed: [u8; 32] = hex(ED1_SEED).try_into().unwrap();
    let env = env_of(&[("HFT_SLICE_API_KEY", "TESTKEY"), ("HFT_SLICE_ED25519", ED1_SEED)]);
    let k = load_keys(&cfg_with(""), &env).unwrap().unwrap();
    assert_eq!((k.api_key.as_str(), k.seed), ("TESTKEY", seed), "SLC-005.1");
    for (row, key) in [
        ("SLC-005.2", "api_key"),
        ("SLC-005.3", "secret"),
        ("SLC-005.4", "private_key"),
    ] {
        let r = config::parse(&format!("{CFG0}\n[orders]\n{key} = \"x\"\n"));
        assert!(matches!(r, Err(CfgErr::Key(_))), "{row}");
    }
    let no_key = env_of(&[("HFT_SLICE_ED25519", ED1_SEED)]);
    assert!(
        matches!(load_keys(&cfg_with(""), &no_key), Err(CfgErr::Key(_))),
        "SLC-005.5"
    );
    let both = config::parse(&format!(
        "{CFG0}\n[orders]\nsecret_env = \"A\"\nsecret_file = \"/x\"\n"
    ));
    assert!(matches!(both, Err(CfgErr::Key(_))), "SLC-005.6");
    let dir = support::tmp("keys");
    let f = dir.join("ed.pem");
    std::fs::write(&f, ed1_pem()).unwrap();
    set_mode(&f, 0o600);
    let cfg = config::parse(&format!(
        "{CFG0}\n[orders]\nsecret_file = {:?}\n",
        f.display().to_string()
    ))
    .unwrap();
    let env = env_of(&[("HFT_SLICE_API_KEY", "TESTKEY")]);
    assert_eq!(load_keys(&cfg, &env).unwrap().unwrap().seed, seed, "SLC-005.7");
    if cfg!(unix) {
        set_mode(&f, 0o644);
        assert!(matches!(load_keys(&cfg, &env), Err(CfgErr::Key(_))), "SLC-005.8");
    }
    let off = cfg_with("[orders]\nenabled = false\n");
    assert_eq!(load_keys(&off, &env_of(&[])).unwrap(), None, "SLC-005.9");
}

#[cfg(unix)]
fn set_mode(p: &std::path::Path, m: u32) {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(p, std::fs::Permissions::from_mode(m)).unwrap();
}

#[cfg(not(unix))]
fn set_mode(_p: &std::path::Path, _m: u32) {}

#[test]
fn slc_006_pem_seed() {
    let seed = config::pem_seed(&ed1_pem()).unwrap();
    assert_eq!(seed.to_vec(), hex(ED1_SEED), "step 1");
    use hft_venue_proto::{
        ArrayBuf, Credential, LoginStep, Role, SecretBuf, SignAlg, VenueCfg, VenueEnv, VenueProtocol,
    };
    let cred = Credential {
        api_key: ArrayBuf::from_bytes(b"TESTKEY").unwrap(),
        secret: SecretBuf(ArrayBuf::from_bytes(&seed).unwrap()),
        passphrase: ArrayBuf::new(),
        alg: SignAlg::Ed25519,
    };
    let mut out = Vec::new();
    let cfg = VenueCfg {
        env: VenueEnv::Testnet,
        stale_guard_ms: 1500,
    };
    assert_eq!(
        venue_binance::Spot::login(Role::Order, &cfg, &cred, 0, 1_790_000_000_000, &mut out),
        Ok(LoginStep::Frame)
    );
    let f = String::from_utf8(out).unwrap();
    assert!(
        f.contains(r#""signature":"kyKvl/kTaZKE+kUIQtZzLoF+xBBLbhAc+PoRvtG+MseTjjM1xPqXeIBiOYildYFAu7yqD9fn723B1gwti8VcCA==""#),
        "step 2: {f}"
    );
    let public = "-----BEGIN PUBLIC KEY-----\nMCowBQYDK2VwAyEA11qYAYKxCrfVS/7TyWQHOg7hcvPapiMlrwIaaPcHURo=\n-----END PUBLIC KEY-----\n";
    assert!(config::pem_seed(public).is_err(), "step 3");
}
