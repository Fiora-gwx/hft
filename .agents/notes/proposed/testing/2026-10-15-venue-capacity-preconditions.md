# Agent Note: 网关验收容量前置的一致性

Status: proposed

## Problem

[VEN-073 与 VEN-294](../../../../crates/hft-venue/TESTS.md) 的原始前置分别要求小于整帧预留的行情环，以及超过墓碑容量的终态集合，与冻结 README 的容量约束冲突。

## Proposal

保持 [hft-venue 容量约束](../../../../crates/hft-venue/README.md) 不变。VEN-073 使用 128 元素环，以两帧各 64 条成交填满，仍验证 md 满时 BBO 可读。VEN-294 使用 4 个墓碑和 4 个活动条目填至 0.5 负载，仍验证新增交易所 ID 被拒而 client id 归属可用。现有测试已采用这些可达前置，本次同步 TESTS.md，前置调整待负责人确认。

## Alternatives considered

**为容量 4 放松整帧预留**会允许先消费后丢失一帧中的部分交易事件；**把墓碑容量 4 当作可容纳 8 个终态**会使有界保留失效。两者都不采用。

## Acceptance criteria

原有 VEN-073/294 回归通过；环、ID 表与墓碑容量算法不因修订而放宽；不修改里程碑。

## Risks

前置数量与冻结文本不同，需要负责人批量评审；测试结论只涵盖合法构造下的相同背压/满表边界。

VEN-015.6 的 Poll 999 ms 同样不可达，refdata 在加载时已拒绝小于 1000 ms 的值。网关下限回归采用合法 Poll 1000 ms 与配置下限 1001 ms，不放宽 refdata；该前置文本同步一并待评审。
