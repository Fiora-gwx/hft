# 05 · CEX 执行层(OMS · 风控 · 网关)

> **实现以 [`hft/`](../README.md) 规格为准,本章是设计证据;冲突处以 hft 为准。** 订单槽与 LegLink 归 [hft-oms](../crates/hft-oms/README.md),执行算法与引擎单归 [hft-exec](../crates/hft-exec/README.md),检查链归 [hft-risk](../crates/hft-risk/README.md),gw 循环归 [hft-venue](../crates/hft-venue/README.md),client id 编码归 [hft-types](../crates/hft-types/README.md);本章的结构体是原始设计草图,只说明思路。
>
> 执行层把策略意图变成交易所能接受的帧,并把回报变成状态。它分三段:sequencer 内的 **执行层 + 风控 + OMS**(单写者,µs 级;执行层把目标意图切成订单)、gw 线程内的 **网关**(TLS + 发送 + 回报解码)、冷进程 **reconciler**(REST 对账)。模拟账户的订单走同一条路,只是出环进 sim-gw([07 §9](07-策略层.md))。各交易所的通道选择见 §7 与 [research/02](research/02-exchange-connectivity.md)。

## 1. 目标

| 指标 | 现状(Python / cex-arb) | 目标 |
|---|---|---|
| 决策写出 → 帧写出(`egress`) | 50 ms 级(Python 下单函数) | 基准形态 p50 ≤ 5 µs、p99 ≤ 15 µs(唯一归属 [latency-budget §3](../docs/latency-budget.md);原稿"意图 → 帧写出 10/30 µs"已被取代);可选:裸金属 + Onload ≤ 3/10 µs |
| 双腿间隔(第一腿成交取出 → 第二腿发出,`leg_gap`) | 数百 ms(Python 回调 + REST) | 同一事件内出环,主机内 ≤ 20 µs |
| 回报 → 由它触发的下一单(`rep_to_order`,覆盖对冲路径) | — | 目标 p50 ≤ 25 µs、p99 ≤ 80 µs;门槛 ⑩ 的硬线由它承担([00 §4.2](00-目标与延迟预算.md)) |
| 对冲主机内(第一腿成交帧 HW RX → 第二腿写出,`hedge_host`) | — | 目标 p50 ≤ 25 µs、p99 ≤ 80 µs(`rep_to_order` 的子集,只记录;含回报在 gw 与回报环里的等待) |
| 撤单/改单 | REST 撤单 | WS `cancel`/`cancelReplace`/`amend`,预编码模板 |
| 重连 | TLS 握手 ~30 ms,期间无连接 | 常备预热连接,切换 < 100 µs;主连接 24 h 到期前主动轮换 |
| 幂等 | 重试可能重复下单(JCT 事件) | 每意图唯一 `client_order_id`,重试复用同 id;交易所去重 |
| 状态一致 | 多进程各持仓位副本 | 仓位/挂单只在 sequencer;REST 对账仅产生报告 |

## 2. 订单模型

### 2.1 订单槽(预分配、索引直查)

原始设计草图;真实布局归 [hft-oms](../crates/hft-oms/README.md),`client_order_id` 的位布局归 [hft-types](../crates/hft-types/README.md)(下文的 `{engine_epoch, venue, slot_id, gen}` 只是思路)。

```rust
#[repr(C, align(64))]
struct OrderSlot {
    slot_id: u32,             // 数组下标,也是 client_order_id 的一部分
    venue: u8, account: u8, inst_id: u32, strategy: u8,   // account:测试/正式等账户是一等维度
    side: Side, tif: Tif, flags: u8,   // reduce_only / post_only / linked
    px: Px, qty: Qty, filled: Qty, avg_px: Px,
    state: OrderState,        // 见 2.2
    exch_order_id: u64,       // 交易所 id(数值化;字符串 id 用哈希表映射一次)
    link: LegLink,            // 多腿绑定(见 2.3)
    t_intent: u64, t_sent: u64, t_ack: u64, t_last: u64,   // TSC
    seq_created: u64, seq_last: u64,
}
```

- 每 (venue, account) 一个 `[OrderSlot; N]`(N 按 24 h 最大在途 + 余量,例如 4096),空闲槽用环形空闲表;**没有 HashMap 在热路径**;交易所返回的字符串 id → slot 的映射在 venue 线程解码回报时用一个预分配开放寻址表完成,把 slot_id 写进事件。
- `client_order_id` 编码 `{engine_epoch, venue, slot_id, gen}`,固定长度、字母数字,满足各所限制(Binance 36 字符、OKX 32、Bybit 36、Bitget 40、Gate 28 前缀 `t-`)。`gen` 递增,槽复用不撞 id。

### 2.2 状态机

```
        place                ack                      fill(部分/全部)
 Free ──────▶ PendingNew ─────────▶ Live ─────────────────────────▶ Filled
                 │                  │  cancel        cancel_ack        ▲
                 │ reject           ├──────▶ PendingCancel ────────▶ Cancelled
                 ▼                  │                    │  fill 先于 cancel_ack(必须处理)
              Rejected              │  amend             └──────▶ Filled / PartiallyFilled→Cancelled
                                    └──────▶ PendingAmend ──ack──▶ Live'
 任何状态 ──(连接断且未知)──▶ Unknown ──(REST/WS 查询或对账)──▶ 真实状态
```

- **回报先于回执**(fill before ack)、**撤单被拒因为已成交**、**重复/乱序回报**是属性测试的必测路径(04 调研清单)。
- `Unknown` 是显式状态,不是错误:连接断开时所有 `PendingNew/PendingCancel` 进入 `Unknown`,重连后先用 WS 查询/REST `openOrders` 收敛,再放行新意图——与链上 UNKNOWN 同一原则(P7)。
- 每次状态迁移都是一条 `OrderEvent` 进 journal;`Position` 只由 `Fill` 事件更新。

### 2.3 多腿绑定(LegLink)

原始设计草图;真实定义归 [hft-oms](../crates/hft-oms/README.md)。

```rust
struct LegLink { group: u32, role: Leg::First|Second, hedge: HedgePlan, tolerance_bp: u16 }
enum HedgePlan { None, OnFill{venue, inst, side, tif: Ioc, px_mode: Cross|Mid|Limit(bp)}, Simultaneous }
```

- `OnFill`:第一腿每一笔 `Fill`(含部分成交)触发第二腿按**实际成交净量**出 IOC(现货手续费从基础资产扣时,净量 = 成交量 − 手续费;规则来自 `VenueCaps.fee_asset_rule`)——数量对齐由 OMS 负责(合约乘数、最小步长、余数按链接累计到下一笔),策略不介入。**两腿 lot 对齐**([M0b 独立评审 P1-2](_work/review-2026-10-03-M0b独立评审.md)):切片与第一腿数量按两腿 lot 的较粗者取整(一方整除另一方时),保证第二腿量 ≥ 其 `min_qty` 且为其 lot 的整数倍;不能整除时取最小公倍数,其名义 > `max_chunk_notional` → `SetParams` 拒绝(`InvalidArgs{LotMismatch}`)。例:现货 0.004 BTC 一片对 OKX 永续一张 0.01 BTC,旧规则每片留下不可对冲的残余。第二腿数量 = min(按成交净量算出的量, LegTracker 当前该方向的裸量),避免同一笔成交被对冲两次。
- 对冲腿定价:报价有效性按 BBO 连接健康判定(§3 末),盘口长时间不变不算过期;对手价偏离超过 `max_slip` 时按 `rr_max_cross` 上限定价照样发 IOC;只有超过 `rr_max_cross` 才返回门控拒绝(不计执行失败次数)——退化行情下不再 3 s 内必然 Broken。
- **引擎单与对冲单共用一个限价函数**(2026-10-03 五路评审 V1 P0-2):hft-types 的纯函数 `cap_cross_px(side, touch, mid, max_cross, lim)` = 买 min(对手价 × (1 + max_cross), mid × (1 + `rr_max_cross`)),卖对称,按 side 取整到 tick;OMS 的对冲定价与执行层全部引擎单(补腿、自动平仓、L3 减仓、flatten)都用它,风控 #6 的名义按 `数量 × 限价` 计。否则价差扩大时引擎减仓单每次被价格带拒、永远发不出去。参数校验 `force_slice.max_cross ≤ rr_max_cross`、`force_slice.max_chunk_notional ≤ max_order_notional`(启动不符退出 2);引擎单仍被拒按执行失败处理并报 P0。
- **挂着的单不算对冲覆盖**:允许在带 LegLink 的腿上挂 GTC/PostOnly,但 OMS 把在途量拆成 `inflight_immediate`(IOC/FOK/Pending*/Unknown)与 `inflight_resting`(已 Ack 的挂单),裸量与补腿抑制只用前者——否则挂着的单会让现货裸多无界(V1 P1-9)。
- `Simultaneous`:两腿同一事件内出环,第二腿失败则 OMS 自动对第一腿发反向 IOC(可配置)——在模拟账户与正式实盘的数据中证明有利时才用。
- 链上第一腿(optional):`TxLifecycle::IncludedOk{amount_out}` 与 `Fill` 等价处理。
- 第二腿重试仍是第二腿:继承该链接的 reduceOnly 语义(开仓链接的重试不是 reduceOnly),重试有计次、冷却与截止,到期 → `Broken`。
- `LegTracker` 监控每 group 两腿差(在途算进裸敞口);超阈值时执行层发补腿单,补腿只用于"平掉多出的一腿",与重试共用计次、冷却与截止,HARD 下抑制。阈值、残余与尘埃规则全部是参数,不写死在 OMS。
- **对冲量有上限**:对冲/补腿量 ≤ 触发成交净量 × 比例 + 1 lot,名义 ≤ `max_hedge_notional`;超限 → `Broken` + P0,不发单(防止解码或单位错误把一笔成交放大成千倍的对冲单)。
- **Broken 不无限期挂着**:Broken 持续 `broken_auto_fix_ms`(默认 120 000)且期间无人工 `HedgeFix`,执行层自动做一次有界的"平掉多出的一腿"(永续腿 reduce-only、现货腿卖 min(库存, 可用余额),按 `max_reduce_notional` 切片,总量 ≤ 裸量);成功 P1、`HedgeFailure` cap 仍需人工 `Resume`(且该账户已无 Broken cell 才能解除),失败 P0 保持 Broken;HARD 下不做。"一次"指**一次可执行的尝试**(R8-11,汇总 M-5):开始前等待(不算失败)多出那腿的报价可用、该 venue 不在限速退避、对手价未越过上限;开始后门控拒绝与零成交按重试处理;只有交易所拒单、MinOrder、SignFlip、残余、可执行时长超过截止才判失败——否则报价暂不可用、限速或零成交就会把这唯一的一次耗掉。残余低于最小下单量造成的 HedgeFailure 死锁:装配期校验 `dust_notional ≥ 交易腿最小名义的最大值`(不符退出 2,`DustBelowMinNotional`),另有 Admin 升级命令 `WriteOffResidual{account, group}`:对**静止的** group(Broken 或非 Broken)把多出那一腿的已成交裸量登记为核销(ledger 记 `AdjustDelta::WriteOff`),并清零该 group 全部腿的残余,Broken 随之可解除(R8-12)。恢复期(启动后)自动平仓、补腿、残余修剪与外来成交默认处理的开始条件是"多出那一腿所在 venue 的对冲闸门子集满足且该 venue 已对账",不满足期间不计时;账户闸门未放行而裸量持续超限 → P0(`ExecGateHeldNaked`);`resolve_broken` 只在 HARD Kill 解除时产生(R8-12,规则归 hft-exec 与 hft-sequencer)。保证金 L3 与 `Kill{flatten}` 也覆盖 Broken 的 group(先永续腿)。冷启动时 `hedge_deadline_ms` 只按引擎运行时间累计,闸门放行时先对未对冲链接做一次 `SecondRetry`;续接对冲只等对冲腿 venue 的闸门子集(身份、账户配置、下单连接)且该链接无 `Unknown`,超过 `resume_hold_max_ms`(30 000)→ Broken + P0。设计理由见 [09 §4.8](09-可观测性、风控与控制面.md)。
- **L3/flatten 不依赖另一腿报价**(2026-10-03 五路评审 V1 P0-1,R8-10):`reason ∈ {MarginStress, Hard}`(含 `Kill{flatten}`)时,非 Broken cell 只要求永续腿报价可用;第二腿不满足"`quote_ok ∧ venue_ready ∧ 下单连接 Up`"(R8-11 补上下单连接)或在途链接处于 RESUME_HOLD 时,退回 Broken 同款的串行路径 `SerialFallback`(永续 reduce-only 先减,现货腿在其报价恢复后再减),写 P0,接受短时现货裸多。L3 与 flatten 同样作用于该账户不属于任何 group 的永续持仓。否则另一所行情一断,永续减仓就完全停摆。DST 必过用例:"L3 + 另一所 BBO 断 30 s → 永续仍按步减"。
- **引擎目标先撤策略挂单**(R8-11,汇总 M-2):设 `reason ∈ {MarginStress, Hard}` 的引擎目标时,先经 OMS 入口 `cancel_resting_on(a, inst, out)` 撤该 inst 上本账户全部非 EXEC 挂单;撤单确认前切片量按 `|pos| − 即时在途` 截取,风控 #7 对 Flatten/Hedge 类只扣即时在途。否则策略挂着的 reduce-only 单会让风控拒掉引擎的减仓单,L3/flatten/补腿卡死(首个策略不挂单,暂不触发)。DST:"L3 时策略挂满 reduce-only 平仓单,仍按步减"。L3 的越价上限按 **inst 所属价格带类别**取值(SDK `Views::risk_for(a, i)` 提供按 inst 的 `rr_max_cross`,不再取账户内最小值);另设可选的 `RiskParams.l3_max_cross`(默认 0 = 不启用,上线签字时决定)。
- **清仓锁**:`Kill{Account, flatten}` 触发的 `engine_flatten` 置账户级 `flatten_lock`,直到 `Resume{Kill}` 前该账户只接受比当前更接近 0 的目标与 Reduce/Flatten 意图,策略的新目标不能把清仓覆盖掉([M0b 独立评审 P1-1](_work/review-2026-10-03-M0b独立评审.md))。

### 2.4 目标意图与执行层

策略也可以只说目标仓位(`TargetIntent`:各腿目标净量、原因、紧急度、不交易带、执行算法),由 sequencer 内的执行层 `hft-exec` 把差额切成订单(吃单切片、两腿 OnFill、Simultaneous;先挂单后对冲预留),再交给风控与 OMS。OMS 只管订单槽状态机与 `LegLink` 机制,"下什么单"由执行层决定;保证金驱动的退出强制先减永续。设计理由见 [07 §4–§5](07-策略层.md),规格见 [hft-exec](../crates/hft-exec/README.md)。

## 3. 风控(sequencer 内,每意图 ~200–500 ns)

按廉价→昂贵顺序,任一失败即拒并记 `RiskReject{reason_bits}`(检查链的顺序、编号与 cap 的唯一归属是 [hft-risk](../crates/hft-risk/README.md)):

1. kill 字(全局 / 账户 / venue,另有 HARD 位,可按账户 HARD);只有 HARD 能拦截带 `RISK_REDUCING` 的单(对冲腿、平仓)。模式与降级的统一模型见 `hft/docs/subsystems/control-plane.md`(期望模式与 cap 分离)。
2. 模式与 cap:有效模式 `CloseOnly` 下 Open 类拒;时钟不可信、`Halted` 等 cap 只拦 Open,硬动作与 Flatten 绕过冷却与在途门。
3. **带 `LegLink` 的开仓同时检查两腿所在 venue**:两边的 kill/cap、连接就绪与 Unknown 门、令牌桶余量都要满足,另外第二腿 venue 的报价按连接健康判定有效(本节末;否则 `LINK_STALE`),且第二腿资金可负担——现货买入腿要求可用计价资产 ≥ `link_notional × (1 + funds_buffer)`,永续腿要求可用保证金 ≥ `link_notional / 杠杆 × (1 + funds_buffer)`,杠杆取 `AccountConfigReport` 读到的该账户实际杠杆(不再用常量 `perp_leverage`;`funds_buffer` 默认 0.05;否则 `LINK_FUNDS`)。只查第一腿时,对冲所断连、报价过期或钱不够都会放行开仓,第二腿必然失败;同账户任一 group `Broken` → 账户级 `HedgeFailure` cap 挡该账户全部开仓。
4. 价格带:相对该 inst 最新 mid 偏离 ≤ X bp,报价按连接健康判定有效(风险递减单只查越价上限)。
5. 单笔名义、单币名义、总名义、单 venue 名义(含在途;名义 = 数量 × 限价)。
6. 仓位方向与上限(含在途未成交量,防重复开仓)。
7. 速率:每 venue、每 inst 令牌桶,与交易所限速对齐并留 20% 余量,为风险递减单预留份额(`rr_reserve`);`RateLimitHit` 事件触发退避。
8. 当日亏损熔断:已实现 + 未实现 ≤ −L → 该账户锁存 `CloseOnly` 并告警。
9. 敞口门:两腿差超阈值时禁止再开同 group。
10. 账户保证金分档(`MarginStress`,按到强平价的距离 `liq_dist` 分档):L1 禁开 → L2 通知策略限期减仓 → L3 执行层按比例减永续敞口,Broken 的 group 一并减([07 §6](07-策略层.md))。
11. 对账偏差:`ReconcileReport` 按名义分级 → `Recon` cap(只平不开)。

价格带、仓位、速率三项对**全部**意图类别生效——对冲、补腿、平仓不只做结构检查。

**报价新鲜度 = BBO 连接健康**([M0b 独立评审 P0-1](_work/review-2026-10-03-M0b独立评审.md)):旧规格按"该 inst 最后一次变化"判过期(对冲定价、#3 的 `hedge_quote_max_age_ms` 500、执行层 `quote_max_age_ms` 1 000),第二腿盘口 0.5 s 不变就 `StaleQuote` → 3 s 后 Broken → 自动平仓每片也要求报价 ≤ 1 s → 失败,安静行情下必然卡死。现在 SDK `BboView` 带 `conn_live_tsc`(该 BBO 连接最近一次收到任何帧——数据、pong、心跳——的时刻)与 `resync`;hft-venue 对 BBO 连接每 `bbo_ping_ms`(1 000)发 WS ping 保证有回帧;判定 = 连接 Live、非重同步、`now − conn_live_tsc ≤ conn_liveness_ms`(2 000),报价取该连接上的最新值(无变化即有效)。对冲腿、风险递减单、自动平仓、L3 减仓、#3 的第二腿检查一律用此判定;`max_mid_age_ms` 退役。连接健康按该 inst 的 BBO 连接**成员**聚合(任一成员 Up 即 up,`conn_live_tsc` 取 Up 成员的最大值),赛马胜出的连接退役不会让安静 inst 失效([04 §2](04-行情接入层.md))。DST 必过用例:"第二腿盘口 10 s 不变"时开仓、对冲与自动平仓照常。规格唯一归属 [hft-risk](../crates/hft-risk/README.md)(#5)与 [hft-venue](../crates/hft-venue/README.md)。

所有限额是 `RiskParams` 版本化结构体,经控制环替换。

## 4. 网关(gw 线程)

### 4.1 连接管理

- 每 venue:**1 条主下单连接 + 1 条预热备用**(已完成 TLS + 登录/会话,定时心跳),+ 1–2 条行情连接。
- 主连接故障或临近 24 h 上限:OMS 在 `Free` 与 `Live` 间无 `Pending*` 的时机切到备用(切换只改一个指针),同时后台新建备用。
- TLS 1.3 会话票据复用;`TCP_NODELAY`;显式 `SO_BUSY_POLL = 0`(收包中断驱动,IRQ 在独占的 IRQ 核,[03 §3.1](03-网络与传输层.md));`SO_TIMESTAMPING`(RX 硬件 + 软件戳;TX 只有软件戳且默认关——ENA 没有 TX 硬件戳);写用 `writev` 一次写出整帧,不分片。原稿的"`SO_BUSY_POLL`、TX/RX 软件 + 硬件时间戳"已被 hft 取代。
- 出口 IP:多 ENI/EIP 拉宽按 IP 的连接/消息限速(见 research/02);每条连接固定出口 IP。

### 4.2 帧模板

- 每 (venue, 消息类型) 一个**预编码模板**:固定部分(方法名、账户参数、常量字段)启动期序列化好;运行时只写入价格、数量、`client_order_id`、时间戳、签名(若需要)四类字段;整帧长度提前算好,WS 掩码用 SIMD。
- 认证:优先会话级登录(Binance `session.logon` Ed25519 之后每单不签名;OKX/Bybit/Bitget/Gate 的 WS 登录一次),避免每单 HMAC/Ed25519(Ed25519 ≈ 40 µs/次,HMAC ≈ 1–2 µs)。
- 可选(默认关;触发条件:升级主机直方图显示出环冷热差):gw 线程每 tick 空跑"填模板 → 掩码"(cache warming),加密路径用 scratch 密钥上下文空跑。真触发时只剩"对真实连接加密一次(≈ 0.1–0.2 µs)+ 一次 `write`"。**不对真实连接预加密**:TLS 1.3 记录序号即 nonce,加密了就必须发([13 R1](13-设计评审与优化.md))。
- 下单连接在独占的 `gw-<venue>` 线程上,不与行情解析共线程([13 R3](13-设计评审与优化.md))。

### 4.3 回报路径

- 回报帧 → `sonic-rs` 专用结构体 → 从 `client_order_id` 直接解出 slot(编码见 §2.1;交易所只回 `exch_id` 的消息查 gw 线程私有的开放寻址表)→ 写 `OrderEvent{slot, kind, filled, px, exch_ts, rx_tsc}` 入环;gw 线程**不改任何订单状态**。用户流连接**每圈**都 poll(Binance USDⓈ-M 的成交只从用户流来;原设计每 8 圈一次);每解出一帧回报就把出环排空一次(`drain_out`),对冲单不排在整批回报之后——这两段等待计入 `rep_to_order` 与 `hedge_host`(§1)。保证金与账户配置的 REST 响应(最大约 50 KiB,解码 ≤ 120 µs)不在 gw 线程上解码,交给 warm 任务 `acct-fetch`,结果经 `acct-<venue>` 环交回 gw(hft-venue 内部字节环),gw 原样推入 `rep-<venue>`(R8-11,W5 P2-2)。
- 用户数据流同样归一化为事件。仓位以 `Fill` 溯源为准,交易所推送的仓位/余额只做校验;但三类推送**驱动状态**:
  - 自身被强平、ADL、人工网页成交、交割(client id 不是我们的格式)→ `ExternalFill`:入账、更新 LegTracker、报 P0、通知策略;强平/ADL/交割在截止时间内策略没处理 → 执行层默认平掉多出的一腿;**人工网页成交(`ExternalFill{Manual}`)不做引擎默认处理**,由策略按自己的规则处理(人在网页上操作时引擎不和人抢,R8-10)。R8-11 起它还让该 cell 置 **`manual_hold`**:置位期间连补腿(HedgeFix)、ResidualTrim、外来成交默认处理与自动有界平仓的计时也一并抑制(不计次、不计截止),写 `Note{ExecManualHold}`(P1);只有策略对该 group 的非清除类动作(`set_target` 或 `place`,`clear_target` 不算)、人工 `HedgeFix` 或引擎目标(L3/flatten/Hard)才解除。原规则只停了"外来成交默认处理",自动补腿仍会立刻把另一腿平掉,等于绕过了这条决定(汇总 M-1)。不能只记一条"外来单"Note——那样仓位不更新,LegTracker 以为仍对冲,另一腿裸露。暂时无法归属的成交(没有 client id、还对不上槽)由 gw 暂存 `gw.unmatched_hold_ms`(默认 5 000)仍对不上才发 `ExternalFill{Unknown}`。
  - 强平价 `liq_px`、保证金率、ADL 档 → `MarginReport`(按 `VenueCaps.margin_data` 推送或轮询),驱动保证金分档(按到强平距离,与 ledger 的估算取更保守者)。
  - 账户配置(持仓模式、保证金模式、**实际杠杆**、现货账户的借币开关等)→ `AccountConfigReport`;与 `VenueCaps` 要求不符 → 启动闸门不过 / 运行中设 cap。现货账户必须关闭借币(OKX `enableSpotBorrow = false`、Bitget 现货杠杆关闭),纳入 `ACCOUNT_CFG` 核验。
  - 成交去重键 = `(inst, trade_id)`(Binance/OKX 的成交 id 按交易对编号,跨品种可能撞号);0 或负的成交 id(强平/ADL)不入去重环,改用 `(inst, exch_order_id, fill_seq)`。
- 资金费按 `VenueCaps.funding_settle` 入账:推送带币种的直接入账;只推余额的所由 sequencer 按"结算时刻持仓 × mark × 费率"合成每币资金费;与交易所逐笔资金费流水的差额由 ledger 自动校正(`AdjustDelta::Funding`),只告警、不进启动闸门(§6)。用户流中已知可忽略的事件类型列在 `VenueCaps.ignorable_user_events`,未知类型只计数,不当解码失败(否则会引发查询风暴并断流)。

## 5. 定时器与维护(sequencer 事件)

| 定时器 | 周期 | 动作 |
|---|---|---|
| `Heartbeat` | 每所要求(10–30 s) | feed/gw 线程各自对自己的连接发 ping,不经 sequencer |
| `StaleOrderSweep` | 1 s | `PendingNew` 超 T 未回执 → 查询;`Live` GTC 超龄 → 策略决定 |
| `DeadMan` | 5–30 s | 刷新交易所死手开关(Binance 合约 `countdownCancelAll`、OKX/Bybit 死手接口):进程死亡 → 交易所自动撤单;countdown 超时 ≥ 2 × 刷新周期 |
| `Reconcile` | 30–60 s | 触发冷进程对账(命令出环) |
| `ConnRotate` | 交易所连接寿命上限(如 Binance 24 h)前 `conn.rotate_margin_s`(默认 1 h);唯一归属 hft-venue(ECF1 段 6) | 主备切换(原稿"每 12–20 h"已被取代) |

## 6. 冷进程 reconciler

- REST 拉各 core venue 的现货与永续:挂单、成交历史、仓位、余额(现货库存)、保证金数据;覆盖范围由 `VenueCaps` 决定;与 journal 重建的引擎视图对比;差异写 `ReconcileReport` 事件(经控制环)。
- 分级前先用同组报告里的 Funding 条目修正结算资产的余额差(资金费差额不再经"余额"条目混进 `Recon` cap 与启动闸门)。
- 差异分级:数量差 < 最小步长 → 忽略;< 名义阈值 → 告警;≥ 阈值(两轮确认)→ `Recon` cap(只平不开)+ 人工。非策略资产按 `SetBalanceBaseline` 登记的基线排除;人工网页操作后用 `AdoptExchange` 入账(可采纳余额条目)。
- 资金费条目单独处理:带交易所逐笔资金费(账户、venue、inst、结算时刻、金额),ledger 对结算时刻早于 `funding_recon_lag_ms`(默认 600 000)的差额自动入账(`Note{FundingCorrected}`,比对窗口 48 h);差额 > `funding_diff_alert`(默认 1 USDT 或 1%)→ P1。不进闸门、不触发 `Recon` cap([09 §4.3](09-可观测性、风控与控制面.md))。ledger 侧查不到历史(`NoHistory`)的条目在窗口结束后按交易所金额入账并写 `Note{FundingCorrected}`(P1),不再要求人工。
- 启动闸门:本轮差异(资金费条目除外)必须全部可忽略,或逐项 `AdoptExchange` / `SetBalanceBaseline` 处置才放行;出现日志尾部丢失或恢复不一致时禁用"干净重启后自动解除 Recovery"。
- 它也是 `Unknown` 订单的收敛来源之一。
- **用户流缺口里漏掉的 ADL**(R8-11):第二轮对账仍见到"永续同号缩小、无在途、无 Unknown"的差异,且两轮都没有用户流断开、该 (account, venue) 用户流自最近一次恢复起已稳定 ≥ 1 个对账周期 → 该条目带标志 `AUTO_EXTERNAL`,sequencer 在这条 `ReconcileReport` Input 内确定地合成成交并 P0(不另写记录,合成函数在 hft-ledger,冷读者同样调用);hft-ledger 按 (account, inst, 方向) 记 `auto_held`,之后补推的同向强平/ADL 先抵扣、只入账超出部分(R8-12);现货差异与反向差异仍走人工。

## 7. 各交易所通道

> 数字与端点以 [research/02](research/02-exchange-connectivity.md) 为准;此处只列设计决定。
>
> 首批交易所(core)是 **Binance、OKX、Bitget**,各接现货 + USDT 本位永续(一个账户在同一交易所只用其中一个市场,不做同所现货 × 永续,[07 §1](07-策略层.md));Binance 现货 FIX、Gate、Bybit、KuCoin、Hyperliquid 是可选扩展,触发条件:该所有经数据证明的策略需要。交易所之间的行为差异(手续费扣哪种资产、是否支持 reduceOnly、要求的持仓与保证金模式、资金费推送方式、保证金数据来源、死手能力)写进 `VenueCaps`,唯一归属 [hft-refdata](../crates/hft-refdata/README.md)。

| 交易所 | 下单通道 | 认证 / 每单签名 | 撤/改/批量 | 限速(默认,待核实数值) | 陈旧订单保护 | 备注 |
|---|---|---|---|---|---|---|
| Binance USDT-M | `ws-fapi` WS API `order.place/modify/cancel`(实测 az2 → `time` RTT p50 2.25 ms) | Ed25519 `session.logon` → 会话内免签(官方确认;现 cex-arb 已用) | `order.modify` 改价量;批量 | ORDERS 300/10 s、1200/min /账户;2400 weight/min/IP;10 入向消息/s;24 h 强断 | 收紧 `recvWindow` 1–2 s | 响应含 `rateLimits` 实时余量 → 本地令牌桶闭环;`countdownCancelAll` 死手;FAPI-MM/FSTREAM-MM 只对 Futures Liquidity Provider 计划成员开放(VIP Portal IP 白名单);REST 用直连的 `api1–api4.binance.com`,**不用 CloudFront 前置的 `api.binance.com`** |
| Binance Spot | `ws-api` WS API(可选:FIX OE `fix-oe.binance.com`) | 同上 | `order.cancelReplace` 原子;`order.amend.keepPriority`(减量保队列) | ORDERS 100/10 s;WS 300 连接/5 min/IP;5 入向消息/s | 同上 | **无死手接口** → kill-switch 进程 |
| OKX | WS `order` / `batch-orders`(≤ 20)/ `amend-order`(在途 ≤ 3)/ `mass-cancel`;**须用整数 `instIdCode`** | HMAC-SHA256,`login` 后会话内免签 | amend、批量 | 60 req/2 s/UID+instId;子账户 1000 单/2 s(VIP5 fill-ratio 放宽到 10 000) | **`expTime`**(到撮合超时即拒) | 响应含 `inTime`/`outTime`(µs)→ 直接测交易所内部延迟;`cancel-all-after` 死手;全部 Cloudflare(REST ≈ 14 ms vs Binance 3 ms,所以只走 WS);**无 colo/专线可申请**,目标 VIP4(解锁 L2 逐笔/SBE) |
| Bybit(optional) | WS Trade `/v5/trade` `order.create/amend/cancel` + 批量(20/10/5) | HMAC-SHA256 或 RSA,登录后免签 | amend、批量 | 10/s(spot 20/s)/UID,PRO 档至 1200/s | `recv_window` | `cts` 撮合时间戳;DCP 仅机构账户;MMWS/MMGW/SBE 下单(µs ack)存在但官方"不保证更低绝对延迟";11-12 迁 apne1-az4;500 连接/5 min/IP |
| Bitget | **UTA V3 WS** place / batch / modify / cancel 频道(每频道限速、批量上限待核实);REST v2 备用 | HMAC-SHA256 + passphrase,登录后免签 | modify、批量 | 10/s → VIP 100/s;MM 600/s | (待核实) | **无死手接口** → kill-switch 进程;SBE 与 "LOLA" 未在官方资料中找到(待核实);机构 MM/PRO 限速可申请 |
| Gate(optional) | WS API v4 `spot.order_place/amend/cancel`、`futures.order_place/batch_place/amend/cancel`(`action_mode=ACK` 最快回执) | **HMAC-SHA512**,`login` 后免签 | amend、批量 | spot 10 r/s;futures 100 r/s /UID | **`x-gate-exptime`** | 响应含 `x_in_time/x_out_time`(µs);LB 覆盖三 AZ,现测 minRTT 0.27 ms;MM 申请 mm@gate.com;**协议最完整,作为延迟基准腿** |
| KuCoin(optional) | **Pro WS 下单 `wsapi.kucoin.com`**(Colo 用户有 ns 时间戳);REST `hf/orders` 备用 | HMAC-SHA256 + 加密 passphrase | — | VIP 分级 30 s 权重池 | (待核实) | WS CloudFront;MM 计划明示 **DMA + co-location**(≥ 30M USDT/30 d)——达标后是真正的直连 |
| Hyperliquid(optional) | `/exchange` 或 WS `post`;自建非验证节点更早拿 fills | EIP-712 secp256k1 每单签名(≈ 50–150 µs,API wallet 免主钱包) | cancel/modify/batch;ALO(post-only)| 1200 weight/min/IP + 按地址成交额度(可用 USDC 买) | **`expiresAfter`** | 出块 ~70 ms;同机房 e2e 中位 0.2 s / p99 0.9 s;形态 = ALO 挂单 + Binance 对冲;优先费机制 |

选择原则:**能用 WS 会话下单的一律 WS,会话登录一次,不逐单签名**(Binance Ed25519 logon;OKX/Bybit/Gate/Bitget UTA/KuCoin Pro 登录后免签;REST 只作备用);REST 只用于对账与查询;FIX 仅在 Binance 现货 SBE/FIX 组合被证明更快时启用(Binance Spot SBE/FIX 是可选扩展)。**每单都带过期保护**(OKX `expTime`、Hyperliquid `expiresAfter`、其余收紧 `recvWindow`),让"发出但迟到"的订单在交易所侧自动作废。所有 MM/低延迟项目并行申请——Binance Futures LP 计划(→ FAPI-MM/FSTREAM-MM 白名单)、KuCoin MM(DMA + co-location)、Gate MM、OKX VIP4/5、Bybit 机构 MMWS(11-12 后评估)、Bitget MM/PRO;这是免费的 p50/p99。OKX 没有专线;Deribit/Kraken/INTX 在伦敦或并入 Deribit(2026-10-01),不作延迟敏感腿。

**M3 之前必须核实**(决定 egress 形态——是否逐单签名、回报从哪条连接来;五路评审 V5 §3.4):Bitget UTA V3 WS 下单频道(每频道限速与批量上限)、Binance USDⓈ-M 测试网是否有 WS API、私有频道能否在主备两条连接上同时订阅。

第二次独立评审核对过的交易所事实(实现与金样的唯一归属在各 `venue-*` README):Binance 带 `recvWindow` 的请求返回 NotFound 不定终态,按 `Unknown` 处理并以成交历史确认;各所心跳口径按产品线(现货 / USDⓈ-M)分别写;OKX 的数量换算只走 refdata 的线格式指数(含 `ctVal` 平移),venue-okx 运行时不再除 `ctVal`,金样加 `ctVal ≠ 1` 与 `1000PEPE` 类合约;Bitget 单向持仓下的强平标记待核实。

## 8. 失败模式清单(必须有测试)

| 场景 | 处理 |
|---|---|
| 下单发出后连接断,未收回执 | 槽 → `Unknown`;重连后查询收敛;期间该 group 禁开 |
| 交易所返回限速错误 | 令牌桶清零 + 指数退避;`RateLimitHit` 事件;策略侧 `Throttle` 联动 |
| `Fill` 先于 `Ack` | 直接进 `Live/Filled`,后到的 `Ack` 幂等忽略 |
| 撤单被拒(已成交) | 用回报里的成交量更新;不重试 |
| 第二腿 venue 的 BBO 连接不健康(断开、重同步、`conn_liveness_ms` 内无任何回帧)或资金不足 | 带 LegLink 的开仓整体被拒(`LINK_STALE` / `LINK_FUNDS`),第一腿不发 |
| 第二腿盘口 10 s 不变(安静行情) | 连接健康即报价有效:开仓、对冲、自动平仓照常(DST 必过) |
| 两腿 lot 不能互相整除,最小公倍数名义 > `max_chunk_notional` | `SetParams` 拒绝(`InvalidArgs{LotMismatch}`) |
| `Kill{Account, flatten}` 之后策略给出新的加仓目标 | `flatten_lock`:只接受更接近 0 的目标与 Reduce/Flatten,直到 `Resume{Kill}` |
| 对冲腿对手价偏离超过 `max_slip` | 按 `rr_max_cross` 上限改价照发 IOC;超过 `rr_max_cross` 才是门控拒绝(不计失败次数,累计时长计入截止) |
| 第二腿被拒 | `Simultaneous`:反向平第一腿(可配);`OnFill`:以第二腿角色重试(新意图、新价格,继承 reduceOnly 语义),计次与截止到期 → `Broken` + 账户级 `HedgeFailure` cap |
| Broken 持续 `broken_auto_fix_ms` 无人处理 | 自动一次有界平掉多出的一腿;成功 P1(仍需 `Resume`),失败 P0 保持 Broken;HARD 下不做 |
| 保证金 L3 或 `Kill{flatten}` 时有 Broken 的 group | 同样减仓:先永续腿 reduce-only(`PerpFirst`,受 `max_reduce_notional`),再现货腿 |
| 保证金 L3 或 `Kill{flatten}` 时另一所 BBO 断开(如 30 s) | 只要求永续腿报价可用:退回串行路径 `SerialFallback`,永续仍按步 reduce-only 减仓,现货腿等报价恢复;P0;接受短时现货裸多(DST 必过) |
| 引擎单(补腿、自动平仓、L3、flatten)价差扩大时 | 限价与名义按 `cap_cross_px` 与 `max_order_notional` 截取,不会每次被价格带拒;仍被风控拒 → 按执行失败处理 + P0 |
| 冷启动时有未对冲的链接 | 停机时间不计入 `hedge_deadline_ms`;闸门放行时先做一次 `SecondRetry`,再按正常截止 |
| 无 client id 的成交 | gw 暂存 `gw.unmatched_hold_ms`(5 000)后仍无法归属才发 `ExternalFill{Unknown}`;第二腿量以裸量为上限,迟到匹配不会重复对冲 |
| 对冲腿查无此单(NotFound,含 Binance `recvWindow` 类 NotFound) | 不直接定为 Rejected:置 `Unknown` 直到成交历史确认;回收的槽保留墓碑,迟到成交照常入账并报 P0(否则会双重对冲) |
| 自身被强平 / ADL / 交割 | `ExternalFill` 入账、更新 LegTracker、P0、通知策略;截止内未处理 → 执行层平掉多出的一腿 |
| 人工网页成交(`ExternalFill{Manual}`) | 入账、更新 LegTracker、通知策略;**不做引擎默认处理**(不自动平),该 cell 置 `manual_hold`(补腿、ResidualTrim 与自动平仓计时一并抑制,`Note{ExecManualHold}` P1),由策略按自己的规则处理 |
| 保证金 L3 或 `Kill{flatten}` 时策略挂着 reduce-only 平仓单 | 引擎先 `cancel_resting_on` 撤该 inst 本账户全部非 EXEC 挂单,撤单确认前按 `\|pos\| − 即时在途` 截量,仍按步减(DST) |
| 用户流断线期间发生 ADL | reconciler 第二轮见到永续同号缩小、无在途、无 Unknown,且用户流两轮都在线并已稳定 → `AUTO_EXTERNAL` → sequencer 在该 Input 内合成成交入账 + P0;之后补推的真实强平/ADL 先抵扣 `auto_held` |
| 残余低于最小下单量,HedgeFailure 解不开 | 装配期 `DustBelowMinNotional` 防止出现;仍出现时 Admin `WriteOffResidual{account, group}` 对静止的 group 核销多出那一腿的已成交裸量(移到未归属持仓)并清零全部腿的残余 |
| 现货手续费以基础资产扣除 | 仓位、库存与对冲按净量;现货卖出量取 min(库存, 可用余额);费用晚到时补差 |
| 账户配置与要求不符(如双向持仓、现货账户开着借币) | 启动闸门不过;运行中变化 → cap |
| 任一把 key(Ed25519 会话 key、HMAC REST key、kill-switch key、reconciler 只读 key)查到的 UID ≠ 登记的 `venue_uid` | hft-core 退出 2 + P0;kill-switch 自检与 reconciler 启动同样核验 |
| 下架公告 / 结算周期变化 | 控制命令 `VenueNotice`:下架 → 该 inst CloseOnly + 通知策略;周期变化随 `Funding` 事件生效 |
| Drain 时仍有在途链接或引擎减仓 | Drain 等全部开仓/对冲链接结束、全部 IOC 终态、重试计时器清空,以及没有未到达的 `MarginStress`/`Hard` 引擎目标(`exec.drain_ready()`)后才退出,仍受 `drain_timeout_ms` 约束;Drain 期间执行层只停增仓切片 |
| 交易所时间与本机漂移 > recvWindow | `ClockStatus` 偏移超界 → 时钟不可信 cap(只拦 Open);chrony/PTP 状态纳入健康检查 |
| 用户数据流中断 | 不影响交易(仓位以 Fill 为准);告警;重连后触发对账 |
| 死手开关刷新失败 | 视为连接异常;切备用连接;连续失败 → 主动撤单 |
