# Agent Note: 无 client id 故障使用已有 Ack 的订单映射

Status: implemented

## Problem

`d05efd0` 的[严格现金对照](https://github.com/Fiora-gwx/hft/actions/runs/38230880362)在完整 220 种子中通过 218 个,另两个 core 种子分别在 591、597 步失败。制品 ZIP SHA-256 为 `80d2bf113886e437d56e9b719fd8a4c03fdbed7b937aa423f3528d67eb608146`。原始交付记录显示,账户 1 的永续订单号 2 已先收到带完整身份的 Ack,随后故障副本却强制使用 UNRESOLVED。严格结算入口拒绝未知累计现金,自动重启后的观察完整性检查先报出了缺失 Input。

这与[网关槽解析规则](../../../../crates/hft-venue/README.md#4-gwruntimepoll_once-的循环结构)不符。client id 缺失时仍要查同账户 IdTable,已到达的 Ack 足以归属该成交。原模型还保留了来自匹配器的 epoch/gen,因而也没有真正表达无法解析完整身份的回报。

## Decision

只修正 DST 的已知映射分支。模型按 venue、账户、交易所订单号保存 Ack 的完整身份及计划交付时刻;故障副本先清空 epoch/slot/gen,已到达的同品种 Ack 可即时匹配,限内稍后到达的 Ack 则让副本延至该时刻交付。其他账户或尚未到达的 Ack 均不能作为当前归属证据。成交副本、原成交、数量、费用和现金事实均保留,由真实 OMS 去重。

不放宽 PERP_CASH 的未知累计量保护,不删除失败种子,不改变生产 OMS 的匹配契约。两个种子加入常规回归集。TK-128 检查同号跨账户、现金保留及未来 Ack 反例;TK-129 跑原完整计划和强制复跑。

## Deferred

`b5e3779` 的[独立托管验证](https://github.com/Fiora-gwx/hft/actions/runs/38232740224)通过全部 222 个严格现金种子和 TK-127 的真实冷读窗口,但固定一致性集停在种子 43 的晚 Ack 边界,单元夹具还因未推进到回报交付时刻而缺 Ack。制品 ZIP SHA-256 为 `65e5ed8df1605ee0bab4246def2540e6b1f22b04b816165c556b4c5a642b296c`。据此补充限内等待、零等待反例,并把 43 加入完整回归集;旧失败不按通过处理。

此处的映射是固定 DST 运行内的简化网关观察,只模拟已排队 Ack 的等待,未实现尚未排队的 Ack、超时转外来成交、终态映射过期或晚到后重新归属。`within_hold` 的两个值不计为这些分支通过。严格现金外来重叠、对账补量及完整恢复装配仍按[结算决定](../architecture/2026-10-12-perp-cash-settlement.md)保留明确错误。
