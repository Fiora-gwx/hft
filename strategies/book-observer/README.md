---
description: "book-observer 策略(kind_id 6):只订阅行情,不交易。订阅各 group 每腿的簿(可选成交),按腿计数并保存最近的最优价,让一个实例在不下单的情况下拉取某个 venue 的行情"
kind: crate
tier: hot
scope: core
status: impl
---

# book-observer

## 概要

`book-observer` 是只看行情的策略(kind_id 6,登记在 [registry](../registry/README.md))。引擎只为策略、Model 与持仓订阅行情([hft-sequencer](../../crates/hft-sequencer/README.md)),所以要在不交易的情况下让 hft-core 拉取某些 inst 的簿与成交(行情验收、录制、新 venue 接入),就把这些 inst 写成本策略的 group 腿。它订阅每腿的簿(`EvMask::BOOK`)与成交(`EvMask::TRADE`,可关),按腿计数并保存每次簿更新后的最优买卖价,**从不发订单意图或目标意图**。只依赖 [hft-strategy-sdk](../../crates/hft-strategy-sdk/README.md) 与 [hft-strat-lib](../../crates/hft-strat-lib/README.md)。

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

每个 group 1–4 腿,腿可以在同一 venue;`check` 只拒绝 0 腿(`GroupShape` 码 1)。腿只用于订阅,没有交易含义。

### 参数

| 字段 | 类型 | 默认 | 说明 | minor_change |
|---|---|---|---|---|
| `trades` | Bool | 1(雏形值) | 同时订阅每腿的成交;关掉后只订阅簿。`relax = Up` | 否 |

### 订阅

`subscriptions` 对每个 group 的每条腿登记 `(inst, gix, BOOK | TRADE, Fast)`(`trades = 0` 时只有 `BOOK`)。参数变化后引擎重算订阅。引擎另按 group 腿自动订阅 BBO(规则归 hft-sequencer);没有 BBO 流的 venue 忽略这一项。

### 事件处理

| 事件 | 处理 |
|---|---|
| `Book{inst, gix}` | 该腿 `books += 1`;`bid`/`ask` = `v.bbo(inst)` 的最优买/卖价(一侧为空时为 0) |
| `Trade{inst, gix}` | 该腿 `trades += 1` |
| 其他(`Risk`、`External`、`Notice`、`Mode`、`Exec`、`Manual`、`Timer` 等) | 忽略:本策略没有订单、目标与仓位 |

### Note 码表与报表字段

无 Note、无报表字段、无策略命令(`desc.rs` 三表为空)。

## 实现说明

<details><summary>实现要点</summary>

- 状态按 gix 保存 `GroupState{id, legs[4], books[4], trades[4], bid[4], ask[4]}`;容量在 `new` 时按 `GroupTable::capacity()` 预留,`on_params` 不分配。`GroupId` 与腿集合都不变的 group 保留计数,其余从零开始。
- 快照:`n: u16`,每个 group `id: u32` 加每腿 `books: u64, trades: u64, bid: i64, ask: i64`;恢复按 `GroupId` 匹配,已不存在的 group 丢弃。

| 文件 | 职责 |
|---|---|
| `src/lib.rs` | `StrategyDesc` 与 `impl Strategy`,转发到 `fast` |
| `src/fast/mod.rs` | 订阅与按腿计数 |
| `src/params.rs`、`src/desc.rs` | 参数 schema 与 `check`;空描述符 |
| `tests/` | 场景测试与 refdata 夹具 |

</details>

## 延迟与资源契约

每个事件 O(腿数 ≤ 4) 的整数运算,不分配(场景测试以计数分配器核对);`Model = NoModel`。状态每 group 200 B 以内。

保留基线预算:每个入口 p50 ≤ 50 ns/p99 ≤ 150 ns;参考机实测尚未完成,不是性能声明。`benches/on_event.rs` 经真实 BookObserver::on_event 测量 tick_fire(Book 事件,读顶档并计数)、tick_miss(Trade 事件,只计数)、exec_update(Order 事件,忽略)、risk_event(忽略);本策略从不发意图,fire 指完整记录路径。夹具为 tests/common 的单 group 两腿,计数只增长,样本间不复位。

所有订阅都是 Part::Fast,无慢侧缓存更新、Advice 或 TargetIntent;`benches/not-applicable.toml` 为 slow_update、advice、target_set 登记理由。

## 测试与验收

用例见 [TESTS.md](TESTS.md)。

## 依赖

只依赖 `hft-strategy-sdk` 与 `hft-strat-lib`;测试另依赖 hft-testkit、hft-types、hft-refdata、hft-event(只在 `[dev-dependencies]`)。

## 延伸阅读

- [cookbook:新增一个策略](../../docs/cookbook/adding-a-strategy.md)

## 已知限制与后续工作

- 不产生延迟样本或报表;需要行情质量指标时由离线工具读 journal。

### 开发备忘

<details><summary>给实现者</summary>

本策略只为让引擎订阅行情而存在;需要交易逻辑时另写策略,不在这里加意图。

</details>
