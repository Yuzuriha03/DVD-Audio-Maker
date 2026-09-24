# 上游 bug 修复（3 个）—— 史料，**不是可重跑的脚本**

这三项是 dvda-author 上游自身的缺陷，与本工程无关，但不修就编译不过 /
会死循环。**改动已经在 `tools/dvda-author-mlp8` 源码树里**，本目录只作记录。

## ⚠️ 不能直接重跑

| 脚本 | 目标文件 | 为什么不能重跑 |
|---|---|---|
| `fix_merged_and_audio_close.py` | `src/audio.c`、`src/include/audio2.h` | 用**行号**删函数（`delete_range(1903, 1914)`），行号早已漂移 |
| `fix_close_handles.py` | `libutils/src/include/winport.h` | 直接**追加**一段内联函数，重跑会产生重复定义 |
| `fix_secure_mkdir.py` | `libutils/src/libc_utils.c` | 有 `assert old in s`，已修则断言失败 |

它们的路径还写着 `tools/dvda-author`（已删除的 core 树）—— 这本身就是史料的一部分。
要重新应用，请按下面的「改了什么」手工做。

## 改了什么（以及现在的状态）

### 1. `interleave_*_merged` 半成品函数 → 删除，调用点改回非 merged 版

上游留了两个从未实现的 `interleave_sample_extended_merged` /
`interleave_24_bit_sample_extended_merged`，而 `audio_close_merged()` 的签名与
声明也对不上。修法是删掉半成品、把调用点改回**普通版**，并修正
`audio_close_merged()` 的签名与 `audio2.h` 里的声明。

验证（在 mlp8 树里）：

```bash
grep -c "interleave_sample_extended_merged\|interleave_24_bit_sample_extended_merged" \
     tools/dvda-author-mlp8/src/audio.c        # → 0
grep -c "audio_close_merged" tools/dvda-author-mlp8/src/audio.c \
     tools/dvda-author-mlp8/src/include/audio2.h # → 2 与 1（签名+声明）
```

### 2. `close_handles()` 的重载冲突 → 改名 + 补声明

`winport.h` 里有一个 4 参 `static inline close_handles()`，而 `winport.c` 里是
**5 参按指针**的实现 —— 同名不同签名，Linux 下编译失败。

修法：

- `winport.h` 的 4 参内联版**改名**为 `close_file_descriptors()`
- `winport.h` 补上 Linux 下按值传参的声明：
  `void close_handles(int tube0, int tube1, int tubeerr0, int tubeerr1);`
- `winport.c` 的实现改为上面这个按值传参的 4 参版本

> 这一项后来由 `patches/_merged/patch_base.py` **重新**做过一遍（patch_base 是
> 为 mlp8 树写的版本）。两者效果相同 —— patch_base 里的 `winport.h 内联函数改名`
> 就是这件事。也就是说 **这一项有脚本可以重跑**。

### 3. `secure_mkdir()` 空路径 → 无限递归

```c
if (path == NULL || path[0] == '\0')
  {
   fprintf(stderr, ... "Could not create directory with empty or null path. ...");
   return secure_mkdir(path, globals->access_rights, globals);   /* ← 递归调用自己 */
  }
```

空路径时**递归调用自身**，不死不休。改为 `return -1`。

> 这一项**没有**对应的 `_merged` 补丁脚本，只存在于 mlp8 源码树里。
> 它在 `libutils/src/libc_utils.c` 第 859 行附近，校验靠 `SOURCE-MANIFEST.txt`
> 的 md5 兜底。

## 从上游重建时的顺序

1. `git clone https://github.com/fabnicol/dvda-author` → `tools/dvda-author-mlp8`
2. 按本目录「改了什么」手工做第 1、3 项（第 2 项可由 `patch_base.py` 代劳）
3. 再按 `../README.md` 的清单跑 `_merged/*.py`

上游基线 commit：`8fca43a`（`Update tag-and-release`）。
