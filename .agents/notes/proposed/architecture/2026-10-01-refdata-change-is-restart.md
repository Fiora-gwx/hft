# Agent Note: refdata 变更 = 重启,不做热替换

Status: proposed

## Problem

[refdata-component](2026-09-30-refdata-component.md) 设想 refdata 经 `ReplaceRefdata` 在运行中整体替换(`EpochSwap`)。红队评审指出,refdata 是几乎所有热路径结构的根:`InstId` 稠密数组、簿的 tick 索引、资产指数、帧模板、符号哈希表、限速表都由它派生。运行中替换意味着这些结构要在两个事件之间一致地重建,在途订单的槽、模板与限速计数要跨 epoch 解释——这是整个系统里最容易出错、又最难测全的一块。与此同时,交易里真正高频变化的是名单、武装状态与阈值,而不是合约的 tick/lot。

## Proposal

裁决见 第三轮裁决(2026-10-01 红队评审后,主控记录) A3、B10:

1. refdata 预装"交易宇宙"超集(候选名单里所有可能交易的 inst 与资产),由 [`hft-cfg`](../../../../tools/README.md#hft-cfg) 编码(唯一编码器);pipeline 只产出名单与 `StrategyParams`。
2. SymbolGroup 成员、武装名单、阈值放在 `StrategyParams`(可热替换;按 `(strategy, account)` 一个 blob,内含全部 group),经 `SetParams` 生效并写 `ConfigBlob`。
3. `ReplaceRefdata` = 写 `ConfigBlob` 后请求 Drain + 重启;冷启动 < 30 s,重启后新进程的首段带新 blob([self-contained-journal](2026-09-30-self-contained-journal.md))。
4. 帧模板随 refdata 在启动期构建一次;解码接口显式传 `rd: &RefData`,`FeedCtx` 只保存 venue 私有状态。
5. 可选扩展:若重启频率或窗口成为问题,先列出允许热生效的变更集(例如只增不改的 inst),写新 Agent Note。

## Alternatives considered

**`EpochSwap` 全量热替换。** 见 Problem;实现与测试成本高,收益只是省一次 < 30 s 的重启。

**只允许 refdata 增量追加(新 inst)热生效。** 比全量简单,但仍需在运行中扩展稠密数组、模板与哈希表;是上面第 5 条可选扩展的候选方案。

**把名单也放在 refdata 里、每次名单变更都重启。** 名单每天多次变化;重启频率不可接受。

## Acceptance criteria

- `ReplaceRefdata`(只由 Ops 在维护窗口人工发起,需确认,预览列出两个账户各自的挂单与 Live cell):写 `ConfigBlob` → Drain(只等 Pending*,`Unknown` 写进快照;超时以退出码 75 退出)→ 重启 → 对账 → 人工 `Resume{Recovery}`;到"对账通过"的端到端演练 < 60 s,期间无裸腿;干净 Drain 重启在闸门通过后自动解除 `Recovery`,否则人工。
- `SetParams` 修改 group 成员与阈值无需重启,且在 journal 中可回放。
- 新 refdata 缺少计划内标的或资产指数变化时,加载拒绝并输出差异([defensive-patterns](../../../../docs/defensive-patterns.md#参考数据有版本刷新不能静默丢标的))。

## Risks

- 交易所临时改 tick/lot 需要一次重启;重启窗口内不交易该 venue。按日计频率可接受。
- 超集预装使 refdata 与稠密数组更大:数百到数千 inst 量级,内存可忽略;符号表构建时间计入冷启动预算。
