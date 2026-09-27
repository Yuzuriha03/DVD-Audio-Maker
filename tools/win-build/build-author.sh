#!/bin/bash
# 构建 dvda-author（含 24-bit 无损 MLP）+ 菜单所需的其他 exe。
#
# 全部在 MSYS2/MinGW-w64 下完成 —— **不需要 WSL**。
set -e
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
. "$HERE/common.sh"

LOGDIR="$KIT/logs"
mkdir -p "$LOGDIR"

export ROOTDIR="$SRC"
cd "$SRC"

step "[1/5] configure"
# ⚠️ 这一步在 Windows 上**可能**很慢，快慢取决于杀软的**内核过滤驱动**：
#
#   有卡巴斯基（21 个 kl* 过滤驱动驻留）时：每个 check ≈ 15 秒，
#       fork/exec ≈ 2~4 秒/次 → 整个 configure ≈ 1 小时
#   卸掉卡巴斯基 + 重启后：               每个 check ≈ 0.4 秒，
#       fork/exec ≈ **27 ms**/次  → 整个 configure ≈ 2~3 分钟
#
#   即快慢是 **100 倍**的差距，不是「MSYS2 fork 天生慢」。
#   （测量要注意：早期我用 `$(date)` 取时间戳，而 `date` 本身就是个 fork，
#    测出来的值被它自己要测的开销撑大了 —— 改用内建 $EPOCHREALTIME 才准。）
#
# ★ 这里**每次都真跑 configure**，不做缓存。
#   曾经做过「把 configure 产物存下来复用」的机制，已去掉，理由：
#     · configure 产物里写死了源码树的绝对路径，复用时要 sed 改写 + 自校验，
#       而那条路径正是最容易出错的地方（踩过：cygpath 把路径压成 `/`，
#       于是缓存恢复出一个路径错乱的 Makefile，报一堆莫名找不到文件）
#     · 误判的代价不对称：缓存失效没检测出来 → 编译出错误的东西且**能通过**；
#       而多跑一次 configure 只是花几分钟
#     · 快环境（无杀软驱动）下它本来只要 2~3 分钟，为省这点时间引入
#       一层路径改写逻辑，不值得
#   ⇒ 宁可慢一点，也要行为确定。手动缓存需求请自行处理源码树。
rm -f config.h config.status config.log
rm -f Makefile src/Makefile libutils/src/Makefile libfixwav/src/Makefile

# CFLAGS 与 Linux 侧保持一致：
#   -Wno-error=incompatible-pointer-types  上游代码本来就需要的
#   -Wno-error=implicit-function-declaration  Windows 上还有几处 POSIX 函数
export CFLAGS="-O2 -DWITHOUT_sox -Wno-error=incompatible-pointer-types -Wno-error=implicit-function-declaration"

./configure \
    --prefix="$SRC/install" \
    CPPFLAGS="-DWITHOUT_sox -I$SRC/local/include" \
    LDFLAGS="-L$SRC/local/lib" \
    CFLAGS="$CFLAGS" \
    > "$LOGDIR/configure.log" 2>&1

grep -m1 -E 'define HAVE_ffmpeg '     config.h
grep -m1 -E 'define HAVE_core_BUILD ' config.h

step "[2/5] 填充 local/（FFmpeg 导入库 + 头文件）"
# ⚠️ 必须在 configure **之后**：configure 里有 `rm -rf "local" && mkdir "local"`，
#    它在 configure 第 4288 行 —— 先放进去会被清掉。
mkdir -p local/lib local/include
for l in avformat avfilter avcodec avutil swresample swscale; do
    [ -f "$MSYS/lib/lib$l.dll.a" ] && cp -f "$MSYS/lib/lib$l.dll.a" "local/lib/lib$l.a"
done
for h in libavcodec libavformat libavutil libavfilter libswresample libswscale; do
    [ -d "$MSYS/include/$h" ] && cp -rf "$MSYS/include/$h" local/include/
done
echo "  local/lib:     $(ls -1 local/lib | tr '\n' ' ')"
echo "  local/include: $(ls -1 local/include | tr '\n' ' ')"

step "[2b/5] UTF-8 argv manifest（Windows 上的必需修复）"
# ── 问题 ────────────────────────────────────────────────────────────────
# MinGW 程序拿到的 argv 是 **ANSI（系统代码页）** 编码。简体中文机器上是
# CP936，编不出韩文谚文（자유로운 영혼의왕）和部分日文/中文字符 —— 文件名在
# argv 里变成 '?'，dvda-author 于是报：
#     [ERR] ...EP/04. ??? ?? ??.mlp n'est pas un fichier.
#
# ── 修复 ────────────────────────────────────────────────────────────────
# 把声明 activeCodePage=UTF-8 的 manifest（资源类型 24）链进 exe。
#
# ⚠️ 关键陷阱：光加 da-utf8.o **不生效**。GCC 的 specs 里有
#       %{!shared:%:if-exists(default-manifest.o%s)}
#    它会自动把 mingw 自带的 default-manifest.o 链进去，**排在前面并覆盖**
#    我们的 manifest（链接器取第一个）。所以必须把
#    <MSYS>/lib/default-manifest.o 换成空对象，同时备份原文件。
DFM="$MSYSBASE/mingw64/lib/default-manifest.o"
[ -f "$DFM" ] || DFM="$MSYS/lib/default-manifest.o"
if [ -f "$DFM" ]; then
    [ -f "$DFM.orig" ] || cp -p "$DFM" "$DFM.orig"
    printf 'int __dvda_empty_default_manifest;\n' > "$SRC/da-empty.c"
    gcc -c "$SRC/da-empty.c" -o "$DFM"
    echo "  default-manifest.o 已置空（备份 $DFM.orig）"
else
    echo "  [警告] 找不到 $DFM —— manifest 可能被 mingw 自带的覆盖"
fi

cp -f "$HERE/da-utf8.manifest" "$SRC/src/da-utf8.manifest"
cp -f "$HERE/da-utf8.rc"       "$SRC/src/da-utf8.rc"

# ⚠️ 不能把 da-utf8.o 加进 OBJECTS —— Makefile 里有静态模式规则
#       $(OBJECTS): %.o: $(ROOT)/src/%.c
#    make 会**合并同一目标的多个规则的前提**，于是 da-utf8.o 多出一个
#    不存在的 da-utf8.c 前提：
#       make[1]: *** 没有规则可制作目标".../src/da-utf8.c"
#    而且**编译错误计数为 0**，很容易误判成「没报错但没产物」。
# ✔ 正确接法：把它作为 dvda-author 的**前提**。该目标没有显式配方，
#    用 make 内建 `%: %.o` 规则，它取 **$^（全部前提）** 拼进链接命令。
grep -q 'da-utf8\.o' "$SRC/src/Makefile" || cat >> "$SRC/src/Makefile" <<'MKEOF'

# --- UTF-8 argv manifest ---
da-utf8.o: da-utf8.rc da-utf8.manifest
	windres -i $< -o $@

dvda-author: da-utf8.o
MKEOF

step "[3/5] 清理旧对象"
# ⚠️ Makefile **不跟踪头文件依赖** —— 改过任何 .h 都必须清 .o，
#    否则会拿到按旧结构体布局编出来的对象，行为诡异。
find src libutils libfixwav -name '*.o' -delete 2>/dev/null || true
rm -f src/libfixwav.a libfixwav/src/libfixwav.a libutils/src/libc_utils.a
rm -f src/dvda-author src/dvda-author.exe src/dvda-author-dev src/dvda-author-dev.exe

step "[4/5] make"
set +e
make -k -j"${JOBS:-$(nproc)}" DEBUG_FLAGS=1 > "$LOGDIR/make.log" 2>&1
set -e
echo "  编译错误数: $(grep -cE 'error:' "$LOGDIR/make.log" || true)"

step "[5/5] 归一化产物名"
cd "$SRC/src"
TARGET=dvda-author-dev.exe
rename_if_distinct() {
    src=$1
    [ -f "$src" ] || return 0
    # ⚠️ MSYS2 把 `foo` 与 `foo.exe` 视作**同一个文件**（exe 后缀魔法），
    #    所以直接 mv 会报「为同一文件」并**非零退出**，被 set -e 抓住让
    #    整个构建报失败 —— 而产物其实早就好了。先判 inode。
    [ "$src" -ef "$TARGET" ] 2>/dev/null && return 0
    mv -f "$src" "$TARGET"
    echo "  $src -> $TARGET"
}
rename_if_distinct dvda-author
rename_if_distinct dvda-author.exe
rename_if_distinct dvda-author-dev

hr
if [ -f "$TARGET" ]; then
    ls -l "$TARGET"
    n=$(grep -ac activeCodePage "$TARGET" || true)
    printf '  UTF-8 manifest: activeCodePage=%s (应为 1)\n' "$n"
    if [ "$n" -ne 1 ]; then
        echo "  [警告] manifest 未嵌入！检查 default-manifest.o 是否已置空"
    fi
else
    echo "  （未产出）—— 见 $LOGDIR/make.log"
    exit 1
fi
