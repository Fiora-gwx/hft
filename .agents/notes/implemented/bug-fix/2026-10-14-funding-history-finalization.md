# Agent Note: 认证资金费账单完成估算

Status: implemented

## Problem

费率推送不是实际付款。两个永续产品线缺少真实账单入口;RateOnly 估算之后再给账本实额会重复加账。账户级余额变动不带完整逐合约归属,不能按余额差推导资金费。

## Decision

使用认证的逐合约账单历史,在 warm 线程固定窗口分页、按账单 ID 去重并汇总完整结算总额,经 acct → gw → rep → journal 进入账本。RateOnly 的实额只补已记金额之差,保留原 group。Binance 默认同样采用 RateOnly + income 历史,避免同时启用余额推送和历史账单。此语义需要负责人评审,见 [变更记录](../../../../docs/changes.md)。规格归 [ledger](../../../../crates/hft-ledger/README.md#资金费) 与 [轮询驱动](../../../../crates/hft-venue/README.md#资金费账单轮询)。

## Alternatives considered

仅靠定时器估算无法包含交易所舍入与结算时实际仓位。仅靠账户余额推送缺少合约归属,也容易与成交、费用或转账混淆。等待 M9 自动对账会让主资金费现金链路一直缺失。本变更不启用 M9 对账自动校正。

## Acceptance criteria

协议拒绝整页错误与循环游标;运行时无部分分页输出且支持背压;实额与估算两种顺序、重复和迟到账单均只记正确净额;sequencer 验证重放一致。

## Risks

HTTP RestPool 与进程装配在当前基线仍未落地;调用者必须驱动 request/response/retry 并遵守账户/IP 权重。账单时间偏离已注册网格时明确失败,需更新 refdata。实际结算延迟与历史保留窗口需要目标账户验证;超过账本水位的旧账单由既有对账流程处置。
