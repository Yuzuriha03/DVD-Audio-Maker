#!/usr/bin/env bash
# 鸣潮 DVD-Audio 一键制作流水线(运行于 WSL 内部)
set -e
set -o pipefail
cd "$(dirname "$0")"

LOG=/root/dvda-build/build.log
mkdir -p "$(dirname "$LOG")"

echo "===== 步骤1:扫描音源 + 专辑归一化 + 解码完整性校验 ====="
python3 -u 01_prepare.py 2>&1 | tee "$LOG"

echo "===== 步骤2:MLP 编码 + dvda-author 出盘 + mkisofs 打包 + 复制到 D 盘 ====="
python3 -u 02_build.py 2>&1 | tee -a "$LOG"

echo "===== 完成 ====="
echo "日志: $LOG"
