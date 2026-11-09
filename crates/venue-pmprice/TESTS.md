---
component: venue-pmprice
prefix: PPX
spec: README.md
---

# venue-pmprice 测试规格

格式与规则见 [docs/tests](../../docs/tests/README.md)。

本文件的记号:

- `RD-PP` = 测试内构建的 refdata(`tests/main.rs`):`USD` = AssetId 1(Stable),`BTC` = AssetId 2(px_exp −4);`POLYMARKET_PRICE`(17)的 `btcusd` = InstId 1、`ethusd` = InstId 2;`COINBASE_SPOT`(16)上另有符号同为 `btcusd` 的 InstId 3(外来 inst)。
- `RX` = 固定接收元数据(`conn` 3、`rx_tsc` 0x12345678、`sw_rx_ns`、`rx_empty_gap_ns` 77)。
- `PRICE`/`TWAP`/`HIST` = 2026-11-09 实盘机抓包中的一条价格、一条 TWAP 与一条(截为三项的)历史帧;`"74748.646845073935"` = 747486468(px_exp −4,四舍五入)。

## 覆盖

| README 小节 | 用例 |
|---|---|
| 概要 | PPX-011 |
| 目录 | 不适用:非规范 |
| 使用本组件 | PPX-001, PPX-012 |
| 端点与路由 | PPX-012 |
| 登录 | PPX-002, PPX-003 |
| 订阅 | PPX-004, PPX-005 |
| 行情频道 → 事件 | PPX-001, PPX-007, PPX-008, PPX-010 |
| 去重 | PPX-007, PPX-008, PPX-010 |
| 价格精度 | PPX-006 |
| 心跳 | PPX-005 |
| 限速默认值 | PPX-011 |
| VenueCaps 默认值 | PPX-011 |
| 账户 | 不适用:hft-core 用例(ReadOnlyVenue) |
| 实现说明 | PPX-009 |
| 延迟与资源契约 | 不适用:未建基准 |
| 测试与验收 | 不适用:非规范 |
| 依赖 | 不适用:由 `cargo xtask deps` 机检 |
| 延伸阅读 | 不适用:非规范 |
| 已知限制与后续工作 | 不适用:范围外 |

## 用例

### PPX-001 分派与序号规则

- 规则:[行情频道 → 事件](README.md#行情频道--事件)、[去重](README.md#去重)
- 级别:table
- 里程碑:M3
- 前置:无
- 实现:`crates/venue-pmprice/tests/main.rs::ppx_001_classify`

| 行 | 输入 | 期望 |
|---|---|---|
| PPX-001.1 | `PRICE` | `Price` |
| PPX-001.2 | `TWAP` | `Twap` |
| PPX-001.3 | `HIST` | `Price` |
| PPX-001.4 | `{"op":"subscribed",…` | `Subscribed` |
| PPX-001.5 | `{"op":"error",…` | `Error` |
| PPX-001.6 | `{"op":"pong",…` | `Pong` |
| PPX-001.7 | `{"op":"authed",…` | `Authed` |
| PPX-001.8 | `PRICE`,二进制帧 | `Unknown` |
| PPX-001.9 | `{"v":1,"channel":"price.other"}` | `Unknown` |
| PPX-001.10 | `seq_rule` | `Price`/`Twap` → `Monotonic`,其余 `None` |

### PPX-002 登录帧

- 规则:[登录](README.md#登录)
- 级别:table
- 里程碑:M3
- 前置:`Credential` 由测试构造
- 实现:`crates/venue-pmprice/tests/main.rs::ppx_002_login_frame`

| 行 | 输入 | 期望 |
|---|---|---|
| PPX-002.1 | secret `<api>|<钱包私钥>|<存款钱包>`,`Md`,step 0 | `Frame`;帧逐字节为 README 形式,secret 只有 `<api>` |
| PPX-002.2 | 同上 | 帧不含第二、三段与 `|`;`api_secret` = 第一段 |
| PPX-002.3 | secret 不含 `|` | `api_secret` = 整个 secret |
| PPX-002.4 | key 含 `"`、secret 含 `\`、secret 以 `|` 开头(第一段为空) | `Err(Cfg)` |
| PPX-002.5 | `Order` 角色 | `Err(Unsupported)` |
| PPX-002.6 | step 1 | `Err(Cfg)` |

### PPX-003 登录应答

- 规则:[登录](README.md#登录)
- 级别:table
- 里程碑:M3
- 前置:无
- 实现:`crates/venue-pmprice/tests/main.rs::ppx_003_login_result`

| 行 | 输入 | 期望 |
|---|---|---|
| PPX-003.1 | `{"op":"authed","rid":"a1"}` | `Ok` |
| PPX-003.2 | `{"op":"error",…}` | `Rejected(1)` |
| PPX-003.3 | `PRICE` | `NotMine` |

### PPX-004 订阅帧

- 规则:[订阅](README.md#订阅)
- 级别:table
- 里程碑:M3
- 前置:`SubFrames` 4096 B / 16 帧
- 实现:`crates/venue-pmprice/tests/main.rs::ppx_004_subscribe_frames`

| 行 | 输入 | 期望 |
|---|---|---|
| PPX-004.1 | `[btcusd Index, btcusd Mark]` | 第一帧只覆盖 1 个规格,逐字节为 `price.crypto` 形式 |
| PPX-004.2 | `[btcusd Mark]` | `price.crypto.twap` 形式,带 `"window_seconds":60` |
| PPX-004.3 | 退订 | 以 `{"op":"unsubscribe"` 开头 |
| PPX-004.4 | 外来 inst | `Err(Cfg)` |
| PPX-004.5 | `Bbo` 流 | `Err(Unsupported)` |

### PPX-005 控制应答与心跳

- 规则:[订阅](README.md#订阅)、[心跳](README.md#心跳)
- 级别:table
- 里程碑:M3
- 前置:无
- 实现:`crates/venue-pmprice/tests/main.rs::ppx_005_control_and_ping`

| 行 | 输入 | 期望 |
|---|---|---|
| PPX-005.1 | `subscribed` | `SubAck{ok: true, code: 0}` |
| PPX-005.2 | `error`(`bad_channel`) | `SubAck{ok: false, code: 1}` |
| PPX-005.3 | `pong` | `Pong` |
| PPX-005.4 | `PRICE` | `None` |
| PPX-005.5 | `app_ping`,64 B / 4 B 缓冲 | `{"op":"ping","rid":"p"}` / 0 |
| PPX-005.6 | `heartbeat(Md)` | 间隔 5 000、pong 超时 5 000、空闲超时 10 000 ms |

### PPX-006 价格精度

- 规则:[价格精度](README.md#价格精度)
- 级别:table
- 里程碑:M3
- 前置:exp −4
- 实现:`crates/venue-pmprice/tests/main.rs::ppx_006_round_dec`

| 行 | 输入 | 期望 |
|---|---|---|
| PPX-006.1 | `74748.646845073935` | 747486468 |
| PPX-006.2 | `74748.64684` | 747486468 |
| PPX-006.3 | `74748.64685` | 747486469(半数进位) |
| PPX-006.4 | `74748` | 747480000 |
| PPX-006.5 | `74748.6` | 747486000 |
| PPX-006.6 | `74748.99995` | 747490000(进位到整数) |
| PPX-006.7 | `-1.0` | `Err(Number)` |
| PPX-006.8 | `1e5` | `Err(Number)` |
| PPX-006.9 | 20 位整数 | `Err(Overflow)` |
| PPX-006.10 | `1.`、空串 | `Err(Number)` |

### PPX-007 实时样本

- 规则:[行情频道 → 事件](README.md#行情频道--事件)、[去重](README.md#去重)
- 级别:example
- 里程碑:M3
- 前置:新 `FeedCtx`
- 实现:`crates/venue-pmprice/tests/main.rs::ppx_007_live_samples`

| 行 | 输入 | 期望 |
|---|---|---|
| PPX-007.1 | `PRICE` | 一条 `Index`:inst 1、`ex_ts_ns` = `ex_send_ns` = 1794227705000 ms、px 747486468 |
| PPX-007.2 | `TWAP` | 一条 `Mark`:px 747666234 |
| PPX-007.3 | 再次 `PRICE` | 无事件 |
| PPX-007.4 | 符号 `dogeusd` | 无事件,`Ok` |
| PPX-007.5 | `PRICE` | venue 17,`conn`/`rx_tsc`/`sw_rx_ns`/`rx_empty_gap_ns` 与 `RX` 相同 |

### PPX-008 历史帧

- 规则:[行情频道 → 事件](README.md#行情频道--事件)、[去重](README.md#去重)
- 级别:example
- 里程碑:M3
- 前置:`HIST` 三项(…702、…703、…704 s)
- 实现:`crates/venue-pmprice/tests/main.rs::ppx_008_history_frame`

| 行 | 输入 | 期望 |
|---|---|---|
| PPX-008.1 | `peek(HIST)` | inst 1、`Index`、`last_id` 1794227704000 |
| PPX-008.2 | 水位已到 …703 s | 只出 …704 s 一条(px 748000001,半数进位) |
| PPX-008.3 | 新 `FeedCtx` 解两次 | 第一次 3 条,第二次 0 条 |
| PPX-008.4 | 其后 `TWAP` | 1 条(价格水位不影响 TWAP) |
| PPX-008.5 | 第三项数值为 `74800.0x` | `Err(Number)`,无事件,水位不动 |

### PPX-009 畸形输入

- 规则:[实现说明](README.md#实现说明)
- 级别:property
- 里程碑:M3
- 前置:无
- 实现:`crates/venue-pmprice/tests/main.rs::ppx_009_malformed`

| 行 | 输入 | 期望 |
|---|---|---|
| PPX-009.1 | `PRICE` 的每个前缀 | `peek` 不恐慌;`decode_md` 返回错误且无事件 |
| PPX-009.2 | 控制类 `Pong` 交给 `decode_md` | `Err(UnknownClass)` |
| PPX-009.3 | `snapshot: true` 但无 `data` | `Err(Shape)` |

### PPX-010 抓包金样

- 规则:[行情频道 → 事件](README.md#行情频道--事件)、[去重](README.md#去重)
- 级别:golden
- 里程碑:M3
- 前置:`fixtures/polymarket-price/capture/btcusd_40s.jsonl`(87 帧)
- 实现:`crates/venue-pmprice/tests/main.rs::ppx_010_capture_golden`

| 行 | 检查 | 期望 |
|---|---|---|
| PPX-010.1 | 控制应答 | 2 个 `SubAck{ok}`、1 个 `SubAck{!ok}`(`bad_channel`) |
| PPX-010.2 | 按接收顺序解码全部数据帧 | 与 `btcusd_40s.expected.jsonl` 的 319 条逐条相同 |

### PPX-011 契约

- 规则:[VenueCaps 默认值](README.md#venuecaps-默认值)、[限速默认值](README.md#限速默认值)
- 级别:contract
- 里程碑:M3
- 前置:无
- 实现:`crates/venue-pmprice/tests/main.rs::ppx_011_contracts`

| 行 | 检查 | 期望 |
|---|---|---|
| PPX-011.1 | `check_default_caps` | 通过 |
| PPX-011.2 | `check_md_basics`(`PRICE`、`TWAP`) | 通过 |

### PPX-012 路由与端点

- 规则:[端点与路由](README.md#端点与路由)
- 级别:table
- 里程碑:M3
- 前置:无
- 实现:`crates/venue-pmprice/tests/main.rs::ppx_012_routes_and_endpoint`

| 行 | 检查 | 期望 |
|---|---|---|
| PPX-012.1 | `routes()` | 一条 `(ROUTE_PRICE, Md, 不赛马)` |
| PPX-012.2 | `stream_route` | `Index`、`Mark` → `ROUTE_PRICE`;`Bbo` → `None` |
| PPX-012.3 | `endpoint` | `Md` → `/ws`;`Order` 或未知路由 → `Unsupported` |
