---
component: venue-binance
prefix: BIN
spec: README.md
---

# venue-binance 测试规格

格式与规则见 [docs/tests](../../docs/tests/README.md)。本文件的记号(各用例在 `前置` 中引用;数值即定义,不依赖默认值):

- `RD-BIN` = 测试 refdata(`tests/support/mod.rs::rd()` 以 `RdbBuilder` 构建;inst id 从 1 稠密,未列出的 id 是 `BINANCE_USDM` 上名为 `F<id>USDT` 的填充 inst):资产 `USDT` = AssetId 1(`qty_exp = −8`)、`BTC` = 2(`px_exp = −2`,`qty_exp = −8`)、`ETH` = 3(`px_exp = −2`,`qty_exp = −8`)、`BNB` = 4(`px_exp = −3`,`qty_exp = −8`);`BINANCE_USDM`(1):`BTCUSDT` = InstId 1042(tick 0.1,lot 0.001)、`ETHUSDT` = 1043(tick 0.01,lot 0.001);`BINANCE_SPOT`(2):`BTCUSDT` = 2001(tick 0.01,lot 0.00001)、`BNBUSDT` = 2002(tick 0.001,lot 0.001);合约平移全为 0;账户 `AccountId 2` 在 `BINANCE_USDM`、`AccountId 1` 在 `BINANCE_SPOT`(一个账户在一个交易所只用一个市场),两者 `venue_uid = "acct-0001"`;两个 venue 的生效 `VenueCaps` 等于 `default_caps()`(BIN-171)、限速等于 `rate_rules()`(BIN-170)。数值写成"十进制(raw)":`65000.1` 价格 = raw 6500010,`0.004` BTC = raw 400000,`1` USDT 名义 = `Notional` raw 1000000,`Rate` 1.0 = raw 10⁹。
- `RX` = `RxMeta{rx_tsc: 1000, hw_rx_ns: 1790000000126000000, sw_rx_ns: 1790000000126500000, prev_override: 0, conn: 3, account: 2, ts_flags: 0, rx_empty_gap_ns: 2500}`。
- `WC` = `WallClock{unix_ms: 1790000000000, unix_us: 1790000000000000}`;`gw.stale_guard_ms = 1500`(经 `VenueCfg.stale_guard_ms` 交给 `build_templates`);除注明外 `Templates` 以 `build_templates(cfg, rd, 2, &HM1)` 构建。
- `CID(e, s, g)` = `ClientOrderId{epoch: e, venue: <本产品线>, account: 2, slot: s, gen: g}` 的规范 24 位文本:`Usdm` 的 `CID(3,42,7) = 1000301020002a000000073d`、`CID(3,43,1) = 1000301020002b000000013a`、`CID(2,42,7) = 1000201020002a000000072d`;`Spot` 的 `CID(3,42,7) = 1000302020002a000000070d`。
- `ED1` = Ed25519 测试凭证(RFC 8032 测试 1 私钥 `9d61b19deffd5a60ba844af492ec2cc44449c5697b326919703bac031cae7f60`),`apiKey = "TESTKEY"`;`HM1` = HMAC REST 测试密钥 `NhqPtmdSJYdKjVHjA7PZj4Mge3R5YNiP1e3UZjInClVN65XAbvqqM6A7H5fATj0j`(Binance 文档示例),`apiKey = "TESTKEY"`。二者都是公开测试向量,不是真实密钥。
- `BT1` = `{"e":"bookTicker","u":400900217,"s":"BTCUSDT","b":"65000.1","B":"1.500","a":"65000.2","A":"0.004","T":1790000000123,"E":1790000000125}`(`Usdm`,毫秒)。
- `SBT1` = `{"u":400900217,"s":"BTCUSDT","b":"65000.10000000","B":"0.50000000","a":"65000.11000000","A":"0.25000000"}`(`Spot`)。
- `DP5` = `{"e":"depthUpdate","E":1790000000301,"T":1790000000300,"s":"BTCUSDT","U":7000000000,"u":7000000001,"pu":6999999999,"b":[["65000.1","1.000"],["65000.0","2.000"],["64999.9","0.500"],["64999.8","0.100"],["64999.7","3.000"]],"a":[["65000.2","0.400"],["65000.3","1.200"],["65000.4","0.050"],["65000.5","2.500"],["65000.6","0.010"]]}`(`Usdm` `public-topn` 路由)。
- `SDP5` = `{"stream":"bnbusdt@depth5@100ms","data":{"lastUpdateId":160,"bids":[["600.100","1.000"],["600.000","2.000"],["599.900","0.500"],["599.800","0.100"],["599.700","3.000"]],"asks":[["600.200","0.400"],["600.300","1.200"],["600.400","0.050"],["600.500","2.500"],["600.600","0.010"]]}}`(`Spot` combined 流)。

可选扩展(增量深度、REST 深度快照与 `align_first`、现货 SBE、`mock` 反向编解码、`order.cancelReplace`)的用例不在本文件;见"验收"。

CORE-120 非正式账户启动链路由 `apps/hft-core/tests/account_process.rs::core_120_mock_accounts_bootstrap_reauth_and_drift` 以独立子进程、两家真实协议和脚本 REST 验证撤空、基线、重新核验、超时/错误模式失败与余额差异不覆盖账本；不替代真实 TLS、交易所或正式恢复验收。资金费账单复用 FundingPoll，沿用现有实际结算记账与去重。

## 覆盖

| README 小节 | 用例 |
|---|---|
| 测试环境 warm 账户入口 | BIN-142、BIN-145 |
| 资金费实额 | BIN-332 |
| 概要 | BIN-001, BIN-038, BIN-210 |
| 目录 | 不适用:非规范 |
| 使用本组件 | BIN-001, BIN-078, BIN-079 |
| 端点与路由 | BIN-002–BIN-007, BIN-215 |
| 行情频道 → 事件 | BIN-020–BIN-025, BIN-030–BIN-034, BIN-037, BIN-300, BIN-302–BIN-304, BIN-320, BIN-322 |
| 下单:WS API | BIN-013, BIN-060–BIN-077, BIN-081, BIN-090, BIN-094, BIN-096, BIN-307–BIN-311, BIN-324–BIN-326 |
| 限速默认值(`rate_rules()`,生效值以 refdata 为准) | BIN-091, BIN-170 |
| 分档快照流 | BIN-002, BIN-003, BIN-027–BIN-029, BIN-201, BIN-301, BIN-321 |
| L2 序号归一化(`L2Batch.prev_id/last_id`) | 不适用:里程碑外 |
| 累计量与费用来源 | BIN-094, BIN-101–BIN-105, BIN-111, BIN-311, BIN-313 |
| 结果未知码表 | BIN-081, BIN-092, BIN-093, BIN-095, BIN-308, BIN-310 |
| VenueCaps 默认值 | BIN-106, BIN-171, BIN-172, BIN-174 |
| 账户配置核验 | BIN-006, BIN-140, BIN-142–BIN-147, BIN-153, BIN-213, BIN-318, BIN-329 |
| 保证金数据换算 | BIN-117, BIN-141, BIN-148–BIN-152, BIN-319 |
| 外部成交识别 | BIN-104, BIN-108–BIN-110, BIN-314 |
| 用户流事件表 | BIN-013, BIN-110, BIN-112–BIN-121, BIN-315–BIN-317, BIN-328 |
| 测试网/模拟盘 | BIN-212–BIN-214, BIN-216 |
| 心跳与 BBO 探测 | BIN-050–BIN-056, BIN-190, BIN-191, BIN-306, BIN-323 |
| 只读查询 | BIN-175, BIN-330–BIN-334 |
| `mock` 特性(给 mock-exchange 的反向编解码) | BIN-211 |
| 实现说明 | 不适用:见子节 |
| 1. 分派(`classify` / `classify_report`,只看前 64 B) | BIN-010–BIN-013, BIN-035, BIN-036, BIN-119, BIN-305 |
| 2. 解码 | BIN-024, BIN-026, BIN-120, BIN-152, BIN-210 |
| 3. 快照对齐(`align_first`) | 不适用:里程碑外 |
| 4. 订单帧模板 | BIN-064–BIN-067, BIN-073–BIN-075, BIN-402 |
| 5. 回报解码 | BIN-090, BIN-092, BIN-096, BIN-100, BIN-101, BIN-106, BIN-107, BIN-111, BIN-313, BIN-327 |
| 6. 死手与会话维护 | BIN-007, BIN-035, BIN-060, BIN-080, BIN-081, BIN-118, BIN-172, BIN-173, BIN-202, BIN-312 |
| 7. 订阅节奏 | BIN-002, BIN-200–BIN-202 |
| 8. 源文件规划 | BIN-142, BIN-174, BIN-400, BIN-401 |
| 延迟与资源契约 | BIN-038, BIN-039, BIN-500–BIN-508 |
| 测试与验收 | 不适用:非规范 |
| 依赖 | BIN-210 |
| 延伸阅读 | 不适用:非规范 |
| 已知限制与后续工作 | BIN-076, BIN-201 |
| 开发备忘 | BIN-214, BIN-215 |

## 用例

### BIN-001 两个产品线的身份常量与 fixtures 目录名一致

- 规则:[概要](README.md#概要)、[使用本组件](README.md#使用本组件)
- 级别:table
- 里程碑:M3
- 前置:无参数;每行断言 `VenueProtocol` 关联常量;`VenueId` 数值取自 hft-types 的 VenueId 表
- 实现:`crates/venue-binance/tests/consts.rs::bin_001_identity_consts`

| 行 | 常量 | 期望 |
|---|---|---|
| BIN-001.1 | `Usdm::NAME` | `"binance-usdm"`;`fixtures/binance-usdm/` 目录存在 |
| BIN-001.2 | `Usdm::VENUE` | `BINANCE_USDM`(1) |
| BIN-001.3 | `Usdm::MARKET` | `MarketKind::LinearPerp`(2),且 `== VenueId::market(BINANCE_USDM)` |
| BIN-001.4 | `Usdm::ORDER_FRAME_CAP` | 2048 |
| BIN-001.5 | `Spot::NAME` | `"binance-spot"`;`fixtures/binance-spot/` 目录存在 |
| BIN-001.6 | `Spot::VENUE` | `BINANCE_SPOT`(2) |
| BIN-001.7 | `Spot::MARKET` | `MarketKind::Spot`(1),且 `== VenueId::market(BINANCE_SPOT)` |
| BIN-001.8 | `Spot::ORDER_FRAME_CAP` | 2048 |

### BIN-002 Usdm 的路由表恰有一条 Bbo、一条 DepthTopN 与两条 Md 路由

- 规则:[端点与路由](README.md#端点与路由)、[分档快照流](README.md#分档快照流)、[7. 订阅节奏](README.md#7-订阅节奏)
- 级别:table
- 里程碑:M1.5
- 前置:`Usdm::routes()`;每行按 `name` 找到一条 `RouteSpec` 并断言全部字段;另断言表长 = 4、`RouteId` 互不相同
- 实现:`crates/venue-binance/tests/routes.rs::bin_002_usdm_routes`

| 行 | `name` | 期望 `kind` / `race` / `max_streams` / `max_in_msgs_per_s` / `topn_levels` |
|---|---|---|
| BIN-002.1 | `public-bbo` | `Bbo` / `true` / 1024 / 10 / `[]` |
| BIN-002.2 | `public-topn` | `DepthTopN` / `false` / 1024 / 10 / `[5, 10, 20]` |
| BIN-002.3 | `market` | `Md` / `false` / 1024 / 10 / `[]` |
| BIN-002.4 | `public-depth` | `Md` / `false` / 1024 / 10 / `[]` |
| BIN-002.5 | 任一 `kind ≠ Bbo` 的行 | `race = false`(只有 Bbo 路由可赛马) |

### BIN-003 Spot 的路由表恰有一条 Bbo、一条 DepthTopN 与一条 Md 路由

- 规则:[端点与路由](README.md#端点与路由)、[分档快照流](README.md#分档快照流)
- 级别:table
- 里程碑:M1.5
- 前置:`Spot::routes()`;特性只开默认(不含 `spot-sbe`);另断言表长 = 3
- 实现:`crates/venue-binance/tests/routes.rs::bin_003_spot_routes`

| 行 | `name` | 期望 `kind` / `race` / `max_streams` / `max_in_msgs_per_s` / `topn_levels` |
|---|---|---|
| BIN-003.1 | `spot-bbo` | `Bbo` / `true` / 1024 / 5 / `[]` |
| BIN-003.2 | `spot-topn` | `DepthTopN` / `false` / 1024 / 5 / `[5, 10, 20]` |
| BIN-003.3 | `spot-md` | `Md` / `false` / 1024 / 5 / `[]` |

### BIN-004 endpoint 按角色与路由写出路径与查询串

- 规则:[端点与路由](README.md#端点与路由)
- 级别:table
- 里程碑:M1.5
- 前置:`RD-BIN`;`VenueCfg` 为 prod 环境;`now_ms = 1790000000000`;`cred = None`(行情)或测试凭证;`boot` 除注明外为 `None`;每行断言 `EndpointSpec` 的 path 与 query(主机来自 refdata 端点表,不在本用例)
- 实现:`crates/venue-binance/tests/endpoint.rs::bin_004_endpoint_paths`

| 行 | 产品线 / 角色 / 路由 | 期望 path + query |
|---|---|---|
| BIN-004.1 | `Usdm` / `Md` / `public-bbo` | `/public/ws`,无 query |
| BIN-004.2 | `Usdm` / `Md` / `public-topn` | `/public/ws`(依赖待核实:开发备忘第 20 条) |
| BIN-004.3 | `Usdm` / `Md` / `market` | `/market/ws` |
| BIN-004.4 | `Usdm` / `UserStream` / —,`boot = Some(br#"{"listenKey":"lk0123"}"#)`(listenKey 请求的响应体) | `/private/ws/lk0123` |
| BIN-004.5 | `Usdm` / `Order` / — | `/ws-fapi/v1` |
| BIN-004.6 | `Spot` / `Md` / `spot-bbo` | `/ws`,query `timeUnit=MICROSECOND` |
| BIN-004.7 | `Spot` / `Md` / `spot-md` | `/ws`,query `timeUnit=MICROSECOND` |
| BIN-004.8 | `Spot` / `Md` / `spot-topn` | `/stream`,query `timeUnit=MICROSECOND` |
| BIN-004.9 | `Spot` / `Order` / — | `/ws-api/v3`,query `timeUnit=MICROSECOND` |

### BIN-005 Spot 没有独立的用户流连接

- 规则:[端点与路由](README.md#端点与路由)
- 级别:unit
- 里程碑:M1.5
- 前置:`RD-BIN`;`now_ms = 1790000000000`;测试凭证
- 步骤:
  1. `Spot::endpoint(Role::UserStream, RouteId(0), &cfg, Some(&cred), None, now_ms, &mut out)`
- 期望:
  - 返回 `Err(ProtoErr::Unsupported)`;`out` 的 path 长度为 0
- 实现:`crates/venue-binance/tests/endpoint.rs::bin_005_spot_no_user_stream_role`

### BIN-006 REST 请求的端点下标按端点表取值

- 规则:[端点与路由](README.md#端点与路由)、[账户配置核验](README.md#账户配置核验)
- 级别:table
- 里程碑:M3
- 前置:`RD-BIN`;`WC`;`gw.stale_guard_ms = 1500`;账户 2;每行生成一个 `RestReqBuf` 并断言 `endpoint`
- 实现:`crates/venue-binance/tests/endpoint.rs::bin_006_rest_endpoint_index`

| 行 | 请求 | 期望 `endpoint` |
|---|---|---|
| BIN-006.1 | `Usdm` Config 第 0 步 `positionSide/dual` | 0 |
| BIN-006.2 | `Usdm` Config 第 4 步 `/fapi/v3/account` | 0 |
| BIN-006.3 | `Usdm` Config 第 5 步 `/api/v3/account` | 1 |
| BIN-006.4 | `Usdm` Config 第 6 步 `apiRestrictions` | 1 |
| BIN-006.5 | `Usdm` `session_identity_request` | 1 |
| BIN-006.6 | `Usdm` `bbo_snapshot_request([1042])` | 0 |
| BIN-006.7 | `Usdm` `dead_man_refresh([1042])` 的请求 | 0 |
| BIN-006.8 | `Spot` Config 第 0、1、2 步与 `session_identity_request` | 0 |
| BIN-006.9 | `Spot` `bbo_snapshot_request([2001])` | 0 |

### BIN-007 只有 Usdm 用户流在建连前需要一次 listenKey 请求

- 规则:[端点与路由](README.md#端点与路由)、[6. 死手与会话维护](README.md#6-死手与会话维护)
- 级别:table
- 里程碑:M1.5
- 前置:`RD-BIN`;prod 环境 `VenueCfg`
- 实现:`crates/venue-binance/tests/endpoint.rs::bin_007_bootstrap`

| 行 | 调用 | 期望 |
|---|---|---|
| BIN-007.1 | `Usdm::bootstrap(Role::UserStream, &cfg, Some(&HM1))` | `Some(BootCall)`:`POST /fapi/v1/listenKey`,REST 端点下标 0,头含 `X-MBX-APIKEY: TESTKEY`,无查询参数与签名 |
| BIN-007.2 | `Usdm::bootstrap(Role::Md, &cfg, None)` | `None` |
| BIN-007.3 | `Usdm::bootstrap(Role::Order, &cfg, Some(&HM1))` | `None` |
| BIN-007.4 | `Spot::bootstrap(Role::Order, &cfg, Some(&HM1))` | `None`(用户流在 ws-api 归属连接上订阅) |
| BIN-007.5 | `Spot::bootstrap(Role::Md, &cfg, None)` | `None` |

### BIN-010 Usdm 行情帧按前 64 B 与路由分派

- 规则:[1. 分派(`classify` / `classify_report`,只看前 64 B)](README.md#1-分派classify--classify_report只看前-64-b)
- 级别:table
- 里程碑:M1.5
- 前置:`Usdm::classify(route, op, head)`;`head` 为 payload 的前 min(64, len) 字节;路由取自 BIN-002
- 实现:`crates/venue-binance/tests/classify.rs::bin_010_usdm_classify`

| 行 | 路由 / op / payload 开头 | 期望 `MdClass` |
|---|---|---|
| BIN-010.1 | `public-bbo` / Text / `{"e":"bookTicker","u":4009…` | `BookTicker` |
| BIN-010.2 | `public-topn` / Text / `{"e":"depthUpdate","E":17…` | `DepthTopN` |
| BIN-010.3 | `public-depth` / Text / `{"e":"depthUpdate","E":17…` | `DepthUpdate` |
| BIN-010.4 | `market` / Text / `{"e":"depthUpdate","E":17…` | `Unknown` |
| BIN-010.5 | `market` / Text / `{"e":"aggTrade","E":17…` | `AggTrade` |
| BIN-010.6 | `market` / Text / `{"e":"markPriceUpdate","E":17…` | `MarkPrice` |
| BIN-010.7 | `market` / Text / `{"e":"forceOrder","E":17…` | `ForceOrder` |
| BIN-010.8 | 任一 / Text / `{"result":null,"id":7}` | `SubAck` |
| BIN-010.9 | 任一 / Text / `{"error":{"code":2,"msg":"Invalid request"},"id":7}` | `SubAck` |
| BIN-010.10 | 任一 / Text / `{"e":"serverShutdown","E":17…` | `ServerShutdown` |
| BIN-010.11 | `market` / Text / `{"e":"kline","E":17…` | `Unknown` |
| BIN-010.12 | `public-bbo` / Binary / 任意 8 字节 | `Unknown` |
| BIN-010.13 | `public-bbo` / Text / 空 | `Unknown` |
| BIN-010.14 | `public-bbo` / Text / `{"e` | `Unknown` |
| BIN-010.15 | `public-bbo` / Text / 70 个空格后接 `{"e":"bookTicker"…` | `Unknown`(事件名不在前 64 B) |

### BIN-011 Spot 行情帧按前 64 B 分派

- 规则:[1. 分派(`classify` / `classify_report`,只看前 64 B)](README.md#1-分派classify--classify_report只看前-64-b)
- 级别:table
- 里程碑:M1.5
- 前置:`Spot::classify(route, op, head)`;默认特性(不含 `spot-sbe`)
- 实现:`crates/venue-binance/tests/classify.rs::bin_011_spot_classify`

| 行 | 路由 / op / payload 开头 | 期望 `MdClass` |
|---|---|---|
| BIN-011.1 | `spot-bbo` / Text / `{"u":400900217,"s":"BTCUSDT",…` | `JsonBookTicker` |
| BIN-011.2 | `spot-md` / Text / `{"e":"trade","E":1790…` | `JsonTrade` |
| BIN-011.3 | `spot-md` / Text / `{"e":"depthUpdate","E":1790…` | `JsonDepth` |
| BIN-011.4 | `spot-topn` / Text / `{"stream":"btcusdt@depth5@100ms","data":{…` | `JsonDepthTopN` |
| BIN-011.5 | `spot-topn` / Text / `{"stream":"btcusdt@trade","data":{…` | `Unknown` |
| BIN-011.6 | 任一 / Text / `{"result":null,"id":3}` | `SubAck` |
| BIN-011.7 | `spot-bbo` / Binary / 任意 8 字节 | `Unknown`(未启用 `spot-sbe`) |
| BIN-011.8 | `spot-md` / Text / `{"e":"24hrTicker",…` | `Unknown` |

### BIN-012 classify 只依赖前 64 字节

- 规则:[1. 分派(`classify` / `classify_report`,只看前 64 B)](README.md#1-分派classify--classify_report只看前-64-b)
- 级别:prop
- 里程碑:M3
- 性质:两个 payload 前 64 B 相同、其后任意不同 → `classify` 与 `classify_report` 的结果相同(`Usdm` 与 `Spot` 各自、对每条路由)
- 生成:前缀取自 `fixtures/binance-usdm/**` 与 `fixtures/binance-spot/**` 的全部 `.in` 的前 64 B;后缀为 0–4096 字节随机串
- 参考:以截断到 64 B 的 payload 调用的结果
- 例数:PR 2 000;每夜 100 000
- 实现:`crates/venue-binance/tests/classify.rs::bin_012_classify_reads_prefix_only`

### BIN-013 回报与用户流帧按前 64 B 分派

- 规则:[1. 分派(`classify` / `classify_report`,只看前 64 B)](README.md#1-分派classify--classify_report只看前-64-b)、[用户流事件表](README.md#用户流事件表)、[下单:WS API](README.md#下单ws-api)
- 级别:table
- 里程碑:M1.5
- 前置:`classify_report(Text, head)`;`default_caps()` 的可忽略名单
- 实现:`crates/venue-binance/tests/classify.rs::bin_013_classify_report`

| 行 | 产品线 / payload 开头 | 期望 `ReportClass` |
|---|---|---|
| BIN-013.1 | `Usdm` / `{"id":"p0000002a00000007","status":200,…` | `WsResp` |
| BIN-013.2 | `Usdm` / `{"id":"q0000002a00000007","status":400,…` | `WsResp` |
| BIN-013.3 | `Usdm` / `{"id":"l0000000000000000","status":200,…` | `Control` |
| BIN-013.4 | `Usdm` / `{"id":"z0000002a00000007",…` | `Unknown` |
| BIN-013.5 | `Usdm` / `{"id":"P0000002A00000007",…`(大写) | `Unknown` |
| BIN-013.6 | `Usdm` / `{"e":"ORDER_TRADE_UPDATE","T":…` | `OrderTradeUpdate` |
| BIN-013.7 | `Usdm` / `{"e":"ACCOUNT_UPDATE",…` | `AccountUpdate` |
| BIN-013.8 | `Usdm` / `{"e":"MARGIN_CALL",…` | `MarginCall` |
| BIN-013.9 | `Usdm` / `{"e":"ACCOUNT_CONFIG_UPDATE",…` | `AccountConfigUpdate` |
| BIN-013.10 | `Usdm` / `{"e":"listenKeyExpired",…` | `ListenKeyExpired` |
| BIN-013.11 | `Usdm` / `{"e":"TRADE_LITE",…` | `Ignorable` |
| BIN-013.12 | `Usdm` / `{"e":"GRID_UPDATE",…` | `Ignorable` |
| BIN-013.13 | `Usdm` / `{"e":"FOO_EVENT",…` | `Unknown` |
| BIN-013.14 | `Spot` / `{"subscriptionId":0,"event":{"e":"executionReport",…` | `ExecutionReport` |
| BIN-013.15 | `Spot` / `{"subscriptionId":0,"event":{"e":"outboundAccountPosition",…` | `OutboundAccountPosition` |
| BIN-013.16 | `Spot` / `{"subscriptionId":0,"event":{"e":"eventStreamTerminated",…` | `EventStreamTerminated` |
| BIN-013.17 | `Spot` / `{"subscriptionId":0,"event":{"e":"balanceUpdate",…` | `Ignorable` |
| BIN-013.18 | `Spot` / `{"subscriptionId":0,"event":{"e":"listStatus",…` | `Ignorable` |
| BIN-013.19 | `Spot` / `{"id":"u0000000000000000","status":200,"result":{"subscriptionId":0}…` | `Control` |
| BIN-013.20 | `Spot` / `{"subscriptionId":0,"event":{"e":"newEvt",…` | `Unknown` |

### BIN-020 seq_rule 与 resync_method 按类给出

- 规则:[行情频道 → 事件](README.md#行情频道--事件)、[分档快照流](README.md#分档快照流)、[hft-venue-proto 契约 26](../hft-venue-proto/README.md#实现方必须遵守的契约)
- 级别:table
- 里程碑:M1.5
- 前置:无参数
- 实现:`crates/venue-binance/tests/md.rs::bin_020_seq_rule_resync`

| 行 | 类 | 期望 `seq_rule` / `resync_method` |
|---|---|---|
| BIN-020.1 | `usdm::BookTicker` | `Monotonic` / `None` |
| BIN-020.2 | `usdm::DepthTopN` | `Snapshot` / `None` |
| BIN-020.3 | `usdm::AggTrade` | `Monotonic` / `None` |
| BIN-020.4 | `usdm::MarkPrice` | `ByTime` / `None` |
| BIN-020.5 | `usdm::ForceOrder` | `None` / `None` |
| BIN-020.6 | `spot::JsonBookTicker` | `Monotonic` / `None` |
| BIN-020.7 | `spot::JsonDepthTopN` | `Snapshot` / `None` |
| BIN-020.8 | `spot::JsonTrade` | `Monotonic` / `None` |

### BIN-021 peek 对 bookTicker 只取 inst 与 u

- 规则:[行情频道 → 事件](README.md#行情频道--事件)、[hft-venue-proto 契约 2](../hft-venue-proto/README.md#实现方必须遵守的契约)
- 级别:table
- 里程碑:M1.5
- 前置:`RD-BIN`;全新 `FeedCtx`;payload 见记号 `BT1`(`Usdm`)、`SBT1`(`Spot`)与下表改写
- 实现:`crates/venue-binance/tests/md.rs::bin_021_peek_ids`

| 行 | 输入 | 期望 `Peek` |
|---|---|---|
| BIN-021.1 | `Usdm` `BT1` | `Ok(Peek{inst: 1042, ids: {prev_id: 0, last_id: 400900217}})` |
| BIN-021.2 | `Usdm` `BT1` 的 `s` 改为 `"DOGEUSDT"`(不在 `RD-BIN`) | `Ok(Peek{inst: 0, …})` |
| BIN-021.3 | `Usdm` aggTrade,`a = 26129` | `ids.last_id = 26129`,`prev_id = 0` |
| BIN-021.4 | `Usdm` markPriceUpdate,`E = 1790000000125` | `ids.last_id = 1790000000125000000`(ns),`prev_id = 0` |
| BIN-021.5 | `Usdm` forceOrder | `ids.last_id = 0`,`prev_id = 0` |
| BIN-021.6 | `Usdm` depth5(`DepthTopN`),`u = 7000000001` | `ids.last_id = 7000000001` |
| BIN-021.7 | `Spot` `SBT1` | `inst: 2001`,`ids.last_id = 400900217` |
| BIN-021.8 | `Spot` combined depth5,`stream = "bnbusdt@depth5@100ms"`,`lastUpdateId = 160` | `inst: 2002`,`ids.last_id = 160` |
| BIN-021.9 | `Spot` combined depth5,`stream = "dogeusdt@depth5@100ms"` | `inst: 0` |

### BIN-022 peek 不解析价格与数量

- 规则:[hft-venue-proto 契约 2](../hft-venue-proto/README.md#实现方必须遵守的契约)
- 级别:unit
- 里程碑:M1.5
- 前置:`RD-BIN`;`BT1` 的 `b` 改为 `"abc"`
- 步骤:
  1. `Usdm::peek(rd, ctx, BookTicker, payload)`
  2. `Usdm::decode_md(rd, ctx, BookTicker, payload, &RX, &mut sink)`
- 期望:
  - 步骤 1 返回 `Ok(Peek{inst: 1042, ids.last_id: 400900217})`
  - 步骤 2 返回 `Err(DecodeErr::Number)`;`sink` 中没有事件
- 实现:`crates/venue-binance/tests/md.rs::bin_022_peek_skips_px_qty`

### BIN-023 Usdm bookTicker 解码为 Bbo,时间戳按毫秒换算且原样带入 RxMeta

- 规则:[行情频道 → 事件](README.md#行情频道--事件)、[2. 解码](README.md#2-解码)、[hft-venue-proto 契约 3](../hft-venue-proto/README.md#实现方必须遵守的契约)
- 级别:unit
- 里程碑:M1.5
- 前置:`RD-BIN`;`Usdm` 的 `FeedCtx.time_unit = Ms`(连接 URL 不带 `timeUnit`);输入 `BT1`;`RX`
- 步骤:
  1. `decode_md(rd, ctx, BookTicker, BT1, &RX, &mut sink)`
- 期望:
  - 返回 `Ok(MdOutcome{events: 1, inst: 1042})`
  - `sink` 恰好一条 `Bbo`:`h = {venue: 1, conn: 3, inst: 1042, ex_ts_ns: 1790000000123000000, ex_send_ns: 1790000000125000000, hw_rx_ns: 1790000000126000000, rx_tsc: 1000, sw_rx_ns: 1790000000126500000, rx_empty_gap_ns: 2500, _rsv: 0}`,`bid_px = 65000.1(6500010)`、`bid_qty = 1.5(150000000)`、`ask_px = 65000.2(6500020)`、`ask_qty = 0.004(400000)`、`update_id = 400900217`
- 实现:`crates/venue-binance/tests/md.rs::bin_023_usdm_bookticker_to_bbo`

### BIN-024 毫秒连接上出现微秒量级的时间戳返回 Overflow,不按数值大小改猜单位

- 规则:[2. 解码](README.md#2-解码)、[hft-venue-proto 契约 3](../hft-venue-proto/README.md#实现方必须遵守的契约)
- 级别:unit
- 里程碑:M1.5
- 前置:`RD-BIN`;`FeedCtx.time_unit = Ms`;`BT1` 的 `T` 改为 `1790000000123456`(µs 量级)
- 步骤:
  1. `decode_md(rd, ctx, BookTicker, payload, &RX, &mut sink)`
- 期望:
  - 返回 `Err(DecodeErr::Overflow)`(1790000000123456 × 10⁶ 超出 i64);`sink` 中没有事件
- 实现:`crates/venue-binance/tests/md.rs::bin_024_no_unit_guessing`

### BIN-025 Spot JSON bookTicker 没有时间戳字段,两个时间戳为 0

- 规则:[行情频道 → 事件](README.md#行情频道--事件)
- 级别:unit
- 里程碑:M1.5
- 前置:`RD-BIN`;`Spot` `FeedCtx.time_unit = Us`;输入 `SBT1`;`RX`
- 步骤:
  1. `decode_md(rd, ctx, JsonBookTicker, SBT1, &RX, &mut sink)`
- 期望:
  - 恰好一条 `Bbo{h.venue: 2, h.inst: 2001, h.ex_ts_ns: 0, h.ex_send_ns: 0, bid_px: 65000.10(6500010), bid_qty: 0.5(50000000), ask_px: 65000.11(6500011), ask_qty: 0.25(25000000), update_id: 400900217}`,`h` 的接收字段同 BIN-023
- 实现:`crates/venue-binance/tests/md.rs::bin_025_spot_bookticker_no_ts`

### BIN-026 bookTicker 字段异常时返回对应的 DecodeErr 且不产出事件

- 规则:[2. 解码](README.md#2-解码)、[hft-venue-proto 契约 4](../hft-venue-proto/README.md#实现方必须遵守的契约)、[hft-venue-proto 契约 5](../hft-venue-proto/README.md#实现方必须遵守的契约)
- 级别:table
- 里程碑:M1.5
- 前置:`RD-BIN`(BTC `px_exp = −2`,`qty_exp = −8`);`Usdm` `FeedCtx.time_unit = Ms`;每行在 `BT1` 上做一处改写;每行另断言 `sink` 为空、无 panic
- 实现:`crates/venue-binance/tests/md.rs::bin_026_bookticker_errors`

| 行 | 改写 | 期望 |
|---|---|---|
| BIN-026.1 | 删除 `"b"` 字段 | `Err(Shape)` |
| BIN-026.2 | `"u"` 改为字符串 `"x"` | `Err(Shape)` |
| BIN-026.3 | 在 `"a":"65000.2` 处截断 | `Err(Truncated)` |
| BIN-026.4 | `"b":""` | `Err(Number)` |
| BIN-026.5 | `"b":"65000.123"`(超精度非零位) | `Err(Number)` |
| BIN-026.6 | `"B":"100000000000000"`(10¹⁴ BTC,raw 超 i64) | `Err(Overflow)` |
| BIN-026.7 | `"b":"6.50001E4"`(科学计数) | `Ok`,`bid_px = 65000.1(6500010)` |
| BIN-026.8 | `"b":"65000.1000000"`(尾零超精度) | `Ok`,`bid_px = 6500010` |
| BIN-026.9 | `"A":"92233720.36854775"`(17 位有效数字,f64 不能精确表示) | `Ok`,`ask_qty = 9223372036854775`(逐位相等,不经 f64) |
| BIN-026.10 | `"T":"1790000000123"`(字符串) | `Err(Shape)` |
| BIN-026.11 | `"b":"0.00000000","B":"0.00000000"`(买侧无挂单) | `Ok`,`bid_px = 0`、`bid_qty = 0`,卖侧照常(下游按缺一侧处理) |

### BIN-027 Usdm 分档快照解码为一条 L2Snapshot{TopN},不产出 L2Batch

- 规则:[分档快照流](README.md#分档快照流)、[hft-venue-proto 契约 26](../hft-venue-proto/README.md#实现方必须遵守的契约)
- 级别:unit
- 里程碑:M3
- 前置:`RD-BIN`;路由 `public-topn`;输入 `DP5`(记号);`RX`
- 步骤:
  1. `decode_md(rd, ctx, DepthTopN, DP5, &RX, &mut sink)`
- 期望:
  - 返回 `Ok(MdOutcome{events: 1, inst: 1042})`
  - `sink` 恰好一次 `l2_snapshot`:`hdr = {h.inst: 1042, h.ex_ts_ns: 1790000000300000000, h.ex_send_ns: 1790000000301000000, snap_id: 7000000001, origin: TopN(3), n_bids: 5, n_asks: 5}`,`bids` 价格严格降序 `[65000.1, 65000.0, 64999.9, 64999.8, 64999.7]`,`asks` 严格升序 `[65000.2, 65000.3, 65000.4, 65000.5, 65000.6]`,数量逐档等于报文
  - `l2_begin`/`l2_level`/`l2_end` 调用 0 次
- 实现:`crates/venue-binance/tests/md.rs::bin_027_usdm_topn_snapshot`

### BIN-028 分档快照中缺失的档与数量为 0 的档不输出

- 规则:[分档快照流](README.md#分档快照流)
- 级别:table
- 里程碑:M3
- 前置:`RD-BIN`;路由 `public-topn`;在 `DP5` 上改写
- 实现:`crates/venue-binance/tests/md.rs::bin_028_topn_empty_levels`

| 行 | 改写 | 期望 |
|---|---|---|
| BIN-028.1 | `b` 只有 3 档 | `n_bids = 3`,`n_asks = 5` |
| BIN-028.2 | `b` 第 2 档 `qty = "0.000"` | `n_bids = 4`,该价位不在 `bids` 中 |
| BIN-028.3 | `b` 与 `a` 都是空数组 | 一条 `L2Snapshot{n_bids: 0, n_asks: 0, snap_id: 7000000001}` |
| BIN-028.4 | `b` 第 1 档价格 `"abc"` | `Err(Number)`;无 `l2_snapshot` 调用 |

### BIN-029 Spot combined 流的分档快照按 stream 名取 inst,时间戳为 0

- 规则:[分档快照流](README.md#分档快照流)、[1. 分派(`classify` / `classify_report`,只看前 64 B)](README.md#1-分派classify--classify_report只看前-64-b)
- 级别:unit
- 里程碑:M3
- 前置:`RD-BIN`;路由 `spot-topn`;输入 `SDP5`(记号,`stream = "bnbusdt@depth5@100ms"`,`lastUpdateId = 160`)
- 步骤:
  1. `decode_md(rd, ctx, JsonDepthTopN, SDP5, &RX, &mut sink)`
- 期望:
  - 一条 `l2_snapshot`:`hdr = {h.venue: 2, h.inst: 2002, h.ex_ts_ns: 0, h.ex_send_ns: 0, snap_id: 160, origin: TopN(3), n_bids: 5, n_asks: 5}`
  - 依赖待核实:开发备忘第 20 条(推送体不带 symbol)
- 实现:`crates/venue-binance/tests/md.rs::bin_029_spot_topn_combined`

### BIN-030 aggTrade 按 m 决定主动方

- 规则:[行情频道 → 事件](README.md#行情频道--事件)
- 级别:table
- 里程碑:M3
- 前置:`RD-BIN`;`Usdm` `market` 路由;payload `{"e":"aggTrade","E":1790000000201,"s":"BTCUSDT","a":26129,"p":"65000.1","q":"0.010","f":100,"l":105,"T":1790000000200,"m":<m>}`
- 实现:`crates/venue-binance/tests/md.rs::bin_030_aggtrade`

| 行 | `m` | 期望 `Trade` |
|---|---|---|
| BIN-030.1 | `true` | `{px: 6500010, qty: 1000000, trade_id: 26129, taker_side: Sell, h.ex_ts_ns: 1790000000200000000, h.ex_send_ns: 1790000000201000000}` |
| BIN-030.2 | `false` | 同上,`taker_side: Buy` |
| BIN-030.3 | 缺失 | `Err(Shape)` |

### BIN-031 Spot trade 的时间戳按微秒换算

- 规则:[行情频道 → 事件](README.md#行情频道--事件)
- 级别:unit
- 里程碑:M3
- 前置:`RD-BIN`;`Spot` `FeedCtx.time_unit = Us`;payload `{"e":"trade","E":1790000000201500,"s":"BTCUSDT","t":12345,"p":"65000.10","q":"0.00100","T":1790000000200500,"m":false}`
- 步骤:
  1. `decode_md(rd, ctx, JsonTrade, payload, &RX, &mut sink)`
- 期望:
  - 一条 `Trade{h.inst: 2001, px: 6500010, qty: 100000, trade_id: 12345, taker_side: Buy, h.ex_ts_ns: 1790000000200500000, h.ex_send_ns: 1790000000201500000}`
- 实现:`crates/venue-binance/tests/md.rs::bin_031_spot_trade_us`

### BIN-032 markPriceUpdate 产出 MarkPrice、IndexPrice 与 Funding 三条事件

- 规则:[行情频道 → 事件](README.md#行情频道--事件)
- 级别:unit
- 里程碑:M3
- 前置:`RD-BIN`;全新 `FeedCtx`;payload `MP1` = `{"e":"markPriceUpdate","E":1790000000125,"s":"BTCUSDT","p":"65001.23","i":"65000.50","P":"65010.00","r":"0.00010000","T":1790006400000}`
- 步骤:
  1. `decode_md(rd, ctx, MarkPrice, MP1, &RX, &mut sink)`
- 期望:
  - 返回 `Ok(MdOutcome{events: 3, inst: 1042})`
  - 依次 `MarkPrice{px: 6500123}`、`IndexPrice{px: 6500050}`、`Funding{rate: 0.0001(100000), next_funding_ns: 1790006400000000000, interval_s: 0}`;三条的 `h.ex_send_ns = 1790000000125000000`、`h.ex_ts_ns = 0`
- 实现:`crates/venue-binance/tests/md.rs::bin_032_markprice_three_events`

### BIN-033 Funding.interval_s 取相邻不同 T 之差并随周期变化更新

- 规则:[行情频道 → 事件](README.md#行情频道--事件)
- 级别:unit
- 里程碑:M3
- 前置:`RD-BIN`;同一个 `FeedCtx`;`A = 1790006400000`;依次喂 5 条 `MP1` 形状的推送,只改 `T`:A、A、A + 28800000、A + 28800000、A + 43200000
- 步骤:
  1. 逐条 `decode_md`,收集 `Funding.interval_s`
  2. 另取一个全新 `FeedCtx` 喂 `T = A + 43200000` 一条
- 期望:
  - 步骤 1 的 `interval_s` 依次为 `0, 0, 28800, 28800, 14400`
  - 步骤 2 的 `interval_s = 0`
  - ETHUSDT 的推送与 BTCUSDT 交错时互不影响(各 inst 独立记 `T`)
- 实现:`crates/venue-binance/tests/md.rs::bin_033_funding_interval`

### BIN-034 公共 forceOrder 解码为 Liquidation

- 规则:[行情频道 → 事件](README.md#行情频道--事件)
- 级别:unit
- 里程碑:M3
- 前置:`RD-BIN`;payload `{"e":"forceOrder","E":1790000000401,"o":{"s":"BTCUSDT","S":"SELL","o":"LIMIT","f":"IOC","q":"0.014","p":"64000.0","ap":"64010.5","X":"FILLED","l":"0.014","z":"0.014","T":1790000000400}}`
- 步骤:
  1. `decode_md(rd, ctx, ForceOrder, payload, &RX, &mut sink)`
- 期望:
  - 一条 `Liquidation{h.inst: 1042, px: 64010.5(6401050), qty: 0.014(1400000), side: Sell, h.ex_ts_ns: 1790000000400000000, h.ex_send_ns: 1790000000401000000}`;无 `ExternalFill`(公共流与本账户无关)
- 实现:`crates/venue-binance/tests/md.rs::bin_034_force_order`

### BIN-035 订阅回执、订阅被拒与 serverShutdown 经 control 给出提示

- 规则:[1. 分派(`classify` / `classify_report`,只看前 64 B)](README.md#1-分派classify--classify_report只看前-64-b)、[6. 死手与会话维护](README.md#6-死手与会话维护)
- 级别:table
- 里程碑:M1.5
- 前置:`Usdm::control(c, payload)`
- 实现:`crates/venue-binance/tests/md.rs::bin_035_control_hints`

| 行 | 类 / payload | 期望 |
|---|---|---|
| BIN-035.1 | `SubAck` / `{"result":null,"id":7}` | `Some(SubAck{ok: true, code: 0})` |
| BIN-035.2 | `SubAck` / `{"error":{"code":2,"msg":"Invalid request"},"id":7}` | `Some(SubAck{ok: false, code: 2})` |
| BIN-035.3 | `ServerShutdown` / `{"e":"serverShutdown","E":1790000000000}` | `Some(ServerShutdown)` |
| BIN-035.4 | `BookTicker` / `BT1` | `None` |

### BIN-036 以不处理的类调用 decode_md 返回 UnknownClass

- 规则:[hft-venue-proto 契约 5](../hft-venue-proto/README.md#实现方必须遵守的契约)
- 级别:table
- 里程碑:M1.5
- 前置:`RD-BIN`;每行喂 BIN-035.1 的 payload
- 实现:`crates/venue-binance/tests/md.rs::bin_036_decode_unknown_class`

| 行 | 类 | 期望 |
|---|---|---|
| BIN-036.1 | `usdm::SubAck` | `Err(UnknownClass)`,`sink` 为空 |
| BIN-036.2 | `usdm::Unknown` | `Err(UnknownClass)` |
| BIN-036.3 | `usdm::ServerShutdown` | `Err(UnknownClass)` |
| BIN-036.4 | `spot::Unknown` | `Err(UnknownClass)` |

### BIN-037 每种行情事件的 MarketHdr 原样带入 RxMeta 的接收字段

- 规则:[hft-venue-proto 契约 3](../hft-venue-proto/README.md#实现方必须遵守的契约)
- 级别:table
- 里程碑:M3
- 前置:`RD-BIN`;`RX`;每行解码一条该类型的样本(取自对应金样的第一条输入),断言事件 `h.venue`、`h.conn = 3`、`h.rx_tsc = 1000`、`h.hw_rx_ns = 1790000000126000000`、`h.sw_rx_ns = 1790000000126500000`、`h.rx_empty_gap_ns = 2500`、`h._rsv = 0`
- 实现:`crates/venue-binance/tests/md.rs::bin_037_rxmeta_passthrough`

| 行 | 事件 | 期望 |
|---|---|---|
| BIN-037.1 | `Usdm` `Bbo` | 上述 6 个字段相等,`h.venue = 1` |
| BIN-037.2 | `Usdm` `L2Snapshot{TopN}` | 同上 |
| BIN-037.3 | `Usdm` `Trade` | 同上 |
| BIN-037.4 | `Usdm` `MarkPrice`、`IndexPrice`、`Funding` | 三条都同上 |
| BIN-037.5 | `Usdm` `Liquidation` | 同上 |
| BIN-037.6 | `Spot` `Bbo`、`L2Snapshot{TopN}`、`Trade` | 同上,`h.venue = 2` |

### BIN-038 热路径解码与编码零分配

- 规则:[延迟与资源契约](README.md#延迟与资源契约)、[hft-venue-proto 契约 5](../hft-venue-proto/README.md#实现方必须遵守的契约)
- 级别:unit
- 里程碑:M3
- 前置:计数分配器(`#[global_allocator]` 包装,测试内计数);`RD-BIN`;`build_templates` 已在计数开始前完成;样本 = 两个产品线全部 core 金样输入
- 步骤:
  1. 开始计数;对每个样本依次调用 `classify`、`peek`、`decode_md`(行情)或 `classify_report`、`decode_report`(回报),共 10⁴ 次;对 Place/Cancel/Amend/Query 各 `encode` 10³ 次
  2. 读取计数
- 期望:
  - 分配次数 = 0,释放次数 = 0
- 实现:`crates/venue-binance/tests/alloc.rs::bin_038_hot_path_zero_alloc`
### BIN-039 M1.5 热路径子集零分配

- 规则:[延迟与资源契约](README.md#延迟与资源契约)、[hft-venue-proto 契约 5](../hft-venue-proto/README.md#实现方必须遵守的契约)
- 级别:unit
- 里程碑:M1.5
- 前置:计数分配器(`#[global_allocator]` 包装);`RD-BIN`;两个产品线的 `build_templates` 在计数开始前完成;不分配的计数 sink(行情与回报);样本 = `BT1`、`SBT1`,`Usdm` 下单 ACK(带 `rateLimits`)、撤单成功、`-5022` 拒单与查询响应各一条
- 步骤:
  1. 预热一轮后开始计数;重复 1 000 次:两条行情样本各 `classify`、`peek`、`decode_md`,两个产品线各 `encode` 一条 `PostOnly` Place 与一条 Cancel,四条回报各 `classify_report`、`decode_report`
  2. 读取计数
- 期望:
  - 分配次数 = 0;回报 sink 共收到 4 × 1 001 条 `OrderEvent`、1 001 次 `exch_id`
- 实现:`crates/venue-binance/tests/alloc.rs::bin_039_hot_subset_zero_alloc`
- 备注:BIN-038(全部 core 金样输入)随金样在 M3 实现

### BIN-050 Usdm BBO 补帧请求一次取一个 inst

- 规则:[心跳与 BBO 探测](README.md#心跳与-bbo-探测)、[hft-venue-proto 契约 24](../hft-venue-proto/README.md#实现方必须遵守的契约)
- 级别:unit
- 里程碑:M3
- 前置:`RD-BIN`;snap-fetch 的全新 `FeedCtx`
- 步骤:
  1. `Usdm::bbo_snapshot_request(rd, ctx, &[1042], &mut out)`
- 期望:
  - 返回 `Ok(true)`;`out = RestReqBuf{endpoint: 0, method: GET, path_query: "/fapi/v1/ticker/bookTicker?symbol=BTCUSDT", headers: 空, body: 空, redacted: false}`
- 实现:`crates/venue-binance/tests/bbo_refresh.rs::bin_050_usdm_bbo_refresh_request`
- 备注:依赖待核实:开发备忘第 17 条(订阅后是否立即推当前 BBO;核实为"立即推"后本用例改为返回 `Ok(false)`)

### BIN-051 Usdm BBO 补帧一次给多个 inst 被拒

- 规则:[心跳与 BBO 探测](README.md#心跳与-bbo-探测)
- 级别:table
- 里程碑:M3
- 前置:`RD-BIN`;全新 `FeedCtx`
- 实现:`crates/venue-binance/tests/bbo_refresh.rs::bin_051_usdm_bbo_refresh_arity`

| 行 | `insts` | 期望 |
|---|---|---|
| BIN-051.1 | `[1042, 1043]`(超过 `BBO_SNAPSHOT_BATCH = 1`) | `Err(ProtoErr::BufFull)`,`out.path_query` 为空 |
| BIN-051.2 | `[]` | `Err(ProtoErr::Unsupported)`(运行时从不传空列表) |
| BIN-051.3 | `[2001]`(Spot 的 inst) | `Err(ProtoErr::Unsupported)` |

### BIN-052 Spot BBO 补帧把多个 symbol 写成 URL 编码的 JSON 数组

- 规则:[心跳与 BBO 探测](README.md#心跳与-bbo-探测)
- 级别:table
- 里程碑:M3
- 前置:`RD-BIN`;全新 `FeedCtx`;`RestReqBuf.path_query` 容量 1024 B
- 实现:`crates/venue-binance/tests/bbo_refresh.rs::bin_052_spot_bbo_refresh_request`

| 行 | `insts` | 期望 |
|---|---|---|
| BIN-052.1 | `[2001]` | `Ok(true)`,`path_query = "/api/v3/ticker/bookTicker?symbols=%5B%22BTCUSDT%22%5D"`,`endpoint = 0` |
| BIN-052.2 | `[2001, 2002]` | `Ok(true)`,`path_query = "/api/v3/ticker/bookTicker?symbols=%5B%22BTCUSDT%22%2C%22BNBUSDT%22%5D"` |
| BIN-052.3 | 使编码后 `path_query` 恰为 1024 B 的 inst 列表(测试 refdata 另加长名 symbol) | `Ok(true)` |
| BIN-052.4 | 再多一个 inst(1024 B + 1) | `Err(ProtoErr::BufFull)` |

### BIN-053 Usdm BBO 补帧响应解码为一条 Bbo

- 规则:[心跳与 BBO 探测](README.md#心跳与-bbo-探测)
- 级别:unit
- 里程碑:M3
- 前置:`RD-BIN`;snap-fetch 的 `FeedCtx`;`RX`;body `{"symbol":"BTCUSDT","bidPrice":"65000.1","bidQty":"1.500","askPrice":"65000.2","askQty":"0.004","time":1790000000500,"lastUpdateId":400900300}`
- 步骤:
  1. `decode_bbo_snapshot(rd, ctx, &[1042], body, &RX, &mut sink)`
- 期望:
  - 返回 `Ok(1)`;一条 `Bbo{h.inst: 1042, h.ex_ts_ns: 1790000000500000000, h.ex_send_ns: 0, bid_px: 6500010, bid_qty: 150000000, ask_px: 6500020, ask_qty: 400000, update_id: 400900300}`
- 实现:`crates/venue-binance/tests/bbo_refresh.rs::bin_053_usdm_bbo_refresh_decode`
- 备注:依赖待核实:开发备忘第 17 条(`lastUpdateId` 字段)

### BIN-054 Spot BBO 补帧响应按数组逐个产出 Bbo,序号与时间为 0

- 规则:[心跳与 BBO 探测](README.md#心跳与-bbo-探测)
- 级别:unit
- 里程碑:M3
- 前置:`RD-BIN`;body `[{"symbol":"BTCUSDT","bidPrice":"65000.10","bidQty":"0.5","askPrice":"65000.11","askQty":"0.25"},{"symbol":"BNBUSDT","bidPrice":"600.100","bidQty":"2","askPrice":"600.200","askQty":"3"}]`
- 步骤:
  1. `Spot::decode_bbo_snapshot(rd, ctx, &[2001, 2002], body, &RX, &mut sink)`
- 期望:
  - 返回 `Ok(2)`;依次 `Bbo{h.inst: 2001, update_id: 0, h.ex_ts_ns: 0, bid_px: 6500010, …}`、`Bbo{h.inst: 2002, update_id: 0, bid_px: 600.100(600100), bid_qty: 2(200000000), ask_px: 600200, ask_qty: 300000000}`
- 实现:`crates/venue-binance/tests/bbo_refresh.rs::bin_054_spot_bbo_refresh_decode`

### BIN-055 BBO 补帧响应的异常形态

- 规则:[心跳与 BBO 探测](README.md#心跳与-bbo-探测)
- 级别:table
- 里程碑:M3
- 前置:`RD-BIN`;每行另断言无 panic
- 实现:`crates/venue-binance/tests/bbo_refresh.rs::bin_055_bbo_refresh_errors`

| 行 | 产品线 / body | 期望 |
|---|---|---|
| BIN-055.1 | `Usdm` / BIN-053 去掉 `lastUpdateId` | `Ok(1)`,`update_id = 0` |
| BIN-055.2 | `Usdm` / `{"code":-1121,"msg":"Invalid symbol."}` | `Err(Shape)`,无事件 |
| BIN-055.3 | `Usdm` / BIN-053 在 `"askQty"` 处截断 | `Err(Truncated)` |
| BIN-055.4 | `Spot` / BIN-054 中 `BNBUSDT` 改为 `DOGEUSDT` | `Ok(1)`,只有 inst 2001 |
| BIN-055.5 | `Spot` / `[]` | `Ok(0)` |
| BIN-055.6 | `Spot` / BIN-054 第一项 `bidPrice` = `"x"` | `Err(Number)` |

### BIN-056 快照解码不依赖也不改动行情解码的 FeedCtx

- 规则:[hft-venue-proto 契约 25](../hft-venue-proto/README.md#实现方必须遵守的契约)
- 级别:unit
- 里程碑:M3
- 前置:`RD-BIN`;两个产品线各:`ctx_new` = 全新 `FeedCtx`;`ctx_used` = 已对 `fixtures/binance-*/` 的全部行情金样输入循环解码 10⁴ 条的 `FeedCtx`
- 步骤:
  1. 用 BIN-053(`Usdm`)/BIN-054(`Spot`)的 body 分别在 `ctx_new`、`ctx_used` 上 `decode_bbo_snapshot`
  2. 在 `ctx_used` 上再解码 BIN-033 的 5 条序列
- 期望:
  - 步骤 1 两次输出逐字节相同
  - 步骤 2 在 `ctx_used` 上得到的 `interval_s` 序列,与另一个解码过同样 10⁴ 条行情、但没有做过快照解码的 `FeedCtx` 上得到的序列逐项相同
- 实现:`crates/venue-binance/tests/bbo_refresh.rs::bin_056_snapshot_isolation`

### BIN-060 Usdm 登录帧用 Ed25519 签名 apiKey 与 timestamp

- 规则:[下单:WS API](README.md#下单ws-api)、[6. 死手与会话维护](README.md#6-死手与会话维护)
- 级别:unit
- 里程碑:M1.5
- 前置:测试凭证 `ED1`(RFC 8032 测试 1 私钥 `9d61b19deffd5a60ba844af492ec2cc44449c5697b326919703bac031cae7f60`,公钥 `d75a9801…511a`),`apiKey = "TESTKEY"`;`now_ms = 1790000000000`
- 步骤:
  1. `Usdm::login(Role::Order, &cfg, &ED1, now_ms, &mut out)`
- 期望:
  - 返回 `Ok(LoginStep::Frame)`
  - `out` 逐字节等于 `{"id":"l0000000000000000","method":"session.logon","params":{"apiKey":"TESTKEY","signature":"kyKvl/kTaZKE+kUIQtZzLoF+xBBLbhAc+PoRvtG+MseTjjM1xPqXeIBiOYildYFAu7yqD9fn723B1gwti8VcCA==","timestamp":1790000000000}}`(签名 = Ed25519(`apiKey=TESTKEY&timestamp=1790000000000`) 的标准 base64)
  - 签名用公钥验证通过
- 实现:`crates/venue-binance/tests/session.rs::bin_060_usdm_logon_frame`

### BIN-061 Spot 登录帧与 Usdm 同形

- 规则:[下单:WS API](README.md#下单ws-api)
- 级别:unit
- 里程碑:M1.5
- 前置:同 BIN-060
- 步骤:
  1. `Spot::login(Role::Order, &cfg, &ED1, now_ms, &mut out)`
- 期望:
  - 返回 `Ok(LoginStep::Frame)`;`out` 与 BIN-060 的帧逐字节相同
- 实现:`crates/venue-binance/tests/session.rs::bin_061_spot_logon_frame`

### BIN-062 login_result 区分成功、拒绝与他人的帧

- 规则:[下单:WS API](README.md#下单ws-api)、[hft-venue-proto 契约 8](../hft-venue-proto/README.md#实现方必须遵守的契约)
- 级别:table
- 里程碑:M1.5
- 前置:无
- 实现:`crates/venue-binance/tests/session.rs::bin_062_login_result`

| 行 | payload | 期望 `LoginResult` |
|---|---|---|
| BIN-062.1 | `{"id":"l0000000000000000","status":200,"result":{"apiKey":"TESTKEY","authorizedSince":1790000000001,"connectedSince":1790000000000}}` | `Ok` |
| BIN-062.2 | `{"id":"l0000000000000000","status":401,"error":{"code":-1022,"msg":"Signature for this request is not valid."}}` | `Rejected(0xFFFFFC02)` |
| BIN-062.3 | `{"id":"l0000000000000000","status":400,"error":{"code":-1021,"msg":"Timestamp for this request is outside of the recvWindow."}}` | `Rejected(0xFFFFFC03)` |
| BIN-062.4 | `{"id":"l0000000000000000","status":503}` | `Rejected(0x200001F7)` |
| BIN-062.5 | `{"id":"p0000002a00000007","status":200,…}` | `NotMine` |
| BIN-062.6 | `pong`(非 JSON) | `NotMine` |

### BIN-063 签名材料不进入模板、上下文或输出以外的任何位置

- 规则:[hft-venue-proto 契约 7](../hft-venue-proto/README.md#实现方必须遵守的契约)、[下单:WS API](README.md#下单ws-api)
- 级别:unit
- 里程碑:M3
- 前置:`ED1`;HMAC REST 测试密钥 `HM1 = "NhqPtmdSJYdKjVHjA7PZj4Mge3R5YNiP1e3UZjInClVN65XAbvqqM6A7H5fATj0j"`;`RD-BIN`
- 步骤:
  1. `login`、`build_templates`、`session_identity_request`、对 Config 序列每步 `acct_request`
  2. 把 `Templates`、`GwCtx`、`FeedCtx`、`AcctCtx`、`AcctScratch` 的内存按字节扫描
  3. `encode` 一条 Place
- 期望:
  - 步骤 2 中找不到 `ED1` 私钥的 32 字节、`HM1` 的 64 字节,也找不到它们的 base64/十六进制文本
  - 步骤 3 的帧中不含 `"apiKey"` 与 `"signature"` 子串
- 实现:`crates/venue-binance/tests/session.rs::bin_063_no_key_material_retained`

### BIN-064 Usdm Place 编码为固定的 order.place 帧

- 规则:[下单:WS API](README.md#下单ws-api)、[4. 订单帧模板](README.md#4-订单帧模板)
- 级别:unit
- 里程碑:M1.5
- 前置:`RD-BIN`;`build_templates(&cfg{stale_guard_ms: 1500}, rd, 2)`;`WC`;`c = GwCmd{op: Place, inst: 1042, slot: 42, gen: 7, side: Buy, tif: Ioc, px: 6500010, qty: 400000, oflags: 0, account: 2}`;gw 生成的 client id = `CID(3,42,7)` = `1000301020002a000000073d`
- 步骤:
  1. `encode(rd, &t, &c, &tgt, &WC, &mut body)`(`body` 2048 B)
- 期望:
  - 返回 `Ok(n)`,`body[..n]` 逐字节等于 `{"id":"p0000002a00000007","method":"order.place","params":{"symbol":"BTCUSDT","side":"BUY","type":"LIMIT","timeInForce":"IOC","price":"65000.1","quantity":"0.004","newClientOrderId":"1000301020002a000000073d","newOrderRespType":"ACK","timestamp":1790000000000,"recvWindow":1500}}`
  - 帧中没有 `reduceOnly`、`apiKey`、`signature`
- 实现:`crates/venue-binance/tests/encode.rs::bin_064_usdm_place_frame`

### BIN-065 Usdm Place 的 side、tif 与 reduceOnly 变体

- 规则:[下单:WS API](README.md#下单ws-api)、[4. 订单帧模板](README.md#4-订单帧模板)、[hft-venue-proto 契约 11](../hft-venue-proto/README.md#实现方必须遵守的契约)
- 级别:table
- 里程碑:M1.5
- 前置:同 BIN-064,每行只改一处;断言帧中对应字段(其余字段同 BIN-064)
- 实现:`crates/venue-binance/tests/encode.rs::bin_065_usdm_place_variants`

| 行 | 改动 | 期望 |
|---|---|---|
| BIN-065.1 | `side = Sell` | `"side":"SELL"` |
| BIN-065.2 | `tif = Gtc` | `"timeInForce":"GTC"` |
| BIN-065.3 | `tif = Fok` | `"timeInForce":"FOK"` |
| BIN-065.4 | `tif = PostOnly` | `"timeInForce":"GTX"`,`"type":"LIMIT"` |
| BIN-065.5 | `oflags = REDUCE_ONLY` | `"quantity":"0.004","reduceOnly":"true","newClientOrderId":…` |
| BIN-065.6 | `oflags = RISK_REDUCING`(不含 `REDUCE_ONLY`) | 帧中没有 `reduceOnly` |
| BIN-065.7 | `oflags = SMOKE` | 帧除 `id`、`newClientOrderId` 外与 `oflags = 0` 时逐字节相同 |

### BIN-066 Spot Place 的类型、tif、STP 与 reduceOnly 拒绝

- 规则:[下单:WS API](README.md#下单ws-api)、[hft-venue-proto 契约 11](../hft-venue-proto/README.md#实现方必须遵守的契约)
- 级别:table
- 里程碑:M1.5
- 前置:`RD-BIN`;`Spot` 模板;`c = GwCmd{op: Place, inst: 2001, slot: 42, gen: 7, side: Buy, tif: Gtc, px: 6500010, qty: 100000, oflags: 0}`;client id = `CID(3,42,7)`(Spot)= `1000302020002a000000070d`
- 实现:`crates/venue-binance/tests/encode.rs::bin_066_spot_place_variants`

| 行 | 改动 | 期望 |
|---|---|---|
| BIN-066.1 | 无 | `{"id":"p0000002a00000007","method":"order.place","params":{"symbol":"BTCUSDT","side":"BUY","type":"LIMIT","timeInForce":"GTC","price":"65000.1","quantity":"0.001","newClientOrderId":"1000302020002a000000070d","newOrderRespType":"ACK","selfTradePreventionMode":"EXPIRE_MAKER","timestamp":1790000000000,"recvWindow":1500}}` |
| BIN-066.2 | `tif = Ioc` | `"timeInForce":"IOC"` |
| BIN-066.3 | `tif = Fok` | `"timeInForce":"FOK"` |
| BIN-066.4 | `tif = PostOnly` | `"type":"LIMIT_MAKER"`,帧中没有 `timeInForce` |
| BIN-066.5 | `oflags = REDUCE_ONLY` | `Err(EncodeErr::Unsupported(GwOp::Place))`,不写出帧 |
| BIN-066.6 | `oflags = REDUCE_ONLY | RISK_REDUCING` | `Err(EncodeErr::Unsupported(GwOp::Place))` |

### BIN-067 价格与数量格式化为最短十进制

- 规则:[4. 订单帧模板](README.md#4-订单帧模板)
- 级别:table
- 里程碑:M1.5
- 前置:`RD-BIN`(BTC `px_exp = −2`,`qty_exp = −8`);在 BIN-064 的帧中读取 `price`/`quantity` 字符串
- 实现:`crates/venue-binance/tests/encode.rs::bin_067_shortest_decimal`

| 行 | `px` / `qty` raw | 期望 `price` / `quantity` |
|---|---|---|
| BIN-067.1 | 6500010 / 400000 | `"65000.1"` / `"0.004"` |
| BIN-067.2 | 6500000 / 100000000 | `"65000"` / `"1"` |
| BIN-067.3 | 1 / 1 | `"0.01"` / `"0.00000001"` |
| BIN-067.4 | 6500099 / 123456789 | `"65000.99"` / `"1.23456789"` |
| BIN-067.5 | 9223372036854775807 / 9223372036854775807 | `"92233720368547758.07"` / `"92233720368.54775807"` |

### BIN-068 Cancel 一律按 origClientOrderId 撤单

- 规则:[下单:WS API](README.md#下单ws-api)
- 级别:table
- 里程碑:M1.5
- 前置:`RD-BIN`;`WC`;`stale_guard_ms = 1500`;`c = GwCmd{op: Cancel, inst: 1042(Spot 行用 2001), target_slot: 42}`;`tgt = TargetRef{id: CID(3,42,7), exch_order_id: <列>}`
- 实现:`crates/venue-binance/tests/encode.rs::bin_068_cancel_frame`

| 行 | 产品线 / `exch_order_id` | 期望帧 |
|---|---|---|
| BIN-068.1 | `Usdm` / 325078477 | `{"id":"c0000002a00000007","method":"order.cancel","params":{"symbol":"BTCUSDT","origClientOrderId":"1000301020002a000000073d","timestamp":1790000000000,"recvWindow":1500}}` |
| BIN-068.2 | `Usdm` / 0 | 与 .1 逐字节相同(不返回 `UnknownTarget`) |
| BIN-068.3 | `Spot` / 0 | 同 .1 形,`symbol` 与 `origClientOrderId` 取 Spot 值,帧中没有 `orderId` |
| BIN-068.4 | `Usdm`,`tgt.id = CID(2,42,7)`(上一 epoch,MirrorRestore 恢复的槽) | `"origClientOrderId":"1000201020002a000000072d"` |

### BIN-069 Usdm Amend 编码为 order.modify

- 规则:[下单:WS API](README.md#下单ws-api)
- 级别:unit
- 里程碑:M1.5
- 前置:`RD-BIN`;`c = GwCmd{op: Amend, inst: 1042, target_slot: 42, side: Buy, tif: Gtc, px: 6500020, qty: 300000}`;`tgt = TargetRef{id: CID(3,42,7), exch_order_id: 325078477}`;`WC`
- 步骤:
  1. `encode`
- 期望:
  - 帧 = `{"id":"m0000002a00000007","method":"order.modify","params":{"symbol":"BTCUSDT","side":"BUY","origClientOrderId":"1000301020002a000000073d","price":"65000.2","quantity":"0.003","timestamp":1790000000000,"recvWindow":1500}}`
- 实现:`crates/venue-binance/tests/encode.rs::bin_069_usdm_modify`

### BIN-070 Spot Amend 编码为 order.amend.keepPriority,只写新数量

- 规则:[下单:WS API](README.md#下单ws-api)
- 级别:table
- 里程碑:M1.5
- 前置:`RD-BIN`;`Spot`;`tgt = TargetRef{id: CID(3,42,7)(Spot), exch_order_id: 0}`;`WC`
- 实现:`crates/venue-binance/tests/encode.rs::bin_070_spot_amend_keep_priority`

| 行 | `GwCmd` | 期望 |
|---|---|---|
| BIN-070.1 | `{op: Amend, inst: 2001, target_slot: 42, px: 6500010, qty: 50000}` | `{"id":"m0000002a00000007","method":"order.amend.keepPriority","params":{"symbol":"BTCUSDT","origClientOrderId":"1000302020002a000000070d","newQty":"0.0005","timestamp":1790000000000,"recvWindow":1500}}` |
| BIN-070.2 | 同上但 `px = 6500020` | 帧与 .1 逐字节相同(帧中不含 `price`) |
| BIN-070.3 | 任一 `GwOp` | 帧中从不出现 `order.cancelReplace` |

### BIN-071 Query 与 CancelAll(Spot)的帧

- 规则:[下单:WS API](README.md#下单ws-api)
- 级别:table
- 里程碑:M1.5
- 前置:`RD-BIN`;`WC`;`stale_guard_ms = 1500`
- 实现:`crates/venue-binance/tests/encode.rs::bin_071_query_cancel_all_frames`

| 行 | 产品线 / `GwCmd` / `TargetRef` | 期望帧 |
|---|---|---|
| BIN-071.1 | `Usdm` / `{op: Query, inst: 1042, target_slot: 42}` / `{CID(3,42,7), 0}` | `{"id":"q0000002a00000007","method":"order.status","params":{"symbol":"BTCUSDT","origClientOrderId":"1000301020002a000000073d","timestamp":1790000000000,"recvWindow":1500}}` |
| BIN-071.2 | `Spot` / `{op: Query, inst: 2001, target_slot: 42}` / `{CID(3,42,7)(Spot), 0}` | 同形,Spot 的 symbol 与 client id |
| BIN-071.3 | `Spot` / `{op: CancelAll, inst: 2001, slot: 0}` / — | `{"id":"x0000000000000000","method":"openOrders.cancelAll","params":{"symbol":"BTCUSDT","timestamp":1790000000000,"recvWindow":1500}}` |

### BIN-072 Usdm CancelAll 生成按 symbol 的签名 REST 全撤请求

- 规则:[下单:WS API](README.md#下单ws-api)、[账户配置核验](README.md#账户配置核验)
- 级别:unit
- 里程碑:M3
- 前置:`RD-BIN`;HMAC REST 密钥 `HM1`,`apiKey = "TESTKEY"`;`WC`;`stale_guard_ms = 1500`;`c = GwCmd{op: CancelAll, account: 2, inst: 1042}`
- 步骤:
  1. `Usdm::order_channel(CancelAll)`;`Usdm::encode_rest(rd, &t, &c, &tgt, &WC, &mut out)`
- 期望:
  - `RestReqBuf{endpoint: 0, method: DELETE, path_query: "/fapi/v1/allOpenOrders?symbol=BTCUSDT&timestamp=1790000000000&recvWindow=1500&signature=381745f38cbbfe1f631e935ec6dbf44fe2f5925a7aafd70800b6b6204c139152", headers 含 "X-MBX-APIKEY: TESTKEY", redacted: true}`
  - `order_channel` 为 `Rest`;不产出 WS 帧
- 实现:`crates/venue-binance/tests/encode.rs::bin_072_usdm_cancel_all_rest`
- 备注:请求由 `encode_rest` 生成(`order_channel(CancelAll) = Rest`);`encode_rest` 以 `inst = 0` 调用返回 `Err(EncodeErr::Unsupported(GwOp::CancelAll))`

### BIN-073 encode 的失败路径不写出半帧

- 规则:[4. 订单帧模板](README.md#4-订单帧模板)、[hft-venue-proto 契约 5](../hft-venue-proto/README.md#实现方必须遵守的契约)
- 级别:table
- 里程碑:M1.5
- 前置:`RD-BIN`;BIN-064 的 `c`;`body` 默认 2048 B
- 实现:`crates/venue-binance/tests/encode.rs::bin_073_encode_errors`

| 行 | 改动 | 期望 |
|---|---|---|
| BIN-073.1 | `c.inst = 2001`(Spot 的 inst)交给 `Usdm` | `Err(UnknownInst)` |
| BIN-073.2 | `c.inst = 9999`(refdata 不存在) | `Err(UnknownInst)` |
| BIN-073.3 | `body` 长度 = 帧长 − 1 | `Err(FieldOverflow)` |
| BIN-073.4 | `body` 长度 = 帧长 | `Ok(帧长)` |
| BIN-073.5 | `c.op = DeadManRefresh`(维护类)交给 `encode` | `Err(Unsupported(DeadManRefresh))` |

### BIN-074 encode 输出可解析且字段与 GwCmd 一致

- 规则:[4. 订单帧模板](README.md#4-订单帧模板)
- 级别:prop
- 里程碑:M1.5
- 性质:任意合法 `GwCmd`(Place/Cancel/Amend/Query,两个产品线)编码后的帧能被 `sonic-rs` 解析;`symbol`、`side`、`timeInForce`/`type`、`price`、`quantity`/`newQty` 与按 hft-types 十进制参考格式化的值相同;`id` 解回 `(op, slot, gen)`;帧长 ≤ 512 B
- 生成:inst 取 `RD-BIN` 全部 inst;`px` ∈ [1, 10¹⁵] 且为 tick 整数倍;`qty` ∈ [1, 10¹⁵] 且为 lot 整数倍;`slot` ∈ [0, 2²⁰);`gen` ∈ u32;side、tif、oflags(Spot 不含 `REDUCE_ONLY`)随机
- 参考:`serde_json` 解析(dev-dependency)与十进制参考实现
- 例数:PR 2 000;每夜 100 000
- 实现:`crates/venue-binance/tests/prop_encode.rs::bin_074_encode_parses_back`

### BIN-075 timestamp 取 WallClock,recvWindow 取 stale_guard_ms

- 规则:[4. 订单帧模板](README.md#4-订单帧模板)、[下单:WS API](README.md#下单ws-api)
- 级别:table
- 里程碑:M1.5
- 前置:`RD-BIN`;BIN-064 的 `c`
- 实现:`crates/venue-binance/tests/encode.rs::bin_075_timestamp_recv_window`

| 行 | `WallClock.unix_ms` / `stale_guard_ms` | 期望 |
|---|---|---|
| BIN-075.1 | 1790000000000 / 1500 | `"timestamp":1790000000000,"recvWindow":1500` |
| BIN-075.2 | 1790000000001 / 1500 | `"timestamp":1790000000001` |
| BIN-075.3 | 1790000000000 / 1000 | `"recvWindow":1000` |

### BIN-076 不支持批量编码,也不逐单签名

- 规则:[已知限制与后续工作](README.md#已知限制与后续工作)、[下单:WS API](README.md#下单ws-api)
- 级别:unit
- 里程碑:M1.5
- 前置:`RD-BIN`;两条 BIN-064 形状的 Place
- 步骤:
  1. `Usdm::encode_batch(rd, &t, &[c1, c2], &[tgt1, tgt2], &WC, &mut body)`;`Spot` 同样调用
  2. `needs_per_order_signing()`
- 期望:
  - 步骤 1 两者都返回 `Err(Unsupported(GwOp::Place))`
  - 步骤 2 两者都返回 `false`
- 实现:`crates/venue-binance/tests/encode.rs::bin_076_no_batch_no_per_order_sign`

### BIN-077 请求 id 的编码与还原

- 规则:[下单:WS API](README.md#下单ws-api)
- 级别:table
- 里程碑:M1.5
- 前置:`decode_report` 读响应的 `id`(其余字段为最小合法 200 响应);"还原"指 `ReportOutcome.req`
- 实现:`crates/venue-binance/tests/req_id.rs::bin_077_request_id`

| 行 | `id` | 期望 |
|---|---|---|
| BIN-077.1 | `"p0000002a00000007"` | `req = Some(ReqRef{slot: 42, gen: 7, op: Place})` |
| BIN-077.2 | `"c0000002a00000007"` | `Some(ReqRef{42, 7, Cancel})` |
| BIN-077.3 | `"m0000002a00000007"` | `Some(ReqRef{42, 7, Amend})` |
| BIN-077.4 | `"q0000002a00000007"` | `Some(ReqRef{42, 7, Query})` |
| BIN-077.5 | `"x0000000000000000"` | `Some(ReqRef{0, 0, CancelAll})` |
| BIN-077.6 | `"pffffffffffffffff"` | `Some(ReqRef{0xFFFFFFFF, 0xFFFFFFFF, Place})` |
| BIN-077.7 | `"p0000002a0000007"`(16 字符) | 类为 `Unknown`,`req = None` |
| BIN-077.8 | `"p0000002A00000007"`(大写十六进制) | 类为 `Unknown`,`req = None` |
| BIN-077.9 | `"z0000002a00000007"` | 类为 `Unknown`,`req = None` |

### BIN-078 client id 渲染为 24 字节规范文本,只认我们的格式

- 规则:[使用本组件](README.md#使用本组件)、[hft-venue-proto 契约 13](../hft-venue-proto/README.md#实现方必须遵守的契约)
- 级别:table
- 里程碑:M1.5
- 前置:`Usdm`;`render_cid(&CID(3,42,7), &mut buf)`;`parse_cid` 的输入如列
- 实现:`crates/venue-binance/tests/cid.rs::bin_078_cid_shape`

| 行 | 输入 | 期望 |
|---|---|---|
| BIN-078.1 | `render_cid(CID(3,42,7))` | 返回 24,`buf[..24] = "1000301020002a000000073d"` |
| BIN-078.2 | `parse_cid("1000301020002a000000073d")` | `Some(ClientOrderId{epoch: 3, venue: 1, account: 2, slot: 42, gen: 7})` |
| BIN-078.3 | `parse_cid("1000301020002a000000073c")`(校验位错) | `None` |
| BIN-078.4 | `parse_cid("1000301020002A000000073D")`(大写) | `None` |
| BIN-078.5 | `parse_cid("1000301020002a000000073d0")`(25 字节) | `None` |
| BIN-078.6 | `parse_cid("1800301020002a0000000735")`(保留位为 1) | `None` |
| BIN-078.7 | `parse_cid("autoclose-1790000000000")` | `None` |
| BIN-078.8 | `parse_cid("web_8fa3b1")` | `None` |
| BIN-078.9 | `parse_cid("")` | `None` |

### BIN-079 client id 往返与交易所字符集

- 规则:[使用本组件](README.md#使用本组件)
- 级别:prop
- 里程碑:M1.5
- 性质:任意 `ClientOrderId`(epoch bit 15 = 0、slot < 2²⁰)经 `render_cid` 后满足 `^[\.A-Z\:/a-z0-9_-]{1,36}$`、长度恰为 24,且 `parse_cid(render_cid(x)) == Some(x)`;两个产品线相同
- 生成:epoch ∈ [0, 2¹⁵);venue ∈ {1, 2};account ∈ u8;slot ∈ [0, 2²⁰);gen ∈ u32
- 参考:hft-types `ClientOrderId::encode/decode`
- 例数:PR 10 000;每夜 1 000 000
- 实现:`crates/venue-binance/tests/cid.rs::bin_079_cid_roundtrip`

### BIN-080 Usdm 会话维护请求为签名的 listenKey PUT,Spot 不需要

- 规则:[6. 死手与会话维护](README.md#6-死手与会话维护)
- 级别:table
- 里程碑:M3
- 前置:`RD-BIN`;`HM1`;`WC`
- 实现:`crates/venue-binance/tests/session.rs::bin_080_session_keepalive_request`

| 行 | 调用 | 期望 |
|---|---|---|
| BIN-080.1 | `Usdm::session_keepalive_request(rd, &t, &WC, &mut out)` | `Ok(true)`;`RestReqBuf{endpoint: 0, method: PUT, path_query: "/fapi/v1/listenKey"}`,头含 `X-MBX-APIKEY: TESTKEY` |
| BIN-080.2 | `Spot::session_keepalive_request(…)` | `Ok(false)`;`out` 不变 |

### BIN-081 decode_rest:全撤、死手与会话维护的 REST 响应

- 规则:[下单:WS API](README.md#下单ws-api)、[6. 死手与会话维护](README.md#6-死手与会话维护)、[结果未知码表](README.md#结果未知码表)
- 级别:table
- 里程碑:M3
- 前置:`RD-BIN`;`Usdm`;`RestOrigin` 按行;`RX`
- 实现:`crates/venue-binance/tests/rest.rs::bin_081_decode_rest`

| 行 | `RestOrigin.what` / status / body | 期望 |
|---|---|---|
| BIN-081.1 | `Order`(`ReqRef{0, 0, CancelAll}`)/ 200 / `{"code":200,"msg":"The operation of cancel all open order is done."}` | 零事件;`raw_code = 0`;`ctl = None` |
| BIN-081.2 | `Order` / 429 / `{"code":-1003,…}` | 零事件;`raw_code = 0xFFFFFC15`;`ctl = RateLimited` |
| BIN-081.3 | `DeadMan` / 200 / `{"symbol":"BTCUSDT","countdownTime":"30000"}` | 零事件;`ctl = None` |
| BIN-081.4 | `Keepalive` / 200 / `{"listenKey":"lk0123"}` | 零事件;`ctl = None` |
| BIN-081.5 | `Keepalive` / 400 / `{"code":-1125,"msg":"This listenKey does not exist."}` | 零事件;`raw_code = 0xFFFFFB9B`;`ctl = UserStreamEnded` |
| BIN-081.6 | `Order` / 200 / `[`(截断) | `Err(DecodeErr::Truncated)` |

### BIN-090 Usdm 下单 ACK 响应解码为 Ack 并登记交易所订单 id

- 规则:[5. 回报解码](README.md#5-回报解码)、[hft-venue-proto 契约 13](../hft-venue-proto/README.md#实现方必须遵守的契约)
- 级别:unit
- 里程碑:M1.5
- 前置:`RD-BIN`;全新 `GwCtx`;`RX`(`account = 2`);payload `{"id":"p0000002a00000007","status":200,"result":{"orderId":325078477,"symbol":"BTCUSDT","status":"NEW","clientOrderId":"1000301020002a000000073d","updateTime":1790000000010},"rateLimits":[{"rateLimitType":"ORDERS","interval":"SECOND","intervalNum":10,"limit":300,"count":12},{"rateLimitType":"ORDERS","interval":"MINUTE","intervalNum":1,"limit":1200,"count":40},{"rateLimitType":"REQUEST_WEIGHT","interval":"MINUTE","intervalNum":1,"limit":2400,"count":5}]}`
- 步骤:
  1. `decode_report(rd, ctx, WsResp, payload, &RX, &mut sink)`
- 期望:
  - 一条 `OrderEvent{what: Ack, venue: 1, account: 2, epoch: 3, slot: 42, gen: 7, inst: 1042, exch_order_id: 325078477, cum_filled: 0, avg_px: 0, reject_code: 0}`
  - `sink.exch_id` 被调用一次:`key = "325078477"`,`owner = Some(CID(3,42,7))`
  - `ReportOutcome{req: Some(ReqRef{42, 7, Place}), rate: Some(RateObs{bucket: Orders, used: 12, limit: 300, window_ms: 10000}), events: 1, outcome_unknown: None, raw_code: 0, skip: No, hint: None, fee_rule_deviation: false}`
- 实现:`crates/venue-binance/tests/reports.rs::bin_090_usdm_place_ack`

### BIN-091 rateLimits 选利用率最高的一条

- 规则:[限速默认值(`rate_rules()`,生效值以 refdata 为准)](README.md#限速默认值rate_rules生效值以-refdata-为准)
- 级别:table
- 里程碑:M1.5
- 前置:BIN-090 的 payload,只替换 `rateLimits`
- 实现:`crates/venue-binance/tests/reports.rs::bin_091_rate_limits_pick`

| 行 | `rateLimits` | 期望 `ReportOutcome.rate` |
|---|---|---|
| BIN-091.1 | ORDERS 10 s 12/300、ORDERS 1 min 40/1200、WEIGHT 1 min 5/2400 | `RateObs{Orders, 12, 300, 10000}` |
| BIN-091.2 | ORDERS 10 s 12/300、WEIGHT 1 min 2000/2400 | `RateObs{Weight, 2000, 2400, 60000}` |
| BIN-091.3 | ORDERS 10 s 30/300、ORDERS 1 min 120/1200(两者都是 0.1) | `RateObs{Orders, 30, 300, 10000}`(窗口短者) |
| BIN-091.4 | 只有 `RAW_REQUESTS` 1 条 | `None` |
| BIN-091.5 | 字段缺失 | `None`,事件照常产出 |
| BIN-091.6 | ORDERS `interval = "DAY"`,`intervalNum = 1`,100/200000 | `RateObs{Orders, 100, 200000, 86400000}` |

### BIN-092 错误码按两张表映射为 Reject 或结果未知

- 规则:[结果未知码表](README.md#结果未知码表)、[5. 回报解码](README.md#5-回报解码)、[hft-venue-proto 契约 6](../hft-venue-proto/README.md#实现方必须遵守的契约)、[hft-venue-proto 契约 8](../hft-venue-proto/README.md#实现方必须遵守的契约)
- 级别:table
- 里程碑:M1.5
- 前置:`RD-BIN`;`Usdm`;响应 `{"id":"p0000002a00000007","status":<st>,"error":{"code":<code>,"msg":"…"}}`(只有状态的行没有 `error`);"Reject" = 恰好一条 `OrderEvent{what: Reject, slot: 42, gen: 7, epoch: 0, reject_code: raw}`(错误响应不带 client id,README"已知限制与后续工作")且 `outcome_unknown = None`;"未知" = 零条 `OrderEvent`、`outcome_unknown = Some(ReqRef{42, 7, Place})`;两种都有 `ReportOutcome.raw_code = raw`
- 实现:`crates/venue-binance/tests/codes.rs::bin_092_error_code_table`

| 行 | status / code | 期望 |
|---|---|---|
| BIN-092.1 | 400 / −1013 | Reject,raw `0xFFFFFC0B` |
| BIN-092.2 | 400 / −1021 | Reject,raw `0xFFFFFC03`;`ctl = ClockSkew` |
| BIN-092.3 | 400 / −1111 | Reject,raw `0xFFFFFBA9` |
| BIN-092.4 | 400 / −2010 | Reject,raw `0xFFFFF826` |
| BIN-092.5 | 400 / −2019 | Reject,raw `0xFFFFF81D` |
| BIN-092.6 | 400 / −2022 | Reject,raw `0xFFFFF81A` |
| BIN-092.7 | 400 / −4164 | Reject,raw `0xFFFFEFBC` |
| BIN-092.8 | 400 / −5022 | Reject,raw `0xFFFFEC62` |
| BIN-092.9 | 400 / −4061 | Reject,raw `0xFFFFF023` |
| BIN-092.10 | 429 / −1003 | Reject,raw `0xFFFFFC15`;`ctl = RateLimited`;`rate = Some(RateObs{Weight, used = limit = 2400, window_ms: 60000})`(refdata 生效值) |
| BIN-092.11 | 429 / −1015 | Reject,raw `0xFFFFFC09`;`ctl = RateLimited`;`rate = Some(RateObs{Orders, used = limit = 300, window_ms: 10000})` |
| BIN-092.12 | 429 / 无业务码 | Reject,raw `0x200001AD`;`ctl = RateLimited`;`rate` 的 `Weight` 置满 |
| BIN-092.13 | 418 / 无业务码 | Reject,raw `0x200001A2`;`ctl = RateLimited`;`rate` 的 `Weight` 置满 |
| BIN-092.14 | 408 / −1007 | 未知,raw `0xFFFFFC11` |
| BIN-092.15 | 500 / −1001 | 未知,raw `0xFFFFFC17` |
| BIN-092.16 | 503 / −1008 | 未知,raw `0xFFFFFC10` |
| BIN-092.17 | 500 / 无业务码 | 未知,raw `0x200001F4` |
| BIN-092.18 | 502 / 无业务码 | 未知,raw `0x200001F6` |
| BIN-092.19 | 503 / 无业务码 | 未知,raw `0x200001F7` |
| BIN-092.20 | 400 / −9999(未登记) | 未知,raw `0xFFFFD8F1` |
| BIN-092.21 | 400 / −1121(未登记于两表) | 未知,raw `0xFFFFFB9F` |
| BIN-092.22 | 以上除 .2、.10–.13 外的每行 | `ctl = None` |
| BIN-092.22 | 同 .5 但交给 `Spot` | Reject,raw `0xFFFFF81D`(两表对两个产品线相同) |

### BIN-093 撤单 -2011 产出 CancelReject 且标明订单已不存在

- 规则:[结果未知码表](README.md#结果未知码表)
- 级别:unit
- 里程碑:M1.5
- 前置:`RD-BIN`;payload `{"id":"c0000002a00000007","status":400,"error":{"code":-2011,"msg":"Unknown order sent."}}`
- 步骤:
  1. `decode_report(rd, ctx, WsResp, payload, &RX, &mut sink)`
- 期望:
  - 一条 `OrderEvent{what: CancelReject, slot: 42, gen: 7, reject_code: 0xFFFFF825, detail: GONE}`
  - `ReportOutcome{req: Some(ReqRef{42, 7, Cancel}), outcome_unknown: None, raw_code: 0xFFFFF825}`;无 `Reject`
- 实现:`crates/venue-binance/tests/codes.rs::bin_093_cancel_unknown_order`

### BIN-094 查询响应解码为带累计量与均价的 QueryResult

- 规则:[下单:WS API](README.md#下单ws-api)、[累计量与费用来源](README.md#累计量与费用来源)、[hft-venue-proto 契约 9](../hft-venue-proto/README.md#实现方必须遵守的契约)
- 级别:table
- 里程碑:M1.5
- 前置:`RD-BIN`;`Usdm` 响应 `{"id":"q0000002a00000007","status":200,"result":{"orderId":325078477,"symbol":"BTCUSDT","status":<X>,"clientOrderId":"1000301020002a000000073d","price":"65000.1","origQty":"0.004","executedQty":<z>,"avgPrice":<ap>,…}}`;`Spot` 行用 `cummulativeQuoteQty`;每行断言一条 `OrderEvent{what: QueryResult, slot: 42, gen: 7, epoch: 3}` 的字段
- 实现:`crates/venue-binance/tests/reports.rs::bin_094_query_result`

| 行 | 产品线 / status / executedQty / avgPrice 或 cummulativeQuoteQty | 期望 `detail`(订单状态码)/ `cum_filled` / `avg_px` |
|---|---|---|
| BIN-094.1 | `Usdm` / `FILLED` / `"0.004"` / `"65000.15"` | Filled(3) / 400000 / 6500015 |
| BIN-094.2 | `Usdm` / `PARTIALLY_FILLED` / `"0.002"` / `"65000.10"` | PartiallyFilled(2) / 200000 / 6500010 |
| BIN-094.3 | `Usdm` / `NEW` / `"0"` / `"0.00"` | New(1) / 0 / 0 |
| BIN-094.4 | `Usdm` / `CANCELED` / `"0.001"` / `"65000.1"` | Cancelled(4) / 100000 / 6500010 |
| BIN-094.5 | `Usdm` / `EXPIRED` / `"0"` / `"0"` | Expired(6) / 0 / 0 |
| BIN-094.6 | `Usdm` / `EXPIRED_IN_MATCH` / `"0"` / `"0"` | Expired(6) / 0 / 0 |
| BIN-094.7 | `Usdm` / `FILLED` / `"0.004"` / `"0"`(均价缺值) | `Err(DecodeErr::Shape)`(`cum_filled > 0` 时 `avg_px = 0` 视为解码错误) |
| BIN-094.8 | `Spot` / `FILLED` / `"0.003"` / `"195.00"`(成交额) | Filled(3) / 300000 / 65000.00(6500000) |
| BIN-094.9 | `Spot` / `PARTIALLY_FILLED` / `"0.003"` / `"100.00"` | PartiallyFilled(2) / 300000 / 33333.33(3333333,HalfEven) |
| BIN-094.10 | `Spot` / `NEW` / `"0"` / `"0"` | New(1) / 0 / 0(不是除零错误) |

### BIN-095 查询 -2013 给出 NotFound 形态,从不给 Rejected

- 规则:[结果未知码表](README.md#结果未知码表)、[hft-venue-proto 契约 10](../hft-venue-proto/README.md#实现方必须遵守的契约)
- 级别:unit
- 里程碑:M1.5
- 前置:`RD-BIN`;`stale_guard() = RecvWindow`;payload `{"id":"q0000002a00000007","status":400,"error":{"code":-2013,"msg":"Order does not exist."}}`
- 步骤:
  1. `decode_report(rd, ctx, WsResp, payload, &RX, &mut sink)`
- 期望:
  - 一条 `OrderEvent{what: QueryResult, slot: 42, gen: 7, cum_filled: 0, avg_px: 0, reject_code: 0xFFFFF823}`,订单状态 NotFound(7)
  - 没有 `Reject`、没有 `Expired`;`outcome_unknown = None`
- 实现:`crates/venue-binance/tests/codes.rs::bin_095_query_not_found`
### BIN-096 撤单与改单的成功响应给出 CancelAck 与 AmendAck,现货全撤响应零事件

- 规则:[5. 回报解码](README.md#5-回报解码)、[下单:WS API](README.md#下单ws-api)
- 级别:table
- 里程碑:M1.5
- 前置:`RD-BIN`;全新 `GwCtx`;`RX`;每行一条 `status = 200` 的响应(响应体为官方文档示例的字段集,值按下表)
- 实现:`crates/venue-binance/tests/reports.rs::bin_096_cancel_amend_acks`

| 行 | 产品线 / 响应 | 期望 |
|---|---|---|
| BIN-096.1 | `Usdm` `order.cancel`,`id = c0000002a00000007`,`result{clientOrderId: CID(3,42,7), orderId: 325078477, status: CANCELED, symbol: BTCUSDT, updateTime: 1790000000020}` | 一条 `OrderEvent{what: CancelAck, epoch: 3, slot: 42, gen: 7, inst: 1042, exch_order_id: 325078477, ex_ts_ns: 1790000000020000000}`;`req = Some(ReqRef{42, 7, Cancel})`;不调用 `exch_id` |
| BIN-096.2 | `Spot` `order.cancel`,`result{origClientOrderId: CID(3,42,7)(Spot), clientOrderId: "91fe37ce9e69c90d6358c0", orderId: 12569099453, transactTime: 1790000000020000(µs)}` | 一条 `OrderEvent{what: CancelAck, epoch: 3, slot: 42, gen: 7, inst: 2001, exch_order_id: 12569099453, ex_ts_ns: 1790000000020000000}`(client id 取 `origClientOrderId`) |
| BIN-096.3 | `Usdm` `order.modify`,`id = m0000002a00000007`,`result{clientOrderId: CID(3,42,7), orderId: 325078477, status: NEW}` | 一条 `OrderEvent{what: AmendAck, epoch: 3, slot: 42, gen: 7}` |
| BIN-096.4 | `Spot` `openOrders.cancelAll`,`id = x0000000000000000`,`result` 为数组 | 零事件,`events = 0` |

### BIN-100 ORDER_TRADE_UPDATE 的执行类型映射为 OrderEvent 种类

- 规则:[5. 回报解码](README.md#5-回报解码)、[用户流事件表](README.md#用户流事件表)
- 级别:table
- 里程碑:M3
- 前置:`RD-BIN`;全新 `GwCtx`;样本 `OTU(x, X, c, l, z, t)` = `{"e":"ORDER_TRADE_UPDATE","E":1790000000011,"T":1790000000010,"o":{"s":"BTCUSDT","c":<c>,"S":"BUY","o":"LIMIT","f":"GTC","q":"0.004","p":"65000.1","ap":<ap>,"x":<x>,"X":<X>,"i":325078477,"l":<l>,"z":<z>,"L":"65000.1","n":"0.0026","N":"USDT","T":1790000000010,"t":<t>,…}}`;`c` 除注明外 = `CID(3,42,7)`
- 实现:`crates/venue-binance/tests/user_stream.rs::bin_100_otu_exec_type`

| 行 | x / X / 其他 | 期望 |
|---|---|---|
| BIN-100.1 | `NEW` / `NEW` / `l = "0"`,`z = "0"` | `OrderEvent{what: Ack, slot: 42, gen: 7, epoch: 3, exch_order_id: 325078477}` |
| BIN-100.2 | `TRADE` / `PARTIALLY_FILLED` / `l = "0.001"`,`z = "0.001"`,`t = 9001` | `PartialFill` |
| BIN-100.3 | `TRADE` / `FILLED` / `l = "0.003"`,`z = "0.004"`,`t = 9002` | `Fill` |
| BIN-100.4 | `CANCELED` / `CANCELED` | `CancelAck` |
| BIN-100.5 | `EXPIRED` / `EXPIRED`(IOC 剩余) | `Expired` |
| BIN-100.6 | `EXPIRED` / `EXPIRED`(GTX 会吃单) | `Expired` |
| BIN-100.7 | `AMENDMENT` / `NEW` | `AmendAck` |
| BIN-100.8 | `CALCULATED` / `FILLED` / `c = CID(3,42,7)` | `Err(DecodeErr::Shape)`;无事件 |
| BIN-100.9 | `CALCULATED` / `FILLED` / `c = "autoclose-1790000000000"` | 一条 `ExternalFill{cause: Liquidation}`,无 `OrderEvent` |
| BIN-100.10 | `TRADE` / `FILLED` 但 `X` 为未知值 `"FOO"` | `Err(DecodeErr::Shape)` |

### BIN-101 成交回报逐字段取自推送

- 规则:[5. 回报解码](README.md#5-回报解码)、[累计量与费用来源](README.md#累计量与费用来源)、[hft-venue-proto 契约 9](../hft-venue-proto/README.md#实现方必须遵守的契约)、[hft-venue-proto 契约 19](../hft-venue-proto/README.md#实现方必须遵守的契约)
- 级别:unit
- 里程碑:M3
- 前置:`RD-BIN`;全新 `GwCtx`;`OTU(TRADE, PARTIALLY_FILLED, CID(3,42,7), l = "0.001", z = "0.003", t = 9001)`,`ap = "65000.05"`,`L = "65000.1"`,`n = "0.0130"`,`N = "USDT"`;`RX`(`account = 2`)
- 步骤:
  1. `decode_report(rd, ctx, OrderTradeUpdate, payload, &RX, &mut sink)`
- 期望:
  - 一条 `OrderEvent{what: PartialFill, venue: 1, account: 2, epoch: 3, slot: 42, gen: 7, inst: 1042, px: 6500010, qty: 100000, cum_filled: 300000, avg_px: 6500005, fee: 0.013(1300000), fee_asset: 1(USDT), exch_order_id: 325078477, ex_ts_ns: 1790000000010000000, hw_rx_ns: 1790000000126000000, rx_tsc: 1000}`
  - `cum_filled` 取自 `z`,不是 `l` 的累加;`ReportOutcome{events: 1, fee_rule_deviation: false}`
- 实现:`crates/venue-binance/tests/user_stream.rs::bin_101_fill_fields`

### BIN-102 同一成交推送两次只产出一次成交增量

- 规则:[累计量与费用来源](README.md#累计量与费用来源)、[hft-venue-proto 契约 22](../hft-venue-proto/README.md#实现方必须遵守的契约)
- 级别:unit
- 里程碑:M3
- 前置:同 BIN-101;同一 `GwCtx`
- 步骤:
  1. 喂 BIN-101 的 payload
  2. 再喂同一 payload(模拟两条连接重叠)
- 期望:
  - 步骤 1 一条 `PartialFill`
  - 步骤 2 零事件,`ReportOutcome{events: 0, skip: No}`,返回 `Ok`
- 实现:`crates/venue-binance/tests/dedup.rs::bin_102_trade_dedup`

### BIN-103 两个 inst 的成交 id 相同时都产出

- 规则:[累计量与费用来源](README.md#累计量与费用来源)、[hft-venue-proto 契约 22](../hft-venue-proto/README.md#实现方必须遵守的契约)
- 级别:unit
- 里程碑:M3
- 前置:同一 `GwCtx`;推送 A = BTCUSDT 成交 `t = 9001`;推送 B = ETHUSDT 成交 `t = 9001`(client id `CID(3,43,1)`)
- 步骤:
  1. 依次喂 A、B
- 期望:
  - 两条成交事件:`inst = 1042` 与 `inst = 1043` 各一条
- 实现:`crates/venue-binance/tests/dedup.rs::bin_103_same_trade_id_two_insts`

### BIN-104 成交 id 缺失或不为正时按订单累计量去重

- 规则:[累计量与费用来源](README.md#累计量与费用来源)、[外部成交识别](README.md#外部成交识别)、[hft-venue-proto 契约 22](../hft-venue-proto/README.md#实现方必须遵守的契约)
- 级别:table
- 里程碑:M3
- 前置:同一 `GwCtx`;推送为 `x = CALCULATED`、`c = "autoclose-1790000000000"`、`i = 777`,按行喂入,`t`/`z` 如列
- 实现:`crates/venue-binance/tests/dedup.rs::bin_104_order_seq_dedup`

| 行 | 依次喂入 (`t`, `z`) | 期望 |
|---|---|---|
| BIN-104.1 | (0, `"0.002"`) | 一条 `ExternalFill{trade_id: 0, fill_seq: 200000}` |
| BIN-104.2 | 再喂 (0, `"0.002"`) | 零事件 |
| BIN-104.3 | 再喂 (0, `"0.004"`) | 一条 `ExternalFill{trade_id: 0, fill_seq: 400000}` |
| BIN-104.4 | 再喂 (−1, `"0.004"`) | 零事件(`t ≤ 0` 与 `t = 0` 同键) |
| BIN-104.5 | 字段 `t` 缺失,`z = "0.006"` | 一条 `ExternalFill{trade_id: 0, fill_seq: 600000}` |

### BIN-105 Usdm 与 Spot 都不产出 FeeOnly

- 规则:[累计量与费用来源](README.md#累计量与费用来源)、[hft-venue-proto 契约 12](../hft-venue-proto/README.md#实现方必须遵守的契约)
- 级别:unit
- 里程碑:M3
- 前置:`fixtures/binance-usdm/ORDER_TRADE_UPDATE/` 与 `fixtures/binance-spot/executionReport/` 的全部金样输入
- 步骤:
  1. 逐条 `decode_report`
- 期望:
  - 输出中 `what = FeeOnly` 的事件数 = 0;每条成交事件的 `fee_asset ≠ ASSET_NONE`
- 实现:`crates/venue-binance/tests/user_stream.rs::bin_105_no_fee_only`

### BIN-106 实际扣费资产与规则不符时置 fee_rule_deviation,事件内容不变

- 规则:[5. 回报解码](README.md#5-回报解码)、[VenueCaps 默认值](README.md#venuecaps-默认值)、[hft-venue-proto 契约 19](../hft-venue-proto/README.md#实现方必须遵守的契约)
- 级别:table
- 里程碑:M3
- 前置:`RD-BIN`;生效 `fee_asset_rule`:`Usdm` = `Quote`,`Spot` = `ReceivedAssetUnlessDiscount{BNB}`;每行一条成交推送,`n = "0.0001"`,`N` 如列;每行另断言事件的 `fee_asset` 等于 `N` 对应的 AssetId
- 实现:`crates/venue-binance/tests/fee.rs::bin_106_fee_rule_deviation`

| 行 | 产品线 / 方向 / `N` | 期望 `fee_rule_deviation` |
|---|---|---|
| BIN-106.1 | `Usdm` / Buy / `USDT` | `false` |
| BIN-106.2 | `Usdm` / Buy / `BNB` | `true` |
| BIN-106.3 | `Spot` 买 BTCUSDT / Buy / `BTC`(所得资产) | `false` |
| BIN-106.4 | `Spot` 买 BTCUSDT / Buy / `BNB` | `false` |
| BIN-106.5 | `Spot` 买 BTCUSDT / Buy / `USDT` | `true` |
| BIN-106.6 | `Spot` 卖 BTCUSDT / Sell / `USDT`(所得资产) | `false` |
| BIN-106.7 | `Spot` 卖 BTCUSDT / Sell / `BTC` | `true` |

### BIN-107 返佣以负费用原样带出

- 规则:[hft-venue-proto 契约 19](../hft-venue-proto/README.md#实现方必须遵守的契约)
- 级别:unit
- 里程碑:M3
- 前置:BIN-101 的推送,`n = "-0.0013"`
- 步骤:
  1. `decode_report`
- 期望:
  - `PartialFill.fee = −0.0013(−130000)`,`fee_asset = 1`
- 实现:`crates/venue-binance/tests/fee.rs::bin_107_negative_fee_rebate`

### BIN-108 外部成交按判定顺序识别

- 规则:[外部成交识别](README.md#外部成交识别)、[hft-venue-proto 契约 18](../hft-venue-proto/README.md#实现方必须遵守的契约)
- 级别:table
- 里程碑:M3
- 前置:`RD-BIN`;全新 `GwCtx`;`Usdm` 成交推送(`x = TRADE`,`X = FILLED`,`t = 9100 + 行号`),`c` 与 `o` 如列;每行断言恰好一条输出且另一类为零条
- 实现:`crates/venue-binance/tests/external.rs::bin_108_external_fill_order`
- 备注:依赖待核实:开发备忘第 2、3 条(前缀与 `o.o`/`x` 语义);核实前按本表

| 行 | `c` / `o.o` | 期望 |
|---|---|---|
| BIN-108.1 | `"autoclose-1790000000000"` / `LIMIT` | `ExternalFill{cause: Liquidation}` |
| BIN-108.2 | `"adl_autoclose"` / `LIMIT` | `ExternalFill{cause: Adl}` |
| BIN-108.3 | `"settlement_autoclose-1790"` / `LIMIT` | `ExternalFill{cause: Delivery}` |
| BIN-108.4 | `CID(3,42,7)` / `LIQUIDATION` | `ExternalFill{cause: Liquidation}`(标记优先于 client id) |
| BIN-108.5 | `"adl_autoclose"` / `LIQUIDATION` | `ExternalFill{cause: Adl}` |
| BIN-108.6 | `CID(3,42,7)` / `LIMIT` | `OrderEvent{what: Fill, slot: 42}`,无 `ExternalFill` |
| BIN-108.7 | `"web_8fa3b1"` / `LIMIT` | `ExternalFill{cause: Manual}` |
| BIN-108.8 | `"ios_12"` / `LIMIT` | `ExternalFill{cause: Manual}` |
| BIN-108.9 | `"and_12"` / `LIMIT` | `ExternalFill{cause: Manual}` |
| BIN-108.10 | `"x-abc123"` / `LIMIT` | `ExternalFill{cause: Unknown}` |
| BIN-108.11 | `""` / `LIMIT` | `ExternalFill{cause: Unknown}` |
| BIN-108.12 | 字段缺失 / `LIMIT` | `ExternalFill{cause: Unknown}` |
| BIN-108.13 | `CID(3,42,7)` 校验位错(`…073c`)/ `LIMIT` | `ExternalFill{cause: Unknown}` |
| BIN-108.14 | `Spot` `executionReport`,`c = "web_8fa3b1"` | `ExternalFill{cause: Manual, venue: 2}` |

### BIN-109 ExternalFill 的字段取自推送

- 规则:[外部成交识别](README.md#外部成交识别)
- 级别:table
- 里程碑:M3
- 前置:`RD-BIN`;`Usdm` 推送 `{"e":"ORDER_TRADE_UPDATE","T":1790000000500,"o":{"s":"BTCUSDT","c":"autoclose-1790000000000","S":"SELL","o":"LIQUIDATION","x":"CALCULATED","X":"FILLED","i":777,"l":"0.010","z":"0.010","L":"60000.5","n":"0.30001","N":"USDT","T":1790000000500,"t":<t>,…}}`
- 实现:`crates/venue-binance/tests/external.rs::bin_109_external_fill_fields`

| 行 | `t` | 期望 `ExternalFill` |
|---|---|---|
| BIN-109.1 | 55001 | `{venue: 1, account: 2, side: Sell, cause: Liquidation, inst: 1042, qty: 1000000, px: 6000050, fee: 30001000, fee_asset: 1, exch_order_id: 777, trade_id: 55001, ex_ts_ns: 1790000000500000000, fill_seq: 0}` |
| BIN-109.2 | 0 | 同上,`trade_id: 0`,`fill_seq: 1000000` |
| BIN-109.3 | −3 | 同上,`trade_id: 0`,`fill_seq: 1000000` |

### BIN-110 强平清算的余额变动只作余额校验,不并入 ExternalFill

- 规则:[外部成交识别](README.md#外部成交识别)、[用户流事件表](README.md#用户流事件表)
- 级别:unit
- 里程碑:M3
- 前置:`RD-BIN`;`ACCOUNT_UPDATE` 推送 `{"e":"ACCOUNT_UPDATE","E":1790000000600,"T":1790000000599,"a":{"m":"INSURANCE_CLEAR","B":[{"a":"USDT","wb":"990.0","cw":"990.0","bc":"-10.0"}],"P":[]}}`
- 步骤:
  1. `decode_report(rd, ctx, AccountUpdate, payload, &RX, &mut sink)`
- 期望:
  - 一条 `BalanceReport{asset: 1, free: 990(99000000000), locked: 0, ex_ts_ns: 1790000000599000000}`;零条 `ExternalFill`、零条 `OrderEvent`
  - `ReportOutcome.hint = MarginNow`
- 实现:`crates/venue-binance/tests/user_stream.rs::bin_110_insurance_clear_balance_only`
- 备注:依赖待核实:开发备忘第 2 条

### BIN-111 Spot executionReport 的映射、client id 取 c 或 C 与累计均价

- 规则:[5. 回报解码](README.md#5-回报解码)、[累计量与费用来源](README.md#累计量与费用来源)
- 级别:table
- 里程碑:M3
- 前置:`RD-BIN`;全新 `GwCtx`;推送 `{"subscriptionId":0,"event":{"e":"executionReport","E":1790000000700000,"s":"BTCUSDT","c":<c>,"S":"BUY","o":"LIMIT","f":"GTC","q":"0.00300","p":"65000.10","C":<C>,"x":<x>,"X":<X>,"i":88001,"l":<l>,"z":<z>,"L":"65000.10","n":"0.00000100","N":"BTC","T":1790000000699000,"t":<t>,"Z":<Z>,…}}`;`Spot` 时间单位 µs
- 实现:`crates/venue-binance/tests/user_stream.rs::bin_111_spot_execution_report`

| 行 | x / X / c / C / l / z / Z / t | 期望 |
|---|---|---|
| BIN-111.1 | `NEW` / `NEW` / `CID(3,42,7)` / `""` / 0 / 0 / 0 / −1 | `Ack{slot: 42, gen: 7, exch_order_id: 88001}` |
| BIN-111.2 | `TRADE` / `PARTIALLY_FILLED` / `CID(3,42,7)` / `""` / 0.001 / 0.001 / 65.0001 / 501 | `PartialFill{qty: 100000, cum_filled: 100000, avg_px: 6500010, px: 6500010, fee: 100, fee_asset: 2, ex_ts_ns: 1790000000699000000}` |
| BIN-111.3 | `TRADE` / `FILLED` / `CID(3,42,7)` / `""` / 0.002 / 0.003 / 195.00 / 502 | `Fill{cum_filled: 300000, avg_px: 6500000}` |
| BIN-111.4 | `CANCELED` / `CANCELED` / `"x-cxl-01"` / `CID(3,42,7)` / 0 / 0.001 / 65.0001 / −1 | `CancelAck{slot: 42, gen: 7}`(client id 取 `C`) |
| BIN-111.5 | `EXPIRED` / `EXPIRED` / `CID(3,42,7)` / `""` / 0 / 0 / 0 / −1 | `Expired` |
| BIN-111.6 | `REPLACED` / `NEW` / `CID(3,42,7)` / `""` / 0 / 0 / 0 / −1 | `AmendAck` |
| BIN-111.7 | `TRADE` / `FILLED` / `"web_77"` / `""` / 0.003 / 0.003 / 195.00 / 503 | `ExternalFill{cause: Manual, qty: 300000, trade_id: 503}`,无 `OrderEvent` |
| BIN-111.8 | `TRADE` / `FILLED` / `CID(3,42,7)` / `""` / 0.003 / 0.003 / `"abc"` / 504 | `Err(DecodeErr::Number)` |

### BIN-112 Usdm ACCOUNT_UPDATE 产出持仓与余额校验事件,并按原因给提示

- 规则:[用户流事件表](README.md#用户流事件表)
- 级别:table
- 里程碑:M3
- 前置:`RD-BIN`;推送 `{"e":"ACCOUNT_UPDATE","E":1790000000800,"T":1790000000799,"a":{"m":<m>,"B":[{"a":"USDT","wb":"1000.5","cw":"1000.5","bc":"0"}],"P":[{"s":"BTCUSDT","pa":"-0.004","ep":"65000.1","cr":"0","up":"-0.40","mt":"cross","iw":"0","ps":"BOTH"},{"s":"DOGEUSDT","pa":"100","ep":"0.1","cr":"0","up":"0","mt":"cross","iw":"0","ps":"BOTH"}]}}`
- 实现:`crates/venue-binance/tests/user_stream.rs::bin_112_account_update`

| 行 | `m` | 期望 |
|---|---|---|
| BIN-112.1 | `ORDER` | 一条 `PositionReport{inst: 1042, qty: −400000, entry_px: 6500010, upnl: −0.40(−400000), liq_px: 0, ex_ts_ns: 1790000000799000000}`(DOGEUSDT 跳过);一条 `BalanceReport{asset: 1, free: 100050000000, locked: 0}`;`hint = None` |
| BIN-112.2 | `MARGIN_TYPE_CHANGE` | 同 .1 的两条事件;`hint = ConfigRecheck` |
| BIN-112.3 | `ADMIN_DEPOSIT` | 同 .1;`hint = MarginNow` |
| BIN-112.4 | `ADMIN_WITHDRAW` | 同 .1;`hint = MarginNow` |
| BIN-112.5 | `DEPOSIT` | 同 .1;`hint = None` |
| BIN-112.6 | 缺失 | `Err(DecodeErr::Shape)` |

### BIN-113 全仓资金费推送产出 inst 为空的 FundingSettle

- 规则:[用户流事件表](README.md#用户流事件表)、[hft-venue-proto 契约 14](../hft-venue-proto/README.md#实现方必须遵守的契约)
- 级别:unit
- 里程碑:M3
- 前置:`RD-BIN`;生效 `funding_settle = BalanceOnlyPush`;推送 `FF1` = `{"e":"ACCOUNT_UPDATE","E":1790006400050,"T":1790006400000,"a":{"m":"FUNDING_FEE","B":[{"a":"USDT","wb":"999.65","cw":"999.65","bc":"-0.35"}],"P":[]}}`
- 步骤:
  1. `decode_report(rd, ctx, AccountUpdate, FF1, &RX, &mut sink)`
- 期望:
  - 恰好一条 `FundingSettle{venue: 1, account: 2, inst: 0(INST_NONE), amount: −0.35(−350000), rate: 0, pos_qty: 0, settle_ts_ns: 1790006400000000000, synth: 0}`;零条 `PositionReport`、`BalanceReport`
- 实现:`crates/venue-binance/tests/funding.rs::bin_113_cross_funding_fee`
- 备注:依赖待核实:开发备忘第 1 条

### BIN-114 资金费推送按 (结算时刻, 钱包余额) 去重

- 规则:[用户流事件表](README.md#用户流事件表)、[hft-venue-proto 契约 22](../hft-venue-proto/README.md#实现方必须遵守的契约)
- 级别:table
- 里程碑:M3
- 前置:同一 `GwCtx`;按行顺序喂入
- 实现:`crates/venue-binance/tests/funding.rs::bin_114_funding_dedup`

| 行 | 推送 | 期望 |
|---|---|---|
| BIN-114.1 | `FF1` | 一条 `FundingSettle` |
| BIN-114.2 | `FF1` 再一次 | 零条 |
| BIN-114.3 | `FF1` 但 `wb = "999.30"`、`bc = "-0.35"`(同一 `T`) | 一条 `FundingSettle{amount: −350000}` |
| BIN-114.4 | `FF1` 但 `T = 1790035200000` | 一条 `FundingSettle{settle_ts_ns: 1790035200000000000}` |

### BIN-115 资金费推送中 USDT 以外的资产不入金额

- 规则:[用户流事件表](README.md#用户流事件表)
- 级别:table
- 里程碑:M3
- 前置:`FF1` 的 `B` 改写如列
- 实现:`crates/venue-binance/tests/funding.rs::bin_115_funding_non_usdt`

| 行 | `B` | 期望 |
|---|---|---|
| BIN-115.1 | `[USDT bc "-0.35", BNB bc "-0.001"]` | 一条 `FundingSettle{amount: −350000}` |
| BIN-115.2 | `[BNB bc "-0.001"]`(无 USDT) | 零条 `FundingSettle`;返回 `Ok` |
| BIN-115.3 | `[USDT bc "abc"]` | `Err(DecodeErr::Number)` |

### BIN-116 逐仓资金费推送带持仓时产出带 inst 的 FundingSettle

- 规则:[用户流事件表](README.md#用户流事件表)
- 级别:unit
- 里程碑:M3
- 前置:`FF1` 加 `"P":[{"s":"BTCUSDT","pa":"0.004","ep":"65000.1","up":"0","mt":"isolated","iw":"10","ps":"BOTH"}]`
- 步骤:
  1. `decode_report`
- 期望:
  - 一条 `FundingSettle{inst: 1042, amount: −350000, settle_ts_ns: 1790006400000000000, synth: 0}`
- 实现:`crates/venue-binance/tests/funding.rs::bin_116_isolated_funding_fee`

### BIN-117 MARGIN_CALL 与 ACCOUNT_CONFIG_UPDATE 只给提示

- 规则:[用户流事件表](README.md#用户流事件表)、[账户配置核验](README.md#账户配置核验)、[保证金数据换算](README.md#保证金数据换算)
- 级别:table
- 里程碑:M3
- 前置:`RD-BIN`;每行断言零事件、`skip = No`
- 实现:`crates/venue-binance/tests/user_stream.rs::bin_117_hint_only_events`

| 行 | 推送 | 期望 `hint` |
|---|---|---|
| BIN-117.1 | `{"e":"MARGIN_CALL","E":1790000000900,"cw":"3.16","p":[…]}` | `MarginNow` |
| BIN-117.2 | `{"e":"ACCOUNT_CONFIG_UPDATE","E":1790000000900,"T":1790000000899,"ac":{"s":"BTCUSDT","l":25}}` | `ConfigRecheck` |
| BIN-117.3 | `{"e":"ACCOUNT_CONFIG_UPDATE","E":1790000000900,"T":1790000000899,"ai":{"j":true}}` | `ConfigRecheck` |
| BIN-117.4 | `{"e":"ACCOUNT_CONFIG_UPDATE","E":1790000000900,"T":1790000000899}`(两者都没有) | `None` |

### BIN-118 listenKeyExpired 与 eventStreamTerminated 不产出事件

- 规则:[用户流事件表](README.md#用户流事件表)、[6. 死手与会话维护](README.md#6-死手与会话维护)
- 级别:table
- 里程碑:M3
- 前置:`RD-BIN`
- 实现:`crates/venue-binance/tests/user_stream.rs::bin_118_stream_end_events`

| 行 | 推送 | 期望 |
|---|---|---|
| BIN-118.1 | `Usdm` `{"e":"listenKeyExpired","E":1790001800000,"listenKey":"lk0123"}` | 类 `ListenKeyExpired`;`decode_report` 返回 `Ok`,零事件,`skip = No`,`ctl = UserStreamEnded` |
| BIN-118.2 | `Spot` `{"subscriptionId":0,"event":{"e":"eventStreamTerminated","E":1790001800000000}}` | 类 `EventStreamTerminated`;`Ok`,零事件,`ctl = UserStreamEnded` |

### BIN-119 可忽略与未知用户流事件只计数,不是解码失败

- 规则:[用户流事件表](README.md#用户流事件表)、[1. 分派(`classify` / `classify_report`,只看前 64 B)](README.md#1-分派classify--classify_report只看前-64-b)、[hft-venue-proto 契约 20](../hft-venue-proto/README.md#实现方必须遵守的契约)
- 级别:table
- 里程碑:M3
- 前置:`RD-BIN` 的生效 `ignorable_user_events`:`Usdm` = 默认 4 个 + refdata 追加 `"BAR_EVENT"`;`Spot` = 默认 3 个;每行断言 `decode_report` 返回 `Ok(ReportOutcome{events: 0, …})`、无 `DecodeErr`
- 实现:`crates/venue-binance/tests/user_stream.rs::bin_119_ignorable_unknown`

| 行 | 产品线 / 事件名 | 期望 `skip` |
|---|---|---|
| BIN-119.1 | `Usdm` / `TRADE_LITE` | `Listed` |
| BIN-119.2 | `Usdm` / `STRATEGY_UPDATE` | `Listed` |
| BIN-119.3 | `Usdm` / `GRID_UPDATE` | `Listed` |
| BIN-119.4 | `Usdm` / `CONDITIONAL_ORDER_TRIGGER_REJECT` | `Listed` |
| BIN-119.5 | `Usdm` / `BAR_EVENT`(refdata 追加) | `Listed` |
| BIN-119.6 | `Usdm` / `FOO_EVENT` | `Unknown` |
| BIN-119.7 | `Spot` / `balanceUpdate` | `Listed` |
| BIN-119.8 | `Spot` / `listStatus` | `Listed` |
| BIN-119.9 | `Spot` / `externalLockUpdate` | `Listed` |
| BIN-119.10 | `Spot` / `newEvt` | `Unknown` |
| BIN-119.11 | `Usdm` / `TRADE_LITE` 但内容截断 | `Listed`(只取事件名,不解析内容) |

### BIN-120 已知类别的推送内容解析失败返回 DecodeErr

- 规则:[hft-venue-proto 契约 20](../hft-venue-proto/README.md#实现方必须遵守的契约)、[2. 解码](README.md#2-解码)
- 级别:table
- 里程碑:M3
- 前置:`RD-BIN`;BIN-101 的推送做一处改写
- 实现:`crates/venue-binance/tests/user_stream.rs::bin_120_known_class_decode_error`

| 行 | 改写 | 期望 |
|---|---|---|
| BIN-120.1 | `"z":"abc"` | `Err(Number)` |
| BIN-120.2 | 在 `"o":{` 内截断 | `Err(Truncated)` |
| BIN-120.3 | 删除 `"i"` | `Err(Shape)` |
| BIN-120.4 | `"s":"DOGEUSDT"`(不在 refdata) | `Err(Shape)` |

### BIN-121 Spot 余额推送每资产一条 BalanceReport

- 规则:[用户流事件表](README.md#用户流事件表)
- 级别:unit
- 里程碑:M3
- 前置:`RD-BIN`;推送 `{"subscriptionId":0,"event":{"e":"outboundAccountPosition","E":1790000001000000,"u":1790000000999000,"B":[{"a":"BTC","f":"0.01000000","l":"0.00300000"},{"a":"USDT","f":"1000.00","l":"0"},{"a":"XYZ","f":"5","l":"0"}]}}`
- 步骤:
  1. `decode_report(rd, ctx, OutboundAccountPosition, payload, &RX, &mut sink)`
- 期望:
  - 两条 `BalanceReport`:`{asset: 2, free: 1000000, locked: 300000, ex_ts_ns: 1790000000999000000}`、`{asset: 1, free: 100000000000, locked: 0}`;`XYZ`(不在 refdata)不产出
- 实现:`crates/venue-binance/tests/user_stream.rs::bin_121_spot_balance`
### BIN-140 Config 查询序列按产品线给出步骤、频率与名义权重

- 规则:[账户配置核验](README.md#账户配置核验)、[hft-venue-proto 契约 16](../hft-venue-proto/README.md#实现方必须遵守的契约)
- 级别:table
- 里程碑:M3
- 前置:`acct_queries(AcctQueryKind::Config)`;每行断言一条 `AcctQuery{kind: Config, step, every, push_fallback, weight}`;另断言 `Usdm` 表长 7、`Spot` 表长 3
- 实现:`crates/venue-binance/tests/acct.rs::bin_140_config_queries`
- 备注:依赖待核实:开发备忘第 5 条(端点与权重)

| 行 | 产品线 / step | 期望 端点 / `every` / `push_fallback` / `weight` |
|---|---|---|
| BIN-140.1 | `Usdm` / 0 | `GET /fapi/v1/positionSide/dual` / 1 / false / 30 |
| BIN-140.2 | `Usdm` / 1 | `GET /fapi/v1/multiAssetsMargin` / 1 / false / 30 |
| BIN-140.3 | `Usdm` / 2 | `GET /fapi/v1/symbolConfig` / 1 / false / 5 |
| BIN-140.4 | `Usdm` / 3 | `GET /fapi/v1/feeBurn` / 1 / false / 30 |
| BIN-140.5 | `Usdm` / 4 | `GET /fapi/v3/account` / 1 / false / 5 |
| BIN-140.6 | `Usdm` / 5 | `GET /api/v3/account` / 1 / false / 20 |
| BIN-140.7 | `Usdm` / 6 | `GET /sapi/v1/account/apiRestrictions` / 1 / false / 1 |
| BIN-140.8 | `Spot` / 0 | `GET /api/v3/account` / 1 / false / 20 |
| BIN-140.9 | `Spot` / 1 | `GET /sapi/v1/account/apiRestrictions` / 1 / false / 1 |
| BIN-140.10 | `Spot` / 2 | `GET /sapi/v1/bnbBurn` / 1 / false / 1 |

### BIN-141 Margin 查询序列:Usdm 三步、ADL 每 6 次一回,Spot 为空

- 规则:[保证金数据换算](README.md#保证金数据换算)、[hft-venue-proto 契约 17](../hft-venue-proto/README.md#实现方必须遵守的契约)
- 级别:table
- 里程碑:M3
- 前置:`acct_queries(AcctQueryKind::Margin)`
- 实现:`crates/venue-binance/tests/acct.rs::bin_141_margin_queries`
- 备注:依赖待核实:开发备忘第 6 条(权重与 ADL 更新周期)

| 行 | 产品线 / step | 期望 端点 / `every` / `weight` |
|---|---|---|
| BIN-141.1 | `Usdm` / 0 | `GET /fapi/v3/account` / 1 / 5 |
| BIN-141.2 | `Usdm` / 1 | `GET /fapi/v3/positionRisk` / 1 / 5 |
| BIN-141.3 | `Usdm` / 2 | `GET /fapi/v1/adlQuantile` / 6 / 5 |
| BIN-141.4 | `Spot` | 空表(长度 0) |

### BIN-142 签名 REST 请求的查询串、HMAC 签名与请求头

- 规则:[账户配置核验](README.md#账户配置核验)、[8. 源文件规划](README.md#8-源文件规划)
- 级别:table
- 里程碑:M3
- 前置:`RD-BIN`;HMAC REST 密钥 `HM1`,`apiKey = "TESTKEY"`;`WC`;`stale_guard_ms = 1500`;账户 2
- 实现:`crates/venue-binance/tests/sign.rs::bin_142_rest_hmac`

| 行 | 输入 | 期望 |
|---|---|---|
| BIN-142.1 | HMAC 函数,Binance 文档示例:密钥 `HM1`,串 `symbol=LTCBTC&side=BUY&type=LIMIT&timeInForce=GTC&quantity=1&price=0.1&recvWindow=5000&timestamp=1499827319559` | `c8db56825ae71d6d79447849e617115f4a920fa2acdcab2b053c4b2838bd6b71` |
| BIN-142.2 | `Usdm` `acct_request(Config step 0)` | `RestReqBuf{endpoint: 0, method: GET, path_query: "/fapi/v1/positionSide/dual?timestamp=1790000000000&recvWindow=1500&signature=cd74c1947d93de5156a92463317b9b8e3c0c89c2f52753b8baf66b1c2c1f133f", headers 含 "X-MBX-APIKEY: TESTKEY", redacted: true}` |
| BIN-142.3 | `Usdm` `acct_request(Config step 5)` | `endpoint: 1`,`path_query = "/api/v3/account?timestamp=1790000000000&recvWindow=1500&signature=cd74c1947d93de5156a92463317b9b8e3c0c89c2f52753b8baf66b1c2c1f133f"`,`redacted: true` |
| BIN-142.4 | `acct_request` 的签名 | 64 个小写十六进制字符(不是 base64、不是 Ed25519) |

### BIN-143 会话 key 的 UID 用 Ed25519 签名查询现货账户接口

- 规则:[账户配置核验](README.md#账户配置核验)、[hft-venue-proto 契约 23](../hft-venue-proto/README.md#实现方必须遵守的契约)
- 级别:unit
- 里程碑:M3
- 前置:`ED1`(`apiKey = "TESTKEY"`);`now_ms = 1790000000000`;`stale_guard_ms = 1500`
- 步骤:
  1. `Usdm::session_identity_request(&cfg, &ED1, now_ms, &mut out)`
  2. `Spot::session_identity_request(…)`
- 期望:
  - 步骤 1 返回 `Ok(true)`;`out = {endpoint: 1, method: GET, path_query: "/api/v3/account?timestamp=1790000000000&recvWindow=1500&signature=K7bvmAqJQoyYhTgBMqaX4BA7vIA8c8W1scUjhqYqNFkhYU6cb%2B7j1nTvn5sq2P0Gn3NbnQLQhmAkGeHTNmfQCg%3D%3D", headers 含 "X-MBX-APIKEY: TESTKEY", redacted: true}`
  - 步骤 2 返回 `Ok(true)`,同一 `path_query`,`endpoint: 0`
- 实现:`crates/venue-binance/tests/sign.rs::bin_143_session_identity_request`
- 备注:依赖待核实:开发备忘第 16 条(只开合约权限的 Ed25519 key 能否读现货账户)

### BIN-144 decode_identity 取出 UID 并右补零

- 规则:[账户配置核验](README.md#账户配置核验)、[hft-venue-proto 契约 23](../hft-venue-proto/README.md#实现方必须遵守的契约)
- 级别:table
- 里程碑:M3
- 前置:`uid` 为 24 字节缓冲,初值全 0xFF
- 实现:`crates/venue-binance/tests/acct.rs::bin_144_decode_identity`

| 行 | status / body | 期望 |
|---|---|---|
| BIN-144.1 | 200 / `{"makerCommission":10,"canTrade":true,"accountType":"SPOT","uid":"acct-0001",…}` | `Ok`;`uid = "acct-0001"` 后接 15 个 0 字节 |
| BIN-144.2 | 200 / `{"uid":354937868,…}`(数字形式) | `Ok`;`uid = "354937868"` 后接 15 个 0 |
| BIN-144.3 | 200 / 无 `uid` 字段 | `Err(Shape)` |
| BIN-144.4 | 401 / `{"code":-2015,"msg":"Invalid API-key, IP, or permissions for action."}` | `Err(Shape)` |
| BIN-144.5 | 200 / `uid` 长 25 字符 | `Err(Shape)` |

### BIN-145 Usdm 账户配置报告的各位按字段逐项置位

- 规则:[账户配置核验](README.md#账户配置核验)、[VenueCaps 默认值](README.md#venuecaps-默认值)、[hft-venue-proto 契约 16](../hft-venue-proto/README.md#实现方必须遵守的契约)
- 级别:table
- 里程碑:M3
- 前置:`RD-BIN`(Usdm 可交易 inst 1042、1043;`AccountMeta.venue_uid = "acct-0001"`;生效 caps 同 BIN-170);全新 `AcctCtx`;基线响应 `CFG0` = 第 0 步 `{"dualSidePosition":false}`、第 1 步 `{"multiAssetsMargin":false}`、第 2 步 `[{"symbol":"BTCUSDT","marginType":"CROSSED","leverage":5,…},{"symbol":"ETHUSDT","marginType":"CROSSED","leverage":10,…},{"symbol":"DOGEUSDT","marginType":"ISOLATED","leverage":3,…}]`、第 3 步 `{"feeBurn":false}`、第 4 步 200 `{"canTrade":true,…}`、第 5 步 200 `{"uid":"acct-0001","canTrade":true,"accountType":"SPOT",…}`、第 6 步 `{"enableWithdrawals":false,"enableFutures":true,"enableReading":true,…}`;每行改一处后依次 `acct_decode` 全部 7 步再 `acct_finish(Config)`,断言产出的 `AccountConfigReportHdr`
- 实现:`crates/venue-binance/tests/acct.rs::bin_145_usdm_config_bits`
- 备注:依赖待核实:开发备忘第 5 条

| 行 | 改动 | 期望 `mismatches` 与其他字段 |
|---|---|---|
| BIN-145.1 | 无 | 0;`position_mode = OneWay(2)`,`margin_mode = Cross(2)`,`multi_asset = Off(2)`,`source = Startup(1)`,`uid_keys = REST(2)`,`venue_uid = "acct-0001"` 右补 0,`n_lev = 2` |
| BIN-145.2 | 第 0 步 `dualSidePosition = true` | `POS_MODE`(1);`position_mode = Hedge(3)` |
| BIN-145.3 | 第 1 步 `multiAssetsMargin = true` | `MULTI_ASSET`(4);`multi_asset = On(3)` |
| BIN-145.4 | 第 2 步 ETHUSDT `ISOLATED` | `MARGIN_MODE`(2);`margin_mode = Mixed(4)` |
| BIN-145.5 | 第 2 步两个 inst 都 `ISOLATED` | `MARGIN_MODE`(2);`margin_mode = Isolated(3)` |
| BIN-145.6 | 第 2 步只有 DOGEUSDT 为 `ISOLATED`(不在 refdata) | 0;`margin_mode = Cross(2)` |
| BIN-145.7 | 第 3 步 `feeBurn = true` | `FEE_RULE`(8) |
| BIN-145.8 | 第 4 步 400,错误码为统一账户码(核实后登记在 `src/errors.rs`) | `ACCT_TYPE`(16) |
| BIN-145.9 | 第 4 步 `canTrade = false` | `KEY_PERM`(64) |
| BIN-145.10 | 第 5 步 `uid = "acct-0002"` | `UID`(128);`venue_uid = "acct-0002"` |
| BIN-145.11 | 第 5 步 `canTrade = false` | `KEY_PERM`(64) |
| BIN-145.12 | 第 6 步 `enableWithdrawals = true` | `KEY_PERM`(64) |
| BIN-145.13 | 第 6 步 `enableFutures = false` | `KEY_PERM`(64) |
| BIN-145.14 | 第 1 步 HTTP 500 | `UNREADABLE`(256);`multi_asset` 不取默认值 |
| BIN-145.15 | 第 0 步缺 `dualSidePosition` | `UNREADABLE`(256) |
| BIN-145.16 | 第 2 步缺 ETHUSDT | `UNREADABLE`(256);ETHUSDT 的 `LeverageItem.leverage = 0` |
| BIN-145.17 | 第 5 步缺 `uid`(账户为 `uid_policy = "verified"`) | `UNREADABLE`(256);`venue_uid` 全 0;`uid_keys` 不含 `REST` |
| BIN-145.18 | 第 3 步 `feeBurn = true` 且第 6 步 `enableWithdrawals = true` | `FEE_RULE | KEY_PERM`(72) |
| BIN-145.19 | 第 4 步 400 / `-1021`(非统一账户码) | `UNREADABLE`(256),不置 `ACCT_TYPE` |

### BIN-146 永续报告对每个可交易 inst 恰有一条杠杆条目

- 规则:[账户配置核验](README.md#账户配置核验)、[hft-venue-proto 契约 16](../hft-venue-proto/README.md#实现方必须遵守的契约)
- 级别:unit
- 里程碑:M3
- 前置:同 BIN-145.1
- 步骤:
  1. 喂 `CFG0` 并 `acct_finish(Config)`
- 期望:
  - `lev` 恰为 `[{inst: 1042, leverage: 5(5000000000)}, {inst: 1043, leverage: 10(10000000000)}]`(按 inst 升序);DOGEUSDT 不出现;`rflags = 0`
  - 杠杆不影响 `mismatches`(把 BTCUSDT 杠杆改为 125 时 `mismatches` 仍为 0)
- 实现:`crates/venue-binance/tests/acct.rs::bin_146_leverage_items`

### BIN-147 Spot 账户配置报告:账户类型、BNB 抵扣与 key 权限

- 规则:[账户配置核验](README.md#账户配置核验)、[VenueCaps 默认值](README.md#venuecaps-默认值)
- 级别:table
- 里程碑:M3
- 前置:`RD-BIN`;生效 `Spot` caps 同 BIN-171(`fee_asset_rule = ReceivedAssetUnlessDiscount{BNB}`);基线 `SCFG0` = 第 0 步 `{"uid":"acct-0001","canTrade":true,"accountType":"SPOT","permissions":["SPOT"],…}`、第 1 步 `{"enableWithdrawals":false,"enableSpotAndMarginTrading":true,…}`、第 2 步 `{"spotBNBBurn":true,"interestBNBBurn":false}`;每行改一处后跑完序列
- 实现:`crates/venue-binance/tests/acct.rs::bin_147_spot_config_bits`

| 行 | 改动 | 期望 |
|---|---|---|
| BIN-147.1 | 无 | `mismatches = 0`;`position_mode = margin_mode = multi_asset = NotApplicable(1)`;`n_lev = 0`;`uid_keys = REST(2)` |
| BIN-147.2 | 第 0 步 `accountType = "MARGIN"` | `ACCT_TYPE`(16) |
| BIN-147.3 | 第 0 步 `canTrade = false` | `KEY_PERM`(64) |
| BIN-147.4 | 第 0 步 `uid = "acct-0002"` | `UID`(128) |
| BIN-147.5 | 第 1 步 `enableWithdrawals = true` | `KEY_PERM`(64) |
| BIN-147.6 | 第 2 步 `spotBNBBurn = false`(规则要求抵扣) | `FEE_RULE`(8) |
| BIN-147.7 | refdata 覆盖 `fee_asset_rule = Quote`,第 2 步 `spotBNBBurn = true` | `FEE_RULE`(8) |
| BIN-147.8 | refdata 覆盖 `fee_asset_rule = Quote`,第 2 步 `spotBNBBurn = false` | 0 |
| BIN-147.9 | 第 2 步 HTTP 404(端点不存在) | `UNREADABLE`(256) |

### BIN-148 保证金轮询换算为 MarginReport 与 PositionReport

- 规则:[保证金数据换算](README.md#保证金数据换算)、[hft-venue-proto 契约 17](../hft-venue-proto/README.md#实现方必须遵守的契约)
- 级别:unit
- 里程碑:M3
- 前置:`RD-BIN`;全新 `AcctCtx`;第 0 步 `{"totalMarginBalance":"10000.000000","totalMaintMargin":"250.000000","availableBalance":"8000.5",…}`;第 1 步 `[{"symbol":"BTCUSDT","positionAmt":"-0.004","entryPrice":"65000.1","unRealizedProfit":"-0.40","liquidationPrice":"72000.5",…},{"symbol":"ETHUSDT","positionAmt":"0","entryPrice":"0","unRealizedProfit":"0","liquidationPrice":"0",…},{"symbol":"DOGEUSDT",…}]`;第 2 步 `[{"symbol":"BTCUSDT","adlQuantile":{"LONG":0,"SHORT":0,"BOTH":3}},{"symbol":"ETHUSDT","adlQuantile":{"BOTH":0}}]`;`RX`
- 步骤:
  1. 三步 `acct_decode`,再 `acct_finish(Margin)`
- 期望:
  - `MarginReportHdr{venue: 1, account: 2, source: Poll(2), mflags: 0, n: 2, margin_balance: 10000(10000000000), maint_margin: 250(250000000), avail_margin: 8000.5(8000500000), margin_ratio: 0.025(25000000)}`
  - 条目 `[{inst: 1042, adl_quantile: 75, liq_px: 7200050}, {inst: 1043, adl_quantile: 0, liq_px: 0}]`
  - 两条 `PositionReport`:`{inst: 1042, qty: −400000, entry_px: 6500010, upnl: −400000, liq_px: 7200050}`、`{inst: 1043, qty: 0, …, liq_px: 0}`;DOGEUSDT 不产出
- 实现:`crates/venue-binance/tests/margin.rs::bin_148_margin_report`

### BIN-149 margin_ratio 的边界取值

- 规则:[保证金数据换算](README.md#保证金数据换算)、[hft-venue-proto 契约 17](../hft-venue-proto/README.md#实现方必须遵守的契约)
- 级别:table
- 里程碑:M3
- 前置:BIN-148,只改第 0 步两个字段
- 实现:`crates/venue-binance/tests/margin.rs::bin_149_margin_ratio_edges`

| 行 | `totalMarginBalance` / `totalMaintMargin` | 期望 `margin_ratio` |
|---|---|---|
| BIN-149.1 | `"0"` / `"250"` | `Rate::MAX` |
| BIN-149.2 | `"-10"` / `"250"` | `Rate::MAX` |
| BIN-149.3 | `"0.000001"` / `"0"` | 0 |
| BIN-149.4 | `"250"` / `"250"` | 1.0(1000000000) |
| BIN-149.5 | `"125"` / `"250"` | 2.0(2000000000) |
| BIN-149.6 | `"10000"` / `"250"`,交易所另给比率字段 `"marginRatio":"0.5"` | 0.025(25000000),不采用交易所比率 |

### BIN-150 ADL 档沿用上一个序列的值,从未取到为 255

- 规则:[保证金数据换算](README.md#保证金数据换算)、[hft-venue-proto 契约 27](../hft-venue-proto/README.md#实现方必须遵守的契约)
- 级别:table
- 里程碑:M3
- 前置:同一 `AcctCtx` 依次跑 6 个 Margin 序列 S1–S6;"无 ADL" = 该序列没有第 2 步(`every = 6` 未到)
- 实现:`crates/venue-binance/tests/margin.rs::bin_150_adl_carry_over`

| 行 | 序列 / 第 2 步 | 期望 BTCUSDT `adl_quantile` |
|---|---|---|
| BIN-150.1 | S1(全新 `AcctCtx`) / 无 ADL | 255 |
| BIN-150.2 | S2 / `BOTH = 3` | 75 |
| BIN-150.3 | S3 / 无 ADL | 75 |
| BIN-150.4 | S4 / `BOTH = 4` | 100 |
| BIN-150.5 | S5 / `BOTH = 0` | 0 |
| BIN-150.6 | S6 / `BOTH = 5`(越界) | 该步 `acct_decode` 返回 `Err(Shape)` |

### BIN-151 保证金换算与 i128 参考一致

- 规则:[保证金数据换算](README.md#保证金数据换算)
- 级别:prop
- 里程碑:M3
- 性质:任意 `totalMarginBalance`、`totalMaintMargin`(USDT 十进制,≤ 10¹² 且最多 6 位小数)得到的 `margin_ratio` 等于 i128 参考 `maint × 10⁹ / balance`(向零取整;`balance ≤ 0` → `Rate::MAX`);`margin_balance`、`maint_margin` 与十进制参考逐值相等
- 生成:balance ∈ [−10¹², 10¹²],maint ∈ [0, 10¹²],小数位 0–6
- 参考:i128 十进制实现
- 例数:PR 5 000;每夜 500 000
- 实现:`crates/venue-binance/tests/margin.rs::bin_151_margin_ratio_reference`

### BIN-152 账户查询只保留 refdata 中的 symbol,大响应不分配

- 规则:[2. 解码](README.md#2-解码)、[hft-venue-proto 契约 16](../hft-venue-proto/README.md#实现方必须遵守的契约)
- 级别:unit
- 里程碑:M3
- 前置:`RD-BIN`;计数分配器;`positionRisk` 响应 50 KiB,含 300 个 symbol(其中 2 个在 refdata)
- 步骤:
  1. `acct_decode(Margin step 1)`;`acct_finish(Margin)`
- 期望:
  - `MarginReportHdr.n = 2`,`mflags` 不含 `ITEMS_TRUNCATED`;`AcctScratch` 条目数 = 2;分配次数 = 0
- 实现:`crates/venue-binance/tests/margin.rs::bin_152_acct_filters_symbols`

### BIN-153 账户查询解码不读写 GwCtx,沿用状态只在 AcctCtx

- 规则:[hft-venue-proto 契约 27](../hft-venue-proto/README.md#实现方必须遵守的契约)
- 级别:unit
- 里程碑:M3
- 前置:`RD-BIN`;BIN-145.1 与 BIN-148 的响应
- 步骤:
  1. 在全新 `AcctCtx` 上跑 Config 与 Margin 序列,记录报告字节
  2. 在已跑过 10³ 个序列的 `AcctCtx` 上再跑(该 `AcctCtx` 上一个序列的 ADL 档与本序列相同)
  3. 在两次之间对同一 `GwCtx` 穿插 10⁴ 条 `decode_report`
- 期望:
  - 步骤 1 与步骤 2 的报告逐字节相同
  - 步骤 3 前后账户查询报告逐字节相同;`decode_report` 的输出与不穿插账户查询时逐字节相同
- 实现:`crates/venue-binance/tests/acct.rs::bin_153_acct_isolation`

### BIN-170 rate_rules 逐行给出官方默认限速

- 规则:[限速默认值(`rate_rules()`,生效值以 refdata 为准)](README.md#限速默认值rate_rules生效值以-refdata-为准)
- 级别:table
- 里程碑:M1.5
- 前置:`rate_rules()`;另断言 `Usdm` 表长 5、`Spot` 表长 5
- 实现:`crates/venue-binance/tests/caps.rs::bin_170_rate_rules`
- 备注:`Usdm` 新连接一行依赖待核实:开发备忘第 11 条

| 行 | 产品线 / 行 | 期望 `RateRule{scope, bucket, window_ms, limit}` |
|---|---|---|
| BIN-170.1 | `Usdm` / 账户 ORDERS 10 s | `{Account, Orders, 10000, 300}` |
| BIN-170.2 | `Usdm` / 账户 ORDERS 1 min | `{Account, Orders, 60000, 1200}` |
| BIN-170.3 | `Usdm` / IP 权重 | `{Ip, Weight, 60000, 2400}` |
| BIN-170.4 | `Usdm` / 行情连接入向 | `{Conn, WsInMsgs, 1000, 10}` |
| BIN-170.5 | `Usdm` / 新连接 | `{Ip, NewConns, 300000, 300}` |
| BIN-170.6 | `Spot` / 账户 ORDERS 10 s | `{Account, Orders, 10000, 100}` |
| BIN-170.7 | `Spot` / 账户 ORDERS 1 日 | `{Account, Orders, 86400000, 200000}` |
| BIN-170.8 | `Spot` / IP 权重 | `{Ip, Weight, 60000, 6000}` |
| BIN-170.9 | `Spot` / 行情连接入向 | `{Conn, WsInMsgs, 1000, 5}` |
| BIN-170.10 | `Spot` / 新连接 | `{Ip, NewConns, 300000, 300}` |

### BIN-171 default_caps 的每个字段取 README 表中的默认值

- 规则:[VenueCaps 默认值](README.md#venuecaps-默认值)、[hft-venue-proto 契约 15](../hft-venue-proto/README.md#实现方必须遵守的契约)
- 级别:table
- 里程碑:M1.5
- 前置:`Usdm::default_caps()` 与 `Spot::default_caps()`;`fee_asset_rule` 的平台币以资产名断言
- 实现:`crates/venue-binance/tests/caps.rs::bin_171_default_caps`

| 行 | 字段 | 期望 `Usdm` / `Spot` |
|---|---|---|
| BIN-171.1 | `venue`、`market` | `BINANCE_USDM`、`LinearPerp` / `BINANCE_SPOT`、`Spot` |
| BIN-171.2 | `fee_asset_rule` | `Quote` / `ReceivedAssetUnlessDiscount{token: "BNB"}` |
| BIN-171.3 | `reduce_only` | `true` / `false` |
| BIN-171.4 | `required_position_mode` | `OneWay` / `NotApplicable` |
| BIN-171.5 | `required_margin_mode` | `Cross` / `NotApplicable` |
| BIN-171.6 | `required_multi_asset` | `Some(false)` / `None` |
| BIN-171.7 | `required_spot_borrow` | `None` / `None` |
| BIN-171.8 | `funding_settle` | `RateOnly` / `NotApplicable` |
| BIN-171.9 | `margin_data` | `Poll{interval_ms: 5000}` / `NotApplicable` |
| BIN-171.10 | `own_liquidation_marker`、`liq_marker_arg` | `ClientIdPrefix`、`"autoclose-\|adl_autoclose\|settlement_autoclose-"` / `NotIdentifiable`、`""` |
| BIN-171.11 | `ignorable_user_events` | `["TRADE_LITE", "STRATEGY_UPDATE", "GRID_UPDATE", "CONDITIONAL_ORDER_TRIGGER_REJECT"]` / `["balanceUpdate", "listStatus", "externalLockUpdate"]` |
| BIN-171.12 | `flags` | 31(`AMEND | POST_ONLY | IOC | FOK | BATCH_CANCEL`)/ 31;两者均不自动声明 PERP_CASH |
| BIN-171.13 | `dms` 与四个时长 | `CountdownPerInst`,`dms_min_ms = 5000`,`dms_max_ms = 0`,`dms_timeout_ms = 30000`,`dms_refresh_ms = 10000` / `None`,四个 0 |
| BIN-171.14 | `testnet` | `Demo` / `Demo` |
| BIN-171.15 | `amend_qty_down_only` | `Usdm` false / `Spot` true |

### BIN-172 stale_guard 与 dead_man 的协议常量

- 规则:[VenueCaps 默认值](README.md#venuecaps-默认值)、[6. 死手与会话维护](README.md#6-死手与会话维护)、[hft-venue-proto 契约 10](../hft-venue-proto/README.md#实现方必须遵守的契约)
- 级别:table
- 里程碑:M1.5
- 前置:无参数
- 实现:`crates/venue-binance/tests/caps.rs::bin_172_stale_guard_dead_man`

| 行 | 调用 | 期望 |
|---|---|---|
| BIN-172.1 | `Usdm::stale_guard()` | `RecvWindow{default_ms: 5000}` |
| BIN-172.2 | `Spot::stale_guard()` | `RecvWindow{default_ms: 5000}` |
| BIN-172.3 | `Usdm::dead_man()` | `CountdownPerInst{min_ms: 5000, timeout_ms: 30000, refresh_ms: 10000, via: Rest}`;`30000 ≥ max(5000, 2 × 10000)` |
| BIN-172.4 | `Spot::dead_man()` | `DeadManSpec::None` |
| BIN-172.5 | `Usdm::default_caps()` 的 `dms_*` 与 `dead_man()` | `dms_min_ms = min_ms`、`dms_timeout_ms = timeout_ms`、`dms_refresh_ms = refresh_ms`、`dms_max_ms = 0` |

### BIN-173 死手刷新对每个有挂单的 inst 发一个签名 REST 请求

Warm 请求与 countdownTime 字符串/数字确认由 `tests/encode.rs::bin_173_deadman_requests_and_response_strings` 验证。

- 规则:[6. 死手与会话维护](README.md#6-死手与会话维护)、[下单:WS API](README.md#下单ws-api)
- 级别:table
- 里程碑:M3
- 前置:`RD-BIN`;HMAC 密钥 `HM1`,`apiKey = "TESTKEY"`;`WC`;`stale_guard_ms = 1500`
- 实现:`crates/venue-binance/tests/session.rs::bin_173_dead_man_refresh`
- 备注:依赖待核实:开发备忘第 10 条(端点、语义与权重)

| 行 | `timeout_ms` / `insts` | 期望 `DeadManOut` |
|---|---|---|
| BIN-173.1 | 30000 / `[1042, 1043]` | 恰 2 个请求:`POST /fapi/v1/countdownCancelAll?symbol=BTCUSDT&countdownTime=30000&timestamp=1790000000000&recvWindow=1500&signature=2659d5bb37d94eb5cc0bebe9cd7773a6c0f091d195467eedea3395d328ecce16` 与同形 ETHUSDT(`signature=d6b2e0e3a0c37c7f3e6ed0bf596a53342daed57ecced145db3862db8e769dc28`);`endpoint 0`,`redacted: true` |
| BIN-173.2 | 30000 / `[]` | 0 个请求,返回 `Ok` |
| BIN-173.3 | 45000 / `[1042]` | 1 个请求,`countdownTime=45000`(毫秒原样) |
| BIN-173.4 | 30000 / `[2001]`(Spot 的 inst) | `Err(EncodeErr::UnknownInst)` |
| BIN-173.5 | `Spot::dead_man_refresh`,30000 / `[2001]` | `Err(EncodeErr::Unsupported(GwOp::DeadManRefresh))` |

### BIN-174 契约检查对两个产品线全部通过

- 规则:[hft-venue-proto 契约 8](../hft-venue-proto/README.md#实现方必须遵守的契约)、[VenueCaps 默认值](README.md#venuecaps-默认值)、[外部成交识别](README.md#外部成交识别)、[8. 源文件规划](README.md#8-源文件规划)
- 级别:table
- 里程碑:M3
- 前置:`tests/contract.rs`;hft-venue-proto `testkit` feature;`RD-BIN`;每个检查用该产品线的金样作样本
- 实现:`crates/venue-binance/tests/contract.rs::bin_174_contract_checks`

| 行 | 检查 | 期望 |
|---|---|---|
| BIN-174.1 | `check_unknown_codes`(`Usdm`、`Spot`) | 两者通过 |
| BIN-174.2 | `check_cum` | 两者通过 |
| BIN-174.3 | `check_unknown_events` | 两者通过 |
| BIN-174.4 | `check_dedup`(`Spot` 不含资金费部分) | 两者通过 |
| BIN-174.5 | `check_external_fill`(`Usdm` 用强平/ADL/网页单样本,`Spot` 用网页单样本) | 两者通过 |
| BIN-174.6 | `check_fee_asset` | 两者通过 |
| BIN-174.7 | `check_account_config` | 两者通过(`Spot` 现货钱包不能借币,借币样本以 `accountType = "MARGIN"` → `ACCT_TYPE` 代替,契约 16) |
| BIN-174.8 | `check_identity` | 两者通过 |
| BIN-174.9 | `check_default_caps` | 两者通过 |
| BIN-174.10 | `check_snapshot_isolation` | 两者通过(只含 BBO 补帧) |
| BIN-174.11 | `check_topn` | 两者通过 |
| BIN-174.12 | `check_acct_isolation` | 两者通过 |
| BIN-174.13 | `check_md_basics` | 两者通过 |
| BIN-174.14 | `check_hot_alloc` | 两者通过 |
| BIN-174.15 | `check_session` | 两者通过(哨兵密钥为 `ED1`、`HM1` 的替身) |
| BIN-174.16 | `check_cid` | 两者通过 |
| BIN-174.17 | `check_funding` | 默认 `Usdm` 按 `RateOnly` 无私有实额推送;认证历史见 BIN-332;`Spot` 无资金费样本、0 条 `FundingSettle` |
| BIN-174.18 | `check_margin` | `Usdm` 按 `Poll` 通过;`Spot` 的 Margin 序列为空 |
| BIN-174.19 | `check_bbo_refresh` | 两者通过(`Usdm` 批量 1、`Spot` 批量不限条数) |
| BIN-174.20 | `check_detail` | 两者通过(`-2011` 为 `GONE` 类) |
| BIN-174.21 | `check_rest` | `Usdm` 通过(`CancelAll` 为 REST,会话维护 `Ok(true)`);`Spot` 全部 op 为 Ws,会话维护 `Ok(false)` |
| BIN-174.22 | `check_ctl` | 两者通过(`-1021`、`-1003`/`-1015`/429/418、`listenKeyExpired`/`eventStreamTerminated`) |

### BIN-175 只读查询的请求与解码通过 check_ro

当前 warm 入口由 `crates/venue-binance/tests/readonly.rs` 验证签名、整页失败原子性、分页与外来身份；FundingFlow 保留为该表未完成项，实额查询沿用 FundingHistoryProtocol。

- 规则:[只读查询](README.md#只读查询)、[hft-venue-proto 契约 31](../hft-venue-proto/README.md#实现方必须遵守的契约)
- 级别:table
- 里程碑:M5
- 前置:`RD-BIN`;以只读测试凭证(同 `HM1` 向量)构建的 `Templates`;`WC`;样本取 BIN-330–BIN-334
- 实现:`crates/venue-binance/tests/ro.rs::bin_175_read_only_queries`
- 备注:依赖待核实:开发备忘第 22 条

| 行 | 调用 | 期望 |
|---|---|---|
| BIN-175.1 | `Usdm::ro_request(OpenOrders, inst = 0)` | `GET /fapi/v1/openOrders?timestamp=…&recvWindow=1500&signature=…`;`redacted = true` |
| BIN-175.2 | `Usdm::ro_request(Fills, inst = 1042, cursor = 0, since_ms = 1790000000000)`;`inst = 0` | 前者 `GET /fapi/v1/userTrades?symbol=BTCUSDT&startTime=1790000000000&…`;后者 `Err(EncodeErr::Unsupported(GwOp::Query))`(按 symbol 查询) |
| BIN-175.3 | `Spot::ro_request(Positions)`、`Spot::ro_request(FundingFlow)` | 都 `Err(EncodeErr::Unsupported(GwOp::Query))` |
| BIN-175.4 | `ro_decode(Fills)` 的成交行 | `FillRow.cid = None`;`trade_id`、`exch_order_id` 取 `id`、`orderId` |
| BIN-175.5 | 两个产品线交给 `check_ro` | 通过 |

### BIN-190 心跳规格按产品线与角色给出

- 规则:[心跳与 BBO 探测](README.md#心跳与-bbo-探测)、[hft-venue-proto 契约 24](../hft-venue-proto/README.md#实现方必须遵守的契约)
- 级别:table
- 里程碑:M1.5
- 前置:`heartbeat(role)`
- 实现:`crates/venue-binance/tests/session.rs::bin_190_heartbeat_spec`
- 备注:依赖待核实:开发备忘第 18 条(各连接的服务端 Ping 间隔)

| 行 | 产品线 / 角色 | 期望 `HeartbeatSpec` |
|---|---|---|
| BIN-190.1 | `Usdm` / `Md` | `{ping: WsPing, interval_ms: 0, unconditional: false, pong_timeout_ms: 10000, idle_timeout_ms: 360000}` |
| BIN-190.2 | `Usdm` / `UserStream` | 同 .1 |
| BIN-190.3 | `Usdm` / `Order` | `{WsPing, 0, false, 10000, 40000}` |
| BIN-190.4 | `Spot` / `Md` | `{WsPing, 0, false, 10000, 40000}` |
| BIN-190.5 | `Spot` / `Order` | `{WsPing, 0, false, 10000, 40000}` |

### BIN-191 BBO 探测用 WS Ping,应用层 ping 不写字节

- 规则:[心跳与 BBO 探测](README.md#心跳与-bbo-探测)
- 级别:unit
- 里程碑:M1.5
- 前置:`out` 为 64 B 缓冲,初值全 0xAA
- 步骤:
  1. `Usdm::bbo_probe()`、`Spot::bbo_probe()`
  2. 对每个角色 `app_ping(role, 1790000000000, &mut out)`
- 期望:
  - 步骤 1 两者都是 `PingKind::WsPing`
  - 步骤 2 每次返回 0,`out` 未被改写
- 实现:`crates/venue-binance/tests/session.rs::bin_191_bbo_probe_ws_ping`

### BIN-200 订阅帧每帧至多 200 个流,帧 id 逐帧递增

- 规则:[7. 订阅节奏](README.md#7-订阅节奏)
- 级别:table
- 里程碑:M1.5
- 前置:测试 refdata 另含 450 个 `Usdm` inst;`SubSpec{stream: Bbo, route: public-bbo}` × k
- 实现:`crates/venue-binance/tests/subscribe.rs::bin_200_subscribe_batching`

| 行 | k | 期望 |
|---|---|---|
| BIN-200.1 | 1 | 1 帧:`{"method":"SUBSCRIBE","params":["btcusdt@bookTicker"],"id":1}` |
| BIN-200.2 | 200 | 1 帧,`params` 200 个 |
| BIN-200.3 | 201 | 2 帧:200 + 1,`id` 为 1、2 |
| BIN-200.4 | 450 | 3 帧:200、200、50,`id` 为 1、2、3 |
| BIN-200.5 | 0 | 0 帧,返回 `Ok` |
| BIN-200.6 | 任意 k | 输出中没有 `SET_PROPERTY` 帧 |

### BIN-201 流名按 StreamKind、档数与产品线生成

- 规则:[7. 订阅节奏](README.md#7-订阅节奏)、[分档快照流](README.md#分档快照流)、[已知限制与后续工作](README.md#已知限制与后续工作)
- 级别:table
- 里程碑:M1.5
- 前置:`RD-BIN`;每行一组 `SubSpec`,断言 `params` 内容
- 实现:`crates/venue-binance/tests/subscribe.rs::bin_201_stream_names`

| 行 | 产品线 / `SubSpec` | 期望 |
|---|---|---|
| BIN-201.1 | `Usdm` / `{1042, Bbo}` | `btcusdt@bookTicker` |
| BIN-201.2 | `Usdm` / `{1042, DepthTopN, levels: 5}` | `btcusdt@depth5@100ms` |
| BIN-201.3 | `Usdm` / `{1042, DepthTopN, levels: 20}` | `btcusdt@depth20@100ms` |
| BIN-201.4 | `Usdm` / `{1042, DepthTopN, levels: 15}` | `Err(ProtoErr::Unsupported)` |
| BIN-201.5 | `Usdm` / `{1042, Trade}` | `btcusdt@aggTrade` |
| BIN-201.6 | `Usdm` / `{1042, Mark}`、`{1042, Index}`、`{1042, Funding}` | 只有一个 `btcusdt@markPrice@1s` |
| BIN-201.7 | `Usdm` / `{1042, Liquidation}` | `btcusdt@forceOrder` |
| BIN-201.8 | `Spot` / `{2001, Bbo}` | `btcusdt@bookTicker` |
| BIN-201.9 | `Spot` / `{2001, DepthTopN, levels: 10}` | `btcusdt@depth10@100ms` |
| BIN-201.10 | `Spot` / `{2001, Trade}` | `btcusdt@trade` |
| BIN-201.11 | `Spot` / `{2001, Funding}` | `Err(ProtoErr::Unsupported)` |
| BIN-201.12 | 任意组合 | 流名中从不出现 `rpiDepth` |

### BIN-202 私有频道订阅:Usdm 不出帧,Spot 订阅与退订用户流

- 规则:[7. 订阅节奏](README.md#7-订阅节奏)、[6. 死手与会话维护](README.md#6-死手与会话维护)
- 级别:table
- 里程碑:M1.5
- 前置:账户 2
- 实现:`crates/venue-binance/tests/subscribe.rs::bin_202_private_subscribe`

| 行 | 调用 | 期望 |
|---|---|---|
| BIN-202.1 | `Usdm::private_subscribe(&cfg, 2, false, &mut out)` | `Ok`,0 帧 |
| BIN-202.2 | `Spot::private_subscribe(&cfg, 2, false, &mut out)` | 1 帧 `{"id":"u0000000000000000","method":"userDataStream.subscribe"}` |
| BIN-202.3 | `Spot::private_subscribe(&cfg, 2, true, &mut out)` | 1 帧 `{"id":"u0000000000000000","method":"userDataStream.unsubscribe"}` |

### BIN-210 依赖图不含运行时、网络与 serde_json

- 规则:[依赖](README.md#依赖)、[2. 解码](README.md#2-解码)
- 级别:unit
- 里程碑:M3
- 前置:`cargo tree -p venue-binance -e normal --prefix none`(默认特性与 `--all-features` 各一次)
- 步骤:
  1. 收集包名集合
- 期望:
  - 集合中没有 `hft-net`、`hft-venue`、`hft-sequencer`、`tokio`、`serde_json`;`serde_json` 只出现在 `-e dev`
- 实现:`crates/venue-binance/tests/deps.rs::bin_210_forbidden_deps`

### BIN-211 生产构建不启用 mock 特性

- 规则:[`mock` 特性(给 mock-exchange 的反向编解码)](README.md#mock-特性给-mock-exchange-的反向编解码)
- 级别:unit
- 里程碑:M3
- 前置:`cargo tree -p hft-core -e features --target aarch64-unknown-linux-gnu`
- 步骤:
  1. 查找 `venue-binance feature "mock"`
- 期望:
  - 找不到;`venue-binance feature "spot-sbe"` 同样找不到(optional,默认关)
- 实现:`crates/venue-binance/tests/deps.rs::bin_211_no_mock_in_prod`

### BIN-212 演示环境两个产品线共用一个 UID 时 refdata 只告警

- 规则:[测试网/模拟盘](README.md#测试网模拟盘)
- 级别:integ
- 里程碑:M3
- 前置:hft-cfg 由 `fixtures/refdata/bin-demo-1.toml` 生成演示环境 refdata:账户 7 启用 `BINANCE_SPOT`、账户 8 启用 `BINANCE_USDM`,两者 `is_prod = false`、`venue_uid = "acct-0009"`、`uid_policy = "verified"`;另一份把账户 8 改为 `is_prod = true`
- 步骤:
  1. `RefData::load` 第一份
  2. `RefData::load` 第二份
- 期望:
  - 步骤 1 `Ok`;`warnings()` 恰一条 `LoadErr{code: DuplicateVenueUid, section: 14}`
  - 步骤 2 `Err(DuplicateVenueUid)`
- 实现:`crates/venue-binance/tests/demo.rs::bin_212_demo_shared_uid`
- 备注:依赖待核实:开发备忘第 14 条(同一演示 key 是否覆盖两条产品线)

### BIN-213 演示环境读不到 UID 时按 unverifiable 配置,Config 序列不变且缺 UID 不置位

- 规则:[测试网/模拟盘](README.md#测试网模拟盘)、[账户配置核验](README.md#账户配置核验)
- 级别:integ
- 里程碑:M3
- 前置:演示环境 refdata,账户 7 `uid_policy = "unverifiable"`、`is_prod = false`、`venue_uid = ""`;`Spot` Config 序列,第 0 步响应无 `uid`
- 步骤:
  1. 加载 refdata;跑 `Spot` Config 序列
  2. 把账户 7 改为 `is_prod = true` 再加载
- 期望:
  - 步骤 1 加载 `Ok`;`acct_queries(Config)` 与 BIN-140.8–BIN-140.10 相同;报告 `mismatches = 0`(缺 `uid` 不置 `UID`、`UNREADABLE`),`uid_keys` 不含 `REST`,`venue_uid` 全 0
  - 步骤 2 `Err(UidPolicyProd)`
- 实现:`crates/venue-binance/tests/demo.rs::bin_213_demo_unverifiable_uid`

### BIN-214 演示环境端点逐项核实并写回 README(M3 之前)

- 规则:[测试网/模拟盘](README.md#测试网模拟盘)、[开发备忘](README.md#开发备忘)
- 级别:testnet
- 里程碑:M3
- 环境:东京服务器(方法 A 抓官方原文)+ dev2 联调窗口(`hft-window start`)内的演示账户(方法 B)
- 授权:无(只读文档与演示账户;演示环境下单不涉及资金)
- 步骤:
  1. 按开发备忘第 14 条缓存 Spot "Demo Mode" 与 USDⓈ-M "General Info" 原文到 `architecture-v2/research/_src/`
  2. 对"测试网/模拟盘"表每个端点建连:`demo-fapi` REST、`demo-fstream` 的 `/public`、`/market`、`/private`、`demo-api`、`demo-ws-api`、`demo-stream`(`/ws` 与 `/stream`);USDⓈ-M 演示环境 WS API 下单端点逐个试
  3. 每个可用端点上各做一次:登录(下单连接)、订阅 BTCUSDT bookTicker、Place + Cancel 一笔远离盘口的限价单、查询一次 Config 序列
- 通过标准:README"测试网/模拟盘"表每行写明"可用 / 不可用 + 替代",USDⓈ-M 演示下单通道有结论;`sapi` 类端点的可用性有结论;开发备忘第 14 条改为已核实并链接原文缓存
- 证据:`tests/host/BIN-214/` 下的 raw 样本(经 `fixtures scrub`)与命令输出;文档缓存文件路径写进 README
- 实现:`tests/host/BIN-214.sh`

### BIN-215 私有频道能否双订阅的核实(M3 之前)

- 规则:[开发备忘](README.md#开发备忘)、[端点与路由](README.md#端点与路由)
- 级别:testnet
- 里程碑:M3
- 环境:dev2 联调窗口内的演示账户
- 授权:无
- 步骤:
  1. `Usdm`:同一 listenKey 建两条 `/private` 连接,下一笔远离盘口的限价单再撤单
  2. `Spot`:同一账户两条 ws-api 连接各 `session.logon` + `userDataStream.subscribe`,同样下单再撤单
  3. 比对两路 raw
- 通过标准:两条连接是否都收到 `NEW` 与 `CANCELED` 推送有结论;不允许时 README 写明该产品线的切换退化为"先断后建 + VERIFY"(开发备忘第 12 条)
- 证据:两路 raw journal 段(`tests/host/BIN-215/`)与比对输出
- 实现:`tests/host/BIN-215.sh`

### BIN-216 演示环境连续 3 天冒烟(门槛 ⑥)

- 规则:[测试网/模拟盘](README.md#测试网模拟盘)
- 级别:testnet
- 里程碑:M6
- 环境:dev2 上指向演示环境的 `prod` 角色实例,`feed_env = "testnet"`、`order_env = "testnet"`,演示环境 refdata(hft-cfg 按演示环境 `exchangeInfo` 生成)
- 授权:无(演示账户)
- 步骤:
  1. 联调窗口内对每个上线策略用到的 (`binance-usdm`, `binance-spot`) 跑下单/撤单/成交/断连/重同步、账户配置与 UID 查询、kill-switch 撤单与 reconciler 对账,连续 3 天
  2. 每天汇总 raw journal 中的解码错误、`skip = Unknown` 事件、`UNREADABLE` 位
- 通过标准:3 天内未解释的解码错误 = 0;每个未知事件名有结论(加入 `ignorable_user_events` 或登记新类);账户配置核验(含两把 key 的 UID,读不到时按 `uid_policy`)、杠杆条目与保证金轮询结果与 reconciler 真值一致;BBO 连接在盘口 10 s 不变时 `now − conn_live_tsc ≤ 1.1 s`
- 证据:门槛 ⑥ 记录(每日汇总、未知事件结论、对账报告)按 [testing §8](../../docs/testing.md#8-上线门槛) 归档
- 实现:`tests/host/BIN-216.sh`
### BIN-300 金样:Usdm bookTicker

- 规则:[行情频道 → 事件](README.md#行情频道--事件)、[2. 解码](README.md#2-解码)
- 级别:golden
- 里程碑:M3
- 输入:`fixtures/binance-usdm/bookTicker/` 的 `normal.in`、`tiny_px_1e-8.in`(1000 倍前缀合约,极小价)、`huge_qty_1e11.in`、`sci_notation_px.in`、`dup_u.in`(同一 `u` 两条)、`missing_T.in`、`empty_qty.in`;来源:现有 collector 的 raw WAL(`fixtures extract --wal`,`source = "s3-wal"`),异常样本 `source = "synthetic"`
- 期望文件:同目录 `<case>.expected.jsonl`(`dup_u` 两行 `Bbo` 相同、去重归运行时;`missing_T` 与 `empty_qty` 为 `{"error":…}`)
- 实现:`crates/venue-binance/tests/golden.rs::bin_300_usdm_bookticker`

### BIN-301 金样:Usdm 分档快照 depth5/10/20

- 规则:[分档快照流](README.md#分档快照流)、[hft-venue-proto 契约 26](../hft-venue-proto/README.md#实现方必须遵守的契约)
- 级别:golden
- 里程碑:M3
- 输入:`fixtures/binance-usdm/depth5/`、`depth10/`、`depth20/` 各一条 `normal.in`;`depth20/dup_u.in`(同一 `u` 重复)、`depth20/short_levels.in`(冷门合约档数不足 20)、`depth5/empty_side.in`(`b` 为空数组);来源:待采集(方法 B:演示环境订阅 `/public` 各档 1 分钟;主网 collector 无该频道)
- 期望文件:同目录 `<case>.expected.jsonl`(每条消息一条 `L2Snapshot{origin = TopN}`,按 `feed.snapshot_part_levels` 切片)
- 实现:`crates/venue-binance/tests/golden.rs::bin_301_usdm_partial_depth`
- 备注:依赖待核实:开发备忘第 20 条

### BIN-302 金样:Usdm aggTrade

- 规则:[行情频道 → 事件](README.md#行情频道--事件)
- 级别:golden
- 里程碑:M3
- 输入:`fixtures/binance-usdm/aggTrade/` 的 `taker_buy.in`(`m = false`)、`taker_sell.in`(`m = true`)、`missing_m.in`、`overflow_qty.in`;来源:collector raw WAL 与合成
- 期望文件:同目录 `<case>.expected.jsonl`
- 实现:`crates/venue-binance/tests/golden.rs::bin_302_usdm_aggtrade`

### BIN-303 金样:Usdm markPriceUpdate

- 规则:[行情频道 → 事件](README.md#行情频道--事件)
- 级别:golden
- 里程碑:M3
- 输入:`fixtures/binance-usdm/markPriceUpdate/` 的 `normal.in`、`interval_change.in`(一段跨两次结算且结算周期由 8 h 变为 4 h 的连续推送)、`sci_notation_rate.in`;来源:collector raw WAL(周期变化样本按 WAL 时段抽取)
- 期望文件:同目录 `<case>.expected.jsonl`(每条推送三行:`MarkPrice`、`IndexPrice`、`Funding`;`interval_s` 按 BIN-033 规则)
- 实现:`crates/venue-binance/tests/golden.rs::bin_303_usdm_markprice`

### BIN-304 金样:Usdm forceOrder(公共强平流)

- 规则:[行情频道 → 事件](README.md#行情频道--事件)
- 级别:golden
- 里程碑:M3
- 输入:`fixtures/binance-usdm/forceOrder/` 的 `sell.in`、`buy.in`、`missing_o.in`;来源:collector raw WAL(`/market` 路由)
- 期望文件:同目录 `<case>.expected.jsonl`
- 实现:`crates/venue-binance/tests/golden.rs::bin_304_usdm_force_order`

### BIN-305 金样:Usdm 订阅回执、订阅被拒与 serverShutdown

- 规则:[1. 分派(`classify` / `classify_report`,只看前 64 B)](README.md#1-分派classify--classify_report只看前-64-b)
- 级别:golden
- 里程碑:M3
- 输入:`fixtures/binance-usdm/control/` 的 `sub_ack.in`、`sub_error.in`、`server_shutdown.in`;来源:collector raw;`server_shutdown` 为 `source = "exchange-doc"`(待采集,方法 A,开发备忘第 11 条)
- 期望文件:同目录 `<case>.expected.jsonl`(`control` 的 `ControlHint` 渲染)
- 实现:`crates/venue-binance/tests/golden.rs::bin_305_usdm_control`

### BIN-306 金样:Usdm BBO 补帧 REST 响应

- 规则:[心跳与 BBO 探测](README.md#心跳与-bbo-探测)
- 级别:golden
- 里程碑:M3
- 输入:`fixtures/binance-usdm/rest.ticker_bookTicker/` 的 `single.in`、`error_body.in`;来源:待采集(方法 B:演示环境与主网各一次 `GET /fapi/v1/ticker/bookTicker?symbol=BTCUSDT`)
- 期望文件:同目录 `<case>.expected.jsonl`
- 实现:`crates/venue-binance/tests/golden.rs::bin_306_usdm_bbo_refresh`
- 备注:依赖待核实:开发备忘第 17 条

### BIN-307 金样:Usdm session.logon 响应

- 规则:[下单:WS API](README.md#下单ws-api)
- 级别:golden
- 里程碑:M3
- 输入:`fixtures/binance-usdm/session.logon/` 的 `ok.in`、`bad_signature.in`(`-1022`)、`clock_skew.in`(`-1021`);来源:待采集(方法 B:演示环境 ws-fapi;登录帧本身不入库,只存响应)
- 期望文件:同目录 `<case>.expected.jsonl`(`LoginResult` 渲染)
- 实现:`crates/venue-binance/tests/golden.rs::bin_307_usdm_logon`
- 备注:依赖待核实:开发备忘第 14 条(演示环境 ws-fapi 端点)

### BIN-308 金样:Usdm order.place 响应与错误码

- 规则:[下单:WS API](README.md#下单ws-api)、[结果未知码表](README.md#结果未知码表)、[5. 回报解码](README.md#5-回报解码)
- 级别:golden
- 里程碑:M3
- 输入:`fixtures/binance-usdm/order.place/` 的 `ack.in`、`err_-1007.in`、`err_-1021.in`、`err_-2019.in`、`err_-2022.in`、`err_-5022.in`、`err_-1013.in`、`err_-1111.in`、`err_-4164.in`、`err_-4061.in`、`http_429.in`、`http_418.in`、`http_503.in`、`unregistered_-9999.in`(合成)、`rate_limits_3types.in`;来源:待采集(方法 B:演示环境逐个构造:精度错、余额不足、GTX 吃单、reduceOnly 无仓位、过期时间戳);拿不到的码 `source = "exchange-doc"`
- 期望文件:同目录 `<case>.expected.jsonl`(`Ack`;登记为确定拒绝的码为 `Reject`;结果未知与未登记码为 `{"outcome_unknown":…,"raw_code":…}`,没有 `Reject`)
- 实现:`crates/venue-binance/tests/golden.rs::bin_308_usdm_place_responses`

### BIN-309 金样:Usdm order.modify 响应

- 规则:[下单:WS API](README.md#下单ws-api)
- 级别:golden
- 里程碑:M3
- 输入:`fixtures/binance-usdm/order.modify/` 的 `ok.in`、`err_-2013.in`、`err_-1001.in`;来源:待采集(方法 B)
- 期望文件:同目录 `<case>.expected.jsonl`
- 实现:`crates/venue-binance/tests/golden.rs::bin_309_usdm_modify_responses`

### BIN-310 金样:Usdm order.cancel 响应

- 规则:[下单:WS API](README.md#下单ws-api)、[结果未知码表](README.md#结果未知码表)
- 级别:golden
- 里程碑:M3
- 输入:`fixtures/binance-usdm/order.cancel/` 的 `ok.in`、`err_-2011_filled.in`、`http_503.in`;来源:待采集(方法 B:先成交再撤得 `-2011`)
- 期望文件:同目录 `<case>.expected.jsonl`(`-2011` → `CancelReject`)
- 实现:`crates/venue-binance/tests/golden.rs::bin_310_usdm_cancel_responses`

### BIN-311 金样:Usdm order.status 响应

- 规则:[下单:WS API](README.md#下单ws-api)、[累计量与费用来源](README.md#累计量与费用来源)
- 级别:golden
- 里程碑:M3
- 输入:`fixtures/binance-usdm/order.status/` 的 `filled_avgPrice.in`、`partially_filled.in`、`new.in`、`canceled.in`、`expired.in`、`not_found_-2013.in`;来源:待采集(方法 B)
- 期望文件:同目录 `<case>.expected.jsonl`(`QueryResult`,带 `cum_filled` 与 `avg_px`;`-2013` 为 NotFound 形态)
- 实现:`crates/venue-binance/tests/golden.rs::bin_311_usdm_order_status`
- 备注:依赖待核实:开发备忘第 19 条

### BIN-312 金样:Usdm 全撤与死手 REST 响应

- 规则:[下单:WS API](README.md#下单ws-api)、[6. 死手与会话维护](README.md#6-死手与会话维护)
- 级别:golden
- 里程碑:M3
- 输入:`fixtures/binance-usdm/rest.allOpenOrders/` 的 `ok.in`、`http_429.in`;`fixtures/binance-usdm/rest.countdownCancelAll/` 的 `ok.in`、`err_body.in`;来源:待采集(方法 B)
- 期望文件:同目录 `<case>.expected.jsonl`
- 实现:`crates/venue-binance/tests/golden.rs::bin_312_usdm_cancel_all_dead_man`
- 备注:依赖待核实:开发备忘第 10 条

### BIN-313 金样:Usdm ORDER_TRADE_UPDATE 各状态

- 规则:[5. 回报解码](README.md#5-回报解码)、[累计量与费用来源](README.md#累计量与费用来源)
- 级别:golden
- 里程碑:M3
- 输入:`fixtures/binance-usdm/ORDER_TRADE_UPDATE/` 的 `new.in`、`partially_filled.in`、`filled.in`、`canceled.in`、`expired_ioc.in`、`expired_gtx.in`、`amendment.in`、`dup_trade.in`(同一成交两次)、`same_t_two_insts.in`、`fee_bnb.in`(合成,`N = BNB`);来源:待采集(方法 B:演示环境逐种状态下单)
- 期望文件:同目录 `<case>.expected.jsonl`(`dup_trade` 只一行成交;`fee_bnb` 带 `fee_rule_deviation`)
- 实现:`crates/venue-binance/tests/golden.rs::bin_313_usdm_otu_states`

### BIN-314 金样:Usdm 自身强平、ADL、结算与网页单成交

- 规则:[外部成交识别](README.md#外部成交识别)、[hft-venue-proto 契约 18](../hft-venue-proto/README.md#实现方必须遵守的契约)
- 级别:golden
- 里程碑:M3
- 输入:`fixtures/binance-usdm/ORDER_TRADE_UPDATE/` 的 `liquidation_autoclose.in`、`adl_autoclose.in`、`settlement_autoclose.in`、`web_order_fill.in`、`liquidation_t0_dup.in`(`t = 0` 同一推送两次与 `z` 不同的第二笔);来源:强平待采集(方法 B:演示环境高杠杆小仓制造);ADL 与结算 `source = "exchange-doc"` 标"合成";网页单待采集(方法 B,不可得时方法 C)
- 期望文件:同目录 `<case>.expected.jsonl`(每条一行 `ExternalFill`,无 `OrderEvent`)
- 实现:`crates/venue-binance/tests/golden.rs::bin_314_usdm_external_fills`
- 备注:依赖待核实:开发备忘第 2、3 条

### BIN-315 金样:Usdm ACCOUNT_UPDATE

- 规则:[用户流事件表](README.md#用户流事件表)、[hft-venue-proto 契约 14](../hft-venue-proto/README.md#实现方必须遵守的契约)
- 级别:golden
- 里程碑:M3
- 输入:`fixtures/binance-usdm/ACCOUNT_UPDATE/` 的 `m_order.in`、`m_funding_fee_cross.in`、`m_funding_fee_cross_dup.in`(同一推送两次)、`m_funding_fee_isolated.in`、`m_margin_type_change.in`、`m_insurance_clear.in`;来源:待采集(方法 B:演示环境持两个 symbol 跨一次结算;不结算时方法 C)
- 期望文件:同目录 `<case>.expected.jsonl`(资金费重复只一行 `FundingSettle`)
- 实现:`crates/venue-binance/tests/golden.rs::bin_315_usdm_account_update`
- 备注:依赖待核实:开发备忘第 1 条

### BIN-316 金样:Usdm MARGIN_CALL 与 ACCOUNT_CONFIG_UPDATE

- 规则:[用户流事件表](README.md#用户流事件表)
- 级别:golden
- 里程碑:M3
- 输入:`fixtures/binance-usdm/MARGIN_CALL/normal.in`;`fixtures/binance-usdm/ACCOUNT_CONFIG_UPDATE/` 的 `ac_leverage.in`、`ai_multi_assets.in`;来源:`MARGIN_CALL` 为 `source = "exchange-doc"`;配置变化待采集(方法 B:演示环境改杠杆与多资产开关)
- 期望文件:同目录 `<case>.expected.jsonl`(零事件,`hint` 渲染)
- 实现:`crates/venue-binance/tests/golden.rs::bin_316_usdm_hint_events`

### BIN-317 金样:Usdm 可忽略、未知与 listenKeyExpired 事件

- 规则:[用户流事件表](README.md#用户流事件表)、[hft-venue-proto 契约 20](../hft-venue-proto/README.md#实现方必须遵守的契约)
- 级别:golden
- 里程碑:M3
- 输入:`fixtures/binance-usdm/TRADE_LITE/normal.in`、`fixtures/binance-usdm/STRATEGY_UPDATE/normal.in`、`fixtures/binance-usdm/GRID_UPDATE/normal.in`、`fixtures/binance-usdm/CONDITIONAL_ORDER_TRIGGER_REJECT/normal.in`(后三者 `source = "exchange-doc"`)、`fixtures/binance-usdm/unknown_event/synthetic.in`、`fixtures/binance-usdm/listenKeyExpired/normal.in`;来源:`TRADE_LITE` 待采集(方法 B)
- 期望文件:同目录 `<case>.expected.jsonl`(可忽略为 `{"ignored":"<事件名>"}`,未知为计数行,不是 `error`)
- 实现:`crates/venue-binance/tests/golden.rs::bin_317_usdm_skip_events`
- 备注:依赖待核实:开发备忘第 4 条

### BIN-318 金样:Usdm 账户配置查询响应

- 规则:[账户配置核验](README.md#账户配置核验)、[hft-venue-proto 契约 16](../hft-venue-proto/README.md#实现方必须遵守的契约)
- 级别:golden
- 里程碑:M3
- 输入:`fixtures/binance-usdm/rest.positionSide_dual/` 的 `false.in`、`true.in`;`rest.multiAssetsMargin/` 的 `false.in`、`true.in`;`rest.symbolConfig/` 的 `all_cross.in`、`mixed.in`;`rest.feeBurn/` 的 `false.in`、`true.in`;`rest.fapi_v3_account/` 的 `ok.in`、`pm_rejected.in`;`rest.api_v3_account/` 的 `with_uid.in`、`without_uid.in`;`rest.apiRestrictions/` 的 `no_withdraw.in`、`withdraw_on.in`;每个 case 以 Config 序列组合后跑 `acct_finish`;来源:待采集(方法 B:演示环境;`sapi` 类不可用时 `source = "exchange-doc"`,方法 A);`pm_rejected` 为 `source = "exchange-doc"`
- 期望文件:同目录 `<case>.expected.jsonl`(`AccountConfigReport` 头与杠杆条目)
- 实现:`crates/venue-binance/tests/golden.rs::bin_318_usdm_config_queries`
- 备注:依赖待核实:开发备忘第 5、14、16 条

### BIN-319 金样:Usdm 保证金轮询响应

- 规则:[保证金数据换算](README.md#保证金数据换算)
- 级别:golden
- 里程碑:M3
- 输入:`fixtures/binance-usdm/rest.fapi_v3_account/margin.in`、`rest.positionRisk/` 的 `two_positions.in`、`liq_px_zero.in`、`large_50k.in`、`rest.adlQuantile/normal.in`;来源:待采集(方法 B)
- 期望文件:同目录 `<case>.expected.jsonl`(`MarginReport` 与 `PositionReport`)
- 实现:`crates/venue-binance/tests/golden.rs::bin_319_usdm_margin_poll`
- 备注:依赖待核实:开发备忘第 6 条

### BIN-320 金样:Spot JSON bookTicker

- 规则:[行情频道 → 事件](README.md#行情频道--事件)
- 级别:golden
- 里程碑:M3
- 输入:`fixtures/binance-spot/bookTicker/` 的 `normal.in`、`sci_notation.in`、`missing_b.in`、`huge_qty.in`;来源:collector raw WAL / 合成
- 期望文件:同目录 `<case>.expected.jsonl`(`ex_ts_ns = 0`)
- 实现:`crates/venue-binance/tests/golden.rs::bin_320_spot_bookticker`

### BIN-321 金样:Spot combined 流分档快照 depth5/10/20

- 规则:[分档快照流](README.md#分档快照流)
- 级别:golden
- 里程碑:M3
- 输入:`fixtures/binance-spot/depth5/`、`depth10/`、`depth20/` 各一条 `normal.in`;`depth20/dup_lastUpdateId.in`、`depth20/short_levels.in`、`depth5/unknown_stream_symbol.in`;来源:待采集(方法 B:演示环境 `/stream` 订阅)
- 期望文件:同目录 `<case>.expected.jsonl`
- 实现:`crates/venue-binance/tests/golden.rs::bin_321_spot_partial_depth`
- 备注:依赖待核实:开发备忘第 20 条

### BIN-322 金样:Spot trade

- 规则:[行情频道 → 事件](README.md#行情频道--事件)
- 级别:golden
- 里程碑:M3
- 输入:`fixtures/binance-spot/trade/` 的 `taker_buy.in`、`taker_sell.in`、`ms_unit_on_us_conn.in`(合成:毫秒量级时间戳出现在 µs 连接上,期望按 µs 原样换算)、`missing_t.in`;来源:collector raw WAL / 合成
- 期望文件:同目录 `<case>.expected.jsonl`
- 实现:`crates/venue-binance/tests/golden.rs::bin_322_spot_trade`

### BIN-323 金样:Spot BBO 补帧 REST 响应

- 规则:[心跳与 BBO 探测](README.md#心跳与-bbo-探测)
- 级别:golden
- 里程碑:M3
- 输入:`fixtures/binance-spot/rest.ticker_bookTicker/` 的 `two_symbols.in`、`empty_array.in`;来源:待采集(方法 B)
- 期望文件:同目录 `<case>.expected.jsonl`
- 实现:`crates/venue-binance/tests/golden.rs::bin_323_spot_bbo_refresh`

### BIN-324 金样:Spot session.logon 与 userDataStream.subscribe 响应

- 规则:[下单:WS API](README.md#下单ws-api)、[端点与路由](README.md#端点与路由)
- 级别:golden
- 里程碑:M3
- 输入:`fixtures/binance-spot/session.logon/` 的 `ok.in`、`bad_signature.in`;`fixtures/binance-spot/userDataStream.subscribe/` 的 `ok.in`、`error.in`;来源:待采集(方法 B:演示环境 ws-api)
- 期望文件:同目录 `<case>.expected.jsonl`
- 实现:`crates/venue-binance/tests/golden.rs::bin_324_spot_session`

### BIN-325 金样:Spot order.place、order.cancel、openOrders.cancelAll 响应

- 规则:[下单:WS API](README.md#下单ws-api)、[结果未知码表](README.md#结果未知码表)
- 级别:golden
- 里程碑:M3
- 输入:`fixtures/binance-spot/order.place/` 的 `ack.in`、`err_-2010_balance.in`、`err_-1013_filter.in`、`http_503.in`、`unregistered.in`;`fixtures/binance-spot/order.cancel/` 的 `ok.in`、`err_-2011.in`;`fixtures/binance-spot/openOrders.cancelAll/ok.in`;来源:待采集(方法 B)
- 期望文件:同目录 `<case>.expected.jsonl`
- 实现:`crates/venue-binance/tests/golden.rs::bin_325_spot_order_responses`

### BIN-326 金样:Spot order.amend.keepPriority 与 order.status 响应

- 规则:[下单:WS API](README.md#下单ws-api)、[累计量与费用来源](README.md#累计量与费用来源)
- 级别:golden
- 里程碑:M3
- 输入:`fixtures/binance-spot/order.amend.keepPriority/` 的 `ok.in`、`err_increase_qty.in`;`fixtures/binance-spot/order.status/` 的 `filled.in`、`partially_filled.in`、`not_found.in`;来源:待采集(方法 B)
- 期望文件:同目录 `<case>.expected.jsonl`(`avg_px = cummulativeQuoteQty / executedQty`)
- 实现:`crates/venue-binance/tests/golden.rs::bin_326_spot_amend_status`

### BIN-327 金样:Spot executionReport 全状态与网页单成交

- 规则:[5. 回报解码](README.md#5-回报解码)、[外部成交识别](README.md#外部成交识别)、[VenueCaps 默认值](README.md#venuecaps-默认值)
- 级别:golden
- 里程碑:M3
- 输入:`fixtures/binance-spot/executionReport/` 的 `new.in`、`partially_filled.in`、`filled.in`、`canceled.in`、`expired.in`、`replaced.in`、`web_order_fill.in`、`fee_bnb.in`(BNB 抵扣)、`fee_base_asset.in`(不抵扣,扣所得 BTC)、`dup_trade.in`;来源:待采集(方法 B:演示环境分别开关 BNB 抵扣各成交一笔,开发备忘第 7 条)
- 期望文件:同目录 `<case>.expected.jsonl`
- 实现:`crates/venue-binance/tests/golden.rs::bin_327_spot_execution_report`
- 备注:依赖待核实:开发备忘第 3、7 条

### BIN-328 金样:Spot outboundAccountPosition、可忽略、未知与 eventStreamTerminated

- 规则:[用户流事件表](README.md#用户流事件表)
- 级别:golden
- 里程碑:M3
- 输入:`fixtures/binance-spot/outboundAccountPosition/normal.in`、`fixtures/binance-spot/balanceUpdate/normal.in`、`fixtures/binance-spot/listStatus/normal.in`、`fixtures/binance-spot/externalLockUpdate/normal.in`(`source = "exchange-doc"`)、`fixtures/binance-spot/eventStreamTerminated/normal.in`、`fixtures/binance-spot/unknown_event/synthetic.in`;来源:待采集(方法 B)
- 期望文件:同目录 `<case>.expected.jsonl`
- 实现:`crates/venue-binance/tests/golden.rs::bin_328_spot_user_stream_misc`

### BIN-329 金样:Spot 账户配置查询响应

- 规则:[账户配置核验](README.md#账户配置核验)
- 级别:golden
- 里程碑:M3
- 输入:`fixtures/binance-spot/rest.api_v3_account/` 的 `spot_with_uid.in`、`margin_account.in`、`without_uid.in`;`rest.apiRestrictions/` 的 `no_withdraw.in`、`withdraw_on.in`;`rest.bnbBurn/` 的 `on.in`、`off.in`;来源:待采集(方法 B;演示环境无 `sapi` 时方法 A + `source = "exchange-doc"`)
- 期望文件:同目录 `<case>.expected.jsonl`
- 实现:`crates/venue-binance/tests/golden.rs::bin_329_spot_config_queries`
- 备注:依赖待核实:开发备忘第 5、14 条

### BIN-330 金样:Usdm 与 Spot 挂单列表(只读查询)

- 规则:[只读查询](README.md#只读查询)
- 级别:golden
- 里程碑:M5
- 输入:`fixtures/binance-usdm/rest.openOrders/` 与 `fixtures/binance-spot/rest.openOrders/` 的 `two_orders_one_foreign.in`、`empty.in`;来源:待采集(方法 B:演示环境只读 key)
- 期望文件:同目录 `<case>.expected.jsonl`(`open_order` 行;外来单 `slot = UNRESOLVED`)
- 实现:`crates/venue-binance/tests/golden.rs::bin_330_open_orders`

### BIN-331 金样:Usdm userTrades 与 Spot myTrades(只读查询)

- 规则:[只读查询](README.md#只读查询)
- 级别:golden
- 里程碑:M5
- 输入:`fixtures/binance-usdm/rest.userTrades/` 与 `fixtures/binance-spot/rest.myTrades/` 的 `page.in`、`last_page.in`;来源:待采集(方法 B)
- 期望文件:同目录 `<case>.expected.jsonl`(`fill` 行与 `RoPage`)
- 实现:`crates/venue-binance/tests/golden.rs::bin_331_fills`

### BIN-332 金样:Usdm 资金费流水(只读查询)

- 规则:[只读查询](README.md#只读查询)
- 级别:golden
- 里程碑:M5
- 输入:`fixtures/binance-usdm/rest.income/` 的 `funding_two_symbols.in`;来源:待采集(方法 B;演示环境不结算资金费时用文档示例,`source = "exchange-doc"`)
- 期望文件:同目录 `<case>.expected.jsonl`(`funding` 行,逐笔带 inst 与结算时刻)
- 实现:`crates/venue-binance/tests/golden.rs::bin_332_funding_flow`

BIN-332 补充单元验收:签名 GET、正负现金金额、同刻多合约、满页继续、HTTP/JSON 错误整页失败。实现:`tests/funding.rs::bin_332_income_history_actual_amount_and_signing`、`bin_332_full_page_requires_following_page`。默认 caps 采用 RateOnly;BIN-113–116 保留为可选 BalanceOnlyPush 用户流配置,不与实额历史并用。

### BIN-333 金样:Usdm balance 与 Spot account 余额(只读查询)

- 规则:[只读查询](README.md#只读查询)
- 级别:golden
- 里程碑:M5
- 输入:`fixtures/binance-usdm/rest.balance/normal.in`、`fixtures/binance-spot/rest.account/normal.in`;来源:待采集(方法 B)
- 期望文件:同目录 `<case>.expected.jsonl`(`balance` 行;refdata 之外的资产不产出)
- 实现:`crates/venue-binance/tests/golden.rs::bin_333_balance`

### BIN-334 金样:Usdm positionRisk 持仓(只读查询)

- 规则:[只读查询](README.md#只读查询)
- 级别:golden
- 里程碑:M5
- 输入:`fixtures/binance-usdm/rest.positionRisk/ro_positions.in`;来源:待采集(方法 B)
- 期望文件:同目录 `<case>.expected.jsonl`(`position` 行)
- 实现:`crates/venue-binance/tests/golden.rs::bin_334_positions`

### BIN-400 fuzz:binance-usdm(计入门槛 ③)

- 规则:[hft-venue-proto 契约 5](../hft-venue-proto/README.md#实现方必须遵守的契约)、[8. 源文件规划](README.md#8-源文件规划)
- 级别:fuzz
- 里程碑:M3
- 目标:`fuzz/binance-usdm`:按输入首字节分派——行情(`classify` → `peek` → `decode_md`,对每条路由)、BBO 补帧(`decode_bbo_snapshot`)、回报与用户流(`classify_report` → `decode_report`,同一 `GwCtx` 连续喂)、账户查询(`acct_decode` 各步 + `acct_finish`);对 `Ok` 的输出再断言契约 9(`cum_filled > 0` 时 `avg_px ≠ 0`)
- 语料:`cargo xtask fixtures corpus binance-usdm`(全部 `binance-usdm` 金样输入)+ 变异 + `fixtures/binance-usdm/regressions/`
- 判定:零 panic、零越界、零单输入超时(口径见 [testing §3.3](../../docs/testing.md#33-fuzz));门槛 ③ 的累计归 testing §8
- 实现:`crates/venue-binance/fuzz/fuzz_targets/binance_usdm.rs::bin_400_fuzz_binance_usdm`

### BIN-401 fuzz:binance-spot(计入门槛 ③)

- 规则:[hft-venue-proto 契约 5](../hft-venue-proto/README.md#实现方必须遵守的契约)、[8. 源文件规划](README.md#8-源文件规划)
- 级别:fuzz
- 里程碑:M3
- 目标:`fuzz/binance-spot`:同 BIN-400 的分派,对 `Spot`;启用 `spot-sbe` 时增加 SBE 分支(可选扩展)
- 语料:`cargo xtask fixtures corpus binance-spot` + 变异 + `fixtures/binance-spot/regressions/`
- 判定:零 panic、零越界、零单输入超时(口径见 [testing §3.3](../../docs/testing.md#33-fuzz))
- 实现:`crates/venue-binance/fuzz/fuzz_targets/binance_spot.rs::bin_401_fuzz_binance_spot`

### BIN-402 fuzz:encode 往返(不计入门槛 ③)

- 规则:[4. 订单帧模板](README.md#4-订单帧模板)
- 级别:fuzz
- 里程碑:M3
- 目标:`fuzz/fuzz_binance_encode_roundtrip`:任意字节构造 `GwCmd`(两个产品线、全部订单类 op)→ `encode`;`Ok` 时帧经 `sonic-rs` 解析,字段与 `GwCmd` 一致、`id` 解回 `(op, slot, gen)`;`Err` 只能是 `EncodeErr` 的四个变体之一
- 语料:BIN-064–BIN-071 的 `GwCmd` 序列化 + 变异
- 判定:零 panic、零单输入超时(口径见 [testing §3.3](../../docs/testing.md#33-fuzz));往返不一致按崩溃处理
- 实现:`crates/venue-binance/fuzz/fuzz_targets/encode_roundtrip.rs::bin_402_fuzz_encode_roundtrip`

### BIN-500 基准:classify(Usdm)

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:bench
- 里程碑:M1.5
- 基准名:`bench_classify_usdm`
- 实现:`crates/venue-binance/benches/decode.rs::bin_500_classify_usdm`

### BIN-501 基准:peek bookTicker

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:bench
- 里程碑:M1.5
- 基准名:`bench_peek_bookticker`
- 实现:`crates/venue-binance/benches/decode.rs::bin_501_peek_bookticker`

### BIN-502 基准:decode_md bookTicker → Bbo

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:bench
- 里程碑:M1.5
- 基准名:`bench_decode_bookticker`
- 实现:`crates/venue-binance/benches/decode.rs::bin_502_decode_bookticker`

### BIN-503 基准:decode_md 分档快照 20 档

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:bench
- 里程碑:M3
- 基准名:`bench_decode_depth20`
- 实现:`crates/venue-binance/benches/decode.rs::bin_503_decode_depth20`

### BIN-504 基准:decode_md 现货 JSON bookTicker

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:bench
- 里程碑:M1.5
- 基准名:`bench_decode_spot_bookticker`
- 实现:`crates/venue-binance/benches/decode.rs::bin_504_decode_spot_bookticker`

### BIN-505 基准:encode order.place

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:bench
- 里程碑:M1.5
- 基准名:`bench_encode_place`
- 实现:`crates/venue-binance/benches/encode.rs::bin_505_encode_place`

### BIN-506 基准:decode_report ORDER_TRADE_UPDATE(含外部成交判定与去重)

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:bench
- 里程碑:M3
- 基准名:`bench_decode_otu`
- 实现:`crates/venue-binance/benches/reports.rs::bin_506_decode_otu`

### BIN-507 基准:decode_report 可忽略与未知事件

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:bench
- 里程碑:M3
- 基准名:`bench_skip_event_usdm`
- 实现:`crates/venue-binance/benches/reports.rs::bin_507_skip_event_usdm`

### BIN-508 基准:acct_decode positionRisk 50 KiB

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:bench
- 里程碑:M3
- 基准名:`bench_acct_decode_position_risk`
- 实现:`crates/venue-binance/benches/acct.rs::bin_508_acct_decode_position_risk`

## 验收

- **金样**:BIN-300–BIN-329 全绿;`fixtures/binance-usdm/` 与 `fixtures/binance-spot/` 满足 [fixtures 测试与验收](../../fixtures/README.md#测试与验收) 的第 1–10 条(上线门槛 ②)。金样不对时修 fixture 来源,不改归一化去迁就([fixtures 更新流程](../../fixtures/README.md#更新流程))。标"待采集"的输入在 M3 联调窗口内采齐;只能由正式实盘产生的样本(真实强平、ADL)先用 `source = "exchange-doc"` 的合成样本,上线后用方法 C 补新 case。
- **单元、表驱动与属性**:本文件里程碑 ≤ 当前里程碑的用例全部有同编号实现且通过;属性用例的 PR 例数见各用例,每夜例数 ×100(BIN-012、BIN-074、BIN-079、BIN-151)。
- **契约检查**:BIN-174 对 `Usdm` 与 `Spot` 全过。
- **fuzz**:BIN-400、BIN-401 满足 [testing §8](../../docs/testing.md#8-上线门槛) 门槛 ③(服务器账本中各累计 ≥ 2 CPU·h、零失败);BIN-402 不计入 ③,失败同样必须修复。
- **基准**:BIN-500–BIN-508 在参考机 `bench-gate --reference` 下不超过 README"延迟与资源契约"的预算与 [hft-venue-proto 统一门槛](../hft-venue-proto/README.md#延迟与资源契约)(数值不在本文件);热路径分配为 0(BIN-038)。
- **演示环境**:BIN-214、BIN-215 在 M3 之前完成并写回 README;BIN-216 连续 3 天通过(门槛 ⑥)。
- **可选扩展**:增量深度(`@depth@100ms`、REST 深度快照、`align_first` 四种对齐、`L2Batch` 序号归一化)、现货 SBE(feature `spot-sbe`)、`mock` 反向编解码、`order.cancelReplace` 的用例与对应基准(`bench_decode_depth_2k`、`bench_decode_sbe_bba`、`bench_decode_snapshot_1000`)在启用该扩展的同一改动中补写,启用前必须全绿。
