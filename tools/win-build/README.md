# Windows x64 构建与发布

当前产品是 Windows x64 Rust 原生 GUI，运行时不需要 .NET。重建原生组件需要 Rust、Visual Studio C++ x64 工具、Windows SDK、Python 和原生 Windows CMake。FFmpeg 的 configure/make 使用 MSYS2 shell，但 C/C++ 编译和链接全部使用 MSVC。

## Rust 构建

```bat
cargo build --manifest-path rust\Cargo.toml --target x86_64-pc-windows-msvc --workspace --offline
cargo fmt --manifest-path rust\Cargo.toml --all -- --check
cargo clippy --manifest-path rust\Cargo.toml --target x86_64-pc-windows-msvc --workspace --offline -- -D warnings
cargo test --manifest-path rust\Cargo.toml --target x86_64-pc-windows-msvc --workspace --offline -- --include-ignored
```

`gui.cmd`、`gui-debug.cmd`、`cli.cmd`、`build.cmd` 和 `verify.cmd` 都调用 Rust workspace。配置只使用 JSON 方案和 `--profile`；不支持 `config.env`、`DVDA_CONFIG` 和 `--config`。

完整 author 由 Rust 实现：应用进程内调用，独立 Rust CLI 复用同一流程。`build-image-author.py` 是 `build-rust-author.py` 的兼容入口，旧 C producer、源码和冻结二进制已清理。AOB、管理表、菜单/静图和 ISO 的验收记录保存在本地源码工作副本中，黄金数据的历史来源摘要仍保留。

## 原生输入

生产构建默认使用 `build/msvc-native/ffmpeg-static/install`。使用
`build-msvc-ffmpeg.py --profile static --work-directory build/msvc-native/ffmpeg-static`
生成 MSVC 静态库，并通过 `--msys-root`、`--cmake` 指定本机工具路径。
五个 FFmpeg 库、soxr、zlib 和 C 运行库静态链接到 EXE；内嵌资源只需解压字体、XML 和菜单素材。
开发对照仍可显式设置 `DVDA_FFMPEG_PREFIX` 使用 shared 构建。静态链接不代表已经测得编码提速。

打包器需要已验证的 x64 媒体/图像构建记录与 `build/rust-author-production`，并验证摘要、大小、PE 架构、DLL 闭包及字体/菜单素材。图像记录必须标识 Rust 实现；本机验收使用 `build/rust-image-runtime`。MLP 编码器直接编入 Rust 应用，`--encoder-runtime` 仅用于认证可选对照产物，不分发编码器 DLL。FFmpeg 和 ImageMagick 命令行程序不是运行时输入，第三方库由 Rust bridge 调用；媒体/图像开发 adapter DLL 不进入静态链接发布包。

格式解析、MLP CRC/奇偶校验、PCM 比较及只读 AOB 成品校验已经内置为 Rust 实现，不需要格式或校验 DLL。打包不再接受 `--formats-runtime` 输入。

生成的 ImageMagick/委托库工程使用项目名前缀命名对象文件，避免合并 MagickCore、JPEG 8/12/16 位静态库时出现同名对象和 MSVC LNK4255 调试信息丢失警告。

## 应用图标

图标源文件为 [SVG](../../assets/dvd-audio-maker.svg)，[PNG](../../assets/dvd-audio-maker.png) 为透明背景栅格母版，[ICO](../../assets/dvd-audio-maker.ico) 包含 16、20、24、32、40、48、64、96、128、256 像素版本。更新 SVG 后，先在浏览器导出透明背景正方形 PNG，再运行：

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File tools\win-build\generate-app-icon.ps1
```

桌面构建将 ICO 嵌入 EXE 的 Windows 资源，主窗口也加载同一资源，因此资源管理器、任务栏和发布 ZIP 内的程序使用同一图标，无需旁置图标文件。打包记录包含三种图标素材的摘要。标识使用普通项目名称文字，不是官方格式或认证 Logo，也不代表取得商标授权。

## 发布 ZIP

```bat
tools\win-build\build-all.cmd ^
  --media-runtime build\msvc-native\ffmpeg-static\install ^
  --image-runtime build\msvc-native\image-runtime ^
  --image-author build\rust-author-production ^
  --prebuilt build\msvc-native\package-assets ^
  --version v1.0
```

命令通过 `dvda-toolchain package` 生成 `DVD-Audio-Maker-v1.0-win-x64.zip`。静态发布包只包含内嵌菜单资源和字体的 GUI EXE，以及旁置用户文档，不包含 .NET、开发 CLI、PDB、构建 JSON 或本机配置。三语用户 README、许可证与两个 NOTICE 均在 EXE 旁边。

### 无损精简菜单字体

静态 Regular TTC 必须包含完整 SC/JP/KR/TC 四地区字体；仅构建时需要 `fonttools==4.68.0` 和 `cffsubr==0.4.0`：

```powershell
.venv\Scripts\python.exe tools\win-build\optimize-menu-fonts.py build\font-sources\DvdaNotoCJK-Regular.ttc build\font-sources\optimized\DvdaNotoCJK-Regular.ttc
```

输出目标不得已存在。脚本对子程序化压缩后的共享 CFF2 轮廓及集合表进行优化，逐一核对全部 65,535 个字形轮廓、字体及 Private 参数、字形顺序与所有非 CFF2 表（仅排除依赖内容的 `head.checksumAdjustment`），全部通过后才写出。遇到不支持的 hint 或可变程序会拒绝处理。不删字符或地区字形，字符映射、排版、度量和元数据保持不变。通过原生渲染验收和 `dvda-toolchain font verify-collection` 后，将结果复制到 `build\msvc-native\package-assets\fonts\DvdaNotoCJK-Regular.ttc` 再打包；保留源字体用于对照。字体原始大小的降幅不等同于发布 ZIP 的降幅。
