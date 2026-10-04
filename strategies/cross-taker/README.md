---
description: "可选策略包:跨所 taker-taker(07 中的 A 类)——快所行情先动、慢所报价过时的几百微秒到几毫秒内,在慢所吃单并在快所对冲;慢速部分用领先-滞后估计决定是否就绪;领先-滞后证据成立且统一升级主机通过延迟验收后启用"
kind: crate
tier: hot
scope: optional
status: spec
---

# cross-taker

## 概要

`CrossTaker` 利用同一资产在两个 venue 之间的**传播延迟**:领先的一腿(fast)报价先动,滞后的一腿(slow)报价在一个短窗口内仍是旧价,此时在 slow 上 IOC 吃单、在 fast 上反向对冲。它是唯一真正依赖微秒级主机内延迟的策略族,所有低延迟机制的收益在这里兑现([07](../../design/07-策略层.md))。快速部分 `CrossTaker` 在 sequencer 线程用预计算触发价做一次整数比较,不做确认窗口;慢速部分 `CrossTakerModel` 在 advisor 线程用 [hft-strat-lib `LeadLag`](../../crates/hft-strat-lib/README.md) 持续估计两腿的领先方向、滞后与可用窗口,以 Advice 告诉快速部分是否就绪(可以开火)。它不拥有第二腿状态机(OMS 的 LegLink)、风控与账本;本 README 不出现交易所名字,腿只用 fast/slow 角色,交易所差异来自 refdata 与 `VenueCaps`。**触发条件**见下节。

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

1. **领先-滞后证据**:在服务器录下的 journal 上,对候选 venue 对,`LeadLag` 的 `confidence ≥ min_conf` 且可用窗口 `window_ns` 大于 slow 报价年龄上限的时间占比 ≥ 评估时设定的比例,连续多日成立;两所几乎同步(例如都迁到同一机房后)则不启用。
2. **venue 已接入**:fast 与 slow 两个 venue 都已有 `venue-*` 实现(core 或已启用的 optional venue)。
3. **主机**:在统一升级主机(主机类按 M3 末满订阅探针选定,见 [apps/hft-core](../../apps/hft-core/README.md))上通过延迟验收之后;dev2 只能做功能验证,不能证明本策略有边。
4. **模拟账户**:在实时行情上用已按正式实盘数据校准的延迟模型运行([hft-sim](../../crates/hft-sim/README.md) 与正式实盘并行校准),结果为正且差异有解释。

### 装配与策略接口

```rust
// strategies/registry 的 strategy_set! 列表中已有一行(Cargo feature 门控);启用 = 发布构建打开 feature,app 与引擎 crate 零改动
#[cfg(feature = "cross-taker")] cross_taker::CrossTaker,

pub struct CrossTaker { inst: StrategyInstance, groups: Box<[XtGroup]>, p: XtParamsNs }
impl Strategy for CrossTaker {
    const DESC: StrategyDesc = StrategyDesc { name: "cross-taker", kind_id: 3, version: 1, non_prod_only: false,
                                              schema: &XtParams::SCHEMA, notes: &NOTES, report: &REPORT, cmds: &[] };   // src/desc.rs;不接受策略命令
    type Params = XtParams;
    type Model = CrossTakerModel;
    /* new / subscriptions / on_event / on_params / snapshot / restore:签名见 SDK */
}
pub struct CrossTakerModel { ll: Box<[LeadLag]>, fp: Box<[u64]> }   // 每 group 一个 LeadLag 与该 group 的 GroupSpec 指纹
impl Model for CrossTakerModel {
    type Params = XtParams; const LAYOUT_VERSION: u32 = 1;
    const LAYOUT: &'static ModelLayoutDesc = &LAYOUT;   // 原始样本节:一个 Group 节,GroupId、指纹、LeadLag::LAYOUT_ELEMS
    const PATH_VERSION: u32 = 0;                        // 无路径状态:累加器由变动环重算,每个模型 tick(UTC 对齐)输出一次
    // mem_bytes = n_groups × LeadLag::mem_bytes(cfg);snapshot_bytes = n_groups × (16 + LeadLag::snapshot_bytes(cfg))
    // path_settle_ms = 0;cold_start_ms = ll_window_ms(窗口填满前样本不足)
    /* new / on_params / on_event / snapshot_chunk / restore:签名见 SDK */
}
```

`kind_id = 3`、`schema.id = 3`,登记于 [strategies/registry](../registry/README.md);`XtParams` 用 SDK 的 `define_params!` 声明。`LeadLag` 只需 `ll_window_ms`(60 s)的数据,冷启动几分钟即就绪,不定义预热载荷。

### 组

| 腿 | 角色 | 说明 |
|---|---|---|
| `leg 0` | Fast | 领先 venue 的该资产(永续或现货) |
| `leg 1` | Slow | 滞后 venue 的同一资产(永续或现货) |

两腿必须是同一资产(同一价格指数,跨 venue 比较不换算),且**在不同交易所**(本策略本来就是跨所;同一账户内 refdata 不允许同所两个市场,`SetParams` 另有 `InvalidArgs{SameExchangeLegs}` 校验);现货腿只能卖出已有库存(方向约束由风控与 `VenueCaps` 执行)。哪一腿是 fast 由参数给出,`LeadLag` 测出的方向与之不符时该 group 不就绪。

### 订阅与事件处理

| `EventRef` | 处理 |
|---|---|
| `Bbo`(Slow) | 重算 `TriggerPx`(slow ask/bid → fast 需到达的价位);记 slow 顶档量 |
| `Bbo`(Fast) | `hit_buy/hit_sell` 不命中即返回;命中 → 开火前检查 → 意图 |
| `Order` | 推进 `in_flight`、库存 `inv`、`Throttle::on_outcome`(`Rejected` 含 `LINK_STALE`/`LINK_FUNDS` 等门控位 → `Gated`,不计失败) |
| `Advice` | 解码 `LeadLagEst` 进 `Expiring` 缓存(有效期从 `basis_tsc` 起算);不就绪 → `TriggerPx::invalidate` |
| `Mode` / `Risk` / `Conn` | 重置开仓状态;halted 镜像;任一腿报价不健康(`v.quote_ok(a, inst)` 为假:连接断、重同步或静默超限;盘口不变不算)→ 不就绪;`Risk{HedgeBroken}` → 该 group 不就绪,直到按实际两腿重新计算的 `inv` 回到 `max_inv` 内且 Broken cap 解除(Broken 可能被执行层自动有界平仓或人工 `HedgeFix` 解除,不假设仓位不变) |
| `External` | 外部成交改变库存:更新 `inv`,超 `max_inv` 走反向平库存 |
| `Manual` / `Warmup` | 不会收到(`cmds` 为空;无预热载荷) |

订阅:两腿 `BBO` 用 `SubsBuilder::market(.., Part::Both)`(Model 也要);`SubsBuilder::model_tick(ll_period_ms)`。`CrossTakerModel` 在每个 `Bbo` 上用该 Bbo 的接收时刻喂 `LeadLag::observe`:一律 `v.now().utc_of(b.rx_tsc)`(两腿同一时钟,确定;不用 PHC 域的 `hw_rx_ns`,见 [SDK 时间轴](../../crates/hft-strategy-sdk/README.md#时间轴));在每个模型 tick 上调 `estimate` 并 `AdviceOut::emit`。`on_params` 中新 GroupId 或指纹变化的 group、`ll_bucket_ns`/`ll_max_lag_ns`/`ll_window_ms`(结构参数)变化 → 该 group 的 `LeadLag` 重建并计入 `ModelParamEffect`(SDK [参数变化时的 group 重置](../../crates/hft-strategy-sdk/README.md#参数变化时的-group-重置));冷启动几分钟即就绪,不需要预热。

### 意图

| 动作 | 意图 | 类别 |
|---|---|---|
| fast 上涨、slow 卖价过时 | slow IOC 买:`px = slow_ask × (1 + slip)`,`qty = min(slow 顶档量 × depth_frac, max_chunk_notional / px, 库存余量)`;对冲按 `hedge_mode` | Open(库存增加方向)/ Reduce(库存减少方向) |
| fast 下跌、slow 买价过时 | slow IOC 卖:`px = slow_bid × (1 − slip)`,其余对称 | 同上 |
| 对冲 `hedge_mode = simultaneous` | slow IOC 带 `link = LegLink{hedge: HedgePlan{kind: Simultaneous, on_fail: Reverse, …}}`(第二腿 = fast 反向 IOC);第二腿由 OMS 在 slow 单 `place` 的同一事件产出,策略只发 slow 一条 | 第二腿由风控按 **Open** 分类(尚无成交,不是风险递减;两腿 venue 同查开仓类检查,分类表归 [hft-risk](../../crates/hft-risk/README.md));失败后的反向单按 Hedge |
| 对冲 `hedge_mode = on_fill` | slow IOC 带 `link = OnFill → fast IOC`(按 slow 实际成交净量,`px_mode = Cross`);第二腿由 OMS 产出 | 第二腿由风控按 Hedge 分类 |
| 平库存 | `|inv| > max_inv` 或反向价差 ≥ `unwind_th` → 反向同构交易 | Reduce |

两腿残余由执行层按其补腿规则处理([hft-exec](../../crates/hft-exec/README.md));本策略不自行补腿。哪种 `hedge_mode` 更好由模拟账户与正式实盘(Shadow 观察期之后)的数据决定。

### 参数 schema(`XtParams`)

| 字段 | 单位 | 默认 | 组 | 含义 |
|---|---|---|---|---|
| `fast_leg` | Enum[leg0, leg1] | leg0 | ✓ | 预期领先的腿 |
| `theta_open` | Rate | 0 = 未设(`check` 拒绝,启用前必须设定) | ✓ | 触发价差(含两腿手续费);`validate` 要求 ≥ 两腿 taker 往返(refdata 费率档) |
| `slip` | Rate | 0 = 未设(`check` 拒绝,启用前必须设定) | ✓ | slow IOC 越价 |
| `depth_frac` | Rate | 50% | ✓ | 单笔 ≤ slow 顶档量 × 比例 |
| `min_top_qty` | Qty | 0 | ✓ | slow 顶档量下限 |
| `max_chunk_notional` | Notional | 0 = 未设(`check` 拒绝,启用前必须设定) | ✓ | 单笔名义上限 |
| `max_slow_age_ms` | DurationMs | 5 ms | ✓ | slow 最近一条报价的年龄上限(`now.tsc − v.bbo(slow).rx_tsc`,本策略的机会窗口,不是新鲜度判定);实际取 min(它, `LeadLag.window_ns`) |
| `min_conf` | Rate | 0 = 未设(`check` 拒绝,启用前必须设定) | | `LeadLag.confidence_ppm` 下限(按 Rate 比较:1 ppm = 1 000) |
| `hedge_mode` | Enum[simultaneous, on_fill] | simultaneous | ✓ | |
| `max_inv` | Notional | 0 = 未设(`check` 拒绝,启用前必须设定) | ✓ | 跨所净库存上限 |
| `unwind_th` | Rate | 0 = 未设(`check` 拒绝,启用前必须设定) | ✓ | 反向价差达到即平库存 |
| `cooldown_ms` | DurationMs | 0 | ✓ | 同 group 两次开火最小间隔 |
| `ll_bucket_ns` | Count | 50,000 | | `LeadLag` 分箱 50 µs(结构参数) |
| `ll_max_lag_ns` | Count | 5,000,000 | | 滞后搜索范围 ±5 ms(结构参数) |
| `ll_window_ms` | DurationMs | 60 s | | 互相关滚动窗口(结构参数) |
| `ll_period_ms` | DurationMs | 5 s | | 输出周期 |
| `ll_valid_ms` | DurationMs | 30 s | | Advice 有效期 |

"0 = 未设"的字段在启用时按证据与模拟结果设定;`ll_*` 的默认值待用 journal 实测标定(开发备忘)。结构参数在 schema 中标 `structural`;阈值类字段(`theta_open`、`slip`、`unwind_th`、`max_inv`、`max_chunk_notional`)标 `relax`。

### Advice 载荷

| `local_kind` | 名称 | 周期 | 有效期 | 载荷 |
|---|---|---|---|---|
| 1 | `LeadLag` | `ll_period_ms`(5 s) | `ll_valid_ms`(30 s) | 版本字节(1)+ [`LeadLagEst`](../../crates/hft-strat-lib/README.md) |

载荷实现 SDK `AdvicePayload`;Advice 没有有效期字段,快速部分以 `EventRef::Advice.basis_tsc`(Model 依据的数据时刻)+ `ll_valid_ms` 判断过期(`Expiring::put_advice`),依据上次启动数据的(`basis_seq < Now.boot_seq`)一律过期。advice 过期、`lead_leg` 与 `fast_leg` 不符、`confidence_ppm < min_conf` → 该 group 不就绪(过期即保守)。

### Note 码表与报表字段

即 `DESC.notes` / `DESC.report`([SDK 策略描述符](../../crates/hft-strategy-sdk/README.md#策略描述符note报表策略命令));启用时按模拟结果补充。

| code | name | 级别 | v[] | 产生时机 |
|---|---|---|---|---|
| 1 | `Fired` | Info | v0 = 方向(1 买 slow、2 卖 slow),v1 = 触发时 slow 报价年龄(µs),v2 = 单笔名义,v3 = `hedge_mode` | 每次开火 |
| 2 | `NotReady` | Info | v0 = 原因(1 过期、2 方向不符、3 置信度不足、4 断连、5 Broken);`Note.group` = 该 group | 就绪状态由真变假时 |
| 3 | `Unwind` | P2 | v0 = `inv` 名义,v1 = 触发(1 超 `max_inv`、2 反向价差) | 平库存 |

| 报表字段 | 单位 | 聚合 | 范围 | 来源 |
|---|---|---|---|---|
| `fires` | Count | Count | Group | `Fired` |
| `slow_age_p99` | Count(µs) | P99 | Group | `Fired` v1 |
| `fire_notional` | Notional | Sum | Group | `Fired` v2 |
| `not_ready` / `unwinds` | Count | Count | Group | `NotReady` / `Unwind` |

### 运行模式

各模式下意图的处理以 [SDK 运行模式行为表](../../crates/hft-strategy-sdk/README.md) 为准。上线路径:模拟账户 → 实盘账户以 `default_mode = Shadow` 观察(意图完整过风控但不出环)→ 运营看过 LeadLag 证据与模拟/影子结果后 `SetMode → Live`。

## 实现说明

<details><summary>实现要点</summary>

### 状态(`XtGroup`,≤ 256 B)

`trig: TriggerPx`、`ll: Expiring<LeadLagEst>`、`thr: Throttle`、`inv: Qty`(跨所净库存,带符号)、`in_flight: bool`、`slow_top: (Qty, Qty)`、`ready: bool`、`fp: u64`(GroupSpec 指纹)。快照里只有 engine_ns(hft-strat-lib 构件内部)与数量,没有 TSC;slow 报价的时刻不进状态,开火时从 `v.bbo(slow).rx_tsc` 现读(本次启动内的 TSC)。`restore` 后触发价失效、`ready = false`,等新的 LeadLag Advice(重启前的 Advice 按 `basis_seq < boot_seq` 过期)。`on_params` 中新 GroupId 或指纹变化的 group 状态从空起步。

### 触发与开火(无确认窗口)

1. slow `Bbo`:`TriggerPx::update(buy_px_slow = slow_ask, sell_px_slow = slow_bid, θ_open = theta_open, θ_close = −theta_open, tick_fast)`。即 fast bid ≥ slow ask × (1 + θ) 时在 slow 买;fast ask ≤ slow bid × (1 − θ) 时在 slow 卖。
2. fast `Bbo`:`hit_buy/hit_sell`;不命中返回(绝大多数 tick 的全部开销)。
3. 命中后依次:`ready`(LeadLag 有效、方向一致、置信度达标);两腿 `v.quote_ok(a, inst)`(连接健康);`now.tsc − v.bbo(slow).rx_tsc < min(max_slow_age, ll.window_ns)`(slow 报价仍是旧价的机会窗口;`rx_tsc = 0` 即未知,不开火);slow 顶档量 ≥ `min_top_qty`;`Throttle::can_fire`;库存余量 > 0(方向使 `|inv|` 增加时受 `max_inv` 限制);数量向零取整到两腿的 `pair_lot`(hft-types 数量取整模块的同一实现,规则见 [hft-exec 两腿 lot 对齐](../../crates/hft-exec/README.md#两腿-lot-对齐)),使对冲腿能按成交量整笔下出,且 ≥ 两腿各自的最小下单量与最小名义。
4. 构造带 LegLink 的 slow IOC(见意图表;第二腿由 OMS 产出);`on_fire`。
5. A 类的"确认"是 `LeadLag` 的统计置信度,不是确认窗口:确认窗口给慢信号用,这里每多等一个 tick 边就可能消失。

### 平库存

每次 `Order` 终态后检查 `|inv| × mark > max_inv` 或反向价差(按当前两腿 BBO)≥ `unwind_th` → 发反向同构交易(slow IOC + fast 对冲,类别 Reduce);库存归零后恢复正常开火。

### 规划源文件

| 文件 | 职责 |
|---|---|
| `src/lib.rs` | `CrossTaker`、`CrossTakerModel`、`DESC` |
| `src/params.rs` | `define_params!` 声明 `XtParams`;`check` 校验 `theta_open` ≥ 两腿 taker 往返 |
| `src/fast/{fire,unwind}.rs` | 开火检查与意图、平库存(`#![deny(clippy::float_arithmetic)]`) |
| `src/model/leadlag.rs` | 驱动 hft-strat-lib `LeadLag` |
| `src/advice.rs` | `LeadLag` 载荷(`AdvicePayload`) |
| `tests/scenarios/*.rs` | SDK 场景测试 |

</details>

## 延迟与资源契约

快速部分计入分段 `decided_tsc − proc_tsc` 内的策略份额;本策略是延迟最敏感的消费者,基准在[参考机](../../benchmarks/README.md#参考机与-ci)上判定,延迟验收在统一升级主机上进行。

| 入口 | p50 | p99 | 基准名 |
|---|---|---|---|
| `on_event` 快侧未命中(fast `Bbo`) | ≤ 15 ns | ≤ 40 ns | `strategies/cross-taker/tick_miss` |
| `on_event` 命中 → 一条带 LegLink 的订单意图 | ≤ 300 ns | ≤ 1 µs | `strategies/cross-taker/tick_fire` |
| `on_event` 慢侧更新(slow `Bbo` → 重算触发价) | ≤ 60 ns | ≤ 150 ns | `strategies/cross-taker/slow_update` |
| `on_event(Advice)` 解码并缓存 | ≤ 100 ns | ≤ 300 ns | `strategies/cross-taker/advice` |
| `on_event` → 构造 `TargetIntent` | 不适用(本策略不发目标意图) | — | — |
| `on_event(Exec)` | ≤ 80 ns | ≤ 250 ns | `strategies/cross-taker/exec_update` |
| `on_event(Risk)`,每个受影响 group | ≤ 150 ns | ≤ 400 ns | `strategies/cross-taker/risk_event` |
| `Model::on_event` 行情采样(`LeadLag::observe`) | ≤ 1 µs | ≤ 5 µs | `model/cross-taker/sample` |
| `Model::on_event` 模型 tick(全部 group 的 `estimate` + emit) | ≤ 300 µs | ≤ 1 ms | `model/cross-taker/tick` |

快速部分零分配、无系统调用、无 `dyn`、无 f64;Model 缓冲启动时按 group 数分配(每对两条变动环 + L 个累加器,见 [hft-strat-lib](../../crates/hft-strat-lib/README.md)),总量即 `mem_bytes` 的声明,hft-core 按声明装配;group 多到单个模型 tick 超过 1 ms 时按 gix 分片;检查点只存变动环。

## 测试与验收

- **场景测试**(SDK `MockViews`;需要成交与故障的引擎级场景在 `strategies/registry/tests/cross_taker_*.rs`):命中 → slow IOC + 对冲(两种 `hedge_mode`);fast 腿盘口长时间不变而连接健康 → 仍就绪(不判过期);任一腿连接静默超限 → 不就绪;重启后 `Throttle` 冷却按 engine_ns 继续、重启前的 LeadLag Advice 过期;slow 报价年龄 ≥ 窗口 → 不开火;LeadLag 过期(按 `basis_tsc`,到达时刻新也算过期)/ 方向翻转 / 置信度不足 → 不就绪;第一腿被 `LINK_STALE` 拒 → 不计失败;`HedgeBroken` → 不就绪,Broken 解除后按实际两腿重算 `inv`;库存超限 → 平库存;两种 `hedge_mode` 下策略都只发一条订单意图(第二腿来自 OMS);Simultaneous 第二腿失败 → OMS 反向;OnFill 部分成交按净量对冲;连接断 → 不就绪;Mode 事件后状态重置;halted 只拦开仓。
- **属性**:`hit_buy` ⇔ `SpreadCalc::buy ≥ θ`(取整方向保守);任意事件序列下 `|inv|` 不超过 `max_inv` + 一笔。
- **DST**:mock 两所 + 故障注入(断连、Unknown、限速、一腿拒单),断言裸敞口有界。
- **模拟与校准**:模拟账户报告成交率、两腿间隔、相对到达价的滑点;正式实盘(Shadow 观察期之后)与并行的同参数模拟账户逐项对比并回写延迟模型。
- **Model**:布局金样(`fixtures/strategies/cross-taker/ckpt/layout.json`)与 `check_layout`;实际分配 ≤ `mem_bytes`;改 `ll_bucket_ns` → 全部 group 报告重置;LeadLag 的时刻一律取 `utc_of(rx_tsc)`(`hw_rx_ns` 与 `rx_ts_src` 取任意值时结果相同);在不同位置检查点 → 恢复 → 继续,输出与连续运行逐字节相同。
- **门槛 ⑦ 的小改判定**:启用评估时在参数表为每个字段填 `FieldDesc.minor_change`(规则见 [testing ⑦](../../docs/testing.md#8-上线门槛));未填的字段取 `None`,任何改动都重新计时。
- **验收**(启用时):触发条件 1–4 成立;参考机上基准满足上表;影子期意图与 LeadLag 报告经运营确认。

## 依赖

- 内部:[`hft-strategy-sdk`](../../crates/hft-strategy-sdk/README.md)、[`hft-strat-lib`](../../crates/hft-strat-lib/README.md)(`TriggerPx`、`Throttle`、`Expiring`、`SpreadCalc`、`fee_round_trip`、`LeadLag`)。
- 外部:无。
- 禁止:任何引擎 crate、其他 `strategies/*`;I/O、`std::time`、`HashMap` 迭代、快速部分 f64、`dyn Trait`。

## 延伸阅读

- [07 策略层](../../design/07-策略层.md)、[03 网络与传输层](../../design/03-网络与传输层.md)、[00 目标与延迟预算](../../design/00-目标与延迟预算.md)
- [hft-strat-lib LeadLag](../../crates/hft-strat-lib/README.md)、[hft-sim](../../crates/hft-sim/README.md)、[strategies/AGENTS.md](../AGENTS.md)
- 第八轮裁决:[round8-decisions](../../design/_work/round8-decisions.md)

## 已知限制与后续工作

- 只做两腿同一资产;三角或跨资产不在范围。
- 现货腿方向受库存约束(不借币);双向自由的组合只在两腿都是永续时成立。
- 模拟账户对本策略的评估依赖延迟模型质量;延迟模型未经正式实盘成交数据校准(hft-sim 与正式实盘并行校准)时,模拟结果不作为启用依据。

### 开发备忘

<details><summary>给实现者</summary>

- `ll_bucket_ns` 与 `ll_max_lag_ns`:待标定(方法:用服务器录下的 journal 实测 mid 变动间隔分布标定)。现有领先-滞后证据只覆盖部分 core venue 对,且来自单一时期:待核实(方法:启用前在服务器上对候选 venue 对重新录数,按触发条件 1 评估)。
- `theta_open`、`slip`、`max_inv` 等必填项在启用评估时用模拟账户扫几组并行模拟账户确定(不做历史回测)。
- 实现顺序:params → fire(simultaneous)→ advice/Model → on_fill 模式 → unwind → 场景测试与基准。
</details>
