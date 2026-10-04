---
component: hft-book
prefix: BOOK
spec: README.md
---

# hft-book 测试规格

格式与规则见 [docs/tests](../../docs/tests/README.md)。本文件的记号(各用例在 `前置` 中引用,数值即定义,不依赖默认值):

- `P` = `BookParams { tick: 10, window: 64, ovf_cap: 4, crossed_tolerance: 0 }`,窗口长 L = 128 档;价格写 raw(资产 px 指数下),档下标 = px / 10。
- `S0` = `L2Snapshot{origin = Exchange(1), snap_id = 100}`,买 `[(10000, 5), (9990, 3)]`,卖 `[(10010, 2), (10020, 4)]`;应用后中价下标 m = 1000,`base = 936`(窗口覆盖价格 9360–10630)。
- `S1` = 同 `S0` 但每侧一档:买 `[(10000, 5)]`,卖 `[(10010, 2)]`,`snap_id = 100`。
- `Δ(prev, last, side, px, qty, idx[, LAST])` = 一条 `L2Delta`;`B(prev, last; 买…; 卖…)` = 一条 `L2Batch`(`n_bids`、`n_asks` 与档数一致,除注明外)。
- `T(snap_id; 买…; 卖…)` = 一条 `L2Snapshot{origin = TopN(3)}`。
- Outcome 写成位名的或;`Stale(r)` 表示 `state() == Stale` 且 `meta().stale_reason == r`。

## 覆盖

| README 小节 | 用例 |
|---|---|
| 概要 | 不适用:非规范 |
| 目录 | 不适用:非规范 |
| 使用本组件 | 不适用:见子节 |
| 决定:簿只接受"已归一化"的增量,但自己做通用校验 | BOOK-030–BOOK-037, BOOK-061, BOOK-106 |
| API | BOOK-001–BOOK-004, BOOK-010–BOOK-025, BOOK-030–BOOK-049, BOOK-065–BOOK-083, BOOK-105 |
| 状态机 | BOOK-013, BOOK-017, BOOK-023, BOOK-032, BOOK-037, BOOK-065–BOOK-067 |
| 实现说明 | BOOK-034, BOOK-038, BOOK-050–BOOK-062 |
| 延迟与资源契约 | BOOK-090–BOOK-103 |
| 测试与验收 | 不适用:非规范 |
| 依赖 | 不适用:非规范 |
| 延伸阅读 | 不适用:非规范 |
| 已知限制与后续工作 | 不适用:非规范 |
| 开发备忘 | 不适用:非规范 |

## 用例

### BOOK-001 BookParams::from_inst 取 refdata 的 tick、窗口与交叉容忍,溢出区容量默认 64

- 规则:[API](README.md#api)
- 级别:unit
- 里程碑:M1
- 前置:`InstHot { tick: 10, book_window: 2048, crossed_tolerance: 2, … }`(其余字段任意)
- 步骤:
  1. `BookParams::from_inst(&h)`
- 期望:
  - `BookParams { tick: 10, window: 2048, ovf_cap: 64, crossed_tolerance: 2 }`
- 实现:`crates/hft-book/tests/params.rs::book_001_from_inst_defaults`

### BOOK-002 new_in 校验内存大小、对齐与参数

- 规则:[API](README.md#api)
- 级别:table
- 里程碑:M1
- 前置:参数除注明外为 `P`;`n = L2Book::bytes_needed(&p)`;内存来自一块 64 B 对齐的堆缓冲,"错位"指起点 + 8 B
- 实现:`crates/hft-book/tests/params.rs::book_002_new_in_checks`

| 行 | 参数与内存 | 期望 |
|---|---|---|
| BOOK-002.1 | `P`,长度 n,对齐 | `Ok`,`state() == Uninit` |
| BOOK-002.2 | `P`,长度 n − 1 | `Err(BookErr::MemTooSmall)` |
| BOOK-002.3 | `P`,长度 n + 64,错位 | `Err(BookErr::MemAlign)` |
| BOOK-002.4 | `tick = 0` | `Err(BookErr::BadParams)` |
| BOOK-002.5 | `tick = -10` | `Err(BookErr::BadParams)` |
| BOOK-002.6 | `window = 63` | `Err(BookErr::BadParams)` |
| BOOK-002.7 | `window = 64` | `Ok` |
| BOOK-002.8 | `window = 65536` | `Ok` |
| BOOK-002.9 | `window = 65537` | `Err(BookErr::BadParams)` |
| BOOK-002.10 | `ovf_cap = 0` | `Err(BookErr::BadParams)` |
| BOOK-002.11 | `ovf_cap = 1` | `Ok` |

### BOOK-003 新建的簿是空的 Uninit 簿

- 规则:[API](README.md#api)
- 级别:unit
- 里程碑:M1
- 前置:`P`;分别以 `L2Book::new(&P)` 与 `new_in` 建两个簿
- 步骤:
  1. 读取 `state()`、`last_id()`、`best_bid()`、`best_ask()`、`meta()`、`state_hash()`
- 期望:
  - `Uninit`、0、`None`、`None`;`meta()` 的 `state = 0`、`stale_reason = 0`、`origin = 0`、`truncated = 0`、`crossed_run = 0`
  - 两个簿的 `state_hash()` 相等
- 实现:`crates/hft-book/tests/params.rs::book_003_new_book_is_empty`

### BOOK-004 BookSet::new_in 只为订阅 L2 的 inst 分配簿并拒绝坏的 inst 列表

- 规则:[API](README.md#api)
- 级别:table
- 里程碑:M1
- 前置:refdata 含 inst 1–5(各 `book_window = 64`、`tick = 10`);内存为足量 64 B 对齐缓冲(除注明外)
- 实现:`crates/hft-book/tests/set.rs::book_004_bookset_new_in`

| 行 | l2_insts 与内存 | 期望 |
|---|---|---|
| BOOK-004.1 | `[2, 4]` | `Ok`;`l2(2)`、`l2(4)` 为 `Some`,`l2(1)`、`l2(3)`、`l2(5)` 为 `None`;`top(1..=5)` 全部字段为 0 |
| BOOK-004.2 | `[]` | `Ok`;五个 `l2(i)` 都为 `None` |
| BOOK-004.3 | `[2, 9]` | `Err(BookErr::UnknownInst)` |
| BOOK-004.4 | `[2, 2]` | `Err(BookErr::DupInst)` |
| BOOK-004.5 | `[2, 4]`,内存 16 B | `Err(BookErr::MemTooSmall)` |

### BOOK-010 Exchange 快照把 Uninit 簿带到 Live 并以中价设窗口基准

- 规则:[API](README.md#api)、[状态机](README.md#状态机)
- 级别:unit
- 里程碑:M1
- 前置:`P`;新簿
- 步骤:
  1. `apply_snapshot(S0)`
- 期望:
  - 返回 `APPLIED | WENT_LIVE`
  - `state() == Live`、`last_id() == 100`、`best_bid() == Some((10000, 5))`、`best_ask() == Some((10010, 2))`
  - `meta()`:`base = 936`、`origin = 1`、`stale_reason = 0`
- 实现:`crates/hft-book/tests/snapshot.rs::book_010_exchange_snapshot_goes_live`

### BOOK-011 一侧为空的快照用另一侧最优定窗口

- 规则:[API](README.md#api)
- 级别:table
- 里程碑:M1
- 前置:`P`;新簿;`snap_id = 100`
- 实现:`crates/hft-book/tests/snapshot.rs::book_011_one_sided_snapshot`

| 行 | 快照 | 期望 |
|---|---|---|
| BOOK-011.1 | 买 `[]`,卖 `[(10010, 2)]` | `Live`;`best_bid() == None`;`meta().base == 937` |
| BOOK-011.2 | 买 `[(10000, 5)]`,卖 `[]` | `Live`;`best_ask() == None`;`meta().base == 936` |
| BOOK-011.3 | 买 `[]`,卖 `[]` | `Live`;两侧 `None`;`meta().base == 0`(两侧都空,base 不变) |

### BOOK-012 交叉或带坏档的快照整条不应用并使簿 Stale

- 规则:[API](README.md#api)、[状态机](README.md#状态机)、[实现说明](README.md#实现说明)
- 级别:table
- 里程碑:M1
- 前置:`P`;每行分别在新簿(Uninit)与已应用 `S0` 的 Live 簿上执行;快照 `snap_id = 200`
- 实现:`crates/hft-book/tests/snapshot.rs::book_012_rejected_snapshot`

| 行 | 快照档 | Uninit 簿上的期望 | Live 簿上的期望 |
|---|---|---|---|
| BOOK-012.1 | 买 `[(10010, 1)]`,卖 `[(10010, 1)]`(买 = 卖) | `INVALID`;`Stale(Crossed)`;`last_id() == 0`;无档 | `INVALID \| WENT_STALE`;`Stale(Crossed)`;`last_id() == 100`;档与 S0 相同 |
| BOOK-012.2 | 买 `[(10020, 1)]`,卖 `[(10010, 1)]` | 同上 | 同上 |
| BOOK-012.3 | 买 `[(10005, 1)]`(非 tick 倍数) | `INVALID`;`Stale(BadLevel)`;无档 | `INVALID \| WENT_STALE`;`Stale(BadLevel)`;档与 S0 相同 |
| BOOK-012.4 | 买 `[(0, 1)]` | `INVALID`;`Stale(BadLevel)` | `INVALID \| WENT_STALE`;`Stale(BadLevel)` |
| BOOK-012.5 | 卖 `[(10010, -1)]` | `INVALID`;`Stale(BadLevel)` | `INVALID \| WENT_STALE`;`Stale(BadLevel)` |

### BOOK-013 快照中 qty = 0 的档被跳过,同价重复时后者生效

- 规则:[API](README.md#api)、[实现说明](README.md#实现说明)
- 级别:unit
- 里程碑:M1
- 前置:`P`;新簿
- 步骤:
  1. `apply_snapshot`:`snap_id = 100`,买 `[(10000, 0), (9990, 3), (9990, 7)]`,卖 `[(10010, 2)]`
- 期望:
  - 返回 `APPLIED | WENT_LIVE`;`best_bid() == Some((9990, 7))`;`iter(Buy)` 只产出 `(9990, 7)`
- 实现:`crates/hft-book/tests/snapshot.rs::book_013_snapshot_zero_and_dup_levels`

### BOOK-014 后到的 Exchange 快照整体替换旧档

- 规则:[API](README.md#api)
- 级别:unit
- 里程碑:M1
- 前置:`P`;已应用 `S0` 的 Live 簿
- 步骤:
  1. `apply_snapshot`:`origin = Exchange`,`snap_id = 200`,买 `[(10100, 1)]`,卖 `[(10110, 1)]`
- 期望:
  - 返回 `APPLIED`(不含 `WENT_LIVE`);`last_id() == 200`
  - `iter(Buy)` 只产出 `(10100, 1)`,`iter(Sell)` 只产出 `(10110, 1)`(9990、10000、10010、10020 都不再出现)
- 实现:`crates/hft-book/tests/snapshot.rs::book_014_snapshot_replaces_levels`

### BOOK-015 Stale 与 Resyncing 簿收到有效 Exchange 快照回到 Live

- 规则:[API](README.md#api)、[状态机](README.md#状态机)
- 级别:table
- 里程碑:M1
- 前置:`P`;已应用 `S0`;按"起始状态"列先把簿带到该状态
- 实现:`crates/hft-book/tests/snapshot.rs::book_015_snapshot_from_non_live`

| 行 | 起始状态(如何得到) | 期望(应用 `snap_id = 200` 的 S0 档) |
|---|---|---|
| BOOK-015.1 | `Stale(Gap)`(`Δ(99, 101, Buy, 10000, 1, 0, LAST)`) | `APPLIED \| WENT_LIVE`;`Live`;`stale_reason == 0`;`last_id() == 200` |
| BOOK-015.2 | `Resyncing`(`on_conn(Resync)`) | `APPLIED \| WENT_LIVE`;`Live` |
| BOOK-015.3 | `Stale(ConnDown)`(`on_conn(Down)`) | `APPLIED \| WENT_LIVE`;`Live` |

### BOOK-016 恢复后应用 Keyframe 快照只写档位,不改恢复出的元数据

- 规则:[API](README.md#api)、[实现说明](README.md#实现说明)
- 级别:unit
- 里程碑:M1
- 前置:`P`;BookSet 含 inst 7 的 L2 簿;节 6 行 `{inst 7, state Live, stale_reason 0, origin 3, last_id 100, base 930, truncated 5, crossed_run 1}` 经 `restore` 读入
- 步骤:
  1. `apply_snapshot`:`origin = Keyframe(2)`,`snap_id = 100`,买 `[(10000, 5)]`,卖 `[(10010, 2)]`
  2. 再 `apply_batch(B(100, 101; (10000, 6); ))`
- 期望:
  - 步骤 1 后 `state() == Live`;`meta()` 的 `origin = 3`、`base = 930`、`truncated = 5`、`crossed_run = 1`、`last_id = 100`;`best_bid() == Some((10000, 5))`
  - 步骤 2 返回 `INVALID`(最近快照仍为 TopN),状态与档不变
- 实现:`crates/hft-book/tests/snapshot.rs::book_016_keyframe_keeps_restored_meta`

### BOOK-017 未经 restore 的 Keyframe 快照按 Exchange 规则自举

- 规则:[API](README.md#api)、[状态机](README.md#状态机)
- 级别:unit
- 里程碑:M1
- 前置:`P`;新簿
- 步骤:
  1. `apply_snapshot`:`origin = Keyframe(2)`,档同 `S0`,`snap_id = 100`
  2. `apply_batch(B(100, 101; (10000, 6); ))`
- 期望:
  - 步骤 1 返回 `APPLIED | WENT_LIVE`;`meta().origin == 2`、`base == 936`
  - 步骤 2 返回 `APPLIED | BID_TOP`;`last_id() == 101`
- 实现:`crates/hft-book/tests/snapshot.rs::book_017_keyframe_bootstrap`

### BOOK-020 连续的分档快照每条整替换

- 规则:[API](README.md#api)
- 级别:unit
- 里程碑:M1
- 前置:`P`;新簿
- 步骤:
  1. `apply_snapshot(T(10; (10000, 5), (9990, 3); (10010, 2), (10020, 4)))`
  2. `apply_snapshot(T(11; (10000, 4); (10010, 1)))`
- 期望:
  - 步骤 1 返回 `APPLIED | WENT_LIVE`;`meta().origin == 3`
  - 步骤 2 返回 `APPLIED`;`iter(Buy)` 只产出 `(10000, 4)`、`iter(Sell)` 只产出 `(10010, 1)`;`last_id() == 11`
- 实现:`crates/hft-book/tests/topn.rs::book_020_topn_full_replace`

### BOOK-021 分档快照按 snap_id 去重,0 不去重

- 规则:[API](README.md#api)
- 级别:table
- 里程碑:M1
- 前置:`P`;已应用 `T(11; (10000, 4); (10010, 1))` 的 Live 簿;每行独立,快照档为买 `[(10000, 9)]`、卖 `[(10010, 9)]`
- 实现:`crates/hft-book/tests/topn.rs::book_021_topn_dedup`

| 行 | snap_id | 期望 |
|---|---|---|
| BOOK-021.1 | 10 | `DUPLICATE`;档不变;`last_id() == 11` |
| BOOK-021.2 | 11 | `DUPLICATE`;档不变 |
| BOOK-021.3 | 12 | `APPLIED`;`best_bid() == Some((10000, 9))`;`last_id() == 12` |
| BOOK-021.4 | 0 | `APPLIED`;`best_bid() == Some((10000, 9))`;`last_id() == 0` |

### BOOK-022 分档快照簿拒收增量且不转状态

- 规则:[API](README.md#api)
- 级别:table
- 里程碑:M1
- 前置:`P`;已应用 `T(11; (10000, 4); (10010, 1))`;每行独立
- 实现:`crates/hft-book/tests/topn.rs::book_022_topn_rejects_increments`

| 行 | 调用 | 期望 |
|---|---|---|
| BOOK-022.1 | `apply_batch(B(11, 12; (10000, 1); ))` | `INVALID`;`Live`;`best_bid() == Some((10000, 4))`;`last_id() == 11` |
| BOOK-022.2 | `apply_delta(Δ(11, 12, Buy, 10000, 1, 0, LAST))` | `INVALID`;`Live`;档不变 |
| BOOK-022.3 | 先 `on_conn(Down)`,再 `apply_batch(B(11, 12; (10000, 1); ))` | `INVALID`;`Stale(ConnDown)`(仍是 TopN 簿,先于 NOT_LIVE 判定) |

### BOOK-023 分档快照簿断连后下一条快照直接回到 Live

- 规则:[API](README.md#api)、[状态机](README.md#状态机)
- 级别:unit
- 里程碑:M1
- 前置:`P`;已应用 `T(11; (10000, 4); (10010, 1))`
- 步骤:
  1. `on_conn(Down)`
  2. `apply_snapshot(T(12; (10000, 5); (10010, 2)))`
- 期望:
  - 步骤 1 返回 `WENT_STALE`;`Stale(ConnDown)`
  - 步骤 2 返回 `APPLIED | WENT_LIVE`;`Live`(未经过 Resyncing);`last_id() == 12`
- 实现:`crates/hft-book/tests/topn.rs::book_023_topn_down_then_live`

### BOOK-024 分档快照在中价未越出窗口时保留 base

- 规则:[API](README.md#api)、[实现说明](README.md#实现说明)
- 级别:table
- 里程碑:M1
- 前置:`P`;已应用 `T(10; (10000, 5); (10010, 2))`(base 936);每行在此基础上应用一条 `T(11; 买; 卖)`
- 实现:`crates/hft-book/tests/topn.rs::book_024_topn_base_kept`

| 行 | 买 | 卖 | 中价下标 m | 期望 `meta().base` |
|---|---|---|---|---|
| BOOK-024.1 | (10300, 1) | (10310, 1) | 1030(m − base = 94) | 936 |
| BOOK-024.2 | (10310, 1) | (10320, 1) | 1031(95) | 936 |
| BOOK-024.3 | (10320, 1) | (10330, 1) | 1032(96) | 968 |
| BOOK-024.4 | (9680, 1) | (9690, 1) | 968(32) | 936 |
| BOOK-024.5 | (9670, 1) | (9680, 1) | 967(31) | 903 |

### BOOK-025 交叉的分档快照不享受交叉容忍

- 规则:[API](README.md#api)
- 级别:unit
- 里程碑:M1
- 前置:`BookParams { tick: 10, window: 64, ovf_cap: 4, crossed_tolerance: 5 }`;已应用 `T(10; (10000, 5); (10010, 2))`
- 步骤:
  1. `apply_snapshot(T(11; (10010, 1); (10010, 1)))`
- 期望:
  - 返回 `INVALID | WENT_STALE`;`Stale(Crossed)`;`best_bid() == Some((10000, 5))`;`last_id() == 10`
- 实现:`crates/hft-book/tests/topn.rs::book_025_topn_crossed_no_tolerance`

### BOOK-030 单档增量消息应用后推进 last_id

- 规则:[决定:簿只接受已归一化的增量](README.md#决定簿只接受已归一化的增量但自己做通用校验)、[API](README.md#api)
- 级别:unit
- 里程碑:M1
- 前置:`P`;已应用 `S0`
- 步骤:
  1. `apply_delta(Δ(100, 101, Sell, 10010, 1, 0, LAST))`
- 期望:
  - 返回 `APPLIED | ASK_TOP`;`best_ask() == Some((10010, 1))`;`last_id() == 101`
- 实现:`crates/hft-book/tests/delta.rs::book_030_single_level_message`

### BOOK-031 多档消息在 LAST 档才推进 last_id

- 规则:[决定:簿只接受已归一化的增量](README.md#决定簿只接受已归一化的增量但自己做通用校验)、[实现说明](README.md#实现说明)
- 级别:unit
- 里程碑:M1
- 前置:`P`;已应用 `S0`
- 步骤:
  1. `apply_delta(Δ(100, 101, Buy, 9990, 0, 0))`
  2. `apply_delta(Δ(100, 101, Sell, 10020, 7, 1))`
  3. `apply_delta(Δ(100, 101, Buy, 10000, 6, 2, LAST))`
- 期望:
  - 步骤 1 返回 `APPLIED`,`last_id() == 100`;步骤 2 返回 `APPLIED`,`last_id() == 100`
  - 步骤 3 返回 `APPLIED | BID_TOP`,`last_id() == 101`;`iter(Buy)` 产出 `(10000, 6)`;`iter(Sell)` 产出 `(10010, 2)`、`(10020, 7)`
- 实现:`crates/hft-book/tests/delta.rs::book_031_multi_level_message`

### BOOK-032 prev_id 与 last_id 不等即 GAP 并转 Stale

- 规则:[决定:簿只接受已归一化的增量](README.md#决定簿只接受已归一化的增量但自己做通用校验)、[状态机](README.md#状态机)
- 级别:table
- 里程碑:M1
- 前置:`P`;已应用 `S0`(`last_id = 100`);每行独立;增量 `Δ(prev, 105, Buy, 10000, 1, 0, LAST)`
- 实现:`crates/hft-book/tests/delta.rs::book_032_gap`

| 行 | prev | 期望 |
|---|---|---|
| BOOK-032.1 | 100 | `APPLIED \| BID_TOP`;`Live`;`last_id() == 105` |
| BOOK-032.2 | 101 | `GAP \| WENT_STALE`;`Stale(Gap)`;`best_bid() == Some((10000, 5))`(未写入);`last_id() == 100` |
| BOOK-032.3 | 99 | `GAP \| WENT_STALE`;`Stale(Gap)` |

### BOOK-033 已应用过的消息按重复整条丢弃

- 规则:[决定:簿只接受已归一化的增量](README.md#决定簿只接受已归一化的增量但自己做通用校验)
- 级别:unit
- 里程碑:M1
- 前置:`P`;已应用 `S0` 与消息 `Δ(100, 101, Buy, 10000, 6, 0)`、`Δ(100, 101, Sell, 10010, 1, 1, LAST)`
- 步骤:
  1. 再次投递同一消息的两档
  2. 投递 `Δ(99, 100, Buy, 9990, 9, 0, LAST)`(last_id = 100)
  3. 投递 `Δ(101, 102, Buy, 10000, 7, 0, LAST)`
- 期望:
  - 步骤 1 两档都返回 `DUPLICATE`;步骤 2 返回 `DUPLICATE`;两步之后档与 `last_id() == 101` 不变,`best_bid() == Some((10000, 6))`
  - 步骤 3 返回 `APPLIED | BID_TOP`;`last_id() == 102`
- 实现:`crates/hft-book/tests/delta.rs::book_033_duplicate_message`

### BOOK-034 消息被打断、跳序或 last_id 不一致判为 Split

- 规则:[决定:簿只接受已归一化的增量](README.md#决定簿只接受已归一化的增量但自己做通用校验)、[实现说明](README.md#实现说明)
- 级别:table
- 里程碑:M1
- 前置:`P`;已应用 `S0`;每行按顺序投递,期望针对最后一条
- 实现:`crates/hft-book/tests/delta.rs::book_034_split`

| 行 | 投递序列 | 期望(最后一条) |
|---|---|---|
| BOOK-034.1 | `Δ(100, 101, Buy, 10000, 6, 1)`(不在消息中而 idx = 1) | `INVALID \| WENT_STALE`;`Stale(Split)` |
| BOOK-034.2 | `Δ(100, 101, Buy, 10000, 6, 0)`、`Δ(100, 101, Sell, 10010, 1, 2)`(跳过 idx 1) | `INVALID \| WENT_STALE`;`Stale(Split)` |
| BOOK-034.3 | `Δ(100, 101, Buy, 10000, 6, 0)`、`Δ(100, 102, Sell, 10010, 1, 1)`(last_id 变了) | `INVALID \| WENT_STALE`;`Stale(Split)` |
| BOOK-034.4 | `Δ(100, 101, Buy, 10000, 6, 0)`、`Δ(101, 102, Buy, 9990, 1, 0, LAST)`(上一条未见 LAST) | `INVALID \| WENT_STALE`;`Stale(Split)` |
| BOOK-034.5 | `Δ(100, 101, Buy, 10000, 6, 0)`、`Δ(100, 101, Sell, 10010, 1, 1, LAST)`(对照行) | `APPLIED \| ASK_TOP`;`Live`;`last_id() == 101` |

### BOOK-035 结构非法的档使簿 Stale,同一消息的剩余档 NOT_LIVE

- 规则:[决定:簿只接受已归一化的增量](README.md#决定簿只接受已归一化的增量但自己做通用校验)、[实现说明](README.md#实现说明)
- 级别:table
- 里程碑:M1
- 前置:`P`;已应用 `S0`;每行先投递 `Δ(100, 101, Buy, 9990, 4, 0)`(返回 `APPLIED`),再投递该行的第二档(idx 1),再投递 `Δ(100, 101, Sell, 10010, 1, 2, LAST)`
- 实现:`crates/hft-book/tests/delta.rs::book_035_bad_level`

| 行 | 第二档 | 期望 |
|---|---|---|
| BOOK-035.1 | `(Buy, 10005, 1)`(非 tick 倍数) | 第二档 `INVALID \| WENT_STALE`;`Stale(BadLevel)`;第三档 `NOT_LIVE`;`last_id() == 100` |
| BOOK-035.2 | `(Buy, 0, 1)` | 同上 |
| BOOK-035.3 | `(Buy, -10, 1)` | 同上 |
| BOOK-035.4 | `(Sell, 10010, -1)` | 同上 |
| BOOK-035.5 | `(Sell, 10010, 0)`(对照:删档) | 第二档 `APPLIED \| ASK_TOP`;第三档 `APPLIED \| ASK_TOP`;`Live` |

### BOOK-036 删除不存在的档是空操作

- 规则:[决定:簿只接受已归一化的增量](README.md#决定簿只接受已归一化的增量但自己做通用校验)、[实现说明](README.md#实现说明)
- 级别:unit
- 里程碑:M1
- 前置:`P`;已应用 `S0`
- 步骤:
  1. `apply_delta(Δ(100, 101, Buy, 9500, 0, 0, LAST))`
- 期望:
  - 返回 `APPLIED`(无 `BID_TOP`);`iter(Buy)` 仍为 `(10000, 5)`、`(9990, 3)`;`last_id() == 101`
- 实现:`crates/hft-book/tests/delta.rs::book_036_delete_missing_level`

### BOOK-037 非 Live 簿丢弃增量

- 规则:[决定:簿只接受已归一化的增量](README.md#决定簿只接受已归一化的增量但自己做通用校验)、[状态机](README.md#状态机)
- 级别:table
- 里程碑:M1
- 前置:`P`;增量 `Δ(100, 101, Buy, 10000, 1, 0, LAST)` 与批 `B(100, 101; (10000, 1); )`
- 实现:`crates/hft-book/tests/delta.rs::book_037_not_live_drops`

| 行 | 簿状态(如何得到) | 期望(增量与批各一次) |
|---|---|---|
| BOOK-037.1 | Uninit(新簿) | 两次都 `NOT_LIVE`;`last_id() == 0` |
| BOOK-037.2 | Stale(应用 S0 后 `mark_stale(Refdata)`) | 两次都 `NOT_LIVE`;档与 S0 相同 |
| BOOK-037.3 | Resyncing(应用 S0 后 `on_conn(Resync)`) | 两次都 `NOT_LIVE` |

### BOOK-038 删除最优档后位图跨字找到下一档

- 规则:[实现说明](README.md#实现说明)
- 级别:unit
- 里程碑:M1
- 前置:`P`;`apply_snapshot`:`snap_id = 100`,买 `[(10000, 5), (9360, 2)]`(9360 = 窗口首档,与 10000 相隔 64 档、不同位图字),卖 `[(10010, 2)]`
- 步骤:
  1. `apply_delta(Δ(100, 101, Buy, 10000, 0, 0, LAST))`
- 期望:
  - 返回 `APPLIED | BID_TOP`(不含 `RECENTERED`:中价下标 968,m − base = 32);`best_bid() == Some((9360, 2))`
- 实现:`crates/hft-book/tests/delta.rs::book_038_best_remove_bitmap_scan`

### BOOK-039 交叉在容忍内只报 CROSSED,超出即 Stale,不交叉清零计数

- 规则:[实现说明](README.md#实现说明)、[API](README.md#api)
- 级别:table
- 里程碑:M1
- 前置:`BookParams { tick: 10, window: 64, ovf_cap: 4, crossed_tolerance: 2 }`;已应用 `S1`;按行顺序投递(单档消息,id 依次递增)
- 实现:`crates/hft-book/tests/delta.rs::book_039_crossed_tolerance`

| 行 | 消息 | 期望 |
|---|---|---|
| BOOK-039.1 | 买 10010 量 1(买 = 卖,交叉) | `APPLIED \| BID_TOP \| CROSSED`;`Live`;`meta().crossed_run == 1` |
| BOOK-039.2 | 买 10010 量 2(仍交叉) | `APPLIED \| BID_TOP \| CROSSED`;`Live`;`crossed_run == 2` |
| BOOK-039.3 | 买 10010 量 0(不交叉) | `APPLIED \| BID_TOP`;`crossed_run == 0` |
| BOOK-039.4 | 买 10020 量 1 | `crossed_run == 1`,`Live` |
| BOOK-039.5 | 买 10020 量 2 | `crossed_run == 2`,`Live` |
| BOOK-039.6 | 买 10020 量 3 | `APPLIED \| BID_TOP \| CROSSED \| WENT_STALE`;`Stale(Crossed)`;`crossed_run == 3` |

### BOOK-040 交叉容忍为 0 时第一次交叉即 Stale

- 规则:[实现说明](README.md#实现说明)、[开发备忘](README.md#开发备忘)
- 级别:unit
- 里程碑:M1
- 前置:`P`(`crossed_tolerance = 0`);已应用 `S1`
- 步骤:
  1. `apply_delta(Δ(100, 101, Buy, 10010, 1, 0, LAST))`
- 期望:
  - 返回 `APPLIED | BID_TOP | CROSSED | WENT_STALE`;`Stale(Crossed)`
- 实现:`crates/hft-book/tests/delta.rs::book_040_zero_tolerance`

### BOOK-041 只有最优档的价或量变化才置 TOP 位

- 规则:[API](README.md#api)
- 级别:table
- 里程碑:M1
- 前置:`P`;已应用 `S0`;每行一条单档消息 `Δ(100, 101, …, LAST)`,每行独立
- 实现:`crates/hft-book/tests/delta.rs::book_041_top_bits`

| 行 | 档 | 期望 |
|---|---|---|
| BOOK-041.1 | `(Buy, 10000, 9)`(最优档改量) | `APPLIED \| BID_TOP` |
| BOOK-041.2 | `(Buy, 9990, 9)`(非最优档) | `APPLIED` |
| BOOK-041.3 | `(Sell, 10010, 2)`(最优卖档,量不变) | `APPLIED` |
| BOOK-041.4 | `(Buy, 9980, 1)`(新增非最优档) | `APPLIED` |
| BOOK-041.5 | `(Sell, 10000, 1)`(更优卖价,与买 10000 交叉) | `APPLIED \| ASK_TOP \| CROSSED \| WENT_STALE` |

### BOOK-045 L2Batch 整条应用并只在批末计算一次最优档

- 规则:[API](README.md#api)、[实现说明](README.md#实现说明)
- 级别:unit
- 里程碑:M1
- 前置:`P`;已应用 `S0`
- 步骤:
  1. `apply_batch(B(100, 101; (10000, 0), (9990, 3), (9980, 1); (10010, 1)))`
- 期望:
  - 返回 `APPLIED | BID_TOP | ASK_TOP`;`best_bid() == Some((9990, 3))`、`best_ask() == Some((10010, 1))`;`last_id() == 101`
- 实现:`crates/hft-book/tests/batch.rs::book_045_batch_applies_whole_message`

### BOOK-046 含一个非法档的 L2Batch 整条不应用

- 规则:[API](README.md#api)
- 级别:unit
- 里程碑:M1
- 前置:`P`;已应用 `S0`
- 步骤:
  1. `apply_batch(B(100, 101; (10000, 9), (9985, 1); (10010, 1)))`(第二档非 tick 倍数)
- 期望:
  - 返回 `INVALID | WENT_STALE`;`Stale(BadLevel)`;`last_id() == 100`
  - `iter(Buy)` 仍为 `(10000, 5)`、`(9990, 3)`,`iter(Sell)` 仍为 `(10010, 2)`、`(10020, 4)`(第一档的 9 与卖档的 1 都没有写入)
- 实现:`crates/hft-book/tests/batch.rs::book_046_batch_atomic_reject`

### BOOK-047 L2Batch 的重复、缺口、非 Live 与档数不符

- 规则:[API](README.md#api)、[实现说明](README.md#实现说明)
- 级别:table
- 里程碑:M1
- 前置:`P`;已应用 `S0`(`last_id = 100`);每行独立
- 实现:`crates/hft-book/tests/batch.rs::book_047_batch_failures`

| 行 | 调用 | 期望 |
|---|---|---|
| BOOK-047.1 | `B(99, 100; (10000, 9); )` | `DUPLICATE`;档不变 |
| BOOK-047.2 | `B(100, 100; (10000, 9); )` | `DUPLICATE` |
| BOOK-047.3 | `B(101, 102; (10000, 9); )` | `GAP \| WENT_STALE`;`Stale(Gap)`;`best_bid() == Some((10000, 5))` |
| BOOK-047.4 | `B(100, 101; (10000, 9); )` 而 `n_bids = 2` | `INVALID \| WENT_STALE`;`Stale(BadLevel)`;档不变 |
| BOOK-047.5 | 先 `on_conn(Resync)`,再 `B(100, 101; (10000, 9); )` | `NOT_LIVE` |

### BOOK-048 同一消息流以批与逐档喂入,每条消息后的簿相同

- 规则:[API](README.md#api)
- 级别:prop
- 里程碑:M1
- 性质:同一快照之后的消息序列,一份以 `apply_batch` 每条消息一次、一份拆成 `L2Delta` 逐档喂入;在第一条非法消息之前,每条消息结束后两簿 `state_hash()`、`last_id()`、`state()`、`meta()` 相同,批的 `CROSSED`/`RECENTERED`/`TRUNCATED` 位等于逐档各次返回位的并集,批的 `BID_TOP`/`ASK_TOP` 位当且仅当该侧最优档(价, 量)在消息前后不同;注入的非法消息在批路径整条不写入(最优档不变)、在逐档路径于非法档转 Stale,两簿都为 `Stale`,之后两路都只返回 `NOT_LIVE`
- 生成:`window ∈ {64, 256}`、`ovf_cap ∈ {1, 4, 64}`;1–200 条消息,每条 1–20 档,价格在中价 ±3W 内(含窗口外与跨窗口大跳),5% 的消息含一个非法档
- 参考:两路实现互为参考
- 例数:PR 1 000
- 实现:`crates/hft-book/tests/prop_batch.rs::book_048_batch_equals_deltas`

### BOOK-049 批中先改后还原最优档不置 TOP 位

- 规则:[API](README.md#api)
- 级别:unit
- 里程碑:M1
- 前置:`P`;已应用 `S0`
- 步骤:
  1. `apply_batch(B(100, 101; (10000, 0), (10000, 5); ))`(同价先删后恢复原量)
- 期望:
  - 返回 `APPLIED`(不含 `BID_TOP`);`best_bid() == Some((10000, 5))`;`last_id() == 101`
- 实现:`crates/hft-book/tests/batch.rs::book_049_batch_top_bits_net`

### BOOK-050 窗口外的档进入溢出区并按价序可见

- 规则:[实现说明](README.md#实现说明)
- 级别:unit
- 里程碑:M1
- 前置:`P`;已应用 `S0`(窗口价格 9360–10630)
- 步骤:
  1. `apply_batch(B(100, 101; (9000, 1), (9350, 2); (10700, 3)))`
- 期望:
  - 返回 `APPLIED`(不含 `TRUNCATED`、`RECENTERED`)
  - `iter(Buy)` 依次产出 `(10000, 5)`、`(9990, 3)`、`(9350, 2)`、`(9000, 1)`;`iter(Sell)` 依次产出 `(10010, 2)`、`(10020, 4)`、`(10700, 3)`
- 实现:`crates/hft-book/tests/overflow.rs::book_050_overflow_visible`

### BOOK-051 溢出区满时离中价最远的档并入远端并计数

- 规则:[实现说明](README.md#实现说明)、[已知限制与后续工作](README.md#已知限制与后续工作)
- 级别:table
- 里程碑:M1
- 前置:`P`(`ovf_cap = 4`);已应用 `S1`;按行顺序各投递一条单档买单批 `B(k, k+1; (px, 1); )`
- 实现:`crates/hft-book/tests/overflow.rs::book_051_overflow_eviction`

| 行 | px | 期望返回 | 期望 `iter(Buy)` 的溢出部分 | `meta().truncated` |
|---|---|---|---|---|
| BOOK-051.1 | 9000 | `APPLIED` | 9000 | 0 |
| BOOK-051.2 | 8990 | `APPLIED` | 9000、8990 | 0 |
| BOOK-051.3 | 8980 | `APPLIED` | 9000、8990、8980 | 0 |
| BOOK-051.4 | 8970 | `APPLIED` | 9000、8990、8980、8970 | 0 |
| BOOK-051.5 | 8960 | `APPLIED \| TRUNCATED` | 9000、8990、8980、8970(8960 并入远端) | 1 |
| BOOK-051.6 | 9010 | `APPLIED \| TRUNCATED` | 9010、9000、8990、8980(8970 被挤出) | 2 |
| BOOK-051.7 | 8960 量 0(删除已并入远端的档) | `APPLIED` | 不变 | 2 |

### BOOK-052 溢出区的最优档触发重新居中

- 规则:[实现说明](README.md#实现说明)
- 级别:unit
- 里程碑:M1
- 前置:`P`;已应用 `S1`(base 936)
- 步骤:
  1. `apply_batch(B(100, 101; (10000, 0), (9000, 1); ))`(窗口内买档清空,只剩溢出区 9000)
- 期望:
  - 返回包含 `APPLIED | BID_TOP | RECENTERED`;`best_bid() == Some((9000, 1))`;`meta().base == 886`(中价下标 ⌊(900 + 1001)/2⌋ = 950,950 − 64)
- 实现:`crates/hft-book/tests/overflow.rs::book_052_best_in_overflow_recenters`

### BOOK-053 窗口上方更优的买档计入最优档

- 规则:[实现说明](README.md#实现说明)
- 级别:unit
- 里程碑:M1
- 前置:`P`;已应用 `S1`(窗口价格 9360–10630)
- 步骤:
  1. `apply_batch(B(100, 101; (10000, 0), (10700, 1); (10010, 0), (10710, 1)))`
- 期望:
  - `best_bid() == Some((10700, 1))`、`best_ask() == Some((10710, 1))`;返回包含 `RECENTERED`;`meta().base == 1006`(中价下标 1070 − 64)
- 实现:`crates/hft-book/tests/overflow.rs::book_053_out_of_window_better_level`

### BOOK-054 重新居中的边界

- 规则:[实现说明](README.md#实现说明)
- 级别:table
- 里程碑:M1
- 前置:`P`;每行在新应用 `S1`(base 936)的簿上投递 `B(100, 101; (10000, 0), (买, 1); (10010, 0), (卖, 1))`
- 实现:`crates/hft-book/tests/recenter.rs::book_054_recenter_bounds`

| 行 | 买 | 卖 | 中价下标 m(m − base) | 期望 |
|---|---|---|---|---|
| BOOK-054.1 | 10300 | 10310 | 1030(94) | 无 `RECENTERED`;base 936 |
| BOOK-054.2 | 10310 | 10320 | 1031(95) | 无 `RECENTERED`;base 936 |
| BOOK-054.3 | 10320 | 10330 | 1032(96) | `RECENTERED`;base 968 |
| BOOK-054.4 | 9680 | 9690 | 968(32) | 无 `RECENTERED`;base 936 |
| BOOK-054.5 | 9670 | 9680 | 967(31) | `RECENTERED`;base 903 |

### BOOK-055 跳幅 ≥ L 走全量重建,结果与参考相同

- 规则:[实现说明](README.md#实现说明)
- 级别:unit
- 里程碑:M1
- 前置:`P`(L = 128);已应用 `S0`;另一侧参考簿 `BTreeMap`
- 步骤:
  1. `apply_batch(B(100, 101; (10000, 0), (9990, 0), (11280, 1), (11270, 2); (10010, 0), (10020, 0), (11290, 3)))`(中价下标跳 128 档)
- 期望:
  - 返回包含 `RECENTERED`;`meta().base == 1064`;`iter(Buy)` 产出 `(11280, 1)`、`(11270, 2)`,`iter(Sell)` 产出 `(11290, 3)`,与参考簿逐档相同
- 实现:`crates/hft-book/tests/recenter.rs::book_055_large_jump_rebuild`

### BOOK-056 单调上涨与下跌 10 × W 档的序列与参考一致

- 规则:[实现说明](README.md#实现说明)
- 级别:prop
- 里程碑:M1
- 性质:每条消息后最优档、前 50 档、`last_id` 与参考簿相同;发生的 `RECENTERED` 次数 ≥ 9;未发生截断时 `TRUNCATED` 从不出现
- 生成:`window ∈ {64, 2048}`、`ovf_cap = 64`;中价从起点单调移动 10 × W 档(上涨与下跌的用例各一半),每步 1–5 档,每条消息改 1–10 档并保持两侧各 ≥ 3 档(另删除越过中价的档与每侧第 40 档之外的档)
- 参考:`tests/model.rs` 的 `BTreeMap` 参考簿
- 例数:PR 200
- 实现:`crates/hft-book/tests/prop_recenter.rs::book_056_monotonic_drift_matches_model`

### BOOK-060 随机快照与增量序列与 BTreeMap 参考簿每步一致

- 规则:[实现说明](README.md#实现说明)、[API](README.md#api)
- 级别:prop
- 里程碑:M1
- 性质:每一步之后最优档、两侧前 50 档、`last_id`、`state()` 与参考相同;参考按同一规则模拟溢出区容量(离中价最远者并入远端)时,本簿在参考发生淘汰的那一步返回 `TRUNCATED`,`meta().truncated` 等于参考的淘汰次数
- 生成:`window ∈ {64, 128, 2048}`、`ovf_cap ∈ {1, 4, 64}`、`crossed_tolerance ∈ {0, 3, 65535}`;一条 Exchange 快照后 1–2 000 条消息;档价在中价 ±4W 内(含窗口外、跨窗口大跳、删除不存在的档、同价多次更新);消息另附删除与新中价交叉的档(每 16 条有 1 条不删,覆盖交叉与 Stale);1% 的消息为新快照
- 参考:`tests/model.rs` 的 `BTreeMap` 参考簿(含溢出容量模拟)
- 例数:PR 500
- 实现:`crates/hft-book/tests/prop_model.rs::book_060_matches_btreemap_model`

### BOOK-061 双路合流的消息去重后与单路相同

- 规则:[决定:簿只接受已归一化的增量](README.md#决定簿只接受已归一化的增量但自己做通用校验)
- 级别:prop
- 里程碑:M1
- 性质:同一消息流复制两份,以整条消息为单位随机交错(每路内部保序)喂入,最终 `state_hash()` 与单路喂入相同;返回 `DUPLICATE` 的次数等于重复消息的档数之和
- 生成:1–500 条消息,每条 1–10 档(逐档 `L2Delta`);两路交错的随机置换
- 参考:单路喂入的同一簿
- 例数:PR 1 000
- 实现:`crates/hft-book/tests/prop_dedup.rs::book_061_dual_path_dedup`

### BOOK-062 随机分档快照整替换后簿等于该快照

- 规则:[API](README.md#api)
- 级别:prop
- 里程碑:M1
- 性质:每应用一条 `TopN` 快照(非重复)后,`iter` 两侧逐档等于该快照去掉 `qty = 0` 档后的内容;`best_*` 等于快照的最优档;重复 `snap_id` 的快照不改变簿
- 生成:1–300 条快照,每条每侧 0–20 档,`snap_id` 单调递增并以 10% 概率重复或回退,5% 概率为 0
- 参考:快照本身
- 例数:PR 1 000
- 实现:`crates/hft-book/tests/prop_topn.rs::book_062_topn_equals_last_snapshot`

### BOOK-065 on_conn 在各状态下的迁移

- 规则:[API](README.md#api)、[状态机](README.md#状态机)
- 级别:table
- 里程碑:M1
- 前置:`P`;按"起始"列把簿带到该状态(Live = 已应用 S0;Stale(Gap) = S0 后投递 `Δ(99, 101, Buy, 10000, 1, 0, LAST)`;Resyncing = S0 后 `on_conn(Resync)`;Uninit = 新簿)
- 实现:`crates/hft-book/tests/state.rs::book_065_on_conn_transitions`

| 行 | 起始 | kind | 期望返回 | 期望状态 |
|---|---|---|---|---|
| BOOK-065.1 | Live | Down | `WENT_STALE` | `Stale(ConnDown)` |
| BOOK-065.2 | Live | Resync | `WENT_STALE` | `Resyncing` |
| BOOK-065.3 | Live | Up | 0 | `Live` |
| BOOK-065.4 | Live | Rotated | 0 | `Live` |
| BOOK-065.5 | Live | Degraded | 0 | `Live` |
| BOOK-065.6 | Live | Alive | 0 | `Live` |
| BOOK-065.7 | Stale(Gap) | Down | 0 | `Stale(Gap)`(原因不变) |
| BOOK-065.8 | Stale(Gap) | Resync | 0 | `Resyncing` |
| BOOK-065.9 | Resyncing | Down | 0 | `Stale(ConnDown)` |
| BOOK-065.10 | Resyncing | Resync | 0 | `Resyncing` |
| BOOK-065.11 | Uninit | Down | 0 | `Uninit` |
| BOOK-065.12 | Uninit | Resync | 0 | `Uninit` |

### BOOK-066 mark_stale 覆盖原因,Uninit 簿不变

- 规则:[API](README.md#api)、[状态机](README.md#状态机)
- 级别:table
- 里程碑:M1
- 前置:`P`;起始状态的得到方式同 BOOK-065;调用 `mark_stale(StaleReason::Refdata)`
- 实现:`crates/hft-book/tests/state.rs::book_066_mark_stale`

| 行 | 起始 | 期望 |
|---|---|---|
| BOOK-066.1 | Live | `Stale(Refdata)`;`meta().stale_reason == 6` |
| BOOK-066.2 | Stale(Gap) | `Stale(Refdata)` |
| BOOK-066.3 | Resyncing | `Stale(Refdata)` |
| BOOK-066.4 | Uninit | `Uninit` |

### BOOK-067 缺口到重同步再回到 Live 的完整循环

- 规则:[状态机](README.md#状态机)
- 级别:unit
- 里程碑:M1
- 前置:`P`;已应用 `S0`
- 步骤:
  1. `apply_delta(Δ(101, 102, Buy, 10000, 1, 0, LAST))`
  2. 投递 3 条 `Δ(102, 103, …)`、`Δ(103, 104, …)`、`Δ(104, 105, …)`(各单档)
  3. `on_conn(Resync)`;再投递 1 条增量
  4. `apply_snapshot`(origin Exchange,`snap_id = 300`,档同 S0)
  5. `apply_delta(Δ(300, 301, Sell, 10010, 1, 0, LAST))`
- 期望:
  - 步骤 1 `GAP | WENT_STALE`;步骤 2 三次 `NOT_LIVE`;步骤 3 `on_conn` 返回 0、状态 `Resyncing`,增量 `NOT_LIVE`
  - 步骤 4 `APPLIED | WENT_LIVE`;步骤 5 `APPLIED | ASK_TOP`,`last_id() == 301`
- 实现:`crates/hft-book/tests/state.rs::book_067_gap_resync_cycle`

### BOOK-068 状态、原因与结果位的码值

- 规则:[API](README.md#api)
- 级别:table
- 里程碑:M1
- 前置:无参数
- 实现:`crates/hft-book/tests/codes.rs::book_068_codes`

| 行 | 名 | 期望 |
|---|---|---|
| BOOK-068.1 | `BookState` Uninit/Live/Stale/Resyncing | 0/1/2/3 |
| BOOK-068.2 | `StaleReason` Gap/Crossed/ConnDown/Split/BadLevel/Refdata | 1/2/3/4/5/6 |
| BOOK-068.3 | `Outcome` APPLIED/BID_TOP/ASK_TOP/DUPLICATE/NOT_LIVE/GAP | 1/2/4/8/16/32 |
| BOOK-068.4 | `Outcome` CROSSED/TRUNCATED/RECENTERED/INVALID/WENT_STALE/WENT_LIVE | 64/128/256/512/1024/2048 |
| BOOK-068.5 | `TopOutcome` BID_PX/ASK_PX/BID_QTY/ASK_QTY/DUPLICATE/OLDER/INVALID | 1/2/4/8/16/32/64 |
| BOOK-068.6 | `BookErr` MemTooSmall/MemAlign/BadParams/UnknownInst/DupInst | 1/2/3/4/5 |

### BOOK-070 Top::apply 的去重、过期、交叉判定与变化位

- 规则:[API](README.md#api)、[实现说明](README.md#实现说明)
- 级别:table
- 里程碑:M1
- 前置:`Top` 初值全 0;按行顺序调用 `apply(&b, Seq(行号))`,b 写作 `{update_id; bid_px/bid_qty; ask_px/ask_qty}`,`rx_tsc = 1000 + 行号`
- 实现:`crates/hft-book/tests/top.rs::book_070_top_apply`

| 行 | b | 期望返回 | 期望 Top |
|---|---|---|---|
| BOOK-070.1 | {5; 100/1; 110/2} | `BID_PX \| ASK_PX \| BID_QTY \| ASK_QTY` | 报价为 b;`update_id = 5`;`seq = 1`;`rx_tsc = 1001` |
| BOOK-070.2 | {5; 101/1; 110/2} | `DUPLICATE` | 不变(`rx_tsc = 1001`) |
| BOOK-070.3 | {4; 101/1; 110/2} | `OLDER` | 不变 |
| BOOK-070.4 | {6; 100/3; 110/2} | `BID_QTY` | `bid_qty = 3`;`seq = 4`;`rx_tsc = 1004` |
| BOOK-070.5 | {7; 110/1; 110/2}(买 = 卖) | `INVALID` | 不变 |
| BOOK-070.6 | {8; 120/1; 110/2}(交叉) | `INVALID` | 不变 |
| BOOK-070.7 | {9; 0/0; 110/2}(买侧缺) | `BID_PX \| BID_QTY` | `bid_px = 0`;`update_id = 9` |
| BOOK-070.8 | {10; 0/0; 110/2}(值不变) | 0 | `update_id = 10`;`seq = 8`;`rx_tsc = 1008` |
| BOOK-070.9 | {0; 100/1; 110/2}(无序号) | `BID_PX \| BID_QTY` | `update_id = 0` |
| BOOK-070.10 | {0; 100/1; 111/2}(无序号,不去重) | `ASK_PX` | `ask_px = 111` |

### BOOK-071 Top 是一个 64 字节缓存行

- 规则:[API](README.md#api)
- 级别:unit
- 里程碑:M1
- 前置:无参数
- 步骤:
  1. 读取 `size_of::<Top>()`、`align_of::<Top>()` 与 8 个字段的 `offset_of!`
- 期望:
  - 64、64;`bid_px` 0、`bid_qty` 8、`ask_px` 16、`ask_qty` 24、`update_id` 32、`ex_ts_ns` 40、`rx_tsc` 48、`seq` 56
- 实现:`crates/hft-book/tests/layout.rs::book_071_top_layout`

### BOOK-072 Top.rx_tsc 只随被应用的 Bbo 更新,Top 没有报价年龄

- 规则:[API](README.md#api)
- 级别:unit
- 里程碑:M1
- 前置:`Top` 已应用 `{5; 100/1; 110/2}`,`rx_tsc = 1000`
- 步骤:
  1. 依次应用 `rx_tsc` 为 2000、3000、4000 的 `{5; …}`(DUPLICATE)、`{4; …}`(OLDER)、`{6; 120/1; 110/2}`(INVALID)
  2. 应用 `{7; 100/1; 110/2}`(值不变),`rx_tsc = 5000`
- 期望:
  - 步骤 1 后 `rx_tsc == 1000`
  - 步骤 2 后 `rx_tsc == 5000`,报价字段不变;`Top` 不含任何年龄或过期字段(报价可用性不由本 crate 判定)
- 实现:`crates/hft-book/tests/top.rs::book_072_rx_tsc_only_on_apply`

### BOOK-073 Top 与 L2 簿相互独立,不做一致性检查

- 规则:[API](README.md#api)
- 级别:unit
- 里程碑:M1
- 前置:BookSet 含 inst 1 的 L2 簿(`P`)
- 步骤:
  1. `top_mut(1).apply({5; 200/1; 210/1}, Seq(1))`;`l2_mut(1).apply_snapshot(S0)`
- 期望:
  - 两次调用都成功;`top(1).bid_px == Px(200)`,`l2(1).best_bid() == Some((10000, 5))`;两者都不返回错误位
- 实现:`crates/hft-book/tests/set.rs::book_073_top_and_l2_independent`

### BOOK-075 快照行 BookMetaRow 与 TopRow 的布局

- 规则:[API](README.md#api)
- 级别:table
- 里程碑:M1
- 前置:无参数
- 实现:`crates/hft-book/tests/layout.rs::book_075_snapshot_row_layout`

| 行 | 项 | 期望 |
|---|---|---|
| BOOK-075.1 | `size_of::<BookMetaRow>()` | 32 |
| BOOK-075.2 | `BookMetaRow` 偏移 inst/state/stale_reason/origin/_pad0/last_id/base/truncated/crossed_run/_pad1 | 0/4/5/6/7/8/16/24/28/30 |
| BOOK-075.3 | `size_of::<TopRow>()` | 64 |
| BOOK-075.4 | `TopRow` 偏移 inst/_pad/bid_px/bid_qty/ask_px/ask_qty/update_id/ex_ts_ns/seq | 0/4/8/16/24/32/40/48/56 |
| BOOK-075.5 | `TopRow` 字段集合 | 不含 `Tsc` 类型字段 |

### BOOK-076 BookSet::snapshot 一次写出节 6 与节 7,按 inst 升序

- 规则:[API](README.md#api)、[实现说明](README.md#实现说明)
- 级别:unit
- 里程碑:M1
- 前置:refdata inst 1–5;BookSet 的 L2 簿为 inst 4、2;inst 3 与 inst 1 各应用过一条 Bbo(`seq` 非 0),其余 Top 未收到过 Bbo
- 步骤:
  1. `snapshot(&mut w)`,用 `SnapshotReader` 读回节头与行
- 期望:
  - 恰好两节:节 6(版本 1)两行,inst 依次 2、4;节 7(版本 1)两行,inst 依次 1、3
  - 节 7 行内报价与两个 Top 相同
- 实现:`crates/hft-book/tests/snapshot_rows.rs::book_076_sections_written_once`

### BOOK-077 快照节往返恢复状态、last_id、base 与报价,rx_tsc 清零

- 规则:[API](README.md#api)、[实现说明](README.md#实现说明)
- 级别:unit
- 里程碑:M1
- 前置:BookSet A(inst 2 的 L2 簿已应用 S0 后 GAP 转 `Stale(Gap)`;inst 4 的 L2 簿 Live、`crossed_run = 1`、`truncated = 2`;inst 3 的 Top 已应用一条 Bbo,`rx_tsc = 777`);B 为同参数新建
- 步骤:
  1. `A.snapshot(w)`;`B.restore(r)`
- 期望:
  - 返回 `Ok`;B 中 inst 2 为 `Stale(Gap)`、`last_id == 100`、`base == 936`;inst 4 的 `meta()` 与 A 相同(`inst` 除外)
  - `B.top(3)` 的报价、`update_id`、`ex_ts_ns`、`seq` 与 A 相同,`rx_tsc == 0`
- 实现:`crates/hft-book/tests/snapshot_rows.rs::book_077_restore_round_trip`

### BOOK-078 restore 拒绝未知行版本与非法码值,跳过本 BookSet 没有的簿

- 规则:[API](README.md#api)、[实现说明](README.md#实现说明)
- 级别:table
- 里程碑:M1
- 前置:BookSet 的 L2 簿只有 inst 2;refdata inst 1–5;每行手工构造节内容后 `restore`
- 实现:`crates/hft-book/tests/snapshot_rows.rs::book_078_restore_rejects`

| 行 | 节内容 | 期望 |
|---|---|---|
| BOOK-078.1 | 节 6 版本 2 | `Err(RestoreError::VersionUnsupported)` |
| BOOK-078.2 | 节 7 版本 2 | `Err(RestoreError::VersionUnsupported)` |
| BOOK-078.3 | 节 6 一行 inst 2,`state = 7` | `Err` |
| BOOK-078.4 | 节 6 一行 inst 2,`stale_reason = 9` | `Err` |
| BOOK-078.5 | 节 6 一行 inst 2,`origin = 4` | `Err` |
| BOOK-078.6 | 节 6 两行 inst 2、inst 4(inst 4 未分配 L2) | `Ok`;inst 2 按行恢复,inst 4 的行被跳过 |
| BOOK-078.7 | 节 7 一行 inst 9(超出 refdata) | `Ok`;该行被跳过 |

### BOOK-079 恢复加关键帧后的簿与连续运行的簿此后逐条相同

- 规则:[API](README.md#api)、[实现说明](README.md#实现说明)
- 级别:prop
- 里程碑:M1
- 性质:簿 A 运行任意前缀后,`snapshot` + `export` 得到节 6 行与关键帧;新簿 B `restore` 后以 `origin = Keyframe` 应用关键帧;此后同一后缀分别喂给 A 与 B,每一步返回位、`state_hash()`、`meta()` 相同
- 生成:同 BOOK-060 的序列(`window ∈ {64, 128}`,档价在中价 ±W 内,消息删除与新中价交叉的档),前缀与后缀长度各 0–500;前缀不含跨窗口大跳;前缀结束时簿为 Live(关键帧只为 Live 簿导出)且未发生截断,否则该例跳过
- 参考:连续运行的簿 A
- 例数:PR 300
- 实现:`crates/hft-book/tests/prop_restore.rs::book_079_restore_plus_keyframe_is_seamless`

### BOOK-080 export 后再 apply_snapshot 得到同一状态哈希

- 规则:[API](README.md#api)
- 级别:unit
- 里程碑:M1
- 前置:`P`;已应用 S0 与 BOOK-050 的批(含溢出区档),`last_id = 101`;`out` 容量 300
- 步骤:
  1. `export(&mut out)`
  2. 新簿 `apply_snapshot(L2Snapshot{origin = Exchange, snap_id = 101}, out[..n])`
- 期望:
  - 步骤 1 返回 `(4, 3, false)`;`out[..4]` 买价依次 10000、9990、9350、9000,`out[4..7]` 卖价依次 10010、10020、10700
  - 步骤 2 后新簿 `state_hash()` 等于原簿
- 实现:`crates/hft-book/tests/export.rs::book_080_export_round_trip`

### BOOK-081 export 容量不足时两侧各保留最内的一半

- 规则:[API](README.md#api)
- 级别:table
- 里程碑:M1
- 前置:`P`;已应用 S0(每侧 2 档);`out` 容量见各行
- 实现:`crates/hft-book/tests/export.rs::book_081_export_truncation`

| 行 | out 容量 | 期望返回 | 期望内容 |
|---|---|---|---|
| BOOK-081.1 | 4 | `(2, 2, false)` | 买 10000、9990;卖 10010、10020 |
| BOOK-081.2 | 3 | `(1, 1, true)` | 买 10000;卖 10010 |
| BOOK-081.3 | 2 | `(1, 1, true)` | 买 10000;卖 10010 |
| BOOK-081.4 | 0 | `(0, 0, true)` | 无 |
| BOOK-081.5 | 0,新簿(无档) | `(0, 0, false)` | 无 |

### BOOK-082 state_hash 覆盖状态、last_id 与档,不含窗口布局

- 规则:[API](README.md#api)
- 级别:table
- 里程碑:M1
- 前置:`P`;基准簿 X = 新簿应用 `T(100; S0 的档)`(base 936);每行构造簿 Y,比较 `X.state_hash()` 与 `Y.state_hash()`
- 实现:`crates/hft-book/tests/export.rs::book_082_state_hash_scope`

| 行 | Y | 期望 |
|---|---|---|
| BOOK-082.1 | 新簿应用 `T(50; (10300, 1); (10310, 1))`(base 966)后再应用 `T(100; S0 的档)`(中价下标 1000,m − base = 34,base 保持 966) | 相等(base 不同、档与 last_id 相同) |
| BOOK-082.2 | 与 X 相同构造后 `mark_stale(Refdata)` | 不等 |
| BOOK-082.3 | 新簿应用 `T(101; S0 的档)` | 不等(last_id 不同) |
| BOOK-082.4 | 新簿应用 `T(100; S0 的档,但 9990 的量为 4)` | 不等 |

### BOOK-083 meta 反映当前状态,inst 由 BookSet 填写

- 规则:[API](README.md#api)
- 级别:unit
- 里程碑:M1
- 前置:`P`;独立 `L2Book` 已应用 S0 后投递 `Δ(101, 102, Buy, 10000, 1, 0, LAST)`;BookSet 中 inst 7 的簿执行同样操作
- 步骤:
  1. 读取独立簿 `meta()` 与 BookSet 写出的 inst 7 的节 6 行
- 期望:
  - 独立簿:`inst = 0`、`state = 2`、`stale_reason = 1`、`origin = 1`、`last_id = 100`、`base = 936`、`truncated = 0`、`crossed_run = 0`
  - 节 6 行:同上但 `inst = 7`
- 实现:`crates/hft-book/tests/snapshot_rows.rs::book_083_meta_fields`

### BOOK-090 窗口内、最优不变的增量耗时

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:bench
- 里程碑:M1
- 基准名:`book_delta_inwin`
- 实现:`crates/hft-book/benches/book.rs::book_090_delta_inwin`

### BOOK-091 20 档 L2Batch 的耗时

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:bench
- 里程碑:M1
- 基准名:`book_batch_20`
- 实现:`crates/hft-book/benches/book.rs::book_091_batch_20`

### BOOK-092 删除最优档需位图重找的增量耗时

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:bench
- 里程碑:M1
- 基准名:`book_delta_best_remove`
- 实现:`crates/hft-book/benches/book.rs::book_092_delta_best_remove`

### BOOK-093 窗口外增量(溢出区 cap 64)的耗时

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:bench
- 里程碑:M1
- 基准名:`book_delta_ovf`
- 实现:`crates/hft-book/benches/book.rs::book_093_delta_ovf`

### BOOK-094 Top::apply 的耗时

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:bench
- 里程碑:M1
- 基准名:`book_top_apply`
- 实现:`crates/hft-book/benches/book.rs::book_094_top_apply`

### BOOK-095 best_bid 与 best_ask 的耗时

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:bench
- 里程碑:M1
- 基准名:`book_best`
- 实现:`crates/hft-book/benches/book.rs::book_095_best`

### BOOK-096 iter 前 20 档的耗时

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:bench
- 里程碑:M1
- 基准名:`book_iter_20`
- 实现:`crates/hft-book/benches/book.rs::book_096_iter_20`

### BOOK-097 每侧 1 000 档快照的耗时

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:bench
- 里程碑:M1
- 基准名:`book_snapshot_1000`
- 实现:`crates/hft-book/benches/book.rs::book_097_snapshot_1000`

### BOOK-098 每侧 20 档分档快照整替换的耗时

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:bench
- 里程碑:M1
- 基准名:`book_snapshot_topn20`
- 实现:`crates/hft-book/benches/book.rs::book_098_snapshot_topn20`

### BOOK-099 BookSet::snapshot 写节 6、7 的耗时

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:bench
- 里程碑:M1
- 基准名:`book_snapshot_sections`
- 实现:`crates/hft-book/benches/book.rs::book_099_snapshot_sections`

### BOOK-100 W = 2048 重新居中的耗时

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:bench
- 里程碑:M1
- 基准名:`book_recenter_2048`
- 实现:`crates/hft-book/benches/book.rs::book_100_recenter_2048`

### BOOK-101 每侧 200 档 export 的耗时

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:bench
- 里程碑:M1
- 基准名:`book_export_200`
- 实现:`crates/hft-book/benches/book.rs::book_101_export_200`

### BOOK-102 真实合约深度 1 h 回放吞吐

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:bench
- 里程碑:M3
- 基准名:`book_replay_binance_1h`
- 状态:可选扩展:全量增量深度
- 实现:`crates/hft-book/benches/book.rs::book_102_replay_binance_1h`

### BOOK-103 new_in 之后的全部入口零分配

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:unit
- 里程碑:M1
- 前置:测试二进制安装计数分配器;BookSet 已 `new_in`;`SnapshotWriter` 与 `export` 缓冲已预分配
- 步骤:
  1. 记录分配计数 c0
  2. 运行 100 000 次混合调用:`apply_delta`、`apply_batch`、`apply_snapshot`(Exchange 与 TopN)、`Top::apply`、`best_*`、`iter`、`export`、`state_hash`、`meta`、触发重新居中与溢出截断的消息、`BookSet::snapshot`
  3. 记录分配计数 c1
- 期望:
  - `c1 − c0 == 0`
- 实现:`crates/hft-book/tests/alloc.rs::book_103_zero_alloc_after_new_in`

### BOOK-105 交易所分档快照金样整替换后与最后一条快照逐档一致

- 规则:[API](README.md#api)
- 级别:golden
- 里程碑:M3
- 输入:`fixtures/binance-usdm/depth20/` 下的全部 `<case>.in`,经 venue-binance 解码为 `L2Snapshot{origin = TopN}` 后按序应用到一个簿
- 期望文件:同目录各 `<case>.expected.jsonl`;每条之后簿两侧逐档等于该条期望快照,重复 `snap_id` 的条目返回 `DUPLICATE`
- 实现:`crates/hft-book/tests/golden.rs::book_105_topn_golden`

### BOOK-106 采集的合约增量深度回放在各 REST 快照时刻与快照逐档一致

- 规则:[决定:簿只接受已归一化的增量](README.md#决定簿只接受已归一化的增量但自己做通用校验)
- 级别:golden
- 里程碑:M3
- 输入:`fixtures/replay/binance-usdm-depth-24h.pointer.toml`(现有采集的 24 h Binance 合约 depth 增量与定时 REST 快照,按 venue 归一化为 `L2Batch` 与 `L2Snapshot{Exchange}`)
- 期望文件:同一指针中的 REST 快照;回放过程中每到一个快照时刻,簿两侧前 1 000 档与该快照逐档相同,全程无 `GAP`、`Split`、`BadLevel`
- 状态:可选扩展:全量增量深度
- 实现:`crates/hft-book/tests/golden.rs::book_106_incremental_capture_golden`

## 验收

- M1 完成:里程碑为 M1 的用例全部有同编号实现且通过;BOOK-105(M3)在对应金样到位后通过;BOOK-102、BOOK-106 属可选扩展"全量增量深度",只在该扩展列入 `tools/xtask/test-trace.toml` 的 `enabled_extensions` 后要求实现。
- 参考模型(BOOK-060)在服务器验证包中累计 1 000 000 步无分歧;其余属性测试 CI 每夜各 10 倍于 PR 例数。
- 基准:BOOK-090–BOOK-101(及启用扩展后的 BOOK-102)在参考机 `bench-gate --reference` 下不超过 README [延迟与资源契约](README.md#延迟与资源契约) 的预算,且不宽于 [benchmarks 单 crate 上限表](../../benchmarks/README.md#单-crate-基准上限表) 的"簿更新"行;从 M1.5 第一次参考机运行起判定,CI 只报告。
