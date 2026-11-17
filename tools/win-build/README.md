# Windows x64/ARM64 构建与发布

当前产品是 Windows x64 与 Windows on ARM64（WoA）Rust 原生 GUI，运行时不需要 .NET。重建原生组件需要 Rust、Visual Studio C++ x64 工具、Windows SDK、Python 和原生 Windows CMake。FFmpeg 的 configure/make 使用 MSYS2 shell，但 C/C++ 编译和链接全部使用 MSVC。

## Rust 构建

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File tools\win-build\cargo-msvc.ps1 build --manifest-path rust\Cargo.toml --target x86_64-pc-windows-msvc --workspace --offline
powershell -NoProfile -ExecutionPolicy Bypass -File tools\win-build\cargo-msvc.ps1 fmt --manifest-path rust\Cargo.toml --all -- --check
powershell -NoProfile -ExecutionPolicy Bypass -File tools\win-build\cargo-msvc.ps1 clippy --manifest-path rust\Cargo.toml --target x86_64-pc-windows-msvc --workspace --offline -- -D warnings
powershell -NoProfile -ExecutionPolicy Bypass -File tools\win-build\cargo-msvc.ps1 test --manifest-path rust\Cargo.toml --target x86_64-pc-windows-msvc --workspace --offline -- --include-ignored
```

`cargo-msvc.ps1` 初始化 MSVC 环境并默认设置 `DVDA_FFMPEG_PREFIX=build\msvc-native\ffmpeg-static\install`，上述命令可在干净 shell 执行（先按下文准备 Native inputs）。不要在没有 MSVC 环境的 shell 中直接调用裸 `cargo`，否则会因缺少 `link.exe` 或 FFmpeg 前缀失败。

`gui.cmd`、`gui-debug.cmd`、`cli.cmd` 和 `build.cmd` 都调用 Rust workspace。配置只使用 JSON 方案和 `--profile`；不支持 `config.env`、`DVDA_CONFIG` 和 `--config`。

完整 author 由 Rust 实现：应用进程内调用，独立 Rust CLI 复用同一流程。`build-image-author.py` 是 `build-rust-author.py` 的兼容入口，旧 C producer、源码和冻结二进制已清理。AOB、管理表、菜单/静图和 ISO 的验收记录保存在本地源码工作副本中，黄金数据的历史来源摘要仍保留。

## 原生输入

生产构建默认使用 `build/msvc-native/ffmpeg-static/install`。使用
`build-msvc-ffmpeg.py --profile static --work-directory build/msvc-native/ffmpeg-static`
生成 MSVC 静态库，并通过 `--msys-root`、`--cmake` 指定本机工具路径。
五个 FFmpeg 库、soxr 和 C 运行库静态链接到 EXE；内嵌资源只需解压字体和菜单素材。
FFmpeg 显式禁用原生 zlib；普通 M4A/ALAC 不受影响，但不再支持带压缩 `cmov` atom 的 MOV/M4A 文件。
请在新的工作目录重建 FFmpeg，勿复用旧的启用 zlib 的库。Rust PNG 编解码不受影响。
开发对照仍可使用 shared 构建；打包 shared/legacy 媒体运行时时必须显式传 `--ffmpeg-prefix <已验证的 shared FFmpeg 前缀>`，不能依赖环境变量顶替已验证输入。静态链接不代表已经测得编码提速。

打包器需要已验证的 x64 媒体/图像构建记录与 `build/rust-author-production`，并验证摘要、大小、PE 架构、DLL 闭包及字体/菜单素材。图像记录必须标识 Rust 实现；本机验收使用 `build/rust-image-runtime`。MLP 编码器直接编入 Rust 应用，不分发编码器 DLL；旧 `--encoder-runtime` 外部对照验证入口已移除。FFmpeg 命令行程序不是运行时输入，媒体库由 Rust bridge 调用；图像直接使用 Rust 实现，媒体/图像开发 adapter DLL 不进入静态链接发布包。

格式解析、MLP CRC/奇偶校验、PCM 比较及只读 AOB 成品校验已经内置为 Rust 实现，不需要格式或校验 DLL。打包不再接受 `--formats-runtime` 输入。

### Rust 图像链路

图像解码、缩放、合成、文字排版及 Y4M 输出由 `dvda-image` 实现，不需要 ImageMagick、MagickCore/Wand、委托库或 XML 配置。菜单外部封面支持 JPEG、PNG 和 WebP；嵌入封面另支持 BMP、GIF、TIFF 的尺寸及原始位深读取，原图字节保持不变。后者不扩展菜单外部封面的文件搜索规则。

图像依赖关闭默认 features，仅启用所需格式；GIF/TIFF 解码使用 Rust crates，不增加 C/C++ 图像构建步骤。构建 author 时只需 FFmpeg 和字体/菜单素材输入。旧 `--magick-work`、`--image-dependencies` 参数会明确报错，不会自动恢复旧后端。

外部图像 oracle 与 `-ImageRuntime` 差分入口已移除；当前测试使用 Rust 图像实现。ARM64 使用 Rust 对应 target，仍需匹配架构的 FFmpeg 与 MSVC 工具；交叉编译通过不等于 ARM64 真机验收。

## 应用图标

全平台唯一图标母版为 [PNG](../../assets/dvd-audio-maker.png)（透明背景栅格母版）。[ICO](../../assets/dvd-audio-maker.ico) 由 `generate-app-icon.ps1` 从该 PNG 生成（16、20、24、32、40、48、64、96、128、256 像素）；Android launcher mipmap 与通知栏小图标均由 `tools/android/generate-icons.py` 从同一 PNG 生成。[SVG](../../assets/dvd-audio-maker.svg) 仅为同一设计的矢量预览，不作为构建输入。更新图标时只修改 PNG，再重新生成派生资源：

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File tools\win-build\generate-app-icon.ps1
python tools/android/generate-icons.py
```

桌面构建将 ICO 嵌入 EXE 的 Windows 资源，主窗口也加载同一资源，因此资源管理器、任务栏和发布单 EXE 使用同一图标，无需旁置图标文件。打包记录包含图标素材摘要。标识使用普通项目名称文字，不是官方格式或认证 Logo，也不代表取得商标授权。

Android 只从同一 PNG 派生两类图标：launcher mipmap（48/72/96/144/192 像素）和 24dp 通知栏小图标 `ic_stat_dvda.png`（24/36/48/72/96 像素，白色 alpha 蒙版）。这 10 个派生 PNG 必须随仓库提交；`tools/android/build-flutter.ps1` 会在构建前拒绝缺失的密度，`tools/android/release-metadata.ps1` 会把母版与全部派生 PNG 的 SHA-256（以及是否已被 Git 跟踪）写入 `release-build.json`。

## 发布（单 EXE）

Windows 只发布单 EXE，按架构各产出一个；不生成也不上传 ZIP。先构建 x64 工具链（x64 与 WoA 共用同一个 x64 构建工具）：

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File tools\win-build\cargo-msvc.ps1 build --manifest-path rust\Cargo.toml --target x86_64-pc-windows-msvc --release --offline -p dvda-toolchain
```

包装脚本初始化 MSVC 环境并默认设置 `DVDA_FFMPEG_PREFIX`，可在干净 shell 执行（需先准备 Native inputs）。打包用的 `dvda-toolchain.exe` 自行初始化 MSVC 环境并写入已校验 prefix，可直接在干净 shell 调用；要求 Rust/Cargo 与 Python 在 PATH。

x64：

```powershell
rust/target/x86_64-pc-windows-msvc/release/dvda-toolchain.exe package --repo . --output <输出目录> --version <版本> --target x86_64-pc-windows-msvc --media-runtime build/msvc-native/ffmpeg-static/install --image-author build/gui-only-author-v1.1/x64 --prebuilt build/gui-only-author-v1.1/x64 --source build/gui-only-author-v1.1/x64/data --single-exe
```

WoA（ARM64，使用同一个 x64 工具）：

```powershell
rust/target/x86_64-pc-windows-msvc/release/dvda-toolchain.exe package --repo . --output <输出目录> --version <版本> --target aarch64-pc-windows-msvc --media-runtime build/msvc-native/ffmpeg-arm64/install --image-author build/gui-only-author-v1.2/arm64 --prebuilt build/gui-only-author-v1.2/arm64 --source build/gui-only-author-v1.2/arm64/data --single-exe
```

- `--source` 指向包含 `menu/` 的 data 目录，`--prebuilt` 指向包含 `fonts/` 的架构目录；两个架构的路径、target 和输出目录不得混用。
- 官方 static 媒体运行时不需要 `--ffmpeg-prefix`；shared/legacy 媒体运行时必须显式追加 `--ffmpeg-prefix <已验证的 shared FFmpeg 前缀>`（static 运行时会拒绝与之不同的 prefix）。
- `--single-exe` 由 `tools\win-build\build-all.cmd` 统一传入；也可直接调用上面的命令。不要手工复制 EXE、不要手工 `Compress-Archive`，也不要另写打包流程。
- 输出目录根部的 `MANIFEST.txt`、`release-build.json` 与 `DVD-Audio-Maker-<版本>-<架构>.exe` 均为本地产物，不进入 Git；发布资产只有两个单 EXE。许可证与第三方声明内嵌在 GUI「关于」窗口。
- Android 发布唯一入口为 `tools/android/build-flutter.ps1`，不进入 Windows 打包流程。
- 三平台（Windows x64、WoA、Android）产物和 SHA-256 全部就绪后才创建 tag/Release；重打包时保留原 release log 并更新 SHA-256。

### 无损精简菜单字体

静态 Regular TTC 必须包含完整 SC/JP/KR/TC 四地区字体；仅构建时需要 `fonttools==4.68.0` 和 `cffsubr==0.4.0`：

```powershell
.venv\Scripts\python.exe tools\win-build\optimize-menu-fonts.py build\font-sources\DvdaNotoCJK-Regular.ttc build\font-sources\optimized\DvdaNotoCJK-Regular.ttc
```

输出目标不得已存在。脚本对子程序化压缩后的共享 CFF2 轮廓及集合表进行优化，逐一核对全部 65,535 个字形轮廓、字体及 Private 参数、字形顺序与所有非 CFF2 表（仅排除依赖内容的 `head.checksumAdjustment`），全部通过后才写出。遇到不支持的 hint 或可变程序会拒绝处理。不删字符或地区字形，字符映射、排版、度量和元数据保持不变。通过原生渲染验收和 `dvda-toolchain font verify-collection` 后，将结果复制到 `build\msvc-native\package-assets\fonts\DvdaNotoCJK-Regular.ttc` 再打包；保留源字体用于对照。字体原始大小的降幅不等同于发布单 EXE 的降幅。
