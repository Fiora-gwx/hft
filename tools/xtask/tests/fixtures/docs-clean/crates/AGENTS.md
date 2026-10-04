# AGENTS.md — crates

## 依赖方向

```
hft-types ← hft-refdata ← hft-event                      (L0,被所有人依赖)
hft-strategy-sdk → L0                                     (策略接口,只依赖 L0)
hft-venue-proto ← venue-<x>                               (L1:trait 与纯协议)
hft-oms                                                   (L1,彼此不依赖)
hft-rt ;  hft-net → hft-rt + hft-event(只用 `ConnRole`)
hft-sequencer → hft-strategy-sdk + L1 + hft-rt
strategies/<name> → hft-strategy-sdk
```

## README 模板

每个 crate 的 README 按以下顺序,标题用原文:

```markdown
---
description: "一句话"
kind: crate
tier: pure
scope: core
status: spec
---

# <name>

## 概要
## 目录
## 使用本组件
## 实现说明
## 延迟与资源契约
## 测试与验收
## 依赖
## 延伸阅读
## 已知限制与后续工作
### 开发备忘
```
