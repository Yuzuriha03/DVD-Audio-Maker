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
- `tools/android/` — `build-flutter.ps1`、`ndk-env.ps1`、图标生成与 CMake 工具链
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

Windows：

    powershell -ExecutionPolicy Bypass -File tools/android/build-flutter.ps1

已配置 NDK/Flutter 的 shell：

    apps/android/build-apk.sh

只构建 Rust 绑定（Windows PowerShell）：

    . tools/android/ndk-env.ps1
    cargo build --release --manifest-path rust/crates/dvda-android/Cargo.toml --target aarch64-linux-android

产物复制到
`apps/android/android/app/src/main/jniLibs/arm64-v8a/libdvda_android.so`。

FRB Dart 绑定由 `apps/android/flutter_rust_bridge.yaml` 生成到
`apps/android/lib/src/rust/`：

    cd apps/android
    flutter_rust_bridge_codegen generate

生成文件不要手改。

## 当前状态与验证边界

- 真 dvda-core 的完整流程已在 aarch64 Android 环境验证：
  prepare → MLP/LPCM 编码 → 规划 → author → staging 发布 → 自动验证。
- 已验证项见 `docs/dvda-android-port-notes.md`（菜单/索引/封面、LPCM 16/24、
  MLP 20-bit、重采样、外部 MLP、取消/继续、五种语言等）。
- Flutter/FRB 是本轮迁移目标；构建入口如上。x64 主机编译通过不等于 ARM64
  真机运行通过。
- 当前只发布 ARM64；x86/x86_64 未构建。正式发布签名与真机安装回归以实际构建
  结果为准。
- 旧 JNI/Compose/手打包流程不再作为当前构建路径；历史演进仅保留在 port notes 中。

## 许可

上游 DVD-Audio-Maker 为 GPL-3.0；本目录中的修改与新增代码同样遵循 GPL-3.0。
