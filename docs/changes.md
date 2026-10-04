# 设计变更记录

设计基线 v1 冻结之后,规格的每一次行为变更在这里追加一行。流程见 [docs/tests §7](tests/README.md#7-修改流程设计基线-v1-之后):同一提交里改 README(规则)、`TESTS.md`(用例)与代码,再在本表记一行。只改措辞、链接或格式的提交不记。

| 日期 | 变更 | 起因(用例编号或问题) | 涉及文件 | 负责人确认 |
|---|---|---|---|---|
| 2026-10-04 | 设计基线 v1:进入实现前的最后一次统一(M0-f)与测试规格(M0-g)合并后的状态;此后只走本表记录的变更 | [负责人决定](../design/_work/round8-decisions.md)(进入实现、测试规格先行) | 全仓 | 是 |
| 2026-10-04 | `test-trace` 追溯规则细化:扫描范围排除 `docs/tests/README.md` 与 xtask 夹具目录;DST 场景、基准、fuzz 用例的"位置"定义;`实现 = 待实现` 而找到测试时列为未实现;pytest skip 等同 `#[ignore]`;xtask 夹具可由测试代码在临时目录生成;xtask 依赖加 `regex` | M1 实现 `cargo xtask test-trace`(TOOL-086–TOOL-091、TOOL-180) | docs/tests/README.md、tools/README.md、tools/TESTS.md、tools/xtask | 否 |
| 2026-10-04 | hft-types M1 实现时的规格修正:TYP-150 的 repr(u8) 枚举集合与线格式枚举补上 `HostClass`;`ClientOrderId` 字段 `gen` 写作 `r#gen`(Rust 2024 保留字)并派生 `Debug`;`VenueId` 常量明确为关联常量;`Px`/`Qty` 的同类型 `Add`/`Sub`/`Neg` 写明在 i64 边界饱和;TYP-024 参考在倍数 > 10 000 时改为素因子最小性校验(暴力搜索 10^9 级倍数不可行) | TYP-150、TYP-024、TYP-135–TYP-142(实现发现) | crates/hft-types/README.md、crates/hft-types/TESTS.md、crates/hft-types/src | 否 |
| 2026-10-04 | hft-refdata M1 实现时的规格修正:新增 RDB1 段 19 `StrRefs`(`ignorable_user_events` 的 StrRef 数组;REF-008 的未知段改用 20);第 1 步拒绝 `version = 0`(REF-005.11,REF-080 判定依赖它);第 4 步要求账户 `venues` 位图中的 venue 存在(REF-017.24);字符串与切片字段为加载时复制的 `Box<str>`/`Box<[T]>`(原 `&'static`),`VenueCaps` 为 `#[repr(C)]` 带显式 `_pad`;公开 API 补装配/工具辅助查询、`finish_raw`/`RawBlob` 测试钩子(替代 `raw_directory`)、`EcfBuilder`、`dump`/`diff`;本 crate 的 `sha2` 不启用 `asm`(sha2-asm 需目标 C 工具链);REF-031.2 补 Fees `(2, 1)`、REF-062.8 掩码值更正为 6、REF-071/REF-073/REF-088 前置与口径澄清 | REF-005、REF-006、REF-008、REF-017、REF-031、REF-062、REF-071、REF-073、REF-080、REF-088(实现发现) | crates/hft-refdata/README.md、crates/hft-refdata/TESTS.md、crates/hft-refdata | 否 |
