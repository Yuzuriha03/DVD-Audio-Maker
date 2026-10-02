# Windows x64 GUI 发布包

[简体中文](README.md) | [English](README.en.md)

<!-- RUNTIME_REQUIREMENTS -->

双击根目录的 `DVD-Audio-Maker.exe` 启动。标准发布包只提供图形入口，不包含 CLI。请完整解压并保留同目录的 DLL、JSON 和资源文件，不要只复制 EXE。

运行条件见 [RUNTIME.md](RUNTIME.md)：精简包不含 .NET，需要安装 .NET 10 Desktop Runtime（Windows x64）；自包含包无需另装。

封面、菜单绘制、字体和图像校验由精简的内置 x64 图像库完成，不启动 ImageMagick 命令行程序，也无需安装 ImageMagick。请保留 image-native 和 menu-bin/fonts 目录。

## 开始制作

1. 在“开始设置”中选择音源、工作目录和成品目录，填写光盘标题与容量。
2. 在“音频编码”中选择采样率和位深，按需设置光盘菜单。
3. 点击“检查音源”检查输入；“预演制作”会准备音源、编码 MLP 并规划分盘，但不创建 ISO。
4. 点击“开始制作”生成 ISO，完成后使用“验证成品”。无损音频验证沿用首轨抽样，不代表逐轨验证。

任务执行时可取消，关闭窗口时会先取消并等待任务清理。

## 配置与语言

日常设置通过界面编辑并保存在用户目录；“打开方案 / 保存方案”处理 JSON，“导入 config.env”保留旧配置读取能力。首次没有保存设置时，会读取旁边的 config.env。导入文件只解析 KEY=VALUE，不执行命令或展开变量；建议使用绝对 Windows 路径。

右上角可切换“中文 / English”，选择会随方案保存。首次按 Windows 界面语言选择。也可用 `DVD-Audio-Maker.exe --language en` 或 `--language zh-CN` 启动；`--config` 可指定 env 或 JSON 文件。

## 外部工具与编码

发布包包含 dvda-author、mkisofs、菜单工具、内置图像库和中日韩字体。GUI 根据包内布局设置工具及字体路径。请保留 menu-bin/fonts 目录。

音源转换、信息读取、解码和校验使用随包的 x64 媒体库，在主程序内完成，不再启动 FFmpeg 或 FFprobe，也无需安装它们。请保留 media-native 目录。MLP 编码由原生 x64 DLL 完成，不需要 eac3to 或原版 SurCode。

可选的 M4A/ALAC 转 FLAC 功能仍使用 Metaflac 整理封面和标签；使用此功能时，在 PATH 或 GUI 中配置 Metaflac。旧 config.env 中的 FFmpeg / FFprobe 路径可以导入，GUI 会自动改用内置组件。

支持 FLAC、M4A/ALAC 音源和 JPG、PNG、WebP 封面。建议一张专辑一个目录，并提供 date、track、album、title 标签。

## 日志和故障排查

日志区默认显示任务摘要，可切换详细日志、只看提醒、暂停、复制或导出。完整日志和启动错误位于 `%LOCALAPPDATA%/DVD-Audio-Maker/logs`。缺少工具时，在设置中补全对应路径。审计成品需要正式制作生成的 build.log，预演日志不能替代。

## 开发诊断

源码工作区保留 CLI 和测试项目，提供 `cli.cmd`、`gui-debug.cmd` 及 VS Code 调试配置。需要可携带的 CLI 诊断包时，打包器显式添加 `--include-cli`；该包另附 CLI-TOOLS.md。

## 许可与字节一致性

项目代码按随附 LICENSE 发布，第三方组件见 THIRD-PARTY.md。MLP 核心内嵌于应用程序集并按 SHA-256 校验，编码后不打补丁。历史原版字节对照需要相同 PCM、编码设置和显式元数据上下文。
