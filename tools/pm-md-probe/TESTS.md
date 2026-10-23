---
component: pm-md-probe
prefix: PMP
spec: README.md
---

# pm-md-probe 测试规格

格式与规则见 [docs/tests](../../docs/tests/README.md)。

## 覆盖

| README 小节 | 用例 |
|---|---|
| 概要 | PMP-001 |
| 目录 | 不适用:非规范 |
| 使用本组件 | PMP-001 |
| 命令行 | PMP-001 |
| 产出 | PMP-001、PMP-004 |
| 验收步骤 | PMP-001 |
| hft-core 验收 | PMP-002、PMP-004 |
| 存储 | PMP-003 |
| 实现说明 | PMP-001、PMP-002 |
| 延迟与资源契约 | 不适用:无门槛 |
| 测试与验收 | 不适用:非规范 |
| 依赖 | 不适用:由 `cargo xtask deps` 机检 |
| 延伸阅读 | 不适用:非规范 |
| 已知限制与后续工作 | 不适用:里程碑外 |

## 用例

### PMP-001 两个完整 5 分钟窗口与独立录制逐条一致

- 规则:[验收步骤](README.md#验收步骤)
- 级别:live
- 里程碑:M3
- 前置:eu-west-1 主机;同一时段 pmrec 在录制(公司录制机);`discover.py --windows 2` 取 BTC、ETH、SOL、XRP 下两个窗口的 16 个代币
- 步骤:第一个窗口开始前启动探针,运行至第二个窗口结束后 60 s;`compare.py` 比对
- 期望:16 个代币全部 `OK`(两边都有的每个交易所时间戳上最优价相等,值不一致为 0;只有一边有的时间戳 ≤ 1% 或 5 个,来自重连缺口),`RESULT PASS`;探针无畸形 bulk 记录
- 实现:手动运行,结果写回本文件"验收"一节

### PMP-002 hft-core 连续两个窗口(含改绑)与独立录制一致

- 规则:[hft-core 验收](README.md#hft-core-验收)
- 级别:live
- 里程碑:M3
- 前置:eu-west-1 主机;同一时段 pmrec 在录制;`pm_sim_config` 生成的配置,`PIPELINE_USER` 为运行 `bind_slots.py` 的系统用户
- 步骤:启动 `hft-core --dry-run`,随后启动 `bind_slots.py`;运行跨越至少两个连续窗口加 60 s 后停止;`pm-md-probe journal` 读 seq 段,`compare.py` 比对
- 期望:每个窗口的 `BindSlot` 结果为 `Applied`(重发为 `NoOp`),同一槽位相隔一个窗口改绑一次;全部绑定 `OK`,`RESULT PASS`
- 实现:手动运行,结果写回本文件"验收"一节

### PMP-003 journal 清理不碰运行中的段

- 规则:[存储](README.md#存储)
- 级别:host
- 里程碑:M3
- 前置:pm-strategy-euw1;两个已停止的运行与一个正在运行的 `pmcore_run.sh`
- 步骤:运行中以 `PMCORE_MIN_AGE_MIN=0 PMCORE_KEEP_DAYS=0` 执行 `journal_housekeep.sh`,运行结束后再执行一次
- 期望:运行中最新段保持 `.jrn`、运行目录不删除、hft-core 正常 Drain 退出 0;结束后全部冷段为 `.jrn.zst`;已停止运行的冷段全部压缩
- 证据:2026-10-20 06:03 UTC 手动执行,两个已停止运行 1.1 GB/1.3 GB → 548 MB/517 MB(其余为验收产物),运行中的段未动,hft-core `STOPPED exit=0`
- 实现:手动运行

### PMP-004 `--orders-only` 与完整解码的订单行一致

- 规则:[产出](README.md#产出)、[hft-core 验收](README.md#hft-core-验收)
- 级别:host
- 里程碑:M3
- 前置:pm-strategy-euw1;运行中的挂单模拟的已封口 seq 段
- 步骤:同一批段各解码一次:完整解码、`--orders-only`;另取后一半的段单独以 `--orders-only` 解码
- 期望:`--orders-only` 的输出没有 `snap`/`batch`/`trade` 行,其 `dec`/`oev`/`ext`/`settle`/`cmdres`/`conn` 行与完整解码逐行相同;`bind` 行数等于完整解码打印的 binds 数;只解码后一半段时,用前一半的 `bind` 行把 `inst` 映射回窗口,标签与完整解码一致
- 实现:手动运行

## 验收

PMP-001 通过即完成 P1 行情验收;PMP-002 通过即完成 P1b(正式进程)验收。

| 运行(2026-10-19 UTC) | 构建 | 窗口 | 结果 |
|---|---|---|---|
| 1 | b9281441 | 16:55、17:00 | 1023378/1023436 时间戳相等;不等的全部在探针重连之后(重订阅后先到整簿、再到更早的增量)→ 修复为丢弃早于最近整簿的增量(PMK-012) |
| 2 | 52959f0a | 17:20、17:25 | 922906/922906 相等;30 s 空闲超时把未开盘的安静市场误断 35 次 → 取消空闲超时 |
| 3 | 1907e1ee | 17:45、17:50 | 842933/842933 相等;4 个"解码错误"是服务端的空数组 `[]` → 按无数据处理(PMK-002.6) |
| 4 | 9a77905b | 18:05、18:10 | **PASS**:16 个代币全部 OK,713260/713260 相等,解码错误 0,畸形记录 0 |

| PMP-002 运行(2026-10-20 UTC) | 构建 | 窗口 | 结果 |
|---|---|---|---|
| 冒烟 1–3 | 0310d88e | 02:50–03:00 | 启动失败依次为 `BboSlots`(装配用了币安默认的 BBO 配置 → 按协议路由配置 Md)、`JournalReader Io`/`JournalPrepare`(memlock 上限 475 MiB,空载已锁 445 MiB → 运行脚本按生产 unit 放开 memlock,段改 8 MiB)、`RoleInsufficient`(sequencer 未放行 pipeline 的 `BindSlot`,SEQ-432.6) |
| 冒烟 4 | 0310d88e / 5d8eaedc | 02:50、02:55、03:00 | 870877/871689;关键帧簿副本被当成行情(已排除);剩余不等在启动后 5 s 内:首个代币未绑定的 `price_change` 被整帧丢弃 → `peek` 按首个已绑定代币路由(PMK-015) |
| 1 | 275d1cfc | 03:20、03:25、03:30、03:35(每个窗口提前 120 s 由 pipeline 主体改绑,32 次 `Applied`) | **PASS**:32 个绑定全部 OK,1378248/1378248 时间戳相等;hft-core 正常 Drain 退出 0 |

上表运行中探针的"断开"(`Down` 原因 64)多数是市场结算后约 147 s 交易所的正常关闭(运行 3、4 各一次、四条连接同一秒),与 1013 记为同一个 `WS_CLOSE` 位;早先据此得出的"探针比独立录制器断得多"不成立。2026-10-20 在同机同期对同 6 个 BTC 市场重新比对:探针异常断开 1 次,Python 双连接录制器 3 次(含同一市场两条连接同一秒被断开一次)。断开原因与对策见 [venue-polymarket 多连接去重](../../crates/venue-polymarket/README.md#多连接去重)。
