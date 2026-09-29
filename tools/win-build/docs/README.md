# DVD-Audio Maker for Windows

此文档随 Windows x64 自包含发布包分发。

发布包包含已并入 `dvda.exe` 的 SurCode 编码类库，以及 `dvda-author`、`mkisofs`、菜单工具、ImageMagick 和中日韩字体。运行时不需要 WSL、Bash、MSYS2、PowerShell、Python 或单独安装 .NET Runtime。

## 外部依赖

FFmpeg、FFprobe 和 Metaflac 需位于 `PATH`，也可以在 `config.env` 中填写完整路径。

```bat
ffmpeg -hide_banner -encoders | findstr /i mlp
```

## 配置

编辑发布包根目录中的 `config.env`：

```text
DVDA_SRC="D:/Music/MyAlbums"
DVDA_FINAL_DIR="D:/DVD_Output"
DVDA_BUILD_DIR="D:/DVD_Output/_work"
DVDA_TITLE="My DVD-Audio Collection"
DVDA_ISO_PREFIX="MyCollection"
DVDA_MAX_DISCS="2"
DVDA_MLP_SOURCE="ffmpeg"
DVDA_MLP_EXTERNAL_DIR=""
DVDA_MENU="on"
```

配置优先级：

```text
环境变量 > config.env > 内置默认值
```

配置文件只解析 `KEY=VALUE`。不会执行命令或展开变量。请使用绝对 Windows 路径。

工具路径和菜单字体由 `dvda.cmd` 根据发布包布局设置，通常不需要手工配置。

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

FFmpeg：

```text
DVDA_MLP_SOURCE="ffmpeg"
DVDA_MLP_EXTERNAL_DIR=""
```

外部 MLP：

```text
DVDA_MLP_SOURCE="external"
DVDA_MLP_EXTERNAL_DIR="D:/Music/MLP"
```

SurCode 批量编码：

```text
DVDA_MLP_SOURCE="surcode-batch"
DVDA_MLP_EXTERNAL_DIR="D:/Music/MLP"
DVDA_MLP_BATCH_TEMP_DIR="D:/dvda-surcode/temp"
DVDA_MLP_BATCH_OUTPUT_DIR="D:/dvda-surcode/output"
DVDA_MLP_SURCODE_EXE="C:/Tools/SurCode MLP/surcodemlp.exe"
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
