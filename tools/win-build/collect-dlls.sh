#!/bin/bash
# 递归收集 Windows 工具链所需的全部 DLL 到目标目录。
#
# 搜索顺序（先找到先用）：
#   1) 目标目录本身
#   2) MSYS2 的 mingw64/bin  —— av*.dll / libiconv / libxml2 / fontconfig 等
#   3) MSYS2 的 usr/bin      —— msys-2.0.dll 等 MSYS 运行时
#   4) 上游 local.w10/bin     —— cygwin1.dll 等上游自带的（最后兜底）
#
# ⚠️ MSYS2 **必须排在 local.w10 之前**。local.w10 是 2019 年那套旧库，
#    而我们的 dvda-author/dvdauthor/spumux 是按 **MSYS2 的 FFmpeg 9 /
#    新版 iconv・glib・zlib** 编的。旧版同名 DLL 抢先时会报
#    `0xC0000139`（STATUS_ENTRYPOINT_NOT_FOUND）—— 实测踩到过：
#    9 个 DLL（libiconv-2 / libglib-2.0-0 / libgsm / liblzma-5 / libvorbis* /
#    zlib1 等）被抢成旧版，dvda-author-dev.exe 直接启动失败。
set -u
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
. "$HERE/common.sh"

BIN="${1:-$BINDIR}"
UPSTREAM="${2:-$SRC/local.w10/bin}"
SEARCH="$BIN $MSYS/bin $MSYS_ROOT/usr/bin $UPSTREAM"

is_system() {
    case "$(echo "$1" | tr 'A-Z' 'a-z')" in
        kernel32*|user32*|advapi32*|gdi32*|gdiplus*|ole32*|oleaut32*|shell32*|\
        urlmon*|usp10*|ws2_32*|msvcrt*|ntdll*|api-ms-*|ucrtbase*|dnsapi*|\
        shlwapi*|comdlg32*|winspool*|imm32*|version*|crypt32*|wintrust*|setupapi*|\
        mpr*|netapi32*|userenv*|psapi*|rpcrt4*|secur32*|iphlpapi*|dwmapi*|uxtheme*|\
        bcrypt*|ncrypt*|winmm*|wsock32*|dwrite*|shcore*|wldap32*|normaliz*|powrprof*|\
        d3d*|dxgi*|opengl32*|glu32*|winhttp*|avrt*|mfplat*|mf*|propsys*|\
        dui70*|duser*|twinapi*|windowscodecs*|wtsapi32*|kernelbase*|sspicli*|\
        win32u*|clusapi*|d2d1*|dcomp*|uiautomationcore*)
            return 0 ;;
    esac
    return 1
}

find_dll() {
    local name="$1" d
    for d in $SEARCH; do
        [ -f "$d/$name" ] && { echo "$d/$name"; return 0; }
    done
    return 1
}

echo "=== 解析依赖（目标: $BIN）==="
queue="$(ls "$BIN"/*.exe 2>/dev/null)"
seen=""
round=0
while [ -n "$queue" ]; do
    round=$((round + 1))
    [ "$round" -gt 12 ] && { echo "  [警告] 递归超过 12 轮，停止"; break; }
    next=""
    for f in $queue; do
        deps="$(objdump -p "$f" 2>/dev/null | grep -i "DLL Name" | awk '{print $3}')"
        for d in $deps; do
            is_system "$d" && continue
            case " $seen " in *" $d "*) continue ;; esac
            seen="$seen $d"
            if ! src="$(find_dll "$d")"; then
                echo "  [缺] $d  (被 $(basename "$f") 需要)"
                continue
            fi
            # 搜索路径第一项就是目标目录，所以已收过的会命中自己 → 跳过，
            # 否则 cp 会报「为同一文件」的噪声。
            [ "$src" = "$BIN/$d" ] && continue
            cp -f "$src" "$BIN/$d"
            echo "  + $d"
            next="$next $BIN/$d"
        done
    done
    queue="$next"
done

echo
echo "=== 结果 ==="
echo "  exe: $(ls -1 "$BIN"/*.exe 2>/dev/null | wc -l) 个"
echo "  dll: $(ls -1 "$BIN"/*.dll 2>/dev/null | wc -l) 个"
du -sh "$BIN" 2>/dev/null
