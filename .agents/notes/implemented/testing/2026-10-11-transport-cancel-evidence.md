# Agent Note: 传输事件内的主动撤单需要独立证据

Status: implemented

## Problem

[CI 38103614553](https://github.com/Fiora-gwx/hft/actions/runs/38103614553) 的 storage 种子 `0xcef145ccd5e68192` 在逐 Input 检查中发现 PendingNew 到 PendingCancel。原 [INV-1](../../../../docs/tests/invariants.md#inv-1-仓位与库存的每一次变化都能追溯到成交证据传输错误从不改变仓位) 把所有传输事件中的槽迁移都约束为 Unknown,只为确定未写出的 Place 留出例外。这同时排除了定时器或连接事件触发的主动撤单,与 OMS 的撤单状态机冲突。进入 PendingCancel 尚未确认撤单,也没有改变持仓。

## Decision

收窄 INV-1 对主动请求状态的约束。只有本 Input 中存在账户、venue、slot、gen 全部匹配且成功的 Cancel 决策时,才接受 PendingCancel;成交、撤单确认和仓位变化仍须由相应交易所证据支持。验证规则及反例归 [TK-111](../../../../crates/hft-testkit/TESTS.md#tk-111-传输-input-内的主动撤单必须有决策证据)。

## Alternatives considered

保留原规则会把正常的撤单保护判为业务失败,无法区分请求与确认。完全放行 PendingCancel 又会漏掉没有撤单决策的状态变化。按整步末状态检查会隐藏中间行为,因此继续使用逐 Input 观察并要求独立决策证据。

## Consequences

保留发现问题的固定种子并以 10000 步运行。反例删除 Cancel 决策后必须失败;传输事件造成任何非零仓位或库存变化仍失败。另一个同期种子发现的重复 SecondRetry 是业务缺陷,按 [OMS-464](../../../../crates/hft-oms/TESTS.md#oms-464-恢复后迟到的第一腿成交不产生重复续接) 修复并由 [SEQ-375](../../../../crates/hft-sequencer/TESTS.md#seq-375-迟到成交解除恢复保持时单次对冲并验证回放) 验证回放,不通过放宽裸量检查处理。
