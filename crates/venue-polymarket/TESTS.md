---
component: venue-polymarket
prefix: PMK
spec: README.md
---

# venue-polymarket 测试规格

格式与规则见 [docs/tests](../../docs/tests/README.md)。

本文件的记号:

- `RD-PM` = 测试内构建的 refdata:`USDC` = AssetId 1(Stable),两个代币资产(px_exp −3,qty_exp −6);`POLYMARKET_CLOB`(15):`btc-up-0` = InstId 1、`btc-dn-0` = InstId 2,tick 0.01,lot 0.000001。
- `T_UP`/`T_DN` = 2026-10-19 录制的 `btc-updown-5m` 窗口的 Up/Down 代币 id;`bound()` = 容量 16 的 `FeedCtx`,`T_UP` → 1、`T_DN` → 2。
- 帧取自 2026-10-19 在 eu-west-1 的独立录制(pmrec);数值写法:价格 `"0.42"` = 420(px_exp −3),数量 `"242.09"` = 242090000(qty_exp −6)。

## 覆盖

| README 小节 | 用例 |
|---|---|
| 概要 | PMK-001 |
| 目录 | 不适用:非规范 |
| 使用本组件 | PMK-001 |
| 端点 | PMK-008 |
| 槽位绑定 | PMK-009, PMK-010, PMK-013, PMK-014 |
| 订阅 | PMK-008, PMK-014 |
| 行情频道 → 事件 | PMK-001–PMK-007, PMK-011, PMK-015 |
| 合成序号 | PMK-003–PMK-005, PMK-010–PMK-012 |
| 多连接去重 | PMK-016, PMK-017 |
| 心跳 | PMK-001 |
| VenueCaps 默认值 | PMK-011 |
| 实现说明 | PMK-006, PMK-007 |
| 延迟与资源契约 | PMK-011 |
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
