#!/bin/bash
# 给发布目录生成校验清单（MANIFEST.txt）。
# 目的：确认拷贝/解压过程没损坏文件，也便于判断「这份包是什么时候产的」。
set -e
DEST="${1:?用法: make-release-manifest.sh <发布目录>}"
DEST="$(cd "$DEST" && pwd)"
cd "$DEST"
OUT="MANIFEST.txt"

{
  echo "# DVD-Audio Maker —— 发布内容校验清单"
  echo "#"
  echo "# 生成时间: $(date -u '+%Y-%m-%d %H:%M:%S UTC')"
  echo "# 校验:    cd DVD-Audio-Maker && md5sum -c MANIFEST.txt"
  echo "#          (Windows: certutil -hashfile <文件> MD5 逐个核对)"
  echo "#"
  echo "# 只列举 exe / dll / ttf / ttc / otf —— 这些是「拷坏了就跑不起来」的部分。"
  echo "# 脚本与图片改坏时会报错，不必逐个校验。"
  echo "#"
  # ⚠️ 扩展名列表要含 .otf —— 字体就是 .otf，漏了它字体就不在校验范围内。
  printf "# 共 %s 个文件，合计 %s\n" \
    "$(find . -type f \( -name '*.exe' -o -name '*.dll' -o -name '*.ttf' -o -name '*.ttc' -o -name '*.otf' \) | wc -l)" \
    "$(du -sh --exclude=MANIFEST.txt . | cut -f1)"
  echo
  find . -type f \( -name '*.exe' -o -name '*.dll' -o -name '*.ttf' -o -name '*.ttc' -o -name '*.otf' \) \
    -printf '%P\0' | sort -z | xargs -0 md5sum
} > "$OUT"

printf '  MANIFEST.txt: %s 条\n' "$(grep -cE '^[0-9a-f]{32} ' "$OUT")"
