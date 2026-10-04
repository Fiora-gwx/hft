# 03 · 链上层研究:BSC / Solana 的状态观测、本地定价与交易落地(2026-09-28)

> 范围:为 cdex 重写(CEX-DEX 套利,Binance USDT-M perp 对冲 + BSC/Solana 现货腿)调研"看链上状态与 pending 流、本地定价、可靠落地"三件事的最快最稳做法。先 BSC、Solana,再简述 Base/Arbitrum、Hyperliquid、Sui/Aptos。
> 数据口径:优先 2025–2026 来源;厂商自测数字标 **[厂商数据]**,推断标 **[推断]**,未能核实标 **[待核实]**。本次调研 WebFetch 被环境策略拦截,所有数字来自搜索摘要,引用页面原文时请二次核对。
> 与现状的对照:现有 Python 栈用公共 RPC `eth_call` / Jupiter quote 定价,经 `BSC_TX_RPC_URL` 发单、多节点轮询回执;两次事故(交易节点 403 → 把已上链交易当失败,裸露敞口;Solana "confirmed" 但 `meta.err != null`)在第 5 节给出根因与规则。

---

## 0. 结论摘要(Top 10)

| # | 结论 | 关键数字 |
|---|---|---|
| 1 | **BSC 已是 0.45 s 出块、~1.1 s 快速终局的链**,套利窗口按"块"算只有 2–3 个块。Fermi 硬分叉 2026-01-14 02:30 UTC 生效(BEP-619 0.45 s、BEP-590 投票规则、BEP-592 块级 access list、BEP-593 增量快照、BEP-610 EVM 超指令);官方文档给的 Fast Finality ≈ 1.125 s(2–3 块),H2 2026 路线图称 in-memory finality 已到 650 ms **[官方宣称,待核实]**。 | 0.45 s / 1.125 s / 21 个出块验证人 |
| 2 | **BSC 的区块 87–96% 由两家 builder 打包**(48Club Puissant ≈ 46.6%、BlockRazor ≈ 41.3%,2025-04→2026-02 样本;更早的 2025-05→11 样本为 >96%)。公共 mempool 还在,但对套利有价值的流基本走私有通道;**发单必须并行直投 builder**,不能只 `eth_sendRawTransaction` 到普通 RPC。 | 2 家 ≈ 87%+,MEV 利润 ≈ 90%+ |
| 3 | **BSC gas 模型:base fee 恒为 0,最低 gasPrice 0.05 gwei(2025-10-01 起),BEP-95 烧 10%**。走 builder 时 bundle 内允许 0 gwei 交易,竞价用"直接向 builder EOA 转 BNB"(48Club、Jetbldr 明确优先;gas 部分因 BEP-95 打 9 折)。 | 0.05 gwei ≈ $0.005/tx |
| 4 | **本地定价要分层**:V2/V3 用事件驱动的内存池状态(纯数学,无 EVM);最终 calldata 用 revm 对最新状态做精确模拟;PancakeSwap Infinity(V4)带 hook 的池子只能模拟、不能闭式计算(dynamic fee 依赖 beforeSwap,且可能 sender-dependent)。远端 `eth_call`(Alchemy)100 次 ≈ 4 s(≈40 ms/次),本地节点 `eth_call` ≈ 0.9 ms/次,revm 热缓存 ≈ 亚毫秒。 | 40 ms → 0.9 ms → <0.2 ms |
| 5 | **Solana 主网今天仍是 TowerBFT**(slot 400 ms,finality ≈ 12.8 s)。Alpenglow/Votor 2026-09-24 上 testnet、09-25 上 devnet;主网**没有**在 9/28 激活,下一个窥测窗口 11/9(Agave v4.4,Anza 未承诺)。Firedancer/Frankendancer 尚不支持 Votor,Frankendancer 将随 Alpenglow 退役。设计上按"今年内 finality 仍 >10 s、confirmed ≈ 0.4–1 s"做。 | 400 ms slot;150 ms finality 为目标值 |
| 6 | **Solana 的 leader 73% 在欧洲**(2026-09 快照:Europe 72.9%,Frankfurt 35.3%、Amsterdam 19.4%;NA 20.4%,Asia 10.5%)。东京→法兰克福单程 ≈ 110–130 ms,**Solana 腿在东京天然吃亏**;建议在 Frankfurt 放中继/RPC 盒子,CEX 信号仍在东京采。 | EU 73% / Asia 10.5% |
| 7 | **Solana 落地:SWQoS(质押加权 QoS)是决定性因素**;Chorus One 实测 priority fee 与 Jito tip 对延迟的影响可忽略,SWQoS 最有效。Jito bundle 用于原子性(最低 tip 1000 lamports,tip_floor API 取分位);Jito ShredStream 已于 2026-09-05 关停,shred 级数据改用 DoubleZero Edge(发布方 ≈ 43% 质押,p95 比旧 ShredStream 快 28 ms);BAM 覆盖 34% 质押并于 2026-09-09 上线 preconfirmation(比 shred 早 5–10 ms p50)。 | SWQoS 保留 ≈80% leader 带宽给质押连接 |
| 8 | **Solana 数据流成本已降**:Helius LaserStream 自 2026-04-07 起 $499/月 Business 档可用(≈$100/TB);Triton Dragon's Mouth 按量 $0.08/GB;QuickNode gRPC 自 2026-06 起含在 $499 Scale。自建非投票 RPC 节点:16C/32T、512 GB(开 account index)、双 NVMe,东京裸金属 ≈ $1,300–1,600/月。 | $499/月 vs $1.5k/月自建 |
| 9 | **CEX-DEX 套利的学术结论**:19 个月 $233.8M 由 19 个 searcher 提取,前 3 家占 75%;盈利能力与 builder 集成度强相关;交易几乎全在 top-of-block;<2% 区块空间贡献 >15% 区块价值(Ethereum,AFT 2025)。BSC 特有:2025-05 起 **builder 自己部署套利合约**直接吃单,短块间隔"压缩了可竞争窗口",利润集中在 wrapped token/稳定币的短路径。 | 前 3 家 75%;builder 自营 |
| 10 | **可靠性规则**:(a) 传输层错误(403/429/5xx/超时)只能得出 UNKNOWN,绝不能得出 FAILED;(b) 用有界有效期(bundle `maxBlockNumber`、Solana `lastValidBlockHeight`)把 UNKNOWN 变成有限时长;(c) 成功判定 = 回执 `status==1`(EVM)/ `err==null`(Solana)且 ≥2 个独立源一致;(d) 会计终局用 `finalized` 标签(BSC 滞后 2–3 块)或 `eth_getFinalizedHeader(-2)`;(e) 私有交易对公共 RPC 的 `pending` nonce 不可见,nonce 必须本地维护。 | 替换交易需 ≥10% 抬价(maxFee 与 tip 同抬) |

---

## 1. BSC(BNB Smart Chain)

### 1.1 共识与出块参数(2025–2026 四次硬分叉)

| 硬分叉 | 主网时间 | 出块 | 关键 BEP | 终局 |
|---|---|---|---|---|
| Pascal | 2025 上半年 | 3 s → 3 s(EIP 兼容性) | — | 7.5 s |
| Lorentz | 2025-04 | 1.5 s | BEP-520 | 3.75 s |
| Maxwell | 2025-06-30 | 0.75 s(实测 ≈0.8 s) | BEP-524(块间隔)、BEP-563(EVN 验证人网络)、BEP-564 | 1.875 s;TurnLength 8→16 块 |
| Fermi | 2026-01-14 02:30 UTC(客户端 v1.6.4) | **0.45 s** | BEP-619、BEP-590、BEP-592、BEP-593、BEP-610 | ≈1.125 s(2–3 块) |
| (H1 2026 增量) | — | 0.45 s | BEP-648 "Enhanced Fast Finality via In-Memory Voting Pool"(注册表标 Enabled) | 官方称 in-memory finality 650 ms **[待核实]** |

- 验证人集合:45 个活跃验证人(21 Cabinet + 24 Candidate),每 epoch 选 18 Cabinet + 3 Candidate 共 21 个出块;PoSA。Fast Finality(BEP-126)= 21 人中 ≥14 票 justify,连续两块 justify 则前一块 finalize;投票停摆时回退为概率终局。
- 路线图:2026 目标 >20k TPS,双客户端(Geth 系为稳定锚、Reth 系为高性能);更远期是"下一代 L1":<50 ms preconfirmation、<1 s 终局、**TxStream(无公共 mempool,交易直送 leader)**、PriorityLane。TxStream 目前只是新链设计目标,不是 BSC 主网 BEP。
- 来源:[Fermi 公告](https://www.bnbchain.org/en/blog/fermi-hard-fork-accelerates-bsc-to-0-45-second-block-times)、[Maxwell 公告](https://www.bnbchain.org/en/blog/bnb-chain-announces-maxwell-hardfork-bsc-moves-to-0-75-second-block-times)、[Tech Roadmap 2026](https://www.bnbchain.org/en/blog/tech-roadmap-2026)、[H2 2026 Roadmap](https://www.bnbchain.org/en/blog/bnb-chain-h2-2026-tech-roadmap-doubling-down-on-speed)、[BSC Introduction](https://docs.bnbchain.org/bnb-smart-chain/introduction/)、[BEP-126](https://github.com/bnb-chain/BEPs/blob/master/BEPs/BEP126.md)、[BEP-590 PR](https://github.com/bnb-chain/BEPs/pull/590)。

**对套利的含义**:从"看到 CEX 价格变化"到"最早可能包含"的最短距离是 1 个块(≤450 ms),但实际取决于 builder→in-turn validator 的传播和我们到 builder 的距离;从包含到 finalized 再 +2–3 块(≈1–1.4 s)。对冲时点应放在"包含证据"而非 finalized(见第 4 节)。

### 1.2 Gas 模型

- **Base fee 恒为 0**(BEP-226:BSC 早有 BEP-95 燃烧,故不烧 base fee;`maxPriorityFeePerGas == maxFeePerGas`)。
- **最低 gasPrice**:2025-05 降到 0.1 gwei,2025-10-01 全部验证人与 builder 采用 **0.05 gwei**(≈$0.005/tx)。BscScan 2026-09 显示 low/avg 0.05、fast 0.051 gwei。
- **BEP-95**:每块 gas 费的 10% 烧掉(比例可治理调整)。
- **Builder 通道下 0 gwei 交易允许**:BEP-322 后所有验证人可接受含 0 gwei 交易的块;block 层面限制 effective gas price ≥ 1。竞价规则各 builder 不同(见 1.4)。
- 来源:[BEP-226](https://github.com/bnb-chain/BEPs/blob/master/BEPs/BEP226.md)、[BNB Chain 0.05 gwei 公告](https://x.com/BNBCHAIN/status/1973334261513031828)、[BlockSec PBS 评估](https://blocksec.com/blog/performance-bsc-after-full-implementation-pbs)。

### 1.3 验证人地理与 P2P 拓扑

没有权威的"45 个验证人在哪"的数据集,只能拼间接证据:
- NodeReal 公开其验证人部署在 **AWS Ireland**(m5zn.3xlarge)。
- BlockRazor 扫描到 BSC 约 2000+ 全节点,选择 **Virginia、Frankfurt、Dublin、Tokyo** 为核心区域,理由是"块由验证人广播,物理靠近验证人最重要"→ 说明验证人/传播枢纽集中在 US-East、西欧、日本。
- 2025-12 的 bsc issue #3503:Frankfurt 节点链头最新;Singapore 5 台有 2 台落后、New York 7 台有 3 台落后,非欧洲区偶发落后 4–16 块。**[推断]** 欧洲(法兰克福/爱尔兰)是块传播最"新鲜"的区域;东京不差但非最优。
- 2025-03 的 "Validator Dedicated Network + Direct Mempool" 提案:VDN 部分作为 BEP-563 EVN 随 Maxwell 上线(只优化共识消息传播);**Direct Mempool(交易定向转发给下一批出块人)被社区(48Club、独立 searcher、Shutter)强烈反对,未上线**;替代提案 BEP-559 加密 mempool 仍在 PR。
- 来源:[NodeReal 验证人实践](https://nodereal.io/blog/en/our-best-practices-in-hosting-a-bsc-validator/)、[BlockRazor 网络基准](https://blockrazor.io/blog/20240914benchmark/)、[bsc #3503](https://github.com/bnb-chain/bsc/issues/3503)、[论坛提案](https://forum.bnbchain.org/t/call-for-feedbacks-bsc-short-block-interval-validator-dedicated-network-and-direct-mempool/3348)、[EVN 文档](https://docs.bnbchain.org/bnb-smart-chain/validator/evn/overview/)。

**建议**:用 BlockRazor 的四个核心区域作为先验,自己在 Tokyo + Frankfurt(+ Virginia 可选)各放一个探针节点,连续 1 周记录每块首次到达时间(hash 对齐),再决定主执行点。

### 1.4 MEV 供应链:builders、端点、竞价与延迟

BSC 的 PBS(BEP-322)没有 relay:builder 必须执行交易与系统合约才能生成合法块头,所以 builder 直接与验证人(sentry 服务)对接,**只能向 in-turn validator 投标,同一高度最多 3 个 bid**;验证人通过 `mev_bestBidGasFee` / `mev_params`(`builderFeeCeil`)决定接受哪个块。builder 注册信息在 [bsc-mev-info](https://github.com/bnb-chain/bsc-mev-info)。

**市场份额(arXiv 2602.15395 v2,2025-04-01→2026-02-28)**

| Builder | 区块占比 | 接入验证人数 | 备注 |
|---|---|---|---|
| 48Club (Puissant) | 46.57% | 45 | 也是验证人;自 2025-05 起自营套利合约,利润量级 ≈ $1M |
| BlockRazor | 41.26% | 45 | 自营套利利润比 48Club 低一个数量级 |
| Jetbldr | 2.57% | 40 | GWA 认证,自称 #3 |
| bloXroute | 1.86% | 44 | 同时是 BDN 网络商 |
| NodeReal | 0.99% | 36 | 早期领先,已边缘化 |
| Blocksmith (TxBoost) | 0.38% | 35 | |
| inblock / Blockbus / Flashblock / Darwin | 零星 | — | Darwin、"Blockroute" 未找到公开文档 |

**端点与竞价机制(2026-09 搜索所见,接入前以官方文档为准)**

| Builder | 发单端点 | 方法 | 竞价/费用规则 | 区域 |
|---|---|---|---|---|
| 48Club Puissant | `https://puissant-builder.48.club`;钱包 RPC `https://rpc.48.club`;0Gas `https://0.48.club`;状态 `https://explore.48.club/v2/bundle?hash=` | `eth_sendBundle`(`maxBlockNumber` 默认当前+100、`maxTimestamp`、`revertingTxHashes`、`48spSign`) | 排序分 = 非公共池交易 gas 费 ×0.9(BEP-95 折扣)+ 直接转给 Builder Control EOA 的 BNB(**直接转账优先**);bundle 平均 gasPrice 门槛(文档写 1 gwei,0.05 gwei 时代是否下调 **[待核实]**);Guest 限 50 cond txs/9 s、5 tx/bundle;持 48 Soul Point(KOGE/48erNFT/委托)提额、拿失败原因、WebSocket、`newPendingAuctions` 竞拍流(48/480 SP) | 未公开 |
| BlockRazor | 钱包 RPC `https://bsc.blockrazor.xyz`;`privacy.bsc.blockrazor.xyz`(零日志);`refund.bsc.blockrazor.xyz`(MEV 退款);builder gRPC `BundleService.SendBundle/SendTransaction` | `eth_sendMevBundle` / private tx / bundle trace | bundle 内允许 0 gwei,但非公共池交易平均 gasPrice ≥ 0.05 gwei;0 gwei 与 gRPC 需 Tier 2+ 订阅(价格未搜到);builder EOA `0x1266C6bE60392A8Ff346E8d5ECCd3E69dD9c5F20` | Virginia / Frankfurt / Dublin / **Tokyo** |
| bloXroute | Cloud-API `https://api.blxrbdn.com`;区域 WS/gRPC `virginia|uk|singapore|germany.bsc.blxrbdn.com`(**BSC 无 Tokyo 端点**,最近是 Singapore) | `bsc_private_tx`(`mev_builders: ["all"]` 或指定 48club/blockrazor/jetbldr/nodereal/bloxroute);bundle 需 Enterprise Elite 走 gateway | 加速费:向合约 `0x6374Ca2da5646C73Eb444aB99780495d61035f9b` 内部转账;2026-04 推出 **BSC Speed Boost**(私有数据中心链路,自称最多快 40%、关键路径省 ≈94 ms,亚洲↔欧美收益最大)**[厂商数据]** | 4 区域 + 私有 relay |
| Blocksmith / TxBoost | `fastbundle-us|eu|ap.blocksmith.org`(ap=JP);`https://bsc-builder.txboost.org`,模拟 `bsc-simulation.txboost.org`;钱包 `bsc-private.blocksmith.org`(免费) | `eth_sendBundle`、`eth_callBundle`、`eth_sendPrivateRawTransaction`、**`eth_cancelPrivateTransaction`**、`bsc_getRegisterValidators` | bid 转到 `0x0000000000007592b04bB3BB8985402cC37Ca224`;可选 X-Flashbots-Signature 提升优先级;支持 backrun refund 分成 | US / EU / **JP** |
| NodeReal | `https://bsc-mainnet-builder.nodereal.io` | 标准 JSON-RPC | GWA 成员;开源 [private-tx-sender](https://github.com/node-real/private-tx-sender) 可一键广播到多 builder | — |
| Jetbldr | `rpc.bsc-us|ap|eu.jetbldr.xyz` | `eth_sendBundle`(`maxBlockNumber`、`revertingTxHashes`) | **"要赢 bid 请直接转 BNB 到 bribe wallet `0xe89c42BC188c993273Ab34231e12ae60c73042E4`,不要加 gasPrice"**;拒绝 front-run bundle | US / **AP** / EU |

来源:[BSC MEV Overview](https://docs.bnbchain.org/bnb-smart-chain/validator/mev/overview/)、[Builder 集成指南](https://docs.bnbchain.org/bnb-smart-chain/validator/mev/builder-integration/)、[User Guide](https://docs.bnbchain.org/bnb-smart-chain/validator/mev/user-guide/)、[48Club Puissant Builder](https://docs.48.club/puissant-builder)、[48Club Send Bundle](https://docs.48.club/puissant-builder/send-bundle)、[48 SoulPoint Benefits](https://docs.48.club/puissant-builder/48-soulpoint-benefits)、[BlockRazor Searcher](https://docs.blockrazor.io/get-started/use-cases/searcher)、[BlockRazor bundle 规则](https://blockrazor.gitbook.io/blockrazor/transaction-submission/bundle/bsc/project-builder)、[bloXroute BSC Private Tx](https://docs.bloxroute.com/bsc-and-eth/apis/frontrunning-protection/bsc-private-transactions)、[bloXroute Cloud-API IPs](https://docs.bloxroute.com/bsc-and-eth/evm-blockchain-distribution-network-bdn/ips-and-relays/cloud-api-ips)、[BSC Speed Boost](https://bloxroute.com/pulse/introducing-bsc-speed-boost/)、[Blocksmith 文档](https://docs.blocksmith.org/)、[TxBoost RPC 文档](https://home.blocksmith.org/guide/rpc-doc.html)、[NodeReal Builder](https://docs.nodereal.io/docs/nodereal-builder)、[Jetbldr BSC API](https://jetbldr.xyz/api_bsc/)、[MEV in Binance Builder (arXiv 2602.15395)](https://arxiv.org/abs/2602.15395)。

**延迟与包含率(厂商互测,方向相反,只作量级参考)**
- bloXroute 2025-06 mempool 对比:bloXroute 68% 先看到,平均领先 ≈10 ms,44% 交易快 ≥10 ms(Frankfurt+Virginia)。
- bloXroute 2026-02 Frankfurt 块发现对比(bdnBlocks vs BlockRazor NewBlocks):约 3/4 的块 bloXroute 先到 **[厂商数据]**。
- BlockRazor 2024-09 自测:Tier1 首见交易数是 bloXroute 的 2 倍,领先 1–5 ms **[厂商数据]**。
- bloXroute 文档:`bdnBlocks` 设 `parsedTxs:false` 可再快 20–60 ms。
- 包含率没有独立公开数字。**[推断]** 由于 48Club+BlockRazor ≈ 87%,同时投这两家 + bloXroute(all)理论覆盖 >95% 的块;剩余由 Jetbldr/Blocksmith/NodeReal 补。
- 来源:[bloXroute vs BlockRazor 交易速度](https://bloxroute.com/pulse/benchmarking-blockchain-transaction-speeds-bloxroute-vs-blockrazor/)、[Frankfurt 块发现基准](https://bloxroute.com/pulse/benchmarking-bsc-block-discovery-bloxroute-vs-blockrazor-frankfurt/)、[bdnBlocks](https://docs.bloxroute.com/bsc-and-eth/streams/bdnblocks)。

**竞争格局对我们的含义 [推断]**:两大 builder 从 2025-05 起自己跑套利,并且看得到所有私有订单流;我们提交的 backrun/套利 bundle 在 builder 眼里是"信息"。可行边界是:(a) 他们不覆盖的币对/路径(长尾、V4 hook 池、跨 V2/V3/Infinity 的混合路径);(b) 由 CEX 信号驱动、不依赖链上 pending 流的"stale pool"吃单——这类机会 builder 只能在自己也接了 CEX 行情时才能竞争。给 builder 的直接转账既是费用也是"保护费",要按期望利润的比例动态出价。

### 1.5 Mempool 结构与私有订单流

- **公共 mempool 仍存在**(Chainstack 2026-08 文档仍把 BSC 列为有公共 mempool 的 EVM 链),每个节点只看到其 peer 转发的子集。
- **但套利相关流几乎全私有**:私有 RPC(48Club、BlockRazor、bloXroute、Blocksmith、NodeReal 等)→ builder 私有池;BNB Chain 官方 proxy 会把交易广播给所有注册 builder。没有公开的"私有 vs 公共交易占比"统计;区块份额 87–96% 是最接近的代理指标。
- **看 pending 流的途径**:bloXroute `newTxs/pendingTxs` 与 `internalTxsMempool`(模拟后的内部调用/日志流,BSC 独有);BlockRazor Private Mempool Stream;48Club `newPendingAuctions(WithState)`(需 48/480 SP,能看到 Puissant 收到的竞拍单)。对 CEX-DEX 策略,pending 流的价值主要是**预测下一块池子状态**(别人的大单会把池子推到哪里),优先级低于 CEX 行情本身。
- 来源:[bloXroute internalTxsMempool](https://bloxroute.com/pulse/bloxroute-launches-internal-txs-mempool-stream-on-bsc/)、[48Club Auction Feed](https://docs.48.club/puissant-builder/auction-transaction-feed)、[BNB Chain MEV FAQ](https://www.bnbchain.org/en/blog/bnb-chain-mev-faqs)。

### 1.6 自建节点 vs 付费 RPC

**硬件(2026)**

| 客户端 | CPU | RAM | 磁盘 | 同步 | 备注 |
|---|---|---|---|---|---|
| bsc-geth(官方最低) | 16 核 | 64 GB | 3 TB SSD(NVMe 推荐);archive 10 TB / 128 GB | 从 genesis 数周;用官方快照数小时;Fermi 快照重建 ≈5 h(参考机) | 稳定锚,验证人默认 |
| reth-bsc(alpha) | — | MDBX mmap,建议 `vm.min_free_kbytes ≥ 4 GB` | 文档写 ≈10.6 TB(full/archive 相近)**[待核实]** | 比 Geth 快 ≈40%;快照后小时–天;live sync <1 s | Parallel Sparse Trie、Pre-warm、Super-Instructions,目标 1 Ggas/s |
| bsc-erigon | — | ≥64 GB | fast node ≥700 GB;archive ≥5 TB(第三方称 12–16 TB) | 约 3 天 | node-real 仓库已指向 reth-bsc 迁移,**在被淘汰** |

云盘不适合(随机读延迟);AWS 用带本地 NVMe 的 i4i/i7ie 类实例,或直接裸金属。
来源:[节点最佳实践](https://docs.bnbchain.org/bnb-smart-chain/developers/node_operators/node_best_practices/)、[reth-bsc](https://github.com/bnb-chain/reth-bsc)、[BSC Reth 博文](https://www.bnbchain.org/en/blog/bsc-reth-client-the-next-evolution-of-bsc-infrastructure)、[bsc-erigon](https://github.com/node-real/bsc-erigon)。

**自建节点值不值(结论:值,但不是为了"最早看到块")**
- 付费 RPC 的公开基准都是请求响应延迟(p50 几十 ms),**没有任何公开数据测 `newHeads` 从出块到推送的滞后**;QuickNode 基准页可按 Tokyo/Singapore 过滤,CompareNodes 有 98 家 BSC 提供商的独立 p95,但都不是块到达时间。要自己测。
- 最早看到块的路径是 **BDN 块流(bloXroute bdnBlocks / BlockRazor NewBlocks)**,它们在验证人附近直连,通常先于普通 P2P 节点;自建节点的价值是 (1) **状态与日志的权威源**(eth_getLogs、storage、receipts 不受第三方限流/403 影响);(2) **本地 revm 模拟的状态后端**(0.9 ms 级 eth_call、直接读 trie);(3) **第二/第三个独立回执源**。
- BlockRazor 提供 "BSC Full Node Synchronization Integration"(把其节点流喂给你的全节点以更快同步/更早收块)——值得接入:[文档](https://docs.blockrazor.io/streams/node-stream/bsc/full-node-synchronization)。
- 调优:静态 peer 指向 builder/BDN 网关与已知快节点;`--txpool.pricelimit 50000000`;关闭不需要的 tracing;单独 NVMe 给 chaindata。
- 来源:[QuickNode 基准](https://www.quicknode.com/benchmarks)、[CompareNodes BSC](https://www.comparenodes.com/protocols/bsc/)。

### 1.7 本地定价:PancakeSwap V2 / V3 / Infinity(V4)

**分层策略**

| 层 | 用途 | 延迟 | 实现 |
|---|---|---|---|
| L0 纯数学内存池 | 机会扫描、路径搜索、初筛 | 微秒级 | V2 储备量由 `Sync/Swap` 事件维护;V3 用 `slot0 + liquidity + tick bitmap + 已初始化 tick` 走 tick 数学;定期 drift 检测与全量重同步(amms-rs 的模式:V2/V3/V4/Slipstream/Curve 支持,后台静默比对 + 定向 resync + 快照持久化) |
| L1 revm 精确模拟 | 对最终 calldata(Universal Router / 自有合约)在最新状态上算真实输出与 gasUsed | 热缓存亚毫秒;冷状态需 `eth_getCode/getStorageAt` 播种 | 状态后端接自建节点(本机 IPC);按块更新 `CacheDB`;`eth_simulateV1` 是远端替代但仍有网络 I/O |
| L2 链上 Quoter | 校验与 hook 池 | 本地节点 ≈1 ms;远端 ≈40 ms | `CLQuoter` / `BinQuoter` / `MixedQuoter`(Infinity),V3 `QuoterV2` |

实测量级(Uniswap V3 mainnet,Alloy+revm 教程):Alchemy 远端 100 次模拟 >4 s、100 次 RPC;本地全节点 eth_call 100 次 ≈90 ms;revm 直接执行 100 次 ≈19 ms(含残余播种 10 次 RPC);结果与 eth_call 逐字节一致。Anvil fork 反而比本地节点慢 30%。
来源:[REVM/Anvil/Alloy 套利模拟](https://pawelurbanek.com/revm-alloy-anvil-arbitrage)、[revm-is-all-you-need](https://github.com/solidquant/revm-is-all-you-need)、[amms-rs](https://github.com/D-lyw/amms-rs)、[clmm-swap-math](https://crates.io/crates/clmm-swap-math)、[uniswap-v3-sdk (Rust)](https://docs.rs/uniswap-v3-sdk)、[BowTiedDevil: Simulation](https://www.degencode.com/p/rewrite-it-in-rust-simulation-part-vi-simulation)。开源参考:Paradigm artemis(事件驱动 collector/strategy/executor 框架)、rusty-sando(revm 三明治机器人)、mev-template-rs——它们的共同点是"内存状态 + revm 模拟 + 多 relay 并发提交",与本文推荐一致。

**PancakeSwap Infinity 的特殊性**
- 单例 `Vault` + 多 PoolManager:`CLPoolManager`(CLAMM,x·y=k 集中流动性)与 `BinPoolManager`(LBAMM,bin 内 x+y=k)。PoolKey 的 `parameters: bytes32` 打包 tickSpacing(24 bit)或 binStep(16 bit)+ hook 权限位,与 Uniswap v4 的 `int24 tickSpacing` 不同,解码要分支。
- **Dynamic fee 由 hook 的 `beforeSwap` 决定**:官方参考 hook 用池价 EWMA 识别套利单、按价格冲击指数抬费、上限 5%——费率是"当前状态 × 本笔 swap 大小"的函数,无法闭式预估;CAKE 持仓折扣 hook 让报价 **sender-dependent**(必须以真实发送地址模拟)。
- **hook 可以对模拟说谎**:0x 对 6 条链 84,163 个 v4 hook 的分析:仅 19.4% 安全、54.2% 恶意、26.4% 疑似恶意;有 hook 检测 EVM 环境(是否 eth_call)后在结算时给出比报价低达 50% 的成交。Infinity 同模型同风险。**规则:hook 池白名单 + 用 revm 在真实区块上下文(coinbase、timestamp、gas)下以真实 sender 模拟;不白名单的 hook 池不做 taker。**
- 部分部署(如 Robinhood chain)`BinQuoter` 未部署,Bin 池报价不一定处处可用。
- 来源:[Infinity Overview](https://developer.pancakeswap.finance/contracts/infinity/overview)、[Infinity vs Uniswap v4](https://developer.pancakeswap.finance/contracts/infinity/faq/pancakeswap-infinity-vs-uniswap-v4)、[Dynamic Fee Hook](https://docs.pancakeswap.finance/trade/pancakeswap-infinity/hooks/dynamic-fee-hook)、[infinity-hooks](https://github.com/pancakeswap/infinity-hooks)、[0x: Uniswap v4 hooks were a mistake](https://webflow.internal.0x.org/post/uniswap-v4-hooks-were-a-mistake)(链接为搜索结果原样)。

---

## 2. Solana

### 2.1 共识状态(2026-09-28)

| 项 | 现状 |
|---|---|
| slot | 400 ms;leader 每 4 slot(≈1.6 s)轮换 |
| finality(主网) | TowerBFT,≈12.8 s;`confirmed`(超多数投票)通常 1–2 slot |
| Alpenglow (SIMD-0326,2025-09 以 98.94% 通过) | **Votor** 2026-09-24 18:12 UTC 在 testnet slot 444,625,255 接管、09-25 devnet slot 504,148,999;主网**未**在 9/28 激活("no Alpenrush"),Agave v4.4 主网激活窗口暂定 **11/9**,Anza 未点名 Alpenglow。目标 finality ≈150 ms(80% 质押一轮 / 60% 两轮),投票改为 off-chain 直传。**Rotor**(替代 Turbine)另起 SIMD,尚未排期 |
| Firedancer | 全量 Firedancer ≈14% 质押,Frankendancer ≈26%(2026 年中),Agave/Jito ≈60%;Q2 2026 另一口径:Jito 客户端族 54%、Harmonic 21%、Rakurai 9%、Frankendancer 8%(口径不同)。**Firedancer/Frankendancer 尚不支持 Votor;Frankendancer 将随 Alpenglow 退役**,GitHub 上有 Votor 相关 PR(unstaked 模式) |
| 区块算力上限 | 60M → **100M CU**(SIMD-0286,2026-07-29 epoch 1009);单账户写锁上限仍 12M CU(SIMD-0306 提议提到块上限的 40%) |

来源:[Alpenglow 页面](https://solana.com/upgrades/alpenglow)、[Solana Compass testnet 报道](https://solanacompass.com/news/alpenglow-is-live-on-solana-testnet-as-anza-retires-towerbft-and-votor-takes)、[CoinDesk 2026-09-23](https://www.coindesk.com/tech/2026/09/23/solana-starts-testing-upgrade-that-could-cut-finality-from-12-8-seconds-to-150-milliseconds)、["No Alpenrush"](https://bitcoinethereumnews.com/tech/solana-developers-say-no-alpenrush-as-alpenglow-mainnet-launch-rumors-collapse/)、[Frankendancer 退役](https://cryptobriefing.com/jump-firedancer-ends-frankendancer-support/)、[SIMD-0286 激活](https://solanacompass.com/news/solana-raises-mainnet-block-compute-limit-66-to-100m-cus-with-simd-0286-at)。

**设计含义**:2026 年内仍按"confirmed 可用于对冲触发、finalized 只用于会计"设计;Alpenglow 上线后 finality 150 ms 会让"finalized 后对冲"成为可能,但要等 Firedancer 跟上、观察期结束。

### 2.2 Leader 地理:欧洲主导,东京劣势

- 2026-09 初(Validators Solutions / Glassnode epoch 1030):Europe **72.9%** leader slots;Germany 36.1%(Frankfurt 35.3%);Amsterdam 19.4%;Frankfurt+Amsterdam ≈ 52–55% 出块与 53% 质押。2026-07(epoch 1006):Europe 68.5%、North America 20.4%、Asia 10.5%——集中度在**加速**。
- 托管:TeraSwitch 一家托管 ≈59% 验证人/leader slots;托管 Nakamoto 系数 ≈6。
- 原因:Frankfurt 投票时延 ≈72 ms vs 美东 ≈140 ms,收益更高,自我强化。
- 反向努力:DoubleZero Phase II(2026-03)向 São Paulo/Singapore/Hong Kong/Tokyo 各定向委托最多 60 万 SOL;DoubleZero 网络已达 59% 主网质押、166 条链路含 Tokyo。
- 来源:[Europe 73% 报道](https://cryptobriefing.com/europe-solana-leader-slots-germany-dominance/)、[Glassnode Latency Map](https://latency.glassnode.com/solana/about)、[Validators Solutions leader slots](https://validators.solutions/en/validators/leader-slots/)、[Coinbase Q2 2026 报告](https://www.coinbase.com/blog/q2-2026-solana-validator-performance-report)、[DoubleZero Phase II](https://bex.co/blog/2026/03/11/doublezero-phase-2-solana-geographic-decentralization)。

**几何 [推断]**:东京↔法兰克福公网单程 ≈110–130 ms。CEX 信号在东京产生,不论执行盒子在东京还是法兰克福,"信号→leader"的最短路径都要跨一次太平洋/欧亚(≈120 ms)。法兰克福盒子的真实收益不在发送,而在:(1) 链上状态新鲜度(shred/preconf 到达快 ~100 ms,本地池子状态更准);(2) 落地确定性(到 leader TPU/Jito 引擎 <5 ms,QUIC 握手与重试更稳);(3) 回执更早(对冲更早)。代价是回执→东京对冲再 +120 ms。净效果对"stale pool 吃单"策略为正(见第 4 节),建议分阶段验证。

### 2.3 数据流:Yellowstone gRPC、shred、preconfirmation

| 方案 | 价格(2026) | 延迟特征 | 备注 |
|---|---|---|---|
| Helius LaserStream(gRPC,Yellowstone 兼容) | $499/月 Business(2026-04-07 起,≤10 并发);流量 20 credits/MB ≈ $100/TB;Pro 加购 5 TB $400…100 TB $6,000 | 从 shred 解码,多节点聚合,9 区域,24 h 回放;"Preprocessed Transactions" 自称快 8 ms | 兼容性:多数来源称 proto 一致、Triton 客户端可直连;一份 2026-04 评测称需 Helius SDK **[待核实]** |
| Helius raw shreds(UDP) | $1,000/IP/月(Pro $800) | 自行 deshred,最早 | |
| Triton Dragon's Mouth / Riptide | $125 预充;$0.08/GB;dedicated 节点 ≈$2,900/月起 | 2026-08 Riptide 低延迟路径;与 DoubleZero/Malbec 合作从 5 地提供 raw shred 流;`SubscribeDeshred` 在执行前给出交易;~1,000 slot 去重回放 | Yellowstone 参考实现作者 |
| QuickNode Solana gRPC | 含在 $499 Scale/Business(2026-06 起);2026-10-01 起 15 credits/0.1 MB;25 条流、50 账户/流 | 3,000 slot 回放 | |
| DoubleZero Edge(shred 多播) | USDC 按 epoch 付费,有试用 | 发布方 379 验证人 ≈43% 质押(上线时);p95 比 Jito ShredStream 快 28 ms;2026-09-09 清退未授权转发方后自称领先 70%+ **[厂商数据]** | **Jito ShredStream 2026-09-05 已关停**,官方迁移路径 |
| Jito BAM Preconfirmations(经 Helius/Triton) | 收入分成模式 | 交易被 BAM 调度后、封包为 shred 前推送,p50 早 5–10 ms;覆盖 34.1% 质押(383/665 验证人) | 是"早信号",不是保证 |
| 低价 Yellowstone(Subglow $99、AllenHark $99、Shyft $199+) | — | 共享,适合备份 | |

来源:[LaserStream 文档](https://www.helius.dev/docs/laserstream)、[Helius 定价](https://www.helius.dev/pricing)、[Triton Streaming](https://triton.one/products/streaming)、[Dragon's Mouth](https://docs.triton.one/project-yellowstone/dragons-mouth-grpc-subscriptions)、[yellowstone-grpc](https://github.com/rpcpool/yellowstone-grpc)、[QuickNode gRPC 含在 Scale](https://blog.quicknode.com/solana-grpc-is-now-included-with-scale-and-business-plans/)、[Jito ShredStream 关停](https://docs.jito.wtf/lowlatencytxnfeed/)、[DoubleZero Edge](https://doublezero.xyz/journal/doublezero-introduces-edge-a-new-real-time-market-data-platform)、[ShredStream 迁移选项](https://orbitservers.io/blog/jito-shredstream-shutdown-migration)、[BAM preconfirmations](https://solanacompass.com/news/jito-bam-preconfirmations-go-live-on-solana-covering-34-of-network-stake)。

### 2.4 发送:SWQoS、Jito、直连 TPU、费用

**机制**
- **SWQoS**:leader 把 ≈80% 的交易入口带宽按质押比例保留给经质押验证人转发的连接;QUIC 每连接的并发流数/PPS 按发送方质押限额;非质押 UDP/QUIC 在拥塞时首先被丢。RPC 节点自身无质押,需与验证人签协议(Helius、Triton Cascade、Everstake、AllenHark 等出售这类通道)。
- **Chorus One 实证(2024 末)**:priority fee 对落地时延影响极小,Jito tip 同样可忽略,**SWQoS 对所有类型交易最有效**;多个 2026 来源称 SWQoS 通道首块命中率 ≈83% vs 拥塞时非 SWQoS <40% **[厂商数据]**。
- **Jito**:8 个区域 block engine(amsterdam/dublin/frankfurt/london/ny/slc/singapore/**tokyo**`.mainnet.block-engine.jito.wtf`)+ 全局 `mainnet.block-engine.jito.wtf`(geo 路由,`jito-region` 头可钉区域);bundle ≤5 笔原子;**最低 tip 1000 lamports**;tip 分位 API `https://bundles.jito.wtf/api/v1/bundles/tip_floor`(25/50/75/95/99 分位 + EMA)与 `tip_stream` WS;8 个固定 tip 账户随机选;`sendTransaction?bundleOnly=true` 得到 revert protection;默认 5 RPS。Jito 代表 >60% 的 priority fee 量。
- **Helius Sender**:7 区域含 **Tokyo**;不耗 credits,只付 tip;`Sender Max` 最低 0.001 SOL 进优先竞拍;`?swqos_only=true` 最低 0.000005 SOL(5,000 lamports)走单一 SWQoS 快路;默认 50 TPS。10 万笔/月的 tip 成本:SWQoS-only 0.5 SOL vs Max 100 SOL。
- **直连 leader TPU(QUIC)**:需实时 leader schedule + TPU 地址解析 + 连接缓存;Triton 开源 Yellowstone Jet(Rust TPU client,支持 swqos/quic、自定义路由);社区 go-solana-tpu;AllenHark Slipstream 自称 leader 在美国数据中心时 <20 ms 投递 **[厂商数据]**。
- **费用**:base 5,000 lamports/签名;priority = CU price × CU limit(按 limit 收费,非实际用量);**费用市场按写锁账户局部竞争**,单账户 12M CU 上限是真实瓶颈;`getRecentPrioritizationFees` 返回 150 slot 内该账户组"最低成交价"而非市场价(是地板不是分位),传 writable 账户而非 program id;v1 交易格式改为绝对 lamport 费用。
- 来源:[SWQoS 文档](https://solana.com/docs/defi/stake-weighted-qos)、[SWQoS 指南](https://solana.com/developers/guides/advanced/stake-weighted-qos)、[Chorus One 实证](https://chorus.one/reports-research/transaction-latency-on-solana-do-swqos-priority-fees-and-jito-tips-make-your-transactions-land-faster)、[Agave TPU](https://docs.anza.xyz/validator/tpu)、[Jito 低延迟发送](https://docs.jito.wtf/lowlatencytxnsend/)、[Jito 主网地址](https://jito-labs.gitbook.io/mev/searcher-resources/block-engine/mainnet-addresses)、[Jito JSON-RPC](https://github.com/jito-labs/mev-protos/blob/master/json_rpc/http.md)、[SLV 端点列表](https://slv.dev/en/doc/mainnet-validator/quickstart/)、[Helius Sender](https://www.helius.dev/docs/sending-transactions/sender)、[Helius Zero-Slot](https://www.helius.dev/blog/zero-slot)、[Everstake SWQoS](https://everstake.one/products/swqos)、[Yellowstone Jet](https://blog.triton.one/introducing-yellowstone-jet-tpu-client-a-high-performance-solana-tpu-client-in-rust/)、[直连 TPU 指南](https://allenhark.com/blog/how-to-submit-directly-to-leader-tpu-solana)、[Solana fees](https://solana.com/docs/core/fees)、[getRecentPrioritizationFees](https://solana.com/docs/rpc/http/getrecentprioritizationfees)、[费用估计是地板不是分位](https://xroot.dev/blog/solana-priority-fee-minimum-not-percentile)、[SIMD-0306](https://github.com/solana-foundation/solana-improvement-documents/blob/main/proposals/0306-raise-account-cu-limits.md)。

### 2.5 自建非投票 RPC 节点(Tokyo 或 Frankfurt)

- Anza 官方:RPC 节点 16C/32T 起,开全部 account index 建议 **512 GB**;两块以上 NVMe 分离 accounts 与 ledger;10 Gbps。社区生产建议 24–32 核 3.5 GHz+、384–512 GB(开 `--account-index` 到 768 GB–1 TB)。Yellowstone 插件 + 512 GB 有 2–3 天 OOM 的案例(agave #15288)。
- 成本:东京裸金属 512 GB DDR5 + 10 Gbps ≈ **$1,300–1,600/月**(CTCservers $1,502;UltaHost $1,280/月需 24 个月);全球范围 $500–1,500/月;云上出口流量另加 $900–1,200/月,不推荐。
- 亚太节点冷启动:`--known-validator` 快照源多在欧美,东京拉快照慢,需区域快照源。
- 来源:[Agave RPC 节点](https://docs.anza.xyz/operations/setup-an-rpc-node)、[Agave 硬件要求](https://docs.anza.xyz/operations/requirements)、[2026 RPC 节点构建](https://bmcservers.com/solana-rpc-node-2026)、[CTCservers Tokyo](https://www.ctcservers.com/dedicated-servers-japan/tokyo/)、[Cherry Servers Tokyo](https://www.cherryservers.com/bare-metal-dedicated-servers/tokyo)、[agave #15288](https://github.com/anza-xyz/agave/issues/15288)。

### 2.6 本地定价 vs Jupiter

- **Jupiter 自托管 Swap API(Metis binary)**:状态刷新靠 RPC 轮询(默认 200 ms)或 Yellowstone(默认 30 s 快照兜底);市场列表来自 `cache.jup.ag/markets?v=3`(每 30 分钟更新,需 `--enable-add-market` 才能不重启加新市场);已知问题:成交后报价延迟 30–100 ms 更新(#49)、gRPC 模式下价格数秒不动(#26)。QuickNode 托管版自称 p99 <0.5 s **[厂商数据]**。→ **只适合做路由发现与离线校验,不能当热路径定价器**。
- **自建 per-venue 报价器**:Raydium CLMM(`PoolState/AmmConfig/TickArrayState`,tick walk)、Raydium CPMM/AMM v4、Orca Whirlpool(tick walk)、Meteora DLMM(bin walk;一个 2026-09 项目 131/131 笔与链上 Swap 事件一致)、PumpSwap;状态由 Yellowstone 账户订阅驱动。**没有 Geyser 时忙池状态滞后 1–2 s**,这是报价误差的主因;版式变化要跟(如 Raydium CLMM 2026-08 布局第 390 字节 `fee_on`)。
- 2026 的 Jupiter 是 Meta-Aggregator(Router + JupiterZ RFQ),RFQ 路径的交易不可修改,不适合我们组合自定义指令。
- 来源:[Jupiter 自托管](https://station.jup.ag/docs/apis/self-hosted)、[issue #49](https://github.com/jup-ag/jupiter-swap-api/issues/49)、[issue #26](https://github.com/jup-ag/jupiter-swap-api/issues/26)、[Jupiter Swap 文档](https://developers.jup.ag/docs/swap)、[Raydium CLMM](https://docs.raydium.io/products/clmm)。

### 2.7 确认语义(对应我们的 "confirmed 但失败" 事故)

- `confirmationStatus`(processed < confirmed < finalized)与 `err` 是**独立字段**:交易可以 finalized 且 `err != null`——它上链、扣费、执行失败。`skipPreflight:true` 时这种情况最常见。
- 成功判定:`err === null && confirmationStatus ≥ 要求级别`;`getTransaction` 则看 `meta.err === null`。
- `getSignatureStatuses` 返回 `null` = **未知/不在缓存**,不是失败;老签名需 `searchTransactionHistory:true`;该方法不接受 commitment 参数。
- "已丢弃"的唯一正当判据:当前 block height > 交易的 `lastValidBlockHeight` 且状态仍为 null;同一交易重复发送只会执行一次(网络保证),所以在 blockhash 有效期内可安全重发。
- web3.js `confirmTransaction` 在 429/503 时有 unhandled rejection 的历史 bug——**不要把库异常当交易失败**。
- 来源:[getSignatureStatuses](https://solana.com/docs/rpc/http/getsignaturestatuses)、[confirmTransaction 迁移说明](https://solana.com/docs/rpc/deprecated/confirmtransaction)、[Helius 指南](https://www.helius.dev/docs/rpc/guides/getsignaturestatuses)、[Retry 指南](https://solana.com/developers/guides/advanced/retry)、[QuickNode 丢弃交易处理](https://www.quicknode.com/guides/solana-development/transactions/solana-transaction-propagation-handling-dropped-transactions)。

---

## 3. 其他链(简述)

| 链 | 出块/确认 | 排序机制 | 地理 | 对 CEX-DEX 的适配性 |
|---|---|---|---|---|
| **Base** | 2 s 块 + **Flashblocks 200 ms 预确认**(2025-07-16 主网,Rollup-Boost sidecar,Reth 分支);实际端到端 ≈300–500 ms;preconf 不是保证(sequencer 重启/failover 可能丢) | 每 2 s 十次子拍卖,按 priority fee 排序,封 flashblock 后不可变 → "latency auction" | Coinbase 单 sequencer,位置未公开(社区假设 us-east-1)**[待核实]** | 中:确认快、费用低;但 sequencer 在美东,东京信号单程 ≈150 ms |
| **Arbitrum One** | 250 ms 块;**Timeboost** express lane 提前 200 ms,普通交易人为延迟 200 ms;每 60 s 密封竞拍 | 2026-01→04 样本:3 个实体赢 99.74% 拍卖;≈30% 的 express lane 交易 revert(spam) | Offchain Labs 单 sequencer,位置未公开 | 低–中:express lane 被垄断,非持有者每笔慢 200 ms |
| **Hyperliquid (HyperCore/HyperEVM)** | HyperCore ≈0.2 s 一块终局、≈12 块/s;HyperEVM 快块 1 s/3M gas、慢块 1 min/30M gas | HyperBFT;CoreWriter 让 EVM 合约写订单簿 | **24 个验证人全部在 AWS ap-northeast-1(Tokyo)**,Glassnode 2026-03 测得东京交易者 ≈200 ms 优势 | **高**:与 Binance 同城,是东京机房最天然的第二腿 |
| **Sui** | Mysticeti 共识 ≈390 ms、结算 ≈640 ms(2024);Mysticeti v2(2025-11)把亚洲节点端到端从 1.00 s 降到 0.65 s | 共享对象交易走共识(DEX 池子都是共享对象) | 全球 | 低–中:DEX 为 Cetus(2025-05 被盗 $223M)、Bluefin(链下簿链上结算,≈1/4 量)、DeepBook CLOB;流动性薄,机会性 |
| **Aptos** | Baby Raptr 后终局 −100–150 ms,≈0.9 s;宣称 <50 ms 出块 | Block-STM 乐观并行,争用高时重执行 | 全球 | 低–中:Decibel 链上 CLOB(2026-02 主网,自称 ≈125 ms 结算、有状态批量改单)值得关注 |

来源:[Flashblocks Deep Dive](https://blog.base.dev/flashblocks-deep-dive)、[Chainstack Flashblocks](https://docs.chainstack.com/docs/flashblocks-on-base)、[Timeboost 介绍](https://docs.arbitrum.io/how-arbitrum-works/timeboost/gentle-introduction)、[Timeboost 实证 (arXiv 2509.22143)](https://arxiv.org/abs/2509.22143)、[Hyperliquid Tokyo 200 ms](https://www.coindesk.com/markets/2026/03/30/hyperliquid-traders-in-tokyo-get-200-millisecond-edge-glassnode-research-shows)、[HyperEVM 双块架构](https://hyperliquid.gitbook.io/hyperliquid-docs/for-developers/hyperevm/dual-block-architecture)、[Mysticeti v2](https://blog.sui.io/mysticeti-v2-sui-consensus/)、[Baby Raptr](https://aptosnetwork.com/currents/baby-raptr-lands-on-mainnet)、[Decibel](https://docs.decibel.trade/quickstart/overview)。

---

## 4. CEX-DEX 套利的延迟结构与执行顺序

### 4.1 研究结论

- **Wu / Sui(Flashbots)/ Thiery(EF)/ Pai,AFT 2025**:2023-08→2025-03 共 7,203,560 笔 CEX-DEX 套利、$233.8M 提取价值、19 个主要 searcher,前 3 家占 75% 量与价值;盈利与 builder 集成度绑定,存在独占的 searcher–builder 关系;beaverbuild/Titan/rsync 三家 builder 中两家垂直整合自营 searcher;这类交易占 <2% 区块空间却贡献 >15% 区块价值。CEX 腿不可见,作者用"信息优势最大化并开始衰减的时刻"反推对冲时点。[arXiv 2507.13023](https://arxiv.org/abs/2507.13023) / [AFT 2025](https://drops.dagstuhl.de/entities/document/10.4230/LIPIcs.AFT.2025.26)
- **Frontier Research《A Tale of Two Arbitrages》(Jump/Ellipsis/Frontier,2023)**:两条腿——先在链上把池子价打回公允值,成功后在链下对冲累积库存;两大风险 = **库存风险**(第一腿到对冲之间)与 **包含风险**(多人竞争同一机会);2023 Q1 60% 的套利机会被 CeFi-DeFi 策略拿走;高流动性币 EV_signal(谁先知道价格)占主导,低流动性币 EV_ordering(谁排前面)占主导。[frontier.tech](https://frontier.tech/a-tale-of-two-arbitrages)
- **EF ethresear.ch 早期实证**:CEX-DEX 套利几乎全部在 top-of-block;单一 searcher 0xa69 占 55.7%。
- **BSC 特有(arXiv 2602.15395)**:2025-05 起 builder 自营套利、直接吃掉原属 searcher 的机会;短块间隔 + 白名单 PBS "压缩了可竞争窗口、放大延迟优势、排除慢 builder 与 searcher";利润集中于 wrapped token 与稳定币的短路径(低 hop)。
- **2026 高吞吐链竞争研究**:《To Wait or To Probe》([arXiv 2606.00720](https://arxiv.org/abs/2606.00720))与《Blockspace under pressure: spam MEV》([arXiv 2604.00234](https://arxiv.org/abs/2604.00234))讨论在 Solana/BSC 这类快链上,套利者用"盲发探测(probe)"替代"等待确认再发",导致大量 revert 交易占用区块空间;《Second Thoughts》([arXiv 2601.00738](https://arxiv.org/abs/2601.00738))分析 1 s 子槽对 Ethereum CEX-DEX 套利的影响;WWW 2026 有用 HMM 建模 CEX-DEX 机会的论文([ACM](https://dl.acm.org/doi/10.1145/3774904.3792185))。**[推断]** 对我们:在 BSC 上"每块都探一发、失败 revert"是竞争者的常态,revert 保护(bundle `revertingTxHashes` 为空 = 失败不上链不付费,或 builder 侧模拟失败即丢)是成本控制的关键。
- 跨链套利(arXiv 2501.17335):两种模式——多链持仓(近即时执行、占资金、承担价格风险)vs 桥接(无库存风险、有延迟);我们是前者。

### 4.2 边在哪里:四个环节的延迟预算(东京执行 BSC 腿)

| 环节 | 典型延迟 | 说明 |
|---|---|---|
| (i) 看到 CEX 价格变化 | <1–2 ms(Binance 撮合在 AWS ap-northeast-1,同区 WS)**[业界普遍认知,待核实]** | 这是我们相对多数链上机器人的**唯一结构性优势**;链上机器人靠 pending 流与池子价格,天然晚我们几百 ms |
| (ii) 本地模拟与决策 | L0 内存池 <50 µs;L1 revm 0.1–1 ms;远端 eth_call 40 ms+ | 现有 Python + 公共 RPC eth_call 在这一步就丢掉 1/10 个块 |
| (iii) 发送与包含 | 到 builder(Tokyo/AP 端点)1–5 ms;builder→in-turn validator 未知(EU/US 时 100–200 ms);下一块 ≤450 ms | 无法控制的部分是 validator 位置;能控制的是并行投 ≥3 家 builder、bundle 有效期只给 2 块 |
| (iv) 对冲 | 看到包含证据(BDN 块流 +50–150 ms)后下 CEX 单 <2 ms | 库存暴露时长 ≈ 0.5–1.2 s。以 BTC 年化 50% 波动率估,1 s 的 1σ ≈ 0.9 bp,远小于典型 5–20 bp 的净边 → **库存风险可接受,包含风险与"失败误判"才是主要风险** |

### 4.3 执行顺序:chain-first 为主,hedge-first 只在高确定性时用

- **DEX 只做 taker,且只在池子相对 CEX "过时"时做**:触发条件 = |pool_mid − cex_mid| > 池费 + gas + builder 出价 + 预期冲击 + 竞争折价;用 L0 状态判断,用 L1 revm 定最终 amountOut 与 minOut。
- **chain-first(默认)**:发 bundle(有效期 current+2 块)→ 收到包含证据(块流里看到自己交易 / builder bundle 状态 / 自建节点回执)→ 立即 CEX 对冲。失败(有效期过、未包含)= 没有仓位,零成本重来。
- **hedge-first(可选)**:仅当 (a) 机会来自 CEX 大幅跳价、池子肯定滞后;(b) 我们的 bundle 投给了 in-turn validator 对应的主 builder 且历史包含率 >90%;(c) 对冲腿可在 1 块内低成本撤回(限价单/撤单)。否则 hedge-first 的失败模式是"CEX 有仓、链上没成交",与我们 9/24 事故对称。
- **Solana 腿**:同样 chain-first;用 Jito bundle(tip 取 tip_floor 75–95 分位随机化)+ SWQoS 通道双路发;`processed` 级别看到含 `err==null` 的交易即可对冲(≈400 ms),`confirmed` 复核,`finalized` 记账。
- **Hyperliquid**:东京同城,可作为第二对冲场所或第二腿,是低延迟栈的自然延伸。

---

## 5. 交易生命周期可靠性(对应两次事故)

### 5.1 状态机与"绝不把传输错误当失败"

```
SENT ──(回执 status==1 / meta.err==null,≥2 源)──> INCLUDED_OK
  │ ──(回执 status==0 / meta.err!=null)──────────> INCLUDED_REVERTED
  │ ──(有效期过 + nonce/blockhash 证据 + ≥2 源 null)──> DROPPED
  └──(403/429/5xx/超时/连接断)──────────────────> UNKNOWN(继续问其他源;不触发任何对冲反向操作)
```
- **403 事故根因**:把 `BSC_TX_RPC_URL` 的 HTTP 403 映射成"交易失败"。规则:HTTP/传输层错误与链上结果正交;只有链上证据能改变状态。UNKNOWN 超过 T(BSC 建议 3 块 ≈1.4 s)仍无法判定时,不做"假设未成交"的反向操作,而是继续用自建节点 + ≥2 家独立 RPC 查 `eth_getTransactionReceipt` / `eth_getTransactionByHash` / 该 nonce 是否被消费,直到 DROPPED 或 INCLUDED。
- **让 UNKNOWN 有界**:BSC 用 bundle `maxBlockNumber = current + 2`(48Club/Jetbldr/BlockRazor 均支持),过期即可确认"不会再上链"(需自建节点 finalized 高度 > maxBlockNumber);Solana 用 `lastValidBlockHeight`。裸 `eth_sendRawTransaction` 的私有交易**没有有效期**,是 UNKNOWN 无限延长的来源,尽量不用;若用,配 TxBoost `eth_cancelPrivateTransaction` 或 48Club 同 nonce +10% 覆盖。
- **回执多源**:自建节点(权威)+ 2 家不同运营商 RPC(NodeReal MegaNode、QuickNode/Chainstack 等)+ builder 的 bundle 状态接口(48Club `explore.48.club/v2/bundle`、Jito `getBundleStatuses`);任一源报 INCLUDED 且 hash/blockNumber 一致即触发对冲,记账等 finalized。

### 5.2 Nonce 管理(BSC)
- geth 系 txpool:`PriceBump=10`,替换需 **gasPrice(及 1559 的 maxFee 与 tip 同时)≥ +10%**;`pending` 不允许 nonce 空洞,`queued` 允许但每账户 64 笔后开始淘汰;bsc #1409 报告过高频发送时"replacement transaction underpriced"卡池。
- **私有交易对公共节点不可见**:`eth_getTransactionCount(addr,"pending")` 从别的节点取会漏掉在途私有交易 → nonce 复用碰撞。必须**本地维护 nonce 分配器**(每账户单调、带在途表),只在 DROPPED/INCLUDED 后回收或推进;启动时用自建节点 `latest` 校准。
- 多账户轮转:每个执行账户同时最多 1 笔在途,避免同账户 bundle 之间的 nonce 依赖;需要更高并发就加账户。
- 来源:[geth txpool](https://geth.ethereum.org/docs/interacting-with-geth/rpc/ns-txpool)、[bsc #1409](https://github.com/bnb-chain/bsc/issues/1409)、[Chainstack nonce 管理](https://chainstack.com/ethereum-nonce-management/)、[Base 替换规则](https://docs.base.org/base-chain/network-information/troubleshooting-transactions)。

### 5.3 模拟、gas 估计与失败判定
- 发送前用 revm 在 `latest` 上模拟完整 bundle(builder 也会模拟,失败即丢);gasLimit = 模拟 gasUsed × 1.2–1.3;BSC base fee 0,成本 = gasPrice × gasUsed + 直接转账给 builder 的 BNB。
- 成功 = 回执 `status == 0x1`;还要校验目标事件(Swap 日志、余额差)以防合约"成功但没换到"的情况——现有栈已加余额核对,保留。
- Solana:`meta.err == null` 且内部指令/余额变化符合预期;`processed` 触发、`confirmed` 复核。

### 5.4 Reorg 与终局
- BSC:`finalized` 标签滞后 2–3 块(≈1–1.4 s);`eth_getFinalizedHeader(verifiedValidatorNum=-2|-3)` 返回 max(快速终局, 概率终局) 且单调递增,历史上 -1/-2/-3 返回的块从未回滚;`eth_newFinalizedHeaderFilter` 可推送。要有"finalized 不再前进而 latest 仍在涨"的停摆检测并回退到概率深度。对冲在 `latest` 包含证据上触发(reorg 概率极低),会计在 `finalized`。
- Solana:reorg 在 `confirmed` 之后极罕见;`finalized` ≈12.8 s(Alpenglow 后 ≈150 ms)。
- 来源:[BSC API 列表](https://docs.bnbchain.org/bnb-smart-chain/developers/json_rpc/bsc-api-list/)、[BEP-126](https://github.com/bnb-chain/BEPs/blob/master/BEPs/BEP126.md)。

---

## 6. 推荐方案

### 6.1 BSC(优先级最高,与 CEX 同城)
1. **执行盒子放东京**(AWS ap-northeast-1 带本地 NVMe 的实例或裸金属),Rust 引擎;同时在 **Frankfurt** 放一台探针/备用节点,连续测每块首达时间与 bundle 包含率,1 个月后决定是否把发送点迁到欧洲。
2. **自建 BSC 全节点 ×1(东京)**:先 bsc-geth v1.6.x(稳定),并行试 reth-bsc alpha;接 BlockRazor Full Node Sync 流与静态 peer;作为状态/日志/回执权威源与 revm 后端。硬件 16C+/64–128 GB/2×NVMe 4 TB。
3. **块与 pending 流**:BlockRazor(Tokyo 区域,Tier 2 拿 0 gwei + gRPC)+ bloXroute Enterprise(Singapore 区域,`bdnBlocks parsedTxs:false`、`newTxs`、Speed Boost 评估)。两路对齐 hash 取先到。
4. **本地定价**:L0 内存池状态(V2/V3 数学,事件驱动 + drift 检测)→ L1 revm 精确模拟 Universal Router / 自有合约 calldata → Infinity hook 池白名单制、只用 revm 真实 sender 模拟。淘汰热路径上的远端 `eth_call`。
5. **发送**:并行投 48Club `puissant-builder`、BlockRazor、bloXroute(`mev_builders:["all"]`)、Blocksmith AP、Jetbldr AP;bundle `maxBlockNumber=current+2`、`revertingTxHashes=[]`;费用用"直接转 BNB 给 builder EOA"按预期利润比例出价;bundle 内交易 0 gwei 或 0.05 gwei 满足各家平均价规则。
6. **可靠性**:第 5 节状态机;nonce 本地分配;回执三源;`finalized` 记账;所有 builder/RPC 的 HTTP 错误只进 UNKNOWN。
7. 预算(月):BSC 节点 $0.5–1.5k;bloXroute Enterprise 级 ≈$1–5k(2026-03 改为模块化计价,以 portal 报价为准);BlockRazor Tier 2(未公开);48Club Soul Point(持 KOGE/NFT 或委托,非现金订阅)。

### 6.2 Solana(第二阶段)
1. **Phase 1(东京,快速上线)**:Helius LaserStream(Business $499)+ Helius Sender Tokyo(SWQoS-only 5,000 lamports 为主,Max 0.001 SOL 用于高价值机会)+ Jito Tokyo block engine(bundle 原子性,tip 取 tip_floor 分位)+ 自建 per-venue 报价器(Raydium CLMM/CPMM、Orca、Meteora DLMM、PumpSwap)。Jupiter 自托管只做路由发现与离线校验。
2. **Phase 2(Frankfurt 中继)**:法兰克福裸金属 RPC 节点(24–32C、768 GB–1 TB、双企业级 NVMe、10 Gbps,≈$1.5–2.5k/月)+ DoubleZero Edge shred 订阅 + BAM preconf(经 Helius/Triton)+ 直连 leader TPU(Yellowstone Jet,配 SWQoS 通道)+ Jito Frankfurt/Amsterdam。CEX 信号仍在东京采集,以紧凑消息推到法兰克福盒子,由盒子在最新状态上复核后发送并就地对冲触发回传。
3. **确认**:`processed` 且 `err==null` 触发对冲;`confirmed` 复核;`lastValidBlockHeight` 判丢弃;`null` 状态视为未知。
4. **跟踪**:Alpenglow 主网时间(11/9 窗口)、Firedancer Votor 支持、SIMD-0306 单账户 CU 上限——三者都会改变落地与终局假设。

### 6.3 其他链
- **Hyperliquid** 优先于 Base/Arbitrum:同在东京 AWS,作为第二对冲场所/第二腿成本最低。
- Base(Flashblocks)可作后续扩展,先在 us-east 放探针测 sequencer 距离;Arbitrum Timeboost 不建议(express lane 被 3 家垄断)。
- Sui/Aptos 观察 Decibel、DeepBook 的流动性,暂不投入。

### 6.4 未决问题(需要实测)
- 48Club 与 BlockRazor builder 节点的物理位置;in-turn validator 位置分布 → 决定东京 vs 法兰克福发送点。
- 各 builder 对我们的真实包含率与"被 builder 自营套利抢跑"的比例(用相同机会在不同 builder A/B)。
- 48Club bundle 平均 gasPrice 门槛在 0.05 gwei 时代的实际值。
- reth-bsc alpha 在 0.45 s 出块下的稳定性;Solana 512 GB 节点 + Yellowstone 的内存余量。

---

## 7. 参考链接(按主题)

**BSC 协议与路线图**
- https://www.bnbchain.org/en/blog/fermi-hard-fork-accelerates-bsc-to-0-45-second-block-times
- https://www.bnbchain.org/en/blog/bnb-chain-announces-maxwell-hardfork-bsc-moves-to-0-75-second-block-times
- https://www.bnbchain.org/en/blog/tech-roadmap-2026
- https://www.bnbchain.org/en/blog/bnb-chain-h2-2026-tech-roadmap-doubling-down-on-speed
- https://docs.bnbchain.org/bnb-smart-chain/introduction/
- https://github.com/bnb-chain/BEPs/blob/master/BEPs/BEP126.md · https://github.com/bnb-chain/BEPs/blob/master/BEPs/BEP226.md · https://github.com/bnb-chain/BEPs/blob/master/BEPs/BEP322.md · https://github.com/bnb-chain/BEPs/pull/590
- https://docs.bnbchain.org/bnb-smart-chain/validator/evn/overview/
- https://forum.bnbchain.org/t/call-for-feedbacks-bsc-short-block-interval-validator-dedicated-network-and-direct-mempool/3348
- https://x.com/BNBCHAIN/status/1973334261513031828

**BSC MEV / builders**
- https://docs.bnbchain.org/bnb-smart-chain/validator/mev/overview/ · /user-guide/ · /builder-integration/ · /validator-integration/
- https://github.com/bnb-chain/bsc-mev-info · https://github.com/bnb-chain/bsc-builder
- https://arxiv.org/abs/2602.15395 (MEV in Binance Builder)
- https://blocksec.com/blog/performance-bsc-after-full-implementation-pbs
- https://docs.48.club/puissant-builder · https://docs.48.club/puissant-builder/send-bundle · https://docs.48.club/puissant-builder/48-soulpoint-benefits · https://docs.48.club/puissant-builder/auction-transaction-feed · https://docs.48.club/privacy-rpc
- https://docs.blockrazor.io/ · https://docs.blockrazor.io/get-started/use-cases/searcher · https://blockrazor.gitbook.io/blockrazor/transaction-submission/bundle/bsc/project-builder · https://docs.blockrazor.io/streams/node-stream/bsc/full-node-synchronization · https://blockrazor.io/blog/20240914benchmark/
- https://docs.bloxroute.com/bsc-and-eth/apis/frontrunning-protection/bsc-private-transactions · https://docs.bloxroute.com/bsc-and-eth/evm-blockchain-distribution-network-bdn/ips-and-relays/cloud-api-ips · https://docs.bloxroute.com/bsc-and-eth/streams/bdnblocks · https://bloxroute.com/pulse/introducing-bsc-speed-boost/ · https://bloxroute.com/pulse/benchmarking-bsc-block-discovery-bloxroute-vs-blockrazor-frankfurt/ · https://bloxroute.com/pulse/benchmarking-blockchain-transaction-speeds-bloxroute-vs-blockrazor/ · https://bloxroute.com/pulse/built-to-scale-bloxroutes-new-pricing-model/ · https://bloxroute.com/pulse/bloxroute-launches-internal-txs-mempool-stream-on-bsc/
- https://docs.blocksmith.org/ · https://docs.blocksmith.org/bsc-builder/private-rpc · https://home.blocksmith.org/guide/rpc-doc.html
- https://docs.nodereal.io/docs/nodereal-builder · https://github.com/node-real/private-tx-sender
- https://jetbldr.xyz/api_bsc/

**BSC 节点与 RPC**
- https://docs.bnbchain.org/bnb-smart-chain/developers/node_operators/node_best_practices/
- https://github.com/bnb-chain/reth-bsc · https://www.bnbchain.org/en/blog/bsc-reth-client-the-next-evolution-of-bsc-infrastructure · https://github.com/node-real/bsc-erigon
- https://github.com/bnb-chain/bsc/issues/3503 · https://nodereal.io/blog/en/our-best-practices-in-hosting-a-bsc-validator/
- https://docs.bnbchain.org/bnb-smart-chain/developers/json_rpc/bsc-api-list/
- https://www.quicknode.com/benchmarks · https://www.comparenodes.com/protocols/bsc/

**本地定价 / 模拟**
- https://pawelurbanek.com/revm-alloy-anvil-arbitrage · https://github.com/solidquant/revm-is-all-you-need · https://www.degencode.com/p/rewrite-it-in-rust-simulation-part-vi-simulation
- https://github.com/D-lyw/amms-rs · https://crates.io/crates/clmm-swap-math · https://docs.rs/uniswap-v3-sdk
- https://developer.pancakeswap.finance/contracts/infinity/overview · https://developer.pancakeswap.finance/contracts/infinity/faq/pancakeswap-infinity-vs-uniswap-v4 · https://docs.pancakeswap.finance/trade/pancakeswap-infinity/hooks/dynamic-fee-hook · https://github.com/pancakeswap/infinity-hooks
- https://webflow.internal.0x.org/post/uniswap-v4-hooks-were-a-mistake

**Solana 共识与地理**
- https://solana.com/upgrades/alpenglow · https://solanacompass.com/news/alpenglow-is-live-on-solana-testnet-as-anza-retires-towerbft-and-votor-takes · https://www.coindesk.com/tech/2026/09/23/solana-starts-testing-upgrade-that-could-cut-finality-from-12-8-seconds-to-150-milliseconds · https://bitcoinethereumnews.com/tech/solana-developers-say-no-alpenrush-as-alpenglow-mainnet-launch-rumors-collapse/ · https://cryptobriefing.com/jump-firedancer-ends-frankendancer-support/
- https://cryptobriefing.com/europe-solana-leader-slots-germany-dominance/ · https://latency.glassnode.com/solana/about · https://validators.solutions/en/validators/leader-slots/ · https://www.coinbase.com/blog/q2-2026-solana-validator-performance-report · https://bex.co/blog/2026/03/11/doublezero-phase-2-solana-geographic-decentralization
- https://solanacompass.com/news/solana-raises-mainnet-block-compute-limit-66-to-100m-cus-with-simd-0286-at · https://github.com/solana-foundation/solana-improvement-documents/blob/main/proposals/0306-raise-account-cu-limits.md

**Solana 数据与发送**
- https://www.helius.dev/docs/laserstream · https://www.helius.dev/pricing · https://www.helius.dev/docs/sending-transactions/sender · https://www.helius.dev/blog/zero-slot · https://www.helius.dev/blog/laserstream-websockets
- https://triton.one/products/streaming · https://docs.triton.one/project-yellowstone/dragons-mouth-grpc-subscriptions · https://github.com/rpcpool/yellowstone-grpc · https://blog.triton.one/introducing-yellowstone-jet-tpu-client-a-high-performance-solana-tpu-client-in-rust/
- https://www.quicknode.com/solana-yellowstone-grpc · https://blog.quicknode.com/solana-grpc-is-now-included-with-scale-and-business-plans/
- https://docs.jito.wtf/lowlatencytxnsend/ · https://docs.jito.wtf/lowlatencytxnfeed/ · https://jito-labs.gitbook.io/mev/searcher-resources/block-engine/mainnet-addresses · https://github.com/jito-labs/mev-protos/blob/master/json_rpc/http.md · https://slv.dev/en/doc/mainnet-validator/quickstart/ · https://bam.dev/ · https://solanacompass.com/news/jito-bam-preconfirmations-go-live-on-solana-covering-34-of-network-stake
- https://doublezero.xyz/journal/doublezero-introduces-edge-a-new-real-time-market-data-platform · https://orbitservers.io/blog/jito-shredstream-shutdown-migration
- https://solana.com/docs/defi/stake-weighted-qos · https://solana.com/developers/guides/advanced/stake-weighted-qos · https://chorus.one/reports-research/transaction-latency-on-solana-do-swqos-priority-fees-and-jito-tips-make-your-transactions-land-faster · https://everstake.one/products/swqos · https://docs.anza.xyz/validator/tpu · https://allenhark.com/blog/how-to-submit-directly-to-leader-tpu-solana
- https://solana.com/docs/core/fees · https://solana.com/docs/rpc/http/getrecentprioritizationfees · https://xroot.dev/blog/solana-priority-fee-minimum-not-percentile

**Solana 节点 / Jupiter / 确认语义**
- https://docs.anza.xyz/operations/setup-an-rpc-node · https://docs.anza.xyz/operations/requirements · https://bmcservers.com/solana-rpc-node-2026 · https://www.ctcservers.com/dedicated-servers-japan/tokyo/ · https://www.cherryservers.com/bare-metal-dedicated-servers/tokyo · https://github.com/anza-xyz/agave/issues/15288
- https://station.jup.ag/docs/apis/self-hosted · https://github.com/jup-ag/jupiter-swap-api/issues/49 · https://github.com/jup-ag/jupiter-swap-api/issues/26 · https://developers.jup.ag/docs/swap · https://docs.raydium.io/products/clmm
- https://solana.com/docs/rpc/http/getsignaturestatuses · https://solana.com/docs/rpc/deprecated/confirmtransaction · https://www.helius.dev/docs/rpc/guides/getsignaturestatuses · https://solana.com/developers/guides/advanced/retry · https://www.quicknode.com/guides/solana-development/transactions/solana-transaction-propagation-handling-dropped-transactions

**其他链**
- https://blog.base.dev/flashblocks-deep-dive · https://docs.chainstack.com/docs/flashblocks-on-base
- https://docs.arbitrum.io/how-arbitrum-works/timeboost/gentle-introduction · https://arxiv.org/abs/2509.22143
- https://www.coindesk.com/markets/2026/03/30/hyperliquid-traders-in-tokyo-get-200-millisecond-edge-glassnode-research-shows · https://hyperliquid.gitbook.io/hyperliquid-docs/for-developers/hyperevm/dual-block-architecture
- https://blog.sui.io/mysticeti-v2-sui-consensus/ · https://aptosnetwork.com/currents/baby-raptr-lands-on-mainnet · https://docs.decibel.trade/quickstart/overview

**CEX-DEX 套利研究**
- https://arxiv.org/abs/2507.13023 · https://drops.dagstuhl.de/entities/document/10.4230/LIPIcs.AFT.2025.26
- https://frontier.tech/a-tale-of-two-arbitrages
- https://arxiv.org/abs/2601.00738 · https://arxiv.org/abs/2606.00720 · https://arxiv.org/abs/2604.00234 · https://dl.acm.org/doi/10.1145/3774904.3792185 · https://arxiv.org/abs/2501.17335

**交易生命周期**
- https://geth.ethereum.org/docs/interacting-with-geth/rpc/ns-txpool · https://github.com/bnb-chain/bsc/issues/1409 · https://chainstack.com/ethereum-nonce-management/ · https://docs.base.org/base-chain/network-information/troubleshooting-transactions
