---
description: "pm-taker-probe 策略(kind_id 8):人工把 enabled 从 0 改为 1 才做一轮吃单——在最优卖价按金额买入(SPEND、Ioc),结束后可选在最优买价卖回持仓;用于在正式账户上端到端核实按金额吃单"
kind: crate
tier: hot
scope: core
status: impl
---

# pm-taker-probe

## 概要

`pm-taker-probe` 是最小的实盘吃单验证策略(kind_id 8,登记在 [registry](../registry/README.md))。它只在参数 `enabled` 经 `SetParams` 从 0 变为 1 时做**一轮**:在唯一 group 唯一腿的最优卖价下一张**按金额买入**单(`OrderIntent::SPEND`,`Tif::Ioc`,花费不超过 `spend_usdc`,语义归 [hft-oms 按金额买入](../../crates/hft-oms/README.md#按金额买入)),该单终结后(`sell_back` 时)在最优买价以 Ioc 卖出该腿全部持仓,然后停住,直到 `enabled` 先回 0 再变 1。任何时刻至多一张在途订单;每一步写一条 Note,journal 里能看到整轮。`non_prod_only = false`:它要绑定正式账户。只依赖 [hft-strategy-sdk](../../crates/hft-strategy-sdk/README.md) 与 [hft-strat-lib](../../crates/hft-strat-lib/README.md)。

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

恰好一个 group、一条腿(被探测的 inst);两条腿以上 → `GroupShape` 码 1,第二个 group → 码 2。腿可以是运行时绑定的槽位 inst(槽位绑定与其他策略相同,由绑定命令完成,策略只订阅该 inst)。

### 参数

| 字段 | 类型 | 默认 | 说明 | minor_change |
|---|---|---|---|---|
| `enabled` | Bool | 0 | `manual_only`;从 0 变为 1 时武装一轮;实例创建(含重启)时为 1 也不武装 | 否 |
| `spend_usdc` | Notional | 1 000 000(1 个计价单位,暂定) | 买单至多花费的计价资产(指数 −6);schema 上限 2 000 000 是硬上限;`relax = Up` | 否 |
| `sell_back` | Bool | 1 | 买单终结后卖回该腿持仓 | 否 |

### 订阅

`subscriptions` 登记 `(腿 inst, gix 0, BOOK, Fast)`;引擎另按 group 腿订阅 BBO(没有 BBO 流的 venue 忽略),最优价都经 `Views::bbo` 读取。

### 规则

1. **武装**:`on_params` 看到 `enabled` 0 → 1 且处于空闲 → `Ready`;`enabled` 变 0 时 `Ready` 与 `Done` 回到空闲;进行中的一轮照常走完。
2. **买入**:`Ready` 时该腿的簿更新(或 BBO)到达,且 `Views::quote_ok` 为真、最优卖价 > 0 → `OrderIntent::spend_buy(Ioc, px = 最优卖价, qty = OrderIntent::spend_qty(spend_usdc, px, 该 inst))`(`spend_qty` 向下取整到 lot,所以 `px × qty ≤ spend_usdc`;成交价更低时交易所可能给出多于 `qty` 的数量);报价缺失或不健康时继续等待;`qty` 为 0 → `Skipped`,这一轮结束。
3. **买单终结**:放行结果为 `Shadowed`/`Rejected(bits)`,或订单更新 `Filled{done}`、`Cancelled`、`Rejected` → `BuyEnd`;`sell_back = 0` 时这一轮结束;否则卖回:持仓 = `group_pos(账户, group).legs[0].qty`(含此前已持有的数量),向下取整到 lot;为 0 → `Skipped`;无买价 → `Skipped`;否则 `Sell`、`Ioc`、`IntentClass::Reduce`、`px = 最优买价`、无 SPEND。
4. **卖单终结**:同第 3 条的终结方式 → `SellEnd`,这一轮结束。
5. **这一轮结束**:`enabled` 仍为 1 → `Done`(不再下单,直到 `enabled` 先变 0);否则回到空闲。任何时刻至多一张在途订单,没有重试、没有第二张买单。

### 事件处理

| 事件 | 处理 |
|---|---|
| 该腿的 `Book`/`Bbo` | `Ready` 时按规则 2 买入;其他阶段忽略 |
| `Order`(本策略在途订单) | 规则 3、4;`Acked`、`Unknown`、`Aged` 与 `done = false` 的成交只记累计量 |
| 每个事件开头 | 读取上一事件下单的放行结果(`Placed` → 记下 `OrderRef`;其他 → 该步终结) |
| 其他(`Risk`、`External`、`Notice`、`Mode`、`Timer` 等) | 忽略:账户限额、kill 与对账归引擎;本策略不持续持仓,一轮之后的残余由人工处理 |

### Note 码表与报表字段

全部 `Info`;`v[3]` = 已开始的轮数。

| 码 | 名 | v | 何时 |
|---|---|---|---|
| 1 | `BuySent` | `px`、`qty`、`spend` | 买单放出 |
| 2 | `BuyEnd` | 终结方式(1 成交完、2 撤销/余量撤、3 交易所拒、4 Shadow、5 门控拒)、累计成交、交易所码或 `RejectBits` | 买单终结 |
| 3 | `SellSent` | `px`、`qty`、持仓 | 卖单放出 |
| 4 | `SellEnd` | 同 `BuyEnd` | 卖单终结,这一轮结束 |
| 5 | `Skipped` | 原因(1 买量为 0、2 无买价、3 无持仓)、数值 | 某步没有发出,这一轮结束 |

无报表字段、无策略命令。

## 实现说明

<details><summary>实现要点</summary>

- 状态:`Phase`(`Idle`、`Ready`、`Buying`、`BuyLive`、`Selling`、`SellLive`、`Done`)、在途订单的 `OrderRef`、累计成交、轮数;`enabled` 记最近一次参数值,武装只看它的 0 → 1 变化。
- 快照:`phase: u8`、`rounds: u32`、`cum: i64`、`has_r: u8`,有在途订单时再 `venue: u8, account: u8, slot: u32, gen: u32`。恢复:在途订单继续跟踪;`Ready` 回到空闲(重启解除武装);`Buying`/`Selling`(放行结果未读)按这一轮结束处理。

| 文件 | 职责 |
|---|---|
| `src/lib.rs` | `StrategyDesc` 与 `impl Strategy`,转发到 `fast` |
| `src/fast/mod.rs` | 订阅与一轮的状态机 |
| `src/params.rs`、`src/desc.rs` | 参数 schema 与 `check`;Note 码 |
| `tests/` | 场景测试(自带驱动:按调用注入放行结果)与 refdata 夹具 |
| `benches/` | 四个入口基准 |

</details>

## 延迟与资源契约

每个事件 O(1) 的整数运算,不分配(场景测试以计数分配器核对);`Model = NoModel`。保留基线预算:`tick_miss`、`exec_update`、`risk_event` p50 ≤ 50 ns/p99 ≤ 150 ns,`tick_fire`(构造并放出一张买单与一条 Note)p50 ≤ 300 ns/p99 ≤ 1 000 ns;参考机实测尚未完成,不是性能声明。`benches/on_event.rs` 经真实 `PmTakerProbe::on_event` 测量,每个样本前在计时区外把阶段重置为该路径的起点。所有订阅都是 `Part::Fast`,无慢侧缓存更新、Advice 或 TargetIntent;`benches/not-applicable.toml` 为 slow_update、advice、target_set 登记理由。

## 测试与验收

用例见 [TESTS.md](TESTS.md)。

## 依赖

只依赖 `hft-strategy-sdk` 与 `hft-strat-lib`;测试与基准另依赖 hft-testkit、hft-types、hft-refdata、hft-event、criterion(只在 `[dev-dependencies]`)。

## 延伸阅读

- [hft-oms 按金额买入](../../crates/hft-oms/README.md#按金额买入)、[hft-strategy-sdk Ctx 与订单意图](../../crates/hft-strategy-sdk/README.md#ctx-与订单意图)、[cookbook:新增一个策略](../../docs/cookbook/adding-a-strategy.md)

## 已知限制与后续工作

- 只为人工验证吃单路径而存在:一轮之后不管理持仓,卖回失败或部分成交的残余由人工处理;不处理 `Risk`/`External`/`Notice`。
- 买入价取最优卖价、卖出价取最优买价,不越价;流动性不足时只成交一部分或无成交(交易所拒或余量撤),这本身就是探测结果。
- 买单的计价金额由 venue 协议按交易所规则取整(可能略少于 `px × qty`);交易所对吃单金额的最小值以 venue 的拒绝为准,策略不预判。

### 开发备忘

<details><summary>给实现者</summary>

不要在这里加重试、定时或第二张买单:每一轮都必须由人工的 `enabled` 0 → 1 发起。

</details>
