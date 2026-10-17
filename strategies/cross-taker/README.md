---
description: "跨所 taker 套利:扣除两侧费用和价格余量后,按深度与持仓限额发出 Simultaneous 链接订单"
kind: crate
tier: hot
scope: optional
status: impl
---

# cross-taker

## 概要

`CrossTaker` 在 sequencer 线程比较同一资产的两个 venue 的 BBO,只在永续 bid 相对现货 ask 的净边严格超过 min_edge 时做空永续、多现货。买入低价腿作为链接的 First,通过 SDK LegLink 让 OMS 在同一事件同时发出买卖两腿;策略不等待成交再发第二腿,不直接发修复订单。第 0 腿固定为协议 VenueId 1 的 USDT 线性永续,第 1 腿固定为 VenueId 11 的现货;现货只卖出本组持有且不属于 baseline 的库存来平仓。默认禁止开火,上线必须显式配置并通过仓库上线门槛。

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

registry 的 cross-taker feature 注册 CrossTaker,kind_id/schema_id = 3,version = 3;`Model = NoModel<XtParams>`。每组恰好两腿,同一 base/quote、相同定点指数、不同交易所。同一实例不允许两个 group 引用同一 inst,避免逐品种限额被重复组放大。账户必须覆盖两腿 venue;加载时拒绝无效引用、同所组合、不支持的市场、无效 tick/lot 和无效限额。两侧 BBO 都订阅 Part::Fast,状态只读 SDK Views。

### 参数 schema

所有字段 minor_change = None,参数变动按上线门槛重新评估。非零默认值均为暂定值。Notional 为 1e-6 quote 单位,Rate 为 1e-9;组必须使用相同 quote,加载时仅接受 USDT。

| 字段 | 单位 | 默认 | per_group | 含义 |
|---|---|---|---|---|
| enabled | Bool | false | 否 | 人工启用;默认不交易 |
| min_edge | Rate | 0 | 是 | 开仓净收益率严格下界;正式账户启用时必须 > 0,负值仅非正式账户测试可用 |
| close_edge | Rate | 0 | 是 | 平仓反向净收益率下界,包含等号;正式账户须 ≥ 0,负值仅非正式账户测试可用 |
| slip | Rate | 0 | 是 | 每腿 IOC 价格余量,0 为吃到触价 |
| depth_frac | Rate | 50% | 是 | 两侧对应顶档量的使用比例 |
| max_chunk_notional | Notional | 0 | 是 | 任一腿单笔上限;启用时必须 > 0 |
| max_position_notional | Notional | 0 | 是 | 任一腿绝对持仓上限;不得小于单笔上限 |
| max_quote_age_ms | DurationMs | 5 | 是 | 两侧报价机会年龄上限,等于上限即不交易 |
| cooldown_ms | DurationMs | 100 | 是 | 两次尝试间的 engine_ns 间隔;门控、Shadow 也消耗冷却 |
| max_hedge_failures | Count | 3 | 是 | 连续失败的双腿尝试达到此值请求 group halt |

`min_edge` 与 `close_edge` 的范围为 −10%..=10%,schema 标记 `prod_min = 0`。只有绑定账户 `is_prod = false` 才允许负值,用于 demo/testnet 的 TD 链路测量;账户标签、实例名、Shadow 或禁用状态均不能豁免正式账户。hft-cfg 检查默认值与所有组覆盖;策略 check、加载、新建和 SetParams 再检查默认值与有效组值。负默认值不能被正组覆盖隐藏,空组与 enabled=false 同样受限。正式账户启用时 min_edge=0 仍拒绝。

v3 更新参数边界与摘要,字段顺序和 body 布局保持;部署须用同一发布制品的 schema 重建 SPB1。快照布局保持,恢复接受 descriptor v1–v3。两腿 IOC、费用计算、cap、模式与引擎风险门控保持。测试配置与频率假设归 [demo 参数](../../deploy/testnet/README.md)。

费率读取 Views::fee().taker,由账户 refdata 费率档提供。负费率保守按零计,不计返佣收益。

### 信号与数量

令 B 为买腿 IOC 限价,S 为卖腿按相同余量计算的限价,fb/fs 为非负 taker 费率。触发条件为 `(S × (1 − fs) − B × (1 + fb)) / B > min_edge`,用 i128 计算、收益率向下取整。开仓只买 leg 1、卖 leg 0;持有空永续/多现货时,反向可执行净边 `>= close_edge` 才买回 leg 0、卖出 leg 1,优先处理减仓。反向信号不得产生新仓。两腿限价固定于本次观察的 BBO:买价不高于 ask × (1 + slip),卖价不低于 bid × (1 − slip),按 tick 保守取整。卖腿使用 PxMode::Limit;引擎风险价格带只能收紧限价,不因后续报价放宽。信号不是利润保证。

数量取两侧对应顶档量 × depth_frac、两侧 max_qty、每腿单笔名义余额、每腿持仓余量的最小值,再向下取到两腿 lot 的最小公倍数。名义上限以对应 ask 与限价的较大值计算;低于任一腿最小数量或最小名义则不交易。现货卖量还受 group 持仓与账户非 baseline 库存限制;可负担性和预留由引擎检查。

平仓两腿都减小绝对仓位时标 Reduce,数量不穿过零,永续腿带 reduceOnly;合法方向的加仓标 Open。超限腿仅允许向零减少。反向净边未达 close_edge 时不主动清仓,强制降仓和裸腿修复归引擎。

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

BBO 处理先完成轮次结算与 halt 投递,再复用本次 BBO 做健康、机会年龄、费用、限价和数量检查。close_edge 可独立于 min_edge,不能用开仓阈值提前过滤减仓。

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
- [demo 负净边测量决定](../../.agents/notes/implemented/feature/2026-10-17-demo-negative-edge.md)
- [产品形态决定](../../.agents/notes/implemented/feature/2026-10-16-perp-spot-shape.md)
- [同时下单决定](../../.agents/notes/implemented/feature/2026-10-14-cross-taker-simultaneous.md)
- [范围与待评审取舍](../../.agents/notes/implemented/feature/2026-10-14-cross-taker-executable-spread.md)

## 已知限制与后续工作

- LeadLag/Advice、最大持有时间与相对入场基差止损不启用;退出由 close_edge 与引擎风险处置决定。持有期限/基差止损已考虑,缺少持有期标定时保持关闭,不引入未经标定的强制亏损退出参数。
- 非 dust 裸量等待引擎修复,现货费用 dust 按引擎阈值保留。
- 延迟验收、14 天模拟、费率与成交校准、零成交冒烟和正式实例 Shadow 仍是上线前工作。

### 开发备忘

待实测:在统一升级主机记录各事件入口的分段直方图;在模拟账户上校准 min_edge、slip、机会年龄与手续费舍入的影响。持仓期间资金费成本及未来最大持有时间/基差止损参数需单独标定。
