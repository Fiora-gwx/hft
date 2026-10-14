# Agent Note: 非正式账户测试环境启动与只读观察

Status: implemented

## Problem

新进程缺外部账户基线、私有流认证和周期查询，无法满足启动闸门。既有 journal 恢复未装配，外部仓位无法推断策略 group。

## Decision

新 journal 的非正式 Live 账户先验证规则/配置/身份、撤销遗留订单并查到空，再读取余额与持仓。空账本建立账户 unassigned 成本并写完整启动快照。运行中的观察先入 journal，只检查余额/持仓差异；RateOnly 真实结算与合成历史比较，不重复记现金。Demo Unverifiable 使用相同 REST/WS key，仍需人工身份确认。

## Alternatives considered

拒绝所有非零仓位会阻碍测试账户验证；猜测 group 会制造错误归属；REST 覆盖账本会隐藏漏报。均不采用。

## Consequences

资金/风险语义在 [changes](../../../../docs/changes.md) 待评审。旧 journal 仍报 RecoveryRequired。mock 不替代真实 demo 的字段、认证、断线窗口及死手验收。操作归 [runbook](../../../../apps/hft-core/RUNBOOK-testnet.md)。
