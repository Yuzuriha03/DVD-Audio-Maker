#!/usr/bin/env bash
# ============================================================================
# DVD-Audio Maker —— 一键流水线（运行于 WSL 内部）
#
#   bash build.sh              正常构建
#   bash build.sh --dry-run    只预览分盘结果，不出盘
#   bash build.sh --config     只打印当前配置后退出
#
# 所有路径来自 config.sh。日志写入 <BUILD_DIR>/build.log。
# ============================================================================
set -e
set -o pipefail
cd "$(dirname "$0")"

HERE="$(pwd)"

# ---- 加载配置 ----
if ! CFG_SH="$(python3 "$HERE/dvda_config.py" --shell)"; then
  echo "[配置错误] 无法读取 config.sh" >&2
  exit 2
fi
eval "$CFG_SH"

# ---- 检查必需项 ----
if ! python3 "$HERE/dvda_config.py" --check; then
  exit 2
fi

if [ "${1:-}" = "--config" ]; then
  python3 "$HERE/dvda_config.py"
  exit 0
fi

DRY=""
[ "${1:-}" = "--dry-run" ] && DRY="--dry-run"

LOG="$DVDA_BUILD_LOG"
mkdir -p "$DVDA_BUILD_DIR"

# ---- 环境自检 ----
echo "============================================================"
echo " DVD-Audio Maker"
echo "============================================================"
echo "  音源     : $DVDA_SRC"
echo "  输出     : $DVDA_FINAL_DIR"
echo "  工作目录 : $DVDA_BUILD_DIR"
echo "  光盘标题 : $DVDA_TITLE    (卷标: \"$DVDA_TITLE 1\", ... ; 文件名前缀: $DVDA_ISO_PREFIX)"
echo "  日志     : $LOG"
echo

missing=0
for t in "$DVDA_AUTHOR" "$DVDA_MKISOFS"; do
  if [ ! -x "$t" ]; then
    echo "[缺少] $t" >&2
    missing=1
  fi
done
if ! command -v "$DVDA_FFMPEG" >/dev/null 2>&1; then
  echo "[缺少] $DVDA_FFMPEG（不在 PATH 中）" >&2
  missing=1
fi
if [ ! -d "$DVDA_SRC" ]; then
  echo "[缺少] 音源目录不存在: $DVDA_SRC" >&2
  missing=1
fi
if [ "$missing" = 1 ]; then
  echo >&2
  echo "请修改 config.sh 后重试；工具需先按 README 编译安装。" >&2
  exit 2
fi
echo "环境自检通过 ✔"
echo

# ---- 两步流水线 ----
echo "===== 步骤1:扫描音源 + 专辑归一化 + 解码完整性校验 ====="
python3 -u "$HERE/01_prepare.py" 2>&1 | tee "$LOG"

echo
echo "===== 步骤2:MLP 编码 + dvda-author 出盘 + mkisofs 打包 ====="
python3 -u "$HERE/02_build.py" $DRY 2>&1 | tee -a "$LOG"

echo
if [ -n "$DRY" ]; then
  echo "===== DRY-RUN 完成（未出盘）====="
else
  echo "===== 完成 ====="
  echo "产物: $DVDA_FINAL_DIR"
fi
echo "日志: $LOG"
echo
echo "下一步可运行:  bash verify.sh"
