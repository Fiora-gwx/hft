//! EXE-001–EXE-004: configuration defaults, `new` checks, account rows.
mod common;
use common::*;
use hft_exec::*;
use hft_strategy_sdk::SnapshotWriter;
use hft_types::Rate;

#[test]
fn exe_001_defaults() {
    let d = ExecConfig::default().default;
    assert_eq!(
        d.band_floor_notional,
        hft_types::Notional(20_000_000),
        "EXE-001.1"
    );
    assert_eq!(d.dust_notional, hft_types::Notional(5_000_000), "EXE-001.2");
    assert_eq!(d.fix_tolerance_default, Rate(20_000_000), "EXE-001.3");
    assert_eq!(d.fix_max, 3, "EXE-001.4");
    assert_eq!(d.fix_cooldown_ms, 1000, "EXE-001.5");
    assert_eq!(d.fix_deadline_ms, 10_000, "EXE-001.6");
    assert_eq!(d.broken_auto_fix_ms, 120_000, "EXE-001.7");
    assert_eq!(d.external_deadline_ms, 5000, "EXE-001.8");
    assert_eq!(d.gate_naked_alarm_ms, 300_000, "EXE-001.9");
    assert_eq!(d.stuck_after_ms, 2000, "EXE-001.10");
    assert_eq!(d.gated_backoff_ms, 500, "EXE-001.11");
    assert_eq!(d.max_slice_fails, 3, "EXE-001.12");
    assert_eq!(d.max_nofill, 5, "EXE-001.13");
    assert_eq!(d.urgent_cooldown_ms, 100, "EXE-001.14");
    assert_eq!(d.passive_cooldown_mult, 4, "EXE-001.15");
    let f = d.force_slice;
    assert_eq!(
        (
            f.depth_frac,
            f.depth_levels,
            f.max_cross,
            f.max_chunk_notional,
            f.min_chunk_notional,
            f.cooldown_ms
        ),
        (
            Rate(300_000_000),
            5,
            Rate(5_000_000),
            hft_types::Notional(1_000_000_000),
            hft_types::Notional(0),
            200
        ),
        "EXE-001.16"
    );
}

#[test]
fn exe_002_new_preallocates() {
    let ex = Exec::new(&cfg(c0()), &caps(256)).unwrap();
    assert_eq!(ex.capacity(), 259);
    assert!(!ex.has_pending());
    assert!(ex.drain_ready());
    let mut w = SnapshotWriter::new(1 << 20);
    ex.snapshot(&mut w);
    let b = w.as_bytes();
    // SectionHdr (16 B) then 3 Gate rows.
    let rows = &b[16..];
    let mut gates = 0;
    let mut off = 0;
    while off + 8 <= rows.len() && rows[off] != 0 {
        let kind = rows[off];
        let len = u32::from_le_bytes(rows[off + 4..off + 8].try_into().unwrap()) as usize;
        assert_eq!(kind, ROW_GATE);
        let body = &rows[off + 8..off + 8 + len];
        assert_eq!(body[0], [1u8, 2, 3][gates]);
        assert_eq!(body[1], 0);
        assert!(body[8..24].iter().all(|x| *x == 0));
        gates += 1;
        off += 8 + len;
    }
    assert_eq!(gates, 3);
}

#[test]
fn exe_003_new_rejects_out_of_range() {
    type Row<'a> = (&'a str, fn(&mut ExecParams), Result<(), ExecConfigError>);
    let rows: [Row; 11] = [
        (
            "EXE-003.1",
            |p| p.force_slice.depth_levels = 0,
            Err(ExecConfigError::ForceSlice),
        ),
        ("EXE-003.2", |p| p.force_slice.depth_levels = 1, Ok(())),
        ("EXE-003.3", |p| p.force_slice.depth_levels = 10, Ok(())),
        (
            "EXE-003.4",
            |p| p.force_slice.depth_levels = 11,
            Err(ExecConfigError::ForceSlice),
        ),
        (
            "EXE-003.5",
            |p| p.force_slice.depth_frac = Rate(0),
            Err(ExecConfigError::ForceSlice),
        ),
        (
            "EXE-003.6",
            |p| p.force_slice.depth_frac = Rate(1_000_000_000),
            Ok(()),
        ),
        (
            "EXE-003.7",
            |p| p.force_slice.depth_frac = Rate(1_000_000_001),
            Err(ExecConfigError::ForceSlice),
        ),
        ("EXE-003.8", |p| p.fix_max = 0, Err(ExecConfigError::ZeroLimit)),
        (
            "EXE-003.9",
            |p| p.max_slice_fails = 0,
            Err(ExecConfigError::ZeroLimit),
        ),
        (
            "EXE-003.10",
            |p| p.max_nofill = 0,
            Err(ExecConfigError::ZeroLimit),
        ),
        (
            "EXE-003.11",
            |p| p.passive_cooldown_mult = 0,
            Err(ExecConfigError::ZeroLimit),
        ),
    ];
    for (row, f, want) in rows {
        let mut p = c0();
        f(&mut p);
        assert_eq!(Exec::new(&cfg(p), &caps(256)).map(|_| ()), want, "{row}");
    }
    assert!(Exec::new(&cfg(c0()), &caps(65_532)).is_ok(), "EXE-003.12");
    assert_eq!(
        Exec::new(&cfg(c0()), &caps(65_533)).map(|_| ()),
        Err(ExecConfigError::Capacity),
        "EXE-003.13"
    );
}

#[test]
fn exe_004_account_row_overrides_default() {
    let mut c = cfg(c0());
    c.accounts[1].params = Some(ExecParams {
        dust_notional: usd("10"),
        ..c0()
    });
    let g9 = gspec(G9, &[(S, A), (P, B)]);
    let mut fx = Fx::with_cfg(c, vec![g7(), g8(), g9], default_insts());
    fx.spot(A1, A, G7, "100", "100");
    fx.spot(A2, A, G9, "100", "100");
    fx.legs(A1, G7, &["1.08", "-1.00"]);
    fx.legs(A2, G9, &["1.08", "-1.00"]);
    fx.leg_adjust(A1, G7);
    fx.leg_adjust(A2, G9);
    let r = fx.adv();
    let fixes = r.ord(ExecOrderKind::HedgeFix);
    assert_eq!(fixes.len(), 1);
    let o = fixes[0];
    assert_eq!(
        (o.intent.group, o.intent.side, o.intent.inst, o.intent.qty),
        (G7, hft_types::Side::Sell, S, q("0.08"))
    );
    assert!(r.decisions.iter().all(|d| d.group != G9));
}
