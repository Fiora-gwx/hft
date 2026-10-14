# Agent Note: 交易世界的账户状态边界

Status: implemented — 草稿 PR 实现,资金与风控决定待负责人评审

## Problem

持续开平仓暴露了初始化、在途与恢复之间的状态缺口。首条余额可能已经包含尚未交付的成交;平仓后旧 mark 继续限制账户;Unknown 回报里的累计成交被丢弃;风险缓存落后于 OMS 当前腿状态。

## Decision

选择资金安全行为。[账本](../../../../crates/hft-ledger/README.md)首次初始化也等待 quiet,冷读者消费同事件 BalanceInit 证据。[OMS](../../../../crates/hft-oms/README.md)对 Unknown 的累计成交只记增量,部分成交保留 Unknown;第一腿身份丢失不允许无约束重试。[sequencer](../../../../crates/hft-sequencer/README.md)准入前刷新腿摘要,空仓且无永续在途时重投影保证金,仍要求报告新鲜。

## Alternatives considered

只让模拟世界提前初始化余额会掩盖真实交易的交付竞态。完全跳过空仓保证金检查会漏掉在途永续订单及报告失联,故只免除无风险品种的 mark 过期。延长墓碑保留期不能保证活跃链接永远保留第一腿槽,故按当前 group 定位并保守拒绝不明身份。

## Acceptance criteria

定向回归覆盖重复累计成交、空仓有/无 mark、墓碑过期及已固定交易种子。标准交易世界至少 300 种子各 10000 步零失败;冒烟 INV-15/19/20 比较次数均大于零,完整回放与 CI 通过。

## Risks

初始化可能因持续在途而推迟,该阶段不采纳报告金额为持仓证据。历史反例保留 legacy 世界;标准随机运行使用交易世界,不重生成 HashTrail 金样。资金语义详见 [待评审记录](../../../../docs/changes.md)。
