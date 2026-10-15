# Agent Note: 先消费已缓冲的 wire 数据

Status: implemented

## Problem

帧数限额可以留下已完整接收的 TLS 记录或 WS 帧。下一轮直接 recvmsg 会重复压缩缓冲，并在内核无新数据时多做 EAGAIN 系统调用。gateway 已有批缓冲，逐项取环仍会逐项写共享 tail。

## Decision

在 hft-net 增加 poll_rx_if_needed，BBO feed 用它先消费完整的缓冲数据。部分记录继续读 socket，避免永久卡住。保留 poll_rx 直接读取契约。gateway 通过 read_batch 复制当前可取批次并一次 release，然后按原顺序规则处理。

## Alternatives considered

只判断 rx_buffered 非零会让部分记录永远等不到剩余字节。read-until-EAGAIN 会在短读之后补一次系统调用，并让突发扩大单次任务工作量。recvmmsg 不提供 TCP 消息边界。内核 busy-poll 违反现有 IRQ 隔离契约，未取得 aarch64 测量证据，保持关闭。

2 KiB 脚本 backlog 的 p99 在缓冲优先模式回退，尽管平均读取次数更少。仅 BBO feed 启用此入口，深度与 gateway 保留直接轮询。理由与完整结果在测量报告中。

## Consequences

每个连接内消息、输出与读取戳仍按原有顺序。批次只包含一次 Acquire 观察到的命令，之后到达的命令留待下次 drain；配额和重排规则不变。测试覆盖部分记录、跨 TLS 帧、完整明文和密文缓冲、时间戳与 gateway 公平性。测量边界和待验证项见 [报告](../../../../benchmarks/reports/2026-10-15-wire-io.md)。
