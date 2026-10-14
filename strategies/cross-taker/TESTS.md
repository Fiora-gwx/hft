# cross-taker 测试与验收

覆盖任务要求与 README 原有信号、库存、报价、冷却、双腿和恢复场景,不另分配冻结规格编号。test-trace 为报告项。

| 行为 | 测试函数 |
|---|---|
| 双边价格加权费用、净边、滑点 | signal_deducts_both_fee_notionals_and_requires_edge |
| 两腿限价固定于观察 BBO,slip 边界与 tick 舍入 | signal_fixes_both_limits_within_observed_slippage |
| 双侧顶档、单笔 cap、共同 lot | signal_sizes_both_depths_caps_and_common_lot |
| 逐腿库存 cap、现货不裸卖 | signal_caps_each_position_and_never_shorts_spot |
| 超限仅减少、Reduce 不穿零 | signal_reduction_stops_at_zero_even_above_position_cap |
| 坏盘口、零价、溢出 | signal_rejects_bad_books_and_arithmetic_overflow |
| 精确最小名义、资产指数 | signal_accepts_exact_minimum_notional_and_all_asset_exponents |
| 同一事件先发双腿、实际成交、cap 停手、反向清仓、验证回放 | cross_taker_trades_both_legs_obeys_caps_and_replays |
| 链接第二腿超过 HEDGE_LIMIT 时第一腿不发出 | cross_taker_link_respects_engine_hedge_limit_before_first_leg |
| 三个 DST 种子、实际拒单后 Reverse 平掉超额腿、全不变量与重跑确定性 | cross_taker_dst_reverses_rejected_pair_and_is_deterministic |
| 年龄边界、健康 pong、断连、模式、Unknown、恢复冷却 | cross_taker_guards_age_connections_modes_unknown_and_cooldown |
| 任一腿失败、tag=0 回报、整对去重、halt、重载/恢复锁存、安全恢复 Live | cross_taker_halts_on_repeated_hedge_failures_and_keeps_latch |

signal_* 在 tests/signal.rs;cross_taker_* 在 registry/tests/cross_taker_e2e.rs,需 cross-taker feature。引擎夹具为 SeqSut<Strategies> + 实际 SimExchange,包含现货/永续和非零 taker 费用,不以测试策略替代真实策略。

短场景与三个 DST 种子不替代 docs/testing.md 上线门槛。真实行情长期模拟、大规模故障 DST、参考机延迟与资金费持有成本校准仍待完成。
