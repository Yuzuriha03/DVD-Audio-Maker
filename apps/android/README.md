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

### Windows（推荐）

    powershell -ExecutionPolicy Bypass -File tools/android/build-flutter.ps1

脚本会加载 `tools/android/ndk-env.ps1`，编译
`rust/crates/dvda-android`（`aarch64-linux-android`），把
`libdvda_android.so` 复制到 TEMP 中的暂存工程，再用
`build/flutter/bin/flutter.bat` 执行
`flutter build apk --release --target-platform android-arm64`。

为避免在 TF 卡等慢速盘上展开大量小文件，脚本把 Cargo target、Gradle 缓存和 Flutter
暂存工程放到 `%TEMP%\dvda-android-build`；每次使用独立的暂存工程，完成后只将
`app-release.apk` 复制回项目的 `apps/android/build/app/outputs/flutter-apk/`。

APK 完全由 Flutter/Gradle 打包和签名，不使用二次 ZIP 重写。通过标准
`useLegacyPackaging` 压缩 JNI 库；`stripReleaseNativeSymbols` 在 AGP 打包前剥离原生库调试符号，
再由 AGP 正常压缩、对齐和签名。当前产物使用 v2 签名。
当前 Android 构建使用本机 debug keystore 签名；它适合安装测试，不是商店正式发布签名。

### 已配置 NDK/Flutter 的 shell

    apps/android/build-apk.sh

该脚本要求 `cargo`、`flutter` 在 PATH 中，并已导出 NDK 环境变量；它只做
Rust 编译、复制 `.so` 和 `flutter build apk`。

APK 通常输出到
`apps/android/build/app/outputs/flutter-apk/app-release.apk`。

### 生成 FRB Dart 绑定

修改 `rust/crates/dvda-android/src/api.rs` 后重新生成：

    cd apps/android
    flutter_rust_bridge_codegen generate

配置见 `apps/android/flutter_rust_bridge.yaml`；生成目录为
`apps/android/lib/src/rust/`。

## 工具链

- Flutter 3.50.0 stable：官方预编译包，已解压到 `build/flutter`；不要 clone
  Flutter 仓库。
- Android NDK：`build/android-ndk-r30`；`tools/android/ndk-env.ps1` 设置
  `ANDROID_NDK_ROOT`、API 24 的 clang/linker 与 `DVDA_FFMPEG_PREFIX`。
- Rust：stable，target `aarch64-linux-android`。

## Android 约束

- `minSdk = 24`，`compileSdk = 37`，`targetSdk = 37`
- 仅 `arm64-v8a`；当前不生成 x86/x86_64 APK
- FRB 动态库为 `libdvda_android.so`；旧的 JNI `libdvda_core.so` 不再使用

## 运行时契约

- `AppBridge`：`init()`、`runProfile(profileJson, action)`、`progress()`、`cancel()`
- action：`prepare` / `build` / `build_continue` / `build_restart` / `verify` /
  `status` / `discard`
- profile JSON 与桌面版相同：`{"Version":1,"Language":"...","Values":{DVDA_*}}`
- 功能行为与桌面版对齐，但交互采用 Android/Material 与 SAF，不照搬桌面控件
- 不声明“所有文件访问”权限；通过 SAF 授权用户选择的目录并持久化授权
- 源目录通过系统文件浏览器流式复制为普通私有文件供 Rust 读取，启动时复用镜像而不重复复制
- 成品生成后导出到用户授权目录
- 不使用符号链接，避免 Rust 工作区的 reparse point 安全限制
- 界面语言：zh-CN / zh-TW / en / ja / ko；主题通过菜单选择并持久化
- 任务运行时锁定设置、语言和导航并自动进入任务页；返回时先确认停止再退出
- 进度采用与桌面版一致的 0–15 / 15–65 / 65–100 阶段百分比
- 高级设置位于右上角三点菜单并带文字开关；临时工作目录固定为应用私有路径，不提供设置
- 默认不保留中间文件；外部 MLP 导入禁用采样设置，抖动仅 16-bit 显示
- 字体设置归入高级设置并支持 SAF 选择；封面暗化保留滑块
- 日志支持仅问题过滤、复制和 SAF 导出；状态区显示运行时长与问题数
- 平台文件操作走 `MethodChannel('dvda/platform')`：
  `pickDirectory`、`linkDirectory`、`openProfile`、`saveProfile`、
  `saveTextFile`、`exportDirectory`、`openOutput`、`pickFont`
- 任务运行期间轮询 progress，不阻塞 UI；取消通过 `cancel()` 传到 dvda-core 的 Callbacks

## 验证范围

- dvda-core 的完整 prepare → MLP/LPCM → author → ISO → verify 链路此前已在
  aarch64 Android 环境跑通，详见 `docs/dvda-android-port-notes.md`。
- 本文档描述 Flutter/FRB 构建入口；在 x64 主机编译成功不等于 ARM64 真机运行通过。
- 当前只验证/支持 ARM64；发布签名、安装包体积与真机 UI 回归以实际构建结果为准。
