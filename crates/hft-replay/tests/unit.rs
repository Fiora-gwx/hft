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
