---
description: "sequencer 在每个意图进入 OMS 之前调用它做分类与按廉价→昂贵排序的检查链;它的 mode 模块是期望模式、cap 集合(含 MarginStress 档位)、有效模式、启动闸门、ctl_state_version 与 kill 投影的唯一归属"
kind: crate
tier: hot
scope: core
status: spec
---

# hft-risk

## 概要

`hft-risk` 拥有**热路径风控**与**模式状态**。热路径部分:意图分类 `classify()`(纯函数,决定 `IntentClass`、`RISK_REDUCING`、交易所侧 `reduceOnly`,全系统唯一来源)、对全部类别生效的检查链、版本化的 `RiskParams`、带风险递减预留的令牌桶与退避、当日亏损、敞口门、按到强平距离的保证金分档(`MarginStress`)、永续实际杠杆上限、清仓锁、对账分级(全系统只在这里分级一次)。**`hft-risk::mode` 模块是期望模式表(`ModeTable`)、cap 记录集合、有效模式、启动闸门、`ctl_state_version` 与 kill 投影的唯一归属**;[hft-sequencer](../hft-sequencer/README.md) 只调用 `Mode::apply()` 并按其输出分发 Mode/Risk 事件、撤单、写 kill 字、写闸门与模式注记、发起 Drain。七层优先级表的语义归属是 [control-plane](../../docs/subsystems/control-plane.md#模式与上限的统一优先级),本模块实现它。它**不拥有**仓位、现金、库存与保证金数据([hft-ledger](../hft-ledger/README.md))、订单与在途量([hft-oms](../hft-oms/README.md))、执行算法与默认减仓的做法([hft-exec](../hft-exec/README.md)),这些经 sequencer 组装的只读视图进入;它不碰任何原子量或环。引擎与策略的风控边界:引擎负责全账户硬限额、kill、保证金率、裸腿、限速、对账、交易所能力约束;策略自己的进出场与止损不在这里。预算:每意图 200–500 ns([05 §3](../../design/05-CEX执行层.md))。

## 目录

- [使用本组件](#使用本组件)
- [实现说明](#实现说明)
- [延迟与资源契约](#延迟与资源契约)
- [测试与验收](#测试与验收)
- [依赖](#依赖)
- [延伸阅读](#延伸阅读)
- [已知限制与后续工作](#已知限制与后续工作)

## 使用本组件

### 热路径:分类与检查

`evaluate_pair(first, second, view, now)` 在同一只读状态上检查两腿,同账户的 group/account 名义含另一腿待提交名义;两腿必须不同 venue/inst,Reduce/Flatten 方向须与实际持仓相反。全部通过才由 sequencer 分别 commit,任何拒绝不扣令牌。

```rust
pub struct Risk { pub mode: mode::Mode, /* 参数、桶、日亏、保证金档位、杠杆表、交易中的永续 inst、对账计数,见实现说明 */ }

/// 每账户一行,由 sequencer 从 hft-exec `ExecConfig` 该账户行投影(本 crate 不依赖 hft-exec)。
/// 执行配置与风控参数的一致性校验(见"引擎单与执行配置的一致性")与 Broken 解除的尘埃线读它。
pub struct ExecBounds {
    pub force_max_cross: Rate,          // ExecConfig.force_slice.max_cross
    pub force_max_chunk: Notional,      // ExecConfig.force_slice.max_chunk_notional
    pub dust_notional: Notional,        // ExecConfig.dust_notional(尘埃线,唯一定义在 hft-exec)
}

impl Risk {
    /// caps:EngineConfig 的容量段;本 crate 的各向量在这里按 refdata 的账户数、(account, venue) 数与 caps 一次分配(见"配置")。
    /// exec:按 AccountId 下标的 ExecBounds;与 p 不一致 → Err(ExecRiskMismatch)(hft-core 装配时退出 2)。
    /// mc:EngineConfig 段 5 的两个开关(字段与默认值归 hft-sequencer"环与容量"),本 crate 只读。
    pub fn new(p: &RiskParams, rd: &RefData, gws: &GwMap /* gw 序号 → (venue, 账户稠密序号表 venue_account_ix) */, caps: &Capacities,
               exec: &[ExecBounds], mc: ModeCfg) -> Result<Risk, RiskParamError>;

    /// 名义的唯一实现(检查链 #6 的口径):qty × px 以计价资产计,× quote_rate 折 USDT,i128 中间积,向上取整到 1e-6;sequencer 投影 IntentView.notional 时调用。
    pub fn notional(qty: Qty, px: Px, quote_rate: Rate) -> Notional;
    /// 纯函数:sequencer 在 evaluate 之前调用,OMS 只把结果拷进 `OrderDecision.oflags`。
    pub fn classify(it: &IntentDraft, origin: Origin, pos: &impl PosView, caps: &VenueCaps) -> Classification;

    /// 每意图一次,只读。now = SDK `Now`:连接活性只用 `now.tsc`(同一次启动内比较),令牌桶与退避用 `now.engine_ns`。
    /// Ok 带本意图需要扣减的令牌与在途增量;Err 带第一个失败的位(短路)。
    pub fn evaluate<V: RiskView>(&self, it: &IntentView, v: &V, now: &Now) -> Result<Admission, RejectBits>;
    /// 意图真的要出环(非 Shadow、模式门放行或不经模式门)时调用:扣令牌、计本地在途。Shadow 与被拒意图从不调用。
    pub fn commit(&mut self, a: &Admission, now: &Now);

    /// 控制命令 PREVIEW 的前置检查;合并 risk 自有状态与外部 ModeView,返回与 apply 相同的 ModeErr,不改状态。
    pub fn check_mode(&self, inp: &mode::ModeInput<'_>, ext: &impl mode::ModeView) -> Result<(), mode::ModeErr>;

    // ── 非模式状态更新(sequencer 在对应 Input 上调用;产生的 cap 请求经 mode.apply 生效)──
    pub fn on_pnl(&mut self, acct: AccountId, day_pnl: Notional, eq: EquityView) -> Option<mode::ModeInput<'static>>; // 越线 → DailyLoss
    /// `ModeOutcome.daily_loss_resumed` 列出的账户:sequencer 在同一 Input 内调用(day_pnl = ledger 当前值),见"当日亏损"的解除。
    pub fn on_daily_loss_resumed(&mut self, acct: AccountId, day_pnl: Notional);
    /// 每次 LegTracker 变化后调用;任何路径对该 group 设 Broken 之后,sequencer 在同一 Input 内再调一次。
    /// 记下该 group 最近一份 LegSummary(检查链 #9 与 naked_cleared 用;派生缓存,不进快照:从快照起步时 sequencer
    /// 对每个有腿的 group 调一次以重建,返回值忽略);该 group 有 Broken cap、s.quiescent 且
    /// s.naked_filled_notional ≤ 该账户 dust_notional → 返回 AutoClear{Broken, (acct, g)}。
    pub fn on_legs(&mut self, acct: AccountId, g: GroupId, s: &LegSummary) -> Option<mode::ModeInput<'static>>;
    // ── ModeView 的 Risk 侧读数(sequencer 的适配器转交 mode.apply;只读,O(1))──
    pub fn naked_cleared(&self, a: AccountId, g: GroupId) -> bool;          // 最近一份 LegSummary:quiescent 且 naked_filled_notional ≤ dust_notional;无记录 → false
    pub fn recon_resumable(&self, a: AccountId, v: VenueId) -> bool;         // 对账恢复条件("对账分级"第 4 条)已满足
    pub fn daily_loss_resumable(&self, a: AccountId) -> bool;                // 当日已解除次数 < daily_loss_max_resumes
    /// SetParams 应用后(命令路径):重建该账户每个永续 venue 的"交易中的永续 inst"(任一 group 的腿引用的永续 inst),
    /// 随即按当前杠杆表重判 lev_bad(见"永续杠杆上限");sequencer 先调它,再 apply(GroupsChanged)(后者按 ModeView::lev_bad 重判闸门与 cap)。
    pub fn on_groups_changed(&mut self, acct: AccountId, groups: &GroupTable);
    pub fn lev_bad(&self, a: AccountId, v: VenueId) -> bool;                  // 见"永续杠杆上限"第 2 条;ModeView::lev_bad 读它
    /// MarginReport 到达、或 mark 变化使该 (account, venue) 的强平距离或保证金估计变化时调用;O(1)。
    pub fn on_margin(&mut self, acct: AccountId, v: VenueId, s: &MarginSample, now: &Now, out: &mut MarginOut);
    /// 本 crate 登记的定时器到期(Timer{Risk, …}):MarginStale / MarginDeadline / MarginForce。
    pub fn on_timer(&mut self, t: RiskTimer, now: &Now, out: &mut MarginOut);
    /// AccountConfigReport 到达:按其 `LeverageItem` 更新该 (account, venue) 每个永续 inst 的实际杠杆(检查链 #7b、永续杠杆上限);
    /// 非 PARTIAL 报告整表替换(未列出 = 未知),PARTIAL 只改列出的 inst;`leverage = 0` = 未知。闸门与 cap 另经 mode.apply(Gate(AccountCfg))。
    pub fn on_account_cfg(&mut self, hdr: &AccountConfigReportHdr, lev: &[LeverageItem]);
    /// 交易所账户上该永续 inst 的实际杠杆(Rate 倍数,取自 AccountConfigReport;refdata `InstMeta.max_leverage` 不参与);未知 → None。O(1)。
    pub fn leverage(&self, a: AccountId, v: VenueId, i: InstId) -> Option<Rate>;
    /// 本次运行起点 = 当前 `EngineClock.base_engine_ns`:sequencer 处理 EngineClock 与从快照起步时调用;早于它的保证金报告与标记价一律视为过期。
    pub fn set_run_start(&mut self, run_start_ens: i64);
    /// 唯一的分级点(余额条目的资金费修正已由 reconciler 做过,这里不重复);返回需设/可清的 Recon cap 与闸门输入;Funding 条目本身不分级(交 ledger)。
    pub fn on_reconcile(&mut self, grp: &ReconcileGroup) -> ReconVerdicts;
    // ReconVerdicts { recon: Option<mode::ModeInput<'static>> /* Auto(Recon{a, v}) */, gate: Option<ReconGateInput> /* None = 组内无 Summary("对账分级"第 8 条),sequencer 不调 apply(Gate(Recon)) */, n_warn: u16 /* 本组 Warn 条数 */ }
    /// AUTO_EXTERNAL 的用户流前置条件(sequencer 在同组 on_reconcile 之前复核时调用,只读;规则见"对账分级"第 7 条)。
    pub fn auto_external_ok(&self, a: AccountId, v: VenueId, summary_sflags: u16) -> bool;
    pub fn on_user_stream_down(&mut self, a: AccountId, v: VenueId);
    /// 返回 Note{RateLimitHit}(P2;v[0] = venue | account << 8,v[1] = 本次 backoff_k,v[2] = backoff_until_ens),sequencer 追加。
    pub fn on_rate_limit_hit(&mut self, v: VenueId, a: AccountId, now: &Now) -> Note;
    pub fn on_day_roll(&mut self, day: u32, eqs: &impl EquitySource);     // 只在 day > last_day_rolled 时被调用(last_day_rolled 归 hft-sequencer)
    /// sequencer 调 `oms::price_hedge` 之前读取(O(1)):连接活性阈值与该 inst 价格带类别的越价上限,与检查链 #5 同值同源。
    pub fn hedge_px_limits(&self, a: AccountId, v: VenueId, i: InstId) -> HedgePxLimits;
    /// SetRisk(带账户作用域)。与 ExecBounds 不一致 → Err(ExecRiskMismatch)(sequencer 回 InvalidArgs{ExecRiskMismatch});
    /// 行数超出启动时的预分配 → Err(Capacity)(sequencer 回 CapacityExceeded)。
    pub fn set_params(&mut self, p: &RiskParams, scope: AccountScope) -> Result<(), RiskParamError>;
    // RiskParamError { Invalid(字段), ExecRiskMismatch, Capacity }:Invalid = 违反"保证金分档"或"当日亏损"的参数校验(new 时 hft-core 退出 2;
    //   SetRisk 时 sequencer 回 InvalidArgs{ParamInvalid}、旧参数生效)。
    pub fn loosens(&self, p: &RiskParams, scope: AccountScope) -> bool;    // sequencer 判定 SetRisk 是否需要确认
    /// SDK `Views::risk`(账户级):生效 cap、HARD、保证金档位与 liq_dist、截止 `deadline_tsc = now.tsc_of(deadline_ens)`、清仓锁、
    /// max_order_notional、max_hedge_notional、max_reduce_notional、conn_liveness_ms。
    pub fn view(&self, a: AccountId, now: &Now) -> RiskStateView;
    /// SDK `Views::risk_for`(按 inst;O(1)):该 inst 所属价格带类别的 max_px_dev、ioc_max_cross、rr_max_cross,与 RiskParams.l3_max_cross。
    /// 越价上限按 inst 取值,不取账户内最小值;执行层与策略据此截取,与检查链 #5 同值同源。
    pub fn view_for(&self, a: AccountId, i: InstId, now: &Now) -> InstRiskView;   // 填价格带、l3_max_cross 与 rate_backoff(令牌桶退避);quote_rate 由 sequencer 适配器从 ledger 换算率填入
    pub fn backoff_until(&self, v: VenueId, a: AccountId) -> i64;   // 该 (venue, account) 令牌桶的退避截止(engine_ns);不在退避为 0。sequencer 据此给 hft-oms hedge_gated 的 retry_at
    /// 一次写完本 crate 的全部节:节 4 RiskState、14 ModeTable、17 RiskCounters、18 Gates、23 MarginState(按此顺序;各组件的写出顺序归 hft-sequencer 快照)。
    pub fn snapshot(&self, w: &mut SnapshotWriter);
    pub fn restore(r: &mut SnapshotReader, p: &RiskParams, rd: &RefData, gws: &GwMap, caps: &Capacities, exec: &[ExecBounds], mc: ModeCfg) -> Result<Risk, RestoreError>;
    // restore:RiskRowHdr.kind 不在 {1, 2, 3, 6, 7, 8}、码值越界或行数超出容量 → Err(RestoreError)。
}

pub struct ModeCfg { pub auto_resume_after_clean_drain: bool, pub allow_missing_standby: bool }

/// sequencer 从 OMS `LegView`(LegTracker)投影;O(腿数)。
pub struct LegSummary {
    pub naked_ratio: Rate,                  // LegView.naked_ratio(含在途;敞口门 #9)
    pub naked_notional: Notional,           // LegView.naked_notional(含在途;敞口门 #9)
    pub naked_filled_notional: Notional,    // 只算已成交:|Σ leg[i].net| × mark;口径与取价同 hft-exec 的尘埃判定
    pub quiescent: bool,                    // 该 group 无 Pending*/Unknown 槽、各腿 inflight_immediate = 0(LegView.has_pending、has_unknown、inflight_immediate;挂单在途不算)
}
// 时间:本 crate 的持久状态(令牌桶、退避、保证金报告与标记价时刻、L2 截止、L3 下一步)一律存引擎时间 engine_ns(i64,字段名以 _ens 结尾;
// 算法唯一定义在 event-model 时间戳语义,sequencer 经 SDK `Now.engine_ns` 给出),恢复后原样有效、不平移;
// TSC 只在单次启动内使用(连接活性、对外的 *_tsc 视图字段与定时器登记,换算用 `Now::tsc_of`),本 crate 的快照不含 TSC。

/// 分类结果(唯一定义;OMS、gw、执行层、链上意图都只读这三个值)。
pub struct Classification { pub class: IntentClass /* Open | Reduce | Hedge | Flatten */, pub risk_reducing: bool, pub reduce_only: bool }

#[repr(u8)] pub enum Origin { Strategy = 1, Exec = 2, Oms = 3, Command = 4 }
// Strategy = 策略 Ctx 的订单意图;Exec = hft-exec 产生的意图(目标切片、自动 HedgeFix、Broken 自动有界平仓、外来成交默认处理、残余修剪、默认减仓、
//   HedgeFix 命令与 Kill{Account, flatten} 的执行);Oms = LegLink 第二腿、SecondRetry、Reverse;
//   Command = 命令直接产生的单,目前只有 SmokeOrder 的冒烟单(带 OrderFlags::SMOKE;其余命令一律经执行层落单)

/// 意图的风控视图(sequencer 从 OrderIntent / HedgeRequest / 执行层意图 / 冒烟单 / SwapIntent + Classification 投影)。
pub struct IntentView {
    pub venue: VenueId, pub account: AccountId, pub side: Side, pub class: IntentClass,
    pub inst: InstId, pub group: GroupId, pub strategy: StrategyId,
    pub origin: Origin,
    pub link_role: LinkRole,                  // None | First | Second | SecondRetry | HedgeFix | Reverse(类型随 LegLink 在 SDK)
    pub tif: Tif,                             // Ioc | Gtc | PostOnly | Fok(SDK);IOC 越价上限 ioc_max_cross 按它判定
    pub flags: u16,                           // OrderFlags(REDUCE_ONLY、POST_ONLY、LINKED、HEDGE_FIX、RISK_REDUCING、SMOKE;位值唯一归属 hft-event `OrderFlags`)
    pub is_swap: bool,                        // optional 链上 SwapIntent(#8 用 chain venue 的桶)
    pub px: Px,                               // 限价(全部订单意图都带限价;引擎单的限价由 hft-types `cap_cross_px` 给出)
    pub qty: Qty,
    pub notional: Notional,                   // = qty × px(定义见检查链 #6)
    // ── 链接腿(带 LegLink 的第一腿 Open:OnFill 的对冲腿或 Simultaneous 的另一腿;Simultaneous 第二腿反过来填第一腿,
    //    取自 HedgeRequest.peer_venue/peer_account;都没有时 link_venue = VENUE_NONE)──
    pub link_venue: VenueId, pub link_account: AccountId, pub link_side: Side,
    pub link_inst: InstId,
    pub link_qty: Qty,                        // 第一腿全部成交时第二腿的数量 = qty × ratio(净量口径)
    pub link_notional: Notional,              // 第二腿名义的上界:两个方向都按 mid × (1 + rr_max_cross) 投影(公式归 sequencer 意图管线第 4 步)
    // ── 对冲类上限(class = Hedge):OMS/执行层提供;策略无链接 Hedge 由 sequencer 读 PosView::hedge_cap_qty(side)──
    pub hedge_cap_qty: Qty,
}

pub trait RiskView {                          // 由 hft-sequencer 用包装 &Oms/&Ledger/&Exec 的适配器实现(泛型、静态分派)
    fn pos(&self, a: AccountId, i: InstId) -> Qty;                      // 永续:持仓;现货:该 group 归属的库存(hft-ledger)
    fn held_notional(&self, a: AccountId, s: Scope) -> Notional;
    fn open_qty(&self, a: AccountId, i: InstId, side: Side) -> Qty;     // 方向在途:即时 + 挂单(OMS `InflightBook` 的 `open_buy`/`open_sell`)
    fn open_qty_immediate(&self, a: AccountId, i: InstId, side: Side) -> Qty;      // 方向在途:只计即时在途(OMS `InflightBook` 按 (account, inst, side) 的 `imm_buy`/`imm_sell`;两类在途的划分归 hft-oms)
    fn open_notional(&self, a: AccountId, s: Scope) -> Notional;
    fn pending_reduce(&self, a: AccountId, i: InstId, side: Side) -> Qty;          // 已挂同向减仓量:即时 + 挂单(`open_buy`/`open_sell`)
    fn pending_reduce_immediate(&self, a: AccountId, i: InstId, side: Side) -> Qty; // 已挂同向减仓量:只计即时在途(`imm_buy`/`imm_sell`)
    fn venue_ready(&self, v: VenueId, a: AccountId) -> bool;           // 无 Unknown、连接 Up(hft-oms)
    fn group_has_unknown(&self, a: AccountId, g: GroupId) -> bool;
    fn equity(&self, a: AccountId) -> EquityView;                       // hft-ledger 合计权益(估值规则为 ledger 参数)
    /// 该 inst 的 BBO 与其连接健康字段(`conn_live_tsc`、`conn_up`、`resync`);按值返回,与 `Views::bbo` 同一投影(写者 sequencer,类型归 SDK)。
    fn bbo(&self, i: InstId) -> BboView;
    // ── 第二腿可负担性(检查链 #7b;只对带链接腿的 Open 调用)──
    /// 现货买入腿:该 (account, venue) 计价资产可用量 = ledger `CashView.free` − 本系统在该 venue 的在途买单名义(OMS,保守全扣);
    /// 资产未初始化(`UNINIT`)→ None。单位:以该计价资产计的定点名义(1e-6)。
    fn avail_quote(&self, a: AccountId, v: VenueId, quote: AssetId) -> Option<Notional>;
    /// 永续腿:可用保证金 = ledger `MarginState.rep_avail` − 本系统在该 venue 在途开仓单的保证金占用(Σ 在途开仓名义_i / 杠杆_i,
    /// 由 OMS InflightBook 按 (account, venue) 增量维护,口径归 hft-oms;O(1));无报告、报告早于本次运行起点或已超 `margin_stale_ms` → None。
    fn avail_margin(&self, a: AccountId, v: VenueId) -> Option<Notional>;
}
pub struct EquityView { pub total: Notional, pub valid: bool }        // valid = 该账户各 venue 余额已初始化且换算率未过期
/// 报价健康:全系统唯一的判定是 SDK `BboView::quote_live`(`conn_up`、未重同步、`now.tsc − conn_live_tsc ≤ conn_liveness_ms × now.tsc_per_ms`),
/// 本 crate 只提供阈值 `conn_liveness_ms` 并在检查链 #5 调用它;报价多久没变化不影响结果。
// HedgePxLimits { conn_liveness_ms: u32, rr_max_cross: Rate }:纯值类型,唯一定义在 hft-types(L1 crate 互不依赖);
// 本 crate 的 hedge_px_limits() 按 RiskParams.conn_liveness_ms 与该 inst 价格带类别的 rr_max_cross 填它,hft-oms price_hedge 接收同一类型。
```

`RejectBits` 的位定义在 [hft-types](../hft-types/README.md)(含 `LINK_STALE = 1 << 28`、`LINK_FUNDS = 1 << 29`);各位何时置位以下面的检查链为唯一定义,唯一例外是策略订单意图的链接结构不合法:sequencer 在模式门之前以 `INVALID` 拒绝、不调用 `evaluate`(判定函数 hft-oms `link_valid`,[LegLink 与 HedgePlan](../hft-oms/README.md#leglink-与-hedgeplan);调用点 [hft-sequencer 意图管线](../hft-sequencer/README.md#意图管线))(`HEDGE_LIMIT` 的上限基数 `hedge_cap_qty` 由产生方按 [hft-exec 对冲量上限](../hft-exec/README.md) 的规则计算,检查在本链 #6)。`IntentClass`、`LinkRole`、`Tif`、`RiskEvent`、`RiskStateView`、`InstRiskView`、`MarginLevel` 的类型在 [hft-strategy-sdk](../hft-strategy-sdk/README.md),本 crate 只填值。

### 保证金分档的输入与输出

```rust
pub struct MarginSample {                              // sequencer 从 ledger `MarginState` 投影;两种强平距离的定义归 hft-ledger
    pub rep_liq_dist: Rate,                            // 按交易所 liq_px:min over 永续持仓 |liq_px − mark| / mark(Rate::MAX = 无可用 liq_px 或无持仓)
    pub est_liq_dist: Rate,                            // ledger 估计:全部永续持仓同时不利变动到强平的比例(Rate::MAX = 无永续持仓)
    pub reported_ratio: Rate, pub est_ratio: Rate,     // 交叉校验:维持保证金 / 保证金余额(统一口径;估计 = 维持 / 估计保证金余额)
    pub report_ens: i64,                               // 最近一条 MarginReport 的 engine_ns
    pub mark_ens: i64,                                 // 该 (account, venue) 永续持仓中最旧的标记价时刻(ledger `MarginState.mark_oldest_ens`;i64::MAX = 无永续持仓)
    pub has_report: bool, pub truncated: bool,         // truncated = MarginReport 带 ITEMS_TRUNCATED
}
pub struct MarginOut {                                 // 每次调用只涉及一个 (account, venue),以下容量是构造上界,不会溢出
    pub mode: FixedVec<mode::ModeInput<'static>, 4>,   // Auto/AutoClear(MarginStress)→ sequencer 调 mode.apply
    pub events: FixedVec<(AccountId, RiskEvent), 4>,   // RiskEvent::MarginStress{venue, level, liq_dist, ratio, deadline_tsc} → 该账户绑定的策略实例与 hft-exec `on_risk`
    pub force: Option<ForceDeleverage>,                // L3:默认减仓一步 → hft-exec `force_deleverage`
    pub timers: FixedVec<(RiskTimer, i64), 4>,         // 待登记的 Timer{Risk, …};截止为 engine_ns,sequencer 按 `Now::tsc_of`(向上取整)换成本次运行的 TSC 进堆
    pub notes: FixedVec<Note, 4>,                      // MarginLevel(L2 = P1,L3 = P0)
}
pub struct ForceDeleverage { pub account: AccountId, pub venue: VenueId, pub frac: Rate /* = force_step */ }
pub enum RiskTimer { MarginStale { a: AccountId, v: VenueId }, MarginDeadline { a: AccountId, v: VenueId }, MarginForce { a: AccountId, v: VenueId } }
```

`ForceDeleverage` 的执行(按比例把该 venue 的永续敞口降 `frac`、步与步按比例复合、`PerpFirst`、另一腿同比例、`reason = MarginStress`;另一腿不可用时的串行退路;设引擎目标前撤该 inst 上的非执行层挂单;该账户 `GROUP_NONE` 的永续持仓同样减)归 [hft-exec](../hft-exec/README.md);**处于 `Broken` 的 group 同样减**(先对永续腿 reduce-only 减仓,再处理现货腿,见[断腿与自动有界平仓](#断腿broken自动有界平仓l3-与-flatten))。本 crate 只决定何时、对哪个 (account, venue)、每步降多少。

### mode 模块:唯一的模式状态

```rust
pub mod mode {
    pub use hft_types::{RunMode, CapSource, KillLevel};     // 枚举与线上编码在 hft-types
    pub use hft_event::CapRecord;                             // 48 B,布局在 hft-event

    pub struct Mode {
        table: ModeTable,             // (strategy, account, group) → 期望 RunMode;另存每 (strategy, account) 的 default_mode
        caps: CapSet,                 // FixedVec<CapRecord, 256>,按 (source, scope) 排序
        eff: EffCache,                // 每 cell 的有效模式缓存
        gates: Gates,                 // 每账户 gate_bits、各 venue 位图、recovery_flags、released
        kill_bits: [KillBits; MAX_GW],// 每 gw 一个,由 caps 投影(本模块不碰原子量;模拟账户不经 gw)
        ctl_state_version: u64,       // 每次 SetMode / Kill / Resume / cap 增删 +1(Viewer 紧急停止的确认不比对它,规则在 hft-sequencer)
        clean_restart: CleanRestart,  // 干净 Drain 重启判定
    }
    pub enum ModeInput<'a> {
        SetMode { scope: CmdScope, mode: RunMode },
        Kill(KillCmd),
        Resume { source: CapSource, scope: CmdScope },
        Auto(CapRecord),              // DailyLoss / Recon / UnknownTimeout / Broken / Reorg / Recovery / Draining / Halted / MarginStress / AccountCfg / Kill(身份不符)
        AutoClear { source: CapSource, scope: CmdScope },
        Clock { ok: bool },           // Timer{ClockOk|ClockBad}
        Storage { kind: StorageKind /* PersistLag | DiskLow | Both(StorageOk) */, level: u8 /* 0 正常 1 禁开 2 Drain */ },
        StorageRecheck,               // drain_deferred() 为真时 sequencer 在每条 Input 的执行层推进之后调用一次:只复核 StorageCritical 的推迟条件,不改 cap
        Notice(&'a VenueNoticeCmd),   // VenueNotice 命令;Delisting → Delisting cap
        Gate(GateEvent<'a>),          // ConnState、簿 Live、Unknown 计数、对账组、差异处置、时钟、AccountConfigReport、KillSelfTestOk
        CellFlat { cell: CellKey, flat: bool },
        GroupsChanged { added: &'a [CellKey], removed: &'a [CellKey] },
        RecoveryFlags { account: AccountId, flags: RecoveryFlags },
    }
    // RecoveryFlags(u8,本模块定义):TAIL_LOST = 1、RECOVER_MISMATCH = 2、POISON = 4、FROM_SNAPSHOT = 8、
    //   CLEAN_DRAIN = 16(sequencer 判定"干净 Drain 重启"的条件 ①–③ 成立时置位)。
    pub enum GateEvent<'a> {
        Feeds { account: AccountId, ok: bool },
        /// sequencer 从 gw 连接的 ConnState 汇总:primary_up = 主下单连接已登录且用户流 Up;standby_up = 热备 Up。
        OrderConns { account: AccountId, venue: VenueId, primary_up: bool, standby_up: bool },
        Unknown { account: AccountId, count: u32 }, Clock { ok: bool },
        Recon(&'a ReconGateInput),    // on_reconcile 的闸门输入(每 (account, venue) 一组)
        Disposed { account: AccountId, venue: VenueId, report_id: u64, item: ReconItemKey, how: Disposition },
        /// account、venue、mismatches(含 UID、UNREADABLE 位)、uid_keys 与三种模式取自报告头;本模块按 refdata VenueCaps 复算
        /// POS_MODE/MARGIN_MODE/MULTI_ASSET 并与 mismatches 取并集。配置不符 = 并集中 UID 以外的任一位。
        AccountCfg { hdr: &'a AccountConfigReportHdr, keys_required: u8 /* 该 (account, venue) 的交易 key 集合(SESSION=1 REST=2);sequencer 按装配时载入的 key 填 */ },
        KillSelfTest { account: AccountId },  // Timer{Sys, KillSelfTestOk}:hft-core 启动时该账户的 kill 位自检通过
    }
    pub enum Disposition {
        Adopted,                      // AdoptExchange(仓位或余额条目)
        Baselined,                    // SetBalanceBaseline
        AutoExternal,                 // 带 AUTO_EXTERNAL 的仓位条目(永续同号缩小,含缩到 0 的全量 ADL;置位条件归 reconciler)经 sequencer 复核后
                                      // 按"引擎仓位 − 交易所仓位"自动入账(复核与合成规则归 hft-sequencer)
    }
    /// 由 sequencer 启动时按 `ModeOutcome::with_capacity(max_cells_total, n_accounts, n_account_venues)` 预分配一次,每次 apply 先清空再填。
    /// 各向量容量取构造上界(N = max_cells_total,数值归 apps/hft-core;A = refdata 账户数;AV = (account, venue) 数),一次 apply 不可能溢出,
    /// 所以从不截断或丢弃;下游投递给策略的通知若超出 sequencer 的 NotifyQueue,由 sequencer 续推(见 hft-sequencer"通知队列与续推")。
    pub struct ModeOutcome {
        pub cells: PreVec<(CellKey, Effective, Effective)>,     // 容量 N:有效模式变化 (cell, old, new) → sequencer 投递 Mode 事件(推迟落 Shadow 的注记也由它据此写)
        pub cancels: PreVec<ModeCancel>,                        // 容量 N:需撤挂单的范围(每个转为 CloseOnly 的 cell 至多一条;覆盖多 cell 的 cap 只出一条更大的范围)
        pub kill_changed: bool,                                  // 任一 gw 的 kill_bits() 与 apply 前不同 → true → sequencer 让实盘驱动重写 kill 字
        pub resolve_broken: bool,                                // 只在删除带 HARD 的 Kill 记录时为真 → sequencer 查 naked_groups,按 hft-exec 的判定(补腿口径)设 Broken
        pub released: PreVec<AccountId>,                         // 容量 A:启动闸门刚放行的账户 → sequencer 调 exec.on_gate_open
        pub hedge_gate_open: PreVec<(AccountId, VenueId)>,       // 容量 AV:对冲闸门子集刚满足的 (account, venue) → sequencer 让 OMS 续接续接单在该 venue 的 RESUME_HOLD 链接
        pub venue_gate_open: PreVec<(AccountId, VenueId)>,       // 容量 AV:恢复期按 venue 放行的条件刚满足的 (account, venue) → sequencer 调 exec.on_venue_gate_open
        pub gate_changes: PreVec<GateChange>,                    // 容量 A:闸门位有变化的账户 → sequencer 写启动闸门注记
        pub risk_events: PreVec<(AccountId, RiskEvent)>,        // 容量 N + 2A:HedgeBroken{group}、CapSet/CapCleared → 该账户绑定的策略实例
        pub drain: Option<DrainCause>,                           // → sequencer 发起内部 Drain(StorageCritical 在 ModeView::engine_reduce_pending() 为真时推迟,见"存储降级")
        pub daily_loss_resumed: PreVec<AccountId>,               // 容量 A:本次 apply 中 Resume{DailyLoss} 删除了 cap 的账户 → sequencer 调 risk.on_daily_loss_resumed
        pub notes: PreVec<Note>,                                 // 容量 8(一次 apply 至多增删 4 条 cap,另至多一条闸门注记):CapSet / CapCleared / RecoveryHeldSim / IdentityUnverified
        pub ctl_state_version: u64,
    }
    pub struct ModeCancel { pub scope: CancelScope, pub open_only: bool }  // open_only:非 HARD 的 Kill 与转为 CloseOnly 的 cell 为 true(只撤非 RISK_REDUCING);HARD 的 Kill 为 false
    pub struct GateChange { pub account: AccountId, pub old_bits: u32, pub new_bits: u32, pub released: bool }
    pub enum DrainCause { StorageCritical, IdentityMismatch { account: AccountId, venue: VenueId } }  // 后者:任何时刻的 UID 不符
    // PreVec<T>:与 FixedVec 同语义(满即不可再推、从不重新分配),但容量在启动时给出、一次分配。
    impl Mode {
        pub fn apply(&mut self, inp: ModeInput<'_>, seq: u64, v: &impl ModeView, out: &mut ModeOutcome) -> Result<(), ModeErr>; // ModeErr → CmdResult(映射见 hft-sequencer 命令路径)
        pub fn effective(&self, cell: CellKey) -> Effective;     // 读缓存,O(1);模式门每意图调用
        pub fn venue_level(&self, a: AccountId, v: VenueId) -> CapLevel; // 覆盖 (a, v) 的全局、账户 a 级与 (a, v) 级 cap 的最高级(不含 cell 级的 Broken/Halted/Reorg 与期望模式)
        pub fn inst_blocked(&self, v: VenueId, i: InstId) -> bool; // Delisting
        pub fn kill_bits(&self) -> &[KillBits];                  // LiveDriver 写进各 gw 的 KillWord
        pub fn released(&self, a: AccountId) -> bool;
        pub fn hedge_gate_ok(&self, a: AccountId, v: VenueId) -> bool; // 对冲闸门子集(见"启动闸门")
        pub fn venue_gate_ok(&self, a: AccountId, v: VenueId) -> bool; // 恢复期按 venue 放行的条件(见"启动闸门")
        pub fn ctl_state_version(&self) -> u64;
        pub fn drain_deferred(&self) -> bool;                    // Storage cap 某项档位为 2 且没有 Draining cap(见"存储降级")
        pub fn is_upgrade(&self, inp: &ModeInput<'_>) -> bool;   // sequencer 判定是否需要确认(按目标值):SetMode{Live}、Resume、Kill{Account, flatten} 为真,其余为假
        pub fn preview(&self, inp: &ModeInput<'_>, v: &impl ModeView, out: &mut Preview); // PREVIEW 命令:不改状态;Resume 类列出未处置的对账差异
    }
    pub enum ModeErr {
        GateNotReleased,              // 目标账户启动闸门未放行时的 SetMode → Live、Resume
        Draining,                     // Draining 期间的升级输入
        NakedNotCleared,              // Resume{Broken} 时已成交裸量仍高于尘埃线
        BrokenRemaining,              // Resume{HedgeFailure} 时该账户仍有 Broken
        ReconNotClean,                // HARD 解除时对账未满足严格条件;Resume{Recon} 时对账恢复条件未满足
        ResumeLimit,                  // Resume{DailyLoss} 当日次数已用完
        NoMatchingCap,                // Resume 找不到完全匹配的 cap
        NotResumable,                 // Resume 的来源不能被命令清除
        InvalidScope,                 // 参数组合非法(hard 与 Venue、flatten 与 Global、flatten 与 hard)
        UnknownPending,               // Resume{UnknownTimeout} 时该 (account, venue) 仍有 Unknown 槽
    }
    // apply 的拒绝按以下顺序取第一个:InvalidScope → NotResumable → Draining → GateNotReleased → NoMatchingCap →
    // 来源前置条件(NakedNotCleared、BrokenRemaining、ReconNotClean、ResumeLimit、UnknownPending);被拒的 apply 不改任何状态、
    // ctl_state_version 不变、ModeOutcome 各向量为空。
    /// mode.apply 读取的模式状态之外的事实;由 sequencer 的适配器实现(泛型、静态分派),只读。
    pub trait ModeView {
        fn cell_flat(&self, c: CellKey) -> bool;                  // 净仓、在途、LegTracker 残余全为 0(SetMode{Shadow} 是否推迟)
        fn naked_cleared(&self, a: AccountId, g: GroupId) -> bool; // = Risk::naked_cleared(Resume{Broken}/Resume{Reorg} 的前置)
        fn recon_resumable(&self, a: AccountId, v: VenueId) -> bool; // = Risk::recon_resumable(Resume{Recon} 的前置)
        fn daily_loss_resumable(&self, a: AccountId) -> bool;     // = Risk::daily_loss_resumable(否则 ResumeLimit)
        fn unknown_count(&self, a: AccountId, v: VenueId) -> u32; // OMS:该 (account, venue) 的 Unknown 槽数(Resume{UnknownTimeout} 的前置)
        fn lev_bad(&self, a: AccountId, v: VenueId) -> bool;     // = Risk::lev_bad(Gate(AccountCfg) 与 GroupsChanged 时读)
        fn engine_reduce_pending(&self) -> bool;                  // sequencer 转调 hft-exec `engine_reduce_pending()`:任一账户有未到达的 reason ∈ {MarginStress, Hard}
                                                                  // 引擎目标(含无组 cell 与 HardAlign 对齐目标;口径归 hft-exec,与 drain_ready_acct 中该项相同)
    }
    pub enum Effective { Shadow, CloseOnly { deferred_shadow: bool }, Live }
    pub struct CellKey { pub strategy: StrategyId, pub account: AccountId, pub group: GroupId }
}
```

## 实现说明

<details><summary>实现要点</summary>

### 分类

`classify()` 是 RISK_REDUCING / reduceOnly 分类表的唯一归属。输入:意图草稿(来源、链接角色、side、qty、tif、`HedgePlan.reduce_only`、`OrderFlags`)、`Origin`、该 (account, inst) 当前仓位(现货为该 group 归属的库存)与已挂同向减仓量、该 group 已成交净量及买卖方向各自的全部在途量(含挂单)、该 venue 的 `VenueCaps.reduce_only`(`VenueId` 即产品线,能力表按 `VenueId` 一行)。

| 意图 | `class` | `risk_reducing` | `reduce_only`(交易所侧) |
|---|---|---|---|
| LegLink 第二腿 `role = Second`,开仓链接(`HedgePlan.reduce_only = 0`:现货买入后开永续空等) | Hedge | 是 | **否**(在该 venue 是开新仓) |
| **开仓链接的重试**(`role = SecondRetry`,继承 `HedgePlan.reduce_only = 0`) | Hedge | 是 | **否** |
| 第二腿或其重试,平仓链接(`HedgePlan.reduce_only = 1`) | Hedge | 是 | 是(`VenueCaps.reduce_only` 时) |
| `Simultaneous` 第二腿(与第一腿同一事件发出) | 减仓提示为 Reduce,其余 Open | Reduce 时校验可减量 | Reduce 且支持时为是 |
| `Simultaneous` 反向单(`Reverse`)、`HedgeFix`(执行层的自动补腿、外来成交默认处理、残余修剪、`Broken` 的自动有界平仓、`HedgeFix` 命令:只平掉多出的一腿) | Hedge | 是 | 是(支持时) |
| `Flatten`:策略硬动作、执行层默认减仓(`reason ∈ {MarginStress, Hard}`,含 `Broken` 的 group 与 `SerialFallback` 路径)、`Kill{Account, flatten}` 清仓 | Flatten | 是 | 是(支持时) |
| `Reduce`,品种支持 reduceOnly(永续减仓、目标意图的减仓切片) | Reduce | 仅当 `qty ≤ |pos| − 已挂同向减仓量` | 是 |
| `Reduce`,品种不支持 reduceOnly(现货卖出) | Reduce | 仅当 `qty ≤ |pos| − 已挂同向减仓量`(`pos` = 该 group 归属的库存净量) | 否 |
| 策略无链接 `Hedge`,数量为正、方向朝组净量的零点、数量不超过可对冲量 | Hedge | 是 | 仅本腿方向也减仓、数量不超过本腿可减量且 venue 支持时为是 |
| 冒烟单(`Origin::Command`、`OrderFlags::SMOKE`) | Open | 否 | 否 |
| 其余(含目标意图的增仓切片) | Open | 否 | 否 |
| `Amend` | 沿用原单 | 原单为是且新量 ≤ 原量 | 沿用原单 |

- **策略无链接 Hedge**:只有意图品种属于该账户的 group 才提供组事实;`PosFacts.group_net = Σ leg.net`(i128),`group_pending_buy/sell` 各为该组所有品种的 OMS `open_buy/open_sell` 之和(即时与挂单,两个方向不互抵)。`PosView::hedge_cap_qty(side)` 对朝零方向返回 `max(0, |group_net| − 同向在途)`(封顶 i64::MAX),其余为 0;事实缺失为 0。`qty` 超过此量或方向扩大裸露时归 Open,CloseOnly 仍以 `CLOSE_ONLY` 拒绝。被分类为 Hedge 的意图把同一可对冲量交给 #6,仍受 HARD、价格、名义、仓位与速率约束;本腿可开新仓以减少组裸量,不强加交易所 reduceOnly。
- `SecondRetry` 只用于重试计次与 `Note`;分类与 `Second` 完全相同(重试保留第二腿语义,`reduce_only` 继承 `HedgePlan.reduce_only`,规则在 [hft-oms](../hft-oms/README.md))。`HedgeFix` 角色只用于"平掉多出的一腿"。
- 执行层订单给出的 `class` 只是提示;最终分类以本表为准([hft-exec](../hft-exec/README.md))。
- `Reduce` 不满足 `qty ≤ |pos| − 已挂同向减仓量` 时 `risk_reducing = false`,检查链 #7 以 `REDUCE_INVALID` 拒绝(不按 Open 放行)。
- 链上意图(optional)同一函数分类,结果写进 `SwapDecision.sflags.RISK_REDUCING`;kill 下链上与 CEX 一样只看这个位。`Cancel`、`CancelAll`、`Query` 不分类(永远放行)。

### 检查链(按廉价 → 昂贵,短路;适用范围以本表为唯一定义)

检查链 #0 的 lot 例外只适用于 `POSITION_CLOSE` 标记、`origin = Exec`、`class = Flatten`、`RISK_REDUCING | REDUCE_ONLY`、无链接且 refdata 声明 `POSITION_CLOSE` 的永续单。其余带该标记的意图以 `INVALID` 拒绝。该例外只放宽 lot 整数倍;正量、tick、绑定、HARD、报价、名义上限及 #7 在途扣减照常检查。

| # | 检查 | 失败位 | Open | Hedge | Reduce / Flatten |
|---|---|---|---|---|---|
| 0 | 结构:`qty > 0`、tick/lot 网格、inst 可交易(`InstStatus::Trading`;`ReduceOnly` 只放行 `RISK_REDUCING` 单;其余状态 `NOT_TRADABLE`);意图的 `account` 绑定到发出它的实例(`AccountMeta.bound_instance`;OMS/执行层/命令的意图按所属 group 的实例;冒烟单的 `strategy` 由 hft-oms 填为该账户绑定的实例,照常查绑定,另要求账户为 Live,否则 `INVALID`) | `INVALID`、`NOT_TRADABLE`、`NOT_BOUND` | ✓ | ✓ | ✓ |
| 1 | HARD:覆盖本意图的 `Kill` cap 带 `cflags.HARD`(Global,或本账户;有链接腿时另查 `link_account`) | `HARD` | ✓ | ✓ | ✓ |
| 2 | 时钟:`ClockBad` 生效 | `CLOCK` | ✓ | — | — |
| 3 | 上限:`mode.effective(cell)` 为 CloseOnly;`mode.venue_level(account, venue) ≥ CloseOnly`;`Draining`;`mode.inst_blocked(venue, inst)`;有链接腿时另查 `venue_level(link_account, link_venue)` 与 `inst_blocked(link_venue, link_inst)` | `DRAINING`、`KILLED`、`DAILY_LOSS`、`RECON_DEVIATION`、`CLOSE_ONLY` | ✓ | 无 `RISK_REDUCING` 时 ✓ | 无 `RISK_REDUCING` 时 ✓ |
| 4 | 条件门:`venue_ready(venue, account)`;该 (account, group) 有 Unknown 且 `unknown_blocks_open`;有链接腿时另查 `venue_ready(link_venue, link_account)` | `VENUE_NOT_READY`、`UNKNOWN_PENDING` | ✓ | — | — |
| 5 | 报价与价格带:本 inst 的 BBO 两侧都有(`mid` 存在)且**报价健康**(`bbo(inst).quote_live`,阈值 `conn_liveness_ms`;报价不变化不算过期,健康时就用该连接上的最新值);Open:`|px − mid| ≤ max_px_dev × mid`、`tif = Ioc` 时越过对手价的比例 ≤ `ioc_max_cross`(买 `(px − ask) / ask`,卖 `(bid − px) / bid`);风险递减单:**只查越价上限** `rr_max_cross`(向不利方向越过 mid 的比例:买 `(px − mid) / mid`,卖 `(mid − px) / mid`;`origin = Exec` 的 `Flatten` 类单在 `RiskParams.l3_max_cross > 0` 时取 max(`rr_max_cross`, `l3_max_cross`));以上阈值都取**本 inst 所属价格带类别**的值;**带链接腿的 Open 另查第二腿报价**:`bbo(link_inst).quote_live`,与 OMS `price_hedge`、hft-exec 用同一判定与同一阈值 | `NO_MID`(一侧为空)、`STALE_MID`(连接非 Live、重同步中或静默超过 `conn_liveness_ms`)、`PX_BAND`、`LINK_STALE` | ✓ | ✓(越价上限) | ✓(越价上限) |
| 6 | 名义与对冲量:**名义 `IntentView.notional = qty × px`**(`px` 为意图限价;以计价资产计,按 hft-ledger 换算率折 USDT,i128 中间积,向上取整;所有类别同一口径)。Open 四级(单笔 `max_order_notional`、group、账户、venue,含在途);Hedge:`qty ≤ hedge_cap_qty` 且单笔名义 ≤ `max_hedge_notional`;Reduce/Flatten:单笔名义 ≤ `max_reduce_notional`(超出 → `NOTIONAL_ORDER`);Open 四级依次为 `NOTIONAL_ORDER`、`NOTIONAL_GROUP`、`NOTIONAL_ACCOUNT`、`NOTIONAL_VENUE` | `NOTIONAL_*`、`HEDGE_LIMIT` | ✓ | ✓ | ✓ |
| 7 | 仓位:Open:方向含在途(即时 + 挂单)≤ `max_pos_qty`(买 `pos + open_qty(Buy) + qty`,卖 `open_qty(Sell) + qty − pos`;非 reduceOnly 的 Hedge 同式、只计即时在途);**Hedge 与 Flatten 只扣即时在途**:非 reduceOnly 的 Hedge 方向含即时在途 ≤ `max_pos_qty`,reduceOnly 的 Hedge 与 Flatten `qty ≤ |pos| − 已挂同向减仓量(只计即时在途)`;Reduce:`qty ≤ |pos| − 已挂同向减仓量(含挂单)`;带链接腿的 Open 另查链接腿 inst 方向含在途 + `link_qty` ≤ 其 `max_pos_qty` | `POSITION`、`REDUCE_INVALID` | ✓ | ✓ | ✓ |
| 7b | 链接腿名义与可负担性(只对带链接腿的 Open):① `link_notional ≤ max_hedge_notional`(第二腿一定会被 #6 放行);② **第二腿可负担**:链接腿是现货买入 → `avail_quote(link_account, link_venue, 计价资产) ≥ link_notional × (1 + funds_buffer)`;链接腿是现货卖出 → `pos(link_account, link_inst) − pending_reduce(…, Sell) ≥ link_qty`(不做借币);链接腿是永续 → `avail_margin(link_account, link_venue) ≥ link_notional / lev × (1 + funds_buffer)`,`lev = leverage(link_account, link_venue, link_inst)`(交易所账户实际杠杆;不设常量默认);视图返回 None(未初始化、无报告或过期)或杠杆未知按不足处理 | `HEDGE_LIMIT`、`LINK_FUNDS` | ✓ | — | — |
| 8 | 速率:(venue, account) 两窗口令牌桶与普通单的 inst 桶,退避期内拒;**非风险递减单只能用 `rr_reserve` 以上的令牌**,风险递减单可用到 0;带链接腿的 Open 另要求链接 venue 不在退避期且有 ≥ 1 个 `rr_reserve` 以上的令牌(只查不扣) | `RATE_BACKOFF`、`RATE` | ✓ | ✓ | ✓ |
| 9 | 敞口门:该 group 最近一次 `on_legs` 的 `naked_ratio > naked_gate` 且 `naked_notional ≥ naked_gate_min` | `EXPOSURE_GATE` | ✓ | — | — |

- **拒绝只带一位**:`Err(RejectBits)` 恰有一位——按 # 顺序第一个失败的检查,同一检查内按"失败位"列的列出顺序取第一个成立的位。#3 的位按挡住该意图的来源取:`Draining` → `DRAINING`;`Kill`(含清仓锁)→ `KILLED`;`DailyLoss` → `DAILY_LOSS`;`Recon` → `RECON_DEVIATION`;期望模式 CloseOnly 与其余 cap(含 `Delisting` 的 `inst_blocked`)→ `CLOSE_ONLY`。#5 先查本 inst(`NO_MID` → `STALE_MID` → `PX_BAND`),再查链接腿(`LINK_STALE`)。#8 先查本 venue,再查链接腿 venue;每个 venue 先查退避(`RATE_BACKOFF`;退避期内桶已清零,所以退避先于令牌),再查两窗口桶(`RATE`),本 venue 最后查 inst 桶(`RATE`)。
- "✓"即该检查对该类别生效;风险递减单(`RISK_REDUCING`)不是"只做结构检查"——价格带越价上限(#5)、名义与对冲量上限(#6)、仓位(#7)、速率(#8)对全部类别生效,只是阈值不同;#2、#3、#4、#9 只拦 Open 与无 `RISK_REDUCING` 的单。
- 所有 cap(含 `MarginStress`、`HedgeFailure`、`Storage`、`AccountCfg`、`Halted`、`ClockBad`)在 #2/#3 只产生 Open 的拒绝;具体 cap 来源由 `CapSet` 注记与 `CmdPreview.cap_sources` 给出。
- **#7 只扣即时在途的理由**:引擎的 Flatten(L3、清仓、对齐目标)与对冲腿必须能在策略挂着同向 reduce-only 或减仓挂单时照样出单;挂单在途(GTC/PostOnly 已 Ack)不计入它们的可减量。设 `MarginStress`/`Hard` 引擎目标时 hft-exec 先请 OMS 撤该 inst 上本账户的非执行层挂单,撤单确认前按 `|pos| − 即时在途` 截量(规则归 hft-exec);永续腿的交易所侧 reduceOnly 保证不会因两者同时成交而穿越零。
- **冒烟单**(`origin = Command` 且 `flags` 含 `SMOKE`,命令 `SmokeOrder`,语义归 [control-plane](../../docs/subsystems/control-plane.md)):按 Open 走全链,但 ① 不经模式门,#3 不读 cell 有效模式与账户级、venue 级的条件 cap(Shadow、CloseOnly、Recovery、DailyLoss、MarginStress、HedgeFailure、Recon、AccountCfg 都不拦),只查 `Draining`、`inst_blocked` 与覆盖该 (account, venue) 的 `Kill` cap(`KILLED`,与 gw 的 kill 字一致);② #5 只查报价健康(`NO_MID`/`STALE_MID`),不查 `max_px_dev` 与 `ioc_max_cross`(冒烟单按定义远离市价且是 PostOnly;交易所自身的价格保护带本链不检查,是否接受与被拒时的处理归 [hft-oms 冒烟单](../hft-oms/README.md));③ #7 用该账户 `default_group` 的 `max_pos_qty`;④ #9 不适用(不属于任何 group)。#1 HARD 照拦。
- **`HEDGE_LIMIT` 是终局拒绝**:产生方不得重试同一请求;sequencer 收到 Hedge 类被 `HEDGE_LIMIT` 拒绝时不出环、对 OMS 来源调 `oms.hedge_vetoed`、对执行层来源回填 `Rejected(HEDGE_LIMIT)`,并设 `Broken{account, group}` + P0(数量或单位解码错误使成交被放大时在这里截住)。其他位对 Hedge 类的拒绝按 [hft-oms](../hft-oms/README.md) 的重试规则处理(门控拒绝不计次、累计时长,超 `hedge_deadline_ms` → Broken)。
- **对冲腿超滑点改价而不失败**:`oms::price_hedge` 在对手价偏离超过其 `max_slip` 时按 `hedge_px_limits().rr_max_cross` 的上限定价发出(仍是 IOC),因此 #5 的越价上限放行它;只有对手价已越过 `rr_max_cross` 时 `price_hedge` 才返回门控拒绝(不计执行失败次数)。
- **引擎单的限价与名义**:OMS `price_hedge` 与 hft-exec 的全部 Engine 订单(HedgeFix、AutoFix、ForceStep、Flatten、补腿)用同一纯函数 hft-types `cap_cross_px` 定价,限价不越过 `mid × (1 ± 上限)`;上限按本笔 inst 取值——hft-exec 读 SDK `Views::risk_for(a, inst)`(本 crate `view_for`:该 inst 所属类别的 `rr_max_cross`,Open 切片另用 `ioc_max_cross`;`reason ∈ {MarginStress, Hard}` 的引擎单在 `l3_max_cross > 0` 时取两者较大者),与 #5 同值同源;数量按限价折算、切片名义 ≤ `max_order_notional`(Open)/ `max_reduce_notional` / `max_hedge_notional`。因此 #5、#6 放行引擎单;Flatten/ForceStep 仍被 #5/#6 拒绝时由执行层按执行失败处理(`ExecDecision{ForceStep, detail = RiskReject}` + P0,规则归 [hft-exec](../hft-exec/README.md))。`conn_liveness_ms`、各类别的 `PxBand` 与 `l3_max_cross` 的唯一配置位置是本页 `RiskParams`。
- **报价新鲜度 = 连接健康**:对冲腿、风险递减单、自动有界平仓、L3 减仓与链接腿 #5 一律用 `BboView::quote_live` 判定,报价取该连接上的最新值——盘口安静、几秒不变不是过期;只有该 inst 的 BBO 连接全部断开或静默超过 `conn_liveness_ms`(默认 2 000)、或处于重同步才是(多连接成员的聚合规则归 [hft-sequencer 连接活性](../hft-sequencer/README.md#连接活性))。hft-venue 对每个 BBO 连接每 `bbo_ping_ms`(1 000)发 WS ping 保证安静行情下也有回帧([hft-venue](../hft-venue/README.md));`conn_liveness_ms` 必须 ≥ 2 × `bbo_ping_ms`(hft-core 启动校验)。本 crate 没有按报价变化时刻计年龄的参数。
- 超出单笔名义上限的减仓由执行层切片([hft-exec](../hft-exec/README.md)),风控不替它拆单;`max_hedge_notional`、`max_reduce_notional` 经 `Views::risk` 提供给执行层。
- 链上 `SwapIntent`(optional,`is_swap`)走同一链,#8 用 chain venue 的桶。

**链接腿规则**:带 LegLink 的第一腿 Open(OnFill 的第一腿、Simultaneous 的两腿互为链接腿)在 #1、#3、#4、#5(第二腿报价新鲜度)、#7、#7b(第二腿名义与可负担性)、#8 对**两腿的 venue 同时**检查;任一腿失败即整笔拒绝,第一腿不发。这样第二腿 venue 被 kill、断连、限速、Unknown、保证金受限、已下架、报价连接不健康(`LINK_STALE`)或资金不足(`LINK_FUNDS`)时不会先开出一条裸腿。`link_notional` 是 sequencer 投影的第二腿名义上界(买卖两个方向都按 `mid × (1 + rr_max_cross)`,不低于第二腿任何可能限价下的名义),所以 #7b ① 放行的链接,其第二腿在 OMS 超滑点按上限定价时仍不会被 #6 以 `HEDGE_LIMIT` 拒绝;#7b ② 的保证金与计价资产检查按同一上界,卖出的永续第二腿因此偏保守。`funds_buffer`(默认 0.05)覆盖手续费、价格在两腿之间的移动与余额推送的滞后;可负担性只是下单时刻的检查,不预留资金(见"已知限制")。一个账户在同一交易所只有一个 venue(refdata `SameExchangeMarkets` 校验,[hft-refdata](../hft-refdata/README.md)),所以两腿的资金互不共用,本检查按链接腿 venue 单独计算即可。同账户任一 group 进入 `Broken` 时本模块同时设账户级 `HedgeFailure`,挡该账户全部 Open,直到该账户已无 `Broken` 且人工 `Resume{HedgeFailure}`。

### 断腿(Broken)、自动有界平仓、L3 与 flatten

本节是本 crate 视角的规则(cap 与检查);执行动作(选腿、切片、计时器、HARD 抑制、人工保持、串行退路)归 [hft-exec](../hft-exec/README.md),链接截止与重试归 [hft-oms](../hft-oms/README.md)。

1. **`Broken` 只拦 Open**:`Broken{account, group}` 挡该 cell 的 Open,`HedgeFailure{account}` 挡该账户全部 Open;两者都不拦 Reduce / Flatten / Hedge。只有 HARD(检查链 #1)拦风险递减单。
2. **自动有界平仓**:某 (account, group) 处于 `Broken` 持续 `broken_auto_fix_ms`(ExecConfig,默认 120 000)且期间无人工 `HedgeFix`,执行层做**一次可执行的**"平掉多出的一腿"尝试(永续腿 reduce-only;现货腿卖出,数量 ≤ min(库存, 可用余额));"一次"的口径(哪些情况只等待或重试、哪些判失败)归 hft-exec。按 `max_reduce_notional` 切片,总量 ≤ 当前裸量。这些单分类为 `Hedge`(`LinkRole::HedgeFix`,`RISK_REDUCING`),照常受 #5(`rr_max_cross`)、#6(`qty ≤ hedge_cap_qty`,单笔名义 ≤ `max_hedge_notional`)、#7、#8 约束。HARD 生效时执行层不发起,即使发起也被 #1 拒绝;恢复期(账户启动闸门放行前)按多出那一腿所在 venue 的 `venue_gate_ok` 开始与计时(见[启动闸门](#启动闸门),执行规则归 hft-exec)。
3. **`Broken` 的条件型解除**(唯一定义):`on_legs` 报告该 group **已静止**(`quiescent`:无 Pending*/Unknown 槽、各腿无即时在途)且**已成交裸量**的名义 `naked_filled_notional` ≤ 该账户的 `dust_notional`(尘埃线的唯一定义在 [hft-exec](../hft-exec/README.md),自动有界平仓的成功判定用同一口径、每片终态后判断)时,本模块返回 `AutoClear{Broken, (account, group)}`,cap 删除并产出 `CapCleared{Broken}`;执行层据此只清除该 cell 的策略目标、保留引擎目标(规则归 hft-exec),清仓或 L3 进行中经过尘埃线时不会丢掉引擎的减仓。在途单不算已对冲:IOC 平仓片发出后、终态之前不解除,零成交后裸露照旧。裸量可以经人工 `HedgeFix`、自动有界平仓(执行层随之写 `Note{ExecAutoFixDone}`,P1)、L3 / flatten 或人工 `WriteOffResidual` 核销降下来,四者同一口径。**`HedgeFailure` 不随之删除**,须该账户已无 `Broken` 后人工 `Resume{HedgeFailure}`。自动有界平仓判失败 → 执行层写 `Note{ExecAutoFixFailed}`(P0),`Broken` 保持、等人工。
4. **L3 与 flatten 覆盖 Broken**:`ForceDeleverage`(保证金 L3)与 `Kill{Account, flatten}` 对 `Broken` cell 同样生效,不跳过:先对永续腿 reduce-only 减仓(`PerpFirst`,单笔 ≤ `max_reduce_notional`),再处理现货腿;这些单是 `Flatten` 类,`Broken`/`HedgeFailure`/`MarginStress`/清仓锁都不拦它们。
5. **冷启动不立刻判 Broken**:`hedge_deadline_ms` 只按引擎运行时间累计(停机与续接前的等待都不计,规则在 hft-oms 的 `recovery_end`)。某账户在某 venue 的**对冲闸门子集**(见[启动闸门](#启动闸门))刚满足时,`ModeOutcome.hedge_gate_open` 列出该 (account, venue),sequencer 让 OMS 对续接单(第二腿或 `Reverse` 的第一腿)在该 venue、处于 `RESUME_HOLD` 的链接续接一次,之后按正常截止判断;`RESUME_HOLD` 超过 `resume_hold_max_ms` 的处理归 hft-oms(OMS 对有敞口的链接判 `Broken` 并写 P0 `Note{OmsResumeTimeout}`),本模块按下表对 OMS 报告的 `Broken` 设 cap。
6. **`resolve_broken`**:本模块只在删除带 HARD 的 `Kill` 记录时产出(HARD 期间补腿与自动处置全部停止,解除后按补腿口径判一次);判定(无活动链接、裸量超过补腿阈值且高于尘埃线、跳过 `manual_hold` 的 cell)归 [hft-exec](../hft-exec/README.md),调用点归 [hft-sequencer 模式与 cap 的驱动](../hft-sequencer/README.md#模式与-cap-的驱动)。非 HARD 的 `Resume{Kill}`、其他锁存 cap 的解除与 `Recovery` 的解除都不产出它:非 HARD 的锁存期间风险递减单照常放行(第 1 条),补腿本就在进行;放行后的补腿与外来成交默认处理按执行层的"放行后新窗口"处理。
7. **残余低于最小下单量**:已成交裸量高于尘埃线却低于交易所最小下单量时,补腿、自动有界平仓与人工 `HedgeFix` 都下不了单(hft-core 装配时校验尘埃线不低于交易腿最小名义,使单腿上不出现这种残余;两腿 lot 零头或价格变化仍可能产生)。此时由 Admin 的 `WriteOffResidual{account, group}` 核销残余(sequencer 执行;适用于静止的 group,`Broken` 或非 `Broken`:OMS 把多出那一腿的已成交裸量移出该 group 并清零该 group 全部腿的余数,ledger 以 `AdjustDelta::WriteOff` 把该量移到 unassigned,规则归 [hft-oms](../hft-oms/README.md#残余核销)),处于 `Broken` 的 group 随后由 `on_legs` 按第 3 条解除;`HedgeFailure` 照常需要人工 `Resume{HedgeFailure}`。

### 有效模式

`effective` 读缓存;缓存在 `apply()` 内对受影响 cell 重算:

1. 期望 `Shadow`:cell flat(净仓、在途、LegTracker 残余全为 0)→ `Shadow`;否则 `CloseOnly{deferred_shadow: true}`,`CellFlat{flat: true}` 到达时落到 Shadow(两次变化都经 `ModeOutcome.cells` 报告,sequencer 据此写推迟与落定的注记)。
2. 否则 `level = max(覆盖该 cell 的 cap 的 level)`(scope 通配:`strategy`/`venue`/`account`/`inst` 为 0,`group` 为 `GROUP_ALL`;哨兵值归 [hft-types](../hft-types/README.md),`GROUP_NONE` 不是通配);**venue 级 cap**(`Kill{Venue}`、`Recon`、`UnknownTimeout`、`AccountCfg`、`Delisting`)不进 cell 计算,在检查链 #3 按意图(及链接腿)的 venue/inst 生效;`level ≥ CloseOnly` → `CloseOnly`,否则期望值。
3. `RunMode` = Shadow / CloseOnly / Live;Live 永远遵守参数里的 `open_allowed`。模式下策略意图的处理表归 [hft-strategy-sdk](../hft-strategy-sdk/README.md)。

`SetMode` 只写期望值、不清 cap;cap 的增删不改期望值。

### cap 记录

```rust
#[repr(u8)] pub enum CapLevel { None = 0, CloseOnly = 1, Hard = 3 }   // 派生量,不存储、不上线(2 不使用)
pub fn level_of(r: &CapRecord) -> CapLevel;    // Kill + HARD → Hard;其余 → CloseOnly
```

除 HARD 外,**所有 cap 只拦 Open**(含 `ClockBad` 与 `Halted`):Reduce / Flatten / Hedge 照常出环。

| `CapSource` | 作用域 | 设置 | 解除(`Resume` 只删完全匹配的一条) |
|---|---|---|---|
| `Kill`(+ `cflags.HARD`) | Global / Account / Venue(× 账户)(`params[0]`:`CANCEL_RESTING = 1`、`FLATTEN = 2`) | `Kill` 命令(含 hft-core 启动时由停机标记产生的 `cli:systemd` Kill,[hft-sequencer 命令路径](../hft-sequencer/README.md#命令路径));`hard` **允许与 Global 或 Account 组合**;`AccountConfigReport` 报 UID 不符(闸门放行前或放行后)→ 自动 `Kill{Account, hard}` + 内部 Drain(`IdentityMismatch`) | 同级同目标 `Resume{Kill}`(带 `flatten` 的记录被删即结束[清仓锁](#清仓锁flatten_lock));带 HARD 的另需 [HARD 解除条件](#对账闸门与-hard-解除的严格条件),删除时产出 `resolve_broken`(断腿第 6 条) |
| `Draining` | 进程(`params[0]` = 该 Drain 的 `cancel_resting`,0/1) | `Drain` / `ReplaceRefdata` / SIGTERM / Storage 危急 / 身份不符 | 不可清除 |
| `ClockBad` | 全局,条件型 | `Clock{ok: false}` | `Clock{ok: true}` 自动删除 |
| `DailyLoss` | 账户(`params[0]` = 越线时的触发线,`Notional` 原始值按 i64 存) | `on_pnl` 越线 | `Resume{DailyLoss}`,每 UTC 日至多 `daily_loss_max_resumes` 次 |
| `Recon` | (账户, venue) | 同一差异连续两轮 | [对账恢复条件](#对账分级全系统唯一一次) |
| `UnknownTimeout` | (账户, venue) | OMS Unknown 持续 > `unknown_alarm_ms`:OMS 写 P0 级 `Note{OmsUnknown}`,sequencer 见到它时以 `Auto(CapRecord)` 设置([hft-sequencer 模式与 cap 的驱动](../hft-sequencer/README.md#模式与-cap-的驱动)) | `Resume{UnknownTimeout}`,前置:该 (account, venue) 无 Unknown 槽(`ModeView::unknown_count = 0`),否则 `ModeErr::UnknownPending` |
| `Broken` | (账户, group) | 链接 `Broken`(第二腿超 `hedge_deadline_ms`(只按引擎运行时间)、重试耗尽、`RESUME_HOLD` 超时,判定归 hft-oms);`HEDGE_LIMIT` 拒绝;执行层 `BrokenReq`;HARD 解除时 hft-exec 判定的 group(`resolve_broken`) | 条件型自动删除:[断腿](#断腿broken自动有界平仓l3-与-flatten)第 3 条(已静止且已成交裸量 ≤ 尘埃线;含 `WriteOffResidual` 核销之后);或 `Resume{Broken}`(前置同一条件,否则 `PreconditionFailed{NakedNotCleared}`) |
| `HedgeFailure` | 账户 | 该账户任一 `Broken` 被设置时同时设置 | 只有人工 `Resume{HedgeFailure, account}`,前置:该账户已无 `Broken` cap,否则 `ModeErr::BrokenRemaining`(sequencer 回 `CmdResult{Rejected, detail = BrokenRemaining}`);`Broken` 被自动解除时它不变;残余卡在最小下单量以下时先 `WriteOffResidual` |
| `MarginStress` | 账户,条件型 | [保证金分档](#保证金分档marginstress) L1 及以上 | 档位回落到 L0 自动删除 |
| `Storage` | 全局,条件型(`params[0]`/`params[1]` = `PersistLag`/`DiskLow` 当前档位) | `Storage{level ≥ 1}`(`Timer{PersistLag\|DiskLow}`) | `Timer{StorageOk}` 自动删除 |
| `Delisting` | (venue, inst)(`CmdScope.inst = 0` = 整个 venue;`params[0]` = 下架时刻 `at_ns`) | `VenueNotice{Delisting}` 命令 | `Resume{Delisting, scope}`(公告撤回);inst 退出 refdata 后在 `restore` 时自动删除 |
| `AccountCfg` | (账户, venue),条件型 | 闸门放行后 `AccountConfigReport` 报配置不符(`mismatches` 除 `UID` 外任一位,含 `AUTO_BORROW`、`UNREADABLE`),或交易中的永续 inst 实际杠杆未知或超过 `max_perp_leverage_x100`;`params[0]` = 原因位(`MISMATCH = 1`、`LEVERAGE = 2`,本模块定义) | 该 (account, venue) 两项都不再不符时自动删除 |
| `Recovery` | 账户 | 冷启动 | 启动闸门已放行时,满足[干净 Drain 重启](#干净-drain-重启自动解除-recovery)或只含 Sim 账户实例的条件则自动解除;否则人工 `Resume{Recovery}` |
| `Reorg` | (账户, group) | 终局前重组超时(链上腿,optional) | 同 `Broken` |
| `Halted` | (strategy, 账户, group)(`params[0]` = 策略 `request_halt` 给的 reason) | 策略执行连续失败(`Ctx::request_halt`) | `Resume{Halted}` |

- **合并**:对同一 (source, scope) 再次设置取更严——`cflags`、`cancel_resting`、`flatten` 按位或,`params` 按来源取更严值(DailyLoss 取更高触发线;MarginStress 取更高档;AccountCfg 原因位按位或)。合并使记录有变化时按一次设置处理(`CapSet` 注记与 `RiskEvent::CapSet` 带合并后的记录,`set_seq` 不变);没有变化时本次 apply 无任何输出。`MarginStress` 的档位下降与 venue 位图变化同样按一次设置处理(档位按保证金分档的规则,不取更严)。
- **`cflags` 与 `level`**:`HARD` 只用于 `Kill`;`CONDITIONAL` 用于 `ClockBad`、`MarginStress`、`Storage`、`AccountCfg`;`NON_CLEARABLE` 用于 `Draining`;其余来源为 0。`level`:`Kill` 为 `KillLevel`,`MarginStress` 为账户档位 1..3,其余 0。
- **`ctl_state_version`**:一次 apply 只要改变了 ModeTable 或 cap 集合(含合并使记录变化)就恰好 +1,不论改了几条;闸门变化、`CellFlat` 推迟落定与无变化的输入不改变它。
- 每次设置 `Broken`/`HedgeFailure`、`MarginStress` 档位变化、以及影响某账户 cell 的 cap 增删,`ModeOutcome.risk_events` 产出 `RiskEvent::{HedgeBroken{group}, CapSet, CapCleared}`,由 sequencer 投递给该账户绑定的策略实例;作用域不限定账户的 cap 对作用域内每个账户各产出一项(全局 cap:全部账户;venue 级 cap:`AccountMeta.venues` 含该 venue 的账户),venue 级 cap 同样产出(它们按 venue/inst 挡该账户的 Open)。
- 每条 cap 的增删另在 `ModeOutcome.notes` 产出一条 `CapSet`/`CapCleared` 注记。**实际级别**(本模块唯一定义;hft-event 登记 Info 为缺省):`CapCleared`、命令设置的 cap(`Kill`、`Delisting`)、`Recovery`、`Draining` 的 `CapSet` 为 Info;运行中自动设置的其余 cap 的 `CapSet` 为 P1(已另有专门 P0 注记的情形——`HEDGE_LIMIT`、身份不符、L3、自动平仓失败、重组超时——由那条注记给 P0)。告警路由归 [watchtower](../../apps/watchtower/README.md)。
- `Resume{source}` 可接受的来源(`CapSource::resumable()`):Kill、DailyLoss、Recon、UnknownTimeout、Broken、Reorg、Recovery、Halted、HedgeFailure、Delisting;`Draining`、`ClockBad`、`MarginStress`、`Storage`、`AccountCfg` 不能被命令清除(条件消失自动删除),对它们的 `Resume` → `ModeErr::NotResumable`。

### 干净 Drain 重启自动解除 Recovery

冷启动时若 ① 二进制与 schema 版本与上一段相同、② journal 末尾是 `Drain` 写出的快照且其后无 Input、③ 该快照中**全部账户**都无 Unknown 槽、无 `Broken`/`HedgeFailure` cap、无未结束的 LegLink、无未到达的 `reason ∈ {MarginStress, Hard}` 引擎目标(恢复后 hft-exec `engine_reduce_pending()` 为假;Drain 超时写出的快照可能不满足)、④ 该账户**未**被标记 `TAIL_LOST`、`RECOVER_MISMATCH`、`POISON`、`FROM_SNAPSHOT`(`ModeInput::RecoveryFlags`,由 sequencer 在恢复时给出;①–③ 的条件只在本节定义,由 sequencer 对整个实例判定一次——任一账户不满足则所有账户都不置 `CLEAN_DRAIN`——以 `CLEAN_DRAIN` 位给出),则该账户 `CleanRestart = true`;`Recovery{account}` 在该账户启动闸门放行时由 `apply(Gate…)` 自动删除并产出 `CapCleared`(auto)注记。是否启用由 `auto_resume_after_clean_drain` 决定(ECF1 段 5,字段与默认值归 [hft-sequencer](../hft-sequencer/README.md#环与容量))。任一条件不满足 → 需人工 `Resume{Recovery}`。

**只含 Sim 账户的实例**(本实例 refdata 的全部账户都是 `AccountMeta.kind = Sim`,即 hft-core 的 `sim` 实例;实例划分归 [apps/hft-core](../../apps/hft-core/README.md)):干净重启照上面的规则;非干净重启(上面 ①–③ 不满足,或带 `TAIL_LOST`、`FROM_SNAPSHOT`)时,该账户只要**未**被标记 `RECOVER_MISMATCH` 与 `POISON`,`Recovery` 同样在启动闸门放行时自动删除(开关同为 `auto_resume_after_clean_drain`,关闭时一律保留且不写下述注记);否则保留,并在放行的同一 `apply` 中写 `Note{RecoveryHeldSim}`(P1,`v[0]` = account,`v[1]` = recovery_flags),等人工 `Resume{Recovery}`。

自动解除只删 `Recovery`:停机前由 kill-switch 写下停机标记时,hft-core 启动后提交的 `Kill` 是普通 Kill cap,照常挡 Open,直到交易员 `Resume{Kill}`。

### 启动闸门

`Gates` 每账户 `gate_bits`,只由 `apply(Gate…)` 推进;全部置位 → `released[account] = true`,锁存到下一个 epoch。**每次运行的复位**:`apply(RecoveryFlags{account, …})`(恢复结束时每账户一次,结果进 Recovery 快照;从快照起步的回放不复位)把该账户的 `gate_bits` 与各 venue 位图清零(模拟账户的三个位随即置位)、`released = false`、kill 自检记录清除、各 venue 的对账组记录(节 17 `ReconGate` 行)清空、`us_up_seq` 清 0。venue 位图反映最近一次输入(条件变假即清该 venue 位);`released` 一旦为真,本次运行内不再变假。`released = false` 时,`apply` 对该账户的升级输入(`SetMode → Live`、`Resume`)返回 `ModeErr::GateNotReleased`;`account = *` 的升级要求全部目标账户已放行。`Kill`(含带 `flatten` 的)从不因闸门未放行或 Draining 被 `apply` 拒绝;`Kill{Account, flatten}` 是升级类,它的确认与 Draining 例外由 sequencer 判定([hft-sequencer 命令路径](../hft-sequencer/README.md#命令路径))。每次 `apply` 中某账户的 `gate_bits` 或 `released` 有变化 → 列入 `ModeOutcome.gate_changes`,由 sequencer 写启动闸门注记。

| 位(`gate_bits` 中的值) | 条件 |
|---|---|
| `FEEDS`(1) | 该账户各 group 所需行情簿 Live |
| `ORDER_CONNS`(2) | 该账户每个有 gw 的 venue 在 `conns_ok_venues`(主下单连接已登录且用户流 Up)中,且在 `standby_ok_venues`(热备 Up)中——`allow_missing_standby`(ECF1 段 5)为真时不要求热备(按 venue 位图累计) |
| `NO_UNKNOWN`(4) | 该账户无 Unknown 槽 |
| `RECONCILED`(8) | 该账户**每个有 gw 的 venue** 各有一组 `EpochStart` 之后的对账报告满足[严格条件](#对账闸门与-hard-解除的严格条件),按 venue 位图累计;`Funding` 条目不参与 |
| `CLOCK`(16) | 时钟可信(`Gate(Clock{ok})`,对全部账户) |
| `ACCOUNT_CFG`(32) | 每个有 gw 的 venue 在 `cfg_ok_venues` 中——最近一条 `AccountConfigReport` 的持仓模式、保证金模式、多资产模式与 `VenueCaps.required_position_mode` / `required_margin_mode` 一致,现货账户借币/自动借币关闭(`AUTO_BORROW`)(gw 给出的 `mismatches` 与本模块按 refdata 复算的结果取并集,除 `UID` 外均为空,即 `FEE_RULE`、`ACCT_TYPE`、`KEY_PERM`、`UNREADABLE` 同样算不符;位定义归 hft-event)——**且**该 venue 交易中的每个永续 inst 实际杠杆**已知**且不超过 `max_perp_leverage_x100`(见[永续杠杆上限](#永续杠杆上限));按 venue 位图累计 |
| `IDENTITY`(64) | 每个有 gw 的 venue 的 `AccountConfigReport` 带 UID 核验结果、`uid_keys` 覆盖该 venue 用到的每把交易 key(Ed25519 会话 key 与 HMAC REST key 分别核对;`uid_keys ⊇ keys_required`)且与 `AccountMeta.venue_uid` 一致(`UID`、`UNREADABLE` 都未置位;未核验或未覆盖视为不一致,但只有 `UID` 置位且 `UNREADABLE` 未置位才按"UID 不符"处理),**且**收到该账户的 Input `Timer{Sys, KillSelfTestOk}`(hft-core 启动时的 kill 位自检通过;Meta 的 `Note{KillSelfTest}` 只供人读,本模块不读);按 venue 位图累计。账户元数据 `uid_policy = Unverifiable`(只允许 `is_prod = false`,规则归 [hft-refdata](../hft-refdata/README.md)):不要求 UID 核验结果,该 venue 收到任一条 `AccountConfigReport`(gw 经签名只读请求取得,可带 `UNREADABLE`)且 kill 自检通过即置位,置位的同一 `apply` 写 `Note{IdentityUnverified}`(P1,`v[0]` = venue \| account << 8) |

**对冲闸门子集**(续接崩溃前在途的对冲用):`hedge_gate_ok(a, v)` = 该账户 kill 自检已通过 ∧ `v` 在 `identity_ok_venues`(UID 一致)∧ `cfg_ok_venues`(只看模式与借币,**不含杠杆**)∧ `conns_ok_venues`(主下单连接与用户流 Up,**不要求热备**)中;不要求 `FEEDS`、`RECONCILED`、`NO_UNKNOWN`、杠杆与整个账户放行(该链接自身有无 Unknown 由 OMS 判定)。续接只是把已成交第一腿的对冲补完,杠杆超限与缺热备不应挡住它。每次 `apply` 中某 (account, venue) 由假变真 → 列入 `ModeOutcome.hedge_gate_open`;OMS 只对处于 `RESUME_HOLD` 的链接动作,重复列出无害。超时规则(`resume_hold_max_ms`)归 [hft-oms](../hft-oms/README.md)。

**恢复期按 venue 放行**(执行层补腿、残余修剪、外来成交默认处理与自动有界平仓的开始条件,按多出那一腿所在 venue 判定):`venue_gate_ok(a, v)` = `hedge_gate_ok(a, v)` ∧ `v ∈ reconciled_venues`。每次 `apply` 中某 (account, venue) 由假变真 → 列入 `ModeOutcome.venue_gate_open`,sequencer 调 `exec.on_venue_gate_open`(执行层每个 venue 每次运行只放行一次,重复列出无害);账户放行(`released` → `on_gate_open`)蕴含全部 venue。另一 venue 断线或某项差异待处置时,健康 venue 上多出的一腿照样开始处置。放行之前 `broken_auto_fix_ms` 与外来成交截止不计时、账户未放行而有裸量时的 P0 告警(`gate_naked_alarm_ms`),都归 [hft-exec](../hft-exec/README.md)。

模拟账户(`AccountMeta.kind = Sim`):`RECONCILED`、`ACCOUNT_CFG`、`IDENTITY` 不适用,启动时(闸门复位时)直接置位,`reconciled_venues`、`cfg_ok_venues`、`identity_ok_venues` 置为该账户全部 venue,kill 自检视为已通过(模拟账户没有 kill 字位),所以其 `hedge_gate_ok`/`venue_gate_ok` 只看 sim-gw 的连接(reconciler 与交易所核验都跳过模拟账户);其余位照常由模拟网关(sim-gw,[hft-sim](../hft-sim/README.md))的连接状态推进(sim-gw 没有热备,`standby_up` 恒为真)。模拟账户的单不经 gw、不占 kill 字位;**kill 与实盘相同经检查链生效**(#1 HARD、#3 cap),cap、有效模式与闸门规则与实盘完全一致,本模块不为模拟账户另设分支。`sim` 实例([apps/hft-core](../../apps/hft-core/README.md))里全部账户都是 Sim,规则不变。

**身份与配置不符**:

- **UID 不符**(`UID` 位置位且未置 `UNREADABLE`;闸门放行前或放行后同一处理):该 venue 的 `IDENTITY` 位清零(放行前保持 0)、`Auto(Kill{Account, hard})`(key 与账户配反时,账户级 HARD 连风险递减单也拦)、`ModeOutcome.drain = Some(IdentityMismatch{account, venue})`:sequencer 写 P0 `Note{IdentityMismatch}` 并发起内部 Drain,结束于 `Done{cause: IdentityMismatch}`;放行前的启动检查与退出码 2 归 [apps/hft-core 启动顺序](../../apps/hft-core/README.md)。只是查不到(`UNREADABLE`、超时)→ 闸门保持关闭、P1,不 Drain(`uid_policy = Unverifiable` 的账户按上表 `IDENTITY` 行放行)。
- **配置不符**(放行后):`Auto(AccountCfg{account, venue})`,产出 `CapSet`;放行前只是 `ACCOUNT_CFG` 不置位(经 `gate_changes` 报告)。

### 对账闸门与 HARD 解除的严格条件

对某 (account, venue),一组完整对账报告(同一 `report_id`)满足严格条件,当且仅当:

1. `Summary` 未截断(`sflags` 无 `TRUNCATED`),`skipped_inflight = 0`,该组报告的窗口内无 UserStream Down(`sflags` 无 `USERSTREAM_DOWN`);且
2. 全部条目(`Funding` 条目除外;余额条目是 reconciler 按资金费修正后的差额,见[对账分级](#对账分级全系统唯一一次))`Ignore`;**或**每个非 `Ignore` 条目都已被处置——`AdoptExchange` 采纳(仓位或余额条目,引用本 `report_id`)、`SetBalanceBaseline` 登记为基线(余额条目)、带 `AUTO_EXTERNAL` 的仓位条目经 sequencer 复核后自动入账(命令语义归 [control-plane](../../docs/subsystems/control-plane.md),自动入账规则归 [hft-sequencer](../hft-sequencer/README.md));孤儿挂单等其他条目先由运维处理(如 `CancelAll`),等下一组报告变为 Ignore;处置以 `GateEvent::Disposed` 进入,`ReconItemKey` = (item, inst | asset)。

`Summary` 条目的 `sflags`、`skipped_inflight` 字段布局归 [hft-event](../hft-event/README.md),语义归 [reconciler](../../apps/reconciler/README.md)。处置状态按 (account, venue) 保存最近一组报告的非 Ignore 条目(上限 `recon_gate_items` = 64;超过则只能等下一组全部 Ignore)。新一组报告到达时以新组为准(旧组的处置作废)。

- `RECONCILED` 位按上述条件逐 venue 置位(看闸门复位之后到达的最近一组;新组不满足即清该 venue 位)。
- **HARD 解除**(`Resume{Kill}` 删除带 HARD 的 Global 或 Account 记录):对受影响的每个账户、每个有 gw 的 venue,该 venue 最近一组报告(`ReconGateRowHdr.view_seq`)晚于该 cap 的 `set_seq` 且满足同一严格条件;否则 `ModeErr::ReconNotClean`。模拟账户不参与这一条件。
- `PREVIEW` 对 `Resume{Kill}`/`Resume{Recovery}` 列出受影响账户各 venue 尚未处置的条目数、各条目的 `ReconItemKey` 与上轮差额(`ReconRow.last_diff`)。

### kill 字(唯一的跨线程共享可变量)

每次 cap 增删后,从剩余的 `Kill` 记录重新计算每个 gw 的 `KillBits`(位常量在 [hft-types](../hft-types/README.md):`GLOBAL`、`VENUE`、`HARD`、`account(ix)`、`account_hard(ix)`,`ix` = refdata `venue_account_ix`),不做增量位操作;`ModeOutcome.kill_changed = true` 时由 sequencer 的 `LiveDriver` 写进各 gw 的 `KillWord`(`Release`)。账户级 HARD 置服务该账户的每个 gw 上的 `account_hard(ix)`;全局 HARD 置 `HARD`。gw 规则(在 [hft-venue](../hft-venue/README.md) 实现):`op ∈ {Place, Amend}` 时 `k = kill & (GLOBAL | VENUE | HARD | account(ix) | account_hard(ix))`;`k & (HARD | account_hard(ix))` → 拒;`k ≠ 0` 且无 `RISK_REDUCING` → 拒;撤单、查询、维护类永远放行。一个交易所(子)账户只由一个策略实例使用,所以账户级 `CancelAll` 与账户级死手总是可用。

### Kill

```rust
pub struct KillCmd { pub level: KillLevel, pub scope: CmdScope, pub cancel_resting: bool, pub flatten: bool, pub hard: bool }
```

| 级别 | 效果([09 §4.1](../../design/09-可观测性、风控与控制面.md)) | kill 字位 |
|---|---|---|
| Venue(× 账户) | `Kill` cap;`cancel_resting`(默认 true)→ 撤 Open 挂单(`ModeCancel{open_only: true}`);`hard` 不允许(`ModeErr::InvalidScope`) | 该 gw 的 `VENUE` 位或指定账户位 |
| Account | `Kill` cap;撤 Open 挂单;`flatten` 时由执行层默认减仓平仓(含 `Broken` 的 group 与该账户 `GROUP_NONE` 的永续持仓,永续腿先)并进入[清仓锁](#清仓锁flatten_lock);`hard` → 该账户 HARD(只影响该账户;撤单 `open_only = false`) | 所有服务该账户的 gw 的 `account(ix)`;`hard` 另置 `account_hard(ix)` |
| Global | `Kill` cap + 撤挂单;`hard` → `cflags.HARD`(撤单 `open_only = false`);`flatten` 不允许(`InvalidScope`) | 所有 gw 的 `GLOBAL`;`hard` 另置 `HARD` |

`flatten` 与 `hard` 不能同时置位(`ModeErr::InvalidScope`):HARD 拦全部引擎单(#1),清仓无法执行;要先停一切再清仓,先发 `Kill{hard}`、确认后 `Resume` 掉 HARD 再发 `Kill{flatten}`。

老板的紧急停止 `/stop` 就是非 HARD、不带 `flatten` 的 `Kill{Global | Account}`:禁开、撤 Open 挂单;已成交第一腿的对冲与平仓照常(风险递减单不被 #3 拦)。其确认绑定预览的 `cmd_hash`(不比对 `ctl_state_version`),规则在 [hft-sequencer 命令路径](../hft-sequencer/README.md#命令路径)。

#### 清仓锁(`flatten_lock`)

`Kill{Account, flatten}` 生效后该账户处于清仓锁,**直到删除该条记录的 `Resume{Kill}`**:锁 = 覆盖该账户的 `Kill` cap 带 `flatten` 位(合并规则按位或,同一目标上后续不带 `flatten` 的 Kill 不会去掉它;`Resume` 只删完全匹配的一条)。`RiskStateView.flatten_lock` 读同一状态。

- 锁内该账户的订单意图只放行 `Reduce`/`Flatten` 类,以及风险递减 `Hedge` 类(含经组净量验证的策略无链接 Hedge、OMS 的第二腿、`SecondRetry`、`Reverse`,执行层的 `HedgeFix`——它们只减少裸敞口,拦下会留下裸腿);Open 一律 `KILLED`(检查链 #3,与 Kill cap 相同,不另设拒绝位)。策略无链接 Hedge 同样必须满足[分类](#分类)的组可对冲量约束。
- "只接受比当前更接近 0 的目标"在目标层由 [hft-exec](../hft-exec/README.md) 判定(`engine_flatten` 置执行层的同名锁,策略新目标不能覆盖引擎目标 0);`Resume{Kill}` 删除该记录时产出 `CapCleared{Kill}`,sequencer 经 `exec.on_risk` 交执行层解除。
- 锁不阻止 L3 `ForceDeleverage`、自动有界平仓与 `HedgeFix` 命令。

带 `cancel_resting` 的 Kill 与 `cancel_resting = true` 的 `Draining`(cap 表 `params[0]`)的撤单是**持续义务**:生效期间 sequencer 在每次 ConnState Up 或 Unknown 收敛为 Live 时重发撤单(`open_only` 与最初的撤单相同)。解除不触发自动 HedgeFix:删除带 HARD 的 Kill 记录时 `ModeOutcome.resolve_broken = true`,sequencer 按[断腿](#断腿broken自动有界平仓l3-与-flatten)第 6 条设 `Broken`(之后按断腿规则处置)。

### 保证金分档(MarginStress)

按**到强平价的距离**分档,原比率(维持/保证金余额)只作交叉校验。数据来源:`MarginReport`(gw 按 `VenueCaps.margin_data` 推送或轮询,Input)由 [hft-ledger](../hft-ledger/README.md) 保存;ledger 按当前 mark 给出两种强平距离、估计比率与最旧标记价时刻(公式唯一定义在 [hft-ledger 保证金数据](../hft-ledger/README.md#保证金数据)),sequencer 投影为 `MarginSample`。本模块对每个有永续市场的 (account, venue):

1. **度量**:`liq_dist = min(rep_liq_dist, est_liq_dist)`——交易所给的 `liq_px` 与 ledger 估计取更保守者(某持仓的报告未给 `liq_px` 时该持仓只用估计);无永续持仓 → `Rate::MAX`。
2. **交叉校验**:`r = max(reported_ratio, est_ratio)`;`r ≥ margin_ratio_crosscheck` → 至少 L2。
3. **数据过期或不完整 → 至少 L1**:无报告;报告早于本次运行起点(`report_ens < run_start_ens`,即重启后尚未收到新报告);`now.engine_ns − report_ens > margin_stale_ms`;报告带 `ITEMS_TRUNCATED`;**标记价断流**:有永续持仓或永续在途订单且 `mark_ens < run_start_ens` 或 `now.engine_ns − mark_ens > mark_stale_ms`(估计强平距离依赖的 mark 不再更新)。停止到达的数据由 `Timer{Risk, MarginStale}` 判定:截止取 `min(report_ens + margin_stale_ms, mark_ens + mark_stale_ms) + 1 ns`(过期按严格大于判定:`now.engine_ns = report_ens + margin_stale_ms` 时未过期,再晚 1 ns 即过期;标记价同理),最后一个永续持仓与在途订单结束后由 sequencer 刷新空仓样本,不再沿用旧仓 mark;保证金报告自身的完整性与新鲜度仍按本条检查。到期时按本模块存下的最近一份 `MarginSample` 复核,未过期则按新截止重新登记(每 (account, venue) 至多一个在堆);定时器早到(`now.engine_ns` < 截止,重启后 TSC 换算的取整差)时只按剩余时长重新登记、不改档位。

| 档 | 条件(任一满足) | 动作 |
|---|---|---|
| L0 | 以下都不满足 | 无 |
| L1 | `liq_dist ≤ liq_dist_block_open`;数据过期或不完整 | `MarginStress{account}` cap(条件型):挡该账户全部 Open(`CLOSE_ONLY`) |
| L2 | `liq_dist ≤ liq_dist_reduce`;`r ≥ margin_ratio_crosscheck` | 同 L1;另向该账户绑定的策略实例投递 `RiskEvent::MarginStress{venue, level: L2, liq_dist, ratio, deadline_tsc}`(截止存 `deadline_ens = now.engine_ns + deleverage_deadline_ms`,事件与视图给 `now.tsc_of(deadline_ens)`),登记 `Timer{Risk, MarginDeadline}`;P1 |
| L3 | `liq_dist ≤ liq_dist_force`;或 L2 截止已过仍处于 L2 | 同 L2(事件 `level: L3`);输出 `ForceDeleverage{account, venue, frac: force_step}` 交 [hft-exec](../hft-exec/README.md)(永续先;覆盖该 venue 上全部有永续仓位的 cell,**含 `Broken` 的**,以及 `GROUP_NONE` 的永续持仓),登记 `Timer{Risk, MarginForce}`,每 `force_step_interval_ms` 到期仍为 L3 则再输出一步;P0 |

- **滞回**:上行即时;下行需数据未过期且 `liq_dist > 该档阈值 + liq_dist_hyst`(比率触发的 L2 另需 `r < margin_ratio_crosscheck − margin_ratio_hyst`);因截止过期进入的 L3 在 `liq_dist > liq_dist_reduce + liq_dist_hyst` 之前不降档。下行逐档判定:从当前档起,满足离开该档的条件才降一档,直到某档不满足或到达按上行条件算出的档位为止(例:L3 时 `liq_dist` 回到 0.18,默认阈值下降到 L1)。`MarginStress` cap 不能被命令清除,回到 L0 时自动删除。
- **进入 L2**(从下方上行或从 L3 下行)时设 `deadline_ens = now.engine_ns + deleverage_deadline_ms` 并登记 `MarginDeadline`;离开 L2/L3 到 L1 以下时截止作废(到期的定时器按档位复核后无动作)。`MarginDeadline` 到期或 `on_margin` 时若处于 L2 且 `now.engine_ns ≥ deadline_ens` → L3(`DEADLINE_L3`)。进入 L3 时立即输出一步 `ForceDeleverage` 并登记 `MarginForce`(`next_force_ens = now.engine_ns + force_step_interval_ms`)。
- venue 档位变化且新档位为 L2 或 L3 时写一条 `Note{MarginLevel}`(`v` 的布局归 hft-event)。
- 账户档位 = 其各 venue 档位的最大值;cap 记录 `level` = 账户档位(1..3),`params[0]` = 处于 L1 以上的 venue 位图。某 venue 档位升降时产出该 venue 的 `RiskEvent::MarginStress`(回到 L0 时 `level` 为 `None`);账户回到 L0 时删除 cap,产出 `RiskEvent::CapCleared{MarginStress}`。`RiskStateView.margin_level`(各 venue 最大)、`liq_dist`(各 venue 最小)、`margin_ratio`、`deadline_tsc` 读同一状态。
- 只对该账户在该 venue 有永续市场的 (account, venue) 判定(纯现货 venue 不产生保证金报告)。
- 判定只读 Input 推出的 `engine_ns` 与已记录的报告,确定可回放。重启后档位、`deadline_ens`、下一步时刻保持快照值(引擎时间连续、停机不计);过期规则保证本次运行收到新报告与新标记价之前至少 L1。`Timer{Risk, …}` 到期时以 `now.engine_ns` 与本 crate 存的截止比较:未到(重启时定时器堆按 TSC 平移的取整差)→ 按剩余时长重新登记,不提前动作。
- **重启后的 L3 出步要本次运行的数据确认**:进入 L3(含重启后因截止过期进入)或 `MarginForce` 到期时,若该 (account, venue) 的 `report_ens` 与 `mark_ens` 都早于 `run_start_ens`(L3 尚未被本次运行的任何报告或标记价确认),只记档位,不输出 `ForceDeleverage`,也不登记下一步;此后第一次 `on_margin` 带来本次运行的新报告或新标记价时按当时数据重判,仍为 L3 则立即输出一步并重新登记 `MarginForce`。这一步确认不等待另一腿的连接;启动时各所连接的先后对串行退路(另一腿暂不可用)的影响归 [hft-exec](../hft-exec/README.md)。
- **为什么按距离**:比率 `r = m(1+X)/(e−X)` 从禁开到强平只留约 0.67·m 的价格空间(e = 0.30、m = 0.5% 时约 0.42%)。按距离分档的示例(单个永续空头,保证金余额 / 名义 e = 0.30,维持保证金率 m = 0.5%,价格单边上涨 y;`est_liq_dist = (0.295 − 1.005·y) / (1.005·(1 + y))`):L1 在 +3.5%、L2 在 +12.5%、L3 在 +17.6%、强平在 +29.35%;L3 之后每 5 s 减仓 20%,单边 +30%/1 h 的压力下有约 12 个百分点的价格空间完成减仓——这是 [hft-sim](../hft-sim/README.md) 的压力用例,上线门槛 ⑦ 的零强平要求见 [testing](../../docs/testing.md#8-上线门槛)。e 越小 L1 来得越早(e = 0.30 时 +3.5% 即禁开);交易所杠杆的设置步骤见 [runbooks/launch](../../docs/runbooks/launch.md),杠杆上限的机检见下节。
- 默认值(**待标定**,模拟账户长跑期间按实测调整;正式实盘的起步值由运营在 `RiskParams` 设定):`liq_dist_block_open` 0.25、`liq_dist_reduce` 0.15、`liq_dist_force` 0.10、`liq_dist_hyst` 0.02、`margin_ratio_crosscheck` 0.80、`margin_ratio_hyst` 0.05、`deleverage_deadline_ms` 30 000、`force_step` 0.20、`force_step_interval_ms` 5 000、`margin_stale_ms` = 3 × 该 venue 的轮询间隔(`Push` 型取 15 000;账户有多个永续 venue 时取其中最大值)、`mark_stale_ms` 10 000。参数校验:`block_open > reduce > force > 0`、`liq_dist_hyst < reduce − force`。watchtower 的保证金 P0 规则读同一组 cap 与注记([watchtower](../../apps/watchtower/README.md))。

### 永续杠杆上限

保证金分档的安全余量取决于交易所账户上的实际杠杆(运营设置,不在 refdata)。本模块机检它:

1. **交易中的永续 inst**:`on_groups_changed` 记下该账户任一 group 的腿引用的永续 inst(按 venue 分组;命令路径,O(该账户的腿数))。
2. **判定**:对每个 (account, 永续 venue),`lev_bad = ∃` 交易中的永续 inst `i`:`leverage(a, v, i)` **未知**(从未报告、未列出、`leverage = 0`、`LEV_TRUNCATED` 截掉),或已知且 `> max_perp_leverage_x100 / 100` 倍。`on_account_cfg` 与 `on_groups_changed` 之后重判。
3. **动作**:`lev_bad` 为真时该 (account, venue) 按 `ACCOUNT_CFG` 不符处理(mode 在 `apply(Gate(AccountCfg))` 与 `apply(GroupsChanged)` 时经 `ModeView::lev_bad` 读取)——放行前该 venue 的 `ACCOUNT_CFG` 不满足(`cfg_ok_venues` 只记模式与借币,不受杠杆影响,所以对冲闸门子集不受它挡),放行后 `Auto(AccountCfg{account, venue})`(`params[0]` 含 `LEVERAGE`),P1。**作用域是整个 (account, venue)**,不是单个 inst:杠杆由运营按 inst 设置,一个交易中的 inst 未知或超限说明该账户的配置流程出了问题,先禁开该 venue 再人工核对;单腿永续 Open(不经 #7b)因此同样被挡。运营把交易所杠杆调回上限以内、下一条 `AccountConfigReport` 给出全部交易中 inst 的已知杠杆后自动解除。
4. 策略经 SDK `Views::leverage(a, i)` 读同一张杠杆表(百分之一倍、向下取整,换算由 sequencer 做,[hft-strategy-sdk](../hft-strategy-sdk/README.md))。

### 当日亏损(锁存)

- **亏损口径**:ledger 增量维护 `day_pnl(account)`(已实现 + 资金费 + 手续费 + 未实现,USDT)。
- **权益来源**:[hft-ledger](../hft-ledger/README.md) 的账户合计权益(`EquityView`,即 SDK `PortfolioView` 的合计权益,估值规则为 ledger 参数)。日切时取 `equity_sod = equity(account).total`(`valid` 时);日切时尚未 `valid` 的账户,在当日首次 `valid` 的 `on_pnl` 中补取。
- **额度**:`quota = min(daily_loss_abs, equity_sod × daily_loss_kill)`;`equity_sod` 尚未取得时(新账户首个余额到达之前)`quota = daily_loss_abs`。`daily_loss_abs` 必须 > 0(参数校验)。
- **触发线**:初值 `−quota`;`day_pnl < trigger` → `on_pnl` 返回 `Auto(DailyLoss)`(`params[0]` = 触发线),锁存;锁存期间 `on_pnl` 返回 `None`;熔断不改 cell。当日首次补取 `equity_sod` 时,未锁存且当日未解除过的账户触发线随新额度重设为 `−quota`。
- **日切**:快照存 `last_day_rolled`(归 hft-sequencer);`on_day_roll(day)` 生效时只把 PnL 基线归零、重取 `equity_sod`、未锁存账户的触发线重设为 `−quota`、`resumes_today = 0`,**不清锁存**。
- **解除**:`Resume{DailyLoss}` 删除 cap 并把触发线设为当前 `day_pnl − quota`;同一 UTC 日内已解除 `daily_loss_max_resumes`(默认 1)次后再请求 → `ModeErr::ResumeLimit`(经 `ModeView::daily_loss_resumable`)。删除 cap 的 apply 在 `ModeOutcome.daily_loss_resumed` 列出该账户,sequencer 同一 Input 内调 `on_daily_loss_resumed(a, day_pnl)`:触发线 = `day_pnl − quota`、`resumes_today` + 1、锁存标记清除;此后 `on_pnl` 按新触发线判定。

### 对账分级(全系统唯一一次)

reconciler 只报差异,不分级。`on_reconcile` 只接受一组完整报告:

1. **在途判定**:报告的视图点 `S0`(`view_seq`)时该 inst 有非终态槽,或 `(S0 − user_stream_lag, S1]` 内该 inst/资产有任何执行事件 → 本组该条目记 `skipped_inflight`,不分级。
2. **资金费修正**:余额条目里结算资产的差额**已由 reconciler 用同一组报告的 `Funding` 条目修正**(其 `engine_qty` 含这些条目的差额;规则唯一归属 [reconciler](../../apps/reconciler/README.md)),这部分差额由 ledger 自动校正入账([hft-ledger 资金费对账校正](../hft-ledger/README.md#资金费对账校正)),不进 `Recon` cap 与启动闸门。本模块按报告值分级,**不再修正第二次**。
3. 每 (account, venue, inst) 仓位条目与每 (account, venue, asset) 余额条目(现货库存按资产):`< lot`(余额:`<` 该资产的 `AssetMeta.dust`,资产级尘埃线的唯一来源在 [hft-refdata](../hft-refdata/README.md#数据结构))→ Ignore;否则 → **Warn**(单轮差异只给 Warn;条数在 `ReconVerdicts.n_warn`,明细就是该 `ReconcileReport` 记录);**同一差异(同键、同符号、差额 `exch_qty − engine_qty` 的变化 < lot,余额条目 < `AssetMeta.dust`)连续两轮** 且名义(条目的 \|`diff`\|,reconciler 填)≥ `recon_closeonly_notional` → `Auto(Recon{account, venue})`。带 `AUTO_EXTERNAL` 的仓位条目照同一规则分级(reconciler 只在同一差异再次出现时置这个位,所以它可能设 `Recon`);sequencer 自动入账后以 `Disposition::AutoExternal` 报告处置,只影响闸门与 HARD 解除的严格条件,`Recon` cap 仍按第 4 条恢复。
4. **对账恢复条件(唯一定义)**:同一 (account, venue) **连续两轮 Ignore**,期间该 venue 无 UserStream Down,且这两轮 `skipped_inflight = 0`;有 `skipped_inflight > 0` 的轮次不计入也不清零。满足后 `Resume{Recon, account, venue}` 才被 `apply` 接受(经 `ModeView::recon_resumable`;否则 `ModeErr::ReconNotClean`)。`on_user_stream_down(a, v)` 与报告组带 `USERSTREAM_DOWN` 都把该 (account, venue) 的连续 Ignore 轮数清零。
5. 每组带 `Summary` 的报告同时产出闸门输入(`ReconVerdicts.gate = Some(ReconGateInput)`:该 (account, venue)、`report_id`、`view_seq`、非 Ignore 条目键表(至多 64 个,超出置溢出标志)、截断、在途(`skipped_inflight > 0`)与 `USERSTREAM_DOWN` 标志)。
6. **`Funding` 条目(交易所资金费与引擎入账之差,含 `inst = INST_NONE` 的钱包条目)本身不分级**:不进 `Recon` 计数、不触发 `Recon` cap、不进 `ReconGateInput`(不影响 `RECONCILED` 与 HARD 解除),"连续两轮 Ignore"也不看它们;余额差中属于它们的部分已由 reconciler 扣除(第 2 步)。处置是 [hft-ledger 资金费对账校正](../hft-ledger/README.md#资金费对账校正)的自动入账(含 `NoHistory` 条目在窗口结束后的入账);告警归 watchtower(`|差额| > max(1 USDT, 1% × |交易所金额|)`(`funding_diff_alert`)→ P1,[watchtower](../../apps/watchtower/README.md))。
7. **`AUTO_EXTERNAL` 的用户流前置条件**(`auto_external_ok(a, v, sflags)`;sequencer 复核时在同组 `on_reconcile` 之前调用,reconciler 置位时查同一条件,[reconciler](../../apps/reconciler/README.md)):本组 `Summary.sflags` 无 `USERSTREAM_DOWN`,该 (account, venue) 上一组完整报告也无(`PREV_US_DOWN` 未置),且用户流自最近一次 Up 起已稳定 ≥ 1 个对账周期——最近一次 Up 的 seq `us_up_seq ≠ 0` 且早于上一组报告的 `view_seq`(两组报告的视图点之间用户流一直 Up)。`us_up_seq` 在 `Gate(OrderConns{primary_up})` 由假变真时记为该 Input 的 seq、变假时与闸门的每次运行复位时清 0(以 `primary_up` 计,它含用户流;`set_run_start` 不动它,所以从快照起步与连续运行结果相同);`on_reconcile` 末尾把本组的 `USERSTREAM_DOWN` 与 `view_seq` 记为下一组的"上一组"。三项进节 17 的 `UserStream` 行。
8. **不含 `Summary` 的组**(reconciler 只做 Unknown 收敛时提交,组内只有 `UnknownResolved` 条目,[reconciler](../../apps/reconciler/README.md)):不是一轮对账。`on_reconcile` 不分级、返回 `ReconVerdicts{recon: None, gate: None, n_warn: 0}`;不计入也不清零第 4 条的连续 Ignore 轮数,不改第 3 条"同一差异"的记录,不更新第 7 条的"上一组"(`PREV_US_DOWN`、`view_seq`);因此不产生闸门输入,`RECONCILED` 与 HARD 解除的严格条件只看带 `Summary` 的组。其中的 `UnknownResolved` 条目由 OMS 应用(sequencer 照常交 `oms.on_reconcile`)。

### 时钟

`Clock{ok: false}` 设条件型 `ClockBad` cap(只拦 Open,任何来源的风险递减单照常);`Clock{ok: true}` 自动删除。

### 令牌桶

每 (venue, account):`tokens_10s`、`tokens_min`,容量 = `RateLimitTable` 限额 × (1 − `rate_headroom`);按 `now.engine_ns − last_ens` 惰性补充(速率 = 容量 / 窗口长度,不超过容量)。每 (account, inst) 的秒级桶(`inst_bucket_per_s`)只限制非风险递减单;Reverse、HedgeFix、裸腿修复与 Kill flatten 等已分类的风险递减单不检查、不扣此桶。每笔 `commit` 仍从两个 venue 窗口各扣 1 个令牌;普通单另检查并扣 inst 桶。`rate_ready_at(intent, now)` 返回所有适用桶补足一个令牌且退避结束的最早 `engine_ns`(向上取整,含链接 venue);OMS/Exec 在该时刻重试,期间不按行情重复尝试。发送侧交易所硬窗口仍由 [hft-venue](../hft-venue/README.md#4-gwruntimepoll_once-的循环结构) 最终拦截。**风险递减预留**:非风险递减单只能在两个窗口的剩余令牌都 > 容量 × `rr_reserve` 时消耗;风险递减单可消耗到 0。`on_rate_limit_hit` → 该 (venue, account) 的两个窗口桶清零 + 指数退避(退避期内两类都拒,`RATE_BACKOFF`):第 k 次(k 从 0 计)退避时长 = min(`backoff_base_ms` × 2^k, `backoff_max_ms`);命中时距上次退避结束不足 `backoff_max_ms`(含仍在退避期内)则 k = 上次 k + 1,否则 k = 0。桶与退避存 `engine_ns`:重启、换机后原样有效,停机期间不补充(交易所窗口按钟表时间早已恢复,所以只会偏保守,至多一个窗口)。模拟账户的 (venue, account) 用所绑定 venue 的同一组限额(模拟与实盘行为一致)。

### 存储降级(Storage)

housekeeping 测得 persist 滞后与冷盘剩余空间,实盘驱动在档位变化时写 `Timer{Sys, PersistLag}` / `Timer{Sys, DiskLow}`(档位 1 = 禁开阈值、2 = Drain 阈值,编进 `timer_id`;两项都回到阈值以下时写 `Timer{Sys, StorageOk}`;阈值归 [apps/hft-core](../../apps/hft-core/README.md))。`Storage{kind, level}`:`level ≥ 1` → 全局条件型 `Storage` cap(只拦 Open;`params` 记两项各自的当前档位);任一项档位为 2 → `ModeOutcome.drain = Some(StorageCritical)`,sequencer 发起内部 Drain;`StorageOk` → 删除 cap。**推迟**:`ModeView::engine_reduce_pending()` 为真(sequencer 转调 hft-exec `engine_reduce_pending()`:存在未到达的 `reason ∈ {MarginStress, Hard}` 引擎目标,口径与 `drain_ready_acct` 中该项相同;不按清仓锁或 `MarginStress` 档位是否仍在判定——清仓锁要等 `Resume{Kill}` 才解除)时不产出 `drain`,`drain_deferred()` 为真;sequencer 在推迟期间每条 Input 的执行层推进之后调一次 `apply(StorageRecheck)`。推迟结束的产出点唯一:此后**第一个**满足"档位仍为 2、没有 `Draining` cap、`engine_reduce_pending()` 为假"的 `apply` 产出一次 `drain`——目标在某条 Input 的执行层推进中到达时,就是该 Input 内随后的 `StorageRecheck`(该 Input 内在它之前没有其他 `apply` 时)。推迟期间 `Storage` cap 照常禁开,L3 与清仓继续;硬阈值是 journal 本身,写满时按 [hft-sequencer journal 满](../hft-sequencer/README.md#journal-满) abort。

### 下架与公告(VenueNotice)

`Notice(VenueNoticeCmd)`:`Delisting{at_ns}` → `Delisting` cap(venue + inst;`inst = 0` 为整个 venue),该 inst 的 Open 在 #3 被拒(`inst_blocked`),同一 apply 的 `cancels` 恰有一项撤该 (venue, inst) 上的 Open 挂单(`open_only = true`);它是 venue 级 cap,cell 的有效模式不变(策略经 `RiskEvent::CapSet` 与 sequencer 投递的 `EventRef::Notice` 得知);`FundingInterval`、`Maintenance` 不设 cap,只由 sequencer 作为 `EventRef::Notice` 投递给订阅该 inst 的策略。

### 配置(`RiskParams`,版本化,经 `SetRisk` 替换并作为 `ConfigBlob` 写入 journal)

```rust
pub struct RiskParams {
    pub version: u64, pub generated_at_ns: i64, pub author: [u8; 16],
    pub rate_headroom: Rate,                         // 限速余量比例的唯一位置
    pub conn_liveness_ms: u32,                       // 报价健康的连接静默上限(#5、LINK_STALE、OMS price_hedge、hft-exec 共用;唯一配置位置)
    pub max_perp_leverage_x100: u32,                 // 交易中的永续 inst 实际杠杆上限(百分之一倍;默认 300 = 3 倍,待标定),见"永续杠杆上限"
    pub l3_max_cross: Rate,                          // 可选的更宽 L3 越价上限:origin = Exec 的 Flatten 类单取 max(该 inst rr_max_cross, 它);默认 0 = 不启用
    pub risk_class: PreVec<InstClassRow>,            // inst → 价格带类别;容量 = Capacities.max_insts
    pub accounts: PreVec<AcctLimits>,                // 容量 = refdata 账户数
    pub venues:   PreVec<VenueLimits>,               // 每 (venue, account);容量 = refdata 的 (account, venue) 数
    pub classes:  FixedVec<PxBand, 8>,               // 全局字段
    pub groups:   PreVec<GroupLimitsRow>,            // 每 (account, group);容量 = Capacities.max_cells_total
}
pub struct AcctLimits {
    pub account: AccountId, pub _pad: [u8; 7],
    pub max_order_notional: Notional, pub default_group: GroupLimits,
    pub max_account_notional: Notional, pub account_notional_from_equity: Rate,  // 账户名义上限 = min(绝对值, equity × 比例);EquityView.valid = false 时只用绝对值
    pub max_hedge_notional: Notional,                // Hedge 类单笔上限(第二腿、重试、Reverse、HedgeFix)
    pub max_reduce_notional: Notional,               // Reduce / Flatten 单笔上限(执行层默认减仓切片以它封顶)
    pub daily_loss_kill: Rate, pub daily_loss_abs: Notional, pub daily_loss_max_resumes: u8,
    pub recon_closeonly_notional: Notional, pub unknown_blocks_open: bool,
    pub funds_buffer: Rate,                          // 第二腿可负担性的余量(检查链 #7b)
    pub margin: MarginLimits,
}
pub struct MarginLimits {                            // 默认值见"保证金分档",全部待标定
    pub liq_dist_block_open: Rate, pub liq_dist_reduce: Rate, pub liq_dist_force: Rate, pub liq_dist_hyst: Rate,
    pub margin_ratio_crosscheck: Rate, pub margin_ratio_hyst: Rate,    // 比率交叉校验:r ≥ margin_ratio_crosscheck → 至少 L2
    pub deleverage_deadline_ms: u32, pub force_step: Rate, pub force_step_interval_ms: u32, pub margin_stale_ms: u32,
    pub mark_stale_ms: u32,                          // 持仓 inst 的标记价断流上限(数据过期 → L1)
}
pub struct GroupLimitsRow { pub account: AccountId, pub _pad: [u8; 3], pub group: GroupId, pub l: GroupLimits }
pub struct GroupLimits { pub max_group_notional: Notional, pub max_pos_qty: Qty, pub naked_gate: Rate, pub naked_gate_min: Notional }
pub struct VenueLimits { pub venue: VenueId, pub account: AccountId, pub max_venue_notional: Notional, pub inst_bucket_per_s: u32,
                         pub backoff_base_ms: u32, pub backoff_max_ms: u32, pub rr_reserve: Rate }   // 窗口限额来自 refdata RateLimitTable
pub struct PxBand { pub max_px_dev: Rate, pub ioc_max_cross: Rate, pub rr_max_cross: Rate }
// 永续腿的实际杠杆不在参数里:#7b 用 AccountConfigReport 读到的值(`Risk::leverage`);参数只给上限 max_perp_leverage_x100。
```

- 默认值(待标定):`conn_liveness_ms` 2 000(≥ 2 × hft-venue `bbo_ping_ms`);`max_perp_leverage_x100` 300;`RiskParams.l3_max_cross` 0(不启用;是否启用与取值在上线签字时决定,见 [runbooks/launch](../../docs/runbooks/launch.md));`rr_reserve` 0.20;`rr_max_cross` = 2 × `ioc_max_cross`;`max_hedge_notional` = 2 × `max_order_notional`;`max_reduce_notional` = `max_order_notional`;`daily_loss_max_resumes` 1;`funds_buffer` 0.05;`unknown_blocks_open` true(有 Unknown 的 group 禁开,检查链 #4)。正式实盘的初始取值与第一次放宽的条件写在上线签字记录里([runbooks/launch](../../docs/runbooks/launch.md))。
- **容量**:`RiskParams` 的各向量在 `Risk::new` 时按 refdata 的账户数、(account, venue) 数与 `Capacities`(`max_insts`、`max_cells_total`)一次分配;`SetRisk` 的行数超出 → `CapacityExceeded`。refdata 不热替换,所以账户数在进程生命周期内不变。
- `conn_liveness_ms`、各类别的 `PxBand` 与 `l3_max_cross` 的**唯一配置位置**是本结构;OMS 的对冲定价不另设阈值,由 sequencer 调 `oms::price_hedge` 时传入 `hedge_px_limits()` 的值(同值同源),hft-exec 与策略经 `RiskStateView.conn_liveness_ms` 与 `InstRiskView`(`view_for`)读同一组值。
- 账户作用域:`SetRisk` 只替换该账户的字段,作用域外有改动 → `InvalidArgs{OutOfScope}`(sequencer 校验);全局字段(含 `l3_max_cross`、`classes`)只能 `account = *`。放宽判定 `loosens()` 按作用域内逐字段,任一字段按下列方向变化即为放宽(反方向与不变不算):**变大**——全部 `max_*`(含 `max_perp_leverage_x100`)、`account_notional_from_equity`、`daily_loss_abs`、`daily_loss_kill`、`daily_loss_max_resumes`、`recon_closeonly_notional`、`naked_gate`、`naked_gate_min`、`inst_bucket_per_s`、`PxBand` 的三项、`l3_max_cross`(含 0 → 非 0)、`conn_liveness_ms`、`margin_ratio_crosscheck`、`deleverage_deadline_ms`、`force_step_interval_ms`、`margin_stale_ms`、`mark_stale_ms`;**变小**——`rate_headroom`、`rr_reserve`、`funds_buffer`、`backoff_base_ms`、`backoff_max_ms`、`liq_dist_block_open`、`liq_dist_reduce`、`liq_dist_force`、`liq_dist_hyst`、`margin_ratio_hyst`、`force_step`;`unknown_blocks_open` 由真变假;`risk_class` 的改动按改后每个 inst 的 `PxBand` 逐项比较。
- 参数来源:`RiskParams` 是静态配置,由 `hft-cfg` 从 TOML 生成 blob,运维以 `cli:<login>`(Admin)经 ctl 发 `SetRisk` 并二次确认;Telegram 不发 `SetRisk`。策略参数是 `RiskParams` 之下的东西,策略越不过这里的上限。


**RKP1 配置编码**由 `RiskParams::encode_blob` / `decode_blob` 唯一提供,只在冷路径分配。头部依次为 magic `HFTRKP01`、`version:u64`、`generated_at_ns:i64`、`author:[u8;16]`、`rate_headroom:i64`、`conn_liveness_ms:u64`、`max_perp_leverage_x100:u64`、`l3_max_cross:i64`,共 72 B。随后依次写 `classes`、`risk_class`、`accounts`、`venues`、`groups`;每表先写 `count:u64`,再按上列 Rust 结构的字段顺序展开行,嵌套结构原位展开。所有整数槽宽 8 B、小端,有符号定点用 i64;窄整数以 u64 写出,解码时检查范围;bool 只接受 0/1。五种行分别为 24、16、216、56、48 B,不写容器容量和填充字节。

`classes` 保持类别下标顺序;其余表分别按 inst、account、(account, venue)、(account, group) 严格递增,重复或乱序拒绝。解码在分配前按剩余字节核对 count,拒绝截短、尾随字节、未知格式、越界整数及缺失/未知账户、venue、inst 引用。每个 refdata 账户及其 venue 都必须有显式限额行。参数值的风控语义、容量和 `ExecRiskMismatch` 仍由 `Risk::new` / `set_params` 检查。完整内容随 `ConfigBlob` 进入 journal,不以全局变量、线程局部值或外部配置文件补齐。

`RiskParams::inspect_blob` 供离线 `show`/`diff` 读取同一格式,执行格式与行顺序检查,不检查 refdata 引用,不能替代加载入口。`decode_blob` 复用该解析器并完成引用检查。TOML 映射与差异预览归 [hft-cfg](../../tools/README.md#hft-cfg)。

### 引擎单与执行配置的一致性(`ExecRiskMismatch`)

执行层的默认减仓、补腿与自动有界平仓用 `ExecConfig.force_slice` 切片;它必须落在风控上限之内,否则这些单会被 #5/#6 反复拒绝。每个账户 `a` 要求:

1. `ExecBounds(a).force_max_cross ≤` 该账户可交易 inst(refdata 中该账户各 venue 的 inst)所属价格带类别的 `rr_max_cross` 最小值(在 `new` 与 `set_params` 时求出;这是配置的一致性下界,不是运行中的越价上限——运行中按 inst 取值,见检查链 #5);
2. `ExecBounds(a).force_max_chunk ≤ AcctLimits(a).max_order_notional`。

不满足:hft-core 装配时 `Risk::new` 失败 → 退出 2(`ExecRiskMismatch`,[apps/hft-core](../../apps/hft-core/README.md));运行中 `SetRisk` → `RiskParamError::ExecRiskMismatch`,sequencer 回 `InvalidArgs{ExecRiskMismatch}`,旧参数继续生效(`ExecConfig` 只随 Drain + 重启变化,同样在装配时校验)。策略 `PairParams.slice` 的 `max_cross` / `max_chunk_notional` 不在此校验:hft-exec 执行时按 `InstRiskView` 与 `max_order_notional` 截取([hft-exec](../hft-exec/README.md))。

### 快照行格式(节 4、17、18、23;草案,实现时以代码与编译期断言为准)

分节表归 [journal-format](../../docs/subsystems/journal-format.md#快照记录结构)。`snapshot(&self, w)` 一次写完节 4、14、17、18、23(各组件的写出顺序归 [hft-sequencer 快照](../hft-sequencer/README.md#快照))。

- **节 4 `RiskState`**:每条生效 cap 一行,行 = hft-event `CapRecord`(48 B)原样;键 `(source, scope, params[0])`。kill 位不另存,`restore` 后由 cap 集合重算。
- **节 17 `RiskCounters`**(行版本 4):

```rust
#[repr(C)] pub struct RiskRowHdr { pub kind: u8 /* Acct=1 Bucket=2 Recon=3 ReconGate=6 Lev=7 UserStream=8;4、5 保留 */, pub account: AccountId, pub venue: VenueId, pub _pad: u8, pub inst: InstId } // 8 B
#[repr(C)] pub struct RiskAcctRow { pub day_pnl_base: Notional, pub loss_trigger: Notional, pub loss_quota: Notional, pub equity_sod: Notional,
                                    pub resumes_today: u8, pub eq_flags: u8 /* SOD_VALID=1 */, pub _pad: [u8; 6] }                      // 40 B
#[repr(C)] pub struct RiskBucketRow { pub tokens_10s: i64, pub tokens_min: i64, pub last_ens: i64, pub backoff_until_ens: i64,
                                     pub backoff_k: u32, pub _pad: u32 }                                                                  // 40 B;时刻为 engine_ns
#[repr(C)] pub struct RiskLevRow { pub leverage: Rate }                                                                                  // 8 B;键 (account, venue, inst);AccountConfigReport 读到的实际杠杆,只写已知的
#[repr(C)] pub struct ReconRow { pub ignore_streak: u8, pub warn_streak: u8, pub _pad: [u8; 6], pub last_diff: Qty }                       // 16 B;键 (account, venue, inst|asset)
#[repr(C)] pub struct ReconGateRowHdr { pub report_id: u64, pub disposed_mask: u64, pub n_items: u16, pub flags: u16 /* TRUNCATED=1 INFLIGHT=2 USERSTREAM_DOWN=4 OVERFLOW=8(非 Ignore 条目 > 64) */, pub _pad: u32,
                                        pub view_seq: u64 /* 该组报告的 view_seq(HARD 解除比对 set_seq) */ } // 32 B;其后 n_items(≤ 64)个 ReconItemKey(8 B)
#[repr(C)] pub struct RiskUsRow { pub us_up_seq: u64, pub prev_view_seq: u64, pub uflags: u8 /* PREV_US_DOWN=1 */, pub _pad: [u8; 7] } // 24 B;键 (account, venue);快照中没有这一行时按全零恢复(AUTO_EXTERNAL 前置不满足,直到用户流 Up 并经过一组报告)
// ReconRow:inst = 0 的行是 (account, venue) 级——ignore_streak = 连续 Ignore 轮数(skipped_inflight > 0 的轮不计,UserStream Down 清零);
// inst ≠ 0 的行是逐键——warn_streak = 同一差异连续出现的轮数,last_diff 为上轮差异。余额条目以资产编码进 inst 字段的高位标志(ASSET_KEY)。
// last_day_rolled 不在本节(归 hft-sequencer 节 20 SeqDay);ctl_state_version 不在本节(归节 18 Gates)。
// "交易中的永续 inst"不进快照:恢复后由 sequencer 按生效的 StrategyParams 调 on_groups_changed 重建(lev_bad 随之重算)。
```

- **节 23 `MarginState`**:每 (account, venue) 一行,键 (account, venue):

```rust
#[repr(C)] pub struct MarginRow {                                   // 行版本 4,96 B;时刻全部为 engine_ns
    pub account: AccountId, pub venue: VenueId, pub level: u8 /* 0..3 = L0..L3 */,
    pub mflags: u8 /* TRUNCATED=1 RATIO_L2=2(比率触发 L2)DEADLINE_L3=4(截止过期进入 L3) HAS_REPORT=8(收到过 MarginReport) */, pub _pad: u32,                    // 0..8
    pub report_seq: u64, pub report_ens: i64, pub reported_ratio: Rate, pub est_ratio: Rate,                                    // 8..40
    pub rep_liq_dist: Rate, pub est_liq_dist: Rate,                                                                              // 40..56
    pub l2_since_ens: i64, pub deadline_ens: i64, pub next_force_ens: i64,                                                       // 56..80
    pub mark_ens: i64,               // 最近一份 MarginSample 的 mark_ens(MarginStale 截止与重启后 L3 确认用;i64::MAX = 无永续持仓)    // 80..88
    pub stale_due_ens: i64,          // 堆中 `Timer{Risk, MarginStale}` 的截止(0 = 不在堆);存下而不是按样本重算,恢复后与不中断时同一截止(回放逐字节一致)    // 88..96
}
// AccountId、VenueId 各 u8(hft-types);每个结构旁有 const _: () = assert!(size_of::<T>() == N)。
```

- **节 18 `Gates`** 与 **节 14 `ModeTable`**:由本模块写;行格式唯一定义在 [journal-format 快照记录结构](../../docs/subsystems/journal-format.md#快照记录结构)。本模块要求 `GateRow` 能表达:`gate_bits`(含 `ACCOUNT_CFG`、`IDENTITY`)、`reconciled_venues`、`cfg_ok_venues`(模式与借币一致的 venue,不含杠杆)、`identity_ok_venues`、`conns_ok_venues`(主下单连接已登录且用户流 Up 的 venue,对冲闸门子集用)、`standby_ok_venues`(热备 Up 的 venue,`ORDER_CONNS` 用)位图、`recovery_flags`(`TAIL_LOST | RECOVER_MISMATCH | POISON | FROM_SNAPSHOT`)、`released`、`clean_restart`、`ctl_state_version`。

### 规划源文件

| 文件 | 职责 |
|---|---|
| `src/lib.rs` | `Risk`、`evaluate`、`commit`、`on_legs`、`view`、`view_for` |
| `src/classify.rs` | `classify` 与分类表 |
| `src/chain.rs` | 检查链(含链接腿、名义口径、冒烟单的口径、Hedge/Flatten 只扣即时在途) |
| `src/mode/{mod,table,caps,effective,gates,kill,recon_gate}.rs` | `Mode::apply` 与其子状态(含对冲闸门子集、恢复期按 venue 放行 `venue_gate_ok`、`gate_changes`、撤单的 `open_only`、`resolve_broken` 只在 HARD 解除、`StorageCritical` 的推迟、只含 Sim 账户实例的 Recovery 解除) |
| `src/bucket.rs` | 令牌桶、`rr_reserve`、退避 |
| `src/margin.rs` | 按强平距离分档、比率交叉校验、过期判定(含标记价断流)、重启后 L3 确认、定时器、`ForceDeleverage` |
| `src/leverage.rs` | 杠杆表、交易中的永续 inst、永续杠杆上限(含未知) |
| `src/pnl_gate.rs` | 日亏锁存与权益额度、敞口门、对账分级与恢复计数、`AUTO_EXTERNAL` 的用户流前置条件 |
| `src/blob.rs` | RKP1 完整配置的冷路径编解码 |
| `src/params.rs` | 校验、加载、账户作用域替换、`loosens`、`ExecRiskMismatch` |
| `benches/risk.rs`、`tests/*.rs` | 基准与测试,用例编号与函数名见 [TESTS.md](TESTS.md) |

</details>

## 延迟与资源契约

分段 `decide` 内的风控份额,预算 200–500 ns/意图(上限表见 [benchmarks](../../benchmarks/README.md#单-crate-基准上限表))。

通过路径的基准先同步 mock 连接活跃时刻并验证输入可放行;每次采样推进引擎时间以补充令牌,更新连接活跃时刻,断言单腿或双腿评估成功后执行相应 `commit`。任何拒绝都使基准失败。

| 入口 | p50 | p99 | 基准名 |
|---|---|---|---|
| `classify` | ≤ 15 ns | ≤ 40 ns | `risk/classify` |
| `evaluate` 全部通过(单腿 Open)+ `commit` | ≤ 200 ns | ≤ 500 ns | `risk/check_pass_open` |
| `evaluate` 全部通过(链接 Open,两腿 venue,含第二腿报价健康与可负担性)+ `commit` | ≤ 280 ns | ≤ 500 ns | `risk/check_pass_linked_open` |
| `evaluate_pair` 两腿 + 两次 `commit`(预算,待参考机实测) | ≤ 560 ns | ≤ 1000 ns | `risk/check_pair` |
| `evaluate` 第 1 层即拒(HARD) | ≤ 30 ns | ≤ 80 ns | `risk/check_reject_kill` |
| `evaluate` + `commit` Hedge 类 | ≤ 150 ns | ≤ 400 ns | `risk/check_pass_hedge` |
| `mode.apply`(cap 增删,含 kill 投影,256 条) | ≤ 2 µs | ≤ 5 µs | `risk/mode_apply` |
| `Risk::check_mode`(Resume Kill Account 预览,只读) | ≤ 2 µs | ≤ 5 µs | `risk/check_mode` |
| `mode.effective` | ≤ 5 ns | ≤ 15 ns | `risk/effective` |
| `on_pnl` | ≤ 20 ns | ≤ 60 ns | `risk/on_pnl` |
| `on_legs` | ≤ 15 ns | ≤ 40 ns | `risk/on_legs` |
| `on_margin`(档位不变) | ≤ 20 ns | ≤ 60 ns | `risk/on_margin` |
| `hedge_px_limits` / `leverage` / `view_for` | ≤ 5 ns | ≤ 15 ns | `risk/hedge_px_limits`、`risk/leverage`、`risk/view_for` |
| `on_account_cfg`(≤ 1 024 条杠杆,含杠杆上限重判;不在 tick-to-trade 路径) | ≤ 5 µs | ≤ 15 µs | `risk/on_account_cfg` |

事件入口零分配(容量在 `new` 时一次分配);RKP1 编解码只在加载/回放装配的冷路径分配;无系统调用;无原子量(kill 字由驱动写);所有算术为整数(`Rate`/`Notional` 定点)。

## 测试与验收

用例与验收见 [TESTS.md](TESTS.md)。

## 依赖

- 内部:`hft-types`(`RunMode`、`KillLevel`、`CapSource`、`KillBits`、`RejectBits`、`HedgePxLimits`)、`hft-refdata`(`RateLimitTable`、`AccountMeta`、`VenueCaps`)、`hft-event`(`CapRecord`、`ReconcileReport`、`AccountConfigReportHdr`/`LeverageItem`、`Note`、`CmdScope`、`OrderFlags`)、`hft-strategy-sdk`(只用类型:`IntentClass`、`LinkRole`、`Tif`、`RiskEvent`、`RiskStateView`、`InstRiskView`、`TargetReason`、`Now`、`BboView` 与其 `quote_live`、`GroupTable`)。**不依赖** `hft-rt`、`hft-oms`、`hft-ledger`、`hft-exec`、任何 `strategies/*`。
- 外部:`bitflags`。
- 禁止:任何 I/O、`std::time`、原子量、f64 于检查路径。

## 延伸阅读

- [round8 裁决](../../design/_work/round8-decisions.md)(R8-3 风控边界、R8-4 账户与资金、R8-8 断腿自动处置与不做同所现货 × 永续、R8-10 引擎兜底的风险取舍、R8-11);评审证据 [review-2026-10-02](../../design/_work/review-2026-10-02-主策略适配与执行正确性.md) §2.2、§3,[review-2026-10-03](../../design/_work/review-2026-10-03-M0独立评审.md) P0-2、P0-3、P1-5,[M0-b 评审](../../design/_work/review-2026-10-03-M0b独立评审.md) P0-1、P0-3、P1-1、P1-3、P1-7,[M0-c 五路汇总](../../design/_work/review-2026-10-03-M0c五路汇总.md)(M-P0-2、M-P1-1、M-P1-5、D9、D10-c′),[M0-d 五路汇总](../../design/_work/review-2026-10-03-M0d五路汇总.md)(M-2 #7 在途口径、§4-13 L3 越价按 inst 与 `l3_max_cross`、M-P2、W3 P1-12/18),[M0-e 五路汇总](../../design/_work/review-2026-10-03-M0e五路汇总.md)(X1 P1-A/B/C/E、X3 P1-2/3、X4 N-P1-7 与 P2-1)
- Agent Note:[resolve_broken 只在 HARD 解除时](../../.agents/notes/proposed/architecture/2026-10-03-resolve-broken-hard-only.md)、[AUTO_EXTERNAL 前置条件](../../.agents/notes/proposed/architecture/2026-10-03-auto-external-guard.md)、[测试网 UID 不可核验](../../.agents/notes/proposed/process/2026-10-03-testnet-uid-unverifiable.md)、[dev2 根级批处理 slice](../../.agents/notes/proposed/process/2026-10-03-dev2-root-batch-slice.md)、[保证金按到强平距离分档](../../.agents/notes/proposed/architecture/2026-10-03-margin-liq-distance.md)、[报价新鲜度 = 连接健康](../../.agents/notes/proposed/architecture/2026-10-03-quote-freshness-conn-health.md)、[引擎时间 engine_ns](../../.agents/notes/proposed/architecture/2026-10-03-engine-time.md)
- [control-plane 统一优先级表](../../docs/subsystems/control-plane.md#模式与上限的统一优先级)、[05 §3](../../design/05-CEX执行层.md)、[09 §4](../../design/09-可观测性、风控与控制面.md)
- [hft-oms](../hft-oms/README.md)、[hft-ledger](../hft-ledger/README.md)、[hft-exec](../hft-exec/README.md)、[hft-sequencer](../hft-sequencer/README.md)、[hft-strategy-sdk](../hft-strategy-sdk/README.md)、[apps/reconciler](../../apps/reconciler/README.md)

## 已知限制与后续工作

- 自成交未检查。可选扩展:`SELF_CROSS` 位(触发条件:启用挂单类执行算法 `MakerFirst` 或同一账户在同一 inst 同时挂双边单)。
- 桶只按"单数"。可选扩展:按请求权重计的桶(触发条件:接入按权重限速下单的 venue,或权重限速在 core venue 上实际触发)。
- 链接 Open 对链接 venue 的令牌只查不扣:同一事件内多笔链接 Open 可能共同耗尽链接 venue 的非预留令牌,此时第二腿用预留令牌,预留被更快消耗。可选扩展:链接 Open 预扣(触发条件:实测出现第二腿因 `RATE` 被拒)。
- 强平距离的 ledger 估计假设该 venue 全部永续持仓同时不利变动、维持保证金率按报告值随名义放大(只放大不缩小);多资产保证金、分级维持保证金率与交易所的强平细则不建模,所以与交易所 `liq_px` 取更保守者,并由 `margin_stale_ms`、`mark_stale_ms` 与报告频率约束误差。
- 默认阈值对高杠杆配置很早禁开(e = 0.30 时价格不利 +3.5% 即 L1);阈值、`max_perp_leverage_x100` 与交易所杠杆要在模拟账户长跑中一起标定。
- 宽价差时引擎的 L3 IOC 即使按 inst 类别的 `rr_max_cross` 定价也可能长时间吃不到;`l3_max_cross` 是为此准备的可选放宽,默认关闭,是否启用由上线签字决定。
- 杠杆的 cap 作用域是整个 (account, venue):一个交易中的 inst 杠杆未知或超限就禁开该 venue 的全部 Open,直到下一条 `AccountConfigReport`;某所的杠杆查询不完整(报告不列出全部交易中的 inst)时,该 venue 会一直禁开(各所查询是否列出全部 inst 待核实(方法:见开发备忘))。可选扩展:按 inst 的杠杆门(触发条件:运营上需要在同一 venue 内容忍个别 inst 的杠杆不符)。
- cap 容量 256 条;作用域爆炸时合并为账户级并告警。对账闸门逐项处置上限 64 条/(account, venue)。
- 第二腿可负担性(#7b)只在第一腿下单时检查,不预留资金:第一腿挂单期间(或多笔链接 Open 同时在途)第二腿 venue 的余额可能被其他单占用,此时第二腿被交易所拒 → OMS `SecondRetry` → 截止 → `Broken`,再由[自动有界平仓](#断腿broken自动有界平仓l3-与-flatten)收敛。可选扩展:链接 Open 预扣第二腿资金(触发条件:模拟或实盘中出现因余额不足导致的 `Broken`)。
- #7b 的杠杆是 `AccountConfigReport` 读到的观测值,不是要求:读不到即按资金不足拒绝链接 Open,所以某所的杠杆查询不可用时该所不能作第二腿永续(各所的查询方法待核实(方法:按各 `venue-*` 的"账户配置核验"表在测试网逐所查询))。

### 开发备忘

<details><summary>给实现者</summary>

- **植入缺陷(feature `planted`,只在测试中)**:`src/planted.rs` 按线程打开的开关:检查链 #7 不检查持仓上限(`risk-no-exposure-limit`)、保证金分档只看比率(`margin-ratio-only`)。只由 hft-testkit 的 `planted` feature 打开,生产清单、`seq` 与 `dst-bin` 都不含它([TK-178](../hft-testkit/TESTS.md#tk-178-植入缺陷不进入任何生产构建));真实引擎 DST 发现各缺陷的验收归 [TK-179](../hft-testkit/TESTS.md#tk-179-真实引擎的植入缺陷在有界种子内被不变量发现) 与 TK-053.21–TK-053.23。
- 开放问题:对账差异的"逐项确认"只经 `AdoptExchange`、`SetBalanceBaseline` 与 `AUTO_EXTERNAL` 自动入账表达,不设单独的确认命令;若 control-plane 增加,`Disposition` 追加一个变体即可。
- **待核实**:各所限速数值与限速错误码分类;各所保证金率换算(统一口径的分子分母)在 `venue-*` 核实,方法:测试网/模拟盘账户下单前后对比交易所 UI 的保证金率、强平价与 `MarginReport`;各所 `liq_px` 在单向/双向持仓与全仓下的含义(Bitget 单向持仓待核实,方法同上);各所标记价推送周期(决定 `mark_stale_ms` 的下限),方法:测试网连续 24 h 统计 `MarkPrice` 间隔分布;各所账户配置查询是否列出全部永续 inst 的杠杆(决定"杠杆未知即禁开"是否会误伤),方法:测试网账户对未持仓、未设置过杠杆的 inst 查询一次,看是否返回默认杠杆。
- 实现顺序:`classify` → `mode`(table、caps、effective、kill 投影、gates、对冲闸门子集、`gate_changes`、recon_gate)→ 检查链(含链接腿、名义口径、在途口径与冒烟单)→ 令牌桶预留 → 保证金分档(强平距离、比率交叉校验、标记价断流、重启后 L3 确认、杠杆表与杠杆上限)→ 日亏/对账/时钟/Storage → 参数、账户作用域与 `ExecRiskMismatch` → `view_for` → 快照。
</details>
