---
component: pm-taker-probe
prefix: PTP
spec: README.md
---

# pm-taker-probe 测试规格

格式与规则见 [docs/tests](../../docs/tests/README.md)。

本文件的记号:夹具 `tests/common`:二元结果代币 venue 的槽位 inst `U`(价 0.001、量 1e-6、lot 0.01)与同 venue 的 `X`,Sim 账户 `A1`,group `G1` = (U);驱动 `D` 在每次调用后给每张下单 `Placed(r<n>)`(n 从 1 计),`next` 可改为其他放行结果;`U` 报价 0.100 / 0.110(连接健康);`position(q)` 设 `G1` 腿持仓;"武装" = 以 `enabled = 0` 创建后 `SetParams{enabled = 1}`。

## 覆盖

| README 小节 | 用例 |
|---|---|
| 概要 | PTP-003 |
| 目录 | 不适用:非规范 |
| 使用本组件 | 不适用:见子节 |
| group 形态 | PTP-009 |
| 参数 | PTP-002、PTP-005 |
| 订阅 | PTP-001 |
| 规则 | PTP-002–PTP-007 |
| 事件处理 | PTP-003、PTP-007 |
| Note 码表与报表字段 | PTP-003–PTP-007 |
| 实现说明 | PTP-008 |
| 延迟与资源契约 | PTP-003 |
| 测试与验收 | 不适用:非规范 |
| 依赖 | 不适用:非规范 |
| 延伸阅读 | 不适用:非规范 |
| 已知限制与后续工作 | 不适用:非规范 |
| 开发备忘 | 不适用:非规范 |

## 用例

### PTP-001 只订阅该腿的簿

- 规则:[订阅](README.md#订阅)
- 级别:unit
- 里程碑:M3
- 前置:默认参数,`G1`
- 实现:`strategies/pm-taker-probe/tests/scenario.rs::ptp_001_subscribes_the_leg_book`

| 行 | 输入 | 期望 |
|---|---|---|
| PTP-001.1 | `subscriptions(G1)` | 恰为 `(U, 0, BOOK, Fast)` |

### PTP-002 只有 enabled 0 → 1 武装

- 规则:[规则](README.md#规则) 1、[参数](README.md#参数)
- 级别:table
- 里程碑:M3
- 实现:`strategies/pm-taker-probe/tests/scenario.rs::ptp_002_nothing_until_enabled_goes_0_to_1`

| 行 | 输入 | 期望 |
|---|---|---|
| PTP-002.1 | 以 `enabled = 1` 创建;簿更新 | 空闲,无下单 |
| PTP-002.2 | 再 `SetParams{enabled 1, spend_usdc 1.5}`;簿更新 | 无下单 |
| PTP-002.3 | `enabled` 0 再 1;簿更新 | `Armed`;一张买单 `px 0.110, qty 13.63` |
| PTP-002.4 | 新实例武装后又 `enabled 0`;簿更新 | 空闲,无下单 |

### PTP-003 一轮:按金额买入,终结后卖回

- 规则:[规则](README.md#规则) 2–5、[事件处理](README.md#事件处理)、[Note 码表与报表字段](README.md#note-码表与报表字段)、[延迟与资源契约](README.md#延迟与资源契约)
- 级别:table
- 里程碑:M3
- 前置:武装,默认参数;每次调用不分配
- 实现:`strategies/pm-taker-probe/tests/scenario.rs::ptp_003_one_round_buy_then_sell_back`

| 行 | 输入 | 期望 |
|---|---|---|
| PTP-003.1 | 簿更新 | 恰一张 `spend_buy(Ioc, px 0.110, qty 9.09, tag 1)`:`Buy`、`Open`、`flags = SPEND`;`BuySent{110, 9 090 000, 1 000 000}` |
| PTP-003.2 | 再一次簿更新;`Filled{cum 9.09091, done false}` | 无下单、无 Note;`BuyLive` |
| PTP-003.3 | 持仓 9.09091;`Filled{done true}` | `BuyEnd{1, 9 090 910, 0}`,再一张卖单:`Sell`、`Ioc`、`Reduce`、`flags 0`、`px 0.100`、`qty 9.09`、`tag 2`;`SellSent{100, 9 090 000, 9 090 910}` |
| PTP-003.4 | 卖单 `Filled{done true}`;再三次簿更新 | `SellEnd{1, 9 090 000, 0}`;`Done`;之后无下单;轮数 1 |

### PTP-004 再次武装要 enabled 先回 0

- 规则:[规则](README.md#规则) 1、5
- 级别:table
- 里程碑:M3
- 前置:武装,`sell_back = 0`
- 实现:`strategies/pm-taker-probe/tests/scenario.rs::ptp_004_rearm_needs_enabled_0_then_1`

| 行 | 输入 | 期望 |
|---|---|---|
| PTP-004.1 | 簿更新;买单 `Cancelled` | `BuyEnd{2}`,无卖单;`Done` |
| PTP-004.2 | `SetParams{enabled 1}`(不变);簿更新 | 无下单 |
| PTP-004.3 | `enabled` 0(→ 空闲)再 1;簿更新 | 第二张买单;轮数 2 |
| PTP-004.4 | 买单在途时 `enabled 0`;买单 `Cancelled` | `BuyEnd`;直接回到空闲 |

### PTP-005 花费上限与参数 schema

- 规则:[参数](README.md#参数)、[规则](README.md#规则) 2
- 级别:table
- 里程碑:M3
- 实现:`strategies/pm-taker-probe/tests/scenario.rs::ptp_005_spend_cap_and_parameter_schema`

| 行 | 输入 | 期望 |
|---|---|---|
| PTP-005.1 | schema 字段 0 | `enabled`,默认 0,`manual_only` |
| PTP-005.2 | 字段 1 | `spend_usdc`,min 1、max 2 000 000、默认 1 000 000 |
| PTP-005.3 | 字段 2 | `sell_back`,默认 1 |
| PTP-005.4 | `spend_usdc = 2 000 000` 武装;簿更新 | 买单 `qty 18.18`,`px × qty ≤ 2` |
| PTP-005.5 | `spend_usdc = 1 000`(0.001)武装;簿更新;再一次簿更新 | 无下单;`Skipped{1}`;`Done`;之后仍无下单 |

### PTP-006 卖回数量与跳过

- 规则:[规则](README.md#规则) 3
- 级别:table
- 里程碑:M3
- 前置:武装;买单 `Filled{done true}` 前设持仓;每行从前置重新开始
- 实现:`strategies/pm-taker-probe/tests/scenario.rs::ptp_006_sell_back_sizing_and_skips`

| 行 | 输入 | 期望 |
|---|---|---|
| PTP-006.1 | 持仓 0.009999 | 无卖单;`BuyEnd`、`Skipped{3, 9 999}`;`Done` |
| PTP-006.2 | 买价为空(0 / 0.110),持仓 9.09091 | 无卖单;`Skipped{2, 9 090 000}` |
| PTP-006.3 | 持仓 12.345678 | 卖单 `qty 12.34` |
| PTP-006.4 | 买单 `Rejected{venue_code 77}`,持仓 2 | `BuyEnd{3, 0, 77}`;一张卖单(卖出已持有的数量) |

### PTP-007 报价、放行结果与无关更新

- 规则:[规则](README.md#规则) 2–5、[事件处理](README.md#事件处理)
- 级别:table
- 里程碑:M3
- 实现:`strategies/pm-taker-probe/tests/scenario.rs::ptp_007_gates_quotes_and_outcomes`

| 行 | 输入 | 期望 |
|---|---|---|
| PTP-007.1 | 武装;卖价为空;再卖价 0.110 但连接断;再 0.100 / 0.120 健康 | 前两次无下单;第三次买单 `px 0.120, qty 8.33` |
| PTP-007.2 | 武装;`X` 的簿更新 | 无下单 |
| PTP-007.3 | 武装;买单放行结果 `Shadowed`;下一事件 | `BuyEnd{4}`、`Skipped{3}`;无下单;`Done` |
| PTP-007.4 | 武装,`sell_back 0`;买单 `Rejected(RATE)`;再两次事件 | `BuyEnd{5, 0, RATE}`;无下单 |
| PTP-007.5 | 买单成交完、持仓 9.09;卖单放行结果 `Rejected(RATE)`;下一事件 | `SellEnd`;`Done` |
| PTP-007.6 | 买单在途;另一 `OrderRef` 的 `Cancelled` | 无下单、无 Note;仍 `BuyLive` |

### PTP-008 快照与重启

- 规则:[实现说明](README.md#实现说明)
- 级别:table
- 里程碑:M3
- 实现:`strategies/pm-taker-probe/tests/scenario.rs::ptp_008_snapshot_and_restart`

| 行 | 输入 | 期望 |
|---|---|---|
| PTP-008.1 | 买单在途(`BuyLive`,r1)时快照 → 新实例恢复 → 快照 | 两次快照逐字节相同;恢复后 `BuyLive`、`r1` |
| PTP-008.2 | 武装(未下单)时快照 → 恢复 | 空闲(重启解除武装) |
| PTP-008.3 | 买单刚放出(放行结果未读)时快照 → 恢复 | `Done`,无在途订单 |

### PTP-009 group 形态

- 规则:[group 形态](README.md#group-形态)
- 级别:table
- 里程碑:M3
- 前置:默认参数
- 实现:`strategies/pm-taker-probe/tests/scenario.rs::ptp_009_group_shape`

| 行 | group | 期望 |
|---|---|---|
| PTP-009.1 | `G1` | `Ok` |
| PTP-009.2 | 一个 group 两腿 (U, X) | `Err(GroupShape{gix 0, code 1})` |
| PTP-009.3 | 两个 group | `Err(GroupShape{gix 1, code 2})` |

## 验收

PTP-001–PTP-009 全部通过。
