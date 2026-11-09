# Agent Note: Polymarket 行情层升级(永续、Chainlink 原生、全量 PM)与成交条件 EV 模型接入

Status: proposed

## Problem

1. 研究(成交条件期望模型,仓外 Spec `E:\trade\reports\成交条件期望_数学模型Spec_2026-11-09.md`)需要的输入有三类不在引擎 journal 里:Binance USDT-M 永续 BTCUSDT 的 bookTicker / aggTrade / depth5(永续领先现货 14–45 ms,是 11-09 唯一显著的 λ 因子)、Chainlink BTC/USD 价格与 60 s TWAP(结算规则:窗口末 TWAP ≥ 窗口初 TWAP → Up)。今天永续只在公司收数机上有;Chainlink 由实盘机上的 Python 旁路 `cl_feed.py` 以 `InjectWarmup` local_kind 1 推入,不随引擎启动、不进行情 journal 的时间轴(只有 ctl 命令时刻),没有 TWAP,重启后丢失,owner 2026-11-09 要求"不要旁路了"。
2. 模型(`fair − p − λ·Delta − c`)要进实盘策略、默认关闭,参数每 30 分钟重拟合一次,必须能在不重启引擎的情况下替换并留下审计记录。
3. 4 核实盘机(c8g.xlarge,`isolcpus=1-3`,`--cpu-plan spin`)多出永续(BTC bookTicker 约 612 条/s,赛跑两路 ≈ 1 200 帧/s)与 Chainlink(2 条/s)两个 feed。

## Proposal

### 1. 永续行情:venue `bp`(`binance-usdm`,只读)

- hft-pmcfg `--maker`/`--live` 缺省加 `[venues.bp]`(`protocol = 'binance-usdm'`、`VenueId::BINANCE_USDM` = 1、无账户);refdata 加 BTCUSDT 永续 inst(`InstKind::Perp`,与现货同一 BTC/USDT 资产指数)。`--no-perp` 去掉(输出与之前逐字节相同)。
- 协议已存在(venue-binance `Usdm`:`public-bbo` 赛跑 bookTicker、`public-topn` depth5@100ms、`market` 路由 aggTrade),不改协议代码,只做装配。
- binary-maker 订阅它的 `BBO | BOOK | TRADE`(否则 feed 不订阅、journal 里没有),只记录不参与决定;将来的永续因子读它。

### 2. Chainlink 原生:新 venue `polymarket-price`(crates/venue-pmprice)

- PolyBolt `wss://ws-live-v2.polymarket.com/ws`。实测(2026-11-09 实盘机):先 `{"op":"auth","rid":…,"auth":{apiKey,secret,passphrase}}` → `{"op":"authed"}`(未认证的订阅回 `{"op":"error","code":"auth_required"}`);每个订阅一帧 `{"op":"subscribe",…}` → `{"op":"subscribed","channel":…}`,随后一条 `snapshot: true` 的历史帧(`payload.data` = 最近约 120 个逐秒样本),之后每秒一条 `price.crypto` 与一条 `price.crypto.twap`(`window_seconds` 60);坏频道 `{"op":"error","code":"bad_channel"}`;`{"op":"ping"}` → `{"op":"pong"}`;`seq` 按连接计数,不跨连接。
- inst = 符号 `btcusd`(资产 BTC、计价 USD);`price.crypto` → `IndexPrice`(`StreamKind::Index`),`price.crypto.twap` → `MarkPrice`(`StreamKind::Mark`,"结算标记价");`ex_ts_ns` = 样本 `timestamp`;按样本时间戳 `Monotonic` 去重(快照帧逐项发出未见过的样本,重连后补上断线期间的秒)。
- 认证:行情连接的登录是新能力。hft-venue `SessionDriver` 增加"行情凭据"(只用于 `ConnRole::Md`),hft-core `[venues.<v>.md_login]`(`api_key_path`/`secret_path`/`passphrase_path`,与账户凭据同一套读取—解链路径、同一路径只读一次,可直接指向 `pm` 账户的同一组文件)。协议只取 secret 中第一个 `|` 之前的部分(Polymarket 账户 secret 文件是 `<api secret>|<EOA key>|<wallet>`),登录帧不进 journal(与下单连接相同)。
- `VenueId::POLYMARKET_PRICE` = 17、`Exchange::Polymarket`(只追加码值);只读协议进 `READ_ONLY_PROTOCOLS`;白名单主机 `ws-live-v2.polymarket.com`。
- SDK `EventRef::Index` / `EventRef::Mark` 追加 `ex_ts_ns`(样本时间,策略按 Chainlink 秒归档;只追加字段)。
- binary-maker:原生 Chainlink 组存在时规则 12 与模型读 `Index`(价格)与 `Mark`(TWAP),忽略 `InjectWarmup` local_kind 1;没有时照旧读旁路样本(旧 journal 回放逐位不变)。

### 3. Polymarket 全量

已原生:`book`、`price_change`(两个代币各一条 `L2Batch`,双连接去重 PR #93/#94)、`last_trade_price`。`tick_size_change` 与 `best_bid_ask` 只在 raw journal(策略的价格网格固定 0.01,下单端 `0.04–0.96` 外不挂);本次只补用例证明 `price_change` 两个代币都进簿,不新增事件类型。

### 4. 线程与核(实测见下节)

- 2026-11-09 实盘机 60 s 采样(live-20261108T142342,spin):核 1 sequencer 100 %(空转)、核 3 gw-pm 100 %(空转)、核 2 feed-pm 21.4 % + feed-bn 16.7 % + feed-cb 16.4 % = 54.5 %、核 0 persist 32.9 % + ctl 3.6 % + build ×3 6.6 % + hk 2.6 % + Python 若干(clfeed、heartbeat、bind、redeem、bot)。feed-cb 的消息率只有 bn 的约 1/20,CPU 却相同:核 2 的开销主要是 `park_us = 20` 的唤醒,不是解码。
- journal 写入:seq 0.70 GB/h、raw-pm 3.53 GB/h、raw-bn 0.09 GB/h、raw-cb 0.02 GB/h;永续估计 +0.27 GB/h seq、+0.3 GB/h raw(+14 %),persist 约 +5 % 核 0。
- 计划(`--cpu-plan spin`):`feed:bp` 进核 2(`park_us = 20`,永续是最快的参考,按 feedback-fast-defaults 走快路径;估计核 2 → 约 75 %);`feed:cl`(2 条/s,Chainlink 本身晚 1.1–1.4 s)放核 0、`park_us = 1000`;`conn-builder:*` 留核 0。sequencer / gw 不变。旁路 `clfeed.service` 停用后核 0 少一个 Python 进程。
- 后续(不在本次):hft-core 多任务线程(README 已规定 `tasks = [...]`,实现仍是一线程一任务)把 bn/bp/cb 合进一个 feed 线程,消除三份唤醒开销。

### 5. 成交条件 EV 模型(binary-maker 规则 14,schema v41,缺省关)

- Python 参照 `E:\trade\staging\fcev\sim2.py`(`Window.features`、`score_arr`、`Strat`);整数/定点实现,`f64` 只在特征计算(非下单路径,与现有 σ 计算同类),下单价仍为整数。
- 参数(v41,全部缺省 0 = 关):`fc_on`、`fc_s`、`fc_lam`、`fc_c0`、`fc_c1`、`fc_w`、`fc_m_post`、`fc_m_cancel`、`fc_m_pair`(< 0 = 不配对)、`fc_iv_n`(IV 中位数窗口,60)、`fc_ver`(参数版本号,审计用)。v39/v40 属于 owner 的研究线 `research/calm-fav-main`(`lo_*`、`pair_sticky`…),本分支不含;两条线合并时 v39/v40 字段排在 v41 之前。
- 开启时规则 14 取代挂/撤判定:在 token 最优买价挂 `clip_shares`,score > `fc_m_post` 才挂;score < `fc_m_cancel` 或不再在最优买价则撤;配对(`fc_m_pair` ≥ 0)时另一腿只用对价单(≤ 100 − 平均成本 − m_pair)买入。窗口末段、库存与风控上限照旧。
- 平价测试:仓外 Python 对若干录制窗口逐秒导出 (dist, sq, remf, sig, mid, px, score),Rust 用同一输入重算,|Δscore| ≤ 1e-6 c。

### 6. 参数热加载与定期重拟合

- hft-core `[strategy_params_file]`(可选):`path`、`instance`、`poll_ms`(缺省 5 000)。ctl-gateway 线程按 mtime + SHA-256 轮询该文件;内容为 `NAME=RAW` 行与一行 `version=…`;变化时以内部发起者 `file:<name>` 走与 `SetParams` 相同的命令路径(记 journal、过 `check`、`relax` 规则),结果写一行审计日志(版本、SHA、接受/拒绝码)。被拒绝时保留现参数(最后一个好版本)。文件只能改 `fc_*` 字段(白名单),其他字段 → 拒绝。
- 重拟合:研究服务器(128 核、已有 `~/data/pmrec` 收数与 `fcev/`)每 30 分钟 cron:拉新数据 → `sim2.py probe` 增量 → `wf2.py` 式指数衰减加权拟合(半衰期参数化) → 校验(界内、无 NaN、样本外半小时 EV 不劣于"不挂"阈值、参数相对上一版变化有界) → 写 `fcev_params_<ver>.txt` 与 `current` 符号链接 → 经 ssh 推到实盘机的参数文件(原子 rename)。杀开关:`fcev.disabled` 标志文件存在或校验失败 → 不推新版;推 `fc_on=0` 的"关闭"版即全局关闭。

## Alternatives considered

**Chainlink 继续走旁路(InjectWarmup)。** owner 否决(2026-11-09)。另外旁路样本在 journal 里只是 ctl 命令时刻,不是行情接收时刻,也没有 TWAP;重启要重新发现 socket。

**Chainlink 做成 venue-polymarket 的第二个协议。** 可以,但 venue-polymarket 受 hft-core `polymarket` feature 门控且已是千行级;PolyBolt 是独立端点、独立消息格式、只读,单独 crate 与 Coinbase 先例一致,fuzz/金样也独立。

**TWAP 由策略从价格样本自算。** 官方 TWAP 是结算依据(Chainlink 节点的 60 s 均值,样本缺秒时口径未知);直接记录官方值,策略可两者比对。

**用 InstKind 或符号区分参考组。** 策略 crate 不得出现交易所名;InstKind 只有 Spot/Perp,区分不了 Chainlink 与 Coinbase。改用显式参数 `ref_aux`(位 0 = 永续参考组,位 1 = Chainlink 组,按此顺序排在原有参考组之后),由 hft-pmcfg 写入,`check` 核对组数与永续组的 InstKind。

**参数热加载由仓外脚本调 ctl `SetParams`。** 可行(机器人已这样做),但 owner 要求引擎自己取文件、留审计行;文件轮询放在 ctl-gateway(warm 线程),命令照常进 journal,决定可复现。

## Acceptance criteria

- venue-pmprice:金样(实盘机 2026-11-09 抓包)逐条一致;快照帧只发出未见样本;secret 只取 `|` 之前(用例断言登录帧不含第二段);fuzz 目标。
- hft-core `--check` 能加载 `--live` 生成的含 `bp`、`cl` 的配置;`md_login` 缺文件 → `CredentialRead` 退出 2。
- binary-maker:`ref_aux = 0` 时决策与 v38 逐位相同(既有场景全过);原生 Chainlink 与旁路样本给出相同的规则 12 输入。
- 模型平价:≥ 3 个录制窗口逐秒 |Δscore| ≤ 1e-6 c。
- 实盘机 24 h Shadow:核 2 平均 ≤ 80 %、BBO 到决策 p50 不劣于 11-06 基线 10 µs 以上;Chainlink 样本缺秒率 ≤ 旁路。

## Risks

- 永续 BBO 让 sequencer 事件数约 ×2(spin 核上无额外延迟,但 journal +14 %,磁盘 134 GB 余量按 4.4 GB/h 算约 30 h,依赖现有归档)。
- PolyBolt 认证用的是交易账户的 CLOB API key;若该 key 被吊销,Chainlink 与下单同时失效。可另配一套只读用途的 key(`md_login` 指向另一组文件)。
- 模型的 σ 缩放跨行情不稳定(Spec §6.1);所以缺省关、参数半小时重拟合、有杀开关。
- schema v41 跳过 v39/v40:合并研究线时字段顺序需按版本排列,否则旧参数 blob 解码错位(gen-schema 金样会报)。
