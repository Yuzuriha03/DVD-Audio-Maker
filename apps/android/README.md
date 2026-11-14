# DVD-Audio Maker（Android 原生版）

直接编译仓库里的 Rust 代码：Android 端 UI 通过薄 JNI 绑定调用
dvda_core::app（AppOptions / run_prepare / run_build / run_verify），
不再使用早期自写的 shim，也没有逐功能重写逻辑。

## 结构
- app/src/main/java/com/dvda/maker/NativeEngine.kt
  加载 libdvda_core.so，导出 runProfile(profileJson, action)、cancel、progress*
- app/src/main/java/com/dvda/maker/MainActivity.kt
  Compose 界面，按桌面版 dvda-desktop 的页面与设置项重写：
  基本设置 / 音频编码 / 光盘菜单 三个页签，底部“检查音源 / 开始制作 / 停止任务 /
  验证成品 / 查看成品”，设置项与桌面版 profile 的 DVDA_* 键一一对应
- app/src/main/res/values*/strings.xml
  界面文案直接取自 dvda-desktop/src/i18n.rs（125 个键 × 简体/繁體/English/日本語/한국어）
- app/src/main/jniLibs/arm64-v8a/libdvda_core.so
  由 dvda-android crate（薄绑定）release 构建、strip 后 8.6MB，
  依赖只有 libc/libm/libdl（FFmpeg 静态链接）

## 构建
    cd rust/crates/dvda-android
    DVDA_FFMPEG_PREFIX=<ffmpeg-prefix> CC_aarch64_linux_android=<cc-wrapper>       cargo build --release --target aarch64-linux-android
    cp target/aarch64-linux-android/release/libdvda_android.so       ../../../apps/android/app/src/main/jniLibs/arm64-v8a/libdvda_core.so
    cd ../../../apps/android && ./gradlew assembleRelease

### 在手机上直接打 APK（aarch64）
本目录带 build-apk.sh：用 apt 的 aapt2/zipalign/apksigner + JDK + kotlinc +
Compose 插件 + R8/D8，在 arm64 设备上完成资源编译、Kotlin 编译、dex、对齐、签名。

    ./build-apk.sh          # 产物 /tmp/apkbuild/app-signed.apk

说明：手打包时 aapt2 只能用 apt 的 API 23 platform 做资源链接，
并会从清单副本里去掉 requestLegacyExternalStorage（新平台属性）；
Gradle/CI 构建仍使用完整的 compileSdk 37 与清单。

## 高刷新率
- Activity 在 onCreate/onResume 里把当前显示模式切到该屏幕支持的最高刷新率
  （preferredDisplayModeId + preferredRefreshRate，兼容旧 API）
- 进度轮询改为 withFrameNanos 跟随 VSYNC，进度条用动画插值，
  120Hz/144Hz 屏幕上进度与滚动随面板刷新
- Manifest 显式 hardwareAccelerated=true

## API 37
- compileSdk = 37、targetSdk = 37、minSdk = 24（Android 17）
- 本仓库附带的真机类型检查：Kotlin 2.2.0 + Compose 插件 2.2.0，
  使用 Android 17（API 37）framework jar，MainActivity/NativeEngine 编译通过

## 行为说明
- profile JSON 与桌面版同格式：{"Version":1,"Language":"zh-CN","Values":{DVDA_*}}
- 勾选项写成 "on"/"off"，其余为字符串；DVDA_MENU_INDEX_MIN_ALBUMS 固定 "4"
- 菜单字体默认 /system/fonts/NotoSansCJK-Regular.ttc（桌面版内置字体在这里换成系统字体）
- 进度来自 dvda-core 的 [TASK]/[METRIC] 事件；取消会传入 dvda-core 的 Callbacks
- 任务生命周期与桌面版一致：build = prepare -> build -> verify -> finish_task；
  未完成任务会弹“继续制作 / 重新制作”，失败/取消后弹“保留进度 / 删除进度”
- runProfile 的 action：prepare / build / build_continue / build_restart / verify / status / discard
