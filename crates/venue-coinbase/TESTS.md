---
component: venue-coinbase
prefix: CBX
spec: README.md
---

# venue-coinbase 测试规格

格式与规则见 [docs/tests](../../docs/tests/README.md)。

本文件的记号:

- `RD-CB` = 测试内构建的 refdata(`tests/support/mod.rs`):`USD` = AssetId 1(Stable),`BTC` = AssetId 2(px_exp −4,qty_exp −8);`COINBASE_SPOT`(16)的 `BTC-USD` = InstId 1(tick 0.01、lot 1e-8);`BINANCE_SPOT`(2)上另有一个符号同为 `BTC-USD` 的 InstId 2(外来 inst)。
- `RD-CB` 另有 `COINBASE_ADV`(18)的 `BIP-20DEC30-CDE` = InstId 3(永续,tick 5,`qty_unit_shift` −2:线上数量是 0.01 BTC 的合约张数,`qty_wire_exp` −6)。
- `RX` = 固定接收元数据(`conn` 3、`rx_tsc` 0x12345678、`sw_rx_ns`、`rx_empty_gap_ns` 77)。
- `TICKER`/`MATCH` = 2026-11-06 eu-west-1 实盘录制(`fixtures/coinbase-spot/`)中的一条 `ticker` 与一条 `match`;数值写法:价格 `"75750.28"` = 757502800(px_exp −4),数量 `"0.06203374"` = 6203374(qty_exp −8)。

## 覆盖

| README 小节 | 用例 |
|---|---|
| 概要 | CBX-001, CBX-015 |
| 目录 | 不适用:非规范 |
| 使用本组件 | CBX-001, CBX-002 |
| 端点与路由 | CBX-002, CBX-003 |
| 订阅 | CBX-002, CBX-004, CBX-005, CBX-012, CBX-013 |
| 行情频道 → 事件 | CBX-006, CBX-008, CBX-010, CBX-011, CBX-012, CBX-017 |
| 多连接去重 | CBX-014 |
| 时间戳 | CBX-007 |
| 心跳与 BBO 探测 | CBX-005, CBX-009 |
| 限速默认值 | CBX-009 |
| VenueCaps 默认值 | CBX-009 |
| 账户 | CBX-015 |
| Advanced Trade 衍生品行情(coinbase-adv) | CBX-019, CBX-020, CBX-021, CBX-022, CBX-023, CBX-024, CBX-025 |
| 接入清单中不适用的步骤 | 不适用:说明 |
| 实现说明 | CBX-006, CBX-008, CBX-016, CBX-018 |
| 延迟与资源契约 | 不适用:未建基准(README 已知限制) |
| 测试与验收 | 不适用:非规范 |
| 依赖 | 不适用:由 `cargo xtask deps` 机检 |
| 延伸阅读 | 不适用:非规范 |
| 已知限制与后续工作 | 不适用:范围外 |

## 用例

### CBX-001 分派

- 规则:[行情频道 → 事件](README.md#行情频道--事件)
- 级别:table
- 里程碑:M3
- 前置:两条路由各调用一次,结果相同
- 实现:`crates/venue-coinbase/tests/main.rs::cbx_001_classify`

| 行 | 输入 | 期望 |
|---|---|---|
| CBX-001.1 | `TICKER` | `Ticker` |
| CBX-001.2 | `MATCH` | `Match` |
| CBX-001.3 | `{"type":"last_match",…` | `LastMatch` |
| CBX-001.4 | `{"type":"heartbeat",…` | `Heartbeat` |
| CBX-001.5 | `{"type":"subscriptions",…` | `Subscriptions` |
| CBX-001.6 | `{"type":"error",…` | `Error` |
| CBX-001.7 | `{"type":"l2update",…` | `Unknown` |
| CBX-001.8 | `TICKER`,二进制帧 | `Unknown` |
| CBX-001.9 | 首字节为空格 | `Unknown` |

### CBX-002 路由、流与序号规则

- 规则:[端点与路由](README.md#端点与路由)、[订阅](README.md#订阅)
- 级别:table
- 里程碑:M3
- 前置:无
- 实现:`crates/venue-coinbase/tests/main.rs::cbx_002_routes_and_seq_rules`

| 行 | 检查 | 期望 |
|---|---|---|
| CBX-002.1 | `routes()` | 两条:`(1, Bbo, race)`、`(2, Md, 不赛马)`,`topn_levels` 皆空 |
| CBX-002.2 | `stream_route` | Bbo → 1、Trade → 2,其余流 `None` |
| CBX-002.3 | `seq_rule` | `Ticker`/`Match`/`LastMatch` → `Monotonic`,其余 `None` |
| CBX-002.4 | `bbo_snapshot_request` | `Ok(false)` |

### CBX-003 端点与登录

- 规则:[端点与路由](README.md#端点与路由)、[账户](README.md#账户)
- 级别:table
- 里程碑:M3
- 前置:`CFG` = Prod、`stale_guard_ms` 1 500
- 实现:`crates/venue-coinbase/tests/main.rs::cbx_003_endpoint_and_login`

| 行 | 输入 | 期望 |
|---|---|---|
| CBX-003.1 | `Md` 角色,两条路由 | 路径 `/`,无额外头 |
| CBX-003.2 | 路由 3;`Order`/`UserStream`/`Rest` 角色 | `Unsupported` |
| CBX-003.3 | `login(Md)`;`login(Order)`;`login_result(TICKER)`;`bootstrap` | `NotNeeded`;`Unsupported`;`NotMine`;`None` |

### CBX-004 订阅帧

- 规则:[订阅](README.md#订阅)
- 级别:table
- 里程碑:M3
- 实现:`crates/venue-coinbase/tests/main.rs::cbx_004_subscribe_frames`

| 行 | 输入 | 期望 |
|---|---|---|
| CBX-004.1 | BTC-USD Bbo,订阅 | `{"type":"subscribe","product_ids":["BTC-USD"],"channels":["ticker","heartbeat"]}`,覆盖 1 |
| CBX-004.2 | BTC-USD Trade,订阅 | 同上,频道 `"matches","heartbeat"` |
| CBX-004.3 | BTC-USD Bbo,退订 | `{"type":"unsubscribe","product_ids":["BTC-USD"],"channels":["ticker"]}` |
| CBX-004.4 | Bbo 后接 Trade | 一帧只覆盖 1 条 |
| CBX-004.5 | Mark 流 | `Unsupported` |
| CBX-004.6 | 外来 inst;inst 0 | `Cfg` |
| CBX-004.7 | 空列表 | `Cfg` |
| CBX-004.8 | `SUB_ACKS` | `true` |

### CBX-005 控制提示

- 规则:[订阅](README.md#订阅)、[心跳与 BBO 探测](README.md#心跳与-bbo-探测)
- 级别:table
- 里程碑:M3
- 实现:`crates/venue-coinbase/tests/main.rs::cbx_005_control_hints`

| 行 | 输入 | 期望 |
|---|---|---|
| CBX-005.1 | `subscriptions`,`channels` 非空 | `SubAck{ok: true, code: 0}` |
| CBX-005.2 | `subscriptions`,`channels` 为 `[]`(含空白) | `None` |
| CBX-005.3 | `error` | `SubAck{ok: false, code: 1}` |
| CBX-005.4 | `heartbeat` | `Pong` |
| CBX-005.5 | `TICKER`、`MATCH` | `None` |

### CBX-006 `peek`

- 规则:[行情频道 → 事件](README.md#行情频道--事件)、[实现说明](README.md#实现说明)
- 级别:table
- 里程碑:M3
- 实现:`crates/venue-coinbase/tests/main.rs::cbx_006_peek`

| 行 | 输入 | 期望 |
|---|---|---|
| CBX-006.1 | `TICKER` | inst 1、Bbo、`last_id` = 138 933 168 161、`prev_id` 0 |
| CBX-006.2 | `MATCH` | inst 1、Trade、`last_id` = 1 116 405 767 |
| CBX-006.3 | `MATCH` 改为 `last_match` | 同上 |
| CBX-006.4 | 产品 `ETH-USD` | inst 0 |
| CBX-006.5 | 价格改成 `"x"` | 与 .1 相同(不解析价格) |
| CBX-006.6 | 字段乱序的 ticker | 与 .1 相同(走结构体解码) |
| CBX-006.7 | 控制类与 `Unknown` | `UnknownClass` |

### CBX-007 交易所时间

- 规则:[时间戳](README.md#时间戳)
- 级别:table
- 里程碑:M3
- 实现:`crates/venue-coinbase/tests/main.rs::cbx_007_time`

| 行 | 输入 | 期望 |
|---|---|---|
| CBX-007.1 | 微秒、无小数、1 位小数、9 位小数、纪元、闰日 | 对应 Unix ns |
| CBX-007.2 | 缺 `Z`、空格分隔、13 月、非闰年 2-29、24 时、`.` 后无数字、10 位小数、尾随字符、1969 年、空串 | `Number` |

### CBX-008 解码错误不产出部分事件

- 规则:[行情频道 → 事件](README.md#行情频道--事件)、[实现说明](README.md#实现说明)
- 级别:table
- 里程碑:M3
- 实现:`crates/venue-coinbase/tests/main.rs::cbx_008_decode_errors`

| 行 | 输入 | 期望 |
|---|---|---|
| CBX-008.1 | `side` = `x` | `Shape`,无事件 |
| CBX-008.2 | `best_ask` = `7a` | `Number`,无事件 |
| CBX-008.3 | 数量 9 位小数(多于 qty 指数) | `Number`,无事件 |
| CBX-008.4 | `time` 无小数也无 `Z` | `Number`,无事件 |
| CBX-008.5 | 截断的 ticker | `Truncated`,无事件 |
| CBX-008.6 | `sequence` 为字符串 | `Shape`,无事件 |
| CBX-008.7 | 未知产品 | `Ok`,0 个事件 |
| CBX-008.8 | 控制类与 `Unknown` 调 `decode_md` | `UnknownClass` |

### CBX-009 契约检查与默认值

- 规则:[VenueCaps 默认值](README.md#venuecaps-默认值)、[限速默认值](README.md#限速默认值)、[心跳与 BBO 探测](README.md#心跳与-bbo-探测)
- 级别:table
- 里程碑:M3
- 前置:hft-venue-proto `testkit::check_default_caps`、`check_md_basics`(`TICKER` 于路由 1、`MATCH` 于路由 2)
- 实现:`crates/venue-coinbase/tests/main.rs::cbx_009_contracts_and_defaults`

| 行 | 检查 | 期望 |
|---|---|---|
| CBX-009.1 | 两个检查;`flags`;`stale_guard` | 通过;0;`None` |
| CBX-009.2 | `heartbeat(Md)`;`bbo_probe` | `(5 000, false, 5 000, 0)`;`WsPing` |
| CBX-009.3 | `rate_rules` | `Ip/WsInMsgs 1 000/8`、`Conn/WsInMsgs 1 000/10`、`Ip/NewConns 1 000/8` |

### CBX-010 金样:ticker

- 规则:[行情频道 → 事件](README.md#行情频道--事件)
- 级别:golden
- 里程碑:M3
- 前置:`fixtures/coinbase-spot/ticker/{normal,initial_on_subscribe}`
- 输入:`fixtures/coinbase-spot/ticker/{normal,initial_on_subscribe}.in`
- 期望文件:同名 `.expected.jsonl`
- 实现:`crates/venue-coinbase/tests/main.rs::cbx_010_golden_ticker`

期望:每个样本一条 `Bbo`,字段等于 `.expected.jsonl`;`MarketHdr` 的 `conn`、`rx_tsc`、`sw_rx_ns`、`rx_empty_gap_ns` 等于 `RX`。

### CBX-011 金样:match

- 规则:[行情频道 → 事件](README.md#行情频道--事件)
- 级别:golden
- 里程碑:M3
- 前置:`fixtures/coinbase-spot/match/{taker_buy,taker_sell}`(`side` `sell` / `buy`)
- 输入:`fixtures/coinbase-spot/match/{taker_buy,taker_sell}.in`
- 期望文件:同名 `.expected.jsonl`
- 实现:`crates/venue-coinbase/tests/main.rs::cbx_011_golden_match`

期望:一条 `Trade`,`taker_side` 与 `side` 相反,其余字段等于 `.expected.jsonl`。

### CBX-012 金样:last_match

- 规则:[订阅](README.md#订阅)
- 级别:golden
- 里程碑:M3
- 前置:`fixtures/coinbase-spot/last_match/on_subscribe`
- 输入:`fixtures/coinbase-spot/last_match/on_subscribe.in`
- 期望文件:`on_subscribe.expected.jsonl`(空)
- 实现:`crates/venue-coinbase/tests/main.rs::cbx_012_golden_last_match`

期望:类 `LastMatch`;`decode_md` 返回 0 个事件、inst 1。

### CBX-013 金样:控制帧

- 规则:[订阅](README.md#订阅)、[心跳与 BBO 探测](README.md#心跳与-bbo-探测)
- 级别:golden
- 里程碑:M3
- 前置:`fixtures/coinbase-spot/` 的 `heartbeat/normal`、`subscriptions/{ticker_heartbeat,empty_after_error}`、`error/{level2_needs_auth,unknown_channel}`
- 输入:`fixtures/coinbase-spot/{heartbeat,subscriptions,error}/*.in`
- 期望文件:同名 `.expected.jsonl`(控制提示)
- 实现:`crates/venue-coinbase/tests/main.rs::cbx_013_golden_control`

期望:控制提示等于 `.expected.jsonl`(空订阅应答无提示);`peek` 全部 `UnknownClass`。

### CBX-014 两条 ticker 连接的赛马回放

- 规则:[多连接去重](README.md#多连接去重)
- 级别:golden
- 里程碑:M3
- 前置:`fixtures/coinbase-spot/race/ticker_two_conns.jsonl`(两条同主机连接按接收顺序的 400 帧);按 hft-venue `Monotonic` 规则(`last_id ≤ last_fwd` 丢弃)逐帧判定
- 输入:`fixtures/coinbase-spot/race/ticker_two_conns.jsonl`
- 期望文件:`ticker_two_conns.expected.jsonl`
- 实现:`crates/venue-coinbase/tests/main.rs::cbx_014_race_two_connections`

| 行 | 检查 | 期望 |
|---|---|---|
| CBX-014.1 | 转发的 `update_id` 序列 | 等于两连接 `sequence` 并集的升序(`.expected.jsonl`),无重复、无缺失 |
| CBX-014.2 | 控制帧 | 2 条 `SubAck{ok}`;有 `Pong`;有被丢弃的重复帧 |
| CBX-014.3 | 首达连接 | 两条连接都有先到的帧 |

### CBX-015 只读:订单与账户方法的拒绝形式

- 规则:[账户](README.md#账户)、[概要](README.md#概要)
- 级别:table
- 里程碑:M3
- 前置:默认 `Credential`,账户 1
- 实现:`crates/venue-coinbase/tests/main.rs::cbx_015_read_only_forms`

| 行 | 调用 | 期望 |
|---|---|---|
| CBX-015.1 | `build_templates` | `Unsupported` |
| CBX-015.2 | `private_subscribe` | `Unsupported` |
| CBX-015.3 | `session_identity_request` | `Unsupported` |
| CBX-015.4 | `account_request`;`account_decode` | `Unsupported`;`UnknownClass` |
| CBX-015.5 | `dead_man` | `None` |
| CBX-015.6 | `render_cid`;`parse_cid` | 0;`None` |
| CBX-015.7 | `classify_report`;`decode_report` | `Unknown`;`UnknownClass` |

### CBX-016 解码器截断冒烟

- 规则:[实现说明](README.md#实现说明)
- 级别:property
- 里程碑:M3
- 前置:fuzz 入口 `fuzz/exercise.rs::run`(两条路由的 classify、control、peek、decode_md,classify_report/decode_report,`iso8601_ns`)
- 性质:任意前缀与任意字节串不 panic,产出事件带 `RX`
- 生成:四个金样的全部前缀 + 256 个 31 字节重复串
- 参考:无(只断言不 panic 与 `MarketHdr`)
- 例数:全部前缀 + 256
- 实现:`crates/venue-coinbase/fuzz/exercise.rs::cbx_016_decoder_truncation_smoke`

期望:ticker、match、last_match、heartbeat 金样的每个前缀与 256 个单字节重复串都不 panic;产出的每个事件带 `RX` 的 `rx_tsc` 与 `conn`。

### CBX-017 一条 matches 连接的成交序列

- 规则:[行情频道 → 事件](README.md#行情频道--事件)、[多连接去重](README.md#多连接去重)
- 级别:golden
- 里程碑:M3
- 前置:`fixtures/coinbase-spot/race/matches_one_conn.jsonl`(同一录制的 120 帧)
- 输入:`fixtures/coinbase-spot/race/matches_one_conn.jsonl`
- 期望文件:无(断言写在实现里)
- 实现:`crates/venue-coinbase/tests/main.rs::cbx_017_matches_run`

| 行 | 检查 | 期望 |
|---|---|---|
| CBX-017.1 | `peek` 的 `trade_id` | 严格递增 |
| CBX-017.2 | 每条 `match` | 一条 `Trade`,`trade_id` 等于 `peek`;多于 100 笔 |
| CBX-017.3 | 其余数据帧 | 只有 `last_match`,0 个事件 |

### CBX-018 fuzz 目标 `coinbase-spot`

- 规则:[实现说明](README.md#实现说明)
- 级别:fuzz
- 里程碑:M3
- 前置:`cargo fuzz run coinbase-spot`(`crates/venue-coinbase/fuzz`,入口同 CBX-016);语料 = `fixtures/coinbase-spot/` 的 `.in`
- 目标:`coinbase-spot`
- 语料:`fixtures/coinbase-spot/` 的 `.in`
- 判定:无 panic
- 实现:`crates/venue-coinbase/fuzz/fuzz_targets/coinbase_spot.rs::cbx_018_fuzz_coinbase_spot`

期望:无 panic。

### CBX-019 `coinbase-adv` 的分类

- 规则:[Advanced Trade 衍生品行情(coinbase-adv)](README.md#advanced-trade-衍生品行情coinbase-adv)
- 级别:table
- 里程碑:M3
- 前置:`UPDATE`、`HEARTBEAT`、`SUBS`、`ERROR` = 2026-11-11 eu-west-1 录制的各一帧
- 实现:`crates/venue-coinbase/tests/adv.rs::cbx_019_adv_classify`

| 行 | 输入(前 64 B) | 期望 |
|---|---|---|
| CBX-019.1 | `l2_data` 帧 | `L2` |
| CBX-019.2 | `heartbeats` 帧 | `Heartbeat` |
| CBX-019.3 | `subscriptions` 帧 | `Subscriptions` |
| CBX-019.4 | `{"type":"error",…}` | `Error` |
| CBX-019.5 | `{"channel":"ticker",…}` | `Unknown` |
| CBX-019.6 | `l2_data` 帧,二进制帧 | `Unknown` |
| CBX-019.7 | `channel` 不在最前 | `Unknown` |
| CBX-019.8 | 空 | `Unknown` |

### CBX-020 `coinbase-adv` 的路由、端点、订阅帧与保活帧

- 规则:[Advanced Trade 衍生品行情(coinbase-adv)](README.md#advanced-trade-衍生品行情coinbase-adv)
- 级别:table
- 里程碑:M3
- 前置:`RD-CB`(`COINBASE_ADV` 的 `BIP-20DEC30-CDE` = InstId 3)
- 实现:`crates/venue-coinbase/tests/adv.rs::cbx_020_adv_route_frames_and_keepalive`

| 行 | 检查 | 期望 |
|---|---|---|
| CBX-020.1 | 路由;`stream_route`;`seq_rule` | 一条 `Bbo` 路由 `level2`、赛马;只有 `Bbo` 有路由;各类都是 `SeqRule::None` |
| CBX-020.2 | `endpoint`(Md / Order);`login` | 路径 `/` / `Unsupported`;`NotNeeded` |
| CBX-020.3 | `subscribe`:`BIP` 的 `Bbo`;退订;`Trade`;别的 venue 的 inst;空 | `{"type":"subscribe","product_ids":["BIP-20DEC30-CDE"],"channel":"level2"}` 覆盖 1 项;`unsubscribe` 同形;`Unsupported`;`Cfg`;`Cfg` |
| CBX-020.4 | `bbo_probe`;`app_ping`;8 字节的缓冲 | `AppJson`;`{"type":"subscribe","channel":"heartbeats"}`;0 |

### CBX-021 `coinbase-adv` 的控制帧与 peek

- 规则:[Advanced Trade 衍生品行情(coinbase-adv)](README.md#advanced-trade-衍生品行情coinbase-adv)
- 级别:table
- 里程碑:M3
- 前置:同 CBX-019
- 实现:`crates/venue-coinbase/tests/adv.rs::cbx_021_adv_control_hints`

| 行 | 输入 | 期望 |
|---|---|---|
| CBX-021.1 | 订阅 `level2` 的应答 | `SubAck{ok: true}` |
| CBX-021.2 | 保活帧的应答(列出 `heartbeats` 与 `level2`) | `SubAck{ok: true}` |
| CBX-021.3 | `heartbeats` 帧 | `Pong` |
| CBX-021.4 | `error` 帧 | `SubAck{ok: false, code: 1}` |
| CBX-021.5 | `l2_data` 帧 | 无 |
| CBX-021.6 | `peek`:`l2_data` 帧;没有 inst 的产品;`Heartbeat` 类 | `BIP`、`Bbo`、`last_id` 0;InstId 0;`UnknownClass` |

### CBX-022 level2 簿 → `Bbo`,两条连接的合并

- 规则:[Advanced Trade 衍生品行情(coinbase-adv)](README.md#advanced-trade-衍生品行情coinbase-adv)
- 级别:table
- 里程碑:M3
- 前置:`RD-CB`;合成的 `l2_data` 帧(一个事件;档的时间 = 2026-11-11T06:00:SS);数值:价格 × 10⁴,数量 × 10⁶;每行依次作用在同一个 `AdvFeedCtx` 上(.7 起各用新的)
- 实现:`crates/venue-coinbase/tests/adv.rs::cbx_022_adv_book_to_bbo`

| 行 | 输入 | 期望 |
|---|---|---|
| CBX-022.1 | 连接 1:快照之前的 `update` | 无事件 |
| CBX-022.2 | 连接 1:快照(买 73755 × 6、73750 × 609、5 × 1;卖 73765 × 12、73770 × 40、100000000 × 5) | 一个 `Bbo`:73755 × 6 / 73765 × 12,交易所时间 = 档的时间 |
| CBX-022.3 | 买 73750 的数量改 700;卖 73765 的数量改 9 | 无;一个 `Bbo`(卖量 9) |
| CBX-022.4 | 同一帧:删买 73755(04.1)、加卖 73760 × 3(04.2) | 一个 `Bbo`:73750 × 700 / 73760 × 3,时间 04.2 |
| CBX-022.5 | 连接 2:快照 = 已转发的状态;同一时间(04.2)把卖量改 4;04.3 改 5 | 无;无(不晚于上一个已转发的);一个 `Bbo`(连接 2,04.3) |
| CBX-022.6 | 连接 1:04.3 卖量 5(与已转发的相同);04.4 加买 73755 × 2 | 无;一个 `Bbo`(连接 1) |
| CBX-022.7 | 新簿:只有买的快照;加卖 100(与买同价);删卖 100、加卖 105 × 2 | 无;无(交叉);一个 `Bbo` 100 × 1 / 105 × 2 |
| CBX-022.8 | 没有 inst 的产品的快照;该连接新的快照(90 / 95) | 无;一个 `Bbo`(簿被替换) |
| CBX-022.9 | 卖方 `ADV_LEVELS` + 5 档的快照;再把保留的 `ADV_LEVELS` 档全部删掉 | 一个 `Bbo`(最优 1000);无(该边最优未知) |
| CBX-022.10 | `ADV_BOOKS` + 1 条连接各发快照;连接 1 的 `update`;最后一条连接的 `update` | 连接 1 的簿已被回收:忽略;一个 `Bbo` |

### CBX-023 `l2_data` 的解码错误

- 规则:[Advanced Trade 衍生品行情(coinbase-adv)](README.md#advanced-trade-衍生品行情coinbase-adv)
- 级别:table
- 里程碑:M3
- 前置:`RD-CB`;参照帧 = 两档(买 10、卖 20)的快照
- 实现:`crates/venue-coinbase/tests/adv.rs::cbx_023_adv_decode_errors`

| 行 | 输入 | 期望 |
|---|---|---|
| CBX-023.1 | 参照帧 | 1 个事件 |
| CBX-023.2 | 去掉末尾 9 字节 | `Truncated` |
| CBX-023.3 | `side` = `buy` | `Shape` |
| CBX-023.4 | `type` = `resync` | `Shape` |
| CBX-023.5 | 价格 `1x` | `Number` |
| CBX-023.6 | 价格 `10.00001`(细于 `px_wire_exp`) | `Number` |
| CBX-023.7 | 时间没有 `Z` | `Number` |
| CBX-023.8 | 数量字符串里有反斜杠转义 | `Shape` |
| CBX-023.9 | 最后一档没有 `new_quantity` | `Shape` |
| CBX-023.10 | 没有 `events` | `Shape` |
| CBX-023.11 | 成员顺序打乱并带未知成员(数组、对象、小数)的快照;`Heartbeat` 类 | 1 个事件;`UnknownClass` |

### CBX-024 两条 level2 连接的录制回放

- 规则:[Advanced Trade 衍生品行情(coinbase-adv)](README.md#advanced-trade-衍生品行情coinbase-adv)
- 级别:golden
- 里程碑:M3
- 前置:`fixtures/coinbase-adv/race/level2_two_conns.jsonl`(2026-11-11 两条同主机连接按接收顺序的 462 帧,含每秒一次的保活订阅的应答与 `heartbeats`);逐帧按控制帧 / `peek` / `decode_md` 处理,连接号 = 录制的 `conn` + 1
- 输入:`fixtures/coinbase-adv/race/level2_two_conns.jsonl`
- 期望文件:`level2_two_conns.expected.jsonl`(独立脚本用字典簿与 `Decimal` 从线上值算出)
- 实现:`crates/venue-coinbase/tests/adv.rs::cbx_024_adv_golden_two_connections`

| 行 | 检查 | 期望 |
|---|---|---|
| CBX-024.1 | 转发的 `Bbo`(连接、交易所时间、买价、买量、卖价、卖量) | 与期望文件逐个相同(147 个) |
| CBX-024.2 | `update_id`;每个事件 | 严格递增;买 < 卖,两边数量 > 0 |
| CBX-024.3 | 首达连接 | 两条连接各转发 20 个以上 |
| CBX-024.4 | 控制帧 | `SubAck{ok}` 与 `Pong` 各 40 条以上 |

### CBX-025 `coinbase-adv` 的契约、默认值与截断冒烟

- 规则:[Advanced Trade 衍生品行情(coinbase-adv)](README.md#advanced-trade-衍生品行情coinbase-adv)、[实现说明](README.md#实现说明)
- 级别:property
- 里程碑:M3
- 前置:`RD-CB`;hft-venue-proto testkit 的 `check_default_caps`、`check_md_basics`
- 实现:`crates/venue-coinbase/tests/adv.rs::cbx_025_adv_contracts_defaults_and_truncation`

| 行 | 检查 | 期望 |
|---|---|---|
| CBX-025.1 | `default_caps`;`NAME`、`VENUE`;`check_md_basics`(一帧快照) | 无能力位、`LinearPerp`、`StaleGuard::None`;`coinbase-adv`、18;通过 |
| CBX-025.2 | `build_templates`、`private_subscribe`、`session_identity_request`、`account_request`、`account_decode`、`dead_man` | `Unsupported` × 4、`UnknownClass`、`None` |
| CBX-025.3 | 一帧真实 `update` 与一帧快照的每个前缀(簿里先有一个快照) | 每个都是 `Truncated` / `Shape` / `Number` 之一,没有事件,不 panic |

## 验收

CBX-001–CBX-017 与 CBX-019–CBX-025 全部通过;CBX-018 在有 cargo-fuzz 的环境运行。
