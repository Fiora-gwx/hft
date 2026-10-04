# Agent Note: CEX-DEX 默认 chain-first:包含证据到了再对冲

Status: proposed

> **第八轮(2026-10-02)**:范围收窄——chain-first 只适用于 optional 的 cdex 策略(`strategies/cdex`),在决定把 cdex 迁到新框架时启用;见 [ccex-first-scope](2026-10-02-ccex-first-scope.md)。

## Problem

CEX-DEX(cdex)有两条腿,顺序决定失败时的形态。实盘数据显示漂移与抢跑几乎全部来自链上腿:币安腿 1.4 s 内漂移中位 0、p99 0.1%,链上"追价"信号约 80% 被抢先([06 §4](../../../../design/06-链上执行层.md)、`analysis/cdex-策略优化分析-2026-09-26.md`)。9/24 的"CEX 有仓、链上没成交"是同时下单/先 CEX 的典型失败([postmortem](../../../../docs/postmortem/README.md))。

## Proposal

默认 **chain-first**([06 §3–§4](../../../../design/06-链上执行层.md)):

1. 策略产生 `SwapIntent`(`min_out` 由信号与发送前模拟决定并记录;free profile 用公共 RPC `eth_call`,revm 为可选扩展),chain-core 以 free profile 把同一签名交易并发提交到多个公共 RPC(可选扩展:builder profile 以有效期 2 块的 bundle 并发直投 builder)。每笔交易经最小包装合约执行,合约内 `block.number <= maxBlock`(= bundle 的 `maxBlockNumber`),使**交易本身**在链上过期——bundle 过期不等于交易过期([tx-onchain-expiry](2026-10-01-tx-onchain-expiry.md))。
2. 包含证据(`TxLifecycle{IncludedOk}`,≥ 2 个来源一致且结果校验通过)进入 sequencer 的**同一事件内**,OMS 的 `LegLink` 发出 CEX 对冲腿(`leg_gap` ≤ 20 µs)。
3. 失败 = 交易未上链(终局块号 > `maxBlock` 且 nonce 未被本交易消耗)= 无仓位、零成本;迟到上链只会 revert(只花 gas)。`Unknown` 期间不做任何 CEX 动作。
4. hedge-first 只在三条件同时满足时启用:机会来自 CEX 大幅跳价且池子必然滞后;bundle 投给 in-turn validator 对应的主 builder 且历史包含率 > 90%;对冲腿可在 1 块内低成本撤回。

## Alternatives considered

**同时下单。** revert 时需要回滚币安腿;9/24 的裸敞口就是这种形态。老板的实盘复盘也得出"先链上后币安优于同时下单"。

**hedge-first(先 CEX)。** 失败模式是 CEX 有仓、链上没成交,且链上失败的概率远高于 CEX;只作为上述三条件下的例外。

**等 finalized 再对冲。** BSC 终局约 1.1 s,把暴露时长翻倍;会计以 finalized 为口径,但对冲用 latest 证据即可(多源一致)。

## Acceptance criteria

- 库存暴露 `exposure` 0.5–1.2 s;对冲 `leg_gap` ≤ 20 µs([latency-budget §4](../../../../docs/latency-budget.md#4-链上路径预算bsc))。
- 没有任何对冲意图在缺少 `IncludedOk` 的情况下发出(回放断言 + DST 不变量)。
- 下一块包含率 ≥ 70%、块内排位 < 10(假设 A4,[12 §2](../../../../design/12-风险与未决问题.md))。

## Risks

- 包含前后价格变动:1 s 内 BTC 1σ ≈ 0.9 bp,远小于 5–20 bp 净边;长尾币波动更大,由成本线与确认窗口吸收。
- builder 自营套利系统性抢跑(假设 A5):按 builder 分组统计成交价差,剔除问题 builder。
- 包含证据来源被单点阻断(9/24 的 403):多源回执,传输错误只进 `Unknown`([defensive-patterns](../../../../docs/defensive-patterns.md#传输错误--业务失败))。
