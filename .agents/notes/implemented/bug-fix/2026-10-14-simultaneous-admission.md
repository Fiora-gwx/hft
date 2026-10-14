# Agent Note: Simultaneous 整对准入与恢复

Status: implemented

## Problem

同事件产生第二腿请求不足以保证同时下单。第一腿先 commit/place,第二腿随后可能因单笔限额、模式或槽容量失败;分别推环也可能只成功一侧。恢复按 Reverse 直接续接会在原始两腿仍 Live 时反向,而终态成交产生的反向若在 ledger 入账前检查,会读取旧可减量。

## Decision

负责人于 2026-10-14 要求双腿同发。sequencer 对两腿做只读完整准入,风险累计限额包含整对名义,减仓提示同时校验真实持仓方向,然后连续建槽并将两帧作为一对推环。环容量是发送环境事实,保持 journal 发布后的双 RingFull Input,不把未记录的容量引入决策。原子性仅覆盖本地准入与容量,网络和两个交易所的成交不具备事务性。

恢复等待原始两腿均终态,按持久化 on_fail 比较净成交;已在途的 Reverse 不重复发送。反向检查位于本事件成交和费用入账后。契约归 [sequencer](../../../../crates/hft-sequencer/README.md#意图管线)、[OMS](../../../../crates/hft-oms/README.md#simultaneous) 和 [risk](../../../../crates/hft-risk/README.md#热路径分类与检查)。

## Alternatives considered

先发第一腿再取消不能收回已发生的成交。只检查第二腿的链接字段缺少完整单腿检查与两腿累计名义。用未记录的环容量拒绝策略意图会使 journal 回放分叉。

## Consequences

保留既有 journal 事件和快照布局。新增 LiveIo 空闲容量接口必须由单生产者实现保守保证。资金、风控和恢复语义的审阅见 [changes](../../../../docs/changes.md),标记待评审。真实引擎种子覆盖拒单、部分成交、断连、快照回放和冷恢复,不更新 HashTrail 金样;延迟仍需统一升级主机实测。
