# 05 · 硬件、托管、OS/内核调优与时间同步(2026-09-29)

> 范围:为加密低延迟交易系统(跨所套利 + CEX/DEX 套利)的从零重构回答四个问题——**放在哪(AWS AZ / .metal / 裸金属云 / 东京 colo)、跑在什么 CPU/NIC 上、Linux 怎么调、时间怎么对**。目标函数是 tick-to-trade 的 p50 / p99,老板愿意为可证明的收益升级硬件或托管。
> 现状:AWS Tokyo `c7g.large`(Graviton3,2 vCPU 共享宿主机,ENA 2 队列,irqbalance 开,adaptive-rx 开,无 hugepages,kernel 7.0.0-aws,Ubuntu),AZ `apne1-az2`。实测:`sleep(0)` p99 155 µs;100 µs 定时器超调 p50 54 µs / p99 193 µs;`ss -ti` minRTT:Gate 0.27 ms、Binance 1.0–1.5 ms、OKX 1.4–1.8 ms;Binance 撮合被认为在 `apne1-az4`。
> 口径:优先 2025–2026 来源;厂商自测标 **[厂商数据]**,推断标 **[推断]**,未能核实标 **[待核实]**,粗算标 **[估算]**。AWS 价格取自 2026-09-25 发布的 ap-northeast-1 官方价目 CSV(Linux/Shared/按需,USD,月 = 730 h)。本文不重复 01(网络/内核旁路)与 02(交易所连接)的内容,只在必要处交叉引用。本次环境限制:搜索引擎与 Binance/OKX/Bybit 站点不可达,相关交易所公告未能核对。

---

## 0. 结论摘要(Top 10)

| # | 结论 | 关键数字 / 依据 |
|---|---|---|
| 1 | **先搬 AZ,再谈一切。** AWS 自己的五层延迟层级:区域/AZ 放置是毫秒级,实例与网络路径决定尾部,OS 调优只有微秒级。AWS 用 c7i.4xlarge + AF_XDP 实测(eu-central-1):同宿主机 RTT p50 22 µs、同 AZ+CPG 40 µs、同 AZ 不同 PG 44 µs、**跨 AZ 173 µs(max 343 µs)**、跨区域 11.8 ms;第三方 ping 的跨 AZ 数据为 0.3–1.2 ms。我们到 Binance 的 1.0–1.5 ms 明显大于纯 AZ 距离,说明"跨 AZ + 一层 LB"叠加——EC2 hunting 后才知道能省多少。 | AWS trading-latency-benchmark `topology_latency.md`;FlashGrid 2023 |
| 2 | **`c7g.large` 这类 2 vCPU 共享实例是结构性抖动源**:vCPU 一空闲就交还 Nitro hypervisor,唤醒靠中断注入;Graviton 无 C/P-state 控制。100 µs 定时器 p50 超调 54 µs ≈ Linux 默认 `timer_slack_ns = 50 µs`;AWS `phc_probe` 在未调优实例上看到 NIC 硬件戳与内核软件戳差值均值 −114 µs、尾部 −388 µs,与我们的 p99 同量级。**热路径线程必须忙等(never idle)**,并 `prctl(PR_SET_TIMERSLACK, 1)`。 | AWS phc_probe README 示例;内核默认 slack 50000 ns |
| 3 | **目标实例 = 同 AZ 的 .metal(或 full-slot)。** AWS loopback 基准(调优后 p50/p99.9):`m8azn.metal` **17.7/19.4 µs**、`c7i.metal` 20.3/22.0、`c6in.metal` 20.9/22.7、`c8g.metal` 21.4/23.4(分布最紧,调优前后几乎不变)、`c7i.24xlarge` 21.7/30.9。metal 对 p99.9 的收益 15–29%,对 p50 仅 6–14%。东京按需价:`c8g.metal-24xl` $4.80/h(**$3,506/月**,1 年无预付 $2,312)、`c7i.metal-24xl` $5.39/h($3,937;$2,596)、`m8azn.metal-12xl` $6.40/h($4,672)、`m8azn.metal-24xl` $12.80/h($9,345)、`c8i.metal-48xl` $11.33/h($8,267)。 | AWS OS tuning benchmark results;ap-northeast-1 价目 CSV 2026-09-25 |
| 4 | **PTP 硬件时钟决定"能不能测"。** 2026-06-30 起需 **Precision Time Placement Group(PTPG,可与 CPG 关联)**;支持族 M7a/M7g/M7gd/M7i/M8a/M8g、C7a/C7gd/C7i/C8g/C8gd、R7a/R7g/R7i/R8g/X8g、I8g/I8ge;Tokyo 对 M7a/M7g/M7i/R7a/R7g/R7i/I8g 保留旧式直连。**C7g、C7gn、M8azn、C8i、R7iz 均不在列表**——选 5 GHz 的 m8azn 就要放弃 PHC。实测:m7i.4xlarge PHC 误差界(chrony 公式)±25–34 µs、ClockBound 守护 ±63–78 µs;纯 NTP 的 m5/c6in 为 ±1.4–1.7 ms。ENA 硬件时间戳**只有 RX 没有 TX**。 | EC2 Time Sync 文档;PTPG 公告;clock_bound_measure README |
| 5 | **ENA 能做与不能做**:ENA Express/SRD 轻载时中位延迟 +数十 µs、且要两端都开(交易所不会开)→ 不用;AF_XDP zero-copy 需 GitHub 驱动 ≥2.13、队列数 ≤ 最大值一半、MTU ≤3498;**ntuple 流导向需 Nitro v5+ 且只对"驱动加载后新挂的 ENI"生效**(AWS 自己的 Onload 示例要求 2 块 ENI),否则 `ntuple-filters: off [fixed]`;OpenOnload 在 ENA 上 2026-08 仍不可用(#337),且 #139 报告延迟反而变差;LLQ(TX push)默认开;Tx 中断合并默认静态 64 µs;PHC 读取上限 125 次/s。 | ENA README/Best Practices/Release notes;Onload issue #337 |
| 6 | **CPU:少而快、缓存大。** 云内:5 GHz Turin(`m8azn`,基于 EPYC 9575F)单核最快;Graviton4(`c8g`,2.7–2.8 GHz,无 SMT,L2 2 MB/核)分布最紧;Granite Rapids(`c8i`,3.9 GHz 全核,L3 4.6×)是最新 Intel。colo:EPYC 9575F(64C,5.0 GHz boost,4.5 GHz 全核,256 MB L3)/ **9175F(16C,5.0/4.55 GHz,512 MB L3 = 每核独占 32 MB)**;桌面 X3D(Ryzen 9950X3D、EPYC 4585PX)在单机 colo 里常更快(AMD 自测 4564P 比 9575F 低 9%);Xeon 6 靠 Latency Optimized Mode(+17%)追平。 | Chips and Cheese;AMD 规格页;lowlatencysystem 2026 |
| 7 | **NIC(仅 colo)**:AMD Solarflare X4(2025-10)是软件路径标杆,4 B 报文 Onload 半程 ≈0.6 µs [推断,自 AMD 脚注反推],比 X3 低 27%、比 X2 低 43%;ConnectX-7/8 + XLIO 次之;ExaNIC 维护期。EC2 上只有 ENA,这是 colo 唯一的硬件级优势。 | AMD X4 博文脚注 9xx5-251/252 |
| 8 | **Linux 调优是"砍尾部"不是"降中位"**:AWS 对 c7i.metal 调优后 p50 28.0→20.3 µs、p99.9 33.5→22.0 µs;m8azn.metal 17.9→17.7;c8g.metal 20.5→21.4(略差)但 max −27%;c7a.48xlarge p50 反而 27.2→28.2。清单核心见 §3;AWS 官方 GRUB 行(Intel):`transparent_hugepage=never audit=0 nmi_watchdog=0 nohz=on clocksource=tsc nosoftlockup mce=ignore_ce cpuidle.off=1 skew_tick=1 acpi_irq_nobalance intel_pstate=disable intel_idle.max_cstate=0 processor.max_cstate=0 idle=poll isolcpus= nohz_full= rcu_nocbs= tsc=reliable`。 | tune_os.yaml;OS tuning benchmark results |
| 9 | **时间**:云内 chrony + `refclock PHC /dev/ptp_ena poll 0 delay 0.000010 prefer`(<40 µs);colo 用 GNSS grandmaster + linuxptp(Timebeat Open Timecard Mini £225 起、全规格 £3,138 起;Meinberg 类 $5–15k)。交易时钟用 TSC/`cntvct_el0` 并与 PHC 定期校准。交易所事件戳只有毫秒分辨率,单向延迟只能靠 HW RX 戳 − 交易所 `E` 的大样本分布估计。 | EC2 文档;Timebeat 商店;AWS ec2_timestamping_programs(T1–T10 十点分解) |
| 10 | **两阶段方案**:阶段一(2–4 周)AWS 内完成 EC2 hunting → 胜出 AZ 的 CPG+PTPG → `c8g.metal-24xl`(ARM64,PTP,$3.5k/月)或 `c7i.metal-24xl`(x86,PTP,$3.9k/月)或 `m8azn.metal-12xl`(5 GHz,无 PTP,$4.7k/月)→ 全套 OS 调优 + 三点时间戳分解。阶段二只有当目标交易所迁出 AWS 或提供 colo/专线时才上东京 colo(Equinix TY2/TY6–8、AT TOKYO CC1、NEC Inzai 是 Direct Connect 站点;10 G 专用端口 $2.142/h),否则 colo→AWS 公共端点 RTT ≈1–2 ms **永远输给同 AZ EC2**。Equinix Metal 已于 2026-06-30 终止服务。 | Zenlayer 自测 ≈2 ms;DX 定价;Equinix Metal 文档 |

---

## 1. 托管选项对比

### 1.1 现状诊断:为什么 `c7g.large` 上的数字这么差

| 观测 | 最可能原因 | 证据 / 说明 |
|---|---|---|
| 100 µs 定时器超调 p50 54 µs | Linux 非 RT 线程默认 `timer_slack_ns = 50000`;`nanosleep`/`epoll_wait(timeout)`/futex 超时都被允许晚 50 µs 合并唤醒 | `prctl(PR_SET_TIMERSLACK, 1)` 后 p50 应降到个位数 µs;`SCHED_FIFO/RR` 线程 slack 自动为 0 |
| 定时器超调 p99 193 µs、`sleep(0)` p99 155 µs | vCPU 空闲即 WFI → 交还 hypervisor,唤醒依赖中断注入与宿主机调度;2 vCPU 还要与 2 个 ENA IRQ、内核线程、chrony、SSM agent 抢核 | EC2 处理器状态控制文档:Graviton 固定频率、**不提供 C/P-state 控制**;x86 Gen7+ 的所有规格(含 c7i.large、m8azn.large)可控 C-state,所有 Intel/AMD bare metal 可控 C+P-state。AWS `phc_probe` 在未调优实例上看到 NIC HW 戳与内核 SW 戳差值均值 −114 µs、尾部 −388 µs,即中断/唤醒路径的量级 |
| minRTT Binance 1.0–1.5 ms vs Gate 0.27 ms | Gate 大概率同 AZ 且端点"近";Binance 端点跨 AZ 且可能多一层 LB | AWS 实测跨 AZ RTT p50 173 µs(eu-central-1,c7i.4xlarge,AF_XDP);FlashGrid ping 各区域跨 AZ 0.3–1.2 ms;东京各 AZ 对未见公开数据 [待核实]。1.0–1.5 ms ≫ 任何一对 AZ 的距离,剩余部分属于 02 号笔记(LB/CloudFront 路径) |
| ENA 2 队列、adaptive-rx 开、irqbalance 开 | 队列数 = MIN(8, vCPU) = 2;DIM 负载上来时拉高 rx-usecs 做批处理;irqbalance 周期性改写 IRQ 亲和 | ENA Best Practices;AWS 两份文档在 irqbalance 上互相矛盾(Best Practices 建议保留 + `IRQBALANCE_BANNED_CPUS`,tune_os.yaml 直接禁用),延迟场景以禁用为准 |

### 1.2 AWS 内部:AZ、CPG、EC2 hunting

- **AZ-ID 映射**:每个账户的 AZ 名(`ap-northeast-1a/c/d`)到物理 AZ-ID(`apne1-az1/az2/az4`)随机映射,只有 AZ-ID 跨账户一致:`aws ec2 describe-availability-zones --region ap-northeast-1 | jq -r '.AvailabilityZones[] | "\(.ZoneName),\(.ZoneId)"'`。Tokyo 对新账户开放 az1/az2/az4(xkyle 2020 已无法在 az3 开机)。
- **拓扑层级的真实数字**(AWS 官方 AF_XDP 基准,c7i.4xlarge,100k 消息 @10k/s,单播 RTT):

| 层级 | min | p50 | p90 | p99 | p99.9 | max |
|---|---|---|---|---|---|---|
| 同一物理宿主机(dedicated host) | 17 µs | 22 µs | 25 µs | 28 µs | 29 µs | 30 µs |
| 同 AZ + 同 CPG | 22 µs | 40 µs | 43 µs | 47 µs | 56 µs | 105 µs |
| 同 AZ,不同 PG | 23 µs | 44 µs | 47 µs | 51 µs | 61 µs | 127 µs |
| 跨 AZ(同区域) | 157 µs | 173 µs | 176 µs | 180 µs | 203 µs | 343 µs |
| 跨区域 | 11.36 ms | 11.78 ms | 11.79 ms | 11.80 ms | 11.88 ms | 11.96 ms |

  多播中继(one-way)表还给出:同 AZ 不同 PG 的单跳 p50 172 µs、跨 AZ 单跳 377 µs、CPG 内单跳 24–25 µs——**"同 AZ 但不同 spine"比"同 CPG"多出的不是几 µs 而是可达 100+ µs**,取决于 AZ 内部拓扑。这就是 EC2 hunting 的价值所在。
- **CPG**:AWS 用 c5n 实测比无 CPG 低 35%(p50)/37%(p90)RTT、pps +56–59%;One Trading 案例中 CPG+VPC peering 比无 CPG 的 peering 在高消息率下 p99 快 41%。**CPG 只在我们与对手方共享时才对交易所延迟有意义**;Binance/OKX 等不公开提供 shared CPG(longcipher 的观察:顶级 MM 有 shared CPG / PrivateLink / Direct Connect 直入撮合前队列的 VIP fast path——**未见交易所公开确认,[待核实]**)。对我们的价值:(a)自家 feed/策略/网关多实例之间;(b)作为 hunting 的单位。
- **EC2 hunting(AWS 官方流程)**:每种实例各建一个 CPG(AWS 的 CDK 模板一次开 50+ 种低价实例覆盖尽可能多的 spine),对目标端点测 RTT,保留最好的,再在胜出 CPG 内扩容。Akuna 前 DevOps 公开描述过同一做法。对交易所端点用 TCP 握手 minRTT(`ss -ti` 的 `minrtt`)+ WS 首帧到达分布,ICMP 常被限速。**端点与实例都会因扩缩容/维护而移动,必须周期复测**;实测时间选交易所低峰。
- **容量**:CPG 缩小了可用容量池,metal 规格在东京可能缺货;用 On-Demand Capacity Reservation 锁住(与 CPG 兼容),Spot 只用于探测。
- **逻辑连通**:对交易所公共端点无关;若未来某交易所提供私有端点,优先 VPC peering(零额外跳、同 AZ 无流量费),避免 PrivateLink/TGW(Hyperplane 增加逻辑跳且不在 CPG 物理范围内)。

### 1.3 EC2 实例选型(Tokyo,价格为 2026-09-25 价目)

| 实例 | CPU / 频率 | vCPU | SMT | Nitro | PTPG/PHC | C-state 可控 | 按需 $/h → $/月 | 1 年无预付 $/月 | AWS loopback p50/p99.9(调优后) | 判断 |
|---|---|---|---|---|---|---|---|---|---|---|
| `c7g.large`(现状) | Graviton3 2.5–2.6 GHz | 2 | 无 | v4 | **否** | 否 | 0.0910 → 66 | 44 | — | 淘汰(仅探测) |
| `c8g.large` / `c7i.large`(探测用) | G4 2.7 / SPR 3.2 | 2 | 无/有 | v5/v4 | 是/是 | 否/是 | 0.1001 → 73 / 0.1124 → 82 | 48 / 54 | — | Spot 探针 |
| **`c8g.metal-24xl`** | Graviton4 Neoverse V2 2.7 GHz | 96(96 核) | 无 | **v5** | **是** | 不适用(固定频率) | **4.8029 → 3,506** | **2,312** | **21.4 / 23.4 µs**(调优前 20.5 / 23.4) | **阶段一首选(ARM64)** |
| `c8g.16xlarge`(备选) | 同上 | 64 | 无 | v5 | 是 | — | 3.2019 → 2,337 | 1,541 | — | metal 缺货时的次选(非 full-slot) |
| **`c7i.metal-24xl`** | Sapphire Rapids 8488C,3.2 GHz 全核 turbo(max 3.8) | 96(48 核) | 有,需关 | v4 | **是** | 是(C+P) | **5.3928 → 3,937** | **2,596** | **20.3 / 22.0 µs**(调优前 28.0 / 33.5) | x86 保守解,调优收益最大 |
| `c7i.24xlarge`(full-slot) | 同上 | 96 | 有 | v4 | 是 | 是(C) | 5.3928 → 3,937 | 2,596 | 21.7 / 30.9 µs | 同价不如 metal |
| **`m8azn.metal-12xl`** | EPYC 9575F(Turin)**up to 5.0 GHz** | 48(24 核) | 有,需关 | **v6** | **否** | 是(C+P) | **6.4006 → 4,672** | 无 RI 报价 | m8azn.metal **17.7 / 19.4 µs**(调优前 17.9 / 20.1) | x86 单核最快;放弃 PHC |
| `m8azn.12xlarge` | 同上(虚拟化) | 48 | 有 | v6 | 否 | 是(C) | 6.4006 → 4,672 | — | 18.9 / 22.7 µs | 同价不如 metal |
| `m8azn.metal-24xl` | 同上 | 96 | 有 | v6 | 否 | 是 | 12.8011 → 9,345 | — | 同上 | 核数过剩 |
| `c8i.24xlarge` | Xeon 6 Granite Rapids **3.9 GHz 全核**,L3 4.6× c7i | 96 | 有 | v6 | 否(列表未含 C8i)[待核实] | 是(C) | 5.6626 → 4,134 | — | 未测 | 半路共享宿主,无 PTP |
| `c8i.metal-48xl` | 同上 | 192(96 核) | 有 | v6 | 否 [待核实] | 是 | 11.3251 → 8,267 | — | 未测 | 最新 Intel 单路 metal;贵 |
| `r7iz.metal-16xl` | Sapphire Rapids **3.9 GHz 持续全核** | 64(32 核) | 有 | v4 | 否 | 是 | 7.2640 → 5,303 | 3,341 | 未测 | 2023 代,无 PTP,内存过剩 |
| `c7gn.16xlarge` / `c7gn.metal` | Graviton3E 2.6 GHz,200 Gbps | 64 | 无 | v5 | 否 | — | 5.0368 → 3,677 | 2,297 | 未测 | 带宽对我们无用 |
| `m7i.metal-24xl` | SPR 3.2 GHz | 96 | 有 | v4 | 是(Tokyo 旧式直连) | 是 | 6.2496 → 4,562 | 3,023 | 未测 | 与 c7i.metal 同芯更贵 |
| `c8gn.metal-48xl` / `c8i.metal-24xl` | — | — | — | v6 | — | — | **东京价目无此规格** | — | — | 不可选 |

**解读**:
1. **c8g.metal-24xl**:唯一同时满足"PTP + 全宿主机独占 + 无 SMT 问题 + 最便宜 metal"的选项;缺点是 2.7 GHz 单核较慢(串行解析/簿更新对频率近似线性敏感)与 ARM64 生态风险(§2.3)。
2. **c7i.metal-24xl**:x86 生态零风险、有 PTP、有 C/P-state 控制;单核 3.2 GHz。AWS 数据显示它"默认状态噪声最大、调优后与 c8g 同级"。
3. **m8azn.metal-12xl**:单核最快(5 GHz,AWS 基准 p50 17.7 µs 最低),Nitro v6,200/100 Gbps;代价是**没有 PHC/硬件时间戳**(只能 link-local NTP,<100 µs 误差界;单向延迟测量精度退化),以及 SMT 需关(48 vCPU → 24 核可用)。
4. **不要再买 2–8 vCPU 共享实例做生产热路径**;探测/影子/研究用小实例即可。
5. 稳定后转 1 年 Compute Savings Plan(no-upfront 约 −34%),或直接买 RI:c8g.metal-24xl $2,312/月、c7i.metal-24xl $2,596/月。

### 1.4 裸金属云(Tokyo)

| 供应商 | 状态(2026-09) | 到 ap-northeast-1 RTT | 代表机型与价格(USD) | 判断 |
|---|---|---|---|---|
| Equinix Metal(TY) | **2026-06-30 已终止服务**(2024-11-07 宣布 EOL;2026-03 起东京等地停售按需) | — | — | 出局 |
| Vultr Bare Metal(nrt) | 在售,东京有货 | 未公开,预计 1–3 ms [推断] | `vbm-24c-384gb-amd5` EPYC 9255(Zen 5,24C)$995/月;`vbm-24c-384gb-amd` EPYC 9254 $825/月;`vbm-24c-256gb-amd` EPYC 7443P $725/月;`vbm-6c-32gb` Xeon E-2286G(6C,4.0/4.9 GHz)$185/月 | 便宜的高频探针盒(E-2286G)可用于非热路径 |
| Latitude.sh(TYO3/TYO4) | 在售,东京两个站点 | 同上 | `f4.metal.medium` EPYC 4564P 16C @4.5 GHz $2.03/h($1,482/月);`f4.metal.small` EPYC 4484PX 12C $456/月;`f4.metal.large` EPYC 9275F 24C @4.1 GHz $2,351/月;东京是否有 f4 系列 [待核实] | 唯一提供 Zen 5 高频 F/AM5 系列的裸金属云 |
| Oracle Cloud ap-tokyo-1 | 在售 | OCI↔AWS 走公网/Megaport,≥1–2 ms [推断] | BM.Standard.E5.192:$0.03/OCPU·h ×192 = $5.76/h(**$4,205/月**);BM.Optimized3.36(Xeon 6354 3.6 GHz,36C,100 G RDMA):$0.054×36 = $1.94/h(**$1,420/月**);E6(Turin)$0.03/OCPU·h;东京库存 [待核实] | 硬件好、位置错 |
| Google Cloud asia-northeast1 | 在售 | 同上 | `c3-standard-192-metal`(SPR,1.9/3.0/3.3 GHz)$9.68–10.61/h(**$7,064–7,748/月**) | 频率低、贵 |
| Zenlayer BMC Tokyo | 在售;直连 BBIX/JPIX/JPNAP | **≈2 ms**(自测)[厂商数据] | 8 vCPU/16 GB VM $106/月;裸金属另询;出口 $0.017/GB(AWS $0.09) | 非热路径(链上节点、采集)可用 |
| IBM Cloud Tokyo BM / Alibaba Cloud Tokyo 弹性裸金属 | 在售 | 同上 | 未取得公开报价 [待核实] | 同上 |

**核心判断**:对 AWS 内的交易所,任何 AWS 之外的裸金属都要多付 ≥1–2 ms 的物理+互联代价,直接违背目标函数。它们只在(a)目标交易所本身不在 AWS,或(b)承载 BSC/Solana 节点、数据采集等非热路径负载时有意义(03 号笔记的 Solana 节点建议放法兰克福,与此一致)。

### 1.5 东京 colo + Direct Connect

- **DX 站点(官方列表)**:东京 **Equinix TY2**(同园区 TY6–TY8 可接入)、**AT TOKYO CC1(中央)**、**NEC Inzai(SCSK SI3 可接入)**;大阪 Equinix OS1、Telehouse Osaka 2。端口 1 G / 10 G / 100 G(M = 需 LOA/MACsec 可选)。longcipher 指出 Equinix TY3/TY11 是传统 FX/HFT 老巢,Citadel/Jump 系团队"租 Equinix 机柜 + FPGA + DX 进 AWS"——这是行业口口相传的做法,**延迟数字未见公开**。
- **日本 DX 价格**:专用端口 1 Gbps $0.285/h(≈$208/月)、10 Gbps $2.142/h(≈**$1,564/月**)、100 Gbps $22.50/h;托管连接 1 Gbps $0.314/h、10 Gbps $2.361/h;入向免费;出向 Tokyo→日本 DX 站点按 DTO 表计费(约 $0.04/GB [待核实,表内 JP 列])。
- **公共 VIF 能不能"抄近路"到交易所公共端点?** 公共 VIF 让 colo 直达 AWS 公网前缀(含 ELB/CloudFront anycast 前缀),避开公网互联;但路径仍是 colo 机房 → DX 站点 → AWS 边缘路由 → 目标 AZ,两次机房穿越 + 城域光纤(东京湾一圈 20–40 km ≈ 0.1–0.2 ms 单程)+ AWS 边缘处理,RTT 估 **1–2 ms** [推断;Zenlayer 走 IX 的实测 ≈2 ms,DX 应略优]。**同 AZ EC2 的 0.04–0.2 ms 量级它永远追不上。**
- **colo 成本量级(Tokyo,[估算])**:CBRE 2026 批发价 **$280/kW/月**(东京,同比 0%);零售半柜/整柜(3–5 kW)约 $1,500–3,000/月;电费 $0.185/kWh(日本商业均价);交叉连接 $200–400/月/条;IP transit 1 Gbps $500–1,000/月;DX 10 G 端口 $1,564/月 + 伙伴费;服务器 CAPEX(单路 EPYC 9175F/9575F 1U + 256 GB + Solarflare X4 + GNSS grandmaster + 天线)$25–40k;远程手 $150–250/小时。**全包约 $4–6k/月 OPEX + $30k CAPEX**,与一台 .metal 同级,延迟却更差。
- **什么时候 colo 才对**:某交易所在东京自建/托管撮合并提供 colo 或交叉连接(日系合规所走此模式;我们的目标所暂无);或跨区域策略需要自有链路(Avelacom 类:东京→法兰克福 224 → 137 ms、→斯德哥尔摩 245 → 124 ms,抖动 <90 µs)。

### 1.6 汇总对比表(延迟 + 成本)

| 方案 | 到同 AZ 交易所端点 RTT | 到其他东京 AWS 所 | 尾部抖动来源 | 月成本(USD) | 时间同步 | 建议 |
|---|---|---|---|---|---|---|
| 现状 `c7g.large` @az2 | 1.0–1.5 ms(Binance) | 0.3–1.8 ms | hypervisor 唤醒、邻居、2 队列、timer slack | 66 | NTP(非增强硬件,ClockBound 可达 ±1.4 ms) | 淘汰 |
| `c8g.metal-24xl` @胜出 AZ + CPG + PTPG | 网络部分 40–130 µs(同 AZ)+ LB 层 [推断] | 0.2–1.2 ms(跨 AZ) | 仅 NIC/内核;无 SMT | 3,506(RI 2,312) | **PHC <40 µs + HW RX 戳** | **阶段一首选** |
| `c7i.metal-24xl` @同上 | 同上 | 同上 | 需关 SMT/C-state | 3,937(RI 2,596) | PHC | x86 保守解 |
| `m8azn.metal-12xl` @同上 | 同上 | 同上 | 需关 SMT/C-state | 4,672 | NTP 仅 | x86 最快单核,放弃 PTP |
| 东京裸金属云(Vultr/Latitude/OCI) | ≈1–3 ms | ≈1–3 ms | 自控硬件 | 0.2–4.2k | 自建 NTP/PTP 无 GPS | 仅非热路径 |
| 东京 colo + DX 10 G | 1–2 ms [推断] | 1–2 ms | 自控硬件(最优) | 4–6k + 30k CAPEX | GNSS grandmaster <1 µs | 仅当交易所出 AWS |

---

## 2. CPU / NIC 选型

### 2.1 微架构数据(单核延迟视角)

| 指标 | Graviton4(Neoverse V2) | Xeon 6 6985P-C(Redwood Cove,c8i/r8i) | Sapphire Rapids(c7i/r7iz) | EPYC 9005 Turin(Zen 5,m8azn) |
|---|---|---|---|---|
| 频率 | 2.8 GHz(48xl 双路 2.7) | 3.9 GHz 持续全核(AWS 定制) | 3.2 全核 / 3.8 单核(c7i);3.9 持续(r7iz) | 9575F:5.0 boost / 4.5 全核 / 3.3 base;9175F:5.0 / 4.55 / 4.2 |
| SMT | 无 | 有 | 有 | 有 |
| L1D | 64 KB,4 cycle | 48 KB,5 cycle | 48 KB,5 cycle | 48 KB,4 cycle |
| L2 | 2 MB,11 cycle | 2 MB,16 cycle | 2 MB,16 cycle | 1 MB,14 cycle |
| L3 命中 | 68 cycle ≈ 25 ns(36 MB 共享/96 核) | ≈33 ns(160 MB/tile,SNC3) | ≈33 ns 级 | Zen 4 Bergamo 18 ns;9175F 每核独占 32 MB [推断] |
| DRAM | 114 ns | ≈131 ns(跨 die +26/+50 ns) | Emerald Rapids 更低 | 125.6 ns(NPS1) |
| 核间 | 30–60 ns(mesh) | 50–80 ns 同 die | 同级 | 8–25 ns 同 CCX / 150–180 ns 跨 CCD |
| 单线程 SPECint | ≈ 6985P-C 同级(3.9 GHz vs 2.8 GHz 拉平) | 同左 | 略低 | 高频 SKU 显著领先 |

来源:[Chips and Cheese, Graviton 4](https://chipsandcheese.com/p/arms-neoverse-v2-in-awss-graviton-4)、[Xeon 6 memory subsystem](https://chipsandcheese.com/p/a-look-into-intel-xeon-6s-memory)、[lowlatencysystem 2026](https://lowlatencysystem.com/guide/)、AMD 规格页、AWS 实例页。

**HFT 社区 2025–2026 共识(colo 场景)**:单路;SMT 关;C-state 关到 C1 或全关;turbo 开;uncore 定频;SMI/功耗监控关;内存满通道满频;核间延迟是一等选型指标(`c2clat`)。**AMD EPYC 9005 F 系列**是"标准服务器里最快"的选择:9575F(64C,400 W)、9475F(48C)、**9175F(16C,320 W,16 个 CCD 各启 1 核 → 512 MB L3)**;AMD 官方 EPYC 9005 低延迟调优指南(文档 58649)与 Solarflare X4 的发布测试均在 9575F 上完成。**桌面/AM5 X3D**(Ryzen 9950X3D:16C,144 MB L3,5.7 GHz;EPYC 4585PX:AM5,3D V-Cache)在单机单 NIC 的 colo 盒子里常常更快——AMD 自己的数据:X4 网卡在 EPYC 4564P(16C,5 GHz AM5)上比 9575F 低 9%(54 ns),比 Xeon 6534 低 12.6%(77 ns);代价是 PCIe 通道少、无 RDIMM、无 BMC 级运维。**Intel Xeon 6**:靠 Latency Optimized Mode(mesh/uncore 定频,Phoronix 实测约 +17%)与 SST-BF 追平;Intel mesh 跨核 50–80 ns 优于 AMD 跨 CCD,适合多线程紧耦合,对单线程为主的我们不重要。**超频精品机**(Blackcore、Hypertec CIARA,5+ GHz 全核液冷,RAM/cache 延迟再降 ~30%)属阶段二之后。

### 2.2 云内 CPU 决策矩阵

| 维度 | Graviton4(c8g.metal) | Turin 5 GHz(m8azn.metal) | Sapphire Rapids(c7i.metal) | Granite Rapids(c8i.metal-48xl) |
|---|---|---|---|---|
| 单核解析速度(相对) | 1× | ≈1.6–1.9× [推断:频率 1.8× × IPC 差] | ≈1.1–1.2× | ≈1.4× [推断] |
| AWS loopback p50 / p99.9 | 21.4 / 23.4 µs | **17.7 / 19.4 µs** | 20.3 / 22.0 µs | 未测 |
| 调优前后 p50→p99.9 差 | 2.0 µs(最紧) | 1.7 µs | 1.7 µs(调优前 5.5) | — |
| PTP / HW RX 戳 | 有 | **无** | 有 | 无(列表未含)[待核实] |
| SMT 需关 | 不需要 | 需要 | 需要 | 需要 |
| Nitro | v5(ntuple 可能可用) | v6 | v4(无 ntuple) | v6 |
| Rust 生态风险 | rustls ARM 吞吐略低、SIMD JSON 走 NEON | 无 | 无 | 无 |
| 东京月价(按需) | 3,506 | 4,672(12xl) | 3,937 | 8,267 |

### 2.3 ARM64(Graviton)的 Rust 注意点

1. **SIMD JSON**:`sonic-rs` README 明确"x86_64 与 aarch64 更快,其他架构回退且可能很慢",并提供 `benchmark_aarch64.md`;`simd-json` 在 aarch64 使用 NEON,`runtime-detection` 特性仅对 x86 有效。NEON 128-bit vs AVX2 256-bit 的差距要在目标机实测 [待核实幅度];按 04 号笔记"按交易所报文布局定制解析"后差距缩小。
2. **编译目标**:AWS 建议 `-Ctarget-cpu=neoverse-v2`(Graviton4;通用二进制用 `neoverse-512tvb`),并确保 LSE 原子指令(`+lse`;Rust ≥1.57 默认 `outline-atomics`)。LSE 对 CPU 间通信、锁的收益可达一个量级;SPSC 队列等无锁结构受益。
3. **时钟源**:x86 用 `rdtsc`/`rdtscp`(invariant TSC;AWS 在 GRUB 加 `clocksource=tsc tsc=reliable`,分辩率 ≈0.3 ns);ARM64 用 `mrs cntvct_el0`,分辩率 = 1/`CNTFRQ_EL0`——**启动时读取 `dmesg | grep arch_timer` 或 `cntfrq_el0` 校验频率**(Graviton 的具体值 [待核实]);`clock_gettime(CLOCK_MONOTONIC)` 走 vDSO 两边都 ≈20 ns。
4. **缓存行**:Graviton L1D 64 B 对齐;Rust `crossbeam_utils::CachePadded` 在 aarch64 按 128 B 填充(兼容 N1/Apple 的相邻行预取),无害。
5. **内存序**:ARM 弱序,x86-TSO 会掩盖 `Relaxed` 误用;无锁队列必须在 ARM 上跑 loom/miri 与压力测试。
6. **电源/隔离**:无 SMT、固定频率 → 不需要 `nosmt`、`intel_idle.max_cstate`、`idle=poll`(后者是 x86 专属参数);AWS 的 Graviton GRUB 行只有 `audit=0 nmi_watchdog=0 nohz=on nosoftlockup skew_tick=1 acpi_irq_nobalance isolcpus= nohz_full= rcu_nocbs=`。
7. **页大小**:Ubuntu aws arm64 内核 4 KB 页,2 MB / 1 GB hugepages 可用 [1 GB 待核实];AWS 建议 THP 用 `madvise`(而非 x86 的 `never`)。

### 2.4 NIC(仅 colo)

| NIC | 软件栈 | 延迟数据 | 时间戳 | 状态 |
|---|---|---|---|---|
| **AMD Solarflare X4**(X4522/X4542,PCIe Gen5,自研 ASIC,CTPIO 直通发送) | Onload(透明 socket 加速)、TCPDirect、ef_vi | 4 B 报文 Onload 半程 ≈0.6 µs [推断:AMD 脚注 54 ns = 9%];0–648 B 均值比 X3 低 27%、比 X2 低 43% [厂商数据] | PTP HW 戳 | 2025-10 发布,当前标杆 |
| Solarflare X3522(Alveo X3) | 同上 | X4 的 1.27× | 有 | 大量在役 |
| Solarflare X2522-25G | 同上 | X4 的 1.43×(≈0.9–1.1 µs 半程 [推断]) | 有 | 性价比,二手多 |
| NVIDIA ConnectX-7 / ConnectX-8 | XLIO(取代 VMA)、Rivermax + NEIO FastSockets、DPDK mlx5 | 半程 ≈1 µs 级 [待核实] | 有,PHC 优秀 | 市场数据扇出/抓包主力 |
| Cisco Nexus SmartNIC(ex-ExaNIC X25/X100) | exasock/libexanic | 568 ns trigger-to-response | 有 | 维护模式,不建议新购 |
| Napatech | 抓包/时间戳 | — | ns 级 | 旁路测量,不上交易路径 |

**在 EC2 上**只有 ENA(Nitro 卡):LLQ 推送模式默认开、HW RX 戳(PTPG)、AF_XDP zero-copy(≥2.13)、DPDK(AWS 自己的 mcast2ucast:metal CPG 内 RTT p50 ≈25 µs,虚拟化 ≈34 µs);没有 HW TX 戳、没有 Onload、ntuple 受限(§3.3)。

---

## 3. Linux 调优清单(每项:做什么 / 预期收益 / 风险)

> 原则:先测(`hiccups`、`rtla timerlat/osnoise`、`perf stat -e sched:sched_switch -a -A`、`/proc/interrupts`、`ethtool -S`),一次只改一项,24 h 直方图对比。数字来自 AWS OS Tuning Reference / tune_os.yaml / 基准结果、Rigtorp、LWN 与内核文档;标 [推断] 者为经验值。AWS 自己的结论:调优主要压尾部;对 c7i/m5zn 这类默认噪声大的实例 p50 −28–33%、p99.9 −26–41%;对 m8azn/c8g 中位几乎不变。

### 3.1 CPU 隔离与电源

| 知识点 | 做法 | 预期收益 | 风险 / 备注 |
|---|---|---|---|
| **隔离热路径核** | 内核参数 `isolcpus=domain,managed_irq,<list> nohz_full=<list> rcu_nocbs=<list> rcu_nocb_poll irqaffinity=<hk>`;或 cgroup v2 `cpuset.cpus.partition=isolated`。AWS 对 AMD 额外加 `rcupdate.rcu_cpu_stall_suppress=1 kthread_cpus=<hk>` | 去掉调度器迁移、1 kHz tick(残留约 1 次/s)、RCU 回调;Rigtorp 示例:隔离核 max 抖动 18 µs vs 非隔离 9–17 ms | `nohz_full` 核上每次 syscall 多几百 ns 的 context tracking → 热路径本就应零 syscall;每核只跑一个可运行线程才真正 tickless |
| 移走内核线程/工作队列 | `find /sys/devices/virtual/workqueue -name cpumask -exec sh -c 'echo <hk_mask> > {}' \;`(AWS 用 systemd 服务在启动后再执行一次并 5 s 后重放,以捕获 NVMe 等晚初始化的队列);`tuna --cpus=<list> --isolate`;`vm.stat_interval=120`;写回线程 `echo 0 > /sys/bus/workqueue/devices/writeback/numa` | 去掉 kworker/vmstat 周期打扰 | 部分 per-CPU kthread 不可移动 |
| **timer slack** | 热路径线程 `prctl(PR_SET_TIMERSLACK, 1)`,或全部忙等不用 sleep/超时 | 我们实测的 p50 54 µs 超调应降到 <5 µs | RT 策略线程自动为 0 |
| SMT 关(x86) | BIOS/`nosmt` 或 `echo off > /sys/devices/system/cpu/smt/control`(AWS 用 systemd 服务每次启动执行) | 独占 L1/L2/分支预测/执行端口;AWS 列为第一项 | vCPU 数减半;Graviton 无此项 |
| C-state 关 | `cpuidle.off=1 intel_idle.max_cstate=0 processor.max_cstate=0 idle=poll`(AWS Intel 行;AMD 行去掉 intel_ 项);运行期 `/dev/cpu_dma_latency` 写 0 | C1+ 退出 10–200 µs 的唤醒抖动消失 | **仅 metal 与 x86 Gen7+ 规格可控**;`idle=poll` 每空闲核 100% 功耗;Graviton 不可控 → 用忙等替代 |
| P-state / governor | `cpupower frequency-set -g performance`;metal 上 `intel_pstate=disable`(AWS)或 `amd_pstate=passive` 后定频;`intel_pstate/min_perf_pct=100` | 消除 10–50 µs 的频率切换 | Graviton 固定频率 |
| `skew_tick=1`、`acpi_irq_nobalance`、`nohz=on` | 内核参数(AWS 三种架构都加) | housekeeping 核 tick 错开,减少锁竞争 | 无 |
| 线程钉核 | `pthread_setaffinity_np` + `numactl --membind`;`lscpu --extended` 确认拓扑 | 避免启动期迁移 | AMD 通信线程同 CCX;Xeon 6 同 die(SNC3 下跨 die +26 ns) |
| **调度策略** | 首选 `SCHED_OTHER` + 单线程/核 + 忙等(Rigtorp);若用 `SCHED_FIFO`:`kernel.sched_rt_runtime_us=-1`(AWS 用 990000 避免锁死),并注意 6.12+ 的 fair deadline server 会给被饿的 CFS 任务每秒最多 50 ms [待核实] | 忙等下 FIFO 无额外收益,反而可能锁死 housekeeping | 隔离核上不留任何 CFS 任务即可规避 |
| EEVDF 调度器(6.6+) | AWS 的 `kernel.sched_min_granularity_ns/sched_latency_ns/sched_wakeup_granularity_ns` 在 6.6+ 已不存在(旧知识);可用 `/sys/kernel/debug/sched/base_slice_ns`;`kernel.timer_migration=0` 仍有效 | 对忙等设计无关 | 不要照抄旧 sysctl |
| PREEMPT_RT(6.12+ 主线) | 一般**不用** | RT 优化"唤醒-调度最坏值",对忙等设计无益;把 softirq/IRQ 线程化增加网络栈开销;Red Hat 经验:标准内核调优拿到 ~90% | 若某些线程必须 sleep/wake 可评估 |
| `mitigations=off` | 内核参数(x86 与 ARM64 均支持) | 历史 15–25% 吞吐差;热路径零 syscall 时收益小(主要省 syscall/上下文切换的 IBRS/retpoline 开销) | 单租户单用途盒子可接受;AWS 官方行未加 |
| 看门狗与杂项 | `nmi_watchdog=0 nosoftlockup audit=0 mce=ignore_ce`(AWS);再加 `nowatchdog pcie_aspm=off`;`kernel.watchdog=0 kernel.numa_balancing=0 kernel.timer_migration=0 kernel.printk=3 3 3 3` | 去掉周期性 NMI/轮询 | `mce=ignore_ce` 丢可纠正错误日志 |

### 3.2 内存

| 知识点 | 做法 | 预期收益 | 风险 |
|---|---|---|---|
| THP | x86:`transparent_hugepage=never` + `khugepaged/defrag=0`;Graviton:`madvise`(AWS 明确区分) | 避免 khugepaged 提升/压缩造成 100 µs–1 ms 停顿与 TLB shootdown | Graviton 上 `MADV_HUGEPAGE` 得 TLB 收益 |
| 显式 hugepages | 启动期 `hugepagesz=1G hugepages=N`(x86)或 `vm.nr_hugepages`(2 MB);`mmap(MAP_HUGETLB)` 承载订单簿、环形队列、日志 mmap;`vm.compaction_proactiveness=0`、`vm.compact_unevictable_allowed=0`(AWS) | TLB 覆盖 ×512;无运行期缺页与压缩 | 不可换出、预留浪费 |
| `mlockall(MCL_CURRENT\|MCL_FUTURE)` + `swapoff -a` + `vm.swappiness=0` | 启动即锁;提高 `RLIMIT_MEMLOCK` | 消除主/次缺页 | — |
| 不归还内存 | 不 `munmap`/`MADV_FREE`,不建可写文件映射(tmpfs/hugetlbfs 除外) | 消除 TLB shootdown IPI | 由分配器策略保证(04 号笔记) |
| NUMA | `kernel.numa_balancing=0`、`vm.zone_reclaim_mode=0`;NIC、内存、核同 NUMA/同 die | 避免页迁移与远端访问 | 单路 metal 上 SNC 模式需确认 |
| KSM | `echo 0 > /sys/kernel/mm/ksm/run` | 避免合并期 TLB shootdown | 云镜像通常已关 |
| 内核预留 | `vm.min_free_kbytes` = 1–3% 内存(AWS 用 1,000,000 KB) | skb 分配不触发回收 | — |
| 脏页 | `vm.dirty_background_bytes=32 MB vm.dirty_bytes=256 MB vm.dirty_writeback_centisecs=10`(AWS) | 日志写回小批量、可预期 | 仅非热路径盘 |

### 3.3 网络栈(ENA 与通用)

| 知识点 | 做法 | 预期收益 | 风险 / ENA 事实 |
|---|---|---|---|
| **irqbalance 关** | `systemctl disable --now irqbalance`;启动脚本写 `/proc/irq/*/smp_affinity_list`(AWS 服务:所有 IRQ 先钉 housekeeping,再把 ENA 各队列 IRQ 轮询分配到 housekeeping 核) | 防止 IRQ 被漂到隔离核 | ENA Best Practices 建议保留 + 黑名单;延迟场景以关闭为准 |
| 队列数与 IRQ 布局 | `ethtool -L ens5 combined N`(N = 交易线程数 + 1);交易流队列的 IRQ 钉到对应应用核(ENA 建议"程序与处理其流量的 IRQ 同 vCPU")或相邻核;其余队列钉 housekeeping | 缓存局部性;忙轮询下 IRQ 很少触发 | ENA 队列 = MIN(8 或 32, vCPU);改队列数有 <1 ms 中断 |
| **流导向** | ENA ntuple(`ethtool -U`,驱动 ≥2.13,Nitro v5+,**只对驱动加载后新挂的 ENI 生效**——AWS 自己的 Onload 示例要求实例带 2 块 ENI;否则 `ntuple-filters: off [fixed]`);若不可用则改 RSS:`ethtool -X` 设 Toeplitz key/indirection(Nitro v4+),对每条交易所连接**挑选本地源端口**使 4 元组哈希落到目标队列(`toeplitz_calc.py` 预计算) | 每条 WS/REST 连接固定落在指定核 | connect 前 bind 源端口;驱动 v2.2.11g–v2.6.0g 哈希初值有 bug |
| 中断合并 | `ethtool -C ens5 adaptive-rx off rx-usecs 0 tx-usecs 0`(AWS udev 规则对所有 ENA 网卡执行;生产折中 rx-usecs 1–2) | 去掉 DIM 动态批处理;ENA 默认 **Tx 静态 64 µs** | rx-usecs 0 高负载下每帧一中断;有 busy-poll 时中断本就少 |
| 环大小 | `ethtool -G ens5 rx 4096 tx 4096`(AWS);Rx 最大 16K;Tx 上限 1K(Nitro v4 开 Large LLQ 后 512,v5+ 可回 1024) | 抗突发不丢包 | 环越深平均缓冲越深;我们流量小,2–4K 足够 |
| 卸载 | `ethtool -K ens5 rx off tx off sg off gso off gro off`(AWS 连校验卸载都关)+ `lro off tso off`;`rx-copybreak` 调小 [待核实收益] | 去掉批处理延迟 | 仅热路径网卡 |
| **busy poll(阻塞 socket)** | `net.core.busy_read`/`busy_poll` = 50(x86)/ 25(Graviton)(AWS tune_os.yaml);ENA 文档建议 70 | 省掉 5–15 µs 的中断投递路径 | 每次 recv 最多自旋该时长 |
| **prefer busy poll / IRQ 屏蔽** | 每 socket `SO_BUSY_POLL`+`SO_PREFER_BUSY_POLL`+`SO_BUSY_POLL_BUDGET`;`/sys/class/net/ens5/napi_defer_hard_irqs=2`、`gro_flush_timeout=200000`(须大于应用处理窗口);epoll 用 `EPIOCSPARAMS`(6.9+/glibc 2.40:`busy_poll_usecs/budget/prefer_busy_poll`);6.13+ 用 per-NAPI `irq-suspend-timeout`(netlink `napi-set`) | 应用轮询期间 IRQ 保持屏蔽;LWN 实测(AF_XDP,32 Kpps + 30 µs 处理):中断模式 p99 **21.1 µs**,inline SO_BUSYPOLL 17.1,独立核轮询 14.0,**threaded NAPI busy poll 13.0** | 忘记续约(gro_flush_timeout 到期)回退 softirq;ENA 驱动在 prefer-busy-poll 补丁集的驱动列表中 |
| threaded NAPI busy poll | `echo 2 > /sys/class/net/ens5/threaded`(内核 6.18+ [待核实版本]),napi 线程钉核 `chrt -f 50` | 比用户态独立核轮询再低 1–1.5 µs;应用处理期间内核线程持续把描述符搬进 recv 环 | 独占一核 100% |
| RPS/RFS/XPS | RPS 关(`rps_cpus=0`、`rps_sock_flow_entries=0`);XPS 指向发送线程所在核(AWS 把 XPS 指向 housekeeping) | 避免跨核软中断 | 多队列 + 正确 RSS 后无需 RPS |
| qdisc | `net.core.default_qdisc=noqueue`(AWS)或 `pfifo_fast`;`tc qdisc replace dev ens5 root mq` | 避免 fq/fq_codel 的定时器/pacing | 无 |
| **TCP 选项** | `TCP_NODELAY`(必开)、每次 recv 后 `TCP_QUICKACK`、`net.ipv4.tcp_autocorking=0`、`tcp_slow_start_after_idle=0`、`tcp_no_metrics_save=1`、`tcp_notsent_lowat` 对小报文无意义(AWS 设 16384)。**不照抄** AWS 的 `tcp_timestamps=0`(丢 RTT 估计/PAWS)、`tcp_sack=0`(公网路径需要;若因 LLQ 96 B 头限制丢包应改开 Large LLQ `tx-push-buf-len 224`)、`tcp_low_latency`(4.14 已移除)、`tcp_fastopen=3`(交易所不支持) | autocorking 在前一 skb 未离开 qdisc 时把小写合并推迟(µs 级);空闲后 cwnd 回落到 initcwnd 会让突发下单多一个 RTT | — |
| 缓冲区 | `rmem_max/wmem_max` 16 MB、`optmem_max` 16 MB、`netdev_max_backlog` 1,000,000、`netdev_budget` 600(AWS) | 抗突发 | 缓冲过大增加 worst-case |
| 内核旁路 | AF_XDP zero-copy(ENA ≥2.13,`combined ≤ max/2`,MTU ≤3498,需 XDP 程序,6.12+ 有 Page Pool,2.17 起 AF_XDP 队列带 HW 时间戳与 XDP hints)或 DPDK | 省 1–10 µs/包;AWS DPDK 基准 metal RTT 25 µs | 自己实现 TCP+TLS(见 01 号笔记);Onload-on-ENA 不可用;HW 戳在 Onload zero-copy 模式下失效 |
| 时间戳 | `SO_TIMESTAMPING`(`RX_HARDWARE|RX_SOFTWARE|RAW_HARDWARE|TX_SCHED|TX_SOFTWARE|OPT_ID|OPT_TSONLY`,TX 走 `MSG_ERRQUEUE`) | 分解 NIC→驱动→用户预算 | ENA 无 `TX_HARDWARE` |

### 3.4 系统与发行版杂项(Ubuntu aws 镜像)

- 卸载/禁用:`snapd`、`unattended-upgrades`、`apport`、`amazon-ssm-agent`(AWS 调优脚本自己也停)、`amazon-cloudwatch-agent`、`motd-news`、`systemd-oomd`、`multipathd`、`apparmor`(或 complain)、`firewalld/iptables`(AWS 清空);`cloud-init` 首次启动后 `touch /etc/cloud/cloud-init.disabled`。
- ENA 驱动用 GitHub 最新版(2.17.3)而非内核自带:PHC(≥2.10,`ENA_PHC_INCLUDE=1 make`,`phc_enable=1`,2.16 起也可 devlink)、HW 包时间戳(≥2.15)、AF_XDP ZC(≥2.13)、ntuple(≥2.13)、Page Pool(6.12+)、Large LLQ 控制、AF_XDP 队列 HW 戳与 XDP hints(2.17)。
- 存储:NVMe `none`/`mq-deadline`、`iostats=0`、`add_random=0`;日志异步落盘。
- 验证:`hiccups`、`rtla timerlat -c <core>`、`perf stat -e irq_vectors:local_timer_entry -a -A`、`watch cat /proc/interrupts`、`ethtool -S ens5 | grep -E 'xdp|drop|overrun|pps_allowance|bw_.*_allowance|busy'`。

### 3.5 预算参考(EC2 metal + 内核栈 + 忙等,[推断])

| 段 | 典型 | 依据 |
|---|---|---|
| NIC HW RX → 内核 skb(D2) | IRQ 路径 5–15 µs;忙轮询 1–3 µs | AWS Tuning Reference;LWN |
| 内核 → 用户读到数据(D3) | 1–2 µs(忙轮询) | 同上 |
| TLS 解密 + JSON 解析 + 簿更新 + 决策 | 3–10 µs(取决于报文与实现) | 04 号笔记 |
| 用户 send → 内核 TX(D5)→ LLQ 推送 | 3–6 µs | LLQ "节省数 µs";dedicated-host RTT 22 µs 含双端栈 |
| 软件全程(不含网络) | **10–25 µs** | 与 AWS loopback 17.7–21.4 µs 一致 |
| 网络到同 AZ 交易所端点(单程) | 20–90 µs(同 spine)~ 100+ µs(不同 spine)+ LB 层 | §1.2 表 |
| 跨 AZ 单程 | ≈90–600 µs | AWS 173 µs RTT;FlashGrid 0.3–1.2 ms RTT |

结论:**网络放置(百 µs 级)≫ 软件栈(十 µs 级)≫ 单项内核参数(µs 级)**。

---

## 4. 时间同步与测量

### 4.1 云内方案

| 方案 | 误差界(相对 UTC) | 条件 | 备注 |
|---|---|---|---|
| chrony → `169.254.169.123`(link-local NTP,所有实例) | 增强硬件上典型 <100 µs(PTPG 内成为 stratum-1 源,示例 root distance ±91 µs);**非增强硬件上 ClockBound 报 ±1.4–1.7 ms**(m5.4xlarge、c6in.4xlarge 实测) | 无 | 1024 pps link-local 总限额(含 IMDS/DNS);闰秒 smear |
| chrony → PHC(`refclock PHC /dev/ptp_ena poll 0 delay 0.000010 prefer`;可用 `dpoll -3` 提高到 8 次/s) | 典型 **<40 µs**;实测 m7i.4xlarge chrony 公式 ±25–34 µs、ClockBound 守护 ±63–78 µs(含 TSC 漂移建模) | PTPG(或 Tokyo 旧式支持族)+ ENA ≥2.10 + `CONFIG_PTP_1588_CLOCK` | PHC gettime 上限 125 次/s;不 smear 闰秒;真实误差界 = |system offset| + 0.5×root delay + root dispersion + `phc_error_bound`(sysfs) |
| 硬件 RX 时间戳 | 每个入向包 64-bit ns,Nitro NIC 打戳 | 同 PHC | `ethtool -T` 显示 `hardware-receive`;loopback 不打戳;**无 hardware-transmit** |
| ClockBound 3.0 守护 | 给出可信区间而非点估计,~14M 调用/s | — | 跨实例事件排序;比 chrony 公式保守 2× |

### 4.2 colo 方案

GNSS(GPS/QZSS)grandmaster → PTP(IEEE 1588-2019)→ Solarflare/ConnectX HW 时间戳 → `ptp4l` + `phc2sys`,可达 <1 µs(好硬件 ~10 ns 级)。设备价:Timebeat **Open Timecard Mini 3.0 £225 起**(多频 GNSS,PTP/NTP,DOCXO 保持 1.5 µs/8 h)、Open Timecard(Rb/DOCXO,USB-C 供电)£3,138 起、三合一 Appliance £4,134 起;Meinberg M1000/microSync、Safran SecureSync 类 $5–15k [估算]。需机房天线通道(额外月费)。MiFID II RTS 25 对 HFT 的 100 µs / 1 µs 粒度可作内部标准。

### 4.3 交易时钟(热路径内部)

- x86:`rdtsc`/`rdtscp`(invariant TSC;Nitro 暴露稳定 TSC,`clocksource=tsc tsc=reliable`),启动期用 `CLOCK_MONOTONIC_RAW` 校准频率,运行期每秒与 PHC 对齐一次,保存 (tsc0, phc0, ratio) 三元组;热路径只做乘加。AWS 的 `ec2_timestamping_programs` 同样用 TSC 校准做 pps 控制,并提供 "RTT D3(TSC)" 与 "RTT D2(系统时钟)" 的对照。
- ARM64:`mrs cntvct_el0` + `cntfrq_el0`,同样校准;分辩率取决于 CNTFRQ [待核实]。
- 日志时间戳携带"PHC 域"与"误差界",供事后与交易所时间对齐。

### 4.4 测什么、怎么测

1. **自身预算十点分解**(AWS `ec2_timestamping_programs` 的 T1–T10 定义):D1 应用 TX→内核 TX;D2 HW RX→内核 RX;D3 内核 RX→应用 RX;D4 应用 RX→应用 TX;D5 应用 TX→内核 TX;D6/D7 客户端侧对应;TT D1 = HW RX→应用 RX(我们的"NIC 到用户"预算);RTT D1 = 应用 TX→HW RX。对交易所连接我们只能测己方一侧,但 D2/D3/D5 足以定位内核与用户态瓶颈。目标(metal + 忙等 + 内核栈):TT D1 p99 <10 µs;D5 <5 µs [推断]。
2. **到交易所的单向延迟**:交易所 WS 事件带 ms 级时间戳(Binance `E`/`T`、OKX `ts`、Bybit `ts/cts`、Gate `t`),精度 1 ms;我们的 HW RX 戳误差 <40 µs → (HW RX − E) 的分布可估"撮合出事件到我们网卡"的单向延迟均值(大样本下 ~0.1 ms 精度)与抖动形态,并区分"AZ 距离"(常数)与"交易所内部扇出/LB 排队"(随负载变化)。另用订单 ack 往返(下单本地戳 → ack 到达 HW 戳)取半作对照。**没有 PHC 的实例(m8azn)只能用 NTP 时钟做这件事,误差 0.1–1 ms 量级,基本失去意义。**
3. **AZ 探测**:Spot 实例 × 各 AZ × CPG,TCP 握手 minRTT + WS 首帧分布;保留最好的 AZ/CPG,每周重测。
4. **OS 抖动基线**:`hiccups`/`rtla osnoise` 在隔离核 24 h 采样,目标 max <20 µs、p99.9 <5 µs(metal);共享小实例上不可能达到。

---

## 5. 推荐的两阶段方案

### 阶段一(2–4 周,全部在 AWS 内)

1. **AZ/spine 探测**:在 az1/az2/az4 各开 3–5 台 Spot(`c8g.large`/`c7i.large`,$0.10–0.11/h),各自 CPG,对 Binance/OKX/Bybit(11-12 迁入后)/Bitget/Gate/KuCoin/Hyperliquid 端点测握手 minRTT 与 WS 首帧分布,确认 Binance 所在 AZ(预期 az4)与最佳 spine。总花费 <$50。
2. **实例升级**:在胜出 AZ 创建 CPG + PTPG(关联),上 **`c8g.metal-24xl`**($3,506/月;ARM64,PTP)或 **`c7i.metal-24xl`**($3,937/月;x86,PTP);若基准显示单核解析是瓶颈且愿放弃 PTP,则 **`m8azn.metal-12xl`**($4,672/月)。用 On-Demand Capacity Reservation 锁容量;稳定后转 1 年 Savings Plan(约 −34%)。
3. **OS 调优**(§3 全清单,以 AWS tune_os.yaml 为基线、按 §3 的修正项调整)+ ENA GitHub 驱动 2.17.3 + PHC + HW RX 戳 + 十点时间戳分解;建立 24 h 抖动与延迟仪表(HDR histogram,p50/p99/p99.9/max)。
4. **验收指标**:隔离核 osnoise max <20 µs;HW RX→用户 p99 <10 µs;定时器超调 p99 <10 µs;到 Binance minRTT 比现状至少降 0.5 ms 且 p99 RTT 抖动 <100 µs;chrony 报告的 PHC 误差界 <40 µs。
5. 保留一台小实例做影子/探测,不承载热路径。

### 阶段二(季度级,条件触发)

- **触发条件**:(a)某主要目标所迁出 AWS 或提供 colo/交叉连接;(b)阶段一测得 AWS 网络抖动(而非我们的栈)主导 p99 且 hunting 无法改善;(c)需要自有跨区域链路。
- **形态**:Equinix TY2/TY6–8 或 AT TOKYO CC1 半柜,单路 EPYC 9175F/9575F(或 9950X3D 试验机)+ Solarflare X4 + Timebeat/Meinberg GNSS grandmaster,Direct Connect 10 G 专用端口($1,564/月)公共 VIF 到 ap-northeast-1 + 1 Gbps transit 备路;OPEX $4–6k/月 + CAPEX $30k [估算]。
- **明确不做**:不为 AWS 内交易所迁 colo;不买 ENA Express;不在共享小实例上跑热路径;不用 PREEMPT_RT 作默认内核;不照抄 AWS sysctl 里的 `tcp_timestamps=0/tcp_sack=0/tcp_fastopen=3`。

---

## 6. 参考链接

- AWS, *Optimize tick-to-trade latency… Part 1*(2025-07):https://aws.amazon.com/blogs/web3/optimize-tick-to-trade-latency-for-digital-assets-exchanges-and-trading-platforms-on-aws/
- AWS, *… Part 2*(2026-04,metal vs full-slot、OS 调优实测):https://aws.amazon.com/blogs/web3/optimize-tick-to-trade-latency-for-digital-assets-exchanges-and-trading-platforms-on-aws-part-2/
- aws-samples/trading-latency-benchmark:仓库 https://github.com/aws-samples/trading-latency-benchmark ;OS Tuning Reference https://raw.githubusercontent.com/aws-samples/trading-latency-benchmark/main/assets/blog/tick_to_trade_latency_part_2_os_tuning_reference.md ;基准原始数据 https://raw.githubusercontent.com/aws-samples/trading-latency-benchmark/main/assets/blog/tick_to_trade_latency_part2_os_tuning.md ;拓扑延迟表 https://raw.githubusercontent.com/aws-samples/trading-latency-benchmark/main/assets/blog/topology_latency.md ;EC2 hunting https://raw.githubusercontent.com/aws-samples/trading-latency-benchmark/main/deployment/LATENCY_HUNTING.md ;tune_os.yaml https://raw.githubusercontent.com/aws-samples/trading-latency-benchmark/main/deployment/ansible/tune_os.yaml ;networking_benchmarks(时间戳程序、AF_XDP、Onload、DPDK、ClockBound)https://raw.githubusercontent.com/aws-samples/trading-latency-benchmark/main/networking_benchmarks/README.md
- AWS, *Crypto market-making latency and EC2 shared placement groups*(2023-01):https://aws.amazon.com/blogs/industries/crypto-market-making-latency-and-amazon-ec2-shared-placement-groups/
- AWS, *One Trading and AWS: cloud-native colocation*(2023-12):https://aws.amazon.com/blogs/industries/one-trading-exchange-and-aws-cloud-native-colocation-for-crypto-trading/
- AWS, *Bare metal performance with the Nitro System*(2021):https://aws.amazon.com/blogs/hpc/bare-metal-performance-with-the-aws-nitro-system
- AWS, *Ultra-low-latency cross-Region crypto trading with Avelacom*(2026-06):https://aws.amazon.com/blogs/industries/ultra-low-latency-cross-region-crypto-trading-with-avelacom-and-aws/
- EC2 Time Sync / PTPG / PHC 文档:https://docs.aws.amazon.com/AWSEC2/latest/UserGuide/configure-ec2-ntp.html ;PTPG 公告(2026-06-30):https://aws.amazon.com/about-aws/whats-new/2026/06/ec2-time-sync-precision-time-placement-group/ ;HW 时间戳公告(2025-06-23):https://aws.amazon.com/about-aws/whats-new/2025/06/amazon-time-sync-nanosecond-hardware-packet-timestamps/ ;*It's About Time*(2023-11):https://aws.amazon.com/blogs/compute/its-about-time-microsecond-accurate-clocks-on-amazon-ec2-instances/
- EC2 处理器状态控制:https://docs.aws.amazon.com/AWSEC2/latest/UserGuide/processor_state_control.html ;Placement groups:https://docs.aws.amazon.com/AWSEC2/latest/UserGuide/placement-groups.html ;Nitro 版本与实例族:https://docs.aws.amazon.com/ec2/latest/instancetypes/ec2-nitro-instances.html ;实例带宽(单流 5/10/25 Gbps):https://docs.aws.amazon.com/AWSEC2/latest/UserGuide/ec2-instance-network-bandwidth.html
- ENA Linux Best Practices:https://raw.githubusercontent.com/amzn/amzn-drivers/master/kernel/linux/ena/ENA_Linux_Best_Practices.rst ;ENA README(LLQ/PHC/XDP/ntuple):https://raw.githubusercontent.com/amzn/amzn-drivers/master/kernel/linux/ena/README.rst ;Release notes:https://raw.githubusercontent.com/amzn/amzn-drivers/master/kernel/linux/ena/RELEASENOTES.md ;ENA Express:https://docs.aws.amazon.com/AWSEC2/latest/UserGuide/ena-express.html
- M8azn GA(2026-02):https://aws.amazon.com/about-aws/whats-new/2026/02/aws-m8azn-instances-generally-available ;C7i/R7iz/C8i 实例页:https://aws.amazon.com/ec2/instance-types/c7i/ 、https://aws.amazon.com/ec2/instance-types/r7iz/ 、https://aws.amazon.com/ec2/instance-types/c8i/
- AWS 价目 CSV(ap-northeast-1,2026-09-25):https://pricing.us-east-1.amazonaws.com/offers/v1.0/aws/AmazonEC2/current/ap-northeast-1/index.csv
- Direct Connect 定价与站点:https://aws.amazon.com/directconnect/pricing/ ;https://aws.amazon.com/directconnect/locations/
- Onload on ENA issue #337(2026-08):https://github.com/Xilinx-CNS/onload/issues/337 ;ENA busy-poll issue #312:https://github.com/amzn/amzn-drivers/issues/312
- Equinix Metal 终止服务(2026-06-30):https://docs.equinix.com/metal/
- Vultr 裸金属 API(含 nrt 库存与价格):https://api.vultr.com/v2/plans-metal ;Latitude.sh 定价与站点:https://www.latitude.sh/pricing 、https://www.latitude.sh/locations ;OCI 价目 API:https://apexapps.oracle.com/pls/apex/cetools/api/v1/products/ ;GCP c3-standard-192-metal:https://gcloud-compute.com/c3-standard-192-metal.html
- Chips and Cheese, Graviton 4:https://chipsandcheese.com/p/arms-neoverse-v2-in-awss-graviton-4 ;Xeon 6:https://chipsandcheese.com/p/a-look-into-intel-xeon-6s-memory
- AMD EPYC 9175F / 9575F 规格:https://www.amd.com/en/products/processors/server/epyc/9005-series/amd-epyc-9175f.html 、https://www.amd.com/en/products/processors/server/epyc/9005-series/amd-epyc-9575f.html ;AMD EPYC 低延迟调优指南(58649):https://www.amd.com/content/dam/amd/en/documents/epyc-technical-docs/white-papers/58649_amd-epyc-tg.pdf
- AMD Solarflare X4 发布博文与产品页:https://www.amd.com/en/blogs/2025/new-amd-solarflare-x4-low-latency-ethernet-adapter-set.html ;https://www.amd.com/en/products/ethernet-adapters/solarflare-x4.html
- AWS Graviton Getting Started(Rust/LSE/-mcpu):https://github.com/aws/aws-graviton-getting-started ;sonic-rs(aarch64 支持):https://github.com/cloudwego/sonic-rs ;simd-json:https://github.com/simd-lite/simd-json
- Rigtorp, *Low latency tuning guide*:https://rigtorp.se/low-latency-guide/ ;*Latency implications of virtual memory*:https://rigtorp.se/virtual-memory/
- lowlatencysystem.com 2026 指南与研究笔记:https://lowlatencysystem.com/guide/ ;https://lowlatencysystem.com/research/
- 内核 NAPI 文档(busy poll / irq suspend / threaded):https://docs.kernel.org/networking/napi.html ;时间戳文档:https://docs.kernel.org/networking/timestamping.html ;内核参数:https://www.kernel.org/doc/html/latest/admin-guide/kernel-parameters.html ;`EPIOCSPARAMS`:https://manpages.ubuntu.com/manpages/stonking/man2/EPIOCGPARAMS.2const.html ;LWN preferred busy-polling:https://lwn.net/Articles/837010/ ;LWN threaded NAPI busy poll(2025-08):https://lwn.net/Articles/1034942/
- chrony `refclock PHC` 选项:https://chrony-project.org/doc/4.6/chrony.conf.html
- Timebeat 商店(Open Timecard 价格):https://store.timebeat.app/ ;Meinberg M1000:https://www.meinbergglobal.com/english/products/modular-1u-sync-system.htm
- AZ 延迟:FlashGrid(2023):https://www.flashgrid.io/news/latencies-between-aws-availability-zones-what-are-they-and-how-to-minimize-them/ ;xkyle(2020):https://xkyle.com/Measuring-AWS-Region-and-AZ-Latency/
- longcipher, *Why Exchanges Cram Into AWS Tokyo*:https://book.longcipher.com/en/blog/aws-tokyo-digital-wall-street/
- Zenlayer, *Low-latency crypto trading servers in Tokyo*(2026-07):https://cloud.zenlayer.com/blog/crypto-trading-latency-tokyo
- Coloprice 指数(CBRE 2026 东京批发价):https://coloprice.com/index/
- Arbitron 交易所位置:https://arbitron.app/learn/crypto-exchange-server-locations ;https://arbitron.app/learn/binance-server-location
- 未能核对(本环境不可达,留给 02 号笔记):Binance Futures low-latency API FAQ https://www.binance.com/en/support/faq/what-are-binance-futures-low-latency-api-services-7df7f3838c3b49e692d175374c3a3283 ;Bybit 迁东京公告 https://announcements.bybit.com/en/article/bybit-trading-server-migration-from-aws-singapore-to-aws-tokyo--art853451a43a75/ ;OKX 香港→东京迁移公告 https://www.okx.com/en-us/help/okx-trading-server-migration-announcement-hong-kong-tokyo ;DCD *Network issue at AWS data center brings down crypto exchanges* https://www.datacenterdynamics.com/en/news/network-issue-at-aws-data-center-brings-down-crypto-exchanges/
