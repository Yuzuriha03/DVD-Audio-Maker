# 自有菜单 C 代码迁移记录

更新于 2026-10-10。此次迁移覆盖生产链路中剩余的自有菜单会话逻辑；完整 AOB/IFO/ISO author 的迁移见 [Rust author](RUST-AUTHOR-MIGRATION.md)。

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

新的静态 vendor 目录为 `build/menu-direct-vendor-rust-session`。旧目录 `build/menu-direct-vendor-full-author` 曾用于差分验收，现已清除其中的 C 基线产物；历史验收摘要保留。`menu-build.json` 标识 Rust session/reset，并记录所有 writable vendor section、长度、Rust reset 源码和 object 摘要；生产 author 和打包器拒绝旧 C session/reset 清单或过期 Rust 源码摘要。当前 session 验收无需旧 C，检查两个新 Rust 进程的完整输出一致性。

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

## 仍保留的 C

| 位置 | 用途 / 是否进入生产运行时 |
|---|---|
| `tools/menu-native/session-rust.c` 和 ABI headers | 小型错误/可变参数隔离桥；静态链接 |
| `tools/menu-native/vendor` | 保留的第三方 dvdauthor SPU/VM/VOB/navigation 算法；静态链接 |
| `dvda-bridges/layout-probe.c`、`image-layout-probe.c` | 构建时测量第三方 ABI，不链接进运行时 |
| 旧 author、media/image helper、C oracle harness | 已删除；保留 Rust golden 数据、来源摘要和 Git 历史 |
| FFmpeg、ImageMagick 等依赖 | 第三方原生库，Rust bridge 调用 |

Python、PowerShell、CMD 是开发构建/验收脚本；`pycdlib` 仅是独立 ISO 读取验收工具，不参与实际 Rust author 或 ISO writer。
