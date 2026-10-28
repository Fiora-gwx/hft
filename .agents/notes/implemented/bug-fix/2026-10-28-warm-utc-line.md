# Agent Note: warm 读者的 UTC 改读发布线

Status: implemented

> **负责人确认(2026-10-28)**:本变更(下单签名时间戳与截止)与规格冻结期新增的用例编号 RT-169–RT-172、VEN-404 获负责人确认,见 [changes.md](../../../../docs/changes.md) 2026-10-28 行。

## Problem

hft-rt README 规定 warm 读者经 `SeqCell<TscUtc>` 取 TSC → UTC 换算、由 housekeeping 每次校准后更新,但实现里没有这条分发:hft-core 在启动时 `clock.tsc_utc(&clock.base)` 按值交给 gw、feed、snap-fetch、acct 连接、ctl-gateway,conn-builder 与 persist 直接用 `TscClock::to_utc_ns`(启动校准对)。启动频率对 CLOCK_MONOTONIC_RAW 校准,得到的是计数器的标称频率;Graviton 上真实频率低约 30 ppm,这些线程的墙钟与引擎 UTC(修正前)一样每天落后约 2.6 s。它们不进决策回放,但离开进程:Binance 签名请求的 `timestamp` 与 `recvWindow`(gw 以 `gw.stale_guard_ms` = 1 500 ms 收紧,约 14 h 后 −1021)、Bitget/OKX 的时间戳、Polymarket 的 L2 `POLY_TIMESTAMP`、订单 `timestamp` 与 GTD `expiration`、ctl 命令的过期判断(签发方是 CLOCK_REALTIME)。引擎 UTC 的修正见 [引擎 UTC 重锚](2026-10-28-engine-utc-reanchor.md)。

## Decision

hft-rt `UtcClock`(读者)/`UtcPublisher`(唯一写者)包装一个泄漏的 `SeqCell<TscUtc>`。housekeeping 每个校准节拍在 `DriftTracker::push` 之后 `update(pair, hz_milli)`:落后则发布以该校准对为基准、实测频率的目标线(前跳);超前则从当前值出发、把频率降低 `min(lead / horizon, 500 ppm)`,约一个节拍后与目标线相交。只发布一条直线,读者的代价与原来按值持有的 `TscUtc` 相同,只多一次 `try_load`;写入进行中时用本读者上次读到的线,不自旋(gw 是热线程)。读者结果取本读者上次结果的 max:线在每个校准对处不减,但读者在发布前读到的 `t` 换到新线上可能低"发布延迟 × 斜率差"(亚微秒),由 max 吸收。时长换算(超时、节拍、限速窗口的 TSC 数)仍取构造时的线,避免定时器随校准变化。conn-builder 的 `now_ns` 与 gw/snap 的 `deadline_ns` 读同一条线。

## Alternatives considered

**每个读者自己拿 `ClockStatus` 校准对**:要把 hk 的观测再分发到每个线程,本质就是这条发布线,且各线程各自换算更容易不一致。

**发布"目标线 + 衰减超前量"两段(同 sequencer 的 UTC 线)**:每次读要多一次乘法和比较,且超前量只有 NTP 回跳时才非零;每秒重发一条减速直线已经足够,决策 UTC 的精确复现不依赖这里。

**线程本地的单调下限**:最直接,但 `thread_local!` 被 D3 禁用;下限放在读者句柄(`Cell`)里,一个线程一个句柄。

**全局 `fetch_max` 下限**:跨线程单调,但每次读都要写一个共享缓存行,gw 热线程上不可接受;跨线程的墙钟本来就不比较先后。

**允许回跳(直接跳到目标线)**:最简单;CLOCK_REALTIME 回跳时签名时间戳与 Polymarket 订单 `timestamp`(代替 nonce 保证唯一)会倒退,改用与 sequencer 相同的 500 ppm 减速。

## Consequences

模拟一天(计数器慢 30 ppm、12 h 再慢 1 ppm、18 h CLOCK_REALTIME 回跳 2 ms)里发布线在第一次更新后与 CLOCK_REALTIME 相差 ≤ 50 µs,第二次更新起 ≤ 1 µs;回跳 2 ms 用 4 s 减速消化,期间墙钟至多超前 2 ms(Binance 拒绝超前服务器 1 s 以上的时间戳,大于约 1 s 的回跳需要更久才能恢复,与 sequencer 的规则相同)。READY 之前(housekeeping 开始校准节拍之前)这些线程读启动线。`UtcClock::fixed` 每次泄漏一个单元,只用于测试、基准与短时工具(latency-slice、pm-md-probe)。sequencer 自己的 live driver UTC(整点滚动等)仍按最近一条 `ClockStatus`,不读这条线。
