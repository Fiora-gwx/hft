# AGENTS.md — 文档标准

核心规则:**一件事实只有一个归属地,别处链接**。

## 层级与归属

| 层级 | 职责 | 不放这里 |
|---|---|---|
| 根 [AGENTS.md](../AGENTS.md) | 每次会话都需要的命令式规则,每条一到三行并链接归属地 | 故事、例子、被链接页的复述 |
| 子树 `AGENTS.md` | 该子树特有的规则 | 根文件已有的规则 |
| [architecture.md](architecture.md) | 有序地图:部署、线程、事件流、状态所有权、分层、扩展点 | 类型定义、数值、决策理由 |
| [subsystems/](subsystems/README.md) | 跨 crate 的类型与协议参考页 | 行为叙述 |
| 组件 README 与 `TESTS.md` | README:实现规格与契约([模板](../crates/AGENTS.md#readme-模板));`TESTS.md`:用例与验收([格式](tests/README.md)) | 其他组件的内容;引擎侧不写策略族名与策略数字 |
| 策略包 README([strategies/AGENTS.md](../strategies/AGENTS.md)) | 该策略的规则、参数 schema、插件与验收 | 交易所名、引擎机制的复述 |
| [Agent Notes](../.agents/notes/README.md) | 决策记录:为什么、放弃了什么、需要什么验证 | 操作步骤、状态清单 |
| [cookbook/](cookbook)、[runbooks/](runbooks/README.md) | 带验证步骤的操作指南;生产操作 | 设计理由、代码规格 |
| [postmortem/](postmortem/README.md) | 事故叙述(唯一允许写故事的地方) | — |
| 横切参考:[latency-budget](latency-budget.md)、[hot-path-rules](hot-path-rules.md)、[testing](testing.md)、[tests/](tests/README.md)(跨组件场景、不变量、演练)、[defensive-patterns](defensive-patterns.md)、[development](development.md)、[glossary](glossary.md) | 全仓统一的定义与规则 | 某个 crate 专属的细节 |
| [../../architecture-v2/](../design/README.md) | 设计证据与取舍(允许写历史) | 实现规格 |

## 写法

- **写当前状态**。历史进 Agent Note、postmortem 或 git;行为变更按 [tests §7](tests/README.md#7-修改流程设计基线-v1-之后) 同一提交改 README、`TESTS.md` 与代码并记 [changes.md](changes.md);正文不写"新增了""改为""以前"一类的变更叙述;决策出处只在"延伸阅读"里链一次。
- **命名具体的东西**:写参数名与检查名,不写"风控会做一些保护"。
- **每个数字带来源或验收方式**:预算写明 p50/p99、分段、基准或直方图;调研数字链接 `architecture-v2/research/`。
- **待核实就标待核实**:写成"待核实(方法:…)"或指向开发备忘中的条目;"待标定""待实测"是合法变体;不把未核实的交易所行为写成事实。
- **开发备忘只放待核实项与开放问题**(可加实现顺序建议),不放跨文档同步清单:需要别处改的,同一改动里直接改归属地。
- **代码块只写签名与数据布局**,不写函数体;布局是契约草案,实现时以代码与编译期断言为准。
- **中文为主,标识符保持英文原样**;**一段一事**。

## 篇幅

上限:根 `AGENTS.md` ≤ 1,800;`architecture.md` ≤ 3,000;子树 `AGENTS.md` ≤ 900;组件 README 无硬上限。

**计量口径**(`doc-check` 与 `check_hft.py` 相同):去掉围栏代码块与链接 URL 后,汉字数(不含标点)+ 英文/数字词数(连续 `[A-Za-z0-9_.-]+` 且含字母或数字的计 1)。

超出时先把不属于这里的内容搬到归属地,再压缩,最后才提高上限。

## 劣质清单(审查时逐条找)

- 同一规则或数值出现两处,或在归属地之外重抄表格:留一个归属地,别处链接。
- 自称唯一归属却没有取值;手抄的码表与归属地或 `gen-schema` 输出分叉。
- 实现状态写进正文("已实现""以后再做""阶段 N""第 N 周"):状态在 frontmatter 与"已知限制与后续工作"。
- 引擎 README 或 `python/` 框架包出现策略族名(含小写 `cdex`)或策略规则数字;策略 README 出现交易所名(`doc-check` 机检)。
- README 里的推理过程与被否决方案:理由进 Agent Note。
- 模糊词("适当""尽量""高性能"):换成数值或参数名。
