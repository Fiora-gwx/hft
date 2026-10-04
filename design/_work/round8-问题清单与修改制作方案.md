# 第八轮:确定版问题清单、设计修改方案、制作方案(2026-10-02)

依据:[round8-decisions.md](round8-decisions.md)(用户裁决)、[review-2026-10-02-主策略适配与执行正确性.md](review-2026-10-02-主策略适配与执行正确性.md)(评审证据与出处)。本文只定"改什么、按什么顺序做",规格正文的修改在用户确认后按 §2 的工作包执行。

---

## 1. 确定版问题清单(按新前提重新分级)

### A. 设计级(写代码前必须在规格里解决)

| # | 问题 | 级别 | 来源 |
|---|---|---|---|
| A1 | **策略与引擎耦合**:所有策略在一个 crate 的封闭枚举 `StrategyKind`;参数是封闭枚举 `ParamsBody`(新增策略要改 hft-cfg、ctl 协议);慢计算须在 hft-event 登记 `AdviceKind`,载荷固定 `[i64; 6]`;sequencer 内写死资金公式 `wallet × leverage × deployable`;OMS LegTracker 内写死 cdex 的 5% 补腿阈值与残余清理规则;hft-strat-lib 混合通用构件与 cdex 专用规则;pipeline 按策略写死;cookbook"新增策略"要改引擎 | P0 | R8-3;评审 §1 |
| A2 | **没有承载组合型策略的接口**:无目标仓位意图、无执行层(切片/目标跟踪)、无按交易所的权益视图(现金、现货库存、永续保证金余额、可用保证金)、策略不能拥有自己的慢计算(168 h 分位、资金费统计、跨币分配) | P0 | 评审 §1 表 1–8 |
| A3 | **交易所写死**:C 族写成"Binance 永续 + Gate 现货";没有交易所能力表(手续费扣币种、reduceOnly、持仓模式、资金费推送方式、最小名义);Gate 实现顺序把期货定为主用;reconciler 只列了 Gate 期货端点 | P0 | R8-1;评审 P1-7/8 |
| A4 | **没有模拟系统**:Shadow 只记录意图、不模拟成交,目标跟踪策略在 Shadow 下无法评估;现有 hft-sim 是 hftbacktest 回测内核(optional) | P0 | R8-5;评审 §1 表 11 |
| A5 | **账户级保证金管理缺失**:无保证金率分档降级、无强平距离/ADL 数据来源、treasury 是 optional;组合携带普涨 50–60% 即接近强平(评审 §2.2 估算) | P0 | 评审 §2.2、P0-3 |

### B. 执行正确性 P0(规格漏洞,会亏钱或留裸腿)

| # | 问题 | 评审编号 |
|---|---|---|
| B1 | 开仓风控只检查第一腿所在交易所 | P0-1 |
| B2 | 对冲重试生成 `role=HedgeFix` 被分类为 reduceOnly,开仓对冲重试必被拒 | P0-2 |
| B3 | 自身强平/ADL 成交被当作 ForeignOrder,不更新仓位;无保证金/ADL 数据 | P0-3 |
| B4 | 现货手续费按基础币扣除时,仓位/对冲/平仓按毛量计(推广为能力表 `fee_asset_rule`) | P0-4 |
| B5 | 启动闸门 RECONCILED 对任何量级差异放行;干净重启自动解除 Recovery | P0-5 |
| B6 | 对冲/HedgeFix/Flatten 无数量与名义上限,检查链自相矛盾 | P0-7 |

### C. P1

| # | 问题 |
|---|---|
| C1 | NotFound 宽限后定 Rejected → 迟到成交双重对冲;槽回收后的成交不入账 |
| C2 | 恢复时输出不一致只告警、状态按重算推进 |
| C3 | watchtower 对账规则永不触发;`NakedExposure` 与 ledger 同源看不见漏记 |
| C4 | 自动 HedgeFix 无次数与冷却上限,可自激循环 |
| C5 | ClockBad、Halted 会挡平仓与硬动作 |
| C6 | 启动不核验交易所账户配置(持仓模式、保证金模式、杠杆等;推广到所有交易所) |
| C7 | 现货余额不是一等"库存";非策略资产无基线;网页手工操作后现货余额无法采纳入账(原 P0-6 中迁移部分降为 optional) |
| C8 | HARD 只能全局;确定性毒丸输入使恢复反复崩溃;新版本崩溃后回滚路径被 `VersionNeedsDrain` 堵死 |
| C9 | API key ↔ AccountId 无交易所身份核验 |
| C10 | 磁盘/persist 退化无自动降级(runbook 却写已降级) |
| C11 | Drain 不等待 HedgeRetry 间隙与已 Ack 的 IOC,可能带着裸腿退出 |
| C12 | 用户流未知事件类型会被当作解码失败,引发查询风暴并断流 |
| C13 | 全仓下资金费推送可能不带 symbol(**需核实**)→ 资金费按币归属改为引擎按"持仓 × mark × 费率"合成,余额事件只校验,对账用带 symbol 的流水校正 |
| C14 | 平仓腿顺序固定为"先现货";保证金驱动的退出应先减永续 → 执行层按退出原因选择腿顺序 |
| C15 | 下架公告、资金费结算周期变化没有事件路径 |

### D. P2 / 文档

- 评审子报告列出的 33 处文档矛盾(金丝雀残留、journal `instance "shadow"`、01 章环优先级旧版、dev2 内存 600/900 MB、c8 journal 20 GiB 超物理内存、benchmarks 把 c8 延迟写成门槛、Shadow 7/14 天、两套二次确认协议等)。
- 同一事实多处书写(dev2 内存 5 处、段大小 4 处、回执规则约 8 处)→ 只留一个归属地。
- 私有结构的字节偏移写得过细 → 以代码 + gen-schema 为准,README 只写语义。
- `_work/check_hft.py` 只查"阶段 N",查不出 Canary、"第 N 周"、引擎 README 中的策略名。

### E. 开发环境(第八轮新增)

| # | 问题 |
|---|---|
| E1 | 本地是 Windows,而运行时 crate(hugetlbfs/tmpfs、busy-poll、epoll、绑核)只在 Linux 上成立;本地连不上 Binance 等交易所 → 交易所联调只能在服务器 |
| E2 | 服务器同时跑 cdex 正式账户(2 vCPU、3.8 GB、swap 已用约 0.8 GB):新框架必须有资源护栏 |
| E3 | `E:\trading\hft` 还不是 git 仓库,没有 CI;现有规则"不在生产服务器上编译" |

### 已不再适用(删除或降为 optional)

迁移前 Shadow 与旧引擎共用账户(无共享账户)、cdex 迁移手册与现货腿采纳(cdex 迁移时再做)、策略 A 的 Gate 领先-滞后证据(A 预留)、工期压缩、独立 Shadow 实例(用户选择在现服务器开发测试)、链上全部条目。

---

## 2. 设计修改方案(工作包,按文件)

### WP1 决策与总纲(`architecture-v2/`)

| 文件 | 改动 |
|---|---|
| `README.md` | 一页结论第 7、8、11 条:新框架以 ccex 为主,链上转可选;策略与工程解耦;模拟系统替代回测;交付改为"新子账户上线",cdex 迁移可选 |
| `00` | 目标顺序写明"延迟 → 正确性 → 可扩展";新增目标"新增/修改策略不改引擎";非目标加"历史回测评估";§8 与雏形关系表更新(portfolio-arb = 策略雏形) |
| `01` | 拓扑加入策略 SDK 层、执行层、模拟网关、账户类型;§8 portfolio-arb/cdex 两行改写;旧环优先级与 housekeeping 持久化残留修正 |
| `07` | 重写:策略包(快速部分/慢速部分/参数 schema)、SDK 接口、两层输出(目标/订单)、执行算法、引擎与策略的风控边界、ccex 策略族清单(组合携带首个实现;永续×永续、同所现货×永续可接入;A/B 预留);cdex(D/E)移为可选节 |
| `08` | 回放 = 确定性验证与排障,不评估收益;模拟账户的 journal 语义 |
| `09` | 风控边界;账户保证金分档 cap(MarginStress);treasury core;watchtower 规则修正 |
| `10` | workspace 布局:`crates/hft-strategy-sdk`、`crates/hft-exec`、`strategies/<name>`;依赖规则与机检 |
| `11` | 重写交付计划:ccex 首要、按 §3 里程碑、dev2 护栏、新子账户上线、统一升级后延迟验收;cdex 迁移、链上为可选 |
| `12` | 风险登记册补:逼空/保证金、ADL、下架、结算周期变化、模拟失真、开发与 cdex 共机 |
| `13` | 追加 §14 第八轮摘要 |
| `现状测量…`、`待测量…` | 标注与第八轮不再相关的项(E4 首批交易所改为待定、链上项转可选) |

### WP2 策略 SDK 与解耦(`hft/`)

| 文件 | 改动 |
|---|---|
| 新建 `crates/hft-strategy-sdk/README.md` | `Strategy`(快速部分)、`Model`(慢速部分,advisor 运行)、`ParamSchema`;只读视图 `MarketView`/`PortfolioView`/`AccountView`;`Ctx`(订单意图、目标意图、定时器、Note);不透明 Advice 载荷编解码;SDK 稳定性规则(版本、兼容) |
| `crates/hft-strategies/` → `strategies/` | 拆为一策略一 crate:`strategies/portfolio-carry`(core,首个实现,结构取自雏形:打分 → 分配 → 目标 → 调仓,公式全部参数化或可替换,不照搬雏形缺陷);`strategies/cdex`(D/E,optional);`strategies/cross-taker`(A,optional);`strategies/maker-taker`(B,optional) |
| `crates/hft-strat-lib/README.md` | 只留通用构件(价差、确认窗口、脏数据过滤、节流、分位统计、资金费统计、冲击估算);cdex 专用(Sizer 因子、ClosePolicy、FundingGate 常数、SwapPicker)移入 `strategies/cdex` |
| `crates/hft-sequencer/README.md` | 对 `StrategySet` 泛型(由 app 装配处的注册宏生成,静态分派);删除资金公式,改为向策略提供原始权益数据;Advice 按 strategy id 路由不透明载荷;参数经注册表的 schema 解码 |
| `crates/hft-event/README.md` | Advice 改为变长不透明载荷 `{strategy, account, group, local_kind, len, bytes}`;`StrategyParams` 为带 schema id/版本的不透明 blob(删除 `ParamsBody` 枚举);新增 `ExternalFill`、`MarginReport`(保证金率、强平价、ADL 档)、`AccountConfigReport` |
| `crates/hft-ctl-proto`、`crates/hft-ctl`、`docs/subsystems/control-plane.md` | `SetParams` 携带 schema 标记的 blob,ctl-gateway 用注册表解码;新增 `GetSchema`;模拟账户命令(重置、模拟划转) |
| `tools/README.md`(hft-cfg) | 按 schema 通用的 TOML → blob;导出各策略 schema(JSON)供 Python 使用 |
| `python/ops_bot`、`python/pipeline` | 参数与面板按 schema 通用;pipeline 改为每策略插件(cdex 插件 optional) |
| `docs/cookbook/adding-a-strategy.md` | 重写为"零引擎改动":模板 crate → schema → 快/慢部分 → 一行注册 → SDK 场景测试 → 模拟账户运行 → 小额实盘 |
| `crates/AGENTS.md`、`docs/architecture.md` §7–§8 | 分层加入 SDK、执行层、策略包;依赖与 doc-check 机检规则(R8-3 第 6 条) |

### WP3 执行层(新建 `crates/hft-exec`)

- 执行算法:`TargetTracker`(目标 → 切片;滞回带、紧急度由策略参数给出)、`TakerSlice`(单笔 = min(差额, 弱侧深度 × 比例, 单笔上限))、`PairOnFill`(按退出原因选腿顺序:开仓流动性差的先、保证金驱动的平仓永续先)、`Simultaneous`(失败反向)、`MakerFirst`(预留)。
- 与 OMS 的边界:OMS 只管订单槽状态机与 LegLink 机制;"下什么单"由执行层决定;补腿阈值、残余/尘埃规则、重试次数与截止全部是参数。
- 落实 B2(重试保留 `Second` 角色)、B4(按净量对冲)、B6(对冲量 ≤ 触发成交 × 比例 + 1 lot、名义上限)、C4(补腿计次与冷却)、C14。

### WP4 引擎正确性(`hft-oms`、`hft-risk`、`hft-ledger`、`hft-sequencer`、`apps/reconciler`、`apps/watchtower`、`apps/kill-switch`)

- `hft-risk`:B1(`IntentView` 带对冲腿交易所,开仓同时检查两腿交易所;同账户 Broken 挡全部开仓)、B6(检查链对全部类别生效,为风险递减单预留令牌)、C5、C8(按账户 HARD)、A5(MarginStress 分档:禁开 → 通知策略减仓 → 截止后引擎按比例默认减仓)。
- `hft-ledger`:按交易所的权益视图;现货库存为一等对象;净量手续费;`ExternalFill`;余额基线与余额采纳(C7)。
- `hft-oms`:C1(墓碑、迟到成交入账并报 P0)、cdex 规则移出。
- `hft-sequencer`:C2(不一致 → 该交易所账户全部 Unknown、闸门严格化)、C8(毒丸隔离、带严格对账的从快照恢复)、C11(Drain 等待全部链接与 IOC 终态)、C10(persist/磁盘退化 → 条件型 cap)。
- `apps/reconciler`:按能力表覆盖各交易所现货 + 永续的挂单/成交/仓位/余额;B5(全部 Ignore 或逐项采纳才放行;`tail_lost` 时禁自动 Resume);保证金与 ADL 数据(或由 gw 的 REST 池提供)。
- `apps/watchtower`:C3;保证金 P0 规则;资金费数据过期。
- `apps/kill-switch`:与"一策略实例一子账户"一致的目标表。

### WP5 交易所层通用化(`hft-refdata`、`hft-venue-proto`、`venue-*`)

- `hft-refdata`:`VenueCaps`(按市场类型的手续费扣币规则、reduceOnly、持仓模式、资金费推送方式 PerSymbol/BalanceOnly/RateOnly、最小名义、死手、现货/永续、限速模型);`AccountMeta` 增加账户类型(Live/Sim)、交易所 UID(C9)、绑定的策略实例。
- `hft-venue-proto`:契约补充——账户配置查询与核验(C6)、保证金/风险报告解码、可忽略事件表(C12)、成交带手续费资产。
- `venue-binance`:持仓/保证金模式核验、`positionRisk`/ADL、`MARGIN_CALL`、强平/ADL 订单识别为 `ExternalFill`、全仓资金费处理(C13)、可忽略事件。
- `venue-gate`:现货优先;现货端点;手续费资产。
- `venue-bitget`:升为候选(首批交易所待定);按同一能力表复核现货与永续规格。
- 首批实现哪两三家在 M3 开始前确定;规格对所有候选按能力表写齐。

### WP6 模拟系统(`crates/hft-sim` 改定位为 core)

- **模拟账户**(`AccountKind::Sim`)绑定真实交易所行情;订单进入进程内模拟网关(warm 线程,不占热核),回报走同一回报环并记入 journal(回放确定)。
- 撮合:吃单按下单时刻 + 模拟延迟(取自实测 RTT/回执分布,可配置)对实时 L2/BBO 逐档成交;**已吃流动性记忆**(短时衰减,避免连续下单重复吃同一档);部分成交;按 refdata/能力表执行拒单规则(步长、最小名义、价格限制、reduceOnly);手续费与扣币种;资金费按真实费率在结算时刻结算;永续全仓保证金与强平计算;现货余额;划转以命令模拟。挂单撮合(保守:成交穿价才成交)预留给 `MakerFirst`。
- 同时运行多个模拟账户(不同参数);报告:按策略的 PnL 分解(资金费、基差、手续费、相对到达价的滑点)、成交率、两腿间隔、裸敞口、保证金占用;经 journal-archiver/watchtower/ops_bot 展示。
- 小额实盘开始后,同参数模拟账户与实盘并行,校准滑点/成交率/延迟。
- testkit 的最小撮合器与模拟撮合共用一套代码。
- 从 core 移除:hftbacktest 假设回放、`replayer` 假设模式与参数扫描、research 回测流程(保留 `replayer verify`)。

### WP7 运维与开发环境(`deploy/`、`docs/development.md`、`docs/testing.md`、`docs/runbooks/`、`python/treasury`)

- 开发环境:纯 crate 在 Windows 本地编译测试;Linux 专属运行时在 WSL2(待确认)与 CI;交易所联调只在服务器;CI 产出 aarch64 制品,服务器不编译。
- **dev2 护栏**(服务器与 cdex 正式账户共机):systemd slice 限 CPU 与内存(具体数值在 M3 前按实测定)、低调度优先级、不忙等(SpinThenPark/park)、journal tmpfs 段大小上限、cdex 健康检查异常时自动停新框架;cdex 优先。
- `docs/testing.md` §8 改为"上线门槛":单元/属性/金样/fuzz/DST、交易所测试网(有则用)、模拟账户运行若干天且报告无未解释异常、新子账户小额实盘、演练。
- `runbooks/migration.md` 转 optional(cdex 迁移时用);新增 `runbooks/launch.md`(新子账户开通:交易 key 无提现权限 + IP 白名单、账户配置核验、模拟 → 小额实盘 → 逐步放量)。
- `python/treasury` 转 core(监控 + 划转建议)。
- ops_bot 按 `from.id` 分角色。

### WP8 文档卫生

- 修正 D 类 33 处矛盾;重复事实只留一个归属地;私有结构字节偏移改为语义描述。
- 按 R8-1 改各 README frontmatter 的 `scope`。
- 扩展 `_work/check_hft.py`:Canary/实例位残留、"第 N 周"、引擎 README 出现策略族名、依赖方向规则、`scope` 与 11 章一致性。

---

## 3. 制作方案(里程碑,按依赖顺序;不设周次)

| 里程碑 | 内容 | 在哪做 | 验收 |
|---|---|---|---|
| **M0 规格修订** | WP1–WP8 | 本地(文档) | `check_hft.py`(扩展版)全过;A、B 类问题在规格中逐条有解法与对应测试用例(testing.md);按新 cookbook 纸面推演"新增第二个策略",引擎零改动 |
| **M1 地基** | `hft-types`、`hft-event`、`hft-refdata`(含能力表)、`hft-rt`、`hft-journal`、`hft-book`、`tools/hft-cfg`、testkit 基础 | 本地 + WSL2/CI | 单元/属性/金样全绿;编码与 gen-schema 往返一致 |
| **M2 决策核心** | `hft-strategy-sdk`、`hft-sequencer`(StrategySet 泛型)、`hft-oms`、`hft-risk`、`hft-ledger`、`hft-exec`、共用撮合器、两个测试策略 | 本地 + CI | DST 种子全过;场景测试全过;**解耦验收**:新增第二个测试策略时引擎 crate 的 diff 为零 |
| **M3 交易所接入** | `hft-net`、`hft-venue-proto`、`hft-venue`、首批 2–3 家交易所(行情、下单、用户流、账户配置核验、保证金数据) | 服务器(护栏内) | 真实报文金样;fuzz;测试网冒烟(有测试网的所);服务器上连续运行无未解释断流,cdex 不受影响 |
| **M4 模拟系统 + 首个策略** | 模拟网关与模拟账户;`strategies/portfolio-carry` 基线;模拟报告 | 服务器 | 模拟账户连续运行;资金守恒、资金费与真实费率一致、拒单规则与交易所一致;报告在 ops_bot 可看;策略研究从这里开始 |
| **M5 运维与冷进程** | reconciler、kill-switch、watchtower、journal-archiver、ops_bot(schema 驱动)、treasury 告警、persist/磁盘护栏;演练 | 服务器(可与 M3–M4 并行) | kill/死手/崩溃恢复/Drain 演练通过;对账与交易所真值一致 |
| **M6 小额实盘** | 新子账户小额实盘 + 同参数模拟账户并行校准 | 服务器 | 账本与交易所一致、无裸腿、模拟与实盘差异有量化;逐步放量 |
| **M7 统一升级** | 升级到 c8(或更高)主机类,打开忙等/隔离核/大页等热路径配置 | 新主机 | 延迟验收(7 天直方图满足 00 章目标) |
| 可选 | 更多交易所、A/B 策略、cdex 迁移(chain-core 等)、metal | — | 各自触发条件 |

关键路径:M0 → M1 → M2 → M3 → M4。M5 可在 M3 之后并行。策略规则的研究在 M4 之后于模拟账户上持续进行,不阻塞框架交付。

## 4. 用户确认结果(2026-10-02,详见 round8-decisions.md R8-6)

1. 模拟系统 = 实时行情纸面交易;**另要** journal 重演排障(不用于评估收益)。
2. 建私有 GitHub 仓库;实现开始时在 `E:\trading` 之外新建目录。
3. 不用 WSL2;本地只写代码,CI 构建测试。
4. 老板只读(Viewer)。
5. 首批交易所:Binance、OKX、Bitget(现货 + USDT 永续);其余后续再加。
6. 开始 M0;改完后另开无上下文子代理独立评审。

M0 执行方式:统一简报 [round8-spec-brief.md](round8-spec-brief.md)(命名与接口唯一来源)+ 10 个按文件划分的工作包(简报 §14);修订前副本在 `E:\trading\_backup\round8-pre\`。
