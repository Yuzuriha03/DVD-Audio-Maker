#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""ATS 图号改成「所属专辑序号」，与 ASVS 的按专辑记录一一对应。

配合 patch_asvs_per_image.py（ASVS 按专辑写 N 条记录）。
分组来自环境变量 DVDA_ASVS_ALBUM_TRACKS（"6,5,6,1,..."）。"""
import pathlib
import sys

SRC = pathlib.Path("/root/dvda-author-mlp8/src")
MARK = "DVDA_ALBUM_RANK"

OLD = """              /* 每轨一个图号（1-based）。ASVS 是单条记录、图数 = 轨数，
                 播放器用这个号去取「该记录的这张图」。 */
              ++pictitlecount;

              atsi[i++] = pictitlecount;
"""

NEW = """              /* 图号 = 所属**专辑**序号（ASVS 也按专辑记录，两者对应）。 */
              ++pictitlecount;
              {
                const char *ap = getenv("DVDA_ASVS_ALBUM_TRACKS");
                uint8_t rk = (uint8_t) pictitlecount;
                if (ap != NULL)
                  {
                    static char ab[2048];
                    char *ak;
                    int pos = 0, acc = 0, me = (int) trackcount - 1;
                    strncpy(ab, ap, sizeof(ab) - 1);
                    ab[sizeof(ab) - 1] = 0;
                    ak = strtok(ab, ",");
                    while (ak != NULL)
                      {
                        int c = atoi(ak);
                        if (c > 0)
                          {
                            if (me < acc + c) { rk = (uint8_t)(pos + 1); break; }
                            acc += c; ++pos;
                          }
                        ak = strtok(NULL, ",");
                      }
                  }
                atsi[i++] = rk;
              }
"""


def main():
    p = SRC / "atsi2.c"
    t = p.read_text(encoding="utf-8", errors="surrogateescape")
    if MARK in t:
        print("[SKIP] 已应用过")
        return 0
    if t.count(OLD) != 1:
        print("[FAIL] 匹配 %d 次" % t.count(OLD))
        return 1
    t = t.replace(OLD, NEW, 1)
    p.write_text(t, encoding="utf-8", errors="surrogateescape")
    print("[OK]   atsi2.c：图号 = 所属专辑序号")
    return 0


if __name__ == "__main__":
    sys.exit(main())
