# 02 · 交易所接入层研究:最低延迟的行情与下单通道(2026-09-29)

> 版本 v1.0(2026-09-29 02:10 UTC 定稿)。研究方法:官方文档/公告(经东京 EC2 中转抓取,okx.com / bybit / binance / bitget / gate / kucoin 从开发机直连均被墙或 WAF 拦截)、GitHub 官方文档仓库、五个并行调研子代理的报告,以及 **2026-09-29 01:30–02:08 UTC 在我们自己的东京 c7g.large(ap-northeast-1d = apne1-az2)上的实测**。
> 标注约定:**[官方]** 交易所文档/公告;**[社区]** 第三方实测/博客/SDK 源码;**[自测]** 我们的 EC2 实测;**(待核实)** 找不到一手来源或来源互相矛盾。
> 与 `03-onchain-bsc-solana.md`(链上腿)、`04-oss-frameworks-and-patterns.md`(框架/库)不重复,本篇只讲 CEX + Hyperliquid 的网络与协议层。

---

## 0. 结论摘要(Top 10,带数字)

1. **2026-11-12 之后所有主力交易所的撮合引擎都在 AWS 东京同一个 Region**:Binance、Gate、KuCoin、Bitget、HTX、Hyperliquid 一直在 ap-northeast-1;OKX 官方公告 2026-07 末从香港迁到东京(已生效,[自测] OKX `bbo-tbt` 事件→接收 p50 从迁移前 ~6 ms 降到 **3.6 ms**);Bybit 官方公告 **2026-11-12 08:00 UTC** 从 AWS Singapore 迁到 **AWS Tokyo apne1-az4**。Coinbase International(INTX)官方公布在 ap-northeast-1 **apne1-az1**,Hyperliquid 基金会非验证节点也在 **apne1-az1**。"选 Region" 的问题已经消失,剩下的是 **选 AZ、选通道、拿专线**。
2. **AZ 是当前唯一还能砍掉 ~1 ms 的网络变量,而且证据已经足够指向 apne1-az4**:Bybit 公告明确写 apne1-az4;2025-04-15 AWS Health Dashboard 记录 "单个 AZ(apne1-az4)EC2 连通性中断",同时打掉 Binance 与 KuCoin [官方 AWS 经 DCD 转述]。[自测] 从 apne1-az2 出发,Binance 全部 8 个 LB 节点 IP 的 TCP 握手 RTT 都是 1.29–1.83 ms(没有一个同 AZ 节点),而 Gate 的 LB 有 0.38/0.42 ms 的同 AZ 节点、0.83–1.07 ms 与 1.35–1.54 ms 的另两个 AZ 节点 —— Binance 是单 AZ 部署、且不在 az2。**迁移前用 az1 / az4 各一台 spot 实例跑 `_src/probe2.py`,谁对 Binance 的 RTT 是 ~0.3 ms 就是答案。**
3. **下单一律走 WebSocket 会话通道,REST 只做兜底**:[自测] Binance `ws-fapi` 未鉴权 `time` 请求往返 p50 **2.25 ms**(`ping` 1.95 ms),spot `ws-api` 2.51 ms;OKX WS 应用层 ping 往返 6.4 ms 但 REST `/public/time` 热连接 **14–16 ms**(Cloudflare 回源多一跳);Bybit 迁移前 WS ping 70.6 ms / REST 71–80 ms。所有交易所的 TLS 握手在我们的 Graviton 小机上要 **60–150 ms**(Python ssl 含证书链校验),任何 "按需建连" 都不可接受 —— 必须常驻热连接 + 备连接。
4. **每家都有 "免逐单签名" 的路径,除 Hyperliquid 外签名成本可以忽略**:Binance `session.logon`(仅 Ed25519,spot 与 futures WS API 都支持 [官方]),OKX/Bybit/Bitget/Gate/KuCoin 的 WS 下单都是 "登录一次,会话内请求不再签名"(OKX/Bitget/Gate/KuCoin 登录用 HMAC,Bybit 用 HMAC 或 RSA);Hyperliquid 每个 action 一次 secp256k1 EIP-712 签名(~50–150 µs,一批订单只签一次)。真正要花心思的是 **时钟**:Binance 拒绝比服务器快 >1 s 的时间戳、`recvWindow` 建议 ≤5000;OKX 30 s、Gate 60 s、Bybit `recv_window` 默认 5000;OKX `expTime`、Gate `x-gate-exptime`、KuCoin `allowMaxTimeWindow`、Hyperliquid `expiresAfter` 都提供引擎侧 "过期即拒" 的陈旧订单保护,应全部启用并收紧到 1–2 s。
5. **行情选 "真实时" 的 BBO/逐笔,深度只做校验与二级信号,各家服务端节拍如下**:Binance futures `bookTicker` 实时、`depth` 100/250/500 ms(**2026 文档里没有 `@0ms`**,我们旧假设需修正)、`aggTrade` 100 ms 聚合;Binance spot SBE `bestBidAsk`/`trade` 实时(µs 时间戳、自动剔除陈旧更新)、SBE `depth` diff **20 ms**;OKX `bbo-tbt` 10 ms、`books-l2-tbt` 10 ms(**VIP4+**)、`books` 100 ms、SBE 通道;Bybit `orderbook.1` 10 ms、`.50` 20 ms、`.200` 100 ms、`.1000` 200 ms,带撮合时间戳 `cts`;Bitget UTA `books1` **1 ms**、`books5` 10 ms、`books50` 20 ms、`books` 50 ms;Gate `spot.book_ticker` 10 ms、`spot.obu` 50 档 20 ms、futures `order_book_update` 20 ms、**SBE 推送**;KuCoin `level1` 10 ms、`level2` 实时增量、`level2Depth5/50` 100 ms;Hyperliquid 公开 WS `l2Book` 默认 **~5 s 一帧**、`fast:true` ~0.5 s、`bbo` 逐块(p50 108 ms 间隔),块级深度只能靠自建节点。
6. **[自测] 行情单向延迟(交易所事件时间 → 本地接收,apne1-az2)**:Binance futures `bookTicker` p50 **2.2 ms**(p99 3.8;9/4 长时间采集为 1.3 ms),OKX `bbo-tbt` **3.6 ms**(p99 5.3),OKX `trades` 5.7 ms,Bybit `orderbook.1` **36 ms**(新加坡,11-12 后应降到 1–3 ms),Bitget(9/4)p50 6.5 ms 但 p90 48 / p99 140 ms 的重尾。[社区] Deltix 2023 在 apne1-az1 用 pcap 测 Binance spot:p50 3.9–4.0 ms、p99 11.5–13.1 ms,三个 AZ 无显著差异。**结论:在 Tokyo 内部,AZ 只影响 ~1 ms 的 RTT 底噪;p99 由交易所侧批处理和突发决定。**
7. **CDN 前置是不可绕过的固定税,已用 AWS `ip-ranges.json` 逐个 IP 分类 [自测]**:直连 AWS EC2/ELB(无 CDN)= Binance 全部 stream / ws-api / ws-fapi / FIX / `*-mm` 主机、`api1–api4.binance.com`、Gate 全部、Coinbase INTX;CloudFront = `api.binance.com`(REST 主域!)、Bybit 全部、Bitget WS、KuCoin WS、Hyperliquid API;Cloudflare = OKX 全部(`wsaws.okx.com` 已于 2025-04-28 停用)、Bitget REST、KuCoin REST、Deribit、Kraken、Coinbase Exchange。**Binance REST 应使用 `api1–api4`(直连,官方称 "性能更好但稳定性略低")而不是 `api.binance.com`。**
8. **低延迟/MM 专线的真实门槛**:Binance Futures Low-Latency API(FAPI-MM / FSTREAM-MM,SDK 还映射了 ws-fapi-mm / dstream-mm 等)只对 **Futures Liquidity Provider Programs(USDⓈ-M Maker / COIN-M Maker / USDⓈ-M Taker)** 合格用户开放,通过 VIP Portal 白名单 IP,低成交量 IP 会被移除;Bybit MMWS / MMGW / SBE 下单(2026-06-02 上线)需机构关系且**官方声明 "不保证更低延迟"**;KuCoin MM 计划(30 日 ≥3000 万 USDT 成交)明示 "Direct Market Access and Co-location service",Colo 用户的 WS 下单回执时间戳是 ns;Gate MM 走 mm@gate.com,VIP14+ 按成交率放宽限速;OKX **没有任何 colocation/专线**,只有 VIP2+ 的 MM 计划和 VIP5+ 按 fill ratio 放宽到 10 000 单/2 s;Bitget 只有 MM/PRO 机构限速(单账户 600/s,母子合计 12 万/s),"LOLA" 与 "Bitget SBE" 在官方文档中**均未找到**(待核实,可能是内部叫法或误传);Deribit 新 "Starbase" 撮合(SBE 下单 + 组播行情)只接受 LD4 colo/cross-connect/AWS PrivateLink;Kraken 在 Equinix London 提供 cross-connect(~200 µs),东京过去 ~140 ms。
9. **连接卫生决定 p99**:Binance 24 h 强断(有 `serverShutdown` 预告)、spot 服务端 20 s ping / 1 min 无 pong 断、futures 3 min / 10 min、300 次连接/5 min/IP、1024 streams/连接;Bybit 500 连接/5 min/域名、20 s ping、私有/交易连接 `max_active_time` 最长 10 min;Bitget 100 连接/IP、300 次/5 min、**120 s 无应用层 ping 即断(即使数据在流)**、我们实测 ~150 s;OKX 3 次连接/s/IP、480 次 sub/login/h/连接、30 s 无数据断、每个私有频道每子账户 30 连接;KuCoin 18 s ping、bullet token 24 h;Hyperliquid 10 连接/IP、60 s 无消息断。按 IP 的限速(Binance REQUEST_WEIGHT、连接数;Bybit 600 req/5 s、3000 req/s WS;Hyperliquid 1200 weight/min)可以用多 ENI/EIP 叠加,**按账户/UID 的下单限速只能靠多账户/子账户**。
10. **Hyperliquid 是 "区块链节拍" 的场所,不是低延迟 CEX**:HyperBFT 每 ~70 ms 一块,官方给出的同城端到端(发出→提交回执)**中位 0.2 s、p99 0.9 s**;撮合优先级 "同一批次内 cancel 与 ALO 先于 IOC/GTC",天然偏向做市方;`split_client_blocks` 可提前 70–150 ms 看到 mempool 输入;gossip/下单优先费可再买 ~25 ms/槽、~45 ms/bp;地址级限速 "每成交 1 USDC 换 1 个请求"。Glassnode "Tokyo 200 ms edge" 一文**未能定位**(research.glassnode.com 2139 篇文章的 sitemap 无相关 slug,insights 域被 Cloudflare 挑战页挡住),但官方文档本身已足以说明 "东京 + 自建节点 + ALO 做市 + Binance 对冲" 的结构。

---

## 1. 地理、AZ 与网络拓扑

### 1.1 撮合引擎位置(2026-09-29)

| 交易所 | 位置 | 证据等级 | 前置(2026-09-29 [自测] IP 分类) | 备注 |
|---|---|---|---|---|
| Binance spot / USDⓈ-M / COIN-M | AWS ap-northeast-1;AZ 假说 **apne1-az4** | [官方 AWS 事故记录] 2025-04-15 apne1-az4 连通性中断 → Binance 暂停提币 ~5 min;[自测] 8 个 LB IP 全部跨 AZ(1.3–1.8 ms) | 直连 EC2/ELB(所有 stream/ws-api/ws-fapi/fix/*-mm/api1–4);`api.binance.com` = CloudFront | COIN-M 架构 2026-06-30 并入 USDⓈ-M [官方 changelog] |
| OKX | 东京(2026-07 末迁自香港);云厂商与 AZ **未披露**(全域 Cloudflare) | [官方公告 2026-05-14,2026-06-12 改期];[自测] 迁后 `bbo-tbt` 3.6 ms | Cloudflare(REST/WS 全部,NRT 边缘 0.9–1.1 ms) | `openapi.okx.com` 为 2026-05-20 起推荐 REST 域;`wsaws.okx.com`、`aws.okx.com` 已 NXDOMAIN |
| Bybit | AWS Singapore apse1-az2/az3 → **AWS Tokyo apne1-az4,2026-11-12 08:00 UTC** | [官方公告 2026-09-02;FAQ 仍写新加坡] | CloudFront(api/stream.bybit.com 与 api/stream.bytick.com,NRT 边缘 0.8–1.5 ms) | 迁移前 [自测] WS ping 70.6 ms、REST 71–80 ms |
| Bitget | AWS 东京(社区/自测推断,无官方声明) | [社区] Arbitron REST 24 ms 最快;[自测] CDN 边缘 0.65–1.8 ms | WS = CloudFront;REST = Cloudflare(GSLB,按地域可能不同) | 无 Region 声明(待核实) |
| Gate | **AWS ap-northeast-1(官方声明 "Gate data center is located in AWS Japan's ap-northeast-1 region")**;LB 三 AZ 均有节点 | [官方 REST 文档];[自测] 同 AZ 节点 0.38 ms / 跨 AZ 0.83–1.54 ms | 直连 ELB/EC2(`api.gateio.ws` = `balancer-gateio-ws-*.ap-northeast-1.elb.amazonaws.com`) | 我们此前 0.27 ms minRTT 即命中同 AZ 节点 |
| KuCoin | AWS 东京;AZ 假说 apne1-az4(2025-04-15 同批受影响) | [官方 AWS 事故记录经 DCD];[社区] Tsybko 2023 "Tokyo clear leader" | WS = CloudFront;REST = Cloudflare | MM 计划明示 co-location [官方] |
| Hyperliquid | 验证人/根节点集中在东京;**基金会非验证节点 apne1-az1** [官方] | [官方 node README "For lowest latency, run the node in Tokyo"];[自测] `gossipRootIps` 24 个 IP 中 19 个落在 ap-northeast-1 | `api.hyperliquid.xyz` = CloudFront(0.9 ms 边缘) | 自建节点直连 gossip,不经 CDN |
| Coinbase International (INTX) | **AWS ap-northeast-1,全部组件 apne1-az1** [官方 Systems & Operations] | 官方表格:FIX OE 网关 / OE 网关 / Trade Engine / WS MD / FIX MD 均 apne1-az1 | 直连 EC2(3 个 LB IP:0.49 / 1.36 / 1.81 ms) | **2026-10-01 并入 Deribit(伦敦)**,东京引擎寿命有限 |
| Deribit | Equinix LD4 伦敦;新 Starbase 引擎仅 colo/cross-connect/PrivateLink | [官方 docs] | Cloudflare(边缘 2.4 ms) | [自测] 东京 REST 总耗时 ~270 ms,`usDiff` 67–89 µs |
| Kraken | Equinix London(Spot 与 Derivatives 引擎) | [官方 colocation 指南、Unlocked #5 博文] | Cloudflare | 官方:cross-connect ~200 µs、AWS London ~2 ms、Tokyo↔London 私网 ~140 ms |
| Coinbase Exchange (spot) | 未核实(常被引为 us-east-1) | (待核实) | Cloudflare | 非目标 |

### 1.2 AZ 层证据链与实验设计

- AZ 名 ↔ AZ ID 每个账户随机映射,只有 AZ ID(apne1-az1/az2/az4;az3 无可用实例类型)跨账户可比;实例元数据 `placement/availability-zone-id` 可直接读到(我们的 c7g.large:ap-northeast-1d = apne1-az2)。当前 EC2 角色没有 `ec2:DescribeAvailabilityZones` 权限,映射表要在控制台查。
- [自测 probe2,2026-09-29] 每个主机名的全部 A 记录逐个测 TCP 握手 RTT(9 次取最小):
  - Binance 14 个主机名、共 ~80 个 IP:**全部 1.29–1.83 ms**,无一低于 1.2 ms → Binance 的 NLB 节点只在一个(非 az2 的)AZ;
  - Gate `api.gateio.ws`:0.38、0.42(同 AZ)/ 0.83、0.99、1.01、1.07 / 1.35、1.54 → 三个 AZ 各有节点;`fx-ws.gateio.ws` 同样三簇(0.40/0.87/1.61);
  - Coinbase INTX:0.49 / 1.36 / 1.81 → 三 AZ 节点,但引擎官方在 az1。
- 这给出一个 **零成本的 AZ 识别方法**:在候选 AZ 起 spot 实例跑 `probe2.py`,对 Binance 的 RTT 从 ~1.4 ms 掉到 ~0.3 ms 的那个 AZ 就是 Binance 所在 AZ。结合 Bybit 官方 az4、AWS 事故 az4,预期答案是 az4;若结果是 az1,则 az1 同时拥有 Binance + INTX + Hyperliquid 基金会节点,也应迁。
- 同 Region 跨 AZ 往返 0.5–1.5 ms、同 AZ 0.1–0.4 ms(AWS/FlashGrid/xkyle 数据与我们实测一致);Deltix 2023 三 AZ 行情延迟无差异说明 **AZ 只改善 RTT 底噪,不改善交易所侧批处理**。
- AWS 官方给交易所的 "云原生 colocation" = shared cluster placement group + VPC peering(One Trading 案例:目标 RTT <200 µs,比走 CloudFront 的公网访问 p99 快 98%);目前**没有任何一家加密交易所对普通 API 用户开放 shared CPG**,KuCoin MM 计划的 "co-location" 是唯一公开提到的入口。

### 1.3 [自测] 端点 RTT 与 CDN 分类表(2026-09-29 02:00 UTC,apne1-az2,TCP 握手最小值)

| 主机 | RTT (ms) | 分类 | 主机 | RTT (ms) | 分类 |
|---|---|---|---|---|---|
| fstream.binance.com | 1.35–1.69 (8 IP) | AWS EC2 apne1 | ws.okx.com:8443 | 0.88 | Cloudflare |
| fstream-mm.binance.com | 1.45–1.83 (8 IP) | AWS EC2 apne1 | www.okx.com | 1.06 | Cloudflare |
| ws-fapi.binance.com | 1.42–1.49 (4 IP) | AWS EC2 apne1 | stream.bybit.com | 1.52 | CloudFront |
| ws-fapi-mm.binance.com | 1.35–1.48 (6 IP) | AWS EC2 apne1 | api.bybit.com | 0.80 | CloudFront |
| fapi.binance.com / fapi-mm | 1.29–1.52 / 1.34–1.44 | AWS EC2 apne1 | ws.bitget.com | 0.65 | CloudFront |
| stream.binance.com | 1.34–1.61 (8 IP) | AWS EC2 apne1 | api.bitget.com | 1.84 | Cloudflare |
| ws-api.binance.com | 1.41–1.61 (4 IP) | AWS EC2 apne1 | api.gateio.ws | **0.38**–1.54 (8 IP) | AWS ELB apne1 |
| stream-sbe.binance.com | 1.33–1.56 (8 IP) | AWS EC2 apne1 | fx-ws.gateio.ws | **0.40**–1.61 (8 IP) | AWS EC2 apne1 |
| fix-oe / fix-md.binance.com:9000 | 2.05 / 2.07 | AWS EC2 apne1 | ws-api-spot / -futures.kucoin.com | 1.51 / 0.76 | CloudFront |
| api1 / api4.binance.com | 1.36–1.47 | AWS EC2 apne1 | api.kucoin.com | 1.22 | Cloudflare |
| **api.binance.com** | 1.06 | **CloudFront** | api.hyperliquid.xyz | 0.94 | CloudFront |
| dstream / ws-dapi / dapi(COIN-M) | 1.33–1.55 | AWS EC2 apne1 | api / ws-md.international.coinbase.com | 0.49–1.81 (3 IP) | AWS EC2 apne1 |
| data-stream.binance.vision | 1.41 | AWS EC2 apne1 | www.deribit.com / ws.kraken.com | 2.38 / 2.40 | Cloudflare |

CDN 边缘的 RTT 不代表源站距离(Bybit 边缘 0.8 ms 但源站 70 ms);判断源站要看应用层往返(§1.4)。

### 1.4 [自测] 应用层往返(已建立的 TLS WebSocket 上,25 次)

| 目标 | 请求 | min / p50 / p90 / max (ms) | 说明 |
|---|---|---|---|
| Binance `ws-fapi` | `{"method":"time"}` | 2.04 / **2.25** / 3.84 / 4.74 | 网关→后端→网关,未鉴权,最接近下单 ack 的下界 |
| Binance `ws-fapi` | `{"method":"ping"}` | 1.76 / **1.95** / 2.76 / 4.70 | 网关本地应答 |
| Binance `ws-api`(spot) | `time` | 2.17 / **2.51** / 3.03 / 7.34 | |
| Binance `fstream` / `stream` | WebSocket ping 帧 | 23–40 / **49.5** / 50.6 / 194 | 行情服务器对协议层 ping 的响应被批处理到 ~50 ms,**不能用它测网络** |
| OKX `/ws/v5/public` / `/private` | 文本 `ping` | 5.8 / **6.4** / 6.9 / 7.1;6.2 / 6.8 / 9.4 / 15.4 | Cloudflare 回源后应答 |
| Bybit `/v5/public/linear` | `{"op":"ping"}` | 70.4 / **70.6** / 71.3 / 71.9 | 新加坡源站;`/v5/trade` 未登录不回 ping |
| OKX REST `/public/time`(热连接)[子代理自测] | GET | 14–16 | Cloudflare REST 路径比 WS 多 ~10 ms;10 req/2 s/IP 即 429 |
| Binance REST `fapi/v1/time`(热连接)[子代理自测] | GET | 3.1–3.6 | 直连 |
| Bybit REST `/v5/market/time`(热连接)[子代理自测] | GET | 71–80 | 迁移前 |
| Deribit REST `public/test`(冷连接)| GET | connect 4 ms,TLS 33 ms,总 274 ms | 伦敦源站;`usDiff` 67–89 µs |

握手成本:Python ssl 在 c7g.large 上 TLS 握手 60–150 ms、WS 升级 3–22 ms(Bybit 回源升级 ~210 ms);Rust rustls 会好很多,但 "30 ms 量级" 的 TLS 是常态 → 永远不要在热路径上建连。

---

## 2. Binance(spot、USDⓈ-M、COIN-M)

### 2.1 摘要表

| 项目 | Spot | USDⓈ-M / COIN-M Futures |
|---|---|---|
| 位置 | ap-northeast-1,AZ 假说 az4 | 同左;COIN-M 2026-06-30 并入 USDⓈ-M 架构 |
| 最快行情 | **SBE 流** `wss://stream-sbe.binance.com/ws`(Ed25519 API key 放 `X-MBX-APIKEY`,无签名):`@trade` 实时、`@bestBidAsk` 实时(含 `eventTime` µs、auto-culling)、`@depth` diff **20 ms**、`@depth20` 50 ms;JSON:`@bookTicker` 实时(**无时间戳字段**)、`@trade` 实时、`@depth@100ms`;`timeUnit=MICROSECOND` | **JSON only**(无 SBE/FIX):`wss://fstream.binance.com/public`(`@bookTicker` 实时、`@depth` 250 ms 默认/500/100 ms、`@depth5/10/20`)、`/market`(`@aggTrade` 100 ms 聚合、`@markPrice@1s`、`@forceOrder`)、`/private`(listenKey);**无 `@trade`、文档无 `@depth@0ms`**;旧无路径 URL 2026-04-23 退役,只送 `/public` 内容(我们 9/23 的 aggTrade 不推送事故即此) |
| 最快下单 | WS API `wss://ws-api.binance.com:443/ws-api/v3`:`session.logon`(Ed25519)→ `order.place / order.cancel / order.cancelReplace / order.amend.keepPriority`(仅减量、保队列)/ 订单列表;FIX OE `tcp+tls://fix-oe.binance.com:9000`(Ed25519,10 并发/账户,10 000 msg/10 s/连接;FIX-SBE 端口 9001/9002);用户数据流 2026-02-20 起改为在 WS-API 连接上订阅(listenKey 移除) | WS API `wss://ws-fapi.binance.com/ws-fapi/v1`(COIN-M `ws-dapi`):`session.logon`(Ed25519,同 spot 语义)、`order.place`(IP 权重 0)/ `order.modify`(仅 LIMIT,**改单会重新排队**)/ `order.cancel` / `order.status`;REST `batchOrders` 最多 5 单、并发处理不保证顺序;MM 专线 `fapi-mm` / `fstream-mm`(FAQ 明示),`ws-fapi-mm` / `dstream-mm` / `dapi-mm`(SDK 映射,待核实) |
| 认证 | HMAC-SHA256(官方称 deprecated)/ RSA / **Ed25519(推荐,唯一支持 logon)**;`recvWindow` 默认 5000、最大 60000、建议 ≤5000;时间戳比服务器快 >1 s 即拒 | 同左;Ed25519 on futures REST(可能,待核实) |
| 限速 [官方 exchangeInfo] | REQUEST_WEIGHT 6000/min/IP;ORDERS **100/10 s 与 200 000/日**/账户(成交单不计入);RAW_REQUESTS 300 000/5 min;WS 连接 300 次/5 min/IP、1024 streams/连接、5 条入向消息/s、24 h | REQUEST_WEIGHT **2400/min/IP**;ORDERS **300/10 s 与 1200/min**/账户;WS 10 条入向/s、1024 streams(2025-07-02 由 200 提升)、24 h;WS-API 握手权重 5 |
| 时间戳 | `transactTime` = 撮合时间;`workingTime` = 上簿时间;用户流 `E` 事件时间、`T` 撮合时间 | 响应只有 `updateTime`;行情 `E` 事件时间、`T` 撮合时间、`pu` 上一 update id |
| STP | NONE / EXPIRE_MAKER(默认)/ EXPIRE_TAKER / EXPIRE_BOTH / DECREMENT / TRANSFER | EXPIRE_MAKER / TAKER / BOTH |
| MM 计划 | Spot Liquidity Provider / MM Program(无专用主机,`api1–api4` 直连即最优) | **Futures Low-Latency API Services**:仅 Futures LP Programs(USDⓈ-M Maker / COIN-M Maker / USDⓈ-M Taker)合格用户,VIP Portal → IP Whitelisting 选 FAPI-MM / FSTREAM-MM,有 IP 配额,低成交量 IP 会被移除,可申请白名单管理 API |

### 2.2 细节与坑

- **RPI 订单**(2025-11-18 futures 引入)不出现在 `bookTicker`/`depth`,有单独 `@rpiDepth@500ms`;OKX/Bybit/Gate 也各有 RPI 机制,做 BBO 对比时注意 "看得见的最优价" 未必可成交。
- 官方建议 "按流量类型拆连接(public / market / private)以降低单连接负载与抖动";社区(picows 维护者)经验:一条连接不要超过 5–10 个高频流,400 个 bookTicker 会掉包。
- [社区 NeutK] 5 条并行 `fstream` 连接按 `u` 去重、定时替换最慢的一条,是 Tokyo 上常见的 "赛马" 做法;BTC 期货 bookTicker ~200 msg/s、波动时 ×14。
- spot SBE 在 NeutK 的测试里 "并不比 JSON 更早到达",但它给出 µs `eventTime` 与自动剔除;是否采用取决于我们是否需要 spot 的精确时间戳。
- `-1007 TIMEOUT`:API 等撮合 10 s 后返回 "状态未知",不等于失败(与我们 BSC 事故同类,状态机必须区分 "传输超时" 与 "拒单")。
- 2025-01-28 起 spot 服务端 ping 20 s / 1 min 无 pong 断;futures 仍 3 min / 10 min;`serverShutdown` 事件在 24 h 断线前预告 —— 用它触发主备切换而不是被动重连。

---

## 3. OKX

| 项目 | 内容 |
|---|---|
| 位置 | 东京(2026-07 末迁自香港,云厂商未披露);Cloudflare 前置 |
| 端点 | REST `https://openapi.okx.com`(2026-05-20 起推荐,www 仍可用);WS `wss://ws.okx.com:8443/ws/v5/public` / `/private` / `/business`;**SBE `wss://ws.okx.com:8443/ws/v5/public-sbe`**;`wsaws.okx.com` 2025-04-28 停用 |
| 最快行情 | `bbo-tbt`(1 档,10 ms 有变化才推,所有等级,SBE 版需登录)、`books-l2-tbt`(400 档增量 10 ms,**VIP4+**,原 VIP6)、`books50-l2-tbt`(VIP4+,原 VIP5)、`books`(400 档 100 ms)、`books5`(100 ms)、`books-rpi`(含 RPI 量,100 ms)、`trades`(按 taker 单聚合)/ `trades-all`(逐笔,/business)、`tickers`(≤1/100 ms);**checksum 2026-06-23 起固定为 0,用 `seqId`/`prevSeqId`**;同一连接推送顺序固定 bbo-tbt → books-l2-tbt → books50 → books → books-rpi → books5 |
| SBE | 通道 trades / bbo-tbt / books-l2-tbt;同一连接 JSON(opcode 1)与 SBE(opcode 2)混发;价格量用 mantissa/exponent;`instIdCode` 整数;`tsUs` µs;无 checksum;无初始快照 → REST `GET /api/v5/market/books-sbe`(400 档,~500 ms 一版,10 req/10 s/IP+instIdCode);服务端 20 s ping、60 s 无 pong 断 |
| 最快下单 | WS `/private` 登录后 `order` / `batch-orders`(≤20)/ `amend-order` / `batch-amend-orders` / `cancel-order` / `batch-cancel-orders`;**2026-03-26 起 WS 下单必须用整数 `instIdCode`**(`instId` 被移除 "以降低 WS 下单延迟");每单最多 3 个在途 amend;响应含 `inTime` / `outTime`(µs,网关收到/发出;文档示例 1.9 ms 停留)与 `ts` |
| 认证 | REST `OK-ACCESS-SIGN = Base64(HMAC-SHA256(ts+method+path+body))`,30 s 窗口;WS `login` 签 `ts+"GET"+"/users/self/verify"`,登录后会话内请求不再签名;API 下单需 KYC L2(2026-04-07) |
| 限速 | 下单/改单/撤单 **60 req/2 s /UID+instId**(与 REST 共享);批量 300 单/2 s;子账户合计 1000 新+改/2 s,VIP5+ 按 7 日 fill ratio 分 8 档到 **10 000/2 s**(`GET /trade/account-rate-limit` 查);WS 3 连接/s/IP、480 sub/unsub/login /h/连接、30 s 无数据断、每私有频道每子账户 30 连接;REST 公共 `/public/time` 10 req/2 s/IP |
| 延迟保护 | `expTime`(ms):服务器时间超过即不处理(place / amend / 批量);`rpiTakerAccess=true` 会给**所有** ordType 加 speed bump —— 热路径必须为 false;`stpMode` cancel_maker(默认)/ cancel_taker / cancel_both,"强制 STP 不增加延迟" |
| 订单类型 | market / limit / post_only / fok / ioc / optimal_limit_ioc / mmp / mmp_and_post_only / rpi(elp 别名至 2026-10-31);`tgtCcy`(spot 市价);2026-08-20 起 post_only/rpi 的 `state: live` 要上簿后 ~1 ms 才推 |
| MM 计划 | Market Maker Program(VIP2+ 或他所 MM,Institutional@okx.com)= GLP(Global Liquidity Program,SPOT / PERP / FUT_NTO 三条线),收益只是费率;**无 colocation / 专线 / AWS 直连** |
| [自测] | `bbo-tbt` 事件→接收 p50 3.6 ms(min 3.2,p99 5.3);`trades` 5.7 ms;`books5` spot 4.9 ms;WS 应用 ping 6.4 ms;REST 热连接 14–16 ms;消息大小 bbo-tbt ~184 B、trades ~220 B、books5 ~454 B |

我们 9/19 报告中 "Binance 领先 OKX ~10 ms" 是香港时代的数据;现在两者事件→接收差 ~1.4 ms(3.6 vs 2.2),必须重做 lead-lag 研究;Bybit 11-12 迁完后再做一次。

---

## 4. Bybit

| 项目 | 内容 |
|---|---|
| 位置 | AWS Singapore(FAQ:apse1-az2 & az3)→ **AWS Tokyo apne1-az4,2026-11-12 08:00 UTC**,端点与字段不变 |
| 端点 | REST `api.bybit.com` / `api.bytick.com`(文档列为备用);WS `stream.bybit.com`(`/v5/public/{spot,linear,inverse,option}`、`/v5/private`、`/v5/trade`);`stream.bytick.com` 可用但未列入文档(待核实);全部 CloudFront(NRT 边缘) |
| 最快行情 | linear/inverse `orderbook.1` **10 ms**、`.50` 20 ms、`.200` 100 ms、`.1000` 200 ms(无 `.500`);spot `.1` 10 ms、`.50` 20 ms、`.200` 100 ms(2026-03-19 由 200 ms 提速)、`.1000` 200 ms;option `.25` 20 ms、`.100` 100 ms;L1 为快照、3 s 无变化重发;字段 `ts`(系统生成时间)与 **`cts`(撮合引擎时间,可与成交 `T` 对齐)**;`publicTrade` 实时(≤1024 笔/帧);`tickers` 衍生品 100 ms、spot 50 ms;RPI 单不进 orderbook |
| 最快下单 | WS Trade `wss://stream.bybit.com/v5/trade`:`order.create / amend / cancel`(单参数)与 `*-batch`(**linear/inverse 20、spot 10、option 5**);头 `X-BAPI-TIMESTAMP`、`X-BAPI-RECV-WINDOW`(默认 5000;规则 `server−recv ≤ ts < server+1000`);ack 头含 `Timenow`、`X-Bapi-Limit-Status`、`X-Bapi-Limit-Reset-Timestamp`;ack ≠ 成交,要看 `order`/`execution` 流;`execution.fast` 流为低延迟精简成交回报;WS 与 HTTP 共享账户限速;`10403` = 3000 req/s/IP |
| 认证 | HMAC-SHA256(系统生成 key)或 **RSA-SHA256**(自生成 key);WS auth 签 `GET/realtime{expires}`;美国/中国大陆 IP 403 |
| 限速 | 按 UID 滚动 1 s:create/cancel/batch **10/s**(linear/inverse/option)、**20/s** spot、amend 10/s;批量端点独立桶按单计;VIP 默认 10/10/20 → VIP3–Supreme 60/60/40;**PRO 框架(2025-08-13)** PRO1 200/s … PRO6 1200/s、MM1–3 600/800/1000/s,机构合计 1–6 万/s;HTTP 600 req/5 s/IP(403 = 封 10 min);WS 500 连接/5 min/域名、≤1000 行情连接/IP、spot 每次订阅 ≤10 args、20 s ping、私有/交易连接 `max_active_time` 30 s–10 min |
| 订单 | `timeInForce` GTC/IOC/FOK/PostOnly/RPI;`rpiTakerAccess`(2026-06 起,IOC/FOK 2026-09-23 起);`smpType` None/CancelMaker/CancelTaker/CancelBoth;市价单在价格带内转 IOC 限价;`slippageToleranceType` |
| 机构通道 | **MMWS**(私有主机名,≤10 白名单 IP,承载 private/trade/public JSON 与 `/v5/public-sbe/*`)与 **MMGW**(私有 HTTP 主机名承载 `/v5/order/*` 等)—— 官方原文 "**does not guarantee lower absolute latency**",定位是稳定性;**SBE**(仅 MMWS):BBO / L50 / PublicTrade、`/v5/trade-sbe` 下单(2026-06-02 上线)、`/v5/private-sbe` Fast Order Response,µs 时间戳、载荷小 30–50%、关闭压缩,1500(spot)/3000(futures)连接/主机;FIX 4.4 仅 spot(`fix-oe.bybit.com:9000`,RSA);无 colocation 产品 |
| [自测,迁移前] | `orderbook.1` 事件→接收 p50 36.2 ms(`cts` 37.4;p99 44.6–51.6);`publicTrade` 38.8 ms;引擎→网关(`ts−cts`)中位 1.2 ms;REST 71–80 ms;WS 握手 ~240 ms |

**策略含义**:11-12 之前 Bybit 只能做 "慢腿"(先成交快腿再补 Bybit),或暂不上线;11-12 之后 Bybit 与 Binance/KuCoin 极可能在同一个 AZ(az4),Bybit 会成为延迟最好的腿之一。

---

## 5. Bitget

| 项目 | 内容 |
|---|---|
| 位置 | AWS 东京(推断);WS `ws.bitget.com` = CloudFront,REST `api.bitget.com` = Cloudflare(GSLB) |
| 最快行情(UTA V3)| `books1`(1 档快照)**1 ms**;`books5` 10 ms;`books50` 20 ms;`books`(全深度 snapshot+update)**50 ms**;`seq`/`pseq` 链续(pseq(N+1)=seq(N),重置后 pseq=0),UTA 无 checksum;公共频道另有 Tickers / New-Trades / RPI-OrderBook / Liquidation。Classic V2:`books/books1/books5/books15/trade/ticker`,我们 [自测] `books` 帧 `seq/pseq` 无 checksum |
| "SBE" | **官方文档未找到 Bitget SBE 通道**(UTA 与 Classic 文档、changelog 均无);[社区 Tardis] "2026-04-28 起经 Bitget WS v3 以 1 ms 采集,此前 v2 为 10 ms" —— 我们此前 "Bitget 公开 SBE books1@1 ms" 的记忆应更正为 "**JSON `books1` 1 ms**",SBE 为待核实 |
| 最快下单 | **UTA V3 WebSocket 下单**:changelog 2026-09-17 列出 WS `place-order` / `batch-place` / `modify-order` / `batch-modify` / `cancel-order` 频道(REST 对应 `/api/v3/trade/place-order`、`/place-batch`、`/modify-order`、`/batch-modify-order`、`/cancel-order`);第三方 SDK(tiagosiebler/bitget-api)确认 "V3 WebSocket order placement";每频道限速与批量上限页面未能打开(待核实);Classic:REST place 10 req/s/UID、batch 5/s(10 单/币对计 1 次)、cancel 10/s;新参数 `pxAmendType`(超价格带拒单/夹到边界) |
| 认证 | `ACCESS-KEY / ACCESS-SIGN / ACCESS-TIMESTAMP / ACCESS-PASSPHRASE`,`Base64(HMAC-SHA256(ts+METHOD+path+query+body))`,可选 RSA,30 s 窗口;WS login 签 `ts+"GET"+"/user/verify"`(我们实测通过) |
| 限速 | VIP:默认 **10/s**,VIP1 60/s,VIP2 80/s,VIP3–7 100/s(Classic 与 UTA 一致);**机构(MM/PRO)限速**:MM 单账户 Tier1 600/s(300 子账户)…Tier4–5 200/s,母子合计 Tier1 **120 000/s**…Tier4–5 40 000/s,WS 与 REST 共享,`POST /api/v3/user/set-rate-limit-quota` 自行分配;WS:300 连接请求/5 min/IP、≤100 连接/IP、240 订阅请求/h/连接、≤1000 频道/连接(建议 <50)、10 msg/s/连接、客户端 30 s `ping`、**服务端 120 s 无 ping 即断(即使有数据)**([自测] ~150 s) |
| MM 计划 | 仅上述机构限速与 Elite Trading API(跟单,非低延迟);**"LOLA" 在官方任何页面/仓库中均未找到(待核实)**;无 colocation 声明 |
| [自测 9/4] | 事件→接收 p50 6.5 ms、p90 48、p99 140 ms,~10% 深度更新落在 50–200 ms;接收→建簿 p99 0.2 ms → 重尾在交易所侧;`books1` 1 ms 是否消掉重尾是首要实验 |

---

## 6. Gate

| 项目 | 内容 |
|---|---|
| 位置 | **官方:AWS Japan ap-northeast-1**;`api.gateio.ws` = ELB 直连、`fx-ws.gateio.ws` = EC2 直连,LB 节点覆盖三个 AZ([自测] 同 AZ 0.38–0.42 ms) |
| 最快行情 | spot `spot.book_ticker` **10 ms**;`spot.obu`(Order Book V2)**50 档 20 ms / 400 档 100 ms**(首帧 `full=true`,`U/u` 连续);`spot.order_book_update` 100 ms(1000 ms 档 2024-11 移除);`spot.trades` 实时;futures `futures.book_ticker` 实时、`futures.order_book_update` **20 ms(仅 20 档)** / 100 ms(20/50/100 档)、`futures.trades` 实时;每帧带 `time_ms` |
| SBE | **`wss://api.gateio.ws/ws/v4/ws/spot/sbe`**(官方 "更快、更省带宽";测试网 2026-06-09 增加):JSON 请求 + 首响应,之后 SBE 帧(opcode 2);8 字节小端头(blockLength / templateId / schemaId / version);schemaId 0 字符串小数、1 定点、2 定点+统一簿字段;模板 bbo=1、publicTrade=2、obu=3、orderBook=4、orderBookUpdate=5、…、orders=9、balances=10–13;**futures SBE 未见文档**(待核实) |
| 最快下单 | WS API v4:`spot.login` → `spot.order_place`(载荷可为 `/spot/orders` 或 `/spot/batch_orders`)/ `order_cancel` / `order_cancel_ids` / `order_cancel_cp` / `order_amend` / `order_status`;`futures.login` → `futures.order_place` / `order_batch_place` / `order_cancel` / `order_cancel_cp` / `order_amend` / `order_status`;签名 `HexEncode(HMAC_SHA512(secret,"api\n<channel>\n<req_param>\n<ts>"))`,60 s 窗口;响应头 **`x_in_time` / `x_out_time`(µs 网关进出)**、`conn_id`、`trace_id`、`x_gate_ratelimit_*`;`action_mode` = ACK(异步精简)/ RESULT / FULL;请求头 **`x-gate-exptime`**(ms,超时拒单;REST 批量与期货下单也支持) |
| 认证 | REST `KEY / Timestamp(秒,≤60 s)/ SIGN = HexEncode(HMAC_SHA512(METHOD\nURL\nquery\nHexEncode(SHA512(body))\nts))` |
| 限速 | spot place/amend **10 r/s /UID+市场**、cancel 200 r/s;永续 place/amend **100 r/s**、cancel 200 r/s;公共 200 r/10 s/端点/IP;WS 同上且 **≤300 连接/IP**;成交率 <0.1 的账户 spot 下单+改单限 10/10 s(2026-01-26);VIP14+ 期货按 7 日 fill ratio 放宽 100→400 r/s;批量:spot 4 币对×10 单、amend 5;futures 10 单 |
| 订单 | TIF `gtc / ioc / poc(post-only)/ fok`;STP `stp_act = cn/co/cb`(需加入 STP 组,否则拒单),`finish_as=stp`;`create_time_ms / update_time_ms` |
| MM 计划 | 邮件 mm@gate.com(UID、成交证明、做市方式,3 个工作日);VIP11+ 需开 GT 抵扣;无 colocation 声明 |

Gate 是**信息最透明、网络最近、协议最完整(WS 下单 + µs 网关戳 + exptime + SBE)**的一家,适合作为 "延迟基准腿"。

---

## 7. KuCoin

| 项目 | 内容 |
|---|---|
| 位置 | AWS 东京(AZ 假说 az4,2025-04-15 同批受影响);REST Cloudflare、WS CloudFront;Pro/UTA 主机 `wsapi.kucoin.com`、`wsapi-push.kucoin.com`、`x-push-spot/futures.kucoin.com`(前置待核实) |
| 最快行情 | spot `/spotMarket/level1:{sym}`(BBO)**10 ms**;`/market/level2:{sym}` 实时增量;`/spotMarket/level2Depth5|50` 100 ms;`/market/match` 逐笔;futures `/contractMarket/level2` 实时增量、`level2Depth5|50` 100 ms、`tickerV2` 实时、`execution` 实时;每 topic ≤100 symbol |
| 最快下单 | **Pro WebSocket 下单** `wss://wsapi.kucoin.com/v1/private?apikey&timestamp&sign&passphrase`(+`enable_ns=true`):UTA `uta.order / uta.cancel`(及 amend),Classic `spot.*`;响应含 **`inTime` / `outTime`(Colo UTA 用户默认 ns)**、`userRateLimit{limit,remaining,reset}`;welcome 给 `pingInterval 18000 / pingTimeout 10000`;REST hf:`Add Order`(权重 1)、`Add Order Sync`、`Batch Add Orders`(≤5 单同 symbol)、`Modify Order`、DCP(dead-man switch);参数 `timeInForce GTC/GTT/IOC/FOK`、`postOnly / hidden / iceberg`、**`stp = CN/CO/CB/DC`**、**`allowMaxTimeWindow + clientTimestamp`**(网关收到时已超窗即拒)、`cancelAfter` |
| 认证 | `KC-API-KEY / KC-API-SIGN = Base64(HMAC-SHA256(ts+METHOD+endpoint+body)) / KC-API-TIMESTAMP(ms) / KC-API-PASSPHRASE(v2+ 加密)/ KC-API-KEY-VERSION 3`;时间窗未明示(历史 5 s,待核实) |
| 限速 | Classic 权重池 /30 s:VIP0 Spot 4000、Futures 2000、Management 2000、Public 2000/IP → VIP12 Spot 40 000 / Futures 20 000;UTA "Trading(含 WS 下单)" **VIP0 300/s … VIP12 4000/s**;头 `gw-ratelimit-*`;WS 下单与 REST 共享配额;Classic WS ≤800 并发连接、100 msg/10 s/连接、≤400 topic/连接(spot);UTA/Pro WS:public spot ≤500 / futures ≤800 连接/IP、private ≤1024、新连接 ≤300/5 min/IP、public 300 / private 100 msg/10 s、≤600 public / ≤400 private topic/连接 |
| MM 计划 | **Market Making Incentive Scheme**:30 日任意交易所成交 **≥30 000 000 USDT**,mm@kucoin.com;权益 "MM 返佣、**更高 API 限额、Direct Market Access 与 Co-location 服务**、≤50 万 USDT 免息借贷";另可向 api@kucoin.com 申请提限 |
| [社区] | Tsybko 2023:Tokyo 明显领先;**hf 高频账户**(需通过 API 划转到 trade_hf 激活)中位/均值往返显著低于普通账户;三 AZ 粗测无差异 |

---

## 8. Hyperliquid

### 8.1 基础设施与节拍 [官方]

- 节点 README:"**For lowest latency, run the node in Tokyo, Japan**";非验证节点 16 vCPU / 128 GB / 500 GB SSD,验证人 32 vCPU / 1 TB;仅 Ubuntu 24.04;4001/4002 端口须公网可达(否则被 peer 降权);默认 ~100 GB 日志/天;"延迟优化" 页建议 ≥32 逻辑核、128 GB、500 MB/s 磁盘(核越多块执行越快)。
- **基金会非验证节点跑在 apne1-az1**;接入需 **质押 10 000 HYPE + Maker 返佣 Tier 1(>0.5% 14 日加权 maker 量)+ 98% 在线**。
- [自测] `gossipRootIps` 返回 24 个 IP,19 个在 ap-northeast-1(AWS ip-ranges 2026-09-28);README 社区种子表 25 个 peer 中 21 个标 "Japan"。`api.hyperliquid.xyz` 经 CloudFront。
- HyperBFT(HotStuff 变体):官方 "同城客户端端到端(发出→提交回执)**中位 0.2 s、p99 0.9 s**,~20 万单/s";订单优先级逻辑按 **70 ms** 时间桶("通常对应 1 块"),提交 "通常 2 块";cancel/ALO 端到端 ~380 ms 但排序在 <10 ms 内可预测;验证人需对 ≥1/3 权益 200 ms 双向延迟。
- API server 监听节点、维持状态、把交易转给节点 gossip,**提交后才回应**。

### 8.2 自建节点 vs API server [官方]

- 节点输出 "比 API 更快更细":`--write-fills`(API fills 格式)、`--write-order-statuses`、`--write-raw-book-diffs`、`--write-trades`、`--write-misc-events`、`--batch-by-block` / `--stream-with-block-info`、`--replica-cmds-style`、**`--disable-output-file-buffering`(逐行 flush 降延迟)**、`--serve-eth-rpc`、`--serve-info`(localhost:3001/info,仅本地状态可算的请求,**无 l2Book、无 WS**)。
- **`split_client_blocks: true`**(`~/override_gossip_config.json`):把未提交的 mempool 交易流到 `~/hl/data/mempool_txs/`,"**70–150 ms** 提前看到输入"(无执行结果,且上游 peer 都要开)。
- **gossip 优先拍卖**(读侧):3 分钟周期两轮荷兰拍,最低 0.1 HYPE、每次归零到上次成交价 ×10,"每槽 ~25 ms 改善";[自测 01:44 UTC] slot0 价格 15.39→2.30 HYPE 衰减中,slot1 1.0→0.23。
- **下单优先费**(写侧):action 里 `{"p": N}`;IOC 在 0–8 bp 线性提升 "~45 ms/bp",≥8 bp 只在 70 ms 桶内破平;ALO 优先级在 400 ms 窗口内重排队列位置;费用销毁。

### 8.3 API 机制 [官方]

- 限速:IP 1200 weight/min(exchange 权重 `1+floor(batch/40)`;info l2Book/allMids/clearinghouseState/orderStatus 权重 2,userRole 60,其它 20);WS **10 连接、30 新建/min、1000 订阅、10 个用户、2000 入向 msg/min、100 个在途 post**;**地址级:累计每成交 1 USDC 换 1 个 action,初始缓冲 10 000**,超限后 1 请求/10 s,cancel 额外 `min(limit+100000, 2×limit)`,批量 n 单计 n;挂单上限 1000(+1/每 500 万 USDC 成交,上限 5000);拥堵时按前一日 maker 份额 ×2 分配块空间;`reserveRequestWeight` 以 **0.0005 USDC/个** 买额度。
- Exchange:订单 `a,b,p,s,r,t{limit{tif:Alo|Ioc|Gtc}|trigger},c(cloid 128-bit)`,`grouping na|normalTpsl|positionTpsl`,`builder{b,f}`;响应 `statuses:[{resting:{oid}}|{filled}|{error}]`;`cancel{a,o}` / `cancelByCloid` 可带 `f:true`(fast);`scheduleCancel{time}`(≥5 s 后,≤10 次/日);`modify` / `batchModify`;`noop` 烧 nonce("撤在途单更有效");**`expiresAfter`(ms)**,陈旧拒单按 5× 计入地址限速;`approveAgent` 1 匿名 + 3 命名 API 钱包(≤180 天)。
- Nonce:每签名者保留 100 个最高 nonce,须大于最小且未用,窗口 **(T−2 天, T+1 天)**;建议每进程一个 API 钱包、每 0.1 s 批量发单/撤单、ALO 批次单独发("验证人优先处理纯 ALO 批")。
- 签名:`keccak(msgpack(action) ‖ nonce ‖ vault ‖ expiresAfter)` → phantom agent → EIP-712(域 "Exchange",chainId 1337)→ 一次 secp256k1 签名/批。
- WS:`wss://api.hyperliquid.xyz/ws`,`{"method":"post","id":N,"request":{"type":"action"|"info","payload":...}}` → `channel:"post"`,**WS 下单可用**;60 s 无消息断;`{"method":"ping"}`。

### 8.4 行情 [官方 + 自测]

| 订阅 | 观察到的节拍(40–45 s 窗口,BTC/ETH) |
|---|---|
| `bbo`("只在某块 BBO 变化时推") | 6.9 msg/s;间隔 min 43 ms、p5 57、**p50 108 ms**、p95 414 ms |
| `l2Book`(默认 20 档) | **~5.3 s 一帧**(min 4.87 s) |
| `l2Book fast:true`(5 档) | ~0.54 s(min 454 ms) |
| `trades` | 间隔 min 43 ms |

公开 `l2Book` 被严重限流;块级深度只能自建节点(`--write-raw-book-diffs` + L4 快照)或第三方(QuickNode 基准 aws-nrt-01、Hydromancer 等)。历史数据 `s3://hyperliquid-archive`、`s3://hl-mainnet-node-data`(requester pays)。

### 8.5 CEX–DEX perp 套利结构

- 资金费每小时结(8 h 公式的 1/8),premium 每 5 s 采样对比验证人 oracle(加权中位 CEX 现货),夹在 ±0.05%,上限 4%/h;费率 taker 0.045%→0.024%、maker 0.015%→0%、maker 返佣 −0.001/−0.002/−0.003%(份额 >0.5/1.5/3%)。
- 撮合顺序 "非订单动作 → cancel → GTC/IOC","t 时刻发出的 cancel 与 ALO 几乎总是先于 t 时刻的 IOC/GTC 执行",官方明说是 "保护做市方免受由其它场所价格驱动的有毒流"。
- 推断(待核实):主流结构 = HL 上 ALO 做市吃返佣 + Binance 上 taker 对冲;当 Binance 领先时用 IOC 打 HL。东京部署的价值在于 (i) `split_client_blocks` 提前 70–150 ms 看到对手输入,(ii) 把 ~200 ms 端到端压到最低,(iii) 用优先费买排序。Glassnode "Tokyo 200 ms edge" 原文未定位到,数字以官方 "中位 0.2 s" 为准。

---

## 9. Deribit / Coinbase International / Kraken(简述)

- **Deribit**:标准 API = JSON-RPC over `wss://www.deribit.com/ws/api/v2` + HTTP + FIX(`fix.deribit.com:9881/9883`);响应 **`usIn / usOut / usDiff`(µs)** [自测确认,`usDiff` 67–89 µs];限速为信用制(非撮合 500 credit/req、上限 50 000、回补 10 000/s;撮合类按 7 日成交 >2500 万 $ 30 req/s … ≤100 万 $ 5 req/s;32 连接/IP、16 会话/key);认证 OAuth2 或 `client_signature`(HMAC-SHA256,60 s),**支持 Ed25519(推荐)/RSA 非对称 key**;行情 `book.{inst}.raw`(逐变化)/ `100ms` / `agg2`;**新 "Starbase" 撮合(SBE 下单、组播行情、FIX drop copy)只接受 LD4 hosted colo / cross-connect / AWS PrivateLink,"不支持互联网接入"**;INTX 2026-10-01 并入 Deribit。从东京 ~270 ms,只适合期权/低频。
- **Coinbase International Exchange**:官方 ap-northeast-1 **apne1-az1**;FIX 5.0(OE `fix.international.coinbase.com:6110`、MD 6120、Drop Copy 6130,TLS 1.2,无 resend,周六重置序列)、REST `api.international.coinbase.com/api/v1`、WS `wss://ws-md.international.coinbase.com`(JSON,3 s 内必须订阅);限速 4000 msg/s/账户、FIX 1 连接 & 800 msg/s/key(1000 断)、REST 40 req/s/key、WS 10 次连接/30 s、30 交易 key、5 drop copy;仅非美用户,日本实体资格待核实;**2026-10-01 并入 Deribit(伦敦)** → 不值得投入。
- **Kraken**:Spot 与 Derivatives 引擎在 **Equinix London**;官方分层:cross-connect ~200 µs、Beeks 同园区 ~200 µs、AWS London eu-west-2 ~2 ms、BSO/Avelacom 私网 **Tokyo↔London ~140 ms**;WS v2 `add_order / amend_order`(响应 `time_in / time_out` µs)、FIX 4.4(粘性网关路由);限速为衰减计数器(REST 15/20/20,撮合按币对计数,cancel 按订单年龄加罚)。东京不可用作低延迟腿。

---

## 10. 跨交易所对比表

| 交易所 | 位置 / AZ(2026-09) | 前置 | 最快 MD 通道 & 服务端节拍 | 最快 OE 通道 | 登录后免逐单签名 | 默认下单限速 | 引擎/网关时间戳 | 过期保护 | MM / 低延迟计划 | [自测] MD 单向 p50 / OE 往返下界 |
|---|---|---|---|---|---|---|---|---|---|---|
| Binance USDⓈ-M | apne1(az4 假说) | 直连 EC2 | `bookTicker` 实时;`depth@100ms`(无 @0ms);`aggTrade` 100 ms | WS API `ws-fapi`(Ed25519 logon) | **是** | 300/10 s、1200/min /账户;2400 weight/min/IP | 行情 `T`;订单只有 `updateTime` | `recvWindow`(≤5000 建议) | FAPI-MM / FSTREAM-MM(LP 计划) | 2.2 ms / 2.25 ms |
| Binance spot | 同上 | 直连(`api.binance.com` 例外) | SBE `bestBidAsk`/`trade` 实时、SBE depth 20 ms | WS API `ws-api`(logon)/ FIX OE | **是** | 100/10 s、20 万/日 /账户;6000 weight/min/IP | `transactTime`、`workingTime`;SBE µs | `recvWindow` | Spot MM Program(无专用主机) | ~4 ms(Deltix 2023)/ 2.5 ms |
| OKX | 东京(云/AZ 未披露) | Cloudflare | `bbo-tbt` 10 ms;`books-l2-tbt` 10 ms(VIP4+);SBE 通道 | WS `order`(`instIdCode`) | **是**(HMAC 登录) | 60/2 s /UID+instId;子账户 1000/2 s → 10 000 | `inTime/outTime` µs | **`expTime`** | 无 colo;MM=GLP(VIP2+) | 3.6 ms / ~6.4 ms(ping) |
| Bybit | SG → **apne1-az4 @ 11-12** | CloudFront | `orderbook.1` 10 ms、`.50` 20 ms;`cts` | WS Trade `/v5/trade` | **是**(HMAC/RSA 登录) | 10/s(20/s spot)/UID;PRO 至 1200/s | `cts`;SBE ack µs | `recv_window` | MMWS/MMGW/SBE(机构,"不保证更低延迟") | 36 ms → 预期 1–3 ms / 70 ms → ~2 ms |
| Bitget | 东京(推断) | WS CloudFront / REST Cloudflare | UTA `books1` **1 ms**、`books5` 10 ms、`books` 50 ms | UTA V3 WS place/batch/modify/cancel | **是**(HMAC 登录) | 10/s → VIP 100/s;MM 600/s | (待核实) | (待核实) | MM/PRO 机构限速;"LOLA"/SBE 未找到 | 6.5 ms(p99 140)/ — |
| Gate | **apne1(官方)**,LB 三 AZ | 直连 ELB | `book_ticker` 10 ms;`obu` 20 ms;futures `order_book_update` 20 ms;**SBE** | WS API `order_place`(spot/futures) | **是**(HMAC-SHA512 登录) | spot 10 r/s;futures 100 r/s /UID | `x_in_time/x_out_time` µs | **`x-gate-exptime`** | mm@gate.com;VIP14+ fill-ratio | 0.38 ms RTT 同 AZ / — |
| KuCoin | 东京(az4 假说) | WS CloudFront / REST Cloudflare | `level1` 10 ms;`level2` 实时;`level2Depth50` 100 ms | Pro WS `wsapi.kucoin.com`(uta.order) | **是** | UTA 300/s → 4000/s;Classic 4000 权重/30 s | `inTime/outTime`(Colo 用户 ns) | **`allowMaxTimeWindow`** | MM 计划含 **DMA + Co-location**(≥3000 万 $/30 d) | — |
| Hyperliquid | 东京;基金会节点 apne1-az1 | API CloudFront;节点直连 | 自建节点块级;公开 `bbo` 逐块(~100 ms)、`l2Book` 限流 | `/exchange` 或 WS `post`;ALO 优先 | 否(每 action 一签,批量一签) | 1200 weight/min/IP;地址 1 请求/1 USDC | 块时间;`statuses` | **`expiresAfter`** | 自建节点;gossip/下单优先费 | 端到端中位 0.2 s(官方) |
| Deribit | 伦敦 LD4 | Cloudflare | `book.raw` 逐变化 | JSON-RPC WS / FIX / Starbase SBE(colo) | 是(OAuth token) | 信用制;撮合 5–30 req/s | `usIn/usOut/usDiff` | `valid_until` | LD4 colo / PrivateLink | ~270 ms 往返 |
| Coinbase INTX | **apne1-az1(官方)** | 直连 | WS JSON 实时 / FIX MD | FIX 5.0 / REST | FIX 会话 | 800 msg/s/key,40 REST/s | — | — | 无;**10-01 并入 Deribit** | 0.49 ms RTT 同 AZ 节点 |
| Kraken | Equinix London | Cloudflare | WS v2 L3 | WS v2 `add_order` / FIX 4.4 | token | 衰减计数器 | `time_in/time_out` µs | — | cross-connect ~200 µs(NDA) | ~140 ms+ |

---

## 11. 推荐接入方案

### 11.1 机房与网络(先做,收益最大)

1. **两周内完成 AZ 判定实验**:在 apne1-az1 与 apne1-az4 各起一台 spot c7g.large,运行 `_src/probe2.py`(全部 A 记录的 TCP 握手 RTT),Binance 任一主机名出现 ~0.3 ms 节点的 AZ 即为 Binance 所在 AZ。预期 az4(Bybit 官方目标 + AWS 事故记录 + KuCoin);若为 az1 则 az1 同时是 INTX 与 Hyperliquid 基金会节点所在,同样迁。
2. **主机迁到该 AZ**,Gate 从 0.27 ms 变 ~1 ms 可接受(Gate 不是锚点);Bybit 11-12 之后与 Binance 同 AZ 是最大红利。
3. **实例**:引擎独占物理核(c7gn / c8g / c7i 家族,或 .metal),`ethtool -C adaptive-rx off rx-usecs 0`、IRQ 与引擎分核、busy_poll;开 **PTP 硬件时钟 + hardware packet timestamps**(Nitro,µs 级)以便所有延迟统计以硬件收包时间为准(细节见 01 号笔记)。
4. **REST 主机**:Binance 用 `api1–api4.binance.com`(直连)而非 CloudFront 的 `api.binance.com`;OKX 用 `openapi.okx.com`;Bybit 可同时配置 `api.bytick.com` 作备域。
5. **DNS 钉选**:对直连 ELB 的交易所(Binance、Gate、INTX)解析全部 A 记录、逐个测 RTT、钉住最快节点(Gate 的同 AZ 节点比跨 AZ 节点快 1 ms);Binance 目前所有节点等距,迁 AZ 后重测。

### 11.2 每个交易所的通道矩阵

| 交易所 | 行情主通道 | 行情校验通道 | 下单通道 | 会话 / 签名 | 必开的保护 |
|---|---|---|---|---|---|
| Binance futures | `/public` 上 `<sym>@bookTicker`(每连接 ≤10 个流,2–3 条连接赛马按 `u` 去重) | `<sym>@depth@100ms`(pu 链续)+ `/market` 上 `@aggTrade`、`@markPrice@1s`、`@forceOrder` | `ws-fapi` `session.logon`(Ed25519)→ `order.place`;主备两条已 logon 连接;REST `fapi/v1/order` 仅兜底 | logon 后免签 | `recvWindow=2000`;`returnRateLimits` 闭环限速;监听 `serverShutdown`;23 h 主动轮换 |
| Binance spot | SBE `@bestBidAsk` + `@trade`(需要 µs 时间戳与 auto-culling)或 JSON `bookTicker`(若解析栈更成熟) | SBE `@depth`(20 ms)| `ws-api` logon → `order.place` / `order.amend.keepPriority`;FIX OE 作为第二阶段(10 会话/账户、10 000 msg/10 s) | logon 后免签 | 同上 |
| OKX | `bbo-tbt`(JSON 或 SBE,登录即可) | `books-l2-tbt`(需 VIP4,否则 `books50-l2-tbt`/`books` 100 ms)+ `trades`;`seqId/prevSeqId` 校验(checksum 已废) | `/private` 登录 → `order` / `batch-orders`(`instIdCode`);amend 在途 ≤3 | 登录后免签 | **`expTime = now+1500 ms`**;`rpiTakerAccess=false`;监听 64008 升级预告;每子账户 ≤30 私有连接 |
| Bybit | `orderbook.1`(10 ms,`cts`) | `orderbook.50`(20 ms)+ `publicTrade`;`execution.fast` 收成交 | `/v5/trade` `order.create` / `order.create-batch`(linear 20) | 登录后免签(HMAC 或 RSA) | `recv_window=2000`;读 `X-Bapi-Limit-Status`;20 s ping;每 5 min ≤500 新连接 |
| Bitget | UTA `books1`(1 ms) | UTA `books5`(10 ms)/ `books`(50 ms,`seq/pseq`)+ New-Trades | UTA V3 WS `place-order` / `batch-place`(限额待核实,先按 10/s 设计) | 登录后免签 | **无条件 30 s 应用层 `ping`**;`pxAmendType` 明确设置 |
| Gate | `spot.book_ticker`(10 ms)/ `futures.book_ticker`;有条件上 spot SBE | `spot.obu` 50 档 20 ms / `futures.order_book_update` 20 ms | WS API `spot.order_place` / `futures.order_place`,`action_mode=ACK` | 登录后免签(HMAC-SHA512) | **`x-gate-exptime`**;监控 `x_in_time/x_out_time`;≤300 连接/IP |
| KuCoin | `level1`(10 ms) | `level2` 实时增量 + `level2Depth50` 校验 | Pro WS `wsapi.kucoin.com`(`uta.order`),`enable_ns=true` | 登录后免签 | **`allowMaxTimeWindow`**;18 s ping;bullet token 24 h 刷新 |
| Hyperliquid | 第一阶段:公开 WS `bbo` + `trades`(API 经 CloudFront);第二阶段:**自建非验证节点(apne1 同 AZ,16 vCPU/128 GB)** `--write-raw-book-diffs --write-fills --disable-output-file-buffering` + `split_client_blocks` | `l2Book fast:true`(0.5 s)仅作校验 | WS `post` action;ALO 挂单 + IOC 吃单分开成批,每 0.1 s 一批;`cancelByCloid f:true` | 每 action 一次 EIP-712(~0.1 ms);每进程一个 API 钱包 | **`expiresAfter`**;nonce 单调;地址级额度监控(`userRateLimit`);必要时 `reserveRequestWeight` |

### 11.3 连接与时钟纪律

- 每交易所:≥2 条行情连接(相同订阅、按序列号去重)、≥2 条已登录的下单连接(主备)、1 个 REST keep-alive 池(账户/仓位查询、紧急撤单);全部错峰重连,永不在热路径建连。
- 时钟:chrony → 169.254.169.123 + PHC(`/dev/ptp0`),目标 <50 µs;所有 "交易所→本地" 统计以交易所撮合时间(`T`、`cts`、`inTime/outTime`、`usIn/usOut`、Gate `x_in_time`)为锚,并区分 "事件时间"(网关)与 "撮合时间"。
- 限速:按 IP 的配额用第二块 ENI/EIP 翻倍(Binance weight/连接数、Bybit 600/5 s、Hyperliquid 1200/min);按 UID 的下单配额靠子账户(OKX 子账户 1000/2 s 起、Bybit 每 UID 10/s、KuCoin UTA 300/s)。
- 观测:每条连接记录 TLS/WS 握手时间、应用 ping RTT、消息间隔直方图、`inTime/outTime` 网关停留;Bitget 重尾、Binance 突发、OKX Cloudflare 尖峰各自建基线。

### 11.4 专线与计划申请(并行推进,"免费" 的 p50/p99)

1. Binance:进入 USDⓈ-M Futures Maker 或 Taker Program → VIP Portal 申请 FAPI-MM + FSTREAM-MM 白名单(注意 IP 配额与低成交量移除规则);目标是脱离共享网关,预期收益需实测(官方无数字)。
2. KuCoin:达到 30 日 3000 万 USDT 门槛后申请 MM 计划,索取 **DMA / co-location** 细节与 ns 级 Colo UTA 通道。
3. Gate:mm@gate.com 申请 MM;确认 futures 是否有 SBE、WS 连接上限是否可放宽。
4. Bybit:11-12 迁移后评估 MMWS(稳定性)与 SBE 下单(µs ack);官方不承诺延迟,先测再谈。
5. OKX:目标 VIP4(解锁 `books-l2-tbt`/SBE 逐笔)、VIP5(fill-ratio 限速放宽);没有专线可申请。
6. Bitget:向机构团队确认 "LOLA" 与 SBE 是否存在;申请 MM/PRO 限速。
7. Hyperliquid:先自建非验证节点;当 maker 份额达 0.5% 时申请基金会节点接入;评估 gossip 优先拍卖的性价比(当前 0.2–2 HYPE/槽)。

### 11.5 不建议投入的场所

- Deribit / Kraken:伦敦引擎,东京 ≥140–270 ms;只能做低频对冲或期权,不进入延迟敏感路径。
- Coinbase INTX:虽在 apne1-az1,但 2026-10-01 并入 Deribit,端点即将消失。

### 11.6 待核实清单(按影响排序)

1. Binance 所在 AZ(实验判定,见 11.1);Binance futures 是否仍接受 `@depth@0ms`(直接订阅测试,文档已无)。
2. Bitget UTA WS 下单的每频道限速、批量上限、响应时间戳、STP;"LOLA" 与 SBE 是否存在。
3. OKX 迁移后所在云/AZ(Cloudflare 遮蔽,只能靠 `inTime−发送时间` 推断)与 REST 14 ms 的来源(Cloudflare 回源 vs 源站跨 AZ)。
4. Binance `ws-fapi-mm` / `dstream-mm` 主机是否属于 FAQ 的服务范围;futures REST 是否接受 Ed25519。
5. Gate futures SBE;KuCoin 时间戳窗口;Hyperliquid 自建节点 fills 相对 API `userFills` 的实际提前量;Glassnode 文章原文。

---

## 12. 参考链接

**官方文档 / 公告**
- OKX 迁移公告(2026-05-14):https://www.okx.com/en-us/help/okx-trading-server-migration-announcement-hong-kong-tokyo
- OKX API v5 文档 / changelog:https://www.okx.com/docs-v5/en/ · https://www.okx.com/docs-v5/log_en/ · checksum 废弃:https://www.okx.com/help/okx-order-book-channels-checksum-field-deprecation · fill-ratio 限速:https://www.okx.com/help/fill-ratio-sub-account-rate-limit
- Bybit 迁移公告(2026-09-02,apne1-az4):https://announcements.bybit.com/en/article/bybit-trading-server-migration-from-aws-singapore-to-aws-tokyo--art853451a43a75/
- Bybit v5 文档:连接 https://bybit-exchange.github.io/docs/v5/ws/connect · orderbook https://bybit-exchange.github.io/docs/v5/websocket/public/orderbook · WS Trade https://bybit-exchange.github.io/docs/v5/websocket/trade/guideline · 限速 https://bybit-exchange.github.io/docs/v5/rate-limit · PRO https://bybit-exchange.github.io/docs/v5/rate-limit/rules-for-pros/introduction · MMWS https://bybit-exchange.github.io/docs/v5/institution/connection/mmws · MMGW https://bybit-exchange.github.io/docs/v5/institution/connection/mmgw · SBE https://bybit-exchange.github.io/docs/v5/sbe/sbe-basic-info · FAQ(新加坡 AZ)https://bybit-exchange.github.io/docs/faq · SBE 下单公告 https://announcements.bybit.com/en-US/article/introducing-sbe-order-entry-a-high-performance-order-channel-for-institutional-traders-bltc00abb851098c0a6/
- Binance Futures Low-Latency API FAQ:https://www.binance.com/en/support/faq/what-are-binance-futures-low-latency-api-services-7df7f3838c3b49e692d175374c3a3283(本地 `_src/binance_ll.txt`)
- Binance spot 文档仓库:https://github.com/binance/binance-spot-api-docs(web-socket-api.md、sbe-market-data-streams.md、fix-api.md、rest-api.md、web-socket-streams.md、CHANGELOG.md、faqs/api_key_types.md)· 期货 changelog:https://developers.binance.com/docs/derivatives/change-log · 期货 WS API:https://developers.binance.com/docs/derivatives/usds-margined-futures/websocket-api-general-info · 旧镜像 https://binance-docs.github.io/apidocs/futures/en/
- Bitget:UTA 限速 https://www.bitget.com/docs/uta/rate-limit/rate-limit · 机构限速 https://www.bitget.com/docs/uta/rate-limit/institutional/institutional-rate-limit · UTA 深度频道 https://www.bitget.com/docs/uta/websocket/public/Order-Book-Channel · changelog 2026-09 https://www.bitget.com/docs/uta/changelog/2026-09 · Classic WS https://www.bitget.com/docs/classic/websocket/intro
- Gate:REST(含 "Data Center"、限速、MM 申请)https://www.gate.com/docs/developers/apiv4/en/ · spot WS(含 SBE、WS API)https://www.gate.com/docs/developers/apiv4/ws/en/ · futures WS https://www.gate.com/docs/developers/futures/ws/en/ · SDK https://github.com/gateio/gatews · https://github.com/gateio/gateapi-python
- KuCoin:WS 介绍 https://www.kucoin.com/docs-new/websocket-api/base-info/introduction · UTA WS https://www.kucoin.com/docs-new/websocket-api/base-info/introduction-uta · 限速 https://www.kucoin.com/docs-new/rate-limit-rule-classic · https://www.kucoin.com/docs-new/rate-limit-rule-uta · Pro WS 下单 https://www.kucoin.com/docs-new/3470344w0 · MM 计划 https://www.kucoin.com/docs-new/user-service/market-making-incentive-scheme · SDK https://github.com/Kucoin/kucoin-universal-sdk
- Hyperliquid:node README https://github.com/hyperliquid-dex/node · 基金会节点 https://hyperliquid.gitbook.io/hyperliquid-docs/for-developers/nodes/foundation-non-validating-node · 延迟优化 https://hyperliquid.gitbook.io/hyperliquid-docs/for-developers/api/optimizing-latency · 优先费 https://hyperliquid.gitbook.io/hyperliquid-docs/for-developers/api/priority-fees · 限速 https://hyperliquid.gitbook.io/hyperliquid-docs/for-developers/api/rate-limits-and-user-limits · exchange https://hyperliquid.gitbook.io/hyperliquid-docs/for-developers/api/exchange-endpoint · nonce https://hyperliquid.gitbook.io/hyperliquid-docs/for-developers/api/nonces-and-api-wallets · WS post https://hyperliquid.gitbook.io/hyperliquid-docs/for-developers/api/websocket/post-requests · 订阅 https://hyperliquid.gitbook.io/hyperliquid-docs/for-developers/api/websocket/subscriptions · HyperCore https://hyperliquid.gitbook.io/hyperliquid-docs/hypercore/overview · order book https://hyperliquid.gitbook.io/hyperliquid-docs/hypercore/order-book · API servers https://hyperliquid.gitbook.io/hyperliquid-docs/hypercore/api-servers · 资金费 https://hyperliquid.gitbook.io/hyperliquid-docs/trading/funding · 费率 https://hyperliquid.gitbook.io/hyperliquid-docs/trading/fees · Python SDK signing https://github.com/hyperliquid-dex/hyperliquid-python-sdk
- Coinbase INTX:Systems & Operations(apne1-az1)https://docs.cdp.coinbase.com/international-exchange/introduction/systems-operations · 限速 https://docs.cdp.coinbase.com/international-exchange/introduction/rate-limits-overview · FIX https://docs.cdp.coinbase.com/international-exchange/fix-api · WS https://docs.cdp.coinbase.com/international-exchange/websocket-feed
- Deribit:https://docs.deribit.com/ · 限速 https://docs.deribit.com/articles/rate-limits · 非对称 key https://docs.deribit.com/articles/asymmetric-api-keys · 连接管理/Starbase https://docs.deribit.com/articles/connection-management-best-practices · INTX 迁移 https://docs.deribit.com/articles/intx-api-migration
- Kraken:Unlocked #5 https://blog.kraken.com/product/api/unlocked-5-ultra-low-latency-trading · colocation https://docs.kraken.com/exchange/guides/general/colocation · WS v2 add_order https://docs.kraken.com/api/docs/websocket-v2/add_order · 限速 https://docs.kraken.com/api/docs/guides/spot-ratelimits
- AWS:shared CPG https://aws.amazon.com/blogs/industries/crypto-market-making-latency-and-amazon-ec2-shared-placement-groups/ · One Trading https://aws.amazon.com/blogs/industries/one-trading-exchange-and-aws-cloud-native-colocation-for-crypto-trading/ · Avelacom https://aws.amazon.com/blogs/industries/ultra-low-latency-cross-region-crypto-trading-with-avelacom-and-aws/ · PTP placement group https://aws.amazon.com/about-aws/whats-new/2026/06/ec2-time-sync-precision-time-placement-group/ · 硬件收包时间戳 https://aws.amazon.com/about-aws/whats-new/2025/06/amazon-time-sync-nanosecond-hardware-packet-timestamps/ · ip-ranges https://ip-ranges.amazonaws.com/ip-ranges.json · CloudFront WebSocket https://docs.aws.amazon.com/AmazonCloudFront/latest/DeveloperGuide/distribution-working-with.websockets.html · Cloudflare WebSocket https://developers.cloudflare.com/network/websockets/

**社区 / 厂商测量**
- DCD,2025-04-15 AWS 东京 apne1-az4 事故:https://www.datacenterdynamics.com/en/news/network-issue-at-aws-data-center-brings-down-crypto-exchanges/
- Deltix/Ember,Binance WS 行情延迟(2023,apne1-az1 pcap):https://ember.deltixlab.com/docs/performance/ws-market-data/
- Viktoria Tsybko,Binance 多 Region 订单往返(2023-10):https://viktoriatsybko.substack.com/p/an-analysis-of-binance-exchange-across · KuCoin/HTX/Gate(2023-11):https://viktoriatsybko.substack.com/p/a-latency-analysis-of-kucoin-htx
- NeutK/binance_feed_test(Tokyo 多连接赛马):https://github.com/NeutK/binance_feed_test · sydowma/test_exchange_latency:https://github.com/sydowma/test_exchange_latency · hftbacktest latency models:https://github.com/nkaz001/hftbacktest
- Arbitron,交易所位置与 REST 延迟(2026-09-22):https://arbitron.app/learn/crypto-exchange-server-locations · Zenlayer,Tokyo 加密延迟(2026-07):https://cloud.zenlayer.com/blog/crypto-trading-latency-tokyo · Glassnode Latency Monitor:https://hyperlatency.glassnode.com/ · longcipher:https://book.longcipher.com/en/blog/aws-tokyo-digital-wall-street/ · Tardis Bitget 采集说明:https://docs.tardis.dev/historical-data-details/bitget
- OKX 位置(迁移前社区 README):https://raw.githubusercontent.com/njv74841/okx-api-server-location/main/README.md · tiagosiebler/binance(MM 子域映射):https://github.com/tiagosiebler/binance · tiagosiebler/bitget-api:https://github.com/tiagosiebler/bitget-api
- AZ 间延迟:https://xkyle.com/Measuring-AWS-Region-and-AZ-Latency/ · https://www.flashgrid.io/news/latencies-between-aws-availability-zones-what-are-they-and-how-to-minimize-them/

**本次自测脚本与原始输出(`E:\trading\architecture-v2\research\_src\`)**:`probe.py` / `probe_result.txt`(端点 RTT + IP 分类)、`probe2.py` / `probe2_result.txt`(逐 A 记录 RTT)、`ws_rtt.py` / `ws_rtt_result.txt`(WS 握手与应用往返)、`okx_docs.txt`(OKX 文档全文 1.8 MB)、`okx_mig.txt`、`bybit_mig.txt`、`binance_ll.txt`、`intx_*.txt`、`kucoin_*.txt`、`hl_node_readme.txt`、`tsybko_*.txt`;抓取工具 `grab.py`(本机 curl)与 `rgrab.py`(经东京 EC2 ssh-curl)。
