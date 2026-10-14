# Agent Note: 恢复缩容的检查顺序与定时器配额

Status: implemented

## Problem

SEQ-006 的旧表要求 B0 把定时器容量缩到 40 后成功,但 B0 的引擎预留量本身超过 40,与 SEQ-349 和 README 的启动约束冲突。只检查当前堆占用还会允许策略定时器挤占引擎未来需要的空间。按新 cell 容量直接恢复组件,则可能先报快照无法解码,掩盖规定顺序中的 CapacityShrink。

## Decision

保留真实 Drain 的 40 条堆占用,定时器边界用引擎预留量加已登记策略定时器数。分别检查少一条、恰好容纳和实际堆超限,不降低启动或运行期配额约束。恢复的临时组件只为读取已有占用扩容,完成按 CapKind 排序的检查后,成功路径必须使用请求容量。

容量改变时,Recovery 引导块使用版本加一的完整 Engine ConfigBlob。只替换段 3,保留其他节;容量不变保留原身份。恢复后的容量拒绝必须能仅靠 journal 独立重现。

Draining 是旧进程不可撤销的退出请求,带到新进程会继续拒绝 Resume 和 SetMode。冷恢复先为全部账户设置 Recovery 并复位闸门,再移除旧 Draining;重启没有自动放行交易,其余 cap 及启动核验仍有效。验证和镜像从各自快照原样恢复,不重做这个运行边界转换。

## Alternatives considered

跳过定时器预留检查会把未来引擎定时器溢出留到运行期。把 40 条改成伪造快照或手工堆条目无法证明真实 Ctx 和 Drain 路径。只在测试装配中传新容量会让离线读者继续使用旧配置。

## Consequences

[SEQ-006/007](../../../../crates/hft-sequencer/TESTS.md#seq-006-恢复后占用超过新容量时-boot-返回-capacityshrink) 覆盖 13 行拒绝/接受顺序与缩容后的参数拒绝,并验证原运行和恢复运行的输出。未改 journal schema 或快照行格式;M2 整体与参考机上线验收仍按原门槛执行,恢复相关改动交负责人评审。

## Verification

[CI 原始 journal 对照](https://github.com/Fiora-gwx/hft/actions/runs/38194159634) 在相同依赖下运行 `7a39cc3` 与 `e18d1bb`,core 种子 7/28/34 各 2000 步。首处分歧分别为 Recovery 快照 seq 2243/1983/1989,此前全部记录相同。三个快照只有节 4 删除 Draining、节 18 的 `ctl_state_version` 从 9 到 10、节 21 两个 cell 的 `wake_at` 归零及头部状态哈希变化。`CapCleared` 经现有执行层事件处理清除等待时间,没有改写其他快照状态。

种子 28 的 seq 2157/2159 只有拒绝位从 Draining 变为 CloseOnly,没有分配槽;其他 journal 记录全部相同。由此更新一致性集(`fixtures/dst/hashtrail.toml`,2026-10-14 已移除)的三个摘要,保留全部 50 个种子、步数与采样间隔。[TK-120](../../../../crates/hft-testkit/TESTS.md#tk-120-一致性集的-drain-重启保留恢复闸门) 固定这些边界和复跑一致性;实际人工 Kill 跨重启的保留由 SEQ-007 检查。
