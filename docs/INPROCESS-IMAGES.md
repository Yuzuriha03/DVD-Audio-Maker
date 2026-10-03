# 内置图像处理

日期：2026-10-02。

封面、背景、文字、按钮三态、专辑索引、播放静图、字体检测和图像校验均使用 `image-native/dvda-image.dll`。C# GUI 和原生制盘程序共用这份 Windows x64 图像库，不启动 magick/convert/mogrify/identify 命令行程序。音频仍使用既有内置媒体库和MLP 核心。

## 保留的能力

- JPG/PNG 读写、WebP 封面读取；保留透明通道、缩放、裁切、合成、渐变、绘图、字幕和统计。
- 完整 SC/JP/KR 字体 face 共用原有 TTC，无字形裁剪；字体路径支持 Unicode 和空格。
- 按需静态链接 ImageMagick 7.0.8-47 Q16 HDRI、FreeType、JPEG/PNG/WebP/zlib。无外部 delegate、动态 coder 模块、视频或文档转换链。
- 取消、超时、损坏图片、并发请求均有错误处理。GUI 图片写入先暂存，成功后替换，失败或取消保留原文件。

这不是通用 ImageMagick 分发包；不提供 TIFF/PDF/SVG 等未使用的图像转换入口，也不写入 WebP。

## 验证范围

- 图像专项 23 项：三种封面格式、中日韩文字、透明度/统计、Unicode 路径、原生命令空格、损坏输入、取消/超时及并发。
- 实际 GUI 预演、ISO 制作和成品校验覆盖普通菜单、单页专辑索引、两页索引以及翻页箭头。Windows 进程树跟踪确认这些流程不创建外部 ImageMagick、FFmpeg/FFprobe 或 SurCode 进程。
- 测试环境仅保留 Windows/.NET 的 PATH，并将导入配置中的 FFmpeg/FFprobe 设为不存在的文件。
- 正常样本 6 个 MLP、索引样本 6/14 个 MLP 均与既有基准逐字节一致；MLP 核心及媒体 DLL 未改变，没有编码后修补。
- 正常菜单 42 张图像的尺寸一致，其中 24 张解码像素完全一致；18 张文字叠加图的字形边缘存在差异。已检查文字、布局和可读性，未宣称所有菜单像素相同。

修复了原生索引命令前导空格被识别为空命令的问题；单页索引不再错误要求翻页箭头。

具体执行记录、文件哈希和体积见 [inprocess-images-validation.json](inprocess-images-validation.json)。该记录描述图像迁移阶段；当前菜单编码、复用、子图像、导航和 ISO 均已内置，见[后续迁移清单](NO-EXTERNAL-RUNTIME-MIGRATION.md)。

## 构建与开发

见 [Windows 构建说明](../tools/win-build/README.md)。原生 ImageMagick/FreeType 归档固定 SHA-256，解压树的未修改上游文件也会核验；补丁由脚本从原始文件重建。JPEG/PNG 的错误跳转使用匹配的 UCRT 上下文及当前栈指针，避免取消时破坏 .NET 主程序栈；不改变宿主异常处理器或 Windows 保护策略。

`build-image-author.py` 在独立目录创建源码快照，`native/author-inprocess-images.patch` 保存七处图像外部调用的迁移补丁。不会覆盖原生作者工作树。图像 DLL 的许可与来源记录随包放在 image-native 下。

正式包仍为 x64、GUI-only、framework-dependent，要求安装 .NET 10 Desktop Runtime x64。支持导入 config.env，开发 CLI 和调试入口保留在源码中。发布目录和 ZIP 均留在忽略的 build 目录，不加入 Git。

## 实测体积

| | Previous | In-process images | Saved |
|---|---:|---:|---:|
| ZIP | 35,333,326 bytes | 30,918,284 bytes | 4,415,042 bytes |
| Unpacked | 59,780,290 bytes | 57,659,623 bytes | 2,120,667 bytes |

ZIP 格式与压缩参数未改变；兼容性测试 108/108 通过。
