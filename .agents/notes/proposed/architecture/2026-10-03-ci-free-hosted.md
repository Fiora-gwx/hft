# Agent Note: CI 与旧项目一致,只用免费的 GitHub 托管 runner;长时间门槛改为跨夜累计

Status: proposed

> **第八轮第三次修订(2026-10-03)**:收窄第 2、3、6 条——账户是 GitHub Free(私有仓库 2,000 min/月,与 Trading 仓库共享):每夜 x86 ≤ 20 min + arm ≤ 5 min(约 750 min/月),PR 质量门目标 ≤ 10 计费分钟,每月为两个仓库的发布构建预留 ≥ 300 min,用量超过 80% 先缩每夜批量;预算表唯一归属 [development](../../../../docs/development.md#免费额度);③④ 累计门槛只在正式实盘前要求,漏跑一夜不清零、只有失败清零,账本丢失按 CI artifact 历史重建、无法重建才清零(规则归 testing §6);见 [no-small-live](2026-10-03-no-small-live.md)。

> **第八轮第四次修订(2026-10-03)**:部分取代——hft 仓库改放免费 GitHub 组织(独立的 2,000 min/月,老板为 Read;没有受保护分支,以发布 `preflight` 为准);每夜 x86 ≤ 15 min + arm ≤ 5 min,只做回归冒烟;③④ 的累计改在服务器上运行 CI 构建的验证包,CI 不再保存累计账本 artifact,`cargo xtask ledger rebuild` 不再存在;第 4 条"dev2 不跑 fuzz、DST"作废(dev2 在护栏内以最低优先级运行验证包);预算表仍唯一归属 development;见 [ci-org-and-profiles](2026-10-03-ci-org-and-profiles.md)、[verify-on-server](../process/2026-10-03-verify-on-server.md)。

## Problem

修订后的规格让全部 CI 跑在 aarch64 托管 runner 上,上线门槛要求每个 core 解码器连续 fuzz 24 h、DST 每夜 ≥ 50,000 种子连续 7 夜,arm runner 不可用时退到自托管的 Graviton runner([build-target-aarch64](../process/2026-10-01-build-target-aarch64.md)、[fuzz-and-deterministic-simulation](../testing/2026-09-30-fuzz-and-deterministic-simulation.md))。私有仓库的免费分钟数远不够这样的批量;评审要求定预算([独立评审](../../../../design/_work/review-2026-10-03-M0独立评审.md) §4 第 6 条)。用户决定:CI 走免费的,与旧项目一致([round8-decisions](../../../../design/_work/round8-decisions.md) R8-8 第 2 条)。

## Proposal

工作流与发布步骤归 [development](../../../../docs/development.md#ci-概要);什么时候跑什么与累计账本归 [testing §6](../../../../docs/testing.md#6-ci-运行矩阵);门槛数字归 [testing §8](../../../../docs/testing.md#8-上线门槛)。

1. **形状照搬 Trading 仓库**:`ci.yml` → `quality-gates.yml` 在 `ubuntu-latest`(x86_64;纯逻辑 crate 跨平台,ISA 专属代码做 aarch64 交叉检查);`release-aarch64.yml` 按 tag 在 `ubuntu-24.04-arm`(`container: ubuntu:24.04`)原生构建发布制品,`preflight` 复用已绿的 SHA;作业条件显式判定上游结果。
2. **每夜 ≤ 40 min**:x86 主批次 ≤ 30 min(DST 一批新种子 + fuzz 按解码器轮转,语料用 cache/artifact 持久)+ arm 专项 ≤ 10 min;版本标签作业另跑 aarch64 一致性集(固定种子的 `HashTrail` 与 x86 结果逐字节比较)。
3. **门槛改为累计量**:每个 core 解码器累计 fuzz ≥ 2 h、零 panic;DST 累计 ≥ 200 000 种子、零违反,且覆盖 ≥ 7 次每夜运行;任何失败使对应计数清零。
4. **不用自托管 runner;dev2 不跑 fuzz、DST 与构建**:dev2 只以低优先级每日运行已安装制品的 `replayer verify`(受护栏约束)。
5. 开发构建只在打 `vX.Y.Z-dev.N` 标签时产出;合并不自动构建。
6. 额度以账户的免费分钟数为准(待核实);超出时减少每夜批量,不加付费资源。arm runner 对私有仓库不可用时的备选:`ubuntu-latest` 交叉编译(需负责人决定)。

## Alternatives considered

**自托管 Graviton runner(修订前的退路)。** 多一台常开实例的费用与维护;runner 持有仓库访问权、跑在 AWS 账户里,安全面变大;用户选择免费的托管 runner。

**付费的大规格托管 runner。** 用户决定只用免费的。

**在 dev2 上跑 fuzz 与 DST。** dev2 与 cdex 正式账户共机,cdex 优先;长时间满载 CPU 正是护栏要防的事。

**保留连续 24 h fuzz 与 7×50 000 种子。** 免费额度内不可能;持久化语料的跨夜累计能覆盖相近的输入空间,代价是到达门槛的日历时间变长。

**全部 CI 都跑在 arm runner。** 私有仓库的可用性与计费倍率待核实;纯逻辑在 x86 上测试足够,ISA 相关的由每夜 arm 专项与标签一致性集覆盖。

## Acceptance criteria

- 工作流通过 `actionlint`;没有任何自托管 runner 注册到仓库;dev2 上没有 CI 作业、fuzz 或 DST 进程。
- 连续两个月的 Actions 用量在账户免费额度内(计费页截图)。
- 每夜账本 artifact 连续存在,计数与清零记录可追溯;标签作业的跨架构 `HashTrail` 比较通过才发布。

## Risks

- 累计 fuzz 不如连续长跑深入:语料持久与按累计时间最少优先轮转缓解;门槛到达需要数月。
- 账本或语料 artifact 过期即清零:每夜运行保证续期;缺失时清零并告警,不猜测。
- 托管 runner 的可用性与计费规则变化:备选方案已写明,启用需负责人决定。
