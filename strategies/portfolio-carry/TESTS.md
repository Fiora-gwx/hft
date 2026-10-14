---
component: portfolio-carry
prefix: CARRY
spec: README.md
---

# portfolio-carry 验收

规则归 [README](README.md)。数值均为测试夹具,不构成默认参数。

## 覆盖

| README 小节 | 用例 |
|---|---|
| 概要 | CARRY-001、CARRY-010 |
| 目录 | 不适用:非规范 |
| 使用本组件 | 不适用:见子节 |
| 测试与验收 | 不适用:非规范 |
| 依赖 | 不适用:非规范 |
| 延伸阅读 | 不适用:非规范 |
| 已知限制与后续工作 | 不适用:非规范 |
| group 形态 | CARRY-002、CARRY-004 |
| 信号与资金口径 | CARRY-001–CARRY-004 |
| 输出与退出 | CARRY-002、CARRY-005、CARRY-008 |
| 参数表 | CARRY-001–CARRY-008 |
| 事件处理表 | CARRY-002–CARRY-008 |
| Note 码表与报表字段 | CARRY-010 |
| 策略命令 | 不适用:非规范 |
| Advice 与预热载荷 | 不适用:非规范 |
| 实现说明 | CARRY-007 |
| 延迟与资源契约 | CARRY-009 |

## 用例

### CARRY-001 年化差额与净成本

- 规则:[README](README.md#使用本组件)
- 级别:unit
- 里程碑:M4
- 前置:两个交易所的同资产现货/永续 fixture,启用策略,显式输入资金、杠杆、报价与费率。
- 步骤:向 SDK MockViews/策略送入标题所列输入变化。
- 期望:不同周期按年化比较,手续费与借贷成本降低信号;周期零不入场。
- 实现:`strategies/portfolio-carry/tests/signals.rs::carry_001_annual_differential_fees_borrow_and_intervals`

### CARRY-002 等量 linked 双腿与硬上限

- 规则:[README](README.md#使用本组件)
- 级别:unit
- 里程碑:M4
- 前置:两个交易所的同资产现货/永续 fixture,启用策略,显式输入资金、杠杆、报价与费率。
- 步骤:向 SDK MockViews/策略送入标题所列输入变化。
- 期望:发一个 Simultaneous/Reverse 目标,两腿数量相反且符合每腿名义上限。
- 实现:`strategies/portfolio-carry/tests/signals.rs::carry_002_entry_has_equal_linked_legs_and_cap`

### CARRY-003 预测数据与结算窗口

- 规则:[README](README.md#使用本组件)
- 级别:unit
- 里程碑:M4
- 前置:两个交易所的同资产现货/永续 fixture,启用策略,显式输入资金、杠杆、报价与费率。
- 步骤:向 SDK MockViews/策略送入标题所列输入变化。
- 期望:周期未知、缺更新时间、过近或过远结算均无开仓目标。
- 实现:`strategies/portfolio-carry/tests/signals.rs::carry_003_missing_stale_or_late_funding_prevents_entry`

### CARRY-004 可用资金和杠杆门控

- 规则:[README](README.md#使用本组件)
- 级别:unit
- 里程碑:M4
- 前置:两个交易所的同资产现货/永续 fixture,启用策略,显式输入资金、杠杆、报价与费率。
- 步骤:向 SDK MockViews/策略送入标题所列输入变化。
- 期望:无杠杆、零杠杆、超过上限或现货现金不足时无开仓目标。
- 实现:`strategies/portfolio-carry/tests/signals.rs::carry_004_margin_and_leverage_fail_closed`

### CARRY-005 持有与边际衰减退出

- 规则:[README](README.md#使用本组件)
- 级别:unit
- 里程碑:M4
- 前置:两个交易所的同资产现货/永续 fixture,启用策略,显式输入资金、杠杆、报价与费率。
- 步骤:向 SDK MockViews/策略送入标题所列输入变化。
- 期望:收益保持时无新目标;衰减时两腿目标为零且 reason=Exit,执行算法仍为 Simultaneous/Reverse。
- 实现:`strategies/portfolio-carry/tests/signals.rs::carry_005_edge_decay_exits_and_hysteresis_holds`

### CARRY-006 安静报价与断线

- 规则:[README](README.md#使用本组件)
- 级别:unit
- 里程碑:M4
- 前置:两个交易所的同资产现货/永续 fixture,启用策略,显式输入资金、杠杆、报价与费率。
- 步骤:向 SDK MockViews/策略送入标题所列输入变化。
- 期望:健康连接旧报价允许入场;连接不可用时无开仓目标。
- 实现:`strategies/portfolio-carry/tests/signals.rs::carry_006_healthy_quiet_quote_accepted_disconnected_quote_rejected`

### CARRY-007 快照与原子恢复

- 规则:[README](README.md#使用本组件)
- 级别:unit
- 里程碑:M4
- 前置:两个交易所的同资产现货/永续 fixture,启用策略,显式输入资金、杠杆、报价与费率。
- 步骤:向 SDK MockViews/策略送入标题所列输入变化。
- 期望:恢复后冷却期不重发;截断快照失败且原状态字节不变。
- 实现:`strategies/portfolio-carry/tests/signals.rs::carry_007_snapshot_restores_cooldown_and_rejects_partial_corruption`

### CARRY-008 禁用与 CloseOnly 退出

- 规则:[README](README.md#使用本组件)
- 级别:unit
- 里程碑:M4
- 前置:两个交易所的同资产现货/永续 fixture,启用策略,显式输入资金、杠杆、报价与费率。
- 步骤:向 SDK MockViews/策略送入标题所列输入变化。
- 期望:禁用或 CloseOnly 都给已有仓位两腿零目标,执行算法为 Simultaneous/Reverse。
- 实现:`strategies/portfolio-carry/tests/signals.rs::carry_008_disabled_and_closeonly_exit_held_positions`

### CARRY-009 单 group 热路径基准

- 规则:[资源契约](README.md#延迟与资源契约)
- 级别:bench
- 基准名:`strategies/portfolio-carry/{tick_miss,tick_fire,target_set,hold,exec_update,risk_event}`
- 里程碑:M4
- 前置:一个 group、显式报价/资金/费率与固定缓冲;持有、Exec、Risk 用例设置等量对冲仓位。
- 步骤:Criterion 重复相应 SDK 事件,每次恢复初态;配置、恢复与输出断言不计时。
- 期望:未入场/持有/Exec 不产生目标;BBO 与定时器入场每次产生等量反向目标;Risk 锁存后下一 group 事件退出至零。参考机 p50/p99 不超过 README/SDK 预算;CI smoke 只报告;慢侧与 Advice 明确不适用。
- 实现:`strategies/portfolio-carry/benches/on_event.rs::carry_009_entries`

### CARRY-010 真实 sequencer 的双向 linked 成交、资金费与重放

- 规则:[测试与验收](README.md#测试与验收)
- 级别:integ
- 里程碑:M4
- 前置:registry 静态 Strategies,真实 sequencer 与两个模拟永续 venue,mock 仅响应引擎发出的订单。
- 步骤:分别缩小两条腿各自对手侧的深度,反侧深度设置为干扰值;四组费率各跑两个方向;在任何成交前读取两腿订单与 OrderDecision;注入实际资金费两次;将净资金费差降至零并检查退出双发。12 个故障种子覆盖任一腿多成、单腿拒单和在途 venue 断线,断线后先恢复连接再用 QueryResult 确认终态;每种子重复运行并验证 journal 重放。
- 期望:入场和退出两笔订单的 trigger_seq 相同、Second.target_slot 指向 First.slot,两腿限价在 BBO ± max_cross 内,IOC 数量取双方可执行深度与上限的较小值;两个等量反向仓位、资金费只记一次、退出后两腿归零。未知结果确认前不修复或重发,断线不改变仓位;终态后只对超额腿发一次 Reverse,成交后恢复等量对冲;重复执行的决策/账本字节一致,replay verify 零差异。
- 实现:`strategies/registry/tests/portfolio_carry.rs::carry_010_linked_entry_funding_and_edge_decay_exit`
