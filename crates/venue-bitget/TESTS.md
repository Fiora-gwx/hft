---
component: venue-bitget
prefix: BGT
spec: README.md
---

# venue-bitget 测试规格

格式与规则见 [docs/tests](../../docs/tests/README.md)。本 crate 的待核实项多(README 开发备忘第 1–17 条):依赖待核实事实的用例在 `备注` 写明条目号;字段名未定的用例按 README 写的语义写期望,金样输入来源写"待采集(方法)"。"实现前置"与"M3 之前必须核实"的事实本身写成 `testnet` 用例(BGT-210–BGT-218),通过标准是结论写回 README 对应表。

本文件的记号(各用例在 `前置` 中引用,数值即定义,不依赖默认值):

- `RD-BGT` = 测试 refdata `fixtures/refdata/bitget-test-1.blob`(由同目录 `bitget-test-1.toml` 经 hft-cfg 生成):资产 `USDT` = AssetId 1(qty_exp −8)、`BTC` = 2(px_exp −2,qty_exp −8)、`ETH` = 3(px_exp −2,qty_exp −8)、`BGB` = 6(px_exp −4,qty_exp −8);`BITGET_UTA_USDT_FUT`(10):`BTCUSDT` = InstId 1801(tick 0.1,lot 0.001)、`ETHUSDT` = 1802(tick 0.01,lot 0.01),两者 `Trading`;`BITGET_UTA_SPOT`(11):`BTCUSDT` = 1901(tick 0.01,lot 0.000001)、`BGBUSDT` = 1902(tick 0.0001,lot 0.01);全部 inst 的合约平移为 0;账户 `AccountId 2`,`venue_uid = "acct-0001"`,`uid_policy = "verified"`;生效 `VenueCaps` = 本 crate `default_caps()`(用例另行覆盖的注明)。
- 数值写法:价格 `"65000.1"` = 6500010(px_exp −2),数量 `"1.5"` = 150000000、`"0.004"` = 400000(qty_exp −8),费用 `"0.039"` USDT = 3900000(qty_exp −8),`Notional` 为 1e-6(`"10000"` = 10000000000),`Rate` 为 1e-9(`0.025` = 25000000);ms 时间戳 `1790000000123` → ns 1790000000123000000。
- `RX` = `RxMeta{rx_tsc: 1000, hw_rx_ns: 1790000000126000000, sw_rx_ns: 1790000000126500000, prev_override: 0, conn: 3, account: 0, ts_flags: 0, rx_empty_gap_ns: 2500}`(私有连接上 `account = 2`)。
- `CID(e,s,g)` = `ClientOrderId{epoch: e, venue: <本产品线>, account: 2, slot: s, gen: g}` 的规范 24 位文本:永续 CID(3,42,7) = `100030a020002a000000078d`,CID(3,43,1) = `100030a020002b000000018a`,CID(2,42,7) = `100020a020002a000000079d`;现货 CID(3,42,7) = `100030b020002a000000079d`,CID(3,43,1) = `100030b020002b000000019a`。"校验位错" = 末字节异或 1(永续 CID(3,42,7) → `100030a020002a000000078c`)。
- `RID(op,s,g)` = 请求 id `"<op><slot 8 位小写十六进制><gen 8 位小写十六进制>"`,如 `RID(p,42,7)` = `p0000002a00000007`。
- `WC` = `WallClock{unix_ms: 1790000000000, unix_us: 1790000000000000}`。
- `KEY` = 测试凭证 `{api_key: "k", secret: "secret", passphrase: "p"}`(合成,非真实 key)。
- `B1v2(seq, ts)` = V2 形态的 `books1` 推送 `{"action":"snapshot","arg":{"instType":"USDT-FUTURES","channel":"books1","instId":"BTCUSDT"},"data":[{"asks":[["65000.2","0.004"]],"bids":[["65000.1","1.5"]],"checksum":0,"seq":<seq>,"ts":"<ts>"}],"ts":1790000000125}`;`B1v3` = 同内容的 UTA 形态(`arg` 为 `{"instType":"usdt-futures","topic":"books1","symbol":"BTCUSDT"}`,写法待核实,开发备忘第 1 条)。
- `VecSink` = hft-venue-proto `testkit` 的收集 sink;`GoldenDriver` 同出处。

CORE-120 非正式账户启动链路由 `apps/hft-core/tests/account_process.rs::core_120_mock_accounts_bootstrap_reauth_and_drift` 以独立子进程、两家真实协议和脚本 REST 验证撤空、基线、重新核验、超时/错误模式失败与余额差异不覆盖账本；不替代真实 TLS、交易所或正式恢复验收。资金费账单复用 FundingPoll，沿用现有实际结算记账与去重。

## 当前实现验收说明

当前 warm `AccountProtocol` 保证金边界与 i128 对照见 tests/acct.rs 的 BGT-147/148;负权益/不可转换的 USD→USDT 组合仍 fail closed,未覆盖冻结 BGT-147.3 的 Rate::MAX 报告要求。BGT-505 测 warm account_decode 的 50 KiB 持仓响应并包含分配,不声明旧 acct_* 无分配接口达标。BGT-053 用合成行情预热并验证公共 BBO 补帧前后状态不变;完整行情/私有金样仍须带来源的原始样本。

BGT-063 对两份不同凭证构建同值小模板,显式借用凭证签名得到不同请求,旧无凭证入口拒绝。BGT-101 增补跨账户身份、未知/变化的手续费币种、累计金额倒退与同批次原子拒绝;BGT-113 增补非法方向/负量及均价舍入;BGT-114 增补钱包 balance 与 available 不同、永续不重复加冻结、现货只扣一次冻结的断言。BGT-093 增补均价取整与 403/429/5xx 保持 Unknown。

## 覆盖

| README 小节 | 用例 |
|---|---|
| 测试环境 warm 账户入口 | BGT-142、BGT-143 |
| 资金费实额 | BGT-336 |
| 概要 | BGT-171, BGT-190, BGT-210–BGT-218 |
| 目录 | 不适用:非规范 |
| 使用本组件 | BGT-001, BGT-010, BGT-013, BGT-076, BGT-080, BGT-081 |
| 账户形态(官方,本地缓存的 UTA 概览页) | BGT-114, BGT-142, BGT-201 |
| 端点 | BGT-002–BGT-006 |
| 行情频道 → 事件 | BGT-010, BGT-020–BGT-036, BGT-219, BGT-300, BGT-301, BGT-305–BGT-307 |
| 下单:UTA V3 WS | BGT-001, BGT-060–BGT-075, BGT-090, BGT-092, BGT-093, BGT-310, BGT-313–BGT-316, BGT-333, BGT-402 |
| 限速默认值 | BGT-170, BGT-193 |
| 分档快照流 | BGT-002, BGT-028–BGT-030, BGT-302–BGT-304 |
| L2 序号归一化(`L2Batch.prev_id/last_id`) | 不适用:里程碑外 |
| 累计量与费用来源 | BGT-101–BGT-107, BGT-117, BGT-321 |
| 结果未知码表 | BGT-091, BGT-213, BGT-313 |
| VenueCaps 默认值 | BGT-065, BGT-117, BGT-171–BGT-174 |
| 账户配置核验 | BGT-140, BGT-142–BGT-145, BGT-326–BGT-328, BGT-330–BGT-332 |
| 保证金数据换算 | BGT-141, BGT-146–BGT-150, BGT-329 |
| 外部成交识别 | BGT-109–BGT-112, BGT-319, BGT-320 |
| 用户流事件表 | BGT-100, BGT-113–BGT-117, BGT-317, BGT-318, BGT-322–BGT-325 |
| 测试网/模拟盘 | BGT-217, BGT-220 |
| 心跳与 BBO 探测 | BGT-050–BGT-053, BGT-190–BGT-193, BGT-308, BGT-309, BGT-312 |
| 只读查询 | BGT-175, BGT-334–BGT-338 |
| `mock` 特性 | 不适用:里程碑外 |
| 实现说明 | BGT-010–BGT-014, BGT-025–BGT-027, BGT-037, BGT-077, BGT-078, BGT-100, BGT-108, BGT-171, BGT-191, BGT-192, BGT-200, BGT-201, BGT-311, BGT-400, BGT-401 |
| 延迟与资源契约 | BGT-038, BGT-500–BGT-505 |
| 测试与验收 | 不适用:非规范 |
| 依赖 | BGT-221 |
| 延伸阅读 | 不适用:非规范 |
| 已知限制与后续工作 | BGT-171, BGT-210–BGT-215 |
| 开发备忘 | BGT-210–BGT-218 |

## 用例

### BGT-001 两个产品线的身份常量与帧容量

- 规则:[使用本组件](README.md#使用本组件)、[下单:UTA V3 WS](README.md#下单uta-v3-ws)
- 级别:table
- 里程碑:M3
- 前置:无参数;读两个类型的关联常量
- 实现:`crates/venue-bitget/tests/session.rs::bgt_001_identity_consts`

| 行 | 常量 | 期望 |
|---|---|---|
| BGT-001.1 | `BitgetUsdtFutures::NAME` | `"bitget-usdt-futures"`(= `fixtures/bitget-usdt-futures/` 目录名) |
| BGT-001.2 | `BitgetUsdtFutures::VENUE` / `MARKET` | `BITGET_UTA_USDT_FUT`(10) / `MarketKind::LinearPerp`(2) |
| BGT-001.3 | `BitgetSpot::NAME` | `"bitget-spot"` |
| BGT-001.4 | `BitgetSpot::VENUE` / `MARKET` | `BITGET_UTA_SPOT`(11) / `MarketKind::Spot`(1) |
| BGT-001.5 | 两者 `ORDER_FRAME_CAP` | 2048 |
| BGT-001.6 | `VENUE.market()` 与 `MARKET` | 两个产品线各自相等 |

### BGT-002 永续 routes() 恰好四条路由

- 规则:[端点](README.md#端点)、[分档快照流](README.md#分档快照流)
- 级别:table
- 里程碑:M3
- 前置:`BitgetUsdtFutures::routes()`
- 实现:`crates/venue-bitget/tests/session.rs::bgt_002_fut_routes`

| 行 | 路由名 | 期望 |
|---|---|---|
| BGT-002.1 | `books1` | `kind = Bbo`,`race = true`,`max_streams = 1000`,`max_in_msgs_per_s = 10`,`topn_levels = []` |
| BGT-002.2 | `public-topn` | `kind = DepthTopN`,`race = false`,`topn_levels = [5, 50]`(非空、升序;UTA 为 `books5`/`books50`,无 `books15`) |
| BGT-002.3 | `public-trade` | `kind = Md`,`race = false`,`topn_levels = []` |
| BGT-002.4 | `public-ticker` | `kind = Md`,`race = false` |
| BGT-002.5 | 路由总数与种类计数 | 5 条(含 `public-books`);`Bbo` 恰 1 条、`DepthTopN` 恰 1 条;`race = true` 的只有 `Bbo` 路由 |
| BGT-002.6 | `public-books`(`RouteId(4)`,增量 `books`,可选扩展) | `kind = Md`,`race = false` |

### BGT-003 现货 routes() 没有 ticker 路由

- 规则:[端点](README.md#端点)、[行情频道 → 事件](README.md#行情频道--事件)
- 级别:table
- 里程碑:M3
- 前置:`BitgetSpot::routes()`
- 实现:`crates/venue-bitget/tests/session.rs::bgt_003_spot_routes`

| 行 | 路由名 | 期望 |
|---|---|---|
| BGT-003.1 | `books1` | `kind = Bbo`,`race = true`,`max_in_msgs_per_s = 10` |
| BGT-003.2 | `public-topn` | `kind = DepthTopN`,`topn_levels = [5, 50]` |
| BGT-003.3 | `public-trade` | `kind = Md` |
| BGT-003.4 | 名为 `public-ticker` 的路由 | 不存在;路由总数 4(`books1`、`public-topn`、`public-trade`、`public-books`) |

### BGT-004 endpoint 按角色给出 UTA V3 路径

- 规则:[端点](README.md#端点)
- 级别:table
- 里程碑:M3
- 前置:`VenueCfg` 取 `RD-BGT` 的生产端点;`now_ms = 1790000000000`;`cred` 为 `KEY`(只对 Order 角色传入);`boot = None`
- 实现:`crates/venue-bitget/tests/session.rs::bgt_004_endpoint_paths`
- 备注:依赖待核实:开发备忘第 1 条(路径);核实后若路径不同,改 README 端点表与本表同一提交

| 行 | 产品线、角色、路由 | 期望 `EndpointSpec` |
|---|---|---|
| BGT-004.1 | 永续,`Md`,`books1` | `Ok`;路径 `/v3/ws/public`,无查询串,无额外 Upgrade 头 |
| BGT-004.2 | 永续,`Md`,`public-topn` | 路径 `/v3/ws/public` |
| BGT-004.3 | 永续,`Md`,`public-ticker` | 路径 `/v3/ws/public` |
| BGT-004.4 | 永续,`Order` | 路径 `/v3/ws/private`;Upgrade 头不含任何凭证字节 |
| BGT-004.5 | 现货,`Md`,`public-trade` | 路径 `/v3/ws/public` |
| BGT-004.6 | 现货,`Order` | 路径 `/v3/ws/private` |

### BGT-005 私有推送不单独建用户流连接

- 规则:[端点](README.md#端点)
- 级别:unit
- 里程碑:M3
- 前置:同 BGT-004
- 步骤:
  1. 两个产品线各调 `endpoint(Role::UserStream, RouteId(0), …)`
- 期望:
  - 两次都返回 `Err(ProtoErr::Unsupported)`,`out` 未被写入(长度 0)
- 实现:`crates/venue-bitget/tests/session.rs::bgt_005_no_user_stream_role`

### BGT-006 全部 REST 请求使用端点下标 0

- 规则:[端点](README.md#端点)
- 级别:table
- 里程碑:M3
- 前置:`RD-BGT`;`KEY`;`WC`;各请求按其用例的输入生成
- 实现:`crates/venue-bitget/tests/routes.rs::bgt_006_rest_endpoint_index`

| 行 | 请求 | 期望 |
|---|---|---|
| BGT-006.1 | `acct_request` 永续 Config 每一步(0、1、2、5) | `RestReqBuf.endpoint = 0` |
| BGT-006.2 | `acct_request` 现货 Config 每一步(0、2、3、4) | `endpoint = 0` |
| BGT-006.3 | `acct_request` 永续 Margin 步 0、1 | `endpoint = 0` |
| BGT-006.4 | `bbo_snapshot_request(insts = [1801])` | `endpoint = 0` |
| BGT-006.5 | `Query` 的 REST 请求(BGT-072) | `endpoint = 0` |

### BGT-010 行情 classify 按路由分类,不依赖频道名的位置

- 规则:[实现说明](README.md#实现说明)、[行情频道 → 事件](README.md#行情频道--事件)
- 级别:table
- 里程碑:M3
- 前置:`BitgetUsdtFutures`;`op = MsgOp::Text`(除注明外);`head` = payload 的前 min(64, len) 字节;路由 id 取 BGT-002 的顺序(0 `books1`、1 `public-topn`、2 `public-trade`、3 `public-ticker`)
- 实现:`crates/venue-bitget/tests/md.rs::bgt_010_classify_md`

| 行 | 路由与 payload 开头 | 期望 `MdClass` |
|---|---|---|
| BGT-010.1 | `books1`,`B1v2`(`{"action":"snapshot","arg":…`,频道名在第 65 字节) | `Books1` |
| BGT-010.2 | `books1`,`B1v3`(`{"arg":{"instType":"usdt-futures","topic":"books1"…`) | `Books1` |
| BGT-010.3 | `public-topn`,`{"action":"snapshot","arg":{"instType":"USDT-FUTURES","channel":"books15"…` | `BooksTopN` |
| BGT-010.4 | `public-topn`,`{"action":"update","arg":{…"channel":"books5"…` | `BooksTopN` |
| BGT-010.5 | `public-trade`,`{"action":"update","arg":{…"channel":"trade"…` | `Trade` |
| BGT-010.6 | `public-ticker`,`{"action":"snapshot","arg":{…"channel":"ticker"…` | `Ticker` |
| BGT-010.7 | 任一路由,`{"event":"subscribe","arg":{…}}` | `SubAck` |
| BGT-010.8 | 任一路由,`{"event":"error","code":30001,"msg":"instType:USDT-FUTURES,channel:books1,instId:XXX doesn't exist"}` | `SubAck` |
| BGT-010.9 | 任一路由,`{"event":"unsubscribe","arg":{…}}` | `SubAck` |
| BGT-010.10 | 任一路由,文本 `pong` | `Pong` |
| BGT-010.11 | 任一路由,`{"event":"login","code":0}` | `Unknown`(行情连接不登录) |
| BGT-010.12 | `books1`,`op = Binary`,任意字节 | `Unknown` |
| BGT-010.13 | `books1`,空 payload | `Unknown` |
| BGT-010.14 | `books1`,payload `{"ac`(4 字节) | `Unknown` |
| BGT-010.15 | `books1`,`[1,2,3]` | `Unknown` |
| BGT-010.16 | `books1`,文本 `ping` | `Unknown` |
| BGT-010.17 | `public-books`(`RouteId(4)`),增量 `books` 推送 | `BooksIncr` |
| BGT-010.18 | 任一路由,`{"code":30001,"msg":…,"event":"error"}`(`event` 不在开头) | `SubAck` |

### BGT-011 现货行情 classify 与永续同一规则

- 规则:[实现说明](README.md#实现说明)
- 级别:table
- 里程碑:M3
- 前置:`BitgetSpot`;路由 id 取 BGT-003 的顺序(0 `books1`、1 `public-topn`、2 `public-trade`);`instType` 为 `SPOT`(V2)/`spot`(UTA)
- 实现:`crates/venue-bitget/tests/md.rs::bgt_011_classify_md_spot`

| 行 | 路由与 payload 开头 | 期望 `MdClass` |
|---|---|---|
| BGT-011.1 | `books1`,V2 现货 `books1` 推送 | `Books1` |
| BGT-011.2 | `public-topn`,V2 现货 `books5` 推送 | `BooksTopN` |
| BGT-011.3 | `public-trade`,UTA 现货 `publicTrade` 推送 | `Trade` |
| BGT-011.4 | `RouteId(3)`(现货没有该路由),任意数据推送 | `Unknown` |
| BGT-011.5 | `public-trade`,`{"event":"error","code":30001,…}` | `SubAck` |

### BGT-012 classify 只看前 64 字节

- 规则:[实现说明](README.md#实现说明)、[契约](../hft-venue-proto/README.md#实现方必须遵守的契约)
- 级别:prop
- 里程碑:M3
- 性质:两个 payload 的前 64 字节相同时,`classify(route, op, ·)` 与 `classify_report(op, ·)` 的结果相同;对任意输入不 panic
- 生成:两个产品线 × 全部路由 × `op ∈ {Text, Binary}`;前 64 字节取自金样(BGT-300–BGT-327 的 `.in`)或 0–64 个随机字节,其后各接 0–4096 个独立随机字节
- 参考:同一前缀的两次调用结果相等
- 例数:PR 5 000
- 实现:`crates/venue-bitget/tests/classify.rs::bgt_012_classify_reads_64_bytes`

### BGT-013 classify_report 区分响应、私有推送与控制帧

- 规则:[实现说明](README.md#实现说明)、[用户流事件表](README.md#用户流事件表)
- 级别:table
- 里程碑:M3
- 前置:`BitgetUsdtFutures`;`op = Text`;`head` = payload 前 min(64, len) 字节
- 实现:`crates/venue-bitget/tests/reports.rs::bgt_013_classify_report`
- 备注:依赖待核实:开发备忘第 1 条(UTA 私有频道名与 `arg` 写法)

| 行 | payload 开头 | 期望 `ReportClass` |
|---|---|---|
| BGT-013.1 | `{"id":"p0000002a00000007",…` | `WsResp` |
| BGT-013.2 | `{"id":"c0000002a00000007",…` | `WsResp` |
| BGT-013.3 | `{"id":"m0000002a00000007",…` | `WsResp` |
| BGT-013.4 | `{"id":"x0000002a00000007",…`(`CancelAll` 不编码成帧,`x` 不是本所的 op) | `Unknown` |
| BGT-013.5 | `{"id":"z0000002a00000007",…`(未知 op 码) | `Unknown` |
| BGT-013.6 | `{"arg":{"instType":"usdt-futures","topic":"order"…`(频道名在 64 B 内) | `Orders` |
| BGT-013.7 | `{"arg":{"instType":"usdt-futures","topic":"position"…` | `Positions` |
| BGT-013.8 | `{"arg":{"instType":"usdt-futures","topic":"account"…` | `Account` |
| BGT-013.9 | `{"action":"snapshot","arg":{"instType":"USDT-FUTURES","channel":"orders"…`(频道名从第 65 字节起) | `Push` |
| BGT-013.10 | `{"event":"login","code":0}` | `Control` |
| BGT-013.11 | `{"event":"error","code":30001,…}` | `Control` |
| BGT-013.12 | 文本 `pong` | `Pong` |
| BGT-013.13 | `{"arg":{"instType":"usdt-futures","topic":"strategy-x"…`(不认得的频道) | `Unknown` |
| BGT-013.14 | 空 payload | `Unknown` |
| BGT-013.15 | `op = Binary` | `Unknown` |
| BGT-013.16 | `{"event":"subscribe","arg":{"instType":"UTA","topic":"order"}}` | `Control` |
| BGT-013.17 | `{"code":"25204","msg":"Order does not exist","id":"c0000002a00000007","event":"error"}`(请求 id 超出前 64 B) | `Control`;`decode_report` 读到本系统请求 id 后按响应解码:一条 `CancelReject{detail: GONE}` |

### BGT-014 Push 类经 decode_report 解析后与直接分类的结果相同

- 规则:[实现说明](README.md#实现说明)
- 级别:unit
- 里程碑:M3
- 前置:`RD-BGT`;同一条订单推送(BGT-101 的部分成交)分别以 V2 action 在前的形态(`classify_report` → `Push`)与 UTA `arg` 在前的形态(→ `Orders`)给出
- 步骤:
  1. 各自 `decode_report(rd, ctx_a/ctx_b, class, payload, &RX, &mut sink)`(两个新的 `GwCtx`)
- 期望:
  - 两次输出的 `OrderEvent` 逐字节相同(`PartialFill`,字段见 BGT-101);`ReportOutcome.events = 1`、`skip = No`
- 实现:`crates/venue-bitget/tests/reports.rs::bgt_014_push_class_resolves`

### BGT-020 seq_rule 与 resync_method 按类给出

- 规则:[行情频道 → 事件](README.md#行情频道--事件)、[契约](../hft-venue-proto/README.md#实现方必须遵守的契约)
- 级别:table
- 里程碑:M3
- 前置:两个产品线各断言一遍
- 实现:`crates/venue-bitget/tests/md.rs::bgt_020_seq_rule_resync`

| 行 | `MdClass` | 期望 `seq_rule` / `resync_method` |
|---|---|---|
| BGT-020.1 | `Books1` | `Snapshot` / `None` |
| BGT-020.2 | `BooksTopN` | `Snapshot` / `None` |
| BGT-020.3 | `Trade` | `Monotonic` / `None` |
| BGT-020.4 | `Ticker` | `ByTime` / `None` |
| BGT-020.5 | `SubAck`、`Pong`、`Unknown` | `None` / `None` |
| BGT-020.6 | `BooksIncr` | `Chain` / `Resubscribe` |
| BGT-020.7 | `align_first(100, raw)`:`[pseq, seq]` = `[99, 101]`、`[100, 105]`、`[90, 100]`、`[101, 105]` | `Apply`、`Apply`、`Drop`、`Gap`(快照 `seq` 落在首条增量的 `[pseq, seq]` 内才接上) |

### BGT-021 peek books1 取 inst 与 seq,不取 prev

- 规则:[行情频道 → 事件](README.md#行情频道--事件)
- 级别:unit
- 里程碑:M3
- 前置:`RD-BGT`;新建 `FeedCtx`;payload = `B1v2(seq = 123456789, ts = 1790000000123)`
- 步骤:
  1. `peek(rd, &ctx, Books1, payload)`
- 期望:
  - `Ok(Peek{inst: 1801, ids: SeqIds{prev_id: 0, last_id: 123456789}})`
- 实现:`crates/venue-bitget/tests/md.rs::bgt_021_peek_books1`

### BGT-022 peek 对未订阅符号给 inst = 0

- 规则:[行情频道 → 事件](README.md#行情频道--事件)
- 级别:unit
- 里程碑:M3
- 前置:`RD-BGT`;payload = `B1v2` 但 `instId = "DOGEUSDT"`(不在 refdata)
- 步骤:
  1. `peek(rd, &ctx, Books1, payload)`
- 期望:
  - `Ok(Peek{inst: 0, …})`;不返回 `Err`
- 实现:`crates/venue-bitget/tests/md.rs::bgt_022_peek_unknown_symbol`

### BGT-023 peek 不解析价量

- 规则:[契约](../hft-venue-proto/README.md#实现方必须遵守的契约)
- 级别:unit
- 里程碑:M3
- 前置:`RD-BGT`;payload = `B1v2(seq = 5, ts = 1790000000123)` 但买一价写成 `"abc"`
- 步骤:
  1. `peek(rd, &ctx, Books1, payload)`
  2. `decode_md(rd, &mut ctx, Books1, payload, &RX, &mut sink)`
- 期望:
  - 步骤 1 `Ok(Peek{inst: 1801, ids.last_id: 5})`
  - 步骤 2 `Err(DecodeErr::Number)`;`sink` 中 0 个事件
- 实现:`crates/venue-bitget/tests/md.rs::bgt_023_peek_skips_prices`

### BGT-024 books1 解码为 Bbo,时间戳与接收元数据逐字段

- 规则:[行情频道 → 事件](README.md#行情频道--事件)、[契约](../hft-venue-proto/README.md#实现方必须遵守的契约)
- 级别:unit
- 里程碑:M3
- 前置:`RD-BGT`;`RX`;payload = `B1v2(seq = 123456789, ts = 1790000000123)`
- 步骤:
  1. `decode_md(rd, &mut ctx, Books1, payload, &RX, &mut sink)`
- 期望:
  - 返回 `Ok(MdOutcome{events: 1, inst: 1801})`
  - `sink` 恰一条 `Bbo{h: MarketHdr{venue: 10, conn: 3, inst: 1801, ex_ts_ns: 1790000000123000000, ex_send_ns: 1790000000125000000, hw_rx_ns: 1790000000126000000, rx_tsc: 1000, sw_rx_ns: 1790000000126500000, rx_empty_gap_ns: 2500}, bid_px: 6500010, bid_qty: 150000000, ask_px: 6500020, ask_qty: 400000, update_id: 123456789}`
- 实现:`crates/venue-bitget/tests/md.rs::bgt_024_books1_to_bbo`

### BGT-025 seq 与 ts 的数字、字符串两种形式解析结果相同

- 规则:[实现说明](README.md#实现说明)
- 级别:table
- 里程碑:M3
- 前置:`RD-BGT`;以 BGT-024 的 payload 为基准,只改下列字段的 JSON 形式;每行与 BGT-024 的输出逐字节比较
- 实现:`crates/venue-bitget/tests/md.rs::bgt_025_number_string_forms`

| 行 | 改动 | 期望 |
|---|---|---|
| BGT-025.1 | `"seq":"123456789"`(字符串) | 与 BGT-024 相同 |
| BGT-025.2 | `"ts":1790000000123`(数字) | 与 BGT-024 相同 |
| BGT-025.3 | 顶层 `"ts":"1790000000125"`(字符串) | 与 BGT-024 相同 |
| BGT-025.4 | `"seq":"12a"` | `Err(DecodeErr::Number)`,0 个事件 |
| BGT-025.5 | `"seq":-1` | `Err(DecodeErr::Number)` |
| BGT-025.6 | `"seq":18446744073709551616`(u64 + 1) | `Err(DecodeErr::Overflow)` |
| BGT-025.7 | `"ts":"1790000000123.5"` | `Err(DecodeErr::Number)` |

### BGT-026 任意 seq/ts 的数字与字符串形式解码一致

- 规则:[实现说明](README.md#实现说明)
- 级别:prop
- 里程碑:M3
- 性质:同一 `books1`/`books5`/`ticker` 推送中 `seq`、`data.ts`、顶层 `ts` 各自随机写成数字或字符串时,`peek` 与 `decode_md` 的输出逐字节相同
- 生成:`seq ∈ [0, 2^63)`,`ts ∈ [1, 9_223_372_036_853]`(ms,乘 10^6 不溢出);价量取 RD-BGT 合法 tick/lot 的随机倍数;每个字段独立以 1/2 概率取字符串
- 参考:全数字形式的输出
- 例数:PR 2 000
- 实现:`crates/venue-bitget/tests/md.rs::bgt_026_prop_number_string_forms`

### BGT-027 books1 的失败路径

- 规则:[行情频道 → 事件](README.md#行情频道--事件)、[实现说明](README.md#实现说明)、[契约](../hft-venue-proto/README.md#实现方必须遵守的契约)
- 级别:table
- 里程碑:M3
- 前置:`RD-BGT`;以 BGT-024 的 payload 为基准,每行只做一处改动
- 实现:`crates/venue-bitget/tests/md.rs::bgt_027_books1_errors`

| 行 | 改动 | 期望 |
|---|---|---|
| BGT-027.1 | 删除 `bids` 键 | `Err(DecodeErr::Shape)`,0 个事件 |
| BGT-027.2 | `"bids":[]` | `Ok`,一条 `Bbo{bid_px: 0, bid_qty: 0}`,卖侧取报文值(空侧由下游按缺一侧处理) |
| BGT-027.3 | `"asks":[]` | `Ok`,一条 `Bbo{ask_px: 0, ask_qty: 0}`,买侧取报文值 |
| BGT-027.4 | payload 在 `"bids":[["65000` 处截断 | `Err(DecodeErr::Truncated)` |
| BGT-027.5 | 买一价 `""` | `Err(DecodeErr::Number)` |
| BGT-027.6 | 买一价 `"65000.123"`(比 px_exp −2 多一位非零) | `Err(DecodeErr::Number)` |
| BGT-027.7 | 买一价 `"65000.10000"`(超精度位全为 0) | `Ok`,`bid_px = 6500010` |
| BGT-027.8 | 买一价 `"6.50001E4"`(科学计数) | `Ok`,`bid_px = 6500010` |
| BGT-027.9 | 买一量 `"100000000000000"`(10^14 BTC,raw 超 i64) | `Err(DecodeErr::Overflow)` |
| BGT-027.10 | 档位 `["65000.1"]`(缺数量) | `Err(DecodeErr::Shape)` |
| BGT-027.11 | `data` 为空数组 | `Err(DecodeErr::Shape)` |

### BGT-028 books5 解码为一条 TopN 快照

- 规则:[分档快照流](README.md#分档快照流)、[契约](../hft-venue-proto/README.md#实现方必须遵守的契约)
- 级别:unit
- 里程碑:M3
- 前置:`RD-BGT`;`RX`;路由 `public-topn`;payload = V2 永续 `books5`,`seq = 777`,`data.ts = "1790000000200"`,顶层 `ts = 1790000000202`,买 `[["65000.1","1"],["65000","2"],["64999.9","3"],["64999.8","4"],["64999.7","5"]]`,卖 `[["65000.2","1"],["65000.3","2"],["65000.4","3"],["65000.5","4"],["65000.6","5"]]`
- 步骤:
  1. `decode_md(rd, &mut ctx, BooksTopN, payload, &RX, &mut sink)`
- 期望:
  - `Ok(MdOutcome{events: 1, inst: 1801})`
  - `sink` 恰一次 `l2_snapshot`:`hdr = L2SnapHdr{h.inst: 1801, h.ex_ts_ns: 1790000000200000000, h.ex_send_ns: 1790000000202000000, snap_id: 777, origin: TopN(3), n_bids: 5, n_asks: 5}`;买档价依次 6500010、6500000、6499990、6499980、6499970(降序),卖档 6500020…6500060(升序);数量依次 100000000…500000000;`part = 0`、`parts = 1`
  - `l2_begin`/`l2_level`/`l2_end` 调用 0 次
- 实现:`crates/venue-bitget/tests/md.rs::bgt_028_books5_topn`

### BGT-029 books50 档数不足与零量档

- 规则:[分档快照流](README.md#分档快照流)、[行情频道 → 事件](README.md#行情频道--事件)
- 级别:unit
- 里程碑:M3
- 前置:`RD-BGT`;payload = UTA 永续 `books50`,`seq = 900`,买 3 档(第 2 档数量 `"0"`),卖 50 档
- 步骤:
  1. `decode_md(rd, &mut ctx, BooksTopN, payload, &RX, &mut sink)`
- 期望:
  - 一条 `L2Snapshot{origin = TopN, snap_id = 900, n_bids: 2, n_asks: 50}`;零量档不出现在买档中
- 实现:`crates/venue-bitget/tests/md.rs::bgt_029_books50_short_and_zero`

### BGT-030 同一 seq 的分档快照两次 peek 序号相同、解码字节相同

- 规则:[分档快照流](README.md#分档快照流)
- 级别:unit
- 里程碑:M3
- 前置:`RD-BGT`;BGT-028 的 payload 喂两次(同一 `FeedCtx`)
- 步骤:
  1. 两次 `peek`,两次 `decode_md`
- 期望:
  - 两次 `peek` 的 `ids.last_id` 都是 777(运行时按 `Snapshot` 规则判第二条为重复)
  - 两次解码的 `L2SnapHdr` 与档位逐字节相同(venue 层不去重)
- 实现:`crates/venue-bitget/tests/md.rs::bgt_030_topn_same_seq`

### BGT-031 订阅后的 trade 快照批不产生 Trade

- 规则:[行情频道 → 事件](README.md#行情频道--事件)
- 级别:unit
- 里程碑:M3
- 前置:`RD-BGT`;路由 `public-trade`;推送 1 = `action:"snapshot"`,`data` 含 50 笔历史成交;推送 2 = `action:"update"`,`data` 含 2 笔(`tradeId` `"1000000000000000001"`、`"1000000000000000002"`)
- 步骤:
  1. `decode_md` 推送 1
  2. `decode_md` 推送 2
- 期望:
  - 步骤 1 返回 `Ok(MdOutcome{events: 0, inst: 1801})`,`sink` 无 `Trade`
  - 步骤 2 `events = 2`,两条 `Trade` 的 `trade_id` 依次为 1000000000000000001、1000000000000000002
- 实现:`crates/venue-bitget/tests/md.rs::bgt_031_trade_snapshot_dropped`

### BGT-032 trade 字段映射

- 规则:[行情频道 → 事件](README.md#行情频道--事件)
- 级别:table
- 里程碑:M3
- 前置:`RD-BGT`;`RX`;V2 永续 trade update 推送,单笔 `{"ts":"1790000000100","price":"65000.1","size":"0.002","side":<s>,"tradeId":"1234567890123456789"}`,顶层 `ts = 1790000000102`
- 实现:`crates/venue-bitget/tests/md.rs::bgt_032_trade_fields`
- 备注:依赖待核实:开发备忘第 1 条(UTA 的 `side` 是否为 taker 方向)

| 行 | `side` 与其他改动 | 期望 |
|---|---|---|
| BGT-032.1 | `"buy"` | `Trade{h.inst: 1801, h.ex_ts_ns: 1790000000100000000, h.ex_send_ns: 1790000000102000000, px: 6500010, qty: 200000, trade_id: 1234567890123456789, taker_side: Buy}` |
| BGT-032.2 | `"sell"` | 同上,`taker_side: Sell` |
| BGT-032.3 | `"hold"`(不是 buy/sell) | `Err(DecodeErr::Shape)` |
| BGT-032.4 | `"tradeId":"abc"` | `Err(DecodeErr::Number)` |
| BGT-032.5 | `"tradeId":"18446744073709551616"` | `Err(DecodeErr::Overflow)` |
| BGT-032.6 | 删除 `tradeId` | `Err(DecodeErr::Shape)` |

### BGT-033 ticker 产出标记价、指数价与资金费三条事件

- 规则:[行情频道 → 事件](README.md#行情频道--事件)
- 级别:unit
- 里程碑:M3
- 前置:`RD-BGT`;`RX`;新 `FeedCtx`;V2 永续 ticker:`markPrice "65001.2"`、`indexPrice "64990.5"`、`fundingRate "0.0001"`、`nextFundingTime "1790006400000"`、`data.ts "1790000000300"`,顶层 `ts = 1790000000301`
- 步骤:
  1. `decode_md(rd, &mut ctx, Ticker, payload, &RX, &mut sink)`
- 期望:
  - `Ok(MdOutcome{events: 3, inst: 1801})`
  - `MarkPrice{h.ex_ts_ns: 1790000000300000000, h.ex_send_ns: 1790000000301000000, px: 6500120}`
  - `IndexPrice{同一 h, px: 6499050}`
  - `Funding{同一 h, rate: 100000, next_funding_ns: 1790006400000000000, interval_s: 0}`
- 实现:`crates/venue-bitget/tests/md.rs::bgt_033_ticker_events`
- 备注:依赖待核实:开发备忘第 1 条(UTA ticker 字段名)

### BGT-034 资金费周期由相邻两次不同的 nextFundingTime 推出

- 规则:[行情频道 → 事件](README.md#行情频道--事件)
- 级别:table
- 里程碑:M3
- 前置:`RD-BGT`;同一 `FeedCtx` 按行序依次解码 BTCUSDT 的 ticker,只改 `nextFundingTime`(记 A = 1790006400000)
- 实现:`crates/venue-bitget/tests/md.rs::bgt_034_funding_interval`

| 行 | `nextFundingTime` | 期望 `Funding.interval_s` |
|---|---|---|
| BGT-034.1 | A | 0(首次见到) |
| BGT-034.2 | A | 0(尚未变过) |
| BGT-034.3 | A + 28 800 000 | 28800 |
| BGT-034.4 | A + 28 800 000 | 28800 |
| BGT-034.5 | A + 43 200 000(周期变为 4 h) | 14400 |
| BGT-034.6 | ETHUSDT 的 ticker,`nextFundingTime = A` | 0(每 inst 独立) |
| BGT-034.7 | BTCUSDT,`nextFundingTime = A`(回退) | `interval_s = 0`;不 panic、不产生负值 |

### BGT-035 每种行情事件原样携带接收元数据

- 规则:[契约](../hft-venue-proto/README.md#实现方必须遵守的契约)
- 级别:table
- 里程碑:M3
- 前置:`RD-BGT`;`RX`;每行用该事件对应的用例输入
- 实现:`crates/venue-bitget/tests/md.rs::bgt_035_rxmeta_passthrough`

| 行 | 事件(输入) | 期望 `MarketHdr` 的 `venue/conn/hw_rx_ns/rx_tsc/sw_rx_ns/rx_empty_gap_ns` |
|---|---|---|
| BGT-035.1 | `Bbo`(BGT-024) | 10 / 3 / 1790000000126000000 / 1000 / 1790000000126500000 / 2500 |
| BGT-035.2 | `L2Snapshot` 头(BGT-028) | 同上 |
| BGT-035.3 | `Trade`(BGT-032.1) | 同上 |
| BGT-035.4 | `MarkPrice`、`IndexPrice`、`Funding`(BGT-033) | 同上 |
| BGT-035.5 | 现货 `Bbo`(BGT-024 的现货版本) | `venue = 11`,其余同上 |

### BGT-036 decode_md 以不处理的类调用返回 UnknownClass

- 规则:[契约](../hft-venue-proto/README.md#实现方必须遵守的契约)
- 级别:table
- 里程碑:M3
- 前置:`RD-BGT`;任意合法 `books1` payload
- 实现:`crates/venue-bitget/tests/md.rs::bgt_036_unknown_class`

| 行 | 调用 | 期望 |
|---|---|---|
| BGT-036.1 | `decode_md(…, SubAck, …)` | `Err(DecodeErr::UnknownClass)`,0 个事件 |
| BGT-036.2 | `decode_md(…, Pong, …)` | `Err(DecodeErr::UnknownClass)` |
| BGT-036.3 | `decode_md(…, Unknown, …)` | `Err(DecodeErr::UnknownClass)` |
| BGT-036.4 | `decode_md(…, BooksIncr, …)`(增量 `books`,可选扩展):`action = snapshot`(`pseq = 0`,`seq = 100`)、随后 `update`(`pseq = 100`,`seq = 101`,买 `65000.1` 量 `0`)、重置推送(`update`,`pseq = 0`) | 快照 → 一条 `L2Snapshot{origin: Exchange, snap_id: 100}`(零量档不出现);`update` → `peek` 的 `ids = {prev_id: 100, last_id: 101}`,`l2_begin{prev_id: 100, last_id: 101}` → 各档 `l2_level`(量 0 = 删档)→ `l2_end`;`pseq = 0` 的推送按新快照解码 |
| BGT-036.5 | 现货 `decode_md(…, Ticker, …)` | `Err(DecodeErr::UnknownClass)` |

### BGT-037 control 给出订阅回执与心跳回应

- 规则:[实现说明](README.md#实现说明)、[心跳与 BBO 探测](README.md#心跳与-bbo-探测)
- 级别:table
- 里程碑:M3
- 前置:两个产品线
- 实现:`crates/venue-bitget/tests/md.rs::bgt_037_control_hints`

| 行 | 类与 payload | 期望 `control(c, payload)` |
|---|---|---|
| BGT-037.1 | `SubAck`,`{"event":"subscribe","arg":{…}}` | `Some(ControlHint::SubAck{ok: true, code: 0})` |
| BGT-037.2 | `SubAck`,`{"event":"error","code":30001,"msg":"…"}` | `Some(SubAck{ok: false, code: 30001})` |
| BGT-037.3 | `SubAck`,`{"event":"error","code":"30001",…}`(字符串码) | `Some(SubAck{ok: false, code: 30001})` |
| BGT-037.4 | `SubAck`,`{"event":"unsubscribe","arg":{…}}` | `Some(SubAck{ok: true, code: 0})` |
| BGT-037.5 | `Pong`,`pong` | `Some(ControlHint::Pong)` |
| BGT-037.6 | `Books1`,`B1v2` | `None` |
| BGT-037.7 | `SubAck`,`{"event":"error"}`(无 code) | `Some(SubAck{ok: false, code: 0})` |
| BGT-037.8 | `SubAck`,`{"event":"subscribe",…,"code":"30001","msg":…}`(UTA 拒绝写在 subscribe 事件里) | `Some(SubAck{ok: false, code: 30001})` |

### BGT-038 热路径方法零分配、不 panic

- 规则:[延迟与资源契约](README.md#延迟与资源契约)、[契约](../hft-venue-proto/README.md#实现方必须遵守的契约)
- 级别:unit
- 里程碑:M3
- 前置:计数分配器(`#[global_allocator]` 包装);`RD-BGT`;`FeedCtx`、`GwCtx`、`Templates` 在计数开始前建好
- 步骤:
  1. 对 BGT-300–BGT-327 的全部行情与回报 `.in` 各调 `classify`/`peek`/`decode_md` 或 `classify_report`/`decode_report` 共 10⁴ 次
  2. 对 BGT-064 的 `GwCmd` 调 `encode` 10⁴ 次
- 期望:
  - 两步期间分配次数 0、释放次数 0
  - 全部调用返回 `Ok` 或 `Err`,无 panic
- 实现:`crates/venue-bitget/tests/alloc.rs::bgt_038_hot_path_no_alloc`

### BGT-050 BBO 补帧请求每次一个 inst、公共接口不签名

- 规则:[心跳与 BBO 探测](README.md#心跳与-bbo-探测)
- 级别:unit
- 里程碑:M3
- 前置:`RD-BGT`;snap-fetch 自己的新 `FeedCtx`
- 步骤:
  1. 永续 `bbo_snapshot_request(rd, &ctx, &[1801], &mut out)`
  2. 现货 `bbo_snapshot_request(rd, &ctx, &[1902], &mut out)`
- 期望:
  - 步骤 1 返回 `Ok(true)`;`out.endpoint = 0`、`method = GET`;`path_query` 是一档深度接口且查询串含 `symbol=BTCUSDT`;`headers` 不含 `ACCESS-KEY`、`ACCESS-SIGN`;`redacted = false`
  - 步骤 2 返回 `Ok(true)`,查询串含 `symbol=BGBUSDT`
- 实现:`crates/venue-bitget/tests/bbo.rs::bgt_050_bbo_public_request`
- 备注:依赖待核实:开发备忘第 16 条(接口路径、订阅后是否立即推送;核实为立即推送后改为返回 `Ok(false)`,本用例同一提交改写)

### BGT-051 BBO 补帧请求的 inst 个数不为 1 时不支持

- 规则:[心跳与 BBO 探测](README.md#心跳与-bbo-探测)
- 级别:table
- 里程碑:M3
- 前置:`RD-BGT`;永续
- 实现:`crates/venue-bitget/tests/bbo.rs::bgt_051_bbo_invalid_inst_count`

| 行 | `insts` | 期望 |
|---|---|---|
| BGT-051.1 | `[]` | `Err(ProtoErr::Unsupported)`,`out` 未写 |
| BGT-051.2 | `[1801, 1802]`(超过 `BBO_SNAPSHOT_BATCH = 1`) | `Err(ProtoErr::BufFull)` |
| BGT-051.3 | `[1901]`(现货 inst,不属于本 venue) | `Err(ProtoErr::Unsupported)` |

### BGT-052 BBO 补帧响应解码为一条 Bbo

- 规则:[心跳与 BBO 探测](README.md#心跳与-bbo-探测)、[契约](../hft-venue-proto/README.md#实现方必须遵守的契约)
- 级别:table
- 里程碑:M3
- 前置:`RD-BGT`;`RX`;以 `decode_bbo_snapshot(rd, &mut ctx, &[1801], body, &RX, &mut sink)` 调用;响应体 = `{"code":"00000","msg":"success","data":{"b":[[65000.1,1.5]],"a":[[65000.2,0.004]],"ts":"1790000000400"}}`(字段按官方 UTA market/orderbook,2026-10-15 核对)
- 实现:`crates/venue-bitget/tests/bbo.rs::bgt_052_bbo_raw_decimal_numbers`

| 行 | 响应变体 | 期望 |
|---|---|---|
| BGT-052.1 | 额外带未定义的 `"seq":555` | `Ok(1)`;一条 `Bbo{h.inst: 1801, h.ex_ts_ns: 1790000000400000000, bid_px: 6500010, bid_qty: 150000000, ask_px: 6500020, ask_qty: 400000, update_id: 0}` |
| BGT-052.2 | 无序号字段 | `Ok(1)`;`update_id = 0` |
| BGT-052.3 | `{"code":"40034","msg":"Parameter does not exist","data":null}` | `Err(DecodeErr::Shape)`,0 个事件 |
| BGT-052.4 | `a` 为空 | `Err(DecodeErr::Shape)` |
| BGT-052.5 | 在 `"b":[["650` 截断 | `Err(DecodeErr::Truncated)` |
| BGT-052.6 | 请求 inst 为另一 venue | `Err(DecodeErr::Shape)`,0 个事件 |

### BGT-053 BBO 补帧解码不依赖 feed 的 FeedCtx 状态

- 规则:[契约](../hft-venue-proto/README.md#实现方必须遵守的契约)
- 级别:unit
- 里程碑:M3
- 前置:`RD-BGT`;`ctx_a` = 新 `FeedCtx`;`ctx_b` = 已用 `decode_md` 解码过 10⁴ 条 BGT-300、BGT-302、BGT-307 输入的 `FeedCtx`
- 步骤:
  1. 在 `ctx_a`、`ctx_b` 上分别 `decode_bbo_snapshot` BGT-052.1 的响应
  2. `ctx_b` 上再解码 BGT-034 的 ticker 序列,与在全新 `FeedCtx` 上(未做步骤 1)解码同一序列比较
- 期望:
  - 步骤 1 两次输出逐字节相同
  - 步骤 2 两份 `Funding` 序列逐字节相同
- 实现:`crates/venue-bitget/tests/snapshot.rs::bgt_053_snapshot_isolation`

### BGT-060 login 帧按 V2 口径签名,时间戳为秒

- 规则:[下单:UTA V3 WS](README.md#下单uta-v3-ws)
- 级别:unit
- 里程碑:M3
- 前置:`KEY`;`now_ms = 1538054050000`;两个产品线
- 步骤:
  1. `login(Role::Order, &cfg, &KEY, 1538054050000, &mut out)`
- 期望:
  - 返回 `Ok(LoginStep::Frame)`
  - `out` = `{"op":"login","args":[{"apiKey":"k","passphrase":"p","timestamp":"1538054050","sign":"QW9NDIxQTfljkfSeZydUIsfx+5D1GgkIbDzvrCplpp4="}]}`(签名 = Base64(HMAC-SHA256("secret", "1538054050GET/user/verify")),现有测试向量)
- 实现:`crates/venue-bitget/tests/session.rs::bgt_060_login_frame`
- 备注:依赖待核实:开发备忘第 10 条(V3 的 `timestamp` 是否改为 ms;核实为 ms 时预签串与期望同一提交改写)

### BGT-061 login 的秒时间戳向下取整,Md 角色不登录

- 规则:[下单:UTA V3 WS](README.md#下单uta-v3-ws)
- 级别:table
- 里程碑:M3
- 前置:`KEY`
- 实现:`crates/venue-bitget/tests/session.rs::bgt_061_login_ts_and_roles`

| 行 | 调用 | 期望 |
|---|---|---|
| BGT-061.1 | `login(Order, …, now_ms = 1790000000999)` | 帧内 `"timestamp":"1790000000"` |
| BGT-061.2 | `login(Order, …, now_ms = 1790000001000)` | `"timestamp":"1790000001"` |
| BGT-061.3 | `login(Md, …)` | `Ok(LoginStep::NotNeeded)`,`out` 为空 |

### BGT-062 login_result 识别成功、拒绝与非登录帧

- 规则:[下单:UTA V3 WS](README.md#下单uta-v3-ws)、[用户流事件表](README.md#用户流事件表)
- 级别:table
- 里程碑:M3
- 前置:无参数
- 实现:`crates/venue-bitget/tests/session.rs::bgt_062_login_result`

| 行 | payload | 期望 |
|---|---|---|
| BGT-062.1 | `{"event":"login","code":0}` | `LoginResult::Ok` |
| BGT-062.2 | `{"event":"login","code":"0"}` | `LoginResult::Ok` |
| BGT-062.3 | `{"event":"error","code":30005,"msg":"<合成>"}` | `LoginResult::Rejected(30005)` |
| BGT-062.4 | `{"event":"error","code":"30012","msg":"<合成>"}` | `LoginResult::Rejected(30012)` |
| BGT-062.5 | `{"event":"login","code":30006}` | `LoginResult::Rejected(30006)` |
| BGT-062.6 | `B1v2` | `LoginResult::NotMine` |
| BGT-062.7 | `pong` | `LoginResult::NotMine` |
| BGT-062.8 | `{"event":"login"`(截断) | `LoginResult::NotMine` |
| BGT-062.9 | `{"event":"login","code":"0","msg":"","connId":"x"}` | `LoginResult::Ok` |

### BGT-063 登录与签名后不留存密钥材料

- 规则:[契约](../hft-venue-proto/README.md#实现方必须遵守的契约)
- 级别:unit
- 里程碑:M3
- 前置:`KEY`,其中 `secret` 换成 32 字节的随机标记串 S;`RD-BGT`
- 步骤:
  1. `login`、`build_templates`、`acct_request`(永续 Config 步 0)、BGT-072 的 Query 请求各调一次
  2. 逐字节扫描 `Templates`、`GwCtx`、`FeedCtx`、`AcctCtx`、`AcctScratch` 的内存表示,以及 `build_templates` 输出的全部片段
- 期望:
  - 步骤 2 中找不到 S、`KEY.passphrase`,也找不到任何一次签名的结果
  - 步骤 1 产生的每个 `RestReqBuf.redacted = true`
- 实现:`crates/venue-bitget/tests/session.rs::bgt_063_no_secret_retained`

### BGT-064 永续限价单编码为 place-order 帧

- 规则:[下单:UTA V3 WS](README.md#下单uta-v3-ws)
- 级别:unit
- 里程碑:M3
- 前置:`RD-BGT`;`WC`;`build_templates(cfg, rd, 2, &KEY)`;`c = GwCmd{op: Place, inst: 1801, slot: 42, gen: 7, side: Buy, tif: Gtc, px: 6500010, qty: 400000, oflags: 0, account: 2}`;`tgt = TargetRef{id: CID(3,42,7), exch_order_id: 0}`
- 步骤:
  1. `encode(rd, &t, &c, &tgt, &WC, &mut body)`(`body` 容量 2048)
- 期望:
  - 返回 `Ok(n)`,`n ≤ 2048`;`body[..n]` 是一个 JSON 对象,请求 id 为 `RID(p,42,7)`,操作为 `place-order`
  - 帧逐字节为 `{"op":"trade","id":"p0000002a00000007","category":"usdt-futures","topic":"place-order","args":[{"symbol":"BTCUSDT","side":"buy","orderType":"limit","price":"65000.1","qty":"0.004","timeInForce":"gtc","clientOid":"100030a020002a000000078d"}]}`(官方 Place-Order-Channel;`reduceOnly` 缺省即否,不发;UTA 没有 `pxAmendType` 参数)
  - 帧中不出现 `apiKey`、`sign`、`passphrase` 字段
- 实现:`crates/venue-bitget/tests/encode.rs::bgt_064_place_frame`

### BGT-065 tif、方向与 reduceOnly 的编码变体

- 规则:[下单:UTA V3 WS](README.md#下单uta-v3-ws)、[VenueCaps 默认值](README.md#venuecaps-默认值)、[契约](../hft-venue-proto/README.md#实现方必须遵守的契约)
- 级别:table
- 里程碑:M3
- 前置:同 BGT-064,每行只改所列字段
- 实现:`crates/venue-bitget/tests/encode.rs::bgt_065_place_variants`
- 备注:依赖待核实:开发备忘第 2 条

| 行 | 改动 | 期望 |
|---|---|---|
| BGT-065.1 | `tif = Ioc`,`side = Sell` | `Ok`;方向 = 卖,tif = IOC 写法 |
| BGT-065.2 | `tif = PostOnly` | `Ok`;post-only 写法 |
| BGT-065.3 | `tif = Fok` | `Ok`;`timeInForce = "fok"`(官方 UTA 支持) |
| BGT-065.4 | `oflags = REDUCE_ONLY` | `Ok`;`"reduceOnly":"YES"` |
| BGT-065.5 | `oflags = RISK_REDUCING`(无 `REDUCE_ONLY`) | `Ok`;没有 `reduceOnly` 键(venue 不推断) |
| BGT-065.6 | `oflags = SMOKE` | `Ok`;帧与 BGT-064 逐字节相同 |
| BGT-065.7 | `inst = 1901`(现货 inst) | `Err(EncodeErr::UnknownInst)` |
| BGT-065.8 | `inst = 9999`(refdata 中没有) | `Err(EncodeErr::UnknownInst)` |

### BGT-066 现货限价单编码,带 REDUCE_ONLY 不支持

- 规则:[下单:UTA V3 WS](README.md#下单uta-v3-ws)、[契约](../hft-venue-proto/README.md#实现方必须遵守的契约)
- 级别:table
- 里程碑:M3
- 前置:`BitgetSpot`;`RD-BGT`;`c = GwCmd{op: Place, inst: 1902, slot: 42, gen: 7, side: Sell, tif: Ioc, px: 12345, qty: 100000000, oflags: 0}`;`tgt.id = CID(3,42,7)`(现货)
- 实现:`crates/venue-bitget/tests/encode.rs::bgt_066_spot_place`

| 行 | 改动 | 期望 |
|---|---|---|
| BGT-066.1 | 无 | `Ok`;`symbol = "BGBUSDT"`、卖、价格 `"1.2345"`、数量 `"1"`、IOC、`clientOid = "100030b020002a000000079d"`,`category = "spot"`;帧中没有 `reduceOnly` 字段 |
| BGT-066.2 | `oflags = REDUCE_ONLY` | `Err(EncodeErr::Unsupported(GwOp::Place))`;不写帧、不丢掉标志后照发 |
| BGT-066.3 | `oflags = REDUCE_ONLY`,`op = Amend` | `Err(EncodeErr::Unsupported(GwOp::Amend))` |
| BGT-066.4 | `tif = Fok` | `Ok`;`timeInForce = "fok"` |

### BGT-067 价量格式化为最短十进制

- 规则:[下单:UTA V3 WS](README.md#下单uta-v3-ws)
- 级别:table
- 里程碑:M3
- 前置:`RD-BGT`;经 `encode` 读出帧中的价格、数量文本
- 实现:`crates/venue-bitget/tests/encode.rs::bgt_067_shortest_decimal`

| 行 | inst、raw 值 | 期望文本 |
|---|---|---|
| BGT-067.1 | 1801 价 6500010 | `"65000.1"` |
| BGT-067.2 | 1801 价 6500000 | `"65000"` |
| BGT-067.3 | 1801 价 1 | `"0.01"` |
| BGT-067.4 | 1801 量 400000 | `"0.004"` |
| BGT-067.5 | 1801 量 100000000 | `"1"` |
| BGT-067.6 | 1801 量 1 | `"0.00000001"` |
| BGT-067.7 | 1902 价 12345 | `"1.2345"` |
| BGT-067.8 | 1902 价 10000 | `"1"` |
| BGT-067.9 | 1801 价 `i64::MAX` | `"92233720368547758.07"`;帧长 ≤ 2048 |

### BGT-068 (撤销)每个下单与改单帧都显式带 pxAmendType

- 规则:[下单:UTA V3 WS](README.md#下单uta-v3-ws)
- 级别:prop
- 里程碑:M3
- 撤销:官方 UTA 下单参数没有 `pxAmendType`(超出价格带的单以 `25205`/`25206` 拒绝,不夹到边界);编号保留不复用
- 性质:任意合法 `Place`/`Amend` 命令编码成功时,帧经 `sonic-rs` 解析后订单参数恰有一个 `pxAmendType` 键,取值为"超出价格带拒单";删掉该键的帧视为本性质失败
- 生成:两个产品线;inst 取 RD-BGT 全部;`side`、`tif ∈ {Gtc, Ioc, PostOnly}`;`px`、`qty` 为 tick/lot 的 1..10^9 倍;`slot < 2^20`;`gen` 任意 u32
- 参考:断言函数 `has_px_amend_type(frame)`
- 例数:PR 2 000
- 实现:`crates/venue-bitget/tests/encode.rs::bgt_068_prop_px_amend_type`
- 备注:依赖待核实:开发备忘第 2 条(取值写法)

### BGT-069 撤单帧按目标订单的 clientOid

- 规则:[下单:UTA V3 WS](README.md#下单uta-v3-ws)
- 级别:unit
- 里程碑:M3
- 前置:`RD-BGT`;`c = GwCmd{op: Cancel, inst: 1801, slot: 42, gen: 7, target_slot: 42}`;`tgt = TargetRef{id: CID(2,42,7), exch_order_id: 0}`(原 epoch 2 的订单)
- 步骤:
  1. `encode(rd, &t, &c, &tgt, &WC, &mut body)`
- 期望:
  - `Ok`;请求 id `RID(c,42,7)`;操作 `cancel-order`;`args` 只有 `clientOid = "100020a020002a000000079d"`(按镜像里的原 epoch,不按当前 epoch)
  - `exch_order_id = 0` 不导致 `UnknownTarget`
- 实现:`crates/venue-bitget/tests/encode.rs::bgt_069_cancel_frame`

### BGT-070 改单帧为 modify-order

- 规则:[下单:UTA V3 WS](README.md#下单uta-v3-ws)
- 级别:unit
- 里程碑:M3
- 前置:`RD-BGT`;`c = GwCmd{op: Amend, inst: 1801, slot: 42, gen: 7, side: Buy, tif: Gtc, px: 6500020, qty: 300000}`;`tgt.id = CID(3,42,7)`
- 步骤:
  1. `encode`
- 期望:
  - `Ok`;请求 id `RID(m,42,7)`;操作 `modify-order`;`clientOid = "100030a020002a000000078d"`;`symbol = "BTCUSDT"`;新价格 `"65000.2"`、新数量 `"0.003"`;`autoCancel = "no"`(改单失败不撤原单)
- 实现:`crates/venue-bitget/tests/encode.rs::bgt_070_amend_frame`
- 备注:依赖待核实:开发备忘第 2 条

### BGT-071 CancelAll 在按品种全撤核实前返回 Unsupported,由运行时展开为逐单 Cancel

- 规则:[下单:UTA V3 WS](README.md#下单uta-v3-ws)
- 级别:table
- 里程碑:M3
- 前置:`RD-BGT`;`c = GwCmd{op: CancelAll, account: 2, inst: 1801, slot: 0, gen: 0}`
- 实现:`crates/venue-bitget/tests/encode.rs::bgt_071_cancel_all_per_order`

| 行 | `tgt` | 期望 |
|---|---|---|
| BGT-071.1 | `encode(c)`,`tgt = TargetRef{id: CID(3,42,7)}` | `Err(EncodeErr::Unsupported(GwOp::CancelAll))`;`body` 未被写入 |
| BGT-071.2 | 展开后的 `GwCmd{op: Cancel, target_slot: 43}`,`tgt = TargetRef{id: CID(3,43,1)}` | `Ok`;`cancel-order`,`clientOid = "100030a020002b000000018a"`,请求 id `RID(c,43,1)` |
| BGT-071.3 | 展开后的 `GwCmd{op: Cancel, target_slot: 42}`,`tgt = TargetRef{id: CID(2,42,7)}` | 与 BGT-069 的帧逐字节相同 |

### BGT-072 Query 生成签名的 REST 订单详情请求

- 规则:[下单:UTA V3 WS](README.md#下单uta-v3-ws)、[实现说明](README.md#实现说明)
- 级别:unit
- 里程碑:M3
- 前置:`RD-BGT`;`KEY`;`WC`;`c = GwCmd{op: Query, inst: 1801, slot: 42, gen: 7, target_slot: 42}`;`tgt.id = CID(3,42,7)`
- 步骤:
  1. `order_channel(Query)`;`encode_rest(rd, &t, &c, &tgt, &WC, &mut out)`;对同一 `c` 调 `encode`
- 期望:
  - `order_channel` 为 `Rest`;`encode` 返回 `Err(EncodeErr::Unsupported(GwOp::Query))`
  - `RestReqBuf{endpoint: 0, method: GET, redacted: true}`;`path_query` = `/api/v3/trade/order-info?clientOid=100030a020002a000000078d`(官方参数只有 `orderId`/`clientOid`)
  - 头恰含 `ACCESS-KEY: k`、`ACCESS-TIMESTAMP: 1790000000000`、`ACCESS-PASSPHRASE: p`、`ACCESS-SIGN` = Base64(HMAC-SHA256("secret", "1790000000000GET" + `path_query`))、`locale: en-US`;`VenueCfg.env = Testnet` 时另加 `paptrading: 1`
- 实现:`crates/venue-bitget/tests/encode.rs::bgt_072_query_rest`
- 备注:依赖待核实:开发备忘第 5 条(订单详情路径)

### BGT-073 输出缓冲不足返回 FieldOverflow

- 规则:[契约](../hft-venue-proto/README.md#实现方必须遵守的契约)
- 级别:table
- 里程碑:M3
- 前置:BGT-064 的命令;`n0` = BGT-064 的帧长
- 实现:`crates/venue-bitget/tests/encode.rs::bgt_073_buffer_too_small`

| 行 | `body.len()` | 期望 |
|---|---|---|
| BGT-073.1 | `n0` | `Ok(n0)` |
| BGT-073.2 | `n0 − 1` | `Err(EncodeErr::FieldOverflow)`,不 panic |
| BGT-073.3 | 0 | `Err(EncodeErr::FieldOverflow)` |

### BGT-074 不支持的默认方法

- 规则:[下单:UTA V3 WS](README.md#下单uta-v3-ws)、[契约](../hft-venue-proto/README.md#实现方必须遵守的契约)
- 级别:table
- 里程碑:M3
- 前置:`RD-BGT`;两条 BGT-064 形态的 Place 命令
- 实现:`crates/venue-bitget/tests/encode.rs::bgt_074_defaults`

| 行 | 调用 | 期望 |
|---|---|---|
| BGT-074.1 | `encode_batch(rd, &t, &[c1, c2], …)` | `Err(EncodeErr::Unsupported(GwOp::Place))` |
| BGT-074.2 | `needs_per_order_signing()` | `false` |
| BGT-074.3 | `encode(…)` 以 `op = SessionKeepalive` 调用 | `Err(EncodeErr::Unsupported(GwOp::SessionKeepalive))` |

### BGT-075 encode 输出可解析且字段与命令一致

- 规则:[下单:UTA V3 WS](README.md#下单uta-v3-ws)
- 级别:prop
- 里程碑:M3
- 性质:对任意合法 `Place`/`Amend`/`Cancel` 命令,`encode` 成功时帧可被 `sonic-rs` 解析;解析出的请求 id 经 `parse_req_id` 得回 `(op, slot, gen)`;`clientOid` 经 `parse_cid` 得回 `tgt.id`;价格、数量文本经 `parse_exact` 按 inst 指数得回 `c.px`、`c.qty`;帧长 ≤ 2048
- 生成:同 BGT-068,另加 `Cancel`;`epoch ∈ [0, 2^15)`
- 参考:输入 `GwCmd` 与 `TargetRef`
- 例数:PR 2 000
- 实现:`crates/venue-bitget/tests/encode.rs::bgt_075_prop_encode_roundtrip`

### BGT-076 请求 id 的渲染与解析

- 规则:[使用本组件](README.md#使用本组件)
- 级别:table
- 里程碑:M3
- 前置:无参数
- 实现:`crates/venue-bitget/tests/encode.rs::bgt_076_request_id`

| 行 | 输入 | 期望 |
|---|---|---|
| BGT-076.1 | 渲染 `(Place, 42, 7)` | `"p0000002a00000007"`(17 字节) |
| BGT-076.2 | 解析 `"p0000002a00000007"` | `Some(ReqRef{slot: 42, gen: 7, op: Place})` |
| BGT-076.3 | 解析 `"c0000002a00000007"` / `"m0000002a00000007"` / `"x0000002a00000007"` | 依次 `Some(ReqRef{op: Cancel})`、`Some(ReqRef{op: Amend})`、`None` |
| BGT-076.4 | 解析 `"pffffffffffffffff"` | `Some(ReqRef{slot: 4294967295, gen: 4294967295, op: Place})` |
| BGT-076.5 | 解析 `"q0000002a00000007"`(Query 不走 WS) | `None` |
| BGT-076.6 | 解析 `"P0000002a00000007"` | `None` |
| BGT-076.7 | 解析 `"p0000002A00000007"`(大写十六进制) | `None` |
| BGT-076.8 | 解析 16 字节 `"p0000002a0000000"` | `None` |
| BGT-076.9 | 解析 18 字节 `"p0000002a000000070"` | `None` |
| BGT-076.10 | 解析 `""` | `None` |

### BGT-077 REST 签名按 ts + METHOD + path + query + body

- 规则:[实现说明](README.md#实现说明)
- 级别:table
- 里程碑:M3
- 前置:`secret = "secret"`;`ts = "1790000000000"`(ms)
- 实现:`crates/venue-bitget/tests/encode.rs::bgt_077_rest_sign`

| 行 | METHOD、path+query、body | 期望 `ACCESS-SIGN` |
|---|---|---|
| BGT-077.1 | `GET`,`/api/v3/account/settings`,空 | `DBRPdhO1WymCRYZqSPp0Y7bUgchoZbxoC3NGixZ6GNA=` |
| BGT-077.2 | `GET`,`/api/v3/account/settings?symbol=BTCUSDT`,空 | `GgDIq9kSphQ6PpVFKQ+XGUeLg+hqA20GYJCbDrYcL98=` |
| BGT-077.3 | `POST`,`/api/v3/trade/place-order`,`{"symbol":"BTCUSDT"}` | `NCZu4d/3HNx//PS1ST6exwD6MHP14nsbYBZU5eaTmhE=` |
| BGT-077.4 | 预签串的方法写成小写 `get`(反例) | 结果 `xw04pj6Z+llprpM+eKuzvlv1se8i6bSALRbUv7jQaGE=` ≠ BGT-077.1;实现必须写大写方法 |

### BGT-078 REST 请求带齐四个 ACCESS 头并标 REDACTED

- 规则:[实现说明](README.md#实现说明)
- 级别:unit
- 里程碑:M3
- 前置:`KEY`;`WC`;`RD-BGT`;永续 Config 步 0 的 `AcctQuery`
- 步骤:
  1. `acct_request(rd, &t, 2, q, &WC, &mut out)`
- 期望:
  - `Ok`;`out.headers` 恰含 `ACCESS-KEY: k`、`ACCESS-SIGN: <Base64 44 字符>`、`ACCESS-TIMESTAMP: 1790000000000`、`ACCESS-PASSPHRASE: p` 四个鉴权头;`out.redacted = true`
  - `ACCESS-SIGN` 等于用 BGT-077 的函数对 `"1790000000000GET" + out.path_query` 计算的值
- 实现:`crates/venue-bitget/tests/sign.rs::bgt_078_rest_headers`

### BGT-080 clientOid 外形与解析

- 规则:[使用本组件](README.md#使用本组件)、[外部成交识别](README.md#外部成交识别)
- 级别:table
- 里程碑:M3
- 前置:永续
- 实现:`crates/venue-bitget/tests/encode.rs::bgt_080_client_oid`

| 行 | 输入 | 期望 |
|---|---|---|
| BGT-080.1 | `render_cid(CID(3,42,7))` | 写入 24 字节 `100030a020002a000000078d`,返回 24 |
| BGT-080.2 | `parse_cid(b"100030a020002a000000078d")` | `Some(ClientOrderId{epoch: 3, venue: 10, account: 2, slot: 42, gen: 7})` |
| BGT-080.3 | `parse_cid` 校验位错 `100030a020002a000000078c` | `None` |
| BGT-080.4 | `parse_cid` 大写 `100030A020002A000000078D` | `None` |
| BGT-080.5 | `parse_cid` 23 字节(去掉末字符) | `None` |
| BGT-080.6 | `parse_cid` 25 字节(末尾多 `0`) | `None` |
| BGT-080.7 | `parse_cid(b"")` | `None` |
| BGT-080.8 | `parse_cid(b"web_100030a020002a000000078d")` | `None`(只接受本所外形) |
| BGT-080.9 | `parse_cid` 保留位为 1 的文本(`1800301…` 形态,按 hft-types 规则计算校验位) | `None` |

### BGT-081 任意 ClientOrderId 往返且外形为 24 位小写十六进制

- 规则:[使用本组件](README.md#使用本组件)
- 级别:prop
- 里程碑:M3
- 性质:`parse_cid(render_cid(id)) == Some(id)`;输出恰 24 字节,全部属于 `[0-9a-f]`(≤ 40;字符集待核实,开发备忘第 2 条)
- 生成:`epoch ∈ [0, 2^15)`、`venue ∈ {10, 11}`、`account ∈ [0, 255]`、`slot ∈ [0, 2^20)`、`gen` 任意 u32
- 参考:输入 `id`
- 例数:PR 5 000
- 实现:`crates/venue-bitget/tests/encode.rs::bgt_081_prop_cid_roundtrip`

### BGT-090 下单成功响应解码为 Ack

- 规则:[实现说明](README.md#实现说明)、[契约](../hft-venue-proto/README.md#实现方必须遵守的契约)
- 级别:unit
- 里程碑:M3
- 前置:`RD-BGT`;私有连接 `RX`(`account = 2`);新 `GwCtx`;响应 = 请求 id `RID(p,42,7)`、成功码、`orderId = "1234567890123"`、`clientOid = "100030a020002a000000078d"`、`symbol = "BTCUSDT"`
- 步骤:
  1. `classify_report(Text, head)` → `WsResp`;`decode_report(rd, &mut ctx, WsResp, payload, &RX, &mut sink)`
- 期望:
  - `sink` 恰一条 `OrderEvent{venue: 10, account: 2, what: Ack, slot: 42, gen: 7, epoch: 3, inst: 1801, exch_order_id: 1234567890123, cum_filled: 0, fee: 0, reject_code: 0}`
  - `sink.exch_id(b"1234567890123", Some(&CID(3,42,7)))` 恰被调用一次
  - `ReportOutcome{req: Some(ReqRef{slot: 42, gen: 7, op: Place}), rate: None, gw_dwell_us: 0, events: 1, outcome_unknown: None, raw_code: 0, skip: No, hint: None, fee_rule_deviation: false}`
- 实现:`crates/venue-bitget/tests/reports.rs::bgt_090_place_ack`
- 备注:依赖待核实:开发备忘第 2 条(响应字段名、是否回显 `clientOid`、有无网关时间戳)

### BGT-091 错误码映射:登记码确定拒绝,其余结果未知

- 规则:[结果未知码表](README.md#结果未知码表)、[契约](../hft-venue-proto/README.md#实现方必须遵守的契约)
- 级别:table
- 里程碑:M3
- 前置:`RD-BGT`;以 BGT-090 的请求为基准,响应为 `{"event":"error","id":<RID>,"code":<码>,"msg":…}`(请求 id 随行变化);`codes::DEFINITE_REJECT` 为官方 UTA 错误码页登记的确定拒绝表
- 实现:`crates/venue-bitget/tests/reports.rs::bgt_091_code_table`

| 行 | 响应 | 期望 |
|---|---|---|
| BGT-091.1 | `codes::DEFINITE_REJECT` | 非空,内容见 README"结果未知码表" |
| BGT-091.2 | place,码 `"40001"`(未登记) | 无 `OrderEvent`;`outcome_unknown = Some(ReqRef{42, 7, Place})`;`raw_code = 40001` |
| BGT-091.3 | place,码 `"25202"`(余额不足,已登记) | 一条 `Reject{slot: 42, gen: 7, reject_code: 25202}`;不置 `outcome_unknown` |
| BGT-091.4 | place,码 `"40017"`(参数校验,已登记) | 一条 `Reject` |
| BGT-091.5 | place,码 `"40010"`(请求超时) | `outcome_unknown` 置;`raw_code = 40010` |
| BGT-091.6 | place,码为 JSON 数字 `40001` | 同 BGT-091.2 |
| BGT-091.7 | place,码 `"abc"`(非数字) | `outcome_unknown` 置;`raw_code` 在 `0x4000_0000` 段 |
| BGT-091.8 | cancel(`RID(c,42,7)`),码 `"43001"`(未登记) | 无 `CancelReject`;`outcome_unknown = Some(ReqRef{42, 7, Cancel})`;`raw_code = 43001` |
| BGT-091.9 | modify(`RID(m,42,7)`),码 `"25000"`(系统错误) | 无 `AmendReject`;`outcome_unknown = Some(ReqRef{42, 7, Amend})` |
| BGT-091.10 | Query 的 REST 响应 HTTP 503、无业务码 | `outcome_unknown = Some(ReqRef{42, 7, Query})`;`raw_code = 0x2000_01F7`(536871415) |
| BGT-091.11 | Query 的 REST 响应 HTTP 429、无业务码 | `outcome_unknown` 置;`ctl = RateLimited`;`raw_code = 0x2000_01AD`(536871341) |
| BGT-091.12 | Query 的 REST 响应 HTTP 200、`code = "00000"` | 不置 `outcome_unknown`;产出 `QueryResult`(BGT-093) |
| BGT-091.13 | place,码 `"25212"`(重复 `clientOid`) | 结果未知(前一笔可能仍挂着),不 `Reject` |
| BGT-091.14 | cancel,码 `"25200"`(已登记) | 一条 `CancelReject{detail: 0}` |
| BGT-091.15 | place,码 `"25004"`(操作过于频繁) | 一条 `Reject`;`ctl = RateLimited` |
| BGT-091.16 | place,码 `"40008"`(时间戳过期) | `ctl = ClockSkew` |
| BGT-091.17 | `event = "trade"` 但 `code = "25202"` | 与 BGT-091.3 相同 |

### BGT-092 改单与撤单的成功响应只是回执

- 规则:[实现说明](README.md#实现说明)、[下单:UTA V3 WS](README.md#下单uta-v3-ws)
- 级别:table
- 里程碑:M3
- 前置:同 BGT-090,换请求 id 与操作(官方:撤单、改单的成功响应只表示交易所收到请求,结果以订单频道为准;改单异步,失败不另行通知)
- 实现:`crates/venue-bitget/tests/reports.rs::bgt_092_amend_cancel_ack`

| 行 | 响应 | 期望 |
|---|---|---|
| BGT-092.1 | modify 成功(`RID(m,42,7)`) | 0 个事件;`req = Some(ReqRef{42, 7, Amend})`(只结束在途请求) |
| BGT-092.2 | cancel 成功(`RID(c,42,7)`) | 0 个事件;`req.op = Cancel`;不置 `outcome_unknown`。`CancelAck` 由订单频道的 `cancelled` 推送产出(BGT-100.4) |
| BGT-092.3 | 由 `CancelAll` 展开的撤单成功(`RID(c,43,1)`) | `req = Some(ReqRef{43, 1, Cancel})` |
| BGT-092.4 | place 成功响应缺 `orderId` | `Err(DecodeErr::Shape)`,0 个事件 |
| BGT-092.5 | cancel / modify 失败,码 `"25204"`(订单不存在) | 一条 `CancelReject` / `AmendReject`,`detail = GONE(1)`,`reject_code = 25204` |

### BGT-093 查单响应解码为 QueryResult,累计量与均价必填

- 规则:[下单:UTA V3 WS](README.md#下单uta-v3-ws)、[累计量与费用来源](README.md#累计量与费用来源)、[契约](../hft-venue-proto/README.md#实现方必须遵守的契约)
- 级别:table
- 里程碑:M3
- 前置:`RD-BGT`;BGT-072 请求的响应体(HTTP 200,`code = "00000"`),订单 `clientOid = CID(3,42,7)`、`orderId = "1234567890123"`;以 `decode_rest(…, &RestOrigin{what: Order, req: Some(ReqRef{42, 7, Query}), inst: 1801}, 200, body, …)` 解码;"状态"即 `OrderEvent.detail`
- 实现:`crates/venue-bitget/tests/reports.rs::bgt_093_query_result`
- 备注:依赖待核实:开发备忘第 5 条(订单详情的状态、累计量、均价字段名)

| 行 | 订单状态、累计成交、均价 | 期望 |
|---|---|---|
| BGT-093.1 | 已成交,`0.004`,`65000.1` | `OrderEvent{what: QueryResult, slot: 42, gen: 7, epoch: 3, cum_filled: 400000, avg_px: 6500010, exch_order_id: 1234567890123}`;订单状态 `Filled`(3) |
| BGT-093.2 | 挂单中,`0`,空 | `cum_filled: 0, avg_px: 0`;状态 `New`(1) |
| BGT-093.3 | 部分成交,`0.002`,`65000` | `cum_filled: 200000, avg_px: 6500000`;状态 `PartiallyFilled`(2) |
| BGT-093.4 | 已撤销,`0.001`,`65000.1` | 状态 `Cancelled`(4),`cum_filled: 100000` |
| BGT-093.5 | 已成交,`0.004`,均价缺失或 `"0"` | `Err(DecodeErr::Shape)`(`cum_filled > 0` 而无均价视为解码错误) |
| BGT-093.6 | "订单不存在"(HTTP 400,码 `"25204"`,`data: null`) | 一条 `QueryResult{slot: 42, gen: 7, cum_filled: 0}`,`detail` = NotFound(7)(槽取自 `RestOrigin`) |
| BGT-093.6 | 订单不存在的响应 | 状态 `NotFound`(7);不产出 `Reject`、不定终态(`stale_guard = None`) |

- 身份回归:`crates/venue-bitget/tests/reports.rs::bgt_093_query_identity_cannot_redirect`;品种、venue/account/槽/代不符或负累计量拒绝整条响应。

### BGT-100 订单频道状态映射为 OrderEvent

- 规则:[实现说明](README.md#实现说明)、[用户流事件表](README.md#用户流事件表)
- 级别:table
- 里程碑:M3
- 前置:`RD-BGT`;私有 `RX`(`account = 2`);新 `GwCtx`;`ORD(...)` = UTA `order` 推送单条(`arg = {instType: "UTA", topic: "order"}`):`category "usdt-futures"`、`symbol "BTCUSDT"`、`orderId "1234567890123"`、`clientOid = CID(3,42,7)`、`timeInForce "gtc"`、`tradeSide "buy_single"`,`orderStatus`、`cumExecQty`、`cumExecValue`、`avgPrice`、`feeDetail` 按行(V2 字段名 `status`/`force`/`accBaseVolume`/`priceAvg` 同样接受);每行用新的 `GwCtx`
- 实现:`crates/venue-bitget/tests/reports.rs::bgt_100_order_status_map`
- 备注:依赖待核实:开发备忘第 1、3 条(UTA 频道名、状态取值与字段名)

| 行 | 推送内容 | 期望 |
|---|---|---|
| BGT-100.1 | `status "live"`,无本笔成交 | 一条 `OrderEvent{what: Ack, slot: 42, gen: 7, epoch: 3, exch_order_id: 1234567890123}` |
| BGT-100.2 | `partially_filled`,累计 `0.001` | 一条 `PartialFill`,`qty: 100000`,`cum_filled: 100000` |
| BGT-100.3 | `filled`,累计 `0.004`(新 `GwCtx`) | 一条 `Fill`,`qty: 400000`,`cum_filled: 400000` |
| BGT-100.4 | `cancelled`,`timeInForce "gtc"`,累计 `0` | 一条 `CancelAck` |
| BGT-100.5 | `cancelled`,`timeInForce "ioc"`,累计 `0.001` | 一条 `Expired`,`cum_filled: 100000` |
| BGT-100.6 | `status "foo"` | `Err(DecodeErr::Shape)`,0 个事件 |
| BGT-100.7 | `filled` 但缺累计成交量(`cumExecQty`) | `Err(DecodeErr::Shape)` |
| BGT-100.8 | `cancelled`,`cancelReason "post_only_fill_cancel"` | 一条 `Expired` |

### BGT-101 部分成交的字段逐一映射

- 回归补充:迟到累计量不得回退累计金额/费用基线;多位小数均价按指数舍入;非法负量/金额不得改变上下文。

- 规则:[累计量与费用来源](README.md#累计量与费用来源)、[实现说明](README.md#实现说明)、[契约](../hft-venue-proto/README.md#实现方必须遵守的契约)
- 级别:unit
- 里程碑:M3
- 前置:`RD-BGT`;私有 `RX`;同一 `GwCtx` 依次解码 `ORD(partially_filled, cumExecQty "0.002", cumExecValue "130", avgPrice "65000", feeDetail USDT "0.026")` 与 `ORD(partially_filled, "0.003", "195.0001", "65000.03", "0.039", updatedTime "1790000000500")`
- 步骤:
  1. `decode_report(rd, &mut ctx, Orders, payload, &RX, &mut sink)` 两次
- 期望:
  - 第 2 条为 `OrderEvent{venue: 10, account: 2, what: PartialFill, slot: 42, gen: 7, epoch: 3, inst: 1801, px: 6500010(= Δ成交额 65.0001 / Δ量 0.001), qty: 100000, cum_filled: 300000, avg_px: 6500003, fee: 1300000(累计费用差分), fee_asset: 1, exch_order_id: 1234567890123, ex_ts_ns: 1790000000500000000, hw_rx_ns: 1790000000126000000, rx_tsc: 1000, reject_code: 0, trade_id: 0}`
  - `ReportOutcome{events: 1, skip: No, fee_rule_deviation: false}`
  - V2 形态(`baseVolume "0.001"`、`fillPrice "65000.1"`、`accBaseVolume "0.003"`、`priceAvg "65000.05"`、`fillFee "-0.039"`、`tradeId "9003"`、`fillTime`)在新 `GwCtx` 上:本笔字段优先,`qty: 100000`、`px: 6500010`、`fee: 3900000`(取绝对值)、`trade_id: 9003`
- 实现:`crates/venue-bitget/tests/reports.rs::bgt_101_partial_fill_fields`

### BGT-102 同一成交推送两次只产出一次

- 规则:[累计量与费用来源](README.md#累计量与费用来源)、[契约](../hft-venue-proto/README.md#实现方必须遵守的契约)
- 级别:unit
- 里程碑:M3
- 前置:BGT-101 的输入;同一 `GwCtx`
- 步骤:
  1. 解码两次
- 期望:
  - 第 1 次一条 `PartialFill`;第 2 次 `ReportOutcome.events = 0`,`sink` 无新事件,返回 `Ok`
- 实现:`crates/venue-bitget/tests/reports.rs::bgt_102_fill_dedup`

### BGT-103 两个 inst 的成交 id 相同时两笔都产出

- 规则:[累计量与费用来源](README.md#累计量与费用来源)
- 级别:unit
- 里程碑:M3
- 前置:同一 `GwCtx`;推送 A = BTCUSDT 成交 `tradeId "9003"`(`clientOid = CID(3,42,7)`);推送 B = ETHUSDT 成交 `tradeId "9003"`(`clientOid = CID(3,43,1)`)
- 步骤:
  1. 解码 A,再解码 B
- 期望:
  - 两条成交事件,`inst` 依次为 1801、1802
- 实现:`crates/venue-bitget/tests/reports.rs::bgt_103_same_trade_id_two_insts`

### BGT-104 成交 id 缺失、为 0 或为负时按 (inst, 订单 id, 累计量) 去重

- 规则:[累计量与费用来源](README.md#累计量与费用来源)、[契约](../hft-venue-proto/README.md#实现方必须遵守的契约)
- 级别:table
- 里程碑:M3
- 前置:同一 `GwCtx` 按行序解码;订单 `orderId "1234567890123"`、`clientOid = CID(3,42,7)`
- 实现:`crates/venue-bitget/tests/reports.rs::bgt_104_dedup_without_trade_id`

| 行 | 推送 | 期望 |
|---|---|---|
| BGT-104.1 | `tradeId ""`,本笔 `0.001`,累计 `0.001` | 一条 `PartialFill`,`cum_filled: 100000` |
| BGT-104.2 | 与上一行相同 | 0 个事件 |
| BGT-104.3 | `tradeId "0"`,本笔 `0.001`,累计 `0.002` | 一条 `PartialFill`,`cum_filled: 200000` |
| BGT-104.4 | `tradeId "-5"`,本笔 `0.001`,累计 `0.002` | 0 个事件(键与上一行相同) |
| BGT-104.5 | 删除 `tradeId` 字段,累计 `0.003` | 一条成交事件,`cum_filled: 300000` |

### BGT-105 (撤销)推送不带累计量时按成交 id 去重累加合成

- 规则:[累计量与费用来源](README.md#累计量与费用来源)、[契约](../hft-venue-proto/README.md#实现方必须遵守的契约)
- 级别:unit
- 里程碑:M3
- 撤销:UTA `order` 推送必带 `cumExecQty`(官方);缺失时按 BGT-100.7 返回 `Shape`。编号保留不复用
- 前置:同一新 `GwCtx`;三条推送都不带累计量字段:t1(`tradeId "9101"`,本笔 `0.001`)、t2(`"9102"`,本笔 `0.002`)、t1 重复
- 步骤:
  1. 依次解码 t1、t2、t1
- 期望:
  - 第 1 条 `PartialFill{cum_filled: 100000}`;第 2 条 `cum_filled: 300000`;第 3 条 0 个事件
  - 合成值 300000 = 去重后两笔之和
- 实现:`crates/venue-bitget/tests/reports.rs::bgt_105_synth_cum_filled`
- 备注:依赖待核实:开发备忘第 3 条;核实为推送带累计量时本用例保留(防御路径)

### BGT-106 累计口径的费用取差分

- 规则:[累计量与费用来源](README.md#累计量与费用来源)
- 级别:unit
- 里程碑:M3
- 前置:同一 `GwCtx`;同一订单两笔成交,费用字段为订单累计:第 1 笔累计 `-0.01` USDT,第 2 笔累计 `-0.03` USDT
- 步骤:
  1. 依次解码
- 期望:
  - 两条成交事件的 `fee` 依次为 1000000、2000000,`fee_asset: 1`
- 实现:`crates/venue-bitget/tests/reports.rs::bgt_106_cumulative_fee_diff`
- 备注:依赖待核实:开发备忘第 3 条;核实为本笔口径时删除本用例并保留编号

### BGT-107 (暂缓)订单终态之后才到的费用产出 FeeOnly

- 规则:[累计量与费用来源](README.md#累计量与费用来源)、[用户流事件表](README.md#用户流事件表)、[契约](../hft-venue-proto/README.md#实现方必须遵守的契约)
- 级别:table
- 里程碑:M3
- 暂缓:费用取自 `order` 推送的订单累计 `feeDetail`(随成交差分产出);`fill` 频道不订阅,核实费用晚于订单终态到达后再启用
- 前置:同一 `GwCtx`;先解码 BGT-100.3(`Fill`,不带费用,`fee: 0`、`fee_asset: ASSET_NONE`);再解码成交频道推送(若存在)
- 实现:`crates/venue-bitget/tests/reports.rs::bgt_107_fee_only`
- 备注:依赖待核实:开发备忘第 3 条(成交频道是否存在、费用是否晚于订单终态)

| 行 | 成交频道推送 | 期望 |
|---|---|---|
| BGT-107.1 | `tradeId "9002"`,费用 `-0.117` USDT,带 `clientOid = CID(3,42,7)` | 一条 `OrderEvent{what: FeeOnly, qty: 0, fee: 11700000, fee_asset: 1, exch_order_id: 1234567890123, slot: 42, gen: 7, epoch: 3}` |
| BGT-107.2 | 同上,不带 `clientOid` | `FeeOnly`,`slot = SlotId::UNRESOLVED`;`sink.exch_id(b"1234567890123", None)` 被调用一次 |
| BGT-107.3 | 与 BGT-107.1 相同的推送再到一次 | 0 个事件 |
| BGT-107.4 | 费用为 0 | 0 个事件(不产出 `qty = 0` 的 `Fill`) |

### BGT-108 扣费币种与规则不符只置偏离标志

- 规则:[实现说明](README.md#实现说明)、[契约](../hft-venue-proto/README.md#实现方必须遵守的契约)
- 级别:table
- 里程碑:M3
- 前置:`RD-BGT`;生效 `fee_asset_rule` 为 `default_caps()` 的值(永续 `Quote`,现货 `ReceivedAssetUnlessDiscount{BGB}`)
- 实现:`crates/venue-bitget/tests/reports.rs::bgt_108_fee_rule_deviation`

| 行 | 成交 | 期望 |
|---|---|---|
| BGT-108.1 | 永续,费用币 `USDT` | `fee_rule_deviation = false` |
| BGT-108.2 | 永续,费用币 `BGB` | `fee_rule_deviation = true`;事件 `fee_asset = 6`,`fee` 为报文值(不改账) |
| BGT-108.3 | 现货买 BTC(1901),费用币 `BTC` | `false`;`fee_asset = 2` |
| BGT-108.4 | 现货买 BTC,费用币 `BGB` | `false`;`fee_asset = 6` |
| BGT-108.5 | 现货买 BTC,费用币 `USDT` | `true`;`fee_asset = 1` |
| BGT-108.6 | 现货卖 BGB(1902),费用币 `USDT` | `false`(卖出时收到的资产是 USDT) |

### BGT-109 外部成交按判定顺序识别

- 规则:[外部成交识别](README.md#外部成交识别)、[契约](../hft-venue-proto/README.md#实现方必须遵守的契约)
- 级别:table
- 里程碑:M3
- 前置:`RD-BGT`;永续成交推送(除注明外),`status "filled"`、`tradeId "9201"`,开平类型与 `clientOid` 按行;每行新 `GwCtx`
- 实现:`crates/venue-bitget/tests/reports.rs::bgt_109_external_order`
- 备注:依赖待核实:开发备忘第 3、6、14 条(开平类型字段名、单向模式取值、人工来源标记)

| 行 | 开平类型、`clientOid` | 期望 |
|---|---|---|
| BGT-109.1 | `burst_close_long`,`CID(3,42,7)` | 一条 `ExternalFill{cause: Liquidation}`;无 `OrderEvent`(标记优先于 client id) |
| BGT-109.2 | `burst_close_short`,空 | `ExternalFill{cause: Liquidation}` |
| BGT-109.3 | `dte_sys_adl_close_long`,空 | `ExternalFill{cause: Adl}` |
| BGT-109.4 | `dte_sys_adl_close_short`,`"abc"` | `ExternalFill{cause: Adl}` |
| BGT-109.5 | `delivery_close_long`,空 | `ExternalFill{cause: Delivery}` |
| BGT-109.6 | `delivery_close_short`,空 | `ExternalFill{cause: Delivery}` |
| BGT-109.7 | `foo_bar`(不在已知集合),`"abc"` | `ExternalFill{cause: Unknown}` |
| BGT-109.8 | `foo_bar`,`CID(3,42,7)` | `OrderEvent{what: Fill}`;无 `ExternalFill` |
| BGT-109.9 | `open`,`CID(3,42,7)` | `OrderEvent{what: Fill, slot: 42, gen: 7}` |
| BGT-109.10 | `open`,`"web_123"` | `ExternalFill{cause: Unknown}`(人工来源标记待核实,开发备忘第 3 条;不出 `Manual`) |
| BGT-109.11 | `open`,`CID(3,42,7)` 校验位错 | `ExternalFill{cause: Unknown}` |
| BGT-109.12 | 现货成交,`"abc"` | `ExternalFill{cause: Unknown}`(现货无强平标记) |
| BGT-109.13 | 现货成交,`burst_close_long` 字段出现,`CID(3,42,7)`(现货) | `OrderEvent{what: Fill}`(现货 `NotIdentifiable`,不按开平类型判) |
| BGT-109.14 | `burst_sell_single`(单向模式强平),`CID(3,42,7)` | `ExternalFill{cause: Liquidation}` |
| BGT-109.15 | `dte_sys_adl_buy_in_single_side_mode`,空 | `ExternalFill{cause: Adl}` |
| BGT-109.16 | `delivery_buy_single`,空 | `ExternalFill{cause: Delivery}` |
| BGT-109.17 | `reduce_sell_single`(单向模式部分强平),`CID(3,42,7)` | `ExternalFill{cause: Liquidation}` |

### BGT-110 ExternalFill 的字段

- 规则:[外部成交识别](README.md#外部成交识别)、[契约](../hft-venue-proto/README.md#实现方必须遵守的契约)
- 级别:table
- 里程碑:M3
- 前置:`RD-BGT`;私有 `RX`;永续强平成交:开平类型 `burst_close_long`、`side "sell"`、本笔 `0.004`、`fillPrice "52000.5"`、`fillFee "-0.6"`、`fillFeeCoin "USDT"`、`orderId "777000"`、`fillTime "1790000000600"`、累计 `0.004`
- 实现:`crates/venue-bitget/tests/reports.rs::bgt_110_external_fields`

| 行 | `tradeId` | 期望 |
|---|---|---|
| BGT-110.1 | `"9301"` | `ExternalFill{venue: 10, account: 2, side: Sell, cause: Liquidation, inst: 1801, qty: 400000, px: 5200050, fee: 60000000, fee_asset: 1, exch_order_id: 777000, trade_id: 9301, ex_ts_ns: 1790000000600000000, rx_tsc: 1000, fill_seq: 0}` |
| BGT-110.2 | `"0"` | 同上但 `trade_id: 0`,`fill_seq: 400000` |
| BGT-110.3 | 缺失 | `trade_id: 0`,`fill_seq: 400000` |

### BGT-111 clientOid 缺失的成交交运行时按 IdTable 解析

- 规则:[外部成交识别](README.md#外部成交识别)、[契约](../hft-venue-proto/README.md#实现方必须遵守的契约)
- 级别:unit
- 里程碑:M3
- 前置:`RD-BGT`;永续成交推送,开平类型 `open`,不带 `clientOid` 字段,`orderId "1234567890123"`,`tradeId "9401"`;`VecSink.exch_id` 对该 key 返回 `ExchIdHint::Unknown`
- 步骤:
  1. `decode_report`
- 期望:
  - 一条 `OrderEvent{what: Fill, slot: SlotId::UNRESOLVED, epoch: 0, exch_order_id: 1234567890123}`;无 `ExternalFill`(暂存超时转 `ExternalFill{Unknown}` 归运行时)
  - `sink.exch_id(b"1234567890123", None)` 被调用一次
- 实现:`crates/venue-bitget/tests/reports.rs::bgt_111_missing_client_oid`
- 备注:依赖待核实:开发备忘第 3 条("必带 `clientOid`"核实成立后改按契约 18 第 3 步,本用例同一提交改写)

### BGT-112 强平成交重复送达只产出一次 ExternalFill

- 规则:[外部成交识别](README.md#外部成交识别)、[契约](../hft-venue-proto/README.md#实现方必须遵守的契约)
- 级别:table
- 里程碑:M3
- 前置:同一 `GwCtx` 按行序;BGT-110 的推送
- 实现:`crates/venue-bitget/tests/reports.rs::bgt_112_external_dedup`

| 行 | 推送 | 期望 |
|---|---|---|
| BGT-112.1 | BGT-110.2(`tradeId "0"`,累计 `0.004`) | 一条 `ExternalFill` |
| BGT-112.2 | 同一推送再到一次 | 0 个事件 |
| BGT-112.3 | 同一订单下一笔:`tradeId "0"`,本笔 `0.002`,累计 `0.006` | 一条 `ExternalFill{qty: 200000, fill_seq: 600000}` |

### BGT-113 持仓频道产出只校验的 PositionReport

- 规则:[用户流事件表](README.md#用户流事件表)、[保证金数据换算](README.md#保证金数据换算)
- 级别:table
- 里程碑:M3
- 前置:`RD-BGT`;私有 `RX`;永续持仓推送,单向模式
- 实现:`crates/venue-bitget/tests/reports.rs::bgt_113_position_report`
- 备注:依赖待核实:开发备忘第 1、5 条(频道与字段名)

| 行 | 推送内容 | 期望 |
|---|---|---|
| BGT-113.1 | BTCUSDT 多 `0.01`,开仓均价 `64000`,未实现 `10.5`,强平价 `52000.5` | 一条 `PositionReport{venue: 10, account: 2, inst: 1801, qty: 1000000, entry_px: 6400000, upnl: 10500000, liq_px: 5200050}` |
| BGT-113.2 | BTCUSDT 空 `0.01` | `qty: -1000000` |
| BGT-113.3 | 强平价 `""` 或 `"0"` | `liq_px: 0` |
| BGT-113.4 | `DOGEUSDT`(不在 refdata) | 0 个事件,返回 `Ok` |
| BGT-113.5 | 两个持仓(BTCUSDT、ETHUSDT) | 两条 `PositionReport`;0 条 `MarginReport` |

### BGT-114 账户频道产出 BalanceReport,轮询模式下不产出 MarginReport

- 规则:[用户流事件表](README.md#用户流事件表)、[账户形态(官方,本地缓存的 UTA 概览页)](README.md#账户形态官方本地缓存的-uta-概览页)
- 级别:table
- 里程碑:M3
- 前置:`RD-BGT`;生效 `margin_data = Poll{interval_ms: 5000}`
- 实现:`crates/venue-bitget/tests/reports.rs::bgt_114_account_push`
- 备注:依赖待核实:开发备忘第 1、5 条

| 行 | 推送内容 | 期望 |
|---|---|---|
| BGT-114.1 | 永续账户:`USDT` 可用 `8000.5`、冻结 `100` | 一条 `BalanceReport{venue: 10, account: 2, asset: 1, free: 800050000000, locked: 10000000000}`;0 条 `MarginReport` |
| BGT-114.2 | 同上并带保证金率字段 | 同上;仍 0 条 `MarginReport` |
| BGT-114.3 | 现货账户:`BTC` 可用 `0.5`、`USDT` 可用 `1000` | 两条 `BalanceReport`,`venue: 11`,`asset` 依次 2、1 |
| BGT-114.4 | 币种 `XYZ`(不在 refdata 资产表) | 该币种 0 条事件,其余币种照常 |

### BGT-115 控制回执、心跳回应与未知频道不是解码失败

- 规则:[用户流事件表](README.md#用户流事件表)、[契约](../hft-venue-proto/README.md#实现方必须遵守的契约)
- 级别:table
- 里程碑:M3
- 前置:`RD-BGT`;生效 `ignorable_user_events` 按行
- 实现:`crates/venue-bitget/tests/reports.rs::bgt_115_skip_events`

| 行 | 输入 | 期望 `decode_report` |
|---|---|---|
| BGT-115.1 | `Control`,`{"event":"login","code":0}` | `Ok`,`events: 0`,`skip: No` |
| BGT-115.2 | `Control`,`{"event":"subscribe","arg":{…}}` | `Ok`,`events: 0`,`skip: No` |
| BGT-115.3 | `Control`,`{"event":"error","code":30001,…}` | `Ok`,`events: 0`,`skip: No` |
| BGT-115.4 | `Pong`,`pong` | `Ok`,`events: 0`,`skip: No` |
| BGT-115.5 | `Unknown`,频道 `strategy-x`,默认名单(空) | `Ok`,`events: 0`,`skip: Unknown` |
| BGT-115.6 | 同上,refdata 把 `strategy-x` 追加进 `ignorable_user_events` | `Ok`,`events: 0`,`skip: Listed` |
| BGT-115.7 | `Push`,V2 形态频道 `strategy-x` | `Ok`,`events: 0`,`skip: Unknown` |
| BGT-115.8 | `default_caps().ignorable_user_events`(两个产品线) | 空 |

### BGT-116 类别已知而内容解析失败才返回 DecodeErr

- 规则:[契约](../hft-venue-proto/README.md#实现方必须遵守的契约)
- 级别:table
- 里程碑:M3
- 前置:`RD-BGT`;以 BGT-101 的推送为基准
- 实现:`crates/venue-bitget/tests/reports.rs::bgt_116_decode_errors`

| 行 | 改动 | 期望 |
|---|---|---|
| BGT-116.1 | 累计量 `"abc"` | `Err(DecodeErr::Number)`,0 个事件 |
| BGT-116.2 | 在 `data` 中途截断 | `Err(DecodeErr::Truncated)` |
| BGT-116.3 | `data` 不是数组 | `Err(DecodeErr::Shape)` |
| BGT-116.4 | 本笔量 `"1e30"` | `Err(DecodeErr::Overflow)` |
| BGT-116.5 | `instId "DOGEUSDT"`(不在 refdata) | `Ok`,0 个事件 |
| BGT-116.6 | 有累计成交而均价缺失 | `Err(DecodeErr::Shape)`(契约 9) |

### BGT-117 任何推送都不产出 FundingSettle

- 规则:[累计量与费用来源](README.md#累计量与费用来源)、[VenueCaps 默认值](README.md#venuecaps-默认值)
- 级别:unit
- 里程碑:M3
- 前置:`RD-BGT`;生效 `funding_settle = RateOnly`;输入 = BGT-300–BGT-327 全部私有推送金样,外加一条合成的账户推送,其余额变化原因写为资金费
- 步骤:
  1. 逐条 `decode_report`
- 期望:
  - 全程 `sink.funding_settle` 调用 0 次
- 实现:`crates/venue-bitget/tests/reports.rs::bgt_117_no_funding_settle`
- 备注:依赖待核实:开发备忘第 7 条;核实存在资金费实收推送后改 `funding_settle` 并改写本用例

### BGT-140 Config 请求序列按产品线

- 规则:[账户配置核验](README.md#账户配置核验)
- 级别:table
- 里程碑:M3
- 前置:无参数;`acct_queries(AcctQueryKind::Config)`
- 实现:`crates/venue-bitget/tests/acct.rs::bgt_140_config_queries`

| 行 | 产品线 | 期望 `[(step, every, push_fallback, weight)]` |
|---|---|---|
| BGT-140.1 | 永续 | `[(0,1,false,1), (1,1,false,1), (2,1,false,1), (5,1,false,1)]`,`kind` 全为 `Config` |
| BGT-140.2 | 现货 | `[(0,1,false,1), (2,1,false,1), (3,1,false,1), (4,1,false,1)]` |

### BGT-141 Margin 请求序列:永续两步,现货为空

- 规则:[保证金数据换算](README.md#保证金数据换算)
- 级别:table
- 里程碑:M3
- 前置:无参数;`acct_queries(AcctQueryKind::Margin)`
- 实现:`crates/venue-bitget/tests/acct.rs::bgt_141_margin_queries`

| 行 | 产品线 | 期望 |
|---|---|---|
| BGT-141.1 | 永续 | `[(0,1,false,1), (1,1,false,1)]`,`kind` 全为 `Margin` |
| BGT-141.2 | 现货 | 空切片 |
| BGT-141.3 | 现货 `acct_finish(Margin)`(空 `AcctScratch`) | `Ok`,`sink.margin` 与 `sink.position` 调用 0 次 |

### BGT-142 永续账户配置核验的每个不符位

当前 AccountProtocol 测试覆盖 UID 缺失策略、实际 UID 矛盾及提现权限拒绝；各不符位细分报告仍未完成，当前失败统一由运行时发布 UNREADABLE。

- 规则:[账户配置核验](README.md#账户配置核验)、[账户形态(官方,本地缓存的 UTA 概览页)](README.md#账户形态官方本地缓存的-uta-概览页)、[契约](../hft-venue-proto/README.md#实现方必须遵守的契约)
- 级别:table
- 里程碑:M3
- 前置:`RD-BGT`;新 `AcctCtx`;基准响应(HTTP 200):步 0 = UTA 账户、基础模式、单向持仓;步 1 = BTCUSDT、ETHUSDT 均全仓;步 2 = UID `acct-0001`、权限含交易不含提现;步 5 = BTCUSDT 杠杆 5、ETHUSDT 杠杆 10。依次 `acct_decode` 四步后 `acct_finish(Config)`;每行只改所列一处
- 实现:`crates/venue-bitget/tests/acct.rs::bgt_142_config_uid_and_permissions_fail_closed`
- 备注:依赖待核实:开发备忘第 5、15 条(端点与字段);输入为待采集(方法 A + B:模拟盘账户逐项切换后抓响应)

| 行 | 改动 | 期望 `AccountConfigReport` |
|---|---|---|
| BGT-142.1 | 无 | `mismatches = 0`;`position_mode = OneWay`(2),`margin_mode = Cross`(2),`multi_asset = Off`(2);`n_lev = 2`;`uid_keys = SESSION + REST`(3);`venue_uid = "acct-0001"` 右补 0;`source = Startup`(1) |
| BGT-142.2 | 步 0 为 Classic 账户 | `ACCT_TYPE`(16) |
| BGT-142.3 | 步 0 为逐仓模式 | `MARGIN_MODE`(2);`margin_mode = Isolated`(3) |
| BGT-142.4 | 步 0 为高级模式 | `MULTI_ASSET`(4);`multi_asset = On`(3) |
| BGT-142.5 | 步 0 为双向持仓 | `POS_MODE`(1);`position_mode = Hedge`(3) |
| BGT-142.6 | 步 1 ETHUSDT 为逐仓 | `MARGIN_MODE`(2);`margin_mode = Mixed`(4) |
| BGT-142.7 | 步 1 两者都为逐仓 | `MARGIN_MODE`(2);`margin_mode = Isolated`(3) |
| BGT-142.8 | 步 2 UID 为 `acct-0002` | `UID`(128);`venue_uid = "acct-0002"` |
| BGT-142.9 | 步 2 权限含提现 | `KEY_PERM`(64) |
| BGT-142.10 | 步 2 权限不含交易 | `KEY_PERM`(64) |
| BGT-142.11 | 步 0 HTTP 500 | `UNREADABLE`(256);不置 `ACCT_TYPE`、`POS_MODE` |
| BGT-142.12 | 步 2 缺 UID 字段 | `UNREADABLE`(256);不置 `UID` |
| BGT-142.13 | 步 1 缺 ETHUSDT | `UNREADABLE`(256) |
| BGT-142.14 | 步 5 缺 ETHUSDT | `mismatches = 0`;ETHUSDT 杠杆条目 `leverage = 0` |
| BGT-142.15 | 步 5 HTTP 500 | `mismatches = 0`;两条杠杆条目都为 0 |
| BGT-142.16 | 步 0 双向持仓且步 2 含提现 | `POS_MODE + KEY_PERM`(65) |
| BGT-142.17 | 步 2 响应体不是 JSON | `UNREADABLE`(256);`acct_decode` 不 panic |
| BGT-142.18 | 步 2 缺 UID 字段,账户为 `uid_policy = "unverifiable"`、`venue_uid = ""`(模拟盘) | `mismatches = 0`(缺 UID 不置 `UID`、`UNREADABLE`);`uid_keys = 0`;`venue_uid` 全 0 |

### BGT-143 现货账户配置核验的每个不符位

- 规则:[账户配置核验](README.md#账户配置核验)、[契约](../hft-venue-proto/README.md#实现方必须遵守的契约)
- 级别:table
- 里程碑:M3
- 前置:`RD-BGT`;现货;基准响应:步 0 = UTA 基础模式;步 2 = UID `acct-0001`、权限含交易不含提现;步 3 = BGB 抵扣开启;步 4 = 现货借币/杠杆关闭;生效 `fee_asset_rule = ReceivedAssetUnlessDiscount{BGB}`(除注明外)
- 实现:`crates/venue-bitget/tests/acct.rs::bgt_143_config_spot_bits`
- 备注:依赖待核实:开发备忘第 5、8、15 条;输入为待采集(方法 A + B)

| 行 | 改动 | 期望 `AccountConfigReport` |
|---|---|---|
| BGT-143.1 | 无 | `mismatches = 0`;`position_mode`、`margin_mode`、`multi_asset` 均 `NotApplicable`(1);`n_lev = 0`;`uid_keys = 3` |
| BGT-143.2 | 步 0 为 Classic 账户 | `ACCT_TYPE`(16) |
| BGT-143.3 | 步 3 BGB 抵扣关闭 | `FEE_RULE`(8) |
| BGT-143.4 | refdata 覆盖 `fee_asset_rule = ReceivedAsset`,步 3 抵扣开启 | `FEE_RULE`(8) |
| BGT-143.5 | refdata 覆盖 `fee_asset_rule = ReceivedAsset`,步 3 抵扣关闭 | `mismatches = 0` |
| BGT-143.6 | 步 4 借币开启 | `AUTO_BORROW`(32) |
| BGT-143.7 | 步 4 读不到(HTTP 404 或字段缺失) | `UNREADABLE`(256);现货账户过不了 `ACCOUNT_CFG` |
| BGT-143.8 | 步 2 UID 为 `acct-0002` | `UID`(128) |
| BGT-143.9 | 步 2 权限含提现 | `KEY_PERM`(64) |
| BGT-143.10 | 步 3 HTTP 500 | `UNREADABLE`(256) |
| BGT-143.11 | 步 0 为高级(多资产)模式 | `ACCT_TYPE`(16);不置 `MULTI_ASSET` |

### BGT-144 会话与 REST 同一把 key,UID 步覆盖两种用途

- 规则:[账户配置核验](README.md#账户配置核验)、[契约](../hft-venue-proto/README.md#实现方必须遵守的契约)
- 级别:unit
- 里程碑:M3
- 前置:`KEY`;`now_ms = 1790000000000`;两个产品线
- 步骤:
  1. `session_identity_request(&cfg, &KEY, now_ms, &mut out)`
  2. BGT-142.1 / BGT-143.1 的报告
- 期望:
  - 步骤 1 返回 `Ok(false)`,`out` 未写
  - 步骤 2 `uid_keys = SESSION | REST`(3)
- 实现:`crates/venue-bitget/tests/acct.rs::bgt_144_single_key_identity`

### BGT-145 杠杆条目逐 inst 给出,只含 refdata 中可交易的永续

- 规则:[账户配置核验](README.md#账户配置核验)
- 级别:unit
- 里程碑:M3
- 前置:`RD-BGT`;BGT-142.1 的响应,步 5 另含 refdata 中没有的 `DOGEUSDT`(杠杆 20)
- 步骤:
  1. 完成 Config 序列
- 期望:
  - `n_lev = 2`;`LeverageItem{inst: 1801, leverage: 5000000000}`、`LeverageItem{inst: 1802, leverage: 10000000000}`;无 DOGEUSDT 条目
  - `rflags = 0`
- 实现:`crates/venue-bitget/tests/acct.rs::bgt_145_leverage_items`

### BGT-146 Margin 序列产出统一口径的 MarginReport

- 规则:[保证金数据换算](README.md#保证金数据换算)、[契约](../hft-venue-proto/README.md#实现方必须遵守的契约)
- 级别:unit
- 里程碑:M3
- 前置:`RD-BGT`;新 `AcctCtx`;步 0 = 账户权益 `10000`、维持保证金 `250`、可用 `8000.5`、交易所保证金率 `0.0251`(含部分强平手续费);步 1 = BTCUSDT 多 `0.01`(开仓均价 `64000`、未实现 `10.5`、强平价 `52000.5`)与 ETHUSDT 空 `0.5`(强平价 `"0"`),无 ADL 档字段;`rx` = `RX`
- 步骤:
  1. `acct_decode` 两步,`acct_finish(Margin)`
- 期望:
  - 一次 `sink.margin`:`MarginReportHdr{venue: 10, account: 2, source: Poll(2), mflags: 0, n: 2, margin_balance: 10000000000, maint_margin: 250000000, avail_margin: 8000500000, margin_ratio: 25000000}`(= 250 / 10000,不采用 0.0251)
  - 条目 `[MarginItem{inst: 1801, adl_quantile: 255, liq_px: 5200050}, MarginItem{inst: 1802, adl_quantile: 255, liq_px: 0}]`
  - 两条 `PositionReport`:`{inst: 1801, qty: 1000000, entry_px: 6400000, upnl: 10500000, liq_px: 5200050}`、`{inst: 1802, qty: -50000000, liq_px: 0}`
- 实现:`crates/venue-bitget/tests/acct.rs::bgt_146_margin_report_and_position_items`
- 备注:依赖待核实:开发备忘第 5 条(端点与字段)

### BGT-147 margin_ratio 的边界

- 规则:[保证金数据换算](README.md#保证金数据换算)、[契约](../hft-venue-proto/README.md#实现方必须遵守的契约)
- 级别:table
- 里程碑:M3
- 前置:同 BGT-146,只改步 0 的权益与维持保证金
- 实现:`crates/venue-bitget/tests/acct.rs::bgt_147_margin_ratio_edges`

| 行 | 权益、维持保证金 | 期望 `margin_ratio` |
|---|---|---|
| BGT-147.1 | `10000`、`0` | 0 |
| BGT-147.2 | `0`、`250` | `Rate::MAX` |
| BGT-147.3 | `-5`、`250` | `Rate::MAX` |
| BGT-147.4 | `10000`、`20000` | 2000000000 |
| BGT-147.5 | `10000`、`10000` | 1000000000(达强平线) |
| BGT-147.6 | `0.000001`、`0` | 0 |
| BGT-147.7 | 交易所比率字段 `0.9`,权益 `10000`、维持 `250` | 25000000(交易所比率只作对照) |

### BGT-148 margin_ratio 与 i128 参考实现一致

- 规则:[保证金数据换算](README.md#保证金数据换算)
- 级别:prop
- 里程碑:M3
- 性质:对任意权益与维持保证金(`Notional`),本 crate 的换算等于 i128 参考 `maint × 10^9 / balance`(向下取整,结果超出 `Rate` 范围时饱和到 `Rate::MAX`);`balance ≤ 0` 时 `Rate::MAX`;不 panic
- 生成:`balance`、`maint` 取 `[-10^15, 10^15]`,另含 0、±1、`i64::MAX`、`i64::MIN + 1`
- 参考:`tests/model.rs::ratio_ref`
- 例数:PR 5 000
- 实现:`crates/venue-bitget/tests/acct.rs::bgt_148_prop_margin_ratio`

### BGT-149 Margin 序列的失败与过滤

- 规则:[保证金数据换算](README.md#保证金数据换算)、[实现说明](README.md#实现说明)
- 级别:table
- 里程碑:M3
- 前置:同 BGT-146
- 实现:`crates/venue-bitget/tests/acct.rs::bgt_149_margin_failures`

| 行 | 改动 | 期望 |
|---|---|---|
| BGT-149.1 | 步 1 另含 `DOGEUSDT` 持仓 | 条目与 `PositionReport` 中都没有 DOGEUSDT;`n = 2` |
| BGT-149.2 | 步 1 为空持仓列表 | `n = 0`;0 条 `PositionReport`;报告头照常产出 |
| BGT-149.3 | 步 0 HTTP 500 | `acct_finish` 返回 `Err(DecodeErr::Shape)`,`sink.margin` 调用 0 次 |
| BGT-149.4 | 步 0 缺维持保证金字段 | `Err(DecodeErr::Shape)`,0 条报告 |
| BGT-149.5 | 步 1 强平价 `""` | 该条目 `liq_px = 0` |

### BGT-150 账户查询不依赖回报解码的状态

- 规则:[契约](../hft-venue-proto/README.md#实现方必须遵守的契约)
- 级别:unit
- 里程碑:M3
- 前置:`RD-BGT`;`ctx_a = AcctCtx::default()`;`ctx_b` = 已运行过 10³ 个 BGT-146 序列的 `AcctCtx`;另一 `GwCtx` 已解码 10⁴ 条 BGT-101 形态推送
- 步骤:
  1. 在 `ctx_a`、`ctx_b` 上分别跑 BGT-142.1 与 BGT-146 的序列
- 期望:
  - 两个上下文产出的 `AccountConfigReport` 与 `MarginReport` 逐字节相同(本 crate 不声明跨序列沿用的字段)
- 实现:`crates/venue-bitget/tests/acct.rs::bgt_150_acct_isolation`

### BGT-170 rate_rules 默认值

- 规则:[限速默认值](README.md#限速默认值)
- 级别:table
- 里程碑:M3
- 前置:两个产品线各断言一遍
- 实现:`crates/venue-bitget/tests/session.rs::bgt_170_rate_rules`

| 行 | 条目 | 期望 `RateRule` |
|---|---|---|
| BGT-170.1 | 下单 | `{scope: Uid, bucket: Orders, window_ms: 1000, limit: 10}` |
| BGT-170.2 | WS 建连 | `{scope: Ip, bucket: NewConns, window_ms: 300000, limit: 300}` |
| BGT-170.3 | 客户端入向 | `{scope: Conn, bucket: WsInMsgs, window_ms: 1000, limit: 10}` |
| BGT-170.4 | 条目总数 | 3;没有 `Weight` 桶的条目 |

### BGT-171 无过期保护、无交易所侧死手

- 规则:[VenueCaps 默认值](README.md#venuecaps-默认值)、[实现说明](README.md#实现说明)、[契约](../hft-venue-proto/README.md#实现方必须遵守的契约)
- 级别:table
- 里程碑:M3
- 前置:`RD-BGT`;`WC`;两个产品线
- 实现:`crates/venue-bitget/tests/session.rs::bgt_171_no_stale_guard_no_dead_man`

| 行 | 调用 | 期望 |
|---|---|---|
| BGT-171.1 | `stale_guard()` | `StaleGuard::None` |
| BGT-171.2 | `dead_man()` | `DeadManSpec::None` |
| BGT-171.3 | `dead_man_refresh(rd, &t, 30000, &[1801], &WC, &mut out)` | `Err(EncodeErr::Unsupported(GwOp::DeadManRefresh))`;`out` 为空 |
| BGT-171.4 | `dead_man_refresh(…, insts = &[])` | 同上 |

### BGT-172 永续 default_caps 每个字段

- 规则:[VenueCaps 默认值](README.md#venuecaps-默认值)
- 级别:table
- 里程碑:M3
- 前置:`BitgetUsdtFutures::default_caps()`
- 实现:`crates/venue-bitget/tests/session.rs::bgt_172_fut_default_caps`

| 行 | 字段 | 期望 |
|---|---|---|
| BGT-172.1 | `venue` / `market` | `BITGET_UTA_USDT_FUT` / `LinearPerp` |
| BGT-172.2 | `fee_asset_rule` | `Quote` |
| BGT-172.3 | `reduce_only` | `true` |
| BGT-172.4 | `required_position_mode` | `OneWay` |
| BGT-172.5 | `required_margin_mode` | `Cross` |
| BGT-172.6 | `required_multi_asset` | `Some(false)` |
| BGT-172.7 | `required_spot_borrow` | `None` |
| BGT-172.8 | `funding_settle` | `RateOnly`(3) |
| BGT-172.9 | `margin_data` | `Poll{interval_ms: 5000}` |
| BGT-172.10 | `own_liquidation_marker` | `OrderTypeField`(3) |
| BGT-172.11 | `liq_marker_arg` 按 `\|` 拆分 | 16 项:双向 `burst_close_long/short`、`reduce_close_long/short`、`dte_sys_adl_close_long/short`、`delivery_close_long/short`;单向 `burst_buy_single`、`burst_sell_single`、`reduce_buy_single`、`reduce_sell_single`、`dte_sys_adl_buy_in_single_side_mode`、`dte_sys_adl_sell_in_single_side_mode`、`delivery_buy_single`、`delivery_sell_single`(官方枚举页) |
| BGT-172.12 | `ignorable_user_events` | 空 |
| BGT-172.13 | `flags` | `POST_ONLY + IOC + FOK` = 14;`AMEND`(改单响应只是回执)与 `BATCH_CANCEL` 位为 0 |
| BGT-172.14 | `dms` 与四个时长 | `DeadManCap::None`(1);`dms_min_ms = dms_max_ms = dms_timeout_ms = dms_refresh_ms = 0` |
| BGT-172.15 | `testnet` | `TestnetKind::Demo`(3) |
| BGT-172.16 | `amend_qty_down_only` | false |

### BGT-173 现货 default_caps 每个字段

- 规则:[VenueCaps 默认值](README.md#venuecaps-默认值)
- 级别:table
- 里程碑:M3
- 前置:`BitgetSpot::default_caps()`
- 实现:`crates/venue-bitget/tests/session.rs::bgt_173_spot_default_caps`

| 行 | 字段 | 期望 |
|---|---|---|
| BGT-173.1 | `venue` / `market` | `BITGET_UTA_SPOT` / `Spot` |
| BGT-173.2 | `fee_asset_rule` | `ReceivedAssetUnlessDiscount{token: 资产名 "BGB"}`(hft-cfg 解析为 `AssetId`) |
| BGT-173.3 | `reduce_only` | `false` |
| BGT-173.4 | `required_position_mode` / `required_margin_mode` | `NotApplicable` / `NotApplicable` |
| BGT-173.5 | `required_multi_asset` | `None` |
| BGT-173.6 | `required_spot_borrow` | `Some(false)` |
| BGT-173.7 | `funding_settle` | `NotApplicable`(4) |
| BGT-173.8 | `margin_data` | `NotApplicable` |
| BGT-173.9 | `own_liquidation_marker` / `liq_marker_arg` | `NotIdentifiable`(1) / `""` |
| BGT-173.10 | `ignorable_user_events` | 空 |
| BGT-173.11 | `flags` | 14 |
| BGT-173.12 | `dms` 与四个时长 | `None`(1);四个时长 0 |
| BGT-173.13 | `testnet` | `Demo`(3) |
| BGT-173.14 | `amend_qty_down_only` | false |

### BGT-174 hft-venue-proto 契约检查全部通过

- 规则:[契约](../hft-venue-proto/README.md#实现方必须遵守的契约)、[VenueCaps 默认值](README.md#venuecaps-默认值)
- 级别:table
- 里程碑:M3
- 前置:`tests/contract.rs` 对两个产品线各调用 hft-venue-proto `testkit` 的全部契约检查;样本取本 crate 金样(BGT-300–BGT-333)
- 实现:`crates/venue-bitget/tests/contract.rs::bgt_174_contract_checks`

| 行 | 检查 | 期望(两个产品线) |
|---|---|---|
| BGT-174.1 | `check_unknown_codes` | 通过 |
| BGT-174.2 | `check_cum` | 通过(含 BGT-105 的合成累计量路径) |
| BGT-174.3 | `check_unknown_events` | 通过 |
| BGT-174.4 | `check_dedup` | 通过;资金费子项因 `RateOnly` 无样本、按 0 条 `FundingSettle` 判定 |
| BGT-174.5 | `check_external_fill` | 永续:强平、ADL、交割、人工样本通过(人工样本期望 `cause = Unknown`);现货:未知来源样本通过 |
| BGT-174.6 | `check_fee_asset` | 通过 |
| BGT-174.7 | `check_account_config` | 通过(现货含借币开启 → `AUTO_BORROW`、借币读不到 → `UNREADABLE`) |
| BGT-174.8 | `check_identity` | 通过(`Ok(false)` 分支:`uid_keys` 覆盖会话与 REST) |
| BGT-174.9 | `check_default_caps` | 通过(`dead_man() = None` ↔ `dms = None`;现货编码带 `REDUCE_ONLY` → `Unsupported`) |
| BGT-174.10 | `check_snapshot_isolation` | 通过(BBO 补帧) |
| BGT-174.11 | `check_topn` | 通过(`public-topn` 恰一条,`topn_levels = [5, 50]`) |
| BGT-174.12 | `check_acct_isolation` | 通过 |
| BGT-174.13 | `check_md_basics` | 通过 |
| BGT-174.14 | `check_hot_alloc` | 通过 |
| BGT-174.15 | `check_session` | 通过(哨兵密钥为 `KEY` 的替身) |
| BGT-174.16 | `check_cid` | 通过 |
| BGT-174.17 | `check_funding` | 通过(`RateOnly`:0 条 `FundingSettle`) |
| BGT-174.18 | `check_margin` | 永续按 `Poll` 通过;现货 Margin 序列为空 |
| BGT-174.19 | `check_bbo_refresh` | 通过(批量 1,inst 取自传入列表) |
| BGT-174.20 | `check_detail` | 通过(核实前无 `GONE` 类码,撤改错误 `detail = 0`;查单状态写 `detail`) |
| BGT-174.21 | `check_rest` | 通过(`Query` 为 REST;`CANCEL_ALL_SWEEP = false`;会话维护 `Ok(false)`) |
| BGT-174.22 | `check_ctl` | 通过(核实前只有 HTTP 429 → `RateLimited`) |

### BGT-175 只读查询的请求与解码通过 check_ro

当前 warm 入口由 `crates/venue-bitget/tests/readonly.rs` 验证签名、整页失败原子性、分页与外来身份；FundingFlow 保留为该表未完成项，实额查询沿用 FundingHistoryProtocol。

- 规则:[只读查询](README.md#只读查询)、[契约](../hft-venue-proto/README.md#实现方必须遵守的契约)
- 级别:table
- 里程碑:M5
- 前置:`RD-BGT`;以只读测试凭证(同 `KEY` 向量)构建的 `Templates`;`WC`;样本取 BGT-334–BGT-338
- 实现:`crates/venue-bitget/tests/ro.rs::bgt_175_read_only_queries`
- 备注:依赖待核实:开发备忘第 18 条(端点与字段)

| 行 | 调用 | 期望 |
|---|---|---|
| BGT-175.1 | 永续五种 `RoKind` 的 `ro_request` | 都 `Ok`;`method = GET`;`redacted = true`;带 `ACCESS-*` 头 |
| BGT-175.2 | 现货 `ro_request(Positions)`、`ro_request(FundingFlow)` | 都 `Err(EncodeErr::Unsupported(GwOp::Query))` |
| BGT-175.3 | `ro_decode(OpenOrders)`:本账户 1 单 + 外来 1 单 | 2 次 `open_order`,外来单 `slot = UNRESOLVED` |
| BGT-175.4 | 两个产品线交给 `check_ro` | 通过 |

### BGT-190 心跳规格是无条件的 25 s 文本 ping

- 规则:[心跳与 BBO 探测](README.md#心跳与-bbo-探测)、[实现说明](README.md#实现说明)
- 级别:table
- 里程碑:M3
- 前置:两个产品线
- 实现:`crates/venue-bitget/tests/session.rs::bgt_190_unconditional_app_ping`

| 行 | 调用 | 期望 |
|---|---|---|
| BGT-190.1 | `heartbeat(Role::Md)` | `HeartbeatSpec{ping: AppText("ping"), interval_ms: 25000, unconditional: true, pong_timeout_ms: 10000, idle_timeout_ms: 30000}` |
| BGT-190.2 | `heartbeat(Role::Order)` | 同上 |
| BGT-190.3 | `app_ping(Role::Md, now_ms, &mut buf[16])` | 返回 4,`buf[..4] = b"ping"` |
| BGT-190.4 | `app_ping(Role::Order, …)` | 返回 4,`b"ping"` |
| BGT-190.5 | `bbo_probe()` | `PingKind::AppText` |
| BGT-190.6 | `interval_ms` 与服务端 120 s 断开阈值 | 25000 < 120000;`interval_ms + pong_timeout_ms` = 35000 < 120000 |

### BGT-191 数据持续推送时 10 min 内仍每 25 s 发 ping,模拟服务端不断开

- 规则:[心跳与 BBO 探测](README.md#心跳与-bbo-探测)、[实现说明](README.md#实现说明)
- 级别:unit
- 里程碑:M3
- 前置:假时钟,步长 1 ms;模拟服务端模型 = "距上次收到客户端 `ping` ≥ 120 s 即断开"(数据每 100 ms 推送一条);客户端调度模型按 [hft-venue §5](../hft-venue/README.md#5-心跳) 的规则消费 `heartbeat(Role::Md)`(`unconditional = true` 时与有无入站数据无关);服务端收到 `ping` 立即回 `pong`
- 步骤:
  1. 运行 600 s
- 期望:
  - 客户端发出 `ping` 24 次,时刻为 25 s、50 s、…、600 s;相邻两次间隔恰 25 s
  - 服务端全程未断开;每个 `ping` 在 10 s 内得到 `pong`,无 `PING_TIMEOUT`
- 实现:`crates/venue-bitget/tests/heartbeat.rs::bgt_191_unconditional_ping_survives`
- 备注:真实 `FeedRuntime` 与 hft-net 上的同一回归属 [hft-venue](../hft-venue/README.md) 的集成测试(venue crate 不依赖运行时)

### BGT-192 只在空闲时 ping 的配置在持续数据下被断开(反例)

- 规则:[实现说明](README.md#实现说明)、[心跳与 BBO 探测](README.md#心跳与-bbo-探测)
- 级别:unit
- 里程碑:M3
- 前置:同 BGT-191,但客户端调度使用 `heartbeat(Role::Md)` 改成 `unconditional = false` 的副本
- 步骤:
  1. 运行 600 s
- 期望:
  - 客户端发出 `ping` 0 次;模拟服务端在 t = 120 s 断开
  - 断言 `heartbeat(Role::Md).unconditional == true`(本 crate 的常量不得是上面的副本)
- 实现:`crates/venue-bitget/tests/heartbeat.rs::bgt_192_idle_only_ping_dropped`

### BGT-193 BBO 连接的探测与订阅节奏不超过入向上限

- 规则:[心跳与 BBO 探测](README.md#心跳与-bbo-探测)
- 级别:unit
- 里程碑:M3
- 前置:`feed.bbo_ping_ms = 1000`;`books1` 路由的 `max_in_msgs_per_s = 10`(BGT-002.1)
- 步骤:
  1. 按 hft-venue §3a 的公式算订阅帧节奏:`max_in_msgs_per_s / 2 − 1000 / bbo_ping_ms`
  2. 合计每秒入向帧:探测 + 订阅帧
- 期望:
  - 步骤 1 = 4 帧/s
  - 步骤 2 = 5 ≤ 10;探测间隔 1 s ≤ 25 s,满足无条件 ping 的要求
- 实现:`crates/venue-bitget/tests/heartbeat.rs::bgt_193_bbo_probe_budget`
- 备注:依赖待核实:开发备忘第 16 条(每秒一次应用层 `ping` 是否被允许)

### BGT-200 订阅帧每帧至多 50 个 args 且不带 id

- 规则:[实现说明](README.md#实现说明)
- 级别:table
- 里程碑:M3
- 前置:永续;`RD-BGT` 另含 118 个合成永续 inst(共 120 个);`SubSpec` 按行
- 实现:`crates/venue-bitget/tests/session.rs::bgt_200_subscribe_frames`
- 备注:依赖待核实:开发备忘第 1 条(UTA `args` 字段名)

| 行 | `subs` | 期望 |
|---|---|---|
| BGT-200.1 | 120 个 inst 的 `Bbo` | 3 帧,`args` 数依次 50、50、20;每帧 `op = "subscribe"`,没有顶层 `id` 键;每个 arg 为 `{"instType":"usdt-futures","topic":"books1","symbol":…}` |
| BGT-200.2 | 1801 的 `DepthTopN`,`levels = 5` | 1 帧,1 个 arg,频道 `books5` |
| BGT-200.3 | 1801 的 `DepthTopN`,`levels = 50` | 频道 `books50`;`levels = 15` → `Err(ProtoErr::Unsupported)` |
| BGT-200.4 | 1801 的 `DepthTopN`,`levels = 10` | `Err(ProtoErr::Unsupported)`,不写帧 |
| BGT-200.5 | 1801 的 `Trade` | 频道 `publicTrade` |
| BGT-200.6 | 1801 的 `Mark`、`Index`、`Funding` 各一个 | 1 个 arg,频道 `ticker`(三者合并,不重复订阅) |
| BGT-200.7 | 1801 的 `Liquidation` | `Err(ProtoErr::Unsupported)`(公共强平流不订阅) |
| BGT-200.8 | 现货 1901 的 `Mark` | `Err(ProtoErr::Unsupported)` |
| BGT-200.9 | 空 `subs` | `Ok`,0 帧 |
| BGT-200.10 | 1801 的 `DepthIncr` | 频道 `books` |

### BGT-201 私有频道订阅帧只含本产品线的频道

- 规则:[用户流事件表](README.md#用户流事件表)、[实现说明](README.md#实现说明)、[账户形态(官方,本地缓存的 UTA 概览页)](README.md#账户形态官方本地缓存的-uta-概览页)
- 级别:table
- 里程碑:M3
- 前置:账户 2
- 实现:`crates/venue-bitget/tests/session.rs::bgt_201_private_subscribe`
- 备注:依赖待核实:开发备忘第 1 条(私有频道名全集、全账户订阅写法)

| 行 | 调用 | 期望 |
|---|---|---|
| BGT-201.1 | 永续 `private_subscribe(cfg, 2, false, &mut out)` | 1 帧 `{"op":"subscribe","args":[{"instType":"UTA","topic":"order"},{"instType":"UTA","topic":"position"},{"instType":"UTA","topic":"account"}]}`;无 `id`(UTA 私有频道覆盖整个统一账户,`decode_report` 按 `category` 只保留本产品线) |
| BGT-201.2 | 现货 `private_subscribe(…, false, …)` | 频道恰为 `order`、`account`;无 `position` |
| BGT-201.3 | 永续 `private_subscribe(…, true, …)` | 同 BGT-201.1 的频道,`op = "unsubscribe"` |

### BGT-210 核实 UTA V3 行情与私有路径、订阅写法、频道名与分档快照

- 规则:[开发备忘](README.md#开发备忘)、[端点](README.md#端点)、[分档快照流](README.md#分档快照流)
- 级别:testnet
- 里程碑:M3
- 环境:东京服务器(方法 A:抓官方 UTA 文档原文缓存到 `architecture-v2/research/_src/`)+ dev2 联调窗口内的模拟盘连接(方法 B)
- 授权:`[需授权]`(使用服务器出网与模拟盘 key)
- 步骤:
  1. 方法 A:缓存 UTA V3 WebSocket 公共与私有频道页原文
  2. 方法 B:连 `/v3/ws/public`,分别订阅 `books1`、`books5`、`books15`、`publicTrade`、`ticker`(永续与现货各一个 inst),各抓 10 min raw;记录订阅帧写法、首帧是否立即推当前快照、`seq` 是否单调、`books5`/`books15` 节拍
  3. 登录 `/v3/ws/private`,订阅订单、成交(若存在)、持仓、账户频道,抓 raw
- 通过标准:开发备忘第 1、17 条的问题逐项有结论并写回 README"端点""行情频道 → 事件""分档快照流""用户流事件表";BGT-004、BGT-010、BGT-200、BGT-201 按结论改写后通过
- 证据:`fixtures/bitget-usdt-futures/*/v3_*.in` 与 `fixtures/bitget-spot/*/v3_*.in`(清洗后);原文缓存文件路径写进 README 延伸阅读;记录 `docs/ops-log` 一条,文件名含 BGT-210
- 实现:`tests/host/BGT-210.sh`

### BGT-211 核实 UTA V3 WS 下单(M3 之前)

- 规则:[下单:UTA V3 WS](README.md#下单uta-v3-ws)、[开发备忘](README.md#开发备忘)
- 级别:testnet
- 里程碑:M3
- 环境:dev2 联调窗口;Bitget 模拟盘 UTA 账户(永续与现货各一个子账户)
- 授权:`[需授权]`
- 步骤:
  1. 方法 A:缓存 `place-order`、`batch-place`、`modify-order`、`cancel-order` 频道页原文
  2. 方法 B:在模拟盘以远离盘口的价格各下一笔限价单(GTC、IOC、post-only 各一),改单一次,撤单一次;另以 FOK 试下一笔;带 `pxAmendType` 的超价格带单一笔;抓请求与响应 raw
- 通过标准:WS 下单频道可用(否则按 README 改走 REST 并重新设计下单路径,本 crate 用例同一提交改写);`clientOid` 长度上限与字符集、`reduceOnly`、post-only/IOC/FOK 写法、`pxAmendType` 取值、请求 id 字段、响应字段(有无网关时间戳)、每频道限速、批量上限、STP 与过期保护字段写回 README;BGT-064–BGT-071、BGT-090 改为逐字节期望后通过
- 证据:`fixtures/bitget-usdt-futures/place-order/`、`modify-order/`、`cancel-order/` 的响应样本;请求帧只记字段表(签名帧不入库)
- 实现:`tests/host/BGT-211.sh`

### BGT-212 核实订单推送的累计量、费用与资金费推送

- 规则:[累计量与费用来源](README.md#累计量与费用来源)、[开发备忘](README.md#开发备忘)
- 级别:testnet
- 里程碑:M3
- 环境:dev2 联调窗口;模拟盘永续与现货账户
- 授权:`[需授权]`
- 步骤:
  1. 方法 B:永续下一笔分 2 次以上成交的单;现货分别在 BGB 抵扣开、关下各成交一笔买单
  2. 永续持仓跨过一次资金费结算(模拟盘不结算时记录"不可用")
- 通过标准:开发备忘第 3、7、8 条有结论:订单推送是否带累计量与均价、费用是本笔还是累计及其符号、成交频道是否存在及费用是否晚到、资金费是否有私有推送、BGB 不足时的回退;结论写回 README"累计量与费用来源""VenueCaps 默认值";BGT-101、BGT-105–BGT-108、BGT-117 按结论改写后通过
- 证据:对应 `fixtures/bitget-*/orders/`、`fills/`、`account/` 样本与 `meta.toml`
- 实现:`tests/host/BGT-212.sh`

### BGT-213 核实错误码全表并分为确定拒绝与结果未知

- 规则:[结果未知码表](README.md#结果未知码表)、[开发备忘](README.md#开发备忘)
- 级别:testnet
- 里程碑:M3
- 环境:东京服务器(方法 A)+ Bitget 机构团队书面答复(方法 D)
- 授权:`[需授权]`
- 步骤:
  1. 方法 A:缓存错误码页原文
  2. 方法 D:书面确认"系统繁忙/超时类码是否可能已执行"
  3. 在模拟盘触发参数错误、余额不足、超价格带、reduceOnly 被拒、限速各一次,抓响应
- 通过标准:开发备忘第 4 条有结论;`codes.rs` 的确定拒绝表只登记步骤 3 观察到且有原文佐证的码;README"结果未知码表"写出码的形态;BGT-091 增补每个登记码一行后通过
- 证据:原文缓存、书面答复存档路径、步骤 3 的响应金样
- 实现:`tests/host/BGT-213.sh`

### BGT-214 核实账户设置、账户信息、资产、持仓、借币与杠杆端点

- 规则:[账户配置核验](README.md#账户配置核验)、[保证金数据换算](README.md#保证金数据换算)、[开发备忘](README.md#开发备忘)
- 级别:testnet
- 里程碑:M3
- 环境:dev2 联调窗口;模拟盘 UTA 账户
- 授权:`[需授权]`
- 步骤:
  1. 方法 A:缓存账户类接口原文
  2. 方法 B:依次切换基础/高级/逐仓模式、单向/双向持仓、某品种逐仓、BGB 抵扣、现货借币开关,每次调用 Config 序列各接口并抓响应;调用资产与持仓接口
- 通过标准:开发备忘第 5、15 条有结论(端点、字段、限速、是否有账户配置变化推送、账户频道是否带保证金率);README"账户配置核验""保证金数据换算"表的候选写成确定端点;BGT-142、BGT-143、BGT-146 的输入从待采集变为真实样本且通过
- 证据:`fixtures/bitget-*/rest.*` 样本(UID 替换为 `acct-0001`)
- 实现:`tests/host/BGT-214.sh`

### BGT-215 核实单向持仓下强平、ADL、交割成交的开平类型取值

- 规则:[外部成交识别](README.md#外部成交识别)、[开发备忘](README.md#开发备忘)
- 级别:testnet
- 里程碑:M3
- 环境:dev2 联调窗口;模拟盘永续账户,单向持仓
- 授权:`[需授权]`
- 步骤:
  1. 方法 A:缓存开平类型字段的枚举原文
  2. 方法 B:高杠杆小仓制造一次强平,抓订单与成交推送;普通开仓、平仓各一笔
- 通过标准:开发备忘第 6、14 条有结论;单向模式的强平/ADL/交割取值与普通开平取值全集写进 README"外部成交识别"表与 `liq_marker_arg`;ADL 取不到样本时金样按文档构造并标"合成";BGT-109、BGT-172.11 按结论增补后通过
- 证据:`fixtures/bitget-usdt-futures/orders/liquidation_*.in`
- 实现:`tests/host/BGT-215.sh`

### BGT-216 核实私有频道双订阅(M3 之前)

- 规则:[开发备忘](README.md#开发备忘)、[用户流事件表](README.md#用户流事件表)
- 级别:testnet
- 里程碑:M3
- 环境:dev2 联调窗口;模拟盘永续账户
- 授权:`[需授权]`
- 步骤:
  1. 两条 `/v3/ws/private` 连接同时登录并订阅订单频道
  2. 下一笔小单并撤单
  3. 比对两路 raw
- 通过标准:开发备忘第 12 条有结论;两路都收到 → 切换按先建后断;否则 README 写明"先断后建 + VERIFY"
- 证据:两路 raw 的比对报告,文件名含 BGT-216
- 实现:`tests/host/BGT-216.sh`

### BGT-217 核实模拟盘端点、请求头、符号映射与 UID 可读性

- 规则:[测试网/模拟盘](README.md#测试网模拟盘)、[开发备忘](README.md#开发备忘)
- 级别:testnet
- 里程碑:M3
- 环境:东京服务器(方法 A)+ dev2(方法 B)
- 授权:`[需授权]`
- 步骤:
  1. 缓存 UTA "Demo Trading" 页原文
  2. 申请模拟盘 key;连接模拟盘行情与私有端点;拉模拟盘品种接口并与主网比较符号、品种集合与 tick/lot
  3. 调账户信息接口,记录是否返回 UID
- 通过标准:开发备忘第 9 条有结论,写回 README"测试网/模拟盘";读不到 UID 时该账户在 refdata 配为 `uid_policy = "unverifiable"`(只允许 `is_prod = false`,规则归 hft-refdata);模拟盘不覆盖的接口逐项列入门槛 ⑥ 记录
- 证据:模拟盘 refdata `fixtures/refdata/bitget-demo-*.blob` 与生成它的 TOML;核实记录文件名含 BGT-217
- 实现:`tests/host/BGT-217.sh`

### BGT-218 核实 V3 登录时间戳单位与 BBO 连接每秒 ping 是否被允许

- 规则:[心跳与 BBO 探测](README.md#心跳与-bbo-探测)、[开发备忘](README.md#开发备忘)
- 级别:testnet
- 里程碑:M3
- 环境:dev2 联调窗口
- 授权:`[需授权]`
- 步骤:
  1. 分别以秒与毫秒时间戳登录 V3 私有端点,记录哪种被接受
  2. `books1` 连接每 1 s 发文本 `ping`,持续 30 min;同时订阅 4 帧/s
  3. 订阅后记录首帧是否立即推当前快照(未推 → 保留 BBO 补帧)
- 通过标准:开发备忘第 10、16 条有结论写回 README;30 min 内连接不因入向超限被断;BGT-060、BGT-050、BGT-193 按结论改写后通过
- 证据:raw 与记录,文件名含 BGT-218
- 实现:`tests/host/BGT-218.sh`

### BGT-219 books1 单向延迟与 V2 基线对比

- 规则:[行情频道 → 事件](README.md#行情频道--事件)
- 级别:host
- 里程碑:M3
- 环境:dev2,`prod` 角色测试网实例的 feed(主网公共行情),同时段 Classic V2 `books1` 连接作对照
- 授权:`[需授权]`
- 步骤:
  1. 同一组 inst 同时订阅 UTA `books1` 与 V2 `books1` 24 h
  2. `replayer latency` 按事件加权统计两路的 `hw_rx − ex_ts_ns`(单向)分位数
- 通过标准:报告给出两路 p50/p90/p99 与样本数(`n < 1000` 不报 p99);V2 的 p99 与 140 ms 基线比较的结论写进 Agent Note(只记录,不判失败)
- 证据:`replayer latency` 输出与 Agent Note 链接,文件名含 BGT-219
- 实现:`tests/host/BGT-219.sh`

### BGT-220 模拟盘连续 3 天冒烟(门槛 ⑥)

- 规则:[测试网/模拟盘](README.md#测试网模拟盘)
- 级别:testnet
- 里程碑:M6
- 门槛:⑥
- 环境:dev2 联调窗口;`prod` 角色实例,`feed_env = "testnet"`、`order_env = "testnet"`,行情与下单都用模拟盘,模拟盘 refdata(BGT-217)
- 授权:`[需授权]`
- 步骤:
  1. 对每个上线策略用到的 Bitget (venue, 市场) 跑下单、撤单、成交、断连、重同步、账户配置与 UID 查询、kill-switch 撤单与 reconciler 对账
  2. 连续运行 3 天(护栏停机时段暂停计时)
- 通过标准:3 天内解码错误 0 条未解释;未知用户流事件逐条有结论;`AccountConfigReport` 与 reconciler 真值一致;模拟盘不覆盖的接口逐项列入门槛记录
- 证据:门槛 ⑥ 记录(按 testing §8),文件名含 BGT-220
- 实现:`tests/host/BGT-220.sh`

### BGT-221 依赖闭包不含禁止的 crate

- 规则:[依赖](README.md#依赖)
- 级别:unit
- 里程碑:M3
- 前置:`cargo xtask deps`(读 `tools/xtask/deps.toml`);反例夹具 `crates/venue-bitget/tests/fixtures/bad-deps/Cargo.toml` = 本 crate 清单的副本,`[dependencies]` 多一行 `serde_json`
- 步骤:
  1. 对 venue-bitget 运行检查
  2. 对反例夹具运行同一检查
- 期望:
  - 步骤 1 退出码 0;`cargo tree -p venue-bitget -e normal` 不含 `hft-net`、`hft-venue`、`hft-sequencer`、`tokio`、`serde_json`
  - 步骤 2 退出码非 0,输出点名 `serde_json`(只允许在 `[dev-dependencies]`)
- 实现:`crates/venue-bitget/tests/deps.rs::bgt_221_forbidden_deps`

### BGT-300 金样:永续 books1

- 规则:[行情频道 → 事件](README.md#行情频道--事件)、[实现说明](README.md#实现说明)
- 级别:golden
- 里程碑:M3
- 输入:`fixtures/bitget-usdt-futures/books1/{v2_normal,v3_normal,seq_as_string,price_scientific,empty_bids,missing_ts}.in`;V2 来自现有采集系统 raw WAL,V3 待采集(方法 B,BGT-210),异常样本合成
- 期望文件:同目录 `<case>.expected.jsonl`(`empty_bids`、`missing_ts` 期望 `{"error":"Shape"}`)
- 实现:`crates/venue-bitget/tests/golden.rs::bgt_300_fut_books1`

### BGT-301 金样:现货 books1

- 规则:[行情频道 → 事件](README.md#行情频道--事件)
- 级别:golden
- 里程碑:M3
- 输入:`fixtures/bitget-spot/books1/{v2_normal,v3_normal,tiny_price,empty_asks}.in`;V2 来自采集系统 raw WAL,V3 待采集(方法 B)
- 期望文件:同目录 `<case>.expected.jsonl`
- 实现:`crates/venue-bitget/tests/golden.rs::bgt_301_spot_books1`

### BGT-302 金样:永续 books5

- 规则:[分档快照流](README.md#分档快照流)
- 级别:golden
- 里程碑:M3
- 输入:`fixtures/bitget-usdt-futures/books5/{v2_normal,v3_normal,duplicate_seq,fewer_levels,zero_qty_level}.in`;`duplicate_seq` 为同一 `seq` 连续两帧
- 期望文件:同目录 `<case>.expected.jsonl`(每帧一条 `L2Snapshot{origin = TopN}`;`duplicate_seq` 两条内容相同)
- 实现:`crates/venue-bitget/tests/golden.rs::bgt_302_fut_books5`

### BGT-303 金样:永续 books15

- 规则:[分档快照流](README.md#分档快照流)
- 级别:golden
- 里程碑:M3
- 输入:`fixtures/bitget-usdt-futures/books15/{v2_normal,v3_normal,fewer_levels}.in`;V3 待采集(方法 B)
- 期望文件:同目录 `<case>.expected.jsonl`
- 实现:`crates/venue-bitget/tests/golden.rs::bgt_303_fut_books15`

### BGT-304 金样:现货 books5 与 books15

- 规则:[分档快照流](README.md#分档快照流)
- 级别:golden
- 里程碑:M3
- 输入:`fixtures/bitget-spot/books5/{v2_normal,v3_normal,duplicate_seq}.in`、`fixtures/bitget-spot/books15/{v3_normal,fewer_levels}.in`
- 期望文件:同目录 `<case>.expected.jsonl`
- 实现:`crates/venue-bitget/tests/golden.rs::bgt_304_spot_books_topn`

### BGT-305 金样:永续 publicTrade

- 规则:[行情频道 → 事件](README.md#行情频道--事件)
- 级别:golden
- 里程碑:M3
- 输入:`fixtures/bitget-usdt-futures/publicTrade/{v2_snapshot_then_update,v3_snapshot_then_update,multi_trades_one_push,non_numeric_trade_id}.in`
- 期望文件:同目录 `<case>.expected.jsonl`(snapshot 批无输出行;`non_numeric_trade_id` 期望 `{"error":"Number"}`)
- 实现:`crates/venue-bitget/tests/golden.rs::bgt_305_fut_trade`

### BGT-306 金样:现货 publicTrade

- 规则:[行情频道 → 事件](README.md#行情频道--事件)
- 级别:golden
- 里程碑:M3
- 输入:`fixtures/bitget-spot/publicTrade/{v2_snapshot_then_update,v3_update}.in`
- 期望文件:同目录 `<case>.expected.jsonl`
- 实现:`crates/venue-bitget/tests/golden.rs::bgt_306_spot_trade`

### BGT-307 金样:永续 ticker(含结算周期变化)

- 规则:[行情频道 → 事件](README.md#行情频道--事件)
- 级别:golden
- 里程碑:M3
- 输入:`fixtures/bitget-usdt-futures/ticker/{v2_normal,v3_normal,interval_8h_to_4h}.in`;`interval_8h_to_4h` 为跨两次结算、周期由 8 h 变 4 h 的连续推送(采集系统 raw 中有则抽取,否则合成并标注)
- 期望文件:同目录 `<case>.expected.jsonl`(每帧 `MarkPrice`、`IndexPrice`、`Funding` 三行;`interval_s` 依次 0、28800、14400)
- 实现:`crates/venue-bitget/tests/golden.rs::bgt_307_fut_ticker`

### BGT-308 金样:永续 BBO 补帧 REST 响应

- 规则:[心跳与 BBO 探测](README.md#心跳与-bbo-探测)
- 级别:golden
- 里程碑:M3
- 输入:`fixtures/bitget-usdt-futures/rest.bbo_depth/{normal,error_code}.in`;待采集(方法 B,BGT-218)
- 期望文件:同目录 `<case>.expected.jsonl`
- 实现:`crates/venue-bitget/tests/golden.rs::bgt_308_fut_bbo_snapshot`

### BGT-309 金样:现货 BBO 补帧 REST 响应

- 规则:[心跳与 BBO 探测](README.md#心跳与-bbo-探测)
- 级别:golden
- 里程碑:M3
- 输入:`fixtures/bitget-spot/rest.bbo_depth/{normal,unknown_symbol}.in`;待采集(方法 B)
- 期望文件:同目录 `<case>.expected.jsonl`
- 实现:`crates/venue-bitget/tests/golden.rs::bgt_309_spot_bbo_snapshot`

### BGT-310 金样:登录回执

- 规则:[下单:UTA V3 WS](README.md#下单uta-v3-ws)
- 级别:golden
- 里程碑:M3
- 输入:`fixtures/bitget-usdt-futures/login/{ok,rejected}.in`(只含服务端回执,登录帧本身不入库);V2 回执来自采集系统私有流记录,V3 待采集(方法 B)
- 期望文件:同目录 `<case>.expected.jsonl`(`LoginResult` 的规范渲染)
- 实现:`crates/venue-bitget/tests/golden.rs::bgt_310_login_result`

### BGT-311 金样:订阅回执与订阅被拒 30001

- 规则:[实现说明](README.md#实现说明)
- 级别:golden
- 里程碑:M3
- 输入:`fixtures/bitget-usdt-futures/subscribe/{ok,rejected_30001,unsubscribe_ok}.in`、`fixtures/bitget-spot/subscribe/{ok,rejected_30001}.in`
- 期望文件:同目录 `<case>.expected.jsonl`(`ControlHint::SubAck` 的规范渲染)
- 实现:`crates/venue-bitget/tests/golden.rs::bgt_311_subscribe_ack`

### BGT-312 金样:心跳回应 pong

- 规则:[心跳与 BBO 探测](README.md#心跳与-bbo-探测)
- 级别:golden
- 里程碑:M3
- 输入:`fixtures/bitget-usdt-futures/pong/pong.in`(文本 `pong`)
- 期望文件:`fixtures/bitget-usdt-futures/pong/pong.expected.jsonl`(`ControlHint::Pong`,无事件)
- 实现:`crates/venue-bitget/tests/golden.rs::bgt_312_pong`

### BGT-313 金样:永续 place-order 响应

- 规则:[下单:UTA V3 WS](README.md#下单uta-v3-ws)、[结果未知码表](README.md#结果未知码表)
- 级别:golden
- 里程碑:M3
- 输入:`fixtures/bitget-usdt-futures/place-order/{ok,unregistered_code,param_error_unregistered,system_busy}.in`;`ok` 待采集(方法 B,BGT-211),错误样本按文档构造(`source = "exchange-doc"`)或合成
- 期望文件:同目录 `<case>.expected.jsonl`(错误样本期望 `OutcomeUnknown` 行,`reject_code` 按契约 8)
- 实现:`crates/venue-bitget/tests/golden.rs::bgt_313_fut_place_resp`

### BGT-314 金样:现货 place-order 响应

- 规则:[下单:UTA V3 WS](README.md#下单uta-v3-ws)、[结果未知码表](README.md#结果未知码表)
- 级别:golden
- 里程碑:M3
- 输入:`fixtures/bitget-spot/place-order/{ok,unregistered_code}.in`;待采集(方法 B)
- 期望文件:同目录 `<case>.expected.jsonl`
- 实现:`crates/venue-bitget/tests/golden.rs::bgt_314_spot_place_resp`

### BGT-315 金样:modify-order 响应

- 规则:[下单:UTA V3 WS](README.md#下单uta-v3-ws)
- 级别:golden
- 里程碑:M3
- 输入:`fixtures/bitget-usdt-futures/modify-order/{ok,unregistered_code}.in`、`fixtures/bitget-spot/modify-order/ok.in`;待采集(方法 B)
- 期望文件:同目录 `<case>.expected.jsonl`
- 实现:`crates/venue-bitget/tests/golden.rs::bgt_315_modify_resp`

### BGT-316 金样:cancel-order 响应

- 规则:[下单:UTA V3 WS](README.md#下单uta-v3-ws)
- 级别:golden
- 里程碑:M3
- 输入:`fixtures/bitget-usdt-futures/cancel-order/{ok,already_filled,unknown_order}.in`、`fixtures/bitget-spot/cancel-order/ok.in`;待采集(方法 B)
- 期望文件:同目录 `<case>.expected.jsonl`(核实前 `already_filled`、`unknown_order` 期望 `OutcomeUnknown`)
- 实现:`crates/venue-bitget/tests/golden.rs::bgt_316_cancel_resp`

### BGT-317 金样:永续订单频道全状态

- 规则:[用户流事件表](README.md#用户流事件表)、[实现说明](README.md#实现说明)
- 级别:golden
- 里程碑:M3
- 输入:`fixtures/bitget-usdt-futures/orders/{live,partially_filled,filled,canceled,ioc_expired,post_only_live,duplicate_fill,trade_id_zero}.in`;V2 私有 `orders` 来自采集系统(若有),V3 待采集(方法 B,BGT-212)
- 期望文件:同目录 `<case>.expected.jsonl`
- 实现:`crates/venue-bitget/tests/golden.rs::bgt_317_fut_orders`

### BGT-318 金样:现货订单频道全状态与扣费资产

- 规则:[用户流事件表](README.md#用户流事件表)、[累计量与费用来源](README.md#累计量与费用来源)
- 级别:golden
- 里程碑:M3
- 输入:`fixtures/bitget-spot/orders/{live,partially_filled,filled,canceled,ioc_expired,fee_in_base_asset,fee_in_bgb,fee_bgb_exhausted}.in`;待采集(方法 B)
- 期望文件:同目录 `<case>.expected.jsonl`(`fee_bgb_exhausted` 期望 `fee_rule_deviation` 位)
- 实现:`crates/venue-bitget/tests/golden.rs::bgt_318_spot_orders`

### BGT-319 金样:永续强平、ADL、交割成交

- 规则:[外部成交识别](README.md#外部成交识别)
- 级别:golden
- 里程碑:M3
- 输入:`fixtures/bitget-usdt-futures/orders/{liquidation,adl,delivery,liquidation_trade_id_zero}.in`;强平待采集(方法 B,BGT-215),ADL 与交割取不到时按文档构造并标"合成"(`source = "synthetic"`)
- 期望文件:同目录 `<case>.expected.jsonl`(`ExternalFill` 行,`cause` 依次 Liquidation、Adl、Delivery)
- 实现:`crates/venue-bitget/tests/golden.rs::bgt_319_fut_external`

### BGT-320 金样:人工单成交

- 规则:[外部成交识别](README.md#外部成交识别)
- 级别:golden
- 里程碑:M3
- 输入:`fixtures/bitget-usdt-futures/orders/manual_web_fill.in`、`fixtures/bitget-spot/orders/manual_web_fill.in`;待采集(方法 B:模拟盘网页端下单)
- 期望文件:同目录 `<case>.expected.jsonl`(核实人工来源标记前期望 `ExternalFill{cause: Unknown}`)
- 实现:`crates/venue-bitget/tests/golden.rs::bgt_320_manual_fill`

### BGT-321 金样:成交频道晚到费用

- 规则:[累计量与费用来源](README.md#累计量与费用来源)
- 级别:golden
- 里程碑:M3
- 输入:`fixtures/bitget-usdt-futures/fills/fee_after_terminal.in`(订单终态推送 + 其后的成交频道推送);依赖 BGT-212 确认成交频道存在,否则按 README 删除该类别并保留编号
- 期望文件:同目录 `<case>.expected.jsonl`(`Fill` 一行 + `FeeOnly` 一行)
- 实现:`crates/venue-bitget/tests/golden.rs::bgt_321_fee_only`

### BGT-322 金样:持仓频道

- 规则:[用户流事件表](README.md#用户流事件表)
- 级别:golden
- 里程碑:M3
- 输入:`fixtures/bitget-usdt-futures/positions/{long,short,no_liq_px,unknown_symbol}.in`;待采集(方法 B)
- 期望文件:同目录 `<case>.expected.jsonl`
- 实现:`crates/venue-bitget/tests/golden.rs::bgt_322_positions`

### BGT-323 金样:永续账户频道

- 规则:[用户流事件表](README.md#用户流事件表)
- 级别:golden
- 里程碑:M3
- 输入:`fixtures/bitget-usdt-futures/account/{usdt_only,with_margin_ratio}.in`;待采集(方法 B)
- 期望文件:同目录 `<case>.expected.jsonl`(`BalanceReport` 行;`margin_data = Poll` 时无 `MarginReport` 行)
- 实现:`crates/venue-bitget/tests/golden.rs::bgt_323_fut_account`

### BGT-324 金样:现货账户频道

- 规则:[用户流事件表](README.md#用户流事件表)
- 级别:golden
- 里程碑:M3
- 输入:`fixtures/bitget-spot/account/multi_coin.in`;待采集(方法 B)
- 期望文件:同目录 `<case>.expected.jsonl`
- 实现:`crates/venue-bitget/tests/golden.rs::bgt_324_spot_account`

### BGT-325 金样:未知用户流事件

- 规则:[用户流事件表](README.md#用户流事件表)
- 级别:golden
- 里程碑:M3
- 输入:`fixtures/bitget-usdt-futures/unknown/{v3_unknown_topic,v2_unknown_channel}.in`、`fixtures/bitget-spot/unknown/v3_unknown_topic.in`(合成)
- 期望文件:同目录 `<case>.expected.jsonl`(计数行,不是错误行)
- 实现:`crates/venue-bitget/tests/golden.rs::bgt_325_unknown_events`

### BGT-326 金样:账户设置查询

- 规则:[账户配置核验](README.md#账户配置核验)
- 级别:golden
- 里程碑:M3
- 输入:`fixtures/bitget-usdt-futures/rest.account_settings/{basic_oneway,advanced,isolated_mode,hedge_mode,classic}.in`;待采集(方法 B,BGT-214)
- 期望文件:同目录 `<case>.expected.jsonl`(配合其余步的基准响应得到的 `AccountConfigReport`)
- 实现:`crates/venue-bitget/tests/golden.rs::bgt_326_account_settings`

### BGT-327 金样:每品种保证金模式(持仓列表)

- 规则:[账户配置核验](README.md#账户配置核验)
- 级别:golden
- 里程碑:M3
- 输入:`fixtures/bitget-usdt-futures/rest.margin_mode/{all_cross,one_isolated,all_isolated}.in`;待采集(方法 B)
- 期望文件:同目录 `<case>.expected.jsonl`
- 实现:`crates/venue-bitget/tests/golden.rs::bgt_327_margin_mode`

### BGT-328 金样:账户信息(UID 与 key 权限)

- 规则:[账户配置核验](README.md#账户配置核验)
- 级别:golden
- 里程碑:M3
- 输入:`fixtures/bitget-usdt-futures/rest.account_info/{ok,withdraw_perm,no_trade_perm,uid_mismatch}.in`、`fixtures/bitget-spot/rest.account_info/ok.in`;待采集(方法 B;UID 清洗为 `acct-0001`)
- 期望文件:同目录 `<case>.expected.jsonl`
- 实现:`crates/venue-bitget/tests/golden.rs::bgt_328_account_info`

### BGT-329 金样:账户资产与当前持仓(保证金轮询)

- 规则:[保证金数据换算](README.md#保证金数据换算)
- 级别:golden
- 里程碑:M3
- 输入:`fixtures/bitget-usdt-futures/rest.account_assets/ok.in` + `fixtures/bitget-usdt-futures/rest.positions/{two_positions,empty}.in`;待采集(方法 B)
- 期望文件:同目录 `<case>.expected.jsonl`(`MarginReport` 与 `PositionReport` 行)
- 实现:`crates/venue-bitget/tests/golden.rs::bgt_329_margin_poll`

### BGT-330 金样:BGB 抵扣开关

- 规则:[账户配置核验](README.md#账户配置核验)
- 级别:golden
- 里程碑:M3
- 输入:`fixtures/bitget-spot/rest.bgb_deduct/{on,off}.in`;待采集(方法 B)
- 期望文件:同目录 `<case>.expected.jsonl`
- 实现:`crates/venue-bitget/tests/golden.rs::bgt_330_bgb_deduct`

### BGT-331 金样:现货借币开关

- 规则:[账户配置核验](README.md#账户配置核验)
- 级别:golden
- 里程碑:M3
- 输入:`fixtures/bitget-spot/rest.spot_borrow/{off,on,missing_field}.in`;待采集(方法 B)
- 期望文件:同目录 `<case>.expected.jsonl`(`on` → `AUTO_BORROW`,`missing_field` → `UNREADABLE`)
- 实现:`crates/venue-bitget/tests/golden.rs::bgt_331_spot_borrow`

### BGT-332 金样:每品种杠杆

- 规则:[账户配置核验](README.md#账户配置核验)
- 级别:golden
- 里程碑:M3
- 输入:`fixtures/bitget-usdt-futures/rest.leverage/{two_insts,one_missing}.in`;待采集(方法 B)
- 期望文件:同目录 `<case>.expected.jsonl`(`LeverageItem` 行;缺失的 inst 为 0)
- 实现:`crates/venue-bitget/tests/golden.rs::bgt_332_leverage`

### BGT-333 金样:REST 查单响应

- 规则:[下单:UTA V3 WS](README.md#下单uta-v3-ws)
- 级别:golden
- 里程碑:M3
- 输入:`fixtures/bitget-usdt-futures/rest.order_detail/{filled,live,canceled_partial,not_found}.in`、`fixtures/bitget-spot/rest.order_detail/filled.in`;待采集(方法 B)
- 期望文件:同目录 `<case>.expected.jsonl`(`QueryResult` 行,`cum_filled` 与 `avg_px` 非 0 的成对出现)
- 实现:`crates/venue-bitget/tests/golden.rs::bgt_333_order_detail`

### BGT-334 金样:UTA 未成交订单(只读查询)

- 规则:[只读查询](README.md#只读查询)
- 级别:golden
- 里程碑:M5
- 输入:`fixtures/bitget-usdt-futures/rest.open_orders/two_orders_one_foreign.in`、`fixtures/bitget-spot/rest.open_orders/two_orders_one_foreign.in`;来源:待采集(方法 A + B)
- 期望文件:同目录 `<case>.expected.jsonl`(`open_order` 行)
- 实现:`crates/venue-bitget/tests/golden.rs::bgt_334_ro_open_orders`
- 备注:依赖待核实:开发备忘第 18 条

### BGT-335 金样:UTA 成交查询(只读查询)

- 规则:[只读查询](README.md#只读查询)
- 级别:golden
- 里程碑:M5
- 输入:`fixtures/bitget-usdt-futures/rest.fills/page.in`、`fixtures/bitget-spot/rest.fills/page.in`;来源:待采集(方法 A + B)
- 期望文件:同目录 `<case>.expected.jsonl`(`fill` 行与 `RoPage`)
- 实现:`crates/venue-bitget/tests/golden.rs::bgt_335_ro_fills`
- 备注:依赖待核实:开发备忘第 18 条

### BGT-336 金样:UTA 资金费账单(永续)(只读查询)

- 规则:[只读查询](README.md#只读查询)
- 级别:golden
- 里程碑:M5
- 输入:`fixtures/bitget-usdt-futures/rest.bills/funding.in`;来源:待采集(方法 A + B)
- 期望文件:同目录 `<case>.expected.jsonl`(`funding` 行)
- 实现:`crates/venue-bitget/tests/golden.rs::bgt_336_ro_funding`
- 备注:依赖待核实:开发备忘第 18 条

BGT-336 补充单元验收:签名 trading-account financial-records 查询、IN/OUT 实额、忽略交易费、cursor 续页及循环拒绝,失败无部分输出。实现:`tests/funding.rs::bgt_336_funding_bills_are_cash_not_ticker_or_wallet`。

### BGT-337 金样:UTA 资产(只读查询)

- 规则:[只读查询](README.md#只读查询)
- 级别:golden
- 里程碑:M5
- 输入:`fixtures/bitget-usdt-futures/rest.assets/ro.in`、`fixtures/bitget-spot/rest.assets/ro.in`;来源:待采集(方法 A + B)
- 期望文件:同目录 `<case>.expected.jsonl`(`balance` 行)
- 实现:`crates/venue-bitget/tests/golden.rs::bgt_337_ro_balance`
- 备注:依赖待核实:开发备忘第 18 条

### BGT-338 金样:UTA 当前仓位(永续)(只读查询)

- 规则:[只读查询](README.md#只读查询)
- 级别:golden
- 里程碑:M5
- 输入:`fixtures/bitget-usdt-futures/rest.positions/ro.in`;来源:待采集(方法 A + B)
- 期望文件:同目录 `<case>.expected.jsonl`(`position` 行)
- 实现:`crates/venue-bitget/tests/golden.rs::bgt_338_ro_positions`
- 备注:依赖待核实:开发备忘第 18 条

### BGT-400 fuzz:bitget-usdt-futures(计入门槛 ③)

- 规则:[契约](../hft-venue-proto/README.md#实现方必须遵守的契约)、[实现说明](README.md#实现说明)
- 级别:fuzz
- 里程碑:M3
- 门槛:③
- 目标:`bitget-usdt-futures`:任意字节按首字节分派到行情(`classify` → `peek` → `decode_md`)、BBO 补帧(`decode_bbo_snapshot`)、回报(`classify_report` → `decode_report`)与账户查询(`acct_decode`,步号取输入第 2 字节)解码,入口签名与实盘相同
- 语料:`cargo xtask fixtures corpus bitget-usdt-futures` 由 BGT-300–BGT-333 的永续 `.in` 生成 + 变异;`fixtures/bitget-usdt-futures/regressions/`
- 判定:零 panic、零越界、零单输入超时(口径见 [testing §3.3](../../docs/testing.md#33-fuzz))
- 实现:`crates/venue-bitget/fuzz/fuzz_targets/bitget_usdt_futures.rs::bgt_400_fuzz_fut`

### BGT-401 fuzz:bitget-spot(计入门槛 ③)

- 规则:[契约](../hft-venue-proto/README.md#实现方必须遵守的契约)、[实现说明](README.md#实现说明)
- 级别:fuzz
- 里程碑:M3
- 门槛:③
- 目标:`bitget-spot`:同 BGT-400,产品线为现货
- 语料:`cargo xtask fixtures corpus bitget-spot` + 变异;`fixtures/bitget-spot/regressions/`
- 判定:零 panic、零越界、零单输入超时(口径见 [testing §3.3](../../docs/testing.md#33-fuzz))
- 实现:`crates/venue-bitget/fuzz/fuzz_targets/bitget_spot.rs::bgt_401_fuzz_spot`

### BGT-402 fuzz:编码往返(不计入门槛 ③)

- 规则:[下单:UTA V3 WS](README.md#下单uta-v3-ws)
- 级别:fuzz
- 里程碑:M3
- 目标:`fuzz_bitget_encode_roundtrip`:任意字节构造 `GwCmd`(两个产品线、全部订单类 `op`)→ `encode`;成功时帧经 `sonic-rs` 解析,请求 id、`clientOid`、价量与输入一致(BGT-075 的性质);失败时只能是 `EncodeErr`
- 语料:BGT-064–BGT-071 的命令序列化 + 变异
- 判定:零 panic、零单输入超时(口径见 [testing §3.3](../../docs/testing.md#33-fuzz));往返不一致按崩溃处理
- 实现:`crates/venue-bitget/fuzz/fuzz_targets/encode_roundtrip.rs::bgt_402_fuzz_encode_roundtrip`

### BGT-500 基准:classify

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:bench
- 里程碑:M3
- 基准名:`bench_classify_bitget`
- 实现:`crates/venue-bitget/benches/md.rs::bgt_500_classify`

### BGT-501 基准:books1 解码为 Bbo

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:bench
- 里程碑:M3
- 基准名:`bench_decode_bitget_books1`、`bench_decode_bitget_books1/spot`
- 实现:`crates/venue-bitget/benches/md.rs::bgt_501_decode_books1`

### BGT-502 基准:books15 解码为 TopN 快照

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:bench
- 里程碑:M3
- 基准名:`bench_decode_bitget_books15`
- 实现:`crates/venue-bitget/benches/md.rs::bgt_502_decode_books15`

### BGT-503 基准:place-order 编码

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:bench
- 里程碑:M3
- 基准名:`bench_encode_bitget_place`
- 实现:`crates/venue-bitget/benches/reports.rs::bgt_503_encode_place`

### BGT-504 基准:订单推送解码(含外部成交判定与去重)

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:bench
- 里程碑:M3
- 基准名:`bench_decode_bitget_order`
- 实现:`crates/venue-bitget/benches/reports.rs::bgt_504_decode_order`

### BGT-505 基准:持仓列表 50 KiB 账户查询解码

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:bench
- 里程碑:M3
- 基准名:`bench_acct_decode_bitget`
- 实现:`crates/venue-bitget/benches/acct.rs::bgt_505_acct_decode_positions`

## 验收

- **实现前置**:开发备忘第 1–6 条(BGT-210–BGT-215)核实完成并写回 README 之前,不开始 `orders.rs`、`reports.rs`、`acct.rs`;M3 之前必须核实的第 2、12 条(BGT-211、BGT-216)与模拟盘第 9、17 条(BGT-210、BGT-217)在 M3 结束前通过。每条结论写回 README 的同一提交里改写备注中引用该条目号的用例(`备注:依赖待核实`),删除的用例保留编号。核实完成前本 crate 不能通过上线门槛。
- **单元、表驱动与属性**:里程碑 M3 的全部用例有同编号实现且通过(门槛 ①);属性测试每夜大例数运行:BGT-012、BGT-026、BGT-068、BGT-075、BGT-081、BGT-148 各 100 000 例。
- **契约检查**:BGT-174 每一行两个产品线都通过。
- **金样**(门槛 ②,覆盖清单归 [fixtures](../../fixtures/README.md#测试与验收)):BGT-300–BGT-333 全绿;金样不对修 fixture 来源,不改归一化去迁就([testing §1](../../docs/testing.md#1-原则))。
- **fuzz**(门槛 ③):BGT-400、BGT-401 按 [testing §8](../../docs/testing.md#8-上线门槛) 的累计与清零规则计数;BGT-402 不计入 ③,失败同样必须修复。
- **基准**:BGT-500–BGT-505 的参考机结果不宽于 README"延迟与资源契约"表(上限登记在 [benchmarks](../../benchmarks/README.md));共享 CI runner 上只报告。
- **心跳回归**:BGT-191(持续推送数据时 10 min 不被服务端断开)与反例 BGT-192 通过;同一回归在真实运行时上的版本归 hft-venue。
- **模拟盘联调**(门槛 ⑥):BGT-220 连续 3 天无未解释异常,行情与下单都用模拟盘;BGT-219 的 `books1` 与 V2 基线对比报告完成。
- **可选扩展**:增量深度 `books`(含 `pseq = 0` 重置金样与 `bench_decode_bitget_books`)、`mock` 反向编解码、`batch-place`/`batch-modify` 批量帧在启用时同一改动补写用例并通过。

BGT-201 同时核对私有频道位图与逐频道 subscribe/error/unsubscribe 回执识别。
