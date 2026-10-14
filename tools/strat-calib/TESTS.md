# strat-calib 测试与验收

覆盖本工具的离线研究契约,不新增冻结策略规格编号。策略规则仍归原 TESTS.md;test-trace 报告不替代行为测试。

| 行为 | 实现 |
|---|---|
| 双腿费用、共同 lot、深度/名义/仓位 cap、现货库存、减仓、溢出与 tick/slip | tests/test_calibration.py::test_shared_rust_signal_sizing 与 tests/conformance.rs::calibration_shared_taker_signal_and_sizing 共享 JSON |
| 年化不同周期、费用、借贷、负费率 floor 和未知周期 | test_shared_rust_funding 与 calibration_shared_funding_net_apr |
| carry 真实 Strategy 目标、最小金额、资金与杠杆限制 | test_shared_real_rust_carry_target 与 calibration_shared_carry_target_uses_real_strategy |
| 冷却、年龄等于上限、cap 停手、期末退出费使持续基差现金流变亏损 | test_caps_cooldown_age_and_terminal_cost_prevent_fake_arbitrage_profit |
| 反向盈利价差减两腿到零 | test_reversal_reduces_to_zero_and_keeps_both_legs_equal |
| 预测可见前不进场、实际结算符号、衰减退出、陈旧预测持有 | test_carry_no_lookahead_settlement_sign_exit_and_stale_hold |
| 同刻先结算后入场、不领取过去现金 | test_carry_settlement_before_entry_cannot_be_collected |
| 目标按双侧深度分片、名义不随杠杆放大 | test_carry_depth_slices_keep_target_and_margin_not_levered |
| 日合计仅筛选、期限敏感性 | test_daily_totals_never_create_timestamped_trades |
| 倍率异常及坏盘口不可产生利润 | test_scale_and_invalid_quotes_are_not_opportunities |
| 参数/费率/元数据缺失拒绝、资金费事件去重和周期校验 | test_config_rejects_unknown_fee_missing_contract_and_invalid_params、test_funding_rejects_duplicates_and_future_prediction_contract |
| Parquet 真实 CLI、交集对齐、不向前填充、JSON/CSV/中文报告/数据哈希 | test_cli_parquet_outputs_and_no_future_fill |

Python 验证命令为 `python -m pytest -q tools/strat-calib/tests`;Rust 为 `cargo test -p strat-calib-conformance --test conformance -- --test-threads 2`。本地 Cargo 命令必须经操作者指定的 memguard,构建前检查提交内存;CI 的 fast-test 选择本工具包,ops-py 独立运行 Python 测试和 ruff。两侧必须同时通过,不能只用 Python 自证与 Rust 一致。
