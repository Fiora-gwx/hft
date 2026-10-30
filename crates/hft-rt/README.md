---
description: "热进程与冷进程共用的运行时原语:SPSC 环(进程内/共享内存)、变长字节环、版本交换、TSC 时钟与校准、绑核与主机检查、大页与锁页、自旋与心跳;库不创建线程,由 apps 装配"
kind: crate
tier: hot
scope: core
status: spec
---

# hft-rt

## 概要

`hft-rt` 拥有线程间与进程间传递数据的原语(`Spsc` 的 `Producer`/`Consumer`、`ShmSpsc`、`ByteSpsc`、所有权交接 `Mailbox`、单写者值单元 `SeqCell`,以及只供可选热替换使用的 `EpochSwap`)、kill 字 `KillWord`、时间原语(`tsc_now`、`TscClock`、`TscUtc`、发布线 `UtcClock`/`UtcPublisher`、校准对采样)、CPU 与内存原语(`CpuSet` 绑核、`check_host` 主机检查、`HugeRegion` 大页、`lock_all` 锁页、预触摸)与等待/存活原语(`cpu_relax`、`Backoff`、`Heartbeat`、共享内存心跳块 `HeartbeatBlock`、`CachePadded`)。它不拥有任何业务状态、不创建线程、不决定哪个线程绑哪个核(装配在 [apps/hft-core](../../apps/hft-core/README.md))。调用者:feed/gw/sequencer(环与 TSC,热路径)、housekeeping 与 ctl-gateway(校准、心跳检查、回收,warm)、chain-core 与冷进程(共享内存环对端、心跳读取)。`unsafe` 允许,每处带 `// SAFETY:`。**主路径是 aarch64**(唯一发布目标 `aarch64-unknown-linux-gnu`,`target-cpu=neoverse-v1`,唯一归属 [development.md 目标 CPU](../../docs/development.md#目标-cpu));x86_64 是可选的 metal 剖面(触发条件:裸金属 + Onload 立项),代码以 `cfg(target_arch)` 分开,换架构即版本变化(按冷启动规则处理)。

## 目录

- [使用本组件](#使用本组件)
- [实现说明](#实现说明)
- [延迟与资源契约](#延迟与资源契约)
- [测试与验收](#测试与验收)
- [依赖](#依赖)
- [延伸阅读](#延伸阅读)
- [已知限制与后续工作](#已知限制与后续工作)

## 使用本组件

`latency_trace` 提供可选的线程私有测量 producer、有界 SPSC 与冷消费者；120 B 固定记录和显式丢点计数不参与业务状态。启动安装、溢出语义及文件格式归 [latency-report](../../tools/latency-report/README.md)。

阶段编号只追加；ReadTiming/FrameTiming 保留网络调用边界，旧记录不补造缺失的时间点。

### SPSC 环(进程内与共享内存同一实现)

```rust
pub fn spsc<T: Copy>(capacity: usize, mem: Backing) -> Result<(Producer<T>, Consumer<T>), RtErr>; // capacity 为 2 的幂且 ≥ 2,否则 Capacity
pub enum Backing { Heap, Huge(PageSize) }          // 进程内:普通堆或匿名大页

#[derive(Debug, PartialEq)]
pub enum RtErr {                                    // 本 crate 启动期与 warm 入口的统一错误(热路径入口不返回它)
    Capacity,                                       // 环容量不合规则
    NoMem,                                          // 分配、mmap 或大页不足
    ArenaFull,                                      // Arena 剩余空间不足
    PinMismatch { want: u16, got: u16 },            // 绑核后 sched_getcpu 复核不符
    BadMagic, BadVersion(u32),                      // HeartbeatBlock 等共享块的 magic/版本不符
    Os(i32),                                        // 其他系统调用失败(errno)
}

impl<T: Copy> Producer<T> {                         // Send、!Sync;恰好一个
    pub fn try_push(&mut self, v: T) -> Result<(), T>;          // 满 → Err(v),不阻塞
    pub fn write_batch(&mut self, max: usize) -> WriteBatch<'_, T>;
    pub fn free_hint(&self) -> usize;
}
impl<T> WriteBatch<'_, T> { pub fn push(&mut self, v: T) -> bool; pub fn commit(self); } // commit = 一次 Release 写 head
impl<T: Copy> Consumer<T> {
    pub fn try_pop(&mut self) -> Option<T>;
    pub fn read_batch(&mut self, max: usize) -> ReadBatch<'_, T>; // 最多两段 &[T](绕回时)
    pub fn len_hint(&self) -> usize;
}
impl<T> ReadBatch<'_, T> { pub fn slices(&self) -> (&[T], &[T]); pub fn release(self); } // release = 一次 Release 写 tail
```

跨进程版本用同一布局放在文件映射里;元素直接以 zerocopy trait 约束(没有自定义 trait,因此 hft-event 的槽类型不需要为 hft-rt 实现任何东西,不触及孤儿规则):

```rust
pub enum RingSide { Producer, Consumer }
pub struct ShmSpsc;
impl ShmSpsc {
    /// T 固定大小;size_of::<T>() 在 8 B–4 KiB 且为 8 的倍数、align_of::<T>() ≤ 8(编译期断言)。
    /// layout_id:布局身份,由元素类型的归属 crate 给出常量(如 hft-event 的 ext::EXT_SLOT_LAYOUT_ID,由 schema_hash 派生);两端不等即拒绝连接。
    pub fn create<T>(path: &Path, capacity: usize, page: PageSize, side: RingSide, layout_id: u64) -> Result<ShmEnd<T>, ShmErr>
        where T: zerocopy::FromBytes + zerocopy::IntoBytes + zerocopy::KnownLayout + zerocopy::Immutable + Copy;
    pub fn attach<T>(path: &Path, side: RingSide, layout_id: u64) -> Result<ShmEnd<T>, ShmErr>
        where T: zerocopy::FromBytes + zerocopy::IntoBytes + zerocopy::KnownLayout + zerocopy::Immutable + Copy;
}
pub enum ShmEnd<T> { Producer(ShmProducer<T>), Consumer(ShmConsumer<T>) } // 与进程内同名方法;另有 peer()、beat()、replaced() 与 header() -> &ShmRingHdr
// ShmEnd::into_producer()/into_consumer() -> Option<…>;create 的 capacity 不是 ≥ 2 的 2 的幂 → LayoutMismatch
pub struct PeerStatus { pub alive: bool, pub pid: u32, pub epoch: u64, pub last_beat: u64 }
// alive = 环头 state 为 Ready 且对端 pid 行非 0(只读共享内存,无系统调用;进程是否还在由调用方按 last_beat 停滞判定)
// replaced():warm 调用,stat 路径并与已映射文件的 inode 比较;inode 不同或路径不存在 → true
#[repr(u8)] pub enum ShmErr { Io = 1, BadMagic = 2, VersionMismatch = 3, LayoutMismatch = 4, SideTaken = 5, Closed = 6, NotReady = 7 }
```

- **槽大小** = `size_of::<T>()`(写进环头 `elem_size`,附着时校验):允许 8 B–4 KiB 且 8 B 对齐(`create`/`attach` 内编译期断言)。在用的共享环:external-venue `chain-link` 256 B 槽(`hft_ext_proto::ext::ExtSlot`,[external-venue-protocol §4](../../docs/subsystems/external-venue-protocol.md#4-载体))、[chain-grpc-relay](../../apps/chain-grpc-relay/README.md#与-chain-core-的接口)(可选)1 KiB 槽(`hft_ext_proto::relay::RelaySlot`);环文件放在 `rt.shm.dir`(`/dev/hugepages2M/hft/rings`)下。
- 共享内存环的元素是 `FromBytes`(任何字节都是合法的槽值,读出按值拷贝);对端是另一个进程,视为不可信边界,槽内容的结构校验由上层解码完成(`hft_event::ext`/`relay` 的帧解码);进程内环不做。进程内 `spsc<T: Copy>` 不加 zerocopy 约束:`FeedMsg`/`RepMsg` 是带数据的 enum,无法 `FromBytes`,也不跨进程。
- **环不承担可靠性**:生产者崩溃 → 未 `commit` 的元素丢失;消费者崩溃 → 已读未 `release` 的元素在重连后再次交付(至少一次)。丢失与重复由上层协议的序号处理([external-venue-protocol](../../docs/subsystems/external-venue-protocol.md)、[control-plane](../../docs/subsystems/control-plane.md) 的 `cmd_id`)。

### 变长字节环与版本交换

```rust
pub fn byte_spsc(capacity_bytes: usize, mem: Backing) -> Result<(ByteProducer, ByteConsumer), RtErr>; // 共享内存版本见已知限制
impl ByteProducer { pub fn reserve(&mut self, len: u32) -> Option<&mut [u8]>; pub fn commit(&mut self); } // len ≤ capacity/4
// capacity_bytes 为 2 的幂且 ≥ 64,否则 Capacity;reserve:len > capacity/4 → 永远 None,空间不足 → None;
// 未 commit 的预留被下一次 reserve 覆盖(从不发布);没有预留时 commit 为空操作;len = 0 的记录合法
impl ByteConsumer { pub fn peek(&mut self) -> Option<&[u8]>; pub fn advance(&mut self); pub fn has_data(&self) -> bool /* 只读,供 has_pending */; }
```

用于体积不定的载荷(交易所快照与 `L2Batch`、ctl 帧、跨进程大消息);定长事件用 `Spsc`。**ctl 环统一是 `ByteSpsc`**:容量取 EngineConfig 的环容量段(各环容量的取值唯一归属 [hft-core](../../apps/hft-core/README.md);ctl 环 ≥ 256 KiB),单条 ≤ 容量/4,帧格式为 hft-event 的 `CtlRecord`。

### 所有权交接、值单元与 kill 字

```rust
pub struct Mailbox<T: Send> { /* 单槽:state: AtomicU8 (Empty/Full) + UnsafeCell<MaybeUninit<T>>,放在 CachePadded 行上 */ }
impl<T: Send> Mailbox<T> {
    pub fn new() -> Mailbox<T>;
    pub fn split(&'static self) -> (MailSender<T>, MailReceiver<T>);    // 各恰好一个
}
impl<T: Send> MailSender<T>   { pub fn try_send(&mut self, v: T) -> Result<(), T>; }  // 槽满 → 原样退回,不阻塞
impl<T: Send> MailReceiver<T> { pub fn try_take(&mut self) -> Option<T>; }            // 一次 Acquire 读 + 移出
```

`Mailbox` 把一个**有所有权的大对象**(ctl-gateway 解析好的 `StrategyParams`、`RiskParams`)从 warm 线程交给 sequencer:发送方 move 进槽、Release 置 Full;sequencer 在处理对应 `ControlCmd` 时 `try_take`。旧对象的释放由 sequencer 通过反向 `Mailbox` 交还 warm 线程(热线程不 drop 大对象);反向邮箱的发送端在 `Spin` 线程上,`wake = None`,warm 线程靠 park 超时取回(见唤醒协议)。`unsafe` 只在本类型内部。

```rust
pub struct SeqCell<T: Copy> { /* 单写者 seqlock:seq: AtomicU64 + T;读者重试撕裂读 */ }
impl<T: Copy> SeqCell<T> { pub fn new(v: T) -> SeqCell<T>; pub fn store(&self, v: T); pub fn load(&self) -> T; pub fn try_load(&self) -> Option<T>; }  // try_load:单次尝试,写入中 → None   // store 只能由一个线程调用(调用约定)

#[derive(Copy, Clone)] pub struct TscUtc { pub base_tsc: Tsc, pub base_utc_ns: i64, pub mult: u64, pub shift: u32, pub err_ns: u32 }
impl TscUtc { pub fn utc_ns(&self, t: Tsc) -> i64; }     // base_utc_ns + ((t − base_tsc) × mult) >> shift(i128,有符号差,算术右移);一条 TSC → UTC 线,经 UtcClock 分发(见[时间](#时间))

pub type KillWord = CachePadded<AtomicU32>;              // 全系统 32 位;位布局 hft_types::KillBits(u32);行为归 hft-risk
const _: () = assert!(core::mem::size_of::<KillWord>() == 128);   // CachePadded 为 128 B
```

- housekeeping 每个校准节拍经 `UtcPublisher::update` `store` 一条新的 `TscUtc`;读者经 `UtcClock` 做一次 `try_load` 拿到一致的值(无分配、不自旋)。这是分发时钟换算的唯一方式(规则见[时间](#时间))。
- `KillWord`:apps/hft-core 启动期分配 `&'static [KillWord]`(每个 gw 一个),交给 hft-sequencer 的 LiveDriver(唯一写者,`store(Release)`)与各 gw(只读,`load(Acquire)`;判定含按账户 HARD 位,掩码与判定式归 [hft-venue](../hft-venue/README.md))。元素宽度与 `KillBits` 一致(u32),全系统没有 8 位的 kill 字;hft-risk 只输出每个 gw 的 `KillBits` 值、不依赖本 crate;快照恢复不重建这些字,由 LiveDriver 按恢复后的 cap 集合重写。hft-core 启动自检经真实的 `store`/`load` 逐位验证(每个位写入后由 gw 侧读回),不只比较常量。

### 版本交换(可选)

```rust
pub struct EpochSwap<T> { /* 单发布者,≤ 16 个读者,每读者一条确认缓存行 */ }
impl<T> EpochSwap<T> {
    pub fn new(initial: Box<T>, readers: usize) -> (SwapPublisher<T>, Vec<SwapReader<T>>, SwapReclaimer<T>);
}
impl<T> SwapPublisher<T> { pub fn publish(&mut self, next: Box<T>) -> Result<u64, Box<T>>; } // 无分配、无释放;退役槽满 → Err
impl<T> SwapReader<T>    { pub fn get(&mut self) -> &T; }      // 每轮循环顶部调用一次:一次 Acquire 读版本号
impl<T> SwapReclaimer<T> { pub fn reclaim(&mut self) -> usize; } // housekeeping:释放所有读者都已确认的旧版本
```

可选扩展:refdata 热替换时用它从 sequencer 分发到 feed/gw 线程(触发条件见 [hft-refdata 生命周期](../hft-refdata/README.md#生命周期变更--重启));当前设计不使用(refdata 变更 = 重启)。

### 时间

```rust
pub use hft_types::Tsc;        // 类型在 hft-types,读取在这里
pub fn tsc_now() -> Tsc;        // aarch64(主路径):MRS CNTVCT_EL0;x86_64(可选 metal 剖面):RDTSC。用于 rx_tsc 等"尽快取一个时间"
pub fn tsc_ordered() -> Tsc;    // aarch64:ISB; MRS CNTVCT_EL0;x86_64:RDTSCP。用于测量区间端点
pub fn tsc_hz_nominal() -> Option<u64>; // aarch64:CNTFRQ_EL0(架构保证恒定);x86:CPUID 0x15(若有)
// 全仓把这个计数器统称 "TSC"(类型 hft_types::Tsc);在 aarch64 上它就是 cntvct_el0
// TSC 只在单次启动(同一 boot_id)内有意义;本 crate 不提供跨开机的 TSC 换算

pub struct TscClock { pub hz_milli: u64, mult: u64, shift: u32, pub base: CalibPair }
impl TscClock {
    pub fn calibrate(cfg: &CalibConfig) -> Result<TscClock, ClockErr>;   // 启动期,冷,约 1 s:第 1 步不变性检查 + calibrate_with(&mut SysClock, …)
    pub fn calibrate_with<R: ClockReads>(src: &mut R, nominal_hz_milli: Option<u64>, cfg: &CalibConfig) -> Result<TscClock, ClockErr>; // 第 2–5 步;读数可注入
    pub fn from_hz_milli(hz_milli: u64, base: CalibPair) -> TscClock;    // shift = 32,mult = round(10^12 × 2^32 / hz_milli)
    pub fn delta_ns(&self, from: Tsc, to: Tsc) -> u64;                   // (Δ × mult) >> shift,u128 中间量;to < from → 0
    pub fn to_utc_ns(&self, t: Tsc) -> i64;                               // 基于 base 校准对,公式同 TscUtc::utc_ns
    pub fn tsc_utc(&self, pair: &CalibPair) -> TscUtc;                    // 以 pair 为基准、本时钟的 mult/shift 构造分发用的 TscUtc
}
/// 校准用的全部读数来源;SysClock 读真实计数器与 clock_gettime,测试注入确定的序列(任意平台)。
pub trait ClockReads { fn tsc(&mut self) -> Tsc; fn mono_raw_ns(&mut self) -> i64; fn realtime_ns(&mut self) -> i64; fn sleep_ms(&mut self, ms: u32); }
#[derive(Debug, PartialEq)] pub enum ClockErr { NotInvariant, Unstable, Mismatch, Phc(i32 /* errno */), Os(i32) }
#[derive(Copy, Clone)] pub struct CalibPair { pub tsc: Tsc, pub realtime_ns: i64, pub phc_ns: i64 /* 0 = 无 */, pub err_ns: u32 /* 误差界 */ }
pub fn sample_pair(clock: &TscClock, phc: Option<&Phc>) -> Result<CalibPair, ClockErr>; // warm:系统调用;= sample_pair_with(&mut SysClock, clock.hz_milli, phc 的读数, rt.calib.samples)
                                                                                         // samples 与 PHC 组数(rt.phc.samples)取 calibrate 时的 CalibConfig,记在 TscClock 内(from_hz_milli 取默认 8/10)
pub struct SysClock;   // ClockReads 的真实实现:tsc_ordered、CLOCK_MONOTONIC_RAW、CLOCK_REALTIME、thread::sleep
pub fn sample_pair_with<R: ClockReads>(src: &mut R, hz_milli: u64, phc: Option<&PhcOffset>, samples: u8) -> CalibPair; // 纯算法:取窗口最窄的一次,窗口按 hz_milli 换成 ns,err_ns 合并 PHC 误差界
pub struct Phc { /* /dev/ptp* 的 fd */ } impl Phc {
    pub fn open(dev: &Path) -> Result<Phc, ClockErr>;
    pub fn offset(&self, n: u8) -> Result<PhcOffset, ClockErr>;   // ioctl PTP_SYS_OFFSET_EXTENDED(n ≤ 25 组 sys_before/phc/sys_after)
}
pub struct PhcOffset { pub phc_ns: i64, pub sys_ns: i64 /* 最窄一组的中点 */, pub err_ns: u32 /* 最窄一组宽度的一半 */ }
pub struct DriftTracker { /* 最近 60 个校准对的线性拟合 */ }
impl DriftTracker {
    pub fn new(step_threshold_ns: u32) -> DriftTracker;           // 取 rt.clock.step_threshold_ns
    pub fn hz_milli(&self) -> Option<u64>;                        // 计数器相对 CLOCK_REALTIME 的频率 × 1000:窗口最小二乘斜率的倒数(精确整数);< 2 对或斜率非正 → None
    pub fn push(&mut self, p: CalibPair) -> ClockVerdict;          // Synced | Unsynced | Stepped{jump_ns}
}
// push:窗口(最近 60 对)不足 2 对 → Unsynced(new 之后的前两对、每次 Stepped 之后的下一对);否则以窗口内 realtime_ns 对 tsc 的最小二乘直线
// 预测本对的 realtime_ns,|实测 − 预测| > step_threshold_ns → Stepped{jump_ns = 实测 − 预测} 并清空窗口;否则 Synced。本对随后加入窗口(满 60 淘汰最老)

pub const UTC_SLEW_PPM: u64 = 500;        // 线超前 CLOCK_REALTIME 时的最大减速(内核 adjtime 上限),从不回跳
pub const UTC_RATE_TOL_PPM: u64 = 1_000;  // 实测频率偏离启动校准超过此值则不用(沿用启动频率)
                                          // 两个常数全仓只在这里定义:hft-sequencer 的引擎 UTC 线(节 32)复用同一组
#[derive(Clone)] pub struct UtcClock { /* &'static SeqCell<TscUtc> + 本读者最近一次读到的线 + 上次结果 */ }  // Send,非 Sync:每线程一个
impl UtcClock {
    pub fn shared(clock: &TscClock, horizon_ms: u32) -> (UtcClock, UtcPublisher); // 启动期:进程唯一的线(初值 = clock.tsc_utc(&clock.base))与它唯一的写者;泄漏一个单元
    pub fn fixed(line: TscUtc) -> UtcClock;  // 不再发布的线(测试、工具、探针);泄漏一个单元;From<TscUtc> 同此
    pub fn line(&self) -> TscUtc;            // 一次 SeqCell::try_load;写入中 → 本读者上次读到的线(不自旋)
    pub fn utc_ns(&self, t: Tsc) -> i64;     // max(本读者上次结果, line().utc_ns(t)):同一读者内单调不减(调用方传不减的 t)
}
pub struct UtcPublisher { /* 单元 + 当前线 + 启动频率 + horizon */ }
impl UtcPublisher {
    pub fn update(&mut self, pair: &CalibPair, hz_milli: Option<u64>) -> TscUtc; // 每校准节拍一次(housekeeping);返回发布的线
    pub fn current(&self) -> TscUtc;
}
```

**warm 读者的 UTC**:feed、gw、snap-fetch、acct-fetch、conn-builder、ctl-gateway、persist 与 paper feed 的墙钟(请求签名时间戳、REST 与建连截止、应用层 ping、raw 段头、ctl 过期判断)都从同一条发布线读,不按启动校准对外推(启动校准对 CLOCK_MONOTONIC_RAW 得到的是计数器的标称频率;Graviton 上真实频率与之差约 30 ppm,一天约 2.6 s)。`update(pair, hz)`:`H` = `hz`(`DriftTracker::hz_milli`;`None` 或偏离启动频率超过 `UTC_RATE_TOL_PPM` → 启动频率),`c` = 当前线在 `pair.tsc` 的值;`pair.realtime_ns ≥ c` → 发布目标线(基准 = 该校准对、频率 `H`,即在该点前跳 `realtime_ns − c`);否则(超前 `lead = c − realtime_ns`)→ 基准 `(pair.tsc, c)`、频率 `H` 的线乘以 `1 − min(lead / horizon, UTC_SLEW_PPM)`(`mult` 整数减小),一个 `horizon` 后与目标线相交(受 500 ppm 上限时继续减速),`err_ns` 加上 `lead`。发布线在每个校准对处不减;读者在发布前读到的 `t` 换到新线上可能比旧线低至多"发布延迟 × 斜率差"(亚微秒),由读者的 `max` 吸收,所以同一 `UtcClock` 的连续结果不回退。时长换算(超时、节拍、限速窗口的 TSC 数)取构造时的线,不随发布变化。hft-core 以 `horizon_ms = clock.interval_ms` 构造、在每个校准节拍 `DriftTracker::push` 之后 `update`。决策用的 UTC(sequencer `Now.utc_ns`)不读这条线,经 `UtcAnchor` 输入重锚,两者用同一组校准对。

热路径**只读 TSC**;TSC → UTC 的换算在冷读者(段头校准对 + `ClockStatus` 记录)与 warm 读者(`UtcClock`;gw 只在填请求时间戳时读一次)里完成。跨重启连续的时间不是 TSC:持久状态里的时刻用 SDK `Now.engine_ns`(sequencer 由 journal 记录的运行区间累加,停机不计)或 UTC 日历量([hot-path-rules D6](../../docs/hot-path-rules.md#确定性d))。sequencer 不读系统时钟:整点滚动等依赖时钟的决定由 live driver 写成 `Timer` 输入记录,决策用的 UTC 由 housekeeping 的校准对与 `DriftTracker::hz_milli` 经 Input `UtcAnchor` 重锚([hft-sequencer 引擎 UTC](../hft-sequencer/README.md#引擎-utc))。housekeeping 按校准节拍(取值归 [hft-core 线程计划](../../apps/hft-core/README.md#线程计划))调 `sample_pair` + `DriftTracker::push`,经 `UtcPublisher::update` 更新发布线,并把这一次的校准对与误差界写成一条 `ClockStatus`(Meta 记录,布局归 [hft-event](../hft-event/README.md#控制与系统0x04xx0x05xx));`Stepped` 时另写一条。离线换算按样本前后两条 `ClockStatus` 插值、逐样本给误差界,规则归 [latency-budget §2.1](../../docs/latency-budget.md#21-跨时钟域换算)。

### CPU、主机检查、内存

```rust
pub struct CpuSet([u64; 4]);                                  // ≤ 256 个逻辑 CPU
impl CpuSet {                                                 // 构造与集合运算(≥ 256 的 CPU 号忽略;parse_list 遇到返回 None)
    pub const fn new() -> CpuSet; pub fn from_cpus(c: &[u16]) -> CpuSet; pub fn parse_list(s: &str) -> Option<CpuSet>; // 内核列表语法 "0-3,8"
    pub fn insert(&mut self, c: u16); pub fn contains(&self, c: u16) -> bool; pub fn count(&self) -> u32; pub fn is_empty(&self) -> bool;
    pub fn union(&self, o: &CpuSet) -> CpuSet; pub fn intersects(&self, o: &CpuSet) -> bool; pub fn is_subset(&self, o: &CpuSet) -> bool;
}
pub fn pin_current_thread(cpu: u16) -> Result<(), RtErr>;     // sched_setaffinity 后用 sched_getcpu 复核;不符 → PinMismatch,系统调用失败 → Os
pub fn name_current_thread(name: &str);                       // ≤ 15 字节(更长按字节截断到 15),供 /proc 与 perf 识别

#[repr(transparent)] pub struct HostCheck(pub u64);           // 写进 HostProfile.host_checks;位只追加,含义见实现说明
impl HostCheck {
    CLASS_MISMATCH = 1 << 0; TSC_NOT_INVARIANT = 1 << 1; HOT_NOT_ISOLATED = 1 << 2; WARM_ON_ISOLATED = 1 << 3;
    HOT_NOT_NOHZ = 1 << 4; HOT_NOT_RCU_NOCB = 1 << 5; IRQ_ON_HOT = 1 << 6; TASK_ON_HOT = 1 << 7;
    HUGEPAGES_SHORT = 1 << 8; SHM_SHORT = 1 << 9; MLOCK_FAILED = 1 << 10; SWAP_ON = 1 << 11;
    NUMA_BALANCING_ON = 1 << 12; TIMER_MIGRATION_ON = 1 << 13;
    THP_NOT_MADVISE = 1 << 16; CNTVCT_TRAPS = 1 << 17; CPU_NOT_NEOVERSE_V1_COMPAT = 1 << 18;   // aarch64 表
    THP_ON = 1 << 24; SMT_ON = 1 << 25; CSTATE_DEEP = 1 << 26;                                 // x86_64 表
}
pub struct HostReport { pub bits: HostCheck, pub hard: HostCheck /* bits 中对 need.class 为 hard 的子集 */, pub lines: Vec<String> /* 冷,人读 */ }
pub fn check_host(root: &SysRoot, hot: &CpuSet, hk: &CpuSet, need: &HostNeeds) -> HostReport; // root 可指向测试夹具目录;不读 rt.host.mode
pub struct SysRoot { /* 根路径 + 是否真实根 */ }
impl SysRoot { pub fn real() -> SysRoot; pub fn fixture(dir: impl Into<PathBuf>) -> SysRoot; } // real = "/";fixture 的可用空间取其 statvfs 文件
pub enum Isa { Aarch64, X86_64 }                              // Isa::TARGET = 编译目标(非 aarch64 一律按 x86_64 表)
pub fn required_cpu_features() -> Vec<&'static str>;          // CPU_NOT_NEOVERSE_V1_COMPAT 的清单(cpuinfo 名);非 aarch64 为空
pub use hft_types::HostClass;                                 // 定义与编码归 hft-types;C32 = prod + sim 同机的默认候选,C16 = 条件候选(latency-budget §7);选定归 apps/hft-core;各类的 CPU/内存下限与 hard 位见实现说明
pub struct HostNeeds {
    pub class: HostClass, pub min_cpus: u16, pub max_cpus: u16, pub min_mem_mib: u32, pub isa: Isa,
    pub hugepages_1g: u32, pub hugepages_2m: u32, pub shm_bytes: u64,  // 本实例全部热段需求(按 hft-journal 的公式)
    pub held: HeldBySelf,                                             // 本实例已有段占用的量:空间检查从需求中扣除
    pub warm: CpuSet,                                                 // 线程计划里的非热核(warm/cold;sim 实例的全部核)
    pub irq: CpuSet,                                                  // 交易网卡 IRQ 专用核(`host.irq_cpus` 的 eni_b,启用 ENI-B2 时另含其核);不放线程
}
pub struct HeldBySelf { pub hugepages_1g: u32, pub hugepages_2m: u32, pub shm_bytes: u64 } // 由 hft-journal::held_by_instance 给出

pub enum PageSize { Huge1G, Huge2M, Normal /* 4 KiB:tmpfs 热段(dev2 的两个实例、升级主机的 sim 实例,按 hft-core 内存表)*/ }
pub struct HugeRegion { /* ptr, len, page, 可选文件 */ }
impl HugeRegion {
    pub fn anon(len: usize, page: PageSize, numa: Option<u8>) -> Result<HugeRegion, RtErr>; // len 向上取整到页;页不足 → NoMem
    pub fn file(path: &Path, len: usize, page: PageSize, mode: OpenMode) -> Result<HugeRegion, RtErr>; // hugetlbfs / /dev/shm
    pub fn prefault(&self);                                   // 每页读一个字节并原值写回(新建区域仍全为 0,已有内容不变)
    pub fn as_ptr(&self) -> *mut u8; pub fn len(&self) -> usize; pub fn page(&self) -> PageSize;  // len = 取整后的映射长度
}
pub enum OpenMode { CreateNew /* 已存在 → Os(EEXIST) */, OpenExisting /* 不存在 → Os(ENOENT);长度不足 len → NoMem */ }
pub fn lock_all() -> Result<(), RtErr>;                       // mlockall(MCL_CURRENT | MCL_FUTURE);失败 → Os(errno)
pub struct Arena { /* HugeRegion 上的启动期 bump 分配器,不释放 */ }
impl Arena {
    pub fn new(region: HugeRegion) -> Arena;                  // 接管区域,'static 生存期
    pub fn alloc_slice<T: Copy>(&mut self, n: usize, init: T) -> Result<&'static mut [T], RtErr>; // 按 align_of::<T>() 对齐;剩余不足 → ArenaFull
}
```

### 等待与存活

```rust
#[repr(C, align(128))] pub struct CachePadded<T>(pub T);     // 128 B:覆盖相邻行预取
pub fn cpu_relax();                                          // aarch64:ISB(Neoverse 上比 YIELD 更接近 x86 PAUSE 的退让效果,待实测);x86:PAUSE
pub struct Backoff { /* spin_limit, yield */ }               // 只给 warm/冷线程:先自旋 N 次再 yield_now,永不 sleep
impl Backoff { pub fn new(spin_limit: u32) -> Backoff; pub fn snooze(&mut self); pub fn reset(&mut self); } // 前 spin_limit 次 snooze 各 cpu_relax 一次,之后每次 yield_now 一次;reset 回到自旋

#[repr(C, align(128))]
pub struct Heartbeat {               // 一条 128 B 填充行(覆盖相邻行预取),单写者
    count: AtomicU64,                //   0 每轮主循环 +1
    pub name: [AtomicU8; 16],        //   8 线程名(ASCII,claim 时写一次;块与其他进程共享且经 & 写入,故为原子)
    pub core: AtomicU16, pub kind: AtomicU8, //  24 kind(HbKind 的码值):Sequencer=1 Feed=2 Gw=3 Persist=4 Housekeeping=5 Advisor=6 Ctl=7 Ext=8 ChainWorker=9(chain-core 的 ingest/sim/send/receipt)Mixed=10(线程计划里承载多个任务的线程;按线程 claim,一线程一槽)
    pub _pad: [u8; 101],
}
impl Heartbeat { pub const fn new() -> Heartbeat; pub fn beat(&self); pub fn read(&self) -> u64; pub fn name_bytes(&self) -> [u8; 16]; }   // beat:Relaxed 读 + Release 写,无 RMW、无系统调用
#[repr(u8)] pub enum HbKind { Sequencer = 1, Feed = 2, Gw = 3, Persist = 4, Housekeeping = 5, Advisor = 6, Ctl = 7, Ext = 8, ChainWorker = 9, Mixed = 10 }
pub struct HeartbeatWatch { /* last, stalled */ }
impl HeartbeatWatch {
    pub fn new() -> HeartbeatWatch;
    pub fn check(&mut self, hb: &Heartbeat) -> Progress; // Advanced | Stalled { for_checks: u32 }:首次 check 记基线并返回 Advanced;
}                                                        // 之后计数变化 → Advanced(清零),不变 → Stalled{连续未变次数}
```

```rust
#[repr(C, align(128))]
pub struct HeartbeatBlock {          // 4096 B;tmpfs 具名文件 /dev/shm/hft/<instance>/heartbeat(0640)
    pub beat: Heartbeat,             // 区 0:sequencer(与 apps/hft-core 已发布的偏移一致)
    pub hk: CachePadded<HkLine>,     // 区 1:housekeeping 写(启动一次 + 每 1 s)
    pub slots: [Heartbeat; 30],      // 区 2..31:其他热/warm 线程(feed、gw、persist、ext…),按 claim 顺序
}
#[repr(C)]
pub struct HkLine {
    pub magic: u64,                  // *b"HFTHB\0\x01\0"
    pub layout_version: u32,         // 1;读者遇到不认识的版本拒绝启动
    pub pid: u32,
    pub instance_id: u8, pub _rsv: u8 /* 0;保留,读者忽略 */, pub state: AtomicU8 /* Starting=1 Warmup=2 Running=3 Draining=4 Stopped=5 */,
    pub n_slots: AtomicU8,           // 已 claim 的区 2.. 数量
    pub _p: [u8; 4],
    pub boot_id: [u8; 16], pub started_utc_ns: i64, pub hk_utc_ns: AtomicI64,
}
impl HeartbeatBlock {
    pub fn create(path: &Path, instance_id: u8) -> Result<&'static HeartbeatBlock, RtErr>; // 启动期;O_CREAT|O_TRUNC、预触摸、magic 最后写;instance_id:prod = 1、sim = 2(只含模拟账户的实例,见 apps/hft-core),各实例的块在各自的 /dev/shm/hft/<instance>/
                                                                                           // 建成后 state = Starting、n_slots = 0、全部计数为 0
    pub fn open_ro(path: &Path) -> Result<&'static HeartbeatBlock, RtErr>;     // PROT_READ;magic 不符 → BadMagic,layout_version ≠ 1 → BadVersion(v)
    pub fn sequencer(&'static self) -> &'static Heartbeat;                     // 交给 Sequencer::boot
    pub fn claim(&'static self, name: &str, core: u16, kind: u8) -> Option<&'static Heartbeat>; // 装配期单线程调用;满 → None;name 超过 16 字节按字节截断
    pub fn set_state(&self, s: u8);
}
```

判活规则(housekeeping、kill-switch、watchtower 相同):`hk.state ∈ {Warmup, Running, Draining}` 且区 0 的计数在 `hb_stale_ms` 内前进;其他区的停滞只告警(某个 feed 线程卡住不等于进程死),由 watchtower 报 P1。`Stopped` = 干净退出。块的布局在这里定义;文件路径、创建时机、槽的 claim 与 `state` 的迁移时刻归属 [apps/hft-core](../../apps/hft-core/README.md#heartbeatblock);kill-switch 与 watchtower 只用 `open_ro` 只读映射(kill-switch 对本 crate 只用这一部分)。

热线程的等待规则:轮询多个来源(多条连接、多个环)的主循环**不插入** `cpu_relax`——它本身在做有用的检查;只盯一个环的线程每次空转调用一次 `cpu_relax`。`Backoff` 禁止出现在 hot 线程(clippy 禁用表,见 [hot-path-rules](../../docs/hot-path-rules.md))。任务内部永不等待;"让出 CPU"只发生在线程主循环的空闲处理里,由 `IdlePolicy` 决定(见下节)——`Spin` 线程永不让出,`SpinThenPark`/`Park` 线程只在**所有任务连续空闲**之后 park。sequencer 每轮主循环对 `HeartbeatBlock` 区 0 `beat()` 一次,housekeeping 每 1 s `check()`,前进才 `sd_notify`([13 R7](../../design/13-设计评审与优化.md));[kill-switch](../../apps/kill-switch/README.md) 读同一个计数。

### 线程计划的执行:任务轮转与空闲策略

线程布局由 apps 的 `[[threads]]` 配置决定:一个线程可以承载多个任务(任务清单与各主机类的映射归 [hft-core 线程计划](../../apps/hft-core/README.md#线程计划)),按固定顺序轮流调用;连续空闲时按 `IdlePolicy` 退让。本 crate 提供任务接口、轮转循环、空闲策略与 park/unpark 原语;**线程的创建与绑核仍只在 apps**(本 crate 不 spawn)。

```rust
/// 线程计划里的一个任务(热任务与 warm 任务都实现它;任务清单归 apps/hft-core 线程计划)。
pub trait PollTask {
    /// 非阻塞;每次调用的工作量有上限(任务自己的批量上限,如 gw.cmds_per_loop、feed.frames_per_poll、
    /// sequencer 的 B_ctl/B_rep/…);返回这次调用是否做了事。禁止在内部自旋等待或睡眠。
    fn poll_once(&mut self, now: Tsc) -> Polled;
    /// 本任务下一个需要被调用的 TSC 时刻(定时器、重试、心跳);None = 只由输入驱动。用于计算 park 超时。
    fn next_deadline(&self) -> Option<Tsc> { None }
    /// 本任务在 park 期间需要监听的 fd(socket);Epoll 后端用。默认无。
    fn wait_fds(&self, out: &mut WaitFds) {}
    /// 进入 park 前的最后检查:输入环/邮箱/socket 缓冲里是否有待处理的东西(只读,无副作用)。
    fn has_pending(&self) -> bool;
}
pub struct WaitFds { /* ≤ 64 个 fd */ }  impl WaitFds { pub fn push(&mut self, fd: i32) -> bool /* 满 → false */; pub fn as_slice(&self) -> &[i32]; }
#[derive(Copy, Clone)] pub struct Polled { pub did_work: bool }
#[derive(Copy, Clone, Debug, PartialEq)] pub enum IdleAction { Continue, Relax, Park { timeout_ns: u64 } }   // timeout 为 0 时不 park,按 Continue 处理

#[derive(Copy, Clone)]
pub enum IdlePolicy {
    Spin,                                        // 忙等,永不让出
    SpinThenPark { spin_us: u32, park_us: u32 }, // 连续空闲 spin_us 后 park,最多 park_us 或被唤醒
    Park { park_us: u32 },                       // 一轮空闲即 park
}
// 哪个线程用哪种策略、取值多少,按主机类归 apps/hft-core 线程计划(升级主机:prod 热线程 Spin,warm 线程 Park;
// dev2:不出现 Spin)。本 crate 只实现语义。

/// T 由 apps 定义:一个列出本进程全部任务类型的 enum,实现 PollTask(静态分派,无 dyn)。
pub struct TaskLoop<T: PollTask, const N: usize> { /* tasks: FixedVec<T, N>、policy、parker、idle_since */ }
impl<T: PollTask, const N: usize> TaskLoop<T, N> {
    /// parker 在 policy = Spin 时为 None;clock 只用于把 spin_us/park_us 换成 TSC 刻度。
    pub fn new(tasks: [T; N], policy: IdlePolicy, parker: Option<Parker>, clock: &TscClock) -> TaskLoop<T, N>;
    /// 一轮:按配置顺序对每个任务调用一次 poll_once;返回这一轮是否有任务做了事。
    pub fn round(&mut self, now: Tsc) -> bool;
    /// 每轮之后的空闲处理决定(下文"空闲处理"第 1–4 步;纯函数,时间由参数给出;run 以 tsc_now() 调用它)。
    pub fn idle_action(&mut self, now: Tsc, did_work: bool) -> IdleAction;
    /// 线程主循环:loop { round(); 空闲处理 };stop 置位后返回。由 apps 在线程函数里调用。
    pub fn run(&mut self, stop: &AtomicBool, hb: &Heartbeat);
    pub fn tasks(&self) -> &[T; N]; pub fn tasks_mut(&mut self) -> &mut [T; N];
}

pub struct Parker { /* state: CachePadded<AtomicU32>(RUNNING=0 PARKING=1 PARKED=2)、后端:Futex | Epoll{epfd, eventfd} */ }
#[derive(Clone)] pub struct Unparker { /* &'static 指向 Parker 的 state 与后端句柄 */ }
impl Parker {
    pub fn new(backend: ParkBackend) -> Result<Parker, RtErr>;   // 启动期;Epoll 后端创建 epfd + eventfd
    pub fn unparker(&self) -> Unparker;
}
impl Unparker { pub fn unpark(&self); }                          // 见唤醒协议;非 PARKED 时无系统调用
pub enum ParkBackend { Futex, Epoll }                             // 线程含 feed/gw/ext(有 socket)→ Epoll;否则 Futex

// 环与邮箱的唤醒挂钩:消费者线程的 idle ≠ Spin 且生产者线程不是 Spin 时,apps 在装配期把该线程的 Unparker 交给生产端;
// 生产者在 Spin 线程上一律 set_wake(None)(hot-path-rules T7),消费者靠 park 超时发现数据。
impl<T: Copy> Producer<T> { pub fn set_wake(&mut self, u: Option<Unparker>); }   // 启动期调用一次
// ByteProducer、Mailbox 发送端同样有 set_wake
```

**轮转**:`TaskLoop::round` 按 `[[threads]].tasks` 的顺序调用,每个任务每轮至多一次、每次有工作量上限,保证同线程任务公平;任务之间只经环交互(同线程的两个任务之间也走环,不共享可变状态)。环与事件语义不因合并而改变:sequencer 的输入顺序本来就由它自己的取环顺序决定并记入 journal,合并线程只改变"何时被调用",不改变记录内容,因此确定性不受影响。

**空闲处理**(`run` 内,每轮之后):

1. 这一轮 `did_work` → `idle_since = None`;否则记 `idle_since = now`(若为空)。
2. `Spin`:`cpu_relax()` 一次后继续(只在全部任务都空闲时;多来源轮询本身就是有用的检查)。
3. `SpinThenPark{spin_us, park_us}`:空闲持续 < `spin_us` → 同 `Spin`;否则进入 park,超时 = `min(park_us, 各任务 next_deadline − now)`(已过期的期限按 0 计,超时为 0 时不 park、直接下一轮)。
4. `Park{park_us}`:一轮空闲即 park,超时同上。
5. 从 park 返回(被唤醒或超时)→ 立即开始下一轮;Heartbeat 在每轮开头 `beat()`,park 超时 ≤ `park_us`;`park_us` 相对停摆判定窗口的上限在加载时校验(唯一归属 [hft-core 线程计划](../../apps/hft-core/README.md#线程计划)加载校验第 7 条),因此心跳在判定窗口内前进。

**唤醒协议**(生产者写环后能唤醒 park 的消费者,且热路径在消费者未 park 时没有系统调用):

```
消费者(准备 park)                                  生产者(每次 commit 之后)
  state.store(PARKING, SeqCst)                        head.store(new, Release)          // 正常提交
  for t in tasks: if t.has_pending() → 取消          fence(SeqCst)                     // 仅 set_wake(Some) 的生产端
  if !cas(PARKING → PARKED): 取消                      s = state.load(Relaxed)
  wait(超时)  // futex_wait(state, PARKED) 或           if s != RUNNING {
              // epoll_wait(epfd: eventfd + 任务 socket)    if state.swap(RUNNING) == PARKED { 系统调用唤醒 }
  state.store(RUNNING)                                    // PARKING:不发系统调用,消费者第 2 步会看到数据
                                                        }
```

- 正确性(无丢失唤醒):消费者"写 PARKING → 读环"与生产者"写环 → 读 state"构成 Dekker 对,两侧各一个 `SeqCst` 栅栏;要么消费者在第 2 步看到数据,要么生产者看到 `PARKING/PARKED` 并负责唤醒。属性测试用 `loom` 穷举。
- **`Spin` 生产者不唤醒**:生产者在 `Spin` 线程上时(sequencer、升级主机类上的 feed/gw)写给 `Park`/`SpinThenPark` 消费者的环与邮箱一律 `wake = None`——热线程上不出现 `futex_wake`/`write(eventfd)`;消费者最多晚 `park_us` 发现数据,所以这类环只承载不在 tick-to-trade 路径上的流量(旧段句柄、反向交还的旧配置、快照请求与给 warm 线程的通知)。这段延迟由消费者线程的 `park_us` 决定:升级主机上 warm 线程取 200–500 µs(非隔离核,唤醒成本低;取值归 [hft-core 线程计划](../../apps/hft-core/README.md#线程计划)),以免快照请求、重建连接与 advisor 尾随多等数毫秒。
- 成本:消费者是 `Spin` 线程时生产端 `wake = None`,没有任何额外指令(分支预测命中);消费者会 park 时,生产者每次 commit 多一个 `SeqCst` 栅栏 + 一次加载(aarch64 `dmb ish` ≈ 20–40 ns,x86 `mfence` ≈ 30 ns,待实测:`rt_wake_check`),**只有消费者处于 PARKED 时**才有一次 `futex_wake`/`write(eventfd)`(≈ 1–3 µs)。生产者批量提交时每批一次。
- socket 不会唤醒 futex:含 feed/gw/ext 任务的线程用 `Epoll` 后端,park 时 `epoll_wait` 同时等 eventfd 与这些任务的 socket(`wait_fds`),网络数据到达即醒;只含环输入的线程(如只有 `sequencer`)用 `Futex`。
- park 中的线程不在热路径上:被唤醒的额外延迟(调度 + 缓存冷)在 2 核主机上是 10–100 µs 量级,这正是 `dev2` 没有延迟验收的原因之一。

| 入口 | 预算 | 基准 |
|---|---|---|
| `TaskLoop::round`(4 任务全空闲) | ≤ 200 ns | `rt_taskloop_idle_round` |
| 生产端唤醒检查(消费者未 park) | ≤ 40 ns(含栅栏) | `rt_wake_check` |
| park → 被生产者唤醒 → 消费者开始下一轮 | 记录实测(futex 与 epoll 各一) | `rt_unpark_latency` |

### 配置项

| 名称 | 类型 | 默认 | 含义 | 来源 |
|---|---|---|---|---|
| `rt.calib.rounds` | u8 | 5 | 频率校准轮数(每轮 200 ms,取中位) | apps 配置 |
| `rt.calib.samples` | u8 | 8 | 每次校准对采样次数,取窗口最窄的一次;不设窗口上限,窗口一半作为 `err_ns` 如实报告 | 同上 |
| `rt.phc.samples` | u8 | 10 | `PTP_SYS_OFFSET_EXTENDED` 每次取的组数(≤ 25) | 同上 |
| `rt.calib.cpuid_tolerance_ppm` | u32 | 100 | 实测频率与 CPUID 标称的最大偏差 | 同上 |
| `rt.clock.step_threshold_ns` | u32 | 50 000 | 校准对偏移跳变超过即 `Stepped` | 同上 |
| `rt.phc.device` | 路径 | `/dev/ptp_ena` | PHC 设备([02 §4.2](../../design/02-硬件与托管方案.md)) | deploy 调优清单 |
| `rt.shm.dir` | 路径 | `/dev/hugepages2M/hft/rings` | 共享内存环目录(2 MiB 页 hugetlbfs,生产的全部共享环都在这里:`chain-link/`、`relay/<chain>/`;开发机可指向 tmpfs)。环文件一律是具名文件,不用匿名 memfd | 同上 |
| `rt.heartbeat.stall_checks` | u32 | 3 | housekeeping 连续几次未见前进判定停滞 | 同上 |
| `rt.host.mode` | enum | `prod` | `prod`:硬项失败拒绝启动;`dev`:全部降为警告 | 同上 |

```rust
pub struct CalibConfig { pub rounds: u8, pub samples: u8, pub phc_samples: u8, pub cpuid_tolerance_ppm: u32, pub step_threshold_ns: u32 }
pub enum HostMode { Prod, Dev }
pub struct RtConfig { pub calib: CalibConfig, pub phc_device: PathBuf, pub shm_dir: PathBuf, pub stall_checks: u32, pub host_mode: HostMode }
impl Default for RtConfig;                                      // = 上表"默认"列;hft-core 解析 [rt] 段时缺省键取它
impl RtConfig { pub fn validate(&self) -> Result<(), &'static str>; } // 失败返回键名:rounds、samples、stall_checks、step_threshold_ns ≥ 1;phc_samples ∈ [1, 25]
```

## 实现说明

<details><summary>实现要点</summary>

**平台**:Linux 专用设施(mmap、futex、epoll、`sched_setaffinity`、`mlockall`、PHC、`HeartbeatBlock` 与共享内存环)在其他平台照常编译,调用返回 `RtErr::Os(38)`(ENOSYS)、`ShmErr::Io` 或 `ClockErr::Phc(38)`;`name_current_thread` 为空操作。

**`Spsc` 布局**(进程内与共享内存相同;共享内存版前面多一个 4 KiB 文件头):

```
[CachePadded] producer 行: head: AtomicU64(下一个写入的单调下标), cached_tail: u64(生产者私有缓存)
[CachePadded] consumer 行: tail: AtomicU64(下一个读取的单调下标), cached_head: u64(消费者私有缓存)
slots: [MaybeUninit<T>; N],首槽对齐 128 B
```

1. 下标是单调 `u64`(不回绕),槽位 = `idx & (N − 1)`;满 = `head − tail == N`。
2. `try_push`:若 `head − cached_tail == N`,Acquire 读 `tail` 刷新 `cached_tail`,仍满则返回 `Err`;写槽;Release 写 `head + 1`。只有在看起来满时才读对方的行(rigtorp 缓存下标优化,[research/06 §1.1](../../design/research/06-data-plane-replay-observability.md))。
3. 批量:`write_batch` 一次刷新 `cached_tail` 得到可写数,逐个写槽,`commit` 一次 Release 写 `head`;消费侧对称。批量发布是降低共享行写次数的关键(研究结论:Disruptor 批量发布)。
4. `cached_*` 字段与本方的游标同行、只被本方写,不与对方共享写。

**共享内存文件头**(`ShmRingHdr`,4 KiB,第一页):

```
行 0(初始化后只读): magic *b"HFTRING1", version: u32 = 1, state: AtomicU32 (Creating=0 Ready=1 Closed=2),
                    layout_id: u64, elem_size: u32, elem_align: u32, capacity: u64, epoch: u64, boot_id: [u8; 16], creator_pid: u32
行 1: producer: PeerLine { pid: AtomicU32, start: AtomicU64(/proc 启动时间,防 pid 复用), beat: AtomicU64 }
行 2: consumer: PeerLine(同上)
其后 4 KiB 起:producer 行、consumer 行、slots(同进程内布局)
```

跨进程协议:

1. **创建**(约定由生命周期更长的一方创建;hft-core 创建它参与的所有环):写入临时名 → 填行 0,`state = Creating` → 预触摸 → `rename` 到正式名 → Release 写 `state = Ready`。`epoch` = 同名旧文件可读且 magic 有效时为旧 `epoch + 1`,否则 1(单调)。
2. **附着**:打开(失败 → `Io`)→ magic 不符 `BadMagic`、`version ≠ 1` → `VersionMismatch`;`layout_id`/`elem_size`/`capacity` 任一不符 `LayoutMismatch`(不猜测、不迁移)→ `state = Creating` → `NotReady`、`Closed` → `Closed`(都立即返回,不等待;调用方在 warm 路径重试)→ 对本方 `*_pid` 做 CAS 占位:原值为 0 或其 `(pid, start)` 已不存在才能占;否则 `SideTaken`。
3. **存活**:两端各自在循环中 `beat()` 本方行;对端用 `peer()` 读(一次 Acquire,无系统调用)。存活判定由调用方按心跳停滞次数决定。
4. **干净退出**:创建者写 `state = Closed`;附着方释放本方 pid 行。
5. **崩溃后**:创建者重启时 `unlink` 旧文件并新建(新 inode、新 `epoch`);对端见到 `Closed`、心跳停滞或文件 inode 变化(warm 路径每秒 `stat` 一次)即重新附着,并在协议层用序号重放/去重。附着方崩溃后重启直接附着现存环:生产者从共享的 `head` 继续,消费者从共享的 `tail` 继续。

**`ByteSpsc`**:记录 = `len: u32 | 0: u32 | payload | 8 B 对齐填充`;尾部放不下时写一条 `len = WRAP` 标记并从 0 继续;单条上限 capacity/4,保证总能前进。

**`EpochSwap`**:`current: AtomicPtr<T>` 与 `version: AtomicU64` 同行;每读者一条 `ack: AtomicU64` 行(读者 `get()` 发现版本变化时 Release 写入);发布者把旧指针放进 4 个退役槽之一;`reclaim` 找出 `min(ack) ≥ 退役版本` 的槽并 `drop`。退役槽满时 `publish` 返回 `Err(next)`,调用方(sequencer)把命令结果置 `Rejected(Busy)`——不阻塞。

**TSC 校准算法**(`calibrate`):

1. 不变性:**aarch64**:读 `CNTFRQ_EL0`(非 0)、`/sys/devices/system/clocksource/clocksource0/current_clocksource == "arch_sys_counter"`,并确认用户态可读 `CNTVCT_EL0`(内核 `CNTKCTL_EL1.EL0VCTEN`,读一次不陷入即可);**x86(可选 metal 剖面)**:CPUID `0x8000_0007:EDX[8]`、`/proc/cpuinfo` 含 `constant_tsc nonstop_tsc`、clocksource 为 `tsc`。失败 → `ClockErr::NotInvariant`(`prod` 模式拒绝启动)。
2. 标称频率:aarch64 取 `CNTFRQ_EL0`(Graviton 的计数器频率与分辨率待核实,方法见开发备忘:分辨率若为数 ns,`rx_tsc` 等测量的粒度随之变粗,基准如实记录);x86 若 CPUID `0x15` 给出 crystal 与比值则计算标称值,否则无。
3. 实测:`rounds` 轮,每轮在开始与 200 ms 后各采一对 `(tsc, CLOCK_MONOTONIC_RAW)`(采样用"TSC—读时钟—TSC"夹逼,取 `samples` 次中窗口最窄的一次,TSC 取两读数中点),`hz = Δtsc / Δns`,取中位;各轮离散(`(最大 − 最小) / 中位`)> 1 ppm 报 `Unstable`;有标称值且 `|中位 − 标称| / 标称` > `cpuid_tolerance_ppm` 报 `Mismatch`(恰等于不报)。
4. 由 `hz_milli` 计算 `mult/shift`(`shift = 32`,`mult = round(1e9 × 2^32 / hz)`),使 `delta_ns` 为一次 128 位乘与移位。
5. 按第 4 步的频率取一个 `sample_pair_with`(无 PHC)作 `base`;`calibrate` 不打开 PHC 设备,需要 PHC 的调用方随后以 `sample_pair(&clock, Some(&phc))` 取校准对。

`sample_pair`:(a)TSC ↔ `CLOCK_REALTIME`:读 TSC → `clock_gettime(CLOCK_REALTIME)` → 读 TSC,`samples` 次取窗口最窄者(同宽取最早一次),TSC 取中点(向下取整),`err_ns` = 窗口换算成 ns 后的一半(向上取整;不设 200 ns 之类的硬窗口,如实报告);(b)PHC ↔ 系统时钟:`ioctl(PTP_SYS_OFFSET_EXTENDED)` 一次取 `rt.phc.samples` 组 `(sys_before, phc, sys_after)`,取最窄一组,误差界 = 其宽度一半,与 (a) 的误差界相加得 `err_ns`;`phc_ns = realtime_ns + (PhcOffset.phc_ns − PhcOffset.sys_ns)`。无 PHC 时 `phc_ns = 0`、`err_ns` 只含 (a)。不用 `clock_gettime(FD_TO_CLOCKID)` 读 PHC(无法给出误差界)。ENA PHC 读取上限约 125 次/s,每秒一次 ioctl(10 组)在限内([02 §5](../../design/02-硬件与托管方案.md));chrony 若也读 PHC,两者合计须在上限内(由 [hostprobe](../../tools/README.md#hostprobe) 检查)。

**`HostCheck` 位**(`hard` 表示 `prod` 模式下拒绝启动;hard 集合按主机类 `HostNeeds.class` 区分;检查项**按 ISA 分表**,`check_host` 只跑当前架构的表。交易网卡的 IRQ 落点与 busy poll 参数的 hard 校验在 [hostprobe](../../tools/README.md#hostprobe),见 [hot-path-rules T12](../../docs/hot-path-rules.md#线程t)):

公共项(两种 ISA):

| 位 | 含义 | 来源 | hard |
|---|---|---|---|
| `CLASS_MISMATCH` | 实际在线逻辑 CPU 数 ∉ [`HostNeeds.min_cpus`, `max_cpus`]、`MemTotal` < `min_mem_mib`、编译目标 ISA ≠ `HostNeeds.isa`,或(`metal`)物理核数 < `min_cpus`;装配按主机类填入的取值:`dev2`:2 vCPU、≥ 3.5 GiB;`c4`:4 核;`c8`:8 核;`c16`:16 vCPU、≥ 30 GiB;`c32`:32 vCPU、≥ 60 GiB;`metal`:≥ 24 物理核 | `/sys/devices/system/cpu/online`、`/sys/devices/system/cpu/cpu*/topology/{core_id,physical_package_id}`(物理核 = 二元组去重)、`/proc/meminfo`、`cfg!(target_arch)` | 是(全部主机类) |
| `TSC_NOT_INVARIANT` | 见校准第 1 步 | 计数器寄存器、clocksource | 是 |
| `HOT_NOT_ISOLATED` | 热核不在 `isolated` 列表 | `/sys/devices/system/cpu/isolated` | 是(`c8`/`c16`/`c32`/`metal`);`dev2`/`c4` 不检查 |
| `WARM_ON_ISOLATED` | `HostNeeds.warm` 中有核落在 `isolated` 列表(同机的 sim 实例或 warm/cold 线程占了 prod 的热核) | 同上 | 是(`c8`/`c16`/`c32`/`metal`);`dev2`/`c4` 不检查 |
| `HOT_NOT_NOHZ` | 热核不在 `nohz_full` | `/sys/devices/system/cpu/nohz_full` | 否 |
| `HOT_NOT_RCU_NOCB` | 热核或 IRQ 专用核(`HostNeeds.irq`)不在 cmdline 的 `rcu_nocbs` 里 | `/proc/cmdline` | 否 |
| `IRQ_ON_HOT` | 任一 IRQ 亲和包含热核 | `/proc/irq/*/smp_affinity_list` | 否(告警;IRQ 落点的 hard 校验在 hostprobe,含 NVMe 等 managed IRQ 的 `effective_affinity` 不落在 IRQ 专用核) |
| `TASK_ON_HOT` | 其他任务允许在热核或 IRQ 专用核运行且最近运行过(IRQ 专用核上"保持唤醒"的允许形式登记在 [hostprobe](../../tools/README.md#hostprobe),不计入) | `/proc/*/task/*/stat`、`status` | 否 |
| `HUGEPAGES_SHORT` | 空闲大页 < 需求 − 本实例已有段占用的大页(`HostNeeds.held`;崩溃后遗留的本实例段由恢复复用或退役,不重复计;同机另一实例的段不扣除) | `/sys/kernel/mm/hugepages/*/free_hugepages` | 是(`c8`/`c16`/`c32`/`metal`);tmpfs 热段(dev2 的两个实例、升级主机的 `sim` 实例)改查 tmpfs 可用空间 < `shm_bytes − held.shm_bytes`(`SHM_SHORT`,hard) |
| `MLOCK_FAILED` | `mlockall` 未生效 | `/proc/self/status` 的 `VmLck` 为 0 kB | 是 |
| `SWAP_ON` | 有启用的 swap | `/proc/swaps` | 否(`dev2` 由 watchtower 的 `HostPressure` 盯) |
| `NUMA_BALANCING_ON` | `kernel.numa_balancing != 0` | `/proc/sys/kernel/numa_balancing` | 否 |
| `TIMER_MIGRATION_ON` | `kernel.timer_migration != 0` | `/proc/sys/kernel/timer_migration` | 否 |

aarch64(主路径,Graviton c7g/c8g):

| 位 | 含义 | 来源 | hard |
|---|---|---|---|
| `THP_NOT_MADVISE` | THP 不是 `madvise`([02 §4.3](../../design/02-硬件与托管方案.md):Graviton 用 `madvise`,不是 `never`) | `/sys/kernel/mm/transparent_hugepage/enabled` | 否 |
| `CNTVCT_TRAPS` | 用户态读 `CNTVCT_EL0` 陷入内核(虚拟化配置错误) | 读计数器耗时 > 100 ns | 是 |
| `CPU_NOT_NEOVERSE_V1_COMPAT` | `/proc/cpuinfo` 缺本二进制编译时启用的任一目标特性(`neoverse-v1` 基线,如 `sve`、`lrcpc`、`atomics`;清单 = 编译期 `cfg!(target_feature)` 集合映射到 cpuinfo 名) | `/proc/cpuinfo` Features | 是(二进制按 `neoverse-v1` 编译) |

x86_64(可选 metal 剖面):

| 位 | 含义 | 来源 | hard |
|---|---|---|---|
| `THP_ON` | THP 不是 `never` | `/sys/kernel/mm/transparent_hugepage/enabled` | 否 |
| `SMT_ON` | SMT 开启 | `/sys/devices/system/cpu/smt/active` | 否 |
| `CSTATE_DEEP` | cmdline 缺 `idle=poll`/`max_cstate=0` | `/proc/cmdline` | 否 |

非 hard 项写进 `HostProfile` 并由 [watchtower](../../apps/watchtower/README.md) 告警;调优参数本身由 [deploy](../../deploy/README.md) 设置,本 crate 只核验。`rt.host.mode` 由调用方应用(`prod`:`HostReport.hard ≠ 0` 拒绝启动;`dev`:只告警);`check_host` 的 `hard` 恒按 `need.class` 计算。各位的判定是对 `SysRoot` 下文件内容的纯计算(夹具目录可在任意平台驱动当前架构的表);只有 `TSC_NOT_INVARIANT` 的寄存器读取与 `CNTVCT_TRAPS` 的耗时测量读真实硬件,`SysRoot` 不是真实根目录时这两项只按文件部分判定。

**规划源文件**

| 文件 | 职责 |
|---|---|
| `src/ring/spsc.rs` | `Spsc` 核心(游标、批量),对 backing 泛型 |
| `src/ring/shm.rs` | `ShmRingHdr`、创建/附着/存活/关闭协议 |
| `src/ring/bytes.rs` | `ByteSpsc` |
| `src/swap.rs` | `EpochSwap`(可选) |
| `src/mailbox.rs` | `Mailbox`、`SeqCell`、`KillWord` |
| `src/tsc.rs` | `tsc_now`/`tsc_ordered`(`cfg(target_arch)` 两套)、`TscClock::calibrate`、`mult/shift` |
| `src/utc.rs` | `UtcClock`、`UtcPublisher`(发布线的读写规则) |
| `src/host_aarch64.rs` / `src/host_x86.rs` | 按 ISA 分开的主机检查表 |
| `src/calib.rs` | `sample_pair`、`Phc`、`DriftTracker` |
| `src/cpu.rs` | `CpuSet`、绑核、线程命名 |
| `src/host.rs` | `check_host`、`SysRoot`(可指向夹具目录;指向夹具时文件系统可用空间取自夹具的 `statvfs` 文件(挂载点 → 可用字节),真实根目录取 `statvfs(2)`) |
| `src/mem.rs` | `HugeRegion`、`Arena`、`lock_all`、预触摸 |
| `src/wait.rs` | `CachePadded`、`cpu_relax`、`Backoff`、`Heartbeat`、`HeartbeatWatch` |
| `src/idle.rs` | `PollTask`、`TaskLoop`、`IdlePolicy`、`Parker`/`Unparker`(futex 与 epoll + eventfd 两种后端)、生产端唤醒挂钩 |
| `src/hb_block.rs` | `HeartbeatBlock`、`HkLine`、创建/只读打开/claim |

</details>

## 延迟与资源契约

测量机器:参考机(M1.5 起按需租用的同型实例:选档前 `c8g.4xlarge`,选档后为选定的统一升级主机类;可选的 `metal` 另行验收;[latency-budget §6](../../docs/latency-budget.md#6-验收方法)),两个隔离核同 socket;环元素 64 B,除非注明。

| 入口 | 预算 | 基准 |
|---|---|---|
| 进程内单跳(生产者 `commit` → 消费者看到,乒乓/2) | p50 ≤ 70 ns,p99 ≤ 150 ns([01 §3.1](../../design/01-总体架构.md) 给 50–100 ns) | `rt_spsc_hop` |
| `try_push`/`try_pop`(不满、不空,摊还) | ≤ 5 ns | `rt_spsc_push_pop` |
| 批量吞吐 | ≥ 100 M 元素/s(8 B 元素)、≥ 20 M 元素/s(128 B) | `rt_spsc_throughput` |
| 共享内存单跳 | 进程内预算 + ≤ 20 ns | `rt_shm_spsc_hop` |
| `ByteSpsc` 1 KiB 记录单跳 | p50 ≤ 150 ns | `rt_bytes_hop_1k` |
| `tsc_now` / `tsc_ordered`(aarch64 主路径) | ≤ 10 ns / ≤ 20 ns(c8g 上待实测) | `rt_tsc_now` |
| `KillWord` 读(gw 发送前,`load(Acquire)`) | ≤ 2 ns(无写者争用) | `rt_killword_load` |
| `delta_ns` | ≤ 3 ns | `rt_tsc_delta` |
| `SwapReader::get`(无变化,可选) | ≤ 2 ns | `rt_swap_get` |
| `Mailbox::try_take`(空) / `SeqCell::load` | ≤ 2 ns / ≤ 5 ns | `rt_mailbox`、`rt_seqcell` |
| `UtcClock::utc_ns`(无写者争用) | ≤ 8 ns | `rt_utc_clock` |
| `Heartbeat::beat` | ≤ 1 ns(无读者争用时) | `rt_heartbeat` |
| `cpu_relax` 本身耗时 | 记录实测值(Neoverse 上 `ISB`/`YIELD` 各测一次;可选 metal 剖面另测 x86 PAUSE) | `rt_pause_cost` |
| `sample_pair` | warm,≤ 2 µs | `rt_sample_pair` |

热路径入口零分配、零系统调用;`calibrate`、`check_host`、`HugeRegion::*`、`ShmSpsc::create/attach` 是启动期/warm 调用,允许系统调用与分配。每个环两条游标行 + 头行;游标行只在批次边界写一次。

## 测试与验收

用例与验收见 [TESTS.md](TESTS.md)。

## 依赖

- 内部:[hft-types](../hft-types/README.md)(`Tsc`、ID)。
- 外部:`libc`(系统调用)、`zerocopy`(`ShmSpsc` 元素约束);dev:`loom`、`rtrb`(对照基准)、`criterion`。
- 禁止:`tokio`、`crossbeam`(自写 `CachePadded` 与环,避免依赖闭包进入热 crate)、`parking_lot`/任何锁、`quanta`/`minstant`(TSC 读取自写,两条指令)、`std::time::Instant` 于热路径。

## 延伸阅读

- [13 O1 一套环实现进程内与共享内存](../../design/13-设计评审与优化.md)、[13 R7 心跳](../../design/13-设计评审与优化.md)、[10 §3–4](../../design/10-语言分工与代码组织.md)、[02 §4–5 调优与时间](../../design/02-硬件与托管方案.md)
- [research/04 §2.1、§2.5、§9](../../design/research/04-oss-frameworks-and-patterns.md)(环库对比、时钟与亲和、热路径清单)、[research/06 §1](../../design/research/06-data-plane-replay-observability.md)(一热写多冷读、seqlock、预触摸)
- 使用方:[hft-journal](../hft-journal/README.md)、[hft-venue](../hft-venue/README.md)、[hft-sequencer](../hft-sequencer/README.md)、[hft-ctl](../hft-ctl/README.md);决策记录 `one-ring-in-proc-and-shm`([Agent Notes](../../.agents/notes/README.md))

## 已知限制与后续工作

- `ByteSpsc` 的共享内存版本未实现(当前没有跨进程变长载荷的使用者;ctl 环在进程内),首个使用者出现时按 `ShmSpsc` 的文件头与协议补上。
- 只支持 SPSC;多订阅者广播(一个生产者、多个共享内存消费者)不在范围,需要时评估 iceoryx2 或加 SPMC 序号广播([13 O1](../../design/13-设计评审与优化.md))。
- 跨机传输(hft-core ↔ 另一台主机上的 chain-core)不在本 crate;走 UDP + 序号/确认,见 [external-venue-protocol](../../docs/subsystems/external-venue-protocol.md)。
- CI 只用 GitHub 托管 runner:PR 质量门(`ubuntu-latest`,x86_64)编译并测试 x86_64 路径;aarch64 路径(`CNTVCT_EL0`、`ISB`、Graviton 主机检查表)的单元测试在按 tag 的 `ubuntu-24.04-arm` 发布作业里先于打包运行;基准验收在 M1.5 起按需租用的 aarch64 参考机(c8g 系列)上做;可选的 metal 剖面立项时另行验收。
- 主机检查不修改系统设置;修正由 deploy 的调优脚本完成。

### 开发备忘

<details><summary>给实现者</summary>

- **待核实**:Graviton(c7g/c8g)`CNTFRQ_EL0` 的值与计数器分辨率、用户态读开销(方法:`rt_tsc_now` 基准 + 读 `CNTFRQ_EL0`);`cpu_relax` 用 `ISB` 还是 `YIELD`(方法:`rt_pause_cost` 与 `rt_spsc_hop` 两种实现对比);`/dev/ptp_ena` 设备名与 `clock_gettime` 对 PHC 的实际开销(方法:目标实例上 `ls /dev/ptp*`、`ethtool -T`,基准 `rt_sample_pair`);hugetlbfs 上 `rename` 是否支持(不支持则改为"写 `state = Ready` 前对端不附着",用 `Creating` 状态兜底)。
- 实现顺序:`wait.rs` → `spsc.rs`(+ loom)→ `tsc.rs` → `mem.rs` → `shm.rs` → `bytes.rs` → `swap.rs` → `host.rs` → `calib.rs`。
- `Producer`/`Consumer` 不实现 `Clone`;`Drop` 不做任何跨线程通知。
- 所有原子序在代码注释中写明配对关系(哪个 Release 与哪个 Acquire 配对)。

</details>
