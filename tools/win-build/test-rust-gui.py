"""Exercise the real x64 Rust window with isolated JSON profiles (no disc build)."""
import argparse
import ctypes as c
from ctypes import wintypes as w
import json
import os
from pathlib import Path
import subprocess
import tempfile
import time

u = c.WinDLL("user32", use_last_error=True)
u.SetProcessDPIAware()
CALLBACK = c.WINFUNCTYPE(w.BOOL, w.HWND, w.LPARAM)
u.EnumWindows.argtypes = [CALLBACK, w.LPARAM]
u.EnumChildWindows.argtypes = [w.HWND, CALLBACK, w.LPARAM]
u.GetWindowThreadProcessId.argtypes = [w.HWND, c.POINTER(w.DWORD)]
u.GetClassNameW.argtypes = [w.HWND, w.LPWSTR, c.c_int]
u.GetDlgCtrlID.argtypes = [w.HWND]
u.SendMessageW.argtypes = [w.HWND, w.UINT, w.WPARAM, w.LPARAM]
u.SendMessageW.restype = w.LPARAM
u.PostMessageW.argtypes = [w.HWND, w.UINT, w.WPARAM, w.LPARAM]
u.GetWindowRect.argtypes = [w.HWND, c.POINTER(w.RECT)]
u.ScreenToClient.argtypes = [w.HWND, c.POINTER(w.POINT)]
u.IsWindowEnabled.argtypes = [w.HWND]
u.IsWindowVisible.argtypes = [w.HWND]
u.UpdateWindow.argtypes = [w.HWND]
u.ShowWindow.argtypes = [w.HWND, c.c_int]
u.SetWindowPos.argtypes = [w.HWND, w.HWND, c.c_int, c.c_int, c.c_int, c.c_int, w.UINT]


def windows(parent=None):
    found = []
    callback = CALLBACK(lambda hwnd, _: found.append(hwnd) or True)
    (u.EnumChildWindows(parent, callback, 0) if parent else u.EnumWindows(callback, 0))
    return found


def classname(hwnd):
    buf = c.create_unicode_buffer(256)
    u.GetClassNameW(hwnd, buf, len(buf))
    return buf.value


def text(hwnd):
    length = u.SendMessageW(hwnd, 0x000E, 0, 0)
    buf = c.create_unicode_buffer(length + 1)
    u.SendMessageW(hwnd, 0x000D, len(buf), c.addressof(buf))
    return buf.value


def set_text(hwnd, value):
    buf = c.create_unicode_buffer(value)
    u.SendMessageW(hwnd, 0x000C, 0, c.addressof(buf))


def wait_for(condition, seconds=8):
    deadline = time.monotonic() + seconds
    while time.monotonic() < deadline:
        value = condition()
        if value:
            return value
        time.sleep(0.05)
    raise AssertionError("GUI condition timed out")


def find_app(pid):
    for hwnd in windows():
        owner = w.DWORD()
        u.GetWindowThreadProcessId(hwnd, c.byref(owner))
        if owner.value == pid and classname(hwnd) == "DVD_AUDIO_MAKER_RUST":
            return hwnd


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--exe", type=Path, required=True)
    parser.add_argument("--output", type=Path, default=Path("build/gui-audit"))
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=True)
    expected = {
        "zh-CN": (0, "检查音源", "开始制作", "停止任务", "只看提醒", "实时更新", "方案已保存："),
        "en": (1, "Check sources", "Build discs", "Stop task", "Problems only", "Live update", "Profile saved: "),
        "ja": (2, "音源を検査", "ディスクを作成", "タスクを停止", "問題のみ", "ライブ更新", "プロファイルを保存しました: "),
    }
    records = []
    with tempfile.TemporaryDirectory(prefix="dvda-gui-") as temporary:
        profile = Path(temporary) / "settings.json"
        source = str(Path(temporary) / "音源日本語-{0}-album")
        baseline = {"Version": 1, "Language": "zh-CN", "Values": {
            "DVDA_SRC": source, "DVDA_BUILD_DIR": str(Path(temporary) / "work"),
            "DVDA_MLP_SOURCE": "external", "DVDA_MLP_EXTERNAL_DIR": str(Path(temporary) / "mlp"),
            "DVDA_DISC_BYTES": "4000000000", "CUSTOM_PLUGIN_SETTING": "keep-me"}}
        profile.write_text(json.dumps(baseline, ensure_ascii=False), encoding="utf-8-sig")
        for launch in range(2):
            proc = subprocess.Popen([str(args.exe.resolve())] + ([] if launch else ["--profile", str(profile)]),
                env=os.environ | {"LOCALAPPDATA":temporary}, creationflags=subprocess.CREATE_NO_WINDOW)
            hwnd = None
            try:
                hwnd = wait_for(lambda: find_app(proc.pid))
                wait_for(lambda: any(u.GetDlgCtrlID(h) == 115 for h in windows(hwnd)))
                wait_for(lambda: any(u.GetDlgCtrlID(h) == 200 and text(h) == source for h in windows(hwnd)))
                children = windows(hwnd)
                controls = {u.GetDlgCtrlID(h): h for h in children if u.GetDlgCtrlID(h) > 0}
                if 200 not in controls:
                    print(json.dumps([{"id": u.GetDlgCtrlID(h), "class": classname(h), "text": text(h)} for h in children], ensure_ascii=False))
                    raise AssertionError("Source field was not created")
                assert 204 not in controls, "Planned-disc input should remain removed"
                assert 303 not in controls, "MLP worker-count input should remain removed"
                assert text(controls[200]) == source
                assert u.SendMessageW(controls[300], 0x0147, 0, 0) == 2, "MLP import lost on load"
                assert u.SendMessageW(controls[203], 0x0147, 0, 0) == 2, "Custom capacity lost on load"
                assert text(controls[210]) == "4000000000"
                assert u.IsWindowEnabled(controls[210])
                assert not u.IsWindowEnabled(controls[301]), "Import must not resample"
                assert u.SendMessageW(controls[104], 0x1304, 0, 0) == 4
                assert u.SendMessageW(controls[103], 0x0147, 0, 0) == (0 if launch == 0 else 2), "Language was not restored"
                if launch:
                    break
                # DVD9 uses the original safe preset; selecting it must not
                # silently add sectors. Restore the custom capacity afterwards.
                u.SendMessageW(controls[203], 0x014E, 1, 0)
                u.SendMessageW(hwnd, 0x0111, 203 | (1 << 16), controls[203])
                u.SendMessageW(hwnd, 0x0111, 102, controls[102])
                assert json.loads(profile.read_text(encoding="utf-8-sig"))["Values"]["DVDA_DISC_BYTES"] == "8540123136"
                u.SendMessageW(controls[203], 0x014E, 2, 0)
                u.SendMessageW(hwnd, 0x0111, 203 | (1 << 16), controls[203])
                set_text(controls[210], "4000000000")
                # Detailed logs always include informational lines even when
                # the summary's Problems only toggle remains checked.
                u.SendMessageW(controls[111], 0x00F1, 1, 0)
                u.SendMessageW(hwnd, 0x0111, 111, controls[111])
                assert "方案已保存" not in text(controls[115])
                u.SendMessageW(controls[110], 0x014E, 1, 0)
                u.SendMessageW(hwnd, 0x0111, 110 | (1 << 16), controls[110])
                assert not u.IsWindowEnabled(controls[111])
                assert "方案已保存" in text(controls[115])
                u.SendMessageW(controls[110], 0x014E, 0, 0)
                u.SendMessageW(hwnd, 0x0111, 110 | (1 << 16), controls[110])
                assert u.IsWindowEnabled(controls[111])
                u.SendMessageW(controls[111], 0x00F1, 0, 0)
                u.SendMessageW(hwnd, 0x0111, 111, controls[111])
                u.ShowWindow(hwnd, 9)
                wait_for(lambda:u.IsWindowVisible(controls[200]))
                assert not u.IsWindowVisible(controls[207]), "Advanced settings should initially fold"
                u.SendMessageW(controls[117], 0x00F1, 1, 0)
                u.SendMessageW(hwnd, 0x0111, 117, controls[117])
                assert u.IsWindowVisible(controls[207])
                u.ShowWindow(hwnd, 9)
                u.SetWindowPos(hwnd, None, 80, 50, 1180, 820, 0x0014)
                # Synthetic mouse coordinates use the target window's logical
                # pixels, which may differ from this screenshot process's DPI.
                u.GetWindowDpiAwarenessContext.argtypes = [w.HWND]
                u.GetWindowDpiAwarenessContext.restype = c.c_void_p
                u.SetThreadDpiAwarenessContext.argtypes = [c.c_void_p]
                u.SetThreadDpiAwarenessContext.restype = c.c_void_p
                old_dpi = u.SetThreadDpiAwarenessContext(u.GetWindowDpiAwarenessContext(hwnd))
                initial = w.RECT()
                u.GetWindowRect(controls[104], c.byref(initial))
                point = w.POINT(initial.left + 20, initial.bottom + 4)
                u.ScreenToClient(hwnd, c.byref(point))
                packed = (point.y << 16) | point.x
                u.SendMessageW(hwnd, 0x0201, 1, packed)
                u.SendMessageW(hwnd, 0x0200, 1, ((point.y - 50) << 16) | point.x)
                u.SendMessageW(hwnd, 0x0202, 0, ((point.y - 50) << 16) | point.x)
                resized = w.RECT()
                u.GetWindowRect(controls[104], c.byref(resized))
                u.SetThreadDpiAwarenessContext(old_dpi)
                assert resized.bottom < initial.bottom - 20, ("Settings/log splitter cannot be dragged", initial.bottom, resized.bottom, point.y)
                pages = [h for h in children if classname(h) == "DVD_AUDIO_PAGE_RUST"]
                assert len(pages) == 4
                for language, (index, check, build, stop, issues, live, saved) in expected.items():
                    u.SendMessageW(controls[103], 0x014E, index, 0)
                    u.SendMessageW(hwnd, 0x0111, 103 | (1 << 16), controls[103])
                    assert [text(controls[i]) for i in (105, 106, 108, 111, 112)] == [check, build, stop, issues, live]
                    assert text(controls[200]) == source, "Language switch modified source data"
                    daily = Path(temporary) / "DVD-Audio-Maker/settings.json"
                    wait_for(lambda: daily.is_file())
                    automatic = json.loads(daily.read_text(encoding="utf-8-sig"))
                    assert automatic["Language"] == language
                    assert automatic["Values"]["DVDA_SRC"] == source
                    u.SendMessageW(hwnd, 0x0111, 102, controls[102])
                    wait_for(lambda: saved in text(controls[115]))
                    stored = json.loads(profile.read_text(encoding="utf-8-sig"))
                    assert stored["Language"] == language
                    assert stored["Values"]["DVDA_MLP_SOURCE"] == "external"
                    assert stored["Values"]["DVDA_DISC_BYTES"] == "4000000000"
                    assert stored["Values"]["CUSTOM_PLUGIN_SETTING"] == "keep-me"
                    texts = [{"id": u.GetDlgCtrlID(h), "class": classname(h), "text": text(h)} for h in children]
                    records.append({"language": language, "controls": texts})
                    from PIL import ImageGrab
                    rect = w.RECT()
                    u.GetWindowRect(hwnd, c.byref(rect))
                    u.UpdateWindow(hwnd)
                    time.sleep(0.2)
                    records[-1]["window_bounds"] = [rect.left, rect.top, rect.right, rect.bottom]
                    ImageGrab.grab(bbox=(rect.left, rect.top, rect.right, rect.bottom)).save(args.output / f"gui-{language}.png")
                    for page_index, field in enumerate((200, 300, 400, 500)):
                        u.SendMessageW(controls[104], 0x1330, page_index, 0)
                        wait_for(lambda: u.IsWindowVisible(controls[field]))
                        assert sum(bool(u.IsWindowVisible(page)) for page in pages) == 1
                        u.UpdateWindow(hwnd)
                        time.sleep(0.1)
                        ImageGrab.grab(bbox=(rect.left, rect.top, rect.right, rect.bottom)).save(args.output / f"gui-{language}-tab{page_index}.png")
                    u.SendMessageW(controls[104], 0x1330, 0, 0)
                u.SetWindowPos(hwnd, None, 80, 50, 1060, 560, 0x0014)
                u.SendMessageW(pages[0], 0x0115, 7, 0)
                field_rect=w.RECT();page_rect=w.RECT()
                u.GetWindowRect(controls[210],c.byref(field_rect));u.GetWindowRect(pages[0],c.byref(page_rect))
                assert field_rect.bottom <= page_rect.bottom, "Lower settings unreachable after scrolling"
                u.SetWindowPos(hwnd, None, 80, 50, 1180, 820, 0x0014)
                # File dialogs are real Win32 dialogs; cancellation must preserve the profile.
                for action in (116,1007):
                    old=text(controls[100])
                    u.PostMessageW(hwnd,0x0111,action,controls[action])
                    def dialog():
                        for h in windows():
                            pid=w.DWORD();u.GetWindowThreadProcessId(h,c.byref(pid))
                            if pid.value==proc.pid and classname(h)=="#32770": return h
                    handle=wait_for(dialog)
                    u.PostMessageW(handle,0x0111,2,0)
                    wait_for(lambda:not dialog())
                    assert text(controls[100])==old
                u.PostMessageW(hwnd,0x0111,116,controls[116])
                handle=wait_for(dialog)
                u.SetForegroundWindow.argtypes=[w.HWND]
                u.SetForegroundWindow(handle)
                u.keybd_event(0x12,0,0,0);u.keybd_event(0x4e,0,0,0)
                u.keybd_event(0x4e,0,2,0);u.keybd_event(0x12,0,2,0)
                class GuiThreadInfo(c.Structure):
                    _fields_=[("size",w.DWORD),("flags",w.DWORD)]+[(n,w.HWND) for n in ("active","focus","capture","menu","move","caret")]+[("rect",w.RECT)]
                u.GetGUIThreadInfo.argtypes=[w.DWORD,c.POINTER(GuiThreadInfo)]
                def focused_filename():
                    info=GuiThreadInfo();info.size=c.sizeof(info)
                    u.GetGUIThreadInfo(u.GetWindowThreadProcessId(handle,None),c.byref(info))
                    return info.focus if info.focus and classname(info.focus)=="Edit" else None
                filename=wait_for(focused_filename)
                alternate=Path(temporary)/"日本語-另存.json"
                set_text(filename,str(alternate))
                u.PostMessageW(handle,0x0111,1,0)
                wait_for(lambda:alternate.is_file())
                assert json.loads(alternate.read_text(encoding="utf-8-sig"))["Values"]["CUSTOM_PLUGIN_SETTING"]=="keep-me"
                assert text(controls[100])==str(alternate)
            finally:
                if hwnd:
                    u.PostMessageW(hwnd, 0x0010, 0, 0)
                try:
                    proc.wait(timeout=8)
                except subprocess.TimeoutExpired:
                    proc.terminate()
                    proc.wait(timeout=5)
                daily = Path(temporary) / "DVD-Audio-Maker/settings.json"
                assert json.loads(daily.read_text(encoding="utf-8-sig"))["Language"] == "ja"
                assert list((Path(temporary) / "DVD-Audio-Maker/logs").glob("gui-*.log"))
    (args.output / "controls.json").write_text(json.dumps(records, ensure_ascii=False, indent=2), encoding="utf-8")
    print(json.dumps({"passed": True, "languages": list(expected), "profile_reload": True, "mlp_import": True, "custom_capacity": True, "screenshots": str(args.output)}))


if __name__ == "__main__":
    main()
