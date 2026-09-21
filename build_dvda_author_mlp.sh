#!/bin/bash
# ============================================================================
# 一键构建「支持 24-bit 无损 MLP 的 dvda-author」
#
# 产物路径 = config.sh 的 DVDA_AUTHOR
#            （默认 /root/dvda-author-mlp8/src/dvda-author-dev）
#            另外会在 <tools>/menu-bin/ 里摆好菜单所需的辅助程序
#            （打过补丁的 dvdauthor、spumux + mjpegtools / ImageMagick 链接）
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
#   · 要用菜单还需要：apt install mjpegtools imagemagick
#       （dvdauthor/spumux 由本脚本自己编译，不装系统包 ——
#        apt 里的 dvdauthor 没有 AMGM / jump group 补丁，不能用）
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

echo "=== [0/8] 配置 ==="
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

echo "=== [1/8] 准备源码树 ==="
if [ ! -d "$SRC" ]; then
  cp -a "$ORIG" "$SRC"
  echo "已复制 $ORIG → $SRC"
else
  echo "已存在 $SRC"
fi

echo "=== [1b] 还原将被补丁修改的源文件（保证可重复执行） ==="
for f in src/mlp.c \
         src/ats.c \
         src/atsi2.c \
         src/amg2.c \
         src/dvda-author.c \
         src/menu.c \
         src/xml.c \
         src/include/menu.h \
         src/include/structures.h \
         src/include/commonvars.h \
         src/auxiliary.c \
         src/command_line_parsing.c \
         src/launch_manager.c \
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

echo "=== [2/8] 应用与 FFmpeg 版本无关的基础修复 ==="
python3 "$PATCHES/patch_base.py"

# fn_strtok() 的越界写是通用缺陷（--stillpics 的空项=空串就触发），
# 与菜单无关，但菜单的「每专辑一张封面」必然用到空项，所以一并修。
python3 "$PATCHES/patch_fix_fn_strtok.py"

echo "=== [3/8] configure（关闭不兼容的 SoX，启用 FFmpeg 音频栈） ==="
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

echo "=== [4/8] 关联系统 FFmpeg 8 与编译选项（Makefile 由 configure 生成，需在 configure 后处理） ==="
# 系统无 libavfilter.so 时从链接行移除
if [ ! -e "$SYS_LIB/libavfilter.so" ]; then
  sed -i '/libavfilter\.a/d' "$SRC/src/Makefile"
  echo "已从链接行移除 libavfilter"
fi
# 去掉链接期 strip(-s)，便于崩溃时定位
sed -i 's/ -s  dvda-author.o/ dvda-author.o/' "$SRC/src/Makefile" || true

echo "=== [5/8] 迁移 mlp.c 到 FFmpeg 8 API ==="
python3 "$PATCHES/patch_read.py"      # channels/ch_layout、pkt_pos、av_read_frame
python3 "$PATCHES/patch_read2.py"     # 提取分支的读取循环
python3 "$PATCHES/patch_encode.py"    # planer 采样格式、放开 24-bit
python3 "$PATCHES/patch_ats_pack.py"  # pack 补到 2048 边界（否则每盘丢 1 轨）
python3 "$PATCHES/patch_atsi_dynamic.py"  # ATSI 按曲目数动态分配（一组可达 99 轨）
python3 "$PATCHES/patch_stillpics_atsi_record.py"  # 播放封面不能被跳过
# 上游因「MLP 不能无缝接轨」的猜测让每轨自成 title，而「下一段」是在
# 同一 title 内换轨 → 点「下一段」切不了歌。去掉该条件，一个组 = 一个 title。
python3 "$PATCHES/patch_mlp_one_title.py"

echo "=== [5b/8] 应用菜单（AMG / ASVS）修复 ==="
# 顺序敏感：layout 会把 command->maxntracks 换成 img->maxbuttons，
# 而 maxbuttons 由 paging 决定，所以 paging 必须先跑。
for p in patch_menu_paging.py \
         patch_menu_backgrounds.py \
         patch_menu_screentext.py \
         patch_menu_layout.py \
         patch_menu_arrows.py \
         patch_menu_stillpics.py \
         patch_menu_stillpics_list.py \
         patch_menu_amg_size.py \
         patch_menu_amg_cells.py \
         patch_menu_one_album_per_page.py ; do
  echo "  -- $p"
  python3 "$PATCHES/$p" || { echo "[FAIL] $p 未全部应用" >&2; exit 3; }
done

echo "=== [6/8] 清理旧对象并重建系统库链接 ==="
# 注意：只清理构建产物，不能删除 local/ 下的库符号链接
find "$SRC/src" "$SRC/libutils" "$SRC/libfixwav" -name '*.o' -delete 2>/dev/null || true
rm -f "$SRC/src/libfixwav.a" "$SRC/libfixwav/src/libfixwav.a" \
      "$SRC/libutils/src/libc_utils.a" "$SRC/src/dvda-author" "$SRC/src/dvda-author-dev"

rm -rf "$SRC/local"
mkdir -p "$SRC/local/lib"
for l in avcodec avformat avutil swresample; do
  ln -sfn "$SYS_LIB/lib$l.so" "$SRC/local/lib/lib$l.a"
done

echo "=== [7/8] 编译 ==="
set +e
# DEBUG_FLAGS=1 会让 Makefile 不往 LDFLAGS 里加 -s —— 保留调试符号，
# 出现段错误时 gdb 才能拿到函数名与行号。
make -j2 DEBUG_FLAGS=1 > "$LOGDIR/mlp8-make.log" 2>&1
STATUS=$?
set -e

if [ $STATUS -ne 0 ]; then
  echo "编译失败（日志: $LOGDIR/mlp8-make.log）"
  grep -n 'error:' "$LOGDIR/mlp8-make.log" | head -n 15
  exit $STATUS
fi

# Makefile 的 all: 目标末尾有 "mv -f dvda-author $(PROGRAM)"，
# 但 PROGRAM 变量在本项目的 configure 产物里取不到值 → mv 会失败、
# 产物仍叫 dvda-author。这里手工改名。
if [ -f "$SRC/src/dvda-author" ]; then
  mv -f "$SRC/src/dvda-author" "$SRC/src/dvda-author-dev"
fi

echo "=== [8/8] 编译菜单辅助程序并搭建 menu-bin ==="
# 菜单（DVD-Audio 的 AMG 菜单）要用到：
#   · dvdauthor —— **必须**是打过 AMGM 补丁的版本：菜单按钮写的是
#     `<button>jump group G track K</button>` 这种跳转语法，apt 里的
#     0.7.2 不认。所以这里从源码自己编。
#   · spumux    —— 做按钮子画面（与 dvdauthor 同一份源码）
#   · mjpegtools / ImageMagick —— 菜单文字与背景的绘制/编码，来自系统包
MENU_SRC="$SRC/dvdauthor-0.7.1"
BINDIR="$(dirname "$SRC")/menu-bin"

if [ -d "$MENU_SRC" ]; then
  # 源码包缺 autotools 需要的辅助文件，先补齐
  mkdir -p "$MENU_SRC/srcm4"
  for m in iconv.m4 lib-ld.m4 lib-link.m4 lib-prefix.m4 libxml.m4; do
    [ -f "$SRC/m4.extra.dvdauthor/$m" ] && \
      cp -f "$SRC/m4.extra.dvdauthor/$m" "$MENU_SRC/srcm4/$m"
  done
  if [ ! -f "$MENU_SRC/autotools/compile" ]; then
    AM_COMPILE="$(ls /usr/share/automake-*/compile 2>/dev/null | tail -n1)"
    [ -n "$AM_COMPILE" ] && cp -f "$AM_COMPILE" "$MENU_SRC/autotools/compile"
  fi
  # 统一时间戳：否则 make 会认为 aclocal.m4 / Makefile.in 过期，
  # 去调本机没有的 aclocal-1.16 / automake-1.16 而失败
  find "$MENU_SRC" -type f -exec touch -d "2020-01-01 00:00:00" {} + \
    2>/dev/null || true

  rm -rf "$MENU_SRC/build-menu"
  mkdir -p "$MENU_SRC/build-menu"
  set +e
  ( cd "$MENU_SRC/build-menu" && \
    ../configure --prefix="$BINDIR" --disable-dvdunauthor \
      > "$LOGDIR/menu-configure.log" 2>&1 && \
    make -j2 > "$LOGDIR/menu-make.log" 2>&1 )
  MENU_STATUS=$?
  set -e
  if [ $MENU_STATUS -ne 0 ]; then
    echo "  [警告] dvdauthor 编译失败（日志: $LOGDIR/menu-make.log）"
    grep -n 'error:' "$LOGDIR/menu-make.log" 2>/dev/null | head -n 5
    echo "         菜单功能将不可用；不影响出盘（可把 DVDA_MENU 设为 off）"
  fi
else
  echo "  [警告] 找不到 $MENU_SRC —— 跳过菜单辅助程序"
fi

mkdir -p "$BINDIR"
for b in dvdauthor spumux; do
  [ -f "$MENU_SRC/build-menu/src/$b" ] && \
    ln -sf "$MENU_SRC/build-menu/src/$b" "$BINDIR/$b"
done
# mjpegtools 与 ImageMagick 用系统的（dvda-author 按 --bindir 里的名字找）
for t in jpeg2yuv mpeg2enc mplex mp2enc mogrify convert; do
  p="$(command -v "$t" 2>/dev/null || true)"
  [ -n "$p" ] && ln -sf "$p" "$BINDIR/$t"
done

echo "  menu-bin: $BINDIR"
MISSING_MENU=""
for b in dvdauthor spumux jpeg2yuv mpeg2enc mplex mp2enc mogrify convert; do
  [ -e "$BINDIR/$b" ] || MISSING_MENU="$MISSING_MENU $b"
done
if [ -n "$MISSING_MENU" ]; then
  echo "  [警告] 缺少:$MISSING_MENU"
  echo "         菜单需要它们；缺 mjpegtools/ImageMagick 时请："
  echo "           sudo apt install mjpegtools imagemagick"
else
  echo "  菜单辅助程序齐备 ✔"
fi

echo
BUILT="$SRC/src/dvda-author-dev"
if [ ! -f "$BUILT" ]; then
  echo "[FAIL] 未产出 $BUILT" >&2
  exit 4
fi
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
