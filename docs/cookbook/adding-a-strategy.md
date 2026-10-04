# Cookbook:新增一个策略

新增策略 = 新建 `strategies/<name>`(Rust crate,只依赖 [hft-strategy-sdk](../../crates/hft-strategy-sdk/README.md) 与 [hft-strat-lib](../../crates/hft-strat-lib/README.md);需要分钟级以上自动生成参数或冷启动预热时,另加同目录的 Python 插件包 `strategies/<name>/pipeline/`)+ 在 [strategies/registry](../../strategies/registry/README.md) 的列表里加一行。**全程引擎零改动**:hft-sequencer、hft-oms、hft-risk、hft-ledger、hft-exec、hft-journal、hft-venue*、hft-event、hft-refdata、hft-ctl*、hft-strategy-sdk、hft-testkit 都不改,ops_bot、hft-cfg、ctl-gateway 与 pipeline 框架的代码也不改(它们按导出的参数 schema 与插件入口点通用处理),apps/hft-core 的规格与容量表也不改——Model 的内存与检查点大小由策略经 `Model::mem_bytes`/`snapshot_bytes` 声明、hft-core 按声明求和,最多需要运维调大配置项 `advisor.mem_budget_mib`。唯一例外是每实例的 group 上限 `max_groups_per_instance`(取值归 [apps/hft-core 容量表](../../apps/hft-core/README.md#内存与-journal-段按主机类唯一归属)):策略需要更多 group 时,先单独提一个 hft-core 容量改动(连同内存与 cell 数核算)并合入,再做策略改动;或者拆成多个账户。如果某一步非改引擎不可,说明 SDK 有缺口:停下,先单独提出 SDK/引擎改动(Agent Note + 评审),不要和策略混在同一个改动里。CI 的解耦门槛([上线门槛](../testing.md#8-上线门槛) ⑧)检查这一点。常驻规则见 [strategies/AGENTS.md](../../strategies/AGENTS.md)。下文以 `foo` 为例;完整的实例是 [portfolio-carry](../../strategies/portfolio-carry/README.md)(含[插件包](../../strategies/portfolio-carry/pipeline/README.md))。

不需要做的事:不登记 `AdviceKind`、不加 `ParamsBody` 变体、不改任何策略枚举、不在引擎里登记 Note 码或报表项、不新增控制命令(人工操作用通用 `StrategyCmd`,预热数据用通用 `InjectWarmup`)、不在 `python/` 下写策略专属代码、不写历史回测。

## 1. 用模板新建 crate,先写规格

1. 生成骨架:`cargo xtask new-strategy foo --kind-id <n>`(`<n>` 是第 2 步登记的编号;其余选项见 [tools](../../tools/README.md#cargo-xtask-new-strategy);未建成前按 [strategies/AGENTS.md 的模块约定](../../strategies/AGENTS.md#模块约定hot-check-按路径施加规则)手工建):`Cargo.toml` 只有 `hft-strategy-sdk`、`hft-strat-lib` 两个内部依赖,`src/lib.rs`、`src/fast/`(带 `#![deny(clippy::float_arithmetic)]`)、`src/model/`(可选)、`src/params.rs`、`src/desc.rs`、`src/advice.rs`、`src/warmup.rs`(有预热载荷时),`benches/budgets.toml` 含 SDK 规定的入口;需要插件时加 `pipeline/`(`pyproject.toml`、`src/hft_plugin_foo/`、`tests/`、`README.md`)。
2. 在 [registry 的 kind_id 登记表](../../strategies/registry/README.md#使用本组件)追加一行,取下一个未用编号;`schema.id = kind_id`。
3. 按 [README 模板](../../crates/AGENTS.md#readme-模板)写 `strategies/foo/README.md`(`kind: crate`,tier 按快速部分标 `hot`):group 形态(腿数、各腿市场类型与方向;各腿在不同交易所——不做同所现货 × 永续)、资金口径、Advice 载荷表、预热载荷表(Model 依赖长窗口历史时)、目标/订单意图的形态、闸门、事件处理表(`Risk`、`External`、`Notice`、`Mode`、`Exec`、`Manual` 逐条)、Note 码表与报表字段、策略命令表(不支持时写一句)、参数表(名称、单位、默认值与来源、范围、是否 per-group、`structural`/`manual_only`/`relax`/`minor_change` 属性;`minor_change` 是门槛 ⑦ 的小改判定,规则见 [testing ⑦](../testing.md#8-上线门槛),不另写白名单)、Model 的内存与检查点上界公式(含路径状态节)、延迟预算;只产生测量样本、不得用于正式资金的策略设 `StrategyDesc.non_prod_only = true`(范例 [latency-probe](../../strategies/latency-probe/README.md))。规则还没定的写明"规则未定、在模拟账户上研究"。有插件包时另写 `strategies/foo/pipeline/README.md`(`kind: python-package`,`tier: cold`)。
4. 决策理由(这个边为什么存在、放弃了什么)写 Agent Note:`.agents/notes/proposed/strategy/<date>-foo.md`。
5. 按 [测试规格格式](../tests/README.md) 写 `strategies/foo/TESTS.md`(README 每个规范性小节对应到用例;用例前缀在其 §3 的前缀表追加一行);第 3–9 步的验证项写成其中的用例,代码与用例同编号。

**验证**:`cargo xtask doc-check` 通过(链接、章节结构、策略 README 不出现交易所名字、`python/` 下不出现策略名);`cargo xtask deps` 通过(只依赖 SDK 与 hft-strat-lib);评审者只看 README 能写出测试用例;`TESTS.md` 的覆盖表覆盖 README 的每个 `##`/`###` 小节。

## 2. 参数 schema 与描述符

1. `src/params.rs` 用 `define_params!` 声明全部字段:阈值、权重、上限、时长都进 schema,代码里不写可调常数;`Qty`/`Px` 字段必须 per-group。决定 Model 已存样本解释的字段(箱宽、范围、窗口、采样间隔)标 `structural`;只许人工设定的(启停、模式)标 `manual_only`;有单调放宽方向的阈值与上限标 `relax = Up | Down`("0 = 关闭"之类使方向不单调的字段标 `None`);门槛 ⑦ 下允许小幅调整的字段标 `minor_change = RelPct(n) | Abs(v) | Any`(结构字段只能 `None`)([SDK 参数 schema](../../crates/hft-strategy-sdk/README.md#参数-schema-与-paramset))。
2. 实现 `check()`:group 形态(腿数、`InstMeta` 市场类型、两腿同资产、计价资产)与跨字段约束,失败返回 `ParamError::GroupShape`/`Invalid`。两腿不同交易所与两腿 lot 对齐(`InvalidArgs{LotMismatch}`)由引擎通用校验,不必在 `check` 里重复。
3. 参数来源:人工与测试用 `hft-cfg` 把 TOML 按 schema 编码成 blob;需要分钟级以上自动生成(名单、流动性分)时,写本策略的插件包 `strategies/foo/pipeline/`:实现 pipeline 框架的插件协议(`hft_pipeline.api`,见 [python/pipeline](../../python/pipeline/README.md#插件接口)),在 `pyproject.toml` 以入口点 `hft_pipeline.plugins` 登记 `foo = "hft_plugin_foo:PLUGIN"`,声明 `api_version` 与 `owned_fields`(人工覆盖项与 `structural` 字段不放进去;预热回看窗口从当前生效的策略参数读取)。两条路径都经 SDK 的 [`params` 编解码器](../../crates/hft-strategy-sdk/README.md#参数编解码器params-模块唯一实现)(hft-cfg 直接调用,框架经 hft-py),插件不编码参数、不写第二个编码器。
4. `src/desc.rs` 声明[描述符](../../crates/hft-strategy-sdk/README.md#策略描述符note报表策略命令):`notes`(每个 `Ctx::note` 用到的码:码 1..=0xFF_FFFF(0 保留)、名、级别 `P1`/`P2`/`Info`、v[] 含义;`Ctx::note` 另带所属 group)、`report`(日报的策略专项字段)、`cmds`(支持人工命令时:每个 `op` 的字段与 `reduce_only`;"只减仓"类命令如单 group 强制退出标 `reduce_only = true`;不支持则为空)。watchtower、journal-archiver、ops_bot 只按它们通用处理。

**验证**:每个 `ParamError` 分支有单测;schema 与描述符的编译期校验通过;规范 schema JSON 与 README 参数表、Note 码表、报表字段表、策略命令表一致;用 `hft-cfg` 从样例 TOML 编码、再用 `decode_body` 解码,往返一致;插件包的 `pytest` 离线通过,只输出 `owned_fields`;改动已有策略时,Note 码、`op` 与检查点节 `tag` 只追加不复用,由 `cargo xtask gen-schema --check` 核对([SDK 版本规则](../../crates/hft-strategy-sdk/README.md))。

## 3. 快速部分

1. `src/fast/`:`new` 按 `GroupTable::capacity()` 预分配 group 状态;`subscriptions` 登记行情(`Part::Fast/Both`);`on_event` 每个事件只处理该事件涉及的 group,零分配、无 f64。
2. 选输出层:跟踪仓位的策略输出 `TargetIntent`(切片、两腿、补腿由 [hft-exec](../../crates/hft-exec/README.md) 做);抢时机的策略输出 `OrderIntent`(两腿用 `LegLink`)。
3. 交易所差异只经 `Views::caps`、`Views::inst`、`Views::fee` 读取;资金口径从 `PortfolioView` 自己算。
4. **报价是否可用按连接健康判**:`v.quote_ok(a, inst)`(引擎阈值)或 `v.bbo(inst).quote_live(now, 自己的阈值)`;不要按 `BboView.rx_tsc`(最近一条 Bbo 的接收时刻)判过期——安静 ≠ 过期([SDK 视图类型](../../crates/hft-strategy-sdk/README.md#视图类型))。按接收时刻的信号用到 `rx_tsc` 时,0 表示未知(关键帧之后、该 inst 的下一条 Bbo 之前),不据此计算。
5. **时间**:跨事件保存的时刻(冷却、确认、封锁、截止)存 `Now.engine_ns`;UTC 给出的时刻(下架、维护)存 UTC ns;快照里不存 TSC;视图里的 `deadline_tsc` 要保存时用 `now.ens_of`([SDK 时间轴](../../crates/hft-strategy-sdk/README.md#时间轴))。`restore` 时 `reset` 确认窗口、`invalidate` 触发价。
6. 按 README 的事件处理表处理 `Risk`(MarginStress 按每 venue 的到强平距离分档)、`External`(在 `deadline_tsc` 前处理,否则执行层默认处理;`cause = Manual` 时执行层不做默认处理并可能置 `manual_hold`(置位与解除条件归 [hft-exec](../../crates/hft-exec/README.md#人工保持与恢复期抑制),`clear_target` 不解除它),策略自己决定,见 [SDK 目标意图](../../crates/hft-strategy-sdk/README.md#目标意图))、`Notice`(下架、结算周期、维护)、`Mode`、`Exec`、`Manual`。要重试卡住的目标(`Stuck(SliceFails)` 等)时改动 `tag`(低位放重发次数)或先 `clear_target` 再 `target`——逐字段相同的目标不会被当作新目标。引擎设 `MarginStress`/`Hard` 目标时会撤掉策略在该 inst 上的挂单,策略照常处理 `Order{Cancelled}`。`Exec{Broken}` 不是终态:执行层可能自动有界平仓、人工 `HedgeFix`、人工核销或 L3/`Kill{flatten}` 减仓后以 `Done{cleared_by_engine}` 解除,策略按实际仓位重新评估;人工核销也可作用于非 Broken 的静止 group,腿净量不经成交而变化([SDK 目标意图](../../crates/hft-strategy-sdk/README.md#目标意图))。门控拒绝(含两腿链接的 `LINK_STALE`、`LINK_FUNDS`)不计为执行失败。目标被拒 `ForcedReduceActive`/`FlattenLocked`(引擎减仓或清仓锁)时只发更接近 0 的目标,`LotMismatch` 时停止该 group 的配对增仓。
7. 处理 `reduce_only` 的 `Manual` 时只产生 Reduce/Flatten 意图(含由它派生的后续分笔)。
8. `snapshot`/`restore` 覆盖全部状态,以 `GroupId` 为键;`on_params` 中新 GroupId 或 `GroupSpec::fingerprint()` 变化的 group 状态从空起步([SDK group 重置](../../crates/hft-strategy-sdk/README.md#参数变化时的-group-重置))。

**验证**:`cargo clippy` 无 `float_arithmetic`、`disallowed_methods`/`disallowed_types`(`Instant::now`、`HashMap` 迭代);`cargo xtask hot-check` 通过。

## 4. 慢速部分(可选)

1. `src/model/` 实现 `Model`:订阅用 `Part::Model/Both`、`model_tick`(UTC 对齐),需要成交、目标进度或人工命令时用 `model_own`(只读);打分、统计、跨 group 分配都放这里。
2. `src/advice.rs` 定义载荷类型(`AdvicePayload`,`LOCAL_KIND` 本策略内唯一,首字节版本号,全部整数定点);引擎不解释载荷,不在 hft-event 登记。
3. 遵守 SDK 的[确定性规则](../../crates/hft-strategy-sdk/README.md#确定性规则):超越函数用 `model_math`,按 gix 升序,不迭代 `HashMap`;模型 tick ≤ 1 ms,超出则按 gix 分片(由 Model 按模型 tick 的 UTC 时刻选片,见 [SDK 延迟与资源契约](../../crates/hft-strategy-sdk/README.md#延迟与资源契约));Model 状态里的时刻存 UTC ns 或 engine_ns。
4. 快速部分对过期或缺失的 Advice 按"不可开仓"处理:`basis_seq < now.boot_seq` 一律过期(跨启动的唯一判据);同一启动内按 `basis_tsc` 年龄判断,不按到达时刻(`Expiring::put_advice`)。
5. 声明 `Model::LAYOUT_VERSION`(从 1 开始)与 `Model::LAYOUT`(原始样本节的布局描述),提交布局金样 `fixtures/strategies/foo/ckpt/layout.json`(`testing::layout_json` 生成);布局或已存样本含义变化时 +1。能由原始样本精确重算的派生状态在 `restore` 时重算,所以改打分逻辑不提升 `LAYOUT_VERSION`;路径依赖的派生状态(按时刻更新的分位带与更新时刻、平滑窗、上次发送摘要等)写进路径状态节(`Model::PATH_VERSION`、`snapshot_path`/`restore_path`,无路径状态时 `PATH_VERSION = 0`),并提交路径金样 `fixtures/strategies/foo/ckpt/path.bin`;周期性更新按 UTC 边界对齐;给出 `path_settle_ms` 与 `cold_start_ms`。检查点不随参数、refdata、发版失效([SDK Model 检查点](../../crates/hft-strategy-sdk/README.md#model-检查点))。`snapshot_chunk` 按 group 分块写出,每块 ≤ advisor 给的 `max_bytes`。
6. 实现 `Model::mem_bytes(p, n_groups)` 与 `snapshot_bytes(p, n_groups)`(上界,按参数实算;hft-strat-lib 的构件各自给出 `mem_bytes`/`snapshot_bytes`,按 group 求和即可),并在 README 写出公式与典型值([SDK 内存与快照声明](../../crates/hft-strategy-sdk/README.md#model-内存与快照声明))。
7. `Model::on_params` 返回 `ModelParamEffect`:新 group、指纹变化的 group、结构参数变化影响的 group 从空起步并报告;非结构参数变化不得重置。被报告的 group 由 pipeline 自动预热。
8. Model 依赖长窗口历史(分位、资金费历史)时,定义预热载荷(`src/warmup.rs`,README 的预热载荷表),由插件包的 `warmup` 生产(每块 ≤ 4096 B、自身完整);Model 对预热块的应用必须幂等、只补空、实时优先([SDK 预热注入](../../crates/hft-strategy-sdk/README.md#预热注入))。有预热载荷的 Model 声明 `Model::WARMUP = true`(结构参数改动预览的 `no_open_ms` 按它估算,口径见 [SDK group 重置](../../crates/hft-strategy-sdk/README.md#参数变化时的-group-重置))。载荷金样 `fixtures/strategies/foo/warmup/` 由策略测试与插件测试共用。

**验证**:SDK 的 Model 确定性夹具([SDK TESTS](../../crates/hft-strategy-sdk/TESTS.md) SDK-079、SDK-080)——同一事件脚本跑两遍 Advice 字节相同;任意位置检查点(任意分块,含路径节)→ 恢复 → 继续与连续运行字节相同,丢弃路径节时 `path_settle_ms` 之后字节相同;布局金样一致且 `testing::check_layout` 通过;最大 group 数、满窗口、预热后 `ModelHarness::peak_alloc_bytes ≤ mem_bytes`、检查点字节 ≤ `snapshot_bytes`;改每个结构参数各一次 → 返回的重置集合正确;预热块乱序、重复、与实时样本交错注入的结果相同,注入后在模型 tick 内就绪。

## 5. 一行注册

1. 在 `strategies/registry` 的 `strategy_set!` 列表里加一行 `foo::Foo,`(optional 策略加 `#[cfg(feature = "foo")]` 与对应 Cargo feature),并在 registry 的 `Cargo.toml` 加依赖。apps/hft-core 与 apps/replayer 经 registry 取得列表,不改;hft-testkit 不依赖 registry,也不改。
2. 编译期断言保证 `kind_id`、`name`、`schema.id` 不重复。
3. 运行 `hft-core schemas --out fixtures/strategy-schemas/` 更新 schema 金样(`<kind_id>-foo.schema.json` 与 `index.json`)并提交(registry 的测试比对它)。
4. 有插件包时,发布构建把它的 wheel 打进发布归档(随 python 统一的 wheelhouse,[development](../development.md));pipeline 实例配置写 `plugin = "foo"`。

**验证**:`cargo xtask deps` 确认只有 registry 依赖 `strategies/*`、引擎 crate 与 hft-testkit 不依赖策略与 registry;本改动只涉及 `strategies/foo`、`strategies/registry`(列表与 `tests/`)与 schema 金样,对 `crates/`、`apps/`、`python/` 的 diff 为零;hft-core 用 `foo` 的 `mem_bytes` 声明装配通过(超出时只调 `advisor.mem_budget_mib`);CI 构建通过;在 `sim` 实例的模拟账户上:`GetSchema{kind}` 返回 schema JSON,`hft-cfg` 生成的 `SetParams` 经 `cli:<login>`(Trader)先 PREVIEW、再带 `expect_state_version` 发送得到 `Applied`(journal 中依次为 `ConfigBlob`、`ControlCmd`、`CmdResult`);改坏一个字段得到 `InvalidArgs` 且旧参数仍生效;账户未绑定该实例得到 `InvalidArgs{AccountBound}`。

## 6. 场景测试

策略 crate 的 `tests/` 用 SDK 的 `testing` 特性(`MockViews`、`CtxHarness`、`ModelHarness`)至少覆盖:

1. 每个闸门的通过与失败;Advice 过期(跨启动 `basis_seq < boot_seq`、启动内超龄)、行情连接不健康、资金费过期时不开仓。
2. **盘口长时间不变**:两腿盘口 10 s 不变、`MockViews::conn` 持续刷新连接存活 → 判定照常可用;连接静默超限、`resync`、连接非 Live → 不可用。
3. **重启**:`MockViews::restart` 后冷却、封锁、截止按 engine_ns 继续(停机不计),重启前的 Advice 过期,确认窗口已重置;快照里没有 TSC。
4. `Ctx` 满(第 17 个订单意图、第 65 个目标)→ 本事件放弃并在后续事件续发。
5. [运行模式行为表](../../crates/hft-strategy-sdk/README.md#运行模式下意图的行为表):`Shadow`、`CloseOnly`、`Live`(含 `open_allowed = false`)、`ShadowDeferred`;`Mode` 事件后开仓状态被重置。
6. `Risk{MarginStress}` L1/L2/L3(按 `venue` 区分,只对相关 venue 的腿响应)、`HedgeBroken`、`CapCleared`;`External`(强平、ADL)在截止前处理,`External{Manual}` 按策略自己的规则处理且引擎不补腿;`Notice`(下架、结算周期变化、维护);`Manual`(支持则测每个 `op`、分笔与上限、`reduce_only` 下只减仓;不支持则 `cmds` 为空)。
7. `Exec{Progress|Done|Stuck|Broken}` 的每种处理(含 `Stuck(EngineOverride)`、`Stuck(Gated(LINK_STALE | LINK_FUNDS))`、`Done{cleared_by_engine}`、Broken 期间 `origin = Engine(AutoFix)` 的 `Progress` 与解除后的重新评估);每个登记的 Note 码至少触发一次(`CtxHarness` 校验码与级别);风险类目标的 `reason` 正确(执行层据此强制永续先);目标被拒(`Ctx::outcome` 返回 `TargetRejected`,含 `FlattenLocked`、`LotMismatch`)时的处理。
8. 用 `LegLink` 的策略:开仓链接的第二腿不带 reduceOnly,平仓链接的带。

需要成交、两腿与故障的**引擎级场景**写在 `strategies/registry/tests/foo_scenarios.rs`:用 registry 的 `Strategies` 实例化 [hft-testkit](../../crates/hft-testkit/README.md) 的通用场景(共用撮合器)跑开仓 → 两腿成交 → 减仓 → 退出;部分成交;手续费以基础币扣除时按净量一致;第二腿盘口 10 s 不变时对冲照常发出。

**验证**:`cargo test -p foo` 与 `cargo test -p hft-strategy-registry` 全绿;`cargo xtask test-trace` 通过(`TESTS.md` 中里程碑已到的用例都有同编号的测试);断言的是意图序列(`OrderDecision`/`ExecDecision` 字节),不是内部字段。

## 7. 确定性与恢复

1. 在 `strategies/registry/tests/foo_recovery.rs` 用 testkit 生成一段含 `foo` 的 journal,跑 `cargo xtask replay-verify <journal>` 两次。
2. 快照属性测试:任意 seq 处"快照 + 其后事件"与连续运行的 `state_hash` 相同。
3. 恢复测试:在任意记录处截断 journal,`RecoverDriver` 截断到最后一个 `EVENT_END` 后,策略状态与连续运行到该事件时一致;换机(TSC 不同)恢复的结果相同。
4. Model 检查点:在检查点之后改非结构参数值、加参数字段、换 refdata 再重启 → 检查点被接受,续算的 Advice 与连续运行一致;改结构参数 → 受影响的 group 被重置并写 `Note{ModelGroupReset}`;改 `LAYOUT_VERSION` → 丢弃并 `Note{ModelCheckpointDiscarded}`;改 `PATH_VERSION` → 只丢路径节并 `Note{ModelPathReset}`,其余照常恢复。

**验证**:两次输出逐字节一致;属性测试 1e5 例通过。

## 8. DST

`cargo xtask dst --seeds 10000 --strategy foo`(目标在 `strategies/registry/tests/foo_dst.rs`;CI 每夜只做冒烟,累计种子由服务器上的 `hft-verify` 运行,账本规则见 [testing §6](../testing.md)):mock 交易所 + 故障注入(断连、重复/乱序回报、限速拒绝、Unknown、强平/ADL 成交、保证金数据过期、`VenueNotice`、第二腿盘口长时间不变、重启)。

**验证**:全部种子通过;断言"无裸敞口超过容差持续超过对冲时延上限""无槽泄漏""引擎仓位 == mock 交易所仓位""`External` 在截止前被策略或执行层处理"。

## 9. 基准

`benches/` 提供 SDK 规定的入口(`strategies/foo/tick_miss`、`target_set`、`advice`、`exec_update`、`risk_event`,以及 Model 的 `model/foo/sample`、`model/foo/tick`、`model/foo/snapshot_chunk`,有路径节时 `model/foo/snapshot_path`,订阅 `model_own` 时 `model/foo/own_event`,有预热载荷时 `model/foo/warmup_chunk`),预算登记在 README 与 `benches/budgets.toml`。

**验证**:`cargo xtask bench-gate` 通过;`seq/bbo_fire` 未因新策略回归 > 10%(数值在[参考机](../../benchmarks/README.md#参考机与-ci)上判,CI 只报告)。

## 10. 模拟账户运行(报告)

1. 按 [deploy](../../deploy/README.md) 冷部署 CI 产出的发布制品(服务器不编译)。
2. 模拟账户在 `sim` 实例上运行(只含 Sim 账户、不持任何 key;dev2 上如此,统一升级后与 `prod` 实例同机,见 [apps/hft-core 实例角色](../../apps/hft-core/README.md#实例角色prod-与-sim))。每个研究中的策略有预置模拟实例池([hft-sim](../../crates/hft-sim/README.md));需要更多时用 `hft-cfg` 写入 `AccountMeta{label, kind: Sim(SimAccountCfg{initial_balances, latency_model, fee_tier, margin_model}), bound_instance}`(`label` 以 `sim-` 开头,前缀规则归 [apps/hft-core](../../apps/hft-core/README.md)),绑定真实 venue 的行情;不同参数用不同模拟账户并行。
3. 对模拟账户发 `SetParams`、`SetMode{Live}`;Model 需要预热时由插件包经框架发 `InjectWarmup`;需要时用 `SimReset`、`SimTransfer`。人工覆盖项按实例分开(`sim` 实例上的试验不流到 `prod`)。
4. 每日报告(journal-archiver/watchtower 产出、ops_bot 展示并标注"模拟"):PnL 分解(资金费、基差、手续费、相对到达价的滑点)、成交率、两腿间隔、裸敞口、保证金占用,以及按 `DESC.report` 从策略 Note 通用汇总的专项字段。
5. 每个上线策略按[上线门槛 ⑦](../testing.md#8-上线门槛)(计时规则、证据绑定与压力用例以 testing 为准;参数改动超出各字段 `minor_change` 会重新计时)。模拟用于观察与改进策略,不设收益门槛;研究结论写进 Agent Note。

**验证**:ops_bot 能看到各模拟账户的日报;watchtower 无 P0;每夜 `replay verify` 通过。

## 11. 正式实盘

模拟账户之后直接上正式实盘,没有小额实盘阶段;切 `Live` 之前按 [launch](../runbooks/launch.md) 做零成交冒烟与正式实例 Shadow(不开仓)。

1. **门槛**:[上线门槛](../testing.md#8-上线门槛)全部通过(含 ⑥ 三所测试网/模拟盘联调、⑦ 模拟账户、⑧ 解耦、⑩ 升级主机上的延迟验收、⑪ 运维演练)。
2. **开通**:按 launch 在统一升级主机上开通新子账户(`non_prod_only` 的策略不能绑定正式账户):交易 key 无提现权限 + IP 白名单、`venue_uid` 登记、账户配置核验(启动闸门 `ACCOUNT_CFG`、`IDENTITY`;持仓模式、现货借币关闭、永续杠杆按策略 README 设定且不超过 `RiskParams.max_perp_leverage_x100`,超过则 `ACCOUNT_CFG` 不符、禁开)、`AccountMeta.bound_instance` 指向本策略实例。一个账户只给一个策略实例。
3. **上限**:风险上限由运营在 `RiskParams` 设定;初始上限与"第一次放宽"的条件是上线签字记录里的必填数值(不是门槛,格式归 launch);参数里 `open_allowed = true` 的 group 由 pipeline 或运营给出;`SetMode{foo, *, account, Live}`(显式账户 + 二次确认 + 闸门已放行)。
4. **并行校准**:同参数的模拟账户与正式实盘并行运行,用实盘结果校准模拟(滑点、成交率、延迟)。
5. **回退**:`Drain`;紧急时 `/stop` 或 `Kill`。

**验证**:ops_bot `/account <a>` 看到 foo 各 group 的模式;journal 中 `CmdResult` 均为 `Applied`;watchtower 无 P0/P1;reconciler 与交易所一致;前 20 笔成交逐笔核对两腿、记账与交易所一致、无裸腿。

## 12. 文档收尾

`strategies/foo/README.md` 的参数表与导出的 schema 一致、`status: impl`(插件包 README 同样);[glossary](../glossary.md) 若有新术语补一行;Agent Note 在验收后移到 `implemented/`。
