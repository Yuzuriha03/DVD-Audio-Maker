# DVD-Audio Maker

将 FLAC 或 ALAC/M4A 音源制作成标准 **DVD-Audio ISO** 的 Windows 原生工具链。

项目使用 **.NET 10 / C#** 实现音源准备、MLP 管理、分盘、菜单生成、ISO 发布和成品校验；底层使用经过修改的 `dvda-author` 与支持 `-dvd-audio` 的 `mkisofs`。

> 本仓库不包含音频、商业编码器或预编译第三方工具。使用者需要自行准备所需程序，并确认相关软件与内容的授权。

## 功能

- 递归扫描 FLAC 和 ALAC/M4A
- 读取专辑、曲名、曲序和日期标签
- 检查解码完整性、声道数和音频参数
- 修复特定 Apple ALAC 文件缺失 END 标记的问题
- 按专辑归一化采样率与位深
- 使用 FFmpeg、外部 MLP 或内置 SurCode 辅助工具取得 MLP
- 保持专辑完整并按容量逐盘填满
- 可选 DVD-Audio AMG 选曲菜单与 ASVS 播放封面
- 事务式发布整套 ISO
- 校验 ISO、IFO、AOB、PTS、菜单、容量和无损性
- Windows x64 自包含发布

## 项目结构

```text
src/DvdaMaker.Cli             命令行入口
src/DvdaMaker.Configuration   config.env 配置解析
src/DvdaMaker.Preparation     扫描、归一化、解码校验、ALAC 修复
src/DvdaMaker.Building        MLP、分盘、菜单、出盘、发布与校验
src/DvdaMaker.Formats         ISO9660、MLP、MPEG/PTS 解析
src/DvdaMaker.Processes       外部进程执行
src/DvdaMaker.SurcodeTool     并入 dvda.exe 的 Windows x64 SurCode 类库
src/DvdaMaker.FontTool        OpenType/TTC 字体工具
src/DvdaMaker.Toolchain       Windows 发布包组装器
tests/                        兼容性与端到端测试
```

仓库不再使用 WSL、PowerShell、Bash 或 `.sh` 脚本。日常入口是 `build.cmd`、`verify.cmd` 和 C# CLI。

## 前置条件

源码运行需要：

- Windows 10/11 x64
- .NET 10 SDK
- FFmpeg、FFprobe 和 Metaflac
- 已构建的 Windows 版 `dvda-author-dev.exe` 与 `mkisofs.exe`
- 启用菜单时所需的菜单工具、ImageMagick、字体和运行期素材

确认 FFmpeg 支持 MLP：

```bat
ffmpeg -hide_banner -encoders | findstr /i mlp
```

## 配置

仓库提供可提交的示例 `config.env`。

自动发现优先级：

```text
环境变量 > config.env > 内置默认值
```

也可以用 `--config` 或环境变量 `DVDA_CONFIG` 指定文件。

最小配置示例：

```text
DVDA_SRC="D:/Music/MyAlbums"
DVDA_FINAL_DIR="D:/DVD_Output"
DVDA_BUILD_DIR="D:/DVD_Output/_work"
DVDA_TITLE="My DVD-Audio"
DVDA_ISO_PREFIX="MyCollection"
DVDA_MAX_DISCS="2"
DVDA_MENU="off"
```

配置只解析 `KEY=VALUE`，不会执行命令或变量展开。路径应使用绝对 Windows 路径；正斜杠和反斜杠均可。

检查配置：

```bat
dotnet run --project src\DvdaMaker.Cli -- config
dotnet run --project src\DvdaMaker.Cli -- config --check
```

## 构建与校验

```bat
build.cmd --dry-run
build.cmd
verify.cmd
```

指定另一份配置：

```bat
build.cmd --dry-run --config "D:\Config\dvda.env"
build.cmd --config "D:\Config\dvda.env"
verify.cmd all --config "D:\Config\dvda.env"
```

也可直接调用 CLI：

```bat
dotnet run --project src\DvdaMaker.Cli -- prepare
dotnet run --project src\DvdaMaker.Cli -- plan
dotnet run --project src\DvdaMaker.Cli -- build --dry-run
dotnet run --project src\DvdaMaker.Cli -- build
dotnet run --project src\DvdaMaker.Cli -- verify all
```

## MLP 来源

### FFmpeg

```text
DVDA_MLP_SOURCE="ffmpeg"
DVDA_MLP_EXTERNAL_DIR=""
```

首次构建编码 MLP，之后复用工作目录中的有效缓存。

### 外部 MLP

```text
DVDA_MLP_SOURCE="external"
DVDA_MLP_EXTERNAL_DIR="D:/Music/MLP"
```

外部目录必须与音源目录同构：

```text
音源：D:/Music/MyAlbums/Album/01 Song.flac
MLP ：D:/Music/MLP/Album/01 Song.mlp
```

### SurCode 批量编码

```text
DVDA_MLP_SOURCE="surcode-batch"
DVDA_MLP_EXTERNAL_DIR="D:/Music/MLP"
DVDA_MLP_BATCH_TEMP_DIR="D:/dvda-surcode/temp"
DVDA_MLP_BATCH_OUTPUT_DIR="D:/dvda-surcode/output"
DVDA_MLP_SURCODE_EXE="C:/Tools/SurCode MLP/surcodemlp.exe"
DVDA_MLP_EAC3TO_EXE="C:/Tools/eac3to/eac3to.exe"
DVDA_MLP_SURCODE_SAMPLE_RATE="48000"
DVDA_MLP_SURCODE_BITS="24"
```

内置 Windows x64 辅助进程通过 JSON 任务调用 eac3to 和官方 SurCode，并将结果写入与音源同构的 MLP 缓存目录。

## 分盘与菜单

- 默认不拆散专辑，按全局曲序逐盘填满。
- `DVDA_MAX_DISCS` 只设置盘数上限，不参与切分。
- DVD-5 默认容量为 `4,707,319,808` 字节。
- DVD-9 可设置 `DVDA_DISC_BYTES="8540123136"`。
- 每个 DVD-Audio 音频组最多 99 轨。

菜单配置示例：

```text
DVDA_MENU="on"
DVDA_MENU_TRACKS_PER_PAGE="12"
DVDA_MENU_INDEX_MIN_ALBUMS="4"
DVDA_MENU_STILLPICS="on"
DVDA_MENU_COVER_DIM="35"
```

不要直接把 TTC 作为菜单字体；应使用 SC、JP、KR 三个独立 OTF face。

`DVDA_FINAL_DIR` 就是最终输出目录。构建成功后 ISO 会直接发布到该目录，不再执行额外复制。

## Windows 自包含发布包

仓库不再现场运行 Autotools、Make、MSYS2、Bash 或 WSL。第三方 C 工具必须事先构建并集中到 Windows 目录。

默认预编译目录：

```text
tools\win-build\prebuilt\
```

生成发布包：

```bat
tools\win-build\build-all.cmd
```

指定第三方工具与运行期素材树：

```bat
tools\win-build\build-all.cmd ^
  --prebuilt "D:\dev\winbuild\menu-bin" ^
  --source "D:\dev\winbuild\src"
```

产物位于：

```text
tools\win-build\release\DVD-Audio-Maker\
tools\win-build\release\DVD-Audio-Maker.zip
```

详细要求见 [`tools/win-build/README.md`](tools/win-build/README.md)。

## 开发与测试

```bat
dotnet build DVD-Audio-Maker.sln --configuration Release
dotnet run --project tests\DvdaMaker.CompatibilityTests --configuration Release
```

当前兼容测试基线为 73 项。

## 文档

- [`docs/CSHARP-MIGRATION.md`](docs/CSHARP-MIGRATION.md)：C# 迁移状态与实现边界
- [`docs/DVDA-AUTHOR-CHANGES.md`](docs/DVDA-AUTHOR-CHANGES.md)：`dvda-author` 改动及依据
- [`docs/DVDA-AUTHOR-DISABLED.md`](docs/DVDA-AUTHOR-DISABLED.md)：试验过但未启用的改动
- [`docs/TROUBLESHOOTING.md`](docs/TROUBLESHOOTING.md)：历史问题、诊断与修复记录
- [`docs/LICENSING.md`](docs/LICENSING.md)：第三方组件及许可说明

## 许可

本仓库代码按 [`GPL-3.0`](LICENSE) 发布。第三方源码、工具、字体、FFmpeg、`dvda-author`、`dvdauthor`、SurCode 及音频内容分别适用其各自许可。
