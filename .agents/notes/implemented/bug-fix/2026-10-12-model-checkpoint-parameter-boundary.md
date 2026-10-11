# Agent Note: Model 恢复参数在检查点边界应用

Status: implemented

## Problem

检查点恢复要求先把原始状态与路径状态读回,再用检查点处的参数应用一次,随后续读输入。把参数回调放在整个镜像批次结束后,会允许同批后面的行情或 SetParams 先进入 Model。结果依赖批大小和检查点在批中的位置,只比较最终累计样本数无法发现这个顺序问题。

## Decision

advisor 在最早待恢复的实例边界暂停输入交付,应用该处参数后再继续。一个输入产生的所有输出仍完整处理。多个实例各自解除等待,Output/Meta 边界以其前缀输入已全部处理为准。此控制仅在 advisor,不改 sequencer 主循环和持久格式。契约归 [模型检查点](../../../../crates/hft-sequencer/README.md#模型检查点),恢复调用顺序沿用 [预热与检查点键](../../proposed/architecture/2026-10-03-warmup-injection.md)。

## Alternatives considered

**固定 batch = 1。** 可以缩小问题,但检查点落在 Meta 时仍需显式处理,也无须让整个追赶过程永久失去批处理。

**保留批尾回调,只检查最终状态。** 参数会在错误时间影响 Model,后续同值回调或累计计数相等不能证明 Advice 产生时参数正确。SDK-066 直接记录每次行情回调已经应用的参数。

## Consequences

追赶期间可能多返回一轮 Busy,每批仍不超过配置上限。SDK-066 覆盖不同批大小和检查点位置,SEQ-402 对照两个不同检查点与不中断运行。重算入口可沿用这一事件顺序,不能以降低 batch 代替边界契约。
