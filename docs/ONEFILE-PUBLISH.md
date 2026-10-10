# Windows x64 单 EXE 发布

标准发布文件为 `DVD-Audio-Maker-v1.0-win-x64.zip`。ZIP 内只放一个用户程序 `DVD-Audio-Maker.exe`、三语言用户 README、LICENSE、两份 THIRD-PARTY 说明及两个根目录 NOTICE。CLI、配置示例 JSON、构建来源 JSON、PDB、.NET 运行时和开发源码不进入用户包。

## 组件布局

必要的 C ABI DLL、共享 FFmpeg 库、制盘组件、菜单素材和一份中日韩字体集合内嵌到 Rust EXE。文档和许可不嵌入。打包器在平铺运行目录时比较同名组件的内容；相同文件只保留一份，内容不同则报错，不能按文件名盲目删除。

打包前核对媒体、图像、author 的构建记录、实际文件集合、大小/SHA-256、原生 x64 PE、普通/延迟导入及共享 FFmpeg profile。原始三份 OTF 可以直接合并为共享 TTC；已有 TTC 也须通过 SC/JP/KR 顺序、名称、覆盖和表校验。候选目录与 ZIP 全部完成后事务提交，失败保留上一份发布。

`--output` 目录根部另生成开发用 `MANIFEST.txt` 和 `release-build.json`，记录最终文件及输入构建清单的哈希。两份记录不进入 `DVD-Audio-Maker/`、用户 ZIP 或内嵌归档。

```text
DVD-Audio-Maker/
  DVD-Audio-Maker.exe
  README.md
  README.en.md
  README.ja.md
  LICENSE
  THIRD-PARTY.md
  THIRD-PARTY.en.md
  NOTICE-Image.txt
  NOTICE-Menu.txt
```

启动时以归档 SHA-256 在 `%LOCALAPPDATA%/DVD-Audio-Maker/runtime/` 下选择缓存目录。逐个核对文件大小和完整 SHA-256，原子替换缺失或损坏文件；Windows 文件锁协调并发启动。归档路径拒绝越界，释放目录拒绝重解析点。压缩和解压使用 Windows 自带的 XPRESS Huffman API。

制盘和 ISO writer 静态编入 GUI，通过 Rust 进程内接口调用；独立开发 author EXE 不进入内嵌运行时。媒体解码、转换、MLP 编码和图像处理通过 Rust 接口与第三方库完成。单 EXE 是交付形式，运行时释放所需第三方库、字体和菜单素材。

## 构建

使用现有 MSYS2 GCC 和 x64 GNU Rust 工具链。静态菜单 vendor 与 Rust author 必须来自经过验证的源码构建；格式处理和只读 MLP/LPCM 成品校验已经内置为 Rust，不打包旧格式或校验 DLL。

```powershell
python -X utf8 tools/win-build/build-rust-author.py --ffmpeg-runtime build/ffmpeg-shared/install --magick-work build/imagemagick-minimal --menu-runtime build/menu-direct-vendor-rust-session --assets-runtime build/prebuilt-release-20261004 --dependency-runtime build/media-native-shared
cargo run --manifest-path rust/Cargo.toml --target x86_64-pc-windows-gnu --release -p dvda-toolchain --offline -- package --repo . --source build/source-release-20261004 --prebuilt build/prebuilt-release-20261004 --output build/rust-author-package --image-runtime build/rust-image-runtime --image-author build/rust-author-production --version v1.0
```

Rust author builder 的可选 `--source` 只用于寻找菜单素材，不编译旧 C author。第三方库、静态菜单 vendor 和其他组件的重建说明见 [Windows 构建](../tools/win-build/README.md)；完整制盘验收见 [Rust author 迁移记录](RUST-AUTHOR-MIGRATION.md)。以上路径按本机已准备的构建目录替换，不要求用户具备开发环境。

未显式指定时，素材源/预构建目录分别取 `DVDA_SRC_TREE` / `DVDA_PREBUILT_DIR`，最后取仓库的 `tools/win-build/prebuilt`；不回退到已删除的 C author 源码树或旧 release 缓存。缺字体或菜单素材会明确失败，需要按源码构建说明准备输入。格式与成品校验无独立运行库输入。

## 验证

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File tools/win-build/test-rust-workflow.ps1 -OtherVolume D:\
python -X utf8 tools/win-build/test-rust-gui.py --exe build/rust-migration-checklist-package/DVD-Audio-Maker/DVD-Audio-Maker.exe
python -X utf8 tools/win-build/test-rust-onefile.py --exe build/rust-migration-checklist-package/DVD-Audio-Maker/DVD-Audio-Maker.exe
```

第一个脚本指定冻结基准使用的原生库，并启用全部集成测试。GUI 脚本用隔离配置检查三语言、保存与恢复、非法输入、滚动和文件对话框。单文件脚本只复制 EXE，移除开发组件环境覆盖，生成测试音频与封面，通过实际 GUI 完成 MLP/LPCM 制作与成品验证，再损坏缓存 DLL 验证自动修复。测试生成的音乐和 ISO 均放在自有临时目录，不使用用户全盘音源。

`-OtherVolume` 指向另一可写卷，仅创建并清理测试自己的临时子目录；没有第二个卷时省略该参数会明确跳过跨卷替换专项，其余集成测试照常执行。

发布产物只留在 Git 忽略目录中，上传 GitHub Release 是单独的发布操作。
