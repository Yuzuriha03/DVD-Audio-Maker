#!/bin/bash
# 构建 dvdauthor + spumux + spuunmux（菜单按钮子画面必需）。
#
# 为什么要自己编：菜单按钮写的是 `<button>jump group G track K</button>`
# 这种跳转语法，发行版里的 dvdauthor 不认（需要 AMGM 支持）。
set -e
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
. "$HERE/common.sh"

LOGDIR="$KIT/logs"
mkdir -p "$LOGDIR"

MENU_SRC="$SRC/dvdauthor-0.7.1"
export ROOTDIR="$SRC"
cd "$SRC"

if [ ! -d "$MENU_SRC" ]; then
    echo "[跳过] 源码树里没有 dvdauthor-0.7.1/ —— 菜单将不可用"
    echo "       （出盘本身仍可进行，把 config.sh 的 DVDA_MENU 设为 off）"
    exit 0
fi

step "[1/3] 补齐 autotools 需要的辅助文件"
mkdir -p "$MENU_SRC/srcm4"
for m in iconv.m4 lib-ld.m4 lib-link.m4 lib-prefix.m4 libxml.m4; do
    [ -f "$SRC/m4.extra.dvdauthor/$m" ] && \
        cp -f "$SRC/m4.extra.dvdauthor/$m" "$MENU_SRC/srcm4/$m"
done
# autotools/compile 由 automake 提供；MSYS2 里找一份
if [ ! -f "$MENU_SRC/autotools/compile" ]; then
    AM_COMPILE="$(ls "$MSYSBASE"/usr/share/automake-*/compile 2>/dev/null | tail -n1)"
    [ -n "$AM_COMPILE" ] && cp -f "$AM_COMPILE" "$MENU_SRC/autotools/compile"
fi

# 统一时间戳：否则 make 认为 aclocal.m4 / Makefile.in 过期，
# 会去调本机没有的 aclocal-1.16 / automake-1.16 而失败。
find "$MENU_SRC" -type f -exec touch -d "2020-01-01 00:00:00" {} + 2>/dev/null || true

step "[2/3] configure"
rm -rf "$MENU_SRC/build-menu-win"
mkdir -p "$MENU_SRC/build-menu-win"
cd "$MENU_SRC/build-menu-win"

# LIBPNG_CFLAGS/LIBS 要显式给：configure 里的 PKG_CHECK_MODULES 在 MSYS2 下
# 若拿不到 pkg-config 就会失败。头文件在 include/libpng16 下（不是 include/ 直下）。
../configure \
    --prefix="$BINDIR" \
    --disable-dvdunauthor \
    LIBPNG_CFLAGS="-I$MSYS/include/libpng16" \
    LIBPNG_LIBS="-L$MSYS/lib -lpng16" \
    > "$LOGDIR/dvdauthor-configure.log" 2>&1

step "[3/3] make"
# ⚠️ 只编这三个，**不编 mpeg2desc**：
#    mpeg2desc.c 用 select() 同时监听 stdin 与输出文件，而 **Win32 的 select()
#    只支持套接字**，不支持管道/普通文件 —— 无法等价移植，而我们用不到它。
# ⚠️ 目标名要带 .exe，且必须在 **src/ 子目录**里 make（顶层 Makefile 只有
#    all/install/clean 这类递归目标）。
set +e
make -j"${JOBS:-$(nproc)}" -C src dvdauthor.exe spumux.exe spuunmux.exe \
     > "$LOGDIR/dvdauthor-make.log" 2>&1
set -e

echo "  编译错误数: $(grep -cE 'error:' "$LOGDIR/dvdauthor-make.log" || true)"
hr
for b in dvdauthor spumux spuunmux; do
    if [ -f "src/$b.exe" ]; then
        ls -l "src/$b.exe" | sed 's/^/  /'
    else
        echo "  [缺] $b.exe"
    fi
done
