---
description: "事件体与登记表"
kind: crate
tier: pure
scope: core
status: spec
---

# hft-event

## 概要

hft-event 的概要。

## 目录

hft-event 的目录。

## 使用本组件

**环拓扑**(两表的环名与元素类型由 doc-check 核对):

| 环 | 写者 | 元素类型 |
|---|---|---|
| `ctl` | ctl-gateway | `CtlRecord` 帧 |
| `hk` | housekeeping 线程 | `HkMsg` |
| `rep-<venue>` | gw | `RepMsg`;配套 `rep-<venue>-bulk` 字节环 |
| `out-<venue>` | sequencer | `GwCmd`(`OutMsg`) |

| `source` | 值 | 产生者 |
|---|---|---|
| `Sequencer` | 1 | sequencer 自身 |
| `Risk` | 3 | hft-risk |

**引擎 Note 码登记表**:

| 名字 | source | 级别 |
|---|---|---|
| `CapSet` | `Risk` | Info |
| `EpochStart` | `Sequencer` | Info |

定时器编码:

| owner | timer_id | 名字 |
|---|---|---|
| `Sys` | 1 | `Snapshot` |
| `Sys` | 2 | `Roll` |

## 实现说明

hft-event 的实现说明。

## 延迟与资源契约

hft-event 的延迟与资源契约。

## 测试与验收

hft-event 的测试与验收。

## 依赖

hft-event 的依赖。

## 延伸阅读

hft-event 的延伸阅读。

## 已知限制与后续工作

hft-event 的已知限制与后续工作。

### 开发备忘

无。
