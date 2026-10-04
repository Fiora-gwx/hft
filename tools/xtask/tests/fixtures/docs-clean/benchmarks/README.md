# benchmarks

### 单 crate 基准上限表

| 原语 | 归属 crate | 上限 | 出处 |
|---|---|---|---|
| OMS 分配槽 + client id + `OrderDecision` 构造 | [hft-oms](../crates/hft-oms/README.md) | p50 ≤ 300 ns、p99 ≤ 1 µs(分配:`decide` 预算的一部分) | 本表分配 |

### 基准名登记

| 基准名 | 组件 | 预算出处 | 用例 |
|---|---|---|---|
| `oms_place` | hft-oms | [hft-oms 延迟与资源契约](../crates/hft-oms/README.md#延迟与资源契约) | OMS-001 |
| `oms/alloc` | hft-oms | [hft-oms 延迟与资源契约](../crates/hft-oms/README.md#延迟与资源契约) | OMS-002 |
