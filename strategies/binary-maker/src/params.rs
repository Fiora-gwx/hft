//! `BinaryMakerParams` (README "参数") and its group-shape `check`.

use hft_strategy_sdk::{GroupTable, ParamError, RefView};

hft_strategy_sdk::define_params! {
    pub struct BinaryMakerParams(schema_id = 7, version = 9, fields = BinaryMakerFields) {
        /// false: no quotes (resting quotes are cancelled)
        enabled: Bool [default = 0, manual_only, relax = Up],
        /// window length in seconds; the slot pair of the window starting at S is (S / window_s) mod pairs
        window_s: Count [min = 60, max = 86_400, default = 300, relax = None],
        /// shares per quote
        clip_shares: Count [min = 1, max = 100_000, default = 10, relax = Up],
        /// net shares (Up minus Down of the current window) that the long side's next clip, filled
        /// completely, may not exceed
        inv_max_shares: Count [min = 1, max = 1_000_000, default = 100, relax = Up],
        /// lowest bid price quoted, as a fraction of one (1e-9)
        p_min: Rate [min = 0, max = 1_000_000_000, default = 100_000_000, relax = Down],
        /// highest bid price quoted, as a fraction of one (1e-9)
        p_max: Rate [min = 0, max = 1_000_000_000, default = 900_000_000, relax = Up],
        /// minimum implied volatility per square-root second to quote (1e-9; 0.5 bp = 50_000)
        sigma_min: Rate [min = 0, max = 1_000_000_000, default = 50_000, relax = Down],
        /// implied over reference realized volatility (1e-9; 1.39 = 1_390_000_000)
        sigma_scale: Rate [min = 1, max = 10_000_000_000, default = 1_390_000_000, relax = None],
        /// minimum remaining standard deviation to quote, implied sigma times sqrt(seconds left) (1e-9)
        min_rem_sd: Rate [min = 0, max = 1_000_000_000, default = 100_000, relax = Down],
        /// fair-value move since placement (fraction of one, 1e-9) that cancels a quote; 0 = off
        fv_cancel: Rate [min = 0, max = 1_000_000_000, default = 5_000_000, relax = Up],
        /// no new quote on a leg for this long after its fair-value cancel
        fv_cool_ms: DurationMs [min = 0, max = 600_000, default = 2_000, relax = Down],
        /// no quotes this close to the window end
        stop_before_ms: DurationMs [min = 0, max = 3_600_000, default = 60_000, relax = Down],
        /// one-second returns in the realized volatility
        vol_window_s: Count [min = 60, max = 7_200, default = 1_800, relax = None],
        /// one-second returns required before quoting
        vol_min_samples: Count [min = 2, max = 7_200, default = 300, relax = Down],
        /// minimum engine time between two quote changes of one leg
        requote_ms: DurationMs [min = 0, max = 60_000, default = 200, relax = Down],
        /// resting limit of every quote (OMS ages it out)
        max_rest_ms: DurationMs [min = 1_000, max = 3_600_000, default = 360_000, relax = Up],
        /// a bid that would complete Up/Down pairs rests at most at 1 − (other side's average cost) −
        /// pair_margin (fraction of one, 1e-9); negative: pairs may lock at a loss of up to −pair_margin
        pair_margin: Rate [min = -100_000_000, max = 1_000_000_000, default = 10_000_000, relax = Down],
        /// no new quote on a leg for this long after it was cancelled for being undercut; 0 = off
        nochase_ms: DurationMs [min = 0, max = 600_000, default = 0, relax = Down],
        /// fills of one leg that start a streak pause; 0 = off (minimum 2 when on)
        streak_k: Count [min = 0, max = 8, default = 0, relax = Down],
        /// the last `streak_k` fills of a leg within this window pause the leg
        streak_window_ms: DurationMs [min = 0, max = 600_000, default = 10_000, relax = Down],
        /// the paused leg places nothing until this long after its last fill
        streak_pause_ms: DurationMs [min = 0, max = 600_000, default = 10_000, relax = Down],
        /// bought cost of the current window (both legs) at which no new quote is placed; 0 = no limit
        max_window_cost: Notional [min = 0, max = 1_000_000_000_000, default = 0, relax = Up],
        /// a cancel unanswered (no terminal order update) for this long is sent again and the leg is
        /// treated as having no quote
        stuck_reset_ms: DurationMs [min = 1_000, max = 600_000, default = 5_000, since = 4, relax = Down],
        /// true: bids are capped by the strategy's own fair value of the Up token (anchored to the
        /// Up mid, moved by the reference, inventory and the direction forecast); false: v5 rules
        fv_limit: Bool [default = 0, since = 5, relax = Down],
        /// half-life of the fair value's anchor to the Up token mid
        fv_anchor_ms: DurationMs [min = 100, max = 600_000, default = 1_000, since = 5, relax = None],
        /// bids rest at most at the fair value minus fv_edge (fraction of one, 1e-9)
        fv_edge: Rate [min = -100_000_000, max = 100_000_000, default = 0, since = 5, relax = Down],
        /// a resting bid more than this above its fair-value limit is cancelled (fraction of one, 1e-9)
        fv_hyst: Rate [min = 0, max = 100_000_000, default = 5_000_000, since = 5, relax = Up],
        /// fair-value limit shift per unit of (Up − Down) / inv_max_shares (fraction of one, 1e-9)
        inv_skew: Rate [min = 0, max = 500_000_000, default = 30_000_000, since = 5, relax = None],
        /// forecast drift of the fair value: pred_k × forecast × reference sigma × sqrt(30 s) (1e-9)
        pred_k: Rate [min = 0, max = 2_000_000_000, default = 200_000_000, since = 5, relax = None],
        /// forecast weight of the reference level-1 imbalance (bid − ask quantity) / (bid + ask) (1e-9)
        pred_w_imb: Rate [min = -10_000_000_000, max = 10_000_000_000, default = 1_372_490_000, since = 5, relax = None],
        /// forecast weight of the last 5 one-second returns in sigma units, sum / (sigma × sqrt 5) (1e-9)
        pred_w_ret: Rate [min = -10_000_000_000, max = 10_000_000_000, default = 121_290_000, since = 5, relax = None],
        /// forecast weight of the 15 s signed efficiency, sum / sum of absolute one-second returns (1e-9)
        pred_w_ser: Rate [min = -10_000_000_000, max = 10_000_000_000, default = 187_200_000, since = 5, relax = None],
        /// a token whose composite direction score is below −comp_skip is not quoted (its resting
        /// bid is cancelled); score in standard deviations (1e-9); 0 = off; negative: only a token
        /// whose score is above −comp_skip is quoted (directional maker)
        comp_skip: Rate [min = -10_000_000_000, max = 10_000_000_000, default = 0, since = 6, relax = None],
        /// a token whose composite direction score is above comp_dir may hold up to
        /// dir_cap_shares net (instead of inv_max_shares); 1e-9 standard deviations; 0 = off
        comp_dir: Rate [min = 0, max = 10_000_000_000, default = 0, since = 6, relax = None],
        /// net-share cap of a token whose score is above comp_dir
        dir_cap_shares: Count [min = 1, max = 1_000_000, default = 20, since = 6, relax = None],
        /// the composite score is recomputed at most this often (UTC time)
        comp_every_ms: DurationMs [min = 0, max = 60_000, default = 250, since = 6, relax = None],
        /// prints of at least this many shares count in the score's 30 s flow feature
        comp_flow_min_shares: Count [min = 0, max = 1_000_000, default = 50, since = 6, relax = None],
        /// true: the strategy's own resting bids are taken out of the bid sizes of the score's
        /// imbalance features (they are in the venue's public book); false: not (a simulated account)
        comp_excl_own: Bool [default = 1, since = 6, relax = None],
        /// score normalization: mean of the best-bid size imbalance of the token and its complement (1e-9)
        comp_imb1_mu: Rate [min = -10_000_000_000, max = 10_000_000_000, default = 6_035, since = 6, relax = None],
        /// score normalization: standard deviation of the best-bid size imbalance of the token and its complement (1e-9)
        comp_imb1_sd: Rate [min = 1, max = 100_000_000_000, default = 674_103_869, since = 6, relax = None],
        /// score normalization: mean of the 5-cent bid depth imbalance (1e-9)
        comp_imb5_mu: Rate [min = -10_000_000_000, max = 10_000_000_000, default = 3_815, since = 6, relax = None],
        /// score normalization: standard deviation of the 5-cent bid depth imbalance (1e-9)
        comp_imb5_sd: Rate [min = 1, max = 100_000_000_000, default = 382_699_882, since = 6, relax = None],
        /// score normalization: mean of the token mid change over 30 s (fraction of one) (1e-9)
        comp_mom30_mu: Rate [min = -10_000_000_000, max = 10_000_000_000, default = -1_799, since = 6, relax = None],
        /// score normalization: standard deviation of the token mid change over 30 s (fraction of one) (1e-9)
        comp_mom30_sd: Rate [min = 1, max = 100_000_000_000, default = 165_941_588, since = 6, relax = None],
        /// score normalization: mean of the token mid change over 60 s (fraction of one) (1e-9)
        comp_mom60_mu: Rate [min = -10_000_000_000, max = 10_000_000_000, default = -5_917, since = 6, relax = None],
        /// score normalization: standard deviation of the token mid change over 60 s (fraction of one) (1e-9)
        comp_mom60_sd: Rate [min = 1, max = 100_000_000_000, default = 224_952_723, since = 6, relax = None],
        /// score normalization: mean of the reference return over 15 s in implied-sigma units, signed for the token (1e-9)
        comp_bret15_mu: Rate [min = -10_000_000_000, max = 10_000_000_000, default = -28_103, since = 6, relax = None],
        /// score normalization: standard deviation of the reference return over 15 s in implied-sigma units, signed for the token (1e-9)
        comp_bret15_sd: Rate [min = 1, max = 100_000_000_000, default = 919_829_667, since = 6, relax = None],
        /// score normalization: mean of the reference return over 60 s in implied-sigma units, signed for the token (1e-9)
        comp_bret60_mu: Rate [min = -10_000_000_000, max = 10_000_000_000, default = -24_869, since = 6, relax = None],
        /// score normalization: standard deviation of the reference return over 60 s in implied-sigma units, signed for the token (1e-9)
        comp_bret60_sd: Rate [min = 1, max = 100_000_000_000, default = 968_755_915, since = 6, relax = None],
        /// score normalization: mean of the signed log1p of the 30 s net taker flow in large prints (shares) (1e-9)
        comp_flow30_mu: Rate [min = -10_000_000_000, max = 10_000_000_000, default = -49_610, since = 6, relax = None],
        /// score normalization: standard deviation of the signed log1p of the 30 s net taker flow in large prints (shares) (1e-9)
        comp_flow30_sd: Rate [min = 1, max = 100_000_000_000, default = 6_260_786_978, since = 6, relax = None],
        /// score normalization: mean of the best-bid change over 1 s in cents (1e-9)
        comp_bdrop1_mu: Rate [min = -10_000_000_000, max = 10_000_000_000, default = 1_550_071, since = 6, relax = None],
        /// score normalization: standard deviation of the best-bid change over 1 s in cents (1e-9)
        comp_bdrop1_sd: Rate [min = 1, max = 100_000_000_000, default = 3_031_529_770, since = 6, relax = None],
        /// no quoting while the previous UTC minute's median of the per-second 90th percentile of the
        /// slot books' and trades' feed lag (kernel receive − venue time) is at least this (ns); 0 = off
        load_gate_ns: Count [min = 0, max = 1_000_000_000, default = 0, since = 6, relax = None],
        /// no quoting while the previous UTC minute's median of the per-second median feed lag is
        /// above this (ns); 0 = off
        mdslow_gate_ns: Count [min = 0, max = 1_000_000_000, default = 0, since = 6, relax = None],
        /// true: a token whose composite score is above comp_dir completes pairs without the pair-cost
        /// limit (two-mode rule: the favoured side pairs off inventory left on the other side)
        dir_pair_free: Bool [default = 0, since = 7, relax = None],
        /// a token whose composite score is above tilt_z quotes tilt_clip_shares per order instead of
        /// clip_shares (the side the score leans to is bid in larger size); 0 = off
        tilt_z: Rate [min = 0, max = 10_000_000_000, default = 0, since = 8, relax = None],
        /// order size of a token whose score is above tilt_z; 0 = off
        tilt_clip_shares: Count [min = 0, max = 100_000, default = 0, since = 8, relax = None],
        /// last-minute stop-loss: from stop_before_ms until late_hedge_ms before the window end only
        /// pairing bids are placed, by the leg whose score is above late_hedge_z (the signal is against
        /// the net position), at the best bid without the pair-cost limit; 0 = off
        late_hedge_ms: DurationMs [min = 0, max = 600_000, default = 0, since = 8, relax = None],
        /// score the pairing leg needs in the last-minute stop-loss
        late_hedge_z: Rate [min = 0, max = 10_000_000_000, default = 0, since = 8, relax = None],
        /// opening rules (schema v9; a leg "opens" when it is not short, i.e. a fill would raise the
        /// net; pairing bids are never affected; a token the score favours (z > comp_dir) is exempt):
        /// no opening while the Up mid travelled less than mm_min_path over the last 60 s (quiet,
        /// drifting market); 0 = off
        mm_min_path: Rate [min = 0, max = 100_000_000_000, default = 0, since = 9, relax = None],
        /// no opening on a leg whose best-bid size is above this many shares (long queue); 0 = off
        mm_max_bq: Count [min = 0, max = 100_000_000, default = 0, since = 9, relax = None],
        /// in the first early_ms of the window the opening net cap is early_cap_shares; 0 = off
        early_ms: DurationMs [min = 0, max = 600_000, default = 0, since = 9, relax = None],
        /// opening net cap in the first early_ms
        early_cap_shares: Count [min = 0, max = 1_000_000, default = 0, since = 9, relax = None],
        /// no opening on a leg whose best bid is below this price (cheap side); 0 = off
        open_min_px: Rate [min = 0, max = 1_000_000_000, default = 0, since = 9, relax = None],
        /// no opening on a leg whose composite score is below -open_skip_z (it may still pair); 0 = off
        open_skip_z: Rate [min = 0, max = 10_000_000_000, default = 0, since = 9, relax = None],
    }
}

/// `ParamError::GroupShape` codes.
pub mod shape {
    /// Not an odd number of groups, at least three (pairs of slots plus the reference).
    pub const COUNT: u16 = 1;
    /// A group does not have exactly one leg.
    pub const LEGS: u16 = 2;
    /// A slot leg is not on the venue of the first slot, or the reference leg is.
    pub const VENUE: u16 = 3;
    /// A leg's inst is not in refdata.
    pub const INST: u16 = 4;
}

/// `ParamError::Invalid` codes.
pub mod invalid {
    /// `p_min > p_max`.
    pub const PRICE_BAND: u16 = 1;
}

impl BinaryMakerParams {
    /// Groups: `2 × pairs` single-leg slot groups on one venue (Up, Down of pair 0, then pair 1, …)
    /// and a last single-leg reference group on another venue.
    pub fn check(&self, groups: &GroupTable, rd: &RefView<'_>) -> Result<(), ParamError> {
        let n = groups.len();
        let d = self.defaults();
        if d.p_min > d.p_max {
            return Err(ParamError::Invalid {
                field_ix: 4,
                code: invalid::PRICE_BAND,
            });
        }
        if n < 3 || n.is_multiple_of(2) {
            return Err(ParamError::GroupShape {
                gix: 0,
                code: shape::COUNT,
            });
        }
        let mut slot_venue = None;
        for (gix, g) in groups.iter() {
            let err = |code| ParamError::GroupShape { gix, code };
            if g.n_legs != 1 {
                return Err(err(shape::LEGS));
            }
            let l = g.legs[0];
            if rd.try_inst(l.inst).is_none() {
                return Err(err(shape::INST));
            }
            let reference = gix + 1 == n;
            let v = *slot_venue.get_or_insert(l.venue);
            if (l.venue == v) == reference {
                return Err(err(shape::VENUE));
            }
        }
        Ok(())
    }
}
