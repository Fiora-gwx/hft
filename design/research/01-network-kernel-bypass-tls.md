# 01 · 网络 / 传输 / TLS / WebSocket 数据路径研究(2026-09-29)

> 范围:从网卡到 WebSocket 帧解析完成、再到下单报文离开网卡的整条路径。目标指标是 tick-to-trade 的 p50 与 p99。
> 现状基线(2026-09-19 实测,c7g.large,Graviton3,2 vCPU,ENA 2 队列,apne1-az2,无 placement group):主机内核路径 50–100 µs,tick-to-trade 3–5 ms;`ss -ti` minRTT:Gate 0.27 ms、Binance fstream/ws-fapi 1.0–1.5 ms、Bitget/KuCoin(CloudFront)1.1 ms、OKX/KuCoin(Cloudflare)1.4–1.8 ms;TLS 握手到 Binance ~30 ms;共享 2 核上 `sleep(0)` 调度抖动 p99 155 µs。
> 状态:v1.0。所有数字均标注来源;"[不确定]" = 未拿到一手数据的判断;"[估算]" = 由一手数据推算。链上部分见 03,框架/库选型见 04,本文不重复(仅在 §6 补充网络数据路径相关的 crate 事实)。
> 方法说明:WebSearch/WebFetch 在本次会话不可用,全部来源通过本机 curl 抓取到 `_src/n1_*`、`n2_*`、`n3_*`;Kraken 博客、hftadvisory、exablaze.com、binance.com 支持页、OKX/Gate/KuCoin/Bitget 文档站从本机不可达(超时),相关条目在文中注明缺口。

---

## 0. 结论摘要(Top 10,带数字)

1. **我们 3–5 ms 的 tick-to-trade,90% 以上花在交易所内部与分发网络,不在我们机器上。** Deltix/EPAM 在 AWS Tokyo(c5.2xlarge,apne1-az1,2023-06)用 pcap + 交易所时间戳测 Binance BTCUSDT WS 行情:交易所 `E` → 本机网卡 **p50 3.97 ms,p90 5.3 ms,p99 13.1 ms,p99.9 15.9 ms**;三个 AZ(az1/az2/az4)中位数 3.8/4.6/4.1 ms,差异在误差内。主机内核路径 50–100 µs 只占 2–3%。**因此路线是:先拓扑(AZ/placement,可省 0.5–1.5 ms RTT),再主机(独占核 + 内核调优,可省 50–150 µs 尾部),最后才是协议(SBE/FIX)与 bypass(≤10 µs)。** 这与 AWS 2026-04 tick-to-trade Part 2 的"五层延迟层级"一致。
2. **AWS ENA 上的 kernel bypass 到 2026-09 仍不值得做。** Onload 走 AF_XDP 在 ENA 上:#62(2022,Amazon 工程师)/#337(2026-08,c8a/c6a,ENA 2.17.2,`Failed to allocate stack rc=-95`,`FIXME AF_XDP` 存根)失败;#139(2023,m5zn.12xlarge)能跑但 **UDP ping-pong 中位数 42.7 µs → 47.9 µs、max 137 → 241 µs,反而变差**。AWS 自己的 `trading-latency-benchmark/networking_benchmarks/open_onload` 需要 **打补丁的 ENA 驱动 + 打补丁的 Onload(commit 2d4ec08aa)、Nitro v5+(i7ie/m8i/r8i)、两块 ENI**,且 `EF_AF_XDP_ZEROCOPY=1` 下硬件时间戳失效。ENA 的 `ntuple-filters: off [fixed]` 横跨 Nitro v4/v5/v6。
3. **在 AWS 上,网络 fabric 才是地板,bypass 动不了它。** AWS 同一仓库实测(64 B,同 AZ + cluster placement group):m8a.metal-24xl **内核 UDP RTT p50 20.7 µs / p99 24.1 µs**;DPDK(ENA PMD,经 TAP 一跳)25.6 / 28.2 µs;AF_XDP zero-copy 转发器在 c7i CPG 内 **RTT p50 32 µs / p99 37 µs**,作者结论 "**on virtualized ENA, ~23 µs/hop is NIC/hypervisor-bound**,CPU 侧调优收益递减"。虚拟机同 AZ 非 CPG:33.7 µs;不调优的 TAP 路径 100 µs。
4. **调优后的内核栈在 EC2 上能到的水平:** AWS 回环基准(C++ 客户端 ↔ Rust 撮合模拟,含两次穿栈)m8azn.metal **p50 17.7 µs / p99.9 19.4 µs**,c8g.metal 21.4 / 23.4 µs;裸金属比同族最大虚拟机 p99.9 好 **15–29%**,中位数好 6–14%。Cloudflare 2015 年的经典阶梯(10GbE,2 GHz Xeon,UDP RTT 最小值):默认 44.9 → `SO_BUSY_POLL` 36.6 → 用户态自旋 31.8 → 绑核 30.1 → RSS/XPS 归位 28.4 → `rx-usecs 0` 25.7 → **Onload 17.7 µs(标准差 1.4 vs 8.6 µs)**。即裸金属上 bypass 省 ~4 µs/单程、抖动降 6 倍——这是 Phase 2 的收益上限。
5. **Linux 6.9–6.17 已把"忙时轮询、闲时中断"做进内核:** `EPIOCSPARAMS`(6.9)按 epoll 实例设 busy poll;**IRQ suspension(6.13,Karsten/Damato)** memcached 基准(EPYC 7662 + ConnectX-5,200K qps):基线 p99 416 µs @26% CPU,全忙轮询 133 µs @100%,**suspend10 = 141 µs @32% CPU**,老式 napibusy 277 µs @56%;**threaded NAPI busy poll(6.17-rc,Google)** AF_XDP xdp_rr:中断模式 p99 21.2 µs → 内核线程轮询 **13.0 µs**,比用户态独立核轮询再低 1–1.5 µs。ENA 从 2020 年的 prefer-busy-poll 补丁集起就被覆盖,ENA 2.17 还加了 busy poll 计数器。
6. **TLS 每记录成本不是瓶颈:** rustls 0.23.31 + aws-lc-rs(Xeon E-2386G)AES-128-GCM 接收 **7.95 GB/s** → 2 KB 记录 ≈ 0.26 µs、200 B ≈ 0.03 µs,再加每记录固定开销 ~0.1–0.3 µs [估算];ARM64(Ampere Altra Q80-30)只有 **2.17 GB/s** → 2 KB ≈ 0.95 µs(Graviton3/4 单核加密应快 1.5–2× [不确定])。三大库批量传输差距 ≤ 20%,rustls 2026-03 报告 16 项测试拿 14 个第一。**真正贵的是握手**(我们 30 ms,其中 CPU 侧只有 0.2–0.5 ms)→ 用热备连接而不是 kTLS/网卡卸载。
7. **JSON 是主机侧最大单项,但也只在 µs 量级:** sonic-rs 结构体反序列化 twitter.json(632 KB):x86 0.83 ms(**0.76 GB/s**),Apple M1 Pro 0.46 ms(1.37 GB/s);simd-json 1.09 / 0.43 ms;serde_json 1.38–2.29 / 0.53–0.83 ms。yyjson DOM 解析 1.7–3.5 GB/s,simdjson DOM 1.5–2.2 GB/s(i5-8259U / c5a.large)。→ 300 B bookTicker ≈ **0.2–0.5 µs**,2 KB depth ≈ **1.5–3 µs**;惰性 `get_unchecked` 跳字段 12 GB/s。
8. **协议升级空间在 Binance Spot,不在 USDT-M 合约:** Spot SBE 流(`stream-sbe.binance.com:9443`,Ed25519 key,时间戳 µs,`bestBidAsk` 带 `eventTime` + 高负载自动丢弃过期事件,**diff depth 20 ms 推送 vs JSON 100 ms**,depth20 50 ms);Spot FIX(`fix-oe/fix-dc/fix-md.binance.com:9000`,TCP+TLS,Ed25519,另有 FIX-SBE 编码);JSON 流可加 `timeUnit=MICROSECOND`。合约仍只有 JSON WS(见记忆笔记);NeutK 实测 SBE **到达不早于 JSON**——SBE 的价值是解析成本与精度,不是到达时间。
9. **AWS 特有事实:** ENA Express(SRD)仅同 Region 实例间、双端开启、轻载时**中位数升高数十 µs**,AWS 明说"HFT 很少是正确选择";EFA 不出公网。**硬件 RX 时间戳**(2025-06,64 位纳秒,`SO_TIMESTAMPING` ts[2],虚拟机也可)+ PHC(误差界 <40 µs;chrony+PHC 公式 ±25–34 µs vs NTP ±1.4 ms)在 Gen7+ 家族(经 2026-06 的 Precision Time Placement Group)或 M7a/M7g/M7i/R7a/R7g/R7i/I8g 上免费可用,需 GitHub ENA 驱动 `phc_enable=1`(PHC ≥2.10,HW 包时间戳 ≥2.15)。**Tokyo 跨 AZ 1.54–1.87 ms(bitsand.cloud 2023-10),是全球最慢的 Region 之一**——我们到 Binance 的 1.0–1.5 ms minRTT 就是跨 AZ 的样子,到 Gate 的 0.27 ms 是同 AZ 的样子。
10. **决策:** Phase 1(AWS,现在到 Q1)= 先用 HW 时间戳把 tick-to-trade 拆成"交易所→网卡 / 网卡→应用 / 应用 / 应用→网卡"四段;EC2 hunting 选 AZ(优先验证 apne1-az4 假设);换独占物理机(m8azn.metal-12xl 或 c8g.metal-24xl);内核调优(`rx-usecs 0`、绑核、`EPIOCSPARAMS prefer_busy_poll` + IRQ suspension 或专核自旋);无运行时自旋线程 + rustls/aws-lc-rs + tungstenite 同步 + 专用 Deserialize;N+1 热备连接;Binance Spot 走 SBE/FIX。**不做** DPDK/AF_XDP/Onload-on-ENA/ENA Express/kTLS。Phase 2(裸金属/托管)只在交易所提供 AWS 之外的接入(FIX 专线、MM 线路)时启动;NIC 选 **AMD Solarflare X4 + Onload**(POSIX 透明,rustls 零改动,X4 比 X3/X2 平均延迟低 27%/43%),备选 ConnectX-7 + XLIO(支持 Arm,uTLS 卸载);预期主机内 tick-to-trade 10–30 µs,如 Deltix 撮合引擎实测 Onload 使 p50 78 → 32 µs、p99 243 → 99 µs。

---

## 1. Kernel bypass 选项与"它到底能省多少"

### 1.1 基线:调优内核栈的分段成本

| 路径段 | 典型值 | 来源 |
|---|---|---|
| NIC DMA → 硬中断 → softirq → socket 队列(中断驱动) | 5–15 µs,含 IRQ 投递与唤醒 | AWS OS Tuning Reference("eliminating the 5–15 µs interrupt delivery path") |
| 同上,busy poll 命中 | 1–3 µs | 内核 NAPI 文档;Cloudflare 阶梯每步 1–7 µs |
| `epoll_wait` 返回 + `read` 系统调用 + skb→用户缓冲拷贝 | 0.5–2 µs | 一次 syscall 100–300 ns,小包拷贝可忽略 |
| C-state 退出 | 10–200 µs(C1 以上) | AWS OS Tuning Reference;必须 `max_cstate=1` 或 `idle=poll` |
| 调度器迁移 / 邻核抢占 / 定时器 tick | 数十到数百 µs 的尾部 | 我们实测 p99 155 µs;rigtorp `hiccups` 隔离核 max 18 µs vs 非隔离核 17 ms |
| Nitro 虚拟化 vs 裸金属 | p99.9 差 15–29%,p50 差 6–14% | AWS t2t Part 2 表 2 |
| EC2 网络 fabric 单程 | ~10–12 µs(CPG 内裸金属,RTT 20.7 µs);~17 µs(同 AZ 虚拟机,RTT 33.7 µs) | AWS networking_benchmarks(mcast2ucast) |

**推论:** 绑核、忙轮询、无中断的内核栈,NIC→应用单程 ≈ 2–5 µs,应用→线上 ≈ 2–4 µs;裸金属 bypass 能压到 ≈ 1 µs 和 ≈ 0.5–1 µs。**差距 ~5 µs/单程**:在 3–5 ms 的现状预算里不可见;在 Phase 1 目标(主机内 p50 < 30 µs)里占 15–30%,才开始有意义;而在 AWS 上它被 ~10 µs 的 fabric 单程与 hypervisor 抖动淹没。

### 1.2 各方案对照

| 方案 | 公开延迟数字 | TCP+TLS 出站可用性 | ARM64 | AWS ENA 可用性 | 判断 |
|---|---|---|---|---|---|
| **调优内核(busy poll / IRQ suspend / 专核)** | 单程 2–5 µs;EC2 回环 RTT p50 17.7 µs(m8azn.metal);Cloudflare 10GbE RTT 25.7 µs min | 原生 | 是 | 是 | **Phase 1 基线** |
| **AF_XDP copy mode** | 比内核少 1–3 µs,仍有 syscall;kernel 文档:XDP_COPY 仍"copy of the data into user space" | 无 TCP,需自带栈 | 是 | 是(in-tree ENA 支持 XDP_REDIRECT) | 只适合 UDP/多播 |
| **AF_XDP zero-copy** | AWS 转发器:c7i CPG RTT p50 32 / p99 37 µs;"~23 µs/hop 为 NIC/hypervisor 地板" | 无 TCP | 是 | GitHub ENA ≥ 2.13(2.13.0 "Re-enable AF XDP zero-copy"),channel ≤ 最大值一半,MTU ≤ 3498,hugepage 不兼容 | 不适合出站 TLS 长连接 |
| **DPDK(ENA PMD)** | AWS mcast2ucast:m8a.metal CPG RTT 25.6/28.2 µs(含 TAP 一跳)vs 内核 UDP 20.7/24.1 µs;AWS 称 "saving 1–10 µs per packet" | 需 F-Stack/TLDK 等用户态 TCP 栈 | 是(PMD 支持 aarch64) | 是;独占网卡;LLQ 需 `igb_uio wc_activate=1`(WC 映射),`llq_policy=0` 在 Nitro v4+ "huge performance degradation" | 工程量大、收益小 |
| **OpenOnload + Solarflare X2/X3/X4** | Cloudflare:RTT 25.7 → 17.7 µs,σ 8.6 → 1.4 µs;Deltix 撮合引擎(Java)p50 78 → 32 µs,p99 243 → 99 µs,p99.9 359 → 158 µs;X4 "sub-microsecond",X3 比 X4 高 27%(4 B 时 35%),X2 高 43%(4 B 时 55%) | **POSIX socket 透明加速,rustls/OpenSSL 不改** | README 只列 x86 发行版与内核,无 aarch64 [x86_64-only,高度可能] | 否:AF_XDP 路径在 ENA 上失败或变慢(§1.4) | **Phase 2 首选** |
| **TCPDirect / ef_vi(Solarflare)** | ef_vi 是 L2 原语层,延迟地板(数百 ns [不确定]) | TCPDirect 有 TCP 无 TLS;ef_vi 纯 L2 | 否 | 否 | 仅自建协议栈时 |
| **NVIDIA XLIO(VMA 继任)+ ConnectX-6 Dx/7/8** | 未取得一手 µs 数字 [不确定:VMA 时代 UDP ~1.1–1.3 µs、TCP ~1.5–2 µs 半 RTT] | LD_PRELOAD 透明;**uTLS:TX 硬件卸载默认开,RX 经 kTLS API** | **README 明确支持 x86_64 与 Arm** | 否 | Phase 2 备选(尤其若坚持 ARM) |
| **Intel E810 ADQ** | 内核内队列隔离(tc mqprio + 应用专用队列 + busy poll),不是 bypass;Intel 页面从本机不可达 [不确定] | 原生 | 是 | 否 | 裸金属上的"温和方案" |
| **Exablaze/Cisco ExaNIC X25/X10(FPGA)** | K35-S "568 ns trigger-to-response"(lowlatencysystem.com 2026 引用);exablaze.com 不可达 | exasock 有 TCP 加速,无 TLS | 否 | 否 | 产品线维护模式,不选 |

### 1.3 Linux 内核 2020–2026 的低延迟机制(ENA 上都能用)

- **`SO_BUSY_POLL` / `net.core.busy_read`(3.11 起):** 机会式——只在 NAPI 未被 softirq 占用时轮询。ENA 最佳实践建议 `busy_read=70 busy_poll=70`,应用绑到处理该队列 IRQ 的 vCPU,`ethtool -C adaptive-rx off rx-usecs 0 tx-usecs 0`。Cloudflare 数据:`SO_BUSY_POLL` 省 7 µs,用户态自旋再省 4 µs。
- **`SO_PREFER_BUSY_POLL` + `napi_defer_hard_irqs` + `gro_flush_timeout`(5.11,Björn Töpel 2020-11):** softirq 让位给用户态轮询;补丁集数据:单核 UDP ping-pong 155K → 420K pps、尾部收敛;补丁触及 `drivers/net/ethernet/amazon/ena/ena_netdev.c`。
- **`EPIOCSPARAMS` / `EPIOCGPARAMS`(6.9,glibc 2.40):** 每个 epoll 实例独立设 `busy_poll_usecs / busy_poll_budget(≤64 除非 CAP_NET_ADMIN)/ prefer_busy_poll`。要求同一 epoll 中的 fd 属于同一 NAPI(`SO_INCOMING_NAPI_ID` 校验)。**对我们:** ENA 无 ntuple,不能按 5 元组定向到队列 → 要么 `ethtool -L combined 1` 单队列(所有连接一个 NAPI、一个 epoll、一个核),要么按 RSS 落点把连接分配到对应核。
- **IRQ suspension(6.13,`irq-suspend-timeout`,2024-11 合入):** epoll 持续取到事件期间彻底屏蔽 IRQ 与 softirq,取不到时退回 defer/timer;memcached 基准(EPYC 7662,CX-5 Ex,8 RX 队列,200K qps):base avg/p99 112/416 µs @26% CPU;defer10 54/142 @29%;fullbusy 46/133 @100%;napibusy 100/277 @56%;**suspend10 53/141 @32%**;1000K qps 时 base 崩到 p99 6.2 ms 而 suspend10 仍 220 µs。推荐 `gro_flush_timeout` 尽量小(10–20 µs),`irq_suspend_timeout` 覆盖一个批次(如 20 ms)。
- **Threaded NAPI busy poll(6.17-rc,Google Samiullah Khawaja,2025-08 v7):** 内核线程持续轮询 NAPI 并填 AF_XDP/ socket 接收环,应用只读;xdp_rr(32 Kpps + 30 µs 应用处理):中断 p50/p99 21.1/21.2 µs,`SO_BUSYPOLL` 内联 17.0/17.1,独立核用户态轮询 13.7/14.0,**内核线程轮询 13.0/13.0 µs**。用一个核换掉 bypass 的大部分收益,且 p5–p99 完全平坦。
- **ENA 具体坑(amzn-drivers #312,2024-07,c7i.2xlarge,io_uring NAPI busy poll):** 轮询开着 ENA 仍产生大量中断(每次 20–50 µs 抖动);`ethtool -L combined 1` 后 "Command parameter 46 is not supported",部分 socket 收不到数据,只有 `combined 4` 正常。AWS AF_XDP 转发器的经验:`gro_flush_timeout=10 µs`(不能 0——会让包滞留在 busy-poll 间隙形成秒级突发;也不能 200 µs——会变成主要投递路径,每跳 +200 µs),`napi_defer_hard_irqs=2`,IRQ 钉到第一个隔离核,应用钉到隔离核+1,irqbalance 关。**这些必须在目标实例上逐项复测。**

### 1.4 AF_XDP / DPDK / Onload 在 ENA 上的真实状态(2026-09)

- **ENA 驱动能力时间线(GitHub RELEASENOTES):** 2.10.0 激活 PHC;2.12.0 AF_XDP 代码 feature 化;**2.13.0 重新启用 AF_XDP zero-copy**;2.14.0 XDP multi-buffer / metadata;**2.15.0 HW 包时间戳 + 扩展 cdesc**;2.16.0 PHC 经 devlink 启用;**2.17.0 XDP hints + AF_XDP metadata、AF_XDP 队列上的 skb HW 时间戳、busy poll 计数器**;当前 2.17.3。发行版 in-tree 驱动**不含** zero-copy(#337 报告人在 Ubuntu 24.04 kernel 6.17/7.0 上仍需 DKMS)。
- **Onload on ENA 三条证据:** #62(2022-02,Amazon 工程师用 beta ENA 驱动,同样的 `FIXME AF_XDP`);#139(2023-04,m5zn.12xlarge:`mtu 3000` + `combined 1` 后可跑,同 placement group 两机 UDP ping-pong 10 万样本 ×2:内核 mean/median/max 43.6/42.7/137 µs,Onload-AF_XDP 50.4/47.9/241 µs;iperf 6.96 → 7.24 Gbps);#337(2026-08,c8a.2xlarge Nitro v6 与 c6a.xlarge Nitro v4,ENA 2.17.2 DKMS,Onload master 3c434f29:注册成功但任何 socket 绑定即 `netif_tcp_helper_alloc_u: Failed to allocate stack (rc=-95)`,`af_xdp_flush_{rx,tx}_dma_channel: FIXME AF_XDP`,静默回退内核栈;`combined 1` 亦无效)。Onload README 自述:"The AF_XDP support is a community-supported work in progress that is not currently at release quality",原生 ef_vi 路径只支持 SFN8xxx / X2522 / X2541 / X3522(X4 在发行版中)。
- **AWS 官方样例(`networking_benchmarks/open_onload`)的代价:** 打补丁的 ENA(`ena-onload-patches.tar`)+ 打补丁的 Onload(commit 2d4ec08aa)、Nitro v5+(i7ie.* / m8i.* / r8i.*)、两块 ENI、`EF_USE_HUGE_PAGES=0 EF_RX_TIMESTAMPING=1 EF_AF_XDP_ZEROCOPY=1`;已知限制 "PTP hardware timestamping does not work with OpenOnload in EF_AF_XDP_ZEROCOPY=1 mode",延迟一栏为"—"(未给数字)。任务描述里"Onload 加了 ENA 驱动"应理解为 **AWS 提供的补丁集**,不是 Onload 上游的原生 ENA 后端 [据仓库 README 判断]。
- **DPDK on ENA:** PMD 成熟(LLQ、Rx HW 时间戳、Rx 中断、多队列、aarch64);AWS mcast2ucast 用 DPDK 25.11 + igb_uio(`wc_activate=1`)在 m8a.metal 得到 25.6 µs RTT,只比内核 UDP 多 5 µs(含 TAP 往返)。但 DPDK 只交付 L2 帧,TCP+TLS+WS 全要自带(§2)。

### 1.5 裸金属 NIC 的公开数字

- **AMD Solarflare X4(X4522 双 SFP / X4542 双 QSFP,PCIe Gen5 x8,定制 ASIC,CTPIO,2025-10):** AMD 内测(EPYC 9575F 5 GHz,Onload RC,0–648 B):X3522 平均比 X4 高 27.2%(4 B:34.9%),X2522 高 42.9%(4 B:55.5%);同一 X4 在 EPYC 4564P 上 4 B 延迟比 9575F 低 54 ns,比 Xeon 6534 低 77 ns(意味着 X4 4 B Onload 半 RTT 在 ~0.5–0.7 µs 量级 [估算,不确定])。官方措辞 "sub-microsecond"。
- **X2522(PCIe 3.1 x8,10/25G):** 产品页 "sub-microsecond hardware latency",支持 TCPDirect/ef_vi、PTP 单数纳秒时间戳。历史公开值 Onload TCP 半 RTT ~1.0–1.2 µs、ef_vi ~0.7 µs [记忆,不确定]。
- **X3522:** 页面为 JS 渲染,未取得数字;由 X4 相对值反推 Onload 半 RTT ~0.8–0.9 µs [估算]。
- **Cisco Nexus SmartNIC(ex-ExaNIC)K35-S:** 568 ns trigger-to-response(lowlatencysystem.com 引用);Cisco 数据表从本机不可达。
- **NVIDIA ConnectX-7 + XLIO:** 文档站为 Confluence JS 渲染,未取得数字;仓库 2026-09-23 仍活跃。

---

## 2. 能承载 TLS+WebSocket 的用户态 TCP 栈

| 栈 | 语言/许可 | TCP 成熟度 | 出站客户端 TLS | ARM64 | 判断 |
|---|---|---|---|---|---|
| **Onload**(AMD Solarflare) | C,GPL-2 + BSD-2 | 20 年生产级;完整 POSIX 语义(fork/exec/UDS 传 fd、应用未调度时协议照常推进) | **任何 TLS 库不改**(走 socket API) | 无 aarch64 [高度可能] | 仅 Solarflare NIC 上有意义 |
| **XLIO**(NVIDIA) | C++,BSD | 生产级(VMA 继任),需 DOCA-Host + DPCP | LD_PRELOAD 透明;uTLS TX 硬件卸载默认开、RX 经 kTLS API | **x86_64 + Arm** | 仅 ConnectX 上;要求"每线程独立 epoll、不跨线程共享 socket"才能拿到最佳延迟 |
| **F-Stack**(腾讯) | C,BSD | 移植 FreeBSD 11 栈到 DPDK,腾讯 DNSPod/CDN 生产;提供 epoll/kqueue 接口,集成 Nginx(含 OpenSSL)、Redis | 服务端 TLS 有先例;客户端 TLS 需自己接 | 支持(随 DPDK) | 重,协程/进程模型,与 Rust 集成难 |
| **Seastar native stack** | C++,Apache-2 | ScyllaDB 生产主要用 POSIX 栈,native 栈少用 [不确定] | 无 | 支持 | 不选 |
| **mTCP / TLDK** | C | 学术/停滞 | 否 | 否/部分 | 不选 |
| **smoltcp** 0.14.0(2026-08) | Rust,0BSD | 面向裸机/实时:无堆分配、窗口缩放有、CUBIC/Reno 可选、零窗口探测有;**无 TCP 时间戳、无 SWS 避免、无 PLPMTUD**;README 自述 "missing many widely deployed features";回环 ~Gbps | 可叠 rustls(`Read+Write` 适配) | 是 | **唯一纯 Rust 选项**,但 TCP 实现薄,面向公网长连接有风险 |
| **lwIP** | C,BSD | 嵌入式级 | mbedTLS altcp | 是 | 不选 |

**判断:** 除 Onload/XLIO 这类"透明 socket 加速"外,没有一个用户态栈能让我们零改动地跑 rustls + tungstenite 出站长连接。因此 **"bypass = 换 NIC + Onload/XLIO"**,而不是"在 ENA 上自己拼栈"——这也是 2026-09-19 结论的根因,只是现在有了 #337 与 AWS 样例的具体证据。

---

## 3. TLS

### 3.1 每记录成本(由吞吐推算)

| 平台 | 库 | AES-128-GCM 接收吞吐 | 2 KB 记录 | 200 B 记录 | 来源 |
|---|---|---|---|---|---|
| x86 Xeon E-2386G(AVX-512,2025-07) | rustls 0.23.31 + aws-lc-rs 1.13 | 7 953 MB/s | ≈ 0.26 µs | ≈ 0.03 µs | rustls.dev 2025-07-31 |
| 同上 | OpenSSL 3.5.1 | 7 220 MB/s | ≈ 0.28 µs | — | 同上 |
| 同上 | BoringSSL 2025-07 | 6 987 MB/s | ≈ 0.29 µs | — | 同上 |
| ARM64 Ampere Altra Q80-30(2024-10) | rustls 0.23.15 | 2 173 MB/s | ≈ 0.95 µs | ≈ 0.09 µs | rustls.dev 2024-10-31 |
| 同上 | OpenSSL 3.3.2 | 2 345 MB/s | ≈ 0.87 µs | — | 同上 |
| 2026-03 x86 更新 | rustls 0.23.37 vs OpenSSL 3.6.1 | rustls 7 333 MB/s 接收,OpenSSL 慢 1.18×,BoringSSL 慢 1.18× | — | — | rustls.dev/perf 首页 |

以上是 16 KB 记录的批量吞吐;小记录另加每记录固定开销(记录头解析、nonce、tag 校验、两次函数调用)≈ 0.1–0.3 µs [估算]。**结论:一条 200 B–2 KB 的行情记录解密在 0.2–1.2 µs 之间,库之间差 ≤ 20%,不构成选型依据。** Graviton3/4 的 AES/PMULL 单核吞吐应高于 Altra Q80-30(3.0 GHz N1)[不确定,需在 c8g 上跑 `cargo bench`]。

### 3.2 库选择与 ARM

- **rustls + aws-lc-rs**(04 §4.2 已定)。aws-lc 对 x86 与 Arm 都有汇编优化,并明确"用 Graviton 测试 ARMv8 优化"。s2n-tls 也以 aws-lc 为 libcrypto、支持 aarch64,但它的 C API 与 Rust 集成不如 rustls 顺手,且对解析错误有 10–30 s 随机延迟盲化(安全特性,和低延迟无关但要知道)。
- 全握手 CPU:rustls TLS 1.3 客户端 RSA 4 187/s(x86)≈ 0.24 ms、3 987/s(ARM);恢复握手 5 614/s ≈ 0.18 ms。**我们实测的 30 ms 握手 = 网络 RTT × 2 + 服务端排队 + PQ(X25519MLKEM768 已是 rustls/OpenSSL 默认)ClientHello 变大**,CPU 侧不到 2%。
- **策略:每个交易所维护 N+1 条已握手、已 Upgrade 的热备连接**,切换代替重连;TLS 1.3 PSK 恢复只能省一个 RTT 的加密协商,0-RTT 对 WebSocket Upgrade 无用(仍要等 101)。

### 3.3 kTLS 与网卡 TLS 卸载

- **kTLS(`TLS_TX`/`TLS_RX` ULP):** 内核文档:收侧"必须收到完整 TLS 记录才解密",大缓冲直接解到用户缓冲;设计目标是 `sendfile` 与大吞吐,Facebook 最初数据(LWN 2015)是"比用户态 TLS 快 2–7%"(吞吐)。对每条几百字节、需要用户态自旋控制的行情连接,kTLS 多一次 `recv` 系统调用往返、失去 rustls 侧的可观测性;**不用**(04 结论不变)。rustls/ktls crate 6.0.2(2025-04)存在,面向 tokio-rustls。
- **网卡 inline TLS(`TLS_HW`,ConnectX-6 Dx/7,Chelsio):** 内核 tls-offload 文档:驱动按 socket 标记包,设备加解密;RX 侧要求按序、依赖 resync 机制,乱序会回退软件。只在裸金属 + ConnectX 上存在;XLIO 的 uTLS 就是这条路(TX 硬件、RX 经 kTLS)。对小记录延迟收益不明确 [不确定],主要是 CPU 卸载;Phase 2 若选 XLIO 可顺带评估,Phase 1 无关。

---

## 4. WebSocket 帧 + JSON

### 4.1 帧处理与压缩
- 服务端→客户端帧**不掩码**,解帧只是读 2–10 字节头 + 长度;零拷贝 = 在 rustls 明文缓冲上原地切片。tungstenite 同步 API 可跑在非阻塞 socket 上(04 §4.1)。
- **permessage-deflate(RFC 7692):** uWebSockets(自述"每天在多家比特币交易所上承载数十亿美元",很可能就是我们对手方的服务端实现)作者的量化:开压缩后"只剩 20–30% 的 I/O 性能",而 TLS 1.3 仍保留 ~80%;shared compressor 是每帧独立流、无滑动窗口。客户端侧 inflate 一条 1–2 KB 帧约数 µs 到十几 µs [估算],且引入上下文状态。**能关就关;Binance Spot/合约 JSON 流文档中根本没有 deflate 选项;其他交易所逐家核对(OKX/Gate/KuCoin/Bitget 文档站从本机不可达,未能核对)。**
- Binance JSON 流约束:`wss://stream.binance.com:9443|443`(公共数据也可走 `data-stream.binance.vision`),连接 24 h 强制断、服务端每 20 s ping、每连接 ≤1024 流、客户端每秒 ≤5 条上行、每 IP 每 5 分钟 ≤300 次连接;`timeUnit=MICROSECOND` 得 µs 时间戳。Bybit:建议每 20 s 客户端 ping,每 5 分钟 ≤500 连接/域名。**24 h 强断 + 300 连接/5 min 决定了热备连接池必须错峰轮换。**

### 4.2 JSON 解析成本

| 解析器 | 平台 | 数据 | 吞吐 | 来源 |
|---|---|---|---|---|
| sonic-rs `from_slice`(结构体) | x86_64(README,CPU 未注明) | twitter.json 632 KB:0.83 ms;citm 1.7 MB:1.37 ms;canada 2.25 MB:4.0 ms | 0.56–1.24 GB/s | sonic-rs README |
| simd-json `from_slice` | 同上 | 1.09 / 2.10 / 8.09 ms | 0.28–0.81 GB/s | 同上 |
| serde_json `from_str` | 同上 | 1.38 / 2.61 / 9.26 ms | 0.24–0.65 GB/s | 同上 |
| sonic-rs `from_slice`(结构体) | **Apple M1 Pro(aarch64)** | 0.459 / 0.898 / 3.14 ms | 0.72–1.9 GB/s | sonic-rs docs/benchmark_aarch64.md |
| simd-json | M1 Pro | 0.425 / 0.837 / 3.23 ms | ≈ 同上 | 同上 |
| sonic-rs DOM | M1 Pro | twitter 0.381 ms | 1.66 GB/s | 同上 |
| sonic-rs `get_unchecked_from_str`(惰性取一个字段) | M1 Pro | twitter 51 µs | 12 GB/s(跳过) | 同上 |
| yyjson / simdjson DOM | i5-8259U | twitter parse 1.72–1.80 / 1.52 GB/s | — | yyjson README |
| yyjson / simdjson DOM | AWS c5a.large(Zen 2) | 2.39–3.51 / 2.19 GB/s | — | yyjson README |
| simdjson 自述 | — | 比 RapidJSON 快 4×;minify 6 GB/s;UTF-8 校验 13 GB/s;NDJSON 3.5 GB/s | — | simdjson README |

**换算到我们的消息:** 以 0.8–1.5 GB/s 计,300 B bookTicker ≈ 0.2–0.4 µs,2 KB depth ≈ 1.3–2.5 µs,再加每次调用固定开销(几十到一两百 ns)。**注意 aarch64 上 sonic-rs 与 simd-json 并列,x86 上 sonic-rs 领先**;要求 `-C target-cpu=native`。策略见 04 §2.7(专用 `Deserialize` + `&str` 借用 + 字符串保留数字 + 惰性分派)。simdjson 文档提醒:浮点数密集的文档解析速度会掉到"几百 MB/s"——所以**价格/数量字段保持字符串,自写定点解析**,这是一半的收益。

### 4.3 二进制 / SBE / FIX(Binance Spot)
- **SBE 行情流:** `stream-sbe.binance.com:9443`,需 Ed25519 API key(`X-MBX-APIKEY` 头,无需签名与额外权限),所有时间戳 **µs**;`<symbol>@trade`、`<symbol>@bestBidAsk`(= bookTicker + `eventTime`,**auto-culling**:高负载时丢弃过期事件而不是排队晚发)、`<symbol>@depth`(**20 ms**,JSON 是 100 ms/1000 ms)、`<symbol>@depth20`(50 ms);订阅控制仍是 JSON 文本帧,行情为二进制帧;24 h 强断、20 s ping、≤1024 流、≤5 req/s。
- **FIX:** `fix-oe`(下单,10 000 msg/10 s)、`fix-dc`(Drop Copy,延迟 1 s)、`fix-md`(行情:trade、bookTicker、diff depth 100 ms、depth 2–5000 档)均为 `tcp+tls://…:9000`,必须发 SNI,仅 Ed25519;`MessageHandling=UNORDERED` 允许撮合前重排;另有 **FIX SBE** 编码变体。REST/WS API 亦可 `Accept: application/sbe` / `responseFormat=sbe`。
- **实测结论(NeutK/binance_feed_test):** Spot SBE 的 µs `eventTime` 好用,但**到达不早于 JSON**,故该项目放弃 SBE 做排名。→ SBE/FIX 的收益是:解析 0.2–2 µs → 几十 ns、时间戳精度 ms → µs、diff depth 20 ms 频率、auto-culling 少排队。**Phase 1 应对 Binance Spot 同时接 SBE 与 JSON 做几小时对照。** USDT-M 合约无 SBE/FIX(记忆笔记 2026-09);Bitget 有公共 SBE(books1 @1 ms)。

---

## 5. AWS 特有约束

### 5.1 ENA / ENA Express / EFA
- **ENA LLQ("push mode"):** Tx 描述符 + 包头直接写入设备 BAR2 的 write-combining 区,"saves several more microseconds";Nitro v4 起 large LLQ(224 B 头)默认开、Tx ring 降到 512。ENAv3(Nitro v4)多一个 BAR1。
- **中断:** 每队列 1 IRQ(Tx/Rx 共享),Tx 静态 64 µs 合并,Rx 视实例默认 adaptive(DIM);低延迟必须 `ethtool -C adaptive-rx off rx-usecs 0 tx-usecs 0`,且要在 `ethtool -L` 之后(`-L` 会重置 adaptive-rx)。我们的 c7g.large 现状是 adaptive-rx on、rx-usecs 20、tx-usecs 64——**这一项就是 20 µs 级的免费收益**。
- **队列数** = min(每 ENI 上限,vCPU 数),上限多为 8、网络优化型 32;RSS 哈希/键可配(Nitro v3 网络优化型与 Nitro v4 起),但 **ntuple 过滤器所有世代 `off [fixed]`**。
- **ENA Express(SRD):** 仅同 Region 实例间、双端开启、无中间盒;单流 5 → 25 Gbps;降低拥塞时尾延迟,但文档明写"轻载时中位数增加数十微秒",AWS t2t Part 2 明写 "for HFT workloads ENA Express is rarely the right choice"。支持列表全是 ≥4xlarge/8xlarge 与 .metal。与交易所公网连接无关。
- **EFA:** HPC/ML 内网协议,不出 VPC。无关。

### 5.2 Nitro 裸金属(.metal)
- 与最大虚拟机同硬件、去掉 Nitro hypervisor;HPC 基准差 <1%,但网络尾延迟 p99.9 好 15–29%(表:m8azn 17.7/19.4 vs 18.9/22.7 µs;m5zn 20.3/22.2 vs 23.6/31.0;c7i.24xl 20.3/22.0 vs 21.7/30.9)。
- .metal 才有完整 P-state/C-state 控制;AWS 建议 Ubuntu 6.12+,SMT 关、`isolcpus/nohz_full/rcu_nocbs/irqaffinity`、`skew_tick=1`、`idle=poll` 可选、irqbalance 关、`gro/tso/gso off`、hugepages、THP 在 x86 关 / Graviton 用 `madvise`、NUMA balancing 关。OS 调优对 c7i/m5zn 中位数改善 28–36%,对 m8azn/c8g 中位数几乎无变化(已接近地板)但 max 降 27%。
- Tokyo 可选:**m8azn.metal-12xl / metal-24xl**(EPYC Turin 5 GHz,Nitro 第六代卡,2026-02 GA 含 Tokyo,AWS 明确面向 HFT)、**c8g.metal-24xl/48xl**(Graviton4,分布最紧 21.4/23.4 µs)、c7i.metal-24xl、r7iz.metal-16xl。价格未在本次抓取 [待查 vantage]。

### 5.3 时间与硬件时间戳
- Amazon Time Sync:NTP 误差界 <100 µs;PHC(`/dev/ptp*`,ENA 驱动 ≥2.10 且 `ENA_PHC_INCLUDE=1` 编译、`phc_enable=1`)误差界 <40 µs,chrony 对 PHC 的 RMS 12 ns 级,`phc_error_bound` sysfs 给出设备侧误差;AWS 实测 m7i.4xlarge:ClockBound 守护 ±63–78 µs,chrony+PHC 公式 **±25–34 µs**;m5.4xlarge NTP ±1.39–1.45 ms。binance_feed_test 在 c6in/c7gn/c7i 上看到系统偏移 0.5–6 µs、RMS 3–30 µs。
- **HW RX 时间戳(2025-06 起):** 每个入站包 64 位纳秒时间戳,`SO_TIMESTAMPING` ancillary `ts[2]`,标准 API;虚拟机与裸金属都行,免费;`ethtool -T` 显示 `hardware-receive`(**仅接收方向**,发送只有软件时间戳);需 GitHub ENA ≥2.15;ENA 2.17 起 XDP 程序可用 `bpf_xdp_metadata_rx_timestamp` kfunc 读到同一时间戳。回环测试没有 HW 时间戳。
- **实例范围:** 传统路径 M7a/M7g/M7i、R7a/R7g/R7i、I8g/I8ge;**Precision Time Placement Group(2026-06)** 把 PHC + HW 时间戳扩到 Gen7 及以后家族(AWS 样例守卫列表:m7*/c7*/r7*/c8*/m8*/r8*/x8*),可与 CPG 关联,免费。**我们的 c7g 在列。**
- **用途:** 这是把 tick-to-trade 拆成"交易所 → 网卡"(需交易所 µs 时间戳:Binance `timeUnit=MICROSECOND` 或 SBE)、"网卡 → 应用"(ts[2] → 用户态 `clock_gettime`)、"应用处理"、"`send()` → 线上"(只能用对端 ack 或交易所回报时间估计)的唯一锚点。AWS 原话:"Large gaps between hardware and kernel timestamps indicate contention or queuing in the network stack."

### 5.4 同 AZ / 跨 AZ / 到交易所的 RTT
| 场景 | RTT | 来源 |
|---|---|---|
| 同 AZ + CPG,裸金属,内核 UDP 64 B | **p50 20.7 µs / p99 24.1 µs**(m8a.metal-24xl) | AWS networking_benchmarks/mcast2ucast |
| 同 AZ + CPG,c7i 虚拟机,AF_XDP zero-copy | p50 32 / p99 37 µs | AWS networking_benchmarks/af_xdp |
| 同 AZ 非 CPG,m8a.2xlarge | 33.7 / 36.4 µs | AWS mcast2ucast |
| 同 AZ,CPG vs 非 CPG(c5n netperf TCP RR,2023) | CPG 内 p50 −35%、p90 −37%;PPS +56% | AWS shared CPG 博客 |
| 交易所接入分层(One Trading,c6id.metal,2023) | CPG+VPC peering **50–200 µs**;PrivateLink 中;CloudFront >1 ms;高频率下 CPG+peering p99 比 CloudFront 快 98%、比无 CPG peering 快 41% | AWS One Trading 博客 |
| TGW 多播单程 | 150–200 µs | AWS networking_benchmarks/transit-gateway |
| **Tokyo 跨 AZ** | **1a↔1c 1.87 ms,1a↔1d 1.54 ms**(账号私有名称;全球最慢 Region 之一,仅次于 São Paulo/UAE) | bitsand.cloud 2023-10 |
| 其他 Region 跨 AZ | 0.25–1.2 ms(最快 apne3 0.29–0.39 ms) | flashgrid 2023 / bitsand 2023 |
| Binance 行情 `E` → Tokyo 网卡 | p50 3.8–4.6 ms,p99 11–13 ms(az1/az2/az4 相近) | Deltix 2023-06 |

**对我们数字的解读:** Gate 0.27 ms ≈ 同 AZ 但经 ELB / 不同 spine;Binance 1.0–1.5 ms ≈ **跨 AZ**(Tokyo 跨 AZ 1.5–1.9 ms 的下沿);Bitget/KuCoin/OKX 1.1–1.8 ms ≈ CDN 边缘。记忆笔记里的 apne1-az4 假设(KuCoin 官方 1a=az4;2025-04-15 az4 故障拖垮 Binance)与此相容。**Phase 1 第一件事仍是 EC2 hunting:在 apne1-az1/az2/az4 各起实例(每个各自 CPG),对每个交易所的 WS/REST/下单端点测 TCP 握手 RTT 分布,按交易所分实例。** 注意 Deltix 数据说明 Binance 的**行情**到达时间对 AZ 不敏感(瓶颈在其内部),AZ 主要改善的是**下单 RTT 与我们相对其他参与者的到达先后**。

---

## 6. Rust 生态(与 04 互补,只列网络数据路径相关)

| 需求 | crate / 做法 | 状态(crates.io,2026-09-29) | 备注 |
|---|---|---|---|
| 热路径 I/O | 无运行时:绑核线程 + 非阻塞 socket 自旋(04 §2.4) | — | io_uring 家族(monoio 0.2.4 停在 2024-08;compio 0.19.2 2026-08)对少连接无收益 |
| `SO_BUSY_POLL` | `socket2::Socket::set_busy_poll` | socket2 0.6.5(2026-07) | mio 1.2.3(2026-09)不暴露;在 `mio::Poll` 注册前用 socket2 设 |
| `SO_PREFER_BUSY_POLL` / `SO_BUSY_POLL_BUDGET` / `SO_INCOMING_NAPI_ID` | `libc::setsockopt` / `getsockopt` 直接调 | libc 常量已存在 | socket2 未封装 |
| `EPIOCSPARAMS` / netlink `napi-set`(defer-hard-irqs、gro-flush-timeout、irq-suspend-timeout、threaded) | `libc::ioctl(epfd, EPIOCSPARAMS, &epoll_params)`;netlink 用 `ynl` 脚本或 `neli`/`netlink-packet-generic` 在启动脚本里做 | — | `mio::Poll::as_raw_fd()` 取 epfd;或干脆自旋不用 epoll |
| AF_XDP | xsk-rs 0.11.0(2026-08-28,自 2020 活跃);xdpilone 1.3.0(2026-04-30,不依赖 libbpf/libxdp);afxdp 0.4.0(2021,停) | 可用但 Phase 1 不用 | 只配 UDP/多播场景 |
| DPDK | dpdk-sys 0.1.5(2018)、capsule 0.1.5(2021)均停滞 | 不可用 | 若 Phase 2 真要 DPDK,只能 bindgen 自绑或 C 侧写 |
| Onload | `onload ./engine` 或 `LD_PRELOAD=libonload.so`;`EF_POLL_USEC`、`EF_TCP_RECV_SPIN`、`EF_UL_EPOLL` 等环境变量控制自旋 [参数名据 Onload 用户指南记忆] | — | **必须动态链接 glibc**(Rust 默认 `*-unknown-linux-gnu` 即可;musl 静态链接会绕过 LD_PRELOAD);Onload 自带 `onload_tcpdump`,普通 tcpdump 看不到加速流量 |
| XLIO | 同上 LD_PRELOAD;需 DOCA-Host | — | 要求每线程独立 epoll、不跨线程共享 socket |
| kTLS | rustls/ktls 6.0.2(2025-04) | 不用 | 见 §3.3 |
| 用户态 TCP | smoltcp 0.14.0(2026-08) | 不用 | 见 §2 |
| HW 时间戳 | `SO_TIMESTAMPING` + `recvmsg` cmsg 解析(`libc::scm_timestamping` / `SO_TIMESTAMPING_NEW`) | 自写 ~100 行 | 与 hdrhistogram 配合(04 §2.5) |
| WebSocket / TLS / JSON | tungstenite 0.30 同步 / rustls 0.23 + aws-lc-rs / sonic-rs 0.5 | 04 已定 | 本文数字支持这些选择 |

---

## 7. 头部加密 HFT 公开说法与云上可达指标

- **AWS(t2t Part 1,2025-07):** "HFT firms operating in digital asset markets strive for **low double-digit microsecond** tick-to-trade";HFT 偏好 FIX 而非 REST/WebSocket;CEX 很少提供二进制协议;避免 CDN/LB,直连 EC2 上的网关,最好 VPC peering 私网。**(t2t Part 2,2026-04):** 五层层级、裸金属数字、"EC2 hunting" 是官方推荐做法、Part 3 将讲 Direct Connect / 多播 / AF_XDP+DPDK(截至抓取未见发布)。
- **One Trading(AWS 博客,2023-12):** 交易所目标"round-trip under 200 µs",MM 侧 c6id.metal + Netty + io_uring + 绑核,四种接入分层(§5.4)。
- **Deltix/EPAM(2023–2024):** Binance 行情到 Tokyo 的 4 ms 中位数(§0.1);撮合引擎 Onload 前后 78 → 32 µs(§1.2)。
- **Nikhil Padala(ex-Akuna Capital crypto DevOps,ex-Gemini SRE):** "Same-AZ colocation discovery through mass benchmarking, keeping the lowest-jitter hosts as production";Gemini 用 Solarflare + PTP。
- **Hyperliquid 官方(Optimizing latency):** 读侧跑非验证节点(≥32 逻辑核、128 GB、500 MB/s 盘),`split_client_blocks: true` 可省 70–150 ms;写侧用 `fast` cancel、priority fees;端到端 ~380 ms 但排序噪声 <10 ms。**与 CEX 的 µs 竞争完全不同量级,链上腿的预算见 03。**
- **lowlatencysystem.com 2026 指南:** 三档——调优软件 + bypass ≈ 1–2 µs tick-to-trade(传统市场 UDP 多播 + 二进制协议)、FPGA 50–500 ns(记录 13.9 ns)、ASIC 10–50 ns。加密 CEX 因 TCP+TLS+JSON+云网络,**现实档位是主机内 20–100 µs + 网络 RTT/2。**
- **记忆笔记(2026-09):** 交易所侧低延迟线路——Binance FAPI-MM / FSTREAM-MM(流动性 MM)、Bitget LOLA、KuCoin MM 线;所有腿 2026 年都在 Tokyo(OKX 7 月底 HK → Tokyo,Bybit 2026-11-12 SG → AWS Tokyo)。
- **未能取得(本机不可达):** Kraken "Unlocked #5 ultra-low-latency trading"、hftadvisory "kernel bypass bottleneck trap"、Wintermute/GSR/Alphaticks/Tardis 博客、Binance Futures 低延迟 API 服务 FAQ、Jump Crypto(其公开写作全是 Firedancer/Solana,无 CEX 基础设施细节)。

**云上可达的 tick-to-trade(综合):** 在 AWS 上,调优内核 + 独占裸金属,**主机内(网卡收到行情 → 订单离开网卡)p50 20–60 µs、p99 < 150 µs** 是有据可依的目标(AWS 回环 17.7 µs 含两次穿栈;Deltix 未 bypass 的 Java 撮合 p50 78 µs);Onload 级 bypass 在裸金属上再到 10–30 µs。加上 fabric:同 CPG 下单 RTT ~30–50 µs,同 AZ ~40–70 µs,跨 AZ 1.5–1.9 ms。**我们当前 1.0–1.5 ms 的 Binance RTT 与 155 µs 的调度抖动,与"主机内 30 µs"之间差两个数量级,先补前者。**

---

## 8. 推荐决策

### Phase 1(AWS Tokyo;现有六家交易所全部在 ap-northeast-1 或经其边缘)
1. **测量先行(1 周):** GitHub ENA 2.17.3 DKMS + `phc_enable=1` + chrony `refclock PHC /dev/ptp0`;引擎记录每条消息的 ts[2]、用户态收到、解析完、决策完、`send()` 返回五个点进 hdrhistogram;Binance 行情开 `timeUnit=MICROSECOND`(或 SBE)拿交易所侧 µs 时间戳。目标是先拿到真实的四段分布,而不是猜。
2. **拓扑(2 周):** 在 apne1-az1/az2/az4 各起一台(各自 CPG),对每个交易所的 WS/REST/下单端点做 TCP 握手 RTT 分布 + NeutK 式行情到达竞速;按结果把交易所分到不同实例/AZ;公网 IP 直连,不经 NAT GW/TGW;向 Binance/Bitget/KuCoin 申请 MM 线路。
3. **实例:** 独占物理主机——**m8azn.metal-12xl**(5 GHz x86,绝对延迟最低,x86 SIMD JSON 全速,AWS 明确面向 HFT)或 **c8g.metal-24xl**(与现有 aarch64 代码零迁移,分布最紧)。预算不足时的过渡:同族 4xlarge 以上 + 隔离核,但要接受 hypervisor/邻居带来的 p99.9 差 15–29%。**2 vCPU 共享盒子的 155 µs 抖动无法靠软件消除。**
4. **内核(与 AWS OS Tuning Reference 一致):** Ubuntu 24.04/26.04 + 6.12+ 内核;`isolcpus/nohz_full/rcu_nocbs/irqaffinity`、`skew_tick=1`、SMT off(x86)、`max_cstate=1`(或 `idle=poll`)、`ethtool -L` 后再 `-C adaptive-rx off rx-usecs 0 tx-usecs 0`、`-K gro off tso off gso off`、`-G rx 4096`、irqbalance 关、`gro_flush_timeout=10000 napi_defer_hard_irqs=2`,每交易所连接组一个 NAPI/epoll/核,`EPIOCSPARAMS{prefer_busy_poll=1}` + `irq-suspend-timeout=20ms`;若核够多,直接专核自旋(或 6.17 threaded NAPI busy poll)。每一项用步骤 1 的直方图验证。
5. **应用:** 每交易所一个绑核线程,非阻塞 socket 自旋,rustls + aws-lc-rs,tungstenite 同步,专用 `Deserialize`(数字保留字符串 → 定点),零分配;N+1 热备已 Upgrade 连接、错峰轮换(24 h 强断、300 连接/5 min 限制);Binance Spot 走 SBE 行情 + FIX 下单(Ed25519)。
6. **不做:** DPDK、AF_XDP、Onload-on-ENA、ENA Express、kTLS、网卡 TLS 卸载、smoltcp。

**Phase 1 目标:** 主机内 tick-to-trade p50 < 30 µs、p99 < 100 µs;到 Binance 下单 RTT < 100 µs(同 AZ)或 < 60 µs(若能进共享 CPG)。

### Phase 2(裸金属 / 托管;触发条件:某交易所提供 AWS 之外的低延迟接入(FIX 专线、跨连接),或链上节点需要专机)
- 服务器:单路 EPYC 9575F(64C,5 GHz)或 EPYC 4564P(16C,5 GHz,AMD 数据显示 X4 在其上比 9575F 再低 54 ns)、SMT 关、NIC 与热路径核同 NUMA;Xeon 6 高频 SKU 为备选。
- NIC:**AMD Solarflare X4522 + Onload**(POSIX 透明、rustls 不改、`EF_POLL_USEC` 自旋、板载 PTP);备选 **ConnectX-7 + XLIO**(支持 Arm、uTLS 卸载)。ExaNIC 不选(维护模式)。
- 预期:NIC → 应用单程 < 1.5 µs,主机内 tick-to-trade 10–30 µs(Deltix Java 撮合 Onload 后 p50 32 µs 是保守参照),抖动降 5–6 倍;剩余全部是物理距离与交易所内部延迟。
- 前提检查:Onload 无 aarch64 → **Phase 2 默认 x86**;先在 AWS m8azn 上把 x86 构建与 SIMD 路径跑通,避免 Phase 2 时双重迁移。

### 未决问题(需实测)
- Graviton4 上 rustls/aws-lc-rs 单记录解密与 sonic-rs 吞吐(本文 ARM 数据来自 Altra 与 M1 Pro)。
- ENA 单队列 `combined 1` 在目标实例上是否复现 #312 的丢数据问题;`irq-suspend-timeout` 在 ENA 上的实际中断计数(ENA 2.17 有 busy poll 计数器可看)。
- Binance Spot SBE 与 JSON 在同一实例的到达时间差(NeutK 结论需在我们的 AZ 复核)。
- apne1-az4 假设;Bybit 11 月迁到 AWS Tokyo 后的 AZ。
- m8azn.metal-12xl / c8g.metal-24xl 在 Tokyo 的价格与容量(建议 On-Demand Capacity Reservation)。

---

## 9. 参考链接

**AWS 官方**
- Optimize tick-to-trade latency Part 1(2025-07):https://aws.amazon.com/blogs/web3/optimize-tick-to-trade-latency-for-digital-assets-exchanges-and-trading-platforms-on-aws/
- Part 2(2026-04,裸金属/OS 调优数字):https://aws.amazon.com/blogs/web3/optimize-tick-to-trade-latency-for-digital-assets-exchanges-and-trading-platforms-on-aws-part-2/
- aws-samples/trading-latency-benchmark(含 networking_benchmarks:open_onload / af_xdp / mcast2ucast / transit-gateway / clock_bound_measure):https://github.com/aws-samples/trading-latency-benchmark ;OS Tuning Reference:https://github.com/aws-samples/trading-latency-benchmark/blob/main/assets/blog/tick_to_trade_latency_part_2_os_tuning_reference.md ;networking_benchmarks README:https://github.com/aws-samples/trading-latency-benchmark/blob/main/networking_benchmarks/README.md
- Crypto market-making latency and shared CPGs(2023-01):https://aws.amazon.com/blogs/industries/crypto-market-making-latency-and-amazon-ec2-shared-placement-groups/
- One Trading cloud-native colocation(2023-12):https://aws.amazon.com/blogs/industries/one-trading-exchange-and-aws-cloud-native-colocation-for-crypto-trading/
- Bare metal performance with Nitro(2021):https://aws.amazon.com/blogs/hpc/bare-metal-performance-with-the-aws-nitro-system
- ENA Linux Best Practices:https://github.com/amzn/amzn-drivers/blob/master/kernel/linux/ena/ENA_Linux_Best_Practices.rst ;README(PHC/XDP/LLQ):https://github.com/amzn/amzn-drivers/blob/master/kernel/linux/ena/README.rst ;RELEASENOTES:https://github.com/amzn/amzn-drivers/blob/master/kernel/linux/ena/RELEASENOTES.md ;issue #312(busy poll / 单队列):https://github.com/amzn/amzn-drivers/issues/312
- ENA Express:https://docs.aws.amazon.com/AWSEC2/latest/UserGuide/ena-express.html
- Time Sync / PHC / HW timestamps 文档:https://docs.aws.amazon.com/AWSEC2/latest/UserGuide/configure-ec2-ntp.html ;HW 包时间戳公告(2025-06):https://aws.amazon.com/about-aws/whats-new/2025/06/amazon-time-sync-nanosecond-hardware-packet-timestamps/ ;Precision Time Placement Group(2026-06):https://aws.amazon.com/about-aws/whats-new/2026/06/ec2-time-sync-precision-time-placement-group/ ;微秒时钟博客(2023-11):https://aws.amazon.com/blogs/compute/its-about-time-microsecond-accurate-clocks-on-amazon-ec2-instances/
- m8azn GA(2026-02):https://aws.amazon.com/about-aws/whats-new/2026/02/aws-m8azn-instances-generally-available
- DPDK ENA PMD 文档:https://doc.dpdk.org/guides/nics/ena.html ;AF_XDP PMD:https://doc.dpdk.org/guides/nics/af_xdp.html

**内核机制**
- NAPI 文档(busy poll / IRQ suspension / threaded busy poll):https://docs.kernel.org/networking/napi.html
- EPIOCSPARAMS man page:https://manpages.ubuntu.com/manpages/stonking/man2/EPIOCGPARAMS.2const.html
- LWN,Introduce preferred busy-polling(2020-11):https://lwn.net/Articles/837010/ ;threaded napi busy poll v7(2025-08):https://lwn.net/Articles/1034942/
- IRQ suspension 合入(2024-11,含 memcached 基准表):https://github.com/torvalds/linux/commit/80b6f094756f ;patch 系列:https://lore.kernel.org/all/20241109050245.191288-1-jdamato@fastly.com/
- AF_XDP 文档:https://docs.kernel.org/networking/af_xdp.html ;XDP RX metadata:https://docs.kernel.org/networking/xdp-rx-metadata.html
- kTLS:https://docs.kernel.org/networking/tls.html ;NIC TLS offload:https://docs.kernel.org/networking/tls-offload.html ;LWN kTLS(2015):https://lwn.net/Articles/666509/
- Cloudflare,How to achieve low latency with 10Gbps Ethernet(2015,含 Onload 对照):https://blog.cloudflare.com/how-to-achieve-low-latency/
- Rigtorp low latency guide:https://rigtorp.se/low-latency-guide/

**Kernel bypass 产品与证据**
- OpenOnload README:https://github.com/Xilinx-CNS/onload ;issue #337(AF_XDP on ENA 失败,2026-08):https://github.com/Xilinx-CNS/onload/issues/337 ;#139(m5zn 上延迟变差,2023):https://github.com/Xilinx-CNS/onload/issues/139 ;#62(2022):https://github.com/Xilinx-CNS/onload/issues/62
- AMD Solarflare X4 博客(2025-10):https://www.amd.com/en/blogs/2025/new-amd-solarflare-x4-low-latency-ethernet-adapter-set.html ;X4 产品页:https://www.amd.com/en/products/ethernet-adapters/solarflare-x4.html ;X2522:https://www.amd.com/en/products/ethernet-adapters/x2522.html
- NVIDIA libxlio:https://github.com/Mellanox/libxlio ;文档:https://docs.nvidia.com/networking/category/xlio
- Deltix/EPAM,Running Ember with Onload:https://ember.deltixlab.com/docs/performance/running_ember_with_onload/ ;WebSocket Data Feed Latency(Binance Tokyo):https://ember.deltixlab.com/docs/performance/ws_market_data/
- lowlatencysystem.com 指南(2026-07):https://lowlatencysystem.com/guide/

**TLS / JSON / WebSocket**
- rustls 性能报告索引:https://rustls.dev/perf/ ;x86 2025-07:https://rustls.dev/perf/2025-07-31-report/ ;ARM64 2024-10:https://rustls.dev/perf/2024-10-31-arm64/ ;2026-Q1 更新:https://www.memorysafety.org/blog/26q1-rustls-performance/
- aws-lc:https://github.com/aws/aws-lc ;s2n-tls:https://github.com/aws/s2n-tls
- sonic-rs README 与 aarch64 基准:https://github.com/cloudwego/sonic-rs ;https://github.com/cloudwego/sonic-rs/blob/main/docs/benchmark_aarch64.md
- simdjson:https://github.com/simdjson/simdjson ;yyjson(含基准表):https://github.com/ibireme/yyjson
- uWebSockets READMORE(压缩/TLS 成本):https://github.com/uNetworking/uWebSockets/blob/master/misc/READMORE.md ;RFC 7692:https://www.rfc-editor.org/rfc/rfc7692
- Rust crates:xsk-rs https://crates.io/crates/xsk-rs ;xdpilone https://crates.io/crates/xdpilone ;socket2 https://docs.rs/socket2 ;ktls https://crates.io/crates/ktls ;smoltcp https://github.com/smoltcp-rs/smoltcp

**交易所协议**
- Binance Spot SBE 行情流:https://github.com/binance/binance-spot-api-docs/blob/master/sbe-market-data-streams.md ;SBE FAQ:https://github.com/binance/binance-spot-api-docs/blob/master/faqs/sbe_faq.md ;FIX API:https://github.com/binance/binance-spot-api-docs/blob/master/fix-api.md ;WebSocket 流:https://github.com/binance/binance-spot-api-docs/blob/master/web-socket-streams.md
- Bybit WS 连接文档:https://bybit-exchange.github.io/docs/v5/ws/connect
- Hyperliquid Optimizing latency:https://hyperliquid.gitbook.io/hyperliquid-docs/for-developers/api/optimizing-latency
- NeutK/binance_feed_test:https://github.com/NeutK/binance_feed_test

**AZ / 拓扑**
- bitsand.cloud,Measuring Latencies Between AWS AZs(2023-10,Tokyo 1.54–1.87 ms):https://www.bitsand.cloud/posts/cross-az-latencies
- flashgrid,inter-AZ latencies(2023):https://www.flashgrid.io/news/latencies-between-aws-availability-zones-what-are-they-and-how-to-minimize-them/
- arbitron 交易所位置:https://arbitron.app/learn/crypto-exchange-server-locations
- Nikhil Padala:https://nikhilpadala.com/
