"""Native settings scrolling/layout regression; no audio jobs or touch injection."""
import argparse
import ctypes as c
from ctypes import wintypes as w
import importlib.util
import json
from pathlib import Path
import subprocess
import tempfile
import time
import threading

spec = importlib.util.spec_from_file_location("gui", Path(__file__).with_name("test-rust-gui.py"))
gui = importlib.util.module_from_spec(spec)
spec.loader.exec_module(gui)
u = gui.u
u.GetDC.argtypes = [w.HWND]
u.GetDC.restype = w.HDC
u.ReleaseDC.argtypes = [w.HWND, w.HDC]
u.RedrawWindow.argtypes = [w.HWND, c.c_void_p, c.c_void_p, w.UINT]
u.GetClientRect.argtypes = [w.HWND, c.POINTER(w.RECT)]
u.GetWindowLongPtrW.argtypes = [w.HWND, c.c_int]
u.GetWindowLongPtrW.restype = c.c_ssize_t
gdi = c.WinDLL("gdi32")
gdi.GetPixel.argtypes = [w.HDC, c.c_int, c.c_int]
gdi.GetPixel.restype = w.DWORD
gdi.CreateCompatibleDC.argtypes = [w.HDC]
gdi.CreateCompatibleDC.restype = w.HDC
gdi.CreateDIBSection.argtypes = [w.HDC, c.c_void_p, w.UINT, c.POINTER(c.c_void_p), w.HANDLE, w.DWORD]
gdi.CreateDIBSection.restype = w.HANDLE
gdi.SelectObject.argtypes = [w.HDC, w.HANDLE]
gdi.SelectObject.restype = w.HANDLE
gdi.BitBlt.argtypes = [w.HDC, c.c_int, c.c_int, c.c_int, c.c_int, w.HDC, c.c_int, c.c_int, w.DWORD]
gdi.DeleteObject.argtypes = [w.HANDLE]
gdi.DeleteDC.argtypes = [w.HDC]


class BitmapHeader(c.Structure):
    _fields_ = [("size", w.DWORD), ("width", w.LONG), ("height", w.LONG),
                ("planes", w.WORD), ("bits", w.WORD), ("compression", w.DWORD),
                ("image_size", w.DWORD), ("xppm", w.LONG), ("yppm", w.LONG),
                ("used", w.DWORD), ("important", w.DWORD)]


def edge_pixels(hwnd):
    rect = w.RECT()
    u.GetClientRect(hwnd, c.byref(rect))
    dc = u.GetDC(hwnd)
    try:
        return [gdi.GetPixel(dc, x, y) for x in (2, 5, rect.right - 5)
                for y in range(2, rect.bottom - 2)]
    finally:
        u.ReleaseDC(hwnd, dc)


def watch_language(hwnd, stop, samples):
    u.GetWindowDpiAwarenessContext.argtypes = [w.HWND]
    u.GetWindowDpiAwarenessContext.restype = c.c_void_p
    u.SetThreadDpiAwarenessContext.argtypes = [c.c_void_p]
    u.SetThreadDpiAwarenessContext.restype = c.c_void_p
    old_dpi = u.SetThreadDpiAwarenessContext(c.c_void_p(-4))
    bounds = w.RECT()
    u.GetWindowRect(hwnd, c.byref(bounds))
    u.WindowFromPoint.argtypes = [w.POINT]
    u.WindowFromPoint.restype = w.HWND
    u.GetAncestor.argtypes = [w.HWND, w.UINT]
    u.GetAncestor.restype = w.HWND
    root = u.GetAncestor(hwnd, 2)
    width, height = bounds.right - bounds.left, bounds.bottom - bounds.top
    header = BitmapHeader(size=c.sizeof(BitmapHeader), width=width, height=-height,
                          planes=1, bits=32)
    dc = u.GetDC(None)
    memory = gdi.CreateCompatibleDC(dc)
    pixels = c.c_void_p()
    bitmap = gdi.CreateDIBSection(dc, c.byref(header), 0, c.byref(pixels), None, 0)
    previous = gdi.SelectObject(memory, bitmap)
    try:
        assert bitmap and pixels.value, "cannot create language capture bitmap"
        while not stop.is_set():
            u.GetWindowRect(hwnd, c.byref(bounds))
            # Overlays from other programs can cover part of a control.
            points = [w.POINT(bounds.left + dx, bounds.top + height // 2) for dx in (1, width // 2, width - 2)]
            covering = next((h for h in map(u.WindowFromPoint, points) if u.GetAncestor(h, 2) != root), root)
            if covering != root:
                samples.append(("occluded", bounds.left, bounds.top, gui.classname(covering), gui.text(covering),
                                "target", hwnd, "root", root, "visible", u.IsWindowVisible(root),
                                "topmost", bool(u.GetWindowLongPtrW(root, -20) & 8)))
                return
            assert gdi.BitBlt(memory, 0, 0, width, height, dc, bounds.left, bounds.top, 0x00CC0020)
            gdi.GdiFlush()
            pixels_copy = c.string_at(pixels, width * height * 4)
            samples.append((width, height, pixels_copy))
            time.sleep(.005)
    except Exception as error:
        samples.append(("capture error", repr(error)))
    finally:
        gdi.SelectObject(memory, previous)
        gdi.DeleteObject(bitmap)
        gdi.DeleteDC(memory)
        u.ReleaseDC(None, dc)
        u.SetThreadDpiAwarenessContext(old_dpi)


def check_samples(samples, context):
    errors = [sample for sample in samples if not isinstance(sample[0], int)]
    assert not errors, (context, errors)
    assert len(samples) > 1, (context, "no paint samples")
    unique = list(dict.fromkeys(samples))
    if len(unique) != 1:
        import struct
        output = Path(tempfile.mkdtemp(prefix="dvda-paint-failure-"))
        for index, (width, height, pixels) in enumerate(unique[:20]):
            header = struct.pack("<IiiHHIIiiII", 40, width, -height, 1, 32, 0, len(pixels), 0, 0, 0, 0)
            (output / f"frame-{index}.bmp").write_bytes(
                struct.pack("<2sIHHI", b"BM", 54 + len(pixels), 0, 0, 54) + header + pixels)
        raise AssertionError((context, "paint changed", len(unique), str(output)))


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--exe", type=Path, required=True)
    parser.add_argument("--large-settings", action="store_true",
                        help="Drag the splitter down and test a taller, still-scrollable settings pane")
    parser.add_argument("--resize-edges", action="store_true",
                        help="Physically drag right/bottom window edges and sample controls that stay put")
    args = parser.parse_args()
    checked = 0
    with tempfile.TemporaryDirectory(prefix="dvda-scroll-") as directory:
        for language in ("zh-CN", "en", "ja"):
            profile = Path(directory) / "profile.json"
            profile.write_text(json.dumps({"Version": 1, "Language": language, "Values": {}}), encoding="utf-8")
            proc = subprocess.Popen([str(args.exe.resolve()), "--profile", str(profile)],
                                    creationflags=subprocess.CREATE_NO_WINDOW)
            hwnd = None
            try:
                hwnd = gui.wait_for(lambda: gui.find_app(proc.pid))
                gui.wait_for(lambda: any(u.GetDlgCtrlID(h) == 2000 for h in gui.windows(hwnd)))
                gui.wait_for(lambda: any(u.GetDlgCtrlID(h) == 1000 for h in gui.windows(hwnd)))
                if args.resize_edges:
                    u.SetThreadDpiAwarenessContext.argtypes = [c.c_void_p]
                    u.SetThreadDpiAwarenessContext.restype = c.c_void_p
                    u.GetWindowDpiAwarenessContext.argtypes = [w.HWND]
                    u.GetWindowDpiAwarenessContext.restype = c.c_void_p
                    old_dpi = u.SetThreadDpiAwarenessContext(u.GetWindowDpiAwarenessContext(hwnd))
                    try:
                        time.sleep(.8)
                        u.SendMessageW(hwnd, 0x0112, 0xF120, 0)
                        time.sleep(.2)
                        assert not u.IsZoomed(hwnd), "window did not restore before drag"
                        # Size in DIPs so high-DPI screens do not start in the
                        # compact layout, where page content is scrolled away, and
                        # leave room for growing 120 px right and down.
                        u.GetDpiForWindow.argtypes = [w.HWND]
                        dpi = u.GetDpiForWindow(hwnd)
                        assert u.SetWindowPos(hwnd, w.HWND(-1), 160, 140, 1000 * dpi // 96, 560 * dpi // 96, 0x0040)
                        # The foreground lock keeps a new window behind the active
                        # application, so the drag would go there; an Alt press lets
                        # SetForegroundWindow succeed.
                        u.WindowFromPoint.argtypes = [w.POINT]
                        u.WindowFromPoint.restype = w.HWND
                        u.GetAncestor.argtypes = [w.HWND, w.UINT]
                        u.GetAncestor.restype = w.HWND
                        for _ in range(10):
                            u.keybd_event(0x12, 0, 0, 0)
                            u.keybd_event(0x12, 0, 2, 0)
                            u.SetForegroundWindow(hwnd)
                            time.sleep(.3)
                            probe = w.RECT()
                            u.GetWindowRect(hwnd, c.byref(probe))
                            if u.GetAncestor(u.WindowFromPoint(w.POINT(probe.left + 100, probe.top + 300)), 2) == hwnd:
                                break
                        else:
                            raise AssertionError("app window stays behind another window")
                        # The Alt press reveals keyboard focus cues; repaint now so the
                        # reference frames already show them.
                        u.RedrawWindow.argtypes = [w.HWND, c.c_void_p, c.c_void_p, w.UINT]
                        u.RedrawWindow(hwnd, None, None, 0x0001 | 0x0080 | 0x0100)
                        time.sleep(.5)
                        controls = {u.GetDlgCtrlID(h): h for h in gui.windows(hwnd)}
                        # Visible controls that stay put while dragging each edge:
                        # tab, log combo and filters, profile/source fields, log and
                        # toolbar/action buttons. Captures keep the initial size, so
                        # fields that stretch are compared by their unchanged left part.
                        stationary = {"right": (2000, 110, 111, 106, 100, 200, 115),
                                      "bottom": (2000, 200, 1000, 100, 101, 117)}
                        # Left/top drags are not compared: DWM can present the moved
                        # window a frame before its repaint, so a few shifted frames
                        # remain even when the app paints correctly.
                        for edge in ("right", "bottom"):
                            before = w.RECT()
                            u.GetWindowRect(hwnd, c.byref(before))
                            client = w.RECT()
                            u.GetClientRect(hwnd, c.byref(client))
                            origin = w.POINT(0, 0)
                            u.ClientToScreen(hwnd, c.byref(origin))
                            x = origin.x + client.right - 2 if edge == "right" else origin.x + 400
                            y = origin.y + 350 if edge == "right" else origin.y + client.bottom - 2
                            u.SetCursorPos(x, y)
                            time.sleep(.3)
                            stop = threading.Event()
                            targets = {cid: [] for cid in stationary[edge]}
                            watchers = [threading.Thread(target=watch_language, args=(controls[cid], stop, frames))
                                        for cid, frames in targets.items()]
                            for watcher in watchers:
                                watcher.start()
                            try:
                                u.SetCursorPos(x, y)
                                time.sleep(.15)
                                print("edge input", edge, x, y, "zoomed", u.IsZoomed(hwnd), "client", client.right, client.bottom, "hit", u.SendMessageW(hwnd, 0x84, 0, (y << 16) | (x & 65535)), flush=True)
                                u.mouse_event(2, 0, 0, 0, 0)
                                time.sleep(.15)
                                for delta in list(range(0, 121, 3)) + list(range(120, -1, -3)):
                                    u.SetCursorPos(x + (delta if edge == "right" else 0),
                                                   y + (delta if edge == "bottom" else 0))
                                    time.sleep(.016)
                                    if delta == 120:
                                        peak = w.RECT()
                                        u.GetWindowRect(hwnd, c.byref(peak))
                                        assert (peak.right > before.right + 60 if edge == "right"
                                                else peak.bottom > before.bottom + 60), "native drag did not resize"
                            finally:
                                u.mouse_event(4, 0, 0, 0, 0)
                                time.sleep(.1)
                                stop.set()
                                for watcher in watchers:
                                    watcher.join()
                            for cid, frames in targets.items():
                                check_samples(frames, (language, edge, cid, "stationary control during native drag"))
                                checked += len(frames)
                            print("PASS native edge drag", language, edge, sum(map(len, targets.values())), flush=True)
                    finally:
                        u.SetThreadDpiAwarenessContext(old_dpi)
                    continue
                sizes = ((960, 1050), (1180, 1050), (1400, 1050)) if args.large_settings else (
                    (800, 800), (960, 564), (960, 800), (1400, 800))
                for width, height in sizes:
                    u.ShowWindow(hwnd, 9)
                    assert u.SetWindowPos(hwnd, w.HWND(-1), 40, 20, width, height, 0x0040), c.get_last_error()
                    u.SetForegroundWindow.argtypes = [w.HWND]
                    u.SetForegroundWindow(hwnd)
                    time.sleep(.5)
                    assert u.SetWindowPos(hwnd, w.HWND(-1), 0, 0, 0, 0, 0x0013), c.get_last_error()
                    time.sleep(.2)
                    if args.large_settings:
                        u.GetWindowDpiAwarenessContext.argtypes = [w.HWND]
                        u.GetWindowDpiAwarenessContext.restype = c.c_void_p
                        u.SetThreadDpiAwarenessContext.argtypes = [c.c_void_p]
                        u.SetThreadDpiAwarenessContext.restype = c.c_void_p
                        old_dpi = u.SetThreadDpiAwarenessContext(u.GetWindowDpiAwarenessContext(hwnd))
                        try:
                            panel = next(h for h in gui.windows(hwnd)
                                         if gui.classname(h) == "DVD_AUDIO_PAGE_RUST" and u.IsWindowVisible(h))
                            before = w.RECT()
                            u.GetWindowRect(panel, c.byref(before))
                            client = w.RECT()
                            u.GetClientRect(hwnd, c.byref(client))
                            print("geometry", width, height, "client", client.right, client.bottom,
                                  "page", before.left, before.top, before.right, before.bottom, flush=True)
                            point = w.POINT(before.left + 20, before.bottom + 4)
                            u.ScreenToClient(hwnd, c.byref(point))
                            label = next(h for h in gui.windows(panel)
                                         if gui.classname(h) == "Static" and u.IsWindowVisible(h)
                                         and gui.text(h))
                            u.SetCursorPos(before.left + 20, before.bottom + 4)
                            time.sleep(.2)
                            targets = [label] + [h for h in gui.windows(hwnd)
                                if gui.classname(h) == "Button" and u.IsWindowVisible(h)
                                and u.GetDlgCtrlID(h) in (101, 102, 116)]
                            recordings = [(target, []) for target in targets]
                            stop = threading.Event()
                            watchers = [threading.Thread(target=watch_language, args=(target, stop, frames))
                                        for target, frames in recordings]
                            for watcher in watchers:
                                watcher.start()
                            try:
                                u.SendMessageW(hwnd, 0x0201, 1, (point.y << 16) | point.x)
                                for offset in range(0, 151, 5):
                                    u.SetCursorPos(before.left + 20, before.bottom + 4 + offset)
                                    u.SendMessageW(hwnd, 0x0200, 1, ((point.y + offset) << 16) | point.x)
                                    time.sleep(.01)
                                u.SendMessageW(hwnd, 0x0202, 0, ((point.y + 150) << 16) | point.x)
                            finally:
                                stop.set()
                                for watcher in watchers:
                                    watcher.join()
                            for target, frames in recordings:
                                check_samples(frames, (language, width, "splitter", gui.text(target)))
                            after = w.RECT()
                            u.GetWindowRect(panel, c.byref(after))
                            assert after.bottom >= before.bottom, "settings pane shrank while dragging down"
                            assert after.bottom - after.top >= 400, ("settings pane too short", after.bottom - after.top)
                            print(language, width, "settings height:", after.bottom - after.top, flush=True)
                        finally:
                            u.SetThreadDpiAwarenessContext(old_dpi)
                    for tab in range(3):
                        children = gui.windows(hwnd)
                        button = next(h for h in children if u.GetDlgCtrlID(h) == 2000 + tab)
                        u.SendMessageW(hwnd, 0x0111, 2000 + tab, button)
                        page = gui.wait_for(lambda: next((h for h in gui.windows(hwnd)
                            if gui.classname(h) == "DVD_AUDIO_PAGE_RUST" and u.IsWindowVisible(h)), None))
                        language_control = next(h for h in children if u.GetDlgCtrlID(h) == 103)
                        time.sleep(.2)
                        samples = []
                        stop = threading.Event()
                        watcher = threading.Thread(target=watch_language, args=(language_control, stop, samples))
                        watcher.start()
                        try:
                            for step in range(40):
                                u.SendMessageW(page, 0x0115, step % 2, 0)
                                time.sleep(.005)
                        finally:
                            stop.set()
                            watcher.join()
                        check_samples(samples, (language, width, tab, "language field during scrolling"))
                        for code in (7, 6, 7, 6, 7, 6):
                            u.SendMessageW(page, 0x0115, code, 0)
                            u.UpdateWindow(page)
                            time.sleep(0.02)
                            before = edge_pixels(page)
                            u.RedrawWindow(page, None, None, 0x0185)
                            after = edge_pixels(page)
                            assert before == after, (language, width, tab, code, "stale border pixels")
                            if code == 6:
                                assert u.GetWindowLongPtrW(page, -21) == 0
                            checked += 1
                        for bar in (h for h in children if gui.classname(h) == "DVD_AUDIO_SCROLLBAR_RUST"):
                            rect = w.RECT()
                            u.GetWindowRect(bar, c.byref(rect))
                            u.GetDpiForWindow.argtypes = [w.HWND]
                            expected = round(10 * u.GetDpiForWindow(hwnd) / 96)
                            assert abs(rect.right - rect.left - expected) <= 1
            finally:
                if hwnd:
                    u.PostMessageW(hwnd, 0x0010, 0, 0)
                try:
                    proc.wait(timeout=8)
                except subprocess.TimeoutExpired:
                    proc.kill()
                    proc.wait()
            assert proc.returncode == 0
    print(f"PASS: {checked} scroll/repaint checks, 3 languages x 4 window sizes x 3 tabs; sampled language field stable during scrolling; 10-DIP scrollbars; clean exits")


if __name__ == "__main__":
    main()
