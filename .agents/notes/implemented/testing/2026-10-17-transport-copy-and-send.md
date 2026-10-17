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
