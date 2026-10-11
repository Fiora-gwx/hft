use hft_exec::{ExecAccount, ExecConfig, ExecParams};
use hft_strategy_sdk::TakerSliceParams;
use hft_types::{AccountId, Notional, Rate};

#[test]
fn exe_514_config_section_roundtrip() {
    let p = ExecParams {
        band_floor_notional: Notional(21_000_001),
        dust_notional: Notional(5_000_002),
        fix_tolerance_default: Rate(25_000_003),
        fix_max: 4,
        fix_cooldown_ms: 1201,
        fix_deadline_ms: 13_002,
        broken_auto_fix_ms: 150_003,
        external_deadline_ms: 6104,
        gate_naked_alarm_ms: 301_005,
        stuck_after_ms: 3121,
        gated_backoff_ms: 789,
        max_slice_fails: 6,
        max_nofill: 7,
        urgent_cooldown_ms: 105,
        passive_cooldown_mult: 8,
        force_slice: TakerSliceParams {
            depth_frac: Rate(250_000_001),
            depth_levels: 7,
            _pad0: [0; 7],
            max_cross: Rate(4_000_002),
            max_chunk_notional: Notional(998_000_003),
            min_chunk_notional: Notional(7_000_004),
            cooldown_ms: 399,
            _pad1: 0,
        },
    };
    let c = ExecConfig {
        default: p,
        accounts: vec![
            ExecAccount {
                account: AccountId(1),
                sim: false,
                params: None,
            },
            ExecAccount {
                account: AccountId(2),
                sim: true,
                params: Some(ExecParams { fix_max: 9, ..p }),
            },
        ],
    };
    assert_eq!(ExecConfig::from_rows(&c.to_rows()), Some(c.clone()));
    let mut reversed = c.clone();
    reversed.accounts.reverse();
    assert_eq!(reversed.to_rows(), c.to_rows());
}

#[test]
fn exe_515_config_section_rejects_malformed() {
    let c = ExecConfig {
        default: ExecParams::default(),
        accounts: vec![ExecAccount {
            account: AccountId(1),
            sim: false,
            params: None,
        }],
    };
    let rows = c.to_rows();
    assert!(ExecConfig::from_rows(&rows[1..]).is_none());
    let mut bad = rows.clone();
    bad.push(rows[1]);
    assert!(ExecConfig::from_rows(&bad).is_none());
    for field in 0..5 {
        let mut bad = rows.clone();
        match field {
            0 => bad[1].sim = 2,
            1 => bad[1].has_params = 2,
            2 => bad[1].reserved[0] = 1,
            3 => bad[1].params[0] = 1,
            _ => bad[0].params[3] = 256,
        }
        assert!(ExecConfig::from_rows(&bad).is_none(), "malformed field {field}");
    }
}
