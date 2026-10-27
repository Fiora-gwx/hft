# Agent Note: 引擎 UTC 经 UtcAnchor 输入重锚

Status: implemented

## Problem

SDK `Now.utc_ns` 由段头校准对按 `EngineClock.tsc_hz_milli` 换算。hft-core 的段头一直写启动时的校准对,频率在启动时对 CLOCK_MONOTONIC_RAW 校准;MONOTONIC_RAW 由计数器按标称频率(aarch64 `CNTFRQ_EL0`)换算,所以校准得到的是标称频率本身。pm-strategy-euw1(c8g,Graviton)上计数器的真实频率比标称频率低约 30 ppm:19 次运行(0.3–24.7 h)的引擎 UTC − 接收 `rx_ns` 都以 30.6–30.8 ppm 线性增长,24.7 h 落后 2.7 s,而 `rx_ns` − 交易所时间一直约 10 ms。策略的 5 分钟窗口与"结束前 60 s 停止报价"按引擎 UTC 判断,长运行时整体偏移数秒。

## Decision

新增 Input `UtcAnchor`(0x050B,32 B):hft-core housekeeping 在 `Synced` 校准节拍上每 `clock.anchor_interval_ms`(默认 10 s)把该节拍的校准对与 `DriftTracker::hz_milli`(60 个校准对的最小二乘频率)经 hk 环送出,live driver 原样写入 journal。sequencer 处理它时把本运行的 UTC 线换成"目标线":落后则在该记录处前跳到目标,超前则保持当前值并以 500 ppm 减速直到追上,任何时候不回跳。线是快照节 32,只在本运行应用过重锚后写出;新运行的 `EngineClock` 清除它。规则与常数归 [hft-sequencer 引擎 UTC](../../../../crates/hft-sequencer/README.md#引擎-utc)。

没有 UTC 线时 `Now` 的锚点与原来逐位相同(段头校准对),所以未重锚的 journal、DST 与既有夹具的输出不变。有 UTC 线时 SDK 锚点取"当前记录及其 UTC",SDK 的结构与公式不变,策略包零改动。实盘驱动自己的 UTC 判断(整点滚动、`DayRoll`、`FundingSettle`、段头 `created_utc_ns`)改用最近一条 `ClockStatus` 的校准对,它们本来就写成 Input,不影响回放。

## Alternatives considered

**只在整点滚动时刷新段头校准对**:段头本来就进 journal,回放确定,不需要新 kind。但一小时的间隔在 30 ppm 下仍累计约 108 ms,且启动后第一小时的频率误差无从校正;换段时校准对直接替换会使 UTC 回跳。

**把 ClockStatus 当作决策输入**:它已经每秒一条,但它是 Meta,sequencer 不读 Meta 是记录分类的基本约定;改角色等于改语义,应当用新 kind。

**扩展 SDK `EngineClockAnchor`,给它独立的 UTC 频率与减速状态**:语义最直接,但要改 SDK 的公开结构与所有策略包里的构造点,会触发策略与引擎同改的解耦门。改为由 sequencer 逐记录给出锚点,SDK 公式不变。

**直接跳到目标(允许回跳)**:实现最简单,但引擎 UTC 会倒退,窗口判断可能重复触发。减速上限取内核 `adjtime` 的 500 ppm。

**节 25 行升版本带上 UTC 线**:hft-ledger 的冷读者与配置装配严格要求节 25 为 32 B、版本 1,旧版本恢复也会拒绝新行;单独的节 32 对旧读者是可跳过的未知节。

## Consequences

决策 UTC 在测试的模拟一天中(计数器慢 30 ppm、12 h 时再变 1 ppm)首次重锚后与 CLOCK_REALTIME 相差 ≤ 50 µs,首次重锚前(约 3 s)≤ 1 ms;只靠启动校准时一天落后约 2.6 s。代价是每 10 s 一条 32 B 的 Input,以及冷读者若要复现决策 UTC 必须按日志重放重锚(段头校准对仍是启动校准)。advisor 从快照起步时,上一条 AdvisorTick 若与快照之间隔着重锚,其 UTC 按恢复出的 UTC 线换算,可能与连续运行相差一次重锚的步长(微秒级)。

warm 线程(gw 签名时间戳、REST 截止、ctl)仍用启动时按值传入的 `TscUtc`,没有经 `SeqCell<TscUtc>` 更新,同样按 30 ppm 漂移;它们不进决策回放,单独修正。
