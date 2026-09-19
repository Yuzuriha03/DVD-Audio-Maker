#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""读取 config.sh —— 全部脚本共用的配置。

## 为什么需要它

bash 脚本可以直接 `source config.sh`，但 Python 不能可靠地 source bash。
为了让「用户只改一个文件」成立，这里用一个小解析器读同一份 `config.sh`：
只认 `KEY=VALUE`（值可带引号、行尾可带 # 注释），不做任何命令替换，
因此解析结果与 bash `source` 一致。

## 优先级

    环境变量  >  config.sh  >  内置默认值

这样既能「改文件」，也能临时 `DVDA_SRC=... python3 01_prepare.py`。

## 用法

    from dvda_config import load
    cfg = load()
    print(cfg.src, cfg.manifest)
"""

import os
import re
import sys

# ---- 内置默认值（config.sh 未填时使用） ----
DEFAULTS = {
    # 路径
    "DVDA_SRC": "",
    "DVDA_FINAL_DIR": "",
    "DVDA_BUILD_DIR": "/root/dvda-build",

    # 光盘标识
    "DVDA_TITLE": "DVD-Audio",
    "DVDA_ISO_PREFIX": "",          # 留空则由 TITLE 派生

    # 分盘
    "DVDA_MAX_DISCS": "2",          # 0 = 不限制
    "DVDA_GROUP_TRACK_LIMIT": "64",
    "DVDA_DISC_BYTES": "",          # 留空用 DVD5_BYTES

    # MLP 来源
    "DVDA_MLP_SOURCE": "ffmpeg",    # ffmpeg | external
    "DVDA_MLP_EXTERNAL_DIR": "",

    # 工具
    "DVDA_AUTHOR": "/root/dvda-author-mlp8/src/dvda-author-dev",
    "DVDA_MKISOFS": "/opt/dvda-author/local.ubuntu.20.10/bin/mkisofs",
    "DVDA_FFMPEG": "ffmpeg",
    "DVDA_FFPROBE": "ffprobe",
    "DVDA_AUTHOR_SRC": "/root/dvda-author-mlp8",
    "DVDA_AUTHOR_ORIG": "/opt/dvda-author",

    # 校验
    "DVDA_LOSS_ERROR_S": "0.05",
    "DVDA_LOSS_WARN_S": "0.005",
    "DVDA_ALAC_REPAIR": "1",
}

# DVD-Audio 协议上限
DVD5_BYTES = 4707319808            # 单层 DVD-5
DVD9_BYTES = 8540123136            # 双层 DVD-9
MAX_TRACKS = 99
# dvda-author 的 ATSI 表缓冲固定 3 扇区，超过约 70 轨会栈溢出
GROUP_TRACK_HARD_LIMIT = 70

# MLP 进入 AOB 后的实测开销系数（实测 80,021,504 / 78,337,762 = 1.02150）
AOB_OVERHEAD = 1.025
ISO_SAFETY = 8 * 1024 ** 2         # ISO 文件系统与 IFO 预留

# 必需的配置项
REQUIRED = {
    "DVDA_SRC": "音源目录",
    "DVDA_FINAL_DIR": "ISO 输出目录",
}

_LINE = re.compile(r"^\s*([A-Za-z_][A-Za-z0-9_]*)\s*=\s*(.*?)\s*$")


def config_path():
    """定位 config.sh。

    优先级：$DVDA_CONFIG > 脚本目录/config.sh > 当前目录/config.sh
    """
    env = os.environ.get("DVDA_CONFIG")
    if env:
        return env
    here = os.path.dirname(os.path.abspath(__file__))
    for cand in (os.path.join(here, "config.sh"),
                 os.path.join(os.getcwd(), "config.sh")):
        if os.path.exists(cand):
            return cand
    return None


def _unquote(v):
    """去掉成对的引号（单/双）。"""
    if len(v) >= 2 and v[0] == v[-1] and v[0] in ("'", '"'):
        return v[1:-1]
    return v


def parse(path):
    """解析 config.sh，返回 {KEY: VALUE}（只含显式写出的项）。"""
    out = {}
    if not path or not os.path.exists(path):
        return out
    with open(path, encoding="utf-8", errors="replace") as f:
        for raw in f:
            line = raw.strip()
            if not line or line.startswith("#"):
                continue
            m = _LINE.match(raw.rstrip("\n"))
            if not m:
                continue
            key, val = m.group(1), m.group(2)
            # 去掉行尾注释（值未加引号时才安全地处理）
            if not (val[:1] in ("'", '"') and val[-1:] == val[:1]):
                hash_pos = val.find(" #")
                if hash_pos < 0:
                    hash_pos = val.find("\t#")
                if hash_pos >= 0:
                    val = val[:hash_pos].rstrip()
            val = _unquote(val.strip())
            val = val.replace("\\", "/") if _looks_like_path(val) else val
            out[key] = val
    return out


def _looks_like_path(v):
    """粗略判断是否像路径（Windows 反斜杠需要转成 /，便于 WSL 使用）。"""
    if not v:
        return False
    return bool(re.match(r"^(/|[A-Za-z]:[\\/]|\\\\)", v)) or "\\" in v


def _derive_prefix(title):
    """由标题派生 ISO 文件名前缀：非字母数字 → 下划线，合并连续下划线。"""
    s = re.sub(r"[^0-9A-Za-z]+", "_", title or "").strip("_")
    return s or "DVD_Audio"


class Config:
    """解析后的配置。属性名去掉 DVDA_ 前缀并用小写。"""

    def __init__(self, values):
        self._values = values
        self._path = config_path()

    # ---- 取值 ----
    def get(self, key, default=None):
        if key in os.environ and os.environ[key] != "":
            return os.environ[key]
        if key in self._values and self._values[key] != "":
            return self._values[key]
        return DEFAULTS.get(key, default)

    def get_int(self, key):
        raw = self.get(key)
        try:
            return int(str(raw).strip())
        except (TypeError, ValueError):
            return None

    def get_float(self, key):
        raw = self.get(key)
        try:
            return float(str(raw).strip())
        except (TypeError, ValueError):
            return None

    # ---- 基本 ----
    @property
    def src(self):
        return self.get("DVDA_SRC").rstrip("/")

    @property
    def build_dir(self):
        return self.get("DVDA_BUILD_DIR").rstrip("/")

    @property
    def final_dir(self):
        return self.get("DVDA_FINAL_DIR").rstrip("/")

    @property
    def title(self):
        return self.get("DVDA_TITLE")

    @property
    def iso_prefix(self):
        v = self.get("DVDA_ISO_PREFIX")
        return v or _derive_prefix(self.title)

    def volid(self, index):
        """第 index 张盘（从 1 起）的卷标。"""
        return f"{self.title} {index}"

    def iso_name(self, index):
        """第 index 张盘的 ISO 文件名。"""
        return f"{self.iso_prefix}_{index}.iso"

    # ---- 派生路径（全部在 build_dir 下） ----
    def _under(self, *parts):
        return os.path.join(self.build_dir, *parts)

    @property
    def manifest(self):
        return self._under("manifest.json")

    @property
    def report(self):
        return self._under("decode_report.txt")

    @property
    def out_root(self):
        return self._under("out")

    @property
    def tmp_root(self):
        return self._under("tmp")

    @property
    def iso_dir(self):
        return self._under("iso")

    @property
    def mlp_dir(self):
        return self._under("mlp")

    @property
    def mlp_index(self):
        return self._under("mlp_index.json")

    @property
    def alac_fix_dir(self):
        return self._under("alacfix")

    @property
    def build_log(self):
        return self._under("build.log")

    # ---- 工具 ----
    @property
    def dvda(self):
        return self.get("DVDA_AUTHOR")

    @property
    def mkisofs(self):
        return self.get("DVDA_MKISOFS")

    @property
    def ffmpeg(self):
        return self.get("DVDA_FFMPEG")

    @property
    def ffprobe(self):
        return self.get("DVDA_FFPROBE")

    @property
    def author_src(self):
        return self.get("DVDA_AUTHOR_SRC").rstrip("/")

    @property
    def author_orig(self):
        return self.get("DVDA_AUTHOR_ORIG").rstrip("/")

    # ---- 分盘 ----
    @property
    def max_discs(self):
        """目标盘数；0 表示不限制。"""
        v = self.get_int("DVDA_MAX_DISCS")
        return 0 if v is None else v

    @property
    def group_track_limit(self):
        v = self.get_int("DVDA_GROUP_TRACK_LIMIT")
        if v is None:
            v = 64
        return max(1, min(v, GROUP_TRACK_HARD_LIMIT))

    @property
    def disc_bytes(self):
        v = self.get_int("DVDA_DISC_BYTES")
        return v if v else DVD5_BYTES

    # ---- MLP 来源 ----
    @property
    def mlp_source(self):
        """MLP 从哪来："ffmpeg"（本工具链编码）或 "external"（外部产出）。"""
        v = (self.get("DVDA_MLP_SOURCE") or "ffmpeg").strip().lower()
        return v if v in ("ffmpeg", "external") else "ffmpeg"

    @property
    def mlp_external_dir(self):
        """外部 MLP 根目录（仅 mlp_source == external 时有意义）。"""
        return (self.get("DVDA_MLP_EXTERNAL_DIR") or "").strip().rstrip("/")

    @property
    def use_external_mlp(self):
        return self.mlp_source == "external"

    # ---- 校验 ----
    @property
    def loss_error_s(self):
        v = self.get_float("DVDA_LOSS_ERROR_S")
        return 0.05 if v is None else v

    @property
    def loss_warn_s(self):
        v = self.get_float("DVDA_LOSS_WARN_S")
        return 0.005 if v is None else v

    @property
    def alac_repair(self):
        return str(self.get("DVDA_ALAC_REPAIR")).strip() not in ("0", "no", "false", "")

    # ---- 诊断 ----
    def describe(self, keys=None):
        keys = keys or sorted(set(DEFAULTS) | set(self._values))
        src = "环境变量" if any(k in os.environ and os.environ[k]
                                for k in keys) else "config.sh"
        lines = [f"配置文件: {self._path}",
                 f"优先级  : 环境变量 > config.sh > 内置默认值"
                 f"（当前有环境变量覆盖）" if src == "环境变量"
                 else "优先级  : 环境变量 > config.sh > 内置默认值"]
        for k in keys:
            v = self.get(k)
            mark = ""
            if k in os.environ and os.environ[k]:
                mark = "  [环境变量]"
            elif k in self._values and self._values[k]:
                mark = "  [config.sh]"
            else:
                mark = "  [默认值]"
            lines.append(f"  {k:<24} = {v!r}{mark}")
        return "\n".join(lines)


def load(need=("DVDA_SRC", "DVDA_FINAL_DIR"), quiet=False):
    """解析配置并校验必需项。

    need 为需要校验非空的键；传 None 可跳过校验。
    校验失败时打印友好提示并以非零码退出（脚本入口直接可用）。
    """
    cfg = Config(parse(config_path()))
    if need:
        missing = [k for k in need if not cfg.get(k)]
        if missing:
            p = cfg._path
            print("=" * 68, file=sys.stderr)
            print("[配置缺失] 请编辑配置文件后重试", file=sys.stderr)
            print("=" * 68, file=sys.stderr)
            if p:
                print(f"  配置文件: {p}", file=sys.stderr)
            else:
                print("  未找到 config.sh —— 请把它与脚本放在同一目录，", file=sys.stderr)
                print("  或设置环境变量 DVDA_CONFIG 指向它", file=sys.stderr)
            print(file=sys.stderr)
            for k in missing:
                label = REQUIRED.get(k, k)
                print(f"  还未填写: {label}", file=sys.stderr)
                print(f"            {k}=\"/你的/路径\"", file=sys.stderr)
            print(file=sys.stderr)
            print("  示例:", file=sys.stderr)
            print('    DVDA_SRC="/mnt/d/Music/我的专辑"', file=sys.stderr)
            print('    DVDA_FINAL_DIR="/mnt/d/DVD_Output"', file=sys.stderr)
            print(file=sys.stderr)
            sys.exit(2)
    if not quiet:
        print(f"[配置] {cfg._path or '（未找到 config.sh，使用默认值）'}")
        print(f"       音源     : {cfg.src}")
        print(f"       输出     : {cfg.final_dir}")
        print(f"       工作目录 : {cfg.build_dir}")
        print(f"       光盘标题 : {cfg.title}   ISO 前缀: {cfg.iso_prefix}")
    return cfg


def main():
    """命令行接口。

    python3 dvda_config.py           查看当前生效的配置（含派生路径与工具可用性）
    python3 dvda_config.py --shell   输出 KEY='VALUE' 形式的赋值，供 bash eval
                                     这样 bash 与 Python 共用同一套解析结果，
                                     不会出现两边不一致。
    python3 dvda_config.py --check   只做必需项校验，成功静默退出 0
    """
    argv = sys.argv[1:]

    if "--shell" in argv:
        cfg = load(need=None, quiet=True)
        pairs = [
            ("DVDA_SRC", cfg.src),
            ("DVDA_FINAL_DIR", cfg.final_dir),
            ("DVDA_BUILD_DIR", cfg.build_dir),
            ("DVDA_TITLE", cfg.title),
            ("DVDA_ISO_PREFIX", cfg.iso_prefix),
            ("DVDA_MANIFEST", cfg.manifest),
            ("DVDA_REPORT", cfg.report),
            ("DVDA_OUT_ROOT", cfg.out_root),
            ("DVDA_TMP_ROOT", cfg.tmp_root),
            ("DVDA_ISO_DIR", cfg.iso_dir),
            ("DVDA_MLP_DIR", cfg.mlp_dir),
            ("DVDA_MLP_INDEX", cfg.mlp_index),
            ("DVDA_ALAC_FIX_DIR", cfg.alac_fix_dir),
            ("DVDA_BUILD_LOG", cfg.build_log),
            ("DVDA_AUTHOR", cfg.dvda),
            ("DVDA_MKISOFS", cfg.mkisofs),
            ("DVDA_FFMPEG", cfg.ffmpeg),
            ("DVDA_FFPROBE", cfg.ffprobe),
            ("DVDA_AUTHOR_SRC", cfg.author_src),
            ("DVDA_AUTHOR_ORIG", cfg.author_orig),
            ("DVDA_MAX_DISCS", str(cfg.max_discs)),
            ("DVDA_GROUP_TRACK_LIMIT", str(cfg.group_track_limit)),
            ("DVDA_DISC_BYTES", str(cfg.disc_bytes)),
            ("DVDA_MLP_SOURCE", cfg.mlp_source),
            ("DVDA_MLP_EXTERNAL_DIR", cfg.mlp_external_dir),
            ("DVDA_LOSS_ERROR_S", str(cfg.loss_error_s)),
            ("DVDA_LOSS_WARN_S", str(cfg.loss_warn_s)),
            ("DVDA_ALAC_REPAIR", "1" if cfg.alac_repair else "0"),
        ]
        for k, v in pairs:
            # 单引号包裹，内部单引号做转义 —— bash eval 后值原样保留
            print("%s='%s'" % (k, str(v).replace("'", "'\\''")))
        return 0

    if "--check" in argv:
        load(need=("DVDA_SRC", "DVDA_FINAL_DIR"), quiet=True)
        return 0

    cfg = load(need=None, quiet=True)
    print(cfg.describe())
    print()
    print("派生路径:")
    for name, val in (("manifest", cfg.manifest), ("report", cfg.report),
                      ("out_root", cfg.out_root), ("tmp_root", cfg.tmp_root),
                      ("iso_dir", cfg.iso_dir), ("mlp_dir", cfg.mlp_dir),
                      ("mlp_index", cfg.mlp_index),
                      ("alac_fix_dir", cfg.alac_fix_dir),
                      ("build_log", cfg.build_log)):
        print(f"  {name:<14} = {val}")
    print()
    print("MLP 来源:")
    if cfg.use_external_mlp:
        d = cfg.mlp_external_dir
        ok = "✔" if (d and os.path.isdir(d)) else "✗ 目录不存在"
        print(f"  external      = {d or '(未设 DVDA_MLP_EXTERNAL_DIR)'}   {ok}")
        print("                  （跳过编码；按 <外部目录>/<专辑目录>/<曲名>.mlp 取文件）")
    else:
        print(f"  ffmpeg        = 本工具链自行编码 -> {cfg.mlp_dir}")
    print()
    print("工具:")
    for name, val in (("dvda-author", cfg.dvda), ("mkisofs", cfg.mkisofs),
                      ("ffmpeg", cfg.ffmpeg), ("ffprobe", cfg.ffprobe)):
        sval = str(val)
        if os.path.sep in sval:
            ok = "✔" if os.path.exists(sval) else "✗ 不存在"
        else:
            ok = "(用 PATH 解析)"
        print(f"  {name:<14} = {sval}   {ok}")
    print()
    print("分盘:")
    print(f"  max_discs         = {cfg.max_discs or '不限制'}")
    print(f"  group_track_limit = {cfg.group_track_limit}")
    print(f"  disc_bytes        = {cfg.disc_bytes:,}")
    print()
    print("目录就绪检查:")
    for label, d in (("音源", cfg.src), ("输出", cfg.final_dir),
                     ("工作目录", cfg.build_dir)):
        if not d:
            print(f"  {label:<8} = (未配置)")
        elif os.path.isdir(d):
            print(f"  {label:<8} = {d}   ✔")
        elif label == "输出":
            print(f"  {label:<8} = {d}   （尚不存在，出盘时会自动创建）")
        else:
            print(f"  {label:<8} = {d}   ✗ 不存在")
    print()
    return 0


if __name__ == "__main__":
    sys.exit(main())
