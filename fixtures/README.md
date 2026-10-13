---
description: "测试固定输入:解码金样(Binance/OKX/Bitget 现货与 USDT 永续的真实报文 → 期望归一化事件)、模拟撮合金样、参数编码金样、策略 schema 与 Model 检查点布局金样、回放样本、fuzz 回归与 DST 种子的唯一来源"
kind: dir
tier: dev
scope: core
status: spec
---

# fixtures

## 概要

本目录存放测试与基准使用的全部固定输入:**解码金样**(交易所原始 payload + 人工核对过的期望归一化事件)、**模拟撮合金样**(一段行情 + 指令脚本 → 期望模拟回报)、**参数编码金样**(参数 TOML → SDK `params` 编码的 `StrategyParams` 字节,`hft-cfg` 与 `hft-py` 必须逐字节复现)、**策略 schema 与 Model 检查点布局金样**、**回放样本**(小段 seq/raw journal,大文件放 S3、仓库里只放指针与哈希)、**fuzz 崩溃回归**与 **DST 失败种子**。它不含任何密钥、登录帧或未脱敏的账户标识。消费者是各 `venue-<x>` 的金样测试与 fuzz、[hft-sim](../crates/hft-sim/README.md)、[hft-testkit](../crates/hft-testkit/README.md) 的 DST、各策略与 `tools/hft-cfg` 的参数测试、[replayer](../apps/replayer/README.md)、[benchmarks/](../benchmarks/README.md)。本目录的内容不进任何生产构建。

## 目录

- [使用本组件](#使用本组件)
  - [组织](#组织)
  - [金样文件格式](#金样文件格式)
  - [模拟撮合金样](#模拟撮合金样)
  - [参数编码金样](#参数编码金样)
  - [大文件:S3 指针](#大文件s3-指针)
  - [来源](#来源)
  - [与 collector 的离线 BBO 一致性对照](#与-collector-的离线-bbo-一致性对照)
  - [更新流程](#更新流程)
  - [敏感信息清洗](#敏感信息清洗)
- [实现说明](#实现说明)
- [延迟与资源契约](#延迟与资源契约)
- [测试与验收](#测试与验收)
- [依赖](#依赖)
- [延伸阅读](#延伸阅读)
- [已知限制与后续工作](#已知限制与后续工作)

## 使用本组件

### 组织

```
fixtures/
  <venue>/                         venue = VenueProtocol 名:core 为 binance-usdm、binance-spot、okx-swap、okx-spot、bitget-usdt-futures、bitget-spot;
                                   可选扩展 venue(gate-*、bybit-*、kucoin-*、hyperliquid-perp)启用时加目录
    <msg_type>/                    交易所频道/消息类型原名:bookTicker、depth20、depthUpdate、ORDER_TRADE_UPDATE、ACCOUNT_UPDATE、bbo-tbt、books5、books1 …;
                                   REST 响应以 rest.<endpoint> 命名(如 rest.positionRisk、rest.account_config;只读查询族的响应同样放这里,
                                   如 rest.openOrders、rest.userTrades、rest.income);分档快照按档数命名(如 binance-usdm/depth20/)
      <case>.in                    输入:一条或多条 TLS 解密后的 WS payload 或完整的 REST 响应体(与 RawRx 记录体相同的字节;raw-gw 里分块记录的账户查询响应按块拼接)
      <case>.expected.jsonl        期望:每行一个归一化事件(见下)
      <case>.ro.expected.jsonl     同一响应另经只读查询解码(ro_decode)时的期望(如 OKX orders-pending 同时用于 gw 兜底与 reconciler)
      <case>.meta.toml             来源、日期、refdata 版本、核对人、交易所文档版本
    regressions/                   fuzz 崩溃最小化输入(来自 CI 冒烟或服务器上 hft-verify 的崩溃记录,由人带回;期望通常是"错误位 X")
  sim/<case>/                      模拟撮合金样(见下)
  params/<strategy>/               参数编码金样(见下);<strategy> 为策略包名,另有 sdk-tp/ = hft-strategy-sdk 的测试参数 schema `TP`
                                   (除参数金样外另含 schema.golden.json + schema.golden.meta.toml,见下)
  strategy-schemas/                `hft-core schemas --out` 输出的金样副本:每策略 `<kind_id>-<name>.schema.json`(参数字段与 `notes`、`report`、`cmds` 描述符)+ `index.json`(格式归 strategies/registry);
                                   CI 比对,schema 或描述符变化须同 PR 更新(参数字段变化另升 schema 版本)
  strategies/<name>/ckpt/layout.json  <name> 为策略包名,另有 sdk-probe/ = hft-strategy-sdk 的测试 Model `ProbeModel`;Model 检查点的布局描述金样(`ModelLayoutDesc`:节标签、元素类型、长度规则;布局描述变化而 `Model::LAYOUT_VERSION` 未升 → CI 失败,规则归 hft-strategy-sdk);不存原始样本节的字节
  strategies/<name>/ckpt/path.bin    Model 路径状态节金样(只在 `Model::PATH_VERSION > 0` 时提交):固定事件脚本跑完后 `snapshot_path` 写出的字节;
                                   字节变化而 `PATH_VERSION` 未升 → CI 失败(事件脚本与规则归 hft-strategy-sdk)
  strategies/<name>/ckpt/meta.toml   上两份金样生成时的 `layout_version`、`path_version`(CI 据此区分"字节变了但版本未升"与"版本已升但金样未重生成");`HFT_REGEN_GOLDEN=1` 时与金样一起重写
  refdata/<version>.blob           金样所用的 refdata 快照(资产指数、tick/lot、InstId 映射、VenueCaps);<version> 也可是用途名(如 ctl-p0.blob = hft-ctl-proto 金样用的 refdata)
  decimal/<venue>.tsv              hft-types 十进制解析金样:每行原文、线格式指数、parse_exact 期望 raw 或错误位名、parse_round(HalfEven) 期望 raw(格式归 hft-types TESTS)
  ctl/                             控制面金样(归 hft-ctl-proto):<case>.request.json(JSON 线格式请求)、<case>.record.hex(规范化后的 ControlCmd 记录字节)、
                                   cmd_hash、帧与响应金样;文件清单与格式归 hft-ctl-proto 的 TESTS
  replay/<name>.pointer.toml       回放样本指针(S3 URI + SHA-256 + 大小 + schema 版本)
  dst-seeds/<suite>.txt            DST 失败种子回归集(每行:种子、首次失败日期、Agent Note 或 issue 链接)
  dst/hashtrail.toml               DST 一致性金样:固定种子 → 各自的 `HashTrail`(`cargo xtask dst` 输出,类型归 hft-testkit);改变 DST 轨迹的 PR 在同一提交里重新生成;
                                   CI 在 x86_64 上逐字节核对,标签工作流的 aarch64 一致性集与同一提交的这份金样比较(规则归 development 与 testing §6)
  dst/recon-before.toml            已提交对账 actor 的迁移参考;TK-137 用旧 actor 重现摘要,新 actor 核对首条差异与检查器透明性;不覆盖当前 HashTrail 金样
  dst/partial-before.toml          部分成交故障接通前的迁移参考;TK-157 重现空操作摘要并核对首次故障前缀及检查器透明性;不覆盖当前 HashTrail 金样
  dst/stale-before.toml            默认旧代数故障接通前的迁移参考;TK-163 重现旧摘要并核对首次故障前缀及检查器透明性;不覆盖当前 HashTrail 金样
  dst/cancel-before.toml           已成交撤单拒绝故障接通前的迁移参考;TK-173 重现旧摘要并核对实际暂缓及检查器透明性
  dst/late-before.toml             回收槽回报重送前的迁移参考;TK-169 重现旧摘要并核对首次故障前缀及检查器透明性;不覆盖当前 HashTrail 金样
```

规则:`<case>` 用行为命名(`snapshot_then_gap_triggers_resync`、`price_in_scientific_notation`、`missing_E_field`、`base_asset_fee_on_buy`),不按日期编号。

### 金样文件格式

`<case>.expected.jsonl` 每行是一个事件的规范 JSON 渲染,由 `hft-event` 的 `to_canonical_json`(`gen-schema` 生成)定义:

```text
{"kind":"Bbo","venue":"binance-usdm","inst":1042,"ex_ts_ns":1790000000123000000,"ex_send_ns":1790000000124000000,
 "bid_px":"6512300","bid_qty":"1500000","ask_px":"6512400","ask_qty":"820000","update_id":400012345678,"flags":[]}
```

- 定点值写成整数字符串(已按 refdata 的资产指数换算),不写小数;字段顺序固定。
- `hw_rx_ns`、`rx_tsc` 不出现在期望里:测试注入固定值,比对时忽略。
- 输入产生错误时,期望行为 `{"error":"<ErrorBit 名>"}`;可忽略的用户流事件期望为 `{"ignored":"<事件名>"}`(计数,不是错误)。
- 解码结果中不是事件的部分各渲染为一行:编码类金样每条命令一行 `{"frame":"<帧文本>"}`(与 `encode`/`encode_batch` 的输出逐字节相等);`login_result` → `{"login":"Ok"}` / `{"login":"Rejected","code":<码>}`;`ReportOutcome.hint` → `{"hint":"MarginNow"|"ConfigRecheck"}`;结果未知 → `{"outcome_unknown":<ReqRef>,"raw_code":<码>}`;控制提示 → `{"ctl":"ClockSkew"|"RateLimited"|"UserStreamEnded"|"ServerShutdown"}`;只读查询的分页 → `{"page":{"rows":<n>,"next_cursor":<c>}}`。
- 分档快照消息的期望是一条 `L2Snapshot{origin = TopN}`(按 `feed.snapshot_part_levels` 切片后的各片);增量深度(可选扩展)消息的期望是一条 `L2Batch`(一条交易所消息一条记录),REST 快照的期望是若干 `L2SnapshotPart`。
- `<case>.meta.toml`:

| 键 | 类型 | 含义 |
|---|---|---|
| `source` | `"collector-fixture" \| "raw-journal" \| "s3-wal" \| "exchange-doc" \| "synthetic"` | 来源类别 |
| `source_ref` | string | 原始位置(文件路径、S3 key + seq 范围、文档 URL) |
| `captured_at` | UTC 日期 | 报文日期 |
| `refdata` | string | `refdata/<version>.blob` 版本 |
| `exchange_doc` | string | 核对所依据的交易所文档 URL 与版本/日期 |
| `reviewed_by` | string | 核对人(人,不是代理) |
| `scrubbed` | bool | 是否经过清洗;为 `false` 时 `fixtures check` 要求内容不含任何账户类字段 |

### 模拟撮合金样

`sim/<case>/` 给 [hft-sim](../crates/hft-sim/README.md) 的 `SimExchange` 与 hft-testkit 的 `run_sim_case` 使用:

| 文件 | 内容 |
|---|---|
| `records.jsonl` | 按 journal 顺序的行情记录(`Bbo`、`L2Batch`、`Trade`、`Funding`、`Mark`,规范 JSON,带记录头 `tsc`) |
| `commands.jsonl` | 指令脚本:`{decided_tsc, account, GwCmd}` 与模拟命令(`SimTransfer`、`SimReset`) |
| `setup.toml` | `SimTuning`、延迟与保证金模型表(对应 hft-core 配置的 `[sim.latency_models]`、`[sim.margin_models]`)、每账户 `SimAccountCfg { initial_balances, latency_model, fee_tier, margin_model }`(两个 model 是表的下标)、refdata 版本、`VenueCaps` 覆盖 |
| `expected.jsonl` | 期望回报序列(规范 JSON,含 `due_tsc`) |
| `meta.toml` | 想覆盖的行为、核对人 |

hft-sim 的用例目录名带用例编号(`sim-125-<case>/`、`sim-123-mixed/`),refdata 固定为 hft-sim 测试夹具 `rd_sim()`,`setup.toml` 只写与夹具不同的键(扣费规则、资金费推送形态、延迟模型、`mmr` 来源、seed 的仓位、结束时刻);`sim-123-mixed/` 另有 `expected-replies.bin` 与 `expected-state-hash.txt`。

覆盖要求:逐档吃单与限价截止、已吃流动性记忆、每条拒单规则、四种扣费规则、三种资金费推送形态、保证金率、强平价(`liq_px`)与强平、单边行情下的强平价移动(testing MS-2 用的路径片段)、划转、`SimReset`。期望由人按 hft-sim 规格手算,不由当前代码输出生成。

### 参数编码金样

`params/<strategy>/` 冻结参数的规范编码。编码只有一份实现(SDK 的 `params` 模块,[hft-strategy-sdk](../crates/hft-strategy-sdk/README.md));`tools/hft-cfg` 与 `hft-py`(Python 经它调用)都只是包装,不存在独立的 Python 编码器。金样防两件事:编码随版本悄悄改变,以及两个包装的输入处理(TOML 解析、单位换算、group 展开)不一致。

| 文件 | 内容 |
|---|---|
| `<case>.input.toml` | 人可读参数(按该策略导出的 schema;含 group 定义与 per-group 覆盖) |
| `<case>.blob` | 完整 `StrategyParams` 字节(头 + `GroupSpec` + body,规范编码见 [hft-event](../crates/hft-event/README.md)) |
| `<case>.meta.toml` | schema id/version、生成工具与版本、核对人、期望结果(`Ok` 或 `ParamError::<变体>`) |
| `schema.golden.json`(只在 `sdk-tp/`) | SDK `schema_json` 对测试 schema 的规范输出(键序、取值写法、`display` 子对象);由人按 [hft-strategy-sdk](../crates/hft-strategy-sdk/README.md) 的键序手写核对,不由当前代码生成 |
| `schema.golden.meta.toml`(只在 `sdk-tp/`) | 该金样的 `schema_version`、`schema_digest`(十六进制)与核对人 |

- `.blob` 由 `hft-cfg` 首次生成后**冻结**:头与 `GroupSpec` 字段由人对照 hft-event 布局逐字段核对(`reviewed_by`),body 抽查按 schema 序的 i64 小端值;之后不再用当前代码重新生成。
- CI 每个 PR:`hft-cfg` 与 `hft-py` 分别把 `.input.toml` 编码后与 `.blob` 逐字节比较;再用该策略的 `ParamSet::decode` + `validate` 解码 `.blob`,结果与 `meta.toml` 的期望一致。
- 每个策略至少:一个全默认值、一个带 per-group 覆盖、每个字段的越界(min − 1、max + 1)各一个、未知 schema 版本一个。
- schema 版本变化 = 新金样,不改旧金样(旧版本 blob 仍可能出现在回放里)。

### 大文件:S3 指针

超过 1 MiB 的文件不进 git。`replay/<name>.pointer.toml` 有单对象与整套文件两种格式。未填写 `format` 时是单对象,字段如下:

| 键 | 含义 |
|---|---|
| `uri` | `s3://<bucket>/hft-fixtures/<name>/<sha256>.zst` |
| `sha256` | 解压后内容的 SHA-256 |
| `bytes` | 解压后字节数 |
| `kind` | `seq-journal` / `raw-journal` / `wal-v2`;可选扩展 `chain-journal` |
| `schema` | 事件 kind 表哈希与各 kind `schema_version`(journal 样本) |
| `window` | 时间窗与 seq 范围 |
| `purpose` | 使用者(如 `benchmarks/tick-to-trade`、`replay-verify PR 样本`、`replayer sim 复现样本`) |

`format = "files-v1"` 描述一套多段 journal、seal 与 Model 检查点。外层不接受单对象的 `uri/sha256/bytes`,字段如下:

| 键 | 含义 |
|---|---|
| `strategy_set` | 必填,只接受 `registry` 或 `testkit-replay-v1`;执行映射归 [replay-verify](../tools/README.md#cargo-xtask-replay-verify) |
| `journal` | 必填,清单内 journal 目录的相对路径,如 `prod/seq` |
| `[[files]]` | 非空;每行含 `path`、`uri`、`sha256`、`bytes`,后三者遵守单对象规则,哈希针对各自解压后的完整文件 |
| `[[checks]]` | 非空,按声明顺序执行;`name` 为不含点的便携名称,忽略大小写后唯一;`advice` 必填且为 `journal` 或 `recompute` |
| `checks.checkpoint` | 可选,仅用于 `recompute`,必须引用清单内文件或目录,传给 replayer 的 `--model-checkpoint` |
| `checks.from_seq/to_seq` | 可选正整数,分别传给 replayer 的同名窗口参数;同时给出时起点不大于终点 |
| `purpose`、`[source]` | 可选描述字符串与来源元数据表;来源可记录生成提交、CI 制品、检查人,不参与执行参数 |

整套格式的外层、files 行和 checks 行拒绝未知操作字段。相对路径只由 ASCII 字母、数字、点、连字符、下划线和 `/` 组成,每段不以点开头或结尾,不接受空段、Windows 设备名、绝对路径与反斜线。文件/目录重叠、重复文件、任一级大小写别名都失败。journal 目录必须至少包含一对同名的数字段号 `.jrn/.seal`,其中每个成员均须配对。段的连续性与 seal 内容仍由正式读者检查。

整套缓存为 `target/fixtures-cache/<name>/`,只允许清单声明的文件和必要父目录,复用与回放前均核对完整成员集合、大小和哈希。每套先持有独占的 `.bundle-<name小写>.lock`,在独占临时目录内逐文件下载解压,全部通过后以目录重命名发布;其他获取进程遇锁退出 2。旧缓存损坏时,只有完整新集合校验通过后才替换;下载失败保留旧缓存供排障,回放入口仍拒绝它。正常返回会清理锁与临时目录;进程被强杀遗留的锁须确认无获取进程后由运维清理。固定样本的检查矩阵不能用命令行窗口覆盖。

`cargo xtask fixtures fetch` 先按文件名排序并校验全部指针,再通过 AWS CLI 下载、zstd 解压到 `target/fixtures-cache/` 内本次独占的临时目录。单对象的解压后大小与 SHA-256 均匹配才发布为 `<name>`;已存在的缓存也逐字节重新计算哈希,合格时不调用 AWS 或 zstd。不合格的旧缓存删除后重新获取,下载、解压或校验失败退出 2,清理本次临时文件,保留其他已验证对象。空指针集合返回 0,不代表任何样本已验证。

指针文件名只含 ASCII 字母、数字、连字符与下划线,不含 Windows 设备名,同批名称不能互为大小写别名;`uri` 只接受 fixture 桶内 `hft-fixtures/` 下的 `.zst` 对象路径,不接受查询参数或 `..`。`sha256` 为 64 位十六进制,`bytes` 为正整数。缓存目录、对象及指针拒绝符号链接和不符预期的文件类型。解压器使用 128 MiB 内存上限,哈希以 64 KiB 缓冲读取;解压临时文件占用磁盘,大小在解压完成后核对。此命令只校验运输内容,事件 schema、段链与业务一致性仍由 journal 读者和回放验证器检查。

fixture 桶与市场数据桶分开;发布归档不进这两个桶。CI 只对需要拉 fixture 的作业开 GitHub OIDC(`id-token: write`),扮演只读 fixture 桶的角色(该角色的创建是 `[需授权]` 的 IAM 变更;工作流与权限归 [development](../docs/development.md))。工具不创建桶或角色、不上传对象。`aws` 与 `zstd` 由运行环境提供;CI 的质量门与 ARM 发布环境安装 zstd。

### 来源

| 来源 | 用于 | 方法 |
|---|---|---|
| 现有 collector 的 `Trading/tests/fixtures/`(Binance、OKX、Bitget) | 第一批行情金样 | 逐个复制输入,期望由人按交易所文档重新写(旧 collector 的归一化结果只作交叉核对,不作真值) |
| 现有 collector 的 raw WAL(S3 `v2`;collector 保持独立运行) | 三所各消息类型的真实行情样本、tick-to-trade 负载;离线 BBO 一致性对照 | `cargo xtask fixtures extract --wal <s3-key> --channel <c> --range <from..to>` |
| hft 的 raw journal(dev2 测试网/模拟盘联调实例;正式实盘开始后补正式账户) | 订单回报、用户流、账户配置/UID/保证金 REST 响应、自身强平/ADL 回报、新频道与线上异常报文 | `cargo xtask fixtures extract --journal <path> --kind RawRx --venue <v> --seq <a..b>`;账户查询响应在 raw-gw 中按块写成同一 `conn` 上紧随 `RawTx` 的连续几条 `RawRx`([hft-venue §4a](../crates/hft-venue/README.md#4a-账户查询保证金轮询与配置核验)),抽取时按连接拼接成一个 `.in` |
| 交易所文档示例 | 没有真实样本的消息(罕见错误码、强平/ADL 推送、维护通知) | 手工录入,`source = "exchange-doc"` |
| 合成 | 极值、畸形输入、模拟撮合与参数金样 | 手写;`source = "synthetic"`,每个需在 `meta.toml` 写明想覆盖的行为 |

### 与 collector 的离线 BBO 一致性对照

两套解码器并存(collector 与新框架的 `venue-*`)是有意的代价;为发现两者对同一报文的解释分歧,`cargo xtask fixtures bbo-diff --wal <collector 的 s3-key> --journal <hft raw journal 段> --venue <v> --window <from..to>` 对**同一时段、同一合约**:

1. 用 `venue-<x>` 解码 collector WAL 中的 BBO 报文,与 collector 自己的 `book_ticker.v2` Parquet 逐条比较 `(update_id, bid_px, bid_qty, ask_px, ask_qty, ex_ts_ns)`;
2. 把两边各自的 BBO 序列按交易所 `update_id`(无则 `ex_ts_ns`)对齐,与新框架 raw journal 解码出的 `Bbo` 序列比较;两边连接不同、各有丢帧,只比较交集。

验收:交集上价量逐条相等,不等的条目 = 0;交集覆盖率 < 95% 时只报告不判失败。按需离线运行(需要读 S3,不进每夜 CI 的时长预算)。

### 更新流程

交易所改协议、新增订阅、或金样失败时:

1. **判定谁错**:金样失败时先对照交易所文档与原始报文。代码错 → 修代码,金样不动。fixture 错(抽取窗口不对、refdata 版本不对、清洗破坏了字段、期望写错)→ 修 fixture 来源并重新抽取。**不在解码器里加特例去迁就金样**([testing §1](../docs/testing.md#1-原则))。
2. **协议变化**:保留旧金样;新协议新建 case,`meta.toml` 写新文档版本。旧协议彻底下线后,旧金样移入该 venue 的 `retired/` 并在 venue README 的"已知限制"里记一笔。
3. **抽取**:`fixtures extract` → `fixtures scrub` → 人按文档写期望 → `fixtures check`。
4. **核对**:`reviewed_by` 必须是人;AI 代理可以起草期望,但合入前由人按文档逐字段核对。
5. **提交**:同一 PR 里提交输入、期望、meta 与(如果有)解码器改动;PR 描述写清是哪一种。

### 敏感信息清洗

- **永不入库**:API key/secret、私钥、签名原文、listenKey、登录帧与任何含签名的帧、会话 token、Cookie、IP 白名单信息。
- **脱敏替换**(确定性:同一原值在同一 fixture 集里映射到同一占位符):交易所账户 id / UID / 子账户名 → `acct-0001`(UID 查询响应的金样同样替换,`AccountMeta.venue_uid` 测试用占位值);交易所订单 id 与 `clientOrderId` 保留格式与长度、替换内容;钱包地址(可选扩展,链上)→ 占位地址。
- `cargo xtask fixtures scrub` 按 `fixtures/scrub-rules.toml` 执行替换;`cargo xtask fixtures check` 扫描全部 fixture:命中 key 形态(64 位十六进制 secret、`-----BEGIN`、Base64 Ed25519 密钥长度)、登录/签名字段名(`signature`、`apiKey`、`passphrase`、`listenKey`)、`scrub-rules.toml` 里登记的真实账户 id(以哈希登记)→ 失败。
- `fixtures check` 在 CI 每个 PR 运行。
- 机检口径:扫描 `fixtures/` 下全部文件(Markdown 与 `scrub-rules.toml` 除外,二进制按字节扫描);字段名 = JSON 键 `"<名>":` 或查询参数 `?<名>=`/`&<名>=`;64 位十六进制只在所在行不含 `sha256`/`hash`/`digest` 时算命中;Base64 Ed25519 密钥 = 引号内 44 字符(32 字节)的 Base64 串或 PKCS#8 前缀 `MC4CAQAwBQYDK2Vw`;登记的账户 id = 文本中每个 `[A-Za-z0-9_.@:-]{3,}` 片段的 SHA-256 命中 `scrub-rules.toml` 的 `[forbid] sha256 = [...]`。`fixtures scrub` 删去上述登录/签名字段(JSON 成员与查询参数),再按 `[[replace]]`(`pattern` 正则 → `with`)替换,只改写 UTF-8 文本文件;不给路径时处理全部 fixture。

## 实现说明

<details><summary>实现要点</summary>

- 解码金样驱动:每个 `venue-<x>` 的 `tests/golden.rs` 遍历 `fixtures/<venue>/**`,用 `refdata/<version>.blob` 构建 `RefData`,把 `.in` 逐条喂给该 venue 的解码入口(与实盘相同的 `&[u8]` 入口),输出渲染为规范 JSON 与期望逐行比较;失败时打印第一处不同的字段。
- fuzz 种子语料由 `cargo xtask fixtures corpus <venue>` 从 `.in` 生成到 `crates/venue-<x>/fuzz/corpus/`(不入库,每次生成);验证包 `hft-verify-<ver>-aarch64.tar.zst` 的初始语料 `corpus/` 由同一命令生成。累积语料在运行 fuzz 的地方保存(CI 冒烟的缓存、服务器上 `hft-verify` 的状态目录,规格归 [tools](../tools/README.md) 与 [testing](../docs/testing.md#33-fuzz)),同样不入库。
- 规划的文件:

| 文件 | 职责 |
|---|---|
| `fixtures/scrub-rules.toml` | 替换规则与禁止模式;真实标识以 SHA-256 登记 |
| `fixtures/refdata/*.blob` | refdata 快照(由 [tools/hft-cfg](../tools/README.md) 生成,带版本) |
| `tools/xtask/src/fixtures/{fetch,extract,scrub,check,corpus}.rs` | 子命令实现(见 [tools](../tools/README.md)) |

</details>

## 延迟与资源契约

无热路径约束:只被测试、fuzz 与基准读取。仓库内 fixture 总量上限 50 MiB(超出的移到 S3 指针);单文件 ≤ 1 MiB。

## 测试与验收

- `cargo xtask fixtures check`:清洗扫描、每个 case 有 `meta.toml`、`refdata` 引用存在、指针哈希格式正确、参数金样的 schema 版本存在于当前策略集合。
- **上线门槛**([testing §8](../docs/testing.md#8-上线门槛) 门槛 ②),对 core 六个协议(Binance、OKX、Bitget 的现货与 USDT 永续)各自:
  1. 每个订阅的消息类型至少 1 个正常金样 + 1 个异常金样(缺字段、极值、科学计数、空数组);分档快照另有重复序号与档数不足 N 各一个;增量深度与 REST 深度快照的金样在该 venue 启用增量深度(可选扩展)前补齐;
  2. 每种订单回报类型(新单、部分成交、成交、撤单、拒单、过期、改单)与只带费用的回报(期望 `FeeOnly`);
  3. 每个"结果未知"错误码至少一个回报金样(期望 `Unknown`),另有一个**未登记码**的金样(期望 `Unknown` + 告警位,不是 `Reject`,[defensive-patterns](../docs/defensive-patterns.md));
  4. 用户流:每个可忽略事件(`VenueCaps.ignorable_user_events`)一个(期望 `ignored`),一个不在表内的未知事件(期望计数,不是解码失败);
  5. 账户:账户配置查询(持仓模式、保证金模式、多资产、实际杠杆、现货借币开关——OKX `enableSpotBorrow`、Bitget 现货杠杆,开启时期望对应不符位)与 UID 查询的 REST 响应 → `AccountConfigReport`;保证金数据(推送或轮询响应,含交易所 `liq_px`)→ `MarginReport`;
  6. 自身强平/ADL 订单(按 `own_liquidation_marker`)→ `ExternalFill`,与普通外来单(`ForeignOrder`)各一个;成交 id 为 0 或负数的强平/ADL 成交(期望不进成交 id 去重环,按 `(inst, exch_order_id, fill_seq)` 去重)各一个;
  7. 数量与去重:两个 inst 出现相同成交 id 的一对回报(期望去重键 `(inst, trade_id)` 不误杀);OKX 永续 `ctVal ≠ 1` 的合约与 `1000PEPE` 类带倍数前缀的合约各至少一组行情与成交回报(期望数量只经 refdata 的线格式指数换算,含 `ctVal` 平移;venue 运行时不乘除 `ctVal`);
  8. 资金费:费率推送 → `Funding`(含结算周期);结算推送(该所 `funding_settle` 形态,含只带余额不带 symbol 的形态)→ `FundingSettle`(带 `settle_ts_ns`);同一结算推送重复送达两次 → 只产出一次(去重键见 [hft-venue-proto 契约 22](../crates/hft-venue-proto/README.md#实现方必须遵守的契约));
  9. 手续费:按该所 `fee_asset_rule` 的成交回报(含以基础资产扣费)的 `fee`/`fee_asset`;
  10. reconciler 与 kill-switch 使用的只读 REST 响应(挂单、成交、仓位、余额、按 client id 查单、账户信息中的 UID;资金费流水须含逐笔的 symbol、结算时刻与金额);挂单、成交、资金费流水、余额与持仓的期望按 [hft-venue-proto 契约 31](../crates/hft-venue-proto/README.md#实现方必须遵守的契约) 的只读查询族渲染(kill-switch 自写解析,只共用原始响应),随 M5。
- 模拟撮合金样覆盖[上节](#模拟撮合金样)列出的全部行为;参数金样覆盖每个已注册策略。
- 每个可选扩展 venue 启用前:同样的 1–10 条。

## 依赖

- 读取者:`venue-*` 测试、`hft-sim`、`hft-testkit`、`strategies/*` 与 `tools/hft-cfg`、`hft-py` 的参数测试、`hft-telemetry` 的策略报告测试(`strategy-schemas/`)、`replayer`、`benchmarks`。
- 写入者:`tools/xtask` 的 `fixtures` 子命令;参数金样由 `hft-cfg` 首次生成后冻结;策略 schema 金样由 `hft-core schemas --out` 生成;AWS CLI 凭据只在抽取者本机,不入库。

## 延伸阅读

- [整套回放样本清单与测试入口的决定](../.agents/notes/implemented/testing/2026-10-12-replay-fixture-bundles.md)。

- [testing.md](../docs/testing.md)、[event-model](../docs/subsystems/event-model.md)、[journal-format](../docs/subsystems/journal-format.md)、[hft-refdata `VenueCaps`](../crates/hft-refdata/README.md)
- [architecture-v2/10 §5](../design/10-语言分工与代码组织.md)、[research/00 §6.1](../design/research/00-existing-codebase-inventory.md)(旧 collector 的 fixtures 与同步器状态机)
- [research/06 §4.6](../design/research/06-data-plane-replay-observability.md)(bookTicker 与 depth 分流、快照拼接等陷阱——金样要覆盖)

## 已知限制与后续工作

- collector 只有 Binance/OKX/Bitget 的行情样本(没有现货的全部频道、没有订单回报与账户类响应);订单回报、用户流、账户配置/UID/保证金响应与强平/ADL 回报的金样要在 dev2 的测试网/模拟盘联调实例上从 raw journal 抽取。测试网/模拟盘不提供的消息只能用交易所文档示例(`source = "exchange-doc"`,标待核实,方法:正式实盘开始后用真实样本对照)补齐以通过门槛 ②,正式实盘开始后从正式账户的 raw journal 补真实样本(新 case,不改旧金样)。
- 自身强平/ADL 的真实报文很难主动产生:首版以交易所文档示例(`source = "exchange-doc"`)为准,标注待核实(方法:线上第一次出现时补真实样本)。
- 规范 JSON 渲染依赖 `hft-event` 的生成代码;在 `gen-schema` 就绪前,金样期望先以手写结构体比较,就绪后统一迁移。

### 开发备忘

<details><summary>给实现者</summary>

- 旧 WAL v2 的记录格式见 `Trading/schemas/wal-v2.md`;抽取器复用现有 `crates/wal` 的读端(research/00 §6.1:"Keep 格式,Borrow 实现")。
- 清洗是否破坏了解码所需字段:`fixtures check` 另跑一遍"清洗前后解码输出只在被替换字段上不同"的对照。
- **待核实**:各所订单回报与账户类响应里是否还有其他账户可识别字段(如 OKX `uid`、Bitget `userId`)——方法:接入测试账户后抓一次全字段回报,逐字段列表写进对应 venue README 的开发备忘,并登记到 `scrub-rules.toml`。
- **待核实**(方法:接入测试账户后逐频道订阅、逐接口查询一次):各所测试网/模拟盘是否支持全部 core 频道与账户查询(决定哪些金样能从测试网抽取),以及测试网报文与主网报文的差异(符号、数值 id、字段);结果写进各 `venue-*` README 的"测试网/模拟盘"节。测试网抽取的金样 `meta.toml` 的 `refdata` 指向测试网 refdata 快照,不与主网 refdata 混用。

</details>
