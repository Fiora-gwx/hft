---
component: hft-replay
prefix: RSR
spec: README.md
---

# hft-replay 测试规格

格式与规则见 [docs/tests](../../docs/tests/README.md)。端到端用例(真实 DST journal 文件经 `run_one` 与 `replayer research`)在 [replayer TESTS](../../apps/replayer/TESTS.md#rpl-300-研究回放模拟账户的运行确定) RPL-300–RPL-303。

## 覆盖

| README 小节 | 用例 |
|---|---|
| 概要 | RPL-300 |
| 目录 | 不适用:非规范 |
| 使用本组件 | RPL-303 |
| 命令行 | RSR-001、RSR-004、RPL-303、RPL-306 |
| 驱动 | RPL-300、RPL-301 |
| 账户屏蔽 | RSR-003、RPL-300、RPL-301 |
| 模拟场所 | RPL-300、RPL-305、RPL-306 |
| 参数变体 | RSR-001、RPL-302 |
| 确定性 | RPL-300、SIM-231 |
| 输出 | RSR-002、RPL-303 |
| parity | RPL-304 |
| 实现说明 | 不适用:非规范 |
| 延迟与资源契约 | 不适用:无门槛 |
| 测试与验收 | 不适用:非规范 |
| 依赖 | 不适用:非规范 |
| 延伸阅读 | 不适用:非规范 |
| 已知限制与后续工作 | 不适用:里程碑外 |
| 开发备忘 | 不适用:非规范 |

## 用例

### RSR-001 变体规格的解析与校验

- 规则:[参数变体](README.md#参数变体)、[命令行](README.md#命令行)
- 级别:table
- 里程碑:M3
- 前置:无
- 实现:`crates/hft-replay/tests/unit.rs::rsr_001_variant_specs`

| 行 | 操作 | 期望 |
|---|---|---|
| RSR-001.1 | `--set " a=1, b=-2 ,a=3"` | `{a: 3, b: -2}` |
| RSR-001.2 | `a`;`a=1.5` | 都拒绝 |
| RSR-001.3 | 变体文件两项 `x`(`a = 1`)、`y`;重名;空名 | 两项、`x.a = 1`、`y` 无字段;后两者拒绝 |

### RSR-002 列表写成 Parquet

- 规则:[输出](README.md#输出)
- 级别:table
- 里程碑:M3
- 前置:系统临时目录下的空目录
- 实现:`crates/hft-replay/tests/unit.rs::rsr_002_parquet_round_trip`

| 行 | 操作 | 期望 |
|---|---|---|
| RSR-002.1 | i64/u64/u8/f64/字符串五列两行,`extend` 自身后写出,带 `hft.run` 元数据 | 读回 4 行、列名顺序不变、键值元数据原样 |
| RSR-002.2 | 列长不等的表写出;同名列换类型后 `extend` | 都返回错误 |

### RSR-003 被替换回报的账户字段

- 规则:[账户屏蔽](README.md#账户屏蔽)
- 级别:table
- 里程碑:M3
- 前置:全零记录体,只设 `account` 字段
- 实现:`crates/hft-replay/tests/unit.rs::rsr_003_report_accounts`

| 行 | 操作 | 期望 |
|---|---|---|
| RSR-003.1 | `OrderEvent`、`SendReport`、`ExternalFill`、`BalanceReport`、`PositionReport`、`FundingSettle` 体,`account` 字段 = 7 | 都属于被替换的种类;读出 `AccountId(7)` |
| RSR-003.2 | `ConnState`、`AccountConfigReport`、`Bbo` | 不属于被替换的种类;无账户 |

### RSR-004 校准表

- 规则:[Polymarket 场所行为、校准表与条件延迟](README.md#polymarket-场所行为校准表与条件延迟)
- 级别:table
- 里程碑:M3
- 前置:`SimParams::default()`
- 实现:`crates/hft-replay/tests/unit.rs::rsr_004_calibration_table`

| 行 | 输入 | 期望 |
|---|---|---|
| RSR-004.1 | 含全部字段与 `pools`(`ack\|1\|0\|0`、`cxl\|2`、`take`、未知 leg `other`)的表 | 64 位 SHA-256;`QueueCap`、墓碑、撤单延迟、吃单延迟、`BinaryTaker{7×10⁷}`、GTD、成交回报延迟、阈值换成 ns;三个池(未知 leg 忽略),样本秒换成 µs、键的通配为 `None` |
| RSR-004.2 | 未知字段;`maker_fill` 非法;`fee_k` 2.0 | 都拒绝 |

## 验收

M1 的验收口径(重构设计 §8.2 M1 (a)–(e))在研究机上用实盘 journal 完成,结果记入 [重构进度](../../docs/refactor/PROGRESS.md):与 `bm_sim.py` 同参数同固定延迟的逐窗相关 ≥ 0.95、合计差 ≤ 5 %;10 M 事件单变体 < 60 s。仓内用例只覆盖机制与确定性。
