# Agent Note: 测试网轮询边界与内存计费

Status: implemented

## Problem

10/15 demo 运行暴露 ticker 连接路由错误、资金费轮询持续失败、gone 订单持续查询和 cgroup OOM。10/16 实采 REST 证实 startTime 等于 endTime 返回 code 20001，正宽 1 ms 返回成功空页。零宽请求在初次 settle-lag 边界形成后被固定重试，延长 lookback 不能修复这个边界。

## Decision

FundingPoll 等待正宽窗口；协议请求再次校验。MD 流按协议声明路由分组，重连保持归属。OMS 查询超过既有阈值后停止 gateway 查询，持久化等待对账的私有位，业务状态仍由交易所证据决定。资金与恢复语义列入 [changes](../../../../docs/changes.md) 待评审。

dev2 prod 单元预算覆盖全额 tmpfs 与非 journal 进程余量，数值唯一归属 [hft-core](../../../../apps/hft-core/README.md#内存与-journal-段按主机类唯一归属)。预检检查实际挂载和 journal 布局，避免只看匿名 RSS。父 slice 不扩容；批处理仍与交易窗口互斥。

## Alternatives considered

- 扩大 lookback 不能消除首个零宽窗口，也会扩大重复查询范围。
- 按报文前缀猜 ticker 不能修复连接、端点与订阅的路由归属。
- 把查询失败当作订单拒绝会绕过 Unknown 的证据约束。
- 只缩小 journal 或把 tmpfs 移出 cgroup 都不能正确表达整个进程的内存开销。

## Consequences

CI 的 EXE-050 随机序列另发现外来成交暂停只检查 Stuck 的展示原因。强制减仓再回落至 L1 可以覆盖原因，截止推进先产生旧目标切片、随后清除目标成为 Idle。以仍非零的 ext_deadline 作为暂停依据，保留既有截止处理顺序与已在途订单；不用放宽不变量或重跑随机测试掩盖反例。固定六步与原始 proptest 种子均留作回归。

多 MD 路由使用更多连接，仍受运行时预分配连接槽上限约束。达到查询预算后依赖 reconciler 提供终态证据，等待期间不会重发订单。内存预算增加会占用更多父 slice 余量，实机仍需验证 memory.peak 和 memory.events。可选用户态 Spin 仅允许独占隔离核，dev2 默认 Park 保持不变；未声称降低本机延迟。
