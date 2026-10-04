# Agent Note: 参考数据(refdata)成为独立组件,版本化整体替换

Status: proposed

> **第八轮(2026-10-02)**:第 2 条改为 refdata 只由 `hft-cfg` 编码(pipeline 只产出交易宇宙候选与差异报告);refdata 新增 `VenueCaps` 与 `AccountMeta` 的 `kind`、`venue_uid`、`bound_instance`;见 [venue-caps](2026-10-02-venue-caps.md)、[one-account-per-strategy-instance](2026-10-02-one-account-per-strategy-instance.md)。

## Problem

跨所套利最容易出错的数据是参考数据:合约表、资产与定点指数、tick/lot、费率档、限速表、符号映射、资金费周期、维护窗口。早期设计只在几处提到 `InstTable`,没有所有者([13 O6](../../../../design/13-设计评审与优化.md))。现实中的问题:符号跨所不一致(`PLAYUSDT`/`PLAY-USDT`/`PLAY_USDT`)、同名假币极多、tokenized 股票别名(`{SYM}ON`、`{SYM}X`、`{SYM}B`)([research/00 §6.2](../../../../design/research/00-existing-codebase-inventory.md));OKX 改了 `price_tick` 而本地没刷新导致分片 crash-loop;刷新只接受 USDT 计价导致 BTCUSDC 被静默剔除([postmortem](../../../../docs/postmortem/README.md))。

## Proposal

[04 §2](../../../../design/04-行情接入层.md)、[07](../../../../design/07-策略层.md)、[`hft-refdata`](../../../../crates/hft-refdata/README.md):

1. `hft-refdata`(pure)定义 `RefData`、`InstMeta`、`AssetMeta`(含 `px_exp/qty_exp`)、`FeeSchedule`、`RateLimitTable`、符号映射与维护窗口;以带版本的二进制 blob 编码。
2. Python [`pipeline`](../../../../python/pipeline/README.md) 生成 blob(发现、身份核验、别名解析),附与当前版本的差异报告。
3. blob 经 `ReplaceRefdata` 命令整体替换,并作为 `ConfigBlob` 写进 journal([self-contained-journal](2026-09-30-self-contained-journal.md));热进程内不做部分刷新。
4. 加载时校验:引用完整、同一资产指数一致、tick/lot 与指数相容、计划内标的未消失;失败拒绝启动或拒绝该命令(根 AGENTS.md "错误配置在加载时失败")。
5. `InstId`、`VenueId` 在 blob 内分配并保持稳定;旧 journal 按其自带的 blob 解释。

**不做热替换**:第 3 步的实现是"写 `ConfigBlob` 后 Drain + 重启";SymbolGroup 成员、武装名单、阈值放在可热替换的 `StrategyParams`,refdata 预装交易宇宙超集;refdata 由 `hft-cfg` 编码。理由与热替换(可选扩展)的触发条件见 [refdata-change-is-restart](2026-10-01-refdata-change-is-restart.md)。

## Alternatives considered

**每个 venue crate 自带合约表。** 跨所的资产指数与符号映射无处安放;同一资产在两处定义就会不一致。

**热进程运行期自己拉交易所 `exchangeInfo`。** 在热进程里做 REST 与 JSON 解析,违反热冷分离;刷新结果不经评审直接生效,正是 BTCUSDC 事故的形态。

**配置文件热加载。** 不入 journal,回放不可重现;现有"三参数源"的教训。

## Acceptance criteria

- refdata blob 的每次生效在 journal 中有对应的 `ConfigBlob` 与 `CmdResult`。
- 新 blob 缺少计划内标的、或资产指数变化,在未确认时被拒绝(单元测试);差异报告列出每一项。
- `watchtower` 对 refdata 年龄超过刷新周期告警。

## Risks

- 身份核验(同名假币)依赖 pipeline 的质量:错误的映射会把两个不同资产当作同一个;五道门身份核验与人工确认列表保留([07](../../../../design/07-策略层.md))。
- blob 体积随标的增长:当前数百标的量级,KB–MB,可接受。
