---
description: "资金与保证金监控器:每分钟汇总每个账户在各 venue 的余额、到强平价的距离与 MarginStress 档位,按上涨与下跌两个方向计算价格情景下到各档与强平的余量,给出跨所/跨钱包的划转建议;只建议、不划转,交易 key 不带提现权限"
kind: python-package
tier: cold
scope: core
status: spec
---

# treasury

## 概要

`treasury` 回答两个问题:**这个账户离保证金危急还有多远**,以及**钱放在哪里最安全、最有用**。跨所两腿的组合把永续持仓放在一个所、另一条腿放在另一个所;价格朝永续持仓的不利方向变动时(空头遇涨、多头遇跌)永续所的保证金被消耗、价格向强平价靠近,而另一条腿的浮盈在另一个所,只能靠人工划转补过去([margin-liq-distance](../../.agents/notes/proposed/architecture/2026-10-03-margin-liq-distance.md))。treasury 每分钟从 journal 读每个账户在每个 venue 的现金、现货库存、永续持仓、保证金余额、维持保证金与各 inst 强平价(`MarginReport`)、实际杠杆(`AccountConfigReport`)、生效的 MarginStress 档位与阈值,计算当前与价格情景下的余量,并给出划转建议推送到 ops_bot。它**只建议、不执行**:不持有任何 key,不调用划转、提现或跨链接口;交易 key 本身不带提现权限。人工执行后经 ops_bot `/transfer` 登记。保证金危急时的自动降级与减仓由引擎负责([hft-risk](../../crates/hft-risk/README.md)),P0/P1 告警由 watchtower 投递;treasury 提供的是提前量与建议。

## 目录

- [使用本组件](#使用本组件)
  - [输入](#输入)
  - [计算](#计算)
  - [划转通道表](#划转通道表)
  - [输出与登记](#输出与登记)
  - [模拟账户](#模拟账户)
  - [链上钱包(optional)](#链上钱包optional)
  - [配置文件字段](#配置文件字段)
- [实现说明](#实现说明)
- [延迟与资源契约](#延迟与资源契约)
- [测试与验收](#测试与验收)
- [依赖](#依赖)
- [延伸阅读](#延伸阅读)
- [已知限制与后续工作](#已知限制与后续工作)

## 使用本组件

```
hft-treasury run     --config /etc/hft/treasury.toml       # hft-treasury.service;监控每分钟一轮
hft-treasury once    --config … [--account <a>]            # 立即算一轮并打印
hft-treasury explain --config … --account <a>              # 每个位置的余额、需求、余量与建议的推导
hft-treasury stress  --config … --account <a> --move 0.5   # 指定价格情景(持仓币统一 +50%;负值为下跌)下的保证金率
```

### 输入

| 数据 | 来源 | 说明 |
|---|---|---|
| 每 venue 现金、现货库存、永续持仓、未实现、保证金余额、维持保证金、可用保证金、保证金率、各 inst 强平价 `liq_px` | journal 经 `hft_py`(ledger 的权益数据与强平价估算 + 最新 `MarginReport`),定义见 [hft-ledger](../../crates/hft-ledger/README.md) | 每 (account, venue);`MarginReport` 带接收时间,过期的标"数据过期";`liq_px` 与引擎一样取交易所值与账本估算中更保守者 |
| 实际杠杆 | journal 中最新的 `AccountConfigReport` | 只用于解释与核对:交易中的永续 inst(当前生效参数中该账户各 group 的腿)杠杆未知或超过 `RiskParams.max_perp_leverage_x100` / 100 时在 `notes` 记 `lev:<venue>:<inst>`(与 [hft-risk 永续杠杆上限](../../crates/hft-risk/README.md#永续杠杆上限)同口径) |
| MarginStress 档位与阈值 | journal 中的 cap 事件与当前 `RiskParams`(`liq_dist_block_open`、`liq_dist_reduce`、`liq_dist_force`、`liq_dist_hyst`、比率交叉校验阈值、`margin_stale_ms`;唯一归属 [hft-risk](../../crates/hft-risk/README.md)) | treasury 不另设阈值,只计算到这些阈值的距离 |
| 交易所真值 | 最近一轮 `ReconcileReport`(reconciler 的 REST 真值) | 与 journal 不一致时以对账为准并在输出中标注:其中 `item = Balance`、`severity ≠ Ignore` 的条目以交易所值替换该位置该资产的余额,`notes` 记 `recon:<venue>:<asset>` |
| mark、现货 bid | journal 行情(`Mark`、`Bbo`) | 估值规则与 [hft-ledger](../../crates/hft-ledger/README.md) 相同 |
| 在途划转 | journal 中的 `RecordTransfer` | ledger 登记即把金额计入目的位置 `to`;到账前目的位置的账本值高于交易所报告值,差额 `max(0, 账本值 − 报告值)`(不超过尚未到账的登记额之和)输出为该位置的 `in_transit`。一笔登记在其后第一份使 `|账本值 − 报告值|` 小于该笔金额一半的目的位置报告到达时视为到账。需求与闲置/缺口按含在途的账本值算,所以已登记的划转不被重复建议;情景余量与紧急度不计在途(见[计算](#计算)第 3 步) |
| 划转通道 | policy 中的通道表(下) | 成本与时延 |

**必需输入**:每个位置的余额、永续 venue 的 `MarginReport` 与持仓 inst 的 mark、当前 `RiskParams`;必需输入缺失记 `missing:<输入>[:<venue 或 inst>]`(实例的 journal 打不开记 `missing:journal`),`MarginReport` 过期记 `stale:MarginReport:<venue>`;`AccountConfigReport` 与 `ReconcileReport` 缺失时只在 `notes` 记 `missing:<输入>`,照常计算。数据年龄 = 这一轮的时刻(UTC)− 该记录的接收时刻(UTC);`MarginReport` 年龄 > 该 venue 的 `margin_stale_ms`(严格大于,取值与口径归 hft-risk)即过期。

### 计算

每个账户独立计算,金额按 USDT 1e-6 整数,比例按 1e-9 整数;函数为纯函数,相同输入逐字节相同输出。

1. **位置**:账户在每个 venue 的钱包是一个位置 `loc`:永续 venue 是合约钱包,现货 venue 是现货钱包;另外,永续 venue 所在交易所的资金/现货钱包(充值先落在那里)作为同所的"待转入"位置。一个账户在每个交易所只用一个市场([no-same-exchange-spot-perp](../../.agents/notes/proposed/architecture/2026-10-03-no-same-exchange-spot-perp.md)),同一交易所的现货与永续共用一个钱包的情形不会出现;各所的钱包结构以 venue README 为准。"待转入"位置的余额不在 journal 中(treasury 没有 key),它不输出余额、不参与闲置/缺口计算,只作为跨所划转进合约钱包的中转(见第 6 步)。
2. **当前状态**:每个位置的可用现金 `B[loc]`(扣除挂单冻结与保证金占用):现货钱包 = 该 venue 各计价资产的 `free`(ledger `CashView`)按 ledger 换算率折 USDT 之和;合约钱包 = `MarginReport` 的可用保证金;保证金类位置的到强平距离 `liq_dist[loc] = min over 永续持仓 |liq_px − mark| / mark`(与 hft-risk 同一口径)与交叉校验比率 `r[loc]`(维持保证金 / 保证金余额)。
3. **情景余量**(两个方向分别算):对持有的全部资产施加统一的相对价格变动 `s`(永续按 mark、现货按 bid 同比例变动);**上涨方向**(`s > 0`)考察永续空头,**下跌方向**(`s < 0`)考察永续多头,账户在每个方向取最危险的持仓。先求到强平的变动 `s_liq`:每个该方向会被强平的持仓 `s_liq,i = liq_px_i / mark_i − 1`(空头为正、多头为负),取绝对值最小者;同一保证金池里有多个永续持仓或缺少交易所 `liq_px` 时,改用保证金模型(维持保证金按 venue 规则随名义变化,规则缺失时按名义同比例近似)二分求"保证金余额 = 维持保证金"的变动,并标"近似"。再按"强平价不变"把档位换算成变动:上涨 `s*_Lk = (1 + s_liq) / (1 + d_k) − 1`,下跌 `s*_Lk = (1 + s_liq) / (1 − d_k) − 1`,`d_k` 为三档的距离阈值。比率交叉校验:按保证金模型二分求 `M(s) / MB(s)` 达到交叉校验阈值的变动,`M(0) = max(rep_maint, rep_ratio × MB(0))`(与 hft-risk `r = max(报告比率, 估计比率)` 同口径),`s*_L2` 取两者中绝对值较小者。精度 0.1 个百分点,绝对值上限 `stress_max_move`(求不到解的方向输出该上限并标 `capped`)。当前已处于或越过某档(上涨方向 `s*_Lk ≤ 0`、下跌方向 `s*_Lk ≥ 0`)时该档输出 0。没有永续持仓的方向输出"无强平风险"(该方向 L1–L3 为 `null`)。保证金模型所用的保证金余额 `MB(0)` 取交易所报告值与账本估计中较小者,在途资金不计。
4. **需求**:保证金类位置要使两个方向的 `|s*_L2| ≥ target_headroom`,需要追加的保证金 `D[loc]`(对 `D` 二分,每次按追加后的保证金重算两个方向的 `s_liq`,取需要更多的一方;追加后的保证金以含在途的账本值为起点;`D` 取满足条件的最小整数 USDT;已满足时 `D ≤ 0`,其绝对值是移出后两个方向仍满足的最大整数 USDT);现金类位置的需求 = 保留额 `reserve_usd[loc]`(`D = 0`)。保留额 `R[loc]` 按位置类别取 `reserve_usd.spot` 或 `reserve_usd.perp`。
5. **闲置/缺口**:现金类 `S[loc] = B[loc] − R[loc]`;保证金类 `S[loc] = min(B[loc] − R[loc], −D[loc])`(移出闲置不会使余量低于目标)。> 0 闲置,< 0 缺口。
6. **配对**:把闲置位置的钱按单位成本从低到高分给缺口位置(位置数 ≤ 12,贪心即确定):重复取当前单位成本最低的(闲置位置, 缺口位置)对并转 `x = min(闲置额, 缺口额)`,单位成本 = (固定成本 + 比例成本 × `x`)/ `x`;相同者按缺口位置两个方向中较小的 `|s*_L2|` 升序、再按位置名字典序;直到没有闲置或没有缺口;`suggestions` 按配对顺序列出。同所钱包之间的内部划转优先(成本 0、秒级),跨所提现其次;跨所划转的建议是一条(`from`、`to` 为两端位置),成本与时延为"所 A ⇄ 所 B"一段,加上两端中每个合约钱包各一段"同所内部划转"(合约钱包经同所资金/现货钱包进出)之和。
7. **过滤**:`amount < min_transfer_usd` 不建议;`cost / amount > max_cost_ratio_bp / 10 000` 不建议(两者等于时照常建议);缺口位置是现金类(只是低于保留额,不影响任何方向的 `|s*_L2|`)的建议只报告、不推送(`notify = false`);缺口位置是保证金类的建议按[输出与登记](#输出与登记)的去重规则推送。
8. **紧急度**:任一方向 `|s*_L2| < min_headroom` → 建议标"紧急"并立即推送;journal 中该账户的 `MarginStress` cap 已处于 L1 及以上(`engine_level ≥ 1`)→ 建议的 `reason` 另含"引擎已禁开/已在减仓,划转只能补救余量"。

### 划转通道表

policy 项(`policy.channels`);treasury 只读 policy、不访问网络。没有官方出处的费用与时延一律待核实(方法见开发备忘),policy 数字按各所充提网络接口的公开值人工更新。

| 通道 | 固定成本 | 比例成本 | 时延 | 执行方式 |
|---|---|---|---|---|
| 同所资金/现货钱包 → 合约钱包(内部划转,用于把充值转入永续 venue 的合约钱包) | 0 | 0 | 秒级 | 人在交易所网页操作(交易 key 不用于划转;各所 key 权限能否只开交易:待核实,方法见开发备忘) |
| 所 A ⇄ 所 B(USDT 提现到对方充值地址) | 按网络(待核实,方法见开发备忘) | 0 | 按网络确认数(待核实,方法见开发备忘) | 人在交易所网页操作,提现地址白名单 |
| 主账户 ⇄ 子账户 | 0(待核实,方法见开发备忘) | 0 | 秒级(待核实,方法见开发备忘) | 人在主账户操作 |

### 输出与登记

- 每轮写 `/var/lib/hft/treasury/latest.json` 与 `history/<ts>.json`(每个 UTC 小时最早的一轮写一份,早于 30 天的删除):`{round_ts, accounts: [{account, kind, status, locations: {loc: {balance, reserve, demand, surplus, in_transit, margin_ratio, data_age_ms}}, headroom: {up: {L1, L2, L3, approximate, capped}, down: {…}, engine_level, day_min_l2: {utc_date, up, down}}, suggestions: [{from, to, asset, amount, cost, latency, urgency, reason, notify}], notes}]}`;`status ∈ {ok, incomplete, stale}`(必需输入失败 / `MarginReport` 过期,两者都有时取 `incomplete`);`kind ∈ {Live, Sim}`;`urgency ∈ {normal, urgent}`;`day_min_l2` = 本 UTC 日内两个方向各自的最小 `|s*_L2|`。
- ops_bot `/treasury [a]` 显示最新结果([ops_bot](../ops_bot/README.md));有新建议时推送一次(按用户订阅的账户,带账户标签;同一建议 `renotify_min` 内不重复;"紧急"不受限):treasury 在建议的 `notify` 中判定:保证金类缺口的建议在其键 `(account, from, to, asset)` 首次出现、距该键上次 `notify = true` 已满 `renotify_min` 分钟、或紧急度由普通升为紧急时 `notify = true`,`urgency = urgent` 的建议每轮都 `notify = true`;ops_bot 只推 `notify = true` 的建议;各键上次推送的时刻存在状态目录的 `notify.json`(原子写),重启后照常按它去重;它缺失或无法解析时按空表处理并在 `notes` 记 `notify_state_reset`。日报附每个账户两个方向的最小 `|s*_L2|` 与当日最低值(`day_min_l2`)。
- 每轮结果只写 treasury 自己的状态目录,不进 journal;每个输出文件旁写 `<文件名>.sha256`(该文件内容的 SHA-256 十六进制)。`once`、`explain`、`stress` 只打印 JSON、不写状态目录(`--account` 不是任何实例中的账户 → 退出 2):`once` 与 `latest.json` 同结构;`explain` 另给每个位置的 `B`、`R`、`D`、`S`、`in_transit`、两个方向 `s_liq` 的来源(`exchange` / `ledger` / `model`)与配对的每一步;`stress --move s`(`s` 须在 `(−1, stress_max_move]` 内,否则退出 2)给该情景下每个位置的权益、维持保证金、保证金率(维持 / 权益)与权益占账户合计权益的比例。
- **登记划转**:人工执行后,交易员在 ops_bot 用 `/transfer` 登记(`RecordTransfer`,[control-plane](../../docs/subsystems/control-plane.md));ledger 记为资金流动、不计 PnL,reconciler 在到账前把相应余额差视为在途。treasury 本身不发命令,在权限表中没有身份。

### 模拟账户

模拟账户(`AccountKind::Sim`,[hft-sim](../../crates/hft-sim/README.md))同样计算并输出,所有数字标"模拟",不与实盘合计;建议可由交易员用 ops_bot `/simtransfer` 执行(`SimTransfer` 只作用于模拟账户),用来在模拟中演练"普涨 → 余量下降 → 划转"的流程与时延。

### 链上钱包(optional)

触发条件:链上策略迁到新框架、chain-core 启用。启用后增加位置"BSC 执行钱包"(USDT/USDC/USD1 与 BNB gas):余额来自 reconciler 的钱包扫描,gas 低于 `gas_min_bnb`(0.0015)建议补到 `gas_topup_bnb`(0.02);通道表增加交易所 → BSC 提现(旧链上策略系统 `binance_transfer.py` 实测 0.01 USDT、3–6 min)与 deBridge DLN(≈ 1.15 USDT + 0.015 SOL + 0.08 %,≈ 12 s;$30 时成本 9 %,$300 时 1 %)。

### 配置文件字段

`/etc/hft/treasury.toml`(部署项)+ `/var/lib/hft/treasury/policy.toml`(版本化 policy)。

| 键 | 类型 | 含义 | 默认 |
|---|---|---|---|
| `hft.instances` | [`{name: "prod"\|"sim", journal_cold, journal_ram}`] | hft-core 实例(统一升级后模拟账户在 `sim` 实例,[deploy 模拟实例](../../deploy/README.md#模拟实例sim)) | 必填 |
| `reconciler.state` | path | 对账报告 | 必填 |
| `cycle_s` | u16 | 监控轮周期 | 60 |
| `policy.target_headroom` | 定点比例 | 建议把 `s*_L2` 补到的目标 | 待标定(无默认,必填) |
| `policy.min_headroom` | 定点比例 | 低于即"紧急" | 待标定(无默认,必填) |
| `policy.stress_max_move` | 定点比例 | 情景求解上限 | 3.0(+300 %) |
| `policy.reserve_usd` | 表(`spot`、`perp` 两类位置) | 每类位置的保留额 | 待标定(无默认,必填) |
| `policy.min_transfer_usd` / `max_cost_ratio_bp` / `renotify_min` | 定点 / u16 / u16 | 过滤与推送 | 10 / 100 / 30 |
| `policy.channels` | `{internal, cross, sub}`,每项 `{fixed_usd, ratio_bp, latency_s}` | 划转通道表(取值见上表,待核实,方法见开发备忘) | 无默认,必填 |
| `policy.chain.*` | 表 | 链上钱包(optional) | 见上 |

加载校验:缺必填键、`cycle_s = 0`、`0 < min_headroom ≤ target_headroom ≤ stress_max_move` 不成立、部署项或 policy 中出现名字含 `key`、`secret` 或 `passphrase`(不区分大小写)的键 → 启动失败(退出 2)。

## 实现说明

<details><summary>实现要点</summary>

- 单进程,每 `cycle_s` 秒一轮;每轮 `plan(inputs, policy) -> Plan` 是纯函数,I/O 只在读输入与写输出。
- 情景求解对每个 venue 用该所的保证金规则(维持保证金率随名义的阶梯,来自 refdata 或 venue README;缺失时同比例近似并标"近似");阈值与口径只读 `RiskParams`,不在 treasury 重复定义。
- 输入的每一项都带"读取成功"与数据年龄;必需输入失败或 `MarginReport` 超过 `margin_stale_ms` → 该账户这一轮只输出"数据不全/数据过期"(只有 `account`、`kind`、`status`、`notes`,`day_min_l2` 不更新),不给划转建议(引擎侧此时已视为 L1)。

**规划的源文件**

| 文件 | 职责 |
|---|---|
| `src/hft_treasury/main.py` | 调度、子命令 |
| `src/hft_treasury/inputs.py` | journal、对账、RiskParams 输入与成功标志 |
| `src/hft_treasury/margin.py` | 各所保证金率与情景求解 |
| `src/hft_treasury/plan.py` | 余额/保留/需求/闲置、配对、过滤、紧急度 |
| `src/hft_treasury/channels.py` | 通道表与成本函数 |
| `src/hft_treasury/output.py` | 状态文件、推送请求 |

</details>

## 延迟与资源契约

冷进程,无热路径约束:一轮 < 5 s(10 个账户);CPU 与内存上限在 systemd 单元中设置,`dev2` 的取值归 [hft-core](../../apps/hft-core/README.md#内存与-journal-段按主机类唯一归属)。

## 测试与验收

用例与验收见 [TESTS.md](TESTS.md)。

## 依赖

- 内部:`hft-py`(journal、ledger)。treasury 不连 ctl-gateway,在权限表中没有身份。
- 外部:`polars`、`pyarrow`。
- 禁止:交易所 SDK、任何 key、划转/提现/跨链桥接口。

## 延伸阅读

- [round8-decisions](../../design/_work/round8-decisions.md)、[评审 §2.2](../../design/_work/review-2026-10-02-主策略适配与执行正确性.md)、[09](../../design/09-可观测性、风控与控制面.md)
- [margin-liq-distance](../../.agents/notes/proposed/architecture/2026-10-03-margin-liq-distance.md)、[margin-stress](../../.agents/notes/proposed/architecture/2026-10-02-margin-stress.md)、[hft-risk](../../crates/hft-risk/README.md)(档位与阈值)、[hft-ledger](../../crates/hft-ledger/README.md)(权益数据)、[ops_bot](../ops_bot/README.md)、[reconciler](../../apps/reconciler/README.md)

## 已知限制与后续工作

- 只建议不执行;自动划转(需要提现权限的凭证、地址白名单、金额上限、二次确认、单独的安全评审)不做,用户已决定。
- 统一价格变动(每个方向一个 `s`)与"强平价不变"是简化情景:不同币涨幅不同、基差变化与全仓下强平价随其他持仓移动未建模;它给的是提前量,当前的精确距离以 `/risk` 中引擎计算的 `liq_dist` 为准。
- 各所维持保证金阶梯与钱包结构核实之前(方法见开发备忘),用保证金模型求出的情景结果都标"近似"(refdata 目前不含阶梯,模型按名义同比例)。

### 开发备忘

<details><summary>给实现者</summary>

- **待核实**:Binance、OKX、Bitget 的统一账户/经典账户下保证金率的定义与换算(核实方法:服务器上拉取账户与持仓风险接口,和 `MarginReport` 金样逐项对照);主/子账户内部划转的到账时间与费用、USDT 各网络提现费与确认数(方法:在测试网或小额主账户上各做一次并记时,与各所充提网络接口的返回值对照);各所 key 能否只开交易不开划转(方法:在子账户 API 管理页逐项查看权限选项并截图)。
- 情景求解与 hft-sim 的保证金模型用同一套各所规则数据(来自 refdata/venue README),避免两处口径漂移。

</details>
