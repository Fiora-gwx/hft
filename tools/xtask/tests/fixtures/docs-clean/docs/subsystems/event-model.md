# 事件模型

## 时间戳语义

见 latency-budget。

## kind 编号段

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
| 0x0405 | `ReconcileBatch` | Var 16 + ≤51200 | Input | 完整报告与原始结算凭据 |
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
| 0x050B | `UtcAnchor` | 定长 32 | Input | 引擎 UTC 的重锚 |
| 0x0601 | `RawRx` | Multi | raw journal | 收到的解密后 payload |
| 0x0602 | `RawTx` | Multi | raw journal | 发出的明文订单帧;**登录帧与任何含签名的帧不记录** |
