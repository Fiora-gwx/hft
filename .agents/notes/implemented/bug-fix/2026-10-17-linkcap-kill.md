# Agent Note: 亚最小量链接退休与 Kill 容量隔离

Status: implemented

## Problem

c4 probe-spin-01 的旧 journal 在 31 条 First 后于 seq 4262 拒绝 CAPACITY。seq 7154 中 16 条链接全部为 Hedging,无在途,owed 为 1000 或 990。现货第二腿按净量入账,基础币手续费使已全成委托仍欠亚 lot 数量;全成不走失败重试,Done 又要求 owed 为零。提高表容量只能延后耗尽。Kill 的配对首腿同样要申请链接,因此 Applied 后仍无法下平仓单。

## Decision

关联订单均有终态证据后,不可下单的欠量交给 group residual,保留全部真实腿净量和现金。Done/Broken 取消 HedgeRetry;Sweep 在既有预分配表内有界回收旧状态,Unknown 或在途绝不强制删除。Broken 已交执行层的链接在全部槽终态后可退休,执行层状态和风险保护不因退休解除。复用还须等待链接输出消费完。

Kill 锁定期间直接走已有无链接的永续优先串行减仓,保留 #76 的永续严格零仓、POSITION_CLOSE、未知结果和风控规则。策略定时器只能占用其配额,引擎保留完整最坏登记量;不借用满链接表中的条目。

## Alternatives considered

仅扩容不修复不完成的状态。向上补足 lot 会制造新敞口。直接核销持仓会使账本与交易所失配。单独预留一个 Kill link 仍限制并行账户/组,且继续依赖 link 回收;无链接串行路径已有风险递减约束。

## Consequences

资金/风控语义在 [changes](../../../../docs/changes.md) 标为待评审。现货残余继续归账本,永续不足最小量不能假装归零。顺序平仓可能暂时留下现货多头,但永续减仓不依赖另一腿报价或 link 空位。验证包括 [真实快照](../../../../crates/hft-oms/tests/fixtures/linkcap/README.md)、1 槽 64 轮、真实 sequencer 满表/计时配额耗尽下 Kill、逐字节回放和 DST seed `0x1017c4`;不作本机生产延迟声明。
