//! hft-replay unit cases (TESTS.md RSR-001–RSR-003).
use hft_event::EventKind;
use hft_replay::table::{Col, Table};
use hft_replay::variants::{parse_file, parse_sets};
use hft_replay::venue::{masked_kind, report_account};
use hft_types::AccountId;

#[test]
fn rsr_001_variant_specs() {
    // .1 NAME=RAW pairs, spaces trimmed, later duplicates win.
    let m = parse_sets(" a=1, b=-2 ,a=3").unwrap();
    assert_eq!(
        m.into_iter().collect::<Vec<_>>(),
        vec![("a".into(), 3), ("b".into(), -2)],
        "RSR-001.1"
    );
    // .2 a missing `=` or a non-integer value is refused.
    assert!(parse_sets("a").is_err(), "RSR-001.2");
    assert!(parse_sets("a=1.5").is_err(), "RSR-001.2");
    // .3 a variants file: names unique and non-empty.
    let v = parse_file("[[variant]]\nname = \"x\"\nset = { a = 1 }\n[[variant]]\nname = \"y\"\n").unwrap();
    assert_eq!((v.len(), v[0].set["a"], v[1].set.len()), (2, 1, 0), "RSR-001.3");
    assert!(
        parse_file("[[variant]]\nname = \"x\"\n[[variant]]\nname = \"x\"\n").is_err(),
        "RSR-001.3"
    );
    assert!(parse_file("[[variant]]\nname = \"\"\n").is_err(), "RSR-001.3");
}

#[test]
fn rsr_002_parquet_round_trip() {
    use parquet::file::reader::{FileReader, SerializedFileReader};
    let dir = std::env::temp_dir().join(format!("rsr-002-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let mut t = Table::new(vec![
        ("i", Col::I64(vec![-1, 2])),
        ("u", Col::U64(vec![3, u64::MAX])),
        ("b", Col::U8(vec![0, 255])),
        ("f", Col::F64(vec![0.5, -1.25])),
        ("s", Col::Str(vec!["a".into(), "bc".into()])),
    ]);
    // .1 extend appends same-typed columns.
    t.extend(t.clone()).unwrap();
    assert_eq!(t.rows(), 4, "RSR-002.1");
    let p = dir.join("t.parquet");
    t.write(&p, &[("hft.run", "{\"k\":1}".into())]).unwrap();
    let r = SerializedFileReader::new(std::fs::File::open(&p).unwrap()).unwrap();
    let md = r.metadata().file_metadata();
    assert_eq!(md.num_rows(), 4, "RSR-002.1");
    let kv = md.key_value_metadata().unwrap();
    assert!(
        kv.iter()
            .any(|k| k.key == "hft.run" && k.value.as_deref() == Some("{\"k\":1}")),
        "RSR-002.1"
    );
    let names: Vec<_> = md
        .schema_descr()
        .columns()
        .iter()
        .map(|c| c.name().to_string())
        .collect();
    assert_eq!(names, ["i", "u", "b", "f", "s"], "RSR-002.1");
    // .2 a ragged table or a type change is refused.
    let bad = Table::new(vec![("i", Col::I64(vec![1])), ("u", Col::U64(vec![]))]);
    assert!(bad.write(&dir.join("bad.parquet"), &[]).is_err(), "RSR-002.2");
    let mut x = Table::new(vec![("i", Col::I64(vec![1]))]);
    assert!(
        x.extend(Table::new(vec![("i", Col::U64(vec![1]))])).is_err(),
        "RSR-002.2"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn rsr_003_report_accounts() {
    use core::mem::{offset_of, size_of};
    // .1 the account of each replaced report kind is read at its layout offset.
    for (kind, size, at) in [
        (
            EventKind::ORDER_EVENT,
            size_of::<hft_event::OrderEvent>(),
            offset_of!(hft_event::OrderEvent, account),
        ),
        (
            EventKind::SEND_REPORT,
            size_of::<hft_event::SendReport>(),
            offset_of!(hft_event::SendReport, account),
        ),
        (
            EventKind::EXTERNAL_FILL,
            size_of::<hft_event::ExternalFill>(),
            offset_of!(hft_event::ExternalFill, account),
        ),
        (
            EventKind::BALANCE_REPORT,
            size_of::<hft_event::BalanceReport>(),
            offset_of!(hft_event::BalanceReport, account),
        ),
        (
            EventKind::POSITION_REPORT,
            size_of::<hft_event::PositionReport>(),
            offset_of!(hft_event::PositionReport, account),
        ),
        (
            EventKind::FUNDING_SETTLE,
            size_of::<hft_event::FundingSettle>(),
            offset_of!(hft_event::FundingSettle, account),
        ),
    ] {
        let mut b = vec![0u8; size];
        b[at] = 7;
        assert!(masked_kind(kind), "RSR-003.1 {kind:?}");
        assert_eq!(report_account(kind, &b), Some(AccountId(7)), "RSR-003.1 {kind:?}");
    }
    // .2 connection facts and account configuration are not replaced.
    for kind in [
        EventKind::CONN_STATE,
        EventKind::ACCOUNT_CONFIG_REPORT,
        EventKind::BBO,
    ] {
        assert!(!masked_kind(kind), "RSR-003.2 {kind:?}");
        assert_eq!(report_account(kind, &[0; 64]), None, "RSR-003.2 {kind:?}");
    }
}

#[test]
fn rsr_004_calibration_table() {
    use hft_replay::SimParams;
    use hft_sim::{FeeModel, MakerFill, PoolLeg};
    // .1 the native fields and research latc.json pools (seconds), with the file hash.
    let text = br#"{"maker_fill":"queue_cap","tombstone":true,"cancel_us":[35000,5000],
        "taker":{"base_us":195000,"slow_us":1250000,"slow_ppm":125000,"resp_us":15000},
        "fee_k":0.07,"gtd_ms":300000,"fill_resp_us":12000,"taker_nofill_reject":true,
        "split_us":5000,"min_n":20,
        "thr":[30.0,60.0],"window_s":300,"phase_s":5,
        "pools":{"ack|1|0|0":[0.04,0.05],"cxl|2":[0.06],"take":[0.2],"other":[1.0]}}"#;
    let mut p = SimParams::default();
    let sha = hft_replay::calib::apply(text, &mut p).unwrap();
    assert_eq!(sha.len(), 64, "RSR-004.1");
    assert_eq!(p.maker_fill, MakerFill::QueueCap, "RSR-004.1");
    let v = &p.venue;
    assert!(v.tombstone, "RSR-004.1");
    assert_eq!(v.cancel_us, Some((35_000, 5_000)), "RSR-004.1");
    assert_eq!(
        v.taker.map(|t| (t.base_us, t.slow_ppm, t.resp_us)),
        Some((195_000, 125_000, 15_000)),
        "RSR-004.1"
    );
    assert_eq!(v.fee, FeeModel::BinaryTaker { k_e9: 70_000_000 }, "RSR-004.1");
    assert_eq!((v.gtd_ms, v.fill_resp_us), (300_000, Some(12_000)), "RSR-004.1");
    assert!(v.taker_nofill_reject, "RSR-004.1");
    assert_eq!(p.cond.load_thr_ns, vec![30_000_000, 60_000_000], "RSR-004.1");
    let pools = v.pools.as_ref().unwrap();
    assert_eq!(
        (pools.split_us, pools.min_n, pools.pools.len()),
        (5_000, 20, 3),
        "RSR-004.1 unknown leg skipped"
    );
    let ack = pools.pools.iter().find(|x| x.leg == PoolLeg::Place).unwrap();
    assert_eq!(
        (ack.load, ack.mv, ack.phase, ack.us.clone()),
        (Some(1), Some(0), Some(0), vec![40_000, 50_000]),
        "RSR-004.1"
    );
    let cxl = pools.pools.iter().find(|x| x.leg == PoolLeg::Cancel).unwrap();
    assert_eq!((cxl.load, cxl.mv), (Some(2), None), "RSR-004.1");
    // .2 an unknown field, maker model or a fee outside 0..=1 is refused.
    for bad in [
        &br#"{"bogus":1}"#[..],
        br#"{"maker_fill":"x"}"#,
        br#"{"fee_k":2.0}"#,
    ] {
        assert!(
            hft_replay::calib::apply(bad, &mut SimParams::default()).is_err(),
            "RSR-004.2"
        );
    }
}

#[test]
fn rsr_005_nullable_feature_column_and_field_metadata() {
    use parquet::arrow::arrow_reader::ParquetRecordBatchReaderBuilder;
    let dir = std::env::temp_dir().join(format!("rsr-005-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let mut t = Table::new(vec![("x", Col::OptI64(vec![Some(-3), None, Some(7)]))]);
    t.field_meta = vec![
        ("x", "unit".into(), "px".into()),
        ("x", "exp".into(), "-2".into()),
    ];
    let p = dir.join("f.parquet");
    t.write(&p, &[]).unwrap();
    let b = ParquetRecordBatchReaderBuilder::try_new(std::fs::File::open(&p).unwrap()).unwrap();
    // .1 the column is nullable and carries its unit and exponent.
    let f = b.schema().field_with_name("x").unwrap().clone();
    assert!(f.is_nullable(), "RSR-005.1");
    assert_eq!(
        (
            f.metadata().get("unit").map(String::as_str),
            f.metadata().get("exp").map(String::as_str)
        ),
        (Some("px"), Some("-2")),
        "RSR-005.1"
    );
    // .2 a missing value reads back as null.
    let batch = b.build().unwrap().next().unwrap().unwrap();
    let a = batch.column(0);
    assert_eq!((a.len(), a.null_count(), a.is_null(1)), (3, 1, true), "RSR-005.2");
    let _ = std::fs::remove_dir_all(&dir);
}
