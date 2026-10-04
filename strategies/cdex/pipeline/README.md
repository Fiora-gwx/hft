---
description: "可选:cdex 的 pipeline 插件包(Python,随策略发布):名单排名、池子查找与核验、五道门、72 h 历史与推荐阈值、按池子费率的成本线、规模因子、armed 与 open_allowed、名单清理,以及冷启动的价差样本预热;cdex 迁移到新框架时启用"
kind: python-package
tier: cold
scope: optional
status: spec
---

# cdex pipeline 插件(`hft_plugin_cdex`)

## 概要

本包是 [cdex](../README.md) 策略的慢路径插件:分发名 `hft-plugin-cdex`,导入名 `hft_plugin_cdex`,源码在 `strategies/cdex/pipeline/`,与策略 crate 同仓同版本发布。[pipeline 框架](../../../python/pipeline/README.md)经入口点发现它,按实例周期调用 `compute`、`explain` 与 `warmup`。它是现有 cdex 慢路径(`monitoring/dex_pipeline.py`、`watchlist.py`、`pool_finder.py`、`token_check.py`、`sizing.py`、`spread_engine.refresh_params`,以 `analysis/util0929/src/` 的 09-29 版本为准)的迁移:**口径与数字原样保留**,只改接口(纯函数 + 框架快照)。它**拥有**名单、池子核验、成本线、`open_th`、`armed`、规模因子、资金费结构性条件与价差样本预热的生成;**不拥有**调度、取数 I/O、参数编码、覆盖项、下发与注入(框架),实时价差、确认、开平与 ClosePolicy(策略 crate)。字段名、单位与预热载荷布局的唯一归属是策略 README。**触发条件**:决定把 cdex 从旧框架迁移到新框架时(与策略 crate 同一触发条件,[ccex-first-scope](../../../.agents/notes/proposed/architecture/2026-10-02-ccex-first-scope.md))。

## 目录

- [使用本组件](#使用本组件)
  - [包与注册](#包与注册)
  - [数据源](#数据源)
  - [排名与名单](#排名与名单)
  - [池子查找](#池子查找)
  - [五道门与准入](#五道门与准入)
  - [72 h 历史与推荐阈值](#72-h-历史与推荐阈值)
  - [成本线](#成本线)
  - [规模因子](#规模因子)
  - [armed 与 open_allowed](#armed-与-open_allowed)
  - [名单清理](#名单清理)
  - [预热](#预热)
- [实现说明](#实现说明)
- [延迟与资源契约](#延迟与资源契约)
- [测试与验收](#测试与验收)
- [依赖](#依赖)
- [延伸阅读](#延伸阅读)
- [已知限制与后续工作](#已知限制与后续工作)

## 使用本组件

### 包与注册

```
strategies/cdex/pipeline/
  pyproject.toml          # [project.entry-points."hft_pipeline.plugins"] cdex = "hft_plugin_cdex:PLUGIN"
  src/hft_plugin_cdex/    # rank.py、pool_finder.py、gates.py、history.py、cost.py、sizing.py、cleanup.py、warmup.py、encode.py、sources.py、explain.py、plugin.py
  tests/
  README.md
```

- `strategy_kind = 2`、`api_version` = 构建时的 `hft_pipeline.api` 版本;`__init__.py` 重导出 `PLUGIN`;实例配置 `plugin = "cdex"`;只 import 框架的公开 API `hft_pipeline.api`(插件协议见 [python/pipeline](../../../python/pipeline/README.md#插件接口))。
- 纯函数:同一快照两次 `compute`/`warmup` 结果逐字节相同;时间只取 `snap.ts`;链上报价、交易所与第三方接口的请求都由框架按本包声明的 `InputSpec` 执行(每个端点独立令牌桶)。
- `owned_fields`(per-group,字段语义见[策略参数表](../README.md#参数-schemacdexparams)):`profile`、`armed`、`open_th`、`funding_structural_ok`、`max_abs_spread`、`slow_cap`、`per_coin_cap`、`carry_enabled`、`cost_pool_fee_rt`、`cost_gas_usd`、`cost_bribe`;另给 `GroupSpec` 的腿(CEX 永续 inst + 链上 venue 与 `route_id`)与 `open_allowed`。

### 数据源

规则只按下表的**角色**引用数据源;交易所与第三方服务的名字只出现在本表与 `sources.py` 里。换来源 = 改本表与 `sources.py`,规则与字段不变。

| 角色 | 提供 | 当前来源 |
|---|---|---|
| `perp_market` | Fast 腿 CEX 的 USDT 永续公共行情:资金费率与实际周期、24 h 成交额、OI、1 h K 线、标记价 | Binance USDⓈ-M 公共 REST |
| `cex_asset_meta` | 同一 CEX 的官方链上合约地址(充提网络)与 Alpha 代币列表 | Binance 现货公共 REST |
| `dex_venue` | 允许的链上池类型 | PancakeSwap V2/V3/V4(BSC) |
| `dex_pool_index` | 池子列表、DEX 24 h 成交与流动性 | DexScreener |
| `dex_v4_index` | V4 池列表(链上恢复 `PoolKey` 用) | Pancake explorer |
| `dex_ohlcv` | 链上 1 h OHLCV(自有样本不足时) | GeckoTerminal |
| `chain_rpc` | Quoter/CLQuoter 的 `eth_call` 报价、链上 FDV | 多个免费公共 BSC RPC |
| `own_samples` | 自有价差样本 | S3 Parquet + 当日 journal |

### 排名与名单

候选 = `perp_market` 的 USDT 永续、资金费率 > 0、24 h 成交额 ≥ 2 M USD;日化 `rate × (24 / interval_h) × 100` ≥ 0.05 %/日;按资金费取前 120 且 OI ≥ 2 M USD;`score = daily_pct × log10(OI_usd)`;`watch_n` = 30,`max_groups = watch_n + 10`;每轮前 15 个未入名单者进入核验;同币 24 h 不重复核验,被禁币 7 天不入名单。资金费周期取自 `perp_market`,不假设 8 h。

### 池子查找

`cex_asset_meta` 给出的官方地址优先(评分 ×20,跳过 FDV 核验;股票代币 `{SYM}B`、Alpha 列表 chainId 56);别名 `SYM`、`WSYM`、`SYMX`(股票代币加 `SYMON`、`SYMB`);从 `dex_pool_index` 取池,只保留 `dex_venue` 的 V2/V3 池,V4 池从 `dex_v4_index` 取并在链上恢复 `PoolKey`、用 CLQuoter 报价,带 hook 的池标 `l0_capable = false`;USDT/USDC/USD1 计价直连,WBNB/BNB/ETH/BTCB 计价经 V3 一跳路由(费率在 {100, 500, 2500} ppm 中取最优,缓存 10 min);按评分前 4 个做 $30 买卖探测,买卖差 ≤ 3 %、相对 `perp_market` 标记价偏离 ≤ 2 % 通过,取 `dev + gap` 最小者。报价来源是 `chain_rpc` 的 `eth_call`(Quoter/CLQuoter),每端点独立令牌桶,不信任 `dex_pool_index` 的美元价格字段;**不依赖 `hft-amm`**。路由须已在当日 refdata 的 `routes` 段内,不在的进入 `universe_candidates`。

### 五道门与准入

任一 🔴 拒绝:价格偏离 ≥ 10 %(≥ 2 % 视为非同一资产);非官方地址且链上 FDV < 市值 × 5 %;流动性 > $50 k 但 24 h 成交 < 流动性 1 %(Solana 的 $22 冲击 ≥ 50 %);72 h 价差触及成本线后从未回落或中位数 > 50 %;真实 Quoter 报价不满足探测条件。准入另需流动性 ≥ $50 k、$22 冲击 ≤ 2 %、至少一个完整开平周期、`cap_usd ≥ 6`。

### 72 h 历史与推荐阈值

`own_samples` ≥ 24 h 时用它,否则 `perp_market` 1 h K 线对 `dex_ohlcv` 1 h OHLCV;统计均值、中位数、标准差、近零占比、翻转、最长单边、abs p90、触及小时数,插针保留并单列;激进阈值从 0.5 % 每步 +0.1 %(≥ `min_hits` = 3 个小时可达),上限 20 %,最多 196 步、每币每轮预算 2 s,超时记 🔴 并告警(所有循环有界,[12 §1](../../../design/12-风险与未决问题.md))。结果写 `open_th`(历史推荐与配置阈值取大)与 `hist_verdict`。

### 成本线

bp 整数:`round_trip_bp = max(40, 10 + 2 × fee_ppm / 100 + gas_bp + 10)`(费率未知或动态费率:BSC 80、Solana 59);`gas_bp = gas_usd / trade_usd × 10 000`;经 WBNB/ETH/BTCB 路由追加 `2 × route_fee_ppm / 100 + 10 + gas_bp`;`cost_floor_bp = round_to_10(round_trip_bp + route_bp + 30 + 20)`。实测对照:0.01 % 与 0.3 % 池 → 130 bp,1 % 池 → 320 bp。下发为 `cost_pool_fee_rt`(池子费往返,含路由中转池)、`cost_gas_usd`、`cost_bribe`;策略 crate 用 `hft-strat-lib::CostLine` 按 `Rate` 精度重组成本线(对照测试按 0.1 % 取整后比较)。

### 规模因子

插件给上限因子,策略取 min:单币硬上限、池深 × `pool_share_frac`(0.005;< $100 k ×0.6、> $500 k ×1.5)、DEX 成交 × 0.01、OI × 0.0005、`perp_market` 24 h 成交 × 0.0002、冲击 > 0.3 % 则减半最多 4 次、分块 `chunk`(下限 $6,`cap < 6` 不开) → 合成 `slow_cap`;携带门在资金利用率 < 0.6 时放宽(cdex 利用率 9/29(本仓库之外:`analysis/cdex-资金利用率与高资金费持有策略复盘-2026-09-29.md`))。账户资金口径由策略从 `PortfolioView` 自算,本包不下发 `wallet × leverage × deployable` 式的资金公式。

### armed 与 open_allowed

per-group 参数 `armed`(已核验、可走价差通道,即现 ✅)与 `GroupSpec.open_allowed`(引擎的开仓门)是两件事。👁 币 `armed = false`,但仍可能走插针/携带通道时 `open_allowed = true`;只平不开的币(坏池、清理中、有仓位但不再满足准入)`open_allowed = false`。`armed` 由 `hist_verdict`、`cap_usd ≥ 6`、1 h 中位 |buy spread| ≤ 5 % 共同决定;`funding_structural_ok` 每轮按近 3 期每期折日化 ≥ 0.10 %、30 天占比 ≥ 25 %(低占用放宽 0.05 % / 10 %)算出。

### 名单清理

坏池(1 h 中位 |spread| > 5 % 持续 3 h 或 1 h 无链上报价,且无仓位)→ 只平不开、下一版移出、禁 7 天;死币轮换(资金费 < 0.05 %/日、无仓位、入名单 > 48 h)每轮 ≤ 10 个;重选池进入次日交易宇宙候选。有仓位、在途或残余的 group 不移除(框架预检同样拦)。

### 预热

`warmup` 为名单中的每个 group(框架因 `ModelGroupReset` 只对部分 group 预热时,只为快照里列出的那些 group)生成 72 h 价差样本,编码为策略 README 定义的 `SpreadWarmup`(`local_kind = 1`,布局见[策略 README 的价差样本统计节](../README.md#价差样本统计72-h-口径允许开仓规则与-carry-线model)):每块一个 group、≤ 509 个样本、≤ 4096 B 且自身完整,`step_ms` = 10 000 的整数倍;来源为旧引擎的 `engine_samples`(10 s 采样)或新栈 journal 归档;与策略 crate 共用金样 `fixtures/strategies/cdex/warmup/`。72 h / 10 s 每 group ≈ 51 块,40 group ≈ 2 040 块、约 8 MiB journal Input。

## 实现说明

<details><summary>实现要点</summary>

- 迁移时逐函数建立"旧函数 → 新函数"对照表并做数值对照(差 ≤ 1 bp / $0.01,取整向保守方向);旧实现的网络调用拆成 `InputSpec` 声明 + 纯计算。
- `recommend` 的循环在任意输入下有界(步数、每币预算);超预算记 🔴,不阻塞整轮。
- 数值用 `decimal.Decimal`,下发值不经 `float`;预热块按小端由 `encode.py` 写出。

| 文件 | 职责 |
|---|---|
| `plugin.py` | 插件对象、`owned_fields`、`InputSpec` 声明 |
| `rank.py` | 排名与名单 |
| `pool_finder.py` | 池子查找、路由、探测 |
| `gates.py` | 五道门与准入 |
| `history.py` | 72 h 历史、推荐阈值、`hist_verdict` |
| `cost.py` | 成本线 |
| `sizing.py` | 规模因子 |
| `cleanup.py` | 名单清理 |
| `warmup.py`、`encode.py` | `SpreadWarmup` 取样与编码 |
| `sources.py` | [数据源](#数据源)表各角色的端点、限速与解析(交易所与第三方名字只在这里与数据源表) |
| `explain.py` | `explain` 步骤 |

</details>

## 延迟与资源契约

冷进程,无热路径约束。

| 指标 | 目标 |
|---|---|
| 一轮墙钟(含框架取数) | < 5 min(30 个名单币 + 15 个候选);超过 10 min → 框架 P1 |
| 每币每轮 `recommend` | ≤ 2 s(超时 🔴) |
| 外部请求 | 每端点独立令牌桶;单轮交易所权重 < 该 IP 限额 10 % |

## 测试与验收

| 层 | 用例 |
|---|---|
| 对照 | 同一输入下成本线、阈值、`sizing`、五道门、名单与现 cdex(`util0929`)输出差 ≤ 1 bp / $0.01;输入网格与旧输出存 `fixtures/strategies/cdex/py/` |
| 有界 | `recommend` 在任意输入(含插针、全零、单边)下终止且不超每币预算 |
| 纯度 | 同一快照两次结果逐字节相同;只输出 `owned_fields` |
| 预热 | `SpreadWarmup` 编码与金样逐字节相同;每块 ≤ 4096 B 且自身完整 |
| 清理 | 坏池与死币轮换的边界;有仓位的 group 不移除 |

验收(迁移时):对照全过;在旧框架与新栈并行的一个观察期内,本包产出的名单与参数和旧 pipeline 的差异逐条可解释。

## 依赖

- 内部:框架公开 API `hft_pipeline.api`。
- 外部:`polars`、`eth-abi`(手写 JSON-RPC 的 ABI 编解码);标准库 `decimal`、`struct`。版本锁定随 python 统一的 `requirements.lock`。
- 禁止:`hft-amm`、框架内部模块、自写参数编解码、交易所私有接口与 key、网络与文件 I/O(全部经框架)。

## 延伸阅读

- [cdex](../README.md)、[pipeline 框架](../../../python/pipeline/README.md)、[strategies/AGENTS.md](../../AGENTS.md)
- 现代码:`E:\trading\analysis\util0929\src\monitoring\{dex_pipeline,watchlist,pool_finder,token_check,sizing,spread_engine}.py`
- [research/00 §2.5–2.6](../../../design/research/00-existing-codebase-inventory.md)

## 已知限制与后续工作

- 可选扩展:Solana 池核验(本包内),触发条件:决定接入 Solana 链上腿。
- 名单增删只经新的 `StrategyParams`;宇宙之外的新币要等次日 refdata。

### 开发备忘

<details><summary>给实现者</summary>

- **待核实**:`cex_asset_meta` 的链上合约地址字段与 Alpha 列表接口、`dex_v4_index` 接口的稳定性与限速;核实方法:在服务器上逐个请求并存金样。
- 实现顺序:`cost.py` + 对照 → `rank.py` → `pool_finder.py` → `gates.py` → `history.py` → `sizing.py` → `cleanup.py` → 预热。
</details>
