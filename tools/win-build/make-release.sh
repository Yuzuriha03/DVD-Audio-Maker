#!/bin/bash
# 组装 Windows 可分发目录：C# CLI + dvda-author 工具链 + 字体/菜单素材。
# 目标机器不需要 Python、MSYS2、WSL 或 .NET Runtime；FFmpeg 仍需在 PATH
# 中，或在 config.sh 里指定完整路径。
set -euo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
. "$HERE/common.sh"

OUT_BASE="${1:-$KIT/release}"
DEST="$OUT_BASE/DVD-Audio-Maker"
REPO="$(cd "$KIT/../.." && pwd)"
CLI_PROJECT="$REPO/src/DvdaMaker.Cli/DvdaMaker.Cli.csproj"
PUBLISH_DIR="$KIT/publish/win-x64"

step "[0/7] 检查前置"
[ -f "$BINDIR/dvda-author-dev.exe" ] || {
    echo "  [缺] $BINDIR/dvda-author-dev.exe —— 先跑 assemble-menu-bin.sh" >&2
    exit 1
}
[ -f "$SRC/menu/silence.wav" ] || {
    echo "  [缺] $SRC/menu/silence.wav" >&2
    exit 1
}
[ -f "$CLI_PROJECT" ] || {
    echo "  [缺] C# CLI 项目: $CLI_PROJECT" >&2
    exit 1
}
command -v dotnet >/dev/null 2>&1 || {
    echo "  [缺] dotnet SDK（需要 .NET 10 SDK 构建 Windows 发布包）" >&2
    exit 1
}
echo "  仓库根   : $REPO"
echo "  源码树   : $SRC"
echo "  工具目录 : $BINDIR"
echo "  CLI 项目 : $CLI_PROJECT"

step "[1/7] 发布 C# CLI（win-x64 self-contained）"
rm -rf "$PUBLISH_DIR"
mkdir -p "$PUBLISH_DIR"
dotnet publish "$CLI_PROJECT" \
    --configuration Release \
    --runtime win-x64 \
    --self-contained true \
    --output "$PUBLISH_DIR" \
    -p:PublishSingleFile=true \
    -p:IncludeNativeLibrariesForSelfExtract=true \
    -p:DebugType=None \
    -p:DebugSymbols=false
[ -f "$PUBLISH_DIR/dvda.exe" ] || {
    echo "  [失败] dotnet publish 未生成 dvda.exe" >&2
    exit 1
}
printf '  dvda.exe: %s\n' "$(du -h "$PUBLISH_DIR/dvda.exe" | cut -f1)"

step "[2/7] 清空目标并复制 CLI"
rm -rf "$DEST"
mkdir -p "$DEST/app"
cp -a "$PUBLISH_DIR/." "$DEST/app/"

step "[3/7] 工具链 menu-bin"
cp -a "$BINDIR" "$DEST/menu-bin"
printf '  exe=%s dll=%s xml=%s fonts=%s\n' \
  "$(find "$DEST/menu-bin" -maxdepth 1 -type f -iname '*.exe' | wc -l)" \
  "$(find "$DEST/menu-bin" -maxdepth 1 -type f -iname '*.dll' | wc -l)" \
  "$(find "$DEST/menu-bin" -maxdepth 1 -type f -iname '*.xml' | wc -l)" \
  "$(find "$DEST/menu-bin/fonts" -maxdepth 1 -type f 2>/dev/null | wc -l)"

step "[4/7] dvda-author 运行期素材"
mkdir -p "$DEST/data"
cp -a "$SRC/menu" "$DEST/data/menu"

step "[5/7] 启动器与配置"
# 保持纯 ASCII，避免 cmd.exe 按 OEM 代码页解析 UTF-8 时损坏命令。
cat > "$DEST/dvda.cmd" <<'CMDEOF'
@echo off
setlocal
set "ROOT=%~dp0"
set "DVDA_AUTHOR=%ROOT%menu-bin\dvda-author-dev.exe"
set "DVDA_MKISOFS=%ROOT%menu-bin\mkisofs.exe"
set "DVDA_AUTHOR_SRC=%ROOT%data"
set "DVDA_MENU_FONT=%ROOT%menu-bin\fonts\NotoSansCJKsc-Regular.otf"
set "DVDA_MENU_FONT_JP=%ROOT%menu-bin\fonts\NotoSansCJKjp-Regular.otf"
set "DVDA_MENU_FONT_KR=%ROOT%menu-bin\fonts\NotoSansCJKkr-Regular.otf"

if not exist "%ROOT%app\dvda.exe" (
  echo [ERROR] not found: %ROOT%app\dvda.exe
  exit /b 2
)
if not exist "%DVDA_AUTHOR%" (
  echo [ERROR] not found: %DVDA_AUTHOR%
  exit /b 2
)

if "%~1"=="" (
  echo Usage: dvda.cmd ^<command^> [args]
  echo Commands: config prepare plan build convert verify quick-check audit aob-pts mlp alac iso
  exit /b 2
)

cd /d "%ROOT%"
"%ROOT%app\dvda.exe" %*
exit /b %ERRORLEVEL%
CMDEOF

if LC_ALL=C grep -q '[^ -~]' "$DEST/dvda.cmd"; then
    echo "  [失败] dvda.cmd 含非 ASCII 字符" >&2
    exit 1
fi

cat > "$DEST/config.sh" <<'CONFEOF'
# DVD-Audio Maker configuration
# Environment variables set by dvda.cmd override tool paths below.

DVDA_SRC="D:/Music/MyAlbums"
DVDA_FINAL_DIR="D:/DVD_Output"
DVDA_BUILD_DIR="D:/DVD_Output/_work"

DVDA_TITLE="My DVD-Audio Collection"
DVDA_ISO_PREFIX="MyCollection"
DVDA_WINDOWS_DEST=""
DVDA_MAX_DISCS="2"

DVDA_MLP_SOURCE="ffmpeg"
DVDA_MLP_EXTERNAL_DIR=""
DVDA_FFMPEG="ffmpeg"
DVDA_FFPROBE="ffprobe"
DVDA_METAFLAC="metaflac"

DVDA_MENU="on"
DVDA_MENU_STILLPICS="on"
DVDA_MENU_TRACKS_PER_PAGE="12"
DVDA_MENU_INDEX_MIN_ALBUMS="4"
DVDA_MENU_COVER_DIM="35"
DVDA_MENU_FONT=""
DVDA_MENU_FONT_JP=""
DVDA_MENU_FONT_KR=""

DVDA_GROUP_TRACK_LIMIT="99"
DVDA_LOSS_ERROR_S="0.05"
DVDA_LOSS_WARN_S="0.02"
CONFEOF

step "[6/7] 文档与许可"
doc_miss=0
for d in README.md THIRD-PARTY.md LICENSE; do
    got=""
    for c in "$HERE/docs/$d" "$REPO/docs/$d" "$REPO/$d" "$SRC/../docs/$d"; do
        if [ -f "$c" ]; then
            cp -f "$c" "$DEST/"
            got="$c"
            break
        fi
    done
    if [ -n "$got" ]; then
        echo "  $d <- $got"
    else
        echo "  [缺] $d"
        doc_miss=$((doc_miss + 1))
    fi
done
[ "$doc_miss" -eq 0 ] || {
    echo "  [失败] 发布包缺 $doc_miss 份必要文档" >&2
    exit 1
}

"$SELF_BASH" "$HERE/make-release-manifest.sh" "$DEST"

step "[7/7] 完成"
du -sh "$DEST"
if [ "${DVDA_TARBALL:-1}" = "1" ]; then
    TG_NAME="$(basename "$DEST").tar.gz"
    TG_DIR="$(dirname "$DEST")"
    rm -f "$TG_DIR/$TG_NAME"
    (cd "$TG_DIR" && tar -czf "$TG_NAME" "$(basename "$DEST")")
    echo "  tar.gz: $TG_DIR/$TG_NAME ($(du -h "$TG_DIR/$TG_NAME" | cut -f1))"
fi
