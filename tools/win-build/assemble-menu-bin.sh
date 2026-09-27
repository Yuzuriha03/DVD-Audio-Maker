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
# ⚠️ **必须优先用 `local.w10` 的那份 IM（7.0.8-47 Q16, 2019）**，
#    **不要**用 MSYS2 的（7.1.2-31 Q16-HDRI）。实测踩到：
#
#      · local.w10 的是 **非模块化** 构建：单个 6.8 MB exe，
#        JPEG/PNG 解码器**编在 exe 内部**，只依赖 Windows 系统 DLL
#        （ADVAPI32/GDI32/gdiplus/KERNEL32/ole32/OLEAUT32/SHELL32/USER32/…）。
#        实测：拷进一个空目录、**不给任何 .xml、不给 modules**，`identify`
#        一张 JPEG 照样成功（exit 0）。
#
#      · MSYS2 的是 `--with-modules` 构建：JPEG 解码器是独立的
#        `lib/ImageMagick-7.1.2/modules-Q16HDRI/coders/jpeg.dll`，
#        而且内建的 CONFIGURE_PATH 是 `/mingw64/etc/ImageMagick-7/`
#        —— 一个 **MSYS2 绝对路径**。放到发布包里就两个都找不到：
#
#            magick.EXE: UnableToOpenConfigureFile `delegates.xml' @ warning/configure.c
#            magick.EXE: NoDecodeDelegateForThisImageFormat `...cover.jpg' @ error/constitute.c
#
#        这两条错误出现在「拼背景图」那一步，看起来像**图片路径写错了**
#        （报的是 cover.jpg 打不开），实际是 IM 自己缺配置和模块 ——
#        很容易误判到 config.sh 上去。
#
#    ★ 曾经的注释写「优先用 MSYS2 的 —— 与 dvda-author 链接的运行库同源」，
#      这个理由是**错的**：dvda-author **不链接** ImageMagick，它是把
#      `mogrify` 当**子进程**执行的。既然没有链接关系，就没有同源的必要，
#      而自包含才是发布包唯一重要的性质。
#    ★ 用 local.w10 那份还有个好处：**已验证的基准就是它产出的**
#      （AOB 与 E 盘基准逐字节相同），换成 7.1.2 反而会引入静图差异。
IM_SRC=""
for c in "$SRC/local.w10/bin" "$MSYS/bin"; do
    [ -f "$c/magick.exe" ] && { IM_SRC="$c"; break; }
done
if [ -n "$IM_SRC" ]; then
    echo "  来源: $IM_SRC"
    for t in magick convert mogrify; do
        [ -f "$IM_SRC/$t.exe" ] && cp -f "$IM_SRC/$t.exe" "$DEST/"
    done
    # IM 配置 .xml 与 exe 同目录（非模块化构建其实不强依赖，但带上无害；
    # 模块化构建则必须有）。
    n=0
    for x in "$IM_SRC"/*.xml; do
        [ -f "$x" ] && { cp -f "$x" "$DEST/"; n=$((n + 1)); }
    done
    echo "  复制 $n 个 xml"

    # 自检：真的能解一张 JPEG 吗？
    # ⚠️ 必须有这道检查 —— 上面那两条错误属于「报错信息指向错误的方向」，
    #    只在构建走到菜单那一步才炸；这里提前用**真图**验一遍，早失败早发现。
    # 探针用**项目自带素材**（$SRC/menu/*.jpg），不依赖用户音源目录 ——
    # 那两行黑场图是菜单的必需素材，一定存在。
    probe=""
    for j in "$SRC"/menu/*.jpg "$SRC"/local.w10/menu/*.jpg; do
        [ -f "$j" ] && { probe="$j"; break; }
    done
    if [ -n "$probe" ]; then
        if "$DEST/magick.exe" identify "$probe" >/dev/null 2>&1; then
            echo "  自检: 能解 JPEG ✔  ($(basename "$probe"))"
        else
            echo "  [失败] $DEST/magick.exe 解不了 JPEG —— 发布包会缺图片功能"
            "$DEST/magick.exe" identify "$probe" 2>&1 | head -3 | sed 's/^/         /'
            MSG="$MSG imagemagick-broken"
        fi
    else
        echo "  自检: 跳过（$SRC/menu/ 下找不到 jpg 探针）"
    fi
    # 模块化构建的痕迹 —— 提醒发布包里还缺 modules
    if [ -d "$IM_SRC/../lib/ImageMagick-7.1.2/modules-Q16HDRI" ]; then
        echo "  [警告] 这份 IM 是模块化构建，发布包需要额外带 coder 模块"
    fi
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
