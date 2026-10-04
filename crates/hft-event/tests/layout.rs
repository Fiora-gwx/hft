//! Runtime layout assertions (sizes, alignment, offsets) on top of the compile-time ones
//! (EVT-001, EVT-040, EVT-050, EVT-060, EVT-070, EVT-090, EVT-120, EVT-130, EVT-150, EVT-160).

use core::mem::offset_of;
use hft_event::*;

macro_rules! check {
    ($row:literal, $t:ty, $size:expr, $align:expr; $($f:ident @ $off:expr),* $(,)?) => {{
        assert_eq!(size_of::<$t>(), $size, "{} size", $row);
        if $align > 0 {
            assert_eq!(align_of::<$t>(), $align, "{} align", $row);
        }
        $(assert_eq!(offset_of!($t, $f), $off, "{} {}", $row, stringify!($f));)*
    }};
}

#[test]
fn evt_001_frame_layouts() {
    check!("EVT-001.1", RecordHeader, 24, 8; len @ 0, kind @ 4, flags @ 6, seq @ 8, tsc @ 16);
    check!("EVT-001.2", PartHdr, 16, 4; total_len @ 0, chunk_len @ 4, part @ 8, parts @ 10, _rsv @ 12);
    check!("EVT-001.3", CtlRecordHdr, 8, 4; kind @ 0, flags @ 2, len @ 4);
    check!("EVT-001.4", SnapshotHdr, 96, 8; purpose @ 0, n_sections @ 4, at_seq @ 8, state_hash @ 16, n_books @ 32,
        out_hash @ 40, snap_tsc @ 56, boot_id @ 64, snap_utc_ns @ 80, engine_ns @ 88);
    check!("EVT-001.5", SectionHdr, 16, 4; section @ 0, version @ 2, count @ 4, len @ 8, _rsv @ 12);
}

#[test]
fn evt_040_market_layouts() {
    check!("EVT-040.1", MarketHdr, 56, 8; venue @ 0, conn @ 1, _pad @ 2, inst @ 4, ex_ts_ns @ 8, ex_send_ns @ 16,
        hw_rx_ns @ 24, rx_tsc @ 32, sw_rx_ns @ 40, rx_empty_gap_ns @ 48, _rsv @ 52);
    check!("EVT-040.2", Bbo, 96, 8; h @ 0, bid_px @ 56, bid_qty @ 64, ask_px @ 72, ask_qty @ 80, update_id @ 88);
    check!("EVT-040.3", L2Delta, 96, 8; prev_id @ 56, last_id @ 64, px @ 72, qty @ 80, side @ 88, dflags @ 89,
        idx @ 90, _pad @ 92);
    check!("EVT-040.4", Trade, 88, 8; px @ 56, qty @ 64, trade_id @ 72, taker_side @ 80, _pad @ 81);
    check!("EVT-040.5", Funding, 80, 8; rate @ 56, next_funding_ns @ 64, interval_s @ 72, _pad @ 76);
    check!("EVT-040.6", MarkPrice, 64, 8; px @ 56);
    check!("EVT-040.7", IndexPrice, 64, 8; px @ 56);
    check!("EVT-040.8", Liquidation, 80, 8; px @ 56, qty @ 64, side @ 72, _pad @ 73);
}

#[test]
fn evt_050_exec_layouts() {
    check!("EVT-050.1", OrderDecision, 88, 8; trigger_seq @ 0, decided_tsc @ 8, px @ 16, qty @ 24, group @ 32,
        inst @ 36, slot @ 40, r#gen @ 44, target_slot @ 48, reject @ 52, strategy @ 56, venue @ 57, account @ 58,
        action @ 59, side @ 60, tif @ 61, oflags @ 62, reason @ 64, intent_no @ 68, req_px @ 72, link_role @ 80,
        _pad @ 81);
    check!("EVT-050.2", SendReport, 48, 8; decision_seq @ 0, sent_tsc @ 8, slot @ 16, r#gen @ 20, target_slot @ 24,
        target_gen @ 28, venue @ 32, account @ 33, conn @ 34, status @ 35, op @ 36, _pad @ 37, os_errno @ 40,
        _pad2 @ 44);
    check!("EVT-050.3", OrderEvent, 96, 8; venue @ 0, account @ 1, what @ 2, detail @ 3, slot @ 4, r#gen @ 8,
        inst @ 12, px @ 16, qty @ 24, cum_filled @ 32, fee @ 40, fee_asset @ 48, epoch @ 50, reject_code @ 52,
        exch_order_id @ 56, ex_ts_ns @ 64, hw_rx_ns @ 72, rx_tsc @ 80, avg_px @ 88);
    check!("EVT-050.4", PositionReport, 56, 8; venue @ 0, account @ 1, _pad @ 2, inst @ 4, qty @ 8, entry_px @ 16,
        upnl @ 24, liq_px @ 32, ex_ts_ns @ 40, rx_tsc @ 48);
    check!("EVT-050.5", BalanceReport, 40, 8; venue @ 0, account @ 1, asset @ 2, _pad @ 4, free @ 8, locked @ 16,
        ex_ts_ns @ 24, rx_tsc @ 32);
    check!("EVT-050.6", FundingSettle, 56, 8; venue @ 0, account @ 1, _pad @ 2, inst @ 4, amount @ 8, rate @ 16,
        pos_qty @ 24, settle_ts_ns @ 32, rx_tsc @ 40, synth @ 48, _pad2 @ 49);
    check!("EVT-050.7", ExternalFill, 80, 8; venue @ 0, account @ 1, side @ 2, cause @ 3, inst @ 4, qty @ 8, px @ 16,
        fee @ 24, fee_asset @ 32, _pad @ 34, exch_order_id @ 40, trade_id @ 48, ex_ts_ns @ 56, rx_tsc @ 64,
        fill_seq @ 72);
    check!("EVT-050.8", MarginReportHdr, 64, 8; venue @ 0, account @ 1, source @ 2, mflags @ 3, n @ 4, _pad @ 6,
        margin_balance @ 8, maint_margin @ 16, avail_margin @ 24, margin_ratio @ 32, ex_ts_ns @ 40, rx_tsc @ 48,
        _rsv @ 56);
    check!("EVT-050.9", MarginItem, 16, 8; inst @ 0, adl_quantile @ 4, _pad @ 5, liq_px @ 8);
    check!("EVT-050.10", AccountConfigReportHdr, 64, 8; venue @ 0, account @ 1, position_mode @ 2, margin_mode @ 3,
        multi_asset @ 4, source @ 5, n_lev @ 6, mismatches @ 8, rflags @ 12, uid_keys @ 14, _pad @ 15,
        venue_uid @ 16, ex_ts_ns @ 40, rx_tsc @ 48, _rsv @ 56);
    check!("EVT-050.11", LeverageItem, 16, 8; inst @ 0, _pad @ 4, leverage @ 8);
    check!("EVT-050.12", ExecDecision, 104, 8; trigger_seq @ 0, set_seq @ 8, group @ 16, tag @ 20, owner @ 24,
        account @ 25, action @ 26, algo @ 27, origin @ 28, reason @ 29, urgency @ 30, state_from @ 31, state_to @ 32,
        leg_ix @ 33, first_leg_ix @ 34, side @ 35, eflags @ 36, _pad @ 37, detail @ 40, gate @ 44,
        exec_order_id @ 48, _pad2 @ 52, qty @ 56, px @ 64, target_qty @ 72, pos_qty @ 80, inflight_qty @ 88,
        depth_qty @ 96);
}

#[test]
fn evt_060_chain_layouts() {
    for k in [0x0301u16, 0x0302, 0x0303, 0x0304] {
        assert!(kind_info(EventKind(k)).unwrap().optional);
    }
    check!("EVT-060.1", ChainHead, 80, 8; chain @ 0, source @ 1, hflags @ 2, _pad @ 3, block @ 8, block_ts_ns @ 16,
        observed_ts_ns @ 24, rx_tsc @ 32, hash @ 40, finalized @ 72);
    check!("EVT-060.2", PoolQuote, 96, 8; quote_id @ 0, block @ 8, group @ 16, route @ 20, chain @ 24, level @ 25,
        qflags @ 26, _pad @ 27, size @ 32, buy_px @ 40, sell_px @ 48, depth @ 56, gas_cost @ 64, fee @ 72,
        computed_ts_ns @ 80, rx_tsc @ 88);
    check!("EVT-060.3", SwapDecision, 128, 8; intent_id @ 0, quote_id @ 8, trigger_seq @ 16, decided_tsc @ 24,
        amount_in @ 32, min_out @ 40, amount_out @ 48, max_in @ 56, limit_px @ 64, edge @ 72, max_bribe @ 80,
        group @ 88, route @ 92, reject @ 96, hedge_inst @ 100, strategy @ 104, account @ 105, chain @ 106,
        side @ 107, valid_blocks @ 108, exact @ 109, hedge_side @ 110, hedge_px_mode @ 111, hedge_limit @ 112,
        sflags @ 120);
    check!("EVT-060.4", TxLifecycle, 152, 8; intent_id @ 0, state @ 8, builder @ 9, account @ 10, _pad @ 11,
        block @ 16, amount_in @ 24, amount_out @ 32, gas_used @ 40, gas_cost @ 48, tx_hash @ 56, observed_ts_ns @ 88,
        reason @ 96, tflags @ 98, src_mask @ 100, wallet @ 101, _pad2 @ 102, bribe @ 104, rx_tsc @ 112, sig_hi @ 120);
}

#[test]
fn evt_070_system_layouts() {
    check!("EVT-070.1", CmdResult, 144, 8; cmd_id @ 0, cmd_seq @ 16, cmd @ 24, status @ 26, _pad @ 27, detail @ 28,
        engine_epoch @ 32, effect_count @ 36, refdata_version @ 40, risk_version @ 48, params_version @ 56,
        ctl_state_version @ 64, rflags @ 72, _pad2 @ 76, preview @ 80);
    check!("EVT-070.2", CmdPreview, 64, 8; target_account @ 0, other_account @ 1, target_mode @ 2, other_mode @ 3,
        target_kind @ 4, _pad @ 5, affected_cells @ 8, open_orders @ 12, active_caps @ 16, cap_sources @ 20,
        reset_groups @ 24, no_open_ms @ 28, wo_venue @ 32, wo_leg_ix @ 33, _pad2 @ 34, wo_inst @ 36, wo_qty @ 40,
        _rsv @ 48);
    check!("EVT-070.3", ReconcileReport, 104, 8; report_id @ 0, venue @ 8, account @ 9, item @ 10, severity @ 11,
        inst @ 12, asset @ 16, idx @ 18, count @ 20, _pad @ 22, engine_qty @ 24, exch_qty @ 32, diff @ 40,
        exch_ts_ns @ 48, slot @ 56, order_status @ 60, history_checked @ 61, _pad2 @ 62, view_seq @ 64, r#gen @ 72,
        epoch @ 76, _pad3 @ 78, exch_amount @ 80, engine_amount @ 88, sflags @ 96, skipped_inflight @ 98, _pad4 @ 100);
    check!("EVT-070.4", LatencySample, 136, 8; trigger_seq @ 0, ack_seq @ 8, slot @ 16, r#gen @ 20, venue @ 24,
        account @ 25, strategy @ 26, _pad @ 27, src @ 28, ex_ts_ns @ 32, ex_send_ns @ 40, hw_rx_ns @ 48, rx_tsc @ 56,
        proc_tsc @ 64, decided_tsc @ 72, sent_tsc @ 80, ack_hw_rx_ns @ 88, ack_rx_tsc @ 96, ack_ex_ts_ns @ 104,
        sw_rx_ns @ 112, link_role @ 120, action @ 121, _pad2 @ 122, trigger_kind @ 128, _pad3 @ 130);
    check!("EVT-070.5", Note, 80, 8; code @ 0, level @ 4, source @ 5, strategy @ 6, _pad @ 7, group @ 8, inst @ 12,
        v @ 16);
    check!("EVT-070.6", HostProfile, 176, 8; boot_id @ 0, build_git @ 16, _pad @ 36, profile_sha256 @ 40,
        release @ 72, kernel @ 88, tsc_hz_milli @ 120, host_checks @ 128, hugepages_1g_total @ 136,
        hugepages_1g_free @ 140, isolated_cpus @ 144);
    check!("EVT-070.7", ClockStatus, 56, 8; tsc @ 0, realtime_ns @ 8, phc_ns @ 16, tsc_hz_milli @ 24,
        chrony_offset_ns @ 32, max_error_ns @ 40, rms_ns @ 44, calib_err_ns @ 48, source @ 52, state @ 53, _pad @ 54);
    check!("EVT-070.8", SegmentRoll, 48, 8; segment_no @ 0, other_segment_no @ 8, seq_mark @ 16, utc_ns @ 24,
        mark @ 32, reason @ 33, journal_kind @ 34, _pad @ 35, boot_records @ 36, _rsv @ 40);
    check!("EVT-070.9", EngineClock, 32, 8; base_engine_ns @ 0, tsc_hz_milli @ 8, prev_last_seq @ 16, source @ 24,
        _pad @ 25);
    check!("EVT-070.10", Timer, 24, 8; timer_id @ 0, owner @ 4, _pad @ 6, group @ 8, _pad2 @ 12, deadline_tsc @ 16);
    check!("EVT-070.11", ConnState, 24, 8; venue @ 0, conn @ 1, state @ 2, role @ 3, inst @ 4, reason @ 8,
        account @ 12, _pad2 @ 13, rx_tsc @ 16);
}

#[test]
fn evt_090_cap_layouts() {
    check!("EVT-090.1", CapRecord, 48, 8; source @ 0, level @ 1, cflags @ 2, _pad @ 3, scope @ 8, set_seq @ 24,
        params @ 32);
    check!("EVT-090.2", CmdScope, 16, 4; strategy @ 0, venue @ 1, account @ 2, _pad @ 3, group @ 4, inst @ 8,
        _pad2 @ 12);
}

#[test]
fn evt_120_advice_hdr_layout() {
    check!("EVT-120", AdviceHdr, 32, 8; strategy @ 0, account @ 1, local_kind @ 2, group @ 4, len @ 8, _pad @ 10,
        basis_seq @ 16, basis_tsc @ 24);
    assert_eq!(ADVICE_MAX_PAYLOAD, 4096);
    assert_eq!(
        kind_info(EventKind::ADVICE).unwrap().layout,
        Layout::Var {
            prefix: 32,
            max: ADVICE_MAX_PAYLOAD as u32
        }
    );
}

#[test]
fn evt_130_spb1_layouts() {
    check!("EVT-130.1", StrategyParamsHdr, 48, 8; strategy @ 0, account @ 1, kind_id @ 2, schema_id @ 4,
        schema_ver @ 8, source @ 10, _pad @ 11, n_groups @ 12, _pad2 @ 14, version @ 16, generated_at_ns @ 24,
        body_len @ 32, group_ctr_hwm @ 36, schema_digest @ 40);
    check!("EVT-130.2", GroupSpec, 80, 8; id @ 0, asset @ 4, n_legs @ 6, open_allowed @ 7, legs @ 8, _rsv @ 72);
    assert_eq!(size_of::<[LegDef; 4]>(), 64);
    check!("EVT-130.3", LegDef, 16, 4; inst @ 0, venue @ 4, kind @ 5, _pad @ 6, route @ 8, _pad2 @ 12);
}

#[test]
fn evt_150_cmd_envelope_layouts() {
    check!("EVT-150.1", CmdHdr, 128, 8; cmd_id @ 0, issuer @ 16, issued_at_ns @ 40, expires_at_ns @ 48,
        expires_tsc @ 56, peer_uid @ 64, cmd @ 68, cmd_ver @ 70, role @ 72, account @ 73, confirmed @ 74,
        hflags @ 75, payload_len @ 76, cmd_hash @ 80, base_version @ 112, expect_state_version @ 120);
    check!("EVT-150.2", BlobRef, 48, 8; kind @ 0, encoding @ 2, len @ 4, version @ 8, sha256 @ 16);
    assert_eq!(STRATEGY_CMD_MAX_PAYLOAD, 256, "EVT-150.3");
    assert_eq!(WARMUP_MAX_PAYLOAD, 4096, "EVT-150.3");
}

#[test]
fn evt_160_var_prefix_layouts() {
    check!("EVT-160.1", L2SnapHdr, 72, 8; h @ 0, snap_id @ 56, origin @ 64, _pad @ 65, n_bids @ 68, n_asks @ 70);
    check!("EVT-160.2", Level, 16, 8; px @ 0, qty @ 8);
    check!("EVT-160.3", L2BatchHdr, 80, 8; h @ 0, prev_id @ 56, last_id @ 64, n_bids @ 72, n_asks @ 74, bflags @ 76,
        _pad @ 78);
    check!("EVT-160.4", BlobHdr, 72, 8; blob @ 0, scope @ 48, boot @ 64, _pad @ 65);
    check!("EVT-160.5", RawHdr, 24, 8; venue @ 0, conn @ 1, what @ 2, rflags @ 3, stream @ 4, hw_rx_ns @ 8,
        payload_len @ 16, _pad @ 20);
}
