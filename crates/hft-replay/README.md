---
description: "研究回放:replayer research 用它把 seq journal 经同一 Sequencer 与同一策略代码回放,被模拟账户的回报由进程内 hft-sim 撮合重新生成,N 个参数变体 × M 个延迟种子,输出 Parquet 与 run.json"
kind: crate
tier: cold
scope: optional
status: impl
---

# hft-replay

## 概要

可选扩展,触发条件:研究需要在录下的 journal 上比较策略参数变体(上线不依赖它)。`hft-replay` 拥有**研究回放**的装配:流式读取 seq journal、从第一个 `StateSnapshot` 启动 [hft-sequencer](../hft-sequencer/README.md)、用 `ResearchDriver` 按事件时间把日志 Input、模拟场所回报与引擎自身的定时器归并成一条输入流、把参数变体作为 `ConfigBlob` + `SetParams` 注入、把决策与模拟成交写成 Parquet。被模拟账户的撮合用 [hft-sim](../hft-sim/README.md) 的同一个 `SimExchange` 与同一套 journal 衔接(`glue` 模块),与 hft-core 的 sim-gw 共用。调用者只有 [replayer](../../apps/replayer/README.md) 的 `research` 子命令。它不写 journal、不读系统时钟(运行耗时的 `wall_s` 只进 `run.json`)、不连交易所;结论只用于研究与参数筛选,上线门槛不变([replay-as-research-tool](../../.agents/notes/proposed/architecture/2026-10-29-replay-as-research-tool.md))。

## 目录

- [使用本组件](#使用本组件)
  - [命令行](#命令行)
  - [驱动](#驱动)
  - [账户屏蔽](#账户屏蔽)
  - [模拟场所](#模拟场所)
  - [参数变体](#参数变体)
  - [确定性](#确定性)
  - [输出](#输出)
  - [parity](#parity)
  - [特征导出](#特征导出)
  - [规则研究工作流](#规则研究工作流)
- [实现说明](#实现说明)
- [延迟与资源契约](#延迟与资源契约)
- [测试与验收](#测试与验收)
- [依赖](#依赖)
- [延伸阅读](#延伸阅读)
- [已知限制与后续工作](#已知限制与后续工作)

## 使用本组件

```rust
pub struct ResearchOpts {
    pub files: Vec<PathBuf>,                 // 一个运行的 seq 段文件(.jrn,链序)
    pub accounts: Option<Vec<AccountId>>,    // 被模拟账户;None = 全部绑定了策略实例的账户;Some(空) = 不模拟
    pub variants: Vec<Variant>,              // 第一个是基线(不改参数)
    pub seeds: Vec<u64>,                     // 每个变体 × 每个种子各跑一次
    pub sim: SimParams,
    pub driver: DriverCfg,
    pub reassembly: usize,                   // 多段记录重组上限(字节)
    pub jobs: usize,                         // 并行运行数(线程;每个运行各自读 journal)
    pub features: Option<FeatureCfg>,        // Some = 另写 features.parquet(见"特征导出")
    pub out: PathBuf,
}
pub fn research<S: StrategySet>(opts: &ResearchOpts) -> Result<serde_json::Value, String>;   // 写出全部表与 run.json,返回 run.json
pub fn run_one<S: StrategySet>(cfg: &RunCfg) -> Result<RunOut, String>;                     // 一个 (变体, 种子)
pub struct Variant { pub name: String, pub set: BTreeMap<String, i64> }                    // 参数名 → schema 原始值
pub struct SimParams { pub latency: LatencyModel, pub maker_fill: MakerFill, pub seed: u64, pub reseed_s: u32,
                       pub consume_decay_ms: u32, pub take_avail_bp: u16 }
pub struct DriverCfg { pub engine_timers: bool, pub per_round: u16 }
```

### 命令行

由 [replayer `research`](../../apps/replayer/README.md#research) 解析;选项与缺省:

| 选项 | 含义 | 缺省 |
|---|---|---|
| `--journal <file\|dir>` | 一个运行的 seq 段(目录递归收集 `.jrn`,按路径排序) | 必填 |
| `--out <dir>` | 输出目录(不存在则创建) | 必填 |
| `--account N` | 被模拟账户(可重复) | 全部绑定实例的账户 |
| `--variants <file.toml>` | `[[variant]] name = "…" set = { 字段 = 原始值, … }` | 无 |
| `--set NAME=RAW[,…]` | 追加一个变体 `set<k>` | 无 |
| `--seeds S[,S…]` | 延迟随机种子 | `1` |
| `--latency ENTRY_US:RESP_US` | `LatencyModel::Fixed` | `40000:0` |
| `--maker-fill queue\|queue-cap\|through` | 挂单成交模型 | `queue` |
| `--calib <file>` | 校准表([Polymarket 场所行为](#polymarket-场所行为校准表与条件延迟)) | 无 |
| `--own` | 隐藏日志里被模拟账户自己的挂单 | 关 |
| `--tombstone` | 撤单先到即墓碑 | 关 |
| `--cancel-latency ENTRY_US:RESP_US` | 撤单自己的延迟 | 账户模型 |
| `--taker-delay BASE_US[:SLOW_US:SLOW_PPM]`、`--taker-resp-us N` | 吃单撮合延迟与回报延迟 | 关 |
| `--fee-binary K` | 二元吃单费系数 | 费率行 |
| `--gtd-ms N` | 挂单有效期 | 0(无) |
| `--fill-resp-us N` | 挂单成交的回报延迟 | 账户模型 |
| `--reseed-s N` | 重播种间隔(UTC 秒,0 = 不重播种) | `300` |
| `--log-timers` | 照读日志里的堆定时器,不由引擎状态触发 | 关 |
| `--jobs N` | 并行运行数 | `1` |
| `--reassembly-bytes N` | 多段记录重组上限 | 128 MiB |
| `--features EVERY_MS` | 另写 `features.parquet`:决策点 + 每 `EVERY_MS` 引擎 UTC 毫秒一行(0 = 只在决策点) | 不写 |

### 驱动

`ResearchDriver` 实现 hft-sequencer 的 `Driver`,模式报告为 `Verify`(启动走非实盘路径:从快照恢复后由驱动继续)。启动:读到第一个 `StateSnapshot` 所在事件结束为止的前缀,`check_snapshot` + `EngineSetup::from_snapshot`,`Sequencer::boot(Journal{AtSeq(快照 seq)})`。每次 `take`:

1. 队列非空 → 交出队首。
2. `engine_timers` 开时,每交出一条之后检查一次:OMS 要继续 → `Timer{owner 3, id 1}`;通知积压 → `Timer{Sys 17}`(与实盘驱动同一编码)。
3. 下一条日志记录的 tsc 为 `T`;堆顶定时器 `deadline ≤ T` 时取 `t = max(deadline, 上次交出的 tsc)`,否则 `t = T`。
4. `SimExchange::advance(t)`,交付 `due ≤ t` 的回报(按 `(due, gen_seq)`),每条成为一个 Input(tsc 取 `max(due, 上次交出的 tsc)`)。
5. 没有回报时:堆顶到期 → 交出该 `Timer`(owner、id、group、deadline 原样);否则读下一条日志记录:Output 跳过;被屏蔽的回报丢弃;`engine_timers` 开时 owner ≠ 1(Sys)的 `Timer` 丢弃(它们是实盘订单流下的堆定时器);其余照交,行情记录先交给 `SimExchange`。
6. 日志读完 → 结束(不再推进到最后一条记录之后)。

`measure()` 返回当前 Input 的 tsc,所以 `decided_tsc` 与模拟下单的决策时刻都是该事件的时刻。`emit` 把被模拟账户的订单类指令交给模拟场所(其 `SendReport{Written}` 立即排队),其他账户的指令忽略(它们的回报留在日志里)。分段:`roll_info` 预读到下一段的第一条记录,取该段段头的校准对;`clock_status` 取该段开头 Meta 中的 `ClockStatus`。所有记录(含插入的 Input)取新的递增 seq;日志 Input 的 `(回放 seq, 日志 seq)` 对照表供决策表的 `trigger_log_seq`。

### 账户屏蔽

被模拟账户(`mask`)的这些日志 Input 不进入状态机:`OrderEvent`、`SendReport`、`ExternalFill`、`PositionReport`、`MarginReport`、`FundingSettle`,以及每个 (账户, venue, 资产) 第一条之后的 `BalanceReport`(第一条用于初始化账本现金,hft-ledger `UNINIT`)。`ConnState`、`AccountConfigReport` 照读(连接与账户配置事实取自实盘)。不在 `mask` 内的账户照读日志回报;`accounts = Some(空)` 且 `--log-timers` 时,回放的输入流与日志的 Input 相同,决策与日志逐条一致(RPL-301)。

### 模拟场所

`ResearchVenue` 为 `mask` 内账户(Live 或 Sim)建一个 `SimExchange`:每个账户 venue 上每个报价资产给 10⁹ 个单位的初始余额(引擎自己的账本与风控决定能否下单,模拟现金不成为约束);`MarginModel` 为固定 0.5 %、1 倍;启动回报丢弃。行情经 `hft_sim::glue::feed_record`。订单类指令 → `SendReport{Written}`(sent_tsc = 决策时刻)+ `on_command`;`SimSettle`/`SimReset`/`SimTransfer` → `on_command`;`Deliver`(Live 账户的结算)→ 与 hft-venue gw 相同形态的 `ExternalFill{Delivery}`(卖出、按兑付价、无费)并 `external_fill` 平掉模拟仓位。交付的回报只保留订单证据(`OrderEvent`、`ExternalFill`、`SendReport`、`FundingSettle`);余额、仓位、保证金与连接回报丢弃(账户真值由引擎账本按成交维护)。

#### Polymarket 场所行为、校准表与条件延迟

`SimParams.venue` 原样交给 `SimExchange::set_venue_model`(规则归 [hft-sim 场所行为选项](../hft-sim/README.md#场所行为选项venuemodel):`QueueCap`、撤单竞速与墓碑、吃单延迟、二元吃单费、GTD、成交回报延迟、条件延迟池、自有挂单叠加层);缺省全部关闭,结果与 M1 的模拟相同(RPL-305.3)。来源:命令行选项,或 `--calib <file>` 的校准表(下表,解析与条件跟踪是 [hft-sim 的共用实现](../hft-sim/README.md#条件跟踪与校准表);选项按出现顺序生效,后者覆盖前者;文件的 SHA-256 写入 `run.json` 的 `calib_file_sha`,全部模拟参数的哈希仍是 `calib_sha`)。

| 校准表字段 | 含义 |
|---|---|
| `maker_fill` | `queue_cap` \| `queue` \| `through` |
| `tombstone`、`cancel_us: [entry, resp]`、`fill_resp_us`、`gtd_ms`、`taker_nofill_reject` | 同名 `VenueModel` 字段 |
| `taker: {base_us, slow_us, slow_ppm, resp_us}` | `TakerDelay` |
| `fee_k` | `FeeModel::BinaryTaker`(0.07 = 7 %) |
| `pools: {"leg\|load\|move\|phase": [秒, …], "leg\|load": …, "leg": …}`、`split_us`(缺省 5 000)、`min_n`(缺省 20) | 条件延迟池;leg `ack`/`place`、`cxl`/`cancel`、`take`/`taker`,其他 leg 忽略(研究侧 `latc.json` 可直接读) |
| `thr: [ms, …]`、`window_s`(缺省 300)、`phase_s`(缺省 5) | 条件的阈值 |

**条件**(只在设了 `pools` 时计算,由 `ResearchVenue` 从 journal 得出,每条订单类指令之前 `set_condition`):`load` = 上一个完整 UTC 分钟(按 `sw_rx_ns` 分秒)里被模拟账户所在 venue 的 L2 批、整簿与成交的 `sw_rx_ns − ex_ts_ns` 每秒 90 分位的中位数超过的 `thr` 个数(还没有完整分钟时为 1);`move` = 参考行情(不在这些 venue 上、第一个出现的 Bbo inst)的买一加卖一在最近 1 s(按记录 tsc)内的变化 ≤ 0.25 bp → 0、≤ 1 bp → 1、否则 2;`phase` = 决策时引擎 UTC 在 `window_s` 内的秒数 < `phase_s` → 0,否则 1。与研究侧 `bm_sim.py` 的差别:`load` 只用已结束的分钟(无前视)。

**`--own`**:被模拟账户在日志里的 `OrderDecision{New}`(未拒)与被屏蔽的 `OrderEvent` 维护日志自有挂单:`Ack` 后隐藏未成交量,成交减少,终态(`Fill`、`CancelAck`、`Reject`、`Expired`)取消隐藏;同一数量在互补代币的对手侧 `1 − px` 上一并隐藏(Polymarket 的互补流动性)。只作用于模拟场所的簿;引擎自己的簿视图(策略看到的)仍含日志里的自有挂单。

#### 校准表的生成(`replayer calib`)

`calib_build::build(&CalibOpts{files, account, reassembly, window_s, phase_s})`:读一个实盘运行的 seq journal,对 `account` 的每条未拒 `OrderDecision{New}` 取到其 `Ack` 的记录头 tsc 差、每条 `OrderDecision{Cancel}`(同一目标的第一条)取到其 `CancelAck` 的差(按段头 `tsc_hz_milli` 换成秒),样本在决策时按上节的规则打条件标签(`load` 用上一完整分钟的值;参考行情是不出现 L2 记录的 venue 上第一个 Bbo inst;`phase` 用段校准对折算的 UTC)。`thr` 取全部分钟值的三分位(分钟不足 3 个时为空,`load` 全为 1)。写出的表含 `pools`(每个样本进 `leg|load|move|phase`、`leg|load`、`leg` 三个键,leg `ack`/`cxl`)、`thr`(ms)、`split_us` 5 000、`min_n` 20、`window_s`、`phase_s` 与 `meta`(来源、账户、seq 范围、样本数、分钟数),`--calib` 直接读。吃单延迟、成交回报延迟与费率不从 journal 估计(需要探针或手工填写)。

每次交出 Input 前驱动记下它的引擎 UTC,`emit` 时交给 `ResearchVenue::on_utc`:重播种区间与 `phase` 都按决策所在 Input 的引擎 UTC。

### 参数变体

一个变体对每个被模拟账户上、schema 含其中至少一个字段的策略实例各发一对 Input(在第一个日志 Input 之前,tsc = 启动快照的 tsc):`ConfigBlob`(`variant_blob`:该实例当前参数 blob 按其 schema 版本解码,改值,按当前 schema 版本重编码,`version + 1`,SHA-256 重算)与 `ControlCmd::SetParams`(issuer `pipeline`、`confirmed = 1`)。字段名在所有被模拟实例的 schema 中都不存在、或值超出 `[min, max]` → 该运行失败;`SetParams` 的结果不是 `Applied` → 失败。

### 确定性

一个运行只依赖 journal、代码与 `(变体, SimParams)`:没有墙钟与线程交互(并行的运行之间不共享状态)。模拟延迟的随机数按 hft-sim 的账户子流,在每个 `reseed_s` 秒的 UTC 区间开始时以 `splitmix64(seed ⊕ splitmix64(区间号) ⊕ 账户)` 重播种(`SimExchange::reseed_account`;判定在命令到达模拟场所时按该事件的引擎 UTC),所以同一区间内各变体从同一状态抽样(common random numbers),分段回放与整段回放在区间边界上对齐。两次相同运行的四张表、计数与 `sim_state_hash` 相同(RPL-300)。

### 输出

每张表的前两列是 `variant`、`seed`;时间列 `utc_ns` 是引擎 UTC(引擎自己的 UTC 线,`Now.utc_ns` 同源)。价、量为定点原始值(指数见 `px_exp`/`qty_exp` 或 refdata)。

| 文件 | 每行 | 列 |
|---|---|---|
| `decisions.parquet` | 一条 `OrderDecision`(全部账户) | `seq`、`trigger_seq`、`trigger_log_seq`(触发 Input 的日志 seq,插入的 Input 为 0)、`tsc`、`utc_ns`、`engine_ns`、`strategy`、`account`、`inst`、`group`、`action`、`side`、`tif`、`oflags`、`px`、`qty`、`px_exp`、`qty_exp`、`slot`、`gen`、`target_slot`、`reject`、`reason`、`intent_no`、`shadow` |
| `fills.parquet` | 一笔模拟成交 | `utc_ns`、`account`、`inst`、`side`、`px`、`qty`、`fee`、`slot`、`gen` |
| `orders.parquet` | 一张被接受的模拟订单 `(account, slot, gen)` | `inst`、`side`、`px`、`qty`、`t_dec`、`t_ack`、`t_cxl_dec`、`t_done`、`first_fill`(UTC ns,0 = 无)、`cum`、`last_event`(`OrderEventKind`)、`reject_code` |
| `windows.parquet` | 每次交付(`ExternalFill{Delivery}`)结束的 (账户, inst) 区间;日志结束时未交付的区间 `end_s = −1` | `end_s`、`bought`、`cost`、`sold`、`proceeds`、`fees`(报价资产单位)、`delivered`、`payout_px`、`pnl` = 交付值 + 卖出 − 买入 − 费、`n_fills` |
| `features.parquet` | 给了 `--features` 时:见[特征导出](#特征导出) | 同特征导出 |
| `run.json` | 运行元数据(也作为每个 Parquet 文件的 `hft.run` 键值元数据) | `version`、`code_sha`(构建时 `HFT_GIT_SHA`)、`journal`、`sim`、`calib_sha`(`SimParams` 的 SHA-256)、`engine_timers`、`features`、`wall_s`、`runs[]`:`variant`、`seed`、`set`、`params_sha`(每个被模拟账户的实例当前参数 blob SHA-256)、`accounts`、`first_seq`、`start_snapshot`、`stats`(驱动计数)、`sim_state_hash` |

### parity

`parity<S>(&ParityOpts)`(`replayer parity`):每个变体一次运行,不模拟任何账户(`accounts = Some(空)`)、照读日志定时器(`engine_timers` 关),所以日志里的真实回报驱动状态;变体作用于全部策略实例。回放重算的 `OrderDecision` 与日志中启动快照之后的 `OrderDecision` 按触发 Input(回放侧经 `trigger_log_seq`)分组,组内按 (strategy、account、inst、action、side、tif、px、qty、reject、reason) 作多重集匹配:两侧都有 → `both`,只在日志 → `logged_only`,只在回放 → `regen_only`(slot、gen、intent_no 不参与:两侧一旦分歧它们就错位)。输出 `parity.parquet`(每条决策一行:`variant`、`seed` = 0、`class` 0/1/2、`trigger_log_seq` 与上列字段)与 `parity.json`(每个变体的计数、`rate` = both / max(日志条数, 回放条数)、`first_diff_trigger`)。基线变体(日志自身参数)与日志一致是代码改动"决策不变"的证据;参数变体的差异是它改变了哪些决策的清单——分歧之后的回报属于日志里的订单,只能当作近似。

```rust
pub struct ParityOpts { pub files: Vec<PathBuf>, pub variants: Vec<Variant>, pub reassembly: usize, pub jobs: usize, pub out: PathBuf }
pub fn parity<S: StrategySet>(opts: &ParityOpts) -> Result<serde_json::Value, String>;
pub struct ParitySummary { pub logged: u64, pub regenerated: u64, pub both: u64, pub logged_only: u64, pub regen_only: u64, pub first_diff_trigger: u64 }
```

`ParityOpts` 另有 `features: Option<FeatureCfg>`(`parity` 不读,`features` 用)。

### 特征导出

`features<S>(&ParityOpts)`(`replayer features`)与 `research --features`:策略自己声明的特征列([hft-strategy-sdk 特征导出](../hft-strategy-sdk/README.md#特征导出),`S::feature_columns` / `S::export_features`)按采样点逐实例写成一行。`features` 的运行与 parity 相同(不模拟、照读日志定时器、变体作用于全部实例),所以基线行就是实盘策略当时读到的值;`research --features` 的行属于模拟运行。

```rust
pub struct FeatureCfg { pub every_ms: u32, pub at_decisions: bool }
pub fn features<S: StrategySet>(opts: &ParityOpts) -> Result<serde_json::Value, String>;   // opts.features 必须是 Some
```

- 采样点:`at_decisions` —— 一个 Input 处理完后,本 Input 上发出过 `OrderDecision` 的每个实例一行(`cause` 0);`every_ms` > 0 —— 引擎 UTC 越过下一个 `every_ms` 整数倍时,每个已创建的实例一行(`cause` 1,`grid_utc_ns` = 越过的格点;一次跳过多个格点只写一行)。第一个格点从运行的第一个 Input 之后开始。导出时每轮只取一个 Input(`per_round = 1`),输入序列与不导出时相同,决策因此不变。
- 列:`variant`、`seed`、`utc_ns`、`engine_ns`(采样时的 `Now`)、`grid_utc_ns`(决策行 0)、`seq`(回放 seq)、`log_seq`(该 Input 的日志 seq,插入的 Input 为 0)、`cause`、`strategy`、`account`(主账户)、`kind_id`,然后是策略集合全部 kind 的特征列按名字合并的并集(同名列的单位与指数不同 → 报错);一个 kind 没有的列为 null,`NA` 也写成 null。特征列是可空 Int64,Arrow 字段元数据带 `unit`(`ratio`/`px`/`qty`/`ns`/`count`/`minute`)与 `exp`;研究侧按 `值 × 10^exp` 换成浮点。没有特征列的 kind 不出行。
- `features.json`(也作为 `features.parquet` 的 `hft.run` 元数据):`version`、`code_sha`、`journal`、`every_ms`、`at_decisions`、`columns[]`(`name`、`unit`、`exp`)、`runs[]`(`variant`、`set`、`params_sha`、`start_snapshot`、`rows`)。不含耗时,同一 journal 两次运行的 Parquet 逐字节相同。

### 规则研究工作流

负责人 2026-10-30 决定:策略规则研究直接在 Rust 上做,不再先在研究侧 Python(`bm_port.py`/`bm_sim.py`)实现再移植。研究里跑的就是将来上线的代码,回放决策与实盘逐字节一致(`replayer verify`/`parity`)。

1. **加规则**:在策略 crate(如 `strategies/binary-maker`)里实现规则,用一个新参数开关,缺省值 = 关(实盘与已有 journal 的决策不变,BMK-034 决策轨迹哈希守住);参数 schema 版本照常递增,`define_params!` 的 `since` 标新版本;规则的单元测试照常写。
2. **构建**:x86_64 `replayer` 由 CI `research-build.yml`(`workflow_dispatch(sha)`)产出,或在 Windows 工作站的 WSL Ubuntu-22.04 交叉构建(`cargo build --release -p replayer`,glibc 2.35 的产物可在研究机运行);研究机上放在固定位置(`~/hft-research/bin/replayer`,同目录 `SHA` 记代码版本)。
3. **跑变体**:变体文件 `[[variant]] name = "…" set = { 新参数 = 原始值, … }`,基线(不改参数)总是第 0 个;在 bundle 6f6f 及以后的实盘 journal 上:
   `replayer research --journal <seq 段目录> --out <dir> --variants v.toml --seeds 1,2,3 --jobs 8 --calib latc.json --own --maker-fill queue-cap --tombstone --fill-resp-us 12000`。
   单个窗口的成交随机性大,结论看多个种子与多份 journal 的平均,不看单次。
4. **分析**:读 `<dir>/windows.parquet`(已交割窗口的盈亏;journal 在交割前结束的窗口 `end_s = −1`,份数改看 `fills.parquet`)、`orders.parquet`、`decisions.parquet`,按 `variant`、`seed` 分组比较;`run.json` 记下代码 SHA、参数 SHA-256 与校准表哈希,结论附上它们。
5. **上线**:规则有效后把缺省值或实盘参数打开,走 PR;不需要再移植一遍。

保真度(2026-10-30 对账,6f6f 的两份实盘 journal,同一延迟池、`--own`、`queue-cap`、墓碑):逐窗盈亏与实盘的相关 Rust 0.71、`bm_sim.py` 0.12,逐窗份数相关 Rust 0.94/0.56、`bm_sim.py` 0.44/0.44,撤单后成交比例实盘 1.7 %、Rust 2.1 %、`bm_sim.py` 2.6 %;每窗 \|Δ\| ≤ 0.3 的窗口 Rust 43 %(设计门槛 100 %,两个模型都未达到);速度约为研究侧流程(解码 + `bm_sim.py`)的 4–5 倍。

## 实现说明

<details><summary>实现要点</summary>

| 文件 | 职责 |
|---|---|
| `src/lib.rs` | `ResearchOpts`、`research`(变体 × 种子的运行、合表、写出) |
| `src/run.rs` | `run_one`:启动、变体注入、步进循环、决策/订单/成交/区间表 |
| `src/driver.rs` | `ResearchDriver`、`ResearchSink`(新 seq、截获 `OrderDecision`/`CmdResult`)、前缀读取 |
| `src/source.rs` | 流式读取(逻辑记录、段校准、预读) |
| `src/venue.rs` | `ResearchVenue`、屏蔽规则、`Deliver`、条件跟踪、`--own` |
| `src/calib.rs` | 校准表(解析在 hft-sim `calib`,这里加文件哈希) |
| `src/calib_build.rs` | 从实盘 journal 生成校准表的延迟池 |
| `src/variants.rs` | 变体文件与 `--set` 解析、`variant_blob` |
| `src/table.rs` | 列表与 Parquet 写出(zstd 3) |
| `src/parity.rs` | 重算决策与日志决策的匹配 |
| `src/features.rs` | 特征导出的采样与列并集 |

- 截获的 Output 在下一次 `take` 时按引擎此刻的 `Now`(即触发它的 Input 处理之后)盖 `utc_ns`/`engine_ns`;模拟回报按其 tsc 用引擎当前的时钟锚换算。
- 被模拟的 Live 账户在引擎里仍是 Live(出环、`Settle` → `Deliver`),只是指令去了模拟场所;Sim 账户的 `SimSettle` 由 `SimExchange` 处理。

</details>

## 延迟与资源契约

冷路径,无热路径约束。内存:前缀日志(到启动快照为止)+ 预读与队列;日志本身流式读取,不整份进内存。吞吐目标与实测见 [TESTS.md](TESTS.md) 的验收一节(重构设计 §5.3 的 ≥ 20× / 单 journal < 1 min 口径待研究机实测)。

## 测试与验收

用例与验收见 [TESTS.md](TESTS.md)。

## 依赖

内部:`hft-types`、`hft-refdata`、`hft-event`、`hft-journal`、`hft-rt`、`hft-sim`(`calib`)、`hft-strategy-sdk`(`schema-json`)、`hft-sequencer`(`replay-io`)。外部:`arrow-array`/`arrow-schema`/`parquet` 59(Parquet 写出,zstd)、`serde`/`serde_json`/`toml`(变体文件与 `run.json`)、`sha2`、`zerocopy`。不被任何引擎 crate 或 hft-core 依赖。

## 延伸阅读

- [replay-as-research-tool](../../.agents/notes/proposed/architecture/2026-10-29-replay-as-research-tool.md)
- [hft-sim journal 衔接](../hft-sim/README.md#journal-衔接glue-模块)
- [重构进度](../../docs/refactor/PROGRESS.md)

## 已知限制与后续工作

- 只读未压缩的 `.jrn` 段(`.jrn.zst` 先 `zstd -d`);一次运行一个 journal 运行(多个运行的目录请分开调用)。
- 返利只在研究报告里另算(不进成交与 `windows.parquet`);`replayer calib` 只生成下单与撤单的延迟池,不含成交回报延迟与吃单延迟。
- `--own` 不改引擎的簿视图:策略在回放里仍看到日志中自己的挂单(与实盘相同,与 `bm_port.py` 的剔除不同)。
- N 个变体各自读 journal、各自重建簿;共享解析与参数无关特征是后续优化(先按 M1 实测决定)。
- 特征导出只有策略自己声明的列(binary-maker 目前 7 列);研究面板里的其余特征要先按设计 §7.1② 固化成节点并加进策略的 `FEATURES`。
- 从非创建处起步的模拟账户没有起点时的模拟挂单(实盘 journal 每次启动从空订单开始,不受影响)。

### 开发备忘

<details><summary>给实现者</summary>

- 设计依据:重构设计 2026-10-29 §4.3.2、§5(仓外 `E:\trade\handoff\hft-refactor-design-2026-10-29.md`);研究侧参照实现 `E:\trade\staging\pm5m\bm_sim.py`。
- 与 `bm_sim.py` 的对账(M1 验收 (b))需要研究机上的实盘 journal 与同参数、同固定延迟的 `bm_sim.py` 输出。

</details>
