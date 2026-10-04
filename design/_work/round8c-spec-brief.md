# 第八轮第三次修订简报(M0-c;所有修订子代理先读)

依据:`round8-decisions.md` 的 **R8-9**(用户决定)与 `review-2026-10-03-M0b独立评审.md`(第二次独立评审,含出处行号)。本简报是本次修订的**唯一权威命名与数值来源**;与旧规格或上一轮简报冲突时以本简报为准。写作规则不变(`spec-brief.md`、`round8-spec-brief.md` §1:中文、标识符英文、README 模板、一件事实一个归属地、未核实标"待核实"、大文件用 Write 工具、只改自己清单里的文件)。修订前副本:`E:\trading\_backup\round8c-pre\`。

## 1. 用户决定的落地

### C1 不设小额实盘(R8-9 第 1 条)

- 交付路径:**dev2**(开发测试 + 交易所测试网/模拟盘联调 + 模拟账户长跑;**不放正式实盘账户**)→ **统一升级主机**(prod + sim 两个实例同机)→ **正式实盘**。
- dev2 上运行的实例:`sim` 实例(只含 Sim 账户、不持任何 key);测试网/模拟盘联调时另起一个指向测试网端点、持测试网 key 的 `prod` 角色实例(只在联调窗口运行,refdata 只含测试网账户)。
- 上线门槛(`docs/testing.md` §8 唯一归属)删除"小额实盘";正式实盘前必须满足:① 单元/属性 ② 金样 ③ fuzz 累计 ④ DST 累计 ⑤ 恢复与混沌 ⑥ 三所测试网/模拟盘联调(Binance 测试网、OKX 模拟盘、Bitget 模拟盘,逐所待核实可用性与 API 覆盖)⑦ 模拟账户 ≥ 14 天(含 C8 的"单边 +30%"压力用例必过)⑧ 解耦门槛 ⑩ 升级主机上的延迟验收 ⑪ 运维与演练。③④ 只卡正式实盘,不卡模拟。
- 延迟验收(⑩)在升级主机上进行:实时行情 + 测试网/模拟盘下单回路测主机内分段;prod 与 sim 同机运行时测。
- 删除各处"小额实盘""新子账户小额"字样(launch、testing、11 章、cookbook、hft-sim 的校准节改为"与正式实盘并行校准")。正式实盘的风险上限由运营在 `RiskParams` 设定(建议从保守值起步,只是建议,不是门槛)。

### C2 sim 与 prod 同机(R8-9 第 4 条)

- 升级后主机同时运行 `prod` 与 `sim` 两个 hft-core 实例(同一二进制)。隔离规则:sim 实例只用非隔离核(warm/cold 核),永不占 prod 的热核;独立的 `hft-sim.slice`(CPU、内存上限);独立的 journal 与冷盘目录;**独立的出口 IP**(主机第二个私有 IP + EIP,sim 的交易所连接绑定它,不占用 prod 的 IP 级限速)。
- 主机规格必须同时容纳两个实例:`c8` 很可能不够,升级主机档位在 M7 前按实测选定(候选:16 vCPU/32 GB 级别,写成 `c16` 主机类候选);hft-core 内存表与线程计划给出"prod + sim 同机"一列。
- 删除"sim 实例放哪台主机待定"的开放问题,改为"与 prod 同机,主机档位待定"。

### C3 pipeline 自动 + 覆盖项按实例(R8-9 第 2 条;评审 P0-2)

- pipeline 主体的 `SetParams` 在 Live 账户上同样为中性(含新增 group、`open_allowed` 0→1),不需确认。
- **人工覆盖项按实例分键**:`overrides/<instance>.toml`;校准对共享覆盖项必须显式 `scope = pair`(默认 `instance`)。ops_bot `/params <实例> set` 只写该实例;作用于 Live 实例的人工修改在 ops_bot 侧仍需确认(既有规则)。

### C4 CI Free(R8-9 第 3 条;评审 P1-9)

- 预算(私有仓库 2,000 min/月,与 Trading 仓库共享):每夜 x86 ≤ 20 min + arm ≤ 5 min(约 750 min/月);PR 门槛目标 ≤ 10 min;每月为两个仓库的发布构建预留 ≥ 300 min;用量超过 80% 时先缩每夜批量。预算表唯一归属 `docs/development.md`,testing.md 链接它。
- ③④ 累计门槛只在正式实盘前要求;漏跑一夜不清零,只有失败清零;累计账本丢失时按 CI artifact 历史重建,无法重建才清零。

### C5 紧急停止(R8-9 第 5 条;评审 P1-8)

- `/stop` = 非 HARD、不带 flatten 的 `Kill{Global | Account}`:禁止新开仓、撤销挂单;已成交第一腿的对冲与平仓照常(RISK_REDUCING 放行)。
- Viewer 的 Kill 确认绑定预览的 `cmd_hash` + TTL(60 s),**不比对 `ctl_state_version`**(其他命令仍比对)。
- hft-core 心跳过期或 ctl-gateway 不可达:ops_bot 改为触发 kill-switch 的本地撤单入口(kill-switch 新增只接受 ops_bot uid 的本地触发套接字),同时 P0 告警给值班交易员;`/stop` 失败或 10 s 无结果 → P0 告警。
- kill-switch 仍只撤单,不平仓。

### C6 portfolio-carry 单 group 强制退出

- per-group 参数 `force_exit: Bool`(默认 false)与 `StrategyCmd` 命令 `exit`(`reduce_only = true`)→ 该 group 目标设 0、`reason = Exit`,跳过成本门(成本门只对 `Rebalance` 生效;`Exit`/`Hard`/`MarginStress` 不受成本门约束)。负资金费的硬性退出规则留到模拟阶段研究。

### C7 MarginStress 改按到强平距离(评审 P0-3)

- 度量:每个 (account, venue) 的 `liq_dist = min over 永续持仓 |liq_px − mark| / mark`(空头为 `(liq_px − mark)/mark`);来源 `MarginReport`(交易所给的 `liq_px`)与 ledger 的估算取更保守者。
- 分档默认(待标定):L1 `liq_dist ≤ 25%` 禁开;L2 `≤ 15%` 通知策略减仓(截止 30 s);L3 `≤ 10%` 或截止已过 → 引擎强制减仓(永续先,每步 20%);数据过期 → L1。原比率(维持/保证金余额)保留为交叉校验:比率 ≥ 0.8 → 至少 L2。参数名:`liq_dist_block_open` 0.25、`liq_dist_reduce` 0.15、`liq_dist_force` 0.10、`liq_dist_hyst` 0.02。
- hft-sim 增加"单边 +30%(1 h 内)"压力用例,作为门槛 ⑦ 的必过项:不得出现强平。
- 永续杠杆:hft-risk `LINK_FUNDS` 用 `AccountConfigReport` 读到的实际杠杆(删除常量 `perp_leverage` 默认 1);portfolio-carry 的 `perp_margin_frac` 默认与运营设定的交易所杠杆一致,`launch.md` 写明设置步骤。

## 2. 评审其余问题的规范解法

| 编号 | 解法 | 归属 |
|---|---|---|
| P0-1 报价新鲜度 | 新鲜度改为**连接健康**:`BboView` 增加 `conn_live_tsc`(该 BBO 连接最近一次收到任何帧——数据、pong、心跳——的时刻)与 `resync: bool`;hft-venue 对 BBO 连接每 `bbo_ping_ms`(1 000)发 WS ping 保证有回帧;判定 = 连接 Live、非重同步、`now − conn_live_tsc ≤ conn_liveness_ms`(2 000)。对冲腿、风险递减单、自动平仓、L3 减仓、链接腿 #5 一律用此判定,报价为该连接上的最新值(无变化即有效);`max_mid_age_ms` 退役;DST 加"第二腿盘口 10 s 不变"必过 | SDK `BboView`、hft-venue、hft-risk #5、hft-oms `price_hedge`、hft-exec、hft-strat-lib(对齐其"安静不算过期"的说法)、testing |
| P1-1 flatten 被覆盖 | `engine_flatten` 置账户级 `flatten_lock`:直到 `Resume{Kill}` 前,该账户只接受比当前更接近 0 的目标与 Reduce/Flatten 意图 | hft-exec、hft-risk |
| P1-2 两腿 lot 对齐 | 切片与第一腿数量按两腿 lot 的较粗者(一方整除另一方时)取整,保证第二腿量 ≥ 其 `min_qty` 且为其 lot 的整数倍;不能整除时取最小公倍数,其名义 > `max_chunk_notional` → `SetParams` 拒绝(`InvalidArgs{LotMismatch}`,sequencer 通用校验) | hft-exec、hft-sequencer、control-plane(detail 码)、hft-oms |
| P1-3 跨重启时间 | `Now.engine_ns`:跨重启连续、停机不计的引擎时间(由 journal 的运行区间累加,确定);策略、strat-lib、ledger、risk 的持久状态一律存 `engine_ns`,TSC 只在单次启动内使用;OMS/exec 的 `recovery_end` 平移保留 | SDK、hft-strat-lib、portfolio-carry、hft-ledger、hft-risk、hft-sequencer |
| P1-4 每把 key 核 UID | hft-core 启动对每把交易 key(Ed25519 会话 key 与 HMAC REST key 分别)查 UID;kill-switch 自检与 reconciler 启动同样查;不等于 `venue_uid` → 退出 2 + P0 | apps/hft-core、apps/kill-switch、apps/reconciler、venue-* |
| P1-5 KillWord 位宽 | 全系统 `AtomicU32`(布局归 hft-types);hft-venue `kill_mask: u32`,判定含 `account_hard(ix)`;启动自检经真实 store/load | hft-types、hft-rt、hft-venue、hot-path-rules、apps/hft-core |
| P1-6 dev2 资源 | dev2 不再有正式实盘(C1);仍修:`hft.slice` 拆为 `hft-core.slice` 与 `hft-batch.slice`(replay-verify、archiver、pipeline、treasury),各自上限;hft-core `OOMScoreAdjust` 在 hft 单元中最低;/dev/shm 空间检查扣除本实例已有的段 | apps/hft-core、deploy、hft-journal |
| P1-7 余额与资金费 | reconciler 分级前用同组报告的 Funding 条目修正结算资产的余额差;ledger `NoHistory` 在窗口结束后按交易所金额入账并写 `Note{FundingCorrected}`(P1),不再要求人工 | apps/reconciler、hft-ledger、hft-risk |
| P1-11 c8 线程与 T8 | 每个下单 venue 的 gw 独占线程(T8 无例外);核不够 → 更大的主机类;`deploy` 的隔离核列表由 hft-core 线程计划生成,不手写 | apps/hft-core、deploy |
| P1-12 venue 两处 | OKX:数量换算只走 refdata 线格式指数(含 ctVal 平移),venue-okx 运行时不再除 ctVal;金样加 ctVal ≠ 1 与 1000PEPE 类。去重键 = `(inst, trade_id)`;0 或负的成交 id(强平/ADL)不入环,改用 `(inst, exch_order_id, fill_seq)` | venue-okx、hft-venue-proto、venue-binance、venue-bitget、fixtures |
| P2 | Advice 跨启动过期只由策略按 `basis_seq < Now.boot_seq` 判定,sequencer 不加标志(删除其"标为过期"说法);固定容量溢出不丢弃,超出部分经续推定时器在后续轮次处理(`ModeOutcome.cells` 容量 = `max_cells_total`);容量数字唯一归属 apps/hft-core,其他处链接;`TargetOrigin::Engine(AutoFix)` 在 hft-event origin 码表登记;`RISK_REDUCING` 位值唯一归属 hft-event `OrderFlags`,hft-oms 链接;incident-response 改为"/hedgefix 只平多出的一腿";毒丸两层分工写清(hft-core `boot_guard` 管恢复期崩溃,sequencer InputHash 管运行期崩溃,计数都在 `DurableMeta`);`AdvisorLag` 唯一定义在 hft-event(滞后 = `engine_ns` 差,超阈值时每 10 s 至多写一次);成本门公式改为 `cost × 1天 ≤ max_break_even_ms × gain_daily`;replayer sim 默认 `--advice recompute`;reconciler 依赖表补 hft-oms(只读类型)并把"hft-risk reconcile 模块"改为指向 hft-risk 的对账分级节;pipeline 的策略插件移到 `strategies/<name>/pipeline/`(Python 包随策略),doc-check 也扫 python;decoupling-gate 的豁免标签需 Admin 批准并在 CI 报告中列出;**hft-testkit 不依赖 registry**(需要真实策略的集成测试放在 `strategies/registry/tests/` 或 `apps/hft-core/tests/`);交易所事实:Binance RecvWindow 的 NotFound 不定终态(按 Unknown + 成交历史确认)、心跳口径按产品线分别写、countdown 超时 ≥ 2 × 刷新周期、Bitget 单向持仓的强平标记待核实、现货账户必须关闭借币(OKX `enableSpotBorrow = false`、Bitget 现货杠杆关闭)并纳入 `ACCOUNT_CFG` 核验 | 各归属文件 |

## 3. 工作分配(只改自己清单里的文件)

| 代号 | 文件 |
|---|---|
| H1 | `architecture-v2/README.md`、`00`–`13`(13 末尾追加 §16"第八轮第三次修订")、`现状测量…`、`待测量…` |
| H2 | `hft/crates/hft-strategy-sdk`、`hft/crates/hft-strat-lib`、`hft/strategies/**`(含把 pipeline 策略插件规格移入 `strategies/<name>/pipeline/`,与 H8 协调:H2 写策略侧插件规格,H8 写框架侧)、`hft/docs/cookbook/adding-a-strategy.md` |
| H3 | `hft/crates/hft-exec`、`hft/crates/hft-oms` |
| H4 | `hft/crates/hft-risk`、`hft/crates/hft-ledger`、`hft/crates/hft-sequencer` |
| H5 | `hft/crates/{hft-types,hft-event,hft-refdata,hft-ctl-proto,hft-ctl}`、`hft/docs/subsystems/*`、`hft/docs/cookbook/{adding-an-event-kind,adding-a-control-command}.md`、`hft/tools/README.md` |
| H6 | `hft/crates/{hft-venue-proto,hft-venue,hft-net,venue-binance,venue-okx,venue-bitget,venue-gate,venue-bybit,venue-kucoin,venue-hyperliquid,hft-sim}`、`hft/apps/{hft-core,replayer}`、`hft/docs/cookbook/adding-a-venue.md` |
| H7 | `hft/apps/{reconciler,watchtower,kill-switch,journal-archiver,mock-exchange}`、`hft/crates/{hft-testkit,hft-telemetry}`、`hft/docs/testing.md`、`hft/benchmarks/*`、`hft/fixtures/README.md` |
| H8 | `hft/python/**`、`hft/deploy/*`、`hft/docs/development.md`、`hft/docs/runbooks/*`、`hft/docs/postmortem/README.md`、`hft/.agents/notes/**`(新 ADR:`2026-10-03-{no-small-live,sim-colocated-with-prod,margin-liq-distance,quote-freshness-conn-health,engine-time}.md`;更新索引与受影响旧 ADR 的状态行,含 `sim-only-instance`、`viewer-emergency-stop`、`ci-free-hosted`、`margin-stress`) |
| H9 | `hft/{AGENTS.md,CLAUDE.md,README.md}`、`hft/docs/{AGENTS.md,architecture.md,glossary.md,defensive-patterns.md,hot-path-rules.md,latency-budget.md}`、`hft/crates/AGENTS.md`、`hft/apps/AGENTS.md`、`hft/docs/cookbook/adding-a-crate.md`、`hft/crates/{hft-journal,hft-rt,hft-book,hft-py,hft-chain-evm,hft-ext-proto,hft-amm,hft-chain-solana}`、`hft/apps/{chain-core,chain-grpc-relay}`、`hft/contracts/README.md`、`architecture-v2/_work/check_hft.py`(新增检查:"小额实盘"残留、`perp_leverage` 常量残留、`max_mid_age_ms` 残留、`AtomicU8` KillWord 残留、引擎 crate(含 dev 依赖)依赖 registry 的说法、python 目录中的策略族名) |

## 4. 最终报告(简短)

1. 改了哪些文件。2. C1–C7 与 §2 各项中你负责的落在哪里。3. 需要别人同步的名字或数字。4. 仍有风险的地方(≤ 5 条)。

## 5. 主控补充裁决(修订与合并过程中)

1. 上线门槛不重新编号:保留 ①–⑧、⑩、⑪,⑨ 标"已删除(原小额实盘)";里程碑 M6 联调与模拟长跑(dev2)、M7 统一升级 + 延迟验收、M8 正式实盘。
2. T8 在所有主机类(含 dev2)成立:feed 可合并,每个下单 venue 的 gw 独占线程;加载时校验。
3. `HedgePxLimits{conn_liveness_ms, rr_max_cross}` 与 `pair_lot` 只在 hft-types 定义;`OrderFlags` 位值只在 hft-event。
4. `AccountConfigReport` 读不到某合约杠杆(`leverage = 0` 或 PARTIAL)→ 该合约的链接开仓按 `LINK_FUNDS` 拒;必需配置项读不到才关 `ACCOUNT_CFG`。
5. 预热数据按策略插件产出的块注入(每块 ≤ 4096 B、一块一个 `chunk_no`、不再切分)。
6. MarginStress 比率交叉校验参数名 `margin_ratio_crosscheck`(0.80)。
7. 资金费校正只做一次:reconciler 发出的余额条目已含校正,hft-risk 不再重复。
8. 合并时补:`Note{IdentityMismatch}`;成交去重键 `(inst, trade_id)`,0/负 id 用 `(inst, exch_order_id, fill_seq)`;SDK `RiskStateView.flatten_lock`、`as_of_ens`/`src_ens`;`EngineClock.base_engine_ns` 为 i64;`Timer{Sys, NotifyContinue = 17}`;快照节 26 `ConnLive`;hft-core 启动校验 `conn_liveness_ms ≥ 2 × bbo_ping_ms`;`DurableMeta.poison_input`;dev2 上全部 hft 单元 `OOMScoreAdjust` 整体为正(旧系统后于 hft 被杀);dev2 的 sim 实例同样用独立出口 IP;测试网 → 正式实盘切换允许 `config_source = file` 首次启动;hft-py `heartbeat.read`;xtask `dst --strategy`。
9. 章节审计(`_work/heading_audit.py`):与 round8b-pre、round8c-pre 相比缺失的标题均为改名或有意删除(小额实盘、插件迁移);M0-b 时 hft-exec 丢失的"残余与尘埃""外来成交的默认处理"两节已在本轮恢复。
