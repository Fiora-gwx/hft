# Agent Note: Model 检查点分为原始样本节与带版本的路径状态节

Status: proposed

## Problem

检查点只存原始样本、派生状态在恢复时重算,而路径依赖的派生状态(分位带及其更新时刻、平滑窗、上次发送的载荷摘要)从原始样本重算不出同一个值:"检查点 → 恢复 → 续算"与连续运行不再逐字节相同,每日 recompute 失败,门槛 ⑦ 过不了([W2 评审](../../../../design/_work/review-2026-10-03-M0d-W2解耦与策略.md) P1-4、§5-1)。

## Proposal

用户决定按推荐执行([round8-decisions](../../../../design/_work/round8-decisions.md) R8-11;规范见 [round8e 简报](../../../../design/_work/round8e-spec-brief.md) E6)。接口归 [hft-strategy-sdk](../../../../crates/hft-strategy-sdk/README.md#model-检查点),Note 码归 [hft-event](../../../../crates/hft-event/README.md),策略侧规则归 [portfolio-carry](../../../../strategies/portfolio-carry/README.md)。

1. 检查点两节:**原始样本节**(布局由 `ModelLayoutDesc` + `LAYOUT_VERSION` 管)与**路径状态节**(自带 `Model::PATH_VERSION: u32`;`Model::snapshot_path(&self, w)`、`Model::restore_path(&mut self, r)`;路径节作为检查点第一块写出)。
2. 恢复时 `PATH_VERSION` 不符 → 只丢路径节,派生状态按原始样本重算,写 `Note{ModelPathReset}`(短暂不一致可接受);原始样本仍有效,168 h 统计不随逻辑改动失效。
3. 有路径节时,"检查点 → 恢复 → 续算"与连续运行逐字节相同;路径节被丢弃的当日 recompute 从下一个分位带边界 + 平滑窗长度之后开始比较。
4. portfolio-carry 的分位带更新时刻对齐 UTC(`band_update_ms` 的整数倍边界)。

## Alternatives considered

**检查点存全部派生状态、只用一个版本号。** 任何派生逻辑的改动都使整份检查点失效,168 h 的样本要靠预热重来。

**只存原始样本,recompute 放宽到"派生状态收敛后再比较"。** 确定性要求变成启发式,⑦ 的"回放每日通过"失去逐字节的含义。

## Acceptance criteria

- SDK 确定性夹具:有路径节时"检查点 → 恢复 → 续算"与连续运行逐字节相同;`PATH_VERSION` 不符时只丢路径节、写 `Note{ModelPathReset}`、原始样本保留。
- 每日 recompute 照常要求逐字节一致;路径节被丢弃的日子按第 3 条的起点比较并在报告中注明。

## Risks

- 路径节的内容由策略决定,漏存某个路径依赖量时确定性测试会暴露,但只在触发该路径的数据上;夹具需覆盖分位带边界与平滑窗满/未满两种情形。
- 对齐 UTC 边界改变了分位带的更新相位,上线前的模拟结果与对齐前不可直接比较。
