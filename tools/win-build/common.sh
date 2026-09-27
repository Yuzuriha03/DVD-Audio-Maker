#!/bin/bash
# ============================================================================
#  共用设置 —— 所有步骤 source 这个文件。
#
#  设计原则（两条，都很重要）：
#
#   1) **位置无关**：一切路径从本文件自己的位置推出来。
#      整包可以放在任何目录（D:\build、C:\dvda、U 盘…），不需要改脚本。
#
#   2) **零 WSL 调用**：只依赖 MSYS2（那是 Windows 原生环境）。
#      本文件里不会出现 wsl.exe / \\wsl.localhost / /mnt/c 之类。
#
#  需要 source 后得到：
#     KIT         本工具包目录（日志写这里）
#     SRC         源码树（含 src/ libutils/ dvdauthor-0.7.1/ menu/ local.w10/）
#     MSYS        MSYS2 的 mingw64 前缀（如 /mingw64）
#     MSYS_ROOT   MSYS2 安装根（如 /d/dev/msys64）
# ============================================================================

# ---- 本工具包位置 ----
KIT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

# ---- Windows 路径 -> MSYS 路径 ----
# 让调用方（.bat / 命令行）可以**直接传 `D:\x\y` 这种 Windows 路径**，
# 不必自己往 /d/x/y 上转 —— 那件事很容易出错，而且 .bat 里做子串运算很难看。
#
# ★ 故意**不用 cygpath**（实测踩到，很阴）：
#       cygpath -u 'D:\dev\msys64'  ->  /              ← 坏！
#       cygpath -m 'D:\dev\msys64'  ->  D:/dev/msys64  ← 对
#       cygpath -u 'D:\'            ->  /d/            ← 单级反而对
#   在 MSYSTEM=MINGW64 下 cygpath -u 会把**多级** Windows 路径压成 `/`。
#   后果：`[ -d "$MSYS2_ROOT" ]` 对 `/` 成立 ⇒ 「指定 MSYS2_ROOT」这个功能
#   整个失灵，而且**完全不报错**（根变成 `/`，之后的标准挂载点探测照样
#   能找到 /mingw64）。症状只在别处显现：某些子脚本里根是 `//`、MSYS 变成
#   `//ucrt64`，集 DLL 时取不到 msys-2.0.dll。
#   纯字符串转换则完全确定，且**零 fork**（不需要 cygpath/tr/sed）。
to_unix() {
    local p="${1:-}" d rest
    [ -n "$p" ] || { printf ''; return 0; }
    # 已经是 unix 形式就不动
    case "$p" in /*) printf '%s' "$p"; return 0 ;; esac
    # 去掉 Windows 的长路径前缀 \\?\ 和 //./
    p="${p#\\\\?\\}"
    case "$p" in
        [A-Za-z]:*)
            d="${p:0:1}"; d="${d,,}"      # bash 4 的 ${var,,} 小写化，零 fork
            rest="${p:2}"
            rest="${rest//\\//}"           # 反斜杠全换成正斜杠
            printf '/%s%s' "$d" "$rest"
            ;;
        *) printf '%s' "$p" ;;
    esac
}

# 允许环境变量里写 Windows 路径
[ -n "${MSYS2_ROOT:-}" ]      && MSYS2_ROOT="$(to_unix "$MSYS2_ROOT")"
[ -n "${DVDA_SRC_TREE:-}" ]   && DVDA_SRC_TREE="$(to_unix "$DVDA_SRC_TREE")"
[ -n "${DVDA_FONT_SRC:-}" ]   && DVDA_FONT_SRC="$(to_unix "$DVDA_FONT_SRC")"
[ -n "${DVDA_SCRIPTS:-}" ]    && DVDA_SCRIPTS="$(to_unix "$DVDA_SCRIPTS")"
[ -n "${DVDA_WIN_ROOT:-}" ]   && DVDA_WIN_ROOT="$(to_unix "$DVDA_WIN_ROOT")"
export MSYS2_ROOT DVDA_SRC_TREE DVDA_FONT_SRC DVDA_SCRIPTS

# ---- 找 MSYS2 ----
# 思路：直接探测 **MSYS2 的 mingw 前缀**（/mingw64 /clang64 /ucrt64 /mingw32…），
# 找到后 MSYS_ROOT 就是它的上一层。
#
# ⚠️ 注意：在 MSYS2 里 `MSYS_ROOT` 通常就是 **`/`** —— 这不是 bug，
#    因为 MSYS2 的 `/` 就是安装根（`C:\msys64` 之类）的挂载点。
#    要用 Windows 形式看它时用 `cygpath -w /`。
find_msys_prefix() {
    local pfx
    # 1) 环境变量直接指定
    if [ -n "${MSYS2_ROOT:-}" ] && [ -d "$MSYS2_ROOT" ]; then
        for pfx in "$MSYS2_ROOT/mingw64" "$MSYS2_ROOT/clang64" \
                   "$MSYS2_ROOT/ucrt64" "$MSYS2_ROOT/mingw32"; do
            [ -d "$pfx/lib/pkgconfig" ] && { echo "$pfx"; return 0; }
        done
    fi
    # 2) 探测标准挂载点
    for pfx in /mingw64 /clang64 /ucrt64 /mingw32 /clang32; do
        [ -d "$pfx/lib/pkgconfig" ] && { echo "$pfx"; return 0; }
    done
    # 3) 从 pkg-config 的位置反推
    local pc
    pc="$(command -v pkg-config 2>/dev/null)"
    if [ -n "$pc" ]; then
        pfx="$(cd "$(dirname "$pc")/.." && pwd)"
        [ -d "$pfx/lib/pkgconfig" ] && { echo "$pfx"; return 0; }
    fi
    return 1
}

if ! MSYS="$(find_msys_prefix)"; then
    cat >&2 <<'EOM'
[失败] 找不到 MSYS2 的 mingw 前缀（/mingw64 之类）。请任选一种：
  · 设环境变量 MSYS2_ROOT=D:\msys64
  · 或把 MSYS2 装到 C:\msys64 / D:\msys64
安装（免安装版）：
  1) 下载 msys2-base-x86_64-*.tar.xz
  2) 解压到某目录（Windows 自带 tar.exe 即可）
  3) 首次运行 <root>\usr\bin\bash.exe，然后：
       pacman -Syu
       pacman -S --needed mingw-w64-x86_64-gcc \
                          mingw-w64-x86_64-ffmpeg \
                          mingw-w64-x86_64-pkgconf \
                          mingw-w64-x86_64-freetype \
                          mingw-w64-x86_64-fontconfig \
                          mingw-w64-x86_64-libpng \
                          mingw-w64-x86_64-imagemagick \
                          mingw-w64-x86_64-python-fonttools \
                          make
EOM
    exit 1
fi
# ---- 归一化前缀，并由它推出安装根 ----
#
# ⚠️ 三条规矩，每条都对应一个实测踩到的坑：
#
#  1) **压掉多于一个的前导斜杠**。MSYS2 把 `//x` 当 UNC，`[ -e //usr/bin/x ]`
#     为假；但 `///x` 又会被折回 `/x` 而为真 —— 于是这类 bug **不报错**，
#     只是让 `[ -f ]`/PATH 查找静静地失败。
#     所以 `$MSYS` 必须**恰好一个**前导斜杠。
#
#  2) **不要用 `cd .. && pwd` 推安装根**。那会 fork 一个子壳；更要命的是
#     在 MSYS2 里 `/mingw64/..` 就是 `/`，得到的字符串是 `/`，
#     再去拼 `/usr/bin` 就成了 `//usr/bin`（见 1）。
#     改用纯字符串 `${VAR%/*}`。
#
#  3) **拼路径一律用 `MSYSBASE`**。它去掉尾部斜杠：根为 `/` 时是空串，
#     于是 `"$MSYSBASE/usr/bin"` 正好是 `/usr/bin`。
_normalize_slashes() {
    local n="$1"
    [ -n "$n" ] || { printf ''; return 0; }
    while [ "${n#//}" != "$n" ]; do n="/${n#//}"; done
    printf '%s' "$n"
}

MSYS="$(_normalize_slashes "$MSYS")"
MSYS="${MSYS%/}"
[ -n "$MSYS" ] || MSYS=/mingw64

# 归一化之后才能判 pkgconfig —— 之前 MSYS 可能带 `//`，那样会误报
if [ ! -d "$MSYS/lib/pkgconfig" ]; then
    echo "[失败] MSYS2 前缀无效：MSYS='$MSYS'" >&2
    exit 1
fi

# 安装根 = 前缀的上一级（纯字符串）
case "$MSYS" in
    /*/*) MSYSBASE="${MSYS%/*}" ;;
    *)   MSYSBASE="" ;;
esac
MSYSBASE="$(_normalize_slashes "$MSYSBASE")"
MSYSBASE="${MSYSBASE%/}"
MSYS_ROOT="${MSYSBASE:-/}"

# 供**显示**用的 Windows 形式（纯字符串，零 fork）。
# ⚠️ 在 MSYS2 里安装根在 bash 里就是 `/`，那不是 bug —— 但打印成
#    `MSYS2 : /` 会让人以为路径解析坏了。显示成 `D:/dev/msys64` 才直观。
unix_to_win() {
    local p="$1" d
    [ -n "$p" ] || { printf ''; return 0; }
    case "$p" in
        /*/*) d="${p:1:1}"; d="${d^^}"; printf '%s:%s' "$d" "${p:2}" ;;
        *)   printf '%s' "$p" ;;
    esac
}
MSYS_ROOT_WIN="$(unix_to_win "$MSYS_ROOT")"

export MSYSBASE MSYS_ROOT MSYS MSYS_ROOT_WIN
export PATH="$MSYS/bin:$MSYSBASE/usr/bin:$PATH"
export PKG_CONFIG_PATH="$MSYS/lib/pkgconfig:$MSYS/share/pkgconfig"

# ---- 调用子脚本时用的 bash ----
# ⚠️ **不要用裸 `bash`**。实测：在 Windows 的 cmd/PowerShell 里
#      where.exe bash
#        -> C:\Users\<u>\AppData\Local\Microsoft\WindowsApps\bash.exe
#    那是 **WSL 的启动器**，不是 MSYS2。进了 MSYS2 之后 PATH 确实会先
#    命中 /usr/bin/bash，所以裸 `bash` 眼下能用；但一旦 PATH 被改坏、
#    或被从别的 shell 调起，就会静默跑到 WSL 里去（那是完全不同的
#    Linux 环境，工具链、路径、字体全不一样，报错还很难懂）。
#    $BASH 就是“正在跑的这个 bash”自己的路径，最准。
SELF_BASH="${BASH:-$MSYSBASE/usr/bin/bash}"
[ -x "$SELF_BASH" ] || SELF_BASH="$MSYSBASE/usr/bin/bash"
export SELF_BASH

# 自检 —— 刻意加的：上面那类拼接错误**不会**让构建失败，只会让各步骤
# 静默跳过（manifest 置空失效、DLL 漏收、automake/compile 找不到…），
# 编译输出照样是 "OK"。宁可在入口处直接停下来。
if [ ! -d "$MSYSBASE/usr/bin" ] || [ ! -d "$MSYS/lib/pkgconfig" ]; then
    echo "[失败] MSYS2 路径拼接异常，拒绝继续：" >&2
    echo "        MSYS_ROOT='$MSYS_ROOT'  MSYSBASE='$MSYSBASE'  MSYS='$MSYS'" >&2
    exit 1
fi

# ---- 绝不能跑到 WSL 里去 ----
# 本工具包的卖点就是「关掉 WSL 也能构建」。但在 Windows 上裸 `bash` 会命中
#     C:\Users\<u>\AppData\Local\Microsoft\WindowsApps\bash.exe
# 那是 **WSL 的启动器**（本机 `where.exe bash` 实测就指向它）。
# 进了 MSYS2 之后 /usr/bin 通常优先，所以裸 `bash` 有时也能跑 —— 但那意味着
# 构建是否走 WSL 取决于 PATH，是**静默**的、机器相关的。这里直接判定：
# 一旦检测到 WSL 痕迹就拒绝继续。
if [ -n "${WSL_DISTRO_NAME:-}${WSL_INTEROP:-}" ]; then
    echo "[失败] 检测到 WSL（WSL_DISTRO_NAME=${WSL_DISTRO_NAME:-<unset>}）。" >&2
    echo "        本工具包必须跑在 MSYS2 里，请用 build-all.bat 启动。" >&2
    exit 1
fi
case "$(uname -s 2>/dev/null)" in
    Linux|*Linux*)
        echo "[失败] uname -s 报的是 Linux —— 这是 WSL/容器，不是 MSYS2。" >&2
        echo "        本工具包必须跑在 MSYS2 里，请用 build-all.bat 启动。" >&2
        exit 1
        ;;
esac
# 正在跑的这个 shell 自己必须属于 MSYS2 安装树
# （纯字符串比较，**不用 cygpath** —— `cygpath -u` 在本机会把多级路径压成 `/`）
if [ -n "${BASH:-}" ]; then
    case "${BASH%/*}" in
        "$MSYSBASE/usr/bin"|"$MSYSBASE/bin"|/usr/bin|/bin) ;;
        *)
            echo "[警告] 当前 bash 不在 MSYS2 树内：BASH=$BASH" >&2
            echo "        期望 $MSYSBASE/usr/bin/bash。仍继续，但请留意。" >&2
            ;;
    esac
fi
# 裸 `bash` / `sh` 必须解析到 MSYS2，而不是 WindowsApps
for _c in bash sh; do
    _p="$(command -v "$_c" 2>/dev/null)" || continue
    case "$_p" in
        "$MSYSBASE"/usr/bin/*|"$MSYSBASE"/bin/*|/usr/bin/*|/bin/*) ;;
        *)
            echo "[警告] \`$_c\` 解析到 $_p（不是 MSYS2）—— 子脚本可能误入 WSL。" >&2
            echo "        脚本内部统一用 \$SELF_BASH，不用裸 bash。" >&2
            ;;
    esac
done
unset _c _p

# ---- 找源码树 ----
# 优先 $DVDA_SRC_TREE；否则找 <kit>/src 或 <kit>/../src。
find_src() {
    for c in "${DVDA_SRC_TREE:-}" "$KIT/src" "$KIT/../src"; do
        [ -n "$c" ] || continue
        # 认「这是源码树」的标志：src/Makefile.in + libutils + 菜单素材
        if [ -f "$c/src/Makefile.in" ] && [ -d "$c/libutils" ]; then
            (cd "$c" && pwd); return 0
        fi
    done
    return 1
}

if ! SRC="$(find_src)"; then
    cat >&2 <<'EOM'
[失败] 找不到源码树。请任选一种：
  · 设环境变量 DVDA_SRC_TREE=<源码树路径>
  · 或把源码树放在 <本工具包>/src
源码树应包含：
    configure  configure.ac  Makefile.in
    src/  libutils/  libfixwav/
    menu/                （silence.wav、activeheader —— C 代码引用）
    m4.extra.dvdauthor/  （dvdauthor 的 autotools 辅助 m4）
    dvdauthor-0.7.1/     （含 AMGM 补丁，菜单必需）
    local.w10/bin/       （mkisofs / mjpegtools 等预编译二进制）
EOM
    exit 1
fi

# 工具输出目录（menu-bin 就在源码树旁边）
BINDIR="$(dirname "$SRC")/menu-bin"
export BINDIR

# ---- 字体：源 ttc 与已抽好的单 face ----
#
# ★ 关键认识：**单 face OTF 才是构建真正需要的东西**，ttc 只是生成它的手段。
#   用户机器上 ttc 很可能是临时下载/临时拷来的，用完就没了；而抽好的 face
#   已经在 menu-bin/fonts 里（那是构建产物的一部分）。只认 ttc 会在这种
#   情况下报「找不到字体」，让人以为得重新弄一份 ttc —— 其实完全不必。
FONT_FACES="NotoSansCJKsc-Regular.otf NotoSansCJKjp-Regular.otf NotoSansCJKkr-Regular.otf"

find_font_ttc() {
    local c
    for c in "${DVDA_FONT_SRC:-}" \
             "$SRC/NotoSansCJK-Regular.ttc" \
             "$SRC/fonts/NotoSansCJK-Regular.ttc" \
             "$KIT/NotoSansCJK-Regular.ttc" \
             "$KIT/fonts/NotoSansCJK-Regular.ttc" \
             /c/Windows/Fonts/NotoSansCJK-Regular.ttc \
             /usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc; do
        [ -n "$c" ] && [ -f "$c" ] && { printf '%s' "$c"; return 0; }
    done
    return 1
}

# 三个单 face 都在哪个目录？输出目录路径。
font_faces_dir() {
    local d f
    for d in "${DVDA_FONT_DIR:-}" "$BINDIR/fonts" "$SRC/menu-bin/fonts" \
             "$KIT/fonts" "$KIT/../fonts"; do
        [ -n "$d" ] && [ -d "$d" ] || continue
        local ok=1
        for f in $FONT_FACES; do [ -f "$d/$f" ] || { ok=0; break; }; done
        [ "$ok" = "1" ] && { printf '%s' "$d"; return 0; }
    done
    return 1
}

# ---- 小工具 ----
log()  { printf '%s\n' "$*"; }
hr()   { printf '%s\n' "------------------------------------------------------------"; }
step() { printf '\n########## %s ##########\n' "$*"; }

# 需要 MSYS2 自带的 python（带了 fontTools）
find_py_fonttools() {
    local c
    for c in "$MSYS/bin/python.exe" python python3; do
        if command -v "$c" >/dev/null 2>&1 && \
           "$c" -c 'import fontTools' >/dev/null 2>&1; then
            echo "$c"; return 0
        fi
    done
    return 1
}
