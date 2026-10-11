# Agent Note: 预创建段完成后才发布正式文件名

Status: implemented

## Problem

JNL-072 在 SIGKILL 后恢复并再次准备 2 MiB 段时失败。预创建直接以正式 `.jrn` 名称创建文件,随后才扩容、预缺页、锁内存和写段头。这段期间被杀会留下空文件、全零文件或不完整的 magic。恢复无法把它识别为 `Preparing`,同号文件又阻止下一次 `O_EXCL` 创建。

## Decision

预创建者在同目录使用 `<10 位数字>.jrn.preparing`,完成内存准备和 [P] 段头后,以 `renameat2(RENAME_NOREPLACE)` 发布为正式名,迁移进程内映射登记,最后把句柄交给写者。恢复只按这个严格的私有名称清理遗留文件,不从正式段的坏 magic 猜测它是否可以删除。启动内存核算包含这些临时文件。

契约归 [hft-journal](../../../../crates/hft-journal/README.md#实现说明),格式与文件名归 [journal-format](../../../../docs/subsystems/journal-format.md#目录与命名)。journal 记录格式、段头布局和写者热路径不变。

## Alternatives considered

- 先检查 `state == Preparing`,再检查 magic。完整映射的全零段可以回收,但创建后尚未扩容的空文件仍无法映射,正式文件损坏时也容易误删。
- 创建后立刻写 magic。仍然保留创建与写 magic 之间的崩溃窗口。
- 普通 rename。可能覆盖同名正式段,违背已有拒绝覆盖契约。

## Consequences

每次预创建在 persist 线程多一次原子重命名。Linux 后端要求文件系统支持 `RENAME_NOREPLACE`,不支持时明确返回 `PrepState::Failed`;tmpfs 与 2 MiB hugetlbfs 在现有 CI 用例中验证,1 GiB 主机验收仍走 JNL-106。

[JNL-124](../../../../crates/hft-journal/TESTS.md#jnl-124-预创建各崩溃边界均可同号重开保留已提交前缀) 固定覆盖发布前后 5 个边界,并检查已提交前缀和下一段序号。JNL-042.4 保留拒绝覆盖反例,JNL-099 覆盖临时段的启动空间核算,JNL-072 保持 PR 50 种子与夜间 1,000 种子。旧版本已遗留的坏正式文件不自动推断为可删除文件,仍需排障确认。
