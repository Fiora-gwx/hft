# Cookbook:新增事件 kind 或给事件追加字段

逐文件清单。事件的规则(记录头、kind 编号段、只追加、改语义用新 kind、64 KiB 上限)归 [event-model.md](../subsystems/event-model.md#演进规则);Rust 定义归 [`hft-event`](../../crates/hft-event/README.md)。本页只给步骤与每一步的验证方法。清单若与 `hft-event` 的实际结构不符,修这里。

## 0. 先判定是哪一种改动

**新增或修改策略不走本页**:策略的慢速输出是 `Advice` 的不透明载荷(用策略自定的 `local_kind` 区分,载荷格式在策略 crate 里),参数是 `StrategyParams` 的 body(按策略自带的 schema 编码),都不需要新 kind,也不在 `hft-event` 登记任何东西(见 [adding-a-strategy](adding-a-strategy.md))。本页只用于**引擎需要理解的新事实**。

| 你要做的 | 走哪条路 |
|---|---|
| 策略需要新的慢计算结果、新参数 | **不改事件**:在策略 crate 里定义 `Advice` 载荷或 schema 字段([hft-strategy-sdk](../../crates/hft-strategy-sdk/README.md)) |
| 策略需要人工操作入口、冷启动预热数据、新的 Note 码或报表字段 | **不改事件**:`StrategyCmd`/`InjectWarmup` 的载荷由策略定义([control-plane](../subsystems/control-plane.md#命令集));Note 码与报表字段登记在 SDK `StrategyDesc.notes`/`report` |
| 引擎组件需要一条新的 Note 码 | **不改事件**:在 [hft-event Note 码登记表](../../crates/hft-event/README.md#note-来源与码登记表) 加一行并在 `NOTE_NAMES` 加对应项(码名按来源加前缀、source、默认级别、跨 crate 读者需要的 `v[]` 字段),在 source 所在 crate 的 `const` 表追加数值(`n` 从 1 起,0 保留;已用的 `n` 永不复用;`Venue`/`Measure`/`Housekeeping` 源的计数码只进 `const` 表);`gen-schema` 核对两边一一对应;在 [hft-event TESTS.md](../../crates/hft-event/TESTS.md) 的 EVT-075 加一行 |
| 新的事实类型(新行情流、新回报类型、新账户状态、新系统事件)且引擎要据此改变状态 | 新 kind(§1–§8) |
| 已有事件多记一个字段,且旧字段含义不变 | 追加字段(§9) |
| 字段的含义、单位、指数、枚举取值集合变化 | **新 kind**,旧 kind 标 `#[deprecated]` 并保留读取(§1–§8 + §10) |
| 只是冷进程想要一个派生量 | 不改事件:在 `watchtower` / `journal-archiver` / Python 里派生 |

判定标准:该信息**能否影响 sequencer 的决策**。能 → 它必须是 seq journal 里的事件(根 [AGENTS.md](../../AGENTS.md#常驻规则)"决策可见 ⟺ 已记录")。不能且只是观测 → 优先派生,不加事件。

## 1. 分配 kind 编号

1. 按域选段:`0x01xx` 行情、`0x02xx` 执行(实盘与模拟账户共用,**不为模拟另设 kind**)、`0x03xx` 链上(optional)、`0x04xx` 控制、`0x05xx` 系统、`0x06xx` raw journal 专用、`0x7Fxx` 测试与实验([event-model](../subsystems/event-model.md#kind-编号段))。
2. 取该段当前最大编号 + 1。**编号永不复用**:被废弃的编号仍占位。
3. 在 `hft-event` 的 `EventKind` 里加变体与常量,`schema_version = 1`;选布局:定长、`Var`(单条、定长前缀 + 有上限的尾部,无 `PartHdr`)或 `Multi`(`PartHdr` 多段)。
4. 在 [event-model 编号表](../subsystems/event-model.md#kind-编号段)加一行:编号、布局、分类(Input/Output/Meta)。

**验证**:在 [hft-event TESTS.md](../../crates/hft-event/TESTS.md) 的 EVT-004 加一行后 `cargo test -p hft-event --test event kinds::`(EVT-004 逐 kind、EVT-005 一一对应与编号段);`cargo xtask gen-schema` 把新编号追加进 `schemas/event-layout.lock`(只追加基线,删除或改名已有编号即退出 1,见 EVT-176)。

## 2. 定义事件体

1. 在 `crates/hft-event/src/<domain>.rs` 定义体:`#[repr(C)]`,derive `zerocopy::{IntoBytes, KnownLayout, Immutable}`,读取方向全整数字段用 `FromBytes`、**含枚举字段用 `TryFromBytes`**;8 B 对齐,显式 `_pad` 字段填满每个空洞(不留隐式填充,规则 [M4、M6](../hot-path-rules.md#内存m))。
2. 字段只用固定宽度整数、`hft-types` 的新类型(`Px`、`Qty`、`InstId`、`VenueId`、`AccountId`…)与 `#[repr(u8)]` 枚举;无 `String`/`Vec`/`Box`/指针/浮点(规则 [M6](../hot-path-rules.md#内存m)、[N3](../hot-path-rules.md#数值n))。
3. 行情体以 `MarketHdr` 开头;带时间戳的体按 [latency-budget §1](../latency-budget.md#1-时间点) 的名称与单位(`*_ns` 为 UTC 纳秒,`*_tsc` 为 TSC)。TSC 只在一次进程运行内可比;需要跨重启比较的时刻不要放进事件体自算,而是由读者按记录头 `tsc` 换算引擎时间 `engine_ns`([event-model 时间戳语义](../subsystems/event-model.md#时间戳语义));不要为此新增"墙钟"字段。
4. 类型旁写编译期断言(必须,不是可选):`size_of`、`align_of`,以及每个字段的 `offset_of`;`hot-check` 对缺断言的 `#[repr(C)]` 结构报错。
5. 体 > 64 KiB 或长度可变 → 用变长/多段编码([journal-format](../subsystems/journal-format.md#变长与多段记录)),非末段置 `PART`。
6. 文档注释写契约:每个字段的单位、指数来源、取值范围、谁写入。

**验证**:`cargo build -p hft-event`(zerocopy derive 与编译期断言在此失败,EVT-170);在 hft-event TESTS.md 对应域的布局用例(EVT-040、EVT-050、EVT-060、EVT-070 或 EVT-160)加一行并 `cargo test -p hft-event --test event layout::`;已有行不变由 `gen-schema` 的只追加核对保证(EVT-175)。

## 3. 定写者与所在 journal

1. 写明唯一写者线程:feed/gw/sim-gw(raw journal 与入环事件)、sequencer(seq journal)、optional 的 chain-core(chain journal)。advisor 只产出 `Advice`,不产出新 kind。`seq` 只来自 `JournalWriter::append` 的返回值,写者不自行计数。
2. **声明记录类别**:Input(喂给状态机)、Output(状态机产生、验证回放逐字节比对)或 Meta(测量与环境描述,占 `seq`,不进状态、不参与比对)——分类表在 [event-model](../subsystems/event-model.md),规则见 [hot-path-rules D5](../hot-path-rules.md#确定性d)。若该事件记录的是"依赖运行环境的决定"(何时快照、滚动、采样),它必须是 live driver 写下的 Input,不能是回放时重算的东西。
3. 如果事件经输入环进入 sequencer,确定它走哪条环以及该环的优先级与每轮上限(环表与取环顺序唯一归属 [hft-sequencer](../../crates/hft-sequencer/README.md),地图见 [architecture.md §3](../architecture.md#3-hft-core-线程));`Var` 体经配套的 bulk 字节环。
4. 需要的 `RecordFlags`(如 `CONFLATED`、`SHADOW`、`HW_TS`/`SW_TS`)写进体的文档注释。
5. 事件包含凭据、签名原文或登录帧内容?→ 不允许。改设计。

**验证**:在写者 crate 的 README "使用本组件"节列出该事件(消费/产生表);`cargo xtask doc-check` 检查每个 kind 至少被一个 README 声明为产生者。

## 4. 生成 schema

1. 运行 `cargo xtask gen-schema`,更新 Arrow schema、Python 只读 dtype 与 `docs/generated/event-catalog.md`。Python 只**读** journal;写 blob 的编码器只在 Rust(参数与配置经 [`hft-cfg`](../../tools/README.md#hft-cfg))。
2. 生成物与源码改动同一提交。

**验证**:`cargo xtask gen-schema --check` 输出无差异(CI 在 `ci` 内运行它,EVT-174);`schemas/event-layout.lock` 已记录新 kind 的布局。

## 5. 接入生产者与 sequencer

1. 生产者代码写入该事件(遵守 [hot-path-rules](../hot-path-rules.md));解码类生产者的输入来自 `venue-<x>` 纯函数。
2. sequencer 的分派 `match` 加上新 kind;不处理的 kind 也要显式列出(不用 `_ =>` 吞掉,否则下一次新增时编译器不会提醒)。
3. 影响状态的事件:确认它进入 `StateSnapshot`(若它改变了需要快照的状态)。

**验证**:`cargo test -p hft-sequencer`;若事件影响决策,新增至少一个 DST 场景与一条回放样本,`cargo xtask dst`(PR 冒烟的种子数与回归集按 [testing §2](../testing.md#2-金字塔))与 `cargo xtask replay-verify fixtures/replay/<sample>` 通过。

## 6. 下游:归档、Python、观测

1. `journal-archiver`:为该 kind 加 Parquet 映射(数据集名、分区键 `(exchange, dataset, symbol, hour)`、时间戳列名 `ex_ts_ns`/`ex_send_ns`/`hw_rx_ns`/`sw_rx_ns`/`rx_ns`/`proc_ns`)。不需要进研究数据集的 kind 显式标记为"不归档"。
2. `hft-py`:`open_journal(...).events(kind=...)` 通过生成的 dtype 自动可读;确认 Python 侧测试读到了新 kind。
3. `watchtower`:若该事件带延迟时间点,确认直方图分组键按 [latency-budget §5](../latency-budget.md#5-分组与不同数据集不混算)。

**验证**:`cargo test -p journal-archiver parquet_mapping_covers_all_kinds`(每个 kind 要么有映射,要么在排除表里);`pytest python/…/test_journal_kinds.py`。

## 7. 金样与回放样本

1. 由交易所报文解码得到的事件:在 `fixtures/<venue>/<msg_type>/` 加金样(输入 payload + 期望事件),流程见 [fixtures](../../fixtures/README.md#更新流程)。
2. 新增一段包含该 kind 的小型 seq journal 样本到 `fixtures/replay/`(S3 指针 + 哈希),供旧读者兼容测试使用(§10)。

**验证**:`cargo test -p venue-<x> --test golden`;`cargo xtask fixtures check`。

## 8. 文档

1. event-model 编号表的一行在 §1 已加;若是关键事件(被多个 crate 消费且布局需要人读),在 event-model 的"关键事件体"写布局,否则布局只写在 [hft-event](../../crates/hft-event/README.md#事件体布局)。
2. 在写者与主要消费者的 README 更新事件表。
3. 事件表达了新的决策语义(例如新的决策记录)→ 写 Agent Note。
4. 在 [hft-event TESTS.md](../../crates/hft-event/TESTS.md) 为新 kind 补用例:EVT-004 一行、对应域布局用例一行、新码表进 EVT-074、EVT-173 金样一份(流程按 [docs/tests §7](../tests/README.md#7-修改流程设计基线-v1-之后))。

**验证**:`cargo xtask doc-check`。

## 9. 给已有事件追加字段

1. 先看体内有没有够用的保留字节(`_pad*`/`_rsv*`):有就占用它们,体的大小与其他字段偏移不变,不升 `schema_version`(条件见 [event-model 演进规则](../subsystems/event-model.md#演进规则):全零必须是该字段的缺省含义)。没有才把字段加在**体尾**;不改已有字段的位置、类型、含义。新字段**不得是枚举**,除非 0 是该枚举合法的"未提供"值——旧记录读出后必须仍能通过 `TryFromBytes` 校验。改公共前缀 `MarketHdr` 另有限制,同见演进规则。
2. 加在体尾时该 kind 的 `schema_version` + 1;段头 schema 表会记录版本。
3. 读者规则:记录 `len` 短于当前体 → 缺失字段补零;长于当前体 → 忽略多出部分。补零后语义必须仍然正确(新字段的"0"要么是合法默认,要么有一个标志位表示"缺失")。
4. 重做 §2 的布局快照(追加时该 kind 大小变化;两种做法都不改已有字段的偏移)、§4、§6。

**验证**:在 hft-event TESTS.md 的 EVT-071 加一行(旧长度记录读出新字段为 0)后 `cargo test -p hft-event --test event version::`;长体截断由 EVT-019 覆盖;`cargo xtask gen-schema` 的只追加核对通过(EVT-175);`replay-verify` 旧样本 journal 仍逐字节一致。

## 10. 废弃旧 kind

1. 旧常量加 `#[deprecated(note = "由 <NewKind> 取代,见 <Agent Note>")]`;读者保留解析能力,生产者停止写入。
2. 回放旧 journal 时,sequencer 对旧 kind 的处理保持原行为(验证回放要求逐字节一致)。
3. `schemas/event-layout.lock` 中该行由 `gen-schema` 标 `deprecated`(EVT-175.14),不删除。

**验证**:`cargo xtask replay-verify` 对包含旧 kind 的样本 journal 仍通过;`cargo test -p hft-event --test event kinds::`;`cargo xtask gen-schema`(编号未消失,EVT-176)。

## 11. 不需要本页的情形(复核)

新增策略、给策略加参数、给策略的模型加输出、给策略加人工命令载荷或预热数据格式、在模拟账户上运行策略——都不改 `hft-event`、不加 kind、不改 `schemas/event-layout.lock`。上线门槛 ⑧ 的脚手架检查(`cargo xtask scaffold-gate`,[tools](../../tools/README.md#cargo-xtask-scaffold-gate) 与 [testing §8](../testing.md#8-上线门槛))在新增第二个测试策略时检查引擎 crate(含 `hft-event`)的 diff 为零。
