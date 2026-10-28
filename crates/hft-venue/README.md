---
description: "hft-core 的 feed/gw 任务循环体与 warm 任务 snap-fetch、acct-fetch:把 venue-* 的 VenueProtocol 实现跑在 hft-net 的连接上,负责 BBO 去重与连接健康(BBO 槽号、成员、存活探测、BBO 补帧)、分档快照深度、发送、kill 检查、切换、私有频道归属、账户查询(保证金轮询与账户配置核验,含每把 key 的 UID)、raw journal(只服务实盘账户);全量增量深度与快照重同步是可选扩展"
kind: crate
tier: hot
scope: core
status: spec
---

# hft-venue

## 概要

`hft-venue` 拥有**通用 venue 运行时**(交易所协议接口 `VenueProtocol` 在 [hft-venue-proto](../hft-venue-proto/README.md)):`FeedRuntime<P>`(行情任务循环体:BBO 独立连接双路赛马去重与连接健康、分档快照深度、成交/标记价/资金费、raw journal 单写者)、`SnapFetchTask<P>`(warm 任务 `snap-fetch` 的一部分:BBO 补帧的 REST 请求、读取与解析)、`GwRuntime<P>`(下单任务循环体:每解一帧回报就检查一次出环、kill 检查、`SendReport`、交易所字符串 id → slot 私有表、无在途才主备切换、私有频道归属、账户查询的调度)、`AcctFetchTask<P>`(warm 任务 `acct-fetch` 的一部分:按 gw 的请求执行保证金与账户配置查询并解码,结果经 `acct-<venue>` 环交回 gw),以及 warm 线程 `conn-builder` 上的 `SessionDriver<P>`(登录后交接连接)。**core 行情范围** = BBO、逐笔成交、标记价/指数价、资金费、BBO 补帧,需要深度时用分档快照流(`RouteKind::DepthTopN`,§3c);全量增量深度与 REST 快照重同步是可选扩展(规格保留在 [§3](#3-去重断裂与重同步),触发条件:某个策略需要全深度)。同一 venue 的行情在升级主机类的 `prod` 实例上拆成两个任务:`feed-bbo:<venue>` 只承载 `RouteKind::Bbo` 路由,`feed-md:<venue>` 承载其余路由;dev2/c4 与 `sim` 实例可用一个任务 `feed:<venue>` 承载全部路由(`routes = All`)。它**不**创建线程(线程、绑核、环的装配在 [apps/hft-core](../../apps/hft-core/README.md)),**不**改订单状态、簿或仓位(只在 sequencer),**不**懂某个交易所的字段(在 `venue-*`;交易所差异的通用表达是 [hft-refdata](../hft-refdata/README.md) 的 `VenueCaps`)。热路径上它覆盖 t1(帧 → 归一化事件 → `bbo`/`md` 环)与 t4(`GwCmd` → 填帧 → 掩码 → 加密一次 → `sendmsg`),见 [architecture.md §4](../../docs/architecture.md#4-一次-tick-to-trade)。所有交易所共用这一份循环([13 O2](../../design/13-设计评审与优化.md));单实例、refdata 变更即重启是当前设计。**模拟账户(`AccountKind::Sim`)不经过 gw**:它们的 `OutMsg` 由 sequencer 路由到 sim-gw([hft-sim](../hft-sim/README.md)),只共用本 crate 的行情任务(真实行情);`GwRuntime` 与 `AcctFetchTask` 只服务实盘账户。

## 目录

- [使用本组件](#使用本组件)
- [实现说明](#实现说明)
- [延迟与资源契约](#延迟与资源契约)
- [测试与验收](#测试与验收)
- [依赖](#依赖)
- [延伸阅读](#延伸阅读)
- [已知限制与后续工作](#已知限制与后续工作)

## 使用本组件

可选 [latency_trace](../../tools/latency-report/README.md) 记录协议解码、规范化事件发布、订单出环及编码边界；关联使用固定事件指纹、decision_seq 和 raw 序号到 frame 的映射，满环只丢测量点。

### 协议接口

每个 MD stream 按 `VenueProtocol::stream_route` 分组，连接的 `RouteId` 同时用于订阅归属、端点选择与 classify。不同路由不能共享同一 MD 连接；重连重发原分组的订阅。`FeedEndpoints.md_routes` 按 RouteId 指定端点，缺省沿用 `md`。

`VenueProtocol` trait 及其全部纯类型(`SeqRule`、`Align`、`RouteKind`、`StreamKind`、`MdSink`/`ReportSink`、`TargetRef`、`WallClock`、`HeartbeatSpec`、`DeadManSpec`、`RateRule`、`AcctQuery`、`AcctHint`、`Skip`、`TradeKey`…)与实现方契约定义在 [hft-venue-proto](../hft-venue-proto/README.md)。本 crate 依赖它,**不依赖任何具体 `venue-*`**;具体 venue 由 apps/hft-core 以泛型参数装进 `FeedRuntime<P>`/`SnapFetchTask<P>`/`GwRuntime<P>`/`AcctFetchTask<P>`。本 crate 实现 trait 规定的两个 sink(写入环)并按 trait 的返回值驱动去重、分档快照、发送、切换与账户查询。所有 `P::*` 热路径调用都显式传入 `rd: &RefData`(进程生命周期内不变的 `&'static RefData`;refdata 变更 = 写 ConfigBlob 后 Drain + 重启);运行时行为差异(保证金数据来源、可忽略事件、是否支持 reduceOnly、连接的流数与入向消息上限)一律读 refdata 里的生效值(`VenueCaps`、`VenueMeta`,按 `P::VENUE` 取),不读 `P::default_caps()` 与 `P::routes()` 里的默认数值。

### 出环:`OutMsg = GwCmd`

sequencer 经 `out-<venue>` 环交给 gw 的元素是 [hft-event](../hft-event/README.md#环元素类型) 定义的 `OutMsg` = `GwCmd`(64 B 定长,没有 enum 包装),按 `op` 分派:

| `op` | gw 的处理 |
|---|---|
| 订单类 `Place`/`Cancel`/`Amend`/`CancelAll`/`Query` | kill 检查 → 限速兜底 → 按 `P::order_channel(op)`:`Ws` → `P::encode` 填帧 → 掩码 → 加密一次 → `sendmsg`;`Rest` → `P::encode_rest_ctx`(可写地借该账户的 `GwCtx`;默认即 `encode_rest_signed`)生成签名请求 → 该账户的 REST 池 `submit`(记下 `RestOrigin{what: Order, req: (slot, gen, op), inst}`,响应经 §4 第 3 步解码);每条恰好一条 `SendReport`,带 `op`、`target_slot`、`target_gen`(OMS 只对 `op = Place` 的非 Written 状态迁 Rejected,其余 op 不动目标槽)。**`CancelAll` 的展开**(kill 的首选动作;两种通道同一规则,仍只回一条 `SendReport`;`Rest` 通道只承载 `inst ≠ 0` 的逐品种全撤,账户级 `inst = 0` 一律走下面的展开,如 Binance USD-M):① 以 `inst = 0`(账户级)编码;返回 `EncodeErr::Unsupported(CancelAll)` → ② 对槽镜像里该账户每个有活动订单的 inst 以该 inst 各编码一次;仍返回 `Unsupported(CancelAll)` → ③ 把槽镜像中该账户的每张活动订单展开为 `Cancel`,`P::encode_batch` 返回 `Ok` 时按批(每批条数以它接受为准),否则逐条 `encode`;③ 产生的帧不另回 `SendReport`。`P::CANCEL_ALL_SWEEP` 为真的所,`CancelAll` 发出后再以 `P::ro_request(OpenOrders)` 查一次挂单列表(`RestOrigin{what: Sweep}`),响应产出的 `QueryResult` 照常入环,由 OMS 收敛([hft-venue-proto 契约 29](../hft-venue-proto/README.md#实现方必须遵守的契约))。命令的 `account` 不是本 gw 服务的账户 → `SendReport{status: NoConn}` 并计数 `cmd_bad_account`,不编码、不发送。带 `OrderFlags::SMOKE` 的 `Place`(Admin 冒烟单,语义归 [hft-oms](../hft-oms/README.md))在 gw 侧与普通订单完全相同:同一 kill 检查、限速、编码与槽镜像 |
| `DeadManRefresh`(`account`,`arg` = timeout_ms) | 调用 `P::dead_man_refresh`(`insts` = 该账户槽镜像中有活动订单的 inst;账户级计时的所每次都刷新,与有无挂单无关),`DeadManOut.frame` 经 primary 发送、`reqs` 逐个提交该账户的 REST 池(`RestOrigin{what: DeadMan}`);失败 → `ConnState{Degraded, reason=DEADMAN}` |
| `Rotate`(`account`) | 立即开始一次计划切换(不等计划轮换时刻),满足"无在途"时 `swap_primary`(§4 第 5 步);计划轮换时刻本身按 ECF1 段 6 的规则(配置项 `conn.rotate_margin_s`) |
| `Resubscribe`(`account`) | 对该账户的用户数据流重新订阅 |
| `ResetRate`(`account`) | 清零该账户的发送侧限速计数(退避结束) |
| `SessionKeepalive`(`account`) | `P::session_keepalive_request` 返回 `Ok(true)` 时提交该账户的 REST 池(`RestOrigin{what: Keepalive}`;如 Binance listenKey 每 30 min `PUT` 续期),`Ok(false)` → 计数忽略;由 sequencer 系统定时器驱动,编号见 [hft-event](../hft-event/README.md#环元素类型)(`SessionKeepalive=11`);响应失败(非 2xx 或超时)→ `ConnState{Degraded, account, role=UserStream, reason=DEADMAN}` |
| `SetEpoch{epoch, accept_prev, prev_epoch}` | 更新 gw 用来生成与接受 client id 的 `engine_epoch`;`accept_prev` 为真时另接受 `prev_epoch` 的 id(`prev_epoch` = 恢复所依据快照节 3 的 `engine_epoch`,定义归 [hft-oms](../hft-oms/README.md#clientorderid);两次运行之间可能隔着安全间隔,gw 不按 `epoch − 1` 推算;字段在 `GwCmd` 中的编码以 hft-event 为准);`accept_prev` 为假后,`prev_epoch` 的 id 按解析失败处理 |
| `BindWire{inst, part, 16 字节}`(`account` 0) | 运行期绑定的 venue(Polymarket)下单要用 token id:sequencer 在 `BindSlot` 生效后与冷启动 `SetEpoch` 之后按段 0、1 下发([hft-event](../hft-event/README.md#环元素类型))。段 0 按 inst 暂存(至多 64 个 inst,满 → 计 `maint_ignored`);同 inst 的段 1 到达时拼成 32 字节,对本 gw 的每个账户调 `P::bind_order_wire(&mut ctx, inst, &id)`,成功计 `binds`、失败计 `bind_failed`(不支持运行期绑定的协议返回 `Unsupported`)。inst 不属于本 venue、没有段 0 的段 1、`part` > 1 → 计 `maint_ignored`。不回 `SendReport` |
| `Deliver{account, inst, payout, end_s, qty}` | 实盘账户的窗口交割(sequencer 执行 `Settle` 后发,[hft-event](../hft-event/README.md#环元素类型)):推一条 `ExternalFill{venue, account, side = Sell, cause = Delivery, inst, qty, px = payout, fee = 0, fee_asset = inst 的计价资产, exch_order_id = end_s, trade_id = 0, ex_ts_ns = end_s × 10⁹, fill_seq = qty}`(去重键 `(inst, end_s, qty)`,与 hft-sim 的交割同形)并计 `delivered`;同一 `(account, inst, end_s)` 最近 64 条内再来 → 计 `maint_ignored`,不重复报;账户不归本 gw、inst 不属本 venue、`qty ≤ 0` 或 `payout < 0` → 计 `maint_ignored`。链上兑付不在 gw 内,余额差由账户余额轮询收敛。不回 `SendReport` |
| `MirrorRestore{slot, gen, epoch, inst, exch_order_id}` | 重启恢复并 `SetEpoch` 之后,sequencer 为每个非终态槽下发一条(字段布局以 hft-event 为准):gw 写回私有槽镜像(`live = true`)与 `IdTable`(`exch_order_id ≠ 0` 时),使随后的 `Query`/`Cancel` 能用**原 epoch** 的 client id 与交易所 id 编码。不回 `SendReport`;`slot` > OMS 槽数 N(镜像放不下)或 inst 不属于本 venue → `ConnState{Degraded, account, reason=MIRROR}`,镜像不变 |

`OrderDecision` 只写 seq journal;sequencer 只把放行且非 SHADOW 的决策投影成订单类 `GwCmd`(价/量已按 tick/lot 取整、slot/gen 已分配、`oflags` 原样复制),**venue 特定的帧编码在 gw 线程完成**;sequencer 因此不依赖任何 `venue-*`。`GwCmd` 只带 `target_slot`,不带目标的 `gen` 与交易所 id:gw 维护一个按 slot 下标的私有**槽镜像**(`[SlotMirror; N + 1]`,N = `GwCfg.oms_slots`(hft-core 取该 venue 各账户 OMS 槽数的最大值);下标即槽号 1..=N,与 [hft-oms](../hft-oms/README.md) 的槽号一致,下标 0 保留不用;`{gen, epoch, inst, exch_order_id, live}`,发 New 时写、`MirrorRestore` 时写、终态回报时清),`encode` 从中取 `TargetRef`(client id 按镜像里的**原 epoch** 渲染,不是当前 epoch)。client id 由 gw 用 `hft_types::ClientOrderId{epoch = engine_epoch, venue, account, slot, gen}` 生成([hft-types](../hft-types/README.md#clientorderid-编解码);epoch 的 bit 15 按该页恒为 0)并交给 `P::render_cid(&id, &mut [u8; 48])` 包装外形;回报里的字符串经 `P::parse_cid` 解回。venue/account 与所在连接不符、或 epoch 不是当前/(`accept_prev` 时)`prev_epoch` → 按解析失败处理(规则在 [hft-oms](../hft-oms/README.md#clientorderid))。

### 运行时入口

请求响应缺少 client id 时，`GwRuntime` 只在同一连接存在唯一 `(slot, gen, op)` 在途请求、该请求的 epoch 与活动槽镜像一致、账户与 venue 相符时补齐 epoch 和缺省品种；补齐后仍执行 epoch 接受窗口。无匹配、重复及跨连接响应不能凭请求号借用槽身份，也不能消耗另一连接的在途请求。回归见 [FP-8](../../docs/tests/scenarios.md#fp-8-本机时钟漂移超过-recvwindow-时交易所以时间窗拒单并标记连接-clock_skew时钟不可信只停新开仓)。

生命周期收尾：调用 `FeedRuntime::retire_all` / `GwRuntime::retire_all` 停止连接后，以消耗运行时所有权的 `seal_and_close(self) -> Result<(), hft_journal::RollErr>` 封存对应 raw journal；调用方在任务退出后最后停止 persister。此冷路径不改变报文处理与资金语义。

```rust
/// 本任务承载哪些路由;hft-core 按任务名给出:feed-bbo:<v> → Bbo,feed-md:<v> → Md,feed:<v> → All。
#[repr(u8)] pub enum FeedRoutes { Bbo = 1, Md = 2, All = 3 }
/// 构造失败(FeedRuntime、SnapFetchTask、GwRuntime、AcctFetchTask 的 new 共用;hft-core 按启动失败处理)。
pub enum InitErr {
    Cfg(&'static str),   // 配置项表下"取值为 0 的键"为 0;值为键名
    BboSlots,            // feed.bbo_conns × 组数 + 1 > MAX_BBO_CONN_SLOTS
    DepsMismatch,        // deps 中环与 journal 的有无与 routes、feed.depth 不符(§1 存在条件)
    SubRate,             // 订阅帧节奏 < 1 帧/s(§2 第 8 步)
    RotateMargin,        // 生效 ws_max_conn_age_s ≠ 0 且 conn.rotate_margin_s ≥ 它
    SimAccount,          // GwRuntime / AcctFetchTask 收到 AccountKind::Sim 的账户
    FrameCap,            // GwRuntime:CAP < P::ORDER_FRAME_CAP
    IdTableCap,          // gw.id_table_cap 不是 2 的幂或 < 2 × (OMS 槽数 + gw.id_tomb_cap)
    MarginPoll,          // 生效 margin_data = Poll{interval_ms} 且 interval_ms < gw.margin_poll_min_ms
}
pub enum ReadyPollMode { Auto, Off }       // feed.ready_poll 的取值 auto | off
/// feed.ready_poll 的判定(hft-core 装配时调用,结果写进 FeedCfg):mode = auto、线程为 Spin、且该线程全部行情任务的计划连接数之和 ≥ 3。
pub fn ready_poll_on(mode: ReadyPollMode, policy: IdlePolicy, thread_conns: u16) -> bool;
/// 拆分形态下一条 FeedCmd 推往哪个 feedctl 环(hft-core 的 live driver 调用;规则见 §1)。
pub enum FeedCmdDest { Bbo, Md, Both }
pub fn feed_cmd_dest(c: &FeedCmd) -> FeedCmdDest;
pub const FEEDCMD_PER_POLL: usize = 8;     // 每次 poll_once 至多取的 FeedCmd
pub const FRAME_EVENTS_MAX: usize = 64;    // 非 Bbo 路由单帧可产出的事件上限;超过 → 整帧按解码错误处理(§1 环满)
pub const SNAP_BBO_BATCH_MAX: usize = 32;  // snap-fetch 一次合并的 BBO 补帧请求上限(§3b;另受 P::BBO_SNAPSHOT_BATCH 限制)

pub struct FeedRuntime<P: VenueProtocol> { /* 见实现说明 §2 */ }
impl<P: VenueProtocol> FeedRuntime<P> {
    pub fn new(cfg: FeedCfg /* 含 routes: FeedRoutes、ready(ready_poll_on 的结果)、max_sub_insts(hft-core venues.<v>.max_sub_insts) */, deps: FeedDeps, ctx: P::FeedCtx) -> Result<Self, InitErr>;   // 启动期,可分配
    /// 循环体一轮。由 hft-rt `TaskLoop` 按线程计划调用(可与其他任务同线程,见 hft-core 线程计划)。
    /// 非阻塞:不自旋等待、不睡眠;每轮工作量有上限(bbo_frames_per_poll、frames_per_poll、每轮至多一条快照记录)。
    pub fn poll_once(&mut self, now_tsc: u64) -> LoopStat;
}
/// poll_once 的返回;did_work = 这一轮处理了任何帧、命令、快照记录、acct 记录、连接事件或定时动作(TaskLoop 的空闲判定只看它);环满而停读的一轮 did_work = false。
pub struct LoopStat { pub did_work: bool, pub frames: u16, pub cmds: u16, pub ring_full: u16, pub next_deadline: Option<u64> /* 心跳/重试/超时的最早 TSC */ }
// FeedRuntime、SnapFetchTask、AcctFetchTask 与 GwRuntime 都实现 hft_rt::PollTask:has_pending = 输入环(出环/命令环/快照环/acct 环)非空或任一连接的 TLS 缓冲有未处理字节;
// wait_fds = 本运行时持有的全部 socket(park 时 epoll 等它们);gw 的 REST 连接只在有在途请求时列入(§4)。

pub struct SnapFetchTask<P: VenueProtocol> { /* 见实现说明 §3b */ }
impl<P: VenueProtocol> SnapFetchTask<P> {
    pub fn new(cfg: SnapCfg, deps: SnapDeps, ctx: P::FeedCtx /* 独立实例,hft-venue-proto 契约 25 */) -> Result<Self, InitErr>;
    pub fn poll_once(&mut self, now_tsc: u64) -> LoopStat;
}

pub struct GwRuntime<P: VenueProtocol, const CAP: usize = 2048> { /* 见实现说明 §4 */ }
impl<P: VenueProtocol, const CAP: usize> GwRuntime<P, CAP> {
    /// 启动期:建槽镜像与 IdTable,并从 raw-gw journal 尾部重建去重环(§6a)。
    /// accts:每个 Live 账户的 GwAccountSpec(账户、出口 IP 序号、独立 REST 凭证、P::build_templates 的结果与 P::GwCtx);其余字段(kill 位、连接号段)由 new 从 refdata 求出。
    pub fn new(cfg: GwCfg, deps: GwDeps, accts: Vec<GwAccountSpec<P>>) -> Result<Self, InitErr>;
    pub fn poll_once(&mut self, now_tsc: u64) -> LoopStat;
}

pub struct AcctFetchTask<P: VenueProtocol> { /* 见实现说明 §4a */ }
impl<P: VenueProtocol> AcctFetchTask<P> {
    /// 启动期:每个 Live 账户建自己的 Templates 与 AcctCtx(hft-venue-proto 契约 27),REST 连接由 conn-builder 交接。
    pub fn new(cfg: AcctCfg, deps: AcctDeps, accts: &[AccountId]) -> Result<Self, InitErr>;
    pub fn poll_once(&mut self, now_tsc: u64) -> LoopStat;
}

pub struct GwAccountSpec<P: VenueProtocol> { pub account: AccountId, pub src_ip_idx: u8, pub tpl: P::Templates, pub ctx: P::GwCtx }

pub struct SessionDriver<P: VenueProtocol> { /* warm 线程 conn-builder */ }
impl<P: VenueProtocol> SessionDriver<P> {
    /// cfg:VenueCfg 与各行情任务的 ReadyHandle(§7 第 4 步);creds:每个账户的会话凭证(Order/UserStream 角色)。
    pub fn new(cfg: SessionCfg, creds: Vec<(AccountId, Credential)>) -> Self;
    /// 按请求方的 BuildReq(角色、连接号)开始一次会话:bootstrap 判定、endpoint(Md 角色不调用)、Connector::submit(§7 第 1–2 步)。
    pub fn start(&mut self, c: &mut Connector, req: BuildReq, acct: Option<AccountId>, now_ns: i64) -> Result<JobId, SessErr>;
    pub fn drive(&mut self, c: &mut Connector, now_ns: i64) -> Option<SessEvent>;  // Ready(Handoff) / Failed{conn, code}
    /// 收到 RetireMsg:先从就绪 epfd 注销(若注册过),再 Connector::reclaim(§7 第 7 步)。
    pub fn retire(&mut self, c: &mut Connector, m: RetireMsg);
}
pub enum SessErr { Busy, Build(BuildErr), Proto(ProtoErr), Bootstrap /* 该角色需要 bootstrap REST(M3 起执行) */ }
/// conn-builder → 请求方任务的交接环元素:登录完成的连接,或请求方所要的那次建连失败(请求方释放该连接号后重建)。
pub enum Handover { Ready(Handoff), Failed(ConnId) }

pub struct FeedDeps {   // 连接经 Handoff 交接、经 RetireMsg 交还,任务不持有 ConnArena 引用
    pub rd: &'static RefData,                // 不热换(变更 = 重启)
    pub bbo: Option<Producer<FeedMsg>>,      // → bbo-<venue> 环(高优先级:Bbo 与 BBO 连接的 ConnState);routes ∈ {Bbo, All}
    pub md: Option<Producer<FeedMsg>>,       // → md-<venue> 环(Trade、Mark/Funding/Index、Liquidation、L2Snapshot{TopN}、深度连接的 ConnState;增量深度另有 L2Batch、L2Snapshot{Exchange});routes ∈ {Md, All}
    pub bulk: Option<ByteProducer>,          // → md-<venue>-bulk:变长 L2Snapshot / L2Batch 体,配 FeedMsg::Bulk;routes ∈ {Md, All}
    pub cmds: Consumer<FeedCmd>,             // ← feedctl-<venue>(Md/All 任务)或 feedctl-<venue>-bbo(拆分时的 feed-bbo),分发规则见 §1
    pub snapreq: Option<Producer<SnapReq>>,  // → snapreq-<venue>-bbo(feed-bbo)或 snapreq-<venue>(All;Md 只在增量深度启用时);wake = None
    pub snap: Option<ByteConsumer>,          // ← 对应的 snap 环(snap-fetch 写入)
    pub ready: Option<ReadyPoll>,            // hft-net 就绪探测;只在 ready_poll 生效时(§2 第 2 步)
    pub raw: JournalWriter,                  // raw-feed-<venue>(Md/All)或 raw-feed-<venue>-bbo(feed-bbo),本任务唯一写者
    pub handoff: Consumer<Handover>, pub retire: Producer<RetireMsg>, pub build: Producer<BuildReq>,
    pub clock: UtcClock,                     // hft-rt 发布线的读者(TSC → UTC,随 housekeeping 校准更新);时长换算取构造时的线
}
pub struct SnapDeps {
    pub rd: &'static RefData,
    pub reqs: [Option<Consumer<SnapReq>>; 2], // snapreq-<venue>、snapreq-<venue>-bbo(各自存在的条件见 §1)
    pub out: [Option<ByteProducer>; 2],       // snap-<venue>、snap-<venue>-bbo
    pub handoff: Consumer<Handover>, pub retire: Producer<RetireMsg>, pub build: Producer<BuildReq>,
    pub clock: UtcClock,
}
pub struct GwDeps {
    pub rd: &'static RefData,
    pub cmds: Consumer<OutMsg>,              // ← sequencer 的 out-<venue> 环
    pub out: Producer<RepMsg>,               // → sequencer 的 rep-<venue> 环(+ rep-<venue>-bulk)
    pub acct_req: Producer<AcctReq>,         // → acctreq-<venue>(acct-fetch);唤醒按 hft-rt 规则(Spin 生产者 wake = None);M3 起(M1.5 实现无此字段)
    pub acct: ByteConsumer,                  // ← acct-<venue>(acct-fetch 写入的 acct 记录,§4a)
    pub raw: JournalWriter,                  // raw-gw-<venue>
    pub raw_tail: JournalReader,             // 同一 journal 的读者,只在 new() 里用于重建去重环(§6a);M3 起(M1.5 实现无此字段)
    pub kill: &'static KillWord,             // hft-rt 的 CachePadded<AtomicU32>;每个 gw 一个;位常量 hft_types::KillBits(u32),语义 hft-risk
    pub kill_mask: u32,                      // GLOBAL | VENUE | HARD(全 gw 共用的位;账户位按 GwAccount.kill_ix 逐条取,见 §4 第 1 步)
    pub tx_tags: Option<Producer<TxTag>>,    // TX 时间戳配对;可选,默认 None
    pub handoff: Consumer<Handover>, pub retire: Producer<RetireMsg>, pub build: Producer<BuildReq>,
    pub clock: UtcClock,
}
pub struct AcctDeps {
    pub rd: &'static RefData,
    pub reqs: Consumer<AcctReq>,             // ← acctreq-<venue>
    pub out: ByteProducer,                   // → acct-<venue>(唤醒按消费者 gw 的空闲策略)
    pub handoff: Consumer<Handover>, pub retire: Producer<RetireMsg>, pub build: Producer<BuildReq>,
    pub clock: UtcClock,
}
```

各任务的 `clock` 是同一条 hft-rt 发布线(`UtcClock`,规则归 [hft-rt 时间](../hft-rt/README.md#时间))的独立读者:墙钟值——gw 与 acct-fetch 填给 `encode`/签名的 `WallClock`、REST 与建连的 `BuildReq.deadline_ns`、应用层 ping 的时间戳、raw 段头与快照记录的 UTC——在使用时按当前发布线换算,同一任务内单调不减;超时、节拍、限速窗口的 TSC 时长取构造时的线。conn-builder 的 `now_ns`(建连截止判定、登录签名)读同一条线,所以截止与判定之间没有跨时钟的偏差。

### 产生与消费的事件

| 任务 | 消费 | 产生(进环,由 sequencer 赋 seq 并记录) | raw journal |
|---|---|---|---|
| `feed-bbo:<venue>`(或 `feed:<venue>` 的 Bbo 部分) | `FeedCmd`、snap 记录(BBO 补帧) | `bbo` 环:`Bbo`,以及 BBO 连接的 `ConnState`(连接级 `Up`/`Down`、成员 `Up`/`Down{inst}`、`Alive`、`Resync`、`Degraded`,§3a) | `RawRx`(每个数据帧、BBO 补帧的 REST 响应、连接/断开/订阅回执标记;双路重复帧只在 `feed.raw_dups` 时写) |
| `feed-md:<venue>`(或 `feed:<venue>` 的 Md 部分) | `FeedCmd`;启用增量深度时另有 snap 记录(L2 快照) | `md` 环:`Trade`、`MarkPrice`、`IndexPrice`、`Funding`、`Liquidation`、`L2Snapshot{origin = TopN}`(分档快照,§3c)、深度连接的 `ConnState`;启用增量深度时另有 `L2Batch`(一条交易所增量消息一条)与 `L2Snapshot{origin = Exchange}` | `RawRx`(每个数据帧、REST 快照响应、连接/断开/订阅回执标记) |
| `snap-fetch`(`SnapFetchTask<P>`) | `SnapReq` | 只写 snap 环(不进 sequencer) | 不写(原始响应随 snap 记录交给行情任务,由行情任务写 `RawRx`) |
| `acct-fetch`(`AcctFetchTask<P>`) | `AcctReq` | 只写 `acct-<venue>` 环(交回 gw,不进 sequencer) | 不写(请求元数据与按块切分的原始响应随 acct 记录交给 gw,由 gw 写 `RawTx`/`RawRx`,§4a) |
| `gw:<venue>` | `OutMsg`(`GwCmd`)、acct 记录 | `SendReport`(每条订单类 `GwCmd` 恰好一条,带 `op/target_slot/target_gen`)、`OrderEvent`(带 `epoch`,OMS 按 `(epoch, slot, gen)` 匹配;成交类与 `QueryResult` 必带 `cum_filled` 与 `avg_px`;订单终态后才到的费用为 `FeeOnly`)、`ExternalFill`(自身强平/ADL/人工/交割/来源不明的成交,[hft-venue-proto 契约 18](../hft-venue-proto/README.md#实现方必须遵守的契约))、`MarginReport` 与 `AccountConfigReport`(§4a;acct 记录原样转入,或由推送解码)、`PositionReport`、`BalanceReport`、`FundingSettle`(已按契约 22 去重)、`ConnState` | `RawRx`(回报、账户查询响应)、`RawTx`(订单帧明文;**登录帧与任何含签名的帧、签名的 REST 请求只记元数据**,`RawHdr.rflags = REDACTED`);`Connect` 标记的记录体为 `[account: u8, role: u8]`(§6a 用它还原帧所属账户) |

记录前缀 `RawHdr` 见 [hft-event](../hft-event/README.md)。`ConnState`(24 B,布局以 [event-model](../../docs/subsystems/event-model.md) 为准):`account`(0 = 行情连接)、`role`(`ConnRole`:`Md=1 Order=2 UserStream=3 Rest=4`,编码归 [event-model](../../docs/subsystems/event-model.md#关键事件体))、`inst`(0 = 整条连接)、`rx_tsc`(引起该状态的帧或检测时刻;`Alive` 为探测回帧的 rx_tsc)。**UserStream 断开(或私有频道失去归属连接)同样推 `ConnState{Down, role=UserStream}`**,OMS 据此把相关槽置 VERIFY(回报缺口)。可忽略与未知的用户流事件**不是**解码失败,不产生 `ConnState`(§4 第 2 步)。

**`ConnState.reason` 码表**(本 crate 是唯一归属,以 `const` 表提供给 `gen-schema`;只追加):`Down`/`Rotated` 时为 [hft-net](../hft-net/README.md) 的 `ErrBits` 原值;`Up`/`Alive` 时为 0;`Resync`/`Degraded` 时取下表。

| 码 | 名 | 状态 | 含义 |
|---|---|---|---|
| 1 | `GAP` | Resync | 增量深度序号断裂(§3) |
| 2 | `DECODE` | Resync | 深度类消息解码失败 |
| 3 | `SNAPSHOT_FAIL` | Degraded | 快照连续失败(增量深度) |
| 4 | `SUB_REJECTED` | Degraded | 订阅被拒 |
| 5 | `RACE_SINGLE` | Degraded | BBO 赛马池只剩一条 |
| 6 | `BBO_REFRESH` | Resync | 该 inst 的 BBO 连接全断过,等一帧新报价(§3a) |
| 7 | `BBO_SILENT` | Degraded | BBO 连接超过 `feed.bbo_dead_ms` 无任何入向帧,随后按 Down 处理 |
| 8 | `DEADMAN` | Degraded | 死手刷新失败 |
| 9 | `CLOCK_SKEW` | Degraded | 交易所报时间戳越界(如 Binance `-1021`) |
| 10 | `INFLIGHT_TIMEOUT` | Degraded | 请求超过 `gw.inflight_timeout_ms` 无响应 |
| 11 | `RATE_LIMIT` | Degraded | 交易所返回限速 |
| 12 | `REPORT_DECODE` | Degraded | 回报解码失败,OMS 按用户流缺口处理(§4 第 2 步) |
| 13 | `MIRROR` | Degraded | 槽镜像满或 inst 未知 |
| 14 | `ACCT_QUERY` | Degraded | 账户查询连续失败(§4a)或会话 UID 查询失败(§7) |
| 15 | `CONN_LOST` | Resync | 该 inst 所在的深度连接(分档快照或增量深度)断开或重连 |
| 16 | `NEW_SUB` | Resync | 新订阅的深度,等第一条快照 |
| 17 | `REQUESTED` | Resync | `FeedCmd::Resync` |

### 配置项

来自 hft-core 配置(内容作为 `ConfigBlob` 记录);协议常量来自 `P`;限速与连接上限的生效值来自 refdata。**本表是这些键的名字、类型、默认与含义的唯一定义**(含 raw-feed 记录方式 `feed.raw.*`);TOML 中它们都在 `[venues.<v>]` 下(如 `venues.<v>.snap.parallel`),按主机类的取值见 [hft-core 配置文件字段](../../apps/hft-core/README.md#配置文件字段) 与[内存表](../../apps/hft-core/README.md#内存与-journal-段按主机类唯一归属)。

| 名称 | 类型 | 默认 | 含义 |
|---|---|---|---|
| `feed.bbo_conns` | u8 | 2 | 每组 BBO 连接的活跃条数(双路赛马,单路是降级);组数 = ⌈该 venue BBO 订阅 inst 数 / 生效 `ws_max_streams_per_conn`⌉;`bbo_conns × 组数 + 1(在建)> MAX_BBO_CONN_SLOTS` → `InitErr::BboSlots`(§1);"BBO 订阅 inst 数"取 `FeedCfg.max_sub_insts`;为 1 时不产出 `RACE_SINGLE`。**BBO 连接条数只由本键决定**;hft-core 的连接配置 `venues.<v>.feed.conns`(每项 `{stream, src_ip}`,字段归 [hft-core](../../apps/hft-core/README.md#配置文件字段))只给各流连接的源地址 |
| `feed.depth_conns` | u8 | 1 | 每个 `DepthTopN` 组的活跃连接条数(同订阅,互为备份,§3c);组数 = ⌈该 venue 深度订阅 inst 数 / 生效 `ws_max_streams_per_conn`⌉(取 `FeedCfg.max_sub_insts`);深度连接与其余 md 连接共用连接号 `MD_FIRST..=MD_LAST`(§1),`depth_conns × 组数 + 1(在建)> 56` → `InitErr::Cfg("feed.depth_conns")`;为 1 时不产出深度组的 `RACE_SINGLE` |
| `feed.bbo_frames_per_poll` | u16 | 8 | Bbo 路由每条连接每次 `poll_once` 至多处理的帧数;连接之间按帧轮转(§2 第 3 步) |
| `feed.frames_per_poll` | u16 | 64 | 非 Bbo 路由每条连接每次 `poll_once` 至多处理的帧数 |
| `feed.ready_poll` | `auto \| off` | `auto` | 就绪探测(§2 第 2 步):`auto` = 本任务所在线程为 `Spin`、且该线程全部行情任务的计划连接数之和 ≥ 3 时生效(hft-core 装配时以 `ready_poll_on` 按线程计划判定,结果写进 `FeedCfg`);`off` 不探测 |
| `feed.bbo_ping_ms` | u32 | 1000 | BBO 连接的存活探测周期(`P::bbo_probe()`,与有无数据无关),保证健康连接每秒至少有一帧回帧(§3a) |
| `feed.bbo_dead_ms` | u32 | 3000 | BBO 连接超过此时长无任何入向帧 → `BBO_SILENT`,退役并补建 |
| `feed.bbo_refresh_grace_ms` | u32 | 500 | 连接恢复、订阅回执齐后,等交易所自然推送新 BBO 的时长;超时仍无报价的 inst 经 snap-fetch 补帧(§3a 第 5 步) |
| `feed.depth` | `topn \| incremental` | `topn` | inst 订阅了深度(`FeedCmd` 的 `BOOK` 流,§1)时用哪种深度流:`topn` = 分档快照流(§3c);`incremental` = 全量增量深度 + 快照重同步(可选扩展,§3;只有该 venue README 已给出增量深度全部必备节时可选) |
| `feed.topn_levels` | u8 | 5 | 分档快照的档数:取该 venue `DepthTopN` 路由支持的档位中 ≥ 本值的最小者,都小于本值时取最大档(档位见各 venue README"分档快照流") |
| `feed.snapshot_part_levels` | u16 | 64 | `L2Snapshot` 每片最多档数:venue 经 `MdSink::l2_snapshot` 交出的快照由运行时 sink 按此切片(分档快照、流内首帧快照、REST 快照同一规则);sequencer 侧 BBO 等待的上界按它计算([latency-budget §2.3](../../docs/latency-budget.md#23-bbo-与深度的通道)) |
| `feed.resync_slots` | u8 | 4 | (增量深度)同时处于重同步的 inst 上限(每槽一块 `resync_buf` 缓冲);超出的 inst 排队 |
| `feed.resync_buf` | u32 | 1024 | (增量深度)每个重同步槽缓冲的交易所增量消息条数(预分配,存已解码的 L2Batch) |
| `feed.snapshot_backoff_ms` | `(u32,u32)` | `(100, 5000)` | (增量深度)快照失败后重试的退避起点/上限;连续 6 次失败 → `Degraded{SNAPSHOT_FAIL}` |
| `feed.raw_dups` | bool | false | BBO 双路重复帧也写 raw(赛马分析时在 dev2 或捕获窗口打开;每个重复帧在 feed-bbo 线程上多一次 raw 追加);`feed.raw.bbo = sampled` 时不生效 |
| `feed.raw.bbo` / `feed.raw.md` | `off \| sampled \| on` | 按主机类与实例取 hft-core 内存表(升级主机 `prod` `on`/`on`、同机 `sim` `off`/`off`) | raw-feed journal 的写入粒度:`on` 全写;`sampled` 每 inst 每 `feed.raw.sample_ms` 至多一条数据帧(`RawHdr.rflags = SAMPLED`);`off` 不写数据帧。**控制帧(连接/断开/订阅回执)与 REST 响应(BBO 补帧、快照)在任何模式下都写**;raw-gw 始终全写。`off`/`sampled` 下金样只能从 `on` 的主机或捕获窗口抽取 |
| `feed.raw.sample_ms` | u32 | 1000 | `sampled` 的采样间隔:同一 inst 距上一条被抽中的帧 ≥ 本值的第一帧被写 |
| `feed.ring_full_park_us` | u32 | 200 | 环满的一轮按空闲处理时给 TaskLoop 的 `next_deadline` |
| `snap.parallel` | u8 | 2 | snap-fetch 每 venue 同时在途的 REST 请求上限(= 该 venue 的快照 REST 连接数,一连接一在途;BBO 补帧与(启用增量深度时)L2 快照共用) |
| `conn.rotate_margin_s` | u32 | 3600 | 计划轮换余量;hft-cfg 把它编码进 ECF1 段 6 `conn`(本 crate 定义行格式:`{venue: VenueId, _pad: [u8; 3], rotate_margin_s: u32}`,8 B,每 venue 一行;[hft-refdata ECF1](../hft-refdata/README.md) 只链接)。**计划轮换时刻的唯一规则**:连接 Open 时刻 + refdata `VenueMeta.ws_max_conn_age_s` − `rotate_margin_s`(`ws_max_conn_age_s = 0` 表示交易所不强制断开,不做计划轮换);feed、gw、snap-fetch 与 acct-fetch 的全部长连接同一规则,由本 crate 算出后交给 hft-net `ConnPool::install`;`GwCmd{Rotate}` 只用于提前触发;生效 `ws_max_conn_age_s ≠ 0` 且本值 ≥ 它 → 各运行时 `new()` 返回 `InitErr::RotateMargin` |
| `gw.cmds_per_loop` | u16 | 16 | 每次 `drain_out()` 最多取的 `GwCmd` |
| `gw.cmds_per_account_per_loop` | u16 | 8 | 每账户每轮最多发送的订单类 `GwCmd`(超出的下一轮先发) |
| `gw.aux_every` | u16 | 8 | standby 下单连接与 gw 的 REST 池中**有在途请求**的连接每 N 圈 poll 一次;primary 与独立 UserStream 连接每圈都 poll(§4) |
| `gw.rest_idle_check_ms` | u32 | 1000 | gw 的 REST 连接没有在途请求时不读;每隔此时长对这些空闲连接做一次存活检查(hft-net `RestPool::idle_check`:各读一次,对端已关闭即退役补建;hft-net `rest_max_idle_ms` 非 0 时同一次检查另退役一条空闲过久的连接,补建经 build 环交给 conn-builder,下单不在新建中的连接上发出)。snap-fetch 与 acct-fetch 的 REST 池按同一间隔检查 |
| `gw.rotate_wait_max_ms` | u32 | 60000 | 计划切换等待"无在途"的上限;超时放弃本次,再过同样时长后重新开始等待 |
| `gw.warm_interval_ns` | u64 | 0(关) | 空闲时"填帧 + 掩码 + scratch 加密"预热;可选,默认关,启用条件:直方图证明冷路径代价显著 |
| `gw.id_table_cap` | u32 | 16384 | 每账户 exch id 表容量(2 的幂,≥ 2 × (OMS 槽数 + `gw.id_tomb_cap`),负载 ≤ 0.5;不满足 → `InitErr::IdTableCap`) |
| `gw.id_tomb_cap` | u32 | 4096 | 每账户终态保留条目上限(§6);超出时先淘汰最老的 |
| `gw.id_tomb_ms` | u32 | 120000 | 终态保留条目至少保留的时长;晚到费用与迟到成交在此窗口内仍能解析到原 `(epoch, slot, gen)` |
| `gw.unmatched_hold_ms` | u32 | 5000 | 无法归属到槽的成交类 `OrderEvent` 在 gw 暂存等待 Ack 补表的时长;超时才转 `ExternalFill{cause: Unknown}`(§4 第 2 步)。Ack 一到即释放,正常情况下暂存只有毫秒级 |
| `gw.unmatched_hold_cap` | u16 | 256 | 上述暂存缓冲条数(每账户,预分配;按 5 s 窗口取值);满 → 最老一条立即按超时处理并计数 |
| `gw.dedup_reseed_ms` | u32 | 900000 | 启动时从 raw-gw journal 尾部重建去重环所读的时间窗(§6a) |
| `gw.inflight_timeout_ms` | u32 | 3000 | 请求超过此时长无响应 → 每个请求推一次 `ConnState::Degraded{reason=INFLIGHT_TIMEOUT}` 并移出 `inflight`(之后迟到的响应照常解码,不再减计数) |
| `gw.report_decode_down_after` | u8 | 3 | 同一连接 60 s 内连续 N 次回报解码失败 → 按 Down 处理(retire + `ConnState{Down}`);该连接任一帧解码成功即清零,第 N 次与这一轮的第一次相隔 > 60 s 时以这一次为新一轮的第一次 |
| `gw.stale_guard_ms` | u32 | 1500 | 覆盖 `P::stale_guard()` 的默认值,也是请求过期字段的窗口([05 §7](../../design/05-CEX执行层.md) "收紧到 1–2 s") |
| `gw.margin_poll_min_ms` | u32 | 1000 | `margin_data = Poll{interval_ms}` 的下限;refdata 给出更小的值 → `GwRuntime::new` 返回 `InitErr::MarginPoll` |
| `gw.margin_push_stale_ms` | u32 | 5000 | `margin_data = Push` 时,超过此时长没有账户/持仓推送 → 执行一次兜底查询(`push_fallback` 请求) |
| `gw.acct_cfg_recheck_s` | u32 | 600 | 周期性重查账户配置(另有回报触发的 `ConfigRecheck`) |
| `gw.acct_query_timeout_ms` | u32 | 3000 | 单个账户查询请求超时(acct-fetch 计时) |
| `gw.acct_query_degraded_after` | u8 | 3 | 同一账户连续 N 个查询序列失败 → `ConnState{Degraded, role=Rest, reason=ACCT_QUERY}` |
| `gw.acct_body_max` | u32 | 262144 | 账户查询响应体上限(字节);超过 → 该序列 `Failed{TooLarge}`,原始响应只记元数据 |
| `gw.unknown_evt_note_every` | u32 | 1000 | 未知用户流事件的第 1 条与第 k × N 条(k ≥ 1)各写一条 Meta Note(源 `Venue`) |

**取值为 0 的键**:`feed.bbo_conns`、`feed.depth_conns`、`feed.bbo_frames_per_poll`、`feed.frames_per_poll`、`feed.bbo_ping_ms`、`feed.bbo_dead_ms`、`snap.parallel`、`gw.cmds_per_loop`、`gw.cmds_per_account_per_loop`、`gw.aux_every`、`gw.unmatched_hold_cap`、`gw.acct_query_degraded_after`、`gw.report_decode_down_after` 为 0 时,用到它的运行时 `new()` 返回 `InitErr::Cfg(<键名>)`。

环容量(`snapreq-*`、`snap-*`、`acctreq-*`、`acct-*` 与各 bulk 环)不在本表:唯一权威值是 hft-core 的环容量表(ECF1 段 4,[hft-core 配置文件字段](../../apps/hft-core/README.md#配置文件字段));本 crate 只规定字节环的单条记录 ≤ 容量 / 4。

## 实现说明

<details><summary>实现要点</summary>

### 1. 线程与环(由 hft-core 装配)

```
                                 ┌──Handoff──▶ feed-bbo:<v>  FeedRuntime<P>{Bbo} ──Bbo/ConnState──▶ sequencer(bbo-<v>,高优先级)
                                 ├──Handoff──▶ feed-md:<v>   FeedRuntime<P>{Md}  ──md 事件──────▶ sequencer(md-<v> + md-<v>-bulk)
conn-builder: SessionDriver<P> ──┼──Handoff──▶ snap-fetch    SnapFetchTask<P>    (公共 REST 连接)
                                 ├──Handoff──▶ acct-fetch    AcctFetchTask<P>    (签名 REST 连接)
                                 └──Handoff──▶ gw:<v>        GwRuntime<P>        ──RepMsg───────▶ sequencer(rep-<v> + rep-<v>-bulk)
                 ◀──RetireMsg / BuildReq───  (全部)                              ◀──OutMsg────── sequencer(out-<v>)
feed-bbo ─SnapReq(snapreq-<v>-bbo)─▶ snap-fetch ─snap 记录(snap-<v>-bbo)─▶ feed-bbo
feed-md ──SnapReq(snapreq-<v>)──▶ snap-fetch ──snap 记录(snap-<v>)──▶ feed-md          (只在启用增量深度时)
gw ──AcctReq(acctreq-<v>)──▶ acct-fetch ──acct 记录(acct-<v>)──▶ gw
sequencer ──FeedCmd(feedctl-<v>)──▶ feed-md          sequencer ──FeedCmd(feedctl-<v>-bbo)──▶ feed-bbo
```

**任务与路由**:升级主机类的 `prod` 实例上每个 venue 是两个行情任务——`feed-bbo:<venue>`(`FeedRoutes::Bbo`,只有 `RouteKind::Bbo` 路由)与 `feed-md:<venue>`(`FeedRoutes::Md`,`DepthTopN` 与 `Md` 路由),两者都是 `FeedRuntime<P>`;`feed-bbo:*` 可以跨 venue 合并在同一线程,但不得与 `feed-md:*` 同线程(hft-core 加载校验 `BboSharesWithMd`,线程计划与该校验的适用范围归 [hft-core](../../apps/hft-core/README.md#线程计划))。dev2/c4 与 `sim` 实例上一个 venue 可用单个任务 `feed:<venue>`(`FeedRoutes::All`)。一个 venue 要么只有 `feed:<venue>`,要么同时有 `feed-bbo:<venue>` 与 `feed-md:<venue>`。REST 全部在 warm 任务里,行情任务与 gw 不做 BBO 补帧、快照与账户查询的 REST:`snap-fetch`(每个 hft-core 实例一个,按 venue 持有各 `SnapFetchTask<P>`,组织方式同 `conn-builder` 持有各 `SessionDriver<P>`)取公共行情 REST;`acct-fetch`(每个有 gw 的 hft-core 实例一个,按 venue 持有各 `AcctFetchTask<P>`)用各账户的 REST key 做签名的账户查询。两个任务的线程落点归 [hft-core 线程计划](../../apps/hft-core/README.md#线程计划)。

**环与 raw journal 的存在条件**(名字的 `-bbo` 后缀表示属于拆分出来的 BBO 任务):

| 环 / journal | 存在条件 |
|---|---|
| `feedctl-<venue>`、`raw-feed-<venue>` | 总是(Md 或 All 任务) |
| `feedctl-<venue>-bbo`、`snapreq-<venue>-bbo`、`snap-<venue>-bbo`、`raw-feed-<venue>-bbo` | 该 venue 拆成 `feed-bbo:` 与 `feed-md:` 两个任务时 |
| `snapreq-<venue>`、`snap-<venue>` | `routes = All` 时(承载 BBO 补帧,启用增量深度时另承载 L2 快照);拆分时只在该 venue `feed.depth = incremental` 时 |
| `acctreq-<venue>`、`acct-<venue>` | 该 venue 有 gw 时(`prod` 实例中有 Live 账户在该 venue 交易) |

每个行情任务是它那份 raw journal 的唯一写者;gw 是 `raw-gw-<venue>` 的唯一写者(acct-fetch 的请求与响应也由 gw 写入)。写者在帧边界按 `needs_roll` 滚动(raw 只有 `EventBoundary`,[hft-journal](../hft-journal/README.md));`roll` 返回 `NextNotReady` → 继续当前段,下一个帧边界重试;`append` 返回 `Full`(或其他 `AppendErr`)→ 该条 raw 不写,计数 `raw_append_err{journal, err}`(告警),入环与收发照常——raw 只服务排障、金样与去重环重建(§6a),决策以 seq journal 为准。

**`FeedCmd` 的分发**(sequencer 侧只产出一条 `FeedCmd`,hft-core 装配的 live driver 按此规则推到一个或两个环):`Resync{inst}` → 承载 Md 路由的任务;`Rotate{conn}` → 拥有该连接的任务(`conn < MAX_BBO_CONN_SLOTS` → feed-bbo);`Subscribe{inst}`/`Unsubscribe{inst}` → 两个任务各一份(拆分形态的去向由 `feed_cmd_dest` 给出);`BindWire{inst, part, chunk}` → 承载 Md 路由的任务。`Subscribe` 带该 inst 需要的流集合 `streams`(该 inst 全部订阅的 SDK `EvMask` 之并;字段归 [hft-event](../hft-event/README.md#环元素类型),位值与 SDK `EvMask` 相同,由 sequencer 在并集变化时重发):`BBO` 位由 feed-bbo 订阅 Bbo 路由;`TRADE`、`MARK`、`INDEX`、`FUNDING`、`LIQUIDATION` 位由 feed-md 订阅对应 `Md` 路由;`BOOK` 位由 feed-md 按 `feed.depth` 订阅 `DepthTopN` 路由(或增量深度)。没有 `BOOK` 位的 inst 不订阅任何深度流。同一 inst 的 `Subscribe` 给出期望的流集合:任务按它与当前集合的差分发订阅与退订帧,集合不变不发帧;`Unsubscribe{inst}` 等同空集合。inst 不属于本 venue → 计数 `cmd_bad_inst` 丢弃;任务收到不属于它的命令(Bbo 任务的 `Resync`、不归它的 `Rotate{conn}`)→ 计数 `cmd_ignored` 丢弃。**`BindWire`**(运行期绑定交易所 id 的 venue,命令语义归 [control-plane `BindSlot`](../../docs/subsystems/control-plane.md#命令集)):每 inst 一块 32 字节缓冲按 `part`(0..=3)拼装;`part = 3` 时四段齐全才调用 `P::bind_wire(ctx, inst, id)`,不齐(或 `part > 3`)→ 计数 `cmd_ignored` 并清空缓冲;协议拒绝 → 计数 `bind_failed`;成功 → 计数 `binds`,该 inst 若有深度订阅(`BOOK` 位),在其深度组的连接上依次排入退订(渲染旧 id)与订阅(渲染新 id;解绑时只退订),`l2_state = AwaitFirst` 并推 `ConnState{Resync, inst, reason=NEW_SUB}`,簿等新 id 的第一条快照。

**连接号(`ConnId`,u8,venue 内)**:

| 号段 | 归属 | 分配 |
|---|---|---|
| `0 .. MAX_BBO_CONN_SLOTS` | BBO 连接(feed-bbo 或 All 任务) | **槽号即连接号**:新 BBO 连接取最小的空闲槽;连接退役并推出 `ConnState{Down, conn, inst = 0}` 之后槽才空闲,可被下一条 BBO 连接复用 |
| `MAX_BBO_CONN_SLOTS`–63 | 分档快照、增量深度、成交、标记价连接(feed-md 或 All 任务) | 号段内循环分配,跳过仍在用的号 |
| 64–79 | snap-fetch 的 REST 连接 | 同上 |
| 80–111 | acct-fetch 的 REST 连接(每 (账户, 用到的端点) 一条) | 同上 |
| 112–255 | gw 的下单、用户流与 REST 连接 | 同上 |

`MAX_BBO_CONN_SLOTS` 的值只在 [hft-types](../hft-types/README.md)(与 `ConnId` 同节);槽号规则归本节。sequencer 按 (venue, 槽) 维护连接活性、按 inst 维护 BBO 成员位图(`bbo_member_mask: u8`,[hft-sequencer](../hft-sequencer/README.md#连接活性));本 crate 保证:同一槽号的新连接产生的任何记录都在旧连接的 `Down{inst = 0}` 之后入 bbo 环,且任一时刻同一 venue 的 BBO 连接(活跃、在建、待退役)不超过 `MAX_BBO_CONN_SLOTS` 条(`feed.bbo_conns` 的校验)。

**环满**:每个 feed/gw 任务的输出环都是 SPSC 单生产者;环满时生产者**不丢、也不在任务内自旋**(丢行情即坏簿;任务内自旋会卡住同线程的其他任务):这一轮停止读输出到该满环的那些连接(数据留在 TLS/内核缓冲,TCP 自然背压;输出到其他环的连接照常),`ring_full` 计数;**这一轮按空闲处理**(`did_work = false`,`next_deadline = now + feed.ring_full_park_us`),并且环满期间 `has_pending` 不把 socket 里的待读数据算作待处理,只看命令环与环空位——这样 `SpinThenPark` 线程(`dev2`)会带超时 park,而不是对着满环忙等;`Spin` 线程照常空转。消费者(sequencer)腾出空位不唤醒生产者,超时到期后下一轮重试。任何非零 `ring_full` 都是告警。每取一帧之前确认目标环至少有该路由单帧事件上限的空位(Bbo 路由 1 条;其余路由 `FRAME_EVENTS_MAX` 条,bulk 环另需放得下一片 `feed.snapshot_part_levels` 档的分片),不足即按环满处理;单帧产出超过 `FRAME_EVENTS_MAX` 条事件时**整帧按解码错误处理**:sink 在帧结束时才发布本帧事件,超限的帧一条都不发布(不部分丢弃),计数 `frame_events_over`(告警)与解码失败计数,深度类按 §2 第 5.4 步推 `Resync{DECODE}`。gw 把 `rep` 环空位 ≤ `gw.cmds_per_loop` 视为满,同样处理(不读下单连接的新回报、不应用 acct 记录;这部分空位留给出环命令的 `SendReport`,出环照常取);空位为 0 时不从出环取命令。sequencer 取环优先级与 md 的逐条处理规则归 [hft-sequencer](../hft-sequencer/README.md)。

**收包**:中断驱动,网卡软中断在专用 IRQ 核,feed/gw 任务只做非阻塞 `recvmsg`,不做 NAPI 轮询([hft-net 接收路径](../hft-net/README.md#1-接收路径));一次读取返回的字节数小于所给空间即认为内核队列已空(短读即停,不再补一次空 `recvmsg`)。在 `SpinThenPark` 线程上(`dev2`),全部任务连续空闲后线程在 epoll(本任务的 socket + 唤醒 eventfd)上 park,网络数据到达即醒([hft-rt 空闲策略](../hft-rt/README.md))。

**到 warm 线程的环不唤醒**:热任务生产到 warm 线程的全部环(`snapreq-*`、`acctreq-*`、`RetireMsg`、`BuildReq`)在 `Spin` 线程上 `wake = None`(热线程上不出现唤醒系统调用,[hot-path-rules T7](../../docs/hot-path-rules.md)),snap-fetch、acct-fetch 与 conn-builder 在自己的 park 超时后取。代价:BBO 补帧、账户查询(含 `MarginNow`)与重建连接各多等至多一个 warm 线程 `park_us`(升级主机类的取值归 [hft-core 线程计划](../../apps/hft-core/README.md#线程计划))。warm 线程生产到热任务的环(`snap-*`、`acct-*`)按消费者的空闲策略设置唤醒(warm 线程可以做系统调用;消费者是 `Spin` 时不需要)。

**网卡**:每条连接绑定其源地址所在的网卡(hft-net `SO_BINDTODEVICE`)。c32 线程计划预留的第二块交易网卡 ENI-B2(默认不启用)启用后,feed-md 与 snap-fetch 的连接用 ENI-B2 上的源地址,feed-bbo、gw 与 acct-fetch 的连接留在 ENI-B(acct-fetch 的签名请求必须用该账户 key 白名单内的地址);源地址的分配归 [hft-core 主机资源隔离](../../apps/hft-core/README.md#主机资源隔离slice核与出口-ip)。snap-fetch 与 acct-fetch 没有自己的源地址键,复用规则归 [hft-core 配置文件字段](../../apps/hft-core/README.md#配置文件字段)(`venues.<v>.feed.conns`、`venues.<v>.gw.src_ip`)。

### 2. `FeedRuntime::poll_once` 的固定顺序

每条路由一个或多个连接池(`ConnPool`;路由表来自 `P::routes()`,本任务只建 `routes` 包含、且有订阅的那些):`RouteKind::Bbo` 路由(`bookTicker` 类)按组建池,每组双路赛马;`DepthTopN` 路由每组 `feed.depth_conns` 条连接(默认 1,§3c),`Md` 路由每组 1 条连接(订阅超过单连接流数时按组拆分)。

1. 取 `FeedCmd`(≤ `FEEDCMD_PER_POLL` 条)。
2. **就绪探测**(`feed.ready_poll` 生效时):`ReadyPoll::probe` 一次(`epoll_pwait(timeout = 0)`,[hft-net](../hft-net/README.md#签名草案)),得到本任务就绪连接的集合,对未就绪的每条活跃连接调 `Conn::note_empty(探测前读的 TSC)`(`rx_empty_gap_ns` 的确认点,hft-net §7);未生效时把全部活跃连接视为就绪。就绪集合只是提示:误报只多一次空 `recvmsg`;缓冲里还有未处理完整帧的连接(上一轮因帧数上限留下)不需要就绪也参加第 3、4 步。
3. **Bbo 路由**(`routes ∈ {Bbo, All}`):对每条就绪的 BBO 连接 `poll_rx_if_needed`;然后**按帧轮转**:起点连接每次 `poll_once` 轮换一位,按连接号顺序每条连接取一帧(`next_frame`)按第 5 步处理,一圈接一圈,直到各连接没有完整帧或已各处理 `feed.bbo_frames_per_poll` 帧。
4. **其余路由**(`routes ∈ {Md, All}`;`DepthTopN` 与 `Md`):对每条就绪连接 `poll_rx`,逐连接逐帧(≤ `feed.frames_per_poll`)按第 5 步处理。`routes = All` 时第 3、4 步按顺序各做一次,即简单轮转:BBO 可能等本任务一整轮其余路由的帧,这一形态只用于没有延迟验收的 dev2/c4 与 `sim` 实例。
5. 对每帧:
   1. `c = P::classify(route, op, &payload[..64])`(`route` = 该连接所属路由);控制类(订阅回执、应用 pong、服务端下线预告)→ 订阅回执更新该连接的订阅状态(被拒 → `ConnState{Degraded, reason=SUB_REJECTED}`),pong 记心跳,下线预告置 `SERVER_HINT` 触发替补(第 8 步),不产生行情事件;本路由不认识的类(`Unknown`)→ 计数丢弃(raw 照写);
   2. `pk = P::peek(rd, ctx, c, payload)`;未订阅符号 → 计数丢弃(raw 照第 5 步写);
   3. 按 `P::seq_rule(c)` 与该 inst 的 `InstSeq` 判定 **转发 / 重复 / 断裂**(§3);
   4. 转发:`P::decode_md(rd, ctx, c, payload, rx, &mut sink)`(`rx` 带该帧 `Timestamps` 的 `rx_tsc`、`hw_rx_ns`(= `best_rx().0`)、`sw_rx_ns`、`rx_empty_gap_ns` 与 `ts_flags` 原值,取戳规则归 [hft-net §7](../hft-net/README.md#7-时间戳);venue 把它们原样写进 `MarketHdr`,[hft-venue-proto 契约 3](../hft-venue-proto/README.md#实现方必须遵守的契约));`Bbo` 写 `bbo` 环,其余写 `md` 环；行情（含 Bulk/REST 快照）与订单回报（含 REST、暂存回报）附带接收来源标志，选择规则见 [事件模型](../../docs/subsystems/event-model.md#时间戳语义)；阻塞快照接口没有内核接收戳时留零;`L2Snapshot`(分档快照,§3c)与增量深度的 `L2Batch` 以变长体流式写入 bulk 字节环,结束时推 `FeedMsg::Bulk`;`DecodeErr` → 计数,若该类是深度则该 inst 推 `ConnState{Resync, inst, reason=DECODE}`;
   5. 追加 `RawRx`(`RawHdr{venue, conn, what=WsText|WsBinary, stream, hw_rx_ns, payload_len}` + payload,记录头 `tsc = rx_tsc`,记录头标志 `HW_TS`/`SW_TS` 由本运行时按 `ts_flags` 置位;**先入环,后写 raw**);是否写由该路由的 `feed.raw.bbo`/`feed.raw.md` 模式决定,重复帧再按 `raw_dups`。
   每条连接处理完后 `finish_poll`。
6. **snap 记录**(每轮至多一条,§3b):Bbo 部分 → BBO 补帧(§3a 第 5 步);启用增量深度时 Md 部分 → 重同步状态机(§3)。记录里的原始响应体写 `RawRx`(`what = RestResponse`,`conn` 为 snap-fetch 的 REST 连接号,戳取响应的接收戳)。
7. 每条连接的 `ErrBits`:致命 → `retire`,推 `ConnState{Down, conn, role=Md, inst = 0, reason=bits}`;Bbo 赛马池只剩一条 → `ConnState{Degraded, reason=RACE_SINGLE}`;深度连接(分档快照或增量深度)断开 → 其上的 inst 推 `ConnState{Resync, inst, reason=CONN_LOST}`,同组另有已应答该 inst 订阅的连接时不推(§3c);`feed.depth_conns ≥ 2` 时该组只剩一条连接 → 对它推 `ConnState{Degraded, reason=RACE_SINGLE}`(md 环)。
8. 心跳(§5)与空闲超时;连接池维护:`next_build` → 按 §1 分配连接号(BBO 连接此时就占用一个空闲槽)→ `BuildReq`;建连失败 → `ConnPool::build_failed` 并释放该号(从未推过记录,可直接复用);`handoff` 到来 → 按 `conn.rotate_margin_s` 算计划轮换时刻 → `install` → 推 `ConnState{Up, conn, inst = 0}` → 以 ⌊生效 `ws_max_in_msgs_per_s` / 2⌋ 帧/s(BBO 连接再减 ⌈1000 / `feed.bbo_ping_ms`⌉)发订阅帧(结果 < 1 → `new()` 返回 `InitErr::SubRate`);`rotate_due` / `ServerShutdown` 提示 → 建替补;赛马池的替补订阅回执齐并收到第一帧数据后,退役同组最早 `install` 的连接。
9. 统计写入本任务的 `LoopStat`(每 1 s 由 hft-core 汇总为 Meta 类 `LatencySample`/`Note`)。

### 3. 去重、断裂与重同步

**每 inst 一条 `InstSeq`**(每个行情任务各一份稠密数组,按 inst 在本 venue 的局部下标索引,一条一缓存行;feed-bbo 只用 BBO 流的槽位):

```rust
#[repr(C, align(64))]
struct InstSeq {
    last_fwd:    [u64; 6],   //  0 按流分别记录:BBO / 深度(分档快照或增量)/ Trade / Mark / Index / Funding(各自的序号或交易所时间)
    l2_state:    u8,         // 48 Live | AwaitFirst | Resync | AwaitSnapshot | Aligning | Degraded(后三者只在增量深度)
    resync_slot: u8,         // 49 占用的重同步槽,0xFF = 无(增量深度)
    _pad:        [u8; 2],    // 50
    resync_n:    u32,        // 52 连续重试次数(增量深度)
    _pad2:       [u8; 8],    // 56
}
const _: () = assert!(core::mem::size_of::<InstSeq>() == 64);
```

**规则**(连接 c 送来一条消息,`ids = (prev_id, last_id)`):

| 规则 | 重复 | 连续(转发) | 断裂 |
|---|---|---|---|
| `Monotonic` / `Snapshot`(BBO 双路、成交、有序号的分档快照) | `last ≤ last_fwd` → 丢 | `last > last_fwd` → 转发 | 不存在(只看最新) |
| `ByTime`(标记价、资金费、只有时间戳的分档快照) | `ex_ts ≤ last_fwd` → 丢 | 否则转发 | 不存在 |
| `Chain`(增量深度,单路;可选扩展) | `last_id ≤ last_fwd` → 丢(重订阅后的回放) | `prev_id == last_fwd` → 转发 | 否则 → **Resync** |

`Chain` 的 `prev_id/last_id` 是 venue 在 `peek` 里归一化好的值(与 `L2Batch` 和 [hft-book](../hft-book/README.md) 的比较同义),所以这一行对所有交易所是同一段代码。BBO 流不参与重同步(每条都是完整一档);分档快照流也不参与(每条都是完整的前 N 档,§3c)。

#### 增量深度与快照重同步(可选扩展)

`feed.depth = incremental` 时才启用;core 上线不实现。触发条件:某个策略需要全深度(分档快照的档数不够)。启用前该 venue README 的"L2 序号归一化"一节与本节的测试行必须全绿。

**重同步状态机**(每 inst,在承载 Md 路由的任务里):

```
           断裂 / 新订阅 / 该 inst 所在连接重连 / FeedCmd::Resync
  Live ─────────────────────────────────────────────▶ Resync ── 推 ConnState{Resync, inst};占一个重同步槽(无空槽则排队)
   ▲                                                    │ ResyncMethod::RestSnapshot:推 SnapReq{L2, inst} 到 snapreq 环(§3b)
   │                                                    │ ResyncMethod::Resubscribe:发退订 + 订阅该 inst(首帧即快照,在本任务解码)
   │                                                    ▼
   │                                              AwaitSnapshot ── 期间到达的增量解码后存入槽缓冲(≤ resync_buf)
   │                                                    │ 快照到达(snap 记录 / 流内首帧快照)
   │                                                    ▼
   │  推 L2Snapshot(多片)→ 按 align_first 丢弃/应用缓冲 ── Aligning
   │  → 推其后连续的 L2Batch → 推 ConnState{Up, inst}     │ Align::Gap / 缓冲溢出 / snap 记录为失败
   └────────────────────────────────────────────────────┘ ──▶ 退避重试(feed.snapshot_backoff_ms);n > 5 → Degraded(告警)
```

1. 进入 Resync 的那一刻起,该 inst 的 L2 增量不再转发给 sequencer(sequencer 已把簿置 `Stale`,[04 §6](../../design/04-行情接入层.md))。
2. 快照的 `snap_id` 与缓冲增量的原始序号按 `P::align_first(snap_id, &raw)` 对齐:`Drop`(快照已含)→ 丢;`Apply` → 该条以 `RxMeta.prev_override = snap_id` 解码(其 `L2Batch.prev_id = snap_id`,满足 hft-book 的 `prev_id == last_id`),之后按 `Chain` 连续;第一条就 `Gap` → 快照太旧,重试。REST 快照的分片由 snap-fetch 已按 bulk 环的记录格式写好(每片 ≤ `feed.snapshot_part_levels` 档),本任务原样拷进 `md-<venue>-bulk` 并推 `FeedMsg::Bulk`。
3. REST 快照的请求、读取、解密与解析都在 snap-fetch(warm);本任务只做原文 `RawRx` 追加、分片拷贝与对齐(§3b,预算见"延迟与资源契约")。一次 `poll_once` 至多处理一条 snap 记录,把重同步风暴摊开。
4. **重连必须伴随重同步**,没有例外:某深度连接被 `install` 时,其上全部 inst 从 Resync 开始。
5. L2 双路合并是这一扩展之上的再一层可选扩展,规格与启用条件见开发备忘。

### 3a. BBO 连接健康

报价能否用于对冲腿、风险递减单、自动平仓与链接腿检查,按**连接健康**判定(SDK `BboView::quote_live`;阈值 `conn_liveness_ms` 默认 2 000,归 [hft-risk](../hft-risk/README.md)):安静的 inst 盘口几十秒不变仍然可用,只要它所属的某条 BBO 连接活着、且该 inst 不在补帧。本节规定 BBO 任务产出哪些记录;活性证据的来源与发布规则归 [event-model 关键事件体](../../docs/subsystems/event-model.md#关键事件体),sequencer 据此按 (venue, 槽) 维护连接活性、按 inst 维护成员位图并投影 `BboView`(维护规则归 [hft-sequencer](../hft-sequencer/README.md#连接活性),字段语义归 [hft-strategy-sdk](../hft-strategy-sdk/README.md))。

每条 BBO 连接(`RouteKind::Bbo` 路由,连接号 = 槽号)一份:

```rust
struct BboConnLive {
    last_rx_tsc: u64,     // 本连接最近一次收到任一完整 WS 帧的 rx_tsc:数据帧(含赛马中被丢弃的重复帧)、Pong、Ping、应用层 pong/心跳
                          // (hft-net Conn::last_rx_tsc())
    pub_tsc: u64,         // 本连接最近一次向 bbo 环发布的记录(Bbo、Alive、连接级与成员 ConnState)所带的 rx_tsc;赛马中被丢弃的重复帧不算
    probe_tsc: u64,       // 下次探测时刻
    insts: InstBitset,    // 本连接上订阅回执已齐的 inst(局部下标位图,启动期按 venue inst 上限分配)
}
```

1. **探测**:每 `feed.bbo_ping_ms`(1 000)对每条 BBO 连接发一次 `P::bbo_probe()`(默认 WS Ping,payload = tsc),与有无数据无关——健康连接每秒至少有一帧回帧。探测计入该连接的入向消息限额:订阅帧的节奏取生效 `ws_max_in_msgs_per_s` 的一半再减去探测频率。
2. **连接与成员**:`install` 后推 `ConnState{Up, role=Md, conn, inst = 0}`(连接级);订阅回执齐后,对该连接上每个新加入的 inst 推一条成员事件 `ConnState{Up, role=Md, conn, inst}`;退订回执后推 `ConnState{Down, role=Md, conn, inst}`。整条连接退役推 `ConnState{Down, conn, inst = 0}`,之后该槽才可复用(§1)。一个 inst 可以同时是多条 BBO 连接的成员(双路赛马、计划轮换的重叠期),成员关系就是 sequencer 的 `bbo_member_mask` 的来源。
3. **存活**:每收到一帧探测回帧(Pong 或应用层 pong),若 `该回帧的 rx_tsc − pub_tsc ≥ feed.bbo_ping_ms / 2`(本连接这段时间没有向环发布任何记录),推一条 `ConnState{Alive, role=Md, conn, inst = 0, rx_tsc = 该回帧的 rx_tsc}`;否则不推——转发的 `Bbo` 本身带 `h.conn` 与 `h.rx_tsc`,就是该连接的存活证据。因此每条连接至多每秒一条 `Alive`,繁忙时为零。sequencer 取 inst 的全部成员连接中 up 者的最大存活时刻;健康连接上 `now − conn_live_tsc` ≤ `bbo_ping_ms` + 往返 ≈ 1.05 s(fstream 的 Pong 批处理另加约 50 ms),低于 `conn_liveness_ms`。
4. **静默**:超过 `feed.bbo_dead_ms`(3 000)没有任何入向帧(从未收到入向帧的连接从 `install` 起算)→ 推 `ConnState{Degraded, reason=BBO_SILENT}`,随后按致命处理(retire、`ConnState{Down, inst = 0}`、补建);赛马池只剩一条 → `RACE_SINGLE`(只在 `feed.bbo_conns ≥ 2` 时),报价照常可用。
5. **补帧**:某 inst 的全部 BBO 连接都断过(期间可能错过报价变化)→ 新连接订阅回执齐时推 `ConnState{Resync, role=Md, conn, inst, reason=BBO_REFRESH}`;该 inst 下一条被转发的 `Bbo` 即结束补帧(sequencer 规则)。回执齐后满 `feed.bbo_refresh_grace_ms` 交易所仍未推送的 inst(补帧失败的,从失败记录到达时再计一个宽限期),若本所支持 BBO 补帧(`FeedRuntime::new` 以 refdata 中本 venue 第一个 inst 调用一次 `P::bbo_snapshot_request`,返回 `Ok(true)` 即支持,结果缓存;运行中的请求只由 snap-fetch 生成),推 `SnapReq{Bbo, inst}`;原始响应写本任务的 raw journal。补帧中的 inst 的 `Bbo`(流上的或 snap 记录带回的)按 `update_id ≥ last_fwd` 转发,第一条被转发即结束补帧;snap 记录带回的 `update_id = 0`(响应无序号)直接转发且不改 `last_fwd`;该 inst 已结束补帧时 snap 记录带回的 `Bbo` 丢弃并计数。订阅即推首帧的所(返回 `Ok(false)`)等交易所推首帧([hft-venue-proto 契约 24](../hft-venue-proto/README.md#实现方必须遵守的契约))。只要还有一条连接在线,就不进入补帧。
6. 这些记录都是 Input:回放时 `BboView` 的健康字段逐字节重现。深度、成交与标记价连接的健康不影响 `BboView`;这些连接收到 pong 时按 event-model 的同一规则产出 `Alive`,走 md 环;它只是活性证据,不改簿状态、不入队给策略(处理规则归 [hft-sequencer](../hft-sequencer/README.md))。

### 3b. 快照获取(snap-fetch)

`SnapFetchTask<P>` 在 warm 线程上执行(线程计划归 hft-core),拥有该 venue 的公共行情 REST 传输(`account::Transport`,生产为 `hft_net::rest::Client`;连接号段见 §1),以及**自己的 `P::FeedCtx` 实例**([hft-venue-proto 契约 25](../hft-venue-proto/README.md#实现方必须遵守的契约))。core 上它只做 **BBO 补帧**;`kind = L2` 的请求只在该 venue 启用增量深度时出现。

```rust
#[repr(C)] pub struct SnapReq {          // 16 B,snapreq 环元素(进程内,不进 journal)
    pub kind: u8,                        //  0 L2 = 1、Bbo = 2
    pub _pad: u8,                        //  1
    pub attempt: u16,                    //  2 第几次尝试(行情任务的退避计数)
    pub inst: InstId,                    //  4
    pub req_id: u32,                     //  8 行情任务内单调,snap 记录原样带回
    pub _pad2: u32,                      // 12
}
#[repr(C)] pub struct SnapRecHdr {  // 40 B;后接 body_len 字节原始响应,成功时再接一个 Bbo
    pub rx_tsc: u64, pub sw_rx_ns: i64,
    pub req_id: u32, pub inst: InstId, pub body_len: u32,
    pub status: u8,  // 1=OK, 2=HTTP, 3=TIMEOUT, 4=DECODE, 5=TOO_LARGE, 6=RATE, 7=REQUEST
    pub kind: u8,    // 1=L2, 2=BBO
    pub conn: ConnId, pub _pad: u8,
    pub http_status: u16,  // 0=无响应;其余为 HTTP 状态码
    pub _pad2: u16, pub _pad3: u32,
}
```

1. **取请求**:从 `snapreq-<venue>-bbo` 与(存在时)`snapreq-<venue>` 取 `SnapReq`;在途数 ≥ `snap.parallel` 时留在本地 FIFO。连续的 `Bbo` 请求合并:以 FIFO 中连续的至多 min(`SNAP_BBO_BATCH_MAX`, `P::BBO_SNAPSHOT_BATCH`) 条调用 `P::bbo_snapshot_request`,返回 `Err(BufFull)` 且条数 > 1 时条数减半(向下取整)重试,1 条仍返回 `Err` → 该 `SnapReq` 得一条 `status = ReqErr` 的记录;响应按 inst 拆回,每条 `SnapReq` 仍各得一条记录(原始响应体只随其中第一条记录携带)。
2. **发请求**:`P::bbo_snapshot_request` / `P::snapshot_request` 生成 `RestReqBuf`(`endpoint` 下标规则同 §4a)→ `RestPool::submit`。REST 权重按 refdata `RateLimitTable` 计入本实例行情出口 IP 的计数;余量不足时延后发送,不失败;交易所返回限速(429/418 类)→ `status = RateLimited`。
3. **收响应**:`P::decode_bbo_snapshot(rd, &mut ctx, insts, body, rx, &mut sink)`(`insts` = 该请求覆盖的 inst,同序)/ `P::decode_snapshot(rd, &mut ctx, inst, body, rx, &mut sink)`;sink 把 `Bbo`(或按 `feed.snapshot_part_levels` 切好的分片)写进记录的尾部。头 + 原始响应体 + 尾部作为一条记录写入对应的 snap 环;环里放不下时暂缓,不再读该连接的下一个响应,直到有空位。单条记录 > 该环容量 / 4 → 只写头,`status = TooLarge`(该 venue 必须改用更少档数)。
4. **失败**:HTTP 非 2xx、超时(hft-net `rest_timeout_ms`)、解码失败 → 只写头与可用的响应体,`status` 标原因;是否重试、何时重试由行情任务决定(BBO 补帧在下一个 `feed.bbo_refresh_grace_ms` 再判;增量深度按 `feed.snapshot_backoff_ms`)。每条 `SnapReq` 恰好得到一条 snap 记录。
5. 调度延迟:请求在 snap-fetch 下一次被调用时才发出,上界 = warm 线程的 park 超时(§1);BBO 与深度的热路径不受影响。

### 3c. 分档快照(DepthTopN)

深度连接上的整簿流若同时推增量(交易所不给序号,venue 为每个 inst 合成连续的 `prev_id`/`last_id`,如 [venue-polymarket](../venue-polymarket/README.md#合成序号)),增量以 `L2Batch` 变长体写入 bulk 环(`L2BatchHdr` + 先买后卖的 `[Level; n_bids + n_asks]`);一帧内档位调用与头部计数不符或买卖顺序颠倒 → 整帧按解码错误丢弃。

core 的深度来源。venue 在 `routes()` 里给出 `RouteKind::DepthTopN` 路由(Binance `depth<N>@100ms`、OKX `books5`、Bitget `books5`/`books15`;频道名、档位与节拍以各 venue README"分档快照流"一节为准,均待核实,方法见各 venue 的开发备忘),feed-md 只为带 `BOOK` 位的 inst 订阅它,档数按 `feed.topn_levels` 选择。

1. 每条消息是该 inst 前 N 档的完整快照:`decode_md` 经 `MdSink::l2_snapshot` 交出 `L2Snapshot{origin = TopN}`([hft-venue-proto 契约 26](../hft-venue-proto/README.md#实现方必须遵守的契约)),sink 按 `feed.snapshot_part_levels` 切片(N ≤ 64 时恰好一片)写入 `md-<venue>-bulk` 并推 `FeedMsg::Bulk`;不产出 `L2Batch`。
2. 去重按 `seq_rule`(`Snapshot`:交易所序号单调;`ByTime`:只有时间戳),写在 `InstSeq.last_fwd` 的深度槽;**没有序号链、没有断裂判定、不发 REST 快照**。簿按整替换应用(hft-book `apply_snapshot`;sequencer 侧规则归 [hft-sequencer](../hft-sequencer/README.md))。
3. **状态**:新订阅 → 推 `ConnState{Resync, inst, reason=NEW_SUB}`,`l2_state = AwaitFirst`;该路由的连接断开或重连 → 其上每个 inst 推 `ConnState{Resync, inst, reason=CONN_LOST}`(多连接时见第 5 项);`FeedCmd::Resync{inst}` → `ConnState{Resync, inst, reason=REQUESTED}`。三者的 `conn` 是该 inst 深度组的一条连接,组内尚无连接时为 `MD_FIRST`,从不取 BBO 槽号(sequencer 把 `conn < MAX_BBO_CONN_SLOTS` 的 `Md` 状态当作 BBO 连接,置该 inst 的 BBO 重同步标志,而只有 BBO 记录能清除它)。三者都在该 inst 下一条被转发、且解码出事件的深度帧之后推 `ConnState{Up, inst}`,`l2_state = Live`。重连后的第一条快照本身就是恢复手段。
4. `ResyncMethod` 对 `DepthTopN` 类恒为 `None`;解码失败只计数并推 `ConnState{Resync, inst, reason=DECODE}`,等下一条快照恢复。
5. **多连接**(`feed.depth_conns ≥ 2`):每组保持该数目的活跃连接,订阅相同(组内每条连接都订阅组内全部 inst;断开的连接按第 8 步补建,补建前组内连接少一条)。同一条交易所消息从每条连接各到一次:有序号的流按 `seq_rule` 在 `InstSeq.last_fwd` 去重(第 2 项);`SeqRule::None` 的流(如 [venue-polymarket](../venue-polymarket/README.md#多连接去重))由协议在 `decode_md` 里去重,重复的帧解出 0 个事件,不推送、不结束 `AwaitFirst`。一条连接断开时,其上的 inst 若同组还有已应答其订阅的连接,则保持 `Live`、不推 `Resync`;组内只剩一条连接时对它推 `ConnState{Degraded, reason=RACE_SINGLE}`;最后一条持有该 inst 的连接断开才推 `Resync{CONN_LOST}`。

### 4. `GwRuntime::poll_once` 的循环结构

```rust
struct GwAccount<P: VenueProtocol> {
    account: AccountId,           // 只能是 AccountKind::Live;Sim 账户在 new() 时拒绝(InitErr::SimAccount)
    kill_ix: u8,                  // refdata venue_account_ix(该账户在本 venue 的稠密序号 0..11);acct_bits = account(ix) | account_hard(ix)
    acct_bits: u32,               // new() 时算好
    pool: ConnPool<3>,            // PrimaryStandby:primary + 已登录 standby + 在建
    user: Option<ConnId>,         // 独立的用户数据流连接(交易所要求时,如 Binance USDT-M listenKey 流)
    priv_owner: Option<ConnId>,   // 当前持有私有频道订阅的连接(§4b);None = 私有频道缺口
    rest: [RestPool<2>; MAX_REST_ENDPOINTS],   // 本 crate 常量 = 4;gw 用到的 REST 端点各一个池:订单查询、REST 型死手、listenKey 续期、下单兜底(账户查询在 acct-fetch,§4a)
    tpl: P::Templates, ctx: P::GwCtx,   // 模板启动期构建一次;ctx 含去重环(契约 22)与推送型保证金缓存
    ids: IdTable,                 // exch id → (epoch, slot, gen),私有;含终态保留条目(§6)
    hold: HoldBuf,                // 解析不到槽的成交类 OrderEvent 暂存(≤ gw.unmatched_hold_cap,预分配)
    acct: AcctSched,              // 账户查询的调度状态(§4a)
    inflight: [u16; 3],           // 每连接已发未回的请求数
    rate: RateCounters,           // 发送侧计数,按 (venue, account) 独立(本账户的 API key 与出口 IP),响应 rateLimits 按窗口校正,不降低本地已记账的用量(未匹配窗口按占用比例保守折算);一个账户的限速风暴不消耗另一个账户的配额
    frame: MaskedFrame<CAP>, warm_frame: MaskedFrame<CAP>,
}
```

稳定 Rust 不支持以关联常量作泛型实参,所以实现为 `GwRuntime<P, const CAP: usize>`,`new` 时断言 `CAP >= P::ORDER_FRAME_CAP`;hft-core 为批量下单的所(OKX)选 8192,其余 2048。

**一圈的顺序**:对每个账户——

1. **primary 下单连接**:`poll_rx`,再逐帧 `next_frame` → 处理回报(第 2 步)→ **每处理完一帧立即 `drain_out()`**(第 1 步);无帧时 poll 完也 `drain_out()` 一次。
2. **独立 UserStream 连接**(有的话,如 Binance USDT-M 的成交只从这里来):**每圈都 poll**,规则同上(每帧之后 `drain_out()`)。
3. 每 `gw.aux_every` 圈:standby 连接,以及 gw 的 REST 池中有在途请求的连接(`RestPool::poll` 只读这些),**每 poll 完一个 socket `drain_out()` 一次**。没有在途请求的 REST 连接不读,每 `gw.rest_idle_check_ms` 做一次存活检查(`RestPool::idle_check`,同样每个 socket 之后 `drain_out()` 一次;空闲达 hft-net `rest_max_idle_ms` 的连接在此退役,退役与补建同对端关闭:`take_retired` 交回 conn-builder,`next_build` 发 `BuildReq{role: Rest}`,交接后装入池;REST 请求不重发)。gw 不用就绪探测(`epoll_pwait(0)` 只给行情线程,[hot-path-rules T7](../../docs/hot-path-rules.md))。

全部账户处理完后:**acct 记录**——至多应用一条(§4a),排在本圈最后一次 `drain_out()` 之后。这样出环里的订单最多等一条回报帧的处理时间(或一个空 socket 的 poll、一条 acct 记录的应用),而不是一整批回报或一整圈;对冲第二腿的主机内等待计入 `hedge_host`([latency-budget](../../docs/latency-budget.md))。

1. **`drain_out()`**:以 `read_batch` 取得当前已发布的 `GwCmd`(≤ `cmds_per_loop` 与可用空间)，复制到预分配批缓冲后一次 Release 归还环槽。**本批内重排**:稳定地把 `oflags & RISK_REDUCING` 的 `Place`/`Amend` 移到批首,其余保持原序;例外——目标槽(`target_slot`)在同批更早一条 `Place` 中的 `Cancel`/`Amend`/`Query` 不越过那条 `Place`(这样的 RR `Amend` 留在原位)。每账户每轮至多 `gw.cmds_per_account_per_loop` 条,超出的留在本地暂存(容量 = `cmds_per_loop`)下一轮先发(每次 `drain_out` 先发暂存中这一圈配额未用完的账户的命令,再从出环取至多 min(`cmds_per_loop`, 暂存空位) 条),使一个账户的突发不推迟另一个账户。跨批的优先级与 (venue, account) 配额由 sequencer 侧 OMS 的扇出队列保证([hft-oms](../hft-oms/README.md) 的输出扇出)。维护类 `op` 按"出环"表处理;订单类 `c` 按下列步骤:
   1. **kill 检查**(仅 `op ∈ {Place, Amend}`;位定义归 [hft-types `KillBits`](../hft-types/README.md),规则归 [hft-risk kill 字](../hft-risk/README.md#kill-字唯一的跨线程共享可变量)):`k = kill.load(Acquire) & (kill_mask | acct.acct_bits)`(`u32`;`acct` = `c.account` 的 `GwAccount`);`k & (HARD | account_hard(ix)) != 0` → 一律拒(含 `RISK_REDUCING`);否则 `k != 0` 且 `c.oflags & RISK_REDUCING == 0` → 拒;拒绝回 `SendReport{status: KillBlocked}`。判定是纯函数 `pub fn kill_admit(word: u32, mask: u32, acct_bits: u32, ix: u8, op: GwOp, oflags: u16) -> bool`,gw 与 hft-core 的启动自检调用同一个函数。`RISK_REDUCING`(对冲第二腿、平仓、HedgeFix)由 sequencer 置位,gw 只看位(置位规则在 [hft-oms](../hft-oms/README.md#risk_reducing-判定))。`Cancel`、`CancelAll`、`Query` 与维护类 `op` 永远放行。
   2. **限速兜底**:该 (venue, account) 的发送侧计数显示会超交易所硬上限 → `SendReport{status: RateLimited}`(风控令牌桶应先拦住,这里是最后一道)。同时检查该账户所有 Account/Uid 的 Orders 窗口,任一窗口不足则全部窗口不扣额度。延迟响应只提高计数,不释放本地已占配额;ResetRate 清零。计数按 (venue, account) 分开;交易所按 IP 计的限额以每账户独立出口 IP(`venues.<v>.gw.src_ip`)隔离,共享 IP 的配置加载时拒绝。
   3. 选连接:该账户 primary;primary 不是 `Open` → standby;都没有 → `SendReport{status: NoConn}`。
   4. 从槽镜像取 `TargetRef`(Cancel/Amend/Query);`order_channel(op) = Rest` 的 op 走 `P::encode_rest` 并提交 REST 池(其后的第 5 步换成 `RestPool::submit`,`SendReport` 规则相同);其余 `P::encode(rd, tpl, &c, &tgt, &wall_clock, frame.body_mut())`:写入价/量/client id/请求 id/时间戳/过期字段;reduce-only 字段只在 `oflags` 带 `REDUCE_ONLY` 时填;`needs_per_order_signing()` 的 venue 在此签名(Hyperliquid)。`EncodeErr` → `SendReport{status: EncodeError}`;其中 **`op = Query` 无法编码**(槽镜像里没有该槽、venue 不支持按 client id 查询且没有 exch id)→ `SendReport{status: QueryUnsupported}`([hft-event](../hft-event/README.md#执行0x02xx) `SendStatus` = 8):OMS 保持该槽 `Unknown` 并转交 reconciler。
   5. `frame.seal(Text, len, mask)` → `conn.send_frame(&frame)`;推 `TxTag`(若启用);`inflight[conn] += 1`;New 写槽镜像。
   6. 推 `SendReport{decision_seq, sent_tsc, slot, gen, op, target_slot, target_gen, venue, account, conn, status: Written, os_errno: 0}`(`target_gen` 取自槽镜像;所有状态都带这三项);然后追加 `RawTx`(记录头 `tsc = sent_tsc`;含签名的帧 `rflags = REDACTED`,只记元数据)。
   `SendErr::NotOpen/TxBacklog` 保证未加密,换 standby 重试一次,仍失败 → `NoConn`;`SendErr::Io` → `SendReport{status: WriteError, os_errno}`:**密文已产生、结果未知**,sequencer 把该槽推入 `Unknown`([defensive-patterns](../../docs/defensive-patterns.md));该 Place 同样写槽镜像(之后的 Query 需要它)。
2. **回报**:`P::classify_report` → `P::decode_report(rd, ctx, c, payload, rx, sink)`。
   - **`OrderEvent` 的槽解析**:client id 判定为本系统的单(`P::parse_cid` 成功:规范文本合法、ver/校验位正确、venue/account 与连接一致,规则见 [hft-types `ClientOrderId`](../hft-types/README.md#clientorderid-编解码))→ 直接得 `(epoch, slot, gen)`;否则查 `IdTable`(条目存 epoch;终态保留条目同样命中,§6);都不行 → `slot = SlotId::UNRESOLVED`。`OrderEvent.epoch` 一律填解析出的 epoch,OMS 按 `(epoch, slot, gen)` 匹配(迟到成交落到旧代时由 OMS 的墓碑规则处理,见 [hft-oms](../hft-oms/README.md))。
   - **解析不到槽的成交**(`PartialFill`/`Fill`/`FeeOnly` 且 `slot = UNRESOLVED`):不立即入环,放进 `hold`(按 `exch_order_id` 索引)等待同账户后续 Ack 把该 exch id 写进 `IdTable`;命中即补上 `(epoch, slot, gen)` 按原顺序入环。超过 `gw.unmatched_hold_ms` 仍解析不到:成交类转为 `ExternalFill{cause: Unknown}`(字段取自该 `OrderEvent`:inst、side、本笔 qty、px、fee、fee_asset、exch_order_id、ex_ts_ns;保留 `trade_id`,缺失时以非负 `cum_filled` 填 `fill_seq`;不从累计 `cash_sum` 猜测本笔现金,`cash_qty/cash_amount` 为 0;缺少有效买卖方向时保留 `UNRESOLVED` 原事件),`FeeOnly` 原样以 `UNRESOLVED` 入环(OMS/ledger 按 `(venue, account, exch_order_id)` 入账费用)。非成交类的 `UNRESOLVED`(他人订单的 Ack/撤单)直接入环,OMS 记 `ForeignOrder`。venue 已按 [hft-venue-proto 契约 18](../hft-venue-proto/README.md#实现方必须遵守的契约) 直接产出的 `ExternalFill`(强平、ADL、交割、可确认的非本系统 client id)不经过 `hold`,立即入环。
   - **可忽略与未知事件**:`ReportOutcome.skip = Listed` → 计数 `user_evt_ignored{venue}`;`Unknown` → 计数 `user_evt_unknown{venue}`,第 1 条与第 k × `gw.unknown_evt_note_every` 条(k ≥ 1)各写一条 Meta `Note`(带事件名前 32 字节),watchtower 据计数报 P2。两者都不产生 `ConnState`、不置 VERIFY、不查询;原帧照写 `RawRx`。
   - **`DecodeErr`(类别已知、内容解析失败)**:该帧照写 `RawRx`,推 `ConnState{Degraded, account, role, reason=REPORT_DECODE}`——OMS 按用户流缺口处理,把该 (venue, account) 的非终态槽置 VERIFY 并查询;同一连接 60 s 内连续 `gw.report_decode_down_after` 次 → retire 该连接并推 `ConnState{Down}`。
   - **控制提示** `ReportOutcome.ctl`([hft-venue-proto 契约 30](../hft-venue-proto/README.md#实现方必须遵守的契约);`role` 为收到该帧或响应的连接角色):`ClockSkew` / `RateLimited` → 推 `ConnState{Degraded, account, role, reason = CLOCK_SKEW | RATE_LIMIT}`,连接不退役(`Note{RateLimitHit}` 与令牌桶处置在 sequencer 处理该 `ConnState` 时由 [hft-risk](../hft-risk/README.md) 给出);`UserStreamEnded` → 推 `ConnState{Down, account, role=UserStream}`,有独立用户流连接的退役并补建(conn-builder 重新 `P::bootstrap` 取新会话),私有频道在下单连接上的对归属连接重发 `P::private_subscribe`,回执齐后按 §4b 强制切换的同一规则推 `Up` 并排查询;`ServerShutdown` → 按第 5 步提前计划切换。
   - 批量响应中结果未知的条目由 venue 直接产出 `OrderEvent{what: OutcomeUnknown}`(契约 8),原样入环。
   - `ReportOutcome.req` → `inflight -= 1`;`ReportOutcome.rate` → 校正计数;`ReportOutcome.outcome_unknown` → 不产出 Reject,推一条 `OrderEvent{what: OutcomeUnknown, slot, gen, reject_code = ReportOutcome.raw_code}`([hft-event](../hft-event/README.md#执行0x02xx);码的编码见 [hft-venue-proto 契约 8](../hft-venue-proto/README.md#实现方必须遵守的契约)),OMS 把该槽置 `Unknown` 并查询;`ReportOutcome.hint` → 交给 §4a 立即排一次 Margin 或 Config 查询;`ReportOutcome.fee_rule_deviation` → 计数并写 Meta `Note`。成交类 `OrderEvent` 的 `cum_filled`/`avg_px` 由 venue 给出或在其 `GwCtx` 内按成交 id 合成(契约 9)。每帧追加 `RawRx`。
   - **推送型保证金数据**(`margin_data = Push`):账户与持仓推送在这里由 `decode_report` 直接产出 `MarginReport`(venue 在 `GwCtx` 里合并账户级与持仓级推送),gw 只更新 §4a 的推送时刻;最大推送类型的解码预算见"延迟与资源契约"。
3. `RestPool::poll`(每 `aux_every` 圈,只读有在途请求的连接):每个响应以提交时记下的 `RestOrigin` 调 `P::decode_rest`,输出与 `ReportOutcome` 按第 2 步处理;查询结果(`Unknown` 收敛)→ `OrderEvent{what=QueryResult}`(带 `cum_filled`、`avg_px` 与 `detail` 订单状态码);`RestPool::poll` 返回 `RestFail`(超时、断开)→ 该请求按无响应处理(订单类 → `OrderEvent{what: OutcomeUnknown, reject_code = 0}`);REST 死手响应失败 → `ConnState{Degraded, account, role=Order, reason=DEADMAN}`,会话维护(listenKey 续期)失败 → 同一 reason、`role=UserStream`([05 §8](../../design/05-CEX执行层.md):视为连接异常)。对 `stale_guard()` 为 `None` 或 `RecvWindow` 的所(如 Binance),查询"不存在"只作为提示交给 OMS,不据此定终态,由成交历史确认或对账收敛;`ExpireAt` 的所只有非对冲、非链接槽可在窗口 + grace 后定终态([hft-venue-proto 契约 10](../hft-venue-proto/README.md#实现方必须遵守的契约),判定在 [hft-oms](../hft-oms/README.md))。
4. `ErrBits`:primary 致命 → **强制切换**(standby 已登录则 `swap_primary`),推 `ConnState{Down, conn, account, role=Order, reason}`,在途请求由 sequencer 推入 `Unknown` 并经新 primary 查询收敛;私有频道归属在旧 primary 上时按 §4b 的强制切换处理;standby 致命 → `retire`、推 `ConnState{Down, conn, account, role=Order, reason}` 并补建;UserStream 致命 → `ConnState{Down, role=UserStream}` 并补建。下单与独立用户流连接 `install` 后推 `ConnState{Up, conn, account, role}`(hft-core 启动闸门 `ORDER_CONNS` 读它)。
5. **计划切换**:primary `rotate_due`(计划轮换时刻按 `conn.rotate_margin_s` 规则)、收到 `ServerShutdown` 提示或 `GwCmd{Rotate}`,standby `Open`,且 `inflight[primary] == 0` → 先按 §4b 把私有频道迁到 standby(先建后断),再 `swap_primary`,推 `ConnState{Rotated, conn = 新 primary, account, role=Order, reason}`(`reason` = 触发它的 `ROTATE_DUE` 或 `SERVER_HINT` 位,`GwCmd{Rotate}` 为 0);旧 primary 再等 1 s 收尾回报后 `retire`。满 `rotate_wait_max_ms` 仍有在途 → 放弃本次,再过 `rotate_wait_max_ms` 重新开始等待；该圈响应清空在途时仍可切换。错峰到期的 standby 只退役并补建，不触发 primary 切换。
6. 心跳(§5);`inflight` 超时检查;`hold` 超时检查(第 2 步);账户查询调度(§4a)。
7. **预热(可选,默认关)**:`gw.warm_interval_ns > 0` 时,空闲超过该间隔用一条固定的假 `GwCmd{op: Place}` 走 `encode → seal`(写 `warm_frame`)→ `primary.warm(&warm_frame)`。不发送、不推任何事件。启用条件:统一升级主机上 `egress` 的冷热差测量证明值得。

**死手**:刷新由 sequencer 的 `DeadMan` 定时器发出 `GwCmd{op: DeadManRefresh, arg: timeout_ms}`([05 §5](../../design/05-CEX执行层.md)),gw **不自行刷新**——sequencer 卡死时刷新自然停止,交易所侧撤单生效。按 inst 计时的所只刷新槽镜像中有活动订单的 inst,所以 Admin 冒烟单(`OrderFlags::SMOKE`,占用槽镜像)所在的 inst 同样被刷新,停掉 hft-core 后由交易所撤单。`DeadManSpec::None` 的所由 [kill-switch](../../apps/kill-switch/README.md) 兜底。

### 4a. 账户查询:保证金轮询与配置核验

**分工**:gw 决定何时查(调度、优先级、限速记账、回报给出的提示、会话 UID 的并入);warm 任务 `acct-fetch` 的 `AcctFetchTask<P>` 执行请求、读取、解码与汇总,把原始响应与切好的报告经 `acct-<venue>` 环交回 gw;gw 写 raw 并把报告原样推入 `rep-<venue>`。结果以 Input 事件进 `rep-<venue>` 环(由 sequencer 赋 seq 并记录,回放直接读这些事件,不重放 REST)。账户查询的解码因此不在 gw 线程上;它不改任何状态,只产出事件;风控与 ledger 如何使用见 [hft-risk](../hft-risk/README.md)(`ACCOUNT_CFG`、`IDENTITY` 闸门位,`MarginStress`)与 [hft-ledger](../hft-ledger/README.md)。

**REST 端点表**:`RestReqBuf.endpoint` 是该 venue REST 端点表的下标。表的顺序、用途标签与默认主机由该 venue README 的端点表给出(下标 0 = 主端点;例:Binance USDT-M 的身份与 key 权限查询指向现货 REST 主机);hft-core 配置 `venues.<v>.gw.rest_endpoints` 只能按同一顺序覆盖主机,条目数或用途标签与 venue 默认表不符 → 启动退出 2。gw 与 acct-fetch 对各自用到的端点各持 `RestPool`(hft-net 一个池只连一个端点),各保持 keep-alive 连接;acct-fetch 的连接用该账户的出口地址(key 白名单)。snap-fetch 用同一张表。

```rust
struct AcctSched {                    // gw 侧,每账户一份
    state: AcctState,                 // Idle | Running { kind, seq_no, sent_tsc } | Backoff { until_tsc }
    next_margin_tsc: u64,             // Poll:上次完成 + interval;Push:max(最后一次推送, 上次兜底完成)+ gw.margin_push_stale_ms
    next_cfg_tsc: u64,                // 上次完成 + gw.acct_cfg_recheck_s
    poll_no: u32,                     // AcctQuery.every = N 的请求只在 poll_no % N == 0 时执行(从 0 起)
    pending_hint: AcctHint,           // 回报给出的 MarginNow / ConfigRecheck,下一个空闲时机执行
    session: SessionUid,              // 交接带来的会话 key UID(§7 第 5a 步),随下一条 Config 请求交给 acct-fetch
    fails: u8,                        // 连续失败的序列数
}
#[repr(C)] pub struct AcctReq {        // 40 B,acctreq-<venue> 环元素(进程内,不进 journal)
    pub kind: u8,                      //  0 AcctQueryKind:Config = 1、Margin = 2
    pub account: AccountId,            //  1
    pub why: u8,                       //  2 Scheduled = 1、MarginNow = 2、ConfigRecheck = 3、PushStale = 4、Startup = 5
    pub session: u8,                   //  3 会话 key UID:0 = 不带(同一把 key 或非 Config)、1 = Ok、2 = Failed
    pub seq_no: u32,                   //  4 gw 内单调,acct 记录原样带回
    pub poll_no: u32,                  //  8
    pub _pad: u32,                     // 12
    pub session_uid: [u8; 24],         // 16 ASCII,右补 0
}
pub struct AcctRecHdr {                // acct-<venue> 环记录头(进程内,不进 journal);其后 req_len + body_len 字节
    pub seq_no: u32, pub account: AccountId,
    pub what: u8,                      // Raw = 1(一个请求的原始响应的一块;首块另带请求元数据)、Done = 2(报告)、Failed = 3
    pub kind: u8,                      // AcctQueryKind
    pub step: u8,                      // Raw:序列内第几个请求
    pub part: u8, pub parts: u8,       // Raw:本块序号(0 起)与该响应的总块数;每块 ≤ ACCT_RAW_CHUNK(本 crate 常量 = 16 KiB)
    pub fail: u8,                      // Failed:Timeout = 1、HttpErr = 2、Decode = 3、TooLarge = 4、RateLimited = 5
    pub conn: ConnId,                  // acct-fetch 的 REST 连接号(写 raw 用)
    pub http_status: u16,              // Raw:HTTP 状态码原值;0 = 无响应
    pub sent_tsc: u64,                 // Raw:请求发出时刻(RawTx 的记录头 tsc)
    pub ts: Timestamps,                // Raw:响应完成时的接收戳(RawRx)
    pub req_len: u32,                  // Raw 首块:请求元数据(方法 + 路径,不含签名与签名头);其余块 0
    pub body_len: u32,                 // Raw:本块响应体;Done:按 hft-event 分条规则切好的 RepMsg 序列(与 gw 写 rep 环的格式相同);Failed:0
}
```

1. **何时开始一个序列**(gw,每 `aux_every` 圈检查一次,同一账户同时至多一个序列在途):
   - **Config**:该账户的下单连接与私有频道首次就绪后立即执行一次(启动闸门 `ACCOUNT_CFG` 等它);之后每 `gw.acct_cfg_recheck_s`;收到 `ConfigRecheck` 提示时尽快;每次会话重建(切换、重连)交接了新的会话 UID 后尽快(`why = ConfigRecheck`)。
   - **Margin**:按生效 `VenueCaps.margin_data`。`Poll{interval_ms}`:每 `max(interval_ms, gw.margin_poll_min_ms)` 一次,`AcctQuery.every = N` 的请求每 N 次才带上;`Push`:正常不轮询,超过 `gw.margin_push_stale_ms` 没有账户/持仓推送时执行 `push_fallback` 请求一次;收到 `MarginNow` 提示(如 Binance `MARGIN_CALL`)时尽快。现货产品线的 Margin 序列为空,不执行。
   - 优先级:`MarginNow` > `ConfigRecheck` > 到期的 Margin > 到期的 Config。
   - 开始时 gw 把本序列将执行的请求的 `AcctQuery.weight` 之和记入该账户的 `RateCounters`,推一条 `AcctReq`(Config 请求带 `session` 与 `session_uid`)。
2. **执行**(acct-fetch):逐个 `P::acct_request(rd, tpl, acct, q, clk, out)` 生成签名请求(acct-fetch 持有该账户 REST key 的签名句柄,与 gw 的同一把 key)→ 按 `endpoint` 选池 `submit`;响应 → `P::acct_decode(rd, &mut actx, acct, q, status, body, &mut scratch)`(`actx` 是 acct-fetch 自己的 `P::AcctCtx`,[hft-venue-proto 契约 27](../hft-venue-proto/README.md#实现方必须遵守的契约))→ 响应体按 `ACCT_RAW_CHUNK` 切成 `parts` 条 `Raw` 记录(首块带请求元数据),使 gw 每次应用的 raw 追加有上界。最后一个响应解码后 `P::acct_finish(..., sink)`:Config → 一条 `AccountConfigReport`(头 + 永续的杠杆条目;Config 请求带会话 UID 时,运行时在 `acct_finish` 之后置 `uid_keys |= SESSION`,与 `AccountMeta.venue_uid` 不符或 `session = Failed` 时按 §7 第 5a 步的规则置位);Margin → 一份 `MarginReport` + 每 inst 一条 `PositionReport`;按 hft-event 的分条规则切好写成一条 `Done` 记录。响应体 > `gw.acct_body_max`、超时(`gw.acct_query_timeout_ms`)、非 2xx 或 `DecodeErr` → 写一条 `Failed` 记录,本序列作废(不产出部分报告);超过 `gw.acct_body_max` 与超时的响应只交回一条带请求元数据、体为空的 `Raw` 记录。单条记录 > `acct-<venue>` 容量 / 4 → 同样 `Failed{TooLarge}`。
3. **应用**(gw,每圈至多一条 acct 记录,排在本圈最后一次 `drain_out()` 之后):`Raw` → 首块追加 `RawTx`(请求元数据,`REDACTED`,记录头 `tsc = sent_tsc`)与 `RawRx`(`what = RestResponse`,本块),其余块只追加 `RawRx`;同一响应的各块因此是 raw-gw 中同一 `conn` 上紧随该 `RawTx` 的连续几条 `RawRx`(一连接一在途,读者按连接依次拼接);`Done` → 把 `RepMsg` 序列原样推入 `rep-<venue>`(+ bulk;同一份报告的分条在环中连续写入),`fails = 0`,此前为 Degraded 的推 `ConnState{Up, role=Rest}`;`Failed` → 退避 1 s × 2ⁿ⁻¹(n = 连续失败的序列数,≤ 30 s),连续失败数达到 `gw.acct_query_degraded_after` 时推一次 `ConnState{Degraded, account, role=Rest, reason=ACCT_QUERY}`。保证金数据因此变旧时,风控按 `margin_stale_ms` 视为 L1(规则在 [hft-risk](../hft-risk/README.md)),gw 不做判断。
4. **限速**:账户查询的 REST 权重计入本账户的发送侧计数(第 1 步预记),与查单、死手共用;计数显示余量 < 20% 时推迟 Margin/Config 序列(`MarginNow` 除外),不推迟订单查询与死手。
5. **推送型保证金数据**(`margin_data = Push`):不经 acct-fetch,由 gw 的 `decode_report` 直接产出(§4 第 2 步);gw 只更新 `next_margin_tsc`。
6. 调度延迟:`AcctReq` 在 acct-fetch 下一次被调用时才发出,上界 = warm 线程的 park 超时(§1);`MarginNow` 的响应因此多出至多一个 park 周期。

### 资金费账单轮询

首次请求必须满足 `end_ms > start_ms`。启动时刻加 `settle_lag_ms` 的边界仍无正宽窗口时返回等待，不保存请求、不标记账户读取失败；下一轮重新计算边界。失败后的重试仍保持同一个合法窗口，避免漏页。

`FundingPoll<P: FundingHistoryProtocol>` 是 warm 账户查询驱动,单账户同时只有一个在途页面。调用者以 `request` 取得签名 GET,向选定产品线的 REST origin 发送,在完成时调用 `response`;超时调用 `retry`。调度与 HTTP 的所有时刻由调用者传入,不读系统时钟。`FundingPollCfg` 的 `poll_ms`、`lookback_ms`、`settle_lag_ms`、`max_settlement_skew_ms`、`max_pages`、`max_bills` 均在加载时校验;`poll_ms` 同时限制分页与错误重试节拍,部署按该账户和 IP 的 REST 权重余量配置。查询每段不超过 30 天,`lookback_ms` 必须小于 30 天;起点由账本历史水位提供。

页面先校验后暂存,账单按 ID 去重;相同 ID 内容冲突、容量越界、循环游标或任意失败均保留原页面,不发布部分金额。完整窗口才按 refdata 的合约结算周期/锚汇总。账单偏离该网格超过 `max_settlement_skew_ms` 或周期未知时失败,需刷新 refdata,不猜结算时刻;`settle_lag_ms` 不小于此偏差上限。窗口开头未完整覆盖的旧结算不发布。后续窗口重叠回查迟到账单,账本按实额总额差分,见 [ledger](../hft-ledger/README.md#资金费)。

`publish_one` 每圈至多写一条 acct 环记录,环满时保留待发项;格式是 8 B `FUNDv001` + 56 B `FundingSettle`。gw 在本圈订单处理之后至多取一条,校验 venue/account/inst 与 `synth=0`,再写 rep 环,由 sequencer 先 journal 再入账。acct-fetch 不直接写 rep。此版本记录不含凭证或签名;通用 `AcctRecHdr` 查询族与 HTTP RestPool 的进程装配仍见已知限制。

### 4b. 私有频道归属与热备连接

私有频道 = 本账户的订单/成交、持仓、余额、保证金推送(Binance USDT-M 的 listenKey 流、Binance 现货 ws-api 上的 `userDataStream.subscribe`、OKX/Bitget `/private` 上的各频道)。

- **不变量**:每个 (venue, account) 的私有频道任一时刻只有一条**归属连接**(`priv_owner`)持有订阅,计划切换的重叠窗口除外。**热备(standby)连接登录、心跳,但不订阅私有频道**;在建连接同样不订阅。
- **独立用户流连接**(Binance USDT-M):`priv_owner = user`,与下单的 primary/standby 无关;它自己的轮换按下面的先建后断执行(新连接用同一 listenKey 或新 key,先收到首条事件或订阅回执,再退役旧连接)。
- **与下单同连接的私有频道**(Binance 现货 ws-api、OKX、Bitget):`priv_owner = primary`。
- **计划切换(先建后断)**:对新归属连接调 `P::private_subscribe(.., unsub = false)`,收到全部订阅回执后把 `priv_owner` 指向它,再对旧连接发退订(`unsub = true`)或直接随旧连接退役。重叠窗口内两路可能送达同一推送:成交、`FeeOnly`、`ExternalFill`、资金费结算推送由 venue 的去重环保证只产出一次([hft-venue-proto 契约 22](../hft-venue-proto/README.md#实现方必须遵守的契约)),`OrderEvent` 本身按 `cum_filled` 幂等,余额/持仓/保证金推送只做校验或取最新值,重复无害。新连接订阅被拒或 5 s 内回执不全 → 放弃本次切换(对新连接发退订,热备不保留私有订阅),保持旧归属,`gw.rotate_wait_max_ms` 后再试。交易所是否允许同一账户的私有频道在两条连接上同时订阅是先建后断的前提,M3 之前必须核实(开发备忘第 4 条);不允许的所退化为"先断后建 + VERIFY",并在该 venue README 写明。
- **强制切换**(归属连接致命断开):立即推 `ConnState{Down, account, role=UserStream}`(OMS 置 VERIFY 并查询,弥补缺口),在新的 primary(或重建的用户流连接)上订阅;订阅回执齐后推 `ConnState{Up, role=UserStream}`,并排一次 Margin 与 Config 查询(推送可能在缺口里丢失)。
- 不在归属连接上的私有推送(切换窗口外)按"不属于本连接"丢弃并计数——正常情况下不应出现。

**模拟账户**:`GwRuntime::new` 收到 `AccountKind::Sim` 的账户 → `InitErr::SimAccount`;模拟账户没有交易所 key、不登录、不订阅私有频道,订单与回报由 sim-gw 处理([hft-sim](../hft-sim/README.md)),它同样产出 `MarginReport`、`ExternalFill` 等同一组事件。

### 5. 心跳

gw 的最近入站时刻包括完整 WS Ping/Pong 控制帧；安静的下单/用户流连接可由心跳证明存活。未完成的 TLS/WS 字节不更新该时刻，真正断流仍触发 idle_timeout。

| 场景 | 动作 |
|---|---|
| 服务端 WS Ping | `hft-net` 自动回 Pong |
| `HeartbeatSpec.ping = AppText/AppJson` | 每 `interval_ms` 发一次；`unconditional=false` 时，收到数据后下次期限为最后入站数据时刻 + `interval_ms`，静默满该间隔即发送 |
| 我们的 RTT 探测 | 下单与深度连接每 5 s 一次 WS Ping(payload = tsc),Pong 记 RTT([03 §3.5](../../design/03-网络与传输层.md));行情服务器对 Ping 批处理的所(Binance fstream ≈ 50 ms)只用于存活,不进 RTT 直方图 |
| BBO 连接存活探测 | 每 `feed.bbo_ping_ms`(1 000)一次 `P::bbo_probe()`,不论有无数据;回帧驱动 `ConnState{Alive}`(§3a);同时充当该连接的 RTT 探测 |
| `pong_timeout_ms` / `idle_timeout_ms` 超时 | 置 `PING_TIMEOUT`/`IDLE_TIMEOUT` → 按致命处理 |

### 6. exch id → slot 私有表(`IdTable`)

```rust
#[repr(C, align(64))]
struct IdEntry {          // 64 B
    hash: u64,            //  0
    term_tsc: u64,        //  8 进入终态的时刻(TERMINAL 时有效)
    slot: u32,            // 16
    gen: u32,             // 20
    epoch: u16,           // 24
    key_len: u8,          // 26
    flags: u8,            // 27 TERMINAL = 1
    key: [u8; 36],        // 28 交易所 id 原文(≤ 36 B)
}
const _: () = assert!(core::mem::size_of::<IdEntry>() == 64);
struct IdTable {
    e: HugeSlice<IdEntry>,        // 容量 2^k = gw.id_table_cap,启动预分配
    len: u32, mask: u32,
    tomb: HugeRing<u32>,          // 终态保留条目的下标,按进入终态的先后;容量 gw.id_tomb_cap
}
```

- 线性探测,负载 ≤ 0.5;物理删除用后移(backward-shift),不使用哈希表意义上的删除标记。
- **插入**:gw 在下单响应(Ack)里同时拿到 client id 与 exch id 时,venue 调 `ReportSink::exch_id(key, Some(&id))` 记录(同时写进该槽镜像的 `exch_order_id`),gw 随即检查 `hold` 中是否有同 exch id 的暂存成交(§4 第 2 步)。回报只带 exch id 时 venue 调 `exch_id(key, None)` 查询。这张表是"回报只带 exch id、不带 client id"时的唯一解析路径(例:晚到的 `FeeOnly`、某些所 maker 成交不带 client id)。
- **终态保留(墓碑条目)**:订单到终态(Filled、Cancelled、Rejected、Expired)时**不删除**条目,而是置 `TERMINAL`、记 `term_tsc`、把下标推入 `tomb`。查找命中终态条目照常返回其 `(epoch, slot, gen)`——晚到费用(`FeeOnly`)与迟到成交因此仍能归到原订单的那一代;OMS 发现该代已被回收时按自己的墓碑规则入账并报警([hft-oms](../hft-oms/README.md),OMS 的墓碑与这里的保留条目是同一件事的两端:gw 负责"exch id → 哪一代",OMS 负责"那一代已回收时怎么入账")。
- **淘汰**:`tomb` 满或表负载将超过 0.5 时,从 `tomb` 头部淘汰,但只淘汰 `now − term_tsc ≥ gw.id_tomb_ms` 的条目;头部条目未满保留时长而表已满 → 计数告警,新插入失败(该订单的回报只能靠 client id 解析);`tomb` 满而头部未到期 → 同样计数告警,新到终态的条目直接删除(不保留,其晚到回报以 `UNRESOLVED` 进入)。同一 slot 的新一代订单写入时不覆盖旧代的保留条目(键是 exch id,不是 slot)。
- 跨进程重启不保留:重启后由 `MirrorRestore` 只恢复非终态槽;重启前终态订单的晚到费用以 `UNRESOLVED` + `exch_order_id` 进入,OMS/ledger 按 `(venue, account, exch_order_id)` 入账。
- `OrderEvent.exch_order_id`(u64):数值型 id 直接存;字符串 id(UUID、24 位十六进制等)存 xxh3-64 哈希,全文只在本表(≤ 36 B)与 `RawRx` 里;超过 36 B 的 id 不入表,计数 `id_too_long`(该单的回报只能靠 client id 解析)。

### 6a. 去重环的重启重建

`GwCtx` 里的去重环(契约 22)只在进程内有效、不进快照。为防止重启后交易所在重订阅时重推(各所是否重推待核实,开发备忘第 8 条)、或重启前已记录过的成交与资金费推送再次入账,`GwRuntime::new` 在开始轮询之前:

1. 用 `raw_tail` 读本 venue `raw-gw-<venue>` journal 最近 `gw.dedup_reseed_ms` 的记录(记录头 `tsc ≥ now − gw.dedup_reseed_ms`);按 `Connect` 标记的记录体 `[account, role]` 还原每个 `RawHdr.conn` 所属的账户。
2. 对其中每条回报类 `RawRx`(下单连接与用户流连接上的数据帧;acct-fetch 交回的账户查询响应不在其中)依次 `P::classify_report` + `P::decode_report`,sink 只把成交与资金费推送的去重键插入该账户的环,其余输出全部丢弃——不写环、不写 raw、不改槽镜像与 `IdTable`。
3. 完成后清空 `GwCtx` 中除去重环以外的状态(累计量合成表、保证金推送缓存回到初值),不以历史帧作为事实来源。

重建只决定"哪些推送已经见过";窗口之外更早的重复推送不在保护范围内,由 reconciler 的对账发现。重建耗时计入启动步骤 6 的预算([hft-core 启动顺序](../../apps/hft-core/README.md#启动顺序))。

### 7. 会话建立(warm 线程 `conn-builder`,`SessionDriver`)

1. `P::bootstrap(role, cfg, cred)`(`cred` = 该账户的 REST 凭证,行情角色为 `None`)返回 `Some(BootCall)` 时先执行该 REST 请求(Binance USDT-M listenKey、KuCoin bullet token),2xx 响应体作为 `endpoint` 的 `boot`。
2. `P::endpoint` 生成 path/query/额外头(可能含签名,如 KuCoin 私有 WS URL)→ `Connector::submit`。行情角色(`Md`)不调用 `P::endpoint`:公共行情的 path 按路由固定,装配时已写进 `Connector` 的端点表,而 `BuildReq` 不带路由号(M1.5 探针上以路由 0 调用时 Binance 现货与 USDT-M 返回 `Unsupported`,行情连接全部建不起来)。
3. `Open` 后:Order/UserStream 角色调 `P::login`,等 `P::login_result`(多步挑战式登录用 `Continue`;返回 `LoginStep::Sent` 的登录帧交易所不回复,发出即视为登录完成);失败按 `LoginResult::Rejected(code)` 上报并退避。
4. 交接前核对 NAPI 落点(`Connector::napi_ifindex`,规则与处置归 [hft-core 主机资源隔离](../../apps/hft-core/README.md#主机资源隔离slice核与出口-ip)):不符 → 不交接,按建连失败退役。登录成功 → `handoff` 给请求它的任务(feed-bbo、feed-md、snap-fetch、acct-fetch 或 gw;连接号由请求方在 `BuildReq` 里按 §1 号段给出)。请求方启用了就绪探测时,交接前先把该连接注册进请求方的就绪 epfd(hft-net `ReadyHandle::add`,`epoll_ctl` 在 warm 线程;hft-core 装配时把各行情任务的 `ReadyHandle` 交给该 venue 的 `SessionDriver`,按 §1 的连接号段找到请求方)。**订阅由热任务在 `install` 后发出**(订阅回执进 raw journal,限速由行情任务控制);登录帧与凭据只在 warm 线程出现,热线程不持有会话私钥(需逐单签名的 Hyperliquid 例外,gw 持有 API 钱包私钥)。
5. REST 签名请求用**独立的 HMAC REST key**:gw 的账户状态与 warm AccountClient 分别持有凭证,通过 `*_signed`/`account_request` 按调用借用;`Templates` 不留密钥(契约 7)。gw 签死手、REST 订单、挂单列表与会话维护;warm task 签账户查询。Ed25519 key 只供会话登录与其身份核验。
5a. **会话 key 的身份核验**([hft-venue-proto 契约 23](../hft-venue-proto/README.md#实现方必须遵守的契约)):Order 角色登录成功后、交接之前,`P::session_identity_request` 返回 `Ok(true)` 时,warm 线程用该会话凭证签名的只读请求查一次 UID(经 warm 线程自己的 REST 连接,超时 `gw.acct_query_timeout_ms`),`P::decode_identity` 解出的 UID 随 `Handoff` 交给 gw(`Handoff.session_uid: [u8; 24]` + 状态 `Ok | Failed`);gw 把它放进下一条 Config 请求(§4a),由 acct-fetch 并入报告:置 `uid_keys |= SESSION`,与 `AccountMeta.venue_uid` 不符时置 `UID`。查询失败时按 §4a 的退避在交接前重试,共至多 `gw.acct_query_degraded_after` 次(含第一次);仍失败则以 `Failed` 交接(连接照常可用,但报告不置 `SESSION`,`IDENTITY` 闸门保持关闭,gw 推 `ConnState{Degraded, role=Rest, reason=ACCT_QUERY}`),由下一次会话重建重查。返回 `Ok(false)` 的所,REST 步的结果同时置 `SESSION | REST`。每次重建会话(切换、重连)都重查,结果并入下一份 Config 报告。
6. 私有频道不在 warm 线程订阅:连接交接后由 gw 按 §4b 决定是否成为归属连接,再发 `P::private_subscribe`(签名型订阅帧由 warm 线程预先生成并随交接带过来,热线程不持有会话私钥)。
7. **回收**:收到 `RetireMsg` 时先从就绪 epfd 注销(`ReadyHandle::del`,若注册过),再 `Connector::reclaim`。

### 8. 源文件规划

| 文件 | 职责 |
|---|---|
| `src/lib.rs` | 公开入口 |
| `src/config.rs` | `FeedCfg`/`GwCfg`/`SnapCfg` 与默认值、`InitErr`、`ready_poll_on`、`feed_cmd_dest` |
| `src/codes.rs` | `ConnState.reason` 码表(gen-schema 读取,导出 `schemas/conn-reason.json`) |
| `src/raw.rs` | raw journal 的追加与滚动 |
| `src/feed.rs` | `FeedRuntime` 循环体、按 `FeedRoutes` 建路由池、就绪探测、BBO 按帧轮转 |
| `src/conn_id.rs` | 连接号号段、BBO 槽分配与复用 |
| `src/seq.rs` | `InstSeq`、去重与断裂判定 |
| `src/topn.rs` | 分档快照的状态(`AwaitFirst`/`Live`)与 `Resync`/`Up` 记录(§3c) |
| `src/resync.rs` | 增量深度的重同步槽、状态机、snap 记录的应用(可选扩展) |
| `src/snap.rs` | `SnapFetchTask`、`SnapReq`/`SnapRecHdr`、请求合并与权重计数 |
| `src/snap_pool.rs` | `PooledSnapFetchTask`、公共 REST 池、合批与有界完成记录 |
| `src/gw.rs` | `GwRuntime` 循环体、`drain_out`、发送、切换 |
| `src/kill.rs` | `kill_admit`(纯函数,gw 与 hft-core 启动自检共用) |
| `src/idtable.rs` | `IdTable`、终态保留与淘汰 |
| `src/hold.rs` | 解析不到槽的成交暂存、超时转 `ExternalFill{Unknown}` |
| `src/reseed.rs` | 去重环的重启重建(§6a) |
| `src/acct.rs` | gw 侧 `AcctSched`:调度、权重预记、acct 记录的应用、失败退避(§4a) |
| `src/acct_fetch.rs` | `AcctFetchTask`、`AcctReq`/`AcctRecHdr`、会话 UID 并入(§4a) |
| `src/private.rs` | 私有频道归属、先建后断与强制切换(§4b) |
| `src/rate.rs` | 发送侧计数与响应校正 |
| `src/session.rs` | `SessionDriver`、就绪 epfd 的注册与注销 |
| `src/sink.rs` | bbo/md 环 sink、`L2Snapshot` 切片与 `L2Batch` 流式写入、raw journal 追加 |
| `src/hb.rs` | 心跳与超时 |
| `src/bbo_live.rs` | `BboConnLive`:探测、`Alive`/成员/补帧记录、静默判定(§3a) |

</details>

BBO 接收端通过 [hft-net 的缓冲优先入口](../hft-net/README.md#1-接收路径) 排空已收到的完整帧或记录后再读 socket；部分记录仍立即补读。帧数限额、BBO 轮转、公平性、每帧后的 `drain_out` 与空队列确认规则保持原契约。

## 延迟与资源契约

测量点用 [event-model](../../docs/subsystems/event-model.md#时间戳语义) 的时间戳;分段名与各分段的数字唯一归属 [latency-budget.md](../../docs/latency-budget.md)(本表是 hft-venue 在 `ingest`、`egress`、`hedge_host` 中所占部分的分配)。预算不含 `hft-net` 的收包部分(见 [hft-net](../hft-net/README.md#延迟与资源契约))。

| 入口 / 段 | 任务 | p50 / p99 预算 | 基准名 |
|---|---|---|---|
| `classify`(前 64 B) | feed/gw | ≤ 30 ns / ≤ 60 ns | 各 venue `bench_classify_*` |
| 帧 → `Bbo` 入 bbo 环(peek + 去重 + decode + push),bookTicker 300 B | feed-bbo | ≤ 0.6 µs / ≤ 1.5 µs | `venue_feed_bbo_path` |
| 重复帧丢弃(peek + 判定) | feed-bbo | ≤ 0.15 µs / ≤ 0.4 µs | `venue_feed_dup_drop` |
| 帧 → 一条 `L2Snapshot{TopN}`(20 档)入 md 环与 bulk 环 | feed-md | ≤ 1.5 µs / ≤ 3 µs | `venue_feed_topn_path` |
| 帧 → 一条 `L2Batch`,2 KiB depth(增量深度,可选扩展) | feed-md | ≤ 2.5 µs / ≤ 5 µs | `venue_feed_l2_path` |
| 应用一条 snap 记录(raw 追加 + 分片拷贝 + 对齐缓冲),1000 档(增量深度,可选扩展) | feed-md | ≤ 15 µs / ≤ 30 µs | `venue_feed_snap_apply_1000` |
| REST 响应 → snap 记录(`decode_snapshot` 1000 档 + 写环;增量深度,可选扩展) | snap-fetch(warm) | ≤ 80 µs / ≤ 120 µs | `venue_snap_fetch_1000` |
| raw journal 追加,300 B | feed/gw | ≤ 0.2 µs / ≤ 0.5 µs | `venue_raw_append` |
| 订单类 `GwCmd` 出环 → `sendmsg` 返回(encode + seal + 加密 + sendmsg,回环) | gw | ≤ 3 µs / ≤ 8 µs | `venue_gw_place_loopback` |
| 同上,不含 `sendmsg` 系统调用 | gw | ≤ 0.8 µs / ≤ 1.5 µs | `venue_gw_place_mem` |
| 回报帧 → `OrderEvent` 入环 | gw | ≤ 1 µs / ≤ 2.5 µs | `venue_gw_report_path` |
| 下单连接已有 64 帧时首笔独立私有回报入环 | gw | ≤ 1 µs / ≤ 2.5 µs | `venue_gw_private_report_burst` |
| 回报突发(64 帧连续到达)期间,出环订单被 `drain_out` 取出的等待 | gw | ≤ 1.2 µs / ≤ 3 µs | `venue_gw_drain_during_burst` |
| `IdTable` 查/插/置终态/淘汰 | gw | ≤ 40 ns / ≤ 100 ns | `venue_idtable` |
| `kill_admit`(一次 `load(Acquire)` + 位运算) | gw | ≤ 3 ns / ≤ 8 ns | `venue_kill_admit` |
| BBO 健康维护(每轮每条 BBO 连接:探测到期判断 + Alive 判定,不含发送) | feed-bbo | ≤ 20 ns / ≤ 50 ns | `venue_bbo_live_tick` |
| 回报帧 → `ExternalFill` 入环(venue 判定 + 去重) | gw | ≤ 1 µs / ≤ 2.5 µs | `venue_gw_external_fill` |
| 可忽略/未知用户流事件跳过 | gw | ≤ 0.2 µs / ≤ 0.4 µs | `venue_gw_skip_event` |
| 最大的保证金推送(`margin_data = Push` 的所:账户或持仓推送 → `MarginReport`,取该所最大的推送样本;在 gw 上内联解码) | gw | ≤ 8 µs / ≤ 15 µs | `venue_gw_margin_push` |
| 应用一条 acct 记录(`Raw`:一块 16 KiB 响应追加 raw-gw;或 `Done`:报告推入 rep 环) | gw | ≤ 2 µs / ≤ 5 µs | `venue_gw_acct_apply_16k` |
| 账户查询响应解码 + 写 acct 记录,50 KiB | acct-fetch(warm) | ≤ 60 µs / ≤ 120 µs | `venue_acct_fetch_50k` |
| 空闲 `poll_once`(feed-bbo,2 条 BBO 连接无数据,不探测) | feed-bbo | ≤ 2 µs / ≤ 4 µs | `venue_idle_loop_bbo` |
| 空闲 `poll_once`(feed-bbo,就绪探测生效,6 条 BBO 连接无数据) | feed-bbo | ≤ 0.8 µs / ≤ 2 µs | `venue_idle_loop_bbo_ready` |
| 空闲 `poll_once`(`routes = All`,3 连接无数据) | feed | ≤ 3 µs / ≤ 6 µs | `venue_idle_loop` |
| gw 出环等待:订单到达 out 环 → 被 `drain_out` 取出,8 个 socket 空闲(含每圈 poll 的 UserStream;REST 连接无在途请求) | gw | ≤ 1 µs / ≤ 2 µs | `venue_gw_idle_8sock` |

- **BBO 在 feed 侧的等待上界**(本条是 [latency-budget §2.3](../../docs/latency-budget.md#23-bbo-与深度的通道) feed 侧口径的归属):升级主机类上 BBO 在独立任务与独立环上,不等深度帧、成交帧或快照。一条 BBO 帧(不含同一连接上排在它前面的帧)从数据可读到被处理,至多等:① 本线程一圈的发现时间(就绪探测生效时为 `venue_idle_loop_bbo_ready`,否则逐连接空 `recvmsg`,`venue_idle_loop_bbo`);② 本任务按帧轮转中排在它前面的其他每条就绪 BBO 连接各一帧;③ 同线程的每个其他 `feed-bbo:*` 任务至多 其 BBO 连接数 × `feed.bbo_frames_per_poll` 帧。每帧按 `venue_feed_bbo_path` 计。`routes = All`(dev2/c4、`sim`)时另加本任务其余路由的至多 连接数 × `feed.frames_per_poll` 帧,没有延迟验收。
- **对冲路径**:成交帧解码入环(`venue_gw_report_path`)+ 第二腿出环等待(`venue_gw_drain_during_burst`)+ 发送(`venue_gw_place_loopback`)是 `hedge_host` 中 gw 的部分;sequencer 部分归 [hft-sequencer](../hft-sequencer/README.md)。
- **账户查询对下单的影响上界**:账户查询的解码在 acct-fetch;gw 每圈至多应用一条 acct 记录(原始响应按 16 KiB 分块,`venue_gw_acct_apply_16k`),排在本圈最后一次 `drain_out` 之后。推送型所的保证金推送在 gw 上内联解码(`venue_gw_margin_push`),该所 venue README 的推送解码预算不得超过这一行;超过时把这些频道移到一条只订阅不下单的连接(可选扩展,写在该 venue README)。若某所账户查询响应常态 > 50 KiB(全符号持仓列表),venue 必须改用按 inst 过滤的请求(写进该 venue 的"保证金数据换算"表)。
- **分配**:`poll_once` 稳态零分配;`InstSeq`、重同步槽、`IdTable`、帧缓冲、snap 与 acct 记录缓冲在 `new` 时预分配(大页)。
- **系统调用**:feed 任务只有 `recvmsg`、控制帧(Pong、应用心跳、订阅帧)的 `sendmsg`,以及就绪探测生效时每次 `poll_once` 至多一次 `epoll_pwait(timeout = 0)`;不持有 REST 连接、不做 `epoll_ctl`(注册在 conn-builder)。gw 另有订单帧与 REST 请求的 `sendmsg`;均在 [hot-path-rules T7](../../docs/hot-path-rules.md) 允许表内。snap-fetch 与 acct-fetch 在 warm 线程,不受 T7 约束。
- **共享缓存行**:只读 `KillWord`;其余只经 SPSC。
- **内存**(每 venue):`InstSeq` 64 B × inst 数 × 行情任务数;重同步槽(只在增量深度)`feed.resync_slots` × `resync_buf` 条消息(解码后 L2Batch,平均 ≤ 1 KiB)≈ 4 MiB;snap/acct 环按 hft-core 环容量表 + snap-fetch 与 acct-fetch 记录缓冲;`IdTable` 16384 × 64 B = 1 MiB/账户;`hold` 256 × 176 B = 44 KiB/账户(包含完整交易所 id 与到达时刻);`AcctScratch` 与 `AcctCtx` 由 venue 定(≤ 64 KiB/账户,在 acct-fetch)。

带 `POSITION_CLOSE` 的 Place 按协议的 `order_channel_for` 路由,使用 `position_close_interval_ms` 的账户间隔护栏;过早请求回 `RateLimited`。REST 提交时登记槽镜像。专用端点返回的 exchange-id 仅在 ACK 与发送时保存的 account/inst/epoch/slot/gen 一致、镜像仍为该代时绑定;随后释放先到的暂存成交。HTTP/解码失败仍为 Unknown,不把 ACK 当仓位清零。恢复镜像保留平仓标记。`P::REST_PLACE_BINDS_ID = true` 的协议(订单不带 client id,Polymarket)对每个 REST Place 的 ACK 按同一条件绑定,不要求平仓标记。

## 测试与验收

独立私有用户流先于同账户下单连接读取,帧严格保持连接内顺序。gw 先排空已缓冲的完整帧/记录再补读 socket;每帧解码后立即发布并 drain_out,不等整批。VEN-207 验证同次 read 内前帧在后帧解码开始前可见,私有回报不等待另一下单连接的突发。

用例与验收见 [TESTS.md](TESTS.md)。

## 依赖

| 依赖 | 备注 | 理由 |
|---|---|---|
| `hft-venue-proto` | — | `VenueProtocol` 与纯类型 |
| `hft-types`、`hft-refdata`、`hft-event` | — | 定点、ID、`KillBits`、`ClientOrderId`、`SlotId::UNRESOLVED`、`MAX_BBO_CONN_SLOTS`、refdata(`SymbolIndex`、`RateLimitTable`、`VenueMeta`)、事件体、`OutMsg`/`GwCmd`/`FeedCmd`/`FeedMsg`/`RepMsg`、`SendReport`、`RawHdr` |
| `hft-net` | — | 连接、池、REST、`MaskedFrame`、`ReadyPoll`/`ReadyHandle` |
| `hft-journal` | `writer` 特性 | raw journal 写者;去重环重建时读 raw-gw 尾部(本 crate 只被 apps/hft-core 链接) |
| `hft-rt` | — | `Producer/Consumer`、`ByteSpsc`、`KillWord`、TSC 与换算、大页 |
| `xxhash-rust`(xxh3) | — | 字符串 exch id 哈希 |

**禁止**:依赖任何具体 `venue-*`(具体 venue 只在 apps/hft-core 以泛型参数装入)、`hft-sequencer`、`hft-oms`、`hft-risk`(kill 字以 `&KillWord` + 掩码传入,位常量在 hft-types);`tokio`、`serde_json`(JSON 解码在 venue crate 用 `sonic-rs`)、`tracing` 于热路径;直接调用 `libc`(系统调用封装在 hft-net 与 hft-rt)。

VEN-341 用以本 crate 为根的 `cargo tree` 检查依赖闭包与特性,不把其他 workspace 成员给共享测试基建启用的特性计入本 crate。

## 延伸阅读

- [04 行情接入层](../../design/04-行情接入层.md)(§2 线程内部、§6 断裂与恢复、§7 raw journal)
- [05 CEX 执行层](../../design/05-CEX执行层.md)(§4 网关、§5 定时器、§7 各所通道、§8 失败模式)
- [13 设计评审 R1、R3、O2、O8](../../design/13-设计评审与优化.md)
- [research/02 交易所接入](../../design/research/02-exchange-connectivity.md)
- [hft-net](../hft-net/README.md)、[event-model](../../docs/subsystems/event-model.md)、[defensive-patterns](../../docs/defensive-patterns.md)、[cookbook/adding-a-venue.md](../../docs/cookbook/adding-a-venue.md)
- 实现(core):[venue-binance](../venue-binance/README.md)、[venue-okx](../venue-okx/README.md)、[venue-bitget](../venue-bitget/README.md);(optional):[venue-gate](../venue-gate/README.md)、[venue-bybit](../venue-bybit/README.md)、[venue-kucoin](../venue-kucoin/README.md)、[venue-hyperliquid](../venue-hyperliquid/README.md)
- [hft-sim](../hft-sim/README.md)(模拟账户的 sim-gw)、[hft-oms](../hft-oms/README.md)(墓碑、`ForeignOrder`、冒烟单)、[hft-risk](../hft-risk/README.md)(`ACCOUNT_CFG`/`IDENTITY` 闸门、`MarginStress`)
- 范围与取舍:[round8-decisions](../../design/_work/round8-decisions.md);Agent Note:`venue-protocol-and-runtime-split`、`dedicated-gateway-threads`、`no-speculative-tls-encryption`、`venue-caps`、`feed-split-and-snap-fetch`、`rx-irq-driven`、`core-market-data-scope`、`irq-split-reserve`

### 测试环境账户启动与观察

`account::Client<P>` 串行执行 warm 协议。撤单按“查一页、逐笔撤、重查第一页”，超过 max_cancel_rounds 失败，不沿已失效的删除分页 cursor 继续。只交付完整 State。私有重连前重新核验账户/身份，再调用 SessionDriver::start_prepared；独立用户流使用 bootstrap 路径。周期失败丢弃部分状态，由调用方发布不可读闸门并重试。

启动最后可执行协议声明的公共资金费周期查询；传输/HTTP/解析失败只丢弃整组 funding_schedule 并保持此前等待 WS 时间戳变化的行为，不放宽必需账户检查。成功观察在 feed 启动前 seed 其上下文，第一条实时 Funding 输入包含周期并先写 seq journal；不以估计费率或余额差生成 FundingSettle。周期账户核验不重新覆盖 feed 的已观察周期。

固定容量 order_rest SPSC 将查单送 warm worker，响应回到 gw 唯一上下文解码；失败保持 OutcomeUnknown。CancelAll 可退化为镜像内逐单 WS 撤单。资金费流水按有界 cursor 分页，只供比较，截断/循环拒绝整轮。

账户 warm 调度在 REST 与会话 UID 比对时，只对 Unverifiable 账户容许缺失值，并发布 uid_keys=0；双方都有值但不同仍失败。保证金条目随同一轮持仓观察传递，失败轮次不发布部分 State。

### 账户与私有回报的运行时接线

启动期以 `set_account_queries` 注入 acctreq 生产端，以 `set_rep_bulk` 配对变长回报尾部环；`set_rest_pools` 的端点顺序对应 `RestReqBuf.endpoint`。进程内 ACCTv001 帧头由字段逐项编码为 80 字节，不依赖 Rust 结构体布局；Raw 只含方法、去掉 query 的路径与响应块，Done 携带一整组报告。gw 检查序列、账户、venue 与品种归属后发布；失败序列不发布部分报告。rep-bulk 的尾部先提交，rep 头后提交，背压时两者一起保留。

同一 gw 中相同 src_ip_idx 的账户共享 IP/Weight 窗口；账户查询和内联 REST 请求共同记账，Account/Uid 窗口仍按账户执行。REST 使用生效表的 default_cost，账户序列使用声明的选中步骤权重之和；不同任务使用同一出口时的全局配额协调仍待装配验证。

`prepare_private` 只在 warm 线程生成订阅/退订帧，使用 Conn 的非秘密交接数据传给 gw。私有回执按频道位去重；拒绝或超时保持旧归属。REST 订单在成功写入时仅发布一次 SendReport，传输失败保留 Unknown，不由超时/429/5xx 合成业务拒绝；池关闭与超时取消先退役旧连接，迟到响应不能归到下一请求。

REST 在途项保存发送时的完整 ClientOrderId；回包省略 epoch 时只补该请求的 epoch，传输失败产生的 Unknown 也使用该身份。槽复用或 SetEpoch 不改变已发送请求的归属；没有匹配在途证据的 Unknown 保持 UNRESOLVED，不能借用当前槽的 epoch。

`AcctFetchTask::set_credential` 仅允许空闲账户更新凭证；编码通过 `acct_request_signed` 借用凭证，Templates 不保留秘密。失败和超时的 raw 记录使用提交时连接号；凭证、签名与登录正文不进入 raw journal。

诊断通过 `RepMsg::Note` 交付，码登记在 `src/codes.rs`。`v[0]` 为 venue、`v[1]` 为 account；未知事件的 `v[2..6]` 是事件名前 32 字节的零填充小端编码，事件名由 `report_event_name` 提供，不能包含用户字段或整个载荷。费用偏离不修改成交或费用事实。

## 已知限制与后续工作

- 有界 `hold` 按账户暂存无法归属的成交,收到有效 epoch 的 Ack 后按到达顺序释放;超时/满载遵循 §4。终态只影响同 account/epoch/gen 的槽镜像,IdTable 淘汰后重新定位可能移动的条目。VEN-294 的前置“4 条终态容量中保留 8 条终态”不可达;测试以 4 条终态 + 4 条活动条目验证相同满表拒绝边界,该前置修订待负责人评审。
- VEN-073 的“md 环容量 4”与 §1 的读取前预留 `FRAME_EVENTS_MAX = 64` 不相容;测试使用容量 128 的环、两帧各 64 笔成交填满后验证 BBO 隔离。该前置修订待负责人评审,不据此声称容量 4 可运行。

- **实现范围**:BBO、分档快照、公共 REST 补帧、raw journal、gw 出环/kill/限速/发送/撤单展开/回报/成交暂存、主备切换与心跳、warm 账户启动。`AcctSched` 与有界 `AcctFetchTask<P: AcctProtocol, T: AcctTransport>` 覆盖完整账户查询序列、失败退避与分块 raw；`AcctRestPools` 通过 `AcctConnections` 管理固定区间的建连、交接与退役，源地址取账户配置；退役环满时保留连接所有权并停止补建。gw 支持 rep-bulk、私有频道归属/回执切换、内联 `RestPool`、空闲检测与 `CANCEL_ALL_SWEEP`。具体产品线的 `AcctProtocol` 实现及应用账户轮询迁移与去重环重建仍待完成。未知用户事件与费用偏离的 Note 经 rep 环交给 live driver，按 Meta 分类记录；VEN-361–365 基准可执行，真实最大响应采集与参考机延迟验收待完成。池化补帧接入应用，驱动内 bootstrap/UID REST 支持有界重试；应用会话仍经账户客户端调用 `start_prepared` / `verified_identity`，缺少独立会话 key 的 UID 证据时交接 Failed 状态。用例里程碑保持 TESTS.md 的定义。
- **深度只有前 N 档**:core 的深度是分档快照(各所档位上限不同,见各 venue README);需要全深度的策略触发可选扩展"增量深度与快照重同步"(§3),它的深度连接是单路,一条连接断开 = 该连接上所有 inst 重同步;双路合并是再一层可选扩展(开发备忘)。
- **gw 负责填帧**:OMS 只产出数值化 `OrderDecision`,sequencer 投影为 `GwCmd`,venue 特定的帧编码在 gw 线程完成(sequencer 不依赖任何 `venue-*`,各 venue 并行填帧)。
- **refdata 变更 = 重启**:新增/移除 inst 走 ConfigBlob + Drain + 重启;可选扩展(refdata 热替换,触发条件:重启窗口影响交易)时 `rd` 参数接口不变,运行时需增加订阅差分。
- **FIX 与 SBE 下单**:`VenueProtocol` 以 WS 文本/二进制帧为单位;FIX(Binance Spot、Bybit spot)需要非 WS 分帧,可选扩展:接入这些通道时扩展 `MsgOp` 与 `hft-net`。
- **REST 经 warm 线程**:BBO 补帧与账户查询都在 warm 任务里执行,各多出至多一个 warm 线程 park 周期;BBO 与对冲路径不受影响。
- **推送型保证金数据仍在 gw 上解码**:它与订单推送同在私有连接上,只能按到达顺序处理;上界见"延迟与资源契约"。
- **BBO 槽上限 `MAX_BBO_CONN_SLOTS`**:单 venue 的 BBO 订阅超过 `ws_max_streams_per_conn × (MAX_BBO_CONN_SLOTS − 1) / bbo_conns` 个 inst 时放不下(Binance 约 3,500 个),需要扩大槽数(改 `bbo_member_mask` 宽度,涉及 sequencer 快照节)。
- **`ExternalFill{Unknown}` 与后续查询的重叠**:暂存超过 `gw.unmatched_hold_ms` 后才到的 Ack 会让同一成交既有 `ExternalFill{Unknown}` 又能被查询结果归到某个槽。去重键见 [hft-venue-proto 契约 22](../hft-venue-proto/README.md#实现方必须遵守的契约);OMS 的 `ext_held` 扣除已按外来成交入账的量([hft-oms](../hft-oms/README.md)),执行层第二腿数量取 min(按成交净量算出的量, LegTracker 当前该方向的裸量)([hft-exec](../hft-exec/README.md)),同一成交不会触发两次对冲。5 s 的暂存使这种重叠只在 Ack 异常迟到时出现。
- **去重环重建有窗口**:只覆盖 `gw.dedup_reseed_ms` 内的推送。需要 venue 合成累计量(契约 9)的所,重启后合成表从空开始,对重启前已有成交的非终态订单合成的 `cum_filled` 会偏小;这类所(README 标明)不得在 core 上线,除非其 README 给出重启后的收敛办法(如先按查询结果重置该订单的合成量)。core 的 Binance、OKX 推送直接给累计量,Bitget 待核实(venue-bitget 开发备忘第 3 条)。

### 开发备忘

<details><summary>给实现者</summary>

**可选扩展:L2 双路合并(增量深度之上,默认不实现)**

每 inst 增加 `conn_last[4]`、`conn_ok` 位图、`gap_since_tsc`;`Chain` 规则变为:`last_id ≤ last_fwd` → 丢并置 `conn_ok[c]`;`prev_id == last_fwd` → 转发;否则清 `conn_ok[c]`,若还有 ok 的连接则等 `gap_grace_us`(默认 2000)内由另一路补齐,超时或无 ok 连接才 Resync。前提:各连接的增量批边界逐条相同——对每所用两条连接同订阅一个 inst 1 小时比较 `(prev_id, last_id)` 序列核实。启用条件:运行中的重同步次数与簿 Stale 时长直方图证明值得。

**待核实**(每条写明方法;实测结论要求改默认值时,同一提交更新[配置项](#配置项)表的默认值并在 [docs/changes.md](../../docs/changes.md) 记一行)

1. **依赖方向**:`cargo xtask ci` 检查本 crate 的依赖闭包里没有任何 `venue-*`。
2. **kill 字的账户位**:`account(ix)` = bit 8..19、`account_hard(ix)` = bit 20..31,`ix` 是 refdata `venue_account_ix` 的稠密序号 0..11([hft-types `KillBits`](../hft-types/README.md));gw 与 hft-risk 用同一映射求位,hft-core 启动自检经真实的 `KillWord` store/load 验证([apps/hft-core 启动顺序](../../apps/hft-core/README.md#启动顺序))。
3. **`gw.aux_every` 与 `gw.rest_idle_check_ms` 取值**:只影响 standby 与 gw 的 REST 池;以 `venue_gw_idle_8sock`、[benchmarks](../../benchmarks/README.md) 的 `gw-ingest/poll-cadence` 与查询响应延迟的直方图权衡,初值 8 与 1 000 ms(待实测)。
4. **私有频道的先建后断是否被交易所允许(M3 之前必须核实)**:同一 listenKey 两条连接是否都收到推送(Binance USDT-M)、同一账户两条 ws-api 连接同时 `userDataStream.subscribe`(Binance 现货)、两条 `/private` 连接同时订阅 `orders` 是否都收到(OKX、Bitget,受每子账户每频道连接数限制)。方法:测试网/模拟盘上两连接同时订阅、下一笔小单,比对两路 raw。结论写进各 venue README。
5. **`gw.unmatched_hold_ms` 取值**:默认 5000 ms;按各所"成交推送先于 Ack"的实测间隔分布(p99.9)× 3 复核,超过默认值时调大(只影响无 client id 且 Ack 迟到的成交,不推迟正常回报)。
6. **`gw.id_tomb_ms` 取值**:按各所"终态之后费用推送"的实测延迟(p99.9)× 3 定;初值 120 s 覆盖已知最慢的费用流(optional 的 Gate usertrades)。
7. **`snap.parallel` 与 REST 权重**:core 上只承载 BBO 补帧(请求小、频率低);启用增量深度时按各所快照 REST 权重与 IP 级配额算出重同步风暴下的上限(现有采集系统以"同时 ≤ 4 个快照"避免 418/429)。初值 2,dev2 上实测后定。
8. **`gw.dedup_reseed_ms` 取值与断流后的补推**:各所私有流断开期间发生的成交(含强平、ADL)在重连或重订阅后是否补推、补推回看多久,待核实(方法:测试网/模拟盘断流演练——持仓时断开私有连接,期间制造成交,重连后比对 raw-gw 与成交历史;强平/ADL 难以制造时以普通成交代替并在结论里注明)。结论写进各 venue README 的"用户流事件表",并据最长回看时间定本键(若交易所从不重推,窗口只防"重启前已记录、重启后再送达"的边界情况);初值 15 min。
9. **warm 线程 park 超时的影响**:dev2 与升级主机上测"BBO 补帧请求 → 补帧 `Bbo`"与"`MarginNow` → `MarginReport`"的时长分布;若 park 周期占主导,调小该 warm 线程的 `park_us`(线程计划归 hft-core)。
10. **就绪探测的收益**:M1.5 与 M3 末满订阅探针上对比 `ready_poll = auto/off` 的 `venue_idle_loop_bbo*` 与 `kernel_rx` 直方图;`feed.bbo_frames_per_poll` 在 4–8 之间按 BBO 突发下的 `ingest` p99 定。
11. **分档快照的节拍与负载**:各所 `DepthTopN` 频道在空闲与突发时的实际推送节拍、每条字节数与跨合约的对齐情况(决定 IRQ 核负载);方法:各所对 50 个合约订阅 1 小时,统计 raw。
12. **acct 记录的 raw 追加成本**:`venue_gw_acct_apply_16k` 在升级主机上待实测;p99 超预算时减小 `ACCT_RAW_CHUNK`。

**实现顺序**:(hft-venue-proto 与 venue-binance 金样先全绿)→ `seq.rs` 属性测试 → `conn_id.rs` → `topn.rs` + `snap.rs`(BBO 补帧)→ `feed.rs` 对 testkit 假交易所(三种 `FeedRoutes`、就绪探测、按帧轮转)→ `idtable.rs` → `hold.rs` → `gw.rs` → `reseed.rs` → `private.rs` → `acct.rs` + `acct_fetch.rs` → `session.rs` →(可选扩展)`resync.rs`。

</details>

### 当前 BBO 补帧传输

`PooledSnapFetchTask<P, N>` 接收独立 FeedCtx、两组请求/响应环与 `RestPool<N>`，以 `parallel` 限定在途批数。启动期通过 `SnapConnections` 接 conn-builder，或 `attach_connector` 在同一 warm 任务建连；源 IP 由装配传入。`SnapFetchTask::into_pooled` 可迁移尚未启动的单连接任务。

每批最多取 `BBO_SNAPSHOT_BATCH` 与 32 的较小值，重复 inst 留到下一批；请求 BufFull 时减半重试。成功响应必须恰好包含每个请求 inst 的一条 Bbo。每个请求都有完成记录，同批原始正文只附在首条；环满保留整批，超过 `max_record_bytes` 整批只发 TooLarge 头。超时退役连接，迟到响应不能匹配后续请求。池空或 Busy 不消耗 refdata IP/Weight 额度。当前 app 使用 `N = parallel = 1`；超时、请求间隔与记录上限仍由 SnapCfg 与环容量约束，参数归本页配置表。接收戳取 hft-net 的响应戳与本轮 TSC，不伪造硬件戳。

### 驱动内会话 REST

`SessionDriver::set_rest` 注入单账户 REST 传输、账户号与 REST 凭证后，`enqueue` 接受该账户的有界建连请求，拒绝其他账户复用其出口；`drive_at` 以 REST 凭证完成 bootstrap，再构造 endpoint。登录与 UID 查询仍用会话凭证。独立 session key 在登录后查询 UID，按 1/2/4/8/16/30 秒退避，达到 `acct_query_degraded_after` 后以失败身份状态交接；HTTP 成功且 UID 非空、ASCII 可见、零填充连续才是有效证据。`start_prepared` / `verified_identity` 继续支持调用方提供证据。应用账户客户端仍使用后一入口；多账户应用采用每账户驱动或外部账户客户端。

FeedRuntime 缓存协议是否支持补帧,在宽限期后每 inst 至多一个在途 req_id。成功无序号报价不改变流序号水位,失败再等一个宽限期;流已恢复、请求已失效或成员全失联时丢弃迟到响应。REST 的连接号不更新 WS 活性,原始响应由 feed 单写者写 raw。

账户 warm task 与网关 REST 池通过 `*_signed` 入口借用各自账户持有的凭证;网关死手、会话维护与挂单扫描同样显式传参,Templates 不携带密钥。
