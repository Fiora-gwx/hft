---
description: "冷进程(watchtower、journal-archiver、replayer、hft-py)用它把 seq journal 里的 LatencySample、行情记录头、计数类事件、保证金与账户数据、Note 变成直方图、按事件加权的离线延迟统计、指标、账户日报、按 SDK 描述符通用汇总的策略日报与可读文本,并提供门槛 ⑩ 同机干扰的 bootstrap 判定;热路径只负责把时间点写进事件"
kind: crate
tier: cold
scope: core
status: spec
---

# hft-telemetry

## 概要

`hft-telemetry` 拥有**观测的冷侧实现**:`LatencySample` 时间点(定义在 latency-budget,含常态记录的 `sw_rx`)的分段计算(含按前后两条校准对插值的跨时钟域换算与逐样本误差界)、按维度分组的 hdrhistogram 聚合、**按事件加权的离线统计** `event_weighted()`(对 seq journal 中每条行情记录算 `kernel_rx`、`ingest`、`feed_host`,命令行入口是 replayer)、从 journal 事件派生的计数器与保证金指标、**账户报告**(`account_report`:模拟与实盘同口径的 PnL 分解、成交率、两腿间隔、裸敞口、保证金占用)、**策略报告**(`strategy_report`:按策略 schema JSON 中的 SDK 描述符 `notes`/`report` 通用汇总 `Note{source = Strategy}`,不认识任何具体策略、不依赖任何策略包)、`Note` 记录的翻译表,以及 watchtower 规则的纯判定函数(含门槛 ⑩ 同机干扰的分块 bootstrap 计算)。它**不在热进程里运行**:热路径只读时间戳并写进事件——`LatencySample` 由 [hft-oms](../hft-oms/README.md) 合成(新单、撤单与改单各在其首个交易所响应时,以 `action` 区分;规则归 hft-oms)、由 sequencer 写入 journal([09 §1](../../design/09-可观测性、风控与控制面.md))。使用方:[watchtower](../../apps/watchtower/README.md)(实时尾随 journal → Prometheus、告警、日报、同机干扰)、[journal-archiver](../../apps/journal-archiver/README.md)(`account_daily` 数据集)、[replayer](../../apps/replayer/README.md)(直方图与 `replayer latency`)、[hft-py](../hft-py/README.md)(Python 面板与模拟结果分析)。延迟分段的**名字、定义、插值与误差界公式**以 [latency-budget](../../docs/latency-budget.md) 为准;同机干扰的测法与阈值以 [testing §8.2](../../docs/testing.md#82-延迟验收的测法) 为准;账户报告字段的**定义**以 [hft-sim 报告字段](../hft-sim/README.md#报告字段) 为准,本 crate 是它们的实现。

## 目录

- [使用本组件](#使用本组件)
  - [输入:时间点](#输入时间点)
  - [分段(`StageSet`)](#分段stageset)
  - [按事件加权的离线统计(`event_weighted`)](#按事件加权的离线统计event_weighted)
  - [账户报告(`account_report`)](#账户报告account_report)
  - [策略报告(`strategy_report`)](#策略报告strategy_report)
  - [告警判定所需的派生量](#告警判定所需的派生量)
  - [保证金与账户指标](#保证金与账户指标)
- [实现说明](#实现说明)
- [延迟与资源契约](#延迟与资源契约)
- [测试与验收](#测试与验收)
- [依赖](#依赖)
- [延伸阅读](#延伸阅读)
- [已知限制与后续工作](#已知限制与后续工作)

## 使用本组件

```rust
/// 从 journal 读者喂入记录;内部维护时钟校准序列、直方图、计数器、账户报告累加器。
pub struct Telemetry { /* 见实现说明 */ }
impl Telemetry {
    pub fn new(cfg: &TelemetryConfig, rd: &RefData, cat: &StrategyCatalog) -> Telemetry;
    pub fn ingest(&mut self, hdr: &RecordHeader, body: &[u8]);        // 任意 seq/raw journal 记录(多段记录传重组后的逻辑体;链上 journal 为可选扩展)
    pub fn finish(&mut self);                                          // journal 结束:仍在等下一条 ClockStatus 的样本按"缺少后一条"计算(计入 unsynced_cal)
    pub fn cal_queue(&self) -> &CalQueue;                              // 校准队列(≤ 4 条)
    pub fn window(&mut self, now_utc_ns: i64) -> Option<WindowReport>; // 每 window_s 滚动一次
    pub fn day_close(&mut self, day: u32) -> DailyClose;               // UTC 日切:账户报告 + 策略报告;day = 自 1970-01-01 起的 UTC 日序号
    pub fn stages(s: &LatencySample, cal: &ClockCal) -> StageSet;      // 纯函数:时间点 → 分段(含逐样本误差界)
    pub fn note_text(n: &Note) -> NoteText<'_>;                        // 纯函数:Note → 文本(Display)
}

/// 一个样本前后两条 ClockStatus 构成的插值区间(加上前一个区间,用于误差界的速率差项)。
pub struct ClockCal { /* prev: Option<ClockStatusRow>, a: ClockStatusRow, b: Option<ClockStatusRow> */ }
pub struct StageSet { /* 每个分段:Option<i64 ns>;err_bound_ns: u32;cal_ok: bool(误差界未超阈值) */ }

pub struct WindowReport {
    pub start_ns: i64, pub end_ns: i64,
    pub hist: Vec<(HistKey, Quantiles)>,     // p50/p90/p99/p99.9/max/count
    pub cal_err: Vec<(HistKey, Quantiles)>,  // 逐样本误差界的分布(键的 stage 记为 kernel_rx:跨域误差限制在这一段)
    pub counters: Vec<(CounterKey, u64)>,    // 含 unsynced、unsynced_cal、missing、negative
    pub gauges: Vec<(GaugeKey, i64)>,        // 保证金率、MarginStress 档位、资金费数据年龄…
    pub clock_trust: ClockTrust,             // 本窗口含交易所时钟的分段是否可信
}
pub struct TelemetryConfig { pub window_s: u32 /* 10 */, pub sig_digits: u8 /* 3 */, pub min_ns: u64 /* 1 */,
                             pub max_ns: u64 /* 10 s */, pub max_series: u32 /* 基数上限,由使用方按内存上限给出,见"延迟与资源契约" */,
                             pub clock_untrusted_offset_ns: i64 /* 200 µs */, pub cal_max_err_ns: u32 /* 取 latency-budget §2.1 的阈值,调用方传入 */,
                             pub naked_usd: Notional, pub dataset: Dataset /* Live;见"直方图"的数据集 */ }
pub struct DailyClose {
    pub day: u32,
    pub hist: Vec<(HistKey, Quantiles)>,     // 按日累计的直方图(日报与回归基线用),cal_err、counters 同理
    pub cal_err: Vec<(HistKey, Quantiles)>,
    pub counters: Vec<(CounterKey, u64)>,
    pub accounts: Vec<AccountDaily>,         // 每账户一行 + 每 (账户, group) 一行
    pub strategies: Vec<StrategyDailyRow>,   // 见"策略报告"
}

/// 策略描述符目录:从 `hft-core schemas --out` 的输出目录加载(index.json + <kind_id>-<name>.schema.json)。
/// 只读规范 schema JSON 中的 "notes"、"report" 数组(格式归 hft-strategy-sdk),不链接任何策略包。
pub struct StrategyCatalog { /* kind_id → { name, notes: Vec<NoteDescOwned>, report: Vec<ReportFieldOwned> } */ }
impl StrategyCatalog {
    pub fn load(dir: &Path) -> Result<StrategyCatalog, CatalogError>;   // 校验 index.json 中每个文件的摘要;不符 → DigestMismatch
    pub fn empty() -> StrategyCatalog;                                  // 没有策略报告需求的使用方(replayer 直方图与 latency)
    pub fn note(&self, kind_id: u16, code: u32) -> Option<&NoteDescOwned>;
}
pub struct NoteDescOwned { pub code: u32, pub name: String, pub level: NoteLevel, pub v: Vec<String> }
pub struct ReportFieldOwned { pub name: String, pub unit: String, pub agg: ReportAgg, pub scope: ReportScope, pub note: u32, pub v_ix: u8 }
pub use hft_event::NoteLevel;            // P0 = 1、P1 = 2、P2 = 3、Info = 4(码表归 hft-event,本 crate 不另定义)
pub enum ReportAgg { Count, Sum, Last, Max, P99 }
pub enum ReportScope { Instance, Group }
```

### 输入:时间点

行情记录的接收来源必须有明确的 `HW_TS` 或 `SW_TS`；标志缺失或冲突时不把非零 `hw_rx_ns` 推断成 HW，不产出依赖该端点的分段。独立的 TSC `ingest` 与有效 `sw_rx_ns` 的 `kernel_to_user` 仍可统计，来源维度为 Missing；旧记录不补造来源。

时间点的名称、编号、时钟域与语义**只在 [latency-budget §1](../../docs/latency-budget.md) 定义**(含尾部追加、常态记录的 `sw_rx`);`LatencySample`(含触发事件 kind `trigger_kind` 与 `link_role`)与 `MarketHdr` 的字节布局在 [hft-event](../hft-event/README.md)。本 crate 读 `LatencySample`、行情记录头与 `MarketHdr`、`ClockStatus`,以及触发/回报事件体;链上分段(可选扩展)读 seq journal 与 chain journal 的对应记录。

### 分段(`StageSet`)

分段的名称与公式**只在 [latency-budget §2](../../docs/latency-budget.md) 定义**;`StageSet` 逐一实现其中的生产分段(`ex_queue`、`wire_in`、`feed_oneway`、`kernel_rx`、`ingest`、`feed_host`、`decide`、`egress`、`kernel_tx`、`tick_to_trade`、`rep_to_order`、`hedge_host`、`order_rtt`、`tick_to_ack`、`leg_gap`,以及可选扩展的链上分段)与由 `sw_rx` 拆出的诊断分段 `nic_to_kernel`、`kernel_to_user`(常态进生产直方图,不作门槛)——不新增、不改名。取数注意:

- **按触发事件分类**(`trigger_kind`;v2 记录为 0 时按 `trigger_seq` 指向的记录头取 kind):触发事件是行情记录的样本产出 `tick_to_trade`;触发事件是回报帧(`OrderEvent`)的样本产出 `rep_to_order`,其中 `link_role = Second`(不含 `SecondRetry`)的同时产出 `hedge_host`;触发事件既不是行情也不是回报(定时器、命令、Advice)的样本只产出不含 `hw_rx` 的分段(`decide`、`egress`、`order_rtt` 等)。`rep_to_order` 按回报种类分组(口径归 latency-budget §5)。
- **按 `action` 取样**(`LatencySample.action`,码表归 [hft-event](../hft-event/README.md)):`order_rtt` 只取 `action = New`;`rep_to_order` 含 New、Cancel、Amend 三种;`action` 是维度键的一项(诊断用),门槛的分组仍按 latency-budget §5。
- **跨时钟域换算**按 latency-budget §2.1:含 PHC 端点的分段用样本前后两条 `ClockStatus` 的校准对线性插值(不取阶梯值),逐样本按 §2.1 的公式给误差界;误差界 > `cal_max_err_ns`,或缺少后一条 `ClockStatus` → 该样本计入 `unsynced_cal`,不进分位数,误差界照常进误差分布直方图。`src` 显示 `hw_rx` 为 SW(`SW_TS`)时以 `sw_rx` 为锚、用 TSC ↔ REALTIME 端插值,样本进 SW 数据集。含交易所时钟的分段:样本前后两条 `ClockStatus` 中任一条的偏移界(`max_error_ns`)超过 `clock_untrusted_offset_ns` 时该样本不进分位数、计入 `unsynced`,所在窗口的 `clock_trust` 记为不可信。
- `kernel_tx` 需要 housekeeping 读回的 `tx_sw`(可选扩展,默认关),按 `OPT_ID` 与 `SendReport` 配对。
- `leg_gap` 把第二腿的 `LatencySample`(`trigger_seq` 指向第一腿成交)与触发记录的 `proc_tsc` 配对。
- `nic_to_kernel`、`kernel_to_user`:样本 `sw_rx` 非 0 时计算;`sw_rx` 缺失计入 `missing`,不填 0。
- **Sim 账户的订单不进延迟直方图**:它们的 `order_rtt` 等是模拟延迟的抽样,只进账户报告;维度键带 `account_kind`,`Sim` 的样本单独成数据集。

### 按事件加权的离线统计(`event_weighted`)

订单样本按时间抽样,会低估突发时段;行情侧分段因此对 seq journal 中**每条**行情记录离线计算(口径归 [latency-budget §6](../../docs/latency-budget.md#6-验收方法),门槛 ⑩ 的用法归 [testing §8.2](../../docs/testing.md#82-延迟验收的测法)):

```rust
pub struct EventWeightedCfg {
    pub from_seq: u64, pub to_seq: u64,     // 或由调用方按 UTC 时间换算出的区间
    pub cal_max_err_ns: u32,                // 同 TelemetryConfig
    pub burst_gap_ns: u32,                  // 突发/平静分层的阈值(取值归 testing §8.2,调用方传入)
    pub max_series: u32,
}
pub enum Stratum { Burst, Calm }            // 判定规则归 testing §8.2
pub struct EwKey { pub venue: VenueId, pub kind: EventKind, pub stage: Stage /* kernel_rx | ingest | feed_host | nic_to_kernel | kernel_to_user */,
                   pub ts_src: TsSrc /* Hw | Sw */, pub inst_class: InstClass, pub stratum: Stratum }
pub struct EventWeightedReport {
    pub hist: Vec<(EwKey, Quantiles)>,
    pub kernel_rx_hi: Vec<(EwKey, Quantiles)>,   // kernel_rx 的上界(公式归 testing §8.2;stage 字段固定为 kernel_rx)
    pub n: u64, pub missing: u64, pub gap_missing: u64 /* rx_empty_gap_ns = 0,无上界 */, pub negative: u64, pub unsynced_cal: u64,
    pub unsynced_cal_by_stratum: [(Stratum, u64 /* unsynced_cal */, u64 /* n */); 2],
    pub cal_err: Vec<(EwKey, Quantiles)>,   // 逐样本误差界的分布
    pub kernel_rx_hi_sw_gap_only: bool,     // kernel_rx_hi 含 SW 键:其上界只有 rx_empty_gap_ns,不含网卡到软中断一段
    pub series_overflow: u64,               // 超过 max_series 的新键被丢弃的样本数
    pub host_profile: [u8; 32], pub release: String,   // 数据集维度(取自段首 HostProfile)
}
/// 同 event_weighted,另对每个记入直方图的样本调用 visit(按 seq 顺序;replayer 导出逐条样本、测试逐值核对用)。
pub fn event_weighted_with(rd: &mut JournalReader, rdata: &RefData, cfg: &EventWeightedCfg, visit: impl FnMut(&EwSample))
    -> Result<Vec<EventWeightedReport>, TelemetryError>;
pub struct EwSample { pub seq: u64, pub key: EwKey, pub value: i64, pub hi: bool /* kernel_rx 上界样本 */ }
/// 只读:顺序读 seq journal(读者 API),每条带 MarketHdr 的行情记录(Bbo、Trade、Mark/Index、Funding、L2Snapshot,
/// 增量深度启用时含 L2Batch)取记录头 tsc(= proc_tsc)与 MarketHdr 的 hw_rx/sw_rx/rx_tsc/rx_empty_gap_ns,按"分段"一节的插值换算。
pub fn event_weighted(rd: &mut JournalReader, rdata: &RefData, cfg: &EventWeightedCfg) -> Result<Vec<EventWeightedReport>, TelemetryError>; // 每个数据集(段首 HostProfile)一份,按 seq 顺序

/// 纯函数:一条行情记录的层(frames_in_read = 同一连接 (venue, MarketHdr.conn) 上 rx_tsc 与它相同的行情记录数,含它自己,≥ 1)。
pub fn stratum(frames_in_read: u16, rx_empty_gap_ns: u32, burst_gap_ns: u32) -> Stratum;

/// 探针样本分层:行情触发的 LatencySample 按其触发记录的层分组,出各层分位数,并按同一窗口 event_weighted 的
/// 两层记录占比加权合成 p50/p99(合成规则归 testing §8.2)。只读,命令行入口是 replayer latency。
pub fn stratified_probe(rd: &mut JournalReader, rdata: &RefData, cfg: &EventWeightedCfg, ew: &EventWeightedReport)
    -> Result<StratifiedReport, TelemetryError>;
pub struct StratifiedReport { pub groups: Vec<StratifiedGroup> }
pub struct StratifiedGroup { pub key: HistKey, pub n: [u64; 2], pub by_stratum: [Option<Quantiles>; 2] /* Burst, Calm */,
                             pub weight: [f64; 2], pub composite_p50_p99: Option<(i64, i64)> }
```

- 同一次 `recvmsg` 解出的多条消息共享 `hw_rx/rx_tsc`,每条都计一个样本(这正是按事件加权)。
- `kernel_rx` 的上界、分层规则与加权合成的公式与阈值只在 [testing §8.2](../../docs/testing.md#82-延迟验收的测法) 定义,本 crate 是它们的实现;`rx_empty_gap_ns` 的产生归 [hft-net §7](../hft-net/README.md#7-时间戳),字段归 hft-event。
- 段首 `HostProfile` 变化(换主机类、换版本)即开新的数据集,不合并;实盘与 Sim 实例的 journal 分开计算。
- 不修改 journal、不写任何文件;输出由调用方(`replayer latency`、watchtower 日报)序列化。

### 账户报告(`account_report`)

字段定义见 [hft-sim 报告字段](../hft-sim/README.md#报告字段)(Sim 与 Live 同口径);本模块的计算规则:

| 字段 | 输入事件 | 计算 |
|---|---|---|
| `pnl_*` | `OrderEvent` 成交、`ExternalFill`、`FundingSettle`(含合成)、`Mark`、`Bbo`、`ConfigBlob`(refdata) | 以 [hft-ledger](../hft-ledger/README.md) 的同一记账代码重建权益视图与 `PnlBreakdown`;`pnl_basis` / `pnl_directional` 按每 group 两腿净量的已对冲 / 未对冲部分拆分 |
| `slippage_cost` | `OrderDecision`(`decided_tsc`、inst、side)+ 当时的 `Bbo` + 成交 | 到达价 = `decided_tsc` 时刻该 inst 的中间价 |
| `fill_rate`、拒单分布 | `OrderDecision`、`OrderEvent` | IOC 成交量 / 下单量;按归一化拒单原因计数 |
| `leg_gap_*` | `LatencySample`(实盘)/ 成交与第二腿 `SendReport`(Sim) | p50/p99 |
| `naked_*` | ledger 重建的每 group 两腿差 | 最大值、时间加权均值、超过 `naked_usd` 的秒数 |
| `margin_*` | `MarginReport` + ledger 估算 | 最大保证金率、最小到强平距离 `liq_dist`(口径同下表)、初始保证金占用均值 |
| Sim 专有计数 | sim-gw 写的计数类 `Note` | 直接累加 |

同一函数也用于 `calib_pairs`(正式实盘账户 × 同参数 Sim 账户)的差异表,见 [hft-sim 与正式实盘并行校准](../hft-sim/README.md#与正式实盘并行校准)。

### 策略报告(`strategy_report`)

策略专项的日报项由策略在 SDK `StrategyDesc` 里声明(`notes`:码、名、级别、`v[]` 含义;`report`:字段名、单位、聚合方式、`scope`、来源 Note 码与 `v` 下标;定义归 [hft-strategy-sdk 策略描述符](../hft-strategy-sdk/README.md#策略描述符note报表策略命令)),随 schema JSON 导出;本模块只按这些描述符做通用汇总,watchtower 日报 `strategies` 节与 journal-archiver `strategy_daily` 数据集共用它:

1. `strategy → kind_id`:从 journal 的 `StrategyParams` 头取(同一日内参数更换不改变 `kind_id`)。
2. 每条 `Note{source = Strategy}`:按 (kind_id, code) 查描述符 → 该 (strategy 实例, group) 的该码计数 +1;查不到 → `unregistered` 计数 +1(watchtower 另按 P2 告警)。
3. 每个 `report` 字段:来源码的每条 Note 取 `v[v_ix]` 作 i64 样本(`Count` 不取样本);按 UTC 日、按 `scope`(`Instance` 一行,`Group` 按 `Note.group` 分行)聚合:`Count` = 条数,`Sum` = 和(i128 累加;结果超出 i64 时饱和到 `i64::MAX`/`i64::MIN` 并在 `flags` 置 `saturated`),`Last` = 当日最后一条(按 seq),`Max` = 最大值,`P99` = 当日样本的 hdrhistogram p99(样本 < 100 时取最大值并标记 `low_n`)。
4. 输出 `StrategyDailyRow { date, instance, account, account_kind, strategy, kind_id, group: Option<GroupId>, item: Note | Report | Unregistered, name, code, level, unit, agg, value: i64, samples: u64, flags }`;Sim 账户与 Live 账户分开出行(`account_kind`),不合并。
5. 引擎不解释任何策略的码与字段:新增策略只需要它的 schema 文件进入目录,本 crate 与使用方零改动。

### 告警判定所需的派生量

| 派生量 | 来源 | 用于 |
|---|---|---|
| `advisor_lag_ms`(每 strategy 实例,10 s 窗口最大值) | `Note{source = Advisor, code = AdvisorLag}` 的 `v[1]`(`engine_ns` 差,公式、阈值与写入频率唯一定义在 [hft-event](../hft-event/README.md),本 crate 不另起口径);该 Note 只在超过阈值时写入,窗口内没有 Note 时导出 0(含义为"不超过阈值") | Prometheus gauge、日报;watchtower `AdvisorLag` 直接读该 Note |
| 资金费逐笔差额 | `ReconcileReport{item = Funding}` 的 `exch_amount`、`engine_amount` | watchtower `FundingDiff` |
| 资金费校正计数与金额 | `Note{FundingCorrected}`(source `Ledger`) | 日报 `funding` 节 |
| 断腿自动平仓结果 | `Note{ExecAutoFixDone}`、`Note{ExecAutoFixFailed}` | watchtower `HedgeAutoFix` |
| `sim_tail_lag_ms` | sim-gw 的测量输出(定义归 [hft-sim](../hft-sim/README.md)) | 只记录:Prometheus gauge 与日报 `latency` 节;不进任何规则 |
| 跨所 mark/mid 偏离(每资产) | 同一资产在各 venue 的最新 `Mark`(永续)或 BBO 中间价(现货),每个 venue 相对该资产全部已订阅 venue(含自身)最新值的中位数的偏离(bp)与各自最近更新时刻;只有一个 venue 订阅该资产时不输出 | watchtower `MarkDivergence` |
| 按小时的 `tick_to_trade`、`rep_to_order`、`hedge_host` 直方图与 `sim` 状态 | 按 UTC 小时、按 (venue, 触发 kind) 的直方图(只含 `cal_ok` 的样本);该小时 `sim` 实例的状态(整小时 Running / 整小时停止 / 混合,来自 `HeartbeatBlock` 采样) | 门槛 ⑩ 同机干扰(`interference`,见规则判定函数) |

### 保证金与账户指标

| 指标 | 来源 | 维度 |
|---|---|---|
| `margin_ratio`(gauge,`Rate`)、`margin_balance`、`avail_margin` | 最新 `MarginReport` | account、venue |
| `margin_data_age_ms`(gauge) | 当前时间 − 最近 `MarginReport` 时间 | account、venue |
| `margin_stress_level`(gauge 0–3) | `Note{CapSet/CapCleared}` 中的 `MarginStress` cap 与 `Risk` 事件 | account |
| `liq_dist`(gauge,`Rate`) | 每 (account, venue) 永续持仓的 `min(abs(liq_px − mark) / mark)`;`liq_px` 取 `MarginReport` 中交易所给的值与 [hft-ledger](../hft-ledger/README.md) 估算值中更保守者——口径与 [hft-risk 保证金分档](../hft-risk/README.md#保证金分档marginstress) 相同;档位只读 hft-risk 的 cap,本 crate 不判档 | account、venue |
| `external_fills_total` | `ExternalFill` | account、venue、cause |
| `account_cfg_mismatch`(gauge 位图) | `AccountConfigReport.mismatches` | account、venue |
| `funding_data_age_s`(gauge) | 每 inst 最近 `Funding` 事件的年龄;以及"下次结算时刻已过而未更新"计数 | venue、inst_class |
| `exec_decisions_total` | `ExecDecision` | account、算法、原因 |

## 实现说明

<details><summary>实现要点</summary>

### 直方图

- `hdrhistogram`,3 位有效数字,范围 1 ns–10 s;每 `window_s`(10 s)输出 p50/p90/p99/p99.9/max/count 后清零,另维护一份按日累积(日报与回归基线用)。窗口按 UTC 对齐到 `window_s` 的整数倍;`window(now)` 在 `now` 达到当前窗口的终点时返回该窗口(`start_ns`、`end_ns` 为窗口边界)并开始下一个窗口,否则返回 `None`;样本计入它被算出时所在的窗口。分段值 < 0 → 计入 `negative`、不进直方图;值 = 0 照常记录;任一端点缺失(来源位为缺失或值为 0)→ 该分段为 `None` 并计入 `missing`。
- 维度键 `HistKey { stage, dataset, venue, account, account_kind, inst_class, action, strategy, trigger_kind, trigger_sub, ts_src, other }`;`trigger_sub` 在回报触发时为触发 `OrderEvent.what`(`rep_to_order` 按回报种类分组,latency-budget §5),其余为 0;`inst_class` 由 refdata 给(如 `perp-major`、`perp-alt`、`spot`),**不用 inst 做维度**以控制基数;超过 `max_series` 的新键并入该分段的 `other` 序列(`other = true`,只保留 `stage` 与 `dataset`),每个并入的样本计入计数器 `series_overflow`。`Quantiles` 的 p99 需 n ≥ 1 000、p99.9 需 n ≥ 10 000(latency-budget §5),不足为空;`max` 为精确最大值。
- **不同数据集不混算**:实盘、Sim 账户、验证回放、模拟重演(`replayer sim`)、mock 回环、按事件加权的离线统计各自一个 `dataset` 标签,绝不合并([00 §3](../../design/00-目标与延迟预算.md));HW 与 SW 时间戳分开。
- 回归基线(可选扩展,随 replayer `bench`):同一份 journal 回放得到的 `decide` 分段按 `strategy` 出 p99,与前 7 日中位比较,上涨 > 10% 阻断发布([09 §2](../../design/09-可观测性、风控与控制面.md))。

### 时钟校准序列

- `ClockStatus`(布局归 hft-event,每 1 000 ms 一条,写入节拍归 [hft-core](../../apps/hft-core/README.md))按 seq 顺序进入一个有界队列;一个 `LatencySample` 或行情记录要等它之后的下一条 `ClockStatus` 到达才计算(实时尾随时延迟约 1 s 出数;journal 结束时(`finish`,离线函数读到末尾)最后一段计入 `unsynced_cal`)。插值区间按样本的 `rx_tsc`(无行情/回报触发时按 `sent_tsc`)所在的相邻两条选取。
- 插值与误差界只按 latency-budget §2.1 的公式实现;`calib_err_ns` 取自 `ClockStatus`,`state = Stepped` 的校准对不跨越插值(区间两端之一为 `Stepped` 的样本计入 `unsynced_cal`)。

### 计数器(由事件派生,冷侧计算)

| 计数器 | 来源 | 维度 |
|---|---|---|
| 事件数 | 记录头 `kind` | journal、kind、venue |
| 决策数 / 放行 / Shadow / 拒绝 | `OrderDecision` | strategy、group、`RejectBits` 每位 |
| 目标意图与执行决定 | `ExecDecision` | strategy、group、算法、原因 |
| 成交数、双腿间隔 | `OrderEvent` + `SendReport` + LegLink 注记 | strategy、venue 对 |
| `RateLimitHit`、reject 按码 | `Note`、`OrderEvent.reject_code` | venue、account |
| BBO 合并丢弃 | `Note{Conflated}` | venue |
| Unknown 持续时长分布 | `Note{OmsTransition}` 进入/离开 Unknown | venue、account |
| 连接切换、重同步 | `ConnState` | venue、conn |
| 未知用户流事件 | venue 的计数类 `Note`(可忽略表内外分开) | venue |
| Ctx 溢出、advice 过期 | 策略/sequencer `Note` | strategy |
| `unsynced`、`unsynced_cal`、`missing`、`negative` | 分段计算 | stage、venue、ts_src |
| 链上(可选扩展) | chain journal | endpoint/builder、pool |

### Note 翻译

- `Note` 记录体(布局在 [hft-event](../hft-event/README.md))= `code: u32`、`level`(`NoteLevel`:P0 = 1、P1 = 2、P2 = 3、Info = 4)、`source: u8`、`strategy`、`group`、`inst`、`v: [u64; 8]`。每个拥有者 crate 在自己的 README 定义其码与 `v[]` 含义,并以 `const` 码表导出;`cargo xtask gen-schema` 汇总成 `notes_catalog.rs`,本 crate 只是解释器——翻译表不手抄。
- 策略 Note(`source = Strategy`)的码不进 `notes_catalog.rs`(引擎构建不链接策略包):`note_text` 经 `StrategyCatalog` 按 (kind_id, code) 取描述符的名与 `v[]` 含义;查不到时输出十六进制并计入 `unregistered`。
- `note_text` 输出结构化字段(键值)与一行中文摘要;未知 kind 输出十六进制并计数。
- 告警级别由 watchtower 规则表决定([watchtower 健康规则](../../apps/watchtower/README.md#健康规则)):专门规则可以另定级别(例如外部成交按 `cause` 定级);没有被专门规则消费的引擎 Note 由通用规则 `EngineNote` 按其记录中的 `level` 投递;策略 Note 的级别以其 SDK 描述符 `NoteDesc.level` 为准,由通用规则 `StrategyNote` 投递。

### 规则判定函数

watchtower 的每条规则是本 crate `rules` 模块的一个纯函数 `fn(&RuleInput, &RuleParams) -> RuleVerdict`(输入窗口统计、gauge、事件列表与阈值),watchtower 只装配输入、状态与投递;规则表见 [watchtower 健康规则](../../apps/watchtower/README.md#健康规则)。注册表 `rules::REGISTRY` 按规则名列出类别(窗口型、事件型)与判定函数,watchtower 的装配表与 deploy 的配置模板按它核对。

两条**直通规则**把其他冷进程写下的告警行转给 watchtower 投递,判定只读该行:`archiver_alert` 读 journal-archiver `alerts.jsonl` 的一行(字段归 [journal-archiver](../../apps/journal-archiver/README.md)),规则名 `ArchiverAlert`;`pipeline_alert` 读 pipeline 某实例 `status.json` 中 `alerts` 段的一项(字段归 [pipeline](../../python/pipeline/README.md)),规则名 `PipelineAlert`。两者都是事件型:级别取行内 `level`(P0/P1/P2);行不可解析、缺 `level`/`kind`/`code` 或 `level` 不是这三者 → P1 并置 `malformed`;指纹字段 `ArchiverAlert` 为 (`kind`, prefix, instance, journal, hour),`PipelineAlert` 为 (实例, `code`, `group`)。

门槛 ⑩ 的同机干扰判定(测法、分块、样本量与阈值唯一定义在 [testing §8.2](../../docs/testing.md#82-延迟验收的测法),调用方按它传入参数,本 crate 不保存这些数)也是纯函数:

```rust
pub struct HourStat { pub hour_utc: i64, pub sim: SimState /* On | Off | Mixed */, pub key: HistKey, pub n: u64, pub hist: HistRef }
pub struct InterferenceParams {
    pub block_hours: u8,          // 块长(小时),块按 UTC 对齐
    pub min_n_each_side: u64,     // 硬判定所需的每侧样本数
    pub max_ratio_ub: f64,        // 置信区间上界的通过线
    pub max_halfwidth: f64,       // 区间半宽上限;超过且上界越线 → Extend
    pub conf: f64,                // 置信水平
    pub resamples: u32,           // bootstrap 重抽样次数
    pub seed: u64,                // 重抽样的种子(写进报告,结果可复现);A/A 时也决定贴标签
    pub hard_stages: Vec<Stage>,  // 作硬判定的分段;其余分段只记录
    pub funding_hours_utc: Vec<u8>,  // 资金费块的判定整点(配平用)
    pub aa: bool,                 // A/A:两小时都须 Off,按配平规则贴"开/关"标签
}
pub struct BlockRow { pub block_start_utc: i64, pub funding: bool, pub first_on: bool, pub excluded: bool /* 含 Mixed 小时 */ }
pub struct GroupVerdict { pub key: HistKey, pub blocks: u32, pub n_on: u64, pub n_off: u64,
                          pub ratio: Option<f64>, pub ci: Option<(f64, f64)>, pub halfwidth: Option<f64>,
                          pub verdict: Verdict /* Pass | Fail | Extend | RecordOnly */ }
pub struct InterferenceReport { pub blocks: Vec<BlockRow>, pub balance: [(bool /* funding */, u32 /* 先开 */, u32 /* 先关 */); 2],
                                pub groups: Vec<GroupVerdict>, pub params: InterferenceParams, pub pass: bool, pub extend: bool }
pub fn interference(hours: &[HourStat], p: &InterferenceParams) -> InterferenceReport;
```

- **分块**:按 UTC 对齐切成 `block_hours` 小时一块;含 `funding_hours_utc` 中任一整点的块记 `funding = true`。块内各小时的 `sim` 状态必须一开一关,含 `Mixed` 小时或两小时同状态的块 `excluded`;`first_on` 记下块内顺序(由 `hft-sim-alternate` 按 testing §8.2 的配平规则决定,本函数只读实际状态),`balance` 给出资金费块与普通块各自的先开/先关块数。
- **A/A**(`aa = true`):每块两小时都须为 Off(否则 `excluded`);块内哪一小时记为"开"由 `seed` 按同一配平规则决定;各组照常算 R、区间与半宽,`verdict` 一律 `RecordOnly`,`pass = false`。
- **统计**:每组把未排除块的全部 `On` 小时直方图合并、全部 `Off` 小时直方图合并,`ratio = p99(On) / p99(Off)`;以块为单位有放回重抽样 `resamples` 次(`ChaCha8Rng::seed_from_u64(seed)`),每次按抽中的块重新合并两侧直方图并算比值,区间取重抽样分布的 `(1 − conf)/2` 与 `(1 + conf)/2` 分位。
- **判定**:`hard_stages` 中的分段,两侧 n 都 ≥ `min_n_each_side` 时:上界 ≤ `max_ratio_ub` → `Pass`;上界越线且半宽 `(上界 − 下界)/2` ≤ `max_halfwidth` → `Fail`;上界越线且半宽 > `max_halfwidth` → `Extend`(区间过宽,延长窗口,不判不通过);否则与其余分段一样 `RecordOnly`。任一组 `Fail` → `pass = false`;无 `Fail` 而有 `Extend` → `pass = false`、`extend = true`;没有任何可硬判定的组时 `pass = false` 并在报告中注明。
- 确定性:同一输入与同一 `seed` 结果逐位相同(hdrhistogram 合并与分位数计算不依赖迭代顺序;组按 `HistKey` 排序处理)。

### 规划源文件

| 文件 | 职责 |
|---|---|
| `src/lib.rs` | `Telemetry`、`ingest`、`window`、`day_close` |
| `src/stages.rs` | 时间点 → 分段(含 `rep_to_order`、`hedge_host` 与 `sw_rx` 诊断分段)、按 `trigger_kind` 分类 |
| `src/clock.rs` | `ClockCal`、校准序列、插值与误差界、`unsynced_cal` |
| `src/event_weighted.rs` | `event_weighted`(离线,每条行情记录;含 `kernel_rx` 上界)、`stratum`、`stratified_probe` |
| `src/hist.rs` | 分维度直方图、基数控制 |
| `src/counters.rs`、`src/gauges.rs` | 计数器与 gauge 派生 |
| `src/account_report.rs` | 账户报告累加器与 `AccountDaily`(调用 hft-ledger 重建) |
| `src/strategy_report.rs` | `StrategyCatalog` 加载与摘要校验、策略 Note 计数与 `report` 字段聚合、`StrategyDailyRow` |
| `src/rules.rs` | watchtower 规则判定函数 |
| `src/interference.rs` | 按小时直方图、`HourStat`、分块(含资金费块与配平计数、A/A 贴标签)与 bootstrap 的 `interference` |
| `src/notes.rs` + 生成的 `notes_catalog.rs` | Note 翻译 |
| `src/export.rs` | `WindowReport` → Prometheus 文本 |

</details>

## 延迟与资源契约

无热路径约束(cold,独立进程内运行)。约束的是吞吐与对热进程零影响:

| 项 | 目标 | 基准 |
|---|---|---|
| `ingest` 吞吐 | 单核 ≥ 30 M 记录/分钟(跟上最坏 journal 速率的 10 倍) | `telemetry/ingest` |
| `stages` | ≤ 50 ns/样本(含插值与误差界) | `telemetry/stages` |
| `event_weighted` | 单核 ≥ 30 M 记录/分钟(一天 seq journal 约 3 亿事件 ≤ 15 min,与 [replay-throughput](../../benchmarks/README.md#路径基准与门槛) 同量级) | `telemetry/event_weighted` |
| `interference` | 7 天 × 每天 24 小时、100 组、2,000 次重抽样 ≤ 60 s(单核) | `telemetry/interference` |
| `day_close` | 16 个账户 × 200 group ≤ 1 s | `telemetry/day_close` |
| 内存 | ≈ `max_series` × 每序列字节(每序列两份 hdrhistogram:10 s 窗口与按日累计;3 位有效数字、1 ns–10 s 的单份大小**待实测**,基准 `telemetry/series_bytes`);`max_series` 由使用方按其进程上限([hft-core 内存表](../../apps/hft-core/README.md#内存与-journal-段按主机类唯一归属))折算后给出,超出并入 `other`;校准队列 ≤ 4 条 `ClockStatus` | `telemetry/series_bytes` |

读 journal 只经 `hft-journal` 读者(mmap 只读、`committed_len` Acquire),不与热线程共享可写内存。

## 测试与验收

用例与验收见 [TESTS.md](TESTS.md)。

## 依赖

- 内部:`hft-types`、`hft-event`、`hft-refdata`(inst class、费率、`VenueCaps`)、`hft-journal`(读者)、`hft-ledger`(权益与 PnL 重建,同一记账代码)。
- 外部:`hdrhistogram`(7.x)、`serde`/`serde_json`(报告序列化、策略 schema JSON 解析)、`sha2`(目录摘要)、`rand_chacha`(bootstrap 重抽样,固定种子)。允许 f64(统计输出)。
- 禁止:被任何 `hot`/`pure` crate 依赖;热进程(hft-core)内不链接本 crate;依赖任何 `strategies/*` 或 `hft-strategy-registry`(策略信息只来自 schema JSON 与 journal)。

## 延伸阅读

- [09 §1–3、§6](../../design/09-可观测性、风控与控制面.md)、[00 §3、§6](../../design/00-目标与延迟预算.md)
- [research/06 可观测性](../../design/research/06-data-plane-replay-observability.md)
- [latency-budget](../../docs/latency-budget.md)、[testing §8.2](../../docs/testing.md#82-延迟验收的测法)、[event-model](../../docs/subsystems/event-model.md)、[apps/watchtower](../../apps/watchtower/README.md)、[apps/replayer](../../apps/replayer/README.md)、[hft-sim](../hft-sim/README.md)

## 已知限制与后续工作

- `event_weighted` 依赖 `MarketHdr` 的三个接收时间戳;只记录在 raw journal 而不进 seq journal 的帧(被合并丢弃的 BBO 等)不在统计内,丢弃数见计数器 `Conflated`。
- `inst_class`:refdata 目前只有合约种类(`InstKind`),先按 `Spot`/`Perp` 两类;`perp-major`/`perp-alt` 的划分待 refdata 增加类别字段(触发条件:watchtower 看板需要按大小币分组)。
- `hw_tx` 在 ENA 上不可用;若换到支持 TX 硬件时间戳的网卡,新增时间点按 hft-event 的尾部追加规则扩展 `LatencySample`。
- `pnl_basis` 的拆分按 group 两腿净量,跨 group 共用库存(unassigned 现货)不计入任何 group 的已对冲部分,单列。
- 实时尾随时,含 PHC 端点的分段要等下一条 `ClockStatus` 才出数(约 1 s 延迟);只影响 10 s 窗口的边界样本归属,不影响按日与按小时直方图。

### 开发备忘

<details><summary>给实现者</summary>

- `Note.source` 取值以 hft-event 为准;hft-ledger、hft-exec、hft-sim(sim-gw)的计数类注记码表由各自 crate 导出,本 crate 只重新生成目录。
- **待核实**:ENA PHC 与 TSC 的同步方式(chrony 对 PHC 还是 `phc2sys`)与 `calib_err_ns` 的实际量级,决定 `unsynced_cal` 的占比;方法:M1.5 探针在 `c8g` 上实测(读 `PTP_SYS_OFFSET_EXTENDED` 的窗口宽度分布),见 [research/05](../../design/research/05-hardware-hosting-os-tuning.md)。
- 账户报告依赖 hft-ledger 的权益视图在冷进程可重建(与热进程同一代码、同一 refdata 版本);refdata 变更日按 `ConfigBlob` 切换。
- bootstrap 的 2,000 次重抽样每次都要合并几十个 hdrhistogram;若 `telemetry/interference` 超预算,先把每小时直方图降为固定桶计数数组再合并(同一数据,结果不变)。

</details>
