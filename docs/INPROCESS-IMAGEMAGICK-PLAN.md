# 进程内图像处理方案

方案已经落地：所需 ImageMagick/FreeType 功能由项目构建的 x64 `dvda-image.dll` 提供，Rust GUI 和 author 直接调用 DLL，不启动 ImageMagick 命令行程序。

发布包只保留应用实际使用的 JPEG、PNG、WebP 读取、字体、合成和绘图功能，禁用外部 delegate、动态 coder、视频和文档转换链。组件来源、哈希、补丁和许可记录在构建目录及发布包 NOTICE 文件中。

配置使用 JSON profile，不读取 `config.env`。完整 Rust 测试、菜单 fixture 和发布包审计见 [进程内图像处理](INPROCESS-IMAGES.md) 与 [Windows 构建说明](../tools/win-build/README.md)。
