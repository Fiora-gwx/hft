# Agent Note: M2 契约对齐与测量隔离

Status: implemented

## Problem

SDK 的模式表与拥有意图处理顺序的 sequencer 规格存在冲突。SDK 写 Shadow 完整执行 evaluate,sequencer 则规定在模式门记录 Shadow 决策。SDK 的 Hedge 列也混合了无链接的策略意图与 OMS 自动第二腿,两者在风控中分类不同。

已有测试日志还显示 JNL-105 读取进程级缺页计数时受到同进程其他测试影响。PY-161 连续扫描时,前一轮留下的驻留堆会改变后一轮的内存基线,两个不同 journal 长度的测量没有相同起点。

## Decision

SDK 的说明和引擎侧测试遵循 [sequencer 意图管线](../../../../crates/hft-sequencer/README.md#意图管线) 与 [risk 意图分类](../../../../crates/hft-risk/README.md)。Shadow 在模式门记录决策,不占用下单资源;无链接 Hedge 仍按 Open 分类,OMS 自动第二腿走已有的风险递减路径。这个口径不能证明 Shadow 已覆盖完整风控检查链。

JNL-105 测量执行 append 的线程。PY-161 每次测量启动独立子进程,先用同一份 100 万行夹具预热,再读取指定 journal。批大小上限和两组测量差异小于 5% 的要求保持。

SDK-004 的真实装配验收与 SDK-005 的 M2 要求保留。工具侧的假仓库测试不替代它们,本批不提高 current_milestone。

## Alternatives considered

**只沿用 SDK 的旧表格** 会让两份规格继续矛盾,并把无链接 Hedge 误写成可绕过 CloseOnly。模式门和分类规则已有明确归属,此处统一引用它们。

**放宽测量阈值** 无法区分其他测试线程的缺页与被测写入的缺页,也不能消除不同驻留堆基线的偏差。因此修正隔离方式,保留原验收要求。

## Consequences

PY-161 多启动几个短生命周期进程,检查时间略有增加。正式验证以对应提交的 CI 为准。真实 DST 中尚未暴露的观察项仍是覆盖缺口,通过种子扫描不等于所有不变量都经过有效检验。
