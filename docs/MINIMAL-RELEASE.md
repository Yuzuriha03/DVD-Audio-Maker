# 不含 .NET 的精简发布包

[简体中文](MINIMAL-RELEASE.md) | [English](MINIMAL-RELEASE.en.md)

日期：2026-10-02。状态：完成。保留完整 GUI、原生 MLP 编码、制盘、菜单和字体，去掉 .NET 运行库、CLI 与开发符号。开发脚本和 F5 配置保持可用。

## 用户运行条件

先安装 **.NET 10 Desktop Runtime（Windows x64）**；普通 .NET Runtime、ASP.NET Core Runtime 或 .NET Framework 4.x 不能替代。已有兼容桌面运行时则无需重复安装。精简包中英文 README 的开头均明确提示，并提供微软官方安装页；另附 RUNTIME.md / RUNTIME.en.md。

FFmpeg 等既有外部工具要求保持不变。完整解压后双击 DVD-Audio-Maker.exe，不要只复制 EXE。

## 打包

在现有 tools/win-build/build-all.cmd 命令上增加 `--framework-dependent`，继续指定 --source、--prebuilt 和 --output。不传该选项仍生成自包含包；省略 --output 时，精简包默认输出到 tools/win-build/release-framework-dependent，避免覆盖自包含包。默认没有 CLI；开发诊断需要时可额外添加 --include-cli。

本次产物：`build/gui-minimal-x64-release/DVD-Audio-Maker`，ZIP 为同级 DVD-Audio-Maker.zip。之前的自包含包保留。

## 体积与验证

| 项目 | 之前仅 GUI 自包含包 | 不含 .NET 的精简包 | 减少 |
|---|---:|---:|---:|
| 解压后 | 340.93 MiB | 223.83 MiB | 34.35% |
| ZIP | 164.93 MiB | 117.61 MiB | 28.70% |
| 文件数 | 423 | 159 | 264 个文件 |

- 26/26 专项检查通过。包内无 CoreCLR、JIT、hostfxr、hostpolicy、System.Private.CoreLib 或 WinForms 运行库。
- runtimeconfig 引用安装在机器上的 .NET 10 Core / Desktop 框架；程序是 AMD64。
- 中英文 GUI 和日志自检通过，主机跟踪确认使用系统安装的 .NET 10 Desktop Runtime。隔离运行时路径后启动失败，未伪装成自包含应用。
- GUI 实际编码 48 kHz / 16 bit / 单声道、48 kHz / 24 bit / 双声道、96 kHz / 24 bit / 六声道；三组完整 MLP 与此前发布包逐字节一致。
- 原生 MLP 核心指纹、全部第三方工具、字体和菜单素材未改变。清单和 ZIP 内容的 SHA-256 已核对。
- 本次运行发布包专项测试，未重复执行此前已通过的 107 项通用回归。

机器可读记录见 [minimal-release-validation.json](minimal-release-validation.json)。
