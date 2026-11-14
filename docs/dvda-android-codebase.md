# DVD-Audio Maker Android 移植代码库

这是本次移植工作的完整源码快照（不含大体积工具链/编译产物）。

## 目录

- rust/                     已合并到当前仓库的上游 GPL-3.0 源码（含平台适配）；rust/crates 下为 dvda-core 等
- rust/crates/dvda-android/ 薄 JNI 绑定 crate（cdylib，导出 NativeEngine 的 runProfile 等）
- apps/android/             Android 原生工程（classic View UI + JNI + build-apk.sh + jniLibs）
- deps/android-aarch64/     交叉编译的静态 FFmpeg/soxr 库与头文件（arm64 Android）
- tools/android/toolchain/  CMake 交叉工具链文件
- testdata/                 测试音频（FLAC/WAV/封面）
- docs/                     移植记录与对齐审计（dvda-android-port-notes.md / dvda-parity-audit.md）
- patches/                  对上游源码的本地修改补丁（git diff）

## 关键改动（相对上游）

- rust/crates/dvda-core、dvda-menu、dvda-native、dvda-bridges：增加 Android/Unix 平台分支
  （文件属性/共享模式、宽路径、进程启动、运行时解包、磁盘空间、机器指纹与时间戳等）
- dvda-core 的路径分隔符、流式 MLP、菜单素材预检等仅在 Windows 生效的行为在非 Windows 上分支
- rust/rust-toolchain.toml 使用 stable 通道（当前为 Rust 1.100.0），targets 包含 Windows 双架构与 aarch64-linux-android
- 新增 dvda-android crate：只做 JNI 绑定，调用 AppOptions / run_prepare / run_build / run_verify

## Windows 本地 NDK（Rust 1.100.0）

将 `android-ndk-r30-windows.zip` 解压到被忽略的 `build/android-ndk-r30`，然后在 PowerShell 中加载项目环境：

    . tools/android/ndk-env.ps1
    cargo  build --release --manifest-path rust/crates/dvda-android/Cargo.toml --target aarch64-linux-android

脚本会设置 `ANDROID_NDK_ROOT`、`aarch64-linux-android24` 的 Rust linker/编译器、`llvm-ar` 和 `DVDA_FFMPEG_PREFIX`。NDK、Rust target 输出和调试密钥都不进入版本控制。

## 构建 Rust（在 aarch64 Linux 上为目标 Android 交叉编译）

环境：NDK sysroot + host clang wrapper（cc-android24）、静态 FFmpeg（deps/android-aarch64）。

    export DVDA_FFMPEG_PREFIX=<repo>/deps/android-aarch64
    export DVDA_HOST_CC=gcc
    export CC_aarch64_linux_android=<cc wrapper>
    export CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER=<cc wrapper>
    cd rust
    cargo build --release --target aarch64-linux-android -p dvda-core

    cd crates/dvda-android
    cargo build --release --target aarch64-linux-android
    cp target/aarch64-linux-android/release/libdvda_android.so \
       ../../../apps/android/app/src/main/jniLibs/arm64-v8a/libdvda_core.so

## 构建 APK

apps/android/build-apk.sh 是一条可在 arm64 设备上直接跑的脚本：
aapt2 资源编译/链接（生成 R.java）-> javac -> kotlinc（jvmTarget 1.8）-> D8 ->
打包 classes.dex + libdvda_core.so -> zipalign -> apksigner 签名。

依赖：apt 的 aapt2/zipalign/apksigner、JDK 17+、kotlinc、r8.jar、
以及 API 37 framework jar（脚本默认路径 /root/kt，见脚本头部变量）。

    cd apps/android && ./build-apk.sh

## 当前状态

- dvda-core 已在本机（aarch64 Android）编译并跑通完整流程：
  prepare -> MLP/LPCM 编码 -> 规划 -> author -> staging 发布 -> 自动验证
- 已验证：菜单/索引页/封面/制式、LPCM 16/24、20-bit LPCM 拒绝、MLP 20-bit、
  44.1k/96k 重采样、外部 MLP 导入（ISO hash 与现编码一致）、取消保留已完成 ISO、
  未完成任务继续/重新制作、5 语言日志
- Android UI 目前是经典 View 实现（为规避手工打包时 Compose 传递依赖缺失导致的闪退）；
  Compose 版本源码仍在 git 历史/补丁可查，待稳定性确认后可恢复
- 已知限制：APK 为 debug 签名、仅 arm64、资源链接使用 API 23 platform（手工打包）

## 许可

上游 DVD-Audio-Maker 为 GPL-3.0；本目录中的修改与新增代码同样遵循 GPL-3.0。