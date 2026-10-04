# 第八轮第二次修订简报(M0-b;所有修订子代理先读)

依据:`round8-decisions.md` 的 **R8-8**(用户决定)与 `review-2026-10-03-M0独立评审.md`(独立评审的 P0/P1/P2,含出处行号)。本简报是本次修订的**唯一权威命名与数值来源**;与旧规格或上一轮简报冲突时以本简报为准。原写作规则(`spec-brief.md`、`round8-spec-brief.md` §1)继续适用:中文、标识符英文、README 模板、一件事实一个归属地、交易所行为未核实就标"待核实"、大文件用 Write 工具、只改自己清单里的文件。

修订前副本:`E:\trading\_backup\round8b-pre\`(本次修订前的完整 `hft\` 与 `architecture-v2\`)。读旧内容以它为准。

## 1. 决定与规范解法

### D1 断腿(Broken)处置(评审 P0-2、P0-3)

- **自动有界平仓**:某 (account, group) 处于 Broken 持续 `broken_auto_fix_ms`(ExecConfig,默认 120 000)且期间无人工 `HedgeFix`,执行层自动发起**一次**"平掉多出的一腿"(永续腿 reduce-only;现货腿卖出,数量取 min(库存, 可用余额)),按 `max_reduce_notional` 切片,总量 ≤ 当前裸量;成功 → 写 `Note{ExecAutoFixDone}`(P1),cell 回到 Idle、`HedgeFailure` cap 仍需人工 `Resume`;失败或被门控拒绝 → `Note{ExecAutoFixFailed}`(P0),保持 Broken 等人工。HARD kill 下不执行。
- **L3 与 flatten 覆盖 Broken**:`force_deleverage` 与 `engine_flatten` 对 Broken cell 也生效:先对永续腿 reduce-only 减仓(`PerpFirst`,受 `max_reduce_notional`),再处理现货腿;不再"跳过 Broken"。
- **冷启动不立刻判 Broken**:`hedge_deadline_ms` 只按引擎运行时间累计(停机时间不计);恢复后闸门放行时,对仍未对冲的链接先做一次 `SecondRetry`,再按正常截止判断。

### D2 第二腿检查(评审 P0-3、P0-1 的风控部分)

- B1 链接腿检查加入 **#5**:第二腿 venue 的 BBO 年龄 ≤ `hedge_quote_max_age_ms`(RiskParams,默认 500,与 OMS `price_hedge` 的过期阈值同值同源)。
- B1 加 **第二腿可负担性**:现货买入腿要求该账户该 venue 可用计价资产 ≥ `link_notional × (1 + funds_buffer)`;永续腿要求可用保证金 ≥ `link_notional / 杠杆 × (1 + funds_buffer)`(`funds_buffer` 默认 0.05)。
- 新 RejectBits:`LINK_STALE = 1<<28`、`LINK_FUNDS = 1<<29`(hft-types 登记)。
- **对冲超滑点改价而不失败**:`price_hedge` 在对手价偏离超过 `max_slip` 时,按 `rr_max_cross` 上限定价发出(仍是 IOC);只有超过 `rr_max_cross` 才返回门控拒绝(不计执行失败次数)。
- **第二腿数量上限**:OnFill 第二腿数量 = min(按成交净量算出的量, LegTracker 当前该方向的裸量);这同时消除"无 client id 成交先按 ExternalFill 处理、后又匹配到槽"造成的重复对冲。
- gw 暂存无法归属的成交 `gw.unmatched_hold_ms`(默认 5 000)后才发 `ExternalFill{Unknown}`。

### D3 不做同一交易所现货 × 永续(R8-8 第 1 条;评审 P0-1)

- refdata 加载校验:一个 `AccountId` 的 venue 集合中不得有两个 `exchange` 相同的 VenueId(错误码 `SameExchangeMarkets`);Sim 账户同样适用。
- `SetParams` 校验:同一 GroupSpec 的两条腿不得在同一交易所(`InvalidArgs{SameExchangeLegs}`)。
- 删除 `balance_domain` 字段与相关逻辑(refdata、reconciler、hft-sim);在 hft-refdata 写一句"同所现货与永续共用钱包的情形被本规则排除,因此不建模余额域"。
- 架构与策略文档里"同所现货 × 永续可接入"改为"不做"。

### D4 模拟账户的位置与优先级(评审 P1-4、P1-3)

- sequencer 输入环优先级改为:`ctl > rep(实盘) > bbo > advice > md > rep-sim`(rep-sim 最低);每轮 sim 处理量上限 `sim_max_per_round`(默认 32 条);同一事件派发时实盘实例先于模拟实例。
- dev2:每个研究中的策略预置 `sim_pool = 2` 个模拟实例(hft-sim 的"预置模拟实例池"改为 2)。
- 统一升级主机后:模拟在**独立的 sim-only hft-core 实例**运行(`instance = "sim"`;refdata 只含 Sim 账户、不加载任何交易所 key;自己的行情连接与 journal);实盘实例(`instance = "prod"`)不含 Sim 账户。同一二进制。它不交易任何交易所账户,不违反"同一账户只由一个系统交易"。journal 实例名只允许 `"prod" | "sim"`。
- c8 线程计划:`persist` 不与 `advisor` 同核。

### D5 冷启动预热与 Model 检查点(评审 P1-2)

- **检查点键**只含:`kind_id`、`Model::LAYOUT_VERSION`(策略声明的状态布局版本)、`schema.id`、`strategy_instance`、`last_seq`;参数版本、refdata 版本、二进制 SHA **不**使检查点失效(参数变化随重放经 `model_on_params` 重新应用)。命名统一:配置 `advisor.model_checkpoint_ms`(默认 600 000);Note `ModelCheckpointDiscarded`。
- **预热注入通道**:新控制命令 `InjectWarmup{instance, local_kind: u16, chunk_no: u32, last: bool, payload ≤ 4096 B}`(主体:Trader 或 pipeline),作为 Input 记 journal,路由到该实例的 Model(SDK `EventRef::Warmup{local_kind, chunk_no, last, payload}`),也可选投递给快速部分;载荷由策略定义;生产者是 pipeline 的策略插件(portfolio-carry 插件从交易所 REST K 线或 collector 数据生成样本)。回放确定。
- 检查点存放在冷盘,磁盘上限 dev2 1 GiB(两代合计;这是磁盘不是内存)。

### D6 资金费对账(评审 P1-5)

- 资金费条目**不进** `RECONCILED` 闸门、不触发 `Recon` cap;只告警(watchtower:差额 > `funding_diff_alert`(默认 1 USDT 或 1%)→ P1)。
- 账本自动校正:reconciler 的 `Funding` 条目带交易所逐笔资金费(account、venue、inst、settle_ts、amount);ledger 对结算时刻早于 `funding_recon_lag_ms`(默认 600 000)的差额自动入 `AdjustDelta::Funding = 3`,写 `Note{FundingCorrected}`;比对窗口为最近 48 h。

### D7 CI 与免费额度(R8-8 第 2 条)

- 与旧项目一致:`.github/workflows` 用 GitHub 托管 runner;PR/推送的质量门(fmt、clippy、单元/属性/金样、fuzz 冒烟、DST 冒烟、解耦门槛、doc-check)跑在 `ubuntu-latest`(x86_64;纯逻辑 crate 跨平台);发布构建按 tag 在 `ubuntu-24.04-arm` 产出 aarch64 制品;每夜定时任务总时长 ≤ 40 min(DST 一批种子 + fuzz 按解码器轮转,语料用 cache/artifact 持久)。不用自托管 runner,不在 dev2 上跑 fuzz/DST。
- 上线门槛中的长时间项改为**累计量**:每个 core 解码器累计 fuzz ≥ 2 h、零 panic;DST 累计 ≥ 200 000 种子、零违反,且覆盖 ≥ 7 次每夜运行;任何失败使对应计数清零。具体额度以账户的免费分钟数为准(待核实),超出时减少每夜批量而不是加付费资源。

### D8 portfolio-carry 负资金费平仓门(评审 P1-6)

平仓收益 `close_gain_daily = max(0, max(best_other_T, 0) − T_i)`;活跃集为空时 `best_other_T = 0`。

### D9 老板紧急停止(R8-8 第 3 条)

`Viewer` 角色增加唯一的写权限:非 HARD、不带 flatten 的 `Kill{Global | Account(a)}`,经 ops_bot 二次确认(PREVIEW + `expect_state_version`)后发出;不能 `Resume`,不能发任何其他写命令;审计照常。ops_bot 命令 `/stop [account]`(中文提示"紧急停止")。

## 2. 评审其余问题的规范解法

| 编号 | 解法 | 归属 |
|---|---|---|
| P1-1 | `EventRef::Advice { gix, local_kind, payload, basis_seq: u64, basis_tsc: Tsc }`;策略按 `basis_tsc` 判过期;SDK 中 `AdviceHdr` 描述与 hft-event 一致(字段与顺序以 hft-event 为准);watchtower 新规则 `AdvisorLag`(滞后 > `advisor_max_lag_ms`,默认 5 000 → P1) | SDK、hft-event、hft-sequencer、watchtower |
| P1-3 | dev2:`max_instances` = 8;容量分两个名字:`max_groups_per_instance`(SDK,默认 64)与 `max_cells_total`(sequencer/exec,dev2 256);advisor 堆 dev2 256 MiB;按首个策略重算 hft-core 内存表;dev2 护栏改为**只按资源信号**(`MemAvailable`、PSI)停 hft,cdex 单元重启或状态文件过期只告警,不停 hft | apps/hft-core、hft-sequencer、hft-exec、SDK、deploy |
| P1-7 | `StrategyDesc` 增加 `notes: &'static [NoteDesc]`(码、名、级别、v[] 含义)与 `report: &'static [ReportFieldDesc]`(名、单位、聚合方式 Count/Sum/Last/Max/P99),随 schema JSON 导出;watchtower 与 journal-archiver 按描述符通用汇总策略 Note,生成实盘与模拟日报;`ManualOpen/ManualClose` 改为通用 `StrategyCmd{instance, group, payload ≤ 256 B}`(SDK `EventRef::Manual{payload}`),载荷含义由策略定义;引擎 README 中删除 `cdex` 字样(改用"链上策略"等通用说法;只有 optional 链上组件的触发条件行可以提 CEX-DEX) | SDK、hft-event、control-plane、ctl-proto、watchtower、journal-archiver、strategies/cdex、各引擎 README |
| P1-8 | pipeline 经 hft-py 调用 SDK `params` 编解码,不再有纯 Python 编码器;schema 导出统一为 `hft-core schemas --out <dir>` → `<kind_id>-<name>.schema.json` + `index.json` | python/pipeline、hft-py、registry、tools |
| P2 | 结构体字节数按字段重算(`Tomb` 56 B、hft-risk `MarginRow` 64 B、ledger Margin 行 56 B;其余发现的也改);`GROUP_NONE = GroupId(0)`(无组 / unassigned / 定时器不适用),`GROUP_ALL = GroupId(u32::MAX)`(命令作用域 "*");删除悬空的 `Ack` 命令引用(对账条目只经 `AdoptExchange`/`SetBalanceBaseline` 处置);`RecoverMismatch` 为 P0;`SimAccountCfg = { initial_balances, latency_model: u16, fee_tier: u8, margin_model: u16 }`,两个 model 是 hft-core 配置 `[sim.latency_models]`、`[sim.margin_models]` 表的下标,模型定义归 hft-sim;ledger 外来成交归属只在非 Shadow cell 中找,歧义 → unassigned;OMS 在 `boot_changed` 时同样重算 `LinkEntry.first_fill_tsc` 与 `Tomb.freed_tsc`;pipeline 主体发的 `SetParams` 为中性(不需确认),人工主体把 `open_allowed` 0→1 为升级;Sim 账户的 kill 与实盘相同经检查链生效(hft-core 的另一种写法删除) | 各归属文件 |

## 3. 工作分配(只改自己清单里的文件)

| 代号 | 文件 |
|---|---|
| G1 | `architecture-v2/README.md`、`00`–`13`(13 末尾追加 §15"第八轮第二次修订")、`现状测量…`、`待测量…` |
| G2 | `hft/crates/hft-strategy-sdk`、`hft/crates/hft-strat-lib`、`hft/strategies/{AGENTS.md,registry,portfolio-carry,cdex,cross-taker,maker-taker}`、`hft/docs/cookbook/adding-a-strategy.md` |
| G3 | `hft/crates/hft-exec`、`hft/crates/hft-oms` |
| G4 | `hft/crates/hft-risk`、`hft/crates/hft-ledger`、`hft/crates/hft-sequencer` |
| G5 | `hft/crates/{hft-types,hft-event,hft-refdata,hft-ctl-proto,hft-ctl}`、`hft/docs/subsystems/*`、`hft/docs/cookbook/{adding-an-event-kind,adding-a-control-command}.md`、`hft/tools/README.md` |
| G6 | `hft/crates/{hft-venue-proto,hft-venue,hft-net,venue-binance,venue-okx,venue-bitget,venue-gate,venue-bybit,venue-kucoin,venue-hyperliquid,hft-sim}`、`hft/apps/{hft-core,replayer}`、`hft/docs/cookbook/adding-a-venue.md` |
| G7 | `hft/apps/{reconciler,watchtower,kill-switch,journal-archiver,mock-exchange}`、`hft/crates/{hft-testkit,hft-telemetry}`、`hft/docs/testing.md`、`hft/benchmarks/*`、`hft/fixtures/README.md` |
| G8 | `hft/python/**`、`hft/deploy/*`、`hft/docs/development.md`、`hft/docs/runbooks/*`、`hft/docs/postmortem/README.md`、`hft/.agents/notes/**`(新 ADR:`2026-10-03-{broken-auto-fix,no-same-exchange-spot-perp,sim-only-instance,warmup-injection,funding-recon-alert-only,ci-free-hosted,viewer-emergency-stop}.md`,并更新索引与受影响旧 ADR 的状态行) |
| G9 | `hft/{AGENTS.md,CLAUDE.md,README.md}`、`hft/docs/{AGENTS.md,architecture.md,glossary.md,defensive-patterns.md,hot-path-rules.md,latency-budget.md}`、`hft/crates/AGENTS.md`、`hft/apps/AGENTS.md`、`hft/docs/cookbook/adding-a-crate.md`、`hft/crates/{hft-journal,hft-rt,hft-book,hft-py,hft-chain-evm,hft-ext-proto,hft-amm,hft-chain-solana}`、`hft/apps/{chain-core,chain-grpc-relay}`、`hft/contracts/README.md`、`architecture-v2/_work/check_hft.py`(新增检查:scope core 的引擎 README 出现小写 `cdex`;`balance_domain` 残留;`Ack` 命令残留;`GROUP_ALL = 0` 残留) |

## 4. 最终报告(简短)

1. 改了哪些文件。2. 本简报 D1–D9 与 §2 各项中你负责的,落在哪个文件哪一节。3. 需要别人同步的名字或数字。4. 仍有风险的地方(≤ 5 条)。

## 5. 主控补充裁决(修订与合并过程中)

1. 断腿自动平仓在组内有 Pending*/Unknown 槽或活动链接、闸门未放行时等待(2 分钟计时照走),组静止后立即执行;HARD kill 期间暂停。
2. CI 增加每夜 aarch64 短任务(`ubuntu-24.04-arm`,≤ 10 min,含在每夜 40 min 内);arm 托管 runner 对私有仓库是否免费待核实,回退方案 = 在 `ubuntu-latest` 上交叉编译 aarch64(文档化,非默认)。
3. `StrategyCmd` 带发送方声明的 `reduce_only`:声明时为中性命令(DRAINING、CloseOnly、闸门未放行时也可发),处理期间该实例的 Open 意图与目标增加一律被拒。
4. ops_bot 二次确认必须由发起预览的同一 `from.id` 按下(群聊亦然)。
5. 策略自声明的 Note 级别上限 P1(P0 只留给引擎)。
6. `funding_diff_alert` = |差额| > max(1 USDT, 交易所金额 × 1%)。
7. D8 公式修正为 `close_gain_daily = max(0, max(best_other_T, 0) − T_i)`(原式在活跃集为空时把 |T_i| 计两次)。
8. 跨开机的 Advice 新鲜度:SDK `Now.boot_seq`(本次启动 `EpochStart` 的 seq);`basis_seq < boot_seq` 的 Advice 视为过期;TSC 不跨开机比较。
9. 合并时补:`BlobKind::SimConfig = 6`;refdata 拒绝不同 `AccountId` 登记同一 (exchange, `venue_uid`)(`DuplicateVenueUid`);Sim 模型分布校验归 hft-sim(`SimCfgErr::BadDist`);网关按 `CmdDesc` 校验 `StrategyCmd` 载荷;hft-py `encode_cmd`;执行层推进顺序实盘先于模拟;`RateOnly`/`BalanceOnlyPush` 所在资金费校正窗口内放宽余额容差;护栏一级先停 `hft-replay-verify`;sim 实例档与"未加载检查点"信号(`ModelCheckpointDiscarded` → pipeline 自动预热)。
