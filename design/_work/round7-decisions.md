# 第七轮裁决:去掉阶段与过渡,cdex 免费版直接做在新框架上(2026-10-01,用户决定)

用户原话:"不需要写成阶段一二三什么的,目前我们仍然跑在这个简单的项目上,那个重构的高性能的做好测试完就直接迁移了,不用考虑什么过渡的情况,你直接在上面做对应的 cdex 免费版就行了。"

## R7-1 交付模型:建成 → 测试 → 一次性迁移

- 现有系统(Python cdex + collector + 现有 Telegram bot)照常运行,不做任何改动,不为新系统做适配。
- 新系统(`hft/`)完整建成并通过测试后,**一次性迁移**:旧引擎停止开仓并收尾 → 新系统启动接管同一批账户。**没有新旧并行交易期、没有共享账户、没有过渡接口、没有金丝雀实例**。
- 测试包括:单元/属性/金样/fuzz/DST、交易所测试网、在现有服务器上用真实行情以 Shadow 模式长时间运行(只记录不下单)、小额实盘验证(迁移前在同一账户上旧引擎暂停时进行,或用独立小额账户)。Shadow 是新系统自身的运行模式,保留。
- collector 保持独立服务研究(第四轮 R4-1 不变);它不是过渡件。

## R7-2 去掉"阶段"

- 规格与设计文档里不再出现"阶段 0/1/1'/2/3/4"的组织方式。README frontmatter 的 `phase` 字段删除,改为 `scope: core | optional`:
  - `core` = 迁移时必须具备、必须通过测试的组件;
  - `optional` = 可选扩展,不在迁移范围内,文档写明"何时值得做"(触发条件),不写周次。
- 正文中"阶段 N 做/阶段 N 推迟"一律改写为两种之一:**直接写成当前设计**(core),或**"可选扩展:……(触发条件)"**(optional)。不要留下"后续阶段""阶段 2 起"之类的措辞。
- 主机类(dev2 / c4 / c8 / metal)是部署配置,不是阶段,保留;metal 为 optional。

## R7-3 迁移范围(core)

- CEX:Binance USDT-M、Gate(合约与现货按现有策略所需)。
- 链上:BSC,经 **chain-core free profile**(公共 RPC、同一签名交易并发提交、≥ 2 独立运营商回执、本地 nonce 槽候选集合、SwapGuard maxBlock、gas 上限)。chain-core 为 cold tier(允许 tokio 与成熟 RPC 客户端),与 hft-core 经外部 venue 协议(同机共享内存环)连接——这是 core。
- 策略:C(Carry,含 CEX-CEX 与 CEX-DEX 携带)、D(cdex chain-first)、E(股票代币,D 的参数集)、A(CrossTaker,以 Shadow 模式运行直到运营决定打开——这是运行配置,不是阶段)。
- 慢路径:`python/pipeline`(watchlist、pool_finder、五道门、成本线、sizing 因子 → StrategyParams)是 core;`hft-cfg` 负责静态配置与 refdata/EngineConfig/RiskParams;StrategyParams 由 pipeline 产出,人工经 `/pl set` 策略文件或 CLI 确认下发。
- 运维:`ops_bot` 在迁移时覆盖现有 Telegram 工作流(老板常用的 `/p /b /pl /account /engine /hedgefix /pipeline` 语义),reconciler 覆盖两所与 BSC 钱包,kill-switch,watchtower,journal-archiver,replayer verify,hft-py(reader + ledger)。
- 测试基建:hft-testkit(金样、属性、最小确定性撮合器、DST、假交易所)+ 交易所测试网。
- **optional**:其余 CEX(OKX、Bybit、Bitget、KuCoin)、Hyperliquid、Binance Spot SBE、策略 B(MakerTaker);builder profile、块流、自建 BSC 节点、L0 池镜像 + revm、hft-amm(若 free profile 用 eth_call 定价则 optional)、chain-grpc-relay;Solana;metal / x86 剖面、Onload;完整 hft-sim 与假设回放;wire 级 mock-exchange;RSS 源端口预计算、TX 时间戳、TLS 预热、L2 双路合并;treasury;research 工具;advisor 的 LeadLag 以外的慢计算可按需。

## R7-4 删除的过渡机制

- `python/cdex_hardening`(对旧 Python cdex 的加固)整个删除;相关 Agent Note `cdex-hardening-in-python-first` 移到 `rejected/`(Status: rejected — 用户决定 cdex 免费版直接做在新框架上),其余引用删除。cdex 免费版的规则全部在 `hft-chain-evm`/`apps/chain-core`/`hft-strategies`(D)里,即原来写给 chain-core 的那套。
- external-venue-protocol 的临时接口(§9 及其保留帧号)删除。
- 金丝雀/第二实例全部删除(ClientOrderId 的实例位保留为"恒 0 的保留位",不再有任何逻辑:`OTHER_INSTANCE`、`canary.*` 配置、实例交接等删除)。新版本验证 = 回放 verify + Shadow 运行。
- 共享账户与所有权机制删除:ownership 表、`NOT_OWNED`、`[[accounts]].exclusive`、`foreign_insts`、kill-switch 只撤"本系统"单的过滤与 `is_ours`(kill-switch 在本系统停摆时撤该账户全部挂单)。保留一句规则:"同一交易所账户同一时刻只由一个系统交易;迁移是切换,不并行。"
- 旧系统控制接口删除:`/kill all` 中停旧引擎的一项、`legacy.*` 配置、`engine.ctl.json` 的 `chain_pause/chain_resume`、ops_bot 并行期规则(`/engine*` 拒绝并提示旧 bot)。`/kill all` = hft-core `Kill{Global}` + chain-core `Pause{all}`。
- `docs/runbooks/cutover.md` 改为 `migration.md`:迁移前检查(测试与 Shadow 结果、账户与资金、配置 blob)、迁移步骤(旧引擎停止开仓 → 平仓或确认剩余仓位 → 停旧引擎 → 新系统启动并对账,剩余仓位用 `AdoptExchange` 入账 → 解除 Recovery → 逐策略 SetMode Live)、回滚(新系统 Drain → 重新启动旧引擎)。
- "与旧 Python 引擎意图一致 > 99.9%"之类的并行对比验收改为:Shadow 运行期间新系统的信号与同期旧引擎的信号做**离线**对照(读双方日志),作为测试项之一。

## R7-5 文档

- 设计书 `architecture-v2/` 00–12 章中"阶段一/阶段二"指 AWS 内核栈 vs 裸金属 Onload 的,改为"基准形态"与"可选:裸金属 + Onload";路线图(11 章)重写为"交付与迁移计划"(按依赖的构建顺序、测试门槛、迁移步骤,不分阶段);13 章追加本轮记录。
- 老板看的分析文档与待决定事项去掉阶段措辞,cdex 免费版改为新框架内实现;E10(现有 cdex 的在线风险)保留为待决定事项,因为现有系统照常运行到迁移。
