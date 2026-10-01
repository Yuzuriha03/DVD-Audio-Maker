# DVD-Audio Maker

## 图形界面（Windows x64）

开发工作区双击 `gui.cmd`；自包含发布包双击根目录的 `DVD-Audio-Maker.exe`。GUI 可选择目录、编辑编码与菜单选项，执行检查、预演、制作和验证；支持取消及实时日志。

使用“导入 config.env”读取旧配置，也可打开/保存 JSON 方案。日常配置自动保存至 `%LOCALAPPDATA%/DVD-Audio-Maker/settings.json`，无需手工修改 env；CLI 与 `--config` 保留。MLP 编码直接调用内嵌 x64 DLL，不启动编码 EXE。

界面按“开始设置 / 音频编码 / 光盘菜单 / 工具与高级”分组，高级参数默认收起；光盘容量可直接选择 DVD5、DVD9 或自定义。下方默认显示中文任务摘要、当前阶段、耗时与下一步提示，可以拖动分隔条调整日志区大小。

日志可切换为详细输出、只看提醒、暂停实时更新、复制当前内容或导出完整任务日志。正常的工具进度不会被当作任务失败；错误和警告仍会保留。完整任务记录自动保存在 `%LOCALAPPDATA%/DVD-Audio-Maker/logs`，窗口仅显示最近记录以限制内存占用。启动异常也会写入该目录。

实施方案和范围：[GUI 与 DLL 方案](docs/GUI-AND-DLL-PLAN.md)。


将 FLAC 或 ALAC/M4A 音源制作成标准 **DVD-Audio ISO** 的 Windows 原生工具链。

项目使用 **.NET 10 / C#** 实现音源准备、MLP 管理、分盘、菜单生成、ISO 发布和成品校验；底层使用经过修改的 `dvda-author` 与支持 `-dvd-audio` 的 `mkisofs`。

> 仓库内含本项目MLP 编码核心源码及固定版本 Windows 产物；不包含原版 SurCode、音频或其他外部工具。

## 功能

- 递归扫描 FLAC 和 ALAC/M4A
- 读取专辑、曲名、曲序和日期标签
- 检查解码完整性、声道数和音频参数
- 修复特定 Apple ALAC 文件缺失 END 标记的问题
- 按专辑归一化采样率与位深
- 使用MLP 编码核心或导入外部 MLP
- 保持专辑完整并按容量逐盘填满
- 可选 DVD-Audio AMG 选曲菜单与 ASVS 播放封面
- 事务式发布整套 ISO
- 校验 ISO、IFO、AOB、PTS、菜单、容量与首轨抽样无损性
- Windows x64 自包含发布

## 项目结构

```text
src/DvdaMaker.Desktop         Windows x64 图形入口
src/DvdaMaker.Cli             命令行入口
src/DvdaMaker.Configuration   GUI JSON 方案与 config.env 配置解析
src/DvdaMaker.Preparation     扫描、归一化、解码校验、ALAC 修复
src/DvdaMaker.Building        MLP、分盘、菜单、出盘、发布与校验
src/DvdaMaker.Formats         ISO9660、MLP、MPEG/PTS 解析
src/DvdaMaker.Processes       外部进程执行
src/DvdaMaker.SurcodeTool     FFmpeg PCM 准备与Windows MLP 核心
src/DvdaMaker.FontTool        OpenType/TTC 字体工具
src/DvdaMaker.Toolchain       Windows 发布包组装器
tests/                        兼容性与端到端测试
```

日常使用和发布包组装无需 WSL、PowerShell 或 Bash。图形入口是 `gui.cmd`（源码工作区）或 `DVD-Audio-Maker.exe`（发布包）；`build.cmd`、`verify.cmd` 和 C# CLI 继续保留。

## 前置条件

源码运行需要：

- Windows 10/11 x64
- .NET 10 SDK
- FFmpeg、FFprobe 和 Metaflac
- 已构建的 Windows 版 `dvda-author-dev.exe` 与 `mkisofs.exe`
- 启用菜单时所需的菜单工具、ImageMagick、字体和运行期素材

FFmpeg 保留用于音源转换、解码和校验，不再用于 MLP 编码。
批量编码使用 DVDA_FFMPEG 指定的 FFmpeg（默认在 PATH 查找）；无需 eac3to 或原版 SurCode。

## CLI 与旧 env 配置

GUI 配置可在界面编辑并保存为 JSON；以下优先级仅适用于 CLI。仓库提供可提交的示例 `config.env`。

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

`plan` 只读取现有 `manifest.json` 与 MLP 大小，不获取或编码 MLP，适合快速查看分盘结果。
`build --dry-run` 会执行完整的 MLP 获取流程并写独立预演索引，不是零写入操作。
`prepare --force` 忽略音源校验缓存，强制重新探测与解码校验。
`build --no-resume` 关闭逐盘续跑，强制重新出盘全部盘。

`verify lossless` 只抽样比对**第 1 盘 / 组 1 / 第 1 轨**：源解码 PCM 与 MLP 解码 PCM 逐字节比较，再从成品 ISO 抽出首轨 MLP 与源 MLP 做字节比较。默认要求等长；仅在 SurCode 模式下允许共同内容完全一致、末尾额外不足 1 ms 且由完整零采样帧构成的填充。截短、非零尾部或内容差异仍报错。它不是全盘逐轨无损验证。

## 重跑与缓存

| 配置 | 默认 | 作用 |
| --- | --- | --- |
| `DVDA_PREPARE_CACHE` | `on` | 音源探测与解码校验结论缓存，位于 `build/prepare-cache.json` |
| `DVDA_RESUME` | `on` | 逐盘续跑，记录位于 `build/publish-staging/resume.json` |
| `DVDA_MLP_JOBS` | `1` | MLP 编码核心的 MLP 编码并发路数（1～16） |
| `DVDA_KEEP_TMP` | `off` | 保留 `build/tmp` 供排查（开启后不自动清理） |
| `DVDA_KEEP_INTERMEDIATE` | `off` | 保留 author 输出与中间 ISO；**开启时逐盘续跑自动关闭** |

- 音源缓存只在文件身份（长度、修改时间、首尾各 64 KiB 哈希）与归一化参数完全一致时复用；未通过校验的轨道永不写入缓存。
- 逐盘续跑只在签名（源/MLP 身份、author/mkisofs 工具身份、影响输出的配置、菜单设置）一致且暂存 ISO 未被改动时跳过该盘；最终仍由整套事务发布 ISO 与索引。构建失败会保留 `build/publish-staging`，下次运行从那里续跑。
- `DVDA_MLP_JOBS` 大于 1 会让多个独立的进程内 DLL 编码状态并发工作，编码前的缓存凭据（源身份 + 编码器身份 + 编码参数 + 输出身份）依旧生效；是否提速取决于磁盘吞吐与 CPU，默认保持 1 路。

## MLP 来源

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

### MLP 编码核心批量编码（默认）

```text
DVDA_MLP_SOURCE="surcode-batch"
DVDA_MLP_EXTERNAL_DIR="D:/Music/MLP"
DVDA_MLP_BATCH_TEMP_DIR="D:/dvda-surcode/temp"
DVDA_MLP_BATCH_OUTPUT_DIR="D:/dvda-surcode/output"
DVDA_MLP_METADATA_CONTEXT=""
DVDA_FFMPEG="C:/Tools/ffmpeg/bin/ffmpeg.exe"
DVDA_MLP_SURCODE_SAMPLE_RATE="48000"
DVDA_MLP_SURCODE_BITS="24"
```

保留 `surcode-batch` 配置名兼容现有任务，执行链为：

`源音频 → FFmpeg → 整数 PCM → MLP 核心 DLL → MLP 缓存`。

原生 x64 主程序内嵌固定哈希的 Windows x64 编码 DLL，运行时提取、校验并在进程内调用；
不启动原版 SurCode，不加载它的 DLL，不写 SSF，也不进行编码后的字节修补。
旧配置中的 DVDA_MLP_EAC3TO_EXE 仍可读取但不再使用；在 GUI 的“音频编码”页设置 FFmpeg 即可。重采样/降位深采用 FFmpeg 的转换结果，不承诺复现 eac3to 的处理字节；相同目标 PCM 和元数据仍要求 MLP 逐字节相同。迁移方案与验收见 [FFmpeg PCM 迁移](docs/FFMPEG-PCM-MIGRATION.md)。

MLP 缓存目录留空时使用 `<build>/mlp`。旧的 `DVDA_MLP_SOURCE=ffmpeg` 会明确报错，
请改为 `surcode-batch`；`DVDA_MLP_SURCODE_EXE` 已删除。

默认使用固定空辅助 TLV，使相同 PCM/设置的输出可重复。需要与历史原版文件
逐字节比较时，可指定该音轨完整的 `DVDA_MLP_METADATA_CONTEXT`；时间元数据不同
会导致整文件不同，不能只靠相同音频推断字节相同。不会将参考音频载荷传入编码器。
旧的无来源凭据缓存会重新生成；失败的重新编码不覆盖已有的有效产物。

接口、精度、声道、缓存及复现说明见 [MLP 编码核心集成](docs/MLP-ENCODER.md)。

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

当前兼容测试基线为 100 项。

## 文档

- [MLP 编码核心 MLP 编码与验证结果](docs/MLP-ENCODER.md)：新批量调用链、元数据配置与逐字节对照范围

- [`docs/CSHARP-MIGRATION.md`](docs/CSHARP-MIGRATION.md)：C# 迁移状态与实现边界
- [`docs/DVDA-AUTHOR-CHANGES.md`](docs/DVDA-AUTHOR-CHANGES.md)：`dvda-author` 改动及依据
- [`docs/DVDA-AUTHOR-DISABLED.md`](docs/DVDA-AUTHOR-DISABLED.md)：试验过但未启用的改动
- [`docs/TROUBLESHOOTING.md`](docs/TROUBLESHOOTING.md)：历史问题、诊断与修复记录
- [`docs/LICENSING.md`](docs/LICENSING.md)：第三方组件及许可说明

## 许可

本仓库代码按 [`GPL-3.0`](LICENSE) 发布。第三方源码、工具、字体、FFmpeg、`dvda-author`、`dvdauthor`、SurCode 及音频内容分别适用其各自许可。
