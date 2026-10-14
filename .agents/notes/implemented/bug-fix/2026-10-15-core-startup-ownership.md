# Agent Note: 启动所有权与定向停机信号

Status: implemented

## Problem

以 `core.lock` 文件存在性判断实例占用会把崩溃留下的文件当作活进程。启动中尚未安装 SIGTERM 处理时，默认信号动作会跳过 Drain；仅让 housekeeping 读 signalfd 无法消费 `tgkill` 定向投递给主线程的信号。

## Decision

冷盘实例目录保留锁文件，进程持续持有内核文件锁，死亡自动释放所有权。持锁期间预约并同步 epoch，之后才能建立账户连接；既有 seq journal 仍由 `RecoveryRequired` 拒绝，不将这一锁修复解释为恢复实现。

主线程在建线程前屏蔽服务信号。housekeeping 与主线程等待循环读取非阻塞 signalfd，仅设置同一个 Drain 请求，唯一 ctl-gateway 写者提交命令。主线程读取用于线程定向信号，不写业务状态或 hk 环。SIGHUP 被消费且不重载配置。

## Alternatives considered

- 删除陈旧锁文件依赖人工判定，且 unlink 已锁 inode 会容许两个不同 inode 的锁同时存在。
- 仅由 housekeeping 读取 signalfd 满足原文字面要求，却无法满足 CORE-381 的主线程 `tgkill` 行。
- 异步 signal handler 不满足全部线程屏蔽信号的要求，也扩大了异步可执行代码面。

## Consequences

CORE-381 的读取者条款修正需要负责人复核；冻结用例的停机结果不变。内核锁和 epoch 测试在本地运行，Linux 进程用例覆盖信号屏蔽、定向信号、SIGHUP、唯一 Drain 及 Stopped。完整恢复、启动步骤记录和真实交易所挂单矩阵仍按组件测试中的覆盖边界验收。
