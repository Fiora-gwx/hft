# Agent Note: 容量拒单可审计与 Ack 后拒绝的成交保护

Status: implemented

## Problem

OMS 槽满时意图没有 `OrderDecision`,回放与审计无法区分没有下单和下单被本机拒绝。Ack 后的 Reject 在旧状态表中被当成重复回报,零成交订单因此可能永久占用在途。

## Decision

容量拒绝由 sequencer 记录 `RejectBits::CAPACITY`,保留意图身份与原因,不分配槽、不出环、不消耗令牌。OMS 成功分配后才提交风控令牌;链接表耗尽同一口径。契约归 [意图管线](../../../../crates/hft-sequencer/README.md#意图管线)。

非终态订单只有在槽与 Reject 都没有成交证据时才进入 Rejected。存在成交证据时采用现有 `OmsAnomaly` 分支,累计成交只补增量,保留剩余在途和原状态,由后续交易所证据收敛。契约归 [回报处理表](../../../../crates/hft-oms/README.md#回报处理表)。

## Alternatives considered

**继续忽略 Ack 后的 Reject**:不能释放被交易所明确拒绝的零成交订单,掩盖异常状态。

**所有 Reject 都终结**:成交后的 Reject 不能证明未成交余量已经撤销,释放全部在途会低估风险。重复累计成交也不能再入账。

**容量不足归为传输失败**:尚未创建或发送订单,不应生成可能在交易所存在的 Unknown 订单;独立容量位保留失败来源。

## Consequences

拒单流增加一类可回放输出;旧日志遇到过去被静默丢弃的相同输入会出现差异。Reject 冲突保留的在途需要正常查询或后续终态回报收敛。两项资金/风控语义列入 [变更记录](../../../../docs/changes.md),等待负责人批量评审。

## Testing

OMS-056 检查 Ack 后零成交拒绝、部分/全部成交后拒绝、新增累计成交及重复回报;SEQ-175 检查槽满记录与从快照重放;SEQ-315 的订单回报子集以计数分配器覆盖通知、分派与 OMS 后处理。OMS-410、EXE-464 与 SEQ-344 校验真实引擎日志、快照及 DST 不变量。
