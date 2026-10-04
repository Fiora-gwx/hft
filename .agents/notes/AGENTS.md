# AGENTS.md — Agent Notes

本子树的规则补充根 [AGENTS.md](../../AGENTS.md#常驻规则)。格式、生命周期、分类与取代规则的唯一定义在 [README.md](README.md);本文件只列写 Note 时最常出错的地方。

- **先找再写**。在 `proposed/`、`implemented/`、`rejected/` 里搜索主题;已有 Note 拥有这个决定就更新它,不新建重复的。
- **路径就是元数据**:`{proposed|implemented|rejected}/{architecture|process|testing|feature|bug-fix|simplification}/yyyy-mm-dd-topic.md`;日期是首次提出日期,`topic` 用英文短横线小写。
- **头三行固定**:`# Agent Note: <中文标题>`、空行、`Status: <proposed|implemented|rejected — 原因>`;`Status` 与所在目录一致。
- **节标题用英文规范名**(`## Problem`、`## Proposal`、`## Decision`、`## Alternatives considered`、`## Acceptance criteria`、`## Risks`、`## Consequences`),正文用中文;格式检查冻结期间 doc-check 只查链接,格式由评审把关(解冻后按英文标题检查)。
- **`## Alternatives considered` 必填**,写真实被讨论或调研过的方案与它输掉的原因,链接证据;不编造稻草人。
- **证据用相对链接**:从 `proposed/<class>/x.md` 到设计文档是 `../../../../../architecture-v2/…`,到 hft 文档是 `../../../../docs/…`,到 crate 是 `../../../../crates/<x>/README.md`,到同生命周期其他 Note 是 `../<class>/y.md`。
- **只写理由,不写规格**。数据布局、接口签名、配置表属于组件 README;操作步骤属于 cookbook/runbook;事故经过属于 postmortem。Note 链接它们,不复述。
- **数字带出处**:预算、成本、实测值链接 architecture-v2 章节、research 笔记或基准输出;未核实的标"待核实"并写核实方法。
- **`## Acceptance criteria` 可观察、带数字**:写"`tick_to_trade` p99 ≤ 60 µs 连续 7 天",不写"延迟足够低"。
- **篇幅 1–3 KB**。超过 5 KB 先搬走不属于这里的内容。
- **决定变了就取代**,不改写成另一个决定;新旧 Note 互相链接(见 README "取代与合并")。被部分取代或收窄的 `proposed/` Note 留在原处,只在 `Status:` 行后加一条"第 N 轮"状态说明并链接新 Note;同时在 README 的当前范围入口表里登记。
- **落地时移动**:第一个实现合入的同一 PR 里把 Note 从 `proposed/` 移到 `implemented/` 并按骨架改写。
