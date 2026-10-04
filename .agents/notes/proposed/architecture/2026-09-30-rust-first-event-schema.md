# Agent Note: 内部事件 schema 以 Rust 定义为唯一来源,生成其余一切

Status: proposed

## Problem

事件格式要同时被热进程(Rust)、冷进程(Rust)、Python 研究与运维(PyO3、Polars)、Parquet 归档与文档使用。早期调研推荐内部编码用 SBE(sbe-tool 生成 Rust)+ `zerocopy`([research/04 §8](../../../../design/research/04-oss-frameworks-and-patterns.md)、[research/06 §0](../../../../design/research/06-data-plane-replay-observability.md))。但 SBE 工具链是 Java,schema 在 XML 里,生成的 Rust 代码与我们的定点新类型、`#[repr(C)]` 布局要求并不天然契合;任何"schema 在 A、代码在 B、文档在 C"的安排迟早会漂移。

## Proposal

[13 O5](../../../../design/13-设计评审与优化.md)、[08 §2](../../../../design/08-数据面与回放.md):

1. 事件体在 [`hft-event`](../../../../crates/hft-event/README.md) 里以 `#[repr(C)]` + `zerocopy::{FromBytes, IntoBytes, KnownLayout, Immutable}` 定义,这是**唯一来源**。
2. `cargo xtask gen-schema` 从 Rust 定义生成:Arrow schema、Python dtype、`docs/generated/event-catalog.md`、金样用的规范 JSON 渲染。生成物入库,CI 以 `--check` 保证不漂移。
3. 演进规则(只追加、改语义用新 kind、`schema_version`、kind 编号永不复用)在 [event-model](../../../../docs/subsystems/event-model.md#演进规则),步骤在 [cookbook](../../../../docs/cookbook/adding-an-event-kind.md)。
4. SBE 只用于**解码交易所的 SBE 流**(Binance Spot、Gate spot、OKX `/public-sbe`),在对应 `venue-<x>` 内;不进入内部编码。

## Alternatives considered

**SBE 作为内部编码(sbe-tool 生成)。** 引入 Java 工具链;XML schema 与 Rust 类型两处维护;生成代码是访问器风格,不是可直接 `memcpy` 的 `#[repr(C)]` 结构;我们需要的 SBE 特性(定长、字对齐、只追加演进)用 `zerocopy` + 规则即可得到。

**Protobuf / FlatBuffers / Cap'n Proto。** Protobuf 需要解码与分配;FlatBuffers/Cap'n Proto 零拷贝但带偏移表与 vtable,定长事件上没有收益,且同样引入外部 schema 语言与编译器。

**serde + bincode。** 需要序列化步骤,布局不稳定、跨版本兼容性弱;不适合 mmap 直读。

**schema 写在 YAML/TOML,生成 Rust。** 与 SBE 同样的"两处维护",而 Rust 编译器对布局的检查(编译期断言、zerocopy derive)只在 Rust 源里生效。

## Acceptance criteria

- `cargo xtask gen-schema --check` 在 CI 中通过;手改生成物即失败。
- Python 用生成的 dtype 以零拷贝读取 journal 记录,字段与 Rust 定义逐一对应(`hft-py` 测试)。
- 仓库中不存在 SBE XML schema 或 sbe-tool 调用(交易所 SBE 解码用手写或 crate 内生成的解码器)。

## Risks

- 生成器本身是代码,可能有 bug:生成器有自己的金样测试(已知结构 → 期望的 Arrow schema 与 dtype)。
- Rust 布局改动(如字段重排)会悄悄改变 wire 格式:布局快照测试(`tests/layout.snap`)与编译期断言阻止。
