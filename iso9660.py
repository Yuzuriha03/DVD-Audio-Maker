#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""纯 Python 的 ISO9660 读取器 —— 替代 xorriso 与 dd。

## 为什么需要

出盘后的校验脚本要干两件事：

  1. **列出** ISO 里的路径        （原来是 `xorriso -find /`）
  2. **取出** ISO 里的文件/目录   （原来是 `xorriso -osirrox on -extract …`）
     以及按扇区号读一小段         （原来是 `dd if=… bs=2048 skip=N count=M`）

这三件事在 Linux 上用外部命令没问题，但 **Windows 上都没有**，而且
**不是「装一下就有」** —— 实测 MSYS2 也没有 xorriso 这个包：

    $ pacman -Ss xorriso
    （无输出）

于是校验脚本在 Windows 上完全没法用。本模块只用标准库实现，两个平台都能跑。

## ⚠️ 只用 PVD，不读 Joliet / Rock Ridge

我们要找的名字（`AUDIO_TS`、`ATS_01_0.IFO`、`ATS_01_1.AOB`、`AUDIO_SV.VOB` …）
都是 ISO level 1 合法名（8.3 大写），**一定在 Primary Volume Descriptor 里**。
Joliet 的条目是 UTF-16BE，`decode("ascii")` 会抛异常 —— 直接跳过即可，
不需要为了它去解析第二个卷描述符。

代价：`all_paths()` 看不到超过 8.3 的长文件名（例如 `NotoSansCJKsc-Regular.otf`
那种，如果它被放进 ISO 的话 —— 实际不会）。调用方只用 `os.path.basename()`
判断关键文件在不在，而那些都是短名，所以不受影响。

## 用法

    from iso9660 import Iso9660

    with Iso9660(iso_path) as iso:
        names = iso.all_paths()                          # ['/AUDIO_TS/ATS_01_0.IFO', …]
        data  = iso.read_file('AUDIO_TS/AUDIO_TS.IFO')   # bytes
        iso.extract('AUDIO_TS', r'D:\\tmp\\AUDIO_TS')     # 目录、文件都行
        raw   = iso.read_sectors(5173, 8)                 # 按扇区读
"""

import os
import struct

# ISO9660 的逻辑块（扇区）默认大小；实际值从 PVD 读，这里只作回退。
DEFAULT_SEC = 2048

# 卷描述符搜索范围：规范说从扇区 16 起，连续读若干张。
_VD_SEARCH = 32


class IsoError(Exception):
    """ISO 结构异常（无法解析、找不到条目、多段文件等）。"""


def _u16le(b, o):
    return struct.unpack("<H", b[o:o + 2])[0]


def _u32le(b, o):
    return struct.unpack("<I", b[o:o + 4])[0]


def _u32be(b, o):
    return struct.unpack(">I", b[o:o + 4])[0]


class _Entry(object):
    """一条目录记录。"""

    __slots__ = ("name", "is_dir", "lba", "size", "xar", "multi")

    def __init__(self, name, is_dir, lba, size, xar, multi):
        self.name = name
        self.is_dir = is_dir
        self.lba = lba          # extent 起始扇区
        self.size = size        # 数据字节数
        self.xar = xar          # 扩展属性记录占用的扇区数
        self.multi = multi      # 多段文件（本模块不支持）

    def __repr__(self):
        kind = "dir " if self.is_dir else "file"
        return "<%s %s lba=%d size=%d>" % (kind, self.name, self.lba, self.size)


class Iso9660(object):
    """只读 ISO9660（PVD）访问器。

    支持 `with` 语句；也可以手工 `close()`。
    """

    def __init__(self, path):
        self.path = str(path)
        try:
            self._f = open(self.path, "rb")
        except OSError as e:
            raise IsoError("无法打开 ISO %s: %s" % (self.path, e))
        try:
            self.sec, self.root_lba, self.root_size = self._read_pvd()
        except Exception:
            self._f.close()
            raise
        self._dir_cache = {}

    # ---- 生命周期 ----
    def close(self):
        try:
            self._f.close()
        except Exception:
            pass

    def __enter__(self):
        return self

    def __exit__(self, *exc):
        self.close()
        return False

    # ---- 底层读取 ----
    def read_sectors(self, lba, nsec):
        """按扇区读一段（等价于 `dd bs=<SEC> skip=lba count=nsec`）。"""
        if lba < 0 or nsec <= 0:
            return b""
        self._f.seek(lba * self.sec)
        return self._f.read(nsec * self.sec)

    def read_range(self, lba, size):
        """读从扇区 lba 起的 size 个**字节**。

        扩展属性记录（XAR）会占在数据前面，调用方要自己把 lba 加过去
        —— 见 `_data_lba()`。
        """
        if lba < 0 or size <= 0:
            return b""
        self._f.seek(lba * self.sec)
        return self._f.read(size)

    def _data_lba(self, e):
        """实际数据起始扇区 = extent + 扩展属性长度（XAR 以扇区计）。"""
        return e.lba + e.xar

    # ---- 卷描述符 ----
    def _read_pvd(self):
        for i in range(_VD_SEARCH):
            lba = 16 + i
            self._f.seek(lba * DEFAULT_SEC)
            s = self._f.read(DEFAULT_SEC)
            if len(s) < 7 or s[1:6] != b"CD001":
                # 第一张就不是 CD001 → 不是 ISO9660（或不是从 16 起）
                if i == 0 and (len(s) < 7 or s[1:6] != b"CD001"):
                    continue
                break
            vtype = s[0]
            if vtype == 0xFF:           # 终止符
                break
            if vtype != 1:              # 只要 Primary
                continue
            sec = _u16le(s, 128) or DEFAULT_SEC
            root = s[156:156 + 34]
            if len(root) < 34:
                raise IsoError("PVD 里的根目录记录被截断")
            return sec, _u32le(root, 2), _u32le(root, 10)
        raise IsoError("在 %s 里找不到 ISO9660 Primary Volume Descriptor"
                       "（扇区 16..%d）" % (self.path, 16 + _VD_SEARCH - 1))

    # ---- 目录解析 ----
    def _parse_dir(self, lba, size):
        """解析一段目录内容 -> [_Entry, …]（已去掉 . 与 ..）。"""
        key = (lba, size)
        if key in self._dir_cache:
            return self._dir_cache[key]

        blob = self.read_range(lba, size)
        out = []
        i = 0
        n = len(blob)
        while i < n:
            ln = blob[i]
            if ln == 0:
                # 长度 0 = 本扇区剩余部分是填充，跳到下一个扇区边界
                nxt = (i // self.sec + 1) * self.sec
                if nxt <= i:
                    break
                i = nxt
                continue
            if i + ln > n:
                break
            rec = blob[i:i + ln]
            i += ln
            if len(rec) < 33:
                continue

            xar = rec[1]
            e_lba = _u32le(rec, 2)
            e_size = _u32le(rec, 10)
            flags = rec[25]
            nlen = rec[32]
            raw = rec[33:33 + nlen]
            if len(raw) != nlen:
                continue
            # "." 与 ".."：标识符是单个 0x00 / 0x01 字节
            if nlen == 1 and raw in (b"\x00", b"\x01"):
                continue
            try:
                name = raw.decode("ascii")
            except UnicodeDecodeError:
                continue        # Joliet 的 UTF-16BE 条目，跳过
            name = _strip_version(name)
            if not name:
                continue
            out.append(_Entry(name=name,
                              is_dir=bool(flags & 0x02),
                              lba=e_lba,
                              size=e_size,
                              xar=xar,
                              multi=bool(flags & 0x80)))
        self._dir_cache[key] = out
        return out

    # ---- 路径查找 ----
    @staticmethod
    def _split(inner):
        """把 '/A/B' / 'A/B' 规整成 ['A', 'B']。"""
        if not inner:
            return []
        parts = [p for p in str(inner).replace("\\", "/").split("/") if p and p != "."]
        return parts

    def listdir(self, inner=""):
        """列出某个目录下的条目 -> [_Entry, …]。目录不存在时返回 []。"""
        e = self._lookup(inner)
        if e is None:
            return []
        if not e.is_dir:
            return []
        return self._parse_dir(self._data_lba(e), e.size)

    def _lookup(self, inner):
        """按路径找一条记录；找不到返回 None。空路径 = 根目录。"""
        parts = self._split(inner)
        if not parts:
            return _Entry(name="", is_dir=True, lba=self.root_lba,
                          size=self.root_size, xar=0, multi=False)
        cur = _Entry(name="", is_dir=True, lba=self.root_lba,
                     size=self.root_size, xar=0, multi=False)
        for k, part in enumerate(parts):
            found = None
            for e in self._parse_dir(self._data_lba(cur), cur.size):
                if e.name.upper() == part.upper():
                    found = e
                    break
            if found is None:
                return None
            if k < len(parts) - 1 and not found.is_dir:
                return None
            cur = found
        return cur

    # ---- 对外功能 ----
    def all_paths(self, inner=""):
        """递归列出所有路径（目录与文件），形如 '/AUDIO_TS/ATS_01_0.IFO'。"""
        out = []
        prefix = "/" + "/".join(self._split(inner)) if self._split(inner) else ""

        def rec(e, base):
            for c in self._parse_dir(self._data_lba(e), e.size):
                p = base + "/" + c.name
                out.append(p)
                if c.is_dir:
                    rec(c, p)

        root = self._lookup("")
        rec(root, prefix)
        return out

    def read_file(self, inner):
        """读出一个文件的全部内容 -> bytes。找不到或不是文件时抛 IsoError。"""
        e = self._lookup(inner)
        if e is None:
            raise IsoError("ISO 里没有 %s" % inner)
        if e.is_dir:
            raise IsoError("%s 是目录，不是文件" % inner)
        if e.multi:
            raise IsoError(
                "%s 是多段（multi-extent）文件，本读取器不支持。"
                "ISO 用 level 3 且文件 >= 4 GiB 时才会出现；"
                "本工程的 AOB 每个约 1 GiB，正常不会命中。" % inner)
        return self.read_range(self._data_lba(e), e.size)

    def extract(self, inner, dest):
        """把 ISO 里的路径取到 dest（文件/目录都可以）。

        语义对齐 `xorriso -osirrox on -indev <iso> -extract <inner> <dest>`：
          · inner 是文件 -> 内容写到 dest（dest 是**文件**路径）
          · inner 是目录 -> 在 dest 建目录并递归取内容
        成功返回 True；找不到条目返回 False（其余异常照常抛）。
        """
        e = self._lookup(inner)
        if e is None:
            return False
        dest = str(dest)
        if e.is_dir:
            self._extract_dir(e, dest)
        else:
            parent = os.path.dirname(dest)
            if parent:
                os.makedirs(parent, exist_ok=True)
            blob = self.read_range(self._data_lba(e), e.size)
            with open(dest, "wb") as f:
                f.write(blob)
        return True

    def _extract_dir(self, e, dest):
        os.makedirs(dest, exist_ok=True)
        for c in self._parse_dir(self._data_lba(e), e.size):
            p = os.path.join(dest, c.name)
            if c.is_dir:
                self._extract_dir(c, p)
            else:
                blob = self.read_range(self._data_lba(c), c.size)
                with open(p, "wb") as f:
                    f.write(blob)

    def find(self, basename, inner=""):
        """按**文件名**（不分大小写）找，返回第一条匹配的完整路径或 None。

        校验脚本常常只关心「这几个关键文件在不在」，用这个比
        自己遍历 all_paths() 再取 basename 更省事。
        """
        want = basename.upper()
        for p in self.all_paths(inner):
            if os.path.basename(p).upper() == want:
                return p
        return None


def _strip_version(name):
    """去掉 ISO9660 的版本后缀 `;1`。

    目录名没有后缀；文件名形如 `ATS_01_0.IFO;1`。
    注意：`ATSI;1` 这种没有点的也要处理，所以不能靠 split('.')。
    """
    if ";" in name:
        base, _, ver = name.rpartition(";")
        if ver.isdigit():
            return base
    return name


def open_iso(path):
    """便捷函数：返回 Iso9660 实例（记得 close，或用 with）。"""
    return Iso9660(path)
