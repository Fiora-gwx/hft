---
description: "慢路径参数流水线框架:按策略实例周期调用随策略包发布的 pipeline 插件(strategies/<name>/pipeline/,经 entry point 发现),合并 policy 默认值与按实例分键的人工覆盖项(校准对显式 scope = pair),按该策略的 schema 校验、经 hft-py 调 SDK params 编码成 StrategyParams blob、版本化并经控制环下发(pipeline 主体的 SetParams 为中性,Live 账户同样;Live 实例的结构字段不经 pipeline 改);插件另产出 Model 预热数据并以 InjectWarmup 逐块注入(新 group 与被重置的 group 自动预热、去重)、产出交易宇宙候选供 hft-cfg 生成 refdata;GroupId 由 StrategyId 与实例内计数器构造、永不复用;本包不含任何策略专属代码"
kind: python-package
tier: cold
scope: core
status: spec
---

# pipeline

## 概要

`pipeline` 是策略参数的慢路径生成器。本包只是**框架**:调度、输入快照、schema 校验与编码、版本库、下发、按实例分键的人工覆盖项、预热注入、状态与解释,以及给插件用的稳定接口 `hft_pipeline.api`。**每个策略的慢数据逻辑是该策略包自带的 pipeline 插件**——一个随策略发布的 Python 包,放在 `strategies/<name>/pipeline/`,规格写在那里的 README(目录约定见 [strategies/AGENTS.md](../../strategies/AGENTS.md)),框架经 entry point 发现它([插件的打包与发现](#插件的打包与发现))。本包的代码与文档不出现任何策略族名;新增策略不改本包。

每个策略实例(一个实例绑定一个账户,[one-account-per-strategy-instance](../../.agents/notes/proposed/architecture/2026-10-02-one-account-per-strategy-instance.md))按自己的周期跑一轮:插件产出 group 列表(`GroupSpec`)与它负责的参数字段,框架合并 policy 默认值与**本实例的**人工覆盖项,按该策略导出的 schema 校验,经 `hft_py.params.encode`(包装 hft-strategy-sdk 的 `params` 模块,全仓唯一的编解码实现)编码成 `StrategyParams` blob,经 ctl-gateway 下发 `SetParams`。pipeline 主体发的 `SetParams` 是中性命令,在 Live 账户上同样不需要确认(含新增 group、`open_allowed` 0→1),但始终受 `RiskParams` 约束;改动 Live 实例结构字段的参数不由 pipeline 下发([参数通道](#参数通道))。实例的 Model 需要历史样本时,插件生成预热数据,框架以 `InjectWarmup` 注入([Model 预热](#model-预热))。它还为 `tools/hft-cfg` 产出交易宇宙候选与差异报告;**refdata 只由 hft-cfg 生成**。它不拥有信号、下单、平仓(sequencer 与执行层),不拥有 `RunMode`(交易员命令),不持有任何交易所 key,只用公共接口。秒级的慢计算(分位、资金费统计、跨币分配)不在这里,属于策略包的 `Model`([hft-strategy-sdk](../../crates/hft-strategy-sdk/README.md))。

## 目录

- [使用本组件](#使用本组件)
  - [一轮的步骤](#一轮的步骤)
  - [插件接口](#插件接口)
  - [插件的打包与发现](#插件的打包与发现)
  - [schema 校验与编码](#schema-校验与编码)
  - [GroupId 分配](#groupid-分配)
  - [实例配置](#实例配置)
  - [参数通道](#参数通道)
  - [Model 预热](#model-预热)
  - [输出与版本库](#输出与版本库)
  - [交易宇宙候选](#交易宇宙候选)
- [实现说明](#实现说明)
- [延迟与资源契约](#延迟与资源契约)
- [测试与验收](#测试与验收)
- [依赖](#依赖)
- [延伸阅读](#延伸阅读)
- [已知限制与后续工作](#已知限制与后续工作)

## 使用本组件

```
hft-pipeline run      --config /etc/hft/pipeline.toml                    # hft-pipeline.service,按实例周期运行(含自动预热)
hft-pipeline once     --config … --instance <实例> [--dry-run]           # 立即跑一轮;--dry-run 写出 blob 到 out/,不发命令
hft-pipeline warmup   --config … --instance <实例> [--source rest|collector] [--dry-run]   # 立即为该实例生成并注入预热数据
hft-pipeline explain  --config … --instance <实例> --asset <资产>        # 该资产这一轮每一步的输入、结论、字段推导
hft-pipeline diff     --instance <实例> --from <version> --to <version>  # 两个参数版本的字段级差异(按 schema 渲染)
hft-pipeline adopt    --config … --instance <实例> --version <v>     # Admin:采纳当前生效的外来版本并解除该实例的下发暂停(参数通道第 2 条)
hft-pipeline schema   --strategy <kind>                                  # 打印框架看到的 schema(来源与版本)
hft-pipeline plugins  --config …                                         # 列出发现的插件(entry point、strategy_kind、版本、来源 wheel)
```

ops_bot 的 `/pipeline`、`/params`、`/warmup` 读本包的状态文件并调用本地接口([ops_bot](../ops_bot/README.md))。

### 一轮的步骤

框架对每个实例执行(插件只参与第 2 步):

1. **快照**:插件声明的外部输入(公共 REST、S3 Parquet)与框架统一读取的 journal 输入(经 `hft_py`:该实例账户的每 group 仓位、在途、`open_allowed`、按 inst 的账户级持仓(含不属于任何 group 的持仓)、当前生效的参数 blob 与版本、当前 refdata 的交易宇宙与各 venue 所属交易所)。外部输入的超时、HTTP 418/429/5xx 与解析失败都算读取失败,不当作空结果;框架读 journal 失败同样算必需输入失败,读不到当前生效参数时这一轮不下发。任一**必需**输入失败(插件仍被调用,快照中该输入标记为失败)或插件返回 `PluginOutput.ok = False` → 这一轮 `ok = False`,只下发相对当前生效参数的收紧类变更——`open_allowed` 1→0、移除第 4 步允许移除的 group、按该字段 `FieldDesc.relax` 判定为收紧的字段改动;新增 group、`open_allowed` 0→1、放宽的字段改动与 `relax = None` 字段的任何改动保持当前生效值。
2. **插件计算**:`plugin.compute(snapshot, policy) -> PluginOutput`(纯函数,见[插件接口](#插件接口))。
3. **合并**(人工优先):字段值 = 本实例的人工覆盖项(未到期;`scope = instance` 优先于该实例所在校准对的 `scope = pair` 项)→ 否则插件输出(只限插件声明拥有的字段)→ 否则本实例的 policy 默认值 → 否则 schema 默认值。人工覆盖项盖过插件输出时,状态文件与 `explain` 标出被覆盖的插件值;schema 标为"只由人工设置"的字段与**结构字段**(`FieldDesc.structural`;两种属性以 [hft-strategy-sdk](../../crates/hft-strategy-sdk/README.md) 为准)只取人工覆盖项或 policy,插件输出它们 → 框架拒绝该轮。**目标实例是 [Live 实例](../../docs/subsystems/control-plane.md#类别)时,结构字段保持当前生效值**:合并结果改动了结构字段(来自覆盖项或 policy)→ 这一轮其余字段照常下发,结构字段的改动不下发,状态写明并 P1"结构字段须经 `hft-cfg` + 运维 CLI 修改"(Live 实例的结构字段改动属升级、pipeline 主体不确认,规则归 [control-plane](../../docs/subsystems/control-plane.md#类别))。
4. **预检**:有仓位(任一腿净量 ≠ 0)、在途或残余的 group 不得移除(先置 `open_allowed = false`,等平仓;该 group 无在途、且每条净量 ≠ 0 的腿的 `|净量|` 都小于该腿 inst 在 refdata 中的最小下单量时,状态提示由 Admin `/writeoff` 核销后再移除,语义归 [hft-oms 残余核销](../../crates/hft-oms/README.md#残余核销));移除 group 时,若它某条腿的 inst 在该账户仍有账户级持仓(例如核销移到无组的量),照常移除并在状态与 `/pipeline` 中提示该 inst 与持仓量(P2,只提示;引擎按持仓继续订阅这些 inst,规则归 [hft-sequencer](../../crates/hft-sequencer/README.md));group 的每条腿必须在当前 refdata 交易宇宙内(不在的进入宇宙候选,不在这一轮下发);**同一 group 的两条腿不得在同一交易所**(与 gateway 的 `InvalidArgs{SameExchangeLegs}` 同一规则,在这里先拦下);`GroupId` 由框架的分配器按[GroupId 分配](#groupid-分配)给出。
5. **校验与编码**:见下节;失败 → 不下发、P1、原因写状态文件。
6. **下发**:内容(`GroupSpec[]` 与 body;头里的版本、时刻不算)与当前生效参数不同时分配新版本并 `SetParams`(带 `base_version`,见[输出与版本库](#输出与版本库));被拒 → 保留旧版本、P1(两腿 lot 对齐不满足时 sequencer 以 `InvalidArgs{LotMismatch}` 拒绝,判定归 [hft-sequencer](../../crates/hft-sequencer/README.md),原因原样写进状态)。内容不变不产生新版本、不发命令。
7. **预热检查**:若该实例或其中某些 group 需要预热(见 [Model 预热](#model-预热))且 `warmup.auto = true`,在这一轮下发被 `Applied` 之后执行一次预热注入(整实例或只对这些 group);这一轮不下发时直接执行;下发被拒时只执行整实例与 `ModelGroupReset` 触发的预热(新增 GroupId 未生效,不预热)。
8. **状态**:写 `/var/lib/hft/pipeline/<实例>/status.json`,原子写:`{round_at, ok, step_ms, groups_added, groups_removed, field_changes, open_allowed_changes, overridden_plugin_values, publish: {version, result, detail}, warmup, override_versions, policy: {version, shared, affects_live}, alerts}`(`step_ms` 按本节步骤 1–8 各一项;`field_changes` 每项 `{group?, field, from, to}`;`overridden_plugin_values` 每项 `{group?, field, plugin, applied, source}`)。`publish.result` 取 `Applied`、gateway 或 sequencer 返回的结果码名、`Unreachable`(ctl socket 不可连)、`Unchanged`(内容不变、未发命令)或 `Paused`(该实例的下发处于暂停,未发命令;不再告警);`Applied`、`Unchanged`、`Paused` 之外都按"被拒"处理(`VersionConflict` 记 P1 并暂停该实例的下发,见[参数通道](#参数通道)第 2 条;其余记 P1 `PublishRejected`)。本文各处的 P1/P2 是 `alerts` 中的一项 `{level, code, group?, detail, at}`(`/pipeline` 显示);`code` 取下列之一:`RoundRejected`(P1,插件输出越权或校验/编码失败)、`PublishRejected`(P1,`SetParams` 被拒)、`VersionConflict`(P1)、`StructuralLive`(P1)、`SameExchangeLegs`(P1)、`WriteOffNeeded`(P2)、`RemovedWithPosition`(P2)、`GroupIdExhausted`(P1)、`GroupIdStateLost`(P1)、`SchemaRuntime`(P2)、`ConfigInvalid`(P1,policy 或覆盖项文件加载失败)、`WarmupNotInjected`(P1)、`RoundSlow`(P1)、`Plugin`(P1,插件 `PluginOutput.notes` 的每一条)。`alerts` 中的每一项由 watchtower 的 `PipelineAlert` 规则读取并按 `level` 投递([watchtower](../../apps/watchtower/README.md);判定函数归 [hft-telemetry](../../crates/hft-telemetry/README.md#规则判定函数)),本包不直接发告警。

### 插件接口

插件只能 import `hft_pipeline.api`(下列类型与 `InputSpec`、`Snapshot`、`WarmupSnapshot`、`Policy`、`ExplainStep`)、标准库与自己声明的依赖;不能 import 框架的其他模块。`hft_pipeline.api` 是插件的稳定接口,只追加;改变已有字段是框架的不兼容变更,需同时更新全部插件。

```python
class Plugin(Protocol):
    strategy_kind: int                      # 与 StrategyDesc.kind_id 一致
    api_version: int                        # 插件编译时的 hft_pipeline.api 版本;框架拒绝更高的版本
    owned_fields: frozenset[str]            # 插件负责填写的 schema 字段名(全局或 per-group);不得含结构字段与只由人工设置的字段
    required_inputs: tuple[InputSpec, ...]  # 外部输入声明:来源、端点、周期、是否必需、令牌桶域
    def compute(self, snap: Snapshot, policy: Policy) -> PluginOutput: ...   # 纯函数:无 I/O、无系统时钟(时间取 snap.ts)
    def explain(self, snap: Snapshot, policy: Policy, asset: str) -> list[ExplainStep]: ...
    # 可选:策略的 Model 需要历史样本时实现
    warmup_inputs: tuple[InputSpec, ...]    # 预热所需的历史输入声明(REST K 线/资金费历史、collector Parquet)
    def warmup(self, snap: WarmupSnapshot, policy: Policy) -> WarmupOut: ...  # 纯函数;snap.groups 为 None = 整实例,否则只为这些 GroupId 生成
                                            # snap.params = 该实例当前生效的 StrategyParams(按 schema 解码,含 per-group 值)

@dataclass(frozen=True)
class GroupOut:
    asset: str                              # 资产符号(refdata 名)
    legs: tuple[LegRef, ...]                # (venue, market_kind, inst 名);腿数 ≤ 4,顺序即 leg_ix;各腿所属交易所互不相同
    open_allowed: bool
    fields: Mapping[str, Decimal]           # 本 group 的 per-group 字段(须在 owned_fields 内)
    reason: str                             # 入选/保留/只平不开的原因(写状态与 explain)

@dataclass(frozen=True)
class PluginOutput:
    groups: tuple[GroupOut, ...]
    fields: Mapping[str, Decimal]           # 全局字段(须在 owned_fields 内)
    universe_candidates: tuple[LegRef, ...] # 下一版 refdata 需要的 inst(不在当前宇宙内)
    ok: bool
    notes: tuple[str, ...]

@dataclass(frozen=True)
class WarmupBatch:
    local_kind: int                         # u16;该策略约定的预热数据种类(只追加,名称与含义归策略 README)
    blocks: tuple[bytes, ...]               # 块列表:插件切好,每块 ≤ 4096 B 且自成一体(策略能单独解码);
                                            # 框架与 hftctl 逐块发送,不再切分或拼接,一块对应一个 chunk_no

@dataclass(frozen=True)
class WarmupOut:
    batches: tuple[WarmupBatch, ...]        # 按顺序注入;一个 local_kind 一批
    samples: int                            # 样本数(写状态,供人核对)
    span: tuple[int, int]                   # 样本覆盖的 [t0, t1) UTC ns
    source: str                             # "rest" | "collector"
    ok: bool
    notes: tuple[str, ...]
```

- 插件不读写文件、不发命令、不访问 journal(框架在快照里给它需要的东西);同一快照两次调用结果逐字节相同(`compute` 与 `warmup` 都是)。
- `owned_fields` 不得包含只由人工设置的字段(例如策略的单 group 强制退出参数)与结构字段:插件声明了 → 框架启动时拒绝载入该插件(退出 2);运行中输出它们 → 框架拒绝该轮。字段的这些属性来自策略导出的 schema。
- **预热回看窗口等取自策略参数的量**,插件只从 `WarmupSnapshot.params`(当前生效的参数)读取,不在 policy 里重复:policy 出现与该策略 schema 字段同名的键 → 该 policy 加载失败(P1,状态写明)。参数热更后,下一次预热自然用新值。
- 没有插件的策略实例由框架用"policy 默认值 + 人工覆盖项 + 静态 group 表(policy 中给出)"生成参数,同样走校验与下发;没有 `warmup` 的插件不做预热。
- 插件里可以出现交易所名(选币与取历史数据需要按所取数);交易所名不进入策略的 Rust 代码,venue 选择来自[实例配置](#实例配置)。

### 插件的打包与发现

| 项 | 约定 |
|---|---|
| 位置 | `strategies/<name>/pipeline/`:`pyproject.toml`、`src/hft_plugin_<name_snake>/`、`tests/`、`README.md`(该插件的规格:外部输入、选币与字段规则、预热载荷生成、阈值与默认值);目录约定归 [strategies/AGENTS.md](../../strategies/AGENTS.md),脚手架 `cargo xtask new-strategy <name> --kind-id <n> --with-pipeline`([tools](../../tools/README.md#cargo-xtask-new-strategy)) |
| 包名 | 分发名 `hft-plugin-<name>`,版本 = 策略包版本;依赖 `hft-pipeline`(只用 `hft_pipeline.api`)与自己声明的第三方包(进合并的 `requirements.lock`) |
| 发现 | `pyproject.toml` 声明 entry point `[project.entry-points."hft_pipeline.plugins"] <name> = "hft_plugin_<name_snake>:PLUGIN"`;框架启动时 `importlib.metadata.entry_points(group="hft_pipeline.plugins")` 载入全部插件,按 `strategy_kind` 建表;两个插件声明同一 `strategy_kind`、`api_version` 高于框架、`strategy_kind` 不在 schema `index.json` 中、或插件对象载入失败与缺少 `Plugin` 协议的必需成员 → 启动失败(退出 2,状态写明) |
| 构建与发布 | CI 质量门的 `ops-py` 作业对每个插件包跑 `ruff`、`mypy --strict`、`pytest`;发布工作流把插件 wheel 与框架 wheel 一起放进归档 `python/`,安装进该版本的单一 venv(冷进程经 `/opt/hft/current-cold/venv` 使用)([development](../../docs/development.md#质量门quality-gatesyml)、[deploy 发布归档](../../deploy/README.md#发布归档与-releasectl)) |
| 金样 | 插件的整轮重放与预热载荷金样在 `fixtures/strategies/<name>/`;预热载荷与策略 crate 的解码测试读同一批文件 |
| 文档检查 | `doc-check` 同样扫描 `python/`:本包与 ops_bot、treasury 的 README 和源码不得出现策略族名;`strategies/<name>/pipeline/` 属于策略包,不受此限 |

策略改动(含插件)只改 `strategies/<name>/`,本包 diff 为零;插件需要新的框架能力时,先单独改 `hft_pipeline.api`(只追加)并升 `api_version`。

### schema 校验与编码

- **schema 来源**:发布归档的 `schemas/strategies/`——`hft-core schemas --out` 导出的 `<kind_id>-<name>.schema.json` 与 `index.json`(格式归 [strategies/registry](../../strategies/registry/README.md));启动时 `hft_py.params.load_schemas(dir)` 一次载入,按 `index.json` 的 `kind_id` 找到实例对应的 schema(单个文件用 `hft_py.params.load_schema(path)`)。若 hft-core 当前生效的 `(schema.id, schema.version)` 与归档不同,用 `GetSchema{strategy_kind}` 取运行时 schema,以运行时为准并 P2 告警。
- **校验**(全部按 `FieldDesc`):字段名存在;`Unit` 换算(`Rate` = 1e-9、`Notional` = USDT 1e-6、`Qty`/`Px` 按资产指数、`DurationMs`、`Count`、`Bool`、`Enum` 名称)只用十进制,不经 `float`;`min ≤ v ≤ max`;非 `per_group` 字段不得按 group 覆盖。
- **编码**:`hft_py.params.encode(schema, header, groups, defaults, overrides) -> bytes`,产出 `StrategyParamsHdr` + `[GroupSpec; n]` + body(布局的唯一归属是 [hft-event](../../crates/hft-event/README.md),body 的 schema 序编码归 [hft-strategy-sdk](../../crates/hft-strategy-sdk/README.md) 的 `params`);编码后用 `hft_py.params.decode(schema, blob)` 回读并逐字段比对。**本包没有任何 Python 实现的编码器或解码器**:编解码只有 SDK `params` 这一份 Rust 代码,`hft-cfg` 与 `hft_py` 都链接它([hft-py](../../crates/hft-py/README.md))。
- `StrategyParamsHdr.source` 写 `Pipeline`;`generated_at_ns` 取快照时间;`account` 取实例绑定的账户。

### GroupId 分配

`GroupId` 的语义与构造(`(StrategyId << 24) | n`、取值范围、`GROUP_NONE`/`GROUP_ALL`、全局唯一、永不复用)归 [hft-types](../../crates/hft-types/README.md#id-与枚举);新 group 的计数器续号与 `StrategyParamsHdr.group_ctr_hwm` 水位规则全仓只有一条,归 [tools/hft-cfg](../../tools/README.md#hft-cfg),本包与 `hft-cfg` 共用:本包编码的每个 blob 按它写 `group_ctr_hwm`。分配器在本包,每个策略实例一个,`StrategyId` 取实例配置的 `strategy_id`;计数器用尽 → 本实例拒绝新增 group(P1)。

- **按指纹记账**:新出现的 group(按 SDK 的 `GroupSpec::fingerprint()`,即资产 + 各腿)取 `next_ctr` 并加一;`next_ctr` 始终大于当前生效 blob 的 `group_ctr_hwm` 与其中本实例的最大计数器(续号规则;水位使已移除的 id 不被再次分配,即 hft-types 的"永不复用"),也大于 `groups.json` 记得的历史最大值;同一指纹在实例中连续存在时沿用原 id;group 被移除后,即使以后再加入同一指纹,也分配新 id。一个 id 从不指向第二个指纹,所以快速部分、Model 与路上的 Advice 不会把旧 group 的状态套到新币上(sequencer 另按 `spec_seq` 丢弃指纹变化前的 Advice,规则归 [hft-sequencer](../../crates/hft-sequencer/README.md))。
- **状态**:`/var/lib/hft/pipeline/versions/<实例>/groups.json`(`next_ctr` 与每个已分配 id 的指纹、首个与最后一个参数版本),随版本库原子写并上传 `s3.params_prefix`。
- **与人工 blob 对齐**:运维经 `hft-cfg` 发出的 blob 也会新增 GroupId([参数通道](#参数通道)第 2 条)。框架每轮从当前生效的 `StrategyParams` 读出本实例的全部 GroupId:不在 `groups.json` 里的 id 连同其指纹登记进去(来源记 `external`),其计数器部分 ≥ `next_ctr` 时 `next_ctr` = 它 + 1;因此人工新增的 id 以后不会被分给别的指纹。
- **丢失时重建**:`groups.json` 丢失时,`next_ctr` = max(journal 中当前生效 `StrategyParams` 的 `group_ctr_hwm`, 其中本实例的最大计数器, 本地或 S3 版本库中出现过的最大计数器) + 1,指纹表从生效 blob 与版本库重建;本实例从未有生效参数时从 1 起。读不到当前生效的 `StrategyParams`(journal 读取失败)时,本实例只保留现有 group、拒绝新增(P1 `GroupIdStateLost`),直到能读到为止。

### 实例配置

`/etc/hft/pipeline.toml`(部署项,`schema = 4`):

| 键 | 类型 | 含义 | 默认 |
|---|---|---|---|
| `host` | string | 本机主机清单的 `host`(S3 对象键的主机段,[deploy 身份、路径与 S3 前缀](../../deploy/README.md#身份路径与-s3-前缀)) | 必填(deploy 生成) |
| `hft.instances` | [`{name: "prod"\|"sim", journal_cold, journal_ram, ctl_socket, s3_prefix}`] | 本机的 hft-core 实例:dev2 上是常驻的 `sim` 与联调窗口里的测试网 `prod` 角色实例,升级主机上 `prod` 与 `sim` 同机([deploy 模拟实例](../../deploy/README.md#模拟实例sim));`s3_prefix` 与该实例 journal-archiver source 的前缀相同(测试网账户实例为 `hft-testnet`) | 必填;`s3_prefix` 缺省 `hft` |
| `instances` | [`{name, core: "prod"\|"sim", strategy_kind, strategy_id, account, plugin?, cadence_min, policy?, venues: {<角色>: <venue>}}`] | 每个策略实例:所在 hft-core 实例、绑定的账户、插件(entry point 名;缺省按 `strategy_kind` 查)、周期、policy 文件(缺省 `policy/<name>.toml`)、按角色给出的 venue(角色名由插件解释)。**各角色的 venue 必须属于不同交易所**(按 refdata `VenueMeta` 的交易所判定),否则加载失败 | 必填 |
| `pairs` | [`{name, live: <实例>, sim: <实例>}`] | 校准对:同参数并行的正式实例与模拟实例;两者必须是同一 `strategy_kind`。只有对内的实例可以共享 policy 文件与 `scope = pair` 的覆盖项 | `[]` |
| `paths.state` / `paths.out` / `paths.versions` | path | 状态(`<state>/<实例>/status.json`、`<state>/<实例>/rounds/`)、dry-run 与宇宙候选输出、版本库 | `/var/lib/hft/pipeline` / `/var/lib/hft/pipeline/out` / `/var/lib/hft/pipeline/versions` |
| `paths.overrides` | path | 覆盖项目录:`<实例>.toml` 与 `pair-<校准对>.toml` | `/var/lib/hft/pipeline/overrides` |
| `s3.params_prefix` | string | 版本库上传前缀模板;`{prefix}` = 策略实例所在 hft-core 实例的 `s3_prefix`,`{host}` = `host`,`{core}` = 该 hft-core 实例名 | `s3://gwx-trading-data/{prefix}/{host}/{core}/private/params/` |
| `http.buckets` | 表 | 每个外部域名的令牌桶 | 必填 |
| `schemas.dir` | path | schema JSON 目录 | `/opt/hft/current-cold/schemas/strategies` |
| `warmup.auto` | bool | 发现实例需要预热时自动注入 | `true` |
| `warmup.sources` | [string] | 预热数据源的尝试顺序 | `["collector", "rest"]` |
| `warmup.collector_prefix` | string | collector Parquet 的只读 S3 前缀 | 部署时填 |

两个实例共享同一 policy 文件只允许在同一校准对内(加载时检查);本表的任何加载失败(缺必填键、角色 venue 同一交易所、校准对种类不同、policy 共享越出校准对、`strategy_id` 不被 [hft-types](../../crates/hft-types/README.md#id-与枚举) 的 `GroupId::compose` 接受)→ 启动退出 2 并写状态;模拟实例上的试验默认只写它自己的覆盖项,不会流到实盘实例。

### 参数通道

- **pipeline 是 `StrategyParams` 的主要写者**:pipeline 主体发的 `SetParams` 是**中性**命令,在 Live 账户上同样不需要确认——包括新增 group、把 `open_allowed` 从 false 改为 true、按插件输出改变字段;唯一例外是 Live 实例的结构字段,pipeline 不下发它们的改动([一轮的步骤](#一轮的步骤)第 3 步);但不能越过 `RiskParams`,每个意图仍过 [hft-risk](../../crates/hft-risk/README.md) 的限额检查。pipeline 不升级 `RunMode`;它可用的命令以[权限表](../../crates/hft-ctl-proto/README.md#权限表)为准(`SetParams`、group 级降级 `SetMode`、`InjectWarmup`、`GetSchema`)。
- **policy**:`policy/<实例>.toml`(或校准对共享的一份),版本化(`policy_version` = 内容 SHA-256),由三部分组成:`[fields]`(schema 字段的 policy 默认值,键必须是 schema 字段名,未知字段名 → 加载失败)、`[[groups]]`(没有插件的实例用的静态 group 表)、其余顶层键(插件阈值,与 schema 字段同名 → 加载失败,见[插件接口](#插件接口))。policy 由 Admin 编辑(不经 ops_bot);共享 policy 的改动同时作用于对内两个实例,这一轮状态与 ops_bot 推送中标出"影响实盘实例"。
- **人工覆盖项按实例分键**:`overrides/<实例>.toml`,每项 `{field, group?, value, scope = "instance", set_by, set_at, expires_at?}`(`expires_at` 为 UTC 时刻,快照时刻 `snap.ts ≥ expires_at` 即到期);校准对共享的覆盖项在 `overrides/pair-<校准对>.toml`,每项必须**显式** `scope = "pair"`(缺省或写错 → 该文件加载失败,不生效)。每个文件有自己的版本号 `override_version`。修改只经两条路:
  1. ops_bot(私聊,Trader):`/params <实例> set …` **只写该实例**的文件;`/params pair <校准对> set …` 写校准对文件。ops_bot 调本地接口 `preview_override(target, field, group, value)`(`target = "instance:<名>" | "pair:<名>"`)→ 返回 `{ok, diff, relax_hint, structural, instances, override_version}`:字段级差异(每项 `{instance, group?, field, from, to}`)、按 schema `relax` 方向的放宽提示(`loosen`、`tighten` 或 `unknown`,`relax = None` 为 `unknown`;只供人读)、是否结构字段、受影响的实例(每项 `{name, live}`,`live` = 是否 Live 实例)、当前 `override_version`;结构字段作用于 [Live 实例](../../docs/subsystems/control-plane.md#类别)(含校准对中的)时预览即拒绝("结构字段须经 hft-cfg + 运维 CLI");用户确认后调 `apply_override(…, expect_override_version)`,版本已变即拒绝。作用于 Live 实例(含校准对)的人工修改在 ops_bot 侧一律需要确认([ops_bot 参数面板](../ops_bot/README.md#参数面板与修改))。写入后立即为受影响的实例各跑一轮(该实例正有一轮在跑时,在它结束后接着跑)并下发,由 ops_bot 发 `Annotate{LocalWrite}`。
  2. 运维用 [`tools/hft-cfg`](../../tools/README.md) 生成完整 blob,经运维 CLI(`cli:<login>`,PREVIEW + `expect_state_version`,带 `base_version`)发 `SetParams`。人工主体的 `SetParams` 是否属升级只按引擎理解的部分判定——任一 group 的 `open_allowed` 由 0 变 1(或新增 `open_allowed = 1` 的 group)、或改动 Live 实例的结构字段;body 字段数值的放宽不参与判定,由 schema 的 `min/max` 与 `RiskParams` 约束(规则归 [control-plane](../../docs/subsystems/control-plane.md#类别))。Live 实例的结构字段只走这条路,预览给出受影响的 group 数与预计不能开仓的时长。之后 pipeline 下一次下发得到 `VersionConflict`:记 P1 `VersionConflict`,并**暂停该实例的下发**——此后每轮照常计算并写状态(`publish.result = "Paused"`,`detail` 为当前生效版本),不发 `SetParams`、不把外来版本的差异写成覆盖项(外来的 GroupId 照常按 [GroupId 分配](#groupid-分配)登记)。恢复由 Admin 决定:先按需要用 `/params` 把要保留的人工改动写成覆盖项,再执行 `hft-pipeline adopt --instance <实例> --version <v>`(本地接口 `adopt_external`):`v` 必须等于 journal 中当前生效的版本(否则拒绝,`VersionChanged`),pipeline 在 `index.jsonl` 登记该版本(`source = external`)、以它为 `base_version` 解除暂停并立即跑一轮。pipeline 重启后,生效版本仍是版本库不认得的外来版本时,第一次下发同样得到 `VersionConflict` 并再次暂停。
- 研究提案([research](../research/README.md))以带到期时间、`scope = "instance"` 的覆盖项进入,只作用于模拟实例;要推到实盘,交易员对实盘实例(或校准对)另发一次修改并确认。

### Model 预热

策略 `Model` 的分位、资金费统计等需要历史样本;Model 检查点缺失时(实例第一次启动,或检查点因策略状态布局版本变化被丢弃)它从空起步。预热把历史样本作为 Input 注入 journal,回放仍确定([warmup-injection](../../.agents/notes/proposed/architecture/2026-10-03-warmup-injection.md));命令语义归 [control-plane](../../docs/subsystems/control-plane.md),Model 侧的处理归 [hft-strategy-sdk](../../crates/hft-strategy-sdk/README.md)(`EventRef::Warmup`),载荷格式归各策略 README,生成规则归各插件规格。

1. **何时**:手动(`hft-pipeline warmup`、ops_bot `/warmup`,本地接口 `run_warmup(instance)`);或 `warmup.auto = true` 时自动,每种情形只做一次:
   - **整实例**:框架在 journal 中看到该实例的 `Note{ModelCheckpointDiscarded}`(hft-core 启动时没有可用检查点也以这条 Note 报告,原因 `Missing`),且该 Note 之后尚未完成整实例预热;
   - **按 group**:journal 中出现该实例的 `Note{ModelGroupReset}`(结构参数热更后 Model 重置了这些 group,group 取自 `Note.group`);或这一轮下发的参数新增了 GroupId(新 group 的 Model 从空起步)。只为这些 group 调插件 `warmup`(`WarmupSnapshot.groups`)。
   - **去重**:按 `(GroupId, 触发它的参数版本)` 只预热一次,已完成的键记在 `status.json`;同一参数版本既新增 GroupId 又产生 `Note{ModelGroupReset}` 时只做一次;hft-core 重启追赶时重放出的 `ModelGroupReset`(其参数版本已预热过)不再触发;整实例预热完成后,同一参数版本下的按 group 触发一并视为完成。
   非结构参数、refdata 与发版不使检查点失效,不触发预热;插件没有 `warmup` 时只记状态。
2. **取数**:按 `warmup.sources` 的顺序取插件声明的 `warmup_inputs`——collector 的 Parquet(只读 S3)或交易所公共 REST(K 线、资金费历史;每域名令牌桶,与参数轮共用);回看的时间跨度由插件按 `WarmupSnapshot.params` 中当前生效的参数算出;一个来源缺数据时换下一个。取到的原始输入存 `rounds/<ts>/warmup/`。
3. **生成**:`plugin.warmup(snap, policy) -> WarmupOut`;`ok = False`(样本是否够用由插件按 `snap.params` 判定)→ 不注入、P1、写状态。
4. **注入**:对 `WarmupOut.batches` 逐批调用 `hftctl.inject_warmup(instance, local_kind, blocks)`(hft-ctl 的纯 Python 客户端,[hft-ctl](../../crates/hft-ctl/README.md)):客户端**不切分、不拼接**,每块原样作为一个 `payload`、`chunk_no` 从 0 连续、该批最后一块 `last = true`(SDK 要求每块自成一体,[hft-strategy-sdk](../../crates/hft-strategy-sdk/README.md));任一块 > 4096 B → 框架在注入前拒绝该批、P1。以 pipeline 主体发 `InjectWarmup{instance, local_kind, chunk_no, last, payload}`;gateway 按 `warmup_rate_per_s` 限速,客户端在 `RateLimited`/`Busy` 时退避重试。全部批次的全部块 `Applied` 才算完成(这时才把触发键记为已完成);中途某块被拒或连接中断 → 该批从 `chunk_no = 0` 重发一次(重发语义归策略 README),仍失败 → `result = Failed`、P1,触发键不记为完成,下一轮按触发条件再试。
5. **记录**:`status.json` 的 `warmup` 段:`{at, source, span, samples, batches: [{local_kind, chunks}], result}`,`result` ∈ `Applied`(全部块 `Applied`)、`NotInjected`(插件 `ok = False` 或有块超长)、`Failed`(注入中途被拒且重发仍失败,附结果码)、`NoWarmup`(插件没有 `warmup`);`/pipeline <实例>` 显示。
6. 预热只发生在 ctl 环上,对 sequencer 的影响按 ctl 批量上限受控;一批数百块是正常的。

### 输出与版本库

- 版本库:`/var/lib/hft/pipeline/versions/<实例>/<version>.bin` + `index.jsonl` + `groups.json`([GroupId 分配](#groupid-分配))(`index.jsonl` 每行 `{version, sha256, policy_version, override_versions: {instance, pair}, snapshot_ts, pipeline_version, plugin_version, result, source}`:内容 SHA-256、本实例与所在校准对的 `override_version`、输入快照时间、下发结果,`source` 为 `pipeline` 或经 `adopt` 采纳的 `external`);同时上传 `s3.params_prefix`。hft-core 写入 journal 的 `ConfigBlob` 才是生效的事实,版本库只是生成侧存档。
- 版本号:每 (strategy, account) 单调递增 u64,由版本库分配:新版本 = max(版本库中最大版本, journal 中当前生效版本) + 1;不递增会被 `StaleVersion` 拒绝。`base_version` = journal 中当前生效的版本,前提是版本库认得它(有同号且同 SHA-256 的 `<version>.bin`,或经 `adopt` 登记的外来版本,见[参数通道](#参数通道));不认得时取版本库中本实例最后一个已生效的版本;版本库为空时取当前生效版本;两者都没有时不带。下发前先写 `<version>.bin`,结果到达后再追加 `index.jsonl` 行,所以别的主体在两轮之间发出的 `SetParams` 使下一次下发得到 `VersionConflict`([参数通道](#参数通道))。
- 每轮的输入快照(JSON + Parquet 片段)存 `/var/lib/hft/pipeline/<实例>/rounds/<ts>/`,保留 7 天,`explain` 与测试从这里重放。

### 交易宇宙候选

pipeline 每个 UTC 日(该日最早的一轮结束后)汇总各插件的 `universe_candidates` 与当前名单,产出 `out/universe-<date>.toml`(`hft-cfg` 的输入格式:inst、资产、venue 与市场类型)与差异报告 `out/universe-<date>.diff.json`(新增/删除的 inst、每个账户受影响的 group 与持仓;有持仓的 inst 被删除时标红,即该项 `has_position = true`)。**pipeline 不生成 refdata blob、不发 `ReplaceRefdata`**:Admin 用 `hft-cfg` 合并候选、生成 blob 与差异报告,在维护窗口经运维 CLI 发 `ReplaceRefdata`(Drain + 重启,[release-and-deploy](../../docs/runbooks/release-and-deploy.md#refdata-变更))。refdata 预装交易宇宙超集,使名单变化只需 `SetParams`。下架等交易状态异常由插件写进 `PluginOutput.notes`(框架把每条 `notes` 记进状态与 `explain`,并以 P1 `Plugin` 告警);正式处理由 Admin 发 `VenueNotice`,pipeline 不发该命令。

## 实现说明

<details><summary>实现要点</summary>

- 单进程 asyncio 调度;外部 HTTP 每域名独立令牌桶(多进程并行请求把 IP 从公共节点封禁的事故,[postmortem](../../docs/postmortem/README.md));pipeline 走主机的管理出口地址,不与 hft-core 的交易连接共用交易所 IP 权重([deploy 出口 IP](../../deploy/README.md#出口-ip));单轮权重 < 该 IP 限额 10 %;预热取数同样受此约束,数据量大时分多轮取完。
- 框架与插件都是"纯函数 + 输入快照":I/O 只在快照、下发与注入三处,便于用 fixtures 重放整轮。
- 下发与注入用 `hftctl.py`(用户 `hft-pipeline`,`issuer = pipeline`);`SetParams` 的 `expires_at_ns` = 签发后 30 s;冲突返回 `VersionConflict` 时按[参数通道](#参数通道)第 2 条暂停该实例的下发。命令发往该策略实例所在的 hft-core 实例(`core`)的 ctl socket。
- 参数编解码只调 `hft_py.params`(`load_schemas`、`load_schema`、`encode`、`decode`);本包不含编码逻辑。
- 本地接口(Unix socket `/run/hft/pipeline/api.sock`,属主 `hft-pipeline:hft-pipeline-api`、`0660`;组成员与路径归 [deploy](../../deploy/README.md#身份路径与-s3-前缀)):`status`、`preview_override`、`apply_override`、`run_now`、`run_warmup`、`adopt_external(instance, version)`;每个响应是 `{ok, error?, …}`,`error` 取 `UnknownTarget`(实例或校准对不存在)、`InvalidValue{field, reason}`(`reason` ∈ `UnknownField`、`OutOfRange`、`NotPerGroup`、`BadText`、`BadEnum`)、`StructuralLive`、`VersionChanged`;`run_now` 返回这一轮的 `status.json`,`run_warmup` 返回其 `warmup` 段;每个请求与结果写一行审计日志(journald:方法、对端 uid、参数摘要、结果)。覆盖项文件的写入是"临时文件 + `os.replace`",每个文件单独加锁;校准对文件的一次修改同时使对内两个实例在这一轮重算。
- 框架代码不按策略名或交易所名分支;按 `strategy_kind` 查插件表。

**规划的源文件**

| 文件 | 职责 |
|---|---|
| `src/hft_pipeline/main.py` | 调度、子命令、本地接口 |
| `src/hft_pipeline/api.py` | 插件的稳定接口(数据类、`InputSpec`、`Snapshot`、`Policy`、`api_version`) |
| `src/hft_pipeline/plugins.py` | entry point 发现、`strategy_kind` 表、版本检查 |
| `src/hft_pipeline/snapshot.py` | 外部与 journal 输入快照 |
| `src/hft_pipeline/schema.py` | 经 `hft_py.params.load_schemas` 载入 schema、单位换算、校验 |
| `src/hft_pipeline/assemble.py` | 合并字段、预检(含两腿不同交易所)、经 `hft_py.params.encode` 编码与 `decode` 回读 |
| `src/hft_pipeline/overrides.py` | 按实例与校准对的覆盖项、`scope` 校验、版本与到期 |
| `src/hft_pipeline/publish.py` | 版本库、S3、`SetParams` 与结果等待 |
| `src/hft_pipeline/warmup.py` | 预热触发判定、取数、调插件 `warmup`、`inject_warmup` 与结果等待 |
| `src/hft_pipeline/universe.py` | 交易宇宙候选与差异报告 |

</details>

## 延迟与资源契约

冷进程,无热路径约束;单元在 `hftbatch.slice`([deploy slice](../../deploy/README.md#slice-与资源切分))。

| 指标 | 目标 |
|---|---|
| 一轮墙钟(框架 + 插件) | < `cadence_min` 的一半,超过 → P1;各插件的目标写在其规格中 |
| 框架开销(不含插件与外部请求) | 每实例每轮 < 2 s(64 个 group) |
| 一次预热 | 取数 + 生成的目标写在插件规格中;注入时间由 `warmup_rate_per_s` 决定 |
| 外部请求 | 每域名 ≤ policy 令牌桶;单轮交易所权重 < 该 IP 限额 10 % |
| CPU / 内存 | 上限在 systemd 单元中设置;`dev2` 的取值归 [hft-core](../../apps/hft-core/README.md#内存与-journal-段按主机类唯一归属) |

## 测试与验收

用例与验收见 [TESTS.md](TESTS.md)。

## 依赖

- 内部:`hft-py`(journal 读取、`params.load_schemas`/`load_schema`/`encode`/`decode`)、`hftctl.py`(`send`、`inject_warmup`)、`python/_generated`(`gen-schema` 生成)、发布归档中的 `schemas/strategies/`;运行时载入随策略包发布的插件 wheel。
- 外部:`httpx`、`polars`、`duckdb`、`pyarrow`、`boto3`;插件的额外依赖在各自的 `pyproject.toml` 声明,进合并的锁文件;版本锁定见 [python/AGENTS.md](../AGENTS.md#版本与依赖锁定)。S3 凭证:dev2 与升级主机上都是凭证代理写的 `/run/hft/creds/hft-pipeline/credentials`(读输入前缀、写本机的 `params/` 前缀),本进程访问不到 IMDS([deploy](../../deploy/README.md#imds-锁定与凭证代理));凭证每 45 min 轮换,boto3 经共享凭证文件重读,DuckDB 的 S3 secret 与 polars 的 `storage_options` 不会自己刷新——每次发起 S3 读写前比较凭证文件的 mtime,变化时重建它们。
- 禁止:任何交易所私有接口与 key;写 hft 文件或共享内存;生成 refdata blob;自写参数编解码;在框架代码中按策略名或交易所名分支;在本包里放任何策略的插件代码。

## 延伸阅读

- [round8-decisions](../../design/_work/round8-decisions.md)、[07](../../design/07-策略层.md)、[09 §5.2](../../design/09-可观测性、风控与控制面.md)
- [warmup-injection](../../.agents/notes/proposed/architecture/2026-10-03-warmup-injection.md)、[no-same-exchange-spot-perp](../../.agents/notes/proposed/architecture/2026-10-03-no-same-exchange-spot-perp.md)、[sim-colocated-with-prod](../../.agents/notes/proposed/architecture/2026-10-03-sim-colocated-with-prod.md)
- [strategies/AGENTS.md](../../strategies/AGENTS.md)(策略包与其 pipeline 插件的目录约定)、[hft-strategy-sdk](../../crates/hft-strategy-sdk/README.md)(schema、`params`、`EventRef::Warmup`)、[hft-event](../../crates/hft-event/README.md)(`StrategyParamsHdr`、`GroupSpec`)、[strategies/registry](../../strategies/registry/README.md)(schema 导出)、[tools/hft-cfg](../../tools/README.md)、[hft-py](../../crates/hft-py/README.md)、[hft-ctl](../../crates/hft-ctl/README.md)、[control-plane](../../docs/subsystems/control-plane.md)

## 已知限制与后续工作

- 各插件的选币阈值默认值在模拟账户上研究确定后写入 policy;在此之前用保守的人工名单(policy 静态 group 表)。
- 预热载荷由 Python 插件按策略定义的格式写、由 Rust 策略读,两侧一致只靠 `fixtures/strategies/<name>/` 的金样;格式变化时提升载荷首字节的版本号(不换 `local_kind`),规则归 [hft-strategy-sdk](../../crates/hft-strategy-sdk/README.md),各策略的具体做法写在其 README。
- `hft_pipeline.api` 只追加;需要不兼容变更时,框架与全部插件在同一发布中升级。

### 开发备忘

<details><summary>给实现者</summary>

- **待核实**:collector 的 BBO Parquet 对新框架交易宇宙的覆盖(哪些 venue 与 symbol 有数据);核实方法:读 collector 的 catalog 与 S3 前缀清单。
- 各插件用到的交易所公共端点、权重与可回溯长度写在插件规格中,每项带"待核实(方法:…)"。
- entry point 发现依赖插件 wheel 已装进同一 venv;CI 的 `ops-py` 作业用 `pip install -e strategies/*/pipeline` 后再跑框架测试,保证发现路径与生产一致。
- 实现顺序:框架(快照、schema、编码、按实例的覆盖项、下发)+ 无插件实例 → 插件接口与发现 → 首个 core 策略的插件 → 预热 → 校准对 → 宇宙候选。

</details>
