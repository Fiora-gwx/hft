# 延迟预算

## 1. 时间点

| 时间点 | 含义 |
|---|---|
| `hw_rx` | 网卡收包 |
| `rx_tsc` | 读完 |
| `proc_tsc` | sequencer 取出 |

## 2. 分段名(全仓唯一)

| 分段 | 定义 |
|---|---|
| `kernel_rx` | `rx_tsc − hw_rx` |
| `ingest` | `proc_tsc − rx_tsc` |
| `decide` | 纯计算分段 |

## 3. CEX 路径预算

| 分段 | 目标 p50 | 目标 p99 |
|---|---|---|
| `ingest`(bookTicker ≈ 300 B) | ≤ 3 µs | ≤ 10 µs |
| `ingest`(深度 ≈ 2 KB) | ≤ 4 µs | ≤ 12 µs |
