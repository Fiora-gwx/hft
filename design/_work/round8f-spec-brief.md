# 第八轮第六次修订简报(M0-f;最后一次文档统一;所有修订子代理先读)

依据:`round8-decisions.md` 的 **R8-12**(用户:"可以开始写代码了,文档上的不一致再统一下,上面这些策略相关的先不用管了……测试要严格写好测试文档")与第五轮五路评审(`review-2026-10-03-M0e-X1…X5*.md`,含出处行号)及汇总 `review-2026-10-03-M0e五路汇总.md`。本简报是本次修订的**唯一权威命名与数值来源**;与旧规格、旧简报(round8e 及更早)或评审原文冲突时以本简报为准。修订前副本:`E:\trading\_backup\round8f-pre\`。

**本轮的性质**:这是进入实现(M1)之前最后一次文档统一。合并后规格冻结为设计基线 v1,不再做全量文档评审;此后只有"实现驱动的变更"(实现或测试发现问题 → 同一提交改 README、TESTS.md 与代码)。所以:**本轮只修已列出的问题与你在自己文件里发现的自相矛盾,不新增功能、不扩写**;能删的重抄就删,改为链接归属地。下一阶段会为每个组件写 `TESTS.md`(测试规格),本轮不写。

## 0. 写作规则(不变,重申)

- 中文、标识符英文;一件事实一个归属地,别处链接;**不在归属地外重抄表格、名单、公式或数值**(第五轮的 P0-4、P1-14、19、21 都是重抄分叉);未核实写"待核实(方法:…)",合法变体只有"待标定""待实测";大改用 Write 整体重写或 Edit 精确替换;**只改自己清单里的文件**;写当前状态,不写历史;开发备忘只放待核实与开放问题。
- 不得丢章节:完成后运行 `PYTHONIOENCODING=utf-8 python E:\trading\architecture-v2\_work\heading_audit.py E:\trading\_backup\round8f-pre`,消失的标题只允许有意删除或改名(报告里列出)。
- 可以只读运行 `PYTHONIOENCODING=utf-8 python E:\trading\architecture-v2\_work\check_hft.py --brief E:\trading\architecture-v2\_work\round8f-spec-brief.md`(J12 在改它;运行出错就跳过)。
- 别人文件里的问题不要改,写进最终报告的"留给别人"。

## 1. 工程类决定的落地(R8-12 第 2 条;汇总 §2 第 1、4、5、6、7、8、9 组)

### F1 引擎兜底与恢复(X1 §5-1…7、X1 P1-A…E)

- (a) **`resolve_broken` 只在删除带 HARD 的 Kill 记录时产生**;判定阈值改为补腿口径(该 group 裸量 > `tol` 且名义 > 尘埃线),跳过 `manual_hold` 的 cell。`Resume{Kill(非 HARD)}`、其他锁存的解除与 Recovery 的自动解除都不产生它;放行后的补腿与外来成交默认处理按"放行后新窗口"处理。规则归 hft-exec(判定)与 hft-sequencer(调用点),hft-risk 只写"HARD 解除时产出 `resolve_broken`"并链接。
- (b) **恢复期抑制按"多出那一腿所在 venue"判定**:补腿、残余修剪、外来成交默认处理与自动有界平仓的开始条件,由"账户启动闸门放行"改为"多出那一腿所在 venue 的对冲闸门子集(`hedge_gate_ok`)满足 ∧ 该 venue 的 `RECONCILED` 已置位"。**`broken_auto_fix_ms` 与外来成交截止在该条件不满足期间不计时**(X1 P2-15 择此口径;testing 与 hft-exec 统一)。账户启动闸门未放行而存在 Broken 或超过补腿阈值的裸量持续 > `gate_naked_alarm_ms`(300 000,hft-exec 参数)→ `Note{ExecGateHeldNaked}`(P0,登记 hft-event)。
- (c) **`AUTO_EXTERNAL` 的前置条件**另加:两轮对账都没有 `USERSTREAM_DOWN`,且该 (account, venue) 用户流自最近一次 Up 起已稳定 ≥ 1 个对账周期(reconciler 置位与 hft-sequencer 复核都查)。**合成成交在 `ReconcileReport` 这条 Input 内确定地合成,不另写记录**(X3 P0-3 的统一口径;reconciler、defensive-patterns、testing AX-1 改为这一说法并链接 hft-sequencer);合成逻辑是 hft-ledger 的一个函数,sequencer 与冷读者(reconciler 的引擎视图、hft-py)都经 hft-ledger 的 journal 消费路径调用同一函数,所以冷读者能看到这笔成交。hft-ledger 对 `AUTO_RECON` 入账量按 (account, inst, 方向) 记 `auto_held`(进快照);之后同 inst、同方向的 `ExternalFill{Adl | Liquidation | Unknown}` 先抵扣 `auto_held`,只入账超出部分,并写 `Note{LedgerAutoHeldOffset}`(P0,登记 hft-event)。三所断流期间的强平/ADL 重连后是否补推:待核实(方法:测试网断流演练)。
- (d) **Drain 与引擎减仓**:Drain 的等待条件改为 `exec.drain_ready()` = 原 `quiescent` ∧ 没有未到达的 `reason ∈ {MarginStress, Hard}` 引擎目标(仍受 `drain_timeout_ms` 约束;`DrainTimeout` 的 P0 列出未完成的账户);hft-sequencer 的 Drain 写"执行层停止**增仓**切片"(与 hft-exec 一致);`StorageCritical` 在有 `force_active` 或 `flatten_lock` 时推迟到 journal 硬阈值。`Kill{flatten}` 在 DRAINING 中照常允许(R8-11 不变)。
- (e) **引擎订阅改为"持仓驱动"**:订阅并集 = GroupSpec 交易腿(现规则)∪ 该实例账户中 `InstPos.qty ≠ 0` 的永续 inst(BBO、MARK、FUNDING)∪ 有现货库存的 inst(BBO)∪ EngineConfig 段 2 的每个 `ref_inst`(BBO);持仓变化引起并集变化时沿用"并集变化重发 `FeedCmd::Subscribe`"。pipeline 移除 group 时提示该 inst 仍有账户级持仓(只提示)。
- (f) **核销**:`WriteOffResidual{account, group}` 适用于**静止的** group(Broken 或非 Broken 均可;静止的定义归 hft-oms),把多出那一腿的已成交裸量移到 unassigned(`AdjustDelta::WriteOff`),并**清零该 group 全部腿的 `residual`**;返回仍是单条 `WriteOff{venue, inst, leg_ix, qty}`(无裸量时 `qty = 0`)。取代合并裁决 R2 中"只清多出那一腿余数"与"只限 Broken"的说法。
- (g) **陈旧停机标记**:`stop_core` 停机失败(`core_stop ∉ {Stopped, Inactive}`)时,kill-switch 把标记改名为 `/var/lib/hft/kill-switch/stop-marker.json.aborted` 并 P0;hft-core 启动只读 `stop-marker.json`;incident-response R2 加一步"确认标记已作废,是否另发 Kill 由人决定"。
- X1 P2-1…19 全部按 X1 §3 的"修"执行(P2-6、7 已由 (f) 覆盖;P2-11 由 (g) 覆盖;P2-15 由 (b) 覆盖)。

### F2 门槛 ⑩ 的证据(X2 §5-4、X5 §6-1、2、8)

- (a) hft-oms 为撤单与改单也记时间戳并合成样本;hft-event `LatencySample` 追加 `action: u8`(0 = New、1 = Cancel、2 = Amend;优先占用现有保留字节,大小不变则不升版本,否则按只追加规则升版本);`order_rtt` 只统计 `action = New`;`rep_to_order` 的样本包含三种 action。
- (b) hft-event `MarketHdr` 追加 `rx_empty_gap_ns: u32`(本连接上次确认接收队列为空的时刻到本次读到这批数据的间隔,饱和到 `u32::MAX`;无确认时为 0;同上的占用保留字节规则);⑩ 报告并列 `kernel_rx` 的上下界;M1.5 与 M3 末探针按"突发 / 平静"分层抽样(分层规则归 testing §8.2)。TCP 一次读到多个包时 `hw_rx` 取哪个包的戳写清楚(取最早一个包;拿不到时标记)。
- (c) HW RX 时间戳不可用或 `unsynced_cal` 占比偏高(阈值待标定)时,⑩ 改用 SW 锚数据集判定,数值门槛不变,报告注明"不含网卡到软中断一段";ENI-B2 的启用判据另加两条替代("回显对端 RTT 的 A/B"或"软中断排队时长")。
- (d) 同机干扰:先做 1–2 天 A/A(两段都关 sim)估计区间宽度;开关顺序按资金费块分层配平;区间过宽时延长窗口,不判失败。规则归 testing §8.2,host-upgrade 引用。

### F3 延迟结构(X5 §6-3、4、5、6、9;按 X5 §1-6 的时间点)

- (a) **现在定(M1.5 前)**:gw 的 REST 连接只在有在途请求时 poll,另每 `gw.rest_idle_check_ms`(1 000,hft-venue 参数)做一次存活检查;standby 连接每个 aux 圈 poll 一次;新增基准 `gw-ingest/poll-cadence`(benchmarks 登记)与跨 gw 的探针变体;不改 T7(gw 不用 `epoll_pwait(0)`)。
- (b) **现在定(M1 第一周)**:关键帧另加事件数触发 `keyframe_max_events`(1 500 000)与最大推迟(到期后最多推迟 2 个 `keyframe_period_ms`,之后在下一个事件边界强制写);**实盘写关键帧时不计算 `state_hash`**(头部该字段写 0,由 replayer/读者侧计算;journal-format 与 hft-sequencer 一致);hft-journal 段滚动的安全阀分两级:先在空闲轮滚动,超过硬上限才在事件边界强制滚动(数值归 hft-journal)。
- (c) **M2 动工前定(本轮不改规格,只在 11 §3"各 crate 动工前必须定下的延迟决定"登记方向)**:rep 与 ctl 各拆"紧急/批量"两环、bbo/rep 逐条跨环轮转、a 段与 ctl 之后也查 rep、`SetParams` 应用预算。
- (d) M3 末满订阅探针同时按 c32 计划跑一次并做 ENI-B2 拆分 A/B(另租几小时 c8g.8xlarge,`[需授权]`);11 §3.2 写。
- (e) M1.5 前在 hostprobe 规则里登记 IRQ 核"保持唤醒"的允许形式(`nohlt` 或列入白名单的 keepawake 线程),tools 写。
- X5 的其余 P1/P2:L-P1-1 → (b)/F2(b);L-P1-2 → F2(c);L-P1-3 → (a) + F2(a);L-P2-1/2/4 → (c) 登记;L-P2-3、5、6、7、9、10、11 按 X5 修法(L-P2-6 由 (b) 的事件数触发解决,写明恢复尾部上界)。

### F4 dev2(X4 §5-1、4、6、7)

- (a) 批处理改放**根级** `hftbatch.slice`(名字不带连字符;systemd 中 `hft-batch.slice` 会落在 `hft.slice` 之下),设 `CPUWeight=idle`(cgroup `cpu.idle`;线程级 SCHED_IDLE 不跨 cgroup 生效);各批处理单元保留 `Nice=19`,删除 `CPUSchedulingPolicy=idle` 的依赖说法。全仓 `hft-batch.slice` 改名为 `hftbatch.slice`。
- (b) 只含 Sim 账户的实例非干净重启后,无 `RecoverMismatch`、无毒丸时自动解除 Recovery;否则写 `Note{RecoveryHeldSim}`(P1,登记 hft-event)。规则归 hft-risk(解除条件)与 hft-core(实例判定)。
- (c) 护栏只在 hft 自身用量明显(`hft.slice` 或 `hftbatch.slice` 的 CPU 或内存超过阈值,待标定)时才停机,否则只告警;dev2 退路的"一周触发 ≥ 2 次"只计这种情况。规则归 hft-core。
- (d) dev2 上只对 hft 各 uid 加 IMDS 的 nft 丢弃规则,并在 dev2 也运行 `hft-cred-broker`(不影响旧系统)。规则归 deploy。

### F5 CI 额度(X4 §5-2、N-P1-3)

- quota-gate 用 `actions: read` 列出本月 workflow run 的作业并按作业时长与 runner 倍率自估用量(GitHub 的用量接口已关闭);每周与计费页人工对账一次,偏差写 ops-log。
- quota-gate 与发布 `preflight` 都接受 Admin 覆盖:`workflow_dispatch` 输入 `quota_override: true`,只认 Admin(按 `github.actor` 对照名单),覆盖写作业摘要。
- 用量过 1,500 min 后,不带 `hotfix` 标签的 PR 只跑 docs 检查与 `decoupling-gate`;带 `hotfix` 的 PR 照常跑全部质量门。
- 额度耗尽的应急:把仓库镜像到负责人个人账户的私有仓库,在那里运行 `release-aarch64.yml` 出包(仍免费,占 Trading 的额度);步骤写进 development(唯一归属)并每季度演练一次。`release-x86-cross.yml` 保留为"arm runner 不可用"时的路径(不是额度路径)。

### F6 测试网身份(X4 §5-3、N-P1-7)

- 账户配置增加 `uid_policy = "verified" | "unverifiable"`(RF4 小写;默认 `verified`;归 hft-refdata 的账户元数据,gen-schema 导出)。`Unverifiable` 只允许 `is_prod = false` 的账户,装配时需要 Admin 签字记录;此时 `IDENTITY` 由"kill 自检通过 + 一次成功签名的只读请求"放行,并写 P1 Note 标明未核验。venue-binance 按 Binance Demo Trading 端点核实(现货与 USDⓈ-M),待核实写法照旧。

### F7 `verify` 开关的切换(X4 §5-5)

- 同一发布里放两份主机清单(M7 用 `verify = true`、M8 用 `verify = false`;文件名由 deploy 定并在报告里给出),M8 只改 `host-local.toml` 的清单选择后重新激活,不另发补丁版。规则归 deploy,launch/host-upgrade 引用。

## 2. 暂缓(策略相关;保留现文)

- 汇总 §2 第 2 组(portfolio-carry 只平不开节奏、逐步减仓、`inst_status` 视图、压力用例方向)、第 3 组(门槛 ⑦ 计时规则)、第 10 组(首批下单产品线数;c32 默认、c16 条件候选的现文不动)。
- X2 中只涉及 portfolio-carry 行为、参数或 ⑦ 计时的条目(P1-1…4、P2 中 pc 相关项)。**例外**:X2 §5-4 → F2(a)。
- 涉及处如有矛盾的现文,只在相应位置加一句"策略研究后定(R8-12)",不改语义。

## 3. 其余修正(全部按评审修法)

- **X3**:§3 的 P0 1–11、P1 1–31、P2 1–62 全部按"修法"列执行;P0-3 按 F1(c);P0-6…9、11 是权限闭包——deploy 路径表、用户表、单元 `SupplementaryGroups`/`ReadWritePaths` 一次补全,各 README 的路径默认值与"X 读/写 Y"声明与之一致。X3 §4 第 1 条提到的简报问题已由本简报取代(E10 路径、E13(a) 签名、R8 理由不再有效)。
- **X1**:见 F1。
- **X4**:N-P1-1 → F4(a);N-P1-2(`verify = false` 时回放单元失败;并发规则进 deploy 主表);N-P1-3 → F5;N-P1-4(M7→M8 补 kill-switch 重启、pipeline 配置与状态切换、停机标记归档);N-P1-5(dev2 IAM 以 deploy 为准,补 S3 Get/List 与 KMS,launch 链接);N-P1-6(探针清单 `hosts/probe-c16.toml` 与 c32 探针清单随正式版发布,M3 末探针有装机步骤);N-P1-7 → F6;N-P1-8(`journal-archiver restore` 命令、DurableMeta 上传、换 AZ 恢复时 epoch 的安全间隔与恢复代);X4 P2 全部按修法(P2-1 → F4(b),P2-2 → F4(c),P2-6 → F7,P2-8 → F4(d))。
- **X5**:见 F2、F3。
- **X2**:除 §2 暂缓项外,解耦与 SDK 的一致性问题按修法(不新增 SDK 能力)。

## 4. 新机检(J12;X3 §4 的 12 项)

权限闭包(deploy 路径表 × 用户表 × 附加组 → 每个单元用户对每条路径的穿越/读/写;与各 README 的路径默认值和"X 读/写 Y"声明比对;创建属组 G 的套接字或文件的进程必须在 G 里);组成员一致("G 组只有 A 与 B"、`SupplementaryGroups`、用户表三方);S3 键模板必须是 `<prefix>/<host>/<instance>/…`;IAM 与 secrets 授权集合按主机类比对,KMS 覆盖每个 SSE-KMS 前缀;单位归一公式比对(`_us/_ms/_ns` 归一后不等式等价);线上码字段必须有数值表("arg = X"、"码 = X"、`timer_id` 的 X 有显式判别值或编码式);原样嵌入检查(标"原样/布局原样"的行嵌入 `repr(C, u8)` 数据枚举或"不以原始字节写进 journal"的类型报错);只追加规则(对比 `_backup/round8f-pre` 的同名结构:`_rsv` 缩短、体中部插入字段、行布局变而版本不升报错);"全部主机类"规则必须出现在 deploy 主表;名单副本比对("只有/只含/白名单"后 ≥ 3 个反引号标识符的列表跨文件集合相等);预算上下限(benchmarks 上限表与各 crate 预算表数值比较);简报漂移(本简报 §6 的签名、类型、路径与规格定义比对,警告级)。误报多的做成警告级。`--brief` 默认指向本简报。

## 5. 工作分配(只改自己清单里的文件)

| 代号 | 文件 | 重点 |
|---|---|---|
| J1 | `architecture-v2/README.md`、`00`–`13`、`现状测量…`、`待测量…` | F2(00 的 ⑩ 证据口径链接 testing)、F3(c)(d) 登记于 11 §3/§3.2、F4、F5(11 §2 只链接 development)、F7;§2 暂缓项在 11/07 加"策略研究后定(R8-12)";13 追加 §19(第五轮评审与 R8-12 摘要,含"设计基线 v1 冻结");12 风险更新;X3 中 A2 相关行 |
| J2 | SDK、strat-lib、`strategies/AGENTS.md`、`strategies/registry/**`、`strategies/{latency-probe,portfolio-carry,cdex,cross-taker,maker-taker}/**`、`docs/cookbook/adding-a-strategy.md` | portfolio-carry 与 cdex 等**只修一致性**(名字、链接、X3 行),不改行为与参数(§2);SDK 一致性(X3 相关行;F1(f) 核销语义的 SDK 侧说明;F2(a) 若 SDK 有提及);latency-probe 的样本口径(F2(a)) |
| J4 | `crates/hft-exec`、`crates/hft-oms` | F1(a)(b)(d)(f) 的执行层与 OMS 侧、X1 P2(exec/oms 部分)、F2(a) OMS 样本合成;X3 相关行(含 P0-1 节 21 `ExecCellRow` 的显式行) |
| J5 | `crates/hft-risk`、`crates/hft-ledger`、`crates/hft-sequencer` | F1(a)(b)(c)(d)(e) 的风控/记账/sequencer 侧、F3(b) 关键帧事件数触发与 `state_hash` 规则(sequencer 侧)、F4(b) 风控侧;X1 P2 相关;X3 相关行 |
| J6 | L0(`hft-types`、`hft-event`、`hft-refdata`、`hft-ctl-proto`、`hft-ctl`)、`docs/subsystems/*`、`docs/cookbook/adding-an-event-kind.md`、`docs/cookbook/adding-a-control-command.md`、`tools/README.md` | 新码登记:`Note{ExecGateHeldNaked, LedgerAutoHeldOffset, RecoveryHeldSim}`、`LatencySample.action`、`MarketHdr.rx_empty_gap_ns`;X3 P0-2 `LoadErrCode` 数值码表(hft-refdata,`#[repr(u16)]`,只追加,gen-schema 导出);F6 `uid_policy`(hft-refdata);journal-format 的关键帧 `state_hash` 规则(F3(b));control-plane 的核销语义(F1(f));tools:F3(e) hostprobe 规则、F5 quota-gate 若在 tools 有描述则链接 development;X3 相关行 |
| J7 | `hft-venue-proto`、`hft-venue`、`hft-net`、`venue-binance`、`venue-okx`、`venue-bitget`、`docs/cookbook/adding-a-venue.md`、`fixtures/README.md` | F3(a) gw 收包节拍、F2(b) `rx_empty_gap_ns` 的产生与 `hw_rx` 取戳规则、F6 Binance Demo Trading 端点核实写法;X5 L-P2-3;X3 相关行 |
| J8 | `apps/hft-core`、`apps/replayer`、`crates/hft-sim` | F1(g) 启动只读 `stop-marker.json`;F4(a)(b)(c)(`hftbatch.slice`、护栏因果判定);F3(b) 恢复尾部上界写明;X3 P0-4(hft-core 第 7 条为 `park_us` 唯一归属)、P0-5(规则在 hft-core,实现归 deploy 主表);replayer 的 `state_hash` 读者侧计算(F3(b));X3/X4/X5 相关行 |
| J9 | 冷进程 app(`reconciler`、`watchtower`、`kill-switch`、`journal-archiver`)、`hft-testkit`、`hft-telemetry`、`docs/testing.md`、`benchmarks/*` | F1(c) reconciler 置位条件、F1(g) kill-switch、F2 testing §8.2(分层、上下界、SW 锚退路、A/A)、F3(a) 基准 `gw-ingest/poll-cadence`;journal-archiver:X3 P0-9、10,X4 N-P1-8 `restore`;testing:AX-1 断言按 F1(c),Broken 计时按 F1(b);X3/X4 相关行 |
| J10 | `python/**`、`deploy/*`、`docs/development.md`、`docs/runbooks/*`、`docs/postmortem/README.md`、`.agents/notes/**` | 权限闭包(X3 P0-6…9、11)、F4(a)(d)、F5(development 唯一归属)、F7、X4 N-P1-2、4、5、6;IR R2(F1(g));ops_bot:X1 P2-12、13;pipeline:F1(e) 的提示;新 ADR(§6) |
| J11 | 根 `AGENTS.md`/`README.md`、`docs/{AGENTS,architecture,glossary,defensive-patterns,hot-path-rules,latency-budget}.md`、`crates/AGENTS.md`、`apps/AGENTS.md`、`docs/cookbook/adding-a-crate.md`、`crates/{hft-journal,hft-rt,hft-book,hft-py}` | F3(b) hft-journal 两级滚动;X3 P0-4(hft-rt 删公式改链接);F1(c) defensive-patterns 的 AUTO_EXTERNAL 说法;hft-py 冷读者按 F1(c);X3 相关行;篇幅仍须在上限内 |
| J12 | 冻结组件 README、`architecture-v2/_work/check_hft.py` | §4 新机检;冻结组件只修引用了已改名标识符(如 `hftbatch.slice`)的地方 |

(J3 本轮并入 J2。)

## 6. 新 ADR(J10 写;`.agents/notes/proposed/`)

`architecture/2026-10-03-resolve-broken-hard-only.md`(F1(a)(b))、`architecture/2026-10-03-auto-external-guard.md`(F1(c))、`architecture/2026-10-03-position-driven-subscription.md`(F1(e))、`process/2026-10-03-dev2-root-batch-slice.md`(F4)、`process/2026-10-03-ci-quota-self-estimate.md`(F5)、`process/2026-10-03-testnet-uid-unverifiable.md`(F6)、`process/2026-10-03-design-baseline-v1.md`(R8-12:冻结与实现驱动的变更流程;测试规格先行)。更新受影响旧 ADR 的状态行(`keyframe-one-shot`、`latency-acceptance-evidence`、`dev2-guard-system-slice-psi`、`manual-hold-and-stop-marker`、`ci-org-and-profiles`)。

## 7. 命名总表(本轮新增;跨文件一律用这些名字)

| 名字 | 种类 | 归属 |
|---|---|---|
| `resolve_broken`(只在 HARD 解除时) | 执行层判定 | hft-exec;调用点 hft-sequencer |
| `gate_naked_alarm_ms` 300000、`Note{ExecGateHeldNaked}`(P0) | 执行层参数/Note | hft-exec;Note 登记 hft-event |
| `auto_held`、`Note{LedgerAutoHeldOffset}`(P0) | 记账 | hft-ledger;Note 登记 hft-event |
| `exec.drain_ready()` | 执行层查询 | hft-exec;hft-sequencer 调用 |
| `/var/lib/hft/kill-switch/stop-marker.json.aborted` | 作废的停机标记 | kill-switch |
| `LatencySample.action`(0 New / 1 Cancel / 2 Amend) | 事件字段 | hft-event;样本合成 hft-oms |
| `MarketHdr.rx_empty_gap_ns` | 事件字段 | hft-event;产生 hft-venue |
| `gw.rest_idle_check_ms` 1000 | 配置 | hft-venue |
| `gw-ingest/poll-cadence` | 基准 | benchmarks |
| `keyframe_max_events` 1500000 | sequencer 参数(ECF1 段 5) | hft-sequencer |
| `hftbatch.slice`(根级,`CPUWeight=idle`) | systemd slice | deploy;数值与规则 hft-core |
| `Note{RecoveryHeldSim}`(P1) | Note | hft-risk;登记 hft-event |
| `uid_policy = "verified" \| "unverifiable"`(RF4:小写) | 账户配置 | hft-refdata |
| `quota_override`(workflow_dispatch 输入)、PR 标签 `hotfix` | CI | development |
| `LoadErrCode`(`#[repr(u16)]`) | 码表 | hft-refdata |
| `docs/changes.md` | 设计变更记录(冻结后) | 主控建立 |

## 8. 最终报告(简短)

1. 改了哪些文件。2. F1–F7 与 §3 中你负责的各项落在哪里(文件 + 小节)。3. 处理了 X1–X5 的哪些条目编号、留给别人的(写明文件与问题)。4. 本简报 §7 之外你新引入、别人需要用的名字(应很少)。5. 你在自己文件里发现、但没能确定怎么改的矛盾(≤ 5 条)。
