---
description: "feed/gw 线程用它在非阻塞 TCP 上收发 TLS 1.3 + WebSocket 帧并拿到 HW/SW 时间戳;warm 线程 conn-builder 用它建连与握手,snap-fetch 用它的 REST 池取快照"
kind: crate
tier: hot
scope: core
status: spec
---

# hft-net

## 概要

`hft-net` 拥有**一条连接从 socket 到 WS 帧的全部字节**:非阻塞 TCP socket 与其选项、TLS(握手用 rustls 0.23 unbuffered API;握手完成后转成 rustls kernel connection,由本 crate 的记录层用 aws-lc-rs AEAD 在我们自己的缓冲上原地解密/加密)、WS 解帧与控制帧、`recvmsg` 的 RX 时间戳、连接池(主/备/预热、轮换)、最小 HTTP/1.1 REST 客户端。它**不**懂任何交易所协议(订阅、登录、JSON 都在 [`hft-venue`](../hft-venue/README.md) 与 `venue-*`),不写 journal,不创建线程。热路径上它是 t1 的前半段(`recvmsg → 解密 → 解帧`)与 t4 的后半段(`掩码帧 → 加密一次 → sendmsg`,见 [architecture.md §4](../../docs/architecture.md#4-一次-tick-to-trade));建连、TLS 握手、WS Upgrade 全部在 warm 线程 `conn-builder`(apps/hft-core 装配)由 `Connector` 完成,热线程只接收"已打开"的连接。**收包是中断驱动**:网卡 IRQ 与软中断在专用 IRQ 核,热线程只做非阻塞 `recvmsg`、不做 NAPI 轮询,短读即停;`Connector` 对每个 socket 显式设 `SO_BUSY_POLL = 0`;多 socket 的行情线程可用 `ReadyPoll` 每次 `poll_once` 做一次 `epoll_pwait(timeout = 0)`,只读就绪的 socket。**不对真实连接预加密**:TLS 1.3 记录序号即 nonce,加密即必须发送;缓存预热用独立的 scratch 会话([13 R1](../../design/13-设计评审与优化.md))。

## 目录

- [使用本组件](#使用本组件)
- [实现说明](#实现说明)
- [延迟与资源契约](#延迟与资源契约)
- [测试与验收](#测试与验收)
- [依赖](#依赖)
- [延伸阅读](#延伸阅读)
- [已知限制与后续工作](#已知限制与后续工作)

## 使用本组件

可选 [latency_trace](../../tools/latency-report/README.md) 在现有 recvmsg、TLS open/seal、WS 交付和 sendmsg 边界记录测量戳；网络 API 与业务事件布局不变。RX 使用现有内核戳，未实现的 kernel TX 不伪造。

成功读取另保留已有 recvmsg pre/post、密文字节范围与最近空确认；WS 测量区分本次调用和同 TLS 记录前序消息的处理等待。sendmsg 前戳只在启用测量时读取；空轮询不新增诊断记录。字段与离线分段归上述测量工具。

### 类型总览

| 类型 | 用途 | 所有者线程 |
|---|---|---|
| `Conn` | 一条 TCP+TLS(+WS 或 HTTP)连接:fd、rustls 会话、收发缓冲、读位置、时间戳、错误位 | 当前持有它的线程(热线程、`Connector` 或 snap-fetch),经交接环转移 |
| `ConnArena` | 启动期在大页上预分配的 `Conn` 槽数组;槽号(arena 下标,不是 `ConnId`)在线程间传递 | hft-core 装配时创建,`'static` |
| `ConnPool` | 某 venue 某角色的一组连接:`Race`(行情双路)或 `PrimaryStandby`(下单) | feed/gw 线程 |
| `Connector` | TCP connect → TLS 握手 → WS Upgrade/HTTP 就绪;给出 `Open` 的连接 | warm 线程 `conn-builder` |
| `MaskedFrame` | 已写好 WS 头、已掩码的客户端帧,`send_frame` 的唯一输入 | gw 线程 |
| `Frame<'a>` | 解出的一个服务端帧:opcode + 借用接收缓冲的 payload + 时间戳 | 借用期内 |
| `Timestamps` | `hw_rx_ns / sw_rx_ns / rx_tsc / rx_empty_gap_ns` + 标志 | 值类型 |
| `ErrBits` | 连接错误/事件位,轮询末尾一次处理 | 值类型 |
| `SysCount` | 每个 `Conn`、`Connector`、`ReadyPoll` 自己的系统调用计数(按种类)与定长调用日志;零分配 | 所有者线程 |
| `NetConfig` / `ConnCfg` | 配置项的承载类型 / 一条连接保留的子集 | 启动期 |
| `RestPool` | 一个 REST 端点的 keep-alive HTTP/1.1 连接池,一连接一在途请求 | gw 线程、snap-fetch、acct-fetch |
| `ReadyPoll` / `ReadyHandle` | 行情任务的就绪探测:一个 epfd,热线程 `probe`(`epoll_pwait(timeout = 0)`),warm 线程经 `ReadyHandle` 注册与注销连接 | 行情任务 / `conn-builder` |
| `TxTsDrain` | 从 `MSG_ERRQUEUE` 取 TX 时间戳(**可选,默认关**;启用条件见已知限制) | housekeeping |
| `RssPlan` | Toeplitz 哈希 + 间接表,为目标 RX 队列挑源端口(**可选,默认关**;启用条件见已知限制) | `Connector` |

`Conn`、`ConnPool`、`MaskedFrame`、`Timestamps` 的归属见 [architecture.md §10](../../docs/architecture.md#10-核心类型归属);`ConnState` 事件体在 [event-model](../../docs/subsystems/event-model.md#关键事件体),本 crate 只产出 `ErrBits`,由 `hft-venue` 翻译成事件。`ConnId` 的分配规则(BBO 槽号与各任务的号段)归 [hft-venue §1](../hft-venue/README.md#1-线程与环由-hft-core-装配)。

### 签名草案

```rust
/// venue 内唯一的连接号(定义在 hft-types),写进 ConnState.conn、SendReport.conn 与 RawHdr.conn;分配规则归 hft-venue。
pub use hft_types::ConnId;

/// 连接角色:Md = 1、Order = 2、UserStream = 3、Rest = 4;类型与编码的唯一归属是 event-model(在 hft-event 实现),这里只重导出。
pub use hft_event::ConnRole;

#[derive(Copy, Clone, Eq, PartialEq)] #[repr(u8)]
pub enum ConnPhase { Free = 0, TcpConnecting = 1, TlsHandshake = 2, WsUpgrade = 3, Open = 4, Closing = 5, Dead = 6 }

#[repr(C)] #[derive(Copy, Clone, Default)]
pub struct Timestamps {     // 32 B
    pub hw_rx_ns: i64,  //  0 SO_TIMESTAMPING ts[2](PHC 域);0 = 不可用
    pub sw_rx_ns: i64,  //  8 ts[0] 内核软件 RX 戳(CLOCK_REALTIME);0 = 不可用;常态记录(MarketHdr、LatencySample 的 sw_rx,布局归 hft-event)
    pub rx_tsc:   u64,  // 16 recvmsg 返回后、解帧前读取的 TSC
    pub flags:    u32,  // 24 TsFlags::{HW = 1, SW = 2, MULTI_SKB = 4};MULTI_SKB = 戳不能确定是该帧最早一个包的戳(实现说明 §7)
    pub rx_empty_gap_ns: u32, // 28 本连接最近一次确认接收队列为空的时刻到 rx_tsc 的间隔,饱和到 u32::MAX;0 = 无确认(实现说明 §7)
}
impl Timestamps {
    /// 事件的 hw_rx_ns:有 HW 用 HW,否则 SW 并返回 RecordFlags::SW_TS 应置位;都没有返回 (0, false)。
    pub fn best_rx(&self) -> (i64, bool);
}

#[derive(Copy, Clone, Eq, PartialEq)] #[repr(u8)]
pub enum Opcode { Text = 1, Binary = 2 }   // 控制帧不外露

pub struct Frame<'a> {
    pub opcode:  Opcode,
    pub payload: &'a [u8],   // 指向 Conn 的连续明文区;下一次 next_frame / poll_rx 前有效
    pub ts:      Timestamps, // hw/sw 戳取该帧最早一个包所在那次 recvmsg,rx_tsc 取凑齐该帧的那次(实现说明 §7)
    pub reassembled: bool,   // 由分片拼接(发生过一次复制)
}

/// 已写好 WS 头并掩码的客户端帧。payload 写在固定偏移 HDR_MAX 处,头右对齐写在其前,零移动。
/// CAP ≤ 65 535(编译期断言;len 是 u16)。
#[repr(C, align(64))]
pub struct MaskedFrame<const CAP: usize = 2048> {
    start:   u16,        // 0 as_bytes() 起点 = HDR_MAX - hdr_len
    len:     u16,        // 2 头 + payload
    opcode:  u8,         // 4
    sealed:  u8,         // 5 0 = 正在填写;1 = 已 seal,可发送
    _pad:    [u8; 2],    // 6
    buf:     [u8; CAP],  // 8 [0..14) 头区,[14..) payload
}
impl<const CAP: usize> MaskedFrame<CAP> {
    pub const HDR_MAX: usize = 14;
    pub fn new() -> Self;                                   // 启动期;之后复用
    pub fn body_mut(&mut self) -> &mut [u8];                // 可写 payload 区,长度 CAP-14;已 seal 时调用 → debug_assert
    /// 写 WS 头(FIN=1、MASK=1,掩码键字节 = mask_key.to_be_bytes())、对 payload[..len] 原地异或掩码;seal 后 body 不得再写。
    /// len > CAP-14 或帧已 seal → 调用方错误:debug_assert,帧不变(仍未 seal 或保持原 seal)。
    pub fn seal(&mut self, op: Opcode, len: usize, mask_key: u32);
    pub fn as_bytes(&self) -> &[u8];                        // 头 + 已掩码 payload
    pub fn unseal(&mut self);                               // 复用前调用
}

bitflags::bitflags! {
    #[derive(Copy, Clone, Default)]
    pub struct ErrBits: u32 {
        // 致命(连接须退役)
        const PEER_EOF     = 1 << 0;   // recv 返回 0
        const RESET        = 1 << 1;   // ECONNRESET / EPIPE / ETIMEDOUT(TCP_USER_TIMEOUT)
        const IO           = 1 << 2;   // 其他 errno,值在 Conn::last_errno
        const TLS_ALERT    = 1 << 3;   // 收到 alert 或 close_notify
        const TLS_PROTO    = 1 << 4;   // 解密失败、记录格式错
        const WS_PROTO     = 1 << 5;   // 服务端帧带掩码、RSV 位非 0、控制帧 >125 B、未知 opcode
        const WS_CLOSE     = 1 << 6;   // 收到 Close 帧(已回 Close)
        const RX_FULL      = 1 << 7;   // 接收缓冲满且无完整帧 / 消息超 max_msg
        const TX_BACKLOG   = 1 << 8;   // 未发出的密文超过 tx 容量;不能再加密
        const PING_TIMEOUT = 1 << 9;   // 由上层(hft-venue 心跳)经 Conn::mark 置
        const IDLE_TIMEOUT = 1 << 10;  // 同上
        const HTTP_PROTO   = 1 << 11;  // REST:响应格式错,或空闲连接收到未请求的数据
        // 非致命(记录、可能触发轮换)
        const TX_PARTIAL   = 1 << 16;  // 有密文留在 tx 待 flush
        const TS_MISSING   = 1 << 17;  // 期望 HW 戳但本次没有
        const WRONG_QUEUE  = 1 << 18;  // SO_INCOMING_NAPI_ID 与计划队列不符(仅 rss_plan 启用时)
        const KEY_UPDATE   = 1 << 19;  // 处理了 TLS KeyUpdate
        const ROTATE_DUE   = 1 << 20;  // 到达计划轮换时刻
        const SERVER_HINT  = 1 << 21;  // 上层标记的服务端下线预告(如 Binance serverShutdown)
    }
}
impl ErrBits { pub const FATAL: ErrBits; /* 低 16 位 */ }

#[derive(Copy, Clone)]
pub struct SendOk { pub sent_tsc: u64, pub queued: u32 /* 仍在 tx 待 flush 的字节 */ }

#[derive(Copy, Clone, Debug)]
pub enum SendErr {
    NotOpen(ConnPhase),   // 未加密,可安全改走备用连接
    TxBacklog,            // 未加密,同上;连接同时置 TX_BACKLOG(致命)
    NotSealed,            // 帧未 seal(调用方错误);未加密
    Encrypt,              // rustls 拒绝(不可达状态);连接置 TLS_PROTO
    Io(i32),              // 已加密:密文留在 tx,连接将退役;订单须按"已发出、结果未知"处理
}

impl Conn {
    // ---- 热线程:接收 ----
    /// 非阻塞读:开头先压缩缓冲(把未消费的明文与密文移到头部),然后对全部空闲空间**至多一次** recvmsg:
    /// 短读(返回字节数 < 所给空闲空间)或 EAGAIN 视为内核队列已空;读满(缓冲已满)本次不再读。空闲空间为 0 时不做系统调用。
    /// 只把密文追加进缓冲并记下这次读取的戳;不解密。返回读到的字节数(0 = 无新数据)。phase 不是 Open 时返回 0、不做系统调用。
    pub fn poll_rx(&mut self, now_tsc: u64) -> u32;
    /// 已有完整 WS 帧或 TLS 记录时先交给 next_frame；否则调用 poll_rx。部分记录不能阻止补读。
    pub fn poll_rx_if_needed(&mut self, now_tsc: u64) -> u32;
    /// 取下一条完整帧;明文区没有完整帧时**按需**逐条解密 TLS 记录,凑齐一帧即停;不做系统调用。
    /// 控制帧在内部处理(Ping→排队 Pong,Close→排队 Close 并置 WS_CLOSE,Pong→记 RTT),不返回。
    /// 读位置存在 Conn 里:两次 next_frame 之间可以对同一连接 send_frame(gw 每解一帧回报就 drain_out 一次)。
    pub fn next_frame(&mut self) -> Option<Frame<'_>>;
    /// 发出排队的 Pong/Close 与剩余密文(sendmsg);每轮循环末调用一次。
    pub fn finish_poll(&mut self, now_tsc: u64);
    /// 调用方确认本连接接收队列在 tsc 时为空(hft-venue 的就绪探测里未就绪);只更新 rx_empty_gap_ns 的起点(§7)。
    pub fn note_empty(&mut self, tsc: u64);
    pub fn err(&self) -> ErrBits;           // 自上次 clear_err 以来累积
    pub fn clear_err(&mut self);
    /// 上层置位:只接受 PING_TIMEOUT、IDLE_TIMEOUT、SERVER_HINT;其他位 → debug_assert,忽略。
    pub fn mark(&mut self, bits: ErrBits);

    // ---- 热线程:发送 ----
    /// 对已 seal 的帧做一次 TLS 加密(推进真实记录序号)并 sendmsg;返回 sendmsg 返回后的 TSC(= sent_tsc)。
    /// 不阻塞;短写时剩余密文留在 tx,由 finish_poll/flush 续写,绝不丢弃或重加密。
    pub fn send_frame<const C: usize>(&mut self, f: &MaskedFrame<C>) -> Result<SendOk, SendErr>;
    /// 订阅、应用层 ping 等非订单文本:内部 scratch 帧 + send_frame。
    /// payload ≤ `TEXT_CAP − 14` = 8 178 B(内部 `MaskedFrame<TEXT_CAP>`,`TEXT_CAP` = 8 192);更长是调用方错误:debug_assert,`NotSealed`。
    pub fn send_text(&mut self, payload: &[u8], now_tsc: u64) -> Result<SendOk, SendErr>;
    pub fn send_ping(&mut self, now_tsc: u64) -> Result<SendOk, SendErr>;   // WS Ping,payload = now_tsc 的 8 字节小端;BBO 连接每 feed.bbo_ping_ms 一次(hft-venue §3a)
    pub fn flush(&mut self) -> Result<u32 /* 剩余字节 */, SendErr>;
    pub fn next_mask(&mut self) -> u32;     // 本连接 xorshift64* 掩码键序列的下一个值(§2 第 1 步)
    /// 缓存预热:用本线程的 scratch TLS 会话加密一个同长度的帧到丢弃缓冲;不触碰本连接的记录序号与 fd。
    pub fn warm<const C: usize>(&mut self, f: &MaskedFrame<C>);

    // ---- 查询 ----
    pub fn id(&self) -> ConnId;
    pub fn phase(&self) -> ConnPhase;
    pub fn opened_tsc(&self) -> u64;
    pub fn last_rx_tsc(&self) -> u64;       // 最近一次解出任一完整 WS 帧(数据、Ping、Pong、Close)的 rx_tsc;hft-venue 用它判 BBO 连接存活
    pub fn last_pong_tsc(&self) -> u64;     // 最近一次 WS Pong(任意 payload)的 rx_tsc;0 = 建连以来没有;hft-venue 的 ws ping 超时判定(§5)
    pub fn rx_buffered(&self) -> usize;     // 已收未消费的字节(未被 next_frame 取走的明文 + 未解密的密文);无系统调用,供上层 has_pending
    pub fn napi_id(&self) -> u32;           // 握手完成时读到的 SO_INCOMING_NAPI_ID;0 = 不可用
    pub fn last_errno(&self) -> i32;        // 最近一次置 IO/RESET 的 errno;0 = 无
    pub fn stats(&self) -> &ConnStats;
    pub fn sys(&self) -> &SysCount;         // 经本连接做的系统调用
    pub fn buf_caps(&self) -> (u32, u32);   // 生效的 rx / tx 容量(按角色)
    pub fn tx_pending(&self) -> usize;      // tx 里待写出的密文字节
    pub fn tls_info(&self) -> TlsInfo;
    pub fn raw_fd(&self) -> i32;            // 脚本化或已关闭的连接为 -1
    pub fn local_addr(&mut self) -> Result<(Ipv4Addr, u16), i32>;   // getsockname(warm 线程)
    pub fn scratch_sessions(&self) -> u32;  // warm 用的 scratch 会话数;预热关闭时恒为 0
}
pub struct ConnStats {
    pub frames: u64, pub bytes: u64,        // 解出的数据帧数与 payload 字节
    pub reassembled: u64,                   // 由分片拼接的消息数
    pub short_writes: u64,                  // sendmsg 短写或 EAGAIN 次数
    pub ts_reads: u64, pub hw_ts_reads: u64, // 读到数据的 recvmsg 次数,其中带 HW 戳的次数(HW 戳覆盖率 = 后者 / 前者)
    pub multi_skb: u64,                     // 置 MULTI_SKB 的帧数
    pub rtt: RttHist,                       // WS Ping → Pong 往返(ns)
}

pub enum PoolMode { Race, PrimaryStandby }

pub struct ConnPool<const N: usize = 4> {
    mode: PoolMode,
    slots: [Option<ConnSlot>; N],   // ConnSlot = ConnArena 槽号 + ConnId + 角色 + 计划轮换时刻
    primary: u8, standby: u8,       // Race 模式下两者都是"活跃"
    policy: RotatePolicy,
}
impl<const N: usize> ConnPool<N> {
    /// target = 期望的活跃连接数(Race 赛马 2、PrimaryStandby 2、单连接路由 1);policy 带 rotate_stagger_s 换算的 TSC 间隔。
    pub fn new(mode: PoolMode, target: u8, policy: RotatePolicy) -> Self;
    pub fn active(&mut self) -> ActiveIter<'_>;              // 固定顺序:primary 在前
    pub fn primary(&mut self) -> Option<&mut Conn>;
    pub fn standby(&mut self) -> Option<&mut Conn>;
    /// 只交换两个索引,返回 (新 primary, 新 standby);调用方(GwRuntime)保证无在途请求。O(1),无系统调用。无 Open 的 standby → NoStandby;Race → WrongMode。
    pub fn swap_primary(&mut self) -> Result<(ConnId, ConnId), PoolErr>;
    /// Connector 交来的已打开连接;rotate_at_tsc = 计划轮换时刻(由 hft-venue 按 ECF1 段 6 算出;None = 不做计划轮换)。
    /// 槽满 → Full;连接不是 Open → NotOpen。结束"在建"状态。
    pub fn install(&mut self, h: Handoff, rotate_at_tsc: Option<u64>) -> Result<ConnId, PoolErr>;
    pub fn retire(&mut self, id: ConnId, why: ErrBits) -> Result<RetireMsg, PoolErr>;   // 交回 warm 线程 shutdown+close;不在池中 → UnknownConn
    /// 需要补建或轮换时给出原因并记为"在建"(至多一条);连接号与 BuildReq 由上层按 hft-venue §1 填。
    pub fn next_build(&mut self, now_tsc: u64) -> Option<BuildWhy>;
    pub fn build_failed(&mut self);                                      // 上层报告在建失败:清除"在建"
    pub fn rotate_due(&mut self, now_tsc: u64) -> Option<ConnId>;        // 到期的连接,同时在其上置 ROTATE_DUE
    pub fn get(&mut self, id: ConnId) -> Option<&mut Conn>;              // 已装入的连接(活跃或替补)
}
pub enum PoolErr { Full, NotOpen, NoStandby, WrongMode, UnknownConn }
pub enum BuildWhy { Fill /* 活跃数 < target */, Rotate(ConnId), Hint(ConnId) /* 该连接带 SERVER_HINT */ }

/// 启动期:plan = (角色, 槽数);每槽按该角色的 rx_buf、tx_buf 与一块 max_msg 分片区一次分配并预触摸;clock 换算 TSC 差。
impl ConnArena { pub fn new(plan: &[(ConnRole, u16)], cfg: &NetConfig, clock: TscClock) -> &'static ConnArena; }

pub struct Connector { /* ConnArena 引用、rustls ClientConfig、作业表 [BuildJob; 16] */ }
impl Connector {
    /// 启动期:校验配置并建 rustls ClientConfig(`endpoint_override` 时读 extra_ca)。尚未实现的选项
    /// (`tls_mode = buffered`、`warm_interval_ns > 0`、`tx_timestamps`、`rss_plan`)→ `ConfigErr`,不静默忽略。
    pub fn new(arena: &'static ConnArena, cfg: &NetConfig) -> Result<Connector, ConfigErr>;
    /// 作业所在连接的相位(TcpConnecting … Open);作业不存在或已结束 → None。
    pub fn phase(&self, job: JobId) -> Option<ConnPhase>;
    pub fn sys(&self) -> &SysCount;                     // Connector 自己的系统调用(解析器)
    /// 作业表满 → JobsFull;arena 无空槽 → ArenaFull;endpoint 不在 endpoints[] → BadEndpoint;src_ip_idx 越界或该地址不在 bind_device → BadSrcIp。
    pub fn submit(&mut self, req: BuildReq) -> Result<JobId, BuildErr>;
    /// 推进所有作业一步(非阻塞 connect / 握手 / Upgrade);就绪或失败时返回一个事件。
    pub fn drive(&mut self, now_ns: i64) -> Option<BuildEvent>;
    /// 已 Open 的连接在 warm 线程上可直接用 Conn API(登录、订阅由 hft-venue 驱动),然后交接;作业不是 Open → None。
    pub fn conn_mut(&mut self, job: JobId) -> Option<&mut Conn>;
    pub fn handoff(&mut self, job: JobId) -> Result<Handoff, BuildErr>;   // 所有权随槽号经 SPSC 移交热线程;作业不是 Open → NotReady
    /// NAPI id → 网卡 ifindex(netdev netlink,内核 ≥ 6.9;启动期缓存,warm 线程);None = 查不到;napi_id = 0 直接 None。
    pub fn napi_ifindex(&mut self, napi_id: u32) -> Option<u32>;
    pub fn reclaim(&mut self, m: RetireMsg);            // shutdown(SHUT_RDWR) + close + 槽归还
}
pub enum BuildEvent { Ready { job: JobId, tls: TlsInfo, napi_id: u32 }, Failed { job: JobId, stage: ConnPhase, why: BuildFail } }
pub struct TlsInfo { pub version: u16, pub suite: u16, pub group: u16 }   // IANA 码,如 0x0304 / 0x1301 / 0x11EC(X25519MLKEM768)
/// stage = 失败时所处的相位(TcpConnecting / TlsHandshake / WsUpgrade)。
pub enum BuildFail { Errno(i32), Timeout, TlsAlert(u8), Tls /* 其他 rustls 错误 */, HttpStatus(u16) /* Upgrade 非 101 */, BadAccept, Extensions }
pub enum BuildErr { JobsFull, ArenaFull, BadEndpoint, BadSrcIp, NotReady }

pub struct BuildReq {
    pub venue: VenueId, pub conn: ConnId, pub role: ConnRole,
    pub endpoint: EndpointId,       // 见配置 endpoints[]
    pub src_ip_idx: u8,             // 本实例出口 IP 表 src_ips[] 的下标
    pub rx_queue: u16,              // 目标 RX 队列;只在 rss_plan 启用时使用
    pub deadline_ns: i64,           // 整个作业的截止;与各阶段超时先到者生效
}

/// 一个池只连一个端点(BuildReq.endpoint);同一 venue 的多个 REST 端点由上层各持一个池(hft-venue §4a)。
pub struct RestPool<const N: usize = 2> { /* N 条 HTTP/1.1 keep-alive Conn,一连接一在途 */ }
impl<const N: usize> RestPool<N> {
    pub fn new(host: &[u8], cfg: &NetConfig, clock: TscClock) -> Self;
    /// 全部 Open 连接都有在途 → Busy;没有 Open 连接 → NoConn;请求拼接超过 tx 明文区 → ReqTooLarge(都未加密)。
    pub fn submit(&mut self, req: &RestReq<'_>, now_tsc: u64) -> Result<ReqTag, RestErr>;
    /// 只读有在途请求的连接;返回一个完整响应(已去 chunk)或一个失败(该连接随之退役)。
    pub fn poll(&mut self, now_tsc: u64) -> Option<Result<RestResp<'_>, RestFail>>;
    /// 对没有在途请求的连接各做一次非阻塞读,只为发现对端关闭(EOF、RST、close_notify)或协议错误(未请求的数据),
    /// 发现即退役补建;`rest_max_idle_ms` 非 0 时另退役至多一条空闲达该时长的连接(§9 空闲回收);返回本次读过的连接数。
    /// 调用节拍由上层定(hft-venue `gw.rest_idle_check_ms`)。
    pub fn idle_check(&mut self, now_tsc: u64) -> u8;
    /// 补建与退役的交接同 ConnPool:连接数 < N 且无在建时 next_build 给 Fill;退役的连接经 take_retired 交出。
    pub fn install(&mut self, h: Handoff) -> Result<ConnId, PoolErr>;
    pub fn next_build(&mut self, now_tsc: u64) -> Option<BuildWhy>;
    pub fn build_failed(&mut self);
    pub fn take_retired(&mut self) -> Option<RetireMsg>;
}
pub struct RestReq<'a> { pub method: Method, pub path_query: &'a [u8], pub headers: &'a [(&'a [u8], &'a [u8])], pub body: &'a [u8], pub tag: u64 }
pub struct RestResp<'a> { pub tag: u64, pub status: u16, pub headers: [Header<'a>; 64], pub header_count: usize, pub body: &'a [u8], pub ts: Timestamps }
pub struct RestFail { pub tag: u64, pub err: RestErr }
pub enum RestErr { Busy, NoConn, ReqTooLarge, Send(SendErr), Timeout, TooLarge, Closed /* 响应完整前对端关闭 */, Proto }

/// 行情任务的就绪探测(hft-venue §2 第 2 步;启用条件归 hft-venue `feed.ready_poll`)。水平触发,`EPOLLIN`,事件 data = ConnId。
pub struct ReadyPoll { /* epfd、events 缓冲 [epoll_event; 64] */ }
impl ReadyPoll {
    pub fn new() -> Result<ReadyPoll, NetErr>;              // 启动期:epoll_create1(EPOLL_CLOEXEC);epfd 进程生命周期内不关闭
    pub fn handle(&self) -> ReadyHandle;                    // 交给 conn-builder
    /// 热线程:一次 epoll_pwait(epfd, events, 64, timeout = 0),把就绪连接的 ConnId 写进 out(先清空);返回就绪数。不做其他系统调用。
    /// 就绪超过 64 条时其余留到下一次(水平触发);系统调用出错 → 返回 0、out 为空。
    pub fn probe(&mut self, out: &mut ConnSet) -> u16;
}
#[derive(Copy, Clone)] pub struct ReadyHandle { /* epfd */ }   // Send;只在 warm 线程使用
impl ReadyHandle {
    pub fn add(&self, c: &Conn) -> Result<(), NetErr>;      // EPOLL_CTL_ADD;在交接前调用;重复注册 → Os(EEXIST)
    pub fn del(&self, c: &Conn) -> Result<(), NetErr>;      // EPOLL_CTL_DEL;在 Connector::reclaim 之前调用;未注册 → Os(ENOENT)
    pub fn del_fd(&self, fd: i32) -> Result<(), NetErr>;    // 同上,按交接时记下的 fd(连接已退役、回收前不再以 Conn 可达)
}
pub struct ConnSet { pub bits: [u64; 4] }                   // 按 ConnId 的 256 位集合
pub enum NetErr { Os(i32) }

pub struct TxTsDrain { /* dup 的 fd 表 */ }
impl TxTsDrain {
    pub fn register(&mut self, key: ConnKey, dup_fd: RawFd);
    pub fn drain(&mut self, out: &mut impl FnMut(TxStamp)) -> usize;   // recvmsg(MSG_ERRQUEUE|MSG_DONTWAIT)
}
#[repr(C)] pub struct ConnKey { pub venue: VenueId, pub conn: ConnId, pub _pad: u16 }                       // 4 B
#[repr(C)] pub struct TxStamp { pub key: ConnKey, pub tskey: u32, pub kind: u8 /* SCHED=1 SW=2 */, pub _pad: [u8; 7], pub ts_ns: i64 }   // 0 key、4 tskey、8 kind、16 ts_ns;24 B
#[repr(C)] pub struct TxTag   { pub key: ConnKey, pub tskey_end: u32, pub slot: u32, pub gen: u32, pub sent_tsc: u64 }                // 0、4、8、12、16;24 B(代码里写作 `r#gen`:edition 2024 保留字)
```

### 调用约束

- `Conn` 任一时刻只属于一个线程;转移只经 `hft_rt::spsc` 的 `Producer<Handoff>` / `Producer<RetireMsg>`(arena 槽号,不是指针)。热线程**永不**调用 `connect`、`shutdown`、`close`、`getaddrinfo`,也不做 TLS 握手。
- `Handoff::retire` 把尚未装入池的交接转换成回收消息；池拒绝安装时仍须交还原 Connector 回收。`test-io` 的 `ScriptIo.wait_fd` 只覆盖就绪列表观察值，不执行该描述符的系统调用。
- `next_frame` 返回的 `Frame::payload` 在同一 `Conn` 下一次 `next_frame` 或 `poll_rx` 前有效;调用方须在此之前完成解码与 raw journal 追加。两次 `next_frame` 之间调用 `send_frame` 不影响读位置与已解出的明文。
- `send_frame` 只接受已 `seal` 的帧;`SendErr::NotOpen/TxBacklog` 保证**没有**加密,调用方可改走备用连接;`SendErr::Io` 表示密文已产生,订单按 `Unknown` 处理([defensive-patterns](../../docs/defensive-patterns.md))。
- `warm` 可每轮空闲循环调用;它不做系统调用,不改真实连接的任何状态。
- `ConnPool::swap_primary` 不检查在途请求——"无 Pending 才切"是 [`GwRuntime`](../hft-venue/README.md) 的不变量。
- `ReadyHandle::add/del`(`epoll_ctl`)只在 warm 线程调用:连接交接前注册、回收前注销;热线程只调 `ReadyPoll::probe`。就绪集合只是提示,误报只多一次空 `recvmsg`。

### 配置项

全部来自 hft-core 的主机/连接配置(apps/hft-core 解析,内容作为 `ConfigBlob` 记入 journal),承载类型是本 crate 的 `NetConfig`(`Default` 给出下表默认值);协议相关的心跳参数来自 `VenueProtocol`,连接轮换时刻来自 [hft-venue ECF1 段 6](../hft-venue/README.md#配置项),都不在这里。

| 名称 | 类型 | 默认 | 含义 / 来源 |
|---|---|---|---|
| `endpoints[]` | `{host, sni, port, path, ips[], upgrade_headers[]}` | — | `ips` 为部署期逐 A 记录测 RTT 后钉选的地址;为空才在 warm 线程解析 DNS([research/02 §11.1](../../design/research/02-exchange-connectivity.md));`upgrade_headers` 是上层给的 Upgrade 请求额外头(如 Binance SBE 流的 `X-MBX-APIKEY`) |
| `bind_device` | 源地址 → 网卡名的表 | deploy 生成 | 每个 socket 按 `BuildReq.src_ip_idx` 选源地址,并 `SO_BINDTODEVICE` 到该地址所在的网卡:升级主机上 `prod` 的地址在 ENI-B(c32 预留的 ENI-B2 启用后,批量流的地址在 ENI-B2)、`sim` 的在 ENI-C;dev2 只有一块网卡。网卡、地址与实例、连接种类的划分归 [hft-core 主机资源隔离](../../apps/hft-core/README.md#主机资源隔离slice核与出口-ip) |
| `src_ips[]` | `[Ipv4Addr]` | — | 本实例出口网卡上的私有 IP(各配 EIP);连接固定源 IP。按实例划分、两组不相交,sim 的连接不消耗 prod 的 IP 级限额(取值与校验归 [hft-core 主机资源隔离](../../apps/hft-core/README.md#主机资源隔离slice核与出口-ip)) |
| `src_port_range` | `(u16,u16)` | `40000..=40999` | 须在 `net.ipv4.ip_local_reserved_ports` 内(deploy 调优脚本保证) |
| `rss_plan` | path | 空(关) | 部署期 `ethtool -x` 导出的哈希函数、key、间接表;为空时源端口由内核分配(默认) |
| `tx_timestamps` | bool | false | TX `SO_TIMESTAMPING` + errqueue 配对;可选,默认关 |
| `extra_ca` | path | 空 | 额外信任的 CA(mock-exchange 回环测生产二进制用);**只在配置含 `endpoint_override` 时加载**,生产配置禁止两者 |
| `endpoint_override` | bool | false | hft-core 配置含 `endpoint_override` 时由 hft-core 置真(只决定是否加载 `extra_ca`) |
| `rx_buf` | bytes | Md 256 KiB / Order 与 UserStream 64 KiB / Rest 1 MiB | 每连接接收缓冲(大页) |
| `tx_buf` | bytes | 16 KiB | 每连接密文发送缓冲 |
| `max_msg` | bytes | 256 KiB | 单条 WS 消息(含分片拼接)上限,超出置 `RX_FULL` |
| `so_rcvbuf` / `so_sndbuf` | bytes | 262144 / 65536 | 适中,过大增加最坏排队([03 §3.2](../../design/03-网络与传输层.md)) |
| `tcp_user_timeout_ms` | u32 | 5000 | 未确认数据超时即 `RESET` |
| `quickack_after_read` | `{md, order}` | `{false, false}` | 有数据的读之后补 `TCP_QUICKACK`(额外一次 setsockopt;不在 [hot-path-rules T7](../../docs/hot-path-rules.md) 允许表内,A/B 证明有收益并更新该表后才可打开) |
| `connect_timeout_ms` / `tls_timeout_ms` / `upgrade_timeout_ms` | u32 | 3000 each | `Connector` 各阶段超时 |
| `reconnect_backoff_ms` | `(u32,u32)` | `(100, 30000)` | WS 建连/登录失败后按端点与源 IP 指数退避,成功交接清零;0 或倒置范围拒绝加载;NET-208 |
| `rotate_stagger_s` | u32 | 3600 | 同池两条连接的计划轮换时刻至少错开 |
| `warm_interval_ns` | u64 | 0(关) | `warm` 最小调用间隔;scratch TLS 预热是可选项,默认关 |
| `rest_timeout_ms` | u32 | 2000 | REST 请求超时 → `RestErr::Timeout`;hft-core 按 venue 配置(`venues.<v>.net.rest_timeout_ms`,[hft-core 配置文件字段](../../apps/hft-core/README.md#配置文件字段));有下单网关的 venue 须 ≤ 该 venue 的 OMS `ack_timeout_ms`([hft-oms 配置](../hft-oms/README.md)),否则订单的迟到应答在 OMS 仍等待时已被丢弃 |
| `rest_max_idle_ms` | u32 | 0(关) | 池中连接空闲(无在途请求、距上次应答或交接)达此时长即由 `idle_check` 退役并补建(§9 空闲回收);hft-core 按 venue 配置(`venues.<v>.net.rest_max_idle_ms`) |
| `ts_expect_hw` | bool | true | 生产开;缺 HW 戳置 `TS_MISSING` |
| `tls_mode` | `unbuffered \| buffered` | `unbuffered`(`dev2` 可设 `buffered`) | `unbuffered` = rustls 握手 + 本 crate 的原地记录层(实现说明 §1);`buffered` = rustls 普通 `ClientConnection`(内部缓冲,读写各多一次拷贝、稳态有分配);**`Conn` 的公开 API 不变**,只换 `src/tls.rs` 的后端。`buffered` 随 M3(NET-340–NET-342)实现,此前 `Connector::new` 拒绝;零分配测试与延迟基准只对 `unbuffered` 生效 |

## 实现说明

<details><summary>实现要点</summary>

### 1. 接收路径

以下按 `tls_mode = unbuffered` 描述;`buffered` 后端用 rustls `ClientConnection::read_tls/process_new_packets/reader()` 与 `writer()/write_tls`,对外的 `poll_rx`/`next_frame`/`send_frame`/`warm` 语义(含"只在发送时加密一次"与加密调用点白名单)完全相同。

**记录层**:rustls 0.23 的 unbuffered `process_tls_records` 与加密都按记录分配(明文进 `Vec`、密文经 `PrefixedPayload`),达不到零分配与原地解密。所以握手(warm 线程)仍由 rustls `UnbufferedClientConnection` 驱动,到达 `WriteTraffic` 后经 `dangerous_into_kernel_connection` 转成 kernel connection(配置开 `enable_secret_extraction`):取出两个方向的流量密钥、IV 与当前序号,本 crate 用 aws-lc-rs `LessSafeKey::open_in_place`/`seal_in_place_separate_tag` 在连接缓冲上原地开、封记录(TLS 1.3:AAD = 记录头,nonce = IV ⊕ 序号,去零填充取内层类型;TLS 1.2:AES-GCM 显式 8 B nonce、ChaCha20 无显式 nonce,AAD = 序号‖类型‖版本‖长度)。kernel connection 只负责 KeyUpdate 后的新密钥(`update_rx_secret`/`update_tx_secret`)与保存会话票据(`handle_new_session_ticket`)。

**收包模型(唯一形态)**:网卡队列的 IRQ 与软中断在专用 IRQ 核(升级主机类为核 1,线程计划归 [hft-core](../../apps/hft-core/README.md#线程计划));内核把包放进 socket 接收队列,热线程用非阻塞 `recvmsg` 取走,**不做 NAPI 轮询**。为此 `Connector` 对每个 socket 显式 `setsockopt(SO_BUSY_POLL, 0)`(不依赖 `net.core.busy_read` 的主机值),不设 `SO_PREFER_BUSY_POLL`、`SO_BUSY_POLL_BUDGET`。主机侧参数(busy poll 相关 sysctl、网卡中断合并、IRQ 落点)由 [deploy 调优](../../deploy/README.md) 设置、[hostprobe](../../tools/README.md#hostprobe) 作为 hard 项校验(不符 → hft-core 退出 3);本 crate 只保证 socket 级不开 busy poll。

1. **`poll_rx`**:先压缩缓冲(把未消费的明文与未解密的密文移到缓冲头;稳态下为 0 或几百字节)。然后 `recvmsg(fd, iov=[rx 空闲尾部], control=cmsg_buf[128], MSG_DONTWAIT)`,每次 `poll_rx` **至多一次**:**短读即停**——返回的字节数小于所给的空闲空间即认为内核队列已空,不再补一次必然返回 `EAGAIN` 的 `recvmsg`;读满所给空间时缓冲已满,本次不再读(不算确认为空),剩余数据留在内核,下一次 `poll_rx` 再读;空闲空间为 0 时不调用 `recvmsg`(零长度读的返回 0 会被误认为 EOF)。没有数据时是一次返回 `EAGAIN` 的调用(`net_poll_empty`),用 `ReadyPoll` 的任务对未就绪的连接不调用。调用前读一次 TSC(队列为空的确认时刻,§7),返回后立即读 TSC 写 `rx_tsc`,解析 cmsg 的 `SCM_TIMESTAMPING`:`ts[0]` → `sw_rx_ns`,`ts[2]` → `hw_rx_ns`,按 §7 写 `rx_empty_gap_ns`;把 `(该次读取的密文起止偏移, 读到的字节数, Timestamps)` 记进每连接的读取戳表(定长 16 项;溢出时合并最老两项、保留较早的戳并置 `MULTI_SKB`)。不解密。
2. **`next_frame`**:明文区已有完整 WS 帧 → 直接解出;否则从 `cipher_start` 取**一条**完整记录原地解密,并入明文区后再试解帧,直到凑齐一帧或没有完整记录。因此一次调用的解密量以"这一帧所跨的记录"为界,一大段积压不会在第一帧之前整段解密。记录长度 > 2^14 + 256 → `TLS_PROTO`;一条记录在压缩后的缓冲里也放不下(明文区未消费字节 + 记录长度 > `rx_buf`)→ `RX_FULL`。
3. **并入连续明文**:没有未消费明文时直接将明文区指向原地解密的 payload，不搬移记录；只有已有跨记录的半帧时，才把新记录 payload `memmove` 到明文区尾。消费与时间戳仍按绝对流偏移推进；补充接收空间时按需压缩缓冲。
4. 在明文区上解 WS 帧:读 2–14 B 头,校验 FIN/RSV/opcode/MASK 位;完整帧就产出 `Frame{payload: &plain[a..b]}`,不复制。时间戳按 §7 从读取戳表取。
5. `finish_poll`:发出排队的 Pong/Close;续写 tx 剩余密文;若 `quickack_after_read` 对该角色为真且这一轮读到数据,`setsockopt(TCP_QUICKACK, 1)`(默认关)。
6. 接收缓冲满且解密全部密文后仍无完整帧 → `RX_FULL`。调用方因下游环满而停读时不调用 `poll_rx`,数据留在内核缓冲(TCP 背压)。

| 记录(解密后的内层类型) | 处理 |
|---|---|
| 应用数据(23) | 明文并入明文区(长度 0 的记录忽略) |
| 握手(22):NewSessionTicket | 计数;warm 线程上交给 rustls 保存,热线程 `install` 之后只计数(跨记录的长票据按长度跳过) |
| 握手(22):KeyUpdate | 按 rustls 给出的新密钥换接收方向;对端要求回应时立即把我们的 KeyUpdate 封进 tx(`finish_poll` 发出)并换发送方向;置 `KEY_UPDATE` |
| 握手(22):其他,或格式错 | `TLS_PROTO` |
| 告警(21),含 close_notify | `TLS_ALERT` |
| 解密失败、其他类型、全零填充 | `TLS_PROTO` |
| 连接仍在握手(rustls 会话未转换,热线程上的 `BlockedHandshake`) | `TLS_PROTO` |

**WS 解帧规则**(RFC 6455 §5):服务端帧带 MASK → `WS_PROTO`;RSV1–3 非 0 → `WS_PROTO`(不协商 permessage-deflate,见 §4);控制帧 >125 B 或分片 → `WS_PROTO`;Close 载荷长度为 1 → `WS_PROTO`;没有未完成的分片消息时收到 continuation 帧,或分片进行中收到新的 Text/Binary 起始帧 → `WS_PROTO`;Ping → 排队同 payload 的 Pong(每连接最多排 4 个;已排 4 个时再到 Ping,清空队列只排这一个);Pong → 若 payload 是我们发的 tsc,记一次 RTT;Close → 排队 Close(回显状态码)并置 `WS_CLOSE`;文本/二进制分片(FIN=0 + continuation)→ 拼到每连接一块 `max_msg` 的 scratch 区(唯一的复制路径,`reassembled=true`,计数);长度字段 > `max_msg` → `RX_FULL`。帧头(2–14 B,RFC 6455 §5.2)由本 crate 自己解析(`src/ws.rs`,与 `fuzz_ws_deframe` 一起交付;不依赖 tungstenite),payload 切片、掩码、控制帧逻辑同样是本 crate 自己的。

**热线程上的相位**:`Open` 收到 Close 帧 → `Closing`(回显的 Close 在 `finish_poll` 发出后 → `Dead`);传输类致命位(`PEER_EOF`、`RESET`、`IO`、`TLS_ALERT`、`TLS_PROTO`、`WS_PROTO`、`RX_FULL`、`HTTP_PROTO`)→ `Dead`;`TX_BACKLOG` 与上层置的 `PING_TIMEOUT`/`IDLE_TIMEOUT` 只置位、相位不变(连接仍须退役,由上层决定;积压期间只 `finish_poll` 续写即可再发)。`Closing`/`Dead` 的连接:`poll_rx` 返回 0 且不做系统调用,`next_frame` 返回 `None`,`send_*` 返回 `NotOpen(phase)`;之后只能 `retire`,`Connector::reclaim` 后槽回到 `Free`。

### 2. 发送路径

1. gw 线程(`hft-venue`)把订单帧明文写进 `MaskedFrame::body_mut()`,`seal(Text, len, mask_key)`:在 `buf[14-hdr_len..14]` 写头(len < 126 → 2 B 头 + 4 B 掩码键;< 65536 → 4 + 4),对 payload 做 SIMD 异或(按 16 B 块,尾部逐字节)。掩码键来自每连接的 xorshift64* 序列,种子在 warm 线程由 `getrandom` 取(RFC 要求不可预测针对浏览器缓存投毒,对我们的客户端无安全意义)。
2. `send_frame`:检查帧已 seal(否则 `NotSealed`)、`phase == Open` 且 tx 剩余 + 本帧密文上界 ≤ `tx_buf`,否则返回 `NotOpen/TxBacklog`(未加密;`TxBacklog` 同时置 `TX_BACKLOG`)。上界 = `len` + 每条记录的开销(TLS 1.3 为 22 B;TLS 1.2 AES-GCM 为 29 B、ChaCha20 为 21 B),帧超过 2^14 B 时按 2^14 B 一条记录切分。
3. `tls::seal_app_record(f.as_bytes(), &mut tx[tail..])`(把帧拷进 tx 后原地 `seal_in_place_separate_tag`)—— **真实记录序号在此推进一次**。TLS 1.3 发送序号到 2^23 时先封一条我们的 KeyUpdate 并换发送密钥(AES-GCM 的保密上限由调用方负责,kernel connection 不跟踪)。
4. `sendmsg(fd, iov, MSG_DONTWAIT|MSG_NOSIGNAL)`:tx 之前为空时 1 个 iovec,有短写剩余时 2 个(`[旧剩余, 新记录]`)。读 TSC 作为 `sent_tsc`(= 系统调用返回时刻)。
5. 短写 / `EAGAIN`:剩余字节留在 tx,置 `TX_PARTIAL`,返回 `Ok(SendOk{queued>0})`;之后每轮 `finish_poll` 续写,直到写完。**加密过的字节只会被完整发出或随连接一起丢弃**,不存在"重加密再发"。
6. 其他 errno:密文留在 tx,返回 `Io(errno)`,置 `RESET`/`IO`。

**TX 队列**:`sendmsg` 落到哪个网卡 TX 队列由 XPS 决定;按热核分配 XPS、队列数取上限的配置归 [deploy 运行期调优](../../deploy/README.md#运行期调优),本 crate 不设 socket 级队列选择。并发发送下的 `egress` 尾部在升级主机上测(开发备忘第 12 条)。

一帧一记录:订单帧 ≤ `MaskedFrame::CAP`(默认 2 KiB,远小于 16 KiB 记录上限),`encrypt` 只产生一条 TLS 记录;批量帧(OKX 20 单)用 `MaskedFrame<8192>`。发送只动 tx 缓冲与发送态,不碰接收缓冲与读位置,所以可以夹在两次 `next_frame` 之间。

### 3. `warm` 与 scratch 会话(scratch 预热可选,默认关)

**TLS 加密调用点白名单**:对 rustls 的 `WriteTraffic::encrypt` / `EncodeTlsData::encode` 与任何 AEAD `seal` 的调用只允许出现在 `src/tls.rs`,且只经这些入口:`tls::seal_app_record`(`Conn::send_frame`、控制帧、Upgrade 请求与 REST 请求的唯一加密点,推进真实记录序号)、`tls::encode_handshake_or_keyupdate`(我们的 KeyUpdate;握手期的 flight 由 rustls 编码、`encode_handshake_flight` 拷入 tx)、`tls::seal_scratch`(`Conn::warm`,只作用于 scratch 会话;随 TLS 预热可选扩展实现,此前不存在)。其他文件出现这些调用即 CI 失败(`cargo xtask ci` 的 grep 规则 + clippy `disallowed-methods` 对 `rustls::unbuffered::WriteTraffic::encrypt` 在 `src/tls.rs` 之外报错)。这是"不对真实连接预加密"([13 R1](../../design/13-设计评审与优化.md))的机械保证。

`warm_interval_ns > 0` 时,每个热线程在启动期(`conn-builder` 代建后交接)持有一个 **scratch 会话对**;默认关闭时不创建 scratch 会话,`Conn::warm` 为空操作:一个 `UnbufferedClientConnection` 与进程内 rustls server 在内存里完成握手(协商与生产连接相同的套件,默认 `TLS13_AES_128_GCM_SHA256`),之后只用 client 侧的 `WriteTraffic::encrypt` 把与真实帧同长度的明文加密进一个丢弃缓冲。这样被预热的是与真实发送**完全相同**的代码路径(rustls 记录层 + aws-lc-rs AES-GCM),而真实连接的序号、fd、tx 一概不碰。scratch 会话的序号自由增长;rustls 在接近 AES-GCM 加密次数上限时会自行发 KeyUpdate(写到丢弃缓冲),每小时由 warm 线程替换一次 scratch 会话。`warm` 同时读触(不写)目标 `Conn` 的 tx 缓冲首几条缓存行与 rustls 发送态所在行。

### 4. TLS 与 WS 握手(warm 线程 `conn-builder`,`Connector`)

```
Free ─submit─▶ TcpConnecting ─EINPROGRESS→可写,SO_ERROR=0─▶ TlsHandshake ─rustls WriteTraffic─▶ WsUpgrade ─101 + Accept 校验─▶ Open ─handoff─▶(热线程)
   ▲              │超时/拒绝                    │alert/超时                   │非 101/带 Extensions
   └──reclaim─────┴────────────────────────────┴────────────────────────────┘ → BuildEvent::Failed{stage, errno/alert}
Rest 角色:TlsHandshake 后直接 Open(不 Upgrade)
```

失败的 `BuildFail`:connect 报错 → `Errno(SO_ERROR)`;收到 TLS alert → `TlsAlert(描述码)`,其他 rustls 错误 → `Tls`;Upgrade 响应非 101 → `HttpStatus(状态码)`,`Sec-WebSocket-Accept` 不符 → `BadAccept`,带 `Sec-WebSocket-Extensions` → `Extensions`;超时 → `Timeout`。超时判定:`now_ns − 该阶段开始 ≥ 该阶段超时`(`connect_timeout_ms`/`tls_timeout_ms`/`upgrade_timeout_ms`),或 `now_ns ≥ BuildReq.deadline_ns`,先到者生效;`stage` = 失败时所处的相位。

1. 创建 socket(`socket2`),按 §5 设置选项(含显式 `SO_BUSY_POLL = 0`);`bind(src_ip, src_port)`:`rss_plan` 配置时 `src_port = RssPlan::pick(...)`,否则端口为 0 由内核分配(默认);非阻塞 `connect`。
2. rustls `ClientConfig`:aws-lc-rs provider;套件顺序 AES-128-GCM、AES-256-GCM、ChaCha20;TLS 1.3 优先、允许 1.2(以实测为准);密钥交换依次 X25519MLKEM768、X25519、P-256、P-384;根证书用固定的 `webpki-roots` 快照(`extra_ca` 只在 `endpoint_override` 存在时追加);会话票据内存存储(复用只省握手 RTT,握手不在热路径);`enable_secret_extraction = true`(握手完成即转 kernel connection,§1)。每次建连克隆一份配置交给 rustls(warm 线程)。
3. WS Upgrade 请求由本 crate 拼接:`GET <path> HTTP/1.1`、`Host`、`Upgrade: websocket`、`Connection: Upgrade`、`Sec-WebSocket-Key`(16 B 随机 base64)、`Sec-WebSocket-Version: 13`,外加上层给的额外头(如 Binance SBE 流的 `X-MBX-APIKEY`)。**不发送 `Sec-WebSocket-Extensions`**;响应若带该头 → 失败(防 permessage-deflate)。响应用 `httparse` 解析,`Sec-WebSocket-Accept` 按 RFC 计算比对。
4. **NAPI 落点与 MSS**:TLS 握手完成后 `getsockopt(SO_INCOMING_NAPI_ID)` 与 `getsockopt(TCP_MAXSEG)` 各一次(握手期间已收到包),分别写进 `Conn::napi_id`(及 `BuildEvent::Ready`)与 `Conn` 的 MSS(§7 的 `MULTI_SKB` 判定);`Connector::napi_ifindex` 经 netdev netlink 把它映射到网卡(内核 ≥ 6.9;hft-core 单元因此须允许 `AF_NETLINK`,单元表归 [deploy](../../deploy/README.md))。conn-builder 在交接前据此核对"连接落在本实例的网卡上"(规则与处置归 [hft-core 主机资源隔离](../../apps/hft-core/README.md#主机资源隔离slice核与出口-ip),不符的连接不交接);hostprobe 在 hft-core 建连之前运行,读不到这些 socket。dev2 只有一块网卡,不适用。启用 `rss_plan` 时另与计划队列比对,不符置 `WRONG_QUEUE`,`Connector` 换下一个候选端口重建(最多 3 次),仍不符则接受并上报。
5. 登录与订阅由 `hft-venue` 的会话驱动器在同一 warm 线程上用 `Conn` API 完成,然后 `handoff`。TLS 1.3 的 NewSessionTicket 通常紧随握手到达;`Connector` 在 `Open` 后最多等 `ticket_wait_ms=20`(常量)让它们在 warm 线程被处理(明文区已有应用数据时不再往后解密,票据留给热线程计数):`Ready` 在 `Open` 之后、已处理至少一张票据或 `now_ns ≥ Open 时刻 + 20 ms` 的第一次 `drive` 给出。

### 5. socket 选项清单

| 选项 | 值 | 目的 / 出处 |
|---|---|---|
| `O_NONBLOCK` | on | 非阻塞读写;热线程轮询 `recvmsg`,park 的线程由 hft-rt 的 epoll 后端等 socket([03 §3.2](../../design/03-网络与传输层.md)) |
| `TCP_NODELAY` | 1 | 关 Nagle |
| `TCP_QUICKACK` | 默认不设;开启时每次有数据的读后设 1 | 该选项非粘性;见配置 `quickack_after_read` |
| `TCP_USER_TIMEOUT` | `tcp_user_timeout_ms` | 发出的订单进黑洞时有界地发现 |
| `SO_BUSY_POLL` | **显式 0**(每个 socket 建立时设置) | 中断驱动收包:非阻塞 `recvmsg` 不在热核上做 NAPI 轮询,也不替同一队列上别的 socket(如 sim 的连接)处理包;不受主机 `net.core.busy_read` 的值影响 |
| `SO_PREFER_BUSY_POLL` / `SO_BUSY_POLL_BUDGET` | 不设(内核默认 0) | 同上 |
| `SO_TIMESTAMPING` | `RX_HARDWARE \| RX_SOFTWARE \| RAW_HARDWARE \| SOFTWARE`;`tx_timestamps=true` 时再加 `TX_SCHED \| TX_SOFTWARE \| OPT_ID \| OPT_TSONLY` | RX 戳进 cmsg(必开,HW 与 SW 都常态记录);TX 戳进 errqueue 是可选项 |
| `SO_BINDTODEVICE` | `bind_device` | 流量固定走本实例的出口网卡 |
| `SO_REUSEADDR` | 1 | 允许重用处于 TIME_WAIT 的预计算源端口 |
| `SO_RCVBUF` / `SO_SNDBUF` | 见配置 | — |
| `SO_INCOMING_NAPI_ID`(get) | — | 握手后读一次,记录落点(§4 第 4 步) |

ENA 的 ntuple 过滤器在所有世代都是 `off [fixed]`([research/01 §5.1](../../design/research/01-network-kernel-bypass-tls.md)),所以"连接 → 队列"唯一可用的导向手段是**源端口预计算**。

### 6. 源端口预计算(`RssPlan`,可选)

默认不启用:全部队列的 IRQ 在专用 IRQ 核,热线程不依赖包落在哪个队列;启用条件:升级主机上 `kernel_rx` 直方图显示尾部由某些队列的软中断排队主导。以下为可选扩展的规格。

1. 部署期(`deploy/` 调优脚本)`ethtool -x <eni-b>` 导出哈希函数、40 B key、间接表(128 项),写 `rss_plan` 文件;hft-core 启动时读入。哈希函数不是 Toeplitz 时 `RssPlan` 拒绝加载,退化为"不导向 + 校验上报"。
2. 网卡对**入站**包计算 `toeplitz(key, remote_ip ‖ local_ip ‖ remote_port ‖ local_port)`;队列 = `indir[hash & (len-1)]`。
3. `pick(local_ip, remote_ip, remote_port, target_queue, used)`:在 `src_port_range` 内按升序找第一个未被占用且落到 `target_queue` 的端口;128 项间接表、4 队列时期望每 4 个端口命中一个。
4. 连接建立后以 `SO_INCOMING_NAPI_ID` 校验(§4 第 4 步)。队列 → 核的 IRQ 亲和由 deploy 保证,本 crate 只接收 `rx_queue`。

### 7. 时间戳

- **RX 戳取哪个包**:TCP 的 `recvmsg` 只报告**本次读取中最后一个 skb** 的戳(待核实,开发备忘第 1 条),同一次读取里更早到达的包的戳拿不到;GRO 关闭(deploy 调优)时一次读取包含的包数取决于两次 `recvmsg` 之间到达的包数(调优见 [research/01 §1.3](../../design/research/01-network-kernel-bypass-tls.md))。规则:帧的 `hw_rx_ns`、`sw_rx_ns` 与 `rx_empty_gap_ns` 取该帧**最早一个包**所在的那次读取(该帧所跨的第一条 TLS 记录的首字节所在的读取);`rx_tsc` 取凑齐该帧的那次读取(帧可被处理的时刻,与 [event-model](../../docs/subsystems/event-model.md#时间戳语义) 的 `rx_tsc` 一致);两者不是同一次读取时,`rx_empty_gap_ns` 另加两次读取的 `rx_tsc` 之差。所取那次读取的字节数 > 连接的 MSS(握手后 warm 线程读一次 `TCP_MAXSEG` 记入 `Conn`)时,该次读取肯定含多个包、戳来自更晚的包,置 `MULTI_SKB`(读取戳表溢出合并时同样置位);此时戳不是该帧最早一个包的戳。字节数 ≤ MSS 却含多个小包(几条 BBO 各占一个包)的情形检测不到,由 `rx_empty_gap_ns` 给出等待的上界。
- **接收队列为空的确认(`rx_empty_gap_ns`)**:`Conn` 记 `empty_tsc` = 最近一次确认本连接接收队列为空的时刻,来源有三种:`recvmsg` 返回 `EAGAIN`、短读(两者都取该次调用**之前**读的 TSC,不晚于内核看到队列为空的时刻),以及调用方的 `note_empty(tsc)`(就绪探测里本连接未就绪,`tsc` = 探测调用之前读的 TSC;调用规则归 [hft-venue §2](../hft-venue/README.md#2-feedruntimepoll_once-的固定顺序))。`empty_tsc` 只前移(新确认早于现值时不变);热线程 `install` 交接来的连接时清零(warm 线程上的确认不沿用)。每次读到数据的 `recvmsg` 在更新 `empty_tsc` 之前写 `rx_empty_gap_ns = delta_ns(empty_tsc, rx_tsc)`(`ConnArena` 持有启动期标定的 hft-rt `TscClock`;饱和到 `u32::MAX`);交接后还没有任何确认时写 0。这次读到的每个包都在 `empty_tsc` 之后到达,所以 `rx_empty_gap_ns` 是这批数据从进入接收队列到被本线程读到的等待上界;离线用法(⑩ 报告并列 `kernel_rx` 的上下界)归 [testing §8](../../docs/testing.md#8-上线门槛)。
- `sw_rx_ns − hw_rx_ns` 与 `rx_tsc − sw_rx_ns`(换算到同一时钟域后)分别是诊断分段 `nic_to_kernel`、`kernel_to_user`([latency-budget](../../docs/latency-budget.md));两个戳都随事件常态记录。帧跨多次读取时,`kernel_to_user` 含等待该帧收全的时间(`rx_tsc` 取凑齐该帧的那次读取)。
- cmsg 里没有 `SCM_TIMESTAMPING`、被截断(`MSG_CTRUNC` 或不足三个 timespec)或某个 timespec 为 0:对应的戳记 0、不置对应的 `HW`/`SW` 位;`ts_expect_hw = true` 且这次读到数据却没有 HW 戳 → 置 `TS_MISSING`。
- HW 戳在 PHC 域;事件里的 `hw_rx_ns` 原样记录 PHC 值,换算由 `ClockStatus` 负责([event-model 时间戳语义](../../docs/subsystems/event-model.md#时间戳语义))。c8g 上 HW RX 戳与 PTP 是否可用在 M1.5 延迟探针核实([architecture-v2/11](../../design/11-实施路线图.md))。
- TX(**可选,`tx_timestamps=false` 时不设 TX 标志、不 dup fd、不推 `TxTag`**):ENA 没有 TX 硬件戳。`OPT_ID` 使每次 `sendmsg` 获得字节偏移键(`tskey` = 该次写入末字节的累计偏移)。gw 发送后把 `TxTag` 推进一个到 housekeeping 的 SPSC;housekeeping 用 `TxTsDrain` 以 `recvmsg(MSG_ERRQUEUE)` 在 dup 的 fd 上取 `TX_SCHED`/`TX_SOFTWARE`,按 `tskey` 配对后产出延迟样本(配对与出口在 [apps/hft-core](../../apps/hft-core/README.md))。errqueue 的 skb 计入接收缓冲配额,必须持续取;housekeeping 每 1 ms 取一次。因为存在 dup 的 fd,退役连接必须先 `shutdown(SHUT_RDWR)` 再 `close`(由 `Connector::reclaim` 做),否则 TCP 不会随原 fd 关闭而断开。

### 8. `ConnPool` 与轮换

```
Race(行情):      [A 活跃] [B 活跃] [C 预热中?]         两条同订阅,按固定顺序轮询
PrimaryStandby:  [P 发单] [S 已登录、心跳] [W 预热中?]  只有 P 发新单
不变量:计划轮换与 swap_primary 不使活跃连接数降到 0(致命退役可以,此时 next_build 立即给 Fill);池满时 next_build 不给请求;任何时刻至多一条在建(错峰)。
```

1. 计划轮换时刻由上层在 `install` 时给出(规则归 [hft-venue ECF1 段 6](../hft-venue/README.md#配置项),由交易所强制断开的时长减去余量算出);同池两条的计划时刻相差 < `rotate_stagger_s` 时,较晚的一条提前到"较早一条的时刻 − `rotate_stagger_s`"(只提前、不推后,保证不越过交易所的强断时刻;新装入的一条与池中每条已有计划时刻逐一比较)。`next_build` 的优先顺序:`Hint` > `Rotate` > `Fill`。
2. `rotate_due` 到期或上层置 `SERVER_HINT` → `next_build` 给出替补请求 → `Connector` 建好、上层登录/订阅 → `install` 进空槽。
3. Race:替补收到与合并链连续的数据后,`retire` 最老的一条。PrimaryStandby:替补成为新 standby;旧 primary 等 `GwRuntime` 确认无在途后 `swap_primary`,再 `retire`。
4. 致命错误:该连接立即 `retire`;Race 池剩一条时 `hft-venue` 报 `ConnState::Degraded`,PrimaryStandby 池由 `GwRuntime` 决定切换。退役的是 primary 时,活跃的 standby(若有)成为 primary、standby 置空。
5. `swap_primary` 只交换两个 `u8` 索引,无系统调用。

### 9. REST 客户端

HTTP/1.1 over 同一 `Conn`(`ConnRole::Rest`,无 WS)。请求在 tx 明文 scratch 中拼接(`Host`、`Accept-Encoding: identity`、`Connection: keep-alive`、上层给的签名头),一次 `encrypt` + `sendmsg`。响应头用 `httparse`,正文支持 `Content-Length` 与 chunked(原地去 chunk);204/304 无正文;其余既无 `Content-Length` 又非 chunked → `RestErr::Proto`;接收缓冲满仍凑不齐响应(或 `Content-Length` 已超过 `rx_buf`)→ `RestErr::TooLarge`;`now_tsc` 距 `submit` ≥ `rest_timeout_ms` 仍无完整响应 → `RestErr::Timeout`;响应完整前对端关闭 → `RestErr::Closed`。每连接一个在途请求(不做 pipelining),`submit` 在空闲连接间轮转(按连接序号循环);`poll` 只读有在途请求的连接,空闲的 keep-alive 连接只在 `idle_check` 时各读一次(对端已关闭或送来未请求的数据 → 退役补建,后者置 `HTTP_PROTO`);`Connection: close` 的响应交出后、或任何 `RestFail` 之后该连接退役补建(超时后迟到的响应不会被当作下一个请求的响应)。**空闲回收**:中间设备(NAT、负载均衡、CDN)可能不告而别地丢弃空闲的 keep-alive 连接,`idle_check` 的读只能发现干净的关闭,死连接上的请求只会超时;`rest_max_idle_ms` 非 0 时,`idle_check` 在读过之后退役一条空闲达该时长的连接(`why` = `IDLE_TIMEOUT`;空闲时长从交接后的第一次 `idle_check`、或上一个应答交出时算起,在途期间不计),每次至多一条,且只在池满(每条通道都有连接)、没有在建连接时进行,补建经 `next_build` 交给 warm 线程,另一条连接在此期间照常承接请求;热线程不建连。取值应明显小于路径上最短的空闲超时。一个池只连一个端点。用途:`Unknown` 订单查询、死手刷新(REST 型)、listenKey 续期、下单兜底(gw);账户配置查询与保证金轮询(warm 任务 acct-fetch,调度在 gw,[hft-venue §4a](../hft-venue/README.md#4a-账户查询保证金轮询与配置核验));BBO 补帧与(启用增量深度时的)行情快照(warm 任务 snap-fetch,[hft-venue §3b](../hft-venue/README.md#3b-快照获取snap-fetch))。feed 任务不持有 REST 连接。对账等冷查询在冷进程自带客户端,不用本池。本 crate 不区分这些用途,只保证一连接一在途、响应体 ≤ `rx_buf`。

### 10. 内存

`ConnArena` 在启动期按 `(角色, 槽数)` 一次分配全部槽并预触摸:每槽 = `Conn` 结构(含记录层密钥与 rustls kernel connection)+ 该角色的 rx + tx + 分片 scratch;槽只给同角色的作业用。目标形态是从 [`hft-rt`](../hft-rt/README.md) 的大页分配器取一整块;M1.5 实现先用堆分配 + 预触摸(见已知限制)。每 venue 典型:BBO 3 条(两条赛马 + 一条在建)、分档快照/成交/标记价 2 条、下单 3 条、gw REST 2 条、acct-fetch REST 每账户 1–2 条、snap-fetch REST `snap.parallel` 条;core 的 3 个交易所 × 现货/永续 = 6 个 venue,按 `rx_buf`/`tx_buf`/`max_msg` 默认值约 50 MiB;每启用一个 optional venue 约加 8 MiB。实际总量以启动日志里 `ConnArena` 的分配为准(内存表归 [hft-core](../../apps/hft-core/README.md))。rustls 内部结构在握手期可能分配(warm 线程),稳态收发不分配(以计数分配器测试为准)。

### 11. 源文件规划

| 文件 | 职责 |
|---|---|
| `src/lib.rs` | 公开 API 与重导出 |
| `src/types.rs` | `ConnPhase`、`Timestamps`、`ErrBits`、`SendOk/SendErr`、`NetErr`、`ConnKey`、`TxStamp/TxTag`;重导出 `ConnRole` |
| `src/sock.rs` | socket 创建、选项(含 `SO_BUSY_POLL = 0`)、`bind`、非阻塞 connect、`recvmsg`/`sendmsg` 封装(`unsafe` 集中处);封装对每种系统调用计数;feature `test-io` 下可换成脚本化 I/O(按脚本给出每次调用的返回值、字节、cmsg 戳与调用前后的 TSC,并可注入一次 TLS 加密失败),供确定性测试 |
| `src/cmsg.rs` | `SCM_TIMESTAMPING` 解析、errqueue 解析 |
| `src/tls.rs` | rustls unbuffered 状态机驱动、按需逐记录解密、明文合并、scratch 会话;**全 crate 唯一的 TLS 加密调用点**(白名单见实现说明 §3) |
| `src/ws.rs` | 帧头解析、控制帧、分片、`MaskedFrame`、SIMD 掩码 |
| `src/conn.rs` | `Conn`、读位置与读取戳表、`next_frame` |
| `src/arena.rs` | `ConnArena`、`Handoff`/`RetireMsg` 所有权转移 |
| `src/pool.rs` | `ConnPool`、`RotatePolicy` |
| `src/connector.rs` | 建连作业、WS Upgrade、NAPI 落点 |
| `src/ready.rs` | `ReadyPoll`、`ReadyHandle`、`ConnSet`(epoll 封装,`unsafe` 集中于 `sock.rs`) |
| `src/rss.rs` | Toeplitz、`RssPlan`、端口挑选 |
| `src/rest_pool.rs` | 有界 HTTP/1.1 keep-alive `RestPool`;响应 headers 只读 `[..header_count]` |
| `src/rest.rs` | warm 单请求 `Client`,由调用方驱动 `Connector` |
| `src/txts.rs` | `TxTsDrain` |

</details>

`poll_rx_if_needed` 供 BBO feed 的轮询入口使用。完整明文帧或密文记录尚未消费时，跳过 recvmsg 与缓冲压缩；部分 WS 帧或部分 TLS 记录仍立即补读。此判断不解密、不消费、不更新读取戳或空队列确认点；控制帧和错误仍由 `next_frame` 处理。底层 `poll_rx` 的直接读取契约不变。

## 延迟与资源契约

预算以统一升级主机类(`c16`/`c32`,Graviton4,`aarch64` + `target-cpu=neoverse-v1`,见 [development 目标 CPU](../../docs/development.md#目标-cpu);档位的选定归 [hft-core 线程计划](../../apps/hft-core/README.md#线程计划))的隔离核、`Spin` 线程为准;`dev2`/`c4` 不作延迟验收。"不含系统调用"的基准用内存对端(进程内 rustls server),含系统调用的用回环 TCP。本 crate 承担的分段是 `kernel_rx` 的用户态部分与 `ingest`、`egress` 中的收发部分;各分段的基准形态(中断驱动)唯一归属 [latency-budget §3](../../docs/latency-budget.md)。

| 入口 | 线程 | p50 / p99 预算 | 基准名 |
|---|---|---|---|
| `next_frame`:解密 + 并入 + 解帧,300 B 单记录单帧(不含 syscall) | feed/gw | ≤ 0.35 µs / ≤ 0.8 µs | `net_rx_300b_mem` |
| 同上,2 KiB | feed | ≤ 0.7 µs / ≤ 1.5 µs | `net_rx_2k_mem` |
| 积压 64 KiB 时取出第一帧(按需解密,只解该帧所跨记录) | feed | ≤ 1 µs / ≤ 2 µs | `net_rx_first_of_64k` |
| `poll_rx` + `next_frame`(回环,300 B) | feed/gw | ≤ 2 µs / ≤ 5 µs | `net_poll_loopback` |
| `poll_rx` 空轮询(EAGAIN,`SO_BUSY_POLL = 0`) | feed/gw | ≤ 0.8 µs / ≤ 2 µs | `net_poll_empty` |
| `ReadyPoll::probe`,12 个已注册连接、无就绪 / 2 个就绪 | feed | ≤ 0.5 µs / ≤ 1.5 µs | `net_ready_probe` |
| `MaskedFrame::seal`,300 B | gw | ≤ 30 ns / ≤ 60 ns | `net_seal_300b` |
| `send_frame` 加密部分,300 B(不含 syscall) | gw | ≤ 0.25 µs / ≤ 0.5 µs | `net_encrypt_300b_mem` |
| `send_frame` 含 `sendmsg`(回环) | gw | ≤ 2.5 µs / ≤ 6 µs | `net_send_loopback` |
| `warm`,300 B(可选) | feed/gw | ≤ 0.3 µs / ≤ 0.6 µs | `net_warm_300b` |
| `ConnPool::swap_primary` | gw | ≤ 20 ns | `net_pool_swap` |
| `RssPlan::pick`(可选) | warm | ≤ 50 µs(整段扫描) | `net_rss_pick` |

- **分配**:稳态 `poll_rx`/`next_frame`/`send_frame`/`warm`/`finish_poll` 零分配。
- **系统调用**(热线程):每个被读的连接每轮至多 1 次 `recvmsg`(短读即停;读满即缓冲满,这一轮不再读;gw 的空闲 REST 连接只在 `idle_check` 时读);启用 `ReadyPoll` 的行情任务每次 `poll_once` 一次 `epoll_pwait(timeout = 0)`,只读就绪的连接,`epoll_ctl` 只在 warm 线程;每帧 1 次 `sendmsg`(gw 的订单帧;feed/gw 的 Pong、应用心跳、订阅帧也走 `sendmsg`,gw 的 REST 请求同样,均在 [hot-path-rules T7](../../docs/hot-path-rules.md) 的允许表内);`TCP_QUICKACK` 默认不用;非阻塞 `recvmsg` 不触发 NAPI 轮询(`SO_BUSY_POLL = 0`)。其他系统调用全部在 warm 线程。
- **共享缓存行**:无。`Conn` 不跨线程共享;`TxTag` 经 SPSC。
- **内存**:见实现说明 §10。

## 测试与验收

用例与验收见 [TESTS.md](TESTS.md)。

## 依赖

| 依赖 | 版本约束 | 理由 |
|---|---|---|
| `hft-rt` | workspace | `tsc_now`/`tsc_ordered`、`HugeRegion` 大页与预触摸、`spsc`(交接环、`TxTag` 环) |
| `hft-types` | workspace | `VenueId`、`ConnId` |
| `hft-event` | workspace | 只用 `ConnRole`(连接角色编码的唯一归属) |
| `rustls` | `0.23.45`,`default-features = false`,features `aws_lc_rs, std, tls12` | 握手用 unbuffered API(`rustls::client::UnbufferedClientConnection`、`rustls::unbuffered::*`),之后用 kernel connection(`rustls::kernel`,0.23.2x 起)管 KeyUpdate 与票据;小版本经 Cargo.lock 锁定,升级须重跑属性测试与零分配测试 |
| `aws-lc-rs` | `1`(与 rustls 同一构建),features `aws-lc-sys, prebuilt-nasm` | 直接依赖:记录层的原地 AEAD、`Sec-WebSocket-Accept` 的 SHA-1、掩码种子与 `Sec-WebSocket-Key` 的随机数(`aws_lc_rs::rand`,仅 warm 线程)。aarch64(AES/PMULL 汇编,唯一发布目标)与 x86 都有优化路径([research/01 §3](../../design/research/01-network-kernel-bypass-tls.md));Graviton 上的单记录成本待 `net_rx_*_mem` 实测;构建需目标平台的 C 编译器(CI 装 `gcc-aarch64-linux-gnu`) |
| `webpki-roots` | 固定快照 | 根证书随发布归档,不读系统证书 |
| `httparse` | `1` | Upgrade 响应与 REST 响应头 |
| `libc` | `0.2` | 全部 socket 系统调用(创建、选项、`bind`、`connect`、`recvmsg`/`sendmsg`、`epoll`、netdev netlink),都经 `src/sock.rs` 计数 |
| `bitflags` | `2` | `ErrBits` |

**禁止**:`tokio`、`mio`、任何 async 运行时;`openssl`/`native-tls`;`serde_json`;`tracing` 于热路径;依赖 `hft-venue`、`venue-*`、`hft-journal`(本 crate 不写 journal)。

## 延伸阅读

- [03 网络与传输层](../../design/03-网络与传输层.md)(§3 基准形态的做法、§6 接口原型)
- [13 设计评审 R1、R3](../../design/13-设计评审与优化.md)
- [research/01 网络/TLS](../../design/research/01-network-kernel-bypass-tls.md)(TLS 每记录成本、内核收包机制、ENA 约束、HW 时间戳)
- [research/02 §1、§11](../../design/research/02-exchange-connectivity.md)(端点直连/CDN 分类、DNS 钉选、连接卫生)
- [hft-venue](../hft-venue/README.md)(本 crate 的唯一上层)、[event-model](../../docs/subsystems/event-model.md)、[latency-budget](../../docs/latency-budget.md)、[hot-path-rules](../../docs/hot-path-rules.md)、[deploy 调优](../../deploy/README.md)、[hostprobe](../../tools/README.md#hostprobe)
- Agent Note:`no-speculative-tls-encryption`、`dedicated-gateway-threads`、`rx-irq-driven`(`.agents/notes/proposed/`)

### Warm REST 单请求传输

`rest::Client` 复用 Connector 的 TLS 校验和源地址绑定，一次一个请求，按 BuildReq 截止时刻取消。支持 Content-Length/chunked，拒绝压缩、歧义 framing、超容量响应；传输失败不等于业务拒绝。`submit_target` 为单条 WS 连接设置协议生成的路径与头，listenKey 不写全局端点或错误信息。

`RestResp` 携带实际 ConnId 与提交时刻 sent_tsc，接收时间戳仍取首字节。`RestPool::cancel` 退役指定请求连接并丢弃已缓冲响应；`retire_all` 用于停机交回全部连接。`wait_fds` 只枚举在途连接，`has_pending` 只表示已完成的可消费结果。Conn 的 handoff_data 限 16 KiB，仅 warm 线程设置非秘密元数据，热线程只借用读取，重置时清零。

## 已知限制与后续工作

- `RestPool` 覆盖复用、Content-Length、超时、关闭、容量与时间戳;chunked、无 Content-Length 的 bodyless 响应、HTTP/2 与重定向仍不支持。上层 warm `Client` 尚未迁移到该池。提交前完成 HTTP 长度校验,失败不推进 TLS 序号;签名请求临时区在提交或编码失败后清零。

- **默认关闭的三项(可选扩展)**:RSS 源端口预计算、TX 时间戳 errqueue、scratch TLS 预热。规格与测试保留,启用条件:升级主机上的直方图(`kernel_rx`、`egress` 冷热差)证明该项是主导项;启用任何一项前,其测试行必须全绿。
- **中断驱动的代价**:每个包多一次中断与软中断(在专用 IRQ 核上),热线程发现数据的等待 ≈ 半个轮询周期;`kernel_rx` 的基准形态按此设定(latency-budget §3)。`IRQ_ON_HOT` 告警保持为真。
- **无 kernel bypass**:可选扩展(裸金属 + Onload,启用条件:交易所提供 AWS 之外的接入或 fabric 抖动主导 p99)若启用,本 crate 零改动(POSIX 透明),但 `SO_TIMESTAMPING` 在 Onload 下的行为需重测([03 §5](../../design/03-网络与传输层.md))。
- **无 FIX**:Binance Spot FIX OE 是 TCP+TLS 上的非 WS 协议;需要时在 `Conn` 上加 `ConnRole::Fix`(无 WS 层,按 SOH 分帧),可选扩展:接入 Binance Spot FIX 时实现。
- **无 permessage-deflate**:某所若只给压缩流,需另建解压路径(`yawc` 候选)并单独测成本。
- **TX 无硬件戳**:ENA 限制;`sent_tsc` 是 `sendmsg` 返回时刻,线上时刻只能用对端回执估计。
- **一次读多包只拿到最后一个包的戳**:见实现说明 §7(取最早一个包所在读取的戳、`MULTI_SKB` 标记、`rx_empty_gap_ns` 上界);若测得 `MULTI_SKB` 比例 > 1%,再评估逐包读(`recvmmsg` 不适用于 TCP)。
- **IPv4 only**:交易所端点全部 IPv4;IPv6 不在范围内。
- **实现范围**:接收、发送、时间戳、`Connector`、`ConnArena`/`ConnPool`、`ReadyPoll` 与 `RestPool` 已实现;`tls_mode = buffered`(M3)、`RssPlan` 与 TX 时间戳 `TxTsDrain`、scratch TLS 预热(可选扩展,M7)尚未实现,对应配置在 `Connector::new` 被拒绝。`ConnArena` 的大页 backing 待 hft-core 装配时接入(现为堆分配 + 预触摸)。

### 开发备忘

<details><summary>给实现者</summary>

**待核实**(每条写明方法)

1. **TCP `recvmsg` 的时间戳语义**:确认 6.12 内核 `tcp_recvmsg` 报告的是本次读取最后一个 skb 的戳(`tcp_update_recv_tstamps`),以及 TCP 接收队列合并 skb 时保留哪个戳。方法:回环 + netem 让两包间隔 1 ms,一次读两包,比较 `sw_rx_ns` 与两包各自的到达时刻;同时统计 `MULTI_SKB` 比例与 `rx_empty_gap_ns` 的分布。
2. **ENA RSS 哈希函数与 key 可配性**:在目标实例上 `ethtool -x` 看 `RSS hash function`(toeplitz/crc32)与 key;c8g(Graviton4)上同样核实。若为 crc32,`RssPlan` 需增加 crc32 实现或退化为校验上报。
3. **已定**:帧头自写(2–14 B,RFC 6455 §5.2),与 `fuzz_ws_deframe` 一起交付;不依赖 tungstenite(其依赖闭包带 `rand`/`log`/`http`)。
4. **已核实(否)**:rustls 0.23.45 unbuffered 的 `process_tls_records` 把每条应用数据记录的明文放进新 `Vec`,`WriteTraffic::encrypt` 也经 `PrefixedPayload` 分配;因此握手后改用 kernel connection + 本 crate 的原地记录层(§1),NET-285 在此基础上通过。热线程上的晚到票据只计数、不交给 rustls。
5. **已定**:按需逐条解密由本 crate 的记录层直接实现(一次一条完整记录),NET-089 覆盖。
6. **各端点协商结果**:TLS 版本、套件、是否 PQ 密钥交换;`Connector` 把结果写进 `BuildEvent`,首周汇总进 HostProfile。TLS 1.2 端点同样适用"不预加密"(显式 nonce 也是序号)。
7. **`SO_BINDTODEVICE` 权限**:6.12 上未绑定过设备的 socket 是否仍需 `CAP_NET_RAW`;systemd unit 预留 `AmbientCapabilities=CAP_NET_RAW`。
8. **`SO_BUSY_POLL = 0` 的效果与权限**:6.12 上把值调低不需要 `CAP_NET_ADMIN`;设为 0 后空的非阻塞 `recvmsg` 不进入 `napi_busy_loop`。方法:主机 `net.core.busy_read` 临时设非 0 的测试机上 perf/bpftrace 计 `napi_busy_loop` 调用次数,并实测 `net_poll_empty`。
9. **`SO_INCOMING_NAPI_ID` 在握手后是否已有效**(握手包经过同一队列);无效时改为首个数据帧之后由非热线程对同一 fd 读取(fd 在进程内共享,热线程不做这次系统调用;执行位置随 hft-core 的落点自检定)。
10. **scratch 会话 KeyUpdate**:确认 rustls 在 AES-GCM 次数上限附近是发 KeyUpdate 而非报错;`warm_interval_ns=50 µs` 时每小时 7.2×10⁷ 次,必要时缩短替换周期。
11. **短读即停的前提**:6.12 内核非阻塞 TCP `recvmsg` 在队列里还有数据时是否总是填满所给空间(即短读 ⇔ 读时队列已空)。方法:回环 + 对端连续写入,bpftrace 在 `tcp_recvmsg` 返回点比较返回值与 `sk_rmem_alloc`;若存在"有数据却短读",改为短读后再读一次、只在第二次也短读时停止。
12. **XPS 与并发发送**:6 个 gw 核与行情核同时发送时 `egress` 的 p99 与 TX 队列锁争用;方法:升级主机上按 deploy 的 XPS 配置与"不设 XPS"各跑一次并发发送基准(`net_send_loopback` 多线程版),结果写 Agent Note。

**实现顺序建议**:`ws.rs`(纯函数,先 fuzz)→ `tls.rs` + 内存对端属性测试(含按需解密)→ `conn.rs` 回环 → `connector.rs` → `pool.rs` → `rss.rs` → `rest.rs` → `txts.rs`。每一步先有基准再优化。

**开放问题**:QUICKACK 对下单连接的收益需 A/B(额外一次系统调用 ≈ 0.2–0.5 µs);有收益才打开并同步更新 hot-path-rules T7 的允许表。

</details>

连接失败与 warm 登录取消均回收作业与 arena 槽。相同 endpoint/src_ip 的后续 WS 作业等待 `reconnect_backoff_ms` 的指数退避,等待期间不连接、不消费脚本输入,其他端点可继续。调用者的 deadline 仍优先;REST 传输不自动重试,防止重发结果未知的修改请求。
