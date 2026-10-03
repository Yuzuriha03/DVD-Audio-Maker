# DVD-Audio Maker v1.0

通过图形界面，将 FLAC、M4A/ALAC 音乐制作成带 MLP 无损音频的 DVD-Audio ISO，支持自动分盘、菜单、封面和成品验证。

## 本次更新

- 精简为单文件主程序，使用说明、配置示例与许可放在旁边，统一放进一个发布 ZIP。
- 发布包约 **14.66 MiB**，支持 **Windows x64**，保留完整中日韩菜单字体。
- 音频处理、MLP 编码、图像处理和制盘所需组件已内置，无需另外安装 FFmpeg、FFprobe、ImageMagick、eac3to 或原版 SurCode。
- 保留中英文界面与日志、JSON 方案保存和旧 config.env 配置导入。

## 下载与运行

下载 **DVD-Audio-Maker-v1.0-win-x64-GUIonly.zip**，其中包含主程序与全部旁文件。

1. 首次使用前安装 [.NET 10 Desktop Runtime（Windows x64）](https://dotnet.microsoft.com/download/dotnet/10.0)。本精简包不包含 .NET 运行时。
2. 解压 ZIP，双击 DVD-Audio-Maker.exe；首次启动会自动准备必要组件，以后校验并复用缓存。请保留随包授权声明。
3. 在界面选择音源、工作目录和成品目录，设置音频格式、容量及菜单。
4. 依次进行音源检查、制作预演和 ISO 制作，完成后运行成品验证。

config.env.example 仅为示例，不会自动加载。可以继续导入已有 config.env，或在界面编辑并保存方案。详细说明见 ZIP 内 README.md。

## English

Create DVD-Audio ISO images from FLAC and M4A/ALAC music, with lossless MLP encoding, disc splitting, menus, cover images and output verification. The interface and logs support Chinese and English.

Download **DVD-Audio-Maker-v1.0-win-x64-GUIonly.zip** (approximately **14.66 MiB**) for **64-bit Windows**. It contains one application EXE plus separate documentation, example configuration and license notices. Complete Chinese, Japanese and Korean menu fonts are retained.

Install [.NET 10 Desktop Runtime for Windows x64](https://dotnet.microsoft.com/download/dotnet/10.0), extract the ZIP and run DVD-Audio-Maker.exe. Required media, image and authoring components are built in; no separate FFmpeg, FFprobe, ImageMagick, eac3to or original SurCode installation is needed. Components are prepared automatically on first launch and reused afterward. Retain the included license notices.

Choose the source and output folders, check the audio, preview the build, create the ISO and verify the result. Existing config.env files and saved JSON profiles remain supported. config.env.example is not loaded automatically. See README.en.md in the ZIP for details.
