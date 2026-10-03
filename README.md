# DVD-Audio Maker

[简体中文](README.md) | [English](README.en.md)

将 FLAC、ALAC/M4A 音源制作成 **DVD-Audio ISO** 的 Windows x64 工具，提供中英文图形界面、MLP 编码、自动分盘、可选菜单和成品校验。

当前默认发布 **Windows x64 GUI 单 EXE，不包含 .NET 运行时**。首次使用请安装 [.NET 10 Desktop Runtime（Windows x64）](https://dotnet.microsoft.com/download/dotnet/10.0)，然后直接运行 `DVD-Audio-Maker.exe`。普通 .NET Runtime、ASP.NET Core Runtime 或 .NET Framework 4.x 不能单独替代桌面运行时。历史目录包仍须完整解压。

## 快速开始

1. 选择音源目录、工作目录和 ISO 成品目录，填写标题与容量。
2. 设置目标采样率、位深及菜单。建议每张专辑单独一个目录，保留 album、title、track、date 标签。
3. 运行“检查音源”，再运行“预演制作”。预演会准备音源并编码 MLP，但不生成 ISO。
4. 运行“开始制作”，完成后执行“验证成品”，检查全部轨道、菜单和 ISO。

发布 ZIP 包含单文件 EXE，以及独立的使用说明、配置示例、许可和组件授权声明。EXE 只内嵌运行必需组件与完整中日韩字体，首次启动释放到 `%LOCALAPPDATA%/DVD-Audio-Maker/runtime`，之后校验并复用缓存。媒体处理和制盘共用同一次源码构建的 FFmpeg 库，每个 DLL 只有一份。关闭所有实例后可以清理此缓存，下次会自动恢复。请保留随包授权声明；`config.env.example` 不会自动载入，仍可导入已有 `config.env`。详见 [单文件构建](docs/ONEFILE-PUBLISH.md)。

## 配置、语言与日志

GUI 替代手工编辑 env：可导入旧 `config.env`，也可打开和保存 JSON 方案。日常设置保存在 `%LOCALAPPDATA%/DVD-Audio-Maker/settings.json`；CLI 与 `--config` 仍可使用。

右上角可切换“中文 / English”；界面和任务日志同步切换，路径、曲目标签、配置值及编码数据不变。首次按 Windows 界面语言选择，任务执行时暂时禁用切换。GUI 和 CLI 支持 `--language en`、`--language zh-CN`、`--language auto`，也支持 `DVDA_LANGUAGE`。

日志默认显示阶段摘要，可切换详细输出、只看提醒、暂停显示、复制或导出。任务可取消。完整日志及启动错误保存在 `%LOCALAPPDATA%/DVD-Audio-Maker/logs`。

## 编码与工具

| 工作 | 当前实现 |
|---|---|
| 音源读取、转换、解码和媒体校验 | 进程内 x64 媒体库，不启动 FFmpeg/FFprobe EXE |
| MLP 编码 | 内嵌MLP 编码核心 DLL，不启动原版 SurCode，也不调用 eac3to |
| 封面、文字、菜单图像、字体及图像校验 | GUI 与原生制盘程序各自在进程内调用精简图像 DLL |
| 菜单编码、复用、制盘及 ISO 生成 | 项目 author 内置 C 菜单模块和 ISO 写入器 |
| 可选 M4A/ALAC 转 FLAC 整理 | 进程内 FLAC 元数据编辑器处理封面与标签 |

无需另装 FFmpeg、FFprobe 或 ImageMagick。GUI 导入的旧媒体工具路径会自动改用内置组件；开发 CLI 和参考测试仍允许显式外部转换器。图像库支持 JPG/PNG 读写和 WebP 封面读取，保留完整 SC/JP/KR 字体 face；未包含通用视频、PDF/SVG 等图像委托链。

批量接口支持 44.1、48、88.2、96 kHz 的 1～6 声道，以及 176.4、192 kHz 的单/双声道，目标位深为 16、20、24 bit。声道布局沿用输入；混合采样率/位深声道组尚未开放为 GUI 配置。特定高噪声素材仍可能超出可用 MLP 码流限制，不会通过有损处理强行通过。

MLP 逐字节一致要求 **目标 PCM、编码参数和辅助元数据上下文均一致**。编码后不打补丁；相同音频但元数据不同的历史文件不能直接宣称整文件相同。详见 [MLP 编码核心集成](docs/MLP-ENCODER.md) 和 [当前媒体处理](docs/INPROCESS-MEDIA.md)。

## 源码开发

源码使用 .NET 10 / C#，GUI、CLI 与原生编码核心均为 Windows x64。开发需要 .NET 10 SDK，以及与当前源码匹配的内置库、制盘工具和素材；这些外部产物不完整包含在 Git 仓库中。

```bat
gui-debug.cmd
cli.cmd config
dotnet build DVD-Audio-Maker.sln -c Debug -p:SelfContained=false
```

VS Code 保留 GUI、CLI 和兼容性测试的 F5 配置。普通 C# 修改可复用已经验证的原生组件；维护原生依赖时才需要 Python、MSYS2/MinGW-w64 等工具。详见 [开发调试](docs/DEVELOPMENT.md) 与 [Windows 构建说明](tools/win-build/README.md)。

| 目录 | 用途 |
|---|---|
| `src/DvdaMaker.Desktop` / `Cli` | 图形入口与开发命令行 |
| `src/DvdaMaker.Configuration` / `Localization` | 配置、方案与中英文资源 |
| `src/DvdaMaker.Preparation` / `Building` | 音源准备、MLP、分盘、菜单及成品验证 |
| `src/DvdaMaker.Processes` | 内置媒体/图像接口与其余外部进程管理 |
| `src/DvdaMaker.SurcodeTool/Native` | 固定版本的MLP 编码核心源码与 x64 DLL |
| `src/DvdaMaker.Formats` / `FontTool` | 码流解析与共享字体工具 |
| `src/DvdaMaker.Toolchain` / `tools/win-build` | 原生构建、打包和回归脚本 |
| `tests` / `docs` | 兼容性测试、设计与验证记录 |
| `build` | 被 Git 忽略的本地组件、发布包、回归材料和缓存 |

## CLI 与旧 env 配置

GUI 配置可在界面编辑并保存为 JSON；以下优先级仅适用于 CLI。仓库提供可提交的示例 `config.env`。可通过“导入旧配置”或 `--config` 选择已有配置。

配置值优先级：

```text
环境变量 > 所选配置文件 > 内置默认值
```

配置文件按 `--config`、`DVDA_CONFIG`、`config.env` 的顺序选择；未找到配置文件时使用内置默认值。

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

verify lossless 逐盘、逐组、逐轨比较全部目标 PCM，并由只读 C 解析器核对成品 ISO 所有 AOB 分段中的全部 MLP 字节。batch-surcode 按编码时相同的 SWR、位深转换和 WAV 规范化重建目标 PCM；默认等长，SurCode 仅容许不足 1 ms 的完整零采样帧尾部填充。截短、非零尾部和内容差异仍失败；转换策略未知的旧外部 MLP 不用采样数相近代替一致性证明。验证不修改编码文件。

## 重跑与缓存

| 配置 | 默认 | 作用 |
| --- | --- | --- |
| `DVDA_PREPARE_CACHE` | `on` | 音源探测与解码校验结论缓存，位于 `<DVDA_BUILD_DIR>/prepare-cache.json` |
| `DVDA_RESUME` | `on` | 逐盘续跑，记录位于 `<DVDA_BUILD_DIR>/publish-staging/resume.json` |
| `DVDA_MLP_JOBS` | `1` | MLP 编码核心的 MLP 编码并发路数（1～16） |
| `DVDA_KEEP_TMP` | `off` | 保留 `<DVDA_BUILD_DIR>/tmp` 供排查（开启后不自动清理） |
| `DVDA_KEEP_INTERMEDIATE` | `off` | 保留 author 输出与中间 ISO；**开启时逐盘续跑自动关闭** |

- 音源缓存只在文件身份（长度、修改时间、首尾各 64 KiB 哈希）与归一化参数完全一致时复用；未通过校验的轨道永不写入缓存。
- 逐盘续跑只在签名（源/MLP 身份、author/ISO 写入器身份、影响输出的配置、菜单设置）一致且暂存 ISO 未被改动时跳过该盘；最终仍由整套事务发布 ISO 与索引。构建失败会保留 `<DVDA_BUILD_DIR>/publish-staging`，下次运行从那里续跑。
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
DVDA_FFMPEG="builtin:media"
DVDA_FFPROBE="builtin:probe"
DVDA_MLP_SURCODE_SAMPLE_RATE="48000"
DVDA_MLP_SURCODE_BITS="24"
```

保留 `surcode-batch` 配置名兼容现有任务，执行链为：

`源音频 → 内置媒体 DLL → 整数 PCM → MLP 核心 DLL → MLP 缓存`。

Windows x64 应用内嵌固定哈希的 Windows x64 编码 DLL，运行时提取、校验并在进程内调用；
不启动原版 SurCode，不加载它的 DLL，不写 SSF，也不进行编码后的字节修补。
旧配置中的 DVDA_MLP_EAC3TO_EXE 仍可读取但不再使用。GUI 自动使用内置媒体库，旧 FFmpeg / FFprobe 路径不会继续启动外部程序。重采样/降位深沿用已验证的 SWR 与 20 位量化行为，不承诺复现 eac3to 的处理字节；相同目标 PCM 和元数据仍要求 MLP 逐字节相同。当前方案与验收见 [内置媒体处理](docs/INPROCESS-MEDIA.md)，早期迁移记录见 [FFmpeg PCM 迁移](docs/FFMPEG-PCM-MIGRATION.md)。

MLP 缓存目录留空时使用 `<DVDA_BUILD_DIR>/mlp`。旧的 `DVDA_MLP_SOURCE=ffmpeg` 会明确报错，
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

随包 TTC 通过 type.xml 分别注册 SC、JP、KR 三个 face，GUI 自动选择区域字体。自定义字体请填写已注册的区域字体名称，或分别提供对应 OTF 文件。

`DVDA_FINAL_DIR` 就是最终输出目录。构建成功后 ISO 会直接发布到该目录，不再执行额外复制。

## 生成精简发布包

先按 [构建说明](tools/win-build/README.md) 准备内置媒体库、图像库、重编译的制盘程序及运行期素材，再执行：

```bat
tools\win-build\build-all.cmd ^
  --framework-dependent ^
  --prebuilt "D:\dev\winbuild\menu-bin" ^
  --source "D:\dev\winbuild\src"
```

上述路径是示例，须替换为已准备的 Windows 工具和完整素材树。默认输出到 `tools/win-build/release-framework-dependent/DVD-Audio-Maker` 及同级 ZIP；`--output` 可指定其他位置。标准包只含 GUI，`--include-cli` 才增加开发诊断入口。省略 `--framework-dependent` 仍可生成自包含包，但会附带 .NET 并显著增大体积。

打包本身复用现成 Windows 产物，不现场运行原生编译链。发布目录、音源、缓存和 ZIP 不提交到 Git；可分发的压缩包放入 GitHub Releases。

## 验证与维护

```bat
dotnet build DVD-Audio-Maker.sln -c Debug -p:SelfContained=false
tests\DvdaMaker.CompatibilityTests\bin\Debug\net10.0-windows\win-x64\DvdaMaker.CompatibilityTests.exe
```

迁移回归通过 109/109 兼容性检查、41 项菜单媒体检查、32 项菜单模块检查和 14 项全流损坏检查。GUI 覆盖普通/索引菜单、多盘多组及 44.1 kHz／20 位转换。菜单模块由源码构建并取代旧菜单 EXE；MLP 核心及整文件字节一致性不变，MPEG-2 菜单不要求与旧编码器输出逐字节相同。

本地验收精简包位于 build/release-menu-final，产物保持忽略。范围见[迁移清单](docs/NO-EXTERNAL-RUNTIME-MIGRATION.md)，结果见[验证记录](docs/menu-migration-validation.json)。历史基准仅用于开发对照。

## 文档

- [开发调试](docs/DEVELOPMENT.md)、[原生构建与打包](tools/win-build/README.md)
- [MLP 编码核心与字节一致性](docs/MLP-ENCODER.md)、[原生核心维护](src/DvdaMaker.SurcodeTool/Native/README.md)
- [内置媒体处理](docs/INPROCESS-MEDIA.md)、[内置图像处理](docs/INPROCESS-IMAGES.md)
- [精简媒体库](docs/MINIMAL-FFMPEG.md)、[共享字体](docs/SHARED-FONTS.md)
- [制盘源码改动](docs/DVDA-AUTHOR-CHANGES.md)、[历史故障记录](docs/TROUBLESHOOTING.md)
- [图像迁移验证记录](docs/inprocess-images-validation.json)、[第三方许可](docs/LICENSING.md)

## 许可

本仓库代码按 [GPL-3.0](LICENSE) 发布。第三方源码、工具、字体及音频内容分别适用其各自许可；原版 SurCode 不随项目提供。
