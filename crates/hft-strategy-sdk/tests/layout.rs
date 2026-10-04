//! SPB1 body struct layout (SDK-100).

use core::mem::{align_of, offset_of, size_of};

use hft_strategy_sdk::params::{ParamOverride, ParamsBodyHdr};

#[test]
fn sdk_100_body_struct_layout() {
    assert_eq!(
        (size_of::<ParamsBodyHdr>(), align_of::<ParamsBodyHdr>()),
        (8, 4),
        "SDK-100.1"
    );
    assert_eq!(
        (
            offset_of!(ParamsBodyHdr, n_fields),
            offset_of!(ParamsBodyHdr, _pad),
            offset_of!(ParamsBodyHdr, n_overrides)
        ),
        (0, 2, 4),
        "SDK-100.2"
    );
    assert_eq!(
        (size_of::<ParamOverride>(), align_of::<ParamOverride>()),
        (16, 8),
        "SDK-100.3"
    );
    assert_eq!(
        (
            offset_of!(ParamOverride, group_ix),
            offset_of!(ParamOverride, field_ix),
            offset_of!(ParamOverride, _pad),
            offset_of!(ParamOverride, value)
        ),
        (0, 2, 4, 8),
        "SDK-100.4"
    );
}
