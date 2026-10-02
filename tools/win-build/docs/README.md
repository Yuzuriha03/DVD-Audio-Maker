# DVD-Audio Maker — Windows x64

[简体中文](README.md) | [English](README.en.md)

<!-- RUNTIME_REQUIREMENTS -->

将 FLAC、ALAC/M4A 音源制作成 DVD-Audio ISO，支持中文/英文界面、MLP 编码、自动分盘、可选菜单及成品验证。

完整解压后双击 `DVD-Audio-Maker.exe`。标准包仅提供图形入口；请保留所有 DLL、JSON、语言资源、`media-native`、`image-native`、`menu-bin` 和 `data`，不要只复制主 EXE。运行条件也见 [RUNTIME.md](RUNTIME.md)。

## 开始制作

1. 选择音源、工作目录和成品目录，设置标题、容量与盘数上限。
2. 选择目标采样率和位深，按需开启选曲菜单、专辑索引和播放封面。
3. 运行“检查音源”，再运行“预演制作”。预演会准备音源并编码 MLP，占用工作目录空间，但不会生成 ISO。
4. 点击“开始制作”，完成后执行“验证成品”。成功生成的 ISO 位于设置的成品目录。

建议一张专辑一个目录，填写 album、title、track、date 标签。封面支持 JPG、PNG、WebP，中日韩菜单字体已随包提供。任务可以取消；关闭窗口时会先取消并等待清理。

## 配置与语言

可打开/保存 JSON 方案，也可导入旧 `config.env`。首次没有已保存设置时会读取随包配置。配置文件只解析键值，不执行命令或展开变量；请使用绝对 Windows 路径。

右上角可切换“中文 / English”，选择随方案保存。语言变化不改变路径、音频标签或编码数据。也可使用 `--language en`、`--language zh-CN`，或通过 `--config` 指定 env/JSON 文件。

日常设置：`%LOCALAPPDATA%/DVD-Audio-Maker/settings.json`。

## 内置组件与依赖

音源转换、探测、解码和媒体校验使用随包的 x64 媒体库；图像、字体及菜单绘图使用内置图像库。无需安装 FFmpeg、FFprobe 或 ImageMagick，这些操作不会启动它们的命令行程序。MLP 编码使用内嵌的MLP 编码核心，不需要原版 SurCode 或 eac3to。

制盘、菜单视频编码/复用和 ISO 生成仍由随包工具完成，因此 menu-bin 也必须完整保留。可选的 M4A/ALAC 转 FLAC 整理功能需要 Metaflac 处理标签与封面；使用该功能时在设置中指定路径，或将它放在 menu-bin/PATH。

旧配置中的 FFmpeg/FFprobe 路径可以导入，GUI 自动使用内置组件。字体包含完整 SC/JP/KR face；无需另行安装字体包。

## 验证范围

“验证成品”检查光盘结构、容量、时间轴和菜单等项目。音频无损检查抽样第 1 盘、组 1、第 1 轨，不是全盘逐轨验证。审计需要正式制作生成的 build.log，预演日志不能替代。

MLP 编码后不打补丁。与历史原版文件逐字节比较时，目标 PCM、编码设置及辅助元数据上下文必须一致；相同音频本身不足以保证整文件相同。

## 日志与常见问题

日志区默认显示任务摘要，可切换详细输出、只看提醒、暂停显示、复制或导出。完整任务日志和启动错误位于 `%LOCALAPPDATA%/DVD-Audio-Maker/logs`。

- 无法启动：先检查是否安装本包要求的 .NET 10 Desktop Runtime x64。
- 提示缺少 DLL、字体或菜单工具：重新完整解压，不要从不同版本拼装文件。
- 需要排查制作失败：导出完整日志，保留对应工作目录后再重试。
- 工作目录占用较大：预演和编码都会生成缓存；清理后相关步骤需重新执行。

## 许可

项目许可见 [LICENSE](LICENSE)，第三方组件与字体见 [THIRD-PARTY.md](THIRD-PARTY.md)。请保留随包许可及组件目录中的说明。
