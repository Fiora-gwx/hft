---
component: book-observer
prefix: OBS
spec: README.md
---

# book-observer 测试规格

格式与规则见 [docs/tests](../../docs/tests/README.md)。

本文件的记号:夹具 `tests/common`:`V1` 现货 venue 的 `I1`、`V2` 永续 venue 的 `I2`,Sim 账户 `A1`,group `G1` = (I1, I2);`quote(i, mid)` 设该 inst 的最优买卖为 `mid − 1` / `mid + 1`。

## 覆盖

| README 小节 | 用例 |
|---|---|
| 概要 | OBS-002 |
| 目录 | 不适用:非规范 |
| 使用本组件 | 不适用:见子节 |
| group 形态 | OBS-004 |
| 参数 | OBS-001 |
| 订阅 | OBS-001 |
| 事件处理 | OBS-002 |
| Note 码表与报表字段 | OBS-002 |
| 实现说明 | OBS-003 |
| 延迟与资源契约 | OBS-002 |
| 测试与验收 | 不适用:非规范 |
| 依赖 | 不适用:非规范 |
| 延伸阅读 | 不适用:非规范 |
| 已知限制与后续工作 | 不适用:非规范 |
| 开发备忘 | 不适用:非规范 |

## 用例

### OBS-001 订阅每腿的簿与成交

- 规则:[订阅](README.md#订阅)、[参数](README.md#参数)
- 级别:table
- 里程碑:M3
- 前置:`G1`
- 实现:`strategies/book-observer/tests/scenario.rs::obs_001_subscribes_books_and_trades_of_every_leg`

| 行 | 参数 | 期望 |
|---|---|---|
| OBS-001.1 | `trades = 1` | `(I1, 0, BOOK\|TRADE, Fast)`、`(I2, 0, BOOK\|TRADE, Fast)` |
| OBS-001.2 | `trades = 0` | `(I1, 0, BOOK, Fast)`、`(I2, 0, BOOK, Fast)` |

### OBS-002 计数与最优价,不发意图

- 规则:[事件处理](README.md#事件处理)、[延迟与资源契约](README.md#延迟与资源契约)
- 级别:unit
- 里程碑:M3
- 前置:默认参数,`G1`
- 步骤:依次 `quote(I1, 6_000_000)` + `Book(I1)`、`quote(I2, 6_000_100)` + `Book(I2)`、`quote(I1, 6_000_010)` + `Book(I1)`,再一条 `Trade(I2)`
- 期望:每个事件都没有输出;`books = [2, 1]`、`trades = [0, 1]`;I1 最优价 `(6_000_009, 6_000_011)`,I2 `(6_000_099, 6_000_101)`;处理事件不分配
- 实现:`strategies/book-observer/tests/scenario.rs::obs_002_counts_books_and_trades_without_intents`

### OBS-003 快照往返

- 规则:[实现说明](README.md#实现说明)
- 级别:unit
- 里程碑:M3
- 前置:默认参数,`G1`
- 步骤:`quote(I1, 6_000_000)` + `Book(I1)`,再 snapshot → restore → snapshot
- 期望:两次快照逐字节相同
- 实现:`strategies/book-observer/tests/scenario.rs::obs_003_snapshot_roundtrip`

### OBS-004 group 形态

- 规则:[group 形态](README.md#group-形态)
- 级别:table
- 里程碑:M3
- 前置:默认参数
- 实现:`strategies/book-observer/tests/scenario.rs::obs_004_group_shape`

| 行 | group | 期望 |
|---|---|---|
| OBS-004.1 | 一腿 (I1) | `Ok` |
| OBS-004.2 | `G1` 两腿 | `Ok` |
| OBS-004.3 | 0 腿 | `Err(GroupShape{gix 0, code 1})` |

## 验收

OBS-001–OBS-004 全部通过。
