> 历史记录：本文记录菜单会话迁移过程。最终状态是菜单子画面与导航已完全由 Rust 实现，C 菜单 vendor、`session-rust.c`、`build-menu-runtime.py` 与静态 vendor 归档均已删除；文中出现的 vendor 目录/清单只属于迁移阶段。

# 自有菜单 C 代码迁移记录

更新于 2026-10-31。此次迁移覆盖生产链路中剩余的自有菜单会话逻辑与 SPU 子画面；完整 AOB/IFO/ISO author 的迁移见 [Rust author](RUST-AUTHOR-MIGRATION.md)。

| 原 C 职责 | 当前 Rust 实现 |
|---|---|
| 会话请求、输入长度与扇区对齐检查、图片回调、失败清理 | `dvda-menu/src/session.rs` |
| 私有 Win32 heap，malloc/calloc/realloc/free，字符串复制 | `dvda-menu/src/resources.rs` |
| UTF-8 路径转换、CRT 文件打开/关闭、目录枚举 | `dvda-menu/src/resources.rs` |
| FILE/fd/目录/COM 的登记、释放和逆序回收 | `dvda-menu/src/resources.rs` |
| XML/布尔解析、descriptor 分发 | `dvda-menu/src/lib.rs` |
| 序列化、递归调用拒绝 | `dvda-menu/src/direct.rs` |
| 第三方 vendor 全局状态快照和重置 | builder 生成的 `spu/reset.rs`、`nav/reset.rs`，编译为 `no_std` Rust object |

`tools/menu-native/session-rust.c` 保留 C 的 `setjmp` 隔离、可变参数解码和返回值到 C 错误的转换。第三方算法仍会调用 `longjmp`；因此先让跳转在 C 栈帧内完成，再返回 Rust 清理资源。Rust 函数不发起跳转，回调也不得展开或跳转穿过 Rust 栈帧。

迁移阶段使用的静态 vendor 目录为 `build/menu-direct-vendor-rust-session`；更早的 `build/menu-direct-vendor-full-author` 曾用于差分验收，现已清除其中的 C 基线产物；历史验收摘要保留。`menu-build.json` 标识 Rust session/reset，并记录所有 writable vendor section、长度、Rust reset 源码和 object 摘要；生产 author 和打包器拒绝旧 C session/reset 清单或过期 Rust 源码摘要。当前 session 验收无需旧 C，检查两个新 Rust 进程的完整输出一致性。

## 验证

`tools/win-build/test-menu-rust-reset.py` 对真实 `.data`/`.bss`、私有 static、重定位指针和 section padding 做交错重置，并检查 SPU/NAV 互不影响。`tools/win-build/test-rust-menu-session.py` 使用固定 MPEG 和 RGBA 回调对比旧/新完整 SPU 与导航文件，验证同进程失败后成功、Unicode 路径、递归拒绝、并发调用序列化和 heap/handle 数量；实际 author 另验证菜单/静图、ISO 读回、失败保留旧 ISO 及重试。同步第三方 vendor 调用期间不能强制中断；取消检查位于外层 Rust author 的回调边界。

验收记录绑定实际源码、静态 archive 和可执行文件摘要：

| 验收 | 结果 / 本地报告 |
|---|---|
| Rust 单测 / 静态检查 | menu 7、toolchain 29、author 38、core 101（另 5 ignored）；menu 两种 feature 的 Clippy、toolchain Clippy、workspace 格式检查通过 |
| Rust reset | 32 轮交错恢复；SPU 37 sections / 1,776 bytes，NAV 85 sections / 333,888 bytes；`build/menu-rust-reset-acceptance/reset-acceptance.json` |
| session 和生产 author | 145 次失败恢复、Unicode、NULL/invalid UTF-8、递归拒绝、四线程、heap 数稳定、完整字节对照；实际菜单/静图 ISO 读回、导航失败保留旧 ISO、同参数重试和外层取消；`build/menu-session-production-refreshed-20261010/acceptance.json` |
| 历史 DLL | PAL/NTSC × 1/2 PGC 共四组完整字节对照，single-load 失败/重入与四线程首调限制；`build/menu-rust-session-dll-parity/matrix.json`、`build/menu-rust-session-dll-once/report.json` |
| 当前源码/产物审计 | archive 内嵌 reset object、generated source/object、全部 vendor/Rust 输入摘要及节清单通过；`build/menu-rust-session-provenance-acceptance.json` |
| 完整 author / 多页菜单 | 6 成功、10 失败、1,898 独立解码；PAL/NTSC 各 13 albums / 15 pages / 13 stills；`build/rust-menu-session-full-author-refreshed/acceptance.json`、`build/rust-menu-session-pal-ntsc-refreshed/report.json` |
| 实际应用 | 菜单/title 81.10 秒、失败保留旧成品与重试 133.68 秒；`build/application-rust-menu-session-final.log`、`build/application-rust-failure-session-final.log` |
| 单 EXE 发布包 | 仅系统 PATH、MLP/LPCM 各两轨 build/verify、菜单/静图、损坏运行库自动修复；`build/rust-menu-session-onefile-refreshed.json` |

最新汇总为 `build/rust-menu-session-final-acceptance.json`；生产输出仍为 `build/rust-author-production`，GUI 和 ZIP 位于 `build/rust-author-package`。剩余开发 author EXE 不进入 GUI 内嵌 runtime；其中只有 19 个第三方 DLL/配置/字体/素材文件，没有项目 adapter DLL。

实际应用两项长回归使用会话实现的冻结源码快照。之后只调整了 `direct.rs` 的测试条件导入，strict Clippy 通过；archive 和 reset object 摘要完全不变。完整 author、菜单/ISO 和单 EXE 验收对调整后重新构建的产物执行，汇总分别绑定对应证据。

## SPU 子画面迁移（2026-10-31）

菜单子画面（SPU）已由 Rust 实现完全接管，不再编译或链接 C subgen 代码：

| 原 C 职责 | 当前 Rust 实现 |
|---|---|
| spumux XML 解析、时间/颜色/按钮属性 | `src/spu/xml.rs`（复用 crate 根已加固的 XML tokenizer） |
| 图像读取、调色板去重、透明色替换、按钮分组、autooutline/方向推断、裁剪 | `src/spu/image.rs` |
| DVD 2-bit RLE、SP_DCSQ 控制块、时长/命令指针 | `src/spu/encode.rs` |
| MPEG-PS 复制、PACK/PES 头、dvdauthor-data 私有扇区、SPU 包插入、时间调度 | `src/spu/mux.rs` |
| 子画面入口与会话接线 | `src/spu/mod.rs`、`src/direct.rs`（`dvda_menu_run_spu` 直接调用 Rust） |

已删除 `tools/menu-native/vendor/subgen.c`、`subgen-parse-xml.c`、`subgen-encode.c`、`subgen-image.c`、`subgen.h`、`subglobals.h`，以及只在 SPU 构建中使用的 `/DDVDA_SUBPICTURE` 分支和颜色解析代码。迁移阶段 vendor 构建曾只生成 `dvda_menu_nav_vendor.lib`；该 vendor 构建及归档现已全部删除。

验收：`test-rust-menu-session.py --sessions-only` 通过；Rust SPU 的 `menu.mpg`/`expected-spu.mpg` 与迁移前 C vendor 的基准**逐字节一致**（1,677,312 字节，SHA-256 相同），导航输出 `AUDIO_TS.VOB` 不变；`dvda-menu` 14 项测试（含 SPU 单元测试）与工作区检查通过。

## 多页菜单回归修正

纯 Rust 导航路径按 PGC 隔离按钮命令，按源 VOB 读取按钮几何和三组颜色，累计扇区地址并分配独立 VOB ID；AMG 的 cell position 同步引用对应页面的 VOB ID，而不是全部指向 VOB 1。

导航读取 PGC 的 16 色 Y/Cr/Cb 调色板，按历史 `dvdvob.c` 的 `remapcolor`、`remapbyte`、`procremap` 顺序同时重映射按钮色表和 SPU 控制命令；透明度保持不变。无显式调色板的既有单页基准保持原样。回归覆盖跨页同名按钮、三页 cell 引用、分片 SET_COLOR / CHG_COLCON 和连续 SPU。

使用 `settings.local.json` 的 147 首真实素材、隔离输出目录及 release 构建重新制盘成功：两张 ISO 均已生成，构建 Diagnostics 为空。直接读取最终 ISO，逐页验证第一盘 31 页、第二盘 19 页的 IFO cell VOB ID、NAV VOB ID 与起始 LBN 一致；author/menu 共 39 项单元测试通过。

以上结构与单元测试不能替代 PowerDVD 8 的实际点击验收。已启动本机播放器并传入挂载盘，但 UI Automation 只暴露窗口及 ActiveMovie Window，没有可操作的菜单按钮；尚未完成热区、高亮、专辑跳转及歌曲/静图对应的播放器验证。

## 仍保留的 C

| 位置 | 用途 / 是否进入生产运行时 |
|---|---|
| `tools/menu-native/session-rust.c` 和 ABI headers | 小型错误/可变参数隔离桥；静态链接 |
| `tools/menu-native/vendor` | 保留的第三方 dvdauthor VM/VOB/navigation 算法（SPU 已迁 Rust）；静态链接 |
| `dvda-bridges/layout-probe.c`、`image-layout-probe.c` | 构建时测量第三方 ABI，不链接进运行时 |
| 旧 author、media/image helper、C oracle harness | 已删除；保留 Rust golden 数据、来源摘要和 Git 历史 |
| FFmpeg、ImageMagick 等依赖 | 第三方原生库，Rust bridge 调用 |

Python、PowerShell、CMD 是开发构建/验收脚本；`pycdlib` 仅是独立 ISO 读取验收工具，不参与实际 Rust author 或 ISO writer。
