# 事件模型

sequencer 的一切输入与决策都是事件;本页是**记录头、kind 编号与分类、关键事件体布局与演进规则的唯一定义**。Rust 定义在 [`hft-event`](../../crates/hft-event/README.md),是唯一来源;`cargo xtask gen-schema` 从它生成 Arrow schema、Python dtype 与 `docs/generated/event-catalog.md`(生成后以生成物为准,本页只保留规则、编号表与关键布局)。journal 文件怎么装这些记录见 [journal-format.md](journal-format.md)。

## 记录头(24 B,全系统唯一)

```rust
#[repr(C, align(8))]
pub struct RecordHeader {
    pub len:   u32,   // 整条记录字节数(含头、含 8 B 对齐填充)
    pub kind:  u16,   // EventKind 编号,见下表
    pub flags: u16,   // RecordFlags 位
    pub seq:   u64,   // seq journal:sequencer 赋予的全局单调序号;raw/chain journal:该 journal 自己的序号
    pub tsc:   u64,   // seq journal:sequencer 取出该事件时的 TSC(= proc_tsc);raw journal:rx_tsc
}
```

`RecordFlags`:`CONFLATED`(同 inst BBO 积压被合并,回放照做)、`SHADOW`(意图在 Shadow 模式下只记录不出环)、`HW_TS`(`hw_rx_ns` 来自网卡硬件)、`SW_TS`(退化为软件时间戳)、`PART`(多段记录的非末段)、`CRC`(体尾带 crc32c)、`EVENT_END`(一个输入事件及其全部输出的最后一条记录;恢复截断到最后一个带此位的记录,见 [journal-format](journal-format.md#提交游标))。位值在 [hft-event](../../crates/hft-event/README.md)。未定义的位写 0,读者忽略未知位。

记录分三类 **Input / Output / Meta**:Input 喂给 sequencer(含 live driver 当场写下的环境决定 `Timer`);Output 由输入确定性地产生、验证回放逐字节比对;Meta(测量与结构记录)占 seq,但不参与状态与验证比对。三类的精确定义与按体决定类别的规则在 [hft-event](../../crates/hft-event/README.md#记录头标志与-kind);每个 kind 的归类见下表。

## 时间戳语义

| 字段 | 含义 | 来源 | 单位 |
|---|---|---|---|
| `ex_ts_ns` | 交易所事件时间(撮合/成交时间,如 Binance `T`) | 报文 | ns(交易所精度多为 ms/µs) |
| `ex_send_ns` | 交易所推送时间(如 Binance `E`);无则 0 | 报文 | ns |
| `hw_rx_ns` | 网卡硬件接收时间(PHC 域);不可用时为软件戳并置 `SW_TS`;都没有为 0;取哪个包的戳归 [hft-net §7](../../crates/hft-net/README.md#7-时间戳) | `SO_TIMESTAMPING` | ns |
| `sw_rx_ns` | 内核软件接收戳(`SOF_TIMESTAMPING_RX_SOFTWARE`,CLOCK_REALTIME 域;latency-budget 的 `sw_rx`);常态记录在 `MarketHdr` 与 `LatencySample`;不可用为 0 | `SO_TIMESTAMPING` | ns |
| `rx_empty_gap_ns` | 本连接上次确认接收队列为空的时刻到本次读到这批数据的间隔(`MarketHdr`,u32,饱和到 `u32::MAX`);本连接尚无确认时为 0;产生规则("确认为空"的判定与取哪次读取)唯一归属 [hft-net §7](../../crates/hft-net/README.md#7-时间戳),门槛 ⑩ 的用法归 [testing §8.2](../testing.md#82-延迟验收的测法) | feed 线程 | ns |
| `rx_tsc` | feed/gw 线程读完该帧最后一个字节时的 TSC;sim-gw 产出回报时的 TSC | `hft-rt` | tsc |
| 记录头 `tsc` | sequencer 取出时的 TSC(`proc_tsc`) | `hft-rt` | tsc |
| `engine_ns` | **引擎时间**:跨进程重启连续、停机时间不计;由 seq journal 的运行区间累加,确定(下述) | 由记录头 `tsc` 与 `EngineClock` 计算,不是记录字段 | ns |

TSC → UTC/PHC 用段头的校准对与 `ClockStatus` 事件换算:housekeeping 每 1 000 ms 写一条 `ClockStatus`(含 `(tsc, realtime_ns, phc_ns, calib_err_ns)`,[hft-event](../../crates/hft-event/README.md#控制与系统0x04xx0x05xx));延迟分段按样本前后两条 `ClockStatus` 线性插值换算并逐样本给误差界,误差界超限的样本计入 `unsynced_cal`、不进分位数——插值、误差界与 HW 时间戳不可用时以 `sw_rx` 为锚的规则唯一归属 [latency-budget §2.1](../latency-budget.md#21-跨时钟域换算)。**不同来源的时间戳不混算**;延迟分段名只用 [latency-budget.md](../latency-budget.md) 中的定义;模拟账户的样本不与实盘直方图混算。

**引擎时间 `engine_ns`(唯一定义)**:每次进程启动,live driver 在任何其他 Input 之前写一条 Input 记录 [`EngineClock`](../../crates/hft-event/README.md#engineclock0x050ainput)(`base_engine_ns`、`tsc_hz_milli`)。对 seq journal 中该条之后、下一条 `EngineClock` 之前的任意记录 r:

```
engine_ns(r) = E.base_engine_ns + ⌊ (r.tsc − E.tsc) × 10^12 / E.tsc_hz_milli ⌋     // E = r 之前最近的 EngineClock,E.tsc 是它的记录头 tsc;u128 中间量
E.base_engine_ns = engine_ns(上次运行最后一条已提交记录)                             // 无历史为 0;--recover-from-snapshot 时取 SnapshotHdr.engine_ns
```

- 性质:同一运行内严格随记录头 `tsc` 单调;跨重启从上次运行的末值接着走,停机时长与恢复耗时不计入;换机(TSC 不同)不影响,因为每次运行只用自己的 `tsc − E.tsc`。
- 用途:**凡是要跨进程重启保存的时刻**(策略状态、hft-strat-lib 构件、ledger、risk 的冷却与截止,SDK `Now.engine_ns`)一律存 `engine_ns`;TSC 只在一次运行内使用(定时器、`*_tsc` 测量字段)。唯一例外:OMS、hft-exec 与定时器堆的已登记 TSC 行随快照保存,恢复结束时由 `recovery_end` 按"停机时间不计"平移到本次运行(规则与行清单归 [hft-sequencer](../../crates/hft-sequencer/README.md))。读者从中途段起步时由该段引导块快照的 `EngineClock` 节取得 E([journal-format](journal-format.md#快照记录结构))。
- 其他进程(advisor、watchtower、replayer)按同一公式从 journal 计算,不读系统时钟;`AdvisorLag` 即以它计量([hft-event](../../crates/hft-event/README.md#控制与系统0x04xx0x05xx))。

## kind 编号段

| 段 | 域 | 用途 |
|---|---|---|
| `0x01xx` | 行情 | 交易所公开行情 |
| `0x02xx` | 执行 | 决策、发送结果、回报、账户状态(实盘与模拟账户共用) |
| `0x03xx` | 链上 | **optional**:编号段保留,链上策略接入时启用;`0x0310–0x033F` 是 chain journal 专用(布局在 [hft-ext-proto](../../crates/hft-ext-proto/README.md)) |
| `0x04xx` | 控制 | 命令、结果、配置内容、对账报告 |
| `0x05xx` | 系统 | 定时器、连接、策略慢速输出、快照、测量 |
| `0x06xx` | raw journal 专用 | 原文 |
| `0x7Fxx` | 测试与实验 | 只在 `dev` 构建中出现(hft-event feature `dev-kinds` 登记的测试 kind),生产读者拒绝 |

| 编号 | kind | 布局 | 分类 | 说明 |
|---|---|---|---|---|
| 0x0101 | `Bbo` | 定长 96 | Input | |
| 0x0102 | `L2Delta` | 定长 96 | Input | 逐档;feed 默认不产出 |
| 0x0103 | `L2Snapshot` | Multi | 按体:`origin = Exchange` 或 `TopN` Input,`Keyframe` Output | `TopN` = 分档快照流的一条消息(簿整替换,无序号链) |
| 0x0104 | `Trade` | 定长 88 | Input | |
| 0x0105 | `Funding` | 定长 80 | Input | `interval_s` = 实际结算周期(0 = 未知) |
| 0x0106 | `MarkPrice` | 定长 64 | Input | |
| 0x0107 | `IndexPrice` | 定长 64 | Input | |
| 0x0108 | `Liquidation` | 定长 80 | Input | 公开市场强平流(不是本账户) |
| 0x0109 | `L2Batch` | Var | Input | 增量深度(可选扩展)的产出:一条交易所增量消息一条记录 |
| 0x0201 | `OrderDecision` | 定长 88 | Output | 意图 + 风控结果 + OMS 分配 |
| 0x0202 | `SendReport` | 定长 48 | Input | gw/sim-gw 写出结果 |
| 0x0203 | `OrderEvent` | 定长 120 | Input | 回执/拒绝/成交/撤单/改单/过期/查询结果/结果未知/仅费用 |
| 0x0204 | `PositionReport` | 定长 56 | Input | 交易所推送,只校验 |
| 0x0205 | `BalanceReport` | 定长 40 | Input | |
| 0x0206 | `FundingSettle` | 定长 56 | 按体:`synth = 1` Output,否则 Input | `inst = INST_NONE` = 只推账户级合计 |
| 0x0207 | `ExternalFill` | 定长 96 | Input | 自身强平、ADL、人工、交割等非本系统下单的成交(`schema_version` 3 的结算字段归 hft-event) |
| 0x0208 | `MarginReport` | Var | Input | 保证金余额/维持/可用/保证金率 + 每 inst 强平价与 ADL 档 |
| 0x0209 | `AccountConfigReport` | Var | Input | 持仓模式、保证金模式、多资产、UID(含核对过的 key)与不一致位(含现货借币);尾部为每个永续 inst 的实际杠杆 |
| 0x020A | `ExecDecision` | 定长 104 | Output | 执行层的每次决定(目标、算法、切片、状态迁移、原因) |
| 0x0301 | `ChainHead` | 定长 80 | Input | optional |
| 0x0302 | `PoolQuote` | 定长 96 | Input | optional |
| 0x0303 | `SwapDecision` | 定长 128 | Output | optional |
| 0x0304 | `TxLifecycle` | 定长 152 | Input | optional |
| 0x0310–0x033F | chain journal 专用 | 见 hft-ext-proto | 见 hft-ext-proto | optional;`0x031E` 退役 |
| 0x0401 | `ControlCmd` | Multi | Input | 含策略通道命令 `StrategyCmd`、`InjectWarmup`(载荷由策略定义) |
| 0x0402 | `CmdResult` | 定长 144 | Output | `schema_version` 3:预览追加 `reset_groups`、`no_open_ms` |
| 0x0403 | `ConfigBlob` | Multi | 按体:`boot = 1` Output,否则 Input | |
| 0x0404 | `ReconcileReport` | 定长 104 | Input | 条目:仓位、余额、挂单、Unknown 收敛(带 `history_checked`)、汇总(`schema_version` 3 追加 `sflags`、`skipped_inflight`;仓位条目可带 `AUTO_EXTERNAL`)、逐笔资金费(交易所金额;只告警与账本校正,不进闸门) |
| 0x0405 | `ReconcileBatch` | Var 16 + ≤51200 | Input | 完整报告与原始结算凭据,布局归 hft-event |
| 0x0501 | `Timer` | 定长 24 | Input | owner/timer_id/group 编码见 hft-event |
| 0x0502 | `ConnState` | 定长 24 | Input | `schema_version` 2 追加 `rx_tsc`;`Alive` 是行情连接的活性证据 |
| 0x0503 | `Advice` | Var | Input | 策略模型的不透明载荷(`AdviceHdr` 32 B,含 `basis_seq`/`basis_tsc` + ≤ 4096 B) |
| 0x0504 | `StateSnapshot` | Multi | Output | |
| 0x0505 | `LatencySample` | 定长 136 | Meta | `schema_version` 3:`trigger_kind`(触发事件的 kind;`OrderEvent` 触发 = `rep_to_order` 样本);`link_role` 区分 `hedge_host` 子集;`action` 区分新单、撤单与改单(占用保留字节) |
| 0x0506 | `Note` | 定长 80 | 按体(`source`) | |
| 0x0507 | `HostProfile` | 定长 176 | Meta | |
| 0x0508 | `ClockStatus` | 定长 56 | Meta | 每 1 000 ms 一条 |
| 0x0509 | `SegmentRoll` | 定长 48 | **Meta** | 结构记录;滚动决定是 Input `Timer{Roll}` |
| 0x050A | `EngineClock` | 定长 32 | Input | 每次进程启动一条:引擎时间 `engine_ns` 的运行区间基准(见[时间戳语义](#时间戳语义)) |
| 0x0601 | `RawRx` | Multi | raw journal | 收到的解密后 payload |
| 0x0602 | `RawTx` | Multi | raw journal | 发出的明文订单帧;**登录帧与任何含签名的帧不记录** |

布局:"定长 N" = 体 N 字节;`Var` = 单条记录、定长前缀 + 变长尾部、无 `PartHdr`;`Multi` = 以 `PartHdr` 开头、可拆成多段(规则见 [journal-format](journal-format.md#变长与多段记录))。表中未列体大小的以 `gen-schema` 生成物为准。

kind 编号**永不复用**;废弃的 kind 在 `hft-event` 中保留为 `#[deprecated]` 常量。**模拟账户**的回报与实盘回报是同一组 kind(不另设编号或标志),读者按 `account` 查 refdata `AccountMeta.kind` 区分。**新增或修改策略不需要新 kind**:策略的慢速输出是 `Advice` 的不透明载荷,参数是 `ConfigBlob{StrategyParams}` 的 body,二者都按策略自带的定义解释([hft-strategy-sdk](../../crates/hft-strategy-sdk/README.md))。

## 关键事件体

> 草案:本节的字节布局(字段顺序、偏移、`_pad`、大小)实现时以代码与编译期断言为准;码值与字段名全仓一致。

所有体是 `#[repr(C)]` + `zerocopy::{IntoBytes, KnownLayout, Immutable}`,再加 `FromBytes`(无枚举字段)或 `TryFromBytes`(含 `Side`、`OrderEventKind` 等枚举字段——枚举的 0 值与越界值非法,读取时顺带校验);无隐式填充、大小是 8 的倍数,无指针、无 `String`/`Vec`。价格与数量是 `hft-types` 的 `Px`/`Qty`(i64,指数由资产决定,见 [hft-types](../../crates/hft-types/README.md))。

```rust
#[repr(C)] pub struct MarketHdr {        // 56 B,行情体公共前缀
    pub venue: VenueId,                  //  0
    pub conn: ConnId,                    //  1 送达该事件的连接;BBO 连接 = 其槽号(< MAX_BBO_CONN_SLOTS,常量在 hft-types;见下文活性证据)
    pub _pad: u16,                       //  2
    pub inst: InstId,                    //  4
    pub ex_ts_ns: i64,                   //  8
    pub ex_send_ns: i64,                 // 16
    pub hw_rx_ns: i64,                   // 24
    pub rx_tsc: u64,                     // 32
    pub sw_rx_ns: i64,                   // 40 内核软件接收戳;0 = 不可用
    pub rx_empty_gap_ns: u32,            // 48 本连接上次确认接收队列为空到本次读到的间隔(见时间戳语义);0 = 尚无确认
    pub _rsv: u32,                       // 52 保留(占用规则见演进规则)
}
#[repr(C)] pub struct Bbo {              // 96 B;update_id 0 = 该 venue 无序号(不去重)
    pub h: MarketHdr, pub bid_px: Px, pub bid_qty: Qty, pub ask_px: Px, pub ask_qty: Qty, pub update_id: u64,
}
#[repr(C)] pub struct L2Delta {          // 96 B;一条交易所增量消息拆成 N 条连续的 L2Delta(idx = 0..N-1),末条置 LAST
    pub h: MarketHdr,
    pub prev_id: u64,                    // 本消息作用于其上的簿状态 id(venue 协议归一化,见下)
    pub last_id: u64,                    // 应用本消息后的簿状态 id
    pub px: Px, pub qty: Qty,            // qty = 0 删档
    pub side: Side, pub dflags: u8,      // dflags: LAST = 1
    pub idx: u16, pub _pad: u32,         // idx:本档在消息内的序号
}
#[repr(C)] pub struct Trade { pub h: MarketHdr, pub px: Px, pub qty: Qty, pub trade_id: u64, pub taker_side: Side, pub _pad: [u8; 7] } // 88 B

#[repr(C)] pub struct OrderEvent {       // gw/sim-gw 解码或产生回报后写入,120 B(schema_version 3)
    pub venue: VenueId, pub account: AccountId, pub what: OrderEventKind, pub detail: u8 /* 按 what 取值(QueryResult 订单状态码;CancelReject/AmendReject 拒绝分类 GONE;PartialFill/Fill 订单方向),码表见 hft-event */
    pub slot: u32, pub gen: u32, pub inst: InstId,
    pub px: Px, pub qty: Qty, pub cum_filled: Qty,
    pub fee: Qty, pub fee_asset: AssetId, pub epoch: u16 /* 从 client id 解出的 engine_epoch;解不出为 0 */,
    pub reject_code: u32,                // 交易所原始拒单/错误码;数值码、负码与字符串码到 u32 的映射归 hft-venue-proto 契约 8;fee 按 fee_asset 的 qty_exp,正 = 支出
    pub exch_order_id: u64, pub ex_ts_ns: i64, pub hw_rx_ns: i64, pub rx_tsc: u64,
    pub avg_px: Px,                      // 累计成交均价(0 = 未成交);OMS 用 avg_px × cum_filled − 已记名义 求本次增量名义
    pub cash_qty: Qty, pub cash_sum: Qty, // v2 的结算事实;单位、覆盖量与缺失语义归 hft-event
    pub trade_id: u64,                  // v3 本笔成交 ID,0 表示无正 ID;不代表整个累计区间,语义归 hft-event
}
// cum_filled 是毛量且必须是累计量:给不出累计量的 venue 由 gw 按 exec id 合成(方法写在各 venue-* README)
// 净量 = 毛量按 refdata VenueCaps.fee_asset_rule 与本条 fee/fee_asset 扣减,由 hft-ledger 计算(FillDelta.net_qty)
// OrderEventKind: Ack, Reject, PartialFill, Fill, CancelAck, CancelReject, AmendAck, AmendReject, Expired, QueryResult, OutcomeUnknown, FeeOnly(编码见 hft-event)
// OMS 在 (venue, account) 的表内按 (epoch, slot, gen) 匹配槽;epoch 不是当前或上一 epoch 的回报按外来单处理(本账户的外来成交另以 ExternalFill 报告)

#[repr(C)] pub struct Timer { pub timer_id: u32, pub owner: u16, pub _pad: u16, pub group: GroupId /* GROUP_NONE(0)= 不适用 */, pub _pad2: u32, pub deadline_tsc: u64 } // 24 B;owner/timer_id 编码(Sys、Keyframe、Oms、Risk、Exec、Strategy)见 hft-event 定时器编码
#[repr(C)] pub struct ConnState {        // 24 B(schema_version 2)
    pub venue: VenueId, pub conn: u8, pub state: ConnStateKind, pub role: ConnRole,   //  0
    pub inst: InstId,                    //  4 0 = 整条连接;≠ 0 = 只涉及该 inst(如单个 inst 的 Resync)
    pub reason: u32,                     //  8 原因码,码表归 hft-venue(REPORT_DECODE、ACCT_QUERY 等)
    pub account: AccountId /* 0 = 行情连接 */, pub _pad2: [u8; 3],                                               // 12
    pub rx_tsc: u64,                     // 16 v2:引起本状态的帧(或检测)时刻;Alive = 收到 pong/心跳帧的 rx_tsc;v1 记录读出 0 = 未提供
}
// UserStream 断开同样通知 OMS:该 (venue, account) 的非终态槽标 VERIFY(回报可能有缺口)
// Rest:REST 池(账户配置、保证金轮询、查询)的状态;reason 码表(REPORT_DECODE、ACCT_QUERY…)由 hft-venue 登记
// ConnStateKind: Up=1, Down=2, Resync=3, Rotated=4, Degraded=5, Alive=6(只追加)
#[repr(u8)] pub enum ConnRole { Md = 1, Order = 2, UserStream = 3, Rest = 4 }   // 连接角色编码的唯一归属(只追加);hft-net、hft-venue 用同一类型

#[repr(C)] pub struct TxLifecycle {      // optional 链上:chain-core → hft-core,152 B;全字段只在此定义,external-venue-protocol 只写填充语义
    pub intent_id: u64, pub state: TxState, pub builder: u8, pub account: AccountId, pub _pad: [u8; 5],
    pub block: u64, pub amount_in: Qty, pub amount_out: Qty, pub gas_used: u64, pub gas_cost: Qty,
    pub tx_hash: [u8; 32], pub observed_ts_ns: i64,
    pub reason: u16, pub tflags: u16,    // 原因码与 EARLY/BAL_MISMATCH/SYNTH 标志,码表见 external-venue-protocol
    pub src_mask: u8, pub wallet: u8, pub _pad2: [u8; 2],
    pub bribe: Qty,                      // 付给 builder 的出价(BNB 的 qty_exp)
    pub rx_tsc: u64,                     // hft-core 收到(ext 线程填写)
    pub sig_hi: [u8; 32],                // Solana:64 B 签名的后 32 B(前 32 B 在 tx_hash);BSC 为 0
}
// TxState: Sent, IncludedOk, IncludedReverted, Dropped, Unknown, Finalized
```

**行情连接的活性证据**(报价新鲜度按**连接健康**判定,不按报价变化时刻;安静的 inst 几秒无推送不算过期):

1. 每条行情记录的 `MarketHdr.(venue, conn, rx_tsc)` 证明该连接在 `rx_tsc` 时活着(数据帧本身就是证据,繁忙连接不需要额外记录)。
2. 安静的连接靠 `ConnState{state = Alive, role = Md, inst = 0, account = 0, rx_tsc = 回帧的 rx_tsc}` 证明存活:feed 任务收到探测回帧(pong 或交易所心跳)且该连接最近一段时间没有向环推过数据记录时发一条,走该连接所在的环(BBO 连接 → `bbo-<venue>`,其余行情连接 → `md-<venue>`;环表归 [hft-sequencer](../../crates/hft-sequencer/README.md))。探测周期、"最近一段时间"的阈值与发布条件的唯一定义在 [hft-venue §3a](../../crates/hft-venue/README.md);热路径代价:每个回帧一次比较,安静连接上约每秒一条记录。
3. `ConnState{Up}` 同样带 `rx_tsc`(恢复的时刻);`Down` 之后该连接的活性证据作废,直到下一次 `Up`;`Resync`(`inst = 0` 为整条连接,`≠ 0` 为单个 inst)表示其后的报价在对应的 `Up` 之前不可用。
4. **按成员聚合**:每个 venue 的 BBO 连接占槽 `0..MAX_BBO_CONN_SLOTS`(常量在 [hft-types](../../crates/hft-types/README.md#id-与枚举);连接 id 即槽号,退役后复用,槽的分配归 [hft-venue](../../crates/hft-venue/README.md))。非 BBO 连接(槽号 ≥ `MAX_BBO_CONN_SLOTS`)的 `Alive` 不改变任何 BBO 健康状态,也不投递给策略。sequencer 每 (venue, 槽) 维护 `ConnLive{up, resync, live_tsc}`(`live_tsc` = 最近一次活性证据的 `rx_tsc`),每 inst 维护 BBO 连接成员位图 `bbo_member_mask`(由 `ConnState{Up/Down, conn, inst}` 维护);SDK `Views::bbo(i)` 按值返回 `BboView`,成员如何聚合成 `conn_up`、`conn_live_tsc`、`resync` 只在 [hft-sequencer 连接活性](../../crates/hft-sequencer/README.md#连接活性)定义。字段与健康判定 `now − conn_live_tsc ≤ conn_liveness_ms` 的定义归 [hft-strategy-sdk](../../crates/hft-strategy-sdk/README.md),维护与快照节归 [hft-sequencer](../../crates/hft-sequencer/README.md)。全部输入都在 journal 里,回放确定。

**分档快照流**(`L2Snapshot{origin = TopN}`)没有序号链:每条消息整体替换该簿(簿只含这 N 档;应用规则归 [hft-book](../../crates/hft-book/README.md)),不做 `prev_id` 校验,也没有 REST 重同步。**`L2Delta`/`L2Batch`(增量深度,可选扩展)的序号归一化**:各所的序号规则(Binance 合约 `pu`、现货 `U/u` 重叠、OKX `seqId/prevSeqId`、Bitget `seq/pseq`、快照后的首条衔接)由 venue 协议在 feed 线程内判定,并归一化为"`prev_id` = 本消息作用于其上的状态 id";快照后的首条消息 `prev_id` 置为快照的 `snap_id`。簿只做通用校验 `prev_id == 当前 last_id`(见 [hft-book](../../crates/hft-book/README.md))。

其余体——`Funding`、`MarkPrice`、`IndexPrice`、`Liquidation`、`OrderDecision`(含 `link_role` 编码与 `OrderFlags` 位值)、`SendReport`、`PositionReport`、`BalanceReport`、`FundingSettle`、`ExternalFill`、`MarginReport`、`AccountConfigReport`(含每 inst 杠杆)、`ExecDecision`(含 `TargetOrigin` 线上码)、`Advice`、`ChainHead`、`PoolQuote`、`SwapDecision`、`CmdResult`、`ReconcileReport`、`LatencySample`、`Note`(含 `source` 表与 `AdvisorLag` 的唯一定义)、`HostProfile`、`ClockStatus`、`SegmentRoll`、`EngineClock`——的布局、各枚举的编码与测量字段表在 [`hft-event`](../../crates/hft-event/README.md#事件体布局) 定义并由 `gen-schema` 导出;`StrategyParams` 的容器(`StrategyParamsHdr` + `GroupSpec`)在 [hft-event 策略载荷容器](../../crates/hft-event/README.md#策略载荷容器);变长记录(`L2Snapshot`、`ControlCmd`、`ConfigBlob`、`StateSnapshot`、`RawRx`、`RawTx`)的分段规则见 [journal-format.md](journal-format.md#变长与多段记录)。线格式枚举与位(`RunMode`、`KillLevel`、`CapSource`、`KillBits`、**`RejectBits`**)在 [hft-types](../../crates/hft-types/README.md)。

## 演进规则

1. **只追加**:新字段只加在体尾,`schema_version` + 1;读者按 `len` 截断或补零,旧读者忽略新字段。
   - **占用保留字节**:也可以把体内已有的 `_pad*`/`_rsv*` 字节改成具名字段,条件是不改体的大小与其他字段的偏移、全零即该字段的缺省含义(旧写者写出的零读出后语义正确)、不是 0 非法的枚举;此时不要求升 `schema_version`。保留字节够用时优先这样做。
   - **公共前缀**:`MarketHdr` 嵌在每个行情体开头,给它追加字段会移动各行情体其后字段的偏移,所以它不按只追加演进:新字段只能占用它的 `_rsv`;再要改就按第 2 条用新 kind。
2. **改语义用新 kind**:字段含义、单位、指数变化一律新 kind,旧 kind 保留读取能力。
3. **每个 kind 有 `schema_version`**:记录在段头的 schema 表里;`gen-schema` 生成物带版本。
4. **记录上限 64 KiB**:更大的内容(快照、大 blob)用 `Multi` 布局拆成多段,非末段置 `PART`;`Var` kind 必须声明尾部上限且整条 ≤ 64 KiB。
5. **决策也是事件**:`OrderDecision`/`ExecDecision`(以及 optional 的 `SwapDecision`)记录策略输出、执行层选择、风控结果与 OMS 分配,使意图流可逐字节比对;不要只记录"发了什么帧"。
6. **策略不登记事件**:策略的慢速输出用 `Advice` 的 `local_kind` 自行区分,参数用 schema,人工操作与冷启动预热数据分别用 `StrategyCmd`、`InjectWarmup` 命令的载荷,Note 码用 SDK `StrategyDesc.notes`;只有引擎需要理解的新事实(新回报类型、新账户状态)才新增 kind。
7. 新增或修改 kind 的步骤见 [cookbook/adding-an-event-kind.md](../cookbook/adding-an-event-kind.md)。

## 延伸阅读

- [hft-event](../../crates/hft-event/README.md)、[journal-format](journal-format.md)、[hft-strategy-sdk](../../crates/hft-strategy-sdk/README.md)(`EventRef` 怎样把这些事件交给策略)
- 决策出处:[round8-decisions](../../design/_work/round8-decisions.md)
