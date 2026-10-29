---
component: venue-polymarket
prefix: PMK
spec: README.md
---

# venue-polymarket 测试规格

格式与规则见 [docs/tests](../../docs/tests/README.md)。

本文件的记号:

- `RD-PM` = 测试内构建的 refdata:`USDC` = AssetId 1(Stable),两个代币资产(px_exp −3,qty_exp −6);`POLYMARKET_CLOB`(15):`btc-up-0` = InstId 1、`btc-dn-0` = InstId 2,tick 0.01;lot 与最小量 `btc-up-0` 0.000001、`btc-dn-0` 0.01(同实盘)。
- `T_UP`/`T_DN` = 2026-10-19 录制的 `btc-updown-5m` 窗口的 Up/Down 代币 id;`bound()` = 容量 16 的 `FeedCtx`,`T_UP` → 1、`T_DN` → 2。
- 帧取自 2026-10-19 在 eu-west-1 的独立录制(pmrec);数值写法:价格 `"0.42"` = 420(px_exp −3),数量 `"242.09"` = 242090000(qty_exp −6)。

## 覆盖

| README 小节 | 用例 |
|---|---|
| 概要 | PMK-001 |
| 目录 | 不适用:非规范 |
| 使用本组件 | PMK-001 |
| 端点 | PMK-008, PMK-029 |
| 槽位绑定 | PMK-009, PMK-010, PMK-013, PMK-014 |
| 订阅 | PMK-008, PMK-014 |
| 行情频道 → 事件 | PMK-001–PMK-007, PMK-011, PMK-015 |
| 合成序号 | PMK-003–PMK-005, PMK-010–PMK-012 |
| 多连接去重 | PMK-016, PMK-017 |
| 心跳 | PMK-001 |
| 签名 | PMK-018–PMK-022 |
| 凭证 | PMK-023, PMK-024 |
| 下单 | PMK-023–PMK-026, PMK-028, PMK-031, PMK-032, PMK-034, PMK-035 |
| 预签名 | PMK-035, PMK-036 |
| 用户流事件表 | PMK-027, PMK-029, PMK-032, PMK-034 |
| 累计量与费用来源 | PMK-027, PMK-031 |
| 结果未知码表 | PMK-026, PMK-034 |
| 账户配置核验 | PMK-030 |
| VenueCaps 默认值 | PMK-011, PMK-033, PMK-034 |
| 实现说明 | PMK-006, PMK-007 |
| 延迟与资源契约 | PMK-011, PMK-036 |
| 测试与验收 | 不适用:非规范 |
| 依赖 | 不适用:由 `cargo xtask deps` 机检 |
| 延伸阅读 | 不适用:非规范 |
| 已知限制与后续工作 | 不适用:里程碑外 |

## 用例

### PMK-001 分类、控制帧与序号规则

- 规则:[行情频道 → 事件](README.md#行情频道--事件)、[心跳](README.md#心跳)
- 级别:table
- 里程碑:M3
- 前置:无
- 实现:`crates/venue-polymarket/tests/main.rs::pmk_001_classify_push_pong_unknown`

| 行 | 输入 | 期望 |
|---|---|---|
| PMK-001.1 | 文本,以 `{` 或 `[` 开头 | `MdClass::Push` |
| PMK-001.2 | 文本 `PONG` | `MdClass::Pong`;`control` → `ControlHint::Pong` |
| PMK-001.3 | 二进制帧;文本 `INVALID OPERATION` | `MdClass::Unknown` |
| PMK-001.4 | `seq_rule(Push)` | `SeqRule::None` |

### PMK-002 peek 给出首个代币的 inst 与流

- 规则:[行情频道 → 事件](README.md#行情频道--事件)
- 级别:table
- 里程碑:M3
- 前置:`bound()`
- 实现:`crates/venue-polymarket/tests/main.rs::pmk_002_peek_stream_and_bound_inst`

| 行 | 消息 | 期望 |
|---|---|---|
| PMK-002.1 | `book`(`T_UP`) | `(1, DepthTopN)` |
| PMK-002.2 | `price_change`(首条 `T_UP`) | `(1, DepthTopN)` |
| PMK-002.3 | `last_trade_price`(`T_UP`) | `(1, Trade)` |
| PMK-002.4 | `tick_size_change` | `Err(UnknownClass)` |
| PMK-002.5 | `book`,代币未绑定 | inst 0 |
| PMK-002.6 | `[]`、`[ ]` 加换行(空数组) | `Err(UnknownClass)`;`[`、`[[` → `Err(Shape)` |

### PMK-003 book 是整簿快照,数量 0 的档丢弃

- 规则:[行情频道 → 事件](README.md#行情频道--事件)、[合成序号](README.md#合成序号)
- 级别:golden
- 里程碑:M3
- 前置:`bound()`;`book` 含买 `0.01×9468.04`、`0.49×0`、`0.5×242.09`,卖 `0.99×1200`、`0.51×1.960785`,`timestamp` `1792424113368`
- 期望:一个 `L2Snapshot`,`origin = EXCHANGE`,`snap_id = 1`,买 2 档、卖 2 档(数值按记号换算),`ex_ts_ns = 1792424113368000000`,`ex_send_ns = 0`;同一代币下一个 `book` 的 `snap_id = 2`
- 实现:`crates/venue-polymarket/tests/main.rs::pmk_003_book_is_a_snapshot_without_zero_levels`

### PMK-004 price_change 按代币各出一个连续的 L2Batch

- 规则:[行情频道 → 事件](README.md#行情频道--事件)、[合成序号](README.md#合成序号)
- 级别:golden
- 里程碑:M3
- 前置:`bound()`;先解码 PMK-003 的 `book`(`T_UP`)
- 输入:`price_change`,`T_UP` `0.42 BUY 242.09`、`T_DN` `0.58 SELL 242.09`
- 期望:依次为 inst 1 的 `L2Batch{prev_id 1, last_id 2, n_bids 1, n_asks 0}` + 买 `(420, 242090000)`,inst 2 的 `L2Batch{prev_id 0, last_id 1, n_bids 0, n_asks 1}` + 卖 `(580, 242090000)`;`MdOutcome{events 2, inst 1}`
- 实现:`crates/venue-polymarket/tests/main.rs::pmk_004_price_change_one_chained_batch_per_token`

### PMK-005 last_trade_price 是带合成 id 的成交

- 规则:[行情频道 → 事件](README.md#行情频道--事件)
- 级别:golden
- 里程碑:M3
- 前置:`bound()`
- 期望:`Trade{px 510, qty 1960785, trade_id 1, taker_side Buy}`;再一条同样的消息 `trade_id = 2`
- 实现:`crates/venue-polymarket/tests/main.rs::pmk_005_last_trade_price_is_a_trade_with_a_synthetic_id`

### PMK-006 列表帧中的多个 book

- 规则:[实现说明](README.md#实现说明)
- 级别:golden
- 里程碑:M3
- 前置:`bound()`
- 输入:`[<book T_UP>,<book T_DN>]`
- 期望:两个 `L2Snapshot`,inst 依次为 1、2;`MdOutcome{events 2, inst 1}`
- 实现:`crates/venue-polymarket/tests/main.rs::pmk_006_list_frame_of_two_books`

### PMK-007 坏数字不写任何事件

- 规则:[行情频道 → 事件](README.md#行情频道--事件)
- 级别:table
- 里程碑:M3
- 前置:`bound()`;未绑定的空表
- 实现:`crates/venue-polymarket/tests/main.rs::pmk_007_bad_number_writes_nothing`

| 行 | 输入 | 期望 |
|---|---|---|
| PMK-007.1 | `price_change` 的第二条价格为 `"0.5x"` | `Err(Number)`,无事件(第一条也不写) |
| PMK-007.2 | 价格 `"0.4215"`(细于 px_exp) | `Err(Number)` |
| PMK-007.3 | 空绑定表解码 `price_change` | `Ok(events 0)`,无事件 |

### PMK-008 订阅帧来自绑定表

- 规则:[订阅](README.md#订阅)、[端点](README.md#端点)
- 级别:golden
- 里程碑:M3
- 前置:`bound()`
- 实现:`crates/venue-polymarket/tests/main.rs::pmk_008_subscribe_frames_from_bindings`

| 行 | 输入 | 期望 |
|---|---|---|
| PMK-008.1 | 订阅 `[1 DepthTopN, 1 Trade, 2 DepthTopN]` | 一帧 `{"type":"market","operation":"subscribe","assets_ids":["T_UP","T_DN"]}`,覆盖 3 条 |
| PMK-008.2 | 退订 `[2 DepthTopN]` | `{"type":"market","operation":"unsubscribe","assets_ids":["T_DN"]}` |
| PMK-008.3 | 未绑定的绑定表 | `Err(Cfg)` |
| PMK-008.4 | `Bbo` 流 | `Err(Unsupported)` |
| PMK-008.5 | `SUB_ACKS` | `false` |

### PMK-009 绑定表规则

- 规则:[槽位绑定](README.md#槽位绑定)
- 级别:table
- 里程碑:M3
- 前置:容量 2 的 `FeedCtx`
- 实现:`crates/venue-polymarket/tests/main.rs::pmk_009_binding_table_rules`

| 行 | 操作 | 期望 |
|---|---|---|
| PMK-009.1 | 绑定空串、`12a`、81 位数字 | `Err(Token)` |
| PMK-009.2 | 绑定到 inst 0 | `Err(Inst)` |
| PMK-009.3 | 1 ← `111`;2 ← `111` | `Ok`;`Err(TokenInUse)` |
| PMK-009.4 | 2 ← `222`;9 ← `333` | `Ok`;`Err(Full)` |
| PMK-009.5 | 2 改绑 `444` | `inst_of("222") = 0`,`token_of(2) = "444"` |
| PMK-009.6 | `unbind(1)` | `inst_of("111") = 0`,`token_of(1) = None` |

### PMK-010 改绑后序号继续递增

- 规则:[槽位绑定](README.md#槽位绑定)、[合成序号](README.md#合成序号)
- 级别:golden
- 里程碑:M3
- 前置:`bound()`;依次解码 PMK-003 的 `book` 与 PMK-004 的 `price_change`,再把 inst 1 改绑到新代币
- 期望:新代币的 `book` 得到 `snap_id = 3`
- 实现:`crates/venue-polymarket/tests/main.rs::pmk_010_rebinding_keeps_the_chain_monotonic`

### PMK-011 实采窗口重建的最优价与交易所一致

- 规则:[行情频道 → 事件](README.md#行情频道--事件)、[合成序号](README.md#合成序号)、[延迟与资源契约](README.md#延迟与资源契约)
- 级别:golden
- 里程碑:M3
- 前置:`bound()`;`fixtures/polymarket/market-btc-400.jsonl`(2026-10-19 pmrec 录制的连续 400 帧,含 `book`、`price_change`、`last_trade_price`)
- 期望:每帧解码成功;每个 `L2Batch` 的 `prev_id` 等于该 inst 上一个 id;每条 `price_change` 之后,按事件重建的簿的最优买/卖价等于该条给出的 `best_bid`/`best_ask`(簿建立之后,至少 100 处)
- 实现:`crates/venue-polymarket/tests/main.rs::pmk_011_recorded_window_matches_exchange_best_prices`

### PMK-012 早于最近整簿的增量丢弃

- 规则:[行情频道 → 事件](README.md#行情频道--事件)
- 级别:golden
- 里程碑:M3
- 前置:`bound()`;先解码 PMK-003 的 `book`(时间 `1792424113368`)
- 实现:`crates/venue-polymarket/tests/main.rs::pmk_012_changes_older_than_the_book_are_dropped`

| 行 | 输入 | 期望 |
|---|---|---|
| PMK-012.1 | `T_UP` 的 `price_change`,时间 `1792424113367` | `Ok(events 0)`,无事件 |
| PMK-012.2 | 同上,时间 `1792424113368` | 一个 `L2Batch{prev_id 1, last_id 2}` |
| PMK-012.3 | inst 1 改绑新代币后,新代币的 `1792424113367` 增量 | 应用(`events 1`) |

### PMK-013 交易所 id(uint256)绑定为十进制代币

- 规则:[槽位绑定](README.md#槽位绑定)
- 级别:table
- 里程碑:M3
- 前置:容量 4 的 `FeedCtx`
- 实现:`crates/venue-polymarket/tests/main.rs::pmk_013_wire_id_binds_the_decimal_token`

| 行 | `bind_wire` 输入 | 期望 |
|---|---|---|
| PMK-013.1 | inst 1,`T_UP` 的 32 字节大端 | `Ok`;`token_of(1) = T_UP`,`inst_of(T_UP) = 1` |
| PMK-013.2 | inst 2,值 7 | `token_of(2) = "7"` |
| PMK-013.3 | inst 2,全零 | 解绑:`token_of(2) = None` |
| PMK-013.4 | inst 0 | `Err(Cfg)` |

### PMK-014 改绑后退订旧代币、订阅新代币

- 规则:[订阅](README.md#订阅)、[槽位绑定](README.md#槽位绑定)
- 级别:golden
- 里程碑:M3
- 前置:`bound()`;inst 1 改绑到新代币
- 实现:`crates/venue-polymarket/tests/main.rs::pmk_014_rebinding_unsubscribes_the_previous_token`

| 行 | 输入 | 期望 |
|---|---|---|
| PMK-014.1 | 退订 inst 1 | 帧里是旧代币 `T_UP` |
| PMK-014.2 | 订阅 inst 1 | 帧里是新代币 |
| PMK-014.3 | inst 2 解绑后订阅 `[2, 1]` | 未绑定的 inst 2 跳过,帧覆盖 2 条 |

### PMK-015 peek 按首个已绑定代币路由

- 规则:[行情频道 → 事件](README.md#行情频道--事件)
- 级别:table
- 里程碑:M3
- 前置:容量 16 的 `FeedCtx`,只有 `T_DN` → 2 绑定
- 实现:`crates/venue-polymarket/tests/main.rs::pmk_015_peek_routes_by_the_first_bound_token`

| 行 | 输入 | 期望 |
|---|---|---|
| PMK-015.1 | PMK-004 的 `price_change`(首条为 `T_UP`) | `(2, DepthTopN)` |
| PMK-015.2 | 列表帧 `[<book T_UP>,<book T_DN>]` | `(2, DepthTopN)` |
| PMK-015.3 | 空绑定表,同 .1 | `(0, DepthTopN)` |
| PMK-015.4 | 解码 .1 | 只写 inst 2 的一个 `L2Batch` |

### PMK-016 另一条连接送来的同一条消息丢弃

- 规则:[多连接去重](README.md#多连接去重)
- 级别:golden
- 里程碑:M3
- 前置:`bound()`;`RX` 的连接号改为 3 或 4
- 实现:`crates/venue-polymarket/tests/main.rs::pmk_016_copies_from_another_connection_are_dropped`

| 行 | 输入 | 期望 |
|---|---|---|
| PMK-016.1 | PMK-003 的 `book` 先经连接 3、再经连接 4 | 连接 3 `events 1`;连接 4 `events 0`;`dups() = 1` |
| PMK-016.2 | PMK-004 的 `price_change` 先经连接 4、再经连接 3 | 连接 4 `events 2`;连接 3 `events 0` |
| PMK-016.3 | 同一条 `last_trade_price`:连接 3 两次,再连接 4 三次 | 连接 3 各 `events 1`;连接 4 前两次 `events 0`、第三次 `events 1`;`dups() = 4` |
| PMK-016.4 | 新的 `bound()`,同一条 `price_change` 经连接 3 两次 | 两次都 `events 2`;`dups() = 0` |
| PMK-016.5 | 新的 `bound()`:PMK-004 的 `price_change` 经连接 3,再一条时间戳、`hash` 相同但价格不同(`0.41`)的经连接 4 | 两条都 `events 2`(不是副本);`dups() = 0` |

### PMK-017 迟到的整簿与消息丢弃

- 规则:[多连接去重](README.md#多连接去重)
- 级别:golden
- 里程碑:M3
- 前置:`bound()`;PMK-003 的 `book`,再一条 `T_UP` 的 `price_change`(时间 `1792424118000`)经连接 3 应用
- 实现:`crates/venue-polymarket/tests/main.rs::pmk_017_late_books_and_messages_are_dropped`

| 行 | 输入 | 期望 |
|---|---|---|
| PMK-017.1 | 连接 4:`T_UP` 的 `book`,时间 `1792424117999`、另一 `hash` | `events 0`;`lates() = 1` |
| PMK-017.2 | 连接 4:`price_change` 时间 `1792424115500`;再一条时间 `1792424116500`(`hash` 各不相同) | 前者 `events 0`、`lates() = 2`;后者 `events 1` |
| PMK-017.3 | `last_trade_price` 时间 `1792424115000`、另一 `transaction_hash` | `events 0`;`lates() = 3` |
| PMK-017.4 | inst 1 改绑新代币后,新代币的 PMK-017.1 整簿 | `events 1` |

## 验收

PMK-001–PMK-011、PMK-015 全部通过;线上验收(两个完整 5 分钟窗口)见 [pm-md-probe](../../tools/pm-md-probe/TESTS.md)。

### PMK-018 订单签名与官方客户端一致

- 规则:[签名](README.md#签名)
- 级别:golden
- 里程碑:M3
- 前置:金样 `crates/venue-polymarket/tests/sign_v2_golden.json`(`py-clob-client-v2` 生成;测试私钥 `0x11…11`,salt 1234567890123,timestamp 1761100000123)
- 步骤:依次执行下表各行
- 实现:`crates/venue-polymarket/tests/main.rs::sign_v2::pmk_018_order_signature_matches_the_official_client`

| 行 | 输入 | 期望 |
|---|---|---|
| PMK-018.1 | 私钥 | EIP-55 地址等于金样 `address` |
| PMK-018.2 | 金样两笔订单(买 10 份 @0.45、卖 10 份 @0.55),域 version "2"、CTF Exchange V2 | `0x` + `r‖s‖v` 十六进制等于金样 `signature` |

### PMK-019 L1 认证签名与官方客户端一致

- 规则:[签名](README.md#签名)
- 级别:golden
- 里程碑:M3
- 前置:同 PMK-018
- 步骤:金样 `auth` 的两组 (timestamp, nonce) 各签一次
- 期望:签名等于金样
- 实现:`crates/venue-polymarket/tests/main.rs::sign_v2::pmk_019_l1_auth_signature_matches_the_official_client`

### PMK-020 L2 请求签名与官方客户端一致

- 规则:[签名](README.md#签名)
- 级别:golden
- 里程碑:M3
- 前置:同 PMK-018
- 步骤:金样 `hmac` 的三组(POST 带请求体、DELETE、GET)
- 期望:URL-safe base64 结果等于金样
- 实现:`crates/venue-polymarket/tests/main.rs::sign_v2::pmk_020_l2_hmac_matches_the_official_client`

### PMK-021 输入校验

- 规则:[签名](README.md#签名)
- 级别:table
- 里程碑:M3
- 前置:无
- 步骤:依次执行下表各行
- 实现:`crates/venue-polymarket/tests/main.rs::sign_v2::pmk_021_inputs_are_validated`

| 行 | 输入 | 期望 |
|---|---|---|
| PMK-021.1 | 私钥长度不对、非十六进制、全零 | `BadKey` |
| PMK-021.2 | token id 255、256、2²⁵⁶ − 1、2²⁵⁶、`12a`、空串 | 前三个按大端 32 字节解析;后三个 `None` |
| PMK-021.3 | API secret 不是 URL-safe base64 | `BadSecret` |

### PMK-022 存款钱包签名与官方客户端一致

- 规则:[签名](README.md#签名)
- 级别:golden
- 里程碑:M3
- 前置:同 PMK-018;金样 `orders_1271`(签名类型 3,`maker = signer` = 存款钱包 `0x7cB0…8cB2`)
- 步骤:两笔订单(买 5 份 @0.45、卖 5 份 @0.55)按存款钱包格式签名
- 期望:`0x` + 十六进制等于金样 `signature`(内层签名 ‖ 域分隔符 ‖ 订单结构哈希 ‖ 类型串 ‖ 长度)
- 实现:`crates/venue-polymarket/tests/main.rs::sign_v2_1271::pmk_022_deposit_wallet_signature_matches_the_official_client`

### PMK-023 下单编码:存款钱包签名的 post-only GTD 订单

- 规则:[下单](README.md#下单)、[凭证](README.md#凭证)
- 级别:unit
- 里程碑:M3
- 前置:测试私钥(同 PMK-018)、测试 API key/secret/passphrase、存款钱包 `0x7cB0…8cB2`;`GwCtx` 绑定 `btc-up-0`、`btc-dn-0` 的 token;时钟 1 792 687 605 366 ms
- 实现:`crates/venue-polymarket/tests/orders.rs::pmk_023_place_is_a_signed_post_only_gtd_order`

| 行 | 输入 | 期望 |
|---|---|---|
| PMK-023.1 | `Place{UP, Buy, 0.45, 5 份, PostOnly, slot 7}` | `POST /order`,`redacted` |
| PMK-023.2 | 同上 | `maker = signer` = 存款钱包;`tokenId` = UP 的 token;`makerAmount` 2250000、`takerAmount` 5000000;`side` BUY;`signatureType` 3;`timestamp` = 时钟毫秒 |
| PMK-023.3 | 同上 | `expiration` = 时钟秒 + 300;`orderType` GTD、`postOnly` true、`deferExec` false、`owner` = API key |
| PMK-023.4 | 同上 | `salt` < 2⁵³,低 20 位 = 槽号 7 |
| PMK-023.5 | 同上 | `signature` = 用同一组字段算出的存款钱包签名 |
| PMK-023.6 | 同上 | L2 头:`POLY_ADDRESS` = EOA(EIP-55)、`POLY_API_KEY`、`POLY_PASSPHRASE`、`POLY_TIMESTAMP` = 时钟秒、`POLY_SIGNATURE` = 对该请求体的 L2 HMAC;`Content-Type: application/json`;每行以 CRLF 结尾 |
| PMK-023.7 | `Place{DN, Sell, 0.55, 5 份, Gtc}` | `makerAmount` 5000000、`takerAmount` 2750000、`side` SELL;`postOnly` false |

### PMK-024 下单拒绝编码

- 规则:[下单](README.md#下单)、[凭证](README.md#凭证)
- 级别:table
- 里程碑:M3
- 前置:同 PMK-023
- 实现:`crates/venue-polymarket/tests/orders.rs::pmk_024_place_refusals`

| 行 | 输入 | 期望 |
|---|---|---|
| PMK-024.1 | `GwCtx` 没有绑定 | `UnknownInst` |
| PMK-024.2 | `Tif` IOC、FOK;`REDUCE_ONLY` | `Unsupported(Place)` |
| PMK-024.3 | 价 0.001 × 量 0.000001(金额不足 1e-6) | `FieldOverflow` |
| PMK-024.4 | secret 少一段;存款钱包不是地址 | `build_templates` 与 `encode_rest_ctx` 都失败 |
| PMK-024.5 | `encode_rest_signed` | 失败(只有 `encode_rest_ctx` 能编码) |

### PMK-025 REST 回执绑定订单哈希,撤单与查单用它

- 规则:[下单](README.md#下单)
- 级别:unit
- 里程碑:M3
- 前置:同 PMK-023;`RestOrigin{Order, (slot 7, gen 3, op), UP}`
- 实现:`crates/venue-polymarket/tests/orders.rs::pmk_025_ack_binds_the_hash_and_cancel_uses_it`

| 行 | 输入 | 期望 |
|---|---|---|
| PMK-025.1 | 实采 `POST /order` 应答(200,`orderID` H1) | 一条 `Ack{slot 7, gen 3, UP, exch_order_id = H1 前 8 字节, epoch 0}`;`exch_id(十进制键, None)` 一次 |
| PMK-025.2 | `Cancel`,`exch_order_id` = H1 的键 | `DELETE /order`,体 `{"orderID":"H1"}`,带 L2 头 |
| PMK-025.3 | `Cancel`,`exch_order_id` 0,本进程没有为目标 client id 签出过订单(重启前的订单) | `DELETE /cancel-market-orders`,体 `{"asset_id":<UP token>}` |
| PMK-025.4 | `CancelAll{DN}`;`CancelAll{inst 0}` | 前者按 DN 的 token;后者 `DELETE /cancel-all`、无体、无 `Content-Type` |
| PMK-025.5 | `Query`,H1 的键;未知键 | `GET /data/order/H1`;`UnknownTarget` |

### PMK-026 REST 拒绝与结果未知

- 规则:[结果未知码表](README.md#结果未知码表)
- 级别:table
- 里程碑:M3
- 前置:同 PMK-025
- 实现:`crates/venue-polymarket/tests/orders.rs::pmk_026_rest_refusals_and_unknown_outcomes`

| 行 | 应答 | 期望 |
|---|---|---|
| PMK-026.1 | 400 `{"error":"invalid post-only order: order crosses book"}` | `Reject{slot 7}`,`reject_code` = `raw_code_str(消息)`,`raw_code` 相同,非 `outcome_unknown` |
| PMK-026.2 | 200 `{"success":false,"errorMsg":"not enough balance / allowance"}` | `Reject` |
| PMK-026.3 | 429;425 | `Reject`,`reject_code` = HTTP 码;`ctl = RateLimited` |
| PMK-026.4 | 500、502、0 | 无事件,`outcome_unknown` |
| PMK-026.5 | 单笔撤单 `not_canceled` 非空 | `CancelReject{detail = GONE(1), slot 7}` |
| PMK-026.6 | 200 截断的 JSON | `DecodeErr` |

### PMK-027 用户频道:成交、撤单与去重

- 规则:[用户流事件表](README.md#用户流事件表)、[累计量与费用来源](README.md#累计量与费用来源)
- 级别:unit
- 里程碑:M3
- 前置:同 PMK-023;帧按实采 `order` 事件的字段与顺序构造;sink 已把 H1、H2 绑定到 `(epoch 5, slot 7, gen 3)`
- 实现:`crates/venue-polymarket/tests/orders.rs::pmk_027_user_channel_fills_and_cancels`

| 行 | 输入 | 期望 |
|---|---|---|
| PMK-027.1 | H1 `PLACEMENT` | 无事件,`skip = Listed` |
| PMK-027.2 | H1 `UPDATE` `size_matched` 2,再 5(原始 5 份,价 0.02) | `PartialFill{qty 2, cum 2}`、`Fill{qty 3, cum 5}`;`px = avg_px` = 0.02;`detail` = Buy;slot 7、epoch 5、`exch_order_id` = H1 键、`ex_ts_ns` = 帧时间 |
| PMK-027.3 | 再送一次 `size_matched` 5 | 无新事件 |
| PMK-027.4 | H2 `CANCELLATION`,再送列出 H2 的撤单应答 | 恰一条 `CancelAck{slot 7, H2 键, UP}` |
| PMK-027.5 | 未绑定订单的 `UPDATE` 1 份 | `PartialFill{slot UNRESOLVED, qty 1}` |
| PMK-027.6 | `asset_id` 不在绑定表;`trade` 事件;`PONG` | 无事件;`trade` 为 `skip = Listed`;`PONG` 分类 `Ignorable` |
| PMK-027.7 | `event_type = market_resolved` | `skip = Unknown`;`report_event_name` = `market_resolved` |
| PMK-027.8 | 两个 `UPDATE`(1、4)的数组帧;`size_matched` 非数字 | 增量 1、3;后者 `DecodeErr`;`is_private_report(Push)` |

### PMK-028 查单结果

- 规则:[下单](README.md#下单)
- 级别:table
- 里程碑:M3
- 前置:同 PMK-025
- 实现:`crates/venue-polymarket/tests/orders.rs::pmk_028_query_result`

| 行 | 应答 | 期望 |
|---|---|---|
| PMK-028.1 | 200 `LIVE`,原始 5、成交 2、价 0.45 | `QueryResult{PartiallyFilled(2), cum 2, avg_px 0.45, H1 键}` |
| PMK-028.2 | `MATCHED`;`CANCELED` | Filled(3);Cancelled(4) |
| PMK-028.3 | 200 `null`;404 | NotFound(7) |

### PMK-029 用户频道会话

- 规则:[用户流事件表](README.md#用户流事件表)、[端点](README.md#端点)
- 级别:unit
- 里程碑:M3
- 前置:同 PMK-023
- 实现:`crates/venue-polymarket/tests/orders.rs::pmk_029_user_channel_session`

| 行 | 输入 | 期望 |
|---|---|---|
| PMK-029.1 | `endpoint(Order)`;`login(Order)` | 路径 `/ws/user`;`LoginStep::Sent`,帧 = `{"auth":{"apiKey","secret","passphrase"},"type":"user"}` |
| PMK-029.2 | `login(Md)`;`private_subscribe` | `NotNeeded`;不发帧 |
| PMK-029.3 | passphrase 含双引号 | 登录失败 |
| PMK-029.4 | `heartbeat(Order)` | 文本 `PING` 每 10 s;`pong_timeout_ms` 0;`idle_timeout_ms` 30 000 |

### PMK-030 账户启动核验

- 规则:[账户配置核验](README.md#账户配置核验)
- 级别:table
- 里程碑:M3
- 前置:RD-PM 另有实盘账户 1(`is_prod`,`Verified`,venue UID = 测试 API key 的前 24 个字符)
- 实现:`crates/venue-polymarket/tests/orders.rs::pmk_030_account_baseline`

| 行 | 输入 | 期望 |
|---|---|---|
| PMK-030.1 | 各查询的请求 | 路径同[账户配置核验](README.md#账户配置核验);除 `/time` 外都带 L2 头,每行以 CRLF 结尾、不带 `Content-Type`(账户客户端自己写);`Cancel` 为 `DELETE /order {"orderID"}` |
| PMK-030.2 | `/auth/api-keys` 含测试 key;只含另一把 key | `venue_uid` = 登记的 24 个字符、`uid_keys = 3`;失败 |
| PMK-030.3 | `closed_only` false;true | 通过;失败 |
| PMK-030.4 | 一页一张挂单(游标 `MTAw`);再查到末页 `LTE=` | 1 张、游标 `MTAw`;0 张、游标空 |
| PMK-030.5 | `balance` 82254260 | 计价资产 USDC `free` 82 254 260(1e-6)、`locked` 0 |
| PMK-030.6 | 401 | 失败 |

### PMK-031 低于原始量完成的订单定为终态

- 规则:[累计量与费用来源](README.md#累计量与费用来源)、[下单](README.md#下单)
- 级别:table
- 里程碑:M3
- 前置:同 PMK-027;原始 5 份、价 0.02(查单 0.42)
- 实现:`crates/venue-polymarket/tests/orders.rs::pmk_031_complete_below_size_ends_the_order`

| 行 | 输入 | 期望 |
|---|---|---|
| PMK-031.1 | H1 `UPDATE`,`status` `MATCHED`,`size_matched` 4.99737(2026-10-23 实盘) | `Fill{qty 4 997 370, cum 4 997 370}`,再 `QueryResult{Filled(3), cum 4 997 370, avg_px 0.02}`;均为 slot 7、epoch 5、H1 键;`events 2` |
| PMK-031.2 | 同一帧再送一次 | 无事件 |
| PMK-031.3 | `btc-dn-0`(lot 0.01):`LIVE`,4.99116 | `Fill` + `QueryResult{Filled}`,cum 4 991 160 |
| PMK-031.4 | `btc-dn-0`:`LIVE`,4.99(剩余恰为一个 lot) | 只有 `PartialFill` |
| PMK-031.5 | 未定槽的 H2 `MATCHED` 4.99737;再解码 H2 的 `POST /order` 回执(slot 7、gen 3) | 前者只有 `Fill{UNRESOLVED}`;后者 `Ack{H2 键}`,再 `QueryResult{Filled, cum 4 997 370, avg_px 0.02, slot 7}`,`events 2` |
| PMK-031.6 | 查单:`btc-up-0` `MATCHED` 4.99737;`btc-dn-0` `LIVE` 4.99116;`btc-dn-0` `LIVE` 4.99 | Filled(3);Filled(3);PartiallyFilled(2) |

### PMK-032 回执未到的下单按签出的哈希查单与定槽

- 规则:[下单](README.md#下单)、[用户流事件表](README.md#用户流事件表)
- 级别:unit
- 里程碑:M3
- 前置:同 PMK-023;`GwCtx` 可写地编码 `Place{UP, Buy, 0.42, 5 份, PostOnly}`(slot 7、gen 3、`TargetRef.id` epoch 5),之后不送 REST 回执(2026-10-23 实盘:回执超过 `ack_timeout_ms`,槽进 `Unknown`)
- 实现:`crates/venue-polymarket/tests/orders.rs::pmk_032_unacked_place_is_resolved_by_its_signed_hash`

| 行 | 输入 | 期望 |
|---|---|---|
| PMK-032.1 | `Query`,`exch_order_id` 0,目标 client id 同下单;代改为 4;epoch 改为 6 | `GET /data/order/<H>`,H = 下单体各字段的 `order_digest`;`UnknownTarget`;`UnknownTarget` |
| PMK-032.2 | 用户频道 H `UPDATE` `MATCHED` 4.99737(sink 未绑定 H) | `exch_id(H 键, Some(签出的 client id))`;`Fill{slot 7, gen 3, epoch 5, cum 4 997 370}` + `QueryResult{Filled}` |
| PMK-032.3 | (只编码过下单的 `GwCtx`)查单应答 200 `MATCHED` 4.99737 | `exch_id(H 键, Some(client id))`;`Ack{slot 7, gen 3, exch_order_id = H 键}`,再 `QueryResult{Filled(3), cum 4 997 370, avg_px 0.42, H 键}`;`events 2` |
| PMK-032.4 | 同一应答再解码一次 | 只有 `QueryResult` |

### PMK-033 过期保护按 GTD 寿命声明为 ExpireAt

- 规则:[VenueCaps 默认值](README.md#venuecaps-默认值)、[下单](README.md#下单)
- 级别:unit
- 里程碑:M3
- 前置:同 PMK-023
- 实现:`crates/venue-polymarket/tests/orders.rs::pmk_033_stale_guard_is_the_gtd_lifetime`

| 行 | 输入 | 期望 |
|---|---|---|
| PMK-033.1 | `PolymarketClob::stale_guard()` | `ExpireAt{default_ms: GTD_S × 1000}`,`GTD_S = 300` |
| PMK-033.2 | 编码 `Place{UP, Buy, 0.45, 5 份, PostOnly}` | `expiration` − 时钟秒 = `default_ms / 1000`(声明的窗口就是签出订单的寿命) |

### PMK-034 先撤后到:回执未到的撤单按签出的哈希发出,应答按先后定终态

- 规则:[下单](README.md#下单)(先撤后到)、[结果未知码表](README.md#结果未知码表)、[VenueCaps 默认值](README.md#venuecaps-默认值)
- 级别:table
- 里程碑:M3
- 前置:同 PMK-023;`GwCtx` 可写地编码 `Place{UP, Buy, 0.42, 5 份, PostOnly}`(slot 7、gen 3、`TargetRef.id` epoch 5),回执未到时再编码该槽的 `Cancel`(`exch_order_id` 0);H = 下单体各字段的 `order_digest`;应答文本按 2026-10-28 实测;.2 起每行从前置的 `GwCtx` 副本开始
- 实现:`crates/venue-polymarket/tests/orders.rs::pmk_034_cancel_before_the_place_answer`

| 行 | 输入 | 期望 |
|---|---|---|
| PMK-034.1 | 前置的撤单请求;`default_caps().flags` | `DELETE /order`,体 `{"orderID":"H"}`,带 L2 头;不含 `CANCEL_NEEDS_ACK` |
| PMK-034.2 | 撤单应答 `not_canceled {H: "order can't be found - already canceled or matched"}`;再下单应答 400 `order H is invalid. Already canceled.` | 前者无事件,`raw_code` = 原因的码;后者一条 `CancelAck{slot 7, gen 3, H 键}`,非 `outcome_unknown` |
| PMK-034.3 | 先下单应答 400 `Already canceled.`,再撤单应答"不存在" | `CancelAck`,再 `CancelReject{GONE}` |
| PMK-034.4 | 撤单应答 `canceled: [H]`;再下单应答 200 `live`;再用户频道 H `CANCELLATION` | `CancelAck{slot 7, gen 3, epoch 5, H 键}`,`exch_id(H 键, Some(签出的 client id))`;`Ack`;无事件 |
| PMK-034.5 | 撤单应答"不存在";再下单应答 200 `live`;再编码 `Cancel`(H 键);其应答"不存在" | 下单应答产出 `CancelReject{EARLY(2), slot 7, gen 3, H 键}` 与 `Ack`(依次,`events 2`);再撤单体 `{"orderID":"H"}`;`CancelReject{GONE}` |
| PMK-034.6 | 先下单应答 200 `live`,再撤单应答"不存在" | `Ack`,再 `CancelReject{EARLY}` |
| PMK-034.7 | 撤单应答"不存在";下单应答 500;查单 200 `LIVE` | 下单应答无事件、`outcome_unknown`;查单产出 `CancelReject{EARLY}`、`Ack`、`QueryResult{New}`(均为 slot 7) |
| PMK-034.8 | 撤单应答"不存在";下单应答 200 `matched` | 只有 `Ack` |
| PMK-034.9 | 撤单应答"不存在";下单应答 400 `invalid post-only order: order crosses book` | `Reject` |

### PMK-035 预签名:空闲时签好锚价附近的订单,下单用一次即删,时间戳全账户唯一

- 规则:[预签名](README.md#预签名)、[下单](README.md#下单)
- 级别:table
- 里程碑:M3
- 前置:同 PMK-023;`on` = `Presign::new(1, 1, 10 000, 30 000)`(锚价 ±0.01,寿命 30 s);`Place{UP, Buy, 价, 5 份, PostOnly}`;签名正确 = 请求体的 `signature` 等于测试私钥对体内各字段的存款钱包签名;"空闲到底" = 反复 `gw_idle` 到返回 `false`
- 实现:`crates/venue-polymarket/tests/orders.rs::pmk_035_presigned_orders`

| 行 | 输入 | 期望 |
|---|---|---|
| PMK-035.1 | `Presign::new`:(3, 4, 10 000, 30 000);(4, 4, …);`step_e6` 0;`max_age_ms` 999、240 001 | 前者 `Some`,其余 `None` |
| PMK-035.2 | 未开启:下单 0.42 后 `gw_idle` | `false`;`presigned() = 0` |
| PMK-035.3 | `on`:槽 6 下单 0.42(时钟 T),T + 10 ms 空闲到底 | 下单当场签名、`timestamp` = T、签名正确;空闲签 3 张(0.41、0.42、0.43);`presigned() = 3` |
| PMK-035.4 | 接 .3,T + 20 ms 槽 7 下单 0.43;同一时钟槽 8 再下 0.43;再编码槽 7 的 `Cancel` | 前者用预签名单:签名正确,T < `timestamp` ≤ T + 13,`salt` 低 20 位为 0,`presigned() = 2`;后者当场签名,`timestamp` = T + 20,`salt` 低 20 位 = 8;撤单体的 `orderID` = 预签名单的哈希 |
| PMK-035.5 | 未开启,同一毫秒两次下单 | `timestamp` 依次 T、T + 1 |
| PMK-035.6 | `on`:下单 0.42、空闲到底(时钟 T);T + 22 499、T + 22 504 空闲到底;再 30 001 ms 后下单 0.41 | 3;0(未到 3/4 寿命);3(重签);下单当场签名、`timestamp` = 当时时钟 |
| PMK-035.7 | `on`:下单 0.42、空闲到底;`UP` 改绑另一代币 | `presigned() = 0`;`gw_idle` 返回 `false` |
| PMK-035.8 | `on`:下单 0.99、空闲到底 | 签 2 张(0.98、0.99;1.00 不在 (0, 1) 内) |

### PMK-036 签名与编码基准

- 规则:[延迟与资源契约](README.md#延迟与资源契约)、[预签名](README.md#预签名)
- 级别:bench
- 里程碑:M3
- 前置:同 PMK-023 的私钥与代币;每次迭代时钟前进 1 ms
- 基准名:`pm/encode_place_signed`、`pm/encode_cancel`、`pm/encode_place_presigned`、`pm/presign_step`
- 实现:`crates/venue-polymarket/benches/sign.rs::pmk_036_signing`
- 期望:预算见 README;`gate = "report"`(只报告)
