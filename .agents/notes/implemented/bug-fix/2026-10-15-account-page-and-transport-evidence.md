# Agent Note: 账户页面与传输失败的证据边界

Status: implemented

## Problem

warm 账户解码逐行修改上一次观察值,后续坏行可留下部分余额、持仓或杠杆。传输失败响应中的业务码也可能绕过 Unknown,产生确定拒绝或订单不存在证据。Bitget 空账单页带游标时被判为窗口结束。

## Decision

账户解码先写隔离的页面状态,全部校验成功后只替换该查询拥有的字段。撤单回执绑定请求身份。传输失败状态优先于业务码,限速仍通过独立控制提示传播。空资金费页携带继续游标时拒绝,保留窗口重试义务。

当前执行面仍是允许分配的 warm AccountProtocol。分步 AcctProtocol 的请求不借用凭证,而 Templates 不允许保留凭证;统一签名入口、完整配置步骤及 ADL 缓存须另行落地。此修复不声称满足 bounded 账户验收,不调整里程碑。

## Alternatives considered

在出错后清空 State 会丢失上一次完整观察,也不能表达页面事务。保留错误体的 GONE/NotFound 会把传输状态误当成交易所业务证据。把 key 放回 Templates 仅为实现旧接口会恢复秘密滞留。

## Consequences

warm 解码增加临时页面分配,热路径只增加状态分支。既有 50 KiB warm 基准包含此成本;性能结论仍需参考机。语义调整列于 [changes](../../../../docs/changes.md) 待负责人评审。验收归 [Binance](../../../../crates/venue-binance/TESTS.md) 与 [Bitget](../../../../crates/venue-bitget/TESTS.md)。
