#!/bin/bash
# ============================================================================
# 成品校验：单盘容量 + 光盘结构审计 + 时间轴 + MLP 无损
#
# 用法（WSL 内）:
#   bash verify.sh            # 全部检查
#   bash verify.sh capacity   # 仅容量与结构
#   bash verify.sh audit      # 仅光盘一致性审计（扇区/PTS/轨边界）
#   bash verify.sh timeline   # 仅时间轴抽查
#   bash verify.sh lossless   # 仅 MLP 无损
#   bash verify.sh config     # 仅打印当前配置（排错用）
#
# 所有路径与工具位置都来自 config.sh（见该文件说明）。
# ============================================================================
set -u

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

# ---- 加载配置（bash 与 Python 共用同一份解析结果） ----
if ! CFG_SH="$(python3 "$HERE/dvda_config.py" --shell)"; then
  echo "[配置错误] 无法读取配置，请检查 config.sh" >&2
  exit 2
fi
eval "$CFG_SH"

WHAT="${1:-all}"

# 单盘容量上限（config.sh 未填则用内置默认值）
DISC_LIMIT="${DVDA_DISC_BYTES:-4707319808}"
# 校验用临时目录（放在工作目录下，便于清理）
VDIR="$DVDA_BUILD_DIR/verify-tmp"

# xorriso 提取出的文件保留 ISO 上的只读权限，目录没有写位，直接 rm -rf 删不掉，
# 会在下次运行时留下残留。先补写权限再删。
rm_rf_rw() {
  [ -e "$1" ] || return 0
  find "$1" -mindepth 1 -exec chmod u+w {} + 2>/dev/null
  chmod u+w "$1" 2>/dev/null
  rm -rf "$1"
}

# ---------------------------------------------------------------- 配置
check_config() {
  python3 "$HERE/dvda_config.py"
}

# ---------------------------------------------------------------- 审计
check_audit() {
  echo "=================== 光盘一致性审计 ==================="
  echo "核对：AOB 扇区数 / 轨间连续性 / PTS 完整性 / PTS 下降点是否落在轨边界"
  echo
  python3 -u "$HERE/audit_disc.py"
}

# ---------------------------------------------------------------- 容量
check_capacity() {
  echo "=================== 容量核对（上限 ${DISC_LIMIT} 字节/盘） ==================="
  local found=0 n=0
  shopt -s nullglob
  for f in "$DVDA_FINAL_DIR"/${DVDA_ISO_PREFIX}_*.iso; do
    found=1; n=$((n+1))
    local s t
    s=$(stat -c %s "$f")
    t=$(stat -c %y "$f" | cut -d. -f1)
    awk -v n="$(basename "$f")" -v s="$s" -v t="$t" -v l="$DISC_LIMIT" 'BEGIN{
      printf "%-40s %13d B  %s  余 %d B  %s\n",
             n, s, t, l-s, (s<=l ? "可刻入" : "!! 超出上限");
    }'
  done
  if [ "$found" = 0 ]; then
    echo "(未找到 ISO，请先运行 02_build.py)"
    echo "  输出目录: $DVDA_FINAL_DIR"
    echo "  文件名前缀: ${DVDA_ISO_PREFIX}_*.iso"
    return 1
  fi
  echo "  共 $n 张"

  echo
  echo "--- 结构抽查（应只含 AUDIO_TS） ---"
  for f in "$DVDA_FINAL_DIR"/${DVDA_ISO_PREFIX}_1.iso; do
    [ -f "$f" ] || continue
    xorriso -indev "$f" -ls / 2>/dev/null | tail -n 3
    echo "卷标: $(xorriso -indev "$f" -toc 2>/dev/null | grep 'ISO session' | sed 's/.*, *//')"
  done
}

# ---------------------------------------------------------------- 无损
# MLP 容器不记录时长，无法回读；源文件与重采样目标由 mlp_index.json 提供
# （02_build.py 生成）。比对时对源施加与编码时相同的重采样链。
check_lossless() {
  echo "=================== MLP 无损验证 ==================="

  if [ ! -f "$DVDA_MLP_INDEX" ]; then
    echo "(未找到 $DVDA_MLP_INDEX，请先运行 02_build.py，跳过)"
    return 1
  fi

  local info mlp src rto how
  # 选取「第 1 张盘 组1 的第 1 轨」对应的 MLP —— 必须与 ATS_01_1.AOB 同组，
  # 否则会误报不一致（ATS_01_1.AOB 存的是组1 = 最先传入 -g 的那组）。
  # 定位顺序：先查构建日志里 dvda-author 的命令行，再退回 mlp_index.json
  # 里的分盘计划。两条路都拿不到时**明确报错**，绝不猜 —— 猜错会选到另一组
  # 的第 1 轨（例如按文件名排序时 group_44100_* 排在 group_48000_* 之前），
  # 结果是把正确无误的 ISO 判为失败。
  mapfile -t info < <(python3 - "$DVDA_MLP_INDEX" "$DVDA_BUILD_LOG" \
                                 "$DVDA_MLP_DIR" <<'PY'
import json, os, re, sys

idx = json.load(open(sys.argv[1], encoding="utf-8"))
log = sys.argv[2] if len(sys.argv) > 2 else ""
pfx = (sys.argv[3] if len(sys.argv) > 3 else "").rstrip("/") + "/"

target = None
how = ""

# 途径一：构建日志里 dvda-author 的命令行（最贴近实际构建）
if log and os.path.exists(log) and pfx != "/":
    t = open(log, encoding="utf-8", errors="replace").read()
    t = re.sub(r"\x1b\[[0-9;]*[A-Za-z]", "", t)
    for line in t.splitlines():
        # dvda-author 命令行: + <author> -g <mlp...> -o <out>/disc1 -D ...
        if not line.startswith("+ ") or " -g " not in line:
            continue
        if "/disc1 " not in line and "/disc1\t" not in line:
            continue
        seg = line.split(" -g ", 1)[1].split(" -o ", 1)[0]
        # 文件名含空格，不能用 split()；按 MLP 目录前缀切分，逐个取到 .mlp 结尾
        names = []
        for chunk in seg.split(pfx)[1:]:
            e = chunk.find(".mlp")
            if e >= 0:
                names.append(pfx + chunk[:e + 4])
        if names:
            target = names[0]
            how = "构建日志（dvda-author 命令行）"
        break

# 途径二：mlp_index.json 的 __discs__ 分盘计划（不依赖日志是否还在）
if target is None:
    try:
        tr = idx["__discs__"][0]["groups"][0]["tracks"][0]
        target = tr["mlp"]
        how = "mlp_index.json 分盘计划（第1盘 组1 第1轨）"
    except (KeyError, IndexError, TypeError):
        target = None

if not target or not os.path.exists(target):
    print("__NOINFO__")
    for _ in range(7):
        print("")
    raise SystemExit

e = idx.get(target, {})
print(target)
print(e.get("src", ""))
print(e.get("resample_to") or "__NONE__")
print(how)
# 外部 MLP 信息：来源 / 是否被重采样 / 时长 / 采样率
print(e.get("mlp_source") or "ffmpeg")
print("1" if e.get("ext_resampled") else "0")
print(e.get("dur") or 0)
print(e.get("sr") or 0)
PY
)
  mlp="${info[0]:-}"; src="${info[1]:-}"
  rto="${info[2]:-}"; how="${info[3]:-}"
  msrc="${info[4]:-ffmpeg}"; ext_rs="${info[5]:-0}"
  dur="${info[6]:-0}"; msr="${info[7]:-0}"
  [ "$rto" = "__NONE__" ] && rto=""

  if [ "$mlp" = "__NOINFO__" ] || [ -z "$mlp" ] || [ -z "$src" ]; then
    echo "[FAIL] 无法确定「第 1 盘 组1 第1轨」对应的源 MLP，跳过比对。"
    echo "       构建日志里没有 dvda-author 命令行（例如只跑过 --dry-run），"
    echo "       且 mlp_index.json 里没有 __discs__ 分盘计划（旧版索引）。"
    echo "       解决：运行一次真正的 bash build.sh 重建索引与日志，"
    echo "             或不要用 --dry-run 覆盖日志（新版已分离为 build-dryrun.log）。"
    return 1
  fi

  rm_rf_rw "$VDIR"; mkdir -p "$VDIR"

  echo "源音源 : $src"
  echo "MLP    : $mlp"
  echo "来源   : $msrc"
  [ -n "$rto" ] && echo "重采样 : -> ${rto}Hz (soxr)"
  [ -n "$how" ] && echo "定位依据: $how"

  local rc=0
  local byte_exact=1
  # ---- 外部编码器改过采样率时：不逐字节比对 ----
  # 原因：外部编码器（如 SurCode）用的重采样滤波器与 soxr 不同，
  # 输出的 PCM 在 LSB 上必然有差别，逐字节比对只会得到无意义的失败。
  # 改而核对「解码采样数 == 时长 × MLP 采样率」，并在 ISO 那一项仍然
  # 逐字节比对（因为那两边是同一份 MLP，应当完全一致）。
  local byte_exact=1
  if [ "$msrc" = "external" ] && [ "$ext_rs" = "1" ]; then
    byte_exact=0
    echo
    echo "[1] 外部编码器改过采样率（源 $(python3 -c "import sys;print(round(float(sys.argv[1])))" "$dur" 2>/dev/null || echo ?)s × ${msr}Hz），"
    echo "    跳过逐字节比对 —— 外部编码器的重采样滤波器与 soxr 不同，"
    echo "    LSB 差异属预期，逐字节比只会误报。改为核对采样数："
    local got_n
    got_n=$("$DVDA_FFMPEG" -hide_banner -nostdin -v info -i "$mlp" \
              -af astats=metadata=1 -f null - 2>&1 \
            | grep -oP 'Number of samples:\s*\K[0-9]+' | head -n1)
    local want_n
    want_n=$(python3 -c "import sys;print(round(float(sys.argv[1])*int(sys.argv[2])))" "$dur" "$msr" 2>/dev/null || echo 0)
    local err_n
    err_n=$("$DVDA_FFMPEG" -hide_banner -nostdin -v info -i "$mlp" -f null - 2>&1 \
            | grep -ciE 'error|invalid|not implemented' || true)
    echo "    解码采样数 $got_n   期望 $want_n   解码错误 $err_n"
    local tol=$(( msr / 20 ))          # 50 ms
    if [ "$want_n" -gt 0 ] && [ "$got_n" -gt 0 ] \
       && [ $(( got_n > want_n ? got_n - want_n : want_n - got_n )) -le "$tol" ] \
       && [ "$err_n" = 0 ]; then
      echo "[1] 采样数与时长吻合（±50ms 内）、解码无错  ✔"
    else
      rc=1
      echo "[1] 采样数或解码异常  ✗"
    fi
    echo
  fi

  local ares=()
  [ -n "$rto" ] && ares=(-af "aresample=${rto}:resampler=soxr")

  if [ "$byte_exact" = "1" ]; then
    # 1) MLP 解码后 PCM 与源音源（同样重采样后）比对
    "$DVDA_FFMPEG" -hide_banner -loglevel error -y -i "$src" "${ares[@]}" \
      -f s24le "$VDIR/src.raw"
    "$DVDA_FFMPEG" -hide_banner -loglevel error -y -i "$mlp" -f s24le "$VDIR/dec.raw"
    local n
    n=$(stat -c %s "$VDIR/src.raw")
    head -c "$n" "$VDIR/dec.raw" > "$VDIR/dec_trim.raw"

    if cmp -s "$VDIR/src.raw" "$VDIR/dec_trim.raw"; then
      echo "[1] MLP 解码 PCM 与源音源逐字节一致  ✔"
    else
      rc=1
      echo "[1] MLP 解码 PCM 与源音源存在差异  ✗"
      echo "    源 $(stat -c %s "$VDIR/src.raw") 字节 / 解码 $(stat -c %s "$VDIR/dec.raw") 字节"
      cmp -l "$VDIR/src.raw" "$VDIR/dec_trim.raw" | head -n 3 | sed 's/^/    /'
    fi
  fi

  # 2) 成品 ISO 内音轨与源 MLP 比对
  local iso="$DVDA_FINAL_DIR/${DVDA_ISO_PREFIX}_1.iso"
  if [ -f "$iso" ]; then
    mkdir -p "$VDIR/chk" "$VDIR/ext"
    xorriso -osirrox on -indev "$iso" \
      -extract /AUDIO_TS/ATS_01_1.AOB "$VDIR/chk/a.AOB" >/dev/null 2>&1

    # 注: --aob-extract 会在收尾阶段段错误，但所需文件已写出，故忽略返回码
    "$DVDA_AUTHOR" --aob-extract "$VDIR/chk/a.AOB" -o "$VDIR/ext" \
      -W -P0 -n >/dev/null 2>&1 || true

    local got
    got=$(find "$VDIR/ext" -name 'track_01_title_01.mlp' 2>/dev/null | head -n1)
    if [ -n "$got" ] && cmp -s "$mlp" "$got"; then
      echo "[2] 成品 ISO 内音轨与源 MLP 一致  ✔"
      md5sum "$mlp" "$got" | sed 's/^/    /'
    else
      rc=1
      echo "[2] 成品 ISO 内音轨校验失败  ✗"
    fi
  else
    echo "[2] (未找到成品 ISO，跳过)"
  fi

  rm_rf_rw "$VDIR"
  return $rc
}

# ---------------------------------------------------------------- 时间轴
check_timeline() {
  echo "=================== 时间轴校验（PTS） ==================="
  echo "检查 AOB 内每个扇区的 PTS 是否随播放推进。"
  echo "若全部相同，则播放器无法定位进度（进度条不可拖、可能变速播放）。"
  echo

  local iso="$DVDA_FINAL_DIR/${DVDA_ISO_PREFIX}_1.iso"
  if [ ! -f "$iso" ]; then
    echo "(未找到成品 ISO，跳过)"
    return 1
  fi

  rm_rf_rw "$VDIR"; mkdir -p "$VDIR"
  xorriso -osirrox on -indev "$iso" \
    -extract /AUDIO_TS/ATS_01_1.AOB "$VDIR/a1.AOB" >/dev/null 2>&1

  python3 "$HERE/check_aob_pts.py" "$VDIR/a1.AOB"
  rm_rf_rw "$VDIR"
}

case "$WHAT" in
  config)   check_config ;;
  capacity) check_capacity ;;
  audit)    check_audit ;;
  lossless) check_lossless ;;
  timeline) check_timeline ;;
  all)
    rc=0
    check_capacity || rc=1
    echo; check_audit || rc=1
    echo; check_lossless || rc=1
    echo
    if [ $rc = 0 ]; then
      echo "=================== 全部校验通过 ✔ ==================="
    else
      echo "=================== 存在问题，见上文 ✗ ==================="
    fi
    exit $rc
    ;;
  *)
    echo "用法: bash verify.sh [config|capacity|audit|timeline|lossless|all]"
    exit 1 ;;
esac
