use hft_secrets::{Entry, Provider, Secret};
struct Never;
impl Provider for Never {
    fn get(&mut self, _: &Entry) -> Result<Secret, &'static str> {
        panic!("check must not fetch")
    }
}
const VALID: &str = "[[secret]]\nname='a'\nsecret_id='arn:aws:secretsmanager:us-east-1:123456789012:secret:a'\njson_key='api_key'\nkind='plain'\n";

#[test]
fn tool_117_manifest_rejects() {
    for bad in [
        format!("{VALID}value='S3CR3T'\n"),
        format!("{VALID}secret='x'\n"),
        format!("{VALID}{VALID}"),
        VALID.replace("kind='plain'", "kind='env'"),
    ] {
        assert!(hft_secrets::manifest(&bad).is_err());
    }
    assert!(hft_secrets::manifest(VALID).is_ok());
    for name in ["", "..", "../a", "a/b", "a\\b", "a\n"] {
        assert!(hft_secrets::manifest(&VALID.replace("name='a'", &format!("name='{name}'"))).is_err());
    }
}

#[test]
fn tool_181_check_mode() {
    let dir =
        std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("manifest-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("m.toml");
    for (row, text) in [
        Some(VALID.to_owned()),
        Some(format!("{VALID}value='S3CR3T'\n")),
        Some(format!("{VALID}{VALID}")),
        Some(VALID.replace("kind='plain'", "kind='env'")),
        Some(VALID.replace(
            "arn:aws:secretsmanager:us-east-1:123456789012:secret:a",
            "hft/live/a",
        )),
        Some(VALID.replace("name='a'", "name=''")),
        None,
    ]
    .into_iter()
    .enumerate()
    {
        if let Some(t) = text {
            std::fs::write(&file, t).unwrap();
        } else {
            std::fs::remove_file(&file).unwrap();
        }
        let args = vec!["--check".into(), "--manifest".into(), file.display().to_string()];
        let (mut out, mut err) = (Vec::new(), Vec::new());
        let code = hft_secrets::run(&args, &mut out, &mut err, || -> Result<Never, &'static str> {
            panic!("check initialized credentials/network")
        });
        assert_eq!(code, if row == 0 { 0 } else { 2 }, "row {row}");
        assert!(out.is_empty());
        assert!(!String::from_utf8(err).unwrap().contains("S3CR3T"));
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), usize::from(row != 6));
    }
    std::fs::remove_dir(dir).unwrap();
}
