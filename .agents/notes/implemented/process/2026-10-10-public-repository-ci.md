# Agent Note: 公开仓库取消 CI 分钟门禁

Status: implemented

## Problem

私有仓库的分钟保护规则使 M2 的完整测试被跳过,即使其他质量门成功也不能据此发布。负责人于 2026-10-10 提出将仓库公开并删除额度脚本,随后自行完成可见性切换。GitHub API 已确认 `Fiora-gwx/hft` 为 public。

## Decision

公开仓库使用 GitHub 标准托管 runner,不再估算月度分钟数或按用量跳过 PR、每夜任务与开发构建。免费范围和资源规则归 [development](../../../../docs/development.md#免费额度),依据为 [GitHub 官方计费说明](https://docs.github.com/en/billing/concepts/product-billing/github-actions)。

旧脚本同时承担的 API 查询与 Admin 名单读取保留为共用模块。正式版的每夜豁免改名 `skip_nightly`,明确其只影响每夜结论;CI、解耦检查与 main 归属仍必须满足。覆盖写入发布证据。用例归 [tools/TESTS](../../../../tools/TESTS.md#tool-188-正式版-admin-覆盖只影响指定检查)。

本决定部分取代 [用量自估](../../proposed/process/2026-10-03-ci-quota-self-estimate.md)、[仓库与构建剖面](../../proposed/architecture/2026-10-03-ci-org-and-profiles.md)及[免费托管 runner](../../proposed/architecture/2026-10-03-ci-free-hosted.md)中的私有仓库与分钟预算安排。其他发布、构建和服务器累计验证约束保留。

## Alternatives considered

- 保持私有并暂时添加 `hotfix` 标签。此前已向负责人提出,负责人选择保留修改但不越过额度阈值;只能延后测试。
- 保持私有并增加付费分钟。可维持访问限制,但负责人选择公开仓库使用标准 runner,没有授权新增付费资源。

## Consequences

完整质量门可继续执行,不需要人为添加标签。公开仓库的源代码和历史可被读取和复制。标准 runner 分钟免费不包含所有存储、larger runner 或云服务费用;作业超时、缓存和制品保留策略继续生效。删除额度判断不能被当作放宽发布验证的理由。
