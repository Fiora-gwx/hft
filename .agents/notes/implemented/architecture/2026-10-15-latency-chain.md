# Agent Note: 隔离逐事件测量上下文

Status: implemented

## Problem

现场联调需要 MD-only 与每次订单回报的完整阶段数据。现有 LatencySample 面向首个回报，无法恢复 TLS、队列、策略与风险细分；向业务事件逐层增加测量字段会扩大环元素并改变冻结的 journal 布局。

## Decision

按 owner 2026-10-15 的逐事件采集要求，采用 [latency-report](../../../../tools/latency-report/README.md) 的独立有界测量环。D3 只对 hft-rt 的测量上下文登记例外：每线程启动前安装单 producer，运行中只写诊断，不参与业务分支、状态哈希或恢复；冷消费者独立排空，满环只计丢失。

没有选择热路径写文件、动态分配记录或一个共享加锁日志器。没有把现有首回报 LatencySample 改成逐 tick 大对象；保留原统计契约，细分放在独立文件。业务事件字节指纹的歧义必须显式暴露，不能用最近时间猜配对。

## Validation

固定工具链的 Clippy 在 `thread_local!` 展开含属性时忽略局部 lint allowance（上游 rust-clippy #17862）。根级豁免必须由 hot-check 对 hft-rt 的模板禁用宏调用及导入补检，仅测量文件允许 TLS；TOOL-032 覆盖格式化、日志、重命名导入和错误文件范围。

沿用 RT-002/166 的有界性与零分配检查、NET-125 的同读多帧、SEQ-053/054 的状态/回放隔离和 RPL-203 的统计检查。原始戳、环丢失、干净关闭标记、Git/HostProfile 与校准证据共同保存。开启测量时的数字包含埋点开销；dev2 仅报告，不形成正式延迟门槛。
