//! SPB1 body struct layout (SDK-100).

#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods
)] // allow-list: test code, not a hot path

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

use hft_strategy_sdk::*;
use hft_types::{AccountId, Notional, Rate, VenueId};

macro_rules! lay {
    ($row:literal, $t:ty, $size:expr, $align:expr; $($f:ident = $off:expr),* $(,)?) => {
        assert_eq!((size_of::<$t>(), align_of::<$t>()), ($size, $align), "{} size/align", $row);
        $(assert_eq!(offset_of!($t, $f), $off, "{} {}", $row, stringify!($f));)*
    };
}

#[test]
fn sdk_141_repr_c_layouts() {
    lay!("SDK-141.1", VenueEquityView, 56, 8; flags = 2, equity = 8, margin_balance = 40, as_of_ens = 48);
    lay!("SDK-141.2", LegTrack, 32, 8; inflight_immediate = 8, inflight_resting = 16, residual = 24);
    lay!("SDK-141.3", LegView, 160, 8; naked_qty = 128, naked_ratio = 144, n_legs = 152, has_unknown = 154, has_pending = 155);
    lay!("SDK-141.4", OrderIntent, 112, 8; class = 4, flags = 5, inst = 8, group = 12, px = 16, qty = 24, link = 32, tag = 104, max_rest_ms = 108);
    lay!("SDK-141.5", LegLinkOpt, 72, 8; some = 0, l = 8);
    lay!("SDK-141.6", LegLink, 64, 8; role = 4, tolerance = 8, hedge = 16);
    lay!("SDK-141.7", HedgePlan, 48, 8; reduce_only = 7, inst = 8, ratio_num = 12, ratio_den = 16, residual = 20, px_param = 24, max_slip = 32, limit_px = 40);
    lay!("SDK-141.8", OrderRef, 12, 4; slot = 4, r#gen = 8);
    lay!("SDK-141.9", TargetIntent, 232, 8; group = 4, legs = 8, n_legs = 72, reason = 73, urgency = 74, band = 80, exec = 88, tag = 224);
    lay!("SDK-141.10", LegTarget, 16, 8; qty = 8);
    lay!("SDK-141.11", ExecSpec, 136, 8;);
    lay!("SDK-141.12", TakerSliceParams, 48, 8; depth_levels = 8, max_cross = 16, max_chunk_notional = 24, min_chunk_notional = 32, cooldown_ms = 40);
    lay!("SDK-141.13", PairParams, 96, 8; first_leg = 48, first_leg_ix = 49, hedge_px_mode = 50, hedge_cross = 56, hedge_max_slip = 64, ratio_num = 72, fix_tolerance = 80, residual = 88);
    lay!("SDK-141.14", SimulParams, 72, 8; on_fail = 48, fix_tolerance = 56, residual = 64);
    lay!("SDK-141.15", MakerParams, 128, 8; max_rest_ms = 16, chunk_notional = 24, hedge = 32);
    // ExecSpec: discriminant byte at 0, payload at 8.
    let s = ExecSpec::Simultaneous(SimulParams {
        slice: TakerSliceParams {
            depth_frac: Rate(1),
            depth_levels: 1,
            _pad0: [0; 7],
            max_cross: Rate(2),
            max_chunk_notional: Notional(3),
            min_chunk_notional: Notional(4),
            cooldown_ms: 5,
            _pad1: 0,
        },
        on_fail: SimulOnFail::Hold,
        _pad0: [0; 7],
        fix_tolerance: Rate(6),
        residual: ResidualPolicy::Hold,
        _pad1: [0; 7],
    });
    let p = core::ptr::addr_of!(s).cast::<u8>();
    assert_eq!(s.algo(), 3, "SDK-141.11 algo");
    // SAFETY: test-only read of the discriminant byte and the first payload word of a live value.
    let (tag, first) = unsafe { (*p, core::ptr::read_unaligned(p.add(8).cast::<i64>())) };
    assert_eq!((tag, first), (3, 1), "SDK-141.11 discriminant at 0, payload at 8");
}

/// Σ field sizes, given as `size_of_val` of every field of a value.
macro_rules! sum {
    ($v:expr; $($f:ident),*) => { 0 $(+ core::mem::size_of_val(&$v.$f))* };
}

fn assert_into_bytes<T: zerocopy::IntoBytes + zerocopy::Immutable>() {}

#[test]
fn sdk_142_no_implicit_padding() {
    let ve = VenueEquityView::empty(VenueId(1));
    assert_eq!(
        sum!(ve; venue, _pad0, flags, _pad1, equity, quote_cash, spot_value, perp_upnl, margin_balance, as_of_ens),
        56
    );
    let lt = LegTrack::ZERO;
    assert_eq!(sum!(lt; net, inflight_immediate, inflight_resting, residual), 32);
    let lv = LegView::EMPTY;
    assert_eq!(
        sum!(lv; leg, naked_qty, naked_notional, naked_ratio, n_legs, open_links, has_unknown, has_pending, _pad),
        160
    );
    let hp = HedgePlan::NONE;
    assert_eq!(
        sum!(hp; kind, px_mode, on_fail, tif, venue, account, side, reduce_only, inst, ratio_num, ratio_den, residual, _pad, px_param, max_slip, limit_px),
        48
    );
    let lo = LegLinkOpt::NONE;
    assert_eq!(sum!(lo; some, _pad, l), 72);
    assert_eq!(sum!(lo.l; group, role, _pad, tolerance, hedge), 64);
    let r = OrderRef::new(VenueId(1), AccountId(1), 2, 3);
    assert_eq!(sum!(r; venue, account, _pad, slot, r#gen), 12);
    let lt2 = LegTarget::ZERO;
    assert_eq!(sum!(lt2; leg_ix, _pad, qty), 16);
    // Every repr(C) struct except ExecSpec and TargetIntent (which contains it) is IntoBytes.
    assert_into_bytes::<VenueEquityView>();
    assert_into_bytes::<LegTrack>();
    assert_into_bytes::<LegView>();
    assert_into_bytes::<OrderIntent>();
    assert_into_bytes::<LegLinkOpt>();
    assert_into_bytes::<LegLink>();
    assert_into_bytes::<HedgePlan>();
    assert_into_bytes::<OrderRef>();
    assert_into_bytes::<LegTarget>();
    assert_into_bytes::<TakerSliceParams>();
    assert_into_bytes::<PairParams>();
    assert_into_bytes::<SimulParams>();
    assert_into_bytes::<MakerParams>();
    // TargetIntent: its fields sum to its size with ExecSpec counted as 136 B.
    assert_eq!(
        1 + 3 + 4 + 64 + 1 + 1 + 1 + 5 + 8 + 136 + 4 + 4,
        size_of::<TargetIntent>()
    );
}
