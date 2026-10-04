# 规格编写任务简报(所有子代理先读)

## 背景

我们在为一个从零重建的低延迟交易系统写**实现规格**(还不写代码)。设计长文在 `E:\trading\architecture-v2\`(00–13 章 + `research/` 调研笔记),其中 **13-设计评审与优化.md** 是最新评审,它对前面各章的修正已写回各章;若章节与 13 冲突,以 13 为准。

代码仓库骨架在 `E:\trading\hft\`,文档体系仿照 DeepSeek Harness(DSH):根 `AGENTS.md` 常驻规则、`docs/architecture.md` 有序地图、每个目录一份 README 作为该组件的规格与契约、`.agents/notes/` 决策记录、"一件事实只有一个归属地"。DSH 的原文样例在 `E:\trading\architecture-v2\research\_dsh\`(`packages_core_session_README.md` 是 README 风格样例;`notes_README.md` 是 Agent Note 格式;`AGENTS.md`、`docs_AGENTS.md`、`docs_architecture.md`)。

## 开始前必读(按顺序)

1. `E:\trading\hft\AGENTS.md`
2. `E:\trading\hft\docs\AGENTS.md`(文档标准)
3. `E:\trading\hft\docs\architecture.md`(地图、线程、状态所有权、类型归属 §10)
4. `E:\trading\hft\crates\AGENTS.md`(tier、依赖方向、**README 模板**)
5. `E:\trading\hft\docs\glossary.md`、`E:\trading\hft\docs\subsystems\event-model.md`
6. `E:\trading\architecture-v2\13-设计评审与优化.md`
7. 你的任务指定的设计章节与调研笔记;需要时读现有代码(`E:\trading\Trading\` 是现有 Rust 采集系统 + `cdex-arb/` Python 引擎)以取得真实规则与字段知识。

## 写作要求

- **中文为主,标识符保持英文**(crate 名、类型名、字段名、命令名、交易所频道名)。
- **严格按 crates/AGENTS.md 的 README 模板**:frontmatter(description/kind/tier/phase/status: spec)→ 概要 → 目录 → 使用本组件 → 实现说明(`<details>`)→ 延迟与资源契约 → 测试与验收 → 依赖 → 延伸阅读 → 已知限制与后续工作 → 开发备忘(`<details>`)。apps / python 包 / 其他目录同样用这个模板(kind 相应改为 app / python-package / dir,不适用的节写一句"无"并说明原因,不要删节)。
- **不写函数体**。只写:类型与 trait 签名、`#[repr(C)]` 布局、状态机图(ASCII)、算法步骤(编号列表)、规划的源文件表(文件 → 职责)、配置项表(名称/类型/含义/来源)。
- **要具体到实现者照着就能写**:数据结构的字段、容量与预分配大小、不变量、错误如何表示(位标志/枚举)、线程归属、每个公开入口的预算与基准名、测试用例清单(尤其失败路径)、验收数字。设计章节里的数字要带过来(并链接出处),不要泛泛而谈。
- **一件事实一个归属地**:跨 crate 的类型以 `docs/architecture.md §10` 的归属为准——在归属 crate 里定义,别处链接;事件体以 `docs/subsystems/event-model.md` 为准。
- **待核实就标待核实**:交易所行为、限速数字等未经官方确认的,放"开发备忘"并写核实方法;调研笔记里标"待核实"的保持待核实。已知更正:Binance USDT-M 每连接最多 1024 个流、每连接每秒入向消息 ≤ 10;fstream 有 `/public`、`/market`、`/private` 路由;Binance 合约文档已无 `@depth@0ms`;Bitget SBE 与 "LOLA" 官方未找到。
- **相对链接必须正确**:从 `hft/crates/<x>/README.md` 到设计文档是 `../../../architecture-v2/…`;到其他 crate 是 `../<y>/README.md`;到 docs 是 `../../docs/…`。从 `hft/docs/subsystems/*.md` 到设计文档是 `../../../architecture-v2/…`。
- **大文件用 Write 工具写**,不要用 Bash heredoc(命令超过约 8 KB 会被截断)。文件 UTF-8。
- 篇幅:crate README 通常 6–15 KB;关键组件(sequencer、oms、journal、venue 运行时、chain-evm、hft-core)可以更长。宁可具体,不要注水;不要复述 architecture.md。

## 边界

- 只写你任务清单里的文件。发现共享文件(`AGENTS.md`、`docs/architecture.md`、`docs/glossary.md`、`docs/subsystems/event-model.md`、`crates/AGENTS.md`)有错误或与你的规格冲突时,**不要改它们**(除非你的任务明确授权),在最终报告里列出:文件、问题、建议改法。
- 不改 `E:\trading\architecture-v2\` 下的任何章节。
- 不运行构建、不联网抓取(需要的事实在调研笔记里;实在没有就标待核实)。

## 最终报告(简短)

1. 写了哪些文件(路径 + 大小级别)。
2. 与共享文件或其他组件的冲突/不一致(具体到文件与段落)。
3. 你认为设计里仍然缺的东西或有风险的地方(≤ 5 条)。
