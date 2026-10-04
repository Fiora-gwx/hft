# Agent Note: 规格冻结为设计基线 v1;此后只走实现驱动的变更;测试规格先行

Status: proposed

> **第八轮第七次修订(M0-g,2026-10-04)**:事实同步——测试规格已写定(38 个组件的 `TESTS.md` 与 `docs/tests/` 三页,格式归 [docs/tests](../../../../docs/tests/README.md),简报见 [round8g 简报](../../../../design/_work/round8g-tests-brief.md),裁决见 [round8g 裁决](../../../../design/_work/round8g-rulings.md));设计基线 v1 = M0-f 与 M0-g 合并后的状态,即 [docs/changes.md](../../../../docs/changes.md) 首行;代码侧的追溯检查是 `cargo xtask test-trace`。

## Problem

M0 到 M0-e 做了六轮全量文档评审,每轮修完又出现新的不一致:规格之间互相抄写的表格与数值会分叉,而没有代码与测试去证伪它们。继续评审文档的边际收益已经低于开始实现的收益;但直接开工又会让"哪份文档算数"、"改设计走什么流程"和"测试照什么写"失去约束([round8-decisions](../../../../design/_work/round8-decisions.md) R8-12 的用户原话;[第五轮评审汇总](../../../../design/_work/review-2026-10-03-M0e五路汇总.md))。

## Proposal

用户决定([round8-decisions](../../../../design/_work/round8-decisions.md) R8-12;规范见 [round8f 简报](../../../../design/_work/round8f-spec-brief.md))。

1. **M0-f 与测试规格(M0-g)合并后规格冻结为设计基线 v1**([docs/changes.md](../../../../docs/changes.md) 首行),不再做全量文档评审轮次;策略相关的暂缓项保留现文,标"策略研究后定(R8-12)"。
2. **实现驱动的变更**([docs/tests §7](../../../../docs/tests/README.md#7-修改流程设计基线-v1-之后)):实现或测试发现规格有误或缺口 → 同一提交里改 README、该组件的 `TESTS.md` 与代码,并在 [docs/changes.md](../../../../docs/changes.md) 记一行(只改措辞、链接或格式的不记;资金、风控、恢复与延迟门槛相关的由负责人确认);仍须满足"一件事实一个归属地"。推翻某个 README 的规格时照旧需要 Agent Note([README](../../README.md#何时写))。
3. **测试规格先行**:每个 core crate、app 与工程类策略增加 `TESTS.md`(稳定用例 id、确定的输入与期望输出、级别、里程碑、规则覆盖表、实现函数名);跨组件场景、不变量与演练放 `docs/tests/`;格式与规则归 [docs/tests/README.md](../../../../docs/tests/README.md)。`check_hft.py` 机检格式、id 唯一与覆盖表完整;代码仓库的 CI 以 `cargo xtask test-trace` 检查"里程碑已到的用例必须有同 id 的测试函数、测试函数的 id 必须在 TESTS.md 中"([docs/tests §6](../../../../docs/tests/README.md#6-与代码的对应))。
4. 进入 M1:代码仓库在 `E:\trading` 之外,组织与远端由负责人建好后再推送(`[需授权]`)。

## Alternatives considered

**再做一轮全量评审再开工。** 第五轮已没有设计类 P0,剩下的是一致性与可执行性问题,它们在实现与测试中暴露得更快、更确定。

**冻结后不再改规格,只改代码。** 规格与代码会迅速分叉;同一提交改规格、测试与代码,把文档变成被测试约束的契约。

**测试只写在代码里,不写 TESTS.md。** 评审无法从规格逐条核对覆盖,"每个规范性小节都有用例"无从检查。

## Acceptance criteria

- `docs/changes.md` 存在,冻结之后每一次行为变更都对应一行与一个同时改了 `TESTS.md` 与代码的提交(只改测试规格的提交写明起因)。
- 每个 core crate 与 app 在其第一个实现 PR 之前有 `TESTS.md`,覆盖表无空项;`cargo xtask test-trace` 在 CI 的 `docs` 作业里生效。

## Risks

- 冻结的规格里仍有未发现的矛盾;靠 `TESTS.md` 的覆盖表与实现时的检查暴露,按实现驱动的变更修。
- `TESTS.md` 与 README 之间同样可能分叉;覆盖表引用 README 的小节锚点,由机检核对。
