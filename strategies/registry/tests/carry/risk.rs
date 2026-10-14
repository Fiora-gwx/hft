use super::*;
pub fn risk() -> RiskParams {
    let big = Notional(1_000_000_000_000_000);
    let group = GroupLimits {
        max_group_notional: big,
        max_pos_qty: Qty(1_000_000_000_000_000),
        naked_gate: Rate(1_000_000_000),
        naked_gate_min: big,
    };
    let mut classes = FixedVec::new();
    classes.push(PxBand {
        max_px_dev: Rate(100_000_000),
        ioc_max_cross: Rate(50_000_000),
        rr_max_cross: Rate(50_000_000),
    });
    let acct = AcctLimits {
        account: A1,
        max_order_notional: big,
        default_group: group,
        max_account_notional: big,
        account_notional_from_equity: Rate(100_000_000_000),
        max_hedge_notional: big,
        max_reduce_notional: big,
        daily_loss_kill: Rate(1_000_000_000),
        daily_loss_abs: big,
        daily_loss_max_resumes: 1,
        recon_closeonly_notional: big,
        unknown_blocks_open: false,
        funds_buffer: Rate(0),
        margin: MarginLimits {
            margin_stale_ms: 86_400_000,
            ..DEFAULT_MARGIN
        },
    };
    let venues = [V1, V2]
        .into_iter()
        .map(|v| VenueLimits {
            venue: v,
            account: A1,
            max_venue_notional: big,
            inst_bucket_per_s: 1_000,
            backoff_base_ms: 1_000,
            backoff_max_ms: 8_000,
            rr_reserve: Rate(0),
        })
        .collect();
    RiskParams {
        version: 1,
        generated_at_ns: 0,
        author: [0; 16],
        rate_headroom: Rate(0),
        conn_liveness_ms: 3_600_000,
        max_perp_leverage_x100: 2_000,
        l3_max_cross: Rate(0),
        risk_class: PreVec::from_vec(Vec::new()),
        accounts: PreVec::from_vec(vec![acct]),
        venues: PreVec::from_vec(venues),
        classes,
        groups: PreVec::from_vec(Vec::new()),
    }
}
