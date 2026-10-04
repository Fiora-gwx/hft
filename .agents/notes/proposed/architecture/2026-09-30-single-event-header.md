# Agent Note: 全系统唯一的 24 B 记录头,领域时间戳在事件体

Status: proposed

> **第八轮第五次修订(2026-10-03)**:事实同步——行情体公共前缀 `MarketHdr` 现为 48 B(增加 `sw_rx_ns` 等字段),第 2 条的 40 B 已过时;布局以 [event-model](../../../../docs/subsystems/event-model.md#记录头24-b全系统唯一) 与 [hft-event](../../../../crates/hft-event/README.md) 为准,记录头 24 B 的决定不变。

## Problem

早期设计有两份事件头定义:01 §5 的 `EventHeader` 32 B(含 seq/kind/venue/inst/三个时间戳)与 08 §2 的 `RecordHeader` 24 B(len/kind/flags/seq/tsc)([13 R5](../../../../design/13-设计评审与优化.md))。实现时一定会分叉成两套编码:venue 线程写一种、journal 写另一种,回放与 Parquet 映射出错,而且两套编码之间的转换本身就是热路径成本。

## Proposal

唯一定义放在 [event-model.md](../../../../docs/subsystems/event-model.md#记录头24-b全系统唯一),Rust 定义在 [`hft-event`](../../../../crates/hft-event/README.md):

1. **记录头 24 B**:`len: u32, kind: u16, flags: u16, seq: u64, tsc: u64`;seq journal 里 `tsc` = sequencer 取出时刻(`proc_tsc`),raw journal 里 = `rx_tsc`。
2. **领域信息在事件体**:行情体以 40 B 的 `MarketHdr` 开头(`venue, account, inst, ex_ts_ns, ex_send_ns, hw_rx_ns, rx_tsc`);回报体自带 `hw_rx_ns/rx_tsc`。
3. 环里传的就是 journal 里存的:feed 线程构造的字节与 sequencer 写进 journal 的字节是同一布局,sequencer 只填 `seq` 与 `tsc`。
4. 时间戳的名称、时钟域与分段定义只在 [latency-budget](../../../../docs/latency-budget.md) 维护。

## Alternatives considered

**32 B 头把 venue/inst/时间戳放进头里。** 对非行情事件(控制命令、定时器、快照)这些字段无意义却占空间;且"哪些时间戳属于头"会随事件类型变化,最终仍需体内时间戳。

**两层编码(环内一种、journal 一种)。** 多一次拷贝与转换;回放时要逆转换,一处写错就破坏逐字节一致。

**SBE 消息头。** SBE 头(blockLength/templateId/schemaId/version)适合跨语言协议;我们的内部编码以 Rust 为唯一来源([rust-first-event-schema](2026-09-30-rust-first-event-schema.md)),不引入 SBE 工具链。

## Acceptance criteria

- 仓库内只有一个 `RecordHeader` 类型;`hot-check`/`doc-check` 找不到第二个头定义。
- 编译期断言 `size_of::<RecordHeader>() == 24`、`align_of == 8`。
- `gen-schema` 生成的 Arrow schema 中每个时间戳列名与 latency-budget 的名称一致。

## Risks

- 24 B 头不带 venue/inst,冷读者按 inst 过滤需要解析体:行情体 `MarketHdr` 固定在体首,解析成本是一次定长读。
- `tsc` 在 raw 与 seq journal 里含义不同(`rx_tsc` vs `proc_tsc`):段头记录 `journal_id`,读者据此解释;文档在 event-model 中明确。
