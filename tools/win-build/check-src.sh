#!/bin/bash
# 检查源码树是否具备构建条件（在动手编译前给出可操作的提示）。
set -u
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
. "$HERE/common.sh"

echo "  工具包   : $KIT"
echo "  MSYS2    : $MSYS_ROOT_WIN   (bash 侧: $MSYS)"
echo "  源码树   : $SRC"
echo "  工具输出 : $BINDIR"
echo

miss=0
chk() {   # chk <相对路径> <说明>
    if [ -e "$SRC/$1" ]; then
        printf '  ok    %-42s %s\n' "$1" "$2"
    else
        printf '  缺!!  %-42s %s\n' "$1" "$2"
        miss=$((miss + 1))
    fi
}

echo "=== dvda-author 源码与构建文件 ==="
chk configure                      "autotools 入口（configure 会清空 local/）"
chk Makefile.in                    "顶层模板"
chk src/Makefile.in                "src 模板"
chk libutils/src/include/c_utils.h "Windows 兼容垫片"
chk libutils/src/winport.c         "Windows 管道实现"
chk src/mlp.c                      "MLP + FFmpeg 适配"
chk src/command_line_parsing.c     "命令行（含 --fontname-jp/-kr）"
chk src/menu.c                     "菜单（含按语言选 face）"
chk src/include/structures.h       "pic 结构（含 textfont_jp/_kr）"

echo
echo "=== 运行期素材（C 代码直接引用，必需）==="
chk menu/silence.wav               "菜单静音轨"
chk menu/activeheader              "菜单激活头"

echo
echo "=== dvdauthor 0.7.1（含 AMGM 补丁，菜单必需）==="
chk dvdauthor-0.7.1/configure                  "autotools 入口"
chk dvdauthor-0.7.1/src/dvdauthor.c            "含 AMGM 补丁"
chk dvdauthor-0.7.1/src/dvdcompile.c           "含 jump group 补丁"
chk m4.extra.dvdauthor/iconv.m4                "autotools 辅助 m4"

echo
echo "=== 预编译辅助二进制（MSYS2 没有这些包）==="
chk local.w10/bin/mkisofs.exe                  "ISO 打包"
chk local.w10/bin/jpeg2yuv.exe                 "JPEG -> YUV"
chk local.w10/bin/mpeg2enc.exe                 "MPEG-2 编码"
chk local.w10/bin/mplex.exe                    "复用"
chk local.w10/bin/mp2enc.exe                   "MP2 音频编码"

echo
echo "=== MSYS2 工具链 ==="
for t in gcc g++ make pkg-config windres objdump; do
    if command -v "$t" >/dev/null 2>&1; then
        printf '  ok    %-42s %s\n' "$t" "$("$t" --version 2>/dev/null | head -1)"
    else
        printf '  缺!!  %-42s %s\n' "$t" "pacman -S mingw-w64-x86_64-<pkg>"
        miss=$((miss + 1))
    fi
done

echo
echo "=== FFmpeg 开发库（dvda-author 链接它）==="
# ⚠️ 名称有两种写法，别混：
#      include 目录是  libavcodec/
#      库文件名是      libavcodec.dll.a   （= "lib" + 组件名 + ".dll.a"）
#    写 `"$MSYS/lib/lib$h.dll.a"` 而 $h 已经是 "libavcodec" 时会拼成
#    `liblibavcodec.dll.a` —— 永远找不到，于是报「FFmpeg 未安装」的**假阴性**。
for c in avcodec avformat avutil swresample; do
    if [ -d "$MSYS/include/lib$c" ] && [ -f "$MSYS/lib/lib$c.dll.a" ]; then
        printf '  ok    %-42s %s\n' "lib$c" "$(pkg-config --modversion "lib$c" 2>/dev/null)"
    else
        printf '  缺!!  %-42s %s\n' "lib$c" "pacman -S mingw-w64-x86_64-ffmpeg"
        miss=$((miss + 1))
    fi
done

echo
echo "=== 字体（三语单 face）==="
# ⚠️ 这里曾经是 `if FONT_TTC="$(ls ... | head -1)"` —— **假阳性**：
#    `head` 对空输入也返回 0，所以文件不存在时照样打印 "ok"。
#    字体是构建必需项，报假 ok 比不检查更糟。改成真正的 [ -f ] 判定。
if FONT_FACES_IN="$(font_faces_dir)"; then
    printf '  ok    %-42s %s\n' "三语单 face" "$FONT_FACES_IN"
elif FONT_TTC_IN="$(find_font_ttc)"; then
    printf '  ok    %-42s %s\n' "NotoSansCJK-Regular.ttc" "$FONT_TTC_IN"
    printf '         %-42s %s\n' "(将由 ttc 抽出三个单 face)"
else
    printf '  缺!!  %-42s %s\n' "CJK 字体" "既没有抽好的单 face，也没有静态 ttc"
    printf '         %-42s %s\n' "" "见 tools/win-build/README.md 的「字体」一节"
    miss=$((miss + 1))
fi
if PY="$(find_py_fonttools)"; then
    printf '  ok    %-42s %s\n' "python + fontTools" "$PY"
else
    # 只有在**需要从 ttc 抽 face** 时才是必需项
    if font_faces_dir >/dev/null 2>&1; then
        printf '  提示  %-42s %s\n' "python + fontTools" \
            "未见；已有单 face，本步不需要（仅刷新字体时才要）"
    else
        printf '  缺!!  %-42s %s\n' "python + fontTools" \
            "pacman -S mingw-w64-x86_64-python-fonttools"
        miss=$((miss + 1))
    fi
fi

echo
hr
if [ "$miss" -eq 0 ]; then
    log "  全部就绪 —— 可以运行 build-all.sh"
    exit 0
else
    log "  有 $miss 项不满足，见上面的「缺!!」行"
    exit 1
fi
