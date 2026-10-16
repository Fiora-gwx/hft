---
description: "有界资金费差策略:跨交易所空永续/多现货,整数年化信号、费用与借贷成本、结算前入场、衰减退出、等量 linked legs 与资金上限"
kind: crate
tier: hot
scope: core
status: impl
---

# portfolio-carry

## 概要

每个 group 持有同一资产的等量反向两腿,赚取净资金费差。第 0 腿为协议 VenueId 1 的线性永续,第 1 腿为 VenueId 11 的现货,同一 base、USDT quote;只做空第 0 腿、多第 1 腿。两腿必须在不同交易所,使用同一计价资产与资产数量指数。策略只输出 SDK `TargetIntent`,入场和退出均由执行层 `Simultaneous` 在同一事件内发出两腿 IOC。

策略参数在模拟账户上标定。当前参数默认值均为暂定研究值,生产上线仍需通过 [上线门槛](../../docs/testing.md#8-上线门槛)。`kind_id=1`、schema_id=1、schema.version=2、描述符 version=2,`NoModel<CarryParams>` 不消费 Advice 或预热。实际资金费记账归 [ledger](../../crates/hft-ledger/README.md#资金费),策略不按预测费率修改 PnL。

## 目录

- [使用本组件](#使用本组件)
- [实现说明](#实现说明)
- [延迟与资源契约](#延迟与资源契约)
- [测试与验收](#测试与验收)
- [依赖](#依赖)
- [延伸阅读](#延伸阅读)
- [已知限制与后续工作](#已知限制与后续工作)

## 使用本组件

### group 形态

恰好两条 CEX 腿。第 0 腿必须为指定永续;第 1 腿必须为指定现货。加载和 SetParams 拒绝其他 venue、永续/永续与颠倒腿顺序。同一实例内任一 inst 只能属于一个 group,避免资金与仓位重叠归属。参数加载拒绝错误市场、不同资产/计价币/数量指数、venue 不符、同交易所两腿及重复 inst。账户绑定与模式闸门沿用 SDK/引擎校验。

### 信号与资金口径

`annual(rate, interval_s) = floor(rate × 365 × 86400 / interval_s)`。永续空腿年化资金费减借贷/机会成本、往返交易成本与预期基差成本。现货腿资金费恒为 0,无需预测或结算记录。往返成本为两腿 taker 费率之和的两倍,加四次 `max_cross` 和一次 `expected_basis_cost`,按 `fee_horizon_ms` 年化;计算用 i128,最终整数越界时不入场。正费率时空腿收款、负费率时空腿付款。

净年化达到 `enter_apr`,且短腿下次结算距当前 UTC 位于 `[min_lead_ms, entry_window_ms]` 才入场。要求时钟可信、永续腿周期非零、预测费率未超过 `funding_max_age_ms`。开仓前两条 BBO 的连接必须健康,venue 必须就绪;盘口不变不视为过期。所有观察均来自已 journal 的 Views。

每个 venue 的可用资金按 `capital_fraction / 共享该 venue 的 group 数` 划分。永续用非陈旧保证金的 `min(margin_balance, avail_margin)`;现货用可用计价币现金。数量按保守价格和两腿 lot 的最小公倍数向下对齐,同时满足最小数量/金额与每腿 `max_notional`。资金上限不随杠杆放大;实际杠杆必须已从账户配置读到且不超过 `max_leverage_x100`。策略不自动借币。

### 输出与退出

目标数量为 `(-q, q)`,其中 q > 0,入场及零目标退出共用 `Simultaneous(SimulParams)`,固定 `on_fail=Reverse`、`fix_tolerance=0`、`residual=TrimExcess`。每片双腿整对准入后同事件发出,无需等待任一腿成交。切片取两腿各自对手侧 BBO 深度的较小值,再受目标剩余量、资金与名义上限约束,按两腿 lot 对齐;`depth_frac=1`、`depth_levels=1`。两腿限价以本次切片观察的 BBO 为准,买价不高于 ask × (1 + `max_cross`),卖价不低于 bid × (1 − `max_cross`),tick 舍入与引擎价格带只能收紧边界。

两原单终态后,非对称成交按 `Reverse` 请求反向处置超额量;反向单仍受引擎风险递减检查,失败交 Broken/修复机制。下单、断腿、风控、Unknown 与修复由引擎负责。存在 Unknown、pending 或 open link 时等待,断线本身不改变仓位;Broken 或仍在工作的引擎目标不覆盖,引擎处置完成后继续评估。重复目标不重发;失败重试受 `retry_ms` 限制。

已有仓位的净年化低于 `exit_apr` 时发两腿零目标(`Exit`)。`enabled=false`、`open_allowed=false`、CloseOnly、保证金压力、自身外部成交或品种通知同样触发退出。安全退出在 Broken/引擎介入期间保留到持仓归零。费率缺失时持有已有对冲仓位;未成交的开仓目标在费率失效后撤向零。退出不要求现货资金费;永续资金费变负并使净边低于 exit_apr 时退出。遗留反向仓位只请求零目标,不继续加仓。

### 参数表

14 个字段由 `define_params!` 唯一声明。Rate 的整数尺度为 1e-9,Notional 为 1e-6。所有 `minor_change=None`,没有自动小改授权;默认值均暂定。完整 min/max 与放宽方向以导出 schema 为准。

| 字段 | 默认 | per_group | manual_only | 用途 |
|---|---:|---|---|---|
| enabled | false | 是 | 是 | 禁用时退出;可对单 group 或全实例设置 |
| enter_apr | 0.10 | 是 | 否 | 净年化入场阈值 |
| exit_apr | 0.02 | 是 | 否 | 净年化退出阈值,必须小于 enter_apr |
| borrow_apr | 0 | 是 | 是 | 年化借贷/机会成本 |
| fee_horizon_ms | 86400000 | 是 | 否 | 往返费用摊销周期 |
| entry_window_ms | 300000 | 是 | 否 | 结算前入场窗口 |
| min_lead_ms | 10000 | 是 | 否 | 成交预留时间,必须小于入场窗口 |
| funding_max_age_ms | 30000 | 否 | 否 | 预测费率最大年龄 |
| max_notional | 100 | 是 | 否 | 每条腿的名义金额硬上限 |
| capital_fraction | 0.4 | 否 | 否 | venue 可用资金使用比例 |
| max_leverage_x100 | 300 | 否 | 是 | 实际杠杆上限,百分之一倍 |
| max_cross | 0.0001 | 是 | 否 | 下单最大跨价及信号成本 |
| retry_ms | 1000 | 否 | 否 | 评估定时器与重试间隔 |
| expected_basis_cost | 0 | 是 | 是 | fee_horizon_ms 内预期不利基差变动的收益率成本;0 表示明确假设无基差损失,不是已标定结论 |

### 事件处理表

| 事件 | 行为 |
|---|---|
| Bbo/Funding/Mark | 单 group 评估;只订阅对应腿需要的流 |
| Timer | 单 group 重评估并续约 engine 时间定时器 |
| Order/Exec | 待订单/链接状态收敛后重评估 |
| Mode | CloseOnly 退出,其余按引擎模式门控 |
| Risk | 保证金 L1/L2/L3 锁存退出;实际执行遵守引擎目标优先级 |
| External/Notice | 锁存安全退出,通过组事件或已注册定时器执行 |
| Conn | 不凭连接事件猜仓位,下次评估用 quote_ok/venue_ready |

### Note 码表与报表字段

| code | 名称 | 值 |
|---|---|---|
| 1 | CarryTarget | v0 为带符号第 0 腿数量,v1 为净年化,v2 为 TargetReason |

报表 `targets` 对 code 1 按 group 计数,`net_apr` 取最后 v1。Note 说明目标请求,不代表成交或实际资金费。

### 策略命令

`cmds=[]`。退出通过设置 `enabled=false`;引擎账户 flatten 用于人工清仓。此版本不接受 `exit/exit_all` 私有命令。

### Advice 与预热载荷

均无。配置来自人工 schema 参数与 GroupTable,不要求 pipeline 插件;[pipeline](pipeline/README.md) 保留研究接口说明。

## 实现说明

`params.rs` 声明与校验参数;`fast/mod.rs` 做整数信号、容量计算、目标与快照;`lib.rs` 声明描述符并转发 SDK 入口。group 表仅在 cold new/on_params 分配。on_event 无堆分配,单次评估扫描有限 group 表以计算每 venue 的资金份额。

快照包含 group id/指纹、定时器期限、重试时刻与退出锁存。持久时刻只存 engine_ns;恢复先在副本上校验版本、容量、指纹与完整字节,成功后一次替换。参数更新按 id/指纹保留状态,结构改变时重置。

## 延迟与资源契约

以下是待参考机标定的验收预算,不是本次实测性能声明;分段归 [latency-budget](../../docs/latency-budget.md)。Criterion 基准 `strategies/portfolio-carry/{tick_miss,tick_fire,target_set,hold,exec_update,risk_event}` 覆盖单 group 未入场、BBO 入场、定时器目标、持有、成交完成重评估与保证金退出锁存;除 hold 外预算归 [SDK](../../crates/hft-strategy-sdk/README.md#延迟与资源契约)。每次采样恢复初态,配置、恢复和输出断言不计时;CI smoke 只报告。NoModel 无慢侧更新或 Advice,在 `benches/not-applicable.toml` 声明这两项不适用。

| 入口 | p50 / p99 预算 | 内存与验证 |
|---|---|---|
| new/on_params/check | cold,无热线程预算 | O(group 数) 状态,启动/配置路径分配 |
| subscriptions | cold,无热线程预算 | 每 group 至多 2 条订阅 |
| on_event(Bbo),持有 | 60 ns / 150 ns,待标定 | 零分配,无系统时钟/锁/阻塞/f64;上述 Criterion |
| snapshot | 10 µs / 50 µs,待标定 | 2 + 29 × group 数字节,无 TSC |
| restore | cold,无热线程预算 | 原子校验,临时 O(group 数) 副本 |
| NoModel | 不适用 | 无历史样本、Advice 或检查点 |

## 测试与验收

[TESTS.md](TESTS.md) 定义信号、进入/退出、资金/杠杆、连接健康和快照用例。真实 sequencer 场景在 `registry/tests/portfolio_carry.rs`,以 mock 成交验证空永续/多现货的同事件双发、双侧深度与限价边界、非对称成交、单腿拒绝、在途断线后查询收敛、真实资金费去重和退出。故障种子检查风险修复、不凭连接猜仓位、重复执行确定性,每个场景逐字节 replay verify。该场景使用 mock 的 PerSymbolPush 能力;实际交易所的 RateOnly 账单完成由独立引擎改动验收。

## 依赖

生产仅依赖 SDK 与 strat-lib。测试用基础类型/refdata 与 Criterion;真实引擎测试依赖放 registry 的 dev-dependencies,引擎与 app 不依赖本策略。

## 延伸阅读

- [策略接口](../../crates/hft-strategy-sdk/README.md)
- [执行层](../../crates/hft-exec/README.md)
- [产品形态决定](../../.agents/notes/implemented/feature/2026-10-16-perp-spot-shape.md)
- [范围决定](../../.agents/notes/implemented/feature/2026-10-14-bounded-funding-carry.md)
- [双腿同发决定](../../.agents/notes/implemented/feature/2026-10-14-carry-simultaneous.md)

## 已知限制与后续工作

纯计算 net_edge 保留双永续差分公式供研究对照,参数校验和交易路径均禁用双永续/反向开仓。同交易所现货/永续不在当前账户与两腿基线内,需要负责人另行决定;不能在策略中绕过 refdata/SDK。首次收到费率时若周期未知,等待后续不同结算时间确认周期。无历史分位 Model、截面打分、pipeline 名单生成、自动借币、动态再平衡或私有命令。实际资金费历史轮询的部署接线与账单时刻验证归 venue/运行时,不可把预测收益视为已收现金。

### 开发备忘

参考机验证前保留基线单 group 重评估预算;当前未提供性能通过声明。费用摊销与资金费窗口需在模拟账户校准。
