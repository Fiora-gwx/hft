//! REF-077: the exported schema matches the code table and the row structs.

use core::mem::{offset_of, size_of};

use hft_refdata::LoadErrCode;
use hft_refdata::blob::{
    AccountRow, AccountVenueRow, AliasRow, AssetRow, CapsRow, EndpointRow, FeeOverrideRow, FeeRow, InstRow,
    InstanceRow, MaintRow, MmrTierRow, RateLimitRow, SimAccountRow, SimBalanceRow, StrRef, TokenRow,
    VenueRow,
};
use hft_refdata::schema::{RDB1_ROWS, load_err_codes};

/// (row name, size_of, [(field, offset_of)]) computed independently of `schema.rs`.
macro_rules! layout {
    ($name:literal, $t:ty, [$($f:ident),*]) => {
        ($name, size_of::<$t>(), vec![$((stringify!($f), offset_of!($t, $f))),*])
    };
}

#[test]
fn ref_077_schema_export() {
    // Step 1: 51 codes equal to REF-076.
    let exported: Vec<(&str, u16)> = load_err_codes().collect();
    assert_eq!(exported.len(), 51);
    for (k, (name, value)) in exported.iter().enumerate() {
        let code = LoadErrCode::try_from(*value).expect("assigned");
        assert_eq!(*value as usize, k + 1);
        assert_eq!(*name, format!("{code:?}"));
    }
    // Step 2: offsets and sizes equal offset_of! / size_of.
    let want = vec![
        layout!(
            "AssetMeta",
            AssetRow,
            [id, class, px_exp, qty_exp, _pad, name, px_max, qty_max, dust]
        ),
        layout!(
            "VenueMeta",
            VenueRow,
            [
                id,
                exchange,
                product,
                ts_unit,
                stale_guard,
                client_id_max_len,
                max_batch,
                max_inflight_amend,
                ws_max_streams_per_conn,
                ws_max_in_msgs_per_s,
                ws_new_conn_per_5min_ip,
                app_ping_interval_s,
                ws_max_conn_age_s,
                crossed_tolerance_default,
                _pad,
                client_id_prefix
            ]
        ),
        layout!(
            "InstMeta",
            InstRow,
            [
                id,
                venue,
                status,
                kind,
                flags,
                asset,
                quote,
                book_window,
                px_unit_shift,
                qty_unit_shift,
                l2_depth,
                crossed_tolerance,
                max_leverage,
                tick,
                lot,
                min_qty,
                max_qty,
                min_notional,
                venue_num_id,
                symbol,
                tick_str,
                lot_str,
                min_qty_str,
                funding_interval_s,
                funding_anchor_s,
                listed_utc_ns,
                delivery_utc_ns,
                band_up,
                band_down
            ]
        ),
        layout!(
            "AccountMeta",
            AccountRow,
            [id, kind, bound_instance, is_prod, uid_policy, _pad, venues, label]
        ),
        layout!("Fee", FeeRow, [venue, tier, _pad, maker, taker]),
        layout!(
            "FeeOverride",
            FeeOverrideRow,
            [account, venue, _pad, inst, maker, taker]
        ),
        layout!(
            "RateLimit",
            RateLimitRow,
            [venue, scope, bucket, _pad, limit, window_ms, default_cost, _pad2]
        ),
        layout!("Endpoint", EndpointRow, [venue, role, env, conn_group, _pad, url]),
        layout!(
            "MaintWindow",
            MaintRow,
            [venue, kind, _pad, inst, start_ns, end_ns]
        ),
        layout!("ChainToken", TokenRow, [chain, decimals, asset, _pad, address]),
        layout!("Alias", AliasRow, [venue, _pad, inst, alias]),
        layout!(
            "VenueCaps",
            CapsRow,
            [
                venue,
                market,
                fee_rule,
                reduce_only,
                position_mode,
                margin_mode,
                multi_asset,
                spot_borrow,
                funding_settle,
                margin_data,
                liq_marker,
                dms,
                testnet,
                amend_qty_down_only,
                fee_asset,
                margin_poll_ms,
                flags,
                dms_min_ms,
                dms_max_ms,
                dms_timeout_ms,
                dms_refresh_ms,
                liq_marker_arg,
                events_off,
                events_count
            ]
        ),
        layout!("AccountVenue", AccountVenueRow, [account, venue, _pad, uid]),
        layout!("InstanceMeta", InstanceRow, [id, _pad, kind_id, _pad2, name]),
        layout!(
            "SimAccount",
            SimAccountRow,
            [account, fee_tier, latency_model, margin_model, _pad]
        ),
        layout!("SimBalance", SimBalanceRow, [account, venue, asset, _pad, qty]),
        layout!("MmrTier", MmrTierRow, [inst, _pad, notional_max, mmr]),
        layout!("StrRef", StrRef, [off, len]),
    ];
    assert_eq!(RDB1_ROWS.len(), want.len());
    for (row, (name, size, fields)) in RDB1_ROWS.iter().zip(&want) {
        assert_eq!(row.name, *name);
        assert_eq!(row.size, *size, "{name}");
        let got: Vec<(String, usize)> = row
            .fields
            .iter()
            .map(|f| (f.name.to_string(), f.offset))
            .collect();
        let exp: Vec<(String, usize)> = fields.iter().map(|(f, o)| (format!("{name}.{f}"), *o)).collect();
        assert_eq!(got, exp, "{name}");
        let covered: usize = row.fields.iter().map(|f| f.size).sum();
        assert_eq!(
            covered, row.size,
            "{name}: fields cover the row without implicit padding"
        );
    }
    // Step 3: the only leverage field is the exchange's maximum.
    let lev: Vec<&str> = RDB1_ROWS
        .iter()
        .flat_map(|r| r.fields.iter())
        .map(|f| f.name)
        .filter(|n| n.contains("leverage"))
        .collect();
    assert_eq!(lev, vec!["InstMeta.max_leverage"]);
}
