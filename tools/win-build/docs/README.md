# DVD-Audio Maker（Windows 独立版）

把一批 FLAC 专辑做成**带选曲菜单与封面的 DVD-Audio 光盘镜像**（24-bit 无损 MLP）。
不依赖 WSL，不需要自己编译任何东西 —— 工具链已随包附带。

---

## 1. 准备

| 需要 | 说明 |
|---|---|
| 操作系统 | **Windows 10 1903+ / Windows 11**（依赖 UTF-8 代码页支持） |
| Python | **3.8 或更高**，安装时勾选 *Add Python to PATH* |
| FFmpeg | 用于 MLP 编解码与格式探测 |
| 磁盘空间 | 中间产物约 **20 GB**（成品再加约 8 GB） |

**FFmpeg** 下载 Windows 静态版即可：<https://www.gyan.dev/ffmpeg/builds/>
（选 `ffmpeg-release-full.7z`）。解压后把 `bin` 目录加进 PATH，
或在 `scripts\config.sh` 里填全路径：

```sh
DVDA_FFMPEG="C:/ffmpeg/bin/ffmpeg.exe"
DVDA_FFPROBE="C:/ffmpeg/bin/ffprobe.exe"
```

## 2. 使用

1. **修改 `scripts\config.sh`** —— 这是唯一需要改的文件。至少填这两项：

   ```sh
   DVDA_SRC="D:/Music/鸣潮先约电台"        # 音源目录（递归扫描，一个子目录 = 一张专辑）
   DVDA_FINAL_DIR="D:/DVD_Output"          # 成品 ISO 输出目录
   ```

   音源目录里每张专辑一个子文件夹，曲目用 FLAC：

   ```
   鸣潮先约电台/
     归墟港(游戏《鸣潮》原声音乐) - EP/
       01. 归墟港.flac
       02. 归墟港 (伴奏版).flac
       cover.jpg            ← 封面（可选，但菜单要用）
   ```

2. **第一步：扫描音源、生成清单**（几分钟）

   ```
   dvda.cmd 01_prepare.py
   ```

3. **第二步：编码并出盘**（几十分钟，视曲目量）

   ```
   dvda.cmd 02_build.py
   ```

   成品是 `DVDA_FINAL_DIR` 下的 `*.iso`，可直接刻录或用播放器挂载。

只跑一遍检查、不出盘（很快）：

```
dvda.cmd 02_build.py --dry-run
```

## 3. 目录结构

```
DVD-Audio-Maker\
  dvda.cmd              启动器（务必用它运行，见下）
  README.md             本文件
  scripts\
    config.sh           ★ 你唯一需要改的文件
    01_prepare.py       第一步
    02_build.py         第二步
    menu_assets.py      菜单素材生成
    dvda_config.py      配置加载器
    mlp_align.py        MLP 头部对齐工具
    m4a2flac.py         （可选）M4A/ALAC 转 FLAC
    audit_disc.py       校验：拆 ISO 逐项核对（需 xorriso）
    verify_menu.py      校验：菜单按钮与跳转（需 xorriso）
    quick_check.py      快速自检（纯 Python，无需任何外部命令）
  menu-bin\             工具链（12 个 exe + 100 个 DLL + ImageMagick 配置）
    fonts\
      NotoSansCJKsc-Regular.otf    简体中文（含拉丁）
      NotoSansCJKjp-Regular.otf    日文（含日文汉字字形）
      NotoSansCJKkr-Regular.otf    韩文（谚文）
  data\
    menu\               dvda-author 的素材目录
```

**为什么必须用 `dvda.cmd` 运行**：工具路径（`dvda-author`、`mkisofs`、素材目录、
自带字体）是按本目录布局相对定位的，`dvda.cmd` 会把它们设成环境变量再启动脚本。
这样整个文件夹可以随意移动、拷贝给别人。

## 4. 常见问题

**出盘后菜单文字是空白/方块**
字体没生效。检查 `menu-bin\fonts\` 下的三个 `.otf` 是否都在，
以及安装路径里是否有空格（见下）。

**中文菜单显示的是日文字形**（直/骨/令 等字写法不对）
说明 `DVDA_MENU_FONT` 指向了 `.ttc` 集合字体。ImageMagick 按文件路径加载
`.ttc` 时只取 **face 0**，而 Noto Sans CJK 的 face 0 是 **JP**。
本包已自带抽好的单 face 字体并用 `dvda.cmd` 自动指向，不要手工改成 `.ttc`。

**日文标题里的汉字字形不对**（或用成了简体字形）
菜单按每条文字所属语言自动选 face：中文→SC、日文→JP、韩文→KR。
缺哪个 face 就会回退到主字体（中文 SC）—— 检查 `menu-bin\fonts\` 下
`NotoSansCJKjp-Regular.otf` 是否还在。三项配置（`DVDA_MENU_FONT_JP` / `_KR`）
留空即自动推导，一般不需要改。

**路径含空格 → 出盘失败或菜单缺字**
`dvda-author` 拼 ImageMagick 命令时不给路径加引号，带空格的目录会被截断。
→ 把整个文件夹放到**不含空格**的路径下（如 `D:\DVD-Audio-Maker`），
音源与输出目录同样避免空格。

**提示找不到 Python / 找不到 ffmpeg**
装 Python 时勾选 *Add Python to PATH*；ffmpeg 见第 1 节。
Python 装在别处可以临时指定：`set DVDA_PYTHON=C:\Python314\python.exe`。

**提示找不到 `dvda-author.conf`**
本包只对「用 `dvda.cmd` 运行」的场景做了配置。直接用别的参数手工调
`menu-bin\dvda-author-dev.exe` 时才可能出现，不影响出盘
（正式流程传了 `-W`，不读该文件）。

**`identify` 相关的 FileNotFoundError**
ImageMagick 7 把各工具合并成 `magick`，没有独立的 `identify.exe`。
本包脚本已按 `magick identify` 处理；若自行改动脚本请保留这一处理。

## 5. 可选：校验成品

**`quick_check.py` 推荐先跑** —— 几秒出结果，而且**不需要任何外部命令**：

```
dvda.cmd quick_check.py D:\DVD_Output
```

> 注意它收的是 **ISO 所在目录**（不是单个 `.iso` 文件），目录里所有 `*.iso`
> 都会被校验。它查四件事：
>
> 1. 构建日志里没有 pack 补齐失败
> 2. 各音频组 IFO 声明的轨数之和 == 音源曲目数
> 3. 每轨首扇区都以 pack 头 `00 00 01 BA` 开头（不是的话读盘端会丢掉那一首）
> 4. 各 title 的 cell 时间戳连续（不然进度条拖不动）、静图引用号不越界

下面两个需要 **xorriso**（<https://www.videohelp.com/software/xorriso>）在 PATH 里：

```
dvda.cmd audit_disc.py  D:\DVD_Output\xxx_1.iso
dvda.cmd verify_menu.py D:\DVD_Output\xxx_1.iso
```

> `check_aob_pts.py` 也不需要外部命令，但它收的是**单个 `.iso`**，
> 用来查 AOB 里的 PES 时间戳是否随播放推进（**进度条能不能拖**就看它）。

## 6. 许可

- 本工具链包含改编自 **dvda-author** 的代码（GPL v3），见 `LICENSE`。
- 随包的 ImageMagick / mjpegtools / cdrtools 等二进制与
  `menu-bin\fonts\NotoSansCJK{sc,jp,kr}-Regular.otf`（SIL OFL 1.1）的许可与
  版本见 `THIRD-PARTY.md`。
