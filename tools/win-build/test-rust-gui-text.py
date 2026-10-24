"""Measure visible Japanese labels/buttons with their actual Win32 font."""
import ctypes as c
from ctypes import wintypes as w
import importlib.util
import json
from pathlib import Path
import subprocess
import tempfile
import time
import sys

spec = importlib.util.spec_from_file_location("scroll", Path(__file__).with_name("test-rust-gui-scroll.py"))
s = importlib.util.module_from_spec(spec)
spec.loader.exec_module(s)
u, gui, g = s.u, s.gui, s.gdi
g.SelectObject.argtypes = [w.HDC, w.HGDIOBJ]
g.SelectObject.restype = w.HGDIOBJ
u.DrawTextW.argtypes = [w.HDC, w.LPCWSTR, c.c_int, c.POINTER(w.RECT), w.UINT]
with tempfile.TemporaryDirectory(prefix="dvda-text-") as directory:
    profile = Path(directory) / "profile.json"
    profile.write_text(json.dumps({"Version": 1, "Language": "ja", "Values": {}}), encoding="utf-8")
    proc = subprocess.Popen([str(Path(sys.argv[1]).resolve()), "--profile", str(profile)])
    hwnd = None
    findings = []
    try:
        hwnd = gui.wait_for(lambda: gui.find_app(proc.pid))
        gui.wait_for(lambda: any(u.GetDlgCtrlID(h) == 1000 for h in gui.windows(hwnd)))
        for width, height in ((800, 800), (960, 564), (960, 800), (1400, 800)):
            u.ShowWindow(hwnd, 9)
            u.SetWindowPos(hwnd, None, 40, 40, width, height, 0x0040)
            for tab in range(3):
                button = next(h for h in gui.windows(hwnd) if u.GetDlgCtrlID(h) == 2000 + tab)
                u.SendMessageW(hwnd, 0x0111, 2000 + tab, button)
                time.sleep(.1)
                visible = []
                for control in gui.windows(hwnd):
                    if not u.IsWindowVisible(control):
                        continue
                    cls = gui.classname(control).lower()
                    if cls not in ("static", "button", "edit", "combobox"):
                        continue
                    bounds = w.RECT()
                    u.GetWindowRect(control, c.byref(bounds))
                    if bounds.right - bounds.left <= 1 or bounds.bottom - bounds.top <= 1:
                        continue
                    parent = u.GetParent(control)
                    if cls in ("static", "button") and not gui.text(control):
                        continue
                    parent_bounds = w.RECT()
                    u.GetWindowRect(parent, c.byref(parent_bounds))
                    # The log EDIT sits in a clipping frame: compare it with the
                    # main window's controls, clipped to the frame.
                    if gui.classname(parent) == "DVD_AUDIO_LOG_FRAME_RUST":
                        parent = u.GetParent(parent)
                    bounds.left = max(bounds.left, parent_bounds.left)
                    bounds.top = max(bounds.top, parent_bounds.top)
                    bounds.right = min(bounds.right, parent_bounds.right)
                    bounds.bottom = min(bounds.bottom, parent_bounds.bottom)
                    if bounds.right <= bounds.left or bounds.bottom <= bounds.top:
                        continue
                    for other, other_parent, other_bounds in visible:
                        if parent != other_parent:
                            continue
                        if (min(bounds.right, other_bounds.right) > max(bounds.left, other_bounds.left)
                                and min(bounds.bottom, other_bounds.bottom) > max(bounds.top, other_bounds.top)):
                            findings.append({"width": width, "height": height, "tab": tab,
                                             "overlap": [u.GetDlgCtrlID(other), u.GetDlgCtrlID(control)],
                                             "text": [gui.text(other), gui.text(control)],
                                             "rects": [[r.left, r.top, r.right, r.bottom] for r in (other_bounds, bounds)]})
                    visible.append((control, parent, bounds))
                for h in gui.windows(hwnd):
                    cls = gui.classname(h)
                    text = gui.text(h)
                    if not u.IsWindowVisible(h) or not text or cls.lower() not in ("static", "button"):
                        continue
                    rect = w.RECT()
                    u.GetClientRect(h, c.byref(rect))
                    if rect.right <= 1 or rect.bottom <= 1:
                        continue
                    dc = u.GetDC(h)
                    font = u.SendMessageW(h, 0x0031, 0, 0)
                    old = g.SelectObject(dc, font) if font else None
                    measured = w.RECT(0, 0, rect.right, 0)
                    style = u.GetWindowLongPtrW(h, -16)
                    wrap = cls.lower() == "static" and style & 31 in (0, 1, 2)
                    flags = 0x400 | 0x800 | (0x10 if wrap else 0x20)
                    u.DrawTextW(dc, text, -1, c.byref(measured), flags)
                    if old:
                        g.SelectObject(dc, old)
                    u.ReleaseDC(h, dc)
                    if measured.right > rect.right or measured.bottom > rect.bottom:
                        finding = {"width": width, "tab": tab, "id": u.GetDlgCtrlID(h), "class": cls, "text": text,
                                   "available": [rect.right, rect.bottom], "required": [measured.right, measured.bottom]}
                        if finding not in findings:
                            findings.append(finding)
    finally:
        if hwnd:
            u.PostMessageW(hwnd, 0x0010, 0, 0)
        try:
            proc.wait(timeout=10)
        except subprocess.TimeoutExpired:
            proc.kill()
            proc.wait()
    print(json.dumps(findings, ensure_ascii=False, indent=2))
    sys.exit(bool(findings))
