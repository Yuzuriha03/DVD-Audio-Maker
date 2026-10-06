# 进程内图像处理

封面、背景、文字、按钮三态、专辑索引、播放静图、字体检测和图像校验使用项目构建的 x64 `dvda-image.dll`。Rust GUI 和 author 共用这套 DLL，不启动 `magick`、`convert`、`mogrify` 或 `identify` 命令行程序。

## 保留能力

- JPEG/PNG 读写、WebP 封面读取、透明通道、缩放、裁切、合成、渐变、绘图、文字和统计；
- SC/JP/KR 三套完整字体 face 共用 TTC，保留 Unicode 和带空格路径；
- 按需构建的 ImageMagick、FreeType、JPEG、PNG、WebP 和 zlib 功能，禁用外部 delegate、动态 coder、视频和文档转换链；
- 取消、超时、损坏输入和并发请求处理，GUI 写入先暂存、成功后替换。

这是应用专用运行时，不提供 TIFF/PDF/SVG 等未使用的转换入口，也不输出 WebP。

## 构建和验证

原生源码、固定哈希和构建入口见 [Windows 构建说明](../tools/win-build/README.md)。菜单 fixture 会覆盖普通菜单、索引页、静图、字体和 ISO 成品校验。图像 DLL 的构建来源和许可随发布包的 NOTICE 文件提供。

```powershell
cargo test --manifest-path rust/Cargo.toml --target x86_64-pc-windows-gnu --workspace --offline -- --include-ignored
```

发布运行时为 Rust 原生 GUI；配置只使用 JSON profile，发布目录和 ZIP 保留在被忽略的 `build` 目录。
