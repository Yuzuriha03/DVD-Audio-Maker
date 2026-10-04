# Windows x64 构建与打包

[简体中文](README.md) | [English](README.en.md)

默认分发 **Windows x64、GUI 单 EXE，不含 .NET 运行时**。用户安装 .NET 10 Desktop Runtime x64；构建使用 .NET 10 SDK。默认 `--onefile --framework-dependent`，不再默认捆绑运行时。目录诊断包使用 `--directory`；只有显式 `--directory --self-contained` 才包含 .NET。

普通 C# 构建与发布包组装复用已准备的 Windows 组件。只有维护原生库时才使用 Python、MSYS2/MinGW-w64、Make 和开发库；运行产品不需要这些构建工具。

## 打包输入

| 输入 | 默认位置或参数 | 内容 |
|---|---|---|
| GUI 媒体运行库 | `build/media-native-shared` 或 `--media-runtime` | shared 配置的 DLL 和 media-build.json |
| 图像运行库 | `build/image-native` | dvda-image.dll、image-build.json、XML 和 NOTICE.txt |
| 已改为内置图像的制盘程序 | `build/image-author-shared` 或 `--image-author` | dvda-author-dev.exe、author-build.json |
| 第三方工具与字体 | `--prebuilt` 或 DVDA_PREBUILT_DIR | menu-bin 的完整运行依赖 |
| 制盘素材 | `--source` 或 DVDA_SRC_TREE | menu/silence.wav、menu/activeheader |

Git 仓库不携带完整第三方工具包。`tools/dvda-author-mlp8` 是局部源码镜像，不能独立编译，也不保证提供所需素材。

预编译工具目录需要以下入口及其完整 DLL 依赖：

```text
dvda-author-dev.exe
# The final author and required DLLs come from --image-author.
```

字体可提供三份 NotoSansCJKsc/jp/kr-Regular.otf，或经过验证的 `fonts/DvdaNotoCJK-Regular.ttc`。打包时共享 SC/JP/KR 的相同字体表，保留全部字形和区域 face。图像库不再要求 magick.exe、convert.exe、mogrify.exe 或 identify.exe。

基础制盘工具必须已经包含本项目的 MLP、时间轴、UTF-8 和菜单修复。最终打包会用 `--image-author` 中的版本替换暂存目录里的 author；旧菜单 EXE 不再是输入要求，打包时从暂存区移除。

## 生成精简单 EXE

先按下文统一构建 FFmpeg、媒体接口和 author，再运行：

```bat
tools\win-build\build-all.cmd ^
  --source "D:\dev\winbuild\src" ^
  --prebuilt "D:\dev\winbuild\menu-bin" ^
  --media-runtime "build\media-native-shared" ^
  --image-author "build\image-author-shared"
```

默认输出目录为 `tools/win-build/release-onefile`：`DVD-Audio-Maker.exe` 只嵌入运行必需组件；README、运行说明、许可、`config.env.example` 和根目录的 `NOTICE-Image.txt`、`NOTICE-Menu.txt` 授权声明作为旁文件，合并生成 `DVD-Audio-Maker-v1.0-win-x64.zip`。不附带 .NET，示例配置不会自动载入。运行资源自动释放到用户缓存；托管 Debug、CLI 和 F5 调试仍按原方式工作。

| 参数 | 说明 |
|---|---|
| `--version` | ZIP 版本，默认 v1.0；后续例如 v1.1.0 |
| `--onefile` | 默认：GUI 单 EXE，要求 shared 原生构建 |
| `--directory` | 目录和 ZIP，便于开发及检查原生组件 |
| `--framework-dependent` | 默认：不捆绑 .NET |
| `--self-contained` | 仅目录模式：附带 .NET |
| `--include-cli` | 仅目录模式：增加开发 CLI |
| `--media-runtime` / `--image-author` | 已验证的媒体与 author 构建目录 |
| `--source` / `--prebuilt` | 制盘素材和字体来源 |
| `--output` / `--repo` | 输出目录和仓库根目录 |

目录模式默认仍从 build/media-native、build/image-author 读取输入；也可显式传入 shared 构建，得到与单 EXE 相同的单套 DLL 布局。发布物都放在忽略目录，通过 GitHub Releases 分发，不提交到 Git。设计及缓存行为见 [单文件发布](../../docs/ONEFILE-PUBLISH.md)。

## 复用组件进行 C# 调试

从与当前源码匹配的完整发布目录加载原生库，保留该目录自身的字体与配置结构。例如：

```bat
set "DVDA_MEDIA_NATIVE_DIR=D:\DVD-Audio-Maker\menu-bin"
set "DVDA_IMAGE_NATIVE_DIR=D:\DVD-Audio-Maker\image-native"
gui-debug.cmd
```

环境变量仅指定运行时库位置；打包仍要准备表中的构建输入。需要复制到构建目录时，保留媒体清单和图像配置/许可。特别注意：发布版 type.xml 的字体路径相对 image-native，不能只复制 DLL 或把相对字体配置放到缺少相邻 menu-bin/fonts 的调试目录。

原生编译输出默认位于 build 下；MSBuild 的 NativeMediaDirectory、NativeImageDirectory 属性可指定组件输入。GUI 只需配置 author 和素材路径，ISO 已由 author 内置写入器完成，不再需要 mkisofs 路径。详见 [开发调试](../../docs/DEVELOPMENT.md)。

## 按需重编译原生组件

需要 Windows x64、Python 3.12+ 和 MSYS2/MinGW-w64。媒体构建还使用 Make/GPG、固定 NASM 归档以及 libsoxr/zlib；图像构建使用 JPEG/PNG/WebP/zlib 静态开发库。源码版本、下载哈希、配置和依赖哈希由脚本/产物清单记录。

### 统一 FFmpeg 源码构建（单文件发布必需）

```bat
python tools\win-build\build-minimal-ffmpeg.py --profile shared --work-directory build\ffmpeg-shared --msys-root "D:\dev\msys64"
python tools\win-build\build-media-bridge.py --prefix build\ffmpeg-shared\install --output build\media-native-shared --msys-root "D:\dev\msys64"
python tools\win-build\build-image-author.py --source "D:\dev\winbuild\src" --ffmpeg-runtime build\ffmpeg-shared\install --work-directory build\image-author-shared --msys-root "D:\dev\msys64"
```

shared 配置合并媒体和菜单所需的 codec、parser、demuxer、muxer、swscale、swresample、SOXR 与 zlib，从同一份固定版本源码一次构建。媒体 C 接口和 author 都链接该前缀。打包检查两个清单及重名 DLL 哈希，统一放入 menu-bin；每个 DLL 只保留一份，GUI 与 author 共用。MSYS2 路径仅为示例，须包含匹配的 soxr/zlib 头文件与导入库；不修改或重装编译器。

下列分离配置仍供历史对照；默认单文件发布拒绝混用未统一的构建。

### GUI 媒体库

```bat
python tools\win-build\build-minimal-ffmpeg.py --msys-root "D:\dev\msys64" --work-directory build\ffmpeg-media --profile media
python tools\win-build\build-media-bridge.py --msys-root "D:\dev\msys64"
```

产物包含所需音频解码、SWR/SOXR、菜单视频读取及 C 接口，不分发 FFmpeg/FFprobe EXE。MLP 编码仍由独立MLP 编码核心完成。见 [内置媒体处理](../../docs/INPROCESS-MEDIA.md)。

### 图像库与原生制盘入口

```bat
python tools\win-build\build-image-runtime.py --msys-root "D:\dev\msys64"
python tools\win-build\build-image-bridge.py --msys-root "D:\dev\msys64"
python tools\win-build\build-minimal-ffmpeg.py --profile menu --work-directory build\ffmpeg-menu --msys-root "C:\msys64"
python tools\win-build\build-menu-runtime.py --msys-root "C:\msys64"
python tools\win-build\build-image-author.py --source "D:\dev\winbuild\src" --msys-root "D:\dev\msys64" --ffmpeg-runtime "build\ffmpeg-menu\install"
```

ImageMagick/FreeType 归档固定 SHA-256；脚本核对上游源文件，并重建项目补丁。图像库保留 Q16 HDRI、JPG/PNG 读写、WebP 读取、绘图/字幕/统计及必要字体功能，禁用外部 delegates 和动态 coder。只生成一份依赖 Windows 系统库的 x64 图像 DLL。

build-image-author.py 对已配置完整源码树建立隔离快照，复制当前镜像及 C 接口，链接 FFmpeg menu 配置并加入菜单模块。默认读取 build/ffmpeg-menu/install 和 build/menu-native，可用 --ffmpeg-runtime、--menu-runtime 指定。不改动原源码树或编译器，也不构建/启动 ffmpeg.exe。
生成目录中的 `dvda-author-dev.exe` 必须与 `author-build.json` 的 `runtime_files` 一起使用；脚本会按 PE 导入闭包收集这些 DLL，只复制 exe 并不完整。正式打包也会把这些 DLL 一并放入 `menu-bin`。

正式打包按已验证的 author 运行库清单保留 menu-bin 中的 DLL，移除旧工具包遗留、清单之外的 DLL。清理仅作用于新发布暂存目录，不修改 --prebuilt 输入、源码或编译工具链。

### 历史 MLP 专用配置

```bat
python tools\win-build\build-minimal-ffmpeg.py --msys-root "D:\dev\msys64"
```

旧 mlp 配置保留作对照和独立库维护；标准单文件发布的 author 和 GUI 媒体接口都必须使用 --profile shared；分离的 menu/media 仅供目录构建和历史对照，不可用 MLP-only 库替换。

## 验证与缓存维护

打包检查组件哈希、x64 架构和导入依赖，生成 MANIFEST.txt，并为对应运行模式填充中英文 README/RUNTIME。字体完整保留；ZIP 格式和压缩参数没有作为体积优化手段改变。

```bat
dotnet build DVD-Audio-Maker.sln -c Debug -p:SelfContained=false
tests\DvdaMaker.CompatibilityTests\bin\Debug\net10.0-windows\win-x64\DvdaMaker.CompatibilityTests.exe
```

迁移通过兼容性、菜单媒体、原生算法对照和全流损坏检查。GUI 进程树覆盖普通/索引菜单、无菜单、多盘多组及转换后的 PCM，不启动外部图像/媒体/菜单工具。见[完成记录](../../docs/menu-migration-validation.json)。

图像回归脚本 `test-image-release.py` 需要独立旧包、对应音源/基准输出及一个全新的输出目录。旧 ImageMagick 仅在测试的参考分支运行。输入要求与结果见 [验证记录](../../docs/inprocess-images-validation.json)。

本机保留的组件、基准和最新包详见本地 build/README.md；该文件与产物不随 Git 克隆出现。源码及个人配置须保留，bin/obj 和发布暂存区可再生；删除 build 下的原生组件会影响调试/打包，应保留可复用产物或准备重编译。

## GUI 与文档维护

GUI 仅提供检查音源、开始制作、验证成品。开发 CLI 保留 cli.cmd build --dry-run，会写编码缓存和独立索引，但不生成 ISO。旧 SurCode 导入选项已移除；旧配置值映射为通用外部 MLP 导入。

本目录 docs/README.md 与 docs/README.en.md 是发布包用户指南模板，只写安装、使用和排错，不放编译命令或开发验收报告。仓库根 README 另提供开发文档入口。打包前同步中英文模板，--version 必须与发布标签和文档中的附件名一致。

GUI 与用户说明支持中文、英文和日语。发布模板为 docs/README.md、docs/README.en.md、docs/README.ja.md；三语 README/RUNTIME 随 EXE 平铺发布。MLP 与 LPCM 为独立编码选项，LPCM 使用规范化整数 WAVE 和原生 DVD-Audio 封装，不调用 MLP 编码核心。原生修改使用 test-lpcm-native.py 检查全部支持格式、跨曲和短音轨边界。

## C17 格式运行库

`tools/formats-native/dvda-formats.c` 提供 MLP 流式检查与对齐、PCM 逐字节比较以及 PTS/MLP 格式解析。它不包含 MLP 编码器，也不引入第三方格式库。用 MSYS2 MinGW GCC 构建：

```bat
python tools\win-build\build-formats-runtime.py --msys-root "C:\msys64" --output build\formats-native
```

打包时使用 `--formats-runtime build\formats-native`（默认路径即为此目录）。C# 开发构建没有 DLL 时继续使用托管回退；发布包会把 DLL 放进单文件运行缓存。115 项兼容性测试可在设置 `DVDA_FORMATS_NATIVE_DIR` 后通过原生实现运行，逐字节对照使用测试程序的 `--native-format-samples <MLP目录>`。
