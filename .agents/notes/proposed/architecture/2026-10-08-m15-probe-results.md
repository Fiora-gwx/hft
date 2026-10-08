# Agent Note: M1.5 延迟探针结果:Graviton 上的第一份分段直方图与收包、时钟、IRQ 核结论

Status: proposed

## Problem

[M1.5 延迟探针](2026-10-03-latency-probe-milestone.md)要在写完热路径之前回答 [11 §3.1](../../../../design/11-实施路线图.md#31-m15-延迟探针) 第 4 步的问题:hft-net 的收包细节、时钟换算口径是否成立、IRQ 核要不要保持唤醒、HW RX 时间戳与 PHC 是否可用,并给出 [latency-budget §3](../../../../docs/latency-budget.md#3-cex-路径预算) 的第一版 Graviton 分配。2026-10-08 在 apne1-az4 的一台 `c8g.4xlarge` 上跑完了 SLC-100–SLC-102 与 NET-370–NET-374;运行记录、主机事实与全部表格在 [ops-log 2026-10-08-m15-probe](../../../../ops-log/2026-10-08-m15-probe.md)。本 Note 只写结论与建议,不改 latency-budget。

测量条件(解读数字前必须知道):单网卡(SSH 与测量共用 `ens34`);一个 venue(Binance 现货)、一个 inst(BTCUSDT)、2 条 BBO 赛马连接;ENA 中断合并保持默认(adaptive-rx on、rx-usecs 20);下单走演示环境(不在本 AZ,TCP RTT 约 1.9 ms);**HW RX 戳不可用,全部是 SW 数据集**(不含网卡到软中断一段)。

## Proposal

### 1. 结论

**收包细节**

- 中断驱动 + 热线程非阻塞 `recvmsg` 的形态成立:平静层 `kernel_rx`(SW)p50 2.6–2.9 µs、p99 3.7–4.5 µs、p99.9 ≤ 6.3 µs(SLC-100 与 SLC-101 各次);空读 `recvmsg` 278–279 ns(`net_poll_empty`);busy poll 关闭时空读不进 `napi_busy_loop`(NET-372);握手后 `napi_id` 全部非 0 且映射到交易网卡(NET-371,8 个队列都出现);`recvmsg` 报告的是本次读到的最后一个 skb 的戳(NET-373,100 %)。
- **突发层的尾部不是收包实现的问题,而是 TCP 乱序的队头阻塞**:突发层 `kernel_rx` p99/p99.9 落在 ≈ 65.5/131/262 µs 三个台阶(2¹⁶–2¹⁸ ns);bpftrace 证明数据按序入队后 1–4 µs 内就被 feed-bbo 读走(23 386 次全部 < 16 µs),多出的时间发生在打戳之后、按序入队之前(`TCPOFOQueue` 占入站段 0.005–0.12 %,乱序段多为多帧突发包)。两条赛马连接的胜出记录仍带这段等待,说明两条流同时乱序。
- `multi_skb / frames` = 7.6 %(> 1 %),按 hft-net 已知限制要评估逐包读;但 NET-373 已证明一读多段时取的是最后一段的戳,一读多段只会把 `kernel_rx` 估小(早到的段多等的那一段不计),不会造成上面的尾部。
- NET-374 按字面不通过(返回值 < 所给长度且返回后 `rmem_alloc > 0` 的样本 7 124 万 / 1.83 亿),但写端持续写入时这个计数与新数据到达有竞态,只是上界。"短读即停"在实盘上的代价已经有直接证据:数据按序入队到被读走 ≤ 16 µs,说明短读后剩下的数据下一圈(< 1 µs)就会被读走,对 BBO 路径无可见影响。
- 就绪探测(`epoll_pwait(0)`)要 ≥ 3 条连接才开启,本切片 2 条 BBO 连接,收益未测(留给 M3 末探针,VEN-384)。

**时钟换算口径**

- CNTVCT 频率 1 000 000 000 Hz(校准值与标称一致到 mHz)。
- 以 REALTIME 为锚(chrony 锁在 PHC 上,RMS offset < 0.7 µs)时,逐样本误差界 p50 0.21–0.23 µs、p99 1.3 µs、max 1.8 µs,`unsynced_cal`(> 1 µs)3.1 %——口径成立。
- 以 ENA PHC 为锚(hk 每秒一次 `PTP_SYS_OFFSET_EXTENDED`)时,误差界变宽:p50 0.61 µs、p99 2.5 µs,`unsynced_cal` 26–38 %,`tick_to_trade` 等组因此掉到 n < 1 000,报告器不给 p99。ENA PHC 的读取窗口约 2.5 µs,`report.cal_max_err_ns = 1 000`(latency-budget §2.1 阈值的初值)对它太紧。
- HW RX 戳不可用时没有任何分段需要 PHC 端点:SW 数据集应以 REALTIME 为锚,hk 不必读 PHC。

**IRQ 核要不要保持唤醒:不要。**

- 本主机没有 cpuidle 驱动,空闲就是 WFI;用 hostprobe 允许的 keepawake 线程(nice 19 自旋,SCHED_OTHER)保持核 1 唤醒,与空闲相比:平静层 `kernel_rx` p50 +0.4 µs、p99 +0.7 µs,回报触发的 `kernel_rx` p50 +2.1 µs,`tick_to_trade` p50 +0.6 µs,`rep_to_order` p50 +1.8 µs;`egress`、`order_rtt` 不变。WFI 的唤醒代价在 SW 分段里看不出来,而自旋线程让收包变慢。
- 压舱(4 条 `DepthTopN` 连接、200 个交易对、约 3 900 帧/s、5 MB/s,核 1 软中断约 4 000 次/s)只让平静层 `kernel_rx` p50 +0.2 µs、p99 +0.4–0.8 µs,`rep_to_order` p50 +0.5–0.8 µs;每包软中断成本约 1–2 µs(tick 采样约 1.1 µs/段;bpftrace 平均 1.84 µs/次 NET_RX)。这个负载下 IRQ 单核远未饱和(核 1 软中断 < 1 %)。`nic_to_kernel` 不可测,"突发窗口里 BBO 排在批量包后面"的量仍要 HW 戳或 M3 末满订阅探针。

**HW 戳与 PHC**

- HW RX 时间戳:**不可用**(c8g/ENA,内核 7.0:`ethtool -T` 无 HW 接收过滤模式,开 PHC 后也一样)。[12 A7](../../../../design/12-风险与未决问题.md) 按"不可用"处理;门槛 ⑩ 按 testing §8.2 第 4 条用 SW 锚数据集。
- PHC:**可用但默认关闭**。ENA 的 `devlink` 参数 `enable_phc`(driverinit,默认 false)设为 true 并 `devlink dev reload` 后出现 `/dev/ptp0`(`ena-ptp-00`),不跨重启保留;chrony `refclock PHC /dev/ptp0 poll 0` 锁定良好。hft-rt `Phc::open` 用 `O_RDWR`,设备默认 0600,非 root 进程打不开时 `latency-slice` 静默退回 REALTIME。

### 2. 对 latency-budget §3 第一版 Graviton 分配的建议(只给建议,不改该表)

依据:`c8g.4xlarge`、`c16` 线程子集、单 venue 单 inst、2 条 BBO 连接、单网卡、SW 数据集、ENA 默认中断合并;不含满订阅、关键帧与段退役、`sim` 同机。

| 分段 | 实测(p50 / p99,µs) | 建议的 Graviton 第一版分配 | 说明 |
|---|---|---|---|
| `kernel_rx`(SW,平静层) | 2.6–2.9 / 3.7–4.5 | 目标 p50 ≤ 4、p99 ≤ 10 | 现行 `c32` 目标 8/20 留有余量;`c16` 一个 feed-bbo 线程承载多 venue 时轮询节拍会加进来,M3 末再定 |
| `kernel_rx`(SW,突发层) | 2.7–3.0 / 135–263(p99.9 ≈ 266) | 不按主机预算分配;单列"网络乱序"记录项,判定用平静层或 HW 数据集 | 尾部来自 TCP 乱序(网络路径),主机调优去不掉;按 §3.1 作 p99.9 记录项的一个来源登记 |
| `ingest`(bookTicker) | 平静 1.2–1.5 / 1.7–2.8;突发 2.9–3.2 / 17–23 | 目标 p50 ≤ 3 不变;p99 分层:平静 ≤ 5、突发 ≤ 30 | 突发层的 `ingest` 含同一读里排在前面的帧的解码与写 raw(一批 5–36 帧);`c32` 的 ≤ 10 只对平静层成立 |
| `decide` | 0.1–0.2 / 0.2 | 不据此改 | 简化 sequencer,不代表真实策略 |
| `egress` | 3.3–4.2 / 3.9–8.1 | 目标 p50 ≤ 5、p99 ≤ 10 | 现行 5/15;New 的 p99 在 PHC 锚的运行中到 7–8 µs(原因未查,gw 核 8 上有 NVMe managed IRQ) |
| `tick_to_trade` | 8.5–8.8 / 11.9–12.0 | 目标 p50 ≤ 15、p99 ≤ 50(主机内,平静层触发) | 远低于现行目标 22/75 与硬线 30/100;但只有一个 inst、`decide` 近乎为 0,M3 末满订阅后重定 |
| `rep_to_order` | 8.9–9.8 / 13.2–13.3 | 目标 p50 ≤ 15、p99 ≤ 40 | 现行 25/80;压舱 +0.5–0.8 µs |
| `order_rtt` / `ws_rtt` | 2.8–3.0 ms(演示环境) | 不据此定 | 演示环境不在 az4(TCP RTT 1.9 ms);主网 `ws-api` 在 az4 约 0.18 ms(az-probe) |

分配校核(latency-budget §3 的规则:各段目标 p99 之和 ≤ 总目标 p99 × 0.85):`kernel_rx` 10 + `ingest`(平静)5 + `decide` 15(沿用现行值,待真实策略)+ `egress` 10 = 40 ≤ 0.85 × 50 = 42.5。这些建议比现行目标更紧,但只覆盖一个 inst、`decide` 近乎为 0;建议 M3 末满订阅探针之前保留现行 `tick_to_trade` 目标(22/75)与硬线(30/100)不动,把上表作为第一版 Graviton 参考附在 §3 表下。

### 3. 建议的后续改动(各自走正常的改动流程)

1. hft-rt:`Phc::open` 改为 `O_RDONLY`(读 PHC 与 `PTP_SYS_OFFSET*` 不需要写权限),打不开时在 `ClockStatus`/日志里明确报出;deploy 加 `enable_phc` 的 devlink 设置(开机单元)与 `/dev/ptp*` 的组权限 udev 规则。
2. 时钟口径:HW 戳不可用的主机上 hk 以 REALTIME 为锚、不读 PHC;若保留 PHC 锚,`cal_max_err_ns` 按 ENA PHC 读取窗口取 ≥ 3 µs(待 M3 末复核)。
3. hostprobe/deploy:IRQ 核保持唤醒的登记表保持为空(本 A/B 否定了 keepawake 线程);`nohlt` 未测(全局生效、要重启),无理由再测。
4. deploy GRUB:加 `isolcpus=managed_irq,…`——本机 NVMe managed IRQ 的 effective 落在核 1(IRQ 专核)与核 8(gw 隔离核)。
5. venue-binance:ws-api 空闲连接的 `idle_timeout_ms = 40 000` 假定服务端每 20 s ping,演示环境的备用下单连接 40 s 收不到帧被反复回收重登(每次运行数十到数百条 `ConnState`);gw 应对空闲的备用连接主动发 WS ping,或把演示环境与主网的空闲超时分开核实(开发备忘)。
6. hft-net:NET-374 改用不与写端竞态的测法(写端按块写、读端在两块之间读),再决定短读规则;逐包读评估按 `multi_skb` 7.6 % 记一笔,优先级低(不影响尾部)。
7. latency-slice:查清演示环境每次运行开头 2–3 笔 `-2010 Duplicate order sent` 的原因(换 `slice.epoch` 仍出现,先核对 client id 的生成);主机脚本修 `CASE=… .` 前缀赋值、NET-372 的 `--features test-io`、NET-371 一次 20 条超出 Connector 作业槽(16)。
8. 网络乱序:M3 末探针加 HW 无关的旁证——按连接记 `TCPOFOQueue`/SACK 计数并与 `kernel_rx` 突发尾部对齐;评估 3 条以上 BBO 赛马连接(不同源端口/不同 IP)是否能削掉两条流同时乱序的尾部。

## Alternatives considered

**把突发层 `kernel_rx` 的 262 µs 尾部当成主机问题去调(中断合并、GRO、RPS)。** bpftrace 显示数据按序入队后 ≤ 16 µs 就被读走,尾部在按序入队之前;中断合并发生在打戳之前,改它不影响 SW `kernel_rx`。没有证据支持主机调优,不做。

**用 `nohlt` 测保持唤醒。** `nohlt` 在本内核可用(`CONFIG_GENERIC_IDLE_POLL_SETUP=y`),但对全部核生效、要重启两次,而 keepawake 线程的结果已经是"变差";WFI 唤醒代价在 SW 分段中不可见,没有理由再测。

**以 PHC 为锚出 SW 数据集。** 误差界 p99 2.5 µs、`unsynced_cal` 最高 38 %,样本被成批剔除;REALTIME 锚(chrony 锁 PHC)误差界 p99 1.3 µs、剔除 3 %。HW 戳不可用时 PHC 锚没有收益。

**按 TESTS 原样跑 30 min × 6 与单独的 NET-370 60 min。** 两个阻断缺陷(Md 会话 endpoint、限速观测折算)的修复用掉约 1.5 h;为在 6.5 h 内收尾,SLC-101/102 每次 25 min、NET-370 与 SLC-100 合并。每次仍有 > 1 400 个新单样本与 > 14 万条 Bbo,A/B 的方向在两轮之间一致。

## Acceptance criteria

- latency-budget §3 在 M3 末满订阅探针后重定时,引用本 Note 的 Graviton 实测(平静/突发分层、SW 数据集)与第 2 节的建议,并写明是否采纳"突发层不按主机预算分配"。
- 第 3 节第 1、2、4、5 条各有对应改动或明确的否决记录。
- deploy 的 keepawake 登记表保持为空(DEP-245.16),hostprobe 规则中的 `nohlt` 一项注明"M1.5 未测,keepawake 线程 A/B 为负"。

## Risks

- 只有一个 venue、一个 inst、两条 BBO 连接、单网卡、演示环境下单;满订阅、`c16` 多 venue 共用 feed 线程、关键帧与段退役、`sim` 同机都没测到,数字偏乐观。
- 各次运行的行情强度差别大(25 min 内 Bbo 14 万–41 万条),A/B 只能比同一轮内的差;SLC-102 只有一轮。
- 突发尾部的成因判断依赖一次 5 min 的 bpftrace 诊断与乱序计数的量级吻合;台阶为什么是 2¹⁶ ns 的倍数没有解释。
- 平静层的 SW `kernel_rx` 不含网卡到软中断一段(中断合并 rx-usecs 20 + 自适应);它在 `order_rtt` 与 `feed_oneway` 里,但无法单独量出。
