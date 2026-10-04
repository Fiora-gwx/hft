# Agent Note: 基准形态用调优内核 TCP + 忙轮询,不做内核绕过

Status: proposed

> **第八轮第四次修订(2026-10-03)**:收窄——基准形态的收包改为中断驱动:`busy_poll`、`busy_read`、`napi_defer_hard_irqs`、`gro_flush_timeout` 全为 0,socket 显式 `SO_BUSY_POLL = 0`,交易网卡全部队列的 IRQ 独占一个核;正文中的忙轮询与"热线程纯 busy poll"作废;内核 TCP、不做内核绕过的决定不变;见 [rx-irq-driven](2026-10-03-rx-irq-driven.md)。

## Problem

主机内网络路径是 tick-to-trade 中最容易被"优化"的一段,DPDK/AF_XDP/Onload 的公开数字很诱人。但我们部署在 AWS 东京,交易所也在 AWS 上;要决定基准形态下字节如何进出机器,以及什么条件下才值得绕过内核。

## Proposal

基准形态([03 §3](../../../../design/03-网络与传输层.md)):

- 内核 TCP,feed/gw 线程绑隔离核、非阻塞 socket 循环读;`SO_BUSY_POLL` + `SO_PREFER_BUSY_POLL`、`napi_defer_hard_irqs`、`rx-usecs 0`、关 GRO/LRO/TSO、ntuple/RSS 把每条连接导到其线程所在核。
- TLS 用 rustls unbuffered + aws-lc-rs,WS 用 tungstenite 同步 API 跑在自定义 `Read + Write` 上;热路径无 tokio/mio/epoll。
- 调优参数随发布归档,由 [hostprobe](../../../../tools/README.md#hostprobe) 校验实际生效值。

可选扩展:裸金属 + Solarflare X4 + OpenOnload(POSIX 透明,代码不改)。触发条件:交易所提供 AWS 之外的接入,或基准形态证明 AWS fabric 抖动主导 p99。

进一步收窄([build-test-migrate](../process/2026-10-01-build-test-migrate.md)):IRQ 全部放在 housekeeping 核(`irqaffinity` 只含 hk 核),热线程纯 busy poll;`nohz_full` 默认只给 sequencer 核,feed/gw 核做 A/B;RSS 源端口预计算与 TX 时间戳 errqueue 为可选扩展,默认关,有直方图证明需要时打开。

## Alternatives considered

**DPDK。** AWS 自测:同 AZ + CPG 的 m8a.metal 内核 UDP RTT p50 20.7 / p99 24.1 µs,DPDK(ENA PMD)反而 25.6 / 28.2 µs;还要自写 TCP + TLS + WS、占核([03 §1、§4](../../../../design/03-网络与传输层.md)、[research/01](../../../../design/research/01-network-kernel-bypass-tls.md))。

**AF_XDP。** 零拷贝需驱动 ≥ 2.13、队列 ≤ 一半、MTU ≤ 3498,仍需用户态 TCP/TLS;AWS 实测 32 / 37 µs,不如内核。

**Onload-on-ENA。** issue #337(2026-08)栈分配失败;#139 能跑但更慢(42.7 → 47.9 µs,max 137 → 241 µs),且失去 HW 时间戳。

**io_uring 运行时、用户态 TCP 栈、kTLS。** 少连接单核时 io_uring 延迟高于 epoll(monoio 官方基准);客户端 TLS + WS 的用户态栈没有生产级方案;kTLS 对小报文无收益并失去用户态自旋控制。

**tokio。** 调度与唤醒进入尾部;只用于冷进程与启动期。

决定性事实:Deltix 实测 Binance 事件 → 我们网卡 p50 3.97 ms,主机内核路径只占 2–3%;先拿 AZ 搬迁(0.5–1.5 ms),再谈 µs。

## Acceptance criteria

- `kernel_rx` p99 < 10 µs;隔离核 `osnoise` 24 h max < 20 µs、p99.9 < 5 µs([02 §6](../../../../design/02-硬件与托管方案.md))。
- 生产 `tick_to_trade` 连续 7 天 p50 ≤ 20 µs、p99 ≤ 60 µs、p99.9 ≤ 200 µs(验收下限 30/100)。
- 每项内核参数 A/B 24 h,以直方图为唯一裁判([03 §7](../../../../design/03-网络与传输层.md))。

## Risks

- 放弃 Onload 约 5 µs/单程与 6 倍抖动改善;对 A 类策略可能重要——以 LeadLag 实测为是否加码的触发条件([build-test-migrate](../process/2026-10-01-build-test-migrate.md))。
- 内核升级可能改变忙轮询行为;调优即代码 + `HostProfile` 记录使回归可定位。
