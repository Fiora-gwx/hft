---
component: hft-strategy-sdk
prefix: SDK
spec: README.md
---

# hft-strategy-sdk 测试规格

格式与规则见 [docs/tests](../../docs/tests/README.md)。

## 覆盖

| README 小节 | 用例 |
|---|---|
| 概要 | SDK-004、SDK-005 |
| 目录 | 不适用:非规范 |
| 使用本组件 | 不适用:见子节 |
| 依赖方向 | SDK-001–SDK-003、SDK-006–SDK-008、SDK-010 |
| 策略包的三部分 | SDK-011、SDK-013、SDK-014、SDK-058 |
| Strategy 与 Model | SDK-009、SDK-016–SDK-041、SDK-059–SDK-089、SDK-207、SDK-208 |
| 参数 schema 与 ParamSet | SDK-090–SDK-121 |
| 策略描述符:Note、报表、策略命令 | SDK-053、SDK-054、SDK-112–SDK-119 |
| Views 与 ModelViews | SDK-056、SDK-123–SDK-140、SDK-213、SDK-214 |
| 视图类型 | SDK-062、SDK-122、SDK-129–SDK-142、SDK-163、SDK-209、SDK-213、SDK-214 |
| EventRef | SDK-031、SDK-040–SDK-058、SDK-143、SDK-144、SDK-157 |
| Ctx 与订单意图 | SDK-116、SDK-141、SDK-142、SDK-145–SDK-157、SDK-212、SDK-216 |
| 目标意图 | SDK-141、SDK-158–SDK-163 |
| Advice 载荷 | SDK-052、SDK-067、SDK-086、SDK-171–SDK-176、SDK-211 |
| 预热注入 | SDK-055、SDK-180–SDK-184、SDK-210 |
| 装配:StrategySet 与 strategy_set! | SDK-002、SDK-087、SDK-185–SDK-189 |
| 特征导出 | SDK-215 |
| 运行模式下意图的行为表 | SDK-164–SDK-170 |
| 确定性规则 | SDK-011、SDK-013、SDK-015、SDK-029、SDK-079、SDK-128、SDK-176–SDK-179 |
| 版本与兼容规则 | SDK-008、SDK-022、SDK-057、SDK-076、SDK-077、SDK-096、SDK-171、SDK-190、SDK-210 |
| chain 特性(optional) | 不适用:里程碑外(`chain` 特性随链上策略接入时启用) |
| 测试桩(testing 特性) | SDK-063、SDK-078、SDK-115、SDK-127、SDK-129、SDK-150、SDK-192–SDK-195、SDK-213 |
| 实现说明 | 不适用:见子节 |
| Ctx 的内存与排空 | SDK-145、SDK-146、SDK-148、SDK-149、SDK-212 |
| 快照节 | SDK-014、SDK-020、SDK-024、SDK-025、SDK-070、SDK-071、SDK-191 |
| define_params! 的展开 | SDK-089、SDK-104 |
| 参数编解码器的使用方 | SDK-005、SDK-108、SDK-121 |
| 规划源文件 | SDK-012 |
| 延迟与资源契约 | SDK-013、SDK-196–SDK-206 |
| 测试与验收 | 不适用:非规范(用例与验收在本文件) |
| 依赖 | SDK-001、SDK-012 |
| 延伸阅读 | 不适用:非规范 |
| 已知限制与后续工作 | 不适用:非规范 |
| 开发备忘 | 不适用:非规范 |

## 用例

**共用夹具**(用例的 `前置` 引用这里的名字;用例依赖的参数值仍在 `前置` 里写出):

- **`TP` 测试 schema**(`crates/hft-strategy-sdk/tests/common/params.rs`,`define_params!`;M1 尚无该宏时为手写的同一 `ParamSchemaDesc` 常量;`schema_id = 0xFF01`,`version = 2`):

  | ix | 字段 | 单位 | min | max | default | 属性 | since |
  |---|---|---|---|---|---|---|---|
  | 0 | `open_th` | Rate | 0 | 100_000_000 | 5_000_000 | per_group、relax = Down、minor = RelPct(10) | 1 |
  | 1 | `cooldown_ms` | DurationMs | 0 | 600_000 | 20_000 | relax = Down、minor = Abs(1_000) | 1 |
  | 2 | `pct_bin` | Rate | 10_000 | 1_000_000 | 100_000 | per_group、structural | 1 |
  | 3 | `max_qty` | Qty | 0 | 1_000_000_000_000 | 0 | per_group、relax = Up、minor = Any | 1 |
  | 4 | `enabled` | Bool | 0 | 1 | 1 | manual_only、minor = Any | 1 |
  | 5 | `mode` | Enum(["a", "b", "c"]) | 0 | 2 | 0 | manual_only | 2 |

- **`GT3`**:3 个 group 的 `GroupTable`,`capacity() = 8`(`max_groups_per_instance = 8`);gix 0 = `G1`(`0x0300_0001`)、gix 1 = `G2`(`0x0300_0002`)、gix 2 = `G3`(`0x0300_0003`),都是 `asset = 1`(BTC,`px_exp = −2`、`qty_exp = −8`)、两腿:leg 0 = `I1`(inst 101,venue `V1` 现货,交易所 X1)、leg 1 = `I2`(inst 102,venue `V2` USDT 永续,交易所 X2),`LegDef.kind = Cex`、`route = 0`、`open_allowed = 1`。refdata 的 InstId 必须从 1 稠密编号,故另有填充 inst 1–100(V1 现货、资产 4,不属于任何 group);参考 inst(都是资产 1):`I3`(inst 103,venue `V3`,交易所 X3,不在账户内,本实例有 feed)、`I4`(inst 104,venue `V4`,交易所 X4,不在账户内,本实例无 feed)、`I5`(inst 105,venue `V2` 永续,另一资产 2)。
- **账户与实例**:`A1`(`AccountKind::Sim`,`is_prod = false`,venues {V1, V2},绑定实例 `S3` = `StrategyId(3)`);需要正式账户的用例另设 `A9`(`is_prod = true`,测试网端点)。
- **SDK 内的测试策略**(`crates/hft-strategy-sdk/tests/common/`,只用于本 crate 的单元、属性与分派测试):`EchoProbe`(kind_id 0xFF01,`DESC.version = 3`,参数 `TP`,只发订单意图;BBO 中价 × `open_th` 越过阈值时对该 group 发一张 `Open` IOC,冷却 `cooldown_ms` 按 `engine_ns` 计;`cmds` 见 SDK-160;Note 码 1 `EchoFire`(Info)、2 `EchoOverflow`(P2)、3 `EchoNoFeed`(P2))与 `TargetProbe`(kind_id 0xFF02,只发 `TargetIntent`,带 `ProbeModel`,`cmds` 为空)。两者经本 crate `tests/set_dispatch.rs` 的 `strategy_set!` 装配成 `ProbeSet`。
- **`ProbeModel`**(TargetProbe 的 Model):`LAYOUT_VERSION = 1`、`PATH_VERSION = 1`、`WARMUP = true`;原始样本节 = 实例级节 tag 1 `{n_ticks: U64 One}` + 每 group 节 tag 2 `{GroupId: u32, utc_ns: I64 Prefixed, mid: I64 Prefixed, pct_bin: I64 One, fp: U64 One}`(每 group 至多 256 个 1 s 样本;`pct_bin` 是样本的存储解释配置,SDK-082 据它报告 `PctConfig`;`fp` 是写出时的 group 指纹;满窗口一条记录 4 124 B);路径状态节 = `n: u32` + 每 group `{id: u32, _pad: u32, band: i64, band_upd_ns: i64, last_sent: u64}`,`band` 在 UTC 60 s 边界由样本重算;`path_settle_ms = 60_000`;`cold_start_ms(p, gix) = 100_000 × (gix + 1)`;`mem_bytes(p, n) = n × 8_192 + 65_536`;`snapshot_bytes(p, n) = 256 + n × 4_352`;每个模型 tick(`model_tick(1_000)`)对每个 group 发一条 `local_kind = 1` 的 Advice `{ver: u8 = 1, flags: u8, _pad: [u8; 6], band: i64}`(样本不足 → `flags` 的 NOT_READY 位 = 1、`band = 0`);预热载荷 `local_kind = 1`:`{ver: u8 = 1, _pad: [u8; 3], group: u32, n: u16, _pad2: [u8; 6], (utc_ns: i64, mid: i64) × n}`。`ColdModel` 与它相同但 `WARMUP = false`。
- **引擎侧契约夹具**(`crates/hft-sequencer/tests/sdk_contract/`,hft-sequencer 测试二进制 `seq` 的模块;计数分配器的 SDK-140 在独立测试目标 `sdk_alloc`):`SeqRig` = hft-sequencer 测试基线 B0(`crates/hft-sequencer/tests/common/`:真实 `Sequencer<TestSet>` + 实盘驱动 `LiveDriver` + 内存环境 `TestIo`;`steady()` 后启动闸门全部放行、cell 为 Live、BBO 连接健康),另加 inst 23(venue B 的 FILL 永续)。`RecProbe`/`RecModel` = 其 `TestSet`(手写 `impl StrategySet`,不调用 `strategy_set!`;种类 probe-taker 0xFF01、probe-target 0xFF02、probe-np 0xFF03(`non_prod_only`),参数字段 `win`(结构)、`thr`、`bad`(`validate` 拒绝非 0)、`mem`、`ckpt`,命令 op 1 `open`(字段 `qty` 1..=100)、op 2 `close`(`reduce_only`));它逐次记录投递(`log()`:实例、快速部分或 Model、`EventRef` 变体、gix、inst、`v.now()`、事件的 Debug 文本)与方法调用(`calls()`:`create`、`on_params`、`subscriptions`、`snapshot`、`restore`、`create_model`、`model_on_params`、`model_restore`、`model_restore_path`),订阅、意图、Advice 与失败注入由测试脚本与钩子给出;`TestSetV5` 是 `DESC.version = 5` 的同一集合。`AdvRig` = 真实 `Advisor<TestSet, MemCkpt, Ring>` 尾随同一 journal 的已提交部分(`MemCkpt` 是内存 `CheckpointStore`,`Ring` 是内存 advice 环;Advice 由测试搬进 sequencer 的 advice 环)。本节其余夹具名字在 `SeqRig` 中按下表落地(用例写 SDK 夹具名,数值按表换算):

  | 用例中的名字 | `SeqRig` 中 |
  |---|---|
  | S3(`RecProbe` 实例)、A1 | s1(`StrategyId 1`,probe-taker)、acct-1(`AccountId 1`,Live 测试网账户,venues {A, B}) |
  | S4、A2 | s2(`StrategyId 2`,probe-target)、acct-2 |
  | 模拟账户上的实例 | s9、sim-9(`AccountId 9`) |
  | V1、V2 | A(`BINANCE_SPOT`,现货)、B(`GATE_USDT_FUT`,USDT 永续) |
  | I1、I2 | a1(inst 11,BTCUSDT 现货,tick 0.01、lot 0.0001)、b1(inst 21,BTC 永续,tick 0.1、lot 0.001) |
  | I5(不在任何 group 的另一资产永续) | b2(inst 22,ETH 永续;用例需要它不在 group 内时 s1 只用 G1) |
  | `GT3` 的 G1、G2、G3 | s1 的 `0x0100_0001`(a1 + b1,BTC)、`0x0100_0002`(a2 + b2,ETH)、`0x0100_0003`(inst 14 + inst 23,FILL)。hft-sequencer 的 `InstOwned` 规则不允许同一实例的两个非 Shadow cell 引用同一 (account, inst),原写的三个 group 共用 I1/I2 无法同时生效 |
  | `TP` 的字段 | `TestSet` 的字段:非结构字段的改动(`cooldown_ms`、`open_th`、`max_qty`)用 `thr`,结构字段(`pct_bin`)用 `win` |
  | I3(他所、有 feed)、I4(无 feed) | 用例内另加的现货 venue(见 SDK-034、SDK-035) |

  `SeqRig` 的 EngineConfig:`max_groups_per_instance = 8`、`conn_liveness_ms = 2_000`、`max_timers = 512`,TSC 1 GHz(`tsc_per_ms = 1_000_000`)。
- 定点数写法:`Rate` 1e-9(1 bp = 100_000),`Notional` 1e-6 USDT,BTC 的 `Qty` 指数 −8、`Px` 指数 −2。

### SDK-001 违反依赖方向的依赖边使 `cargo xtask deps` 失败

- 规则:[依赖方向](README.md#依赖方向)、[依赖](README.md#依赖)
- 级别:table
- 里程碑:M2
- 前置:`tools/xtask/tests/fixtures/` 下的小型假仓库,`deps.toml` 与主仓库相同;每行在干净副本上只做该行的一处改动
- 实现:`tools/xtask/tests/deps_sdk.rs::sdk_001_dependency_direction`

| 行 | 改动 | 期望 |
|---|---|---|
| SDK-001.1 | 不改 | 退出 0 |
| SDK-001.2 | `crates/hft-strategy-sdk` 的 `[dependencies]` 加 `hft-oms` | 退出 1;报告边 `hft-strategy-sdk → hft-oms` |
| SDK-001.3 | SDK 的 `[dev-dependencies]` 加 `hft-sequencer` | 退出 1;报告该 dev 边 |
| SDK-001.4 | SDK 的 `[dependencies]` 加 `hft-journal` | 退出 1 |
| SDK-001.5 | SDK 的 `[dependencies]` 加 `tokio` | 退出 1(pure crate 闭包含 tokio) |
| SDK-001.6 | `strategies/x` 直接依赖 `hft-types`(L0 应经 SDK `prelude`) | 退出 1;报告边 `x → hft-types` |
| SDK-001.7 | `strategies/x` 依赖 `hft-oms` | 退出 1 |
| SDK-001.8 | `strategies/x` 依赖另一个策略 crate `strategies/y` | 退出 1 |
| SDK-001.9 | `crates/hft-sequencer` 的 `[dev-dependencies]` 加 `hft-strategy-registry` | 退出 1 |
| SDK-001.10 | `crates/hft-testkit` 依赖 `strategies/x` | 退出 1 |
| SDK-001.11 | `apps/hft-core` 直接依赖 `strategies/x`(不经 registry) | 退出 1 |
| SDK-001.12 | `strategies/x` 只依赖 `hft-strategy-sdk` 与 `hft-strat-lib` | 退出 0 |

### SDK-002 `strategy_set!` 只能在 registry、hft-testkit 与 SDK 自己的测试里调用

- 规则:[装配](README.md#装配strategyset-与-strategy_set)
- 级别:table
- 里程碑:M2
- 前置:同 SDK-001 的假仓库;每行在一个文件里加一次 `strategy_set!` 调用(列表只含 kind_id 0xFF00–0xFFFF 的测试策略)
- 实现:`tools/xtask/tests/deps_sdk.rs::sdk_002_strategy_set_call_sites`

| 行 | 调用位置 | 期望 |
|---|---|---|
| SDK-002.1 | `strategies/registry/src/lib.rs`(原有的一次) | 退出 0 |
| SDK-002.2 | `crates/hft-testkit/src/strategies.rs`(原有的一次) | 退出 0 |
| SDK-002.3 | `crates/hft-strategy-sdk/tests/set_dispatch.rs` | 退出 0 |
| SDK-002.4 | `apps/hft-core/src/main.rs` | 退出 1;报告文件与行号 |
| SDK-002.5 | `crates/hft-sequencer/tests/x.rs` | 退出 1 |
| SDK-002.6 | `strategies/x/src/lib.rs` | 退出 1 |

### SDK-003 prelude 重导出的 L0 类型与常量就是 L0 本身

- 规则:[依赖方向](README.md#依赖方向)
- 级别:unit
- 里程碑:M2
- 前置:无
- 步骤:
  1. 对 `prelude` 的 `Px`、`Qty`、`Rate`、`Notional`、`Tsc`、`InstId`、`VenueId`、`AccountId`、`GroupId`、`StrategyId`、`Side`、`Tif`、`GroupSpec`、`LegDef`、`Level`、`NoteLevel`、`RejectBits`、`RunMode` 逐个比较 `TypeId::of` 与 hft-types/hft-event 中的同名类型
  2. 比较常量 `GROUP_NONE`、`INST_NONE`、`INST_ALL`、`MAX_BBO_CONN_SLOTS` 与 hft-types 的同名常量
- 期望:
  - 步骤 1 每对 `TypeId` 相等(18 对)
  - 步骤 2 每对值相等;`GROUP_NONE == GroupId(0)`
- 实现:`crates/hft-strategy-sdk/tests/prelude.rs::sdk_003_prelude_is_l0`

### SDK-004 新增第二个测试策略时引擎 crate、app 与 SDK 的 diff 为零

- 规则:[概要](README.md#概要)、[依赖方向](README.md#依赖方向)、[装配](README.md#装配strategyset-与-strategy_set)
- 级别:proc
- 里程碑:M2
- 门槛:⑧
- 前置:干净的 `main` 工作区;`ci-probe` 由 `cargo xtask new-strategy ci-probe --kind-id 0xFF10 --with-model --with-pipeline` 的模板生成(scaffold-gate 内部步骤);探针 Model 声明 `mem_bytes` = 2 MiB/group;`sim` 实例测试配置把 `ci-probe` 绑定到一个 Sim 账户,`advisor.mem_budget_mib = 160`
- 步骤:
  1. `cargo xtask scaffold-gate`
  2. `git status --porcelain -- crates apps tools/hft-cfg python`
- 期望:
  - 步骤 1 退出 0;其中补丁副本构建的 `hft-core --check` 退出 0,`ci-probe` 的场景测试与 100 个种子的 DST 短集全部通过
  - 步骤 2 输出为空(含 `crates/hft-strategy-sdk/`、`crates/hft-testkit/`、`crates/hft-sequencer/`);全部生成物只在 `target/decoupling/` 下
- 实现:`tools/xtask/tests/scaffold_gate.rs::sdk_004_second_test_strategy_zero_engine_diff`

### SDK-005 新策略只经导出的 schema 被通用工具处理,工具二进制不重建

- 规则:[schema JSON 与 define_params!](README.md#schema-json-与-define_params)、[参数编解码器的使用方](README.md#参数编解码器的使用方)
- 级别:proc
- 里程碑:M2
- 门槛:⑧
- 前置:SDK-004 的补丁副本构建;`hft-cfg` 与 hft-py wheel 取自同一 `main` 构建(不含 `ci-probe`);一份 `ci-probe` 的参数 TOML(1 个 group,覆盖 1 个 per_group 字段)
- 步骤:
  1. 补丁副本 `hft-core schemas --out d/`
  2. `hft-cfg` 以 `d/` 为 schema 目录把 TOML 编码成 `SetParams` blob
  3. hft-py `params.load_schemas("d/")` 后 `encode` 同一 TOML
  4. 补丁副本的 ctl-gateway 以 PREVIEW 发送步骤 2 的 blob
- 期望:
  - 步骤 1 写出 `<kind_id>-ci-probe.schema.json`,`index.json` 含 `kind_id = 65296` 的一项
  - 步骤 2 退出 0;步骤 3 的 body 与步骤 2 的 body 逐字节相同
  - 步骤 4 得到 `CmdResult{preview}`,无错误 detail
- 实现:`tools/xtask/tests/scaffold_gate.rs::sdk_005_generic_tools_handle_new_schema`

### SDK-006 同一 PR 同时改 SDK 与策略被 `decoupling-gate` 拒绝

- 规则:[依赖方向](README.md#依赖方向)
- 级别:proc
- 里程碑:M2
- 门槛:⑧
- 前置:假 git 仓库;PR 无标签、无评审;base..head 的改动 = {`crates/hft-strategy-sdk/src/ctx.rs`, `strategies/latency-probe/src/fast/mod.rs`}
- 步骤:
  1. `cargo xtask decoupling-gate --base <base> --head <head> --pr 1`(评审列表由假 API 返回空)
- 期望:
  - 退出 1;输出同时列出 `crates/hft-strategy-sdk/src/ctx.rs`(引擎侧)与 `strategies/latency-probe/src/fast/mod.rs`(策略侧)
- 实现:`tools/xtask/tests/decoupling_gate.rs::sdk_006_sdk_and_strategy_same_pr_rejected`

### SDK-007 只改策略、registry 一行与 schema 金样的 PR 通过 `decoupling-gate`

- 规则:[依赖方向](README.md#依赖方向)
- 级别:proc
- 里程碑:M2
- 门槛:⑧
- 前置:同 SDK-006;改动 = {`strategies/foo/**`(12 个文件), `strategies/registry/src/lib.rs`(加一行 `foo::Foo,`), `strategies/registry/Cargo.toml`, `fixtures/strategy-schemas/7-foo.schema.json`, `fixtures/strategy-schemas/index.json`}
- 步骤:
  1. `cargo xtask decoupling-gate --base <base> --head <head> --pr 2`
- 期望:
  - 退出 0;摘要无豁免记录
- 实现:`tools/xtask/tests/decoupling_gate.rs::sdk_007_strategy_only_pr_passes`

### SDK-008 只改 SDK 的 PR 通过 `decoupling-gate`,SDK 改动与策略改动须分开提交

- 规则:[版本与兼容规则](README.md#版本与兼容规则)
- 级别:proc
- 里程碑:M2
- 前置:同 SDK-006;改动 = {`crates/hft-strategy-sdk/src/views.rs`, `crates/hft-strategy-sdk/README.md`, `crates/hft-strategy-sdk/TESTS.md`}
- 步骤:
  1. `cargo xtask decoupling-gate --base <base> --head <head> --pr 3`
- 期望:
  - 退出 0
- 实现:`tools/xtask/tests/decoupling_gate.rs::sdk_008_sdk_only_pr_passes`

### SDK-009 策略改动与 hft-core 容量表放在同一 PR 被拒

- 规则:[Strategy 与 Model](README.md#strategy-与-model)
- 级别:proc
- 里程碑:M2
- 前置:同 SDK-006;改动 = {`strategies/foo/src/params.rs`, `apps/hft-core/README.md`(容量表 `max_groups_per_instance` 64 → 96)}
- 步骤:
  1. `cargo xtask decoupling-gate --base <base> --head <head> --pr 4`
- 期望:
  - 退出 1;输出列出 `apps/hft-core/README.md`
- 实现:`tools/xtask/tests/decoupling_gate.rs::sdk_009_capacity_change_with_strategy_rejected`

### SDK-010 SDK 与引擎源码中出现策略族名被 `doc-check` 拒绝

- 规则:[依赖方向](README.md#依赖方向)
- 级别:table
- 里程碑:M2
- 前置:`tools/xtask/tests/fixtures/` 假仓库;每行只做该行一处改动
- 实现:`tools/xtask/tests/doc_check_names.rs::sdk_010_family_names_out_of_engine`

| 行 | 改动 | 期望 |
|---|---|---|
| SDK-010.1 | 不改 | 退出 0 |
| SDK-010.2 | `crates/hft-strategy-sdk/README.md` 正文加一句含 `portfolio-carry` 的话 | 退出 1;报告文件与行 |
| SDK-010.3 | `crates/hft-strategy-sdk/src/views.rs` 的注释含 `cdex` | 退出 1 |
| SDK-010.4 | `crates/hft-strategy-sdk/src/params.rs` 的字符串字面量含 `maker-taker` | 退出 1 |
| SDK-010.5 | `strategies/foo/src/lib.rs` 注释含 `portfolio-carry`(策略 crate 内) | 退出 0 |

### SDK-011 策略 crate 快速部分与 Model 的禁止项被机检拒绝

- 规则:[策略包的三部分](README.md#策略包的三部分)、[确定性规则](README.md#确定性规则)
- 级别:table
- 里程碑:M9
- 前置:`tools/xtask/tests/fixtures/strategy-x/` 是按 strategies/AGENTS.md 模块约定生成的最小策略 crate(`cargo clippy` 与 `cargo xtask hot-check` 在原样上退出 0);每行只做该行一处改动
- 实现:`tools/xtask/tests/hot_check_strategy.rs::sdk_011_fast_and_model_prohibitions`

| 行 | 改动 | 期望 |
|---|---|---|
| SDK-011.1 | 删除 `src/fast/mod.rs` 的 `#![deny(clippy::float_arithmetic)]` | `hot-check` 退出 1,指出该文件 |
| SDK-011.2 | `src/fast/` 中 `let r = q as f64 * 0.5;` | `clippy` 报 `float_arithmetic` 错误 |
| SDK-011.3 | `src/fast/` 调 `std::time::Instant::now()` | `clippy` 报 `disallowed_methods` |
| SDK-011.4 | `src/fast/` 迭代 `std::collections::HashMap` | `clippy` 报 `disallowed_types` 或 `disallowed_methods` |
| SDK-011.5 | `src/fast/` 出现 `&dyn Fn()` | `hot-check` 退出 1 |
| SDK-011.6 | `src/fast/` 写 `use crate::model::Probe;` | `hot-check` 退出 1(快速部分不得引用 `src/model/`) |
| SDK-011.7 | `src/fast/` 调 `hft_strategy_sdk::model_math::ln` | `hot-check` 退出 1 |
| SDK-011.8 | `src/model/` 调 `f64::powf` | `clippy` 报 `disallowed_methods`(超越函数只经 `model_math`) |
| SDK-011.9 | `src/model/` 调 `f64::mul_add` | `clippy` 报 `disallowed_methods` |
| SDK-011.10 | `src/model/` 调 `std::fs::read` | `clippy` 报 `disallowed_methods` |
| SDK-011.11 | `src/model/` 调 `std::thread::spawn` | `clippy` 报 `disallowed_methods` |
| SDK-011.12 | `src/model/` 迭代 `HashMap` | `clippy` 报 `disallowed_types` 或 `disallowed_methods` |
| SDK-011.13 | `src/model/` 调 `hft_strategy_sdk::model_math::ln` | 全部检查退出 0 |
| SDK-011.14 | `src/params.rs` 中 `let x = 1.0 * 2.0;` | `clippy` 报 `float_arithmetic` 错误 |

### SDK-012 SDK 自身的浮点只出现在 `model_math`

- 规则:[依赖](README.md#依赖)、[规划源文件](README.md#规划源文件)
- 级别:unit
- 里程碑:M9
- 前置:SDK crate 的副本
- 步骤:
  1. 原样运行 `cargo clippy -p hft-strategy-sdk` 与 `cargo xtask hot-check`
  2. 在 `src/ctx.rs` 加 `let _x = 1.0_f64 * 2.0;` 再运行步骤 1
  3. 在 `src/lib.rs` 去掉 `#![deny(clippy::float_arithmetic)]` 再运行 `hot-check`
- 期望:
  - 步骤 1 两项都退出 0;`#[allow(clippy::float_arithmetic)]` 在全 crate 只出现在 `src/model_math.rs`
  - 步骤 2 `clippy` 报 `float_arithmetic` 错误,位置 `src/ctx.rs`
  - 步骤 3 `hot-check` 退出 1
- 实现:`tools/xtask/tests/hot_check_strategy.rs::sdk_012_sdk_floats_only_in_model_math`

### SDK-013 SDK 热路径入口零分配

- 规则:[策略包的三部分](README.md#策略包的三部分)、[确定性规则](README.md#确定性规则)、[延迟与资源契约](README.md#延迟与资源契约)
- 级别:unit
- 里程碑:M2
- 前置:测试二进制以计数全局分配器运行;`GT3`;`CtxHarness::new(&EchoProbe::DESC)`;一条合法的 16 B `StrategyCmd` 载荷;一个合法的 88 B 参数 body
- 步骤:
  1. 计数器清零后,各调用 10 000 次:`Ctx::place`、`target`、`clear_target`、`cancel`、`amend`、`timer_at`、`cancel_timer`、`note`、`request_halt`、`outcome`(每 16 次调用换一个事件,不超出容量)、`CmdView::parse` + `val`、`BboView::quote_live`、`Now::ens_of`/`tsc_of`/`utc_of`、`GroupTable::gix_of`/`get`/`len`、`params::body_view`
- 期望:
  - 步骤 1 结束时分配次数 = 0、释放次数 = 0
- 实现:`crates/hft-strategy-sdk/tests/alloc.rs::sdk_013_hot_entries_zero_alloc`

### SDK-014 测试策略的 `on_event` 与 `snapshot` 对每个事件变体零分配

- 规则:[Strategy 与 Model](README.md#strategy-与-model)、[快照节](README.md#快照节)
- 级别:unit
- 里程碑:M2
- 前置:计数全局分配器;`EchoProbe`、`TargetProbe` 以 `TP` 默认值、`GT3` 构造(构造期的分配不计);`MockViews` 已设 I1、I2 的 BBO
- 步骤:
  1. 计数器清零;对每个非 chain 的 `EventRef` 变体(`testing::ev` 构造)各投递 1 000 次给两个策略
  2. 计数器清零;对两个策略各调 `snapshot` 100 次(writer 预先按 256 KiB 分配)
- 期望:
  - 步骤 1、2 结束时分配次数均为 0
- 实现:`crates/hft-strategy-sdk/tests/alloc.rs::sdk_014_probe_on_event_zero_alloc`

### SDK-015 快速部分的算术溢出只放弃该 group 的动作并写 Note,不 panic

- 规则:[确定性规则](README.md#确定性规则)
- 级别:unit
- 里程碑:M2
- 前置:`ci` 剖面(溢出检查开启);`EchoProbe`,`TP` 默认值,`GT3`;`MockViews` 中 I1 的 BBO = bid `Px(i64::MAX)`、ask `Px(i64::MAX)`(中价的 checked 加法溢出),I2 正常
- 步骤:
  1. 投递 `ev::bbo(I1, gix 0)` 与 `ev::bbo(I1, gix 1)`
- 期望:
  - 不 panic;`Recorded.places` 为空
  - `Recorded.notes` 恰两条:`code = 2`(EchoOverflow)、`level = P2`,`group` 分别为 G1、G2
- 实现:`crates/hft-strategy-sdk/tests/probe_fast.rs::sdk_015_overflow_drops_group_action`

### SDK-016 首个 `StrategyParams` 生效时依次调用 `create` 与 `subscriptions`

- 规则:[Strategy 与 Model](README.md#strategy-与-model)
- 级别:integ
- 里程碑:M2
- 前置:`SeqRig`;`max_groups_per_instance = 8`;refdata 已登记实例 S3 绑定 A1;S3 尚无参数
- 步骤:
  1. 发送 `SetParams{S3, A1, version = 1}`(`GT3`,`TP` 默认值)
- 期望:
  - `CmdResult{Applied}`
  - `RecProbe` 调用记录(按序)恰为 `create`(SDK `Strategy::new` 经 `StrategySet::create`)、`subscriptions`,命令交出前没有调用、该 `ControlCmd` 处理完时两者都已发生;`create` 收到的 `GroupTable` 满足 `len() = 3`、`capacity() = 8`;没有 `on_params`
- 实现:`crates/hft-sequencer/tests/sdk_contract/lifecycle.rs::sdk_016_first_params_create_then_subscribe`

### SDK-017 后续 `SetParams` 依次调用 `on_params` 与 `subscriptions`,不再 `new`

- 规则:[Strategy 与 Model](README.md#strategy-与-model)
- 级别:integ
- 里程碑:M2
- 前置:SDK-016 之后;`cooldown_ms` 由 20 000 改为 30 000
- 步骤:
  1. 发送 `SetParams{S3, A1, version = 2}`
- 期望:
  - `CmdResult{Applied}`;本 Input 内调用记录恰为 `on_params`、`subscriptions`;`on_params` 收到的参数 `defaults().cooldown_ms = 30_000`
- 实现:`crates/hft-sequencer/tests/sdk_contract/lifecycle.rs::sdk_017_later_params_on_params_then_subscribe`

### SDK-018 `on_params` 返回错误时旧参数继续生效

- 规则:[Strategy 与 Model](README.md#strategy-与-model)
- 级别:integ
- 里程碑:M2
- 前置:SDK-016 之后(version 1 生效,`cooldown_ms = 20_000`);`RecProbe` 脚本:下一次 `on_params` 返回 `Err(ParamError::Invalid{field_ix: 1, code: 9})`
- 步骤:
  1. 发送 `SetParams{S3, A1, version = 2, cooldown_ms = 30_000}`
  2. 投递一条 I1 的 Bbo
- 期望:
  - 步骤 1:`CmdResult{InvalidArgs, detail = ParamInvalid}`,`arg & 0xF = 1`(kind `Strategy`);本 Input 内没有 `subscriptions` 调用;S3 生效参数的 `version` 仍为 1
  - 步骤 2:S3 照常收到该 Bbo(生效参数仍是 version 1:`cooldown_ms`(`thr`)= 20_000)
- 实现:`crates/hft-sequencer/tests/sdk_contract/lifecycle.rs::sdk_018_on_params_err_keeps_old`

### SDK-019 gateway 拒绝的 `SetParams` 不调用策略的任何方法

- 规则:[Strategy 与 Model](README.md#strategy-与-model)、[schema JSON 与 define_params!](README.md#schema-json-与-define_params)
- 级别:integ
- 里程碑:M2
- 前置:SDK-016 之后;新 blob 把 `open_th` 默认值写成 100_000_001(越过 max)
- 步骤:
  1. 发送该 `SetParams`
- 期望:
  - `CmdResult{InvalidArgs, detail = ParamInvalid}`,`arg = 1 | 0 << 4`(kind `Strategy`、字段 0);`RecProbe` 调用记录在本命令前后相同
- 实现:`crates/hft-ctl/tests/e2e/sdk.rs::sdk_019_gateway_reject_no_strategy_call`

### SDK-020 关键帧中 `snapshot` 每实例调用一次,行头带 `desc_version`

- 规则:[Strategy 与 Model](README.md#strategy-与-model)、[快照节](README.md#快照节)
- 级别:integ
- 里程碑:M2
- 前置:SDK-016 之后;`RecProbe` 的 `DESC.version = 5`
- 步骤:
  1. 交出 `Timer{Sys, Snapshot}`
- 期望:
  - 本 Input 内 `snapshot` 恰调用 1 次;`StateSnapshot` 节 5 中 S3 的 `StrategyRowHdr.desc_version = 5`,其后的节体与 `snapshot` 写出的字节逐字节相同
- 实现:`crates/hft-sequencer/tests/sdk_contract/lifecycle.rs::sdk_020_keyframe_snapshot_once`

### SDK-021 从快照恢复时 `restore` 读到写出时的 `desc_version`

- 规则:[Strategy 与 Model](README.md#strategy-与-model)、[版本与兼容规则](README.md#版本与兼容规则)
- 级别:integ
- 里程碑:M2
- 前置:SDK-020 写出的快照(行头 `desc_version = 5`);以同一 journal 冷启动恢复
- 步骤:
  1. `RecoverDriver` 从该快照起步
- 期望:
  - `restore` 恰调用 1 次,`r.desc_version() = 5`;恢复后的 `snapshot` 字节与快照节体逐字节相同
- 实现:`crates/hft-sequencer/tests/sdk_contract/lifecycle.rs::sdk_021_restore_sees_desc_version`

### SDK-022 `restore` 接受当前与上一版本,其他版本返回 `VersionUnsupported`

- 规则:[版本与兼容规则](README.md#版本与兼容规则)
- 级别:table
- 里程碑:M2
- 前置:`EchoProbe`(`DESC.version = 3`);各行的 `SnapshotReader` 以给定的 `desc_version` 构造,节体为该版本布局的合法字节
- 实现:`crates/hft-strategy-sdk/tests/probe_fast.rs::sdk_022_restore_version_window`

| 行 | `desc_version` | 期望 |
|---|---|---|
| SDK-022.1 | 3 | `Ok(())` |
| SDK-022.2 | 2 | `Ok(())` |
| SDK-022.3 | 1 | `Err(RestoreError::VersionUnsupported)`,策略状态与调用前逐字节相同 |
| SDK-022.4 | 4 | `Err(RestoreError::VersionUnsupported)` |

### SDK-023 快速部分 `snapshot` → `restore` → `snapshot` 字节相同

- 规则:[Strategy 与 Model](README.md#strategy-与-model)、[快照节](README.md#快照节)
- 级别:prop
- 里程碑:M2
- 性质:任意事件脚本跑完后,`snapshot` 字节 B1;新实例 `restore(B1)` 后再 `snapshot` 得 B2;B1 == B2,且两实例对之后同一脚本的 `Recorded` 相同
- 生成:`EchoProbe`、`TargetProbe` 各自;`GT3` 的 1–3 个 group;0–500 个事件,从 Bbo/Order/Exec/Timer/Manual/Advice/Mode 中抽取
- 参考:逐字节比较
- 例数:PR 500;每夜 20 000
- 实现:`crates/hft-strategy-sdk/tests/probe_fast.rs::sdk_023_snapshot_roundtrip`

### SDK-024 最大 group 数下快速部分快照不超过 `strategy_snapshot_max`

- 规则:[快照节](README.md#快照节)
- 级别:unit
- 里程碑:M2
- 前置:`EchoProbe`、`TargetProbe`,group 数 = `capacity()` = 8,每个 group 状态写满(冷却与确认窗口都在进行中);`SnapshotWriter` 上限 262_144 B
- 步骤:
  1. 各调一次 `snapshot`
- 期望:
  - `SnapshotWriter::status()` 返回 `Ok(n)`,两者的 `n` ≤ 262_144
- 实现:`crates/hft-strategy-sdk/tests/snapshot.rs::sdk_024_max_groups_fit_snapshot_max`

### SDK-025 写超 `strategy_snapshot_max` 的快照报告 `SnapshotFull`

- 规则:[快照节](README.md#快照节)
- 级别:unit
- 里程碑:M1
- 前置:`SnapshotWriter` 上限 262_144 B
- 步骤:
  1. `put_bytes(&[0u8; 262_144])`
  2. 再 `put_u8(1)`
- 期望:
  - 步骤 1 后 `status() = Ok(262_144)`
  - 步骤 2 后 `status() = Err(SnapshotFull)`,writer 的字节数仍为 262_144(越界的那次及之后不写入)
- 实现:`crates/hft-strategy-sdk/tests/snapshot.rs::sdk_025_overflow_reports_snapshot_full`

### SDK-026 移除有仓位的 group 被拒,策略不收到 `on_params`

- 规则:[Strategy 与 Model](README.md#strategy-与-model)
- 级别:integ
- 里程碑:M2
- 前置:SDK-016 之后;G2 的 leg 0 有 0.01 净仓(`SeqRig` 中 G2 是 a2 + b2,即 0.01 ETH);G2 登记了一个 60 s 的策略定时器
- 步骤:
  1. 发送只含 G1、G3 的 `SetParams{version = 2}`
- 期望:
  - `CmdResult{PreconditionFailed, detail = GroupHasPosition}`;`RecProbe` 本命令期间无调用;S3 生效参数仍为 version 1、G2 仍在其中,G2 的定时器仍在定时器堆(节 6)中
- 实现:`crates/hft-sequencer/tests/sdk_contract/lifecycle.rs::sdk_026_remove_group_with_position_rejected`

### SDK-027 `GroupTable` 的长度、容量、下标与按 GroupId 查找

- 规则:[Strategy 与 Model](README.md#strategy-与-model)
- 级别:unit
- 里程碑:M2
- 前置:`GroupTable` 由 blob 顺序 [G3, G1, G2] 构造(gix 0 = G3、1 = G1、2 = G2),`capacity = 8`
- 步骤:
  1. 读 `len()`、`capacity()`、`get(1).id`
  2. `gix_of(G1)`、`gix_of(G2)`、`gix_of(G3)`、`gix_of(0x0300_0004)`、`gix_of(GROUP_NONE)`、`gix_of(GROUP_ALL)`
  3. 收集 `iter()`
- 期望:
  - 步骤 1:`3`、`8`、`G1`
  - 步骤 2:`Some(1)`、`Some(2)`、`Some(0)`、`None`、`None`、`None`
  - 步骤 3:`[(0, G3), (1, G1), (2, G2)]`(按 gix 升序)
- 实现:`crates/hft-strategy-sdk/tests/group.rs::sdk_027_group_table_lookup`

### SDK-028 `GroupSpec::fingerprint()` 的字节口径与金样值

- 规则:[Strategy 与 Model](README.md#strategy-与-model)
- 级别:table
- 里程碑:M2
- 前置:基准 group `B`:`id = G1`、`asset = 1`、`n_legs = 2`、leg 0 = (inst 101, venue 1, kind 1, route 0)、leg 1 = (inst 102, venue 2, kind 1, route 0)、`open_allowed = 1`、其余字节为 0
- 实现:`crates/hft-strategy-sdk/tests/group.rs::sdk_028_fingerprint_golden`

| 行 | 相对 `B` 的改动 | 期望 `fingerprint()` |
|---|---|---|
| SDK-028.1 | 不改 | `0x64DA_DF7E_A1B0_D2C2` |
| SDK-028.2 | `open_allowed = 0` | `0x64DA_DF7E_A1B0_D2C2`(不变) |
| SDK-028.3 | `id = G2` | `0x64DA_DF7E_A1B0_D2C2`(不变) |
| SDK-028.4 | `legs[3].inst = 999`(`n_legs` 仍为 2) | `0x64DA_DF7E_A1B0_D2C2`(不变) |
| SDK-028.5 | `_rsv = 7` | `0x64DA_DF7E_A1B0_D2C2`(不变) |
| SDK-028.6 | `asset = 2` | `0x5E26_12DA_2221_982F` |
| SDK-028.7 | `n_legs = 1`(只留 leg 0) | `0x86B0_1B84_81B0_D000` |
| SDK-028.8 | leg 1 `inst = 105` | `0x7EE5_476A_A264_BE25` |
| SDK-028.9 | leg 1 `venue = 3` | 与 `.1` 不同 |
| SDK-028.10 | leg 1 `kind = 2` | 与 `.1` 不同 |
| SDK-028.11 | leg 1 `route = 1` | 与 `.1` 不同 |

### SDK-029 跨参数版本以 GroupId 对齐状态,gix 变化不丢状态

- 规则:[Strategy 与 Model](README.md#strategy-与-model)、[参数变化时的 group 重置](README.md#参数变化时的-group-重置)、[确定性规则](README.md#确定性规则)
- 级别:unit
- 里程碑:M2
- 前置:`EchoProbe`,`TP` 的 `cooldown_ms = 20_000`;`GT3`;`MockViews` 的 `engine_ns = 1_000_000_000_000`;G3 在 gix 2 上刚触发一次(冷却到 `engine_ns + 20_000_000_000`)
- 步骤:
  1. `on_params(p, GroupTable[G3, G1](G2 移除), rd)`(G3 变为 gix 0)
  2. 在 `engine_ns + 10_000_000_000` 投递 I1 的 Bbo(满足触发条件)给 gix 0
  3. 在 `engine_ns + 20_000_000_000` 再投递一次
- 期望:
  - 步骤 1 返回 `Ok(())`
  - 步骤 2 后 `Recorded.places` 为空(G3 的冷却随 GroupId 保留)
  - 步骤 3 后恰一条 `place`,`group = G3`
- 实现:`crates/hft-strategy-sdk/tests/probe_fast.rs::sdk_029_state_follows_group_id`

### SDK-030 `NoModel` 的常量与声明全为 0,不写检查点

- 规则:[Strategy 与 Model](README.md#strategy-与-model)
- 级别:unit
- 里程碑:M2
- 前置:`NoModel<TP>`;`TP` 默认值;`n_groups = 8`
- 步骤:
  1. 读 `LAYOUT_VERSION`、`PATH_VERSION`、`WARMUP`、`LAYOUT.sections.len()`、`mem_bytes(p, 8)`、`snapshot_bytes(p, 8)`、`path_settle_ms(p)`、`cold_start_ms(p, 0)`
  2. `ModelHarness::snapshot_all(&m, 65_536)`
  3. 投递 `ev::bbo(I1, 0)` 与 `ev::warmup(1, 0, true, &[1])`
- 期望:
  - 步骤 1:`0`、`0`、`false`、`0`、`0`、`0`、`0`、`0`
  - 步骤 2:`Ckpt{path: [], body: [], n_chunks: 0}`
  - 步骤 3:`AdviceOut` 为空
- 实现:`crates/hft-strategy-sdk/tests/model.rs::sdk_030_no_model_is_empty`

### SDK-031 advisor 对新实例调用 `create_model`,随后每条订阅的记录调用 `model_on_event`

- 规则:[Strategy 与 Model](README.md#strategy-与-model)、[EventRef](README.md#eventref)
- 级别:integ
- 里程碑:M2
- 前置:`SeqRig` + `AdvRig`;`RecProbe.subscriptions` 对 I1 调 `market(I1, 0, BBO, Part::Model)`;无检查点
- 步骤:
  1. 发送 `SetParams{S3, A1, version = 1}`(`GT3`)
  2. 投递 3 条 I1 的 Bbo(seq q1、q2、q3)
- 期望:
  - `RecModel` 的第一个调用是 `new`(在该 `SetParams` 的记录上);随后恰 3 次 `on_event(Bbo, gix 0)`,`v.now().seq` 依次为 q1、q2、q3
  - `RecProbe`(快速部分)没有收到 Bbo
- 实现:`crates/hft-sequencer/tests/sdk_contract/advisor.rs::sdk_031_model_created_and_fed`

### SDK-032 `Model::on_params` 幂等:同一参数重复应用不改状态

- 规则:[Strategy 与 Model](README.md#strategy-与-model)
- 级别:unit
- 里程碑:M2
- 前置:`ProbeModel`,`GT3`,每个 group 已有 100 个样本
- 步骤:
  1. `on_params(p, GT3, rd)` 两次,每次后用 `ModelHarness::snapshot_all(.., 65_536)` 取字节
- 期望:
  - 两次都返回 `ModelParamEffect::None`;两次 `Ckpt` 的 `path` 与 `body` 都与调用前逐字节相同
- 实现:`crates/hft-strategy-sdk/tests/model.rs::sdk_032_model_on_params_idempotent`

### SDK-033 `market` 的同一 inst 被多个 group 订阅时按 gix 升序各投递一次

- 规则:[Strategy 与 Model](README.md#strategy-与-model)
- 级别:integ
- 里程碑:M2
- 前置:`SeqRig`;`RecProbe.subscriptions`:`market(I1, 2, BBO, Fast)`、`market(I1, 0, BBO, Fast)`(调用顺序 gix 2 在前)
- 步骤:
  1. 投递一条 I1 的 Bbo
- 期望:
  - `RecProbe.on_event` 恰两次,gix 依次为 0、2;两次的 `b` 逐字段相同
- 实现:`crates/hft-sequencer/tests/sdk_contract/subs.rs::sdk_033_multi_group_same_inst_gix_order`

### SDK-034 参考行情订阅:不在账户 venues 内但有 feed 的 inst 照常投递

- 规则:[Strategy 与 Model](README.md#strategy-与-model)
- 级别:integ
- 里程碑:M2
- 前置:`SeqRig`;本 hft-core 实例配置了 V3 的 feed(`SeqRig` 另加 V3 = `OKX_SPOT`、I3 = inst 24 与 V4 = `BYBIT_SPOT`、I4 = inst 25,都是 BTC 现货;feed = {A, B, V3});`RecProbe.subscriptions` 先 `rd.insts_of(asset 1)`,再对返回中 venue = V3 的 inst 调 `market(I3, 0, BBO, Fast)`
- 步骤:
  1. `SetParams` 生效
  2. 投递一条 I3 的 Bbo
- 期望:
  - `insts_of(BTC)` 返回 `[11, 13, 21, 24, 25]`(InstId 升序,含不在 A1 venues 内的 24、25);`market` 返回 `Ok(())`
  - 步骤 1 后出环一条 `FeedCmd::Subscribe{inst = 24}`(venue V3),`streams` 含 BBO
  - 步骤 2:`RecProbe.on_event(Bbo{inst = 24, gix = 0})` 一次
- 实现:`crates/hft-sequencer/tests/sdk_contract/subs.rs::sdk_034_reference_market_subscription`

### SDK-035 订阅本实例无 feed 的 venue 返回 `NoFeed`,`SetParams` 照常生效

- 规则:[Strategy 与 Model](README.md#strategy-与-model)
- 级别:integ
- 里程碑:M2
- 前置:`SeqRig`;V4 在本 hft-core 实例无 feed;`RecProbe.subscriptions` 调 `market(I4, 0, BBO, Fast)`;`subscriptions` 没有 `Ctx`,脚本在其后的第一个事件里对记下的 `Err` 写 `Note{code = 3, group = G1, inst = I4}`
- 步骤:
  1. 发送 `SetParams{version = 1}`
  2. 投递一条 I1 的 Bbo
- 期望:
  - 步骤 1:`market` 返回 `Err(SubsErr::NoFeed)`;`CmdResult{Applied}`;没有针对 inst 25 的 `FeedCmd::Subscribe`
  - 步骤 2:journal 中有一条 `Note{source = Strategy, code = 2 << 24 | 3, strategy = S3, group = G1, inst = 25}`
- 实现:`crates/hft-sequencer/tests/sdk_contract/subs.rs::sdk_035_no_feed_does_not_fail_params`

### SDK-036 订阅超出该 inst 的条目容量返回 `Full`,`SetParams` 照常生效

- 规则:[Strategy 与 Model](README.md#strategy-与-model)
- 级别:integ
- 里程碑:M2
- 前置:`SeqRig`;`max_subs_per_inst = 2`;订阅按派发顺序重建,先运行的实例先占条目:实例 S4(本用例为 s1,派发顺序在前)以其 G1 腿占 I1 的一个条目;本用例的 S3 为 s2(两个 group),其 `subscriptions` 依次 `market(I1, 0, BBO, Fast)`、`market(I1, 1, BBO, Fast)`;sim-9 无参数(I1 只被两个 group 引用,`SetParams` 的容量复核通过)
- 步骤:
  1. 发送 `SetParams{S3, version = 2}`
  2. 投递一条 I1 的 Bbo
- 期望:
  - 第一次 `market` 返回 `Ok(())`,第二次返回 `Err(SubsErr::Full)`;`CmdResult{Applied}`
  - 步骤 2:S3 只收到 gix 0 的一次;S4 照常收到 I1 的 Bbo
- 实现:`crates/hft-sequencer/tests/sdk_contract/subs.rs::sdk_036_subs_full_does_not_fail_params`

### SDK-037 模型 tick 在 UTC 对齐边界后的第一条 `AdvisorTick` 上投递

- 规则:[Strategy 与 Model](README.md#strategy-与-model)
- 级别:integ
- 里程碑:M2
- 前置:`SeqRig` + `AdvRig`;`AdvisorTick` 间隔 1 s(hft-sequencer 实盘驱动的定时器表);`RecProbe.subscriptions` 调 `model_tick(2_000)`;启动时的 `AdvisorTick` 在 UTC0(`SeqRig` 的 UTC 起点,整秒)
- 步骤:
  1. 驱动到 UTC0 + 4.95 s(其后 4 条 `AdvisorTick`,UTC0 + 1.95 s、2.95 s、3.95 s、4.95 s)
- 期望:
  - `RecModel` 恰收到 2 次 `Timer{tag = MODEL_TICK, group = GROUP_NONE, gix = GIX_NONE}`,在 UTC0 + 2.95 s 与 UTC0 + 4.95 s 的 `AdvisorTick` 记录上(各自越过 2 s 整数倍边界后的第一条;`v.now().seq` 等于该记录的 seq);UTC0 + 1.95 s 与 3.95 s 的记录上没有
  - `RecProbe`(快速部分)没有收到 `MODEL_TICK`
- 实现:`crates/hft-sequencer/tests/sdk_contract/advisor.rs::sdk_037_model_tick_utc_aligned`

### SDK-038 `every_ms` 不是 tick 间隔整数倍时按刻度取整、SDK 不拒绝

- 规则:[Strategy 与 Model](README.md#strategy-与-model)
- 级别:integ
- 里程碑:M2
- 前置:同 SDK-037,但 `model_tick(1_500)`(不是 1 s 刻度的整数倍)
- 步骤:
  1. 驱动到 UTC0 + 6.04 s(其后 6 条 `AdvisorTick`,UTC0 + 1.04 s … 6.04 s)
- 期望:
  - `SetParams` 为 `Applied`;模型 tick 恰 4 次,落在 UTC0 + 2.04 s、3.04 s、5.04 s、6.04 s 的记录上(1.5 s 整数倍边界 1.5、3.0、4.5、6.0 s 之后的第一条)
- 实现:`crates/hft-sequencer/tests/sdk_contract/advisor.rs::sdk_038_model_tick_rounds_to_resolution`

### SDK-039 advisor 在不同位置重启,模型 tick 的投递记录不变

- 规则:[Strategy 与 Model](README.md#strategy-与-model)、[Model 检查点](README.md#model-检查点)
- 级别:prop
- 里程碑:M2
- 性质:同一段 journal(含 `AdvisorTick` 与 Bbo),advisor 在第 k 条记录处丢弃内存状态并从检查点(或空状态 + 全量重放)重启;各次运行中 `MODEL_TICK` 投递所在记录的 seq 集合相同
- 生成:每个 `model_tick` ∈ {100, 250, 1 000, 60 000} 一段约 2 min 的 journal(每 100 ms 一条 I1 Bbo,约 1 500 条);k 在 [首个快照所属引导事件完整提交之后, 长度) 均匀抽取,日志按 k 之前最近的 EVENT_END 截断;检查点取在已应用位置(或不用检查点)。固定回归种子覆盖最早提交边界的两种检查点选择,并确认只含快照、缺 BootDone 的前缀不能启动
- 参考:不重启的一次运行
- 例数:PR 200;每夜 5 000
- 实现:`crates/hft-sequencer/tests/sdk_contract/advisor.rs::sdk_039_model_tick_restart_invariant`

### SDK-040 订阅 `model_own` 的 Model 收到与快速部分同值同序的 Order、Exec、Manual

- 规则:[Strategy 与 Model](README.md#strategy-与-model)、[EventRef](README.md#eventref)
- 级别:integ
- 里程碑:M2
- 前置:`SeqRig` + `AdvRig`;`RecProbe.subscriptions` 调 `model_own(ORDER | EXEC | MANUAL)`;`TP.cmds` 中 op 1 合法
- 步骤:
  1. `RecProbe` 下一张 I1 IOC 0.001(裸露名义低于敞口闸门下限),交易所回 Ack、部分成交 0.0004、全部成交
  2. `RecProbe` 设一个 `TakerSlice` 目标,成交一片
  3. 发送 `StrategyCmd{S3, A1, group = G2, payload = op 1}`(`qty` = 5)
- 期望:
  - `RecModel` 收到的 `Order` × 3、`Exec` × ≥ 1、`Manual` × 1 与 `RecProbe` 收到的同名事件逐字段相同、顺序相同,`v.now().seq` 相同
  - 脚本按固定的 `update_id` 触发,以同一 journal 重放(`AdvRig` 从头重启)得到逐字节相同的 `RecModel` 调用记录
- 实现:`crates/hft-sequencer/tests/sdk_contract/delivery.rs::sdk_040_model_own_mirrors_fast`

### SDK-041 未调用 `model_own` 的 Model 收不到本实例的 Order、Exec、Manual

- 规则:[Strategy 与 Model](README.md#strategy-与-model)、[EventRef](README.md#eventref)
- 级别:integ
- 里程碑:M2
- 前置:同 SDK-040,但 `subscriptions` 不调 `model_own`;另一组:只调 `model_own(EXEC)`
- 步骤:
  1. 重复 SDK-040 的步骤 1–3
- 期望:
  - 第一组:`RecModel` 的调用记录中没有 `Order`、`Exec`、`Manual`
  - 第二组:只有 `Exec`,没有 `Order` 与 `Manual`
- 实现:`crates/hft-sequencer/tests/sdk_contract/delivery.rs::sdk_041_model_own_mask_respected`

### SDK-042 行情变体按 `EvMask` 与 `Part` 投递给快速部分与 Model

- 规则:[EventRef](README.md#eventref)、[Strategy 与 Model](README.md#strategy-与-model)
- 级别:table
- 里程碑:M2
- 前置:`SeqRig` + `AdvRig`;每行一次独立运行,`RecProbe.subscriptions` 对 I2 调一次 `market(I2, 0, <掩码>, <Part>)`(本用例的 I2 为不在任何 group 内的 b2,`l2_depth = 5`;group 腿有 L2 簿时簿未建立前 cell 不能进 Live);随后依次投递 I2 的 Bbo、L2Snapshot{origin = TopN}、Trade、Funding、MarkPrice、IndexPrice、Liquidation 各一条
- 实现:`crates/hft-sequencer/tests/sdk_contract/delivery.rs::sdk_042_market_variants_by_mask_and_part`

| 行 | 掩码 / Part | 快速部分收到 | Model 收到 |
|---|---|---|---|
| SDK-042.1 | 全部 7 位 / `Fast` | Bbo、Book、Trade、Funding、Mark、Index、Liquidation 各 1 次 | 无 |
| SDK-042.2 | 全部 7 位 / `Model` | 无 | 同 .1 的 7 个变体各 1 次 |
| SDK-042.3 | 全部 7 位 / `Both` | 7 个变体各 1 次 | 7 个变体各 1 次 |
| SDK-042.4 | `BBO` / `Both` | 只有 Bbo | 只有 Bbo |
| SDK-042.5 | `TRADE \| LIQUIDATION` / `Fast` | Trade、Liquidation | 无 |
| SDK-042.6 | 不订阅 I2 | 无 | 无 |

### SDK-043 L2 一条消息整批应用后只投递一次 `Book`

- 规则:[EventRef](README.md#eventref)、[Strategy 与 Model](README.md#strategy-与-model)
- 级别:integ
- 里程碑:M2
- 前置:`SeqRig`;本用例的 I2 = 填充 inst 15(V1 现货、资产 4,不属于任何 group),其 `l2_depth = 5`(启动闸门 `Gate(Feeds)` 要等 group 腿的 L2 簿 Live 才放行,group 腿的簿在 steady 状态下不可能尚无快照);`market(I2, 0, BOOK, Fast)`;路由 `DepthTopN`(N = 5)
- 步骤:
  1. 投递一条 5 档的 `L2Snapshot{origin = TopN}`
  2. `RecProbe` 在收到 `Book` 时调 `v.book(I2).levels(Bid, &mut [Level; 8])`
- 期望:
  - `on_event(Book{inst = I2, gix = 0})` 恰一次;`levels` 返回 5,5 档与快照逐字段相同、由内向外;`status() = Live`
- 实现:`crates/hft-sequencer/tests/sdk_contract/views.rs::sdk_043_book_once_per_message`

### SDK-044 只因引擎持仓驱动订阅的行情不投递给策略

- 规则:[EventRef](README.md#eventref)
- 级别:integ
- 里程碑:M2
- 前置:`SeqRig`;A1 在 I5(V2 永续)有账户级 `GROUP_NONE` 持仓 0.5;`RecProbe` 不订阅 I5
- 步骤:
  1. 投递 I5 的 MarkPrice、Funding、Bbo 各一条
- 期望:
  - `RecProbe` 与 `RecModel` 都没有任何 `inst = 105` 的调用;ledger 的 I5 标记价已更新(`v.portfolio(A1).perp(V2, 105).upnl` 随 mark 变化)
- 实现:`crates/hft-sequencer/tests/sdk_contract/delivery.rs::sdk_044_engine_only_feed_not_delivered`

### SDK-045 `Order` 只投递策略自己 `place` 的订单,执行层切片只汇总为 `Exec`

- 规则:[EventRef](README.md#eventref)
- 级别:integ
- 里程碑:M2
- 前置:`SeqRig`;实例 Live、`open_allowed = 1`;`RecProbe` 脚本:在 G1 上 `place` 一张 I1 买入 IOC(`tag = 11`),同时对 G2 设 `TakerSlice` 目标(`tag = 22`)
- 步骤:
  1. `MockVenue` 对 IOC 回 Ack 与全部成交;对执行层的第一片回 Ack 与全部成交
- 期望:
  - `RecProbe` 收到 `Order{gix = 0}` 两次(`Acked`;`Filled{done = true}`),`u.tag = 11`、`u.r` 等于 `outcome` 返回的 `Placed(r)`
  - 收到 `Exec{gix = 1, u.kind = Progress{leg_ix = 0, d = 成交净量}, u.tag = 22}`;没有 `tag = 22` 的 `Order` 事件
- 实现:`crates/hft-sequencer/tests/sdk_contract/delivery.rs::sdk_045_order_only_for_placed`

### SDK-046 `External` 投递给账户绑定的实例,不可归属时 `gix = GIX_NONE`,Model 永不收到

- 规则:[EventRef](README.md#eventref)
- 级别:integ
- 里程碑:M2
- 前置:`SeqRig` + `AdvRig`;`RecProbe.subscriptions` 调 `model_own(ORDER | EXEC | MANUAL)`;G2 的 leg 1(b2)持有空头 0.02
- 步骤:
  1. 投递 `ExternalFill{venue V2, inst 22, cause = Liquidation, qty = 0.02}`(可归属 G2)
  2. 投递 `ExternalFill{venue V1, inst 13, cause = Manual}`(a3 不在 S3 的任何 group 内)
- 期望:
  - 步骤 1:`RecProbe.on_event(External{gix = 1})`,`f.group = G2`、`f.cause = Liquidation`、`f.deadline_tsc > 0`
  - 步骤 2:`RecProbe.on_event(External{gix = 0xFFFF})`,`f.group = GROUP_NONE`、`f.deadline_tsc = 0`
  - `RecModel` 两步都没有 `External` 调用
- 实现:`crates/hft-sequencer/tests/sdk_contract/delivery.rs::sdk_046_external_fast_only`

### SDK-047 `Risk`、`Mode`、`Conn` 只投递给快速部分

- 规则:[EventRef](README.md#eventref)
- 级别:integ
- 里程碑:M2
- 前置:`SeqRig` + `AdvRig`;`RecProbe` 订阅 I1(BBO,Both),`model_own` 全部三位;实例 Live
- 步骤:
  1. 投递使 V2 进入 MarginStress L2 的 `MarginReport`(`margin_ratio = 0.90` ≥ 交叉核对线 0.80)
  2. 发送 `SetMode{S3, G2, A1, CloseOnly}`
  3. 投递 V1 BBO 连接的 `ConnState{Down}`
- 期望:
  - 步骤 1:`RecProbe` 收到 `Risk{a = A1, e = MarginStress{venue = V2, level = L2}}`
  - 步骤 2:`RecProbe` 收到 `Mode{gix = 1, old = Live, new = CloseOnly}`
  - 步骤 3:`RecProbe` 收到 `Conn{venue = V1, state = Down}`
  - `RecModel` 在三步中都没有 `Risk`、`Mode`、`Conn` 调用
- 实现:`crates/hft-sequencer/tests/sdk_contract/delivery.rs::sdk_047_risk_mode_conn_fast_only`

### SDK-048 `Conn` 只投递给订阅了该 venue 任一 inst 的实例,`Alive` 不投递

- 规则:[Strategy 与 Model](README.md#strategy-与-model)、[EventRef](README.md#eventref)
- 级别:integ
- 里程碑:M2
- 前置:`SeqRig`;实例 S3 订阅 I1(V1);实例 S4(s2,绑定 A2)只订阅 I2(V2)
- 步骤:
  1. 投递 V1 BBO 连接的 `ConnState{Resync}`
  2. 投递 V1 BBO 连接的 `ConnState{Alive}`
- 期望:
  - 步骤 1:S3 收到 `Conn{venue = V1, state = Resync}` 一次;S4 没有
  - 步骤 2:S3、S4 都没有 `Conn`
- 实现:`crates/hft-sequencer/tests/sdk_contract/delivery.rs::sdk_048_conn_by_venue_subscription`

### SDK-049 `Notice` 投递给订阅了该 inst 的 group,快速部分与 Model 都收

- 规则:[Strategy 与 Model](README.md#strategy-与-model)、[EventRef](README.md#eventref)
- 级别:integ
- 里程碑:M2
- 前置:`SeqRig` + `AdvRig`;`RecProbe.subscriptions`:`market(I1, 0, BBO, Fast)`、`market(I1, 2, BBO, Fast)`;G2 不订阅 I1
- 步骤:
  1. 发送 `VenueNotice{venue V1, inst 101, Delisting{at_ns = T}}`
- 期望:
  - `RecProbe` 收到 `Notice{inst = 101}` 两次,gix 依次 0、2;`n.kind = Delisting{at_ns = T}`
  - `RecModel` 同样收到 gix 0、2 两次(订阅的 `Part` 为 `Fast` 也收)
- 实现:`crates/hft-sequencer/tests/sdk_contract/delivery.rs::sdk_049_notice_both_parts`

### SDK-050 策略定时器以 GroupId 登记、投递时现查 gix、只投递给快速部分

- 规则:[EventRef](README.md#eventref)、[Ctx 与订单意图](README.md#ctx-与订单意图)
- 级别:integ
- 里程碑:M2
- 前置:`SeqRig` + `AdvRig`;`RecProbe` 在 t0 调 `timer_at(now.tsc_of(now.engine_ns + time::ms(5_000)), tag = 7, G3)`(此时 G3 在 gix 2)
- 步骤:
  1. t0 + 1 s:`SetParams` 移除 G2(G3 变为 gix 1;G2 无仓位)
  2. 推进到 t0 + 5 s
- 期望:
  - 步骤 2:`RecProbe` 收到 `Timer{tag = 7, group = G3, gix = 1}` 一次;`RecModel` 没有 `tag = 7` 的调用
- 实现:`crates/hft-sequencer/tests/sdk_contract/delivery.rs::sdk_050_timer_by_group_id`

### SDK-051 被移除 group 的定时器在应用参数时撤销并写 Note,之后不投递

- 规则:[Strategy 与 Model](README.md#strategy-与-model)
- 级别:integ
- 里程碑:M2
- 前置:同 SDK-050,但定时器登记在 G2(`tag = 8`)
- 步骤:
  1. t0 + 1 s:`SetParams` 移除 G2
  2. 推进到 t0 + 10 s
- 期望:
  - 步骤 1 的同一 Input 内 journal 有一条 sequencer 的 Note,`group = G2`
  - 步骤 2 期间 `RecProbe` 没有 `tag = 8` 的 `Timer`
- 实现:`crates/hft-sequencer/tests/sdk_contract/delivery.rs::sdk_051_removed_group_timer_cancelled`

### SDK-052 `Advice` 只投递给产出它的实例的快速部分

- 规则:[EventRef](README.md#eventref)、[Advice 载荷](README.md#advice-载荷)
- 级别:integ
- 里程碑:M2
- 前置:`SeqRig` + `AdvRig`;实例 S3、S4 都是 `TestSet`,`AdvRig` 运行两者的 Model;`RecModel`(S3)在一次模型 tick 上 `emit_raw(G2, 1, &[1, 0, 0, 0, 0, 0, 0, 0])` 与 `emit_raw(GROUP_NONE, 2, &[1])`
- 步骤:
  1. 驱动到该 Advice 被 sequencer 处理
- 期望:
  - S3 的 `RecProbe` 收到 `Advice{gix = 1, local_kind = 1, payload = [1, 0, 0, 0, 0, 0, 0, 0]}` 与 `Advice{gix = 0xFFFF, local_kind = 2, payload = [1]}`,`basis_seq` 等于 `RecModel` 产出它时正在处理的记录的 seq,`basis_tsc` 等于该记录头 tsc
  - S4 没有 `Advice`;S3、S4 的 Model 都没有 `Advice` 调用
- 实现:`crates/hft-sequencer/tests/sdk_contract/delivery.rs::sdk_052_advice_to_producer_fast_only`

### SDK-053 `Manual` 投递给命令指向的 group,`GROUP_NONE` 为实例级

- 规则:[EventRef](README.md#eventref)、[策略描述符:Note、报表、策略命令](README.md#策略描述符note报表策略命令)
- 级别:integ
- 里程碑:M2
- 前置:`SeqRig`;`RecProbe` 的 `cmds` = op 1 `open`(字段 `qty`: 1..=100,`reduce_only = false`)
- 步骤:
  1. 发送 `StrategyCmd{S3, A1, group = G2, reduce_only = false, payload = encode_cmd(op 1, [5])}`
  2. 发送同一命令但 `group = none`
- 期望:
  - 步骤 1:`RecProbe.on_event(Manual{gix = 1, reduce_only = false})`,`CmdView::parse(payload)` 的 `op() = 1`、`val(0) = Some(5)`;`v.now().seq` = 该命令记录的 seq
  - 步骤 2:`Manual{gix = 0xFFFF}`
- 实现:`crates/hft-sequencer/tests/sdk_contract/delivery.rs::sdk_053_manual_by_group`

### SDK-054 `cmds` 为空的策略收不到 `Manual`

- 规则:[EventRef](README.md#eventref)、[策略描述符:Note、报表、策略命令](README.md#策略描述符note报表策略命令)
- 级别:integ
- 里程碑:M2
- 前置:`SeqRig`;实例 S3 的种类 `cmds = &[]`
- 步骤:
  1. 发送 `StrategyCmd{S3, A1, group = none, payload = [1, 1, 0, 0, 0, 0, 0, 0]}`
- 期望:
  - 客户端收到 gateway 的 `CmdResult{InvalidArgs, detail = ParamInvalid}`;`RecProbe` 没有 `Manual`
- 实现:`crates/hft-ctl/tests/e2e/sdk.rs::sdk_054_no_cmds_no_manual`

### SDK-055 `Warmup` 默认只投递给 Model,`warmup(Part::Both)` 时快速部分也收

- 规则:[EventRef](README.md#eventref)、[预热注入](README.md#预热注入)
- 级别:integ
- 里程碑:M2
- 前置:`SeqRig` + `AdvRig`;第一组 `RecProbe.subscriptions` 不调 `warmup`;第二组调 `warmup(Part::Both)`
- 步骤:
  1. 发送 `InjectWarmup{instance S3, local_kind = 1, chunk_no = 0, last = true, payload = 48 B}`
- 期望:
  - 两组:`CmdResult{Applied}`;`RecModel` 收到 `Warmup{local_kind = 1, chunk_no = 0, last = true}` 一次,`payload` 与命令逐字节相同
  - 第一组:`RecProbe` 没有 `Warmup`;第二组:`RecProbe` 在该命令的同一 Input 内收到同一 `Warmup`
- 实现:`crates/hft-sequencer/tests/sdk_contract/delivery.rs::sdk_055_warmup_routing`

### SDK-056 `Views` 在 `on_event` 中反映引擎已应用本输入、本次意图尚未处理的状态

- 规则:[Views 与 ModelViews](README.md#views-与-modelviews)、[EventRef](README.md#eventref)
- 级别:integ
- 里程碑:M2
- 前置:`SeqRig`;`RecProbe` 订阅 I1(BBO,Fast);G1 的 leg 0 `inflight_immediate = 0`
- 步骤:
  1. 投递 I1 的 Bbo(bid 6_500_000、ask 6_500_100);`RecProbe` 在 `on_event` 内先读 `v.bbo(I1)`,再 `place` 一张 G1 的 I1 买入 IOC 0.01,再读 `v.legs(A1, G1)`
  2. 下一事件里读 `v.legs(A1, G1)`
- 期望:
  - 步骤 1:`v.bbo(I1)` 与事件的 `b` 逐字段相同(bid 6_500_000、ask 6_500_100);`place` 之后读到的 `leg[0].inflight_immediate = 0`
  - 步骤 2:`leg[0].inflight_immediate = 1_000_000`(0.01)
- 实现:`crates/hft-sequencer/tests/sdk_contract/views.rs::sdk_056_views_state_point`

### SDK-057 没有兜底分支的 `match EventRef` 编译失败

- 规则:[版本与兼容规则](README.md#版本与兼容规则)、[EventRef](README.md#eventref)
- 级别:unit
- 里程碑:M2
- 前置:`trybuild`;`tests/ui/event_no_wildcard.rs` 在下游 crate 中穷举当前全部非 chain 变体、没有 `_ =>`;`tests/ui/exec_kind_no_wildcard.rs` 同样处理 `ExecUpdateKind`
- 步骤:
  1. 编译两个文件
- 期望:
  - 两者都编译失败,错误含 `non-exhaustive patterns`
- 实现:`crates/hft-strategy-sdk/tests/ui.rs::sdk_057_non_exhaustive_needs_wildcard`

### SDK-058 回放与 `replay verify` 不运行 Model

- 规则:[策略包的三部分](README.md#策略包的三部分)、[Model 检查点](README.md#model-检查点)
- 级别:integ
- 里程碑:M2
- 前置:`SeqRig` 写一个关键帧后经 advice 环交出 50 条 Advice;`RecSet` 记录每次 Model 方法调用
- 步骤:
  1. `VerifyDriver` 回放整段 journal
- 期望:
  - 验证回放没有不一致;没有任何 Model 方法调用或 Model 投递;`RecProbe` 收到的 50 条 Advice 与实盘逐字节相同
- 实现:`crates/hft-sequencer/tests/sdk_contract/replay.rs::sdk_058_replay_never_runs_model`

### SDK-059 group 数超过 `max_groups_per_instance` 时解码返回 `TooManyGroups`

- 规则:[Strategy 与 Model](README.md#strategy-与-model)
- 级别:table
- 里程碑:M2
- 前置:`EchoProbe` 的 `ParamSet::decode`;`max_groups_per_instance = 8`;body 为 `TP` v2 默认值、无覆盖
- 实现:`crates/hft-strategy-sdk/tests/params_decode.rs::sdk_059_too_many_groups`

| 行 | `GroupTable.len()` | 期望 |
|---|---|---|
| SDK-059.1 | 7 | `Ok` |
| SDK-059.2 | 8 | `Ok` |
| SDK-059.3 | 9 | `Err(ParamError::TooManyGroups)` |

### SDK-060 `TooManyGroups` 在 gateway 映射为 `ParamInvalid`,不到达 sequencer

- 规则:[Strategy 与 Model](README.md#strategy-与-model)
- 级别:integ
- 里程碑:M2
- 前置:`SeqRig`;`max_groups_per_instance = 8`;S3 已有 version 1 生效
- 步骤:
  1. 发送 9 个 group 的 `SetParams{version = 2}`
- 期望:
  - 客户端收到 `CmdResult{InvalidArgs, detail = ParamInvalid}`,`arg & 0xF = 1`(kind `Strategy`);S3 生效版本仍为 1;`RecProbe` 无调用
- 实现:`crates/hft-ctl/tests/e2e/sdk.rs::sdk_060_too_many_groups_at_gateway`

### SDK-061 `non_prod_only` 的策略不能绑定正式账户

- 规则:[Strategy 与 Model](README.md#strategy-与-model)
- 级别:integ
- 里程碑:M2
- 前置:`SeqRig`;种类 probe-np(`non_prod_only = true`);实例 S3(`StrategyId 3`)绑定账户 3(`is_prod = false`),实例 S5(`StrategyId 4`,同一种类)绑定账户 4(`is_prod = true`);两账户 venues {A, B}
- 步骤:
  1. 对 S3 发 `SetParams`
  2. 对 S5 发 `SetParams`
- 期望:
  - 步骤 1:`CmdResult{Applied}`
  - 步骤 2:`CmdResult{InvalidArgs, detail = ParamInvalid}`,`arg & 0xF = 3`(kind `NonProdOnly`);S5 不被创建(无 `create` 调用)
- 实现:`crates/hft-sequencer/tests/sdk_contract/lifecycle.rs::sdk_061_non_prod_only_rejected_on_prod`

### SDK-062 两腿各在自己的 venue 记现金,一个 venue 的成交不改另一个 venue 的余额

- 规则:[Strategy 与 Model](README.md#strategy-与-model)、[视图类型](README.md#视图类型)
- 级别:integ
- 里程碑:M2
- 前置:`SeqRig`;A1 在 V1 有 USDT 10_000(10_000_000_000)、在 V2 有 USDT 5_000(两条都是 A1 在该 venue 该资产的首条 `BalanceReport`:ledger 只用首条报告初始化现金,之后的报告只做漂移核对);G1 两腿 I1(V1)、I2(V2)
- 步骤:
  1. `RecProbe` 在 I1 买入 0.01 BTC @ 65_000.00 的 IOC,`MockVenue` 全部成交、手续费 0 
  2. 下一事件读 `v.portfolio(A1).cash(V1, USDT)`、`cash(V2, USDT)`、`venues()`
- 期望:
  - `cash(V1, USDT).free = 9_350_000_000`(10_000 − 650);`cash(V2, USDT).free = 5_000_000_000`(不变);`venues()` = {V1, V2}
- 实现:`crates/hft-sequencer/tests/sdk_contract/views.rs::sdk_062_cash_per_venue`

### SDK-063 Model 在最大 group 数、满窗口、预热后实际分配不超过 `mem_bytes`

- 规则:[Model 内存与快照声明](README.md#model-内存与快照声明)、[测试桩(testing 特性)](README.md#测试桩testing-特性)
- 级别:unit
- 里程碑:M2
- 前置:`ModelHarness` + `ProbeModel`;`n_groups = 8`;每 group 先注入 256 个预热样本,再投递 300 条实时 Bbo(窗口满)
- 步骤:
  1. 读 `peak_alloc_bytes()`
  2. `snapshot_all(&m, 65_536)`
- 期望:
  - `peak_alloc_bytes() ≤ ProbeModel::mem_bytes(p, 8)`(= 131_072)
  - `path.len() + body.len() ≤ ProbeModel::snapshot_bytes(p, 8)`(= 35_072)
- 实现:`crates/hft-strategy-sdk/tests/model.rs::sdk_063_model_alloc_within_declaration`

### SDK-064 声明小于实际分配的 Model 被夹具拒绝

- 规则:[Model 内存与快照声明](README.md#model-内存与快照声明)
- 级别:unit
- 里程碑:M2
- 前置:`ProbeModel` 的变体 `UnderModel`:`mem_bytes(p, n) = 1`、`snapshot_bytes(p, n) = 1`,其余相同;同 SDK-063 的事件脚本
- 步骤:
  1. 运行 SDK-063 的同一断言函数 `testing::assert_within_declaration::<UnderModel>`
- 期望:
  - 断言失败(`#[should_panic]`),消息含 `mem_bytes` 与实测峰值
- 实现:`crates/hft-strategy-sdk/tests/model.rs::sdk_064_under_declared_model_fails`

### SDK-065 检查点键五项与其他版本量变化时的接受与丢弃

- 规则:[Model 检查点](README.md#model-检查点)
- 级别:table
- 里程碑:M2
- 前置:`SeqRig` + `AdvRig`;实例 S3(s1:`account = A1`、kind 0xFF01、`schema.id = 0xFF01`、`PATH_VERSION = 1`;`CkptHdr.layout_version = Model::LAYOUT_VERSION`);version 7(`thr = 7`)生效、Model 订阅 I1 的 BBO;在 seq L 写出一份检查点(`last_seq = L`),之后 journal 继续(3 条 I1 Bbo);每行只改该行的一项后重启 advisor。检查点文件格式归 [apps/hft-core](../../apps/hft-core/README.md#内存与-journal-段按主机类唯一归属),`MemCkpt` 存运行期 `CkptHdr` + 按该格式排列的路径前缀、路径节与原始样本节
- 实现:`crates/hft-sequencer/tests/sdk_contract/checkpoint.rs::sdk_065_checkpoint_key_matrix`

| 行 | 重启前的改动 | 期望 |
|---|---|---|
| SDK-065.1 | 不改 | `model_restore` 与 `model_restore_path` 各 1 次;没有 `ModelCheckpointDiscarded`、`ModelPathReset` |
| SDK-065.2 | 检查点文件头 `kind_id = 0xFF04` | `Note{source = Advisor, code = ModelCheckpointDiscarded}`,`v[0] = 3`、`v[1] = 3`(KeyMismatch);`create_model` 空状态起步,无 `model_restore` |
| SDK-065.3 | 二进制的 `LAYOUT_VERSION = 2` | 同 .2(`v[1] = 3`) |
| SDK-065.4 | 文件头 `schema_id = 0xFF05`(与二进制的 `schema.id` 不同) | 同 .2(`v[1] = 3`) |
| SDK-065.5 | 文件头 `strategy = 4` | 同 .2(`v[1] = 3`) |
| SDK-065.6 | 文件头 `account = A2` | 同 .2(`v[1] = 3`) |
| SDK-065.7 | 删除检查点文件 | `v[1] = 1`(Missing) |
| SDK-065.8 | 文件头 `len` 与体长不符 | `v[1] = 2`(Corrupt) |
| SDK-065.9 | 原始样本节翻转一位(CRC 不符) | `v[1] = 2` |
| SDK-065.10 | 体内 `path_len` 大于体长(校验和按改后的体重算) | `v[1] = 2` |
| SDK-065.11 | `last_seq` 不在可读的 journal 内(文件头 `last_seq = 2^40`) | `v[1] = 4`(SeqNotInJournal) |
| SDK-065.12 | `RecModel` 脚本:`restore` 返回 `Err` | `v[1] = 5`(RestoreErr);随后 `create_model` |
| SDK-065.13 | seq 5 000 之后有一条改非结构字段的 `SetParams` | 接受;重放到该记录时 `model_on_params` 被调用;无丢弃 Note |
| SDK-065.14 | 换成 `DESC.version` 4 的二进制(`TestSetV4`;`schema.version` 不在检查点键内,`CkptHdr` 没有该字段) | 接受;无丢弃 Note |
| SDK-065.15 | 重启前发 `ReplaceRefdata`(refdata 版本 +1) | 接受;无丢弃 Note |
| SDK-065.16 | hft-core 以另一个构建(`HostProfile.build_git` 不同)重启(版本变化须先 Drain:Drain 结束后以 `DrainTail` 起步) | 接受;无丢弃 Note |
| SDK-065.17 | 二进制的 `PATH_VERSION = 2` | 接受原始样本节;不调 `model_restore_path`;`Note{ModelPathReset}`,`v[0] = 1`、`v[1] = L`、`v[2]` = `path_settle_ms`(`TestSet` 为 0)、`v[3] = 1`、`v[4] = 2` |

### SDK-066 恢复顺序:`restore` → `restore_path` → 以 `last_seq` 处参数调 `on_params` → 从 `last_seq + 1` 重放

- 规则:[Model 检查点](README.md#model-检查点)
- 级别:integ
- 里程碑:M2
- 前置:同 SDK-065(检查点之前 `SetParams{version = 7}` 生效);检查点之前的 I1 Bbo 数分别为 0、3、17,每行 advisor batch 取 1、2、7、64、65535;检查点之后三条 I1 Bbo,再 `SetParams{version = 8}`(`thr = 8`)及一条 I1 Bbo
- 步骤:
  1. 以该检查点重启 advisor,追到 journal 尾
- 期望:
  - `RecModel` 的调用顺序:`create_model`(空状态)、`restore`、`restore_path`、`on_params(version 7)`,`on_params(version 8)`;`on_event` 恰为 seq > L 的全部 I1 Bbo(含 version 8 之后的那条),没有 seq ≤ L 的 `on_event`
  - 每条 Bbo 的 Model 回调执行前,最近应用参数为其 seq 处的版本;15 组组合均不因批处理越过检查点而晚调、漏调或重复调用;引擎 journal 严格回放无分歧
- 实现:`crates/hft-sequencer/tests/sdk_contract/checkpoint.rs::sdk_066_restore_order`

### SDK-067 追赶期间产出的 Advice 依据上次启动的记录,`basis_seq < boot_seq`

- 规则:[Model 检查点](README.md#model-检查点)、[Advice 载荷](README.md#advice-载荷)
- 级别:integ
- 里程碑:M2
- 前置:同 SDK-065;检查点之后 hft-core 停机(崩溃)并从最新快照冷启动,本次 `EpochStart` 的 seq = E
- 步骤:
  1. advisor 以该检查点重启,从 L + 1 重放到实时,期间 `RecModel` 对每条 I1 Bbo 发一条 Advice;把它们交给 sequencer
- 期望:
  - 每条 Advice 都满足 `L < basis_seq < E`;`RecProbe` 收到它们时 `v.now().boot_seq = E`
- 实现:`crates/hft-sequencer/tests/sdk_contract/checkpoint.rs::sdk_067_catchup_advice_predates_boot`

### SDK-068 `restore_path` 出错时只丢路径节,Model 状态等于只恢复原始样本节的结果

- 规则:[Model 检查点](README.md#model-检查点)
- 级别:unit
- 里程碑:M2
- 前置:`ModelHarness` + `ProbeModel`,3 个 group 各 200 个样本;检查点 C;把 C 的路径节截断 1 B 得到 C'
- 步骤:
  1. 新实例 M1:`restore_all(&mut M1, &C', keep_path = true)`
  2. 新实例 M2:`restore_all(&mut M2, &C, keep_path = false)`
  3. 两者各 `snapshot_all(.., 65_536)`
- 期望:
  - 步骤 1 中 `restore_path` 返回 `Err`,`restore_all` 返回 `Err(RestoreError)`,但 M1 的原始样本已恢复
  - 步骤 3 两者的 `path` 与 `body` 逐字节相同(无部分写入的路径状态)
- 实现:`crates/hft-strategy-sdk/tests/checkpoint.rs::sdk_068_restore_path_err_drops_path_only`

### SDK-069 advisor 遇到 `restore_path` 出错时写 `ModelPathReset{RestoreErr}` 并照常使用原始样本节

- 规则:[Model 检查点](README.md#model-检查点)
- 级别:integ
- 里程碑:M2
- 前置:同 SDK-065;`RecModel` 脚本:`restore_path` 返回 `Err`
- 步骤:
  1. 重启 advisor
- 期望:
  - `Note{source = Advisor, code = ModelPathReset}`,`v[0] = 2`、`v[1] = L`、`v[2]` = `path_settle_ms`(0);没有 `ModelCheckpointDiscarded`;随后 `on_params(version 7)` 与 L 之后的重放照常
- 实现:`crates/hft-sequencer/tests/sdk_contract/checkpoint.rs::sdk_069_restore_path_err_note`

### SDK-070 路径状态节总是检查点的第一块,原始样本节按 group 分块

- 规则:[Model 检查点](README.md#model-检查点)、[快照节](README.md#快照节)
- 级别:unit
- 里程碑:M2
- 前置:`ProbeModel`,8 个 group 各 256 个样本;用记录型 writer 测得一个满 group 记录的字节数 R 与实例级节的字节数 S(S < R);`max_bytes = 3R`
- 步骤:
  1. `ModelHarness::snapshot_all(&m, 3R)`,记录 `snapshot_path` 与每次 `snapshot_chunk` 的调用与返回
- 期望:
  - 第一次写出的是 `snapshot_path`;随后 `snapshot_chunk(0, .., 3R)` 写实例级节与 gix 0、1,返回 `More(2)`;`snapshot_chunk(2, ..)` 写 gix 2、3、4,返回 `More(5)`;`snapshot_chunk(5, ..)` 写 gix 5、6、7,返回 `Done`;`n_chunks = 3`
  - 每块原始样本字节 ≤ 3R
- 实现:`crates/hft-strategy-sdk/tests/checkpoint.rs::sdk_070_path_first_then_group_chunks`

### SDK-071 单个 group 大于 `max_bytes` 时该块只含这一个 group

- 规则:[Strategy 与 Model](README.md#strategy-与-model)、[快照节](README.md#快照节)
- 级别:unit
- 里程碑:M2
- 前置:同 SDK-070
- 步骤:
  1. `snapshot_all(&m, 1_000)`
- 期望:
  - 原始样本节共 8 块,每块恰一个 group(4 124 B,第一块另含实例级节),每次返回 `More(gix + 1)`,最后 `Done`;writer 不截断(每块字节 = 该 group 记录长度)
- 实现:`crates/hft-strategy-sdk/tests/checkpoint.rs::sdk_071_oversized_group_own_chunk`

### SDK-072 检查点字节与分块大小无关

- 规则:[Model 检查点](README.md#model-检查点)、[确定性规则](README.md#确定性规则)
- 级别:prop
- 里程碑:M2
- 性质:同一 Model 状态在不同 `max_bytes` 下 `snapshot_all` 得到的 `path` 与 `body` 逐字节相同;`restore_all` 后再 `snapshot_all` 字节不变
- 生成:`ProbeModel`,1–8 个 group,每 group 0–256 个样本;`max_bytes` ∈ {1, 单个 group 记录长度, 65_536, u32::MAX}
- 参考:`max_bytes = u32::MAX` 的结果
- 例数:PR 300;每夜 10 000
- 实现:`crates/hft-strategy-sdk/tests/checkpoint.rs::sdk_072_chunking_invariant`

### SDK-073 无 group 时原始样本节只写实例级节

- 规则:[Strategy 与 Model](README.md#strategy-与-model)
- 级别:unit
- 里程碑:M2
- 前置:`ProbeModel`,`GroupTable` 为空,`n_ticks = 9`
- 步骤:
  1. `snapshot_chunk(0, &mut w, 65_536)`
- 期望:
  - 返回 `Done`;写出的字节恰为实例级节(tag 1,`n_ticks = 9`),无 tag 2 的节
- 实现:`crates/hft-strategy-sdk/tests/checkpoint.rs::sdk_073_no_groups_instance_section_only`

### SDK-074 两节合计超过 `snapshot_bytes` 时本代检查点作废、上一代保留

- 规则:[Model 检查点](README.md#model-检查点)
- 级别:integ
- 里程碑:M2
- 前置:`SeqRig` + `AdvRig`;`RecModel` 声明 `snapshot_bytes = 10_000`(`ckpt` 字段),每个 group 写 8 B 原始样本;第一代检查点已提交(`last_seq = L1`);之后 `SetParams` 把声明降到 16 B(3 个 group 的样本 24 B 超出)
- 步骤:
  1. 写下一代检查点(`AdvRig` 直接开始一轮检查点,代替 `advisor.model_checkpoint_ms` 周期)
  2. 重启 advisor
- 期望:
  - 步骤 1 后 `MemCkpt` 中当前代仍是 `last_seq = L1` 的那份,没有新的提交;一条 Meta `Note{ModelCkptSkipped}`(P1,`v[2] = 16`)
  - 步骤 2 恢复的是 `last_seq = L1` 的检查点(无丢弃 Note)
- 实现:`crates/hft-sequencer/tests/sdk_contract/checkpoint.rs::sdk_074_oversize_generation_discarded`

### SDK-075 原始样本节不存可重算的派生状态,改打分逻辑不需提升 `LAYOUT_VERSION`

- 规则:[Model 检查点](README.md#model-检查点)
- 级别:unit
- 里程碑:M2
- 前置:`ProbeModel` 与它的变体 `ProbeModelB`(Advice 的 `band` 改为样本中位数而非均值;节布局相同、`LAYOUT_VERSION = 1`、`PATH_VERSION = 2`);`ProbeModel` 跑满 256 个样本后写检查点 C
- 步骤:
  1. `ProbeModelB` 的新实例 `restore_all(&C, keep_path = false)`
  2. 再投递 1 个模型 tick
- 期望:
  - 步骤 1 返回 `Ok`;恢复后的 256 个样本与写出前逐个相等
  - 步骤 2 的 Advice `band` 等于这 256 个样本的中位数(由原始样本重算,不依赖检查点中的派生值)
- 实现:`crates/hft-strategy-sdk/tests/checkpoint.rs::sdk_075_derived_state_recomputed`

### SDK-076 布局金样变化而 `LAYOUT_VERSION` 未升时 CI 失败

- 规则:[Model 检查点](README.md#model-检查点)
- 级别:golden
- 里程碑:M2
- 输入:`ProbeModel::LAYOUT` 经 `testing::layout_json` 的输出
- 期望文件:`fixtures/strategies/sdk-probe/ckpt/layout.json`
- 前置:金样旁记录生成时的 `LAYOUT_VERSION = 1`
- 备注:反例——给 tag 2 的节追加元素 `vol: I64 Prefixed` 而 `LAYOUT_VERSION` 仍为 1 → 测试失败,消息指出 `LAYOUT_VERSION` 未升;同时把 `LAYOUT_VERSION` 改为 2 并重新生成金样 → 通过
- 实现:`crates/hft-strategy-sdk/tests/checkpoint.rs::sdk_076_layout_golden_requires_bump`

### SDK-077 路径金样字节变化而 `PATH_VERSION` 未升时 CI 失败

- 规则:[Model 检查点](README.md#model-检查点)
- 级别:golden
- 里程碑:M2
- 输入:固定事件脚本 `tests/common/path_script.rs`(3 个 group、各 200 个 1 s 样本、跨 3 个 UTC 60 s 边界)跑完后 `ProbeModel::snapshot_path` 的字节
- 期望文件:`fixtures/strategies/sdk-probe/ckpt/path.bin`
- 前置:金样旁记录生成时的 `PATH_VERSION = 1`
- 备注:反例——把 `band` 的更新周期从 60 s 改为 30 s 而 `PATH_VERSION` 仍为 1 → 失败;`PATH_VERSION` 改为 2 并重新生成 → 通过;只改 `ProbeModel` 的 Advice 编码而路径节字节不变 → 通过
- 实现:`crates/hft-strategy-sdk/tests/checkpoint.rs::sdk_077_path_golden_requires_bump`

### SDK-078 `check_layout` 发现代码写出的节与 `LAYOUT` 不一致

- 规则:[Model 检查点](README.md#model-检查点)、[测试桩(testing 特性)](README.md#测试桩testing-特性)
- 级别:table
- 里程碑:M2
- 前置:`ProbeModel` 有 3 个 group 各 10 个样本;各行用一个只改该处的变体
- 实现:`crates/hft-strategy-sdk/tests/checkpoint.rs::sdk_078_check_layout`

| 行 | 变体 | 期望 |
|---|---|---|
| SDK-078.1 | 原样 | `Ok(())` |
| SDK-078.2 | `snapshot_chunk` 在 tag 2 节多写一个 `u64` | `Err(LayoutMismatch)` |
| SDK-078.3 | tag 2 节的 `mid` 写成 `I32` | `Err(LayoutMismatch)` |
| SDK-078.4 | Group 节不以 `GroupId` 开头 | `Err(LayoutMismatch)` |
| SDK-078.5 | 写出 `LAYOUT` 中没有的 tag 3 | `Err(LayoutMismatch)` |
| SDK-078.6 | `Prefixed` 元素的前置计数与实际元素个数不符 | `Err(LayoutMismatch)` |

### SDK-079 Model 确定性夹具:任意位置检查点恢复后输出与连续运行逐字节相同

- 规则:[Model 检查点](README.md#model-检查点)、[确定性规则](README.md#确定性规则)
- 级别:prop
- 里程碑:M2
- 性质:同一事件脚本跑两遍 `AdviceOut` 字节相同;在随机位置 k 以随机 `max_bytes` 写检查点 → 新实例 `restore_all(.., keep_path = true)` → 继续脚本,之后的全部 Advice 字节与 Model 的 `snapshot_all` 字节都与连续运行相同
- 生成:`ProbeModel`;1–8 个 group;脚本 100–5 000 条(Bbo、模型 tick、预热块、`on_params` 改非结构字段);k 均匀;`max_bytes` ∈ [1, 65 536]
- 参考:不中断的连续运行
- 例数:PR 200;每夜 10 000
- 实现:`crates/hft-strategy-sdk/tests/checkpoint.rs::sdk_079_checkpoint_resume_bytewise`

### SDK-080 丢弃路径节后,依据时刻在检查点之后 `path_settle_ms` 以外的 Advice 与连续运行相同

- 规则:[Model 检查点](README.md#model-检查点)
- 级别:prop
- 里程碑:M2
- 性质:同 SDK-079,但恢复用 `keep_path = false`;设检查点所指记录的引擎时间为 E0,则依据记录引擎时间 ≥ E0 + `path_settle_ms` × 1_000_000 的全部 Advice 与连续运行逐字节相同
- 生成:同 SDK-079;脚本中按引擎时间插入 0–3 段停机(引擎时间不前进、UTC 前进 1 s–2 h)
- 参考:不中断的连续运行
- 例数:PR 200;每夜 10 000
- 实现:`crates/hft-strategy-sdk/tests/checkpoint.rs::sdk_080_path_reset_converges_after_settle`

### SDK-081 `Model::on_params` 按原因报告从空状态起步的 group,其余 group 字节不变

- 规则:[参数变化时的 group 重置](README.md#参数变化时的-group-重置)
- 级别:table
- 里程碑:M2
- 前置:`ProbeModel`,旧参数 = `TP` 默认值 + G2 覆盖 `pct_bin = 200_000`,`GT3`;每 group 已有 100 个样本;每行从这一状态出发,只做该行的改动后调一次 `on_params`,再用 `snapshot_chunk` 取每个 group 的记录字节
- 实现:`crates/hft-strategy-sdk/tests/group_reset.rs::sdk_081_model_reset_by_cause`

| 行 | 新参数相对旧参数的改动 | `on_params` 返回 | 状态 |
|---|---|---|---|
| SDK-081.1 | 新增 G4(gix 3) | `ResetGroups`:{3: `Fingerprint`} | G4 为空;G1–G3 的记录字节不变 |
| SDK-081.2 | G3 的 leg 1 改为 I5(GroupId 不变) | {2: `Fingerprint`} | G3 为空;G1、G2 不变 |
| SDK-081.3 | G3 的 `asset` 改为 2 | {2: `Fingerprint`} | 同 .2 |
| SDK-081.4 | G2 的 `pct_bin` 覆盖 200_000 → 300_000 | {1: `Structural`} | G2 为空;G1、G3 不变 |
| SDK-081.5 | `pct_bin` 默认值 100_000 → 150_000 | {0: `Structural`, 2: `Structural`} | G1、G3 为空;G2(有覆盖)不变 |
| SDK-081.6 | 去掉 G2 的覆盖,同时 `pct_bin` 默认值改为 150_000 | `ResetAll` | 三个 group 都为空 |
| SDK-081.7 | `open_th` 默认值改 7_000_000、`cooldown_ms` 改 30_000(非结构字段) | `None` | 三个 group 的记录字节与路径节都不变 |
| SDK-081.8 | 只改 G1 的 `open_allowed` 1 → 0 | `None` | 不变 |
| SDK-081.9 | gix 顺序变为 [G3, G1, G2](GroupId 与内容不变) | `None` | 各 GroupId 的记录字节不变 |

### SDK-082 检查点里的存储解释配置与当前参数不一致时报告 `PctConfig`

- 规则:[参数变化时的 group 重置](README.md#参数变化时的-group-重置)
- 级别:unit
- 里程碑:M2
- 前置:`ProbeModel` 以 `pct_bin = 100_000`(全部 group)写出检查点 C;新实例以 `pct_bin` 默认值 100_000、G2 覆盖 250_000 的参数构造
- 步骤:
  1. `restore_all(&C, keep_path = true)`
  2. `on_params(新参数, GT3, rd)`
- 期望:
  - 步骤 2 返回 `ResetGroups`:{1: `PctConfig`};G2 为空,G1、G3 的样本与 C 中一致
- 实现:`crates/hft-strategy-sdk/tests/group_reset.rs::sdk_082_pct_config_mismatch`

### SDK-083 快速部分对新 GroupId 与指纹变化的 group 从空状态起步

- 规则:[参数变化时的 group 重置](README.md#参数变化时的-group-重置)、[Strategy 与 Model](README.md#strategy-与-model)
- 级别:unit
- 里程碑:M2
- 前置:`EchoProbe`,`GT3`,三个 group 都在冷却中(冷却到 `engine_ns + 20 s`)
- 步骤:
  1. `on_params`:G3 的 leg 1 改为 I5(GroupId 不变),新增 G4
  2. 在 `engine_ns + 1 s` 对 gix 0、2、3 各投递满足触发条件的 Bbo
- 期望:
  - 步骤 1 返回 `Ok(())`
  - 步骤 2:G1 不触发(冷却保留);G3、G4 各触发一次(状态为空)
- 实现:`crates/hft-strategy-sdk/tests/group_reset.rs::sdk_083_fast_part_resets_new_and_changed`

### SDK-084 `GroupResets` 每个 gix 至多一个原因,保留码值最小者,按 gix 升序迭代

- 规则:[Strategy 与 Model](README.md#strategy-与-model)
- 级别:unit
- 里程碑:M2
- 前置:`GroupResets::new(8)`
- 步骤:
  1. `set(5, Fingerprint)`、`set(1, PctConfig)`、`set(1, Structural)`、`set(5, PctConfig)`
  2. `get(1)`、`get(5)`、`get(0)`;收集 `iter()`
- 期望:
  - `Some(Structural)`、`Some(Fingerprint)`、`None`;`iter()` = `[(1, Structural), (5, Fingerprint)]`
- 实现:`crates/hft-strategy-sdk/tests/group_reset.rs::sdk_084_group_resets_min_code`

### SDK-085 advisor 对每个被重置的 group 写一条 `ModelGroupReset`

- 规则:[参数变化时的 group 重置](README.md#参数变化时的-group-重置)
- 级别:integ
- 里程碑:M2
- 前置:`SeqRig` + `AdvRig`;S3 的 version 4 生效(`GT3`,G2 覆盖 `pct_bin`);`RecModel` 的 `model_on_params` 按脚本返回重置结果(version 5:G1、G3 `Structural`;version 6:`ResetAll`)
- 步骤:
  1. `SetParams{version = 5}`:`pct_bin` 默认值改为 150_000
  2. `SetParams{version = 6}`:去掉 G2 覆盖并把默认值改回 100_000
- 期望:
  - 步骤 1 后 journal 中恰两条 `Note{source = Advisor, code = ModelGroupReset}`:`group = G1` 与 `G3`,`v[0] = 1`(Structural)、`v[1] = 5`
  - 步骤 2 后恰三条:`group` = G1、G2、G3,`v[0] = 4`(ResetAll)、`v[1] = 6`
- 实现:`crates/hft-sequencer/tests/sdk_contract/group_reset.rs::sdk_085_reset_notes_per_group`

### SDK-086 依据旧指纹产出的 Advice 被 sequencer 丢弃,不投递

- 规则:[参数变化时的 group 重置](README.md#参数变化时的-group-重置)、[Advice 载荷](README.md#advice-载荷)
- 级别:integ
- 里程碑:M2
- 前置:`SeqRig` + `AdvRig`;advisor 落后:`RecModel` 依据 I1 Bbo(seq q)产出 G1 与 G3 的 Advice;之后、Advice 被处理之前 `SetParams` 把 G3 的 leg 1 换成另一个 inst(inst 24,fingerprint 改变,`spec_seq` 前移)
- 步骤:
  1. 两条 Advice 交给 sequencer
- 期望:
  - G3 的 Advice:不投递,journal 有 `Note{Sequencer, AdviceStale}`;G1 的 Advice 照常投递(`gix = 0`、`basis_seq = q`)
- 实现:`crates/hft-sequencer/tests/sdk_contract/group_reset.rs::sdk_086_stale_fingerprint_advice_dropped`

### SDK-087 `reset_preview` 的 `reset_groups` 与 `no_open_ms`

- 规则:[参数变化时的 group 重置](README.md#参数变化时的-group-重置)、[装配:StrategySet 与 strategy_set!](README.md#装配strategyset-与-strategy_set)
- 级别:table
- 里程碑:M2
- 前置:旧参数 = `TP` 默认值 + G2 覆盖 `pct_bin = 200_000`,`GT3`;`ProbeModel`(`WARMUP = true`)与 `ColdModel`(`WARMUP = false`):`cold_start_ms(p, gix) = 100_000 × (gix + 1)`、`path_settle_ms = 60_000`
- 实现:`crates/hft-strategy-sdk/tests/group_reset.rs::sdk_087_reset_preview`

| 行 | Model | 新参数的改动 | 期望 `ResetPreview` |
|---|---|---|---|
| SDK-087.1 | `ColdModel` | 只改 `open_th`、`cooldown_ms` | `{reset_groups: 0, no_open_ms: 0}` |
| SDK-087.2 | `ColdModel` | 新增 G4(gix 3) | `{1, 400_000}` |
| SDK-087.3 | `ProbeModel` | 新增 G4(gix 3) | `{1, 60_000}`(min(400_000, 60_000)) |
| SDK-087.4 | `ColdModel` | `pct_bin` 默认值改为 150_000 | `{2, 300_000}`(G1 gix 0、G3 gix 2) |
| SDK-087.5 | `ProbeModel` | 同 .4 | `{2, 60_000}` |
| SDK-087.6 | `ColdModel` | G3 换腿 + 新增 G4 | `{2, 400_000}` |
| SDK-087.7 | `ColdModel` 变体:`cold_start_ms = 5_000_000_000` | 新增 G4 | `{1, 4_294_967_295}`(饱和到 u32::MAX) |
| SDK-087.8 | `ColdModel` | gix 顺序变为 [G3, G1, G2],内容不变 | `{0, 0}` |

### SDK-088 `reset_preview` 的计数与 `Model::on_params` 实际重置的个数相同

- 规则:[参数变化时的 group 重置](README.md#参数变化时的-group-重置)
- 级别:prop
- 里程碑:M2
- 性质:任意一对新旧参数,`reset_preview(old, old_g, new, new_g).reset_groups` = 对按旧参数构造的 `ProbeModel` 调 `on_params(new, new_g)` 后被重置的 group 数(`ResetAll` 计为 `new_g.len()`,`None` 计 0)
- 生成:旧、新各 1–8 个 group,从 G1–G12 中抽 GroupId;每个 group 以 20% 概率换腿或换资产;`pct_bin` 默认值与每个 group 的覆盖各以 30% 概率变化;非结构字段随机
- 参考:按 README 口径逐 group 计算的参考函数(新 GroupId、指纹变化、结构字段生效值变化之一即计 1)
- 例数:PR 2 000;每夜 50 000
- 实现:`crates/hft-strategy-sdk/tests/group_reset.rs::sdk_088_preview_matches_on_params`

### SDK-089 `define_params!` 的 `structural_changed` 按 GroupId 比较生效值

- 规则:[define_params! 的展开](README.md#define_params-的展开)、[参数变化时的 group 重置](README.md#参数变化时的-group-重置)
- 级别:table
- 里程碑:M2
- 前置:旧参数 = `TP` 默认值 + G2 覆盖 `pct_bin = 200_000`;`structural_changed(&old, old_gix, gix)` 以 GroupId 对齐
- 实现:`crates/hft-strategy-sdk/tests/params_macro.rs::sdk_089_structural_changed`

| 行 | 新参数 | 比较的 group | 期望 |
|---|---|---|---|
| SDK-089.1 | 同旧 | G2 | `false` |
| SDK-089.2 | G2 覆盖 200_000 → 300_000 | G2 | `true` |
| SDK-089.3 | 同 .2 | G1 | `false` |
| SDK-089.4 | 默认值 100_000 → 150_000 | G1 | `true` |
| SDK-089.5 | 同 .4 | G2(有覆盖) | `false` |
| SDK-089.6 | 去掉 G2 覆盖(默认值仍 100_000) | G2 | `true`(生效值 200_000 → 100_000) |
| SDK-089.7 | 新增覆盖 G1 `pct_bin = 100_000`(等于默认值) | G1 | `false` |
| SDK-089.8 | `open_th` 默认值改 | G1 | `false` |

### SDK-090 `n_fields_at` 按 `since` 取字段前缀,更新的版本返回 `None`

- 规则:[参数编解码器(`params` 模块,唯一实现)](README.md#参数编解码器params-模块唯一实现)
- 级别:table
- 里程碑:M1
- 前置:`TP`(`version = 2`,字段 0–4 `since = 1`,字段 5 `since = 2`)
- 实现:`crates/hft-strategy-sdk/tests/params_codec.rs::sdk_090_n_fields_at`

| 行 | `ver` | 期望 |
|---|---|---|
| SDK-090.1 | 0 | `Some(0)` |
| SDK-090.2 | 1 | `Some(5)` |
| SDK-090.3 | 2 | `Some(6)` |
| SDK-090.4 | 3 | `None` |

### SDK-091 `field_ix` 按名字精确查找

- 规则:[参数编解码器(`params` 模块,唯一实现)](README.md#参数编解码器params-模块唯一实现)
- 级别:table
- 里程碑:M1
- 前置:`TP`
- 实现:`crates/hft-strategy-sdk/tests/params_codec.rs::sdk_091_field_ix`

| 行 | 名字 | 期望 |
|---|---|---|
| SDK-091.1 | `"open_th"` | `Some(0)` |
| SDK-091.2 | `"mode"` | `Some(5)` |
| SDK-091.3 | `"Open_th"` | `None` |
| SDK-091.4 | `""` | `None` |

### SDK-092 `parse_value` 按字段单位把文本转成原始值

- 规则:[参数编解码器(`params` 模块,唯一实现)](README.md#参数编解码器params-模块唯一实现)、[参数 schema 与 ParamSet](README.md#参数-schema-与-paramset)
- 级别:table
- 里程碑:M1
- 前置:`TP`;另有一个 Count 字段 `n`(0..=10_000)与一个 Notional 字段 `cap`(0..=10^12)的辅助 schema;BTC 指数 `AssetExp{px_exp: −2, qty_exp: −8}`
- 实现:`crates/hft-strategy-sdk/tests/params_text.rs::sdk_092_parse_value`

| 行 | 字段 | 文本 | `exp` | 期望 |
|---|---|---|---|---|
| SDK-092.1 | `open_th`(Rate) | `"0.5%"` | `None` | `Ok(5_000_000)` |
| SDK-092.2 | `open_th` | `"50bp"` | `None` | `Ok(5_000_000)` |
| SDK-092.3 | `open_th` | `"0.005"` | `None` | `Ok(5_000_000)` |
| SDK-092.4 | `open_th` | `"-0.5%"` | `None` | `Ok(-5_000_000)`(范围不在这里检查) |
| SDK-092.5 | `open_th` | `"0.0000000001"` | `None` | `Err(BadText{field_ix: 0})`(小数位超过 1e-9) |
| SDK-092.6 | `open_th` | `"abc"` | `None` | `Err(BadText{field_ix: 0})` |
| SDK-092.7 | `cooldown_ms`(DurationMs) | `"5m"` | `None` | `Ok(300_000)` |
| SDK-092.8 | `cooldown_ms` | `"30s"` | `None` | `Ok(30_000)` |
| SDK-092.9 | `cooldown_ms` | `"2h"` | `None` | `Ok(7_200_000)` |
| SDK-092.10 | `cooldown_ms` | `"1500"` | `None` | `Ok(1_500)` |
| SDK-092.11 | `n`(Count) | `"1500"` | `None` | `Ok(1_500)` |
| SDK-092.12 | `cap`(Notional) | `"1500"` | `None` | `Ok(1_500_000_000)` |
| SDK-092.13 | `enabled`(Bool) | `"true"` | `None` | `Ok(1)` |
| SDK-092.14 | `enabled` | `"false"` | `None` | `Ok(0)` |
| SDK-092.15 | `mode`(Enum) | `"b"` | `None` | `Ok(1)` |
| SDK-092.16 | `mode` | `"d"` | `None` | `Err(BadEnum{field_ix: 5})` |
| SDK-092.17 | `max_qty`(Qty) | `"0.5"` | BTC | `Ok(50_000_000)` |
| SDK-092.18 | `max_qty` | `"0.5"` | `None` | `Err(BadText{field_ix: 3})` |
| SDK-092.19 | `max_qty` | `"0.000000001"` | BTC | `Err(BadText{field_ix: 3})`(超过 `qty_exp`) |

### SDK-093 `format_value` 与 `parse_value` 往返

- 规则:[参数编解码器(`params` 模块,唯一实现)](README.md#参数编解码器params-模块唯一实现)
- 级别:prop
- 里程碑:M1
- 性质:对每个字段与其 `[min, max]` 内的任意原始值 x,`parse_value(f, &format_value(f, x, exp), exp) == Ok(x)`
- 生成:`TP` 与 SDK-092 的辅助 schema 的全部字段;x 在 `[min, max]` 均匀,另含 min、max、0;Qty 字段 `exp` 取 `qty_exp` ∈ [−10, 0]
- 参考:恒等
- 例数:PR 10 000;每夜 1 000 000
- 实现:`crates/hft-strategy-sdk/tests/params_text.rs::sdk_093_format_parse_roundtrip`

### SDK-094 `encode_body` 写出的字节布局

- 规则:[SPB1 body 布局(草案,实现时以代码与编译期断言为准)](README.md#spb1-body-布局草案实现时以代码与编译期断言为准)、[参数编解码器(`params` 模块,唯一实现)](README.md#参数编解码器params-模块唯一实现)
- 级别:unit
- 里程碑:M1
- 前置:`TP` v2;`n_groups = 3`;`RawParams{defaults: [5_000_000, 20_000, 100_000, 0, 1, 0], overrides: [(2, 0, 7_000_000), (0, 0, 6_000_000), (0, 0, 6_000_000)]}`(`(group_ix, field_ix, value)`)
- 步骤:
  1. `encode_body(&TP, 3, &raw)`
- 期望:
  - `Ok(bytes)`,`bytes.len() = 88`(8 + 8 × 6 + 16 × 2)
  - 字节 0..8 = `06 00 00 00 02 00 00 00`(`n_fields = 6`、`_pad = 0`、`n_overrides = 2`)
  - 字节 8..56 = 6 个小端 i64 默认值;字节 56..72 = `(0, 0, 0, 6_000_000)`;字节 72..88 = `(2, 0, 0, 7_000_000)`(排序后、重复的一条合并)
- 实现:`crates/hft-strategy-sdk/tests/params_codec.rs::sdk_094_encode_body_layout`

### SDK-095 `encode_body` 的错误分支

- 规则:[参数编解码器(`params` 模块,唯一实现)](README.md#参数编解码器params-模块唯一实现)、[参数 schema 与 ParamSet](README.md#参数-schema-与-paramset)
- 级别:table
- 里程碑:M1
- 前置:`TP` v2;`n_groups = 3`;基准 `raw` 同 SDK-094;每行只做该行改动
- 实现:`crates/hft-strategy-sdk/tests/params_codec.rs::sdk_095_encode_body_errors`

| 行 | 改动 | 期望 |
|---|---|---|
| SDK-095.1 | `defaults[0] = 100_000_000`(= max) | `Ok` |
| SDK-095.2 | `defaults[0] = 100_000_001` | `Err(OutOfRange{field_ix: 0, group_ix: 0xFFFF})` |
| SDK-095.3 | `defaults[2] = 9_999`(min − 1) | `Err(OutOfRange{field_ix: 2, group_ix: 0xFFFF})` |
| SDK-095.4 | `defaults[5] = 3`(Enum 只有 3 个名字) | `Err(BadEnum{field_ix: 5})` |
| SDK-095.5 | 覆盖 `(1, 1, 30_000)`(`cooldown_ms` 不是 per_group) | `Err(NotPerGroup{field_ix: 1})` |
| SDK-095.6 | 覆盖 `(3, 0, 1)`(`group_ix = n_groups`) | `Err(GroupIxOutOfRange{group_ix: 3})` |
| SDK-095.7 | 覆盖 `(1, 0, 100_000_001)` | `Err(OutOfRange{field_ix: 0, group_ix: 1})` |
| SDK-095.8 | 覆盖表改为 `[(0, 0, 6_000_000), (0, 0, 6_500_000)]`(同一键、值不同) | `Err(OverrideOrder{ix: 1})` |
| SDK-095.9 | `defaults` 只有 5 项 | `Err(BodyLen)` |

### SDK-096 `decode_body` 接受旧 `schema_ver` 的 body 并为新字段补默认值

- 规则:[参数编解码器(`params` 模块,唯一实现)](README.md#参数编解码器params-模块唯一实现)、[版本与兼容规则](README.md#版本与兼容规则)
- 级别:unit
- 里程碑:M1
- 前置:用 `TP` 的 v1 前缀(5 个字段)编码的 body(`n_fields = 5`,默认值 [6_000_000, 25_000, 100_000, 0, 0],无覆盖)
- 步骤:
  1. `decode_body(&TP, 1, 3, &body)`
- 期望:
  - `Ok(raw)`,`raw.defaults = [6_000_000, 25_000, 100_000, 0, 0, 0]`(字段 5 `mode` 取默认 0),`raw.overrides` 为空
- 实现:`crates/hft-strategy-sdk/tests/params_codec.rs::sdk_096_decode_old_version_fills_default`

### SDK-097 `decode_body` 的版本与内容错误

- 规则:[参数编解码器(`params` 模块,唯一实现)](README.md#参数编解码器params-模块唯一实现)
- 级别:table
- 里程碑:M1
- 前置:`TP` v2;`n_groups = 3`;各行的 body 由一个不做检查的测试编码器写出
- 实现:`crates/hft-strategy-sdk/tests/params_codec.rs::sdk_097_decode_body_errors`

| 行 | body 与 `schema_ver` | 期望 |
|---|---|---|
| SDK-097.1 | `n_fields = 6`,`schema_ver = 3` | `Err(SchemaTooNew{have: 2, got: 3})` |
| SDK-097.2 | `n_fields = 5`,`schema_ver = 2` | `Err(BodyLen)`(应为 6) |
| SDK-097.3 | `n_fields = 6`,`schema_ver = 1` | `Err(BodyLen)`(应为 5) |
| SDK-097.4 | 默认值 `open_th = −1` | `Err(OutOfRange{field_ix: 0, group_ix: 0xFFFF})` |
| SDK-097.5 | 覆盖 `(0, 4, 0)`(`enabled` 不是 per_group) | `Err(NotPerGroup{field_ix: 4})` |
| SDK-097.6 | 覆盖 `(2, 2, 1_000_001)` | `Err(OutOfRange{field_ix: 2, group_ix: 2})` |
| SDK-097.7 | 默认值 `mode = 7` | `Err(BadEnum{field_ix: 5})` |

### SDK-098 `body_view` 的容器校验

- 规则:[SPB1 body 布局(草案,实现时以代码与编译期断言为准)](README.md#spb1-body-布局草案实现时以代码与编译期断言为准)
- 级别:table
- 里程碑:M1
- 前置:SDK-094 的 88 B body;`n_groups = 3`;每行只做该行改动
- 实现:`crates/hft-strategy-sdk/tests/body_view.rs::sdk_098_body_view_checks`

| 行 | 改动 | 期望 |
|---|---|---|
| SDK-098.1 | 不改 | `Ok(v)`:`v.defaults.len() = 6`、`v.overrides.len() = 2` |
| SDK-098.2 | 截掉最后 1 B(87 B) | `Err(BodyLen)` |
| SDK-098.3 | 末尾多 8 B(96 B) | `Err(BodyLen)` |
| SDK-098.4 | 空 body(0 B) | `Err(BodyLen)` |
| SDK-098.5 | `n_overrides = 3`,长度不变 | `Err(BodyLen)` |
| SDK-098.6 | `ParamsBodyHdr._pad = 1` | `Err(Padding)` |
| SDK-098.7 | 第 2 条覆盖的 `_pad = 0x0100_0000` | `Err(Padding)` |
| SDK-098.8 | 两条覆盖交换顺序 | `Err(OverrideOrder{ix: 1})` |
| SDK-098.9 | 两条覆盖都是 `(0, 0)` | `Err(OverrideOrder{ix: 1})` |
| SDK-098.10 | 第 2 条覆盖 `group_ix = 3` | `Err(GroupIxOutOfRange{group_ix: 3})` |
| SDK-098.11 | 只有 `ParamsBodyHdr`(`n_fields = 0`、`n_overrides = 0`,8 B) | `Ok`,两个切片都为空 |

### SDK-099 `body_view` 零拷贝、不分配

- 规则:[SPB1 body 布局(草案,实现时以代码与编译期断言为准)](README.md#spb1-body-布局草案实现时以代码与编译期断言为准)
- 级别:unit
- 里程碑:M1
- 前置:计数分配器;8 B 对齐的缓冲,含 4 096 条覆盖的合法 body
- 步骤:
  1. `body_view(&buf, 64)`
- 期望:
  - `Ok(v)`;`v.defaults.as_ptr()` 等于 `buf.as_ptr() + 8`;分配次数 0
- 实现:`crates/hft-strategy-sdk/tests/body_view.rs::sdk_099_body_view_zero_copy`

### SDK-100 `ParamsBodyHdr` 与 `ParamOverride` 的大小与偏移

- 规则:[SPB1 body 布局(草案,实现时以代码与编译期断言为准)](README.md#spb1-body-布局草案实现时以代码与编译期断言为准)
- 级别:table
- 里程碑:M1
- 前置:无
- 实现:`crates/hft-strategy-sdk/tests/layout.rs::sdk_100_body_struct_layout`

| 行 | 断言 | 期望 |
|---|---|---|
| SDK-100.1 | `size_of::<ParamsBodyHdr>()`、`align_of` | 8、4 |
| SDK-100.2 | `offset_of!(ParamsBodyHdr, n_fields / _pad / n_overrides)` | 0 / 2 / 4 |
| SDK-100.3 | `size_of::<ParamOverride>()`、`align_of` | 16、8 |
| SDK-100.4 | `offset_of!(ParamOverride, group_ix / field_ix / _pad / value)` | 0 / 2 / 4 / 8 |

### SDK-101 随机 schema 与覆盖表的 `encode_body → body_view → decode_body` 往返

- 规则:[参数编解码器(`params` 模块,唯一实现)](README.md#参数编解码器params-模块唯一实现)、[SPB1 body 布局(草案,实现时以代码与编译期断言为准)](README.md#spb1-body-布局草案实现时以代码与编译期断言为准)
- 级别:prop
- 里程碑:M1
- 性质:合法的 `RawParams` 编码后 `body_view` 成功,`decode_body` 得到与输入相同的 `RawParams`(覆盖表按 `(group_ix, field_ix)` 排序去重后)
- 生成:随机 schema(1–64 个字段,单位随机;`per_group` 30%;min ≤ default ≤ max);`n_groups` 0–64;覆盖 0–4 096 条,只落在 per_group 字段,值在范围内,含 10% 重复项
- 参考:`BTreeMap<(u16, u16), i64>` 语义
- 例数:PR 2 000;每夜 100 000
- 实现:`crates/hft-strategy-sdk/tests/params_codec.rs::sdk_101_encode_decode_roundtrip`

### SDK-102 `decode`/`validate` 对任意字节与任意参数不 panic

- 规则:[schema JSON 与 define_params!](README.md#schema-json-与-define_params)
- 级别:prop
- 里程碑:M2
- 性质:`ci` 剖面下对任意 body、`schema_ver`、`GroupTable`,`EchoProbe` 的 `ParamSet::decode` 与 `validate` 返回 `Ok` 或 `Err(ParamError)`,`catch_unwind` 从不捕获到 panic
- 生成:body 0–8 192 B 随机字节,另 50% 由合法 body 随机翻转 1–8 字节;`schema_ver` 0–5;`GroupTable` 0–9 个 group,腿的 inst 随机(含不存在的)
- 参考:不 panic
- 例数:PR 5 000;每夜 500 000
- 实现:`crates/hft-strategy-sdk/tests/params_decode.rs::sdk_102_decode_validate_never_panic`

### SDK-103 策略 `check` 的错误经 `validate` 返回 `GroupShape` 与 `Invalid`

- 规则:[参数 schema 与 ParamSet](README.md#参数-schema-与-paramset)、[schema JSON 与 define_params!](README.md#schema-json-与-define_params)
- 级别:table
- 里程碑:M2
- 前置:`EchoProbe::check`:每个 group 必须 2 腿、leg 0 为现货、leg 1 为 USDT 永续、两腿同资产,否则 `GroupShape{gix, code}`(code 1 腿数、2 市场类型、3 资产);`enabled = 1` 时任一 group 的生效 `open_th` 为 0 → `Invalid{field_ix: 0, code: 1}`
- 实现:`crates/hft-strategy-sdk/tests/params_decode.rs::sdk_103_validate_shape_and_cross_field`

| 行 | 输入 | 期望 |
|---|---|---|
| SDK-103.1 | `GT3`,默认值 | `Ok(())` |
| SDK-103.2 | G2 只有 1 腿 | `Err(GroupShape{gix: 1, code: 1})` |
| SDK-103.3 | G3 的 leg 1 换成现货 inst | `Err(GroupShape{gix: 2, code: 2})` |
| SDK-103.4 | G1 的 leg 1 换成 I5(资产 2) | `Err(GroupShape{gix: 0, code: 3})` |
| SDK-103.5 | G2 覆盖 `open_th = 0`,`enabled = 1` | `Err(Invalid{field_ix: 0, code: 1})` |
| SDK-103.6 | 同 .5 但 `enabled = 0` | `Ok(())` |

### SDK-104 `define_params!` 生成的访问表按 gix 应用覆盖

- 规则:[define_params! 的展开](README.md#define_params-的展开)、[schema JSON 与 define_params!](README.md#schema-json-与-define_params)
- 级别:unit
- 里程碑:M2
- 前置:`TP` v2;`GT3`;body:默认值同 SDK-094,覆盖 `(0, 0, 6_000_000)`、`(2, 3, 50_000_000)`
- 步骤:
  1. `TPParams::decode(&body, 2, &GT3)`
- 期望:
  - `p.defaults().open_th = Rate(5_000_000)`;`p.at(0).open_th = Rate(6_000_000)`;`p.at(1).open_th = Rate(5_000_000)`;`p.at(2).max_qty = Qty(50_000_000)`;`p.at(1).max_qty = Qty(0)`;`p.at(0).mode` 为下标 0(`"a"`)
- 实现:`crates/hft-strategy-sdk/tests/params_macro.rs::sdk_104_at_applies_overrides`

### SDK-105 `define_params!` 的 schema 编译期校验

- 规则:[schema JSON 与 define_params!](README.md#schema-json-与-define_params)、[参数 schema 与 ParamSet](README.md#参数-schema-与-paramset)
- 级别:table
- 里程碑:M2
- 前置:`trybuild`;每行是 `tests/ui/params_*.rs` 中的一个 `define_params!` 声明,只违反该行一条规则
- 实现:`crates/hft-strategy-sdk/tests/ui.rs::sdk_105_schema_const_checks`

| 行 | 声明 | 期望 |
|---|---|---|
| SDK-105.1 | `TP` 原样 | 编译通过 |
| SDK-105.2 | 两个字段都叫 `open_th` | 编译失败 |
| SDK-105.3 | `min = 10, default = 9` | 编译失败 |
| SDK-105.4 | `default = 11, max = 10` | 编译失败 |
| SDK-105.5 | `min = default = max = 10` | 编译通过 |
| SDK-105.6 | `Qty` 字段不带 `per_group` | 编译失败 |
| SDK-105.7 | `Px` 字段不带 `per_group` | 编译失败 |
| SDK-105.8 | `Enum(["a", "b"])` 的 `default = 2` | 编译失败 |
| SDK-105.9 | 字段 `since` 依次为 1、2、1 | 编译失败 |
| SDK-105.10 | `version = 1` 而最大 `since = 2` | 编译失败 |
| SDK-105.11 | 257 个字段 | 编译失败 |
| SDK-105.12 | 256 个字段 | 编译通过 |
| SDK-105.13 | `structural` 字段带 `minor = Abs(1)` | 编译失败 |
| SDK-105.14 | `Bool` 字段带 `minor = RelPct(10)` | 编译失败 |
| SDK-105.15 | `Enum` 字段带 `minor = Abs(1)` | 编译失败 |
| SDK-105.16 | `Bool` 字段带 `minor = Any` | 编译通过 |
| SDK-105.17 | `minor = RelPct(0)` | 编译失败 |
| SDK-105.18 | `minor = RelPct(101)` | 编译失败 |
| SDK-105.19 | `minor = RelPct(1)` 与 `RelPct(100)` | 编译通过 |

### SDK-106 规范 schema JSON 的键序、取值写法与 `display` 子对象

- 规则:[schema JSON 与 define_params!](README.md#schema-json-与-define_params)、[策略描述符:Note、报表、策略命令](README.md#策略描述符note报表策略命令)
- 级别:golden
- 里程碑:M1
- 输入:`schema_json(&EchoProbe::DESC, &ModelLayoutDesc::EMPTY, 2)`(`TP` v2、`non_prod_only = false`、3 个 Note、1 个报表字段、SDK-114 的 3 个命令、无 Model);另以 `ProbeModel::LAYOUT` 调用一次
- 期望文件:`fixtures/params/sdk-tp/schema.golden.json`
- 前置:金样由人按 README 的键序手写核对:顶层键依次 `name`、`kind_id`、`non_prod_only`、`schema_id`、`schema_version`、`fields`、`notes`、`report`、`cmds`、`sections`(`[]`);`open_th` 的 `minor_change = "rel_pct:10"`、`cooldown_ms` 的 `"abs:1000"`、`max_qty` 的 `"any"`、`pct_bin` 的 `"none"`;只有 `mode` 带 `enum`;`open_th` 的 `display = {"min":"0%","max":"10%","default":"0.5%"}`;无多余空白;数值为十进制整数;以 `ProbeModel::LAYOUT` 调用时只有末尾不同:`"sections":[{"tag":1,"name":"ticks"},{"tag":2,"name":"samples"}]`
- 实现:`crates/hft-strategy-sdk/tests/schema_json.rs::sdk_106_schema_json_golden`

### SDK-107 `schema_digest` 只覆盖编码相关的键

- 规则:[参数编解码器(`params` 模块,唯一实现)](README.md#参数编解码器params-模块唯一实现)
- 级别:table
- 里程碑:M1
- 前置:基准 D0 = `schema_digest(&EchoProbe::DESC, 2)`;每行用只改该处的描述符变体
- 实现:`crates/hft-strategy-sdk/tests/schema_json.rs::sdk_107_digest_scope`

| 行 | 改动 | 期望 |
|---|---|---|
| SDK-107.1 | 不改 | 与金样 `fixtures/params/sdk-tp/schema.golden.json` 旁记录的 `schema_digest` 相同 |
| SDK-107.2 | 改 `open_th` 的 `doc` | = D0 |
| SDK-107.3 | `pct_bin` 去掉 `structural` | = D0 |
| SDK-107.4 | `enabled` 去掉 `manual_only` | = D0 |
| SDK-107.5 | `open_th` 的 `relax` 改为 `Up` | = D0 |
| SDK-107.6 | `open_th` 的 `minor_change` 改为 `Any` | = D0 |
| SDK-107.7 | `non_prod_only = true` | = D0 |
| SDK-107.8 | 增删一个 Note、报表字段或命令 | = D0 |
| SDK-107.9 | `open_th` 的 `max` 改为 100_000_001 | ≠ D0 |
| SDK-107.10 | `cooldown_ms` 的 `default` 改为 20_001 | ≠ D0 |
| SDK-107.11 | `max_qty` 的 `per_group` 去掉(换成 Notional 单位以通过编译期校验) | ≠ D0 |
| SDK-107.12 | 字段 `cooldown_ms` 改名 `cool_ms` | ≠ D0 |
| SDK-107.13 | `mode` 的枚举名改为 `["a", "b", "z"]` | ≠ D0 |
| SDK-107.14 | `schema_digest(&DESC, 1)` | ≠ D0(按 v1 截取 5 个字段) |
| SDK-107.15 | `kind_id` 改为 0xFF09 | ≠ D0 |

### SDK-108 从 JSON 还原的 `OwnedSchema` 与编译进来的 schema 编解码结果相同

- 规则:[参数编解码器(`params` 模块,唯一实现)](README.md#参数编解码器params-模块唯一实现)、[参数编解码器的使用方](README.md#参数编解码器的使用方)
- 级别:prop
- 里程碑:M1
- 性质:`OwnedSchema::from_json(&schema_json(&DESC, 2))` 得到的 schema 与 `&TP` 对同一 `RawParams` 的 `encode_body` 字节相同、对同一 body 的 `decode_body` 结果相同;`schema_digest` 相同
- 生成:SDK-101 的随机 `RawParams`(落在 `TP` 上)
- 参考:`&'static ParamSchemaDesc` 的结果
- 例数:PR 1 000;每夜 50 000
- 实现:`crates/hft-strategy-sdk/tests/schema_json.rs::sdk_108_owned_schema_equivalent`

### SDK-109 `OwnedSchema::from_json` 拒绝不合规的 JSON

- 规则:[参数编解码器(`params` 模块,唯一实现)](README.md#参数编解码器params-模块唯一实现)
- 级别:table
- 里程碑:M1
- 前置:基准为 SDK-106 金样;每行只做该行改动
- 实现:`crates/hft-strategy-sdk/tests/schema_json.rs::sdk_109_owned_schema_rejects`

| 行 | 改动 | 期望 |
|---|---|---|
| SDK-109.1 | 不改 | `Ok` |
| SDK-109.2 | 截断到一半 | `Err(_)` |
| SDK-109.3 | 删除 `fields[0].min` | `Err(_)` |
| SDK-109.4 | `fields[0].unit = "Foo"` | `Err(_)` |
| SDK-109.5 | `fields[0].min = 10`、`default = 9` | `Err(_)` |

### SDK-110 `minor_change_check` 按字段属性、基准与 GroupId 判定大改

- 规则:[参数 schema 与 ParamSet](README.md#参数-schema-与-paramset)、[参数编解码器(`params` 模块,唯一实现)](README.md#参数编解码器params-模块唯一实现)
- 级别:table
- 里程碑:M1
- 前置:`TP`;基准 = `TP` 默认值 + G2 覆盖 `open_th = 5_000_000`,groups [G1, G2];当前参数每行只做该行改动;返回值写 `Ok` 或 `Err(MajorChange{field_ix, group})`
- 实现:`crates/hft-strategy-sdk/tests/minor_change.rs::sdk_110_minor_change_check`

| 行 | 当前参数相对基准的改动 | 期望 |
|---|---|---|
| SDK-110.1 | 不改 | `Ok` |
| SDK-110.2 | `open_th` 默认值 5_000_000 → 5_500_000(+10%,`RelPct(10)`) | `Ok` |
| SDK-110.3 | → 5_500_001 | `Err{field_ix: 0, group: GROUP_NONE}` |
| SDK-110.4 | → 4_500_000 | `Ok` |
| SDK-110.5 | → 4_499_999 | `Err{0, GROUP_NONE}` |
| SDK-110.6 | 基准 `open_th` 默认值为 0,当前 1 | `Err{0, GROUP_NONE}` |
| SDK-110.7 | `cooldown_ms` 20_000 → 21_000(`Abs(1_000)`) | `Ok` |
| SDK-110.8 | → 21_001 | `Err{1, GROUP_NONE}` |
| SDK-110.9 | → 18_999 | `Err{1, GROUP_NONE}` |
| SDK-110.10 | `max_qty` G1 覆盖 0 → 10^12(`Any`) | `Ok` |
| SDK-110.11 | `pct_bin` 默认值 100_000 → 100_001(`None`) | `Err{2, GROUP_NONE}` |
| SDK-110.12 | G2 覆盖 `open_th` 5_000_000 → 6_000_000 | `Err{0, G2}` |
| SDK-110.13 | 当前 groups 顺序 [G2, G1],覆盖值随 GroupId 不变 | `Ok` |
| SDK-110.14 | 新增 G3 且其 `open_th` 覆盖 9_000_000 | `Ok`(只在一版里出现的 group 不比较) |
| SDK-110.15 | 移除 G1 | `Ok` |
| SDK-110.16 | G2 覆盖 `open_th` → 6_000_000 且 `cooldown_ms` → 30_000 | `Err{0, G2}`(按 field_ix 取第一个) |
| SDK-110.17 | G1、G2 都覆盖 `open_th` → 6_000_000 | `Err{0, G1}`(同字段按 GroupId 升序) |

### SDK-111 `minor_change_check` 按与基准的累计差判定,分步改不能绕过

- 规则:[参数 schema 与 ParamSet](README.md#参数-schema-与-paramset)
- 级别:unit
- 里程碑:M1
- 前置:`TP`;基准 `open_th` 默认值 5_000_000
- 步骤:
  1. 版本 a:5_400_000;`minor_change_check(TP, 基准, a)`
  2. 版本 b:5_800_000(相对 a 只 +7.4%);`minor_change_check(TP, 基准, b)`
- 期望:
  - 步骤 1 `Ok`;步骤 2 `Err(MajorChange{field_ix: 0, group: GROUP_NONE})`
- 实现:`crates/hft-strategy-sdk/tests/minor_change.rs::sdk_111_cumulative_against_base`

### SDK-112 `encode_cmd` 与 `decode_cmd` 的载荷布局与往返

- 规则:[策略描述符:Note、报表、策略命令](README.md#策略描述符note报表策略命令)、[参数编解码器(`params` 模块,唯一实现)](README.md#参数编解码器params-模块唯一实现)
- 级别:table
- 里程碑:M1
- 前置:`EchoProbe::DESC.cmds`(SDK-114)
- 实现:`crates/hft-strategy-sdk/tests/cmd.rs::sdk_112_cmd_codec`

| 行 | 调用 | 期望 |
|---|---|---|
| SDK-112.1 | `encode_cmd(open, [1_000_000_000])` | `Ok`,16 B:`01 01 01 00 00 00 00 00` + `1_000_000_000` 的小端 i64 |
| SDK-112.2 | `decode_cmd(cmds, <.1 的字节>)` | `Ok((1, [1_000_000_000]))` |
| SDK-112.3 | `encode_cmd(close, [])` | `Ok`,8 B:`01 02 00 00 00 00 00 00` |
| SDK-112.4 | `decode_cmd(cmds, <.3 的字节>)` | `Ok((2, []))` |
| SDK-112.5 | `encode_cmd(open, [10_000_000_001])` | `Err(OutOfRange{field_ix: 0, group_ix: 0xFFFF})` |
| SDK-112.6 | `encode_cmd(open, [])` | `Err(BodyLen)` |
| SDK-112.7 | `encode_cmd(set_mode, [2])`(枚举 2 个名字) | `Err(BadEnum{field_ix: 0})` |
| SDK-112.8 | `decode_cmd(cmds, op = 9 的合法形状载荷)` | `Err(UnknownOp{op: 9})` |
| SDK-112.9 | `decode_cmd(&[], <.1 的字节>)` | `Err(UnknownOp{op: 1})` |
| SDK-112.10 | `decode_cmd(cmds, ver = 2)` | `Err(BodyLen)` |
| SDK-112.11 | `decode_cmd(cmds, op 1 但 n = 0、长度 8)` | `Err(BodyLen)`(字段数应为 1) |
| SDK-112.12 | `decode_cmd(cmds, op 1、值 10_000_000_001)` | `Err(OutOfRange{field_ix: 0, group_ix: 0xFFFF})` |
| SDK-112.13 | `decode_cmd(cmds, 7 B)` | `Err(BodyLen)` |

### SDK-113 `CmdView::parse` 的错误分支与取值

- 规则:[策略描述符:Note、报表、策略命令](README.md#策略描述符note报表策略命令)
- 级别:table
- 里程碑:M2
- 前置:无
- 实现:`crates/hft-strategy-sdk/tests/cmd.rs::sdk_113_cmd_view_parse`

| 行 | 载荷 | 期望 |
|---|---|---|
| SDK-113.1 | SDK-112.1 的 16 B | `Ok(v)`:`v.op() = 1`、`v.val(0) = Some(1_000_000_000)`、`v.val(1) = None` |
| SDK-113.2 | 7 B | `Err(CmdError::TooShort)` |
| SDK-113.3 | 8 B,`ver = 2` | `Err(CmdError::BadVersion)` |
| SDK-113.4 | `n = 32`,长度 264 | `Err(CmdError::TooMany)` |
| SDK-113.5 | `n = 2`,长度 16 | `Err(CmdError::LenMismatch)` |
| SDK-113.6 | `n = 1`,长度 24 | `Err(CmdError::LenMismatch)` |
| SDK-113.7 | `n = 31`,长度 256 | `Ok`,`val(30)` 为最后 8 B |

### SDK-114 描述符的编译期校验

- 规则:[策略描述符:Note、报表、策略命令](README.md#策略描述符note报表策略命令)
- 级别:table
- 里程碑:M2
- 前置:`trybuild`;基准为 `EchoProbe::DESC`:`notes` = {1 `EchoFire` Info, 2 `EchoOverflow` P2, 3 `EchoNoFeed` P2};`report` = {`fires`: Count、`agg = Count`、`scope = Group`、`note = 1`、`v_ix = 0`};`cmds` = {op 1 `open`(`notional`: Notional 0..=10_000_000_000,`reduce_only = false`)、op 2 `close`(无字段,`reduce_only = true`)、op 3 `set_mode`(`m`: Enum(["x", "y"]))};每行只违反一条
- 实现:`crates/hft-strategy-sdk/tests/ui.rs::sdk_114_descriptor_const_checks`

| 行 | 改动 | 期望 |
|---|---|---|
| SDK-114.1 | 不改 | 编译通过 |
| SDK-114.2 | 两个 Note 的 `code` 都是 1 | 编译失败 |
| SDK-114.3 | 两个 Note 的 `name` 相同 | 编译失败 |
| SDK-114.4 | 某 Note 的 `v` 有 9 项 | 编译失败 |
| SDK-114.5 | 某 Note 的 `v` 有 8 项 | 编译通过 |
| SDK-114.6 | 某 Note 的 `level = P0` | 编译失败 |
| SDK-114.7 | 某 Note 的 `code = 0x0100_0000` | 编译失败 |
| SDK-114.8 | 某 Note 的 `code = 0x00FF_FFFF` | 编译通过 |
| SDK-114.9 | 两个报表字段同名 | 编译失败 |
| SDK-114.10 | 报表字段 `note = 4`(未登记) | 编译失败 |
| SDK-114.11 | 报表字段 `v_ix = 8` | 编译失败 |
| SDK-114.12 | 两个命令 `op = 1` | 编译失败 |
| SDK-114.13 | 某命令 `op = 0` | 编译失败 |
| SDK-114.14 | 两个命令同名 | 编译失败 |
| SDK-114.15 | 某命令 32 个字段 | 编译失败 |
| SDK-114.16 | 某命令字段 `per_group = true` | 编译失败 |
| SDK-114.17 | 某命令字段 `structural = true` | 编译失败 |
| SDK-114.18 | 某命令字段 `minor_change = Any` | 编译失败 |
| SDK-114.19 | 某命令字段 `min > default` | 编译失败 |
| SDK-114.20 | `strategy_set!` 登记一个 Note `level = P0` 的策略 | 编译失败(`strategy_set!` 再断言一次) |
| SDK-114.21 | 某 Note 的 `code = 0` | 编译失败(码 0 保留) |
| SDK-114.22 | `notes` 只保留 `EchoFire`(`code = 1`,最小合法码) | 编译通过 |
| SDK-114.23 | `strategy_set!` 登记一个 Note `code = 0` 的策略 | 编译失败(`strategy_set!` 再断言一次) |

### SDK-115 `CtxHarness` 对未登记的 Note 码或级别断言失败

- 规则:[策略描述符:Note、报表、策略命令](README.md#策略描述符note报表策略命令)、[测试桩(testing 特性)](README.md#测试桩testing-特性)
- 级别:table
- 里程碑:M2
- 前置:`CtxHarness::new(&EchoProbe::DESC)`;每行在 `run` 内调一次 `ctx.note`
- 实现:`crates/hft-strategy-sdk/tests/harness.rs::sdk_115_note_registration_asserted`

| 行 | `note(code, level, group, inst, v)` | 期望 |
|---|---|---|
| SDK-115.1 | `(1, Info, G2, 101, [7, 0, …])` | 记录一条:`code = 1`、`level = Info`、`group = G2`、`inst = 101`、`v[0] = 7` |
| SDK-115.2 | `(1, Info, GROUP_NONE, INST_NONE, [0; 8])` | 记录一条,`group = GROUP_NONE` |
| SDK-115.3 | `(4, Info, G1, 101, [0; 8])`(未登记) | 断言失败(`#[should_panic]`,消息含 `code 4`) |
| SDK-115.4 | `(2, Info, G1, 101, [0; 8])`(登记为 P2) | 断言失败(消息含 `level`) |

### SDK-116 策略 Note 写进 journal 时码按 source 组合、带所属 group

- 规则:[策略描述符:Note、报表、策略命令](README.md#策略描述符note报表策略命令)、[Ctx 与订单意图](README.md#ctx-与订单意图)
- 级别:integ
- 里程碑:M2
- 前置:`SeqRig`;`RecProbe.DESC.notes` 含 code 1 Info;脚本在 G2 的事件里 `note(1, Info, G2, 101, [5, 0, 0, 0, 0, 0, 0, 0])`
- 步骤:
  1. 投递触发该脚本的 Bbo
- 期望:
  - journal 有一条 `Note`:`code = 0x0200_0001`(`Strategy = 2` 左移 24 位或 1)、`level = Info`、`source = Strategy`、`strategy = 3`、`group = G2`、`inst = 101`、`v = [5, 0, 0, 0, 0, 0, 0, 0]`
- 实现:`crates/hft-sequencer/tests/sdk_contract/ctx.rs::sdk_116_strategy_note_record`

### SDK-117 `reduce_only` 的 `Manual` 中策略发出的 Open 类意图被拒

- 规则:[策略描述符:Note、报表、策略命令](README.md#策略描述符note报表策略命令)
- 级别:integ
- 里程碑:M2
- 前置:`SeqRig`;实例 Live、G1 `open_allowed = 1`;G1 的 I2 有空头 0.02;A1 的敞口门不生效(`naked_gate_min` = 1 000 000 USDT;G1 只有一条腿持仓,否则 h1 先被 `EXPOSURE_GATE` 拒绝);`RecProbe` 收到 `Manual` 时依次 `place` 一张 I1 买入 Open IOC(h1)与一张 I2 买入 0.01 的 Reduce IOC(h2)
- 步骤:
  1. 发送 `StrategyCmd{S3, A1, group = G1, reduce_only = true, payload = op 2}`
  2. 下一事件读 `ctx.outcome(h1)`、`ctx.outcome(h2)`
- 期望:
  - h1 的 `OrderDecision.reject = CLOSE_ONLY`(1 << 4)、`slot = 0`、`trigger_seq` = 该命令的 seq;无对应 `GwCmd`
  - h2 的 `OrderDecision.reject = 0`,出环一条 `GwCmd`
  - `outcome(h1) = Some(Rejected(CLOSE_ONLY))`、`outcome(h2) = Some(Placed(_))`
- 实现:`crates/hft-sequencer/tests/sdk_contract/ctx.rs::sdk_117_manual_reduce_only_rejects_open`

### SDK-118 `reduce_only = false` 的命令产生的意图照常经模式门与检查链

- 规则:[策略描述符:Note、报表、策略命令](README.md#策略描述符note报表策略命令)
- 级别:integ
- 里程碑:M2
- 前置:同 SDK-117,但命令 `reduce_only = false`(op 1)
- 步骤:
  1. 发送命令(确认后)
- 期望:
  - h1 放行(`reject = 0`、出环);h2 放行
- 实现:`crates/hft-sequencer/tests/sdk_contract/ctx.rs::sdk_118_manual_not_reduce_only_normal`

### SDK-119 gateway 拒绝与 `CmdDesc.reduce_only` 不一致或载荷非法的 `StrategyCmd`

- 规则:[策略描述符:Note、报表、策略命令](README.md#策略描述符note报表策略命令)
- 级别:table
- 里程碑:M2
- 前置:`SeqRig`;`RecProbe.cmds` 同 `EchoProbe`(SDK-114);每行一条命令
- 实现:`crates/hft-ctl/tests/e2e/sdk.rs::sdk_119_gateway_cmd_checks`

| 行 | 命令 | 期望 |
|---|---|---|
| SDK-119.1 | op 2 `close`,`reduce_only = true` | `Applied`;`RecProbe` 收到 `Manual{reduce_only = true}` |
| SDK-119.2 | op 2 `close`,`reduce_only = false` | `InvalidArgs{ParamInvalid}`;无 `Manual` |
| SDK-119.3 | op 1 `open`,`reduce_only = true` | `InvalidArgs{ParamInvalid}` |
| SDK-119.4 | op 9 | `InvalidArgs{ParamInvalid}` |
| SDK-119.5 | op 1,值 10_000_000_001 | `InvalidArgs{ParamInvalid}` |
| SDK-119.6 | 载荷 257 B | `InvalidArgs{TooLong}` |

### SDK-120 参数编码金样:`encode_body` 逐字节、`decode_body` 往返

- 规则:[参数编解码器(`params` 模块,唯一实现)](README.md#参数编解码器params-模块唯一实现)
- 级别:golden
- 里程碑:M1
- 输入:`fixtures/params/sdk-tp/*.input.toml`(至少:全默认值;每个 per_group 字段各一条覆盖;3 个 group 的乱序覆盖;v1 schema 的旧 body;另有期望为 `ParamError` 的 case,不带 `.blob`)
- 期望文件:`fixtures/params/sdk-tp/<case>.blob` 与 `<case>.meta.toml`(期望结果 `Ok` 或 `ParamError::<变体>`)
- 前置:金样由人按 README 的布局手算核对,不由当前代码生成;本用例把输入中的 refdata 名字(实例 `sdk-probe` = S3、账户 `sdk-a1` = A1、`BTC`、`v1:BTC-SPOT` = I1、`v2:BTC-PERP` = I2)按 `GT3` 映射,`generated_at_ns = 0`;v1 的 case 用 `TP` 的 v1 前缀编码、用 `TP` 以 `schema_ver = 1` 解码
- 实现:`crates/hft-strategy-sdk/tests/params_golden.rs::sdk_120_params_golden`

### SDK-121 hft-cfg 与 hft-py 对同一输入产出相同字节

- 规则:[参数编解码器的使用方](README.md#参数编解码器的使用方)、[参数编解码器(`params` 模块,唯一实现)](README.md#参数编解码器params-模块唯一实现)
- 级别:golden
- 里程碑:M9
- 输入:`fixtures/params/sdk-tp/*.input.toml` 与 SDK-112 的命令 `(op, vals)` 清单
- 期望文件:`fixtures/params/sdk-tp/<case>.blob`
- 前置:hft-cfg 二进制与 hft-py wheel 由同一提交构建
- 备注:hft-cfg 的 strategy blob 编码与 hft-py `params.encode` 对每个 case 与 `.blob` 逐字节相同;非法 case 两者给出同一 `ParamError` 变体;hft-py `encode_cmd` 与 hft-ctl `encode_cmd` 对每条命令逐字节相同
- 实现:`tools/hft-cfg/tests/sdk_wrappers.rs::sdk_121_wrappers_byte_identical`

### SDK-122 `quote_live` 只看连接健康,边界含等号

- 规则:[视图类型](README.md#视图类型)
- 级别:table
- 里程碑:M2
- 前置:`max_silence = 6_000_000_000`(`conn_liveness_ms = 2_000` × `tsc_per_ms = 3_000_000`);`now = 100_000_000_000`;基准 `BboView`:`conn_up = true`、`resync = false`、`conn_live_tsc = now`、`rx_tsc = now`
- 实现:`crates/hft-strategy-sdk/tests/quote.rs::sdk_122_quote_live_boundaries`

| 行 | 改动 | 期望 |
|---|---|---|
| SDK-122.1 | 不改 | `true` |
| SDK-122.2 | `conn_live_tsc = now − 6_000_000_000` | `true` |
| SDK-122.3 | `conn_live_tsc = now − 6_000_000_001` | `false` |
| SDK-122.4 | `conn_live_tsc = now − 5_999_999_999` | `true` |
| SDK-122.5 | `conn_up = false` | `false` |
| SDK-122.6 | `resync = true` | `false` |
| SDK-122.7 | `rx_tsc = now − 30_000_000_000`(报价 10 s 未变),连接仍 `now` | `true` |
| SDK-122.8 | `rx_tsc = now`(报价刚变),`conn_live_tsc = now − 7_000_000_000` | `false` |
| SDK-122.9 | `conn_live_tsc = 0` | `false` |

### SDK-123 `Views::quote_ok` 等于引擎阈值下的 `quote_live`

- 规则:[Views 与 ModelViews](README.md#views-与-modelviews)、[视图类型](README.md#视图类型)
- 级别:prop
- 里程碑:M2
- 性质:对任意 `MockViews` 状态,`v.quote_ok(a, i) == v.bbo(i).quote_live(v.now().tsc, v.risk(a).conn_liveness_ms as u64 × v.now().tsc_per_ms)`
- 生成:`conn_liveness_ms` 1–10_000;`tsc_per_ms` 1_000_000–4_000_000;`conn_live_tsc` 在 [0, now] 内,另含边界 ±1;`conn_up`、`resync` 随机
- 参考:右侧表达式
- 例数:PR 10 000;每夜 1 000 000
- 实现:`crates/hft-strategy-sdk/tests/quote.rs::sdk_123_quote_ok_is_engine_threshold`

### SDK-124 盘口 10 s 不变而连接存活帧持续到达时报价一直可用

- 规则:[视图类型](README.md#视图类型)、[Views 与 ModelViews](README.md#views-与-modelviews)
- 级别:integ
- 里程碑:M2
- 前置:`SeqRig`;`conn_liveness_ms = 2_000`;I2 在 t = 0 收到一条 Bbo,之后 10 s 没有新 Bbo;V2 的 BBO 连接每 500 ms 一条 `ConnState{Alive}`;`RecProbe` 订阅 I2 与一个 100 ms 定时器,每次读 `v.quote_ok(A1, I2)`
- 步骤:
  1. 驱动 10 s
  2. 停止 `Alive`,再驱动 2.6 s
- 期望:
  - 步骤 1 中 100 次读数全为 `true`;`v.bbo(I2).rx_tsc` 始终等于 t = 0 那条 Bbo 的 `rx_tsc`
  - 步骤 2 中最后一次 `Alive` 之后 ≤ 2_000 ms 的读数为 `true`,超过 2_000 ms 的读数为 `false`
- 实现:`crates/hft-sequencer/tests/sdk_contract/views.rs::sdk_124_quiet_book_stays_live`

### SDK-125 `Now` 的时间换算:往返、UTC 与早于本次启动的饱和

- 规则:[时间轴](README.md#时间轴)
- 级别:table
- 里程碑:M2
- 前置:`MockViews::at(Now{tsc: 9_000_000_000, engine_ns: 50_000_000_000, utc_ns: U0, tsc_per_ms: 3_000_000, boot_seq: 100, ..})`,本次启动的 `EngineClock` 基准 `base_engine_ns = 47_000_000_000` 对应 tsc 0
- 实现:`crates/hft-strategy-sdk/tests/time.rs::sdk_125_now_conversions`

| 行 | 调用 | 期望 |
|---|---|---|
| SDK-125.1 | `ens_of(now.tsc)` | `50_000_000_000`(= `now.engine_ns`) |
| SDK-125.2 | `utc_of(now.tsc)` | `U0`(= `now.utc_ns`) |
| SDK-125.3 | `tsc_of(now.engine_ns + 1_000_000_000)` | `12_000_000_000` |
| SDK-125.4 | `ens_of(tsc_of(e))`,e = `now.engine_ns + 123_456_789` | 与 e 相差 ≤ 1 ns |
| SDK-125.5 | `tsc_of(46_999_999_999)`(早于本次启动) | `0` |
| SDK-125.6 | `time::ms(1_500)` | `1_500_000_000` |
| SDK-125.7 | `time::ms(0)` | `0` |

### SDK-126 `ens_of`/`tsc_of` 在本次启动内往返误差不超过 1 ns

- 规则:[时间轴](README.md#时间轴)
- 级别:prop
- 里程碑:M2
- 性质:任意本次启动内的 tsc t,`|tsc_of(ens_of(t)) − t| ≤ ceil(tsc_per_ms / 1_000_000)`;任意 e ≥ 启动基准,`|ens_of(tsc_of(e)) − e| ≤ 1`
- 生成:`tsc_hz_milli` 对应 1–4 GHz;t 在 [0, 2^50];e 在 [基准, 基准 + 10^15]
- 参考:i128 精确计算的同一公式
- 例数:PR 20 000;每夜 2 000 000
- 实现:`crates/hft-strategy-sdk/tests/time.rs::sdk_126_time_roundtrip`

### SDK-127 `MockViews::restart`:TSC 重新计数、engine_ns 连续、boot_seq 前进、行情与连接清空

- 规则:[测试桩(testing 特性)](README.md#测试桩testing-特性)、[时间轴](README.md#时间轴)
- 级别:unit
- 里程碑:M2
- 前置:`MockViews` 在 `Now{tsc: 9_000_000_000, engine_ns: 50_000_000_000, boot_seq: 100}`,I1 有 BBO、连接健康
- 步骤:
  1. `restart(new_tsc = 1_000, new_boot_seq = 900)`
  2. 读 `now()`、`bbo(I1)`、`quote_ok(A1, I1)`
- 期望:
  - `now().tsc = 1_000`、`now().engine_ns = 50_000_000_000`、`now().boot_seq = 900`
  - `bbo(I1).rx_tsc = 0`、`conn_live_tsc = 0`、`conn_up = false`;`quote_ok = false`
- 实现:`crates/hft-strategy-sdk/tests/mock_views.rs::sdk_127_mock_restart`

### SDK-128 按 engine_ns 保存的冷却跨重启只差停机时长

- 规则:[时间轴](README.md#时间轴)、[确定性规则](README.md#确定性规则)
- 级别:unit
- 里程碑:M2
- 前置:`EchoProbe`,`cooldown_ms = 20_000`;在 engine_ns E 触发一次(G1);`snapshot` 得 B
- 步骤:
  1. E + 5 s 时 `MockViews::restart(new_tsc = 7, new_boot_seq = 900)`(停机 1 h:UTC 前进 3 600 s、engine_ns 不前进),新实例 `restore(B)`
  2. 引擎时间 E + 19.999 s 投递满足条件的 Bbo
  3. 引擎时间 E + 20 s 再投递
- 期望:
  - `B` 中不含任何 TSC 值(只含 E + 20 s 的 engine_ns 截止)
  - 步骤 2 无 `place`;步骤 3 恰一条 `place`(group G1)
- 实现:`crates/hft-strategy-sdk/tests/probe_fast.rs::sdk_128_cooldown_survives_restart`

### SDK-129 `BboView.rx_tsc` 为 0 表示未知,`MockViews::bbo_rx` 能设出这一状态

- 规则:[视图类型](README.md#视图类型)、[测试桩(testing 特性)](README.md#测试桩testing-特性)
- 级别:unit
- 里程碑:M2
- 前置:`MockViews`;`bbo(I1, (6_500_000, 100_000_000), (6_500_100, 100_000_000))` 于 `now.tsc = 5_000`
- 步骤:
  1. 读 `bbo(I1).rx_tsc`
  2. `bbo_rx(I1, 0, 0, RxTsSrc::None)` 后读 `bbo(I1)`
  3. `conn(I1, 5_000, true, false)` 后读 `bbo(I1)`
- 期望:
  - 步骤 1:`5_000`
  - 步骤 2:`rx_tsc = 0`、`hw_rx_ns = 0`、`rx_ts_src = None`;`bid_px = 6_500_000` 不变
  - 步骤 3:报价字段与 `rx_tsc` 不变,`conn_live_tsc = 5_000`、`conn_up = true`
- 实现:`crates/hft-strategy-sdk/tests/mock_views.rs::sdk_129_rx_tsc_zero_unknown`

### SDK-130 `rx_ts_src` 由 Bbo 记录头的 `HW_TS`/`SW_TS` 位投影

- 规则:[视图类型](README.md#视图类型)
- 级别:table
- 里程碑:M2
- 前置:`SeqRig`;每行投递一条 I1 的 Bbo,`MarketHdr.hw_rx_ns = 1_700_000_000_123_456_789`(无戳的行为 0)
- 实现:`crates/hft-sequencer/tests/sdk_contract/views.rs::sdk_130_rx_ts_src_projection`

| 行 | 记录头位 | 期望 `b.rx_ts_src` / `b.hw_rx_ns` |
|---|---|---|
| SDK-130.1 | `HW_TS` | `Hw` / 1_700_000_000_123_456_789 |
| SDK-130.2 | `SW_TS` | `Sw` / 1_700_000_000_123_456_789 |
| SDK-130.3 | 都不置(`hw_rx_ns = 0`) | `None` / 0 |

### SDK-131 `Views::leverage` 把 Rate 换成百分之一倍并向下取整

- 规则:[Views 与 ModelViews](README.md#views-与-modelviews)
- 级别:table
- 里程碑:M2
- 前置:`SeqRig`;每行投递一条 A1 在 V2 的 `AccountConfigReport`,`LeverageItem{inst = 102, lev}`;另有现货 inst 101
- 实现:`crates/hft-sequencer/tests/sdk_contract/views.rs::sdk_131_leverage_x100`

| 行 | `lev`(Rate)/ 查询 | 期望 |
|---|---|---|
| SDK-131.1 | 20_000_000_000(20x)/ `leverage(A1, 102)` | `Some(2_000)` |
| SDK-131.2 | 2_500_000_000(2.5x) | `Some(250)` |
| SDK-131.3 | 2_509_000_000(2.509x) | `Some(250)` |
| SDK-131.4 | 9_999_999(0.0999…x) | `Some(0)` |
| SDK-131.5 | 0 | `None` |
| SDK-131.6 | 尚无报告 / `leverage(A1, 102)` | `None` |
| SDK-131.7 | 20x / `leverage(A1, 101)`(现货) | `None` |

### SDK-132 `Views::risk_for` 按 inst 所属价格带类别取值,不取账户内最小值

- 规则:[Views 与 ModelViews](README.md#views-与-modelviews)、[视图类型](README.md#视图类型)
- 级别:integ
- 里程碑:M2
- 前置:`SeqRig`;`RiskParams` 价格带类别 A:`rr_max_cross = 2_000_000`、`ioc_max_cross = 1_000_000`、`max_px_dev = 5_000_000`;类别 B:`rr_max_cross = 10_000_000`、`ioc_max_cross = 5_000_000`、`max_px_dev = 20_000_000`;I1 属 A、I2 属 B;`l3_max_cross = 0`;`ExecParams.force_slice.max_cross = 2_000_000`(hft-risk 启动校验 `ExecRiskMismatch`:不得大于账户内最小的 `rr_max_cross`)
- 步骤:
  1. 读 `v.risk_for(A1, I1)` 与 `v.risk_for(A1, I2)`
- 期望:
  - I1:`rr_max_cross = 2_000_000`、`ioc_max_cross = 1_000_000`、`max_px_dev = 5_000_000`
  - I2:`rr_max_cross = 10_000_000`、`ioc_max_cross = 5_000_000`、`max_px_dev = 20_000_000`;两者 `l3_max_cross = 0`
- 实现:`crates/hft-sequencer/tests/sdk_contract/views.rs::sdk_132_risk_for_per_class`

### SDK-133 `Views::order` 在槽代数不符时返回 `None`

- 规则:[Views 与 ModelViews](README.md#views-与-modelviews)、[视图类型](README.md#视图类型)
- 级别:integ
- 里程碑:M2
- 前置:`SeqRig`;`RecProbe` 下一张 GTC 得到 `OrderRef{slot = s, gen = g}`
- 步骤:
  1. 读 `v.order(OrderRef{slot = s, gen = g})`
  2. 撤单完成(`CancelAck`)后推进 7 s(槽在 OMS `reuse_quarantine_ms` 之后由 sweep 放回空闲表队尾),再逐张下 GTC 直到一张落在槽 s 上(`gen = g + 1`),读 `v.order(OrderRef{slot = s, gen = g})`
- 期望:
  - 步骤 1:`Some(o)`,`o.state = Live`、`o.group = G1`、`o.class = Open`、`o.tag` 等于下单时的值
  - 步骤 2:`None`
- 实现:`crates/hft-sequencer/tests/sdk_contract/views.rs::sdk_133_order_gen_mismatch_none`

### SDK-134 `Views::target` 与 `ModelViews` 的 `target`/`order`

- 规则:[Views 与 ModelViews](README.md#views-与-modelviews)
- 级别:integ
- 里程碑:M2
- 前置:`SeqRig` + `AdvRig`;`RecProbe` 对 G2 设 `TakerSlice` 目标(`tag = 5`)并下一张 GTC
- 步骤:
  1. 下一事件读 `v.target(A1, G2)`;`RecModel` 在同一 seq 读 `v.target(A1, G2)` 与 `v.order(r)`
  2. `clear_target(A1, G2)` 后读 `v.target(A1, G2)`
- 期望:
  - 步骤 1:快速部分 `Some(t)`,`t.intent.tag = 5`、`t.origin = Strategy`;Model 侧两者都是 `None`
  - 步骤 2:`None`
- 实现:`crates/hft-sequencer/tests/sdk_contract/views.rs::sdk_134_target_view_and_model_side`

### SDK-135 `Views::book` 的状态与逐档读取

- 规则:[Views 与 ModelViews](README.md#views-与-modelviews)
- 级别:integ
- 里程碑:M2
- 前置:`SeqRig`;本用例的 I2 = 填充 inst 15(同 SDK-043:`l2_depth = 5`,不属于任何 group),订阅 `BOOK`(`DepthTopN`,N = 5),I1 只订阅 `BBO`
- 步骤:
  1. 投递 I2 的 5 档快照前读 `v.book(I2).status()`
  2. 投递后读 `best(Bid)`、`levels(Bid, &mut [Level; 3])`、`levels(Ask, &mut [Level; 8])`
  3. 读 `v.book(I1).status()`
- 期望:
  - 步骤 1:`Uninit`
  - 步骤 2:`best(Bid)` = 第 1 档;3 档缓冲返回 3(最优的 3 档,由内向外);8 档缓冲返回 5
  - 步骤 3:`Uninit`
- 实现:`crates/hft-sequencer/tests/sdk_contract/views.rs::sdk_135_book_read`

### SDK-136 `FundingView` 的 `prev_interval_s` 与 `upd_tsc`

- 规则:[视图类型](README.md#视图类型)
- 级别:integ
- 里程碑:M2
- 前置:`SeqRig`;I2 订阅 `FUNDING`
- 步骤:
  1. 投递 `Funding{interval_s = 28_800}`(rx_tsc T1)
  2. 投递 `Funding{interval_s = 14_400}`(rx_tsc T2)
  3. 写一个关键帧,以该快照冷启动恢复,再读 `v.funding(I2)`
- 期望:
  - 步骤 1 后:`interval_s = 28_800`、`prev_interval_s = 28_800`、`upd_tsc = T1`
  - 步骤 2 后:`interval_s = 14_400`、`prev_interval_s = 28_800`、`upd_tsc = T2`
  - 步骤 3 后:`interval_s = 14_400`、`prev_interval_s = 28_800`、`upd_tsc = 0`
- 实现:`crates/hft-sequencer/tests/sdk_contract/views.rs::sdk_136_funding_view`

### SDK-137 `PortfolioView.equity_total` 等于各 venue 汇总行之和

- 规则:[视图类型](README.md#视图类型)
- 级别:integ
- 里程碑:M2
- 前置:`SeqRig`;A1 在 V1 有 USDT 10_000 与 BTC 0.1(首条 `BalanceReport`,同 SDK-062)、在 V2 有永续多头 0.1 与保证金 5_000;I1 bid 65_000.00,I2 mark 65_100.00
- 步骤:
  1. 读 `p = v.portfolio(A1)`:`equity_total()`、`venue(V1).equity`、`venue(V2).equity`
- 期望:
  - `equity_total() = venue(V1).equity + venue(V2).equity`;`venue(V1).equity = 16_500_000_000`(10_000 + 0.1 × 65_000);`p.account() = A1`
- 实现:`crates/hft-sequencer/tests/sdk_contract/views.rs::sdk_137_equity_total_sum`

### SDK-138 `MarginView.stale` 按 `margin_stale_ms` 判定

- 规则:[视图类型](README.md#视图类型)
- 级别:table
- 里程碑:M2
- 前置:`SeqRig`;`margin_stale_ms = 10_000`;V2 的 `MarginReport` 在引擎时间 E 到达;每行在给定引擎时间读 `v.portfolio(A1).margin(V2)`
- 实现:`crates/hft-sequencer/tests/sdk_contract/views.rs::sdk_138_margin_stale`

| 行 | 读取时的 engine_ns | 期望 |
|---|---|---|
| SDK-138.1 | E + 9_999 ms | `stale = false`、`src_ens = E` |
| SDK-138.2 | E + 10_000 ms | `stale = false` |
| SDK-138.3 | E + 10_001 ms | `stale = true` |

### SDK-139 `Views::venue_ready` 反映 Unknown 槽与下单连接

- 规则:[Views 与 ModelViews](README.md#views-与-modelviews)
- 级别:integ
- 里程碑:M2
- 前置:`SeqRig`;V1 下单连接 Up、无 Unknown 槽
- 步骤:
  1. 读 `venue_ready(A1, V1)`
  2. `RecProbe` 的一张 IOC 发出后连接断开、无回执(槽进入 Unknown),读一次
  3. 查单收敛为 `Filled` 且下单连接回到 Up,读一次
- 期望:
  - 步骤 1 `true`;步骤 2 `false`;步骤 3 `true`
- 实现:`crates/hft-sequencer/tests/sdk_contract/views.rs::sdk_139_venue_ready`

### SDK-140 `Views` 的全部访问器在 `on_event` 中零分配

- 规则:[Views 与 ModelViews](README.md#views-与-modelviews)
- 级别:integ
- 里程碑:M2
- 前置:`SeqRig` 以计数全局分配器运行(独立测试目标 `sdk_alloc`);`RecProbe` 有一张 Live 的 GTC(供 `order` 读),在每个事件里依次调用 `now`、`bbo`、`book`(+ `levels`)、`funding`、`mark`、`portfolio`(+ 其全部方法)、`group_pos`、`legs`、`target`、`order`、`inst`、`caps`、`fee`、`mode`、`risk`、`risk_for`、`venue_ready`、`leverage`、`quote_ok`
- 步骤:
  1. 计数器清零后投递 1 000 条 Bbo
- 期望:
  - 1 000 次调用期间分配次数 0(以 `on_event` 内调用访问器前后的计数差计;sequencer 自身与测试集自身的投递日志不在本断言范围)
- 实现:`crates/hft-sequencer/tests/sdk_alloc.rs::sdk_140_views_zero_alloc`

### SDK-141 `#[repr(C)]` 视图与意图结构的大小、对齐与关键偏移

- 规则:[视图类型](README.md#视图类型)、[Ctx 与订单意图](README.md#ctx-与订单意图)、[目标意图](README.md#目标意图)
- 级别:table
- 里程碑:M2
- 前置:无(编译期 `const` 断言 + 运行时 `offset_of!` 断言)
- 实现:`crates/hft-strategy-sdk/tests/layout.rs::sdk_141_repr_c_layouts`

| 行 | 结构 | 大小 / 对齐 | 关键偏移 |
|---|---|---|---|
| SDK-141.1 | `VenueEquityView` | 56 / 8 | `flags` 2、`equity` 8、`margin_balance` 40、`as_of_ens` 48 |
| SDK-141.2 | `LegTrack` | 32 / 8 | `inflight_immediate` 8、`inflight_resting` 16、`residual` 24 |
| SDK-141.3 | `LegView` | 160 / 8 | `naked_qty` 128、`naked_ratio` 144、`n_legs` 152、`has_unknown` 154、`has_pending` 155 |
| SDK-141.4 | `OrderIntent` | 112 / 8 | `class` 4、`flags` 5、`inst` 8、`group` 12、`px` 16、`qty` 24、`link` 32、`tag` 104、`max_rest_ms` 108 |
| SDK-141.5 | `LegLinkOpt` | 72 / 8 | `some` 0、`l` 8 |
| SDK-141.6 | `LegLink` | 64 / 8 | `role` 4、`tolerance` 8、`hedge` 16 |
| SDK-141.7 | `HedgePlan` | 48 / 8 | `reduce_only` 7、`inst` 8、`ratio_num` 12、`ratio_den` 16、`residual` 20、`px_param` 24、`max_slip` 32、`limit_px` 40 |
| SDK-141.8 | `OrderRef` | 12 / 4 | `slot` 4、`gen` 8 |
| SDK-141.9 | `TargetIntent` | 232 / 8 | `group` 4、`legs` 8、`n_legs` 72、`reason` 73、`urgency` 74、`band` 80、`exec` 88、`tag` 224 |
| SDK-141.10 | `LegTarget` | 16 / 8 | `qty` 8 |
| SDK-141.11 | `ExecSpec` | 136 / 8 | 判别字节 0、载荷 8 |
| SDK-141.12 | `TakerSliceParams` | 48 / 8 | `depth_levels` 8、`max_cross` 16、`max_chunk_notional` 24、`min_chunk_notional` 32、`cooldown_ms` 40 |
| SDK-141.13 | `PairParams` | 96 / 8 | `first_leg` 48、`first_leg_ix` 49、`hedge_px_mode` 50、`hedge_cross` 56、`hedge_max_slip` 64、`ratio_num` 72、`fix_tolerance` 80、`residual` 88 |
| SDK-141.14 | `SimulParams` | 72 / 8 | `on_fail` 48、`fix_tolerance` 56、`residual` 64 |
| SDK-141.15 | `MakerParams` | 128 / 8 | `max_rest_ms` 16、`chunk_notional` 24、`hedge` 32 |

### SDK-142 `#[repr(C)]` 结构没有隐式填充

- 规则:[Ctx 与订单意图](README.md#ctx-与订单意图)、[目标意图](README.md#目标意图)
- 级别:unit
- 里程碑:M2
- 前置:SDK-141 的 15 个结构
- 步骤:
  1. 对每个结构断言 `size_of` = 各字段 `size_of` 之和(`ExecSpec` 除外:判别字节 1 + 显式说明的 7 B 对齐 + 128 B 载荷;`TargetIntent` 的 `exec` 字段按 136 B 计)
  2. 对除 `ExecSpec` 与含它的 `TargetIntent` 外的 13 个结构做 `zerocopy::IntoBytes` 派生(要求无填充)
- 期望:
  - 步骤 1 全部相等;步骤 2 编译通过(加一个去掉 `_pad` 的变体 → 编译失败,`trybuild`)
- 实现:`crates/hft-strategy-sdk/tests/layout.rs::sdk_142_no_implicit_padding`

### SDK-143 SDK 枚举的判别值与线上码表逐值相同

- 规则:[视图类型](README.md#视图类型)、[EventRef](README.md#eventref)、[Ctx 与订单意图](README.md#ctx-与订单意图)、[目标意图](README.md#目标意图)、[Strategy 与 Model](README.md#strategy-与-model)
- 级别:table
- 里程碑:M2
- 前置:hft-testkit 依赖全部引擎 crate,能同时看到 SDK、hft-event 与 hft-exec 的常量
- 实现:`crates/hft-sequencer/tests/sdk_contract/codes.rs::sdk_143_enum_codes_match_wire`

| 行 | SDK 枚举 | 期望(判别值) | 对照 |
|---|---|---|---|
| SDK-143.1 | `IntentClass` | Open 1、Reduce 2、Hedge 3、Flatten 4 | hft-event `OrderDecision` 体的一致性断言 |
| SDK-143.2 | `LinkRole` | First 1、Second 2、SecondRetry 3、HedgeFix 4、Reverse 5 | hft-event `OrderDecision.link_role`(0 只表示无链接) |
| SDK-143.3 | `HedgeKind` / `PxMode` / `OnFail` / `ResidualPolicy` | None 0、OnFill 1、Simultaneous 2 / Cross 1、Mid 2、Limit 3 / Retry 1、Reverse 2、Hold 3 / Defer 1、TrimExcess 2、Hold 3 | hft-oms 读取的同名类型 |
| SDK-143.4 | `TargetReason` / `Urgency` | Rebalance 1 … Manual 5 / Passive 1、Normal 2、Urgent 3 | hft-event `ExecDecision.reason` / `urgency` |
| SDK-143.5 | `ExecSpec` 判别值 | TakerSlice 1、PairOnFill 2、Simultaneous 3、MakerFirst 4 | hft-event `ExecDecision.algo` |
| SDK-143.6 | `FirstLeg` / `SimulOnFail` | ByLiquidity 1、PerpFirst 2、SpotFirst 3、Fixed 4 / Reverse 1、Hold 2 | hft-exec 的同名码 |
| SDK-143.7 | `TargetReject` | NotOwner 1 … LotMismatch 10、ReduceOnlyCmd 11(11 个) | hft-exec `ExecDecision.detail` 表 1 |
| SDK-143.8 | `StuckReason` | Gated 1、Shadow 2、NoLiquidity 3、StaleQuote 4、SliceFails 5、External 6、EngineOverride 7、MinOrder 8 | hft-exec `detail` 表 2 |
| SDK-143.9 | `BrokenReason` | LinkBroken 1、ReverseFailed 2、FixExhausted 3、HedgeVeto 4 | hft-exec `detail` 表 3 |
| SDK-143.10 | `ExternalCause` | Liquidation 1、Adl 2、Manual 3、Delivery 4、Unknown 5 | hft-event `ExternalFill.cause` |
| SDK-143.11 | `TargetOrigin` | 6 个值各经 `TargetOriginWire::from` 得码 1..=6(`Strategy` 1 … `Engine(AutoFix)` 6),再经 `TargetOrigin::from` 回到原值 | hft-event `TargetOriginWire` / `ExecDecision.origin` |
| SDK-143.12 | `ResetCause` | Structural 1、Fingerprint 2、PctConfig 3 | hft-event `Note{ModelGroupReset}` 的 `v[0]`(`ResetAll` = 4 不是本枚举的值) |
| SDK-143.13 | `RxTsSrc` | None 0、Hw 1、Sw 2 | — |
| SDK-143.14 | `NoteLevel`(重导出) | P0 1、P1 2、P2 3、Info 4 | hft-event `Note.level` |

### SDK-144 `ExecUpdate.inflight` 只计即时在途

- 规则:[EventRef](README.md#eventref)
- 级别:integ
- 里程碑:M2
- 前置:`SeqRig`;`RecProbe` 对 G1 设 `PairOnFill` 目标;同时它在 I1 上有一张已 Ack 的 GTC 0.05(挂单在途)
- 步骤:
  1. 执行层发出第一腿 IOC 0.01,`MockVenue` 成交 0.01;第二腿 IOC 0.01 已发出、未回执
  2. 第一笔 `Exec{Progress}` 投递时读 `u.inflight`
- 期望:
  - 第二腿的 `u.inflight` = 该腿 0.01 的即时在途(带符号:卖出为 −1_000_000);第一腿的 `u.inflight` 不含 GTC 的 0.05(= 0);`u.origin = Strategy`
- 实现:`crates/hft-sequencer/tests/sdk_contract/delivery.rs::sdk_144_exec_inflight_immediate_only`

### SDK-145 `Ctx` 每类意图的容量:第 N+1 个返回 `CtxFull` 并计数,已接受的不受影响

- 规则:[Ctx 与订单意图](README.md#ctx-与订单意图)、[Ctx 的内存与排空](README.md#ctx-的内存与排空)
- 级别:table
- 里程碑:M2
- 前置:`CtxHarness::new(&EchoProbe::DESC)`;每行一次 `run`,按该行连续调用 N + 1 次
- 实现:`crates/hft-strategy-sdk/tests/ctx.rs::sdk_145_ctx_capacities`

| 行 | 调用 | N | 第 N 次 / 第 N+1 次 | `Recorded` |
|---|---|---|---|---|
| SDK-145.1 | `place` | 16 | `Ok(IntentHandle)` / `Err(CtxFull)` | `places.len() = 16`、`full = 1` |
| SDK-145.2 | `target` 与 `clear_target` 交替 | 64 | `Ok` / `Err(CtxFull)` | `targets.len() + clears.len() = 64`、`full = 1` |
| SDK-145.3 | `cancel` 与 `amend` 交替 | 16 | `Ok` / `Err(CtxFull)` | `cancels.len() = 16`(含改单)、`full = 1` |
| SDK-145.4 | `timer_at` | 8 | `Ok(TimerId)` / `Err(CtxFull)` | `timers.len() = 8`、`full = 1` |
| SDK-145.5 | `note`(登记过的码) | 16 | 记录 / 丢弃 | `notes.len() = 16`、`full = 1` |
| SDK-145.6 | `request_halt` | 4 | `Ok` / `Err(CtxFull)` | `halts.len() = 4`、`full = 1` |
| SDK-145.7 | `place` 16 次后 `target` 1 次 | — | `target` 返回 `Ok` | 各类容量互不占用 |

### SDK-146 容量常量与 README 表一致

- 规则:[Ctx 与订单意图](README.md#ctx-与订单意图)、[Ctx 的内存与排空](README.md#ctx-的内存与排空)
- 级别:unit
- 里程碑:M2
- 前置:无
- 步骤:
  1. 读 `src/ctx.rs` 导出的容量常量
- 期望:
  - place 16、target + clear 64、cancel/amend 16、timer 8、note 16、request_halt 4;操作序列容量 128;`chain` 特性下 swap 4
- 实现:`crates/hft-strategy-sdk/tests/ctx.rs::sdk_146_capacity_constants`

### SDK-147 引擎侧:`Ctx` 满时超出的意图不产生决策,其余照常

- 规则:[Ctx 与订单意图](README.md#ctx-与订单意图)
- 级别:integ
- 里程碑:M2
- 前置:`SeqRig`;实例 Live;`RecProbe` 在一次 `on_event` 内对 17 个不同的 tag 依次 `place` 17 张 I1 Open IOC
- 步骤:
  1. 投递触发脚本的 Bbo
- 期望:
  - 第 17 次 `place` 返回 `Err(CtxFull)`;该 Input 的 Output 中恰 16 条策略来源的 `OrderDecision{New}`,`intent_no` 连续(= 各次 `place` 的 `IntentHandle`,0..15),下一事件经 `v.order` 读得的 `tag` 为前 16 个
- 实现:`crates/hft-sequencer/tests/sdk_contract/ctx.rs::sdk_147_ctx_full_engine_side`

### SDK-148 意图按调用顺序处理:先撤后下、先 clear 后 target

- 规则:[Ctx 的内存与排空](README.md#ctx-的内存与排空)、[Ctx 与订单意图](README.md#ctx-与订单意图)
- 级别:integ
- 里程碑:M2
- 前置:`SeqRig`;`RecProbe` 有一张 Live 的 GTC(槽 s1),G2 有一个当前目标 T0
- 步骤:
  1. 一次 `on_event` 内依次调用 `cancel(s1)`、`place(新 IOC)`、`clear_target(A1, G2)`、`target(T1 for G2)`
  2. 另一次运行中按相反顺序 `target(T1)`、`clear_target(A1, G2)`
- 期望:
  - 步骤 1:Output 顺序为 `OrderDecision{Cancel, target_slot = s1}`、`OrderDecision{New}`、`ExecDecision{TargetCleared}`、`ExecDecision{TargetSet}`;之后 `v.target(A1, G2)` 的 `intent` = T1
  - 步骤 2:`ExecDecision{TargetReplaced}` 在 `TargetCleared` 之前;之后 `v.target(A1, G2) = None`
- 实现:`crates/hft-sequencer/tests/sdk_contract/ctx.rs::sdk_148_ops_in_call_order`

### SDK-149 `Ctx::outcome` 只对紧邻的上一次 `on_event` 有效

- 规则:[Ctx 与订单意图](README.md#ctx-与订单意图)、[Ctx 的内存与排空](README.md#ctx-的内存与排空)
- 级别:integ
- 里程碑:M2
- 前置:`SeqRig`;实例 Live
- 步骤:
  1. 事件 1:`h = place(Open IOC)`
  2. 事件 2:读 `outcome(h)`,不发任何意图
  3. 事件 3:读 `outcome(h)`
- 期望:
  - 步骤 1 `h = IntentHandle(0)`;步骤 2 `Some(Placed(r))`,r 等于该 `OrderDecision` 的 (slot, gen);步骤 3 `None`
- 实现:`crates/hft-sequencer/tests/sdk_contract/ctx.rs::sdk_149_outcome_previous_call_only`

### SDK-150 `CtxHarness::set_outcomes` 模拟引擎对上一事件的处理结果

- 规则:[测试桩(testing 特性)](README.md#测试桩testing-特性)、[Ctx 与订单意图](README.md#ctx-与订单意图)
- 级别:unit
- 里程碑:M2
- 前置:`CtxHarness::new(&TargetProbe::DESC)`
- 步骤:
  1. `run` 内 `target(T)` 得到 `h = IntentHandle(0)`
  2. `set_outcomes(&[TargetRejected(TargetReject::LotMismatch)])`
  3. 下一次 `run` 内读 `outcome(IntentHandle(0))`、`outcome(IntentHandle(1))`
- 期望:
  - `Some(TargetRejected(LotMismatch))`、`None`
- 实现:`crates/hft-strategy-sdk/tests/harness.rs::sdk_150_set_outcomes`

### SDK-151 门控拒绝的位来自 `RejectBits`,链接第一腿的第二腿检查给出 `LINK_STALE`/`LINK_FUNDS`

- 规则:[Ctx 与订单意图](README.md#ctx-与订单意图)
- 级别:table
- 里程碑:M2
- 前置:`SeqRig`;实例 Live、`open_allowed = 1`;`RecProbe` 在 G1 上 `place` I1 买入 Open IOC 0.01,带 `LegLink{role: First, hedge: OnFill 卖出 I2 0.01}`;`conn_liveness_ms = 2_000`;每行只改该行条件
- 实现:`crates/hft-sequencer/tests/sdk_contract/ctx.rs::sdk_151_gate_bits_link_checks`

| 行 | 条件 | 期望 `outcome` |
|---|---|---|
| SDK-151.1 | I2 连接健康、V2 可用保证金充足 | `Placed(_)` |
| SDK-151.2 | I2 的 BBO 连接静默 2 001 ms | `Rejected(LINK_STALE)`(1 << 28) |
| SDK-151.3 | I2 盘口 30 s 未变但连接每 500 ms 有 `Alive`(期间 V2 `MarginReport` 每 5 s 一条,保证金数据不过期) | `Placed(_)` |
| SDK-151.4 | V2 可用保证金为 0 | `Rejected(LINK_FUNDS)`(1 << 29) |
| SDK-151.5 | 账户 `Kill` 生效 | `Rejected(bits)`,`bits` 含 `KILLED` |

### SDK-152 `request_halt` 对本实例一个 cell 设 Halted cap,只有 `Resume{Halted}` 解除

- 规则:[Ctx 与订单意图](README.md#ctx-与订单意图)
- 级别:integ
- 里程碑:M2
- 前置:`SeqRig`;G2 有效模式 Live
- 步骤:
  1. `RecProbe` 调 `request_halt(G2, reason = 42)`
  2. 发送 `SetMode{S3, G2, A1, Live}`
  3. 发送 `Resume{Halted, scope = (S3, A1, G2)}`(确认后)
- 期望:
  - 步骤 1 之后 `RecProbe` 恰收到一次 `Mode{gix = 1, old = Live, new = CloseOnly}`;`v.mode(A1, G2) = CloseOnly`;G1、G3 仍为 Live;该 `Halted` cap 的 `CapRecord.params[0] = 42`(经 `Note{Risk, CapSet}` 的 `v[0..6]` 读出),作用域 (S3, A1, G2)
  - 步骤 2 后 `v.mode(A1, G2)` 仍为 `CloseOnly`
  - 步骤 3 后 `Mode{old = CloseOnly, new = Live}`,`v.mode(A1, G2) = Live`
- 实现:`crates/hft-sequencer/tests/sdk_contract/ctx.rs::sdk_152_request_halt_cap`

### SDK-153 `cancel_timer` 撤销已登记的定时器

- 规则:[Ctx 与订单意图](README.md#ctx-与订单意图)
- 级别:integ
- 里程碑:M2
- 前置:`SeqRig`;`RecProbe` 在事件 1 `id = timer_at(+5 s, tag 9, G1)`
- 步骤:
  1. 事件 2 中 `cancel_timer(id)`
  2. 推进 10 s
- 期望:
  - 步骤 2 期间 `RecProbe` 没有 `tag = 9` 的 `Timer`
- 实现:`crates/hft-sequencer/tests/sdk_contract/ctx.rs::sdk_153_cancel_timer`

### SDK-154 带 `LegLink` 的第一腿只由策略 `place`,第二腿由 OMS 产出

- 规则:[Ctx 与订单意图](README.md#ctx-与订单意图)
- 级别:integ
- 里程碑:M2
- 前置:`SeqRig`;实例 Live;`RecProbe` 在 G1 上 `place` I1 买入 Open IOC 0.02,`link = LegLinkOpt{some: 1, l: LegLink{group: G1, role: First, tolerance: 10_000_000, hedge: HedgePlan{kind: OnFill, px_mode: Cross, on_fail: Retry, tif: Ioc, venue: V2, account: A1, side: Sell, reduce_only: 0, inst: 102, ratio_num: 1, ratio_den: 1, residual: Defer, px_param: 1_000_000, max_slip: 2_000_000, limit_px: 0}}}`
- 步骤:
  1. `MockVenue` 对第一腿回成交 0.01、再回成交 0.01
- 期望:
  - Output 依次:第一腿 `OrderDecision{link_role = 1}`;每个成交增量各一条第二腿 `OrderDecision{link_role = 2, side = Sell, inst = 102, qty = 1_000_000, tif = Ioc, target_slot = 第一腿槽}`,`oflags` 不含 `REDUCE_ONLY`
  - `RecProbe` 只收到第一腿的 `Order` 事件;第二腿不产生给策略的 `Order`
- 实现:`crates/hft-sequencer/tests/sdk_contract/intent.rs::sdk_154_second_leg_by_oms`

### SDK-155 平仓链接的第二腿带 reduceOnly

- 规则:[Ctx 与订单意图](README.md#ctx-与订单意图)
- 级别:integ
- 里程碑:M2
- 前置:同 SDK-154,但第一腿为 I1 卖出 `Reduce` 0.01(持有现货 0.01、永续空头 0.01,逐腿建仓,建仓期间 A1 的敞口门不生效:`naked_gate_min` = 1 000 000 USDT),`HedgePlan{side: Buy, reduce_only: 1}`;V2 的 `VenueCaps.reduce_only` 支持
- 步骤:
  1. 第一腿全部成交
- 期望:
  - 第二腿 `OrderDecision{link_role = 2, side = Buy, qty = 1_000_000}`,`oflags` 含 `REDUCE_ONLY`
- 实现:`crates/hft-sequencer/tests/sdk_contract/intent.rs::sdk_155_close_link_second_reduce_only`

### SDK-156 `Simultaneous` 链接的第二腿在第一腿 `place` 的同一事件内发出

- 规则:[Ctx 与订单意图](README.md#ctx-与订单意图)
- 级别:table
- 里程碑:M2
- 前置:`SeqRig`;实例 Live;`RecProbe` 在 G1 上 `place` 第一腿 I1 买入 Open IOC 0.01,`HedgePlan{kind: Simultaneous, tif: Ioc, side: Sell, inst: 102, ratio_num: 1, ratio_den: 1, …}`;每行改 `on_fail`
- 实现:`crates/hft-sequencer/tests/sdk_contract/intent.rs::sdk_156_simultaneous_same_event`

| 行 | `on_fail` | 期望 |
|---|---|---|
| SDK-156.1 | `Reverse` | `outcome = Placed(_)`;同一 Input 的 Output 中第二腿 `OrderDecision{link_role = 2, inst = 102, qty = 1_000_000, reject = 0}` 紧邻第一腿 `OrderDecision{link_role = 1}` 之前(hft-sequencer "OMS 输出的排空":`hedges` 先于 `decisions`) |
| SDK-156.2 | `Hold` | 同 .1 |

### SDK-157 GTC 挂单超过 `max_rest_ms` 后收到 `Order{Aged}`

- 规则:[Ctx 与订单意图](README.md#ctx-与订单意图)、[EventRef](README.md#eventref)
- 级别:integ
- 里程碑:M2
- 前置:`SeqRig`;`OmsSweep` 周期 1 000 ms;`RecProbe` 在 t = 0 `place` 一张 I1 的 GTC,`max_rest_ms = 3_000`,`MockVenue` 立即 Ack、不成交
- 步骤:
  1. 推进 6 s
- 期望:
  - `RecProbe` 恰收到一次 `Order{u.kind = Aged}`,所在 Input 的引擎时间在 [3 000 ms, 4 000 ms] 内;此前只有 `Acked`
- 实现:`crates/hft-sequencer/tests/sdk_contract/intent.rs::sdk_157_max_rest_aged`

### SDK-158 `set_target` 的每个拒绝原因经 `Ctx::outcome` 返回

- 规则:[目标意图](README.md#目标意图)、[Ctx 与订单意图](README.md#ctx-与订单意图)
- 级别:table
- 里程碑:M2
- 前置:`SeqRig`;实例 Live、`open_allowed = 1`;基准目标 T = `TargetIntent{account: A1, group: G1, n_legs: 2, legs: [(0, +0.01), (1, −0.01)], reason: Rebalance, urgency: Normal, band: 1_000_000, exec: PairOnFill{..}}`;两腿 lot 0.0001(a1)与 0.001(b1);每行只改该行,下一事件读 `outcome`。行 .8 的 G1 另有第三腿 a3(inst 13,A 现货,BTC)。行 .11:G1 的 OnFill 链接第一腿 a1 买 0.02 成交、第二腿 b1 只成交 0.01 后过期,`hedge_deadline_ms` 到期链接 Broken。行 .12/.13:G1 现货多 0.01、永续空 0.01,b1 标记价 50 010,V2 的 `MarginReport` 项 b1 `liq_px = 52 000`(距离 ≈ 4%)使其进入 L3,引擎目标为 (0, +0.008)、(1, −0.008)。行 .15/.16:a1 lot 0.004、b1 lot 0.01(公共 lot 0.02 ≈ 1 000 USDT),T 两腿 ±0.04、`max_chunk_notional = 500`
- 实现:`crates/hft-sequencer/tests/sdk_contract/target.rs::sdk_158_target_rejects`

| 行 | 改动 | 期望 `outcome` |
|---|---|---|
| SDK-158.1 | 不改 | `TargetAccepted`;`ExecDecision{TargetSet}` |
| SDK-158.2 | `account = A2`(未绑定 S3) | `TargetRejected(NotOwner)` |
| SDK-158.3 | `group = 0x0300_0009`(不在 S3 参数中) | `TargetRejected(NotOwner)` |
| SDK-158.4 | `legs[1].leg_ix = 2`(≥ `n_legs`) | `TargetRejected(BadLegs)` |
| SDK-158.5 | 两项都是 `leg_ix = 0` | `TargetRejected(BadLegs)` |
| SDK-158.6 | 现货腿目标 −0.01 | `TargetRejected(SpotShort)` |
| SDK-158.7 | 现货腿目标 0 | `TargetAccepted` |
| SDK-158.8 | `n_legs = 3`、`PairOnFill` | `TargetRejected(TooManyLegs)` |
| SDK-158.9 | `PairOnFill{first_leg: Fixed, first_leg_ix: 5}` | `TargetRejected(SpecInvalid)` |
| SDK-158.10 | `ExecSpec::MakerFirst`(未启用) | `TargetRejected(AlgoUnsupported)` |
| SDK-158.11 | G1 的 cell 处于 Broken | `TargetRejected(CellBroken)` |
| SDK-158.12 | 账户 MarginStress L3 默认减仓期,T 的永续腿(−0.01)比引擎目标(−0.008)更远离 0 | `TargetRejected(ForcedReduceActive)` |
| SDK-158.13 | 同 .12,T = (0, +0.005)、(1, −0.005):每腿比引擎目标更接近 0 且不变号 | `TargetAccepted` |
| SDK-158.14 | `Kill{Account, flatten}` 之后(未 `Resume{Kill}`),T 增仓 | `TargetRejected(FlattenLocked)` |
| SDK-158.15 | 两腿 lot 0.004 与 0.01,公共 lot 名义 > `max_chunk_notional`,G1 持仓 0 | `TargetRejected(LotMismatch)` |
| SDK-158.16 | 同 .15 但 `reason = Exit` 且为减仓:G1 持 +0.04 / −0.04,T 为 (0, +0.02)、(1, −0.02) | `TargetAccepted` |

### SDK-159 与当前目标逐字段相同的 `target` 不替换、不复位失败计数

- 规则:[目标意图](README.md#目标意图)
- 级别:integ
- 里程碑:M2
- 前置:`SeqRig`;G1 当前目标 T(`tag = 0x10`)处于 `Stuck(SliceFails)`
- 步骤:
  1. `RecProbe` 再发同一个 T
  2. 发 `tag = 0x11`、其余与 T 相同的目标
  3. 另一次运行:同一次 `on_event` 内 `clear_target(A1, G1)` 后 `target(T)`
- 期望:
  - 步骤 1:`outcome = TargetAccepted`;没有 `ExecDecision{TargetReplaced}`;`v.target(A1, G1).state` 仍为 `Stuck(SliceFails)`
  - 步骤 2:`ExecDecision{TargetReplaced}`;`state = Working`
  - 步骤 3:`ExecDecision{TargetCleared}` 后 `ExecDecision{TargetSet}`,`state = Working`
- 实现:`crates/hft-sequencer/tests/sdk_contract/target.rs::sdk_159_identical_target_not_reset`

### SDK-160 引擎设 MarginStress/Hard 目标时先撤策略在该 inst 上的挂单

- 规则:[目标意图](README.md#目标意图)
- 级别:integ
- 里程碑:M2
- 前置:`SeqRig`;`RecProbe` 在 I2 上有一张 reduce-only 的 GTC 平仓单(槽 s1)与一张 Open GTC(槽 s2),G1 有永续空头(G1 现货多 0.01、永续空 0.01;两张挂单为 b1 买 0.01 @ 49 500 reduce-only、b1 卖 0.001 @ 50 500,均已 Ack);V2 的 L3 同 SDK-158.12
- 步骤:
  1. 投递使 V2 进入 MarginStress L3 的 `MarginReport`
- 期望:
  - 同一 Input 内两条 `OrderDecision{Cancel}`(`target_slot` = s1、s2),先于引擎的第一笔减仓单;`MockVenue` 回撤单确认后 `RecProbe` 收到 `Order{Cancelled}` × 2
- 实现:`crates/hft-sequencer/tests/sdk_contract/target.rs::sdk_160_engine_target_cancels_strategy_resting`

### SDK-161 Broken 期间执行层自动有界平仓,成交以 `Exec{Progress}` 报告,解除时 `Done{cleared_by_engine: true}`

- 规则:[目标意图](README.md#目标意图)
- 级别:integ
- 里程碑:M2
- 前置:`SeqRig`;`broken_auto_fix_ms = 30_000`;G1 现货多 0.02、永续空 0.01(裸 0.01;构造同 SDK-158.11),cell 进入 Broken,group 静默,无人工 `HedgeFix`;a1、b1 每 1 s 一条 Bbo(报价健康),30 s 从进入 Broken 起算
- 步骤:
  1. 推进 30 s
  2. `MockVenue` 成交自动平仓单
- 期望:
  - 步骤 1:`RecProbe` 之前已收到 `Exec{Broken(LinkBroken)}`;30 s 时出一张 `origin = Engine(AutoFix)` 的平仓单
  - 步骤 2:`RecProbe` 收到 `Exec{Progress{leg_ix = 0, d = −1_000_000}, origin = Engine(AutoFix)}`,随后 `Exec{Done{cleared_by_engine: true}}`;`v.legs(A1, G1)` 两腿净量相抵;journal 有 `Note{ExecAutoFixDone}`
- 实现:`crates/hft-sequencer/tests/sdk_contract/target.rs::sdk_161_broken_auto_fix_progress_done`

### SDK-162 人工核销不经成交改变腿净量,Broken 随之解除

- 规则:[目标意图](README.md#目标意图)
- 级别:integ
- 里程碑:M2
- 前置:`SeqRig`,a1 的最小下单量改为 0.001(B0 为 0.0001);G1 现货多 0.0105、永续空 0.01(OnFill 链接第一腿 a1 买 0.0105 成交,第二腿 b1 卖 0.01 成交);多出的 0.0005 低于最小下单量、高于尘埃线,补腿无法下单,`fix_deadline_ms`(10 s)后 cell Broken(`FixExhausted`)、静止
- 步骤:
  1. Admin 发送 `WriteOffResidual{A1, G1}`(确认后)
- 期望:
  - 本 Input 内没有任何 `OrderDecision`;`RecProbe` 收到 `Exec{Done{cleared_by_engine: true}}`;之后 `v.group_pos(A1, G1).legs[0].qty` 减少 50_000、`v.legs(A1, G1).leg[0].residual = 0`、`leg[1].residual = 0`;`v.portfolio(A1).inventory(V1, BTC).unassigned` 增加 50_000
- 实现:`crates/hft-sequencer/tests/sdk_contract/target.rs::sdk_162_write_off_changes_legs_without_fill`

### SDK-163 `External{cause = Manual}` 的 `deadline_tsc = 0`,引擎不做默认处理

- 规则:[视图类型](README.md#视图类型)、[目标意图](README.md#目标意图)
- 级别:integ
- 里程碑:M2
- 前置:`SeqRig`;G1 平衡两腿;人工在交易所卖出现货 0.005(推送 `ExternalFill{cause = Manual}`,归属 G1)
- 步骤:
  1. 投递该外来成交,推进 60 s
- 期望:
  - `RecProbe` 收到 `External{gix = 0, f.cause = Manual, f.deadline_tsc = 0}`;60 s 内没有 `origin = Engine(External)` 的订单
- 实现:`crates/hft-sequencer/tests/sdk_contract/target.rs::sdk_163_manual_external_no_default`

### SDK-164 运行模式下策略意图的处理表

- 规则:[运行模式下意图的行为表](README.md#运行模式下意图的行为表)、[Ctx 与订单意图](README.md#ctx-与订单意图)
- 级别:table
- 里程碑:M2
- 前置:`SeqRig`;G1 两腿 I1 现货、I2 永续;G1 持有现货 0.01 与永续空头 0.01(Reduce 与 Hedge 有量可减);每行先把 G1 的有效模式设为该行的值(Shadow 行另设持仓为 0 以免 `ShadowDeferred`,只测 Open),再在一次 `on_event` 内 `place` 一张该行类别的 IOC(0.001;Open = a1 买、Reduce/Flatten = a1 卖、Hedge = b1 买,均无链接);下一事件读 `outcome`。`open_allowed = 0` 的行在建仓之后经 `SetParams` 把 G1 的 `open_allowed` 改为 0
- 实现:`crates/hft-sequencer/tests/sdk_contract/mode.rs::sdk_164_mode_table_orders`

| 行 | 有效模式 | `class` | 期望 |
|---|---|---|---|
| SDK-164.1 | Shadow | Open | `OrderDecision` 记录头带 `SHADOW` 位、`reject = 0`、`slot = 0`;无 `GwCmd`;`outcome = Shadowed` |
| SDK-164.2 | CloseOnly | Open | `reject = CLOSE_ONLY`、`slot = 0`;无 `GwCmd`;`outcome = Rejected(CLOSE_ONLY)` |
| SDK-164.3 | CloseOnly | Reduce | `reject = 0`、`slot ≠ 0`;一条 `GwCmd`;`outcome = Placed(_)` |
| SDK-164.4 | CloseOnly | Hedge(无链接、平衡组再买入会扩大裸露) | `reject = CLOSE_ONLY`、`slot = 0`;无 `GwCmd`;`outcome = Rejected(CLOSE_ONLY)` |
| SDK-164.5 | Live,`open_allowed = 1` | Open | `outcome = Placed(_)` |
| SDK-164.6 | Live,`open_allowed = 0` | Open | `reject = MODE_SHADOW`(1 << 24)、`slot = 0`;无 `GwCmd`;`outcome = Shadowed` |
| SDK-164.7 | Live,`open_allowed = 0` | Reduce | `outcome = Placed(_)` |
| SDK-164.8 | Live,`open_allowed = 0` | Flatten | `outcome = Placed(_)` |

- SDK-164.4 补充:组净量 +0.002 / −0.002 时分别在永续腿卖/买 0.001,同一 Input 连续两笔通过且带 RISK_REDUCING(仅买回空仓带 REDUCE_ONLY);第三笔同向与一笔反向均 CLOSE_ONLY,不出环。实现:`crates/hft-sequencer/tests/sdk_contract/mode.rs::check_unlinked_hedge_closes_naked_exposure`。

### SDK-165 Shadow 下完整风控评估且不扣资源

- 规则:[运行模式下意图的行为表](README.md#运行模式下意图的行为表)
- 级别:integ
- 里程碑:M2
- 前置:`SeqRig`;G1 有效模式 Shadow、持仓 0;V1 令牌桶剩余 5;OMS 空闲槽 64(B0 中把 A 的订单限额改为 10 s 5 个、OMS 每表 64 槽;令牌数读 hft-risk 快照节 17 的 (acct-1, A) 桶行,空闲槽读 `oms.table_view(A, acct-1).free_len`)
- 步骤:
  1. 一次 `on_event` 内 `place` 3 张 Open IOC
  2. 分别以 Shadow 与 Live 发送相同意图,覆盖有效单、零数量、越价、超名义、HARD 及非法链接
- 期望:
  - 3 条 `OrderDecision` 都带 `SHADOW` 位;之后令牌剩余仍为 5、空闲槽仍为 64、`v.legs(A1, G1).leg[0].inflight_immediate = 0`
  - Shadow 与 Live 的首个风控拒绝位相同;Shadow 同时带 SHADOW,slot=0、outcome=Shadowed,无 GwCmd;资源不变
- 实现:`crates/hft-sequencer/tests/sdk_contract/mode.rs::sdk_165_shadow_no_commit`
- 风控结果补充实现:`crates/hft-sequencer/tests/sdk_contract/mode.rs::check_shadow_records_live_risk_result`

### SDK-166 目标在各有效模式下的增仓与减仓部分

- 规则:[运行模式下意图的行为表](README.md#运行模式下意图的行为表)、[目标意图](README.md#目标意图)
- 级别:table
- 里程碑:M2
- 前置:`SeqRig`;G1 持仓 0;每行设该行模式(`open_allowed = 0` 经 `SetParams`)后发 `TakerSlice` 目标 +0.02(只含现货腿);随后投递 10 条 I1 的 Bbo(间隔 400 ms,超过切片冷却与 `stuck_after_ms`)
- 实现:`crates/hft-sequencer/tests/sdk_contract/mode.rs::sdk_166_mode_table_targets`

| 行 | 有效模式 | 期望 |
|---|---|---|
| SDK-166.1 | Shadow | `outcome = TargetShadowed`;恰一条带 `SHADOW` 位的切片 `OrderDecision`;随后 `Exec{Stuck(Shadow)}`;10 条 Bbo 后没有更多切片 |
| SDK-166.2 | CloseOnly | `outcome = TargetAccepted`;无切片;`Exec{Stuck(Gated(CLOSE_ONLY))}`;`v.target(A1, G1)` 仍为该目标 |
| SDK-166.3 | Live,`open_allowed = 0` | `outcome = TargetShadowed`(`ExecDecision{TargetSet}` 带 SHADOW);恰一条切片 `OrderDecision`,`reject = MODE_SHADOW`、`slot = 0`、记录头无 `SHADOW` 位;随后 `Exec{Stuck(Shadow)}`;10 条 Bbo 后没有更多切片 |
| SDK-166.4 | Live,`open_allowed = 1` | `outcome = TargetAccepted`;切片 `reject = 0` 并出环 |
| SDK-166.5 | CloseOnly,G1 持现货 0.02、永续空 0.02(两腿相抵,执行层不补腿),现货腿目标 0 | `outcome = TargetAccepted`;减仓切片(a1 卖)出环 |

### SDK-167 有效模式变化投递 `Mode`,降级时撤该 cell 的 Open 挂单,回到 Live 后按原目标继续

- 规则:[运行模式下意图的行为表](README.md#运行模式下意图的行为表)
- 级别:integ
- 里程碑:M2
- 前置:`SeqRig`;G1 Live,现货多 0.01、永续空 0.01;`RecProbe` 在 I1 有一张 Open GTC(s1,买 0.001 @ 49 500)与一张 Reduce GTC(s2,卖 0.001 @ 50 500),均已 Ack;G2 有一个增仓目标正在推进(`TakerSlice` a2 +0.1,`max_chunk_notional = 70`,第一片已成交)
- 步骤:
  1. `SetMode{S3, G1, A1, CloseOnly}` 与 `SetMode{S3, G2, A1, CloseOnly}`,之后 2 s 内每 400 ms 投递 a1、a2 各一条 Bbo
  2. `SetMode{S3, G1, A1, Live}` 与 `SetMode{S3, G2, A1, Live}`(确认后)
- 期望:
  - 步骤 1:`Mode{gix = 0, old = Live, new = CloseOnly}` 与 `gix = 1` 各一次;只有 s1 的 `OrderDecision{Cancel}`,s2 保留;G2 的目标仍在(`v.target(A1, G2)` 为 `Some`),无新增仓切片
  - 步骤 2:gix 0 与 1 各收到 `Mode{old = CloseOnly, new = Live}`;G2 在 `SetMode{S3, G2, A1, Live}` 的同一 Input 内出一笔增仓切片(a2 买,`reject = 0`;执行层在 `on_mode` 后立即推进,不等下一条 Bbo)
- 实现:`crates/hft-sequencer/tests/sdk_contract/mode.rs::sdk_167_mode_event_and_resting_cancel`

### SDK-168 期望 Shadow 但 cell 有仓位时有效模式为 CloseOnly,归零后自动落到 Shadow

- 规则:[运行模式下意图的行为表](README.md#运行模式下意图的行为表)
- 级别:integ
- 里程碑:M2
- 前置:`SeqRig`;G1 Live,现货 0.01、永续空头 0.01
- 步骤:
  1. `SetMode{S3, G1, A1, Shadow}`
  2. `RecProbe` 发 Reduce 单把两腿平到 0,`MockVenue` 成交
- 期望:
  - 步骤 1:`CmdResult{Applied, detail = ShadowDeferred}`;`Mode{old = Live, new = CloseOnly}`;Reduce 单放行
  - 步骤 2:最后一笔成交的同一 Input 内 `Mode{old = CloseOnly, new = Shadow}`;journal 有 `Note{ShadowDeferred}`,`v[2] = 1`
- 实现:`crates/hft-sequencer/tests/sdk_contract/mode.rs::sdk_168_shadow_deferred`

### SDK-169 OMS 第二腿不经模式门,已成交的第一腿不因模式切换裸着

- 规则:[运行模式下意图的行为表](README.md#运行模式下意图的行为表)
- 级别:integ
- 里程碑:M2
- 前置:`SeqRig`;G1 Live;`RecProbe` 发带 `OnFill` 链接的第一腿 Open IOC 0.01(已出环、未回执)
- 步骤:
  1. `SetMode{S3, G1, A1, Shadow}`(持仓 0、在途非 0 → 有效模式 CloseOnly)
  2. 第一腿成交 0.01
- 期望:
  - 步骤 2:第二腿 `OrderDecision{link_role = 2}` 的 `reject = 0`、无 `SHADOW` 位,出环一条 `GwCmd`
- 实现:`crates/hft-sequencer/tests/sdk_contract/mode.rs::sdk_169_second_leg_bypasses_mode_gate`

### SDK-170 `SetMode{group = *}` 写 `default_mode`,`SetParams` 新增的 group 取它

- 规则:[运行模式下意图的行为表](README.md#运行模式下意图的行为表)
- 级别:integ
- 里程碑:M2
- 前置:`SeqRig`;S3 有 G1–G3,全部 Live;G4 = `0x0100_0004`,单腿 inst 15(A 现货,资产 FILL),新增前先投递一条 inst 15 的 Bbo
- 步骤:
  1. `SetMode{S3, *, A1, CloseOnly}`
  2. `SetParams` 新增 G4
  3. `SetParams` 移除 G4(无仓位)后读 `v.mode(A1, G4)`
- 期望:
  - 步骤 1 后 G1–G3 都收到 `Mode{new = CloseOnly}`
  - 步骤 2 后 `v.mode(A1, G4) = CloseOnly`
  - 步骤 3 后 G4 的 cell 已删除:`RecProbe` 之后不再收到 `gix` 指向 G4 的任何事件
- 实现:`crates/hft-sequencer/tests/sdk_contract/mode.rs::sdk_170_default_mode_for_new_groups`

### SDK-171 `AdvicePayload` 编解码往返,未知版本按"无数据"处理

- 规则:[Advice 载荷](README.md#advice-载荷)、[版本与兼容规则](README.md#版本与兼容规则)
- 级别:unit
- 里程碑:M2
- 前置:`ProbeModel` 的载荷类型 `BandAdvice{flags: 0, band: 6_500_000}`(`LOCAL_KIND = 1`,`MAX_LEN = 16`);`TargetProbe` 以 `MockViews`(`boot_seq = 100`、`now.tsc = 10_000`)运行,`advice_max_age_ms = 1_000`、`tsc_per_ms = 1`
- 步骤:
  1. `encode` 到 `PayloadWriter`,再 `BandAdvice::decode`
  2. 投递 `ev::advice::<BandAdvice>(G1, &p, basis_seq = 150, basis_tsc = 9_500)`
  3. 投递首字节改为 2 的同一载荷(`ev::advice_raw`)
- 期望:
  - 步骤 1:16 B,首字节 1;解码结果逐字段相等
  - 步骤 2:`TargetProbe` 对 G1 发一个目标(`Recorded.targets.len() = 1`)
  - 步骤 3:`Recorded.targets` 为空,`TargetProbe` 视 G1 为无建议(不开仓)
- 实现:`crates/hft-strategy-sdk/tests/advice.rs::sdk_171_payload_roundtrip_unknown_version`

### SDK-172 Advice 过期:跨启动按 `basis_seq < boot_seq`,启动内按 `basis_tsc` 年龄

- 规则:[Advice 载荷](README.md#advice-载荷)
- 级别:table
- 里程碑:M2
- 前置:`TargetProbe`;`advice_max_age_ms = 1_000`;`MockViews`:`boot_seq = 100`、`tsc_per_ms = 1_000_000`、`now.tsc = 30_000_000_000`;每行投递一条 G1 的合法 `BandAdvice` 后检查是否发目标(新鲜则发)
- 实现:`crates/hft-strategy-sdk/tests/advice.rs::sdk_172_advice_staleness`

| 行 | `basis_seq` / `basis_tsc` | 期望 |
|---|---|---|
| SDK-172.1 | 150 / 29_000_000_000(年龄 1 000 ms) | 新鲜,发目标 |
| SDK-172.2 | 150 / 28_999_999_999(年龄 1 000 ms + 1 tsc) | 过期,不发 |
| SDK-172.3 | 99 / 30_000_000_000(上次启动的依据) | 过期,不发 |
| SDK-172.4 | 100 / 29_500_000_000 | 新鲜,发目标 |

### SDK-173 `AdviceOut` 的容量:128 条、合计 128 KiB

- 规则:[Advice 载荷](README.md#advice-载荷)
- 级别:table
- 里程碑:M2
- 前置:`ModelHarness::run`;每行在一次 `run` 内按该行连续 `emit_raw`
- 实现:`crates/hft-strategy-sdk/tests/advice.rs::sdk_173_advice_out_capacity`

| 行 | 调用 | 期望 |
|---|---|---|
| SDK-173.1 | 128 次,各 8 B | 全部 `Ok`;输出 128 条 |
| SDK-173.2 | 129 次,各 8 B | 第 129 次 `Err(AdviceFull)`;输出 128 条;计数 +1 |
| SDK-173.3 | 32 次,各 4 096 B(合计 131 072 B) | 全部 `Ok` |
| SDK-173.4 | 33 次,各 4 096 B | 第 33 次 `Err(AdviceFull)`;输出 32 条 |
| SDK-173.5 | 1 次,4 097 B | `Err(AdviceFull)`;输出为空 |
| SDK-173.6 | `local_kind = 0` | `Err(AdviceFull)` |
| SDK-173.7 | `local_kind = 0xFF00` | `Err(AdviceFull)` |
| SDK-173.8 | `local_kind = 0xFEFF` | `Ok` |
| SDK-173.9 | `group = GROUP_ALL` | `Err(AdviceFull)` |

### SDK-174 `AdvicePayload` 的 `LOCAL_KIND` 保留值与 `MAX_LEN` 在编译期检查

- 规则:[Advice 载荷](README.md#advice-载荷)
- 级别:table
- 里程碑:M2
- 前置:`trybuild`;每行一个 `impl AdvicePayload` 后在 `emit::<P>` 处使用
- 实现:`crates/hft-strategy-sdk/tests/ui.rs::sdk_174_payload_const_checks`

| 行 | 声明 | 期望 |
|---|---|---|
| SDK-174.1 | `LOCAL_KIND = 1`、`MAX_LEN = 4_096` | 编译通过 |
| SDK-174.2 | `MAX_LEN = 4_097` | 编译失败 |
| SDK-174.3 | `LOCAL_KIND = 0` | 编译失败 |
| SDK-174.4 | `LOCAL_KIND = 0xFFFF` | 编译失败 |

### SDK-175 Advice 记录头字段来自 `emit` 与 advisor

- 规则:[Advice 载荷](README.md#advice-载荷)
- 级别:integ
- 里程碑:M2
- 前置:`SeqRig` + `AdvRig`;`RecModel` 在处理 seq q 的 Bbo(记录头 tsc t)时 `emit(G2, &BandAdvice{..})`
- 步骤:
  1. 驱动到该 Advice 进入 journal
- 期望:
  - journal 中的 `Advice` 记录:`AdviceHdr{strategy = 3, account = A1, local_kind = 1, group = G2, len = 16, basis_seq = q, basis_tsc = t}`,载荷与 `encode` 输出逐字节相同
- 实现:`crates/hft-sequencer/tests/sdk_contract/advice.rs::sdk_175_advice_header_fields`

### SDK-176 `model_math::to_*` 的 HalfEven、饱和与非有限值

- 规则:[确定性规则](README.md#确定性规则)、[Advice 载荷](README.md#advice-载荷)
- 级别:table
- 里程碑:M2
- 前置:无
- 实现:`crates/hft-strategy-sdk/tests/model_math.rs::sdk_176_to_fixed`

| 行 | 调用 | 期望 |
|---|---|---|
| SDK-176.1 | `to_rate(0.0005)` | `Some(500_000)` |
| SDK-176.2 | `to_rate(2.5e-9)` | `Some(2)`(2.5 → 偶数) |
| SDK-176.3 | `to_rate(3.5e-9)` | `Some(4)` |
| SDK-176.4 | `to_rate(-2.5e-9)` | `Some(-2)` |
| SDK-176.5 | `to_rate(5e-10)` | `Some(0)` |
| SDK-176.6 | `to_rate(1e-10)` | `Some(0)` |
| SDK-176.7 | `to_notional(2.5e-6)` | `Some(2)` |
| SDK-176.8 | `to_notional(1234.5678905)` | `Some(1_234_567_890)` |
| SDK-176.9 | `to_rate(1e10)`(× 1e9 超出 i64) | `Some(i64::MAX)` |
| SDK-176.10 | `to_rate(-1e10)` | `Some(i64::MIN)` |
| SDK-176.11 | `to_rate(f64::NAN)` | `None` |
| SDK-176.12 | `to_rate(f64::INFINITY)` | `None` |
| SDK-176.13 | `to_notional(f64::NEG_INFINITY)` | `None` |

### SDK-177 `model_math` 的超越函数跨平台逐位相同

- 规则:[确定性规则](README.md#确定性规则)
- 级别:table
- 里程碑:M2
- 前置:同一测试在 x86_64(CI)与 aarch64(标签工作流)各跑一次;期望值以 `f64::to_bits` 的十六进制写在测试文件里,由 x86_64 首次运行产生并人工核对后固定
- 实现:`crates/hft-strategy-sdk/tests/model_math.rs::sdk_177_libm_bitwise`

| 行 | 调用 | 期望 |
|---|---|---|
| SDK-177.1 | `ln(2.0)`、`ln(1e-300)`、`ln(1.0)` | 与测试文件中的位模式逐位相同;`ln(1.0)` 为 `0.0` |
| SDK-177.2 | `exp(1.0)`、`exp(-745.0)`、`exp(710.0)` | 逐位相同;`exp(710.0)` 为 `+∞` |
| SDK-177.3 | `powf(1.0001, 10_000.0)`、`powf(2.0, 0.5)` | 逐位相同 |
| SDK-177.4 | `log10(1e-9)`、`log10(3.0)` | 逐位相同 |

### SDK-178 Model 只输出已就绪的值,NaN 结果置标志不输出

- 规则:[确定性规则](README.md#确定性规则)
- 级别:unit
- 里程碑:M2
- 前置:`ProbeModel`;G1 有 0 个样本(均值为 0/0),G2 有 10 个样本
- 步骤:
  1. 投递一个模型 tick
- 期望:
  - 两条 Advice:G1 的 `flags & NOT_READY = 1`、`band = 0`;G2 的 `flags & NOT_READY = 0`、`band` = 10 个样本中价均值按 HalfEven 取整后的 `Px` 原始值
- 实现:`crates/hft-strategy-sdk/tests/model.rs::sdk_178_nan_not_output`

### SDK-179 快速部分与 Model 对同一输入两次运行输出逐字节相同

- 规则:[确定性规则](README.md#确定性规则)
- 级别:prop
- 里程碑:M2
- 性质:同一事件脚本、同一 `MockViews` 状态序列,`EchoProbe`/`TargetProbe` 的 `Recorded` 与 `ProbeModel` 的 `AdviceOut` 两次运行逐字节相同;在两次运行之间用不同种子初始化进程内的 `RandomState`(若有代码迭代 `HashMap`,结果会不同)
- 生成:事件脚本 0–2 000 条,覆盖全部非 chain 变体;group 1–8 个
- 参考:第一次运行
- 例数:PR 500;每夜 20 000
- 实现:`crates/hft-strategy-sdk/tests/determinism.rs::sdk_179_two_runs_identical`

### SDK-180 预热块乱序、重复、与实时样本交错注入的结果相同

- 规则:[预热注入](README.md#预热注入)
- 级别:prop
- 里程碑:M2
- 性质:同一批预热块(各自完整、按 group 与时间范围独立)以任意顺序、任意重复次数、与实时 Bbo 任意交错投递给 `ProbeModel`,最终 `snapshot_all` 字节与之后一个模型 tick 的 Advice 字节都与"先按 `chunk_no` 顺序注入一次、再投递实时样本"的结果相同;实时样本与注入样本的 UTC 时刻相同时保留实时样本
- 生成:1–4 个 group;每 group 1–8 块、每块 1–64 个样本;实时 Bbo 0–200 条,其中 20% 与某注入样本同一 UTC 秒
- 参考:按顺序注入一次的结果
- 例数:PR 500;每夜 20 000
- 实现:`crates/hft-strategy-sdk/tests/warmup.rs::sdk_180_warmup_idempotent_order_free`

### SDK-181 预热注入经 journal 进 Model,重放与重启结果相同

- 规则:[预热注入](README.md#预热注入)
- 级别:integ
- 里程碑:M2
- 前置:`SeqRig` + `AdvRig`;`RecModel` 按 `ProbeModel` 的规则应用预热块
- 步骤:
  1. 发送 3 块 `InjectWarmup{S3, local_kind = 1, chunk_no = 0..=2, last = (chunk_no == 2)}`
  2. advisor 从头重放同一 journal(无检查点)
- 期望:
  - 步骤 1:每块 `CmdResult{Applied}`;`RecModel` 依次收到 `Warmup{chunk_no = 0, last = false}`、`1, false`、`2, true`,每次在对应 `CmdResult{Applied}` 之后
  - 步骤 2:`RecModel` 的调用记录与 Model 状态字节与步骤 1 结束时逐字节相同
- 实现:`crates/hft-sequencer/tests/sdk_contract/warmup.rs::sdk_181_warmup_via_journal`

### SDK-182 `InjectWarmup` 只校验实例存在与长度

- 规则:[预热注入](README.md#预热注入)
- 级别:table
- 里程碑:M2
- 前置:`SeqRig`;实例 S3 存在、有 Model;每行发一条 `InjectWarmup`(`account = *`)
- 实现:`crates/hft-sequencer/tests/sdk_contract/warmup.rs::sdk_182_inject_warmup_checks`

| 行 | 命令 | 期望 |
|---|---|---|
| SDK-182.1 | `instance = S3`,载荷 4 096 B | `Applied` |
| SDK-182.2 | `instance = S3`,载荷 4 097 B | 命令编码失败(gateway 以 `TooLong` 拒绝,不进 ctl 环);`RecModel` 无 `Warmup` |
| SDK-182.3 | `instance = S7`(不存在) | `UnknownTarget`;无投递 |
| SDK-182.4 | `instance = S3`,`chunk_no = 5`(之前没有 0–4) | `Applied`;`RecModel` 收到 `Warmup{chunk_no = 5}` |

### SDK-183 注入后写出的检查点含预热样本,重启不需再注入

- 规则:[预热注入](README.md#预热注入)、[Model 检查点](README.md#model-检查点)
- 级别:unit
- 里程碑:M2
- 前置:`ModelHarness` + `ProbeModel`;G1 空状态;一块 G1 的 64 个样本的预热载荷
- 步骤:
  1. 投递该 `Warmup`;`snapshot_all(.., 65_536)` 得 C
  2. 新实例 `restore_all(&C, keep_path = true)`,投递一个模型 tick
- 期望:
  - 步骤 2 的实例有 64 个样本;G1 的 Advice 与步骤 1 的实例在同一 tick 上的 Advice 逐字节相同
- 实现:`crates/hft-strategy-sdk/tests/warmup.rs::sdk_183_checkpoint_keeps_warmup`

### SDK-184 预热注入不直接产生意图

- 规则:[预热注入](README.md#预热注入)
- 级别:integ
- 里程碑:M2
- 前置:`SeqRig` + `AdvRig`;`RecProbe` 调了 `warmup(Part::Both)`,脚本:收到 `Warmup` 时不做任何事
- 步骤:
  1. 发送一块 `InjectWarmup`
- 期望:
  - 该 Input 的 Output 中没有 `OrderDecision`、`ExecDecision`
- 实现:`crates/hft-sequencer/tests/sdk_contract/warmup.rs::sdk_184_warmup_no_intents`

### SDK-185 `strategy_set!` 生成的分派与直接调用结果相同

- 规则:[装配:StrategySet 与 strategy_set!](README.md#装配strategyset-与-strategy_set)
- 级别:prop
- 里程碑:M2
- 性质:对 `ProbeSet` 的每个方法(`on_event`、`on_params`、`subscriptions`、`snapshot`、`restore`、`model_*`、`reset_preview`、`decode_params`、`validate_params`),经 `ProbeSet` 的结果与直接调用 `EchoProbe`/`TargetProbe`/`ProbeModel` 的同名方法相同(`Recorded`、返回值与写出的字节)
- 生成:随机选种类;事件脚本 0–500 条;参数为 `TP` 的随机合法值
- 参考:直接调用
- 例数:PR 1 000;每夜 50 000
- 实现:`crates/hft-strategy-sdk/tests/set_dispatch.rs::sdk_185_dispatch_equivalent`

### SDK-186 `StrategySet` 的静态查询与 Model 布局版本

- 规则:[装配:StrategySet 与 strategy_set!](README.md#装配strategyset-与-strategy_set)
- 级别:table
- 里程碑:M2
- 前置:`ProbeSet`(`EchoProbe` 0xFF01 在前、`TargetProbe` 0xFF02 在后)
- 实现:`crates/hft-strategy-sdk/tests/set_dispatch.rs::sdk_186_static_queries`

| 行 | 调用 | 期望 |
|---|---|---|
| SDK-186.1 | `kinds()` | 两项,`kind_id` 依次 0xFF01、0xFF02 |
| SDK-186.2 | `cmds(0xFF01)` | 3 项(op 1、2、3) |
| SDK-186.3 | `cmds(0xFF02)` | 空 |
| SDK-186.4 | `cmds(0xFF09)`(集合外) | 空 |
| SDK-186.5 | `schema_json(0xFF01)` | `Some(s)`,`s == params::schema_json(&EchoProbe::DESC, &ModelLayoutDesc::EMPTY, 2)` |
| SDK-186.6 | `schema_json(0xFF09)` | `None` |
| SDK-186.7 | `model_path_version(0xFF02)` / `(0xFF01)` | 1 / 0(`NoModel`) |
| SDK-186.8 | `model_layout(0xFF02)` | 与 `ProbeModel::LAYOUT` 是同一个 `&'static` |
| SDK-186.9 | `decode_params(0xFF09, ..)` | `Err(SchemaMismatch{want: 0, got: 0xFF09})` |
| SDK-186.10 | `model_mem_bytes(&p_target, 8)` | = `ProbeModel::mem_bytes(&p, 8)`(131_072) |
| SDK-186.11 | `model_layout_version(0xFF02)` | = `ProbeModel::LAYOUT_VERSION` |
| SDK-186.12 | `model_layout_version(0xFF01)` | 0(`NoModel`) |
| SDK-186.13 | `model_layout_version(0xFF09)` | 0(集合外) |
| SDK-186.14 | `has_model(0xFF02)` / `(0xFF01)` / `(0xFF09)` | true / false(NoModel) / false(集合外) |
| SDK-186.15 | 独立 `ZeroSet` 的真实无状态 Model,LAYOUT 为空、版本与内存/快照声明均为 0 | `has_model` 为 true |

### SDK-187 `strategy_set!` 拒绝重复的 `kind_id`、`name` 与 `schema.id`

- 规则:[装配:StrategySet 与 strategy_set!](README.md#装配strategyset-与-strategy_set)
- 级别:table
- 里程碑:M2
- 前置:`trybuild`;每行一次 `strategy_set!` 调用
- 实现:`crates/hft-strategy-sdk/tests/ui.rs::sdk_187_set_uniqueness`

| 行 | 列表 | 期望 |
|---|---|---|
| SDK-187.1 | `EchoProbe`、`TargetProbe` | 编译通过 |
| SDK-187.2 | 两个 `kind_id` 都为 0xFF01 的策略 | 编译失败 |
| SDK-187.3 | 两个 `name` 相同的策略 | 编译失败 |
| SDK-187.4 | 两个 `schema.id` 相同、`kind_id` 不同的策略 | 编译失败 |

### SDK-188 `strategy_set!` 的行可用 `#[cfg]` 门控

- 规则:[装配:StrategySet 与 strategy_set!](README.md#装配strategyset-与-strategy_set)
- 级别:unit
- 里程碑:M2
- 前置:`tests/set_cfg.rs` 中 `strategy_set!{ EchoProbe, #[cfg(feature = "probe-target")] TargetProbe }`
- 步骤:
  1. 不开 `probe-target` 编译并调 `kinds()`
  2. 开 `probe-target` 编译并调 `kinds()`
- 期望:
  - 步骤 1 只有 0xFF01;步骤 2 有 0xFF01、0xFF02
- 实现:`crates/hft-strategy-sdk/tests/set_cfg.rs::sdk_188_cfg_gated_rows`

### SDK-189 `StrategySet` 分派生成的代码不含 `dyn`

- 规则:[装配:StrategySet 与 strategy_set!](README.md#装配strategyset-与-strategy_set)
- 级别:unit
- 里程碑:M2
- 前置:`strategy_set!` 的展开模板(`src/set_macro.rs` 中的宏定义,去掉注释行;工具链不含 `cargo expand`,每次展开都由该模板生成)
- 步骤:
  1. 在展开模板里搜索 `dyn ` 与 `Box<`
- 期望:
  - 两者都 0 处;`on_event` 的展开是一层 `match`
- 实现:`crates/hft-strategy-sdk/tests/set_dispatch.rs::sdk_189_no_dyn_in_expansion`

### SDK-190 影响 journal 字节的意图类型与决策记录体字段一致

- 规则:[版本与兼容规则](README.md#版本与兼容规则)
- 级别:unit
- 里程碑:M2
- 前置:`SeqRig`(hft-sequencer 的测试同时看到 SDK、hft-event、hft-oms 与 hft-exec;编译期 `const` 断言在 hft-oms/hft-exec 把意图投影为记录体的位置)
- 步骤:
  1. 构造一个每个字段取不同非零值的 `OrderIntent`(带链接)与 `TargetIntent`(`PairOnFill`),由 `RecProbe` 在一次 `on_event` 内发出,经 hft-oms/hft-exec 的投影写成 journal 中的 `OrderDecision`/`ExecDecision` 体,再按 hft-event 解码
- 期望:
  - `OrderDecision` 的 `px`、`qty`、`group`、`inst`、`side`、`tif`、`link_role`(= 1)与意图相同;`ExecDecision` 的 `reason`、`urgency`、`algo`(= 2)、`tag` 与目标相同;投影过程中的 `const` 断言全部编译通过
- 实现:`crates/hft-sequencer/tests/sdk_contract/codes.rs::sdk_190_intent_record_consistency`

### SDK-191 `SnapshotWriter`/`SnapshotReader` 小端定长读写与节版本

- 规则:[快照节](README.md#快照节)
- 级别:unit
- 里程碑:M1
- 前置:`SnapshotWriter` 上限 1 024 B
- 步骤:
  1. `section(7)`;`put_u8(1)`、`put_u16(0x0203)`、`put_i64(-2)`、`put_i128(1 << 100)`、`put_bytes(&[9, 9])`
  2. 以 `desc_version = 5` 构造 `SnapshotReader` 读回;另以"其他用途"构造读回
  3. 新 writer:`begin_section(6, 1)`、两次 `put_row(&[u32; 2])`;`begin_section(7, 2)`、`put_u8(5)`、`add_rows(1)`、`end_section()`;读者 `find_section(7)`、`find_section(6)`、`find_section(9)`
- 期望:
  - 步骤 1 中 `section(7)` 不写字节,`put_u16` 写出 `03 02`(字节 1..3);`put_i64(-2)` 写出 `FE FF FF FF FF FF FF FF`
  - 步骤 2 读回的值逐个相等;前者 `desc_version() = 5`,后者 `desc_version() = 0`;读过尾 → `Err(RestoreError::Truncated)`
  - 步骤 3 共 56 B;节 7:`version = 2`、`count = 1`、节体 8 B(补零)且首字节 5;节 6:`version = 1`、`count = 2`、节体 16 B;节 9 → `None`
- 实现:`crates/hft-strategy-sdk/tests/snapshot.rs::sdk_191_writer_reader_le`

### SDK-192 `testing::layout_json` 输出稳定

- 规则:[测试桩(testing 特性)](README.md#测试桩testing-特性)、[Model 检查点](README.md#model-检查点)
- 级别:golden
- 里程碑:M2
- 输入:`ProbeModel::LAYOUT`
- 期望文件:`fixtures/strategies/sdk-probe/ckpt/layout.json`
- 备注:同一输入在 x86_64 与 aarch64 上输出逐字节相同;键按 `ModelLayoutDesc`/`SectionDesc`/`ElemDesc` 的字段顺序;`ElemTy` 写变体名、`LenRule` 写 `"one"`、`{"fixed": n}`、`"prefixed"`
- 实现:`crates/hft-strategy-sdk/tests/checkpoint.rs::sdk_192_layout_json_stable`

### SDK-193 `testing::ev` 构造器产生的事件与引擎投递的同一事件相同

- 规则:[测试桩(testing 特性)](README.md#测试桩testing-特性)
- 级别:integ
- 里程碑:M2
- 前置:`SeqRig`;`RecProbe` 订阅 I1(BBO)、`warmup(Part::Both)` 与 `model_own` 全部位
- 步骤:
  1. 投递一条 I1 的 Bbo、一条 `StrategyCmd{op 1, [5], reduce_only = false, group = none}`、一块 `InjectWarmup`
  2. 用 `ev::bbo(..)`、`ev::manual(1, &[5], false)`、`ev::warmup(..)` 以相同字段构造
- 期望:
  - 步骤 1 中 `RecProbe` 收到的三个 `EventRef` 与步骤 2 的 `.as_ref()` 逐字段相等(`Manual.payload` 逐字节相等)
- 实现:`crates/hft-sequencer/tests/sdk_contract/harness.rs::sdk_193_ev_matches_engine`

### SDK-194 `MockViews` 只回放设定的状态,不撮合

- 规则:[测试桩(testing 特性)](README.md#测试桩testing-特性)
- 级别:unit
- 里程碑:M2
- 前置:`MockViews`;`legs(A1, G1, L)`,`L.leg[0].net = 0`;`EchoProbe` 在一次事件中 `place` 一张 Open IOC
- 步骤:
  1. 读 `v.legs(A1, G1)` 与 `v.order(任意 OrderRef)`
- 期望:
  - `legs` 等于 L(`net = 0`、在途 0);`order` 返回 `None`(MockViews 不因意图改变状态)
- 实现:`crates/hft-strategy-sdk/tests/mock_views.rs::sdk_194_mock_views_no_matching`

### SDK-195 `MockViews` 的设定器逐项生效

- 规则:[测试桩(testing 特性)](README.md#测试桩testing-特性)
- 级别:table
- 里程碑:M2
- 前置:`MockViews::new(RefFixture{I1, I2, BTC, USDT, V1, V2, A1})`
- 实现:`crates/hft-strategy-sdk/tests/mock_views.rs::sdk_195_mock_setters`

| 行 | 设定 | 读取与期望 |
|---|---|---|
| SDK-195.1 | `mode(A1, G1, CloseOnly)` | `mode(A1, G1) = CloseOnly` |
| SDK-195.2 | `risk(A1, r)`(`conn_liveness_ms = 2_000`) | `risk(A1) == r` |
| SDK-195.3 | `risk_for(A1, I2, x)` | `risk_for(A1, I2) == x` |
| SDK-195.4 | `venue_ready(A1, V2, false)` | `venue_ready(A1, V2) = false`、`venue_ready(A1, V1) = true` |
| SDK-195.5 | `leverage(A1, I2, Some(300))` | `leverage(A1, I2) = Some(300)` |
| SDK-195.6 | `target(A1, G1, None)` | `target(A1, G1) = None` |
| SDK-195.7 | `funding(I2, f)` | `funding(I2) == f` |
| SDK-195.8 | `book(I2, bids[3], asks[2])` | `book(I2).status() = Live`;`levels(Bid, ..)` 返回 3 |
| SDK-195.9 | `group_pos(A1, G1, p)` | `group_pos(A1, G1) == p` |
| SDK-195.10 | `portfolio(A1, |f| f.cash(V1, USDT, 1_000_000_000, 0))` | `portfolio(A1).cash(V1, USDT).free = 1_000_000_000` |

### SDK-196 SDK 基准:`Ctx::place`

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:bench
- 里程碑:M2
- 基准名:`sdk/ctx_place`
- 实现:`crates/hft-strategy-sdk/benches/ctx.rs::sdk_196_ctx_place`

### SDK-197 SDK 基准:`Ctx::target`

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:bench
- 里程碑:M2
- 基准名:`sdk/ctx_target`
- 实现:`crates/hft-strategy-sdk/benches/ctx.rs::sdk_197_ctx_target`

### SDK-198 SDK 基准:`StrategySet` 分派开销

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:bench
- 里程碑:M2
- 基准名:`sdk/set_dispatch`
- 实现:`crates/hft-strategy-sdk/benches/set.rs::sdk_198_set_dispatch`

### SDK-199 SDK 基准:`AdvicePayload::decode`

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:bench
- 里程碑:M2
- 基准名:`sdk/payload_decode`
- 实现:`crates/hft-strategy-sdk/benches/advice.rs::sdk_199_payload_decode`

### SDK-200 SDK 基准:`CmdView::parse` 与全部 `val`

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:bench
- 里程碑:M2
- 基准名:`sdk/cmd_parse`
- 实现:`crates/hft-strategy-sdk/benches/cmd.rs::sdk_200_cmd_parse`

### SDK-201 SDK 基准:`BboView::quote_live`

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:bench
- 里程碑:M2
- 基准名:`sdk/quote_live`
- 实现:`crates/hft-strategy-sdk/benches/views.rs::sdk_201_quote_live`

### SDK-202 SDK 基准:`Now::ens_of`/`tsc_of`/`utc_of`

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:bench
- 里程碑:M2
- 基准名:`sdk/time_conv`
- 实现:`crates/hft-strategy-sdk/benches/views.rs::sdk_202_time_conv`

### SDK-203 SDK 基准:`ParamSet::decode`(64 group × 64 字段)

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:bench
- 里程碑:M2
- 基准名:`sdk/params_decode`
- 实现:`crates/hft-strategy-sdk/benches/params.rs::sdk_203_params_decode`

### SDK-204 SDK 基准:`params::body_view`(4 096 条覆盖)

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:bench
- 里程碑:M1
- 基准名:`sdk/body_view`
- 实现:`crates/hft-strategy-sdk/benches/params.rs::sdk_204_body_view`

### SDK-205 SDK 基准:`decode` + `validate` 按 group 数线性

- 规则:[schema JSON 与 define_params!](README.md#schema-json-与-define_params)
- 级别:bench
- 里程碑:M2
- 基准名:`params/validate`
- 实现:`crates/hft-strategy-sdk/benches/params.rs::sdk_205_params_validate`

### SDK-206 新策略模板的 `benches/budgets.toml` 含 SDK 规定的全部入口

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:table
- 里程碑:M9
- 前置:`cargo xtask new-strategy probe-x --kind-id 0xFF20` 在临时副本中生成;每行一种生成选项
- 实现:`tools/xtask/tests/new_strategy.rs::sdk_206_template_budget_entries`

| 行 | 选项 | `benches/budgets.toml` 的条目 |
|---|---|---|
| SDK-206.1 | 无 | `strategies/probe-x/{tick_miss, tick_fire, slow_update, advice, target_set, exec_update, risk_event}`,上限等于 README 的默认值 |
| SDK-206.2 | `--with-model` | .1 的全部 + `model/probe-x/{sample, tick, snapshot_chunk}` |
| SDK-206.3 | `--with-model --with-pipeline` | .2 的全部 + `model/probe-x/warmup_chunk` |

### SDK-207 同一策略在模拟账户与实盘账户上对同一输入给出相同意图

- 规则:[Strategy 与 Model](README.md#strategy-与-model)
- 级别:integ
- 里程碑:M2
- 前置:`SeqRig`;实例 S3 = s9 绑定 Sim 账户 sim-9,实例 S6 = s1(同一种类、同一参数值)绑定 Live 测试网账户 acct-1(`is_prod = false`,venues {A, B});两者订阅 I1;两者 Live、`open_allowed = 1`
- 步骤:
  1. 投递 20 条 I1 的 Bbo,其中 5 条满足触发条件
- 期望:
  - S3 与 S6 的 `OrderDecision` 序列条数相同(各 5 条),除 `strategy`、`account`、`slot`、`gen`、`intent_no` 与 `group`(GroupId 含实例号)外逐字段相同,`trigger_seq` 相同;同一 Bbo 上 S6(实盘实例)的决策排在 S3 之前
- 实现:`crates/hft-sequencer/tests/sdk_contract/lifecycle.rs::sdk_207_sim_and_live_identical`

### SDK-208 `RefView` 只暴露本实例需要的参考数据

- 规则:[Strategy 与 Model](README.md#strategy-与-model)
- 级别:table
- 里程碑:M2
- 前置:refdata 含 I1–I5、资产 1(BTC)与 2、venue V1–V4、账户 A1(绑定 S3)与 A2;`RefView` 为 S3 构造
- 实现:`crates/hft-strategy-sdk/tests/refview.rs::sdk_208_ref_view`

| 行 | 调用 | 期望 |
|---|---|---|
| SDK-208.1 | `inst(102).kind`;`caps(inst(102).venue).market` | `InstKind::Perp`(热行的产品类型);`MarketKind::LinearPerp`(市场类型在 `VenueCaps`) |
| SDK-208.2 | `insts_of(1)` | `[101, 102, 103, 104]`(InstId 升序,含不在 A1 venues 内的) |
| SDK-208.3 | `insts_of(2)` | `[105]` |
| SDK-208.4 | `account().id` | A1 |
| SDK-208.5 | `caps(V2)` | V2 那一行 `VenueCaps` |
| SDK-208.6 | `fee(A1, 101)` | 该 (账户, inst) 的 `FeeSchedule` 档:`maker`、`taker` 为 `Rate` |
| SDK-208.7 | `try_inst(102)` | `Some`,与 `inst(102)` 同一引用 |
| SDK-208.8 | `try_inst(999)`(不在 refdata) | `None` |
| SDK-208.9 | `try_fee(A1, 101)` | `Some`,与 .6 相同 |
| SDK-208.10 | `try_fee(A2, 101)`(A2 不是本实例绑定的账户) | `None` |
| SDK-208.11 | `try_fee(A1, 999)` | `None` |
| SDK-208.12 | `try_fee(A1, i)`,i 为 `insts_of(1)` 中 venue 不在 A1 venues 内的 inst | `None` |

### SDK-209 `Risk` 事件与 `RiskStateView` 的读数

- 规则:[视图类型](README.md#视图类型)、[EventRef](README.md#eventref)
- 级别:table
- 里程碑:M2
- 前置:`SeqRig`;实例 Live;每行一个独立运行,触发该行事件后读下一事件中的 `v.risk(A1)`
- 实现:`crates/hft-sequencer/tests/sdk_contract/views.rs::sdk_209_risk_events_and_state`

| 行 | 触发 | 期望 |
|---|---|---|
| SDK-209.1 | G1 链接 `Broken{Deadline}` | `RecProbe` 收到 `Risk{e = HedgeBroken{group = G1}}`;`Exec{gix = 0, Broken(LinkBroken)}` |
| SDK-209.2 | `Kill{Account(A1), flatten}`(确认后) | `Risk{e = CapSet{source = Kill, ..}}`;`v.risk(A1).flatten_lock = true`、`open_blocked = true` |
| SDK-209.3 | 接 .2,`Resume{Kill}`(确认后,仓位已平) | `Risk{e = CapCleared{source = Kill, ..}}`;`flatten_lock = false`、`open_blocked = false` |
| SDK-209.4 | V2 MarginStress L2 | `Risk{e = MarginStress{venue = V2, level = L2, deadline_tsc > 0}}`;`v.risk(A1).margin_level = L2` |
| SDK-209.5 | 接 .4,保证金回到安全区 | `Risk{e = CapCleared{source = MarginStress, ..}}`;`margin_level = None` |
| SDK-209.6 | 无事件,读 `v.risk(A1)` | `conn_liveness_ms = 2_000`、`dust_notional` = A1 的 `ExecConfig.dust_notional` |

### SDK-210 未知版本的预热载荷按"无数据"处理

- 规则:[版本与兼容规则](README.md#版本与兼容规则)、[预热注入](README.md#预热注入)
- 级别:unit
- 里程碑:M2
- 前置:`ProbeModel`,G1 空状态;一块 64 个样本的预热载荷,首字节改为 2
- 步骤:
  1. 投递该 `Warmup`
- 期望:
  - G1 样本数仍为 0;不 panic;`snapshot_all` 字节与投递前相同
- 实现:`crates/hft-strategy-sdk/tests/warmup.rs::sdk_210_unknown_warmup_version_ignored`

### SDK-211 `PayloadWriter` 超出 `MAX_LEN` 返回 `PayloadFull`,截断的载荷解码失败

- 规则:[Advice 载荷](README.md#advice-载荷)
- 级别:table
- 里程碑:M2
- 前置:`BandAdvice`(`MAX_LEN = 16`)
- 实现:`crates/hft-strategy-sdk/tests/advice.rs::sdk_211_payload_bounds`

| 行 | 调用 | 期望 |
|---|---|---|
| SDK-211.1 | `encode` 写 16 B | `Ok(())` |
| SDK-211.2 | 测试变体 `encode` 写 17 B | `Err(PayloadFull)` |
| SDK-211.3 | `decode(&bytes[..15])` | `Err(PayloadError)` |
| SDK-211.4 | `decode(&[])` | `Err(PayloadError)` |

### SDK-212 共享定时器配额按调用顺序预留与释放

- 规则:[Ctx 的内存与排空](README.md#ctx-的内存与排空)
- 级别:unit
- 里程碑:M2
- 前置:两个独立 CtxBufs 共享两格配额;只读登记表按 TimerId 升序
- 步骤:交替借用两个实例与同一实例的多个回调;取消已有 ID、本轮 ID、未知 ID 及重复 ID;先取消尚未分配的 ID 再登记并取消;填满登记/取消缓冲;将计数器设为 u64::MAX
- 期望:每次返回值与剩余配额一致,失败不消耗 ID 或预留量;有效取消只释放一次,未知/丢弃的取消不释放且不约束未来登记;begin 清理本轮操作但保留 ID 计数;耗尽不回绕;操作数组保留接受顺序
- 实现:`crates/hft-strategy-sdk/tests/ctx.rs::sdk_212_timer_admission_preserves_call_order`

### SDK-213 `MockViews` 的订单视图与行情戳

- 规则:[Views 与 ModelViews](README.md#views-与-modelviews)(`md_count`/`md_stamp`)、[视图类型](README.md#视图类型)(`MdStampView`)、[测试桩(testing 特性)](README.md#测试桩testing-特性)
- 级别:table
- 里程碑:M3
- 前置:`MockViews::new(RefFixture{I1, I2, BTC, USDT, V1, V2, A1})`;引擎侧(hft-sequencer 的记录与环)见 SEQ-447
- 实现:`crates/hft-strategy-sdk/tests/mock_views.rs::sdk_213_mock_orders_and_md_stamps`

| 行 | 设定 | 读取与期望 |
|---|---|---|
| SDK-213.1 | `set_order(o)`(`r = (V1, A1, 7, 2)`) | `order(r) == Some(o)`;gen 3 → `None` |
| SDK-213.2 | `clear_order(r)` | `order(r) = None` |
| SDK-213.3 | 无行情戳 | `md_count(I1) = 0`,`md_stamp(I1, 1) = None` |
| SDK-213.4 | `md_push(I1, …)` 20 次(`ex_ts_ns = k`、`sw_rx_ns = 100 + k`) | `md_count(I1) = 20`;`md_stamp` 第 4 条 `None`、第 5 条 `ex_ts_ns = 5`、第 20 条 `sw_rx_ns = 120`、第 21 条 `None`;`md_count(I2) = 0` |

### SDK-214 `Views` 的 `bbo_stamp`、`stamps`、`open_orders` 提供的默认方法

- 规则:[Views 与 ModelViews](README.md#views-与-modelviews)、[视图类型](README.md#视图类型)(`Stamps`)
- 级别:table
- 里程碑:M3
- 前置:`MockViews::new(RefFixture{I1, I2, BTC, USDT, V1, V2, A1})`,`now` 的 `engine_ns`、`utc_ns` 已设;引擎侧见 SEQ-448、SEQ-449
- 实现:`crates/hft-strategy-sdk/tests/mock_views.rs::sdk_214_provided_stamps_and_open_orders`

| 行 | 读取 | 期望 |
|---|---|---|
| SDK-214.1 | `bbo_stamp(I1)` | `None` |
| SDK-214.2 | `stamps()` | `engine_ns`、`utc_ns` 等于 `now()` 的值;`kind`、`venue_ns`、`sw_rx_ns`、`hw_rx_ns`、`rx_tsc` 为 0 |
| SDK-214.3 | `open_orders(A1, I1, 2 条缓冲)` | 0 |

### SDK-215 `Strategy::FEATURES`、`export_features` 与 `StrategySet` 的分派

- 规则:[特征导出](README.md#特征导出)、[装配:StrategySet 与 strategy_set!](README.md#装配strategyset-与-strategy_set)
- 级别:table
- 里程碑:M3
- 前置:`tests/set_dispatch.rs` 的 `ProbeSet { EchoProbe(0xFF01), TargetProbe(0xFF02) }`;`EchoProbe` 声明两列 `mode`、`groups`(`Count`,exp 0),`TargetProbe` 用缺省;实例以 `gt3()`(3 个 group)创建
- 实现:`crates/hft-strategy-sdk/tests/set_dispatch.rs::sdk_215_feature_columns_and_export`

| 行 | 操作 | 期望 |
|---|---|---|
| SDK-215.1 | `feature_columns(0xFF01)`、`(0xFF02)`、`(0xFF09)` | `["mode", "groups"]`;空;空 |
| SDK-215.2 | `export_features` 写进 4 格、初值 `NA` 的存储 | EchoProbe:`len = 2`,存储 `[0, 3, NA, NA]`;TargetProbe:什么都不写(`is_empty`) |

### SDK-216 按金额买入:`spend_buy` 与 `spend_qty`

- 规则:[Ctx 与订单意图](README.md#ctx-与订单意图)(按金额买入)
- 级别:table
- 里程碑:M3
- 前置:`e = AssetExp{px_exp −3, qty_exp −6}`(二元结果代币,价 0.001、量 1e-6)
- 实现:`crates/hft-strategy-sdk/tests/ctx.rs::sdk_216_spend_buy_and_spend_qty`

| 行 | 操作 | 期望 |
|---|---|---|
| SDK-216.1 | `OrderIntent::SPEND`;`spend_buy(V1, A1, G1, I1, Ioc, Px(110), Qty(9_090_000), 7)` | `0x80`(= hft-event `OrderFlags::SPEND`);`side Buy`、`class Open`、`flags = SPEND`、`spend()` 真、`reduce_only()` 假、无链接、`max_rest_ms 0`、`tag 7` |
| SDK-216.2 | `spend_qty(Notional(1_000_000), Px(110), e, 10_000)`(1 USDC、0.11、lot 0.01) | `Qty(9_090_000)`(9.09) |
| SDK-216.3 | 同上,`lot 1` | `Qty(9_090_909)` |
| SDK-216.4 | `Px(0)`;`Notional(0)`;`px_exp 40` | 都为 `Qty(0)` |

## 验收

- 里程碑已到的用例全部有同编号的实现且通过(`cargo xtask test-trace`,规则见 [docs/tests §6](../../docs/tests/README.md#6-与代码的对应));`params` 模块的用例(SDK-090–SDK-101、SDK-106–SDK-112、SDK-120、SDK-204)在 M1 随 `tools/hft-cfg` 先行完成,快照读写类型的用例(SDK-025、SDK-191,hft-book 在 M1 使用)也在 M1 完成,其余在 M2。
- 属性测试的大例数运行:各属性用例"例数"字段中的每夜例数在 CI 每夜任务中运行(运行矩阵归 [testing §6](../../docs/testing.md#6-ci-运行矩阵));SDK-079、SDK-080 的 Model 确定性夹具同时是每个有 Model 的策略必须复用的夹具,策略 crate 以自己的 Model 与事件脚本各跑一遍(cookbook 第 4 步的验证)。
- SDK-177 与 SDK-192 在 x86_64 与 aarch64 上都运行,结果逐位相同。
- 编译期用例(`trybuild`:SDK-057、SDK-105、SDK-114、SDK-174、SDK-187)在 PR 中运行。
- 基准:SDK-196–SDK-205 的数值上限只在 [benchmarks](../../benchmarks/README.md) 登记;CI 只报告,参考机上由 `cargo xtask bench-gate --reference` 判定。
- 上线门槛 ⑧(解耦,[testing §8](../../docs/testing.md#8-上线门槛)):SDK-004–SDK-009 全部通过;M2 末在干净的 `main` 上再跑一次 SDK-004,`crates/`、`apps/`、`tools/hft-cfg`、`python/` 的 diff 为零。

SDK-107 的 sdk_107_production_minimum_roundtrip_and_digest 覆盖可选 prod_min 的 JSON 往返、类型拒绝、摘要与生产/非生产原始值校验。仅边界变化的 schema 可以升版本而不追加字段,既有字段的 since 保持。
