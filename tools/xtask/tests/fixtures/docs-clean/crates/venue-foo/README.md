---
description: "Foo 交易所协议"
kind: crate
tier: pure
scope: core
status: spec
---

# venue-foo

## 概要

venue-foo 的概要。

## 目录

venue-foo 的目录。

## 使用本组件

```rust
pub struct FooPerp;   // NAME = "foo-perp", MARKET = LinearPerp
pub struct FooSpot;   // NAME = "foo-spot", MARKET = Spot
```

### 分档快照流

`FooPerp`(永续)与 `FooSpot`(现货)的分档快照流。

### L2 序号归一化

`FooPerp`(永续)与 `FooSpot`(现货)的L2 序号归一化。

### 累计量与费用来源

`FooPerp`(永续)与 `FooSpot`(现货)的累计量与费用来源。

### 结果未知码表

`FooPerp`(永续)与 `FooSpot`(现货)的结果未知码表。

### VenueCaps 默认值

`FooPerp`(永续)与 `FooSpot`(现货)的VenueCaps 默认值。

### 账户配置核验

`FooPerp`(永续)与 `FooSpot`(现货)的账户配置核验。

### 保证金数据换算

`FooPerp`(永续)与 `FooSpot`(现货)的保证金数据换算。

### 外部成交识别

`FooPerp`(永续)与 `FooSpot`(现货)的外部成交识别。

### 用户流事件表

`FooPerp`(永续)与 `FooSpot`(现货)的用户流事件表。

### 只读查询

`FooPerp`(永续)与 `FooSpot`(现货)的只读查询。

### 测试网/模拟盘

`FooPerp`(永续)与 `FooSpot`(现货)的测试网/模拟盘。

### 心跳与 BBO 探测

`FooPerp`(永续)与 `FooSpot`(现货)的心跳与 BBO 探测。

## 实现说明

venue-foo 的实现说明。

## 延迟与资源契约

| 入口 | p50 / p99 预算 | 基准名 |
|---|---|---|
| `classify` | ≤ 20 ns / ≤ 50 ns | `bench_classify_foo` |
| `decode_md` bookTicker → `Bbo` | ≤ 0.4 µs / ≤ 0.8 µs | `bench_decode_foo_bbo` |

## 测试与验收

venue-foo 的测试与验收。

## 依赖

venue-foo 的依赖。

## 延伸阅读

venue-foo 的延伸阅读。

## 已知限制与后续工作

venue-foo 的已知限制与后续工作。

### 开发备忘

无。
