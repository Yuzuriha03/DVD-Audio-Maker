# Windows x64 构建与打包

[简体中文](README.md) | [English](README.en.md)

当前分发方案是 **GUI-only、Windows x64、不含 .NET 的精简包**。运行端安装 .NET 10 Desktop Runtime x64；开发和打包端使用 .NET 10 SDK。打包命令必须带 `--framework-dependent`；省略该参数仍生成自包含包。

普通 C# 构建与发布包组装复用已准备的 Windows 组件。只有维护原生库时才使用 Python、MSYS2/MinGW-w64、Make 和开发库；运行产品不需要这些构建工具。

## 打包输入

| 输入 | 默认位置或参数 | 内容 |
|---|---|---|
| GUI 媒体运行库 | `build/media-native` | DLL 和 media-build.json |
| 图像运行库 | `build/image-native` | dvda-image.dll、image-build.json、XML 和 NOTICE.txt |
| 已改为内置图像的制盘程序 | `build/image-author` 或 `--image-author` | dvda-author-dev.exe、author-build.json |
| 第三方工具与字体 | `--prebuilt` 或 DVDA_PREBUILT_DIR | menu-bin 的完整运行依赖 |
| 制盘素材 | `--source` 或 DVDA_SRC_TREE | menu/silence.wav、menu/activeheader |

Git 仓库不携带完整第三方工具包。`tools/dvda-author-mlp8` 是局部源码镜像，不能独立编译，也不保证提供所需素材。

预编译工具目录需要以下入口及其完整 DLL 依赖：

```text
dvda-author-dev.exe   mkisofs.exe      dvdauthor.exe
spumux.exe            spuunmux.exe     jpeg2yuv.exe
mpeg2enc.exe          mplex.exe        mp2enc.exe
```

字体可提供三份 NotoSansCJKsc/jp/kr-Regular.otf，或经过验证的 `fonts/DvdaNotoCJK-Regular.ttc`。打包时共享 SC/JP/KR 的相同字体表，保留全部字形和区域 face。图像库不再要求 magick.exe、convert.exe、mogrify.exe 或 identify.exe。

基础制盘工具必须已经包含本项目的 MLP、时间轴、UTF-8 和菜单修复。最终打包会用 `--image-author` 中的版本替换暂存目录里的 author；其余菜单工具仍需相应的 AMGM/跳转支持。

## 生成精简包

在仓库根目录运行，示例路径须替换为本机准备的目录：

```bat
tools\win-build\build-all.cmd ^
  --framework-dependent ^
  --source "D:\dev\winbuild\src" ^
  --prebuilt "D:\dev\winbuild\menu-bin" ^
  --image-author "build\image-author"
```

| 参数 | 说明 |
|---|---|
| `--repo` | 仓库根目录，通常自动识别 |
| `--source` | 完整运行期素材树；此打包步骤不编译该源码树 |
| `--prebuilt` | Windows 工具与字体目录，默认 tools/win-build/prebuilt |
| `--image-author` | 已重编译 author 的目录，默认 build/image-author |
| `--output` | 发布输出根目录 |
| `--framework-dependent` | 不打包 .NET；当前推荐分发模式 |
| `--include-cli` | 可选开发诊断包，附加 CLI 及说明 |
| `--ffmpeg-libraries` | 可选：已验证的 MLP 专用制盘库替换目录 |

默认精简输出为：

```text
tools/win-build/release-framework-dependent/
├── DVD-Audio-Maker/
│   ├── DVD-Audio-Maker.exe
│   ├── 应用 DLL、JSON、语言资源
│   ├── media-native/
│   ├── image-native/
│   ├── menu-bin/fonts/
│   ├── data/menu/
│   ├── config.env、MANIFEST.txt
│   └── README、RUNTIME、THIRD-PARTY、LICENSE
└── DVD-Audio-Maker.zip
```

不指定 `--framework-dependent` 时输出默认改为 tools/win-build/release，并附带 .NET。普通包没有 dvda.exe/dvda.cmd；开发工作区的 cli.cmd 和 VS Code 调试能力不受影响。产物放在忽略目录，ZIP 通过 GitHub Releases 分发，不提交到 Git。

## 复用组件进行 C# 调试

从与当前源码匹配的完整发布目录加载原生库，保留该目录自身的字体与配置结构。例如：

```bat
set "DVDA_MEDIA_NATIVE_DIR=D:\DVD-Audio-Maker\media-native"
set "DVDA_IMAGE_NATIVE_DIR=D:\DVD-Audio-Maker\image-native"
gui-debug.cmd
```

环境变量仅指定运行时库位置；打包仍要准备表中的构建输入。需要复制到构建目录时，保留媒体清单和图像配置/许可。特别注意：发布版 type.xml 的字体路径相对 image-native，不能只复制 DLL 或把相对字体配置放到缺少相邻 menu-bin/fonts 的调试目录。

原生编译输出默认位于 build 下；MSBuild 的 NativeMediaDirectory、NativeImageDirectory 属性可指定组件输入。GUI 的 author、mkisofs 和素材路径另在设置中指向有效工具包。详见 [开发调试](../../docs/DEVELOPMENT.md)。

## 按需重编译原生组件

需要 Windows x64、Python 3.12+ 和 MSYS2/MinGW-w64。媒体构建还使用 Make/GPG、固定 NASM 归档以及 libsoxr/zlib；图像构建使用 JPEG/PNG/WebP/zlib 静态开发库。源码版本、下载哈希、配置和依赖哈希由脚本/产物清单记录。

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
python tools\win-build\build-image-author.py --source "D:\dev\winbuild\src" --msys-root "D:\dev\msys64"
```

ImageMagick/FreeType 归档固定 SHA-256；脚本核对上游源文件，并重建项目补丁。图像库保留 Q16 HDRI、JPG/PNG 读写、WebP 读取、绘图/字幕/统计及必要字体功能，禁用外部 delegates 和动态 coder。只生成一份依赖 Windows 系统库的 x64 图像 DLL。

`build-image-author.py` 从已经应用基础项目改动的完整工作树创建独立快照，再转换图像调用；不要事先手工应用同一图像增量补丁。它不覆盖原始工作树或编译器安装。详见 [源码镜像边界](../dvda-author-mlp8/README.md) 与 [内置图像处理](../../docs/INPROCESS-IMAGES.md)。旧 magick-shim 仅用于历史参考测试，当前发布不使用它。

### MLP 专用制盘解码库（独立维护）

```bat
python tools\win-build\build-minimal-ffmpeg.py --msys-root "D:\dev\msys64"
```

默认 mlp 配置服务 menu-bin 中的制盘工具，与 GUI 的 media 配置及编码核心分开。需要替换时传 `--ffmpeg-libraries build\ffmpeg-minimal\install\bin`。打包器依据已验证配置检查哈希并清理闲置依赖；编译器或配置变化后先完成回归，再更新指纹。见 [精简媒体库记录](../../docs/MINIMAL-FFMPEG.md)。

## 验证与缓存维护

打包检查组件哈希、x64 架构和导入依赖，生成 MANIFEST.txt，并为对应运行模式填充中英文 README/RUNTIME。字体完整保留；ZIP 格式和压缩参数没有作为体积优化手段改变。

```bat
dotnet build DVD-Audio-Maker.sln -c Debug -p:SelfContained=false
tests\DvdaMaker.CompatibilityTests\bin\Debug\net10.0-windows\win-x64\DvdaMaker.CompatibilityTests.exe
```

当前记录为 108/108 兼容性检查和 23/23 图像专项通过。完整 GUI 流程的进程树检查覆盖普通、单页及多页索引，不启动外部 ImageMagick/FFmpeg/FFprobe/SurCode；必要的菜单编码、复用及 ISO 工具仍会启动。

图像回归脚本 `test-image-release.py` 需要独立旧包、对应音源/基准输出及一个全新的输出目录。旧 ImageMagick 仅在测试的参考分支运行。输入要求与结果见 [验证记录](../../docs/inprocess-images-validation.json)。

本机保留的组件、基准和最新包详见本地 build/README.md；该文件与产物不随 Git 克隆出现。源码及个人配置须保留，bin/obj 和发布暂存区可再生；删除 build 下的原生组件会影响调试/打包，应保留可复用产物或准备重编译。
