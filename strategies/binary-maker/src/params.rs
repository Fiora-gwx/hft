//! `BinaryMakerParams` (README "参数") and its group-shape `check`.

use hft_strategy_sdk::{GroupTable, ParamError, RefView};

hft_strategy_sdk::define_params! {
    pub struct BinaryMakerParams(schema_id = 7, version = 48, fields = BinaryMakerFields) {
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
        /// inventory absorption (schema v10): of a net position on the other token, the first
        /// absorb_shares count as directional; beyond them a token skipped by comp_skip still bids to
        /// pair the excess (pairing only, pair-cost limit as usual); 0 = off
        absorb_shares: Count [min = 0, max = 1_000_000, default = 0, since = 10, relax = None],
        /// (schema v11) no opening on a leg whose best bid is at or above this price (expensive side;
        /// same exemptions as the other opening rules); 0 = off
        open_max_px: Rate [min = 0, max = 1_000_000_000, default = 0, since = 11, relax = None],
        /// pairing priority: until pair_stop_ms before the window end a leg that is short bids to pair
        /// (pair-cost limit as usual) regardless of the volatility and venue-load gates, the fair-value
        /// limit and cancel, the cooldowns and the streak pause, also inside stop_before_ms; 0 = off
        pair_stop_ms: DurationMs [min = 0, max = 600_000, default = 0, since = 11, relax = None],
        /// (schema v12) pairing priority only while the window is younger than this (early pairs
        /// earn, late ones are adversely selected); 0 = the whole window
        pair_prio_age_ms: DurationMs [min = 0, max = 600_000, default = 0, since = 12, relax = None],
        /// from this window age on, a pairing bid uses pair_late_margin instead of pair_margin (a
        /// favoured token with dir_pair_free and the last-minute stop-loss are not affected); 0 = off
        pair_late_ms: DurationMs [min = 0, max = 600_000, default = 0, since = 12, relax = None],
        /// pair margin from pair_late_ms on (fraction of one; 1 = no pairing bids)
        pair_late_margin: Rate [min = -100_000_000, max = 1_000_000_000, default = 0, since = 12, relax = None],
        /// (schema v13) true: from pair_late_ms on the favoured token's pairing bid (dir_pair_free) also
        /// uses pair_late_margin (no late pairing at any price)
        pair_late_all: Bool [default = 0, since = 13, relax = None],
        /// (schema v14) early fast phase: while the window is younger than fast_until_ms the quote
        /// change interval is fast_requote_ms and, with fast_free, no no-chase cooldown and no streak
        /// pause (the fair-value cooldown stays); 0 = off
        fast_until_ms: DurationMs [min = 0, max = 600_000, default = 0, since = 14, relax = None],
        /// quote change interval in the fast phase
        fast_requote_ms: DurationMs [min = 0, max = 60_000, default = 0, since = 14, relax = None],
        /// true: no no-chase cooldown and no streak pause in the fast phase
        fast_free: Bool [default = 0, since = 14, relax = None],
        /// (schema v15) true: the fast phase applies only to a leg that is short (its bid pairs);
        /// opening legs keep requote_ms, the no-chase cooldown and the streak pause
        fast_pair_only: Bool [default = 0, since = 15, relax = None],
        /// (schema v16) from this window age on the leg the composite score favours (comp_dir) uses
        /// the fast-phase settings (fast_requote_ms, fast_free); 0 = off
        fast_dir_from_ms: DurationMs [min = 0, max = 600_000, default = 0, since = 16, relax = None],
        /// (schema v17) a leg that is not short bids this many cents below the best bid; 0 = at it
        bid_offset_c: Count [min = 0, max = 20, default = 0, since = 17, relax = None],
        /// (schema v18) a leg that is not short does not quote while its best-bid size over that
        /// plus the other token's best-bid size (the mirrored ask) is below this (1e-9); 0 = off
        open_min_imb: Rate [min = 0, max = 1_000_000_000, default = 0, since = 18, relax = None],
        /// (schema v19) bid this many cents above the best bid while that stays below the best ask
        /// (a new level at the front of the queue); with bid_improve_short only for a short leg; 0 = off
        bid_improve_c: Count [min = 0, max = 5, default = 0, since = 19, relax = None],
        /// true: bid_improve_c applies only to a leg that is short
        bid_improve_short: Bool [default = 0, since = 19, relax = None],
        /// (schema v20) true: once one token's net exceeds inv_max_shares in a window (a directional
        /// position), the other token is not favoured for the rest of that window
        dir_lock: Bool [default = 0, since = 20, relax = None],
        /// favoured leg that is not short does not quote while its best bid is at or above this
        /// (fraction of one, 1e-9); 0 = off
        fav_max_px: Rate [min = 0, max = 1_000_000_000, default = 0, since = 20, relax = None],
        /// favoured pairing (dir_pair_free) may lock pairs at a cost up to 1 + this (1e-9); 0 = no limit
        fav_pair_cap: Rate [min = 0, max = 1_000_000_000, default = 0, since = 20, relax = None],
        /// (schema v21) one direction per phase: the window splits at this age into [0, it) and
        /// [it, end); in each phase the first token the composite score favours is the only one
        /// favoured until the phase ends (one reversal per window at most); 0 = off; the window
        /// length = one phase
        dir_phase_ms: DurationMs [min = 0, max = 600_000, default = 0, since = 21, relax = None],
        /// (schema v22, research export) true: the composite score is kept current while the
        /// strategy does not quote too (every window, every second of it); decisions read it only
        /// while quoting
        comp_always: Bool [default = 0, since = 22, relax = None],
        /// (schema v23) a token is favoured only while its score is at most this (1e-9 standard
        /// deviations; over-extended signals are weaker); 0 = no upper bound
        comp_dir_max: Rate [min = 0, max = 10_000_000_000, default = 0, since = 23, relax = None],
        /// a token is favoured only within this long after its score last rose above comp_dir in
        /// the window (a fresh signal); 0 = no limit
        dir_fresh_ms: DurationMs [min = 0, max = 600_000, default = 0, since = 23, relax = None],
        /// (schema v24, mature-level admission) a leg that is not short places a new bid only while
        /// the best bid's displayed size less our own is at least this many shares; 0 = off
        open_min_bq: Count [min = 0, max = 100_000_000, default = 0, since = 24, relax = None],
        /// and only while the best-bid price has been unchanged for this long; 0 = off
        open_min_level_ms: DurationMs [min = 0, max = 600_000, default = 0, since = 24, relax = None],
        /// the same two thresholds for a short (pairing) leg; 0 = off
        pair_min_bq: Count [min = 0, max = 100_000_000, default = 0, since = 24, relax = None],
        pair_min_level_ms: DurationMs [min = 0, max = 600_000, default = 0, since = 24, relax = None],
        /// a resting bid of a leg that is not short at the best bid is cancelled when the level's
        /// displayed size less our own falls below this many shares; 0 = off
        leave_bq: Count [min = 0, max = 100_000_000, default = 0, since = 24, relax = None],
        /// true: the favoured leg obeys the opening rules too (no exemption)
        fav_obey_open: Bool [default = 0, since = 24, relax = None],
        /// true: an undercut cancels only when the new best bid is mature (size at least
        /// undercut_min_bq, or unchanged for undercut_min_level_ms, or we have been below it for
        /// hold_below_max_ms)
        hold_thin_undercut: Bool [default = 0, since = 24, relax = None],
        undercut_min_bq: Count [min = 0, max = 100_000_000, default = 0, since = 24, relax = None],
        undercut_min_level_ms: DurationMs [min = 0, max = 600_000, default = 0, since = 24, relax = None],
        hold_below_max_ms: DurationMs [min = 0, max = 600_000, default = 0, since = 24, relax = None],
        /// a fill whose price level disappears within 1 s (swept) pauses the leg's opening and adding
        /// for this long; pairing during the pause is limited to cost 1 + swept_pair_cap; 0 = off
        swept_pause_ms: DurationMs [min = 0, max = 600_000, default = 0, since = 24, relax = None],
        swept_pair_cap: Rate [min = 0, max = 1_000_000_000, default = 0, since = 24, relax = None],
        /// true: the complement's best bid or ask moving up cancels this leg's bid when it is not
        /// short and pauses it for oth_up_cool_ms
        oth_up_cancel: Bool [default = 0, since = 24, relax = None],
        oth_up_cool_ms: DurationMs [min = 0, max = 600_000, default = 0, since = 24, relax = None],
        /// (schema v25, rule 12 settlement-window liquidity) in the last settle_ms of the window, while
        /// the main rules have no target, rest a post-only bid at the best bid of the token the locked
        /// settlement-average probability favours (held to settlement); 0 = off
        settle_ms: DurationMs [min = 0, max = 60_000, default = 0, since = 25, relax = None],
        /// lowest locked-average probability of the favoured token to bid (1e-9)
        settle_p_min: Rate [min = 0, max = 1_000_000_000, default = 950_000_000, since = 25, relax = None],
        /// price band of the module's bid, lowest and highest (fraction of one, 1e-9)
        settle_bid_lo: Rate [min = 0, max = 1_000_000_000, default = 900_000_000, since = 25, relax = None],
        settle_bid_hi: Rate [min = 0, max = 1_000_000_000, default = 970_000_000, since = 25, relax = None],
        /// shares per module bid
        settle_clip_shares: Count [min = 1, max = 100_000, default = 20, since = 25, relax = None],
        /// shares the module buys per window at most (a new bid needs bought + clip within it)
        settle_max_shares: Count [min = 0, max = 1_000_000, default = 60, since = 25, relax = None],
        /// multiplier of the reference realized sigma in the probability (1e-9; 1.3 = 1_300_000_000)
        settle_vol_mult: Rate [min = 1, max = 10_000_000_000, default = 1_300_000_000, since = 25, relax = None],
        /// a module bid is cancelled when its token's probability falls below this (1e-9)
        settle_cancel_p: Rate [min = 0, max = 1_000_000_000, default = 900_000_000, since = 25, relax = None],
        /// no new module bid before this many seconds of the settlement average are locked
        settle_min_lock_s: Count [min = 0, max = 60, default = 5, since = 25, relax = None],
        /// no new module bid while the expected relative move of the average against the strike is
        /// below this in absolute value (1e-9; 0.5 bp = 50_000)
        settle_mu_min: Rate [min = 0, max = 1_000_000_000, default = 50_000, since = 25, relax = None],
        /// minimum engine time between two changes of a module bid of one leg
        settle_requote_ms: DurationMs [min = 0, max = 60_000, default = 200, since = 25, relax = None],
        /// true: bid one cent above the best bid when the spread is at least two cents
        settle_improve: Bool [default = 0, since = 25, relax = None],
        /// (schema v26) no module bid in the last this-long of the window (resting ones are
        /// cancelled); 0 = to the end
        settle_stop_ms: DurationMs [min = 0, max = 60_000, default = 0, since = 26, relax = None],
        /// (schema v27, phased direction) a token can be favoured (directional holding, dir_cap_shares,
        /// dir_pair_free) only from this window age on; before it the score only skips and times
        /// pairing; 0 = from the start
        dir_from_ms: DurationMs [min = 0, max = 600_000, default = 0, since = 27, relax = None],
        /// lazy pairing: a short leg whose score is below pair_eager_z bids pair_lazy_c cents below
        /// the best bid (still under the pair-cost limit); at or above it, it bids at the best bid
        pair_eager_z: Rate [min = -10_000_000_000, max = 10_000_000_000, default = 0, since = 27, relax = None],
        pair_lazy_c: Count [min = 0, max = 20, default = 0, since = 27, relax = None],
        /// (schema v28, first-leg rules) public sweep gate: a fall of a token's best bid by at least
        /// this many cents cancels its non-short bid and pauses it for mkt_drop_pause_ms; 0 = off
        mkt_drop_min_c: Count [min = 0, max = 20, default = 0, since = 28, relax = None],
        mkt_drop_pause_ms: DurationMs [min = 0, max = 600_000, default = 0, since = 28, relax = None],
        /// true: the fall counts only with a trade on either token in the 100 ms before it
        mkt_drop_need_trade: Bool [default = 0, since = 28, relax = None],
        /// short pairing window: a short leg pairs only within this long after the long leg's last
        /// fill (then the net is held), unless the unpaired risk exceeds pair_risk_usd; 0 = off
        pair_window_ms: DurationMs [min = 0, max = 600_000, default = 0, since = 28, relax = None],
        /// unpaired risk (net shares x the long leg's average cost) above which pairing has no time
        /// limit; 0 = never
        pair_risk_usd: Notional [min = 0, max = 1_000_000_000_000, default = 0, since = 28, relax = None],
        /// no opening or adding (a leg that is not short) before this window age; 0 = off
        open_from_ms: DurationMs [min = 0, max = 600_000, default = 0, since = 28, relax = None],
        /// (schema v29, dip bids) true: a leg that is not short and not favoured opens only through
        /// a dip bid: after its token rose (30 s mid change at least dip_mom_min) and the score turned
        /// (at most dip_z_max), for dip_ttl_ms it bids below the best bid by a depth set by the
        /// token's recent price range; otherwise it does not open
        dip_mode: Bool [default = 0, since = 29, relax = None],
        dip_mom_min: Rate [min = 0, max = 1_000_000_000, default = 0, since = 29, relax = None],
        dip_z_max: Rate [min = -10_000_000_000, max = 10_000_000_000, default = 0, since = 29, relax = None],
        dip_ttl_ms: DurationMs [min = 0, max = 600_000, default = 0, since = 29, relax = None],
        /// depth = dip_depth_k x the token mid's high-low range over the last dip_range_s seconds,
        /// in cents, clamped to [dip_depth_min_c, dip_depth_max_c]
        dip_depth_k: Rate [min = 0, max = 10_000_000_000, default = 0, since = 29, relax = None],
        dip_range_s: Count [min = 1, max = 120, default = 30, since = 29, relax = None],
        dip_depth_min_c: Count [min = 0, max = 50, default = 1, since = 29, relax = None],
        dip_depth_max_c: Count [min = 0, max = 50, default = 5, since = 29, relax = None],
        /// (schema v30) true: the dip bid goes to the token that FELL (30 s mid change at most
        /// -dip_mom_min) once its score turned up (at least dip_z_min): buy the loser on a predicted
        /// reversal, pair later
        dip_falling: Bool [default = 0, since = 30, relax = None],
        dip_z_min: Rate [min = -10_000_000_000, max = 10_000_000_000, default = 0, since = 30, relax = None],
        /// (schema v31) chop gate: dip bids only while the mean efficiency ratio of the Up mid
        /// (|end - start| / path, 30..240 s of each window) over the last chop_k windows is at most
        /// chop_er_max (a choppy, mean-reverting market); 0 = off
        chop_k: Count [min = 0, max = 24, default = 0, since = 31, relax = None],
        chop_er_max: Rate [min = 0, max = 1_000_000_000, default = 0, since = 31, relax = None],
        /// a dip bid needs the leg's window holding plus one clip to stay within this many shares; 0 = no cap
        dip_max_shares: Count [min = 0, max = 1_000_000, default = 0, since = 31, relax = None],
        /// (schema v32) stop-loss pairing: from this window age until stop_before_ms, a short leg whose
        /// score is above rescue_z while the held leg's best bid is at most rescue_px pairs at the best
        /// bid, without the pair-cost limit and the volatility gate; once it bid, the window opens
        /// nothing more; 0 = off
        rescue_ms: DurationMs [min = 0, max = 600_000, default = 0, since = 32, relax = None],
        rescue_z: Rate [min = -10_000_000_000, max = 10_000_000_000, default = 0, since = 32, relax = None],
        rescue_px: Rate [min = 0, max = 1_000_000_000, default = 0, since = 32, relax = None],
        /// a stop-loss pair may lock at a cost up to 1 + rescue_cap (fraction of one, 1e-9); 0 = no limit
        rescue_cap: Rate [min = 0, max = 1_000_000_000, default = 0, since = 32, relax = None],
        /// (schema v32) rule 12 price source: 0 = the Binance reference; 1 = Chainlink BTC/USD pushed
        /// by InjectWarmup (local_kind 1, README "Chainlink"): strike, locked seconds and the current
        /// price from Chainlink, σ still from the reference; without a fresh Chainlink price no bids
        settle_src: Count [min = 0, max = 1, default = 0, since = 32, relax = None],
        /// rule 12 bids only while the projected settlement average leads the strike by at least this
        /// many US dollars towards the token (Up above, Down below); a bid is cancelled below half; 0 = off
        settle_lead_usd: Count [min = 0, max = 100_000, default = 0, since = 32, relax = None],
        /// rule 12 bids at most at the token's probability less this (fraction of one, 1e-9) and
        /// cancels a bid above the probability; 0 = off
        settle_edge: Rate [min = 0, max = 1_000_000_000, default = 0, since = 32, relax = None],
        /// (schema v33) pressure block (Albers et al. 2025 "reversal"): a leg that is not short opens
        /// nothing (and cancels its opening bid) while its own book's L1 imbalance (bid qty - ask qty) /
        /// (bid qty + ask qty) is below ofi_obi_max and its order-flow imbalance (Cont-Kukanov-Stoikov,
        /// own best bid / ask) over the last ofi_win_ms is at most 0; 0 = off
        ofi_win_ms: DurationMs [min = 0, max = 25_000, default = 0, since = 33, relax = None],
        ofi_obi_max: Rate [min = -1_000_000_000, max = 1_000_000_000, default = 0, since = 33, relax = None],
        /// (schema v34) chop gate scope: true = the chop gate (chop_k, chop_er_max) holds back every
        /// opening of a leg that is not short, not only dip bids
        chop_all: Bool [default = 0, since = 34, relax = None],
        /// (schema v34) trend gate: a leg that is not short opens only while the mean efficiency ratio
        /// over the last chop_k windows is at least chop_er_min (needs chop_k > 0); 0 = off
        chop_er_min: Rate [min = 0, max = 1_000_000_000, default = 0, since = 34, relax = None],
        /// (schema v34) low-volatility band: implied sigma in [sigma_lo, sigma_min) still quotes, but a
        /// leg that is not short opens only while the chop mean is at most lo_er_max (0 = no chop
        /// condition) and, with lo_obi_on, its own L1 imbalance is at least lo_obi_min; 0 = off
        sigma_lo: Rate [min = 0, max = 1_000_000_000, default = 0, since = 34, relax = None],
        lo_er_max: Rate [min = 0, max = 1_000_000_000, default = 0, since = 34, relax = None],
        lo_obi_on: Bool [default = 0, since = 34, relax = None],
        lo_obi_min: Rate [min = -1_000_000_000, max = 1_000_000_000, default = 0, since = 34, relax = None],
        /// (schema v35) trade-activity gate: a leg that is not short opens only while the window's two
        /// tokens printed at least trd_min trades over the last trd_win_ms (a drop on active selling
        /// reverts more often than a quiet repricing); history shorter than trd_win_ms does not block; 0 = off
        trd_win_ms: DurationMs [min = 0, max = 25_000, default = 0, since = 35, relax = None],
        trd_min: Count [min = 0, max = 1_000_000, default = 0, since = 35, relax = None],
        /// (schema v36) A/B sizing: above 1, windows with an odd index (window start / window_s) scale
        /// clip_shares, inv_max_shares, dip_max_shares and early_cap_shares by ab_scale (the fair-value
        /// skew sees the net scaled back); 0 or 1 = off
        ab_scale: Count [min = 0, max = 4, default = 0, since = 36, relax = None],
        /// (schema v37) Coinbase guard: the Up fair-value move implied by the Coinbase mid over the
        /// last cb_win_ms (InjectWarmup local_kind 3 samples through the digital delta of the Up mid)
        /// against a leg by at least cb_shock (fraction of one) blocks and cancels that leg's opening
        /// bids for cb_hold_ms (with cb_pair also its pairing bids); 0 = off
        cb_win_ms: DurationMs [min = 0, max = 10_000, default = 0, since = 37, relax = None],
        cb_shock: Rate [min = 0, max = 1_000_000_000, default = 0, since = 37, relax = None],
        cb_hold_ms: DurationMs [min = 0, max = 60_000, default = 0, since = 37, relax = None],
        cb_pair: Bool [default = 0, since = 37, relax = None],
        /// (schema v38, research) fair-value reference fusion: the reference mid the fair value and
        /// its anchors read is the first reference's mid + ref_cb_w × (the second reference's mid in
        /// the first's units − the first's mid), the unit ratio a running mean (1/64 per quote) of the
        /// two mids; needs the second (Coinbase) reference group; 0 = off (the first reference alone)
        ref_cb_w: Rate [min = 0, max = 1_000_000_000, default = 0, since = 38, relax = None],
        /// auxiliary groups after the reference groups, in this order: bit 0 = a perpetual
        /// reference (recorded only), bit 1 = a Chainlink group (Index = price, Mark = 60 s TWAP;
        /// replaces the InjectWarmup samples), bit 2 = a Coinbase perpetual reference (rule 15
        /// trigger source, schema v45); 0 = none (schema v41; v39/v40 belong to another line)
        ref_aux: Count [min = 0, max = 7, default = 0, since = 41, relax = None],
        /// rule 14 (fill-conditional EV, README "规则" 14): the model alone decides the bids of the
        /// current window; needs the Chainlink group (ref_aux bit 1); false: rules 1–13
        fc_on: Bool [default = 0, since = 41, relax = Down],
        /// rule 14 tail factor s of sigma (1e-9)
        fc_s: Rate [min = 0, max = 10_000_000_000, default = 1_000_000_000, since = 41, relax = None],
        /// rule 14 adverse move per fill lambda, basis points (1e-9)
        fc_lam: Rate [min = -10_000_000_000, max = 10_000_000_000, default = 100_000_000, since = 41, relax = None],
        /// rule 14 fixed cost per fill c0, cents (1e-9)
        fc_c0: Rate [min = -50_000_000_000, max = 50_000_000_000, default = 1_000_000_000, since = 41, relax = None],
        /// rule 14 price-dependent cost c1 (times |p − 50| / 50), cents (1e-9)
        fc_c1: Rate [min = -50_000_000_000, max = 50_000_000_000, default = 0, since = 41, relax = None],
        /// rule 14 weight w of the model probability against the token mid (1e-9)
        fc_w: Rate [min = 0, max = 1_000_000_000, default = 500_000_000, since = 41, relax = None],
        /// rule 14 post a bid when its score exceeds this, cents (1e-9)
        fc_m_post: Rate [min = -50_000_000_000, max = 50_000_000_000, default = 0, since = 41, relax = None],
        /// rule 14 cancel a resting bid whose score falls below this, cents (1e-9)
        fc_m_cancel: Rate [min = -50_000_000_000, max = 50_000_000_000, default = 0, since = 41, relax = None],
        /// rule 14 pairing margin, cents (1e-9): the short leg is bought only by a pair bid at
        /// most 100 − (long leg's average cost) − fc_m_pair; negative = no pairing
        fc_m_pair: Rate [min = -1_000_000_000, max = 50_000_000_000, default = -1_000_000_000, since = 41, relax = None],
        /// rule 14 implied-sigma samples (one per second) whose median is sigma
        fc_iv_n: Count [min = 5, max = 600, default = 60, since = 41, relax = None],
        /// rule 14 parameter version (the refit's tag; audit only, no effect)
        fc_ver: Count [min = 0, max = 4_000_000_000, default = 0, since = 41, relax = None],
        /// rule 15 (lead quote, README "规则" 15) look-back of the perpetual mid move; 0 = off;
        /// needs the perpetual group (ref_aux bit 0)
        lead_w_ms: DurationMs [min = 0, max = 2_000, default = 0, since = 42, relax = None],
        /// rule 15 trigger: |perpetual mid return over lead_w_ms| at least this (1e-9 of one;
        /// 1 bp = 100_000); 0 = off
        lead_x_bp: Rate [min = 0, max = 100_000_000, default = 0, since = 42, relax = None],
        /// rule 15 a lead bid not fully filled after this long is cancelled
        lead_life_ms: DurationMs [min = 100, max = 60_000, default = 3_000, since = 42, relax = None],
        /// rule 15 minimum time between two triggers in the same direction
        lead_cool_ms: DurationMs [min = 0, max = 60_000, default = 2_000, since = 42, relax = None],
        /// rule 15 shares per lead bid
        lead_shares: Count [min = 1, max = 1_000, default = 5, since = 42, relax = None],
        /// rule 15 perp cancel look-back; 0 = off
        lead_pc_w_ms: DurationMs [min = 0, max = 2_000, default = 0, since = 42, relax = None],
        /// rule 15 perp cancel: a move at least this against a token (1e-9 of one) cancels its lead
        /// bid; 0 = off
        lead_pc_x_bp: Rate [min = 0, max = 100_000_000, default = 0, since = 42, relax = None],
        /// rule 15 perp cancel blocks rule-14 entries on the hurt token for this long
        lead_pc_block_ms: DurationMs [min = 0, max = 60_000, default = 1_000, since = 42, relax = None],
        /// rule 15 minimum time between two perp cancels in the same direction
        lead_pc_cool_ms: DurationMs [min = 0, max = 60_000, default = 1_000, since = 42, relax = None],
        /// true: rule 15 only — no other opening rule (rules 1–14, pairing, dip) places anything;
        /// inventory is held to settlement
        lead_only: Bool [default = 0, since = 42, relax = Down],
        /// rule 15 trigger sources (bits): 1 = the perpetual reference, 2 = the fair-value (spot)
        /// reference, 4 = the second (Coinbase spot) reference, 8 = the Coinbase perpetual
        /// reference (ref_aux bit 2) (4 and 8: schema v45); each source and direction has its own
        /// cool-down
        lead_src: Count [min = 0, max = 15, default = 1, since = 43, relax = None],
        /// rule 15 bid price: 0 = best bid + 1 cent; 1 = the model price (mid + lead_k × Delta ×
        /// |move| − lead_m, between 1 and lead_maxtick cents above the bid, below the ask)
        lead_price: Count [min = 0, max = 1, default = 0, since = 43, relax = None],
        /// rule 15 model price margin, cents (1e-9)
        lead_m: Rate [min = -10_000_000_000, max = 50_000_000_000, default = 500_000_000, since = 43, relax = None],
        /// rule 15 model price: at most this many cents above the best bid
        lead_maxtick: Count [min = 1, max = 50, default = 3, since = 43, relax = None],
        /// rule 15 model price: repricing factor k of the expected move (1e-9)
        lead_k: Rate [min = 0, max = 10_000_000_000, default = 1_000_000_000, since = 43, relax = None],
        /// rule 15 places no lead bid below this price (fraction of one, 1e-9; 0 = off)
        lead_p_min: Rate [min = 0, max = 1_000_000_000, default = 0, since = 44, relax = None],
        /// rule 15 confirmation of a lead bid placed by a Coinbase trigger (lead_src bits 4, 8):
        /// cancelled this long after its place unless the perpetual or the fair-value (spot)
        /// reference has moved lead_cf_x_bp in its direction since lead_w_ms before the place;
        /// 0 = off (such a bid lives like any other)
        lead_cf_ms: DurationMs [min = 0, max = 10_000, default = 0, since = 45, relax = None],
        /// rule 15 confirmation: the confirming move (1e-9 of one; 1 bp = 100_000)
        lead_cf_x_bp: Rate [min = 0, max = 100_000_000, default = 50_000, since = 45, relax = None],
        /// rule 17 (fair-gap lead, schema v46): look-back of the perpetual move that confirms an
        /// undervalued token; 0 (or fvl_x_bp 0) = rule 17 off
        fvl_w_ms: DurationMs [min = 0, max = 10_000, default = 0, since = 46, relax = None],
        /// rule 17: the perpetual mid return over fvl_w_ms in the token's favour is at least this
        /// (1e-9 of one; 1 bp = 100_000)
        fvl_x_bp: Rate [min = 0, max = 100_000_000, default = 0, since = 46, relax = None],
        /// rule 17: the token's model fair value Φ(d) is at least this above its mid (fraction of
        /// one, 1e-9; 6 cents = 60_000_000)
        fvl_gap: Rate [min = -1_000_000_000, max = 1_000_000_000, default = 60_000_000, since = 46, relax = None],
        /// rule 17: minimum time between two rule-17 bids of one token
        fvl_cool_ms: DurationMs [min = 0, max = 60_000, default = 2_000, since = 46, relax = None],
        /// rule 18 (lead take, schema v47): true = a rule-15 trigger buys the favoured token with a
        /// taker order (FAK, lead_shares at the best ask + take_slip_c cents) instead of posting
        /// the lead bid; nothing rests
        take_on: Bool [default = 0, since = 47, relax = None],
        /// rule 18: only a token whose best ask is at least this (fraction of one, 1e-9)
        take_p_min: Rate [min = 0, max = 1_000_000_000, default = 550_000_000, since = 47, relax = None],
        /// rule 18: ... and below this
        take_p_max: Rate [min = 0, max = 1_000_000_000, default = 850_000_000, since = 47, relax = None],
        /// rule 18: the order's worst price is the best ask plus this many cents
        take_slip_c: Count [min = 0, max = 10, default = 1, since = 47, relax = None],
        /// rule 18: taker orders per token per window
        take_max_win: Count [min = 1, max = 100, default = 1, since = 47, relax = None],
        /// rule 18: taker orders per rolling hour over all tokens (0 = no limit; at most 256)
        take_max_hour: Count [min = 0, max = 256, default = 0, since = 47, relax = None],
        /// rule 18 (schema v48): true = no taker buy of a token while the window's other token is
        /// held or being bought (one side per window)
        take_one_side: Bool [default = 0, since = 48, relax = None],
    }
}

/// `ParamError::GroupShape` codes.
pub mod shape {
    /// More than two reference groups, or not an even number (≥ 2) of slot groups before them.
    pub const COUNT: u16 = 1;
    /// A group does not have exactly one leg.
    pub const LEGS: u16 = 2;
    /// A slot leg is not on the venue of the first slot, or the reference leg is.
    pub const VENUE: u16 = 3;
    /// A leg's inst is not in refdata.
    pub const INST: u16 = 4;
    /// A perpetual auxiliary group (`ref_aux` bit 0 or bit 2) is not on a perpetual venue.
    pub const AUX: u16 = 5;
}

/// `ParamError::Invalid` codes.
pub mod invalid {
    /// `p_min > p_max`.
    pub const PRICE_BAND: u16 = 1;
}

impl BinaryMakerParams {
    /// Groups: `2 × pairs` single-leg slot groups on one venue (Up, Down of pair 0, then pair 1, …),
    /// then one or two single-leg reference groups on other venues: the first is the fair-value
    /// reference (Binance), a second one the fast reference of the v37 Coinbase guard.
    pub fn check(&self, groups: &GroupTable, rd: &RefView<'_>) -> Result<(), ParamError> {
        let n = groups.len();
        let d = self.defaults();
        if d.p_min > d.p_max {
            return Err(ParamError::Invalid {
                field_ix: 4,
                code: invalid::PRICE_BAND,
            });
        }
        let ref_aux = d.ref_aux;
        let aux = crate::fast::aux_count(ref_aux);
        let refs = crate::fast::reference_count(groups, aux).max(1);
        let n_main = n.saturating_sub(aux);
        if n_main < 3 || refs > 2 || (n_main - refs) < 2 || !(n_main - refs).is_multiple_of(2) {
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
            let reference = gix + refs >= n_main;
            let v = *slot_venue.get_or_insert(l.venue);
            if (l.venue == v) == reference {
                return Err(err(shape::VENUE));
            }
            if gix >= n_main
                && matches!(
                    crate::fast::aux_role(ref_aux, gix - n_main),
                    crate::fast::AUX_PERP | crate::fast::AUX_CBP
                )
                && rd.caps(l.venue).market != hft_strategy_sdk::prelude::MarketKind::LinearPerp
            {
                return Err(err(shape::AUX));
            }
        }
        Ok(())
    }
}
