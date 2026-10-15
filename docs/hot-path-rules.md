# 热路径编码规则

本页是热路径编码规则的唯一清单。每条规则有编号(README 与评审意见引用编号,如"违反 M3")、适用范围与**检查方式**:能机器检查的写明是哪一种(clippy、`cargo deny`、`cargo xtask` 子命令、编译期断言、测试或基准),其余标"人工评审"。来源:[architecture-v2/10 §4](../design/10-语言分工与代码组织.md)、[research/04 §9](../design/research/04-oss-frameworks-and-patterns.md)、[13 评审](../design/13-设计评审与优化.md)(R1、R2、R3、R4、R7、R10、O9)。

## 适用范围

| tier | 适用 |
|---|---|
| `hot` | 全部规则 |
| `pure` | 内存 M5–M6、数值 N 全部、控制流 C1/C3、确定性 D 全部、构建 B 全部;不适用线程规则(pure 不建线程) |
| `warm` | T3(不与热线程共享锁)、T7、C3;可阻塞、可分配,但不得出现在 tick-to-trade 路径 |
| `cold` / `dev` | 只适用 B4/B6(不被链接进 hot crate)与 N4 中"禁 f64 于下单路径" |
| 策略包 `strategies/*` | 快速部分(`Strategy` 的全部方法)按 `hot` 全部规则;慢速部分(`Model`)按 [S5](#策略-sdk-与执行层s);快/慢部分的模块约定见 [strategies/AGENTS.md](../strategies/AGENTS.md) |

tier 定义见 [crates/AGENTS.md](../crates/AGENTS.md#tier)。一个 crate 按其最严格的公开入口标 tier。

## 检查机制

| 机制 | 位置 | 说明 |
|---|---|---|
| clippy 配置(`disallowed-methods`/`disallowed-types`/`disallowed-macros`) | `tools/lint/clippy-hot.toml`、`tools/lint/clippy-pure.toml`;每个 hot/pure crate 根目录放同名副本 `clippy.toml` | clippy 从 crate 目录向上查找 `clippy.toml`;`cargo xtask hot-check` 校验副本与模板逐字节一致 |
| crate 级 lint 属性 | 每个 hot/pure crate 的 `lib.rs` 顶部 `#![deny(...)]` 块 | Cargo `[lints]` 不能在 `workspace = true` 之上追加,所以更严的 restriction lint 用属性;`hot-check` 校验属性块存在且未被削弱 |
| `cargo deny` | `deny.toml` | 许可证、advisory、禁用 crate(`bans`) |
| `cargo xtask deps` | `tools/xtask` | 依赖方向与"tokio 不进 hot 闭包"等闭包检查([tools](../tools/README.md#cargo-xtask-deps)) |
| `cargo xtask hot-check` | `tools/xtask` | 基于 `syn` 的源码检查:`dyn`、浮点类型、`unsafe` 白名单、指定 API 的调用点白名单、`#[allow]` 白名单、Cargo profile |
| 编译期断言 | 类型定义旁 `const _: () = assert!(...)` | 布局、大小、对齐、偏移 |
| 测试/基准 | `hft-testkit`、[benchmarks/](../benchmarks/README.md) | 计数分配器、系统调用审计、延迟门槛 |
| 启动自检 | `apps/hft-core` 启动期,结果写进首条 `HostProfile` 事件 | 绑核、大页、mlock、CPU 特性;`Live` 模式下任一项不满足拒绝启动 |

`#[allow(clippy::disallowed_*)]` 只允许出现在 `tools/xtask/allow-list.toml` 登记的"文件 + 条目"上,每条带理由;`hot-check` 拒绝未登记的 allow。

## 线程(T)

| # | 规则 | 检查方式 |
|---|---|---|
| T1 | 线程布局由 hft-core 的 `[[threads]]` 线程计划决定([hft-core 线程计划](../apps/hft-core/README.md#线程计划)):feed 任务(`feed-bbo:<venue>`、`feed-md:<venue>`,`dev2`/`c4` 上可为 `routes = All` 的单一任务)与 `ext:<chain>` 可以按计划合并到同一线程轮转(受 T8 约束),`sequencer` 与每个 `gw:<venue>` 永远独占线程(T8)。升级主机类(`c8`/`c16`/`c32`/`metal`;统一升级主机的选定见 [latency-budget §7](latency-budget.md#7-主机类与验收口径))上 prod 的每个热线程绑一个隔离核、`SCHED_OTHER`、每个隔离核只有一个可运行线程、`idle = Spin` 忙等;同机的 sim 实例只用非隔离核,永不占 prod 的热核;隔离核列表(`isolcpus`/`nohz_full`/`irqaffinity`/`AllowedCPUs`)由线程计划生成,不手写;`dev2`/`c4`/`c8` 不作延迟验收。不用 `SCHED_FIFO`([02 §4.2](../design/02-硬件与托管方案.md))。库 crate 不创建线程,线程装配只在 `apps/hft-core` | 启动自检(`sched_getaffinity` 为单核且属于隔离集合;sim 实例的任何线程核落在隔离集合 → 拒绝启动);deploy 生成器从线程计划导出核列表并比对;`hot-check`:`std::thread::spawn`/`Builder::spawn` 只允许出现在 `apps/*` |
| T2 | 热**任务**(`poll_once`/`step` 内部)永不 `park`/`await`/`sleep`,不做阻塞调用、不自旋等待(环满也不等,见 hft-venue);线程层面的让出只允许在 hft-rt `TaskLoop` 的空闲处理里、按线程计划的 `IdlePolicy` 发生——`Spin` 线程永不让出;`SpinThenPark`/`Park` 线程在**全部任务连续空闲**后经 `Parker` park(futex 或 epoll);**feed/gw 输出环满的轮次按空闲处理**(带超时 park,`dev2` 不为等环忙等;规则归 [hft-venue](../crates/hft-venue/README.md)),唤醒协议见 [hft-rt](../crates/hft-rt/README.md#线程计划的执行任务轮转与空闲策略) | clippy `disallowed-methods`:`std::thread::sleep`、`std::thread::park`、`std::thread::park_timeout`、`std::sync::Condvar::wait*`、`std::sync::mpsc::Receiver::recv*`、`std::io::stdin`;`cargo xtask deps`:`tokio`、`async-std`、`mio`、`futures-executor` 不在任何 hot crate 的依赖闭包里 |
| T3 | 单写者:每个状态只有一个写线程;跨线程只经 [`hft-rt`](../crates/hft-rt/README.md) 的 SPSC 环(`Spsc`/`ShmSpsc`/`ByteSpsc`)或单槽所有权交接 `Mailbox<T: Send>`(warm → sequencer 的大对象,如解析好的参数)。唯一允许的跨线程共享可变量:每 gw 一个 `KillWord`(全系统 `CachePadded<AtomicU32>`,位布局 `KillBits` 归 [hft-types](../crates/hft-types/README.md);hft-risk 只输出每个 gw 的 `KillBits`,由 sequencer 的 LiveDriver 写原子字,gw 读,判定含按账户 HARD 位——hft-risk 不依赖 hft-rt;hft-core 启动自检经真实 store/load 逐位验证)、sequencer 心跳计数、journal `committed_len` | clippy `disallowed-types`(hot):`std::sync::{Mutex, RwLock, Arc}`、`std::rc::Rc`、`parking_lot::*`、`crossbeam_channel::*`、`std::sync::mpsc::*`;`hot-check`:`Atomic*` 类型只允许在 `hft-rt`、`hft-journal` 内定义;`KillWord` 的元素类型与 `KillBits` 的位宽一致(编译期断言 `size_of::<KillBits>() == 4`) |
| T4 | 热线程不读系统时钟。TSC(aarch64 为 `CNTVCT_EL0`)只在 feed/gw/sequencer 循环里读,随事件传递;策略、风控、OMS、簿、记账不读任何时钟,时间来自事件与 `Timer`。**TSC 量不跨开机比较**:持久状态里的时刻存 `engine_ns`;进快照的 TSC 只允许 D6 登记的行,恢复时的处置见 D6([defensive-patterns](defensive-patterns.md#tsc-不跨开机比较)) | clippy `disallowed-methods`:`std::time::Instant::now`、`std::time::SystemTime::now`、`chrono::Utc::now`、`chrono::Local::now`、`quanta::Instant::now`、`quanta::Clock::now`、`libc::clock_gettime`;`cargo xtask deps`:L1/L2 crate 不依赖 `hft-rt` |
| T5 | **不做任何"每 tick 空跑下单路径"**(sequencer 侧与 gw 侧都不做),scratch TLS 预热(`Conn::warm`)默认关闭。可选扩展:当 `seq/bbo_fire` 基准的冷/热差与生产 `decide`、`egress` 直方图证明预热有收益时加回;加回时必须是**无副作用路径**(不写 journal、不动令牌与槽、不推进任何 TLS 序号),并以 Agent Note 记录测量数据 | 基准 `seq/bbo_fire` 冷热对照(见 [benchmarks](../benchmarks/README.md#路径基准与门槛));配置项默认值检查(`hot-check` 校验 warm 开关默认 `false`);人工评审 |
| T6 | **不对真实 TLS 连接做投机式预加密**。TLS 1.3 记录序号即 nonce,加密了不发就无法丢弃([13 R1](../design/13-设计评审与优化.md))。真实连接的加密只经 `Conn::send_frame`;`Conn::warm` 只用独立的 scratch 会话 | `hot-check`:rustls 的记录加密入口(`WriteTraffic::encrypt` 等)调用点只允许在 `crates/hft-net/src/tls.rs`(真实会话与 scratch 会话的驱动集中于此);测试 `hft-net::tls_seq_advances_only_on_send`(warm N 次后 send,对端解密成功) |
| T7 | 热线程的系统调用只限下表(**允许表**),其余一律不做:<br>• **sequencer**:无(journal 写 RAM 段是内存拷贝)。<br>• **gw**:`recvmsg` 读本线程连接;`sendmsg`/`writev` 写订单帧;写本连接的控制帧(Pong、交易所要求的应用层 ping、订阅/退订帧);REST 型操作(死手刷新、`Unknown` 查询)的请求与读取。<br>• **feed**:`recvmsg` 读本线程连接;`sendmsg` 写本连接的控制帧(同上)。**承载 ≥ 3 个 socket 的 `Spin` 行情线程**上(就绪探测 `feed.ready_poll`,生效判定归 [hft-venue](../crates/hft-venue/README.md)),每个行情任务每次 `poll_once` 至多调用一次 `epoll_pwait(epfd, …, timeout = 0)`(不阻塞),只对就绪的 socket 调 `recvmsg`;非阻塞读返回短读(读到的字节 < 缓冲)即视为队列已空,先解帧,不再补一次空 `recvmsg`(规则归 [hft-net](../crates/hft-net/README.md))。**feed 线程不发、不读、不解析行情 REST**:BBO 补帧(以及启用全量增量深度时的 L2 快照)由 warm 任务 `snap-fetch`(hft-venue `SnapFetchTask`)请求、读取、解密并解析,经 `snap-<venue>-bbo` / `snap-<venue>` 环交回 `feed-bbo` / `feed-md`;单次解析预算是 warm 预算,归 [hft-venue](../crates/hft-venue/README.md)。<br>• **TCP/TLS 握手、WS 升级、登录**只在 warm 核的 `conn-builder` 线程,热线程只接收已就绪的连接。<br>• **`SpinThenPark`/`Park` 线程(按线程计划)**:park 前后的 `futex`/`epoll_wait`,以及**非 `Spin` 生产者**在消费者已 PARKED 时的一次 `futex_wake`/`write(eventfd)`;只在 park 状态边界出现。<br>• **`Spin` 线程上不出现任何唤醒系统调用**:`Spin` 生产者写给 `Park`/`SpinThenPark` 消费者的环与邮箱(如滚动后交出的旧段句柄、反向交还的旧配置)一律 `wake = None`,消费者靠 park 超时(`park_us`)发现新数据([hft-rt 唤醒协议](../crates/hft-rt/README.md#线程计划的执行任务轮转与空闲策略))。<br>**`sd_notify` 不在热线程**:sequencer 只递增心跳计数,housekeeping 每 1 s 确认计数前进后才 `sd_notify`([13 R7](../design/13-设计评审与优化.md)) | clippy `disallowed-methods`:`sd_notify::notify`、`libsystemd::daemon::notify`(全仓),只在 `apps/hft-core/src/housekeeping.rs` 登记 allow;`hot-check`:rustls 握手驱动与 `connect` 调用点只在 `conn-builder` 模块,REST 快照的请求与解析调用点只在 `snap-fetch` 模块;hft-core 装配期断言:`Spin` 线程上的生产端 `set_wake` 一律为 `None`;参考机运行路径基准期间用 bpftrace 挂 `tracepoint:raw_syscalls:sys_enter`,按线程名(`comm`)与系统调用号计数,线程名到角色(sequencer / gw / feed / park 线程)的映射取自运行时的 `[[threads]]`(不按名字前缀猜),对照本条允许表(`Spin` 行情线程的 `epoll_pwait` 只允许 `timeout = 0`),出现表外调用即红(`cargo xtask bench-gate --syscall-audit`;不用 strace——ptrace 会改变被测时序) |
| T11 | `TCP_QUICKACK`:**默认关**(每次有数据的读后补设需要一次额外 `setsockopt`,≈ 0.2–0.5 µs)。只有在按角色(行情 / 下单)做完 24 h A/B、直方图证明 `order_rtt` 或 `kernel_rx` 有收益后才可对该角色打开;打开时同一改动把 `setsockopt(TCP_QUICKACK)` 加进 T7 允许表,并在 Agent Note 记录 A/B 数据 | 配置项 `quickack_after_read` 默认 `{false, false}`([hft-net](../crates/hft-net/README.md));`--syscall-audit` 在其关闭时把 `setsockopt` 视为表外调用 |
| T8 | 每个下单 venue 的 `gw:<venue>` 独占一个线程,**所有主机类无例外**(`dev2` 也一样;升级主机上另独占一个隔离核);同一线程不同时解析行情与发单([13 R3](../design/13-设计评审与优化.md))。核不够 → 选更大的主机类,不合并 feed 与 gw。每个 venue 的 BBO 流(bookTicker 类)与其他行情流用**不同连接**,feed 分别写 `bbo` 环与 `md` 环;升级主机类的 prod 实例上 BBO 连接在 `feed-bbo:<venue>` 任务(可跨 venue 合并到同一线程),成交、标记价/指数价、资金费与分档深度(TopN)连接在 `feed-md:<venue>` 任务(全量增量深度是可选扩展,启用时也在这里),**`feed-bbo:*` 不与 `feed-md:*` 同线程** | 类型:`GwRuntime` 只接受 `OrderConn`,`FeedRuntime` 只接受 `FeedConn`(不同类型,见 [hft-venue](../crates/hft-venue/README.md));hft-core 加载期校验 `GwSharesThread` 与 `BboSharesWithMd`(退出 2;适用的主机类与实例归 [hft-core 线程计划](../apps/hft-core/README.md#线程计划)的校验清单) |
| T9 | sequencer 内只做 O(1) 比较与状态迁移(执行层每事件 O(腿数));周期性重计算(分位数、跨币打分与分配、资金费统计、深度 VWAP、冲击曲线)放该策略的 `Model`,在 advisor 线程运行,以 `Advice` 事件回来([13 R10](../design/13-设计评审与优化.md)) | 基准:`seq-decide` 的 `decide` p99;人工评审("这段代码的最坏情况是否与簿深/名单长度成正比") |
| T10 | 所有循环有界:输入环每轮每环最多取 K 条([13 O9](../design/13-设计评审与优化.md);优先级最低的 `rep-sim` 环的 K 为 `sim_max_per_round`,模拟负载不挤占实盘;md 环逐条处理,每轮另受时间片限额,参数归 [hft-sequencer](../crates/hft-sequencer/README.md));遍历只针对预分配容器;没有"直到满足条件为止"的扫描。**输出扇出有界**:OMS/风控处理一个事件最多发 K 条 `OutMsg`(配置项),剩余由 sequencer 写入 Input 记录 `Timer{OmsContinue}` 在后续轮次推进;固定容量的结果缓冲按构造上界预分配(如 `ModeOutcome`,容量取 `max_cells_total`),不会溢出;投递给策略的通知超出 sequencer 通知队列时不丢弃,经续推定时器(`Timer{Sys, NotifyContinue}`)在后续轮次处理;kill 优先用 venue 级 `CancelAll` 而不是逐单撤 | DST 极端输入用例(价差 10⁵ %、空簿、满槽);人工评审;对应事故见 [defensive-patterns](defensive-patterns.md#所有循环有界看门狗看的是进展) |
| T12 | **收包只用中断驱动,IRQ 在专用 IRQ 核**(升级主机类 `c8`/`c16`/`c32`/`metal`):交易网卡(ENI-B)全部队列的 IRQ 与软中断独占一个核(核 1,不放任何线程——IRQ 核"保持唤醒"只允许 [hostprobe](../tools/README.md#hostprobe) 登记的形式——并列入 `rcu_nocbs`),管理网卡(ENI-A)与系统在核 0,`irqaffinity` 只含这两个核;sim 实例用独立网卡 ENI-C,其队列 IRQ 钉在线程计划给出的 `eni_c` 核(不是 prod 热核,也不是 ENI-B 的 IRQ 核;各主机类的核号归 [hft-core 线程计划](../apps/hft-core/README.md#线程计划))。`c32` 另**预留**核 28 为第二 IRQ 核、第二块交易网卡 ENI-B2 为批量流网卡(默认不启用;启用后延迟敏感流(BBO、下单、用户流、签名 REST)留在 ENI-B,深度、成交、标记价与行情 REST 走 ENI-B2,ENI-B2 不进任何 key 白名单;启用条件与划分归 hft-core 线程计划)。热线程只做非阻塞 `recvmsg`,不做 NAPI 轮询;**busy poll 全关**:`net.core.busy_poll`、`net.core.busy_read`、ENI-B 的 `napi_defer_hard_irqs` 与 `gro_flush_timeout` 都为 0,conn-builder 建连时对每个 socket 显式 `setsockopt(SO_BUSY_POLL, 0)`(取值与调优脚本归 [deploy](../deploy/README.md))。`dev2`(2 核,单网卡,与 cdex 正式账户共机)不改 IRQ 亲和(会影响同机 cdex),见 [deploy dev2 护栏](../deploy/README.md#dev2-护栏)。`nohz_full` 默认只给 sequencer 核;feed/gw 核是否加入 `nohz_full` 以 24 h A/B 决定 | [hostprobe](../tools/README.md#hostprobe) 把上述四项网络参数与 IRQ 落点列为 hard 校验(不符 → hft-core 退出 3),含 NVMe 等 managed IRQ 的 `effective_affinity` 不落在 IRQ 专用核;每条连接落在哪块网卡(sim 连接不落在 ENI-B)由 conn-builder 在交接前经 `SO_INCOMING_NAPI_ID` 与 netdev netlink 自检(hostprobe 不做连接级检查,规则归 [hft-core 主机资源隔离](../apps/hft-core/README.md#主机资源隔离slice核与出口-ip));hft-core 启动自检中的 `IRQ_ON_HOT`(任一 IRQ 可落到热核)保持为真告警,`HOT_NOT_RCU_NOCB` 与 `TASK_ON_HOT` 同样覆盖 IRQ 专用核([hft-rt](../crates/hft-rt/README.md));A/B 结果写 Agent Note |
| T14 | warm 任务(ctl-gateway、conn-builder、`snap-fetch`、`acct-fetch`、advisor、sim-gw)也是 `poll_once` 形态:gateway、`snap-fetch` 与 `acct-fetch` 把待等的 fd 交给线程的 epoll(`wait_fds`),advisor 每次分片 ≤ 1 ms(一个 `Model` 的单次 `on_event` 超过分片时由模型自己切分),sim-gw 每轮处理的订单数有上限。加载期校验线程计划:persist(含段预创建与回收)永远独占线程,升级主机类上独占一个非隔离核、不在核 0/1、不与 advisor 同核;housekeeping 不与可阻塞任务同线程、nice 0;`dev2` 的 `WatchdogSec` ≥ 10 s | hft-core 加载期校验(不符拒绝启动,见 [hft-core 线程计划](../apps/hft-core/README.md#线程计划));基准 advisor 分片时长 |
| T13 | gw 循环每 poll 完**一个** socket 就检查一次出环,不等整圈;UserStream 连接**每圈** poll,每解一帧回报就 `drain_out()` 一次;standby 连接每个 aux 圈(每 `gw.aux_every` 圈)poll 一次;REST 连接只在有在途请求时 poll,另每 `gw.rest_idle_check_ms` 做一次存活检查(gw 不用 `epoll_pwait(0)`,T7 不变)。feed 的 BBO 路由每连接每次 poll 至多 `feed.bbo_frames_per_poll` 帧,连接之间按帧轮转;非 Bbo 路由每连接至多 `feed.frames_per_poll` 帧;`routes = All` 的 feed 任务(只用于无延迟验收的 `dev2`/`c4`)在路由之间简单轮转(配置归 [hft-venue](../crates/hft-venue/README.md)) | 基准 `gw-egress/idle8`(8 个空闲 socket 时 `OutMsg` 被取走的等待)与 `gw-ingest/poll-cadence`(回报帧被读到一侧的节拍),见 [benchmarks](../benchmarks/README.md#路径基准与门槛);hft-venue 单元测试("同线程另一条 BBO 连接突发时,一条 BBO 帧最多等其他连接各 `bbo_frames_per_poll` 帧"、"回报突发时对冲单不排在整批回报之后");人工评审 |

## 策略 SDK 与执行层(S)

适用于 `hft-strategy-sdk`、`hft-exec`、`hft-sequencer` 的策略分派,以及 `strategies/*`。各公开入口的预算与基准名写在对应 README 的"延迟与资源契约"里。

| # | 规则 | 检查方式 |
|---|---|---|
| S1 | 快速部分唯一热路径入口是 `Strategy::on_event`;策略经 `StrategySet` 生成的枚举 `match` 静态分派,不用 trait object;`on_event` 不分配、不阻塞、不读系统时钟、不 panic、无 f64 | `hot-check` 对 `strategies/*` 快速部分应用 hot 规则集;`StrategySet` 的分派基准(见 [hft-strategy-sdk](../crates/hft-strategy-sdk/README.md)) |
| S2 | `Ctx` 是固定容量缓冲,满返回 `CtxFull`;策略把它当作本事件的失败处理,不在同一事件内重试;sequencer 在每个输入事件后排空 | 单元测试"Ctx 满时不越界、不丢已接受的意图";DST |
| S3 | 视图(`Views`)只读借用 sequencer 拥有的状态,策略不跨事件持有引用;时间只来自 `Views::now()`(事件时间),不是系统时钟 | 借用检查(视图生命周期绑定到一次 `on_event`);T4 的 clippy 禁用表 |
| S4 | 执行层每个相关事件(BBO、成交、定时器)对每个活动目标推进一次,复杂度 O(腿数)(≤ 4),不扫描全部目标;每次决定写一条 `ExecDecision` Output | 基准 `exec_step`(见 [hft-exec](../crates/hft-exec/README.md));DST 极端输入下单事件处理时间有上界(T10) |
| S5 | 策略 `Model`(慢速部分)在 advisor 线程运行:允许分配与 f64;禁止 I/O、系统时钟(T4)、线程、未播种随机(D2)、全局可变状态(D3)、迭代顺序依赖(D4);只读 journal 事件(预热数据也只经 `InjectWarmup` 记录进入),输出 `Advice` 载荷 ≤ 4096 B、不含浮点;单次计算按 T14 分片;状态布局变化必须递增 `Model::LAYOUT_VERSION`(检查点键的一部分,见 [hft-strategy-sdk](../crates/hft-strategy-sdk/README.md)) | `hot-check` 对 `Model` 模块应用 warm + D 规则集;模拟重演"同一 journal 两次运行 `Advice` 逐字节相同" |
| S6 | 报价可用性只按 BBO 连接健康判定(`BboView`:该 inst 的成员连接中至少一条 Up、`resync = false`、`now − conn_live_tsc ≤ conn_liveness_ms`),盘口不变不算过期;执行层、风控、OMS 不用最近一条行情的接收时刻(`Top.rx_tsc`、BBO 年龄)判报价过期(策略可把它用作信号,但不作为下单可用性判断)([defensive-patterns](defensive-patterns.md#安静--过期报价可用性看连接健康)) | DST 必过"第二腿盘口 10 s 不变";`replay-verify` 中无 `LINK_STALE` 出现在连接健康的区间;人工评审 |

## 内存(M)

| # | 规则 | 检查方式 |
|---|---|---|
| M1 | 启动期预分配一切(对象池、订单槽、环、解析缓冲、簿数组、直方图);热路径零分配。`Vec::push` 超容量视为 bug(`debug_assert!(len < capacity)`)。全局 mimalloc 设 `purge_delay = -1`(不在运行期把页还给内核,避免热线程上的 `madvise`) | 测试:`hft-testkit::CountingAlloc` 全局分配器;mock 回环与路径基准在预热后断言分配次数 = 0;`hot-check` 校验 mimalloc 选项 |
| M2 | 显式 hugetlbfs 承载簿、环、journal 热段(页大小与 tmpfs 热段的实例按主机类归 [hft-core 内存表](../apps/hft-core/README.md#内存与-journal-段按主机类唯一归属));`mlockall(MCL_CURRENT\|MCL_FUTURE)`;THP:aarch64(Graviton,基准形态)用 `madvise`,可选的 x86 剖面关闭([02 §4.3](../design/02-硬件与托管方案.md));启动时预触摸全部页 | 启动自检写进 `HostProfile`;[hostprobe](../tools/README.md#hostprobe) 校验内核参数 |
| M3 | **热 journal 只写 RAM 段**:hugetlbfs 或 tmpfs(`/dev/shm`)上的**具名文件**,启动预触摸。热写者不碰磁盘文件系统上的文件型 `mmap`——回写后重新写保护会触发 `page_mkwrite`,遇 stable pages 要等 I/O(ms 级)。持久化由 persist 线程(独占非隔离核,T14)顺序复制并 `fdatasync`([13 R2](../design/13-设计评审与优化.md))。不用匿名 `memfd_create`:进程崩溃时它随进程释放,"崩溃不丢"不成立 | `hft-journal` 的写者构造函数检查 `fstatfs` 魔数为 `HUGETLBFS_MAGIC` 或 `TMPFS_MAGIC`,否则返回错误;测试 `hft-journal::writer_rejects_disk_backed_segment`、`hft-journal::segment_survives_writer_crash`;`hot-check`:`memmap2::MmapMut` 只允许在 `hft-journal` 内 |
| M4 | 布局显式:`#[repr(C)]`/`#[repr(align(64))]`;**每个 `#[repr(C)]` 结构写出显式 `_pad` 字段**(无隐式填充),并在类型旁写 `size_of`/`align_of` 与关键字段 `offset_of` 的**编译期断言**——这是规则,不是建议;高频写字段与只读字段分到不同缓存行;SPSC 头尾游标各占一行 | 编译期断言(缺断言由 `hot-check` 报告:`#[repr(C)]` 结构旁无 `const _: () = assert!(size_of::<T>() == …)` 即失败);zerocopy derive 对隐式填充报错;人工评审 |
| M5 | 数组索引(`InstId`、`slot_id`、tick 索引)优先;hot/pure crate 禁 `HashMap`/`HashSet`/`BTreeMap` 于运行期路径(分配与迭代顺序两个理由)。需要映射时用启动期构建、运行期只读的有序表或预分配开放寻址表 | clippy `disallowed-types`(hot、pure):`std::collections::{HashMap, HashSet, BTreeMap, BTreeSet}`、`hashbrown::*`;启动期构建的例外登记在 allow-list |
| M6 | 事件体与环槽里无 `String`/`Vec`/`Box`/引用/指针。事件体 derive `zerocopy::{IntoBytes, KnownLayout, Immutable}`,读取方向:全整数字段的体用 `FromBytes`,**含枚举字段的体用 `TryFromBytes`**(非法判别值读出为错误,不是未定义行为)。**尾部追加的字段不得是枚举**,除非 0 是该枚举合法的"未提供"值——旧记录按 `len` 补零后必须仍能通过校验。跨进程的 `ShmSpsc` 元素必须是 `FromBytes`(对端不可信,任何字节都是合法槽值,结构校验在上层解码);进程内 `Spsc<T: Copy>` 不加 zerocopy 约束 | 编译期:zerocopy derive,含指针即编译失败;`hot-check`:含枚举字段的事件体若 derive `FromBytes` 即失败;`hft-event` 测试 `reader_accepts_short_and_long_records` 覆盖补零后的尾部字段 |
| M7 | 启动期共享的只读数据用 `&'static`(启动期 `Box::leak`),不用引用计数 | 同 T3 的 `disallowed-types`(`Arc`、`Rc`) |

## 数值(N)

| # | 规则 | 检查方式 |
|---|---|---|
| N1 | 价格/数量是 [`hft-types`](../crates/hft-types/README.md) 的 `Px`/`Qty`:i64 定点,`#[repr(transparent)]`,**指数按资产**(同一资产所有 venue 同一 `px_exp/qty_exp`,写在 refdata);交易所 tick/lot 是该指数下的整数步长([13 R4](../design/13-设计评审与优化.md)) | 类型系统(newtype 不暴露裸 i64 算术);refdata 加载期校验指数一致,不一致拒绝启动;属性测试 |
| N2 | 乘积、名义、PnL 用 i128 或 `Notional`(USDT 1e-6 的 i64);溢出显式处理(checked 运算 + 错误位),不依赖 wrap | `hft-types` 内 `#![deny(clippy::arithmetic_side_effects)]`;CI 剖面 `overflow-checks = true`(release 关闭);属性测试覆盖极值(PEPE 类 1e12 数量 × 1e-10 价格) |
| N3 | 下单路径禁 `f32`/`f64`:`hft-types`、`hft-strategy-sdk`(唯一例外:其 `model_math` 模块,只供 `Model` 使用,`hot-check` 确认它只被策略的 `src/model/` 引用)、`hft-oms`、`hft-risk`、`hft-exec`、`hft-strat-lib`(例外:其 `src/model/`,只供 `Model` 使用,`hot-check` 确认它只被策略的 `src/model/` 引用)、`venue-*` 的编码与解码、`hft-ledger`、`strategies/*` 的快速部分。浮点只允许在策略 `Model`(advisor)、冷进程与研究;`Advice` 载荷不含浮点(`Model` 输出前换算为定点整数,快速部分解码不碰 f64) | 上述 crate 与模块 `#![deny(clippy::float_arithmetic)]`(两处例外模块除外);`hot-check`:上述位置(豁免 SDK `model_math` 与 `crates/hft-strat-lib/src/model/`)的类型出现 `f32`/`f64` 即失败 |
| N4 | 禁 `rust_decimal`、`bigdecimal`、`num-bigint` 于 hot/pure;链上 `U256` 只在 `hft-amm`/`hft-chain-evm` 内部,进入 sequencer 前按资产指数换算一次 | `cargo deny` `bans`;`cargo xtask deps`(`alloy-primitives` 只允许出现在 `hft-amm`/`hft-chain-evm`/`hft-py`/`apps/chain-core` 闭包) |
| N5 | 交易所数字字符串 → 定点直接解析(不经浮点);JSON 数字按字符串借用 | 金样 + fuzz 差分测试(对照十进制参考实现,见 [testing](testing.md)) |
| N6 | 编译期分派:`enum` + `match`,交易所差异用泛型/`const` 参数;hot/pure crate 不用 `dyn Trait` | `hot-check`:类型位置出现 trait object 即失败(启动期构建器登记例外) |
| N7 | JSON 先按固定偏移/`memchr` 取分派字段，使用字段可直接借用字节投影；其余形态用 `sonic-rs` 专用借用结构体；保持使用字段严格校验，禁通用值类型 | clippy `disallowed-types`:`serde_json::Value`、`sonic_rs::Value`、`simd_json::OwnedValue` |

## 控制流(C)

| # | 规则 | 检查方式 |
|---|---|---|
| C1 | 热路径无 panic:无 `unwrap`/`expect`/`panic!`/`unreachable!`/`todo!`/`unimplemented!`/可越界的切片索引;`panic = "abort"`(单热进程里任何 panic 即全停) | hot/pure crate `#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unreachable, clippy::todo, clippy::unimplemented)]`;`venue-*` 与 `hft-event` 解码额外 `clippy::indexing_slicing`;`hot-check` 校验 profile 的 `panic = "abort"`;fuzz |
| C2 | 错误聚合成位标志,每轮循环末尾一次判断;错误不经 `Result` 链层层展开到热循环外 | 人工评审 |
| C3 | 冷操作(日志、指标、告警)只写定长记录(`Note`:kind + 8×u64;`LatencySample`)经环或 journal 发出;不格式化字符串 | clippy `disallowed-macros`(hot):`std::format`、`std::println`、`std::eprintln`、`std::dbg`、`log::*`、`tracing::*` |
| C4 | 依赖冷配置的分支在启动期特化(泛型、`const`、启动期选定的函数表);热路径不做字符串比较 | 人工评审 |
| C5 | 廉价判断在前:风控检查链按廉价 → 昂贵排序,首个失败即返回 | 基准 `risk-chain`;人工评审 |
| C6 | 传输错误(超时、403/429/5xx、断连)只能把订单或交易推入 `Unknown`,不改变仓位 | OMS 与 `TxState` 属性测试;规则归属见 [defensive-patterns](defensive-patterns.md#传输错误--业务失败) |

## 确定性(D)

适用于 sequencer 可达的全部代码(L1、L2、`hft-sequencer`、`strategies/*` 的快速部分),是验证回放逐字节一致的前提;策略 `Model` 同样适用 D2–D4(它的输出 `Advice` 是 Input,回放读日志,但模型自身必须确定,模拟重演与排障才能复现)。

| # | 规则 | 检查方式 |
|---|---|---|
| D1 | 影响决策的每个输入都先成为 journal 事件:行情、回报(含模拟账户的回报)、命令(含预热数据 `InjectWarmup`)、`Timer`、`Advice`、`ConfigBlob`(配置**内容**,不是路径或哈希) | `cargo xtask replay-verify`;DST |
| D5 | **记录分三类**:Input(喂给状态机的)、Output(状态机产生、验证回放逐字节比对的)、Meta(测量与环境描述:`LatencySample`、`Note{Measure}`、`ClockStatus`、`HostProfile`)。Meta 占 `seq` 但**不进入状态、不参与验证比对**,引导块比对也屏蔽 Meta。**依赖运行环境的决定**(何时写快照、何时滚动段、何时写关键帧、何时采样)由 live driver 当场写成 `Timer` Input 记录(名目归 [hft-sequencer](../crates/hft-sequencer/README.md)),回放照读,不在回放时重新判断 | 分类表归 [event-model](subsystems/event-model.md);`replay-verify` 对 Meta 做差异注入测试(改动 Meta 内容不影响比对结果);DST |
| D2 | 随机数显式播种,种子来自事件;不用线程随机源 | clippy `disallowed-methods`:`rand::thread_rng`、`rand::rng`、`rand::random`、`getrandom::getrandom` |
| D3 | 无全局可变状态:无 `static mut`、运行期写入的 `OnceLock`/`thread_local!` | clippy `disallowed-macros`:`std::thread_local`;`static_mut_refs` 设为 deny;人工评审 |
| D4 | 无迭代顺序依赖 | 由 M5 的类型禁令覆盖;`replay-verify` |
| D6 | **持久状态不存 TSC**:进快照或检查点的时刻与期限(确认窗口起点、节流、再入场冷却、保证金截止等)存 `engine_ns`(SDK `Now.engine_ns`,跨重启连续、停机不计,sequencer 由 journal 的运行区间累加)或 UTC 日历量;TSC 只在单次启动内比较。**唯一例外**是登记在 allow-list 的 TSC 行,共三类:① OMS、exec 与定时器堆及其身份登记(快照节 8、27)的行,恢复时按 `recovery_end` 平移(例外:OMS 节 3 头的 `resume_hold_until` 在恢复时重新设定、不平移,[hft-oms](../crates/hft-oms/README.md);节 21 Gate 行的 `gate_closed_tsc` 由 `recovery_end` 重设为当时的 TSC、不平移,[hft-exec](../crates/hft-exec/README.md#快照与恢复);停机不计;平移公式唯一定义在 [hft-exec 快照与恢复](../crates/hft-exec/README.md#快照与恢复),`recovery_end` 的取值归 [hft-sequencer](../crates/hft-sequencer/README.md));② 连接活性行(节 26 `ConnLiveRow.live_tsc`),只含本次运行的值,冷启动恢复结束时作废,等连接重建后重新取得;③ 节 16 `ClockCalRow.tsc`,是该快照所在运行的校准对拷贝,只供冷读者换算 UTC,不计入 `state_hash`、sequencer 不读、不平移。节 7 的行(hft-book `TopRow`)不含 `rx_tsc`,不在 allow-list 里(恢复后 `Top.rx_tsc` 为 0)。不按 `boot_id` 或 UTC 重建([defensive-patterns](defensive-patterns.md#tsc-不跨开机比较)) | `hot-check`:快照节的行类型中出现 `Tsc` 字段即失败,除非登记在 allow-list(只允许上述三类);DST"停机 N 小时后恢复"与"换机恢复" |

## 构建(B)

| # | 规则 | 检查方式 |
|---|---|---|
| B1 | Cargo 剖面(`release`、`release-hot`、`ci`、`bench`)的取值唯一归属 [development 构建剖面](development.md#构建剖面),本规则不复写取值;`release-hot`(fat LTO、单 codegen unit)只用于正式版的 `hft-core` 与 `kill-switch` 两个二进制,其余二进制与开发构建用 `release` | `hot-check` 解析根 `Cargo.toml`,与 development 表逐项比对 |
| B2 | 目标三元组与 `-C target-cpu` 只按 [development#目标-cpu](development.md#目标-cpu) 设置(本规则不复写取值);hft-core 启动时检查 CPU 特性是发布构建要求的超集,否则拒绝启动 | `hot-check` 校验 `.cargo/config.toml` 与 development 表一致;启动自检 |
| B3 | PGO 用真实 journal 的验证回放训练(公开数据 5–20%);PGO 构建与非 PGO 构建都过 `bench-gate` 才可发布 | 发布流水线 |
| B4 | 工具链固定在 `rust-toolchain.toml`;升级工具链是一次独立改动,PR 附 CI 上同一作业内 base/head 的相对基准,并在参考机(M1.5 起按需租用,[latency-budget §6](latency-budget.md#6-验收方法))重跑 `bench-gate --reference` 并附直方图(参考机不是 CI runner,CI 只用 GitHub 托管 runner) | CI:`rust-toolchain.toml` 变更的 PR 强制运行相对基准作业;人工评审核对参考机输出 |
| B5 | `unsafe` 只在 `hft-rt`、`hft-journal`、`hft-net`、`hft-book`;其他 crate `#![forbid(unsafe_code)]`;每处 `unsafe` 带 `// SAFETY:` | `hot-check` 校验属性;`#![deny(clippy::undocumented_unsafe_blocks)]` |
| B6 | cold 依赖(`tokio`、`tracing-subscriber`、`serde_json`、`reqwest`、`pyo3`)不进 hot/pure crate 的依赖闭包 | `cargo xtask deps` |

## 评审清单(每个触及 hot crate 的改动)

1. 新公开入口在 README "延迟与资源契约"里有一行预算与基准名。
2. 新增循环的最坏迭代次数是多少,由什么界定(T10)。
3. 新增状态的唯一写线程是谁(T3)。
4. 有没有新系统调用、新分配、新时钟读取(T7、M1、T4)。
5. 新数值字段的指数从哪来、乘积是否 i128(N1、N2)。
6. 失败时推入什么状态;传输错误是否只进 `Unknown`(C6)。
7. 新策略逻辑放在快速部分还是 `Model`:最坏情况与名单长度或簿深成正比的必须在 `Model`(T9、S5)。
8. 新增进快照的时刻字段用 `engine_ns`;若必须是 TSC,是否已在 D6 登记并写明恢复时平移还是作废;新的"报价是否可用"判断是否走连接健康(S6)。
9. 新的跨线程环或邮箱:生产者是 `Spin` 线程时 `wake = None`(T7)。
