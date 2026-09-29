# DVD-Audio Maker for Windows

此文档随 Windows 自包含发布包分发。发布包用于将 FLAC 或 ALAC/M4A 音源制作成 DVD-Audio ISO。

发布包已包含 C# CLI、修改后的 `dvda-author`、`mkisofs`、菜单工具、ImageMagick 和中日韩字体；运行时不需要安装 WSL、MSYS2、Python 或 .NET Runtime。

## 外部依赖

以下程序需位于 `PATH`，也可以在 `config.sh` 中填写完整路径：

- FFmpeg
- FFprobe
- Metaflac

确认 FFmpeg 支持 MLP 编码：

```bat
ffmpeg -hide_banner -encoders | findstr /i mlp
```

应能看到 `mlp` 编码器。

## 音源目录

建议每个子目录对应一张专辑：

```text
D:\Music\MyAlbums\
├── Album A\
│   ├── 01. First Song.flac
│   ├── 02. Second Song.flac
│   └── cover.jpg
└── Album B\
    ├── 01. Song One.m4a
    └── 02. Song Two.m4a
```

支持：

- FLAC
- M4A 容器中的 ALAC
- JPG、PNG、WebP 专辑封面

每个音频文件建议包含 `date`、`track`、`album` 和 `title` 标签。同一批音源的声道数必须一致；采样率和位深可以混用，准备阶段会按专辑归一化。

## 配置

编辑发布包根目录中的 `config.sh`。Windows 路径建议使用正斜杠：

```bash
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

工具路径和菜单字体由 `dvda.cmd` 根据发布包布局自动设置，无需手工填写 `DVDA_AUTHOR`、`DVDA_MKISOFS` 或字体路径。

配置优先级为：

```text
环境变量 > config.sh > 内置默认值
```

配置文件只解析 `KEY=VALUE`，不会展开变量或执行命令。路径应写成绝对路径。

建议避免让发布包、工作目录或字体路径包含空格和特殊符号，以兼容底层第三方 C 工具。

## 构建 ISO

打开命令提示符或 PowerShell，进入发布包目录：

```bat
cd D:\Tools\DVD-Audio-Maker
```

检查配置：

```bat
dvda.cmd config
dvda.cmd config --check
```

扫描并校验音源：

```bat
dvda.cmd prepare
```

预演 MLP 和分盘，不生成正式 ISO：

```bat
dvda.cmd build --dry-run
```

正式构建：

```bat
dvda.cmd build
```

完整校验：

```bat
dvda.cmd verify all
```

典型完整流程：

```bat
dvda.cmd config --check
dvda.cmd prepare
dvda.cmd build --dry-run
dvda.cmd build
dvda.cmd verify all
```

## MLP 来源

### FFmpeg 编码

```bash
DVDA_MLP_SOURCE="ffmpeg"
DVDA_MLP_EXTERNAL_DIR=""
```

首次构建会编码 MLP，之后复用工作目录中的有效缓存。

### 外部 MLP

```bash
DVDA_MLP_SOURCE="external"
DVDA_MLP_EXTERNAL_DIR="D:/Music/MLP"
```

外部目录必须与音源目录同构，只把扩展名改成 `.mlp`：

```text
音源：D:/Music/MyAlbums/Album/01 Song.flac
MLP ：D:/Music/MLP/Album/01 Song.mlp
```

外部模式仍需要原始音源，用于读取标签、曲序、专辑、封面和时长。构建器会检查 MLP 的参数、结构、EOS、空文件和路径歧义。

## 分盘规则

- 默认以专辑为最小单位，不拆散专辑。
- 按顺序逐盘填满，而不是按目标盘数平均分配。
- `DVDA_MAX_DISCS` 只限制最多允许多少张盘，不参与切分。
- 默认容量为 DVD-5：`4,707,319,808` 字节。
- DVD-9 可设置 `DVDA_DISC_BYTES="8540123136"`。

## 菜单

关闭菜单：

```bash
DVDA_MENU="off"
```

启用菜单和播放封面：

```bash
DVDA_MENU="on"
DVDA_MENU_STILLPICS="on"
DVDA_MENU_TRACKS_PER_PAGE="12"
DVDA_MENU_INDEX_MIN_ALBUMS="4"
DVDA_MENU_COVER_DIM="35"
```

菜单仍属于 DVD-Audio 结构；音频保存在 `ATS_*.AOB` 中，不会转为 DVD-Video 音频。

## 校验命令

```bat
dvda.cmd verify quick
dvda.cmd verify capacity
dvda.cmd verify audit
dvda.cmd verify menu
dvda.cmd verify timeline
dvda.cmd verify lossless
dvda.cmd verify all
```

| 模式 | 检查内容 |
|---|---|
| `quick` | ISO、IFO、轨数、轨首 pack、cell 时间轴和静图引用 |
| `capacity` | ISO 是否超过配置容量 |
| `audit` | AOB 扇区、轨边界、PTS 与构建日志对账 |
| `menu` | 菜单结构、按钮跳转和视觉内容 |
| `timeline` | 规划与成品时间轴 |
| `lossless` | 源音频与 MLP 解码结果 |
| `all` | 执行全部适用检查 |

## 其他工具

检查或对齐 MLP：

```bat
dvda.cmd mlp --check file.mlp
dvda.cmd mlp --align file.mlp
```

检查或修复 ALAC：

```bat
dvda.cmd alac check input.m4a
dvda.cmd alac repair input.m4a output.m4a
```

列出或提取 ISO 内容：

```bat
dvda.cmd iso list disc.iso /AUDIO_TS
dvda.cmd iso extract disc.iso /AUDIO_TS/ATS_01_1.AOB output.aob
```

## 工作目录

`DVDA_BUILD_DIR` 中可能包含：

```text
manifest.json           音源清单
decode_report.txt       解码校验报告
mlp/                    FFmpeg MLP 缓存
mlp_index.json          正式索引和分盘计划
mlp_index-dryrun.json   预演索引
alacfix/                ALAC 修复副本
menu/                   菜单素材
build.log               正式构建日志
build-dryrun.log        预演日志
```

请为工作目录预留足够空间。MLP 缓存和 ISO 都可能达到数 GB。

## 常见问题

### 找不到 FFmpeg、FFprobe 或 Metaflac

把程序目录加入 Windows `PATH`，或在 `config.sh` 中设置：

```bash
DVDA_FFMPEG="D:/Tools/ffmpeg/bin/ffmpeg.exe"
DVDA_FFPROBE="D:/Tools/ffmpeg/bin/ffprobe.exe"
DVDA_METAFLAC="D:/Tools/flac/metaflac.exe"
```

### `config --check` 报音源或输出目录无效

确认路径存在、使用绝对路径，并优先使用正斜杠。输出目录的父目录需要可写。

### 菜单文字为空或字形错误

不要移动或删除发布包中的 `menu-bin/fonts`。启动器会自动设置 SC、JP、KR 三个字体路径。

### `verify audit` 报缺少日志

`audit` 依赖正式构建生成的 `build.log`。预演只生成 `build-dryrun.log`，不能代替正式日志。

### 构建失败后旧 ISO 是否安全

正式发布采用整套事务式替换。构建、复制或容量检查失败时，不会把半套新 ISO 与旧索引混在一起。

## 许可

本发布包中的项目代码按随附 `LICENSE` 发布。第三方组件适用各自许可，详见 `THIRD-PARTY.md`。
