---
description: "可选策略包:跨所 maker-taker(07 中的 B 类)——在 slow 所以 post-only 挂单被动成交,成交后在 fast 所 IOC 对冲;核心是 fast 价格一动就撤/改单;数据证明挂单 + 吃单有边、执行层 MakerFirst 与模拟挂单撮合就绪后启用"
kind: crate
tier: hot
scope: optional
status: spec
---

# maker-taker

## 概要

`MakerTaker` 在 slow 腿所在 venue 以 post-only 挂在 fast 腿中间价推出的目标价位,目标是被动成交、少付 taker 费并吃到价差;挂单一旦成交,立即在 fast 腿 IOC 反向对冲。它的盈亏由**撤单速度**决定:fast 价格一动,挂单必须立刻改价或撤掉,否则被逆向选择([07](../../design/07-策略层.md))。快速部分 `MakerTaker` 在 sequencer 线程跟随 fast 中间价维护一张挂单;本策略没有慢速部分(`Model = NoModel`)。它不拥有订单状态机与第二腿(OMS LegLink;执行层的 `MakerFirst` 落地后可改用目标意图)、风控与账本。本 README 不出现交易所名字:腿只用 fast/slow 角色,改单能力等差异来自 `VenueCaps`。**触发条件**见下节。

## 目录

- [使用本组件](#使用本组件)
- [实现说明](#实现说明)
- [延迟与资源契约](#延迟与资源契约)
- [测试与验收](#测试与验收)
- [依赖](#依赖)
- [延伸阅读](#延伸阅读)
- [已知限制与后续工作](#已知限制与后续工作)

## 使用本组件

### 触发条件(全部满足才启用)

1. **有边的证据**:CEX-CEX 可交易性分析(journal 数据 + 模拟账户)显示某个 venue 对上"slow 挂单 + fast 吃单对冲"扣除 maker/taker 费与逆向选择后为正。
2. **venue 已接入**:两腿 venue 都有 `venue-*` 实现;slow 腿 venue 支持 post-only(`VenueCaps.flags` 的 `POST_ONLY` 位),最好支持改单(`AMEND` 位)。
3. **模拟挂单撮合**:[hft-sim](../../crates/hft-sim/README.md) 的挂单撮合(保守:成交价穿过挂单价才成交)已实现,并有 L2 队列前方深度估计;模拟结果为正。
4. **主机**:在统一升级主机(主机类按 M3 末满订阅探针选定,见 [apps/hft-core](../../apps/hft-core/README.md))上通过延迟验收之后(撤单延迟决定逆向选择损失,dev2 无法评估)。
5. **slow 腿深度**:slow 腿 inst 订阅 `EvMask::BOOK`;分档快照流(SDK `EvMask::BOOK` 的默认来源,簿只有 N 档)足够估计队列前方深度,不需要全量增量深度(深度路由与档数归 [hft-venue](../../crates/hft-venue/README.md))。

### 装配与策略接口

```rust
// strategies/registry 的 strategy_set! 列表中已有一行(Cargo feature 门控);启用 = 发布构建打开 feature,app 与引擎 crate 零改动
#[cfg(feature = "maker-taker")] maker_taker::MakerTaker,

pub struct MakerTaker { inst: StrategyInstance, groups: Box<[MtGroup]>, p: MtParamsNs }
impl Strategy for MakerTaker {
    const DESC: StrategyDesc = StrategyDesc { name: "maker-taker", kind_id: 4, version: 1, non_prod_only: false,
                                              schema: &MtParams::SCHEMA, notes: &NOTES, report: &REPORT, cmds: &[] };   // src/desc.rs;不接受策略命令
    type Params = MtParams;
    type Model = NoModel<MtParams>;
    /* new / subscriptions / on_event / on_params / snapshot / restore:签名见 SDK */
}
```

`kind_id = 4`、`schema.id = 4`,登记于 [strategies/registry](../registry/README.md);`MtParams` 用 SDK 的 `define_params!` 声明。

### 组

| 腿 | 角色 | 说明 |
|---|---|---|
| `leg 0` | Fast | 对冲腿(吃单);价格参考 |
| `leg 1` | Slow | 挂单腿(post-only) |

两腿同一资产,且**在不同交易所**(跨所策略;同一账户内 refdata 不允许同所两个市场,`SetParams` 另有 `InvalidArgs{SameExchangeLegs}` 校验)。每个 group 每个方向至多一张挂单(参数 `sides` 决定只挂买、只挂卖或双边)。

### 订阅与事件处理

| `EventRef` | 处理 |
|---|---|
| `Bbo`(Fast) | 更新 `fast_mid`;单次跳动 > `cancel_on_move` → 立即撤单;否则重算目标价,偏离 > `requote` → 改单或撤后重挂 |
| `Book`(Slow) | 更新队列前方深度估计(只用簿里的档,目标价在簿外时按簿内最后一档之后的深度未知、视为超限);前方深度 > `max_queue_ahead` → 撤单或不挂 |
| `Bbo`(Slow) | 目标价会穿过对手价(post-only 会被拒)→ 退一个 tick 或不挂 |
| `Order` | `Acked` → 记挂单;`Filled(d)` → 对冲由 LegLink 自动发出,更新库存;`Cancelled`/`Aged` → 清挂单并按需重挂;`Rejected`(post-only 穿价)→ 本次不重试,等下一次 fast 变化;`Rejected` 含 `LINK_STALE`(fast 报价过旧)/`LINK_FUNDS`(对冲腿资金不足)等门控位 → 不计失败,前者等 fast 报价恢复、后者停挂并 `LinkFunds` Note |
| `Mode` / `Risk` / `Conn` | 降级或任一腿报价不健康(`v.quote_ok(a, inst)` 为假:连接断、重同步或静默超限;盘口不变不算)→ 撤全部挂单(Open 挂单由 sequencer 撤,策略同步状态);`Risk{HedgeBroken}` → 撤该 group 挂单、停挂;Broken 可能被执行层自动有界平仓或人工 `HedgeFix` 解除,解除后按实际两腿重算 `inv` 再恢复 |
| `External` | 外部成交改变库存:更新 `inv`,超 `max_inv` 走平库存 |
| `Manual` / `Warmup` | 不会收到(`cmds` 为空;无 Model) |

### 意图

| 动作 | 意图 | 类别 |
|---|---|---|
| 挂单 | slow `PostOnly`,`px = target`,`qty = min(max_chunk_notional / px, 库存余量)` 向零取整到两腿的 `pair_lot`([hft-exec 两腿 lot 对齐](../../crates/hft-exec/README.md#两腿-lot-对齐)的同一实现),`max_rest_ms`;`link = OnFill → fast IOC`(按成交净量,`px_mode = Cross`) | Open(库存增加方向)/ Reduce |
| 改价 | `VenueCaps.flags` 含 `AMEND` → `Ctx::amend(order, target, qty)`;否则 `cancel` + 新挂单(撤单确认前不发新单,避免双挂) | — |
| 撤单 | `Ctx::cancel` | — |
| 平库存 | `|inv| > max_inv` → fast 或 slow IOC 反向(Reduce) | Reduce |

执行层的 `ExecSpec::MakerFirst`([hft-exec](../../crates/hft-exec/README.md),预留)实现后,可改为输出目标意图由执行层挂单与对冲(本策略参数映射到 SDK `MakerParams`:`edge + cost` → `offset`、`requote` → `requote`、`max_rest_ms` → `max_rest_ms`、`max_chunk_notional` → `chunk_notional`;`cancel_on_move` 与队列深度门须由执行层同等支持后才能切换);在此之前用上表的直接订单意图(OMS 已支持 PostOnly + OnFill)。

### 参数 schema(`MtParams`)

| 字段 | 单位 | 默认 | 组 | 含义 |
|---|---|---|---|---|
| `sides` | Enum[both, bid, ask] | both | ✓ | 挂哪一侧 |
| `edge` | Rate | 0 = 未设(`check` 拒绝,启用前必须设定) | ✓ | 目标价相对 fast 中间价的额外边 |
| `requote` | Rate | 0 = 未设(`check` 拒绝,启用前必须设定) | ✓ | 挂单价偏离目标超过它才改价 |
| `cancel_on_move` | Rate | 0 = 未设(`check` 拒绝,启用前必须设定) | ✓ | fast 中间价单次跳动超过它立即撤 |
| `max_rest_ms` | DurationMs | 0 = 未设(`check` 拒绝,启用前必须设定) | ✓ | 最长挂单时间(OMS 到期发 `Aged`) |
| `max_queue_ahead` | Notional | 0 = 未设(`check` 拒绝,启用前必须设定) | ✓ | 队列前方深度上限 |
| `max_chunk_notional` | Notional | 0 = 未设(`check` 拒绝,启用前必须设定) | ✓ | 单张挂单名义上限 |
| `max_inv` | Notional | 0 = 未设(`check` 拒绝,启用前必须设定) | ✓ | 跨所净库存上限 |
| `requote_min_ms` | DurationMs | 0 | ✓ | 同一挂单两次改价最小间隔(受 slow venue 限速约束) |
| `stale_fast_ms` | DurationMs | 0 = 未设(`check` 拒绝,启用前必须设定) | | fast 腿连接静默上限:`!v.bbo(fast).quote_live(now, stale_fast_ms)` 即撤单(按连接健康,fast 盘口不变不撤;可比引擎阈值更严) |

成本 `cost = slow maker 费 + fast taker 费`(一次挂单成交 + 一次对冲的单程费用),`on_params` 时取 [`hft-strat-lib::fee_round_trip`](../../crates/hft-strat-lib/README.md)(slow 腿 `Liquidity::Maker`,maker 费可为负 = 返佣)结果的一半,费率来自 refdata 账户费率档,不是参数。阈值与上限类字段在 schema 中标 `relax`(`edge` 变小是放宽;`cancel_on_move`、`requote`、`max_queue_ahead`、`max_chunk_notional`、`max_inv`、`max_rest_ms`、`stale_fast_ms` 变大是放宽);本策略没有 Model,没有结构参数。

### Note 码表与报表字段

即 `DESC.notes` / `DESC.report`([SDK 策略描述符](../../crates/hft-strategy-sdk/README.md#策略描述符note报表策略命令));启用时按模拟结果补充。

| code | name | 级别 | v[] | 产生时机 |
|---|---|---|---|---|
| 1 | `QuoteFilled` | Info | v0 = 侧(1 买、2 卖),v1 = 成交名义,v2 = 挂单时长 ms | 挂单成交 |
| 2 | `CancelOnMove` | Info | v0 = fast 中间价跳动(Rate) | 跳动撤单 |
| 3 | `PostOnlyReject` | Info | v0 = 侧 | post-only 穿价被拒 |
| 4 | `LinkFunds` | P2 | v0 = `RejectBits` | 挂单因对冲腿资金不足被拒 |
| 5 | `InvLimit` | P2 | v0 = `inv` 名义 | 库存超限、发平库存 |

| 报表字段 | 单位 | 聚合 | 范围 | 来源 |
|---|---|---|---|---|
| `fills` / `fill_notional` | Count / Notional | Count / Sum | Group | `QuoteFilled` / v1 |
| `rest_ms_p99` | DurationMs | P99 | Group | `QuoteFilled` v2 |
| `cancels_on_move` / `postonly_rejects` / `inv_limits` | Count | Count | Group | `CancelOnMove` / `PostOnlyReject` / `InvLimit` |

### 运行模式

各模式下意图的处理以 [SDK 运行模式行为表](../../crates/hft-strategy-sdk/README.md) 为准;降到 CloseOnly/Shadow 时 Open 挂单由 sequencer 撤销。上线路径同其他可选策略:模拟账户 → 实盘账户 Shadow → `SetMode → Live`。

## 实现说明

<details><summary>实现要点</summary>

### 状态(`MtGroup`,≤ 256 B)

每侧一个 `rest: Option<OrderRef>`、`quote_px: Px`、`pending_cancel: bool`、`last_requote_ens: i64`;`target_px: [Px; 2]`、`last_fast_mid: Px`、`queue_ahead: [Notional; 2]`、`thr: Throttle`、`inv: Qty`。跨事件时刻全部是 engine_ns,快照里没有 TSC;`restore` 后不假设挂单仍按原价有效,按恢复后的订单状态与新行情重算目标价。

### 目标价与改单

1. fast `Bbo` → `mid = (bid + ask) / 2`;`|mid − last_fast_mid| / last_fast_mid > cancel_on_move` → 撤两侧挂单,`last_fast_mid = mid`,返回。
2. 目标:买侧 `target_bid = floor_to_tick(mid × (1 − (edge + cost)))`,卖侧 `target_ask = ceil_to_tick(mid × (1 + (edge + cost)))`(slow 的 tick;取整向不利于成交、有利于边的一侧)。
3. 目标会穿过 slow 对手价 → 退到对手价内一个 tick;仍不满足 post-only → 不挂。
4. 有挂单且 `|quote_px − target| / target > requote` 且 `now.engine_ns − last_requote_ens ≥ requote_min_ms` → 改价(按 `VenueCaps.flags` 的 `AMEND` 位);无挂单且队列前方深度 ≤ `max_queue_ahead`、库存余量 > 0、`Throttle::can_fire(Open)` → 挂单。
5. fast 腿连接不健康(`!v.bbo(fast).quote_live(now.tsc, stale_fast_ms × now.tsc_per_ms)`)或任一腿 `quote_ok` 为假 → 撤两侧挂单。fast 盘口长时间不变而连接健康时挂单保留(目标价没动)。

### 成交与库存

挂单 `Filled(d)`:OMS 按 LegLink 在同一事件内发出 fast IOC 对冲;本策略更新 `inv` 并按需重挂剩余量;`|inv| × mark > max_inv` → 停止增加库存方向的挂单并发平库存 IOC。

### 规划源文件

| 文件 | 职责 |
|---|---|
| `src/lib.rs` | `MakerTaker`、`DESC` |
| `src/params.rs` | `define_params!` 声明 `MtParams`;`check` 校验必填项 |
| `src/fast/{quote,inventory}.rs` | 目标价、挂/改/撤、库存(`#![deny(clippy::float_arithmetic)]`) |
| `tests/scenarios/*.rs` | SDK 场景测试 |

</details>

## 延迟与资源契约

快速部分计入分段 `decided_tsc − proc_tsc` 内的策略份额;撤单路径是本策略的关键路径,基准在[参考机](../../benchmarks/README.md#参考机与-ci)上判定,延迟验收在统一升级主机上进行。

| 入口 | p50 | p99 | 基准名 |
|---|---|---|---|
| `on_event` 快侧未命中(fast 变动未越过 `requote`/`cancel_on_move`) | ≤ 15 ns | ≤ 40 ns | `strategies/maker-taker/tick_miss` |
| `on_event` 命中 → 改价/挂单意图 | ≤ 200 ns | ≤ 600 ns | `strategies/maker-taker/tick_fire` |
| `on_event` fast 跳动 → 撤单(本策略额外登记,关键路径) | ≤ 80 ns | ≤ 250 ns | `strategies/maker-taker/tick_cancel` |
| `on_event` 慢侧更新(slow `Book`/`Bbo` → 队列前方深度、穿价检查) | ≤ 60 ns | ≤ 150 ns | `strategies/maker-taker/slow_update` |
| `on_event(Advice)` | 不适用(无 Model) | — | — |
| `on_event` → 构造 `TargetIntent` | 不适用(`MakerFirst` 落地前不发目标意图) | — | — |
| `on_event(Exec)` | ≤ 80 ns | ≤ 250 ns | `strategies/maker-taker/exec_update` |
| `on_event(Risk)`,每个受影响 group | ≤ 150 ns | ≤ 400 ns | `strategies/maker-taker/risk_event` |
| `Model::on_event` 采样 / 模型 tick | 不适用(`NoModel`) | — | — |

零分配、无系统调用、无 `dyn`、无 f64。

## 测试与验收

- **场景测试**(SDK `MockViews`;需要成交与故障的引擎级场景在 `strategies/registry/tests/maker_taker_*.rs`):fast 小幅变动不改价;fast 盘口 10 s 不变而连接健康 → 挂单保留;fast 连接静默超过 `stale_fast_ms` → 撤两侧;重启后改价间隔按 engine_ns 继续;越过 `requote` → 支持改单的 venue 发 amend、不支持的撤后重挂且撤单确认前不双挂;单次跳动 > `cancel_on_move` → 同一事件撤单;队列前方过深 → 不挂;post-only 穿价被拒 → 不重试;`Aged` 到期重挂;部分成交按净量对冲;库存超限 → 停挂并平库存;fast 过期或断连 → 撤全部;Mode 降级 → 挂单由 sequencer 撤、策略状态同步;`Rejected(LINK_STALE)` 不计失败、`LINK_FUNDS` 停挂并 Note;`HedgeBroken` → 撤挂停挂,Broken 解除后按实际两腿重算 `inv`;每个登记的 Note 码至少触发一次。
- **属性**:任意事件序列下每侧至多一张活动挂单(含撤单在途);`|inv|` 不超过 `max_inv` + 一张挂单量。
- **模拟**:模拟账户(保守挂单撮合)报告成交率、对冲滑点、成交后 1 s/5 s 的逆向选择;正式实盘(Shadow 观察期之后)与并行的同参数模拟账户对比校准。
- **门槛 ⑦ 的小改判定**:启用评估时在参数表为每个字段填 `FieldDesc.minor_change`(规则见 [testing ⑦](../../docs/testing.md#8-上线门槛));未填的字段取 `None`,任何改动都重新计时。
- **验收**(启用时):触发条件 1–5 成立;参考机上基准满足上表。

## 依赖

- 内部:[`hft-strategy-sdk`](../../crates/hft-strategy-sdk/README.md)、[`hft-strat-lib`](../../crates/hft-strat-lib/README.md)(`Throttle`、`fee_round_trip`)。
- 外部:无。
- 禁止:任何引擎 crate、其他 `strategies/*`;I/O、`std::time`、`HashMap` 迭代、f64、`dyn Trait`。

## 延伸阅读

- [07 策略层](../../design/07-策略层.md)、[05 CEX 执行层](../../design/05-CEX执行层.md)
- [hft-exec](../../crates/hft-exec/README.md)(`MakerFirst` 预留)、[hft-sim](../../crates/hft-sim/README.md)(挂单撮合)、[strategies/AGENTS.md](../AGENTS.md)
- 第八轮裁决:[round8-decisions](../../design/_work/round8-decisions.md)

## 已知限制与后续工作

- 不建 L3/队列位置模型;只用 L2 前方深度估计。需要更准的成交概率时随 `hft-sim` 的可选队列模型 feature 一起引入。
- 不做做市(双边持续报价、库存偏斜报价);只做"挂单 + 立即对冲"。
- 本策略没有 Model;若需要按时段/波动调整 `edge`,作为 Model 加入(输出参数建议 Advice)。

### 开发备忘

<details><summary>给实现者</summary>

- 必填参数在启用评估时用多个并行模拟账户确定(不做历史回测)。
- **待核实**:各候选 slow venue 的 post-only 穿价拒单码、改单是否保留队列位置、改单与撤单的限速权重;核实方法见对应 `venue-*` README 的开发备忘,结论写进 `VenueCaps`。
- 实现顺序:params → quote(无改单路径)→ amend 路径 → 队列深度 → 库存 → 场景测试与基准。
</details>
