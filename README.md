# DVD-Audio Maker

将 FLAC 或 ALAC/M4A 音源制作成标准 **DVD-Audio ISO** 的工具链。

项目以 **.NET 10 / C#** 实现音源准备、MLP 管理、分盘、菜单生成、ISO 发布和成品校验；底层使用经过修改的 `dvda-author` 与支持 `-dvd-audio` 的 `mkisofs`。

> 本仓库不包含音频、商业编码器或预编译第三方工具。使用者需要自备音源，并自行确认相关软件与内容的授权。

## 功能

- 递归扫描 FLAC 和 ALAC/M4A
- 读取专辑、曲名、曲序和日期标签
- 检查解码完整性、声道数和音频参数
- 自动修复特定 Apple ALAC 文件缺失 END 标记的问题
- 按专辑归一化采样率与位深，必要时使用 SoXr 重采样
- 使用 FFmpeg 编码 MLP，或复用 SurCode 等工具生成的外部 MLP
- 保持专辑完整，按容量逐盘填满
- 可选 DVD-Audio AMG 选曲菜单与 ASVS 播放封面
- 事务式发布整套 ISO，失败时保留上一套成品
- 校验 ISO、IFO、AOB、PTS、菜单、容量和无损性
- 支持 Windows 原生自包含发布包及 WSL/Linux 开发流程

## 当前实现

业务入口统一为 C# CLI：

```text
src/DvdaMaker.Cli             命令行入口
src/DvdaMaker.Configuration   配置解析
src/DvdaMaker.Preparation     扫描、归一化、解码校验、ALAC 修复
src/DvdaMaker.Building        MLP、分盘、菜单、出盘、发布与校验
src/DvdaMaker.Formats         ISO9660、MLP、MPEG/PTS 解析
src/DvdaMaker.Processes       外部进程执行
tests/                        兼容性与端到端测试
```

`build.sh` 和 `verify.sh` 是 C# CLI 的便捷包装。迁移前的 Python 业务实现已从主分支删除，最终版本保存在 Git 标签 `python-reference-final`。

## 选择运行方式

| 场景 | 推荐方式 |
|---|---|
| 普通 Windows 用户 | 构建或获取 Windows 自包含发布包，运行 `dvda.cmd` |
| 在仓库中开发或使用 Linux 工具链 | 在 WSL/Linux 中运行 `build.sh` |
| 只需检查配置、规划或 ISO | 直接运行 C# CLI |

---

# Windows 原生流程

Windows 发布包在运行时不需要 WSL、MSYS2、Python 或单独安装 .NET Runtime。MSYS2 只用于从源码构建发布包。

## 1. 构建发布包

### 安装 MSYS2 依赖

将 MSYS2 安装或解压到 `D:\dev\msys64`、`C:\msys64` 等目录，然后在 MINGW64 环境运行：

```bash
pacman -Syu
pacman -S --needed mingw-w64-x86_64-gcc \
                   mingw-w64-x86_64-ffmpeg \
                   mingw-w64-x86_64-pkgconf \
                   mingw-w64-x86_64-freetype \
                   mingw-w64-x86_64-fontconfig \
                   mingw-w64-x86_64-libpng \
                   mingw-w64-x86_64-imagemagick \
                   make
```

### 准备 dvda-author 源码树

需要完整源码树，其中包括：

```text
configure、src/、libutils/、libfixwav/
menu/silence.wav、menu/activeheader
m4.extra.dvdauthor/
dvdauthor-0.7.1/
local.w10/bin/
NotoSansCJK-Regular.ttc
```

可以从固定上游版本应用本仓库的改动集：

```bash
git clone https://github.com/fabnicol/dvda-author tools/win-build/src
cd tools/win-build/src
git checkout 8fca43a
git apply ../../../docs/dvda-author-changes.patch
```

### 执行构建

在仓库根目录运行：

```bat
tools\win-build\build-all.bat
```

若 MSYS2 不在自动探测位置：

```bat
set MSYS2_ROOT=D:\dev\msys64
tools\win-build\build-all.bat
```

发布目录位于：

```text
tools\win-build\release\DVD-Audio-Maker\
```

构建过程会编译 `dvda-author-dev.exe`、带 AMGM 补丁的菜单工具，组装 DLL、ImageMagick 配置和中日韩字体，并发布自包含的 C# CLI。

详细说明见 [`tools/win-build/README.md`](tools/win-build/README.md)。

## 2. 配置发布包

编辑发布目录中的 `config.sh`：

```bash
DVDA_SRC="D:/Music/MyAlbums"
DVDA_FINAL_DIR="D:/DVD_Output"
DVDA_BUILD_DIR="D:/DVD_Output/_work"
DVDA_TITLE="My DVD-Audio"
DVDA_ISO_PREFIX="MyCollection"
DVDA_MAX_DISCS="2"
DVDA_MENU="on"
```

Windows 路径建议使用正斜杠。工具链和菜单字体路径由 `dvda.cmd` 根据发布包布局自动设置。

> 为兼容第三方 C 工具，发布包、工作目录和字体路径最好不要包含空格或特殊符号。

## 3. 准备、出盘与校验

```bat
cd tools\win-build\release\DVD-Audio-Maker

dvda.cmd config --check
dvda.cmd prepare
dvda.cmd build --dry-run
dvda.cmd build
dvda.cmd verify all
```

只做快速结构检查：

```bat
dvda.cmd verify quick
```

---

# WSL / Linux 流程

## 1. 安装依赖

以 Ubuntu 为例：

```bash
sudo apt install dotnet-sdk-10.0 ffmpeg flac \
  make gcc autoconf \
  libavcodec-dev libavformat-dev libavutil-dev libswresample-dev
```

启用菜单时还需要：

```bash
sudo apt install mjpegtools imagemagick fonts-noto-cjk
```

确认 FFmpeg 包含 MLP 编码器：

```bash
ffmpeg -hide_banner -encoders | grep mlp
```

## 2. 准备并编译 dvda-author

```bash
git clone https://github.com/fabnicol/dvda-author /root/dvda-author-mlp8
cd /root/dvda-author-mlp8
git checkout 8fca43a
git apply /path/to/DVD-Audio-Maker/docs/dvda-author-changes.patch

cd /path/to/DVD-Audio-Maker
bash build_dvda_author_mlp.sh
```

默认工具路径：

```text
/root/dvda-author-mlp8/src/dvda-author-dev
/root/dvda-author-mlp8/local.ubuntu.20.10/bin/mkisofs
```

源码树不在默认位置时，在 `config.sh` 中设置 `DVDA_AUTHOR`、`DVDA_MKISOFS` 和 `DVDA_AUTHOR_SRC`。

## 3. 配置

最少需要设置音源和输出目录：

```bash
DVDA_SRC="/mnt/d/Music/MyAlbums"
DVDA_FINAL_DIR="/home/user/dvda/out"
DVDA_BUILD_DIR="/home/user/dvda/build"
DVDA_TITLE="My DVD-Audio"
DVDA_ISO_PREFIX="MyCollection"
```

Windows 与 WSL 路径的对应关系：

| Windows | WSL |
|---|---|
| `C:\Users\me\Music` | `/mnt/c/Users/me/Music` |
| `D:\Music\Albums` | `/mnt/d/Music/Albums` |

建议把 `DVDA_BUILD_DIR` 放在 WSL 的 ext4 文件系统中。MLP 缓存包含大量读写，放在 `/mnt/c` 或 `/mnt/d` 会明显变慢。

查看并检查生效配置：

```bash
dotnet run --project src/DvdaMaker.Cli -- config
dotnet run --project src/DvdaMaker.Cli -- config --check
```

配置优先级：

```text
环境变量 > config.sh > 内置默认值
```

配置文件只解析 `KEY=VALUE`，不会执行变量展开或命令替换。请填写绝对路径，不要在值中引用其他变量。

## 4. 一键构建

```bash
bash build.sh --dry-run   # prepare + 预演分盘，不生成 ISO
bash build.sh             # prepare + 正式出盘
bash verify.sh all        # 完整校验
```

指定另一份配置：

```bash
bash build.sh --config /path/to/config.sh
bash verify.sh all --config /path/to/config.sh
```

VS Code 内置任务：

- `WSL: 预演分盘 (build.sh --dry-run)`
- `WSL: 完整出盘 (build.sh)`
- `WSL: 校验成品 (verify.sh)`

---

# 音源要求

推荐一个目录对应一张专辑：

```text
音源根目录/
├── Album A/
│   ├── 01. First Song.flac
│   ├── 02. Second Song.flac
│   └── cover.jpg
└── Album B/
    ├── 01. Song One.m4a
    └── 02. Song Two.m4a
```

支持：

- FLAC
- M4A 容器中的 ALAC

每个文件建议具有以下标签：

| 标签 | 用途 |
|---|---|
| `date` | 决定专辑顺序 |
| `track` | 决定专辑内曲序 |
| `album` | 识别专辑及保持专辑完整 |
| `title` | 曲目名称及菜单显示 |

同一批音源必须使用相同声道数。采样率和位深可以混用；准备阶段会按专辑选择主要参数并处理少数不一致曲目。

封面支持常见的 JPG、PNG 和 WebP。封面只用于菜单及播放静图，不会修改音频本体。

# MLP 来源

## 使用 FFmpeg

```bash
DVDA_MLP_SOURCE="ffmpeg"
DVDA_MLP_EXTERNAL_DIR=""
```

构建器会从音源生成 MLP，执行结构检查、字节对齐及 EOS 校验。有效缓存保存在工作目录中，之后可以复用。

## 使用外部 MLP

```bash
DVDA_MLP_SOURCE="external"
DVDA_MLP_EXTERNAL_DIR="/path/to/mlp-root"
```

外部目录必须与音源目录同构：

```text
音源：<DVDA_SRC>/Album/01 Song.flac
MLP ：<DVDA_MLP_EXTERNAL_DIR>/Album/01 Song.mlp
```

构建器会探测 MLP 的实际参数，并拒绝空文件、重名歧义、缺少 EOS、无效结构或多个曲目复用同一文件。

外部模式仍需要原始音源，因为曲序、专辑、标题、封面及时长校验来自音源元数据。

## 直接调用 SurCode Batch MLP Encoder

```bash
DVDA_MLP_SOURCE="surcode-batch"
DVDA_MLP_EXTERNAL_DIR="D:/Music/MLP"
DVDA_MLP_BATCH_ENCODER="C:/path/to/Batch-MLP-Encoder-3"
DVDA_MLP_SURCODE_EXE="C:/path/to/SurCode MLP/surcodemlp.exe"
DVDA_MLP_EAC3TO_EXE="C:/path/to/eac3to/eac3to.exe"
DVDA_MLP_SURCODE_SAMPLE_RATE="48000"
DVDA_MLP_SURCODE_BITS="24"
```

构建器会把缺少或过期的 FLAC 按专辑作为命令行参数传给 Batch MLP Encoder，
将输出暂存到纯 ASCII 工作目录，再移动到与音源同构的 MLP 缓存目录。留空
`DVDA_MLP_SURCODE_EXE` 和 `DVDA_MLP_EAC3TO_EXE` 必须在 `config.sh` 中显式配置，
不会使用 Batch MLP Encoder GUI 曾经保存的路径。已有且通过结构校验的 MLP 会直接复用。

旧的 `surcode` 值仅复用现成 MLP，行为与 `external` 相同；只有
`surcode-batch` 会把 FLAC 传给 Batch MLP Encoder 并调用官方 SurCode 编码器。

# 分盘规则

- 以专辑为最小单位，默认不拆散专辑
- 按全局曲序逐盘填满，而不是按盘数平均分配
- `DVDA_MAX_DISCS` 只设置允许的盘数上限，不参与切分
- 默认单盘容量为 DVD-5：`4,707,319,808` 字节
- DVD-9 可设置 `DVDA_DISC_BYTES="8540123136"`
- 每个 DVD-Audio 音频组最多 99 轨

常用配置：

```bash
DVDA_MAX_DISCS="2"
DVDA_GROUP_TRACK_LIMIT="99"
DVDA_DISC_BYTES=""
```

# 菜单与播放封面

菜单默认关闭：

```bash
DVDA_MENU="off"
```

启用菜单：

```bash
DVDA_MENU="on"
DVDA_MENU_TRACKS_PER_PAGE="12"
DVDA_MENU_INDEX_MIN_ALBUMS="4"
DVDA_MENU_STILLPICS="on"
DVDA_MENU_COVER_DIM="35"
```

生成内容包括：

- AMG 专辑索引和选曲页：`AUDIO_TS.IFO`、`AUDIO_TS.VOB`
- ASVS 播放封面：`AUDIO_SV.IFO`、`AUDIO_SV.VOB`
- 中、日、韩及拉丁字符覆盖检查
- 页面、按钮跳转、封面数量和视觉内容验证

菜单仍属于 DVD-Audio 规范；音频保存在 `ATS_*.AOB` 中，不会转成 DVD-Video 音频。

> 不要使用发行版自带的普通 `dvdauthor` 替代本项目编译的版本。菜单跳转依赖 AMGM 补丁。

# 输出到 Windows

在 WSL 中，建议先将 ISO 生成到 Linux 文件系统，再由 Windows 原生 `Robocopy.exe` 拷出：

```bash
DVDA_FINAL_DIR="/home/user/dvda/out"
DVDA_WINDOWS_DEST="D:\DVD_Output"
```

复制使用 `\\wsl.localhost\<发行版>\...` 作为源。Robocopy 退出码 0–7 视为成功；复制失败只产生警告，不会删除 Linux 侧已发布的 ISO。

如果 `DVDA_FINAL_DIR` 本身就在 `/mnt/c` 或 `/mnt/d`，不要再设置 `DVDA_WINDOWS_DEST`。

# 命令参考

以下示例使用源码工作区中的 CLI。Windows 发布包中将命令前缀替换为 `dvda.cmd`。

## 配置

```bash
dotnet run --project src/DvdaMaker.Cli -- config
dotnet run --project src/DvdaMaker.Cli -- config --check
dotnet run --project src/DvdaMaker.Cli -- config --shell
```

## 准备与构建

```bash
dotnet run --project src/DvdaMaker.Cli -- prepare
dotnet run --project src/DvdaMaker.Cli -- plan
dotnet run --project src/DvdaMaker.Cli -- build --dry-run
dotnet run --project src/DvdaMaker.Cli -- build
```

`prepare` 生成 `manifest.json` 和解码报告。`plan` 使用已有 MLP 计算分盘，不调用 `dvda-author` 或 `mkisofs`。

## 校验

```bash
dotnet run --project src/DvdaMaker.Cli -- verify quick
dotnet run --project src/DvdaMaker.Cli -- verify capacity
dotnet run --project src/DvdaMaker.Cli -- verify audit
dotnet run --project src/DvdaMaker.Cli -- verify menu
dotnet run --project src/DvdaMaker.Cli -- verify timeline
dotnet run --project src/DvdaMaker.Cli -- verify lossless
dotnet run --project src/DvdaMaker.Cli -- verify all
```

| 模式 | 检查内容 |
|---|---|
| `quick` | ISO、IFO、轨数、轨首 pack、cell 时间轴、静图引用 |
| `capacity` | ISO 是否超过配置容量 |
| `audit` | AOB 扇区、轨边界、PTS 与构建日志对账 |
| `menu` | AMG/ASVS 结构、按钮跳转与页面视觉内容 |
| `timeline` | 规划与成品时间轴 |
| `lossless` | 源音频与 MLP 解码结果 |
| `all` | 执行全部适用检查 |

指定输入：

```bash
dotnet run --project src/DvdaMaker.Cli -- quick-check \
  --iso-dir /path/to/iso \
  --manifest /path/to/manifest.json \
  --log /path/to/build.log

dotnet run --project src/DvdaMaker.Cli -- verify menu --iso /path/to/disc.iso
```

## ALAC 与 M4A

```bash
dotnet run --project src/DvdaMaker.Cli -- alac check input.m4a
dotnet run --project src/DvdaMaker.Cli -- alac repair input.m4a output.m4a

dotnet run --project src/DvdaMaker.Cli -- convert /path/to/music --dry-run
dotnet run --project src/DvdaMaker.Cli -- convert /path/to/music
dotnet run --project src/DvdaMaker.Cli -- convert input.m4a --in-place
```

转换器会检查 PCM、标签和封面；`--in-place` 仅在转换及校验成功后删除源文件。

## MLP、AOB 与 ISO 工具

```bash
dotnet run --project src/DvdaMaker.Cli -- mlp --check file.mlp
dotnet run --project src/DvdaMaker.Cli -- mlp --align file.mlp
dotnet run --project src/DvdaMaker.Cli -- aob-pts ATS_01_1.AOB
dotnet run --project src/DvdaMaker.Cli -- iso list disc.iso /AUDIO_TS
dotnet run --project src/DvdaMaker.Cli -- iso extract disc.iso /AUDIO_TS/ATS_01_1.AOB output.aob
```

# 工作目录

`DVDA_BUILD_DIR` 下的主要内容：

```text
manifest.json           音源清单
decode_report.txt       解码完整性报告
mlp/                    FFmpeg MLP 缓存
mlp_index.json          正式 MLP 索引与分盘计划
mlp_index-dryrun.json   预演索引，不覆盖正式索引
alacfix/                ALAC 修复副本
menu/                   菜单素材
build.log               正式构建日志
build-dryrun.log        预演日志
out/、tmp/、iso/        出盘临时目录
```

正式构建成功后会清理可重建的大型中间目录。调试时可以临时设置：

```bash
DVDA_KEEP_INTERMEDIATE=1 bash build.sh
DVDA_KEEP_TMP=1 bash build.sh
```

# 开发与测试

需要 .NET 10 SDK：

```bash
dotnet build DVD-Audio-Maker.sln
dotnet run --project tests/DvdaMaker.CompatibilityTests
```

真实样本测试：

```bash
dotnet run --project tests/DvdaMaker.CompatibilityTests -- \
  --real-fixtures \
  "/path/to/reference-discs" \
  "/path/to/external-mlp"
```

真实 ISO 和 MLP 不进入仓库；测试只保存可重复验证的结构、哈希和扇区基线。

# 常见问题

## 找不到 `dotnet`

源码运行需要 .NET 10 SDK。Windows 自包含发布包不需要另装 .NET。

## 找不到 `dvda-author` 或 `mkisofs`

先检查配置：

```bash
dotnet run --project src/DvdaMaker.Cli -- config --check
```

工具树不在默认 `/root/dvda-author-mlp8` 时设置：

```bash
DVDA_AUTHOR="/path/to/dvda-author-dev"
DVDA_MKISOFS="/path/to/mkisofs"
DVDA_AUTHOR_SRC="/path/to/dvda-author-tree"
```

`DVDA_AUTHOR_SRC` 的父目录还用于定位 `menu-bin`，启用菜单时不能遗漏。

## Windows 构建误用了 WSL Bash

不要运行 PATH 中来源不明的裸 `bash`。Windows 工具链应从 `build-all.bat` 启动；它会定位 MSYS2 的 `usr\bin\bash.exe` 并拒绝 WSL 环境。

## 菜单文字为空或中文显示成日文字形

确认发布包或 `menu-bin/fonts/` 中存在独立的 SC、JP、KR 字体。不要直接把 `.ttc` 交给 ImageMagick；它通常只使用 face 0，而 Noto Sans CJK 的 face 0 是日文字形。

## 构建很慢

- 将工作目录放在本地 ext4 或 NTFS，而不是 WSL 9P 路径
- 检查实时杀毒软件是否拖慢 `configure`、`make` 或大量短进程
- 保留并复用有效的 MLP 缓存
- 已有 SurCode MLP 时可以使用 `external` 模式

## `verify audit` 报缺少构建日志或轨道表

审计依赖正式构建产生的 `build.log`。`--dry-run` 写入独立的 `build-dryrun.log`，不会替代正式日志。

## WSL 启动时提示无法转换某个 Windows PATH

这通常是 Windows PATH 中存在当前不可访问的盘符或目录，与 DVD-Audio 构建本身无关。只要所需的 .NET、FFmpeg 和工具链路径可用即可。

# 文档

- [`docs/CSHARP-MIGRATION.md`](docs/CSHARP-MIGRATION.md)：C# 迁移状态与实现边界
- [`docs/DVDA-AUTHOR-CHANGES.md`](docs/DVDA-AUTHOR-CHANGES.md)：`dvda-author` 改动及依据
- [`docs/DVDA-AUTHOR-DISABLED.md`](docs/DVDA-AUTHOR-DISABLED.md)：试验过但未启用的改动
- [`docs/TROUBLESHOOTING.md`](docs/TROUBLESHOOTING.md)：历史问题、诊断与修复记录
- [`docs/LICENSING.md`](docs/LICENSING.md)：第三方组件及许可说明
- [`tools/win-build/README.md`](tools/win-build/README.md)：Windows 原生工具链构建

# 许可

本仓库代码按 [`GPL-3.0`](LICENSE) 发布。第三方源码、工具、字体、FFmpeg、`dvda-author`、`dvdauthor`、SurCode 及音频内容分别适用其各自许可；详情见 [`docs/LICENSING.md`](docs/LICENSING.md)。
