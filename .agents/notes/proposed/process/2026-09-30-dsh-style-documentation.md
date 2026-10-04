# Agent Note: 采用 DeepSeek Harness 式文档体系

Status: proposed

> **第八轮第六次修订(2026-10-03)**:补充——M0-f 之后规格冻结为设计基线 v1,只走实现驱动的变更(同一提交改 README、TESTS.md 与代码并在 `docs/changes.md` 记一行);见 [design-baseline-v1](2026-10-03-design-baseline-v1.md)。

## Problem

新仓库在写代码之前先写规格,由两名工程师与多个 AI 代理并行实现。现有 Trading 仓库的决策散在设计长文与 `AGENTS.md` 里,没有决策记录目录([research/00 §5.1](../../../../design/research/00-existing-codebase-inventory.md));architecture-v2 的 13 章长文适合论证,不适合作为实现时的契约——同一事实在多章出现(如事件头 01 §5 与 08 §2 两份定义,[13 R5](../../../../design/13-设计评审与优化.md)),代理按其中一处实现就会分叉。需要一个让代理与人都能"读一页就能动手、一件事实只有一个地方"的文档体系。

## Proposal

仿照 DeepSeek Harness(样例在 research/_dsh(未入库的资料缓存:`architecture-v2/research/_dsh/README.md`)):

1. 根 [AGENTS.md](../../../../AGENTS.md) 放常驻规则(每条一到三行并链接归属地);子树 `AGENTS.md` 放该子树特有规则。
2. [docs/architecture.md](../../../../docs/architecture.md) 是有序地图;跨 crate 的类型与协议在 `docs/subsystems/`;横切定义在 `latency-budget.md`、`hot-path-rules.md`、`testing.md`、`defensive-patterns.md`、`development.md`。
3. 每个目录的 `README.md` 是该组件的**实现规格**,按 [crates/AGENTS.md](../../../../crates/AGENTS.md#readme-模板) 的固定模板;`status: spec` 在首次合入实现时改为 `impl`。
4. 决策理由写 Agent Note,格式见 [.agents/notes/README.md](../../README.md);操作步骤写 cookbook/runbook;事故经过只写 postmortem。
5. 机器检查:`cargo xtask doc-check` 检查链接、README 结构、预算表与基准一致、Agent Note 格式。

**改编点**:

| DSH | 本仓库 |
|---|---|
| 英文正文 + `.zh.md` 对照 + 一致性 sidecar | 中文正文、标识符英文;不做双语对照(团队单语),机器检查标记(`# Agent Note:`、`Status:`、`##` 节名)保留英文 |
| `archived/` 冻结树与归档技能 | 暂不设;规格仓库期间所有 Note 都是活动的,需要时再写 Note 引入 |
| 包 README 描述已实现的服务 | README 先作为规格存在,frontmatter 带 `tier`/`scope`/`status` |
| TypeScript 工具(`pnpm run verify-*`) | `cargo xtask doc-check` |
| 基准按用户路径组织(Vitest workers) | 按热路径组织,区分参考机与 CI 伸缩([benchmarks](../../../../benchmarks/AGENTS.md)) |

## Alternatives considered

**只保留 architecture-v2 长文 + 代码注释。** 长文是论证,不是契约;同一事实多处出现,代理无从判断以哪处为准。长文保留为证据库,在仓库建成 git 仓库时迁入 `docs/design/`。

**经典 ADR 目录(编号 ADR-0001…)。** 编号不携带状态与类别;DSH 的路径编码(生命周期/类别/日期)可被机器检查,且状态变化就是移动文件。

**一份大 SPEC.md。** 无法按组件分配给并行的代理;修改冲突频繁;归属不清。

## Acceptance criteria

- 每个组件目录有符合模板的 README;`doc-check` 在 CI 中通过。
- 实现期间的每个 PR:代码与规格不一致时,同一 PR 更新 README 或新增 Agent Note(评审清单项)。
- 抽查:任选一条跨 crate 类型,仓库中只有一处定义,其余为链接。

## Risks

- 规格与代码漂移:以 `status` 字段与评审纪律约束,`doc-check` 能查结构但查不了语义。
- 文档量大、维护成本高:篇幅上限写在 [docs/AGENTS.md](../../../../docs/AGENTS.md);超限先搬走不属于这里的内容。
