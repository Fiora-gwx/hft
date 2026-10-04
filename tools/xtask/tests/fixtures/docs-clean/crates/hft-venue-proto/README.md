---
description: "VenueProtocol"
kind: crate
tier: pure
scope: core
status: spec
---

# hft-venue-proto

## 概要

hft-venue-proto 的概要。

## 目录

hft-venue-proto 的目录。

## 使用本组件

hft-venue-proto 的使用本组件。

## 每个 venue README 的必备节

| 小节 | 对应契约 |
|---|---|
| 分档快照流(频道、档位) | 26 |
| L2 序号归一化(增量深度,可选扩展;规格保留) | 1 |
| 累计量与费用来源 | 9 |
| 结果未知码表(两张表的来源) | 6 |
| VenueCaps 默认值(含 `fok`) | 15 |
| 账户配置核验(含现货借币开关) | 16 |
| 保证金数据换算 | 17 |
| 外部成交识别 | 18 |
| 用户流事件表(含资金费推送) | 14 |
| 只读查询(端点、分页) | 31 |
| 测试网/模拟盘(端点) | `VenueCaps.testnet` |
| 心跳与 BBO 探测(ping 间隔) | 24 |

## 实现说明

hft-venue-proto 的实现说明。

## 延迟与资源契约

| 入口 | p50 门槛 | p99 门槛 | 说明 |
|---|---|---|---|
| `classify` / `classify_report` | ≤ 30 ns | ≤ 60 ns | 只看前 64 B |
| BBO 类 `decode_md` | ≤ 0.4 µs | ≤ 0.8 µs | bookTicker 类 |
| 分档快照类 `decode_md`,20 档 | ≤ 1.2 µs | ≤ 2 µs | 契约 26 |
| `encode` | ≤ 0.3 µs | ≤ 0.6 µs | — |

## 测试与验收

hft-venue-proto 的测试与验收。

## 依赖

hft-venue-proto 的依赖。

## 延伸阅读

hft-venue-proto 的延伸阅读。

## 已知限制与后续工作

hft-venue-proto 的已知限制与后续工作。

### 开发备忘

无。
