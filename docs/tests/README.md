# 测试规格:格式与规则

本页规定 `TESTS.md`(组件测试规格)与 `docs/tests/` 下跨组件测试规格的写法、编号、覆盖要求、与代码的对应方式和修改流程。测试的层次、CI 运行矩阵与上线门槛归 [testing.md](../testing.md);本页只管"用例怎么写、怎么与规格和代码对上"。来源:[负责人决定 R8-12](../../design/_work/round8-decisions.md)。

## 1. 地位

1. **测试规格是验收合同**。README 写规则("是什么、为什么");`TESTS.md` 写"给定什么输入,必须看到什么输出"。一个组件的实现以"它的 `TESTS.md` 中里程碑已到的用例全部有实现且通过"为完成标准。
2. **一条规则至少一个用例**。README 的每个规范性小节都要在覆盖表(§5)里对应到用例,或写明不适用的理由。
3. **不一致就是缺陷**。README 与 `TESTS.md` 说法不同、用例的期望无法由 README 推出、或两个用例互相矛盾,都按缺陷处理:在同一提交里改正 README、`TESTS.md` 与代码(§7)。
4. **评审以测试规格为准**。设计基线 v1 冻结后不再做全量文档评审;PR 评审检查三件事:改动的行为有用例、用例具体可判定、实现与用例一致。

## 2. 文件与范围

| 文件 | 内容 |
|---|---|
| `<组件目录>/TESTS.md` | 该组件自己的用例(单元、属性、金样、fuzz、基准、组件级 DST 与集成) |
| [docs/tests/scenarios.md](scenarios.md) | 跨组件场景:模式与人工干预、恢复、OMS 与对账、执行正确性、断腿与保证金、失败路径(原 testing §3.6、§4.1–§4.4、§5 的表) |
| [docs/tests/invariants.md](invariants.md) | 全局不变量 `INV-n`(DST 与属性测试每一步都断言) |
| [docs/tests/drills.md](drills.md) | 主机、测试网与运维演练(需要真实主机、交易所或人工操作的用例) |

**必须有 `TESTS.md` 的组件**:README frontmatter 为 `scope: core` 且 `status` 不是 `frozen` 的 crate、app 与 python 包,以及 `tools/`、`deploy/`。例外:`strategies/portfolio-carry`(含其 `pipeline/`)的测试规格随策略研究再写([负责人决定](../../design/_work/round8-decisions.md))。冻结组件解冻时补写。

## 3. 编号

- 组件用例:`<前缀>-<三位数>`,如 `OMS-014`。前缀见下表,全仓唯一;编号在文件内递增,**删除的编号不复用**(保留标题,正文写"已删除:<原因>")。
- 表驱动用例的行:`<前缀>-<三位数>.<行号>`,如 `OMS-030.4`。
- 跨组件场景沿用已被各 README 引用的稳定编号(`A5`、`B1`–`B6`、`B-1`…`B-22`、`C1`–`C15`、`BK-n`、`LK-n`、`FD-n`、`AX-n`、`MS-n`、`QF-n`、`AL-n`、`FL-n`、`LT-n`),新增族在 [scenarios.md](scenarios.md) 页首登记;不变量 `INV-n`;演练 `DRL-n`。
- 全仓所有编号唯一(机检)。

| 组件 | 前缀 | 组件 | 前缀 | 组件 | 前缀 |
|---|---|---|---|---|---|
| hft-types | TYP | hft-strategy-sdk | SDK | hft-core | CORE |
| hft-event | EVT | hft-strat-lib | SLB | replayer | RPL |
| hft-refdata | REF | hft-exec | EXE | reconciler | RCN |
| hft-rt | RT | hft-oms | OMS | watchtower | WT |
| hft-journal | JNL | hft-risk | RSK | kill-switch | KS |
| hft-book | BOOK | hft-ledger | LED | journal-archiver | ARC |
| hft-net | NET | hft-sequencer | SEQ | latency-probe | LPR |
| hft-venue-proto | VPR | hft-sim | SIM | registry | REG |
| hft-venue | VEN | hft-ctl-proto | CTP | tools | TOOL |
| venue-binance | BIN | hft-ctl | CTL | deploy | DEP |
| venue-okx | OKX | hft-telemetry | TEL | ops_bot | OPS |
| venue-bitget | BGT | hft-testkit | TK | pipeline | PIP |
| hft-py | PY | book-observer | OBS | treasury | TRS |
| latency-slice | SLC | | | | |
| portfolio-carry | CARRY | | | | |

## 4. 用例格式

文件头:

```markdown
---
component: hft-oms
prefix: OMS
spec: README.md
---

# hft-oms 测试规格

格式与规则见 [docs/tests](README.md)。

## 覆盖
(§5 的覆盖表)

## 用例
(各用例)

## 验收
(本组件的整体验收条件:属性测试例数、DST 门槛、联调指标;数值归属见各自出处,本节链接)
```

每个用例是一个三级标题加一个字段列表。标题是一句行为陈述(不是"测试 xx")。

```markdown
### OMS-014 链接腿查单 NotFound 在成交历史确认前永不定 Rejected

- 规则:[Unknown 收敛流程](README.md#unknown-收敛流程)
- 级别:unit
- 里程碑:M2
- 前置:venue `stale_guard = RecvWindow`;`notfound_grace_ms = 5000`;槽 s1 为 PairOnFill 链接的第二腿,状态 `Unknown`
- 步骤:
  1. 投递 `QueryResult{slot = s1, status = NotFound, history_checked = 0}`,`now = 第一次查询 + 6000 ms`
  2. 投递 `QueryResult{slot = s1, status = NotFound, history_checked = 1}`
- 期望:
  - 步骤 1 后 s1 仍为 `Unknown`;输出恰好一条 `OmsNeedsRestQuery{slot = s1, trades = 1}`;`query_count` 加 1;无 `OrderDecision`
  - 步骤 2 后 s1 为 `Rejected(NOT_FOUND)`;链接按截止规则处理(见 OMS-031)
- 实现:`crates/hft-oms/tests/unknown.rs::oms_014_linked_notfound_waits_history`
```

表驱动与属性测试的写法:

```markdown
### TYP-020 十进制字符串解析为定点数

- 规则:[字符串 ↔ 定点](README.md#字符串--定点逐字节)
- 级别:table
- 里程碑:M1
- 前置:`qty_exp = -8`
- 实现:`crates/hft-types/tests/parse.rs::typ_020_parse_decimal`

| 行 | 输入 | 期望 |
|---|---|---|
| TYP-020.1 | `"0.00000001"` | `Ok(Qty(1))` |
| TYP-020.2 | `"0.000000001"` | `Err(FxErr::PRECISION)`(多一位小数) |
| TYP-020.3 | `"46116860184.27387903"` | `Ok(Qty(4611686018427387903))`(`MAX_RAW`) |
| TYP-020.4 | `"46116860184.27387904"` | `Err(FxErr::OVERFLOW)`(超过 `MAX_RAW`) |

### OMS-120 任意回报交错下终态与成交量等于参考模型

- 规则:[订单状态机](README.md#订单状态机)、[回报处理表](README.md#回报处理表)
- 级别:prop
- 里程碑:M2
- 性质:任意 place/cancel/amend 与交易所回报的交错、重复、乱序、丢失序列执行完毕后,每个槽的 `filled` 与终态等于参考模型;不变量 INV-2、INV-7 每步成立
- 生成:1–64 张订单;每张 0–6 条回报,从 {Ack, PartialFill, Fill, CancelAck, Reject, 重复上一条} 抽取,随机置换顺序,每条以 5% 概率丢弃
- 参考:`tests/model.rs` 的 `Vec<Order>` 语义实现
- 例数:PR 2 000;每夜 100 000
- 实现:`crates/hft-oms/tests/prop_fsm.rs::oms_120_fsm_matches_model`
```

DST 场景写 `故障`(何时注入什么)、`种子` 与 `断言`;`步骤` 选填:

```markdown
### EXE-210 第二腿盘口 10 s 不变而连接健康时照常对冲

- 规则:[对冲定价](README.md#对冲定价)
- 级别:dst
- 里程碑:M2
- 门槛:④
- 前置:两腿 venue A(现货)、B(永续);`conn_liveness_ms = 1000`;策略在 t = 0 发 PairOnFill 开仓
- 故障:B 的 BBO 自 t = −10 s 起不变,连接每 500 ms 回 pong;A 的第一腿在 t = 0.2 s 全部成交
- 种子:PR 200(提交派生)+ 回归集;每夜日期派生
- 断言:第二腿 `OrderDecision{New}` 在第一腿成交的同一 Input 内发出,无 `StaleQuote` 拒绝;INV-3、INV-8、INV-19
- 实现:`crates/hft-testkit/src/scenarios/exec.rs::exe_210_quiet_book_still_hedges`
```

**必填字段**(所有级别):`规则`(指向 README 小节锚点的链接,可多个)、`级别`、`里程碑`、`实现`;再按级别另需下表"必填"列的字段。**选填**:`前置`(非必填的级别也可写)、`夹具`(fixtures 路径)、`门槛`(上线门槛编号 ①–⑪)、`不变量`(`INV-n`)、`状态`(只允许两种:"隔离:<Agent Note 链接>",或"可选扩展:<扩展名>"——属于未启用的可选扩展,扩展名与 [11 §1](../../design/11-实施路线图.md) 的可选扩展表一致)、`备注`。字段写成 `- 字段:值`,多行值缩进两格续写。

| 级别 | 含义 | 在哪跑 | 必填 |
|---|---|---|---|
| `unit` | 单 crate 确定性测试 | PR | `前置`、`步骤`、`期望` |
| `table` | 表驱动:一张"输入 → 期望"表,每行一个子用例 | PR | `前置`;正文一张表,首列为行号 `<编号>.<k>`,其余列至少有输入与期望 |
| `prop` | 属性测试(proptest,有参考模型或不变量) | PR | `性质`、`生成`(取值域)、`参考`(参考模型或断言)、`例数`(PR 例数;大例数运行写在 `## 验收`) |
| `golden` | 金样:固定输入 → 逐字段相等的期望输出 | PR | `输入`、`期望文件`(fixtures 路径) |
| `fuzz` | fuzz 目标 | PR 冒烟、每夜、服务器累计 | `目标`、`语料`、`判定`(崩溃与超时口径链接 testing §3.3) |
| `bench` | 基准 | PR 只报告;参考机判定 | `基准名`(数值上限只在 [benchmarks](../../benchmarks/README.md) 登记,这里不写数值) |
| `dst` | 确定性模拟场景(hft-testkit) | PR 冒烟、每夜、服务器累计 | `故障`(注入表)、`种子`(PR / 每夜 / 回归集)、`断言`(`INV-n` 与本场景特有断言) |
| `integ` | 进程内多组件集成(假交易所、真实 sequencer) | PR | `前置`、`步骤`、`期望` |
| `proc` | 多进程(Linux,CI 中真实进程与 socket) | PR 或每夜 | `前置`、`步骤`、`期望` |
| `replay` | `replayer verify` 样本 | PR | `输入`(fixtures/replay 指针)、`期望` |
| `host` | 在 dev2 或升级主机上运行 | 按里程碑 | `环境`、`授权`(`[需授权]` 或"无")、`步骤`、`通过标准`、`证据`(记录写到哪里) |
| `testnet` | 交易所测试网/模拟盘 | dev2 联调窗口 | 同 `host` |
| `drill` | 运维演练(人参与) | 按 runbook | 同 `host`;`步骤` 链接 runbook 小节 |

**写法规则**(机检的项目标 ✔):

1. ✔ `期望` 必须可观察:返回值、输出事件及其字段、状态字段、journal 记录、计数器、Note 码、退出码、文件内容。不许写"正确""正常""合理""适当""符合预期""等等""之类"。
2. **参数在 `前置` 里显式设定**,不依赖默认值(例:`前置:hedge_deadline_ms = 3000`)。默认值本身只在归属组件的 `TESTS.md` 里由一条"默认值"用例覆盖。这样默认值改动不会让别的组件的用例悄悄失效,也不在归属地外重抄数值。
3. 一个用例只验证一条行为;成功路径与失败路径分开编号。失败路径(拒绝、超时、断连、乱序、重复、截断、溢出、边界值 ±1)不少于成功路径([testing §1](../testing.md#1-原则) 第 3 条)。
4. 边界值写三点:边界、边界 − 1 单位、边界 + 1 单位(单位取该量的最小步长:1 ns、1 ms、1 tick、1 lot、1 raw)。
5. 数值写具体值,定点数写成"十进制值(raw 值,指数)"一次,之后可只写十进制值。
6. ✔ 不重抄 README 的规则文字:用 `规则` 链接,`期望` 只写这一输入下的具体结果。
7. 跨组件的期望(例如"ledger 与 LegTracker 一致")写成 `integ`/`dst` 用例放在发起方组件或 [scenarios.md](scenarios.md),不在两个组件里各写一半。

## 5. 覆盖表

`## 覆盖` 下一张两列表:`| README 小节 | 用例 |`。

- ✔ README 中代码块以外的每个 `##`、`###` 标题都必须作为一行出现(按标题文字精确匹配),每行至少一个用例编号或编号区间(`OMS-010–OMS-024`),或 `不适用:<理由>`。
- 允许的理由:`非规范`(概要、目录、测试与验收(README 中只保留指向本文件的链接)、依赖、延伸阅读、已知限制与后续工作、开发备忘)、`见子节`(该标题下只有引导语,规则都在它的 `###` 子节里;引导语里若有规则,必须列出覆盖它的编号而不是写见子节)、`由 <编号> 覆盖`(规则由别处的用例验证,如跨组件场景)、`里程碑外`(属于未启用的可选扩展)。其他理由不接受。
- ✔ 覆盖表里出现的编号都必须在本文件或 `docs/tests/` 中存在;每个用例都至少被覆盖表引用一次。
- ✔ `规则` 字段的锚点必须存在。

## 6. 与代码的对应

- 测试函数名以用例编号开头:`<前缀小写>_<三位数>_<描述>`,如 `oms_014_linked_notfound_waits_history`;跨组件场景编号(`B1`、`B-6`、`BK-1`、`MS-2a` 等)取编号小写、`-` 换成 `_` 作前缀:`<编号小写>_<描述>`,如 `b1_link_open_…`、`b_6_venue_…`、`bk_1_…`,`<描述>` 不以数字开头(`b1_6_…` 是表行 B1.6,不算 B1;`b_10_…` 是 B-10,不算 B-1)。Python(pytest)的测试函数必须以 `test_` 开头,写作 `test_<前缀小写>_<三位数>_<描述>`。表驱动用例的每行用 `rstest` 的 `#[case::<前缀小写>_<三位数>_<行号>]` 或在循环断言信息中带行号;属性测试的 `proptest!` 函数同名规则;DST 场景以编号注册(`Scenario::new("OMS-201", …)`);金样目录名含编号;基准的 Criterion id 等于 `基准名`;`host`/`testnet`/`drill` 用例的脚本放 `tests/host/<编号>.sh` 或指向 runbook 步骤,证据文件名含编号。
- `实现` 字段写 `<路径>::<函数名>`;尚未实现的写 `待实现`。
- **追溯检查** `cargo xtask test-trace`(CI `ci.yml` 的文档作业,每个 PR):
  1. 解析全仓 `TESTS.md` 与 `docs/tests/*.md`(本页除外);xtask 测试的夹具仓库 `tools/xtask/tests/fixtures/` 既不读其规格也不扫其代码;
  2. 级别为 `unit`/`table`/`prop`/`golden`/`fuzz`/`bench`/`dst`/`integ`/`proc`/`replay`、且里程碑 ≤ 当前里程碑(`tools/xtask/test-trace.toml` 的 `current_milestone`)的用例,必须有同编号的测试函数、场景或基准,且 `实现` 字段与找到的位置一致(恰好一处)。位置 = `<文件>::<函数>`:测试函数(Rust 带测试属性或在 `proptest!` 内;pytest 测试文件中的 `test_` 函数)取其自身;DST 场景取 `Scenario::new("<编号>", …)` 调用所在的函数;基准取等于 `基准名` 的 Criterion id 字面量所在的函数(`benches/` 或 `benchmarks/` 下);fuzz 用例取 fuzz 目标文件中以编号开头的函数(`fuzz_target!` 调用它);`实现` 为 `待实现` 时即使找到也列为未实现;
  3. 代码中名字匹配 `^(test_)?[a-z]+_[0-9]{3}_` 的测试函数,其编号必须存在于测试规格;
  4. 带 `#[ignore]`(pytest:`@pytest.mark.skip…`)的测试视为未实现,除非用例有 `状态:隔离:…`;
  4a. `状态:可选扩展:<名>` 的用例只在 `tools/xtask/test-trace.toml` 的 `enabled_extensions` 列出该扩展时才要求实现;启用扩展的同一提交把它加进该列表;
  5. 输出未实现、多余与不一致三张清单及逐里程碑的实现计数,只报告、不使 CI 失败(负责人决定 2026-10-14);只有重复编号、无法解析的里程碑或既非 `待实现` 也非 `<路径>::<函数>` 的 `实现` 字段使其失败。
- `host`/`testnet`/`drill` 用例不进 CI;其执行记录按 `证据` 字段归档,上线门槛([testing §8](../testing.md#8-上线门槛))按编号检查证据。

## 7. 修改流程(设计基线 v1 之后)

1. 实现或测试发现规格有缺陷、矛盾或遗漏 → 在同一提交里改 README(规则)、`TESTS.md`(用例)与代码。**只有改变资金、风控或恢复语义的变更**在 [docs/changes.md](../changes.md) 追加一行(日期、改了什么、起因的用例编号或问题、涉及文件);测试、夹具、金样、CI、工具与文档的调整不记,起因写在提交说明里。
2. 只改测试规格不改代码的提交,也要说明起因;删除用例写明原因并保留编号。**规格冻结**(负责人决定,2026-10-14):新增用例编号(含表驱动的新行)须负责人批准;修正已有用例、调整里程碑不需要。里程碑 `M9` 表示上线后([11 §3.3](../../design/11-实施路线图.md))。
3. 涉及跨组件名字或数值的变更,先改归属地,再改引用处;机检(`check_hft.py` 与 `cargo xtask test-trace`)全过才能合并。
4. 资金、风控、恢复与延迟门槛相关的变更由负责人确认:每周一批,批次清单放在 `.agents/notes/proposed/process/`(列出 changes.md 中"负责人确认"列不是"是"的行),确认后该列改为"是";PR 评审中写明需要确认的项。
