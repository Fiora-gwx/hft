//! RKP1 is complete without any process-local configuration (RSK-284–286).
use super::common::*;
use hft_risk::{PreVec, RiskBlobError, RiskParams};
use hft_types::{AccountId, InstId};

#[test]
fn rsk_284_risk_blob_roundtrip() {
    let rd = r0_refdata();
    let mut p = p0_for(rd, 3);
    p.version = 0x1234_5678_9abc_def0;
    p.generated_at_ns = -123_456_789;
    p.author = *b"risk-codec-test!";
    let b = p.encode_blob().unwrap();
    assert_eq!(&b[..8], b"HFTRKP01");
    assert_eq!(&b[8..16], &0x1234_5678_9abc_def0u64.to_le_bytes());
    assert_eq!(&b[16..24], &(-123_456_789i64).to_le_bytes());
    assert_eq!(&b[24..40], b"risk-codec-test!");
    let q = RiskParams::decode_blob(&b, rd).unwrap();
    assert_eq!(p.version, q.version);
    assert_eq!(p.generated_at_ns, q.generated_at_ns);
    assert_eq!(p.author, q.author);
    assert_eq!(p.rate_headroom, q.rate_headroom);
    assert_eq!(p.conn_liveness_ms, q.conn_liveness_ms);
    assert_eq!(p.max_perp_leverage_x100, q.max_perp_leverage_x100);
    assert_eq!(p.l3_max_cross, q.l3_max_cross);
    assert_eq!(p.classes, q.classes);
    assert_eq!(p.risk_class.as_slice(), q.risk_class.as_slice());
    assert_eq!(p.accounts.as_slice(), q.accounts.as_slice());
    assert_eq!(p.venues.as_slice(), q.venues.as_slice());
    assert_eq!(p.groups.as_slice(), q.groups.as_slice());
    assert_eq!(b, q.encode_blob().unwrap());
    // Allocation capacity is not a configuration field and cannot alter the journal hash.
    let mut larger = Vec::with_capacity(256);
    larger.extend_from_slice(p.accounts.as_slice());
    p.accounts = PreVec::from_vec(larger);
    assert_eq!(b, p.encode_blob().unwrap());
}

#[test]
fn rsk_285_risk_blob_rejects_malformed() {
    let rd = r0_refdata();
    let p = p0_for(rd, 3);
    let b = p.encode_blob().unwrap();
    for n in 0..b.len() {
        assert!(RiskParams::decode_blob(&b[..n], rd).is_err(), "truncation at {n}");
    }
    for (off, word) in [(0, 0), (8, 0), (48, u64::MAX), (72, u64::MAX)] {
        let mut bad = b.clone();
        bad[off..off + 8].copy_from_slice(&word.to_le_bytes());
        assert!(RiskParams::decode_blob(&bad, rd).is_err(), "word at {off}");
    }
    let mut bad = b.clone();
    bad.push(0);
    assert_eq!(RiskParams::decode_blob(&bad, rd), Err(RiskBlobError::Trailing));
    // Header 72; classes count + rows; inst count + rows; account count. In an account row,
    // unknown_blocks_open is word 14 (after account, limits and daily-loss fields).
    let first_account = 72 + 8 + p.classes.len() * 24 + 8 + p.risk_class.len() * 16 + 8;
    let bool_off = first_account + 14 * 8;
    bad = b.clone();
    bad[bool_off..bool_off + 8].copy_from_slice(&2u64.to_le_bytes());
    assert_eq!(RiskParams::decode_blob(&bad, rd), Err(RiskBlobError::Value));
    let mut dup = p.clone();
    dup.accounts.as_mut_slice()[1].account = AccountId(1);
    assert_eq!(dup.encode_blob(), Err(RiskBlobError::Order));
    let mut missing = p.clone();
    missing.accounts.truncate(2);
    assert_eq!(
        RiskParams::decode_blob(&missing.encode_blob().unwrap(), rd),
        Err(RiskBlobError::Reference("accounts.missing"))
    );
    let mut unknown = p;
    unknown.risk_class.as_mut_slice().last_mut().unwrap().inst = InstId(999_999);
    assert_eq!(
        RiskParams::decode_blob(&unknown.encode_blob().unwrap(), rd),
        Err(RiskBlobError::Reference("risk_class.inst"))
    );
}

#[test]
fn rsk_286_risk_blob_no_process_state() {
    let rd = r0_refdata();
    let a = p0_for(rd, 3);
    let mut b = a.clone();
    b.accounts.as_mut_slice()[0].max_order_notional.0 += 23;
    b.conn_liveness_ms += 7;
    let aa = a.encode_blob().unwrap();
    let bb = b.encode_blob().unwrap();
    let threads: Vec<_> = [aa.clone(), bb.clone(), aa.clone(), bb.clone()]
        .into_iter()
        .map(|raw| {
            std::thread::spawn(move || RiskParams::decode_blob(&raw, rd).unwrap().encode_blob().unwrap())
        })
        .collect();
    for (t, want) in threads.into_iter().zip([aa.clone(), bb.clone(), aa, bb]) {
        assert_eq!(t.join().unwrap(), want);
    }
}
