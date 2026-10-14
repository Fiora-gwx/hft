# 全局不变量

格式与规则见 [docs/tests](README.md)。本页是 DST 与属性测试每一步都断言的全局不变量 `INV-n` 的唯一定义(原 testing §4,序号不变);hft-testkit 的检查器 `I<n>` 逐条实现它们([hft-testkit DST 框架](../../crates/hft-testkit/README.md#6-dst-框架))。

**检查时机与数据源**(全部条目通用):一"步"= DST 世界处理完一条 Input 及其全部 Output(含排空 OMS、执行层与风控的输出)。检查器只读:各组件的调试视图(OMS 槽表、LegTracker、InflightBook、链接表;hft-risk 的 `CapSet`、有效模式缓存与令牌桶;hft-exec 的 cell 与 `AutoFixState`;hft-ledger 的账本)、`DstDriver` 交出的出环帧(`GwCmd`,含 `out-sim`)、内存段 journal 与 `MockVenue`/`SimActor` 的模拟真值。违反即 `DstOutcome::Fail{invariant = "I<n>"}`,输出种子、步号与违反前 200 条事件。各条的"例外"只按条目内写明的情形放行。

## 1. 不变量

### INV-1 仓位与库存的每一次变化都能追溯到成交证据,传输错误从不改变仓位

- 规则:[成交增量与净量](../../crates/hft-ledger/README.md#成交增量与净量)、[外部成交](../../crates/hft-ledger/README.md#外部成交externalfill)、[人工干预的采纳](../../crates/hft-ledger/README.md#人工干预的采纳adoptexchange)、[Unknown 收敛流程](../../crates/hft-oms/README.md#unknown-收敛流程)、[SendReport 处理](../../crates/hft-oms/README.md#sendreport-处理)
- 级别:dst
- 里程碑:M2
- 故障:全部 DST 套件;每步检查。最能触发:下单连接断开、`SendReport{WriteError}`、发送后无回报、`REPORT_DECODE`、未登记错误码、旧代数成交、无 client id 成交
- 种子:标准集([scenarios §2.4](scenarios.md#24-种子集))
- 断言:
  - 每步比较本 Input 前后 ledger 的每个 (account, venue, inst) 持仓与每个 (account, venue, asset) 现货库存;任何非零变化都必须由本 Input 内的证据逐笔解释,变化量之和 = 证据净量之和(净量规则见规则链接,到最小单位):① OMS `FillOut` 转出的 `FillDelta`(其 Input 是带成交的 `OrderEvent`,或含匹配 `UnknownResolved` 条目的 `ReconcileReport`);② `ExternalFill`——Input 本身,或同一事件内带 `Note{OmsForeignFill}` 的防御路径与 `StaleGen` 合成,或带 `AUTO_RECON` 的 `Note{ExternalFill}` 的对账合成;③ `AdjustDelta`——`AdoptExchange` 命令(引用的 `report_id` 已生效);④ `FeeOnly` 经 `apply_fee` 返回的 `NetAdjust`(费用为基础资产时);⑤ 可选扩展(`chain` feature)的 `TxLifecycle{IncludedOk}`
  - kind 为 `ConnState`、`SendReport`(任何 status)、`Timer{Oms, …}` 与 `Timer{Sys, …}` 的 Input 不改变任何持仓与库存
  - 上述 Input 把槽迁出 `PendingNew`/`PendingCancel`/`PendingAmend`/`Live` 时,目标状态只能是 `Unknown`;唯一例外是 `SendReport{op = Place}` 的确定未写出状态(`KillBlocked`、`RateLimited`、`NoConn`、`EncodeError`、`RingFull`)迁到 `Rejected`(该单从未写出)。同一 Input 内另有身份匹配且成功的 `OrderDecision{Cancel}` 时允许迁到 `PendingCancel`,只表示主动发起撤单;没有该决策证据时仍失败
  - group 归属的变化(LegTracker 各腿 `net`)只来自 ①–④ 与 `WriteOffResidual` 产生的 `WriteOff`;`WriteOff` 不改变账户级持仓合计
- 实现:`crates/hft-testkit/src/dst/invariants.rs::inv_1_position_change_has_fill_evidence`
- 备注:检查器 `I1`;hft-ledger 与 hft-oms 的属性测试在组件内断言同一性质;主要触发场景 FP-1、FP-9、RC-3、LK-4

### INV-2 每条订单回报都匹配一个槽或墓碑,否则产生告警并进入对账,从不被静默丢弃

- 规则:[回报匹配与成交增量](../../crates/hft-oms/README.md#回报匹配与成交增量)、[墓碑与迟到成交(C1)](../../crates/hft-oms/README.md#墓碑与迟到成交c1)、[回报处理表](../../crates/hft-oms/README.md#回报处理表)、[外来成交(B3)](../../crates/hft-oms/README.md#外来成交b3)
- 级别:dst
- 里程碑:M2
- 故障:全部 DST 套件;每步检查。最能触发:重复与乱序回报、回收槽后的迟到成交、旧代数(`StaleGen`)成交、无 client id 成交、他人订单回报
- 种子:标准集([scenarios §2.4](scenarios.md#24-种子集))
- 断言:每条 `OrderEvent` Input 恰好落入下列一类,并产生该类列出的可观察输出(没有任何一类成立即违反;`FeeOnly` 优先走独立分支):
  - `FeeOnly`:当前槽或墓碑按 `(venue, account, epoch, slot, gen)` 匹配并提供归属;两者都不匹配(含 `SLOT_UNRESOLVED`)时有 `Note{OmsFeeUnattributed}`。槽状态与累计成交不变,手续费恰入账一次。W1 的 DST 按独立读取的 ledger 前后余额/库存验证 `delta = −fee`(费用、返佣均按原始资产精度);现货基础币费用还要求对应持仓 `delta = −fee`,其余手续费持仓不变。零费用保持上述量不变;组件内 `FeeOut` 条数与归属由 [OMS-067](../../crates/hft-oms/TESTS.md#oms-067-回报处理表feeonly-在任何状态都不迁移) 断言(TK-104)
  - 匹配当前槽 `(epoch, slot, gen)`:该槽一条 `Note{OmsTransition}`,或 `OmsStats` 的 dup 计数加 1,或 `Note{OmsAnomaly}`(`v[0] = Table`),或落在[回报处理表](../../crates/hft-oms/README.md#回报处理表)的"保持"格(`PendingCancel`/`PendingAmend` 收到 `Ack`、`Unknown` 收到 `CancelReject`(其他原因)/`AmendReject`/`OutcomeUnknown`、`PendingCancel` 收到 `AmendAck`/`AmendReject`、`QueryResult` 确认当前状态且累计成交不增:槽的状态与成交量不变);`FeeOnly` 另有一条 `FeeOut`
  - 匹配墓碑:`cum_filled > tomb.filled` → `FillOut{flags 含 LATE}` 与 `Note{OmsLateFill}`(P0);否则 dup 计数加 1
  - `slot = SLOT_UNRESOLVED`:无成交 → `Note{OmsForeignOrder}`(P1);有成交 → `Note{OmsForeignFillViaOrderEvent}`(P1),且(扣除 `ext_held` 后仍有量时)一条 `ForeignFill` 与一条 `Note{OmsForeignFill}`,扣除部分写 `Note{OmsExtDedup}`
  - `StaleGen`(不匹配当前槽也不匹配墓碑):无成交 → `OmsStats.stale_gen` 加 1;有成交 → `Note{OmsStaleGenFill}`(P0,`v[2]` = 是否已入账),`exch_order_id ≠ 0` 时另有 `ForeignFill` 与 `Note{OmsForeignFill}`
  - 每条 `ExternalFill` Input:要么入账(`Note{ExternalFill}`),要么按去重键命中已入账记录(ledger `ExtSeen` 含该键),要么被 `auto_held` 全部抵扣(`Note{LedgerAutoHeldOffset}`)
- 实现:`crates/hft-testkit/src/dst/invariants.rs::inv_2_every_report_matched_or_alarmed`
- 备注:检查器 `I2`;hft-oms 的属性测试断言组件内部分;主要触发场景 C1、LK-4、FP-3

### INV-3 每个账户每个资产的未对冲量不超过该账户 RiskParams 的仓位上限

- 规则:[检查链](../../crates/hft-risk/README.md#检查链按廉价--昂贵短路适用范围以本表为唯一定义)、[配置](../../crates/hft-risk/README.md#配置riskparams版本化经-setrisk-替换并作为-configblob-写入-journal)、[LegTracker](../../crates/hft-oms/README.md#legtracker)
- 级别:dst
- 里程碑:M2
- 故障:全部 DST 套件;每步检查。最能触发:第二腿持续被拒、L3 期间现货腿所在 venue 断线(串行腿)、两腿 lot 不齐、基础币扣费
- 种子:标准集([scenarios §2.4](scenarios.md#24-种子集))
- 断言:
  - 记 `L(a, X)` = 账户 `a` 中引用资产 `X` 的各 group 的 `GroupLimits.max_pos_qty` 的最大值(没有这样的 group 时取 `AcctLimits.default_group.max_pos_qty`);每步:`|Σ_{inst ∈ X} 持仓(a, inst)|` ≤ `L(a, X)`(现货取 ledger 库存、永续取带符号仓位,同资产同尺度相加)
  - 每个 (a, g):`LegView.naked_qty` ≤ 该 group 各腿 `max_pos_qty` 的最大值
  - 每个增加 |仓位| 的出环 `Place`(Open,与不带 reduceOnly 的 Hedge):出环时 |持仓 + 同向在途 + qty| ≤ `max_pos_qty`(Open 计即时与挂单两类在途,Hedge 只计即时在途;口径同检查链 #7)
  - 例外:由 `ExternalFill{cause ∈ {Manual, Unknown}}`、`AdoptExchange` 或 DST 注入的交易所真值差异使某 (a, X) 超出时,该 (a, X) 的前两条从这一步起暂停检查;旧代数/外来 OrderEvent 由同事件 ExternalFill 账本注记及实际持仓增量证明。只有资产净额、各品种持仓绝对量及 group 含在途裸量均回到上限以内才解除,避免被另一腿暂时抵消的超限在先平永续时重新暴露;第三条不例外
- 实现:`crates/hft-testkit/src/dst/invariants.rs::inv_3_unhedged_within_risk_limits`
- 备注:检查器 `I3`;原文"`RiskParams` 限额"取仓位上限 `max_pos_qty`(检查链 #7 的同一字段);主要触发场景 MS-3、BK-1、LT-1

### INV-4 有 Unknown 槽的 group 在收敛前不开新仓、不做反向操作

- 规则:[Unknown 收敛流程](../../crates/hft-oms/README.md#unknown-收敛流程)、[OnFill](../../crates/hft-oms/README.md#onfill)、[检查链](../../crates/hft-risk/README.md#检查链按廉价--昂贵短路适用范围以本表为唯一定义)、[运行模式、Drain 与 Unknown](../../crates/hft-exec/README.md#运行模式drain-与-unknown)
- 级别:dst
- 里程碑:M2
- 故障:全部 DST 套件(全部套件设 `unknown_blocks_open = true`);每步检查。最能触发:下单连接断开、发送后无回报、未登记错误码、`WriteError`、无过期保护的所返回 NotFound
- 种子:标准集([scenarios §2.4](scenarios.md#24-种子集))
- 断言:按每条 New 决策的时刻判断(应用本次 Input 之后、提交该单之前);对每个 (a, g),其 `LegState.unknown_slots > 0` 期间:
  - 没有 `class = Open` 且 `reject = 0` 的 `OrderDecision{New}` 属于 (a, g);该 group 的 Open 意图都以 `VENUE_NOT_READY` 或 `UNKNOWN_PENDING` 被拒
  - 含 `Unknown` 槽的链接没有 `link_role ∈ {SecondRetry, Reverse}` 的 `OrderDecision{New}`
  - 处于 `Unknown` 的槽只出现 `OrderDecision{Query}` 或 `{Cancel}`,从不以相同 `(slot, gen)` 再出现 `New`;没有引擎订单以该槽的剩余量为对象做反向
  - 例外:`reason ∈ {MarginStress, Hard}` 引擎目标的永续 reduce-only 切片(`Flatten` 类)照常推进
- 实现:`crates/hft-testkit/src/dst/invariants.rs::inv_4_unknown_group_no_open_no_reverse`
- 备注:检查器 `I4`;主要触发场景 FP-1、FP-9、C1、B-2

### INV-5 kill 生效后不带 RISK_REDUCING 的下单与改单不离开受影响的 gw,HARD 范围内任何下单与改单都不离开

- 规则:[kill 字](../../crates/hft-risk/README.md#kill-字唯一的跨线程共享可变量)、[Kill](../../crates/hft-risk/README.md#kill)、[GwRuntime 循环结构](../../crates/hft-venue/README.md#4-gwruntimepoll_once-的循环结构)、[检查链](../../crates/hft-risk/README.md#检查链按廉价--昂贵短路适用范围以本表为唯一定义)
- 级别:dst
- 里程碑:M2
- 故障:全部 DST 套件;每步检查。最能触发:`Kill`(全局、按账户、按 venue、带 `hard`)与 `SetParams` 交错、Viewer 紧急停止、HARD 期间第二腿到期重试、按账户 `Kill{hard}` 与另一账户的对冲
- 种子:标准集([scenarios §2.4](scenarios.md#24-种子集))
- 断言:
  - 检查器在每步之后按剩余 `Kill` cap 重算每个 gw 的 kill 字(与 `mode.kill_bits()` 相等);对每个出环帧 `op ∈ {Place, Amend}`,以产生它的 Input 处理完之后的 kill 字调用 hft-venue 的纯函数 `kill_admit(word, mask, acct_bits, ix, op, oflags)`,结果必须为真——即覆盖该帧账户的 `GLOBAL`/`VENUE`/`account(ix)` 位置位时帧必须带 `RISK_REDUCING`;`HARD` 或该账户 `account_hard(ix)` 置位时该范围内没有任何 `Place`/`Amend` 帧
  - 模拟账户的帧(`out-sim`)同样以覆盖该账户的 `Kill` cap 判定
  - `Cancel`、`CancelAll`、`Query` 与维护类帧(含死手刷新)在任何 kill 状态下都不被 MockVenue 的 gw 层回 `SendReport{KillBlocked}`;带撤单义务的 `Kill` 生效期间,范围内可撤的非 `RISK_REDUCING` 槽(`open_only = false` 时为全部可撤槽)在该 Input 或随后的 `Timer{OmsContinue}` 中都有 `OrderDecision{Cancel}` 或 `{CancelAll}`
- 实现:`crates/hft-testkit/src/dst/invariants.rs::inv_5_kill_blocks_non_risk_reducing`
- 备注:检查器 `I5`;gw 侧 kill 位矩阵的组件测试归 hft-venue;主要触发场景 MD-10、MD-11、C8

### INV-6 任一时刻从最近的快照加后续事件重建的状态与运行中状态逐组件相同

- 规则:[快照](../../crates/hft-sequencer/README.md#快照)、[状态哈希](../../crates/hft-sequencer/README.md#状态哈希)、[确定性规则](../../crates/hft-sequencer/README.md#确定性规则)、[状态哈希对比](../../crates/hft-testkit/README.md#7-状态哈希对比)
- 级别:dst
- 里程碑:M2
- 故障:全部 DST 套件;每 1 000 步、每次写出 `StateSnapshot` 之后与场景结束时检查。最能触发:任意时刻崩溃后 `RecoverDriver` 重启、关键帧与段滚动、命令与 `SetParams` 交错、advisor 落后
- 种子:标准集([scenarios §2.4](scenarios.md#24-种子集))
- 断言:取内存段 journal 中最近一条 `StateSnapshot`(关键帧的 `state_hash` 由读者侧计算),以验证驱动重放其后的全部 Input 到当前 seq:`ComponentHashes{book, oms, risk, ledger, exec, strategies, all}` 与运行中实例在同一 seq 的值逐项相等;重放产生的 Output 记录与 journal 中的 Output 逐字节相等(Meta 类不比对)。真实引擎或 journal 已含快照时,配置/恢复失败、未完成回放或未消费输出均失败,不得以空重建结果跳过定期或结束检查
- 快照边界:每条快照以所属 Input 或启动完成的 EVENT_END 为比较位置,观察缺失、重复或跨过后续 Input 均失败;同一世界步内多个快照和重启逐条检查,关闭定期采样不能关闭该检查(TK-115)
- 实现:`crates/hft-testkit/src/dst/invariants.rs::inv_6_state_is_function_of_journal`
- 备注:检查器 `I6`;不一致时用 `bisect_divergence` 报出第一个分歧的 seq 与组件;主要触发场景 RC-1、RC-2、MD-25

### INV-7 OMS 槽不泄漏:每个被分配的槽最终进入终态并回到空闲池

- 规则:[状态机](../../crates/hft-oms/README.md#状态机)、[StaleOrderSweep](../../crates/hft-oms/README.md#staleordersweeptimeroms-sweep1-s)、[墓碑与迟到成交(C1)](../../crates/hft-oms/README.md#墓碑与迟到成交c1)
- 级别:dst
- 里程碑:M2
- 故障:全部 DST 套件;每步与场景结束时检查。最能触发:回报丢失、超时、撤单竞态、`SimReset`、冒烟单到期
- 种子:标准集([scenarios §2.4](scenarios.md#24-种子集))
- 断言:
  - 每步、每张 (venue, account) 表:`unknown_count` = `state = Unknown` 的槽数;`state = Free` ⇔ 槽在空闲表中
  - 每个终态槽在进入终态后 `reuse_quarantine_ms + ⌈非空闲槽数 / sweep_batch⌉ × 1 000 ms + 1 000 ms` 引擎运行时间内回到 `Free`
  - 场景结束时(停止注入、推进虚拟时间超过上一条的时限之后):非空闲槽数 = 非终态槽数(仍在途的订单);空闲槽数 = 初始槽数 − 非终态槽数
- 实现:`crates/hft-testkit/src/dst/invariants.rs::inv_7_slots_do_not_leak`
- 备注:检查器 `I7`;hft-oms 属性测试另断言槽表的内部不变量

### INV-8 OnFill 第二腿的对冲数量之和等于触发它的第一腿累计净成交量

- 规则:[OnFill](../../crates/hft-oms/README.md#onfill)、[成交净量(B4)](../../crates/hft-oms/README.md#成交净量b4)、[成交增量与净量](../../crates/hft-ledger/README.md#成交增量与净量)、[对冲量上限(B6)](../../crates/hft-exec/README.md#对冲量上限b6)
- 级别:dst
- 里程碑:M2
- 故障:全部 DST 套件;每步检查。最能触发:部分成交、基础币扣费与费用晚到(`FeeOnly`)、两腿 lot 不齐、第二腿被拒与重试、无 client id 成交、超量成交
- 种子:标准集([scenarios §2.4](scenarios.md#24-种子集))
- 断言:
  - 对每个 (a, g) 与其对冲腿 h,只计 `HedgePlan.kind = OnFill` 且从未发出 `Reverse` 的链接,每步成立(i128 精确):Σ 链接的 `hedge_req` + Σ 被裸量上限截去的量(该 group 全部 `Note{OmsHedgeNakedCapped}` 的 `v[1] − v[2]`)+ `residual[g][h]` = Σ 链接的 `first_net` × `ratio_num / ratio_den` + Σ 本 group 经 `net_adjust` 记入余数的手续费补差
  - 每条 `HedgeRequest{role = Second}` 的 `basis` = 触发它的成交增量 `d_net`(基础资产手续费已扣除;费用未到时 = 毛量)× ratio + 发出前的 `residual`;`qty` ≤ `basis` 向零取整到对冲腿 lot;任何 `Second` 的数量都不按意图数量或毛量计算(有基础资产手续费的成交,`basis` < 毛量 × ratio)
  - 超量成交的超出部分(`FillOut.flags` 含 `OVERFILL`)不计入 `first_net`
  - 真实引擎的退休链接历史按账户、组、对冲品种与比例累计;相邻 Input 间已有历史桶不可消失,链接数、净成交、请求量与截断量不可减少或重复成行
- 实现:`crates/hft-testkit/src/dst/invariants.rs::inv_8_second_leg_equals_net_fill`
- 备注:检查器 `I8`;`Simultaneous` 与发出过 `Reverse` 的链接按各自规则另由 hft-oms 属性测试覆盖;主要触发场景 B4、FP-12、LT-1

### INV-9 同一种子两次运行(含 x86_64 与 aarch64)的意图流与状态哈希序列逐字节相同

- 规则:[确定性规则](../../crates/hft-sequencer/README.md#确定性规则)、[确定性规则](../../crates/hft-strategy-sdk/README.md#确定性规则)、[状态哈希对比](../../crates/hft-testkit/README.md#7-状态哈希对比)、[hft-dst 运行器](../../crates/hft-testkit/README.md#8-hft-dst-运行器)
- 级别:dst
- 里程碑:M2
- 故障:全部 DST 套件;每次运行抽样复跑。最能触发:同一时刻多条 Input 的交错、定时器与输入环交错、崩溃恢复、advisor 追赶
- 种子:标准集([scenarios §2.4](scenarios.md#24-种子集))
- 断言:
  - 每次运行按种子哈希抽 5% 的种子在同一进程内复跑:两次的 Output 记录序列(Meta 除外)逐字节相同,`HashTrail` 与事件摘要(全部 journal 记录的 xxh3-128)相同;只比较两次运行,不与仓库中登记的摘要比较
  - 同一种子在 `hft-dst run` 与 `cargo xtask dst --seeds 1 --seed <S>` 下的违反列表与 `HashTrail` 相同
  - 版本标签作业:同一提交上 x86_64 与 aarch64 各跑一次 DST 冒烟,两侧均无违反(不比较登记摘要)
- 实现:`crates/hft-testkit/src/dst/invariants.rs::inv_9_same_seed_same_trail`
- 备注:检查器 `I9`

### INV-11 有效模式恒等于由期望模式、生效 cap 与 cell 是否 flat 重算的值

- 规则:[有效模式](../../crates/hft-risk/README.md#有效模式)、[cap 记录](../../crates/hft-risk/README.md#cap-记录)、[mode 模块](../../crates/hft-risk/README.md#mode-模块唯一的模式状态)
- 级别:dst
- 里程碑:M2
- 故障:全部 DST 套件;每步检查。最能触发:多个 cap 叠加与逐条 `Resume`、`SetMode` 与 cap 交错、Shadow 有仓位、`SetParams` 增删 group、MarginStress 档位升降
- 种子:标准集([scenarios §2.4](scenarios.md#24-种子集))
- 断言:
  - 每步对每个 cell c,从头重算 `E(c)`:期望模式为 `Shadow` 时,`cell_flat(c)` 为真 → `Shadow`,否则 `CloseOnly{deferred_shadow = true}`;否则 `level` = 覆盖 c 的全部 cap 的 `level_of` 最大值(venue 级 cap——`Kill{Venue}`、`Recon`、`UnknownTimeout`、`AccountCfg`、`Delisting`——不参与 cell 计算),`level ≥ CloseOnly` → `CloseOnly`,否则为期望模式;断言 `mode.effective(c) == E(c)`(增量缓存与重算相同)
  - 每步从剩余 `Kill` 记录重算的各 gw `KillBits` 与 `mode.kill_bits()` 相等
  - `SetMode` 前后 `CapSet` 不变;cap 的增删前后期望模式表不变
- 实现:`crates/hft-testkit/src/dst/invariants.rs::inv_11_effective_mode_recomputes`
- 备注:检查器 `I11`;hft-risk 的 mode 属性测试在组件内断言同一性质;主要触发场景 MD-1、MD-2

### INV-12 带 RISK_REDUCING 的单只被 HARD 拦,其余任何 cap 都不拦它

- 规则:[检查链](../../crates/hft-risk/README.md#检查链按廉价--昂贵短路适用范围以本表为唯一定义)、[cap 记录](../../crates/hft-risk/README.md#cap-记录)、[分类](../../crates/hft-risk/README.md#分类)、[kill 字](../../crates/hft-risk/README.md#kill-字唯一的跨线程共享可变量)
- 级别:dst
- 里程碑:M2
- 故障:全部 DST 套件;每步检查。最能触发:非 HARD 的 Kill 与锁存期间的第二腿与重试、`DRAINING` 期间的补腿、时钟不可信、`Halted`、日亏锁存、对账 cap、`UnknownTimeout`
- 种子:标准集([scenarios §2.4](scenarios.md#24-种子集))
- 断言:
  - 分类为 `risk_reducing = true` 的每条 `OrderDecision` 若 `reject ≠ 0`,其位 ∉ {`CLOCK`、`DRAINING`、`KILLED`、`DAILY_LOSS`、`RECON_DEVIATION`、`CLOSE_ONLY`、`VENUE_NOT_READY`、`UNKNOWN_PENDING`、`EXPOSURE_GATE`、`LINK_STALE`、`LINK_FUNDS`};`HARD` 与非门控的检查位(结构、价格带越价上限、名义与对冲量、仓位、速率)照常可出现
  - `origin ∈ {Exec, Oms}` 的风险递减单从不被模式门记为 `SHADOW` 或因有效模式被拒;策略在期望模式为 `Shadow` 的 cell 中的意图除外(期望模式不是 cap)
  - 带 `RISK_REDUCING` 的出环帧收到 `SendReport{KillBlocked}` 时,该帧发出时的 kill 字含 `HARD` 或该账户的 `account_hard(ix)`
- 实现:`crates/hft-testkit/src/dst/invariants.rs::inv_12_risk_reducing_only_hard_blocks`
- 备注:检查器 `I12`;主要触发场景 MD-4、MD-14、B6

### INV-13 Resume、SetMode 与 cap 的设置和解除本身从不产生开仓或对冲单

- 规则:[mode 模块](../../crates/hft-risk/README.md#mode-模块唯一的模式状态)、[有效模式](../../crates/hft-risk/README.md#有效模式)、[HARD 解除时的 Broken 判定](../../crates/hft-exec/README.md#hard-解除时的-broken-判定resolve_broken)、[命令路径](../../crates/hft-sequencer/README.md#命令路径)
- 级别:dst
- 里程碑:M2
- 故障:全部 DST 套件;每步检查。最能触发:`Resume{Kill}`(含解除 HARD)、`Resume{HedgeFailure}`、`Resume{Recovery}`、`SetMode` 升降级、`Timer{ClockOk|ClockBad}`、`Timer{StorageOk}`、`VenueNotice`
- 种子:标准集([scenarios §2.4](scenarios.md#24-种子集))
- 断言:对 kind 为 `ControlCmd{SetMode | Resume | Kill | VenueNotice}` 的 Input,以及只改变 cap 的 `Timer{Sys, ClockOk | ClockBad | PersistLag | DiskLow | StorageOk}` Input:
  - 本 Input 内没有 `link_role ∈ {Second, SecondRetry, Reverse, HedgeFix}` 的 `OrderDecision{New}`,也没有执行层种类为 `HedgeFix`、`ResidualTrim`、`ExternalDefault`、`AutoFix` 的引擎单
  - 使任一 cell 有效模式下降或新增 cap 的 Input 内没有 `class = Open` 的 `OrderDecision{New}` 与 `{Amend}`;它的 `OrderDecision{Cancel}`/`{CancelAll}` 只指向 `ModeOutcome.cancels` 范围内的槽(`open_only = true` 时只有非 `RISK_REDUCING` 的 Open 类挂单)
  - 使有效模式上升的 Input 内,`class = Open` 的 `OrderDecision{New}` 只来自策略在本 Input 投递中发出的意图(`origin = Strategy`)或该 Input 之前已设下的策略目标的切片;`Kill{Account, flatten}` 产生的 `Flatten` 类单不在本条限制内
- 实现:`crates/hft-testkit/src/dst/invariants.rs::inv_13_mode_changes_emit_no_open_or_hedge`
- 备注:检查器 `I13`;主要触发场景 MD-2、MD-3、BK-7

### INV-14 门控拒绝不计入执行失败计数,只有交易所执行结果才计入

- 规则:[门控拒绝与退避](../../crates/hft-exec/README.md#门控拒绝与退避)、[SendReport 处理](../../crates/hft-oms/README.md#sendreport-处理)、[OnFill](../../crates/hft-oms/README.md#onfill)、[Ctx 与订单意图](../../crates/hft-strategy-sdk/README.md#ctx-与订单意图)、[Throttle](../../crates/hft-strat-lib/README.md#throttle)
- 级别:dst
- 里程碑:M2
- 故障:全部 DST 套件;每步检查。最能触发:`KillBlocked`、限速与退避、`NoConn`、报价连接不健康(`StaleQuote`)、对手价越过上限(`BeyondCross`)、cap 与有效模式拒绝
- 种子:标准集([scenarios §2.4](scenarios.md#24-种子集))
- 断言:
  - OMS 链接的 `retries` 只在本链接对冲单的交易所 `Reject`、IOC 终态未全成、`place` 错误或 `SendReport{EncodeError}` 时加 1;`hedge_gated`(`StaleQuote`、`BeyondCross`、`HEDGE_LIMIT` 以外的风控位)与 `SendReport{KillBlocked | RateLimited | NoConn | RingFull}` 前后 `retries` 不变
  - 执行层 cell 的 `fail_streak` 与补腿 `count` 只在交易所 `Reject`、`SendReport{EncodeError}` 与引擎单的 `PX_BAND`/`NOTIONAL_*` 拒绝(`RiskReject`)时加 1;其余风控位、模式门与上述四种门控 `SendReport` 前后不变
  - 给策略的 `OrderUpdate::Rejected`:门控拒绝的 `bits ≠ 0`(风控 `RejectBits` 或 `LOCAL_SEND_FAIL`),交易所拒单的 `bits = 0`
- 实现:`crates/hft-testkit/src/dst/invariants.rs::inv_14_gate_rejects_not_counted`
- 备注:检查器 `I14`;策略侧 `Throttle` 的 `Gated` 不改变 `consec_fail` 由 hft-strat-lib 属性测试断言;主要触发场景 B-5、LK-3、FP-2
- place 证据:真实引擎逐 Input 保存调用前原始事实,检查错误与空闲槽数并按完整链接身份限制计数增量;观察缺失、重复或跨链接复用失败,反例验收见 [TK-121](../../crates/hft-testkit/TESTS.md#tk-121-对冲-place-失败必须有同一链接实例的原始证据)。

### INV-15 对冲类单的数量不超过基数加 1 lot、名义不超过对冲单笔上限,Flatten 类名义不超过减仓单笔上限

- 规则:[对冲量上限(B6)](../../crates/hft-exec/README.md#对冲量上限b6)、[限价与单笔名义](../../crates/hft-exec/README.md#限价与单笔名义)、[检查链](../../crates/hft-risk/README.md#检查链按廉价--昂贵短路适用范围以本表为唯一定义)、[OnFill](../../crates/hft-oms/README.md#onfill)
- 级别:dst
- 里程碑:M2
- 故障:全部 DST 套件;每步检查。最能触发:解码单位错使 `cum_filled` 放大(超量成交)、外来成交、Broken 自动有界平仓、L3 与清仓、残余修剪
- 种子:标准集([scenarios §2.4](scenarios.md#24-种子集))
- 断言:
  - 每个 `class = Hedge` 的出环 `Place`:`qty ≤ basis + 1 lot(该 inst)`,`basis` 由检查器在决策那一步按规则链接的表从调试视图独立算出(`Second`:`d_net × ratio + residual`;`SecondRetry`:`owed`;`Reverse`:`first_net − hedge_filled × den / num`;`HedgeFix`、`ResidualTrim`、`ExternalDefault`、`CmdHedgeFix`、`AutoFix`:`|Σ leg[i].net|`);名义 `⌈qty × px × quote_rate⌉ ≤ max_hedge_notional`;`AutoFix` 另 ≤ `max_reduce_notional`
  - 每个 `class = Flatten` 的出环 `Place`:名义 ≤ `max_reduce_notional`
  - 每条 `reject = HEDGE_LIMIT` 的 `OrderDecision` 不出环;同一 Input 内该 (account, group) 的 `Broken` 与该账户 `HedgeFailure` 处于生效(本 Input 新设时有 `Note{CapSet}`),并有一条 P0 注记(OMS 来源为 `Note{OmsHedgeBroken}`)
- 实现:`crates/hft-testkit/src/dst/invariants.rs::inv_15_hedge_qty_and_notional_capped`
- 备注:检查器 `I15`;主要触发场景 B6、BK-1、MS-3

### INV-16 无在途时账本的仓位、现货库存与余额等于模拟真值

- 规则:[仓位与已实现](../../crates/hft-ledger/README.md#仓位与已实现平均成本法)、[资金费对账校正](../../crates/hft-ledger/README.md#资金费对账校正)、[外部成交](../../crates/hft-ledger/README.md#外部成交externalfill)、[hft-sim 不变量](../../crates/hft-sim/README.md#10-不变量)
- 级别:dst
- 里程碑:M2
- 故障:全部 DST 套件;每步对满足条件的账户检查。最能触发:基础币扣费与费用晚到、全仓资金费只推余额、交易所逐笔资金费与本地入账不一致、外来成交、模拟账户崩溃恢复
- 种子:标准集([scenarios §2.4](scenarios.md#24-种子集))
- 断言:
  - 对每个 (account, venue),在"无在途"的步——该账户在该 venue 没有非终态槽,`MockVenue`/`SimActor` 对该账户没有未交付的回报、`FeeOnly` 或 `ExternalFill`,gw 暂存(`unmatched_hold`)为空——ledger 的每个 inst 持仓、每个资产的现货库存与余额等于 `SimExchange::account()` 真值(数量到最小单位,手续费与资金费到 `Notional` 的最小单位)
  - 资金费:注入了逐笔差额的结算资产余额在该笔 `AdjustDelta::Funding` 入账(`Note{FundingCorrected}`)之后才比较,之后同样相等
  - 稀疏行按两边键的并集比较,一边缺行视为零,交易所已归零不能掩盖引擎残留;适用账户/venue 缺真值、缺品种元数据或键重复均失败。现货按资产总库存比较,不把单品种策略归属当成总库存(TK-125)
  - 例外:DST 注入的用户流缺口 ADL 在 `AUTO_EXTERNAL` 合成入账之前、`ReconActor` 注入的待人工采纳差异在 `AdoptExchange` 之前,该 (account, venue, inst/asset) 不比较
- 实现:`crates/hft-testkit/src/dst/invariants.rs::inv_16_ledger_matches_sim_truth`
- 备注:检查器 `I16`;主要触发场景 FD-1、FD-2、AX-1、B4

### INV-17 MarginStress L3 的强制减仓只减不增,永续腿先行,每一步目标按 force_step 收缩

- 规则:[保证金危急的默认减仓(A5)与 engine_flatten](../../crates/hft-exec/README.md#保证金危急的默认减仓a5与-engine_flatten)、[PairOnFill](../../crates/hft-exec/README.md#paironfill)、[保证金分档](../../crates/hft-risk/README.md#保证金分档marginstress)
- 级别:dst
- 里程碑:M2
- 故障:全部 DST 套件;每步检查。最能触发:到强平距离路径(`margin_path`)、单边 +30% 行情、L3 期间另一所 BBO 断开或下单连接断开、L3 时存在 `Broken` cell、L3 中途重启
- 种子:标准集([scenarios §2.4](scenarios.md#24-种子集))
- 断言:
  - 每个 `reason = MarginStress` 的引擎订单(`Flatten` 类):方向使该腿 |仓位| 减小,`qty ≤ |仓位| − 同向即时在途`(发出时),所以每腿只向 0 移动、不越过 0
  - 每次处理 `ForceDeleverage` 设定的每条腿目标 `t_i`:`t_i = 0` 或与基准 `base_i` 同号,且 `|t_i| ≤` `|base_i| × (1 − force_step)` 向零取整到 lot(`base_i` = 该 cell 尚未完成的上一步引擎目标,否则当前仓位);策略目标更接近 0 时 `t_i` 取策略目标
  - 非 Broken、未进入 `SerialFallback` 的 cell:同一步两腿目标按同一比例设定;该步该 cell 的第一张引擎订单在永续腿上(`PerpFirst`)
  - `Broken` 与 `SerialFallback` 的 cell:该 cell 任一永续腿 |仓位| 仍高于其引擎目标且不低于最小下单量时,没有该 cell 现货腿的减仓单
- 实现:`crates/hft-testkit/src/dst/invariants.rs::inv_17_l3_reduces_toward_zero_perp_first`
- 备注:检查器 `I17`;主要触发场景 A5、MS-3、BK-3

### INV-18 Drain 以 Done 结束(非 DrainTimeout)时不留裸腿,也不截断引擎减仓

- 规则:[Drain](../../crates/hft-sequencer/README.md#drain)、[入口](../../crates/hft-exec/README.md#入口全部在-sequencer-线程调用无分配)、[运行模式、Drain 与 Unknown](../../crates/hft-exec/README.md#运行模式drain-与-unknown)、[Drain 静止判定](../../crates/hft-oms/README.md#drain-静止判定支持-c11)
- 级别:dst
- 里程碑:M2
- 故障:全部 DST 套件;每次 Drain 结束时检查。最能触发:Drain 时有已发第一腿、未发第二腿的链接,有已 Ack 的 IOC,有 HedgeRetry 定时器,Drain 中发 `Kill{flatten}` 或进入 L3
- 种子:标准集([scenarios §2.4](scenarios.md#24-种子集))
- 断言:
  - `Sequencer` 的 Drain 返回 `StepOutcome::Done` 且本次 Drain 没有 `Note{DrainTimeout}` 的那一步:OMS 链接表中没有 `Open`/`Hedging` 链接;没有非终态的 IOC/FOK 槽、没有 `Pending*` 槽;定时器堆中没有 `Timer{Oms, HedgeRetry}`;`exec.drain_ready()` 为真且每个账户 `drain_ready_acct(a)` 为真;执行层没有未到达的 `reason ∈ {MarginStress, Hard}` 引擎目标(含无组 cell 与对齐目标)
  - 该次 Drain 写出的 `purpose = Drain` 快照中没有 `Open`/`Hedging` 状态的链接行
  - 上述任一条件在 `drain_timeout_ms` 到期时仍不满足,则该次 Drain 必须写 `Note{DrainTimeout}`,不得以无注记的 `Done` 结束
- 实现:`crates/hft-testkit/src/dst/invariants.rs::inv_18_drain_leaves_no_naked_leg`
- 备注:检查器 `I18`;主要触发场景 C11、MD-14、RC-9

### INV-19 对冲不越过裸量、减仓不越过仓位,同一笔成交不被对冲两次

- 规则:[OnFill](../../crates/hft-oms/README.md#onfill)、[LegTracker](../../crates/hft-oms/README.md#legtracker)、[回报匹配与成交增量](../../crates/hft-oms/README.md#回报匹配与成交增量)、[实现方必须遵守的契约](../../crates/hft-venue-proto/README.md#实现方必须遵守的契约)、[Broken 处置](../../crates/hft-exec/README.md#broken-处置)
- 级别:dst
- 里程碑:M2
- 故障:全部 DST 套件;每步检查。最能触发:无 client id 的成交(先按外来成交入账、后匹配到槽)、外来成交默认处理与第二腿竞态、重复回报、L3 时策略挂着同向 reduce-only 单
- 种子:标准集([scenarios §2.4](scenarios.md#24-种子集))
- 断言:
  - OMS 产生的 `Second`、`SecondRetry`:`qty ≤ naked_dir`(LegTracker 在决策那一步、写入触发成交之后、计入本单在途之前的该方向裸量);执行层的 `HedgeFix`、`ResidualTrim`、`ExternalDefault`、`CmdHedgeFix`、`AutoFix`:`qty ≤ |Σ leg[i].net|`,且发出时该 group 各腿 `inflight_immediate = 0`
  - L3 与清仓的 `Flatten` 类单:`qty ≤ |该腿持仓| − 同向即时在途`(发出时)
  - 每张对冲类单计入在途之后,该 group 反方向的 `naked_dir = 0`(没有越过裸量的反向敞口)
  - 每条带 `EXT_DEDUP` 的 `FillOut`:`qty = d − min(d, b)`(`b` 为发出前的 `ext_held`);同一去重键(契约 22,在账户内)至多入账一次
- 实现:`crates/hft-testkit/src/dst/invariants.rs::inv_19_no_hedge_beyond_naked`
- 备注:检查器 `I19`;主要触发场景 LK-4、MS-5、B3

### INV-20 Broken 有界:非 HARD 下每个 Broken 期计时满 broken_auto_fix_ms 恰好开始一次自动有界平仓,至多一个终态

- 规则:[Broken 处置](../../crates/hft-exec/README.md#broken-处置)、[人工保持与恢复期抑制](../../crates/hft-exec/README.md#人工保持与恢复期抑制)、[断腿](../../crates/hft-risk/README.md#断腿broken自动有界平仓l3-与-flatten)、[保证金危急的默认减仓(A5)与 engine_flatten](../../crates/hft-exec/README.md#保证金危急的默认减仓a5与-engine_flatten)
- 级别:dst
- 里程碑:M2
- 故障:全部 DST 套件;每步检查。最能触发:第二腿持续被拒使 cell `Broken` 且无人工 `HedgeFix`、自动平仓本身被拒、对冲腿所在 venue 断线或闸门位长期不满足、`manual_hold`、HARD 与解除、L3 或 `Kill{flatten}` 覆盖 Broken、补腿进行中崩溃
- 种子:标准集([scenarios §2.4](scenarios.md#24-种子集))
- 断言:对每个 Broken 期(执行层 cell 进入 `Broken` 到 `CapCleared{Broken}`),计时口径以规则链接为唯一定义:
  - 终态 `Note{ExecAutoFixDone}` 与 `Note{ExecAutoFixFailed}` 合计 ≤ 1;`ExecAutoFixFailed` 的 `fail` ∈ {`ChainLeg`、`MinOrder`、`Rejected`、`Residual`、`Deadline`、`SignFlip`、`RiskReject`},门控拒绝与零成交从不产生终态
  - 每条 `ExecDecision{AutoFix, detail = AutoFix/Start}` 发生在 `now ≥ due_tsc` 时,且 `due_tsc` 与执行层调试视图中按规则链接重算的到期时刻相同(停机、HARD、`manual_hold` 与恢复期按规则不计或重给窗口)
  - 某一步 `phase = Primed`、`now ≥ due_tsc` 且"开始"的等待条件都不成立时,该步的 `advance` 写出 `AutoFix/Start`(不会错过开始);期间有人工 `HedgeFix` 命令(`phase = Cancelled`)时不再自动开始
  - HARD 覆盖该账户期间没有 `AutoFix` 订单;每片 `AutoFix` 的数量累计 ≤ 开始时的 `budget` ≤ 开始时的已成交裸量
  - 该账户处于 L3 或 `engine_flatten` 时,该 Broken cell 的永续腿有 reduce-only 的 `Flatten` 类单,且先于该 cell 的任何现货腿减仓单
- 实现:`crates/hft-testkit/src/dst/invariants.rs::inv_20_broken_auto_fix_bounded`
- 备注:检查器 `I20`;主要触发场景 BK-1、BK-2、BK-3、BK-5

## 2. 可选扩展的不变量

INV-10(可选扩展,链上):每个钱包在途 nonce ≤ 1([06 §2.8](../../design/06-链上执行层.md))。属于未启用的可选扩展,里程碑外;编号保留,用例随 [hft-chain-evm](../../crates/hft-chain-evm/README.md) 解冻时写进它的 `TESTS.md`,检查器 `I10` 只在 `chain` feature 下启用。
