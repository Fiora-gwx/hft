# 06 · 数据平面、存储、确定性回放与可观测性(2026-09-29)

> 范围:热路径事件传输、事件日志(journal)格式与持久化、研究存储栈、确定性回放/回测方法论、近零开销可观测性、风控与控制面、Python 角色。
> 与 04 的分工:04 已在库级别评估 disruptor-rs / rtrb / iceoryx2 / Aeron / SBE / rkyv / hftbacktest;本文不重复,只引用其结论,专注**端到端设计与量化取舍**。
> 现状痛点(v0.3.x 原型,`E:\trading\Trading`):`crates/wal` 以 512 MiB(`SegmentConfig::max_bytes`)按大小滚动、写线程每 1 s `sync_data()`;滚动无快照导致 84% 订单簿段被隔离(v0.3.5 用 rotate 路径绕过);采集(collector)与交易引擎(cdex-arb Python)分进程;Parquet 按 `category/exchange/market/symbol/date/hour` 分区、row group 131 072 行 / 64 MiB;时间戳为 `ClockStamp{utc_ns, monotonic_ns, clock_synchronized}` + `exchange_event_ts_ns: Option<i64>`;遥测用 `metrics` + `metrics-exporter-prometheus`。
> 标记约定:**[实测]** 公开基准或我们自己测的数字;**[自报]** 厂商/作者数字;**[估算]** 本文推算;**[不确定]** 凭记忆或需复测。

## 0. 结论摘要(Top 10)

1. **Journal 是唯一事实来源(single source of truth)。** 每个入站报文(原始字节 + 硬件/内核/用户态时间戳)、每个策略决策、每个出站消息与回执,按到达顺序追加进一个 mmap 顺序日志;Parquet、S3、监控、Telegram、研究 Notebook 全部是**异步 tail 读者**,任何读者掉队或崩溃都不能影响写者。这是 LMAX(6M orders/s 单线程 + journaler 并行)、Chronicle Queue("record everything store",100+ TB 可从任意点回放)与 Databento(DBN 记录 `ts_event/ts_recv/ts_in_delta`)的共同做法。
2. **同一进程、thread-per-core、SPSC ring 做热路径;进程边界只放冷读者。** 同机跨进程 IPC 最好情况(iceoryx2 ~240 ns、Chronicle 99% 0.78 µs)也比进程内 SPSC(Disruptor 论文 <50 ns 均值,rigtorp SPSC 1.1 亿 ops/s)慢一个量级,且引入调度/唤醒尾部。采集与交易引擎**必须合并进一个进程**;多进程只用于 uploader / Python / 运维 bot,通过 mmap journal + seqlock 头做"一热写多冷读"。
3. **记录格式 = 32 B 定长头(zerocopy)+ SBE 正文 + 原始载荷。** SBE 的六条设计原则(copy-free、原生类型、零分配、流式访问、字对齐、通过可选扩展字段向后兼容)正对我们的需求;原始交易所字节必须保留(解析 bug 是数据丢失的第一原因),规范化事件用于快速回放。
4. **时间戳存四元组 + 序号:**exchange event `E`、exchange tx `T`/`cts`、本地接收(hw/kernel ts 或用户态 CLOCK_REALTIME ns)、本地单调计数(TSC/CNTVCT 原始 tick),每段写一条校准记录。i64 ns 是研究友好格式;原始 tick 是最便宜的热路径读取,两者在 processor 阶段合并,不在热路径做换算。
5. **段滚动改为"时间对齐 + 快照先行"(keyframe 模型)。** 按 UTC 整点滚动(与 Parquet 分区 1:1),大小只做安全上限;新段第一批记录是各 instrument 的全量 L2 快照 + 校准记录;段头记录前一段 id 与每 instrument 最后序号;此外每 60 s / N 条增量插一次周期快照,任何读者从任一段开头都能自举。这直接消灭 84% 隔离问题。
6. **fsync 不在热线程;O_DIRECT 不用。** 写者只做 memcpy 进预 fault 的 mmap;冷线程每 100–1000 ms 或每 8–16 MiB `fdatasync`;崩溃模型是"进程死 → page cache 全保留;主机死 → 丢最后 <1 s",这是 Chronicle Queue 的同款取舍(OSS 版无同步模式,依赖 page cache)。用 `fallocate` 预分配 + 每记录 CRC32C 抗撕裂写(已有)。
7. **研究存储:Parquet(zstd)+ S3 + DuckDB/Polars 是 2026 小团队默认;ClickHouse 是第二阶段。** ArcticDB 生产使用需付费许可(BSL 1.1),kdb+ 成本高,QuestDB/Timescale 对全量 L2 无优势。全量 L2 数据量:Tardis 全部交易所约 1 TB/天原始 JSON;我们实测 4 个 BTC 品种 ≈ 9 MB/min 原始 WAL(≈ 13 GB/天);50–200 个 symbol 单交易所估 5–20 GB/天(zstd 后)[估算]。
8. **回放 = 同一 sequencer 二进制吃 journal,决策必须逐字节一致;成交模拟才是可调部分。** 借 hftbacktest 的双时间轴(`EXCH_EVENT`/`LOCAL_EVENT` 双序)、三段延迟(feed / order entry / order response)、`IntpOrderLatency`(用定期发不可成交订单采集的真实延迟序列)、`ProbQueueModel` 队列位置、`PartialFillExchange`。校准闭环是"影子模式 → 用同日 journal 回放 → 比对决策哈希与成交差异 → 调队列模型"。
9. **可观测性:事件自带阶段时间戳,HdrHistogram(3–6 ns/record)在热线程记录、冷线程 1 s 交换导出,Prometheus 只 scrape 冷线程。** `tracing` 宏、`format!`、`log` 不进热路径;想要外部追踪工具优先 LTTng-UST(用户态 lock-free per-CPU ring,单点亚微秒级 [不确定]),USDT/eBPF uprobe 只在排障时挂(int3 陷入约 1–3 µs [不确定])。长期指标与延迟分解落 ClickHouse(OTel/Vector 摄入,官方声称日志/trace 平均压缩 14×),Grafana 统一展示。
10. **风控与控制面:预交易检查是同线程 O(1) 表查(<200 ns [估算]),kill switch 依赖交易所侧 dead-man's switch + 本地 cancel-all 双保险;控制命令走 SPSC 控制 ring 进 sequencer 并同样写 journal,回放包含运维动作。** 交易所差异大:Binance USDⓈ-M `countdownCancelAll`(毫秒倒计时、心跳式续期)、OKX `cancel-all-after`(10–120 s)、Deribit `enable_cancel_on_disconnect`;**Bybit DCP 仅机构客户可开**(默认 10 s、窗口 3–300 s、需订阅 `dcp` 私有主题);Binance 现货与 Bitget 无原生 dead-man's switch [不确定],只能靠本地看门狗 + REST cancel-all。

---

## 1. 事件传输(in-process / inter-process)

### 1.1 三种拓扑与量级

| 拓扑 | 单跳延迟(同机) | 尾部行为 | 读者数量的影响 | 证据 |
|---|---|---|---|---|
| 进程内 SPSC ring,双方绑核忙等 | **几十 ns**(同 socket) | 无系统调用、无唤醒;尾部来自 cache miss / 中断 | 每个读者一个 ring(SPSC)或 SPMC 序号广播 | Disruptor 论文:三级流水线均值比队列低 3 个量级,"25M msg/s、延迟 <50 ns"(https://lmax-exchange.github.io/disruptor/disruptor.html)[自报];rigtorp SPSC:朴素实现 5.5M ops/s → cache line 对齐 + 缓存索引后 112M ops/s(https://rigtorp.se/ringbuffer/)[实测] |
| 跨进程共享内存(iceoryx2 / Aeron IPC / Chronicle Queue) | **100 ns – 1 µs** | 依赖读者是否忙等;一旦 park/唤醒进入 µs–十 µs | iceoryx2 每订阅者独立通道,发布延迟随订阅者数线性增长(1.2 → 21.7 µs @ n 个订阅者,见 04);Chronicle 多 tailer 各自读 mmap,不影响 appender | Chronicle Queue README:40 B 消息 JVM↔JVM 99% 0.78 µs / 99.9% 1.2 µs(10M 与 60M events/min),100M/min 时 99% 1.2 µs;96 B 消息约 5M msg/s(i7-4790);跨机复制 99% 20–176 µs、99.9% 0.7–5.4 ms(https://github.com/OpenHFT/Chronicle-Queue)[自报];iceoryx2 64 KB 载荷 240 ns vs UDS 23 µs vs MQueue 14 µs(04 §2.2)[自报] |
| 内核 IPC(UDS / pipe / TCP loopback)+ epoll | **10–30 µs** | futex/epoll 唤醒 + 调度,尾部 50–200 µs | 无所谓 | iceoryx2 对比数据;我们自己在 c7g.large 上测到 `sleep(0)` p99 155 µs、100 µs 定时器 p50 超时 54 µs / p99 193 µs(memory: low-latency-research)[实测] |

关键结论:**同机 IPC 的最好情况也比进程内 ring 慢 5–20×,而它的坏情况(读者被调度出去、写者要通知)是热路径不可接受的**。原型把 collector 与交易引擎分成两个进程、中间还隔着 Python,是 tick-to-trade 3–5 ms 的结构性原因之一。

### 1.2 "一热写、多冷读"的实现方式

需求:热线程以 memcpy 的代价把每条事件追加到日志;任意数量冷读者(uploader、Python、Telegram bot、Prometheus 导出器)以任意速度 tail;冷读者的崩溃、慢速、崩掉页缓存都不影响热线程。

设计要素(综合 Chronicle Queue、Aeron log buffer、rigtorp seqlock):

1. **单写者顺序追加到 mmap 段文件**(Chronicle:每个 roll cycle 一个 `.cq4`;Aeron:term buffer 最大 1 GB,发布者领先订阅者的距离以一个 term 为度量)。写者只写、不读、不锁;记录 = 头 + 正文,头里的 `len` 字段最后写(release 序),读者看到非零 `len` 才算记录可见。
2. **段头一个 seqlock 保护的 `committed_bytes`/`last_seq`**:写者每 N 条或每 100 µs 更新一次(不是每条,避免与读者共享 cache line 抖动);读者 `acquire` 读它决定能读到哪。rigtorp 的 seqlock 是"写者永不阻塞、读者检测撕裂后重试"(https://rigtorp.se/seqlock/),正适合广播小状态。
3. **读者只读 mmap(`PROT_READ`)**,Chronicle v5 明确把 tailer 做成 read-only 以简化与优化。Python 读者用 `numpy.memmap` / Arrow 按固定布局解析。
4. **预 fault 与 hugepages**:Chronicle 的 pre-toucher 提前把页拉进 page cache,避免写者遇到 page fault(µs 级);rigtorp 指出 ring 用 hugepages 可减少 TLB miss。我们:`MAP_POPULATE` + `madvise(MADV_HUGEPAGE)`(或 `/dev/hugepages` 上的 ring),段文件用 `fallocate` 预分配。
5. **写者与读者不共享任何频繁写的 cache line**:读者的进度只存在读者自己的内存里(journal 不需要背压——磁盘是无限队列;ring 才需要背压)。
6. **通知**:冷读者轮询(每 100 µs–1 ms)即可;不要让热线程做 futex wake。

与 LMAX 的对照:LMAX 让 journaler / replicator / unmarshaller 三个消费者**并行**消费同一输入 ring,业务逻辑处理器等三者都完成后才处理(Fowler, https://martinfowler.com/articles/lmax.html)。这是"先落盘再处理"的强持久性语义。对我们(无需集群复制、可接受主机崩溃丢 <1 s)更简单的做法是:**feed handler 线程解析后先 memcpy 进 journal(几十 ns),再 push 进 ring**;journal 写入即视为"已接收"。

kdb+tick 是同一模式的第三个独立佐证(https://code.kx.com/q/architecture/):tickerplant "捕获初始数据流、写入日志文件、再发布给订阅者",要求"轻量、不保存数据、独占核心";RDB 重启时"回放日志文件回到当前状态",启动时向 TP 索要"日志位置与应读行数";最佳实践明说"日志放在快速本地磁盘以减少发布延迟与 I/O 等待"。二十年来三套体系(LMAX、Chronicle、kdb+)收敛到同一个形状,说明这不是风格问题。

### 1.3 推荐拓扑

```
[ws conn A] feed-handler core 1 ─┐ (journal append + SPSC)
[ws conn B] feed-handler core 2 ─┼──► sequencer/strategy core 3 ──SPSC──► order-gateway core 4 (ws-api / REST / BSC tx)
[ws conn C] feed-handler core 5 ─┘        ▲ control SPSC                      │ journal append (outbound + ack)
                                          │                                   ▼
  control/ops process (tokio) ────────────┘                 mmap journal segments  ◄── tail: uploader, processor, python, tg-bot
```

- 每个热线程一个核(`isolcpus` + `SCHED_FIFO`,见 04 §2.5),线程数 = 交易所连接组数 + 1(sequencer)+ 1–2(gateway)。c7g.large(2 vCPU)撑不起;至少 8 vCPU 的实例,热路径独占 4–6 个物理核(硬件选型见 05)。
- **sequencer 单线程**:持有所有订单簿、仓位、风控状态,消费 feed ring 与 control ring,输出决策事件与订单意图;它是唯一有权改变状态的实体,因此回放只需要重放它的输入。
- **order gateway** 把订单意图编码发送,记录 `t_send`,并把交易所回执作为事件写回 journal 与 sequencer。

---

## 2. 事件格式与日志

### 2.1 记录布局

```
┌─ RecordHeader (32 B, #[repr(C)], zerocopy) ──────────────────────────────┐
│ u32 len_total │ u32 crc32c(body+payload) │ u16 kind │ u16 schema_ver     │
│ u8 venue │ u8 conn │ u16 flags │ u64 seq(journal 全局单调)                │
│ i64 ts_recv_ns(CLOCK_REALTIME) │ u64 ts_mono_raw(TSC/CNTVCT 原始 tick)     │
├─ SBE body(templateId 由 kind 决定;定长块 + 可选扩展)──────────────────┤
│ 例 BookDelta: instrument_id u32, exch_event_ts i64, exch_tx_ts i64,      │
│   first_update_id u64, last_update_id u64, prev_update_id u64,           │
│   n_bids u16, n_asks u16, repeating group {px i64 ticks, qty i64 lots}   │
├─ raw payload(可选;原始 ws 帧字节;flags 标记是否 zstd-dict 压缩)─────┤
└──────────────────────────────────────────────────────────────────────────┘
```

- **为何两份(规范化 + 原始)**:规范化事件让回放不必再跑 JSON 解析(sonic-rs 解析一条 depth 仍需 µs 级),原始字节保证解析 bug 可事后修复(原型 `inspect_v2_dataset` 里就统计过 `missing_event_timestamps`)。原始载荷在热路径不压缩,uploader 冷线程用**训练过的 zstd 字典**压缩(zstd README:字典对"前几 KB"的小样本收益最大,一类数据一个字典)。
- **hftbacktest 的 8 字段记录**(`ev u64, exch_ts i64, local_ts i64, px f64, qty f64, order_id u64, ival i64, fval f64`,https://hftbacktest.readthedocs.io/en/latest/data.html)证明"每条事件一行、双时间戳、flags 编码方向/类型"足以驱动高保真回测;我们的 SBE body 应能无损投影到这个形状。
- **Databento DBN**(`rust/dbn/src/record.rs`,https://github.com/databento/dbn)是商业级"每条记录一个 `#[repr(C)]` 结构体"的范本:16 B `RecordHeader{length: u8(以 32-bit 字计), rtype: u8, publisher_id: u16, instrument_id: u32, ts_event: u64(撮合引擎接收时刻,UNIX ns)}`;MBO 正文 `order_id: u64, price: i64(1e-9 定点), size: u32, flags, channel_id: u8, action: c_char, side: c_char, ts_recv: u64(采集服务器接收 ns), ts_in_delta: i32(撮合引擎发送时刻相对 ts_recv 的 ns 差), sequence: u32`;文件外层 zstd。与我们的头几乎一一对应:`ts_event`↔`exch_tx_ts`、`ts_recv`↔`ts_recv_ns`、`ts_in_delta` 是把"交易所发送时间"压成 4 B 的技巧,值得照抄。

### 2.2 时间戳与序号

| 字段 | 来源 | 用途 | 注意 |
|---|---|---|---|
| `exch_event_ts` | Binance `E`、Bybit `ts`、OKX `ts` | 交易所"发出"时间 | Tardis 提醒:部分交易所给的是发送时间而非撮合时间(https://hftbacktest.readthedocs.io/en/latest/tutorials/Data%20Preparation.html) |
| `exch_tx_ts` | Binance futures `T`、Bybit `cts`("matching engine 生成时间,可与成交 `T` 关联") | 撮合时间;跨渠道对齐 | 现货 Binance depth 无 `T`;Binance 可用 `timeUnit=MICROSECOND` 拿 µs(https://github.com/binance/binance-spot-api-docs/blob/master/web-socket-streams.md) |
| `ts_recv_ns` | `SO_TIMESTAMPING` 硬件/内核 RX ts,或用户态 `clock_gettime(REALTIME)` | feed 延迟、跨所到达顺序 | ENA 2025-06 起支持 ns 级 RX 硬件时间戳(04 §2.5);TLS 之上无法拿到"消息级"硬件戳,只能拿到 TCP 段级,需按字节偏移对齐(内核文档 `SOF_TIMESTAMPING_OPT_ID_TCP`,https://docs.kernel.org/networking/timestamping.html) |
| `ts_mono_raw` | `rdtsc` / `cntvct_el0` | 阶段间差值(唯一可信的延迟度量) | 每段写一条 `ClockCalib{realtime, mono_raw, freq_hz, chrony_offset, chrony_rms}`;Graviton 的 CNTVCT 频率固定但需读 `cntfrq_el0` [不确定] |
| 序号 | Binance `U/u/pu`、Bybit `u/seq`、OKX `seqId/prevSeqId` | 缺口检测、去重 | Tardis 采集时用 `pu/u` 校验,发现缺口就重连;我们同样在 feed handler 内做 |

**i64 ns vs TSC tick**:热路径每条事件只做一次 `clock_gettime`(vDSO,~20–30 ns)+ 一次计数器读(~10–20 ns);tick→ns 的换算和 chrony 偏移修正放在 processor。研究侧统一 i64 ns UTC(Parquet `TIMESTAMP(NANOS)`)。

### 2.3 Schema 演进(SBE)

- 消息头 `blockLength / templateId / schemaId / version`;新增字段只能**追加在定长块末尾**并标 `sinceVersion`,老解码器按 `blockLength` 跳过多余字节,新解码器读旧数据时按 `actingVersion` 判断字段不存在返回 null。改变已有字段的类型/偏移或新增必填字段 → 新 `templateId`(SBE 设计原则 "Backwards Compatibility",https://github.com/aeron-io/simple-binary-encoding/wiki/Design-Principles)。
- 每个 journal 段头写入 schema 文件的哈希;processor 拿哈希找对应的解码器版本 → 任何历史段都能解。
- 生成器:官方 `sbe-tool`(Java)输出 100% safe Rust;或 `ergo-sbe`(纯 Rust,见 04)。

### 2.4 压缩(数字)

zstd 1.5.7 README(i7-9700K 4.9 GHz,Silesia,单线程,https://github.com/facebook/zstd)与 lz4 README(https://github.com/lz4/lz4)[自报]:

| 编解码 | 压缩比 | 压缩 MB/s | 解压 MB/s |
|---|---|---|---|
| zstd -1 | 2.896 | 510 | 1550 |
| zstd --fast=1 | 2.439 | 545 | 1850 |
| zstd --fast=4 | 2.146 | 665 | 2050 |
| lz4 1.10 默认 | 2.101 | 675 (lz4 README: 780) | 3850 (4970) |
| lz4 HC -9 | 2.721 | 41 | 4900 |
| snappy 1.2.1 | 2.089 | 520 | 1500 |
| zlib -1 | 2.743 | 105 | 390 |

结论:热路径**不压缩**(memcpy ≫ 任何编解码);冷路径 journal 归档用 zstd -3…-6 + 字典(JSON 载荷重复度极高,实际比率通常 8–15× [估算,原型 25 GB zstd 对应约 250 个 500 MiB 段 ≈ 5×,但那是含二进制帧的混合数据]);Parquet 用 zstd(level 3)——列式已去掉大部分冗余,lz4 只在需要 >2 GB/s 解压的本地扫描场景才值得。

### 2.5 段滚动与 snapshot-at-roll(修复 84% 隔离)

原型的失败链:`SegmentWriter::append` 在 500 MiB 返回 `RollReason::Size` → 写线程静默换段 → 新段以 `action:update` 开头、无快照 → processor `bind_checkpoint` 找不到任何 book → 隔离。v0.3.5 的补救是走 `collectorctl rotate` 那条"审计 → checkpoint → invalidate → rotate → 重连"的重路径,代价是每次滚动都重连交易所。

新设计(视频编码的 keyframe 思路):

1. **时间对齐滚动**:按 UTC 整点(或 15 min)滚动,大小(如 2 GiB)只做安全阀。段名 = `{exchange}/{yyyymmdd}/{hh}/{seq}.jrn`,与 Parquet 分区一一对应,研究侧"某小时的数据"就是一个段。
2. **滚动是 sequencer 的一个事件**,不是写者的私事:写者到点发 `RollRequested` 进 control ring;sequencer(持有全部订单簿)在处理完当前事件后,向 journal 写 `SegmentEnd{next_id}`,打开新段,写 `SegmentBegin{prev_id, schema_hash, ClockCalib}`,然后**逐 instrument 写 `BookSnapshot`(从本地镜像生成,不打 REST、不重连)**,再继续写增量。因为在单线程内完成,快照与后续增量之间不存在缝隙。
3. **周期快照(I-frame)**:每 instrument 每 60 s 或每 5 000 条增量写一次本地镜像快照;读者可从段内任意快照开始,恢复时间上界 = 快照间隔。代价:BTCUSDT 1000 档 ×2 侧 × 16 B ≈ 32 KB/次,每分钟一次即 46 MB/天/instrument,可接受;长尾 symbol 档位更少。
4. **段头链**:`prev_segment_id` + 每 instrument `last_update_id`,processor 能判定"段 N+1 可用段 N 尾部做 seed"并自动多段 seed(原型的 `replay_to_parquet_with_seed` 只接受单个 seed,是遗留的 ~250 个隔离包无法恢复的直接原因)。
5. **交易所侧重连**(24 h 强断、`serverShutdown`、序号缺口)与滚动解耦:重连产生 `Connect` + REST 快照事件,写在当前段中间,不触发滚动。

### 2.6 fsync / O_DIRECT 策略

- **崩溃模型**:进程崩溃 → 已 memcpy 进 page cache 的数据全部保留(内核负责刷盘);主机断电/内核崩溃 → 丢最后一次 `fdatasync` 之后的数据。对行情 journal,丢 <1 s 可接受(下次连接会重新拿快照);对订单/成交事件,交易所是权威(对账循环兜底,见 §6.3)。
- Chronicle Queue OSS 版**没有同步模式**,完全依赖 page cache,并明确"99% 延迟取决于 OS 与磁盘子系统"(FAQ, https://github.com/OpenHFT/Chronicle-Queue/blob/ea/docs/FAQ.adoc)。
- PostgreSQL 的 WAL 可靠性文档提醒:控制器/SSD 的写回缓存可能在断电时丢数据、"很多 SSD 默认不响应 cache flush",并用 full-page writes 抗撕裂写(https://www.postgresql.org/docs/current/wal-reliability.html)。我们的对应物是**每记录 CRC32C + 恢复时截断到最后一条完整记录**(`recovery.rs` 已有雏形)。
- **推荐**:热线程只 memcpy;冷"syncer"线程每 250 ms 或每 16 MiB `msync(MS_ASYNC)`/`fdatasync`;段打开时 `fallocate` 全长(避免追加时的元数据更新与碎片);封段时 `fsync` 文件 + 目录。原型把 `sync_data()` 放在写线程里每秒一次——AWS 文档只对 io2 Block Express 承诺"16 KiB I/O 平均延迟 <500 µs"(https://docs.aws.amazon.com/ebs/latest/userguide/ebs-volume-types.html),gp3 通常是个位数毫秒 [不确定],无论哪种都不该出现在写线程里;实例存储 NVMe 更低但重启即失,只适合做 journal 的第一落点 + 异步复制到 EBS/S3。
- **O_DIRECT 不用**:它要求对齐缓冲、绕过 page cache 使 tailer 读者失去零拷贝读、只在写入带宽 GB/s 级时有意义;我们的写入是 ~0.1–1 MB/s 级。

---

## 3. 存储与研究栈

### 3.1 数据量估算

| 锚点 | 数字 | 来源 |
|---|---|---|
| Tardis 全部支持交易所(~40 家、全 symbol、全频道)原始 JSON | **≈ 1 TB/天** | Tardis FAQ(https://docs.tardis.dev/faq)[自报] |
| 我们原型:4 个 BTC 品种(现货+永续,book_ticker + depth + trades)原始 WAL | **≈ 9 MB/min ≈ 13 GB/天** | memory: btc-usdc-24h-capture [实测] |
| 我们原型:BTCUSDC 永续 bookTicker 24 h | 12.5 M 行;18 条 BBO 流 48 h 共 1.42 B 行(98% 为不改价的 size 变化) | memory [实测] |
| 原型隔离包:~250 个 ≈ 500 MiB 段 → 25 GB zstd | 压缩比 ≈ 5×(混合二进制帧 + JSON) | memory: l2-size-roll-quarantine [实测] |
| Bybit 推送节奏 | L1 10 ms、L50 20 ms、L200 100 ms、L1000 200 ms(线性/反向/现货) | https://bybit-exchange.github.io/docs/v5/websocket/public/orderbook |
| Binance | 现货 depth 100 ms / 1000 ms(快照最多 5000 档);期货 `depth@0ms`(Tardis 自 2020-01-07 用之,"实时、动态调整")+ 1000 档 REST 快照 | Binance 文档;https://docs.tardis.dev/historical-data-details/binance-futures |

按此推算 [估算]:头部 symbol(BTC/ETH 永续)原始 L2 增量 2–4 GB/天,zstd 后 0.3–0.6 GB;第 20–200 名的 symbol 递减一个量级。**单交易所 50 个 symbol:原始 30–60 GB/天、zstd 5–10 GB/天;200 个 symbol:原始 60–120 GB/天、zstd 10–20 GB/天。**三个交易所一年 ≈ 10–20 TB(zstd),S3 标准存储 ≈ $250–500/月,Parquet 规范化数据再加 30–50%。S3 单区域 PUT/GET 与 DuckDB 扫描费用可忽略,跨太平洋下载是瓶颈(原型实测单流 ~60 KB/s,要在服务器侧聚合后只拉小结果)。

### 3.2 方案对比

| 方案 | 许可/成本 | 写入 | 典型查询延迟 | 与 Python 的距离 | 2026 定位 |
|---|---|---|---|---|---|
| **Parquet(zstd)+ S3 + DuckDB / Polars** | 全开源;S3 存储费 | 由 processor 批量写,行组 100k–1M 行 | 单 symbol-day 全扫 <1 s(本地 NVMe),S3 上受带宽限制;谓词/投影下推(Polars `scan_parquet` 对云端也下推) | 零距离(Arrow) | **默认**。小团队/单机研究的事实标准;原型已就位 |
| ClickHouse | Apache-2.0;自托管 1 台 8 vCPU 即可 | 批量 insert,时序 codec(Delta/DoubleDelta/Gorilla)+ zstd;官方称可观测数据平均压缩 14×(https://clickhouse.com/docs/use-cases/observability) | 亚秒交互式聚合、ASOF JOIN、多用户 | 好(clickhouse-connect → Arrow) | **第二阶段**:在线多人查询、可观测性存储、跨天聚合;Parquet 作为冷层可直接 `s3()` 表函数查 |
| QuestDB | Apache-2.0 / 企业版 | ILP 高速摄入,SAMPLE BY / ASOF | 快,但生态窄 | 中 | 不引入(功能是 ClickHouse 子集) |
| TimescaleDB | TSL 许可 | 行存 + 压缩 chunk | 对十亿行 tick 聚合明显慢于列式 | 好(SQLAlchemy) | 不适合全量 L2 |
| kdb+ / KDB-X | 商业(个人版免费限制多) | tickerplant/RDB/HDB 架构是行业模板 | 极快 | PyKX | 借架构思路,不买 |
| ArcticDB | **BSL 1.1:生产使用需付费许可**,各版本 ~2 年后转 Apache-2.0(https://github.com/man-group/ArcticDB) | DataFrame 版本化写入 S3/LMDB,声称单消费者 1 亿行/s | 快 | 原生 pandas | 注意许可;我们不需要 bitemporal |
| Tardis 式布局(参考) | — | `datasets.tardis.dev/v1/{exchange}/{channel}/{yyyy}/{mm}/{dd}/{SYMBOL}.csv.gz` + 原始 JSON 按分钟切片 API;`local_timestamp` µs;同毫秒内按行序 | — | — | 布局值得照抄:**exchange/channel/date/symbol**,原始与规范化并存 |

### 3.3 分区、文件大小、行组

- 分区键保持原型的 `category/exchange/market/symbol/date/hour`(Hive 风格,DuckDB/Polars 都能剪枝),但**hour 级分区只对头部 symbol 有意义**;长尾 symbol 按 day 合并(小文件是 S3+Parquet 的头号性能杀手,目标单文件 100 MB–1 GB)。
- 行组 100k–1M 行(原型 131 072 行 / 64 MiB 合理)。DuckDB 官方微基准(https://duckdb.org/docs/current/guides/performance/file_formats.html):行组 <5 000 行慢 5–10×,5 000–20 000 行慢 1.5–2.5×,>100 000 行后差异 ≈10%;DuckDB 只能按行组并行,"总行组数 ≥ 线程数";`ROW_GROUP_SIZE` 默认 122 880;**单文件 100 MB–10 GB 为理想区间**;避免 gzip(解压重),用 snappy/lz4/zstd。
- 写前按最常过滤的列(时间、symbol)排序,让每个行组的 min/max 统计紧凑不重叠,谓词下推才真正跳过行组(https://duckdb.org/docs/current/data/parquet/tips.html);`hive_partitioning=true` 读目录键,S3 上用 `ListObjectsV2` glob + HTTP range 请求,开 `enable_http_metadata_cache`。
- 一张"catalog"表(instrument、tick size、lot size、`catalog_version`、schema hash、段 id → 文件映射)是研究可复现的关键;原型的 `catalog_units.jsonl` 正确,升级为 Parquet/DuckDB 表即可。
- 三层数据:**L0 原始 journal 段(zstd,永久)→ L1 规范化事件 Parquet(book deltas / snapshots / trades / bbo / funding / 我们的订单与成交)→ L2 派生(1 s 采样 BBO、特征、回测结果)**。L1、L2 可随时从 L0 重建。

### 3.4 典型查询与工具选择

| 查询 | 工具 |
|---|---|
| 某 symbol-day 重建全量订单簿并按事件时间迭代 | Rust reader(PyO3 暴露)→ Arrow 流;Python 侧 Polars 只做统计 |
| 跨所 lead-lag、基差、到达间隔分布 | DuckDB / Polars(ASOF join、窗口)|
| 我们的 tick-to-trade 分解、每阶段 p99 按小时 | ClickHouse(或先 DuckDB 读 journal 派生的 latency Parquet)|
| 事故复盘("那 30 秒发生了什么") | 直接读 journal 段 + `trading-replay records` 式 CLI |

---

## 4. 确定性回放与回测

### 4.1 事件溯源引擎:实盘与回放同一个二进制

规则(LMAX/Chronicle 一脉,04 §5.2 亦有):
1. sequencer 的**全部输入**只来自 ring(feed、gateway 回执、control 命令、定时器 tick),全部输入都先写 journal;
2. sequencer **不读墙钟**——时间来自事件里的时间戳与显式 `TimerFired` 事件;RNG 显式播种并把种子写进 journal;
3. 输出(订单意图、日志事件)也是事件,写 journal;
4. **回放 = 用 journal 段替换 ring 的生产者**,sequencer 代码零改动,输出与实盘逐字节一致(哈希比对);
5. 回测 = 回放 + 用"交易所模拟器"替换 order gateway。

Fowler 描述 LMAX 用一天的 journal 在开发环境重放,能复现"只有百万事件按特定顺序才出现的 bug",Chronicle 也把"用真实时序加速回放复现 bug"列为核心用例。这是我们摆脱"补丁脚本 + 无法复现事故"状态的唯一路径。

### 4.2 双时间轴与 look-ahead

hftbacktest 的核心设计:每条事件带 `exch_ts` 与 `local_ts`,并用 `EXCH_EVENT` / `LOCAL_EVENT` 两个 flag 标记它在两条时间轴上的有效性——**交易所侧模拟(成交判定)按 `exch_ts` 顺序推进,本地策略只能看到 `local_ts` ≤ 当前时间的事件**;要求"feed 延迟必须为正"(https://hftbacktest.readthedocs.io/en/latest/data.html)。其数据准备文档展示了同一条 depth 消息在两条轴上被拆成两次出现。

我们的 journal 天然按 `ts_recv` 排序,交易所时间戳是字段——这是正确的原始形态;回放器在加载时派生双序。**看到未来的三种典型方式**:按 `E` 排序后让策略读到尚未到达本地的更新;用 REST 快照的 `lastUpdateId` 时间当作接收时间;跨交易所用各自 `E` 对齐(时钟不同步 + 传播路径不同,我们实测 Binance→OKX→Bitget 到达顺序差 10–30 ms,几乎全部"套利机会"是传播延迟)。

### 4.3 延迟模型

hftbacktest 定义三段延迟:feed latency、order entry latency、order response latency(https://hftbacktest.readthedocs.io/en/latest/latency_models.html)。模型:`ConstantLatency`;`IntpOrderLatency`——用真实 `(req_ts, exch_ts, resp_ts)` 序列插值,"最准确";`FeedLatency`——无订单数据时按 feed 延迟乘系数 + 偏移生成。**真实订单延迟数据的采集方法:定期提交无法成交的订单并撤销**(Order Latency Data 教程)。

我们的优势:journal 里每笔真实订单都有 `t_send`(用户态/TX 软件时间戳)、交易所 `T`/`updateTime`、`t_ack_recv`;影子模式下也可以以 1 次/秒的频率发深度外限价单探测。回放器从 journal 自动生成延迟序列,按小时/交易所/端点(ws-api vs REST vs BSC)分桶——**不用常数模型**。03 已给出 BSC 腿的延迟结构,CEX-DEX 回测必须同时模拟"上链确认 ≠ 成功"(03 §2.7)。

### 4.4 队列位置、部分成交、费用、资金费

- 队列位置(仅 L2 数据时必须建模):`RiskAverseQueueModel`(最保守:撤单只发生在队尾,只有成交推进位置)与 `ProbQueueModel`(按当前位置以幂/对数函数分配前后撤单概率;来源 rigtorp 2013 与 quant.SE 讨论,https://hftbacktest.readthedocs.io/en/latest/order_fill.html)。**选哪一个只能靠与实盘成交对比来定**(Probability Queue Position Models 教程原话)。
- 成交:`NoPartialFillExchange`(默认,吃单全成)vs `PartialFillExchange`(按队列位置与成交量部分成交);两者都**不建模市场冲击**,因此只对小单成立——对我们的跨所套利(单笔 $2–$100 毛利、簿薄一侧 $4.6k)尤其要用 PartialFill + 以实际簿深截断。04 已指出 hftbacktest 有 PartialFill + 概率队列下部分成交不入账的 bug(PR #323)需先合入。
- 费用:按账户实际 tier 的 maker/taker bp 表;我们的分析显示 3 bp vs 6 bp 往返费决定策略是否存在,费用必须是回放参数而非常数。
- 资金费:在结算时间戳事件上按持仓计提(原型 `funding_rate_v3` 有 `next_settlement_ts_ms`);永续 vs 现货腿的资金费方向是 24h 分析中的主要 P&L 项之一。

### 4.5 影子模式与交叉验证

1. **Shadow mode**:实盘进程完整运行(feed → 决策 → 风控),gateway 处于 `dry_run`,决策照常写 journal;
2. 当日结束后用同一 journal 回放,断言**决策序列哈希相等**(证明确定性);
3. 用回测模拟器对同一决策序列生成模拟成交,与"探测单"的真实成交/排队结果比对,拟合队列模型参数与延迟分布;
4. 小资金实盘后,继续比对"回测预测的成交率 / 滑点 / P&L"与真实。hftbacktest 的 "Debugging Backtesting and Live Discrepancies"(https://hftbacktest.readthedocs.io/en/latest/debugging_backtesting_and_live_discrepancies.html)给出的流程:把实盘与回测的权益曲线、仓位、alpha、订单价格画在同一张图上;若策略实现一致,差异只来自两处——**延迟**(自己采集 feed 延迟;订单延迟靠记录真实订单或"定期在远离中价处下单再撤"的探测)和**队列模型**(在 Power/Log 概率族里调到与实盘吻合);并强调回测无市场冲击,所以"先用小仓位对齐,再逐步放大同时持续比对"。其 README 也说要先验证"回测能复现实盘",再做深入研究。
5. 对比指标:成交率、成交时的簿状态、被动单排队时间分布、逆向选择(成交后 1 s 中价变动)。

### 4.6 陷阱清单

| 陷阱 | 现象 | 对策 |
|---|---|---|
| bookTicker 与 depth 不是同一条流 | bookTicker "实时"推送、depth 100 ms 批量;二者序号空间不同,bookTicker 常早于 depth 数十 ms | 策略里 BBO 用 bookTicker/`books1`,深度用 depth;回放同样分流;不要用 depth 重建的 BBO 去度量 bookTicker 能抓的机会 |
| 快照与增量拼接 | Binance 要求 `U <= lastUpdateId+1 <= u`(期货还看 `pu`);Bybit 用 `u`=1 或新 snapshot 表示重置 | 严格实现,缺口即重连(Tardis 做法);journal 记录 `Connect/Snapshot/Gap` 事件 |
| 用 `E` 代替 `T` | `E` 是发送时间,含交易所内部排队 | 有 `T`/`cts` 时以之为撮合时间;`E - T` 本身是交易所拥塞指标 |
| ms 分辨率并列 | 同毫秒多条事件顺序丢失 | 用到达序为 tie-breaker(Tardis 规则);Binance 开 `timeUnit=MICROSECOND` |
| 重连后重复/乱序 | 24 h 强断、`serverShutdown`、ping 20 s/pong 1 min 超时 | 按 `(conn_id, seq)` 去重,原型的 dedup key 思路保留 |
| 交易所时间戳语义随版本变化 | Tardis 注明部分交易所给"发送"而非"撮合"时间 | 每交易所每字段写文档 + 用探测单校准 |
| 回测吃掉自己的成交量 | 市场回放无冲击 | 限制单笔 ≤ 触及量的一部分,PartialFill |

---

## 5. 可观测性

### 5.1 每事件阶段时间戳

事件结构里预留 8 个 `u64 mono_raw` 槽位:

| 阶段 | 取法 | 成本 |
|---|---|---|
| t0 NIC RX 硬件 | `SO_TIMESTAMPING` `SOF_TIMESTAMPING_RX_HARDWARE|RAW_HARDWARE`(PHC 时钟,需与系统时钟对齐) | 内核在 skb 上带出,`recvmsg` cmsg 读取;TLS 之上对应到 TCP 段而非消息 |
| t1 内核 RX 软件 | `SOF_TIMESTAMPING_RX_SOFTWARE` | 同上 |
| t2 用户态 recv 返回 | 计数器读 | ~10–20 ns |
| t3 解析完成 / t4 决策完成 / t5 `send()` 调用前 | 计数器读 | 同 |
| t6 TX 内核/硬件 | `SOF_TIMESTAMPING_TX_SOFTWARE|TX_SCHED|TX_HARDWARE` + `OPT_ID_TCP`,从 `MSG_ERRQUEUE` 读回 | **在冷线程轮询错误队列**,通过 `OPT_ID` 对回订单 |
| t7 回执到达 | 与 t0–t2 同 | |

内核文档(https://docs.kernel.org/networking/timestamping.html)推荐新应用用 `OPT_ID` 消歧、`OPT_TSONLY` 减少拷贝、`OPT_ID_TCP` 让 TCP 流的 id 按字节计数更稳健。这样每笔订单得到 NIC→内核→用户→决策→发送→线上→回执 的完整分解,我们当前的 "kernel path ~50–100 µs of 3–5 ms" 估算就能变成实测。

### 5.2 直方图与导出

- HdrHistogram:记录 3–6 ns、内存固定、零分配(`new_with_max` 关闭自动扩容避免重分配,https://github.com/HdrHistogram/HdrHistogram_rust)。每热线程每阶段一个,**双缓冲**:热线程写 A,冷线程每 1 s 原子换成 B 并读 A(或用 `hdrhistogram::sync::SyncHistogram` 的 phase 机制)。
- 冷线程把 p50/p90/p99/p99.9/max/count 作为 Prometheus gauge 暴露(`metrics-exporter-prometheus` 已在原型),并把完整压缩直方图(HdrHistogram 有 V2 压缩编码)每分钟写进 journal 或 ClickHouse,离线可任意重聚合。
- **禁止**:热路径上的 `format!`、`log::info!`、`tracing::event!`(带 fmt 订阅者)、`Instant::now()` 以外的系统调用、原子计数器与冷线程共享 cache line(用 per-thread 结构 + 周期性发布)。

### 5.3 追踪工具成本对比

| 工具 | 关闭时 | 打开时(单点) | 适用 |
|---|---|---|---|
| 事件内时间戳槽位(自研) | 0 | 10–20 ns(一次计数器读) | 热路径,常开 |
| HdrHistogram record | — | 3–6 ns | 热路径,常开 |
| `tracing` 宏 + `tracing-subscriber` fmt | 静态 `max_level_*` 编译期裁掉 ≈ 0;运行期 filter 数十 ns | 数百 ns–数 µs(格式化、锁、写 stderr)[不确定] | 控制面/tokio 侧 |
| LTTng-UST tracepoint | 一次 load + 分支(~1 ns) | 约 100–200 ns 到 per-CPU lock-free ring,消费者进程异步落盘 [不确定,凭 EfficiOS 公开说法] | 想用 Trace Compass/babeltrace 生态时的热路径可选项 |
| USDT(`probe!`)+ eBPF/bpftrace | `nop`(0) | 1–3 µs(int3 陷入 + 内核态处理)[不确定];Linux 6.x 的 uprobe 优化在缩小差距 | 只在排障时挂,不常开 |
| perf / uprobes 动态 | 0 | 与 USDT 同量级 | 排障 |

Brendan Gregg 的 uprobe 与 USDT 文章展示了 `nop` + semaphore 的 is-enabled 机制(https://www.brendangregg.com/blog/2015-07-03/hacking-linux-usdt-ftrace.html),即未启用时零成本、启用时目标进程可选择做昂贵的参数准备。

### 5.4 后端栈

- **指标**:Prometheus(scrape 冷线程,15 s)+ Grafana;告警规则针对每阶段 p99 预算(例如 `parse_p99 > 5µs for 5m`、`tick_to_send_p99 > 50µs`、`feed_gap_total` 增速、`journal_sync_lag_bytes`)。
- **延迟明细与日志**:ClickHouse 自托管,摄入用 Vector 或 OTel Collector(ClickHouse 官方推荐 OTel + Grafana,也认可 Vector/Fluent;若"以指标为主需要 PromQL"则保留 Prometheus,https://clickhouse.com/docs/use-cases/observability)。Loki/Tempo 对我们没有优势:日志量小、trace 是我们自己的事件模型而非 OTel span,一个 ClickHouse 同时承载"每笔订单的阶段延迟"与"结构化日志"更省。
- **journal 本身就是最好的日志**:冷进程把 journal 里的 `LogEvent{level, code, args}` 渲染为文本 → Vector → ClickHouse;热线程从不格式化字符串。

### 5.5 交易所侧延迟测量与时钟

- 本地时钟:chrony 对 AWS Time Sync(PTP 硬件时钟可到 µs 级,04/05 覆盖);每段 `ClockCalib` 记录 chrony `offset/rms`,让研究侧知道每条 `ts_recv` 的可信区间。chrony FAQ(https://chrony-project.org/faq.html):开 `hwtimestamp`(网卡硬件时间戳)+ 交错模式 `xleave` + 短轮询,"亚微秒精度、几十 ns 的稳定性是可能的",前提是 CPU 定频、关 EEE;chrony ≥4.5 还能用 PTP 透明时钟的修正。`ts_mono_raw` 方面注意 quanta/minstant 的 TSC 路径只支持 x86,aarch64 回退到 std(https://github.com/metrics-rs/quanta;与 04 §2.5 一致),Graviton 上直接读 `cntvct_el0` 更省。
- feed 单向延迟 `ts_recv - E`:只有在时钟误差 ≪ 延迟时有意义(我们的 1–2 ms RTT 环境下需要 <100 µs 同步);按交易所/频道/小时画分布,是"交易所拥塞/我们线路劣化"的最早信号;Tardis 的 `local_timestamp - timestamp` 可作外部对照(其 Binance 期货采集机在 GCP 东京)。
- 订单 RTT 与半程:`t_ack_recv - t_send` 与交易所 `T`,可分解为"去程 + 撮合排队 + 回程";每小时探测单的 RTT 分布进回放延迟模型(§4.3)。

---

## 6. 风控与控制面

### 6.1 热路径 pre-trade checks(同线程,O(1))

在 sequencer 决策后、写出站事件前执行,全部是对预计算表的比较,目标 <200 ns [估算]:

| 检查 | 数据结构 | 备注 |
|---|---|---|
| 单笔数量/名义上限 | per-instrument `i64` | 定点数比较 |
| 价格带(相对本地 BBO/mark 的 bp 偏离) | per-instrument 缓存 BBO | 防 fat-finger 与簿空时的极端价 |
| 仓位/敞口上限(含在途订单) | per-instrument + per-asset 累加器 | 在途订单计入,回执时冲销 |
| 当日亏损熔断 | 全局累加器 | cdex 已有"当日亏损熔断"(memory: cdex-spread-engine),移入 Rust |
| 速率限制(交易所权重、每秒订单) | 令牌桶 per endpoint | 权重表来自交易所文档 |
| 重复/自成交 | 最近订单环形表 | Binance 有 STP 模式可选 |
| 状态门 | `enum RiskState{Normal, ReduceOnly, Halted}` | 由 control ring 切换 |

美国 SEC Rule 15c3-5(市场准入规则)对经纪商的要求列表——信用/资本阈值、错误订单(价格、数量)、重复订单、"pre-trade、immediate、automatic"——是一份现成的检查清单模板。

### 6.2 kill switch 语义(各所)

| 交易所 | 机制 | 参数 | 注意 |
|---|---|---|---|
| Binance USDⓈ-M 期货 | `POST /fapi/v1/countdownCancelAll` 倒计时,心跳式重复调用续期 | `countdownTime` 毫秒,0 取消;文档称系统约每 10 ms 检查一次倒计时 [不确定:凭记忆,文档站本机不可达] | 每 symbol 设置;倒计时到点撤该 symbol 全部挂单 |
| Binance 现货 | 无原生 dead-man's switch [不确定] | — | 本地看门狗 → `DELETE /api/v3/openOrders` / WS API `openOrders.cancelAll`;WS API 连接同样 24 h 强断、支持 Ed25519 与 SBE 二进制响应、`timeUnit=MICROSECOND`,回执带 `transactTime`/`workingTime`(https://github.com/binance/binance-spot-api-docs/blob/master/web-socket-api.md) |
| OKX | `POST /api/v5/trade/cancel-all-after` | `timeOut` 10–120 s,0 关闭 [不确定:凭记忆] | 按账户/连接续期 |
| Bybit | DCP(Disconnection Protect):私有 ws 断开超过窗口则撤全部期货/现货/期权挂单 | 默认 10 s,`timeWindow` 3–300 s;**仅 Ins(机构)客户可开,VIP 不可**;私有连接必须订阅 `dcp` 主题(https://bybit-exchange.github.io/docs/v5/order/dcp) | 我们大概率不满足资格,需本地方案 |
| Deribit | `private/enable_cancel_on_disconnect` | scope `connection` / `account` [不确定:凭记忆] | 参考实现 |
| Bitget | 未见 COD/倒计时 [不确定] | — | 本地看门狗 + cancel-all |

**本地语义**(与交易所侧互补):
1. gateway 检测到私有 ws 断开、心跳超时(Binance ping 20 s / pong 1 min;Bybit 建议 20 s 心跳)、连续 N 次下单失败 → 进入 `Halted`,通过 REST 撤全部挂单,并对冲敞口(cdex 已有"自动补腿");
2. sequencer 心跳丢失(看门狗线程 500 ms 未见进度)→ 外部 `killswitch` 进程用**独立凭证与独立进程**撤单;
3. 主机级:systemd `ExecStopPost` 跑 cancel-all(原型 `ExecStop=collectorctl drain` 的思路);
4. 每次 `Halted` 与其原因写 journal,回放可见。

### 6.3 对账循环(冷)

每 5–30 s 从 REST 拉 open orders / positions / balances,与 sequencer 的影子状态比对(通过 journal 尾部读取,不打断热线程);差异 → 告警,超过阈值 → `ReduceOnly`。这是对抗"传输错误被当作失败"(memory: cdex 9/24 事故,交易节点 403 把已上链交易当失败)与"确认 ≠ 成功"两类事故的唯一系统性手段;03 §5.1 的状态机是同一原则。

### 6.4 控制通道与热更新

- 命令类型化:`SetParam{key, value}`、`Pause/Resume`、`CancelAll{scope}`、`Flatten`、`SetRiskState`、`RollSegment`、`ReloadCatalog`;每条带 `cmd_id`、发起者、签名/来源。
- 传输:ops 进程(tokio)→ UDS → gateway 进程内的 control 线程 → SPSC control ring → sequencer;**sequencer 把命令当输入事件写 journal**,回放时命令与行情按原顺序重放。
- 参数热更新 = 命令,不是重读 TOML 文件;TOML 只是启动态快照,任何运行期改动都走命令(可审计、可回放)。cdex 现在"补丁后要跑 sync-code.sh"的做法由此终结。

### 6.5 Telegram / ops bot 作为冷进程

只读:mmap tail journal(仓位、P&L、延迟直方图、最近事件)→ 渲染消息;写:只能发上述类型化命令,带白名单 + 二次确认 + 速率限制;bot 崩溃/卡住对热路径零影响。Telegram 没有只读角色(memory: cdex-arb-system),所以 bot 进程本身就是权限边界,应最小化。

---

## 7. Python 的角色

| 需求 | 方案 | 依据 |
|---|---|---|
| 读 journal / Parquet 进 Polars | Rust reader crate 用 PyO3 + `pyo3-arrow` 暴露 `RecordBatchReader`,通过 Arrow PyCapsule Interface 零拷贝进 Polars(≥1.2)/pandas(≥2.2)/DuckDB/pyarrow | arro3/pyo3-arrow README:任意实现 PyCapsule 的库互通,arro3-core 7 MB vs pyarrow 100 MB(https://github.com/kylebarron/arro3);Polars `to_arrow(compat_level=newest)` 零拷贝 |
| 在 Python 里跑同一份 Rust 策略回测 | 策略、风控、订单簿是 Rust crate;`maturin` 打成 wheel;Python 只传参数与数据路径、收结果 Arrow 表 | hftbacktest 的模式(Rust 内核,Python 侧回测;实盘 bot Rust-only);nautilus 的 PyO3 路线 |
| 研究 → 参数版本化 | Jupyter 产出 `strategy_params/<name>/<hash>.toml` 提交仓库;实盘 `SetParam` 命令引用该 hash;journal 记录生效时刻 | 消灭"散落补丁脚本" |
| CI 回归 | (a)golden day 回放:决策哈希必须等于基线;(b)微基准阈值(ring、解析、订单簿、风控)不得回退 >10%;(c)属性测试 OMS 状态机(04 §7);(d)nightly 全天回放 + 延迟直方图对比 | 确定性是前提,§4.1 |
| 数据质量巡检 | Polars/DuckDB 脚本:缺口、负 feed 延迟、快照-增量不一致、重复;结果进 ClickHouse 看板 | hftbacktest 数据校验工具是参考 |
| 运维 | Telegram bot、报表、S3 生命周期、成本 | tokio/Python 均可,冷进程 |

**Python 不做的事**:任何在事件路径上的决策、任何订单发送、任何需要 <10 ms 响应的动作。cdex 的 Python 执行栈(recommend() 死循环烧核 7.5 h 那类事故)证明这条线必须划清。

---

## 8. 推荐方案(汇总)

1. **进程模型**:一个 Rust 热进程(feed handlers + sequencer + gateways,各绑核,无 tokio),一个 Rust 冷进程(uploader/processor/syncer/Prometheus/对账),一个 ops 进程(Telegram、REST 管理、Python 工具)。热↔冷只通过 mmap journal(单向)与 control ring(单向)。
2. **Journal**:mmap 顺序段,32 B zerocopy 头 + SBE 正文 + 原始载荷;UTC 整点滚动 + 快照先行 + 周期快照 + 段头链;fallocate 预分配;CRC32C;syncer 线程 250 ms fdatasync;/dev/shm 不用(要跨重启),本地 NVMe 实例存储优先、EBS 次之。
3. **时间**:四元组 + 序号 + 每段校准记录;`SO_TIMESTAMPING` RX/TX 硬件时间戳全开,错误队列由冷线程读。
4. **存储**:L0 段归档 zstd+字典 → S3;L1 Parquet(zstd-3,行组 128k–1M,hive 分区,长尾 symbol 按天);DuckDB/Polars 研究;第二阶段 ClickHouse 承载可观测与在线查询。
5. **回放**:sequencer 只吃 ring,时间/RNG 注入;回放器从 journal 派生 hftbacktest 式双时间轴;延迟模型来自 journal 内真实订单与探测单;队列模型用影子/小资金实盘校准;先合入 hftbacktest PR #323 再 fork 其模拟器。
6. **可观测**:事件内 8 个阶段戳;每线程每阶段 HdrHistogram 双缓冲;冷线程 Prometheus + ClickHouse;告警按阶段 p99 预算;热路径无字符串。
7. **风控**:同线程表查;交易所 dead-man's switch(Binance 期货 countdown / OKX cancel-all-after)+ 本地看门狗 + 独立 killswitch 进程 + systemd 兜底;对账循环 5–30 s;所有命令进 journal。
8. **Python**:PyO3/Arrow 读取器、同一 Rust 策略库回测、参数 hash 版本化、CI 决策哈希回归。
9. **迁移顺序**:先把 journal 格式与 snapshot-at-roll 落地并用它替换 `crates/wal`(立刻止血 84% 隔离);再合并 collector 与执行路径为单进程;再做回放器与影子模式;最后 ClickHouse。
10. **必须实测的项**:c7g/c8g 上 SPSC 单跳 ns、mmap append + 周期 fdatasync 的写者 p99、ENA 硬件 RX ts 与用户态戳的差、EBS vs 实例存储 fdatasync 延迟、zstd 字典对我们 JSON 的实际比率、DuckDB 在 S3 上按 hour 分区扫描的耗时。

---

## 9. 参考链接

**事件传输 / journal**
- Chronicle Queue README(延迟表、roll cycle、tailer 只读、"record everything"):https://github.com/OpenHFT/Chronicle-Queue ;FAQ(page cache、无同步模式):https://github.com/OpenHFT/Chronicle-Queue/blob/ea/docs/FAQ.adoc
- LMAX 架构(Fowler):https://martinfowler.com/articles/lmax.html ;Disruptor 技术论文:https://lmax-exchange.github.io/disruptor/disruptor.html
- Aeron 性能测试与最佳实践(term buffer、/dev/shm、idle strategy):https://github.com/aeron-io/aeron/wiki/Performance-Testing 、https://github.com/aeron-io/aeron/wiki/Best-Practices-Guide
- rigtorp:SPSC 优化(5.5M → 112M ops/s):https://rigtorp.se/ringbuffer/ ;seqlock:https://rigtorp.se/seqlock/ ;低延迟调优指南:https://rigtorp.se/low-latency-guide/
- iceoryx2:https://github.com/eclipse-iceoryx/iceoryx2

**事件格式 / 压缩 / 持久化**
- SBE 设计原则与向后兼容:https://github.com/aeron-io/simple-binary-encoding/wiki/Design-Principles ;FIX SBE 规范:https://www.fixtrading.org/standards/sbe-online/
- zstd README 基准与字典:https://github.com/facebook/zstd ;lz4 README:https://github.com/lz4/lz4
- Linux 时间戳文档(SO_TIMESTAMPING 全部标志):https://docs.kernel.org/networking/timestamping.html
- PostgreSQL WAL 可靠性(写缓存、撕裂写):https://www.postgresql.org/docs/current/wal-reliability.html
- hftbacktest 数据格式(8 字段、EXCH/LOCAL flags):https://hftbacktest.readthedocs.io/en/latest/data.html

**交易所文档**
- Binance 现货 WS(24 h、ping/pong、timeUnit=MICROSECOND、本地订单簿流程、bookTicker):https://github.com/binance/binance-spot-api-docs/blob/master/web-socket-streams.md
- Bybit 订单簿(档位/频率、u/seq/cts):https://bybit-exchange.github.io/docs/v5/websocket/public/orderbook ;DCP:https://bybit-exchange.github.io/docs/v5/order/dcp ;WS 连接与心跳:https://bybit-exchange.github.io/docs/v5/ws/connect
- Tardis Binance 期货采集细节(depth@0ms、生成的 depthSnapshot、pu/u 校验、GCP 东京):https://docs.tardis.dev/historical-data-details/binance-futures ;CSV 布局:https://docs.tardis.dev/downloadable-csv-files ;FAQ(~1 TB/天):https://docs.tardis.dev/faq

**存储**
- DuckDB 文件格式性能(行组/文件大小微基准):https://duckdb.org/docs/current/guides/performance/file_formats.html ;Parquet 写入技巧(ROW_GROUP_SIZE、排序剪枝):https://duckdb.org/docs/current/data/parquet/tips.html ;Hive 分区:https://duckdb.org/docs/current/data/partitioning/hive_partitioning.html ;S3 API:https://duckdb.org/docs/current/core_extensions/httpfs/s3api.html
- Databento DBN 记录布局(record.rs):https://github.com/databento/dbn/blob/main/rust/dbn/src/record.rs
- kdb+tick 架构(tickerplant 日志 → RDB 回放 → HDB):https://code.kx.com/q/architecture/
- AWS EBS 卷类型与延迟:https://docs.aws.amazon.com/ebs/latest/userguide/ebs-volume-types.html
- ArcticDB(BSL 许可、性能声称):https://github.com/man-group/ArcticDB 、https://docs.arcticdb.io/
- ClickHouse 可观测性用例:https://clickhouse.com/docs/use-cases/observability
- Polars Parquet / 云存储 / Arrow 互通:https://docs.pola.rs/user-guide/io/parquet/ 、https://docs.pola.rs/user-guide/io/cloud-storage/ 、https://docs.pola.rs/user-guide/misc/arrow/
- Apache Parquet 格式:https://parquet.apache.org/docs/file-format/

**回放 / 回测**
- hftbacktest 延迟模型:https://hftbacktest.readthedocs.io/en/latest/latency_models.html ;成交与队列模型:https://hftbacktest.readthedocs.io/en/latest/order_fill.html ;订单延迟数据采集:https://hftbacktest.readthedocs.io/en/latest/tutorials/Order%20Latency%20Data.html ;队列模型对比:https://hftbacktest.readthedocs.io/en/latest/tutorials/Probability%20Queue%20Models.html ;数据准备(快照、Tardis、时间戳语义):https://hftbacktest.readthedocs.io/en/latest/tutorials/Data%20Preparation.html
- hftbacktest 回测/实盘差异排查:https://hftbacktest.readthedocs.io/en/latest/debugging_backtesting_and_live_discrepancies.html
- rigtorp 队列位置估计(2013):https://rigtorp.se/2013/06/08/estimating-order-queue-position.html

**可观测性**
- HdrHistogram Rust:https://github.com/HdrHistogram/HdrHistogram_rust
- Brendan Gregg uprobes / USDT:https://www.brendangregg.com/blog/2015-06-28/linux-ftrace-uprobe.html 、https://www.brendangregg.com/blog/2015-07-03/hacking-linux-usdt-ftrace.html
- LTTng 文档:https://lttng.org/docs/v2.13/ ;lttng-ust(3):https://lttng.org/man/3/lttng-ust/v2.13/
- tokio-rs/tracing:https://github.com/tokio-rs/tracing ;metrics-rs / quanta:https://github.com/metrics-rs/metrics 、https://github.com/metrics-rs/quanta
- chrony FAQ(硬件时间戳、亚微秒精度、PTP 透明时钟):https://chrony-project.org/faq.html
- ClickHouse + OpenTelemetry 摄入:https://clickhouse.com/docs/observability/integrating-opentelemetry
- Binance 现货 WS API(24 h、Ed25519、SBE、timeUnit):https://github.com/binance/binance-spot-api-docs/blob/master/web-socket-api.md

**Python**
- arro3 / pyo3-arrow:https://github.com/kylebarron/arro3 ;PyO3:https://github.com/PyO3/pyo3
- hftbacktest 主页(Rust 内核 + Python 回测 + Rust 实盘):https://hftbacktest.readthedocs.io/en/latest/
