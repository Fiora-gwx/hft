# Agent Note: 双腿同时下单

Status: implemented

## Problem

[可执行价差](2026-10-14-cross-taker-executable-spread.md)采用 OnFill,第二腿须等待第一腿成交回报。负责人于 2026-10-14 明确决定两腿同时发出,减少等待和再次验证的往返。

## Decision

[策略](../../../../strategies/cross-taker/README.md)使用 SDK 已有 Simultaneous 链接,两腿价格取同次观察的盘口与配置 slip,整对准入交给引擎。默认 Reverse 对成交不等的超额腿反向减仓,避免保留额外方向敞口。失败计数以双腿尝试为单位,任一腿都可能先失败,引擎生成腿的 tag 不作为归属依据。

## Alternatives considered

OnFill 的成交后对冲时序与负责人要求冲突。两条无关联的策略订单无法表达整对准入和超额修复,不采用。Hold 留给裸腿追踪与修复,本策略采用 Reverse 优先回收超额风险;这不保证成交或利润。

## Consequences

SDK 无需新字段。引擎修复与策略切换分属 PR,部署须同时包含引擎的原子准入、出环容量及恢复修复。真实策略经 sequencer、模拟撮合和拒单 DST 验证,用例归 [TESTS](../../../../strategies/cross-taker/TESTS.md)。资金与风险行为仍登记在 [changes](../../../../docs/changes.md) 待评审;同时下单时序已由负责人决定。上线门槛不变。
