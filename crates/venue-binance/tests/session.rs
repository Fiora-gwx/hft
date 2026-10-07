//! BIN-060–BIN-062, BIN-190, BIN-191: logon frames, login answers, heartbeats.
#![allow(clippy::disallowed_types)] // serde_json::Value: reference parser of the frames under test

mod support;

use base64::Engine;
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use hft_venue_proto::{HeartbeatSpec, LoginResult, LoginStep, PingKind, Role, VenueProtocol};
use support::{ED1_PUB, cfg, ed1, hex};
use venue_binance::{Spot, Usdm};

const NOW: i64 = 1_790_000_000_000;
const FRAME: &str = r#"{"id":"l0000000000000000","method":"session.logon","params":{"apiKey":"TESTKEY","signature":"kyKvl/kTaZKE+kUIQtZzLoF+xBBLbhAc+PoRvtG+MseTjjM1xPqXeIBiOYildYFAu7yqD9fn723B1gwti8VcCA==","timestamp":1790000000000}}"#;

fn logon<P: VenueProtocol>() -> String {
    let mut out = Vec::new();
    let r = P::login(Role::Order, &cfg(1500), &ed1(), 0, NOW, &mut out);
    assert_eq!(r, Ok(LoginStep::Frame));
    String::from_utf8(out).unwrap()
}

#[test]
fn bin_060_usdm_logon_frame() {
    let f = logon::<Usdm>();
    assert_eq!(f, FRAME);
    // The signature verifies with the RFC 8032 test 1 public key.
    let v: serde_json::Value = serde_json::from_str(&f).unwrap();
    let sig_b64 = v["params"]["signature"].as_str().unwrap();
    let sig = base64::engine::general_purpose::STANDARD.decode(sig_b64).unwrap();
    let key = VerifyingKey::from_bytes(&hex(ED1_PUB).try_into().unwrap()).unwrap();
    key.verify(
        b"apiKey=TESTKEY&timestamp=1790000000000",
        &Signature::from_slice(&sig).unwrap(),
    )
    .unwrap();
}

#[test]
fn bin_061_spot_logon_frame() {
    assert_eq!(logon::<Spot>(), FRAME);
}

#[test]
fn bin_062_login_result() {
    let rows: [(&str, &[u8], LoginResult); 6] = [
        (
            "BIN-062.1",
            br#"{"id":"l0000000000000000","status":200,"result":{"apiKey":"TESTKEY","authorizedSince":1790000000001,"connectedSince":1790000000000}}"#,
            LoginResult::Ok,
        ),
        (
            "BIN-062.2",
            br#"{"id":"l0000000000000000","status":401,"error":{"code":-1022,"msg":"Signature for this request is not valid."}}"#,
            LoginResult::Rejected(0xFFFF_FC02),
        ),
        (
            "BIN-062.3",
            br#"{"id":"l0000000000000000","status":400,"error":{"code":-1021,"msg":"Timestamp for this request is outside of the recvWindow."}}"#,
            LoginResult::Rejected(0xFFFF_FC03),
        ),
        (
            "BIN-062.4",
            br#"{"id":"l0000000000000000","status":503}"#,
            LoginResult::Rejected(0x2000_01F7),
        ),
        (
            "BIN-062.5",
            br#"{"id":"p0000002a00000007","status":200,"result":{}}"#,
            LoginResult::NotMine,
        ),
        ("BIN-062.6", b"pong", LoginResult::NotMine),
    ];
    for (row, p, want) in rows {
        assert_eq!(Usdm::login_result(p), want, "{row}");
        assert_eq!(Spot::login_result(p), want, "{row} (spot)");
    }
}

#[test]
fn bin_190_heartbeat_spec() {
    let hb = |idle| HeartbeatSpec {
        ping: PingKind::WsPing,
        interval_ms: 0,
        unconditional: false,
        pong_timeout_ms: 10_000,
        idle_timeout_ms: idle,
    };
    assert_eq!(Usdm::heartbeat(Role::Md), hb(360_000), "BIN-190.1");
    assert_eq!(Usdm::heartbeat(Role::UserStream), hb(360_000), "BIN-190.2");
    assert_eq!(Usdm::heartbeat(Role::Order), hb(40_000), "BIN-190.3");
    assert_eq!(Spot::heartbeat(Role::Md), hb(40_000), "BIN-190.4");
    assert_eq!(Spot::heartbeat(Role::Order), hb(40_000), "BIN-190.5");
}

#[test]
fn bin_191_bbo_probe_ws_ping() {
    assert_eq!(Usdm::bbo_probe(), PingKind::WsPing);
    assert_eq!(Spot::bbo_probe(), PingKind::WsPing);
    for role in [Role::Md, Role::Order, Role::UserStream, Role::Rest] {
        let mut out = [0xAAu8; 64];
        assert_eq!(Usdm::app_ping(role, NOW, &mut out), 0);
        assert_eq!(Spot::app_ping(role, NOW, &mut out), 0);
        assert!(out.iter().all(|&b| b == 0xAA));
    }
}
