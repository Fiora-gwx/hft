# Agent Note: Model 检查点使用显式版本与规范文件格式

Status: implemented

## Problem

advisor 用布局描述的调试文本哈希代替 `Model::LAYOUT_VERSION`。只升级版本号时,旧检查点仍会被接受;仅改描述文本时却可能被丢弃。内存检查点的路径首块也将长度写在版本前,与文件规格相反。编码与恢复采用同一错误顺序,单独做往返测试无法发现。

## Decision

`StrategySet` 与装配宏导出真实布局版本,advisor 直接使用它。公共冷路径编解码器落实既有文件格式,提供可借用的键、路径与样本视图,不承担文件发现和原子落盘。引擎时间从 journal 取得,不另塞进保留字段。规格及行为归 [sequencer](../../../../crates/hft-sequencer/README.md#模型检查点)。

这落实 [预热与检查点键](../../proposed/architecture/2026-10-03-warmup-injection.md) 和 [路径状态节](../../proposed/architecture/2026-10-03-model-path-section.md) 的既有决定,不改变 journal 或引擎快照格式。

## Alternatives considered

**保留布局描述哈希。** 它不能表达策略显式宣布的不兼容版本,也把描述文本当成兼容性承诺。SDK-065 与 SEQ-047 固定了只改版本号的反例。

**只测编码和解码互相往返。** 两侧共享错误时仍会通过。SEQ-399 使用按字段偏移与独立 CRC 算法取得的固定金样,SEQ-400 单独扰动长度、身份、分节和保留位。

## Consequences

手写 `StrategySet` 必须提供版本查询,不会静默落回描述哈希。SEQ-401 通过真实临时文件恢复后对比完整 Advice 字节与 Model 累计状态。完整 `--advice recompute` 的来源选择、比较区间和 CLI 仍需继续实现;本次不把格式编解码视为重算验收完成。
