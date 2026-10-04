# Agent Note: sequencer 主循环逐条处理深度消息,每条之后先看高优先级环

Status: proposed

> **第八轮第五次修订(2026-10-03)**:收窄第 1 条——每处理完一条 BBO 或 advice 也查看 ctl 与实盘 rep 环头(不只在每条 md 之后);md 与 bbo 循环中把下一个定时器到期时刻与当前 TSC 比较一次,到期即让位;环头检查的预算按环数写。关键帧不再按节分片,见 [keyframe-one-shot](2026-10-03-keyframe-one-shot.md)。依据 [round8e 简报](../../../../design/_work/round8e-spec-brief.md) §3(W5 N-P1-3、P2);规格见 [hft-sequencer](../../../../crates/hft-sequencer/README.md)。

> **第八轮第六次修订(2026-10-03)**:收窄第 2 条——删除 `md_levels_per_round`,md 每轮只按时间片 `md_round_ns` 限额(T10 的"每轮至多 K 条"在 md 上按时间计);参数归 [hft-sequencer](../../../../crates/hft-sequencer/README.md)。

## Problem

sequencer 的 `step()` 先按优先级一次性取完一批(ctl 4、rep 64、bbo 128、advice 16、md 128、rep-sim 32),再逐条处理。取批之后到达的 BBO 要等整批处理完:128 × `seq/l2batch_20` 的 1–3 µs ≈ 128–384 µs;Binance 深度每 100 ms 集中推送,这种停顿每 100 ms 出现一次,与 latency-budget "BBO 最多等正在应用的那一条 L2Batch"的承诺矛盾。唯一针对它的基准依赖可选的 mock-exchange,附加断言"bbo 环非空时从不取 md 环"也无法从 journal 顺序核验([V5 评审](../../../../design/_work/review-2026-10-03-M0c-V5延迟与整体.md) P0-2)。

## Proposal

规范解法见 [round8d 简报](../../../../design/_work/round8d-spec-brief.md) L-P0-2。主循环归 [hft-sequencer](../../../../crates/hft-sequencer/README.md),预算归 [latency-budget §2.3](../../../../docs/latency-budget.md),基准归 [benchmarks](../../../../benchmarks/README.md)。

1. 驱动生成的 Input、ctl/hk、实盘 rep、bbo、advice 按原优先级取并处理;**md 改为逐条处理**:每处理完一条 md 消息(`L2Batch`、`Trade` 等)即查看 ctl、实盘 rep、bbo 环头(各一次 Acquire 读),非空则先取并处理它们(本轮各自的上限照旧计),再继续 md。取出顺序照常写进 journal,确定性不变。
2. md 每轮上限改为 `md_levels_per_round`(默认 4,096 档)与 `md_round_ns`(默认 50,000 ns)先到者;`rep-sim` 仍在最后。
3. latency-budget §2.3:BBO 在 sequencer 侧最多等正在应用的那一条 md 消息加一次环头检查。
4. 基准 `seq/l2burst` 改为不依赖 mock 的单进程基准(生产者线程注入深度突发与 BBO),放进 M2,断言"BBO 等待 ≤ 一条 md 消息的应用时间 + 检查开销"。

## Alternatives considered

**缩小 md 每轮条数(例如 8 条)。** 停顿缩短但仍按条数而不按工作量计;一条 1,000 档的快照分片与一条 1 档增量代价相差两个数量级。

**BBO 与深度进不同的 sequencer。** 打破单写者:簿、仓位与策略状态只能有一个写线程。

**按时间片抢占(定时中断)。** 热线程没有定时器与信号,环头检查是一次缓存行读,足够便宜。

## Acceptance criteria

- `seq/l2burst`(M2):深度突发下 BBO 在 sequencer 侧的等待 p99 ≤ 一条 md 消息的应用时间 + 检查开销(数值归 latency-budget)。
- 回放:同一 journal 在新旧主循环下的意图流逐字节一致(取出顺序写在 journal 里,处理顺序由 journal 决定)。
- `md_levels_per_round`、`md_round_ns` 在 ECF1 sequencer 段,超出默认值的配置有直方图依据。

## Risks

- 每条 md 后多三次 Acquire 读,深度吞吐略降;按 `seq/l2batch_*` 基准确认在预算内。
- md 在 BBO 持续高频时可能被饿;`md_round_ns` 与每轮上限保证 md 每轮至少前进一条。
