# Agent Note: 引擎订阅改为持仓驱动

Status: proposed

## Problem

引擎自身的订阅只覆盖已绑定 GroupSpec 的交易腿。group 之外的永续持仓是规格内的正常产物——残余核销移到无组、外来成交归属不到 group、人工在网页交易别的币——可 pipeline 移除 group 后该 inst 被退订:没有标记价的持仓让该 (account, venue) 永久停在 MarginStress L1,强平距离不再更新,L3 进不来,进来了也报不出价。EngineConfig 段 2 的 `ref_inst` 也不在订阅集合里,策略改参数后换算率会静默过期([X1 评审](../../../../design/_work/review-2026-10-03-M0e-X1资金正确性.md) P1-D)。

## Proposal

用户决定按推荐执行([round8-decisions](../../../../design/_work/round8-decisions.md) R8-12;规范见 [round8f 简报](../../../../design/_work/round8f-spec-brief.md) F1(e))。订阅规则归 [hft-sequencer](../../../../crates/hft-sequencer/README.md),pipeline 的提示归 [pipeline](../../../../python/pipeline/README.md#一轮的步骤)。

1. 订阅并集 = GroupSpec 交易腿(现规则)∪ 该实例账户中 `InstPos.qty ≠ 0` 的永续 inst(BBO、MARK、FUNDING)∪ 有现货库存的 inst(BBO)∪ EngineConfig 段 2 的每个 `ref_inst`(BBO)。
2. 持仓变化引起并集变化时,沿用"并集变化重发 `FeedCmd::Subscribe`"。
3. pipeline 移除 group 时,若该 inst 仍有账户级持仓,只提示、不阻止。

## Alternatives considered

**移除 group 的前置条件扩展到账户级持仓。** 核销产生的无组持仓是有意留下的,禁止移除会让 group 永久占一个 cell;持仓与订阅解耦更直接。

**无组持仓由人工全部平掉后再移除。** 依赖人;平仓之前的那段时间仍然没有标记价。

## Acceptance criteria

- DST:核销把永续残余移到无组后移除该 group → 该 inst 的 MARK 与 FUNDING 订阅保留,`mark_oldest_ens` 正常前进,MarginStress 不因断流停在 L1;无组持仓归零后订阅随之撤销。
- 策略参数改动 `ref_inst` 后,新的 `ref_inst` 在同一轮出现在订阅中。

## Risks

- 订阅数随持仓增加,可能超出 `max_sub_insts`;超出时按容量规则报错,不静默丢弃。
- 订阅变化更频繁,feedctl 环的突发容量按 hft-sequencer 的规则处理。
