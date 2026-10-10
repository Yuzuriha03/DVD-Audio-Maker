# DVD authoring 与 ISO writer 的 Rust 实现

更新于 2026-10-10。完整制盘入口位于 `rust/crates/dvda-author/src/command.rs`；应用通过 `dvda-core::author_runtime` 进程内调用，独立 `dvda-author-dev.exe` 复用同一入口。生产构建不编译、链接或启动旧项目 C author。

| 制盘环节 | Rust 实现 |
|---|---|
| 音频输入 | RIFF/RF64/WAVE extensible 整数 PCM；MLP major sync、AU、调度字段解析 |
| AOB | LPCM 16/20/24-bit 多声道重排；MLP pack/PES、PTS/DTS/SCR、EOS、gapless、title 转换、1 GiB 分卷 |
| ATSI / SAMG | 曲目、标题、downmix、静图表、备份；SAMG 按实际 ISO 布局计算绝对地址 |
| AMG / ASVS | 两份标题目录、完整菜单 PGCI/cell、PAL/NTSC 属性、palette、静图索引 |
| 菜单 / 静图 | 专辑页、多个索引页、按钮与导航 XML、分语言字体、图片/VOB 编排 |
| ISO | 有界缓冲的 ISO9660 / UDF 1.02 bridge writer、原子替换、取消 |

`dvda-author` 只依赖 Rust 标准库。图像处理、MPEG-2 编码和 SPU/DVD 导航通过现有 Rust bridge 调用保留的第三方 ImageMagick、FFmpeg 与菜单 vendor；这些第三方代码没有宣称已改写成 Rust。Python 构建/验收脚本和 `pycdlib` 独立读取器只用于开发，不进入 author 的运行时依赖。

## 入口与构建

应用的 `build::execute` 直接调用 Rust author，不再通过 `process::execute` 启动外部制盘程序；旧配置中的 author 路径保留兼容。续跑签名绑定当前程序的完整摘要，避免复用旧 C author 的成品。

`tools/win-build/build-image-author.py` 是 `build-rust-author.py` 的兼容入口。传入现有 FFmpeg、ImageMagick 和静态菜单 vendor 构建目录，生成 release Rust author、第三方 DLL 闭包、字体/菜单素材和 `author-build.json`。该记录声明 `implementation=rust`，校验 Rust 源码、vendor、资源和二进制摘要；发布工具接受这个记录。旧 C producer、源码树和 C oracle 构建入口已删除；历史源码摘要与版权来源保存在 Rust golden fixtures 中，完整源码可从 Git 历史恢复。

```powershell
$env:DVDA_FFMPEG_PREFIX = 'D:/dvda-release-build-20261006/rebuild-current/ffmpeg-shared/install'
$env:DVDA_MAGICK_WORK = 'D:/dvda-release-build-20261006/rebuild-current/imagemagick-minimal'
$env:DVDA_MSYS_ROOT = 'C:/msys64'
$env:DVDA_MENU_NATIVE_DIR = (Resolve-Path build/menu-direct-vendor-rust-session).Path
python tools/win-build/build-rust-author.py --assets-runtime 'D:/dvda-release-build-20261006/release-v1.0-current/prebuilt' --dependency-runtime 'D:/dvda-release-build-20261007/media-native-shared'
# 实际运行菜单时，需使用已装配的第三方运行库、字体与图像配置文件。
build/rust-author-production/dvda-author-dev.exe -g input.mlp -o disc -D author-temp --iso=disc.iso --iso-volume 'DVD AUDIO'
```

输入支持产品预处理后的整数 PCM WAV 和 MLP。FLAC/ALAC 转换及 MLP 编码继续由应用已有 Rust 媒体/编码流程完成。多个 `-g` 表示分组，`-z` 强制新标题；菜单使用产品已有的 `--topmenu`、`--nmenus`、`--background`、`--screentext`、字体、索引与 `--stillpics` 参数。未知参数明确报错。

## 迁移中修正的布局问题

- 生产 `atsi::encode_checked` 将 codec/CGA 计入格式身份；格式属性和标题标志取自对应格式/标题，避免旧 C 在首标题有多轨时错用扁平轨道索引。`atsi::encode` 保留冻结行为用于历史字节差分。
- AMG 标题表超过一个扇区时分别扩容，text 放在 PGCI 之后；读取与验证端跟随 header 指针。
- SAMG 先预留实际大小，再取得 ISO writer 的 AOB 地址；曲目多于固定矩阵容量时扩容八份矩阵，消除旧 C start-sector 估算偏差。
- NTSC 的按钮、缩略图、文字检查按实际 480 行高度缩放，SPU XML 显式声明 PAL/NTSC 时制。
- 菜单 vendor 的 Windows 目录创建改由 Rust UTF-8 路径接口完成，支持中文、日本语临时目录。
- ISO 目录记录不跨扇区，UDF FID 保持连续，ISO path table 按层级/父索引排序。

制盘先写独占 staging 目录，失败清理该目录；仅在成功后提交光盘目录。已有非空输出目录被拒绝。ISO 向独占临时文件流式写入，flush/sync 成功后替换目标；失败或取消保留旧 ISO。如果 ISO 最终提交失败，已提交的光盘目录一并回滚，恢复原有空目录，支持重试。ISO 单文件上限 4 GiB − 1，AOB 每文件 1 GiB，每组最多九个 AOB。

2026-10-11 清理后，Windows 目录发布对短时共享冲突执行有界重试并检查取消；
源码、旧产物和外部 oracle 的清理边界见 [清理记录](RUST-CLEANUP.md)。

## 验收与范围

```powershell
cargo test --manifest-path rust/Cargo.toml -p dvda-author --offline
cargo test --manifest-path rust/Cargo.toml -p dvda-core --lib --offline
cargo clippy --manifest-path rust/Cargo.toml -p dvda-author -p dvda-core --all-targets --offline -- -D warnings
python tools/win-build/test-rust-author-menu.py --help
python tools/win-build/test-rust-iso.py --report build/rust-author-iso/acceptance.json
```

仓库内测试覆盖全文件 ATSI/AMG/ASVS 冻结 C fixtures、真实布局、多组/title、静图复用、菜单 XML、字段溢出、失败清理及实际 ISO payload/绝对地址。AOB 验收使用完整 C 字节对照和独立解码；真实菜单验收使用已装配的 Rust 程序，通过 ISO9660/UDF 提取所有文件、检查表地址与菜单 cell、解码音频和 PAL/NTSC 图像。应用验收使用不存在的外部 author 路径，覆盖预处理、制盘、续跑与成品验证。

完整 author 迁移的基线已通过以下验收，汇总记录位于 `build/rust-author-final-acceptance.json`，各记录绑定当时程序/资源摘要。此后的自有菜单会话迁移及其新生产产物记录见 [菜单会话迁移](RUST-MENU-SESSION-MIGRATION.md)：

| 验收 | 结果 / 记录 |
|---|---|
| Rust 测试 | author 38 项、core 101 项（另 5 项需外部 fixture 的 ignored 测试）、toolchain 28 项；格式检查与相关 Clippy 通过 |
| AOB 差分与解码 | 55 个完整字节对照、60 个独立解码；`build/rust-aob-acceptance-provenance-final/acceptance.json` |
| 全制盘 | 6 个成功场景、10 个失败场景、1,898 条曲目独立解码；891 轨/标题扩容、Unicode、混合 PCM/MLP、ISO9660/UDF 全文件与实际物理地址通过；`build/rust-author-full-final-vendor/acceptance.json` |
| 菜单与静图 | PAL/NTSC 各 13 albums、15 pages、13 stills；全部画面解码、字体、导航和 native verifier 通过；`build/rust-author-menu-production-final/report.json` |
| 单 EXE 产品流程 | 只复制发布 GUI，清除开发覆盖并使用系统 PATH；MLP/LPCM 各两轨实际 build/verify、菜单/静图和损坏运行库自动修复通过；`build/rust-author-onefile-final.json` |

发布候选包为 `build/rust-author-package/DVD-Audio-Maker-v1.0-win-x64.zip`。GUI 内嵌运行时只有 19 个第三方 DLL/配置/字体/素材文件，没有项目 adapter DLL 或开发 author EXE；完整 author 直接静态编入主程序。第三方图像/视频/menu vendor 的边界保持明确，不把这些库称作纯 Rust 重写。

当前编码器界限为 1–9 group、每组 1–99 track、每组最多八种音频格式、1–6 声道；ASVS 固定目录容纳最多 99 个带新图片的标题，不把这个实现界限称作已证明的 DVD 规范上限。LPCM 超过 9.6 Mbit/s 会拒绝并要求先编码为 MLP。软件验收不能代替实体 DVD-Audio 播放器和烧录测试。
