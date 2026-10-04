---
description: "VenueProtocol trait 与它的纯类型:venue-* 实现它(含 VenueCaps 默认值、账户配置与保证金查询、外部成交识别),hft-venue 运行时(feed/gw/snap-fetch)、mock-exchange、replayer、金样与 fuzz 通过它调用交易所协议"
kind: crate
tier: pure
scope: core
status: spec
---

# hft-venue-proto

## 概要

`hft-venue-proto` 只拥有**交易所协议的接口**:`VenueProtocol` trait,以及它签名里出现的全部纯类型——路由与流(`RouteKind`、`StreamKind`)、分派结果、序号与重同步判定(`SeqRule`、`Align`、`ResyncMethod`)、解码输出 sink(`MdSink`、`ReportSink`)、帧模板接口(`Templates`、`encode`、`TargetRef`、`WallClock`)、账户查询接口(`AcctQuery`、`AcctQueryKind`、`AcctHint`、`AcctCtx`)、心跳/过期/死手规格、限速规则、错误枚举、成交与资金费去重键,以及所有实现方必须遵守的**契约**(本页"实现方必须遵守的契约")。它不含任何交易所的知识(在 `venue-<x>`),不含 I/O 或运行时(在 [hft-venue](../hft-venue/README.md)),也不定义能力表类型——`VenueCaps` 与限速、死手的枚举在 [hft-refdata](../hft-refdata/README.md),`MarketKind` 在 [hft-types](../hft-types/README.md),本 crate 只规定"每个实现提供一份默认值"。单独成 crate 是因为 Rust 孤儿规则:实现方 `venue-<x>` 必须能看到 trait,而 trait 不能与 I/O 运行时放在一起([crates/AGENTS.md 依赖方向](../AGENTS.md#依赖方向))。调用者:hft-venue 的 feed/gw 任务(热路径 t1、t4 上的每一次解码与填帧都经过这些方法)、warm 任务 snap-fetch(BBO 补帧与快照的请求与解析)与 acct-fetch(账户查询的请求与解析)、[mock-exchange](../../apps/mock-exchange/README.md)、[replayer](../../apps/replayer/README.md) 的 raw 模式、`tools/hft-cfg`(取 `default_caps()` 生成 refdata)、各 venue 的金样与 fuzz。

交易所之间的差异只在两处表达:协议细节在 `venue-<x>` 的实现里,行为差异(手续费扣哪个币、是否支持 reduceOnly、资金费怎么推、保证金数据从哪来、怎么认出自己被强平)在 `VenueCaps` 里。策略、执行层、OMS、风控只读 `VenueCaps`,不出现交易所名字。

## 目录

- [使用本组件](#使用本组件)
- [实现方必须遵守的契约](#实现方必须遵守的契约)
- [每个 venue README 的必备节](#每个-venue-readme-的必备节)
- [实现说明](#实现说明)
- [延迟与资源契约](#延迟与资源契约)
- [测试与验收](#测试与验收)
- [依赖](#依赖)
- [延伸阅读](#延伸阅读)
- [已知限制与后续工作](#已知限制与后续工作)

## 使用本组件

### `VenueProtocol`(静态分派)

每个 venue 产品线一个零大小类型实现它(如 `venue_binance::Usdm`、`venue_okx::OkxSpot`);一个产品线只属于一种 `MarketKind`(`Spot` 或 `LinearPerp`,类型在 hft-types)。hft-venue 的 `FeedRuntime<P>` / `GwRuntime<P>` / `SnapFetchTask<P>` 以它为泛型参数,无 `dyn`。方法全部是关联函数或以显式上下文为参数,实现方没有全局可变状态。

```rust
pub trait VenueProtocol: Sized + 'static {
    // ---- 身份与常量 ----
    const NAME: &'static str;            // venue 名,如 "binance-usdm":hft-core venues.<v>.protocol 的取值、fixtures/<NAME>/ 的目录名;
                                         // 与 hft-types 的 VenueId 常量名不要求同形(如 "bitget-usdt-futures" ↔ BITGET_UTA_USDT_FUT),对应关系只由 VENUE 给出
    const VENUE: VenueId;                // 产品线粒度(BINANCE_USDM、BINANCE_SPOT…)
    const MARKET: MarketKind;            // Spot | LinearPerp(hft-types)
    const ORDER_FRAME_CAP: usize;        // gw 选 MaskedFrame 容量(2048 / 8192)
    type MdClass: Copy + Eq;             // 行情分派结果(venue 私有枚举,含 Control/Ack/Pong/Unknown)
    type ReportClass: Copy + Eq;         // 回报分派结果(必须含 Ignorable 与 Unknown 两类,契约 20)
    type FeedCtx;                        // 只放 venue 私有状态(SBE 模板表、时间单位、路由内订阅下标…);refdata 不放这里,调用时显式传 rd。
                                         // feed 任务与 snap-fetch 各持一个实例(契约 25)
    type GwCtx;                          // 只放 venue 私有状态:请求 id 编码、累计量合成表、成交与资金费去重环(契约 22)、保证金推送缓存
    type Templates;                      // 帧模板:该 (venue, account) 的预编码片段与 REST 签名凭证(契约 7);随 refdata 在启动期构建一次(refdata 不热换,变更 = 重启);
                                         // gw 与 acct-fetch 各构建一份(交易 REST key),reconciler 以只读 key 构建自己的一份(契约 31)
    const CANCEL_ALL_SWEEP: bool = false; // true:gw 发完 CancelAll 后再查一次挂单列表兜底(契约 29;OKX)
    const BBO_SNAPSHOT_BATCH: u16 = 1;   // 一次 BBO 补帧请求可覆盖的 inst 上限(契约 24);u16::MAX = 只受 RestReqBuf 长度限制
    type AcctCtx: Default;               // 账户查询跨序列的 venue 私有状态(如上一次的 ADL 档);acct-fetch 每账户一个(契约 27)
    type AcctScratch: Default;           // 一次账户查询序列的中间结果;定长,不含堆(契约 16、17)

    // ---- 能力表默认值(冷;hft-cfg 生成 refdata 时调用,运行时不调用) ----
    /// 契约 15;以资产名给出的字段(平台币抵扣的 token)经 asset 按 refdata 资产表解析,查不到写 ASSET_NONE
    fn default_caps(asset: impl Fn(&str) -> Option<AssetId>) -> VenueCaps;

    // ---- 会话(warm 线程 conn-builder;可以分配、可以签名) ----
    /// 连接前需要先执行的 REST 请求(Binance USDT-M listenKey、KuCoin bullet token);cred = 该账户的 REST 凭证,行情角色为 None
    fn bootstrap(role: Role, cfg: &VenueCfg, cred: Option<&Credential>) -> Option<BootCall>;
    fn routes() -> &'static [RouteSpec];                                          // 行情路由表(如 Binance /public、/market);含 Bbo、DepthTopN 与 Md 路由
    fn endpoint(role: Role, route: RouteId, cfg: &VenueCfg, cred: Option<&Credential>, boot: Option<&[u8]>,
                now_ms: i64, out: &mut EndpointSpec) -> Result<(), ProtoErr>;     // path、query、额外 Upgrade 头
    fn login(role: Role, cfg: &VenueCfg, cred: &Credential, now_ms: i64, out: &mut Vec<u8>)
             -> Result<LoginStep, ProtoErr>;                                      // NotNeeded / Frame / Handshake
    fn login_result(payload: &[u8]) -> LoginResult;                               // Ok / Rejected(code) / Continue / NotMine
    fn subscribe(subs: &[SubSpec], cfg: &VenueCfg, out: &mut SubFrames) -> Result<(), ProtoErr>; // 分批帧
    /// 私有频道订阅帧(用户流、订单、持仓、账户、保证金推送);只在被选为私有频道归属的连接上发(见 hft-venue §4b)。
    fn private_subscribe(cfg: &VenueCfg, acct: AccountId, unsub: bool, out: &mut SubFrames) -> Result<(), ProtoErr>;
    fn heartbeat(role: Role) -> HeartbeatSpec;
    fn app_ping(role: Role, now_ms: i64, out: &mut [u8]) -> usize;               // 0 = 用 WS Ping
    /// BBO 连接的存活探测方式(契约 24):WsPing(默认)或本所应用层 ping(服务端不回 WS Pong 时)。
    fn bbo_probe() -> PingKind { PingKind::WsPing }
    /// 身份核验(契约 23):会话凭证(WS 登录用,如 Binance Ed25519 key)与 REST 签名凭证不是同一把时,
    /// 生成一个用**会话凭证**签名的 UID 查询请求并返回 Ok(true);同一把凭证(OKX、Bitget)返回 Ok(false)。
    /// warm 线程(conn-builder)在登录成功后、交接前调用;响应用 decode_identity 解出 UID。
    fn session_identity_request(cfg: &VenueCfg, cred: &Credential, now_ms: i64, out: &mut RestReqBuf) -> Result<bool, ProtoErr>;
    fn decode_identity(status: u16, body: &[u8], uid: &mut [u8; 24]) -> Result<(), DecodeErr>;   // ASCII,右补 0

    // ---- 行情(feed 任务;热) ----
    fn classify(route: RouteId, op: MsgOp, head: &[u8]) -> Self::MdClass;          // 只看前 64 B;route = 该连接所属路由(同名推送按路由区分)
    fn peek(rd: &RefData, ctx: &Self::FeedCtx, c: Self::MdClass, payload: &[u8]) -> Result<Peek, DecodeErr>; // inst + 归一化序号,不解价格
    fn seq_rule(c: Self::MdClass) -> SeqRule;
    fn decode_md(rd: &RefData, ctx: &mut Self::FeedCtx, c: Self::MdClass, payload: &[u8], rx: &RxMeta,
                 out: &mut impl MdSink) -> Result<MdOutcome, DecodeErr>;
    fn control(c: Self::MdClass, payload: &[u8]) -> Option<ControlHint>;          // 订阅回执、服务端下线预告等
    fn resync_method(c: Self::MdClass) -> ResyncMethod;                           // 增量深度:RestSnapshot / Resubscribe;BBO、分档快照与其余类:None
    fn align_first(snap_id: u64, raw: &RawSeq) -> Align;                          // 增量深度快照后第一条增量:交易所原始序号规则(feed 任务调用)

    // ---- 快照(warm 任务 snap-fetch,用它自己的 FeedCtx 实例;契约 25)。core 只用 BBO 补帧;
    //      snapshot_request/decode_snapshot 只在该 venue 启用增量深度(可选扩展)时被调用 ----
    fn snapshot_request(rd: &RefData, ctx: &Self::FeedCtx, inst: InstId, out: &mut RestReqBuf) -> Result<(), ProtoErr>;
    fn decode_snapshot(rd: &RefData, ctx: &mut Self::FeedCtx, inst: InstId, body: &[u8], rx: &RxMeta,
                       out: &mut impl MdSink) -> Result<SnapshotInfo, DecodeErr>;  // SnapshotInfo.snap_id
    /// BBO 补帧(契约 24):订阅回执后本所不立即推当前 BBO 时,生成按 inst 取当前最优一档的 REST 请求并返回 Ok(true);
    /// 订阅即推首帧的所返回 Ok(false)(默认)。响应经 decode_bbo_snapshot 产出 Bbo(update_id 取响应里的序号,无则 0)。
    /// insts.len() > BBO_SNAPSHOT_BATCH 或请求放不进 out → Err(ProtoErr::BufFull)。
    fn bbo_snapshot_request(rd: &RefData, ctx: &Self::FeedCtx, insts: &[InstId], out: &mut RestReqBuf) -> Result<bool, ProtoErr> { Ok(false) }
    /// insts = 发起该请求时的 inst 列表(同序;响应不带 symbol 的所据此定 inst);返回产出的 Bbo 条数。
    fn decode_bbo_snapshot(rd: &RefData, ctx: &mut Self::FeedCtx, insts: &[InstId], body: &[u8], rx: &RxMeta, out: &mut impl MdSink)
                           -> Result<u16, DecodeErr> { Err(DecodeErr::UnknownClass) }

    // ---- 下单与回报(gw 任务;热) ----
    fn build_templates(cfg: &VenueCfg, rd: &RefData, acct: AccountId, rest: &Credential) -> Result<Self::Templates, ProtoErr>; // 启动期一次
    /// 订单类 op 走 WS 帧还是 REST(契约 29);默认全部 Ws。
    fn order_channel(op: GwOp) -> OrderChannel { OrderChannel::Ws }
    fn encode(rd: &RefData, t: &Self::Templates, c: &GwCmd, tgt: &TargetRef, clk: &WallClock, body: &mut [u8])
              -> Result<usize, EncodeErr>;                                       // 按 c.op 填 Place/Cancel/Amend/CancelAll/Query(订单类 op)
    /// order_channel(c.op) = Rest 的订单操作(如 Binance USDT-M 按 symbol 全撤、OKX/Bitget 查单):生成签名 REST 请求(契约 29)。
    fn encode_rest(rd: &RefData, t: &Self::Templates, c: &GwCmd, tgt: &TargetRef, clk: &WallClock, out: &mut RestReqBuf)
                   -> Result<(), EncodeErr> { Err(EncodeErr::Unsupported(c.op)) }
    /// 会话维护请求(GwOp::SessionKeepalive,如 Binance USDT-M listenKey PUT 续期);不需要维护的所返回 Ok(false)(默认)。
    fn session_keepalive_request(rd: &RefData, t: &Self::Templates, clk: &WallClock, out: &mut RestReqBuf)
                                 -> Result<bool, EncodeErr> { Ok(false) }
    /// gw 发出的 REST 请求的响应(encode_rest、dead_man_refresh 的 REST 形态、session_keepalive_request、CANCEL_ALL_SWEEP 的挂单列表):
    /// req 是发起请求时 gw 记下的用途与 ReqRef(响应不带 client id 的所据此定槽);输出规则同 decode_report(契约 29)。
    fn decode_rest(rd: &RefData, ctx: &mut Self::GwCtx, req: &RestOrigin, status: u16, body: &[u8], rx: &RxMeta,
                   out: &mut impl ReportSink) -> Result<ReportOutcome, DecodeErr> { Err(DecodeErr::UnknownClass) }
    /// 可选:把一次 drain_out 连续取到的同账户、同类决策合成一帧(OKX batch-orders ≤ 20、Hyperliquid 一批一签)。
    /// 默认不支持;运行时只在返回 Ok 时使用,否则逐条 encode。每条决策仍各得一条 SendReport。
    fn encode_batch(rd: &RefData, t: &Self::Templates, cs: &[GwCmd], tgts: &[TargetRef], clk: &WallClock, body: &mut [u8])
                    -> Result<usize, EncodeErr> { Err(EncodeErr::Unsupported(GwOp::Place)) }
    /// 把 hft-types 的规范 24 位十六进制包装成本所外形(只允许加前缀,规范文本位于末尾);返回写入字节数。
    fn render_cid(id: &ClientOrderId, out: &mut [u8; 48]) -> usize;
    /// 去掉本所外形后交给 ClientOrderId::decode;非我们的格式 → None。
    fn parse_cid(bytes: &[u8]) -> Option<ClientOrderId>;
    fn classify_report(op: MsgOp, head: &[u8]) -> Self::ReportClass;
    fn decode_report(rd: &RefData, ctx: &mut Self::GwCtx, c: Self::ReportClass, payload: &[u8], rx: &RxMeta,
                     out: &mut impl ReportSink) -> Result<ReportOutcome, DecodeErr>;

    // ---- 账户查询(warm 任务 acct-fetch,用它自己的 Templates 与 AcctCtx;不在 gw 线程上,见 hft-venue §4a、契约 27) ----
    /// 有序请求表:Config = 账户配置核验(契约 16);Margin = 保证金/强平价/ADL 轮询(契约 17)。
    /// `margin_data = Push` 的所可返回空表,或返回一组仅在推送过期时使用的兜底请求。
    fn acct_queries(k: AcctQueryKind) -> &'static [AcctQuery];
    fn acct_request(rd: &RefData, t: &Self::Templates, acct: AccountId, q: AcctQuery, clk: &WallClock,
                    out: &mut RestReqBuf) -> Result<(), EncodeErr>;               // 含签名(该账户的独立 HMAC REST key)
    fn acct_decode(rd: &RefData, ctx: &mut Self::AcctCtx, acct: AccountId, q: AcctQuery, status: u16, body: &[u8],
                   s: &mut Self::AcctScratch) -> Result<(), DecodeErr>;
    /// 一个序列的全部响应到齐后调用:产出 AccountConfigReport(Config)或 MarginReport + PositionReport(Margin)。
    fn acct_finish(rd: &RefData, ctx: &mut Self::AcctCtx, acct: AccountId, k: AcctQueryKind, s: &Self::AcctScratch,
                   rx: &RxMeta, out: &mut impl ReportSink) -> Result<(), DecodeErr>;

    // ---- 只读查询族(冷;reconciler 调用,gw 只用 OpenOrders 做 CancelAll 兜底;契约 31) ----
    /// 本产品线没有该查询(如现货的 Positions、FundingFlow)→ Err(EncodeErr::Unsupported(GwOp::Query))
    fn ro_request(rd: &RefData, t: &Self::Templates, acct: AccountId, q: &RoQuery, clk: &WallClock, out: &mut RestReqBuf)
                  -> Result<(), EncodeErr>;
    fn ro_decode(rd: &RefData, acct: AccountId, q: &RoQuery, status: u16, body: &[u8], out: &mut impl RoSink)
                 -> Result<RoPage, DecodeErr>;

    // ---- 限速、过期、死手 ----
    fn rate_rules() -> &'static [RateRule];      // 官方默认(协议常量);生效值取 refdata RateLimitTable
    fn stale_guard() -> StaleGuard;              // recvWindow / expTime / x-gate-exptime / expiresAfter(契约 10)
    fn dead_man() -> DeadManSpec;
    fn dead_man_refresh(rd: &RefData, t: &Self::Templates, timeout_ms: u32, insts: &[InstId], clk: &WallClock, out: &mut DeadManOut)
                        -> Result<(), EncodeErr>;                                // WS 帧或 REST 请求;insts 只对按 inst 计时的所有意义;dead_man() = None 的所返回 Err(Unsupported(GwOp::DeadManRefresh))
    fn needs_per_order_signing() -> bool { false }                               // Hyperliquid 为 true
}
```

`DeadManOut` 由 gw 在启动期按 refdata 每 venue inst 上限预留容量,运行中不扩容(热路径零分配);`RoSink` 的输出只给冷调用方。`ClientOrderId`(96 bit 逻辑值与规范 24 位十六进制编解码)在 [hft-types](../hft-types/README.md),语义与分配在 [hft-oms](../hft-oms/README.md#clientorderid);`render_cid`/`parse_cid` 只负责本所外形,不另定义编码。`VenueCaps` 与各字段的枚举(含 `LimitScope`、`LimitBucket`、`DeadManCap`、`MarginMode`)在 [hft-refdata](../hft-refdata/README.md);`MarketKind` 在 hft-types;`ExternalFill`、`MarginReportHdr`/`MarginItem`、`AccountConfigReportHdr`/`LeverageItem` 的事件体在 [hft-event](../hft-event/README.md)。

### 辅助类型

全部定长、`Copy`,无堆(`login`/`subscribe`/`private_subscribe` 的输出缓冲除外,它们只在 warm 线程调用):

```rust
pub type Role = hft_event::ConnRole;                                // Md = 1、Order = 2、UserStream = 3、Rest = 4;编码归 event-model
#[derive(Copy, Clone)] pub struct RouteId(pub u8);                  // 0 = 默认路由
/// 行情路由的种类;BBO 流、分档快照流与其余行情流不同连接。
pub enum RouteKind {
    Bbo,        // bookTicker 类;由 feed-bbo 任务承载,事件进 bbo-<venue> 环;可双路赛马
    DepthTopN,  // 分档快照流(每条 = 前 N 档完整快照,契约 26);由 feed-md 承载,产出 L2Snapshot{origin = TopN};无序号链、无 REST 重同步
    Md,         // 成交、标记价/指数价、资金费、公共强平流;增量深度(可选扩展)也在这一类路由上;由 feed-md 承载
}
/// max_streams / max_in_msgs_per_s 是协议常量形式的默认值;运行时生效值取 refdata VenueMeta(ws_max_streams_per_conn、ws_max_in_msgs_per_s),
/// hft-cfg 以这里的值为默认生成 refdata,覆盖只能更小(hft-cfg 加载校验)。
/// topn_levels:DepthTopN 路由支持的档位(升序,其余路由为空);运行时按 hft-venue feed.topn_levels 选档。
pub struct RouteSpec { pub id: RouteId, pub kind: RouteKind, pub name: &'static str, pub race: bool /* 只有 Bbo 路由可为 true */,
                       pub max_streams: u16, pub max_in_msgs_per_s: u16, pub topn_levels: &'static [u8] }
/// 一个 inst 上可订阅的流;运行时由 FeedCmd 的流集合换算(hft-venue §1),venue 决定它落在哪条路由、哪个频道。
#[repr(u8)] pub enum StreamKind { Bbo = 1, DepthTopN = 2, DepthIncr = 3 /* 增量深度,可选扩展 */, Trade = 4, Mark = 5, Index = 6, Funding = 7, Liquidation = 8 }
pub struct SubSpec { pub inst: InstId, pub stream: StreamKind, pub levels: u8 /* DepthTopN 的档数,其余 0 */, pub route: RouteId }
pub enum MsgOp { Text, Binary }
#[repr(C)] pub struct RxMeta {   // 48 B
    pub rx_tsc: u64,             //  0
    pub hw_rx_ns: i64,           //  8 = hft-net Timestamps::best_rx().0(调用方已做 HW → SW 的退化;都没有为 0)
    pub sw_rx_ns: i64,           // 16 内核软件 RX 戳;填进事件的 sw_rx(MarketHdr、LatencySample,布局归 hft-event)
    pub prev_override: u64,      // 24 ≠ 0:运行时要求本条消息的 L2Batch.prev_id 写成该值(增量深度快照后首条 = snap_id)
    pub conn: ConnId,            // 32 送达该消息的连接;写进 MarketHdr.conn
    pub account: AccountId,      // 33 回报与账户查询所属账户;行情消息不读此字段
    pub _pad: [u8; 2],           // 34
    pub ts_flags: u32,           // 36 = hft-net Timestamps.flags 原值(TsFlags)
    pub rx_empty_gap_ns: u32,    // 40 = hft-net Timestamps.rx_empty_gap_ns 原值(产生规则归 hft-net §7);填进 MarketHdr.rx_empty_gap_ns
    pub _pad2: u32,              // 44
}

/// 归一化序号(与 hft-book / event-model 的 L2Batch/L2Delta.prev_id/last_id 同义)。
#[repr(C)] pub struct SeqIds { pub prev_id: u64, pub last_id: u64 }
/// 交易所原始序号,只给 align_first 用(快照衔接规则各所不同)。
#[repr(C)] pub struct RawSeq { pub first: u64, pub last: u64, pub prev: u64, pub has_prev: bool, pub _pad: [u8; 7] }
pub struct Peek { pub inst: InstId, pub ids: SeqIds, pub raw: RawSeq }   // inst = 0:未订阅符号
pub enum SeqRule {
    None,          // 无序号:只按到达转发(不去重)
    Monotonic,     // 单调 id(BBO update_id、trade_id):last_id > 已转发 才转发 —— BBO 双路去重
    ByTime,        // 无 id,按交易所时间单调(markPrice、funding)
    Chain,         // L2 增量(单路,可选扩展):venue 已归一化 prev_id/last_id,连续 ⇔ prev_id == 上一条 last_id
    Snapshot,      // 每条都是完整快照(Bitget books1、有序号的分档快照流):Monotonic 去重,无断裂;只有时间戳的分档快照用 ByTime
}
pub enum Align { Drop, Apply, Gap }
pub enum ResyncMethod { RestSnapshot, Resubscribe, None }
pub struct SnapshotInfo { pub inst: InstId, pub snap_id: u64, pub levels: u32 }
pub struct HeartbeatSpec {
    pub ping: PingKind,          // WsPing | AppText | AppJson
    pub interval_ms: u32, pub unconditional: bool,   // unconditional:有数据也按周期发(Bitget)
    pub pong_timeout_ms: u32, pub idle_timeout_ms: u32,
}
pub enum StaleGuard { None, RecvWindow { default_ms: u32 }, ExpireAt { default_ms: u32 } }   // 选择规则见契约 10
/// 倒计时类死手的不变量(hft-cfg 生成 refdata 时校验,违反即加载失败):timeout_ms ≥ max(min_ms, 2 × refresh_ms)
/// ——丢失一次刷新不会触发交易所侧全撤;DeadManRefresh 的 arg 取 timeout_ms,sequencer 的 DeadMan 定时器周期取 refresh_ms。
/// 与 refdata VenueCaps.dms 的对应(规则归 hft-refdata"死手";hft-cfg 生成并校验,契约 15):变体与 DeadManCap 同名一一对应
///   (None、CountdownPerInst、CountdownAccount、OnDisconnect、Schedule;码值归 hft-refdata);时长一律毫秒,原样写入
///   dms_min_ms ← min_ms(Schedule:min_ahead_ms)、dms_max_ms ← max_ms(没有上限的变体写 0)、dms_timeout_ms ← timeout_ms
///   (OnDisconnect:window_ms)、dms_refresh_ms ← refresh_ms(OnDisconnect 写 0)。交易所线上以秒计的,由 dead_man_refresh 在编码时换算。
pub enum DeadManSpec {
    None,                                                        // 由 kill-switch 进程兜底
    CountdownPerInst { min_ms: u32, timeout_ms: u32, refresh_ms: u32, via: Via }, // Binance USDT-M countdownCancelAll
    CountdownAccount { min_ms: u32, max_ms: u32, timeout_ms: u32, refresh_ms: u32, via: Via }, // OKX cancel-all-after
    OnDisconnect { window_ms: u32 },                             // Bybit DCP(机构)
    Schedule { min_ahead_ms: u32, max_triggers_per_day: u16, timeout_ms: u32, refresh_ms: u32 }, // Hyperliquid scheduleCancel:反复把时刻推后即心跳;只有真正触发才计次
}
pub enum Via { Ws, Rest }
pub struct WallClock { pub unix_ms: i64, pub unix_us: i64 }     // gw 在填帧前从 TSC 校准换算,不调用系统时钟
/// Cancel/Amend/Query 的目标:gw 从自己的槽镜像查出(GwCmd 只带 target_slot)。
pub struct TargetRef { pub id: ClientOrderId, pub exch_order_id: u64 /* 0 = 未知 */ }

/// 运行时交给协议的配置;hft-venue 启动期由 hft-core 配置组装(主机与端点取自 refdata 端点表,不在这里)。
pub struct VenueCfg {
    pub env: VenueEnv,                 // Prod | Testnet(测试网/模拟盘:选端点表与请求头,如 OKX x-simulated-trading)
    pub stale_guard_ms: u32,           // 请求过期字段的窗口 = hft-venue gw.stale_guard_ms(覆盖 stale_guard() 的默认值)
}
pub enum VenueEnv { Prod, Testnet }
/// 一把 API 凭证:调用方(hft-core 从 Secrets Manager 载入、reconciler 读只读 key)持有,venue 只借用;Debug 不输出密钥材料。
pub struct Credential { pub api_key: ArrayBuf<128>, pub secret: SecretBuf, pub passphrase: ArrayBuf<64> /* OKX、Bitget;其余空 */, pub alg: SignAlg }
pub enum SignAlg { HmacSha256, Ed25519 }
/// bootstrap 的输出:warm 线程先执行的 REST 请求;2xx 响应体原样作为 endpoint 的 boot 参数。
pub type BootCall = RestReqBuf;
/// dead_man_refresh 的输出:Via::Ws → frame 一帧(经下单 primary 发送);Via::Rest → reqs 每个请求一项(按 inst 计时的所每个 inst 一个)。
pub struct DeadManOut { pub frame: ArrayBuf<1024>, pub reqs: Vec<RestReqBuf> }
pub enum OrderChannel { Ws, Rest }
/// gw 发出的一个 REST 请求的来源,随响应交给 decode_rest。
pub struct RestOrigin { pub what: RestWhat, pub req: Option<ReqRef> /* Order 时 = 发起该请求的 GwCmd 的 (slot, gen, op) */, pub inst: InstId /* 0 = 不针对 inst */ }
pub enum RestWhat { Order, DeadMan, Keepalive, Sweep /* CANCEL_ALL_SWEEP 的挂单列表 */ }

/// REST 请求缓冲(快照、BBO 补帧、查单、死手、账户查询共用);定长,gw 与 snap-fetch 复用。
pub struct RestReqBuf {
    pub endpoint: u8,          // 该 venue REST 端点表的下标(0 = 主端点;表的顺序与默认主机由 venue README 的端点表给出,配置规则归 hft-venue §4a)
    pub method: Method,
    pub path_query: ArrayBuf<1024>, pub headers: ArrayBuf<512>, pub body: ArrayBuf<1024>,
    pub redacted: bool,        // 含签名:运行时写 RawTx 时只记元数据
}

// ---- 账户查询 ----
#[repr(u8)] pub enum AcctQueryKind { Config = 1, Margin = 2 }
#[derive(Copy, Clone)]
pub struct AcctQuery {
    pub kind: AcctQueryKind,
    pub step: u8,            // 序列内序号;venue 私有含义(如 Binance:0 = positionSide/dual,1 = multiAssetsMargin…)
    pub every: u8,           // 每 N 次轮询才执行一次(1 = 每次;Binance adlQuantile 取 6)
    pub push_fallback: bool, // 仅在 margin_data = Push 且推送过期时使用
    pub weight: u16,         // 该请求的名义 REST 权重(取自该 venue README 的"保证金数据换算"/"账户配置核验"表);gw 在序列开始时预记入限速计数
}
/// 回报解码给运行时的提示:立即执行一次账户查询(不改变任何状态)。
#[repr(u8)] pub enum AcctHint { None = 0, MarginNow = 1, ConfigRecheck = 2 }
/// 用户流里不产出事件的消息(契约 20)。
#[repr(u8)] pub enum Skip { No = 0, Listed = 1 /* 在 ignorable_user_events 里 */, Unknown = 2 /* 未知事件名 */ }

/// 去重键(契约 22)。成交 id 按交易对编号(Binance、OKX 等),不同 inst 可能撞号,所以键总是带 inst。
pub enum TradeKey {
    Id(u64),                                       // 交易所成交 id > 0(字符串 id 取 xxh3-64,结果为 0 时取 1)
    OrderSeq { exch_order_id: u64, fill_seq: u64 }, // 成交 id 缺失、为 0 或为负(强平/ADL 等):fill_seq = 该订单本笔之后的累计成交量 raw(交易所给的累计字段)
    Funding { settle_ts_ns: i64, disc: u64 },      // 资金费结算推送:结算/记账时刻 + 区分同一时刻不同推送的字段(各 venue README 写明)
}
pub fn dedup_key(inst: InstId, k: TradeKey) -> u64;   // xxh3-64(tag ‖ inst ‖ 字段),tag 区分三种键;结果为 0 时取 1(0 是环的空位)
/// 去重环:venue 的 GwCtx 每账户一个;存 dedup_key 的结果。
pub struct TradeIdRing<const N: usize> { /* [u64; N] + 写指针;线性查找,N = 1024 时 ≤ 0.3 µs */ }
impl<const N: usize> TradeIdRing<N> {
    pub fn seen_or_insert(&mut self, key: u64) -> bool;   // 已见过返回 true(调用方丢弃该推送);未见过则插入,满时挤出最老的;key = 0 → debug_assert,返回 false 且不插入
}

// ---- 限速(枚举复用 hft-refdata 的 LimitScope / LimitBucket) ----
pub struct RateRule { pub scope: LimitScope, pub bucket: LimitBucket, pub window_ms: u32, pub limit: u32 }   // 官方默认;window_ms > 0、limit > 0
pub struct RateObs { pub bucket: LimitBucket, pub used: u32, pub limit: u32, pub window_ms: u32 }           // 响应里的实时余量

pub enum ControlHint { SubAck { ok: bool, code: u32 }, ServerShutdown, SessionExpired, Maintenance, Pong }
pub struct MdOutcome { pub events: u16, pub inst: InstId }
pub struct ReportOutcome {
    pub req: Option<ReqRef>, pub rate: Option<RateObs>, pub gw_dwell_us: u32 /* 0 = 无网关戳 */, pub events: u16,
    pub outcome_unknown: Option<ReqRef>,   // 响应码属于"结果未知"(含未登记码,契约 8):不产出 Reject;运行时据此发 OrderEvent{what: OutcomeUnknown}
    pub raw_code: u32,                     // outcome_unknown 或 Reject 的原始码,按契约 8 编码;0 = 无
    pub skip: Skip,                        // 契约 20:Listed/Unknown 只计数,不是解码失败
    pub hint: AcctHint,                    // 如 MARGIN_CALL → MarginNow;保证金模式变化 → ConfigRecheck
    pub fee_rule_deviation: bool,          // 契约 19:实际扣费资产与 fee_asset_rule 不符(只计数告警,不改账)
    pub ctl: ReportCtl,                    // 契约 30:给运行时的连接级提示
}
pub struct ReqRef { pub slot: u32, pub gen: u32, pub op: GwOp }   // 从请求 id 解出(REST 响应取自 RestOrigin)
#[repr(u8)] pub enum ReportCtl {
    None = 0,
    ClockSkew = 1,        // 交易所报请求时间戳越界(如 Binance -1021)
    RateLimited = 2,      // 交易所报限速(如 Binance -1003/-1015/429/418、OKX 50011)
    UserStreamEnded = 3,  // 用户流被交易所终止(Binance listenKeyExpired、eventStreamTerminated、续期报 listenKey 不存在)
    ServerShutdown = 4,   // 私有连接上的下线/维护预告(如 OKX 64008)
}

// ---- 只读查询族(契约 31) ----
#[repr(u8)] pub enum RoKind { OpenOrders = 1, Fills = 2, FundingFlow = 3, Balance = 4, Positions = 5 }
pub struct RoQuery {
    pub kind: RoKind,
    pub inst: InstId,        // 0 = 全部(本所按 symbol 查询的,调用方逐 inst 调用;inst = 0 → Unsupported)
    pub cursor: u64,         // Fills/FundingFlow:上一页 RoPage.next_cursor;0 = 从 since_ms 起
    pub since_ms: i64,       // Fills/FundingFlow 的起点(UTC ms)
    pub limit: u16,          // 每页条数;0 = 本所默认
}
pub struct RoPage { pub rows: u32, pub next_cursor: u64 /* 0 = 已到末页 */ }
/// 成交历史的一行;记录不带 client id 的所(README 写明)cid = None。
pub struct FillRow { pub inst: InstId, pub side: Side, pub qty: Qty, pub px: Px, pub fee: Qty, pub fee_asset: AssetId,
                     pub exch_order_id: u64, pub trade_id: u64, pub ex_ts_ns: i64, pub cid: Option<ClientOrderId>,
                     pub cause: Option<ExternalCause> /* 按 own_liquidation_marker 判出的强平/ADL/交割;其余 None */ }
pub trait RoSink {
    fn open_order(&mut self, e: &OrderEvent);       // what = QueryResult;slot/gen/epoch 由 client id 解出,解不出 = UNRESOLVED;detail = 订单状态码(契约 28)
    fn fill(&mut self, r: &FillRow);
    fn funding(&mut self, e: &FundingSettle);       // 资金费流水逐笔:inst 必填,settle_ts_ns = 交易所原始结算时刻,synth = 0
    fn balance(&mut self, e: &BalanceReport);
    fn position(&mut self, e: &PositionReport);
}

// ---- 原始码编码(契约 8;src/code.rs) ----
pub const RAW_CODE_NONE: u32 = 0;                 // 无响应(传输层失败)
pub fn raw_code_num(c: i32) -> u32;               // 交易所数值码:i32 补码
pub fn raw_code_str(s: &[u8]) -> u32;             // 契约 8 的字符串码规则(数字串按数值,其余取哈希段)
pub fn raw_code_http(status: u16) -> u32;         // 0x2000_0000 | status
/// 协议解码/编码错误。与 hft_event::DecodeErr(事件体解码)不是同一类型;venue 代码以 hft_venue_proto:: 路径引用。
pub enum DecodeErr { Truncated /* 报文在必需字段前结束 */, Shape /* 结构或字段类型不符 */, Number /* 数值解析失败(契约 4) */, Overflow, UnknownClass /* 以本方法不处理的类调用 */ }
pub enum EncodeErr { FieldOverflow /* 输出缓冲或字段放不下 */, UnknownInst /* inst 不属于本 venue */, UnknownTarget /* 本所需要 exch id 而 TargetRef.exch_order_id = 0 */, Unsupported(GwOp) }
pub enum ProtoErr { BufFull /* 输出缓冲放不下 */, Unsupported /* 本所不支持该角色、路由或请求 */, Cfg /* VenueCfg 缺必需项 */ }
```

`RateRule` 是协议常量形式的默认值;生效限额是 [hft-refdata](../hft-refdata/README.md) 的 `RateLimitTable`(hft-cfg 用 `rate_rules()` × 账户等级生成),本 crate 不重复定义它。`RouteSpec` 的流数与入向消息上限同理(生效值在 refdata `VenueMeta`)。`HeartbeatSpec` 只有协议常量,没有 refdata 对应项。

### 解码输出 sink

由调用方实现(hft-venue 写入环;测试里是 `VecSink`;mock-exchange 有自己的实现),`venue-*` 只调用它们:

```rust
pub trait MdSink {
    fn bbo(&mut self, e: &Bbo);                     // Bbo 路由 → bbo-<venue> 环
    /// 一条交易所增量消息 = 一条变长 L2Batch(增量深度,可选扩展):begin(头:inst、prev_id、last_id、时间戳)→ N × level → end。
    fn l2_begin(&mut self, h: &L2BatchHdr);
    fn l2_level(&mut self, side: Side, px: Px, qty: Qty);   // qty = 0 删档
    fn l2_end(&mut self);
    /// 快照,类型以 hft-event 为准;运行时按 hft-venue feed.snapshot_part_levels 切片。分档快照流(契约 26)由 feed-md 的 sink
    /// 写进 md-<venue>-bulk;REST 快照由 snap-fetch 的 sink 写进 snap 记录。
    fn l2_snapshot(&mut self, part: &L2SnapshotPart<'_>);
    fn trade(&mut self, e: &Trade);
    fn mark(&mut self, e: &MarkPrice);  fn index(&mut self, e: &IndexPrice);
    fn funding(&mut self, e: &Funding); fn liquidation(&mut self, e: &Liquidation);   // Liquidation = 公共强平流,与本账户无关
}
pub trait ReportSink {
    fn order(&mut self, e: &OrderEvent);            // 必填 cum_filled 与 avg_px(契约 9)、epoch(契约 13);晚到费用用 FeeOnly(契约 12);slot 可为 SlotId::UNRESOLVED,运行时用 id 表补
    fn position(&mut self, e: &PositionReport);     // 只用于校验
    fn balance(&mut self, e: &BalanceReport);       // 只用于校验
    fn funding_settle(&mut self, e: &FundingSettle);   // 按 funding_settle 能力推送(契约 14),已按契约 22 去重
    fn external_fill(&mut self, e: &ExternalFill);  // 契约 18:强平/ADL/人工/交割/未知来源的成交
    fn margin(&mut self, hdr: &MarginReportHdr, items: &[MarginItem]);   // 契约 17;线上分条规则归 hft-event,运行时负责切分写环
    fn account_config(&mut self, hdr: &AccountConfigReportHdr, lev: &[LeverageItem]);   // 契约 16;同上
    /// 交易所订单 id ↔ 槽(运行时的 IdTable,hft-venue §6):owner = Some(id) 时记录(Ack 同时带 client id 与 exch id),
    /// None 时查询(回报只带 exch id)。
    fn exch_id(&mut self, key: &[u8], owner: Option<&ClientOrderId>) -> ExchIdHint;
}
pub enum ExchIdHint { Resolved { epoch: u16, slot: SlotId, gen: u32 }, Recorded, Unknown }
```

事件体定义在 [hft-event](../hft-event/README.md),编号与分类以 [event-model](../../docs/subsystems/event-model.md) 为准。

## 实现方必须遵守的契约

编号稳定,其他文档按编号引用;新增契约只追加。

**行情**

1. **`L2Batch.prev_id/last_id` 归一化**(增量深度,可选扩展;规格保留,启用该 venue 的增量深度前必须实现。一条交易所增量消息一条 `L2Batch`;`L2Delta` 只给需要逐档的读者):`prev_id` = 本消息作用于其上的簿状态 id,`last_id` = 应用后的状态 id;`RxMeta.prev_override ≠ 0` 时 `prev_id` 写成该值(快照后首条 = `snap_id`)。簿只做 `prev_id == last_id` 的通用比较([hft-book](../hft-book/README.md))。各所换算写在各自 README。
2. `peek` 返回的 `ids` 与 `decode_md` 写出的 `L2Batch` 头序号一致;`peek` 不解析价格与数量。
3. 时间戳按 [event-model 时间戳语义](../../docs/subsystems/event-model.md#时间戳语义) 映射;不按数值大小猜单位。`RxMeta` 的三个接收时间点(`rx_tsc`、`hw_rx_ns`、`sw_rx_ns`)、`rx_empty_gap_ns` 与 `conn` 原样写进事件(行情事件的 `MarketHdr`,字段布局归 hft-event);实现方不做 HW → SW 的退化,也不写记录头标志(`HW_TS`/`SW_TS` 由运行时按 `ts_flags` 置)。
4. 价格/数量保持字符串解析为资产指数定点([hft-types](../hft-types/README.md) `parse_exact`);不经 f64;`FxErr::OVERFLOW` → `DecodeErr::Overflow`,其余 `FxErr`(语法、空串、超精度非零位)→ `DecodeErr::Number`。
5. 热路径方法(行情、下单两组)零分配、无系统调用、无 panic;所有错误以返回值表达。

**下单与回报**

6. 传输超时类错误码(如 Binance `-1007`、OKX `50004`)不得解码为 `Reject`,只能走"结果未知"(契约 8;[defensive-patterns](../../docs/defensive-patterns.md))。
7. 登录帧与任何含签名的帧由调用方标 `REDACTED`,实现方不把密钥材料存进任何静态或上下文。唯一例外是 `build_templates` 的 `rest` 凭证:它只保存在该份 `Templates` 里,用于该份模板产出的 REST 签名(`acct_request`、`encode_rest`、`dead_man_refresh`、`session_keepalive_request`、`ro_request`);`FeedCtx`、`GwCtx`、`AcctCtx` 与静态量不含密钥材料;会话凭证只经 `login`、`endpoint`、`bootstrap`、`session_identity_request` 的参数借用。
8. **未登记的错误码默认"结果未知"**(不是 Reject):`decode_report` 只对 `codes.rs` 里显式登记为"确定拒绝"的码产出 `Reject`;登记为"结果未知"的码与**任何未登记码**都返回 `ReportOutcome.outcome_unknown`;运行时据此推一条 `OrderEvent{what: OutcomeUnknown, reject_code = raw_code}`,OMS 把该槽置 `Unknown` 并查询。每个 venue 的 `codes.rs` 必须有两张表:确定拒绝、结果未知;README 有"结果未知码表"一节。**原始码的编码**(`OrderEvent.reject_code` 与 `ReportOutcome.raw_code`,u32;SDK 的 `venue_code: u32` 原样携带,数值码按 i32 补码解读):
    - 交易所数值码(含负数)按 i32 补码存(Binance `-2010` → `0xFFFF_F826`,读回 −2010);
    - 纯数字的字符串码(可带一个前导 `-`)按十进制解析为 i32(OKX `"51008"` → 51008);超出 i32 范围的按非数字处理;
    - 非数字字符串码 → `0x4000_0000 | (xxh3_64(码文本) & 0x00FF_FFFF)`(原文在 raw journal);
    - 只有 HTTP/WS 状态、没有业务码 → `0x2000_0000 | status`;
    - 无响应(传输层失败)→ 0。
    各 venue README 的"结果未知码表"写明本所码的形态。
    **批量响应**(一帧多单,如 OKX `batch-orders`):逐条按上述规则判定;其中结果未知的条目由 `decode_report` 直接产出 `OrderEvent{what: OutcomeUnknown, epoch, slot, gen, reject_code = 该条原始码}`(槽取自该条的 client id),不经 `ReportOutcome.outcome_unknown`;`outcome_unknown` 只用于单单响应,或整帧结果未知且没有逐条数据时(取请求 id 中的第一单)。
9. **累计成交量与均价**:每条成交类 `OrderEvent`(`PartialFill`/`Fill`)**与每条 `QueryResult`** 必须带 `cum_filled`(资产单位)与 `avg_px`(累计均价);查询响应里交易所给了均价字段(Binance `avgPrice`、OKX `avgPx` 等)就必须填,`cum_filled > 0` 时 `avg_px = 0` 视为解码错误。交易所不直接给累计量的,在 `GwCtx` 里按成交 id 去重累加合成(各所来源表见各 venue README)。
10. **查询"订单不存在"何时可定终态**由 `stale_guard()` 决定,实现方在 README 写明本所属于哪一种:
    - `None`(无过期保护)与 `RecvWindow`(Binance `recvWindow`):**不定终态**。`recvWindow` 只约束请求进入撮合前的时间校验,不保证超窗后"查不到"就永不成交(撮合排队、查询可见性延迟);查询返回"不存在"产出 `QueryResult` 的 NotFound 形态,OMS 保持 `Unknown` 并经成交历史确认收敛([hft-oms](../hft-oms/README.md))。
    - `ExpireAt`:超过窗口 + grace 后,**非对冲、非链接**槽的"不存在"可定终态;对冲腿与链接腿在任何 `StaleGuard` 下都要成交历史确认(规则归 [hft-oms](../hft-oms/README.md))。**只有过期字段的生效位置已核实为"撮合入口之前拒绝过期请求"的所才可声明 `ExpireAt`**;核实前 `stale_guard()` 返回 `None`,请求照样可以带过期字段(只减少过期请求被执行的机会,不作定终态依据)。
11. **reduceOnly**:只在 `GwCmd.oflags` 带 `REDUCE_ONLY` 时填交易所的 reduce-only 字段;该标志由 OMS/风控的分类决定([hft-risk](../hft-risk/README.md) `classify`),venue 不推断。`VenueCaps.reduce_only = false` 的产品线(现货)收到带 `REDUCE_ONLY` 的命令 → `EncodeErr::Unsupported`,不静默丢掉该标志。
12. **费用晚到 → `FeeOnly`**:订单已到终态(或该笔成交已报过)之后才到的费用,产出 `OrderEvent{what: FeeOnly, qty: 0, fee, fee_asset, exch_order_id}`,不得用"`Fill` 且 qty = 0"表达。消息不带 client id 时 `slot = UNRESOLVED`,由运行时经 `IdTable`(含终态保留条目,[hft-venue §6](../hft-venue/README.md#6-exch-id--slot-私有表idtable))解析;OMS 对 `FeeOnly` 在任何状态下只把费用交 ledger。各 venue 的"累计量与费用来源"表注明哪些流会产出 `FeeOnly`。
13. **client id → `(epoch, slot, gen)`**:消息带 client id 时,`decode_report` 用 `parse_cid` 解出的 `ClientOrderId` 填 `OrderEvent.epoch/slot/gen`;不带时 `slot = SlotId::UNRESOLVED`、经 `ReportSink::exch_id(key, None)` 让运行时用 `IdTable` 补齐(条目里存 epoch)。实现方不得用当前 epoch 猜测。
14. **资金费按 `funding_settle` 能力推送**,venue 层从不合成:
    - `PerSymbolPush`:交易所推送带 symbol 的实收付 → `FundingSettle{inst, amount, settle_ts_ns, synth: 0}`。
    - `BalanceOnlyPush`:推送只有余额变化、没有 symbol(如 Binance 全仓 `ACCOUNT_UPDATE m=FUNDING_FEE`,待核实,见 venue-binance 开发备忘第 1 条)→ `FundingSettle{inst: INST_NONE, amount, settle_ts_ns, synth: 0}`,用于钱包入账与校验(ledger 按 (account, venue, settle_ts) 记已入钱包额,规则归 [hft-ledger](../hft-ledger/README.md))。
    - `RateOnly`:不产出 `FundingSettle`。
    推送型的 `FundingSettle` 一律填交易所给的结算/记账时刻 `settle_ts_ns`,并按契约 22 去重(两条连接重叠、重订阅回放都不会重复入账)。后两种由 sequencer 在结算时刻按"持仓 × mark × 费率"合成每 inst 的 `FundingSettle{synth: 1}`(规则在 [hft-sequencer](../hft-sequencer/README.md)),reconciler 用带 symbol 的资金流水校正。费率与下次结算时刻一律经 `MdSink::funding` 产出 `Funding`(含实际结算周期)。

**能力表、账户与保证金**

15. **`VenueCaps` 默认值**:每个实现的 `default_caps()` 返回本产品线的完整默认能力表(字段见 [hft-refdata](../hft-refdata/README.md) 的 `VenueCaps`,含 `flags` 的 `AMEND`/`POST_ONLY`/`IOC`/`FOK`/`BATCH_CANCEL`);README 有"VenueCaps 默认值"一节,每个字段标来源等级(官方 / 自测 / 待核实;待核实的写出方法或指向该 venue 开发备忘的条目)。`hft-cfg` 以它为底、叠加 refdata 配置里的覆盖后写入 refdata blob;运行时(gw、sim-gw、风控、执行层)只读 refdata 里的生效值,不调用 `default_caps()`。默认值与 `stale_guard()`、`dead_man()`(`DeadManSpec` 与 `DeadManCap` 同名变体、毫秒时长,对应见上文 `DeadManSpec` 注释)不一致 → hft-cfg 加载失败。
16. **账户配置查询与解码 → `AccountConfigReport`**(启动闸门 `ACCOUNT_CFG`、`IDENTITY`):`acct_queries(Config)` 列出读取该产品线账户配置所需的全部只读请求;`acct_finish(Config)` 产出一条 `AccountConfigReport`(头 `AccountConfigReportHdr` + 杠杆条目 `LeverageItem`,布局与位名归 [hft-event](../hft-event/README.md)):
    - `position_mode`/`margin_mode`/`multi_asset` 按交易所原值换算为 hft-refdata 的通用枚举;现货产品线填 `NotApplicable`。永续各 inst 的保证金模式不一致(部分全仓、部分逐仓)时 `margin_mode = MarginMode::Mixed`(码值归 hft-refdata)。
    - `mismatches` 由本函数对照 `rd` 里该 (venue) 的生效 `VenueCaps`(`required_position_mode`、`required_margin_mode`、多资产要求、`fee_asset_rule`、`required_spot_borrow`)逐项计算;另外检查本所特有、会改变上述语义的设置(扣费币种开关、借币开关、账户等级、API key 是否带提现权限、交易所 UID 是否等于 `AccountMeta.venue_uid`)。本层用到的位见下表。
    - **现货账户必须关闭借币**:现货产品线的 Config 序列必须读到本所的现货借币 / 现货杠杆 / 自动借币开关(各 venue README 的"账户配置核验"表写明端点与字段);开启 → `AUTO_BORROW`,读不到 → `UNREADABLE`。现货钱包本身不能借币的所(`required_spot_borrow = None`)在 README 写明理由并给出"账户类型不是杠杆/统一账户"的核验项(否则 `ACCT_TYPE`);这类所的契约检查以该核验项的触发样本代替借币开关样本。
    - **UID 按 key 核验**(契约 23):Config 序列里的 UID 步用 gw 持有的 REST 签名 key 查询,结果与 `AccountMeta.venue_uid` 比对;会话 key 的结果由运行时从交接记录并入同一份报告。`uid_keys` 记录本报告覆盖了哪几把 key,`UID` 位 = 其中任一把不符,`venue_uid` 字段填交易所返回的 UID。`uid_policy = Unverifiable` 的账户(只允许非正式账户,规则归 [hft-refdata](../hft-refdata/README.md#数据结构))不核对 UID:返回的 UID 缺失或为空时 `UID`、`UNREADABLE` 都不因它置位,`uid_keys` 不含该步覆盖的 key,`venue_uid` 填返回值(缺失为全 0);`Verified` 账户缺 UID 置 `UNREADABLE`。
    - **永续杠杆条目**:永续产品线对 refdata 中该 venue 每个可交易永续 inst 读出账户上的实际杠杆,写 `LeverageItem{inst, leverage}`(读不到写 0);杠杆是观测值,不产生 `mismatches` 位,用法归 [hft-risk](../hft-risk/README.md)。杠杆变化的推送(如 Binance `ACCOUNT_CONFIG_UPDATE` 的 `ac`)给 `ConfigRecheck`。现货 `n_lev = 0`。
    - **读不出的字段一律置位**(fail closed),不按默认值放行。
    - 运行中:回报流里出现配置变化的迹象(Binance `ACCOUNT_CONFIG_UPDATE`、`ACCOUNT_UPDATE m=MARGIN_TYPE_CHANGE` 等)→ `ReportOutcome.hint = ConfigRecheck`,运行时立即重查并产出新的 `AccountConfigReport`(风控据此设 cap)。

    | 位 | 含义 |
    |---|---|
    | `POS_MODE` | 持仓模式 ≠ `required_position_mode` |
    | `MARGIN_MODE` | 保证金模式 ≠ `required_margin_mode`(任一可交易 inst 不符即置位) |
    | `MULTI_ASSET` | 多资产/联合保证金开关 ≠ 要求 |
    | `FEE_RULE` | 扣费相关设置与 `fee_asset_rule` 矛盾(如要求 `Quote` 而开了平台币抵扣) |
    | `ACCT_TYPE` | 账户类型/等级不受支持(如统一账户未开、组合保证金账户、现货账户是杠杆账户) |
    | `AUTO_BORROW` | 现货借币 / 现货杠杆 / 自动借币打开(现货卖超会变成借币) |
    | `KEY_PERM` | API key 带提现权限,或缺交易权限 |
    | `UID` | 任一把已核对的交易 key 对应的交易所 UID ≠ `AccountMeta.venue_uid` |
    | `UNREADABLE` | 某项无法读取或无法解码 |

17. **保证金/强平价/ADL → `MarginReport`**:来源由生效 `VenueCaps.margin_data` 决定。`Push`:在 `decode_report` 里由账户/持仓推送产出(`GwCtx` 缓存最近一次的账户级字段与每 inst 条目,任一更新即产出一份完整报告;它在 gw 线程上解码,最大推送类型的预算见 [hft-venue 延迟与资源契约](../hft-venue/README.md#延迟与资源契约));`Poll{interval_ms}`:运行时按周期执行 `acct_queries(Margin)`,`acct_finish(Margin)` 产出 `MarginReport` 与每 inst 一条 `PositionReport`(在 acct-fetch,契约 27)。统一口径(与 [hft-event](../hft-event/README.md) `MarginReportHdr`、[hft-ledger](../hft-ledger/README.md) 的权益视图一致):
    - `margin_balance` = 钱包余额 + Σ未实现盈亏(USDT 名义;多资产账户用交易所给出的折算后权益);
    - `maint_margin` = 维持保证金;`avail_margin` = 可用于开仓的保证金;
    - `margin_ratio` = `maint_margin / margin_balance`(`Rate`,1e-9,向上取整;越大越危险,≥ 1 即达强平线),**一律由这两个字段算出**,不直接采用交易所给的比率字段(交易所口径可能另含强平手续费等项);`margin_balance ≤ 0` 时填 `Rate::MAX`。
    - 每 inst 条目 `(inst, liq_px, adl_quantile)`:`liq_px = 0` 表示无强平价;`adl_quantile` 归一化为 0..=100(交易所档位 / 最高档 × 100,向下取整),255 = 该所不提供。
    - 每个 venue README 有"保证金数据换算"表(端点/频道、字段、换算、频率与权重)。
    - 现货产品线没有保证金:`acct_queries(Margin)` 为空,不产出 `MarginReport`。
18. **自身强平、ADL、人工与交割成交 → `ExternalFill`**:对每条成交类消息按以下顺序判定,同一成交只走一条路径:
    1. 命中本所的 `own_liquidation_marker`(如 Binance 的强平 client id 前缀、OKX `category`)→ `ExternalFill{cause: Liquidation | Adl | Delivery}`,不论 client id 是否可解析;
    2. client id 存在且 `parse_cid` 成功 → 本系统订单,走 `OrderEvent`;
    3. client id 存在但不是我们的格式 → `ExternalFill{cause: Manual}`(能认出网页/App 来源时)或 `ExternalFill{cause: Unknown}`;
    4. client id 缺失:本所"我方订单的成交推送必带 client id"(README 标明)时同第 3 步;否则产出 `OrderEvent{slot: UNRESOLVED}` 交运行时按 `IdTable` 解析,暂存超过 `gw.unmatched_hold_ms`(默认 5 000)仍未解析的,由运行时转为 `ExternalFill{cause: Unknown}`([hft-venue §4](../hft-venue/README.md#4-gwruntimepoll_once-的循环结构))。
    `ExternalFill` 带成交方向(我方视角)、成交量、价格、费用与费用资产、`exch_order_id`、`trade_id`(契约 22 的成交 id;缺失或 ≤ 0 时填 0)、`ex_ts_ns`;强平类成交的清算费放在 `fee` 中(交易所单独列出时)。它不迁移任何订单槽;ledger 入账、LegTracker 更新、风控报 P0、策略收到 `External`(下游语义见 [hft-oms](../hft-oms/README.md)、[hft-ledger](../hft-ledger/README.md))。每个 venue README 有"外部成交识别"表(即 `own_liquidation_marker` 的实现),每项待核实的都写核实方法。
19. **手续费与扣费资产**:每条带费用的事件(`PartialFill`、`Fill`、`FeeOnly`、`ExternalFill`)填**交易所报告的实际** `fee` 与 `fee_asset`(`fee > 0` = 支出,返佣为负;OKX 等以负数表示扣费的所在此取反)。消息不带费用的,`fee = 0`、`fee_asset = ASSET_NONE`,由之后的 `FeeOnly` 补齐;不得按 `fee_asset_rule` 猜一个非零费用。`fee_asset_rule` 用于下单前的净量估算、模拟撮合与一致性检查:实际 `fee_asset` 不在规则允许的集合内(如 BNB 抵扣余额耗尽后改扣成交币)→ `ReportOutcome.fee_rule_deviation = true`(运行时计数、写 Meta Note、watchtower 告警),**不改账**——ledger 一律按实际 `fee_asset` 计算 `FillDelta.net_qty`。
20. **可忽略与未知的用户流事件不是解码失败**:`classify_report` 不带 refdata,只认编译进来的默认名单(`default_caps()` 的 `ignorable_user_events`):命中返回 `Ignorable` 类,不认得的事件名返回 `Unknown` 类;`decode_report` 对 `Ignorable` 返回 `Ok(ReportOutcome{skip: Listed, events: 0})`,对 `Unknown` 再查生效 `VenueCaps.ignorable_user_events`(refdata 可追加,不需发版):命中 → `skip: Listed`,否则 `skip: Unknown`;二者都 `events: 0`,不产出事件、不返回 `DecodeErr`。只有"类别已知、内容解析失败"才返回 `DecodeErr`(运行时按 `REPORT_DECODE` 处理)。每个 venue README 有"用户流事件表":事件名 → 产出事件 / 提示 / 可忽略。
21. **两种 MarketKind 都覆盖**:core 交易所(Binance、OKX、Bitget)的 crate 同时实现现货与 USDT 本位永续两个产品线;README 的每张必备表对两者分别给出(不适用的写"不适用"及原因)。**一个账户在同一交易所只用其中一个产品线**(refdata 加载校验 `SameExchangeMarkets`,规则归 [hft-refdata](../hft-refdata/README.md)):不同账户可以分别用现货与永续,但同所现货与永续共用一个钱包的情形被排除,本层不建模两条产品线之间的资金占用——账户级推送与查询(余额、保证金)只产出到该账户启用的那一个 venue。
22. **成交与资金费推送去重**(本条是去重键的唯一定义;hft-event `ExternalFill.trade_id`、hft-oms、hft-ledger 引用本条):
    - 同一笔成交至多产出一次成交增量、一次 `FeeOnly`、一次 `ExternalFill`。键 = `dedup_key(inst, TradeKey)`:成交 id > 0 时用 `(inst, trade_id)`;成交 id 缺失、为 0 或为负(强平、ADL 等交易所自生成的成交)时**不以 id 入环**,改用 `(inst, exch_order_id, fill_seq)`,`fill_seq` = 该订单本笔之后的累计成交量(交易所给的累计字段,重复推送得同一值;来源字段写在各 venue README 的"累计量与费用来源"表)。
    - 推送型 `FundingSettle` 至多产出一次:键 `dedup_key(inst, TradeKey::Funding{settle_ts_ns, disc})`,`inst` 为推送里的 inst(`BalanceOnlyPush` 为 `INST_NONE`),`disc` 区分同一结算时刻的不同推送(同一推送重复送达时相同;取值字段写在各 venue README 的"用户流事件表")。
    - `GwCtx` 每账户持有 `TradeIdRing<1024>`;私有频道在两条连接上短暂重叠(计划切换的先建后断)与同一进程内重订阅后的回放因此无害。环只在进程内有效、不进快照;hft-core 重启后由运行时从 raw-gw journal 尾部重建([hft-venue §6a](../hft-venue/README.md#6a-去重环的重启重建))。
    - 不带成交的推送(余额、持仓)只做校验,重复无害。

**身份与连接健康**

23. **每把交易 key 核 UID**:一个 (venue, 账户) 用到的每把交易凭证都要查一次交易所 UID 并与 `AccountMeta.venue_uid` 比对(`Unverifiable` 账户的处置见契约 16)。REST 签名 key 在 Config 序列里查(契约 16);会话凭证与 REST key 不是同一把时(Binance:Ed25519 会话 key + HMAC REST key),`session_identity_request` 生成用会话凭证签名的只读请求,warm 线程登录后执行、`decode_identity` 解出 UID,随交接交给 gw 并入 `AccountConfigReport.uid_keys`/`UID`(运行时步骤见 [hft-venue §7](../hft-venue/README.md#7-会话建立warm-线程-conn-buildersessiondriver))。同一把凭证的所返回 `Ok(false)`,REST 步的结果同时覆盖两种用途。查询端点与字段写在各 venue README 的"账户配置核验"表;找不到能按某把 key 返回 UID 的接口 → 该所不能上线(README 写明,标待核实并写出核实方法)。
24. **BBO 连接的存活与补帧**:`bbo_probe()` 给出 BBO 连接上运行时每 `feed.bbo_ping_ms` 发送的探测(默认 WS Ping;服务端不回 WS Pong 的所改用本所应用层 ping,README 写明并标来源);探测与回帧计入该连接的入向消息限额(`RouteSpec.max_in_msgs_per_s` 的生效值)。订阅回执后本所不立即推当前 BBO 的,实现 `bbo_snapshot_request`/`decode_bbo_snapshot`,供运行时在"该 inst 的全部 BBO 连接都断过"之后取一帧最新报价(请求由 snap-fetch 执行,[hft-venue §3a、§3b](../hft-venue/README.md#3a-bbo-连接健康));订阅即推首帧的所保留默认实现,并在 README 写明来源。`BBO_SNAPSHOT_BATCH` 声明一次请求可覆盖的 inst 数(只能按单个 inst 查的所为 1);输入超过它、或请求放不进 `RestReqBuf` → `Err(ProtoErr::BufFull)`;`decode_bbo_snapshot` 按传入的 `insts` 定 inst(响应里不在其中的 symbol 跳过)。

**快照**

25. **快照解码与行情解码互不依赖**:`snapshot_request`、`decode_snapshot`、`bbo_snapshot_request`、`decode_bbo_snapshot` 由 warm 任务 snap-fetch 在**它自己的 `FeedCtx` 实例**上调用(与 feed 任务的实例用同一构造参数创建,之后各自演进)。实现不得让这些方法依赖 `decode_md` 写进 `FeedCtx` 的状态,也不得留下 `decode_md` 需要读取的状态;快照衔接所需的序号只经 `SnapshotInfo` 返回值与 `align_first` 传递。`decode_snapshot` 的输出经 `MdSink::l2_snapshot` 分片,单次调用在 warm 线程上完成(预算见"延迟与资源契约")。

**分档快照与账户查询**

26. **分档快照流(`RouteKind::DepthTopN`)**:`routes()` 给出一条 `DepthTopN` 路由及其支持的档位(`RouteSpec.topn_levels`);该路由的每条消息是一个 inst 前 N 档的完整快照,`decode_md` 经 `MdSink::l2_snapshot` 交出 `L2Snapshot{origin = TopN}`(`snap_id` = 消息里的序号,没有则 0;`origin` 码归 [hft-event](../hft-event/README.md)),不产出 `L2Batch`。`seq_rule` 为 `Snapshot`(消息带单调序号)或 `ByTime`(只有交易所时间),`resync_method` 为 `None`;实现不得要求 REST 快照或序号链。簿按整替换应用(hft-book `apply_snapshot`)。每个 venue README 有"分档快照流"一节:按产品线写频道名、档位、推送节拍、序号与时间字段、路由与端点,未核实的写"待核实(方法:…)"。
27. **账户查询与回报解码互不依赖**:`acct_request`、`acct_decode`、`acct_finish` 由 warm 任务 acct-fetch 调用,用它为该账户构建的 `Templates` 与**它自己的 `AcctCtx`**(跨序列的 venue 私有状态,如上一次的 ADL 档);实现不得读写 `GwCtx`,也不得让 `decode_report` 依赖这些方法留下的状态。推送型保证金数据(契约 17 的 `Push`)只在 `decode_report` 里产出,不经这三个方法;`push_fallback` 请求的结果是一份独立完整的报告,不并入推送缓存。reconciler 复用这些纯函数时自己构造 `AcctCtx::default()`(kill-switch 不链接 `venue-*`,其依赖规则归 [kill-switch](../../apps/kill-switch/README.md))。

**订单事件细节、REST 出口、控制提示与只读查询**

28. **`OrderEvent.detail`**(u8;字段与码值归 [hft-event](../hft-event/README.md)):`QueryResult` 填订单状态码(New=1 … NotFound=7,与 `ReconcileReport.order_status` 同码);`CancelReject`/`AmendReject` 在 `codes.rs` 把该码登记为"订单已成交、已撤或不存在"类时填 `GONE`,其他原因填 0;其余 `what` 填 0。每个 venue README 的"结果未知码表"写明哪些码属 `GONE` 类,查询响应的状态映射写在下单一节。
29. **REST 形态的订单操作与会话维护**:`order_channel(op) = Rest` 的订单类 op 由 `encode_rest` 生成签名请求,其余 op 由 `encode` 写 WS 帧;同一 op 只走一种通道。`CancelAll` 的展开(账户级 → 按 inst → 按单)由运行时按 `Unsupported(CancelAll)` 逐级进行,两种通道相同([hft-venue 出环](../hft-venue/README.md#出环outmsg--gwcmd));`CANCEL_ALL_SWEEP = true` 的所,运行时发完 `CancelAll` 后用 `ro_request(OpenOrders)` 查一次挂单列表,响应经 `decode_rest`(`RestWhat::Sweep`)对 client id 可解析为本账户的每个订单产出 `OrderEvent{what: QueryResult}`。需要会话维护的所实现 `session_keepalive_request`。gw 的全部 REST 响应经 `decode_rest` 解码:槽与操作取自 `RestOrigin`(响应不带 client id 时不得猜测),输出与 `decode_report` 遵守同样的契约 8、9、10、13、28、30;死手与会话维护响应不产出订单事件,失败只体现在 `ReportOutcome.raw_code` 与 `ctl`。
30. **控制提示 `ReportOutcome.ctl`**:`decode_report`/`decode_rest` 遇时间戳越界拒单置 `ClockSkew`、遇交易所限速置 `RateLimited`(两者照常按码表产出 `Reject` 等事件)、遇用户流被交易所终止置 `UserStreamEnded`、遇私有连接上的下线或维护预告置 `ServerShutdown`;不产出 `ConnState`(由运行时按 [hft-venue §4](../hft-venue/README.md#4-gwruntimepoll_once-的循环结构) 第 2 步产出)。各 venue README 的"结果未知码表"与"用户流事件表"写明哪些码与事件置哪种提示。
31. **只读查询族**(reconciler 的对账输入,[reconciler](../../apps/reconciler/README.md#覆盖范围与数据来源)):`ro_request` 只生成 `GET` 类只读请求(签名用 `Templates` 的凭证;reconciler 以只读 key 构建,`redacted = true`);`ro_decode` 按 `RoKind` 经 `RoSink` 逐行产出——`OpenOrders` → `open_order`、`Fills` → `fill`、`FundingFlow` → `funding`(逐笔带 inst 与交易所原始结算时刻)、`Balance` → `balance`、`Positions` → `position`;只保留 refdata 中该账户可交易的 inst 与资产;分页以 `RoPage.next_cursor` 交回调用方。产品线没有的查询(现货的 `Positions`、`FundingFlow`)返回 `Unsupported`。不依赖 `GwCtx`/`AcctCtx`,同一输入的输出确定。每个 venue README 有"只读查询"表(按产品线:端点、分页与游标、字段映射、是否带 client id)。kill-switch 不使用本族:它零依赖、自写每所的查挂单与查 UID([kill-switch 依赖](../../apps/kill-switch/README.md#依赖)),与本族只共用 fixtures 中的原始响应。

## 每个 venue README 的必备节

`cargo xtask doc-check` 检查每个 `scope: core` 的 `venue-*` README 含以下全部小节(两种 MarketKind 都要填);`scope: optional` 的 `venue-*` 至少有"VenueCaps 默认值"一节(其余各节写明"接入时补齐"),开始实现即补全并通过同一检查:

| 小节 | 对应契约 |
|---|---|
| 分档快照流(频道、档位、节拍、序号与时间字段) | 26 |
| L2 序号归一化(增量深度,可选扩展;规格保留) | 1 |
| 累计量与费用来源 | 9、12、19、22 |
| 结果未知码表(确定拒绝 / 结果未知两张表的来源;本所码的形态;`GONE` 类与置控制提示的码) | 6、8、28、30 |
| VenueCaps 默认值(含 `fok` 与 `dms` 的 `DeadManCap` 取值) | 15 |
| 账户配置核验(含现货借币开关、每把 key 的 UID、永续杠杆) | 16、23 |
| 保证金数据换算 | 17 |
| 外部成交识别 | 18 |
| 用户流事件表(含资金费推送的 `settle_ts_ns` 与 `disc` 字段、置控制提示的事件) | 14、20、22、30 |
| 只读查询(端点、分页与游标、字段映射、是否带 client id) | 31 |
| 测试网/模拟盘(行情与下单端点、测试网符号映射、不覆盖的接口) | `VenueCaps.testnet` |
| 心跳与 BBO 探测(按产品线与连接种类分别写服务端 ping 间隔、无 pong 断开时限、我们的探测方式) | 24 |

## 实现说明

<details><summary>实现要点</summary>

- 本 crate 只有类型与 trait,默认项只有 `CANCEL_ALL_SWEEP`、`BBO_SNAPSHOT_BATCH`、`encode_batch`、`needs_per_order_signing`、`bbo_probe`、`bbo_snapshot_request`/`decode_bbo_snapshot`、`order_channel`、`encode_rest`、`session_keepalive_request`、`decode_rest`;`TradeIdRing` 与 `dedup_key` 是唯一带实现的辅助项(纯数据结构与纯函数,供各 venue 复用)。
- 所有热路径方法显式接收 `rd: &RefData`(refdata 在进程生命周期内不变,hft-core 以 `&'static RefData` 传入);`FeedCtx`/`GwCtx`/`AcctCtx` 不缓存 refdata 的任何指针,若将来做 refdata 热替换,接口不变。
- `SeqRule::Chain` 统一链续判定(增量深度,可选扩展):交易所特有的判定(Binance 合约 `pu`、现货 `U/u` 重叠、OKX `prevSeqId`、Bitget `pseq`)全部在 `peek` 里换算成 `prev_id/last_id`,运行时的断裂检测是一段通用代码;只有快照衔接仍需原始序号(`align_first(snap_id, &RawSeq)`)。core 的深度是分档快照(契约 26),不经这条路径。
- REST 请求与解码不在热线程:BBO 补帧与(启用增量深度时的)快照由 snap-fetch 用自己的 `FeedCtx` 调用(契约 25),feed 任务只调用 `align_first` 并把 snap-fetch 送回的分片推进环([hft-venue §3b](../hft-venue/README.md#3b-快照获取snap-fetch));账户查询由 acct-fetch 用自己的 `AcctCtx` 调用(契约 27,[hft-venue §4a](../hft-venue/README.md#4a-账户查询保证金轮询与配置核验))。
- 账户查询是"有序请求表 + 定长中间结果":`AcctScratch` 只保留 refdata 中该账户可交易 inst 的条目(容量 = hft-refdata 的每 venue inst 上限),其他符号在解码时跳过;因此响应体再大也不分配。
- `own_liquidation_marker` 在 `VenueCaps` 里是一个说明性的标签(供文档、模拟撮合与审计使用),判定逻辑只在 venue crate 的 `reports.rs` 里;两者不一致由该 venue 的金样测试发现。
- feature `testkit`:`VecSink`(按调用顺序记录每个 sink 方法的输入;`exch_id(key, Some(id))` 记录并返回 `Recorded`,`exch_id(key, None)` 命中已记录的 key 返回 `Resolved{epoch, slot, gen}`,否则 `Unknown`)、`GoldenDriver`(给 `P` 喂 raw payload 序列,逐字节比对期望事件体;支持在指定位置注入 `prev_override`;支持喂账户查询响应序列;编码类方法——`encode`、`encode_batch`、`render_cid`、`login`、`subscribe`、`private_subscribe`、`endpoint` 与产出 `RestReqBuf` 的方法(含 `encode_rest`、`session_keepalive_request`、`ro_request`)——按输入逐字节比对期望输出;支持喂 `decode_rest` 与 `ro_decode` 的响应;不一致时报告样本名、记录序号与第一个不同字节的偏移)、`FuzzHarness`(把任意字节依次喂 `classify → peek → decode_md`、`decode_snapshot`、`decode_bbo_snapshot`、`classify_report → decode_report`、`decode_rest`、`acct_decode`、`ro_decode`,断言无 panic、错误走返回值),以及每条契约的检查函数 `check_*`(失败返回 `Err(ContractFail{contract, sample, detail})`,`contract` 为契约编号;清单与各自的断言见 [TESTS.md](TESTS.md);每个 venue 的 `tests/contract.rs` 必须对每个产品线调用全部检查,只适用于增量深度的 `check_chain` 在该 venue 启用增量深度时调用;`check_account_config` 对 `required_spot_borrow = None` 的现货产品线接受以账户类型 → `ACCT_TYPE` 的样本代替借币开关样本,契约 16)。检查函数的样本由 venue 的 `tests/contract.rs` 提供;需要计数分配的检查以参数接收分配计数函数(全局分配器由测试二进制安装)。venue crate 的 `tests/` 与 `fuzz/` 只依赖这个 feature。

| 文件 | 职责 |
|---|---|
| `src/lib.rs` | `VenueProtocol` |
| `src/types.rs` | 会话、行情、下单、REST 出口、账户查询、只读查询、限速、心跳、死手的全部辅助类型 |
| `src/sink.rs` | `MdSink`、`ReportSink`、`RoSink`、`ExchIdHint` |
| `src/dedup.rs` | `TradeKey`、`dedup_key`、`TradeIdRing` |
| `src/code.rs` | 契约 8 的原始码编码函数(数值、数字字符串、非数字字符串、HTTP 状态) |
| `src/testkit.rs` | `VecSink`、`GoldenDriver`、`FuzzHarness`、契约检查(feature `testkit`) |

</details>

## 延迟与资源契约

本 crate 自身除 `TradeIdRing`、`dedup_key` 与码编码外没有运行时代码;trait 方法的预算由每个实现在其 README 给出(基准名 `bench_classify_<venue>`、`bench_decode_*`、`bench_encode_*`、`bench_acct_decode_*`),**每个 venue 的 p50 与 p99 都不得宽于下表统一门槛**(`doc-check` 比对 venue README 与 `benches/budgets.toml`):

| 入口 | p50 门槛 | p99 门槛 | 说明 |
|---|---|---|---|
| `classify` / `classify_report` | ≤ 30 ns | ≤ 60 ns | 只看前 64 B |
| BBO 类 `decode_md` | ≤ 0.4 µs | ≤ 0.8 µs | JSON bookTicker 类 ≈ 200–300 B;二进制编码的行由 venue 自定更紧的值 |
| 分档快照类 `decode_md`,20 档 | ≤ 1.2 µs | ≤ 2 µs | 契约 26 |
| `encode` | ≤ 0.3 µs | ≤ 0.6 µs | 逐单签名的所除外(单独基准) |
| 成交类 `decode_report`(含 `ExternalFill` 判定与去重) | ≤ 1.2 µs | ≤ 2.5 µs | — |
| `decode_report` 对 `Ignorable`/`Unknown` 事件 | ≤ 0.15 µs | ≤ 0.3 µs | 只取事件名并查名单 |
| `decode_report` 保证金推送(`margin_data = Push`) | 不超过 hft-venue `venue_gw_margin_push` 的 p50 | 不超过其 p99 | 在 gw 线程上;数值归 [hft-venue](../hft-venue/README.md#延迟与资源契约),各 venue 以本所最大推送样本做基准 |
| `decode_snapshot`,1000 档(≈ 50 KiB) | ≤ 80 µs | ≤ 100 µs | warm(snap-fetch),只在启用增量深度时;更大的快照按档数线性放宽,README 写明 |
| `acct_decode`,50 KiB 响应 | ≤ 60 µs | ≤ 100 µs | warm(acct-fetch),不在 gw 线程上(契约 27) |
| `TradeIdRing::seen_or_insert`,N = 1024 | ≤ 0.25 µs | ≤ 0.3 µs | 本 crate,`bench_trade_id_ring` |
| `dedup_key` | ≤ 15 ns | ≤ 20 ns | 本 crate,`bench_dedup_key` |

**与 [latency-budget §3](../../docs/latency-budget.md#3-cex-路径预算) `ingest` 分配的核对**(任一门槛或 hft-net 解帧预算调整时同一改动重做):BBO 路径上 hft-net 解帧(`net_rx_300b_mem`)、`classify` 与 BBO 类 `decode_md` 三项门槛之和,p50 ≤ `ingest`(bookTicker)目标 p50 的 1/3,p99 ≤ 其目标 p99 的 1/5;分档快照路径上 `net_rx_2k_mem`、`classify` 与 20 档 `decode_md` 之和,p50 ≤ `ingest`(深度)目标 p50 的 1/2,p99 ≤ 其目标 p99 的 1/3;其余留给入环、环中排队与 sequencer 取出。

端到端分段见 [hft-venue](../hft-venue/README.md#延迟与资源契约)。编译期断言:`RxMeta`(48 B)、`SeqIds`、`RawSeq`、`TargetRef`、`AcctQuery` 大小固定且 ≤ 64 B。

## 测试与验收

用例与验收见 [TESTS.md](TESTS.md)。

## 依赖

允许:`hft-types`(ID、`ClientOrderId`、定点、`MarketKind`)、`hft-refdata`(`RefData`、`SymbolIndex`、`VenueCaps`、`LimitScope`/`LimitBucket`、`DeadManCap`)、`hft-event`(事件体、出环元素 `GwCmd`/`GwOp`)、`xxhash-rust`(xxh3,`dedup_key` 与码编码)。**禁止**:任何 I/O、线程、`hft-rt`、`hft-net`、`hft-journal`、`hft-venue`、任何 `venue-*`、`serde_json`、`tokio`。

## 延伸阅读

- [hft-venue](../hft-venue/README.md)(运行时如何调用这些方法)、[cookbook/adding-a-venue.md](../../docs/cookbook/adding-a-venue.md)
- 实现(core):[venue-binance](../venue-binance/README.md)、[venue-okx](../venue-okx/README.md)、[venue-bitget](../venue-bitget/README.md);(optional):[venue-gate](../venue-gate/README.md)、[venue-bybit](../venue-bybit/README.md)、[venue-kucoin](../venue-kucoin/README.md)、[venue-hyperliquid](../venue-hyperliquid/README.md)
- [hft-refdata](../hft-refdata/README.md)(`VenueCaps`、`AccountMeta`)、[hft-event](../hft-event/README.md)(`ExternalFill`、`MarginReportHdr`、`AccountConfigReportHdr`)、[hft-risk](../hft-risk/README.md)(启动闸门 `ACCOUNT_CFG`/`IDENTITY`、`MarginStress`)
- [13 O2](../../design/13-设计评审与优化.md)、[04 §2](../../design/04-行情接入层.md)、[05 §4](../../design/05-CEX执行层.md)、[research/02](../../design/research/02-exchange-connectivity.md)
- 范围与取舍:[round8-decisions](../../design/_work/round8-decisions.md);Agent Note:`venue-protocol-and-runtime-split`、`venue-caps`、`feed-split-and-snap-fetch`、`core-market-data-scope`

## 已知限制与后续工作

- **帧单位是 WS 消息**:FIX(Binance Spot、Bybit spot)需要非 WS 分帧,可选扩展:接入 FIX 时扩展 `MsgOp` 与 hft-net。
- **Hyperliquid 自建节点**输出本地文件而非 WS,不适合本 trait 的帧模型;需要时另开 Agent Note 设计文件尾随变体。
- **账户查询只走 REST 池**:部分所的 WS API 也提供账户查询(Binance ws-fapi 的账户方法,待核实,见 venue-binance 开发备忘第 10 条);改走 WS 只换 `acct_request` 的输出形式,接口不变,触发条件:REST 权重成为瓶颈。
- **杠杆只观测、不核验**:实际杠杆由运营在交易所设置([runbooks/launch](../../docs/runbooks/launch.md)),本层只读出并报告(契约 16),不与任何要求比对;上限检查在 hft-risk。
- **契约 22 的资金费键依赖 `disc`**:交易所对同一结算时刻的多笔推送若没有可区分的字段,`disc` 取不到,只能按 `(settle_ts_ns)` 合并去重,标待核实并在该 venue 的开发备忘写出核实方法;这种所的资金费以 reconciler 的流水校正为准。
- trait 在接入新交易所时允许修订(关联类型与 `const` 表达差异);修订须同一改动更新所有实现与本 README。

### 开发备忘

<details><summary>给实现者</summary>

- 实现顺序:`types.rs` → trait → `dedup.rs` → `code.rs` → `testkit`(先用假协议跑通全部契约检查)→ 交给 venue-binance。
- 开放问题:`MdSink` 用方法集还是单个 `push(ev: MdEvent<'_>)`——以 hft-venue 的写环代码生成是否更短为准,基准无差异时保留方法集。
- `MarginReport` 与 `AccountConfigReport` 的线上分条规则由 hft-event 决定;`ReportSink` 以"头 + 条目切片"交出,运行时负责按 hft-event 的规则切分写环,venue 不感知。
- 契约 8 的非数字字符串码哈希段 `0x4000_0000..0x40FF_FFFF` 与真实数值码不重叠的前提:core 交易所的数值码绝对值 < 2²⁹(Binance、OKX、Bitget 已知码均在 10⁶ 以内);接入新所时核对。

</details>
