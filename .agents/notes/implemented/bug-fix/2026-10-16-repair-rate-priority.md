# Agent Note: 修复单速率优先与定时器输入公平性

Status: implemented

## Problem

2026-10-16 testnet 的非对称双腿成交后，Reverse 被普通单秒级桶拒绝十次，终态至 Reverse 发送约 1001 ms。venue 桶已有风险递减预留，inst 桶却无差别扣减；OMS 的 RATE 重试固定等待，Exec 也使用一般门控退避，未利用令牌连续补充的最早可用时刻。

同次延迟证据中 35.945 ms 的 MD 等待实际在 carry-01。Timer 体哈希 `5f0ba6082600ed34` 对应 Sys/Roll，单事件约 37.568 ms；`845cfb8c161c553c` 同为 Roll，约 40.348 ms；`cf85ec2579b28092` 对应 Sys/Snapshot，约 17.741 ms。慢段位于 journal Input 后、exec 前。每次快照新建 64 MiB writer，与 live 的 `mlockall(MCL_FUTURE)` 组合会把大映射的锁页/缺页成本放到事件内。此外，16 个到期 Timer 后的中途检查能立即结束本轮，使持续积压的 Timer 跳过市场输入。

## Decision

风险递减分类继续由现有风控判定；普通单 inst 桶只对非风险递减单检查和扣减。所有单仍消耗 venue 预算，普通单不可消耗 `rr_reserve`，交易所发送侧硬窗口保持最终约束。风控给出适用桶及退避共同允许的最早时刻，OMS/Exec 登记一次到期重试，未获准尝试不额外施加切片/修复 cooldown；已有裸腿修复截止优先到期，不能因长退避延后 Broken。

主循环按数量与现有时间片限制到期 Timer，并在回调间检查输入；处理过 Timer 的轮次继续服务输入。快照 writer 移至启动期分配并复用，保持单写者、单事件原子快照与现有 wire 格式。契约归 [risk](../../../../crates/hft-risk/README.md#令牌桶)、[sequencer](../../../../crates/hft-sequencer/README.md#主循环step)。

## Alternatives considered

提高普通单限额不能保证修复容量；绕过交易所桶会超真实限额。给容量为一的 inst 桶切分固定比例会使普通单无法取得完整令牌。固定 100/500 ms 重试既可能空转也可能错过补充时刻。把快照分块会引入跨事件一致性与恢复格式变更；本次先消除已定位的大型临时分配，保留原子契约。

## Consequences

风控/修复时序变更在 [changes](../../../../docs/changes.md) 标为待评审。真实引擎覆盖耗尽普通预算后的非对称成交、Reverse 出环、归零与回放；到期边界与修复截止回归覆盖有界重试。Timer 测试覆盖过期堆不饿死输入；分配测试排除每次 Snapshot/Roll 重新申请 64 MiB。批次时间片是协作式边界，单事件仍须完成；参考机快照耗时与重跑实盘延迟验收继续按既有基准及门槛执行，本机测试不作生产延迟声明。
