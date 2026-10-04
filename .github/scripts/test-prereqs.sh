#!/usr/bin/env bash
# Prepare the Linux prerequisites of the `proc` test cases on a hosted x86_64 runner and export
# their locations (crates/hft-journal/TESTS.md: HFT_TEST_HUGE2M, HFT_TEST_COLD_SMALL,
# HFT_TEST_SUDO, HFT_TEST_SHM8; strace for the DurableMeta syscall case). Tests whose variable is
# unset print the missing prerequisite and return, so a runner change shows up in the test log.
set -euo pipefail

uid=$(id -u)
gid=$(id -g)
command -v strace >/dev/null || echo "::warning::strace is not installed (the job's apt step installs it)"

# JNL-043 / audit: 2 MiB hugetlbfs.
sudo sysctl -q vm.nr_hugepages=16
sudo mkdir -p /mnt/huge2m
sudo mount -t hugetlbfs -o "pagesize=2M,uid=$uid,gid=$gid" none /mnt/huge2m

# JNL-048: a 2 MiB tmpfs as cold root, remounted by the test through password-less sudo.
sudo mkdir -p /mnt/cold-small
sudo mount -t tmpfs -o "size=2M,uid=$uid,gid=$gid,mode=0755" none /mnt/cold-small

# JNL-100: an 8 MiB tmpfs.
sudo mkdir -p /mnt/shm8
sudo mount -t tmpfs -o "size=8M,uid=$uid,gid=$gid,mode=0755" none /mnt/shm8

{
  echo "HFT_TEST_HUGE2M=/mnt/huge2m"
  echo "HFT_TEST_COLD_SMALL=/mnt/cold-small"
  echo "HFT_TEST_SUDO=1"
  echo "HFT_TEST_SHM8=/mnt/shm8"
} >>"$GITHUB_ENV"
