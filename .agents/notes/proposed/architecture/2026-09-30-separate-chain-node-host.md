# Agent Note: BSC 节点从第一天起放独立的同 CPG 实例

Status: proposed

## Problem

早期设计让 bsc-geth 与 hft-core 同机([13 R9](../../../../design/13-设计评审与优化.md),原 01 §7、02 §2.2)。两个问题:c7i.metal-24xl **没有实例存储 NVMe**,BSC 全节点要么用 io2(贵)要么用 gp3(慢);更重要的是 geth 的 GC、压缩与内存带宽会污染同一颗 CPU 的 L3(8488C 单路 105 MB L3 共享),直接拖 hft-core 的 p99。

## Proposal

[01 §7](../../../../design/01-总体架构.md)、[02 §2.3](../../../../design/02-硬件与托管方案.md):

1. BSC 节点(bsc-geth,之后评估 reth-bsc)放在与 `tokyo-cex` **同一 CPG** 的 i7ie/i4i 实例上(本地 NVMe ≥ 4 TB)。
2. 本 Note 只在启用可选扩展(builder profile + 自建节点)时适用;core 的 free profile 不需要自建节点([chain-core-free-profile](2026-10-01-chain-core-free-profile.md))。启用时 chain-core 可跑在 hft-core 主机上经 CPG 内网读节点(`eth_call` 验收 < 1 ms),或与节点同机经外部 venue 协议(同 CPG 的 UDP)接入 hft-core,放置由实测决定。
3. 若因故必须同机,用 Intel RDT/CAT(resctrl)给热核划 L3 份额;由 [hostprobe](../../../../tools/README.md#hostprobe) 在部署镜像上验证 metal 上 resctrl 可用(假设 A12)。
4. 成本:i7ie.2xlarge 级别约 $0.7–1k/月(需要老板拍板,[13 §7](../../../../design/13-设计评审与优化.md))。

## Alternatives considered

**同机 + io2 Block Express。** 解决存储但不解决 L3 与内存带宽污染;io2 按 IOPS 计费,全节点的随机读写代价高。

**同机 + resctrl 分区。** 可以限制 L3 占用,但内存带宽(MBA)控制粒度粗,且依赖 A12 成立;作为兜底而不是默认。

**只用付费 RPC,不自建节点。** 公共/付费 RPC 的延迟与可用性不受控;9/23–24 公共节点全部 403 五小时([postmortem](../../../../docs/postmortem/README.md));链上腿需要 L0 事件驱动 < 50 µs 的池状态,只能来自自己的节点。

## Acceptance criteria

- hft-core 在 chain-core 与节点满载(同步 + 高频 `eth_call`)时,`tick_to_trade` p99 与节点空闲时相比上涨 < 5%。
- 从 chain-core 到节点的 `eth_call` < 1 ms;块流领先节点 ≥ 50 ms([11 §3](../../../../design/11-实施路线图.md))。

## Risks

- 跨实例多一跳 CPG 内网(约 20 µs 量级 RTT):链上路径以 ms 计,可忽略。
- 多一台实例的运维面:沿用同一发布与调优即代码纪律。
