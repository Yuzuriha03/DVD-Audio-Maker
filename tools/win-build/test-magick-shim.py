"""Native forwarder integration checks; Python standard library only.

Usage: python test-magick-shim.py BASELINE_MENU_BIN SHIM_EXE OUTPUT_DIRECTORY
"""
from pathlib import Path
import ctypes
from ctypes import wintypes
import hashlib
import json
import os
import shutil
import struct
import subprocess
import sys
import time

baseline, shim, work = (Path(p).resolve() for p in sys.argv[1:])
work.mkdir(parents=True, exist_ok=False)
candidate = work / "tool directory with spaces"
candidate.mkdir()
for p in baseline.glob("*.xml"):
    shutil.copy2(p, candidate / p.name)
shutil.copytree(baseline / "fonts", candidate / "fonts")
shutil.copy2(baseline / "magick.exe", candidate / "magick.exe")
for name in ["convert.exe", "mogrify.exe"]:
    shutil.copy2(shim, candidate / name)
env = {k: v for k, v in os.environ.items()
       if not k.upper().startswith(("MAGICK_", "FONTCONFIG_"))}
report = {"status": "RUNNING", "checks": []}

def check(name, condition):
    if not condition:
        raise AssertionError(name)
    report["checks"].append(name)
    print("PASS " + name, flush=True)

def run(exe, args, data=None):
    return subprocess.run([str(exe), *map(str, args)], input=data,
                          stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                          cwd=work, env=env, timeout=45,
                          creationflags=subprocess.CREATE_NO_WINDOW)

def compare(name, args, data=None):
    old = run(baseline / "convert.exe", args, data)
    new = run(candidate / "convert.exe", args, data)
    check(name, old.returncode == new.returncode == 0 and
          not old.stderr and not new.stderr and old.stdout == new.stdout)

def pixels(path):
    result = run(candidate / "magick.exe", [path, "-depth", "8", "RGBA:-"])
    check("decode " + path.name, result.returncode == 0 and not result.stderr)
    return result.stdout

try:
    pe = shim.read_bytes()
    offset = struct.unpack_from("<I", pe, 0x3c)[0]
    check("forwarder is AMD64", pe[offset:offset+6] == b"PE\0\0\x64\x86")
    compare("binary stdout and image expression quoting",
            ["-size", "91x73", "gradient:#19344a-#eec8a1", "(", "+clone", "-flop", ")",
             "+append", "-resize", "83x47!", "-depth", "8", "RGBA:-"])
    for region in ["SC", "JP", "KR"]:
        compare("regional font and quoted text " + region,
                ["-background", "white", "-fill", "black", "-font", "DVDA-Noto-Sans-CJK-" + region,
                 "-pointsize", "22", "label:中文 日本語 한국어 'single' \"double\" (parentheses)",
                 "-depth", "8", "RGBA:-"])
    for extension in ["png", "jpg", "webp"]:
        source = work / ("cover with spaces." + extension)
        generated = run(baseline / "magick.exe", ["-size", "128x96", "gradient:#204060-#d0b090", source])
        check("generate " + extension, generated.returncode == 0 and not generated.stderr)
        compare(extension + " input and relative path", [source.name, "-resize", "65x49!", "-depth", "8", "RGBA:-"])
        old, new = work / ("old." + extension), work / ("new." + extension)
        shutil.copy2(source, old)
        shutil.copy2(source, new)
        common = ["-resize", "87x63!", "-fill", "#8faabe", "-stroke", "none", "-draw", "rectangle 5,7 31,28"]
        a = run(baseline / "mogrify.exe", [*common, old])
        b = run(candidate / "mogrify.exe", [*common, new])
        check(extension + " in-place mogrify pixels", a.returncode == b.returncode == 0 and
              not a.stderr and not b.stderr and pixels(old) == pixels(new))
    compare("binary stdin forwarding", ["png:-", "-depth", "8", "RGBA:-"],
            (work / "cover with spaces.png").read_bytes())
    a = run(baseline / "convert.exe", ["missing-source.png", "null:"])
    b = run(candidate / "convert.exe", ["missing-source.png", "null:"])
    check("nonzero exit and stderr forwarded", a.returncode == b.returncode != 0 and
          b"missing-source.png" in a.stderr and b"missing-source.png" in b.stderr)
    missing = work / "missing core"
    missing.mkdir()
    shutil.copy2(shim, missing / "convert.exe")
    failed = run(missing / "convert.exe", ["-version"])
    check("missing core fails without PATH fallback", failed.returncode == 125 and
          b"Cannot launch bundled magick.exe" in failed.stderr)

    # Killing only the forwarder must not leave an image-processing child alive.
    class ProcessEntry(ctypes.Structure):
        _fields_ = [("size", wintypes.DWORD), ("usage", wintypes.DWORD),
                    ("pid", wintypes.DWORD), ("heap", ctypes.c_size_t),
                    ("module", wintypes.DWORD), ("threads", wintypes.DWORD),
                    ("parent", wintypes.DWORD), ("priority", wintypes.LONG),
                    ("flags", wintypes.DWORD), ("name", wintypes.WCHAR * 260)]
    kernel = ctypes.WinDLL("kernel32", use_last_error=True)
    kernel.CreateToolhelp32Snapshot.argtypes = [wintypes.DWORD, wintypes.DWORD]
    kernel.CreateToolhelp32Snapshot.restype = wintypes.HANDLE
    kernel.Process32FirstW.argtypes = [wintypes.HANDLE, ctypes.POINTER(ProcessEntry)]
    kernel.Process32NextW.argtypes = [wintypes.HANDLE, ctypes.POINTER(ProcessEntry)]
    kernel.OpenProcess.argtypes = [wintypes.DWORD, wintypes.BOOL, wintypes.DWORD]
    kernel.OpenProcess.restype = wintypes.HANDLE
    kernel.WaitForSingleObject.argtypes = [wintypes.HANDLE, wintypes.DWORD]
    kernel.CloseHandle.argtypes = [wintypes.HANDLE]
    process = subprocess.Popen([str(candidate / "convert.exe"), "-bench", "100000",
                                "-size", "512x512", "plasma:fractal", "null:"],
                               cwd=work, env=env, stdout=subprocess.DEVNULL,
                               stderr=subprocess.DEVNULL, creationflags=subprocess.CREATE_NO_WINDOW)
    child_handle = None
    try:
        deadline = time.monotonic() + 8
        while not child_handle and time.monotonic() < deadline and process.poll() is None:
            snapshot = kernel.CreateToolhelp32Snapshot(2, 0)
            entry = ProcessEntry()
            entry.size = ctypes.sizeof(entry)
            more = kernel.Process32FirstW(snapshot, ctypes.byref(entry))
            while more:
                if entry.parent == process.pid and entry.name.lower() == "magick.exe":
                    child_handle = kernel.OpenProcess(0x100000, False, entry.pid)
                    break
                more = kernel.Process32NextW(snapshot, ctypes.byref(entry))
            kernel.CloseHandle(snapshot)
            if not child_handle:
                time.sleep(0.02)
        check("long-running child observed", bool(child_handle))
        process.kill()
        process.wait(timeout=5)
        check("cancellation kills supervised child", kernel.WaitForSingleObject(child_handle, 5000) == 0)
    finally:
        if process.poll() is None:
            process.kill()
            process.wait(timeout=5)
        if child_handle:
            kernel.CloseHandle(child_handle)
    report["status"] = "PASS"
    report["shim_bytes"] = shim.stat().st_size
    report["shim_sha256"] = hashlib.sha256(shim.read_bytes()).hexdigest()
except Exception as error:
    report["status"] = "FAIL"
    report["error"] = str(error)
    raise
finally:
    (work / "report.json").write_text(json.dumps(report, ensure_ascii=False, indent=2), encoding="utf-8")
    print(json.dumps(report, ensure_ascii=True), flush=True)
