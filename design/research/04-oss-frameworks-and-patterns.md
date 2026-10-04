# 04 · 开源框架、库与架构模式调研

> 面向从零重构的加密低延迟交易系统(跨所套利 / CEX-DEX 套利 / 其他延迟敏感策略)。
> 目标函数:tick-to-trade 的 p50 / p99。语言:热路径 Rust 优先,C/C++ 仅在有明确收益处使用,Python 负责研究 / 配置 / 运维。
> 团队 1–3 人 + AI agents,因此"借用已被验证的设计"权重很高。
>
> 调研日期:2026-09-28。版本号、最后提交日期、star 数均取自 crates.io / GitHub API 当日快照;基准数据全部注明来源与测试条件。
> 标注约定:**[不确定]** = 未能独立核实的说法或个人判断;**[自报]** = 项目作者自己给出的数字,未见第三方复现。
>
> 评分口径(1–5):**生产就绪度**(维护活跃度、测试、真实生产使用);**延迟设计**(分配策略、线程模型、是否为 µs 级尾延迟而设计);**契合度**(对我们的交易所、Rust 热路径、小团队的匹配程度)。

---

## 0. 结论摘要(Top 10)

1. **不整体采用任何一个交易框架作为热路径。** hftbacktest、nautilus_trader、barter-rs 三者都把 tokio 放在网络 I/O 中心,都没有公开的 tick-to-trade 尾延迟数据;其价值在"可借鉴的子系统"而不是"可复用的整机"。热路径应是我们自己写的单写者、固定核心、忙等的流水线(见 §6、§9)。
2. **hftbacktest 是最值得借鉴的回测/仿真设计,但 2026 年维护已停滞。** 最后一次发布 rust-v0.9.4 / py-v2.4.4(2025-12-10),2026 年零提交,12 个 PR 未合并(含一个置顶的部分成交入账 bug #316 修复),issue 创建已被限制。其队列位置模型、双侧延迟模型、ROI 向量订单簿、"同一份策略代码跑回测与实盘"以及用 iceoryx2 做 connector↔bot IPC 的架构,应作为设计参考并 fork 关键代码,而非作为依赖。
3. **nautilus_trader v2(Rust 核心 + PyO3)是当前最活跃、覆盖最广的 Rust 交易引擎**(29.5k stars,v2.0.0rc4 于 2026-09-02,1.231.0 是最后的 Cython v1 版本),适配器覆盖 Binance/OKX/Bybit/Hyperliquid/dYdX/Coinbase/Kraken/Deribit/Polymarket 等,但**不含 Bitget**,采用 **LGPL-3.0**,没有公开 p50/p99 数字。定位:研究/回测/中低频执行的"第二套系统"候选,以及适配器实现的对照物。热路径不用。
4. **线程间通信直接用 `disruptor`(disruptor-rs,4.4.0,2026-08)或 `rtrb`(0.3.5,2026-08)**,两者都支持忙等 + 核心绑定;跨进程零拷贝用 **iceoryx2**(0.10.0,2026-09-18,同机 IPC 延迟 ~100–250 ns 级,前提是忙等或正确使用 WaitSet)。Aeron 仅在需要跨机可靠组播 / 集群复制时引入,Rust 侧用 GSR 维护的 **rusteron**(FFI 包装 C 客户端,生产使用)。
5. **WebSocket 客户端不是瓶颈,TLS + JSON 解析 + 唤醒才是。** 没有任何证据显示 C++ WS 客户端(Beast / IXWebSocket / websocketpp)优于 Rust 实现;uWebSockets 根本不是客户端库。Rust 侧 `tokio-tungstenite ≥0.26.2` 已与 fastwebsockets 基本持平,`yawc`(2026-09-17,MPL-2.0,自称支撑 7×24 交易系统)提供最完整的 permessage-deflate 控制。**热路径推荐:`tungstenite`(同步 API)跑在非阻塞 socket 上、由绑核线程自旋驱动,不引入 async 运行时**;tokio 只留给控制面 / REST / 运维。
6. **TLS 选 rustls + aws-lc-rs**(0.23.45 / 1.18.1)。官方 2025-07 与 2026-Q1 基准显示 x86_64 上普遍优于 OpenSSL 3.5;注意 **ARM64 上 rustls 吞吐略落后**(2024-10 报告),对我们的 Graviton 实例是需要实测的点。kTLS 对小报文 feed 无延迟收益,不用。
7. **JSON 解析选 `sonic-rs`**(0.5.10,2026-09;ByteDance,直接反序列化到结构体、arena DOM、支持惰性取字段),反序列化比 simd-json 快 1.5–2×、比 serde_json 快 3–4×[自报,但多方复现];更根本的是**按交易所消息布局定制解析,避免 `serde_json::Value`**。
8. **价格/数量用 `#[repr(transparent)]` 的 i64 定点数(scale 编译期固定)**,而不是 `rust_decimal`。基准显示定点 add/sub ≈ 0.31 ns、曲线平坦;rust_decimal 是"带每实例 scale 的十进制浮点",分支多、不可预测。现成可用的是 `primitive_fixed_point_decimal`(1.5.0,2026-08),也可自写 200 行。
9. **订单簿自己写:按 tick 索引的稠密数组 + 缓存 BBO**。这是 Optiver David Gross(CppCon 2024)与 hftbacktest ROI 向量簿的共同结论:node-based 容器必败,sorted vector / 稠密数组胜出。现成 crate(orderbook-rs、ninjabook、lob-orderbook)都不面向"接收交易所增量、维护本地镜像"这一场景。
10. **架构模式采用 LMAX / Jane Street 一脉:单写者、顺序化事件日志、确定性状态机、可重放。** 具体:feed handler → SPSC ring → 单线程策略/风控状态机 → SPSC ring → order gateway;所有输入事件带序号写入 mmap 追加日志(Chronicle Queue 思路,SBE/zerocopy 编码),Python 通过 mmap / Arrow 直接读;测试以"回放日志得到逐字节一致的状态"为金标准。配合 Carl Cook 的 cache warming(每个 tick 都跑完整下单路径、只在真正触发时放行)与 HRT 的显式 huge pages。

---

## 1. 加密交易框架

### 1.1 总览表

| 项目 | 语言 / 许可证 | 活跃度(2026-09 快照) | 架构要点 | 热路径分配 / 线程模型 | 我们所需交易所的覆盖 | 生产就绪 | 延迟设计 | 契合度 |
|---|---|---|---|---|---|---|---|---|
| [hftbacktest](https://github.com/nkaz001/hftbacktest) | Rust 核心 + Python;MIT | 4.8k★;最后发布 2025-12-10;最后推送 2025-12-23;2026 年 0 提交;12 PR 积压 | 回测/实盘同代码;队列位置 + 双侧延迟模型;L2/L3;ROI 向量簿;实盘 = 独立 connector 进程 + iceoryx2 IPC | 回测核心无 GC、数组化;实盘 connector 基于 tokio;下单仍走 REST(WS 下单在 roadmap) | Binance Futures、Binance Spot、Bybit(仅 Rust 实盘);OKX / Hyperliquid 在 roadmap 未完成;无 Bitget | 3(回测)/ 2(实盘,作者自述"未充分测试") | 3 | **4(借鉴)/ 1(依赖)** |
| [nautilus_trader](https://github.com/nautechsystems/nautilus_trader) | Rust 核心 + PyO3;**LGPL-3.0** | 29.5k★;推送 2026-09-28;v2.0.0rc4(2026-09-02);22 核心 crate + 16 适配器 crate | 确定性事件驱动引擎;MessageBus;同一时间模型跑回测与实盘;可全 Rust 编写 | mimalloc + tokio;Criterion/CodSpeed 持续基准但**不公开绝对数字** | binance、bybit、okx、hyperliquid、dydx、coinbase、kraken、deribit、polymarket、lighter、databento、tardis…;**无 Bitget** | 4 | 3 | 3(研究/第二系统)/ 1(热路径) |
| [barter-rs](https://github.com/barter-rs/barter-rs) | Rust;MIT | 2.3k★;barter 0.14.0(2026-08-20) | Engine + 数据导向状态(索引直查);MarketStream / ExecutionClient trait | "minimal allocations";tokio I/O;多线程 | barter-data:binance、bybit、okx、coinbase、kraken、gateio、bitmex、bitfinex;**barter-execution 仅 binance + mock** | 3 | 2 | 2 |
| [cryptofeed](https://github.com/bmoscon/cryptofeed) | Python;XFree86 式许可 | 2.9k★;仍有提交(2026-09-27)但 12 个月无 PyPI 发布,bug 积压 | asyncio WS 采集 + 多后端落库 | Python,不适用 | 覆盖面最广(含 Bitget、OKX、Bybit、Binance 等) | 2 | 1 | 1(仅作报文格式参考) |
| [tectonicdb](https://github.com/0b01/tectonicdb) | Rust;许可不明 | 752★;2021 年后无实质开发 | L2 tick 专用列存,~50× 压缩[自报] | — | — | 1 | — | 1(概念参考) |
| [ccxt](https://github.com/ccxt/ccxt) | JS/Python/PHP…;MIT | 非常活跃 | REST/WS 统一抽象 | 不面向延迟 | 全覆盖 | 4(作为参考) | 1 | 2(符号规范化、REST 语义速查) |
| [Hummingbot](https://github.com/hummingbot/hummingbot) + [Gateway](https://github.com/hummingbot/gateway) | Python + TypeScript;Apache-2.0 | 20k★;月度发布(v2.15 2026-06,7 月又一版) | 策略 / 连接器 / Gateway(DEX 中间件:Router / AMM / CLMM 子类型) | Python;DEX 延迟取决于 RPC 供应商(Chainstack / Helius) | CEX 广;DEX:Solana、EVM(含 BSC) | 3 | 1 | 2(DEX 连接器 schema 与路由逻辑的参考) |
| [Kelp](https://github.com/stellar-deprecated/kelp) | Go | 已进入 stellar-deprecated 组织 | — | — | — | 0 | — | 0 |
| [openlimits](https://github.com/nash-io/openlimits) | Rust;BSD-2 | 最后推送 2022-07 | — | — | — | 0 | — | 0 |

### 1.2 逐项评估

**hftbacktest。** 它解决的是我们最关心的仿真问题:限价单在队列中的位置(RiskAverse / PowerProb / LogProb 等模型)、feed 延迟与订单往返延迟分别建模(可用实盘采集的延迟序列插值)、L3 MBO 重建、以及"策略代码在 `Backtest` 与 `LiveBot` 两种 runner 下不改一行"。Rust 实盘架构是:一个 **connector 进程**(binancefutures / binancespot / bybit)统一持有交易所连接,多个 bot 进程通过 **iceoryx2** 共享内存订阅行情、发送订单(ROADMAP 已勾选"Support external connectors through IPC for multiple bots via a unified connection")。这恰好是我们要的"多策略共享一条行情连接、strategy 进程崩溃不影响连接"的拓扑。缺点:connector 内部是 tokio + reqwest,下单走 REST;作者明确写"live bot 未经充分测试";2026 年项目实质停摆(社区 fork 如 tommy-ca/ChronForge 以 overlay 方式续命)。**结论:fork `hftbacktest` crate 做回测内核并锁定版本,参考其 connector↔bot 拓扑;不把它放进实盘依赖树。**

**nautilus_trader。** 2026-08 完成 v1(Cython)→ v2(Rust + PyO3)切换,v1 只在 `develop_v1` 分支保留约三个月安全回补。Rust 核心的宣传口径是"确定性事件驱动、同一时间模型跨回测与实盘、可全 Rust 编写关键系统"。适配器里 Hyperliquid(含 HIP-3 builder perps、HIP-4 outcome markets、builder code 零费率归因)和 dYdX(gRPC 下单)都是 Rust 实现,对 CEX-DEX 方向有直接参考价值。但要清楚三点:(a)全链路仍以 tokio 为 I/O 中心,消息经 MessageBus 分发,这是"数十 µs 级"而非"个位 µs 级"的设计;(b)**LGPL-3.0**:作为 Python 包动态加载没有问题,但若把其 Rust crate 静态链接进我们的私有二进制,需要法务确认(**[不确定]** Rust 静态链接与 LGPL 的边界在业界仍有争议);(c)没有公开的 p50/p99 数字,官方只说"持续基准防回归"。**结论:作为研究平台 / 非延迟敏感策略的执行器 / 适配器实现的"参考答案",不上热路径。**

**barter-rs。** 设计干净(数据导向、索引直查、trait 化的 MarketStream/ExecutionClient),0.14 仍在活跃演进。但执行端只有 Binance + mock,且 README 带"仅供教育研究"免责声明。**结论:可读其 `barter-integration` 的 WS 重连 / 订阅校验 / 心跳处理代码作为参考,不依赖。**

**cryptofeed / ccxt / Hummingbot。** 三者都是 Python/TS,不进入热路径。cryptofeed 的价值是它为几十个交易所写好的报文归一化逻辑(字段名、时间戳单位、增量/快照语义),ccxt 的价值是 REST 语义与符号规范化速查,Hummingbot Gateway 的价值是 DEX 连接器的接口分层(Router / AMM / CLMM)与 RPC 供应商抽象——对我们 BSC 路由那一侧是现成的对照。

---

## 2. 通用低延迟构件

### 2.1 线程间队列(同进程)

| Crate | 版本 / 日期 | 模型 | 等待策略 | 基准结论 | 评价 |
|---|---|---|---|---|---|
| [disruptor](https://github.com/nicholassm/disruptor-rs) | 4.4.0 / 2026-08-08;MIT;905★ | SPSC / SPMC / MPSC / MPMC,消费者依赖图 | BusySpin(可 `pin_at_core`)/ Sleep | 作者基准(2016 MacBook,未绑核)优于 crossbeam;HN 2024-07 讨论;**批量发布**是低延迟关键 | 最贴近 LMAX 原版语义;推荐作为核心 ring |
| [rtrb](https://github.com/mgeier/rtrb) | 0.3.5 / 2026-08-18;347★ | SPSC(wait-free,real-time safe) | 调用方自旋 | 作者 issue #39 坦言微基准"深度不可靠";crossbeam-queue 在无竞争时慢于 SPSC 实现、竞争时接近 | 极简、无 unsafe 表面;适合 feed→strategy 单向管道 |
| [ringbuf](https://crates.io/crates/ringbuf) | 0.5.2 / 2026-09-13 | SPSC,支持 slice 批量 | 自旋 | rtrb 基准中略慢于 rtrb | 备选 |
| crossbeam-channel | 0.5.17 | MPMC 通用 | park/unpark | 尾延迟含 futex 唤醒 | 控制面用,热路径不用 |

要点:三者都做了 cache-line padding 防 false sharing;区别在 API 与批处理。一篇 2026-02 的 SPSC 手写实现文章给出 p99 207 µs vs crossbeam 349 µs 的对比,但其绝对值(百 µs)说明测试机噪声很大,只能看相对趋势。**最终数字必须在我们的绑核实例上用 `benches/spsc.rs` 复测。**

### 2.2 进程间 / 跨机通信

| 方案 | 状态 | 已发布延迟数据 | 适用 |
|---|---|---|---|
| [iceoryx2](https://github.com/eclipse-iceoryx/iceoryx2) | 0.10.0(2026-09-18);Apache-2.0;2.6k★;计划 2026 年底前 1.0;C/C++/Python/Rust/C# 绑定 | README:64 KB 载荷 240 ns(i7-10875H)vs iceoryx 1 µs vs MQueue 14 µs vs UDS 23 µs;仓内 ROS2 对比 145 ns vs rmw_fastrtps 11.2 µs;学术论文(2026)指出**默认 100 µs 轮询间隔会把端到端拉到 54 µs**、发布延迟随订阅者数量从 1.2 µs 升到 21.7 µs(每订阅者独立通道);0.10 每次 loan 多一次 14–19 ns 的小分配[下游报告] | connector 进程 ↔ 策略进程;Python 直接订阅(官方 Python 绑定)。**必须忙等或用 WaitSet + 绑核,否则失去意义** |
| [Aeron](https://github.com/aeron-io/aeron)(Java/C) + [rusteron](https://github.com/gsrxyz/rusteron) | Aeron 8.9k★ 活跃;rusteron 0.2.7(2026-09-14),GSR 出资维护、生产使用;含 client / archive / media-driver / rb;FFI 包装 C 客户端,`unsafe`,handler 需手动 leak/release | AWS 2025 基准(c6in.16xlarge,集群置放组):Transport 开源 Java 驱动 P50/P99/P99.9 = 21/32/46 µs,C 驱动 22/43/60,Premium(DPDK)24/29/31;1M msg/s 时开源 P99 84 µs vs Premium 39 µs;Cluster 三节点开源 95/136/197 µs,Premium 76/98/106;1M msg/s 时开源劣化到 8.5 ms P99,Premium 143 µs;加密仅 +1 µs | 跨机可靠 UDP / 组播、Archive 回放、Cluster 状态机复制。单机场景不需要;若日后多 AZ 部署再引入 |
| [aeron-rs](https://github.com/UnitedTraders/aeron-rs)(纯 Rust 移植) | 0.1.8(2025-01),低活跃,有 UB 历史 | — | 不推荐 |
| Chronicle Queue(Java) | 活跃,Apache-2.0(OSS 版) | 同机 JVM↔JVM <1 µs;99% <40 µs;尾部取决于 OS/page cache;掉电丢未刷页 | **借概念不借代码**:mmap 追加式日志 + 多读者 tail,是我们事件日志与 Python 读取器的模板 |
| Seastar(C++) | 9.4k★,活跃(C++23/26,2026-09 加 QUIC),DPDK 原生栈 | ScyllaDB/Redpanda 底座 | shared-nothing、thread-per-core、100% 轮询——理念完全适用;代码库对小团队过重,不引入 |

### 2.3 编码 / 序列化

| 方案 | 版本 | 特性 | 评价 |
|---|---|---|---|
| [SBE](https://github.com/aeron-io/simple-binary-encoding) 官方 Rust 生成器 | sbe-tool(Java)输出 100% safe、零依赖的 Rust crate | 定长字段 O(1) 访问、schema 演进(版本号 + 可选 group/var data)、金融业标准 | 需要 Java 跑生成器,一次性成本可接受;**内部事件 / 日志格式首选** |
| [ergo-sbe](https://crates.io/crates/ergo-sbe) | 0.1.29(2026-09-19),作者同 rusteron | 纯 Rust、build.rs 集成、"与 sbe-tool 在已测范围内 wire 兼容" | 年轻;若不想引 Java 可试,需自测兼容性 |
| [sbe_gen](https://crates.io/crates/sbe_gen) | 0.7.3(2026-04) | 基于 zerocopy,WIP | 观望 |
| [zerocopy](https://github.com/google/zerocopy) | 0.8.59(2026-09-25),Google | `FromBytes/IntoBytes/KnownLayout` 派生,`#[repr(C)]` 结构体直接 view 字节 | 与 SBE 互补:固定布局的内部结构体、共享内存记录 |
| [rkyv](https://github.com/rkyv/rkyv) | 0.8.18 | 零拷贝反序列化最快;schema 演进弱 | 归档 / 内部缓存,不做跨版本协议 |
| bincode 3.0.0 / postcard 1.1.3 | — | 紧凑、简单 | 配置与非热路径消息 |
| Cap'n Proto 0.27.2 / FlatBuffers 25.12 | — | 跨语言、零拷贝 | 无特别优势;iceoryx2 0.10 新增 FlatBuffers 载荷支持可注意 |

基准来源:[rust_serialization_benchmark](https://github.com/djkoloski/rust_serialization_benchmark)(2026-09 nightly 重跑),rkyv 在多数项目领先;但对我们而言,热路径消息是**定长、字段少、无需通用序列化**——SBE/zerocopy 直接读写字节即可,"序列化框架"根本不该出现在热路径。

### 2.4 I/O 运行时:tokio、io_uring 家族、无运行时自旋

| 运行时 | 版本 / 活跃 | 基准 | 评价 |
|---|---|---|---|
| tokio | 活跃 | issue #2702:多线程调度器负载下"首次 poll 延迟"造成尾延迟;协作式预算 0.2.14 起降低尾部约 3× | 控制面 / REST / 日志 / 运维;**不做行情与下单热路径** |
| [monoio](https://github.com/monoio-rs/monoio) | 仓库活跃(2026-07),crate 0.2.4 停在 2024-08;Apache-2.0 | 官方基准:**单核 + 少量连接时 io_uring 延迟高于 epoll**;多核吞吐 2–3× tokio 多线程(其中大部分来自 thread-per-core 而非 io_uring) | 我们是"少量长连接",io_uring 无收益 |
| compio | 0.19.2(2026-08),跨平台 | statico(2026-05)4 线程:tokio-uring 1.39M、monoio 1.36M、compio 1.27M、glommio 1.14M req/s,比 tokio 快 27–64% | 同上 |
| glommio / tokio-uring | crate 分别停在 2024-03 / 2024-05 | — | 不选 |
| **无运行时:绑核线程 + 非阻塞 socket 自旋** | std / libc / nix | 没有发表的 Rust 直接对比;共识(tokio 维护者博客 2026-09、Automated Quant "Thread Spinning"、Optiver/Jane Street 材料)是:专用核心上的自旋在 µs 尾部必胜,tokio 社区的答案是"独立绑核 runtime + 有界自旋(如 50 µs)" | **热路径采用。** `recv` 返回 `EAGAIN` 即自旋(可加 `_mm_pause`/`isb`),可选 `SO_BUSY_POLL`;每个连接一个核或少数连接共享一个核 |

我们只有个位数到十位数的长连接、每条连接消息率 ≤ 数万/s,这正是"epoll/io_uring 的批处理优势为零、上下文切换与唤醒成本主导"的区间。因此运行时选择不是性能问题而是**架构纪律问题**:热路径线程不 await、不 park。

### 2.5 时钟、直方图、亲和性、调度

| 需求 | 选型 | 备注 |
|---|---|---|
| 高频取时 | [quanta](https://github.com/metrics-rs/quanta) 0.13.0(2026-09-27)/ [fastant](https://crates.io/crates/fastant) 0.1.11 / minstant 0.1.7(2024) | TSC 仅 x86;minstant 自测 ~10.5 ns vs quanta 31.6 ns vs std 26.9 ns(Xeon E5-2630 v4)。**我们的 Graviton(aarch64)上三者都回退到 std(vDSO 读 CNTVCT),约 20–30 ns,可接受[不确定:未在 c7g 实测]**。quanta 的 `recent()` 缓存值适合非关键路径 |
| 延迟直方图 | [hdrhistogram](https://github.com/HdrHistogram/HdrHistogram_rust) 7.6.0 | 记录 3–6 ns、无分配;预先分配好 |
| 绑核 | [core_affinity](https://crates.io/crates/core_affinity) 0.8.3 或直接 `libc::sched_setaffinity` | 配合内核 `isolcpus` / `nohz_full` / `rcu_nocbs`,IRQ 亲和性挪走 |
| 实时调度 | `libc::sched_setscheduler(SCHED_FIFO)` | 需要 CAP_SYS_NICE;2026 Zenodo 论文验证 taskset + SCHED_FIFO 在 CPU 饱和下对网络尾延迟的抑制效果 |
| 硬件时间戳 | ENA 自 2025-06 支持纳秒级 RX 硬件时间戳(PHC、`SO_TIMESTAMPING`、`ethtool -T`) | 用来度量"网卡到应用"这一段,是我们 tick-to-trade 拆解的锚点 |

### 2.6 分配器

mimalloc 0.1.52 / tikv-jemallocator 0.7.0 / bumpalo 3.20.3 都活跃。证据:分配器只在"分配密集 + 并发"时移动 p99;有 2025-07 的 mimalloc 反而变慢的案例(issue #1119,page-collect 占 20% cycles);musl 默认分配器慢 7×。**结论:全局用 mimalloc(与 nautilus 一致)保护控制面;热路径的正确答案是零分配——启动时预分配所有对象池、订单槽、缓冲区,`bumpalo` 仅用于每 tick 可整体重置的临时 arena。**

### 2.7 JSON 解析

| Crate | 版本 | 方式 | 数据 |
|---|---|---|---|
| [sonic-rs](https://github.com/cloudwego/sonic-rs) | 0.5.10(2026-09-11);Apache-2.0 | SIMD,直接 → 结构体(无 tape 中间层),arena DOM(对象是 vector 非 hashmap),`get_from_str` 惰性取字段,借用 `&str` | 反序列化比 simd-json 快 1.5–2×、比 serde_json 快 3–4×[自报,Shuttle 等复现];x86 全速,提供 aarch64 基准;unsafe 多 |
| [simd-json](https://github.com/simd-lite/simd-json) | 0.18.1(2026-08-23) | 两阶段(tape)| canada.json:simd-json 8.09 ms vs serde_json 9.36 ms |
| serde_json | — | 基线 | DOM 模型低效 |

策略:交易所每种消息(bookTicker、depth diff、trade、订单回报)写专用 `#[derive(Deserialize)]` 结构体,字段用 `&str`/`Cow` 借用,数字字段保持字符串再用自写的定点解析(避免 f64 往返);消息类型分派用 `sonic-rs` 惰性 `get` 读一个字段而非全量 DOM。2026-03 有文章指出 sonic-rs 的惰性解析使得基准必须"真的读取数据"才公平——我们的用法正是读取全部字段,数字可信。

### 2.8 十进制 / 定点

| 方案 | 表示 | 性能 | 评价 |
|---|---|---|---|
| [rust_decimal](https://crates.io/crates/rust_decimal) 1.43.0 | 96 bit 尾数 + 每实例 scale(本质是十进制浮点) | add 需比较 scale、处理符号,分支多 | 研究 / 报表 / 配置 |
| [primitive_fixed_point_decimal](https://github.com/WuBingzheng/primitive_fixed_point_decimal) 1.5.0 | `ConstScaleFpdec<i64, S>` `#[repr(transparent)]`;也有运行期 scale 的 `OobScaleFpdec` | add/sub ≈ 0.31 ns(单指令);作者跨 crate 基准排序:bigdecimal ≪ fastnum < rust_decimal < decimax < pfpd;定点曲线平坦 | **热路径首选**;或自写 |
| fixdec 0.1.0 | i64 8 位小数 / i128 12 位小数 | README 基准是占位符 | 观望 |

注意:i64 → f64 转换在 |raw| > 2^53 时双重舍入(某交易引擎 PR 记录 precision 16 几乎全错);所有对外输出走十进制字符串格式化,不经过 f64。

### 2.9 订单簿

| 实现 | 结构 | 更新成本 | 评价 |
|---|---|---|---|
| Optiver(David Gross,CppCon 2024) | sorted vector + `lower_bound`;明确"node-based 容器输" | 未给绝对数 | 设计准则 |
| hftbacktest ROIVectorMarketDepth | 以价格 tick 为索引的向量,限定 ROI 上下界 | O(1) 定位 | 我们的模板 |
| nautilus OrderBook(Rust) | L1/L2/L3 三种类型;**[不确定]** 内部为 BTreeMap 价格阶梯 | 有 Criterion 基准,未公开数字 | 参考 |
| [orderbook-rs](https://github.com/joaquinbejar/OrderBook-rs) 0.13.1(2026-09-18) | 线程安全 lock-free price level,面向撮合引擎 | 未公开 ns/更新 | 撮合视角,不是"维护交易所镜像"视角 |
| [ninjabook](https://github.com/ninja-quant/ninjabook) 0.1.7(2024-11) | 固定 500 档 vs naive Vec,30 万条 L2 事件基准框架 | — | 基准方法可借 |
| lob-orderbook | BTreeMap<OrderedF64> | O(log n),只重建不增量 | 不选 |

我们的 feed:Binance `depth@100ms` 增量(需 REST 快照 + `U/u` 序号对齐)、OKX `books`/`books-l2-tbt`(400 档、checksum)、Bybit `orderbook.50/200/500`、Bitget `books` 增量(checksum)。档位少、更新频繁、访问模式是"改一档 + 读 BBO / 前 N 档 VWAP",稠密数组(index = (price − base)/tick)+ 两端 BBO 指针 + 惰性重扫是最优结构;超出预分配范围时整簿重定位是罕见路径。

---

## 3. C++ 选项(仅在 Rust 生态有缺口时)

| 领域 | 候选 | 事实 | 结论 |
|---|---|---|---|
| 用户态 TCP 栈(LD_PRELOAD 零改动) | [OpenOnload](https://github.com/Xilinx-CNS/onload)(GPLv2/BSD);NVIDIA XLIO | Onload 快路径需 Solarflare 网卡(ef_vi),在 **AWS ENA 上只能走最慢的 generic AF_XDP**,有用户报 "eth0 does not support XDP";XLIO 只认 ConnectX-6 Dx / 7 / BlueField-3 | 在 AWS 上两者均不可用。AWS 官方 tick-to-trade 指南:ENA Express(SRD)会抬高 p50、不适合 HFT;要内核旁路只能 DPDK(ENA PMD,LLQv2)或 XDP。我们此前研究已判定 **c7g.large 上 DPDK 不值得**,维持该结论,把力气花在内核调优 + 绑核 + 自旋 |
| WebSocket 客户端 | Boost.Beast、IXWebSocket、websocketpp、uWebSockets | uWebSockets **无客户端**;IXWebSocket 维护者公开表示精力不足,自报 localhost ~5 µs/消息(2015 MBP);Beast WS 流不支持非阻塞模式;autoroute 客户端对比(2020/2021)显示 Beast / websocketpp / libwebsockets / IX 互有胜负且噪声巨大 | **没有任何可测量的优势**。WS 帧解析成本 ≪ TLS 解密 + JSON 解析。全部用 Rust |
| FIX | hffix(header-only)、QuickFIX、OnixS(商业) | 加密 CEX 主流仍是 WS/REST;**[不确定]** 少数(如 Binance Spot FIX、Coinbase)提供 FIX,是否更快取决于场馆内部 | 先不做;若某场馆 FIX 入口实测更快,用 FFI 包 hffix 即可(单头文件、零依赖) |
| 哈希 / 容器 | folly F14、abseil SwissTable | Rust `hashbrown`(std HashMap)即 SwissTable 移植 | 热路径尽量用数组索引而不是 hash |

---

## 4. Rust WebSocket + TLS 客户端栈

### 4.1 WebSocket

| Crate | 版本 / 日期 / 许可 | 定位 | 已知数据 | 评价 |
|---|---|---|---|---|
| [tungstenite](https://github.com/snapview/tungstenite-rs) / [tokio-tungstenite](https://github.com/snapview/tokio-tungstenite) | 0.30.0 / 2026-07-11 / MIT | 事实标准;同步 API 可跑在任意 `Read+Write`,包括**非阻塞 socket**(返回 `WouldBlock` 即自旋) | 维护者声明 ≥0.26.2 已基本追平 fastwebsockets;2023 旧基准落后 3.7× 已过时 | **热路径:`tungstenite` 同步 + 非阻塞 socket + 绑核自旋线程**;`tokio-tungstenite` 用于控制面 |
| [yawc](https://github.com/infinitefield/yawc) | 0.4.2 / 2026-09-17 / **MPL-2.0** | permessage-deflate(RFC 7692)含 context takeover / window bits 控制,零拷贝,WASM;自称"驱动 7×24 高频交易系统" | 无第三方基准 | 若必须 deflate(OKX、Bitget 等可协商压缩,通常应关闭)再考虑;MPL 文件级 copyleft 可接受 |
| [fastwebsockets](https://github.com/denoland/fastwebsockets) | 0.8.1 / 2026-01-01 / Apache-2.0 | Deno 出品 | tokio-websockets 基准 README 标注其 **"unsound and not thread-safe"(issue #42)**、非严格规范 | 不选 |
| [tokio-websockets](https://github.com/Gelbpunkt/tokio-websockets) | 0.13.3 / 2026-06-25 / MIT | 严格规范、SIMD 掩码/UTF-8,基准用 uWebSockets load_test | 与 uWebSockets 同量级 | tokio 侧的合格替代 |
| [wtx](https://github.com/c410-f3r/wtx) | 0.52.1 / 2026-08 / MPL-2.0 | 作者基准:wtx > tokio-tungstenite > uWebSockets > fastwebsockets | 自报 | 观望 |
| [sockudo-ws](https://github.com/sockudo/sockudo-ws) | 2.1.0 / 2026-09-18 / MIT;2025-12 才创建,222★ | "面向 HFT":SIMD 掩码、零拷贝、tokio/compio/io_uring、fuzz | 自报:10 万次 "Hello World" 回环 fastwebsockets 3.3 ms vs tokio-tungstenite 6.4 ms(自身数字更优);vs uWebSockets 1.02–1.03× | **太年轻**,一年后再看 |
| soketto 0.8.1(2024-12)、ws-tool 0.11.1(2025-05) | — | 前者停滞;后者支持阻塞与 deflate,社区小 | — | 不选 |

### 4.2 TLS

- **rustls 0.23.45 + aws-lc-rs 1.18.1**。官方 [2025-07-31 报告](https://rustls.dev/perf/2025-07-31-report/)(vs OpenSSL 3.5.15 / BoringSSL,x86_64):批量传输与恢复握手(+80–330%)领先;RSA 全握手服务端少 27–30%(客户端反而多 2.06×);[2026-Q1 更新](https://www.memorysafety.org/blog/26q1-rustls-performance/)显示 rustls 又略快;0.24 将引入 "split mode"(握手后拆分收发对象,利于收发分线程)。
- **ARM64 警示**:[2024-10 ARM64 报告](https://rustls.dev/perf/2024-10-31-arm64/)称 rustls 吞吐"略落后"OpenSSL——我们跑 Graviton,需要在目标实例上重测 AES-GCM 单记录解密延迟(预期在百 ns 级,不构成瓶颈,但要拿数据)。
- ring 后端受编译器影响大(GCC 比 Clang 慢 15–27%),选 aws-lc-rs 规避。
- **kTLS**([rustls/ktls](https://github.com/rustls/ktls) 6.0.2,2025-04):把记录层加解密交给内核,收益在 sendfile/大吞吐;对我们每条几百字节的小报文,反而多一次内核往返、失去用户态自旋的可控性。**不用。**
- 后量子 X25519MLKEM768 已是 rustls/OpenSSL 默认,握手更贵——只影响重连,预建备用连接即可掩盖。

### 4.3 单线程阻塞/非阻塞自旋 vs tokio:证据小结

没有找到"std socket 自旋 vs tokio 的 Rust p99 对照"论文级数据;但以下证据方向一致:
- tokio 官方 issue #2702(多线程调度器负载下首次 poll 尾延迟)、协作式预算博客(尾延迟改善约 3×,说明原本尾部显著);
- tokio 生态 2026-09 的两篇实践文("Principles for fast Tokio applications"、"Fast Tokio Is a Scheduling Budget")把"专用核心 + 有界自旋"定性为"追逐微秒级延迟的高级战术",并提到 TokioConf 多数团队最终采用多 runtime 隔离;
- monoio 官方基准:少连接、单核时 io_uring 延迟高于 epoll;
- 业界(Optiver、Jane Street、LMAX、Aeron BusySpinIdleStrategy)的一致做法是忙等。

**判断:对固定的少量连接,绑核线程 + 非阻塞 socket 自旋在 p99 上优于任何 async 运行时,这是"少做事"带来的确定性收益,不需要更多证据即可采用;我们要做的是在实施后用 HW 时间戳与 hdrhistogram 量化差距。**

---

## 5. 公开 HFT 工程材料中的架构模式

### 5.1 来源与核心主张

| 来源 | 主张 |
|---|---|
| LMAX 架构(Fowler 2011;Martin Thompson Disruptor) | 单线程业务逻辑处理器、全内存、事件顺序化、journaller 持久化 → **生产 bug 可在开发机按原顺序重放复现**;Single Writer Principle:任一数据只有一个写者,消灭锁与缓存行争用 |
| Jane Street "How to Build an Exchange"(Brian Nigito,2017)/ "Multicast and the Markets"(2020) | 组件在 500k msg/s 下个位数 µs 延迟,**速度的目的是让复制协议简单**(可靠组播 + retransmitter);撮合引擎即 sequencer,序号错误的消息直接丢弃,强迫客户端先处理正确流;单线程 + 组播是现代交易所的两块积木 |
| Aeron Cluster / Aeron Sequencer;Bullish 案例 | 确定性命令应用是副本不分叉的前提;Bullish 的日志 = Aeron Archive + SBE、零分配、单写者,并用 TLA+ 验证不变量 |
| David Gross(Optiver)Meeting C++ 2022 "Trading at Light Speed" | 低延迟是**设计阶段的战略**而非事后优化;instrument store 用 stable vector;并发读用 SeqLock;事件用 SPMC 队列;C-state/P-state、共享 LLC 调优;先定义 "trigger-to-target latency" 再谈优化 |
| David Gross CppCon 2024 "When Nanoseconds Matter" | 订单簿:node-based 容器输,sorted vector + lower_bound 赢;用硬件计数器测量真正重要的东西;并发队列设计 |
| Carl Cook(Optiver)CppCon 2017 "When a Microsecond Is an Eternity" | ~2.5 µs wire-to-wire;**cache warming:每个行情 tick 都执行完整下单路径,只在真正触发时放行订单**;错误检查聚合为一个标志位、只判一次;模板替代虚函数;低成本判断放在 `||` 前;检查编译器产物;不要过度使用线程与共享内存 |
| Imperial 论文 arXiv 2309.04259 / 2308.14185 | 实测 cache warming 与 constexpr 收益最大;半静态条件(运行期改写跳转)优于分支提示 |
| HRT huge pages 两篇 | 4 KiB 页最坏 3 次依赖访存 ≈ 210 ns 才拿到地址;THP 分配延迟不确定、碎片整理会挂起进程 → 用**显式 hugetlbfs 预分配**,并把 .text 也映射到大页(libhugetlbfs / elfremapper / MADV_COLLAPSE) |
| Databento 内核旁路指南;AWS 数字资产 tick-to-trade 指南 | 内核 6.12+、集群置放组、DPDK/XDP/SR-IOV 而非 ENA Express;ENA 硬件 RX 时间戳(2025-06)用于测量 |

### 5.2 提炼为我们的结构

```
[Feed thread ×N, pinned]  --SPSC ring-->  [Sequencer/Strategy thread, pinned, single writer]  --SPSC ring-->  [Order gateway thread(s), pinned]
        |                                           |                                                          |
   HW RX timestamp                        append-only mmap journal (SBE)                            send timestamp → hdrhistogram
                                                    |
                                    Python readers (mmap / Arrow), replay tool, tests
```

- 所有进入 sequencer 的事件(行情、订单回报、定时器、运维命令)先被赋予单调序号并写入日志,再被处理 → 状态 = f(日志),任何时刻可重放。
- 策略线程不做 I/O、不分配、不加锁;所有跨线程共享只读数据(如合约参数)用 SeqLock 或双缓冲交换。
- 下单路径每 tick 走到"最后一步"(序列化好订单帧、算好签名前缀),只在触发时写 socket——Carl Cook 的 warm path。
- 重放测试:回放昨日日志,比对状态哈希与订单流逐字节一致,是 CI 的金标准(S2 的做法:同 seed 重跑比对 TRACE 日志)。

---

## 6. Python 集成(不碰热路径)

| 需求 | 方案 | 状态 |
|---|---|---|
| Rust ↔ Python 绑定 | [PyO3](https://github.com/PyO3/pyo3) 0.29.2(2026-08-28)+ maturin | 成熟;nautilus / hftbacktest / Polars 皆用 |
| 批量数据零拷贝交换 | Arrow PyCapsule Interface(`__arrow_c_array__` / `__arrow_c_stream__`):[pyo3-arrow / arro3](https://github.com/kylebarron/arro3) 0.19.0、minarrow-pyo3 | 与 Polars、DuckDB、pyarrow、nanoarrow 互通;Arc 引用计数管理生命周期 |
| 实时读取事件日志 | (a)mmap 日志文件 + `numpy.memmap` 按 SBE 布局解析(Chronicle 模式);(b)iceoryx2 官方 Python 绑定直接订阅 | 记录必须是扁平 POD(无指针、定长),这是跨进程共享内存的硬约束 |
| 研究 | Polars / DuckDB 读 S3 parquet(现有管线) | 已就位 |
| 回测/实盘一致性(参考) | hftbacktest:同一份 Rust 策略在 `Backtest` 与 `LiveBot` 下运行,Python 侧只跑回测;nautilus:同一引擎与时间模型,Python 策略经 PyO3 在两端运行 | 我们:策略核心 Rust,回测 = 用日志重放驱动同一 sequencer;Python 负责参数搜索、报表、监控 |

---

## 7. 测试与仿真

| 类别 | 可用资产 | 评价 |
|---|---|---|
| 确定性重放 | 自有事件日志 + sequencer(§5.2) | 核心手段;测试断言"重放输出逐字节等于生产输出" |
| 带队列位置的回测 | hftbacktest 回测内核(MIT):队列位置模型、Constant/插值延迟模型、PartialFillExchange、L3 MBO、fee model | **fork 并锁定 rust-v0.9.4**;先合入 PR #323(修 #316:PartialFillExchange + 概率队列模型下部分成交不入账);用自有 tick 数据(S3 parquet)转其格式 |
| 交易所仿真 / mock | [Binance-local-exchange](https://github.com/SockTrader/Binance-local-exchange)(TS,GPL-3,2023,部分代理真实 Binance);Binance testnet(24 h 强断、`serverShutdown` 提前 10 min、20 s ping / 1 min pong 超时);nautilus `sandbox` 适配器 | 现成 mock 都弱;**自写一个按各所 wire 格式回放捕获报文的 mock WS server**(我们已有 L2 捕获与 WAL 基础设施),支持注入延迟、乱序、断连、序号缺口 |
| 订单状态机属性测试 | [proptest-state-machine](https://crates.io/crates/proptest-state-machine) 0.8.0(2026-03);bowst 蓝图:PendingNew→Live→PendingCancel/PendingReplace→Filled/Cancelled/Rejected,覆盖 fill-before-ack、cancel-reject-because-filled、重复/乱序消息 | 参考模型(纯内存 OMS)与被测系统同步步进、比对不变量(无未知订单、敞口有上限) |
| 网络混沌 / 确定性仿真 | [turmoil](https://github.com/tokio-rs/turmoil) 0.7.2(2026-04,tokio 侧);[madsim](https://github.com/madsim-rs/madsim) 0.2.34(libc 级时间/熵劫持);mad-turmoil(S2 2025 混合) | 只覆盖 tokio 控制面;热路径的断连/灰色故障(连接活着但极慢)用 mock server + 故障代理注入;"seed 只钉住仿真器的选择",时钟与 RNG 必须可注入 |
| 基准与回归 | Criterion + CodSpeed(nautilus 做法);hdrhistogram 采 p50/p99/p99.9;HW RX 时间戳做端到端 | 每次提交跑 ring / 解析 / 订单簿 / 定点数微基准;每日跑一次全链路重放延迟 |

---

## 8. 决策表:借鉴什么、直接用什么、自己写什么

| 组件 | 决策 | 选型 / 参照 | 理由 |
|---|---|---|---|
| 线程间 ring | **直接用** | `disruptor`(或 `rtrb`) | 成熟、忙等、绑核、批处理;自写收益小 |
| 跨进程 IPC | **直接用** | `iceoryx2`(忙等 / WaitSet) | 100 ns 级、Python 绑定、hftbacktest 已验证该拓扑 |
| 跨机传输 / 集群 | **暂缓;需要时直接用** | Aeron + `rusteron` | 单机阶段不需要;多 AZ 时再上 |
| 内部消息编码 | **直接用** | SBE(sbe-tool Rust 目标)+ `zerocopy` | 金融业标准、O(1) 字段访问、schema 演进 |
| 事件日志 | **自己写(借概念)** | Chronicle Queue / Aeron Archive 思路:mmap 追加、单写者、多读者 | 几百行;与 SBE 记录、Python memmap 直接对接 |
| Sequencer / 状态机 | **自己写(借模式)** | LMAX、Jane Street JX、Aeron Cluster | 这是系统的核心资产,必须完全掌控 |
| 订单簿 | **自己写(借设计)** | hftbacktest ROI 向量簿、Gross 的 sorted vector 结论 | 现成 crate 都是撮合视角 |
| 定点数 | **直接用或自写** | `primitive_fixed_point_decimal` / 200 行自写 | i64 + 编译期 scale |
| JSON | **直接用** | `sonic-rs` + 每消息专用结构体 | 最快且支持惰性取字段 |
| WebSocket | **直接用** | `tungstenite`(同步、非阻塞 socket、自旋线程);控制面 `tokio-tungstenite` | 框架成熟;C++ 无优势 |
| TLS | **直接用** | `rustls` + `aws-lc-rs` | 基准领先;ARM64 需实测 |
| 交易所适配器(Binance / OKX / Bybit / Bitget / Hyperliquid) | **自己写(参照实现)** | nautilus 适配器(Rust)、cryptofeed 报文归一化、我们已有的 Bitget 集成 | 现成框架都绑定自身引擎与 tokio;Bitget 无人覆盖 |
| DEX 侧(BSC 路由 / 报价) | **自己写(参照)** | Hummingbot Gateway 分层、现有 `traders/bsc_route` | 已有生产代码 |
| 回测内核 | **fork** | hftbacktest rust-v0.9.4 + 未合并修复 | 唯一带队列位置与双侧延迟的开源实现;上游停滞 |
| Mock 交易所 | **自己写** | 按 wire 格式回放捕获报文 + 故障注入 | 现成 mock 太弱 |
| 属性测试 / 混沌 | **直接用** | `proptest-state-machine`、`turmoil`(控制面) | 成熟 |
| 时钟 / 直方图 / 绑核 | **直接用** | `quanta` 或 std、`hdrhistogram`、`core_affinity` + libc | 成熟 |
| 分配器 | **直接用** | `mimalloc` 全局;热路径零分配 | 与 nautilus 一致 |
| 研究 / 运维 | **直接用** | PyO3 + maturin、pyo3-arrow、Polars、DuckDB | 现有管线 |
| 第二套系统(研究 / 非延迟敏感策略) | **可选直接用** | nautilus_trader v2(注意 LGPL) | 覆盖广、活跃;不上热路径 |

---

## 9. 热路径编码规则清单(Checklist)

**进程与线程**
- [ ] 每个热路径角色一个 OS 线程,`sched_setaffinity` 绑到 `isolcpus`/`nohz_full` 隔离核,IRQ 亲和性移到其他核;可选 `SCHED_FIFO`。
- [ ] 热路径线程永不 `park` / `await` / `sleep`;等待一律自旋(`spin_loop()`),必要时有界自旋后再退让——但生产配置默认无界。
- [ ] 单写者原则:每个数据结构只有一个写线程;跨线程只经 SPSC ring 或 SeqLock/双缓冲。
- [ ] 不在热路径调用 `std::time::Instant::now()` 以外的系统调用;时间戳来自 TSC/vDSO 一次读取并随事件传递。
- [ ] 每个 tick 都执行完整下单路径直到"写 socket 前一步"(cache warming),用一个标志决定是否真正发送。

**内存**
- [ ] 启动期预分配一切:对象池、订单槽、ring、解析缓冲、订单簿数组;热路径 `Vec::push` 超容量视为 bug(用 `debug_assert!(capacity)`)。
- [ ] 全局分配器 mimalloc;热路径用固定大小数组 / `ArrayVec` / bump arena 每 tick 重置。
- [ ] 显式 hugetlbfs 预留大页并 `mlockall`;关闭 THP(避免不确定的碎片整理);启动时触碰(pre-fault)所有页。
- [ ] `#[repr(C)]` / `#[repr(align(64))]` 控制布局;高频写字段与只读字段分离到不同 cache line;SPSC 头尾游标各占一行。
- [ ] 优先数组索引(合约 id、tick 索引、订单槽 id)而非 HashMap / BTreeMap;需要 map 时用预分配开放寻址表。

**数值与数据**
- [ ] 价格/数量为 i64 定点,scale 编译期固定,`#[repr(transparent)]`;禁止 f64 出现在下单路径;字符串 ↔ 定点直接转换。
- [ ] 消息类型用 `enum` + `match`(编译期分派),不用 `dyn Trait`;交易所差异用泛型/`const` 参数在编译期展开。
- [ ] JSON:每消息专用结构体、字段借用、惰性取分派字段;禁止 `serde_json::Value`。
- [ ] 内部事件用 SBE/zerocopy 定长记录;无 `String`、无 `Vec` 在事件结构体中。

**控制流**
- [ ] 无 `panic!` / `unwrap` / `expect` 于热路径;错误聚合为位标志,一次判断;`panic = "abort"`。
- [ ] 廉价判断放在 `||`/`&&` 前;冷路径(日志、指标、告警)通过 ring 发到其他线程,热路径只写一个定长记录。
- [ ] 分支尽量基于热数据;依赖冷配置的分支改为启动期特化(泛型/`const`)或半静态开关。
- [ ] 不使用 `Mutex`/`RwLock`/`Arc<..>` 引用计数更新于热路径;只读共享用 `&'static` 或 SeqLock。

**构建与部署**
- [ ] `lto = "fat"`、`codegen-units = 1`、`panic = "abort"`、`-C target-cpu=native`(为目标实例族固定,升级工具链后复测);PGO 用真实重放日志训练(预期 5–20%)。
- [ ] 关闭 debug assertions 但保留 `debug_assert!` 在测试 build;`overflow-checks` 在 release 关闭、CI 开启。
- [ ] 内核:6.12+、`isolcpus`/`nohz_full`/`rcu_nocbs`、C-state 固定(`idle=poll` 或 `intel_idle.max_cstate=0`,Graviton 上以云厂商指导为准)、关闭 irqbalance、网卡多队列 RSS 固定到非热核;集群置放组。
- [ ] 打开 ENA 硬件 RX 时间戳;每条订单记录 RX-HW → 解析完成 → 决策 → write() 返回四个时间点入 hdrhistogram;每日回放对比。

**测试**
- [ ] 每次提交:ring / 解析 / 订单簿 / 定点数 Criterion 基准回归门槛;每日:全链路重放 p50/p99。
- [ ] 重放决定性:同一日志 → 同一状态哈希与订单流;时钟与 RNG 必须注入(测试期禁止 `Instant::now`)。
- [ ] OMS 属性测试覆盖 fill-before-ack、cancel-reject-because-filled、重复/乱序回报;mock 交易所注入断连、序号缺口、延迟回报、灰色故障(存活但极慢)。
- [ ] 重连必须伴随重同步(快照 + 序号对齐),否则本地簿与仓位漂移——这是混沌测试的必测项。

---

## 10. 参考链接

**框架**
- hftbacktest:https://github.com/nkaz001/hftbacktest ;ROADMAP:https://github.com/nkaz001/hftbacktest/blob/master/ROADMAP.md ;docs:https://hftbacktest.readthedocs.io ;bug #316:https://github.com/nkaz001/hftbacktest/issues/316
- nautilus_trader:https://github.com/nautechsystems/nautilus_trader ;RELEASES:https://github.com/nautechsystems/nautilus_trader/blob/develop/RELEASES.md ;Hyperliquid 文档:https://nautilustrader.io/docs/latest/integrations/hyperliquid/ ;dYdX:https://nautilustrader.io/docs/nightly/integrations/dydx/ ;Message Bus:https://nautilustrader.io/docs/latest/concepts/message_bus/
- barter-rs:https://github.com/barter-rs/barter-rs ;cryptofeed:https://github.com/bmoscon/cryptofeed ;tectonicdb:https://github.com/0b01/tectonicdb ;Hummingbot Gateway:https://github.com/hummingbot/gateway ;Kelp(已弃用):https://github.com/stellar-deprecated/kelp ;openlimits:https://github.com/nash-io/openlimits

**队列 / IPC / 编码**
- disruptor-rs:https://github.com/nicholassm/disruptor-rs ;HN 讨论:https://news.ycombinator.com/item?id=40954104 ;rtrb:https://github.com/mgeier/rtrb ;rtrb 基准 issue:https://github.com/mgeier/rtrb/issues/39
- iceoryx2:https://github.com/eclipse-iceoryx/iceoryx2 ;v0.10.0:https://github.com/eclipse-iceoryx/iceoryx2/releases/tag/v0.10.0 ;基准说明:https://github.com/eclipse-iceoryx/iceoryx2/blob/main/benchmarks/README.md ;v0.9 博文:https://ekxide.io/blog/iceoryx2-0.9-release/ ;Agnocast 对比论文:https://arxiv.org/pdf/2605.04226
- Aeron:https://github.com/aeron-io/aeron ;rusteron:https://github.com/gsrxyz/rusteron ;aeron-rs:https://github.com/UnitedTraders/aeron-rs ;AWS 2025 Aeron 基准:https://aws.amazon.com/blogs/industries/aeron-on-aws-2025-performance-benchmark-results/ ;Aeron Sequencer:https://aeron.io/aeron-sequencer/ ;Bullish 案例:https://aeron.io/case-studies/bullish-aeron-sequencer-crypto-exchange/
- SBE:https://github.com/aeron-io/simple-binary-encoding ;Rust 生成器:https://github.com/aeron-io/simple-binary-encoding/tree/master/rust ;ergo-sbe:https://docs.rs/ergo-sbe/ ;sbe_gen:https://crates.io/crates/sbe_gen ;zerocopy:https://github.com/google/zerocopy ;rkyv:https://github.com/rkyv/rkyv ;序列化基准:https://github.com/djkoloski/rust_serialization_benchmark
- Chronicle Queue:https://github.com/OpenHFT/Chronicle-Queue ;生产深潜:https://sanj.dev/post/chronicle-queue-production-deep-dive/ ;Seastar:https://github.com/scylladb/seastar ;shared-nothing:https://seastar.io/shared-nothing/

**运行时 / 系统**
- monoio 基准:https://github.com/monoio-rs/monoio/blob/master/docs/en/benchmark.md ;statico 对比(2026-05):https://crates.io/crates/statico ;compio:https://github.com/compio-rs/compio
- tokio 尾延迟 issue:https://github.com/tokio-rs/tokio/issues/2702 ;协作式预算:https://tokio.rs/blog/2020-04-preemption ;Principles for fast Tokio applications:https://dial9-rs.github.io/blog/principles-for-fast-tokio-applications/ ;Thread Spinning(Automated Quant):https://automatedquant.substack.com/p/hft-engine-latency-5-thread-spinning
- quanta:https://github.com/metrics-rs/quanta ;minstant:https://github.com/tikv/minstant ;fastant:https://crates.io/crates/fastant ;hdrhistogram:https://github.com/HdrHistogram/HdrHistogram_rust ;core_affinity:https://docs.rs/core_affinity ;taskset+SCHED_FIFO 尾延迟研究:https://zenodo.org/records/22038679
- mimalloc 回归案例:https://github.com/microsoft/mimalloc/issues/1119 ;allocator 与 P99:https://www.kunalganglani.com/blog/rust-allocator-jemalloc-mimalloc-tcmalloc ;musl 分配器:https://nickb.dev/blog/default-musl-allocator-considered-harmful-to-performance/
- Onload:https://github.com/Xilinx-CNS/onload ;Onload AF_XDP 讨论:https://github.com/Xilinx-CNS/onload/issues/37 ;XLIO:https://docs.nvidia.com/networking/display/xliov361 ;AWS tick-to-trade Part 2:https://aws.amazon.com/blogs/web3/optimize-tick-to-trade-latency-for-digital-assets-exchanges-and-trading-platforms-on-aws-part-2/ ;ENA 硬件时间戳公告:https://aws.amazon.com/about-aws/whats-new/2025/06/amazon-time-sync-nanosecond-hardware-packet-timestamps/ ;ENA 驱动 README:https://github.com/amzn/amzn-drivers/blob/master/kernel/linux/ena/README.rst ;DPDK ENA PMD:https://doc.dpdk.org/guides/nics/ena.html ;Databento 内核旁路:https://databento.com/microstructure/kernel-bypass

**JSON / 数值 / 订单簿**
- sonic-rs:https://github.com/cloudwego/sonic-rs ;simd-json:https://github.com/simd-lite/simd-json ;Shuttle 对比:https://www.shuttle.dev/blog/2024/01/18/parsing-json-rust
- primitive_fixed_point_decimal:https://github.com/WuBingzheng/primitive_fixed_point_decimal ;decimal crate 基准:https://wubingzheng.github.io/en/Decimal-Crates-Comparison.html ;rust_decimal:https://crates.io/crates/rust_decimal ;fixdec:https://crates.io/crates/fixdec
- orderbook-rs:https://github.com/joaquinbejar/OrderBook-rs ;ninjabook:https://github.com/ninja-quant/ninjabook ;hftbacktest ROI 簿教程:https://hftbacktest.readthedocs.io/en/latest/tutorials/Working%20with%20Market%20Depth%20and%20Trades.html

**WebSocket / TLS**
- tungstenite:https://github.com/snapview/tungstenite-rs ;tokio-tungstenite:https://github.com/snapview/tokio-tungstenite ;基准讨论 issue #352:https://github.com/snapview/tungstenite-rs/issues/352 ;yawc:https://github.com/infinitefield/yawc ;fastwebsockets:https://github.com/denoland/fastwebsockets ;tokio-websockets 基准:https://github.com/Gelbpunkt/tokio-websockets/blob/main/benches/README.md ;wtx 作者文章:https://c410-f3r.github.io/thoughts/the-fastest-websocket-implementation/ ;sockudo-ws:https://github.com/sockudo/sockudo-ws
- IXWebSocket 性能页:https://machinezone.github.io/IXWebSocket/performance/ ;autoroute 客户端对比:https://github.com/bsergean/autoroute ;WS 实现对比表:https://en.wikipedia.org/wiki/Comparison_of_WebSocket_implementations
- rustls 2025-07 报告:https://rustls.dev/perf/2025-07-31-report/ ;ARM64 报告:https://rustls.dev/perf/2024-10-31-arm64/ ;2026-Q1 更新:https://www.memorysafety.org/blog/26q1-rustls-performance/ ;细分结果:https://github.com/aochagavia/rustls-bench-results ;ktls:https://github.com/rustls/ktls

**架构模式**
- Fowler LMAX:https://martinfowler.com/articles/lmax.html ;Single Writer Principle:https://mechanical-sympathy.blogspot.com/2011/09/single-writer-principle.html
- Jane Street How to Build an Exchange:https://www.janestreet.com/tech-talks/building-an-exchange/ ;视频:https://www.youtube.com/watch?v=b1e4t2k2KJY ;Multicast and the Markets:https://signalsandthreads.com/multicast-and-the-markets/
- David Gross CppCon 2024:https://www.youtube.com/watch?v=sX2nF1fW7kI ;Meeting C++ 2022:https://www.youtube.com/watch?v=8uAW5FQtcvE ;Carl Cook CppCon 2017:https://www.youtube.com/watch?v=NH1Tta7purM ;幻灯:https://github.com/CppCon/CppCon2017/blob/master/Presentations/When%20a%20Microsecond%20Is%20an%20Eternity/When%20a%20Microsecond%20Is%20an%20Eternity%20-%20Carl%20Cook%20-%20CppCon%202017.pdf
- Imperial 低延迟模式论文:https://arxiv.org/pdf/2309.04259 ;半静态条件:https://arxiv.org/pdf/2308.14185
- HRT huge pages:https://www.hudsonrivertrading.com/hrtbeat/low-latency-optimization-part-1/ 与 https://www.hudsonrivertrading.com/hrtbeat/low-latency-optimization-part-2/ ;Easyperf 大页代码段:https://easyperf.net/blog/2022/09/01/Utilizing-Huge-Pages-For-Code ;elfremapper:https://github.com/dmitriy-philimonov/elfremapper
- Rust 构建配置:https://nnethercote.github.io/perf-book/build-configuration.html ;Aeron 状态机复制深潜:https://aeron.io/other/state-machine-replication-trading-systems/

**Python / 测试**
- PyO3:https://github.com/PyO3/pyo3 ;arro3 / pyo3-arrow:https://github.com/kylebarron/arro3 ;minarrow-pyo3:https://docs.rs/minarrow-pyo3/
- proptest-state-machine:https://crates.io/crates/proptest-state-machine ;turmoil:https://github.com/tokio-rs/turmoil ;madsim:https://github.com/madsim-rs/madsim ;S2 DST 博文:https://s2.dev/blog/dst ;Binance-local-exchange:https://github.com/SockTrader/Binance-local-exchange ;bowst OMS/混沌蓝图:https://github.com/ayoola-xet/bowst ;Binance testnet WS 行为:https://developers.binance.com/docs/binance-spot-api-docs/testnet/web-socket-streams
