# Agent Note: 私有回报优先调度

Status: implemented

## Problem

round4 的 read→frame 等待包含较早帧的处理时间。当前网关已经逐帧发布并检查出站命令,但先排空下单连接再读私有用户流,且已有完整缓冲时仍会再次读 socket。

## Decision

先处理私有用户流,已有完整帧或 TLS record 时先交付缓冲。连接内保持顺序,每帧发布后检查出站命令。VEN-207 在下一帧解码入口验证上一帧已对消费者可见,并验证私有流不等待 64 帧下单响应。

## Alternatives considered

同一连接内按消息类型重排会改变交易所事实顺序。批量暂存规范化回报会推迟 sequencer 看见成交。两者均不采用。

## Consequences

Windows dev profile、同一脚本连接基准 `venue_gw_private_report_burst` 的 Criterion 时间区间由 `[219.41,247.90,286.15] µs` 降至 `[10.495,11.657,13.067] µs`。区间是均值置信区间,不是分位数;度量包含排队到第一条私有回报发布后下一解码入口的时间。它证明跨连接突发等待被消除,不等于 c8 Fill→Order 的 25 µs 验收。真实主机仍需同口径复测,同一流较早帧的必要解码时间无法在不重排的情况下消除。
