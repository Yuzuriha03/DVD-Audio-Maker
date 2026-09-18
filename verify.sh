#!/bin/bash
# ============================================================================
# 成品校验：DVD5 容量 + 光盘结构审计 + 时间轴 + MLP 无损
#
# 用法（WSL 内）:
#   bash verify.sh            # 全部检查
#   bash verify.sh capacity   # 仅容量与结构
#   bash verify.sh audit      # 仅光盘一致性审计（扇区/PTS/轨边界）
#   bash verify.sh timeline   # 仅时间轴抽查
#   bash verify.sh lossless   # 仅 MLP 无损
# ============================================================================
set -u

FINAL_DIR="${DVDA_FINAL_DIR:-/mnt/d/鸣潮DVD_Audio}"
MLP_DIR="${DVDA_MLP_DIR:-/root/dvda-build/mlp}"
MLP_INDEX="${DVDA_MLP_INDEX:-/root/dvda-build/mlp_index.json}"
NEW="${DVDA_AUTHOR:-/root/dvda-author-mlp8/src/dvda-author-dev}"
ISO_PREFIX="${DVDA_ISO_PREFIX:-Wuthering_Waves_Singles_EPs}"
DVD5_BYTES=4707319808
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

WHAT="${1:-all}"

# ---------------------------------------------------------------- 审计
check_audit() {
  echo "=================== 光盘一致性审计 ==================="
  echo "核对：AOB 扇区数 / 轨间连续性 / PTS 完整性 / PTS 下降点是否落在轨边界"
  echo
  python3 -u "$HERE/audit_disc.py"
}

# ---------------------------------------------------------------- 容量
check_capacity() {
  echo "=================== DVD5 容量核对 ==================="
  local found=0
  shopt -s nullglob
  for f in "$FINAL_DIR"/${ISO_PREFIX}_*.iso; do
    found=1
    local s t
    s=$(stat -c %s "$f")
    t=$(stat -c %y "$f" | cut -d. -f1)
    awk -v n="$(basename "$f")" -v s="$s" -v t="$t" -v l="$DVD5_BYTES" 'BEGIN{
      printf "%-38s %13d B  %s  余 %d B  %s\n",
             n, s, t, l-s, (s<=l ? "可刻入DVD5" : "!! 超出DVD5");
    }'
  done
  [ "$found" = 1 ] || echo "(未找到 ISO，请先运行 02_build.py)"

  echo
  echo "--- 结构抽查（应只含 AUDIO_TS） ---"
  for f in "$FINAL_DIR"/${ISO_PREFIX}_1.iso; do
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

  if [ ! -f "$MLP_INDEX" ]; then
    echo "(未找到 $MLP_INDEX，请先运行 02_build.py，跳过)"
    return
  fi

  local info mlp src rto
  mapfile -t info < <(python3 - "$MLP_INDEX" <<'PY'
import json, os, sys
idx = json.load(open(sys.argv[1], encoding="utf-8"))
keys = sorted(k for k in idx if os.path.exists(k) and os.path.getsize(k) > 0)
if not keys:
    print(""); print(""); print(""); raise SystemExit
k = keys[0]
e = idx[k]
print(k); print(e["src"]); print(e.get("resample_to") or "")
PY
)
  mlp="${info[0]:-}"; src="${info[1]:-}"; rto="${info[2]:-}"
  if [ -z "$mlp" ] || [ -z "$src" ]; then
    echo "(MLP 缓存为空，跳过)"
    return
  fi

  local tmp=/root/dvda-build/verify-tmp
  rm -rf "$tmp"; mkdir -p "$tmp"

  echo "源音源 : $src"
  echo "MLP    : $mlp"
  [ -n "$rto" ] && echo "重采样 : -> ${rto}Hz (soxr)"
  echo

  local ares=()
  [ -n "$rto" ] && ares=(-af "aresample=${rto}:resampler=soxr")

  # 1) MLP 解码后 PCM 与源音源（同样重采样后）比对
  ffmpeg -hide_banner -loglevel error -y -i "$src" "${ares[@]}" -f s24le "$tmp/src.raw"
  ffmpeg -hide_banner -loglevel error -y -i "$mlp" -f s24le "$tmp/dec.raw"
  local n
  n=$(stat -c %s "$tmp/src.raw")
  head -c "$n" "$tmp/dec.raw" > "$tmp/dec_trim.raw"

  if cmp -s "$tmp/src.raw" "$tmp/dec_trim.raw"; then
    echo "[1] MLP 解码 PCM 与源音源逐字节一致  ✔"
  else
    echo "[1] MLP 解码 PCM 与源音源存在差异  ✗"
    echo "    源 $(stat -c %s "$tmp/src.raw") 字节 / 解码 $(stat -c %s "$tmp/dec.raw") 字节"
    cmp -l "$tmp/src.raw" "$tmp/dec_trim.raw" | head -n 3 | sed 's/^/    /'
  fi

  # 2) 成品 ISO 内音轨与源 MLP 比对
  local iso="$FINAL_DIR/${ISO_PREFIX}_1.iso"
  if [ -f "$iso" ]; then
    mkdir -p "$tmp/chk" "$tmp/ext"
    xorriso -osirrox on -indev "$iso" \
      -extract /AUDIO_TS/ATS_01_1.AOB "$tmp/chk/a.AOB" >/dev/null 2>&1

    # 注: --aob-extract 会在收尾阶段 abort，但所需文件已写出，故忽略返回码
    "$NEW" --aob-extract "$tmp/chk/a.AOB" -o "$tmp/ext" -W -P0 -n >/dev/null 2>&1 || true

    local got
    got=$(find "$tmp/ext" -name 'track_01_title_01.mlp' 2>/dev/null | head -n1)
    if [ -n "$got" ] && cmp -s "$mlp" "$got"; then
      echo "[2] 成品 ISO 内音轨与源 MLP 一致  ✔"
      md5sum "$mlp" "$got" | sed 's/^/    /'
    else
      echo "[2] 成品 ISO 内音轨校验失败  ✗"
    fi
  else
    echo "[2] (未找到成品 ISO，跳过)"
  fi

  rm -rf "$tmp"
}

# ---------------------------------------------------------------- 时间轴
check_timeline() {
  echo "=================== 时间轴校验（PTS） ==================="
  echo "检查 AOB 内每个扇区的 PTS 是否随播放推进。"
  echo "若全部相同，则播放器无法定位进度（进度条不可拖、可能变速播放）。"
  echo

  local iso="$FINAL_DIR/${ISO_PREFIX}_1.iso"
  if [ ! -f "$iso" ]; then
    echo "(未找到成品 ISO，跳过)"
    return
  fi

  local tmp=/root/dvda-build/timeline-tmp
  rm -rf "$tmp"; mkdir -p "$tmp"

  xorriso -osirrox on -indev "$iso" \
    -extract /AUDIO_TS/ATS_01_1.AOB "$tmp/a1.AOB" >/dev/null 2>&1

  python3 "$HERE/check_aob_pts.py" "$tmp/a1.AOB"
  rm -rf "$tmp"
}

case "$WHAT" in
  capacity) check_capacity ;;
  audit)    check_audit ;;
  lossless) check_lossless ;;
  timeline) check_timeline ;;
  all)      check_capacity; echo; check_audit; echo; check_lossless ;;
  *)        echo "用法: bash verify.sh [capacity|audit|timeline|lossless|all]"; exit 1 ;;
esac
