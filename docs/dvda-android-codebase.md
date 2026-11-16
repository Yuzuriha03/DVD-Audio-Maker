# DVD-Audio Maker Android 移植代码库

这是 Android 迁移的源码快照与架构说明（不含 Flutter/NDK 等大体积工具链和编译产物）。

## 目录

- `rust/` — 已合并到当前仓库的上游 GPL-3.0 源码（含 Android/Unix 平台分支）
- `rust/crates/dvda-android/` — Flutter Rust Bridge 绑定 crate（cdylib）
  - `src/api.rs` — 导出 `run_profile`、`cancel`、`progress_snapshot`，只转发，
    不实现编码或编排逻辑
  - 产物 — `libdvda_android.so`
- `apps/android/` — Flutter 根目录
  - `lib/` — Dart UI、状态、设置与语言资源
  - `lib/src/rust/` — FRB 生成的 Dart 绑定（生成物，不手改）
  - `android/` — Android 宿主工程（Gradle、FlutterActivity、MethodChannel）
- `deps/android-aarch64/` — 交叉编译的静态 FFmpeg/soxr 库与头文件（ARM64 Android）
- `tools/android/` — `build-flutter.ps1`（唯一构建入口）、`ndk-env.ps1`、`generate-icons.py`、`release-metadata.ps1`
- `build/flutter/`、`build/android-ndk-r30/` — 本地预编译 Flutter 与 NDK（不提交）
- `testdata/` — 测试音频（FLAC/WAV/封面）
- `docs/` — 移植记录与对齐审计

## 架构

```
Flutter UI
  -> AppController
  -> lib/bridge.dart
  -> FRB generated Dart (lib/src/rust)
  -> libdvda_android.so: rust/crates/dvda-android/src/api.rs
  -> dvda-core: AppOptions / run_prepare / run_build / run_verify
```

- Flutter 只负责展示、编辑设置和调用；业务唯一实现在 `dvda-core`。
- Android shell 只负责 FlutterActivity 生命周期与 MethodChannel 的文件/存储操作，
  不复制编码、菜单或验证逻辑。
- FRB 绑定不写编排逻辑，只把 desktop 兼容的 profile JSON、进度和取消传给
  `dvda-core`。

## 关键改动（相对上游）

- `dvda-core`、`dvda-menu`、`dvda-native`、`dvda-bridges`：增加 Android/Unix
  平台分支（文件属性/共享模式、宽路径、进程启动、运行时、磁盘空间、机器指纹与
  时间戳等）
- 非 Windows 分支：路径分隔符、流式 MLP 能力降级、菜单素材目录预检等
- `rust/rust-toolchain.toml` 使用 stable 通道，targets 包含 Windows 双架构与
  `aarch64-linux-android`
- 新增 `dvda-android` FRB crate，替代旧的 JNI 业务桥
- 新增 Flutter UI 与 Android 宿主；不再维护 Compose/classic View 界面

## 工具链与约束

- Flutter 3.50.0 stable：官方预编译包，解压到 `build/flutter`；不要 clone 仓库
- Android NDK r30：`build/android-ndk-r30`
- `minSdk/NDK API 24`，`compileSdk/targetSdk 37`
- 仅 ARM64 Android（`aarch64-linux-android` / `arm64-v8a`）
- FRB 动态库名为 `libdvda_android.so`，不是旧 JNI 的 `libdvda_core.so`

## 构建

唯一入口（Windows）：

    powershell -ExecutionPolicy Bypass -File tools/android/build-flutter.ps1

脚本加载 `tools/android/ndk-env.ps1`（NDK r30、API 24），编译
`rust/crates/dvda-android`（`aarch64-linux-android`），把 `libdvda_android.so`
放进 `%TEMP%` 暂存工程的 `android/app/src/main/jniLibs/arm64-v8a/`，再执行
`flutter build apk --release --target-platform android-arm64`。源码树不保留
`jniLibs/` 目录。APK 输出到
`apps/android/build/app/outputs/flutter-apk/app-release.apk`，同目录生成
`release-build.json`（commit、version、target、NDK、SDK、APK 字节数与 SHA-256，
并标注 `deviceVerified: false`）。

只交叉编译 Rust 绑定（不含 Gradle 打包与签名）：

    . tools/android/ndk-env.ps1
    cargo build --release --manifest-path rust/crates/dvda-android/Cargo.toml --target aarch64-linux-android

产物在 `$env:CARGO_TARGET_DIR/aarch64-linux-android/release/libdvda_android.so`；
官方流程由 `build-flutter.ps1` 放入 TEMP 暂存工程，不复制进源码树。

FRB Dart 绑定由 `apps/android/flutter_rust_bridge.yaml` 生成到
`apps/android/lib/src/rust/`：

    cd apps/android
    flutter_rust_bridge_codegen generate

生成文件不要手改。

## 验证

    # Rust bridge：dvda-android 被排除在 rust workspace 之外，必须单独跑
    cargo clippy --manifest-path rust/crates/dvda-android/Cargo.toml --target aarch64-linux-android --all-targets -- -D warnings
    cargo test --manifest-path rust/crates/dvda-android/Cargo.toml --lib

host 测试用 `--lib`：`examples/dlopen.rs` 是 Android/Linux 的 dlopen 探针，
Windows host 无法链接 `dl`。

    # Flutter
    cd apps/android
    flutter analyze
    flutter test

    # APK 产物
    & build/android-sdk/build-tools/37.0.0/aapt2.exe dump badging apps/android/build/app/outputs/flutter-apk/app-release.apk
    & build/android-sdk/build-tools/37.0.0/apksigner.bat verify --print-certs apps/android/build/app/outputs/flutter-apk/app-release.apk
    Get-FileHash -Algorithm SHA256 apps/android/build/app/outputs/flutter-apk/app-release.apk

构建或静态检查通过不代表 ARM64 真机业务链路已验证。

## 当前状态与验证边界

- 历史：早期 JNI/proot 阶段曾在 aarch64 Android 环境验证过 dvda-core 完整流程
  （prepare → MLP/LPCM 编码 → 规划 → author → staging 发布 → 自动验证），
  见 `docs/dvda-android-port-notes.md`；该文是历史实测，数字与结论不作现行基准。
- 当前 Flutter/FRB 入口只产出编译结果与 `release-build.json`；构建成功只代表
  编译通过，不等于 ARM64 真机运行通过。
- 当前只发布 ARM64；x86/x86_64 未构建。正式发布签名与真机安装回归以实际构建
  结果为准。
- 旧 JNI/Compose/手打包流程（`build-apk.sh`、手写 CMake 工具链）已不再存在；
  历史演进仅保留在 port notes 中。

## 许可

上游 DVD-Audio-Maker 为 GPL-3.0；本目录中的修改与新增代码同样遵循 GPL-3.0。
