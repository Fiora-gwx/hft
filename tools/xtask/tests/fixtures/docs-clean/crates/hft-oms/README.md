---
description: "订单槽与状态机"
kind: crate
tier: hot
scope: core
status: spec
---

# hft-oms

## 概要

hft-oms 的概要。

## 目录

hft-oms 的目录。

## 使用本组件

槽状态变化写 `Note{CapSet}`;见 [延迟与资源契约](#延迟与资源契约)。

```rust
#[repr(C)] pub struct LegRow {   // 40 B
    pub leg_kind: u8,            // 1 = First、2 = Second
    pub _pad: [u8; 7],
    pub qty: u64,
    pub px: i64,
    pub ts: u64,
    pub id: u64,
}
```

## 实现说明

hft-oms 的实现说明。

## 延迟与资源契约

| 入口 | 预算 | 基准 |
|---|---|---|
| `place` | p50 ≤ 300 ns,p99 ≤ 1 µs | `oms_place` |

## 测试与验收

hft-oms 的测试与验收。

## 依赖

hft-oms 的依赖。

## 延伸阅读

- [hft-sequencer](../hft-sequencer/README.md)

## 已知限制与后续工作

hft-oms 的已知限制与后续工作。

### 开发备忘

无。
