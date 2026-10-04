# Windows x64 单文件发布

## 目标与边界

- 默认发布一个 `DVD-Audio-Maker.exe`，文档与许可放在旁边，一同打包为 ZIP；不捆绑 .NET 10 Desktop Runtime，继续保留目录发布和开发 CLI。
- 保留全部 SC/JP/KR 字体、原有媒体功能及 MLP 编码核心，编码后不修改输出文件。
- 缩小实际 EXE：按内容去重原生素材，压缩内嵌资源，省略符号与开发入口；不是调整 ZIP 参数。
- 用户只需复制 EXE。首次运行自动将原生组件和字体释放到用户缓存；运行过程中仍会调用项目自己的原生制盘程序。
- 支持相邻 config.env、显式 --config、GUI 导入和已保存 JSON 配置。
- EXE 内只保留运行需要的 DLL、author、图像配置、完整字体和菜单素材。README、运行说明、配置示例、许可证及组件授权声明作为旁文件；示例命名为 config.env.example，避免覆盖用户配置。两个组件授权文件以 NOTICE-Image.txt、NOTICE-Menu.txt 放在 ZIP 根目录，不创建子文件夹。
- 用运行素材白名单排除未使用的 archive.7z。内嵌的资源索引保留文件哈希与路径，用于缓存完整性检查；构建来源 JSON 仅留在本地构建目录供打包校验，不进入发布包。

## 实现步骤

1. 复用现有打包流程验证原生组件，生成只包含运行素材的压缩资源。相同 SHA-256 内容只存储一次。
2. 将资源嵌入 GUI，由 .NET SDK 生成 framework-dependent x64 single-file；禁止引入运行时或不受支持的 WinForms 裁剪。
3. 启动时按资源版本建立缓存，校验文件长度和 SHA-256，原子完成提取；处理并发启动和缓存损坏。
4. 将媒体、图像、字体和制盘资源定位到缓存，保持开发目录运行方式。
5. 对单独复制到含中文和空格目录的 EXE 验证冷启动、复用、配置导入、预演、完整制作和成品校验；对比 MLP 与既有基准。
6. 记录 EXE 实际字节数、缓存占用、与上一版目录及 ZIP 的差异，并更新使用和构建文档。

## 技术约束

当前 .NET SDK 的 EnableCompressionInSingleFile 要求 SelfContained=true，与不捆绑运行时的要求不符。因此对运行素材使用应用内 Brotli 压缩和内容去重，SDK 负责打包托管入口，不启用该选项。

按用户要求，在源码构建层面合并 FFmpeg 功能：`build-minimal-ffmpeg.py --profile shared` 一次构建媒体与菜单功能的并集，媒体 C 接口和 author 均重新链接该安装前缀。打包核对两份构建清单及重名文件 SHA-256 后，将原生媒体组件统一放在 `menu-bin`；GUI 从这里加载 dvda-media.dll，author 从其自身目录解析相同 FFmpeg DLL。单文件资源和释放缓存都不再保留第二套 DLL。旧目录构建仍可用于对照。

## 当前 GUI 发布验收（2026-10-04）

- GUI 提供检查音源、开始制作、验证成品；移除预演，开发 CLI 继续保留 build --dry-run。
- 音频方式只显示内置编码和通用 MLP 导入。旧 env/JSON 中的 surcode 映射为 external，不再有独立模式；外部文件使用严格 PCM 比较。
- 10 份受版本控制的 README 均已更新，发布包中英文 README 面向最终用户；安装、制作、设置、日志和排错均以实际界面为准。
- 标准附件为 DVD-Audio-Maker-v1.0-win-x64.zip，15,352,026 字节（14.64 MiB），SHA-256：1ab14f89a9b5f3613b1bdae918dfc55338ca7af18388c2ae111ab3fb01baad51。
- EXE 为 16,117,346 字节，SHA-256：8bedbbf93ce306809fdeb3bc6aa72c340e969523052c5f9e832becd138d46297。ZIP 仍是单 EXE 与全部旁文件平铺，两个 NOTICE 不放子目录，不附带开发 JSON 或 .NET。
- Release 构建无警告、无错误；111 项兼容性测试和 31 项发布轻量检查通过。包括中英文 GUI、已移除的 Preview 请求明确失败、旧 env/JSON 导入迁移、单 EXE 启动、并发启动、缓存完整性与修复、ZIP 与本地文件哈希一致。
- 按用户要求未重跑完整制盘。此次未改 MLP 原生核心，也没有完成反编译来源替换；界面及命名调整不代表来源风险已经消除。
- 本机产物：build/release-v1.0-gui-refresh；报告：build/onefile-validation/gui-refresh-smoke/report.json。可审阅摘要见 gui-release-refresh-validation.json。
- 打包参数 --version 默认 v1.0；后续版本使用 DVD-Audio-Maker-<version>-win-x64.zip，不再增加 GUI-only 后缀。

## 前一版平铺发布验收（2026-10-04）

- 最终 ZIP 所有文件平铺在根目录：主 EXE、README、运行说明、LICENSE、THIRD-PARTY、config.env.example、NOTICE-Image.txt、NOTICE-Menu.txt 和 MANIFEST.txt；没有子目录或开发 JSON。
- ZIP：15,351,318 字节（14.64 MiB），SHA-256 为 637240b384d31559183fae96e1b19f443bec5ca5feccdef8e5b1c454462c1c51。
- EXE：16,109,154 字节，SHA-256 为 03496d69a1bba8e8872ee837f08e0b0561adf2b664bc835180b20162ef08ce7b。仅内嵌 25 个运行所需文件，解压资源 29,289,124 字节。
- 构建成功，26 项轻量检查通过，包括 ZIP 平铺布局、无开发 JSON、文件哈希、冷启动、并发启动、缓存复用与修复、配置导入和旧方案路径迁移。按用户要求，没有重跑完整制盘流程。
- 本机产物：build/release-v1.0-onefile/DVD-Audio-Maker-win-x64-GUI-only.zip；报告：build/onefile-validation/release-v1.0-flat-smoke/report.json。
- 发布附件命名为 DVD-Audio-Maker-v1.0-win-x64-GUIonly.zip。发布物不进入 Git，仅上传到 v1.0 Release；源码与开发用构建记录仍保留。

## 旁文件初版验收（2026-10-04，移除开发 JSON 前）

- 发布物是一个 ZIP，包含单 EXE、README、运行说明、许可、config.env.example、components/ 来源与授权清单以及 MANIFEST.txt。只上传这个 ZIP；EXE 和旁文件不拆开分发。
- 本机成品目录：`build/release-v1.0-onefile`。ZIP 为 `DVD-Audio-Maker-win-x64-GUI-only.zip`，15,370,105 字节（14.66 MiB）；发布附件命名为 `DVD-Audio-Maker-v1.0-win-x64-GUIonly.zip`。
- ZIP SHA-256：`df611dbd68ae91563fc0711bca08ce76916fe60ee52316783abbe8ee43436428`。
- EXE 为 16,109,154 字节（15.36 MiB），SHA-256：`8b9b25537c0456be9ffbabe3ffb0cbf13781c875d3defd95da854992ff3c910b`。内嵌 25 个必要运行文件，总计 29,289,124 字节；缓存另含完整性索引。
- 构建与 24 项轻量检查通过：ZIP/清单逐文件核对、内嵌白名单、独立 EXE 启动、中文路径、并发启动、缓存复用与修复、配置导入和旧方案路径迁移。
- 按用户要求，本次旁文件调整未重跑完整制盘。原生二进制和字体哈希与前一版完整回归的产物一致；MLP 核心未改变。
- 本机报告：`build/onefile-validation/release-v1.0-smoke/report.json`；命令使用 `test-onefile-release.py --startup-only`。发布产物仍被 Git 忽略，只上传到 GitHub Release。

## 前一版验收记录（2026-10-04，调整旁文件之前）

- 成品：`build/release-onefile-shared/DVD-Audio-Maker.exe`，Windows x64 GUI，只有一个 EXE，不包含 .NET 运行时或开发 CLI。
- 大小：16,207,458 字节（15.46 MiB）；原目录包 ZIP 为 19,668,049 字节，分发体积减少 17.6%。未改用其他外部压缩格式。
- SHA-256：`ad747ce111fea5019b1e08c3dd3c0d7ade9935c6883f4b5368d5d585e3e7a537`。
- 首次运行释放的资源合计 29,484,964 字节，另有校验清单；缓存和 EXE 同时占用磁盘。资源包含运行组件、完整字体、许可和组件构建清单，不包含 README、运行说明和 config.env 示例。读取外部 config.env 的能力保留。
- 最终 EXE 验收 20 项通过，GUI 全流程 68 项通过，多盘、多音轨组及 44.1 kHz / 20 bit 工作流 10 项通过。覆盖独立 EXE、中文路径、并发启动、缓存复用与修复、配置导入、旧方案缓存路径迁移。
- 共享原生库的 PCM / MLP 矩阵 202/202 通过，所有样本整文件 MLP SHA-256 与先前基准相同；最终 GUI 制盘的 6 个整文件 MLP 与基准逐字节相同。媒体操作 35 项、PAL/NTSC 菜单 41 项、托管兼容性 111 项通过；开发目录包 CLI 读取配置正常。
- MLP 核心未修改，编码完成后不修补码流。上述字节一致结论限于所列测试样本，不替代对任意输入的证明。

详细报告位于本机 `build/onefile-validation/` 下：`final-acceptance/report.json`、`pcm-matrix/result.json`、`media-operations/report.json`、`menu-media/report.json` 和 `compatibility.log`。构建产物与回归材料不提交到 Git；本记录留在仓库，不嵌入 EXE。
