---
description: "主循环"
kind: crate
tier: hot
scope: core
status: spec
---

# hft-sequencer

## 概要

hft-sequencer 的概要。

## 目录

hft-sequencer 的目录。

## 使用本组件

| 环 | 方向 | 元素 | 优先级 |
|---|---|---|---|
| `ctl` ×1 | ctl-gateway → sequencer | `CtlRecord` | 1 |
| `hk` ×1 | housekeeping → sequencer | `HkMsg`:`ClockStatus` | 1 |
| `rep-<venue>` ×M | gw → sequencer | `RepMsg`(条目在配套的 `rep-<venue>-bulk` 字节环) | 2 |
| `out-<venue>` ×M | sequencer → gw | `OutMsg` = `GwCmd`(64 B) | — |

### 消费与产生的事件

- 产生:`Bbo`、`L2Delta`、`L2Snapshot`、`Trade`、`Funding`、`MarkPrice`、`IndexPrice`、`Liquidation`、`L2Batch`、`OrderDecision`、`SendReport`、`OrderEvent`、`PositionReport`、`BalanceReport`、`FundingSettle`、`ExternalFill`、`MarginReport`、`AccountConfigReport`、`ExecDecision`、`ChainHead`、`PoolQuote`、`SwapDecision`、`TxLifecycle`、`ControlCmd`、`CmdResult`、`ConfigBlob`、`ReconcileReport`、`ReconcileBatch`、`Timer`、`ConnState`、`Advice`、`StateSnapshot`、`LatencySample`、`Note`、`HostProfile`、`ClockStatus`、`SegmentRoll`、`EngineClock`、`RawRx`、`RawTx`。

## 实现说明

hft-sequencer 的实现说明。

## 延迟与资源契约

hft-sequencer 的延迟与资源契约。

## 测试与验收

hft-sequencer 的测试与验收。

## 依赖

hft-sequencer 的依赖。

## 延伸阅读

hft-sequencer 的延伸阅读。

## 已知限制与后续工作

hft-sequencer 的已知限制与后续工作。

### 开发备忘

无。
