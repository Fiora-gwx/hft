---
description: "从 seq/raw journal 与有界测量环重建逐事件、逐订单延迟链"
kind: dir
tier: cold
scope: core
status: impl
---

# latency-report

## 概要

`report.py` 只读同一次进程运行的 journal 和测量文件，输出原始点、事件链、订单链、双腿发送偏差、分位数与直方图。它使用发布包内的 `hft_py` 读者，不运行策略、不写交易状态。MD-only、被合并而未进入 sequencer 的行情、每次 PartialFill/Fill 都保留；关联不唯一时报告歧义，不选择一个看似合理的匹配。

## 目录

- [使用本组件](#使用本组件)
- [实现说明](#实现说明)
- [延迟与资源契约](#延迟与资源契约)
- [测试与验收](#测试与验收)
- [依赖](#依赖)
- [延伸阅读](#延伸阅读)
- [已知限制与后续工作](#已知限制与后续工作)

## 使用本组件

在 core 配置的 `journal` 中设置可选的 `latency_trace_capacity`，每线程容量为 256..65536 中的二次幂，所有环合计不超过 32 MiB；省略时不分配环、不读测量时钟。文件位于 `journal_cold/<instance>/latency/trace-<启动计数器>/`。每个工作线程独占一个环；已有 persist 冷线程排空，停止生产者后排完并写 `completion.json`。数据文件用 `create_new`，不同启动不覆盖。此配置不改变策略或风险判定。

用发布包 Python 环境执行 `python tools/latency-report/report.py --trace <trace目录> --journal <seq目录> --out <新的输出目录>`。读取尚未冷落盘的 journal 时给 `--hot <journal_ram>`；每份 raw journal 用一个 `--raw <raw目录>`。正式分析应在干净停止后执行，原始 trace、seq/raw journal 与 RELEASE 元数据一起留存。

输出如下。

| 文件 | 含义 |
|---|---|
| `raw_points.csv` | 每个固定记录一行，含线程、流水号、丢失累计、阶段、原始时间与关联键 |
| `journal_events.csv` | 白名单字段的规范化行情、订单/执行事件与全部 ClockStatus；不导出配置、秘密或 UID |
| `event_chains.csv` | 每个已解码输入一行，MD 无交易也输出；空 input_seq 表示未关联到 sequencer 输入 |
| `order_chains.csv` | 每个决策及其逐次 ack/fill 一行，含 `input_*` 触发链、strategy/risk/OMS、出站戳与交易所订单/成交标识 |
| `leg_skew.csv` | 同一 trigger_seq/group 的 First/Second 对，第二腿减第一腿的出环和发送时间；不把 OnFill 当同时下单 |
| `exchange_times.csv` | 可选 raw 输入的 E/T/ts 等时间叶子，通过 raw seq 映射 `trace_thread/trace_frame`；没有报文内容 |
| `stages.csv`, `histograms.csv` | 每 venue/分段的 n、负间隔数、经验最近秩 p50/p90/p99/p99.9/max 和 log2 桶 |
| `summary.json`, `analysis.sqlite` | 覆盖率、歧义/缺失/负值、时钟来源及可重复查询数据 |

## 实现说明

### 原始格式与关联

版本 1 每记录 120 B，15 个小端 u64；列顺序以随运行保存的 `schema.json` 为准：`tsc, serial, dropped, stage, input_seq, decision_seq, order, frame, event_hash, venue_conn, kind_inst, v0..v3`。`order = gen<<32 | slot`；`venue_conn = conn<<8 | venue`；`kind_inst` 低 16 位为 kind，高 32 位可带 inst。0 表示未提供，不能当有效时间。纳秒有符号字段在文件中保留二补码，CSV 的原始词保持 u64。

`frame` 是该线程 Frame 记录的流水号，同一个 recvmsg 内多个帧仍独立。规范化事件以 FNV-1a 全体字节指纹连接解码、环和 sequencer；指纹重复或匹配不唯一时留空并计数。订单用决策 seq 精确连接出环、gw、编码、加密和发送；seq journal 的 trigger_seq、slot/gen、link_role、reason 保留 Reverse/修复的因果链。`event_hash` 只用于诊断，不参与业务状态。

阶段编号按 `hft_rt::latency_trace::Stage` 声明顺序从 1 开始；只追加。Read 的 v0/v1 是 HW/SW RX ns，v2 是字节数，v3 是 TsFlags。Decrypt 的 v0 是凑齐 TLS record 的 read TSC，v1/v2 是密文字节偏移/长度。Frame 的 v0..v3 是 read TSC、HW RX ns、SW RX ns、最后一个 TLS record 解密完成 TSC。DecodedEvent/FeedPush 的行情值是 ex_ts_ns、ex_send_ns、inst、rx_tsc；OrderEvent 的值是 ex_ts_ns、trade_id、exch_order_id、rx_tsc。Decision 的值是 intent_no、link_role、reason、reject。其他阶段的值是局部关联/结果；非时间值不得当作 TSC。

### 时钟与边界

本机阶段戳使用有序 TSC 读取（aarch64 为 CNTVCT_EL0）；频率和启动校准锚点在 schema，之后的校准锚点与误差见 seq journal 的 ClockStatus。SW RX 是 SO_TIMESTAMPING 的 CLOCK_REALTIME，HW RX 是 PHC；ENA 上通常只有 SW。交易所时间属于交易所时钟，精度和同步误差不能由本机校准推断。raw 时间单位按位数仅作提示，同时保留未换算原值。

Read 在 recvmsg 返回后；解密包围实际 TLS open；Frame 在 WS 交付前；decode 包围协议分派/JSON 规范化；环 push 在成功发布之后，pop 在取出时/转换后。push 的观测可能晚于并行消费者的 pop，负值单列，不截断为零。InputHanded 是合并/调度后交给处理器的边界；process 包围 journal、业务处理、输出与出环；strategy 包围每个真实 on_event；risk 包围单腿或整对 evaluate；OMS 包围实际 place。encode 是订单帧模板写入，加密包围真实 seal，Send 是 sendmsg 返回戳，短写的返回值/请求字节数保留。

TxQueued/TxWrite 的编号为 32/33：`frame` 在这两类记录中表示连接发送实例标识；前者 v0/v1 是密文绝对字节区间，后者 v0/v1/v2 是已消费偏移、带符号返回值、请求字节数。离线按区间匹配包含 flush 的每次写入，输出 `send_complete_tsc` 与次数；不把首次短写当整单发送完成。

RawRecord 的编号为 34，v0..v3 为 raw seq、raw kind、接收/发送 TSC、what；在原始记录成功追加后写入当前 frame 关联。原始交易所时间可精确回连同读多帧；缺失或多匹配明确标记。订单行按 trigger_seq 和 intent_no/inst 连接 OMS、按 strategy 连接回调；同时双腿共享风险准入时 `risk_owner_intent_no` 标识第一腿，独立 OnFill/Reverse 仍用自身准入。多次同策略回调无法唯一连接时留空。

分段是 [latency-budget](../../docs/latency-budget.md) 的诊断细分，不构成新预算。网络往返按同一次进程运行内完整 `(venue, account, slot, gen)` 身份匹配每次实际 ack/fill，不把其他订单表的同号回报归入当前订单；缺少 venue/account 时不关联。CSV 保留 account 与回报 epoch。同时双腿偏差保留正负方向。经验分位数始终带 n；小样本尾部不能作为门槛或回归裁决。

`kernel_to_user` 用样本前后两条 ClockStatus 的 REALTIME 端线性插值；逐样本误差界以同一 REALTIME 域的相邻速率变化计算，不依赖 PHC 是否存在，默认 `--cal-max-err-ns 1000`。缺后一锚点、Stepped 或超界只计 `unsynced_cal`。事件行保留换算值与误差界，负间隔单列。`tick_to_trade.sw` / `rep_to_order.sw` 包含有效的内核软件戳到用户态部分；`.userspace` 明确从 recvmsg 返回开始，不能作为完整主机口径。HW 数据集的预算验收仍用发布包 replayer latency；这里不混合 HW 与 SW。回报 RTT 诊断从 send 返回到用户态收包，不能冒充内核收包口径的 order_rtt。

## 延迟与资源契约

热路径只读计数器并写预分配 SPSC，不做文件 I/O、分配、锁或新增 syscall。测量环默认无 wake；满时丢诊断点并递增独立计数，不等待、不改变订单结果。线程私有 RefCell 仅持有测量上下文和 producer，启动期安装；不提供业务条件分支依据。冷线程每轮每环最多排 4096 条，文件缓冲 64 KiB。最终丢失数即使尾部再无成功记录也在 completion 中保留；缺 completion 不能声称完整。

启用记录会增加内存写、计数器读和调度开销；报告描述启用时的实际系统，不能直接宣称未启用时的性能。对跨 CPU 的 TSC 间隔，应同时保留 HostProfile 和校准误差。

TLS 槽只持有启动期分配的 producer 静态引用，类型经 `needs_drop=false` 断言；首次关闭态访问也不注册 TLS 析构器。每线程最多安装一次，producer 与环保留至进程退出，容量有界；进程停止时先排空冷端，操作系统释放进程资源。

固定工具链的 Clippy 在 `thread_local!` 展开时丢失局部 lint 作用域；hft-rt 的根级宏豁免由 hot-check 独立补检。检查读取统一宏禁用模板，拒绝该 crate 中的禁用宏调用及重命名导入，仅允许测量文件中的 `thread_local!`。TOOL-032 检查普通文件和测量文件中的格式化仍被拒绝。

## 测试与验收

沿用 RT-002/166、NET-125、VEN-199、SEQ-053/054、CORE-012 与 RPL-203 的行为范围，见 [TESTS.md](TESTS.md)。Python 回归用 `python -m unittest discover -s tools/latency-report -p 'test_*.py'`，Rust 测试走对应 crate 的既有 CI。

## 依赖

标准 Python 库与发布包的 `hft_py`；SQLite 用于有界缓存下的离线连接。热记录原语只依赖 hft-rt 的 SPSC 与计数器。

## 延伸阅读

- [隔离测量上下文的决定](../../.agents/notes/implemented/architecture/2026-10-15-latency-chain.md)
- [hft-core](../../apps/hft-core/README.md)、[replayer](../../apps/replayer/README.md)

## 已知限制与后续工作

Connector 当前拒绝 tx_timestamps，kernel TX 戳为空；Send 不能冒充 kernel TX。完整原始帧时间叶子需要同时提供 raw journal；raw 采样/丢失会限制其覆盖率。没有进入已解码事件集的控制帧/重复帧仍留在 raw_points 与 raw 时间表，不伪造 seq。warm REST 的跨线程逐阶段关联尚未覆盖；其规范化 journal 时间仍可导出。分析不把无关联、时钟异常或丢失样本算成零延迟。
