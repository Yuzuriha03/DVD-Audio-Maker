#!/bin/bash
# ============================================================================
#  一键构建全部 —— Windows 原生，**不需要 WSL**。
#
#  用法（在 MSYS2 的 bash 里）：
#      bash build-all.sh
#
#  或直接用 Windows 入口（推荐）：
#      build-all.bat
#
#  可选环境变量：
#      MSYS2_ROOT=<MSYS2 根>        MSYS2 不在默认位置时
#      DVDA_SRC_TREE=<源码树>       源码树不在 <工具包>/src 时
#      DVDA_FONT_SRC=<ttc 路径>     Noto CJK 静态 ttc 的位置
#      JOBS=<并行数>                默认 nproc
#
#  各步骤可单独跑（调试时有用）：
#      bash check-src.sh         只体检
#      bash build-author.sh      只编 dvda-author
#      bash build-dvdauthor.sh   只编 dvdauthor/spumux
#      bash assemble-menu-bin.sh 只组装工具目录
#      bash make-release.sh      只打发布包
# ============================================================================
set -e
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
. "$HERE/common.sh"

T0=$(date +%s)

echo "============================================================"
echo "  DVD-Audio Maker —— Windows 原生构建"
echo "============================================================"
echo "  工具包   : $KIT"
echo "  MSYS2    : $MSYS_ROOT_WIN   (bash 侧: $MSYS)"
echo "  源码树   : $SRC"
echo "  工具输出 : $BINDIR"
echo "  并行数   : ${JOBS:-$(nproc)}"
echo "============================================================"

# ---- [1] 体检 ----
echo
echo "########## [1/5] 体检 ##########"
"$SELF_BASH" "$HERE/check-src.sh"

# ---- [2] dvda-author ----
echo
echo "########## [2/5] 构建 dvda-author（含 MLP）##########"
"$SELF_BASH" "$HERE/build-author.sh"

# ---- [3] dvdauthor / spumux ----
echo
echo "########## [3/5] 构建 dvdauthor / spumux ##########"
"$SELF_BASH" "$HERE/build-dvdauthor.sh"

# ---- [4] 组装工具目录 ----
echo
echo "########## [4/5] 组装工具目录 ##########"
"$SELF_BASH" "$HERE/assemble-menu-bin.sh"

# ---- [5] 打发布包 ----
echo
echo "########## [5/5] 打包发布 ##########"
"$SELF_BASH" "$HERE/make-release.sh"

T1=$(date +%s)
echo
echo "============================================================"
printf '  全部完成，用时 %d 分 %d 秒\n' $(((T1 - T0) / 60)) $(((T1 - T0) % 60))
echo "============================================================"
echo "  中间产物 : $BINDIR"
echo "  发布目录 : $KIT/release/DVD-Audio-Maker"
echo "  日志     : $KIT/logs/"
echo
echo "  下一步："
echo "    1) 把 release/DVD-Audio-Maker 拷到目标机器（或打包成 tar.gz）"
echo "    2) 目标机器只需 FFmpeg（在 PATH 里）；C# CLI 与 .NET 运行时已自带"
echo "    3) 改 config.sh 的 DVDA_SRC / DVDA_FINAL_DIR"
echo "    4) dvda.cmd prepare   然后   dvda.cmd build"
