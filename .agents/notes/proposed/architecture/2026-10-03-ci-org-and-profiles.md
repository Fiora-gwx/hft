# Agent Note: hft 仓库放进免费 GitHub 组织;构建剖面按用途拆开;CI 触发收窄

Status: proposed

实现期修订(2026-10-11):私有仓库与月度分钟门槛部分由 [公开仓库 CI](../../implemented/process/2026-10-10-public-repository-ci.md) 取代;构建与验证边界保留。正式版的 Admin 每夜豁免名为 `skip_nightly`,不再承担用量豁免。

> **第八轮第五次修订(2026-10-03)**:收窄第 2、6 条——`ci` 剖面显式 `panic = "unwind"`,PR 的基准冒烟用 `ci` 剖面;发布 `build` 超时 90 min、验证包 60 min;`nightly-arm` ≤ 10 min 并与 arm 发布共用缓存键;本月用量 ≥ 1,500 min 时每夜任务与开发构建自动跳过;应急工作流 `release-x86-cross.yml` 每季度演练;`main` 推送不取消进行中的运行。分钟数以 [development 免费额度](../../../../docs/development.md#免费额度)为准;依据 [round8e 简报](../../../../design/_work/round8e-spec-brief.md) E11。

> **第八轮第六次修订(2026-10-03)**:收窄第 6 条——用量由 quota-gate 按作业时长自估(GitHub 用量接口已关闭),quota-gate 与发布 `preflight` 接受 Admin 的 `quota_override`,过 1,500 min 后不带 `hotfix` 标签的 PR 只跑 docs 与 `decoupling-gate`,额度耗尽时经负责人个人账户的私有镜像仓库出包;`release-x86-cross.yml` 只用于 arm runner 不可用;`nightly-arm` 改用自己的缓存键。见 [ci-quota-self-estimate](../process/2026-10-03-ci-quota-self-estimate.md)。

> **实现期修订(2026-10-07,负责人决定)**:部分取代第 1 条——不建 GitHub 组织,仓库放负责人个人账户下的私有仓库 `Fiora-gwx/hft`,老板为协作者(个人私有仓库没有只读角色,老板实际有写权限;已知取舍,安全靠服务器只装经 `preflight` 的制品与发布工作流的 Admin 检查,`.github/hft-admins.txt` 只有 `Fiora-gwx`);2,000 min/月与 Trading 共享,1,500 min 规则按 hft 自估用量计,负责人每周对账时把 Trading 用量一并考虑;OIDC 信任策略 `repo:Fiora-gwx/hft:*`;deploy 容器测试只在改动 `deploy/` 或 `apps/hft-core/` 的 PR 上跑,每夜全量。规格以 [development](../../../../docs/development.md#仓库与-ci) 为准。

## Problem

第三次修订后的 CI 预算在现实里不成立。只读统计 Trading 仓库的 Actions 用量:8 月 530 min、9 月 411 min;ARM64 发布每次 15–17 min;CI 每次计费中位 9 min(无 LTO)。hft 与 Trading 共享个人账户的 2,000 min,扣掉每夜 750 min 与 Trading 的实际用量后只剩约 720–840 min;`ci` 剖面继承 `release` 的 fat LTO,PR 现实 15–30 计费分钟、发布 35–60 min;评审标签与评审事件会重跑整套质量门;发布 artifact 保留 7 天、账本 artifact 保留 90 天,Free 计划 artifact 存储只有 500 MB;额度用尽会把 Trading 的发布一起停掉。另外个人账户的私有仓库不能给协作者只读权限,也没有受保护分支([V4 评审](../../../../design/_work/review-2026-10-03-M0c-V4运行与交付.md) P0-1、P1-9)。

## Proposal

用户决定按推荐执行([round8-decisions](../../../../design/_work/round8-decisions.md) R8-10;落地规格见 [round8d 简报](../../../../design/_work/round8d-spec-brief.md) D1)。规格唯一归属 [development](../../../../docs/development.md)。

1. M1 时在一个**免费 GitHub Organization** 下建私有仓库(已由 2026-10-07 实现期修订取代:个人私有仓库 `Fiora-gwx/hft`):独立的 2,000 min/月、artifact 500 MB、cache 10 GB;负责人 Admin、老板 Read。没有受保护分支,生产安全以发布 `preflight` 证实该 SHA 的 `ci.yml` 全绿且 `decoupling-gate` 通过为准。
2. 构建剖面:`release` = thin LTO、16 个 codegen 单元;`release-hot` = fat LTO、1 个 codegen 单元,只用于正式版的 `hft-core` 与 `kill-switch`;`ci` = 继承 `release` 但关 LTO、关增量、开断言与溢出检查;`bench` = `release-hot`。开发构建只用 `release`。
3. `ci.yml` 只由 PR 的 opened/synchronize/reopened 与 `main` 推送触发;评审与标签事件只经 `decoupling.yml` 重跑 `decoupling-gate`;`python` 与 `operations` 合并为按路径的 `ops-py` 作业。
4. CI 全绿的 PR 头可经 `workflow_dispatch(sha, dev_n)` 直接出开发构建。
5. 发布 artifact `retention-days: 1`;CI 不保存累计账本;③④ 的累计移到服务器([verify-on-server](../process/2026-10-03-verify-on-server.md))。
6. 预算:每夜 x86 ≤ 15 + arm ≤ 5 min,发布类预留 ≥ 500 min/月,PR 与推送约 900 min/月、PR 目标 ≤ 12 计费分钟;M1 第一周按实测重算。
7. S3 fixtures 经 GitHub OIDC 扮演只读角色,只给需要的作业 `id-token: write`。

## Alternatives considered

**个人账户设约 $10/月的超额上限。** 能多约 1,600 min,但解决不了"老板只读"与受保护分支,也不把 Trading 与 hft 的额度隔开;评审把它列为备选。

**保留单一 `release` 剖面(fat LTO)给全部二进制。** 每个测试二进制与每次开发构建都要付 fat LTO 的时间;fat LTO 的收益只对热路径进程有意义,冷进程与测试不需要。

**评审与标签事件照旧触发整套质量门。** 同一 SHA 被重复构建;解耦豁免只需要重新判定标签与批准人,不需要重新编译。

**自托管 runner 或付费大规格 runner。** 用户此前已决定只用免费托管 runner([ci-free-hosted](2026-10-03-ci-free-hosted.md))。

## Acceptance criteria

- M1 第一周:组织仓库的 arm runner 可用性与倍率、PR/发布/开发构建/验证包的实测计费分钟写进 ops-log,development 的预算表据此更新。
- 连续两个月组织计费页显示用量 < 2,000 min,且没有一次因额度暂停 Actions。
- 一个 PR 的 `ci.yml` 计费分钟中位 ≤ 12;评审提交只产生 `decoupling.yml` 的一次 ≤ 2 min 运行。
- 发布 `preflight` 对 `ci.yml` 未全绿或 `decoupling-gate` 未通过的 SHA 失败(演练记录)。

## Risks

- 没有受保护分支:红的 PR 仍能被合并;靠 `preflight` 不让它进入发布,靠纪律不让它留在 `main`。
- `pull_request_review` 触发的运行与 PR 头 SHA 的关联方式待 M1 核实;关联不上时 `preflight` 会误判 `decoupling-gate` 缺失。
- `ci` 剖面关 LTO 后与发布二进制的代码生成不同:跨剖面的确定性靠整数运算与一致性集(正式版在 arm 上用固定种子比对 `HashTrail`)保证。
- 组织私有仓库的 arm runner 若不可用,备选是交叉编译,验证包的 fuzz 目标交叉编译可行性待核实。
