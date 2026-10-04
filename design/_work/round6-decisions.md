# 第六轮裁决:全局评审(耦合/边界 26 条 + 场景潜在 bug 35 条)

优先于既有规格。编号:C = 耦合评审,B = 场景评审。

## A. 结构与范围

- **A1 构建目标(C1)**:阶段 1–3 唯一发布目标 `aarch64-unknown-linux-gnu` + `target-cpu=neoverse-v1`(dev2 的 c7g 与 c8 的 c8g 公共基线,同一二进制);x86 只作为阶段 4(metal)的独立剖面。唯一归属 `hft/docs/development.md#目标-cpu`,其他处链接。hft-py 出 aarch64 wheel;CI 增加 arm64 runner;hft-rt 以 aarch64 为主路径,主机检查项按 ISA 分表。换架构 = 版本变化(按 B3 规则冷启动)。
- **A2 阶段 1 依赖闭合(C2、C13)**:
  - 新增阶段 1 工具 `hft-cfg`(Rust CLI,`tools/`):手写 TOML → refdata / StrategyParams / RiskParams / EngineConfig 四类 blob + 差异报告,复用各 crate 的 Rust 构建器;删除"Python dtype 写 blob"的路径,RDB1 只有一个编码器。
  - 阶段 1 参数通道 = `hft-cfg` 生成 blob + `cli:<login>`(Trader + 二次确认)经 ctl 发 `SetParams`/`SetRisk`;pipeline(新)推迟到阶段 3(随 cdex 迁入)。这修正第五轮 M8"阶段 1 不开放人工 SetParams":开放给 CLI + 确认,不开放给 Telegram。
  - reconciler 最小版(两所仓位/挂单/余额)进阶段 1 第 8–9 周。
  - hft-testkit 内置最小确定性撮合器,支撑 DST 与阶段 1 测试;完整 hft-sim 阶段 2。
  - hft-py 按 feature 拆分,阶段 1 只要只读 reader(不依赖 sequencer/sim)。
  - mock-exchange 的 wire 级实现推迟到 c8 延迟验收;阶段 1 用交易所测试网 + testkit 的 VenueProtocol 级假交易所。
  - **阶段归属唯一来源 = README frontmatter `phase`**;`doc-check` 生成/比对路线图表。
- **A3 cdex 免费档先在 Python 里做(C10)**:阶段 1' 不建 Rust chain-core。工程师 B 在现有 Python cdex 内落地新框架的四条安全规则 + SwapGuard:① 传输错误只进 Unknown(状态只由链上证据改变);② 本地 nonce 分配器(在途表、只在包含/作废后推进);③ 每笔交易经 SwapGuard 合约(`maxBlock = 发送块号 + N`、minOut);④ ≥ 2 个**独立运营商**回执一致才算成交(运营商按封禁/限流域划分,见 B-14);另加结构化追加日志(JSONL,字段与 TxLifecycle 对齐)以便日后导入。规格新建 `hft/python/cdex_hardening/README.md`(phase 1')。
  - Rust chain-core / hft-chain-evm 推迟到阶段 3(随 cdex 策略迁入 hft-core),届时按 **cold tier** 实现 free profile(允许 tokio、成熟 RPC 客户端、离线 journal 写者);builder profile 立项时再评估热化。
  - "发送后端"改称 **profile**,列出 free 与 builder 不同的模块(预检、签名、gas 记账、块头来源、证据来源、Sent 语义);TxTracker 从第一天按"nonce 槽 → 候选交易集合"设计。
  - external-venue-protocol §9 的临时接口不再需要(Python cdex 直接内置规则),标为删除;ext 协议整体阶段 3。
- **A4 模式/cap 收拢(C4、C5)**:唯一归属模块 `hft-risk::mode`:ModeTable、cap 集合、effective、启动闸门、`ctl_state_version`、kill 投影;sequencer 只调用 `apply()` 并分发 Mode 事件与撤单。`RunMode` 编码与枚举下沉 hft-types。control-plane 的优先级表标注"由代码生成"(阶段 1 手写,CI 比对)。
  - ctl-gateway 不复制 sequencer 状态:SetMode 按**目标值**分类(→Live 升级;→CloseOnly/Shadow 降级);放宽判定、确认令牌与 `ctl_state_version` 校验移到 sequencer(`CmdHdr.confirmed` 由 sequencer 复核,返回 `NeedsConfirm`/`StateChanged`);Prepare = 带 PREVIEW 标志的 ControlCmd,sequencer 回 `CmdResult{preview}`。gateway 只做传输、鉴权、角色上限。
- **A5 RISK_REDUCING 判定(C3)**:hft-risk 提供纯函数 `classify(intent, origin, pos_view) -> {class, risk_reducing, reduce_only}`,sequencer 在 evaluate 之前调用,OMS 只把结果拷进 `oflags`;删除 OMS 中的分类表(改链接)与"chain-out 在 kill 下只放行 side=Sell"。
- **A6 依赖方向(C9、C15)**:Risk 只输出每个 gw 的 `KillBits`,由 LiveDriver 写 `KillWord` 原子字(删除 hft-risk→hft-rt 例外);`CtlRecord` 改 `encode_into(&mut [u8])`;hft-core/chain-core 不依赖 hft-telemetry;`RejectBits` 下沉 hft-types;hft-sequencer 的 `hypo`(HypoDriver)移到 replayer/hft-py;ext/relay 帧移到阶段 3 的 `hft-ext-proto`,hft-event 只预留链上 kind 编号段;hft-ctl 拆 `hft-ctl-proto`(帧、权限表),权限表从代码生成 README。
- **A7 EngineConfig(C7)**:refdata 只保留宇宙数据;oms/ledger/容量/批量上限/环容量拆为 `EngineConfig` blob(按主机类由 hft-core 配置给出,`ConfigBlob{Engine}` 记录);`risk_class` 并入 RiskParams。
- **A8 单一归属表(C8)**:账户独占/共享 → hft-core `[[accounts]].exclusive`(RiskParams 与 kill-switch 读它,不另配);账户列表与标签 → refdata `AccountMeta`;限速 → refdata `RateLimitTable`(余量比例在 RiskParams 一处);死手能力 → refdata `VenueMeta.dms`(hft-core 只配开关);每事件出环上限 K → EngineConfig;连接轮换 → refdata `ws_max_conn_age_s` + EngineConfig 余量;dev2 段大小与 hot_keep → hft-journal(hft-core 预算表从它推导);环容量 → EngineConfig;停摆判定窗口 → hft-core `watchdog.hb_stale_ms`(kill-switch 读同一配置)。
- **A9 warm 任务形态(C6、B-P1-12)**:ctl-gateway 与 advisor 改成 `poll_once`(gateway 用 `wait_fds` 交给 epoll;advisor 分片 ≤ 1 ms);加载校验:persist 永远独占线程;housekeeping 不与可阻塞任务同线程、nice 0;dev2 `WatchdogSec ≥ 10 s`。
- **A10 升级演进(C12)**:写换机 runbook(旧机 Drain → 拷贝冷副本 → 新机 `config_source=journal` → 验收);可选:每夜在 ≥ 8 vCPU spot 上以 `rt.host.mode=dev` 跑 c8 线程计划功能回环(需老板批准小额费用)。
- **A11 阶段 1 简化(C19–C24、C26)**:阶段 1 cap 来源 = Kill(+HARD)、Draining、Recovery、DailyLoss、Broken、UnknownTimeout、ClockBad,Reorg/Halted 后补;同版本、尾部为空、无 Unknown/Broken 的**干净 Drain 重启**在闸门通过后自动解除 Recovery(配置 `auto_resume_after_clean_drain`,默认开;崩溃恢复仍需人工);符号表阶段 1 用有序数组/FxHash(API 不变);hft-net 在 dev2 可用 buffered rustls(`Conn` API 不变);raw-feed journal 按 venue 可选 `off|sampled|on`(dev2 默认深度 off、BBO sampled,raw-gw 始终 on);CI 门槛分阶段(bench-gate、gen-schema 推迟);kill-switch 若核实 Gate 合约 `countdown_cancel_all` 可用则推迟到阶段 2;journal-archiver vendoring Trading 的几个 s3 函数。

## B. 潜在 bug 修正(规则直接写入归属规格)

- **B-1 SendReport**:非 Written 状态只对 `op=Place` 把槽迁到 Rejected;Cancel/Amend/Query/CancelAll 只清 CANCEL_SENT/AMEND_SENT 并写 Note,目标槽不变;SendReport 增加 `op`、`target_slot`、`target_gen`。
- **B-2 重启后查单**:恢复并 SetEpoch 之后,sequencer 为每个非终态槽向 gw 下发 `MirrorRestore{slot, gen, epoch, inst, exch_order_id}`;无法编码的查询保持 Unknown 并转交 reconciler。
- **B-3 epoch/gen 匹配**:OrderEvent 携带 epoch,OMS 按 (epoch, slot, gen) 匹配;engine_epoch 在发出第一笔订单前持久化到冷副本并 fsync,取 max(快照, 持久值)+1;**版本变化的冷启动只在 journal 末尾是 Drain 快照时允许**,否则先用旧二进制恢复并 Drain。
- **B-4 LegTracker**:naked = 已成交 + 在途(hedge_sent − hedge_filled、Pending/Unknown 槽剩余量);group 内存在 Open/Hedging 的 LinkState、Pending/Unknown 槽、或 Broken/Reorg cap 时不发任何自动 HedgeFix(含 ClosePolicy 的 hedge_fix)。
- **B-5 第二腿重试**:每次失败(风控拒、place Err、本地发送失败、交易所拒)登记 OMS 定时器 `HedgeRetry`,间隔 max(100 ms, backoff_until);门控拒绝不计次但累计时长;超过 `hedge_deadline_ms` 仍未对冲 → Broken cap + P0。
- **B-6 venue_ready**:只是第 5 层条件门、只拒 Open;对冲与减仓在 #0 只检查 qty、tick/lot、inst 可交易。
- **B-7 链上槽数量**:成交量一律是基础资产(代币)数量:Buy 取 amount_out,Sell 取 amount_in;计价资产数量只用于名义与 ledger。
- **B-8 TSC 跨开机**:快照中 TSC 量与 boot_id 同存;boot_id 不同 → 定时器按 UTC 剩余时长重建,退避与令牌桶清零,`t_state` 取恢复时刻;周期性系统定时器每次启动重新登记。
- **B-9 HARD**:`hard` 只允许与 Global 组合;set_cap 合并取更严(cflags/cancel_resting/flatten 按位或);撤 HARD 只经 `Resume{Kill, Global}`。
- **B-10 kill/Drain 撤单是持续义务**:带 cancel_resting 的 Kill cap 与 Draining cap 生效期间,每次 ConnState Up 或 Unknown 收敛为 Live 都重发撤单。
- **B-11 Drain**:带 `cancel_resting` 参数(SIGTERM 取 `shutdown.cancel_open_orders`);不撤单时等到所有 IOC/FOK 与 LINKED 槽终态;三处统一。
- **B-12 FeeOnly**:新增回报种类 `FeeOnly`:任何状态下只累加费用交 ledger,不迁移状态、不请求对账;gen 不符按 (venue, account, exch_order_id) 入账。
- **B-13 REPORT_DECODE**:回报解码失败按用户流缺口处理:该 (venue, account) 非终态槽置 VERIFY 并查询;连续 N 次按 Down。
- **B-14 AdoptExchange**:ReconcileReport 带 `view_seq`;该 inst 自 view_seq 以来有成交 → `PreconditionFailed{StaleReport}`;同一命令同量调整 LegTracker。
- **B-15 对账窗口**:S0 有非终态槽或 (S0 − user_stream_lag, S1] 内有事件的 inst 视为在途;单轮差异只给 Warn,CloseOnly 需同一差异连续两轮;`skipped_inflight > 0` 的轮次不计入恢复所需 Ignore 轮数;分级只在 hft-risk 做一次(reconciler 只报差异)。
- **B-16 对账条目**:`UnknownResolved`/`OpenOrder` 带 gen 与 epoch;仅完全匹配且槽仍为 Unknown/VERIFY 时应用。
- **B-17 日切**:快照存 `last_day_rolled`;时钟可信且 UTC 日 > 该值时(含启动后第一轮)写 `Timer{DayRoll{day}}`;day ≤ 该值的 DayRoll 忽略。
- **B-18 journal 满**:超过安全阈值在下一个事件边界滚动(不等空闲轮);为滚动预留引导块 × 2 空间;`AppendErr::Full` → `Fatal(JournalFull)` 并 abort,交给死手与 kill-switch。
- **B-19 refdata 不可变量**:已存在 AssetId 的指数、InstId 的合约乘数变化、容量缩小一律拒绝;Drain 写快照前再校验,不通过不以 75 退出、新 blob 标 Rejected;ReplaceRefdata 要求 `config_source = journal`。
- **B-20 dev2 统一数字**:段大小与 hot_keep 只在 hft-journal 定义;hft-core 预算从它推导;testing/latency-budget/notes 链接不复写。
- **B-21 部署**:每个单元自建自己的目录(RuntimeDirectory/ExecStartPre);dev2 覆盖文件同时重写 CPUAffinity。
- **B-22 恢复边界**:引导块写到一半崩溃 → 该段按 Preparing 整段删除;事件内产生的 Meta 不置 EVENT_END。
- **B-23 数值**:group 内各腿必须同一报价资产(refdata 加载校验);refdata 校验 px_exp + qty_exp ≤ −6(或 checked 饱和);每个 `ref_inst` 必须被订阅(加载校验)。
- **B-24 资金费合成**:只推费率的所,由 sequencer(而非 venue-* 纯协议 crate)按持仓 × mark × rate 合成 FundingSettle。
- **B-25 环满**:环满的轮次按空闲处理,带超时 park(dev2 不忙等)。
- **B-26 kill-switch 交接**:出现新 (pid, boot_id) 后完成当前这一轮撤单即停止;心跳块映射检查 inode、处理 SIGBUS 重映射。
- **B-27 链上(阶段 3 chain-core 与阶段 1' Python 加固共用)**:运营商按封禁/限流域划分;回执来源独立运营商 ≥ 3;可用独立来源 < 2 持续 N 块 → P0 并转人工(不推断);risk_reducing 意图另设 gas 硬上限,超过 → P0 转人工。
