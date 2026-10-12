# Agent Note: 无 client id 故障使用已有 Ack 的订单映射

Status: implemented

## Problem

`d05efd0` 的[严格现金对照](https://github.com/Fiora-gwx/hft/actions/runs/38230880362)在完整 220 种子中通过 218 个,另两个 core 种子分别在 591、597 步失败。制品 ZIP SHA-256 为 `80d2bf113886e437d56e9b719fd8a4c03fdbed7b937aa423f3528d67eb608146`。原始交付记录显示,账户 1 的永续订单号 2 已先收到带完整身份的 Ack,随后故障副本却强制使用 UNRESOLVED。严格结算入口拒绝未知累计现金,自动重启后的观察完整性检查先报出了缺失 Input。

这与[网关槽解析规则](../../../../crates/hft-venue/README.md#4-gwruntimepoll_once-的循环结构)不符。client id 缺失时仍要查同账户 IdTable,已到达的 Ack 足以归属该成交。原模型还保留了来自匹配器的 epoch/gen,因而也没有真正表达无法解析完整身份的回报。

## Decision

只修正 DST 的已知映射分支。模型按 venue、账户、交易所订单号保存 Ack 的完整身份及计划交付时刻;故障副本先清空 epoch/slot/gen,再使用不晚于成交的同品种 Ack 匹配。其他账户或未来 Ack 均不能作为当前归属证据。成交副本、原成交、数量、费用和现金事实均保留,由真实 OMS 去重。

不放宽 PERP_CASH 的未知累计量保护,不删除失败种子,不改变生产 OMS 的匹配契约。两个种子加入常规回归集。TK-128 检查同号跨账户、现金保留及未来 Ack 反例;TK-129 跑原完整计划和强制复跑。

## Deferred

此处的映射是固定 DST 运行内的简化网关观察,未实现 hold 等待、超时转外来成交、终态映射过期或晚到后重新归属。`within_hold` 的两个值只在已有映射时共用即时解析;这部分不计为 LK-4 通过。严格现金外来重叠、对账补量及完整恢复装配仍按[结算决定](../architecture/2026-10-12-perp-cash-settlement.md)保留明确错误。
