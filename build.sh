#!/usr/bin/env bash
# 鸣潮 DVD-Audio 一键制作流水线(运行于 WSL 内部)
set -e
cd "$(dirname "$0")"

echo "===== 步骤1:FLAC→WAV + 归一化 + 分组 ====="
python3 01_prepare.py

echo "===== 步骤2:dvda-author + mkisofs + 复制到 D 盘 ====="
python3 02_build.py

echo "===== 完成 ====="
