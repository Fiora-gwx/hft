---
description: "sequencer 线程在策略给出目标意图、行情与回报推进、风控要求默认减仓时调用它:把每 (account, group) 的 TargetIntent 按执行规格切成订单请求,并承担补腿、残余、外来成交、断腿(Broken)处置与保证金危急的引擎默认处理"
kind: crate
tier: hot
scope: core
status: spec
---

# hft-exec

## 概要

`hft-exec` 是执行层。它拥有**每 (account, group) 的目标状态**(当前目标、本目标以来的已成、在途、残余、状态机 Idle/Working/Stuck/Broken)与**执行算法**(`TakerSlice`、`PairOnFill`、`Simultaneous`;`MakerFirst` 预留),把策略经 SDK `Ctx::target` 给出的 `TargetIntent` 变成一笔笔订单请求,交给 sequencer 走模式门、风控与 OMS。它也是引擎层面"下什么单"的唯一决定者:自动补腿 `HedgeFix`(只平掉多出的一腿)、残余与尘埃规则、外来成交(自身强平/ADL/人工)无人处理时的默认处理、人工外来成交之后的 cell 级 `manual_hold`、恢复期(多出那一腿所在 venue 放行前)的自动处置抑制与闸门滞留告警、HARD 解除时哪些 group 判 Broken(`resolve_broken`)、**断腿(Broken)处置**(持续 `broken_auto_fix_ms` 后做一次可执行的有界平仓尝试,平掉多出的一腿)、保证金危急 L3 的按比例默认减仓与 `Kill{Account, flatten}` 清仓(两者都覆盖 Broken 的 cell 与该账户不属于任何 group 的永续持仓;设引擎目标前请 OMS 撤该 inst 上本账户的非执行层挂单;另一腿不可用时退回永续先的串行腿 `SerialFallback`;清仓另置账户级 `flatten_lock`,直到 `Resume{Kill}`);腿顺序、两腿 lot 对齐、对冲量上限、引擎单与切片的限价和单笔名义(与风控 #5/#6 同一口径)的计算规则、`ExecDecision.detail` 与执行层订单 `reason` 的码表、"报价可用 = 连接健康"的执行侧用法也归这里。它**不拥有**订单槽、client id 与 LegLink 机制([hft-oms](../hft-oms/README.md))、仓位/库存/权益([hft-ledger](../hft-ledger/README.md))、分类、检查链、cap、模式与保证金分档([hft-risk](../hft-risk/README.md));它与这三者同属 L1、互不依赖,它们的状态经 sequencer 组装的 [SDK](../hft-strategy-sdk/README.md) 只读视图与本 crate 定义的输入结构进入。唯一调用方是 [hft-sequencer](../hft-sequencer/README.md),在同一输入事件内、策略 `Ctx` 应用之后调用;它是纯状态机(无 I/O、无时钟、无分配、无 `dyn`、无 f64),实盘、模拟账户、冷启动恢复与 `replay verify` 跑同一份代码。第一腿成交 → 第二腿的快路径在 OMS 内,不经过本 crate。

## 目录

- [使用本组件](#使用本组件)
- [实现说明](#实现说明)
- [延迟与资源契约](#延迟与资源契约)
- [测试与验收](#测试与验收)
- [依赖](#依赖)
- [延伸阅读](#延伸阅读)
- [已知限制与后续工作](#已知限制与后续工作)

## 使用本组件

### 入口(全部在 sequencer 线程调用,无分配)

事件处理函数只更新状态并把受影响的 cell 标为"待推进";真正的推进(切片、补腿、默认处理、Broken 处置)只在 `advance` 里按 (account, group) 升序发生——这是 [hot-path-rules S4](../../docs/hot-path-rules.md) 的 `exec_step`。

```rust
pub struct Exec { /* 见实现说明:TargetCell 表、CellIndex、待推进位图、每账户状态 AcctState、ExecConfig */ }

impl Exec {
    /// 启动期按 Capacities.max_cells_total 预分配目标单元与索引;之后零分配。全部账户 gate_open = false、venue_open = 0。
    /// 逐行校验 ExecConfig(全局默认行与每个账户行):force_slice 越出目标参数的合法范围(见"目标的接受与替换")→ ForceSlice;
    /// fix_max、max_slice_fails、max_nofill、passive_cooldown_mult 为 0 → ZeroLimit;max_cells_total + 账户数 > 65 535(cix 为 u16)→ Capacity。
    pub fn new(cfg: &ExecConfig, cap: &Capacities) -> Result<Exec, ExecConfigError>;  // ExecConfigError { ForceSlice, ZeroLimit, Capacity }
    /// SetParams 应用后(命令路径,不在 tick 路径):按 GroupSpec 重建 cell 表(cix 按 (account, group) 升序分配)
    /// 与 inst → cell 索引,保留同 (account, group) 的状态;腿定义变化的 cell 清除目标并写 ExecDecision{TargetCleared}。
    pub fn on_groups_changed(&mut self, groups: &GroupTable, rd: &RefView, out: &mut ExecOut);

    // ── 登记(O(1) 或 O(该 inst 的 cell 数 ≤ 8);只改状态、标记待推进;单次产出上限见"输出容量与续推")──
    /// 校验见"目标的接受与替换"(含 flatten_lock 与 L3 期间只接受更接近 0 的目标、两腿 lot 对齐 LotMismatch)。
    /// v 与 advance 用同一视图适配器(读 legs 的当前净量与 mark,供"更接近 0"与 pair_lot 名义判断)。
    /// ro = true:本次 on_event 处理的是带 reduce_only 的 StrategyCmd(MANUAL_RO,sequencer 置位),目标只减不增(ReduceOnlyCmd)。
    pub fn set_target<V: Views>(&mut self, owner: StrategyInstanceId, t: &TargetIntent, ro: bool, v: &V, now: Now,
                                out: &mut ExecOut) -> Result<(), TargetReject>;  // 结果回填 IntentHandle
    pub fn clear_target(&mut self, owner: StrategyInstanceId, a: AccountId, g: GroupId, now: Now, out: &mut ExecOut);
    /// 清除某账户全部目标(`SimReset`、账户删除);不发单。非 Broken 的 cell 置 Idle,Broken 的 cell 只清目标(状态等 hft-risk 的
    /// `CapCleared{Broken}`);清该账户全部 cell 的 FixState、外来成交计时、自动有界平仓状态、`manual_hold` 与串行标记;不清 `flatten_lock`。
    /// 通知策略 `ExecUpdate::Done{cleared_by_engine: true}`。
    pub fn clear_account(&mut self, a: AccountId, now: Now, out: &mut ExecOut);
    /// 策略本事件对该 group 下了单(`Ctx::place` 或链上意图):外来成交视为已处理;解除该 cell 的 `manual_hold`。
    pub fn note_strategy_action(&mut self, a: AccountId, g: GroupId, now: Now, out: &mut ExecOut);
    pub fn touch_inst(&mut self, inst: InstId);                               // Bbo / L2Batch / Mark 之后:引用该 inst 的活动 cell 待推进
    /// 只投递带 EXEC 标志的槽;v 与 advance 同一视图适配器(成交后按 legs 判定到达与补腿触发)。
    pub fn on_order_update<V: Views>(&mut self, u: &OrderUpdate, v: &V, now: Now, out: &mut ExecOut);
    pub fn on_link(&mut self, l: &LinkNotice, now: Now, out: &mut ExecOut);            // 链接状态与 RESUME_HOLD 变化(sequencer 从 OMS 输出投影)
    pub fn on_place_result(&mut self, id: ExecOrderId, r: PlaceResult, now: Now, out: &mut ExecOut); // 意图管线结果,同一事件内回填
    pub fn on_timer(&mut self, timer_id: u64, now: Now, out: &mut ExecOut);            // Timer{owner = Exec}
    /// 每一笔经 hft-ledger 入账的外来成交都调用,含不可归属的(`f.group = GROUP_NONE`,此时恒返回 0);f 即投递给策略的同一 SDK 视图。
    /// 返回默认处理的截止时刻(0 = 引擎不做默认处理,见"外来成交的默认处理"),sequencer 写进 f.deadline_tsc 后再投递给策略。
    /// v:同 advance 的视图适配器(manual_hold 的 Note 与默认处理判定读 legs 的 naked)。
    pub fn on_external<V: Views>(&mut self, a: AccountId, f: &ExternalFillView, v: &V, now: Now, out: &mut ExecOut) -> Tsc;
    pub fn on_leg_adjust(&mut self, a: AccountId, g: GroupId);                // FeeOnly 净量补差、AdoptExchange、WriteOffResidual 之后
    /// MarginStress 档位、HedgeBroken、CapSet/CapCleared(含 Broken 进出、HARD 解除:Broken 处置的计时;
    /// 该账户 Kill cap 的 CapCleared = Resume{Kill}:清 flatten_lock)。同一 Input 内 sequencer 先投递 EXEC 槽的
    /// `on_order_update`,再调本函数(顺序归 hft-sequencer),所以最后一片自动平仓的结束判定先于 `CapCleared{Broken}`。
    pub fn on_risk(&mut self, a: AccountId, e: &RiskEvent, now: Now);
    pub fn on_mode(&mut self, a: AccountId, g: GroupId, old: RunMode, new: RunMode, now: Now, out: &mut ExecOut);
    /// (a, v) 的恢复期放行条件在本次运行首次满足:hft-risk `hedge_gate_ok(a, v)` ∧ 该 venue 的 `RECONCILED` 位(sequencer 转调,
    /// 由 Input 驱动;报告方式归 hft-risk 与 hft-sequencer)。置 venue_open 的 v 位,该账户有腿在 v 上的 cell 重设截止
    /// (见"人工保持与恢复期抑制")。已置位时无动作。
    pub fn on_venue_gate_open(&mut self, a: AccountId, v: VenueId, now: Now);
    /// hft-risk 启动闸门对账户 a 放行(冷启动与恢复都调用;sequencer 转调 `ModeOutcome.released`):置 gate_open(闸门滞留告警停止);
    /// 尚未置位的 venue 一并按 on_venue_gate_open 处理(完整放行蕴含各 venue 的条件都已满足)。
    pub fn on_gate_open(&mut self, a: AccountId, now: Now);
    /// hft-risk 的 ForceDeleverage(L3 每 force_step_interval_ms 一次):把 (account, venue) 的永续敞口降 frac
    /// (含 Broken 的 cell 与无组永续持仓)。只登记请求并标记待推进;引擎目标在同一 Input 的 advance 里按视图设定。
    pub fn force_deleverage(&mut self, a: AccountId, venue: VenueId, frac: Rate, now: Now, out: &mut ExecOut);

    // ── 推进(exec_step)──
    /// 对全部待推进 cell 按 cix(= (account, group) 升序)推进一次,每个 O(腿数);ExecOut.orders 满则其余留待推进并登记 Continue。
    pub fn advance<V: Views>(&mut self, v: &V, now: Now, out: &mut ExecOut);   // sequencer 在每次调用本 crate 任一入口后立即排空 ExecOut;recovery_timers() 的条目与堆内已有定时器按键合并,保留较早者;推进顺序:实盘账户的 cell 先于 Sim 账户(与 sequencer 的派发顺序一致);reduce_only 的 StrategyCmd 期间设的目标由 set_target 的 ro 判定(见"目标的接受与替换")

    // ── 命令 ──
    /// HedgeFix 命令(control-plane):平掉多出的一腿;Broken 的人工处理方式、Reorg 的唯一处理方式。立即产出订单;解除该 cell 的 `manual_hold`。
    pub fn hedge_fix_cmd<V: Views>(&mut self, a: AccountId, g: GroupId, v: &V, now: Now,
                                   out: &mut ExecOut) -> Result<(), ExecCmdErr>;
    /// Kill{Account, flatten}:对该账户全部 cell(含 Broken)与无组永续持仓设引擎目标 0(reason = Hard,永续先),随后由 advance 推进;
    /// 置 AcctState.flatten_lock(直到该账户 Kill cap 的 CapCleared)。幂等。
    pub fn engine_flatten(&mut self, a: AccountId, now: Now, out: &mut ExecOut);

    // ── 只读 ──
    pub fn view(&self, a: AccountId, g: GroupId) -> Option<&TargetView>;   // SDK Views::target 的数据源
    pub fn has_pending(&self) -> bool;                                      // 有待推进 cell(sequencer 决定本 Input 是否调 advance)
    /// Drain 的执行层等待条件,sequencer 写作 `exec.drain_ready()`(Drain 的完整条件与 drain_timeout_ms 归 hft-sequencer):全部账户都满足 drain_ready_acct。
    pub fn drain_ready(&self) -> bool;
    /// 该账户无在途切片、无待发补腿/默认处理/自动平仓切片、无未到期的外来成交计时,且无未到达的 reason ∈ {MarginStress, Hard}
    /// 引擎目标(含无组 cell 与 HardAlign 对齐目标)。DrainTimeout 的 P0 用它列出未完成的账户。
    /// 自动有界平仓只有 phase = Running 时使其为假;Broken 中尚未开始的(Primed,含已到期而等待条件成立的,WaitHard、WaitHold)
    /// 不等:Broken 状态与 AutoFixState 进快照,重启后按"Broken 处置"的计时继续。
    pub fn drain_ready_acct(&self, a: AccountId) -> bool;
    /// 任一账户(含 Sim 账户)有未到达的 reason ∈ {MarginStress, Hard} 引擎目标(含无组 cell 与 HardAlign 对齐目标;
    /// 与 drain_ready_acct 中该项同一口径)。hft-risk `ModeView::engine_reduce_pending()` 的数据源(sequencer 的适配器转调;
    /// StorageCritical 的 Drain 推迟规则归 hft-risk"存储降级")。
    pub fn engine_reduce_pending(&self) -> bool;
    /// HARD 解除时 sequencer 对 oms.naked_groups 列出的每个 group 调用;为真即设 Broken{account, group}(判定见"HARD 解除时的 Broken 判定")。
    pub fn resolve_broken<V: Views>(&self, a: AccountId, g: GroupId, v: &V) -> bool;
    pub fn snapshot(&self, w: &mut SnapshotWriter);                       // 一次写本 crate 的全部节(节 21)

    // ── 恢复 ──
    /// 只装载快照(恢复、验证回放、advisor 镜像共用);不改任何 TSC 量。rd:重建 cell 表与 inst 缓存(同 on_groups_changed)。
    pub fn restore(r: &mut SnapshotReader, cfg: &ExecConfig, cap: &Capacities, groups: &GroupTable, rd: &RefView)
                   -> Result<Exec, RestoreError>;
    /// 冷启动恢复结束(只由恢复驱动调用:尾部重放之后、写 purpose = Recovery 的快照之前)。
    /// now = 本次运行的 TscClock 读数(与 Recovery 快照头同值);stop_tsc = 重放到的最后一条 Input 的 tsc(不重放尾部时 = 所选快照
    /// 所在记录的记录头 tsc;快照头的 snap_tsc 是测量字段,不用);
    /// hz_prev / hz_cur = 上次 / 本次运行的 tsc_hz_milli。全部 TSC 量按"停机时间不计"平移(见"快照与恢复");全部账户 gate_open = false、venue_open = 0;
    /// 有在途链接的 cell 置 LINK_HOLD(OMS 的 recovery_end 把全部 Open/Hedging 链接置 RESUME_HOLD,同一规则)。
    pub fn recovery_end(&mut self, now: Tsc, stop_tsc: Tsc, hz_prev: u64, hz_cur: u64);
    /// recovery_end 之后由 sequencer 逐条登记(命令路径,无分配,遍历 cell 表):每个 cell 每个 kind 至多一条。
    pub fn recovery_timers(&self) -> impl Iterator<Item = ExecTimerReq> + '_;
}

/// 链接状态变化(sequencer 从 OMS 的链接输出投影;字段全是 L0/SDK 类型)。
pub struct LinkNotice {
    pub link_id: u32, pub account: AccountId, pub group: GroupId, pub first: OrderRef,
    pub state: LinkPhase,             // Open | Hedging | Done | Broken
    pub broken: LinkBrokenCause,      // None | RetryExhausted | Deadline | HedgeLimit | Overfill | ReverseFailed | ResumeTimeout(值归 hft-oms LinkBroken)
    pub flags: u8,                    // RESUME_HOLD = 1(= hft-oms LinkUpdate.flags):OMS 清除该位时产出一条不含它的通知
    pub first_net: Qty, pub hedge_filled: Qty, pub owed: Qty,
    pub tolerance: Rate, pub residual: ResidualPolicy,   // 该链接 LegLink.tolerance 与 HedgePlan.residual(补腿阈值与残余策略的来源)
}
```

`Views` 是 SDK 的只读视图 trait(泛型,静态分派)。sequencer 传给 `advance` 的视图适配器**不含执行层自身**(`target()` 返回 `None`),避免借用重叠。执行层只读:`bbo`(含连接健康字段 `conn_live_tsc`、`resync`,见"报价可用")、`book`、`mark`、`inst`、`caps`、`group_pos`(各腿净量,也用于求无组永续持仓)、`legs`(`LegView`,数据由 OMS LegTracker 维护;在途分 `inflight_immediate` 与 `inflight_resting`)、`order`、`portfolio`(现货库存与可用余额、账户永续持仓 `perp(v, i)`)、`mode`、`venue_ready(a, v)`(该 (venue, account) 无 Unknown 槽且下单连接 Up,值取 hft-oms `venue_ready`)、`risk`(HARD、MarginStress 档位、Broken/Reorg cap、单笔名义上限 `max_order_notional`/`max_reduce_notional`/`max_hedge_notional`、`conn_liveness_ms`)、`risk_for(a, i)`(SDK `InstRiskView`:该 inst 所属价格带类别的 `rr_max_cross`、`ioc_max_cross`,可选的 L3 越价上限 `l3_max_cross`,该 inst 计价资产的 USDT 换算率 `quote_rate`(与检查链 #6 同值同源),该 (venue, account) 是否处于限速退避 `rate_backoff`)。视图的类型与访问器归 [SDK](../hft-strategy-sdk/README.md),取值归各写者(hft-risk、hft-oms、hft-ledger)。

### 输出 `ExecOut`(容量固定,sequencer 预分配;**每次调用本 crate 之后排空**)

```rust
pub struct ExecOut {
    pub orders:    FixedVec<ExecOrder, 8>,     // → sequencer 意图管线(见"调用顺序");结果经 on_place_result 回填
    pub cancels:   FixedVec<OrderRef, 8>,      // 目标被替换/清除时撤 EXEC 挂单(MakerFirst)
    pub cancel_resting: FixedVec<CancelResting, 8>, // → sequencer 调 oms.cancel_resting_on(account, inst):设 MarginStress/Hard 引擎目标时
                                               //   撤该 inst 上本账户全部非 EXEC 挂单(见"保证金危急的默认减仓"第 1 条)
    pub decisions: FixedVec<ExecDecision, 16>, // → seq journal(Output)
    pub notify:    FixedVec<ExecNotify, 8>,    // → 拥有者策略 EventRef::Exec
    pub broken:    FixedVec<BrokenReq, 4>,     // → risk.mode.apply(Auto(Broken{account, group}))
    pub timers:    FixedVec<ExecTimerReq, 8>,  // 登记/撤销 Timer{owner = Exec}
    pub notes:     FixedVec<Note, 8>,          // source = Exec
}

pub struct CancelResting { pub account: AccountId, pub inst: InstId }
pub struct ExecOrder {
    pub id: ExecOrderId,          // { cix: u16, n: u16 }:cell 下标 + cell 内单调序号,回填与 ExecDecision 关联用
    pub intent: OrderIntent,      // SDK 类型;PairOnFill/Simultaneous/MakerFirst 的第一腿带 LegLink
    pub origin: ExecOrigin,       // Target(为策略目标服务)| Engine(补腿、残余修剪、外来成交默认处理、自动有界平仓、默认减仓、命令、Kill 清仓;
                                  //   账户 force_active/flatten_lock 时策略目标的订单也是 Engine,见"增仓与减仓的分类")
    pub kind: ExecOrderKind,      // 码值见"ExecDecision.detail 打包与子码"的 ExecOrderKind 表;sequencer 原样写进 OrderDecision.reason
    pub reason: TargetReason,     // 来自目标(引擎目标即 MarginStress | Hard);补腿类订单取 cell 当前目标的 reason,无目标时与
                                  //   hedge_fix_cmd、自动有界平仓为 Manual
    pub hedge_cap_qty: Qty,       // Hedge 类订单的数量上限(见"对冲量上限"),sequencer 填进 IntentView;其余为 0
}
pub enum PlaceResult { Placed(OrderRef), Rejected(RejectBits), Shadowed }
// 目标拒绝原因用 SDK 的 TargetReject(本 crate 定义各值的判定,见"目标的接受与替换");Stuck/Broken 原因用 SDK 的 StuckReason/BrokenReason;
// 线上码与 ExecDecision.detail 的打包见"ExecDecision.detail 打包与子码"。
```

**输出容量与续推**(固定容量从不丢弃输出):各向量的容量是**单次调用**的上限,sequencer 在每次调用后排空(Output 追加、订单进意图管线、通知入队),所以与一个输入事件内的调用次数无关。

`FixedVec::as_mut_slice` 只暴露已有元素,不改变长度或容量;sequencer 原地补决策标志,排空时直接读取预分配向量,不另建临时副本。

| 入口 | 单次产出上限 |
|---|---|
| 单 cell 入口:`set_target`、`clear_target`、`on_external`、`hedge_fix_cmd`、`on_place_result`、`on_order_update`、`on_link`、`on_timer`、`on_mode`、`note_strategy_action` | decisions ≤ 2、notify ≤ 1、orders ≤ 1、cancels ≤ 4、cancel_resting 0、broken ≤ 1、timers ≤ 2、notes ≤ 2 |
| 涉及多个 cell 的登记:`engine_flatten`、`force_deleverage`、`clear_account`、`on_risk`、`on_gate_open`、`on_venue_gate_open`、`on_groups_changed` | **不直接写输出**:只改 cell 状态、置 cell 的 `deferred` 位(待写的 `ExecDecision`、通知与按新时刻重登的定时器)并标记待推进;对应记录在 `advance` 处理该 cell 时写出 |
| `advance` | 处理每个 cell 前检查各向量剩余容量 ≥ 单 cell 上限(orders 4、cancels 4、cancel_resting 4、decisions 6、notify 2、broken 1、timers 3、notes 3);不足即停止,其余 cell 保持待推进并登记 `Timer{Exec, Continue}`,在后续输入中继续。同一次 `advance` 内同一 (account, inst) 的 `cancel_resting` 只写一条 |
| `resolve_broken`、`drain_ready`、`drain_ready_acct`、`engine_reduce_pending` | 只读,不写输出 |

### 执行规格参数(类型定义在 SDK,本节定义语义)

`TargetIntent`、`LegTarget`、`ExecSpec` 及其参数结构由 [hft-strategy-sdk](../hft-strategy-sdk/README.md) 定义;执行层读取下列字段,含义以本节为准。数量为资产定点 `Qty`,比率为 `Rate`(1e-9),金额为 `Notional`(USDT 1e-6)。目标只能引用交易所腿(现货、永续);含链上腿(optional)的目标 → `TargetReject::SpecInvalid`(链上腿用 SDK 的链上意图直接下单)。

| 结构 | 字段 | 含义 |
|---|---|---|
| `TargetIntent` | `legs[i].qty` | 该腿目标净量(带符号;现货腿 < 0 → `TargetReject::SpotShort`) |
| | `reason` | `Rebalance` \| `Exit` \| `MarginStress` \| `Hard` \| `Manual`;决定腿顺序与减仓分类。`Exit` = 策略的强制退出(如单 group 强制退出参数或只减仓命令),执行层按普通减仓目标执行,在 L3 默认减仓期与 `flatten_lock` 期间作为"更接近 0 的目标"被接受;成本门等收益判断是策略侧规则,执行层不按 `reason` 做任何成本判断 |
| | `urgency` | `Passive` \| `Normal` \| `Urgent`(见"不交易带与紧急度") |
| | `band` | 不交易带:`|target − pos| / max(|target|, floor) ≤ band` 时该腿不动;`floor = band_floor_notional / mark` |
| | `exec` | 算法与参数(下列各行) |
| | `tag` | 原样带进每笔订单的 `OrderIntent.tag`、`ExecDecision` 与 `ExecUpdate` |
| `TakerSliceParams` | `depth_frac` | 单笔占弱侧可见深度的比例 |
| | `depth_levels` | 计入可见深度的档数上限(1–10);深度由本 crate 读 `v.book(inst)`,与策略快速部分是否订阅 L2 事件无关:该 inst 的簿由任一订阅了其深度的部分(含 Model)维持,没有 L2 簿或簿非 `Live` 时只用 BBO 顶档 |
| | `max_cross` | IOC 越价参数:执行时按该 inst 的风控越价上限截取(不拒绝目标;截取规则见"限价与单笔名义"第 1 条),限价由 `cap_cross_px` 给出;同时是可见深度的价格范围 |
| | `max_chunk_notional` | 单笔名义上限:执行时截取为 min(`max_chunk_notional`, `max_order_notional`)(不拒绝目标);配对增仓目标的 `LotMismatch` 阈值(见"两腿 lot 对齐") |
| | `min_chunk_notional` | 单笔名义下限(低于它不发,除非是本目标的最后一笔且 ≥ 交易所最小名义) |
| | `cooldown_ms` | 同一 cell 两笔切片的最小间隔 |
| `PairParams` | `slice` | 第一腿的 `TakerSliceParams` |
| | `first_leg` | `ByLiquidity` \| `PerpFirst` \| `SpotFirst` \| `Fixed`(先手腿号取 `first_leg_ix`;默认 `ByLiquidity`;MarginStress/Hard 下被覆盖,见"腿顺序") |
| | `hedge_px_mode`、`hedge_cross`、`hedge_max_slip` | 写进 `HedgePlan` 的 `px_mode`、`px_param`、`max_slip`(第二腿定价规则归 [hft-oms 对冲定价](../hft-oms/README.md#对冲定价)) |
| | `ratio_num`/`ratio_den` | 第二腿量 = 第一腿成交净量 × num/den(同资产默认 1/1);参与 `pair_lot` 的计算 |
| | `fix_tolerance` | 写进 `LegLink.tolerance`:该 group 的补腿阈值 |
| | `residual` | `Defer` \| `TrimExcess` \| `Hold`(见"残余与尘埃") |
| `SimulParams` | `slice`、`on_fail`(`Reverse` \| `Hold`)、`fix_tolerance`、`residual` | 同上 |
| `MakerParams`(optional) | `offset`、`requote`、`max_rest_ms`、`chunk_notional`、`hedge: PairParams` | 见 MakerFirst |

### 配置项(`ExecConfig`,`EngineConfig` blob 的 `exec` 段;每账户一行 + 全局默认行;变更 = Drain + 重启)

| 名称 | 类型 | 默认 | 含义 |
|---|---|---|---|
| `band_floor_notional` | Notional | 20 USDT | 不交易带分母下限 |
| `dust_notional` | Notional | 5 USDT(待标定) | 尘埃线(唯一定义):**已成交**名义 ≤ 它即为尘埃——不动作、不阻塞 Done、不触发补腿;也是自动有界平仓的成功判定线与 hft-risk 条件型解除 Broken 的同一口径 |
| `fix_tolerance_default` | Rate | 0.02(待标定) | group 从未由链接/目标给出阈值时的补腿阈值 |
| `fix_max` | u8 | 3 | 自动补腿执行失败计次上限(门控拒绝不计次) |
| `fix_cooldown_ms` | u32 | 1 000 | 两次自动补腿的最小间隔 |
| `fix_deadline_ms` | u32 | 10 000 | 从首次满足补腿条件起的截止(恢复期与 `manual_hold` 期间不计,见"人工保持与恢复期抑制");超时 → Broken + P0。自动有界平仓也以它为一次尝试的**可执行时长**上限(见"Broken 处置") |
| `broken_auto_fix_ms` | u32 | 120 000 | cell 处于 Broken 持续此时长且期间无人工 `HedgeFix` → 一次可执行的自动有界平仓尝试,平掉多出的一腿(计时规则见"Broken 处置") |
| `external_deadline_ms` | u32 | 5 000(待标定) | 外来成交后等待策略处理的时长 |
| `gate_naked_alarm_ms` | u32 | 300 000 | 闸门滞留与自动平仓等待的告警阈值:`Note{ExecGateHeldNaked}`(P0,见"人工保持与恢复期抑制") |
| `stuck_after_ms` | u32 | 2 000 | 同一原因持续不能推进多久报 `Stuck` |
| `gated_backoff_ms` | u32 | 500 | 门控拒绝后同一 cell 再次尝试的最小间隔 |
| `max_slice_fails` | u8 | 3 | 连续执行失败(交易所拒单)→ `Stuck{SliceFails}` |
| `max_nofill` | u8 | 5 | 连续 IOC 零成交 → `Stuck{NoLiquidity}` |
| `urgent_cooldown_ms` | u32 | 100 | `Urgent` 时切片间隔取 min(参数, 此值) |
| `passive_cooldown_mult` | u8 | 4 | `Passive` 时切片间隔与 Stuck 判定时长的倍数 |
| `force_slice` | TakerSliceParams | depth_frac 0.3、depth_levels 5、max_cross 0.5%、max_chunk_notional 1 000 USDT、min_chunk_notional 0、cooldown 200 ms(待标定) | 默认减仓、外来成交默认处理、补腿、自动有界平仓、无组永续持仓减仓使用的切片参数;须满足下面的风控一致性约束 |


ECF1 段 7 的行版本为 1,`ExecConfigRow` 为 176 B 小端。前 8 B 为 `{account:u8, sim:u8, has_params:u8, reserved:[u8;5]}`,其后为 21 个 i64 槽。前 15 槽依照本表从 band_floor_notional 到 passive_cooldown_mult 的顺序,后 6 槽为 force_slice 的 depth_frac、depth_levels、max_cross、max_chunk_notional、min_chunk_notional、cooldown_ms。窄整数必须在原类型范围内;sim/has_params 只取 0/1,填充为 0。account=0 必须恰有一行(sim=0、has_params=1),保存全局默认;其余行逐账户递增且不重复,has_params=0 的参数槽全为 0。`to_rows` / `from_rows` 是唯一行编解码器;`from_engine` 还核对 refdata 的账户全集及 Sim 身份。全部为冷路径,配置的业务范围由 `Exec::new` 继续校验。

**与风控参数的一致性约束**(每个账户逐行检查):`force_slice.max_cross ≤ rr_max_cross`(该账户适用的各价格带类别取最小)且 `force_slice.max_chunk_notional ≤ max_order_notional`(该账户 `AcctLimits`)。hft-core 装配时不符 → 退出 2(`ExecRiskMismatch`,[apps/hft-core](../../apps/hft-core/README.md));运行中 `SetRisk` 使其不符 → `InvalidArgs{ExecRiskMismatch}`([control-plane](../../docs/subsystems/control-plane.md#结果码)),旧参数继续生效。策略目标里的切片参数不受此约束,执行时截取(见"限价与单笔名义")。

**尘埃线与最小名义**:每个账户的 `dust_notional` 必须 ≥ 该账户交易腿(refdata 中该账户各 venue 被 GroupSpec 引用的 inst)`min_notional` 的最大值,使"高于尘埃线却低于最小名义"的已成交裸量不会出现在单腿上;校验在 hft-core 装配期,不符退出 2(`DustBelowMinNotional`,归 [apps/hft-core](../../apps/hft-core/README.md))。这条校验不覆盖 `min_qty`/lot × 价格:最小下单量可能大于最小名义,所以"高于尘埃线却低于最小下单量"的残余仍可能出现(还有两腿 lot 零头、价格变化后低于最小名义),由 `WriteOffResidual` 命令核销(见"残余与尘埃")。

容量不在本表:cell 总数是 `Capacities.max_cells_total`,一个 inst 最多被多少 cell 引用是 `Capacities.max_subs_per_inst`(都在 EngineConfig 的 `capacities` 段,即 ECF1 段 3,字段与语义归 [hft-sequencer](../hft-sequencer/README.md),各主机类的取值唯一归属 [apps/hft-core 容量表](../../apps/hft-core/README.md#内存与-journal-段按主机类唯一归属)),与 SDK 的每实例上限 `max_groups_per_instance` 是不同的量;无组 cell(每账户一个,见"保证金危急的默认减仓")另占账户数个下标,不计入 `max_cells_total`。单笔名义的硬上限也不在本表:Open 类 `max_order_notional`、Hedge 类 `max_hedge_notional`、Reduce/Flatten 类 `max_reduce_notional`、按 inst 价格带类别的越价上限 `rr_max_cross`/`ioc_max_cross` 与可选的 `l3_max_cross` 归 [hft-risk `RiskParams`](../hft-risk/README.md),执行层从 `Views::risk`/`Views::risk_for` 读取并按"限价与单笔名义"把每笔订单切到其下(风控不替执行层拆单)。报价可用的阈值 `conn_liveness_ms`(默认 2 000)同样不在本表:唯一配置位置是 hft-risk `RiskParams`(与检查链 #5、OMS `price_hedge` 同值同源),执行层从 `Views::risk` 读取。

### 消费与产生的事件

| 方向 | 事件 | 用途 |
|---|---|---|
| 消费(经 sequencer 调用) | 行情 `Bbo`/`L2Batch`/`MarkPrice` | `touch_inst`:推进引用该 inst 的活动 cell |
| 消费 | `OrderEvent`(经 OMS 迁移后的 SDK `OrderUpdate`)、链接状态(`LinkNotice`) | 切片结果、链接终态 |
| 消费 | `ExternalFill`(hft-ledger 入账后的 SDK `ExternalFillView`,含不可归属的 `GROUP_NONE`) | 外来成交默认处理计时(截止时刻回填进视图)、`manual_hold` 置位、清仓锁下的无组持仓重扫 |
| 消费 | `FeeOnly` 的净量补差、`AdoptExchange` | `on_leg_adjust`:重查补腿 |
| 消费 | `RiskEvent`(`MarginStress`、`HedgeBroken`、`CapSet`/`CapCleared`)、有效模式变化 | Broken 进出与自动有界平仓计时、默认减仓期间的约束、门控解除后立即重试、该账户 Kill cap 解除(`Resume{Kill}`)时清 `flatten_lock` |
| 消费 | hft-risk 的 `ForceDeleverage`(随 `MarginReport` 或 `Timer{Risk, MarginForce}`) | 默认减仓一步(含无组永续持仓) |
| 消费 | venue 的恢复期放行(`on_venue_gate_open`)、账户启动闸门放行(`on_gate_open`)(sequencer 转调) | 解除该 venue 的恢复期抑制并重设截止;账户放行后不再有闸门滞留告警 |
| 被调用 | HARD 解除时的 `resolve_broken`(sequencer 对 `oms.naked_groups` 的候选逐个调用) | 判定哪些 group 设 Broken |
| 消费 | `Timer{owner = Exec}` | 冷却、退避、补腿冷却、外来成交截止、自动有界平仓到期、续推、告警检查 |
| 消费 | `ControlCmd{HedgeFix}`、`Kill{flatten}`(sequencer 转调);`WriteOffResidual` 之后的 `on_leg_adjust` | 命令补腿、账户清仓、核销后重查 |
| 产生 | `ExecDecision`(Output) | 每个执行决定一条(见"ExecDecision 与进度通知") |
| 产生 | 订单请求(→ `OrderDecision` 由 OMS 写,`reason` = `ExecOrderKind` 码)、撤挂单请求(`cancel_resting` → OMS `cancel_resting_on`) | 本 crate 不写 `OrderDecision` |
| 产生 | `Note{source = Exec}` | `ExecBroken`(P0)、`ExecExternalDefault`(按条件定级:对多出的一腿进入默认处理时 P0,裸量在阈值内不发单时 P1,见"外来成交的默认处理"第 3 条)、`ExecChainLegExcess`(P0)、`ExecAutoFixFailed`(P0)、`ExecSerialFallback`(P0)、`ExecEngineOrderRejected`(P0)、`ExecUngroupedReduce`(P0)、`ExecGateHeldNaked`(P0)、`ExecAutoFixDone`(P1)、`ExecStuck`(P1)、`ExecForceStep`(P1)、`ExecManualExternal`(P1)、`ExecManualHold`(置位 P1,解除时实际级别 Info)(码名与默认级别由 hft-event 登记,数值 `n` 在本 crate 的 `const` 表,`v[]` 含义见各节) |
| 产生 | `EventRef::Exec{gix, u: &ExecUpdate}` 给拥有者策略 | `Progress` \| `Done` \| `Stuck(reason)` \| `Broken(reason)` |

### 调用顺序(sequencer 同一输入事件内;流程归属 [hft-sequencer](../hft-sequencer/README.md))

1. Input 自身的处理(OMS 迁移与对冲快路径、ledger、风控)期间,sequencer 调用本 crate 的登记函数(`on_order_update`、`on_link`、`on_external`、`on_leg_adjust`、`on_risk`、`on_venue_gate_open`、`on_gate_open`、`force_deleverage`、行情的 `touch_inst`、定时器的 `on_timer`)。同一 Input 内 EXEC 槽的 `on_order_update` 先于该 Input 产生的 `on_risk`(本 crate 依赖这个顺序判定自动平仓的最后一片,顺序归 hft-sequencer)。
2. 策略 `Ctx` 应用:`target`/`clear_target` → `set_target`/`clear_target`;`place` → 意图管线并调 `note_strategy_action`。
3. `has_pending()` 为真 → `advance`:按 (account, group) 升序推进全部待推进 cell;`ExecDecision` 先追加,`ExecOut.cancel_resting` 逐条调 `oms.cancel_resting_on`,`ExecOut.orders` 再逐条进意图管线:`origin = Target` 的订单过**模式门**(与策略意图同一行为表:Shadow 只记录;CloseOnly 拒增仓、放行减仓),`origin = Engine` 的不经模式门、只受 HARD 约束 → `classify`(本 crate 给的 `class` 只是提示)→ `evaluate` → `commit` → `oms.place` → `on_place_result`。
4. `ExecOut.broken` → `risk.mode.apply(Auto(Broken))`;`notify` 在同一 Input 的二次投递中交给拥有者策略。以上各步中,`ExecOut` 在每次调用本 crate 之后排空(见"输出容量与续推");Output 记录的顺序即调用顺序,回放确定。二次投递中策略给出的目标只登记,推进留给下一个相关 Input(`set_target` 接受新目标或替换目标时(逐字段相同的不算)一律登记立即到期的 `Timer{Exec, Wake}`,不区分哪一次投递;首次投递中给出的目标在同一 Input 的 `advance` 里推进过,该定时器按"定时器"的早到规则只重新登记)。

## 实现说明

<details><summary>实现要点</summary>

### 目标单元(`TargetCell`)

每 (account, group) 一个,数组下标 `cix: u16` 按 (account, group) 升序分配(一个账户只绑定一个策略实例,所以 (account, group) 唯一确定拥有者);其后每账户一个**无组 cell**(`group = GROUP_NONE`,只承载 L3/清仓对无组永续持仓的引擎目标,见"保证金危急的默认减仓")。字段按语义列出,字节布局为草案,以代码与 `const` 断言为准(目标 ≤ 640 B):

```rust
pub struct TargetCell {
    // ── 定义(on_groups_changed 写)──
    pub account: AccountId, pub group: GroupId, pub owner: StrategyInstanceId,
    pub n_legs: u8, pub legs: [CellLeg; 4],      // CellLeg { venue, inst, kind: Spot | LinearPerp | Chain, lot, min_qty, reduce_only_cap }(min_notional 读 inst 行)
    // ── 当前目标 ──
    pub has_target: bool, pub target: TargetIntent, pub origin: TargetOrigin, // Strategy | Engine(MarginStress | External | Command | KillFlatten)
                                                 // (AutoFix 不是目标来源,只出现在自动有界平仓的 ExecDecision/ExecUpdate 上,见"ExecDecision 与进度通知")
    pub set_seq: u64, pub set_tsc: Tsc,
    pub pair_lot: Qty,                           // 配对算法的第一腿取整单位(见"两腿 lot 对齐");非配对目标 = 0。不进快照,restore 时重算
    pub deferred: u8,                            // 待写的决定与通知种类位(ENGINE_SET=1 | ENGINE_CLEARED=2 | CLEARED_BY_CMD=4 | STATE=8 | TIMERS=16 | ALARM=32
                                                 // | HOLD_REL=64 待写 manual_hold 解除 Note | EXT_CLR=128 待写 External/0/Cleared),advance 写出后清零(见"输出容量与续推")
    // ── 进度 ──
    pub state: CellState,                        // Idle | Working | Stuck | Broken;判别值取 hft-event TargetState 线上码(不另设码表)
    pub stuck: StuckReason, pub broken: BrokenReason, pub gated_bits: RejectBits,
    pub filled: [Qty; 4],                        // 普通 cell:本目标以来各腿成交净量(带符号)。无组 cell 不投递 ExecUpdate、不需要它,
                                                 // 同一存储用作 own_infl:本 cell 非终态切片的带符号剩余量(含 Unknown 槽;无 LegTracker,由 on_order_update 维护)
    pub active: ActiveSlice,                     // { kind, leg_ix, first: OrderRef, link_id, planned: Qty, n: u16 };kind = None 即无在途
    pub last_slice_tsc: Tsc, pub wake_at: Tsc, pub gated_since: Tsc, pub stuck_since: Tsc,
    pub fail_streak: u8, pub nofill_streak: u8, pub fund_shrink: u8, pub n_slices: u32,
    // ── 补腿 / 外来成交 / 默认减仓 ──
    pub fix: FixState,                           // { count: u8, first_tsc: Tsc, next_tsc: Tsc }
    pub ext_deadline: Tsc,                       // 0 = 无待处理外来成交
    pub force_steps: u16,
    pub tolerance: Rate,                         // 补腿阈值:最近 LinkNotice.tolerance 或目标的 fix_tolerance;0 = 用 fix_tolerance_default
    pub cap_block: u8,                           // BROKEN | REORG:来自 on_risk 的 CapSet/CapCleared(group 作用域)
    pub serial: u8,                              // SERIAL_FALLBACK = 1:非 Broken cell 的 L3/清仓已退回串行腿(见"保证金危急的默认减仓");引擎目标到达或清除时清零
    pub hold: u8,                                // MANUAL_HOLD = 1 | LINK_HOLD = 2(在途链接处于 OMS RESUME_HOLD)| MANUAL_HOLD_DEFERRED = 4(引擎目标期间到达的
                                                 // Manual 成交,引擎目标清除时转为 MANUAL_HOLD;见"人工保持与恢复期抑制")
    // ── Broken 处置 ──
    pub broken_since: Tsc,                       // 进入 Broken 的时刻(0 = 不在 Broken)
    pub auto_fix: AutoFixState,                  // 见"Broken 处置"
    pub slot: [u32; 4],                          // 无组 cell:各腿被跟踪切片的 OMS 槽号(区分已释放的 Unknown 单与该腿的新切片;
                                                 //   进快照:LegRef.slot)
}
#[repr(C)]
pub struct AutoFixState {                        // 56 B(布局为草案,以代码与编译期断言为准)
    pub phase: u8,                               //  0 AutoFixPhase:None=0 Primed=1 WaitHard=2 Running=3 Done=4 Failed=5 Cancelled=6 WaitHold=7
    pub leg_ix: u8,                              //  1
    pub n_slices: u8,                            //  2
    pub fail: u8,                                //  3 AutoFixFail 码(见"Broken 处置");0 = 无
    pub start_sign: i8,                          //  4 开始时 naked_sum 的符号(+1 / −1)
    pub _pad: [u8; 3],                           //  5
    pub exec_ms: u32,                            //  8 本次尝试已累计的可执行时长(ms)
    pub _pad2: u32,                              // 12
    pub due_tsc: Tsc,                            // 16 到期时刻 = broken_since(或重给窗口的时刻,见"Broken 处置"计时)+ broken_auto_fix_ms
    pub started_tsc: Tsc,                        // 24
    pub run_mark_tsc: Tsc,                       // 32 可执行时长的起算点;0 = 当前处于等待(不累计)
    pub budget: Qty,                             // 40 开始时的总量上限 = floor_to_lot(开始时的已成交裸量 naked_filled),此后不增加
    pub done_qty: Qty,                           // 48 已成交量
}
pub struct CellIndex {
    by_key: Box<[(AccountId, GroupId, u16)]>,    // 有序;set_target 等按 (account, group) 二分查 cix
    inst_cells: Csr<InstId, u16>,                // inst → 引用它的 cix(≤ max_subs_per_inst)
    live: BitSet,                                // 有目标、非 Idle、外来成交计时中、默认减仓中或 Broken 处置中的 cell;touch_inst 只标这些
    pending: BitSet,                             // 待推进;advance 按位升序遍历(= (account, group) 升序)
}
pub struct AcctState {                           // 每账户
    pub force_active: bool,                      // 处于默认减仓期(由 force_deleverage 置位,MarginStress 降到 L3 以下清除)
    pub force_req: ForceReq,                     // 待设定的引擎目标:{ venues: VenueSet, frac: Rate } 或清仓;force_deleverage/engine_flatten 写,advance 设定后清零
    pub flatten_lock: bool,                      // 清仓锁(engine_flatten 置位;该账户 Kill cap 的 CapCleared,即 Resume{Kill},清除)。
                                                 // 权威状态是 hft-risk 该账户 Kill cap 的 flatten 位,本字段是执行层的镜像;进快照
    pub gate_open: bool,                         // 账户启动闸门已放行(on_gate_open 置位;new 与 recovery_end 清零);为 false 时才有闸门滞留告警
    pub venue_open: u64,                         // VenueId 位图:恢复期放行条件已满足的 venue(on_venue_gate_open / on_gate_open 置位;new 与 recovery_end 清零);
                                                 // 多出那一腿所在 venue 无此位时自动处置受抑制,见"人工保持与恢复期抑制"
    pub gate_closed_tsc: Tsc,                    // 本次闸门关闭的起点(recovery_end 置为 now;new 时为 0),闸门滞留告警的起算下限
}                                                // 后三个字段进快照(节 21 的 Gate 行)
```

- **已成、在途、残余**:`filled` 由本 crate 记;各腿净量 `leg[i].net`、即时在途 `leg[i].inflight_immediate`、挂单在途 `leg[i].inflight_resting`、残余 `leg[i].residual`、naked 读 [OMS LegTracker](../hft-oms/README.md#legtracker)(`Views::legs`;两类在途按槽状态的划分唯一定义在那里),不复制。本 crate 的"在途"一律指 `inflight_immediate`:挂单不算对冲覆盖、不算推进中的切片(策略在目标管理的 group 上自挂的 resting 单,成交后由下一次推进按新差值处理)。`TargetView`(SDK 类型)由 cell 投影,`view()` 返回其引用。无组 cell 没有 LegTracker:仓位取 hft-ledger(见"保证金危急的默认减仓"),在途取本 cell 的 `own_infl`。
- 容量:cell 数 = `Capacities.max_cells_total`(取值见 [apps/hft-core 容量表](../../apps/hft-core/README.md#内存与-journal-段按主机类唯一归属))+ 账户数个无组 cell;全部在 `new` 时分配。
- 不变量(属性测试逐条检查):**新发的** `origin = Target` 切片只在 `state ∈ {Working, Stuck}` 时产生;`state == Broken` 时只产生引擎目标的订单、补腿类订单(`AutoFix` 等)与 `CmdHedgeFix`(进入 Idle/Broken 时已在途的 IOC 不撤,`active` 保留到其终态);`state == Idle ⇒ !has_target ∨ 全部腿在带内`;`state == Broken ⇒ 不产生 origin = Target 的订单`;`hold & MANUAL_HOLD` 的 cell 不产生 `HedgeFix`/`ResidualTrim`/`ExternalDefault`/`AutoFix` 订单,这四类订单也不落在 `venue_open` 无该位的 venue 上;Engine 订单从不增加任何腿的 |仓位|;任何一笔订单不穿越零(含 Unknown 切片按最坏情况全部成交);本 crate 从不对链上腿产生订单;每个 Broken 期 `AutoFix` 至多执行一次,其成交总量 ≤ `budget`;`flatten_lock` 账户上任何被接受的目标每腿 |目标| ≤ 接受时的参照量(见"目标的接受与替换"),因此该账户没有增仓切片;配对算法的第一腿订单量是 `pair_lot` 的整数倍(按较粗 lot 取整的减仓目标除外,见"两腿 lot 对齐");`serial ≠ 0` 的 cell 不产生带配对链接(`role = First`)的订单(补腿类订单的 `role = HedgeFix` 单笔链接不算);每笔订单的限价是 `cap_cross_px` 的结果、`qty × 限价` 折 USDT 后 ≤ 本笔的单笔名义上限(见"限价与单笔名义");无组 cell 只产生 reduce-only 的永续 `Flatten` 单。

### 状态机

```
                 set_target(有腿在带外)                 全部腿在带内且无在途
    Idle ─────────────────────────────▶ Working ──────────────────────────────▶ Idle(Done)
     ▲   clear_target / 被替换为带内目标     │  ▲
     │                                      │  │ 原因消失(下一次推进)
     │   不能推进 ≥ stuck_after_ms,或即时原因 ▼  │
     │   (Shadow、External、EngineOverride、SliceFails)
     │                                    Stuck
     │                                      │
     │  RiskEvent::CapCleared{Broken}       │ 任一状态:链接 Broken(含 HEDGE_LIMIT、续接超时)、
     │  (hft-risk 判定已成交裸量 ≤ 尘埃线    ▼ 补腿计次/截止耗尽、Simultaneous 反向失败、
     │   且无即时在途/Pending*/Unknown:       Broken cap 被设置(含 HARD 解除时 resolve_broken 判定的 group)
     │   自动有界平仓成功、HedgeFix 命令、
     │   L3/flatten 减到尘埃线以下、核销残余;
     │   有引擎目标时回 Working 继续推进它)
     └──────────────────────────────────── Broken ── broken_auto_fix_ms ──▶ 自动有界平仓尝试(仍在 Broken 内)
```

- `Stuck` 不是终态:每次相关事件重新推进,原因消失即回 `Working`;`SliceFails` 只在目标被替换或清除时解除(逐字段相同的目标不算替换,见"目标的接受与替换")。
- `Broken`:不发 Target 订单、不做自动补腿与残余修剪;策略目标保留但冻结。期间只产生三类引擎订单:自动有界平仓(`AutoFix`)、L3 默认减仓与 `engine_flatten` 的引擎目标、`hedge_fix_cmd`。只有 `RiskEvent::CapCleared{Broken}` 让 cell 离开 `Broken`:**只清策略目标**(通知策略 `Done{cleared_by_engine: true}`);cell 有引擎目标(L3、清仓、对齐目标)时保留它、回到 `Working` 继续推进,否则回到 `Idle`。账户级 `HedgeFailure` cap 仍须人工 `Resume{HedgeFailure}`,且只在该账户已无 Broken cell 时被接受(否则 `BrokenRemaining`,前置条件归 [hft-risk](../hft-risk/README.md))。

### 推进(`advance` 中每个 cell,O(n_legs) + 每腿 ≤ `depth_levels` 档)

0. 该账户 `force_req` 非空 → 先按"保证金危急的默认减仓"设定引擎目标(含无组 cell),清 `force_req`。
1. `state == Broken` 且无引擎目标、`auto_fix.phase ∉ {Primed 且已到期, Running}` → 跳过;或该 cell 不是因成交/链接/模式/风控/新目标被标记且 `now < wake_at` → 跳过(到期由定时器重新标记)。Broken 的 cell 走"Broken 处置"一节的步骤,不走下面 2–8。
2. 每腿读 `pos_i = legs.leg[i].net`、`infl_i = legs.leg[i].inflight_immediate`(无组 cell:ledger 仓位与 `own_infl[i]`);`eff_i = pos_i + infl_i`;`diff_i = target_i − eff_i`。
3. **完成判定**:每腿 `reached_i = |target_i − pos_i| ≤ band × max(|target_i|, floor_i)`;`target_i == 0` 时改为 `|pos_i| × mark ≤ dust_notional`。全部 `reached` 且全部 `infl_i == 0` → `Done`:`state = Idle`、`ExecDecision{State, → Idle}`、通知 `Done`、`serial` 清零;随后按"残余与尘埃"检查一次。
4. 全部 `|diff_i|` 在带内(目标为 0 的腿按尘埃线,见"不交易带与紧急度")但仍有在途 → 等待。`active.kind ≠ None`(配对算法每 cell 同时只有一笔在途;`TakerSlice` 与串行腿每腿一笔)→ 等待。例外(只用于 `reason ∈ {MarginStress, Hard}` 引擎目标的串行腿、单腿 `TakerSlice` 与无组 cell;配对第一腿带链接,仍等待):永续腿的在途切片进入 `Unknown`、且该 venue `VenueCaps.reduce_only` 支持时,该腿不等待——`active` 释放该切片(其剩余量仍计在 `inflight_immediate`/`own_infl` 里),下一片数量 ≤ `|pos_i| − |该腿同向即时在途|`(Unknown 剩余按全部成交计),所以即使 Unknown 的单全部成交也不穿越零;现货腿与不支持 reduceOnly 的腿照旧等待收敛。无组 cell 每腿至多一笔已释放的 Unknown 切片(该腿已有释放剩余时,新的 Unknown 切片照旧等待),于是那一笔的终态恰好结清该腿的释放剩余;更新按切片的 OMS 槽号区分已释放的单与该腿的新切片(`TargetCell.slot`)。
5. **预检**(只读,避免门控风暴):增仓腿看 `v.mode(a, g)`(CloseOnly;Shadow 与 `open_allowed = false` 按"运行模式、Drain 与 Unknown"只交出一片,之后 `Stuck{Shadow}`)与 `v.risk(a)`(HARD、`open_blocked`(含 Kill cap)、MarginStress ≥ L1);减仓腿只看 HARD;两类都要求所用腿 `quote_ok`——例外:`reason ∈ {MarginStress, Hard}` 的配对目标只要求第一腿(永续腿),第二腿不可用、在途链接处于 `LINK_HOLD` 或其第一腿 Unknown 时退回串行腿(`SerialFallback`,触发条件见"保证金危急的默认减仓"第 2 条)。不满足 → 记 `gated_since`,持续 ≥ `stuck_after_ms` → `Stuck{Gated(bits)}`(只因报价不可用时为 `Stuck{StaleQuote}`)。
6. **穿越零**:`sign(target_i) ≠ sign(pos_i)` 且 `pos_i ≠ 0` 时本腿先只做"减到 0"的部分;开向另一侧在减到 0 之后的下一笔。
7. 按 `exec` 选算法计算切片(下文各节;限价与单笔名义一律按"限价与单笔名义"),写 `ExecOrder`、`active`、`last_slice_tsc`、`wake_at = now + 冷却`,写 `ExecDecision{Slice}`,登记定时器。
8. 补腿、残余、外来成交截止的检查在同一次推进里进行(下文各节;`manual_hold` 与恢复期的抑制见"人工保持与恢复期抑制")。

### 不交易带与紧急度

| `urgency` | 切片间隔 | 可见深度 | Stuck 判定 | 带 |
|---|---|---|---|---|
| `Passive` | `cooldown_ms × passive_cooldown_mult` | 只用对手价一档(`depth_levels = 1`) | `stuck_after_ms × passive_cooldown_mult` | 按 `band` |
| `Normal` | `cooldown_ms` | 按参数 | `stuck_after_ms` | 按 `band` |
| `Urgent` | `min(cooldown_ms, urgent_cooldown_ms)` | 按参数 | `stuck_after_ms / 2` | 按 `band` |

`reason ∈ {MarginStress, Hard}` 一律按 `Urgent`。带的分母下限 `floor_i = band_floor_notional / mark_i`(按 lot 向零取整),小目标不会因比例带过窄而来回交易。

**目标为 0 的腿不用带**(任何紧急度):该腿"在带内"⇔ `|x_i| × mark_i ≤ dust_notional`(完成判定 `x_i = pos_i`,出片判定 `x_i = eff_i`,见"推进"第 2–4 步),`band` 与 `floor_i` 只用于目标非 0 的腿。所以目标为 0 的腿一直切片到尘埃线(含)以下,不会停在"带内却高于尘埃线"而既不出单也不判完成;低于最小下单量的最后一笔按"残余与尘埃"处理。

### 增仓与减仓的分类

| 切片对该腿的作用 | 本 crate 给的 `class` 提示 | 交易所侧 `REDUCE_ONLY` | 模式门 | 说明 |
|---|---|---|---|---|
| 增加 |仓位|(含配对的第一腿开仓) | `Open` | 否 | 过(`origin = Target`) | 风控 Open 全链;带 LegLink 时两腿 venue 同查,含第二腿报价连接健康 `LINK_STALE` 与可负担性 `LINK_FUNDS`(B1,见 [hft-risk](../hft-risk/README.md)) |
| 减少,`reason ∈ {Rebalance, Exit, Manual}` | `Reduce` | 该 (venue, kind) `VenueCaps.reduce_only` 支持时置位 | 过(CloseOnly 放行) | 现货卖出量 ≤ min(该 group 库存, 可用余额)(B4) |
| 减少,`reason ∈ {MarginStress, Hard}` 或 `origin = Engine` | `Flatten` | 同上 | 不过 | 风险递减;含 Broken cell、`SerialFallback` 串行腿与无组 cell 上的 L3/flatten 引擎目标。账户处于 `force_active` 或 `flatten_lock` 时,该账户 cell 上策略目标(如替换了引擎目标的 `reason = Exit` 目标)的订单同样按 `origin = Engine` 发出;这期间该账户没有增仓切片(L3 时 MarginStress ≥ L1 的预检不出增仓切片,清仓锁只接受更接近 0 的目标),离开后恢复按 `origin = Target` |
| 补腿、残余修剪、外来成交默认处理、自动有界平仓 | `Hedge`(`LinkRole::HedgeFix`) | 同上 | 不过 | 只减多出的一腿;填 `hedge_cap_qty` |

最终分类与 `RISK_REDUCING` 判定只在 [hft-risk `classify()`](../hft-risk/README.md);本 crate 不自行放行任何检查。配对算法的第二腿由 OMS 按 `HedgePlan.reduce_only` 发出:第二腿是减仓且该腿支持 reduceOnly 时 `reduce_only = 1`,否则 0。

### 报价可用(连接健康)

本 crate 每一处要用对手价的地方——切片与配对第一腿的定价和深度、补腿、残余修剪、外来成交默认处理、自动有界平仓、L3 默认减仓与 `engine_flatten` 的引擎订单——都用同一个判定 `quote_ok(a, inst)`,**按连接健康判定,不按盘口最后一次变化的时刻**:

1. 该 inst 的 `BboView` 两侧都有价;
2. SDK `BboView` 的连接健康判定成立(判定式 `quote_live` 归 [hft-strategy-sdk `BboView`](../hft-strategy-sdk/README.md),成员连接的聚合归 [hft-sequencer 连接活性](../hft-sequencer/README.md#连接活性)):`conn_up`、`resync = false`、`now − conn_live_tsc ≤ conn_liveness_ms`。`conn_live_tsc` 是成员连接最近一次**活性证据**(行情记录或安静时的 `ConnState{Alive}`)的接收时刻,取法归 hft-sequencer;[hft-venue](../hft-venue/README.md) 对 BBO 连接每 `bbo_ping_ms` 发 WS ping,安静行情下也有活性证据;
3. 报价取该 inst 最新一条 Bbo:盘口多久没变都有效(安静的 inst 不是过期的 inst)。

阈值 `conn_liveness_ms` 取 `v.risk(a).conn_liveness_ms`(hft-risk `RiskParams`,默认 2 000),与风控检查链 #5(含链接腿 `LINK_STALE`)和 OMS [`price_hedge`](../hft-oms/README.md#对冲定价) 同值同源,所以执行层认为可用的报价不会被风控以"报价过期"拒绝。不成立 → 本笔不发,原因 `StuckReason::StaleQuote`(语义 = 报价连接不健康,不是"报价没变"),计入 Stuck 判定;引擎订单按门控等待、不计次——补腿与外来成交默认处理的截止照算,自动有界平仓不计入可执行时长(见"Broken 处置"),L3/flatten 不设截止。可见深度用 L2 时,`v.book(inst).status()` 必须为 `Live`,否则只用 BBO 顶档。

### 限价与单笔名义

规则唯一归属本节。本 crate 产生的每一笔订单(目标切片、配对第一腿、补腿、残余修剪、外来成交默认处理、命令补腿、自动有界平仓、L3 默认减仓与清仓的引擎单、无组永续持仓的减仓单)按同一规则定价与定量,使风控检查链 #5(风险递减单只查越价上限)与 #6(单笔名义)按同一口径放行它们;OMS 的对冲腿定价 [`price_hedge`](../hft-oms/README.md#对冲定价) 用同一个函数。

1. **越价参数与越价上限**(全部按本笔 inst 取值:`rr_i`、`ioc_i` = `v.risk_for(a, inst)` 的 `rr_max_cross`、`ioc_max_cross`,即该 inst 所属价格带类别的值,与检查链 #5 同值同源):
   - 策略目标的 Open 切片:`max_cross_eff` = min(`slice.max_cross`, `ioc_i`)(#5 对 IOC Open 查 `ioc_max_cross`);减仓切片:min(`slice.max_cross`, `rr_i`)。执行时截取,不拒绝目标。
   - 引擎单:`max_cross_eff` = `force_slice.max_cross`(配置约束保证 ≤ 账户内最小的 `rr_max_cross`,见"配置项")。
   - 越价上限 `rr_eff`(填进 `lim.rr_max_cross`):一般为 `rr_i`;`reason ∈ {MarginStress, Hard}` 的引擎目标订单(L3、清仓、对齐目标与无组持仓减仓)在 `v.risk_for(a, inst).l3_max_cross > 0` 时取 max(`rr_i`, `l3_max_cross`)(可选的更宽 L3 上限,默认 0 = 不启用,上线签字时决定;风控对这类单按同一值放行,规则归 [hft-risk](../hft-risk/README.md))。
2. **限价** `px = cap_cross_px(side, touch, mid, max_cross_eff, &lim, tick)`([hft-types](../hft-types/README.md) 纯函数:买 = min(对手价 × (1 + max_cross), mid × (1 + rr_max_cross)),卖对称,按 side 取整到 tick;签名以 hft-types 为准);`touch` = 对手价(买取 ask、卖取 bid),`mid = (bid + ask) / 2`,`lim = HedgePxLimits{conn_liveness_ms, rr_max_cross: rr_eff}`(`conn_liveness_ms` 取自 `v.risk(a)`),`tick` 取该 inst 的 `InstMeta`。报价按"报价可用"判定,不可用则本笔不发。
3. **单笔名义上限** `cap_n`(USDT)= min(`max_chunk_eff`, 本笔类别的风控上限):Open → `max_order_notional`;Reduce/Flatten → `max_reduce_notional`;Hedge 类(补腿、残余修剪、外来成交默认处理、命令补腿)→ `max_hedge_notional`;自动有界平仓 → min(`max_reduce_notional`, `max_hedge_notional`)。`max_chunk_eff`:策略目标 = min(`slice.max_chunk_notional`, `max_order_notional`),引擎单 = `force_slice.max_chunk_notional`。上限都读 `v.risk(a)`。
4. **数量按限价折算**(与 #6 同一口径):#6 的名义是 `qty × 限价` 以计价资产计、按 hft-ledger 换算率折 USDT 并向上取整(定义归 [hft-risk](../hft-risk/README.md))。本 crate 用同一换算率 `q = v.risk_for(a, inst).quote_rate`(USDT 计价时为 1)反算:`qty ≤ ⌊cap_n / (px × q)⌋`(i128 中间积,向零),再按 lot 向零取整,并验证 `⌈qty × px × q⌉ ≤ cap_n`(不满足减 1 lot),所以按本节定出的单不会因单笔名义被 #6 拒。Open 另受 #5 的 `max_px_dev` 与 #6 的 group/账户/venue 名义约束(策略参数与敞口上限,超出按门控处理)。
5. **引擎单被 #5/#6 拒**(`PX_BAND` 或 `NOTIONAL_*`):按本节定价定量的引擎单仍被这两类位拒绝,说明执行层与风控口径不一致(参数或实现错误),**按执行失败处理**(计入 `fail_streak` 与补腿计次,不当门控):写 `ExecDecision{该单的 action, detail = Engine/RiskReject, gate = bits}` 与 `Note{ExecEngineOrderRejected}`(**P0**,`v = [account | group << 8, leg_ix, ExecOrderKind, reject_bits, qty, px, mid, cap_n]`);L3、清仓与无组持仓的单 action = `ForceStep`。`HEDGE_LIMIT` 不在此列(见"对冲量上限");自动有界平仓遇到这类拒绝按失败结束(见"Broken 处置")。策略目标的切片被这两类位拒绝仍按门控处理(`Stuck{Gated(bits)}`)。

### TakerSlice

对每条 `|diff_i|` 在带外、无在途的腿 `i`:

1. `px_ref` = 对手价(买取 ask、卖取 bid);`quote_ok` 不成立 → `StaleQuote`(计入 Stuck 判定,不发)。
2. `depth_i` = 对手侧从顶档起、价格在 `px_ref × (1 ± max_cross_eff)` 内、至多 `depth_levels` 档的数量和(无 L2 簿或簿非 `Live` 时为 BBO 顶档量)。
3. 限价 `px` 按"限价与单笔名义"第 2 条;`chunk = min(|diff_i|, depth_i × depth_frac, 按 cap_n 折算的数量上限)`(`cap_n` 按本笔类别、折算按同节第 3、4 条);现货卖出再与 `min(group 库存, 可用余额)` 取小。
4. `chunk` 按 lot 向零取整([hft-types](../hft-types/README.md) 的下单数量规则,不向上凑)。`chunk × px < min_chunk_notional`:若 `|diff_i|` 本身就小于它且 ≥ 交易所最小名义 → 发 `|diff_i|`(本目标最后一笔);若 `|diff_i|` 低于交易所最小下单量 → 交"残余与尘埃";否则 `NoLiquidity` 等待。
5. `tif = Ioc`,限价即第 3 步的 `px`。

"弱侧"是本笔会交易的全部腿中可见深度(折算到第一腿单位)最小者;单腿时即该腿。`TakerSlice` 的各腿独立推进,不做配对取整(各按自己的 lot)。

### PairOnFill

两腿目标(`n_legs == 2`;更多腿 → `TargetReject::TooManyLegs`)。

1. **腿顺序**(C14,规则唯一归属本节):
   - `reason ∈ {MarginStress, Hard}`,或该账户处于默认减仓期间 → 强制 `PerpFirst`,忽略参数:永续腿先;两腿都是永续时取 `ForceDeleverage` 指定的 venue(否则保证金率较高的 venue,`PortfolioView`),再相同取名义大者;没有永续腿时退回 `ByLiquidity`。
   - 其余(`Rebalance`/`Exit`/`Manual`,增仓与减仓)→ `params.first_leg`,参数默认 `ByLiquidity`。
   - `ByLiquidity`:比较两腿对手侧可见深度名义(同 TakerSlice 第 2 步),小者先("流动性差的先做");相同取 `leg_ix` 小者。
   - `SpotFirst`/`PerpFirst` 找不到对应市场类型的腿时按 `ByLiquidity`;`Fixed` 而 `first_leg_ix` 越界 → `SpecInvalid`。
2. **第一腿量** = TakerSlice 第 3 步,弱侧深度取 `min(depth_first, depth_second × den / num)`;并保证第二腿名义 ≤ `max_hedge_notional`:第二腿名义按其**上界**估算——价格取 `mid_h × (1 + rr_max_cross(第二腿 inst))`,**买卖两个方向相同**(买单限价至多为该价,超滑点时 OMS 按它定价;卖单限价不高于 `mid_h`;与 [hft-sequencer `link_notional`](../hft-sequencer/README.md#意图管线) 同一口径),数量 `qty × ratio`,按"限价与单笔名义"第 4 条同一换算率与取整口径折 USDT——使风控按 `link_notional` 放行、第二腿任何定价下都不会被 `HEDGE_LIMIT` 拒;再右移 `fund_shrink` 位(见第 7 条);最后**向零取整到 `pair_lot` 的整数倍**,低于 `pair_min` 不发(见"两腿 lot 对齐")。两腿都须 `quote_ok`(第二腿不可用时风控另以 `LINK_STALE` 拒第一腿);`reason ∈ {MarginStress, Hard}` 时只要求第一腿,第二腿不可用 → `SerialFallback`(见"保证金危急的默认减仓")。
3. **第一腿订单**:IOC,`LegLink { group, role: First, tolerance: fix_tolerance, hedge: HedgePlan { kind: OnFill, venue/account/inst/side = 第二腿, px_mode/px_param/max_slip = 参数, reduce_only, ratio, residual, on_fail: Retry, tif: Ioc } }`。第二腿方向取第二腿 `diff` 的符号;第二腿 `diff` 在带内时本笔退化为单腿切片(无链接)。
4. 第二腿由 OMS 在第一腿每个成交增量的**同一事件内**按**成交净量**发出,且不超过 LegTracker 在该方向的裸量(B4、D2,规则归 [hft-oms OnFill](../hft-oms/README.md#onfill));本 crate 不介入数量。
5. `LinkNotice{Done}` → 清 `active`、按冷却排下一笔;`LinkNotice{Broken}` → `Broken{LinkBroken}`(原因为 `HedgeLimit` 的 → `Broken{HedgeVeto}`,见"对冲量上限");不带 `RESUME_HOLD` 的 `LinkNotice` 或链接终态 → 清 `LINK_HOLD`。
6. 第一腿到带内后第二腿仍在带外(残余、费用补差)→ 第二腿单腿尾笔:减少多出部分按补腿语义(`ResidualTrim`),增加不足部分按增仓分类。
7. **链接腿门控**:第一腿被风控以 `LINK_STALE`(第二腿 venue 的报价连接不健康:断开、重同步或超过 `conn_liveness_ms` 没有活性证据;盘口不变不算)或 `LINK_FUNDS`(第二腿可用计价资产/保证金不足)拒绝时按门控处理(不计执行失败,`gated_backoff_ms` 后再试);`LINK_FUNDS` 另使 `fund_shrink += 1`(上限 4,即第一腿量至多缩到 1/16;缩量后低于 `pair_min` 时取 `pair_min`),第一腿下一笔被接受时清零。判定公式与 `funds_buffer` 归 [hft-risk B1](../hft-risk/README.md);本 crate 不复算。这两位只检查带链接腿的 Open,`reason ∈ {MarginStress, Hard}` 的减仓第一腿不会遇到它们(它的第二腿可用性按"保证金危急的默认减仓"第 2 条判定)。

### 两腿 lot 对齐

规则唯一归属本节;`SetParams` 时的通用校验由 [hft-sequencer](../hft-sequencer/README.md) 执行,错误码 `InvalidArgs{LotMismatch}` 的 detail 登记在 [control-plane](../../docs/subsystems/control-plane.md)。目的:配对算法每一片的第二腿都能按第一腿成交量**整笔**下出,不留不可对冲的残余(例:一腿 lot 0.001、另一腿 lot 0.01 时,第一腿按 0.001 取整的 0.004 在第二腿只能下 0 张)。

1. **`pair_lot`**:两腿的 lot 记为 `lot_f`(第一腿)、`lot_h`(第二腿),同资产同尺度(hft-types 按资产定点)。对冲比 1/1 时:一方整除另一方 → `pair_lot` = 较粗者(`max(lot_f, lot_h)`);不能整除 → `pair_lot = lcm(lot_f, lot_h)`。对冲比 `num/den` 一般情形:`pair_lot` = 使第一腿量为 `lot_f` 整数倍、且第二腿量 `q × num / den` 恰为 `lot_h` 整数倍的最小 `q`,即 `lcm(lot_f, M / gcd(M, num))`,`M = den × lot_h`(i128 中间量;纯函数 `pair_lot` 在 [hft-types](../hft-types/README.md) 的数量取整模块,引擎与策略共用)。腿顺序按 `ByLiquidity` 等规则每片可换,`pair_lot` 对两腿对称计算并分别折算到各自单位。
2. **`pair_min`**:`pair_lot` 的最小整数倍,使两腿各自的 `min_qty` 与 `min_notional`(按当时对手价)都满足。第一腿量低于 `pair_min` → 本片不发(`NoLiquidity` 等待;目标剩余本身小于 `pair_min` 时交第 6 条的单腿尾笔与"残余与尘埃")。
3. **取整**:第一腿量一律向零取整到 `pair_lot` 的整数倍,所以第一腿全部成交时第二腿量恰为 `lot_h` 的整数倍且 ≥ 其 `min_qty`;IOC 部分成交与现货基础资产扣费产生的零头进 OMS 的 group 余数([hft-oms OnFill](../hft-oms/README.md#onfill) 第 2–3 步),由下一片吸收或交残余规则。价格上涨使一个 `pair_lot` 的名义超过 `max_chunk_eff` 时,本片取一个 `pair_lot`(仍受本笔类别的风控单笔上限,即 `cap_n` 中的风控项,超过它则本片不发),不因取整为 0 而卡住。
4. **减仓与风险目标不拒**:目标的每条腿都不增加 |仓位|(减仓方向),或 `reason ∈ {Exit, MarginStress, Hard}`(含引擎目标)时,不做 `LotMismatch` 拒绝:两腿 lot 不能整除且 `lcm` 名义 > 该目标的 `max_chunk_eff` 时,第一腿改按两腿中**较粗的 lot**(`max(lot_f, lot_h)` 折算到第一腿单位)取整,第二腿按自己的 lot 向零取整后的零头进 OMS 的 group 余数,剩余交"残余与尘埃";能整除或 `lcm` 名义不超过阈值时照常用 `pair_lot`。
5. **校验**(只对配对算法的增仓目标与 `SetParams`;阈值字段统一为切片参数的 `max_chunk_notional`):
   - `SetParams`(sequencer 通用校验,对每个含两条交易所腿的 GroupSpec,按对冲比 1/1):GroupSpec 不带切片参数,取该账户引擎切片的 `force_slice.max_chunk_notional`;不能整除且 `pair_lot` 名义 > 它 → 整条命令拒绝 `InvalidArgs{LotMismatch}`;名义所用的价格与无价格时的处理归 hft-sequencer 的 SetParams 校验。
   - `set_target`(配对算法、至少一条腿增仓、`reason ∈ {Rebalance, Manual}`):按目标的对冲比与该目标的 `slice.max_chunk_notional`(截取后的 `max_chunk_eff`)重算;不能整除且 `pair_lot` 名义(按第一腿 `v.mark`,无 mark 时 BBO 中价,都没有则跳过此项、留给切片时的第 3 条)> 阈值 → `TargetReject::LotMismatch`。策略切片的阈值小于 `force_slice.max_chunk_notional` 时,`SetParams` 放行的 group 的增仓目标仍可能被拒——只影响开仓,减仓与风险目标按第 4 条照常执行。
   - 算出的 `pair_lot`(或第 4 条的较粗 lot)存入 cell。

### Simultaneous

两腿同一事件发出、失败反向。第一腿量同 PairOnFill 第 2 步;第一腿带 `HedgePlan { kind: Simultaneous, on_fail: params.on_fail }`,OMS 在 `place` 第一腿时于同一事件产生第二腿,任一腿未全成时按 `on_fail` 处理(`Reverse`:对已成交未对冲部分发反向 IOC;`Hold`:只记 LegTracker),规则归 [hft-oms Simultaneous](../hft-oms/README.md#simultaneous)。反向失败(OMS 链接 `Broken{ReverseFailed}`)→ `Broken{ReverseFailed}`。第二腿在风控里按 Open 分类(与第一腿同时发出,不是风险递减)。

### MakerFirst(optional)

可选扩展,触发条件:某策略需要挂单腿,且 [hft-sim](../hft-sim/README.md) 的挂单撮合已启用并在模拟账户上验证。规格保留:第一腿 `PostOnly`(数量按 `pair_lot` 取整)挂在对手价内侧 `offset` 处、`max_rest_ms` 到期由 OMS `Aged` 通知后撤单重挂;对手价移动超过 `requote` → `amend`;`HedgePlan{kind: OnFill}` 成交后 IOC 对冲;目标被替换、清除或 `urgency` 升为 `Urgent` → 撤挂单并改用 PairOnFill。未启用 feature 时 `ExecSpec::MakerFirst` → `TargetReject::AlgoUnsupported`。

### 对冲量上限(B6)

规则(唯一归属本节):每条 Hedge 类订单的数量 `qty ≤ hedge_cap_qty = basis + 1 lot(inst)`,单笔名义 ≤ `max_hedge_notional`。`basis` 的来源:

| 订单 | 产生方 | `basis` |
|---|---|---|
| `Second` | hft-oms | 本次触发成交的净量 × ratio + 该 group 余数(取整前 `raw`);OMS 保证净量不超过第一腿槽的委托量(超量成交不进基数);实际数量另受 LegTracker 该方向裸量约束(只会更小) |
| `SecondRetry` | hft-oms | 链接欠对冲量 `owed`(已发出的第二腿请求量 − 已成 − 在途);实际数量同样受该方向裸量约束 |
| `Reverse` | hft-oms | 第一腿已成交净量 − 已对冲量 / ratio |
| `HedgeFix`、`ResidualTrim`、`ExternalDefault`、`CmdHedgeFix`、`AutoFix` | 本 crate | `naked_filled = |Σ legs.leg[i].net|`(多出的那条腿的超出量,只看已成交;发单时该 group 无即时在途,见各节) |
| `ForceStep`、引擎清仓(`Flatten` 类,含 Broken cell、串行腿与无组 cell 上的) | 本 crate | 不适用(`Flatten` 由风控的仓位检查保证不超过 |仓位|,单笔 ≤ `max_reduce_notional`) |

- **检查**在 [hft-risk 检查链 #6](../hft-risk/README.md)(`HEDGE_LIMIT`):产生方填 `hedge_cap_qty`,sequencer 投影进 `IntentView`;`max_hedge_notional` 是 `RiskParams` 字段。
- **违反** → 不出环;sequencer 设 `Broken{account, group}` + P0(OMS 来源的另调 `oms.hedge_vetoed` 把链接置 `Broken`,原因 `HedgeLimit`);hft-risk 随之设账户级 `HedgeFailure`。本 crate 收到 `on_place_result(Rejected(HEDGE_LIMIT))` 或原因为 `HedgeLimit` 的 `LinkNotice{Broken}` 时 cell → `Broken(HedgeVeto)`。
- 这一上限防的是单位/解码错误把对冲量放大(张数 × 乘数、`1000` 前缀指数等):基数受第一腿委托量约束,名义受账户上限约束,两者都由 Input 确定,回放一致。

### 补腿(`HedgeFix`,C4)

`HedgeFix` **只用于平掉多出的一腿**;链接存续期间"补齐不足的一腿"是 OMS 的 `SecondRetry`。

**触发**(在链接终态、成交、外来成交截止、`on_leg_adjust`、定时器之后的推进中检查,O(1)):

1. `naked_filled = |Σ legs.leg[i].net|`,`naked_filled × mark > dust_notional`,且 `naked_filled / max(|leg[i].net|) > tol`;`tol` = cell 的 `tolerance`(最近链接的 `LegLink.tolerance` 或目标的 `fix_tolerance`),为 0 时 `fix_tolerance_default`。
2. **抑制**(任一成立即不发,也不计次):`v.legs(a, g)` 的 `open_links > 0`、`has_unknown`、或任一腿 `inflight_immediate ≠ 0`(该 group 有 Open/Hedging 链接、Unknown 槽或即时在途订单;已 Ack 的挂单不抑制,它不是对冲覆盖,见 OMS LegTracker);cell 的 `cap_block` 有 `BROKEN`/`REORG`(Broken 的处理见"Broken 处置");`v.risk(a).hard`;该 cell 有在途切片;cell 处于 `SerialFallback`(串行腿本身就在减多出的一腿);外来成交计时未到期(先给策略处理的机会);`hold & MANUAL_HOLD`(期间 `FixState` 清零、不计截止);多出的那一腿所在 venue 未放行(恢复期:不发、不判截止;`fix.first_tsc` 照记,只作闸门滞留告警的起点,放行时清零)。规则见"人工保持与恢复期抑制"。
3. **HARD 下抑制**:HARD 生效时清空 `FixState`,不发、不计次。HARD 解除本身不产生任何订单:哪些 group 随之判 Broken 见"HARD 解除时的 Broken 判定",这些 cell 随后按"Broken 处置"计时。

**动作**:多出的一腿 = 符号与 `Σ leg[i].net` 相同、|量| 最大的腿(并列时优先支持 reduceOnly 的那条);数量 = `floor_to_lot(naked_filled)`,并 ≤ 该腿 |仓位|、现货 ≤ min(group 库存, 可用余额);低于该腿最小下单量 → 交"残余与尘埃"。该腿 `quote_ok` 不成立 → 按门控等待(不计次,截止照算;恢复期内不计,见上)。订单 IOC,限价与单笔名义按"限价与单笔名义"(引擎单,Hedge 类),`role = HedgeFix`,`origin = Engine`,`hedge_cap_qty = naked_filled + 1 lot`。

**多出的一腿是链上腿**(optional 链上 feature):执行层不动作(它不能下链上单),不计次;写 `Note{ExecChainLegExcess}`(**P0**,`v = [account | group << 8, leg_ix, naked_filled, naked_filled × mark]`),留给策略用链上意图处理;同一 group 在 naked 变化前不重复写。这条同样适用于残余修剪、外来成交默认处理、自动有界平仓(按失败结束)与 `hedge_fix_cmd`(命令返回 `ExecCmdErr::ChainLeg`)。

**计次、冷却、截止**(与 OMS 第二腿重试同一机制):首次满足触发且未被第 2 条抑制时记 `fix.first_tsc`(抑制期间不记,唯一例外是多出那一腿所在 venue 未放行时照记、只作闸门滞留告警的起点;venue 恢复期放行与 `manual_hold` 解除时清零,下一次满足触发时重新记);执行结果类失败(交易所拒单、IOC 零成交、引擎单被 #5/#6 拒(`RiskReject`)、计次类的本地发送失败)`count += 1`,门控拒绝不计次但时间照算(本地发送失败哪些计次见 [hft-oms SendReport 处理](../hft-oms/README.md#sendreport-处理));两次间隔 ≥ `fix_cooldown_ms`(`Timer{Exec, Fix}`);`count ≥ fix_max` 或 `now − first_tsc > fix_deadline_ms` → `Broken{FixExhausted}` + `Note{ExecBroken}`(P0)。naked 回到阈值内 → `FixState` 清零。

`fix.first_tsc = 0` 且 `now < wake_at` 时,本 cell 的切片或门控退避尚未结束,不启动补腿窗口;容量续推、回填引起的强制推进与快照恢复均遵守此限制。已经开始的窗口在退避期间继续计时,不因 `wake_at` 延后而重置或延长截止。

**命令** `hedge_fix_cmd`:同一动作,`kind = CmdHedgeFix`,不受计次、冷却、Broken 状态、`manual_hold` 与恢复期抑制的限制(它是 Broken 的人工处理方式、Reorg 的唯一处理方式;调用即解除该 cell 的 `manual_hold`),仍受 HARD 与风控;预览由 control-plane 基于最新对账生成。在 Broken 期间调用会取消本期的自动有界平仓(`auto_fix.phase = Cancelled`)。命令不看补腿阈值 `tol`,只要求已成交裸量高于尘埃线。`ExecCmdErr`:`Hard`(HARD 生效)、`UnknownGroup`(该账户无此 group 的 cell)、`NotQuiescent`(该 group 有活动链接、Unknown/Pending* 槽或即时在途)、`NothingToFix`(已成交裸量 ≤ 尘埃线,或按 lot 向零取整后低于多出那一腿的最小下单量)、`QuoteUnavailable`(多出那一腿 `quote_ok` 不成立)、`ChainLeg`(optional,见上);返回错误时不产生输出、不改状态(不解除 `manual_hold`、不取消自动有界平仓)。naked 降到尘埃线以下后 Broken cap 的解除条件由 hft-risk 判定。

### HARD 解除时的 Broken 判定(`resolve_broken`)

判定规则唯一归属本节;调用点归 [hft-sequencer](../hft-sequencer/README.md),hft-risk 只在删除带 HARD 的 Kill 记录时置 `ModeOutcome.resolve_broken`。

1. **只在 HARD 解除时判定**。HARD 期间引擎订单(含补腿与第二腿)全部停止,解除时未对冲的 group 若交给补腿,可能与人工在 HARD 期间的处理重复。`Resume{Kill}`(非 HARD)、其他锁存的解除与 Recovery 的自动解除都不判定:非 HARD 锁存期间风险递减单照常,补腿本来就在跑;恢复期的补腿与外来成交默认处理在 venue 放行后按新窗口进行(见"人工保持与恢复期抑制")。
2. sequencer 对 `oms.naked_groups(a)` 列出的每个候选 group 调 `resolve_broken(a, g, v)`;为真 ⇔ 该 group 无活动链接(`open_links = 0`;有活动链接的由 OMS 按截止判定)、cell 无 `MANUAL_HOLD`,且已成交裸量满足补腿"触发"第 1 条(`naked_filled × mark > dust_notional` 且 `naked_filled / max(|leg[i].net|) > tol`)。O(腿数),只读。
3. 容差内的残余与人工保持的 cell 不判 Broken,分别留给补腿与人工;判 Broken 的 cell 从解除时刻起按"Broken 处置"计时。

### Broken 处置

Broken 的 (account, group) 有三条出路,优先级从高到低:人工 `HedgeFix` 命令、引擎减仓(L3 默认减仓与 `engine_flatten`,见下一节)、**自动有界平仓**(本节)。三者都只减不增,都受 HARD 约束;任何一条把已成交裸量降到尘埃线(含)以下且该 group 无即时在途、无 Pending*/Unknown 槽后,Broken cap 由 hft-risk 按同一条件解除(cell 按"状态机"离开 Broken);账户级 `HedgeFailure` 仍须人工 `Resume{HedgeFailure}`(该账户已无 Broken cell 时才被接受)。残余低于最小下单量、三条出路都下不了单时,由 `WriteOffResidual` 核销(见"残余与尘埃")。

**量的口径**(与 hft-risk 条件型解除 Broken 同一口径):只看**已成交**的裸量,`naked_sum = Σ leg[i].net`(带符号),`naked_filled = |naked_sum|`;已发出未终态的单(含自己的平仓片)不算已减。开始与每一片之前都要求该 group 无即时在途、无 Pending*/Unknown 槽、无活动链接(见下),所以发单时已成交口径就是真实敞口,不会重复下单。多出的一腿 = 符号与 `naked_sum` 相同、|`net`| 最大的腿(并列时优先支持 reduceOnly 的那条)。

**"一次"的含义**:每个 Broken 期只有**一次可执行的尝试**。开始前的等待、开始后的门控拒绝与零成交都不消耗这次尝试;只有下面"结束"表里的失败原因才结束它。

**计时**(`AutoFixState`,O(1);`broken_auto_fix_ms` 计时口径的唯一归属,testing 与 runbook 链接这里):

1. cell 进入 Broken(本 crate 自己判定的 `BrokenReq`、`LinkNotice{Broken}`、`on_place_result(Rejected(HEDGE_LIMIT))`、或 `on_risk` 收到该 group 的 `CapSet{Broken}`,幂等)→ `broken_since = now`、`phase = Primed`、`due_tsc = now + broken_auto_fix_ms`,登记 `Timer{Exec, AutoFix}`;此时已有 `manual_hold` 则 `phase = WaitHold`。离开 Broken(`CapCleared{Broken}`)→ `AutoFixState` 清零、撤定时器。期间收到 `hedge_fix_cmd` → `phase = Cancelled`,本 Broken 期不再自动执行(人工已接手)。
2. 到期前后的三种情况:
   - **暂停,解除后重给完整窗口**:HARD(全局或该账户)生效 → `Primed`/`Running` 转 `WaitHard`;`manual_hold` 置位 → 转 `WaitHold`(两者都让在途切片自然终结,不再发新片)。解除时 `phase = Primed`、`due_tsc = now + broken_auto_fix_ms`。
   - **恢复期不计时**:多出那一腿所在 venue 未放行时不开始;该 cell 任一腿所在 venue 放行(`on_venue_gate_open`)时,`Primed` 的 cell `due_tsc = now + broken_auto_fix_ms`——放行之前的时间(含崩溃前已 Broken 的时长)一律不计,人有完整窗口。
   - **其余等待条件计时不停**:下面"开始"第 1 条的其他等待条件不影响 `due_tsc`;到期后条件一消失就立即开始。
3. 停机不计:`recovery_end` 平移 `broken_since`、`due_tsc`、`run_mark_tsc`(见"快照与恢复")。

**开始**(`Primed` 且 `now ≥ due_tsc` 的推进中检查;任一等待条件成立 → 不开始、不算失败,条件消失的那次推进立即开始;到期后因等待条件(引擎目标除外)超过 `gate_naked_alarm_ms` 仍未开始 → `Note{ExecGateHeldNaked}`(P0,场合 2,见"人工保持与恢复期抑制")):

1. 等待条件:
   - 该 group 不静止(静止的定义归 [hft-oms LegTracker](../hft-oms/README.md#legtracker):有 Pending*/`Unknown` 槽、任一腿 `inflight_immediate ≠ 0` 或 `open_links > 0`)——等 OMS 收敛;`Unknown` 长期不收敛另由 OMS 的 `OmsUnknownStuck`(P0)与 `UnknownTimeout` cap 告警;
   - 多出的那一腿所在 venue 未放行(`venue_open` 无该位;放行时到期时刻重设,见"计时");
   - 该 cell 有引擎目标(引擎减仓优先;引擎目标清除后若仍 Broken 立即再查);
   - `cap_block` 有 `REORG`(链上重组只能人工);
   - 多出的那一腿 `quote_ok` 不成立;
   - 该腿所在 (venue, account) 处于限速退避(`v.risk_for(a, inst).rate_backoff`);
   - 对手价已越过越价上限(买 `ask > mid × (1 + rr_eff)`、卖 `bid < mid × (1 − rr_eff)`,`rr_eff` 按"限价与单笔名义"第 1 条):IOC 在 `cap_cross_px` 的限价下吃不到;
   - 现货腿的可用余额不足最小下单量,而按裸量与 group 库存算的量足够(余额被挂单锁定或余额推送滞后;不计入失败)。
2. `naked_filled × mark ≤ dust_notional` → 直接按成功结束(`done_qty = 0`)。
3. 多出的一腿是链上腿 → 失败(`ChainLeg`);`budget = floor_to_lot(naked_filled)`,再 ≤ 该腿 |`net`|,现货腿 ≤ group 库存;`budget` 低于该腿最小下单量 → 失败(`MinOrder`)。现货的可用余额只约束每一片(见下),不约束 `budget`。
4. `phase = Running`、`started_tsc = now`、`run_mark_tsc = now`、`exec_ms = 0`、`leg_ix`、`start_sign` = 开始时 `naked_sum` 的符号、`budget`;写 `ExecDecision{AutoFix, detail = AutoFix/Start}`。

**可执行时长**:`Running` 期间,"开始"第 1 条的任一等待条件(除引擎目标,它按下表转交)重新成立时暂停计时(`exec_ms += (now − run_mark_tsc)` 折成 ms,`run_mark_tsc = 0`),条件消失时 `run_mark_tsc = now` 继续;门控退避与零成交后的等待照常计时(这些是可执行状态下的结果)。`exec_ms` 加上当前段超过 `fix_deadline_ms` → 失败(`Deadline`)。

**每一片**(`Running`、上一片已终态、该 group 无即时在途、无等待条件;两片间隔 ≥ `force_slice.cooldown_ms`,门控拒绝后 ≥ `gated_backoff_ms`):

1. **复核**:按当时的 `naked_sum` 重新确定多出的一腿;`naked_filled × mark ≤ dust_notional` → 按成功结束;`naked_sum` 的符号与 `start_sign` 不同(已越过零,如外来成交或人工操作改变了另一腿)→ 停止,按失败结束(`SignFlip`),不向另一侧下单。
2. 限价与 `cap_n` 按"限价与单笔名义"(引擎单,自动有界平仓类);`qty = min(budget − done_qty, floor_to_lot(naked_filled), 按 cap_n 折算的上限, 该腿 |net|)`,现货腿再 ≤ min(group 库存, 可用余额);按 lot 向零取整;低于该腿最小下单量:只因可用余额不足时按"开始"第 1 条的等待条件处理(暂停可执行时长),否则失败(`MinOrder`)。永续腿 = reduce-only 买回/卖出(该 venue `VenueCaps.reduce_only` 支持时置位);现货腿 = 卖出。
3. 订单 IOC,`kind = AutoFix`,`role = HedgeFix`,`origin = Engine`,`hedge_cap_qty = naked_filled + 1 lot`;写 `ExecDecision{AutoFix, detail = AutoFix/Slice}`。

**重试**(不结束尝试):本片被门控拒绝(风控 `RejectBits` 中 `HEDGE_LIMIT` 与引擎单的 `PX_BAND`/`NOTIONAL_*` 以外的位、模式门、OMS 本地发送失败中不计次的那几类)→ `gated_bits`、`gated_backoff_ms` 后再发一片;IOC 零成交 → `nofill_streak += 1`,≥ `max_nofill` 后等 `stuck_after_ms` 再发,任何成交清零。两者都写 `ExecDecision{AutoFix, detail = AutoFix/Slice, gate = bits}`,不写 P0。

**结束**(每片终态后判定,口径同"量的口径"):

| 结果 | 动作 |
|---|---|
| `naked_filled × mark ≤ dust_notional` 且该 group 无即时在途、无 Pending*/Unknown 槽 | `phase = Done`;`Note{ExecAutoFixDone}`(**P1**,`v = [account | group << 8, leg_ix, done_qty, naked_before, naked_after, n_slices]`);Broken cap 由 hft-risk 按同一口径解除 → `CapCleared{Broken}` |
| 交易所拒单;本地发送失败中计次的那类(`EncodeError`);`HEDGE_LIMIT` 或引擎单被 #5/#6 拒(口径不一致) | `phase = Failed`(前两类 `fail = Rejected`,后两类 `fail = RiskReject`) |
| 待发量低于最小下单量(不是因为可用余额);`done_qty ≥ budget` 而 naked 仍在尘埃线以上;可执行时长超过 `fix_deadline_ms` | `phase = Failed`(`fail = MinOrder | Residual | Deadline`) |
| 复核时 `naked_sum` 变号 | `phase = Failed`(`fail = SignFlip`) |
| 引擎目标在 `Running` 期间被设下(L3/flatten/对齐目标) | 本片终态后停止,`phase = Primed`(由引擎减仓接手;引擎目标清除后若仍 Broken,按当时的 naked 重新开始,`budget` 重算,`exec_ms` 清零) |

失败一律写 `Note{ExecAutoFixFailed}`(**P0**,`v = [account | group << 8, leg_ix, fail, reject_bits, done_qty, naked_before, naked_after]`)与 `ExecDecision{AutoFix, detail = AutoFix/Failed}`,cell 保持 Broken 等人工(`HedgeFix`、`WriteOffResidual` 或 `Resume`)。`AutoFixFail` 判别值:`ChainLeg=1 MinOrder=2 Rejected=4 Residual=6 Deadline=7 SignFlip=8 RiskReject=9`;3、5 保留不用(只追加,不复用)。每个 Broken 期至多一次终态(`Done` 或 `Failed`);再次进入 Broken 才重新计时。

### 残余与尘埃

规则全部由参数给出,不写死任何策略的数字。

- **尘埃**:`naked_filled × mark ≤ dust_notional` → 不动作;残余留在 LegTracker 的 `leg[i].residual`,对策略可见,不阻塞 `Done`。
- **残余**(高于尘埃线,但低于对冲腿最小下单量,无法由 OnFill 发出;OMS 按 group 累计,不向上取整;配对第一腿按 `pair_lot` 取整后,残余只来自部分成交与扣费零头):按该 group 的残余策略(最近链接的 `HedgePlan.residual`,目标的 `PairParams.residual`):
  - `Defer`:留给同 group 的下一笔链接(OMS 把 group 余数并入下一次 OnFill)。目标进入 `Idle` 后不再有下一笔,等同 `Hold`。
  - `TrimExcess`:对多出的一腿发 `ResidualTrim`(补腿同一动作、计次与抑制条件,含 `quote_ok`、`manual_hold` 与恢复期抑制;多出的是链上腿时同补腿的链上规则);修剪量低于该腿最小下单量 → `Hold`。
  - `Hold`:不动作;超过补腿阈值时由"补腿"处理。
- 目标为 0(`Exit`、默认减仓、清仓)时最后剩下的、低于交易所最小下单量的一腿:高于尘埃线 → `Stuck{MinOrder}` + `Note{ExecStuck}`(P1,人工处理),≤ 尘埃线 → `Done`。
- 按较粗 lot 取整的减仓与风险目标(见"两腿 lot 对齐"第 4 条)留下的第二腿零头同样按本节处理。
- **核销残余**:高于尘埃线却低于最小下单量的已成交裸量,补腿、自动有界平仓(`MinOrder`)与人工 `HedgeFix` 都下不了单;非 Broken 的 group 以这样的残余结束时同样无法移除。Admin 的升级命令 `WriteOffResidual{account, group}`(权限与命令级前置条件归 [control-plane](../../docs/subsystems/control-plane.md))适用于静止的 group(Broken 与否均可):OMS 把多出那一腿的已成交裸量移出该 group 的 LegTracker、清零该 group 全部腿的余数,返回单条 `WriteOff`(语义只在 [hft-oms 残余核销](../hft-oms/README.md#残余核销)),hft-ledger 对这一腿记 `AdjustDelta::WriteOff`(移到 unassigned / 账户级);sequencer 随后调 `on_leg_adjust`,并在同一 Input 内调 `on_legs`,Broken 的由 hft-risk 按尘埃口径解除。本 crate 不因核销产生任何订单(cell 若有目标,下一次推进按新的净量照常进行);Broken 的 cell 收到 `CapCleared{Broken}` 后按"状态机"离开 Broken,自动有界平仓状态清零。

### 外来成交的默认处理(B3)

1. `on_external(a, f)`:hft-ledger 已入账并给出归属 (account, group, leg)(`f.group`、`f.leg_ix`),OMS 已更新 LegTracker;sequencer 对每一笔入账量非 0 的外来成交都调用本函数(含不可归属的;被 hft-ledger `auto_held` 全部抵扣的不调用)。
   - `f.group == GROUP_NONE` → 不按下表处理、返回 0(P0 由 ledger 与 watchtower 报);该账户 `flatten_lock` 时登记一次无组 cell 的清仓重扫(`force_req`,同一 Input 的 `advance` 把新出现的无组永续持仓设为目标 0),L3 期间由下一步 `ForceDeleverage` 的扫描纳入。
   - `f.cause = Manual` → 先按"人工保持与恢复期抑制"置 `manual_hold`(cell 有引擎目标或账户 `force_active`/`flatten_lock` 时改置延迟位 `MANUAL_HOLD_DEFERRED`)。
   - 写 `ExecDecision{ExternalSeen, detail = External/…}`(子码见"`ExecDecision.detail` 打包与子码"),按下表处理该 cell(自上而下取第一条成立的):

| 条件 | 处理 | 返回(写进 `f.deadline_tsc`) |
|---|---|---|
| cell `Broken` | 保持 Broken,不暂停、不计时;Broken 处置照常(裸量按成交后的值;`manual_hold` 置位时自动有界平仓转 `WaitHold`) | 0 |
| cell 目标 `origin = Engine`,或该账户 `force_active`/`flatten_lock` | 引擎目标不暂停、不清除,按成交后的仓位继续推进(引擎单从不增加 |仓位|,被强平/ADL 掉的部分不会被开回来);推进到达后剩余的多出一腿按"补腿"处理 | 0 |
| `f.cause = Manual`(人工在交易所网页操作) | **不做引擎默认处理**:不计时、不在截止后自动平;`manual_hold` 抑制补腿、残余修剪与默认处理;`origin = Strategy` 的目标暂停为 `Stuck{External}`(不把人工平掉的一腿再开回来),直到 `manual_hold` 解除。后续由策略按自己的规则处理 | 0 |
| 其余(无目标,或 `origin = Strategy` 的目标) | `ext_deadline = now + external_deadline_ms`,登记 `Timer{Exec, External}`;有目标 → `Stuck{External}`(目标保留但暂停,避免把被强平的一腿再开回来);恢复期内截止到达也不执行,该 cell 的腿所在 venue 放行时截止重设(见"人工保持与恢复期抑制") | `ext_deadline`(名义值;恢复期内实际截止晚于它,不另通知策略) |

返回 0 表示引擎不做默认处理。`f.cause = Manual` 时无论落在哪一行都另写 `Note{ExecManualExternal}`(默认 P1;同一调用刚置位 `manual_hold`、已写 P1 的 `ExecManualHold` 时实际级别 Info;`v = [account, leg_ix, inst, side, qty, px]`)。`cause = Unknown`(含 reconciler 连续两次对账确认后以 `AUTO_EXTERNAL` 自动入账的 ADL 差异)与强平、ADL 同样按表处理。策略同时收到 `EventRef::External`(sequencer 投递)。
2. **已处理**:策略对该 (account, group) 调 `set_target`(替换了当前目标的;与当前目标逐字段相同的不算,同 `manual_hold` 解除的口径)/`clear_target`,或 `Ctx::place`/链上意图下了该 group 的订单(`note_strategy_action`)→ 清 `ext_deadline` 与暂停;新目标照常进入 `Working`(`flatten_lock` 与 L3 期间仍按"目标的接受与替换"校验)。`ext_deadline` 不经截止而清零时(本条、`manual_hold` 置位、`clear_account`)写一条 `ExecDecision{ExternalSeen, detail = External/0/Cleared}`。`manual_hold` 的解除另有规则(`clear_target` 不算),见下一节。
3. **截止**未处理(只有表中最后一行会到这里):清除暂停的 `origin = Strategy` 目标(通知 `Done{cleared_by_engine}`);若 `naked_filled` 超过补腿阈值且高于尘埃线 → 对多出的一腿发 `ExternalDefault`(补腿同一动作、计次、冷却、截止与抑制条件,含 `quote_ok`;多出的是链上腿时只写 `Note{ExecChainLegExcess}`);无论是否发单都写 `ExecDecision{ExternalDefault}`(不发单时 `qty = 0`)与一条 `Note{ExecExternalDefault}`,级别按条件:`naked_filled` 超过补腿阈值且高于尘埃线(进入默认处理:发 `ExternalDefault` 单,或多出的是链上腿)→ **P0**;否则(裸量在阈值内或在尘埃线以下,不发单)→ **P1**;`ext_deadline` 清零。

### 人工保持与恢复期抑制

两类状态暂停引擎的**自动**处置——补腿(`HedgeFix`)、残余修剪(`ResidualTrim`)、外来成交默认处理(`ExternalDefault`)与自动有界平仓;期间不计次、不判截止。L3 默认减仓、`engine_flatten`、对齐目标与人工 `HedgeFix` 命令不受影响。

**`manual_hold`**(cell 级,`TargetCell.hold & MANUAL_HOLD`):

1. **置位**:`on_external` 收到该 group 的 `ExternalFill{cause = Manual}`。cell 有引擎目标或账户处于 `force_active`/`flatten_lock` 时不立即置位(引擎减仓优先),改置 `MANUAL_HOLD_DEFERRED`;该 cell 的引擎目标清除时(L3 回落、对齐目标或清仓目标到达后清除)延迟位转为置位。置位时清 `FixState` 与 `ext_deadline`(撤 `Timer{Exec, Fix}`、`Timer{Exec, External}`),`origin = Strategy` 的目标暂停为 `Stuck{External}`,自动有界平仓 `Primed`/`Running` 转 `WaitHold`;写 `Note{ExecManualHold}`(**P1**,`v = [account | group << 8, 1(置位), leg_ix, inst, side, qty, naked_filled]`)。已置位时再收到 Manual 成交不重复写。
2. **抑制**:见上;补腿与残余修剪的抑制条件、外来成交表的 Manual 行、自动有界平仓的 `WaitHold` 都读这一位。策略目标推进也直接检查保持位,其他外来成交改变 `Stuck` 原因后仍暂停,包含配对目标的单腿尾量。不超时:人工动过的 group 等人或策略接手。
3. **解除**(只认下列动作):策略对该 group 的**非清除类动作**——替换了当前目标的 `set_target`(与当前目标逐字段相同的不算:每 tick 重发同一目标的策略不会因此解除并把人工平掉的腿开回来)或 `note_strategy_action`(`Ctx::place`、链上意图);`clear_target` 不算;人工 `hedge_fix_cmd`;该 cell 被设下引擎目标(L3 一步、`engine_flatten`、L3 回落时的对齐目标);`clear_account`。除"设下引擎目标"外,这些动作同时清 `MANUAL_HOLD_DEFERRED`。解除时写 `Note{ExecManualHold}`(实际级别 Info,`v = [account | group << 8, 0(解除), 原因(Strategy=1 HedgeFixCmd=2 EngineTarget=3 AccountCleared=4)]`),`fix.first_tsc` 清零(补腿从下一次满足触发起重新计截止),自动有界平仓 `WaitHold` → `Primed`、`due_tsc = now + broken_auto_fix_ms`。
4. 两个位都进快照(节 21 cell 行的 `flags`),重启后保持。

**恢复期**(按 venue:多出的那一腿所在 venue 在 `AcctState.venue_open` 中无位。`new`/`recovery_end` 起,直到该 (account, venue) 的 hft-risk `hedge_gate_ok` 与该 venue 的 `RECONCILED` 位首次同时满足、sequencer 调 `on_venue_gate_open(a, v)`;账户启动闸门放行 `on_gate_open` 蕴含全部 venue):

1. 多出的一腿在未放行 venue 上时:补腿与残余修剪不发、不判截止(`fix.first_tsc` 照记,只作闸门滞留告警的起点);外来成交默认处理在截止到达时不执行;自动有界平仓不开始(见"Broken 处置")。多出的一腿所在 venue 已放行就照常处置,不等同账户的其他 venue 与整个启动闸门:另一所断线时,健康 venue 上多出的永续腿照样被补腿或自动平仓。
2. `on_venue_gate_open(a, v)`(已置位则无动作):置 v 位;该账户每个有腿在 v 上的 cell:`fix.first_tsc`、`fix.count` 清零,`ext_deadline ≠ 0` 的改为 `now + external_deadline_ms`,`auto_fix.phase = Primed` 的 `due_tsc = now + broken_auto_fix_ms`;置 `deferred` 的 `TIMERS` 位并标记待推进(定时器在 `advance` 中按新时刻重登,已可处置的立即检查)。多腿 cell 的各 venue 先后放行时以最后一次为准(每个 venue 每次运行只放行一次)。
3. 恢复期不影响 L3/flatten 的引擎目标与 OMS 的链接续接(续接只等对冲闸门子集,规则归 [hft-oms](../hft-oms/README.md#恢复停机时间不计与闸门续接))。

**闸门滞留告警**(`Note{ExecGateHeldNaked}`,**P0**;阈值 `gate_naked_alarm_ms`;每 cell 一个 `Timer{Exec, Alarm}`,到期置 `deferred` 的 `ALARM` 位,由 `advance` 复核后写出):

- **场合 1**(账户启动闸门未放行):`gate_open = false` 期间,无 `MANUAL_HOLD` 的 cell 处于 Broken 或已成交裸量超过补腿阈值。起点 = max(`broken_since` 或 `fix.first_tsc`, `AcctState.gate_closed_tsc`),告警时刻 = 起点 + `gate_naked_alarm_ms`。
- **场合 2**(自动有界平仓到期后等待):`phase = Primed`、`now ≥ due_tsc`,且"开始"的等待条件(引擎目标除外)成立;告警时刻 = `due_tsc + gate_naked_alarm_ms`。
- `advance` 只在 `now <` 告警时刻时登记定时器(两种场合各有告警时刻时先登记较早的;起点或 `due_tsc` 变化时改期);到期复核仍成立 → 写一条,同一 cell 两种场合同时成立只写场合 1。所以每个起点至多一条;`recovery_timers` 按恢复后的状态重新登记(新的闸门关闭期从 `gate_closed_tsc` 起算)。`on_gate_open` 之后场合 1 不再成立。
- `v = [account | group << 8, 场合, 等待原因, 已持续 ms, leg_ix, naked_filled, naked_filled × mark]`;等待原因 = 写出时"Broken 处置 / 开始"第 1 条中成立的等待条件按码值从小到大取第一条:`Venue=1 NotQuiescent=2 Reorg=3 Quote=4 RateBackoff=5 BeyondCross=6 Balance=7`(引擎目标没有码,不参与)。场合 1 的 Broken cell 同样按此取值,不论自动有界平仓是否已到期,都不成立时为 0;非 Broken 的场合 1 为 0。

### 保证金危急的默认减仓(A5)与 `engine_flatten`

档位、阈值、截止与每一步的节奏归 [hft-risk](../hft-risk/README.md):L3 时它对 (account, venue) 发出 `ForceDeleverage{frac = force_step}`,每 `force_step_interval_ms` 仍为 L3 则再发一次。本 crate 只执行每一步。设定目标要读仓位(视图),所以 `force_deleverage` 只登记 `AcctState.force_req`(venue 并入集合、`frac`)、置 `force_active` 并把该账户相关的 cell 与无组 cell 标记待推进;同一 Input 的 `advance` 处理该账户时先按下列规则设定引擎目标:

1. **cell 的引擎目标**:该账户每个在 `venue` 上有永续腿仓位的 cell(**包括 Broken 的 cell**):
   - 基准 `base_i` = 该 cell 尚未完成的上一步引擎目标(有则用它,使步与步按比例复合,不因成交慢而重复减同一段),否则当前仓位;比例目标 `p_i = base_i × (1 − frac)`,按 lot **向零**取整(多减不超过 1 lot)。
   - 每腿取 `p_i` 与该 cell **当前目标**(任何来源;无目标时取当前仓位)中**更接近 0** 者:同号取绝对值小者,当前目标与 `p_i` 反号时取 0。所以策略已给的更接近 0 的目标(如退出 0/0)不会被换回较大的比例目标。
   - 同 group 的其他交易所腿按同一规则;链上腿不设目标(执行层不能下链上单),由此产生的多出部分写 `Note{ExecChainLegExcess}` 留给策略;`reason = MarginStress`、`urgency = Urgent`、`band = 0`。原有的策略目标被替换,写 `ExecDecision{ForceStep, detail = Engine/Step}` 并通知策略 `Stuck{EngineOverride}`;设下引擎目标即解除 `manual_hold`。
   - **先撤策略挂单**:设定 `reason ∈ {MarginStress, Hard}` 的引擎目标时(本条、`engine_flatten`、第 5 条的对齐目标、无组 cell),对该目标每条交易所腿的 inst 写一条 `cancel_resting{account, inst}`,sequencer 调 OMS `cancel_resting_on(a, inst, …)` 撤该 inst 上本账户全部非 EXEC 挂单(范围归 [hft-oms](../hft-oms/README.md#输出扇出));L3 每一步重设引擎目标时再请求一次(策略期间新挂的减仓单同样被撤),每次设定在该 cell 写一条 `ExecDecision{ForceStep, detail = Engine/CancelResting}`(`arg` = 请求撤单的腿位图;同一 `advance` 内重复的 (account, inst) 只出一条 `cancel_resting`)。撤单确认前,每片数量另 ≤ `|pos_i| − |该腿同向即时在途|`;风控 #7 对 Flatten/Hedge 类只扣即时在途(规则归 hft-risk),所以策略挂着的 reduce-only 单不会让引擎单被 `REDUCE_INVALID` 拒。
2. **算法**:
   - **非 Broken 的 cell**:`PairOnFill{first_leg: PerpFirst, slice: force_slice, residual: TrimExcess}`(单腿 cell 用 `TakerSlice{force_slice}`)。预检只要求第一腿(永续腿)`quote_ok`。第二腿不可用——第二腿 inst 的 `quote_ok`、`v.venue_ready(a, 第二腿 venue)`(无 Unknown 槽且下单连接 Up)任一不成立——或该 cell 的在途链接处于 `LINK_HOLD`(OMS `RESUME_HOLD`,重启后续接前)、在途链接的第一腿槽处于 `Unknown` 时**不等第二腿**,改走下一条的串行腿,称 **`SerialFallback`**:置 cell 的 `serial`,`active` 释放该链接(链接本身仍由 OMS 续接或超时,其第二腿在途照常计入 LegTracker),写 `ExecDecision{ForceStep, detail = Engine/SerialFallback}` 与 `Note{ExecSerialFallback}`(**P0**,`v = [account, 第二腿 leg_ix, 原因(StaleQuote=1 NotReady=4 ResumeHold=5 FirstUnknown=6;2、3 保留不用), 永续腿净量, 现货腿净量, reason]`;多个原因同时成立时记码值最小的一个,即按 StaleQuote、NotReady、ResumeHold、FirstUnknown 的顺序取第一条成立的,`ExecDecision` 的 `arg` 与 `v[2]` 同值)。进入后直到该 cell 到达引擎目标(或引擎目标被清除)都走串行腿,不切回 PairOnFill:永续腿先减,现货腿在其报价恢复后再减,**接受短时现货裸多**(已报 P0);期间不做自动补腿。
   - **Broken 的 cell 与 `SerialFallback` 的 cell**:不建链接(Broken 的 group 链接机制已失效;SerialFallback 的第二腿不可用),按腿**串行**:先全部永续腿(`Flatten` 类,reduce-only),永续腿全部到达引擎目标(或剩余低于最小下单量)后才开始现货腿(卖出 ≤ min(group 库存, 可用余额);报价不可用时门控等待);`ExecDecision.algo = Engine`。Broken 的 cell 状态保持 `Broken`,不发 Target 订单。
3. 推进照常(限价与单笔名义按"限价与单笔名义"的引擎单规则,越价上限可取 `l3_max_cross`;对手价按 `quote_ok`,盘口长时间不变照常减仓;永续腿在途切片 Unknown 时按"推进"第 4 条扣量继续);订单 `origin = Engine`、`Flatten` 类、永续先。每一步写 `ExecDecision{ForceStep}` 与 `Note{ExecForceStep}`(P1)。
4. **无组永续持仓**:该账户在 `venue` 上不属于任何 cell 的永续持仓同样减。数量 `u_i = portfolio(a).perp(v, i).qty − Σ 引用该 inst 的 cell 的 group_pos(a, g).legs[ix].qty`(两者同一写者 hft-ledger,同一 Input 内一致);只在 `u_i` 与账户该 inst 持仓 `perp(v, i).qty` **同号**时装入,数量 ≤ |账户持仓|(异号说明 group 归属与账户持仓不一致,不对它下单,由对账处理);扫描 refdata 中该 venue 的永续 inst 表(只在设定目标时一次,不在 tick 路径)。由该账户的**无组 cell**(`group = GROUP_NONE`)执行:至多 4 条永续腿,按 `|u_i| × mark` 从大到小装入(到达目标的腿在下一次设定目标时换入下一个;`|u_i| × mark ≤ dust_notional` 的不装);每腿 `TakerSlice{force_slice}`、`Flatten` 类、reduce-only,目标与第 1 条同一公式(基准为该腿未完成的上一步目标,否则 `u_i`);在途取本 cell 的 `own_infl`(无 LegTracker)。每次设定写 `ExecDecision{ForceStep, group = GROUP_NONE, detail = Engine/Ungrouped}` 与 `Note{ExecUngroupedReduce}`(**P0**,`v = [account, venue(清仓时 0xFF), n_legs, Σ |u_i| × mark, 本次装入的 inst 数, reason]`);不通知策略(无 group)。无组的现货(`unassigned`、基线资产)不动。
5. `RiskEvent::MarginStress` 降到 L3 以下 → `force_active = false`;在途切片照常完成,之后:
   - 非串行的 cell 与无组 cell:引擎目标清除(`serial` 清零),通知 `Done{engine}`;不恢复被替换的策略目标(策略按新的保证金状况重新给目标,增仓受 L1 禁开约束)。
   - `serial ≠ 0` 的 cell:引擎目标改设为 **`Hard` 对齐目标**——永续腿目标 = 其当前净量,现货腿目标 = 永续腿当前净量按对冲比折算的对冲量(与现货同号、|目标| ≤ 现货当前净量,按 lot 向零取整),`reason = Hard`、`urgency = Urgent`、`band = 0`、`origin = Engine(MarginStress)`,写 `ExecDecision{ForceStep, detail = Engine/HardAlign}`;仍走串行腿(现货在其报价恢复后减到对齐量),到达后清除并通知 `Done{engine}`。这样另一所仍断时现货残余由引擎目标门控等待,不进补腿截止、不判 Broken。对齐目标被替换时:新的引擎目标(L3 再次进入)沿用 `serial`;策略目标(此时账户已不在 `force_active`,按普通规则接受)清除 `serial`,之后按普通目标推进,第二腿不可用时按预检门控等待,剩余的多出一腿交补腿。
   - Broken 的 cell 引擎目标清除后仍为 Broken,自动有界平仓若已到期立即检查。
6. 默认减仓期间该账户的 `set_target`:每腿 `|t_i| ≤ |引擎目标_i|` 且不变号(减得更多,`reason = Exit` 的退出目标同样适用)才接受并替换引擎目标,腿顺序仍强制 `PerpFirst`、`urgency` 仍为 `Urgent`、`serial` 保持,订单按 `origin = Engine` 发出(见"增仓与减仓的分类");否则 `TargetReject::ForcedReduceActive`。Broken 的 cell 照旧拒绝 `CellBroken`。

**引擎目标只向 0 推进**:引擎单从不增加 |仓位|,所以账户处于 `force_active`/`flatten_lock` 或 cell 有引擎目标时,各腿目标按 `[0, 当前量]`(当前量含即时在途)截取——外来成交等使某腿先于引擎越过目标(更接近 0 或越过 0)时,该腿按已到达处理,不会被开回。`SerialFallback` 置位后该 cell 的任何目标(含 `force_active` 期间被接受、替换了引擎目标的策略目标)都走串行腿,直到 `serial` 清零。

**引擎目标与 cell 状态**(本节全部引擎目标:L3、清仓、对齐目标与无组 cell;Broken 的 cell 状态保持 Broken):设定时替换了策略目标的 cell 进入 `Stuck{EngineOverride}`,原本无策略目标的进入 `Working`(各写一条 `State` 决定);引擎目标在 `Stuck{EngineOverride}` 下照常推进;到达时按"推进"第 3 步写 `State → Idle` 并通知 `Done`(设定时已到达的 cell 在同一 `advance` 内写 `Working` 与 `Idle` 两条)。清仓目标与对齐目标到达后随之清除(不另写 `TargetCleared`,通知的 `cleared_by_engine = true`);L3 的引擎目标到达后保留,到第 5 条清除。

`engine_flatten`(`Kill{Account, flatten}`)用同一机制(登记 `force_req = Flatten`,advance 设定):对该账户**全部** cell(含 Broken)与全部 venue 上的无组永续持仓设引擎目标 0、`reason = Hard`(同一账户再次调用幂等),同样先请求撤策略挂单;非 Broken 的 cell 用 PairOnFill 永续先(第二腿不可用或链接 `LINK_HOLD` 时同样 `SerialFallback`),Broken 的 cell 用上面的串行腿(永续 reduce-only 先、再现货)。两腿都到 0 后 naked 为 0,Broken cap 由 hft-risk 解除;cell 的引擎目标(0)保留到到达,`CapCleared{Broken}` 不清它。HARD 生效时引擎单全部停止,目标保留并按门控记 `Stuck{Gated(HARD)}`(`ExecStuck` P1)。

**清仓锁 `flatten_lock`**:`engine_flatten` 同时置 `AcctState[a].flatten_lock`;它**不随清仓完成而解除**,只在 `on_risk` 收到该账户 Kill cap 的 `CapCleared`(即 Trader/Admin 的 `Resume{Kill}`)时清除。锁定期间:

1. 该账户的 `set_target` 只接受**比当前更接近 0** 的目标(`clear_target` 被忽略,见"目标的接受与替换"):每腿 `t_i = 0`,或 `sign(t_i) = sign(ref_i)` 且 `|t_i| ≤ |ref_i|`;`ref_i` = 该 cell 现有引擎目标的该腿(有则用它,清仓进行中即 0),否则该腿当前净量 `pos_i`。不满足 → `TargetReject::FlattenLocked`。被接受的目标(如策略的 `reason = Exit` 退出目标)替换引擎目标,但腿顺序仍强制 `PerpFirst`、`urgency` 仍为 `Urgent`,订单按 `origin = Engine` 发出;清仓完成后新开的 group 或已平的 group 因此只能接受 0 目标。
2. 策略经 `Ctx::place` 直接下的意图只放行 Reduce/Flatten 类(以及 OMS 的第二腿、重试与反向),规则归 [hft-risk](../hft-risk/README.md)(Kill cap 的 flatten 位);本 crate 只管目标。
3. L3 `force_deleverage` 照常(引擎目标以现有的 0 为基准,仍为 0);自动有界平仓照常排在引擎目标之后。
4. `flatten_lock` 进快照,重启后保持;`clear_account`(`SimReset`)不清它(锁跟随 Kill cap)。

### 门控拒绝与退避

- **门控**(不计执行失败):风控 `RejectBits`(`HEDGE_LIMIT` 除外,见对冲量上限;`LINK_STALE`/`LINK_FUNDS` 见 PairOnFill 第 7 条;引擎单的 `PX_BAND`/`NOTIONAL_*` 除外,见"限价与单笔名义"第 5 条)、模式门拒绝、OMS 本地发送失败中不计次的那几类(`KillBlocked`、`RateLimited`、`NoConn`、`RingFull`;分类唯一定义在 [hft-oms SendReport 处理](../hft-oms/README.md#sendreport-处理))。→ `gated_bits`、`wake_at = now + gated_backoff_ms`;连续门控 ≥ `stuck_after_ms` → `Stuck{Gated(bits)}`。每个 cell 每 `gated_backoff_ms` 至多产生一条被拒的意图,所以门控期间 journal 里的 `OrderDecision{reject ≠ 0}` 有界。自动有界平仓同样按本条退避重试,但不进 `Stuck`,它的失败条件只看"Broken 处置"的结束表。
- **执行失败**:交易所 `Reject`、本地发送失败 `EncodeError`、引擎单的 `RiskReject` → `fail_streak += 1`,≥ `max_slice_fails` → `Stuck{SliceFails}` + `Note{ExecStuck}`(P1),直到目标被替换或清除(L3 的每一步重新设定引擎目标,即替换)。
- **零成交**:IOC 终态且成交为 0 → `nofill_streak += 1`,≥ `max_nofill` → `Stuck{NoLiquidity}`,`stuck_after_ms` 后再试。任何成交清零两个计数。
- 有效模式变化、`CapCleared`、`venue_ready` 恢复(经 `on_mode`/`on_risk`)→ 立即重置 `wake_at` 并标记待推进。

### 运行模式、Drain 与 Unknown

- **Shadow**(含 Live 下 `open_allowed = false` 的增仓):`set_target` 照常接受(`ExecDecision{TargetSet, shadow}`);第一笔切片计算一次并交出(模式门记为 `SHADOW` 决策),随后 `Stuck{Shadow}`,在目标变化或模式变为 Live 之前不再出切片——不会每个 tick 重复同一意图。评估策略执行效果用模拟账户([hft-sim](../hft-sim/README.md)),不用 Shadow。
- **CloseOnly**:增仓切片不发(`Stuck{Gated(CLOSE_ONLY)}`);配对开仓整笔不发;减仓照常。
- **Kill(非 HARD,含 Viewer 的紧急停止)**:风控以 Kill cap 拒增仓,增仓切片按门控处理(`Stuck{Gated(bits)}`);减仓切片、OMS 的第二腿与重试、补腿、外来成交默认处理、自动有界平仓与 L3 默认减仓照常(`RISK_REDUCING` 放行)。带 `flatten` 时另有 `engine_flatten` 与 `flatten_lock`;HARD 时一切引擎订单停止(见各节)。
- **Draining**:只停增仓切片(风控以 `DRAINING` 拒 Open,按门控处理);减仓切片、进行中的配对步骤、补腿、外来成交默认处理、自动有界平仓与 L3/清仓的引擎目标照常推进(Draining 中照常接受 `Kill{flatten}`)。`exec.drain_ready()` 是 Drain 的执行层等待条件:未到达的 `reason ∈ {MarginStress, Hard}` 引擎目标使 Drain 等待,受 `drain_timeout_ms` 约束(Drain 的完整条件与超时处理归 hft-sequencer)。Drain 只等已开始(`Running`)的自动有界平仓,不等 Broken 中尚未开始的(见 `drain_ready_acct`):Drain 可能在自动有界平仓开始前完成,Broken 与其计时进快照,重启后按"Broken 处置"的计时继续。同一口径的全局读数 `engine_reduce_pending()` 供 hft-risk 推迟 StorageCritical 的 Drain(规则归 [hft-risk 存储降级](../hft-risk/README.md#存储降级storage))。
- **Unknown**:该 (account, group) 有 Unknown 槽时风控拒其 Open;本 crate 不对 Unknown 的订单做任何重发或反向,只等 OMS 收敛(`LegView.has_unknown` 时抑制补腿;`has_unknown`/`has_pending` 时推迟自动有界平仓的开始与下一片)。唯一例外是 `reason ∈ {MarginStress, Hard}` 引擎目标的永续 reduce-only 腿:Unknown 切片的剩余量按全部成交扣除后继续下一片(新槽、新 id,不重发原单;见"推进"第 4 条)。
- **恢复期**(多出那一腿所在 venue 放行前):自动处置的抑制见"人工保持与恢复期抑制";目标切片与 L3/flatten 照常受模式门与风控约束。

### 目标的接受与替换

`set_target` 校验:调用者是该账户绑定的策略实例且 group 在其 GroupSpec 中(`NotOwner`);`leg_ix < n_legs` 且不重复(`BadLegs`);现货腿目标 < 0(`SpotShort`);目标引用链上腿(`SpecInvalid`);算法参数越界——`depth_levels ∉ 1..=10`、`depth_frac ∉ (0, 1]`、`ratio_num` 或 `ratio_den` 为 0、`Fixed` 的 `first_leg_ix` 越界(`SpecInvalid`);算法与腿数匹配(`TooManyLegs`、`SpecInvalid`);cell `Broken`(`CellBroken`);默认减仓期间不比引擎目标减得更多(`ForcedReduceActive`);`flatten_lock` 期间不比当前更接近 0(`FlattenLocked`);`ro = true` 而目标会增加任一腿的 |仓位|(`ReduceOnlyCmd`,见下);配对算法的增仓目标 `pair_lot` 名义超过阈值(`LotMismatch`;减仓方向与 `Exit`/`MarginStress`/`Hard` 不做此项,见"两腿 lot 对齐"第 4、5 条)。按上列顺序取第一条不满足的原因。`reason = Exit` 的目标与其他减仓目标同样校验,不另设门槛。`TargetReject` 的线上码见"`ExecDecision.detail` 打包与子码"。

**只减仓命令下的目标**(规则唯一归属本段;`ro` 由 sequencer 在投递带 `reduce_only` 的 `StrategyCmd` 的那次 `on_event` 期间置位,命令与类别归 [hft-sequencer 命令路径](../hft-sequencer/README.md#命令路径)与 [SDK `reduce_only` 契约](../hft-strategy-sdk/README.md#策略描述符note报表策略命令)):目标列出的任一腿 `|t_i| > |pos_i|`,或 `t_i ≠ 0` 且 `sign(t_i) ≠ sign(pos_i)`(`pos_i = legs.leg[i].net`;含 `pos_i = 0` 而 `t_i ≠ 0`、穿越零)→ **整条拒绝** `TargetReject::ReduceOnlyCmd`,不部分接受、不改当前目标;每腿都满足 `t_i = 0` 或(同号且 `|t_i| ≤ |pos_i|`)才进入其余校验。`clear_target` 不受 `ro` 限制。

**相同目标不重设**:新目标与当前目标逐字段相同(各腿目标量、`reason`、`urgency`、`band`、`exec` 参数、`tag`)→ 视为已接受,不替换、不清零 `filled`、不清零失败计数、不写 `ExecDecision`(策略每个 tick 重发同一目标不会刷 journal);任何字段不同即按下文替换并记录。目标在带宽内的小幅变化是否值得重发由策略决定(策略侧规则)。**重置失败计数必须改字段**:`Stuck{SliceFails}` 只在目标被替换或清除时解除,原样重发同一目标解不开它;策略要"以相同内容重试"时改动 `tag`(例如低位放重发次数),或在同一 `Ctx` 内先 `clear_target` 再 `target`。

接受后:替换旧目标(在途切片不撤,IOC 自然终结;MakerFirst 挂单撤销),`filled` 清零,`fail_streak`/`nofill_streak` 清零,`state = Working`,标记待推进,写 `ExecDecision{TargetSet | TargetReplaced}`。替换了当前目标的 `set_target` 解除该 cell 的 `manual_hold`(逐字段相同的不解除,见"人工保持与恢复期抑制")。`clear_target`:清目标、`state = Idle`(Broken 保持 Broken),写 `ExecDecision{TargetCleared}`,不撤在途 IOC。账户处于 `flatten_lock` 或默认减仓期(`force_active`)时,`clear_target` **一律忽略**(无论当前目标是引擎目标还是替换它的、更接近 0 的策略目标),写 `ExecDecision{TargetRejected, detail = FlattenLocked | ForcedReduceActive}`,减仓继续;目标在此期间只能被更接近 0 的目标替换——否则策略先用一个 0 目标替换引擎目标、再 `clear_target`,就能停掉清仓。

### `ExecDecision` 与进度通知

`ExecDecision` 是 Output 记录(体布局与编号归 [hft-event](../hft-event/README.md)),验证回放逐字节比对。**只在有决定时写**,无动作的推进不写。每条携带:

| 字段 | 含义 |
|---|---|
| `account`、`group`、`owner` | cell |
| `action` | `TargetSet` \| `TargetReplaced` \| `TargetCleared` \| `TargetRejected` \| `Slice` \| `State` \| `HedgeFix` \| `ResidualTrim` \| `ExternalSeen` \| `ExternalDefault` \| `ForceStep` \| `CmdHedgeFix` \| `ChainLegExcess` \| `AutoFix`(判别值 14;全部编号归 hft-event) |
| `algo`、`origin`、`reason`、`urgency` | 算法(`TakerSlice` \| `PairOnFill` \| `Simultaneous` \| `MakerFirst` \| `Engine`)与目标属性;自动有界平仓的决定填 `origin = Engine(AutoFix)`(`TargetOrigin` 的线上值唯一登记在 [hft-event `ExecDecision.origin` 码表](../hft-event/README.md),只追加) |
| `state_from`、`state_to`、`detail`、`gate` | 状态迁移与原因码(`detail` 按下一节打包;门控与 `RiskReject` 的 `RejectBits` 在 `gate`) |
| `leg_ix`、`first_leg_ix`、`side`、`qty`、`px` | 本笔切片 |
| `target_qty`、`pos_qty`、`inflight_qty`、`depth_qty` | 该腿决策依据(目标、已成、即时在途、弱侧可见深度;`AutoFix` 时 `target_qty` 填 `budget`) |
| `exec_order_id`、`set_seq`、`tag` | 与 `OrderDecision`(同一 `trigger_seq`)、目标、策略标签关联 |

无组 cell 的决定 `group = GROUP_NONE`、`owner` = 该账户绑定的实例,只有 `ForceStep`、`Slice`、`State`、`TargetCleared` 四种 action。

`ExecUpdate`(SDK 类型)经 `EventRef::Exec` 投递给拥有者策略:`Progress`(每笔切片终态且有成交时一次)、`Done`(每次到达目标一条——到达后因仓位变化离开带、再次到达时再发一条;或目标被引擎清除,带 `cleared_by_engine`)、`Stuck(reason)`(进入 Stuck 与原因变化时)、`Broken(reason)`;带 `origin`(自动有界平仓的成交为 `Engine(AutoFix)`)、`tag`、各腿目标/已成/在途。执行层订单(`EXEC` 槽)的 `OrderUpdate` 只投递给本 crate,策略看 `Exec` 事件与 `Views::order`。

**冷读者重建**([ops_bot](../../python/ops_bot/README.md) 停机兜底判定等读 journal 的冷读者用;只读本 crate 的 `ExecDecision` 与 hft-risk 的 cap 注记,不读快照之外的内部状态)。重建结果不漏报:内部状态成立时重建一定成立;允许多报,多报的方向一律是"仍在处置中"(引擎目标接手自动有界平仓的那一片终态之前,重建可以多报"执行中")。策略目标(如 `Exit`)替换了引擎目标(含清仓目标)后,按该 cell 的引擎目标已结束处理——与内部状态相同:被接受的策略目标不是引擎目标。Broken 的 cell 不需要单独重建(Broken 本身已在兜底条件中)。新的外来成交开始新一轮计时(`External/<cause>/Timed`)时,上一轮截止后尚待发出的默认处理作废(由新一轮截止重新判定),所以计时中的 cell 不会先收到上一轮的 `ExternalDefault`。

| 状态 | 开始 | 结束 |
|---|---|---|
| cell 有未到达的引擎目标 | `ExecDecision{ForceStep, detail = Engine/Step \| Engine/Ungrouped \| Engine/HardAlign}` | 同一 cell 其后的 `ExecDecision{State, state_to = Idle}`、`TargetCleared` 或 `TargetReplaced` |
| 账户的清仓未完成 | 该账户 Kill cap(带 flatten)的 `CapSet` 注记 | 该账户再无"未到达的引擎目标"的 cell;锁本身随该 Kill cap 的 `CapCleared` 结束 |
| cell 的外来成交计时中 | `ExecDecision{ExternalSeen, detail = External/<cause>/Timed}` | 同一 cell 其后的 `ExecDecision{ExternalDefault}` 或 `ExecDecision{ExternalSeen, detail = External/0/Cleared}` |
| cell 的自动有界平仓执行中 | `ExecDecision{AutoFix, detail = AutoFix/Start}` | 同一 cell 其后的 `AutoFix/Done`、`AutoFix/Failed`、`ExecDecision{CmdHedgeFix}`、`ExecDecision{ForceStep}`、置位的 `Note{ExecManualHold}`,或该账户/全局 HARD 的 `CapSet`、该 group 的 `CapCleared{Broken}` |

### `ExecDecision.detail` 打包与子码

`ExecDecision.detail`(u32)的打包与全部子码的数值**唯一登记在本节**([hft-event](../hft-event/README.md) 的布局与 SDK 的枚举链接这里;只追加,不复用):

```
detail = table | code << 8 | arg << 16        // table、code:u8;arg:u16;无原因码时 detail = 0
```

| `table` | 用于 | `code` | `arg` |
|---|---|---|---|
| 0 `None` | 无原因码的决定(`TargetSet`、`TargetReplaced`、`TargetCleared`、`Slice`、`State → Idle/Working`、`ChainLegExcess` 等) | 0 | 0 |
| 1 `TargetReject` | `TargetRejected`(含被忽略的 `clear_target`) | `NotOwner=1 BadLegs=2 SpecInvalid=3 AlgoUnsupported=4 SpotShort=5 CellBroken=6 ForcedReduceActive=7 TooManyLegs=8 FlattenLocked=9 LotMismatch=10 ReduceOnlyCmd=11`(与 SDK `TargetReject` 逐值相同) | 0 |
| 2 `Stuck` | `State → Stuck` | SDK `StuckReason`:`Gated=1 Shadow=2 NoLiquidity=3 StaleQuote=4 SliceFails=5 External=6 EngineOverride=7 MinOrder=8`(`Gated` 的 `RejectBits` 在 `gate`) | 0 |
| 3 `Broken` | `State → Broken` | SDK `BrokenReason`:`LinkBroken=1 ReverseFailed=2 FixExhausted=3 HedgeVeto=4` | `LinkBroken` 时 = 链接原因(hft-oms `LinkBroken` 的值:`RetryExhausted=1 Deadline=2 HedgeLimit=3 Overfill=4 ReverseFailed=5 ResumeTimeout=6`),否则 0 |
| 4 `AutoFix` | `AutoFix` | `Start=1 Slice=2 Done=3 Failed=4` | `Failed` 时 = `AutoFixFail`(`ChainLeg=1 … RiskReject=9`,见"Broken 处置"),否则 0 |
| 5 `Engine` | `ForceStep`、`HedgeFix`、`ResidualTrim`、`ExternalDefault`、`CmdHedgeFix` | `Step=1`(设定 L3/清仓引擎目标)`SerialFallback=2` `RiskReject=3`(引擎单被 #5/#6 拒,位在 `gate`)`Ungrouped=4`(无组永续持仓的引擎目标)`CancelResting=5`(请求 OMS 撤该 inst 的非 EXEC 挂单)`HardAlign=6`(L3 回落时串行 cell 改设的对齐目标) | `SerialFallback` 时 = 原因(`StaleQuote=1 NotReady=4 ResumeHold=5 FirstUnknown=6`;2、3 保留不用);`CancelResting` 时 = 腿位图(bit i = `leg_ix` i);否则 0 |
| 6 `External` | `ExternalSeen` | `ExternalFill.cause`(`Liquidation=1 Adl=2 Manual=3 Delivery=4 Unknown=5`,与 hft-event 同值);`Cleared` 时为 0 | 处理:`Timed=1`(计时,截止后默认处理)`ManualHold=2`(Manual,不做默认处理,`manual_hold` 置位或已置位)`EngineOwned=3`(引擎目标继续)`BrokenKept=4`(保持 Broken)`Cleared=5`(计时不经截止而结束,见"外来成交的默认处理"第 2 条) |

SDK 的 `StuckReason`/`BrokenReason`/`TargetReject` 新增值时同一改动在本表追加。

**执行层订单的 `OrderDecision.reason`**(u32;hft-event 规定"执行层发出 = 本 crate 登记的码"):= `ExecOrderKind` 码,唯一登记在此、只追加:`Slice=1 PairFirst=2 SimulFirst=3 MakerFirst=4 HedgeFix=5 ResidualTrim=6 ExternalDefault=7 ForceStep=8 CmdHedgeFix=9 AutoFix=10`。`Note{ExecEngineOrderRejected}` 的 `v[2]` 用同一码。

### 定时器

`Timer{owner = Exec}`(owner 编号由 hft-event 登记),`Timer.timer_id = kind | account << 8`、`Timer.group = GroupId`(记录布局见 event-model;`on_timer` 收到的 u64 = `timer_id | group << 32`);`kind`:`Wake=1`(冷却/退避/Stuck 重试/二次投递中给出的目标)、`Fix=2`、`External=3`、`Continue=4`(全局只有一个:`account` 位为 0、`group = GROUP_NONE`)、`AutoFix=5`、`Alarm=6`(闸门滞留告警的复核时刻,见"人工保持与恢复期抑制")。每 cell 同一 kind 至多一个,改期时先撤后登。`on_timer` 一律与 cell 内保存的时刻(`Alarm` 为按"闸门滞留告警"算出的告警时刻)比较:早到(恢复后的旧定时器)只重新登记,不动作。本 crate 的定时器绑定 cell 的 group:普通 cell 是真实 group;无组 cell 用 `GROUP_NONE`(= `GroupId(0)`)——每账户只有一个无组 cell,所以键 (owner, timer_id, group) 仍唯一;从不使用 `GROUP_ALL`。

### 快照与恢复

本 crate 的快照节是节 21(编号由 [journal-format](../../docs/subsystems/journal-format.md) 分配;**行格式与行版本只在本节登记**)。`snapshot(&self, w)` 一次写完整节:每个"非空" cell 一行(有目标、待推进,或行内任一字段与新建 cell 不同——如非 Idle、`FixState` 非零、`ext_deadline ≠ 0`、`auto_fix.phase ≠ None`、`serial ≠ 0`、`hold ≠ 0`、`deferred ≠ 0`、冷却时刻、`active` 的切片序号或已登记的定时器位;只写前几项会让清除目标后的冷却与订单序号在 restore 后丢失;无组 cell 同样,键 `(account, GROUP_NONE)`,含其当前装入的腿定义),再每个 `force_active`、`flatten_lock` 或 `force_req` 非空的账户一行,再每个账户一条 Gate 行(`gate_open`、`venue_open`、`gate_closed_tsc`:验证回放与 advisor 镜像从快照值继续;恢复驱动在 `recovery_end` 时清零并重设);全部小端、按键升序。`pending` 位随 cell 行写出(因输出容量停下、等 `Timer{Exec, Continue}` 的 cell 在事件边界仍待推进);`pair_lot` 不进快照(restore 时由目标与 refdata 重算)。含枚举字段的行用 `zerocopy::TryFromBytes` 读回,非法判别值 → `RestoreError`。

**目标按逐字段的显式行写出,不原样写 SDK 类型**:SDK 的 `ExecSpec` 是 `#[repr(C, u8)]` 数据枚举,判别字节后的 7 B 是隐式填充,较小的变体在 128 B 载荷里留下未初始化的尾部,原样写进快照会使 `state_hash` 不确定,也派生不了 `IntoBytes`。所以 cell 行之后接 `ExecTargetRow`:字段顺序与偏移同 SDK `TargetIntent`,只把 `exec` 换成显式的 `ExecSpecRow`。写出时逐字段拷贝,全部 `_pad` 与 `params` 的尾部写 0;读回时 `algo` 必须是合法的 `ExecSpec` 判别值(`has_target = 0` 时整行为 0),按 `algo` 用 `TryFromBytes` 读对应参数结构,尾部非零 → `RestoreError`。

```rust
// 节 21 ExecTargets,行版本 1(草案,实现时以代码与编译期断言为准)。每行以 ExecRowHdr 开头,顺序 Cell* → Acct* → Gate*。
#[repr(C)] pub struct ExecRowHdr { pub kind: u8 /* Cell=1 Acct=2 Gate=3 */, pub _pad: [u8; 3], pub len: u32 }   // 8 B;len = 行体字节数
#[repr(C)] pub struct LegRef {                   // 32 B;无组 cell 写当前装入的腿,普通 cell 由 GroupSpec 重建、行内全零
    pub venue: VenueId, pub kind: u8 /* Spot=1 LinearPerp=2 Chain=3 */, pub reduce_only_cap: u8, pub _pad: u8, //  0
    pub inst: InstId,                                                                                          //  4
    pub lot: Qty, pub min_qty: Qty,                                                                            //  8 / 16
    pub slot: u32, pub _pad2: u32,               // 24 无组 cell:该腿被跟踪切片的 OMS 槽号(TargetCell.slot);min_notional 由 refdata 读
}
#[repr(C)] pub struct ActiveSliceRow {           // 32 B
    pub kind: u8,                                //  0 None=0 Slice=1 Pair=2 Simul=3 Maker=4 Serial=5 AutoFix=6 Force=7
    pub leg_ix: u8, pub n: u16,                  //  1 / 2
    pub link_id: u32,                            //  4 0 = 无链接
    pub first: OrderRef,                         //  8 12 B(SDK)
    pub ext: [u8; 4],                            // 20 [订单 kind 码, 在途腿位, 卖出腿位, 已有成交腿位](TakerSlice 与串行腿每腿一笔)
    pub planned: Qty,                            // 24
}
#[repr(C)] pub struct ExecCellRow {              // 400 B;其后是 ExecTargetRow(232 B),ExecRowHdr.len = 632
    pub account: AccountId,                      //   0
    pub state: u8,                               //   1 TargetState 线上码(码表归 hft-event ExecDecision.state_from)
    pub stuck: u8,                               //   2 StuckReason 码(detail 表 2);0 = 无
    pub broken: u8,                              //   3 BrokenReason 码(detail 表 3);0 = 无
    pub group: GroupId,                          //   4 GROUP_NONE = 无组 cell
    pub owner: StrategyInstanceId,               //   8
    pub n_legs: u8, pub has_target: u8,          //   9 / 10
    pub origin: u8,                              //  11 TargetOrigin 线上码(hft-event ExecDecision.origin 码表)
    pub flags: u8,                               //  12 PENDING=1 SERIAL=2 MANUAL_HOLD=4 LINK_HOLD=8 MANUAL_HOLD_DEFERRED=16
    pub deferred: u8,                            //  13 ENGINE_SET=1 ENGINE_CLEARED=2 CLEARED_BY_CMD=4 STATE=8 TIMERS=16 ALARM=32 HOLD_REL=64 EXT_CLR=128
    pub cap_block: u8,                           //  14 BROKEN=1 REORG=2
    pub fix_count: u8,                           //  15
    pub fail_streak: u8, pub nofill_streak: u8, pub fund_shrink: u8,
    pub def_from: u8,                            //  19 待写 State 决定(deferred 的 STATE 位)的 state_from 码;0 = 无
    pub gated_bits: u32,                         //  20 RejectBits
    pub n_slices: u32,                           //  24
    pub force_steps: u16,                        //  28
    pub aux16: u16,                              //  30 cell 内部位:0–7 已登记的定时器 kind,8–11 Unknown 腿,12–13 链接残余策略,14 截止后待发默认处理
    pub set_seq: u64,                            //  32
    pub set_tsc: Tsc, pub last_slice_tsc: Tsc, pub wake_at: Tsc, pub gated_since: Tsc, pub stuck_since: Tsc, //  40..80
    pub fix_first_tsc: Tsc, pub fix_next_tsc: Tsc, pub ext_deadline: Tsc, pub broken_since: Tsc,          //  80..112
    pub tolerance: Rate,                         // 112
    pub filled: [Qty; 4],                        // 120 普通 cell:本目标以来已成;无组 cell:已释放的 Unknown 切片剩余量
    pub own_infl: [Qty; 4],                      // 152 无组 cell:own_infl;普通 cell:各腿在途切片已成交量
    pub active: ActiveSliceRow,                  // 184
    pub auto_fix: AutoFixState,                  // 216 56 B(见"目标单元")
    pub legs: [LegRef; 4],                       // 272
}
#[repr(C)] pub struct ExecAcctRow {              // 24 B;键 account
    pub account: AccountId, pub force_active: u8, pub flatten_lock: u8,
    pub force_kind: u8,                          //  3 0 = 无 force_req,Venues=1(按比例,venue 位图与 frac 有效),Flatten=2
    pub force_flags: u8, pub _pad: [u8; 3],      //  4 FULL=1:清仓请求覆盖全部 cell
    pub force_venues: u64,                       //  8 VenueId 位图
    pub force_frac: Rate,                        // 16
}
#[repr(C)] pub struct ExecSpecRow {              // 136 B
    pub algo: u8,                                //   0 SDK ExecSpec 的判别值(与 hft-event ExecDecision.algo 同码)
    pub _pad: [u8; 7],                           //   1 写 0
    pub params: [u8; 128],                       //   8 该变体的参数结构(SDK TakerSliceParams / PairParams / SimulParams / MakerParams,
                                                 //     都是无隐式填充的 repr(C),大小以 SDK 为准)的字节,其后补零到 128 B
}
#[repr(C)] pub struct ExecTargetRow {            // 232 B;has_target = 0 时全零
    pub account: AccountId, pub _pad0: [u8; 3],  //   0
    pub group: GroupId,                          //   4
    pub legs: [LegTarget; 4],                    //   8 SDK LegTarget(16 B;其 _pad 写 0)
    pub n_legs: u8, pub reason: u8, pub urgency: u8, pub _pad1: [u8; 5],   //  72 / 73 / 74 / 75;reason、urgency 码表 = SDK TargetReason / Urgency(与 hft-event ExecDecision 同码)
    pub band: Rate,                              //  80
    pub exec: ExecSpecRow,                       //  88
    pub tag: u32, pub _pad2: u32,                // 224 / 228
}
#[repr(C)] pub struct ExecGateRow {              // 24 B;每账户一行,键 account
    pub account: AccountId, pub gate_open: u8, pub _pad: [u8; 6],
    pub venue_open: u64,                         //  8 VenueId 位图
    pub gate_closed_tsc: Tsc,                    // 16
}
const _: () = assert!(size_of::<ExecRowHdr>() == 8 && size_of::<LegRef>() == 32 && size_of::<ActiveSliceRow>() == 32
                      && size_of::<ExecCellRow>() == 400 && size_of::<ExecAcctRow>() == 24 && size_of::<AutoFixState>() == 56
                      && size_of::<ExecSpecRow>() == 136 && size_of::<ExecTargetRow>() == 232 && offset_of!(ExecTargetRow, exec) == 88
                      && size_of::<ExecGateRow>() == 24);
```

**进快照的 TSC 字段**(本 crate 的已登记行;是 [crates/AGENTS](../AGENTS.md)"快照内时刻用 engine_ns"规则的登记例外,其余进快照的时刻一律 engine_ns):`set_tsc`、`last_slice_tsc`、`wake_at`、`gated_since`、`stuck_since`、`fix.first_tsc`、`fix.next_tsc`、`ext_deadline`、`broken_since`、`auto_fix.due_tsc`、`auto_fix.started_tsc`、`auto_fix.run_mark_tsc`,以及 Gate 行的 `gate_closed_tsc`。cell 行的这些字段只在 `recovery_end` 按下式平移,不按 `boot_id` 或 UTC 剩余时长重建;`gate_closed_tsc` 由 `recovery_end` 重新设为 `now`,不平移。`auto_fix.exec_ms` 是时长(ms),不平移。

`restore` 只装载,不改 TSC 量;尾部重放用的是旧运行的事件 tsc,与快照内的值同一次运行,必须原样比较,否则重放结果会偏离 journal。**恢复结束**时由 `recovery_end(now, stop_tsc, hz_prev, hz_cur)` 统一平移(停机时间不计;同开机、跨开机与换机同一规则,与 [hft-sequencer 定时器](../hft-sequencer/README.md#定时器)堆内条目的跨重启平移同一口径):

- `now` = 本次运行的 `TscClock` 读数(恢复驱动取一次,同一值写入 `purpose = Recovery` 快照头;不是日志里的记录时刻);`stop_tsc` 见入口注释;`hz_prev`、`hz_cur` = 上次与本次运行的 `tsc_hz_milli`。
- 时长换算 `conv(Δ) = Δ × hz_cur / hz_prev`(i128 中间积,向零取整;即先按上次运行的频率换成 ns,再按本次运行的频率换回 tick;同频率时 = Δ)。
- 过去的时刻 `t ≤ stop_tsc` → `max(now − conv(stop_tsc − t), 1)`;将来的时刻 `t > stop_tsc` → `now + conv(t − stop_tsc)`;0 保持 0(0 = 未设;平移结果至少为 1,不会被读成未设)。规则按值判定,不按字段:已过期未处理的截止(如 `wake_at ≤ stop_tsc`)落到 `now` 之前,恢复后立即到期。
- 全部账户 `gate_open = false`、`venue_open = 0`、`gate_closed_tsc = now`(恢复期抑制,见"人工保持与恢复期抑制");`active.link_id ≠ 0` 的 cell 置 `LINK_HOLD`,收到该链接不带 `RESUME_HOLD` 的 `LinkNotice` 或链接终态时清除。
- 堆里的旧定时器由 sequencer 按同一口径处理;本 crate 经 `recovery_timers()` 按平移后的时刻重新登记全部定时器,旧定时器早到时 `on_timer` 只重新登记。

### 规划源文件

| 文件 | 职责 |
|---|---|
| `src/lib.rs` | `Exec`:cell 表(按 (account, group) 升序,其后每账户一个无组 cell)、inst → cell 索引、待推进位图、`on_groups_changed`、只读入口、决定/Note/通知/定时器的公共构造 |
| `src/types.rs` | `TargetCell`、`AutoFixState`、`AcctState`、`ExecOrder`、`CancelResting`、`LinkNotice`、`ExecConfig` 行、`AutoFixFail`、快照行 `ExecRowHdr`/`ExecCellRow`/`ExecAcctRow`/`ExecGateRow`/`ExecTargetRow`/`ExecSpecRow`/`LegRef`/`ActiveSliceRow`(`StuckReason`、`BrokenReason`、`TargetReject`、`TargetState` 用 SDK 类型) |
| `src/out.rs` | `ExecOut`、`FixedVec`、`OutCaps`(README 容量与单 cell 上限) |
| `src/calc.rs` | 定点计算:限价(调用 hft-types `cap_cross_px`)、按类别的单笔名义与计价资产换算、不交易带、可见深度(≤ 10 档,整数)。`pair_lot(lot_f, lot_h, num, den) -> Option<Qty>`(溢出 → None,按 `LotMismatch` 处理)本身是 [hft-types](../hft-types/README.md) 数量取整模块的纯函数:本 crate、sequencer 的 SetParams 校验与直接下 LegLink 单的策略共用这一份实现;规则语义归本 README |
| `src/targets.rs` | `set_target`/`clear_target`/`clear_account`/`note_strategy_action`、接受校验(含更接近 0 与 `LotMismatch`)、`pair_lot` 重算 |
| `src/advance.rs` | `advance` 主流程、带与紧急度、穿越零、报价可用与预检、TakerSlice/PairOnFill/Simultaneous 与腿顺序、`pair_min`、对冲量上限、输出容量检查与续推、`deferred` 记录的写出 |
| `src/events.rs` | `on_order_update`、`on_place_result`、`on_link`、`on_timer`、`on_risk`、`on_mode`、`touch_inst`、`on_leg_adjust` |
| `src/fix.rs` | 补腿、残余与尘埃、链上腿规则、命令补腿 |
| `src/broken.rs` | Broken 处置:自动有界平仓的计时、等待条件、可执行时长、切片、重试与结束;HARD 解除时的 `resolve_broken` 判定 |
| `src/external.rs` | 外来成交默认处理(按条件表:Broken、引擎目标、Manual、计时)、`manual_hold` 的置位、延迟置位与解除、按 venue 的恢复期抑制与 `on_venue_gate_open`/`on_gate_open` 的截止重设、闸门滞留告警 |
| `src/force.rs` | `force_req` 登记与引擎目标设定(含更接近 0 的取值与 `cancel_resting` 请求)、默认减仓、`engine_flatten`、串行腿(Broken 与 `SerialFallback`,含 `LINK_HOLD`)、L3 回落的对齐目标、Unknown 切片扣量继续、无组 cell 的装入与扫描、`flatten_lock` 的置位与解除 |
| `src/decision.rs` | `ExecDecision`、`detail` 打包与子码、`ExecOrderKind` 码、`ExecUpdate`、`Note` 码表(const 导出) |
| `src/snapshot.rs` | 快照/恢复(含 `ExecTargetRow` 的逐字段写出与补零、Gate 行)、`recovery_end` 平移、`recovery_timers` |
| `tests/*.rs`、`benches/exec.rs` | 用例的落点见 [TESTS.md](TESTS.md) 各用例的"实现"字段(DST 场景在 hft-testkit) |

</details>

## 延迟与资源契约

分段:`decided_tsc − proc_tsc` 内的执行层份额(整段基准形态 1–5 µs,[00 §4](../../design/00-目标与延迟预算.md));测量:Criterion(热缓存、单核)+ 实盘 `LatencySample` 分段直方图。`exec_step` 是 [hot-path-rules S4](../../docs/hot-path-rules.md) 引用的基准组。

| 入口 | p50 | p99 | 基准名 |
|---|---|---|---|
| `touch_inst`,该 inst 无活动 cell(绝大多数 tick) | ≤ 8 ns | ≤ 20 ns | `exec_step/idle` |
| `advance` 推进一个活动 cell、无切片 | ≤ 60 ns | ≤ 150 ns | `exec_step/hold` |
| `advance` 推进一个活动 cell 并产生一笔配对第一腿(2 腿、各 ≤ 10 档深度) | ≤ 300 ns | ≤ 800 ns | `exec_step/slice_pair` |
| `advance` 推进一个 Broken cell 并产生一片自动有界平仓 | ≤ 200 ns | ≤ 500 ns | `exec_step/auto_fix` |
| `set_target`(校验 + 登记,含"更接近 0"与 `pair_lot`) | ≤ 80 ns | ≤ 200 ns | `exec/set_target` |
| `on_order_update` / `on_link` | ≤ 40 ns | ≤ 100 ns | `exec/on_link` |
| `on_external`(含 `manual_hold` 置位与 Note) | ≤ 60 ns | ≤ 150 ns | `exec/on_external` |
| `on_risk` / `on_gate_open` / `on_venue_gate_open`(一个账户 64 个 cell) | ≤ 300 ns | ≤ 1 µs | `exec/on_risk_64` |
| `resolve_broken`(HARD 解除时每个候选 group 一次;不在 tick 路径) | ≤ 30 ns | ≤ 80 ns | `exec/resolve_broken` |
| `drain_ready` / `engine_reduce_pending`(全部账户共 64 个 cell;Drain 等待与存储推迟期间每轮一次) | ≤ 200 ns | ≤ 600 ns | `exec/drain_ready_64` |
| L3 一步的目标设定(`force_deleverage` + `advance` 设定),一个账户 8 个 cell(含 Broken) | ≤ 1 µs | ≤ 3 µs | `exec/force_step_8` |
| 无组永续持仓扫描(该 venue 512 个永续 inst;只在设定 L3/清仓目标时) | ≤ 5 µs | ≤ 15 µs | `exec/force_ungrouped_512` |
| 限价与单笔名义(`cap_cross_px` + `cap_n`,每笔订单一次) | ≤ 10 ns | ≤ 25 ns | `exec/price_cap` |
| `recovery_end`(恢复路径,1 024 cell:高于各主机类的 `max_cells_total`,作上界) | — | ≤ 200 µs | `exec/recovery_end_1024` |
| `set_target` 中的 `pair_lot` 重算(含 gcd/lcm,i128;函数在 hft-types) | ≤ 30 ns | ≤ 80 ns | `exec/pair_lot` |

- 第一腿成交 → 第二腿出环(主机内 ≤ 20 µs,OMS 份额 ≤ 1 µs)不经过本 crate。
- 零分配(`new`/`restore` 与命令路径 `on_groups_changed` 之外);无系统调用;无锁、无原子量;无 f64(比例运算为 `Rate` × `Qty` 的 i128 中间积);无 `HashMap`、无 `dyn`。只被 sequencer 线程触碰。
- 每个 Input 的 `advance` 工作量有界:待推进 cell 数 × O(腿数),且受"输出容量与续推"的单次上限约束(每次至多 8 条订单),其余经 `Timer{Exec, Continue}`(T10)。
- 内存:cell ≤ 640 B(无组 cell 的 `own_infl` 与 `filled` 共用存储);cell 表 = (`max_cells_total` + 账户数) × 640 B + 索引与位图(每 cell ≈ 36 B),`max_cells_total` = 256 时 ≈ 170 KiB(各主机类的取值与内存合计见 [apps/hft-core](../../apps/hft-core/README.md#内存与-journal-段按主机类唯一归属))。

## 测试与验收

默认关闭的 `testkit` feature 复制强制目标的仓位、前一目标、原始缩减比例与实际保存的目标,以及自动平仓计时触发与开始门控的只读诊断;三类缓冲各为 `max(8, cell_count × 8)`,只在构造时分配,溢出显式失败,不进快照与生产构建。跨组件验收归 [TK-109](../hft-testkit/TESTS.md#tk-109-真实保证金强制目标独立重算)。

用例与验收见 [TESTS.md](TESTS.md)。

## 依赖

- 内部:`hft-types`(`Px`/`Qty`/`Rate`/`Notional`、取整、`pair_lot`、`cap_cross_px` 与 `HedgePxLimits`、`RejectBits`、ID、`GROUP_NONE`、无分配容器)、`hft-refdata`(`InstMeta`、`VenueCaps`、`EngineConfig` 容器)、`hft-event`(`ExecDecision`、`Note`、`Timer` 编码)、`hft-strategy-sdk`(`TargetIntent`、`ExecSpec` 与参数、`OrderIntent`、`LegLink`/`HedgePlan`/`LinkRole`/`ResidualPolicy`、`OrderRef`、`OrderUpdate`、`ExternalFillView`、`RiskEvent`、`TargetReason`、`TargetReject`、`TargetState`、`TargetOrigin`、`StuckReason`、`BrokenReason`、`Views` 与视图类型 `LegView`/`OrderView`/`PortfolioView`/`PerpPosView`/`GroupPosView`/`RiskStateView`、`ExecUpdate`、`TargetView`、`GroupTable`)。
- 外部:无。
- 禁止:`hft-oms`、`hft-ledger`、`hft-risk`(L1 互不依赖;它们的状态经 SDK 视图、`ExternalFillView` 与本 crate 定义的 `LinkNotice` 进入)、`hft-sequencer`、`hft-journal`、`hft-rt`、`hft-strat-lib`、任何 `strategies/*`、`hft-venue`、`venue-*`、I/O、`std::time`、f64、`HashMap`、`dyn Trait`。层次:本 crate 是 L1,只依赖 L0 + SDK;`hft-sequencer` 依赖本 crate。

## 延伸阅读

- [容量续推不提前启动补腿窗口](../../.agents/notes/implemented/bug-fix/2026-10-11-repair-window-after-backoff.md)

- [07 策略层:执行算法与两层输出](../../design/07-策略层.md)、[05 CEX 执行层 §2.3 多腿绑定](../../design/05-CEX执行层.md)、[09 风控与控制面(保证金分档)](../../design/09-可观测性、风控与控制面.md)、[12 风险与未决问题](../../design/12-风险与未决问题.md)
- [第八轮裁决(含 R8-8 断腿处置、R8-11 人工保持与引擎兜底、R8-12 第五轮评审的引擎兜底修正)](../../design/_work/round8-decisions.md);Agent Note [manual-hold-and-stop-marker](../../.agents/notes/proposed/architecture/2026-10-03-manual-hold-and-stop-marker.md)、[resolve-broken-hard-only](../../.agents/notes/proposed/architecture/2026-10-03-resolve-broken-hard-only.md)
- [hft-strategy-sdk](../hft-strategy-sdk/README.md)、[hft-oms](../hft-oms/README.md)、[hft-risk](../hft-risk/README.md)、[hft-ledger](../hft-ledger/README.md)、[hft-sequencer](../hft-sequencer/README.md)、[hft-sim](../hft-sim/README.md)
- [defensive-patterns](../../docs/defensive-patterns.md)、[hot-path-rules](../../docs/hot-path-rules.md)、[control-plane](../../docs/subsystems/control-plane.md)

## 已知限制与后续工作

- 配对算法只支持两腿;3–4 腿目标只能用 `TakerSlice` 各腿独立推进(无 LegLink)。可选扩展:多腿链式 OnFill(触发条件:出现需要三腿同时对冲的策略)。
- 目标不支持链上腿;链上腿的多出部分只告警不处理(策略负责)。可选扩展:链上腿纳入执行层(触发条件:链上腿策略迁移到新框架并需要目标式执行)。
- 对冲比 `ratio` 是目标内静态整数比;按实时资金费或基差动态调整不支持。
- `ExecConfig` 改动需要 Drain + 重启。可选扩展:并入可热替换的参数(触发条件:上线后需要频繁调整补腿阈值、截止、`broken_auto_fix_ms` 等)。
- 自动有界平仓每个 Broken 期只做一次可执行的尝试,失败后只能人工处理;它平的是"多出的一腿",不恢复对冲(不补开缺的那条腿),组合因此可能比策略意图小。等待条件(多出腿所在 venue 未放行、报价不可用、限速退避、对手价越过上限、可用余额被锁定)长期不消失时尝试一直不开始,到期后等待超过 `gate_naked_alarm_ms` 写 `ExecGateHeldNaked` P0,此外由对应的连接与风控告警兜底;夜间失败时依赖 P0 告警叫人。
- `manual_hold` 不超时:人工在网页动过的 group,直到策略给出非清除类动作、人工 `HedgeFix` 或引擎目标之前,另一腿的裸露不会被自动处理(`ExecManualHold` P1 提示)。这是"人工接手即由人负责"的取舍。
- 重启时各所连接就绪的先后可能让 L3 中的 cell 进入 `SerialFallback`(另一所报价未就绪或链接仍在 `RESUME_HOLD`),代价是短时现货裸多与一条 P0。不为 `SerialFallback` 另设启动宽限:重启后 L3 的第一步本来就要等本次运行的新保证金报告或新标记价确认([hft-risk"重启后的 L3 出步要本次运行的数据确认"](../hft-risk/README.md#保证金分档marginstress)),这已足够;L3 期间再额外等另一所的风险更大。
- 不做按时间的切片(TWAP)、不按资金费结算时刻避让;策略用 `urgency`、`cooldown_ms` 与目标更新时机表达。
- 默认减仓对该 venue 全部永续腿按同一比例减,不按各 group 的收益或成本排序;精细选择应由策略在 L2 截止前完成。Broken 的 cell 按同一比例减时保持原有的不平衡比例,剩余裸量交自动有界平仓或人工。
- `MakerFirst` 为 optional(触发条件见该节)。
- `flatten_lock` 只由 `Resume{Kill}` 解除:清仓完成后该账户的策略在人工解除前不能重新开仓,这是有意的(清仓是人工决定,恢复也须人工)。
- 两腿 lot 不能整除且 lcm 名义过大的组合不做增仓("按名义近似对冲 + 残余修剪"只用于减仓与风险目标);这类组合需要换市场或调大切片上限。
- "报价可用"依赖 BBO 连接按 `bbo_ping_ms` 有回帧;某所在安静期既不推数据也不回 pong 时,该所的报价按不可用处理(门控等待,不会误下单),表现为对冲与减仓延后,由 `hedge_deadline_ms` 与 P0 告警兜底。
- M2 实现未含:`replay verify` 回放用例(EXE-464,依赖 replayer 与 `fixtures/replay/exec/`)与 DST 级场景(EXE-500–EXE-505,依赖 hft-testkit 的 DST 框架),随各自依赖的里程碑补上。
- `SerialFallback` 以短时现货裸多换永续减仓不停摆:另一所长时间不可用时现货腿一直等报价恢复,裸多期间价格下跌的损失由人工处理(P0 已报)。
- 无组永续持仓每次至多装入 4 条腿;同一账户有更多无组永续持仓时分批减完,前一批到达目标后才换入下一批。
- 越价上限 `rr_max_cross`(按 inst 价格带类别)是风控对风险递减单的硬界:对手价已越过 mid × (1 + `rr_max_cross`) 时引擎单按该上限挂 IOC,吃不到就零成交等待,不会为了成交而突破风控界;L3 可另启用更宽的 `l3_max_cross`(默认关)。

### 开发备忘

<details><summary>给实现者</summary>

- **植入缺陷(feature `planted`,只在测试中)**:`src/planted.rs` 按线程打开的开关:恢复丢失 `set_seq`(`snapshot-field`)、Broken 从不自动平仓或终态后再次开始(`broken-never-autofix`、`broken-repeat-autofix`)。只由 hft-testkit 的 `planted` feature 打开,生产清单、`seq` 与 `dst-bin` 都不含它([TK-178](../hft-testkit/TESTS.md#tk-178-植入缺陷不进入任何生产构建));真实引擎 DST 发现各缺陷的验收归 [TK-179](../hft-testkit/TESTS.md#tk-179-真实引擎的植入缺陷在有界种子内被不变量发现) 与 TK-053.21–TK-053.23。
- **待核实**(方法:各所官方文档 + 测试网/模拟盘小单,结果写进对应 `venue-*` 与 `VenueCaps`):① reduceOnly 订单是否豁免最小名义/最小数量(影响"目标为 0 的最后一腿"、补腿、自动有界平仓与无组持仓减仓的可下单性);② 现货卖出时手续费扣哪个资产(决定 `group 库存` 与可用余额哪个是上限);③ 各所 IOC 对"超出可见深度"的处理是否一致(部分成交后余量撤销)。
- **开放问题**:`SetParams` 的 `LotMismatch` 用引擎切片上限、`set_target` 用策略切片上限,两者取值不同时增仓目标仍可能在 `SetParams` 放行后被拒(见"两腿 lot 对齐"第 5 条);若模拟账户运行中频繁出现,考虑让策略在参数里声明切片上限、由 `SetParams` 一并校验。
- 默认值标"待标定"的项在模拟账户运行期间按报告标定,上线前写回本表。
- 实现顺序:types/index → 限价与单笔名义 → TakerSlice + 带 → PairOnFill(依赖 OMS OnFill)→ 补腿/残余 → 外来成交 → 默认减仓(含 Broken 与 `SerialFallback` 的串行腿、无组 cell)→ Broken 处置 → Simultaneous → 快照与 `recovery_end` → MakerFirst(optional)。
</details>
