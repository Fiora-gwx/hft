# Agent Note: 首个跨所 taker 的可执行价差范围

Status: implemented

## Problem

首个可运行套利策略需要基于现有双腿 SDK、真实费用和双侧数量上限交易,在真实 sequencer 验证闭环。原 [策略规格](../../../../strategies/cross-taker/README.md) 要求 LeadLag 模型就绪,该慢速构件尚无实现。本次负责人任务明确要求可执行价差、可配置净边和克制的参数数量。

## Decision

按扣费及两腿价格余量后的净边触发,只发买入低价腿的一条 OnFill 订单。无 LeadLag/Advice、Simultaneous 或无利润退出。两侧限定机会年龄,不改变引擎按连接健康判报价可用性的契约。cap 约束每腿绝对持仓;连续对冲失败请求 halt,引擎继续修复。

## Alternatives considered

照搬 LeadLag 需补慢速模型与 advisor 工作,与本次策略专属 PR 范围不符。直接发两单会绕过 OnFill、HEDGE_LIMIT 与裸腿修复,不采用。两腿相抵后的净敞口 cap 会允许等量反向持仓无限增长,采用逐腿 cap。

## Consequences

无需 SDK/引擎改动;双腿及回放证据在 registry 测试。基线差异与新增 halt/限额标入 [changes](../../../../docs/changes.md) 待评审。上线门槛和延迟预算不放宽;无获利反向价差时持仓可能持续,价格余量不是未来第二腿成交价格保证。
