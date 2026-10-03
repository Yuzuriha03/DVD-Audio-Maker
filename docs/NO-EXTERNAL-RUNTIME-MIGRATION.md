# 脱离外部运行时依赖迁移清单

[English](NO-EXTERNAL-RUNTIME-MIGRATION.en.md)

本文只记录当前 GUI 全盘制作流程仍需迁入的外部运行时功能。仓库已有的 C# 业务代码、MLP 核心、进程内媒体和图像库，以及可由仓库源码构建的 dvda-author-dev.exe，不列入迁移目标。它们的编译工具属于开发环境要求，不属于用户运行时依赖。

## 当前边界

ISO 写入器和 FLAC 元数据迁移已经完成。无菜单制作由 dvda-author-dev.exe 内置 C ISO9660 写入器直接生成镜像，不再需要或打包 mkisofs.exe；M4A/ALAC 整理由 C# 进程内 FLAC metadata 编辑器读写标签和封面，不再启动 metaflac.exe。菜单开启时仍会使用下列菜单编码和 DVD-Video authoring 工具，这些是下一阶段迁移内容。

## 已完成

### ISO9660 写入器

新的 C 实现 dvda_iso_write 已接入 dvda-author-dev.exe，负责：

- Primary Volume Descriptor、终止描述符以及大小端路径表；
- AUDIO_TS、VIDEO_TS 和菜单目录的目录记录；
- 2048 字节扇区对齐、流式大文件写入和最终文件长度；
- 卷标传递、ISO 容量检查及 DiscBuildExecutor 原有的暂存、发布和失败清理语义。

DiscBuildExecutor 将 ISO 目标直接传给 author，不再启动外部进程。生成的 ISO 已由仓库 Iso9660Reader 回读，兼容性测试目前为 109/109。旧 DVDA_MKISOFS 配置键仍可读取，以兼容旧 config.env，但运行时不会解析或启动该程序。

## 仍需迁入

### DVD-Video 菜单 authoring

当前由 dvdauthor.exe 完成。需要迁入 VMG/VTS 菜单结构、PGC、按钮、导航命令、VOB/IFO/BUP 生成，以及菜单到 DVD-Audio 音频入口的连接。

### 子图像和按钮覆盖

当前由 spumux.exe / spuunmux.exe 完成。需要迁入 normal、selected、activated 状态，透明度和调色板，子图像 RLE 编解码，按钮坐标和导航校验。

### MPEG-2 菜单视频

当前由 jpeg2yuv.exe 和 mpeg2enc.exe 完成。需要覆盖 JPEG/PNG 到 YUV、PAL/NTSC 720x576/720x480、菜单帧率、序列头、GOP、结束标记、DVD 菜单码率和 I-frame 输出。

### 菜单音频和 MPEG-PS 复用

当前由 mp2enc.exe 和 mplex.exe 完成。需要覆盖静音或背景音频、音频编码、PTS/SCR、pack 对齐和 VOB 输出。

## 后续可迁入

M4A/ALAC 转 FLAC 的可选封面和标签整理已经迁入 `FlacMetadataEditor`：它解析 Vorbis Comment 和 PICTURE block，导出/导入封面，并原子重写 metadata 前缀，同时逐字节复制音频帧。该功能不再依赖 metaflac.exe，也没有重写 FLAC 音频压缩器。

全盘内置验证可以后续加入 ISO/AOB reader、MLP decoder、逐轨 PCM 比较以及菜单和 PTS parser。当前首轨抽样校验可在 authoring 迁移期间继续使用。

## 明确排除

以下项目不需要重新实现为脱离仓库的运行时组件：

- C# GUI、CLI、共享业务 DLL 和测试项目；
- mlp_encoder.dll 及 Native/source；
- dvda-media.dll、dvda-image.dll 和桥接代码；
- dvda-author-dev.exe 本身及其项目补丁；
- MLP 解析、缓存、分盘、菜单规划和日志代码；
- 历史 FFmpeg/FFprobe 对照路径；
- ImageMagick 参考程序、magick-shim 和图像回归辅助程序；
- eac3to、原版 SurCode 和旧 surcode.exe 入口；
- Python、MSYS2、MinGW、Make、GPG、NASM 等构建工具；
- 独立回归基准包和仅供对照的第三方旧二进制。

“明确排除”表示这些功能不需要在仓库外重新实现为用户运行时组件；开发者仍可使用它们重建或测试原生组件。

## 推荐顺序

1. ISO writer：已完成。
2. 无菜单模式移除 mkisofs.exe：已完成。
3. 内置 FLAC metadata：已完成。
4. 菜单 MPEG 视频、菜单音频和复用。
5. 子图像按钮和 DVD-Video authoring。
6. 将菜单 API 接入 dvda-author-dev。
7. 扩展全盘内置验证。

新代码尽量使用 C。每一步都要对照当前工具生成的目录、轨道表、菜单导航、PTS、AOB 和 ISO。MLP 验收条件仍是目标 PCM、编码设置和元数据上下文一致时，输出文件逐字节一致。
