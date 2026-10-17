# Agent Note: 模拟订单目标身份与 DST I8 诊断

Status: implemented

## Problem

main dc523111 的 [CI 39000761210](https://github.com/Fiora-gwx/hft/actions/runs/39000761210) 在 core seed `0xdc523111e89863ee` 第 8217 步报 I8。实际错误为 `missing Input observation at seq 15769`,没有发生对冲等式差异。

seq 15711 发出账户 2、品种 2、slot 52/gen 7 的 Cancel。SimExchange 仅按 slot 查目标,返回旧 gen 2、exch_order_id 111、cum_filled 100000 的 CancelReject。OMS 找不到这代槽或墓碑,不能把无法证明来源的旧累计成交重复记入现金,故以 `OmsCash(Untracked)` 停机。DST 自动重启裁掉未完成 Input,步记录仍带该 Input,观察完整性失败被归到 I8。

目标查询实现来自 2026-10-09 的 `8e4069c52`,早于近期测量、Kill strict-zero、修复节奏与策略形态合并。本次固定种子暴露了旧的模拟器身份缺陷。

## Decision

撤单、改单和查询共用按 `(account, inst, target_slot, gen)` 查找的目标函数。保留的历史代可查,不能用最新 slot 记录代替;未命中响应携带请求身份与零成交、零现金事实。OMS 的不明现金防御及全部 DST 不变量保持原有要求。

## Alternatives considered

忽略旧代现金回报或放宽 I8 会掩盖重复记账风险。延长墓碑保留只能推迟失败,不能阻止模拟器撤改另一代挂单。仅在 DST 层过滤回报不能修复实时模拟账户共用的撮合核心。

## Consequences

模拟结果可能改变,因为错误目标不再影响其他订单或借用其成交事实;生产 OMS、对冲、Reverse 与 Kill 资金规则没有修改。模拟资金语义列入 [changes](../../../../docs/changes.md) 待评审。不重生成 HashTrail 金样。

## Testing

SIM-100/101/103 覆盖已成交旧代、复用后的当前挂单、未知代及错误品种,核对回报身份、累计量、现金字段、锁定余额与当前代可操作性。失败 seed 按 `world=trading` 固定在 [core.txt](../../../../fixtures/dst-seeds/core.txt),完整 10000 步验证包含 journal 独立重建。
