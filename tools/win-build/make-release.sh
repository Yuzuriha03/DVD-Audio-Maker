#!/bin/bash
# 组装**可分发**的发布目录（自带 exe / DLL / 字体 / 配置），
# 拷到任何 Windows 机器都能跑 —— 目标机器不需要 MSYS2、不需要 WSL。
#
# 产出：<输出目录>/DVD-Audio-Maker/
#         dvda.cmd          启动器（设工具路径 + UTF-8 控制台，再调 python）
#         README.md / THIRD-PARTY.md / LICENSE / MANIFEST.txt
#         scripts/          python 脚本 + 发行版 config.sh
#         menu-bin/         工具链（exe + DLL + IM 配置 + fonts/）
#         data/menu/        dvda-author 的运行期素材
set -e
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
. "$HERE/common.sh"

OUT_BASE="${1:-$KIT/release}"
DEST="$OUT_BASE/DVD-Audio-Maker"

# python 脚本来源。按顺序找第一个真的装着 01_prepare.py 的目录：
#   1) $DVDA_SCRIPTS
#   2) <工具包>/../..           —— 工具包在仓库里 tools/win-build/ 时的仓库根
#   3) <工具包>/../scripts      —— 工具包与脚本并列时的常见布局
#   4) <工具包>/scripts
find_scripts() {
    local c
    for c in "${DVDA_SCRIPTS:-}" \
             "$KIT/../.." \
             "$KIT/../scripts" \
             "$KIT/scripts"; do
        [ -n "$c" ] || continue
        [ -f "$c/01_prepare.py" ] && { (cd "$c" && pwd); return 0; }
    done
    return 1
}
if ! SCRIPTS="$(find_scripts)"; then
    cat >&2 <<'EOM'
[失败] 找不到 python 脚本目录（需要一个含 01_prepare.py 的目录）。
       处理：设 DVDA_SCRIPTS=<仓库根>，或把本工具包放在仓库的
             tools/win-build/ 下（那样 <工具包>/../.. 就是仓库根）。
EOM
    exit 1
fi

step "[0/6] 检查前置"
[ -f "$BINDIR/dvda-author-dev.exe" ] || { echo "  [缺] menu-bin/ —— 先跑 assemble-menu-bin.sh"; exit 1; }
[ -f "$SRC/menu/silence.wav" ]       || { echo "  [缺] $SRC/menu/"; exit 1; }
[ -f "$HERE/da-utf8.manifest" ]      || { echo "  [缺] da-utf8.manifest"; exit 1; }
[ -d "$SCRIPTS" ]                    || { echo "  [缺] python 脚本目录: $SCRIPTS"; exit 1; }
echo "  源码树   : $SRC"
echo "  工具目录 : $BINDIR"
echo "  脚本来源 : $SCRIPTS"

step "[1/6] 清空目标"
rm -rf "$DEST"
mkdir -p "$DEST"

step "[2/6] 工具链 menu-bin"
cp -a "$BINDIR" "$DEST/menu-bin"
printf '  exe=%s dll=%s xml=%s fonts=%s\n' \
  "$(ls -1 "$DEST/menu-bin"/*.exe 2>/dev/null | wc -l)" \
  "$(ls -1 "$DEST/menu-bin"/*.dll 2>/dev/null | wc -l)" \
  "$(ls -1 "$DEST/menu-bin"/*.xml 2>/dev/null | wc -l)" \
  "$(ls -1 "$DEST/menu-bin/fonts/"* 2>/dev/null | wc -l)"

step "[3/6] dvda-author 运行期素材 data/menu"
# 只有 silence.wav（静音轨）与 activeheader 被 C 代码真用到，
# 但整目录才 236 KB，一并带上以免漏掉别的引用。
mkdir -p "$DEST/data"
cp -a "$SRC/menu" "$DEST/data/menu"
printf '  %s 个文件\n' "$(ls -1 "$DEST/data/menu" | wc -l)"

step "[4/6] python 脚本"
mkdir -p "$DEST/scripts"
cp "$SCRIPTS"/*.py "$DEST/scripts/"
printf '  %s 个脚本\n' "$(ls -1 "$DEST/scripts"/*.py | wc -l)"

step "[5/6] 启动器与发行版 config.sh"
# ⚠️ .cmd 必须**纯 ASCII** —— cmd.exe 按 OEM 代码页读，UTF-8 中文会解析失败。
cat > "$DEST/dvda.cmd" <<'CMDEOF'
@echo off
REM ===========================================================================
REM  dvda.cmd - launcher for the Windows DVD-Audio Maker
REM
REM  Usage:  dvda.cmd 01_prepare.py
REM          dvda.cmd 02_build.py
REM          dvda.cmd 02_build.py --dry-run
REM
REM  Why a launcher: the tool paths (dvda-author, mkisofs, menu data dir,
REM  bundled CJK fonts) live NEXT TO THIS FILE, and their location changes if
REM  the folder is moved.  dvda_config.py gives environment variables the
REM  highest priority (env > config.sh > built-in defaults), so setting them
REM  here keeps config.sh free of machine-specific paths.
REM
REM  NOTE: keep this file pure ASCII - cmd.exe reads .cmd/.bat in the OEM
REM  code page, and UTF-8 text can break parsing.
REM ===========================================================================
setlocal EnableDelayedExpansion

set "ROOT=%~dp0"

REM -- paths relative to this file ------------------------------------------
set "DVDA_AUTHOR=%ROOT%menu-bin/dvda-author-dev.exe"
set "DVDA_MKISOFS=%ROOT%menu-bin/mkisofs.exe"
set "DVDA_AUTHOR_SRC=%ROOT%data"
set "DVDA_MENU_FONT=%ROOT%menu-bin/fonts/NotoSansCJKsc-Regular.otf"

REM -- UTF-8 console: track titles contain Chinese/Japanese/Korean -----------
set "PYTHONUTF8=1"
set "PYTHONIOENCODING=utf-8"

REM -- sanity check ---------------------------------------------------------
if not exist "%DVDA_AUTHOR%" (
  echo [ERROR] not found: %DVDA_AUTHOR%
  echo         extract the whole archive, keep the folder layout intact.
  exit /b 2
)
if not exist "%DVDA_MENU_FONT%" (
  echo [ERROR] not found: %DVDA_MENU_FONT%
  exit /b 2
)

REM -- python --------------------------------------------------------------
set "PY=%DVDA_PYTHON%"
if "%PY%"=="" set "PY=python"
where %PY% >nul 2>&1
if errorlevel 1 (
  where py >nul 2>&1
  if errorlevel 1 (
    echo [ERROR] Python not found in PATH.
    echo         install Python 3.8+ from https://www.python.org/downloads/
    echo         or set DVDA_PYTHON to the full path of python.exe
    exit /b 2
  )
  set "PY=py -3"
)

if "%~1"=="" (
  echo Usage: dvda.cmd ^<script^> [args]
  echo   e.g. dvda.cmd 01_prepare.py
  echo        dvda.cmd 02_build.py
  exit /b 2
)

cd /d "%ROOT%scripts"
%PY% -u %*
CMDEOF

if LC_ALL=C grep -q '[^ -~]' "$DEST/dvda.cmd"; then
    echo "  [警告] dvda.cmd 含非 ASCII 字符 —— cmd.exe 可能解析失败"
else
    echo "  dvda.cmd 纯 ASCII ✔"
fi

# ---- 发行版 config.sh（只留用户要改的项；工具路径由 dvda.cmd 注入） ----
cat > "$DEST/scripts/config.sh" <<'CONFEOF'
# ============================================================================
#  config.sh —— 你唯一需要改的文件
#
#  ⚠️ 请用 `dvda.cmd` 运行（它会设好工具链路径与 UTF-8 控制台）：
#
#        dvda.cmd 01_prepare.py        第一步：扫描音源、生成 manifest
#        dvda.cmd 02_build.py          第二步：编码 MLP、出 ISO
#
#  ⚠️ 安装路径**不要含空格** —— dvda-author 拼 ImageMagick 命令时不给路径
#     加引号，带空格的目录会被截断（字体、素材路径都会失效）。
#
#  ⚠️ 工具链路径不在这里配置：dvda.cmd 会按本目录布局自动设置
#     （DVDA_AUTHOR / DVDA_MKISOFS / DVDA_AUTHOR_SRC / DVDA_MENU_FONT）。
#     环境变量优先级最高，所以就算这里写了也会被覆盖。
# ============================================================================

# ---- 音源 ----------------------------------------------------------------
# FLAC 音源目录。**递归**扫描，子目录即为专辑。
DVDA_SRC="D:/Music/MyAlbums"

# ---- 输出与工作目录 ------------------------------------------------------
DVDA_FINAL_DIR="D:/DVD_Output"            # 成品 ISO 放这里
DVDA_BUILD_DIR="D:/DVD_Output/_work"      # 中间产物（约需 20 GB 临时空间）

# ---- 光盘信息 ------------------------------------------------------------
DVDA_TITLE="My DVD-Audio Collection"
DVDA_ISO_PREFIX="MyCollection"
DVDA_WINDOWS_DEST=""                      # 可选：出盘后再拷贝一份到该目录
DVDA_MAX_DISCS="2"                        # 盘数上限（只做检查与提示，不参与切分）

# ---- MLP 来源 ------------------------------------------------------------
#   "ffmpeg"   = 本工具链自己编码（需要 ffmpeg 在 PATH 或下面给出全路径）
#   "external" = 用外部编码器产出的 MLP（如 SurCode），不自己编
#               外部目录结构需与音源同构：<外部目录>/<专辑>/<曲名>.mlp
DVDA_MLP_SOURCE="ffmpeg"
DVDA_MLP_EXTERNAL_DIR=""

# ---- FFmpeg（MLP 编解码、格式探测用）------------------------------------
# 留空则用 PATH 里的 ffmpeg / ffprobe。
DVDA_FFMPEG=""
DVDA_FFPROBE=""

# ---- 菜单 ----------------------------------------------------------------
DVDA_MENU="on"
DVDA_MENU_STILLPICS="on"
DVDA_MENU_TRACKS_PER_PAGE="12"
DVDA_MENU_INDEX_MIN_ALBUMS="4"
DVDA_MENU_COVER_DIM="35"
#
# 字体：由 dvda.cmd 指向自带的 NotoSansCJKsc-Regular.otf（单 face SC）。
# ⚠️ 不要改成 Windows 系统字体：系统里没有哪个能单独覆盖中日韩四语
#    （NotoSansSC 无谚文、malgun 无汉字），而本盘四种语言标题都有。
# ⚠️ 也不要用 .ttc 集合：ImageMagick 按路径加载只取 face 0（=JP），
#    中文会变日文字形。
#
# 下面两项是**按语言分派**的 face（日文标题 -> JP、韩文 -> KR）。
# 留空时自动按主字体的同族命名推导（包内已并排提供三个单 face）。
# ⚠️ 为什么需要：各 face 都含四个字符集，但**汉字**有区域性变体字形
#    （直/骨/令/次/别…）。实测真实曲名逐像素比对：日文曲名 SC vs JP
#    最多差 1464 像素，中文曲名 SC vs JP 最多差 2624 像素。
#    即中文用 SC、日文用 JP 才对；韩文两者皆可（谚文无区域性差异）。
DVDA_MENU_FONT=""
DVDA_MENU_FONT_JP=""
DVDA_MENU_FONT_KR=""

# ---- 分盘与无损校验阈值 --------------------------------------------------
DVDA_GROUP_TRACK_LIMIT="99"               # 每个 title 的轨数上限
DVDA_LOSS_ERROR_S="0.05"                  # 时长偏差超过它 → 判失败（秒）
DVDA_LOSS_WARN_S="0.02"                   # 超过它 → 只警告
CONFEOF

step "[6/6] 文档与许可"
# ⚠️ 必须把 `$SCRIPTS/docs/` 也列进来。
#    实测踩到：仓库布局是 <仓库>/docs/{README.md,THIRD-PARTY.md,LICENSE}，
#    只找 `$HERE/docs/`（工具包自己的）和 `$SCRIPTS/`（仓库根）都找不到，
#    于是发布包里**静默地没有 README / LICENSE** —— 而这一步还报「完成」。
#    现在每个文件都回显是否拷到，缺了就列出来。
doc_miss=0
for d in README.md THIRD-PARTY.md LICENSE; do
    got=""
    for c in "$HERE/docs/$d" "$SCRIPTS/docs/$d" "$SCRIPTS/$d" "$SRC/../docs/$d"; do
        [ -f "$c" ] && { cp -f "$c" "$DEST/"; got="$c"; break; }
    done
    if [ -n "$got" ]; then
        echo "  $d  <- $got"
    else
        echo "  [缺] $d  （四个候选位置都没有）"
        doc_miss=$((doc_miss + 1))
    fi
done
[ "$doc_miss" -gt 0 ] && echo "  【注意】缺 $doc_miss 份文档，发布包不完整"

"$SELF_BASH" "$HERE/make-release-manifest.sh" "$DEST"

step "完成"
du -sh "$DEST"

# ---- 打包成 tar.gz ----
# ⚠️ 这里原来是一句 `echo "  打包: cd ... && tar -czf ..."` ——
#    只**打印**了命令，从来没有真的执行。所以发布目录里一直没有
#    .tar.gz，而日志里那一行还让人以为已经打好了。
#    想跳过：DVDA_TARBALL=0
if [ "${DVDA_TARBALL:-1}" = "1" ]; then
    TG_NAME="$(basename "$DEST").tar.gz"
    TG_DIR="$(dirname "$DEST")"
    rm -f "$TG_DIR/$TG_NAME"
    ( cd "$TG_DIR" && tar -czf "$TG_NAME" "$(basename "$DEST")" )
    if [ -f "$TG_DIR/$TG_NAME" ]; then
        echo "  tar.gz: $TG_DIR/$TG_NAME  ($(du -h "$TG_DIR/$TG_NAME" | cut -f1))"
    else
        echo "  [警告] tar.gz 未生成（tar 失败？）"
    fi
else
    echo "  (已跳过 tar.gz：DVDA_TARBALL=0)"
fi
