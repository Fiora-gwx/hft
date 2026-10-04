# 子系统参考页

跨 crate 的类型与协议参考:每页是一类线格式或协议的**唯一定义**,crate README 链接这里而不复述。行为叙述在 [architecture.md](../architecture.md);规则见 [docs/AGENTS.md](../AGENTS.md#层级与归属)。

| 页 | scope | 定义什么 | Rust 实现 |
|---|---|---|---|
| [event-model.md](event-model.md) | core | 记录头(24 B)、`RecordFlags`、时间戳语义(含跨重启的引擎时间 `engine_ns` 与常态记录的 `sw_rx`)、行情连接的活性证据与 BBO 连接成员聚合、`ConnRole` 编码、kind 编号与分类(Input/Output/Meta)、关键事件体布局、schema 演进规则;模拟账户与实盘共用同一组事件 | [hft-event](../../crates/hft-event/README.md)(其余事件体、Note 码登记表、`Advice` 与 `StrategyParams` 的容器) |
| [journal-format.md](journal-format.md) | core(chain journal 部分 optional) | journal 目录与命名、段文件与 `SegmentHeader`、段大小公式(取值归 [apps/hft-core](../../apps/hft-core/README.md))、提交游标、CRC、`Var`/`Multi` 变长记录、段滚动与引导块、`StateSnapshot` 结构、冷副本与封存、校验与缺段检测 | [hft-journal](../../crates/hft-journal/README.md) |
| [control-plane.md](control-plane.md) | core | 控制命令集的语义、参数、类别(降级/升级/中性/只读)、角色(Admin/Trader/Viewer)、账户语义与账户绑定、版本与二次确认协议(含 Viewer 紧急停止的预览绑定确认)、幂等、过期、结果码与 detail 码表、模式与 cap 的优先级表 | [hft-ctl](../../crates/hft-ctl/README.md)、[hft-ctl-proto](../../crates/hft-ctl-proto/README.md)(权限矩阵)、[hft-event 控制命令编码](../../crates/hft-event/README.md#控制命令编码) |
| [external-venue-protocol.md](external-venue-protocol.md) | optional(冻结;链上策略接入时启用) | hft-core 与 chain-core 之间的消息、载体(共享内存环 / UDP)、会话与心跳、断连与恢复语义、版本 | [hft-ext-proto](../../crates/hft-ext-proto/README.md)、[hft-chain-evm](../../crates/hft-chain-evm/README.md) |

事件与命令的完整字段表由 `cargo xtask gen-schema` 生成到 `docs/generated/event-catalog.md`,以生成物为准;各策略的 schema JSON(参数字段、Note 码表 `notes`、报表字段 `report`)由 `hft-core schemas --out <dir>` 随发布制品导出为 `<kind_id>-<name>.schema.json` + `index.json`(数组,格式归 [strategies/registry](../../strategies/registry/README.md);[tools hft-cfg](../../tools/README.md#hft-cfg)、hft-py、pipeline、watchtower 与 journal-archiver 读取它)。
