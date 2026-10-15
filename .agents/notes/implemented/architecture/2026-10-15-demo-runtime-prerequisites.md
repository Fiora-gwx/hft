# Agent Note: demo 主机的 CPU 与秘密装配

Status: implemented

## Problem

获准测试的 c8g.large 使用 Neoverse V2，旧发布清单含 CI 原生 SM3/SM4/SVE-SM4，编译器 neoverse-v1 本身也启用 sm4。仅删 manifest 特性不能证明兼容。该专用 demo 主机没有 IAM instance profile；旧预检必须检查 Secrets Manager，因此不能按既有路径启动。

## Decision

负责人于 2026-10-15 明确授权公共 V1/V2 特性包络和仅 demo 的本地秘密模式。保持 V1 调优，显式禁用 sm4；同时禁用上一 ARM CI runner 未提供的 rand/dit，使制品可以在 CI 原机验证。包络从 Cargo 实际参数的 rustc cfg 生成，映射 Linux HWCAP，未知特性失败。每个主机 CPU 都必须满足包络。

local_demo 是显式配置值，要求 testnet 端点、非正式账户和 dev2 formal_live=false。源目录 root:0700，文件 root:0400/0600；准备器先调用 core --check，再复制到 hft 可读的 tmpfs 临时文件，由 core 读后 unlink。生产保留 IAM/Secrets Manager 检查，IMDS 隔离与资源检查不豁免。

## Alternatives considered

删除 CPU 失败检查仍可能执行非法指令；运行整个交易进程为 root 不符合服务身份；把 key 放进 hft 可读的持久目录扩大暴露面。采用发布内预检和启动准备器，运行时只保留已有短期副本路径。

## Validation

DEP-235 验证未知特性、异构 CPU 缺项和额外 runner 特性；DEP-168 验证环境、目录/文件权限、符号链接与配置引用。CORE-222 验证生产环境在读取文件前拒绝 local_demo，CORE-013 保持 --check 不读取秘密。真实主机预检和运行仍需新 main CI 制品。
