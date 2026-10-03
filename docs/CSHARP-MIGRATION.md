# C# 迁移状态

[简体中文](CSHARP-MIGRATION.md) | [English](CSHARP-MIGRATION.en.md)

> 历史工程记录：本文保留当时的实验、假设和验证结果，部分结论已被后续实验修正。已禁用的方案不是当前配置建议；当前支持的流程请参阅[项目说明](../README.md)。

## 迁移完成状态

已建立第一批可独立编译的 .NET 10 项目：

- `DvdaMaker.Configuration`：实现配置解析、来源优先级和派生值。
- `DvdaMaker.Cli`：提供配置、准备、规划、构建、M4A 转换及成品校验入口。
- `DvdaMaker.Formats`：ISO9660 读取、MLP 对齐校验和 PES PTS 解析。
- `DvdaMaker.Processes`：统一外部程序执行、参数传递、输出捕获、超时和取消。
- `DvdaMaker.Preparation`：音源扫描、ffprobe 元数据、专辑归一化、ffmpeg 解码校验及 manifest 生成。
- `DvdaMaker.Building`：读取 manifest、全局排序、按专辑贪心分盘及盘内参数分组。
- `DvdaMaker.CompatibilityTests`：不依赖 NuGet 测试框架的兼容性基线（当前 88 项）。

C# 已覆盖配置、准备、转换、构建、菜单素材、字体覆盖探测、AMG/ASVS、
菜单视觉和成品审计；`build.cmd` 与 `verify.cmd` 是 C# CLI 的 Windows 原生薄包装。
根目录旧 Python 业务脚本已于 2026-09-29 删除；删除前的最终版本保存在
Git 标签 `python-reference-final`（提交 `0afe53a`）中。

## 构建与测试

需要 .NET 10 SDK：

    dotnet build DVD-Audio-Maker.sln
    dotnet run --project tests/DvdaMaker.CompatibilityTests

当前普通兼容性测试共 88 项，覆盖配置、格式解析、外部进程、音源准备、ALAC 修复、
M4A dry-run 零写入、非 ALAC 拒绝、文件级失败隔离、审计日志解析、MLP 获取与索引、
分盘分组、假 `dvda-author` 正式出盘端到端流程，以及 `build.cmd` 参数分支、
PCM 等长比对、工作盘空间预检、音源校验缓存、MLP 缓存凭据、逐盘续跑与并发编码。

真实参考盘与 SurCode MLP 对拍：

        dotnet run --project tests/DvdaMaker.CompatibilityTests -- \
            --real-fixtures \
            "E:\DISCs\DVD-Audio\Wuthering Waves Singles EPs" \
            "D:\yyz57\Music\output\鸣潮先约电台"

真实样本测试不会把数 GB 的 ISO/MLP 复制进仓库，只固化迁移前参考实现读取出的
路径、LBA、文件哈希、扇区样本哈希和 MLP 结构统计。缺少外部样本时，普通测试
仍可独立运行。

查看配置：

    dotnet run --project src/DvdaMaker.Cli -- config
    dotnet run --project src/DvdaMaker.Cli -- config --shell
    dotnet run --project src/DvdaMaker.Cli -- config --check

运行 C# 音源准备流程：

    dotnet run --project src/DvdaMaker.Cli -- prepare

使用已有 MLP 缓存预览 C# 构建规划：

    dotnet run --project src/DvdaMaker.Cli -- plan

`plan` 不调用 `dvda-author` 或内置 ISO 写入器；缺少对应 MLP 缓存时会列出错误。它实现
曲目排序、专辑聚合、容量估算、贪心分盘、参数分组和专辑边界规则。

执行构建预演：

    dotnet run --project src/DvdaMaker.Cli -- build --dry-run

该命令会按配置使用MLP 编码核心编码/复用 MLP，或定位并探测外部 MLP，然后完成分盘并
写出独立的 `mlp_index-dryrun.json`，不会覆盖正式成品使用的 `mlp_index.json`。
MLP 编码核心在序列化时生成完整头部、校验及终止标志；输出只验证，不进行编码后修补。

无菜单正式出盘：

    dotnet run --project src/DvdaMaker.Cli -- build

正式模式已接入 `dvda-author` 内置 ISO 写入器、审计兼容构建日志、ISO 容量检查、
最终发布和成功后的中间产物清理。`DVDA_MENU=on` 时，
C# 会生成菜单素材并执行菜单出盘及成品菜单校验。正式索引先写入
`mlp_index.pending.json`。全部计划盘先发布到独立 staging 目录，随后将整套 ISO 与索引
作为一个可回滚事务提交；任一目标被占用、复制失败、超出容量、取消或中途失败时，
旧正式 ISO 集合及 `mlp_index.json` 均保持一致，不再产生与索引脱节的 `_new.iso`。

当前 `prepare` 已迁移 FLAC/M4A 扫描、参数归一化、声道一致性检查、解码采样数
校验、报告和 manifest，并已接入 Apple ALAC 未压缩帧缺失 END 标记的自动修复。
修复只写入工作目录副本，原文件不修改；修复帧记录会进入报告及 manifest。

独立的 `convert` / `m4a2flac` 已迁移：只接受首音频流为 ALAC 的 `.m4a`，转换前
检测并修复 END 标记，保留规范化标签和封面，并校验修复后 PCM MD5、输出标签及
封面字节。`--dry-run` 只检测，不创建修复副本或 FLAC；单文件异常不会终止整个批次。

成品校验已拆分为两个明确入口：

- `quick-check`：跨所有 ISO 累计 IFO 轨数，检查轨首 pack、PGC cell 时间轴、
    title 长度及 ATSI/ASVS 静图引用边界。
- `audit`：在候选构建日志中按修改时间选最新文件，剥离 ANSI，按 dvda-author
    命令映射每盘组数，并检查 AOB 扇区、轨间连续性、PTS 完整性和 title 起点重置。

两个独立入口均支持覆盖输入路径：

    dvda quick-check --iso-dir DIR --manifest FILE --log FILE
    dvda audit --iso-dir DIR --manifest FILE --log FILE

单张菜单 ISO 可脱离默认输出目录检查：

    dvda verify menu --iso FILE

若正式索引中存在该 ISO 的规划记录，会同时核对页数、专辑索引格和静图数量；没有
可用索引时仍执行通用 AMG/ASVS 结构及逐页画面检查。结构检查发现问题后，只要菜单
VOB 和 PGC cell 范围仍可读取，也会继续视觉检查并合并报告，不再直接跳过。

缺日志、缺轨道表或缺 dvda-author 命令属于“不可审计”，退出码为 2；发现实际
成品问题退出码为 1。`verify menu/all` 会按 AMG PGC cell 范围逐页抽取
`AUDIO_TS.VOB` 画面，使用 ImageMagick 检查画面统计，并按 4×3 网格逐格检查所有
一级索引页的背景、缩略图和标题亮字（包括不足 12 格的末页）；
外部工具缺失会报告 unavailable，不会误报为通过。

外部 MLP 模式已增加镜像路径优先、basename 重名拒绝、空文件拒绝、MLP 结构和
EOS 校验；写入 `mlp_index.json` 前也会拒绝多个曲目复用同一 MLP 路径。

正式出盘执行器已通过成功与失败两条端到端 fixture：成功路径验证 author 输出、
轨道表日志、ISO 发布及清理；失败路径验证错误诊断、禁止发布和现场保留。

`DVDA_FINAL_DIR` 是唯一最终输出目录。正式出盘事务完成后，ISO 直接发布到该目录，
不再经过 Robocopy 或第二个目标目录。

## 保留边界

- `build_dvda_author_mlp.sh` 及 `tools/` 内部工具链编译脚本仍保留；它们是第三方/本地
    C 工具构建入口，不属于 Python 业务流程。
- Windows 发布包现通过 `dotnet publish -r win-x64 --self-contained` 携带 C# CLI，
    不再复制或启动 Python 业务脚本，目标机器无需 Python 或 .NET Runtime。
- `tools/win-build/make-menu-font.sh` 已改用 `DvdaMaker.FontTool`：按 family 名从
    TTC 提取 SC、JP、KR face，以纯 C# 重建 standalone OpenType，修复校验和并
    校验四种字符覆盖。开发/打包期也不再依赖 Python/fontTools。
- 已删除的 Python 参考实现可通过 `git show python-reference-final:<文件名>` 查看，
    或从 `python-reference-final` 标签建立临时 worktree；不再在主分支保留双份实现。

## 已冻结的配置契约

- 优先级：环境变量 > 所选配置文件 > 内置默认值。
- 只解析 `KEY=VALUE`，不执行变量展开或命令替换。
- 支持成对单引号、双引号和未加引号值的行尾注释。
- 疑似路径中的反斜杠转换为 `/`，保持 Python 当前行为。
- 保持派生路径、ISO 前缀、卷标、文件名和数值限制规则。
- `--shell` 保持 Bash 单引号转义规则和既有键集合。
- `--shell-all` 输出 C# 新增的菜单、旧版元数据工具兼容键和派生目录配置键。
- `--check` 缺少必填路径时返回退出码 2。

## 后续验证

1. 持续运行 88 项普通兼容测试及真实 ISO/SurCode MLP 3 项基线。
2. 对新增的真实 ALAC 样本核对标签、封面和 PCM MD5。
3. 对真实多页菜单 ISO 逐页抽帧，并在 Windows self-contained 发布包中做 smoke test。
4. 若需追查迁移差异，以 `python-reference-final` 标签为只读历史基准。
