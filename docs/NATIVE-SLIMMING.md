# 原生工具精简记录

[简体中文](NATIVE-SLIMMING.md) | [English](NATIVE-SLIMMING.en.md)

本文记录 ImageMagick 精简阶段。后续 FFmpeg 按需重编译将 ZIP 进一步降至约 32.11 MiB，见 [最新重编译记录](MINIMAL-FFMPEG.md)。

日期：2026-10-02。本轮延续字体共享方案，保持 Windows x64、仅 GUI、不含 .NET 运行时。ZIP 格式与 CompressionLevel.SmallestSize 参数不变。

## 结果

| 项目 | 字体共享版 | 本轮精简版 | 减少 |
|---|---:|---:|---:|
| ZIP | 92.30 MiB | 76.98 MiB | 15.32 MiB / 16.60% |
| 解压后 | 193.35 MiB | 172.53 MiB | 20.82 MiB / 10.77% |

ZIP 精确大小：96,780,971 → 80,716,656 字节。相对最初 v1.0 的 123,318,008 字节，累计减少 34.55%。

最终包：`build/native-slim-x64-release/DVD-Audio-Maker.zip`。

SHA-256：`e39417ca1d30f072356002179eb554143e5d236d8cf2a5ee6a733270e5b2801f`。

## 改动

- 保留原有 `magick.exe` 图像核心。`convert.exe`、`mogrify.exe` 各由 6,847,032 字节替换为 5,632 字节的本项目原生 x64 转发程序。
- 原始参数尾部直接传递给相应子命令，保留标准输入输出、退出码和工作目录。Windows Job Object 负责取消时结束子进程。转发入口只导入 Kernel32，不添加运行库或首次启动解包步骤。
- 清理六个闲置文件：`libfftw3-3.dll`、`liblqr-1-0.dll`、`libltdl-7.dll`、`libMagickCore-7.Q16HDRI-10.dll`、`libMagickWand-7.Q16HDRI-10.dll`、`libraqm-0.dll`。
- 清理使用包含 118 个文件 SHA-256 的原生工具集指纹；还检查普通导入、延迟导入和二进制/XML 文件名引用，并保留被留下组件需要的传递依赖。未知版本、新增原生组件或被引用文件不自动删除。
- 原始输入目录不改写。其他所有原生程序和 DLL、字体、MLP 编码核心均保持原字节；AVCodec、AVFormat 及其仍在导入链中的 H.265/AV1 等库保留。

## 验证

- 108/108 项兼容性测试通过，包含 PE 普通/延迟导入、x64 入口校验和未知组件保留。
- 25 项转发入口检查通过：中日韩字形、图像表达式/引号、含空格路径、JPG/PNG/WebP、原地修改、二进制管道、失败退出、缺失核心及强制取消。
- 90 项发布包和实机制盘检查通过；运行时 PATH 限制为 Windows 系统目录及 .NET，避免借用开发工具目录里的 DLL。
- 三组独立 MLP 样本（48 kHz / 16-bit / 单声道、48 kHz / 24-bit / 双声道、96 kHz / 24-bit / 六声道）与既有基准整文件逐字节相同。
- 中日韩三个专辑、六首四秒音轨完成制盘及成品校验；六个 MLP 与旧包逐字节相同，42 张菜单/按钮/静图的像素相同。
- 最终包相对实测候选仅更新两份第三方组件说明；所有运行期文件逐字节相同。重新核对 ZIP/清单，并通过中英文 GUI 启动检查。

实机制盘样本采用 ASCII 卷标、Windows 8.3 原生工具路径、每页一首专辑菜单和静图，索引阈值为 99。旧包的单页索引菜单 `MENU_INDEX_ARROW_MISSING` 问题未纳入本次通过范围。非 ASCII 安装路径的既有 ImageMagick 字体限制也未修复。

详细记录：[native-slimming-validation.json](native-slimming-validation.json)。

## 后续开发

构建与测试命令见 [Windows 打包说明](../tools/win-build/README.md#原生工具精简)。原生转发入口可独立重编译；正常 C# GUI 开发不增加编译器要求。已经精简的工具目录可以直接复用，最终包也通过了该复用路径。

本轮生成本地候选包。尚未提交、推送或替换公开 v1.0 的标签/附件；发布包不纳入 Git 跟踪。
