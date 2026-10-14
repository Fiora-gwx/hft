# Agent Note: 有界资金费差策略的首个实现

Status: implemented

## Problem

冻结研究规格依赖历史分位 Model、截面分配与未实现的 pipeline,当前任务要求先交付净年化信号、双永续方向与较少的参数。真实资金费输入还存在独立引擎缺口,不能混入策略依赖。

## Decision

采用整数净年化差、人工名单/参数和 NoModel,等量双腿交给 SDK 执行目标。两腿分别限定资金份额与硬上限,CloseOnly 与禁用都退出。历史模型、私有命令和插件不作为当前契约。语义变化列入 [changes](../../../../docs/changes.md) 待评审,规格归 [策略 README](../../../../strategies/portfolio-carry/README.md)。同交易所组合需要负责人决定账户模型后另行实现。

## Alternatives considered

一次实现历史分位与截面分配会把本次资金费主链路任务扩展到已推迟的 Model 检查点。直接在策略里计算真实资金费会破坏单写者和回放,因此实际账单走独立引擎 PR。改变账户模型以放行同所组合涉及 SDK/refdata 与恢复,不能由本策略绕过。

## Acceptance criteria

两个永续方向各四组确定性输入均完成 linked 双腿成交、一次实际资金费入账、重复不重记及收益衰减平仓,且 replay verify 零差异。参数字段 13 个;性能预算待参考机验证。

## Risks

年化差是预测,未计未实现基差波动的收益保证。费用摊销、未知资金费周期、实际到账延迟和阈值需要模拟账户研究。策略没有动态截面调仓或自动借币。
