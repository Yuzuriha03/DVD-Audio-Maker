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
to_unix() {
    local p="${1:-}"
    [ -n "$p" ] || { printf ''; return 0; }
    case "$p" in
        [A-Za-z]:[\\/]*)
            if command -v cygpath >/dev/null 2>&1; then
                cygpath -u "$p"
            else
                local d
                d="$(printf '%s' "${p:0:1}" | tr 'A-Z' 'a-z')"
                printf '/%s%s' "$d" "$(printf '%s' "${p:2}" | tr '\\' '/')"
            fi
            ;;
        [A-Za-z]:*)
            printf '%s' "$p" | tr '\\' '/' | sed 's|^\([A-Za-z]\):|/\L\1|'
            ;;
        *)
            printf '%s' "$p"
            ;;
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
MSYS_ROOT="$(cd "$MSYS/.." && pwd)"

# ⚠️ 归一化前缀，别拼出双斜杠。
#    MSYS2 的安装根在 bash 里就是 `/`，于是 `$MSYS_ROOT/mingw64` 会得到
#    **`//mingw64`** —— 而 MSYS2 把 `//` 当成 **UNC/网络路径**，所有
#    `[ -f //mingw64/... ]` 都不成立。症状是各步骤报「FFmpeg 未安装」之类的
#    **假阴性**，而同一台机器上路径明明存在（实测：
#    `[ -d //mingw64/include/libavcodec ]` = 假，`/mingw64/...` = 真）。
#    用 `${VAR%/}` 去尾斜杠再拼，`/` 与前缀两种情况都对。
MSYS="${MSYS_ROOT%/}/$(basename "$MSYS")"
export PATH="$MSYS/bin:$MSYS_ROOT/usr/bin:$PATH"
export PKG_CONFIG_PATH="$MSYS/lib/pkgconfig:$MSYS/share/pkgconfig"

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
