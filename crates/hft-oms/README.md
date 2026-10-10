---
description: "sequencer 线程在每个意图通过风控后、每条回报到达时调用它:分配订单槽与 client order id、推进订单状态机、维护 LegLink 链接与 LegTracker,并在第一腿成交的同一事件内按成交净量产生第二腿请求"
kind: crate
tier: hot
scope: core
status: spec
---

# hft-oms

## 概要

`hft-oms` 拥有**所有订单的引擎视角状态**:每 (venue, account) 一张预分配的 `OrderSlot` 数组、空闲表与墓碑、`ClientOrderId` 的分配语义、订单状态机与 Unknown 收敛、多腿绑定的**机制**(`LinkTable`、OnFill/Simultaneous 第二腿、重试计时、恢复后的对冲续接与续接超时)、`LegTracker` 敞口账本(在途分即时与挂单两类)与在途量账本(`InflightBook`,含风控可负担性所需的增量和);另有三个由命令或执行层触发的机制:Admin 冒烟单(`SmokeOrder`)的建槽、校验与到期自动撤单,执行层设引擎目标前的按 inst 撤挂单入口 `cancel_resting_on`,以及残余核销 `write_off_residual` 对 LegTracker 的调整。它**不决定下什么单**:目标切片、补腿(HedgeFix)、残余与尘埃规则、外来成交、断腿(Broken)与保证金危急的默认处理归 [hft-exec](../hft-exec/README.md);它也不拥有仓位与 PnL([hft-ledger](../hft-ledger/README.md))、分类与风控判断([hft-risk](../hft-risk/README.md))、帧编码与连接([hft-venue](../hft-venue/README.md) 的 gw 线程与 `venue-<x>`)。唯一调用方是 [hft-sequencer](../hft-sequencer/README.md),在 tick-to-trade 的 t3(意图 → `OrderDecision`)与 t5(回报 → 迁移 → 第二腿)两处。它是纯状态机:无 I/O、无时钟(时间都来自调用参数里的事件 `tsc`),所以同一份代码跑实盘、模拟账户、恢复、`replay verify` 与 DST。

## 目录

- [使用本组件](#使用本组件)
- [实现说明](#实现说明)
- [延迟与资源契约](#延迟与资源契约)
- [测试与验收](#测试与验收)
- [依赖](#依赖)
- [延伸阅读](#延伸阅读)
- [已知限制与后续工作](#已知限制与后续工作)

## 使用本组件

### 与 hft-exec 的边界

| 归 OMS(机制) | 归 hft-exec(决定) |
|---|---|
| 槽分配、client id、状态机、Unknown 收敛、墓碑与迟到成交 | 目标跟踪、切片大小、腿顺序、何时下下一笔 |
| LegLink:第一腿每个成交增量在同一事件内按净量产生第二腿(不超过该方向裸量);第二腿失败的 `SecondRetry` 计时、计次、截止与 Broken;恢复后续接单所在 venue 的对冲闸门子集满足时的一次续接重试与 `resume_hold_max_ms` 续接超时;`Simultaneous` 的同发与反向 | 自动补腿 `HedgeFix`(只平掉多出的一腿)及其计次、冷却、截止与 HARD 下抑制;Broken 处置(自动有界平仓、L3/flatten 覆盖 Broken) |
| LegTracker:各腿净量、即时在途与挂单在途、按 group 累计的余数、naked 与该方向裸量;抑制条件中属于订单侧的部分(`LegView` 的 `open_links`、`has_unknown`、`has_pending`、各腿 `inflight_immediate`);残余核销对 LegTracker 的调整 | 补腿阈值、尘埃线、残余策略(`Defer`/`TrimExcess`/`Hold`)——全部是参数;`manual_hold` 与恢复期的自动处置抑制 |
| 按 (account, inst) 撤非执行层挂单的机制 `cancel_resting_on`;冒烟单的建槽、校验与到期撤单 | 何时撤(设 MarginStress/Hard 引擎目标时)与撤单确认前的截量 |
| 按 hft-ledger 对外来成交(`ExternalFill`)的归属结果更新 LegTracker;外来成交与槽成交的去重 | 外来成交无人处理时的默认处理、保证金 L3 的默认减仓 |
| 对冲腿定价 `price_hedge`(报价按连接健康判定可用、限价用 hft-types `cap_cross_px`、超滑点按越价上限定价);为本 crate 的对冲请求填 `hedge_cap_qty`;第二腿数量按第二腿 lot 向零取整、零头进 group 余数;本地发送失败是否计次的分类 | 对冲量上限的规则(检查在 hft-risk 检查链 #6 `HEDGE_LIMIT`);第一腿数量按两腿 `pair_lot` 取整的规则([两腿 lot 对齐](../hft-exec/README.md#两腿-lot-对齐));引擎单与切片的限价和单笔名义规则([hft-exec 限价与单笔名义](../hft-exec/README.md#限价与单笔名义)) |

### 入口(全部在 sequencer 线程调用,`&mut self`,无分配)

```rust
pub struct Oms { /* 见实现说明 */ }

impl Oms {
    /// 启动期按 refdata 的 (venue, account) 表预分配;之后永不分配(refdata 不热替换:变更经 `ReplaceRefdata` = Drain + 重启)。
    pub fn new(cfg: &OmsConfig, rd: &RefData) -> Result<Oms, OmsConfigError>;
    /// SetParams 应用后(命令路径):按 GroupSpec 重建 (account, inst) → (group, leg_ix) 归属表与 LegTracker 的腿定义;容量预分配。
    pub fn on_groups_changed(&mut self, groups: &GroupTable);

    /// 意图已通过风控(策略意图与执行层 Target 单另经模式门;Engine 单、OMS 对冲单与冒烟单不经模式门)。
    /// 分配槽、生成 client id、写出 OrderDecision(调用方负责写 journal 与推出环);冒烟单另按"冒烟单"第 3 条建槽。
    /// 返回 Err 时槽、InflightBook、LegTracker 与链接表都不变;唯一例外:本 crate 产生的对冲请求(origin = Oms)失败时,
    /// 其链接按 OnFill 第 6 条记一次执行失败(计次、登记 HedgeRetry)。
    pub fn place(&mut self, it: &ApprovedIntent, now: Tsc, cause_seq: u64,
                 out: &mut OmsOut) -> Result<SlotRef, PlaceError>;
    /// 拥有者(策略或执行层)的撤单与改单;trig = 触发本次决策的事件的时间戳,供延迟样本(见"延迟样本")。
    pub fn cancel(&mut self, s: SlotRef, trig: &TrigStamps, now: Tsc, cause_seq: u64, out: &mut OmsOut) -> Result<(), OpError>;
    pub fn amend(&mut self, s: SlotRef, px: Px, qty: Qty, trig: &TrigStamps, now: Tsc, cause_seq: u64, out: &mut OmsOut) -> Result<(), OpError>;
    /// Kill / Drain / CancelAll 命令:撤范围内可撤的槽;open_only = true 时只撤其中非 RISK_REDUCING 的(非 HARD 的 Kill)。
    /// open_only 由 sequencer 从 hft-risk `ModeOutcome` 的撤单项原样转交;槽的状态范围与批量 CancelAll 的条件见"撤单范围与批量撤单"。
    pub fn cancel_all(&mut self, scope: CancelScope, open_only: bool, now: Tsc, cause_seq: u64, out: &mut OmsOut);
    /// 有效模式降到 CloseOnly / Shadow 时由 sequencer 调用:撤该 cell 的 Open 类挂单(不碰 RISK_REDUCING 单)——
    /// 即 SlotExt.class = Open、无 RISK_REDUCING 位、处于可撤状态的 GTC/PostOnly 槽,逐槽撤单。
    pub fn cancel_open(&mut self, cell: CellKey, now: Tsc, cause_seq: u64, out: &mut OmsOut);
    /// 执行层设 MarginStress / Hard 引擎目标前(经 ExecOut.cancel_resting,由 sequencer 调用):撤账户 a 在 inst 上的全部非 EXEC 挂单
    /// (策略与冒烟单的 GTC/PostOnly,不论是否 RISK_REDUCING);不是持续义务,执行层每设一次引擎目标请求一次。O(该 inst 上本账户的非终态槽数)。
    pub fn cancel_resting_on(&mut self, a: AccountId, inst: InstId, now: Tsc, cause_seq: u64, out: &mut OmsOut);
    /// HARD 解除时由 sequencer 调用:返回此刻已成交裸量非 0 的 (account, group) 作为候选;是否设 `Broken` cap 由
    /// hft-exec `resolve_broken` 逐个判定([HARD 解除时的 Broken 判定](../hft-exec/README.md#hard-解除时的-broken-判定resolve_broken));不产生任何订单。
    /// 只读、按 group 升序;out 满时返回 true,sequencer 以同一游标再调直到 false(一个 Input 内完成,不丢弃)。
    pub fn naked_groups(&self, a: AccountId, out: &mut FixedVec<GroupId, 64>, cursor: &mut u32) -> bool;
    /// 输出扇出:任一输出向量满时其余工作项留在待发队列(状态,进快照),见"输出扇出"。
    pub fn wants_continue(&self) -> bool;
    pub fn on_continue(&mut self, now: Tsc, cause_seq: u64, out: &mut OmsOut);   // Timer{OmsContinue}
    /// 撤单持续义务:带 cancel_resting 的 Kill cap 或 Drain{cancel_resting = true} 生效期间,sequencer 在每次 ConnState Up
    /// 或 Unknown 收敛为 Live 时对范围内仍 Live 的槽重发撤单(open_only 与最初的撤单命令相同)。
    pub fn reissue_cancels(&mut self, scope: CancelScope, open_only: bool, now: Tsc, cause_seq: u64, out: &mut OmsOut);
    /// 恢复后为 gw 重建私有映射:每个非终态槽一条 `MirrorRestore{slot, gen, epoch, inst, exch_order_id}`。
    pub fn mirror_restore(&self, v: VenueId, a: AccountId, out: &mut FixedVec<MirrorRestore, 256>, cursor: &mut u32) -> bool;
    /// AdoptExchange:按采纳的差额同量调整 LegTracker(不建槽)。
    pub fn adopt_adjust(&mut self, a: AccountId, g: GroupId, inst: InstId, qty: Qty);
    /// 恢复时决策不一致(hft-sequencer "恢复时决策不一致"):把该 (venue, account) 全部非终态槽置 Unknown(迁移原因 `ForceUnknown`)并登记查询;终态与空闲槽不变;返回受影响槽数。
    pub fn force_unknown(&mut self, v: VenueId, a: AccountId, now: Tsc, cause_seq: u64, out: &mut OmsOut) -> u32;
    /// 仅 Sim 账户(`SimReset`):要求该账户无 `Unknown` 槽且无非终态 IOC/FOK 槽(否则 Err,sequencer 以 `PreconditionFailed{SimNotIdle}` 拒绝,
    /// 前置条件的唯一定义在 [control-plane](../../docs/subsystems/control-plane.md))。先清除该账户已有墓碑,再把其余非终态槽(只可能是挂单)
    /// 按撤销终结并写进墓碑(sim-gw 同时撤单,其 CancelAck 命中墓碑按 dup 处理),最后清零该账户 LegTracker、InflightBook 与冒烟单记录。
    /// sim-gw 在重置后不再产生该账户此前订单的成交(规则归 hft-sim);若仍出现,按墓碑规则照常入账并报 P0。
    pub fn sim_reset(&mut self, a: AccountId) -> Result<(), OpError>;
    /// 残余核销(`WriteOffResidual{account, group}`,sequencer 命令路径;命令的权限与确认归 control-plane):该 group 须静止
    /// (定义见"LegTracker",否则 Err(NotQuiescent)),Broken 与否均可;移出多出那一腿的已成交裸量、清零全部腿的余数,返回单条 WriteOff。见"残余核销"。
    pub fn write_off_residual(&mut self, a: AccountId, g: GroupId) -> Result<WriteOff, OpError>;
    /// SmokeOrder 命令(sequencer 命令路径):按"冒烟单"一节生成 PostOnly 意图;只读,不改状态。
    pub fn smoke_intent(&self, c: &SmokeReq, bbo: &BboView, hot: &InstHot, conn_liveness_ms: u32, now: Tsc) -> Result<OrderIntent, SmokeErr>;
    /// Timer{Oms, SmokeTtl} 到期(键 (account, venue),编码归 hft-event):撤该 (account, venue) 的冒烟单(见"冒烟单")。
    /// keep = true:sequencer 在 Draining 生效且该 Drain 的 cancel_resting = false 时给出(取自进快照的 cap 状态,回放确定)→ 该 venue 有交易所死手时不撤,见"冒烟单"第 5 条。
    pub fn on_smoke_ttl(&mut self, a: AccountId, v: VenueId, keep: bool, now: Tsc, cause_seq: u64, out: &mut OmsOut);
    /// HedgeRetry 定时器到期。
    pub fn on_hedge_retry(&mut self, link_id: u32, now: Tsc, cause_seq: u64, out: &mut OmsOut);
    /// Timer{Oms, UnknownQuery} 到期(id = table_ix | slot_id << 8):该槽仍为 Unknown 或带 VERIFY 时按"Unknown 收敛流程"第 3 条
    /// 再查询并重新登记;否则无动作、不再登记。
    pub fn on_unknown_query(&mut self, id: u32, now: Tsc, cause_seq: u64, out: &mut OmsOut);
    /// 风控以 HEDGE_LIMIT 拒绝了本 crate 产生的对冲请求:不重试,链接置 Broken{HedgeLimit}(LinkUpdate 输出)。
    pub fn hedge_vetoed(&mut self, link_id: u32, now: Tsc, cause_seq: u64, out: &mut OmsOut);
    /// 本 crate 产生的对冲请求在出环前被门控:`price_hedge` 的 `StaleQuote`(报价连接不健康)/`BeyondCross`,或 HEDGE_LIMIT 以外的风控 RejectBits。
    /// 不计次、时间照算,登记 HedgeRetry(见 OnFill 第 6 条)。
    /// retry_at = 该 venue 的退避截止(hft-risk 给出;无退避为 0),HedgeRetry 到期时刻取 max(now + hedge_retry_min_ms, retry_at)。
    pub fn hedge_gated(&mut self, link_id: u32, why: HedgeGate, retry_at: Tsc, now: Tsc, cause_seq: u64, out: &mut OmsOut);
    /// hft-risk 报告 (a, v) 的对冲闸门子集——该 venue 的 IDENTITY、ACCOUNT_CFG、ORDER_CONNS 条件(hft-risk `hedge_gate_ok`,
    /// 经 `ModeOutcome.hedge_gate_open` 列出)——由假变真时由 sequencer 调用(由 Input 驱动;完整闸门放行之前或同时;重复调用无害):
    /// 续接单落在 (a, v) 上的 RESUME_HOLD 链接各续接一次(见"恢复")。
    pub fn on_hedge_gate_open(&mut self, a: AccountId, v: VenueId, now: Tsc, cause_seq: u64, out: &mut OmsOut);

    /// 输入事件 → 迁移。每个函数只读事件体,不看系统状态之外的任何东西。
    pub fn on_order_event(&mut self, ev: &OrderEvent, hdr: &RecordHeader, out: &mut OmsOut);
    pub fn on_send_report(&mut self, ev: &SendReport, hdr: &RecordHeader, out: &mut OmsOut);
    pub fn on_conn_state(&mut self, ev: &ConnState, hdr: &RecordHeader, out: &mut OmsOut);
    pub fn on_reconcile(&mut self, ev: &ReconcileReport, hdr: &RecordHeader, out: &mut OmsOut);
    /// 外来成交(B3)经 hft-ledger 归属后由 sequencer 调用:按带符号净量更新 LegTracker;cause = Unknown 且带
    /// exch_order_id 时记入 ext_held,供该单之后被匹配到槽时去重。不产生订单。
    /// `out` 只接收 `ext_held` FIFO 淘汰的 `OmsAnomaly{ForeignEvict}`。
    pub fn external_adjust(&mut self, a: AccountId, v: VenueId, g: GroupId, inst: InstId, net: Qty,
                           exch_order_id: u64, cause: ExternalCause, out: &mut OmsOut);
    /// FeeOnly 的净量补差(hft-ledger apply_fee 返回的 NetAdjust)由 sequencer 同一事件内转交。
    pub fn net_adjust(&mut self, a: AccountId, g: GroupId, inst: InstId, delta: Qty);
    /// 系统定时器 OMS_SWEEP(1 s)。有界:每次最多扫 `sweep_batch` 个非空闲槽,游标跨次推进;另判 RESUME_HOLD 的续接超时(见"恢复")。
    pub fn on_sweep(&mut self, now: Tsc, out: &mut OmsOut);

    /// 链上意图(optional,feature `chain`;带链上腿的策略迁移时启用)。槽语义相同,见 §链上槽。
    pub fn place_swap(&mut self, it: &ApprovedSwap, now: Tsc, cause_seq: u64, out: &mut OmsOut) -> Result<SlotRef, PlaceError>;
    pub fn on_tx_lifecycle(&mut self, ev: &TxLifecycle, hdr: &RecordHeader, out: &mut OmsOut);

    /// 只读视图(策略视图、风控、执行层、ops 快照用)。
    pub fn slot(&self, s: SlotRef) -> Option<&OrderSlot>;
    pub fn inflight(&self) -> &InflightBook;
    pub fn legs(&self, a: AccountId, g: GroupId) -> &LegState;          // = SDK LegView
    pub fn link_peer(&self, link_id: u32) -> Option<(VenueId, AccountId)>; // 链接另一腿的 venue/account(B1)
    /// sequencer 填写待合成样本的 `decided_tsc`(见"延迟样本"第 1 条;槽无待合成样本时无动作)。
    pub fn stamp_decided(&mut self, s: SlotRef, decided_tsc: Tsc);
    pub fn venue_ready(&self, v: VenueId, a: AccountId) -> bool;          // 该 (venue, account) 无 Unknown、连接 Up
    pub fn quiescent(&self, a: AccountId) -> QuiesceBits;                 // Drain 等待:LINKS_ACTIVE | IOC_NONTERMINAL | HEDGE_RETRY_PENDING | UNKNOWN
    pub fn snapshot(&self, w: &mut SnapshotWriter);                       // 一次写本 crate 的全部节(2、3、12)

    /// 只装载快照(恢复、验证回放、advisor 镜像三种驱动共用);不改任何 TSC 量。
    pub fn restore(r: &mut SnapshotReader, cfg: &OmsConfig, rd: &RefData) -> Result<Oms, RestoreError>;
    /// 冷启动恢复结束(只由恢复驱动调用:尾部重放之后、写 purpose = Recovery 的快照之前)。now = 本次运行的 TscClock 读数
    /// (与 Recovery 快照头同值);stop_tsc = 重放到的最后一条 Input 的 tsc(不重放尾部时 = 所选快照所在记录的记录头 tsc;快照头的 snap_tsc 是测量字段,不用);hz_prev / hz_cur =
    /// 上次 / 本次运行的 tsc_hz_milli。非终态槽置 Unknown、TSC 平移(停机不计)、链接置 RESUME_HOLD 并给出续接超时。
    pub fn recovery_end(&mut self, now: Tsc, stop_tsc: Tsc, hz_prev: u64, hz_cur: u64, cause_seq: u64, out: &mut OmsOut);
}

/// 对冲腿定价(纯函数,sequencer 在投影 IntentView 之前调用),规则见"对冲定价"。lim 是 hft-types 的 HedgePxLimits
/// (唯一定义在 hft-types;sequencer 按对冲单所在账户与 inst 从 hft-risk 取值,与风控检查链 #5 同值同源,本 crate 不另设阈值);
/// tick = 对冲腿 inst 的价格步长(refdata `InstHot.tick`,sequencer 按 req.inst 取),交给 cap_cross_px 取整。
/// tsc_per_ms = 本次运行每毫秒的 TSC tick 数(把 lim.conn_liveness_ms 换成 TSC;纯函数不持有频率)。
pub fn price_hedge(req: &HedgeRequest, plan: &HedgePlan, bbo: &BboView, now: Tsc, lim: &HedgePxLimits, tick: i64,
                   tsc_per_ms: u64) -> Result<HedgePx, HedgePxErr>;
pub struct HedgePx { pub px: Px, pub capped: bool,                      // capped:限价被越价上限截住,或超滑点按上限定价
                     pub slipped: bool, pub touch: Px, pub cap: Px }     // slipped:Cross/Mid 因 dev > max_slip 按 cap 定价(第 4 步;Limit 恒 false);touch/cap 供 OmsHedgeSlipCapped;
                                                                        // sequencer 把结果随 ApprovedIntent.hedge_px 交回 place
pub enum HedgePxErr { StaleQuote /* 报价连接不健康或缺一侧;盘口不变不算 */, BeyondCross }   // 两者都是门控:sequencer 转 hedge_gated
pub enum HedgeGate { Px(HedgePxErr), Risk(RejectBits) }
pub struct SmokeReq { pub account: AccountId, pub venue: VenueId, pub inst: InstId, pub side: Side, pub ttl_ms: u32 } // = ControlCmd SmokeOrder 载荷(布局归 hft-event,参数与默认值归 control-plane)
pub enum SmokeErr { Busy /* 该 (account, venue) 已有非终态冒烟单 */, NoQuote /* 报价不可用或缺对手侧 */, BadInst /* inst 不属于该 venue */ } // 结果码映射归 control-plane
pub struct WriteOff { pub venue: VenueId, pub inst: InstId, pub leg_ix: u8, pub qty: Qty /* 带符号:从多出那一腿移出的已成交净量;0 = 无多出的一腿 */ } // sequencer 转成 hft-ledger AdjustDelta::WriteOff
pub enum PlaceError { NoTable /* (venue, account) 不在 refdata 表内 */, NoFreeSlot /* 空闲表为空 */, LinkTableFull /* 带 LegLink 的第一腿而该账户 LinkTable 已满 */ }
pub enum OpError { StaleRef /* SlotRef 的 gen 与当前槽不符或槽空闲 */, NotAllowed /* 当前状态不允许该操作,见"状态机" */,
                   NotQuiescent /* write_off_residual */, NotIdle /* sim_reset */,
                   AmendUnsupported /* amend_qty_down_only 的 venue 上改价或增量,见"状态机"的拥有者撤改前置 */ }
pub enum OmsConfigError { SlotsOutOfRange { venue: VenueId, account: AccountId } /* slots ∉ 1..=65 535 */, TooManyTables /* (venue, account) 表数 > 256 */,
                      TombGens /* tombstone_gens ∉ 1..=4 */ }
/// 触发事件的时间戳(sequencer 从触发记录的头、MarketHdr 或回报头取;缺失为 0)。ApprovedIntent 带一份,cancel/amend 作参数,写进 SlotStamps。
pub struct TrigStamps { pub ex_ts_ns: i64, pub ex_send_ns: i64, pub hw_rx_ns: i64, pub sw_rx_ns: i64,
                        pub rx_tsc: Tsc, pub proc_tsc: Tsc, pub trigger_seq: u64, pub kind: u16 /* 触发事件的 EventKind 编号 */ }
```

### 输出 `OmsOut`(容量固定,由 sequencer 预分配并在每个输入事件后清空)

```rust
pub struct OmsOut {                               // 各向量容量 = 单次调用上限;满了的工作项进 pending_out 续发,从不丢弃(见"输出扇出")
    pub decisions: FixedVec<OrderDecision, 32>,   // 容量 ≥ max_out_per_event 的上限 32,K 不被向量容量截断;→ seq journal;放行且非 SHADOW 的由 sequencer 投影成 GwCmd(OutMsg)推 gw 出环
    pub swaps:     FixedVec<SwapDecision, 4>,     // optional(chain):→ seq journal + chain 出环
    pub hedges:    FixedVec<HedgeRequest, 8>,     // Second / SecondRetry / Reverse → price_hedge → 风控(含 #6 HEDGE_LIMIT)→ place()
    pub fills:     FixedVec<FillOut, 16>,         // 成交增量(含净量)→ sequencer 转 FillDelta 交 ledger
    pub fees:      FixedVec<FeeOut, 8>,           // FeeOnly → ledger.apply_fee(净量补差经其返回的 NetAdjust 回到 net_adjust)
    pub links:     FixedVec<LinkUpdate, 8>,       // 链接状态变化 → sequencer 投影给 hft-exec;Broken → sequencer 设 Broken cap
    pub foreign_fills: FixedVec<ForeignFill, 4>,  // OrderEvent{SLOT_UNRESOLVED} 带成交、StaleGen 带成交 → sequencer 按 ExternalFill{cause: Unknown} 走 ledger;
                                                  //   每条另写一条 Note{OmsForeignFill}(冷读者的入账依据,见"回报匹配与成交增量"第 8 条)
    pub notes:     FixedVec<Note, 32>,            // source = Oms 的迁移、异常、告警(码表见"Note 码表")
    pub latency:   FixedVec<LatencySample, 4>,
    pub updates:   FixedVec<OrderUpdate, 16>,     // → 槽的拥有者:EXEC 标志的槽交 hft-exec,其余交策略 EventRef::Order;冒烟单无拥有者,不产出
    pub timers:    FixedVec<OmsTimerReq, 8>,      // 登记/撤销 Timer{Oms, UnknownQuery | HedgeRetry | SmokeTtl}(编码归 hft-event);sequencer 排空进堆
}
pub struct OmsTimerReq { pub kind: OmsTimerKind /* UnknownQuery | HedgeRetry | SmokeTtl */,
                         pub id: u32 /* UnknownQuery:table_ix | slot_id << 8(table_ix 见"数据布局");HedgeRetry:link_id;SmokeTtl:account | venue << 8 */,
                         pub deadline_tsc: Tsc, pub cancel: bool }   // id 在 Timer.timer_id 里的位置与位宽归 hft-event

pub struct HedgeRequest {
    pub link_id: u32, pub role: LinkRole,         // Second | SecondRetry | Reverse(本 crate 不产生 HedgeFix)
    pub venue: VenueId, pub account: AccountId, pub side: Side, pub reduce_only: bool, // reduce_only = HedgePlan.reduce_only(Reverse 恒为 true)
    pub inst: InstId, pub group: GroupId, pub strategy: StrategyId,
    pub qty: Qty,                                  // 已按 lot 向零取整,且 ≤ 该方向裸量(Second/SecondRetry)
    pub basis: Qty,                                // 上限基数:Second = 取整前 raw;SecondRetry = owed;Reverse = 未对冲的第一腿净量
    pub hedge_cap_qty: Qty,                        // = basis + 1 lot(规则归 hft-exec"对冲量上限");sequencer 填进 IntentView
    pub ref_px: Px,                                // = LinkEntry.ref_px(price_hedge 的 max_slip 基准;Reverse 为 0)
    pub peer_venue: VenueId, pub peer_account: AccountId, // 第一腿所在(Simultaneous 第二腿的风控双 venue 检查用)
    pub trigger_seq: u64,
}
pub struct FillOut {                               // sequencer 转成 hft-ledger 的 FillDelta
    pub slot: SlotRef, pub account: AccountId, pub venue: VenueId, pub inst: InstId, pub group: GroupId, pub leg_ix: u8,
    pub side: Side, pub class: IntentClass, pub flags: FillFlags,   // PX_EST | LATE(墓碑)| OVERFILL | EXT_DEDUP(部分已按外来成交入账)
    pub qty: Qty, pub net_qty: Qty,                // 入账量(EXT_DEDUP 时已扣除先前按外来成交入账的部分);net_qty 见"成交净量"
    pub px: Px, pub fee: i64, pub fee_asset: AssetId, pub seq: u64,
}
pub struct LinkUpdate {
    pub link_id: u32, pub account: AccountId, pub group: GroupId, pub first: SlotRef,
    pub state: LinkState,                          // Open | Hedging | Done | Broken
    pub broken_reason: LinkBroken,                 // None=0 RetryExhausted=1 Deadline=2 HedgeLimit=3 Overfill=4 ReverseFailed=5 ResumeTimeout=6(只追加;
                                                   //   hft-exec 原样放进 ExecDecision.detail 的 arg)
    pub flags: u8,                                 // RESUME_HOLD = 1(同 LinkEntry.flags 的该位);清除该位时产出一条不含它的 LinkUpdate
    pub first_net: Qty, pub hedge_filled: Qty, pub owed: Qty,
    pub tolerance: Rate, pub residual_policy: ResidualPolicy,   // 该链接的 LegLink.tolerance 与 HedgePlan.residual(sequencer 投影给 hft-exec)
}
pub struct ForeignFill {                           // 防御路径与 StaleGen 的外来成交增量(按 exch_order_id 的累计量去重后)
    pub venue: VenueId, pub account: AccountId, pub inst: InstId, pub side: Side,
    pub qty: Qty, pub px: Px, pub fee: i64, pub fee_asset: AssetId, pub exch_order_id: u64, pub seq: u64,
    pub trade_id: u64,                             // 回报带成交 id 时填,否则 0(OrderEvent 不带成交 id,恒为 0)
    pub cum_filled: Qty,                           // 本增量之后该交易所订单的累计成交量(毛量)
}
// sequencer 构造 ExternalFill{cause = Unknown}:trade_id 原样;trade_id = 0 时 fill_seq = cum_filled 的 raw 值,使去重键
// (inst, exch_order_id, fill_seq) 对同一订单的每笔增量互不相同(键的唯一定义在 hft-venue-proto 契约 22)。
```

`OrderDecision`、`SwapDecision`、`LatencySample`、`Note`、`ExternalFill` 与出环元素 `OutMsg`(= `GwCmd`,64 B)的布局归 [hft-event](../hft-event/README.md#环元素类型)。`OrderDecision` 只写 journal;放行且非 SHADOW 的决策由 sequencer 投影成一条 `GwCmd`(`op = action`、`oflags` 原样)出环,gw 从 `GwCmd` 取下单字段按 venue 模板填帧;OMS 不编码交易所帧。`OrderUpdate`、`OrderRef`、`LegView`、`GroupTable`、`BboView` 是 [SDK](../hft-strategy-sdk/README.md) 类型。

`ApprovedIntent` = 订单意图(策略经 `Ctx::place` 的 SDK `OrderIntent`,hft-exec 的 `ExecOrder.intent`,或 `smoke_intent` 生成的冒烟单)+ 来源(`Strategy` \| `Exec` \| `Oms` \| `Command`;`Command` 目前只用于冒烟单)+ 风控结果 + **hft-risk `classify()` 的结果**(`class`、`risk_reducing`、`reduce_only`,OMS 只把它们拷进 `oflags`,自己不做分类)+ 风控评估永续 Open 时所用的杠杆 `lev_x100`(百分之一倍;其余为 0,供 InflightBook 增量维护)+ 冒烟单的 `smoke_ttl_ms`(其余为 0)+ 本 crate 对冲请求的定价结果 `hedge_px: HedgePx`(sequencer 原样拷入;其余为零值)+ 带 LegLink 的第一腿的 `hedge_ref_px`(sequencer 在本次 `place` 前从第二腿 inst 的 `BboView` 取第二腿方向的对手价——买取 ask、卖取 bid;报价不健康或缺该侧时为 0;其余为 0,写进 `LinkEntry.ref_px`)+ 触发事件的时间戳 `TrigStamps`;来自 hft-exec 的意图在槽上置 `EXEC` 标志,冒烟单在 `oflags` 置 `SMOKE`(见"冒烟单")。`OrderDecision.reason`:执行层订单 = hft-exec 登记的 `ExecOrderKind` 码;本 crate 产生的对冲单(第二腿、重试、反向)与冒烟单 = 0(角色与来源分别在 `link_role` 与 `oflags`)。`ApprovedSwap` 同理对应 `SwapIntent`(optional)。`SlotRef` 就是 SDK 的 `OrderRef { venue, account, slot, gen }`(12 B),是持有订单的唯一句柄。`ApprovedIntent` 另带 `reason`(→ `OrderDecision.reason`)、`intent_no`(SDK 意图序号,原样回填 `OrderDecision`)与 `link_id`(对冲请求的链接号,`ApprovedIntent::hedge` 填写;其余 0)。`CancelScope`/`CellKey` 目前在本 crate 定义,与 hft-risk 的同名类型逐值一致(sequencer 转换);待两者移到下层公共 crate 时合并。

`GroupId` 约定(常量定义在 [hft-types](../hft-types/README.md)):`GROUP_NONE = GroupId(0)` = 不属于任何 group(策略不带 group 的订单、无法归属的外来成交);真实 group 从 1 起;`GROUP_ALL` 只用于命令作用域,本 crate 不使用。

### 消费与产生的事件

| 方向 | 事件(定义见 [event-model](../../docs/subsystems/event-model.md)) | 用途 |
|---|---|---|
| 消费 | `OrderEvent`(Ack/Reject/PartialFill/Fill/CancelAck/CancelReject/AmendAck/AmendReject/Expired/QueryResult/OutcomeUnknown/FeeOnly) | 状态迁移;成交 → LegLink、LegTracker |
| 消费 | `SendReport`(gw 写出结果与 `sent_tsc`;每条订单类 `GwCmd` 恰好一条;带 `op`、`target_slot`、`target_gen`) | 见"SendReport 处理" |
| 消费(经 sequencer) | `ExternalFill` 经 hft-ledger 归属后的净量 | LegTracker 更新、与槽成交去重(B3) |
| 消费 | `ConnState`(下单连接、用户流) | Down → Pending* 进 `Unknown`;Up → 触发查询收敛与撤单持续义务;`REPORT_DECODE` 见"REPORT_DECODE" |
| 消费 | `ReconcileReport` | `Unknown` 收敛的第二来源;NotFound 槽的成交历史确认 |
| 消费 | `Timer{owner = Oms}` | `Sweep`(含续接超时判定)、`UnknownQuery`、`HedgeRetry`、`SmokeTtl`、`OmsContinue` |
| 消费(经 sequencer) | 对冲闸门子集满足(`on_hedge_gate_open(a, v)`) | 恢复后的对冲续接 |
| 消费(经 sequencer) | `ControlCmd{SmokeOrder}`、`ControlCmd{WriteOffResidual}`;执行层的 `ExecOut.cancel_resting` | 冒烟单、残余核销、引擎目标前撤策略挂单 |
| 消费 | `TxLifecycle`(optional) | 链上槽迁移 |
| 产生 | `OrderDecision`(New/Cancel/Amend/CancelAll/Query)、`SwapDecision`(optional) | 决策记录进 journal;放行的决策由 sequencer 投影为 `GwCmd` 出环 |
| 产生 | `Note{source = Oms}` | 每次迁移一条 `OmsTransition`;异常类见"Note 码表" |
| 产生 | `LatencySample` | 新单、撤单、改单各在其首个交易所响应时合成一条(`action` 区分),规则见"延迟样本";8 个时间点的定义见 [latency-budget §1.2](../../docs/latency-budget.md) |

### 配置项(`OmsConfig`)

来自 `EngineConfig` blob 的 `oms` 段;变更 = 重启。

| 名称 | 类型 | 默认 | 含义 / 来源 |
|---|---|---|---|
| `slots[(venue, account)]` | u32 | 4096 | 槽数;24 h 最大在途 + 余量([05 §2.1](../../design/05-CEX执行层.md))。取值 1..=65 535(槽号 1..=N,0 保留),且 (venue, account) 表数 ≤ 256(`Timer{Oms, UnknownQuery}` 的编码位宽,见 `OmsTimerReq`),超出 → `OmsConfigError` |
| `links_per_account` | u32 | 4096 | `LinkTable` 容量 |
| `ack_timeout_ms[venue]` | u32 | 2000 | `PendingNew` 无回执多久进 `Unknown` 并查询 |
| `cancel_timeout_ms[venue]` | u32 | 2000 | `PendingCancel`/`PendingAmend` 同上 |
| `ioc_timeout_ms[venue]` | u32 | 1000 | IOC/FOK `Live` 超时仍无终态 → `Unknown` |
| `unknown_query_interval_ms` | u32 | 1000 | Unknown 查询重试间隔 |
| `unknown_max_queries` | u8 | 5 | 超过后升级:请求对账 + P0 告警 |
| `unknown_alarm_ms` | u32 | 30000 | Unknown 持续超过 → sequencer 设 `UnknownTimeout{account, venue}` cap(只挡 Open,不暂停对冲) |
| `out_quota[(venue, account)]` | u16 | `max_out_per_event` × 4 | 扇出队列按 (venue, account) 分配额,一个账户的风暴不占另一个账户的出环名额 |
| `not_found_grace_ms[venue]` | u32 | 该所过期保护窗口 + 500;无过期保护的所 2 000 | 查询"不存在"后,`sent_tsc` 之后超过此值才请求成交历史确认;只有 `stale_guard = ExpireAt` 的 venue 上的非链接槽可在此之后直接定终态(见 Unknown 收敛)。成交历史确认本身的等待由 reconciler 的 `notfound_grace_ms` 与轮次规则决定,不是本参数 |
| `smoke_far_bp` | Rate | 0.03(300 bp) | 冒烟单限价离对手价的最小距离(比率,见"冒烟单") |
| `reuse_quarantine_ms` | u32 | 5000 | 终态槽回收前的冷却;FIFO 空闲表另外保证复用间隔 |
| `tombstone_gens` | u8 | 2(≤ 4) | 每槽保留的已回收代数(墓碑),用于迟到成交入账(C1) |
| `tombstone_keep_ms` | u32 | 3 600 000 | 墓碑保留时长;过期由 sweep 清除(限制快照体积);须 ≥ hft-venue 的 `gw.id_tomb_ms`(gw 私有 id 表的墓碑) |
| `sweep_batch` | u16 | 256 | 每次 sweep 最多检查的槽数 |
| `max_out_per_event` | u16 | 16 | 每个 Input 最多交出的出环决策数(K),其余经 `Timer{OmsContinue}` 续发 |
| `hedge_retry_max` | u8 | 3 | 第二腿执行结果类失败后的 `SecondRetry` 次数上限(门控拒绝不计次) |
| `hedge_retry_min_ms` | u32 | 100 | `HedgeRetry` 定时器最小间隔(实际 = max(此值, 该 venue 退避截止)) |
| `hedge_deadline_ms` | u32 | 3000 | 从最早一笔尚未对冲完的第一腿成交起(`first_fill_tsc`,见 OnFill 第 1 条),**引擎运行时间**超过此值仍未对冲完(含门控拒绝期间)→ 链接 Broken + P0;停机时间与恢复后 `RESUME_HOLD` 期间不计(见"恢复") |
| `resume_hold_max_ms` | u32 | 30000 | `recovery_end` 之后链接停在 `RESUME_HOLD` 的最长**引擎运行时间**(等续接单所在 venue 的对冲闸门子集、等该链接的 Unknown 收敛);超时 → 链接 `Broken{ResumeTimeout}` + `Note{OmsResumeTimeout}`(P0) |
| `foreign_fill_track[(venue, account)]` | u16 | 64 | 防御路径:按 `exch_order_id` 跟踪外来单累计成交的表容量(见"外来成交") |
| `ext_held_track[(venue, account)]` | u16 | 64 | `ext_held` 表容量:按 `exch_order_id` 记录已按 `ExternalFill{cause: Unknown}` 入账、可能之后被匹配到槽的成交量 |
| `decode_fail_down_n` | u8 | 3 | 连续 N 次 `REPORT_DECODE` 按连接 Down 处理 |
| `expire_window_ms[venue]` | u32 | 无 | 该所下单请求的过期保护窗口(`recvWindow`/`expTime` 的配置值),只用于推出 `not_found_grace_ms` 的默认值 |
| `max_groups` | u32 | 4096 | LegTracker 容量((account, group) 条目数;`on_groups_changed` 不分配) |

`OmsConfig` 另带本次运行的 `engine_epoch` 与 `tsc_hz_milli`(启动装配给出,不是可调参数);全部 `*_ms` 按后者换算成 TSC。

对冲腿定价的报价连接健康阈值 `conn_liveness_ms`(默认 2 000)与越价上限 `rr_max_cross` 不在本表:它们是 [hft-risk `RiskParams`](../hft-risk/README.md) 字段,风控检查链 #5(含 B1 第二腿 `LINK_STALE`)与 `price_hedge` 用同一个值,由 sequencer 经 [hft-types `HedgePxLimits`](../hft-types/README.md#对冲定价限值) 传入。尘埃线、补腿阈值、残余规则与 Broken 处置时长不在本表:它们是 [hft-exec 配置](../hft-exec/README.md#配置项execconfigengineconfig-blob-的-exec-段每账户一行--全局默认行变更--drain--重启)与目标/链接参数。gw 暂存无法归属成交的时长 `gw.unmatched_hold_ms`(默认 5 000)归 [hft-venue](../hft-venue/README.md)。

## 实现说明

<details><summary>实现要点</summary>

### 数据布局(草案,实现时以代码与编译期断言为准)

```rust
#[repr(C, align(64))]
pub struct OrderSlot {                 // 128 B = 2 条缓存行
    // ── 行 0:每次迁移都触碰 ──
    pub state: OrderState,             //  0 u8
    pub side: Side,                    //  1 u8
    pub tif: Tif,                      //  2 u8
    pub prev_state: OrderState,        //  3 u8,进入 Unknown 前的状态
    pub oflags: OrderFlags,            //  4 u16 线上订单标志(REDUCE_ONLY、POST_ONLY、LINKED、HEDGE_FIX、RISK_REDUCING、SMOKE;
                                       //        位值唯一归属 hft-event `OrderFlags`),下单时由 classify 结果与来源填,原样拷进 OrderDecision.oflags
    pub venue: VenueId,                //  6 u8
    pub account: AccountId,            //  7 u8
    pub gen: u32,                      //  8 槽代数;每次分配 +1,编进 client id
    pub slot_id: u32,                  // 12 数组下标(0 保留为无效)
    pub inst: InstId,                  // 16 u32
    pub strategy: StrategyId,          // 20 u8(拥有者;执行层订单为该账户绑定的策略实例)
    pub query_count: u8,               // 21
    pub epoch: u16,                    // 22 下单时的 engine_epoch & 0x7FFF(低 15 位;bit 15 恒 0,见"ClientOrderId");回报按 (epoch, slot, gen) 匹配
    pub group: GroupId,                // 24 u32;GROUP_NONE(0)= 不属于任何 group
    pub link_id: u32,                  // 28 0 = 无;LinkTable 下标
    pub qty: Qty,                      // 32 当前有效委托量(资产指数定点,见 hft-types)
    pub filled: Qty,                   // 40 累计成交(毛量,单调)
    pub px: Px,                        // 48 当前有效委托价
    pub exch_order_id: u64,            // 56 交易所数值 id(字符串 id 由 gw 私有表映射)
    // ── 行 1:成交与改单时触碰 ──
    pub filled_notional: i128,         // 64 Σ px×qty(报价资产定点),均价 = filled_notional / filled
    pub pending_px: Px,                // 80 改单目标
    pub pending_qty: Qty,              // 88
    pub fee_accum: i64,                // 96 fee_asset 定点
    pub fee_asset: AssetId,            // 104 u16
    pub sflags: SlotFlags,             // 106 u16 本 crate 私有状态位(不出现在任何线上记录,进快照):
                                       //        CANCEL_SENT=1 AMEND_SENT=2 FILL_BEFORE_ACK=4 VERIFY=8 QUEUED=16 EXEC=32
                                       //        SMOKE_DUE=64(冒烟单已到期、待撤)ACKED=128(收到过 Ack 或成交,在途分类用,见"LegTracker")
                                       //        HIST_WAIT=256(NotFound 已过 grace、等成交历史确认)ALARMED=512(本次 Unknown 已按 unknown_alarm_ms 升级)
                                       //        AGED=1024(本次 Live 已发过 Aged);其余预留
    pub reject_code: u32,              // 108 交易所原始码或 LocalReject 码
    pub t_state: Tsc,                  // 112 进入当前状态时的事件 tsc(sweep 用;进快照的已登记 TSC 字段,见"恢复")
    pub seq_last: u64,                 // 120 最后一次改变本槽的事件 seq
}

#[repr(C, align(32))]
pub struct SlotExt {                   // 64 B,与槽平行;成交、费用与链接时触碰,进快照
    pub net_filled: Qty,               //  0 累计成交净量(见"成交净量";LegTracker 与对冲基数用)
    pub tag: u32,                      //  8 意图 tag 原样
    pub leg_ix: u8,                    // 12 该槽在 group 中的腿下标(归属表给出)
    pub role: LinkRole,                // 13 u8;无链接为 0
    pub origin: u8,                    // 14 Strategy=1 Exec=2 Oms=3 Command=4(冒烟单)
    pub class: u8,                     // 15 IntentClass 码 = ApprovedIntent 的 classify 结果(FillOut.class、OrderView.class、cancel_open 用);占用原保留字节
    pub lev_x100: u16,                 // 16 = ApprovedIntent.lev_x100(永续 Open;其余 0),InflightBook.open_margin 用
    pub _pad1: u16,                    // 18
    pub max_rest_ms: u32,              // 20 = 意图的 max_rest_ms(0 = 不限;sweep ② 用)
    pub reason: u32,                   // 24 = ApprovedIntent.reason(New 决策排队后交出时用)
    pub intent_no: u32,                // 28 = ApprovedIntent.intent_no(同上)
    pub fee_cum: Qty,                  // 32 最后一条 PartialFill/Fill 的 cum_filled(费用入账到此,见"回报匹配与成交增量"第 9 条)
    pub _rsv: [u64; 3],                // 40 保留(0)
}

#[repr(C)]
pub struct SlotStamps {                // 80 B,与槽平行;只在下单/撤单/改单决策、对应的 SendReport 与首个响应时触碰,不进快照(见"延迟样本")
    pub trig_ex_ts_ns: i64, pub trig_ex_send_ns: i64, pub trig_hw_rx_ns: i64, pub trig_sw_rx_ns: i64,   //  0 / 8 / 16 / 24
    pub trig_rx_tsc: u64, pub trig_proc_tsc: u64, pub decided_tsc: u64, pub sent_tsc: u64,              // 32 / 40 / 48 / 56
    pub trigger_seq: u64,                                                                               // 64
    pub trig_kind: u16,                // 72 触发事件的 EventKind 编号(码表 = event-model kind 编号表;→ LatencySample.trigger_kind)
    pub op: u8,                        // 74 待合成样本的操作:New=0 Cancel=1 Amend=2(= LatencySample.action);0xFF = 无
    pub _pad: [u8; 5],                 // 75
}

#[repr(C)]
pub struct Tomb {                      // 56 B;已回收的一代(C1)
    pub gen: u32, pub epoch: u16, pub oflags: OrderFlags,                           //  0 / 4 / 6
    pub inst: InstId, pub group: GroupId,                                           //  8 / 12
    pub side: Side, pub leg_ix: u8, pub strategy: StrategyId, pub role: LinkRole,   // 16 / 17 / 18 / 19
    pub link_id: u32,                                                               // 20
    pub filled: Qty, pub net_filled: Qty, pub exch_order_id: u64, pub freed_tsc: Tsc, // 24 / 32 / 40 / 48
}

const _: () = assert!(size_of::<OrderSlot>() == 128 && align_of::<OrderSlot>() == 64
                      && offset_of!(OrderSlot, oflags) == 4 && offset_of!(OrderSlot, sflags) == 106);
const _: () = assert!(size_of::<SlotExt>() == 64 && size_of::<SlotStamps>() == 80);
const _: () = assert!(size_of::<Tomb>() == 56 && offset_of!(Tomb, filled) == 24 && offset_of!(Tomb, freed_tsc) == 48);

pub struct VenueAcctTable {            // 每 (venue, account) 一个
    slots: Box<[OrderSlot]>,           // 启动期一次分配
    ext: Box<[SlotExt]>,
    stamps: Box<[SlotStamps]>,
    tombs: Box<[Tomb]>,                // slots.len() × tombstone_gens;下标 slot × T + (gen % T)
    free: FifoRing<u32>,               // 容量 = slots.len();FIFO 保证复用间隔最大
    foreign: FifoMap<u64, Qty>,        // 防御路径:exch_order_id → 已入账的外来累计成交(容量 foreign_fill_track)
    ext_held: FifoMap<u64, Qty>,       // exch_order_id → 已按 ExternalFill{cause: Unknown} 入账、尚未被槽认领的量(容量 ext_held_track)
    unknown_count: u32,                // > 0 时 venue_ready = false
    conn_up: bool,
    decode_fails: u8,                  // 连续 REPORT_DECODE 次数(见"REPORT_DECODE")
    hedge_gate: bool,                  // 本次运行已收到 on_hedge_gate_open(该 (venue, account) 的对冲闸门子集满足)
    sweep_cursor: u32,
    smoke_slot: u32,                   // 非终态冒烟单的 slot_id;0 = 无(每 (venue, account) 至多一张)
}
```

- 表的定位是 `tables[table_ix]`,`table_ix = venue_ix × n_accounts + account_ix`,两个下标都是 refdata 给出的稠密序号;**热路径没有 HashMap**。`foreign` 是预分配的开放寻址表,只在外来单路径使用。
- 空闲表初始含 1..N;终态槽在 `t_state + reuse_quarantine` 之后由 sweep 写墓碑并放回队尾。
- 私有位的置位与清除:进入 `Live` 时置 `ACKED`、清 `AGED`;离开 `PendingCancel` 时清 `CANCEL_SENT`,离开 `PendingAmend` 时清 `AMEND_SENT`;离开 `Unknown` 时清 `HIST_WAIT`、`ALARMED`;进入终态时清 `CANCEL_SENT`、`AMEND_SENT`、`VERIFY`。
- 不变量(属性测试逐条检查):`filled ≤ qty`(改单减量后若 `qty < filled`,`qty := filled` 并迁 `Filled`;交易所报告的 `cum_filled > qty` 见"超量成交");`filled` 单调不减;`net_filled` = `filled` 按"成交净量"规则逐笔调整后的累计(买入扣、卖出加基础资产费用;无基础资产费用时 `= filled`);`state == Free ⇔ 槽在空闲表中`;`unknown_count == 本表 state==Unknown 的槽数`;`InflightBook` 的各项等于按其定义对所有非终态槽求和的结果;LegTracker 各腿 `inflight_immediate + inflight_resting` 等于该 group 该腿非终态槽的带符号剩余量之和,两者按"LegTracker"的分类互斥。

### ClientOrderId

语义与约束归本 crate(字段含义、分配与唯一性、代数、接受窗口);**编解码与位布局不在本 crate**:gw 线程里的 `venue-<x>` 要在填帧时渲染、在解码回报时把字符串解回,而 `venue-<x>` 不能依赖 `hft-oms`。因此 96 位布局与规范 24 位十六进制编解码是 [hft-types `ClientOrderId`](../hft-types/README.md#clientorderid-编解码) 的唯一定义;每所的外形(前缀、长度)由 [hft-venue-proto](../hft-venue-proto/README.md) 的 `VenueProtocol::render_cid(&ClientOrderId, &mut [u8; 48])` / `parse_cid` 包装规范形式。分配语义:`epoch = engine_epoch & 0x7FFF`——bit 15 是 hft-types 规定的**恒 0 保留位**(没有任何逻辑;一个交易所账户只由一个系统、一个策略实例交易,id 里不编实例),低 15 位是 `engine_epoch`(每次冷启动递增,取值与跨 AZ 恢复时的安全间隔归 [apps/hft-core](../../apps/hft-core/README.md);存于 `StateSnapshot` 节 3;低 15 位回绕时只比较低 15 位,当前/上一 epoch 的判断按模 2^15);`(venue, account, slot, gen)` = 本 crate 分配的槽与代数。gw 从订单类 `GwCmd` 的 `(account, slot, gen)`、本 gw 的 venue 与启动时交给它的 `engine_epoch` 生成 id,出环条目不携带 id 字符串。

规范形式 24 个十六进制字符对各所 client id 字段的限制核对如下(各所外形的唯一归属是对应 `venue-<x>` README):

| venue | 字段 | 限制(来源 [05 §2.1](../../design/05-CEX执行层.md)) | 我们发出的形式 | 长度 |
|---|---|---|---|---|
| Binance USDT-M / Spot | `newClientOrderId` | ≤ 36,字符集含 `[.A-Za-z:/0-9_-]` | `<24hex>` | 24 |
| OKX | `clOrdId` | ≤ 32,仅字母数字 | `<24hex>` | 24 |
| Bitget | `clientOid` | ≤ 40(待核实,方法见开发备忘 ①) | `<24hex>` | 24 |
| Bybit(optional) | `orderLinkId` | ≤ 36 | `<24hex>` | 24 |
| Gate(optional) | `text` | 前缀 `t-`,去前缀 ≤ 28 | `t-<24hex>` | 26 |
| KuCoin(optional) | `clientOid` | ≤ 40(待核实,方法见开发备忘 ①) | `<24hex>` | 24 |
| Hyperliquid(optional) | `cloid` | 128 位十六进制 `0x…`(待核实,方法见开发备忘 ①) | `0x` + `00000001` + `<24hex>` | 34 |
| 外部 venue(optional,chain) | `SwapDecision.intent_id: u64` | 外部 venue 协议 | `(engine_epoch << 32) | n`,`n = gen × N_chain + slot` | 8 B |

- **保留位非 0**:`decode` 返回 `CidErr::Reserved`,与下一条的其他解码失败同样处理(外来单)。
- 解码失败(非我们格式、校验位错、ver 未知、venue/account 与所在连接不符、epoch 低 15 位不是当前或上一个)且 gw 私有 id 表也查不到 → 无成交的回报由 gw 写 `OrderEvent{slot = SLOT_UNRESOLVED}`,OMS 计数并写 `Note{OmsForeignOrder}`;**有成交的外来单由 gw 写 `ExternalFill`**(见"外来成交")。
- 上一 epoch 的 id 只在冷启动收敛期内被接受(用于把崩溃前在途的订单收敛);收敛完成后同样按外来单处理。"上一 epoch"是恢复所依据快照节 3 里的 `engine_epoch`,不一定等于当前值 − 1(两次运行之间可能隔着安全间隔)。
- **重试不复用 id 发新单**:同一 id 只在"查询"与"撤单"里再出现。`Unknown` 的订单绝不重发 New;确认"不存在"之后,若仍要下单,是一个新意图、新槽、新 id(重试必须重新判断,见 [12 §1](../../design/12-风险与未决问题.md))。

### 状态机

```
            place                 Ack                      Fill(cum == qty)
  Free ───────────▶ PendingNew ─────────▶ Live ─────────────────────────────▶ Filled
                     │  │  PartialFill/Fill │ cancel      CancelAck/Expired     ▲
                     │  └─(fill-before-ack)─┤───────▶ PendingCancel ──────────▶ Cancelled
                     │ Reject               │ amend          │ CancelReject(too late/filled)
                     ▼                      ├───────▶ PendingAmend ──AmendAck──▶ Live
                  Rejected                  │                 └─AmendReject──▶ Live
                                            │
  PendingNew/PendingCancel/PendingAmend ──(连接断 / 超时 / 写出不完整)──▶ Unknown
  Unknown ──(QueryResult / ReconcileReport / 任何带本 slot+gen 的回报)──▶ 真实状态
  Filled/Cancelled/Rejected ──(reuse_quarantine 后 sweep:写墓碑)──▶ Free
```

`Live` 且 `filled > 0` 就是"部分成交",不单设状态。**成交是证据,永远生效**:任何状态收到 `cum_filled > filled` 的回报都先应用增量(即使是 `Cancelled`),再决定迁移;与状态冲突时额外发异常 `Note` 并请求对账(= 写指向该槽的 `Note{OmsNeedsRestQuery}`,`trades = 1`)。

**拥有者撤改的前置**:`cancel` 只接受状态 `Live`、`PendingNew`、`PendingAmend` 且未置 `CANCEL_SENT` 的槽(→ `PendingCancel`,置 `CANCEL_SENT`,产出 `OrderDecision{Cancel}`);`amend` 只接受 `Live` 且该 venue 的 `VenueCaps.flags` 含 `AMEND` 的槽;该 venue 的 `VenueCaps.amend_qty_down_only` 为真时(只能减量、不能改价的交易所,字段归 [hft-venue-proto](../hft-venue-proto/README.md))另要求新价格等于当前限价且新数量 ≤ 当前数量,改价或增量 → `Err(OpError::AmendUnsupported)`;通过时 → `PendingAmend`,`pending_px/pending_qty` 记目标,置 `AMEND_SENT`,产出 `OrderDecision{Amend}`;其余 → `Err(OpError::NotAllowed)`;`SlotRef.gen` 与槽不符或槽为 `Free` → `Err(OpError::StaleRef)`。各种 Err 都无输出、无状态改变。

**码表**(唯一登记在此、只追加;进快照节 2、`OutItem` 与 `Note{OmsTransition}` 的 `v[0]`,[hft-telemetry](../hft-telemetry/README.md) 按它翻译):

| 枚举 | 码值 |
|---|---|
| `OrderState`(`#[repr(u8)]`) | `Free=0 PendingNew=1 Live=2 PendingCancel=3 PendingAmend=4 Filled=5 Cancelled=6 Rejected=7 Unknown=8` |
| `TransitionCause`(`#[repr(u8)]`,迁移原因) | `Place=1 CancelReq=2 AmendReq=3 Ack=4 Reject=5 Fill=6 CancelAck=7 Expired=8 CancelReject=9 AmendAck=10 AmendReject=11 QueryResult=12 Reconcile=13 OutcomeUnknown=14 ConnDown=15 Timeout=16 LocalSendFail=17 WriteError=18 ReportDecode=19 RecoveryEnd=20 ForceUnknown=21 NotFoundExpire=22 Recycle=23 SimReset=24 SmokeTtl=25 FeeOnly=26` |

`Fill` 覆盖 `PartialFill` 与 `Fill`(成交增量的迁移);`CancelReq`/`AmendReq` 是拥有者或撤单义务发起的撤改(含 `cancel_all`、`cancel_resting_on`);`Recycle` 是终态 → `Free`;`FeeOnly` 不迁移状态,只在 `v[]` 中出现。

### 回报处理表

行 = 回报类型,列 = 当前状态;`(epoch, gen)` 不符的事件在查表前已转入"回报匹配"。`dup` = 幂等忽略并计数。

| 回报 \ 状态 | PendingNew | Live | PendingCancel | PendingAmend | Unknown | 终态 |
|---|---|---|---|---|---|---|
| `Ack` | → Live,记 `exch_order_id` | dup | 保持(迟到的新单回执) | 保持 | → Live(证据) | dup |
| `Reject` | → Rejected | 异常 Note,保持 | 异常 Note | 异常 Note | → Rejected | dup |
| `PartialFill` | 应用增量 → Live,置 `FILL_BEFORE_ACK` | 应用增量 | 应用增量,保持 | 应用增量,保持 | 应用增量 → prev_state 或 Live | Cancelled 时应用增量 + 异常;Filled dup |
| `Fill`(cum==qty) | → Filled(fill-before-ack,置 `FILL_BEFORE_ACK`) | → Filled | → Filled | → Filled | → Filled | Cancelled 时 → Filled + 异常;Filled dup |
| `CancelAck` / `Expired` | → Cancelled(撤掉未确认的单) | → Cancelled(交易所主动撤/IOC 余量) | → Cancelled | → Cancelled | → Cancelled | dup |
| `CancelReject` 原因=已成交/不存在 | — | — | 若 `filled==qty` → Filled;否则 → Unknown + 查询(不重试撤单) | 同左 | 保持,继续查询 | dup |
| `CancelReject` 其他原因 | — | — | → Live(拥有者/sweep 决定是否再撤) | — | 保持 | dup |
| `AmendAck` | — | 异常 | — | → Live,`px/qty := pending` | → Live,取事件内 px/qty | dup |
| `AmendReject` | — | 异常 | — | → Live(原价量);原因=已成交同 CancelReject | 保持 | dup |
| `QueryResult` | 按结果收敛 | 按结果收敛 | 按结果收敛 | 按结果收敛 | 按结果收敛(见 Unknown 收敛) | 仅应用成交增量 |
| `FeeOnly`(只带手续费的回报) | 不迁移、不请求对账;`FeeOut` 交 ledger,净量补差见"成交净量" | 同左 | 同左 | 同左 | 同左 | 同左;已回收则按墓碑归属 |
| `OutcomeUnknown`(venue 协议的"结果未知"码,[hft-venue-proto 契约 8](../hft-venue-proto/README.md#实现方必须遵守的契约)) | → Unknown + 查询 | 置 `VERIFY` + 查询(改单/撤单结果未知时按目标槽处理) | → Unknown + 查询 | → Unknown + 查询 | 保持,继续查询 | dup |

重复与乱序的判定只用两样东西:`(epoch, gen)` 与单调的 `cum_filled`。表的补充规则(唯一定义):

- `—` = 该状态下没有对应的在途操作:按"异常 Note"格处理(写 `OmsAnomaly{Table}`、保持状态;带成交增量时先应用增量)。例外:`PendingCancel` 且置 `AMEND_SENT`(改单之后又撤单)时,`AmendAck` → `px/qty := pending`、`AmendReject` → 保持原价量,两者都保持 `PendingCancel` 并清 `AMEND_SENT`。
- 任何状态收到 `Ack` 而槽的 `exch_order_id = 0` 时记下它;收到 `Ack` 或成交即置 `ACKED`。`QueryResult` 收敛为 `Live` 时 `px/qty` 取事件内的值。
- `Unknown` 收到 `PartialFill` 时回到 `prev_state`(`prev_state ∈ {PendingCancel, PendingAmend}`),否则回到 `Live`。
- "终态"列中 `Rejected` 与 `Cancelled` 同样处理(应用增量、异常 Note 并请求对账;`Fill` → `Filled`)。
- "原因=已成交/不存在"由 venue 协议按其 `codes.rs` 判定,经 `OrderEvent.detail = GONE` 给出;`QueryResult` 的订单状态同样取 `OrderEvent.detail`(码与 `ReconcileReport.order_status` 相同:New=1 … NotFound=7)。字段布局归 [hft-event](../hft-event/README.md)。
- 交易所 `Reject` 给拥有者 `OrderUpdate::Rejected{bits: 0, venue_code: reject_code}`;`Rejected(NOT_FOUND)` = 状态 `Rejected`、`reject_code = 0`,迁移原因 `NotFoundExpire`(过期保护定终态)或 `Reconcile`(成交历史确认),给拥有者 `Rejected{bits: 0, venue_code: 0}`。

### SendReport 处理

每条订单类 `GwCmd` 恰好一条 `SendReport`(状态码 `SendStatus` 归 [hft-event](../hft-event/README.md#执行0x02xx);`op`、`target_slot`、`target_gen` 指明对象):

| `op` \ `status` | `Written` | 确定未写出:`KillBlocked`、`RateLimited`、`NoConn`、`EncodeError`、`RingFull`、`QueryUnsupported` | `WriteError`(加密后 I/O,可能部分发出) |
|---|---|---|---|
| `Place` | 记 `SlotStamps.sent_tsc` | 槽 → `Rejected(LOCAL_SEND_FAIL)`(`reject_code` = status),InflightBook 与 LegTracker 在途 −=;给拥有者 `OrderUpdate::Rejected{bits: LOCAL_SEND_FAIL, venue_code: status}`(此时 `venue_code` 是 `SendStatus` 码值而不是交易所码,读者按 `bits` 含 `LOCAL_SEND_FAIL` 区分);链接腿按 OnFill 第 6 条 | 槽 → `Unknown` + 查询 |
| `Cancel` / `Amend` | 记 `sent_tsc`(只在该槽 `SlotStamps.op` 是这次撤改时,见"延迟样本") | 目标槽状态不变,只清 `CANCEL_SENT`/`AMEND_SENT`,写 `Note{OmsCancelNotSent}`;撤单义务仍在时由 `reissue_cancels` 重发 | 目标槽置 `VERIFY` + 查询 |
| `Query` | `query_count += 1`(`query_count` 唯一的增加点) | 保持 `Unknown`,`query_count` 不增;`QueryUnsupported` 转交 reconciler(`Note{OmsNeedsRestQuery}`) | 同左 |
| `CancelAll` | — | 范围内槽状态不变,清 `CANCEL_SENT`,写一条 `Note{OmsCancelNotSent}`;义务仍在时重发 | 范围内槽置 `VERIFY` |

**本地发送失败是否计次**(唯一定义;OMS 第二腿 `SecondRetry`/`Reverse` 的 `retries`、[hft-exec](../hft-exec/README.md#门控拒绝与退避) 的 `fail_streak` 与补腿计次都按此表):

| `status` | 类别 | 理由 |
|---|---|---|
| `KillBlocked`、`RateLimited`、`NoConn`、`RingFull` | 门控:不计次,时间照算(`hedge_deadline_ms`、`fix_deadline_ms` 照常) | 由 kill、限速、连接或 gw 停摆造成,条件消失后同一单可以发出;`RingFull` 另由 sequencer 报 P0 |
| `EncodeError` | 执行失败:计次 | 编码错误是确定性的,原样重发仍会失败;计次耗尽后链接 Broken 交人工 |
| `WriteError` | 不是失败:进 `Unknown` | 字节可能已到交易所,收敛后按真实结果处理 |

### 延迟样本

`LatencySample` 的写入时机唯一归属本节(记录布局与 `action` 码归 [hft-event](../hft-event/README.md),分段公式与分组归 [latency-budget](../../docs/latency-budget.md)):

1. **记戳**:`place`(新单)与拥有者的 `cancel`、`amend` 把触发事件的 `TrigStamps` 与操作 `op` 写进该槽的 `SlotStamps`,sequencer 随后填 `decided_tsc`;该操作的 `SendReport{Written}` 填 `sent_tsc`。槽上已有未合成的样本(`op ≠ 0xFF`)时,新的撤改不记戳、不产生样本,新单的样本不被覆盖。批量撤单(`cancel_all`、`cancel_open`、`cancel_resting_on`、`reissue_cancels`)与冒烟单到期撤单不记戳。
2. **合成**:该操作的首个交易所响应到达的同一事件内合成一条,`action = op`,随后 `op = 0xFF`。首个响应:新单 = 该槽的任何交易所回报(Ack、Reject、成交、CancelAck/Expired);撤单 = `CancelAck` 或 `CancelReject`;改单 = `AmendAck` 或 `AmendReject`。`ack_*` 取该回报,`link_role` 取 `SlotExt.role`,`trigger_kind` 取 `trig_kind`,`sw_rx_ns` 取 `trig_sw_rx_ns`。
3. 不合成、直接 `op = 0xFF` 的情形:该操作的 `SendReport` 报告确定未写出;恢复后的在途槽(`SlotStamps` 不进快照,恢复后 `op = 0xFF`);槽回收。
4. `sent_tsc` 同时是 NotFound grace 的起点(见"Unknown 收敛流程"):撤改写出后它是撤改的写出时刻,只会更晚(偏保守)。

### REPORT_DECODE

gw 对某 (venue, account) 的回报帧解码失败时写 `ConnState{role = UserStream, kind = Degraded, reason = REPORT_DECODE}`:OMS 按用户流缺口处理——该 (venue, account) 全部非终态槽置 `VERIFY` 并按 client id 查询;连续 `decode_fail_down_n` 次视为 Down(Pending* 进 `Unknown`)。计数 `decode_fails` 在该 (venue, account) 收到 `ConnState{UserStream, Up}` 时清零;达到 `decode_fail_down_n` 的那一次按 `ConnState{Order, Down}` 的槽迁移处理(Pending* → `Unknown`、`Live` 置 `VERIFY`;`conn_up` 不变)并清零计数。

### 恢复后的 gw 映射

`restore` 与 `SetEpoch` 之后,sequencer 对每个 (venue, account) 调 `mirror_restore`,把每个非终态槽(含 Unknown、VERIFY)作为 `GwCmd{MirrorRestore{slot, gen, epoch, inst, exch_order_id}}` 发给 gw(分批,经扇出队列),让 gw 重建 exch id → slot 私有表并能对上一 epoch 的槽查单;gw 无法为其编码查询的槽回 `QueryUnsupported`,OMS 保持 Unknown 并写 `Note{OmsNeedsRestQuery}` 转交 reconciler。

### 回报匹配与成交增量

1. `ev.slot == SLOT_UNRESOLVED`(0)→ 外来单:无成交 → `Note{OmsForeignOrder}`;带 `cum_filled > 0`(本应由 gw 写成 `ExternalFill`,这里是防御路径)→ 按 `foreign` 表求增量,先按第 4 条扣除 `ext_held`(只对未入账部分)后产出 `ForeignFill`,sequencer 按 `ExternalFill{cause: Unknown}` 走 hft-ledger 的归属与入账;写 `Note{OmsForeignFillViaOrderEvent}`(P1,venue crate 契约偏差)。
2. **匹配键是 `(ev.epoch, ev.slot, ev.gen)`**。与当前槽不符 → 查该槽的墓碑(见"墓碑与迟到成交");墓碑也没有 → `StaleGen`:不改本槽;不带成交的只计数(`OmsStats.stale_gen`),无 Note;带 `cum_filled > 0` 时**照样入账**——`ev.exch_order_id ≠ 0` 时按 `foreign` 表求增量并先扣除 `ext_held`(同第 1 条防御路径;扣完为 0 则不产出),产出 `ForeignFill`,sequencer 按 `ExternalFill{cause: Unknown}` 走 hft-ledger 的归属与入账(随后 hft-exec 的外来成交规则照常适用);`exch_order_id = 0` 时无法去重,不入账;两种情况都写 `Note{OmsStaleGenFill}`(P0,`v[2]` = 是否已入账);"请求对账"的载体就是这条 P0 Note,OMS 不另发命令或查询(仓位真值由 reconciler 下一轮对账裁决,重复入账或漏账都由对账采纳校正)。
3. 成交增量 `d = ev.cum_filled − slot.filled`:`d ≤ 0` → 成交部分视为重复/乱序,仅状态部分按表处理;`d > 0` → 增量名义 = `ev.avg_px × ev.cum_filled − slot.filled_notional`(`OrderEvent.avg_px` 为交易所给的累计均价,i128 积),`filled_notional` 取新值。
4. **与外来成交去重**(成交去重键的唯一定义是 [hft-venue-proto](../hft-venue-proto/README.md) 契约 22:`(inst, trade_id)`,强平/ADL 等 0 或负的成交 id 改用 `(inst, exch_order_id, fill_seq)`;本 crate 不另设键):gw 无法匹配到槽的成交先暂存 `gw.unmatched_hold_ms`(默认 5 000,等同账户的 Ack 补上 exch id),超时才报为 `ExternalFill{cause: Unknown}`(规则在 [hft-venue](../hft-venue/README.md))。若之后该单被查询或迟到回报匹配到槽,且 `ext_held[slot.exch_order_id] = b > 0`(这部分已按外来成交入账):槽的 `filled`、链接的 `first_net` 照常按全量 `d` 推进,但 `FillOut` 与 LegTracker 只计 `d − min(d, b)`(为 0 时不产出 `FillOut`),`b` 同量递减,`FillOut.flags |= EXT_DEDUP`,写 `Note{OmsExtDedup}`(P1)。第 1、2 条的 `ForeignFill` 同样按 `ext_held[ev.exch_order_id]` 先扣,只产出 `d − min(d, b)`,并写同一 Note。对冲按 OnFill 规则发出,数量受**该方向裸量**约束:若这段敞口已被 hft-exec 的外来成交默认处理或策略平掉,裸量为 0,不再重复对冲;若仍裸露,照常补发。
5. **`avg_px` 为 0**(交易所没给均价):`OrderEvent` 成交类回报 → 退回 `ev.px × d`(`ev.px` 是本笔成交价);`QueryResult`(`ev.px` 可能只是委托价)→ 以委托价计、`FillOut.flags |= PX_EST`、写 `Note{OmsFillPxEstimated}`(P1)与 `Note{OmsNeedsRestQuery, trades = 1}`,由对账的成交历史校正价格(校正走 ledger 的采纳路径)。venue 契约:`QueryResult` 在 `cum_filled > 0` 时应给均价,给不出的由 gw 先查成交再出事件。
6. **超量成交**:`ev.cum_filled > slot.qty`(只可能是解码/单位错误)→ 成交照常入账(`FillOut.flags |= OVERFILL`,由对账裁决),但超出 `qty` 的部分**不进入对冲基数**;同一事件内先按 OnFill 第 1–5 步对不超过 `qty` 的部分产生第二腿,再把该槽所在链接置 `Broken{Overfill}`;`Note{OmsOverfill}`(P0)。
7. `venue-<x>` 必须在每条成交回报里给出**累计成交量** `cum_filled` 与累计均价;给不出的在 gw 内按 exec id 去重合成;仍给不出则该 venue 不允许启用下单(加载时拒绝)。
8. **冷读者的入账依据**:第 1、2 条每产出一条 `ForeignFill`,同一事件内另写一条 `Note{OmsForeignFill}`(Info,Output 类,回放比对;字段唯一定义在此):`Note.inst = inst`、`Note.group = GROUP_NONE`,`v = [venue | account << 8 | side << 16 | path << 24 | fee_asset << 32, qty, px, fee, exch_order_id, trade_id, fill_seq, cum_filled]`(`path`:防御路径 = 1、`StaleGen` = 2;数值为 raw;`fill_seq` = `trade_id = 0` 时 `cum_filled` 的 raw 值,否则 0)。这笔外来成交与对账 `AUTO_EXTERNAL` 同理,由 sequencer 在本条 Input 内按这些字段确定地构造为 `ExternalFill{cause = Unknown}`(其余字段为 0)并入账,不另写 Input;冷读者(reconciler 的引擎视图、hft-py)在同一事件内读到本 Note 时构造同一 `ExternalFill`,经 hft-ledger 的 journal 消费路径入账(消费规则归 [hft-ledger 外部成交](../hft-ledger/README.md#外部成交externalfill),码名登记归 hft-event)。
9. **数量先由非成交回报带来时的费用**:`CancelReject`、`CancelAck`/`Expired`、`AmendAck`/`AmendReject`、`QueryResult` 与对账收敛带来的增量不带费用(第 3 条照常入账,`fee = 0`);随后到达、数量已被覆盖(`d ≤ 0`)的 `PartialFill`/`Fill`,若 `ev.cum_filled > SlotExt.fee_cum`,它的 `fee` 按 `FeeOnly` 入账(`FeeOut`,净量补差同"成交净量");`fee_cum ≥ ev.cum_filled` 的是重复回报,不入账。每条 `PartialFill`/`Fill` 之后 `fee_cum = max(fee_cum, ev.cum_filled)`;其他回报不改 `fee_cum`。

### 成交净量(B4)

LegTracker、对冲基数与 `FillOut.net_qty` 一律用**净量**(仓位与库存的真实变化)。净量规则的唯一定义在 [hft-ledger 成交增量与净量](../hft-ledger/README.md#成交增量与净量):回报里的 `fee_asset` 是该 inst 的基础资产时,现货买入 `net = qty − fee`、现货卖出 `net = qty + fee`(库存多减),永续与其他资产计费 `net = qty`;只看回报里的 `fee_asset`,不按交易所名字分支。本 crate 在处理成交回报时按该规则填 `FillOut.net_qty`、推进 `SlotExt.net_filled` 与 LegTracker。

- **费用未随成交到达**(`fee_asset = ASSET_NONE`,之后由 `FeeOnly` 补):先按 `net = qty` 计,不预估费用;第二腿因此按毛量对冲。
- **`FeeOnly` 到达**:本 crate 按 (epoch, slot, gen) 找到槽或墓碑,输出 `FeeOut{account, venue, inst, group, side, fee, fee_asset}`;hft-ledger 的 `apply_fee` 在费用为基础资产时返回 `NetAdjust{account, venue, group, inst, delta}`,sequencer 在同一事件内调 `net_adjust(a, g, inst, delta)`:该腿 `leg[i].net += delta`、槽的 `net_filled` 同量校正;若该 group 最近一条链接的第一腿是这个 inst 且未 Broken,按对冲比把 `delta × ratio` 记入对冲腿的 group 余数(负 = 已超额对冲),由下一笔 OnFill 吸收或按 hft-exec 的残余规则处理。找不到槽与墓碑 → `Note{OmsFeeUnattributed}`(P1),费用只记账户级(仍输出 `FeeOut`,`group = GROUP_NONE`、`inst = ev.inst`、`side` 为 0;无净量补差)。
- 现货卖出量取 min(库存, 可用余额) 是下单方的规则(hft-exec 切片、策略);OMS 不检查余额。

### Unknown 收敛流程

1. **进入**:① 下单连接 `ConnState{role=Order, Down}` → 该 (venue, account) 所有 `PendingNew/PendingCancel/PendingAmend` 进 `Unknown`(保存 `prev_state`),所有 `Live` 置 `VERIFY` 标志;用户流 `ConnState{role=UserStream, Down}` → 该 (venue, account) 所有非终态槽置 `VERIFY`(回报可能有缺口);② sweep 发现 `PendingNew` 超 `ack_timeout`、`Pending{Cancel,Amend}` 超 `cancel_timeout`、IOC/FOK `Live` 超 `ioc_timeout`;③ `SendReport` 报告加密后 I/O 错误(字节可能已部分发出);④ 冷启动恢复结束(`recovery_end`)时的全部非终态槽。**传输错误只能到这里,不能到 Rejected/Cancelled**([defensive-patterns](../../docs/defensive-patterns.md))。
2. **隔离**:`unknown_count += 1` → `venue_ready(v, a) = false`;风控对该 (account, group) 的开仓意图拒绝(`UNKNOWN_PENDING`),减仓与对冲照常。
3. **查询**:连接 Up 后,对每个 `Unknown` 与带 `VERIFY` 的槽发 `OrderDecision{action=Query}`(按 client id 查);不支持 WS 查询的所改为在 `Note{OmsNeedsRestQuery}` 里请求 reconciler。每 `unknown_query_interval` 重试,`query_count` 计数。时机(唯一定义):槽进入 `Unknown` 或被置 `VERIFY` 时登记它的 `Timer{Oms, UnknownQuery}`(`now + unknown_query_interval_ms`),下单连接此刻 Up 则同一调用内发一条 `Query`;连接 Down 期间不发,`ConnState{Order, Up}` 的那次调用对全部 `Unknown`/`VERIFY` 槽各发一条;`on_unknown_query` 到期且连接 Up 时再发一条(槽带 `HIST_WAIT` 时改为重写 `Note{OmsNeedsRestQuery, trades = 1}`,不发 `Query`),无论 Up 与否都重新登记。`query_count` 只在 `SendReport{Query, Written}` 时加 1。`VERIFY` 在该槽的 `QueryResult` 或匹配的 `ReconcileReport` 条目应用后清除。
4. **收敛**:`QueryResult` 映射 New/PartiallyFilled → Live,Filled → Filled,Canceled/Expired → Cancelled,Rejected → Rejected,均先应用成交增量。`NotFound` **一律先保持 `Unknown`**,再按下面两类处理:
   - **按成交历史确认**(默认):链接内的任何一腿(`SlotExt.role ≠ 0`:First、Second、SecondRetry、Reverse、HedgeFix,C1),以及 `stale_guard ∈ {None, RecvWindow}` 的 venue 上的**全部**槽。**永不按超时定终态**:`now − sent_tsc > not_found_grace`(`sent_tsc` 为 0 时以 `t_state` 为起点,下同)的同一事件置 `HIST_WAIT` 并写 `Note{OmsNeedsRestQuery, trades = 1}`(grace 内的 NotFound 只保持 `Unknown`、照常按 client id 查询),请 reconciler **按需**(不等其周期)查该单的**成交历史**;只有 `ReconcileReport` 的 `UnknownResolved` 条目 `history_checked = 1` 且 (epoch, slot, gen) 完全匹配时才收敛:无成交 → `Rejected(NOT_FOUND)`;有成交 → 先应用增量再按报告的终态迁移(累计成交取条目的 `exch_qty`,价格按第 5 条 `avg_px = 0` 的 `QueryResult` 规则以委托价计并置 `PX_EST`)。`history_checked = 0` 的条目只记录、不收敛,下一个 `unknown_query_interval_ms` 再请求一次。**等待成交历史确认期间的重请求与 `history_checked = 0` 的结果不计入 `query_count`**(`query_count` 只计按 client id 的查询):reconciler 按自己的 `notfound_grace_ms` 与"两个不同轮次"规则才可能给 1,早于此的重请求必然得 0,计入会让每个这类 NotFound 都先触发 `OmsUnknownStuck`;这类槽的升级只按 `unknown_alarm_ms`(`OmsUnknown` 升 P0 与 `UnknownTimeout` cap)。`history_checked` 的判据归 [reconciler](../../apps/reconciler/README.md):成交历史不带 client id 的 venue(Binance,待核实)按 orderId 归属——窗口内存在不属于任何已知槽、也不属于已登记外来单的 orderId 成交 → 给 0 并报 P0;grace 之后按 client id 复查两次仍"不存在"且无未归属成交才给 1。理由:对冲腿被误定为 Rejected 后重试,迟到成交会造成双重对冲;`RecvWindow`(Binance 等)只约束请求到达网关的时刻,已被网关接受、仍在撮合队列中的单不受它约束,查询"不存在"不能证明该单不会再成交(交易所在"执行状态未知"时的行为待核实,见开发备忘)。链接腿在确认期间 `hedge_deadline_ms` 照算:截止前未确认 → 链接按 `Deadline` Broken(敞口未被证明已对冲,交 hft-exec 的 Broken 处置;确认到达后 LegTracker 按真实成交更新)。
   - **按过期保护定终态**:只限 `stale_guard = ExpireAt` 的 venue(服务器时间超过请求里的绝对过期时刻即不处理,如 OKX `expTime`)上的**非链接槽**:`now − sent_tsc > not_found_grace` 时确定为 `Rejected(NOT_FOUND)`,否则继续查询。
   万一定终态之后仍有成交到达,按墓碑入账并报 P0。`sent_tsc` 在 `SlotStamps` 里、不进快照:恢复后的槽按 `recovery_end` 平移后的 `t_state` 计时(只会更晚,偏保守)。
   回报里未登记的交易所错误码按"结果未知"处理:槽进 `Unknown`,不当作 `Reject`(码表在各 `venue-<x>` 的 `codes.rs`)。`ReconcileReport` 的 `OpenOrder`/`UnknownResolved` 条目带 `epoch` 与 `gen`,**仅当 (epoch, slot, gen) 完全匹配且槽仍为 Unknown 或 VERIFY 时**按同一映射收敛,否则忽略并写 Note。
5. **升级**:`query_count > unknown_max_queries` → `Note{OmsUnknownStuck}`(P0;每次进入 `Unknown` 至多一条);`Unknown` 持续 > `unknown_alarm_ms`(`on_sweep` 按 `now − t_state` 判定,未置 `ALARMED` 时置位并再写一条 `level = P0` 的 `Note{OmsUnknown}`)→ sequencer 设 `UnknownTimeout{account, venue}` cap(只挡该 (account, venue) 的 Open;**对冲与重试照常**);解除需 `Resume{UnknownTimeout}` 且该 (account, venue) 已无 Unknown 槽。**Unknown 期间绝不对该订单做反向或重发**;它所属链接在其收敛前不发 `SecondRetry`(9/24 事故约束,[12 §1](../../design/12-风险与未决问题.md))。
6. 最后一个 `Unknown` 收敛且连接 Up → `venue_ready` 恢复,发 `Note{OmsVenueReady}`(`venue_ready` 每次由假变真恰好一条,含连接 Up 时已无 `Unknown` 的情形)。

### 墓碑与迟到成交(C1)

- 槽在 sweep 回收为 `Free` 时,把这一代的摘要写进 `tombs[slot × T + gen % T]`(`T = tombstone_gens`),覆盖最老的一代。墓碑超过 `tombstone_keep_ms`(以 `freed_tsc` 计)由 sweep 清零。
- 回报的 `(epoch, slot, gen)` 与当前槽不符、但与某个墓碑相符:`d = ev.cum_filled − tomb.filled > 0` → **照常入账**:`FillOut{flags: LATE}` 交 ledger、按墓碑的 group/腿更新 LegTracker、`tomb.filled`/`net_filled` 前移;`Note{OmsLateFill}`(**P0**)。不改当前槽、不重开已结束的链接、不产生任何订单;由此出现的多出一腿由 hft-exec 的补腿规则(抑制条件照常)或 `HedgeFix` 命令处理。`FeeOnly` 同样可按墓碑归属。
- 墓碑也没有(超过 T 代或已过期)→ 按"回报匹配"第 2 条的 `StaleGen` 处理(经 `ForeignFill` 入账、P0、请求对账)。

### StaleOrderSweep(`Timer{Oms, Sweep}`,1 s)

每次从 `sweep_cursor` 起最多检查 `sweep_batch` 个非空闲槽:① Pending 超时 → Unknown(上节);② `Live` GTC/PostOnly 年龄(`now − t_state`)> 意图的 `max_rest_ms`(≠ 0)且未置 `AGED` → 置 `AGED`、给拥有者 `OrderUpdate{Aged}`(每次进入 `Live` 至多一条),由拥有者决定撤/改(OMS 不替拥有者撤单);③ Unknown 槽:`unknown_alarm_ms` 升级(上节第 5 条)与所属链接的截止判定(见"LegLink 与 HedgePlan");查询重试由该槽的 `UnknownQuery` 定时器驱动(`on_unknown_query`),sweep 不发 `Query`;④ 终态超过 `reuse_quarantine` → 写墓碑并回收;⑤ 过期墓碑清零;⑥ 带 `SMOKE_DUE`、仍 `Live`、未置 `CANCEL_SENT` 的冒烟单补撤(见"冒烟单")。其余周期性维护(`DeadMan` 刷新、`ConnRotate`)是 sequencer 的系统定时器,不在本 crate([05 §5](../../design/05-CEX执行层.md))。

### LegLink 与 HedgePlan

`LegLink`、`HedgePlan`、`LinkRole`、`HedgeKind`、`PxMode`、`OnFail`、`ResidualPolicy`、`OrderRef` 的**类型**只在 [SDK](../hft-strategy-sdk/README.md) 定义(随 `OrderIntent`);`LinkRole` 的线上编码是 [hft-event `OrderDecision.link_role`](../hft-event/README.md);本 crate 不另定义副本,本节规定它们在 OMS 里的**语义**(`LinkRole` 各值的含义只归这里)。策略与 hft-exec 只给出 `role = First` 的链接;其余角色只由引擎产生。

| 字段 | OMS 语义 |
|---|---|
| `LegLink.role` | `First`:建立链接;`Second`:OnFill/Simultaneous 的第二腿;`SecondRetry`:第二腿失败后的重试(含恢复后的续接重试),只为计数与审计区分,分类与 `Second` 完全相同(B2);`Reverse`:失败反向;`HedgeFix`:只用于"平掉多出的一腿",只由 hft-exec 产生 |
| `LegLink.tolerance`(`Rate`) | 该 group 的补腿阈值;本 crate 只记录并随 `LinkUpdate` 转出,判定在 hft-exec |
| `HedgePlan.kind` | `None` \| `OnFill`(第一腿每个成交增量同一事件产生第二腿)\| `Simultaneous`(第一腿 `place` 的同一事件产生第二腿) |
| `HedgePlan.px_mode`、`px_param`、`max_slip`、`limit_px` | 第二腿定价,见"对冲定价";`max_slip` 以 `LinkEntry.ref_px` 为基准,超过时按越价上限定价(不算失败);`limit_px` 在非 Limit 模式下是保护价(0 = 无) |
| `HedgePlan.on_fail` | `Retry`(OnFill 默认)\| `Reverse` \| `Hold`;Simultaneous 只取 `Reverse` \| `Hold` |
| `HedgePlan.tif` | 第二腿固定 `Ioc`(Simultaneous 可 `Fok`) |
| `HedgePlan.venue/account/side/inst` | 第二腿所在 |
| `HedgePlan.reduce_only` | 1 = 第二腿带交易所侧 reduceOnly(第二腿是减仓且该腿支持时);开仓对冲为 0;`SecondRetry` 原样继承 |
| `HedgePlan.ratio_num/ratio_den` | 第二腿数量 = 第一腿成交净量 × num/den(同资产通常 1/1) |
| `HedgePlan.residual` | 随 `LinkUpdate` 转出给 hft-exec 的残余规则 |

**策略链接的结构校验**(纯函数,本表取值的可判定形式;sequencer 在意图管线的模式门之前对策略 `place` 的带链接意图调用,为假 → 只写 `OrderDecision{reject = INVALID}`、结果 `Rejected(INVALID)`,不经检查链、不建槽,[hft-sequencer 意图管线](../hft-sequencer/README.md#意图管线)):

```rust
/// 为真 ⇔ role = First;hedge.kind ∈ {OnFill, Simultaneous};kind = OnFill 时 on_fail ∈ {Retry, Reverse, Hold} 且 tif = Ioc;
/// kind = Simultaneous 时 on_fail ∈ {Reverse, Hold} 且 tif ∈ {Ioc, Fok};ratio_num > 0 且 ratio_den > 0。
pub fn link_valid(l: &LegLink) -> bool;
```

```rust
#[repr(C, align(8))]
pub struct LinkEntry {          // LinkTable 元素,80 B(本 crate 私有;布局为草案,以代码与编译期断言为准)
    pub link_id: u32, pub group: GroupId,                       //  0 / 4
    pub account: AccountId, pub state: LinkState /* Open | Hedging | Done | Broken */, pub retries: u8,
    pub flags: u8,              // 11 位:RESUME_HOLD=1(恢复后、续接前:不判 hedge_deadline_ms,受 resume_hold_max_ms 约束)| RESUME_RETRY=2(续接单终态前不判截止)
                                //    | SLIP_NOTED=4 STALE_NOTED=8 BEYOND_NOTED=16(对冲定价 Note 已写,见"对冲定价"第 7 条)
                                //    | RETRY_PENDING=32(已登记未到期的 HedgeRetry)| HEDGE_QUEUED=64(续接 Hedge 项在 pending_out)
                                //    | REVERSING=128(失败处理已转为 Reverse)
    pub plan: u32,              // 12 低 24 位 = 计划表下标(= 链接表下标;表内存 SDK 的 HedgePlan 与 LegLink.tolerance);
                                //    高 8 位 = Broken 时的 LinkBroken 码
    pub first: SlotRef,         // 16 12 B
    pub held_ms: u32,           // 28 RESUME_HOLD 时保存的已用引擎运行时间(≤ hedge_deadline_ms)
    pub first_net: Qty,         // 32 第一腿累计成交净量(对冲基数,超量部分不计)
    pub hedge_req: Qty,         // 40 应对冲量:OnFill 第 5 条每发出一条 Second 时 += q(SecondRetry 重发的是 owed,不再累加;被裸量上限核销的部分扣除)
    pub hedge_filled: Qty,      // 48 第二腿累计成交
    pub hedge_open: Qty,        // 56 第二腿非终态槽的剩余量(终态即扣除;不用"已发 − 已成"近似在途)
    pub first_fill_tsc: Tsc,    // 64 hedge_deadline_ms 起点:当前未对冲区间开始的那笔第一腿成交时刻;0 = 无未对冲量(含第一腿尚无成交)(进快照的已登记 TSC 字段,恢复时平移,见"恢复")
    pub ref_px: Px,             // 72 链接建立时第二腿的对手价(max_slip 基准)
}
const _: () = assert!(size_of::<LinkEntry>() == 80 && offset_of!(LinkEntry, first) == 16 && offset_of!(LinkEntry, first_net) == 32);
```

含枚举字段的结构(`OrderSlot`、`SlotExt`、`LinkEntry`、`Tomb`,以及快照中的 `HedgePlan`)从快照读回时用 `zerocopy::TryFromBytes`,非法判别值 → `RestoreError`。链接的欠对冲量 `owed = hedge_req − hedge_filled − hedge_open`。

**链接状态**(唯一定义):带 `LegLink{role = First}` 的意图 `place` 成功时建链接(`Open`;`first` = 该槽,`ref_px = ApprovedIntent.hedge_ref_px`;该账户 `LinkTable` 已满 → `Err(PlaceError::LinkTableFull)`);产出第一条对冲请求(OnFill 第 5 条或 Simultaneous)时 → `Hedging`;第一腿已终态且 `owed = 0`、`hedge_open = 0`、无已登记的 HedgeRetry、无在途 `Reverse` 时 → `Done`(第一腿零成交终态的 OnFill 链接同样 `Done`;`on_fail = Hold` 的失败处理 → `Done`);`Broken` 的来源见 OnFill 第 6、7 条、"回报匹配与成交增量"第 6 条与"恢复"第 5 条。每次状态变化产出一条 `LinkUpdate`。**链接号分配**:`link_id` = 全局链接表下标 + 1,取最低的空闲(`Free` 或 `Done`)下标(确定性);`Broken` 条目保留到 hft-exec 处置完。**`hedge_deadline_ms` 的判定时机**:该链接的失败处理(OnFill 第 6 条)、`hedge_gated`、`on_hedge_retry`,以及该链接任一槽的 `on_unknown_query` 与 sweep ③;以当次调用的 `now` 判定,条件为 `first_fill_tsc ≠ 0`、不处于 `RESUME_HOLD`/`RESUME_RETRY`、`owed > 0` 或 `hedge_open > 0`,且 `now − first_fill_tsc > hedge_deadline_ms` → `Broken{Deadline}`。

### OnFill

第一腿每个成交增量(含部分成交;链上第一腿为 `TxLifecycle::IncludedOk`,optional)在**同一事件处理内**:

1. 取净量增量 `d_net`(见"成交净量");超量成交部分不计(见"回报匹配"第 6 条)。先把本增量写进 LegTracker(该腿 `net` 与在途),再 `first_net += d_net`。**截止起点**:未对冲量 `u = first_net × num / den − hedge_filled`;本增量使 `|u|` 从 < 第二腿 lot 变为 ≥ 1 lot 时 `first_fill_tsc = max(now, 1)`(最早一笔尚未对冲完的成交;0 是"无"的哨兵);第二腿成交(或 `Reverse` 成交)使 `|u|` 回到 < 1 lot 时 `first_fill_tsc = 0`。`|u|` ≥ 1 lot 期间的后续第一腿成交不改起点——起点按未对冲区间计,不按逐笔先进先出前移(更保守)。
2. `raw = ⌊first_net × num / den⌋ − ⌊(first_net − d_net) × num / den⌋ + residual[g][hedge_leg]`(即 `d_net × num / den` 按链接累计换算,i128 中间积,截断误差不随成交笔数累积;**余数按 group 累计**,同 group 先后多条链接共享,可为负)。
3. `q_fill = raw` 按 lot **向零**取整;`q_fill < hedge_inst.min_qty` 或 `q_fill × ref_px < hedge_inst.min_notional`(`ref_px = 0` 时不判名义)→ `residual = raw`,本次不发。
4. 否则 `residual = raw − q_fill`;**第二腿数量上限**(D2):`q = min(q_fill, floor_to_lot(naked_dir))`,`naked_dir` 是 LegTracker 在对冲方向上的裸量(定义见 [LegTracker](#legtracker))。`q < q_fill` → 差额 `q_fill − q` 不计入余数、不计入 `hedge_req`(这段敞口已由别的途径覆盖),写 `Note{OmsHedgeNakedCapped}`(P1,`v = [link_id, q_fill, q, naked_dir]`);`q` 低于最小下单量 → 本次不发。
5. 发出时 `hedge_req += q`;产出 `HedgeRequest{role: Second, qty: q, basis: raw, hedge_cap_qty: raw + 1 lot, reduce_only: plan.reduce_only, ref_px}`。sequencer:`price_hedge` → 投影 `IntentView`(含 `hedge_cap_qty`)→ `classify` → `evaluate`(检查链 #6 `HEDGE_LIMIT`)→ `commit` → `place()`。第二腿永远 IOC。
6. **第二腿失败**(`HEDGE_LIMIT` 以外的风控拒、`price_hedge` 门控、`place` 错误、本地发送失败(`SendReport` 确定未写出)、交易所拒、IOC 未全成)且 `owed > 0`:
   - `on_fail = Retry`(OnFill 默认):登记 `Timer{Oms, HedgeRetry{link_id}}`,到期时刻 `max(now + hedge_retry_min_ms, retry_at)`(`retry_at` = 该 venue 的退避截止,只经 `hedge_gated` 传入,其余失败为 0);到期 `on_hedge_retry` 以当时价格产出 `HedgeRequest{role: SecondRetry, qty: min(owed, floor_to_lot(naked_dir)), basis: owed, hedge_cap_qty: owed + 1 lot, reduce_only: plan.reduce_only}`(新槽、新 id、重新过风控)。被裸量上限截掉的部分从 `hedge_req` 核销(`owed` 随之减少;`owed` 归零且无在途 → 链接 Done),写 `Note{OmsHedgeNakedCapped}`。**角色保持第二腿语义**:开仓链接的重试不带 reduceOnly,分类为 Hedge、RISK_REDUCING(分类表在 hft-risk)。该链接有 Unknown 的第二腿槽时不重试,等收敛。
   - `on_fail = Reverse`:不重试,对第一腿已成交未对冲的净量(`first_net − hedge_filled × den / num`,lot 向零取整)发 `HedgeRequest{role: Reverse, reduce_only: true}`(不受裸量上限约束;`basis` = 该未对冲量,`ref_px = 0`),同时 `hedge_req = hedge_filled + hedge_open`(未对冲部分改由 Reverse 处置,`owed` 归 0;Done 条件中的"无在途 `Reverse`"保证反向终态前不 Done)。`Reverse` 的成交从被反向那一腿的链接计数中扣除(第一腿 `first_net −= d_net`;Simultaneous 在第二腿上反向时 `hedge_filled −= d`);`Reverse` 失败同样经 HedgeRetry 以当时的未对冲量重发 `Reverse`,计次与截止同下,计次耗尽 → `Broken{ReverseFailed}`。
   - `on_fail = Hold`:不发单,只记 LegTracker;链接置 Done,剩余敞口交 hft-exec 规则。
   - 计次:执行结果类失败(交易所拒、IOC 未全成、`place` 错误、计次类的本地发送失败)`retries += 1`,上限 `hedge_retry_max`(失败后 `retries ≤ hedge_retry_max` 才登记重试,`retries > hedge_retry_max` 即计次耗尽:最多发出 `hedge_retry_max` 次重试单);**门控拒绝(kill、cap、风控门、`price_hedge` 的 `StaleQuote`/`BeyondCross`,经 `hedge_gated` 交回;不计次类的本地发送失败)不计次但时间照算**;本地发送失败的分类见"SendReport 处理"。链接不处于 `RESUME_HOLD`/`RESUME_RETRY` 时,`now − first_fill_tsc > hedge_deadline_ms` 仍未对冲完,或计次耗尽 → `LinkState::Broken` + `Note{OmsHedgeBroken}`(P0)+ `LinkUpdate`;sequencer 设 `Broken{account, group}` cap(hft-risk 据此对该账户设 `HedgeFailure`,挡全部 Open)。此后该 group 的未对冲量由 [hft-exec 的 Broken 处置](../hft-exec/README.md#broken-处置)(`broken_auto_fix_ms` 后一次可执行的自动有界平仓尝试、L3/flatten 永续先)或 `HedgeFix` 命令处理。
7. `hedge_vetoed(link_id)`(风控以 `HEDGE_LIMIT` 拒绝:数量超 `hedge_cap_qty` 或名义超 `max_hedge_notional`,上限规则归 [hft-exec](../hft-exec/README.md))→ 不重试,链接直接 `Broken{HedgeLimit}` + P0。

### Simultaneous

`HedgePlan.kind = Simultaneous` 的第一腿在 `place()` 的**同一事件内**产出第二腿 `HedgeRequest{role: Second, qty: floor_to_lot(qty × ratio + residual)}`(第二腿在风控里按 Open 分类,检查两腿 venue;尚无成交,不受裸量上限约束)。两腿都终态后按成交净量比较:

- 第二腿少于第一腿 × ratio:`on_fail = Reverse` → 对第一腿已成交未对冲部分发 `Reverse`(reduce-only,IOC);`Hold` → 只进 LegTracker。
- 第一腿少于第二腿 / ratio(第一腿被拒或部分成交):对称处理,在第二腿上发 `Reverse`。
- 反向单失败按 OnFill 第 6 条的计次与截止,耗尽 → `Broken{ReverseFailed}`。

是否选用 Simultaneous 由下单方(hft-exec 的 `ExecSpec` 或策略)决定;本 crate 只提供机制。

### 对冲定价

`price_hedge` 是纯函数,sequencer 在第二腿、重试与反向单出环前调用;`lim`(hft-types `HedgePxLimits`)由 sequencer 按对冲单账户与 inst 从 hft-risk `RiskParams` 填(`conn_liveness_ms`、`rr_max_cross`),与风控同值同源。限价一律由 hft-types 纯函数 `cap_cross_px` 给出——hft-exec 的全部引擎单用同一个函数([hft-exec 限价与单笔名义](../hft-exec/README.md#限价与单笔名义)),所以第二腿、引擎减仓与补腿对风控 #5 是同一口径。目标:对冲腿在退化行情下仍然发得出去,只在连越价上限都吃不到时才等待。

1. **报价**(按连接健康,不按盘口最后变化的时刻):BBO 两侧都有价,且 SDK `BboView` 的连接健康判定成立——`conn_up`、`resync = false`、`now − bbo.conn_live_tsc ≤ lim.conn_liveness_ms`(`conn_live_tsc` = 承载该 inst 的 BBO 成员连接最近一次活性证据——数据、pong、心跳——的时刻,本次运行的 TSC;成员聚合的唯一定义在 [hft-sequencer 连接活性](../hft-sequencer/README.md#连接活性),判定式 `quote_live` 归 [hft-strategy-sdk `BboView`](../hft-strategy-sdk/README.md))。报价取该连接上的最新值,**盘口多久没变都有效**:安静行情下第二腿的对手价不变,对冲照常定价发出。不成立(断开、重同步、超过 `conn_liveness_ms` 无任何帧、缺一侧):`limit_px ≠ 0` → 以保护价作 IOC 限价(`capped = false`);否则 `Err(StaleQuote)`,写 `Note{OmsHedgePxStale}`(P1;含义 = 报价连接不健康)。
2. **越价上限** `cap`:`mid = (bid + ask) / 2`;买 `cap = mid × (1 + rr_max_cross)`,卖 `cap = mid × (1 − rr_max_cross)`,取整方向与 `cap_cross_px` 相同(买向下、卖向上取到 tick)。这与 hft-risk 检查链 #5 对风险递减单的越价上限是同一公式,所以本函数定出的价不会被 #5 拒。
3. **吃不到**:对手价 `touch`(买取 ask、卖取 bid)已越过 `cap`(买 `ask > cap`、卖 `bid < cap`)→ `Err(BeyondCross)`,写 `Note{OmsHedgePxGated}`(P1;每链接每次进入此状态一条)。
4. **定价**:偏离 `dev = |touch − ref_px| / ref_px`(`ref_px` = `LinkEntry.ref_px`):
   - `dev ≤ max_slip`:`Cross` → `cap_cross_px(side, touch, mid, px_param, lim, tick)`;`Mid` → `cap_cross_px(side, mid, mid, px_param, lim, tick)`;`Limit` → `limit_px` 夹到 `cap` 以内(买取 min、卖取 max)。结果等于 `cap`(被上限截住)时 `capped = true`。
   - `dev > max_slip`(**超滑点改价而不失败**):`Cross`/`Mid` 直接以 `cap` 作限价(`capped = true`),写 `Note{OmsHedgeSlipCapped}`(P1,`v = [link_id, ref_px, touch, cap]`;每链接每次进入此状态一条);`Limit` 仍用 `limit_px`(策略指定的价不被覆盖),夹到 `cap`。
   - 仍是 IOC。
5. 两种 `Err` 都是**门控**:sequencer 调 `hedge_gated(link_id, Px(err))`,本 crate 登记 HedgeRetry、不计次、截止照算。超滑点本身不是失败;IOC 在 `cap` 内仍未成交才按执行失败计次。
   - `ref_px = 0`(第一腿 `place` 时第二腿报价不可用)时第 4 步按 `dev ≤ max_slip` 处理(无基准,不判超滑点);`Reverse` 一律按 `Cross` 定价(`px_param` 取 `HedgePlan.px_param`),不用 `limit_px`、`ref_px = 0`,报价取被反向那一腿的 inst。
6. 风控价格带对风险递减单只查越价上限([hft-risk](../hft-risk/README.md));本函数不重复其余检查。
7. **Note 的写入**(`price_hedge` 是纯函数,不写 Note):`OmsHedgePxStale`/`OmsHedgePxGated` 由 `hedge_gated` 按 `HedgeGate::Px(StaleQuote | BeyondCross)` 写,`OmsHedgeSlipCapped` 由该对冲请求的 `place` 按 `ApprovedIntent.hedge_px.slipped` 写;每链接每次进入该状态一条:写后置 `LinkEntry.flags` 的 `STALE_NOTED`/`BEYOND_NOTED`/`SLIP_NOTED`,该链接下一次 `place` 对冲请求时清 `STALE_NOTED` 与 `BEYOND_NOTED`,`hedge_px.slipped = false` 时清 `SLIP_NOTED`。

### 数量对齐

第二腿数量只在 OMS 决定:下单方给的是第一腿数量与 HedgePlan,不介入第二腿数量;`hft-types` 的按资产定指数保证同一资产两个 venue 的 `Qty` 同尺度([13 R4](../../design/13-设计评审与优化.md)),`ratio` 只用于合约乘数不同或故意的对冲比。合约张数与资产数量的换算在 refdata 的线格式指数与各 `venue-<x>` 编码层完成,OMS 只见资产数量。

两腿 lot 不同时,**第一腿数量的取整归下单方**:hft-exec 的配对算法把第一腿量向零取整到两腿的 `pair_lot`(一方整除另一方时取较粗者,否则取最小公倍数;lcm 名义过大的 group 在 `SetParams` 被 `InvalidArgs{LotMismatch}` 拒绝),规则唯一归属 [hft-exec 两腿 lot 对齐](../hft-exec/README.md#两腿-lot-对齐);直接经 `Ctx::place` 带 LegLink 的策略须用同一规则(取整函数 `pair_lot` 在 [hft-types](../hft-types/README.md) 的数量取整模块,引擎与策略共用一份实现)。因此第一腿**全部成交**时,OnFill 第 3 步算出的第二腿量恰为其 lot 的整数倍且 ≥ 其 `min_qty`,不产生余数;IOC 部分成交、现货基础资产扣费与晚到的 `FeeOnly` 产生的零头照旧进 group 余数(OnFill 第 2–4 步)。本 crate 不重新取整第一腿、不拒绝未对齐的第一腿(那是下单方的不变量,由 hft-exec 属性测试保证)。

### 风控所需的链接信息(B1)

- 带 LegLink 的 Open 意图:sequencer 从 `OrderIntent.link.hedge.{venue, account}` 填 `IntentView.link_venue/link_account`,hft-risk 对两腿 venue 同时执行 cap/kill、`venue_ready`/Unknown 门、令牌桶、第二腿报价连接健康(`LINK_STALE`,与 `price_hedge` 第 1 步同一判定)与第二腿可负担性(`LINK_FUNDS`)检查(规则归 [hft-risk](../hft-risk/README.md))。本 crate 提供:`venue_ready(v, a)` 对任一侧可查;`link_peer(link_id)` 返回链接另一腿的 (venue, account);`HedgeRequest.peer_venue/peer_account` 供 Simultaneous 第二腿(Open 类)做同样的双 venue 检查。
- 链接 `Broken` 经 `LinkUpdate` 与 `Note{OmsHedgeBroken}` 交 sequencer,由它设 `Broken{account, group}`;账户级 `HedgeFailure` 的升级规则归 hft-risk。

### RISK_REDUCING 判定

分类的唯一归属是 [hft-risk `classify()`](../hft-risk/README.md#分类):sequencer 在 evaluate 之前调用,OMS 只把 `risk_reducing`/`reduce_only` 拷进 `OrderDecision.oflags` 与槽的 `oflags`;gw 与链上意图只看这些位。`RISK_REDUCING`、`REDUCE_ONLY` 等的**位值唯一归属 [hft-event `OrderFlags`](../hft-event/README.md)**,本 crate 不另定义位值;本 crate 私有的槽状态位在独立的 `SlotFlags` 里,不与线上位共用一个字段。带 LegLink 的第一腿与本 crate 产生的 `Second`、`SecondRetry`、`Reverse` 都置 `LINKED`(执行层的 `HedgeFix` 单置 `HEDGE_FIX`,不置 `LINKED`);`tif = PostOnly` 置 `POST_ONLY`;冒烟单置 `SMOKE`。本 crate 给分类的输入是 `HedgeRequest.role` 与 `reduce_only`:`Second` 与 `SecondRetry` 的 `reduce_only` 都等于 `HedgePlan.reduce_only`(开仓链接 0、平仓链接 1);`Reverse` 恒为 1。

非 HARD 的 Kill(含 Viewer 的紧急停止)不拦 `RISK_REDUCING`:已成交第一腿的第二腿、`SecondRetry`、`Reverse` 照常出环,撤单义务只针对 Open 类挂单(`cancel_all`/`reissue_cancels` 的 `open_only = true`;批量撤单的使用条件见"输出扇出")。`HARD` kill(全局或账户级)下连风险递减单也拦:第二腿意图在 [hft-risk 检查链](../hft-risk/README.md#检查链按廉价--昂贵短路适用范围以本表为唯一定义) #1 被拒(`HARD`;只写 `OrderDecision`,不建槽、不出环),sequencer 经 `hedge_gated(…, HedgeGate::Risk(HARD), …)` 交回 → HedgeRetry 流程(门控拒绝不计次、时间照算,超 `hedge_deadline_ms` → Broken);cap 生效之前 kill 字已置位的窗口里已出环的单由 gw 回 `KillBlocked` → 槽 `Rejected(LOCAL_SEND_FAIL)`,按同一门控处理。**解除任何 kill 或锁存都不立即产生订单**:只有 HARD 解除时 sequencer 用 `naked_groups()` 列出候选、按 hft-exec `resolve_broken` 判定后设 `Broken` cap;之后按 hft-exec 的 Broken 处置(到期后一次可执行的自动有界平仓尝试,或人工 `HedgeFix`)。

### 外来成交(B3)

自身被强平、被 ADL、人工在网页下单成交、交割,都不是我们的 client id。识别方法属于各 venue(`VenueCaps.own_liquidation_marker`,写在 `venue-<x>`);gw 把**有成交的外来单**写成 `ExternalFill{cause: Liquidation | Adl | Manual | Delivery | Unknown}`(Input);无法匹配到槽的成交在 gw 暂存 `gw.unmatched_hold_ms`(默认 5 000)后同样报为 `cause = Unknown`([hft-venue](../hft-venue/README.md))。group 归属与入账归 [hft-ledger](../hft-ledger/README.md)(`apply_external` → `ExternalApplied{group, net_qty}`,P0 `Note{ExternalFill}` 由 ledger 写)。本 crate 的部分:

1. `external_adjust(a, v, g, inst, net, exch_order_id, cause)`:`g ≠ GROUP_NONE` 时该腿 `leg[i].net += net`,重算 naked;不改任何槽与链接(链接的对冲账照旧,LegTracker 反映真实敞口)。`g == GROUP_NONE` 时 LegTracker 不变。
2. `cause = Unknown` 且 `exch_order_id ≠ 0` → `ext_held[exch_order_id] += |net|`(容量 `ext_held_track`,FIFO 淘汰写 Note;本 crate 自己产出的 `ForeignFill` 入账时 sequencer 传 `exch_order_id = 0`,不登记——其累计量已在 `foreign` 表):若该单之后被查询或迟到回报匹配到我们的槽,按"回报匹配"第 4 条去重,不重复入账;对冲受该方向裸量约束,不重复对冲。该单之后经防御路径或 `StaleGen` 再到达时同样先扣("回报匹配"第 1、2 条)。
3. 本 crate 不产生任何订单;"平掉多出的一腿"的默认处理(以及 `Manual` 不做默认处理)在 [hft-exec](../hft-exec/README.md#外来成交的默认处理b3)。

防御路径:若 gw 仍以 `OrderEvent{SLOT_UNRESOLVED}` 报来带成交的外来单,按 `foreign` 表(`exch_order_id → 已入账累计成交`,FIFO 淘汰,淘汰写 Note)求增量、先扣 `ext_held`,产出 `ForeignFill`,sequencer 按 `ExternalFill{cause: Unknown}` 走同一流程。

### 冒烟单(SmokeOrder)

Admin 的 `SmokeOrder{account, venue, inst, side, ttl_ms}` 用来在正式端点上验证签名、下单与撤单路径以及死手刷新:挂一张远离市价、最小名义的 PostOnly,`ttl_ms` 后由本 crate 自动撤销。命令的权限、确认、`ttl_ms` 的默认与上限、结果码归 [control-plane](../../docs/subsystems/control-plane.md),载荷布局与 `OrderFlags::SMOKE` 位值归 [hft-event](../hft-event/README.md);下面的生成、建槽与到期规则只归本 crate。

1. **生成**(`smoke_intent`,只读,不改状态):
   - 每 (account, venue) 同时至多一张:该表 `smoke_slot ≠ 0` → `Err(Busy)`;inst 不属于该 venue → `Err(BadInst)`。
   - 报价:该 inst 的 `BboView` 两侧有价且 `quote_live(now, conn_liveness_ms)`(阈值与风控同值同源),否则 `Err(NoQuote)`。
   - 价格:对手价 `touch`(买取 ask、卖取 bid);买 `px` = `touch × (1 − smoke_far_bp)` 向下取整到 tick,卖 `px` = `touch × (1 + smoke_far_bp)` 向上取整到 tick(`Rate` 比率,i128 中间积),比对手价远至少 `smoke_far_bp`,PostOnly 不会作为 taker 成交。
   - 数量:`qty` = max(`min_qty`, `min_notional × 1.1 / px`) **向上**取整到 lot(i128;冒烟单是唯一向上取整的下单数量,目的是确保过交易所最小名义)。
   - 意图:`tif = PostOnly`、`group = GROUP_NONE`、`strategy` = 该账户绑定的实例(只填槽的拥有者字段与 Note;不计入任何策略、不改任何目标;检查链 #0 对冒烟单的口径归 hft-risk)、`max_rest_ms = 0`、`link` 为空。
   - 本 crate 不检查交易所的价格保护带(各所对挂单价偏离的百分比限制;refdata 没有这项):被交易所拒单时如实回报(`Reject`),不重试。各所是否接受离对手价 `smoke_far_bp` 的限价:待核实(方法:各所 exchangeInfo / instruments 文档 + 测试网挂单)。
2. **风控与模式**:sequencer 让它走完整检查链(`classify` → `evaluate` → `commit`),**不经模式门**,所以 Shadow 与 CloseOnly 下照样发出;HARD 下被检查链 #1 拒绝。风控对 `SMOKE` 意图的放行口径归 [hft-risk](../hft-risk/README.md)。
3. **建槽**(`place`,`ApprovedIntent.origin = Command` 且 `smoke_ttl_ms > 0`):槽 `oflags |= SMOKE`、`SlotExt.origin = Command`、`group = GROUP_NONE`;`smoke_slot = slot_id`;登记 `Timer{Oms, SmokeTtl}`(键 (account, venue),每 (account, venue) 至多一张所以唯一),到期时刻 = 当前事件 tsc + `smoke_ttl_ms`(按本次运行的 tsc 频率换算,与其他 `*_ms` 参数相同)。不建链接、不进 LegTracker,照常计入 InflightBook。
4. **回报**:迁移按回报处理表;没有拥有者策略,`OrderUpdate` 不投递,只写 `OmsTransition`。成交(远离市价的 PostOnly 本不应成交)照常入账(`GROUP_NONE`,ledger 记为未归属持仓),另写 `Note{OmsAnomaly}`(实际级别 **P0**,`v[0]` = `SmokeFilled`,见"Note 码表")。
5. **到期撤单**(`on_smoke_ttl`,按 `smoke_slot` 找到槽;`smoke_slot = 0` 时无动作):槽为 `Live` 或 `PendingNew` → `cancel`(迁移原因 `SmokeTtl`);槽在 `Unknown` → 置 `SMOKE_DUE`,收敛为 `Live` 的同一事件内撤;已终态 → 无动作。sweep 对带 `SMOKE_DUE`、仍 `Live`、未置 `CANCEL_SENT` 的冒烟单补撤(重启后定时器平移与 Unknown 收敛的兜底)。槽终态时 `smoke_slot = 0` 并撤销该 (account, venue) 的 `SmokeTtl` 定时器,下一张冒烟单不会被上一张的定时器提前撤掉。**唯一例外是不撤挂单的 Drain**:`Drain{cancel_resting = false}` 生效期间到期(`keep = true`)且该 venue 有交易所死手(`VenueCaps.dms ≠ None`)→ 不撤、不置 `SMOKE_DUE`,冒烟单保留到进程退出、由交易所死手撤掉(launch D-smoke 第 2 步靠它验证死手);没有死手的 venue 上 `keep` 不生效,照常撤单(否则干净退出后无人撤它)。其余情况一律按 `ttl_ms` 自动撤单。冒烟单不跨运行保留:`recovery_end` 对仍非终态的冒烟单槽置 `SMOKE_DUE`,收敛为 `Live` 的同一事件内撤(sweep 补撤兜底)。
6. Kill、`Drain{cancel_resting = true}`、`CancelAll` 与 `cancel_resting_on` 照常撤冒烟单(它不是 `RISK_REDUCING`);`Drain{cancel_resting = false}` 不撤它(冒烟单无链接,不属于该 Drain 一律撤的"带 LegLink 的挂单第一腿"),到期按第 5 条处理。

### 残余核销

`write_off_residual(a, g)` 是 Admin 升级命令 `WriteOffResidual{account, group}` 的 OMS 部分(权限与确认归 [control-plane](../../docs/subsystems/control-plane.md))。核销的语义——适用哪些 group、移出哪一腿、LegTracker 怎么调整、清哪些余数——只在本节定义,control-plane、hft-sequencer、hft-ledger、SDK 以此为准:把多出那一腿高于尘埃线却低于最小下单量、补腿与自动有界平仓都下不了单的已成交裸量从该 group 移到账户级(unassigned),并清零该 group 的全部余数,使 Broken 能按尘埃口径解除、非 Broken 的 group 能被移除。

1. **前置**(本 crate 检查):group 有 `LegState` 且**静止**(定义见"LegTracker");Broken 与否均可。否则 `Err(NotQuiescent)`(sequencer 回 `PreconditionFailed{NotQuiescent}`,结果码归 control-plane)。
2. `naked_sum = Σ leg[i].net`(各腿同资产同尺度);多出的一腿 = 符号与 `naked_sum` 相同、|`net`| 最大的腿(与 hft-exec 补腿同一选择规则)。
3. 多出的那一腿 `net -= naked_sum`(其余腿的 `net` 不动);该 group **全部腿**的 `residual` 清零(对冲余数记在对冲腿上,只清多出那一腿会留下余数,之后的 OnFill 会把它"补对冲"成反向裸量);重算 naked。返回单条 `WriteOff{venue, inst, leg_ix, qty: naked_sum}`;`naked_sum = 0`(没有多出的一腿)时只清余数,返回 `qty = 0`,sequencer 不做 ledger 调整。不改任何槽与链接,不产生订单。
4. sequencer 在同一 Input 内把这条 `WriteOff` 转成 hft-ledger 的 `AdjustDelta::WriteOff`(这一腿的该量从 group 移到 unassigned / 账户级,记账规则归 [hft-ledger](../hft-ledger/README.md)),再调 `exec.on_leg_adjust` 与 `risk.on_legs`;Broken 的 group 已成交裸量为 0,hft-risk 解除 `Broken`([hft-exec 残余与尘埃](../hft-exec/README.md#残余与尘埃))。

### LegTracker

每 (account, group) 一个 `LegState`,内嵌 SDK 的 `LegView`(`Views::legs` 直接返回它的引用;类型在 SDK,本 crate 是唯一写者),另有本 crate 私有的计数。由 OMS 在每个成交增量、`net_adjust`、`external_adjust`、`adopt_adjust`、`write_off_residual` 与每次槽迁移里更新。`GROUP_NONE` 没有 `LegState`。

```rust
#[repr(C)]
pub struct LegState {                 // 192 B(布局为草案,以代码与编译期断言为准)
    pub view: LegView,                //   0 160 B,SDK:leg[i] = { net(净量,含外来成交与采纳),
                                      //  inflight_immediate(即时在途,带符号)、inflight_resting(挂单在途,带符号)——划分见下,
                                      //  residual(对冲腿未达最小下单量的余数,按 group 累计;负 = 已超额对冲) },
                                      //  naked_qty = |Σ (net + inflight_immediate)|、naked_notional、naked_ratio、open_links、has_unknown、has_pending
    pub tolerance: Rate,              // 160 最近一条链接的 LegLink.tolerance(随 LinkUpdate 转出)
    pub last_fill_tsc: Tsc,           // 168 进快照的已登记 TSC 字段(见"恢复")
    pub last_link: u32,               // 176 最近一条链接(net_adjust 的余数归属用)
    pub broken_links: u16,            // 180
    pub pending_slots: u16,           // 182 has_pending = pending_slots > 0(Pending* 槽)
    pub unknown_slots: u16,           // 184 has_unknown = unknown_slots > 0
    pub last_px48: [u8; 6],           // 186 该 group 最近成交价(48 位有符号 raw,小端;naked_notional 用,超出 ±2^47 饱和)
}
const _: () = assert!(size_of::<LegState>() == 192);
```

- **在途的两类**(划分的唯一定义;SDK `LegView` 与 glossary 链接这里):一个非终态槽的剩余量只计入其中一类。

  | 槽 | 即时在途 `inflight_immediate` | 挂单在途 `inflight_resting` |
  |---|---|---|
  | IOC/FOK,任何非终态状态 | ✓ | |
  | GTC/PostOnly,未置 `ACKED`(`PendingNew`,或由 `PendingNew` 撤单而来、尚未收到 Ack 的 `PendingCancel`) | ✓ | |
  | GTC/PostOnly,已置 `ACKED` 的 `Live`、`PendingCancel`、`PendingAmend`(撤单或改单未确认期间仍按挂单计) | | ✓ |
  | 任何槽处于 `Unknown`(结果未知,按最坏处理) | ✓ | |

  **裸量与补腿抑制只用即时在途**:挂着的 GTC/PostOnly 不保证成交,不能当作对冲覆盖(例:永续腿挂卖 1.0 GTC 时现货买 1.0 成交,裸量仍是 1.0,第二腿照常按裸量发出);策略在带 LegLink 的腿上挂 GTC/PostOnly 因此是允许的。风控仓位检查 #7 对 Open 与 Reduce 计两类之和(防重复开仓),对 Hedge 与 Flatten 只扣即时在途;两种读数都由 [InflightBook](#inflightbook) 提供。
- **naked 的定义**:各腿同资产同尺度,`Σ` 即净敞口;即时在途按方向带符号计入,第二腿已发出、未回报时不会被当作裸露而再补一次。`naked_ratio = naked_qty / max(|net|)`、`naked_notional = naked_qty × 该 group 最近成交价`,供风控敞口门。只看已成交的敞口(`|Σ net|`,hft-exec 的补腿、Broken 处置与 hft-risk 解除 Broken 的口径)由读者从 `leg[i].net` 自行计算。
- **该方向裸量 `naked_dir`**(第二腿数量上限,唯一定义在此):取链接的第一腿 `f` 与对冲腿 `h`,折算到对冲腿单位 `E = net_f × ratio_num / ratio_den + net_h`(带符号,i128 中间积);在途只计**会减少 |E| 的即时在途**(对冲腿上的在途对冲、第一腿上的反向在途),第一腿尚未成交的同向在途与任何挂单在途都不计。对冲为卖:`naked_dir = max(0, E + Σ_i min(0, w_i × inflight_immediate_i))`;对冲为买:`naked_dir = max(0, −(E + Σ_i max(0, w_i × inflight_immediate_i)))`;`w_f = ratio_num / ratio_den`、`w_h = 1`。它在 OnFill 第 1 步把本次成交写进 LegTracker 之后计算,O(2)。
- **订单侧抑制条件**:`open_links > 0`(group 内有 Open/Hedging 链接)、`has_unknown`、任一腿 `inflight_immediate ≠ 0`;hft-exec 经 `Views::legs` 读取,与 cap、HARD 合并判断;任一成立时不发任何自动补腿。
- **静止**(group 级,唯一定义在此):`open_links = 0`、`has_unknown = false`、`has_pending = false`、各腿 `inflight_immediate = 0`(挂单在途不影响,它不是对冲覆盖)。残余核销的前置条件与 hft-exec 自动有界平仓的等待条件都用它。
- 阈值与残余规则全部是参数:补腿阈值来自链接的 `LegLink.tolerance`(或 hft-exec 默认),尘埃线与残余策略在 hft-exec;本结构不含任何策略的数字。
- 策略构件库没有自己的 LegTracker,一律读 `Views::legs`。

### InflightBook

在 `place`/每次迁移时 O(1) 增减的在途账本,风控的仓位、名义与可负担性检查读它,不必逐单求和。仓位项按 (account, inst, side) 同时给出两类在途之和与只计即时在途的读数(两类的划分见 [LegTracker](#legtracker)"在途的两类"表;Ack、进 Unknown 等使剩余量在两类之间移动的迁移同步更新 `imm_*`):风控检查链 #7 对 Open 与 Reduce 计两类之和,对 Flatten 与 Hedge 只扣即时在途(规则与读数口径归 [hft-risk](../hft-risk/README.md)):

| 项 | 键 | 内容 | 读者 |
|---|---|---|---|
| `open_buy`、`open_sell` | (account, inst) | 非终态槽剩余量(两类在途之和) | 仓位检查 #7 的 Open 与 Reduce(防重复开仓;hft-risk `open_qty`、`pending_reduce`) |
| `imm_buy`、`imm_sell` | (account, inst) | 同上,只计即时在途 | 仓位检查 #7 的 Flatten 与 Hedge(hft-risk `open_qty_immediate`、`pending_reduce_immediate`) |
| 在途名义 | (account, group)、(account, venue) | 非终态 Open 类槽剩余量 × 委托价 | 名义检查 |
| `buy_notional` | (account, venue) | 非终态现货买单剩余量 × 委托价 | 可负担性 `avail_quote` |
| `open_margin` | (account, venue) | 非终态永续 Open 类槽的 剩余量 × 委托价 × 100 / `lev_x100`(`SlotExt.lev_x100`,下单时风控所用的杠杆;0 按 1 倍) | 可负担性 `avail_margin` |

`open_margin` 按下单时的杠杆计入,杠杆之后变化不重算已在途的槽(在途 IOC 寿命短;挂单的偏差持续到该槽终态,由下一份 `MarginReport` 的交易所可用保证金吸收)。读数口径归 [hft-risk](../hft-risk/README.md)。

### Drain 静止判定(支持 C11)

`quiescent(a)` 返回该账户在 Drain 意义上仍未就绪的原因位(账户级,与 LegTracker 的 group 级"静止"不是同一判定):`LINKS_ACTIVE`(有 Open/Hedging 链接)、`IOC_NONTERMINAL`(有非终态的 IOC/FOK 槽,含已 Ack 的)、`HEDGE_RETRY_PENDING`(有已登记未到期的 HedgeRetry)、`UNKNOWN`。Drain 何时可以结束的完整条件归 [hft-sequencer](../hft-sequencer/README.md)。

### 恢复:停机时间不计与闸门续接

`hedge_deadline_ms` 只按引擎运行时间累计:冷启动(恢复与对账约 30 s)本身就超过默认的 3 s 截止,若把停机与续接前的等待算进去,每个崩溃前在途的链接都会一启动就 Broken。续接只等续接单所在 venue 的对冲闸门子集,不等整个账户的启动闸门,并受 `resume_hold_max_ms` 约束。规则:

1. **`restore` 只装载**,不改任何 TSC 量:尾部重放用的是旧运行的事件 tsc,与快照内的值同一次运行,必须原样比较,否则重放出的 Broken 等决定会偏离 journal(→ 恢复时决策不一致)。
2. **`recovery_end(now, stop_tsc, hz_prev, hz_cur)`**(恢复驱动在尾部重放之后、写 `purpose = Recovery` 的快照之前调用;其结果进入该快照,所以新段的验证回放从平移后的状态开始;`now` 是本次运行的 `TscClock` 读数,同一值写入该快照头):
   - 全部非终态槽置 `Unknown`(保存 `prev_state`)并登记查询;链接腿的 NotFound 规则照常;冒烟单槽另置 `SMOKE_DUE`(冒烟单不跨运行保留,见"冒烟单"第 5 条)。
   - 每个 `Open`/`Hedging` 链接(**先于下一条的平移**,`first_fill_tsc` 与 `stop_tsc` 都还是上次运行的 TSC):尚无 `RESUME_HOLD` 时 `held_ms` = 已用的引擎运行时间——`first_fill_tsc = 0`(第一腿尚无成交)时为 0,否则 `min((stop_tsc − first_fill_tsc) 按 hz_prev 折成 ms, hedge_deadline_ms)`——并置 `RESUME_HOLD`;已有的保持原 `held_ms`(恢复期间再次崩溃不重复累计)。
   - **TSC 平移**:本 crate 进快照的 TSC 字段只有已登记的这几个(是 [crates/AGENTS](../AGENTS.md)"快照内时刻用 engine_ns"规则的登记例外)——`OrderSlot.t_state`、`LinkEntry.first_fill_tsc`、`Tomb.freed_tsc`、`LegState.last_fill_tsc`,以及节 3 头的 `resume_hold_until`(本步重新设定,不平移)。前四个按 [hft-exec 快照与恢复](../hft-exec/README.md#快照与恢复) 的同一公式平移(按两次运行的 `tsc_hz_milli` 换算时长、过去的时刻至少为 1、0 保持 0;同开机、跨开机与换机同一规则,不按 `boot_id` 或 UTC 重建)。`SlotStamps` 不进快照,不涉及。
   - `resume_hold_until = now + resume_hold_max_ms`(按 `hz_cur` 折成 tick;每次 `recovery_end` 重新给满窗口);各 (venue, account) 的 `hedge_gate`、`conn_up` 与 `decode_fails` 清零(本次运行的连接尚未建立,查询在 `ConnState{Order, Up}` 时发出)。本调用不为置 `RESUME_HOLD` 的链接逐条产出 `LinkUpdate`:hft-exec 的 `recovery_end` 按同一规则自行标记(见 [hft-exec 快照与恢复](../hft-exec/README.md#快照与恢复))。
3. **`RESUME_HOLD` 期间**:不判 `hedge_deadline_ms`;`on_hedge_retry` 到期只忽略(重试改由续接触发);OnFill 照常(迟到的第一腿成交仍在同一事件产生第二腿,失败照常交 HedgeRetry 流程但不判截止)。
4. **续接**:续接单所在的 (account, venue)——`on_fail = Retry` 时为第二腿的,`Reverse` 时为第一腿的,`Hold` 时为第二腿的——收到 `on_hedge_gate_open(a, v)`(hft-risk 报告该 venue 的对冲闸门子集 `IDENTITY`、`ACCOUNT_CFG`、`ORDER_CONNS` 已满足;由 Input 驱动、回放确定;置该表的 `hedge_gate`),**且**该链接没有任何 Unknown 槽(有则等收敛,收敛的那个事件内续接)时:清 `RESUME_HOLD`;`first_fill_tsc ≠ 0` 时 `first_fill_tsc = now − held_ms`(停机与续接前的等待都不计入);按 `on_fail` 立即做一次续接:`Retry` → `SecondRetry{qty: min(owed, floor_to_lot(naked_dir))}`;`Reverse` → `Reverse`;`Hold` → 无。**只有确实发出续接单时才置 `RESUME_RETRY`**;算出的量为 0(被裸量上限截掉的部分从 `hedge_req` 核销)或 `Hold` 时不置,链接直接按 OnFill 第 6 条正常判断;`owed == 0` 且无在途 → 链接 Done。写 `Note{OmsResumeRetry}`(P1,`v = [link_id, held_ms, owed, qty]`)。整个账户的启动闸门是否放行不影响续接(完整放行蕴含各 venue 的子集都已满足)。清 `RESUME_HOLD` 时产出一条 `LinkUpdate`(`flags` 不含 `RESUME_HOLD`;输出满时经 `pending_out` 的 `Link` 项),hft-exec 据此清该 cell 的 `LINK_HOLD`。
5. **续接超时**:`on_sweep` 时 `now ≥ resume_hold_until` 而仍有 `RESUME_HOLD` 链接,逐条处理(按引擎运行时间计,判定粒度为 sweep 周期 1 s;输出多时经 `pending_out` 分批):
   - 有敞口的链接(`first_net ≠ 0`)→ 置 `LinkState::Broken`(`LinkBroken::ResumeTimeout`),写 `Note{OmsResumeTimeout}`(**P0**,`v = [link_id, account | venue << 8, held_ms, owed, 原因(GATE=1 子集未满足、UNKNOWN=2 有 Unknown 槽), first_net]`)与 `LinkUpdate`;sequencer 随之设 `Broken{account, group}` cap,未对冲量交 hft-exec 的 Broken 处置。
   - **零敞口的链接**(`first_net = 0`:第一腿尚无已知成交,常见于第一腿 Unknown 等成交历史确认)→ 不置 Broken、不设 cap:清 `RESUME_HOLD`(此后按正常规则:`first_fill_tsc = 0` 时截止本来不计,第一腿迟到成交到达时 OnFill 照常发第二腿、截止从该成交起算),写 `Note{OmsResumeTimeout}`(同上 `v`,实际级别 **P1**)与清标志的 `LinkUpdate`;链接继续等其 Unknown 收敛。
6. 续接请求到达终态(成交、执行失败或门控)后清 `RESUME_RETRY`,此后按 OnFill 第 6 条的计次与截止正常判断;它的执行失败照常计次。

堆内定时器的跨重启规则归 [hft-sequencer](../hft-sequencer/README.md#定时器);本 crate 的旧 `HedgeRetry` 定时器在恢复后早到或晚到都无害(上面第 3 条)。

### 输出扇出

**出环配额与优先级**:扇出队列按 (venue, account) 分队,每队上限 `out_quota`(= 一次交出(一个 Input 或一次 `on_continue`)中该队至多交出的出环决策数;各队每轮交出一条,轮转到 K 条或各队都到上限/取空为止);同一 (venue, account) 内 `RISK_REDUCING` 决策(第二腿、重试、Reverse、补腿、撤单)排在 Open 之前出环;不同账户轮转出环,一个账户的限速风暴不推迟另一个账户的对冲。gw 侧在每批内同样让 `RISK_REDUCING` 先发、按账户限每轮条数,限速计数按 (venue, account) 分开(见 [hft-venue GwRuntime](../hft-venue/README.md))。

**容量与续推**(固定容量从不丢弃输出):`OmsOut` 各向量的容量是**单次调用**的上限,出环类决策另受 `max_out_per_event`(K)约束。单槽入口(`place`、`cancel`、`amend`、`on_order_event`、`on_send_report`、`on_hedge_retry` 等)的产出天然在上限内;批量入口(`cancel_all`、`cancel_open`、`cancel_resting_on`、`reissue_cancels`、`on_conn_state`、`force_unknown`、`recovery_end`、`on_hedge_gate_open`、`on_sweep`)可能超出。规则:**状态迁移在本次调用内全部完成**(`unknown_count`、`venue_ready`、InflightBook、LegTracker、链接标志立即正确,风控不会看到中间态),**只有输出**按产生顺序延后——任一向量满时,其余工作项以 `OutItem` 进入 `pending_out: FifoRing<OutItem>`:

```rust
#[repr(C)]
pub struct OutItem {                 // 32 B(布局为草案,以代码与编译期断言为准)
    pub kind: u8,                    //  0 Decision=1(出环决策)| Notify=2(Transition Note + 给拥有者的 OrderUpdate)| Hedge=3(续接/重试的 HedgeRequest)
                                     //    | Link=4(LinkUpdate,交出时按链接当前状态填)| Timer=5(OmsTimerReq;action = 种类
                                     //      UnknownQuery 1 / HedgeRetry 2 / SmokeTtl 3,link_id = id,from = cancel,cause_seq = deadline_tsc)
                                     //    | Alarm=6(槽的 P0 OmsUnknown)| Aged=7(给拥有者的 OrderUpdate{Aged})
    pub action: u8,                  //  1 Decision:OrderAction;Notify:to 状态(OrderState 码);Hedge:LinkRole;
                                     //    Link:随行 Note(0 无、1 OmsHedgeBroken、2 OmsResumeTimeout P0、3 同 P1,from = 原因)
    pub from: u8, pub cause: u8,     //  2 / 3 Notify:from 状态与迁移原因(OrderState、TransitionCause 码,见"状态机")
    pub link_id: u32,                //  4 Hedge、Link;其余 0
    pub cause_seq: u64,              //  8 触发迁移/决策的事件 seq(Note 与 OrderDecision.trigger_seq 用原值,不用续发时的 seq)
    pub slot: SlotRef,               // 16 12 B;Hedge 为全零
    pub _pad: u32,                   // 28
}
const _: () = assert!(size_of::<OutItem>() == 32);
```

`pending_out` 由一个全局 FIFO(Notify、Hedge、Link、Timer、Alarm、Aged 项,容量 4 × 全部表槽数 + 2 × 全部账户链接数 + 64)与每 (venue, account) 两条决策队列(风险递减、Open)组成;Notify 项交出时另带进入 Unknown 的 `OmsUnknown`,UnknownQuery 等定时器登记同样经 Timer 项排队,保证同类输出按产生顺序交出。进快照(节 12 的 `Out` 行:先全局 FIFO,再按表键升序的各表风险递减队列、Open 队列,各自 FIFO);恢复时 Decision 项按槽的 (venue, account) 与是否风险递减回到对应队列。出环决策的槽置 `QUEUED`;已排队决策的槽不再排第二条 `Query`(队列长度以槽数为界,下一次 UnknownQuery 定时器再查)。`wants_continue()` 为真时 sequencer 的实盘驱动下一轮写 `Timer{OmsContinue}`,`on_continue` 按 FIFO 再交出至多 K 条决策及相应的其他输出;`Hedge` 项在交出时按当时的 `owed` 与 `naked_dir` 计算数量(与立即发出时同一规则,回放确定)。队列中的撤单在交出前若槽已终态则丢弃(那是已无意义的命令,不是丢输出)。

**撤单范围与批量撤单**(Kill、`Drain`、`CancelAll` 命令走 `cancel_all`,撤单持续义务走 `reissue_cancels`,两者同一规则;本段是可撤状态的唯一定义,[control-plane `CancelAll`](../../docs/subsystems/control-plane.md) 与 Kill 链接这里):"可撤的槽"= 状态 `Live`、`PendingNew` 或 `PendingAmend` 的槽(`PendingAmend` 的槽在交易所上仍挂着);`PendingCancel` 已在撤;`Unknown` 槽不发撤单,收敛为 `Live` 时由撤单持续义务补撤;已置 `CANCEL_SENT` 的不重发:

| 来源 | `open_only` | 撤哪些槽 | 用 venue 批量 `CancelAll` 的条件 |
|---|---|---|---|
| 非 HARD 的 `Kill{cancel_resting}`(含 Viewer 紧急停止) | true | 范围内可撤的非 `RISK_REDUCING` 槽 | venue 支持批量撤单(`VenueCaps.flags` 含 `BATCH_CANCEL`)**且**该 (venue, account) 此刻没有非终态的 `RISK_REDUCING` 槽;否则逐槽撤单,风险递减单(在途第二腿、挂着的减仓单)不受影响 |
| `Kill{hard}`、`Drain{cancel_resting}`、`CancelAll` 命令 | false | 范围内全部可撤的槽 | venue 支持批量撤单即用 |
| `cancel_resting_on(a, inst)`(执行层设 MarginStress/Hard 引擎目标前) | — | 该 (account, inst) 上可撤的、无 `EXEC` 位的 GTC/PostOnly 槽(含 `RISK_REDUCING` 的减仓挂单与冒烟单);不是持续义务 | 不用(逐槽撤单,只动这一个 inst) |
| 冒烟单到期(`on_smoke_ttl`) | — | 该冒烟单一张 | 不用 |

批量撤单时每 (venue, account) 只出一条 `CancelAll`,回报按各槽的 `CancelAck` 或查询收敛;不支持批量撤单的 venue 逐槽撤单。一个交易所账户只由本系统的一个策略实例交易,venue 级撤单不会误撤他人的单。

### 链上槽(optional)

feature `chain`,随链上腿的策略迁移启用(触发条件见 [hft-chain-evm](../hft-chain-evm/README.md))。外部 venue([external-venue-protocol](../../docs/subsystems/external-venue-protocol.md))在 OMS 里与 CEX venue 一样有 `VenueAcctTable`(account = 执行钱包)。`TxState` 映射:`Sent` → PendingNew;`IncludedOk` → Filled(**latest 证据触发对冲**;成交量一律是**基础资产(代币)数量**:Buy 取 `amount_out`,Sell 取 `amount_in`,计价资产数量只用于名义与 ledger);`IncludedReverted` → Rejected;`Dropped` → Cancelled;`Unknown` → Unknown(不反向,等 bundle 有效期使其有界);`Finalized` → 不改订单状态,只转发给 ledger 作为会计口径。链上槽**在 `Finalized`(或确定的 Dropped/Reverted)之前不回收**,因为 `Filled` 之后仍可能收到重组:`Filled` + `Unknown{reason=REORG}` → 槽回到 `Unknown`(`prev_state = Filled`,`filled` 保留),`unknown_count += 1`;随后重新 `IncludedOk` → `Filled`(`amount_out` 不同则产出正/负成交增量);`Dropped`/`IncludedReverted` → `Rejected`,产出等于原 `filled` 的负向增量(`Reversal`)交 ledger,LegTracker 随之出现 CEX 腿多出的敞口,由 hft-exec 补腿或 `HedgeFix` 命令处理(`Reorg` cap 期间抑制自动补腿与自动有界平仓)。完整口径与告警级别见 [hft-ledger 终局前重组](../hft-ledger/README.md#链上腿optionalfeature-chain)。`SwapDecision.min_out` 由下单方从**信号价**反推并随决策写进 journal,chain-core 只能在最新块上再校验、不能放宽([06 §2.4](../../design/06-链上执行层.md))。

### 每次迁移产生的输出

| 迁移 | 输出 |
|---|---|
| Free → PendingNew | `OrderDecision{New}`(→ `GwCmd{Place}`)、`OmsTransition`、InflightBook +=、LegTracker 即时在途 += |
| `SendReport` 已写出 | `SlotStamps.sent_tsc`(无 Note) |
| `SendReport` 确定未写出 / `WriteError` | 见"SendReport 处理" |
| → Live(Ack) | `OmsTransition`;`OrderUpdate{Acked}`;GTC/PostOnly 的剩余量从即时在途移到挂单在途 |
| 由 `PendingNew` 或 `Unknown` 经其他回报(`QueryResult`、`ReconcileReport`、成交)→ Live | 同上一行(`OrderUpdate{Acked}`;成交另按"成交增量"行) |
| 由 `PendingCancel` 或 `PendingAmend` → Live(`CancelReject`、`AmendAck`、`AmendReject`、`QueryResult`) | `OmsTransition`;无 `OrderUpdate`(拥有者以 `Views::order` 的当前状态为准) |
| 该槽待合成样本的操作收到首个交易所响应 | `LatencySample{action}`(见"延迟样本") |
| 成交增量 | `OmsTransition{d}`、`FillOut`、`OrderUpdate{Filled(d)}`、LegTracker 更新、若 LINKED 第一腿 → `HedgeRequest`(≤ 该方向裸量);链接状态变化 → `LinkUpdate` |
| `FeeOnly` | `FeeOut`(净量补差随 ledger 的 `NetAdjust` 经 `net_adjust` 回到 LegTracker 与 group 余数) |
| → Cancelled/Rejected | `OmsTransition`、InflightBook −=、LegTracker 在途 −=、`OrderUpdate`;若 LINKED 且为第二腿 → on_fail 处理 |
| → Unknown | `OmsTransition{cause}`、`unknown_count += 1`、`Note{OmsUnknown}`(P1;超时升 P0)、`OrderUpdate{Unknown}`;挂单在途移回即时在途 |
| Unknown → 任意 | `OmsTransition{converged_by}`、`unknown_count −= 1` |
| 终态 → Free | 写墓碑(回收计数进 `OmsStats`,无 Note) |
| 墓碑命中的迟到成交 | `FillOut{LATE}`、LegTracker 更新、`Note{OmsLateFill}`(P0) |
| `external_adjust`(ExternalFill 经 ledger 归属后) | LegTracker 更新;`cause = Unknown` 记入 `ext_held` |
| 槽成交命中 `ext_held` | `FillOut{EXT_DEDUP}`(只计未入账部分)、链接按全量推进、对冲受裸量上限、`Note{OmsExtDedup}`(P1);防御路径与 `StaleGen` 的 `ForeignFill` 同样只产出未入账部分 |
| 防御路径或 `StaleGen` 产出 `ForeignFill` | `ForeignFill` 一条、`Note{OmsForeignFill}` 一条,另按来源写 `OmsForeignFillViaOrderEvent`(P1)或 `OmsStaleGenFill`(P0) |
| 对冲门控(`hedge_gated`) | HedgeRetry 登记;`OmsHedgePxStale` / `OmsHedgePxGated`(P1) |
| 对冲闸门子集满足(`on_hedge_gate_open`) | 每个续接链接至多一条 `HedgeRequest{SecondRetry \| Reverse}`、`Note{OmsResumeRetry}`(P1) |
| 续接超时(`on_sweep`) | 每个仍 `RESUME_HOLD` 的有敞口链接:`LinkUpdate{Broken, ResumeTimeout}`、`Note{OmsResumeTimeout}`(P0);零敞口链接:清 `RESUME_HOLD` 的 `LinkUpdate`、`Note{OmsResumeTimeout}`(实际级别 P1) |
| `cancel_resting_on` | 每个被撤的槽 `OrderDecision{Cancel}`、`OmsTransition{CancelReq}` |
| 冒烟单建槽 / 到期 | 同 Free → PendingNew(槽带 `SMOKE`)并登记 `Timer{Oms, SmokeTtl}`;到期 `OrderDecision{Cancel}`、`OmsTransition{SmokeTtl}`;冒烟单成交另写 `Note{OmsAnomaly}`(P0) |
| `write_off_residual` | LegTracker 更新(多出那一腿的 `net`、全部腿的余数),返回单条 `WriteOff`;无 Note(审计由 ledger 的 `AdjustDelta::WriteOff` 与命令的 `CtlAudit` 承担) |

### Note 码表

`Note{source = Oms}` 的码名一律带 `Oms` 前缀;[hft-event](../hft-event/README.md) 的 Note 登记表只登记码名、source 与默认级别(下表的级别与之一致,以登记表为准;`Note.level` 是实际值),数值 `n`(`Note.code` 的打包归 hft-event)与 `v[]` 含义只在本 crate 的 `const` 码表里,由 xtask 汇总、[hft-telemetry](../hft-telemetry/README.md) 翻译。`strategy/group/inst` 填 Note 的专用字段。

| 码 | 级别 | 何时 |
|---|---|---|
| `OmsTransition` | Info | 每次槽迁移;`v[8] = [venue \| account << 8 \| from << 16 \| to << 24 \| cause << 32, slot, gen, cause_seq, filled, qty, px, exch_order_id]` |
| `OmsUnknown` | P1(超过 `unknown_alarm_ms` 升 P0) | 槽进入 `Unknown`:`v = [venue \| account << 8 \| cause << 16, slot, gen, prev_state]`;超过 `unknown_alarm_ms` 时同一槽再写一条 `level = P0` 的同码(见"Unknown 收敛流程"第 5 条) |
| `OmsUnknownStuck` | P0 | `query_count > unknown_max_queries` |
| `OmsVenueReady` | Info | `venue_ready` 由假变真(最后一个 Unknown 收敛且连接 Up,或连接 Up 时已无 Unknown):`v = [venue \| account << 8]` |
| `OmsNeedsRestQuery` | Info | 请 reconciler 用 REST 查询(`trades = 1` 时查成交历史):`v = [venue \| account << 8 \| trades << 16, slot, gen, epoch, exch_order_id]` |
| `OmsStaleGenFill` | P0 | 回报既不匹配当前槽也不匹配墓碑且带成交:`v = [venue \| account << 8, ev.slot, 是否已入账(0/1), ev.gen, ev.epoch, ev.exch_order_id, ev.cum_filled]` |
| `OmsLateFill` | P0 | 墓碑命中的迟到成交 |
| `OmsOverfill` | P0 | `cum_filled > qty` |
| `OmsForeignOrder` | P1 | 无成交的外来单回报 |
| `OmsForeignFillViaOrderEvent` | P1 | 带成交的外来单经 `OrderEvent` 到达(venue 契约偏差) |
| `OmsForeignFill` | Info | 每条 `ForeignFill` 一条,带足入账字段(冷读者的入账依据;`v[]` 见"回报匹配与成交增量"第 8 条) |
| `OmsExtDedup` | P1 | 槽成交、防御路径或 `StaleGen` 的成交命中 `ext_held` |
| `OmsFillPxEstimated` | P1 | 成交价按委托价估计 |
| `OmsFeeUnattributed` | P1 | `FeeOnly` 找不到槽与墓碑 |
| `OmsCancelNotSent` | P2 | 撤单/改单确定未写出 |
| `OmsHedgeBroken` | P0 | 链接 Broken(计次耗尽、截止、`HEDGE_LIMIT`、超量成交、反向失败) |
| `OmsHedgePxStale` | P1 | `price_hedge` 报价连接不健康 |
| `OmsHedgePxGated` | P1 | 对手价越过越价上限 |
| `OmsHedgeSlipCapped` | P1 | 超滑点按越价上限定价 |
| `OmsHedgeNakedCapped` | P1 | 第二腿被该方向裸量截量 |
| `OmsResumeRetry` | P1 | 恢复后的续接 |
| `OmsResumeTimeout` | P0(零敞口链接实际级别 P1) | `resume_hold_max_ms` 内未能续接:有敞口的链接 Broken,零敞口的只清 `RESUME_HOLD` |
| `OmsAnomaly` | P1(`SmokeFilled` 实际级别 P0) | 回报处理表中的"异常 Note"格与其他异常;`v[0]` = 种类:`Table=1`(回报处理表异常格)`ForeignEvict=2`(`foreign`/`ext_held` 表 FIFO 淘汰)`SmokeFilled=3`(冒烟单成交)`ReconIgnored=4`(`ReconcileReport` 的 `OpenOrder`/`UnknownResolved` 条目不匹配或槽已不是 Unknown/VERIFY,被忽略),其余 `v[]` 按种类 |

### 快照行格式(节 2、3、12;草案,实现时以代码与编译期断言为准)

`StateSnapshot` 的分节表与 `SectionHdr` 归 [journal-format](../../docs/subsystems/journal-format.md#快照记录结构);本 crate 定义节 2、3、12 的行。全部小端、按所列键升序,`SectionHdr.count` = 行数;读者按值拷贝(节体只保证 8 B 对齐)。

```rust
// 节 2 OrderSlots:每个 state ≠ Free 的槽一行(含 Unknown);键 (venue_ix, account_ix, slot_id)
// 行 = OrderSlot 的 128 字节 ‖ SlotExt 的 64 字节 = 192 B,按字节写出;读者把两段分别按值拷贝进对齐的 OrderSlot / SlotExt。
// 行版本 1 的 SlotExt 只有前 32 字节(无 fee_cum,行长 160 B):读者补零,并取 fee_cum = filled。
pub const SLOT_ROW_LEN: usize = 192;

// 节 3 OmsCounters:节首 1 行 OmsCountersHdr,其后每 (venue, account) 表 1 行 TableRow + 变长尾
#[repr(C)] pub struct OmsCountersHdr { pub engine_epoch: u32, pub n_tables: u32, pub resume_hold_until: Tsc } // 16 B;0 = 无续接窗口
#[repr(C)] pub struct TableRow {                                  // 32 B;键 (venue_ix, account_ix)
    pub venue: VenueId, pub account: AccountId, pub conn_up: u8, pub hedge_gate: u8,                 //  0
    pub n_slots: u32, pub free_len: u32, pub sweep_cursor: u32, pub unknown_count: u32,             //  4 / 8 / 12 / 16
    pub n_foreign: u32, pub n_ext_held: u32,                                                        // 20 / 24
    pub smoke_slot: u32,                                                                            // 28 非终态冒烟单的 slot_id;0 = 无
}   // 其后:free: [u32; free_len](FIFO 顺序,队头在前)、gens: [u32; n_slots](每槽当前代数,含 Free 槽)、
    //       foreign: [(u64, Qty); n_foreign]、ext_held: [(u64, Qty); n_ext_held](均为 FIFO 顺序,每项 16 B)、
    //       decode_fails: u8 与 7 B 零(连续 REPORT_DECODE 次数),尾部补零到 8 B

// 节 12 OmsLinks:每行以 LinkRowHdr 开头,顺序 Link* → Plan* → Leg* → Tomb* → Out*
#[repr(C)] pub struct LinkRowHdr { pub kind: u8 /* Link=1 Plan=2 Leg=3 Tomb=4 Out=5 */, pub _pad: [u8; 3], pub len: u32 } // 8 B
// Link:LinkEntry 原样(80 B;state ≠ Done 的项;键 link_id)
// Plan:{ link_id: u32, _pad: u32, tolerance: Rate, plan: HedgePlan }(64 B = 8 + 8 + HedgePlan 48 B;被活动 Link 引用的计划;键 link_id)
// Leg: { account: AccountId, _pad: [u8; 3], group: GroupId, state: LegState }(200 B = 8 + LegState 192 B;键 (account, group);全零项不写)
// Tomb:{ venue: VenueId, account: AccountId, _pad: u16, slot: u32, tomb: Tomb }(64 B = 8 + Tomb 56 B;未过期的非空墓碑;键 (venue, account, slot, gen))
// Out: OutItem 原样(32 B;pending_out 按 FIFO 顺序,队头在前;不排序——顺序本身是状态)
const _: () = assert!(size_of::<TableRow>() == 32 && size_of::<LinkRowHdr>() == 8 && size_of::<OmsCountersHdr>() == 16);
```

- `SlotStamps` 不进快照:它含测量值(`decided_tsc` 等),写入会破坏 `state_hash` 的确定性;恢复后在途槽的首个响应不生成 `LatencySample`。
- `InflightBook` 与 LegTracker 各腿的两类在途不单独恢复:它们由非终态槽的剩余量与状态决定(不变量),`restore` 时由节 2 重算并与快照中的 `LegState` 校验。
- `(account, inst) → (group, leg_ix)` 归属表不进快照,由恢复时的 GroupSpec 重建。
- 进快照的 TSC 字段只有"恢复"第 2 步列出的已登记字段;其余时刻不进快照。
- **行版本**(只在本 crate 登记,写进 `SectionHdr.version`):节 2 = 2、节 3 = 1、节 12 = 1。行格式只追加、零值表示未提供;版本递增时读者按旧版本的行长读出并补零,未知的更高版本拒读(`RestoreError`)。

### 规划源文件

| 文件 | 职责 |
|---|---|
| `src/lib.rs` | `Oms` 入口与公开 API、`OmsOut` |
| `src/types.rs` | `OrderSlot`、`SlotExt`、`SlotStamps`、`Tomb`、`OrderState`、`TransitionCause`、`SlotFlags`(私有状态位;线上 `OrderFlags` 用 hft-event 的)、`LinkEntry`、`LinkBroken`、`OutItem`、`OmsTimerReq`、`HedgeRequest`、`HedgePx`、`HedgeGate`、`FillOut`、`FeeOut`、`LinkUpdate`、`ForeignFill`、`LegState`、`SmokeReq`、`SmokeErr`、`WriteOff`、`TrigStamps`、错误枚举(`LegLink`/`HedgePlan`/`LinkRole`/`OrderRef`/`LegView` 用 SDK 类型;`HedgePxLimits` 与 `cap_cross_px` 用 hft-types 的) |
| `src/table.rs` | `VenueAcctTable`、FIFO 空闲表、槽分配/回收、墓碑 |
| `src/fsm.rs` | 回报处理表(一个 `match (state, kind)`,每格一个分支)、回报匹配 |
| `src/net.rs` | 成交净量、`net_adjust` |
| `src/unknown.rs` | 进入/查询/收敛/升级(含 NotFound 的两类处理:成交历史确认与 `ExpireAt` 过期定终态) |
| `src/out.rs` | `pending_out`(`OutItem` 四种)、`on_continue`、批量入口的输出延后、撤单范围(`cancel_all`、`reissue_cancels`、`cancel_resting_on`) |
| `src/link.rs` | `LinkTable`、OnFill/Simultaneous、重试、`hedge_gated`、按 group 的余数、裸量上限 |
| `src/price.rs` | `price_hedge`(连接健康判定调用 SDK `BboView`,限价调用 hft-types `cap_cross_px`) |
| `src/send.rs` | `SendReport` 处理、本地发送失败的计次分类、`REPORT_DECODE` |
| `src/latency.rs` | 延迟样本:`SlotStamps` 记戳与按操作合成 `LatencySample` |
| `src/legs.rs` | `LegTracker`(内嵌 SDK `LegView`;即时/挂单两类在途)、`naked_dir`、(account, inst) → (group, 腿) 表、`write_off_residual` |
| `src/smoke.rs` | 冒烟单:`smoke_intent`、建槽、`on_smoke_ttl`、`SMOKE_DUE` 补撤 |
| `src/external.rs` | `external_adjust`、`ext_held` 去重、外来单防御路径与 StaleGen 入账 |
| `src/inflight.rs` | `InflightBook`(含只计即时在途的 `imm_*`、`buy_notional`、`open_margin`) |
| `src/sweep.rs` | StaleOrderSweep、续接超时 |
| `src/recover.rs` | `recovery_end`(Unknown、TSC 平移、RESUME_HOLD、`resume_hold_until`)、`on_hedge_gate_open`、`mirror_restore` |
| `src/snapshot.rs` | 快照/恢复(节 2、3、12 的行) |
| `tests/*.rs`、`benches/oms.rs` | 用例与基准;位置与函数名见 [TESTS.md](TESTS.md) 各用例的 `实现` 与 `基准名` 字段 |

</details>

## 延迟与资源契约

分段:`decided_tsc − proc_tsc`(簿 + 策略 + 执行层 + 风控 + OMS,基准形态 1–5 µs,[00 §4](../../design/00-目标与延迟预算.md));OMS 占其中以下份额。测量:Criterion 基准(热缓存,单核)+ 实盘 `LatencySample` 分段直方图。

| 入口 | p50 | p99 | 基准名 |
|---|---|---|---|
| `place`(分配 + id 编码 + 决策填充) | ≤ 150 ns | ≤ 400 ns | `oms/place` |
| `on_order_event` Ack | ≤ 100 ns | ≤ 300 ns | `oms/on_event_ack` |
| `on_order_event` 成交(不含第二腿) | ≤ 100 ns | ≤ 300 ns | `oms/on_event_fill` |
| 成交 → 第二腿 `HedgeRequest`(含净量、对齐与裸量上限) | ≤ 150 ns | ≤ 400 ns | `oms/leglink_onfill` |
| `price_hedge`(含越价上限与超滑点改价) | ≤ 15 ns | ≤ 40 ns | `oms/price_hedge` |
| `hedge_gated` / `hedge_vetoed` | ≤ 30 ns | ≤ 80 ns | `oms/hedge_gated` |
| `on_unknown_query`(一个槽:再查询并重新登记) | ≤ 80 ns | ≤ 200 ns | `oms/unknown_query` |
| 墓碑命中的迟到成交 | ≤ 120 ns | ≤ 350 ns | `oms/tomb_late_fill` |
| `external_adjust` / `net_adjust` | ≤ 30 ns | ≤ 80 ns | `oms/leg_adjust` |
| `on_sweep`(256 槽) | ≤ 3 µs | ≤ 8 µs | `oms/sweep_256` |
| `on_hedge_gate_open`(一个 (account, venue) 64 个续接链接;不在热路径) | ≤ 3 µs | ≤ 10 µs | `oms/hedge_gate_open_64` |
| `on_continue`(交出 K = 16 项 `OutItem`) | ≤ 2 µs | ≤ 5 µs | `oms/continue_16` |
| `cancel_resting_on`(该 (account, inst) 8 个非终态槽) | ≤ 300 ns | ≤ 1 µs | `oms/cancel_resting_8` |
| `smoke_intent` + 冒烟单 `place` / `on_smoke_ttl`(命令路径,不在热路径) | ≤ 300 ns | ≤ 1 µs | `oms/smoke` |
| `write_off_residual`(命令路径) | ≤ 50 ns | ≤ 150 ns | `oms/write_off` |
| `recovery_end`(一张 4096 槽表 + 4096 链接;恢复路径) | — | ≤ 300 µs | `oms/recovery_end_4096` |

- `ClientOrderId` 编码与解码在 hft-types,预算与基准(`types/clid_encode`、`types/clid_decode`)归那里。
- 第一腿成交回报取出 → 第二腿 `OrderDecision` 入出环:主机内 ≤ 20 µs([05 §1](../../design/05-CEX执行层.md)),OMS 份额(OnFill + `price_hedge` + `place`)≤ 1 µs;风控检查链(含 #6 `HEDGE_LIMIT`)另计。
- 风控可负担性读数(`avail_quote`、`avail_margin`)读 InflightBook 的增量和,O(1),与在途单数无关。
- 分配:只在 `new`/`restore` 与命令路径 `on_groups_changed`(容量预分配,只重写内容);运行期零分配。内存:每 (venue, account) 表 = N × (128 + 64 + 80 + 56 × T + 4) B,N = 4096、T = 2 时 ≈ 1.52 MiB(另加 `foreign`/`ext_held` 各 64 × 16 B);`LinkTable` 每账户 = `links_per_account` × (80 + 56) B(链接 + 计划表项),4096 时 ≈ 544 KiB;LegTracker 每 group ≈ 240 B(`LegState` 192 B + 归属表项);`pending_out` = 32 B × (2 × 全部表槽数 + 全部账户链接数),每张 4096 槽表约摊 256 KiB + 128 KiB(链接)。
- 无系统调用、无锁、无共享缓存行(只被 sequencer 线程触碰)。

## 测试与验收

用例与验收见 [TESTS.md](TESTS.md)。

## 依赖

- 内部:`hft-types`(`Px`/`Qty`/ID/`GROUP_NONE`/`ClientOrderId` 编解码、`RejectBits`、取整、`HedgePxLimits` 与 `cap_cross_px`)、`hft-refdata`(lot/min_qty/min_notional、venue 过期窗口、`VenueCaps`)、`hft-event`(`OrderEvent`、`OrderDecision`、`SendReport`、`ExternalFill`、`ReconcileReport`、`Note`、`LatencySample`;optional `SwapDecision`、`TxLifecycle`)、`hft-strategy-sdk`(仅类型:`OrderIntent`、`IntentClass`、`LegLink`、`HedgePlan`、`LinkRole`、`HedgeKind`、`PxMode`、`OnFail`、`ResidualPolicy`、`OrderRef`、`OrderUpdate`、`OrderView`、`LegView`、`GroupTable`、`BboView`;本 crate 不定义这些类型的副本)。
- 外部:无(`FixedVec`/`FifoRing`/`FifoMap` 来自 `hft-types` 的无分配容器)。
- 禁止:`hft-ledger`、`hft-risk`、`hft-exec`(L1 互不依赖;与 hft-exec 的交换由 sequencer 用 SDK 类型与各自的输出结构投影完成;风控参数经 `HedgePxLimits` 传入)、`hft-sequencer`、`hft-strat-lib`、任何 `strategies/*`、`hft-venue`、`venue-*`、任何 I/O 或 `std::time`。

## 延伸阅读

- [05 §2 订单模型、§5 定时器、§8 失败模式](../../design/05-CEX执行层.md)
- [06 §2.7 tx 生命周期、§2.9 外部 venue 接口](../../design/06-链上执行层.md)(optional 链上腿)
- [13 R4 按资产定点](../../design/13-设计评审与优化.md)、[12 §1 七起事故](../../design/12-风险与未决问题.md)
- [research/00 §2.3 双腿回滚](../../design/research/00-existing-codebase-inventory.md)
- [第八轮裁决(含 R8-8、R8-11)](../../design/_work/round8-decisions.md);Agent Note [smoke-order-command](../../.agents/notes/proposed/process/2026-10-03-smoke-order-command.md)
- [hft-exec](../hft-exec/README.md)、[hft-sequencer](../hft-sequencer/README.md)、[event-model](../../docs/subsystems/event-model.md)、[defensive-patterns](../../docs/defensive-patterns.md)

## 已知限制与后续工作

- 批量下单不在本规格:每个意图一个槽一个帧。可选扩展:`place_batch`(触发条件:启用需要批量的策略或 venue,且逐帧发送的限速或延迟成为瓶颈)。
- `cancelReplace`/`amend.keepPriority` 语义按 `amend()` 处理。接入原子撤改的 venue 时(触发条件:策略需要该 venue 的改单路径)为"撤成功新单失败"的半成功情形补单独的状态迁移格。
- 交易所 STP(自成交保护)触发的撤单按 `CancelAck` 处理,原因码只记录不区分。
- `ratio` 为静态整数比;按实时资金费/基差动态调整对冲比不支持。
- 费用晚到(`FeeOnly`)的 venue 上,第二腿先按毛量对冲,费用到达后多出的对冲量(≤ 费用 × ratio)以负余数留在 group,由下一笔链接或 hft-exec 的残余规则吸收;不做费用预估。
- 超滑点时按越价上限定价意味着对冲腿可能以比开仓时差 `rr_max_cross` 量级的价格成交;这是"宁可付滑点也不留裸腿"的取舍,代价进 ledger 的执行成本,由策略按报告调整 `max_slip` 与开仓门槛。
- 停机期间交易所侧仍可能成交(第二腿 IOC 已发出),这部分由恢复后的查询与对账收敛;本 crate 只保证不因停机时长误判 Broken。
- `RecvWindow` 与无过期保护的 venue 上,查询"不存在"的订单要等 reconciler 的成交历史确认才收敛:期间该 (venue, account) `venue_ready = false`,持续超过 `unknown_alarm_ms` 时 `UnknownTimeout` cap 挡 Open(对冲照常)。代价是新开仓延后数秒,换来不会把仍可能成交的单误判为失败。
- 批量入口的输出(逐槽 Note、`OrderUpdate`)经 `pending_out` 分批交出,大批迁移后的几轮输入里拥有者看到的 `OrderUpdate` 晚于状态本身;策略与执行层以 `Views::order` 的当前状态为准。
- 对冲腿 NotFound 的成交历史确认通常长于 `hedge_deadline_ms`(3 s):这类链接先按 `Deadline` Broken、账户设 `HedgeFailure`,确认后靠 Broken 的条件型解除与人工 `Resume{HedgeFailure}` 恢复开仓。这是"未证明已对冲就按裸露处理"的取舍。
- `StaleGen` 带成交按外来成交入账:墓碑已过期的一代若其成交早已按槽入账、又收到重复回报,会重复入账一次,由 P0 与对账采纳校正。
- `RESUME_HOLD` 超时按 `resume_hold_max_ms` 一刀切:交易所侧长时间不可用时,恢复后有敞口的在途链接全部按 Broken 交 hft-exec 处置,而不是无限等待(零敞口的只清标志、继续等 Unknown 收敛)。
- 冒烟单是每 (account, venue) 至多一张的远离市价 PostOnly,只验证下单、撤单、签名与死手刷新路径,不验证成交与对冲;交易所价格保护带若不接受 `smoke_far_bp` 的距离,冒烟单会被交易所拒绝(见"冒烟单"第 1 条)。
- `cancel_resting_on` 撤的是该 inst 上本账户全部非执行层挂单,包括策略为平仓挂的 reduce-only 单与冒烟单:L3 与清仓期间策略的挂单平仓被引擎减仓取代,这是"风控边界归引擎"的取舍。

### 开发备忘

<details><summary>给实现者</summary>

- **待核实**(方法:各所官方文档 + mock 回环 + 测试网/模拟盘小单):① Bitget `clientOid`、KuCoin `clientOid` 长度与字符集;Hyperliquid `cloid` 必须 16 字节十六进制;② 各所对重复 client id 的去重语义(Binance 仅在"未终结订单"中唯一——所以本规格从不用同 id 重发 New);③ 各所成交回报是否带累计成交量、累计均价与本笔手续费(Binance `z`、OKX `accFillSz`、Bybit `cumExecQty` 已知有;Bitget 的字段待查文档),以及手续费是否可能晚到(决定是否走预估);④ 各所 WS 按 client id 查询订单的接口是否存在(不存在的走 reconciler REST);⑤ `NotFound` grace:取 `recvWindow`/`expTime` 的实际配置值;⑥ 被判 NotFound 的订单之后还能否成交:Binance(`RecvWindow`)已按"能"处理(全部槽走成交历史确认),核实其"执行状态未知"类返回码与查询最终一致的时间;OKX `expTime` 是在网关还是撮合侧判定(决定 `ExpireAt` 的非链接槽能否继续按 grace 定终态;若只在网关判定,应把 `ExpireAt` 也并入成交历史确认);⑦ 各所强平/ADL 成交在用户流里的标记(`own_liquidation_marker`,由 venue crate 写明);⑧ 各所 maker 成交不带 client id 时,Ack 与成交的到达间隔分布(决定 `gw.unmatched_hold_ms` 5 000 是否足够);⑨ Binance 现货与 USDⓈ-M 的成交历史接口是否带 client id(按"不带"处理:`history_checked` 由 reconciler 按 orderId 归属判定,见"Unknown 收敛流程");⑩ reconciler 对 `OmsNeedsRestQuery{trades = 1}` 按需查询的实际时延(决定对冲腿 NotFound 是否常态地晚于 `hedge_deadline_ms`)。
- `OrderDecision.decided_tsc` 是测量字段,验证回放比较时屏蔽(见 [hft-sequencer](../hft-sequencer/README.md#确定性规则))。
- **开放问题**:`resume_hold_max_ms` 的默认 30 000 是否覆盖实际冷启动中对冲闸门子集(UID 核验、账户配置查询、下单与用户流连接)的完成时间,在 dev2 测试网窗口按实测标定(待标定)。
- 实现顺序:table + fsm + 属性测试 → `SendReport` 处理 → Unknown(含对冲腿 NotFound)→ 墓碑 → 成交净量 → LinkTable OnFill/重试/裸量上限 → LegTracker(两类在途)/Inflight → 外来成交 → Simultaneous → `price_hedge` → 快照 → `recovery_end`/`on_hedge_gate_open`/续接超时 → 链上槽(optional)。
- sweep 游标要跨次推进,不能每次从 0 开始(否则大表时后部槽饿死)。
</details>
