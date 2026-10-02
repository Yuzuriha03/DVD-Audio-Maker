# 按需重编译 FFmpeg 原生依赖

[简体中文](MINIMAL-FFMPEG.md) | [English](MINIMAL-FFMPEG.en.md)

日期：2026-10-02。延续字体共享和 ImageMagick 入口精简，发布形态仍为 Windows x64、仅 GUI、不含 .NET 运行时；ZIP 格式及压缩参数不变。

## 结果

| 项目 | 上一轮精简包 | 本轮重编译包 |
|---|---:|---:|
| ZIP | 76.98 MiB | 32.11 MiB |
| 解压后 | 172.53 MiB | 52.00 MiB |

本轮 ZIP 再减少约 58.29%；相对最初 117.61 MiB 的 v1.0，累计减少约 72.70%。精确字节数、最终 SHA-256 和验证记录见 [minimal-ffmpeg-validation.json](minimal-ffmpeg-validation.json)。

本地最终包：`build/ffmpeg-minimal-x64-release/DVD-Audio-Maker.zip`。该目录不进入 Git；本次没有提交、推送或更新公开 v1.0。

## 实际变更

- 从官方、签名验证通过的 FFmpeg 9.0.2 原始源码，重编译 `avcodec-63.dll`、`avformat-63.dll`、`avutil-61.dll`，ABI 分别为 63.1.102、63.1.102、61.1.102。
- 仅启用 MLP 解码器、编码器、解析器、原始 MLP 输入/输出及 file/pipe 协议。保留原生工具旧接口所需的 MLP 编码能力；GUI 仍使用编码算法的独立 DLL。
- 关闭视频编解码器、网络、外部编解码库及无关模块，保留 x86 汇编优化和运行期 CPU 检测。新增三库总计 2,407,424 字节。
- 三库替换后，移除 74 个不再被任何保留程序依赖的 DLL，包括 x264/x265、AOM/dav1d/SVT-AV1、JPEG XL、Rsvg 和网络相关依赖。与上一轮的六个 DLL 合计，较最初包减少 80 个 DLL。
- libwinpthread 等仍被实际导入的公共运行库保留。字体、ImageMagick 图像核心、制盘程序和菜单视频工具保持原字节。

`NativeOptimizationProfile.json` 和 `MinimalFfmpegProfile.json` 固定完整工具集及替换库的 SHA-256。删除前还检查普通/延迟导入、二进制和 XML 名称引用，并保留传递依赖。未知文件或版本不进入自动清理范围。原始输入目录不被覆盖。

GUI 的音源转换、解码检查和成品校验仍使用用户配置的外部 FFmpeg。MLP 编码 DLL 和承载它的程序集未改动，也没有在输出文件上打补丁。

## 验证范围

- 108/108 项项目兼容性测试通过，包括拒绝未知重编译库。
- 校验现有工具及测试程序使用的 86 个 FFmpeg 导入符号；三个新库均为原生 AMD64 PE，ABI 版本保持一致。
- 自动生成 84 组 PCM：44.1/48/88.2/96 kHz × 16/20/24 bit × 1–6 声道，以及 176.4/192 kHz × 16/20/24 bit × 1–2 声道。每个声道数采用项目默认布局。包含声道各异的信号、低位变化、静音和非整 AU 长度。
- 用未修改的编码 DLL 生成 MLP，新旧 FFmpeg DLL 分别解码。84 组解码数据逐字节一致，均还原输入 PCM，帧数/采样率/位深/声道一致，末尾补齐为零。测试核对实际加载的 DLL 路径，不允许从开发环境借库。
- 91 项发布包回归通过：中文/英文 GUI 启动，三个既有 MLP 样本整文件字节一致，中日韩三个专辑、六首四秒音轨的完整制盘和无损校验；六个制盘 MLP 与旧包整文件字节一致，42 张菜单/按钮/静图像素一致。
- 最终包与该实测候选包核对所有运行期文件，再校验 ZIP、SHA-256 清单及双语 GUI 启动。ZIP 保持标准 Deflate，未捆绑 .NET 或 CLI。

这 84 组是新旧解码库和输入 PCM 的对照，不代表原版 SurCode 支持或生成了全部配置。原版 SurCode 不支持的高采样率也纳入MLP 编码器的解码回归。

完整制盘测试沿用基准的 ASCII 卷标和 Windows 8.3 原生工具路径；单页索引菜单的既有 `MENU_INDEX_ARROW_MISSING` 问题不属于本次通过范围，索引阈值设为 99。原有 ImageMagick 非 ASCII 安装路径字体限制未在本次修复。详细测试输入和限制保存在验证记录中。

## 重编译与开发

普通 C# GUI 开发仍只需 .NET 10 SDK。维护这些原生 DLL 时另需 Windows x64、Python 3.12+、MSYS2 的 bash/make/gpg，以及 MinGW-w64 GCC/strip。脚本不会修改 MSYS2 安装，也不使用 WSL。

```bat
python tools\win-build\build-minimal-ffmpeg.py --msys-root "D:\dev\msys64"
```

默认将源码、NASM、构建日志、导入库和 DLL 放在被忽略的 `build/ffmpeg-minimal`。可用 `--work-directory` 和 `--jobs` 调整目录和并行数；路径含空格时使用 Windows 8.3 名称，如果系统未启用短文件名，应指定无空格的工作目录。

脚本固定源码/NASM 哈希，并核对 FFmpeg 发布密钥指纹及已解压源码。固定来源、实际编译器版本和完整配置见 [minimal-ffmpeg-build.json](minimal-ffmpeg-build.json)。更换编译器、路径或配置可能改变 DLL 哈希；这是可复现的构建配方，不承诺任意工具链下的二进制哈希相同。

使用已有工具目录组装精简包：

```bat
tools\win-build\build-all.cmd --source "D:\dev\winbuild\src" --prebuilt "D:\dev\winbuild\menu-bin" --framework-dependent --magick-shim "build\magick-shim\magick-shim.exe" --ffmpeg-libraries "build\ffmpeg-minimal\install\bin"
```

已验证的新包 `menu-bin` 可直接作为下次的 `--prebuilt`，不需要重复传入两个替换参数。CLI 调试入口和 VS Code F5 保留在开发目录，测试辅助程序不进入发行包。

变更原生配置后，先将三个新 DLL 放入一份独立的候选工具目录，再进行对照；`--output` 必须是尚不存在的新目录：

```bat
python tools\win-build\test-minimal-ffmpeg.py --baseline "旧包\menu-bin" --candidate "候选包\menu-bin" --prefix "build\ffmpeg-minimal\install" --msys-root "D:\dev\msys64" --output "build\ffmpeg-validation-new"
```

`native/mlp-decode-probe.c` 通过原生 libav API 验证解码，`native/pe_dependencies.py` 检查 PE 接口。完成接口、音频和完整制盘验证后，才更新 `MinimalFfmpegProfile.json`。打包器拒绝未匹配指纹的显式替换库。
