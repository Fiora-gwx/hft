# Agent Note: 门槛 ⑩ 的硬线、目标与证据

Status: proposed

> **第八轮第六次修订(2026-10-03)**:补充第 2–4、6 条——`LatencySample` 追加 `action`(New/Cancel/Amend),`order_rtt` 只统计新单、`rep_to_order` 含三种;`MarketHdr.rx_empty_gap_ns` 让 ⑩ 报告并列 `kernel_rx` 的上下界,探针按突发/平静分层抽样;HW RX 时间戳不可用或 `unsynced_cal` 占比偏高时改用 SW 锚数据集(数值门槛不变,注明不含网卡到软中断一段);同机干扰先做 1–2 天 A/A 估计区间宽度,开关顺序按资金费块分层配平,区间过宽时延长窗口而不判失败。判定归 [testing §8.2](../../../../docs/testing.md#82-延迟验收的测法);依据 [round8f 简报](../../../../design/_work/round8f-spec-brief.md) F2。

## Problem

门槛 ⑩ 的写法有三处站不住:基准形态 22/75 µs 被多处写成硬门槛,而 testing 只把验收下限当硬线;`kernel_rx` 等跨时钟域分段用 60 s 一条的校准对阶梯换算,误差可达 6–60 µs,与预算同量级;按时间抽样的探针低估突发尾部,`hedge_host` 只能在另取的 7 天测试网行情窗口上测,同机干扰的"≤ 5%"落在统计噪声里([W5 评审](../../../../design/_work/review-2026-10-03-M0d-W5延迟与整体.md) N-P1-4、N-P1-5;[W4 评审](../../../../design/_work/review-2026-10-03-M0d-W4运行与交付.md) P1-4)。

## Proposal

用户决定按推荐执行([round8-decisions](../../../../design/_work/round8-decisions.md) R8-11;规范见 [round8e 简报](../../../../design/_work/round8e-spec-brief.md) E2)。判定归 [testing §8.2](../../../../docs/testing.md#82-延迟验收的测法),数字与分段归 [latency-budget](../../../../docs/latency-budget.md),离线统计归 [hft-telemetry](../../../../crates/hft-telemetry/README.md) 与 [replayer](../../../../apps/replayer/README.md)。

1. **硬线 = 验收下限**:`tick_to_trade` p50 ≤ 30 µs、p99 ≤ 100 µs;新分段 `rep_to_order` p50 ≤ 35 µs、p99 ≤ 110 µs。基准形态是目标;落在目标与硬线之间通过,差距与原因写验收记录并由 Admin 签字。p99.9 只记录。
2. **时钟校准**:每 1 000 ms 一条 `ClockStatus`;按样本前后两条校准对线性插值,逐样本给误差界,> 1 µs 的样本计入 `unsynced_cal`、不进分位数;HW 时间戳不可用时以 `sw_rx` 为锚。
3. **`rep_to_order`**:回报帧 `hw_rx` → 由它触发的下一单 `sent_tsc`,覆盖对冲路径;latency-probe 的 `quote` 模式在主网行情负载下每天产生大量样本;`hedge_host` 是它的子集。
4. **按事件加权的离线统计**:从 seq journal 对每条 `Bbo`/`L2Batch` 算 `kernel_rx`、`ingest`、`feed_host`(`event_weighted()`、`replayer latency`),热路径零成本。
5. 只有一个 7 天主网行情窗口;`pair` 模式只作小样本确认。
6. 同机干扰:2 小时一块、块内先开后关的顺序按记录的种子随机,块配对,bootstrap 95% 置信区间上界 ≤ 1.05;每组两侧 n ≥ 10,000 才作硬判定。

## Alternatives considered

**保留 22/75 为硬门槛。** 首次实测前没有证据支持这一组数,硬门槛只会逼出"改口径"或"放宽重签",不如事先写清目标与下限的关系。

**保留单独的 7 天测试网行情窗口。** M7 至少 14 天,而测试网行情负载与生产差得远;`rep_to_order` 在主网负载下测同一路径,样本更多。

**每小时交替开关 sim、相邻小时配对。** 一天 24 小时是偶数,资金费结算都在偶数小时,配对消不掉这种混杂。

## Acceptance criteria

- testing §8.2、latency-budget、host-upgrade、launch 对 ⑩ 的写法一致:硬线与目标两组数只出现在归属地,其他文档链接。
- ⑩ 报告附校准误差分布与 `unsynced_cal` 计数;`ingest`/`kernel_rx` 来自按事件加权的统计,订单类分段来自探针样本。
- 同机干扰报告给出种子、块数、各组样本量与置信区间。

## Risks

- PHC 在 c8g 上不可用时只能以 `sw_rx` 为锚,`kernel_rx` 的含义变为"软件接收到用户态";待 M1.5 核实。
- 每组 n ≥ 10,000 在 7 天内对低频分段可能凑不够;不足的组只记录,不作硬判定。
