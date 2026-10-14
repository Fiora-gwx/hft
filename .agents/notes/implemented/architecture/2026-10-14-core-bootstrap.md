# Agent Note: 热进程的开发装配入口

Status: implemented

## Problem

`apps/hft-core` 的完整进程规格覆盖多阶段交付，而现有 venue runtime 仍缺账户查询、用户流 bootstrap 与身份验证。负责人要求先获得可运行的装配和离线下单撤单闭环，同时限制引擎改动范围。

## Decision

进程直接装配 registry、sequencer、SPSC、journal、ctl 与泛型 venue runtime。离线模式用已有 SimExchange 和固定行情，仍经过正式策略、风控、OMS 与控制命令路径。真实账户的身份与账户闸门保留；不合成成功的交易所报告。Bitget 使用保留注册名，未知或未编译协议加载失败。

已有 journal 一律拒绝按空状态启动，直到恢复装配完成。线程停止后消耗 venue runtime 的所有权封 raw 段，persister 最后退出；这需要两个公开冷路径方法，未改变库的交易语义。文件路径式秘密来源仅用于当前开发/测试网入口，遵循本次负责人明确要求。

## Alternatives considered

**扩大引擎改动补齐账户网络层**：超出本次装配 PR 和“小型必要公开 API”约束，接口缺口作为后续工作交付。

**模拟账户配置成功或绕过启动闸门**：不能证明真实账户身份与仓位，不能作为测试网运行捷径。

**发现旧日志后继续空启动**：会失去账户与订单状态，因此拒绝启动并保留日志。

## Consequences

离线闭环可以独立验证装配；测试网联网装配不代表交易准备就绪。生产资源护栏、恢复、advisor 与真正账户初始化仍是上线阻断项。可执行子集与运行方法归 [hft-core](../../../../apps/hft-core/config/README.md)，不修改既有上线门槛或资金语义。
