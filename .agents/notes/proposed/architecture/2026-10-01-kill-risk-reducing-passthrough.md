# Agent Note: 非 HARD kill 放行风险递减单

Status: proposed

> **第八轮(2026-10-02)**:收窄——HARD 不再只允许与 Global 组合,`Kill{Account(a), hard}` 按账户生效([one-account-per-strategy-instance](2026-10-02-one-account-per-strategy-instance.md));风险递减单同样受数量/名义上限、检查链 #5/#7/#8 与令牌桶预留份额 `rr_reserve` 约束,开仓链接的重试保留 `Second` 角色、不带 reduceOnly([exec-layer-targets](2026-10-02-exec-layer-targets.md))。

> **第八轮补充(2026-10-03)**:收窄第 6 条——解除 HARD 时的未对冲量仍标 `Broken`,但不再只能由 `HedgeFix` 处理:`Broken` 持续 `broken_auto_fix_ms` 后自动有界平仓(HARD 下暂停,解除后重给完整窗口);Viewer 可发非 HARD、不带 flatten 的 `Kill{Global | Account}`(带确认,不能解除);见 [broken-auto-fix](2026-10-03-broken-auto-fix.md)、[viewer-emergency-stop](2026-10-03-viewer-emergency-stop.md)。

## Problem

kill 开关要保证"切换瞬间已在出环里的开仓帧不漏出",所以 gw 线程在发送前检查 kill 字([09 §4.1](../../../../design/09-可观测性、风控与控制面.md))。最直接的实现是 kill 置位后拦截一切 `Place`/`Amend`。但 kill 常常发生在双腿交易的中间:第一腿已经成交,`LegLink` 的第二腿正在出环;或者 kill 本身要求 `CloseOnly` + 自动平仓。一刀切拦截会把**已成交的第一腿留成裸腿**,也让平仓单发不出去——这正是我们反复出事的形态(9/24 裸多、ARC 裸多,[postmortem](../../../../docs/postmortem/README.md))。kill 的目的是降低风险,不应该制造敞口。

## Proposal

语义与位定义归 [`hft-risk`](../../../../crates/hft-risk/README.md),kill 字的所有权见 [architecture.md §5](../../../../docs/architecture.md#5-状态所有权):

1. 每个 gw 线程读自己的 `KillBits` 原子字:`GLOBAL`、`VENUE`、`HARD`、各账户位。kill 级别只有 `Account` / `Venue` / `Global` + `HARD` 位;**没有策略级 kill**——策略层面的停止用 `SetMode`(第五轮 M1)。kill 是 cap 集合中的一类记录,kill 字由剩余 cap 重新计算([desired-mode-and-caps](2026-10-01-desired-mode-and-caps.md))。
2. sequencer 在 OMS 生成 `OutMsg` 时,只对以下意图置 `RISK_REDUCING` 标志:`LegLink` 对冲腿(数量 ≤ 触发它的第一腿实际成交量)、`HedgeFix` 补腿、reduce-only 的平仓/`flatten`。数量上限 = 当前待对冲量或持仓量,由 sequencer 按自己的状态判定;gw 不做分类(gw 看不到仓位,单写者)。
3. gw 发送 `Place`/`Amend` 前:`HARD` 置位 → 一律拒;否则 kill 生效且未带 `RISK_REDUCING` → 拒;拒绝回 `SendReport{KillBlocked}`。`Cancel`、`CancelAll`、`Query`、死手刷新永远放行。
4. 交易所侧 `reduceOnly` **只用于 `HedgeFix`、`Flatten` 等"平掉多出一腿"的单**;chain-first 的 `LegLink` 对冲在 CEX 上是**开仓**,不能带 `reduceOnly`(否则会被交易所拒),它的约束只来自数量上限 = 链上已包含的成交量。
5. 链上同理:`SwapIntent` 可带 `RISK_REDUCING`,chain-core 只在 `HARD` 下全拒。
6. **只有 `HARD` 能拦 `RISK_REDUCING` 单**:其余任何 cap——kill、`DRAINING`、时钟不可信、锁存 CloseOnly、条件门——都放行它。`HARD` 只由 `Kill{Global, hard}`(Trader)置位(人工判断交易所异常、定价错误或疑似 key 泄露),此时宁可留下裸腿也不再发任何单;置位即 P0 告警并列出当前未对冲量。解除需 `Resume{Kill, Global}` + 确认 + kill 后新对账 ≤ Warn;解除时的未对冲量标为 Broken,只能由 `HedgeFix` 命令处理,**解除本身不自动对冲**([adopt-exchange-manual-intervention](2026-10-01-adopt-exchange-manual-intervention.md))。

## Alternatives considered

**kill 后拦截一切下单。** 实现最简单,但已成交第一腿变成裸腿、自动平仓无法执行;把一个降风险的动作变成增风险的动作。

**只清空出环里的待发单,不在 gw 检查。** sequencer 与 gw 之间有竞态:清空时已被 gw 取走、正在加密的帧仍会发出;kill 字在发送前检查是唯一无竞态的点。

**为对冲单单开一条绕过 kill 的环。** 多一条环与一套优先级,分类问题并没有消失(仍要有人判定"哪些单进这条环"),还增加了漏出的路径。

**gw 自行判断订单是否降风险。** gw 没有仓位与待对冲量,只能按订单方向猜;违反状态只在 sequencer 的单写者规则。

## Acceptance criteria

- DST 场景"第一腿 `Fill` 与全局 kill 在同一轮到达":第二腿发出,数量 = 实际成交量,场景结束时无未对冲量([testing §4](../../../../docs/testing.md#4-必须在-dst-与属性测试中持续成立的不变量) 不变量 5、8)。
- DST 场景"`HARD` 置位":此后没有任何 `Place`/`Amend` 离开 gw;撤单全部发出。
- 属性测试:任何带 `RISK_REDUCING` 的 `OutMsg` 数量 ≤ 生成时的待对冲量或持仓量;无 `reduceOnly` 支持的场所(现货)以库存上限约束。

## Risks

- **分类错误放出开仓单**:`HedgeFix`/`Flatten` 有数量上限 + 交易所侧 `reduceOnly` + 属性测试三层约束;`LegLink` 对冲只有数量上限与属性测试两层(不能带 `reduceOnly`);现货没有 `reduceOnly`,只能靠库存上限,需在各 venue README 写明。
- **kill 期间仍有订单流**:运维看到 kill 后仍有成交可能误判 kill 失效;ops_bot 的 kill 回执列出放行的风险递减单。
- **`HARD` 会有意留下裸腿**:这是人工决定的代价,告警必须把裸腿清单推给操作者,由其手动处理。
