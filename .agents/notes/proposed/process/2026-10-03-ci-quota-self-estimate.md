# Agent Note: CI 用量自估、Admin 覆盖、hotfix 标签与额度耗尽的镜像应急

Status: proposed

## Problem

quota-gate 计划读 GitHub 的 workflow run 用量接口,而它与按产品的计费接口都已关闭,兜底的"每周手填"会让阈值滞后一周。用量过 1,500 min 后每夜任务被跳过,而正式版的 preflight 要求最近一次实际执行的每夜是绿的——每夜恰好是红的那个月,任何热修都出不去;过 1,500 min 后 PR 与推送照常跑,最后 500 min 没有保护,额度耗尽后全部 Actions 停止,而交叉编译工作流用同一份额度,不是出路([X4 评审](../../../../design/_work/review-2026-10-03-M0e-X4运行与交付.md) N-P1-3、P2-5、§5-2)。

## Proposal

用户决定按推荐执行([round8-decisions](../../../../design/_work/round8-decisions.md) R8-12;规范见 [round8f 简报](../../../../design/_work/round8f-spec-brief.md) F5)。规格唯一归属 [development 免费额度](../../../../docs/development.md#免费额度)。

1. quota-gate 用 `actions: read` 列出本月 workflow run 的作业,按作业时长(向上取整到分钟)与 runner 倍率自估用量;每周与计费页人工对账一次,偏差写 ops-log。
2. quota-gate 与发布 `preflight` 接受 Admin 覆盖:`workflow_dispatch` 输入 `quota_override: true`,只认名单内的 `github.actor`,覆盖写作业摘要与 `RELEASE.json.evidence`。
3. 用量过 1,500 min 后,不带 `hotfix` 标签的 PR 只跑 docs 检查与 `decoupling-gate`;带 `hotfix` 的 PR 照常跑全部质量门。
4. 额度耗尽的应急:把仓库镜像到负责人个人账户的私有仓库,在那里跑 `ci.yml` 与 `release-aarch64.yml` 出包(仍免费,占 Trading 所在的个人额度),再把资产发布到组织仓库的同一标签;步骤写在 development 并每季度演练一次。`release-x86-cross.yml` 只用于 arm runner 不可用,不是额度路径。

## Alternatives considered

**接受"到月初之前不能热修"。** 评审列出的另一选择;实盘期间整月出不了热修的风险不可接受。

**给组织加付款方式、设超额上限。** 违反"只用免费资源"的既定决定([ci-free-hosted](../architecture/2026-10-03-ci-free-hosted.md))。

**继续由负责人每周手填用量。** 阈值最多滞后一周,正好在月末最需要时失准。

## Acceptance criteria

- M1 第一周:自估值与组织计费页的差 ≤ 5%(或差额已写进倍率表),写 ops-log。
- 演练:每夜为红时 Admin 以 `quota_override` 出一个正式版,摘要与 `RELEASE.json.evidence` 记录覆盖人;非 Admin 覆盖被拒。
- 季度演练:镜像仓库出一个开发构建,dev2 上安装到 `sim` 组并通过第一分钟验收。

## Risks

- 自估不含 GitHub 的计费细节(例如以后改变的倍率),靠每周对账发现。
- 镜像应急占用 Trading 的个人额度;两边同月都紧时只能取舍,由负责人决定。
