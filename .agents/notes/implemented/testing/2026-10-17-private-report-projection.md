# Agent Note: 私有订单回报字段投影

Status: implemented

## Problem

orders-spin-02 的长订单回报重复扫描整个 JSON；Binance 事件分派后再次解码订单，Bitget 单行回报也构建最大容量订单数组。帧延迟的旧测量包括同一 TLS record 内前序消息处理，不能将约 11 µs 全算作 WS 复制。

## Decision

Binance 现货与永续订单使用单遍嵌套投影，Bitget 只投影恰好一个订单的数组；既有归一化、累计量、费用、外部成交、错误与去重逻辑保持同一入口。Bitget feeDetail 保留严格解码。重复选定字段、别名、转义、非 ASCII、异常数字和多行回报仍走参考解码器；投影不修改账户状态。

BIN-026 与 BGT-027 用转义未知字段强制参考路径，逐字节变异订单报文并比较所有结果及事件；重复报文和后续有效报文检查去重状态，Bitget 还逐次比较完整上下文。现有多行原子失败与费用用例继续运行。

`private_wire` 使用固定合成回报及带 2 KiB 未知元数据的版本，在计时外建立上下文。GitHub 原生 ARM 同机以相同 harness 跑 base/head/base，输出新成交与重复成交的分位数；共享 CI 只报告，不冒充统一升级主机验收。

## Alternatives considered

直接替换所有 JSON 解码会扩大语义风险；只投影第一行会丢失后续订单和整帧校验。服务端 WS 帧不允许掩码，增加解掩码路径会违反现有协议验证。当前未分片帧已经借用缓冲，不因旧 frame 数字重写该路径。

## Consequences

没有资金、风控或 journal schema 语义变更。目标主机需要复测真实回报 decode、分离后的 frame 和 rep_to_order；合成与补齐报文不能替代真实线上分布。ARM 基准见下表。


## Evidence

[Latency profile 38998416226](https://github.com/Fiora-gwx/hft/actions/runs/38998416226)，2026-10-17，GitHub ubuntu-24.04-arm、4 vCPU Neoverse-N2，bench 优化 profile。base 为 fd2dc3b，head 为 facd1158；同机 base/head/base，相同 harness，每格 20,000 次采样、2,000 次预热；计时包含 Instant 边界开销，不包含账户上下文构建或销毁。单位 µs。base 范围为前后两次；全部为新成交。

| 报文 | base p50 | head p50 | base p99 | head p99 |
|---|---:|---:|---:|---:|
| Binance 262 B | 1.488 / 1.488 | 1.152 | 1.520 / 1.520 | 1.176 |
| Binance 补齐 2324 B | 2.968 / 2.800 | 2.176 | 3.104 / 2.992 | 2.528 |
| Bitget 520 B | 3.296 / 3.200 | 1.383 | 3.424 / 3.287 | 1.416 |
| Bitget 补齐 2582 B | 3.600 / 3.512 | 2.360 | 3.759 / 3.599 | 2.640 |

正常报文重复成交 p50：Binance 1.064/1.088 → 0.744 µs，Bitget 2.976/2.976 → 1.296 µs。收益支持保留投影；不把共享机器的 max 或合成回报数字换算为实盘 rep_to_order 达标结论。

独立 DST file replay 38999494697 的 100 个派生 seed 通过，39 个固定回归 seed 在导出前因 `fixture destination already exists` 失败。恢复的 Rust 缓存含旧 `target/dst`，而 runner PID 重复，导致证据目录碰撞。工作流在恢复缓存后、生成本次证据前只清除旧 `target/dst`；不清编译制品，不跳 seed，不覆盖同一运行的失败证据。本次生成的完整证据仍上传归档。
