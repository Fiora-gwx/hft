# Agent Note: 资金费策略入场与退出双腿同时发出

Status: implemented

## Problem

portfolio-carry 的目标以 PairOnFill 执行,第二腿等待首腿成交回报。负责人于 2026-10-14 决定资金费策略和 cross-taker 一样双腿同时下单,覆盖入场和退出。

## Decision

[策略](../../../../strategies/portfolio-carry/README.md)共用目标构造器输出 Simultaneous,固定 Reverse、零 fix_tolerance 和 TrimExcess,保留原有切片参数。已合入的引擎负责同事件整对准入、双侧深度取小、限价和失败修复;策略不复制执行状态机。实际资金费仍由结算事件进入账本。

## Alternatives considered

**PairOnFill** 要等待首腿成交,与负责人的同发决定不符。按[同发方案](2026-10-14-cross-taker-simultaneous.md)采用引擎已有 Simultaneous,不以两条独立策略订单替代关联腿。

## Consequences

任一腿均可能先成交或失败,两腿发出不保证同时成交。Unknown 等交易所证据确认,终态后的不对称成交由 Reverse 交引擎处理,反向失败仍需 Broken/修复路径。价格以每次切片观察的 BBO 为准,目标本身不冻结未来切片的报价。

真实策略通过 sequencer 的同事件 trigger_seq、深度/限价、非对称成交、拒单、断线查询、结算去重和逐字节重放验收;用例归 [TESTS](../../../../strategies/portfolio-carry/TESTS.md)。资金与风控语义在 [changes](../../../../docs/changes.md) 标为待评审,同发时序已经负责人决定。
