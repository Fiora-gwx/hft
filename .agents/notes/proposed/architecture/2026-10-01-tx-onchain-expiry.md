# Agent Note: 每笔链上交易带链上过期:bundle 有效期不等于交易有效期

Status: proposed

> **第八轮(2026-10-02)**:适用范围收窄为 optional 的链上组件(cdex 迁移时启用);见 [ccex-first-scope](2026-10-02-ccex-first-scope.md)。

## Problem

早期设计用 bundle 的有效期(`maxBlockNumber`,2 块)让链上 `Unknown` 有界:"最多 2–3 块后必知",过期即 `Dropped`、零成本重来([06 §2.7](../../../../design/06-链上执行层.md))。第三轮红队评审指出这是资金风险:bundle 过期只说明 builder 不再按该 bundle 打包;bundle 里那笔**已签名交易在其 nonce 被消耗之前一直有效**。它可能泄露到公共 mempool、被某个 builder 留存、或在重组后被回灌,在我们判定 `Dropped`、甚至已经为它做了反向处理之后上链——正是 9/24"以为没成交、链上却成交了"的形态([postmortem](../../../../docs/postmortem/README.md))。独立的 N+1 出价转账有同样问题:它可以脱离主交易单独上链。

## Proposal

裁决见 第三轮裁决(2026-10-01 红队评审后,主控记录) C1;规则归 [defensive-patterns](../../../../docs/defensive-patterns.md#bundle-有效期--交易有效期):

1. 每笔交易经**最小包装合约**(SwapGuard)执行([contracts/](../../../../contracts/README.md)):`require(block.number <= maxBlock)`、`minOut` 校验、给 builder 的出价在合约内转出(InTx)。`maxBlock` = bundle 的 `maxBlockNumber`;走 Router 的路径同时设 `deadline`。
2. 状态机([`hft-chain-evm`](../../../../crates/hft-chain-evm/README.md)):`Dropped` 仅当**终局块号 > `maxBlock` 且该 nonce 未被本交易消耗**;此前一律 `Sent`/`Unknown`。迟到上链只会 revert,只花 gas。
3. 出价一律 InTx,不用独立的 N+1 转账。
4. 重来 = 新意图,重新规划 nonce;旧交易仍受 `maxBlock` 约束。
5. 授权(含 Permit2)在上币时完成,不在平仓路径上临时授权。

## Alternatives considered

**只依赖 bundle 有效期。** 见 Problem:签名交易的生命由 nonce 决定,不由 bundle 决定。

**过期后用同 nonce 发一笔 0 值自转账"作废"旧交易。** 需要多花一笔 gas 与一个块,且作废交易本身也可能丢失,形成新的 `Unknown`;合约内过期不需要任何额外动作。

**只用 Router 的 `deadline`。** `deadline` 是时间戳,BSC 0.45 s 出块下粒度与块号不一致;并且不覆盖不经 Router 的路径与出价转账。

**独立出价转账(N+1)。** 可以脱离主交易单独上链,白付出价;InTx 使出价与成交原子绑定。

## Acceptance criteria

- Foundry 测试:`block.number > maxBlock` 必 revert;`minOut` 不满足必 revert;出价只在成功路径转出。
- DST 故障"泄露/重组回灌后迟到上链"([testing §3.4](../../../../docs/testing.md#34-dst)):零仓位变化,状态机不产生对冲,`Dropped` 判定满足第 2 条。
- 生命周期属性测试:任何 `Dropped` 之前都观察到终局块号 > `maxBlock` 且 nonce 状态已知。

## Risks

- 包装合约是新的攻击面与审计对象:合约最小化(只含上述检查与转账),内部审计 + 小额验证后启用。
- 多一次合约调用的 gas;以省下的失败出价与避免裸腿抵消。
- 终局等待拉长 `Dropped` 的判定(约 1.125 s 终局 + `maxBlock` 余量),期间该 group 不开新仓——换来的是不会误判。
