# Agent Note: 恢复差异只在有证据的旧尾部接受

Status: implemented

## Problem

恢复段的名字不能界定差异范围。正常启动后仍在同一个 Recovery 段写行情、决策和快照,整段放行会漏掉恢复后的错误。现有 RecoverMismatch 注记只保存每个账户与 venue 的首处分歧位置,没有逐条差异清单。离线工具必须用现有证据限定接受范围,同时保留排障信息。

## Decision

冷规划器核验正式恢复流程写出的启动注记,用旧运行最近快照和最后完整事件确定尾部,从每个受影响账户与 venue 的首处分歧起接受其后差异。范围内仍做比较并保留明细,新运行从自己的引导快照重建。缺失旧尾部时不把新段单独判为通过。实际规则与错误分类归 [replayer verify](../../../../apps/replayer/README.md#verify)。

这细化了 [RecoverDriver 的离线验证分工](../../proposed/architecture/2026-10-01-recover-driver.md),没有放宽生产启动的风控闸门。默认构建没有新的诊断存储,日志格式与恢复流程保持原契约。

## Alternatives considered

按 Recovery 段整体放行会覆盖正常交易,因此不采用。只接受注记指向的单条差异也不足,同一账户后续差异不会再产生注记。为每条差异新增日志记录可以缩小范围,但需要改变生产记录协议,本批先使用已有证据并明确其粒度。

## Consequences

接受的差异仍可供负责人检查,不会因普通分歧为空而消失。故障测试从真实写者注入差异,由正式恢复路径生成注记,避免测试自己伪造合法证据。反例覆盖账户范围、指针、事件边界、缺失与重复,用例归 [RPL-024、RPL-264、RPL-265](../../../../apps/replayer/TESTS.md#rpl-024-recovery-区间按-journal-值接受差异只列入-recovery_diffs)。完整样本和 RecoveryGap 仍按组件文档验收。
