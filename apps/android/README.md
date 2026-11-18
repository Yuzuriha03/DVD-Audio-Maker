# DVD-Audio Maker（Android · Flutter + flutter_rust_bridge）

Flutter 只实现 GUI；音源检查、规划、MLP/LPCM 编码、菜单、ISO 发布与验证仍由
Rust 的 dvda-core 完成，通过 flutter_rust_bridge 直接调用。Android 端不再维护
Compose/classic View 业务界面，也不使用手写 JNI 业务桥。

## 结构

- `apps/android/` — Flutter 根目录（`pubspec.yaml`、`lib/`）
  - `lib/` — Dart UI、状态、设置与五种界面语言
  - `lib/src/rust/` — FRB 生成的 Dart 绑定（生成物，不手改）
  - `android/` — Flutter 的 Android 宿主（Gradle、FlutterActivity、MethodChannel）
- `rust/crates/dvda-android/` — FRB crate；`src/api.rs` 只做参数、进度与取消转发，
  调用 `dvda_core::app`，产物为 `libdvda_android.so`
- `tools/android/` — 构建脚本与 NDK 环境
- `deps/android-aarch64/` — ARM64 Android 静态 FFmpeg/soxr

## 构建

**唯一构建/发布入口**：

    powershell -ExecutionPolicy Bypass -File tools/android/build-flutter.ps1

脚本会加载 `tools/android/ndk-env.ps1`（NDK r30、API 24 的 clang/linker），编译
`rust/crates/dvda-android`（`aarch64-linux-android`），把
`libdvda_android.so` 复制到 TEMP 中的暂存工程，再用
`build/flutter/bin/flutter.bat` 执行
`flutter build apk --release --target-platform android-arm64 --no-pub`。依赖只在构建前解析一次；
本地分发包和依赖缓存齐备时，可给同一脚本传 `-Offline`，按提交的 Pub 锁文件解析，
让 Gradle 和 Cargo 只用本地缓存，并禁用 Gradle 的 SDK 自动下载。

脚本在 `%TEMP%\dvda-android-build` 中为每次构建创建独立的 Flutter 暂存工程，
复用 `build/android-cargo-target` 中的 Cargo 产物，并优先使用 `build/gradle-home` 中的
本地 Gradle 分发包和缓存。完成后将 `app-release.apk` 复制回
`apps/android/build/app/outputs/flutter-apk/`。
不再维护第二个 shell/手打包入口：手工复制 `.so`、aapt2/kotlinc/D8 拼包都不属于当前流程。

APK 由 Flutter/Gradle 打包和签名，不做二次 ZIP 重写。`useLegacyPackaging = false`，
JNI 原生库以非压缩方式存放，发布包需核对 16 KiB 对齐。`stripReleaseNativeSymbols`
在打包前剥离原生库的调试符号。当前脚本使用本机 debug keystore 签名，供直接安装，
不是 Google Play 上传签名。实际签名方案和证书以 `apksigner verify` 的结果为准。

复制 APK 后，脚本调用 `tools/android/release-metadata.ps1` 在 APK 旁生成
`release-build.json`，记录 commit、versionName/versionCode、`android-arm64` /
`aarch64-linux-android`、NDK revision、compileSdk/minSdk/targetSdk、APK 字节数与
SHA-256，并在控制台打印哈希。该记录同时写明 `"deviceVerified": false`：
构建成功只代表编译通过，不代表已在真机运行。

### 生成 FRB Dart 绑定

修改 `rust/crates/dvda-android/src/api.rs` 后重新生成：

    cd apps/android
    flutter_rust_bridge_codegen generate

配置见 `apps/android/flutter_rust_bridge.yaml`；生成目录为
`apps/android/lib/src/rust/`。

## 验证

产物核对（使用本机 `build/android-sdk` 中的 build-tools）：

    $apk = 'apps/android/build/app/outputs/flutter-apk/app-release.apk'
    & build/android-sdk/build-tools/37.0.0/aapt2.exe dump badging $apk |
        Select-String 'package:|sdkVersion|native-code'
    & build/android-sdk/build-tools/37.0.0/apksigner.bat verify --print-certs $apk
    & build/android-sdk/build-tools/37.0.0/zipalign.exe -c -P 16 4 $apk
    Get-FileHash -Algorithm SHA256 $apk

Rust bridge 门禁（`dvda-android` 被排除在 `rust` workspace 之外，`--workspace`
不会覆盖它，必须单独运行）：

    cargo clippy --manifest-path rust/crates/dvda-android/Cargo.toml `
        --target aarch64-linux-android --all-targets -- -D warnings
    cargo test --manifest-path rust/crates/dvda-android/Cargo.toml --lib

host 测试只跑 `--lib`：`examples/dlopen.rs` 是 Android/Linux 的 dlopen 探针，
Windows host 无法链接 `dl`。

Flutter 检查（需要 `build/flutter` 的 `flutter` 在 PATH 中）：

    cd apps/android
    flutter analyze
    flutter test

真机安装与启动（用于确认 APK 能安装并启动，不代替业务链路验证）：

    build/android-sdk/platform-tools/adb.exe install -r $apk

先在一个终端抓取启动和崩溃日志，再在另一个终端启动应用：

    build/android-sdk/platform-tools/adb.exe logcat -b main -b crash -s DvdaMaker AndroidRuntime flutter libc DEBUG
    build/android-sdk/platform-tools/adb.exe shell am start -n com.dvda.maker/.MainActivity

`DvdaMaker` 标记应用启动阶段；崩溃由 Android 系统记录，不另写应用私有崩溃文件。

以上任意一项通过都只代表对应层级通过；ARM64 真机上的完整
prepare → 编码 → author → verify 链路需要单独在设备上执行。

## 工具链

- Flutter 3.50.0 stable：官方预编译包，已解压到 `build/flutter`；不要 clone
  Flutter 仓库。
- Android NDK：`build/android-ndk-r30`；`tools/android/ndk-env.ps1` 设置
  `ANDROID_NDK_ROOT`、API 24 的 clang/linker 与 `DVDA_FFMPEG_PREFIX`。
- Rust：stable，target `aarch64-linux-android`。
- 图标：只由 `assets/dvd-audio-maker.png` 生成，入口
  `tools/android/generate-icons.py`（launcher mipmap + 通知栏小图标）。
- 菜单字体：复用项目既有已准备字体 `DvdaNotoCJK-Regular.ttc`
  （sha256 `b0668861…5131`，14,251,880 字节）。构建要求它的 x64/arm64
  prebuilt 副本完全一致；`tools/android/build-flutter.ps1` 校验哈希后把它复制进
  暂存工程的 `android/app/src/main/assets/fonts/`，不下载、也不新增第二字体来源。
  缺失时先按 `tools/win-build/README.md` 的字体流程准备。
- 发布记录：`tools/android/release-metadata.ps1` 由
  `tools/android/build-flutter.ps1` 自动调用，输出 `release-build.json`。

## Android 约束

- `minSdk = 24`，`compileSdk = 37`，`targetSdk = 37`
- 仅 `arm64-v8a`；当前不生成 x86/x86_64 APK
- FRB 动态库为 `libdvda_android.so`；旧的 JNI `libdvda_core.so` 不再使用

## 运行时契约

- `AppBridge`：`init()`、`runProfile(profileJson, action)`、`progress()`、`cancel()`
- action：`build` / `build_continue` / `build_restart` / `status` / `discard` /
  `finish_delivery`
- profile JSON 与桌面版相同：`{"Version":1,"Language":"...","Values":{DVDA_*}}`
- 功能行为与桌面版对齐，但交互采用 Android/Material 与 SAF，不照搬桌面控件
- 不声明“所有文件访问”权限；通过 SAF 授权用户选择的目录并持久化授权
- 源目录优先通过 `saf` 插件打开文件描述符，把 `/proc/self/fd/<fd>` 直接交给共享 Rust；逻辑路径用于专辑、标题和封面，fd 仅用于实际读取
- 不支持 fd 的提供方回退为受空间预算保护的普通私有文件镜像；不创建符号链接，不把 fd 路径跨进程保存
- 成品生成后导出到用户授权目录
- 界面语言：zh-CN / zh-TW / en / ja / ko；主题通过菜单选择并持久化
- 任务运行时锁定设置、语言和导航并自动进入任务页；运行时返回会先询问是否停止任务，空闲时遵循 Android 的系统返回行为
- 进度采用与桌面版一致的 0–15 / 15–65 / 65–100 阶段百分比
- 高级设置位于右上角「更多设置」中；临时工作目录固定为应用私有路径，不提供设置
- 默认不保留中间文件；外部 MLP 导入禁用采样设置，抖动仅 16-bit 显示
- 字体设置归入高级设置并支持 SAF 选择；封面暗化保留滑块
- 日志支持仅问题过滤和复制当前视图；SAF 导出的是单次任务完整日志，不受面板过滤限制。状态区显示运行时长与问题数
- 平台文件操作走 `MethodChannel('dvda/platform')` 和 `saf` 插件：前者保留目录选择、
  事务导出、镜像回退和缓存释放，后者提供源文档遍历及 fd 打开/关闭
- SAF 导出是事务：先写临时 SAF 文档并校验长度/SHA-256，再把旧成品改名为备份、
  用已校验文档替换；任一步失败都回滚并保留旧成品，失败不删除任务工作区。
  只有校验通过的导出才会调用共享 finish；finish 失败显示本地化清理失败、
  保留可恢复工作区并提供重试，绝不显示成功
- 导出期间进度保持 99，只有共享 finish 成功才由 Rust 置为 100；导出响应取消
- 源文档 fd 在操作结束时统一关闭；外部 MLP 目录保留既有镜像回退。
- 镜像回退逐文件按 64 MiB 余量检查空间，不足时安全失败并删除半成品，不残留部分文件
- 内部缓存（SAF 源镜像、旧内部 output）属于可重建 I/O 缓存：
  导出适配器在本次导出成功后释放本任务的源镜像；设置页的“删除进度并清理”是
  唯一显式用户确认入口，一次确认后先走共享 discard 删除未完成任务工作区，再经
  Kotlin 缓存适配器删除上述已知目录；无未完成任务时也允许单独清缓存，不删除
  SAF 授权目录和已导出的 ISO
- 菜单字体来自唯一项目字体母版：官方构建把准备好的 `DvdaNotoCJK-Regular.ttc`
  打入 APK assets，Kotlin 在启动/任务前解压到
  `files/runtime/fonts/DvdaNotoCJK-Regular.ttc`；不下载、不引入第二字体来源
- 任务运行期间轮询 progress，不阻塞 UI；取消通过 `cancel()` 传到 dvda-core 的 Callbacks

## 验证范围

- 历史记录：早期 JNI/proot 阶段曾在 aarch64 Android 环境跑通完整链路，见
  `docs/dvda-android-port-notes.md`（该文是历史实测记录，不代表当前 Flutter+FRB 构建）。
- 当前 Flutter+FRB 入口产出的是编译结果与 `release-build.json`；构建成功只代表
  编译通过，不等于 ARM64 真机运行通过，也不代表业务链路已验证。
- 当前只发布/支持 ARM64；发布签名、安装包体积与真机 UI 回归以实际构建结果为准。
