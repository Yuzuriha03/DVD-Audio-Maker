# 发布包体积优化

[简体中文](RELEASE-SIZE.md) | [English](RELEASE-SIZE.en.md)

日期：2026-10-02。状态：完成。目标是减少 Windows x64 发布包体积，同时保留 GUI、CLI、离线自包含运行、菜单、字体和编码行为。

## 基线

当前 `build/gui-x64-release/DVD-Audio-Maker`：147 个文件，总计 424,911,644 字节；ZIP 为 206,580,409 字节。GUI 单文件 117,046,595 字节，CLI 单文件 74,479,428 字节，两者分别捆绑运行库。

## 实施顺序

1. 将 GUI 和 CLI 发布到同一应用目录，共享一套自包含 .NET 运行库和应用程序集；同名文件必须逐字节一致才能合并。
   打包时为 CLI 设置 `ShareDesktopRuntime=true`，使其与 GUI 使用完全相同的 Windows Desktop 运行库；普通 CLI 构建仍使用原有框架引用。避免把 Core 和 Desktop 中不同的同名门面程序集互相覆盖。
2. 保留根目录 `DVD-Audio-Maker.exe` 和 `dvda.cmd`，CLI 可执行文件移到根目录 `dvda.exe`；整个目录一起分发。
3. 保留全部第三方工具、动态库、字体和语言资源，不凭文件名删除依赖；使用 ZIP 的最小体积压缩等级。
4. 构建新发布包，验证共享依赖、原生核心指纹、GUI/CLI 入口、隔离全局 .NET 搜索路径后启动及编码回归，记录实际体积。

不启用未经验证的 WinForms 裁剪，不改变 MLP 核心或浮点编译配置。新布局中的 EXE 不是独立单文件，不能单独拷贝。

## 实测结果

| 项目 | 优化前 | 优化后 | 减少 |
|---|---:|---:|---:|
| 解压后体积 | 405.23 MiB | 341.17 MiB | 64.05 MiB / 15.81% |
| ZIP 体积 | 197.01 MiB | 165.03 MiB | 31.98 MiB / 16.23% |
| 文件数量 | 147 | 428 | 运行库从 EXE 中展开，共享一份 |

新包目录：`build/gui-x64-compact-release/DVD-Audio-Maker`；图形入口为其中的 `DVD-Audio-Maker.exe`。旧的 `build/gui-x64-release` 保留作基线，未覆盖。

## 验收

- 兼容性回归：107/107 通过；发布包专项检查：32/32 通过。
- GUI/CLI/运行库均为 AMD64。设置无效 DOTNET_ROOT、禁用多层搜索并限制 PATH 后，两个入口启动成功；主机跟踪确认从发布包加载 coreclr.dll。未通过卸载机器上的 .NET 来测试。
- 中英文 GUI 的设置控件、日志自检，以及 `dvda.cmd` CLI 转发均通过。
- 全部第三方工具、字体和菜单素材的文件集合与 SHA-256 均保持不变。清单覆盖全部文件；ZIP CRC、文件集合及解压内容 SHA-256 均通过。
- 使用实际新旧发布包分别执行音源准备和 MLP 编码：48 kHz / 16 bit / 单声道、48 kHz / 24 bit / 双声道、96 kHz / 24 bit / 六声道，三组完整 MLP 均逐字节相同；解码 PCM 均与输入完全相同。
- MLP DLL SHA-256 保持 `ece6d0a8033a26e2528042a7b74c66c249ea3c8d7378c06809fb94c8f6bd79b8`。本次验收覆盖打包变化，没有重跑原版 SurCode 的全部历史矩阵。

机器可读记录见 [release-size-validation.json](release-size-validation.json)。详细检查日志与主机跟踪位于 `build/release-size-verification/run-20261002-103607`。
