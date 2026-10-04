//! `StrategyCmd` payload codec (SDK-112).

mod common;

use common::*;
use hft_strategy_sdk::CmdDesc;
use hft_strategy_sdk::params::*;

fn payload(ver: u8, op: u8, vals: &[i64]) -> Vec<u8> {
    let mut b = vec![ver, op, vals.len() as u8, 0, 0, 0, 0, 0];
    for v in vals {
        b.extend_from_slice(&v.to_le_bytes());
    }
    b
}

#[test]
fn sdk_112_cmd_codec() {
    let cmds = &ECHO_CMDS;
    let [open, close, set_mode] = cmds;
    let oor = ParamError::OutOfRange {
        field_ix: 0,
        group_ix: 0xFFFF,
    };

    let p1 = encode_cmd(open, &[1_000_000_000]).unwrap();
    let mut want = vec![1, 1, 1, 0, 0, 0, 0, 0];
    want.extend_from_slice(&1_000_000_000i64.to_le_bytes());
    assert_eq!(p1, want, "SDK-112.1");
    assert_eq!(decode_cmd(cmds, &p1), Ok((1, vec![1_000_000_000])), "SDK-112.2");

    let p3 = encode_cmd(close, &[]).unwrap();
    assert_eq!(p3, vec![1, 2, 0, 0, 0, 0, 0, 0], "SDK-112.3");
    assert_eq!(decode_cmd(cmds, &p3), Ok((2, vec![])), "SDK-112.4");

    assert_eq!(encode_cmd(open, &[10_000_000_001]), Err(oor), "SDK-112.5");
    assert_eq!(encode_cmd(open, &[]), Err(ParamError::BodyLen), "SDK-112.6");
    assert_eq!(
        encode_cmd(set_mode, &[2]),
        Err(ParamError::BadEnum { field_ix: 0 }),
        "SDK-112.7"
    );
    assert_eq!(
        decode_cmd(cmds, &payload(1, 9, &[])),
        Err(ParamError::UnknownOp { op: 9 }),
        "SDK-112.8"
    );
    let none: &[CmdDesc] = &[];
    assert_eq!(
        decode_cmd(none, &p1),
        Err(ParamError::UnknownOp { op: 1 }),
        "SDK-112.9"
    );
    assert_eq!(
        decode_cmd(cmds, &payload(2, 1, &[1])),
        Err(ParamError::BodyLen),
        "SDK-112.10"
    );
    assert_eq!(
        decode_cmd(cmds, &payload(1, 1, &[])),
        Err(ParamError::BodyLen),
        "SDK-112.11"
    );
    assert_eq!(
        decode_cmd(cmds, &payload(1, 1, &[10_000_000_001])),
        Err(oor),
        "SDK-112.12"
    );
    assert_eq!(decode_cmd(cmds, &p1[..7]), Err(ParamError::BodyLen), "SDK-112.13");
}
