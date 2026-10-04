# Agent Note: 不对真实 TLS 连接做投机式预加密

Status: proposed

## Problem

早期设计写着"每 tick 预加密到用户缓冲、触发时只剩一次 write"([13 R1](../../../../design/13-设计评审与优化.md),原 03 §6 的 `prepare()` 与 05 §4.2)。TLS 1.3 每条记录用递增序号构造 nonce:预先加密而不发送的记录无法丢弃——丢了,之后的记录序号错位,对端解密失败、连接断开;发了,就是一笔多余的订单。rustls unbuffered 的加密同样推进序号。**投机式预加密在 TLS 上不可能**,照原设计实现会在第一次"预热未发送"时断连。

## Proposal

[03 §3.2](../../../../design/03-网络与传输层.md)、[01 §3.4](../../../../design/01-总体架构.md):

1. 每个 tick 把下单路径走到**加密前一步**:sequencer 内策略 → 风控 → OMS 分配槽与 client id;gw 内明文帧模板填好 → WS 掩码完成;由一个标志决定是否真的出环与发送。
2. (**可选扩展,默认关闭**;当 `egress` 直方图与 `seq/bbo_fire` 冷热差证明有收益时打开)加密代码路径用一个**独立的 scratch AES-GCM 上下文**每 tick 对同长度假帧空跑一次,加密到丢弃缓冲,保持 I-cache 与分支预测热,不触碰任何真实连接的记录序号(`Conn::warm`)。
3. 真触发时,gw 线程对真实连接加密约 300 B(≈ 0.1–0.2 µs)再 `writev`(`Conn::send_frame`),这是真实连接唯一的加密点。

接口归 [`hft-net`](../../../../crates/hft-net/README.md);规则见 [hot-path-rules T5、T6](../../../../docs/hot-path-rules.md#线程t)。`egress` 预算不变(p50 ≤ 5 µs)。

## Alternatives considered

**预加密后在不触发时发一个"空"记录。** 仍是发到交易所的数据:要么是协议外字节(断连),要么是 WS ping 之类的真实帧(改变限速计数、增加噪声);且触发时刚好有一条已加密但内容不对的记录排在前面。

**每 tick 维护一份"序号 +1"的备用会话状态,触发时切换。** rustls 不暴露这种分叉;自改 TLS 状态机的风险远大于 0.1–0.2 µs 的收益。

**kTLS / 网卡 TLS 卸载。** 小报文无收益、失去用户态自旋控制;ENA 无 TLS 卸载([03 §3.3](../../../../design/03-网络与传输层.md))。

## Acceptance criteria

- 测试 `hft-net::tls_seq_advances_only_on_send`:`warm()` 调用 N 次后 `send_frame()`,对端逐帧解密成功。
- `hot-check`:rustls 记录加密入口的调用点只在 `crates/hft-net/src/tls.rs`。
- `gw-egress` 基准 p50 ≤ 5 µs、p99 ≤ 15 µs,且对端能解密每一帧([benchmarks](../../../../benchmarks/README.md))。

## Risks

- scratch 上下文与真实上下文的密钥调度不同,缓存预热效果可能不完全;以 `egress` 分段 warm/cold 对照基准实测。
- 真实加密留在触发路径上,可选的 Onload 形态追求 p50 ≤ 8 µs 时这 0.1–0.2 µs 仍在;可接受。
