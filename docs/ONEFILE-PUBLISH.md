# Windows 单 EXE 发布

标准发布资产只有两个单文件 EXE：`DVD-Audio-Maker-<版本>-win-x64.exe`（Intel/AMD）与 `DVD-Audio-Maker-<版本>-win-arm64.exe`（Windows on ARM）。`dvda-toolchain package --single-exe` 直接产出可上传的 EXE；不生成、不手工打包、也不上传 ZIP。输出目录中的 `MANIFEST.txt` 与 `release-build.json` 是开发核对记录，不作为发布资产。

## 组件布局

必要的 C ABI DLL、共享 FFmpeg 库、制盘组件、菜单素材和一份中日韩字体集合内嵌到 Rust EXE。许可证、FFmpeg LGPL、第三方与图像/菜单声明也内嵌到 GUI「关于」窗口；仓库内同时保留文本。打包器在平铺运行目录时比较同名组件的内容；相同文件只保留一份，内容不同则报错，不能按文件名盲目删除。

打包前核对媒体、图像、author 的构建记录、实际文件集合、大小/SHA-256、目标架构 PE、普通/延迟导入及共享 FFmpeg profile。原始四份 OTF 可以直接合并为共享 TTC；已有 TTC 也须通过 SC/JP/KR/TC 顺序、名称、覆盖和表校验。可变字体源须先实例化为 400 字重的静态 Regular，不能只改文件名把默认 Thin 当作 Regular。候选目录与单 EXE 全部完成后事务提交，失败保留上一份发布。

启动时把组件释放到 `%TEMP%/DVD-Audio-Maker-tasks/task-*/runtime/` 中的独立任务目录，不再保留共享运行库缓存。逐个核对文件大小和完整 SHA-256；退出清理助手等待主进程释放 DLL 后删除该目录，异常残留在下次启动回收。归档路径拒绝越界，释放目录拒绝重解析点。压缩和解压使用 Windows 10+ 自带的 LZMS 压缩 API，对字体数据的压缩率明显优于 XPRESS Huffman。

制盘和 ISO writer 静态编入 GUI，通过 Rust 进程内接口调用；独立开发 author EXE 不进入内嵌运行时。媒体解码、转换、MLP 编码和图像处理通过 Rust 接口与第三方库完成。单 EXE 是交付形式，运行时释放所需第三方库、字体和菜单素材。

## 构建

先构建 x64 工具链（x64 与 WoA 共用同一个 x64 构建工具）。使用项目的 MSVC 包装脚本，保证在干净 shell 中可用：

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File tools\win-build\cargo-msvc.ps1 build --manifest-path rust\Cargo.toml --target x86_64-pc-windows-msvc --release --offline -p dvda-toolchain
```

该包装脚本初始化 MSVC 环境并默认设置 `DVDA_FFMPEG_PREFIX=build\msvc-native\ffmpeg-static\install`（需先准备 Native inputs）。打包用的 `dvda-toolchain.exe` 自行初始化 MSVC 环境并记录已校验的 prefix，可在干净 shell 直接运行，但要求 Rust/Cargo 与 Python 在 PATH。

x64：

```powershell
rust/target/x86_64-pc-windows-msvc/release/dvda-toolchain.exe package --repo . --output <输出目录> --version <版本> --target x86_64-pc-windows-msvc --media-runtime build/msvc-native/ffmpeg-static/install --image-author build/gui-only-author-v1.1/x64 --prebuilt build/gui-only-author-v1.1/x64 --source build/gui-only-author-v1.1/x64/data --single-exe
```

WoA（ARM64，使用同一个 x64 工具）：

```powershell
rust/target/x86_64-pc-windows-msvc/release/dvda-toolchain.exe package --repo . --output <输出目录> --version <版本> --target aarch64-pc-windows-msvc --media-runtime build/msvc-native/ffmpeg-arm64/install --image-author build/gui-only-author-v1.2/arm64 --prebuilt build/gui-only-author-v1.2/arm64 --source build/gui-only-author-v1.2/arm64/data --single-exe
```

- `--source` 必须指向包含 `menu/` 的 data 目录；`--prebuilt` 必须指向包含 `fonts/` 的架构目录；x64 与 WoA 的 target、构建目录和输出目录必须一一对应，禁止混用。
- 官方 static 媒体运行时不需要 `--ffmpeg-prefix`；使用 shared/legacy 媒体运行时必须显式追加 `--ffmpeg-prefix <已验证的 shared FFmpeg 前缀>`（static 运行时会拒绝与之不同的 prefix）。
- 两个架构都必须使用 `--single-exe`；不要手工复制 EXE、不要手工 `Compress-Archive`，也不要另写打包流程。
- Android 发布唯一入口为 `powershell -ExecutionPolicy Bypass -File tools/android/build-flutter.ps1`，不进入 Windows 打包流程。

## 验证

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File tools/win-build/test-rust-workflow.ps1 -OtherVolume D:\
python -X utf8 tools/win-build/test-rust-gui.py --exe <输出目录>/DVD-Audio-Maker-<版本>-win-x64.exe
python -X utf8 tools/win-build/test-rust-onefile.py --exe <输出目录>/DVD-Audio-Maker-<版本>-win-x64.exe
powershell -NoProfile -ExecutionPolicy Bypass -File tools/win-build/test-app-icon.ps1 -Executable <输出目录>/DVD-Audio-Maker-<版本>-win-x64.exe
python -X utf8 tools/win-build/test-rust-package.py <x64输出目录> <WoA输出目录> --output build/package-validation.json
```

第一个脚本指定冻结基准使用的原生库，并启用全部集成测试。GUI 脚本用隔离配置检查三语言、保存与恢复、非法输入、滚动和文件对话框。单文件脚本只复制 EXE，移除开发组件环境覆盖，生成测试音频与封面，通过实际 GUI 完成 MLP/LPCM 制作与成品验证，再损坏缓存 DLL 验证自动修复。图标脚本直接核对 EXE 内嵌图标与 `assets/dvd-audio-maker.ico` 的逐帧一致性；ICO 由唯一母版 `assets/dvd-audio-maker.png` 生成。发布包校验脚本读取每个单 EXE 输出目录中的 `release-build.json` 与 `MANIFEST.txt`，核对 `single_exe=true`、EXE SHA-256、PE 架构、记录输入（`--source`、`--prebuilt`、FFmpeg prefix 等）以及 `DVDRUN01` 内嵌资源哈希；必须指向真实输出目录，它不启动制盘、不删除文件（仅 Windows 可用 Cabinet API）。

`-OtherVolume` 指向另一可写卷，仅创建并清理测试自己的临时子目录；没有第二个卷时省略该参数会明确跳过跨卷替换专项，其余集成测试照常执行。

## 发布门槛

1. x64 EXE、WoA EXE 与 Android APK 三个产物全部构建完成，并分别计算 SHA-256。
2. 保留上一份 release log（`UPDATELOG.md` 的历史段落）与既有 Release 说明，再追加本版本与三平台 SHA-256；重新打包时必须更新对应 SHA-256，不得用半成品覆盖已发布资产。
3. 三平台产物与 SHA-256 全部就绪后再更新标签与 Release。Windows Release 只上传两个单 EXE（x64 与 WoA），不上传 ZIP；Android 只上传 APK。

发布产物只留在 Git 忽略目录中；上传 GitHub Release 是单独的发布操作。工具链的旧 ZIP 模式仅用于历史核对，不进入当前发布流程。
