# Windows x64 GUI 发布包

双击根目录 `DVD-Audio-Maker.exe` 启动图形界面；无参数运行 `dvda.cmd` 也启动 GUI。带命令参数时继续执行 CLI。

配置通过界面编辑并自动保存在用户目录；“导入 config.env”保留读取旧配置的能力，“打开方案 / 保存方案”处理 JSON 配置。首次没有 GUI 设置时读取旁边的旧 env。MLP 编码使用进程内原生 x64 DLL，不需要编码 EXE；eac3to、FFmpeg 解码与制盘工具仍保留。

“预演制作”执行音源准备和 MLP 编码但不创建 ISO；“开始制作”完成出盘。“验证成品”的 PCM 无损检查沿用首轨抽样范围。任务可取消，日志可保存。

界面中的“更多设置”展开较少使用的参数。光盘容量可选 DVD5 / DVD9 / 自定义，采样率和编码方式显示为易读名称。

日志默认显示中文任务摘要；可切换详细日志、只看提醒、暂停实时更新、复制内容和导出完整任务日志。拖动中间分隔条可以调整日志区高度。完整日志自动写入 `%LOCALAPPDATA%/DVD-Audio-Maker/logs`，包含工具原始诊断信息；窗口仅保留最近记录。

# DVD-Audio Maker for Windows

此文档随 Windows x64 自包含发布包分发。

GUI 和 CLI 均内嵌原生 x64 MLP DLL，发布包同时包含 `dvda-author`、`mkisofs`、菜单工具、ImageMagick 和中日韩字体。运行时不需要 WSL、Bash、MSYS2、PowerShell、Python 或单独安装 .NET Runtime。

## 外部依赖

FFmpeg、FFprobe 和 Metaflac 需位于 `PATH`，也可以在 `config.env` 中填写完整路径。

FFmpeg 仅用于转换、解码和校验。批量编码需配置 eac3to，无需原版 SurCode。

## 旧配置与 CLI

GUI 可通过“导入 config.env”读取旧配置；日常设置可直接在界面中编辑。使用 CLI 时，也可以编辑发布包根目录中的 `config.env`：

```text
DVDA_SRC="D:/Music/MyAlbums"
DVDA_FINAL_DIR="D:/DVD_Output"
DVDA_BUILD_DIR="D:/DVD_Output/_work"
DVDA_TITLE="My DVD-Audio Collection"
DVDA_ISO_PREFIX="MyCollection"
DVDA_MAX_DISCS="2"
DVDA_MLP_SOURCE="surcode-batch"
DVDA_MLP_EXTERNAL_DIR=""
DVDA_MLP_EAC3TO_EXE="C:/Tools/eac3to/eac3to.exe"
DVDA_MENU="on"
```

CLI 配置优先级：

```text
环境变量 > config.env > 内置默认值
```

配置文件只解析 `KEY=VALUE`。不会执行命令或展开变量。请使用绝对 Windows 路径。

工具路径和菜单字体由 GUI 或 `dvda.cmd` 根据发布包布局设置，通常不需要手工配置。

## 构建 ISO

```bat
dvda.cmd config --check
dvda.cmd prepare
dvda.cmd build --dry-run
dvda.cmd build
dvda.cmd verify all
```

## 音源

支持 FLAC、M4A/ALAC 及 JPG、PNG、WebP 封面。建议每张专辑使用一个子目录，并提供 `date`、`track`、`album` 和 `title` 标签。

## MLP 来源

默认MLP 编码核心：

```text
DVDA_MLP_SOURCE="surcode-batch"
DVDA_MLP_EXTERNAL_DIR=""
DVDA_MLP_EAC3TO_EXE="C:/Tools/eac3to/eac3to.exe"
```

外部 MLP：

```text
DVDA_MLP_SOURCE="external"
DVDA_MLP_EXTERNAL_DIR="D:/Music/MLP"
```

MLP 编码核心批量编码（保留旧配置名）：

```text
DVDA_MLP_SOURCE="surcode-batch"
DVDA_MLP_EXTERNAL_DIR="D:/Music/MLP"
DVDA_MLP_BATCH_TEMP_DIR="D:/dvda-surcode/temp"
DVDA_MLP_BATCH_OUTPUT_DIR="D:/dvda-surcode/output"
DVDA_MLP_METADATA_CONTEXT=""
DVDA_MLP_EAC3TO_EXE="C:/Tools/eac3to/eac3to.exe"
```

## 校验模式

```bat
dvda.cmd verify quick
dvda.cmd verify capacity
dvda.cmd verify audit
dvda.cmd verify menu
dvda.cmd verify timeline
dvda.cmd verify lossless
dvda.cmd verify all
```

## 常见问题

### 找不到外部程序

在 `config.env` 中设置完整路径：

```text
DVDA_FFMPEG="D:/Tools/ffmpeg/bin/ffmpeg.exe"
DVDA_FFPROBE="D:/Tools/ffmpeg/bin/ffprobe.exe"
DVDA_METAFLAC="D:/Tools/flac/metaflac.exe"
```

### 菜单文字为空或字形错误

不要移动或删除 `menu-bin\fonts`。启动器会自动设置 SC、JP、KR 三个独立字体路径。

### `verify audit` 报缺少日志

`audit` 依赖正式构建生成的 `build.log`；预演日志不能替代它。

## 许可

项目代码按随附 `LICENSE` 发布。第三方组件适用各自许可，详见 `THIRD-PARTY.md`。

MLP 核心内嵌于程序并按 SHA-256 校验；编码后不打补丁。默认辅助元数据固定为空。
历史原版字节对照需要相同 PCM、设置及显式元数据上下文；空上下文不冒充历史时间戳。
