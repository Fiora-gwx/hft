# 测试规格编写简报(M0-g;所有测试规格子代理先读)

依据:`round8-decisions.md` 的 **R8-12** 第 3、4 条(用户:"除了设计文档,项目里关于我们设计的架构各种情况的测试要严格写好测试文档,不然你这每次review都有问题,每次修完又有问题无穷无尽了")。格式与规则的**唯一权威**是 `E:\trading\hft\docs\tests\README.md`(先通读它);本简报只做分工与补充要求。修订前副本:`E:\trading\_backup\round8g-pre\`。

## 0. 你要交付什么

1. 为你名下每个组件写 `<组件目录>/TESTS.md`,完全按 `docs/tests/README.md` 的格式(frontmatter、`## 覆盖`、`## 用例`、`## 验收`;编号前缀见其 §3)。
2. 把该组件 README "测试与验收"一节的全部内容迁进 `TESTS.md`(用例进 `## 用例`,整体验收条件进 `## 验收`);README 的这一节只保留标题和一句"用例与验收见 [TESTS.md](TESTS.md)。"(若该节下有 `###` 子标题,一并删去并在报告中列出)。README 其他小节不迁移、不重抄。
3. **补齐规格缺口**:写具体期望值时,如果从 README 推不出唯一结果(规则缺失、含糊或自相矛盾),这是规格缺陷——在**你名下的 README** 里用最小的改动把规则写明(与现有设计一致,不新增功能),并在报告"规格改动"里逐条列出(文件、小节、改了什么、哪个用例暴露的);涉及别人文件的写进"留给别人"。不要把含糊照抄进用例,也不要写"待定"用例。
4. 处理 §4 中分给你的"合并遗留"条目(上一轮修订代理留给别人的跨文件改动)。

## 1. 质量要求(在 docs/tests/README.md 之上)

- **覆盖面**:README 中每个公开类型、函数、入口与命令至少一个用例;每个状态机的每条边(含非法迁移)一个用例;每个错误码、拒绝原因、Note 码至少一个触发用例;每个可配置参数一条"默认值"用例(只在归属组件)和一条边界用例(边界、−1、+1);每张码表、每个线上编码的往返;每个 `#[repr(C)]` 结构的大小、对齐与关键字段偏移断言;每个延迟预算项一条 `bench` 用例(数值不写,链接 benchmarks);每条"禁止/从不/必须"语句至少一个反例用例(构造违反的输入,期望被拒或断言失败)。
- **具体**:每条 `期望` 都能直接写成断言;数字写具体值;事件写出类型与关键字段;"不发生 X"写成"输出中没有 X"并说明在哪个时间窗或哪一步之后。
- **参数**:用例在 `前置` 里显式设定所依赖的参数,不依赖默认值(docs/tests/README §4 规则 2)。
- **失败路径不少于成功路径**;并发、乱序、重复、截断、溢出、崩溃与重启是首要对象。
- **里程碑**:按 `E:\trading\architecture-v2\11-实施路线图.md` §3 填组件所在里程碑;同一组件里依赖后续里程碑能力的用例(例如 M3 才有的 venue 数据)填后者。
- **级别**:能用 `unit`/`table`/`prop` 的不用 `dst`;跨组件的期望写在发起方或 `docs/tests/scenarios.md`,不在两边各写一半;需要真实主机、交易所或人的写 `host`/`testnet`/`drill`。
- **实现字段**:写建议的测试位置与函数名(`crates/<x>/tests/<主题>.rs::<前缀小写>_<三位数>_<描述>`);DST 场景写 `crates/hft-testkit/src/scenarios/<主题>.rs::<…>`;代码尚不存在,这是约定的落点,不是"待实现"。
- **规模**:不设上限,以覆盖 README 每条规则为准;大组件预计 150–400 个用例。可以分多次写入(先 Write 文件头与覆盖表骨架,再分段追加用例)。
- **跨组件场景编号**:沿用 testing.md 已有的稳定编号(`A5`、`B1`–`B6`、`B-n`、`C1`–`C15`、`BK-n`、`LK-n`、`FD-n`、`AX-n`、`MS-n`、`QF-n`、`AL-n`、`FL-n`、`LT-n`);testing.md 中无编号的表行在迁移时按 §3 的规则编号——覆盖表可以写"不适用:由 BK-1 覆盖"这类引用。
- **检查**:完成后运行并修到零问题(你名下的文件):
  - `PYTHONIOENCODING=utf-8 python E:\trading\architecture-v2\_work\check_tests.py --allow-missing -v`(只看你的文件;其他组件缺 TESTS.md 是正常的)
  - `PYTHONIOENCODING=utf-8 python E:\trading\architecture-v2\_work\check_hft.py`(只看你的文件)
  - `PYTHONIOENCODING=utf-8 python E:\trading\architecture-v2\_work\heading_audit.py E:\trading\_backup\round8g-pre`(你名下 README 消失的标题只允许是"测试与验收"下的子标题)
- 写作规则同历次简报:中文、标识符英文;一件事实一个归属地;只改你名下的文件。

## 2. 分工

(见 §5;每个代理只改自己名下的文件。)

## 3. 跨组件场景的新编号规则(T22 执行,其他人可直接引用)

testing.md 中没有编号的三张表按**本简报写定时**的行序编号(编号表见 §6,由主控生成):§3.6 崩溃恢复与冷启动 → `RC-n`;§4.1 模式、账户与人工干预 → `MD-n`;§5 OMS 与执行的必测失败路径 → `FP-n`。不变量沿用 testing §4 的序号,写作 `INV-n`。演练 `DRL-n`(T20 编号)。

## 4. 合并遗留与主控裁决

上一轮(M0-f)11 个修订代理的报告全文在 `E:\trading\architecture-v2\_work\round8f-merge-notes.md`。每份报告的"留给别人"一节按代号(J1…J12)与文件名列出跨文件改动。**你要在其中找出所有指向你名下文件的条目**(按文件名或 crate 名搜索,不只看代号——本轮的分工与上一轮不同),逐条处理并在最终报告里写"已改 / 已一致 / 无法改(原因)"。与下列裁决冲突时以裁决为准:

- **RF1 布局与版本**:实现之前的布局变化(如 `MarketHdr` 48 → 56 B、各行情体偏移后移、节 21 新增 Gate 行与显式目标行、`SlotStamps` 80 B)全部接受,**不升版本**;设计基线 v1 冻结时的布局就是实现的起点,之后才适用只追加规则(机检的对比基线随冻结切换)。各处说"行版本 vN"的地方保持现值,不要改。
- **RF2 恢复期计时**:多出那一腿所在 venue 的条件(`hedge_gate_ok` ∧ 已 `RECONCILED`)不满足期间不计时,**放行时重给完整窗口**(J4 的口径,规则唯一归属 hft-exec"Broken 处置 / 计时");testing、runbook、glossary 只链接。
- **RF3 `Note{ExecGateHeldNaked}`** 覆盖两种场合(账户闸门未放行时滞留裸量;自动有界平仓到期后长期等待),以 `v[]` 中的场合字段区分,共用 `gate_naked_alarm_ms`。
- **RF4 `uid_policy`** 的 TOML 取值按 hft-cfg 惯例小写:`uid_policy = "verified" | "unverifiable"`;Rust 枚举 `UidPolicy::{Verified, Unverifiable}`;签字字段 `uid_policy_signoff`。
- **RF5 Admin 名单**:GitHub 侧的 Admin 校验(`quota_override`、CI 校验的签字字段)以 `.github/hft-admins.txt` 为准,签字字段记录 GitHub 登录名;ctl 的角色仍以 ctl 签发者表为准。两份名单的人员由负责人维护一致(launch 检查清单里核对一次)。
- **RF6 `DuplicateVenueUid`**:只对 `is_prod = true` 的账户是加载错误;`is_prod = false` 的账户在同一交易所不同 `VenueId` 上 UID 相同(Binance 演示环境现货与 USDⓈ-M 共用一个 UID)只写 P1 告警。
- **RF7 `StorageCritical` 推迟条件** = 存在未到达的 `reason ∈ {MarginStress, Hard}` 引擎目标(与 `exec.drain_ready()` 同口径),不是"`flatten_lock` 仍在"(清仓锁要等 `Resume{Kill}` 才解除,按锁推迟会推到 journal 写满)。
- **RF8 OMS 防御路径合成的外来成交**(`StaleGen`/`ForeignFill` 带成交):与 `AUTO_EXTERNAL` 同理,在处理该 Input 时确定地合成,不另写 Input;另写一条带足入账字段的 Output 类 `Note`,冷读者(reconciler 的引擎视图、hft-py)经 hft-ledger 与 `AUTO_RECON` 注记同一消费路径入账。字段归 hft-oms,消费归 hft-ledger,Note 登记 hft-event。
- **RF9 sim 实例非干净重启后的模拟真值**:sim-gw 的模拟交易所状态(模拟账户余额、仓位、挂单)必须能由该实例的快照与 journal 确定地恢复(模拟回报本来就是 Input);hft-sim 写明状态放在哪一节、恢复步骤,并给出用例。
- **RF10 GroupId 分配**:只有一个分配规则——新 group 的实例内计数器从当前生效 blob 中的最大值续号(hft-cfg 与 pipeline 相同);规则归 hft-types(构造)与 tools hft-cfg(续号),pipeline 链接。
- **RF11 "Live 实例"** 的定义以 control-plane 为唯一归属(有期望模式为 Live 的 cell 的实例)。
- **RF12 `rx_empty_gap_ns` 与取戳**:产生规则唯一归属 hft-net §7(J7 已写);event-model、latency-budget 只链接。
- **RF13 `gw.rest_idle_check_ms`** 同时适用于 warm 任务的 REST 池(J7 的写法),不另开键。
- **RF14 解码预算**:hft-venue-proto 的统一门槛表写明分位数(p50/p99);各 venue 的预算必须不宽于门槛。门槛与 latency-budget 的 `ingest` 分配一致;若 venue 的 JSON BBO 解码 p99 0.8 µs 才可行,就把门槛定为 p99 ≤ 0.8 µs 并核对 latency-budget 合计,否则收紧 venue 的值。
- **RF15** "策略研究后定(R8-12)"这句话不算历史叙述(主控已让机检放行)。
- **RF17 IMDS 锁定的两种机制都保留**:dev2 用按 uid 的 nft 丢弃规则(与旧系统同机、简单);升级主机等延迟主机用 cgroup BPF connect 钩子(只在 connect 时执行,热路径无逐包开销)。deploy 写明各自的理由与适用主机类。
- **RF18 冷副本目录权限**:hft-journal 建冷副本子目录用 `0770`(并继承父目录 setgid 组),使 deploy 给 `hft-archive` 的 ACL 不被 mask 收窄;读者在段文件被删除后解除映射。
- **RF16 设计基线 v1 的入口文档**:根 `AGENTS.md`/`README.md` 不再称"规格仓库",写明"设计基线 v1 + 实现(M1 起)";`docs/AGENTS.md` 的层级表、`docs/cookbook/adding-a-crate.md` 与 `apps/AGENTS.md`/`crates/AGENTS.md` 写明每个组件有 `TESTS.md`、变更流程链接 `docs/tests/README.md` §7 与 `docs/changes.md`(篇幅仍在上限内)。

## 5. 工作分配(只改自己名下的文件;路径相对 `E:\trading\hft\`,T23 另含 architecture-v2)

| 代号 | 写 `TESTS.md` 的组件 | 另外名下的文件(只做合并遗留与必要的规格修正) | 补充 |
|---|---|---|---|
| T1 | `crates/hft-types`、`crates/hft-book`、`crates/hft-rt` | — | hft-types 是第一个动工的 crate,用例要最完整(每个运算的溢出与取整方向、十进制解析/格式化往返、每个码表、`GroupId` 构造与 RF10、`cap_cross_px`、`funding_settle_norm_ns`、`ClientOrderId` 编解码与校验位) |
| T2 | `crates/hft-event` | `docs/subsystems/event-model.md`、`docs/cookbook/adding-an-event-kind.md` | 每个事件 kind 的布局断言与编解码往返;Note 登记表每一行;Timer 编码;gen-schema 导出与 `--check` |
| T3 | `crates/hft-refdata`、`crates/hft-journal` | `docs/subsystems/journal-format.md` | `LoadErrCode` 每个码一个触发用例;RF4、RF6;journal 段滚动两级、关键帧 `state_hash = 0`、截断恢复、RF18 |
| T4 | `tools`(`tools/TESTS.md`)、`crates/hft-testkit`、`crates/hft-telemetry` | — | 每个 `cargo xtask` 子命令、`hft-cfg`(RF10 续号、`uid_policy_signoff`)、`hostprobe`、`hft-verify`、`cargo xtask test-trace`(按 docs/tests/README §6 的五条规则写用例);testkit 的 DST 框架本身(种子决定性、失败输出) |
| T5 | `crates/hft-strategy-sdk` | `docs/cookbook/adding-a-strategy.md` | 解耦验收(新增第二个测试策略引擎零改动)写成用例 |
| T6 | `crates/hft-strat-lib`、`strategies/latency-probe`、`strategies/registry` | `strategies/AGENTS.md`、`strategies/{portfolio-carry,cdex,cross-taker,maker-taker}/**`(portfolio-carry 不写 TESTS.md、不改行为,只处理一致性遗留) | — |
| T7 | `crates/hft-exec` | — | RF2、RF3;ops_bot 兜底判定需要的执行层事件(J10 留言) |
| T8 | `crates/hft-oms` | — | RF8(字段) |
| T9 | `crates/hft-risk` | — | RF7;`venue_gate_open` 报告 |
| T10 | `crates/hft-ledger`、`crates/hft-sim` | — | RF8(消费)、RF9 |
| T11 | `crates/hft-sequencer` | — | 处理顺序、Drain、关键帧、持仓驱动订阅、恢复驱动的用例 |
| T12 | `crates/hft-net`、`crates/hft-venue-proto` | — | RF12、RF14(与 T14、T22 的 benchmarks 一致) |
| T13 | `crates/hft-venue` | `docs/cookbook/adding-a-venue.md`、`fixtures/README.md` | RF13 |
| T14 | `crates/venue-binance`、`crates/venue-okx`、`crates/venue-bitget` | — | 金样用例按消息类型逐一列出(`golden`);RF14;Binance 演示环境(RF6) |
| T15 | `crates/hft-ctl-proto`、`crates/hft-ctl`、`apps/kill-switch` | `docs/subsystems/control-plane.md`、`docs/cookbook/adding-a-control-command.md` | 每条控制命令 × 每个角色 × 每个前置条件;RF5、RF11 |
| T16 | `apps/hft-core` | — | 启动顺序每一步、装配校验每个退出码、线程计划加载校验、dev2 护栏(含 `drill` 级演练) |
| T17 | `apps/replayer`、`apps/reconciler` | — | — |
| T18 | `apps/watchtower`、`apps/journal-archiver` | — | archiver `restore` |
| T19 | `deploy`(`deploy/TESTS.md`) | `deploy/AGENTS.md` | 生成器输出、单元文件、权限闭包(每个单元用户对每条路径)、preflight 每一项、两份主机清单(F7)、RF17 |
| T20 | — | `docs/runbooks/*`;新建 `docs/tests/drills.md`(`DRL-n`) | runbook 中的演练与人工步骤写成 `drill`/`testnet`/`host` 用例;组件 README 里定义的演练归该组件的 `TESTS.md`,不在这里重复 |
| T21 | `python/ops_bot`、`python/pipeline`、`python/treasury`、`crates/hft-py` | `python/AGENTS.md`、`python/research/**` | — |
| T22 | — | `docs/testing.md`、新建 `docs/tests/scenarios.md` 与 `docs/tests/invariants.md`、`benchmarks/**` | 见下 |
| T23 | — | `architecture-v2/*`(不含 `_work/`)、根 `AGENTS.md`/`README.md`/`CLAUDE.md`、`docs/{AGENTS,architecture,glossary,defensive-patterns,hot-path-rules,latency-budget,development}.md`、`docs/postmortem/README.md`、`docs/subsystems/{README,external-venue-protocol}.md`、`docs/cookbook/adding-a-crate.md`、`crates/AGENTS.md`、`apps/AGENTS.md`、`.agents/notes/**`、冻结组件 README | 合并遗留;RF16;development 的质量门加 `cargo xtask test-trace`(规则链接 docs/tests §6) |

**T22 的任务**:
1. `docs/tests/scenarios.md`:把 testing §3.6、§4.1–§4.4、§5 的每一行写成结构化用例(三级标题 `### <编号> <行为陈述>`,字段按 docs/tests/README §4;级别多为 `dst`/`integ`;`规则` 链接到各组件 README 的规则小节;`实现` 写 `crates/hft-testkit/src/scenarios/…` 或 `tests/recover/…`)。沿用已有编号,无编号的三张表按 §6 的编号表。页首写编号族登记表。
2. `docs/tests/invariants.md`:testing §4 的不变量逐条写成 `### INV-n <陈述>`(级别 `dst`/`prop`;`断言` 写具体可检查的条件;`实现` 写 testkit 的检查函数,如 `crates/hft-testkit/src/invariants.rs::inv_18_drain_leaves_no_naked_leg`)。
3. testing.md:§3.6、§4、§4.1–§4.4、§5 的**标题保留**(别处有锚点链接),正文改为一句指向 scenarios.md / invariants.md 的链接;§8 上线门槛每一项写明由哪些编号(或哪一类级别的全部用例)构成证据。
4. benchmarks:RF14 与基准名的唯一登记(包括各 `TESTS.md` 中 `bench` 用例引用的基准名)。

## 6. 编号表(testing.md 无编号表行的新编号;按行序)

**RC**(3.6 崩溃恢复与冷启动):

| 编号 | 原表首列 |
|---|---|
| RC-1 | 崩溃点落在一个输入事件的输出写到一半时 |
| RC-2 | 崩溃点落在段滚动/快照写入中途 |
| RC-3 | 尾部含已发出但未回执的订单 |
| RC-4 | 版本变化后冷启动 |
| RC-5 | 版本不变、尾部输出与重算输出不一致 |
| RC-6 | 同一恢复区间连续崩溃 `poison_crash_limit`(2)次 |
| RC-7 | 新版本崩溃后回滚 |
| RC-8 | 对账不通过 |
| RC-9 | 干净 Drain 重启 |
| RC-10 | 长停机后冷启动(停机时长 > `hedge_deadline_ms`),尾部有第一腿已成交、第二腿未完成的链接 |
| RC-11 | 跨重启的计时:策略确认窗与节流、MarginStress 截止、Broken 计时在停机前进行到一半;同开机重启、跨开机 |
| RC-12 | 改参数、换 refdata、发新版本后重启(Model 状态布局不变) |

**MD**(4.1 模式、账户与人工干预):

| 编号 | 原表首列 |
|---|---|
| MD-1 | Shadow 有仓位 |
| MD-2 | 多个 cap 叠加 |
| MD-3 | 日亏 Resume |
| MD-4 | 时钟不可信 / Halted |
| MD-5 | 一个账户限速风暴 |
| MD-6 | 命令账户字段不一致 |
| MD-7 | 二次确认 |
| MD-8 | 覆盖项按实例 |
| MD-9 | `account=*` 升级 |
| MD-10 | Kill 参数 |
| MD-11 | Viewer 紧急停止 |
| MD-12 | 紧急停止兜底 |
| MD-13 | 停机标记:`stop_core` 之后 hft-core 干净重启(尾部为空、无 `Unknown`,本可自动解除 `R |
| MD-14 | Drain 期间 |
| MD-15 | refdata-drain 超时 |
| MD-16 | 冷启动 |
| MD-17 | kill-switch 换实例 |
| MD-18 | 人工干预 |
| MD-19 | 同所两腿 |
| MD-20 | 模拟账户 |
| MD-21 | 冒烟单:Admin(`cli:<login>` 或 ops_bot 私聊中的 Admin)确认后发 `SmokeOrde |
| MD-22 | 模拟优先级 |
| MD-23 | sim 实例 |
| MD-24 | 预热注入 |
| MD-25 | 快照与回放 |

**FP**(5. OMS 与执行的必测失败路径):

| 编号 | 原表首列 |
|---|---|
| FP-1 | 下单后连接断、无回执 |
| FP-2 | 限速错误 |
| FP-3 | `Fill` 先于 `Ack` |
| FP-4 | 撤单被拒(已成交) |
| FP-5 | 第二腿被拒 |
| FP-6 | 第二腿盘口长时间不变 |
| FP-7 | 第二腿对手价超滑点 |
| FP-8 | 本机时钟漂移 > recvWindow |
| FP-9 | 未登记的交易所错误码 |
| FP-10 | 无过期保护的所查单 NotFound;超出 recvWindow 被拒后查单 NotFound |
| FP-11 | 自身强平/ADL |
| FP-12 | 基础币扣费 |
| FP-13 | 可选扩展(链上):403/429/超时、`IncludedReverted`/`Dropped`、迟到上链、回执与日志不 |

## 7. 最终报告(简短)

1. 写了哪些 `TESTS.md`,各多少用例(按级别计数)。2. 规格改动清单(文件、小节、改动、暴露它的用例编号)。3. 留给别人的跨文件问题。4. §4 合并遗留的处理结果(已改 / 已一致 / 无法改及原因)。5. 你认为测试仍覆盖不到、需要负责人决定的地方(≤ 5 条)。
