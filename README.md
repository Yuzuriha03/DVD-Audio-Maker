# DVD-Audio Maker

[简体中文](README.md) | [English](README.en.md) | [日本語](README.ja.md)

将 FLAC、ALAC/M4A 音乐制作成 DVD-Audio ISO，支持 MLP / LPCM 编码、自动分盘、选曲菜单、专辑封面和成品验证。适用于 Windows x64，界面与日志支持中文、英文和日语。

**首次使用前，请安装 [.NET 10 Desktop Runtime（Windows x64）](https://dotnet.microsoft.com/download/dotnet/10.0)。** 发布包不包含 .NET；普通 .NET Runtime、ASP.NET Core Runtime 或 .NET Framework 不能替代桌面运行时。

## 安装与启动

1. 从 [GitHub Releases](https://github.com/Yuzuriha03/DVD-Audio-Maker/releases/tag/v1.0) 下载 `DVD-Audio-Maker-v1.0-win-x64.zip`。
2. 将 ZIP 解压到自己的文件夹，双击 `DVD-Audio-Maker.exe`。
3. 首次启动会自动准备所需组件，稍等片刻即可打开界面。无需另装 FFmpeg、ImageMagick、eac3to 或 SurCode。

主程序、使用说明、配置示例及许可证都在解压目录内。请保留 `LICENSE`、`THIRD-PARTY.md`、`THIRD-PARTY.en.md`、`NOTICE-Image.txt` 和 `NOTICE-Menu.txt`。

## 制作第一张光盘

1. **选择音源。** 选择存放音乐的文件夹，程序会读取子文件夹中的音源。建议每张专辑单独一个文件夹，并填写专辑、标题、曲序和日期标签。
2. **选择成品位置。** 指定 ISO 保存目录，填写光盘标题，选择 DVD5、DVD9 或自定义容量。“光盘数量上限”填 0 表示不限制，程序按容量自动分盘。
3. **设置音频。** 通常使用“MLP 编码”。按音源和播放设备选择采样率、位深；降低采样率或位深会改变原始音频精度，提高它们不会增加原有细节。
4. **设置菜单。** 按需启用选曲菜单、专辑索引和播放封面。支持 JPG、PNG、WebP 封面，已附带中日韩菜单字体。
5. 点击 **“检查音源”**，处理提示的问题；再点击 **“开始制作”**。开始制作也会自动检查音源，然后编码并生成 ISO。
6. 完成后点击 **“验证成品”**。验证通过后，可通过“查看成品”打开 ISO 所在文件夹。

如果刚完成“检查音源”且音源、检查设置和准备清单都没有变化，“开始制作”会复用这次检查结果，不重复探测和解码校验。任何音源文件变化都会自动触发重新检查。

制作过程可取消。关闭正在工作的窗口时，程序会先停止任务并清理。已完成的成品不会因为取消而删除。烧录和播放需要支持 DVD-Audio 的软件或设备；生成 ISO 不代表普通 DVD-Video 播放器能够播放。

## 音频设置

| 方式 | 存储方式 | 体积与限制 | 适用情况 |
| --- | --- | --- | --- |
| MLP 编码 | 对 PCM 进行无损压缩 | 通常较小；支持 16 / 20 / 24 位 | 希望节省空间，或制作高采样率多声道内容 |
| LPCM 编码 | 非压缩 PCM | 较大；当前支持 16 / 24 位，音频码率不超过 9.6 Mb/s | 希望直接存储未压缩音频 |

**相同采样率、位深、声道和 PCM 输入下，两者音质相同。** LPCM 不会因为体积更大而增加音源细节。降低采样率或位深会改变精度，提高参数也不会补回音源中没有的信息。

LPCM 还需满足码率限制。例如 96 kHz / 24 位 / 6 声道为 13.824 Mb/s，不能作为 LPCM 写入；请选择 MLP 或降低目标参数。20 位目前仅支持 MLP。LPCM 缓存和制作过程需要更多磁盘空间。

- 44.1、48、88.2、96 kHz 支持 1～6 声道；176.4、192 kHz 支持单声道和双声道。
- MLP 目标位深可选 16、20、24 位，LPCM 可选 16、24 位；声道布局沿用输入。
- 极高噪声等素材可能超过 MLP 码流限制。若编码失败，请查看对应曲目的日志，调整音频格式后重试。
- **导入已有 MLP 文件：** 适用于导入由原版 SurCode MLP（surcodemlp.exe）编码生成的 MLP 文件。请选择 MLP 文件夹，并保持与对应音源一致的专辑目录和文件名。此方式直接使用已有编码结果，不重新编码，也不会启动原版 SurCode MLP。请保留原始音源及编码时的采样率、位深等设置，以便进行检查和无损验证；来源或转换参数不明的文件不能仅凭成功导入就视为已验证。

工作文件夹保存检查记录、编码缓存和制作中间文件。请选择空间充足的磁盘；默认会复用有效缓存并继续尚未完成的制作。成品验证完成前，建议保留对应工作文件夹。

## 保存设置与导入配置

设置可直接在界面中修改，通过“保存方案”存成 JSON 文件，之后用“打开方案”继续。日常设置自动保存在 `%LOCALAPPDATA%/DVD-Audio-Maker/settings.json`。

已有 `config.env` 可通过“导入旧配置…”读取。没有已保存的日常设置时，程序也会查找 EXE 旁或当前工作目录的 `config.env`。导入不会改写原文件；旧配置中的 SurCode 导入选项会转换为通用 MLP 导入，请检查音源及 MLP 路径。

随包的 `config.env.example` 只是可选示例，不会自动加载。首次使用无需手工编辑它。右上角可切换“中文 / English / 日本語”，语言偏好随方案保存。

## 查看进度和处理问题

日志区默认显示任务摘要。需要排查问题时，切换到详细日志或点击“保存详细日志”；也可筛选提醒、暂停显示和复制内容。暂停日志显示不会暂停正在执行的任务。

- **程序无法启动：** 确认安装了 .NET 10 Desktop Runtime 的 Windows x64 版本，然后重新打开程序。
- **音源检查失败：** 查看提示的曲目和原因，检查文件是否可读、是否为受支持的无损音源。
- **磁盘空间不足：** 工作目录和成品目录都需要足够空间，可在“更多设置”中更换工作文件夹。
- **制作或验证失败：** 保存完整日志和对应工作目录，修正问题后重试。不要仅凭生成了 ISO 就跳过验证。
- **组件缺失或损坏：** 关闭所有实例后重新启动。仍失败时，重新解压发布包；必要时关闭程序后清理 `%LOCALAPPDATA%/DVD-Audio-Maker/runtime`，下次启动会重建组件缓存。

完整任务日志和启动错误保存在 `%LOCALAPPDATA%/DVD-Audio-Maker/logs`。成品验证检查容量、曲目、播放时间、菜单和音频内容，并核对光盘中的全部 MLP 或 LPCM 数据；不会修改已编码文件。

## 许可与说明

项目许可见 [LICENSE](LICENSE)，第三方组件和字体说明见 [第三方组件说明](tools/win-build/docs/THIRD-PARTY.md)。发布包中的 README 和 RUNTIME 面向最终用户，不包含开发操作。

## 开发与构建

开发需要 Windows x64、.NET 10 SDK 和与源码匹配的原生组件。普通 C# 修改可复用已有组件；重新编译原生代码使用 MSYS2/MinGW-w64 和 Python。仓库中的 dvda-author 目录是局部源码镜像，完整依赖需按构建说明准备。

```bat
gui-debug.cmd
cli.cmd config
cli.cmd build --dry-run --config "C:/work/test.env"
dotnet build DVD-Audio-Maker.sln -c Release -p:SelfContained=false -m:1
```

`dry-run` 仅供开发调试，通过源码 CLI 使用；它会准备音源、编码 MLP 并生成独立索引，不生成 ISO，也不是零写入操作。GUI 只提供检查、制作、验证。配置名 `surcode-batch` 保留为兼容键；旧 `surcode` 导入值映射为 `external`，不再是单独的编码方式。

- [开发调试与 CLI](docs/DEVELOPMENT.md)
- [C# 应用层迁移至 Rust：阶段与验收计划](docs/RUST-MIGRATION.md)
- [C17 格式运行库与逐字节验证](docs/C17-FORMATS.md)
- [Windows 构建、原生组件与发布](tools/win-build/README.md)
- [单文件打包设计与验收](docs/ONEFILE-PUBLISH.md)
- [MLP 集成与测试边界](docs/MLP-ENCODER.md)
- [原生依赖迁移记录](docs/NO-EXTERNAL-RUNTIME-MIGRATION.md)
- [常见问题与历史诊断](docs/TROUBLESHOOTING.md)

默认打包生成单个 EXE 与旁文件，统一压缩为 `DVD-Audio-Maker-v1.0-win-x64.zip`；使用 `--version` 指定后续版本。CLI、PDB、构建来源 JSON 和 .NET 运行时不进入标准用户包。发布物留在被 Git 忽略的目录，仅作为 GitHub Release 附件分发。
