"""Reproduce the fixed-baseline source/test inventory; this is not a proof of equivalence.

The explicit mapping is reviewed with the migration checklist. Historical C# is
read from Git only, never restored, compiled, or used as a runtime fallback.
"""
import hashlib
import json
from pathlib import Path
import re
import subprocess

REPO = Path(__file__).resolve().parents[2]
BASELINE = "fb3cbb84788a807758fa6a8f472a9ffdad3115e8"
CORE = "rust/crates/dvda-core/"
NATIVE = "rust/crates/dvda-native/"
CLI = "rust/crates/dvda-cli/"
GUI = "rust/crates/dvda-desktop/"
TOOLS = "rust/crates/dvda-toolchain/"


def git(*args):
    return subprocess.check_output(["git", *args], cwd=REPO).decode("utf-8-sig")


# Each former compilation unit is accounted for; supporting types inherit the
# implementation owning their behavior. The JSON includes private signatures.
SOURCE_MAP = {
    "DvdaMaker.Building": {
        "AobPtsAnalyzer": "aob", "AobSectorScanner": "aob",
        "AudioParameterProbe": "audio", "BuildLogWriter": "buildlog",
        "BuildMath": "planner", "BuildModels": "disc", "BuildPipeline": "build",
        "BuildPlanService": "disc", "DiscBuildExecutor": "build", "DiscPlanner": "disc",
        "DiscPublisher": "publication", "DiscResumeStore": "resume",
        "DiscVerification": "verify,verification,ifo", "DiskSpacePlanner": "disk_space,workflow",
        "DvdaAuthorCommandBuilder": "author", "ExternalMlpProvider": "acquisition,mlp_import",
        "LpcmProvider": "lpcm", "ManifestBuildReader": "manifest",
        "MenuAssetBuilder": "menu", "MenuBuildVerifier": "menu_check",
        "MenuDiscVerifier": "menu_verify", "MenuFontResolver": "menu_fonts",
        "MenuModels": "menu", "MenuPlanner": "menu", "MenuVisualVerifier": "menu_check",
        "MlpCacheIndex": "cache,mlp_workflow", "MlpCacheValidator": "mlp_workflow",
        "MlpIndexWriter": "index", "NativeDiscVerifier": "verify_audio",
        "PcmComparer": "@native", "SurcodeMlpProvider": "mlp_workflow",
        "VerificationPipeline": "verify,verify_audio",
    },
    "DvdaMaker.Configuration": {
        "ConfigDefaults": "options", "ConfigLoader": "app,options",
        "ConfigurationFileInterop": "config_files", "DvdaOptions": "app,options",
        "ManagedDvdaOptions": "options", "OptionEvaluation": "options",
        "ProjectSettings": "config_files", "ShellFormatter": "options",
    },
    "DvdaMaker.Preparation": {
        "AlacEndRepairer": "preparation/alac", "AlbumNormalizer": "preparation/rules",
        "AudioMetadataReader": "audio", "DecodeValidator": "audio",
        "FileIdentity": "identity", "FlacMetadataEditor": "formats",
        "M4aFlacConverter": "conversion", "Models": "preparation/models",
        "PreparationPipeline": "preparation/pipeline", "PreparationSnapshot": "preparation/snapshot",
        "PrepareCache": "preparation/state",
    },
    "DvdaMaker.Processes": {
        "BuiltinImages": "images", "BuiltinMedia": "media", "BundledRuntime": "runtime",
        "CommandLineFormatter": "process", "ExecutablePath": "process",
        "FileHash": "hash", "ProcessExecutionException": "process",
        "ProcessRequest": "process", "ProcessResult": "process", "ProcessRunner": "process",
        "RustBridge": "@excluded",
    },
    "DvdaMaker.SurcodeTool": {
        "FfmpegPcmConverter": "media", "MlpEncoder": "encoder",
        "SurcodeBatchEncoder": "batch", "SurcodeEncodingJob": "batch", "SurcodePcmWav": "pcm",
    },
}


def current_sources(path):
    parts = Path(path).parts
    project, stem = parts[1], Path(path).stem
    if stem == "AssemblyInfo":
        return [], "Excluded: .NET assembly metadata has no Rust runtime behavior."
    if project in SOURCE_MAP:
        mapping = SOURCE_MAP[project][stem]
        if mapping == "@native":
            return ["tools/formats-native/dvda-formats.c", NATIVE + "src/lib.rs"], "Retained in C17."
        if mapping == "@excluded":
            return [], "Excluded by user: C# host/bridge/fallback."
        return [CORE + "src/" + x + ".rs" for x in mapping.split(",")], "Retained behavior; removed external executable/config.env branches remain excluded."
    if project == "DvdaMaker.Desktop":
        return [GUI + "src/main.rs", GUI + "src/presentation.rs"], "Rust Win32 behavior; WinForms implementation and GUI dry-run excluded."
    if project == "DvdaMaker.Cli":
        return [CLI + "src/developer.rs", CLI + "src/main.rs"], "Developer commands retained with JSON profiles; old env file format excluded."
    if project == "DvdaMaker.FontTool":
        return [TOOLS + "src/fonts.rs", TOOLS + "src/main.rs"], "Font developer commands share toolchain executable; no extra shipped EXE."
    if project == "DvdaMaker.Toolchain":
        return [TOOLS + "src/" + s + ".rs" for s in ["main", "validation", "pe", "publication"]], "Source-built input validation retained; .NET publish/legacy portable dependency pruning excluded."
    if project == "DvdaMaker.Formats":
        return [CORE + "src/formats.rs", NATIVE + "src/lib.rs", "tools/formats-native/dvda-formats.c"], "Native format behavior retained in C17; C# wrapper removed."
    if project == "DvdaMaker.Localization":
        return [CORE + "src/localization.rs", CORE + "src/config_files.rs", GUI + "src/main.rs"], "Three-language behavior; opaque paths/names preserved."
    if project == "Shared":
        return [CORE + "src/runtime.rs", NATIVE + "src/compression.rs"], "Native onefile archive format replaces managed archive; extraction protections retained."
    raise AssertionError(f"Unmapped historical source: {path}")


def test_mapping():
    result = {}
    def put(ids, *paths):
        for item in ids:
            assert item not in result
            result[item] = list(paths)
    put([1, 2], CORE+"src/runtime.rs", "tools/win-build/test-rust-onefile.py")
    put([3], CORE+"tests/flac_metadata.rs", CORE+"tests/conversion_native.rs")
    put([4, 5, 7], CORE+"src/localization.rs", GUI+"src/main.rs", "tools/win-build/test-rust-gui.py")
    put([6], CORE+"src/lpcm.rs", CORE+"src/disc.rs", CORE+"tests/application_native.rs")
    put([8, 17, 18, 19, 22], CLI+"tests/developer_commands.rs", CORE+"src/app.rs", GUI+"src/main.rs", "tools/win-build/test-rust-gui.py")
    put([9, 26])
    put([10, 11, 12, 13, 14, 15, 16, 32, 33, 34, 86], CORE+"tests/options_parity.rs", CLI+"tests/developer_commands.rs")
    put([20, 21], GUI+"src/main.rs", GUI+"src/presentation.rs", CORE+"src/process.rs", "tools/win-build/test-rust-gui.py")
    put([23, 27, 30, 109, 110, 115], CORE+"tests/encoder_native.rs", CORE+"tests/application_native.rs")
    put([24, 25], CORE+"tests/media_native.rs", CORE+"tests/audio_native.rs")
    put([28, 31], CORE+"tests/pcm_streaming.rs")
    put([29], CORE+"tests/encoder_native.rs", CORE+"src/acquisition.rs")
    put([35], CORE+"tests/native_format_parity.rs")
    put([36, 37, 38, 39, 40, 41, 101], NATIVE+"src/format_parity.rs")
    put([42, 43, 44], CLI+"tests/process_native.rs", CORE+"src/process.rs", CORE+"src/buildlog.rs")
    put([45, 46], CORE+"tests/preparation_native.rs")
    put([47], CORE+"tests/audio_native.rs", CORE+"src/audio.rs")
    put([48], CORE+"src/manifest.rs")
    put([49, 50, 51, 52, 59], CORE+"src/disc.rs")
    put([53], CORE+"tests/encoder_native.rs", "tools/win-build/test-rust-package.py", "tools/win-build/test-rust-onefile.py")
    put([54, 81], CORE+"src/acquisition.rs", CORE+"tests/application_native.rs")
    put([55], CORE+"src/index.rs", CORE+"tests/application_native.rs")
    put([56, 83, 84, 85, 114], CORE+"tests/application_native.rs", CLI+"tests/developer_commands.rs")
    put([57, 58, 60, 89], CORE+"src/author.rs", CORE+"tests/application_native.rs")
    put([61], CORE+"src/buildlog.rs", CORE+"tests/application_native.rs")
    put([62, 63, 64, 65], CORE+"tests/publication_parity.rs")
    put([66, 67, 68], CORE+"tests/preparation_native.rs", CORE+"tests/native_format_parity.rs")
    put([69, 70, 71, 72, 73, 74], CORE+"tests/conversion_native.rs", CORE+"src/conversion.rs", CORE+"tests/flac_metadata.rs")
    put([75, 76, 107, 108], CORE+"src/aob.rs", CORE+"tests/application_native.rs")
    put([77, 78, 79, 80], CORE+"src/verification.rs", CORE+"tests/application_native.rs")
    put([82], CORE+"src/verify.rs", CORE+"tests/application_native.rs")
    put([87, 88, 96], CORE+"tests/menu_planning_parity.rs", CORE+"tests/application_native.rs")
    put([90], CORE+"src/menu_fonts.rs", CORE+"tests/application_native.rs")
    put([91], TOOLS+"src/fonts.rs", "build/rust-font-migration/font-validation.json")
    put([92], TOOLS+"src/pe.rs", TOOLS+"src/validation.rs", TOOLS+"src/main.rs", "tools/win-build/test-rust-package.py")
    put([93], CORE+"tests/menu_planning_parity.rs", CORE+"tests/images_native.rs")
    put([94], CORE+"src/menu.rs", CORE+"tests/menu_planning_parity.rs")
    put([95], CORE+"tests/application_native.rs", CORE+"src/menu_check.rs")
    put([97, 98], CORE+"src/menu_verify.rs", CORE+"tests/application_native.rs")
    put([99, 100], "tools/win-build/test-rust-build-entrypoints.py")
    put([102], CORE+"src/workflow.rs")
    put([103, 104, 105, 106], CORE+"tests/preparation_native.rs", CORE+"src/preparation/snapshot.rs")
    put([111], CORE+"src/cache.rs", CORE+"src/mlp_workflow.rs", CORE+"tests/application_native.rs")
    put([112, 113], CORE+"src/resume.rs", CORE+"src/signature.rs", CORE+"tests/application_native.rs")
    assert set(result) == set(range(1, 116)), sorted(set(range(1,116))-set(result))
    return result


def tests_in(path):
    p = REPO / path
    if p.suffix != ".rs" or not p.exists():
        return []
    text = p.read_text(encoding="utf-8")
    return re.findall(r"#\[test\]\s*(?:#\[[^\n]*\]\s*)*fn\s+(\w+)", text)


def main():
    units = []
    for path in git("ls-tree", "-r", "--name-only", BASELINE, "src").splitlines():
        if not path.endswith(".cs"):
            continue
        source = git("show", BASELINE+":"+path)
        current, scope = current_sources(path)
        for mapped in current:
            assert (REPO / mapped).is_file(), mapped
        # Signatures are an index into the fixed source, not an assertion that a
        # regex has inspected all control flow. Nested branches are reviewed in G/A.
        members = [{"line":i,"signature":line.strip()} for i,line in enumerate(source.splitlines(),1)
                   if re.match(r"\s*(?:public|private|internal|protected|static)\s+.*\(",line)]
        units.append({"historicalSource":path,"historicalSha256":hashlib.sha256(source.encode()).hexdigest(),
                      "historicalMembers":members,"currentSources":current,"scope":scope})
    registry = git("show", BASELINE+":tests/DvdaMaker.CompatibilityTests/Program.cs")
    registry = registry.split("var tests =",1)[1].split("\n};",1)[0]
    entries = re.findall(r'^\s*\("(.*?)", (.*?)\),\s*$',registry,re.M)
    assert len(entries) == 115, len(entries)
    mapped = test_mapping()
    exclusions = {
        9:"用户删除 config.env 解析，不恢复引号/注释/查找旧文件。",
        10:"保留 JSON Windows 路径与原样值；env 文本转义规则排除。",
        11:"保留开发环境覆盖 JSON 优先级；config.env 文件排除。",
        18:"排除旧 env 导入；保留 JSON 未知键、版本校验和方案持久化。",
        24:"保留目标精度/声道/转换行为；外部 FFmpeg 命令字符串排除。",
        25:"保留原生转换失败/取消保护；外部程序 PATH 选择排除。",
        26:"用户删除外部 FFmpeg/eac3to 可执行文件设置与兼容层。",
        28:"保留奇数 PCM 长度/容器封装；不调用 eac3to。",
        33:"保留有效派生键/转义/顺序；删除 DVDA_FFMPEG/FFPROBE/MKISOFS/EAC3TO 外部命令键。",
        53:"保留原生编码库身份/缓存修复；.NET 嵌入资源格式排除。",
        73:"结构化原生 PICTURE 字段取代旧外部文本描述解析，不恢复外部工具。",
        92:"保留普通/延迟 PE 导入与依赖完整性；旧便携二进制包硬编码裁剪规则排除。",
        95:"保留每页批量统计/失败回退；原进程计数替换为进程内图像调用，不再启动 ImageMagick。",
        99:"保留 prepare→build、dry-run 开发开关和 JSON profile；旧 --config env 路径排除。",
    }
    tests=[]
    for i,(name,entry) in enumerate(entries,1):
        references=[{"path":p,"testFunctions":tests_in(p)} for p in mapped[i]]
        tests.append({"id":i,"historicalName":name,"historicalEntry":entry,
                      "disposition":"excluded" if not mapped[i] else "retained-with-explicit-exclusions" if i in exclusions else "retained",
                      "scopeNote":exclusions.get(i,"按旧行为核对，当前证据见所列回归入口；不是重新运行 C#。"),
                      "currentEvidence":references})
    output={"date":"2026-10-05","baselineCommit":BASELINE,"historicalTestCount":115,
            "historicalSourceCount":len(units),
            "scope":"固定基线全源文件/成员索引与115测试逐项映射；测试证据和G01-G14分支复核一起使用，不证明未知输入全等。",
            "currentRunEvidence":"docs/rust-migration-checklist-validation-2026-10-05.json",
            "sources":units,"tests":tests}
    target=REPO/"docs/rust-migration-entrypoints-2026-10-05.json"
    target.write_text(json.dumps(output,ensure_ascii=False,indent=2)+"\n",encoding="utf-8")
    rows=["# 固定 C# 基线的功能与测试映射", "",
          f"基线 `{BASELINE}`：{len(units)} 份 C# 源文件、115 个兼容性测试入口。完整源文件、公开/私有成员签名、Rust/C 对应路径见 [机器可读清单](rust-migration-entrypoints-2026-10-05.json)。",
          "", "本表是逐项映射，不把历史 C# 测试结果冒充本轮执行结果。当前执行记录见 [验收记录](rust-migration-checklist-validation-2026-10-05.json)。一个新回归测试可覆盖多个旧入口；冻结 MLP 116 个样本是另一项计数。被用户排除的行为单独注明。", "",
          "| 编号 | 旧入口 | 当前证据文件 | 范围说明 |", "|---|---|---|---|"]
    for item in tests:
        files="、".join(f"[{Path(r['path']).name}](../{r['path']})" for r in item["currentEvidence"])
        rows.append(f"| {item['id']:03} | `{item['historicalEntry'].replace('|','&#124;')}` | {files or '明确排除'} | {item['scopeNote']} |")
    (REPO/"docs/RUST-MIGRATION-ENTRYPOINTS.md").write_text("\n".join(rows)+"\n",encoding="utf-8")
    print(f"Mapped {len(units)} historical source files and {len(tests)} test entries; no unmapped row.")


if __name__ == "__main__":
    main()
