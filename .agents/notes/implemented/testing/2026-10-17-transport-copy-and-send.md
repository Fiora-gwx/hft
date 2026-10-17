# Agent Note: TLS 明文借用与小订单 TCP 发送测量

Status: implemented

## Problem

16 核测量的旧 `egress.send` 是加密完成到 send 返回的包围段，包含 trace 与记账，不能直接当作 send syscall 或物理 wire 时间。当前实现已经把整个掩码 WS 帧加密成一个 TLS record，以一次 sendmsg 提交；有未发送尾部时用两个 iovec 一次提交。接收侧虽然原地解密，仍无条件搬移解密后的 payload。

## Decision

无未消费明文时直接借用 TLS 解密位置，只有跨记录的半帧才搬移拼接。保留固定 scratch、服务端 MASK 拒绝、绝对流偏移、时间戳与借用生命周期。NET-040 检查两条连续记录的实际 payload 地址，NET-092/136 继续验证任意切分及时间戳。

生产发送继续使用一次 sendmsg、TCP_NODELAY=1、SO_BUSY_POLL=0、既有缓冲参数。`send_profile` 在原生 Linux/aarch64 比较等长不可变报文的 send、sendmsg、writev、双 iovec、逐单 uncork、SO_SNDBUF 与空闲 1 ms；零拷贝等待每次 errqueue 完成再释放存储，并报告 copied 标志。计时外排空接收端，零拷贝提交数字不含完成处理成本；loopback 的同核网络处理不能当作远端 NIC 的延迟下限。Transport profile 同机用相同接收 harness 跑 base/head/base；共享 CI 只报告，数字以工作流日志为准。

## Alternatives considered

- `SO_ZEROCOPY` 的内存固定与完成通知增加成本，内核文档说通常超过约 10 KiB 才值得；loopback 总是 deferred copy，不能据其结果声称远端真实零拷贝收益。[Linux MSG_ZEROCOPY](https://docs.kernel.org/networking/msg_zerocopy.html)
- 每个订单设置/清除 TCP_CORK 会增加系统调用；多个订单合并 TLS record 会改变等待时间。当前完整 record 一次提交，不需要逐单切换 cork。[Linux TCP](https://man7.org/linux/man-pages/man7/tcp.7.html)
- 对真实交易连接预加密会提前消费 TLS 序号和 nonce，且签名时间、订单字段尚未确定；不能复用密文或改变一次加密的重试契约。scratch warm 配置仍在现有加载校验处拒绝。
- 更大 socket buffer 不保证降低小报文提交时间，可能掩盖积压；主动保温涉及主机空闲状态和发送频率，未经目标机证据不添加周期性流量或热循环系统调用。

## Consequences

没有订单、费用、风控、TLS 序号或 journal 语义变化。目标机需用分离后的 send_syscall 点及相同市场窗口复测，连同 IRQ/softirq、TCP OFO 和 pre-recv/empty-poll 记录一起核实毫秒接收尾部；共享 runner 的 loopback 数字不能代替订单 decision→wire 验收。生产 send 形式与 socket 参数只有可重复收益才调整。


## Evidence

[Transport profile 38998741699](https://github.com/Fiora-gwx/hft/actions/runs/38998741699)，2026-10-17，GitHub ubuntu-24.04-arm / Neoverse-N2，bench 优化 profile。接收 base fd2dc3b / head 074dc528 / base，50,000 次采样及 5,000 次预热；TLS 加密准备不计时，ScriptIo 接收与解密/解帧计时，不代表真实 syscall。

| buffered 接收 | base p50 前 / 后 (µs) | head p50 (µs) | base p99 前 / 后 (µs) | head p99 (µs) |
|---|---:|---:|---:|---:|
| 274 B | 0.240 / 0.240 | 0.240 | 0.264 / 0.264 | 0.256 |
| 2048 B | 0.728 / 0.728 | 0.704 | 4.275 / 3.665 | 3.817 |
| 8192 B | 2.449 / 2.449 | 2.345 | 7.075 / 6.843 | 6.979 |

长帧 p50 收益约 0.10 µs，小帧无收益；p99 无稳定改善。eager 小帧 p50 1.384/1.416 → 1.440 µs，不能声称所有形态都更快。脚本接收复制和共享 runner 调度使 max 达毫秒，不能拿该 max 推断生产接收尾部。

同一 runner 的 322 B warm loopback 提交，10,000 次采样、500 次预热，单位 µs：

| 形式 | p50 | p99 |
|---|---:|---:|
| send | 1.688 | 1.976 |
| sendmsg 单 iovec | 1.753 | 2.048 |
| sendmsg 双 iovec | 1.768 | 2.064 |
| writev | 1.776 | 1.840 |
| MSG_ZEROCOPY | 2.160 | 2.456 |
| sendmsg + uncork | 2.096 | 2.152 |

零拷贝全部 10,500 次完成均为 copied（含预热），且表中没有计入 errqueue 开销。16 KiB 报文也为 copied，sendmsg/zerocopy p50 为 2.216/3.360 µs。SO_SNDBUF 请求 16 KiB、256 KiB、1 MiB 时，322 B sendmsg p50 为 1.752、1.744、1.752 µs，未显示有用收益。空闲 1 ms 后 sendmsg/send p50 为 2.288/2.504 µs，p99 为 6.920/8.193 µs；睡眠影响调度与缓存，不能据此在生产周期发包。

这里可重复测得的是 warm 小包提交约 1.7 µs 的实验量级，不是统一升级主机的物理 wire 下限。send 相对 sendmsg 仅约 0.065 µs，且 idle 结果方向相反，暂不扩展热路径系统调用允许表或改变生产调用；没有证据支持靠 socket 参数把原包围段 4.2–4.4 µs 直接压到此值。真实目标机必须用 send_syscall 的前后点复测后裁决。
