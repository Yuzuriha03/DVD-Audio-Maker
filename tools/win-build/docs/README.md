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
    iso9660.py          ISO 读取（纯 Python，校验脚本共用）
    alac_endfix.py      Apple ALAC「未压缩帧缺 END 标记」修复
    mlp_align.py        MLP 头部对齐工具
    m4a2flac.py         （可选）M4A/ALAC 转 FLAC
    quick_check.py      校验：结构与时间轴（快，推荐先跑）
    check_aob_pts.py    校验：AOB 里的 PES 时间戳是否推进
    audit_disc.py      校验：扇区/轨边界/PTS 逐项审计
    verify_menu.py      校验：菜单按钮、跳转、封面图
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

## 5. 校验成品

**全部校验脚本都是纯 Python，不需要 xorriso、dd 或其他外部工具**
（只需 Python；`verify_menu.py` 另需 ffmpeg 用来抽帧看画面）。

出盘后建议按这个顺序跑：

```
REM 1) 结构与时间轴 —— 几秒出结果，先跑这个
dvda.cmd quick_check.py D:\DVD_Output

REM 2) AOB 里的时间戳是否随播放推进（进度条能不能拖就看它）
dvda.cmd check_aob_pts.py D:\DVD_Output\Wuthering_Waves_Singles_EPs_1.iso

REM 3) 逐项审计：扇区数、轨边界、PTS 下降点、pack 头
dvda.cmd audit_disc.py

REM 4) 菜单：页数、按钮跳转、封面图、索引页逐格
dvda.cmd verify_menu.py
```

| 脚本 | 参数 | 查什么 |
|---|---|---|
| `quick_check.py` | ISO **目录** | ① 构建日志无 pack 补齐失败 ② IFO 声明轨数 == 音源曲目数 ③ 每轨首扇区是 pack 头 `00 00 01 BA` ④ 各 title 内 cell 时间戳连续、静图引用号不越界 |
| `check_aob_pts.py` | 单个 `.iso` | AOB 里每个扇区的 PES 时间戳是否单调推进；时间轴有问题时播放器会「加速」、进度条拖不动 |
| `audit_disc.py` | 无（读 config） | 按音频组：AOB 扇区总数 vs 轨道表、轨间是否首尾相接、每扇区有无 PTS、PTS 下降点是否落在 title 起点、首扇区是否 pack 头 |
| `verify_menu.py` | 无（读 config） | 菜单页数、IFO 容量、各页 cell 地址链与 next/prev、播放封面表、菜单画面是不是真的画上了、索引页每格缩略图与专辑名 |

> `quick_check.py` 收的是 **ISO 所在目录**（不是单个 `.iso`），目录里所有
> `*.iso` 都会被校验 —— 因为出盘会有两张盘，这样一次跑完。
>
> `audit_disc.py` / `verify_menu.py` 不带参数，直接从 `config.sh` 读
> `DVDA_FINAL_DIR` / `DVDA_BUILD_DIR` / `DVDA_ISO_PREFIX`。

**判读结果**：输出里 `[OK]` / `✔` 是通过，`[FAIL]` / `✗` 是失败，
脚本退出码 0 = 全部通过。全部通过的样子：

```
快速校验 全部通过 ✔                     （quick_check.py）
审计结论: 全部通过 ✔                    （audit_disc.py）
菜单校验全部通过 ✔                      （verify_menu.py）
```

## 6. 许可

- 本工具链包含改编自 **dvda-author** 的代码（GPL v3），见 `LICENSE`。
- 随包的 ImageMagick / mjpegtools / cdrtools 等二进制与
  `menu-bin\fonts\NotoSansCJK{sc,jp,kr}-Regular.otf`（SIL OFL 1.1）的许可与
  版本见 `THIRD-PARTY.md`。
