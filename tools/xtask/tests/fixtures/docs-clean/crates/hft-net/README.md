---
description: "网络"
kind: crate
tier: hot
scope: core
status: spec
---

# hft-net

## 概要

hft-net 的概要。

## 目录

hft-net 的目录。

## 使用本组件

hft-net 的使用本组件。

## 实现说明

hft-net 的实现说明。

## 延迟与资源契约

| 入口 | 线程 | p50 / p99 | 基准 |
|---|---|---|---|
| `next_frame`,300 B | feed/gw | ≤ 0.35 µs / ≤ 0.8 µs | `net_rx_300b_mem` |
| 同上,2 KiB | feed | ≤ 0.7 µs / ≤ 1.5 µs | `net_rx_2k_mem` |

## 测试与验收

hft-net 的测试与验收。

## 依赖

hft-net 的依赖。

## 延伸阅读

hft-net 的延伸阅读。

## 已知限制与后续工作

hft-net 的已知限制与后续工作。

### 开发备忘

无。
