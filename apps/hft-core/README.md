---
description: "唯一的热进程(实例角色 prod 或 sim,同一二进制):按主机类装配 feed/gw/sequencer/sim-gw/advisor/snap-fetch/acct-fetch 线程、环、journal 与控制入口,注册策略集合,按启动顺序恢复状态(含停机标记)、连接交易所、过启动闸门后按 RunMode 放行;实例角色、主机类、线程计划(含 IRQ 核与 ENI 布局)、内存与 journal 段大小、大页、容量、冷盘容量、主机资源隔离(slice、核、I/O、出口 IP)、advisor 检查点、dev2 护栏的唯一归属"
kind: app
tier: hot
scope: core
status: impl
---

# hft-core

## 概要

`hft-core` 把库 crate 装配成一个进程:按线程计划(`[[threads]]`,按主机类)创建线程并绑核,建环与 journal 热段(大页,或 tmpfs),载入 refdata/参数/秘密,以 [hft-strategy-registry](../../strategies/registry/README.md) 的策略集合实例化 sequencer 与 advisor,恢复状态,连接交易所,产出启动闸门,再由交易员按 `RunMode` 放行。它**拥有**:实例角色、主机类与线程计划(含 IRQ 专核与网卡布局)、各主机类的内存预算、大页、容量、journal 段大小与冷盘容量、同机两个实例的资源隔离(slice、核、冷盘 I/O、出口 IP)、dev2 护栏(含标志文件与 `sim` 的自动重启)、启动时的停机标记、启动/退出顺序与退出码、崩溃隔离与 `--recover-from-snapshot`、advisor 的模型检查点、配置文件格式、`HeartbeatBlock` 的装配与 `HostProfile` 校验。它**不拥有**任何业务逻辑(簿、策略、执行层、风控、OMS、记账、解码、模拟撮合都在 crate 里)。实例角色 `instance.name ∈ {prod, sim}`(同一二进制,见[实例角色](#实例角色prod-与-sim)):`prod` 只含交易所账户(dev2 上是测试网/模拟盘账户,统一升级主机上是正式实盘账户),`sim` 只含模拟账户、不持任何交易所 key;两者在任何主机类上都不混装。统一升级后 `prod` 与 `sim` 同机运行,sim 只用非隔离核、在自己的 slice、网卡(ENI-C)与出口 IP 下([主机资源隔离](#主机资源隔离slice核与出口-ip))。实例内含多个账户:每个账户只绑定一个策略实例,账户类型为 `Live`(交易所子账户)或 `Sim`(模拟账户,订单进 sim-gw);一个账户在同一交易所只用一个市场。core venue 为 Binance、OKX、Bitget 的现货与 USDT 永续;链上外部 venue(`ext-bsc`)为可选扩展。

## 目录

- [使用本组件](#使用本组件)
  - [命令行与退出码](#命令行与退出码)
  - [策略装配](#策略装配)
  - [实例角色(prod 与 sim)](#实例角色prod-与-sim)
  - [线程计划](#线程计划)
  - [内存与 journal 段(按主机类唯一归属)](#内存与-journal-段按主机类唯一归属)
  - [主机资源隔离(slice、核与出口 IP)](#主机资源隔离slice核与出口-ip)
  - [dev2 护栏](#dev2-护栏)
  - [启动顺序](#启动顺序)
  - [崩溃隔离与从快照恢复](#崩溃隔离与从快照恢复)
  - [优雅退出与 kill 语义](#优雅退出与-kill-语义)
  - [refdata 变更(Drain + 重启)](#refdata-变更drain--重启)
  - [多账户](#多账户)
  - [HostProfile](#hostprofile)
  - [HeartbeatBlock](#heartbeatblock)
  - [配置文件字段](#配置文件字段)
- [实现说明](#实现说明)
- [延迟与资源契约](#延迟与资源契约)
- [测试与验收](#测试与验收)
- [依赖](#依赖)
- [延伸阅读](#延伸阅读)
- [已知限制与后续工作](#已知限制与后续工作)

## 使用本组件

`journal.latency_trace_capacity` 可选启用逐事件测量环；每线程独立 producer，已有 persist 冷线程落盘，关闭时排空并记录丢点数。容量约束、文件格式与离线分析归 [latency-report](../../tools/latency-report/README.md)。省略该项时不分配测量环。

### 命令行与退出码

```
hft-core --config /etc/hft/hft-core-<instance>.toml           # 运行(systemd 单元 hft-core@<instance>.service,instance ∈ {prod, sim})
hft-core --config <path> --check                             # 只加载与校验(配置、blob、秘密引用、策略集合、内存预算、HostProfile、出口 IP),不连接、不写 journal
hft-core --config <path> --recover-from-snapshot <latest|SEQ> --reason "<文本>"   # 一次性:从快照起步、丢弃尾部、严格对账(见崩溃隔离与从快照恢复)
hft-core schemas --out <dir>                                 # 导出已注册策略的参数 schema JSON + 带摘要的 index.json(进发布制品,供 hft-cfg 与 Python 使用)
hft-core --version                                           # 版本、git sha、target-cpu、schema 版本表、已注册策略 (kind_id, version)
```

| 退出码 | 含义 | systemd 行为 |
|---|---|---|
| 0 | 优雅退出完成 | 不重启 |
| 75 | `ReplaceRefdata` 触发的 Drain 完成且 Drain 末的复核通过,请求重启(复核不通过时以 0 退出,见 [refdata 变更](#refdata-变更drain--重启)) | `RestartForceExitStatus=75`,立即重启 |
| 2 | 配置/blob/秘密引用/策略集合错误(一次打印全部错误);装配期交叉校验不符(`ProbeOnProdAccount`、`ExecRiskMismatch`、`DustBelowMinNotional`、`ModelMemBudget`、`BboSharesWithMd`、`AccountNamePrefix` 等,见各节);恢复后占用超过容量(`CapacityShrink`);**交易 key 的身份核验不符**(步 6,经 Drain 后退出,见[启动顺序](#启动顺序)) | `RestartPreventExitStatus=2 3 4`,不重启 |
| 3 | 主机前置条件不符(`host.on_mismatch = "refuse"`,含 hostprobe 的收包参数与 IRQ 落点)、内存预算或 tmpfs 空间不足;运行中存储严重不足(`StorageCritical`)的 Drain 完成 | 不重启 |
| 4 | journal 恢复失败(段链哈希断裂、快照不可解、冷热冲突)、版本变化且末尾不是 Drain 快照(`VersionNeedsDrain`)、`--recover-from-snapshot` 找不到可解码快照 | 不重启,人工处理([incident-response](../../docs/runbooks/incident-response.md)) |
| 134(SIGABRT) | `panic = "abort"`、`Fatal(JournalFull \| ClockRange \| TimerOverflow)`,或 housekeeping 发现 sequencer 停摆后自杀 | `Restart=on-failure`,`RestartSec=1`,`StartLimitBurst=3`/`StartLimitIntervalSec=600` |

sequencer 的 `StepOutcome::Done{cause}` 到退出码的映射(cause 的定义归 [hft-sequencer](../../crates/hft-sequencer/README.md)):命令或 SIGTERM 的 `Drain` → 0;`ReplaceRefdata` → 75(Drain 末复核不通过时 sequencer 改以 `Drain` 结束 → 0,见 [refdata 变更](#refdata-变更drain--重启));`StorageCritical` → 3;`IdentityMismatch` → 2。Drain 超过 `shutdown.drain_timeout_ms`(`Note{DrainTimeout}`)时按当前状态封存,退出码仍按触发该 Drain 的 cause 映射。`schemas` 与 `--version` 成功时退出 0。

**错误报告**(退出码 2、3、4):每条错误在 stderr 打印一行 `hft-core: error[<码名>] <详情>`,第一条同时写进 `sd_notify(STATUS=…)`。步 1–2 收集全部错误后一次打印(TOML 语法错误与未知键在解析时即终止,列表只有这一条);同时有退出 2 类与退出 3 类错误时以 2 退出;步 3 起遇到第一条错误即退出。refdata、EngineConfig、参数与风控 blob 的加载与跨 blob 校验失败,码名用 [hft-refdata `LoadErrCode`](../../crates/hft-refdata/README.md#loaderrcode-码表) 的名字(如 `SameExchangeMarkets`、`UnknownStrategyKind`、`UnknownVenue`、`BindingChangedWithPosition`);其余码名:

| 码名 | 退出码 | 条件(规则所在小节) |
|---|---|---|
| `Usage` | 2 | 命令行错误:未知参数、缺 `--config`、`--recover-from-snapshot` 缺 `--reason` 或 `SEQ` 不是十进制 u64、与 `--check` 同用;`schemas --out` 的目录不可写 |
| `ConfigParse` | 2 | TOML 语法或类型错误、未知键(`deny_unknown_fields`) |
| `MissingField` | 2 | 必填字段缺失,每个缺失键一条,详情为键路径([配置文件字段](#配置文件字段)) |
| `BadValue` | 2 | 取值不在允许集合(`instance.name`、`host.class`、`idle` 形式、`venues.<v>.protocol` 等) |
| `FileRead` | 2 | blob、`host.egress` 或 `paths.release_manifest` 读不到 |
| `BlobHash` | 2 | blob 的 SHA-256 与 `{path, sha256}` 不符 |
| `InstanceHostClass` | 2 | 实例角色或 `order_env` 与 `host.class` 不允许组合([实例角色](#实例角色prod-与-sim)) |
| `SimAccountInProdInstance` / `LiveAccountInSimInstance` / `EnvAccountMismatch` / `ProdAccountOnDev2` / `AccountNamePrefix` / `DevBuildWithProdAccount` | 2 | 见[实例角色](#实例角色prod-与-sim) |
| `EnvCombo` | 2 | `feed_env = "testnet"` 而 `order_env = "prod"`;`sim` 实例出现 `order_env` 或 `feed_env = "testnet"` |
| `AccountMismatch` | 2 | `[[accounts]]` 的 `id`/`label` 与 refdata `AccountMeta` 不一致,或引用 refdata 中没有的账户 |
| `SecretsInSim` | 2 | `sim` 实例出现 `paths.secrets_dir` 或任一 `[[accounts]].keys` |
| `MissingCredential` | 2 | Live 账户在其交易的某个 venue 缺会话 key 或 REST key 的秘密名 |
| `EndpointOverrideInProd` | 2 | `order_env = "prod"` 的配置出现 `endpoint_override` 或 `test.extra_ca` |
| `SimSectionInProd` | 2 | `prod` 实例出现 `sim.*` 或 `[[sim.*_models]]` |
| `BindingMismatch` | 2 | 账户的 `bound_instance` 没有对应的 `StrategyParams`,或同一实例被两份参数引用([策略装配](#策略装配)) |
| `ProbeOnProdAccount` | 2 | 见[策略装配](#策略装配) |
| `ModelMemBudget` | 2 | 见[内存与 journal 段](#内存与-journal-段按主机类唯一归属)规则 5 |
| `StandbyFlagMismatch` | 2 | `startup.allow_missing_standby` 与 ECF1 段 5 不等 |
| `EngineMismatch` | 2 | EngineConfig 段 4 的环容量行与本实例按线程计划要建的环不符或不合法([配置文件字段](#配置文件字段)的环容量校验;`sim` 实例拿到 `prod` 的 EngineConfig 由此拒绝);ECF1 `host_class` 与 `host.class` 不符用 `HostClassMismatch` |
| `ExecRiskMismatch` / `DustBelowMinNotional` | 2 | 见装配期交叉校验([内存与 journal 段](#内存与-journal-段按主机类唯一归属)) |
| `ConnLiveness` / `CellCapacity` / `SubCapacity` / `FeedCtlCapacity` | 2 | 装配期交叉校验的另四项,依次为:`conn_liveness_ms < 2 × bbo_ping_ms`;cell 数超过 `max_cells_total`;Σ `max_sub_insts` 超过 `max_insts`;`feedctl` 环容量小于 2 × 某 venue 的 `max_sub_insts` |
| `SimConfig` | 2 | `SimExchange::new` 干校验失败(含 `latency_model`/`margin_model` 下标越出 `[[sim.*_models]]` 表) |
| `GwSharesThread` / `BboSharesWithMd` | 2 | 线程计划加载校验第 4、13 条 |
| `SimOnHotCore` | 2 | `sim` 实例的线程绑在 `host.hot_cpus` 内的核(第 8 条;`sim` 实例出现 `Spin` 为 `ThreadPlan` 第 8 条) |
| `ThreadPlan` | 2 | 线程计划加载校验的其余各条(详情为条号;线程名超长或重复、`nice` 用在绑核线程、未知 task 名等语法错误为第 0 条) |
| `EgressIpOverlap` / `SharedAccountIp` | 2 | 出口 IP 不在本实例列表、列表相交或 `gw:`/`acct-fetch` 用了 `prod_bulk`([主机资源隔离](#主机资源隔离slice核与出口-ip));同一 venue 的两个 Live 账户共用一个 `gw.src_ip`([多账户](#多账户)) |
| `CapacityShrink` | 2 | 步 5 恢复后的占用超过 EngineConfig 容量 |
| `TombstoneKeep` / `TimerCapacity` | 2 | 装配期交叉校验的另两项:OMS `tombstone_keep_ms` 小于某个有 gw 的 venue 的 `venues.<v>.gw.id_tomb_ms`;`max_timers` 不大于按装配结果算出的引擎定时器需求 `engine_timer_demand`(与 `Sequencer::boot` 返回的 `BootError::TimerCapacity` 同一判定,后者同样以此码名退出 2;[hft-sequencer 定时器](../../crates/hft-sequencer/README.md#定时器)、[内存与 journal 段](#内存与-journal-段按主机类唯一归属)) |
| `ExecConfig` | 2 | 装配时 hft-exec 拒绝执行配置(`ExecConfigError` 的任一值,详情为值名;[hft-exec](../../crates/hft-exec/README.md)) |
| `VenueInit` | 2 | `FeedRuntime`、`SnapFetchTask`、`GwRuntime` 或 `AcctFetchTask` 的构造返回 `InitErr`(详情为 venue、任务与 `InitErr` 值;[hft-venue](../../crates/hft-venue/README.md)) |
| `CtlPrincipals` | 2 | `paths.ctl_principals` 读不到或 hft-ctl `PrincipalTable::load` 失败(详情为该错误;[配置文件字段](#配置文件字段)) |
| `IdentityMismatch` | 2 | 步 6 及之后的身份不符,经 Drain 后退出 |
| `HostCheck` | 3 | `check_host` 的 hard 位(详情列位名) |
| `HostProfile` | 3 | hostprofile.json 缺失、与 `host.expect_profile` 不符或 `pending_reboot = true`(含 hostprobe 报告的 `isolcpus`、IRQ 落点、出口 IP 与 EIP),`host.on_mismatch = "refuse"` |
| `MemBudget` / `ShmSpace` / `Hugepages` | 3 | 步 3 的内存预算检查、tmpfs 空间检查、大页映射失败 |
| `StorageCritical` | 3 | 存储严重不足的 Drain 完成 |
| `Journal` | 4 | `JournalWriter::open` 失败(段链哈希断裂、冷热冲突等,详情为 hft-journal 的错误) |
| `SnapshotUndecodable` / `VersionNeedsDrain` | 4 | 步 5 |
| `NoDecodableSnapshot` | 4 | `--recover-from-snapshot` 找不到本二进制能解码的快照 |

**`--check`** 执行步 1–2 与规则 3 的预算求和:不读秘密文件(它们只在单元启动时由 `hft-secrets` 写出;只核对 `[[accounts]].keys` 的引用完整)、不锁内存、不映射大页或 tmpfs、不建心跳块、不读写 journal 与 `DurableMeta`、不连接;退出码与错误行同上,全部通过时退出 0。

冷进程不直接调用 hft-core;它们读 journal 尾部,并经 ctl-gateway 的 Unix socket 发 `ControlCmd`(协议见 [hft-ctl](../../crates/hft-ctl/README.md))。

### 策略装配

生产策略只在一处列出:[`strategies/registry`](../../strategies/registry/README.md)(crate `hft-strategy-registry`)里唯一的 `strategy_set!` 宏调用与 `export_schemas()`。依赖它的只有 hft-core 与 [replayer](../replayer/README.md)(两个 app),所以回放与实盘的策略集合必然相同;引擎 crate(含测试用的 hft-testkit)不依赖它——需要真实策略的集成测试放在本 app 的 `tests/` 或 `strategies/registry/tests/`。新增策略 = 新 `strategies/<name>` crate + 在 registry 里加一行,hft-core 的代码与全部引擎 crate 零改动:策略经 SDK 声明 Model 内存与检查点上界,由本 app 按 advisor 预算检查([内存与 journal 段](#内存与-journal-段按主机类唯一归属) 规则 5);唯一可能的改动是调大本页内存表中的预算或容量表中的上限(如 `max_groups_per_instance`;改表后由生成器重算单元上限),这类改动单独提交,不与策略包同一改动。

- 宏(定义在 [hft-strategy-sdk](../../crates/hft-strategy-sdk/README.md))生成实现 `StrategySet` 的静态分派枚举;`hft-sequencer` 与 advisor 任务对它泛型实例化,由本 app 以泛型参数传入。
- 启动步 2 校验:`StrategyParams` 中每个 `kind_id` 都在集合里(否则退出 2,`UnknownStrategyKind`);每个账户的 `AccountMeta.bound_instance` 恰好对应一个 `StrategyParams`(一个账户只绑定一个策略实例)。
- **非生产专用策略**:描述符 `StrategyDesc.non_prod_only = true`([hft-strategy-sdk](../../crates/hft-strategy-sdk/README.md#策略描述符note报表策略命令))的策略实例绑定到任一 `is_prod = true` 的账户 → 退出 2(`ProbeOnProdAccount`);运行中试图建立这种绑定的命令由 sequencer 以 `ParamInvalid{NonProdOnly}` 拒绝([control-plane](../../docs/subsystems/control-plane.md#结果码))。
- `hft-core schemas --out <dir>` 调 `export_schemas()` 写每个策略的参数 schema JSON 与 `index.json`(每个 schema 的摘要);金样副本在 [`fixtures/strategy-schemas/`](../../fixtures/README.md#组织),CI 比对二者;CI 把输出放进发布制品,[hft-cfg](../../tools/README.md) 与 Python 工具按它通用处理参数。`--version` 打印集合的 `(name, kind_id, version, schema id/version)`;ctl-gateway 的 `GetSchema` 与 `SetParams` 校验用同一集合的 `decode_params`。

### 实例角色(prod 与 sim)

配置 `instance.name` 只允许 `"prod"` 与 `"sim"`(其他值退出 2);它同时是实例名,出现在 journal 与冷副本路径、心跳块路径、archiver 分区与 S3 前缀里。两种角色是同一个二进制,差别只在配置与 refdata,启动步 2 校验:

| | `prod`(交易所账户实例) | `sim`(模拟实例) |
|---|---|---|
| 账户 | 只有 Live 账户;出现 Sim 账户 → 退出 2(`SimAccountInProdInstance`,任何主机类)。`instance.order_env = "testnet"` 时全部账户 `is_prod = false`(测试网/模拟盘账户),`"prod"` 时全部 `is_prod = true`;不一致 → 退出 2(`EnvAccountMismatch`) | 只有 Sim 账户;出现 Live 账户 → 退出 2(`LiveAccountInSimInstance`) |
| 账户名(= refdata `AccountMeta.label`) | 不得以 `sim-` 开头 | 必须以 `sim-` 开头 |
| 交易所 key | 每个 Live 账户每 venue 一组:下单会话 key 与 REST key(Binance 为两把,OKX/Bitget 为同一把),从 `paths.secrets_dir` 读 | 不加载任何 key:配置出现 `paths.secrets_dir` 或任一 `[[accounts]].keys` → 退出 2;`hft-secrets` 不运行 |
| 任务 | 行情任务(`feed-bbo:`/`feed-md:` 或 `feed:`)、`gw:`、`acct-fetch`、`sequencer`、`advisor`、`snap-fetch`;没有 `sim-gw` | 行情任务、`sequencer`、`advisor`、`snap-fetch`、`sim-gw`;没有 `gw:` 与 `acct-fetch` |
| 行情 | 自己的 feed 连接;端点按 `instance.feed_env`(`prod` = 正式行情,`testnet` = 测试网行情);`feed_env = "testnet"` 只允许与 `order_env = "testnet"` 组合 | 自己的 feed 连接,恒为正式行情端点(`feed_env = "prod"`);不读 `prod` 实例的环或 journal |
| 下单端点 | 按 `instance.order_env`:`prod` = 正式端点,`testnet` = refdata `Endpoint{env ∈ Testnet\|Demo}` | 无 |
| 冷进程 | kill-switch、reconciler(只管本实例的 Live 账户)、watchtower、journal-archiver、ops_bot、pipeline、treasury | 不需要 kill-switch 与 reconciler;watchtower、journal-archiver、ops_bot、pipeline 按 `instance` 区分两份 journal |
| `RELEASE.json.channel = "dev"`(读本实例所属激活组的清单 `paths.release_manifest`) | 有 `is_prod = true` 账户时退出 2(`DevBuildWithProdAccount`);测试网账户允许 | 允许 |

账户名前缀规则按 refdata `AccountMeta.label` 判定([hft-refdata](../../crates/hft-refdata/README.md)),使同一主机上两个实例的账户名必然不相交(ops_bot 按账户名路由,`/kill`、`/stop` 不带实例名);违反 → 退出 2(`AccountNamePrefix`)。`sim` 实例可以先于 `prod` 激活新版本,所以 `channel` 检查只读本组链接下的清单(`current-core` 或 `current-sim`,[deploy 单元组](../../deploy/README.md#单元组与分组激活))。

**运行位置**(交付路径:M6 dev2 联调与模拟长跑 → M7 统一升级 + 延迟验收 → M8 正式实盘):

| 主机 | `sim` 实例 | `prod` 实例 |
|---|---|---|
| `dev2` | 常驻(模拟账户长跑) | **只在测试网/模拟盘联调窗口运行**(窗口由 `hft-window` 开关,见 [dev2 护栏](#dev2-护栏)):`order_env = "testnet"`、`feed_env = "testnet"`、refdata 为测试网 refdata 且只含测试网账户(门槛 ⑥ 的配置;测试网符号映射待核实(方法:按各 `venue-*` 的测试网端点拉取合约列表,与正式 refdata 的符号逐一比对));`host.class = dev2` 时任一账户 `is_prod = true` → 退出 2(`ProdAccountOnDev2`)。dev2 不放正式实盘账户 |
| 统一升级主机(默认 `c32`;`c16` 是有条件的候选,见[线程计划](#线程计划)的 `c16` 节;按 M3 末满订阅探针在 M3 结束时选定) | 与 `prod` 同机常驻,只用非隔离核、`hft-sim.slice`、独立网卡 ENI-C、独立 journal/冷盘目录与出口 IP([主机资源隔离](#主机资源隔离slice核与出口-ip));切 Live 前在这里以满规模再跑门槛 ⑦(⑩ 测量窗口之外,两段的计时与证据归 [testing §8.1](../../docs/testing.md#81-模拟账户长跑的计时与证据)) | M7:`feed_env = "prod"`、`order_env = "testnet"`,只为门槛 ⑩ 采集延迟样本(主网行情时序 + 测试网下单回路;订单类样本主要来自 `non_prod_only` 的测试网策略);这一配置下的拒单、Broken 等交易结果不作门槛 ⑥ 的证据。M8 起 `order_env = "prod"`(正式实盘)。`order_env = "prod"` 只允许 `host.class ∈ {c8, c16, c32, metal}` |

- 允许的组合(步 2 校验,不符 → 退出 2,`InstanceHostClass`):`sim` 实例只在 `host.class ∈ {dev2, c16, c32, metal}` 上运行(`c4`、`c8` 只承载 `prod`);`order_env = "prod"` 只在 `{c8, c16, c32, metal}` 上。
- 两个实例的 refdata 各自独立(`AccountId`、`StrategyInstanceId` 各自编号);把研究中的模拟实例迁到另一台主机的 sim 实例 = 在其 refdata 里新建 Sim 账户与实例(模拟真值从 `initial_balances` 起步),不搬状态。
- `order_env = "testnet"` 的 `prod` 实例:journal-archiver 按 `is_prod = false` 把它归档到测试网前缀(规则归 [journal-archiver](../journal-archiver/README.md)),不与正式实盘数据混在一起。
- 两个实例的 journal 根、冷副本根、tmpfs 挂载、心跳块、ctl socket、`RuntimeDirectory` 全部按 `instance` 分开(`/dev/shm/hft/<instance>`、`<cold_root>/<instance>/`、`/run/hft/core-<instance>/`);sim 实例不交易任何交易所账户,不违反"一个交易所账户只由一个系统交易"。
- **未加载检查点的信号**:advisor 启动时某实例没有可用检查点(缺失、损坏、键不符)→ 写 `Note{ModelCheckpointDiscarded}`(原因码见 hft-event);pipeline 据此对该实例自动发起预热注入(`InjectWarmup`)。

### 线程计划

线程布局由配置 `[[threads]]` 驱动,不写死在代码里。**本节是 tokyo-cex 上线程、任务、核与 IRQ 专核的唯一归属地**;其他 app 的 README 引用这里,`deploy/` 的 GRUB、IRQ 计划、cpuset 与各 slice 的 `AllowedCPUs` 由当前主机类的计划**生成**,不手写([apps/AGENTS.md](../AGENTS.md))。

**主机类**:配置 `host.class ∈ {dev2, c4, c8, c16, c32, metal}`(必填;枚举与 `HostClass` 编码在 [hft-types](../../crates/hft-types/README.md),hft-rt 与 hft-refdata 使用它)。启动步 2 由 `hft_rt::check_host` 核对实际在线 CPU 数、物理核数、ISA 与内存,不符 → `CLASS_MISMATCH`,退出 3。hft-rt 的进程内检查对 `dev2`/`c4` 不把核隔离与大页列为 hard 项([hft-rt HostCheck](../../crates/hft-rt/README.md));`dev2` 不隔离核、不用大页,`c4` 的隔离核与 2 MiB 大页由 deploy 生成、由 hostprobe 核对(见 `c4` 节)。**升级主机类** = `c8`、`c16`、`c32`、`metal`(有隔离核、ENI-B IRQ 专核与大页);`c8` 只承载 `prod`(放不下 `sim`)。统一升级主机(`prod` + `sim` 同机)默认 `c32`;`c16` 只在 M3 末满订阅探针达标时保留为候选(条件见 `c16` 节)。**选档依据是 M3 末满订阅探针**:按需租用 `c8g.4xlarge`,按本节 `c16` 计划、6 个产品线满订阅(BBO、成交、标记价、资金费,另加只读不解码的压舱深度连接),打开 raw journal 与 persist,带测试网探针单,`prod` 与 `sim` 同时运行;同一次另租几小时 `c8g.8xlarge` 按 `c32` 计划跑一遍并做 ENI-B2 拆分 A/B(`[需授权]`),据此决定是否启用 IRQ 拆分(见 `c32` 节)。探针以正式版与探针主机清单装机(清单归 [deploy](../../deploy/README.md)),里程碑项归 [architecture-v2/11](../../design/11-实施路线图.md#32-m3-末满订阅探针);M1.5 的纵切片只提供分段证据,不承担选档。宁可上更大档位,不砍首批下单 venue。**升级 = 换主机类配置 + 部署对应的 GRUB/cpuset**,不改代码、不重新构建:发布目标只有 `aarch64-unknown-linux-gnu`(`target-cpu=neoverse-v1`,dev2 的 c7g 与 c8/c16/c32 的 c8g 共用同一二进制,[development.md 目标 CPU](../../docs/development.md#目标-cpu));`metal` 是可选的 x86 剖面,换架构 = 版本变化。延迟验收在统一升级主机上、`prod` 与 `sim` 同机运行时进行(门槛 ⑩,测法归 [testing §8.2](../../docs/testing.md#82-延迟验收的测法))。

**`[[threads]]` 语法**(每项一个 OS 线程):

```toml
[[threads]]
name  = "feed-bbo-bn"                              # ≤ 15 字节、实例内唯一;PR_SET_NAME;HeartbeatBlock 槽名;watchtower 用它识别
core  = 6                                          # u16,或 "none"(不绑核)
idle  = "Spin"                                     # "Spin" | { SpinThenPark = { spin_us, park_us } } | { Park = { park_us } }
nice  = 0                                          # 可选;只允许 core = "none" 的线程 > 0
tasks = ["feed-bbo:binance-usdm", "feed-bbo:binance-spot"]
```

task ∈ `sequencer`、`feed-bbo:<venue>`、`feed-md:<venue>`、`feed:<venue>`、`gw:<venue>`、`sim-gw`、`advisor`、`housekeeping`、`persist`、`ctl-gateway`、`conn-builder`、`snap-fetch`、`acct-fetch`,可选扩展 `ext:<chain>`。`<venue>` 是 `[venues.<v>]` 的键,协议名见[配置文件字段](#配置文件字段)。线程主循环是 [hft-rt `TaskLoop`](../../crates/hft-rt/README.md):按 `tasks` 顺序轮流调用各任务的 `poll_once()`/`step()`,每个任务每轮有处理上限,连续空闲后按 `idle` 退让。同线程的任务之间也只经环交互;sequencer 的输入顺序记录在 journal,确定性不受线程合并影响。

| task | tier | 职责 | 承载者 |
|---|---|---|---|
| `sequencer` | hot | 单写者决策循环 | [hft-sequencer](../../crates/hft-sequencer/README.md) |
| `feed-bbo:<venue>` / `feed-md:<venue>` / `feed:<venue>` | hot | 行情:只含 BBO 路由 / 成交、标记价与指数价、资金费路由(订阅分档深度时另含 `RouteKind::DepthTopN`;增量深度是可选扩展)/ 全部路由。hft-core 按任务名构造 `FeedRuntime<P>` 的 `routes = Bbo` / `Md` / `All`;每个 venue 要么一个 `feed:`,要么一对 `feed-bbo:` + `feed-md:`;core 行情范围归 [hft-venue](../../crates/hft-venue/README.md) | [hft-venue](../../crates/hft-venue/README.md) + `venue-*` |
| `gw:<venue>` | hot | 下单与回报(账户查询的解码不在 gw 线程,见 `acct-fetch`) | hft-venue + `venue-*` |
| `sim-gw` | warm | 全部 Sim 账户的模拟交易所:取 `out-sim`、尾随 seq journal 行情、回报进 `rep-sim` | [hft-sim `SimGwTask`](../../crates/hft-sim/README.md#sim-gw-任务task-feature) |
| `advisor` | warm | 承载每个已绑定策略实例的 `Model`:尾随 seq journal → `Model::on_event` → `Advice` 进 `advice` 环;分块写模型检查点 | [hft-sequencer advisor](../../crates/hft-sequencer/README.md),对 `StrategySet` 泛型 |
| `housekeeping` | warm | 心跳 → `sd_notify`、`ClockStatus`(每 1 000 ms 一条,节拍归本 app;用途与换算规则归 [latency-budget](../../docs/latency-budget.md))、persist/磁盘水位采样、signalfd、停机标记改名(见[启动顺序](#启动顺序)) | 本 app |
| `persist` | warm(可阻塞) | 热段 → 冷副本复制与 `fdatasync`;段预创建(`MAP_POPULATE`)与回收 | [hft-journal](../../crates/hft-journal/README.md) |
| `ctl-gateway` / `conn-builder` | warm | 控制入口 / 握手、登录与会话 key 身份核验 | [hft-ctl](../../crates/hft-ctl/README.md) / hft-net + hft-venue |
| `snap-fetch` | warm | 行情 REST:BBO 补帧(core)与增量深度的快照(可选扩展启用时);请求、读取、解密、解析,经 `snap-<venue>-bbo`(拆分时)或 `snap-<venue>` 交回请求它的行情任务;每实例一个,内含每个启用 venue 的 `SnapFetchTask<P>`(经 `VenueSet` 分派) | hft-venue `SnapFetchTask<P>` |
| `acct-fetch` | warm | Live 账户的账户查询(账户配置与 UID 核验、保证金轮询):按 gw 经 `acctreq-<venue>` 发来的请求执行签名 REST 并解码,结果经 `acct-<venue>` 交回 gw,由 gw 写 raw 并推入 `rep-<venue>`;每个有 `gw:` 的实例一个,内含每个下单 venue 的 `AcctFetchTask<P>`(分工归 [hft-venue §4a](../../crates/hft-venue/README.md)) | hft-venue `AcctFetchTask<P>` |

**加载校验**(步 2,任一不满足 → 退出 2;第 12 条的 hostprobe 部分为退出 3):

1. 每个 task 恰好出现在一个线程;必需:`sequencer`、`housekeeping`、`persist`、`ctl-gateway`、`conn-builder`、`snap-fetch`;每个 `[venues.*]` 恰好有一个 `feed:<v>`,或同时有 `feed-bbo:<v>` 与 `feed-md:<v>`(同一 venue 不混用两种形式);有 Live 账户在该 venue 交易时加 `gw:`,存在任一 `gw:` 时必须有 `acct-fetch`(`sim` 实例不得有 `gw:` 与 `acct-fetch`);refdata 中存在 Sim 账户(只可能是 `sim` 实例)时必须有 `sim-gw`,不存在时不得有;存在带 `Model` 的已绑定策略实例时必须有 `advisor`;`chain.enabled` 时必须有 `ext:`。
2. `sequencer` 独占一个线程(所有主机类)。
3. warm 任务(`sim-gw`、`advisor`、`persist`、`housekeeping`、`ctl-gateway`、`conn-builder`、`snap-fetch`、`acct-fetch`)不与热任务(`sequencer`、`feed-bbo:`、`feed-md:`、`feed:`、`gw:`、`ext:`)同线程。
4. **每个 `gw:<venue>` 独占一个线程**(所有主机类,含 dev2 测试网联调;[hot-path-rules T8](../../docs/hot-path-rules.md)):该线程没有任何其他任务,包括另一个 `gw:`;行情任务之间可以同线程(受第 13 条约束)。违反 → 退出 2(`GwSharesThread`)。
5. `persist` 独占一个线程(`fdatasync` 会阻塞);`sim-gw` 不与 `advisor` 或 `persist` 同线程(模型长步骤与阻塞 I/O 会推迟模拟回报的交付);升级主机类的 `prod` 实例:`persist` 线程所在核上没有其他线程,该核不是 hk 核或 IRQ 专核,且 `persist` 与 `advisor` 不在同一个核。
6. `Spin` 线程必须绑核,同一核上不能有第二个 `Spin` 线程;升级主机类上 `prod` 实例的热线程必须 `Spin` 且在隔离核上,每个 `gw:` 线程独占一个隔离核;交易网卡的 IRQ 专核(`host.irq_cpus.eni_b`,启用 ENI-B2 时另有 `eni_b2`)上不绑任何线程(两个实例的 `[[threads]]` 都适用;IRQ 专核上允许的"保持唤醒"形式不是 hft-core 线程,见第 12 条)。核数不够放下全部下单 venue 的 gw → 换更大的主机类,不合并。
7. `dev2` 上任何线程都不得是 `Spin`(只允许 `SpinThenPark`/`Park`)。**心跳约束**(全部主机类、每个带 `park_us` 的线程;本条是这项校验的唯一归属,hft-rt 链接这里):`park_us × 4 < watchdog.hb_stale_ms × 1 000`——`park_us` 以微秒计,`hb_stale_ms` 以毫秒计,两边都换算成微秒比较(`hb_stale_ms = 1000` 时 `park_us` 须 < 250 000;`Park{10000}`、`Park{300}` 都通过)。
8. `sim` 实例在任何主机类上都没有 `Spin` 线程;绑核时核号不得在 `host.hot_cpus` 内(同机时那是 `prod` 的隔离核)→ 退出 2(`SimOnHotCore`);hft-rt 的 `WARM_ON_ISOLATED` 检查位同时兜底。
9. 线程数 ≤ 30(`HeartbeatBlock` 的 claim 槽上限;多任务线程 kind = `Mixed`)。
10. `housekeeping` 不与任何可阻塞任务同线程,且所在线程 `nice = 0`。
11. `ctl-gateway`、`conn-builder`、`snap-fetch`、`acct-fetch`、`advisor`、`sim-gw` 以 `poll_once` 形态运行,从不阻塞;`advisor` 每次 `poll_once` 最多处理一个模型步骤或写一个检查点块。`dev2` 的 systemd `WatchdogSec` ≥ 10 s;`watchdog.hb_stale_ms` 是停摆判定窗口的**唯一归属**,kill-switch 读同一份 hft-core 配置。
12. **隔离核列表与 IRQ 计划由本计划生成**(升级主机类;`c4` 只生成隔离集合,没有 IRQ 专核,见 `c4` 节):`host.hot_cpus` 必须等于 `prod` 实例全部 `Spin` 线程所在核的集合(不等 → 退出 2);deploy 的生成器读同一份 `[[threads]]` 模板与 `host.irq_cpus` 算出 GRUB `isolcpus`(= 隔离集合)、`rcu_nocbs`(= 隔离集合 ∪ 交易网卡 IRQ 专核 `eni_b` 与启用时的 `eni_b2`)、`nohz_full`(只含 sequencer 核)、`irqaffinity`(= hk 核与交易网卡 IRQ 专核)、IRQ 计划(ENI-A → hk 核,ENI-B 全部队列 → `eni_b` 核,启用时 ENI-B2 全部队列 → `eni_b2` 核,ENI-C 全部队列 → `eni_c` 核;XPS 按热核分配发送队列,规则归 [deploy](../../deploy/README.md))与各 slice 的 `AllowedCPUs`;hostprobe 报告的实际 `isolcpus` 或 IRQ 落点与期望不符 → 退出 3。IRQ 专核的卫生同样是 hostprobe 的 hard 项([tools/hostprobe](../../tools/README.md#hostprobe)):NVMe 等 managed IRQ 的 `effective_affinity` 不落在任何 IRQ 专核上,IRQ 专核上没有可运行任务(允许的"保持唤醒"形式——`nohlt` 或 hostprobe 白名单中的 keepawake 线程——例外,登记归 tools/hostprobe);hft-rt 的 `TASK_ON_HOT` 也覆盖 IRQ 专核,但只是运行期告警,不是 hard 项。没有下单账户的 venue 不分配 gw 核,其核不进隔离集合,由生成器并入批处理核集合。
13. **BBO 不与深度同线程**(升级主机类的 `prod` 实例):`feed-bbo:*` 任务不与任何 `feed-md:*` 任务同线程,且不使用 `feed:<v>`(`routes = All`)形式;违反 → 退出 2(`BboSharesWithMd`)。`feed-bbo:*` 之间、`feed-md:*` 之间可以跨 venue 合并。`dev2`/`c4` 与 `sim` 实例可用 `feed:<v>`(`routes = All`,轮转规则归 hft-venue)。行情线程的就绪探测(`epoll_pwait(…, timeout = 0)`)的适用条件与调用频率归 [hot-path-rules T7](../../docs/hot-path-rules.md#线程t),生效判定归 hft-venue。

**升级主机类的布局原则**(`c8`/`c16`/`c32`/`metal`;下列各表按它生成,deploy 生成器读取):

- 核 0:`hk` 线程 + 系统服务(kworker、systemd、sshd、chronyd)+ ENI-A(管理网卡)IRQ。
- 核 1:**ENI-B(交易网卡)全部队列的 IRQ 与软中断独占**,不放任何线程,在 `rcu_nocbs` 内。收包为中断驱动(busy poll 关),参数与校验归 [hft-net](../../crates/hft-net/README.md) 与 [deploy](../../deploy/README.md)。`c32` 另预留第二个 IRQ 核与第二块交易网卡 ENI-B2(默认不启用,见 `c32` 节)。
- `persist`(含段预创建与回收)独占一个非隔离核,不在核 0/1,不与 advisor 同核。
- warm 线程承载 `ctl-gateway`、`conn-builder`、`snap-fetch`、`acct-fetch`;`advisor` 另占一核(`c16` 例外:与 warm 同核)。warm 与 advisor 线程 `Park{park_us = 300}`(取值范围 200–500,待标定):Spin 生产者不发唤醒,快照请求、建连交接、账户查询与 advisor 尾随最多晚一个 `park_us` 被发现。
- 隔离热核(`Spin`):`seq`(`nohz_full`);`feed-bbo-*`(只承载 BBO 路由,可跨 venue 合并);`feed-md-*`(成交、标记价/指数价、资金费与可选的分档深度路由);每个下单 venue 一个 `gw-*`。
- `sim`:自己的非隔离核 + 独立 ENI-C;ENI-C 全部队列的 IRQ 落在 `host.irq_cpus.eni_c`(`c32`:独立的核 22,不在任何 slice;`c16`:sim 的核 13,与 sim 的非热线程共用);批处理与冷进程另占核。
- 少于表中下单 venue 数时,空出的 gw 核并入批处理核集合;核号表由生成器按启用的下单 venue 输出。
- 线程计划在 M3 末满订阅探针后定档(见上文主机类);`kernel_rx` 预算按主机类给出(`c32` 为 [latency-budget §3](../../docs/latency-budget.md#3-cex-路径预算) 的数字,`c16` 的数字待该探针)。

#### `dev2`:现有 `c7g.large`(2 vCPU,3.8 GB,与旧系统同机)

dev2 上同时运行旧系统(现有套利引擎的正式账户,下称"旧系统")。用途(M6):开发联调、功能与正确性测试、`sim` 实例的模拟账户长跑、交易所测试网/模拟盘联调(`prod` 实例,`order_env = feed_env = "testnet"`,只在联调窗口运行)、验证包的累计运行(`hft-verify`,批处理)。**无延迟验收**;受 [dev2 护栏](#dev2-护栏) 约束,旧系统优先。两个实例的线程都不隔离核、都不 `Spin`;只有一块网卡,不设 IRQ 专核与 ENI-C。

`sim` 实例(常驻):

| 线程 | core | idle | nice | tasks |
|---|---|---|---|---|
| `feed` | 0 | `SpinThenPark{spin_us = 20, park_us = 1000}` | 0 | 每个启用 venue 一个 `feed:<venue>`(`routes = All`) |
| `seq` | 1 | `SpinThenPark{spin_us = 20, park_us = 1000}` | 0 | `sequencer` |
| `hk` | `none` | `Park{park_us = 10000}` | 0 | `housekeeping` |
| `persist` | `none` | `Park{park_us = 10000}` | 10 | `persist`(含段预创建与回收) |
| `warm` | `none` | `SpinThenPark{spin_us = 0, park_us = 10000}` | 10 | `ctl-gateway`、`conn-builder`、`snap-fetch` |
| `sim` | `none` | `Park{park_us = 1000}` | 10 | `sim-gw` |
| `adv` | `none` | `Park{park_us = 10000}` | 15 | `advisor` |

`prod` 实例(测试网/模拟盘联调窗口):

| 线程 | core | idle | nice | tasks |
|---|---|---|---|---|
| `feed` | 0 | `SpinThenPark{spin_us = 20, park_us = 1000}` | 0 | 每个启用 venue 一个 `feed:<venue>`(`routes = All`) |
| `gw-<v>`(每个下单 venue 一个) | 0 | `SpinThenPark{spin_us = 0, park_us = 1000}` | 0 | 恰好一个 `gw:<venue>` |
| `seq` | 1 | `SpinThenPark{spin_us = 20, park_us = 1000}` | 0 | `sequencer` |
| `hk` / `persist` / `warm` / `adv` | `none` | 同上表 | 同上表 | `housekeeping` / `persist` / `ctl-gateway`、`conn-builder`、`snap-fetch`、`acct-fetch` / `advisor` |

- 不隔离核、不用大页、不改 GRUB、不改整机 PM QoS(不打开 `/dev/cpu_dma_latency`);journal 热段在每实例一个的专用 tmpfs(见[主机资源隔离](#主机资源隔离slice核与出口-ip)),环、簿、槽用普通页(`Backing::Heap` + `mlockall`)。
- 订阅(待标定):`sim` 每 venue `venues.<v>.max_sub_insts` ≤ 16、合计 ≤ 96;测试网 `prod` 合计 ≤ 64;订阅分档深度(TopN)的 inst ≤ 32;raw 模式见内存表。dev2 的 `sim` 按护栏内存预算以**缩小规模**运行(容量见[内存与 journal 段](#内存与-journal-段按主机类唯一归属)):门槛 ⑦ 的 C 阶段前置是 dev2 缩小规模的 ⑦ 通过,满规模的 ⑦ 在切 Live 前于统一升级主机的 `sim` 实例上再连续运行(判定与计时归 [testing §8.1](../../docs/testing.md#81-模拟账户长跑的计时与证据))。
- `spin_us` 与 `park_us` 待标定(M3 前实测):目标是两个实例合计空载 CPU < 8%、不推高旧系统的 PSI。

#### `c4`:4 物理核(联调;可选)

`c8g.xlarge`(8 GiB),只承载 `prod` 实例,**至多一个下单 venue**(第 4 条);热线程 `Spin`;无延迟验收;不是升级主机类:**没有 IRQ 专核**,全部 IRQ(含交易网卡)落在核 0,`host.irq_cpus` 不出现,deploy 不按升级主机布局为它生成 IRQ 计划;隔离核 `1-3`(= `host.hot_cpus`,GRUB 由 deploy 生成,hostprobe 核对)与 2 MiB 大页(按内存表);第 13 条不适用;不打开 `/dev/cpu_dma_latency`。

| 线程 | core | idle | tasks |
|---|---|---|---|
| `hk` / `persist` / `warm` / `adv` | 0 | `Park{5000}` | `housekeeping`(核 0 同时承担全部 IRQ 与冷进程)/ `persist` / `ctl-gateway`、`conn-builder`、`snap-fetch`、`acct-fetch` / `advisor` |
| `seq` | 1 | `Spin` | `sequencer` |
| `feed` | 2 | `Spin` | 每个启用 venue 一个 `feed:<venue>`(`routes = All`) |
| `gw-<v>` | 3 | `Spin` | 唯一的下单 venue 的 `gw:` |

#### `c8`:`c8g.2xlarge`(8 物理核,16 GiB;只承载 prod)

Graviton4,无 SMT,逻辑 CPU 0–7。只承载 `prod` 实例与冷进程,**至多两个下单 venue**;放不下同机的 `sim` 实例(核与内存都不够),不是统一升级主机候选,只在 `sim` 另有主机时使用。8 个核放不下布局原则的全部专核:**`c8` 例外**——warm、advisor 与冷进程落在核 0。

| 线程 | core | idle | tasks |
|---|---|---|---|
| `hk` | 0 | `Park{5000}` | `housekeeping`(核 0 同时承担 ENI-A IRQ、kworker、systemd、sshd、chronyd) |
| `warm` / `adv` | 0 | `Park{300}` | `ctl-gateway`、`conn-builder`、`snap-fetch`、`acct-fetch` / `advisor` |
| (冷进程与批处理) | 0 | — | `prod` 的冷进程(`hft-core.slice`)与批处理(根级 `hftbatch.slice`,`CPUWeight=idle`) |
| (ENI-B IRQ) | 1 | — | 无线程 |
| `persist` | 2 | `Park{5000}` | `persist` |
| `seq` | 3 | `Spin` | `sequencer`(`nohz_full`) |
| `feed-bbo` | 4 | `Spin` | 全部 `feed-bbo:*` |
| `feed-md` | 5 | `Spin` | 全部 `feed-md:*`(成交、标记价/指数价、资金费;分档深度可选) |
| `gw-<v1>` / `gw-<v2>` | 6 / 7 | `Spin` | 两个下单 venue 的 `gw:` |

- 隔离核 `3-7`;`host.irq_cpus = {eni_a = 0, eni_b = 1}`;`irqaffinity=0,1`;`rcu_nocbs=1,3-7`;`system.slice`/`user.slice` 的 `AllowedCPUs=0`。
- 增量深度(可选扩展)在 `c8` 上只有内存表"可选扩展:增量深度"一行的余量,启用前按该行复核。
- 计数器分辨率(`CNTFRQ_EL0`)、PTP 硬件时钟是否存在,开机后核实并写入 HostProfile。

#### `c16`:`c8g.4xlarge`(有条件的候选:首批下单 venue ≤ 4)

16 vCPU(Graviton4,无 SMT,逻辑 CPU 0–15),32 GiB。承载 `prod` 实例(至多 4 个下单 venue)、同机的 `sim` 实例与冷进程;旧系统不在这台主机上。**有条件的候选**:只有在 M3 末满订阅探针(在 `c8g.4xlarge` 上按本节计划运行)达标、且 `feed-bbo-*` 线程启用就绪探测(`epoll_pwait(0)`,规则见第 13 条)时保留;否则统一升级主机用 `c32`。这一档没有核可留给第二个 IRQ 核,也放不下增量深度(可选扩展)所需的第二个 md 核。

`prod` 实例:

| 线程 | core | idle | tasks |
|---|---|---|---|
| `hk` | 0 | `Park{5000}` | `housekeeping`(核 0 同时承担 ENI-A IRQ、kworker、systemd、sshd、chronyd) |
| (ENI-B IRQ) | 1 | — | 无线程 |
| `persist` | 2 | `Park{5000}` | `persist`(含段预创建与回收) |
| `warm` / `adv` | 3 | `Park{300}` | `ctl-gateway`、`conn-builder`、`snap-fetch`、`acct-fetch` / `advisor`(`c16` 例外:advisor 与 warm 同核;仍不与 persist 同核) |
| `seq` | 4 | `Spin` | `sequencer`(`nohz_full`) |
| `feed-bbo-a` / `feed-bbo-b` | 5 / 6 | `Spin` | `feed-bbo:*` 分两组(初值:a = Binance 两个产品线,4 个 socket;b = OKX 与 Bitget 的四个产品线,8 个 socket;按 M3 末探针重分) |
| `feed-md` | 7 | `Spin` | 全部 `feed-md:*`(成交、标记价/指数价、资金费;分档深度可选) |
| `gw-<v1>` … `gw-<v4>` | 8–11 | `Spin` | 四个下单 venue 的 `gw:` |
| (冷进程与批处理) | 12 | — | `prod` 的冷进程(`hft-core.slice`)与批处理(根级 `hftbatch.slice`,`CPUWeight=idle`);冷进程(含 watchtower 的采样)在争用时优先 |

`sim` 实例(`hft-sim.slice`,`AllowedCPUs=13-15`,全部 `Park`):

| 线程 | core | idle | tasks |
|---|---|---|---|
| `hk` / `persist` / `warm` / `adv` | 13 | `Park{10000}` | `housekeeping` / `persist` / `ctl-gateway`、`conn-builder`、`snap-fetch` / `advisor`(核 13 同时承担 ENI-C 全部队列的 IRQ) |
| `seq` | 14 | `Park{1000}` | `sequencer` |
| `feed` / `sim` | 15 | `Park{1000}` | 每个启用 venue 一个 `feed:<venue>`(`routes = All`)/ `sim-gw` |

- 隔离核 `4-11`;`host.irq_cpus = {eni_a = 0, eni_b = 1, eni_c = 13}`;`irqaffinity=0,1`;`rcu_nocbs=1,4-11`;`system.slice`/`user.slice` 的 `AllowedCPUs=0,12`;`sim` 实例与批处理永不使用隔离核。
- `kernel_rx` 的 `c16` 预算待 M3 末探针给出(按主机类分写,归 [latency-budget §3](../../docs/latency-budget.md#3-cex-路径预算))。

#### `c32`:`c8g.8xlarge`(统一升级主机默认候选,prod + sim 同机)

32 vCPU(Graviton4,无 SMT,逻辑 CPU 0–31),64 GiB。承载 `prod` 实例(全部 6 个产品线都可下单)、同机的 `sim` 实例、冷进程与批处理(含验证包的累计运行)。

`prod` 实例:

| 线程 | core | idle | tasks |
|---|---|---|---|
| `hk` | 0 | `Park{5000}` | `housekeeping`(核 0 同时承担 ENI-A IRQ、kworker、systemd、sshd、chronyd) |
| (ENI-B IRQ) | 1 | — | 无线程 |
| `persist` | 2 | `Park{5000}` | `persist`(含段预创建与回收) |
| `warm` | 3 | `Park{300}` | `ctl-gateway`、`conn-builder`、`snap-fetch`、`acct-fetch` |
| `adv` | 4 | `Park{300}` | `advisor` |
| `seq` | 5 | `Spin` | `sequencer`(`nohz_full`) |
| `feed-bbo-bn` / `feed-bbo-okx` / `feed-bbo-bg` | 6 / 7 / 8 | `Spin` | 各所两个产品线的 `feed-bbo:*`(每线程约 4 个 socket;实际条数待核实(方法:按各所每连接的流数上限与目标订阅数算连接数,Binance USDⓈ-M 每连接 200 流),可能到 6 个) |
| `feed-md-bn` / `feed-md-okx` / `feed-md-bg` | 9 / 10 / 11 | `Spin` | 各所两个产品线的 `feed-md:*`(成交、标记价/指数价、资金费;分档深度与增量深度可选) |
| `gw-bn-usdm` / `gw-bn-spot` | 12 / 13 | `Spin` | `gw:binance-usdm` / `gw:binance-spot` |
| `gw-okx-swap` / `gw-okx-spot` | 14 / 15 | `Spin` | `gw:okx-swap` / `gw:okx-spot` |
| `gw-bg-fut` / `gw-bg-spot` | 16 / 17 | `Spin` | `gw:bitget-usdt-futures` / `gw:bitget-spot` |
| (冷进程) | 18、29 | — | `prod` 的冷进程(`hft-core.slice`)与 `system.slice` 中的 hft 单元 |
| (批处理) | 19–21、30–31 | — | `hftbatch.slice`(含 `hft-verify`、`hft-replay-verify`);ENI-B2 未启用时核 28 也并入 |
| (ENI-C IRQ) | 22 | — | 无线程 |
| (ENI-B2 IRQ,预留) | 28 | — | 默认不启用;启用后为 ENI-B2 全部队列的 IRQ 专核,不放任何线程 |

`sim` 实例(`hft-sim.slice`,`AllowedCPUs=23-27`,全部 `Park`):

| 线程 | core | idle | tasks |
|---|---|---|---|
| `seq` | 23 | `Park{1000}` | `sequencer` |
| `feed` | 24 | `Park{1000}` | 每个启用 venue 一个 `feed:<venue>`(`routes = All`) |
| `sim` | 25 | `Park{1000}` | `sim-gw` |
| `adv` | 26 | `Park{10000}` | `advisor` |
| `hk` / `persist` / `warm` | 27 | `Park{10000}` | `housekeeping` / `persist` / `ctl-gateway`、`conn-builder`、`snap-fetch` |

- 隔离核 `5-17`;`host.irq_cpus = {eni_a = 0, eni_b = 1, eni_c = 22}`(启用 ENI-B2 时另有 `eni_b2 = 28`);`irqaffinity=0,1`(启用时 `0,1,28`);`rcu_nocbs=1,5-17`(启用时另含 28);`system.slice`/`user.slice` 的 `AllowedCPUs=0,18,29`;`sim` 实例与批处理永不使用隔离核与 IRQ 专核。
- **第二块交易网卡 ENI-B2 与 IRQ 核 28(预留,默认不启用)**:启用后,延迟敏感流(BBO 连接、下单连接、用户流)留在 ENI-B(IRQ 核 1),批量流(`feed-md` 的成交/标记价/资金费/深度连接、snap-fetch 的公共 REST)走 ENI-B2(IRQ 核 28)。ENI-B2 只承载行情与不带 key 的 REST,它的地址不进任何 key 的 IP 白名单;`gw:` 与 `acct-fetch` 的连接不得使用它(步 2 校验,不符退出 2)。**启用条件**:M3 末满订阅探针(含 `c8g.8xlarge` 上的拆分 A/B)中 BBO 的 `nic_to_kernel` p99 在深度/标记价突发窗口内显著上升;HW RX 时间戳不可用或 `unsynced_cal` 占比偏高、`nic_to_kernel` 算不出时,改看"回显对端 TCP RTT 的 A/B"或"IRQ 核上的软中断排队时长"之一显著上升(两种替代测法归 [testing §8.2](../../docs/testing.md#82-延迟验收的测法) 第 10 条;阈值写在该探针的 Agent Note)。启用 = 主机清单加 `[eni_b2]`、`host.irq_cpus.eni_b2 = 28`、重新生成 GRUB/IRQ 计划/`egress.toml`(增加 `prod_bulk` 地址表;清单与生成物归 [deploy](../../deploy/README.md)),不改代码。

#### `metal`:`c7i.metal-24xl`(可选)

可选扩展:当统一升级主机的直方图证明主机内延迟是收益瓶颈时才值得上。关 SMT 后 48 个物理核,单 NUMA(SNC 状态待核实(方法:见开发备忘))。按布局原则:核 0 = hk、系统与 ENI-A IRQ;核 1 = ENI-B IRQ 专核(ENI-B2 规则同 `c32`);`persist`、warm(`ctl-gateway`、`conn-builder`、`snap-fetch`、`acct-fetch`)、`advisor` 各占一个不隔离核;`sequencer` 与每个 `feed-bbo:`/`feed-md:`/`gw:` 各占一个隔离核(一任务一线程);`sim` 实例占 4–5 个不隔离核 + 一个 ENI-C IRQ 核;可选扩展 `ext-bsc` 与 chain-core builder profile 的热核预留;冷进程与批处理 4–5 个核。具体核号表在立项时按启用的 venue 生成,隔离集合、`nohz_full`(只给 sequencer 核)、`irqaffinity` 与 `AllowedCPUs` 规则同 `c32`。

**线程间的环**(全部在步 3 建好,容量见[配置文件字段](#配置文件字段)的环容量表;sequencer 输入输出环的种类与元素归 [hft-sequencer 环表](../../crates/hft-sequencer/README.md),行情任务、gw、snap-fetch、acct-fetch 之间的环名与**存在条件**归 [hft-venue §1](../../crates/hft-venue/README.md),下面只列名字):每 venue `bbo-<venue>`(BBO 行情任务 → seq)与 `md-<venue>`(md 行情任务 → seq)及其字节环 `md-<venue>-bulk`(分档深度快照与可选的增量深度)、`rep-<venue>`(gw → seq,只在 `prod` 实例)及 `rep-<venue>-bulk`、`acctreq-<venue>`(gw → acct-fetch,`AcctReq`)与 `acct-<venue>`(acct-fetch → gw,`ByteSpsc`;两者只在该 venue 有 gw 时)、`out-<venue>`(seq → gw,`OutMsg` = 64 B `GwCmd`)、`feedctl-<venue>`(seq → 行情任务)、`snapreq-<venue>`(行情任务 → snap-fetch,`SnapReq`)与 `snap-<venue>`(snap-fetch → 行情任务,`ByteSpsc`,携带原始响应体与解析后的分片;不是每个 venue 都有,见 hft-venue §1);某 venue 拆成独立的 `feed-bbo:` 与 `feed-md:` 两个任务时,BBO 任务另有 `feedctl-<venue>-bbo`、`snapreq-<venue>-bbo`、`snap-<venue>-bbo`(BBO 补帧)与 raw journal `raw-feed-<venue>-bbo`(`FeedCmd` 的分发规则同样归 hft-venue §1);`out-sim`(seq → sim-gw)与 `rep-sim`、`rep-sim-bulk`(sim-gw → seq;只在 `sim` 实例);`advice`(advisor → seq,变长 `ByteSpsc`);`ctl`(`ByteSpsc`)、`hk`(housekeeping → seq);可选扩展 `chain-in`/`chain-out`;以及 `conn-builder` → 各行情任务、gw、snap-fetch 与 acct-fetch 的连接交接环。sequencer 的取环优先级、md 逐条处理、每轮上限与环头检查的规则与取值都归 [hft-sequencer](../../crates/hft-sequencer/README.md)(ECF1 段 5)。

线程创建后第一件事:`core` 非 `none` 时绑核、`prctl(PR_SET_TIMERSLACK, 1)`、`nice` 生效、命名、claim 一个 `HeartbeatBlock` 槽,预触摸栈与缓冲,再进入 `TaskLoop::run`。为 `idle ≠ Spin` 的线程创建 `Parker`,把 `Unparker` 只交给由非 `Spin` 线程持有的生产端;`Spin` 线程持有的生产端(含热线程滚动段时推给 persist 的 `RetiredSegment`)一律 `wake = None`,不发唤醒系统调用([hot-path-rules T7](../../docs/hot-path-rules.md)),消费者靠 `park_us` 超时发现新数据。所有主机类都不用 `SCHED_FIFO`([research/05 §3.1](../../design/research/05-hardware-hosting-os-tuning.md))。

### 内存与 journal 段(按主机类唯一归属)

**本节是各主机类、各实例的 journal 段大小、热段保留份数、hft-core 内存预算(含 `advisor.mem_budget_mib`)、大页预留、容量、冷盘容量与整机各单元 `MemoryMax` 的唯一归属**;[hft-journal](../../crates/hft-journal/README.md) 与 [journal-format](../../docs/subsystems/journal-format.md) 只写公式并链接这里,deploy 的单元覆盖文件与大页参数由本节生成。容量数字只在本节出现,其他 crate 链接本节。

**规则**:

1. 每个 journal 常驻热段 = `(2 + hot_keep) × segment_bytes`(1 活动 + 1 预创建 + 保留);段大小的估算公式(最忙小时量 × 1.5)见 journal-format 三类 journal。
2. 段大小受两条约束:**不超过本主机类的内存预算**(本节的表),且 **≥ 16 × 引导块(快照)大小**(否则滚动开销 > 6%)。最忙小时量超过段大小时由 hft-journal 的滚动安全阀提前滚动(两级,阈值归 [hft-journal](../../crates/hft-journal/README.md)),不停写。
3. 启动步 3 用实际配置算出 Σ 热段 + 引擎区(环、簿、槽、快照暂存)+ `advisor.mem_budget_mib` + 堆上限,超过 `host.mem_budget_mib` → 退出 3。
4. **大页不计入 `memory.max`**:hugetlb 页由 cgroup 的 hugetlb 控制器单独计量,不受单元 `MemoryMax` 约束;其上界是启动时预留的大页数(GRUB `hugepages=`,由本节生成)。所以单元的 `MemoryMax` 只等于下表的普通页部分(含 tmpfs 页与套接字缓冲等内核内存),`host.mem_budget_mib` 才是全部内存(大页 + 普通页)的预算。
5. **Model 内存**:装配时对每个已绑定策略实例求 SDK `Model::mem_bytes(params, n_groups)` 之和,超过 `advisor.mem_budget_mib` → 退出 2(`ModelMemBudget`);对 `Model::snapshot_bytes(params, n_groups)` 之和的两倍超过 `advisor.checkpoint_max_mib` 同样退出 2(`ModelMemBudget`)。运行中 `SetParams` 与绑定变更由 sequencer 复核同一组预算(内存预算与检查点盘上限都查;字段与规则归 [hft-sequencer](../../crates/hft-sequencer/README.md),回放确定),超出 → `ParamInvalid{ModelMemBudget}`([control-plane](../../docs/subsystems/control-plane.md#结果码))。**预算的唯一来源是下表 advisor 一行与[检查点表](#内存与-journal-段按主机类唯一归属)的磁盘上限**:`tools/hft-cfg` 生成 EngineConfig 时写进 ECF1 段 5 的 `advisor_mem_budget_mib` 与 `advisor_ckpt_max_mib`,deploy 生成实例配置时写 `advisor.mem_budget_mib` 与 `advisor.checkpoint_max_mib`;启动步 2 校验两两相等,不等 → 退出 2(`ModelMemBudget`)。调大预算 = 改本表,由生成器重算该单元 `MemoryMax`、slice 上限与整机余量。
6. **大页数从 journal 列表求和**:`prod` 的 journal 列表由线程计划与 `[venues]` 唯一确定——`seq` 一份;每个 `feed:<v>` 或 `feed-md:<v>` 一份 `raw-feed-<v>`;每个 `feed-bbo:<v>` 一份 `raw-feed-<v>-bbo`;每个 `gw:<v>` 一份 `raw-gw-<v>`。2 MiB 页数 = Σ(用 2 MiB 页的 journal)`(2 + hot_keep) × segment_bytes` / 2 MiB + 64(余量);用 1 GiB 页的主机类,1 GiB 页数 = (`seq` 热段 + 引擎区)/ 1 GiB + 1(余量)。`cargo xtask gen-host --check`([tools](../../tools/README.md#cargo-xtask-gen-host))按同一公式从该主机的线程计划重算,与下面的内存表、大页预留和生成的 GRUB `hugepages=` 逐项对账,任一不符即失败;hft-core 启动步 3 再用实际配置算一次(规则 3)。

**行情范围**:下列各表按 core 行情范围取值——BBO、逐笔成交、标记价/指数价、资金费与 BBO 补帧(REST);策略需要深度时用分档快照流(`RouteKind::DepthTopN`,整替换应用,无 REST 重同步;规则归 [hft-venue](../../crates/hft-venue/README.md))。全量增量深度与 REST 快照重同步是可选扩展(触发条件:某个策略需要全深度),它的增量预算单列在内存表之后。

**journal 段(`segment_bytes` / `hot_keep` / 页)**:

| journal | `dev2` `sim`(tmpfs) | `dev2` `prod` 测试网(tmpfs) | `c4`(2 MiB 页) | `c8` `prod` | `c16`/`c32` `prod` | `c16`/`c32` `sim`(tmpfs,4 KiB 页) | `metal`(可选) |
|---|---|---|---|---|---|---|---|
| `seq` | 48 MiB / 1 | 32 MiB / 1 | 256 MiB / 1 | 1 GiB / 1(1 GiB 页) | 1 GiB / 1(1 GiB 页) | 512 MiB / 1 | 2 GiB / 2(1 GiB 页) |
| `raw-feed-<venue>`(md 路由:成交、标记价/指数价、资金费、分档深度、REST 响应;`routes = All` 时含 BBO) | 4 MiB / 0 | 4 MiB / 0 | 64 MiB / 0 | 64 MiB / 0(2 MiB 页) | 128 MiB / 0(2 MiB 页) | 8 MiB / 0 | 256 MiB / 1(2 MiB 页) |
| `raw-feed-<venue>-bbo`(拆分时:BBO 与 BBO 补帧) | 无(`routes = All`) | 无(`routes = All`) | 无(`routes = All`) | 64 MiB / 0(2 MiB 页) | 128 MiB / 0(2 MiB 页) | 无(`routes = All`) | 256 MiB / 1(2 MiB 页) |
| `raw-gw-<venue>` | 无(没有 gw) | 2 MiB / 0 | 16 MiB / 0 | 32 MiB / 0(2 MiB 页) | 32 MiB / 0(2 MiB 页) | 无 | 64 MiB / 1(2 MiB 页) |
| raw 模式 `feed.raw.{bbo, md}`(键与语义归 [hft-venue](../../crates/hft-venue/README.md)) | `{off, off}` | `{off, off}` | `{on, sampled}` | `{on, on}` | `{on, on}` | `{off, off}`(同机 `prod` 已全写) | `{on, on}` |
| `chain-*`(可选扩展) | 不启用 | 不启用 | 不启用 | 启用即超预算,需按规则重算或换主机类 | 同左 | 不启用 | 1 GiB / 2 |

`hot_keep = 0` 的 journal 在封存并同步后即回收热段;它的读者(watchtower、archiver、离线工具)读冷副本。`seq` 保留 1 段供同进程尾随读者(advisor、sim-gw)与冷进程的热尾随。raw 模式为 `off` 时控制帧与 REST 响应仍写。升级主机类的 `prod` 另设 `feed.raw_dups = false`(键归 hft-venue):BBO 赛马的重复帧不写 raw,省去 feed-bbo 线程上每个重复帧一次 raw 追加;需要赛马分析时在探针或临时配置里打开。`c8` 的两类 raw 段减半是为了内存余量(滚动约快一倍)。按[现状测量 §3.5](../../design/现状测量与升级分析-2026-10-01.md) 的日量(约 150 个合约 × 三所:seq ≈ 30 GB/天、raw ≈ 160 GB/天,含深度,core 行情范围下更低)估算,升级主机类 `prod` 的 `seq` 段约每 15–30 分钟滚动一次、每个 raw 段约每 5–20 分钟一次(估算);`seq` 段中关键帧的触发(周期 `keyframe_period_ms` 与事件数 `keyframe_max_events`)是 sequencer 参数(ECF1 段 5,归 [hft-sequencer](../../crates/hft-sequencer/README.md);它对恢复尾部的约束见[启动顺序](#启动顺序)步 5)。全部取值**待实测**:M3 前用现有采集数据按目标订阅实算最忙小时量与快照大小,改动只改本表。

**hft-core 内存预算(`host.mem_budget_mib`;MiB)**(dev2 与升级主机类按 6 个产品线的行情;下单 venue 数:`c4` 1、`c8` 2、`c16` 4、`c32` 6):

dev2 `prod` 的预检将 tmpfs 挂载上限全额计入单元预算，另保留 `process_memory=384 MiB` 给非 journal 内存（含延迟环）。按实际配置枚举 seq、raw-feed、raw-gw，校验热段总和、额外一段滚动余量及挂载容量；`MemoryMax` 必须覆盖 tmpfs 上限与进程余量。父 slice 与联调窗口的批处理互斥规则保持有效。

| 部分 | `dev2` `sim` | `dev2` `prod` 测试网 | `c4` | `c8` `prod` | `c16` `prod` | `c32` `prod` | `c16` `sim` | `c32` `sim` |
|---|---|---|---|---|---|---|---|---|
| `seq` 热段 | 144 | 96 | 768 | 3,072 | 3,072 | 3,072 | 1,536 | 1,536 |
| `raw-feed-<v>` 热段 | 48 | 48 | 768 | 768 | 1,536 | 1,536 | 96 | 96 |
| `raw-feed-<v>-bbo` 热段 | 0 | 0 | 0 | 768 | 1,536 | 1,536 | 0 | 0 |
| `raw-gw-<v>` 热段 | 0 | 24 | 32 | 128 | 256 | 384 | 0 | 0 |
| tmpfs 挂载内未用容量预留(仍计入 MemoryMax) | 0 | 24 | 0 | 0 | 0 | 0 | 0 | 0 |
| 引擎区(环、簿、OMS 槽、快照暂存) | 64 | 72 | 1,024 | 2,048(1 GiB 页 × 2) | 2,048(同左) | 2,048(同左) | 1,024 | 1,024 |
| sim-gw(簿副本 + 模拟账户) | 48 | 0 | 0 | 0 | 0 | 0 | 256 | 512 |
| advisor(全部 `Model` 的堆)= `advisor.mem_budget_mib` | 160 | 48 | 256 | 384 | 512 | 1,024 | 512 | 2,048 |
| advisor 镜像状态(为 `ModelViews` 重建的簿、ledger、OMS 视图;不含环,见 [hft-sequencer advisor](../../crates/hft-sequencer/README.md)) | 48 | 32 | 384 | 768 | 768 | 768 | 768 | 768 |
| 代码、栈、其余堆与内核内存(套接字缓冲、页表) | 64 | 232 | 128 | 256 | 256 | 256 | 128 | 160 |
| **合计 = `host.mem_budget_mib`** | **576** | **576** | **3,360** | **8,192** | **9,984** | **10,624** | **4,320** | **6,144** |
| 其中大页(不计入 `memory.max`) | 0 | 0 | 1,568 | 6,784 | 8,448 | 8,576 | 0 | 0 |
| **单元 `MemoryMax`(普通页部分)** | **576** | **576** | **1,792** | **1,408** | **1,536** | **2,048** | **4,320** | **6,144** |

- dev2 `sim` 的引擎区按 core 行情范围取:环(含 `md-<v>-bulk`)约 20 MiB、分档深度簿(≤ 32 个)与 BBO 缓存、OMS 槽、快照暂存,64 MiB 含余量;advisor 160 MiB 见容量表后的 dev2 说明。
- 引擎区中的执行层 cell 表按 [hft-exec](../../crates/hft-exec/README.md) 的公式(行数 = `max_cells_total` + 账户数,行大小与索引开销归 hft-exec)计;按每行 < 1 KiB 的上界,最大的 `c32` `sim`(1,024 + 16 行)< 1.1 MiB,dev2 `sim`(160 + 16 行)< 0.2 MiB,各列引擎区都已覆盖,不改表。

**大页预留**(只给 `prod`;按规则 6 由 journal 列表求和,deploy 生成 GRUB `hugepages=`;`sim` 实例与 dev2 不用大页):

| 主机类 | 1 GiB 页 | 2 MiB 页 | 推导 |
|---|---|---|---|
| `c4` | 0 | 848 | (`seq` 768 + `raw-feed` 6 × 128 + `raw-gw` 32)/ 2 + 64;`c4` 全用 2 MiB 页 |
| `c8` | 6 | 896 | 1 GiB:`seq` 3 + 引擎区 2 + 1;2 MiB:(`raw-feed` 6 × 128 + `-bbo` 6 × 128 + `raw-gw` 2 × 64)/ 2 + 64 |
| `c16` | 6 | 1,728 | 同上;2 MiB:(6 × 256 + 6 × 256 + 4 × 64)/ 2 + 64 |
| `c32` | 6 | 1,792 | 同上;2 MiB:(6 × 256 + 6 × 256 + 6 × 64)/ 2 + 64 |

**可选扩展:增量深度的增量预算**(启用全量增量深度与 REST 重同步时,在上表之上增加;启用即按本行改表并重新生成):

| 项 | `dev2` `sim` | `c8` `prod` | `c16`/`c32` `prod` |
|---|---|---|---|
| `raw-feed-<v>` 段 | 不变(raw `off`,只写 REST 响应) | 64 → 128 MiB:+768 MiB(+384 个 2 MiB 页) | 128 → 256 MiB:+1,536 MiB(+768 个 2 MiB 页) |
| 引擎区(L2 簿 ≈ 70 KiB/簿,[hft-book](../../crates/hft-book/README.md);`snap-<v>` 与 `md-<v>-bulk` 按[环容量表](#配置文件字段)中"增量深度启用时"的取值) | +16 MiB,在 dev2 列内先减订阅再让出 | 在 2 GiB 内 | 在 2 GiB 内 |
| 线程 | 不变 | 不变 | `c32` 不变;`c16` 没有第二个 md 核,不支持 |
| 整机余量(启用后,口径同整机表) | 不变 | ≈ 2.2 GiB(偏紧) | `c32` ≈ 37.6 GiB;`c16` 不适用 |

**容量**(按主机类与实例取值的唯一归属;语义归括号里的 crate;EngineConfig 由 `tools/hft-cfg` 按本表生成,ECF1 段 3 的行格式归 hft-sequencer):

| 容量 | `dev2` `sim` | `dev2` `prod` 测试网 | `c4` | `c8`/`c16`/`c32` `prod` | `c16` `sim` | `c32` `sim` |
|---|---|---|---|---|---|---|
| `max_insts`:引擎跟踪的 inst 容量([hft-sequencer](../../crates/hft-sequencer/README.md));Σ `venues.<v>.max_sub_insts` 不得超过它 | 96 | 64 | 1024 | 4096 | 4096 | 4096 |
| `max_cells_total`:本实例全部 (account, group) cell 合计([hft-sequencer](../../crates/hft-sequencer/README.md)、[hft-exec](../../crates/hft-exec/README.md)) | 160 | 64 | 256 | 256 | 512 | 1024 |
| `max_instances`:本实例的策略实例合计,含模拟实例([hft-sequencer](../../crates/hft-sequencer/README.md)) | 6 | 4 | 8 | 8 | 16 | 16 |
| `max_groups_per_instance`:单个策略实例的 group 上限(ECF1 段 3 的行格式归 [hft-sequencer](../../crates/hft-sequencer/README.md),名字与语义归 [hft-strategy-sdk](../../crates/hft-strategy-sdk/README.md)) | 64 | 64 | 64 | 64 | 64 | 64 |
| `max_subs_per_inst` / `max_timers` / `recent_cmd_ids` | 8 / 1024 / 4096 | 8 / 1024 / 4096 | 8 / 4096 / 4096 | 8 / 4096 / 4096 | 8 / 4096 / 4096 | 8 / 4096 / 4096 |
| `ModeOutcome.cells` 与 `ModeOutcome.cancels` 容量([hft-risk](../../crates/hft-risk/README.md)) | = `max_cells_total` | 同左 | 同左 | 同左 | 同左 | 同左 |
| Sim 账户([hft-sim](../../crates/hft-sim/README.md) `MAX_SIM_ACCOUNTS`) | ≤ 16 | 0 | 0 | 0 | ≤ 16 | ≤ 16 |

- **dev2 `sim` 的规模**按"2 个研究中的策略 × 每策略 `sim_pool = 2` 个模拟实例([hft-sim](../../crates/hft-sim/README.md#预置模拟实例池))× 每实例 ≤ 32 group"定:4 个实例、128 个 cell;`max_instances` 6 与 `max_cells_total` 160 在此之上留余量;`advisor.mem_budget_mib` 160 = 4 个实例 × 32 group 的 Σ `Model::mem_bytes`(按已登记策略的声明约 128 MiB)加约 25% 余量。各策略在 sim 实例上的默认 group 数归各自的策略包;超出这一规模的研究改在统一升级主机的 `sim` 实例上做,或按调节旋钮改本表。
- **装配期交叉校验**(步 2,任一不满足 → 退出 2):cell 数超过 `max_cells_total`;Σ `venues.<v>.max_sub_insts` 超过 `max_insts`;`RiskParams.conn_liveness_ms < 2 × bbo_ping_ms`(连接活性判定需要至少两次探测间隔);**`ExecRiskMismatch`**:某账户的 `ExecConfig` 与其 `RiskParams` 不满足 [hft-risk 引擎单与执行配置的一致性](../../crates/hft-risk/README.md)(条件的唯一定义在那里;`ExecConfig` 字段的语义归 [hft-exec](../../crates/hft-exec/README.md);运行中 `SetRisk` 的同一校验返回 `InvalidArgs{ExecRiskMismatch}`);`feedctl` 环容量(环容量表)小于 2 × 任一 venue 的 `venues.<v>.max_sub_insts`;**`TombstoneKeep`**:OMS 的 `tombstone_keep_ms`([hft-oms](../../crates/hft-oms/README.md))小于任一有 gw 的 venue 的 `venues.<v>.gw.id_tomb_ms`;**`TimerCapacity`**:`max_timers` 小于按本实例装配结果(cell、OMS 槽、账户与 venue 数)算出的最坏定时器需求(公式归 [hft-sequencer 定时器](../../crates/hft-sequencer/README.md#定时器);运行中堆满是引擎缺陷,按该页处理);**`DustBelowMinNotional`**:对每个账户(Live 与 Sim),其 `ExecConfig.dust_notional`(尘埃线,语义归 hft-exec)小于该账户绑定实例的参数中全部 group 交易腿 inst 的 `InstMeta.min_notional` 的最大值——否则介于两者之间的残余裸量既不算尘埃、又下不了单,自动有界平仓与补腿都无法完成(已经出现的残余由升级命令 `WriteOffResidual` 处置,[control-plane](../../docs/subsystems/control-plane.md))。`ReplaceRefdata` 的预校验与 Drain 末复核用同一组函数([refdata 变更](#refdata-变更drain--重启))。
- **缩容**:容量只能经 Drain + 换 `blobs.engine` 后重启改变。缩小前运维先把占用降到新容量以下(解绑实例、减 group、`SimReset` 等),再 Drain、换 EngineConfig、重启;步 5 恢复后,快照中的占用(inst、cell、策略实例、Sim 账户、每实例 group 数)超过新容量 → 退出 2(`CapacityShrink`)。增大不受此限。
- 测试网 → 正式实盘切换(launch D 步):测试网 journal 归档、冷副本目录搬走、hugetlbfs 上测试网实例的热段清除(否则冷热冲突,退出 4)之后,正式 `prod` 实例以 `config_source = file` 从空 journal 首次启动,这是允许的首次启动路径(不要求 Drain 快照);步骤归 [launch](../../docs/runbooks/launch.md)。
- 持久元数据(`DurableMeta`)里两层毒丸计数分开:`boot_guard`(恢复期崩溃,本节)与 `poison_input = {hash, count}`(运行期同一 Input 连续崩溃,规则归 [hft-sequencer](../../crates/hft-sequencer/README.md));`EngineClock.base_engine_ns` 取自 journal 读者的 `stop_engine_ns()`(上次运行最后一条已提交记录的引擎时间)。
- **调节旋钮**(超支时按此顺序):先减订阅(`venues.<v>.max_sub_insts`、分档深度的订阅数),再缩 `seq` 段(须仍满足规则 2),再按缩容流程降 `max_cells_total`,再降 `advisor.mem_budget_mib`(已绑定实例的声明之和超过新预算时装配失败,须先改参数或解绑),再停可选单元;不得关 `mlock`、不得依赖 swap。

**advisor 模型检查点(冷盘;本表是检查点文件格式与存放的唯一归属,写读时机见 [hft-sequencer advisor](../../crates/hft-sequencer/README.md),`snapshot_chunk`/`restore` 契约见 [hft-strategy-sdk](../../crates/hft-strategy-sdk/README.md#model-检查点))**:

| 项 | 值 |
|---|---|
| 目录 | `advisor.checkpoint_dir`(冷盘,默认 `<journal_cold>/<instance>/advisor`),每策略实例一个文件 `<strategy_instance>.ckpt` + 上一代 `.prev`;目录对 `hft-journal-ro` 组只读(journal-archiver 每天归档每个实例在 UTC 00:00 之后写出的第一份,规则归 [journal-archiver](../journal-archiver/README.md)) |
| 周期 | `advisor.model_checkpoint_ms`(默认 600 000,按事件时间)与 Drain 时各写一次 |
| 格式校验 | `_pad0` 与 `_rsv` 必须全零;体长不足路径前缀,或 `path_version = 0` 却带非空路径节均判 `Corrupt`。共用冷路径编解码器归 [hft-sequencer 检查点](../../crates/hft-sequencer/README.md#模型检查点);app 在读取前按预算限制文件大小,不可相信文件内的长度声明 |
| 磁盘上限 | `advisor.checkpoint_max_mib`:各主机类、各实例 1 024,`c16`/`c32` `sim` 4 096(本实例全部模型、两代;磁盘占用,不计入内存表);装配时须 ≥ 2 × Σ `Model::snapshot_bytes`(规则 5);运行中超出 → advisor 跳过本次写,写 `Note{ModelCkptSkipped}`(P1,经 `advice` 环) |
| 键(文件头) | `kind_id`、`Model::LAYOUT_VERSION`(策略声明的状态布局版本)、`schema.id`、策略实例(`strategy`, `account`)、`last_seq`(Model 已消费的最后一条 journal 记录)。**只有这些决定检查点能否用**(键的语义归 [hft-strategy-sdk](../../crates/hft-strategy-sdk/README.md#model-检查点)):参数版本、refdata 版本、二进制 SHA 都不使它失效——参数变化随重放经 `model_on_params` 重新应用 |
| 文件布局(草案,实现时以代码与编译期断言为准) | `CkptHdr`(64 B)‖ `PathSecHdr`(8 B)‖ 路径状态节(`path_len` 字节)‖ 原始样本节(到文件尾)。`CkptHdr`(`#[repr(C)]`、小端、无隐式填充)= `{magic: [u8; 8] = *b"HFTCKPT1", hdr_ver: u16 = 1, kind_id: u16, layout_version: u32, schema_id: u32, strategy: u8, account: u8, _pad0: [u8; 2], last_seq: u64, body_len: u64 /* CkptHdr 之后的字节数 */, crc32c: u32 /* 覆盖整个文件,本字段按 0 计 */, _rsv: [u8; 20]}`;`PathSecHdr` = `{path_version: u32 /* 写出时的 Model::PATH_VERSION */, path_len: u32}`,`PATH_VERSION = 0` 时两者为 0、路径节为空。magic、`hdr_ver`、`body_len` 或 CRC 不符,或 `path_len` 超出体长 → `Corrupt`(丢弃,原因码归 hft-event);`path_version` 与当前 `PATH_VERSION` 不符 → 只丢路径节(规则归 hft-strategy-sdk) |
| 写法 | **分块**:第一块是路径节(`Model::snapshot_path` 一次写出),之后 advisor 每次 `poll_once` 至多调一次 `Model::snapshot_chunk(from_gix, w, max_bytes)`(`max_bytes` = `advisor.checkpoint_chunk_kib` × 1024),返回 `SnapshotProgress::More(next_gix)` 时下次接着写,`Done` 时收尾;单块的编码与写入 ≤ 1 ms。写检查点期间整个 advisor 不消费 journal(镜像为全部实例共享),各块属于同一 `last_seq`(期间落后计入 `AdvisorLag`;规则归 [hft-sequencer advisor](../../crates/hft-sequencer/README.md))。收尾:在临时文件里回填 `PathSecHdr` 与 `CkptHdr`(`body_len`、`crc32c`)→ fsync → 旧文件改名 `.prev` → rename → fsync 目录 |

**冷盘容量**(`paths.journal_cold` 所在卷):容量 ≥ (Σ 实例 (`seq` 日量 × `retention.local_days.seq` + raw 日量 × `retention.local_days.raw`)+ advisor 检查点 + 验证包状态目录 + archiver 暂存)/(1 − `storage.disk_free_block_pct`)。保留天数是 [journal-archiver](../journal-archiver/README.md) 的配置(`seq` 7、`raw-*` 3)。按现状测量 §3.5 的日量估算,`c16`/`c32`:`prod` `seq` 7 天 ≈ 210 GB + `prod` raw 3 天 ≈ 480 GB + `sim` `seq` 7 天 ≈ 210 GB(`sim` raw 为 `off`,< 5 GB)+ 其余 ≈ 30 GB ≈ 0.93 TB,除以 0.85 → **冷盘 ≥ 1.1 TB(gp3)**(raw 日量按含深度的实测取,core 行情范围下是上界);`dev2` 用现有 `/data`(剩余约 119 GB),其 `retention.local_days` 按实测日量调低,使 hft 的冷盘占用 ≤ 60 GB。日量按 M3 前的实算更新本段。**冷盘吞吐**:升级主机类的 gp3 预置 ≥ 3,000 IOPS、≥ 250 MB/s(待标定;清单字段归 deploy);同卷上的写者与读者是 `prod` 的 persist、`sim` 的 journal 与检查点、archiver、`hft-replay-verify`,后三者受[主机资源隔离](#主机资源隔离slice核与出口-ip)的 I/O 带宽上限约束;M7 验收测这四者并发时 `prod` 的 `journal_persist_lag_ms` p99(目标见[延迟与资源契约](#延迟与资源契约))。

**整机 `MemoryMax`(各单元,按主机类;MiB)**:

| 单元 | slice | `dev2` | `c4` | `c8` | `c16` | `c32` |
|---|---|---|---|---|---|---|
| `hft-core@prod`(普通页部分) | `hft-core.slice` | 576(测试网,只在联调窗口) | 1,792 | 1,408 | 1,536 | 2,048 |
| `hft-core@prod` 的大页预留(不计入 `memory.max`) | — | — | 2 MiB × 848 | 1 GiB × 6 + 2 MiB × 896 | 1 GiB × 6 + 2 MiB × 1,728 | 1 GiB × 6 + 2 MiB × 1,792 |
| `hft-core@sim` | `dev2`:`hft-core.slice`;`c16`/`c32`:`hft-sim.slice` | 576 | — | — | 4,320 | 6,144 |
| `hft-watchtower` | `hft-core.slice` | 160 | 512 | 1,024 | 1,536 | 1,536 |
| `hft-reconciler` | `hft-core.slice` | 128(只在联调窗口) | 256 | 512 | 512 | 512 |
| `hft-ops-bot` | `hft-core.slice` | 128 | 192 | 256 | 256 | 256 |
| `hft-journal-archiver` | `hftbatch.slice` | 256 | 1,024 | 2,048 | 2,048 | 2,048 |
| `hft-pipeline`(常驻调度器 `hft-pipeline run`) | `hftbatch.slice` | 128 | 256 | 512 | 768 | 768 |
| `hft-treasury` | `hftbatch.slice` | 64 | 96 | 128 | 128 | 128 |
| `hft-replay-verify`(每日回放校验) | `hftbatch.slice` | 384 | 1,024 | 1,536 | 4,096 | 4,096 |
| `hft-verify`(验证包的累计运行) | `hftbatch.slice` | 384 | — | — | 2,048(只在 M7) | 2,048(只在 M7) |
| `hft-kill-switch` | `system.slice` | 64 | 64 | 64 | 64 | 64 |
| `hft-dev2-guard` / `hft-offbox-watchdog` | `system.slice` | 32 / 32 | — | — | — | — |
| `hft-cred-broker`(只在运行 S3 消费者的主机:dev2 与 `formal_live = true` 的主机,装法归 deploy) | `system.slice` | 64(待实测(方法:dev2 上读该单元 cgroup `memory.peak` 24 h,含 AWS CLI 刷新)) | — | 128 | 128 | 128 |
| **slice 上限** | — | `hft.slice` 1,376(`hft-core.slice` ≤ 1,376)、`hftbatch.slice` 512;两者不会同时满额,hft 峰值 1,376(见 dev2 整机表后的说明) | `hft-core.slice` 2,752、`hftbatch.slice` 1,536 | `hft-core.slice` 3,200、`hftbatch.slice` 2,048 | `hft-core.slice` 3,840、`hft-sim.slice` 4,400、`hftbatch.slice` 4,992 | `hft-core.slice` 4,352、`hft-sim.slice` 6,272、`hftbatch.slice` 4,992 |
| **hft 合计(含大页预留与 `system.slice` 中的 hft 单元)** | — | **1,568** | ≈ 5.9 GiB | ≈ 13.1 GiB | ≈ 22.5 GiB | ≈ 24.9 GiB |
| 系统、页缓存与余量(= 实例内存 − hft 合计;内核与系统服务的常驻占用在本行内,不单独扣) | — | 满载 `MemAvailable` ≈ 1,007(见 dev2 整机表) | ≈ 2.1 GiB(8 GiB) | ≈ 2.9 GiB(16 GiB,偏紧) | ≈ 9.5 GiB(32 GiB) | ≈ 39.1 GiB(64 GiB) |

- 合计 = 大页预留 + 各 hft slice 上限(`hft-core.slice`、`hft-sim.slice`、`hftbatch.slice`)+ `system.slice` 中的 hft 单元(kill-switch 与凭证代理);例:`c32` = 9,728(1 GiB × 6 + 2 MiB × 1,792)+ 4,352 + 6,272 + 4,992 + 192 = 25,536 MiB。
- **批处理的并发规则**(全部主机类):`hft-replay-verify` 运行时 `hft-journal-archiver` 与 `hft-pipeline` 暂停(运行前停、结束后恢复;二者的 `ExecCondition=` 在它运行时拒绝启动);`hft-verify` 在 `hft-replay-verify` 运行时不启动(主机未安装 `hft-verify` 时,停它的一步是空操作,不得使回放失败)。实现在 [deploy](../../deploy/README.md) 的基础单元(全部主机类的主表),不只在 dev2 覆盖;dev2 上另在 `hft-core@prod` 运行或标志文件存在时全部不启动([dev2 护栏](#dev2-护栏))。因此 `c16`/`c32` 的 `hftbatch.slice` = max(`hft-replay-verify` + treasury,`hft-verify` + archiver + pipeline + treasury)= max(4,224, 4,992)。
- `dev2`/`c4`/`c8` 的 `hftbatch.slice` 上限小于其中各单元上限之和(超额共享):批处理在 slice 内互相挤占,OOM 只在 slice 内选择,`hft-verify` 的 `OOMScoreAdjust` 最高、最先被杀,其次 `hft-replay-verify`(下一次运行补跑);不会因此杀到 `hft-core`。被 OOM 杀掉的 `hft-verify` 运行记为中止,不是失败(账本规则归 [testing §6.1](../../docs/testing.md#61-服务器上的累计运行))。
- `hft-verify` 在升级主机上只在 M7 期间、门槛 ⑩ 的 7 天测量窗口之外运行(定时器只在此期间启用,步骤归 [host-upgrade](../../docs/runbooks/host-upgrade.md));正式实盘(M8)开始后升级主机不再运行它,后续版本的累计在 dev2 上继续。装不装 `hft-verify.*` 由主机清单 `verify = true|false` 决定(归 deploy)。
- watchtower 与 journal-archiver 在各主机类上的内部上限(序列数、row group 大小、转换线程数)按各自配置表取值,须在本表的单元 `MemoryMax` 内运行([watchtower 配置](../watchtower/README.md#配置文件字段)、[journal-archiver 配置](../journal-archiver/README.md#配置文件字段));不满足时改它们的配置或按调节旋钮重算本表。
- `hft-ops-bot` 与 `hft-pipeline` 导入 polars/duckdb/pyarrow 后的实际峰值**待实测**(方法:dev2 上读两个单元的 cgroup `memory.peak`,常态与预热注入各 24 h);超出本表时先压缩 dev2 的 `hftbatch.slice`,再按调节旋钮处理。dev2 与升级主机各运行一个 ops_bot,各用自己的 bot token(规则归 [ops_bot](../../python/ops_bot/README.md#配置文件字段))。

**dev2 整机**(3.8 GB;实测见 [现状测量 §2](../../design/现状测量与升级分析-2026-10-01.md)):

| 部分 | 内存 | 依据 |
|---|---|---|
| 现有占用:内核与系统服务、旧系统 6 个服务、数据平台空闲服务、研究探针 | ≈ 1.1 GB | 实测总量 3.8 GB − 可用约 2.7 GB(2026-10-01 停测试账户服务之后) |
| hft 全部单元上限 | 1,568 MiB(`hft.slice` 与 `hftbatch.slice` 合计峰值 1,376 MiB + `system.slice` 中的 kill-switch 64、`hft-cred-broker` 64、`hft-dev2-guard` 32、`hft-offbox-watchdog` 32) | 上表 |
| **满载时 `MemAvailable`** | **≥ 1,000 MiB**(设计目标;停机阈值 600 MiB 之上留 400 MiB) | 可用 2.7 GB 按十进制保守取 2,575 MiB,减 1,568 MiB 得 1,007 MiB(余 7 MiB) |
| 护栏停机阈值 | `MemAvailable` < 600 MiB | [dev2 护栏](#dev2-护栏) |

- hft 峰值 1,376 MiB 覆盖两种运行状态:**常态**(`hft-core.slice` = `sim` 实例 576 + watchtower 160 + ops-bot 128 = 864 MiB,加根级 `hftbatch.slice` 512 MiB = 1,376 MiB);**联调窗口**(`hft-core.slice` 再加测试网 `prod` 实例 384 与 reconciler 128 = 1,376 MiB,此时批处理单元已由 `hft-window` 停止、窗口标志使它们无法启动,`hftbatch.slice` 为空,见 [dev2 护栏](#dev2-护栏))。`hftbatch.slice` 不在 `hft.slice` 之下([主机资源隔离](#主机资源隔离slice核与出口-ip)),所以 `hft.slice` 的 `MemoryMax=1376M` 按联调窗口取;两种状态互斥由窗口标志与 `ExecCondition=` 保证,不由父 slice 的上限保证。`sim` 实例的 576 MiB 已按"2 × 2 × 32 group"的规模重算(advisor 160、引擎区 64),dev2 整机余量不变。
- tmpfs 热段页计入写入者单元;单元停止后残留的热段页转记到父 slice。干净 Drain 后由 `hft-shm-release` 删除(见 dev2 护栏);测试网实例在窗口内崩溃而留下热段时,`hft-window stop` 不恢复批处理(见联调窗口第 2 条),所以批处理恢复后 hft 合计不会因残留热段超过 1,376 MiB;崩溃路径保留的热段在重启时由本实例复用(不重复计量)。
- 现有服务已换出的约 0.8 GB swap 不计入可用;hft 单元 `MemorySwapMax=0`,不依赖 swap。
- 研究用 collector 的采集窗口**不与 hft-core 同时长期运行**(collector 自带 `MemoryHigh 2500M`,放不进上表);排采集窗口时先 `Drain` 停两个 hft-core 实例。采集窗口会中断 `sim` 实例的长跑:排期上先采集、后开门槛 ⑦ 的长跑,或在 ⑦ 期间只排短窗口并按 [testing §8.1](../../docs/testing.md#81-模拟账户长跑的计时与证据) 门槛 ⑦ 的暂停与重新计时规则处理。
- 预算超支的处理顺序见上面的调节旋钮。

### 主机资源隔离(slice、核与出口 IP)

同一台主机上的 hft 单元按下列 systemd slice 归属(`-` 是 slice 层级分隔符:`hft-core.slice` 与 `hft-sim.slice` 在 `hft.slice` 之下;批处理的 `hftbatch.slice` 名字不带 `-`,是根级 slice,与 `hft.slice`、`system.slice` 同级),数值按主机类取上一节的表,deploy 的单元覆盖文件由本节与线程计划生成:

| slice | 成员 | `dev2` | `c8` | `c16` | `c32` |
|---|---|---|---|---|---|
| `hft-core.slice` | `hft-core@prod`、watchtower、reconciler、ops-bot;dev2 上另含 `hft-core@sim` | `CPUWeight=100`;两个 hft-core 实例的线程按线程计划 | `AllowedCPUs=0,2-7` | `AllowedCPUs=0,2-12` | `AllowedCPUs=0,2-18,29` |
| `hft-sim.slice`(只在 `prod` 与 `sim` 同机的主机类:`c16`/`c32`/`metal`) | `hft-core@sim` | — | — | `AllowedCPUs=13-15`、`CPUQuota=300%` | `AllowedCPUs=23-27`、`CPUQuota=500%`;`MemoryMax` 见上节 |
| `hftbatch.slice`(根级) | `hft-verify`、`hft-replay-verify`、`hft-journal-archiver`、`hft-pipeline`、`hft-treasury` | `CPUWeight=idle`,**不设 `CPUQuota`**;`MemoryMax=512M` | `AllowedCPUs=0` | `AllowedCPUs=12` | `AllowedCPUs=19-21,30-31`(ENI-B2 未启用时另含 28);各主机类 `CPUWeight=idle` |
| `system.slice` | `hft-kill-switch`(保证护栏或 OOM 停 hft 时兜底仍在);dev2 上 `hft-dev2-guard`、`hft-offbox-watchdog`;dev2 与 `formal_live` 主机上 `hft-cred-broker` | 不覆盖 `AllowedCPUs` | `AllowedCPUs=0`(同 `user.slice`) | `AllowedCPUs=0,12` | `AllowedCPUs=0,18,29` |

- **批处理**(全部主机类):单元 `Nice=19`;让出 CPU 靠根级 `hftbatch.slice` 的 `CPUWeight=idle`(cgroup `cpu.idle = 1`):调度器在根一级比较 `hftbatch.slice`、`hft.slice` 与 `system.slice` 这些组,idle 组只用其他组不用的 CPU,争用时让出给同核的冷进程、hft-core 与旧系统。线程级 `SCHED_IDLE` 只在同一 cgroup 内比较,不跨组生效,批处理单元不依赖它。`CPUWeight=idle` 需 systemd ≥ 252、内核 ≥ 5.15;dev2 上待核实(方法:`systemctl --version`、`uname -r`,装后读 `/sys/fs/cgroup/hftbatch.slice/cpu.idle` 与根的 `cgroup.subtree_control`)。`hftbatch.slice` 不设 `CPUQuota`:`cpu.idle` 已保证争用时让出,配额只会额外制造节流与整机 cpu 压力。
- **I/O**:EBS 卷的调度器是 `none`,`IOWeight=` 与 `IOSchedulingClass=` 在其下不生效,一律不用;改用不依赖调度器的 `io.max`,数值见下表。
- IRQ 专核(各主机类的 `eni_b` 核,`c32` 启用时的 `eni_b2` 核 28;`c32` 的 `eni_c` 核 22)不在任何 slice 的 `AllowedCPUs` 里;`c16` 的 `eni_c` 核 13 由 `sim` 的非热线程共用。
- **`hft.slice`(父)**:dev2 上 `MemoryMax=1376M`、`MemorySwapMax=0`、`CPUQuota=100%`(两核中的一核)、`CPUWeight=50`(旧系统所在 `system.slice` 为默认 100,争用时让旧系统);父级配额造成的节流只计入 `hft.slice` 自身与整机的压力,不计入旧系统所在 slice 的压力(护栏读后者,见 [dev2 护栏](#dev2-护栏));`hftbatch.slice` 不在它之下,自设上限(上表);其他主机类不设 `hft.slice` 的父级上限,只设子 slice 与 `hftbatch.slice`。

**冷盘 I/O 带宽上限**(`IOReadBandwidthMax=`/`IOWriteBandwidthMax=`,作用于冷盘所在设备):

| slice | `dev2`(`/data` 所在卷) | 升级主机类(冷盘 gp3) |
|---|---|---|
| `hftbatch.slice` | 读 40 MB/s、写 20 MB/s | 读 100 MB/s、写 40 MB/s |
| `hft-sim.slice` | — | 写 30 MB/s |
| `hft-core.slice` | 不限 | 不限(`prod` 的 persist 优先) |

数值**待标定**:dev2 在 M3 标定(先确认 `/data` 卷的类型与预置吞吐,方法:`aws ec2 describe-volumes` 只读,或 `lsblk` 与控制台);升级主机在 M7 按[冷盘吞吐](#内存与-journal-段按主机类唯一归属)一段的并发测量调整。

- **OOM 优先级**(`OOMScoreAdjust`,越小越后被杀):`hft-core@prod` −900(与 kill-switch 同为最低;kill-switch 只有 64 MiB,不会先被选中)、`hft-core@sim` −800、watchtower / reconciler / ops-bot / `hft-cred-broker` −300、archiver / pipeline / treasury +300、`hft-replay-verify` +900、`hft-verify` +950(全部 hft 单元中最先被杀)。hft-core 在 hft 单元中最低。**dev2 例外(旧系统优先)**:dev2 上全部 hft 单元整体上移,使任何 hft 单元都先于旧系统被杀——`hft-core@sim`/`hft-core@prod`(测试网)+100、watchtower / reconciler / ops-bot +300、批处理 +600、`hft-replay-verify` +900、`hft-verify` +950;kill-switch、`hft-dev2-guard`、`hft-offbox-watchdog` 仍 −900(合计 128 MiB),`hft-cred-broker` +300;hft 单元之间的先后顺序不变。
- **核**:`sim` 实例只用非隔离核(warm/冷核),永不占 `prod` 的热核与 ENI-B IRQ 专核:线程计划加载校验第 6、8 条 + hft-rt `WARM_ON_ISOLATED` + `hft-sim.slice` 的 `AllowedCPUs` 三重保证。
- **网卡**:升级主机类上 ENI-A 管理流、ENI-B 只承载 `prod` 的交易所连接(`c32` 启用 ENI-B2 时,`prod` 的批量行情与公共 REST 改走 ENI-B2,见线程计划 `c32` 节)、ENI-C(主机清单 `[eni_c]`)只承载 `sim` 的交易所连接,各网卡的 IRQ 落点按 `host.irq_cpus`。**收包网卡的自检在连接建立时做,不在 hostprobe**(hostprobe 在 hft-core 建连之前运行,也读不到别的进程的 socket):`conn-builder` 在握手收到第一个响应后读该 socket 的 `SO_INCOMING_NAPI_ID`,经 netdev netlink 映射到网卡(需内核 ≥ 6.9,待核实(方法:在 c8g 上对 ENI-B、ENI-C 各建一条连接,对照 netlink 的 NAPI 列表));`sim` 的连接落在 ENI-B,或 `prod` 的连接落在 ENI-C → 该连接不交接、该 venue 保持 `Down`,P1(实现归 [hft-net](../../crates/hft-net/README.md) 与 [hft-venue §7](../../crates/hft-venue/README.md#7-会话建立warm-线程-conn-buildersessiondriver))。dev2 只有一块网卡,不适用(dev2 无延迟验收)。
- **journal 与冷盘**:每实例独立的热段根(`prod` 在升级主机类用 hugetlbfs,`sim` 与 dev2 的两个实例用 tmpfs `/dev/shm/hft/<instance>`,mount 单元 `dev-shm-hft-<instance>.mount`,`hft-core@<instance>.service` 以 `RequiresMountsFor=` 依赖它)与独立的冷盘目录(`<cold_root>/<instance>/`)。tmpfs 大小:dev2 `sim` `size=224M`(Σ 热段 192 MiB + 心跳与余量)、dev2 `prod` `size=192M`(Σ 168 MiB + 余量)、`c16`/`c32` `sim` `size=1700M`(Σ 1,632 MiB + 余量);热段页计入写入者所在单元的内存。可选扩展:`sim` 冷盘放独立 EBS 卷(触发条件:`sim` 运行时 `prod` 的 `journal_persist_lag_ms` p99 上升 > 20%)。
- **tmpfs 空间检查**(启动步 3):`statvfs` 可用空间 + 本实例已存在的热段文件(`hft_journal::held_by_instance(...)`,崩溃后留存、会被步 4 复用或截断的段)≥ 本实例全部热段需求,否则退出 3;别的实例的段不计入"可复用"。
- **出口 IP**:`prod` 与 `sim` 同机时各用自己的出口 IP:`prod` 的交易所连接绑 ENI-B 上的私有 IP + EIP;`sim` 的全部交易所连接(行情与 REST 快照)绑 sim 出口地址——升级主机类上是 **ENI-C 的私有 IP + EIP**,dev2 上是唯一网卡的**第二个私有 IP + EIP**——不消耗 `prod`(dev2 上是旧系统)的 IP 级限额(建连数、REST 权重)。deploy 生成 `host.egress`(`/etc/hft/egress.toml`,`{prod = [...], sim = [...]}`,两个实例读同一文件;启用 ENI-B2 时另有 `prod_bulk = [...]`:ENI-B2 的地址,不进任何 key 的白名单,格式归 deploy);步 2 校验本实例全部 `src_ip`(`venues.<v>.feed.conns[].src_ip`、`venues.<v>.gw.src_ip`)都在自己的列表内(`prod` 实例的批量流连接——`md` 行情与 snap-fetch 的公共 REST——另可用 `prod_bulk`;`gw:` 与 `acct-fetch` 只能用 `prod`)且各列表互不相交,否则退出 2(`EgressIpOverlap`);hostprobe 核对 `prod` 的每个 IP 配在 ENI-B 上、`prod_bulk` 的配在 ENI-B2 上、`sim` 的每个 IP 配在 ENI-C(dev2:唯一网卡)上,且都已关联 EIP,否则退出 3。`prod` 的每 (venue, account) 出口 IP 规则见[多账户](#多账户)。

### dev2 护栏

dev2 同时跑旧系统:**旧系统优先**,hft 的全部进程受以下约束(dev2 上两个实例规则相同)。本节是护栏全部数值、联调窗口规则与退路的唯一归属;实现它们的单元与脚本归 [deploy](../../deploy/README.md)。数值全部**待标定(M3 前实测)**,标定结果只改本节与上两节的 dev2 列。

| 项 | 值 | 作用 |
|---|---|---|
| slice | `hft.slice` 父级上限、子 slice 划分与根级 `hftbatch.slice` 见[主机资源隔离](#主机资源隔离slice核与出口-ip);kill-switch、守护与机外看门狗在 `system.slice` | 内存与 CPU 有界,批处理不挤占 hft-core |
| 单元 `Nice` 与调度 | 冷进程 10;批处理 19,在根级 `hftbatch.slice`(`CPUWeight=idle`、不设 `CPUQuota`);hft-core 线程按线程计划 | 批处理只用空闲 CPU,不制造旧系统的 CPU 压力 |
| tmpfs | 每实例一个专用挂载(见上节),热段不挤占 `/dev/shm`;干净 Drain 后由 `hft-shm-release` 删除热段(见下) | 停机即释放 |
| 启动前提 | 护栏守护不在运行时 dev2 上的 `hft-core@*` 不启动;守护重启不连带停止或重启 hft-core(单元写法归 [deploy](../../deploy/README.md):启动前检查守护 active 并排在守护之后启动,不用 `Requisite=`/`Requires=`) | 无护栏不跑 |
| 正式账户 | dev2 上任何实例都不含 `is_prod = true` 的账户(启动步 2,`ProdAccountOnDev2`) | 护栏停 hft 不会危及正式资金 |

**不忙等**:dev2 线程计划没有 `Spin` 线程(加载校验第 7 条)。

**停机后释放热段**:dev2 的 `hft-core@.service` 带 `ExecStopPost=+…/hft-shm-release %i`。只有本实例**干净 Drain**(`HeartbeatBlock.state = Stopped`,即全部段已封存且 persist 已刷完)时,它删除 `/dev/shm/hft/<instance>/` 下的热段文件;崩溃、超时被杀或状态不是 `Stopped` 时保留(启动步 4 需要它们恢复)。判据只有 `state = Stopped` 这一项:`Stopped` 只在全部段封存、persist 刷完之后写出,systemd 的 `$SERVICE_RESULT` 不参与判定。下次启动没有 RAM 热段,journal 从冷副本恢复;冷副本末段已封存且以 Drain 快照结尾,不判 `tail_lost`(见[启动顺序](#启动顺序)步 4)。

**标志文件**(两个标志的生存期不同;目录与权限归 deploy):

| 标志 | 谁写 | 谁清除 | 作用 |
|---|---|---|---|
| `/var/lib/hft/guard/tripped`(持久目录,主机重启不消失) | 护栏守护在一级或二级动作时写 | 只由人以 `hft-dev2-guard clear` 清除(`[需授权]`,处置步骤归 [incident-response P](../../docs/runbooks/incident-response.md));守护自身、主机重启与自动重启 `sim` 都不清除它 | 全部批处理单元不启动,定时器到点也无法绕过 |
| `/run/hft/window/active`(tmpfs) | `hft-window start` | `hft-window stop`(满足第 2 条时);主机重启即消失,联调窗口随之结束 | 联调窗口内全部批处理单元不启动 |

dev2 上每个批处理单元(`hft-verify`、`hft-replay-verify`、`hft-journal-archiver`、`hft-pipeline`、`hft-treasury`)的 `ExecCondition=` 依次检查:两个标志都不存在;`hft-core@prod` 不处于 active;另加[批处理的并发规则](#内存与-journal-段按主机类唯一归属)。任一不满足即跳过本次启动。不用 `systemctl mask`(对装在 `/etc/systemd/system` 的单元无效)。

**联调窗口**(`prod` 测试网实例只在窗口内运行;dev2 不用 `Conflicts=`):窗口由脚本 `hft-window start|stop`(deploy)开关。

1. `start`:写 `/run/hft/window/active` → 停批处理定时器(`hft-verify.timer`、`hft-replay-verify.timer`)并停止正在运行的批处理单元 → 启动 `hft-reconciler` 与 `hft-core@prod`。
2. `stop`:停止 `hft-core@prod`(SIGTERM → Drain)与 `hft-reconciler` → 核对 `/dev/shm/hft/prod` 挂载的**已用量**回到只剩心跳块的水平(`statvfs`;不只看文件列表——已删除但仍被读者映射的段不释放页,读者在实例 `Stopped` 后须解除映射,kill-switch 的规则归 [kill-switch](../kill-switch/README.md))→ 删除窗口标志 → 启动常驻的批处理单元与两个定时器。已用量未回落(测试网实例在窗口内崩溃留下热段,或仍有映射)→ **保留窗口标志、不恢复批处理**,P1,提示再执行一次 `hft-window start`(幂等:标志已在、批处理已停,重新启动 `hft-reconciler` 与 `hft-core@prod`)让实例恢复并干净 Drain,之后重跑 `stop`;这样批处理恢复后 hft 合计不超过 dev2 整机表的峰值。窗口期间没有报告的回放日子由 `hft-replay-verify` 下一次运行补跑(规则归 [testing §6](../../docs/testing.md#6-ci-运行矩阵))。
3. 窗口内的 `hft.slice` 峰值按 dev2 整机表的"联调窗口"状态计。

**护栏守护**(`hft-dev2-guard.service`,在 `system.slice`,每 5 s 采样;实现、旧系统的单元名与所在 cgroup、状态文件路径在 [deploy](../../deploy/README.md) 的护栏配置)。**只有资源信号会停 hft**,而且 CPU 与内存压力只读**旧系统所在的 cgroup**(默认 `system.slice`;旧系统实际所在 cgroup 待核实(方法:`systemd-cgls` 只读))——hft 自己 slice 的节流与回收停顿计入整机 PSI 但不计入那里,所以整机 PSI 只告警。**因果判定**:停机类信号越过阈值时,只有 hft 自身用量明显(下表"hft 自身用量"一行成立)才执行一级/二级动作;否则压力来自旧系统或其他进程,停 hft 无益,只告警(P1,注明"hft 用量不明显,未动作"),不写 `tripped`、不计入退路 (b) 的次数。旧系统自身的状态也只告警。watchtower 的 `HostPressure` 规则用同一个 `MemAvailable` 阈值(规则归 [watchtower](../watchtower/README.md)):

| 信号 | 阈值(共机 dev2) | 持续 | 动作 | 专用主机(退路 (b),见下) |
|---|---|---|---|---|
| `MemAvailable` | < 600 MiB | 连续 3 次(15 s) | 停 hft(一级 → 二级) | < 300 MiB:停 hft |
| 旧系统所在 cgroup 的 `memory.pressure` `some avg10` | > 5% | 30 s | 停 hft(一级 → 二级) | 不启用 |
| 旧系统所在 cgroup 的 `cpu.pressure` `some avg10` | > 20%(基线待标定) | 60 s | 停 hft(一级 → 二级) | 不启用 |
| swap-in 速率(`/proc/vmstat` `pswpin` 增量) | > 1,000 页/min | 2 min | 停 hft(一级 → 二级) | 不启用 |
| hft 自身用量(因果判定,本身不触发动作) | `hft.slice` 或 `hftbatch.slice` 在该信号的持续窗口内 CPU 均值 > 0.25 核(`cpu.stat` `usage_usec` 增量),或 `memory.current` > 256 MiB;初值,待标定(方法:M3 标定演练与第一周运行中记录两个 slice 在常态、满载与旧系统高峰时的用量分布) | 同所判定的停机信号 | 上面四行停机信号只在本行成立时执行动作,否则只告警(P1) | 不启用(专用主机上没有旧系统) |
| 整机 PSI `memory some avg10` / `cpu some avg10` | > 10% / > 40% | 30 s / 60 s | **只告警**(P2,进日报) | 只告警 |
| swap 已用量增长 | 5 min 内 > 64 MiB | — | 只告警(P1) | 不启用 |
| 旧系统健康 | 其主单元非 active(含重启中),或其状态文件更新年龄 > 30 s | 30 s | **只告警**(P1),不停任何 hft 单元 | 不启用 |

动作:

1. **一级**(资源信号且因果判定成立):写 `/var/lib/hft/guard/tripped` → 停 `hftbatch.slice` 的单元——**最先**停 `hft-verify`,再停 `hft-replay-verify`,再停 `hft-journal-archiver`、`hft-pipeline`、`hft-treasury`;P1 告警(守护自己的 Telegram token,与 watchtower 同群)。联调窗口内批处理本来就停着,一级没有可停的单元,资源信号持续 30 s 即进入二级。
2. **二级**(一级后 30 s 资源信号仍在,且因果判定仍成立——一级停掉批处理后只看 `hft.slice`):先 `systemctl stop hft-core@prod`(测试网实例,若在运行)与 `hft-reconciler`(只在联调窗口运行,随测试网实例一起停),再 `systemctl stop hft-core@sim`(各自 SIGTERM → `Drain`,`TimeoutStopSec=20`),等待 `HeartbeatBlock.state = Stopped` 或超时;P1(dev2 不持正式账户,不升级为 P0)。干净 Drain 的实例由 `hft-shm-release` 释放热段。watchtower 与 ops-bot 不停(它们是告警与人工处置的入口,上限已计入 dev2 整机表);kill-switch 不停:测试网实例未干净退出时它照常撤单。
3. **只自动重启 `sim`,每 UTC 日至多一次**:二级动作停掉 `hft-core@sim` 之后,停机类资源信号全部连续 ≥ 30 min 低于阈值、本 UTC 日尚未自动重启过、且二级动作之后没有人手动启停过该单元(该单元最近一次状态变化就是二级动作的停止)→ 守护执行 `systemctl start hft-core@sim.service`(polkit 只为这一个单元增加 `start`),P1;同一 UTC 日第二次被二级动作停掉后,这次停机不再自动重启(此后跨过 UTC 0 点也不),P1 并等人工启动。守护从不启动 `hft-core@prod`、批处理单元或其定时器,也不清除 `tripped` 标志;`sim` 实例停机期间的门槛 ⑦ 计时规则归 [testing §8.1](../../docs/testing.md#81-模拟账户长跑的计时与证据)。
4. 旧系统健康信号只发 P1(注明"hft 未动作"):旧系统的单元重启或状态文件短暂过期不打断 hft 的连续运行;同时出现资源信号时按资源信号处理。
5. 每次触发(含因果判定不成立而只告警的)、自动重启与标志变化写 `/var/lib/hft/guard/events.jsonl`(时刻、信号值、两个 slice 的用量、动作)。

**退路**(两种用法共用一台另开的 `c7g.large`,约 $66/月,`[需授权]`):

- **(a) 批处理迁出**:M3 标定演练 ⑧(见下)失败——批处理满载时护栏动作,或旧系统所在 cgroup 的 PSI 上升——即提前启用。新主机只运行 `hft-verify`、`hft-replay-verify`(回放读 S3 上的 journal 与归档检查点,[replayer](../replayer/README.md))与只有 `files` source 的 `hft-journal-archiver`(把验证运行记录与回放报告传到 S3,安装归 [deploy](../../deploy/README.md));原 dev2 的主机清单改为 `verify = false`、`replay_verify = false`(不装 `hft-replay-verify`;清单字段归 deploy),其 `hftbatch.slice` 只剩 archiver、pipeline、treasury。新主机上没有旧系统与 hft-core,不装护栏。
- **(b) 整体迁出**:护栏一周内执行停机动作(一级或二级,因果判定成立的)≥ 2 次(只告警的触发不计),或旧系统的延迟或成交可测劣化(对比护栏启用前一周的旧系统日志与延迟分布)→ 新主机专给 hft。主机类仍为 `dev2`(线程计划、内存表与容量不变),护栏按上表"专用主机"列运行(主机上没有旧系统:不采集旧系统信号、swap-in 与旧系统 cgroup 压力);迁移 = 原 dev2 上两个实例干净 Drain、冷盘目录与 advisor 检查点拷到新主机后启动,原 dev2 上的 hft 单元停用。机外看门狗(`hft-offbox-watchdog`)留在原 dev2。

验收(护栏演练,记录进 [incident-response](../../docs/runbooks/incident-response.md)):

1. 在 `hft.slice` 内人为制造内存压力,使 `MemAvailable` 越过阈值 → 一级(`hft-verify` 最先)、二级动作依次完成,`tripped` 标志存在;旧系统不受影响(其主单元不重启、状态文件持续更新、所在 cgroup 的两项 PSI 不越过上表阈值)。
2. 把守护读取的旧系统状态文件指向一个停止更新的测试文件(不动旧系统的正式单元)→ 只有 P1 告警,两个 hft-core 实例持续运行、无 `Drain`。
3. `hft-window start` 后批处理单元全部停止,手工 `systemctl start hft-replay-verify` 被 `ExecCondition` 跳过,`hft.slice` 峰值 ≤ 1,376 MiB;`hft-window stop` 后 `/dev/shm/hft/prod` 的已用量只剩心跳块、标志已删除、批处理恢复后 hft 合计 ≤ 1,376 MiB;窗口内 `kill -9` 测试网实例后 `hft-window stop` → 标志保留、批处理不恢复、P1。
4. 常态与联调窗口两种状态满载 1 h,`MemAvailable` 最小值 ≥ 1,000 MiB。
5. 用 `stress-ng` 在 `system.slice` 外制造换页(期间 `hft-verify` 运行,使因果判定成立),swap-in 速率信号在 2 min 后触发一级动作。
6. 一级动作后 15 min 内没有任何批处理单元启动(`hft-verify.timer` 到点被 `tripped` 标志跳过);`hft-dev2-guard clear` 清除标志后定时器恢复;标志存在时重启主机,开机后批处理仍不启动。
7. 二级动作停掉 `sim` 后让资源信号回落:30 min 后 `hft-core@sim` 被自动启动一次;同日再触发二级 → 不再自动启动;守护全程没有启动 `hft-core@prod` 与任何批处理单元。
8. **M3 标定演练**(`[需授权]`,在生产主机 dev2 上做):先确认 `hftbatch.slice` 的 `cpu.idle = 1` 已生效(见[主机资源隔离](#主机资源隔离slice核与出口-ip)的批处理一条);`hft-verify` 与 `hft-replay-verify` 满载 10 min,护栏不动作、旧系统所在 cgroup 的 `cpu.pressure`/`memory.pressure` 不升(读 `/proc/pressure/cpu` 与该 cgroup 的 `cpu.stat`;"不升" = 两项 `some avg10` 在负载 10 min 内的均值不高于负载前 10 min 的均值 + 1 个百分点(待标定),且每个采样都低于上表阈值)。失败 → 启用退路 (a)。
9. hft 空载(只有 `sim` 常驻、无批处理)时,在 hft 两个 slice 之外用 `systemd-run --scope` 跑 `stress-ng` 制造 CPU 争用,使旧系统所在 cgroup 的 `cpu.pressure` 越过阈值 → 只有 P1("hft 用量不明显,未动作"),无 `tripped`、无停机,`events.jsonl` 有记录、不计入退路 (b)。

### 启动顺序

启动分两段:**串行段**(步 1–5)在主线程执行,完成后 sequencer 进入主循环;**循环内段**(步 6–8)由事件驱动。恢复与模式的规则归 [hft-sequencer](../../crates/hft-sequencer/README.md)(冷启动不改写期望模式,每个账户加 `Recovery` cap),闸门位的语义归 [hft-risk](../../crates/hft-risk/README.md);本节只写装配顺序、失败处理与时间预算。每步开始与结束各写一条 `Note{StartupStep}`(步 1–3 的在 journal 打开后补写),`sd_notify(STATUS=…)` 同步更新。`StartupStep` 的 `v[0]` = 步号 1–8,`v[1]` = 0 开始 / 1 结束,`v[2]` = 进程启动以来的单调时钟 µs,`v[3]` = 结束记录的标志(开始记录为 0):bit 0 = 此刻已超过 `startup.budget_s`,bit 1 = 步 7 因 `startup.gate_timeout_s` 到期而结束、仍有账户闸门未通过,bit 2 = `tail_lost`;下表"超时 P1""超过即 P1"的告警按这些位产生。步 6 在每个 venue 的连接就绪或判 `Down`、或步 6 预算到期时结束;步 7 在全部账户闸门通过或 `startup.gate_timeout_s` 到期时结束;步 8 的两条在全部账户闸门首次通过时写出。

```
 Process start
   │ 1 load ──▶ 2 validate ──▶ 3 allocate ──▶ 4 journal recover ──▶ 5 state restore + EngineClock ──┐
   │   (exit 2)     (exit 2/3)      (exit 3)       (exit 4)              (exit 4;容量不足 exit 2)     │
   ▼                                                                                                ▼
                                  sd_notify(READY=1),sequencer 主循环开始
   6 connect + 身份核验(行情、Live 账户 gw 主 + 热备、snap-fetch、acct-fetch、sim-gw)──▶ 7 startup gates ──▶ 8 release
                     (每把 key 的 UID 不符 → Drain → exit 2)
```

| 步 | 做什么 | 失败时 | 预算 |
|---|---|---|---|
| 1 加载 | 读 TOML(`deny_unknown_fields`);`hft_rt::lock_all()`;读 `[blobs]`(refdata、`StrategyParams`、`RiskParams`、EngineConfig)并核对 SHA-256;从 `paths.secrets_dir` 读 **Live 账户**的全部交易 key(会话 key 与 REST key)后立即 `unlink`、`zeroize`(`sim` 实例不读任何 key);读上一次运行的启动守卫记录(见[崩溃隔离](#崩溃隔离与从快照恢复)) | 收集全部错误 → 退出 2 | 0.3 s |
| 2 校验 | [实例角色](#实例角色prod-与-sim)(账户类型、`is_prod` 与 `order_env` 一致、`feed_env` 与 `order_env` 组合、账户名前缀(`AccountMeta.label`)、dev2 无正式账户、key、任务与 `instance.name` 一致);refdata 自洽(含同一账户不得有同一交易所的两个 venue,`SameExchangeMarkets`)与装配校验 `hft_refdata::check_assembly`(码名沿用 `LoadErrCode`);`RefData::warnings()` 的每一项写一条 `Note{RefdataWarning}`(P1,与步 1–3 的 `StartupStep` 一起在 journal 打开后经 hk 环补写;`v[]` 归 hft-event),不阻断启动;参数与风控 blob 只引用存在的 group/inst/venue/account;策略集合与绑定([策略装配](#策略装配),含 `ProbeOnProdAccount`);Model 内存与检查点预算;ECF1 段 5 与本配置一致:`advisor_mem_budget_mib` = `advisor.mem_budget_mib`、`advisor_ckpt_max_mib` = `advisor.checkpoint_max_mib`(不等 → `ModelMemBudget`),`allow_missing_standby` = `startup.allow_missing_standby`;装配期交叉校验(含 `ExecRiskMismatch`、`DustBelowMinNotional`,见[内存与 journal 段](#内存与-journal-段按主机类唯一归属));每个 Live 账户在其交易的每个 venue 有全部凭证,Sim 账户没有任何凭证;Sim 账户的 `SimAccountCfg` 经 `hft_sim::SimExchange::new` 干校验;每个 `[venues.*]` 有对应 `VenueProtocol`;本实例所属激活组的发布清单 `RELEASE.json`(`paths.release_manifest`)的 `channel = "dev"` 而配置含 `is_prod = true` 账户 → 退出 2(`DevBuildWithProdAccount`);线程计划(含 `BboSharesWithMd`);出口 IP([主机资源隔离](#主机资源隔离slice核与出口-ip));`check_host` 与 hostprofile.json 比对 | 退出 2;主机不符退出 3 | 0.5 s |
| 3 分配 | tmpfs 空间检查([主机资源隔离](#主机资源隔离slice核与出口-ip));映射并预触摸:环、簿数组、OMS 槽、快照暂存区(升级主机类的 `prod` 用 1 GiB 页,其余普通页 + `mlockall`);内存预算检查([规则 3](#内存与-journal-段按主机类唯一归属));分配 `&'static [hft_rt::KillWord]`(每个 gw 一个 `CachePadded<AtomicU32>`),交给 `LiveDriver`;**kill 自检**(见下);创建 `HeartbeatBlock`;只在升级主机类的 `prod` 实例打开 `/dev/cpu_dma_latency` 写 0 并持有到退出(设备权限归 deploy;`dev2`、`c4` 与 `sim` 实例不改整机 PM QoS) | 大页、tmpfs 不足或超预算 → 退出 3 | 4 s |
| 4 journal 恢复 | 每个 journal(`seq`、各 `raw-feed-<venue>`、拆分时各 `raw-feed-<venue>-bbo`、各 `raw-gw-<venue>`)`JournalWriter::open`(截断到最后一个带 `EVENT_END` 的完整事件、补复制、校验段链);被截掉的半个事件的输入记 `Note{PartialEventDropped{seq, kind}}`(P1),其 `InputHash` 交毒丸判定;RAM 热段不存在时以冷副本为准:冷副本末段已封存且以 Drain 快照结尾(干净 Drain 后热段被释放)→ `tail_lost = false`,否则(整机重启等)`tail_lost = true`(判定由 [hft-journal](../../crates/hft-journal/README.md) 的恢复结果给出),并写 `Note{JournalTailLost}`(P1) | 链断裂或冷热冲突 → 退出 4 | 3 s |
| 5 状态恢复 | `Sequencer::boot` 用 `RecoverDriver`:从最新完整 `StateSnapshot` 起应用其后的输入(尾部长度的上界见表下"恢复尾部");重新产生的输出与日志不一致不中止恢复,写 `Note{RecoverMismatch}`(P0),处理规则(该 (venue, account) 全部置 Unknown、闸门严格化)归 hft-sequencer。版本或 schema 与 journal 不同时,只在末尾是 Drain 快照时允许,否则 `VersionNeedsDrain` 退出 4(回滚路径见下节)。恢复后的占用超过 EngineConfig 容量 → 退出 2(`CapacityShrink`);生效 refdata 中某账户的 `bound_instance` 与快照不同而快照中该账户有仓位、挂单或 `Unknown` → 退出 2(`BindingChangedWithPosition`:调 `RefData::check_rebind(快照中的 refdata, busy)`,`busy(a)` 按快照判定,规则归 [hft-refdata](../../crates/hft-refdata/README.md))。取 epoch:`e = max(快照 engine_epoch, DurableMeta engine_epoch) + 1`,**在任何订单发出之前** `write_sync`(换 AZ 从 S3 恢复冷副本时 DurableMeta 的来源、epoch 的安全间隔与恢复代归 [journal-archiver](../journal-archiver/README.md) 的恢复命令,本公式不变);非终态槽置 `Unknown`;每个账户加 `Recovery` cap;`config_source` 规则同前;写 `HostProfile` 与 `ConfigBlob{HostManifest}`;`sim` 实例按 [hft-sim 重启恢复](../../crates/hft-sim/README.md#重启恢复)由恢复后的状态对 sim-gw 的 `SimExchange` 调 `seed_account_from`(在步 6 启动 sim-gw 之前);最后由 live driver 在**任何其他 Input 之前**写恰好一条 `EngineClock`(引擎时间基准,字段与算法归 [hft-event](../../crates/hft-event/README.md#engineclock0x050ainput) 与 event-model),sequencer 处理它时写 `Note{EpochStart}`;`prod` 实例随即处理停机标记(见下) | 快照不可解 → 退出 4 | 同版本 ≤ 6 s;版本变化 ≤ 1 s |
| 6 连接与身份核验 | 启动全部行情/gw 线程(`sim` 实例没有 gw)、`conn-builder`、`snap-fetch`、`acct-fetch`、`sim-gw`、`advisor`;每 venue 的 BBO 赛马连接(条数归 hft-venue `feed.bbo_conns`)与 md 连接一条(成交、标记价、资金费;订阅分档深度时含 TopN),BBO 收到首帧或 BBO 补帧、分档深度收到首个快照后该 inst 可用;每 (venue, Live 账户) 下单连接 = 主 + 1 条已登录热备;**会话 key 的 UID**:`conn-builder` 登录后用该会话 key 查一次交易所 UID 并随交接带给 gw([hft-venue §7](../../crates/hft-venue/README.md#7-会话建立warm-线程-conn-buildersessiondriver));下单连接就绪后由 gw 经 `acct-fetch` 执行(分工归 hft-venue §4a):① 查询账户配置 → `AccountConfigReport`(含永续杠杆条目);② 用 REST key 查询交易所 UID;两把 key 的结果都并入该报告(`uid_keys`、`UID` 位,[hft-venue-proto 契约 23](../../crates/hft-venue-proto/README.md#实现方必须遵守的契约));③ 按 `VenueCaps.margin_data` 开始推送订阅或轮询 → `MarginReport`;OMS 刷新交易所死手;`Unknown` 槽由 gw 查单收敛;sim-gw 为每个 Sim 账户发 `ConnState{Up}` 与 `AccountConfigReport` | 单连接失败按 hft-venue 重试;预算内不齐 → 该 venue `Down`,P1;**身份不符**见下 | 8 s |
| 7 启动闸门 | 逐账户判定,结果写 `Note{StartupGate, account, bits}`(ops_bot `/status` 显示) | 超时未全过 → P1(`tail_lost` 时 P0) | 6 s |
| 8 放行 | hft-core 不改模式。**干净 Drain 重启**(同版本、尾部为空、无 `Unknown`/`Broken`)且未出现 `tail_lost`、`RecoverMismatch`、毒丸隔离或 `--recover-from-snapshot` 时,闸门通过即由 hft-risk 自动解除 `Recovery`(`auto_resume_after_clean_drain`)。**只含 Sim 账户的实例**(实例判定归本 app:refdata 中本实例全部账户都是 `Sim`,即 `sim` 实例——步 2 保证 `sim` 只含 Sim 账户、`prod` 不含)非干净重启后,闸门通过时按 [hft-risk](../../crates/hft-risk/README.md) 的 Sim 实例条件自动解除 `Recovery`,条件不满足时写 `Note{RecoveryHeldSim}`(P1)等人工。其余情况须交易员 `Resume{Recovery, account}`。停机标记写入的 Kill cap 不随 `Recovery` 解除,须交易员 `Resume{Kill}`(`/resume kill`) | — | — |
| **合计** | 进程启动 → 全部闸门通过 | | **< 30 s**(升级主机类);同版本约 28 s |

**恢复尾部**(步 5 预算的依据):重放从最近的完整快照起;关键帧按周期 `keyframe_period_ms` 或事件数 `keyframe_max_events` 先到者触发(触发、推迟与强制写的规则和取值归 [hft-sequencer](../../crates/hft-sequencer/README.md)),所以正常情况下尾部 ≤ `keyframe_max_events` 条输入。约束(本 app 的启动预算对 sequencer 参数的要求):`keyframe_max_events` ÷ 恢复回放吞吐下限(hft-sequencer 的 `seq/replay_throughput` 预算)≤ 5 s,加快照加载与 epoch 落盘仍在步 5 同版本 ≤ 6 s 之内,冷启动 < 30 s 不变;按现值约 4.5 s。改这两个值之一须同一改动核对本约束。持续没有空闲轮使关键帧推迟时尾部可超出上界(推迟上限归 hft-sequencer),此时步 5 超预算,只按 `startup.budget_s` 报 P1。

**kill 自检**(步 3,经真实的原子字,不在内存里模拟):对每个 Live 账户 a、服务它的每个 gw:① 用 hft-risk 的投影函数算出 `{Kill(Account a)}` 与 `{Kill(Account a), hard}` 的 `KillBits`(`u32`,位定义归 [hft-types](../../crates/hft-types/README.md)),以 `KillWord::store(Release)` 写进**该 gw 实际分配的字**;② 用 gw 运行时的同一函数 `hft_venue::kill_admit(word.load(Acquire), mask, acct_bits, ix, op, oflags)` 对合成的 Place(无/有 `RISK_REDUCING`)求值,结果须与 kill 放行表一致;③ 另把 32 个位逐位 store 后 load 回读,必须逐位相等(检出位宽截断);④ 结束后把每个字 store 回 0(步 5 之后 `LiveDriver` 按恢复的 kill 状态重写)。不发任何帧。启动路径把每个通过的账户经 hk 环送一条 `HkMsg::Sys{timer_id}`(`KillSelfTestOk`,编码归 hft-event),live driver 在 `EngineClock` 之后把它写成 Input `Timer{Sys, KillSelfTestOk}`(驱动 `IDENTITY` 闸门);同时经 hk 环写 Meta `Note{KillSelfTest}` 供人读。

**停机标记**(步 5,只在 `prod` 实例;`sim` 实例不读):kill-switch 的 `stop_core` 在 `systemctl stop` 之前写 `/var/lib/hft/kill-switch/stop-marker.json`(fsync;格式(含 `v`、`scope`)、写法与停机失败时作废为 `stop-marker.json.aborted` 的规则都归 [kill-switch](../kill-switch/README.md#停止-hft-corestop_core)),使"停"跨过下一次干净启动仍然有效:

1. `EngineClock` 之后、任何其他 Input 之前,live driver **只读** `stop-marker.json` 这一个文件名;作废的 `stop-marker.json.aborted` 与已消费的 `stop-marker.json.consumed` 一律不读、不改名。不存在 → 不写任何记录。
2. 存在 → 写一条 Input `ControlCmd{Kill{scope, hard = false, flatten = false}}`,主体 `cli:systemd`(权限表中这一项"仅启动时由停机标记触发的 Kill"归 [hft-ctl-proto 权限表](../../crates/hft-ctl-proto/README.md#权限表),命令语义归 [control-plane](../../docs/subsystems/control-plane.md))。`scope` 的解析:`"*"` → `Kill{Global}`;其他值按 refdata `AccountMeta.label` 找账户 a → `Kill{Account(a)}`;文件不可解析、`v ≠ 1` 或账户名找不到 → `Kill{Global}`,并在其前写 Meta `Note{StopMarkerCorrupt}`(P0;安全方向)。它与 ops_bot 的待补 Kill 是同一范围的同一命令,先后到达都幂等。
3. housekeeping 在 persist 水位越过这条 Input(已 `fdatasync` 到冷副本)后把文件改名为 `stop-marker.json.consumed`(覆盖上一份);改名之前崩溃 → 下次启动再写一条同样的 Kill。`hft-core@prod` 的运行用户在该目录的属组 `hft-stopmark` 内,可读标记并在目录内改名(用户、组与目录权限归 [deploy](../../deploy/README.md))。
4. 这个 Kill cap 写进快照,不随 `Recovery` 的自动解除而解除;交易员 `/resume kill`(`Resume{Kill}`)解除(规则归 hft-risk 与 control-plane,处置步骤归 [incident-response R](../../docs/runbooks/incident-response.md))。

**身份不符**(步 6 及之后每次会话重建;放行前与放行后同一处理,放行前的分支由本节规定,hft-risk 链接这里):`AccountConfigReport` 的 `UID` 位置位且未置 `UNREADABLE`(某把交易 key 对应的交易所账户不是 refdata 登记的 `venue_uid`)→ hft-risk 保持 `IDENTITY` 为 0、对该账户设 HARD 并要求以原因 `IdentityMismatch` Drain,sequencer 写 `Note{IdentityMismatch}`(P0,产出方与码由 hft-event 登记)并发起 Drain(撤本实例全部挂单);Drain 完成后 hft-core 以**退出码 2** 退出,systemd 不重启,人工核对 key 与 refdata。只是查不到(`UNREADABLE`、超时)→ 闸门保持关闭、P1,不退出。kill-switch 与 reconciler 的 key 由它们各自在启动时核对([kill-switch](../kill-switch/README.md)、[reconciler](../reconciler/README.md))。

**闸门位与它们的输入**(位定义、判定与 Sim 账户的不适用规则归 [hft-risk 启动闸门](../../crates/hft-risk/README.md);下表只写 hft-core 由谁产生输入):

| 位 | 输入来自 |
|---|---|
| `FEEDS` | 行情任务:该账户用到的 venue 行情 `ConnState{Up}`、订阅 inst 的 BBO 可用(订阅分档深度的 inst 另需簿 `Live`) |
| `ORDER_CONNS` | gw 任务:主连接已登录、热备 Up(`startup.allow_missing_standby` 时热备缺失只告警);Sim 账户:sim-gw 的 `ConnState{Up}` |
| `NO_UNKNOWN` | OMS 槽状态(gw 查单与 reconciler `UnknownResolved` 收敛) |
| `RECONCILED` | reconciler 在 `Note{EpochStart}` 之后对每个 (account, venue) 的报告;**严格**:该次对账全部 `Ignore`,或每项差异已经 `AdoptExchange`/`SetBalanceBaseline` 处置([reconciler](../reconciler/README.md#启动闸门与严格放行));资金费条目不进本闸门(只告警) |
| `CLOCK` | housekeeping 的 `ClockStatus`(`Synced` 且 offset ≤ `startup.max_clock_offset_us`) |
| `ACCOUNT_CFG` | 步 6 ① 的 `AccountConfigReport`(acct-fetch 解码、gw 推入 `rep-<venue>`):持仓模式、保证金模式、多资产、现货借币关闭与 `VenueCaps` 的 `required_*` 一致(杠杆条目的判定归 hft-risk);运行中变化 → hft-risk 设 cap |
| `IDENTITY` | `AccountConfigReport.uid_keys` 覆盖该 (venue, account) 的全部交易 key 且 `UID` 未置位 + 步 3 该账户的 kill 位自检通过(以 Input `Timer{Sys, KillSelfTestOk}` 进入 sequencer;Meta `Note{KillSelfTest}` 只供人读)。账户元数据 `uid_policy = "unverifiable"` 的测试网账户不核 UID,改由 kill 自检 + 一次成功签名的只读请求放行(字段归 [hft-refdata](../../crates/hft-refdata/README.md),判定归 hft-risk) |

闸门未放行期间,升级命令(含 `Resume{Recovery}`)被拒(`PreconditionFailed`);例外是带 `flatten` 的 `Kill`(只减仓,仍需确认;闸门未放行与 DRAINING 期间都允许,类别规则归 [control-plane](../../docs/subsystems/control-plane.md#类别))。闸门位与 cap 一起写进快照。

### 崩溃隔离与从快照恢复

**毒丸隔离分两层**(计数都在 `DurableMeta`,[hft-journal 持久元数据](../../crates/hft-journal/README.md)):

| 层 | 管什么 | 归属 |
|---|---|---|
| `boot_guard` | **恢复期**崩溃:步 5 重放同一区间 `(snapshot_seq, tail_end_seq]` 时反复崩溃 | 本 app(下面第 1–2 条) |
| `InputHash` 隔离 | **运行期**崩溃:处理某条输入时崩溃,该输入成为被截掉的半个事件,重启后又被重新生成(如同样内容的对账报告) | [hft-sequencer 毒丸输入隔离](../../crates/hft-sequencer/README.md);本 app 只把步 4 报告的 `InputHash` 与计数 `write_sync` 进 `DurableMeta`,并按它返回的 `RecoverPlan` 启动 |

1. hft-core 维护 `boot_guard = {snapshot_seq, tail_end_seq, attempts}`。步 5 调用 `RecoverDriver` 前:若本次要重放的区间与记录相同则 `attempts += 1`,否则置 1;`write_sync` 后才开始重放。步 5 成功结束后 `attempts = 0`。
2. 启动时 `attempts ≥ startup.poison_crash_limit`(默认 2)→ **不重放尾部**:从该快照起步(与版本变化同一路径),写 `Note{TailSkipped{from, to, reason = Poison}}`(P0),`tail_lost = true`(全部非终态槽 `Unknown`、`RECONCILED` 严格、禁用自动解除 `Recovery`)。被跳过的尾部留在旧段里,`replayer verify --from-seq <snapshot_seq>` 离线复现崩溃。
3. 运行期:同一 `InputHash` 连续崩溃达 `startup.poison_crash_limit` → 按 hft-sequencer 的规则**照常重放尾部**,并在本进程生命周期内隔离该致崩输入(只写 Meta `Note{InputQuarantined}`,P0)。两层都不触发时,systemd `StartLimitBurst=3/600 s` 是最后一道截停 → `ProcessDown` P0,人工处理。

**`--recover-from-snapshot <latest|SEQ> --reason <文本>`**(人工、一次性;版本回滚的唯一路径):

1. 适用:新版本崩溃后无法 Drain,旧二进制因 `VersionNeedsDrain` 起不来;或 journal 尾部被判定不可用。
2. 选择本二进制**能解码**的最新快照(`latest`,不可解的较新快照跳过、向更早回退)或 seq ≤ `SEQ` 的最新快照(不回退:它不可解 → 退出 4,`SnapshotUndecodable`)——回滚时通常是升级前旧版本写的 Drain 快照;没有可选的快照 → 退出 4(`NoDecodableSnapshot`)。
3. 跳过版本检查;新记录的 seq 接在 journal 末尾之后(段链不断),新段以标记 `RecoverFromSnapshot{snapshot_seq, skipped_from, skipped_to, reason, issuer}` 的引导块开头(`issuer` = `cli:systemd`,操作者写在 `--reason` 里),`replayer verify` 在此重新起步;`EngineClock.source = Snapshot`。
4. epoch 仍取 `max(快照, DurableMeta) + 1`(快照较旧也保证单调)。
5. 按 `tail_lost` 处理:全部非终态槽 `Unknown`、每账户 `Recovery` cap、`RECONCILED` 严格、不按干净 Drain 自动解除(只含 Sim 账户的实例按启动步 8 的 Sim 实例条件);写 `Note{TailSkipped{from, to, reason = Operator}}`(P0)。快照之后的仓位与余额变化由对账差异逐项 `AdoptExchange` 收敛(非策略资产用 `SetBalanceBaseline`)。
6. 传入方式:运维以单元 drop-in 或 `systemd-run` 一次性传参,成功启动后移除;操作步骤归 [incident-response](../../docs/runbooks/incident-response.md) 与 [release-and-deploy](../../docs/runbooks/release-and-deploy.md)。

### 优雅退出与 kill 语义

| 触发 | 行为 |
|---|---|
| `SIGTERM` / `SIGINT`(`systemctl stop`,含 dev2 护栏二级、`hft-window stop` 与 kill-switch 的 `stop_core`) | housekeeping 经 signalfd 收到 → 交给 ctl-gateway 线程以 `issuer = cli:systemd` 提交 `Drain{cancel_resting = shutdown.cancel_open_orders}`(与外部命令同一路径,被 journal 记录)→ sequencer 执行 Drain(等待条件归 [hft-sequencer](../../crates/hft-sequencer/README.md);`Unknown` 写进快照)→ 写快照 → `Done` → 停行情/gw/snap-fetch/acct-fetch/sim-gw/advisor、关闭连接 → 各 journal `seal_and_close` → persist 刷完 → `HeartbeatBlock.state = Stopped` → 按 `Done.cause` 映射的退出码退出([命令行与退出码](#命令行与退出码))。超过 `shutdown.drain_timeout_ms` → `Note{DrainTimeout}`,按当前状态封存后退出(退出码同上)。预算 ≤ 10 s,`TimeoutStopSec=20` |
| `SIGHUP` | 忽略。配置只经控制环命令改变 |
| `Drain{cancel_resting}`(ops_bot `/shutdown` 或运维 CLI;确认规则归 [control-plane](../../docs/subsystems/control-plane.md#类别)) | 同 SIGTERM 的 sequencer 部分;`Done` 后同样退出 0 |
| 崩溃(`panic=abort`、SIGKILL、OOM) | 无清理。RAM 热段在进程外存活;[kill-switch](../kill-switch/README.md) 在心跳停止后对全部 Live 账户撤单,有死手接口的 venue 另由交易所在倒计时到期后撤单;systemd 重启,走完整启动顺序(含毒丸守卫) |
| sequencer 停摆(主循环计数 `hb_stale_ms` 内不前进) | housekeeping 不再 `sd_notify(WATCHDOG=1)`;systemd `WatchdogSec` 到期发 SIGABRT;kill-switch 同时看到心跳停止 |
| `Kill{level ∈ Account\|Venue\|Global, hard, cancel_resting, flatten}` | 语义归 [control-plane](../../docs/subsystems/control-plane.md) 与 [hft-risk](../../crates/hft-risk/README.md);`hard` 可与 `Global` 或 `Account` 组合(按账户 HARD 只封该账户)。hft-core 只负责接线:步 3 分配的 `KillWord`(每 gw 一个 `CachePadded<AtomicU32>`,位定义 `KillBits` 在 [hft-types](../../crates/hft-types/README.md),含 `account(ix)` 与 `account_hard(ix)`),由 `LiveDriver` 按 hft-risk 的投影写入,gw 用 `kill_admit` 判定([hft-venue §4](../../crates/hft-venue/README.md#4-gwruntimepoll_once-的循环结构))。Sim 账户不占 kill 字位,kill 与 HARD 对它们与实盘一样经 hft-risk 的检查链生效(规则归 hft-risk 与 hft-sequencer) |

`shutdown.cancel_open_orders` **没有默认值**:它就是 SIGTERM 路径上 `Drain` 的 `cancel_resting` 参数,必须显式写在配置里。

### refdata 变更(Drain + 重启)

refdata 不做热替换(可选扩展:允许热替换的变更集,触发条件:Drain + 重启的中断成为运营负担)。参数(`StrategyParams`)与风控参数经控制环热替换;只有宇宙本身(新 inst、费率档、限速表、`VenueCaps`、`AccountMeta` 含 `SimAccountCfg`)变化才换 refdata:

1. Ops 在维护窗口人工发 `ReplaceRefdata{blob, version}`(需确认;预览列出各账户的挂单与 Live cell)。**要求 `instance.config_source = "journal"`**,否则 `PreconditionFailed`。新 blob 在进入 Drain **之前**分两处校验完,任一不通过 → 旧 refdata 继续生效,不 Drain、不退出:
   - **ctl-gateway 的装配校验**(启动步 2 的同一组函数,本 app 装配时经 hft-ctl `RefdataAssemblyCheck` 交给 gateway):新 refdata 与当前配置的组合——账户前缀与凭证、`SimExchange::new` 干校验、`DustBelowMinNotional`、线程计划覆盖全部 venue。不通过 → gateway 直接回 `Rejected{RefdataAssembly}`(arg = 失败项,码表归 [control-plane](../../docs/subsystems/control-plane.md#结果码)):命令不写 `ctl` 环、不进 sequencer,没有 `CmdResult`。
   - **sequencer 的复核**(装配校验通过后命令才进 sequencer):`RefData::load` 的跨版本兼容(已有 AssetId 的指数、已有 InstId 的合约乘数不得变化,容量不得缩小;有持仓的 inst 不得消失)与绑定变更(规则归 hft-sequencer、hft-refdata)。不通过 → `CmdResult{Rejected, detail = RefdataRecheck}`。
2. 校验通过 → 写 `ConfigBlob`,随即进入 `Drain{cancel_resting = false}`。
3. **Drain 末的二次复核**(本 app 负责):Drain 的等待条件满足、写快照之前,第 1 步的两处校验再做一次——Drain 期间未撤的挂单可能成交,使"绑定变更的账户无仓位"等条件不再成立:live driver 用待生效的 blob 跑装配校验(同一 `RefdataAssemblyCheck`),结果作为一条 `Timer{Sys, RefdataRecheck}` Input 交给 sequencer(编码归 [hft-event](../../crates/hft-event/README.md#定时器编码));sequencer 处理它时再跑自己的复核(`RefData::load`、绑定变更、订阅容量;纯函数)。记录名、形态与复核细节归 [hft-sequencer](../../crates/hft-sequencer/README.md)(Drain 一节),回放确定:
   - 通过 → 照常写快照,以退出码 **75** 退出,systemd 立即重启;
   - 不通过 → sequencer 撤销该 blob 的待生效引用(快照中的生效 refdata 仍是旧的)、写 `Note{RefdataRecheckFailed}`(P1,码归 hft-event),以 `Done{cause: Drain}` 结束,Drain 完成后以退出码 **0** 退出、systemd 不重启;人工启动(`[需授权]`)按旧 refdata 运行,处理差异后重新发 `ReplaceRefdata`。Drain 超时同样先做这次复核。
4. 重启走完整启动顺序;快照是 Drain 刚写的,尾部为空;冷启动 < 30 s;之后按启动步 8 放行。步骤见 [release-and-deploy](../../docs/runbooks/release-and-deploy.md)。

### 多账户

- 每个 `[[accounts]]` 是一个 `AccountId(u8)`。账户列表、账户名(`label`)、类型(`Live` | `Sim(SimAccountCfg)`)、`is_prod` 与绑定的策略实例(`bound_instance`)的**唯一归属**是 refdata 的 `AccountMeta`,每 (account, venue) 的交易所 UID(`venue_uid`)在 refdata 的 `AccountVenues`([hft-refdata](../../crates/hft-refdata/README.md));配置只按 `id`/`label` 引用并核对一致。**一个账户只绑定一个策略实例**;不存在多个系统或多个策略共用一个交易所账户的情形,账户上的全部挂单与仓位都属于该策略实例;一个交易所子账户(同一 `venue_uid`)只登记在一个 `AccountId` 下。账户名前缀规则见[实例角色](#实例角色prod-与-sim)。
- **一个账户在同一交易所只用一个市场**(现货或永续):账户的 venue 集合中不得有两个 `exchange` 相同的 VenueId,Sim 账户同样适用(refdata 加载失败 `SameExchangeMarkets`,规则归 [hft-refdata](../../crates/hft-refdata/README.md))。同所现货与永续共用钱包的情形因此不出现;跨所两腿由同一账户在两个交易所各用一个市场完成。
- **Live 账户**:交易所子账户——测试网/模拟盘账户(`is_prod = false`,dev2 联调与 M7 延迟验收)或新开的正式实盘子账户(`is_prod = true`,M8 起);每 venue 一组交易 key(无提现权限、IP 白名单;会话 key 与 REST key 各自核对 UID),独立 `RiskParams` 行、独立 kill、独立死手刷新;下单连接按 (venue, account) 建,一个 `gw-<venue>` 任务轮询该 venue 下所有 Live 账户的主/备连接;每 (venue, account) 固定出口 IP(`src_ip`,共享 IP 的配置加载时拒绝)。reconciler 用只读 key,kill-switch 用独立的撤单 key([python/AGENTS.md](../../python/AGENTS.md) 的持 key 划分)。
- **Sim 账户**:没有任何 key、死手与出口 IP;只在 `sim` 实例里;订单经 `out-sim` 进 sim-gw;其余(风控档、kill、模式、执行层)与 Live 相同。开通流程见 [launch](../../docs/runbooks/launch.md)。
- 行情连接按 venue 共享,不按账户复制。每账户的限速令牌桶在 gw 与风控各记一份(发送侧计数在 gw,准入在 sequencer)。
- client order id 的编解码归 [hft-types](../../crates/hft-types/README.md)、分配语义归 [hft-oms](../../crates/hft-oms/README.md)。

### HostProfile

两层检查,结果合成一条 `HostProfile` 事件(布局归 [hft-event](../../crates/hft-event/README.md))与一条 `ConfigBlob{HostManifest}`:

1. **进程内**(步 2):`hft_rt::check_host(root, hot, hk, need)` 得到 `HostCheck` 位(位定义与 hard 项归 [hft-rt](../../crates/hft-rt/README.md);`need` 为 `HostNeeds`,字段归 hft-rt,由装配按本页填入:本实例热段需求 `shm_bytes`、本实例已持有的段 `held`、warm 线程核集合 `warm`、交易网卡 IRQ 专核 `irq`(= `host.irq_cpus` 的 `eni_b`,启用 ENI-B2 时另含 `eni_b2`;`dev2`/`c4` 为空集))。任一 hard 位 → 退出 3(`rt.host.mode = prod`);非 hard 位写进 `HostProfile.host_checks`,由 watchtower 告警。
2. **部署侧**:`tools/hostprobe` 采集网卡、IRQ 落点(含 managed IRQ 的 `effective_affinity`)、`isolcpus`、sysctl(含收包模型的 busy poll 与 NAPI 参数)、chrony、出口 IP 与 EIP 关联、发布版本等实际值写 `/run/hft/hostprofile.json`([tools/hostprobe](../../tools/README.md#hostprobe);期望值由 [deploy](../../deploy/README.md) 按线程计划与 `host.irq_cpus` 生成;收包参数与 IRQ 落点是 hard 项,规则归 [hft-net](../../crates/hft-net/README.md))。hft-core 与 `host.expect_profile` 逐项比较:不符或 `pending_reboot = true` → 按 `host.on_mismatch` 处理;文件内容整体作为 `ConfigBlob{HostManifest}` 写入 journal。网卡 MAC 这类随实例变化的项只在运行期按主机本地清单核对,不进发布哈希(换 AZ 重建不需要先发版;规则归 deploy)。收包网卡的 NAPI 归属在建连时自检,见[主机资源隔离](#主机资源隔离slice核与出口-ip)。

`host.on_mismatch = "refuse"`(生产必须)→ 退出 3;`"degrade"`(只允许开发主机)→ 继续启动,写 `Note{HostDegraded}`,P1。

### HeartbeatBlock

进程外可见的存活信息,类型与布局归 [hft-rt](../../crates/hft-rt/README.md)(`hft_rt::HeartbeatBlock`)。hft-core 负责:

- 步 3 在 `paths.heartbeat_shm`(`/dev/shm/hft/<instance>/heartbeat`,`0640`,属主为本实例的运行用户、属组 `hft-journal-ro`;所在目录的建立者与属主归 [deploy](../../deploy/README.md) 的路径表——升级主机类的 `prod` 没有 tmpfs 挂载,同样要有人建立该目录)创建 4096 B 块:区 0 = sequencer 心跳(`&'static Heartbeat` 交给 `Sequencer::boot`);区 1 = housekeeping 的 `HkLine`;区 2..31 由装配期为每个线程 `claim` 一个槽。
- 状态字段:步 1 `Starting` → sequencer 主循环开始 `Warmup` → 全部账户闸门通过 `Running` → 收到 `Drain` `Draining` → 干净退出 `Stopped`(dev2 的 `hft-shm-release` 以它判定能否释放热段)。
- 判活规则(housekeeping 的 `sd_notify` 与 kill-switch 相同):状态为 `Warmup`/`Running`/`Draining` 且计数在 `watchdog.hb_stale_ms` 内前进;`Stopped` 表示干净退出。

### 配置文件字段

`/etc/hft/hft-core-<instance>.toml`,`schema = 1`。"必填"字段缺失即退出 2。

| 键 | 类型 | 含义 | 默认 / 来源 |
|---|---|---|---|
| `schema` | u32 | 配置 schema 版本 | 必填 |
| `instance.name` | `prod`\|`sim` | 实例角色兼实例名,出现在 journal 与冷副本路径、心跳块路径、archiver 分区、S3 前缀;其他值退出 2([实例角色](#实例角色prod-与-sim)) | 必填 |
| `instance.order_env` | `prod`\|`testnet` | `prod` 实例的下单端点与账户环境:`testnet` → 全部账户 `is_prod = false`、端点取 refdata `Endpoint{env ∈ Testnet\|Demo}`;`prod` → 全部 `is_prod = true`、只允许 `c8`/`c16`/`c32`/`metal`;`sim` 实例出现即退出 2 | `prod` 实例必填 |
| `instance.feed_env` | `prod`\|`testnet` | 行情端点环境;`sim` 实例只能是 `prod`;`testnet` 只允许与 `order_env = "testnet"` 组合,用于 dev2 联调窗口(门槛 ⑥ 的配置)与测试网行情时段的 `pair` 模式小样本确认(只记录,[testing §8.2](../../docs/testing.md#82-延迟验收的测法));门槛 ⑩ 的测量窗口用 `prod` | `prod` |
| `instance.config_source` | `journal`\|`file` | 恢复时 refdata/参数取 journal 最新 `ConfigBlob` 还是 `[blobs]` | 必填 |
| `paths.journal_ram` / `paths.journal_ram_raw` | path | `seq` 热段根 / `raw-*` 热段根;tmpfs 主机类与 `sim` 实例为 `/dev/shm/hft/<instance>`(专用挂载) | 必填;deploy 创建 |
| `paths.journal_cold` | path | 冷副本根(按实例分目录);卷容量见[冷盘容量](#内存与-journal-段按主机类唯一归属) | 必填 |
| `paths.release_manifest` | path | 本实例所属激活组的发布清单(读 `channel`、版本、制品哈希;没有 `/opt/hft/current`,组链接归 [deploy](../../deploy/README.md#单元组与分组激活)) | `prod`:`/opt/hft/current-core/RELEASE.json`;`sim`:`/opt/hft/current-sim/RELEASE.json` |
| `paths.ctl_socket` / `paths.heartbeat_shm` / `paths.secrets_dir` | path | ctl socket、心跳块、秘密文件目录(tmpfs,0400) | 必填 / 必填 / `prod` 必填,`sim` 出现即退出 2 |
| `paths.ctl_principals` | path | ctl 签发者表(uid → 主体、issuer → 角色;文件格式与加载归 [hft-ctl 鉴权](../../crates/hft-ctl/README.md#鉴权),主体表语义归 hft-ctl-proto);只在启动时加载并交给 ctl-gateway,改动须重启 hft-core;读不到或加载失败退出 2(`CtlPrincipals`) | `/etc/hft/ctl-principals.toml`(deploy 生成) |
| `paths.hostprofile` | path | hostprobe 的输出(本 app 只读;写者是 [tools/hostprobe](../../tools/README.md#hostprobe)) | `/run/hft/hostprofile.json` |
| `blobs.refdata` / `blobs.strategy_params` / `blobs.risk_params` / `blobs.engine` | `{path, sha256}` | 初始 blob:refdata、RiskParams、EngineConfig 由 [`tools/hft-cfg`](../../tools/README.md) 生成;`StrategyParams` 由 hft-cfg 按策略 schema 生成或由 [pipeline](../../python/pipeline/README.md) 产出 | 必填 |
| `host.class` | `dev2`\|`c4`\|`c8`\|`c16`\|`c32`\|`metal` | 主机类 | 必填 |
| `host.mem_budget_mib` | u32 | hft-core 全部内存(大页 + 普通页)预算;步 3 检查 | 必填(deploy 按[内存表](#内存与-journal-段按主机类唯一归属)生成) |
| `host.irq_cpus` | `{eni_a, eni_b, eni_c?, eni_b2?}` → 核号 | 网卡 IRQ 专核;生成器与 hostprobe 的期望值来源,加载校验第 6、12 条使用;`eni_b2` 只在 `c32` 启用 ENI-B2 时出现(= 28) | 升级主机类必填(deploy 按线程计划生成);`dev2`/`c4` 不出现 |
| `host.egress` | path | 两个实例共用的出口 IP 划分文件(`{prod = [...], sim = [...]}`;启用 ENI-B2 时另有 `prod_bulk = [...]`) | `/etc/hft/egress.toml`(deploy 生成) |
| `[[threads]]` | 数组 `{name, core, idle, nice?, tasks}` | 线程计划 | 必填(deploy 按主机类模板生成) |
| `rt.*` | 表 | 校准、PHC、`rt.host.mode` | 见 [hft-rt 配置项](../../crates/hft-rt/README.md) |
| `journal.<name>.segment_bytes` / `.hot_keep`、`journal.page` | u64 / u8 / `huge`\|`normal` | 键由 hft-journal 定义 | 取值按[内存表](#内存与-journal-段按主机类唯一归属);tmpfs 主机类与 `sim` 实例必须 `normal` |
| `venues.<v>.protocol` | enum | core:`binance-usdm`、`binance-spot`、`okx-swap`、`okx-spot`、`bitget-usdt-futures`、`bitget-spot`;可选扩展:`gate-futures`、`gate-spot`、`bybit-linear`、`bybit-spot`、`kucoin-spot`、`kucoin-futures`、`hyperliquid-perp` | 必填 |
| `venues.<v>.max_sub_insts` | u16 | 该 venue 订阅 inst 上限;Σ ≤ 容量 `max_insts`(装配期交叉校验);装配时写进该 venue 每个行情任务的 `FeedCfg.max_sub_insts` | 必填(deploy 生成:`dev2` 按线程计划,其他按容量表) |
| `venues.<v>.optional` | bool | true 时该 venue 起不来不阻塞其他 venue 的闸门 | 必填 |
| `venues.<v>.feed.conns` | [`{stream, src_ip}`] | 行情连接的出口地址:`stream ∈ bbo\|md`;`src_ip` 须在本实例的出口 IP 列表内(启用 ENI-B2 时 `md` 可用 `prod_bulk` 的地址)。snap-fetch 的公共 REST 连接没有单独的键:复用该 venue 第一条 `stream = md` 条目的 `src_ip`。BBO 赛马的条数与 md 连接的组织归 hft-venue(`feed.bbo_conns` 等) | 必填 |
| `venues.<v>.feed.*` / `venues.<v>.snap.*` / `venues.<v>.conn.*` / `venues.<v>.gw.*`(其余键) | — | hft-venue 的运行时配置(`feed.raw.bbo`/`feed.raw.md`、`feed.bbo_frames_per_poll`、`snap.parallel` 等;各 REST 连接数也在其中),键名、默认值与语义归 [hft-venue 配置项](../../crates/hft-venue/README.md#配置项) | raw 模式按[内存表](#内存与-journal-段按主机类唯一归属);其余按 hft-venue 默认 |
| `venues.<v>.gw.accounts` / `.gw.src_ip` | [string] / {account → ip} | 在该 venue 下单的 Live 账户(按 `AccountMeta.label`)与出口 IP(只能是 ENI-B 的地址);acct-fetch 对该账户的连接复用同一 `src_ip` | 有 Live 账户时必填 |
| `venues.<v>.gw.rest_endpoints` | [host] | gw REST 池连接的主机列表;请求以下标 `RestReqBuf.endpoint` 选择端点(语义归 [hft-venue](../../crates/hft-venue/README.md)),例:Binance USD-M 的身份查询走现货 REST 主机 | 该 `venue-*` 默认 |
| `venues.<v>.endpoint_override` / `test.extra_ca` | string / path | 只在 `order_env = "testnet"` 或 `feed_env = "testnet"` 的配置,以及 `sim` 实例(只改行情与 REST 快照端点;不持 key、不下单)出现(交易所测试网、可选的 mock-exchange 或 CI 的假交易所);每个生效的 `endpoint_override`(测试网配置与 `sim` 实例)在步 5 写一条 `Note{EndpointOverride}`(P1,`v[0]` = venue;码由 hft-event 登记),防止误把实例指向假行情;额外 CA 只在存在 `endpoint_override` 时加载;`order_env = "prod"` 的配置出现任一项即退出 2(`EndpointOverrideInProd`) | 无 |
| `[[accounts]].id` / `.label` | u8 / string | 必须与 refdata `AccountMeta.id`/`AccountMeta.label` 一致;账户名前缀规则见[实例角色](#实例角色prod-与-sim) | 必填 |
| `[[accounts]].keys` | {venue → {session, rest} 秘密名} | Live 账户必填(会话 key 与 REST key 是同一把的 venue 两项指向同一秘密);Sim 账户出现即退出 2 | — |
| `[[accounts]].deadman` | {venue → bool} | Live 账户是否启用交易所侧死手;能力来自 refdata `VenueCaps.dms` | Live 必填 |
| `sim.*` | 表 | `SimTuning`:`seed`、`consume_decay_ms`、`take_avail_bp`、`quiet_ms`、`report_interval_s`、`transfer_delay_ms`、`rate_limit`、`late_fee`、`maker_fill` | 语义与默认见 [hft-sim 配置](../../crates/hft-sim/README.md#配置);`sim` 实例必填 `seed`;`prod` 实例出现即退出 2 |
| `[[sim.latency_models]]` | 表数组 | `LatencyModel` 取值表:`{kind = "Fixed", entry_us, resp_us}` 或 `{kind = "Empirical", entry = [[q_bp, us], …], resp = [[q_bp, us], …]}`;`SimAccountCfg.latency_model` 是下标(出现顺序,从 0 起) | `sim` 实例至少一项;定义与校验归 [hft-sim](../../crates/hft-sim/README.md#配置) |
| `[[sim.margin_models]]` | 表数组 | `MarginModel` 取值表:`{mmr = "RefdataTiers" 或 { Flat = "0.005" }, leverage, liq_fee, liq_slippage_bp}`;`SimAccountCfg.margin_model` 是下标 | 同上 |
| `advisor.mem_budget_mib` | u32 | 本实例全部已绑定策略实例的 Model 内存预算([规则 5](#内存与-journal-段按主机类唯一归属));须等于 EngineConfig ECF1 段 5 的 `advisor_mem_budget_mib`,不等退出 2 | 必填(deploy 按内存表 advisor 一行生成) |
| `advisor.checkpoint_dir` | path | 模型检查点目录(冷盘) | `<journal_cold>/<instance>/advisor` |
| `advisor.model_checkpoint_ms` | u32 | 模型检查点周期(另在 Drain 时写一次) | 600 000 |
| `advisor.checkpoint_max_mib` | u32 | 本实例全部检查点合计磁盘上限(两代);须等于 ECF1 段 5 的 `advisor_ckpt_max_mib`,不等退出 2 | 1 024;`c16`/`c32` `sim` 4 096([检查点表](#内存与-journal-段按主机类唯一归属)) |
| `advisor.checkpoint_chunk_kib` | u32 | 每次 `poll_once` 写出的检查点块上限(`Model::snapshot_chunk` 的 `max_bytes`) | 1 024(待实测:单块 ≤ 1 ms) |
| `advisor.advisor_max_lag_ms` | u32 | `AdvisorLag` 的阈值(按引擎时间;语义与写法唯一定义在 [hft-event](../../crates/hft-event/README.md)) | 5 000 |
| `storage.persist_lag_block_ms` / `.persist_lag_drain_ms` | u32 | housekeeping 判定 persist 滞后进入 `Storage` 条件(禁开)/ 超阈值 Drain 的门限:滞后 ≥ 门限即进入该档(档位 1 / 2) | 1 000 / 10 000(待标定) |
| `storage.disk_free_block_pct` / `.disk_free_drain_pct` | u8 | 冷盘可用空间的两个门限:可用空间占卷容量的比例 < 门限即进入该档(档位 1 / 2);persist 滞后与可用空间都回到档位 1 门限之外时 `StorageOk` | 15 / 5(待标定) |
| `startup.budget_s` / `startup.gate_timeout_s` | u32 | 超过即 P1 / 步 7 等待上限 | 30 / 15 |
| `startup.allow_missing_standby` | bool | 热备缺失是否仍置 `ORDER_CONNS`;须等于 ECF1 段 5 的 `allow_missing_standby`(hft-cfg 按本键写入,闸门判定读段 5),不等退出 2 | 必填 |
| `startup.max_clock_offset_us` | u32 | 闸门 `CLOCK` | 500 |
| `startup.poison_crash_limit` | u8 | 同一恢复区间(`boot_guard`)或同一输入(`InputHash`)连续崩溃多少次后跳过尾部 / 隔离输入 | 2 |
| `shutdown.cancel_open_orders` | bool | SIGTERM 路径上 `Drain` 的 `cancel_resting` | 必填 |
| `shutdown.drain_timeout_ms` | u32 | 等 `Drain` 返回 `Done` 的上限 | 8 000 |
| `watchdog.hb_stale_ms` | u32 | 停摆判定窗口的唯一归属 | 1 000 |
| `host.on_mismatch` / `host.hot_cpus` / `host.hk_cpus` / `host.expect_profile` | — | 见 HostProfile;`hot_cpus` 须等于 `prod` 实例 `Spin` 线程核的集合(线程计划第 12 条),`sim` 实例读同一值以避开。`HostNeeds` 不是配置键,由装配按本页填入(见 HostProfile) | 必填 |
| `chain.enabled` / `chain.link` / `chain.session_timeout_ms` | bool / string / u32 | 可选扩展:接 chain-core 外部 venue(链上策略迁移时启用,[external-venue-protocol](../../docs/subsystems/external-venue-protocol.md)) | false / — / 300 |

每个 Live 账户的 `RiskParams` 行按 `AccountId` 对应(结构归 [hft-risk](../../crates/hft-risk/README.md)),配置里没有档名键。`auto_resume_after_clean_drain` 在 EngineConfig(归 hft-risk/hft-sequencer)。**EngineConfig**:环容量、每轮批量上限、每事件出环上限 K、OMS 槽数、ledger 容量、`MAX_SIM_ACCOUNTS` 内的 Sim 账户数等按主机类与实例给出,由 `tools/hft-cfg` 从模板生成,启动时校验与 `host.class`、`instance.name` 一致(不符退出 2)并写成 `ConfigBlob{Engine}`;容量按上节缩容流程改变。每轮上限、环优先级与关键帧周期 `keyframe_period_ms` 等 sequencer 参数在 EngineConfig 段 5,字段与默认值归 [hft-sequencer](../../crates/hft-sequencer/README.md);hft-cfg 写段 5 的 `advisor_mem_budget_mib`、`advisor_ckpt_max_mib` 与 `allow_missing_standby` 时取本页的内存表、检查点表与本配置的 `startup.allow_missing_standby`;启动步 2 核对三者与本配置一致。

**环容量**(ECF1 段 4 `rings` 的行格式与各主机类取值的唯一归属,hft-venue 与 hft-sequencer 的环只链接这里;`snap-*`、`*-bulk` 与 `acct-*` 的值待实测):

- **行格式**(草案,实现时以代码与编译期断言为准;行版本 1,只追加):每类环一行 `RingCapRow { kind: u8 /* RingCapKind,0 非法 */, _pad: [u8; 3], cap: u32 }`(8 B,`#[repr(C)]`、小端、无隐式填充)。同类的各 venue 实例(含拆分时的 `-bbo` 变体)共用一行,各自按 `cap` 分配。`cap` 的单位由类决定:"元素"为元素数(2 的幂,`hft_rt::spsc` 的要求),"字节"为 `ByteSpsc` 的字节容量。`RingCapKind`(u8)的码值是下表第二列,只追加、永不复用。
- **校验**(该段的 `EngineValidator` 由本 app 提供):本实例按线程计划要建的每类环恰好一行;缺行、重复行、未知 `kind`、元素环的 `cap` 不是 2 的幂、出现本实例不建的类(如 `sim` 实例的 `Rep`)→ EngineConfig 与本实例不符,退出 2。
- **不进段 4 的环**:conn-builder 与各持有连接的任务之间的交接环(`BuildReq`/`Handoff`/`RetireMsg`)由装配按该任务的连接号段大小取 2 的幂(号段归 [hft-venue §1](../../crates/hft-venue/README.md)),在途数受号段约束,不会满;可选扩展的 `chain-in`/`chain-out` 启用时追加码值与行(取值随该扩展立项定)。

| 环 | `RingCapKind` 码 | 单位 | 升级主机类 `prod` | `c16`/`c32` `sim` | `dev2` `prod` | `dev2` `sim` |
|---|---|---|---|---|---|---|
| `ctl` | 1 `Ctl` | 字节 | 256 KiB | 同左 | 同左 | 同左 |
| `hk` | 2 `Hk` | 元素 | 256 | 同左 | 同左 | 同左 |
| `feedctl-<venue>`(及拆分时的 `-bbo`) | 3 `FeedCtl` | 元素 | max(256, 2 × 各 venue `max_sub_insts` 的最大值),向上取 2 的幂(启动时每个订阅 inst 各发一条 `Subscribe`,规则归 hft-sequencer;装配期交叉校验) | 同左 | 同左 | 同左 |
| `bbo-<venue>` | 4 `Bbo` | 元素 | 65536 | 65536 | 8192 | 8192 |
| `md-<venue>` | 5 `Md` | 元素 | 65536 | 65536 | 8192 | 8192 |
| `md-<venue>-bulk`(分档深度快照;启用增量深度时含 `L2Batch` 与 REST 快照分片) | 6 `MdBulk` | 字节 | 4 MiB(增量深度启用时 16 MiB) | 4 MiB | 1 MiB | 1 MiB |
| `rep-<venue>` | 7 `Rep` | 元素 | 16384 | 无 | 4096 | 无 |
| `out-<venue>` | 8 `Out` | 元素 | 4096 | 无 | 4096 | 无 |
| `rep-<venue>-bulk`(推送型保证金报告与杠杆条目) | 9 `RepBulk` | 字节 | 1 MiB | 无 | 256 KiB | 无 |
| `acctreq-<venue>` | 10 `AcctReq` | 元素 | 64 | 无 | 16 | 无 |
| `acct-<venue>`(acct-fetch 交回 gw 的记录含原始响应体,单条 ≤ 容量 / 4;响应体上限 `gw.acct_body_max` 归 hft-venue) | 11 `Acct` | 字节 | 2 MiB | 无 | 2 MiB | 无 |
| `rep-sim` | 12 `RepSim` | 元素 | 无 | 16384 | 无 | 4096 |
| `out-sim` | 13 `OutSim` | 元素 | 无 | 4096 | 无 | 1024 |
| `rep-sim-bulk` | 14 `RepSimBulk` | 字节 | 无 | 1 MiB | 无 | 256 KiB |
| `advice` | 15 `Advice` | 字节 | 256 KiB | 同左 | 同左 | 同左 |
| `snapreq-<venue>`(及拆分时的 `-bbo`) | 16 `SnapReq` | 元素 | 64 | 64 | 16 | 16 |
| `snap-<venue>`(及拆分时的 `-bbo`) | 17 `Snap` | 字节 | 4 MiB | 4 MiB | 1 MiB | 1 MiB(增量深度启用时 2 MiB;现货深度快照的响应体大小待核实(方法:按目标档数抓取各所 REST 快照)) |

**`[sim]` 进 journal**:`sim` 实例启动步 5 把生效的 `[sim]` 全表(`SimTuning` 与两张模型表)以规范编码写成一条 `ConfigBlob`(与 `ConfigBlob{HostManifest}` 同一步写入;段首与其他生效配置一起重述),[replayer sim](../replayer/README.md#sim) 从 journal 取,不读当前配置文件(`replayer verify` 不需要它:Sim 账户的回报本身是 Input)。该 blob 的 `BlobKind` 编号由 [hft-event](../../crates/hft-event/README.md) 登记。

## 实现说明

输入适配器把行情（含 bulk）与订单回报环元素的接收来源原样传给 `InputSlot`，规则见 [事件模型](../../docs/subsystems/event-model.md#时间戳语义)。



<details><summary>实现要点</summary>

**启动状态机**(主线程串行段 + sequencer 循环内段):

```
Load ─▶ Validate ─▶ Allocate ─▶ JournalRecover ─▶ StateRestore(+EngineClock) ─▶ [READY=1] Warmup ─▶ Running
  │        │           │              │                │                                   │   ▲        │
  └─exit2──┴──exit2/3──┴──exit3───────┴─exit4──────────┴─exit4/2                           │   └gates───┘
                                                                                          ▼            ▼
                                                     (SIGTERM/Drain/IdentityMismatch/StorageCritical) Draining ─▶ Stopped(exit 0/2/3)
```

- `HbState` 的 `Warmup → Running` 表示"全部账户闸门已通过",不改变模式;运行中闸门条件失效由 hft-risk 降级,`HbState` 不回退。
- 主线程在创建线程前屏蔽 SIGTERM/SIGINT/SIGHUP。housekeeping 读 signalfd；主线程等待 sequencer 退出期间也轮询该 fd，以消费定向投递给主线程的信号。两处只设置 Drain 请求，由 ctl-gateway 写一条 `cli:systemd` 命令；SIGHUP 不重载配置。
- **hk 环只有一个写者**(环表归 hft-sequencer):步 3–5 由主线程(启动路径)写 `Boot` 源的记录(步 1–5 的 `Note{StartupStep}`、`Note{KillSelfTest}`、`Note{RefdataWarning}` 等)与 `HkMsg::Sys{timer_id}`(`KillSelfTestOk`);步 5 结束、`READY=1` 之前主线程把生产端交给 housekeeping 线程并启动它,此后只有 housekeeping 写 hk 环。步 6–8 的 `Note{StartupStep}` 由 live driver 在 sequencer 线程上判定并直接写入 journal(不经环);advisor 的 Note(如 `AdvisorLag`、`ModelCkptSkipped`)经 `advice` 环,不写 hk 环。
- housekeeping 调用 `HeartbeatWatch::check`、每 1 000 ms 一个校准节拍:恰好一条 `ClockStatus`(`(tsc, phc, calib_err_ns)` 校准对与该节拍 `DriftTracker::push` 的判定,字段归 hft-event),判定与上一节拍不同(含每次 `Stepped`)时另送一条时钟判定的 `HkMsg::Sys{timer_id}`(编码归 hft-event)、停机标记的改名,并按 `storage.*` 门限采样 persist 滞后与冷盘水位,状态变化经 `hk` 环送给 sequencer,由 `LiveDriver` 记为 `Timer{PersistLag|DiskLow|StorageOk}`(Input;cap 规则归 hft-risk)。persist 线程调用 `Persister::poll`、`SegmentPreparer::poll`(段预创建)与 `HotRetirer::poll`(旧段回收);热线程滚动段时经 `wake = None` 的环把 `RetiredSegment` 交给它(规则归 hft-journal)。
- advisor:每个已绑定且有 `Model` 的策略实例一个模型实例;尾随 seq journal(预映射段槽位);按上节分块写模型检查点(journal 之外的文件)。重启:读检查点,键中 `kind_id`、`LAYOUT_VERSION`、`schema.id`、`strategy_instance` 与当前一致,且 `last_seq` 之后的 journal 仍可读(热段或冷副本)→ `Model::restore`,从 `last_seq + 1` 重放 journal;重放中的 `SetParams` 照常经 `model_on_params` 应用,追上后若当前生效参数与 Model 最后应用的不同(如以 `config_source = file` 启动),再调用一次 `model_on_params`。当前代不可用时试 `.prev`;两代都不可用(缺失、键不符、CRC 错、`last_seq` 已不可读)→ 空 `Model` 起步,写 `Note{ModelCheckpointDiscarded}`。Advice 是 journal 的 Input,模型重建不影响确定性回放。advisor 落后按 `advisor.advisor_max_lag_ms` 写 `AdvisorLag`(口径归 hft-event)。
- **预热注入**:空 `Model` 的预热数据经控制命令 `InjectWarmup{instance, local_kind, chunk_no, last, payload ≤ 4096 B}` 进入(主体 Trader 或 pipeline 的策略插件;命令语义与权限归 [control-plane](../../docs/subsystems/control-plane.md),投递给 Model 的 `EventRef::Warmup` 归 [hft-sequencer](../../crates/hft-sequencer/README.md) 与 [hft-strategy-sdk](../../crates/hft-strategy-sdk/README.md))。它作为 Input 记入 journal,回放确定;hft-core 不另接线,命令经 ctl-gateway 进 `ctl` 环(256 KiB,每条 ≤ 4 KiB 载荷,逐条确认)。
- **交给 sequencer 与 hft-risk 的装配信息**(步 2 校验后随 `Sequencer::boot` 交接一次,运行中不变,不进快照;参数形状归 [hft-sequencer](../../crates/hft-sequencer/README.md)):① 实例角色(`prod` / `sim`,[实例角色](#实例角色prod-与-sim)),sequencer 据此拒绝 `prod` 上的 `SimFault`;② 每个 (Live 账户, venue) 的 `keys_required` = 本实例为该账户载入的交易 key 集合(`[[accounts]].keys` 的会话 key → `SESSION`(1)、REST key → `REST`(2);Sim 账户为 0),sequencer 在 `Gate(AccountCfg{hdr, keys_required})` 中原样填入,[hft-risk 启动闸门](../../crates/hft-risk/README.md#启动闸门) 的 `IDENTITY` 据此核对 `uid_keys`;③ `startup.allow_missing_standby`(步 2 已校验等于 ECF1 段 5),sequencer 以 `ModeCfg.allow_missing_standby` 交 hft-risk。
- sim-gw:`SimGwTask` 装配时取 refdata 中全部 Sim 账户的 `SimAccountCfg` 与 `[sim]` 全表(含两张模型表,组成 `hft_sim::SimSetup`;`SimSetup.tsc_hz_milli` 取本次运行的 TSC 频率);非首次启动按启动步 5 的 seed 规则恢复模拟真值;sequencer 按 `AccountMeta.kind` 把订单路由到 `out-sim`。
- 行情任务:hft-core 按任务名构造每个 venue 的 `FeedRuntime<P>`(`feed-bbo:` → `routes = Bbo`,`feed-md:` → `Md`,`feed:` → `All`;`FeedCfg.ready` = `hft_venue::ready_poll_on(feed.ready_poll, 所在线程的 idle, 该线程全部行情任务的计划连接数之和)`;拆分形态下 live driver 按 `hft_venue::feed_cmd_dest` 把 `FeedCmd` 推到一个或两个 `feedctl` 环),按 [hft-venue §1](../../crates/hft-venue/README.md) 的存在条件为该 venue 建 `snapreq-*`/`snap-*` 环(拆分时 BBO 补帧一对回送到 `feed-bbo`)、把对应的 `SnapFetchTask<P>` 挂进 `snap-fetch` 任务;有 Live 账户的 venue 另建 `acctreq-<venue>`/`acct-<venue>` 环并把该 venue 的 `AcctFetchTask<P>` 挂进 `acct-fetch` 任务。
- 秘密预取:默认 `instance.secrets_mode = "aws_secrets_manager"`，由 [`hft-secrets`](../../tools/README.md#hft-secrets) 写 tmpfs `secrets/`(0400)。显式 `local_demo` 仅允许 `prod` 角色 + `order_env=testnet` + 全部账户 `is_prod=false`，端点仍严格核对 demo；正式 Live 账户拒绝。root 源文件与准备流程归 [deploy](../../deploy/README.md#秘密与实例角色)。步 1 读取运行时副本后逐个 `unlink`。会话 key 交给 `conn-builder`，REST key 交给 gw 与 `acct-fetch`；hft-core 不链接 AWS SDK。
- 账户基线的可选 REST funding_schedule 在建 feed 前通过协议 seed_funding 装入上下文；第一条实时 Funding 输入携带周期并记录 journal。查询不可用保留未知周期，等待真实时间戳变化；不修改策略阈值或估造结算现金。具体协议与失败处理归 [warm 账户运行时](../../crates/hft-venue/README.md#4a-账户查询保证金轮询与配置核验)。
- 环与内存全部在步 3 分配;步 6 之后没有 `mmap`/`malloc`(advisor 的 `Model` 与 sim-gw 在各自 warm 线程按其规则使用堆,计入内存表对应行)。
- `LiveIo::out_free` 取各出环单生产端的 `free_hint`,保证后续对应数量的推送成功;Simultaneous 整对出环契约归 [sequencer](../../crates/hft-sequencer/README.md)。
- `venues.rs` 用 `enum VenueSet { BinanceUsdm(…), BinanceSpot(…), OkxSwap(…), OkxSpot(…), BitgetUsdtFutures(…), BitgetSpot(…), … }` 编译期分派;新增 venue = 加一个变体 + 配置 `protocol` 值([cookbook](../../docs/cookbook/adding-a-venue.md))。

**规划的源文件**

| 文件 | 职责 |
|---|---|
| `src/main.rs` | 参数解析、退出码映射(含 `Done.cause` 与 Drain 超时)、`--check`、`--recover-from-snapshot` |
| `src/config.rs` | TOML 类型(`deny_unknown_fields`)、收集全部错误的校验器(含实例角色、`order_env`/`feed_env`/`is_prod`、账户名前缀、dev2 无正式账户、`ExecRiskMismatch`、`DustBelowMinNotional`、`ProbeOnProdAccount`、advisor 预算与 `allow_missing_standby` 与 ECF1 段 5 一致) |
| `src/schemas.rs` | `schemas` 子命令(调 registry 的 `export_schemas()`) |
| `src/hostcheck.rs` | `check_host`、hostprofile.json、出口 IP 划分校验、`HostProfile` 与 `ConfigBlob{HostManifest}` |
| `src/budget.rs` | 内存预算求和与检查(含 Model 内存与检查点预算)、tmpfs 空间检查(扣除本实例已持有的段)、容量与恢复后占用比较 |
| `src/secrets.rs` | 读取并 `unlink` 秘密文件、`zeroize` |
| `src/layout.rs` | 线程计划解析与校验(含 gw 独占线程、`BboSharesWithMd`、IRQ 专核无线程、`sim` 不占热核、`hot_cpus` 与 `Spin` 核一致)、线程创建、绑核、命名 |
| `src/memory.rs` | 大页映射、预触摸、`mlockall`、`cpu_dma_latency` |
| `src/startup.rs` | 启动状态机、每步计时与 `Note`、`EngineClock` 写入、停机标记读取与 Kill 写入、`sd_notify(STATUS)` |
| `src/recovery.rs` | journal 恢复、`boot_guard`、`InputHash` 计数落盘、毒丸跳过、`--recover-from-snapshot`、快照恢复、`ReplaceRefdata` 的 Drain 末复核 |
| `src/gates.rs` | 闸门输入接线、kill 自检(真实 store/load) |
| `src/venues.rs` | `VenueSet` 枚举装配、行情任务的 `routes`、`snap-fetch` 与 `acct-fetch` 接线 |
| `src/sim.rs` | sim-gw 装配 |
| `src/advisor.rs` | advisor 装配、模型检查点分块读写 |
| `src/housekeeping.rs` | 心跳状态、`sd_notify(WATCHDOG=1)`、存储水位、signalfd |
| `src/conn_builder.rs`、`src/persist.rs` | 线程接线(persist 含段预创建与回收) |
| `src/chain_link.rs`(`chain` feature,可选扩展) | `ext-<chain>` 线程接线 |

</details>

## 延迟与资源契约

hft-core 自身不在 tick-to-trade 路径上增加代码;路径上的分段预算归各 crate,整体以 [latency-budget.md](../../docs/latency-budget.md) 为准。下表延迟目标是**统一升级主机**(默认 `c32`,`c16` 为有条件的候选)上的验收口径,**在 M7 验收**:`prod` 实例以实时行情(`feed_env = "prod"`)+ 测试网/模拟盘下单回路(`order_env = "testnet"`)运行,`sim` 实例同机常驻;`dev2`/`c4` 只记录直方图([latency-budget §7](../../docs/latency-budget.md#7-主机类与验收口径))。

| 指标 | 目标 | 测量 |
|---|---|---|
| 主机内 `tick_to_trade`、`rep_to_order`(`hedge_host` 等其余分段只记录) | 哪些分段判定、硬线(验收下限)与目标(基准形态)归 [testing §8.2](../../docs/testing.md#82-延迟验收的测法),数字归 [latency-budget §3](../../docs/latency-budget.md#3-cex-路径预算)(`kernel_rx` 按主机类给出);p99.9 只记录 | 一个 7 天主网行情窗口,`prod` 与 `sim` 同机:订单类分段来自 watchtower 直方图,`kernel_rx`/`ingest`/`feed_host` 来自 [`replayer latency`](../replayer/README.md#latency) 的按事件加权统计 |
| `sim` 同机对 `prod` 的干扰 | 数值与测法的唯一归属是 [testing §8.2](../../docs/testing.md#82-延迟验收的测法) | 同上 |
| 冷启动(进程启动 → 全部闸门通过) | < 30 s(升级主机类;步 5 的尾部上界见[启动顺序](#启动顺序)"恢复尾部") | `Note{StartupStep}` 求和([TESTS.md](TESTS.md) CORE-400);步 5 另有基准 `startup-cold`(CORE-267,数值归 [benchmarks](../../benchmarks/README.md)) |
| 优雅退出(SIGTERM → 退出) | ≤ 10 s | journal 中 `Drain` 与末条记录的时间差 |
| persist 滞后 | 按冷盘卷类型的预算,数值唯一归 [hft-journal](../../crates/hft-journal/README.md#延迟与资源契约)(升级主机类 `prod`) | `journal_persist_lag_ms` |
| advisor 检查点块 | 单块编码与写入 ≤ 1 ms | advisor `poll_once` 计时;基准 `core/advisor_ckpt_chunk` |
| sim-gw 落后 journal | 只记录、进日报(模拟不做延迟验收;`prod` 实例没有 sim-gw) | `sim_tail_lag_ms` |
| 热线程系统调用 | 只出现 [hot-path-rules T7](../../docs/hot-path-rules.md#线程t) 允许表中的调用(sequencer 为 0;`Spin` 线程不发唤醒系统调用;`SpinThenPark` 线程的 park 调用同样按该表) | bpftrace `raw_syscalls:sys_enter` 按 `[[threads]]` 的线程名 |
| warm 线程发现新数据的延迟 | ≤ `park_us`(升级主机类 warm 与 advisor 为 300 µs) | 快照请求、建连交接、账户查询的入环到取出时间差 |
| 运行期缺页 | 0(步 3 后,热线程) | `perf stat -e page-faults` 每线程 |
| 内存 | 按[内存表](#内存与-journal-段按主机类唯一归属);`dev2` 空载 1 h 两个实例常驻 ≤ 576 / 384 MiB、合计 CPU < 8%;dev2 常态与联调窗口满载时 `MemAvailable` ≥ 1,000 MiB | cgroup `memory.peak`、`cpu.stat`;`/proc/meminfo` |

## 测试与验收

用例与验收见 [TESTS.md](TESTS.md)。
跨组件测试 `tests/safety_scenarios.rs` 与 `tests/clock_skew.rs` 使用真实 sequencer、网关与进程内 TLS/WS；[C6、C15、FP-8 的覆盖边界](../../docs/tests/scenarios.md)保留在场景规格，不作为完整 W1 种子集验收。
进程级离线冒烟见 `tests/paper_process.rs`；可移植配置与控制环测试见 `tests/config.rs`。运行入口与当前可执行子集见 [config](config/README.md)。模拟网关在取出订单指令时生成 `SendReport{Written}`，并为纯模拟器的订单回报补当前 epoch，使 OMS 按 `(slot, gen, epoch)` 匹配。

## 依赖

- 内部:`hft-types`、`hft-event`、`hft-refdata`、`hft-rt`、`hft-journal`(启用 `writer` 特性)、`hft-net`、`hft-venue`、各 core `venue-*`、`hft-book`、`hft-oms`、`hft-risk`、`hft-ledger`、`hft-exec`、`hft-strategy-sdk`、`hft-strat-lib`、`hft-sequencer`、`hft-sim`(`task` feature)、`hft-ctl`(gateway 端)、`hft-strategy-registry`(app 依赖;经它引入已注册的 `strategies/*`)。可选扩展(`chain` feature):`hft-ext-proto` 与链上相关 crate。**不依赖** `hft-telemetry`。
- 外部:`serde` + `toml`(只在步 1)、`libc`、`sd-notify`(或手写 `sendmsg`)、`zeroize`、`mimalloc`。
- 禁止:`tokio` 及任何异步运行时、热线程里的 `tracing`、AWS SDK、`serde_json::Value`。`cargo xtask ci` 检查依赖闭包。

## 延伸阅读

- [architecture.md §2–§5](../../docs/architecture.md)(部署单元、线程、状态所有权)
- [01](../../design/01-总体架构.md)、[02](../../design/02-硬件与托管方案.md)、[08](../../design/08-数据面与回放.md)、[09](../../design/09-可观测性、风控与控制面.md)、[11](../../design/11-实施路线图.md)、[13 R2/R3/R7/O3](../../design/13-设计评审与优化.md)
- [现状测量与升级分析](../../design/现状测量与升级分析-2026-10-01.md)(dev2 实测、主机类选择、数据量)、[round8-decisions](../../design/_work/round8-decisions.md)
- [research/05 §3](../../design/research/05-hardware-hosting-os-tuning.md)(调优清单依据)
- [journal-format](../../docs/subsystems/journal-format.md)、[event-model](../../docs/subsystems/event-model.md)、[defensive-patterns](../../docs/defensive-patterns.md)
- Agent Note:`sim-only-instance`、`sim-colocated-with-prod`、`no-small-live`、`engine-time`、`warmup-injection`、`no-same-exchange-spot-perp`、`host-class-c32`、`rx-irq-driven`、`feed-split-and-snap-fetch`、`verify-on-server`、`m3-end-probe`、`irq-split-reserve`、`core-market-data-scope`、`manual-hold-and-stop-marker`、`dev2-guard-system-slice-psi`、[thread-plan-and-host-classes](../../.agents/notes/proposed/architecture/2026-10-01-thread-plan-and-host-classes.md)、[recover-driver](../../.agents/notes/proposed/architecture/2026-10-01-recover-driver.md)、[recovery-epoch-and-boot-rules](../../.agents/notes/proposed/architecture/2026-10-01-recovery-epoch-and-boot-rules.md)
- 运维:[host-tuning](../../docs/runbooks/host-tuning.md)、[release-and-deploy](../../docs/runbooks/release-and-deploy.md)、[incident-response](../../docs/runbooks/incident-response.md)、[launch](../../docs/runbooks/launch.md)

公共 BBO 补帧使用独立的 `RestPool<1>` 与 FeedCtx,沿用 venue 的行情源 IP;由 conn-builder warm 线程驱动 [PooledSnapFetchTask](../../crates/hft-venue/README.md#当前-bbo-补帧传输),经 snap 环回到 feed 单写者发布。

### 测试环境账户装配

双 venue 的永续/现货模板与 refdata 端点映射见 [可执行配置](config/README.md)。模板按已注册协议与 VenueId 校验，使用非正式账户。

网络模式只接受非正式 Live 账户与 testnet 账户端点。公共行情环境由 [可执行配置](config/README.md) 的 `instance.feed_env` 选择，私有链路保持 testnet。`venues.<name>.account_io` 显式指定 `timeout_ms`、`interval_ms`、`request_gap_ms`、`max_cancel_rounds`、`listen_keepalive_ms`。启动先串行核对交易规则、账户模式和身份，撤销遗留订单并查到空，再读取余额与永续持仓。余额与账户 unassigned 持仓进入第一份完整 StateSnapshot，不猜测 group。启动 Summary 只证明本次快照与空 OMS 一致；seq journal 在 bootstrap 完成后创建。

凭据由 `src/secrets.rs` 在实际启动的配置验证前读入并 unlink，同一普通文件的共享引用去重；原始输入用 `Zeroizing` 保存，交给连接组件后丢弃。语法解析失败不消费文件，`--check` 与 Sim 不读取文件；完整启动阶段 Note 与各持有组件的密钥生命周期验收仍待完成。

`bitget` 默认启用，可与 Binance 同进程装配。两种 key 分别签名核验；非正式 Unverifiable 账户要求相同 API key，仍需人工确认身份闸门。Ed25519 文件支持原始 32 字节、PKCS#8 DER/PEM。周期余额/持仓只校验；REST 失败发布配置 UNREADABLE 后按 interval 重试。RateOnly 真实结算只与合成历史比较，不重复入现金；窗口与滞后取 LedgerConfig。操作见 [测试网 runbook](RUNBOOK-testnet.md)。

账户入口按 State 中的 MarginItem 编码变长 MarginReport；超过单体容量时设置 ITEMS_TRUNCATED，未观察到的 ADL 值不填零。该编码由 CORE-169 验证。

网关账户显式接收 REST 凭证,与非敏感模板分开持有。网关装配将 rep-bulk 与 rep 配对交给 GatewayInput，并按 REST 端点创建 gateway RestPool；REST 连接由 conn-builder 交接。会话重认证后的 UID 证据绑定至本次 SessionDriver job。账户观察仍使用 warm AccountClient，有界 AcctProtocol 接入状态见 [hft-venue](../../crates/hft-venue/README.md#已知限制与后续工作)。

## 已知限制与后续工作

- 新 journal 启动记录步 1–8 的 `StartupStep`、累计预算与闸门超时标志；步 1–5 的 STATUS 在串行步骤开始时发送，记录在 journal 打开后补写。步 6–8 的固定容量 Meta 队列由 sequencer 独占，housekeeping 只观察阶段并更新 STATUS，不参与闸门决策；步骤记录经过正常 journal 提交路径。恢复的 `tail_lost` 标志、完整连接失败与主机矩阵仍待装配。

- 当前文件配置支持分别设置 seq、raw-feed 与 raw-gw 的段大小和热段保留，字段归 [可执行配置](config/README.md)。预算从实际 journal 列表及各自布局求和；tmpfs 空间检查作用于实例挂载点。完整引擎/advisor/大页预算矩阵仍未装配。

- 当前可执行子集是 Linux 上的新 journal 会话、文件配置、单任务线程、已注册的 `NoModel` 策略与 `--dry-run` 离线闭环；配置生成示例、测试网模板与命令见 [config](config/README.md)。Windows 支持 `--check`、`schemas`、`--version` 与配置测试；实际引擎要求 Linux tmpfs 和 Unix socket。
- `binance-spot`、`binance-usdm` 静态装配 `FeedRuntime`、`GwRuntime`、`SessionDriver`。`bitget-spot`、`bitget-usdt-futures` 通过 `bitget` Cargo feature 装配 UTA 协议，未启用时返回 `VenueNotBuilt`。单 venue 最多一个交易所账户；ECF1 必须要求两种 key 的身份闸门，app 不伪造账户配置、身份或对账成功。账户链路与限制见“测试环境账户装配”，完整恢复和正式权限核验仍未装配。
- 启动先取得冷盘实例目录的内核文件锁，锁文件不删除，进程死亡由内核释放所有权。持锁期间用 `max(snapshot_epoch, durable_epoch) + 1` 预约 epoch，`write_sync` 成功后才能建立账户连接；腐坏与溢出退出 4。当前新会话没有 snapshot epoch。启动前按实际 journal 列表检查 tmpfs 可用量与本实例持有量，创建工作线程前 `mlockall`，失败退出 3；完整引擎区预算仍待接线。
- 既有 seq journal 返回 `RecoveryRequired`(4)，不按空账户重启。`SetRefdata`、advisor、主机 profile 验证、完整资源预算与大页尚未装配。共享心跳块记录 sequencer 进度、每个线程的进度与进程状态；`Running` 跟随全部账户首次闸门放行，信号和控制命令的 Drain 都进入 `Draining`。housekeeping 独占 `READY`/`WATCHDOG` 通知；`READY` 等待全部工作线程的绑核与调度设置成功；sequencer 停摆达到配置时限后不续 watchdog。`Stopped` 只在 seq/raw 封存且 persist 刷完后发布。完整规格与上线门槛仍以本页其余部分为准。
- `--dry-run` 只用 Sim 账户和配置中的固定报价，不连接交易所；撮合由 `hft-sim::SimExchange` 执行。模拟任务沿 journal 前缀推进，在成功订单决定处等待 out-sim 交接；不使用后来行情处理先前决定。SIGINT/SIGTERM 与 `--run-for-ms` 经 ctl-gateway 的内部 Drain 命令停止开仓、按配置撤单；sequencer 完成后封 seq/raw 段，persist 最后落盘并退出。它是装配冒烟工具，不替代实时模拟账户的上线观察。

- **单机 SPOF**:`prod` 实例一台;停得安全靠死手 + kill-switch + 冷启动 < 30 s;整机失联时的机外告警与人工处置归 watchtower 的 `healthz`、dev2 上的机外看门狗与 incident-response。可选扩展:第二台 warm standby(人工切),在单机故障的停机时间成为收益瓶颈时再做。
- **整机掉电**丢 persist 滞后部分;靠交易所真值 + 严格对账收敛。
- **`sim` 与 `prod` 同机**:核、内存、出口 IP、网卡(ENI-C)与冷盘 I/O 带宽上限都隔离,但内存带宽、L3/系统级缓存与实例的网络总带宽仍共享;干扰的上界只能实测(M7,testing ⑩)。超标时先降 `hft-sim.slice` 的 `CPUQuota` 与 `sim` 订阅数,再考虑把 `sim` 迁到另一台主机(主机类改为只承载 `prod` 的档位)。
- **dev2 与旧系统共机**:护栏只能"停 hft 保旧系统",资源压力下不能保证两个 hft-core 实例在 dev2 上连续运行(旧系统自身的重启、或 hft 用量不明显时的压力都不会停 hft);`sim` 每 UTC 日至多自动重启一次,第二次停机要等人工;模拟账户长跑的中断记入日报。联调窗口内批处理停止,回放校验由下一次运行补跑、归档顺延。dev2 上的门槛 ⑦ 是缩小规模的,满规模在升级主机上补跑。护栏频繁触发或 M3 标定演练失败时按退路处理。
- **M7 的下单回路**:主网行情 + 测试网下单只用于延迟样本(订单类分段 `tick_to_trade`、`rep_to_order` 在主网行情负载下取样;`hedge_host` 只记录,样本来自只作小样本确认的 `pair` 模式,[testing §8.2](../../docs/testing.md#82-延迟验收的测法)),测试网盘口与价格带和主网不同,交易结果不作正确性证据(正确性证据来自 dev2 的门槛 ⑥ 与模拟账户 ⑦)。
- **core 行情不含增量深度**:首次上线只有 BBO、成交、标记价/指数价、资金费与可选的分档深度(TopN);需要全深度的策略启用可选扩展前,按内存表的增量预算与线程计划(`c16` 不支持)复核。
- **sim 实例的行情口径**:`sim` 实例有自己的行情连接与出口 IP,与 `prod` 同机但看到的行情时刻仍可能不同;模拟与实盘的校准以各自 journal 的口径为准([hft-sim 与正式实盘并行校准](../../crates/hft-sim/README.md#与正式实盘并行校准))。
- 冷启动不自动恢复(干净 Drain 重启与只含 Sim 账户的实例除外,见启动步 8):`prod` 实例夜间崩溃后要等交易员 `Resume{Recovery}`;这是有意的代价。
- 配置不支持热重载:改 `[venues]`/`[accounts]`/`[sim]`/核分配与 `paths.ctl_principals` 的内容必须重启。
- **毒丸守卫与 systemd 启动限制**:单元的 `StartLimitBurst=3`/`StartLimitIntervalSec=600` 计入原始那次启动;原始启动后 600 s 内发生的崩溃若使步 5 的重放反复崩溃,systemd 在守卫跳过尾部的那次启动之前截停单元(`ProcessDown` P0,人工按 `--recover-from-snapshot` 处置)。这是安全方向的代价:守卫的自动跳过尾部只在运行超过 600 s 之后的崩溃上生效。

### 开发备忘

<details><summary>给实现者</summary>

- **待核实**:各 core venue 按每把交易 key 查询账户 UID 与账户配置(持仓模式、保证金模式、多资产、现货借币、永续杠杆)的端点与字段——结果写进各 `venue-*` README 的"账户配置核验"(方法:官方文档 + 测试网/模拟盘实测)。没有 UID 查询接口的 venue 不能上线(`IDENTITY` 无法放行)。Binance 测试网(Demo Trading 端点)上能否按每把 key 读到 UID 待核实(方法与写法归 venue-binance);读不到时该账户按 `uid_policy = "unverifiable"` 放行(见闸门表 `IDENTITY` 一行)。
- **待核实(方法:M3 末满订阅探针)**:`c8g.4xlarge`/`c8g.8xlarge` 的 ENA 队列数、RSS 与 IRQ 布局、HW RX 时间戳与 PTP、每实例可挂的 ENI 数(ENI-C 与 ENI-B2);`c8g.8xlarge` 的物理核拓扑;探针按 `c16` 计划满订阅运行、另在 `c8g.8xlarge` 上按 `c32` 计划运行并做 ENI-B2 拆分 A/B 后选定统一升级主机档位(默认 `c32`),核对:`prod` 的隔离核计划放得下全部下单 venue;`feed-bbo-*` 在 `epoll_pwait(0)` 下的 `kernel_rx` 满足按主机类的预算;BBO 的 `nic_to_kernel` 在突发窗口内是否上升(决定 ENI-B2;HW 时间戳不可用时用 `c32` 节的替代判据);`sim` 在其非隔离核上 `sim_tail_lag_ms` p99 < 1 s;整机内存余量 ≥ 6 GiB;大页与内存的实际占用与内存表一致;关键帧一次写完的停顿与其后 1 ms 内的 `decide` p99、persist 滞后记录在案;同机干扰满足 testing ⑩。选档结果只改本页线程计划与内存表、hft-types `HostClass` 与 deploy 清单。M1.5 的增测项(压舱深度连接、IRQ 核空闲 A/B、`calib_err_ns` 等)归 [architecture-v2/11](../../design/11-实施路线图.md)。
- **待核实(方法见各项括号)**:c7i.metal-24xl 的 SNC 状态与核拓扑(`lscpu -e`、`numactl -H`);`/dev/cpu_dma_latency` 在 `idle=poll` 下是否仍需要;hugetlbfs 文件在进程崩溃后存活(`kill -9` 后检查);hugetlb 页确实不计入 `memory.max`(在 c8g 上 `memory.current` 与 `hugetlb.1GB.current` 对照);netdev netlink 的 NAPI → 网卡映射在 c8g 的内核上可用(见主机资源隔离的网卡一条)。
- **待核实(方法:按已登记策略的 `Model::snapshot_bytes` 求 `c16`/`c32` `sim` 一代检查点的大小,在 `c32` 上限速 30 MB/s 计时)**:Drain 时写的 advisor 检查点在 `hft-sim.slice` 的写带宽上限下能否在 `TimeoutStopSec` 内写完;写不完则这次 Drain 的检查点作废、下次用上一代,`sim` 在 ⑩ 窗口内每小时的干净启停也受影响。
- **待标定(M3 前)**:dev2 的 `spin_us`/`park_us`、`max_sub_insts`、段大小与快照大小、护栏全部阈值(含旧系统所在 cgroup 的 cpu/memory 压力基线、swap-in 速率与因果判定的 hft 自身用量阈值)、两个实例的 tmpfs 大小、冷盘 I/O 带宽上限、watchtower、archiver、ops_bot、pipeline 在 dev2 配置下的实际峰值;升级主机类的段大小、冷盘日量、`snap-*`/`*-bulk`/`acct-*` 环容量与 warm 线程 `park_us`(200–500 µs 之间)按目标订阅实算后回写本页。
- 实现顺序建议:config + `--check` + 预算 → memory/layout(含第 13 条与 IRQ 专核)→ 与 testkit 假交易所 / 交易所测试网跑通单 venue → recovery(冷副本路径 → RAM 热段路径 → 毒丸守卫 → `--recover-from-snapshot`)→ gates(含 ACCOUNT_CFG、IDENTITY、kill 自检)→ shutdown → sim-gw → advisor 分块检查点 → HostProfile 与出口 IP →(可选)ext-bsc。

</details>
