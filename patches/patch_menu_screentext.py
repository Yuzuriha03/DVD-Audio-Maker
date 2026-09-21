#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""修复 `-O/--screentext`（菜单文字链）—— 当前版本一用就崩。

调用点（menu.c 的 generate_menu_pics，screentext 分支）有三处缺陷，
合起来使 `--screentext` **任何输入都段错误**：

1) `basemotif = fn_strtok(chain, '=', ..., 1, cutloop, remainder)`

   `count` 是传给 `cutloop` 的「消费几个子串」计数器，而 cutloop 是
   `++loop; if (count > loop) return 1; else { loop=0; return 0; }`
   —— 传 1 时第 1 个子串就 return 0 而 break。
   而 fn_strtok 在 break 时的语义是：`array[0..k-1]` 有效、`array[k]` 被置哨兵
   NULL、`remainder` = 第 k 个子串。
   于是 k=0：`basemotif[0]` 变成 NULL（专辑标题丢失），`remainder` 退回**整串**。

   传 2 才正确：第 1 个子串进 array[0]，break 于 k=1，
   `remainder` = 第一个 '=' 之后的部分。

2) `dim` 是 fn_strtok 输出的**槽位数**（元素数 + 1 个哨兵），
   但遍历写成 `for (k = 0; k < dim; k++)`，最后一次读到
   `grouparray[dim-1] == NULL` → `strlen(NULL)` 段错误。
   上界必须用实际元素个数。

3) `size` 被复用：既作为列宽传入 `&size` 当 fn_strtok 的输出参数，
   之后又当**字符串截断长度**用于
   `if (strlen(text) > size) text[size] = '\\0'`。
   fn_strtok 会把 `*size` 覆写成子串个数（例如 3），
   于是**每个曲名被截成 3 字节**（中文只剩 1 个字）。

另外两处 `remainder`/`rem` 是不初始化 VLA，而 fn_strtok 只在提前 break 时
才写它们（子串数刚好用尽时**不写**）→ 读到未初始化栈内存。
统一先置 `'\\0'`。

修好后 `--screentext "专辑=组标题=曲1,曲2:组标题2=曲3,曲4"` 才可用。
"""
import pathlib
import sys

PATH = pathlib.Path("/root/dvda-author-mlp8/src/menu.c")

if not PATH.exists():
    print("[FAIL] 找不到 %s" % PATH)
    sys.exit(1)

text = PATH.read_text(encoding="utf-8", errors="surrogateescape")
n_ok = 0


def rep(old, new, label):
    global text, n_ok
    if old not in text:
        print("[MISS] %s" % label)
        return False
    if text.count(old) != 1:
        print("[MISS] %s（匹配到 %d 处，需唯一）" % (label, text.count(old)))
        return False
    text = text.replace(old, new, 1)
    print("[OK] %s" % label)
    n_ok += 1
    return True


OLD = """  if (img->screentextchain)
    {
      uint32_t size;

      size = (uint16_t)((norm_x - 40 - 20 * (img->ncolumns - 1)) / img->ncolumns);

      // to avoid using reentrant version of strtok (strtok_r, not mingw32 protable)

      char remainder[strlen(img->screentextchain)];

      basemotif = fn_strtok(img->screentextchain, '=', basemotif, &size, 1, cutloop, remainder, globals) ;
      albumtext = basemotif[0];

      grouparray = fn_strtok(remainder, ':', grouparray, &dim, 0, NULL, NULL, globals) ;

      tracktext = calloc(dim, sizeof(char **));
      if (tracktext == NULL) perror(ERR "Track text allocation");
      grouptext = calloc(dim, sizeof(char **));
      if (grouptext == NULL) perror(ERR "Group text allocation");

      for (k = 0; k < dim; k++)
        {
          char rem[strlen(grouparray[k])];
          grouptext[k] = fn_strtok(grouparray[k], '=', grouptext[k], &globals->grouptextsize[k], 1, cutloop, rem, globals);
          tracktext[k] = fn_strtok(rem, ',', tracktext[k], &globals->tracktextsize[k], 0, NULL, NULL, globals);
          free(grouparray[k]);
        }
"""

NEW = """  if (img->screentextchain)
    {
      /* ⚠️ 这里**不能**用 fn_strtok(..., count, cutloop, ...)：
         cutloop 的 `static int32_t loop` 会在调用之间**泄漏** ——
         `++loop; if (count > loop) return 1; else { loop = 0; return 0; }`
         当某次调用的子串数**少于** count 时循环自然结束，loop **不归零**；
         下一次调用就会在**第一个子串上 break**，而 fn_strtok 在 break 时
         把 array[0] 置成**哨兵 NULL**。
         实测后果：`--screentext` 的段0 只有一个 '='（如
         `ALBUM==曲1,曲2`）→ 子串数 1 < count=2 → loop 留下 1
         → 段1 的 grouptext[1][0] 变成 NULL
         → 画组标题时 mogrify_img(strlen(NULL)) 段错误。
         所以改成**手工按第一个分隔符切分**，不依赖任何 static 状态。 */

      /* size 只用于「按像素预算截断曲名」，绝不能同时当 fn_strtok 的输出参数
         （那会被覆写成子串个数，把曲名截成几个字节）。 */
      uint32_t size;
      size = (uint16_t)((norm_x - 40 - 20 * (img->ncolumns - 1)) / img->ncolumns);

      char *psep = strchr(img->screentextchain, '=');
      char *groupspec;
      if (psep)
        {
          *psep = '\\0';
          albumtext = strdup(img->screentextchain);
          groupspec = psep + 1;
        }
      else
        {
          albumtext = strdup(img->screentextchain);
          groupspec = NULL;
        }
      if (!albumtext || !albumtext[0])
        {
          free(albumtext);
          albumtext = strdup(DEFAULT_ALBUM_HEADER);
        }

      /* ':' 分组的解析用 f=NULL（没有计数器回调），无 static 泄漏风险 */
      grouparray = groupspec ? fn_strtok(groupspec, ':', grouparray,
                                         &dim, 0, NULL, NULL, globals)
                             : NULL;

      /* dim 是槽位数（含末尾 NULL 哨兵），实际定义的组数要用 arraylength。
         后面画页面时按 dim 索引 grouptext[]/tracktext[]（含释放循环），
         所以 dim 至少要 >= ngroups。 */
      int ndef = grouparray ? arraylength(grouparray) : 0;
      if (ndef < (int) ngroups)
        foutput(WAR "Screen text defines only %d group(s) but the disc has "
                "%d：missing groups will get blank labels.\\n",
                ndef, (int) ngroups);
      dim = (ndef > (int) ngroups) ? ndef : (int) ngroups;

      tracktext = calloc(dim + 1, sizeof(char **));
      if (tracktext == NULL) perror(ERR "Track text allocation");
      grouptext = calloc(dim + 1, sizeof(char **));
      if (grouptext == NULL) perror(ERR "Group text allocation");

      for (k = 0; k < ndef && k < dim; k++)
        {
          char *q = strchr(grouparray[k], '=');
          char *trackspec;
          if (q)
            {
              *q = '\\0';              /* 组标题到此为止 */
              trackspec = q + 1;       /* 之后是曲名列表 */
            }
          else
            {
              /* 没有 '=' → 整段只是组标题，轨名留空。
                 （与文档一致：组标题在 '=' 之前） */
              trackspec = NULL;
            }

          grouptext[k] = (char **) calloc(2, sizeof(char *));
          grouptext[k][0] = strdup(grouparray[k]);
          globals->grouptextsize[k] = 2;

          if (trackspec)
            tracktext[k] = fn_strtok(trackspec, ',', tracktext[k],
                                     &globals->tracktextsize[k], 0,
                                     NULL, NULL, globals);

          free(grouparray[k]);
        }
      free(grouparray);
      grouparray = NULL;

      /* 统一补齐：menu.c 画文字时按 ntracks[组] 索引 tracktext[组][轨]，
         条数不足就会读到 NULL 哨兵 → strlen(NULL) 段错误。
         **与文字内容无关，只与条数有关**，故这里无条件补够。 */
      for (k = 0; k < dim; k++)
        {
          if (!grouptext[k])
            {
              grouptext[k] = (char **) calloc(2, sizeof(char *));
              grouptext[k][0] = strdup("");
              globals->grouptextsize[k] = 2;
            }

          int need = (k < (int) ngroups) ? (int) ntracks[k] : 0;
          int have = tracktext[k] ? arraylength(tracktext[k]) : 0;
          if (have >= need) continue;

          char **grown = (char **) realloc(tracktext[k],
                                           (need + 1) * sizeof(char *));
          if (!grown) { perror(ERR "tracktext realloc"); continue; }
          tracktext[k] = grown;
          for (int i = have; i < need; ++i) tracktext[k][i] = strdup("");
          tracktext[k][need] = NULL;
          globals->tracktextsize[k] = need + 1;
        }
"""

rep(OLD, NEW, "重写 screentext 解析（改用手工切分、统一补齐条数、size 不复用）")

# 收尾释放：basemotif 现在恒为 NULL，而原式直接 free(basemotif[0]) —— 解引用 NULL
rep(
    """  if (img->screentextchain)
    {
      free(basemotif[0]);
      free(basemotif[1]);
      free(basemotif);
    }""",
    """  if (img->screentextchain && basemotif)
    {
      free(basemotif[0]);
      free(basemotif[1]);
      free(basemotif);
    }""",
    "basemotif 释放前判空",
)

if n_ok == 2:
    PATH.write_text(text, encoding="utf-8", errors="surrogateescape")
    print("\nscreentext 修复完成，共 %d 项" % n_ok)
else:
    print("\n只应用了 %d/2 项，**未写入文件**" % n_ok)
    sys.exit(1)
