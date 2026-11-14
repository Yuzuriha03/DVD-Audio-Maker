# DVD-Audio-Maker 移植 Android：实测记录（2026-11-13，手机 Ubuntu 24.04 proot）

## 环境
- 主机：aarch64（Android 17 内核），6 核，15GB RAM，Ubuntu 24.04 proot
- Rust：rustup stable 1.100.0 + target aarch64-linux-android
- NDK：android-ndk-r28c（官方仅提供 x86_64 Linux 预编译，本机跑不了；
  实际用 Ubuntu clang 18 + NDK sysroot 交叉链接）
- 源码：Yuzuriha03/DVD-Audio-Maker（GPL-3.0），clone 到 /root/workspace/dvda-port

## 已验证可用
1. 5 个纯 Rust crate 交叉编译到 aarch64-linux-android 通过：
   dvda-mlp / dvda-audio / dvda-image / dvda-author / dvda-menu（28.5s）
2. dvda-author 全套测试在 ARM 上全绿（改了一处非 Windows 默认字体）
3. 端到端出 ISO：shim -> command::run -> disc.iso 815104 字节，ISO9660 CD001 有效
4. Android .so：libdvda_shim.so 5.0MB，aarch64 ELF，导出 dvda_selftest
   NEEDED: libdl.so / libm.so / libc.so（bionic）
5. 性能（本机 A78 类核心，通用非 SIMD 路径）：
   - MLP 编码 96kHz/24bit/2ch：7.3x 实时，压缩比 0.59
   - AOB + ISO 写出：约 200x 实时

## 移植阻塞点（dvda-cli / dvda-core 全链路）
1. dvda-menu/src/resources.rs:428 断言 size_of::<Dirent>()==268（MinGW 布局），
   Linux/Android 上 c_long=8 字节 -> 272，编译失败
2. 同文件 511 行 c_char 符号性（i8 vs u8）
3. dvda-bridges/build.rs 硬断言只接受 Windows MSVC，且要求 MSVC 编的
   avformat/avcodec/avutil/swresample/swscale + soxr + build-manifest.json；
   还链接 ole32/user32/bcrypt/advapi32/shell32/ws2_32
4. dvda-cli/build.rs、dvda-desktop/build.rs 同样断言 Windows-only
5. GUI 是 Win32/GDI，需重写（Kotlin/Compose 或 Flutter）
6. 默认菜单字体依赖 Windows 字体名/路径；Android 要打包 DvdaNotoCJK 字体
7. 配置/日志写在 EXE 同目录，Android 要改成 app 私有目录

## 复用已有成果的构建配方
    NDK_SYSROOT=$NDK/toolchains/llvm/prebuilt/linux-x86_64/sysroot
    CC_aarch64_linux_android="clang --target=aarch64-linux-android24 --sysroot=$NDK_SYSROOT -fuse-ld=lld -L$NDK/.../lib/clang/19/lib/linux/aarch64"
    CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER=同上
    cargo build --release --target aarch64-linux-android --lib

## 工作量估计
- 交叉编译 FFmpeg+soxr：1-2 天（PC 上做，换掉 build-msvc-ffmpeg.py）
- 平台化 dvda-bridges（FFI/链接/清单校验）：1-2 天
- 去掉 dirent/Win32/字体/路径硬编码：1-2 天
- 手机 GUI：1-2 周
合计约 2-3 周能出可用 App；MLP 编码、ISO/UDF 写出这些核心零改动。

## 第二轮：FFmpeg + 媒体桥移植（2026-11-14）

### 已完成
1. FFmpeg 9.0.2 + libsoxr 0.1.3 交叉编译到 Android aarch64 成功
   - 源码哈希与仓库固定值一致（ffmpeg 8c3850...，soxr db6ca1...）
   - 用的配置（media profile）：target-os=android arch=aarch64，静态库，
     解码 mlp/flac/alac/aac/mpeg2video，解复用 mlp/flac/mov/mpegps，swresample+swscale+libsoxr
   - 产物：deps/android-aarch64/lib/libav{format,codec,util}.a、libsw{resample,scale}.a、libsoxr.a
2. 关键 hack：官方 NDK 只有 x86_64 预编译 clang，本机 arm64 跑不了。
   用 Ubuntu clang 18 + NDK r28c sysroot 交叉编译，wrapper 需要：
   --target=aarch64-linux-android24 --sysroot=NDK/sysroot -fuse-ld=lld
   -rtlib=compiler-rt --unwindlib=libunwind -L NDK/lib/clang/19/lib/linux/aarch64
   并把 NDK 的 libclang_rt.builtins-aarch64-android.a 软链到 clang18 的资源目录。
   FFmpeg configure 还要 --extra-ldflags 带 -lm，否则 soxr 检测失败。
3. dvda-bridges/build.rs 增加 aarch64-linux-android 分支：
   读 DVDA_FFMPEG_PREFIX，静态链接 6 个 .a；ABI 布局探测用 host gcc 编
   layout-probe.c 在本机跑（aarch64 LP64 布局一致）。
4. 修 media.rs 6 处 c_char 符号性（Windows i8 / Android u8）指针转型。
5. win.rs：kernel32/shell32 声明加 cfg(windows)，补 unix 版 module_path/workers。
6. dvda-menu/resources.rs：MinGW dirent 断言改 cfg(windows)，c_char 转型；
   Windows CRT/heap 函数（_errno/_wfopen/_wopen/_close/HeapAlloc/HeapFree）补 unix 实现。
7. dvda-native：files.rs 的 MoveFileExW/CopyFileW 换 std::fs 实现；
   media.rs loaded_dependencies 在 unix 返回空表；process.rs（Win32 作业对象）整模块 cfg(windows)。
8. 结果：dvda-bridges media 版成功产出 Android .so
   libdvda_bridges.so 4.4MB（release），FFmpeg 静态链入，
   NEEDED 只有 libm.so/libdl.so/libc.so（bionic）。

### dvda-core 还差什么（14 个文件）
- FILE_FLAG_SEQUENTIAL_SCAN 等 custom_flags(0x04000000)：9 个文件，unix 改成 no-op 即可
  build.rs、cache.rs、encoder.rs、mlp_stream.rs、mlp_workflow.rs、pcm.rs、
  preparation/alac.rs、preparation/state.rs、workspace.rs
- Windows 文件身份（volume serial + file index）→ unix 用 dev+inode：
  identity.rs、workspace.rs、metrics.rs
- 磁盘剩余空间 GetDiskFreeSpaceEx → statvfs：disk_space.rs
- 进程内存/优先级/运行时信息（Win32）：runtime.rs
- UTF-16 宽路径转换：menu.rs、disk_space.rs
- 另外 dvda-cli/build.rs、dvda-desktop/build.rs 仍断言 Windows，GUI 是 Win32/GDI

### 结论
媒体链路（FFmpeg 解码 + soxr 重采样 + 媒体桥 ABI）已经证明能在 Android 上编译链接。
剩下是 dvda-core 的平台细节清理（约 14 个文件，机械改动）+ dvda-cli 入口 + GUI。

## 第三轮：完整链路在 Android 上跑通（2026-11-14）

### 重大发现
这台手机的 proot 环境里能直接运行 Android(bionic) 可执行文件：
/system/bin/linker64 可见，所以交叉编译的 Android 程序可以当场运行验证。

### 已完成
1. 写了最小流水线（绕开 dvda-core 的 Windows 依赖）：
   FLAC --dvda-native::media(FFmpeg 解码+soxr)--> WAV
        --dvda-mlp::encode--> MLP
        --dvda-author::command::run--> DVD-Audio ISO
2. Android aarch64 可执行文件 pipeline 编译成功并在手机上实跑：
   - 3 秒 48kHz/24bit 立体声 FLAC -> ISO 1.37MB，MLP 编码 0.16s
   - 60 秒 96kHz/24bit 立体声 FLAC -> ISO 25MB，MLP 编码 4.56s
   - 全流程墙钟 5 秒（含解码、重采样、MLP 编码、ISO 写出）
3. ISO 验证：ISO9660 CD001 有效，卷标 ANDROID，含 AUDIO_TS / VIDEO_TS / AMG
4. 同一个 crate 也产出 Android .so：libdvda_shim.so 9.2MB，
   导出 dvda_make_iso（FLAC->ISO）和 dvda_selftest，
   NEEDED 只有 libm.so / libdl.so / libc.so，FFmpeg 全部静态链入。

### 距离“能安装的安卓 App”还差
- JNI/Kotlin 或 Flutter 外壳：选文件、显示进度、取消、写设置
- 菜单/封面/多专辑分盘：需要接 dvda-menu + dvda-image（已在 aarch64 编译通过）
  + 打包 Noto CJK 字体；菜单视频/静图编码走 FFmpeg 静态库
- dvda-core 的平台细节（14 个文件的 Win32 调用）可以完全绕开，
  自己写编排层，或者后续再移植
- 打包：libdvda_shim.so + Kotlin UI，APK 体积大约 12-15MB

## 第四轮：Flutter 外壳（2026-11-14）

- 交付：dvda-flutter-shell.zip（lib/main.dart、lib/dvda_engine.dart、pubspec.yaml、
  android/app/src/main/jniLibs/arm64-v8a/libdvda_shim.so（7.1MB stripped）、README.md）
- 架构：Flutter UI -> dart:ffi -> dvda_make_iso(flac, iso, rate, bits) -> ISO
  编码在独立 isolate 跑，UI 不卡；中间文件写在输出目录旁，避免 /data/local/tmp 不可写
- 注意：这台手机装不了 Flutter（官方 Linux SDK 只有 x64，Android 构建工具也是 x64），
  所以 APK 必须回电脑上 flutter build apk；代码和 .so 都已备好
- 体积预估：APK 25-35MB（.so 7.1MB + Flutter 运行时）

## 第五轮：菜单 + 封面接进 C ABI（2026-11-14）

新增 C ABI：
  dvda_make_iso_menu(flac, iso, rate, bits, title, font, background, cover) -> uint64
  - title/font 必填，background/cover 可传 NULL（纯色背景）
  - 走项目自带 Rust 菜单：dvda-author 排版 -> menu_backend 出 Y4M
    -> FFmpeg 静态库编码 MPEG-2 + mp2 -> dvda-menu 生成 SPU/导航
  - dvda-image 负责 PNG 解码、缩放、Y4M 写出

FFmpeg 需额外启用：--enable-encoder=mpeg2video,mp2 --enable-muxer=mpeg2dvd
（已有 mlp/flac/alac 解码和 swscale/swresample/libsoxr）

实测（本机 Android 二进制 + dlopen .so 两种方式都验证）：
  FLAC 3 秒 48k/24 -> 带菜单 ISO 1.50MB（无菜单 1.37MB）
  ISO 内新增：AUDIO_SV.VOB / AUDIO_SV.IFO / AUDIO_TS.VOB，
  MPEG-2 序列头解析为 720x576、25fps、4:3，AUDIO_TS.IFO 从 6KB 变 8KB
  C ABI 直接调用返回 1501184 字节，说明 dlopen + FFI 通路完全可用

踩坑记录：
- bionic 没有 __errno_location，要用 __errno；libc 的 free/fopen/open/close/lseek
  需要用 #[link_name] 映射，否则 .so 能链接但 dlopen 报找不到符号（cdylib 默认允许未定义符号）
- cdylib 链接成功不代表可加载，必须真 dlopen 一次验证

Flutter 外壳同步升级：菜单开关、标题输入、封面选择、字体自动回退。

## 第六轮：原生 Android 17 外壳 + 音乐库分盘（2026-11-14）

### 后端（Rust）
新增音乐库规划器 planner.rs：
- 扫描音乐库根目录，每个子文件夹 = 一个专辑（按文件名排序音频文件）
- 用 FFmpeg Probe 读取每个音轨时长/声道，估算 MLP 后体积（约 0.63 x PCM）
- 按容量（DVD5 4.7GB / DVD9 8.5GB / 自定义字节数）装盘，专辑不拆开
- 每张盘：逐轨解码 -> 重采样 -> MLP 编码 -> 组 -> 菜单（封面作背景+播放封面）-> ISO
- 专辑封面自动发现：cover.jpg / cover.jpeg / cover.png / cover.webp / folder.jpg

新增进度/取消/错误 C ABI：
  dvda_build_library(...)  阻塞构建，返回盘数，-1 失败
  dvda_progress_percent() / dvda_progress_text(buf,size) / dvda_cancel_build()
  dvda_last_error(buf,size)

新增 JNI（jni crate，符号 Java_com_dvda_maker_NativeEngine_*）：
  buildLibrary / progressPercent / progressText / cancelBuild / lastError

### 界面（原生 Android 17）
apps/android/：Kotlin 2.2 + Jetpack Compose Material3
- compileSdk/targetSdk 37，minSdk 24，只打 arm64-v8a
- SAF 目录选择（映射 primary 存储到真实路径）+ 路径输入框
- DVD5/DVD9 容量、采样率/位深、菜单开关、标题、字体路径
- 构建在 IO 线程，UI 每 300ms 轮询进度，支持取消
- 输出目录 ISO 列表，失败显示 lastError
- .github/workflows/android.yml：x64 runner 上自动 assembleRelease

### 实测（本机直接跑 Android 二进制）
- 两个专辑 + DVD5 容量 -> 1 张盘 15MB（两个 group，两段菜单背景，封面仍图）
- 同样两个专辑 + 11MB 容量 -> 自动分 2 张盘（1.5MB + 14MB）
- 取消：后台线程调用 dvda_cancel_build，下一个安全点返回 -1

### 本轮抓到的真 bug
同一个盘里放多个专辑时，--background 传参方式错了：
菜单选项解析是按逗号拆分的一个字符串，不是多个参数。
把第二张封面当成音频输入 -> "Input must be integer WAVE or DVD MLP"。
修复：paths.join(",") 作为一个参数传入。

### 距离完整对齐还差
- 专辑索引页（--index-pages，12 格封面网格）
- 断点续制、成品验证
- 菜单背景视频（--background-mpg）
- 批量队列与历史记录、通知栏进度

## 第七轮：索引页 + 成品 lossless 验证（2026-11-14）

### 索引页（对齐 Windows 的专辑索引）
- 同一张盘 >= 2 个专辑且都有封面时，自动启用：
  --index-pages N（N = ceil(内容页数/12)）+ --index-covers 封面清单文件
  + --screentext "标题=索引1=...:专辑=曲目1,曲目2,..."
- 内容页每页 29 轨（有索引页时 MAX_ROWS-1），索引页 12 格，带缩略图和跳转
- 实测：3 个专辑 1 张盘
  - 有索引：AUDIO_TS.VOB = 233472 字节
  - 关掉索引（DVDA_NO_INDEX=1）：AUDIO_TS.VOB = 165888 字节
  差值 67584 字节 = 索引页那段 MPEG-2 菜单视频，说明索引页真的编进去了
- 构建失败保护：索引页的 screen_text 页数/曲目数不匹配时 author 会直接报错

### 成品验证（lossless）
- 新写最小 ISO9660 读取器（shim/src/iso_reader.rs）：PVD -> 根目录 -> AUDIO_TS
  -> ats_*.aob 流式 128KB 分块（处理 ;1 版本后缀）
- 用 dvda-native::disc_verify::NativeDiscVerifier 逐轨对比 AOB 载荷与 MLP 源
- 构建流程里自动验证（LibraryOptions.verify / ABI verify 参数 / App 开关）
- 独立验证接口：dvda_verify_iso(iso, sources_semicolon) -> 0/-1
- 实测：
  - 正常 ISO：3 轨 / 14077812 字节 全部通过
  - 翻转 AOB 中 1 个字节：验证失败
    "DVD audio payload verification failed: status -5, track 1, offset 97872, sectors 48"
- 失败时保留中间工作目录，便于排查

### 本轮修掉的 bug
- ISO9660 记录边界判断写成了 at+33+name_len > record.len()，导致根目录永远解析为空
- AOB 文件名带 ISO9660 版本后缀 ";1"，必须先剥掉再匹配 ats_XX_Y.aob

### 仍然没对齐的行为（下一轮候选）
- 断点续制：已完成轨道/盘的跳过与恢复（Windows: 继续未完成的制作）
- 菜单背景视频（--background-mpg）、still_vob/top_vob
- 批量队列、历史任务、通知栏进度、详细日志文件
- 验证的其它模式：容量、时间线、菜单结构（当前只有 lossless）
- 多语言界面/日志（Windows 支持 5 种语言）

## 第八轮：断点续制 + 容量/菜单结构验证（2026-11-14，goal 2/256）

### 断点续制（对齐 Windows 的“继续未完成的制作”）
- 状态文件：输出目录/.dvda-resume.json（版本、音乐库路径、采样率/位深/容量/菜单/标题/字体、每张盘的专辑/音轨/ISO/完成/已验证）
- 每编码完一条音轨就保存一次状态；每完成一张盘再保存
- 恢复规则：
  - 选项与音乐库路径不一致 → 不恢复，从头来
  - 已完成且已验证的盘 → 直接跳过（“第 N 张盘已存在，跳过”）
  - 已编码的 MLP（源路径 + 源文件大小 + MLP 大小都匹配）→ 复用（“复用 01 - 甲曲1”）
  - 全部盘完成且验证通过 → 删除状态文件和 .dvda-build-* 工作目录
- 实测：2 张盘、每张 2 条 60 秒 96k 曲目
  - 中途 kill -9：状态文件保留，disc0 done=true，disc1 只有 1 条轨
  - 重启 resume=1：跳过 disc0、复用 disc1 已编码轨、只重做剩余轨，最终 2 张盘全部验证通过

### 验证扩展（对齐 Windows 的多种检查）
- 新增 dvda_verify_iso_mode(iso, sources, mode, capacity, menu)
  - mode 0 = lossless（AOB 载荷 vs MLP 源）
  - mode 1 = 容量 + 菜单结构
- 构建流程里在 lossless 之后再自动做容量与菜单结构检查
- 实测：
  - 容量传 1000 字节 → “Image payload is 14958592 bytes, above the configured capacity 1000”
  - 无菜单盘按“应有菜单”检查 → “Menu expected but AUDIO_SV.VOB is missing”
  - 正常盘两种检查全部通过（3 专辑单盘、带菜单；以及菜单关闭的盘）

### ABI 变更
dvda_build_library(music_root, output_dir, rate, bits, capacity, menu, title, font,
                   verify, resume, progress, context)
- JNI: Java_com_dvda_maker_NativeEngine_buildLibrary 增加 verify/resume 两个 boolean
- buildlib 命令行参数：buildlib <音乐库> <输出目录> [容量] [采样率] [位深] [菜单] [续制]
- Kotlin 界面新增“继续未完成的制作”开关

### 仍未对齐的行为
- 菜单背景视频（--background-mpg / top_vob / still_vob）
- 批量队列、历史任务、通知栏进度、详细日志文件
- 验证的时间线模式（PTS/时间戳单调性）
- 5 种界面语言、设置持久化到文件、每盘成品查看/播放

## 第九轮：菜单背景视频 + 详细日志（2026-11-14，goal 3/256）

### 菜单背景视频 / 自定义 VOB
- LibraryOptions 新增 background_mpgs / top_vob / still_vob
  -> 传给 author：--background-mpg（逗号列表，按页循环）
     --topvob / --stillvob（替换顶层菜单与静止画面）
- 有背景视频时不再用封面现场编码菜单视频，但播放封面仍走 --stillpics
- 新 C ABI：dvda_build_library_media(... verify, resume, background_mpgs, top_vob, still_vob, progress, ctx)
  旧的 dvda_build_library 保留为不带媒体参数版本
- JNI：Java_com_dvda_maker_NativeEngine_buildLibraryMedia
- Kotlin：新增“选择背景视频”按钮 + 路径输入框
- 实测：单专辑带背景视频构建成功；AUDIO_TS.VOB 从 53248 变成 65536 字节
  （直接采用传入 MPG，不再现场编码封面视频），结构验证通过

### 详细日志（对齐 Windows 的会话日志）
- 每次构建在输出目录写 dvda-<unix时间戳>-<pid>.log
- 内容：[INF]/[ERR] + 相对时间 + 阶段文字（扫描/解码/编码/制作/验证/完成）
- 只保留最近 3 个会话日志（rotate_logs 删除更早的）
- 实测：同一目录连续构建 4 次 -> 正好剩 3 个日志；最新日志含
  “0.5s build finished: 1 disc(s)”

### 本轮修掉的 bug
- 扩展 ABI 里 topVob/stillVob 传空字符串时被当成真实路径（Some("")），
  author 打不开 -> “No such file or directory”。已改为空串/空白视为未设置。

### 仍未对齐（下一轮候选）
- 批量任务队列、历史任务列表、通知栏进度
- 设置持久化（Windows: settings.json 便携保存）
- 5 种界面语言、成品查看/播放、详细日志切换
- 时间线验证模式（PTS 单调性）

## 第十轮：settings.json + history.json + 时间线验证（2026-11-14，goal 4/256）

### 设置持久化（对齐 Windows 的 settings.json）
- 每次构建在输出目录写 settings.json：
  Version / MusicRoot / SampleRate / Bits / Capacity / Menu / Title / Font /
  Verify / Resume / BackgroundMpgs / TopVob / StillVob
- Kotlin App 启动时读取该文件回填界面（SavedSettings + loadSettings）
- 实测：/tmp/full3/settings.json 内容与构建参数一致

### 历史记录（对齐 Windows 的会话/任务记录）
- 输出目录 history.json 追加每次构建结果：
  时间戳、音乐库、成功/失败、每张盘的 ISO/字节/专辑列表，失败带错误原因
- 最多保留 50 条
- 实测：成功构建后 history.json 最后一条 Success=true，含 1 张盘的路径与字节

### 时间线验证（Windows 的 timeline 模式）
- 解析 AOB 每个 2048 字节扇区里的 PES（00 00 01 BD），读取 PTS 33 位字段
- 逐组检查 PTS 单调不回退，跨 AOB 分段累计扇区
- 校验 PTS 标记位与前缀（0x2x/0x3x），MLP 末包缺时间戳时跳过
- 新增 dvda_verify_iso_mode 的 mode 2；构建流程自动执行
- 实测：
  - 正常盘 OK（时间线模式通过）
  - 把某扇区 PTS 改成最大值：timeline jumps backwards at sector 12: 536870911 -> 9398
- 本轮修掉两个解码 bug：PTS 第 3 字节少了一次 >>1（时间翻倍），
  以及 MLP 末包缺时间戳被误读

### 仍未对齐（下一轮候选）
- 批量任务队列与任务列表 UI、通知栏进度
- 5 种界面语言
- 成品查看/播放（App 侧）
- 详细日志开关、按时间清理历史

## 第十一轮：五种语言 + 批量队列 + JNI 修漏（2026-11-14，goal 5/256）

### 五种界面语言（对齐 Windows 的界面语言）
- 新增 src/lang.rs：zh-CN / zh-TW / en / ja / ko 进度与日志文案
  覆盖 扫描/恢复/解码/编码/复用/制作/验证/结构/时间线/完成/跳过/补验/通过/完成提示/队列任务
- LibraryOptions.language -> 进度回调、日志头、日志完成行全部本地化
- ABI：dvda_build_library_media 增加 language 参数；JNI buildLibraryMedia 增加 language
- Kotlin：界面加语言 FilterChip（5 个）
- 实测：同一次构建分别用 en/ja/ko/zh-TW，日志分别为
  “Scanning albums / Build finished”“アルバムをスキャン中 / 構築完了”
  “앨범 스캔 중 / 빌드 완료”“掃描專輯 / 建置日誌”

### 批量队列（对齐 Windows 的批量任务）
- queue.json：{"Jobs":[{MusicRoot, OutputDir, Capacity, Rate, Bits, Menu, Title,
  Font, Verify, Resume, BackgroundMpgs, TopVob, StillVob, Language}]}
- 新 ABI dvda_run_queue(queue_path, progress, ctx) -> 成功任务数
  单任务失败不阻断后续；每个任务独立 settings.json / history.json / 日志
- JNI：Java_com_dvda_maker_NativeEngine_runQueue
- Kotlin：加“运行 queue.json 队列”按钮
- 实测：2 任务（英文单专辑 + 日文三专辑）一次运行 jobs_succeeded=2，
  两个输出目录各自出 ISO，history 各 1 条 Success=true，日志语言各自正确

### 修掉一个真 bug（上一轮遗留）
- Kotlin 早已声明 buildLibraryMedia，但 Rust JNI 里只有 buildLibrary，
  运行时会 UnsatisfiedLinkError；本轮重写 jni.rs：
  buildLibrary / buildLibraryMedia / runQueue / progress* / cancelBuild / lastError
  全部对齐，并删掉误加到 buildLibrary 上的 language 参数。

### 仍未对齐（下一轮候选）
- 通知栏进度（Android 前台服务）
- 成品查看/播放、详细/简洁日志切换
- 任务队列 UI（目前只运行固定 queue.json）
- 菜单字体选择器、多语言菜单字体（CJK 5 语）

## 第十二轮：详细日志 + 状态文件 + 成品查看 + CJK 菜单字体（2026-11-14，goal 6/256）

### 详细日志开关（对齐 Windows 的“详细日志”）
- LibraryOptions.verbose；Progress trait 新增 detail()，LogProgress 里以 [DBG] 写出
- 详细内容：专辑数/容量/采样率/位深/菜单/验证开关、每个专辑的轨数/估算体积/封面、
  每轨源大小与 MLP 大小
- ABI media 增加 verbose 参数；buildlib 增加 verbose 位置参数；Kotlin 加“详细日志”开关
- 实测：verbose=1 的日志有 3 条 [DBG]，关闭时 0 条

### 进度状态文件（给通知栏/前台服务轮询）
- 每次进度更新写输出目录 status.json：{"Percent","Text","Updated"}
- 完成后 Percent=100 并写完成文案；失败写 Percent=-1 + 错误
- 实测：构建中读取到 {"Percent":0,"Text":"解码 01 - 乙曲1"}，
  完成后 {"Percent":100,"Text":"Build finished: 1 disc(s)"}

### 成品查看（对齐 Windows 的“查看成品”/验证信息）
- 新 ABI dvda_inspect_iso(iso, buffer, size)：返回 JSON
  {Size, Payload, Groups:[...], HasMenu, Files:[{Name,Size}]}
- JNI Java_com_dvda_maker_NativeEngine_inspectIso；Kotlin 加“查看成品信息”按钮
- 实测：CJK 双专辑盘返回 Groups=[1,2]、HasMenu=true 与 AUDIO_TS 文件清单

### 菜单 CJK 字体（对齐 Windows 的菜单多语言字体）
- ABI media 增加 font_jp / font_kr；planner 传 --fontname-jp / --fontname-kr
- author 的 font_for() 会按曲目名的假名/谚文自动选字体
- 实测：日语专辑（さくら）+ 韩语专辑（봄날）同盘构建成功（2.3MB），
  结构验证与时间线验证都通过

### 仍未对齐（下一轮候选）
- 通知栏前台服务（后端 status.json 已就绪，差 Kotlin Service + 通知）
- 队列 UI（编辑/排序/删除任务）、任务列表历史查看
- 成品播放（需要解码 VOB/菜单预览）

## 第十三轮：LPCM 编码 + TPDF 抖动 + 检查音源（2026-11-14，goal 7/256）

### LPCM 编码（对齐 Windows 的“LPCM 编码”选项）
- LibraryOptions.encoding: Mlp | Lpcm；LPCM 直接把解码后的整数 WAV 交给 author，
  不经过 MLP 编码；限制 16/24bit（20bit 报错，与桌面版一致）
- LPCM 跳过 lossless 载荷校验，但仍做结构与时间线校验
- ABI media 增加 encoding；buildlib 增加编码参数；Kotlin 加 MLP/LPCM 选项
- 实测：LPCM 24bit 单专辑 AOB 884736 字节（同源 MLP 是 655360），
  结构+时间线 OK；LPCM 16bit 也成功；LPCM+20bit 返回
  “LPCM requires 16 or 24 bit output; use MLP for 20 bit”

### TPDF 抖动（对齐 Windows 的 16bit 抖动）
- LibraryOptions.dither 传给媒体解码请求（默认开）
- 实测：同样 16bit 参数，dither 开/关产出的 ISO SHA-256 不同
  （2af821dd… vs a3c19f5a…）

### 检查音源（对齐 Windows 的“检查音源”预检）
- 新 ABI dvda_check_library(music_root, capacity, menu, buffer, size) -> JSON
- JSON：{Ok, Albums:[{Name,Cover,EstimatedBytes,Tracks:[{Name,Duration,Channels,Ok,Issue}]}],
  Errors, Warnings}
- 封面图等已知图片文件不再误报“不支持的文件”
- JNI checkLibrary；Kotlin 加“检查音源”按钮
- 实测：
  - 好库：Ok=true，Warnings 为空
  - 坏库（容量 1000、含 readme.txt、无封面）：Ok=false，
    Errors=[estimated 548416 above capacity 1000]，
    Warnings=[unsupported file readme.txt, no cover image found]
  - 空库：Ok=false，Errors=[No album folders with FLAC/M4A files found]

### 修掉的 PCM 对齐 bug
- dvda-mlp 只接受 24bit 左对齐样本（16bit 要 <<8、20bit 低 4 位清零），
  上一轮 16bit 路径会 InvalidPcm；现统一在 read_wav 输出 24bit 容器，
  目标位深交给 profile 决定，16/20/24bit 全部通过

### 仍未对齐（下一轮候选）
- 通知栏前台服务（后端 status.json 已就绪）
- 队列任务编辑 UI、历史任务列表查看
- 成品播放/菜单预览

## 第十四轮：菜单制式/宽高比 + 下混/通道分配 + 保存方案（2026-11-14，goal 8/256）

### 菜单制式与宽高比（对齐 Windows 的 NTSC/PAL 与画面比例）
- LibraryOptions.norm（pal/ntsc/secam）+ aspect（1..4），传 author 的 --norm/--aspect
- ABI media 增加 norm/aspect；buildlib 增加参数；Kotlin 加 PAL/NTSC 与宽高比 1-4 选择
- 实测：NTSC 菜单视频解析为 720x480、fpscode 4（29.97）；
  PAL 为 720x576、fpscode 3（25）

### 下混与通道分配（对齐 Windows 的 downmix / CGA）
- LibraryOptions.downmix（12 个 f32 系数 -> --downmix + --dtable）、cga（--cga）
- cga 必须是 author 通道表里对应声道数的编号：
  CHANNELS=[1,2,3,4,3,4,5,3,4,5,4,5,6,...]，6 声道用 12
- 实测：6 声道专辑（cga=12 + 12 系数表）构建成功；
  与无下混版本 ISO 逐字节比较有 386 字节差异，说明下混表写进了 IFO；
  cga 与声道数不匹配时报 “CGA does not match the input channel count”

### 保存方案 / 打开方案（对齐 Windows 的保存/打开方案）
- 新 ABI：dvda_save_profile(path, json) / dvda_load_profile(path, buf, size)
- JNI saveProfile/loadProfile；Kotlin 加“保存方案/打开方案”按钮，
  把当前界面设置序列化成 profile.json 并可读回
- 实测：save 返回 0，load 原样读回 {"Version":1,"Title":"方案 A","Rate":96000}

### 本轮修掉的坑
- 6 声道测试源必须是 WAVE_FORMAT_EXTENSIBLE，否则 flac 拒绝编码
- cga 不是随便填的；必须匹配 CHANNELS 表（6ch -> 12）

### 仍未对齐（下一轮候选）
- 通知栏前台服务（status.json 就绪）
- 队列任务编辑 UI、历史任务列表
- 成品播放/菜单预览

## 第十五轮：历史/日志/队列查看 + 通知栏前台服务 + 队列进度（2026-11-14，goal 9/256）

### 新增查看类 ABI（对齐 Windows 的历史任务/日志查看/保存日志）
- dvda_read_history(output_dir, buf, size) -> history.json 文本
- dvda_read_log(output_dir, buf, size) -> 最新 dvda-*.log 内容
- dvda_save_log(output_dir, dest) -> 复制最新日志到目标（Windows 的“保存详细日志”）
- dvda_check_queue(queue_path, buf, size) -> {"Ok","Jobs":[{Index,MusicRoot,OutputDir,Albums,Issues}]}
- JNI：readHistory/readLog/saveLog/checkQueue；Kotlin 加“任务历史/查看日志/保存日志/校验队列”按钮
- 实测：history 返回历史数组；readLog 返回最新日志（含 [DBG]）；
  saveLog 复制成功；checkQueue 对 2 个任务返回 Ok=true、Albums=1/3

### 队列进度文件（给通知栏用）
- 运行 queue.json 时在 queue.json 同目录写 queue_status.json：
  {Job, Total, MusicRoot, FinishedJob, Updated}
- 实测：运行中 {"Job":1,"Total":2,"FinishedJob":false}；
  结束后 {"Job":2,"Total":2,"FinishedJob":true}

### 通知栏前台服务（Kotlin）
- 新增 BuildNotificationService：前台服务轮询 status.json，
  显示百分比进度通知，完成或失败后自动停止；MainActivity 在开始构建时启动、结束时停止
- 队列校验/进度：服务同样可轮询 queue_status.json（已具备数据源）
- AndroidManifest 注册 service（foregroundServiceType=dataSync），
  build.gradle 增加 core-ktx 与 kotlinx-coroutines-android，
  启动时申请 POST_NOTIFICATIONS 权限

### 本轮修掉的问题
- 队列任务没写 Font 且开启菜单时直接失败（menu enabled without a font path）；
  现在自动回退到系统 NotoSansCJK / DejaVu —— 2 个任务全部成功（51MB + 1.4MB）
- 队列结束状态原来写成 Job=Total+1，已改为 Job=Total 且 FinishedJob=true

### 剩余差距（下一轮候选）
- 队列编辑 UI（真正增删/排序任务，目前是直接运行固定 queue.json）
- 成品播放/菜单预览（需要解码 VOB 或生成缩略图）
- 详细/简洁日志切换的持久化（目前开关不写入 settings.json）

## 第十六轮：队列编辑 + 详细日志持久化（2026-11-14，goal 10/256）

### 队列编辑（对齐 Windows 的批量任务增删/排序）
- 新 ABI：
  dvda_queue_add(queue, jobJson) / dvda_queue_remove(queue, index)
  dvda_queue_move(queue, from, to) / dvda_queue_clear(queue) / dvda_queue_count(queue)
- 队列文件不存在时视为空队列（clear/add 可直接创建）
- JNI：queueAdd/queueRemove/queueMove/queueClear/queueCount
- Kotlin：加“加入队列（当前设置一键入队）/ 清空队列 / 队列数”按钮
- 实测：clear 空文件 -> add 两个任务 -> count=2 -> move 0,1 顺序互换
  -> remove 0 剩 1 个 -> 再 add 回来 -> 运行 jobs_succeeded=2（1.4MB + 15MB ISO）

### 详细日志开关持久化
- settings.json 增加 "Verbose" 字段；Kotlin 保存/打开方案也会带上 Verbose 与背景视频路径
- 实测：构建后 settings.json 里能看到 "Verbose": false

### 修正
- 之前 queue_status.json 只写在 queue.json 同目录，测试路径看错了；已确认文件位置与内容
- 队列结束后状态为 {"Job":2,"Total":2,"FinishedJob":true}

### 剩余差距（下一轮候选）
- 成品播放/菜单预览（最大的一块，需要解码 VOB 或生成缩略图）
- 队列任务的图形化列表（增删按钮已有，列表展示仍靠 checkQueue 文本）
- 安装包签名/发布流程（Android 侧）

## 第十七轮：按 README 核对 Windows 行为 + 自动检查音源 + 递归扫描（2026-11-14，goal 11/256）

### 依据 README 纠正的“对齐”范围
用户确认：Windows 既不能播放成品，也没有磁盘空间检查/成品预览行为，
因此这三项不作为对齐目标（预览代码已删除，未实现空间检查）。

### 自动检查音源（README：“开始制作会自动检查音源；检查失败会阻止制作”）
- build_library 开始时先跑 check_library_with(rate,bits)，Ok=false 直接返回
  “Source check failed: …”，不产出任何 ISO
- 实测：坏库（容量 1000）报 above capacity 并阻止；超容量专辑（100000 字节容量）同样阻止；
  正常库不受影响

### 递归扫描子文件夹（README：“扫描其中的 .flac 和 .m4a 文件，包括子文件夹”）
- album_directories() 递归（深度 6）查找“直接包含音频文件的目录”，每个目录一个专辑
- 实测：/流派/专辑/ 两级目录库识别出 2 个专辑（专辑二、专辑一），构建出 2.3MB ISO
- 扁平库回归正常

### 其他小对齐
- settings.json 增加 "Language"，App 启动时恢复上次选择的语言
- App 增加“查看成品（打开 ISO 文件夹）”按钮（系统文件管理器）
- 容量估算改保守（PCM x0.72 + 1MB/轨），避免“规划能装下、实际 ISO 超容量”：
  实测 11MB 容量会正确在检查阶段拒绝 60 秒 96k 专辑；16MB 容量分 2 张盘，
  每张都通过容量/结构/时间线验证
- 取消保留成品：中断后第一张 ISO 保留，resume 后补齐第二张（已复测）

### 仍然存在的差距（诚实清单）
- App 界面文字目前只有中文；Windows 支持 5 种界面语言（日志/进度已 5 语，
  Kotlin 标签尚未资源化）
- 队列任务列表还是文本（按钮已有增删/清空/计数）
- APK 签名与发布流程未做

## 第十八轮：按你的要求删掉 shim，直接编译真正的 dvda-core（2026-11-14）

### 结论
之前"逐功能对齐"是因为 shim 绕开了 dvda-core；本轮改为直接移植真代码：
**dvda-core 已在 aarch64-linux-android 编译通过（debug + release），并在手机上真跑起来。**

### 改动（只做平台分支，不动业务逻辑）
新增 rust/crates/dvda-core/src/platform.rs（90 行）：
- OpenOptionsExt/MetadataExt 的 Unix 实现（share_mode/custom_flags 空操作；
  file_size/last_write_time/file_attributes/file_index/volume_serial 用 Unix 元数据）
- 宽路径编解码（wide/from_wide）

按平台拆分/替换的 dvda-core 文件（共 14 个）：
encoder.rs、mlp_stream.rs、mlp_workflow.rs、pcm.rs、preparation/alac.rs、
preparation/state.rs、cache.rs、metrics.rs、workspace.rs、identity.rs、
disk_space.rs（Unix 返回空建议）、runtime.rs（Android 静态链接，无解包）、
process.rs（Android 进程内 authoring，保留 Outcome 类型 + 明确的不支持错误）、
encoder_stamp.rs（Unix 机器指纹与 UTC 时间戳替代 GetComputerNameW/GetLocalTime）

### 设备实测（bionic 二进制直接在手机跑）
examples/android_smoke：
- options::defaults -> 25 个真实设置键
- planner::plan -> discs=1、groups=3（按 采样率/位深 分桶）、
  DiscContentLimitBytes=4698931200 = 4_707_319_808 − 8 MiB（与 Windows 完全一致）
- menu::plan -> pages=6、index_pages=1（6 张专辑页 + 1 张索引页，最小专辑数 4 规则生效）
- app::AppOptions::load -> language=zh-CN、title=DVD-Audio、
  DiscBytes=4707319808、MlpSource=mlp-batch（Windows 默认值）

### 已删除
- /root/workspace/dvda-port/shim（797MB，24 个 rs 文件、3775 行自写 shim）
- android_app 里由 shim 编出的旧 libdvda_shim.so
- Downloads 里的旧二进制/示例盘保留作历史记录，不再维护

### 下一步（不再写新逻辑）
1. 新建薄 JNI crate（只做 Kotlin<->Rust 绑定），直接调：
   AppOptions::load、run_prepare（=检查音源）、run_build、run_verify
2. 用它重建 App 的 .so，替换旧 ABI；App 设置 JSON 直接喂 AppOptions
3. 这样标签管线、两阶段规划、5 种验证、resume 签名、菜单设置、发布流程全部来自真代码

### 修改文件清单
git status: dvda-core 20 个文件改动 + 新增 platform.rs、examples/android_smoke.rs；
另有此前为 Android 做的 dvda-bridges/dvda-menu/dvda-native/dvda-author 兼容补丁。

## 第十九轮：真 dvda-core 全流程在 Android 跑通 + 薄 JNI 绑定

### 结果
用真正的 Windows 代码（不是 shim）在手机上完整跑通：
prepare（标签/解码/重采样/缓存/manifest/snapshot）→ MLP 批量编码 →
规划 → Rust author 出 ISO → staging 原子发布 → 自动验证（容量/时间线/lossless PCM 对比/菜单检查）

实测（menu on）：
- BUILD ok=true diagnostics=1（只有一条菜单字体 TTC 的警告）
- ISO：/tmp/coretest/final/AndroidCore_1.iso（命名规则 {IsoPrefix}_{盘号}.iso，与 Windows 一致）
- 验证：verify_timeline、verify_pcm(pcm_ok)、verify_menu_page 全部通过
- 控制台是真实 [TASK]/[METRIC] 事件流，不再是 shim 自造日志

### 本轮修的平台差异（不动业务逻辑）
1. 流式 MLP 编码仅在 x86_64 支持实时元数据刷新 → arm64 改走“解码 WAV + 批量编码”；
   批量编码的 live_metadata 也只在 x86_64 开启（静态时间戳）
2. 路径分隔符：manifest.rs / mlp_workflow.rs / options.rs 在非 Windows 用 '/'（Windows 仍是 '\'）
3. 菜单素材目录预检（author_source/data/menu）只在 Windows 检查；Android 菜单资源链接进进程
4. 之前的 platform.rs 平台抽象（文件属性/共享模式/宽路径）继续生效

### 薄 JNI 绑定（新 crate dvda-android，不写业务逻辑）
- 依赖 dvda-core，导出：
  runProfile(profileJson, action) / cancel() / progressDone() / progressTotal() / progressText()
- 产物 libdvda_core.so：release 8.6MB（strip 后），NEEDED 只有 libc/libm/libdl，
  已放入 apps/android/app/src/main/jniLibs/arm64-v8a/，NativeEngine 改为 loadLibrary("dvda_core")
- 设备上 dlopen 验证通过（Android 链接器可加载）
- 旧 shim ABI 声明仍留在 NativeEngine，但已无实现；下一步按 Windows 行为重写 UI 后删除

### 下一步
1. 按 Windows 界面重写 Compose UI：检查音源 / 开始制作 / 验证成品 / 查看成品 / 设置 / 方案，
   调用 runProfile("prepare"|"build"|"verify")，进度轮询 progressDone/Total/Text
2. 删除旧 shim 时代的多余功能（队列/历史/保存日志按钮等 Windows 没有的）
3. 打包 APK（PC/CI）并真机回归

## 第二十轮：按桌面版重写 Android UI + 文案/设置项直接从 GUI 源码取

### 界面
- MainActivity 重写为 Compose，页面与控件照 dvda-desktop：
  基本设置 / 音频编码 / 光盘菜单 三个页签；高级设置开关；
  底部：检查音源 / 开始制作 / 停止任务 / 验证成品 / 查看成品
- 设置项覆盖桌面版全部 profile 键：SRC、FINAL_DIR、TITLE、DISC_BYTES（DVD5/DVD9/自定义）、
  BUILD_DIR、ISO_PREFIX、GROUP_TRACK_LIMIT、MLP_SOURCE（mlp/lpcm/external）、RATE、BITS、
  DITHER_16、MLP_EXTERNAL_DIR、MENU、TRACKS_PER_PAGE、STILLPICS、COVER_DIM、NORM、
  FONT/FONT_JP/FONT_KR、KEEP_TMP、TITLE_MODE、ALBUM_LIMIT
- 方案保存/打开就是桌面版 profile 格式（Version/Language/Values，DVDA_* 字符串，on/off）
- 语言切换用桌面版同样 5 种；界面文案直接从 dvda-desktop/src/i18n.rs 生成
  values/values-zh-rTW/values-en/values-ja/values-ko 各 125 条，XML 已校验
- 删除 Windows 没有的东西：通知栏前台服务、队列/历史/保存日志等按钮

### 验证
- Android 资源：5 个 strings.xml 各 125 条，XML 解析通过
- 设置项比对：Kotlin 生成的 profile 键与桌面版 edited_profile_values 完全一致
- 真机用“和 App 完全相同形状”的 profile（25 键、on/off、系统字体路径）跑通真 core：
  BUILD ok=true，发布 AndroidUI_1.iso（1.3MB），验证含 verify_menu_page 全部通过
- JNI 库 libdvda_core.so 已放入 jniLibs，dlopen 通过

### 说明
- Kotlin 未在本机构建（Android 工具仅 x64）；APK 需在 PC/CI 打，workflow 已带
- 菜单字体用系统 NotoSansCJK TTC，会出现“TTC 只有 face 0”的提醒（真 core 自身诊断）

## 第二十一轮：按 GUI 源码校正设置默认值与序列化，端到端复测

### 从 dvda-desktop 源码逐项校正 UI（不是凭文档）
- DVD9 容量：GUI 用的是 8_540_123_136（不是通用 8.5GB），Kotlin 已改
- 采样率/位深下拉：照 GUI 的 SAMPLE_RATE_LABELS（44.1 kHz…）和 SAMPLE_BITS_LABELS（16 bit…）
- 菜单默认关闭（options.rs DVDA_MENU 默认 off；之前 Kotlin 默认 on，已改）
- 勾选项写 "on"/"off"；保留文件同时写 DVDA_KEEP_TMP 和 DVDA_KEEP_INTERMEDIATE（与 GUI overrides 一致）
- 复用检查/断点（DVDA_RESUME）固定 "on"，与 GUI 强制 ResumeEnabled=true 一致
- 菜单字体为空且启用菜单时回退到系统 NotoSansCJK TTC（否则 core 会去找桌面版内置字体别名）
- 临时工作路径默认 App filesDir/build（桌面版默认 LOCALAPPDATA/build 的对应）
- 查看成品改用 DocumentsUI 的外部存储树 URI（file:// 在 Android 7+ 会抛 FileUriExposed）
- 打开方案/保存方案仍是桌面版 profile 格式，保存时写全量当前值

### 验证（真机、真 core）
- profile roundtrip：用 core 自己的 save_profile 重新保存再加载，13 个关键设置 **mismatches=0**
- 最小 profile 交给 core，解析出的默认值与桌面版默认一致：
  title=DVD-Audio、DiscBytes=4_707_319_808、menu=false、mlp-batch、48000/24
- DVD9 profile：DiscBytes=8_540_123_136 正常加载
- 用“App 默认值形状”的 25 键 profile（菜单关）构建：
  BUILD ok=true、diagnostics=0，产出 AndroidUI_1.iso（1.2MB）

### 说明
- 设备上没有 JDK/Kotlin 编译器（只有 aarch64 用户态，无 java），Kotlin 代码只能在 PC/CI 编译；
  本轮通过资源 XML 校验、键名比对、profile roundtrip、真机 core 复测来交叉验证 UI 数据契约

## 第二十二轮：任务生命周期/未完成制作/多语言日志与桌面版对齐

### 从 GUI 源码补齐的生命周期（task.rs）
- build 流程照桌面版：run_prepare -> run_build -> run_verify -> finish_task
  （之前 build_continue 走 run_build 不会自动验证/收尾，会残留 unfinished 任务）
- 检查音源使用 scratch 工作区（桌面版 scratch_options），检查完就丢弃，不产生未完成任务
- 未完成制作：status 动作查 has_unfinished_task；UI 弹“继续制作 / 重新制作”，
  分别调用 recover_task(false)/(true)
- 失败/取消：弹“保留进度 / 删除进度”，删除调用 discard_task
- 对话框文案按语言逐字取自 task.rs 的 TaskDialog 文案表（5 语言，共 8 条 × 5 份 strings.xml）
- 日志本地化：JNI 用 dvda_core::localization::translate 把 core 日志翻成界面语言；
  新增 logText() 与结果 JSON 的 Log 字段

### 真机验证（真 core，phase 隔离）
- 正常流程：fresh lifecycle ok=true，unfinished=false
- 检查音源：scratch prepare 成功，unfinished=false（不会冒未完成任务）
- 取消：unfinished=true，已完成的 ISO 保留（Windows“已完成成品不删除”）
- 继续制作：recover_task(false) -> prepare/build/verify -> finish，
  结果 ok=true、unfinished=false、ISO 存在
- 取消后直接再 build 会报 “An unfinished task exists...”，与桌面版一致
- 五语言本地化：core 日志在 en/ja/ko/zh-TW 下正确翻译（例：Found 1 audio files…
  / 音声ファイルが 1 件… / 오디오 파일 1개…）

### 产物
- libdvda_core.so 重新构建（8.6MB strip），已更新进 jniLibs
- 新增 gui_lifecycle / task_recovery / localization_smoke 示例二进制用于回归

## 第二十三轮：装 JDK 做真编译 + 高刷 + API 37 类型检查

### 真编译（不再是“无法编译”）
- apt 安装 openjdk-17-jdk-headless（aarch64），修复 dpkg 缺 /usr/share/man/man1 的问题
- 下载 kotlinc 2.2.0 + compose 编译器插件 2.2.0
- framework 用 Android 17（API 37）的 android-all jar（Robolectric 17-robolectric-16653203，221MB）
- 依赖用 Google Maven/Huawei 镜像补齐（Compose 1.8.3、material3 1.3.2、activity 1.10.1 等）
- 生成 R.java（133 个字符串资源）后编译：
  MainActivity.kt + NativeEngine.kt -> 14 个 class，0 error（只有一条 deprecated 警告）

### 编译发现并修掉的真问题
1. attachBaseContext 返回类型错误（ContextWrapper 返回 void，之前写成 : Context）
2. Section(title: () -> String) 改成 String 后函数体还在调 title()，编译错误
3. 高刷实现里 Window.setFrameRate/FRAME_RATE_COMPATIBILITY_DEFAULT 在当前 API 37 jar 不存在，
   改用 preferredDisplayModeId + preferredRefreshRate

### 高刷
- onCreate/onResume 选该屏幕支持的最高刷新率模式（preferredDisplayModeId + preferredRefreshRate）
- 进度轮询用 withFrameNanos 跟随 VSYNC（不再固定 300ms）
- 进度条 animateFloatAsState 插值（120ms）
- Manifest 显式 hardwareAccelerated=true

### API 37
- 项目本来就是 compileSdk/targetSdk 37、minSdk 24
- 类型检查用 API 37 framework jar 完成，不是用旧 API 34 顶替

### 产物
- dvda-ui-classes.jar（编译产物，14 个 class）
- dvda-kotlin-compile.log（编译日志，0 error）
- APK 仍需 PC/CI（aapt2 只有 x86_64），但源码已在本机通过 Kotlin/Compose 编译

## 第二十四轮：在手机上打出可签名的 APK

### 工具链（arm64 原生，不是模拟）
- apt: aapt2 2.19、zipalign、apksigner、android-sdk-build-tools、platform-23、zip
- JDK 17（aarch64）+ kotlinc 2.2.0 + Compose 插件 2.2.0
- R8/D8 8.5.35（Maven 下载的 jar，JVM 跑在 arm64）
- framework：API 37 android-all（类型检查）+ apt 的 android-23（资源链接）

### 流程（build-apk.sh）
aapt2 compile res -> aapt2 link（生成 R.java、base.apk）-> javac R ->
kotlinc（Compose 插件，jvmTarget 1.8）-> jar -> D8（多 dex）-> 加 classes.dex 与
lib/arm64-v8a/libdvda_core.so -> zipalign -p 4 -> apksigner（debug key）-> 验证

### 实测结果
- 生成的 APK：16MB，package com.dvda.maker，minSdk 24，targetSdk 37
- 5 个语言 config（默认 zh-CN、en、ja、ko、zh-rTW），134 个字符串
- apksigner verify 通过；.so 未压缩且页对齐
- 期间修掉真问题：英文字符串里的撇号没转义（aapt2 直接报错）；
  清单缺 package（手动链接需要，Gradle 由 namespace 注入）；
  API 23 框架没有 requestLegacyExternalStorage（只在手打包清单副本里去掉）
- 新增权限处理：检查/请求 MANAGE_EXTERNAL_STORAGE 与 READ_MEDIA_AUDIO，
  因为 native core 要真实路径而不是 SAF URI

### 限制
- 本机没有 adb/pm 权限，不能自动安装；APK 已放 /sdcard/Download/DSH-Go/，
  可手动点击安装（debug 签名，需允许未知来源）
- aapt2 资源链接用的是 API 23 platform，手打包清单去掉了 requestLegacyExternalStorage；
  PC/CI 的 Gradle 构建不受此影响
- AAR 里的额外资源没有合并（Compose 系基本无 XML 资源），若真机运行报缺资源，
  需在 CI 用 Gradle 正式打包
