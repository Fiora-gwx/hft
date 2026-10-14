---
description: "跨所 taker 套利:扣除两侧费用和价格余量后,按深度与持仓限额发出 Simultaneous 链接订单"
kind: crate
tier: hot
scope: optional
status: impl
---

# cross-taker

## 概要

`CrossTaker` 在 sequencer 线程比较同一资产的两个 venue 的 BBO,选择扣除两腿 taker 费和价格余量后仍达到 min_edge 的方向。买入低价腿作为链接的 First,通过 SDK LegLink 让 OMS 在同一事件同时发出买卖两腿;策略不等待成交再发第二腿,不直接发修复订单。两腿可以是现货或 USDT 线性永续,现货只能卖出分配给该 group 的现有库存。默认禁止开火,上线必须显式配置并通过仓库上线门槛。

## 目录

- [使用本组件](#使用本组件)
- [实现说明](#实现说明)
- [延迟与资源契约](#延迟与资源契约)
- [测试与验收](#测试与验收)
- [依赖](#依赖)
- [延伸阅读](#延伸阅读)
- [已知限制与后续工作](#已知限制与后续工作)

## 使用本组件

### 装配与组

registry 的 cross-taker feature 注册 CrossTaker,kind_id/schema_id = 3,version = 1;`Model = NoModel<XtParams>`。每组恰好两腿,同一 base/quote、相同定点指数、不同交易所。同一实例不允许两个 group 引用同一 inst,避免逐品种限额被重复组放大。账户必须覆盖两腿 venue;加载时拒绝无效引用、同所组合、不支持的市场、无效 tick/lot 和无效限额。两侧 BBO 都订阅 Part::Fast,状态只读 SDK Views。

### 参数 schema

所有字段 minor_change = None,参数变动按上线门槛重新评估。非零默认值均为暂定值。Notional 为 1e-6 quote 单位,Rate 为 1e-9;组必须使用相同 quote,首批部署配置 USDT。

| 字段 | 单位 | 默认 | per_group | 含义 |
|---|---|---|---|---|
| enabled | Bool | false | 否 | 人工启用;默认不交易 |
| min_edge | Rate | 0 | 是 | 扣费与价格余量后的收益率;启用时必须 > 0 |
| slip | Rate | 0 | 是 | 每腿 IOC 价格余量,0 为吃到触价 |
| depth_frac | Rate | 50% | 是 | 两侧对应顶档量的使用比例 |
| max_chunk_notional | Notional | 0 | 是 | 任一腿单笔上限;启用时必须 > 0 |
| max_position_notional | Notional | 0 | 是 | 任一腿绝对持仓上限;不得小于单笔上限 |
| max_quote_age_ms | DurationMs | 5 | 是 | 两侧报价机会年龄上限,等于上限即不交易 |
| cooldown_ms | DurationMs | 100 | 是 | 两次尝试间的 engine_ns 间隔;门控、Shadow 也消耗冷却 |
| max_hedge_failures | Count | 3 | 是 | 连续失败的双腿尝试达到此值请求 group halt |

费率读取 Views::fee().taker,由账户 refdata 费率档提供。负费率保守按零计,不计返佣收益。

### 信号与数量

令 B 为买腿 IOC 限价,S 为卖腿按相同余量计算的限价,fb/fs 为非负 taker 费率。触发条件为 `(S × (1 − fs) − B × (1 + fb)) / B >= min_edge`,用 i128 计算、收益率向下取整。两方向可行时选择净收益率较高者,相同则买 leg 0。两腿限价固定于本次观察的 BBO:买价不高于 ask × (1 + slip),卖价不低于 bid × (1 − slip),按 tick 保守取整。卖腿使用 PxMode::Limit;引擎风险价格带只能收紧限价,不因后续报价放宽。信号不是利润保证。

数量取两侧对应顶档量 × depth_frac、两侧 max_qty、每腿单笔名义余额、每腿持仓余量的最小值,再向下取到两腿 lot 的最小公倍数。名义上限以对应 ask 与限价的较大值计算;低于任一腿最小数量或最小名义则不交易。现货卖量还受 group 持仓与账户非 baseline 库存限制;可负担性和预留由引擎检查。

两腿都减小绝对仓位时标 Reduce,数量不穿过零,永续腿带 reduceOnly;其他方向标 Open。超限腿仅允许向零减少。无获利反向价差时不主动清仓,强制降仓和裸腿修复归引擎。

### 事件与状态

| 事件 | 行为 |
|---|---|
| BBO | 检查 clock、quote_ok、机会年龄、venue_ready、Unknown、在途、open_links 和冷却;超过引擎 dust 的裸量阻止新交易 |
| Order | 读实际仓位;任一腿拒单或可确认的零成交撤单累计失败,每次双腿尝试只计一次,包括 tag=0 的引擎生成腿;无失败且双腿结束并对齐的成交轮次清零连续失败数 |
| Risk HedgeBroken / Exec Broken | 同一 Broken episode 只计一次,此前已记失败则不再加;暂停本组等待引擎修复 |
| Exec Done(cleared_by_engine) | 解除 Broken 镜像;下次 BBO 按实际仓位、cap 和裸量判断 |
| Mode | CloseOnly 禁止 Open;CloseOnly/Shadow → Live 在无在途、无裸量、无开仓 cap 时解除失败锁存 |
| Conn / Risk / External | 下次 BBO 读 Views 最新连接、cap、仓位,不推测成交、不改变账本 |
| Notice | Maintenance 接收后至 to_ns 阻止交易;Delisting 持久阻止本组交易 |

两腿 IOC,LegLink.role = First,HedgeKind::Simultaneous,PxMode::Limit,数量比 1:1,ResidualPolicy::TrimExcess。默认并固定 OnFail::Reverse:两腿终态后的非对称成交由引擎反向平掉超额腿,优先回收风险,不等待第二腿重试增加敞口;Unknown 须先确认。两腿风险、模式、容量须整对通过才发出;HEDGE_LIMIT、Reverse 和 Broken 自动修复由引擎拥有。halt 只阻止 Open,已有 hedge 继续。参数重载和重启不清除失败锁存。

### Note 与报表

| code | 名称 | 级别 | v[] |
|---|---|---|---|
| 1 | Fired | Info | 买腿索引、数量、净边 Rate、是否同时减两腿 |
| 2 | HedgeFailed | P2 | 连续失败数 |
| 3 | Halted | P1 | 连续失败数 |

报表 fires、hedge_failures 按 group 统计 Note 1/2 次数。cmds 为空,无 Advice 与预热载荷。

## 实现说明

每组状态预分配,只含 engine_ns 冷却、UTC 公告期限、失败锁存、Broken 镜像和订单去重身份,不存 TSC、不复制仓位。快照按 GroupId/fingerprint 验证;参数替换保留相同 GroupId/指纹的状态。params.rs 定义 schema;fast/signal.rs 实现整数信号和数量;fast/mod.rs 处理事件、意图、快照;desc.rs 定义 Note;lib.rs 接 SDK trait。

BBO 处理先完成轮次结算与 halt 投递。正 min_edge 下,若两个方向均为原始 bid ≤ 对侧 ask,直接结束本次信号计算;限价不得比触价更有利、负费率不计收益,因此该条件不会排除有效信号。其余报价继续完整的健康、机会年龄、费用、限价和数量检查;已读取的 BBO 在本次调用内复用。

## 延迟与资源契约

on_event 无分配、无锁、无系统调用、无浮点数;每个 BBO 最多检查两个方向、提交一条双腿链接意图。每组状态不超过 128 B,按 GroupTable::capacity 预分配。快照 2 B 头 + 每组 50 B,无 Model 分配。

保留分段 `decided_tsc − proc_tsc` 的基线预算:未命中 p50 ≤ 15 ns/p99 ≤ 40 ns,命中 p50 ≤ 300 ns/p99 ≤ 1 µs,Order/Exec p50 ≤ 80 ns/p99 ≤ 250 ns,Risk 每组 p50 ≤ 150 ns/p99 ≤ 400 ns;Mode/Conn/Notice 按 Risk 预算。统一升级主机实测尚未完成,这些不是性能声明。

`benches/on_event.rs` 经真实 CrossTaker::on_event 测量 tick_miss(双侧健康但价差不足)、tick_fire(产生 Simultaneous 链接意图)、exec_update(Broken)、risk_event(HedgeBroken)。边界为入口 proc_tsc 至返回 decided_tsc;夹具和状态恢复在计时外。使用与 registry 引擎测试相同的合成常量、单 group 和 SDK MockViews,默认 Criterion 预热 3 s/100 个样本,无绑核、系统分配器;CI 短跑配置归 [tools](../../tools/README.md#cargo-xtask-bench-gate)。Criterion 仅作相对比较,参考机逐次直方图仍待完成。

两腿都走 Part::Fast,无慢侧缓存更新、Advice 或 TargetIntent;`benches/not-applicable.toml` 为 slow_update、advice、target_set 登记理由,不产生空操作计时。Model 不适用。

## 测试与验收

用例与验收见 [TESTS.md](TESTS.md)。

## 依赖

运行时只依赖 hft-strategy-sdk。hft-strat-lib 已有构件不提供双腿价格加权费用与双侧库存 sizing,此处用有界整数函数实现。引擎测试在 registry,以真实 Strategies 驱动 hft-testkit SeqSut 与模拟撮合。禁止正式依赖引擎、其他策略、I/O、系统时钟、动态分派和浮点数。

## 延伸阅读

- [Windows 开发机前后测量](../../benchmarks/reports/2026-10-15-windows-cross-taker.md)(非参考机验收)
- [SDK](../../crates/hft-strategy-sdk/README.md)、[执行层](../../crates/hft-exec/README.md)、[OMS](../../crates/hft-oms/README.md)
- [策略规则](../AGENTS.md)、[上线门槛](../../docs/testing.md#8-上线门槛)
- [同时下单决定](../../.agents/notes/implemented/feature/2026-10-14-cross-taker-simultaneous.md)
- [范围与待评审取舍](../../.agents/notes/implemented/feature/2026-10-14-cross-taker-executable-spread.md)

## 已知限制与后续工作

- LeadLag/Advice 和无利润自动库存退出不在本实现;触发为可执行价差。基线差异见 docs/changes.md,待负责人评审。
- 非 dust 裸量等待引擎修复,现货费用 dust 按引擎阈值保留。
- 延迟验收、14 天模拟、费率与成交校准、零成交冒烟和正式实例 Shadow 仍是上线前工作。

### 开发备忘

待实测:在统一升级主机记录各事件入口的分段直方图;在模拟账户上校准 min_edge、slip、机会年龄与手续费舍入的影响。持仓期间资金费和库存退出规则待负责人确认。
