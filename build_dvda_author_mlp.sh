#!/bin/bash
# ============================================================================
# 一键构建「支持 24-bit 无损 MLP 的 dvda-author」
#
# 产物路径 = config.sh 的 DVDA_AUTHOR
#            （默认 /root/dvda-author-mlp8/src/dvda-author-dev）
#
# 原理:
#   原版是 core 构建(无 MLP)，随包 FFmpeg 4.2.4 的 MLP 编码器只支持 16-bit
#   且读写路径有缺陷。这里改为链接「系统 FFmpeg 8」并把 mlp.c 迁移到 8.x API，
#   从而支持 24-bit 无损 MLP。
#
# 前置条件:
#   · 系统已装 FFmpeg 8 开发库
#       apt install libavcodec-dev libavformat-dev libavutil-dev libswresample-dev
#   · 已把 dvda-author 源码放到 DVDA_AUTHOR_ORIG（默认 /opt/dvda-author）
#       并打过 fixes/ 下的上游补丁、configure 过 core 构建
#
# 本脚本幂等：可重复执行（会还原被补丁修改的源文件后重新应用）。
# ============================================================================
set -e

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

# ---- 加载配置（若存在 config.sh；否则用内置默认值） ----
if [ -f "$HERE/config.sh" ] && [ -f "$HERE/dvda_config.py" ]; then
  if CFG_SH="$(python3 "$HERE/dvda_config.py" --shell 2>/dev/null)"; then
    eval "$CFG_SH"
  fi
fi

SRC="${DVDA_AUTHOR_SRC:-/root/dvda-author-mlp8}"
ORIG="${DVDA_AUTHOR_ORIG:-/opt/dvda-author}"
SYS_LIB="${DVDA_SYS_LIB:-/usr/lib/x86_64-linux-gnu}"
PATCHES="$HERE/patches"
LOGDIR="${DVDA_BUILD_DIR:-/root/dvda-build}"
# 期望产出的可执行文件（取自配置）
EXPECT="${DVDA_AUTHOR:-}"
mkdir -p "$LOGDIR"

echo "=== [0/7] 配置 ==="
echo "  源码目录 : $SRC"
echo "  原始源码 : $ORIG"
echo "  系统库   : $SYS_LIB"
echo "  日志目录 : $LOGDIR"
[ -n "$EXPECT" ] && echo "  期望产物 : $EXPECT"
echo

if [ ! -d "$ORIG" ]; then
  echo "[失败] 原始源码不存在: $ORIG" >&2
  echo "       请先 git clone https://github.com/fabnicol/dvda-author $ORIG" >&2
  exit 2
fi

echo "=== [1/7] 准备源码树 ==="
if [ ! -d "$SRC" ]; then
  cp -a "$ORIG" "$SRC"
  echo "已复制 $ORIG → $SRC"
else
  echo "已存在 $SRC"
fi

echo "=== [1b] 还原将被补丁修改的源文件（保证可重复执行） ==="
for f in src/mlp.c \
         src/ats.c \
         libutils/src/winport.c \
         libutils/src/include/winport.h \
         src/libsoxconvert.c ; do
  if [ -f "$ORIG/$f" ]; then
    cp -f "$ORIG/$f" "$SRC/$f"
    echo "  还原 $f"
  else
    echo "  [WARN] 缺少原始文件 $f"
  fi
done

echo "=== [2/7] 应用与 FFmpeg 版本无关的基础修复 ==="
python3 "$PATCHES/patch_base.py"

echo "=== [3/7] configure（关闭不兼容的 SoX，启用 FFmpeg 音频栈） ==="
cd "$SRC"
./configure \
  --enable-ffmpeg-build \
  --enable-minimal-build \
  --prefix="$SRC/install" \
  CPPFLAGS=-DWITHOUT_sox \
  CFLAGS="-g -O0 -Wno-error=incompatible-pointer-types -DWITHOUT_sox" \
  > "$LOGDIR/mlp8-configure.log" 2>&1
grep -m1 -E 'define HAVE_ffmpeg ' "$SRC/config.h"
grep -m1 -E 'define HAVE_core_BUILD ' "$SRC/config.h"

echo "=== [4/7] 关联系统 FFmpeg 8 与编译选项（Makefile 由 configure 生成，需在 configure 后处理） ==="
# 系统无 libavfilter.so 时从链接行移除
if [ ! -e "$SYS_LIB/libavfilter.so" ]; then
  sed -i '/libavfilter\.a/d' "$SRC/src/Makefile"
  echo "已从链接行移除 libavfilter"
fi
# 去掉链接期 strip(-s)，便于崩溃时定位
sed -i 's/ -s  dvda-author.o/ dvda-author.o/' "$SRC/src/Makefile" || true

echo "=== [5/7] 迁移 mlp.c 到 FFmpeg 8 API ==="
python3 "$PATCHES/patch_read.py"      # channels/ch_layout、pkt_pos、av_read_frame
python3 "$PATCHES/patch_read2.py"     # 提取分支的读取循环
python3 "$PATCHES/patch_encode.py"    # planer 采样格式、放开 24-bit

echo "=== [6/7] 清理旧对象并重建系统库链接 ==="
# 注意：只清理构建产物，不能删除 local/ 下的库符号链接
find "$SRC/src" "$SRC/libutils" "$SRC/libfixwav" -name '*.o' -delete 2>/dev/null || true
rm -f "$SRC/src/libfixwav.a" "$SRC/libfixwav/src/libfixwav.a" \
      "$SRC/libutils/src/libc_utils.a" "$SRC/src/dvda-author" "$SRC/src/dvda-author-dev"

rm -rf "$SRC/local"
mkdir -p "$SRC/local/lib"
for l in avcodec avformat avutil swresample; do
  ln -sfn "$SYS_LIB/lib$l.so" "$SRC/local/lib/lib$l.a"
done

echo "=== [7/7] 编译 ==="
set +e
make -j2 > "$LOGDIR/mlp8-make.log" 2>&1
STATUS=$?
set -e

if [ $STATUS -ne 0 ]; then
  echo "编译失败（日志: $LOGDIR/mlp8-make.log）"
  grep -n 'error:' "$LOGDIR/mlp8-make.log" | head -n 15
  exit $STATUS
fi

echo
BUILT="$SRC/src/dvda-author-dev"
echo "构建完成:"
ls -l "$BUILT"
"$BUILT" --help 2>&1 | grep -i -E '^\s+--encode' | head -n 2 || true
echo

# 若配置指定的产物路径与默认不同，提示是否需要调整 config.sh
if [ -n "$EXPECT" ] && [ "$EXPECT" != "$BUILT" ]; then
  echo "[提示] config.sh 里 DVDA_AUTHOR 指向:"
  echo "         $EXPECT"
  echo "       实际产物为:"
  echo "         $BUILT"
  echo "       请把 config.sh 的 DVDA_AUTHOR 改为实际路径，或建立软链接："
  echo "         ln -sf \"$BUILT\" \"$EXPECT\""
elif [ -n "$EXPECT" ]; then
  echo "[OK] 与 config.sh 的 DVDA_AUTHOR 一致 ✔"
fi
