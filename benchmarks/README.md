---
description: "跨组件延迟门槛:按用户路径组织的基准、参考机定义与回归阈值;共享 CI runner 上只做基准冒烟(数值只报告、不阻断);从 M1.5 起每个里程碑结束时在按需租用的同型参考机上做单 crate 上限与相对回归判定,主机类选定后加路径绝对门槛"
kind: dir
tier: dev
scope: core
status: spec
---

# benchmarks

## 概要

本目录拥有两样东西:**路径基准**(一条用户路径跨多个 crate,如 sequencer 决策段、BBO 在深度突发下的等待、gw 发送段)与**单 crate 基准的上限表**(环跳、journal 追加、解码、簿更新、风控链、OMS 分配、gw 填帧模板)。单 crate 基准的代码与自己的预算留在 `crates/<x>/benches/`,但不得宽于这里的上限。`cargo xtask bench-gate` 读取这里的门槛做裁决。本目录不在任何生产进程里运行;规则见 [AGENTS.md](AGENTS.md)。**门槛只在参考机上判定**:参考机是按需租用、与目标主机类同型的实例,从 M1.5 起每个里程碑结束时运行一次 `bench-gate --reference`(单 crate 上限与相对回归;主机类选定后加路径绝对门槛),不是 CI。上线门槛 ⑩ 的延迟验收是另一件事——在升级主机本身、`prod` 与 `sim` 同机运行时用实时行情 + 测试网/模拟盘下单回路的直方图判定([testing §8](../docs/testing.md#8-上线门槛)),参考机 `bench-gate --reference` 通过是其中一项。共享的 GitHub 托管 runner 上只跑**基准冒烟**——编译,并对改动涉及的基准短跑、报告数值(时间与分配次数),只在基准无法编译或运行出错时失败,不判任何门槛、不做回归阻断。

## 目录

- [使用本组件](#使用本组件)
  - [路径基准与门槛](#路径基准与门槛)
  - [单 crate 基准上限表](#单-crate-基准上限表)
  - [基准名登记](#基准名登记)
  - [参考机与 CI](#参考机与-ci)
  - [回归阈值](#回归阈值)
  - [输出格式](#输出格式)
- [实现说明](#实现说明)
- [延迟与资源契约](#延迟与资源契约)
- [测试与验收](#测试与验收)
- [依赖](#依赖)
- [延伸阅读](#延伸阅读)
- [已知限制与后续工作](#已知限制与后续工作)

## 使用本组件

```sh
cargo xtask bench-gate --smoke               # CI(ubuntu-latest):全部基准编译;改动热 crate 或 benchmarks/ 时短跑涉及的基准;数值只报告
cargo xtask bench-gate --smoke --crate hft-book   # 只跑一个 crate 的冒烟
cargo xtask bench-gate --reference           # 参考机:单 crate 上限 + 相对滚动基线;主机类选定后加路径绝对门槛
cargo xtask bench-gate --reference --path seq-decide   # 参考机上只跑一条路径
cargo xtask bench-gate --reference --syscall-audit     # 端到端路径运行期间 bpftrace 按线程计数系统调用,核对白名单
```

退出码:0 全部通过(冒烟:全部基准能运行;冒烟不读取任何门槛,数值超出预算仍返回 0);1 有门槛被突破,或 `--reference` 时某个已登记的基准在 `budgets.toml` 中没有预算(冒烟:某基准编译失败或运行出错);2 环境不符(不是参考机却要求 `--reference`、`HostProfile` 不匹配、fixture 哈希不符)。`gate.kind = "report"` 的基准超出预算不影响退出码,只在输出 JSON 的 `verdict` 中标出。

### 路径基准与门槛

时间点与分段名见 [latency-budget](../docs/latency-budget.md)。下表的门槛与"额外断言"只在参考机上判定;CI 冒烟只报告这些量:

| 路径目录 | 测量边界(起 → 止) | 驱动与负载 | 门槛(p50 / p99;p99.9 只报告) | 额外断言 |
|---|---|---|---|---|
| `feed-ingest` | 原始 TLS 记录字节就绪 → sequencer 取出(`ingest`) | 内存中的 rustls 客户端/服务端对 + `hft-venue` feed 循环 + `venue-<x>` 解码 + `hft-rt` 环;每 venue × 消息类型一组(bookTicker ≈ 300 B、分档深度快照 TopN、成交、标记价、订单回报;增量深度启用时另加 depth 增量 ≈ 2 KB) | 按 [latency-budget §3](../docs/latency-budget.md#3-cex-路径预算) 的 `ingest` 目标列(bookTicker 行与深度行) | 分配 = 0 |
| `feed-ingest/poll-cadence` | socket 可读(发送端写入时的 TSC)→ 行情线程读完(`rx_tsc`):轮询节拍造成的等待 | 一个 `Spin` 行情线程承载若干本机 TCP socket,数量取 [hft-core 线程计划](../apps/hft-core/README.md#线程计划) 中各主机类每条 `feed-bbo-*` 线程承载的连接数(每种形态一组),一个 socket 按固定间隔到达 bookTicker,其余空闲或按突发到达;分别以就绪探测(`epoll_pwait(0)`,T7)与逐个 `recvmsg` 两种方式轮询;另一组让同线程上另一条 BBO 连接突发,检验每连接每次 poll 至多 `feed.bbo_frames_per_poll` 帧与按帧轮转 | 只报告(含内核的分段,SW 数据集,不与生产 `kernel_rx` 合并);结果供 M3 末探针定档参考 | 每次 poll 至多一次 `epoll_pwait` 且 `timeout = 0`;短读后不再补空 `recvmsg`(计数) |
| `feed-ingest/snapshot` | 行情 REST 响应一次解析:BBO 补帧;增量深度启用时另测 L2 快照(Binance 1000 档 ≈ 50 KB) | `hft-venue` 的 `snap-fetch` 任务(warm 线程)解析 | 只报告;预算归 [hft-venue](../crates/hft-venue/README.md) 的 warm 预算 | 分配 = 0 |
| `seq-decide` | `proc_tsc` → `decided_tsc`(`decide`) | `hft-sequencer` 以回放驱动吃 `fixtures/replay/` 样本;已注册的生产策略与 hft-testkit 测试策略各一套参数 | ≤ 5 / 15 µs | 分配 = 0;与该样本的验证回放意图流一致 |
| `seq/l2burst`(目录 `seq-decide/`) | `Bbo` 入 `bbo` 环 → sequencer 取出(`bbo_wait`);同时报告每条 md 消息的应用时间;另一组报告回报入实盘 rep 环 → sequencer 取出的等待 | **单进程,不依赖 mock**:生产者线程按时间表向 `md` 环注入 md 突发(常速 10×:成交、标记价与分档快照 `L2Snapshot{TopN}`;增量深度启用时另含 `L2Batch` 与按 hft-venue 分片的快照)、向 `bbo` 环注入稳定速率的 `Bbo`;另一组在 `bbo` 环突发(每轮取满批)期间向实盘 rep 环注入回报;`hft-sequencer` 以基准驱动在另一隔离核上运行(md 每轮限额与时间片取 [hft-sequencer](../crates/hft-sequencer/README.md) 的默认值);策略为 hft-testkit 测试策略 | 突发段与平静段的 `bbo_wait` 与回报等待各报告 p50/p99/max | **每个 `Bbo` 样本:`bbo_wait` ≤ 同次运行中单条 md 消息应用时间的最大值 + 环头检查开销上限 + 环跳 p99**;**每条回报:等待 ≤ 同次运行中单条 bbo(或 advice)处理时间的最大值 + 环头检查开销上限 + 环跳 p99**(检查开销预算归 hft-sequencer);journal 中的取出位置与"每处理完一条 md 消息查看 ctl/rep/bbo 环头、每处理完一条 bbo 或 advice 查看 ctl/rep 环头"一致 |
| `seq/bbo_fire`(归 [hft-sequencer](../crates/hft-sequencer/README.md)) | `Bbo` → 策略命中 → evaluate/commit → OMS → 出环 | 冷(缓存被逐出后首次触发)与热(连续触发)两种测法 | 门槛归该 crate README;本目录只保存冷/热差的参考机序列 | 冷/热差是决定是否加回"每 tick 空跑"(可选扩展)的依据([hot-path-rules T5](../docs/hot-path-rules.md#线程t)) |
| `gw-egress` | `decided_tsc` → `sent_tsc`(`egress`) | `OutMsg`(64 B `GwCmd`)入 out 环 + `hft-venue` gw 循环(`venue-<x>` 填帧模板、WS 掩码)+ `hft-net` 真实 TLS 加密 + `writev` 到本机已连接 TCP socket | ≤ 5 / 15 µs | 分配 = 0;对端能解密每一帧(证明未预加密) |
| `gw-egress/idle8` | `OutMsg` 入 out 环 → gw 取出 | gw 循环持有 8 个空闲 socket(主/备下单、REST、UserStream 等,均无数据可读) | p99 ≤ 2 µs(分配:约一次空 `recvmsg` 加一次出环检查) | 验证"每 poll 完一个 socket 就检查一次出环"([hot-path-rules T13](../docs/hot-path-rules.md#线程t));各 socket 的 poll 节拍按 [hft-venue](../crates/hft-venue/README.md) 的 gw 收包规则 |
| `gw-ingest/poll-cadence` | 回报帧所在 socket 可读(发送端写入时的 TSC)→ gw 读完(`rx_tsc`):gw 收包节拍造成的等待(`rep_to_order` 与对冲路径的前段) | 一个 `Spin` gw 循环(`hft-venue` gw + `hft-net`)持有主下单连接、用户流、standby 与 REST 连接(本机 TCP socket,数量取 hft-venue 的 gw 连接上限);回报帧按固定间隔从下单连接或用户流到达,分别落在普通圈与 aux 圈;REST 一组有在途请求、一组没有 | 只报告(含内核的分段,SW 数据集,不与生产 `kernel_rx` 合并);结果供 M1.5 复核 gw 收包方式与 M3 末探针参考 | 无在途请求的 REST 连接只在每 `gw.rest_idle_check_ms` 的存活检查时被 poll、standby 每个 aux 圈 poll 一次(计数;规则归 [hft-venue](../crates/hft-venue/README.md));gw 不调用 `epoll_pwait`([hot-path-rules T7](../docs/hot-path-rules.md#线程t));分配 = 0 |
| `leg-link` | 第一腿 `Fill` 的 `proc_tsc` → 第二腿 `sent_tsc`(`leg_gap`) | sequencer + OMS `LegLink` + gw;第一腿回报由基准驱动注入 rep 环 | `leg_gap` p50 ≤ 20 µs | 第二腿在同一事件内出环;另报告 `sent_tsc − rx_tsc`(第一腿成交帧)作为 `hedge_host` 的主机内 TSC 部分(无 `hw_rx`,不与生产 `hedge_host` 合并) |
| `leg-link/noisy-neighbor` | 同 `leg-link`,只看账户 b | 同一 venue 上账户 a 持续触发限速拒绝、堆满其 out 环配额;账户 b 照常做双腿 | 账户 b 的 `leg_gap` p99 相对无干扰时上涨 < 10% | out 环按 (venue, account) 配额,`RISK_REDUCING` 优先出环([testing §4.1](../docs/testing.md#41-模式账户与人工干预)) |
| `journal-persist` | 写者提交 → 冷副本 `fdatasync` 完成(persist 滞后) | 以生产速率的 3 倍写 RAM 段;速率按目标订阅实算(**待实测**);规模参照 [现状测量 §3.5](../design/现状测量与升级分析-2026-10-01.md):collector 订阅口径下归一化事件约 30 GB/天、原始报文约 160 GB/天 | p99 < 50 ms([hft-journal](../crates/hft-journal/README.md#延迟与资源契约)) | 写者线程无缺页(`perf stat` 的 `page-faults` 在稳态为 0) |
| `startup-cold` | `Sequencer::boot`(`RecoverDriver`,与 hft-core 启动步 5 同一路径)开始 → 恢复完成 | `fixtures/replay/` 的 seq journal 两组:末尾是 Drain 快照、尾部为空;尾部为 `keyframe_max_events`(参数归 hft-sequencer)条事件;单核 | 按 [hft-core 启动顺序](../apps/hft-core/README.md#启动顺序) 步 5 的预算(本表不重抄);进程启动 → 全部闸门通过的整段由运行中的 `Note{StartupStep}` 求和,不在本基准 | 恢复后的状态哈希与 `replayer verify` 在同一位置的哈希一致 |
| `replay-throughput` | 回放一天 seq journal 的墙钟时间 | `replayer` 验证模式,单核 | 一天 seq journal(约 30 GB、约 3 亿事件)≤ 15 min(20–50 M 事件/分钟,[08 §5](../design/08-数据面与回放.md)) | 输出与实盘逐字节一致 |
| `tick-to-trade/cpg`、`tick-to-trade/local`(可选扩展,依赖**冻结**的 `mock-exchange`) | 触发事件 `hw_rx` → `sent_tsc`(`tick_to_trade`;回报触发的下一单计入 `rep_to_order`);同时报告 `kernel_rx`、`ingest`、`decide`、`egress` | `mock-exchange` 按 wire 格式回放 `fixtures/replay/binance-usdm-bookticker-peak` 并对下单帧回执;hft-core 生产二进制以 `Live`(对 mock)运行已注册的生产策略;`cpg` 的 mock 在同 CPG 的另一台实例上,`local` 在同机回环(无 HW 时间戳,属于不同数据集) | `tick_to_trade` 与 `rep_to_order` 的 p50/p99 按 [latency-budget §3](../docs/latency-budget.md#3-cex-路径预算) 的目标列(硬线与签字只属上线门槛 ⑩);`local` 只判 `ingest`、`decide`、`egress` | 预热后分配次数 = 0;`--syscall-audit` 无表外调用;`cpg` 的 HW 时间戳比例 ≥ 99% |

- `seq/l2burst` 是 [latency-budget §2.3](../docs/latency-budget.md#23-bbo-与深度的通道) 的 sequencer 侧验收基准,在 M2 实现;feed 侧(`feed-bbo:<venue>` 与 `feed-md:<venue>` 分线程、BBO 连接之间按帧轮转)由 `feed-ingest` 的 bookTicker 组、`feed-ingest/poll-cadence` 与 hft-venue 的单元测试覆盖。
- `tick-to-trade/*` 测的是**生产二进制**:mock 的额外 CA 只在配置含 `endpoint_override` 时加载,生产配置禁止该项(加载时拒绝)。它们不进 CI 冒烟;mock-exchange 解冻(触发条件见其 README)之前不运行。上线前的延迟验收用升级主机上的直方图判定([testing §8](../docs/testing.md#8-上线门槛) 门槛 ⑩)。`cpg` 与 `local` **不合并**。

### 单 crate 基准上限表

下表是上限:crate 的 `benches/budgets.toml` 可以更紧,不能更宽(`doc-check` 比对)。上限只在参考机上判定。

| 原语 | 归属 crate | 上限 | 出处 |
|---|---|---|---|
| 环跳(push → pop,跨核) | [hft-rt](../crates/hft-rt/README.md) | 进程内单跳(定长元素 `Spsc`):p50 < 100 ns;`ByteSpsc` 1 KiB 记录单跳:p50 ≤ 150 ns | [latency-budget](../docs/latency-budget.md);`ByteSpsc` 一行按 hft-rt |
| TSC 读取 | [hft-rt](../crates/hft-rt/README.md) | ≈ 20 ns | [09 §1](../design/09-可观测性、风控与控制面.md) |
| journal 追加(32–512 B 记录) | [hft-journal](../crates/hft-journal/README.md) | p50 < 200 ns | [08 §2](../design/08-数据面与回放.md) |
| 解码(每条消息,不含 TLS) | [venue-binance](../crates/venue-binance/README.md)、[venue-okx](../crates/venue-okx/README.md)、[venue-bitget](../crates/venue-bitget/README.md) 等 | 按消息类别与分位数分行的统一门槛表归 [hft-venue-proto 延迟与资源契约](../crates/hft-venue-proto/README.md#延迟与资源契约)(本表不重抄;各 venue 的 `budgets.toml` 不得宽于它) | hft-venue-proto |
| 簿更新 | [hft-book](../crates/hft-book/README.md) | p50 < 100 ns/档 | [hft-book](../crates/hft-book/README.md) |
| 风控链(每意图全部检查 + commit) | [hft-risk](../crates/hft-risk/README.md) | 单腿 Open:p50 ≤ 200 ns、p99 ≤ 500 ns;链接 Open(两腿 venue,含第二腿报价健康与可负担性):p50 ≤ 280 ns、p99 ≤ 650 ns | [05 §3](../design/05-CEX执行层.md);链接 Open 一行按 hft-risk 的检查链分配 |
| OMS 分配槽 + client id + `OrderDecision` 构造 | [hft-oms](../crates/hft-oms/README.md) | p50 ≤ 300 ns、p99 ≤ 1 µs(分配:`decide` 5 µs 预算的一部分) | 本表分配 |
| gw 填帧模板(订单类 `GwCmd` → 明文帧) | [hft-venue](../crates/hft-venue/README.md)、`venue-*` | p50 ≤ 300 ns、p99 ≤ 1 µs(分配:`egress` 5 µs 预算的一部分) | 本表分配 |
| L0 池报价(可选扩展,链上) | [hft-chain-evm](../crates/hft-chain-evm/README.md) | < 50 µs | 该 crate README |

### 基准名登记

全仓每个基准名在下表登记一行:Criterion 的 benchmark id 或 `HistBench` 名,等于 `TESTS.md` 中 `bench` 用例的 `基准名`([docs/tests §6](../docs/tests/README.md#6-与代码的对应));名字全仓唯一,删除的名字不复用。数值只写在"预算出处"一处:路径基准在本页 [路径基准与门槛](#路径基准与门槛);单 crate 基准在该组件 README 的"延迟与资源契约",且不得宽于 [单 crate 基准上限表](#单-crate-基准上限表) 中同一原语的上限;本表与 `TESTS.md` 都不写数值。新增、改名或删除基准时在同一改动里改本表与对应用例。

| 基准名 | 组件 | 预算出处 | 用例 |
|---|---|---|---|
| `archiver_convert_seq_1h` | journal-archiver | [journal-archiver 延迟与资源契约](../apps/journal-archiver/README.md#延迟与资源契约) | ARC-155 |
| `bench_acct_decode_bitget` | venue-bitget | [venue-bitget 延迟与资源契约](../crates/venue-bitget/README.md#延迟与资源契约) | BGT-505 |
| `bench_acct_decode_position_risk` | venue-binance | [venue-binance 延迟与资源契约](../crates/venue-binance/README.md#延迟与资源契约) | BIN-508 |
| `bench_classify_bitget` | venue-bitget | [venue-bitget 延迟与资源契约](../crates/venue-bitget/README.md#延迟与资源契约) | BGT-500 |
| `bench_classify_okx` | venue-okx | [venue-okx 延迟与资源契约](../crates/venue-okx/README.md#延迟与资源契约) | OKX-500 |
| `bench_classify_usdm` | venue-binance | [venue-binance 延迟与资源契约](../crates/venue-binance/README.md#延迟与资源契约) | BIN-500 |
| `wire/binance/bbo/peek` | venue-binance | 单次生产 `peek` 调用，HistBench 5,000 次预热 / 50,000 次采样；smoke 不绑核、系统分配器 | BIN-502 |
| `wire/binance/bbo/decode` | venue-binance | 单次生产 `decode` 调用，HistBench 5,000 次预热 / 50,000 次采样；smoke 不绑核、系统分配器 | BIN-502 |
| `wire/binance/spot-bbo/peek` | venue-binance | 单次生产 `peek` 调用，HistBench 5,000 次预热 / 50,000 次采样；smoke 不绑核、系统分配器 | BIN-502 |
| `wire/binance/spot-bbo/decode` | venue-binance | 单次生产 `decode` 调用，HistBench 5,000 次预热 / 50,000 次采样；smoke 不绑核、系统分配器 | BIN-502 |
| `wire/binance/captured-bbo/peek` | venue-binance | 单次生产 `peek` 调用，HistBench 5,000 次预热 / 50,000 次采样；smoke 不绑核、系统分配器 | BIN-502 |
| `wire/binance/captured-bbo/decode` | venue-binance | 单次生产 `decode` 调用，HistBench 5,000 次预热 / 50,000 次采样；smoke 不绑核、系统分配器 | BIN-502 |
| `wire/binance/trade/peek` | venue-binance | 单次生产 `peek` 调用，HistBench 5,000 次预热 / 50,000 次采样；smoke 不绑核、系统分配器 | BIN-502 |
| `wire/binance/trade/decode` | venue-binance | 单次生产 `decode` 调用，HistBench 5,000 次预热 / 50,000 次采样；smoke 不绑核、系统分配器 | BIN-502 |
| `wire/binance/funding-mark/peek` | venue-binance | 单次生产 `peek` 调用，HistBench 5,000 次预热 / 50,000 次采样；smoke 不绑核、系统分配器 | BIN-502 |
| `wire/binance/funding-mark/decode` | venue-binance | 单次生产 `decode` 调用，HistBench 5,000 次预热 / 50,000 次采样；smoke 不绑核、系统分配器 | BIN-502 |
| `wire/binance/depth/peek` | venue-binance | 单次生产 `peek` 调用，HistBench 5,000 次预热 / 50,000 次采样；smoke 不绑核、系统分配器 | BIN-502 |
| `wire/binance/depth/decode` | venue-binance | 单次生产 `decode` 调用，HistBench 5,000 次预热 / 50,000 次采样；smoke 不绑核、系统分配器 | BIN-502 |
| `wire/bitget/bbo/peek` | venue-bitget | 单次生产 `peek` 调用，HistBench 5,000 次预热 / 50,000 次采样；smoke 不绑核、系统分配器 | BGT-501 |
| `wire/bitget/bbo/decode` | venue-bitget | 单次生产 `decode` 调用，HistBench 5,000 次预热 / 50,000 次采样；smoke 不绑核、系统分配器 | BGT-501 |
| `wire/bitget/spot-bbo/peek` | venue-bitget | 单次生产 `peek` 调用，HistBench 5,000 次预热 / 50,000 次采样；smoke 不绑核、系统分配器 | BGT-501 |
| `wire/bitget/spot-bbo/decode` | venue-bitget | 单次生产 `decode` 调用，HistBench 5,000 次预热 / 50,000 次采样；smoke 不绑核、系统分配器 | BGT-501 |
| `wire/bitget/depth/peek` | venue-bitget | 单次生产 `peek` 调用，HistBench 5,000 次预热 / 50,000 次采样；smoke 不绑核、系统分配器 | BGT-501 |
| `wire/bitget/depth/decode` | venue-bitget | 单次生产 `decode` 调用，HistBench 5,000 次预热 / 50,000 次采样；smoke 不绑核、系统分配器 | BGT-501 |
| `wire/bitget/trade/peek` | venue-bitget | 单次生产 `peek` 调用，HistBench 5,000 次预热 / 50,000 次采样；smoke 不绑核、系统分配器 | BGT-501 |
| `wire/bitget/trade/decode` | venue-bitget | 单次生产 `decode` 调用，HistBench 5,000 次预热 / 50,000 次采样；smoke 不绑核、系统分配器 | BGT-501 |
| `wire/bitget/funding-ticker/peek` | venue-bitget | 单次生产 `peek` 调用，HistBench 5,000 次预热 / 50,000 次采样；smoke 不绑核、系统分配器 | BGT-501 |
| `wire/bitget/funding-ticker/decode` | venue-bitget | 单次生产 `decode` 调用，HistBench 5,000 次预热 / 50,000 次采样；smoke 不绑核、系统分配器 | BGT-501 |
| `bench_decode_bitget_books1` | venue-bitget | [venue-bitget 延迟与资源契约](../crates/venue-bitget/README.md#延迟与资源契约) | BGT-501 |
| `bench_decode_bitget_books1/spot` | venue-bitget | [venue-bitget 延迟与资源契约](../crates/venue-bitget/README.md#延迟与资源契约) | BGT-501 |
| `bench_decode_bitget_books15` | venue-bitget | [venue-bitget 延迟与资源契约](../crates/venue-bitget/README.md#延迟与资源契约) | BGT-502 |
| `bench_decode_bitget_order` | venue-bitget | [venue-bitget 延迟与资源契约](../crates/venue-bitget/README.md#延迟与资源契约) | BGT-504 |
| `bench_decode_bookticker` | venue-binance | [venue-binance 延迟与资源契约](../crates/venue-binance/README.md#延迟与资源契约) | BIN-502 |
| `bench_decode_depth20` | venue-binance | [venue-binance 延迟与资源契约](../crates/venue-binance/README.md#延迟与资源契约) | BIN-503 |
| `bench_decode_okx_account` | venue-okx | [venue-okx 延迟与资源契约](../crates/venue-okx/README.md#延迟与资源契约) | OKX-505 |
| `bench_decode_okx_bbo` | venue-okx | [venue-okx 延迟与资源契约](../crates/venue-okx/README.md#延迟与资源契约) | OKX-501 |
| `bench_decode_okx_books5` | venue-okx | [venue-okx 延迟与资源契约](../crates/venue-okx/README.md#延迟与资源契约) | OKX-502 |
| `bench_decode_okx_orders` | venue-okx | [venue-okx 延迟与资源契约](../crates/venue-okx/README.md#延迟与资源契约) | OKX-504 |
| `bench_decode_okx_positions` | venue-okx | [venue-okx 延迟与资源契约](../crates/venue-okx/README.md#延迟与资源契约) | OKX-506 |
| `bench_decode_otu` | venue-binance | [venue-binance 延迟与资源契约](../crates/venue-binance/README.md#延迟与资源契约) | BIN-506 |
| `bench_decode_spot_bookticker` | venue-binance | [venue-binance 延迟与资源契约](../crates/venue-binance/README.md#延迟与资源契约) | BIN-504 |
| `bench_dedup_key` | hft-venue-proto | [hft-venue-proto 延迟与资源契约](../crates/hft-venue-proto/README.md#延迟与资源契约) | VPR-141 |
| `bench_encode_bitget_place` | venue-bitget | [venue-bitget 延迟与资源契约](../crates/venue-bitget/README.md#延迟与资源契约) | BGT-503 |
| `bench_encode_okx_order` | venue-okx | [venue-okx 延迟与资源契约](../crates/venue-okx/README.md#延迟与资源契约) | OKX-503 |
| `bench_encode_place` | venue-binance | [venue-binance 延迟与资源契约](../crates/venue-binance/README.md#延迟与资源契约) | BIN-505 |
| `bench_peek_bookticker` | venue-binance | [venue-binance 延迟与资源契约](../crates/venue-binance/README.md#延迟与资源契约) | BIN-501 |
| `bench_skip_event_usdm` | venue-binance | [venue-binance 延迟与资源契约](../crates/venue-binance/README.md#延迟与资源契约) | BIN-507 |
| `bench_trade_id_ring` | hft-venue-proto | [hft-venue-proto 延迟与资源契约](../crates/hft-venue-proto/README.md#延迟与资源契约) | VPR-140 |
| `book_batch_20` | hft-book | [hft-book 延迟与资源契约](../crates/hft-book/README.md#延迟与资源契约) | BOOK-091 |
| `book_best` | hft-book | [hft-book 延迟与资源契约](../crates/hft-book/README.md#延迟与资源契约) | BOOK-095 |
| `book_delta_best_remove` | hft-book | [hft-book 延迟与资源契约](../crates/hft-book/README.md#延迟与资源契约) | BOOK-092 |
| `book_delta_inwin` | hft-book | [hft-book 延迟与资源契约](../crates/hft-book/README.md#延迟与资源契约) | BOOK-090 |
| `book_delta_ovf` | hft-book | [hft-book 延迟与资源契约](../crates/hft-book/README.md#延迟与资源契约) | BOOK-093 |
| `book_export_200` | hft-book | [hft-book 延迟与资源契约](../crates/hft-book/README.md#延迟与资源契约) | BOOK-101 |
| `book_iter_20` | hft-book | [hft-book 延迟与资源契约](../crates/hft-book/README.md#延迟与资源契约) | BOOK-096 |
| `book_recenter_2048` | hft-book | [hft-book 延迟与资源契约](../crates/hft-book/README.md#延迟与资源契约) | BOOK-100 |
| `book_replay_binance_1h` | hft-book | [hft-book 延迟与资源契约](../crates/hft-book/README.md#延迟与资源契约) | BOOK-102 |
| `book_snapshot_1000` | hft-book | [hft-book 延迟与资源契约](../crates/hft-book/README.md#延迟与资源契约) | BOOK-097 |
| `book_snapshot_sections` | hft-book | [hft-book 延迟与资源契约](../crates/hft-book/README.md#延迟与资源契约) | BOOK-099 |
| `book_snapshot_topn20` | hft-book | [hft-book 延迟与资源契约](../crates/hft-book/README.md#延迟与资源契约) | BOOK-098 |
| `book_top_apply` | hft-book | [hft-book 延迟与资源契约](../crates/hft-book/README.md#延迟与资源契约) | BOOK-094 |
| `compare_day` | replayer | [replayer 延迟与资源契约](../apps/replayer/README.md#延迟与资源契约) | RPL-252 |
| `core/advisor_ckpt_chunk` | hft-core | [hft-core 延迟与资源契约](../apps/hft-core/README.md#延迟与资源契约) | CORE-402 |
| `ctl/get_schema` | hft-ctl | [hft-ctl 延迟与资源契约](../crates/hft-ctl/README.md#延迟与资源契约) | CTL-152 |
| `ctl/ingest` | hft-ctl | [hft-ctl 延迟与资源契约](../crates/hft-ctl/README.md#延迟与资源契约) | CTL-150 |
| `ctl/ingest_setparams_500` | hft-ctl | [hft-ctl 延迟与资源契约](../crates/hft-ctl/README.md#延迟与资源契约) | CTL-151 |
| `ctl/ingest_warmup_4k` | hft-ctl | [hft-ctl 延迟与资源契约](../crates/hft-ctl/README.md#延迟与资源契约) | CTL-153 |
| `ctlproto/normalize_1m` | hft-ctl-proto | [hft-ctl-proto 延迟与资源契约](../crates/hft-ctl-proto/README.md#延迟与资源契约) | CTP-172 |
| `event_cmd_decode` | hft-event | [hft-event 延迟与资源契约](../crates/hft-event/README.md#延迟与资源契约) | EVT-194 |
| `event_crc_64k` | hft-event | [hft-event 延迟与资源契约](../crates/hft-event/README.md#延迟与资源契约) | EVT-193 |
| `event_decode_view` | hft-event | [hft-event 延迟与资源契约](../crates/hft-event/README.md#延迟与资源契约) | EVT-192 |
| `event_encode_80b` | hft-event | [hft-event 延迟与资源契约](../crates/hft-event/README.md#延迟与资源契约) | EVT-190 |
| `event_encode_advice_1k` | hft-event | [hft-event 延迟与资源契约](../crates/hft-event/README.md#延迟与资源契约) | EVT-191 |
| `event_spb1_parse_500` | hft-event | [hft-event 延迟与资源契约](../crates/hft-event/README.md#延迟与资源契约) | EVT-195 |
| `exec/drain_ready_64` | hft-exec | [hft-exec 延迟与资源契约](../crates/hft-exec/README.md#延迟与资源契约) | EXE-489 |
| `exec/force_step_8` | hft-exec | [hft-exec 延迟与资源契约](../crates/hft-exec/README.md#延迟与资源契约) | EXE-490 |
| `exec/force_ungrouped_512` | hft-exec | [hft-exec 延迟与资源契约](../crates/hft-exec/README.md#延迟与资源契约) | EXE-491 |
| `exec/on_external` | hft-exec | [hft-exec 延迟与资源契约](../crates/hft-exec/README.md#延迟与资源契约) | EXE-486 |
| `exec/on_link` | hft-exec | [hft-exec 延迟与资源契约](../crates/hft-exec/README.md#延迟与资源契约) | EXE-485 |
| `exec/on_risk_64` | hft-exec | [hft-exec 延迟与资源契约](../crates/hft-exec/README.md#延迟与资源契约) | EXE-487 |
| `exec/pair_lot` | hft-exec | [hft-exec 延迟与资源契约](../crates/hft-exec/README.md#延迟与资源契约) | EXE-494 |
| `exec/price_cap` | hft-exec | [hft-exec 延迟与资源契约](../crates/hft-exec/README.md#延迟与资源契约) | EXE-492 |
| `exec/recovery_end_1024` | hft-exec | [hft-exec 延迟与资源契约](../crates/hft-exec/README.md#延迟与资源契约) | EXE-493 |
| `exec/resolve_broken` | hft-exec | [hft-exec 延迟与资源契约](../crates/hft-exec/README.md#延迟与资源契约) | EXE-488 |
| `exec/set_target` | hft-exec | [hft-exec 延迟与资源契约](../crates/hft-exec/README.md#延迟与资源契约) | EXE-484 |
| `exec_step/auto_fix` | hft-exec | [hft-exec 延迟与资源契约](../crates/hft-exec/README.md#延迟与资源契约) | EXE-483 |
| `exec_step/hold` | hft-exec | [hft-exec 延迟与资源契约](../crates/hft-exec/README.md#延迟与资源契约) | EXE-481 |
| `exec_step/hold/1024_cells` | hft-exec | [hft-exec 延迟与资源契约](../crates/hft-exec/README.md#延迟与资源契约) | EXE-481 |
| `exec_step/idle` | hft-exec | [hft-exec 延迟与资源契约](../crates/hft-exec/README.md#延迟与资源契约) | EXE-480 |
| `exec_step/slice_pair` | hft-exec | [hft-exec 延迟与资源契约](../crates/hft-exec/README.md#延迟与资源契约) | EXE-482 |
| `feed-ingest` | benchmarks | [路径基准与门槛](#路径基准与门槛) | —(路径基准,本页) |
| `feed-ingest/poll-cadence` | benchmarks | [路径基准与门槛](#路径基准与门槛) | —(路径基准,本页) |
| `feed-ingest/snapshot` | benchmarks | [路径基准与门槛](#路径基准与门槛) | —(路径基准,本页) |
| `gw-egress` | benchmarks | [路径基准与门槛](#路径基准与门槛) | —(路径基准,本页) |
| `gw-egress/idle8` | benchmarks | [路径基准与门槛](#路径基准与门槛) | —(路径基准,本页) |
| `gw-ingest/poll-cadence` | benchmarks | [路径基准与门槛](#路径基准与门槛) | —(路径基准,本页) |
| `hft-cfg/build_strategy_500` | tools | [tools 延迟与资源契约](../tools/README.md#延迟与资源契约) | TOOL-175 |
| `hft-verify/driver_rss` | tools | [tools 延迟与资源契约](../tools/README.md#延迟与资源契约) | TOOL-173 |
| `hostprobe/collect` | tools | [tools 延迟与资源契约](../tools/README.md#延迟与资源契约) | TOOL-174 |
| `journal-persist` | benchmarks | [路径基准与门槛](#路径基准与门槛) | —(路径基准,本页) |
| `journal_append_80b` | hft-journal | [hft-journal 延迟与资源契约](../crates/hft-journal/README.md#延迟与资源契约) | JNL-108 |
| `journal_append_raw_512b` | hft-journal | [hft-journal 延迟与资源契约](../crates/hft-journal/README.md#延迟与资源契约) | JNL-109 |
| `journal_boot_400books` | hft-journal | [hft-journal 延迟与资源契约](../crates/hft-journal/README.md#延迟与资源契约) | JNL-112 |
| `journal_end_event` | hft-journal | [hft-journal 延迟与资源契约](../crates/hft-journal/README.md#延迟与资源契约) | JNL-113 |
| `journal_publish` | hft-journal | [hft-journal 延迟与资源契约](../crates/hft-journal/README.md#延迟与资源契约) | JNL-110 |
| `journal_read_scan` | hft-journal | [hft-journal 延迟与资源契约](../crates/hft-journal/README.md#延迟与资源契约) | JNL-115 |
| `journal_recover_1g` | hft-journal | [hft-journal 延迟与资源契约](../crates/hft-journal/README.md#延迟与资源契约) | JNL-116 |
| `journal_retire_tlb` | hft-journal | [hft-journal 延迟与资源契约](../crates/hft-journal/README.md#延迟与资源契约) | JNL-114 |
| `journal_roll_switch` | hft-journal | [hft-journal 延迟与资源契约](../crates/hft-journal/README.md#延迟与资源契约) | JNL-111 |
| `latency_day` | replayer | [replayer 延迟与资源契约](../apps/replayer/README.md#延迟与资源契约) | RPL-253 |
| `ledger/apply_external` | hft-ledger | [hft-ledger 延迟与资源契约](../crates/hft-ledger/README.md#延迟与资源契约) | LED-251 |
| `ledger/apply_fill` | hft-ledger | [hft-ledger 延迟与资源契约](../crates/hft-ledger/README.md#延迟与资源契约) | LED-250 |
| `ledger/apply_funding_push` | hft-ledger | [hft-ledger 延迟与资源契约](../crates/hft-ledger/README.md#延迟与资源契约) | LED-252 |
| `ledger/apply_reconciled_external` | hft-ledger | [hft-ledger 延迟与资源契约](../crates/hft-ledger/README.md#延迟与资源契约) | LED-294 |
| `ledger/validate_reconcile_batch` | hft-ledger | [hft-ledger 延迟与资源契约](../crates/hft-ledger/README.md#延迟与资源契约) | LED-294 |
| `ledger/apply_settled_external` | hft-ledger | [hft-ledger 延迟与资源契约](../crates/hft-ledger/README.md#延迟与资源契约) | LED-251 |
| `ledger/apply_settled_fill` | hft-ledger | [hft-ledger 延迟与资源契约](../crates/hft-ledger/README.md#延迟与资源契约) | LED-250 |
| `ledger/apply_spot_fill` | hft-ledger | [hft-ledger 延迟与资源契约](../crates/hft-ledger/README.md#延迟与资源契约) | LED-300 |
| `ledger/equity` | hft-ledger | [hft-ledger 延迟与资源契约](../crates/hft-ledger/README.md#延迟与资源契约) | LED-257 |
| `ledger/funding_window_sweep` | hft-ledger | [hft-ledger 延迟与资源契约](../crates/hft-ledger/README.md#延迟与资源契约) | LED-254 |
| `ledger/on_mark` | hft-ledger | [hft-ledger 延迟与资源契约](../crates/hft-ledger/README.md#延迟与资源契约) | LED-256 |
| `ledger/on_ref_px` | hft-ledger | [hft-ledger 延迟与资源契约](../crates/hft-ledger/README.md#延迟与资源契约) | LED-255 |
| `ledger/reconcile_funding` | hft-ledger | [hft-ledger 延迟与资源契约](../crates/hft-ledger/README.md#延迟与资源契约) | LED-253 |
| `ledger/replay_day` | hft-ledger | [hft-ledger 延迟与资源契约](../crates/hft-ledger/README.md#延迟与资源契约) | LED-258 |
| `leg-link` | benchmarks | [路径基准与门槛](#路径基准与门槛) | —(路径基准,本页) |
| `leg-link/noisy-neighbor` | benchmarks | [路径基准与门槛](#路径基准与门槛) | —(路径基准,本页) |
| `net_encrypt_300b_mem` | hft-net | [hft-net 延迟与资源契约](../crates/hft-net/README.md#延迟与资源契约) | NET-327 |
| `net_poll_empty` | hft-net | [hft-net 延迟与资源契约](../crates/hft-net/README.md#延迟与资源契约) | NET-324 |
| `net_poll_loopback` | hft-net | [hft-net 延迟与资源契约](../crates/hft-net/README.md#延迟与资源契约) | NET-323 |
| `net_pool_swap` | hft-net | [hft-net 延迟与资源契约](../crates/hft-net/README.md#延迟与资源契约) | NET-330 |
| `net_ready_probe` | hft-net | [hft-net 延迟与资源契约](../crates/hft-net/README.md#延迟与资源契约) | NET-325 |
| `net_rss_pick` | hft-net | [hft-net 延迟与资源契约](../crates/hft-net/README.md#延迟与资源契约) | NET-331 |
| `net_rx_2k_mem` | hft-net | [hft-net 延迟与资源契约](../crates/hft-net/README.md#延迟与资源契约) | NET-321 |
| `net_wire_poll/274/eager` | hft-net | 生产 poll + next_frame；5,000 次预热 / 50,000 次采样；smoke 不绑核、系统分配器；脚本 recv 计数 | NET-320 |
| `net_wire_poll/274/buffered` | hft-net | 生产 poll + next_frame；5,000 次预热 / 50,000 次采样；smoke 不绑核、系统分配器；脚本 recv 计数 | NET-320 |
| `net_wire_poll/2048/eager` | hft-net | 生产 poll + next_frame；5,000 次预热 / 50,000 次采样；smoke 不绑核、系统分配器；脚本 recv 计数 | NET-320 |
| `net_wire_poll/2048/buffered` | hft-net | 生产 poll + next_frame；5,000 次预热 / 50,000 次采样；smoke 不绑核、系统分配器；脚本 recv 计数 | NET-320 |
| `net_wire_handoff/item` | hft-net | SPSC 写入 + 取出 16 个 64 B 元素；5,000 / 50,000 次；smoke 不绑核、系统分配器 | NET-322 |
| `net_wire_handoff/batch` | hft-net | SPSC 写入 + 取出 16 个 64 B 元素；5,000 / 50,000 次；smoke 不绑核、系统分配器 | NET-322 |
| `net_rx_300b_mem` | hft-net | [hft-net 延迟与资源契约](../crates/hft-net/README.md#延迟与资源契约) | NET-320 |
| `net_rx_first_of_64k` | hft-net | [hft-net 延迟与资源契约](../crates/hft-net/README.md#延迟与资源契约) | NET-322 |
| `net_seal_300b` | hft-net | [hft-net 延迟与资源契约](../crates/hft-net/README.md#延迟与资源契约) | NET-326 |
| `net_send_loopback` | hft-net | [hft-net 延迟与资源契约](../crates/hft-net/README.md#延迟与资源契约) | NET-328 |
| `net_warm_300b` | hft-net | [hft-net 延迟与资源契约](../crates/hft-net/README.md#延迟与资源契约) | NET-329 |
| `oms/cancel_resting_8` | hft-oms | [hft-oms 延迟与资源契约](../crates/hft-oms/README.md#延迟与资源契约) | OMS-452 |
| `oms/continue_16` | hft-oms | [hft-oms 延迟与资源契约](../crates/hft-oms/README.md#延迟与资源契约) | OMS-451 |
| `oms/hedge_gate_open_64` | hft-oms | [hft-oms 延迟与资源契约](../crates/hft-oms/README.md#延迟与资源契约) | OMS-450 |
| `oms/hedge_gated` | hft-oms | [hft-oms 延迟与资源契约](../crates/hft-oms/README.md#延迟与资源契约) | OMS-445 |
| `oms/leg_adjust` | hft-oms | [hft-oms 延迟与资源契约](../crates/hft-oms/README.md#延迟与资源契约) | OMS-448 |
| `oms/leglink_onfill` | hft-oms | [hft-oms 延迟与资源契约](../crates/hft-oms/README.md#延迟与资源契约) | OMS-443 |
| `oms/on_event_ack` | hft-oms | [hft-oms 延迟与资源契约](../crates/hft-oms/README.md#延迟与资源契约) | OMS-441 |
| `oms/on_event_fill` | hft-oms | [hft-oms 延迟与资源契约](../crates/hft-oms/README.md#延迟与资源契约) | OMS-442 |
| `oms/place` | hft-oms | [hft-oms 延迟与资源契约](../crates/hft-oms/README.md#延迟与资源契约) | OMS-440 |
| `oms/price_hedge` | hft-oms | [hft-oms 延迟与资源契约](../crates/hft-oms/README.md#延迟与资源契约) | OMS-444 |
| `oms/recovery_end_4096` | hft-oms | [hft-oms 延迟与资源契约](../crates/hft-oms/README.md#延迟与资源契约) | OMS-455 |
| `oms/smoke` | hft-oms | [hft-oms 延迟与资源契约](../crates/hft-oms/README.md#延迟与资源契约) | OMS-453 |
| `oms/sweep_256` | hft-oms | [hft-oms 延迟与资源契约](../crates/hft-oms/README.md#延迟与资源契约) | OMS-449 |
| `oms/tomb_late_fill` | hft-oms | [hft-oms 延迟与资源契约](../crates/hft-oms/README.md#延迟与资源契约) | OMS-447 |
| `oms/unknown_query` | hft-oms | [hft-oms 延迟与资源契约](../crates/hft-oms/README.md#延迟与资源契约) | OMS-446 |
| `oms/reconcile_cash_guard` | hft-oms | [hft-oms 延迟与资源契约](../crates/hft-oms/README.md#延迟与资源契约) | OMS-474 |
| `oms/foreign_cash_seen` | hft-oms | [hft-oms 延迟与资源契约](../crates/hft-oms/README.md#延迟与资源契约) | OMS-474 |
| `oms/external_cash_guard` | hft-oms | [hft-oms 延迟与资源契约](../crates/hft-oms/README.md#延迟与资源契约) | OMS-474 |
| `oms/remember_external_cash` | hft-oms | [hft-oms 延迟与资源契约](../crates/hft-oms/README.md#延迟与资源契约) | OMS-474 |
| `oms/write_off` | hft-oms | [hft-oms 延迟与资源契约](../crates/hft-oms/README.md#延迟与资源契约) | OMS-454 |
| `params/validate` | hft-strategy-sdk | [hft-strategy-sdk 延迟与资源契约](../crates/hft-strategy-sdk/README.md#延迟与资源契约) | SDK-205 |
| `py_events` | hft-py | [hft-py 延迟与资源契约](../crates/hft-py/README.md#延迟与资源契约) | PY-160 |
| `refdata_caps` | hft-refdata | [hft-refdata 延迟与资源契约](../crates/hft-refdata/README.md#延迟与资源契约) | REF-085 |
| `refdata_check_group` | hft-refdata | [hft-refdata 延迟与资源契约](../crates/hft-refdata/README.md#延迟与资源契约) | REF-086 |
| `refdata_inst` | hft-refdata | [hft-refdata 延迟与资源契约](../crates/hft-refdata/README.md#延迟与资源契约) | REF-084 |
| `refdata_load_5000` | hft-refdata | [hft-refdata 延迟与资源契约](../crates/hft-refdata/README.md#延迟与资源契约) | REF-087 |
| `refdata_num_get` | hft-refdata | [hft-refdata 延迟与资源契约](../crates/hft-refdata/README.md#延迟与资源契约) | REF-083 |
| `refdata_symbol_get` | hft-refdata | [hft-refdata 延迟与资源契约](../crates/hft-refdata/README.md#延迟与资源契约) | REF-081 |
| `refdata_symbol_get_miss` | hft-refdata | [hft-refdata 延迟与资源契约](../crates/hft-refdata/README.md#延迟与资源契约) | REF-082 |
| `replay-throughput` | replayer | [路径基准与门槛](#路径基准与门槛) | RPL-250 |
| `risk/check_pass_hedge` | hft-risk | [hft-risk 延迟与资源契约](../crates/hft-risk/README.md#延迟与资源契约) | RSK-254 |
| `risk/check_pass_linked_open` | hft-risk | [hft-risk 延迟与资源契约](../crates/hft-risk/README.md#延迟与资源契约) | RSK-252 |
| `risk/check_pass_open` | hft-risk | [hft-risk 延迟与资源契约](../crates/hft-risk/README.md#延迟与资源契约) | RSK-251 |
| `risk/check_reject_kill` | hft-risk | [hft-risk 延迟与资源契约](../crates/hft-risk/README.md#延迟与资源契约) | RSK-253 |
| `risk/classify` | hft-risk | [hft-risk 延迟与资源契约](../crates/hft-risk/README.md#延迟与资源契约) | RSK-250 |
| `risk/effective` | hft-risk | [hft-risk 延迟与资源契约](../crates/hft-risk/README.md#延迟与资源契约) | RSK-256 |
| `risk/hedge_px_limits` | hft-risk | [hft-risk 延迟与资源契约](../crates/hft-risk/README.md#延迟与资源契约) | RSK-260 |
| `risk/leverage` | hft-risk | [hft-risk 延迟与资源契约](../crates/hft-risk/README.md#延迟与资源契约) | RSK-261 |
| `risk/mode_apply` | hft-risk | [hft-risk 延迟与资源契约](../crates/hft-risk/README.md#延迟与资源契约) | RSK-255 |
| `risk/on_account_cfg` | hft-risk | [hft-risk 延迟与资源契约](../crates/hft-risk/README.md#延迟与资源契约) | RSK-263 |
| `risk/on_legs` | hft-risk | [hft-risk 延迟与资源契约](../crates/hft-risk/README.md#延迟与资源契约) | RSK-258 |
| `risk/on_margin` | hft-risk | [hft-risk 延迟与资源契约](../crates/hft-risk/README.md#延迟与资源契约) | RSK-259 |
| `risk/on_pnl` | hft-risk | [hft-risk 延迟与资源契约](../crates/hft-risk/README.md#延迟与资源契约) | RSK-257 |
| `risk/view_for` | hft-risk | [hft-risk 延迟与资源契约](../crates/hft-risk/README.md#延迟与资源契约) | RSK-262 |
| `rt_bytes_hop_1k` | hft-rt | [hft-rt 延迟与资源契约](../crates/hft-rt/README.md#延迟与资源契约) | RT-154 |
| `rt_heartbeat` | hft-rt | [hft-rt 延迟与资源契约](../crates/hft-rt/README.md#延迟与资源契约) | RT-160 |
| `rt_killword_load` | hft-rt | [hft-rt 延迟与资源契约](../crates/hft-rt/README.md#延迟与资源契约) | RT-156 |
| `rt_mailbox` | hft-rt | [hft-rt 延迟与资源契约](../crates/hft-rt/README.md#延迟与资源契约) | RT-158 |
| `rt_pause_cost` | hft-rt | [hft-rt 延迟与资源契约](../crates/hft-rt/README.md#延迟与资源契约) | RT-161 |
| `rt_sample_pair` | hft-rt | [hft-rt 延迟与资源契约](../crates/hft-rt/README.md#延迟与资源契约) | RT-162 |
| `rt_seqcell` | hft-rt | [hft-rt 延迟与资源契约](../crates/hft-rt/README.md#延迟与资源契约) | RT-159 |
| `rt_shm_spsc_hop` | hft-rt | [hft-rt 延迟与资源契约](../crates/hft-rt/README.md#延迟与资源契约) | RT-153 |
| `rt_spsc_hop` | hft-rt | [hft-rt 延迟与资源契约](../crates/hft-rt/README.md#延迟与资源契约) | RT-150 |
| `rt_spsc_push_pop` | hft-rt | [hft-rt 延迟与资源契约](../crates/hft-rt/README.md#延迟与资源契约) | RT-151 |
| `rt_spsc_throughput` | hft-rt | [hft-rt 延迟与资源契约](../crates/hft-rt/README.md#延迟与资源契约) | RT-152 |
| `rt_taskloop_idle_round` | hft-rt | [hft-rt 延迟与资源契约](../crates/hft-rt/README.md#延迟与资源契约) | RT-163 |
| `rt_tsc_delta` | hft-rt | [hft-rt 延迟与资源契约](../crates/hft-rt/README.md#延迟与资源契约) | RT-157 |
| `rt_tsc_now` | hft-rt | [hft-rt 延迟与资源契约](../crates/hft-rt/README.md#延迟与资源契约) | RT-155 |
| `rt_utc_clock` | hft-rt | [hft-rt 延迟与资源契约](../crates/hft-rt/README.md#延迟与资源契约) | RT-172 |
| `rt_unpark_latency` | hft-rt | [hft-rt 延迟与资源契约](../crates/hft-rt/README.md#延迟与资源契约) | RT-165 |
| `rt_wake_check` | hft-rt | [hft-rt 延迟与资源契约](../crates/hft-rt/README.md#延迟与资源契约) | RT-164 |
| `sdk/body_view` | hft-strategy-sdk | [hft-strategy-sdk 延迟与资源契约](../crates/hft-strategy-sdk/README.md#延迟与资源契约) | SDK-204 |
| `sdk/cmd_parse` | hft-strategy-sdk | [hft-strategy-sdk 延迟与资源契约](../crates/hft-strategy-sdk/README.md#延迟与资源契约) | SDK-200 |
| `sdk/ctx_place` | hft-strategy-sdk | [hft-strategy-sdk 延迟与资源契约](../crates/hft-strategy-sdk/README.md#延迟与资源契约) | SDK-196 |
| `sdk/ctx_target` | hft-strategy-sdk | [hft-strategy-sdk 延迟与资源契约](../crates/hft-strategy-sdk/README.md#延迟与资源契约) | SDK-197 |
| `sdk/params_decode` | hft-strategy-sdk | [hft-strategy-sdk 延迟与资源契约](../crates/hft-strategy-sdk/README.md#延迟与资源契约) | SDK-203 |
| `sdk/payload_decode` | hft-strategy-sdk | [hft-strategy-sdk 延迟与资源契约](../crates/hft-strategy-sdk/README.md#延迟与资源契约) | SDK-199 |
| `sdk/quote_live` | hft-strategy-sdk | [hft-strategy-sdk 延迟与资源契约](../crates/hft-strategy-sdk/README.md#延迟与资源契约) | SDK-201 |
| `sdk/set_dispatch` | hft-strategy-sdk | [hft-strategy-sdk 延迟与资源契约](../crates/hft-strategy-sdk/README.md#延迟与资源契约) | SDK-198 |
| `sdk/time_conv` | hft-strategy-sdk | [hft-strategy-sdk 延迟与资源契约](../crates/hft-strategy-sdk/README.md#延迟与资源契约) | SDK-202 |
| `seq/advice_route` | hft-sequencer | [hft-sequencer 延迟与资源契约](../crates/hft-sequencer/README.md#延迟与资源契约) | SEQ-330 |
| `seq/bbo_exec_fire` | hft-sequencer | [hft-sequencer 延迟与资源契约](../crates/hft-sequencer/README.md#延迟与资源契约) | SEQ-327 |
| `seq/bbo_fire` | hft-sequencer | [hft-sequencer 延迟与资源契约](../crates/hft-sequencer/README.md#延迟与资源契约) | SEQ-326 |
| `seq/bbo_hit` | hft-sequencer | [内部阶段诊断](../crates/hft-sequencer/README.md#延迟与资源契约),与 `seq/bbo_fire` 同夹具 | SEQ-326 |
| `seq/bbo_miss_ram` | hft-sequencer | [内部阶段诊断](../crates/hft-sequencer/README.md#延迟与资源契约),真实 SPSC/RAM 写者 | SEQ-324 |
| `seq/bbo_miss` | hft-sequencer | [hft-sequencer 延迟与资源契约](../crates/hft-sequencer/README.md#延迟与资源契约) | SEQ-324 |
| `seq/bbo_target_idle` | hft-sequencer | [hft-sequencer 延迟与资源契约](../crates/hft-sequencer/README.md#延迟与资源契约) | SEQ-325 |
| `seq/bbo_view` | hft-sequencer | [hft-sequencer 延迟与资源契约](../crates/hft-sequencer/README.md#延迟与资源契约) | SEQ-322 |
| `seq/conn_alive` | hft-sequencer | [hft-sequencer 延迟与资源契约](../crates/hft-sequencer/README.md#延迟与资源契约) | SEQ-320 |
| `seq/conn_down` | hft-sequencer | [hft-sequencer 延迟与资源契约](../crates/hft-sequencer/README.md#延迟与资源契约) | SEQ-321 |
| `seq/external_fill` | hft-sequencer | [hft-sequencer 延迟与资源契约](../crates/hft-sequencer/README.md#延迟与资源契约) | SEQ-332 |
| `seq/fill_to_hedge` | hft-sequencer | [hft-sequencer 延迟与资源契约](../crates/hft-sequencer/README.md#延迟与资源契约) | SEQ-329 |
| `seq/head_check` | hft-sequencer | [hft-sequencer 延迟与资源契约](../crates/hft-sequencer/README.md#延迟与资源契约) | SEQ-318 |
| `seq/inject_warmup` | hft-sequencer | [hft-sequencer 延迟与资源契约](../crates/hft-sequencer/README.md#延迟与资源契约) | SEQ-335 |
| `seq/l2batch_20` | hft-sequencer | [hft-sequencer 延迟与资源契约](../crates/hft-sequencer/README.md#延迟与资源契约) | SEQ-328 |
| `seq/l2burst` | hft-sequencer | [路径基准与门槛](#路径基准与门槛) | SEQ-319 |
| `seq/margin_report` | hft-sequencer | [hft-sequencer 延迟与资源契约](../crates/hft-sequencer/README.md#延迟与资源契约) | SEQ-331 |
| `seq/mirror_throughput` | hft-sequencer | [hft-sequencer 延迟与资源契约](../crates/hft-sequencer/README.md#延迟与资源契约) | SEQ-339 |
| `seq/notify_continue` | hft-sequencer | [hft-sequencer 延迟与资源契约](../crates/hft-sequencer/README.md#延迟与资源契约) | SEQ-323 |
| `seq/overhead_per_event` | hft-sequencer | [hft-sequencer 延迟与资源契约](../crates/hft-sequencer/README.md#延迟与资源契约) | SEQ-317 |
| `seq/recon_funding_item` | hft-sequencer | [hft-sequencer 延迟与资源契约](../crates/hft-sequencer/README.md#延迟与资源契约) | SEQ-333 |
| `seq/recon_group` | hft-sequencer | [hft-sequencer 延迟与资源契约](../crates/hft-sequencer/README.md#延迟与资源契约) | SEQ-334 |
| `seq/replay_throughput` | hft-sequencer | [hft-sequencer 延迟与资源契约](../crates/hft-sequencer/README.md#延迟与资源契约) | SEQ-338 |
| `seq/snapshot_keyframe` | hft-sequencer | [hft-sequencer 延迟与资源契约](../crates/hft-sequencer/README.md#延迟与资源契约) | SEQ-337 |
| `seq/strategy_cmd` | hft-sequencer | [hft-sequencer 延迟与资源契约](../crates/hft-sequencer/README.md#延迟与资源契约) | SEQ-336 |
| `seq-decide` | benchmarks | [路径基准与门槛](#路径基准与门槛) | —(路径基准,本页) |
| `sim/on_l2` | hft-sim | [hft-sim 延迟与资源契约](../crates/hft-sim/README.md#延迟与资源契约) | SIM-210 |
| `sim/place_match` | hft-sim | [hft-sim 延迟与资源契约](../crates/hft-sim/README.md#延迟与资源契约) | SIM-211 |
| `sim/poll_once` | hft-sim | [hft-sim 延迟与资源契约](../crates/hft-sim/README.md#延迟与资源契约) | SIM-212 |
| `sim/replay_day` | hft-sim | [hft-sim 延迟与资源契约](../crates/hft-sim/README.md#延迟与资源契约) | SIM-213 |
| `startup-cold` | hft-core | [路径基准与门槛](#路径基准与门槛) | CORE-267 |
| `strategies/portfolio-carry/hold` | portfolio-carry | [portfolio-carry 延迟与资源契约](../strategies/portfolio-carry/README.md#延迟与资源契约) | CARRY-009 |
| `strategies/binary-maker/exec_update` | binary-maker | [binary-maker 延迟与资源契约](../strategies/binary-maker/README.md#延迟与资源契约) | —(策略延迟验收) |
| `strategies/binary-maker/risk_event` | binary-maker | [binary-maker 延迟与资源契约](../strategies/binary-maker/README.md#延迟与资源契约) | —(策略延迟验收) |
| `strategies/binary-maker/tick_fire` | binary-maker | [binary-maker 延迟与资源契约](../strategies/binary-maker/README.md#延迟与资源契约) | —(策略延迟验收) |
| `strategies/binary-maker/tick_miss` | binary-maker | [binary-maker 延迟与资源契约](../strategies/binary-maker/README.md#延迟与资源契约) | —(策略延迟验收) |
| `strategies/book-observer/exec_update` | book-observer | [book-observer 延迟与资源契约](../strategies/book-observer/README.md#延迟与资源契约) | —(策略延迟验收) |
| `strategies/book-observer/risk_event` | book-observer | [book-observer 延迟与资源契约](../strategies/book-observer/README.md#延迟与资源契约) | —(策略延迟验收) |
| `strategies/book-observer/tick_fire` | book-observer | [book-observer 延迟与资源契约](../strategies/book-observer/README.md#延迟与资源契约) | —(策略延迟验收) |
| `strategies/book-observer/tick_miss` | book-observer | [book-observer 延迟与资源契约](../strategies/book-observer/README.md#延迟与资源契约) | —(策略延迟验收) |
| `strategies/cross-taker/exec_update` | cross-taker | [cross-taker 延迟与资源契约](../strategies/cross-taker/README.md#延迟与资源契约) | —(策略延迟验收) |
| `strategies/cross-taker/risk_event` | cross-taker | [cross-taker 延迟与资源契约](../strategies/cross-taker/README.md#延迟与资源契约) | —(策略延迟验收) |
| `strategies/cross-taker/tick_fire` | cross-taker | [cross-taker 延迟与资源契约](../strategies/cross-taker/README.md#延迟与资源契约) | —(策略延迟验收) |
| `strategies/cross-taker/tick_miss` | cross-taker | [cross-taker 延迟与资源契约](../strategies/cross-taker/README.md#延迟与资源契约) | —(策略延迟验收) |
| `strategies/latency-probe/ack_cancel` | latency-probe | [latency-probe 延迟与资源契约](../strategies/latency-probe/README.md#延迟与资源契约) | LPR-074 |
| `strategies/latency-probe/advice` | latency-probe | [latency-probe 延迟与资源契约](../strategies/latency-probe/README.md#延迟与资源契约) | LPR-083 |
| `strategies/latency-probe/exec_update` | latency-probe | [latency-probe 延迟与资源契约](../strategies/latency-probe/README.md#延迟与资源契约) | LPR-076 |
| `strategies/latency-probe/risk_event` | latency-probe | [latency-probe 延迟与资源契约](../strategies/latency-probe/README.md#延迟与资源契约) | LPR-077 |
| `strategies/latency-probe/slow_update` | latency-probe | [latency-probe 延迟与资源契约](../strategies/latency-probe/README.md#延迟与资源契约) | LPR-082 |
| `strategies/latency-probe/target_set` | latency-probe | [latency-probe 延迟与资源契约](../strategies/latency-probe/README.md#延迟与资源契约) | LPR-075 |
| `strategies/latency-probe/tick_fire` | latency-probe | [latency-probe 延迟与资源契约](../strategies/latency-probe/README.md#延迟与资源契约) | LPR-073 |
| `strategies/latency-probe/tick_miss` | latency-probe | [latency-probe 延迟与资源契约](../strategies/latency-probe/README.md#延迟与资源契约) | LPR-072 |
| `stratlib/confirm` | hft-strat-lib | [hft-strat-lib 延迟与资源契约](../crates/hft-strat-lib/README.md#延迟与资源契约) | SLB-140 |
| `stratlib/cost_read` | hft-strat-lib | [hft-strat-lib 延迟与资源契约](../crates/hft-strat-lib/README.md#延迟与资源契约) | SLB-137 |
| `stratlib/dirty` | hft-strat-lib | [hft-strat-lib 延迟与资源契约](../crates/hft-strat-lib/README.md#延迟与资源契约) | SLB-141 |
| `stratlib/expiring` | hft-strat-lib | [hft-strat-lib 延迟与资源契约](../crates/hft-strat-lib/README.md#延迟与资源契约) | SLB-146 |
| `stratlib/funding_observe` | hft-strat-lib | [hft-strat-lib 延迟与资源契约](../crates/hft-strat-lib/README.md#延迟与资源契约) | SLB-144 |
| `stratlib/funding_summary` | hft-strat-lib | [hft-strat-lib 延迟与资源契约](../crates/hft-strat-lib/README.md#延迟与资源契约) | SLB-145 |
| `stratlib/leadlag_estimate` | hft-strat-lib | [hft-strat-lib 延迟与资源契约](../crates/hft-strat-lib/README.md#延迟与资源契约) | SLB-152 |
| `stratlib/leadlag_observe` | hft-strat-lib | [hft-strat-lib 延迟与资源契约](../crates/hft-strat-lib/README.md#延迟与资源契约) | SLB-151 |
| `stratlib/pct_observe` | hft-strat-lib | [hft-strat-lib 延迟与资源契约](../crates/hft-strat-lib/README.md#延迟与资源契约) | SLB-147 |
| `stratlib/pct_prefill_chunk` | hft-strat-lib | [hft-strat-lib 延迟与资源契约](../crates/hft-strat-lib/README.md#延迟与资源契约) | SLB-148 |
| `stratlib/pct_query` | hft-strat-lib | [hft-strat-lib 延迟与资源契约](../crates/hft-strat-lib/README.md#延迟与资源契约) | SLB-149 |
| `stratlib/pct_query_small` | hft-strat-lib | [hft-strat-lib 延迟与资源契约](../crates/hft-strat-lib/README.md#延迟与资源契约) | SLB-150 |
| `stratlib/spread` | hft-strat-lib | [hft-strat-lib 延迟与资源契约](../crates/hft-strat-lib/README.md#延迟与资源契约) | SLB-136 |
| `stratlib/throttle` | hft-strat-lib | [hft-strat-lib 延迟与资源契约](../crates/hft-strat-lib/README.md#延迟与资源契约) | SLB-142 |
| `stratlib/trigger_hit` | hft-strat-lib | [hft-strat-lib 延迟与资源契约](../crates/hft-strat-lib/README.md#延迟与资源契约) | SLB-139 |
| `stratlib/trigger_update` | hft-strat-lib | [hft-strat-lib 延迟与资源契约](../crates/hft-strat-lib/README.md#延迟与资源契约) | SLB-138 |
| `stratlib/vwap20` | hft-strat-lib | [hft-strat-lib 延迟与资源契约](../crates/hft-strat-lib/README.md#延迟与资源契约) | SLB-143 |
| `telemetry/day_close` | hft-telemetry | [hft-telemetry 延迟与资源契约](../crates/hft-telemetry/README.md#延迟与资源契约) | TEL-089 |
| `telemetry/event_weighted` | hft-telemetry | [hft-telemetry 延迟与资源契约](../crates/hft-telemetry/README.md#延迟与资源契约) | TEL-087 |
| `telemetry/ingest` | hft-telemetry | [hft-telemetry 延迟与资源契约](../crates/hft-telemetry/README.md#延迟与资源契约) | TEL-085 |
| `telemetry/interference` | hft-telemetry | [hft-telemetry 延迟与资源契约](../crates/hft-telemetry/README.md#延迟与资源契约) | TEL-088 |
| `telemetry/series_bytes` | hft-telemetry | [hft-telemetry 延迟与资源契约](../crates/hft-telemetry/README.md#延迟与资源契约) | TEL-090 |
| `telemetry/stages` | hft-telemetry | [hft-telemetry 延迟与资源契约](../crates/hft-telemetry/README.md#延迟与资源契约) | TEL-086 |
| `testkit/dst_step` | hft-testkit | [hft-testkit 延迟与资源契约](../crates/hft-testkit/README.md#延迟与资源契约) | TK-092 |
| `testkit/golden_all` | hft-testkit | [hft-testkit 延迟与资源契约](../crates/hft-testkit/README.md#延迟与资源契约) | TK-093 |
| `tick-to-trade/cpg` | benchmarks | [路径基准与门槛](#路径基准与门槛) | —(路径基准,本页) |
| `tick-to-trade/local` | benchmarks | [路径基准与门槛](#路径基准与门槛) | —(路径基准,本页) |
| `types/clid_decode` | hft-types | [hft-types 延迟与资源契约](../crates/hft-types/README.md#延迟与资源契约) | TYP-178 |
| `types/clid_encode` | hft-types | [hft-types 延迟与资源契约](../crates/hft-types/README.md#延迟与资源契约) | TYP-177 |
| `types_cap_cross_px` | hft-types | [hft-types 延迟与资源契约](../crates/hft-types/README.md#延迟与资源契约) | TYP-179 |
| `types_mul_qty` | hft-types | [hft-types 延迟与资源契约](../crates/hft-types/README.md#延迟与资源契约) | TYP-174 |
| `types_parse_px` | hft-types | [hft-types 延迟与资源契约](../crates/hft-types/README.md#延迟与资源契约) | TYP-170 |
| `types_parse_qty` | hft-types | [hft-types 延迟与资源契约](../crates/hft-types/README.md#延迟与资源契约) | TYP-171 |
| `types_parse_sci` | hft-types | [hft-types 延迟与资源契约](../crates/hft-types/README.md#延迟与资源契约) | TYP-172 |
| `types_round` | hft-types | [hft-types 延迟与资源契约](../crates/hft-types/README.md#延迟与资源契约) | TYP-175 |
| `types_u256` | hft-types | [hft-types 延迟与资源契约](../crates/hft-types/README.md#延迟与资源契约) | TYP-176 |
| `types_write_fixed` | hft-types | [hft-types 延迟与资源契约](../crates/hft-types/README.md#延迟与资源契约) | TYP-173 |
| `venue_acct_fetch_50k` | hft-venue | [hft-venue 延迟与资源契约](../crates/hft-venue/README.md#延迟与资源契约) | VEN-365 |
| `venue_bbo_live_tick` | hft-venue | [hft-venue 延迟与资源契约](../crates/hft-venue/README.md#延迟与资源契约) | VEN-360 |
| `venue_feed_bbo_path` | hft-venue | [hft-venue 延迟与资源契约](../crates/hft-venue/README.md#延迟与资源契约) | VEN-350 |
| `venue_feed_dup_drop` | hft-venue | [hft-venue 延迟与资源契约](../crates/hft-venue/README.md#延迟与资源契约) | VEN-351 |
| `venue_feed_l2_path` | hft-venue | [hft-venue 延迟与资源契约](../crates/hft-venue/README.md#延迟与资源契约) | VEN-107(可选扩展:全量增量深度) |
| `venue_feed_snap_apply_1000` | hft-venue | [hft-venue 延迟与资源契约](../crates/hft-venue/README.md#延迟与资源契约) | VEN-108(可选扩展:全量增量深度) |
| `venue_feed_topn_path` | hft-venue | [hft-venue 延迟与资源契约](../crates/hft-venue/README.md#延迟与资源契约) | VEN-352 |
| `venue_gw_acct_apply_16k` | hft-venue | [hft-venue 延迟与资源契约](../crates/hft-venue/README.md#延迟与资源契约) | VEN-364 |
| `venue_gw_drain_during_burst` | hft-venue | [hft-venue 延迟与资源契约](../crates/hft-venue/README.md#延迟与资源契约) | VEN-357 |
| `venue_gw_external_fill` | hft-venue | [hft-venue 延迟与资源契约](../crates/hft-venue/README.md#延迟与资源契约) | VEN-361 |
| `venue_gw_idle_8sock` | hft-venue | [hft-venue 延迟与资源契约](../crates/hft-venue/README.md#延迟与资源契约) | VEN-369 |
| `venue_gw_margin_push` | hft-venue | [hft-venue 延迟与资源契约](../crates/hft-venue/README.md#延迟与资源契约) | VEN-363 |
| `venue_gw_place_loopback` | hft-venue | [hft-venue 延迟与资源契约](../crates/hft-venue/README.md#延迟与资源契约) | VEN-354 |
| `venue_gw_place_mem` | hft-venue | [hft-venue 延迟与资源契约](../crates/hft-venue/README.md#延迟与资源契约) | VEN-355 |
| `venue_gw_report_path` | hft-venue | [hft-venue 延迟与资源契约](../crates/hft-venue/README.md#延迟与资源契约) | VEN-356 |
| `venue_gw_skip_event` | hft-venue | [hft-venue 延迟与资源契约](../crates/hft-venue/README.md#延迟与资源契约) | VEN-362 |
| `venue_idle_loop` | hft-venue | [hft-venue 延迟与资源契约](../crates/hft-venue/README.md#延迟与资源契约) | VEN-368 |
| `venue_idle_loop_bbo` | hft-venue | [hft-venue 延迟与资源契约](../crates/hft-venue/README.md#延迟与资源契约) | VEN-366 |
| `venue_idle_loop_bbo_ready` | hft-venue | [hft-venue 延迟与资源契约](../crates/hft-venue/README.md#延迟与资源契约) | VEN-367 |
| `venue_idtable` | hft-venue | [hft-venue 延迟与资源契约](../crates/hft-venue/README.md#延迟与资源契约) | VEN-358 |
| `venue_kill_admit` | hft-venue | [hft-venue 延迟与资源契约](../crates/hft-venue/README.md#延迟与资源契约) | VEN-359 |
| `venue_raw_append` | hft-venue | [hft-venue 延迟与资源契约](../crates/hft-venue/README.md#延迟与资源契约) | VEN-353 |
| `venue_snap_fetch_1000` | hft-venue | [hft-venue 延迟与资源契约](../crates/hft-venue/README.md#延迟与资源契约) | VEN-109(可选扩展:全量增量深度) |
| `xtask/decoupling_gate` | tools | [tools 延迟与资源契约](../tools/README.md#延迟与资源契约) | TOOL-176 |

### 参考机与 CI

| | 参考机 | CI(共享托管 runner) |
|---|---|---|
| 硬件 | 按需租用、与目标主机类同型的实例(与生产同 AMI、同调优剖面、同一份 `prod` 线程计划;`HostProfile` 哈希与 `host.class` 记录在 `benchmarks/reference.toml`)。主机类选定之前(M1.5 到 M3)用 M1.5 延迟探针的同型实例(`c8g.4xlarge`);M3 末满订阅探针选定主机类(默认 `c32`;`c16` 的保留条件与线程计划归 [hft-core](../apps/hft-core/README.md#线程计划))之后用选定的类;可选的 `metal` 参考机(c7i.metal-24xl)只在 bake-off 期间。参考机上不运行 `sim` 实例(基线只量 `prod` 的热路径,同机干扰由门槛 ⑩ 测);`tick-to-trade/cpg` 另需同 CPG 的一台 mock 实例(mock 解冻后) | GitHub 托管 `ubuntu-latest`(x86_64,共享、型号与邻居不固定) |
| 运行什么 | `bench-gate --reference`:单 crate 上限、相对回归、分配与系统调用审计;主机类选定后加路径基准的绝对门槛 | `bench-gate --smoke`:以 `ci` 剖面编译全部基准(不做 LTO;`bench` 剖面只用于发布作业产出的基准制品,剖面取值归 [development](../docs/development.md#构建剖面));改动热 crate 或 `benchmarks/` 的 PR 对涉及的基准短跑(热身 1,000 次、采样 10,000 次);改动 `rust-toolchain.toml` 的 PR 在同一作业内对 base 与 head 各短跑一次、报告相对差(hot-path-rules B4);报告时间与分配次数 |
| 二进制从哪来 | 正式版标签作业(`ubuntu-24.04-arm`)随发布另出的基准制品 `hft-bench-<ver>-aarch64.tar.zst`(不进生产归档);里程碑结束时没有正式版的先打一个正式版标签(规则归 [development 参考机](../docs/development.md#参考机));参考机只下载并运行,不编译 | CI 当场编译 |
| 频率 | 从 M1.5 起每个里程碑结束时;M7 起另加每个正式版候选、工具链或 `target-cpu` 变更 | 每个 PR 与 `main` 推送 |
| 阻断 | 里程碑验收;M7 起另阻断发布 | 只有编译失败或运行出错阻断合并;时间与分配数值只写进输出 JSON,**不作门槛、不作基线**。预热后零分配的合并门槛由各 crate 的 `CountingAlloc` 单元测试与 hft-testkit `StrategyHarness` 承担 |

- 参考机不是生产机,也**不是 CI runner**(不注册自托管 runner):按需按小时租用(一次约 1 h,开机与删机是 `[需授权]` 的 AWS 操作,由负责人执行,租用与费用规则归 [development 参考机](../docs/development.md#参考机)),运行前由 [hostprobe](../tools/README.md#hostprobe) 校验剖面,不符即退出码 2;结果上传 S3 `hft-bench/` 前缀,由人或发布流程读取裁决。
- **门槛何时生效**:单 crate 上限与相对回归从 M1.5 的第一次参考机运行起生效(第一次运行建立基线);路径基准的绝对门槛在 M3 末探针选定主机类、[latency-budget §3](../docs/latency-budget.md#3-cex-路径预算) 据探针实测重定之后生效,此前只报告。
- **按主机类的口径**(定义见 [latency-budget §7](../docs/latency-budget.md#7-主机类与验收口径)):门槛只在参考机(目标主机类 / `metal`)上判定;`dev2`(现有 2 核服务器)与 `c4` 不跑 `--reference`(`host.class` 不是参考机的类即退出码 2,与"环境不符"相同)。`dev2` 上只做功能与正确性验证,见 [testing](../docs/testing.md#7-按主机类的验收)。

### 回归阈值

全部只在参考机上判定。

| 类别 | 适用 | 阈值 | 动作 |
|---|---|---|---|
| 纯计算分段 | `seq-decide`、`feed-ingest`、单 crate 上限表中除 journal 与 L0 外的全部 | p99 相对基线上涨 > 10%([09 §2](../design/09-可观测性、风控与控制面.md)) | 里程碑验收不通过;M7 起阻断发布 |
| 绝对门槛 | 单 crate 上限表;主机类选定后的路径表 | 超出即失败 | 同上 |
| 含内核的分段 | `kernel_rx`、`egress`、`tick-to-trade/*`、`journal-persist` | 只判绝对门槛;相对变化报告不阻断 | — |
| 分配 | 所有标"分配 = 0"的基准 | 预热后 > 0 | 同上(CI 冒烟只报告) |
| 系统调用 | `--syscall-audit` | 出现白名单外调用 | 同上 |
| Criterion 中位数 | 单 crate 基准 | 中位数 +5% 且统计显著 | 报告,不阻断 |

基线:参考机最近 7 次通过运行的 p99 中位数,按 `(基准名, HostProfile 哈希)` 存放在 S3 `hft-bench/` 前缀下;`HostProfile` 变化(含主机类从探针同型实例换到选定主机类)即开启新基线序列,旧序列不参与比较。基线不足 7 次时用已有的全部通过运行。CI 冒烟的数值不进基线。

两家 venue 的 `wire` 基准用 HistBench，热身 5,000 次、逐次采样 50,000 次，不绑核；测量公开 peek/decode 调用前至返回。VecSink 在测量前预留容量，清空包含在 decode 中；使用系统分配器，不声明分配数。输入为已登记 capture 与现有测试合成消息，不联网，只报告，不判参考机门槛。

### 输出格式

每个基准写 `target/bench/<path>/<case>.json`:

```text
{ name, path, case, dataset: { host_profile_hash, host_class, git_sha, target_cpu, ts_source: "HW"|"SW", mode: "smoke"|"reference" },
  n, warmup, p50_ns, p90_ns, p99_ns, p999_ns, max_ns, allocs_after_warmup, syscalls_outside_allowlist,
  gate: { kind: "absolute"|"relative"|"smoke"|"report", budget_p50_ns, budget_p99_ns, baseline_p99_ns, verdict } }
```

`mode = "smoke"` 时 `budget_*`、`baseline_p99_ns` 为空,`verdict` 只反映基准能否运行;`gate.kind = "report"` 表示该基准在本阶段只报告(路径绝对门槛生效之前)。

## 实现说明

<details><summary>实现要点</summary>

- **规划的目录**

| 路径 | 职责 |
|---|---|
| `benchmarks/Cargo.toml` | 私有 workspace 成员 `hft-benchmarks`,只在 `[dev-dependencies]` 与 bench 目标里依赖产品 crate |
| `benchmarks/budgets.toml` | 路径门槛、生效阶段(`report` / `absolute`)与冒烟参数(热身/采样次数)(源常量) |
| `benchmarks/reference.toml` | 参考机 `HostProfile` 哈希、实例 ID 标签、mock 实例标签 |
| `benchmarks/feed-ingest/`、`seq-decide/`(含 `l2burst`)、`gw-egress/`、`gw-ingest/`、`leg-link/` | `HistBench` 驱动 |
| `benchmarks/startup-cold/` | 库内调用 `Sequencer::boot`,每组重复运行并逐次计时 |
| `benchmarks/journal-persist/`、`replay-throughput/` | 进程级驱动 |
| `benchmarks/tick-to-trade/` | 可选扩展:场景配置、`mock-exchange` 场景文件、结果解析(从 hft-core 的 seq journal 读 `LatencySample`) |
| `benchmarks/support/` | 至少两条路径共用后才放这里 |

- **HistBench**(在 [`hft-testkit`](../crates/hft-testkit/README.md) 定义):绑到一个隔离核;热身 ≥ 100,000 次(冒烟模式 1,000 次);每次迭代前后各读一次 TSC;记录进预分配的 hdrhistogram(3 位有效数字,1 ns–10 s);同时开启 `CountingAlloc` 计数。冒烟模式不绑核(共享 runner 无隔离核)。
- **`seq/l2burst` 的驱动**:生产者线程与 sequencer 线程各绑一个隔离核;生产者按预生成的时间表(`fixtures/replay/` 的深度样本按 10× 速率重排,外加周期快照分片)写 `md` 环,并以固定间隔写 `Bbo`,每条 `Bbo` 带入环时的 TSC;sequencer 以基准驱动运行真实主循环(与 `LiveDriver` 同一取环顺序),`bbo_wait` = `proc_tsc` − 入环 TSC;每条 md 消息的应用时间由同一运行中 md 记录的取出与处理完成 TSC 得到。只在单进程内,不需要网卡与 mock。
- **端到端结果来源**(`tick-to-trade/*`):不在基准里另测时间,直接读 hft-core 写的 `LatencySample` 与事件时间戳——测的就是生产代码的测量。
- **系统调用审计**:bpftrace 挂 `tracepoint:raw_syscalls:sys_enter`,按 `comm`(线程名)与系统调用号计数(不用 strace,它改变时序);线程名到角色的映射取自被测 hft-core 运行时的 `[[threads]]`(不按名字前缀猜),每个角色的允许调用唯一取自 [hot-path-rules T7](../docs/hot-path-rules.md#线程t) 的允许表(本页不复写);只审计承载热任务的线程,warm 线程不审计。只在参考机上运行。
- **端到端负载的 fixture**:从现有 S3 WAL 抽取一小时 Binance USDT-M bookTicker 峰值窗口,脱敏后作为 `fixtures/replay/binance-usdm-bookticker-peak` 的 S3 指针。
- **基准制品**:正式版标签作业以 `bench` 剖面构建 `hft-benchmarks` 的可执行文件与 `bench-gate` 运行器,打包为 `hft-bench-<ver>-aarch64.tar.zst` + SHA-256(与生产归档分开,不进 `RELEASE.json` 的安装清单);参考机按 SHA-256 校验后运行。

</details>

## 延迟与资源契约

无热路径约束:本目录只在参考机与 CI 上运行。资源约束:CI 冒烟的编译增量 ≤ 1 min、按改动短跑 ≤ 2 min,都计入 PR 的计费分钟目标([testing §6](../docs/testing.md#6-ci-运行矩阵)、[development 免费额度](../docs/development.md#免费额度));参考机一次运行总时长 ≤ 60 min;`seq/l2burst` 每次 ≥ 10 分钟负载(突发段与平静段各 ≥ 10,000 个 `Bbo` 样本);`tick-to-trade/cpg` 单次 ≥ 10 分钟负载,保证 p99.9 有 ≥ 10,000 个样本([latency-budget §5](../docs/latency-budget.md#5-分组与不同数据集不混算) 的样本数规则)。

## 测试与验收

- `bench-gate` 自身的用例在 [tools/TESTS.md](../tools/TESTS.md)(`cargo xtask bench-gate` 一节);各基准的用例见 [基准名登记](#基准名登记) 的"用例"列。
- **里程碑验收**(从 M1.5 起每个里程碑结束时):单 crate 上限表中已实现的 core 组件全部满足;已实现的路径基准相对基线不回归;M2 起含 `seq/l2burst` 的逐样本断言。
- **门槛 ⑩ 的参考机一项**([testing §8](../docs/testing.md#8-上线门槛),正式实盘之前完成):在选定主机类的参考机上 `seq-decide`、`seq/l2burst`、`feed-ingest`、`gw-egress`、`leg-link` 连续 7 次运行通过(可在同一次租用中重复运行);`tick-to-trade/*` 为可选扩展。

## 依赖

- 内部:所有产品 crate(仅 dev 依赖)、`hft-testkit`、`apps/hft-core`、`apps/replayer`(以二进制调用);可选扩展 `apps/mock-exchange`(冻结)。
- 外部:`criterion`、`hdrhistogram`。
- 禁止:产品 crate 依赖 `hft-benchmarks`;基准代码联网;在共享 runner 上以时间数值作门槛或基线。

## 延伸阅读

- [Wire JSON 字段投影测量](reports/2026-10-15-wire-json.md)（Windows 共享主机诊断）

- [Windows 开发机热点测量与前后对照(2026-10-15)](reports/2026-10-15-windows-hotpath.md)(诊断报告,非参考机验收)
- [latency-budget.md](../docs/latency-budget.md)、[testing.md](../docs/testing.md)、[hot-path-rules.md](../docs/hot-path-rules.md)
- [architecture-v2/00](../design/00-目标与延迟预算.md)、[03 §7](../design/03-网络与传输层.md)、[09 §2](../design/09-可观测性、风控与控制面.md)、[10 §5](../design/10-语言分工与代码组织.md)、[11 实施路线图](../design/11-实施路线图.md)(M1.5 延迟探针)、[research/04 §7](../design/research/04-oss-frameworks-and-patterns.md)
- DSH 样例:`architecture-v2/research/_dsh/benchmarks_AGENTS.md`
- [Wire 轮询审查与测量](reports/2026-10-15-wire-io.md)（Windows 共享主机诊断）
- [round8-decisions](../design/_work/round8-decisions.md)(延迟验收与 CI 只用免费托管 runner 的决定)

## 已知限制与后续工作

- M1.5 之前没有参考机运行:M1 期间热路径的性能退化靠代码评审、`hot-check`、单元测试中的零分配断言与 CI 冒烟报告的趋势约束。
- 里程碑之间的回归要到下一个里程碑结束才被参考机判定;M7 之前参考机运行不阻断发布(只阻断里程碑验收)。
- ENA 没有 TX 硬件戳:`tick_to_trade` 的终点是 `sent_tsc`(用户态 `writev` 返回),不含内核 TX 与网卡排队;`kernel_tx` 只在诊断运行中报告。
- 可选的裸金属 + Onload 形态的门槛在立项时加入本页,届时参考机换成对应硬件,旧基线不可比。
- 链上路径(`chain_decide` 等)的基准在 `hft-chain-evm` 的 crate 内;跨主机的链上路径门槛(包含率、`chain_include_seen`)是生产指标,不是基准。

### 开发备忘

<details><summary>给实现者</summary>

- 参考机的费用:`c8g.4xlarge` 东京按需价约 $584/月(约 $0.8/h,来源 ec2.shop 报价),每个里程碑一次约 1 h;选定主机类后按该类价格;可选的 `metal`(c7i.metal-24xl,x86 剖面)约 $5.4/h,只在 bake-off 期间开机。开机与删机步骤写进 ops-log,费用计入 [11 实施路线图](../design/11-实施路线图.md) 的预算。
- 同机回环(`local`)的 `kernel_rx` 走 loopback,不经 ENA,不可与生产比较——报告里显式标 `ts_source: SW`。
- 实现顺序:`seq-decide`(最早可跑,纯回放驱动)→ `startup-cold` → `gw-ingest/poll-cadence`(M1.5 前,gw 收包方式的依据)→ `seq/l2burst`(M2)→ `feed-ingest` → `gw-egress` → `leg-link`;冒烟模式随第一个基准一起做;`tick-to-trade/*` 等 mock-exchange 解冻。

</details>
