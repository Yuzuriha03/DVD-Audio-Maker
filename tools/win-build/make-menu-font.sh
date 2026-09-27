#!/bin/bash
# 从 Noto Sans CJK 的 **静态 .ttc** 抽出 SC / JP / KR 三个单 face 字体。
#
# ⚠️ 为什么必须抽（两个独立的坑，都实测过）：
#
#   1) **`.ttc` + 文件路径 ⇒ ImageMagick 只取 face 0 = JP**
#      Noto Sans CJK 的 .ttc 里有 10 个 face（[0]=JP [1]=KR [2]=SC …），
#      按文件路径加载时只认第 0 个。于是中文菜单显示成**日文字形**
#      （直/骨/令/次/别 写法不同），而且**完全不报错**。
#      IM 也不支持 `.ttc[2]` / `:index=2`；Windows 的 type.xml 也认不出
#      `Noto-Sans-CJK-SC` 这类家族名。所以只能给单 face 文件。
#
#   2) **三个 face 都要**，因为同一批**汉字**有区域性变体字形。
#      实测真实曲名、逐像素比对：
#          日文曲名  SC vs JP   最多 1464 像素不同
#          中文曲名  SC vs JP   最多 2624 像素不同
#          韩文曲名  SC vs JP/KR  完全相同（谚文没有区域性变体）
#      即中文用 JP、日文用 SC 都是字形错。菜单按每条文字所属语言自动选
#      face（C 侧 menu.c 的 textfont_for()，配 --fontname-jp/-kr）。
#
# 前置：MSYS2 的 python + fontTools
#       pacman -S mingw-w64-x86_64-python-fonttools
set -e
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
. "$HERE/common.sh"

DEST="${1:-$BINDIR/fonts}"
mkdir -p "$DEST"

# ---- 已经抽好了？直接复用 ----
#
# ★ 单 face OTF 才是构建**真正需要**的东西，ttc 只是生成它的手段。
#   实测踩到：用户的 ttc 是临时拷来的，后来没了；如果这一步只认 ttc，
#   就会硬报「找不到字体」—— 而抽好的三个 face 其实好好躺在
#   menu-bin/fonts 里（那是构建产物）。白白让人以为要重新弄 ttc。
#
#   想强制重抽：FONT_REEXTRACT=1 bash make-menu-font.sh
if [ "${FONT_REEXTRACT:-0}" != "1" ] && FACE_SRC="$(font_faces_dir)"; then
    same=0
    case "$FACE_SRC" in
        "$DEST"|"$DEST"/*) same=1 ;;
    esac
    if [ "$same" = "0" ]; then
        for f in $FONT_FACES; do
            cp -f "$FACE_SRC/$f" "$DEST/$f" 2>/dev/null || true
        done
    fi
    ok=1
    for f in $FONT_FACES; do [ -f "$DEST/$f" ] || ok=0; done
    if [ "$ok" = "1" ]; then
        echo "  复用已有单 face: $FACE_SRC"
        echo "  产物: $DEST/NotoSansCJK{sc,jp,kr}-Regular.otf  ($(du -sh "$DEST" | cut -f1))"
        exit 0
    fi
    echo "  [警告] $FACE_SRC 里的 face 不全，回退到从 ttc 重抽"
fi

# ---- 找源 ttc ----
# ⚠️ 必须是 **Noto Sans CJK 的静态版**（NotoSansCJK-Regular.ttc）。
#    不要用 Windows 自带的 NotoSansSC-VF.ttf / NotoSansJP-VF.ttf ——
#    那些是**单语**字体（SC 版没有谚文），会给别的语言开出空白。
SRC_TTC="${DVDA_FONT_SRC:-}"
if [ -z "$SRC_TTC" ]; then
    for c in "$SRC/NotoSansCJK-Regular.ttc" \
             "$SRC/fonts/NotoSansCJK-Regular.ttc" \
             "$KIT/NotoSansCJK-Regular.ttc" \
             /c/Windows/Fonts/NotoSansCJK-Regular.ttc \
             /usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc; do
        [ -f "$c" ] && { SRC_TTC="$c"; break; }
    done
fi

if [ -z "$SRC_TTC" ] || [ ! -f "$SRC_TTC" ]; then
    cat >&2 <<'EOM'
[失败] 找不到 NotoSansCJK-Regular.ttc（Noto Sans CJK 的静态版）
       ⚠️ 不要用 NotoSansSC-VF / NotoSansJP-VF —— 它们是单语字体，
          没有谚文，会给韩文标题开出空白。

       获取方式（任选）：
         · 指定已有文件:  DVDA_FONT_SRC=/path/to/NotoSansCJK-Regular.ttc \
                              bash make-menu-font.sh
         · 放到源码树:    <源码树>/NotoSansCJK-Regular.ttc
         · 放到工具包:    <本工具包>/NotoSansCJK-Regular.ttc
         · 或装到系统:    C:\Windows\Fonts\NotoSansCJK-Regular.ttc
EOM
    exit 1
fi

PY="$(find_py_fonttools)" || {
    echo "[失败] 找不到带 fontTools 的 python"
    echo "       pacman -S mingw-w64-x86_64-python-fonttools"
    exit 1
}

cat > "$KIT/extract_face.py" <<'PYEOF'
# -*- coding: utf-8 -*-
"""按 **family 名**从 .ttc 里抽一个 face 出来，存成单 face 字体。

按名字而不按索引：face 顺序随 Noto 版本变，`Noto Sans CJK SC` 这个名字不变。
"""
from fontTools.ttLib import TTCollection
import os
import sys

src, dst, want = sys.argv[1], sys.argv[2], sys.argv[3]
ttc = TTCollection(src, lazy=False)
for i, f in enumerate(ttc.fonts):
    fam = f["name"].getDebugName(1) or ""
    if fam == want:
        f.save(dst)
        ps = f["name"].getDebugName(6) or ""
        print(f"face[{i}] '{fam}' ps='{ps}' -> {os.path.basename(dst)}"
              f" ({os.path.getsize(dst)} bytes)")
        raise SystemExit(0)
fams = [f["name"].getDebugName(1) for f in ttc.fonts]
raise SystemExit(f"[FAIL] family='{want}' not found; available: {fams}")
PYEOF

cat > "$KIT/verify_face.py" <<'PYEOF'
# -*- coding: utf-8 -*-
"""校验：确实是单 face、family 正确、四个字符集编码齐备。"""
from fontTools.ttLib import TTFont
import sys

p, want = sys.argv[1], sys.argv[2]
try:
    f = TTFont(p, lazy=True)
except Exception as e:
    raise SystemExit(f"  [X] 不是单 face 字体（.ttc？）：{e}")

fam = f["name"].getDebugName(1) or ""
ps = f["name"].getDebugName(6) or ""
if fam != want:
    raise SystemExit(f"  [X] family 应为 '{want}'，实为 '{fam}'")

# 编码表必须同时有汉字、假名、谚文 —— 缺谚文是最常见的坑
# （Windows 自带的 NotoSansSC-VF.ttf 就是单语字体）
cmap = f.getBestCmap()
need = {"汉字": 0x6C49, "假名": 0x3042, "谚文": 0xAC00, "拉丁": 0x0041}
miss = [k for k, cp in need.items() if cp not in cmap]
flags = " ".join(f"{k}={'Y' if cp in cmap else 'N'}" for k, cp in need.items())
print(f"family='{fam}' ps='{ps}'  {flags}")
if miss:
    raise SystemExit(f"  [X] 缺字符集: {miss}")
PYEOF

step "抽 SC / JP / KR face"
echo "  源   : $SRC_TTC"
echo "  目标 : $DEST"
for f in sc jp kr; do
    case $f in
        sc) fam="Noto Sans CJK SC" ;;
        jp) fam="Noto Sans CJK JP" ;;
        kr) fam="Noto Sans CJK KR" ;;
    esac
    out="$DEST/NotoSansCJK$f-Regular.otf"
    printf '  [%s] ' "$f"
    "$PY" "$KIT/extract_face.py" "$SRC_TTC" "$out" "$fam" || exit 1
    [ -s "$out" ] || { echo "    [X] 产出为空"; exit 1; }
done

# 旧版直接拷的 .ttc（如果有残留）删掉：留着会被误用
rm -f "$DEST/NotoSansCJK-Regular.ttc"

step "校验"
for f in sc jp kr; do
    case $f in
        sc) fam="Noto Sans CJK SC" ;;
        jp) fam="Noto Sans CJK JP" ;;
        kr) fam="Noto Sans CJK KR" ;;
    esac
    printf '  [%s] ' "$f"
    "$PY" "$KIT/verify_face.py" "$DEST/NotoSansCJK$f-Regular.otf" "$fam" || exit 1
done

echo
echo "  产物: $DEST/NotoSansCJK{sc,jp,kr}-Regular.otf  ($(du -sh "$DEST" | cut -f1))"
echo "  ⚠️ 不要把 .ttc 当作 DVDA_MENU_FONT —— 按路径加载只会取 face 0（=JP），"
echo "     中文会变日文字形且不报错。"
