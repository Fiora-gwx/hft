# 第八轮第五次修订简报(M0-e;所有修订子代理先读)

依据:`round8-decisions.md` 的 **R8-11**(用户:"按推荐")与第四轮五路评审(`review-2026-10-03-M0d-W1…W5*.md`,含出处行号)及汇总 `review-2026-10-03-M0d五路汇总.md`。本简报是本次修订的**唯一权威命名与数值来源**;与旧规格、旧简报或评审原文冲突时以本简报为准。修订前副本:`E:\trading\_backup\round8e-pre\`。

## 0. 写作规则(不变,重申)

- 中文、标识符英文;一件事实一个归属地,别处链接;**不在归属地外重抄表格或数值**(本轮评审发现重抄的表已经分叉);未核实写"待核实(方法:…)"或指向开发备忘编号,"待标定""待实测"是合法变体;大改用 Write 整体重写或 Edit 精确替换;**只改自己清单里的文件**;写当前状态,不写历史;开发备忘只放待核实与开放问题。
- 不得丢章节:完成后运行 `PYTHONIOENCODING=utf-8 python E:\trading\architecture-v2\_work\heading_audit.py E:\trading\_backup\round8e-pre`,消失的标题只允许有意删除或改名。
- 可以只读运行 `PYTHONIOENCODING=utf-8 python E:\trading\architecture-v2\_work\check_hft.py`。

## 1. 用户决定的落地(R8-11,对应汇总 §4)

### E1 关键帧

删除"按节分片 + 作废重来":关键帧 `StateSnapshot{purpose=Keyframe}` 在 `Timer{Sys, Snapshot}` 到期后的第一个空闲轮**一次写完**(预算 ≤ 300 µs,记入 latency-budget §3.1 的 p99.9 项)。周期 `keyframe_period_ms` 默认 **300 000**(原 60 s);每个 Live 簿的 `L2Snapshot{Keyframe}` 均摊到同一周期。删除:hft-journal `JournalWriter::mark()/rollback()` 与 `WriteMark`/`AppendMark`;ECF1 段 5 的 `snapshot_slice_bytes`、`snapshot_max_defer_ms`;各 crate 的 `snapshot_section`(各组件只保留 `snapshot(&self, w)` 一次写本 crate 全部节;hft-book 为 `BookSet::snapshot(&self, w)` 写节 6、7,`Top.rx_tsc` 置 0);基准 `seq/snapshot_slice`、测试 `keyframe_slice`。可选扩展:由 advisor 镜像在旁路生成关键帧(触发条件:实测关键帧停顿超出 p99.9 记录目标)。

### E2 门槛 ⑩

1. **硬线 = 验收下限**:`tick_to_trade` p50 ≤ 30 µs、p99 ≤ 100 µs;新分段 `rep_to_order`(见 3)p50 ≤ 35 µs、p99 ≤ 110 µs(M3 末探针后可重定)。基准形态(`tick_to_trade` 22/75、`hedge_host` 与 `rep_to_order` 25/80)为**目标**;落在目标与硬线之间通过,差距与原因写验收记录并由 Admin 签字。p99.9 只记录。归属:testing §8.2(判定)、latency-budget §3(数字)。所有写"基准形态是硬门槛"的地方改为链接 testing。
2. **时钟校准**:housekeeping 每 **1 000 ms** 写一条 `ClockStatus`(Meta,含 `(tsc, phc, calib_err_ns)`);含 PHC 端点的分段按样本前后两条校准对**线性插值**换算,逐样本给误差界,误差界 > 1 µs 的样本计入 `unsynced_cal` 计数、不进分位数;HW 时间戳不可用时以 `sw_rx`(REALTIME)为锚。⑩ 报告附误差分布。
3. **新分段 `rep_to_order`** = 由某条回报(`OrderEvent` 帧:Ack/Fill/Cancel 等)触发的下一笔订单的 `sent_tsc` − 该回报帧的 `hw_rx`(计算同 `tick_to_trade`)。覆盖对冲路径(gw 解码 → rep 环 → sequencer → out 环 → gw 发送);`hedge_host` 是它的子集(只取 LegLink 第二腿,`link_role = Second`,不含 SecondRetry)。latency-probe 的 `quote` 模式 Ack 后同一事件内撤单即产生 `rep_to_order` 样本(主网行情负载下)。`LatencySample` 用触发事件 kind 区分,`link_role` 保留。
4. **按事件加权的离线统计**:从 seq journal 对**每条** `Bbo`/`L2Batch` 计算 `kernel_rx`、`ingest`、`feed_host`(记录头 tsc + `MarketHdr` 的 hw/sw/rx 时间戳),不增加热路径成本;实现为 hft-telemetry 的离线模块 `event_weighted()` 与命令 `replayer latency`(规格归 replayer)。⑩ 的 `ingest`/`kernel_rx` 读这份;探针样本只用于订单类分段。
5. **不再要求单独 ≥ 7 天 `feed_env = testnet` 窗口**:⑩ 只有一个 7 天主网行情窗口;`pair` 模式作小样本确认(M6 dev2 联调窗口或 M7 任意 ≥ 1 天的测试网行情时段,只记录)。删除上一轮加的"另取 ≥ 7 天"说法(testing、launch、hft-core、host-upgrade)。
6. **同机干扰**:开/关 sim 改为 **2 小时一块**、块内先开后关的顺序按记录在案的种子随机;按块配对;判定 = p99 比值(开/关)的 bootstrap 95% 置信区间**上界 ≤ 1.05**;每组两侧各 n ≥ 10,000 才作硬判定,不足的组只记录(含 `hedge_host`)。
7. latency-probe `pair_period_ms` 默认改 10 000(样本 ≈ 8,640/天)。

### E3 M1.5 扩大与 M3 末探针

- M1.5 增测(结论写 Agent Note):"压舱"深度连接(全量订阅、只读不解码)下的 `nic_to_kernel`;IRQ 核空闲 vs 保持唤醒 A/B;`calib_err_ns`;`net_poll_empty`;每包软中断成本;CNTVCT 频率;HW RX 时间戳与 PTP;ENA 队列数与 RSS。M1.5 前先做 AZ 探测(az-probe),租用实例开在选定 AZ。
- **M3 末满订阅探针**(新里程碑项,architecture-v2/11 归属):按需租 c8g.4xlarge,按 c16 线程计划、6 个产品线满订阅(BBO、成交、标记价、资金费,另加压舱深度)、打开 raw journal 与 persist、测试网探针单;**据此选定主机类(c16 / c32)并决定是否启用 IRQ 拆分(E4)**。M1.5 不再承担选档。

### E4 IRQ 拆分、轮询与 c16

- c32 线程计划**预留**核 28 为第二 IRQ 核与第二块交易网卡 **ENI-B2**(默认不启用):启用后,延迟敏感流(BBO、下单、用户流)留在 ENI-B(IRQ 核 1),批量流(深度、成交、标记价、REST)走 ENI-B2(IRQ 核 28);ENI-B2 只有行情与 REST,不进任何 key 白名单。启用条件:M3 末探针中 BBO 的 `nic_to_kernel` p99 在突发窗口显著上升(阈值由探针 Agent Note 定)。IRQ 核 1 加入 `rcu_nocbs`;hostprobe 检查 NVMe 等 managed IRQ 的 effective_affinity 不落在 IRQ 专用核,TASK_ON_HOT 检查扩展到 IRQ 核。
- **T7 改**:承载 ≥ 3 个 socket 的 `Spin` 行情线程每圈可调用一次 `epoll_pwait(epfd, …, timeout = 0)`,只对就绪的 socket 调 `recvmsg`;系统调用白名单同步。BBO 路由每连接每次 poll 至多 `feed.bbo_frames_per_poll`(默认 **8**)帧,连接之间按帧轮转;非 Bbo 路由仍为 `feed.frames_per_poll`。`kernel_rx` 预算按主机类分写(c32 为 §3 数字;c16 待 M3 末探针)。
- **短读即停**:TCP 非阻塞读返回短读(读到的字节 < 缓冲)即认为队列已空,先解帧,不再补一次空 `recvmsg`。
- **c16**:只在 M3 末探针达标且启用 `epoll_pwait(0)` 时保留为候选;默认 c32。

### E5 首次上线的行情范围

- core 行情 = BBO、逐笔成交、标记价/指数价、资金费,以及 BBO 补帧(REST)。**全量增量深度与 REST 快照重同步改为可选扩展**(触发条件:某个策略需要全深度);规格保留但标为可选。
- 需要深度时优先用**分档快照流**(Binance `depth{5,10,20}@100ms`、OKX `books5`、Bitget `books5/books15`,各所待核实):新路由 `RouteKind::DepthTopN`,产出 `L2Snapshot{origin = TopN}`,簿按"整替换"应用(`apply_snapshot`),无序号链,无 REST 重同步。
- snap-fetch 只保留 BBO 补帧(增量深度启用时才承担深度快照);`feed-md:` 任务承载成交/标记价/资金费/TopN。portfolio-carry 快速部分不订阅深度;执行层切片的可见深度有 TopN 订阅时读簿,否则用 BBO 顶档。

### E6 Model 检查点的路径状态节

- SDK:检查点分两节——**原始样本节**(布局由 `ModelLayoutDesc` + `LAYOUT_VERSION` 管)与**路径状态节**(路径依赖的派生状态:分位带及其时刻、平滑窗、上次发送的载荷摘要等;自带 `Model::PATH_VERSION: u32`;接口 `Model::snapshot_path(&self, w)`、`Model::restore_path(&mut self, r) -> Result<(), RestoreError>`,路径节作为检查点第一块写出)。恢复时 `PATH_VERSION` 不符 → 只丢路径节,派生状态按原始样本重算,写 `Note{ModelPathReset}`(短暂不一致可接受)。
- 有路径节时,"检查点 → 恢复 → 续算"与连续运行逐字节相同(SDK 确定性夹具、每日 recompute 照常要求逐字节一致);路径节被丢弃的当日 recompute 从下一个分位带边界 + 平滑窗长度之后开始比较。
- portfolio-carry:分位带更新时刻对齐 UTC(`band_update_ms` 的整数倍边界)。

### E7 人工与兜底操作

- **`manual_hold`**(hft-exec cell 标志):收到 `ExternalFill{Manual}` 置位;置位期间抑制 HedgeFix(补腿)、ResidualTrim、外来成交默认处理与自动有界平仓的计时(不计次、不计截止);解除只认:策略对该 group 的非清除类动作(`set_target` 或 `place`;`clear_target` 不算)、人工 `HedgeFix`、引擎目标(L3/flatten/Hard)。写 `Note{ExecManualHold}` P1。
- **`stop_core` 的持久标记**:kill-switch 在 `systemctl stop` 之前写 `/var/lib/hft/kill-switch/stop-marker.json`(`{req_id, scope, utc_ns}`,fsync);hft-core 启动步 5 读到标记 → 写一条 Input `ControlCmd{Kill{scope, 非 HARD, 不带 flatten}}`(issuer `cli:systemd`,权限表为该主体增加"仅启动时由停机标记触发的 Kill")→ 标记改名 `.consumed`;Kill cap 由交易员 `/resume kill` 解除(既有规则)。incident-response R 节与 hft-core 放行规则同步改正。
- **`stop_core` 的额外确认**:任一账户 MarginStress ≥ L2、有 Broken 或在途链接时,ops_bot 的兜底只撤单 + P0;停引擎需要第二次确认(`/stop … core`)。
- **`Kill{flatten}`** 在启动闸门未放行与 DRAINING 期间也允许(仍需确认);control-plane 写成"升级类的例外"。

### E8 Admin 冒烟单命令

- 新控制命令 **`SmokeOrder{account, venue, inst, side, ttl_ms}`**:只允许 Admin(`cli:<login>`,或 ops_bot 私聊中的 Admin),需确认;OMS 以 `origin = Command`、新 `OrderFlags::SMOKE` 位建槽;只能是 PostOnly,价格离对手价 ≥ `smoke_far_bp`(默认 300 bp,并在交易所价格带内),数量 = 该 inst 最小名义 × 1.1 向上取整到 lot;`ttl_ms` 默认 10 000、上限 60 000,到期由 OMS 自动撤单;每 (account, venue) 同时至多一张;Shadow/CloseOnly 下允许,HARD 下拒绝;走完整风控链(不经模式门);不计入任何策略、不改目标。码值与结果码归 control-plane/hft-event/hft-oms(J6、J4 登记)。
- launch D-smoke 与每月正式账户冒烟改用此命令(死手刷新因此覆盖该 inst,可验证);删除"网页手工挂单"与"Recon 降 P1"的说法。

### E9 门槛 ⑦

- C 阶段前置 = **dev2 缩小规模的 ⑦ 通过**;切 Live 前(launch D)在升级主机的 sim 实例上**满规模再连续运行 ≥ 7 天**(在 ⑩ 测量窗口之外)并以最终参数重跑压力用例。
- **决策不变的修复**:对最近 7 天的 journal `replayer verify` 逐字节一致,且 diff 只涉及 `venue-*`、`hft-venue`、`hft-net`、冷进程、`deploy/` → 不使 ⑦ 重新计时;D-shadow 重计 3 天。
- ⑦ 重新计时范围加入 hft-oms 与 hft-strategy-sdk;小改白名单改为 schema 字段属性 `FieldDesc.minor_change: MinorChange { None, RelPct(u16), Abs(i64), Any }`(工具通用判定);门槛文字改为"每个上线策略"。
- 护栏停掉 sim 后:资源信号回落到阈值以下 ≥ 30 min,守护**自动重启 `hft-core@sim`**,每 UTC 日至多一次;从不启动 prod 与批处理;polkit 只为 `hft-core@sim.service` 增加 `start`。"只停不启"改为"只自动重启 sim,每天至多一次"。
- 回放校验补跑:`hft-replay-verify` 每次运行补跑所有缺少报告的 UTC 日(不只前一日);窗口期的迟到报告视同当日;replay-verify 运行时 archiver 与 pipeline 暂停(`ExecCondition`)。

### E10 dev2 批处理与护栏

- dev2 的 `hft-batch.slice` **去掉 `CPUQuota`**,改 `CPUWeight=1`;全部批处理单元 `CPUSchedulingPolicy=idle`、`Nice=19`(`IOSchedulingClass` 在调度器 `none` 下无效,改用 E14 的带宽上限)。
- 护栏的 CPU/内存压力停机信号改读**旧系统所在的 `system.slice`** 的 `cpu.pressure`/`memory.pressure`,加上 `MemAvailable` 与 swap-in 速率;整机 PSI 只告警。
- 护栏一级/二级动作时写 `/run/hft/guard/tripped`;全部批处理单元的 `ExecCondition=` 先查它(存在即拒绝启动),人工清除才解除(定时器因此不能绕过)。
- M3 标定加演练:批处理满载 10 min,护栏不动作、`system.slice` PSI 不升(`[需授权]`,生产主机)。演练失败 → 提前启用 dev2 退路(另开 c7g.large 专跑验证包与回放)。

### E11 CI 额度

- 每夜作业开始时与开发构建 dispatch 时读取本月用量:≥ **1,500 min** → 每夜任务与开发构建自动跳过(写作业摘要与告警),余额只给正式版与热修。
- 新工作流 `release-x86-cross.yml`:在 `ubuntu-latest` 上交叉编译 aarch64 发布归档的应急路径,每季度演练一次。
- 发布作业 `timeout-minutes: 90`、验证包 60;PR 的基准冒烟用 `ci` 剖面(不用 `bench`/`release-hot`);`scaffold-gate` 复用主 target 目录;`ci` 剖面显式 `panic = "unwind"`(`release`/`release-hot` 仍 abort);`nightly-arm` 与 arm 发布作业共用缓存键以保温。

### E12 累计门槛 ③④ 的口径

- ③ 按**子进程 CPU 时间**计:每个 core 解码器目标 ≥ **2 CPU·h**;单输入超时按 harness 内的线程 CPU 时间 > 10 ms 判定。`ASAN_OPTIONS=quarantine_size_mb=16`,另设 `-malloc_limit_mb`;受限主机上的 OOM/超时先记**疑似**,由 CI 以默认限额对最小化输入复现,复现才按失败清零;`hft-dst` 与 fuzz 子进程的 SIGABRT/SIGSEGV 一律失败(SIGKILL 仍为中止);`hft-dst` 每个种子开始前写"进行中种子"标记。
- **跨版本累计**:验证包为每个目标组记录 `path_digest`(构建时对相关源路径求哈希,写进 `VERIFY.json`;③ 的路径 = 对应 `venue-*`、`hft-venue-proto`、`hft-types` 十进制解析;④ 的路径 = 引擎 crate 与 `hft-testkit`);`hft-verify ledger show --candidate <ver>` 只累计 `path_digest` 与候选版本相同的运行。候选版本补跑规则保留(每目标 ≥ 15 CPU·min、DST ≥ 20,000 种子)。

### E13 引擎兜底

- (a) 设 `reason ∈ {MarginStress, Hard}` 的引擎目标时,先经 OMS 新入口 `cancel_resting_on(a, inst, out)` 撤该 inst 上本账户全部非 EXEC 挂单;撤单确认前切片量按 `|pos| − 即时在途` 截取;风控 #7 对 Flatten/Hedge 类只扣即时在途。DST:"L3 时策略挂满 reduce-only 平仓单,仍按步减"。
- (b) 自动有界平仓的"一次" = **一次可执行的尝试**:开始前等待(不算失败)多出那腿报价可用、该 venue 不在限速退避、对手价未越过 cap;开始后门控拒绝与零成交按 `max_nofill`/`gated_backoff_ms` 重试;只有交易所拒单、MinOrder、SignFlip、Residual、可执行时长超过 `fix_deadline_ms` 判失败。
- (c) L3 越价上限按 **inst 所属价格带类别**取值(SDK `Views::risk_for(a, i)` 或 `RiskStateView` 提供按 inst 的 `rr_max_cross`,取消"账户内最小值");另设可选 `RiskParams.l3_max_cross`(默认 0 = 不启用,上线签字时决定)。
- (d) 用户流缺口里漏掉的 ADL:reconciler 第二轮仍见到"永续同号缩小、无在途、无 Unknown"的差异 → 该条目带标志 `AUTO_EXTERNAL`,sequencer 据此写 `ExternalFill{cause = Unknown}`(Input,可回放)并 P0;现货差异与反向差异仍走人工。
- (e) HedgeFailure 死锁:hft-core 装配校验 `dust_notional ≥ 交易腿最小名义的最大值`(不符退出 2,`DustBelowMinNotional`);新升级命令 **`WriteOffResidual{account, group}`**(Admin + 确认):把该 group 当前残余登记为核销(ledger 记 `AdjustDelta::WriteOff`,LegTracker 的 `residual` 清零),Broken 随之可解除。
- 其余 W1 P1/P2 照评审修法(M-3 RESUME_HOLD 时直接 SerialFallback、M-4 恢复期 `gate_open = false` 抑制补腿并在放行时重设截止、SerialFallback 触发条件改为"第二腿 `quote_ok ∧ venue_ready ∧ 下单连接 Up`"、L3 回落时串行 cell 改设 `Hard` 对齐目标、L3 每步取"当前目标与按比例目标中更接近 0 者"、Broken 解除只清策略目标、同一 Input 先处理 EXEC 槽回报再 `on_risk`、零敞口链接续接超时只 P1、杠杆取整统一**向下**、交易中永续 inst 杠杆未知 → `ACCOUNT_CFG` 不放行、ForeignFill 带 `trade_id`/`cum_filled`、资金费时刻归一化函数共用、SimReset 后逐 group `on_legs` 等)。

### E14 策略侧与运维小项

- (a) portfolio-carry:超配 > 2 × `overweight_tol` 或 `open_allowed = false` 持续 `close_only_rounds`(默认 3)轮后,按 `stress_step_ms` 每步减 `stress_reduce_frac`,不受成本门约束(pc 内部规则)。
- (b) Live 实例上改动任何 `FieldDesc.structural` 字段的 `SetParams` → 升级类(需确认);预览显示受影响 group 数与预计不能开仓的时长;pipeline 插件不得拥有结构字段(pipeline 拒绝);预热回看窗口从当前生效的策略参数读取,不在 policy 里重复。
- (c) 见 E9 的 `minor_change`。
- (d) SDK 只追加:Model 可订阅本实例的 `Exec`/`Order`/`Manual` 事件(只读、来自 journal、确定);hft-sim 增加 `StressScope::Venue(v)`(按 venue 单独变价);latency-probe 增加 `target` 模式(走 TargetIntent → hft-exec 切片路径)。
- (e) dev2 sim 容量按"2 个研究策略 × 2 个池实例 × 32 group"重定(`max_cells_total`、advisor 预算、`max_instances`),portfolio-carry 写明 sim 实例默认 ≤ 32 group。
- (f) 主机清单:网卡 MAC **只在运行期核对,不进发布哈希**;预建第二 AZ 的待命清单(预建 ENI-B/ENI-C,每个正式版带它的 `host_expect`);incident-response S 节加"改看门狗目标"一步(看门狗目标为主机本地配置文件)。
- (g) 看门狗维护静默:`/healthz` 返回 `maintenance_until`(本机命令 `hft-maint start <分钟>`,≤ 30 min);静默期间看门狗只发 Telegram、不打电话。若 az-probe 证实 Binance 所在 AZ 与 dev2 相同,另开 t4g.nano(约 $3–4/月,`[需授权]`)在第三 AZ 跑看门狗。
- (h) GroupId = `(StrategyId << 24) | 实例内计数器`(计数器 1..=0xFF_FFFE;StrategyId ≤ 254),唯一性由构造保证;pipeline 与 hft-cfg 同规则。

## 2. 第四轮评审 P0 的规范解法

| 编号 | 解法 | 归属 |
|---|---|---|
| A-P0-1 | 见 E1 | J5、J11、J9、J4、J6 |
| A-P0-2 | 见 E10 | J8、J10、J9 |
| C-P0-1 | ops_bot 系统用户统一为 **`hft-opsbot`**(deploy 用户表为准);hft-ctl、hft-ctl-proto、pipeline 改正;pipeline 要的写权限改用 deploy 定义的组 | J6、J10 |
| C-P0-2 | 账户名字段 = `AccountMeta.label`;`sim-` 前缀查 `label`;tools 示例改 `sim-…`;hft-cfg 加 `AccountNamePrefix` 校验;launch:83 改 | J6、J8、J10 |
| C-P0-3 | `raw-feed-<v>-bbo` 计入升级主机内存表与大页(每 venue 两份),重算 c8/c16/c32 prod;大页推导从 journal 列表求和,`gen-host --check` 与内存表对账 | J8、J10 |
| C-P0-4 | 删 hft-core `rest_pool.feed`,以 hft-venue `snap.parallel` 为准 | J8 |
| C-P0-5 | 死手枚举与单位以 hft-refdata(ms、`CountdownPerInst…Schedule`)为准 | J7 |
| C-P0-6 | `cargo xtask dst` 签名归 tools:`--seeds N [--steps M] [--seed S] [--suite X] [--regressions] [--trace] [--strategy K]`;删除 `--nightly`(累计在服务器) | J6、J9 |
| C-P0-7 | 没有 `/opt/hft/current`:按组 `/opt/hft/current-<group>`(`current-core`、`current-sim`、`current-cold`…),全部引用改本组链接;`paths.release_manifest` 默认 `/opt/hft/current-core/RELEASE.json`(sim 实例 `current-sim`) | J10、J8、J6、J9 |
| C-P0-8 | 消费者凭证:archiver 覆盖 `hft/` 与 `hft-testnet/` 前缀;S3 对象键加主机段 `<prefix>/<host>/<instance>/…`;pipeline 参数版本库前缀同样带主机段;launch D-23 加"同前缀下无 quarantine" | J10、J9 |
| C-P0-9 | portfolio-carry 资金费环按 256 重算 `snapshot_bytes`/`mem_bytes`,标志取 summary 的 `TRUNCATED` | J3 |

## 3. 其余修正(全部按评审修法)

- W1 P1-1…6 → E7、E13;W1 P2-1…21 按 W1 §3。
- W2 P1-1(Exit 重发改 `tag` 低位计数或先 clear 再 target;hft-exec 写明"重置失败计数必须改字段")、P1-2(有仓位 = 一腿 > 尘埃且 ≥ 最小下单量;MinOrder 残余标志)、P1-3(GroupState 持久"待完成退出")、P1-4 → E6、P1-5 → E14(h)、P1-6 → C-P0-9;W2 P2 全部。
- W4 P1-1 → E10;P1-2 → E12;P1-3 → C-P0-3;P1-4 → E2-6;P1-5:用 `IOReadBandwidthMax=`/`IOWriteBandwidthMax=`,清单写 gp3 吞吐与 IOPS,M7 测并发下 persist 滞后;P1-6 → C-P0-8;P1-7:主机清单开关决定装不装 `hft-verify.*`(`verify = true|false`),preflight 查 `LoadState`,`hft-window` 改为"停定时器 + 窗口标志文件 `/run/hft/window/active`,批处理 `ExecCondition` 查它",不再用 `mask`;P1-8 → E14(f);P1-9 → E8;P1-10 → E9;P1-11 → E11;W4 P2 全部。
- W5 N-P0-1 → E1;N-P1-1 → E4;N-P1-2 → E4;N-P1-3:sequencer 每处理完一条 BBO 或 advice 也查看 ctl 与实盘 rep 环头;N-P1-4 → E2-2;N-P1-5 → E2;N-P1-6 → E3;W5 P2:短读即停(E4)、gw 的账户查询解码移到 warm 线程(启用 hft-venue 已有的可选扩展"warm 线程 + `acct-<venue>` 环",改为 core)、md/bbo 循环中检查定时器到期并让位、环头检查预算按环数写、warm 线程 `park_us` 升级主机上 200–500、XPS 按热核分配、THP 写法统一为 Graviton `madvise`(GRUB 不写 `transparent_hugepage=never`)、`engine_ns` mult/shift 写明 Δtsc 取值范围、ReconcileReport 整组处理补预算或分批。
- W3 §3 全表(P1 60 条、P2 76 条)按"修法"列;冻结组件不改。另修 W3 指出的 brief §10-11 错误(uid 应为 `hft-opsbot`)。

## 4. 新机检(J12,W3 §4 的 12 项)

身份与路径注册表(deploy 单元表抽 User/Group、`/opt/hft/…`、`/run/hft/…`、`/var/lib/hft/…`、S3 前缀,全仓引用必须在表内,消费者凭证前缀覆盖每个 archiver 前缀);配置键注册表(同一键只一处给默认值,TOML 前缀存在);CLI 签名比对(tools 中 `cargo xtask`、`hft-verify`、`hft-dst`、`replayer` 的 flags);环与 journal 名单闭包(sequencer/hft-venue/hft-event 环表的每个名字在 hft-core 环容量表与内存表里有一行);严格隐式填充(全部 `#[repr(C)]`,填充 > 0 报错);归属地外重抄大小比对;枚举判别值(repr(u8) 或写了"逐值相同"的枚举必须显式值);Note 源与产出 crate 一致;基准名唯一且同名预算相同;"规则归/唯一归属"链接目标含前文标识符(警告级);待核实格式;brief §8 命名表落地(新名字在声明的归属文件中出现)。误报多的做成警告级。

## 5. 工作分配(只改自己清单里的文件;与 M0-d 相同)

| 代号 | 文件 | 重点 |
|---|---|---|
| J1 | `architecture-v2/README.md`、`00`–`13`、`现状测量…`、`待测量…` | E2(00 记录硬线与目标)、E3(11:M1.5 增项、M3 末探针、AZ 探测前移)、E4、E5、E9、E10 退路触发、E11、E14(f)(g);W3 P2-65;12 风险更新;13 追加 §18 |
| J2 | SDK、strat-lib、`strategies/AGENTS.md`、`strategies/registry/**`、`strategies/{cdex,cross-taker,maker-taker,latency-probe}/**`、`docs/cookbook/adding-a-strategy.md` | E2-3/7(latency-probe `rep_to_order`、`pair_period_ms`、`target` 模式)、E6(SDK 接口)、E13(c)、E14(b)(d)(h) 的 SDK 侧、`FieldDesc.minor_change`;W2 P1-1 的 SDK 说明;W3 SDK 相关行(P1-8、19、20、23–26、29、33、41、42,P2-21、24–27、55、56、76) |
| J3 | `strategies/portfolio-carry/**` | C-P0-9、E6(路径节、UTC 对齐)、E14(a)(e)、W2 P1-1/2/3、W2 P2、`minor_change` 取代文字白名单 |
| J4 | `crates/hft-exec`、`crates/hft-oms` | E7 `manual_hold`、E8(OMS 槽与自动撤单)、E13 全部、W1 P1/P2 中 exec/oms 部分、W2 P1-1 执行层说明、E1(删 `snapshot_section`)、W3 相关行(P1-9–15、28、30、31) |
| J5 | `crates/hft-risk`、`crates/hft-ledger`、`crates/hft-sequencer` | E1(sequencer 快照)、W5 N-P1-3、定时器中途检查、E13(a)(c)(d)(e) 风控/记账/sequencer 侧、E7 启动 Kill 的 sequencer 侧、W1 相关、W3 相关行(P1-12、16、18、21、22、32,P2-2、22、38、44、60) |
| J6 | L0、控制面、`docs/subsystems/*`、两篇 cookbook、`tools/README.md` | 新码登记:`SmokeOrder`、`WriteOffResidual`、`OrderFlags::SMOKE`、`AUTO_EXTERNAL`、`Note{ExecManualHold, ModelPathReset}`、`rep_to_order` 样本区分、`ClockStatus` 1 s、`AdjustDelta::WriteOff`;E7 权限(`cli:systemd` 启动 Kill、ops_bot 用户名)、Kill{flatten} 例外;E12 `path_digest`/`hft-verify` 改动;C-P0-1、2、6;GroupId 构造(E14 h);`RouteKind::DepthTopN` 与 `L2Snapshot{origin = TopN}` 码;W3 相关行 |
| J7 | `hft-venue-proto`、`hft-venue`、`hft-net`、`venue-binance`、`venue-okx`、`venue-bitget`、`adding-a-venue`、`fixtures/README.md` | E4(`epoll_pwait(0)`、`bbo_frames_per_poll`、短读即停)、E5(TopN 路由、增量深度改可选、snap-fetch 只 BBO 补帧)、W5 P2(gw 账户解码移 warm `acct-<venue>`)、C-P0-5、W3 相关行(P1-35–39、43、46,P2-17、30、34、35、49、58) |
| J8 | `apps/hft-core`、`apps/replayer`、`crates/hft-sim` | C-P0-2、3、4;E1、E3(M3 末探针线程计划引用)、E4(c32 预留核 28 与 ENI-B2、c16 条件、warm `park_us`)、E5(内存表按 core 行情范围重算)、E7(启动读停机标记)、E9、E10(dev2 slice 与护栏信号的数值)、E13(e) 装配校验、E14(e)(f);replayer `latency` 命令(E2-4);hft-sim `StressScope::Venue`;W3 相关行 |
| J9 | 冷进程 app、`hft-testkit`、`hft-telemetry`、`docs/testing.md`、`benchmarks/*` | E2 全部(testing §8.2、telemetry `event_weighted()`、`rep_to_order`、bootstrap、2 h 块随机)、E7(kill-switch 停机标记)、E9(⑦ 规则、回放补跑)、E12(testing §6.1)、E13(d) reconciler、E14(g) watchtower 维护静默;删 `keyframe_slice`/`seq/snapshot_slice`;W3/W4 相关行 |
| J10 | `python/**`、`deploy/*`、`docs/development.md`、`docs/runbooks/*`、`docs/postmortem/README.md`、`.agents/notes/**` | E7(ops_bot 二次确认、IR R 节)、E8(launch D-smoke)、E9(护栏自动重启 sim、polkit)、E10、E11、E12(验证包工作流与 `path_digest`)、E14(f)(g);C-P0-1、7、8;W4 P1-5/7 与 P2;新 ADR(见 §6) |
| J11 | 根 `AGENTS.md`/`README.md`、`docs/{AGENTS,architecture,glossary,defensive-patterns,hot-path-rules,latency-budget}.md`、`crates/AGENTS.md`、`apps/AGENTS.md`、`adding-a-crate.md`、`crates/{hft-journal,hft-rt,hft-book,hft-py}` | E1(hft-journal 删 mark/rollback;hft-book `BookSet::snapshot`)、E2(latency-budget 硬线/目标、`rep_to_order`、时钟插值、§3.1)、E4(T7、T12、T13、`kernel_rx` 按主机类)、E5(latency-budget 深度 ingest 标可选)、D6 白名单补节 16 `ClockCalRow.tsc` 与节 7 `Top.rx_tsc`(W3 13a)、defensive-patterns 页首定义(W3 P2-74);篇幅仍须在上限内 |
| J12 | 冻结组件 README、`architecture-v2/_work/check_hft.py` | §4 新机检;冻结说明按需补 |

## 6. 新 ADR(J10 写)

`architecture/2026-10-03-keyframe-one-shot.md`、`process/2026-10-03-latency-acceptance-evidence.md`(E2)、`architecture/2026-10-03-m3-end-probe.md`(E3)、`architecture/2026-10-03-irq-split-reserve.md`(E4)、`architecture/2026-10-03-core-market-data-scope.md`(E5)、`architecture/2026-10-03-model-path-section.md`(E6)、`architecture/2026-10-03-manual-hold-and-stop-marker.md`(E7)、`process/2026-10-03-smoke-order-command.md`(E8)、`process/2026-10-03-dev2-guard-system-slice-psi.md`(E10);更新受影响旧 ADR 的状态行(`sequencer-md-preempt`、`shadow-before-live`、`verify-on-server`、`latency-probe-milestone`、`host-class-c32`、`feed-split-and-snap-fetch`、`ci-org-and-profiles`)。

## 7. 命名总表(新增;跨文件一律用这些名字)

| 名字 | 种类 | 归属 |
|---|---|---|
| `keyframe_period_ms` 300000 | sequencer 参数(ECF1 段 5) | hft-sequencer |
| `rep_to_order` | 分段 | latency-budget |
| `unsynced_cal` | 计数 | latency-budget / hft-telemetry |
| `event_weighted()`、`replayer latency` | 离线统计 | hft-telemetry、replayer |
| M3 末满订阅探针 | 里程碑项 | architecture-v2/11 |
| ENI-B2、IRQ 核 28(c32 预留) | 网卡/核 | hft-core 线程计划;deploy 清单 `[eni_b2]` |
| `feed.bbo_frames_per_poll` 8 | 配置 | hft-venue |
| `RouteKind::DepthTopN`、`L2Snapshot{origin = TopN}` | 路由/事件 | hft-venue-proto、hft-event |
| `acct-<venue>` 环、warm 任务 `acct-fetch` | 环/任务 | hft-venue;线程计划 hft-core |
| `Model::PATH_VERSION`、`snapshot_path`、`restore_path`、`Note{ModelPathReset}` | SDK | SDK;Note 登记 hft-event |
| `manual_hold`、`Note{ExecManualHold}` | 执行层 | hft-exec |
| `/var/lib/hft/kill-switch/stop-marker.json` | 停机标记 | kill-switch;hft-core 读 |
| `SmokeOrder{account, venue, inst, side, ttl_ms}`、`OrderFlags::SMOKE`、`smoke_far_bp` 300 | 命令 | control-plane、hft-event、hft-oms |
| `WriteOffResidual{account, group}`、`AdjustDelta::WriteOff` | 命令/记账 | control-plane、hft-ledger |
| `AUTO_EXTERNAL` | ReconcileReport 条目标志 | hft-event(位)、reconciler(语义) |
| `DustBelowMinNotional` | 装配退出 2 | hft-core |
| `cancel_resting_on(a, inst, out)` | OMS 入口 | hft-oms |
| `RiskParams.l3_max_cross`(默认 0) | 风控参数 | hft-risk |
| `FieldDesc.minor_change`、`MinorChange` | SDK | SDK |
| `close_only_rounds` 3 | pc 参数 | portfolio-carry |
| `StressScope::Venue(v)` | hft-sim | hft-sim |
| latency-probe `target` 模式、`pair_period_ms` 10000 | 策略 | latency-probe |
| `/var/lib/hft/guard/tripped`(合并裁决 R6)、`/run/hft/window/active` | 标志文件 | hft-core(规则)、deploy |
| 主机清单 `verify = true\|false`、`[eni_b2]` | 清单字段 | deploy |
| `release-x86-cross.yml` | 工作流 | development |
| `path_digest` | 验证包/运行记录字段 | tools(hft-verify)、deploy(VERIFY.json) |
| `hft-maint start <分钟>`、`maintenance_until` | 维护静默 | watchtower、deploy |
| `hft-opsbot` | ops_bot 系统用户 | deploy |
| `/opt/hft/current-<group>` | 安装路径 | deploy |

## 8. 最终报告(简短)

1. 改了哪些文件。2. E1–E14、§2、§3 中你负责的各项落在哪里(文件 + 小节)。3. W1–W5 中你处理的条目编号、留给别人的。4. 本简报 §7 之外你新引入、别人需要用的名字(应很少)。5. 仍有风险的地方(≤ 5 条)。
