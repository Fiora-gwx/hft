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
| 产出 | PMP-001 |
| 验收步骤 | PMP-001 |
| 实现说明 | PMP-001 |
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
- 期望:16 个代币全部 `OK`(两边都覆盖的时间段内 `price_change` 最优价序列逐条相等),`RESULT PASS`;探针无畸形 bulk 记录
- 实现:手动运行,结果写回本文件"验收"一节

## 验收

PMP-001 通过即完成 P1 行情验收。结果记录(日期、窗口、每个代币比对条数)在运行后补在本节。
