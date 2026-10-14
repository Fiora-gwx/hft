# Agent Note: 成交 detail 契约冲突

Status: proposed

## Problem

[VPR-124.6](../../../../crates/hft-venue-proto/TESTS.md) 要求 Fill.detail 为零，冻结 README 契约 28 却要求成交方向。零值使网关无法安全地把无归属成交转换为 ExternalFill。

## Proposal

遵循冻结 README，Fill/PartialFill.detail 必须为 Side::Buy 或 Side::Sell；查询保留订单状态，撤改单拒绝保留 GONE。通用 check_detail 与独立买卖样本验证该规则，测试文本同步，等待负责人确认。产品线已有方向编码不改为零。

## Alternatives considered

清零会丢失资金归属所需方向；从当前订单猜方向无法覆盖外来或迟到成交。两者都不采用。

## Acceptance criteria

VPR-124 验证买、卖、查询与撤改单拒绝；不重标里程碑。VEN-073/294 保持[容量前置评审](2026-10-15-venue-capacity-preconditions.md)中的合法容量，不放宽安全约束。

## Risks

测试冲突待负责人批量评审。Fake 样本只能验证通用检查器，不能代替交易所采集。
