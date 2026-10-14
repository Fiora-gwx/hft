//! Shared demo UID is a warning only while both product accounts remain non-production.
use hft_refdata::{LoadErrCode, RefData};

#[test]
fn bin_212_demo_shared_uid() {
    let demo = RefData::load(&super::support::demo_blob(false), None).unwrap();
    assert_eq!(demo.warnings().len(), 1);
    assert_eq!(demo.warnings()[0].code, LoadErrCode::DuplicateVenueUid);
    assert_eq!(demo.warnings()[0].section, 14);
    let errors = RefData::load(&super::support::demo_blob(true), None)
        .unwrap_err()
        .errors;
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].code, LoadErrCode::DuplicateVenueUid);
    assert_eq!(errors[0].section, 14);
}
