# Agent Note: 离线 Model 重算的起点与配对

Status: implemented

## Problem

普通状态快照不含 Model 的累计样本。在任意快照创建空 Model 会把缺起点误报为通过或算法分歧。Model 可以无持久字段却仍生成 Advice,因此空布局也不能代表 NoModel。同一回调连续 emit 同键时,比较任意一条相同载荷又会放过实盘 outbox 不可能留下的旧值。

## Decision

冷回放按实例和独立运行寻找创建、检查点或丢弃注记。未找到就报告 model_no_start,不借用上一次运行的对象。SDK 用关联 Model 的具体类型识别 NoModel。重算保留同一五元键的最后一次产出,每条只匹配一次,其余计 coalesced。检查点要求提前重建视图时,普通 Output 验证也覆盖所需前缀。契约归 [replayer verify](../../../../apps/replayer/README.md#verify) 与 [sequencer 冷装配](../../../../crates/hft-sequencer/README.md#装配与运行由-appshft-coreappsreplayerhft-testkit-调用)。

## Alternatives considered

- 直接复用在线 advisor 的最新快照冷起步。它服务在线继续运行,不能证明离线窗口包含 Model 起点。
- 以空 LAYOUT 或零 mem_bytes 判定 NoModel。会跳过合法的无状态 Model。
- 从一组同键产出中寻找任意字节相同项。会接受已经被后续同键 emit 覆盖的旧载荷。

## Consequences

只影响默认关闭的 replay-io 冷入口与 SDK 查询,不改 journal 和快照格式。SEQ-403 验证最终产出及重复消费,RPL-266–269 使用真实 advisor、文件写者和检查点验证装配。完整 T1/T3、路径版本变化和跨重启的长时 CLI 验收仍须按原规格完成,短样本不替代它们。资金、恢复相关改动保留负责人评审。
