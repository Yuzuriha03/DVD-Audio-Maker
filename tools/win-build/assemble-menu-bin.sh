#!/bin/bash
# 组装「工具目录」——菜单运行时按名字从这里找辅助程序。
#
# 组成：
#   · dvda-author-dev.exe        自编（含 MLP + 按语言分派字体）
#   · dvdauthor / spumux / spuunmux  自编（含 AMGM 补丁，必须用我们的）
#   · jpeg2yuv / mpeg2enc / mplex / mp2enc / mkisofs
#                                ← 上游 local.w10/bin（MSYS2 无这些包）
#   · convert / mogrify / magick ← ImageMagick（MSYS2 有包，优先用 MSYS2 的）
#   · 11 个 ImageMagick 配置 .xml  —— **必须与 exe 同目录**
#   · fonts/                    三语单 face 字体（由 make-menu-font.sh 生成）
#   · 100+ DLL                  递归依赖（由 collect-dlls.sh 解析）
set -e
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
. "$HERE/common.sh"

DEST="${1:-$BINDIR}"
MSG=""

step "[1/5] 自编的 exe"
mkdir -p "$DEST"
if [ -f "$SRC/src/dvda-author-dev.exe" ]; then
    cp -f "$SRC/src/dvda-author-dev.exe" "$DEST/"
else
    echo "  [缺] dvda-author-dev.exe —— 先跑 build-author.sh"; exit 1
fi
for b in dvdauthor spumux spuunmux; do
    s="$SRC/dvdauthor-0.7.1/build-menu-win/src/$b.exe"
    if [ -f "$s" ]; then cp -f "$s" "$DEST/"; else
        echo "  [缺] $b.exe —— 先跑 build-dvdauthor.sh"; MSG="$MSG $b"
    fi
done

step "[2/5] 上游预编译的辅助程序"
# ⚠️ magick 是必需的：ImageMagick 7 把各工具合并成单一 `magick`，
#    **不再提供独立的 identify.exe**，而叠加图自检与图片尺寸校验都要用它。
for t in jpeg2yuv mpeg2enc mplex mp2enc mkisofs; do
    s="$SRC/local.w10/bin/$t.exe"
    if [ -f "$s" ]; then cp -f "$s" "$DEST/"; else
        echo "  [缺] $t.exe"; MSG="$MSG $t"
    fi
done

step "[2b/5] ImageMagick"
# MSYS2 有 imagemagick 包（7.1.2），优先用它 —— 与 dvda-author 链接的
# 运行库同源，省得混进 2019 年的旧 IM。
IM_SRC=""
for c in "$MSYS/bin" "$SRC/local.w10/bin"; do
    [ -f "$c/magick.exe" ] && { IM_SRC="$c"; break; }
done
if [ -n "$IM_SRC" ]; then
    echo "  来源: $IM_SRC"
    for t in magick convert mogrify; do
        [ -f "$IM_SRC/$t.exe" ] && cp -f "$IM_SRC/$t.exe" "$DEST/"
    done
    # IM 配置 .xml **必须与 exe 同目录**，否则会有
    # `UnableToOpenConfigureFile 'colors.xml' / 'type.xml'` 警告，
    # 且命名颜色与字体解析失效。
    n=0
    for x in "$IM_SRC"/*.xml; do
        [ -f "$x" ] && { cp -f "$x" "$DEST/"; n=$((n + 1)); }
    done
    echo "  复制 $n 个 xml"
else
    echo "  [缺] ImageMagick —— pacman -S mingw-w64-x86_64-imagemagick"
    MSG="$MSG imagemagick"
fi

step "[3/5] 字体（中/日/韩 三语单 face）"
"$SELF_BASH" "$HERE/make-menu-font.sh" "$DEST/fonts" || MSG="$MSG font"

step "[4/5] 收集 DLL（递归传递依赖）"
"$SELF_BASH" "$HERE/collect-dlls.sh" "$DEST" "$SRC/local.w10/bin"

step "[5/5] 汇总"
hr
printf '  exe:   %s\n' "$(ls -1 "$DEST"/*.exe 2>/dev/null | wc -l)"
printf '  xml:   %s\n' "$(ls -1 "$DEST"/*.xml 2>/dev/null | wc -l)"
printf '  dll:   %s\n' "$(ls -1 "$DEST"/*.dll 2>/dev/null | wc -l)"
printf '  fonts: %s\n' "$(ls -1 "$DEST"/fonts/* 2>/dev/null | wc -l)"
du -sh "$DEST"
if [ -n "$MSG" ]; then
    echo
    echo "  [注意] 以下组件缺失，菜单可能不可用:$MSG"
    echo "         出盘本身不受影响（可把 config.sh 的 DVDA_MENU 设为 off）"
fi
